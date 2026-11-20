//! HTTP-Zugriff (Bearer-Token, 401-Wiederholung) und JSON-Typen der Chat-/People-API
//! samt Abbildung auf das neutrale Modell.

use std::collections::HashMap;

use reqwest::{Method, RequestBuilder};
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::oauth::Tokens;
use crate::error::{Error, Result};
use crate::model::{Attachment, Message, Reaction, Space, SpaceKind, User};

/// Basis-URLs (in Tests auf einen lokalen Mock umgebogen).
#[derive(Debug, Clone)]
pub struct Endpoints {
    /// `https://chat.googleapis.com` (ohne `/v1`, wegen `/upload/v1`).
    pub chat: String,
    pub people: String,
    pub token: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            chat: "https://chat.googleapis.com".into(),
            people: "https://people.googleapis.com".into(),
            token: super::oauth::TOKEN_URL.into(),
        }
    }
}

pub struct Api {
    pub http: reqwest::Client,
    pub tokens: Tokens,
    pub ep: Endpoints,
}

impl Api {
    pub fn chat(&self, path: &str) -> String {
        format!("{}/v1/{path}", self.ep.chat)
    }

    pub fn upload(&self, path: &str) -> String {
        format!("{}/upload/v1/{path}", self.ep.chat)
    }

    pub fn people(&self, path: &str) -> String {
        format!("{}/v1/{path}", self.ep.people)
    }

    /// Request mit Bearer-Token; bei 401 einmal Token erneuern und wiederholen.
    pub async fn send(
        &self,
        method: Method,
        url: &str,
        build: impl Fn(RequestBuilder) -> RequestBuilder,
    ) -> Result<reqwest::Response> {
        for attempt in 0..2 {
            let token = self.tokens.access().await?;
            let resp = build(self.http.request(method.clone(), url).bearer_auth(&token))
                .send()
                .await?;
            let status = resp.status();
            if status == reqwest::StatusCode::UNAUTHORIZED {
                if attempt == 0 {
                    self.tokens.invalidate(&token).await;
                    continue;
                }
                break;
            }
            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(Error::Api {
                    status: status.as_u16(),
                    body,
                });
            }
            return Ok(resp);
        }
        Err(Error::Auth(
            "Google lehnt den Zugriff ab (401) – bitte neu anmelden".into(),
        ))
    }

    pub async fn json<T: DeserializeOwned>(
        &self,
        method: Method,
        url: &str,
        build: impl Fn(RequestBuilder) -> RequestBuilder,
    ) -> Result<T> {
        let resp = self.send(method, url, build).await?;
        let text = resp.text().await?;
        // Leere Antworten (z. B. DELETE) als `{}` behandeln.
        Ok(serde_json::from_str(if text.trim().is_empty() {
            "{}"
        } else {
            &text
        })?)
    }

    pub async fn get<T: DeserializeOwned>(&self, url: &str, query: &[(&str, &str)]) -> Result<T> {
        self.json(Method::GET, url, |rb| rb.query(query)).await
    }
}

// ---------------------------------------------------------------- JSON-Typen

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GSpace {
    pub name: String,
    pub display_name: String,
    pub space_type: String,
    pub last_active_time: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SpaceList {
    pub spaces: Vec<GSpace>,
    pub next_page_token: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GUser {
    pub name: String,
    pub display_name: String,
    #[serde(rename = "type")]
    pub kind: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Membership {
    pub member: Option<GUser>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MemberList {
    pub memberships: Vec<Membership>,
    pub next_page_token: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GMessage {
    pub name: String,
    pub sender: GUser,
    pub create_time: String,
    pub last_update_time: Option<String>,
    pub text: String,
    pub annotations: Vec<GAnnotation>,
    pub thread: Option<GThread>,
    pub thread_reply: bool,
    pub attachment: Vec<GAttachment>,
    pub emoji_reaction_summaries: Vec<GReactionSummary>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GAnnotation {
    #[serde(rename = "type")]
    pub kind: String,
    pub user_mention: Option<GUserMention>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GUserMention {
    pub user: GUser,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GThread {
    pub name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GAttachment {
    pub content_name: String,
    pub content_type: String,
    pub attachment_data_ref: Option<GDataRef>,
    pub drive_data_ref: Option<GDriveRef>,
    pub download_uri: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GDataRef {
    pub resource_name: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GDriveRef {
    pub drive_file_id: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GReactionSummary {
    pub emoji: GEmoji,
    pub reaction_count: u32,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GEmoji {
    pub unicode: Option<String>,
    pub custom_emoji: Option<GCustomEmoji>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GCustomEmoji {
    pub emoji_name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MessageList {
    pub messages: Vec<GMessage>,
    pub next_page_token: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchResult {
    pub message: GMessage,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SearchResponse {
    pub results: Vec<SearchResult>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReadState {
    pub last_read_time: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GReaction {
    pub name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReactionList {
    pub reactions: Vec<GReaction>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Person {
    pub resource_name: String,
    pub names: Vec<PersonName>,
    pub email_addresses: Vec<PersonEmail>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PersonName {
    pub display_name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PersonEmail {
    pub value: String,
}

impl Person {
    pub fn display_name(&self) -> Option<String> {
        self.names
            .iter()
            .map(|n| n.display_name.clone())
            .find(|n| !n.is_empty())
    }
}

// ---------------------------------------------------------------- Abbildung

/// RFC 3339 → Millisekunden; 0 bei ungültigem Wert.
pub fn parse_time(s: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|t| t.timestamp_millis())
        .unwrap_or(0)
}

/// Millisekunden → RFC 3339 (UTC, Millisekunden).
pub fn format_time(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// `spaces/x/messages/y` → `spaces/x`.
pub fn space_of(message_name: &str) -> String {
    message_name
        .split("/messages/")
        .next()
        .unwrap_or(message_name)
        .to_string()
}

pub fn space_kind(space_type: &str) -> SpaceKind {
    match space_type {
        "DIRECT_MESSAGE" => SpaceKind::Direct,
        "GROUP_CHAT" => SpaceKind::Group,
        _ => SpaceKind::Channel,
    }
}

pub fn map_space(g: &GSpace, name: String, unread: bool) -> Space {
    Space {
        id: g.name.clone(),
        name,
        kind: space_kind(&g.space_type),
        unread,
        mention_count: 0,
        last_active: g.last_active_time.as_deref().map(parse_time).unwrap_or(0),
        msg_count: 0,
        favorite: false,
        muted: false,
    }
}

fn map_attachment(a: &GAttachment) -> Attachment {
    let id = a
        .attachment_data_ref
        .as_ref()
        .map(|r| r.resource_name.clone())
        .unwrap_or_default();
    let url = if id.is_empty() {
        a.download_uri.clone().or_else(|| {
            a.drive_data_ref
                .as_ref()
                .filter(|d| !d.drive_file_id.is_empty())
                .map(|d| format!("https://drive.google.com/open?id={}", d.drive_file_id))
        })
    } else {
        None
    };
    Attachment {
        id,
        name: a.content_name.clone(),
        mime: Some(a.content_type.clone()).filter(|m| !m.is_empty()),
        size: None,
        url,
    }
}

fn map_reaction(r: &GReactionSummary) -> Option<Reaction> {
    let emoji = match (&r.emoji.unicode, &r.emoji.custom_emoji) {
        (Some(u), _) if !u.is_empty() => u.clone(),
        (_, Some(c)) if !c.emoji_name.is_empty() => c.emoji_name.clone(),
        _ => return None,
    };
    Some(Reaction {
        emoji,
        count: r.reaction_count,
        me: false,
    })
}

/// `me` = eigene Chat-User-ID (`users/123`), `names` = bekannte Anzeigenamen.
pub fn map_message(m: &GMessage, me: &str, names: &HashMap<String, String>) -> Message {
    let sender_name = if m.sender.display_name.is_empty() {
        names
            .get(&m.sender.name)
            .cloned()
            .unwrap_or_else(|| m.sender.name.clone())
    } else {
        m.sender.display_name.clone()
    };
    let mentions_me = m.annotations.iter().any(|a| {
        a.kind == "USER_MENTION"
            && a.user_mention
                .as_ref()
                .is_some_and(|u| u.user.name == me || u.user.name == "users/all")
    });
    Message {
        id: m.name.clone(),
        space_id: space_of(&m.name),
        thread_id: m
            .thread
            .as_ref()
            .map(|t| t.name.clone())
            .filter(|t| !t.is_empty()),
        is_reply: m.thread_reply,
        sender: User {
            id: m.sender.name.clone(),
            name: sender_name,
            avatar_url: None,
        },
        text: m.text.clone(),
        created: parse_time(&m.create_time),
        edited: m
            .last_update_time
            .as_deref()
            .map(parse_time)
            .filter(|t| *t > 0),
        reactions: m
            .emoji_reaction_summaries
            .iter()
            .filter_map(map_reaction)
            .collect(),
        attachments: m.attachment.iter().map(map_attachment).collect(),
        reply_count: 0,
        mentions_me,
        mine: m.sender.name == me,
    }
}

/// Hauptverlauf aus einer absteigenden Seite: aufsteigend sortiert, ohne Thread-Antworten;
/// `reply_count` = in dieser Seite gesehene Antworten (Untergrenze).
pub fn main_timeline(mut msgs: Vec<Message>) -> Vec<Message> {
    msgs.sort_by_key(|m| m.created);
    let mut counts: HashMap<String, u32> = HashMap::new();
    for m in msgs.iter().filter(|m| m.is_reply) {
        if let Some(t) = &m.thread_id {
            *counts.entry(t.clone()).or_default() += 1;
        }
    }
    msgs.into_iter()
        .filter(|m| !m.is_reply)
        .map(|mut m| {
            if let Some(c) = m.thread_id.as_ref().and_then(|t| counts.get(t)) {
                m.reply_count = *c;
            }
            m
        })
        .collect()
}

/// Rohdaten für einen Multipart/related-Upload (Metadaten + Datei).
pub fn multipart_related(
    boundary: &str,
    filename: &str,
    mime: &str,
    data: &[u8],
) -> Result<Vec<u8>> {
    let meta = serde_json::to_string(&serde_json::json!({ "filename": filename }))?;
    let mut body = Vec::with_capacity(data.len() + 512);
    body.extend_from_slice(
        format!(
            "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: {mime}\r\n\r\n"
        )
        .as_bytes(),
    );
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MSG: &str = r#"{
      "name": "spaces/AAA/messages/M1.M1",
      "sender": {"name": "users/111", "type": "HUMAN"},
      "createTime": "2026-11-20T10:00:00.123456Z",
      "lastUpdateTime": "2026-11-20T10:05:00Z",
      "text": "Hallo @Martin",
      "annotations": [
        {"type": "USER_MENTION", "startIndex": 6, "length": 7,
         "userMention": {"user": {"name": "users/999", "type": "HUMAN"}, "type": "MENTION"}}
      ],
      "thread": {"name": "spaces/AAA/threads/T1"},
      "threadReply": true,
      "attachment": [
        {"name": "spaces/AAA/messages/M1.M1/attachments/A1", "contentName": "bild.png",
         "contentType": "image/png", "attachmentDataRef": {"resourceName": "RES123"},
         "source": "UPLOADED_CONTENT"},
        {"name": "spaces/AAA/messages/M1.M1/attachments/A2", "contentName": "Doku",
         "contentType": "application/vnd.google-apps.document",
         "driveDataRef": {"driveFileId": "DRV1"}, "source": "DRIVE_FILE"}
      ],
      "emojiReactionSummaries": [
        {"emoji": {"unicode": "👍"}, "reactionCount": 2},
        {"emoji": {"customEmoji": {"uid": "x", "emojiName": ":party:"}}, "reactionCount": 1}
      ],
      "space": {"name": "spaces/AAA"}
    }"#;

    #[test]
    fn message_mapping() {
        let g: GMessage = serde_json::from_str(MSG).unwrap();
        let names = HashMap::from([("users/111".to_string(), "Erika".to_string())]);
        let m = map_message(&g, "users/999", &names);
        assert_eq!(m.id, "spaces/AAA/messages/M1.M1");
        assert_eq!(m.space_id, "spaces/AAA");
        assert_eq!(m.thread_id.as_deref(), Some("spaces/AAA/threads/T1"));
        assert!(m.is_reply);
        assert_eq!(m.sender.name, "Erika");
        assert_eq!(m.created, 1_795_168_800_123);
        assert_eq!(m.edited, Some(1_795_169_100_000));
        assert!(m.mentions_me);
        assert!(!m.mine);
        assert_eq!(m.reactions.len(), 2);
        assert_eq!(
            m.reactions[0],
            Reaction {
                emoji: "👍".into(),
                count: 2,
                me: false
            }
        );
        assert_eq!(m.reactions[1].emoji, ":party:");
        assert_eq!(m.attachments[0].id, "RES123");
        assert_eq!(m.attachments[0].mime.as_deref(), Some("image/png"));
        assert_eq!(m.attachments[0].url, None);
        assert_eq!(m.attachments[1].id, "");
        assert_eq!(
            m.attachments[1].url.as_deref(),
            Some("https://drive.google.com/open?id=DRV1")
        );

        let mine = map_message(&g, "users/111", &HashMap::new());
        assert!(mine.mine);
        assert!(!mine.mentions_me);
        assert_eq!(mine.sender.name, "users/111");
    }

    #[test]
    fn minimal_message_and_edit_absent() {
        let g: GMessage = serde_json::from_str(
            r#"{"name":"spaces/B/messages/X","sender":{"name":"users/1","displayName":"Bot"},
                "createTime":"2026-01-01T00:00:00Z","text":"hi"}"#,
        )
        .unwrap();
        let m = map_message(&g, "users/2", &HashMap::new());
        assert_eq!(m.sender.name, "Bot");
        assert_eq!(m.edited, None);
        assert_eq!(m.thread_id, None);
        assert!(m.reactions.is_empty() && m.attachments.is_empty());
    }

    #[test]
    fn space_mapping() {
        let list: SpaceList = serde_json::from_str(
            r#"{"spaces":[
              {"name":"spaces/S1","spaceType":"SPACE","displayName":"Team","lastActiveTime":"2026-11-20T10:00:00Z"},
              {"name":"spaces/D1","spaceType":"DIRECT_MESSAGE","singleUserBotDm":false},
              {"name":"spaces/G1","spaceType":"GROUP_CHAT"}
            ],"nextPageToken":"p2"}"#,
        )
        .unwrap();
        assert_eq!(list.next_page_token, "p2");
        let s = map_space(&list.spaces[0], "Team".into(), true);
        assert_eq!(s.kind, SpaceKind::Channel);
        assert_eq!(s.last_active, 1_795_168_800_000);
        assert!(s.unread);
        assert_eq!(
            map_space(&list.spaces[1], "x".into(), false).kind,
            SpaceKind::Direct
        );
        assert_eq!(
            map_space(&list.spaces[2], "x".into(), false).kind,
            SpaceKind::Group
        );
        assert_eq!(map_space(&list.spaces[2], "x".into(), false).last_active, 0);
    }

    #[test]
    fn timeline_hides_replies() {
        let mk = |id: &str, t: i64, reply: bool, thread: &str| Message {
            id: id.into(),
            space_id: "spaces/A".into(),
            thread_id: Some(thread.into()),
            is_reply: reply,
            sender: User {
                id: "users/1".into(),
                name: "n".into(),
                avatar_url: None,
            },
            text: String::new(),
            created: t,
            edited: None,
            reactions: vec![],
            attachments: vec![],
            reply_count: 0,
            mentions_me: false,
            mine: false,
        };
        let page = vec![
            mk("c", 3, true, "t1"),
            mk("b", 2, false, "t2"),
            mk("a", 1, false, "t1"),
        ];
        let tl = main_timeline(page);
        assert_eq!(
            tl.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(),
            ["a", "b"]
        );
        assert_eq!(tl[0].reply_count, 1);
        assert_eq!(tl[1].reply_count, 0);
    }

    #[test]
    fn time_roundtrip_and_space_of() {
        assert_eq!(format_time(1_795_168_800_123), "2026-11-20T10:00:00.123Z");
        assert_eq!(
            parse_time(&format_time(1_795_168_800_123)),
            1_795_168_800_123
        );
        assert_eq!(parse_time("kaputt"), 0);
        assert_eq!(space_of("spaces/A/messages/B"), "spaces/A");
    }

    #[test]
    fn multipart_layout() {
        let b = multipart_related("XX", "a.txt", "text/plain", b"inhalt").unwrap();
        let s = String::from_utf8(b).unwrap();
        assert!(s.starts_with("--XX\r\nContent-Type: application/json"));
        assert!(s.contains(r#"{"filename":"a.txt"}"#));
        assert!(s.contains("Content-Type: text/plain\r\n\r\ninhalt\r\n--XX--\r\n"));
    }

    #[test]
    fn person_name() {
        let p: Person = serde_json::from_str(
            r#"{"resourceName":"people/123","names":[{"displayName":"Erika Muster"}],
                "emailAddresses":[{"value":"e@example.org"}]}"#,
        )
        .unwrap();
        assert_eq!(p.display_name().as_deref(), Some("Erika Muster"));
        assert_eq!(p.email_addresses[0].value, "e@example.org");
    }
}
