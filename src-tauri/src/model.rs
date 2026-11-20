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
