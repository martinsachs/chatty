//! Backend-neutrale Datentypen. Zeiten sind Millisekunden seit Unix-Epoche.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendKind {
    Google,
    Mattermost,
}

/// Ein angemeldetes Konto. Enthält keine Geheimnisse (die liegen im Keyring).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: String,
    pub kind: BackendKind,
    /// Server-URL (Mattermost); bei Google leer.
    #[serde(default)]
    pub server: Option<String>,
    /// Anzeige, z. B. Benutzername oder E-Mail.
    pub label: String,
    /// Benutzername für den Mattermost-Login (für das Formular beim Neu-Anmelden).
    #[serde(default)]
    pub login: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpaceKind {
    /// Kanal / benannter Space
    Channel,
    /// Direktnachricht 1:1
    Direct,
    /// Gruppen-DM ohne Namen
    Group,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub kind: SpaceKind,
    pub unread: bool,
    pub mention_count: u32,
    pub last_active: i64,
    /// Nachrichten insgesamt (Sortierung), 0 wenn das Backend es nicht liefert.
    #[serde(default)]
    pub msg_count: i64,
    #[serde(default)]
    pub favorite: bool,
    /// Stumm: keine Benachrichtigung und kein Fettdruck, außer bei Erwähnung.
    #[serde(default)]
    pub muted: bool,
    /// Status-Emoji des DM-Partners (Unicode oder ":name:" für eigene Server-Emojis).
    #[serde(default)]
    pub status_emoji: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reaction {
    /// Unicode-Emoji, z. B. "👍". Unbekannte Mattermost-Namen als ":name:".
    pub emoji: String,
    pub count: u32,
    /// Eigene Reaktion vorhanden (bei Google nicht immer bekannt → false).
    pub me: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attachment {
    /// Backend-spezifische Referenz zum Herunterladen.
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub mime: Option<String>,
    #[serde(default)]
    pub size: Option<u64>,
    /// Externer Link (z. B. Google-Drive-Datei), wenn kein Download über die API möglich ist.
    #[serde(default)]
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    pub space_id: String,
    /// Thread-Kennung; Nachrichten mit gleicher thread_id gehören zusammen.
    #[serde(default)]
    pub thread_id: Option<String>,
    /// true = Antwort in einem Thread (nicht im Hauptverlauf anzeigen).
    pub is_reply: bool,
    pub sender: User,
    pub text: String,
    pub created: i64,
    #[serde(default)]
    pub edited: Option<i64>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    /// Anzahl Antworten (0 = unbekannt oder keine).
    #[serde(default)]
    pub reply_count: u32,
    /// Nachricht erwähnt den angemeldeten Nutzer.
    #[serde(default)]
    pub mentions_me: bool,
    /// Vom angemeldeten Nutzer selbst.
    #[serde(default)]
    pub mine: bool,
}

/// Antwortzähler der Startnachrichten aus den mitgeladenen Antworten (Mattermost liefert `reply_count` nur bei
/// zusammengefassten Threads). Der Verlauf wird lückenlos von der neuesten Nachricht an geladen, daher sind die
/// Antworten jeder geladenen Startnachricht dabei. Nur erhöhen: Backends mit eigenem Zähler (Google) bleiben gleich.
pub fn count_replies(messages: &mut [Message]) {
    let mut counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for m in messages.iter().filter(|m| m.is_reply) {
        if let Some(t) = &m.thread_id {
            *counts.entry(t.clone()).or_default() += 1;
        }
    }
    for m in messages.iter_mut().filter(|m| !m.is_reply) {
        if let Some(n) = m.thread_id.as_ref().and_then(|t| counts.get(t)) {
            m.reply_count = m.reply_count.max(*n);
        }
    }
}

/// Seite des Verlaufs, neueste zuerst geladen, `messages` aufsteigend nach Zeit sortiert.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Page {
    pub messages: Vec<Message>,
    /// Cursor für ältere Nachrichten; None = Anfang erreicht.
    pub next_cursor: Option<String>,
}

/// Datei zum Hochladen.
#[derive(Debug, Clone)]
pub struct UploadFile {
    pub name: String,
    pub mime: String,
    pub data: Vec<u8>,
}

/// Ereignisse aus dem Backend (Polling oder WebSocket).
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BackendEvent {
    MessageCreated {
        message: Message,
    },
    MessageUpdated {
        message: Message,
    },
    MessageDeleted {
        space_id: String,
        message_id: String,
    },
    SpaceUpdated {
        space: Space,
    },
    /// Verbindungszustand (z. B. WebSocket getrennt, Token abgelaufen).
    Status {
        online: bool,
        detail: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(id: &str, thread: &str, reply: bool, count: u32) -> Message {
        Message {
            id: id.into(),
            space_id: "s".into(),
            thread_id: Some(thread.into()),
            is_reply: reply,
            sender: User {
                id: "u".into(),
                name: "u".into(),
                avatar_url: None,
            },
            text: String::new(),
            created: 0,
            edited: None,
            reactions: vec![],
            attachments: vec![],
            reply_count: count,
            mentions_me: false,
            mine: false,
        }
    }

    #[test]
    fn counts_loaded_replies_only_upwards() {
        let mut m = vec![
            msg("r", "r", false, 0),
            msg("a", "r", true, 0),
            msg("b", "r", true, 0),
            msg("g", "g", false, 5),
        ];
        count_replies(&mut m);
        assert_eq!(m[0].reply_count, 2);
        assert_eq!(m[3].reply_count, 5, "eigener Zähler bleibt");
    }
}
