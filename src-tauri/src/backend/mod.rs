//! Schnittstelle zwischen App und Chat-Diensten. Ein Objekt je Konto.

/// rustls-Provider (ring) einmalig als Standard installieren; vor jedem HTTP-Client aufrufen (ADR 03).
pub fn init_tls() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // Fehler heißt: schon ein Provider installiert – dann ist nichts zu tun.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

pub mod google;
pub mod mattermost;

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, watch};

use crate::error::Result;
use crate::model::{Account, Attachment, BackendEvent, Message, Page, Space, UploadFile, User};

/// Ereigniskanal: (Konto-ID, Ereignis).
pub type EventSender = mpsc::UnboundedSender<(String, BackendEvent)>;

/// Bilddaten mit MIME-Typ.
#[derive(Debug, Clone)]
pub struct Avatar {
    pub data: Vec<u8>,
    pub mime: String,
}

#[async_trait]
pub trait ChatBackend: Send + Sync {
    fn account(&self) -> &Account;

    /// Angemeldeter Nutzer.
    async fn me(&self) -> Result<User>;

    async fn list_spaces(&self) -> Result<Vec<Space>>;

    /// Hauptverlauf eines Space (ohne Thread-Antworten). `cursor` aus vorheriger Seite.
    async fn list_messages(&self, space_id: &str, cursor: Option<String>) -> Result<Page>;

    /// Alle Nachrichten eines Threads inkl. Startnachricht, aufsteigend.
    async fn list_thread(&self, space_id: &str, thread_id: &str) -> Result<Vec<Message>>;

    async fn send(
        &self,
        space_id: &str,
        text: &str,
        thread_id: Option<&str>,
        files: Vec<UploadFile>,
    ) -> Result<Message>;

    async fn edit(&self, space_id: &str, message_id: &str, text: &str) -> Result<Message>;

    async fn delete(&self, space_id: &str, message_id: &str) -> Result<()>;

    /// `emoji` als Unicode.
    async fn react(&self, space_id: &str, message_id: &str, emoji: &str) -> Result<()>;

    async fn unreact(&self, space_id: &str, message_id: &str, emoji: &str) -> Result<()>;

    async fn download(&self, attachment: &Attachment) -> Result<Vec<u8>>;

    /// Profilbild eines Nutzers, falls das Backend es nur authentifiziert liefert.
    async fn avatar(&self, _user_id: &str) -> Result<Option<Avatar>> {
        Ok(None)
    }

    /// Namen eigener Server-Emojis (ohne Doppelpunkte).
    async fn custom_emojis(&self) -> Result<Vec<String>> {
        Ok(Vec::new())
    }

    /// Bild eines eigenen Server-Emojis.
    async fn custom_emoji(&self, _name: &str) -> Result<Option<Avatar>> {
        Ok(None)
    }

    /// Serverseitige Volltextsuche über alle Spaces des Kontos.
    async fn search(&self, query: &str) -> Result<Vec<Message>>;

    async fn mark_read(&self, space_id: &str) -> Result<()>;

    /// Beitretbare Spaces (noch nicht Mitglied), passend zu `query`.
    async fn browse_spaces(&self, _query: &str) -> Result<Vec<Space>> {
        Err(crate::error::Error::other("Vom Backend nicht unterstützt"))
    }

    /// Space beitreten; liefert ihn für die Seitenleiste.
    async fn join_space(&self, _space_id: &str) -> Result<Space> {
        Err(crate::error::Error::other("Vom Backend nicht unterstützt"))
    }

    /// Favorit setzen. `Ok(false)` = Backend kann es nicht, chatty merkt es sich lokal.
    async fn set_favorite(&self, _space_id: &str, _on: bool) -> Result<bool> {
        Ok(false)
    }

    /// Stumm schalten. `Ok(false)` = Backend kann es nicht, chatty merkt es sich lokal.
    async fn set_muted(&self, _space_id: &str, _on: bool) -> Result<bool> {
        Ok(false)
    }

    /// Startet Polling bzw. WebSocket. `active_space` ist der gerade offene Space
    /// (Google pollt ihn häufiger). Der Task endet, wenn `tx` geschlossen wird
    /// oder der Handle abgebrochen wird.
    fn start_events(
        self: Arc<Self>,
        tx: EventSender,
        active_space: watch::Receiver<Option<String>>,
    ) -> tauri::async_runtime::JoinHandle<()>;
}
