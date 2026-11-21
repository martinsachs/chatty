//! Mattermost (REST v4 + WebSocket).

mod api;
mod emoji;
mod emoji_table;
mod ws;

#[cfg(test)]
mod it;

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use async_trait::async_trait;
use reqwest::{Client, Method, RequestBuilder, Response, StatusCode};
use serde::de::DeserializeOwned;
use serde_json::json;
use tokio::sync::watch;

use crate::backend::{Avatar, ChatBackend, EventSender};
use crate::error::{Error, Result};
use crate::model::{
    Account, Attachment, BackendKind, Message, Page, Space, SpaceKind, UploadFile, User,
    count_replies,
};
use crate::secrets;
use api::{
    ApiChannel, ApiMember, ApiPost, ApiTeam, ApiUser, Me, PostList, Preference, UploadResponse,
};

const PAGE_SIZE: usize = 50;
/// Gesamt-Timeout für REST-Aufrufe ohne Dateien.
const API_TIMEOUT: Duration = Duration::from_secs(30);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

pub struct MattermostBackend {
    account: Account,
    /// Basis-URL ohne abschließenden Schrägstrich.
    server: String,
    token: String,
    /// Nur HTTP/1.1, damit derselbe Client auch das WebSocket-Upgrade kann.
    http: Client,
    me: Me,
    users: Mutex<HashMap<String, ApiUser>>,
    /// Teams aus dem letzten `list_spaces` (für Präfixe bei Einzel-Aktualisierungen).
    teams: Mutex<Vec<ApiTeam>>,
    /// DM-Partner → DM-Kanal (für Status-Änderungen per WebSocket).
    dm_by_user: Mutex<HashMap<String, String>>,
    /// Eigene Server-Emojis: Name → ID, beim ersten Bedarf geladen.
    custom_emojis: tokio::sync::Mutex<Option<HashMap<String, String>>>,
    /// Zuletzt bekannte Spaces; `posted` rechnet sie fort statt sie per REST neu zu laden.
    spaces: Mutex<HashMap<String, Space>>,
    /// REST-Aufrufe (für den Integrationstest).
    calls: AtomicUsize,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn http_client() -> Result<Client> {
    crate::backend::init_tls();
    Ok(Client::builder()
        .http1_only()
        .connect_timeout(CONNECT_TIMEOUT)
        .user_agent(concat!("chatty/", env!("CARGO_PKG_VERSION")))
        .build()?)
}

/// Mattermost-Fehler `{"message": ...}` lesbar machen.
fn error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.get("message")
                .and_then(|m| m.as_str())
                .map(str::to_string)
        })
        .unwrap_or_else(|| body.to_string())
}

/// HTTP-Fehler auf `Error` abbilden (401 → Auth).
async fn check(resp: Response) -> Result<Response> {
    let status = resp.status();
    if status.is_success() || status == StatusCode::SWITCHING_PROTOCOLS {
        return Ok(resp);
    }
    let body = resp.text().await.unwrap_or_default();
    if status == StatusCode::UNAUTHORIZED {
        return Err(Error::Auth(error_message(&body)));
    }
    Err(Error::Api {
        status: status.as_u16(),
        body: error_message(&body),
    })
}

fn host_of(server: &str) -> String {
    url::Url::parse(server)
        .ok()
        .and_then(|u| u.host_str().map(str::to_string))
        .unwrap_or_else(|| server.to_string())
}

impl MattermostBackend {
    /// Login mit Benutzer/Passwort (+ MFA-Code); speichert Session-Token im Keyring unter `account_id`.
    pub async fn login(
        account_id: String,
        server: String,
        login: String,
        password: String,
        mfa: Option<String>,
    ) -> Result<Self> {
        let server = server.trim_end_matches('/').to_string();
        let http = http_client()?;
        let mut body = json!({ "login_id": login, "password": password });
        if let Some(code) = mfa.filter(|c| !c.trim().is_empty()) {
            body["token"] = json!(code.trim());
        }
        let resp = check(
            http.post(format!("{server}/api/v4/users/login"))
                .timeout(API_TIMEOUT)
                .json(&body)
                .send()
                .await?,
        )
        .await?;
        let token = resp
            .headers()
            .get("Token")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
            .ok_or_else(|| Error::Auth("Server liefert kein Session-Token".into()))?;
        let user: ApiUser = resp.json().await?;
        secrets::set(&account_id, &token)?;
        let account = Account {
            id: account_id,
            kind: BackendKind::Mattermost,
            label: format!("{}@{}", user.username, host_of(&server)),
            server: Some(server.clone()),
            login: Some(login),
        };
        Ok(Self::new(account, server, token, http, user))
    }

    /// Mit gespeichertem Session-Token anmelden.
    pub async fn restore(account: Account) -> Result<Self> {
        let server = account
            .server
            .clone()
            .ok_or_else(|| Error::other("Mattermost-Konto ohne Server-URL"))?
            .trim_end_matches('/')
            .to_string();
        let token = secrets::get(&account.id)?
            .ok_or_else(|| Error::Auth("Kein gespeichertes Session-Token".into()))?;
        let http = http_client()?;
        let resp = http
            .get(format!("{server}/api/v4/users/me"))
            .timeout(API_TIMEOUT)
            .bearer_auth(&token)
            .send()
            .await?;
        let user: ApiUser = check(resp).await?.json().await?;
        Ok(Self::new(account, server, token, http, user))
    }

    fn new(account: Account, server: String, token: String, http: Client, user: ApiUser) -> Self {
        let me = Me {
            id: user.id.clone(),
            username: user.username.clone(),
        };
        let users = HashMap::from([(user.id.clone(), user)]);
        Self {
            account,
            server,
            token,
            http,
            me,
            users: Mutex::new(users),
            teams: Mutex::new(Vec::new()),
            dm_by_user: Mutex::new(HashMap::new()),
            custom_emojis: tokio::sync::Mutex::new(None),
            spaces: Mutex::new(HashMap::new()),
            calls: AtomicUsize::new(0),
        }
    }

    fn req(&self, method: Method, path: &str) -> RequestBuilder {
        self.http
            .request(method, format!("{}/api/v4{path}", self.server))
            .bearer_auth(&self.token)
    }

    /// REST-Aufruf mit Gesamt-Timeout.
    async fn send_req(&self, rb: RequestBuilder) -> Result<Response> {
        self.send_file_req(rb.timeout(API_TIMEOUT)).await
    }

    /// Für Datei-Übertragungen (Upload, Download, Bilder): ohne Gesamt-Timeout.
    async fn send_file_req(&self, rb: RequestBuilder) -> Result<Response> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        check(rb.send().await?).await
    }

    #[cfg(test)]
    fn call_count(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }

    /// Space nach einem neuen Post fortschreiben (gleiche Regel wie Frontend und Benachrichtigung).
    /// `None`, wenn der Kanal unbekannt ist.
    pub(super) fn advance_space(&self, m: &Message) -> Option<Space> {
        let mut spaces = lock(&self.spaces);
        let s = spaces.get_mut(&m.space_id)?;
        s.last_active = s.last_active.max(m.created);
        s.msg_count += 1;
        if !m.mine {
            if !m.is_reply {
                s.unread = true;
            }
            if m.mentions_me || (s.kind != SpaceKind::Channel && !s.muted && !m.is_reply) {
                s.mention_count += 1;
            }
        }
        Some(s.clone())
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        Ok(self
            .send_req(self.req(Method::GET, path))
            .await?
            .json()
            .await?)
    }

    async fn post_json<T: DeserializeOwned>(
        &self,
        path: &str,
        body: &serde_json::Value,
    ) -> Result<T> {
        Ok(self
            .send_req(self.req(Method::POST, path).json(body))
            .await?
            .json()
            .await?)
    }

    /// Fehlende Nutzer nachladen (für Absendernamen und DM-Namen).
    async fn ensure_users<'a>(&self, ids: impl IntoIterator<Item = &'a str>) -> Result<()> {
        let missing: Vec<String> = {
            let cache = lock(&self.users);
            let set: HashSet<&str> = ids
                .into_iter()
                .filter(|id| !id.is_empty() && !cache.contains_key(*id))
                .collect();
            set.into_iter().map(str::to_string).collect()
        };
        if missing.is_empty() {
            return Ok(());
        }
        let found: Vec<ApiUser> = self.post_json("/users/ids", &json!(missing)).await?;
        let mut cache = lock(&self.users);
        for u in found {
            cache.insert(u.id.clone(), u);
        }
        // Nicht gefundene (gelöschte Nutzer, Bots) merken, damit sie nicht bei jedem Post erneut gefragt werden.
        for id in missing {
            cache.entry(id.clone()).or_insert_with(|| ApiUser {
                id,
                ..ApiUser::default()
            });
        }
        Ok(())
    }

    /// Posts abbilden (Reihenfolge bleibt), Absender vorher nachladen.
    async fn map_posts(&self, posts: &[&ApiPost]) -> Result<Vec<Message>> {
        self.ensure_users(posts.iter().map(|p| p.user_id.as_str()))
            .await?;
        let users = lock(&self.users);
        Ok(posts
            .iter()
            .map(|p| api::map_post(p, &self.me, &users))
            .collect())
    }

    async fn map_one(&self, post: &ApiPost) -> Result<Message> {
        let mut list = self.map_posts(&[post]).await?;
        list.pop()
            .ok_or_else(|| Error::other("Post-Abbildung leer"))
    }

    async fn fetch_post(&self, id: &str) -> Result<ApiPost> {
        self.get(&format!("/posts/{id}")).await
    }

    async fn my_teams(&self) -> Result<Vec<ApiTeam>> {
        let teams: Vec<ApiTeam> = self.get("/users/me/teams").await?;
        lock(&self.teams).clone_from(&teams);
        Ok(teams)
    }

    /// Anzeigename: DM → Benutzername des Gegenübers, Kanäle bei mehreren Teams mit Team-Präfix.
    fn space_name(&self, ch: &ApiChannel, team: Option<&ApiTeam>, multi_team: bool) -> String {
        match ch.kind.as_str() {
            "D" => {
                let users = lock(&self.users);
                api::dm_partner(&ch.name, &self.me.id)
                    .and_then(|id| users.get(id))
                    .filter(|u| !u.username.is_empty())
                    .map_or_else(|| ch.display_name.clone(), |u| u.username.clone())
            }
            "G" => ch.display_name.clone(),
            _ => match team {
                Some(t) if multi_team => format!("{} / {}", t.display_name, ch.display_name),
                _ => ch.display_name.clone(),
            },
        }
    }

    /// DM-Partner aller Kanäle in den Nutzer-Cache laden.
    async fn ensure_dm_users(&self, channels: &[&ApiChannel]) -> Result<()> {
        let ids: Vec<&str> = channels
            .iter()
            .filter(|c| c.kind == "D")
            .filter_map(|c| api::dm_partner(&c.name, &self.me.id))
            .collect();
        self.ensure_users(ids).await
    }

    /// Einzelnen Space neu laden (für Ereignisse).
    async fn fetch_space(&self, channel_id: &str) -> Result<Space> {
        let ch: ApiChannel = self.get(&format!("/channels/{channel_id}")).await?;
        let member: ApiMember = self
            .get(&format!("/channels/{channel_id}/members/me"))
            .await?;
        self.ensure_dm_users(&[&ch]).await?;
        let teams = lock(&self.teams).clone();
        let team = teams.iter().find(|t| t.id == ch.team_id);
        let name = self.space_name(&ch, team, teams.len() > 1);
        let favorite = self.favorites().await?.contains(channel_id);
        let mut space = api::map_channel(&ch, Some(&member), name, favorite);
        self.add_dm_status(&ch, &mut space);
        lock(&self.spaces).insert(space.id.clone(), space.clone());
        Ok(space)
    }

    /// Bei DMs das Status-Emoji des Partners setzen und den Kanal für Status-Ereignisse merken.
    fn add_dm_status(&self, ch: &ApiChannel, space: &mut Space) {
        if ch.kind != "D" {
            return;
        }
        let Some(partner) = api::dm_partner(&ch.name, &self.me.id) else {
            return;
        };
        lock(&self.dm_by_user).insert(partner.to_string(), ch.id.clone());
        let now = chrono::Utc::now().timestamp_millis();
        space.status_emoji = lock(&self.users)
            .get(partner)
            .and_then(|u| u.status_emoji(now));
    }

    /// Nutzer per WebSocket geändert (z. B. Status): Cache aktualisieren, DM-Kanal liefern.
    pub(super) fn user_updated(&self, user: ApiUser) -> Option<String> {
        let dm = lock(&self.dm_by_user).get(&user.id).cloned();
        lock(&self.users).insert(user.id.clone(), user);
        dm
    }

    /// Eigene Server-Emojis (Name → ID), seitenweise geladen; ohne Berechtigung leer.
    async fn custom_emoji_map(&self) -> HashMap<String, String> {
        let mut cache = self.custom_emojis.lock().await;
        if let Some(map) = cache.as_ref() {
            return map.clone();
        }
        #[derive(serde::Deserialize)]
        struct ApiEmoji {
            id: String,
            name: String,
        }
        let mut map = HashMap::new();
        for page in 0..50 {
            let list: Vec<ApiEmoji> = match self
                .get(&format!("/emoji?page={page}&per_page=200&sort=name"))
                .await
            {
                Ok(l) => l,
                Err(_) => break,
            };
            let full = list.len() == 200;
            map.extend(list.into_iter().map(|e| (e.name, e.id)));
            if !full {
                break;
            }
        }
        cache.replace(map.clone());
        map
    }

    /// Kanal-IDs mit Preference `favorite_channel` = "true" (wie Webapp und Handy-App).
    async fn favorites(&self) -> Result<HashSet<String>> {
        let prefs: Vec<Preference> = match self.get("/users/me/preferences/favorite_channel").await
        {
            // Server meldet 404, solange es keinen Favoriten gibt.
            Err(Error::Api { status: 404, .. }) => Vec::new(),
            r => r?,
        };
        Ok(prefs
            .into_iter()
            .filter(|p| p.value == "true")
            .map(|p| p.name)
            .collect())
    }
}

#[async_trait]
impl ChatBackend for MattermostBackend {
    fn account(&self) -> &Account {
        &self.account
    }

    async fn me(&self) -> Result<User> {
        Ok(User {
            id: self.me.id.clone(),
            name: self.me.username.clone(),
            avatar_url: None,
        })
    }

    async fn list_spaces(&self) -> Result<Vec<Space>> {
        let teams = self.my_teams().await?;
        let multi_team = teams.len() > 1;
        let mut seen = HashSet::new();
        // (Kanal, Team, Mitgliedschaft)
        let mut rows: Vec<(ApiChannel, &ApiTeam, ApiMember)> = Vec::new();
        for team in &teams {
            let channels: Vec<ApiChannel> = self
                .get(&format!("/users/me/teams/{}/channels", team.id))
                .await?;
            let members: Vec<ApiMember> = self
                .get(&format!("/users/me/teams/{}/channels/members", team.id))
                .await?;
            let mut members: HashMap<String, ApiMember> = members
                .into_iter()
                .map(|m| (m.channel_id.clone(), m))
                .collect();
            for ch in channels {
                if ch.delete_at > 0 || !seen.insert(ch.id.clone()) {
                    continue;
                }
                // Manche Server liefern auch öffentliche Kanäle ohne Mitgliedschaft: weglassen.
                let Some(member) = members.remove(&ch.id) else {
                    continue;
                };
                rows.push((ch, team, member));
            }
        }
        let channels: Vec<&ApiChannel> = rows.iter().map(|(c, _, _)| c).collect();
        self.ensure_dm_users(&channels).await?;
        let favorites = self.favorites().await?;
        let spaces: Vec<Space> = rows
            .iter()
            .map(|(ch, team, member)| {
                let name = self.space_name(ch, Some(team), multi_team);
                let mut space =
                    api::map_channel(ch, Some(member), name, favorites.contains(&ch.id));
                self.add_dm_status(ch, &mut space);
                space
            })
            .collect();
        *lock(&self.spaces) = spaces.iter().map(|s| (s.id.clone(), s.clone())).collect();
        Ok(spaces)
    }

    async fn list_messages(&self, space_id: &str, cursor: Option<String>) -> Result<Page> {
        // Antworten stehen mit im Hauptverlauf (als Antwort markiert, wie Mattermost ohne „Collapsed Reply
        // Threads“); der Thread-Bereich zeigt sie zusätzlich gesammelt.
        let mut query = vec![
            ("per_page", PAGE_SIZE.to_string()),
            ("collapsedThreads", "false".into()),
        ];
        if let Some(c) = cursor {
            query.push(("before", c));
        }
        let rb = self
            .req(Method::GET, &format!("/channels/{space_id}/posts"))
            .query(&query);
        let list: PostList = self.send_req(rb).await?.json().await?;
        let next_cursor = (list.order.len() >= PAGE_SIZE)
            .then(|| list.order.last().cloned())
            .flatten();
        let mut posts: Vec<&ApiPost> = list.ordered().into_iter().collect();
        posts.reverse();
        let mut messages = self.map_posts(&posts).await?;
        count_replies(&mut messages);
        Ok(Page {
            messages,
            next_cursor,
        })
    }

    async fn list_thread(&self, _space_id: &str, thread_id: &str) -> Result<Vec<Message>> {
        let list: PostList = self.get(&format!("/posts/{thread_id}/thread")).await?;
        let mut posts = list.ordered();
        posts.sort_by_key(|p| p.create_at);
        self.map_posts(&posts).await
    }

    async fn send(
        &self,
        space_id: &str,
        text: &str,
        thread_id: Option<&str>,
        files: Vec<UploadFile>,
    ) -> Result<Message> {
        let mut file_ids: Vec<String> = Vec::new();
        if !files.is_empty() {
            let mut form = reqwest::multipart::Form::new().text("channel_id", space_id.to_string());
            for f in files {
                let part = reqwest::multipart::Part::bytes(f.data)
                    .file_name(f.name)
                    .mime_str(&f.mime)?;
                form = form.part("files", part);
            }
            let up: UploadResponse = self
                .send_file_req(self.req(Method::POST, "/files").multipart(form))
                .await?
                .json()
                .await?;
            file_ids = up.file_infos.into_iter().map(|f| f.id).collect();
        }
        let body = json!({
            "channel_id": space_id,
            "message": text,
            "root_id": thread_id.unwrap_or_default(),
            "file_ids": file_ids,
        });
        let post: ApiPost = self.post_json("/posts", &body).await?;
        self.map_one(&post).await
    }

    async fn edit(&self, _space_id: &str, message_id: &str, text: &str) -> Result<Message> {
        let rb = self
            .req(Method::PUT, &format!("/posts/{message_id}/patch"))
            .json(&json!({ "message": text }));
        let post: ApiPost = self.send_req(rb).await?.json().await?;
        self.map_one(&post).await
    }

    async fn delete(&self, _space_id: &str, message_id: &str) -> Result<()> {
        self.send_req(self.req(Method::DELETE, &format!("/posts/{message_id}")))
            .await?;
        Ok(())
    }

    async fn react(&self, _space_id: &str, message_id: &str, emoji: &str) -> Result<()> {
        let body = json!({
            "user_id": self.me.id,
            "post_id": message_id,
            "emoji_name": emoji::to_name(emoji),
        });
        self.send_req(self.req(Method::POST, "/reactions").json(&body))
            .await?;
        Ok(())
    }

    async fn unreact(&self, _space_id: &str, message_id: &str, emoji: &str) -> Result<()> {
        let path = format!(
            "/users/{}/posts/{message_id}/reactions/{}",
            self.me.id,
            emoji::to_name(emoji)
        );
        self.send_req(self.req(Method::DELETE, &path)).await?;
        Ok(())
    }

    async fn download(&self, attachment: &Attachment) -> Result<Vec<u8>> {
        let resp = self
            .send_file_req(self.req(Method::GET, &format!("/files/{}", attachment.id)))
            .await?;
        Ok(resp.bytes().await?.to_vec())
    }

    async fn browse_spaces(&self, query: &str) -> Result<Vec<Space>> {
        let teams = self.my_teams().await?;
        let multi_team = teams.len() > 1;
        let mut out = Vec::new();
        for team in &teams {
            let members: Vec<ApiMember> = self
                .get(&format!("/users/me/teams/{}/channels/members", team.id))
                .await?;
            let joined: HashSet<String> = members.into_iter().map(|m| m.channel_id).collect();
            // Öffentliche Kanäle des Teams, nach Name gesucht.
            let channels: Vec<ApiChannel> = self
                .post_json(
                    &format!("/teams/{}/channels/search", team.id),
                    &json!({ "term": query }),
                )
                .await?;
            out.extend(
                channels
                    .iter()
                    .filter(|c| c.delete_at == 0 && !joined.contains(&c.id))
                    .map(|c| {
                        let name = self.space_name(c, Some(team), multi_team);
                        api::map_channel(c, None, name, false)
                    }),
            );
        }
        Ok(out)
    }

    async fn join_space(&self, space_id: &str) -> Result<Space> {
        let _: serde_json::Value = self
            .post_json(
                &format!("/channels/{space_id}/members"),
                &json!({ "user_id": self.me.id }),
            )
            .await?;
        self.fetch_space(space_id).await
    }

    async fn avatar(&self, user_id: &str) -> Result<Option<Avatar>> {
        let resp = self
            .send_file_req(self.req(Method::GET, &format!("/users/{user_id}/image")))
            .await?;
        let mime = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/png")
            .to_string();
        Ok(Some(Avatar {
            data: resp.bytes().await?.to_vec(),
            mime,
        }))
    }

    async fn custom_emojis(&self) -> Result<Vec<String>> {
        Ok(self.custom_emoji_map().await.into_keys().collect())
    }

    async fn custom_emoji(&self, name: &str) -> Result<Option<Avatar>> {
        let Some(id) = self.custom_emoji_map().await.get(name).cloned() else {
            return Ok(None);
        };
        let resp = self
            .send_file_req(self.req(Method::GET, &format!("/emoji/{id}/image")))
            .await?;
        let mime = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("image/png")
            .to_string();
        Ok(Some(Avatar {
            data: resp.bytes().await?.to_vec(),
            mime,
        }))
    }

    async fn search(&self, query: &str) -> Result<Vec<Message>> {
        let body =
            json!({ "terms": query, "is_or_search": false, "page": 0, "per_page": PAGE_SIZE });
        let mut all: Vec<ApiPost> = Vec::new();
        let mut seen = HashSet::new();
        for team in self.my_teams().await? {
            let list: PostList = self
                .post_json(&format!("/teams/{}/posts/search", team.id), &body)
                .await?;
            for p in list.ordered() {
                if seen.insert(p.id.clone()) {
                    all.push(p.clone());
                }
            }
        }
        all.sort_by_key(|p| std::cmp::Reverse(p.create_at));
        all.truncate(PAGE_SIZE);
        let refs: Vec<&ApiPost> = all.iter().collect();
        self.map_posts(&refs).await
    }

    async fn set_favorite(&self, space_id: &str, on: bool) -> Result<bool> {
        let pref = json!([{
            "user_id": self.me.id,
            "category": "favorite_channel",
            "name": space_id,
            "value": if on { "true" } else { "false" },
        }]);
        let rb = self
            .req(Method::PUT, &format!("/users/{}/preferences", self.me.id))
            .json(&pref);
        self.send_req(rb).await?;
        Ok(true)
    }

    async fn set_muted(&self, space_id: &str, on: bool) -> Result<bool> {
        let props = json!({ "mark_unread": if on { "mention" } else { "all" } });
        let rb = self
            .req(
                Method::PUT,
                &format!("/channels/{space_id}/members/{}/notify_props", self.me.id),
            )
            .json(&props);
        self.send_req(rb).await?;
        Ok(true)
    }

    async fn mark_read(&self, space_id: &str) -> Result<()> {
        let rb = self
            .req(Method::POST, "/channels/members/me/view")
            .json(&json!({ "channel_id": space_id }));
        self.send_req(rb).await?;
        Ok(())
    }

    fn start_events(
        self: Arc<Self>,
        tx: EventSender,
        _active: watch::Receiver<Option<String>>,
        _focused: watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(ws::run(self, tx))
    }
}
