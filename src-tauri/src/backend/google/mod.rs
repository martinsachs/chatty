//! Google Chat (REST v1, OAuth Loopback + PKCE, Polling).

mod api;
mod oauth;
mod poll;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{StreamExt, stream};
use reqwest::Method;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio::sync::watch;

use self::api::{
    Api, Endpoints, GMessage, GSpace, MemberList, MessageList, Person, ReactionList, ReadState,
    SearchResponse, SpaceList,
};
use self::oauth::Tokens;
use crate::backend::{ChatBackend, EventSender};
use crate::config::{GoogleConfig, PollConfig};
use crate::error::{Error, Result};
use crate::model::{Account, Attachment, BackendKind, Message, Page, Space, UploadFile, User};
use crate::secrets;

const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
const PAGE_SIZE: &str = "50";
/// Gleichzeitige Lesestatus-Abfragen beim ersten Laden der Space-Liste.
const READSTATE_PARALLEL: usize = 4;

pub struct GoogleBackend {
    account: Account,
    me: User,
    api: Api,
    poll: PollConfig,
    /// `users/ID` → Anzeigename
    names: Mutex<HashMap<String, String>>,
    /// Space-ID → aufgelöster Name (DMs/Gruppen ohne displayName)
    space_names: Mutex<HashMap<String, String>>,
    /// Space-ID → (lastActiveTime, lastReadTime) in ms
    read_state: Mutex<HashMap<String, (i64, i64)>>,
}

fn http_client() -> Result<reqwest::Client> {
    crate::backend::init_tls();
    Ok(reqwest::Client::builder()
        .user_agent(concat!("chatty/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(60))
        .build()?)
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    // Vergiftete Caches sind unkritisch: Inhalt weiterverwenden.
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl GoogleBackend {
    /// OAuth-Login im System-Browser; speichert Refresh-Token im Keyring unter `account_id`.
    pub async fn login(
        account_id: String,
        cfg: GoogleConfig,
        poll: PollConfig,
        open_url: impl Fn(String) + Send,
    ) -> Result<Self> {
        let ep = Endpoints::default();
        let http = http_client()?;
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let redirect = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
        let verifier = oauth::random_b64(32);
        let state = oauth::random_b64(16);
        open_url(oauth::auth_url(
            &cfg.client_id,
            &redirect,
            &oauth::pkce_challenge(&verifier),
            &state,
        )?);
        drop(open_url);
        let code = oauth::wait_for_code(&listener, &state, LOGIN_TIMEOUT).await?;
        drop(listener);

        let tok = oauth::exchange_code(&http, &ep.token, &cfg, &code, &verifier, &redirect).await?;
        let refresh = tok
            .refresh_token
            .clone()
            .ok_or_else(|| Error::Auth("Google hat kein Refresh-Token geliefert".into()))?;
        let tokens = Tokens::new(http.clone(), ep.token.clone(), cfg, refresh.clone());
        tokens.set(&tok).await;
        let api = Api { http, tokens, ep };
        let (me, label) = fetch_me(&api).await?;
        secrets::set(&account_id, &refresh)?;
        let account = Account {
            id: account_id,
            kind: BackendKind::Google,
            server: None,
            label,
            login: None,
        };
        Ok(Self::new(account, me, api, poll))
    }

    /// Mit gespeichertem Refresh-Token anmelden.
    pub async fn restore(account: Account, cfg: GoogleConfig, poll: PollConfig) -> Result<Self> {
        let refresh = secrets::get(&account.id)?.ok_or_else(|| {
            Error::Auth("Kein Google-Token gespeichert – bitte neu anmelden".into())
        })?;
        let ep = Endpoints::default();
        let http = http_client()?;
        let api = Api {
            tokens: Tokens::new(http.clone(), ep.token.clone(), cfg, refresh),
            http,
            ep,
        };
        api.tokens.access().await?;
        let (me, _) = fetch_me(&api).await?;
        Ok(Self::new(account, me, api, poll))
    }

    fn new(account: Account, me: User, api: Api, poll: PollConfig) -> Self {
        let names = HashMap::from([(me.id.clone(), me.name.clone())]);
        Self {
            account,
            me,
            api,
            poll,
            names: Mutex::new(names),
            space_names: Mutex::new(HashMap::new()),
            read_state: Mutex::new(HashMap::new()),
        }
    }

    /// Anzeigename eines Chat-Nutzers (`users/ID`) über Cache bzw. People API.
    async fn user_name(&self, user_id: &str, display_name: &str) -> String {
        if !display_name.is_empty() {
            lock(&self.names).insert(user_id.to_string(), display_name.to_string());
            return display_name.to_string();
        }
        if let Some(n) = lock(&self.names).get(user_id) {
            return n.clone();
        }
        let Some(id) = user_id.strip_prefix("users/") else {
            return user_id.to_string();
        };
        let url = self.api.people(&format!("people/{id}"));
        match self
            .api
            .get::<Person>(&url, &[("personFields", "names")])
            .await
        {
            Ok(p) => {
                let name = p.display_name().unwrap_or_else(|| user_id.to_string());
                lock(&self.names).insert(user_id.to_string(), name.clone());
                name
            }
            // Nicht auffindbar (z. B. extern, Bot): Fallback merken, um nicht erneut zu fragen.
            Err(Error::Api {
                status: 403 | 404, ..
            }) => {
                lock(&self.names).insert(user_id.to_string(), user_id.to_string());
                user_id.to_string()
            }
            Err(_) => user_id.to_string(),
        }
    }

    async fn map_messages(&self, msgs: &[GMessage]) -> Vec<Message> {
        for m in msgs {
            if m.sender.display_name.is_empty() {
                self.user_name(&m.sender.name, "").await;
            } else {
                lock(&self.names).insert(m.sender.name.clone(), m.sender.display_name.clone());
            }
        }
        let names = lock(&self.names).clone();
        msgs.iter()
            .map(|m| api::map_message(m, &self.me.id, &names))
            .collect()
    }

    async fn fetch_messages(&self, space_id: &str, query: &[(&str, &str)]) -> Result<MessageList> {
        self.api
            .get(&self.api.chat(&format!("{space_id}/messages")), query)
            .await
    }

    /// Neueste Seite inkl. Thread-Antworten, aufsteigend (für das Polling).
    async fn latest_page(&self, space_id: &str) -> Result<Vec<Message>> {
        let list = self
            .fetch_messages(
                space_id,
                &[("pageSize", PAGE_SIZE), ("orderBy", "createTime desc")],
            )
            .await?;
        let mut msgs = self.map_messages(&list.messages).await;
        msgs.sort_by_key(|m| m.created);
        Ok(msgs)
    }

    /// Nachrichten nach `since_ms` (für Benachrichtigungen aus nicht offenen Spaces).
    async fn messages_since(&self, space_id: &str, since_ms: i64) -> Result<Vec<Message>> {
        let filter = format!("createTime > \"{}\"", api::format_time(since_ms));
        let list = self
            .fetch_messages(
                space_id,
                &[
                    ("pageSize", PAGE_SIZE),
                    ("orderBy", "createTime asc"),
                    ("filter", &filter),
                ],
            )
            .await?;
        let msgs = self.map_messages(&list.messages).await;
        // Millisekunden-Rundung: die bereits bekannte Nachricht ausschließen.
        Ok(msgs.into_iter().filter(|m| m.created > since_ms).collect())
    }

    /// Name für DMs/Gruppen ohne displayName: andere Mitglieder.
    async fn space_name(&self, g: &GSpace) -> Result<String> {
        if !g.display_name.is_empty() {
            return Ok(g.display_name.clone());
        }
        if let Some(n) = lock(&self.space_names).get(&g.name) {
            return Ok(n.clone());
        }
        let url = self.api.chat(&format!("{}/members", g.name));
        let list: MemberList = match self.api.get(&url, &[("pageSize", "100")]).await {
            Ok(list) => list,
            Err(e) if poll::is_transient(&e) => return Err(e),
            // Mitglieder nicht lesbar: Space-ID als Name merken, statt die ganze Liste scheitern zu lassen.
            Err(_) => {
                lock(&self.space_names).insert(g.name.clone(), g.name.clone());
                return Ok(g.name.clone());
            }
        };
        let mut names = Vec::new();
        for m in list.memberships.iter().filter_map(|m| m.member.as_ref()) {
            if m.name != self.me.id && m.kind != "BOT" {
                names.push(self.user_name(&m.name, &m.display_name).await);
            }
        }
        let name = if names.is_empty() {
            self.me.name.clone()
        } else {
            names.join(", ")
        };
        lock(&self.space_names).insert(g.name.clone(), name.clone());
        Ok(name)
    }

    /// Lesestatus nur für Spaces abfragen, deren lastActiveTime sich geändert hat.
    async fn update_read_state(&self, spaces: &[GSpace]) -> Result<()> {
        let todo: Vec<(String, i64)> = {
            let cache = lock(&self.read_state);
            spaces
                .iter()
                .map(|s| {
                    (
                        s.name.clone(),
                        s.last_active_time
                            .as_deref()
                            .map(api::parse_time)
                            .unwrap_or(0),
                    )
                })
                .filter(|(id, active)| cache.get(id).is_none_or(|(a, _)| a != active))
                .collect()
        };
        let results: Vec<_> = stream::iter(todo)
            .map(|(id, active)| async move {
                let url = self.api.chat(&format!("users/me/{id}/spaceReadState"));
                let r = self.api.get::<ReadState>(&url, &[]).await;
                (id, active, r)
            })
            .buffer_unordered(READSTATE_PARALLEL)
            .collect()
            .await;
        let mut cache = lock(&self.read_state);
        for (id, active, r) in results {
            match r {
                Ok(rs) => {
                    let read = rs
                        .last_read_time
                        .as_deref()
                        .map(api::parse_time)
                        .unwrap_or(0);
                    cache.insert(id, (active, read));
                }
                Err(e @ (Error::Auth(_) | Error::Http(_) | Error::Api { status: 429, .. })) => {
                    return Err(e);
                }
                // Einzelner Space nicht lesbar: als gelesen behandeln, beim nächsten Mal erneut.
                Err(_) => {}
            }
        }
        Ok(())
    }

    async fn upload(&self, space_id: &str, f: &UploadFile) -> Result<Value> {
        let boundary = format!("chatty-{}", oauth::random_b64(12));
        let body = api::multipart_related(&boundary, &f.name, &f.mime, &f.data)?;
        let ctype = format!("multipart/related; boundary={boundary}");
        let url = self.api.upload(&format!("{space_id}/attachments:upload"));
        let resp: Value = self
            .api
            .json(Method::POST, &url, |rb| {
                rb.query(&[("uploadType", "multipart")])
                    .header(reqwest::header::CONTENT_TYPE, &ctype)
                    .body(body.clone())
            })
            .await?;
        resp.get("attachmentDataRef")
            .cloned()
            .ok_or_else(|| Error::other("Upload ohne attachmentDataRef"))
    }
}

async fn fetch_me(api: &Api) -> Result<(User, String)> {
    let p: Person = api
        .get(
            &api.people("people/me"),
            &[("personFields", "names,emailAddresses")],
        )
        .await?;
    let id = p
        .resource_name
        .strip_prefix("people/")
        .ok_or_else(|| Error::other("People API: keine Nutzer-ID"))?;
    let email = p
        .email_addresses
        .iter()
        .map(|e| e.value.clone())
        .find(|e| !e.is_empty());
    let name = p
        .display_name()
        .or_else(|| email.clone())
        .unwrap_or_else(|| id.to_string());
    let label = email.unwrap_or_else(|| name.clone());
    Ok((
        User {
            id: format!("users/{id}"),
            name,
            avatar_url: None,
        },
        label,
    ))
}

#[async_trait]
impl ChatBackend for GoogleBackend {
    fn account(&self) -> &Account {
        &self.account
    }

    async fn me(&self) -> Result<User> {
        Ok(self.me.clone())
    }

    async fn list_spaces(&self) -> Result<Vec<Space>> {
        let mut all = Vec::new();
        let mut token = String::new();
        loop {
            let mut q = vec![("pageSize", "1000")];
            if !token.is_empty() {
                q.push(("pageToken", token.as_str()));
            }
            let page: SpaceList = self.api.get(&self.api.chat("spaces"), &q).await?;
            all.extend(page.spaces);
            if page.next_page_token.is_empty() {
                break;
            }
            token = page.next_page_token;
        }
        self.update_read_state(&all).await?;
        let mut out = Vec::with_capacity(all.len());
        for g in &all {
            let name = self.space_name(g).await?;
            let last_active = g
                .last_active_time
                .as_deref()
                .map(api::parse_time)
                .unwrap_or(0);
            let unread = lock(&self.read_state)
                .get(&g.name)
                .is_some_and(|(_, read)| *read < last_active);
            out.push(api::map_space(g, name, unread));
        }
        Ok(out)
    }

    async fn list_messages(&self, space_id: &str, cursor: Option<String>) -> Result<Page> {
        let mut q = vec![("pageSize", PAGE_SIZE), ("orderBy", "createTime desc")];
        if let Some(c) = cursor.as_deref() {
            q.push(("pageToken", c));
        }
        let list = self.fetch_messages(space_id, &q).await?;
        let msgs = self.map_messages(&list.messages).await;
        Ok(Page {
            messages: api::main_timeline(msgs),
            next_cursor: Some(list.next_page_token).filter(|t| !t.is_empty()),
        })
    }

    async fn list_thread(&self, space_id: &str, thread_id: &str) -> Result<Vec<Message>> {
        let filter = format!("thread.name = \"{thread_id}\"");
        let list = self
            .fetch_messages(
                space_id,
                &[
                    ("pageSize", "1000"),
                    ("orderBy", "createTime asc"),
                    ("filter", &filter),
                ],
            )
            .await?;
        let mut msgs = self.map_messages(&list.messages).await;
        msgs.sort_by_key(|m| m.created);
        Ok(msgs)
    }

    async fn send(
        &self,
        space_id: &str,
        text: &str,
        thread_id: Option<&str>,
        files: Vec<UploadFile>,
    ) -> Result<Message> {
        let mut refs = Vec::new();
        for f in &files {
            refs.push(json!({ "attachmentDataRef": self.upload(space_id, f).await? }));
        }
        let mut body = json!({ "text": text });
        if !refs.is_empty() {
            body["attachment"] = Value::Array(refs);
        }
        let mut q = Vec::new();
        if let Some(t) = thread_id {
            body["thread"] = json!({ "name": t });
            q.push(("messageReplyOption", "REPLY_MESSAGE_FALLBACK_TO_NEW_THREAD"));
        }
        let url = self.api.chat(&format!("{space_id}/messages"));
        let g: GMessage = self
            .api
            .json(Method::POST, &url, |rb| rb.query(&q).json(&body))
            .await?;
        Ok(self.map_messages(std::slice::from_ref(&g)).await.remove(0))
    }

    async fn edit(&self, _space_id: &str, message_id: &str, text: &str) -> Result<Message> {
        let body = json!({ "text": text });
        let g: GMessage = self
            .api
            .json(Method::PATCH, &self.api.chat(message_id), |rb| {
                rb.query(&[("updateMask", "text")]).json(&body)
            })
            .await?;
        Ok(self.map_messages(std::slice::from_ref(&g)).await.remove(0))
    }

    async fn delete(&self, _space_id: &str, message_id: &str) -> Result<()> {
        self.api
            .send(Method::DELETE, &self.api.chat(message_id), |rb| rb)
            .await?;
        Ok(())
    }

    async fn react(&self, _space_id: &str, message_id: &str, emoji: &str) -> Result<()> {
        let body = json!({ "emoji": { "unicode": emoji } });
        let url = self.api.chat(&format!("{message_id}/reactions"));
        self.api
            .send(Method::POST, &url, |rb| rb.json(&body))
            .await?;
        Ok(())
    }

    async fn unreact(&self, _space_id: &str, message_id: &str, emoji: &str) -> Result<()> {
        let filter = format!(
            "emoji.unicode = \"{emoji}\" AND user.name = \"{}\"",
            self.me.id
        );
        let url = self.api.chat(&format!("{message_id}/reactions"));
        let list: ReactionList = self
            .api
            .get(&url, &[("filter", &filter), ("pageSize", "200")])
            .await?;
        for r in list.reactions {
            self.api
                .send(Method::DELETE, &self.api.chat(&r.name), |rb| rb)
                .await?;
        }
        Ok(())
    }

    async fn download(&self, attachment: &Attachment) -> Result<Vec<u8>> {
        if attachment.id.is_empty() {
            return Err(Error::other("Nur im Browser öffnen"));
        }
        let url = self.api.chat(&format!("media/{}", attachment.id));
        let resp = self
            .api
            .send(Method::GET, &url, |rb| rb.query(&[("alt", "media")]))
            .await?;
        Ok(resp.bytes().await?.to_vec())
    }

    async fn search(&self, query: &str) -> Result<Vec<Message>> {
        let body = json!({ "filter": query, "pageSize": 50 });
        let url = self.api.chat("spaces/-/messages:search");
        let r: SearchResponse = self
            .api
            .json(Method::POST, &url, |rb| rb.json(&body))
            .await?;
        let msgs: Vec<GMessage> = r.results.into_iter().map(|r| r.message).collect();
        Ok(self.map_messages(&msgs).await)
    }

    async fn mark_read(&self, space_id: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        let body = json!({ "lastReadTime": api::format_time(now) });
        let url = self
            .api
            .chat(&format!("users/me/{space_id}/spaceReadState"));
        self.api
            .send(Method::PATCH, &url, |rb| {
                rb.query(&[("updateMask", "lastReadTime")]).json(&body)
            })
            .await?;
        if let Some(e) = lock(&self.read_state).get_mut(space_id) {
            e.1 = now;
        }
        Ok(())
    }

    fn start_events(
        self: Arc<Self>,
        tx: EventSender,
        active_space: watch::Receiver<Option<String>>,
        focused: watch::Receiver<bool>,
    ) -> tauri::async_runtime::JoinHandle<()> {
        tauri::async_runtime::spawn(poll::run(self, tx, active_space, focused))
    }
}

/// Nur für Tests: Backend gegen lokale Endpunkte.
#[cfg(test)]
impl GoogleBackend {
    fn for_test(ep: Endpoints, me: &str) -> Self {
        let http = http_client().expect("HTTP-Client");
        let cfg = GoogleConfig {
            client_id: "cid".into(),
            client_secret: String::new(),
        };
        let tokens = Tokens::new(http.clone(), ep.token.clone(), cfg, "rt".into());
        let account = Account {
            id: "t".into(),
            kind: BackendKind::Google,
            server: None,
            label: "t".into(),
            login: None,
        };
        let me = User {
            id: me.into(),
            name: "Ich".into(),
            avatar_url: None,
        };
        Self::new(account, me, Api { http, tokens, ep }, PollConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;
    use crate::model::SpaceKind;

    /// Minimaler HTTP-Mock: `handler(method_and_path, authorization, body) -> (status, json)`.
    async fn mock<F>(handler: F) -> (String, Arc<Mutex<Vec<String>>>)
    where
        F: Fn(&str, &str, &str) -> (u16, String) + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let log = Arc::new(Mutex::new(Vec::new()));
        let log2 = log.clone();
        let handler = Arc::new(handler);
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else {
                    return;
                };
                let handler = handler.clone();
                let log = log2.clone();
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 4096];
                    let (head, body) = loop {
                        let n = s.read(&mut chunk).await.unwrap();
                        buf.extend_from_slice(&chunk[..n]);
                        let text = String::from_utf8_lossy(&buf).to_string();
                        if let Some(pos) = text.find("\r\n\r\n") {
                            let head = text[..pos].to_string();
                            let len = head
                                .lines()
                                .find_map(|l| {
                                    l.to_ascii_lowercase()
                                        .strip_prefix("content-length:")
                                        .map(|v| v.trim().parse::<usize>().unwrap())
                                })
                                .unwrap_or(0);
                            if buf.len() >= pos + 4 + len {
                                break (
                                    head,
                                    String::from_utf8_lossy(&buf[pos + 4..pos + 4 + len])
                                        .to_string(),
                                );
                            }
                        }
                        if n == 0 {
                            return;
                        }
                    };
                    let first = head.lines().next().unwrap_or("").to_string();
                    let auth = head
                        .lines()
                        .find_map(|l| {
                            l.strip_prefix("authorization: ")
                                .or_else(|| l.strip_prefix("Authorization: "))
                        })
                        .unwrap_or("")
                        .to_string();
                    let req = first
                        .rsplit_once(' ')
                        .map(|x| x.0)
                        .unwrap_or("")
                        .to_string();
                    log.lock().unwrap().push(req.clone());
                    let (status, json) = handler(&req, &auth, &body);
                    let resp = format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{json}",
                        json.len()
                    );
                    let _ = s.write_all(resp.as_bytes()).await;
                });
            }
        });
        (base, log)
    }

    fn ep(base: &str) -> Endpoints {
        Endpoints {
            chat: base.into(),
            people: base.into(),
            token: format!("{base}/token"),
        }
    }

    #[tokio::test]
    async fn refresh_on_401_and_name_resolution() {
        let tokens = Arc::new(AtomicUsize::new(0));
        let t2 = tokens.clone();
        let (base, log) = mock(move |req, auth, body| {
            if req.starts_with("POST /token") {
                assert!(body.contains("grant_type=refresh_token"));
                let n = t2.fetch_add(1, Ordering::SeqCst) + 1;
                return (200, format!(r#"{{"access_token":"tok{n}","expires_in":3600}}"#));
            }
            if auth != "Bearer tok2" {
                return (401, "{}".into());
            }
            if req.starts_with("GET /v1/people/42") {
                return (200, r#"{"resourceName":"people/42","names":[{"displayName":"Erika"}]}"#.into());
            }
            if req.starts_with("GET /v1/spaces/A/messages") {
                assert!(req.contains("orderBy=createTime+desc") || req.contains("orderBy=createTime%20desc"));
                return (
                    200,
                    r#"{"messages":[
                      {"name":"spaces/A/messages/2","sender":{"name":"users/42"},"createTime":"2026-01-01T00:00:02Z","text":"b","thread":{"name":"spaces/A/threads/1"},"threadReply":true},
                      {"name":"spaces/A/messages/1","sender":{"name":"users/42"},"createTime":"2026-01-01T00:00:01Z","text":"a","thread":{"name":"spaces/A/threads/1"}}
                    ],"nextPageToken":"NEXT"}"#
                        .into(),
                );
            }
            (404, "{}".into())
        })
        .await;
        let b = GoogleBackend::for_test(ep(&base), "users/7");
        let page = b.list_messages("spaces/A", None).await.unwrap();
        assert_eq!(page.next_cursor.as_deref(), Some("NEXT"));
        assert_eq!(page.messages.len(), 1);
        assert_eq!(page.messages[0].text, "a");
        assert_eq!(page.messages[0].sender.name, "Erika");
        assert_eq!(page.messages[0].reply_count, 1);
        // tok1 abgelehnt → einmal erneuert; People-API nur einmal (Cache).
        assert_eq!(tokens.load(Ordering::SeqCst), 2);
        let people = log
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.contains("/people/"))
            .count();
        assert_eq!(people, 1);
    }

    #[tokio::test]
    async fn persistent_401_is_auth_error() {
        let (base, _) = mock(|req, _, _| {
            if req.starts_with("POST /token") {
                return (200, r#"{"access_token":"x","expires_in":3600}"#.into());
            }
            (401, "{}".into())
        })
        .await;
        let b = GoogleBackend::for_test(ep(&base), "users/7");
        assert!(matches!(b.mark_read("spaces/A").await, Err(Error::Auth(_))));
    }

    #[tokio::test]
    async fn invalid_grant_is_auth_error() {
        let (base, _) = mock(|_, _, _| (400, r#"{"error":"invalid_grant","error_description":"Token has been expired or revoked."}"#.into())).await;
        let b = GoogleBackend::for_test(ep(&base), "users/7");
        assert!(matches!(b.list_spaces().await, Err(Error::Auth(_))));
    }

    #[tokio::test]
    async fn spaces_unread_and_dm_names() {
        let (base, log) = mock(|req, _, _| {
            if req.starts_with("POST /token") {
                return (200, r#"{"access_token":"x","expires_in":3600}"#.into());
            }
            let r = match req {
                q if q.starts_with("GET /v1/spaces?") => r#"{"spaces":[
                    {"name":"spaces/S","spaceType":"SPACE","displayName":"Team","lastActiveTime":"2026-01-01T00:00:10Z"},
                    {"name":"spaces/D","spaceType":"DIRECT_MESSAGE","lastActiveTime":"2026-01-01T00:00:05Z"}]}"#,
                q if q.starts_with("GET /v1/users/me/spaces/S/spaceReadState") => r#"{"lastReadTime":"2026-01-01T00:00:01Z"}"#,
                q if q.starts_with("GET /v1/users/me/spaces/D/spaceReadState") => r#"{"lastReadTime":"2026-01-01T00:00:05Z"}"#,
                q if q.starts_with("GET /v1/spaces/D/members") => r#"{"memberships":[
                    {"member":{"name":"users/7","type":"HUMAN"}},
                    {"member":{"name":"users/8","displayName":"Max","type":"HUMAN"}}]}"#,
                q if q.starts_with("PATCH /v1/users/me/spaces/S/spaceReadState?updateMask=lastReadTime") => "{}",
                _ => return (404, "{}".into()),
            };
            (200, r.into())
        })
        .await;
        let b = GoogleBackend::for_test(ep(&base), "users/7");
        let s = b.list_spaces().await.unwrap();
        assert_eq!(
            (s[0].name.as_str(), s[0].unread, s[0].kind),
            ("Team", true, SpaceKind::Channel)
        );
        assert_eq!(
            (s[1].name.as_str(), s[1].unread, s[1].kind),
            ("Max", false, SpaceKind::Direct)
        );
        b.mark_read("spaces/S").await.unwrap();
        let s = b.list_spaces().await.unwrap();
        assert!(!s[0].unread);
        // Zweiter Durchlauf: kein erneuter Lesestatus-/Mitglieder-Abruf.
        let log = log.lock().unwrap();
        assert_eq!(
            log.iter()
                .filter(|r| r.contains("spaceReadState") && r.starts_with("GET"))
                .count(),
            2
        );
        assert_eq!(log.iter().filter(|r| r.contains("/members")).count(), 1);
    }

    #[tokio::test]
    async fn dm_name_falls_back_to_space_id() {
        let (base, log) = mock(|req, _, _| {
            if req.starts_with("POST /token") {
                return (200, r#"{"access_token":"x","expires_in":3600}"#.into());
            }
            if req.starts_with("GET /v1/spaces?") {
                return (
                    200,
                    r#"{"spaces":[{"name":"spaces/X","spaceType":"DIRECT_MESSAGE","lastActiveTime":"2026-01-01T00:00:05Z"}]}"#.into(),
                );
            }
            if req.contains("spaceReadState") {
                return (200, r#"{"lastReadTime":"2026-01-01T00:00:05Z"}"#.into());
            }
            (404, "{}".into())
        })
        .await;
        let b = GoogleBackend::for_test(ep(&base), "users/7");
        let s = b.list_spaces().await.unwrap();
        assert_eq!(s[0].name, "spaces/X", "Mitglieder 404 → Space-ID als Name");
        b.list_spaces().await.unwrap();
        let members = log
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.contains("/members"))
            .count();
        assert_eq!(members, 1, "Rückfall gemerkt");
    }

    #[tokio::test]
    async fn unreadable_space_does_not_block_others() {
        let round = Arc::new(AtomicUsize::new(0));
        let r2 = round.clone();
        let (base, log) = mock(move |req, _, _| {
            if req.starts_with("POST /token") {
                return (200, r#"{"access_token":"x","expires_in":3600}"#.into());
            }
            if req.starts_with("GET /v1/spaces?") {
                // Ab dem zweiten Abruf sind beide Spaces aktiver geworden.
                let t = if r2.fetch_add(1, Ordering::SeqCst) == 0 { 1 } else { 9 };
                return (
                    200,
                    format!(
                        r#"{{"spaces":[
                        {{"name":"spaces/A","spaceType":"SPACE","displayName":"A","lastActiveTime":"2026-01-01T00:00:0{t}Z"}},
                        {{"name":"spaces/B","spaceType":"SPACE","displayName":"B","lastActiveTime":"2026-01-01T00:00:0{t}Z"}}]}}"#
                    ),
                );
            }
            if req.contains("spaceReadState") {
                return (200, r#"{"lastReadTime":"2026-01-01T00:00:00Z"}"#.into());
            }
            if req.starts_with("GET /v1/spaces/A/messages") {
                return (404, "{}".into());
            }
            if req.starts_with("GET /v1/spaces/B/messages") {
                return (
                    200,
                    r#"{"messages":[{"name":"spaces/B/messages/1","sender":{"name":"users/42","displayName":"Erika"},"createTime":"2026-01-01T00:00:09Z","text":"neu"}]}"#.into(),
                );
            }
            (404, "{}".into())
        })
        .await;
        let b = GoogleBackend::for_test(ep(&base), "users/7");
        let mut st = poll::State::for_test(None, true);
        poll::poll_spaces(&b, &mut st).await.unwrap();
        let events = poll::poll_spaces(&b, &mut st).await.unwrap();
        assert!(
            events.iter().any(|e| matches!(e, crate::model::BackendEvent::MessageCreated { message } if message.space_id == "spaces/B")),
            "B wird trotz Fehler in A gemeldet"
        );
        poll::poll_spaces(&b, &mut st).await.unwrap();
        let a_calls = log
            .lock()
            .unwrap()
            .iter()
            .filter(|r| r.starts_with("GET /v1/spaces/A/messages"))
            .count();
        assert_eq!(a_calls, 1, "A nicht bei jedem Durchlauf erneut abgefragt");
    }
}
