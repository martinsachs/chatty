//! WebSocket-Ereignisse (`/api/v4/websocket`) mit automatischem Reconnect.
//!
//! Das Upgrade läuft über den reqwest-Client des Backends (HTTP/1.1). So nutzt auch `wss://`
//! dessen TLS-Konfiguration; der eingebaute rustls-Connector von tokio-tungstenite würde ohne
//! installierten Krypto-Provider abbrechen (ring und aws-lc-rs sind beide im Abhängigkeitsbaum).

use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::Engine;
use futures_util::{SinkExt, StreamExt};
use reqwest::{Method, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::Message as WsMessage;
use tokio_tungstenite::tungstenite::protocol::Role;

use super::MattermostBackend;
use super::api::{ApiPost, ApiUser, text_mentions};
use crate::backend::EventSender;
use crate::error::{Error, Result};
use crate::model::BackendEvent;

const MAX_BACKOFF: Duration = Duration::from_secs(60);
/// Eigener Ping wie die Mattermost-Webapp: Proxys vor dem Server kappen sonst still liegende
/// Verbindungen (beobachtet: Reset nach 30–90 s ohne Verkehr).
const PING_INTERVAL: Duration = Duration::from_secs(20);
/// Auf den Ping antwortet der Server; bleibt länger alles still, gilt die Verbindung als tot.
const READ_TIMEOUT: Duration = Duration::from_secs(60);
/// Höchstdauer für das Verarbeiten eines Ereignisses (REST-Nachladen); danach verfällt es.
const HANDLE_TIMEOUT: Duration = Duration::from_secs(10);
/// Erst eine Sitzung dieser Länge gilt als stabil: Abrisse danach werden still neu verbunden.
const STABLE_SESSION: Duration = Duration::from_secs(30);

enum End {
    /// Ereigniskanal geschlossen → Task beenden.
    Closed,
    /// Verbindung verloren; `bool` = Sitzung lief stabil (still neu verbinden, Backoff zurücksetzen).
    Lost(String, bool),
}

pub async fn run(backend: Arc<MattermostBackend>, tx: EventSender) {
    let id = backend.account.id.clone();
    let mut delay = Duration::from_secs(1);
    loop {
        let (detail, was_connected) = match session(&backend, &tx).await {
            End::Closed => return,
            End::Lost(detail, connected) => (detail, connected),
        };
        // Abriss einer laufenden Verbindung: erst still neu verbinden, Offline nur wenn das scheitert.
        if was_connected {
            delay = Duration::from_secs(1);
            tokio::time::sleep(delay).await;
            continue;
        }
        let status = BackendEvent::Status {
            online: false,
            detail: Some(detail),
        };
        if tx.send((id.clone(), status)).is_err() {
            return;
        }
        tokio::time::sleep(delay).await;
        delay = (delay * 2).min(MAX_BACKOFF);
    }
}

fn ws_key() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    base64::engine::general_purpose::STANDARD.encode(nanos.to_le_bytes())
}

async fn connect(backend: &MattermostBackend) -> Result<WebSocketStream<reqwest::Upgraded>> {
    let resp = backend
        .req(Method::GET, "/websocket")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", ws_key())
        .send()
        .await?;
    let resp = super::check(resp).await?;
    if resp.status() != StatusCode::SWITCHING_PROTOCOLS {
        return Err(Error::other(format!(
            "WebSocket-Upgrade abgelehnt: {}",
            resp.status()
        )));
    }
    let upgraded = resp.upgrade().await?;
    Ok(WebSocketStream::from_raw_socket(upgraded, Role::Client, None).await)
}

async fn session(backend: &MattermostBackend, tx: &EventSender) -> End {
    let id = &backend.account.id;
    let mut ws = match connect(backend).await {
        Ok(ws) => ws,
        Err(e) => return End::Lost(e.to_string(), false),
    };
    let auth = json!({
        "seq": 1,
        "action": "authentication_challenge",
        "data": { "token": backend.token },
    });
    if let Err(e) = ws.send(WsMessage::text(auth.to_string())).await {
        return End::Lost(e.to_string(), false);
    }
    if tx
        .send((
            id.clone(),
            BackendEvent::Status {
                online: true,
                detail: None,
            },
        ))
        .is_err()
    {
        return End::Closed;
    }
    let started = tokio::time::Instant::now();
    let stable = || started.elapsed() >= STABLE_SESSION;
    let mut seq = 1u64;
    let mut ping =
        tokio::time::interval_at(tokio::time::Instant::now() + PING_INTERVAL, PING_INTERVAL);
    let mut deadline = tokio::time::Instant::now() + READ_TIMEOUT;
    loop {
        let next = tokio::select! {
            _ = ping.tick() => {
                seq += 1;
                let msg = json!({ "seq": seq, "action": "ping" }).to_string();
                if let Err(e) = ws.send(WsMessage::text(msg)).await {
                    return End::Lost(e.to_string(), stable());
                }
                continue;
            }
            () = tokio::time::sleep_until(deadline) => {
                return End::Lost("WebSocket: Zeitüberschreitung".into(), stable());
            }
            next = ws.next() => next,
        };
        let msg = match next {
            None => return End::Lost("WebSocket geschlossen".into(), stable()),
            Some(Err(e)) => return End::Lost(e.to_string(), stable()),
            Some(Ok(m)) => m,
        };
        deadline = tokio::time::Instant::now() + READ_TIMEOUT;
        let WsMessage::Text(text) = msg else { continue };
        let Ok(event) = serde_json::from_str::<WsEvent>(text.as_str()) else {
            continue;
        };
        // Fehler oder Zeitüberschreitung beim Nachladen einzelner Ereignisse ignorieren; der nächste
        // Abruf holt sie nach. Die Frist hält Pings und Lesen nicht länger als HANDLE_TIMEOUT auf.
        let events = tokio::time::timeout(HANDLE_TIMEOUT, handle(backend, &event))
            .await
            .ok()
            .and_then(Result::ok)
            .unwrap_or_default();
        for ev in events {
            if tx.send((id.clone(), ev)).is_err() {
                return End::Closed;
            }
        }
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct WsEvent {
    event: String,
    data: Value,
}

/// `data.<key>` ist bei Mattermost ein JSON-String, der erneut geparst werden muss.
fn nested<T: for<'de> Deserialize<'de>>(data: &Value, key: &str) -> Result<T> {
    let raw = data
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| Error::other(format!("Ereignis ohne {key}")))?;
    Ok(serde_json::from_str(raw)?)
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ReactionRef {
    post_id: String,
}

async fn handle(backend: &MattermostBackend, ev: &WsEvent) -> Result<Vec<BackendEvent>> {
    let data = &ev.data;
    match ev.event.as_str() {
        "posted" => {
            let post: ApiPost = nested(data, "post")?;
            if post.is_system() {
                return Ok(Vec::new());
            }
            let mentioned = data
                .get("mentions")
                .and_then(Value::as_str)
                .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
                .is_some_and(|ids| ids.contains(&backend.me.id));
            let mut message = backend.map_one(&post).await?;
            message.mentions_me |=
                !message.mine && (mentioned || text_mentions(&message.text, &backend.me.username));
            // Bekannter Kanal: ohne REST fortschreiben; unbekannter (z. B. neue DM) einmal laden.
            let space = match backend.advance_space(&message) {
                Some(space) => Some(space),
                None => backend.fetch_space(&post.channel_id).await.ok(),
            };
            let mut out = vec![BackendEvent::MessageCreated { message }];
            out.extend(space.map(|space| BackendEvent::SpaceUpdated { space }));
            Ok(out)
        }
        "post_edited" => {
            let post: ApiPost = nested(data, "post")?;
            Ok(vec![BackendEvent::MessageUpdated {
                message: backend.map_one(&post).await?,
            }])
        }
        "post_deleted" => {
            let post: ApiPost = nested(data, "post")?;
            Ok(vec![BackendEvent::MessageDeleted {
                space_id: post.channel_id,
                message_id: post.id,
            }])
        }
        "reaction_added" | "reaction_removed" => {
            let r: ReactionRef = nested(data, "reaction")?;
            let post = backend.fetch_post(&r.post_id).await?;
            Ok(vec![BackendEvent::MessageUpdated {
                message: backend.map_one(&post).await?,
            }])
        }
        "channel_viewed" => {
            let channel = data
                .get("channel_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Ok(vec![BackendEvent::SpaceUpdated {
                space: backend.fetch_space(channel).await?,
            }])
        }
        "multiple_channels_viewed" => {
            let ids: Vec<String> = data
                .get("channel_times")
                .and_then(Value::as_object)
                .map(|m| m.keys().cloned().collect())
                .unwrap_or_default();
            let mut out = Vec::new();
            for id in ids {
                out.push(BackendEvent::SpaceUpdated {
                    space: backend.fetch_space(&id).await?,
                });
            }
            Ok(out)
        }
        // Favorit in anderem Client geändert
        "preferences_changed" | "preferences_deleted" => {
            let prefs: Vec<Value> = nested(data, "preferences")?;
            let mut out = Vec::new();
            for p in prefs {
                if p.get("category").and_then(Value::as_str) != Some("favorite_channel") {
                    continue;
                }
                if let Some(id) = p.get("name").and_then(Value::as_str) {
                    out.push(BackendEvent::SpaceUpdated {
                        space: backend.fetch_space(id).await?,
                    });
                }
            }
            Ok(out)
        }
        // Profil geändert, z. B. Status-Emoji: DM-Eintrag aktualisieren.
        "user_updated" => {
            let Some(user) = data
                .get("user")
                .cloned()
                .and_then(|u| serde_json::from_value::<ApiUser>(u).ok())
            else {
                return Ok(Vec::new());
            };
            match backend.user_updated(user) {
                Some(dm) => Ok(vec![BackendEvent::SpaceUpdated {
                    space: backend.fetch_space(&dm).await?,
                }]),
                None => Ok(Vec::new()),
            }
        }
        // Stummschaltung in anderem Client geändert
        "channel_member_updated" => {
            let member: Value = nested(data, "channelMember")?;
            let id = member
                .get("channel_id")
                .and_then(Value::as_str)
                .unwrap_or_default();
            Ok(vec![BackendEvent::SpaceUpdated {
                space: backend.fetch_space(id).await?,
            }])
        }
        _ => Ok(Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_post_string() -> Result<()> {
        let ev: WsEvent = serde_json::from_str(
            r#"{"event":"posted","seq":3,"data":{
                "post":"{\"id\":\"p1\",\"channel_id\":\"c1\",\"message\":\"hi\"}",
                "mentions":"[\"me1\"]"},"broadcast":{"channel_id":"c1"}}"#,
        )?;
        assert_eq!(ev.event, "posted");
        let post: ApiPost = nested(&ev.data, "post")?;
        assert_eq!(post.id, "p1");
        assert_eq!(post.channel_id, "c1");
        assert!(nested::<ApiPost>(&ev.data, "missing").is_err());
        Ok(())
    }

    #[test]
    fn ws_key_is_16_bytes() -> std::result::Result<(), base64::DecodeError> {
        let raw = base64::engine::general_purpose::STANDARD.decode(ws_key())?;
        assert_eq!(raw.len(), 16);
        Ok(())
    }
}
