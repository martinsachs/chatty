//! Schnittstelle zwischen App und Chat-Diensten. Ein Objekt je Konto.

/// rustls-Provider (ring) einmalig als Standard installieren; vor jedem HTTP-Client aufrufen (ADR 03).
pub fn init_tls() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        // Fehler heißt: schon ein Provider installiert – dann ist nichts zu tun.
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

/// Obergrenze für Profilbilder und eigene Emojis (lädt ohne Klick für jeden Absender).
pub const MAX_IMAGE: usize = 5 << 20;
/// Obergrenze für Bildvorschauen von Anhängen im Verlauf (größere bleiben reiner Download).
pub const MAX_PREVIEW: usize = 10 << 20;
/// Obergrenze für Anhänge (liegen beim Speichern vollständig im Speicher).
pub const MAX_ATTACHMENT: usize = 512 << 20;

/// Antwort lesen, aber höchstens `max` Bytes: ein bösartiger Server kann sonst Speicher fluten.
pub async fn read_limited(
    mut resp: reqwest::Response,
    max: usize,
) -> crate::error::Result<Vec<u8>> {
    let too_big = || crate::error::Error::other(format!("Antwort größer als {} MiB", max >> 20));
    if resp.content_length().is_some_and(|n| n > max as u64) {
        return Err(too_big());
    }
    let mut out = Vec::new();
    while let Some(chunk) = resp.chunk().await? {
        if out.len() + chunk.len() > max {
            return Err(too_big());
        }
        out.extend_from_slice(&chunk);
    }
    Ok(out)
}

/// Bildantwort lesen (höchstens `max` Bytes), MIME-Typ aus dem Header. Die Oberflächen prüfen den Typ selbst.
pub async fn read_image(resp: reqwest::Response, max: usize) -> crate::error::Result<Avatar> {
    let mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("image/png")
        .to_string();
    Ok(Avatar {
        data: read_limited(resp, max).await?,
        mime,
    })
}

/// Anhang als Bildvorschau im Verlauf zeigen: Bildtyp, über die API ladbar.
pub fn is_preview_image(a: &crate::model::Attachment) -> bool {
    !a.id.is_empty()
        && a.mime
            .as_deref()
            .is_some_and(|m| matches!(m, "image/png" | "image/jpeg" | "image/gif" | "image/webp"))
}

pub mod google;
pub mod mattermost;

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{mpsc, watch};

use crate::error::Result;
use crate::model::{
    Account, Attachment, BackendEvent, Message, Page, Presence, Space, UploadFile, User,
};

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

    /// Bildvorschau eines Anhangs (Anhang-ID wie bei `download`), höchstens [`MAX_PREVIEW`].
    async fn preview(&self, _attachment_id: &str) -> Result<Option<Avatar>> {
        Ok(None)
    }

    /// Sitzung bzw. Token beim Server widerrufen (beim Entfernen des Kontos). Fehler sind unkritisch.
    async fn logout(&self) -> Result<()> {
        Ok(())
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

    /// Eigener Online-Status; None, wenn das Backend keinen kennt.
    async fn own_presence(&self) -> Result<Option<Presence>> {
        Ok(None)
    }

    /// Eigenen Online-Status setzen. `text` ist der Status-Text, den andere bei den Zwischenstufen
    /// (gesprächsbereit, nicht verfügbar, beschäftigt) sehen.
    async fn set_presence(&self, _presence: Presence, _text: &str) -> Result<()> {
        Err(crate::error::Error::other("Vom Backend nicht unterstützt"))
    }

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
    /// oder der Handle abgebrochen wird. Aufruf nur innerhalb einer Tokio-Laufzeit.
    fn start_events(
        self: Arc<Self>,
        tx: EventSender,
        active_space: watch::Receiver<Option<String>>,
        focused: watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()>;
}

#[cfg(test)]
mod tests {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    /// Antwortet einmal mit `head` und danach `body_len` Bytes (bösartiger Server).
    async fn server(head: &'static str, body_len: usize) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (mut s, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = s.read(&mut buf).await;
            let _ = s.write_all(head.as_bytes()).await;
            let chunk = vec![b'x'; 64 * 1024];
            let mut sent = 0;
            while sent < body_len {
                if s.write_all(&chunk).await.is_err() {
                    return;
                }
                sent += chunk.len();
            }
        });
        format!("http://{addr}/")
    }

    #[tokio::test]
    async fn read_limited_stops_oversized_bodies() {
        super::init_tls();
        let client = reqwest::Client::new();
        // Angekündigte Länge zu groß: sofort abgelehnt.
        let url = server("HTTP/1.1 200 OK\r\nContent-Length: 999999999\r\n\r\n", 0).await;
        let resp = client.get(&url).send().await.unwrap();
        assert!(super::read_limited(resp, 1 << 20).await.is_err());
        // Ohne Länge (bis Verbindungsende), 8 MiB: nach 1 MiB abgebrochen.
        let url = server("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n", 8 << 20).await;
        let resp = client.get(&url).send().await.unwrap();
        assert!(super::read_limited(resp, 1 << 20).await.is_err());
        // Kleine Antwort: vollständig.
        let url = server("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n", 0).await;
        let resp = client.get(&url).send().await.unwrap();
        assert_eq!(super::read_limited(resp, 1 << 20).await.unwrap().len(), 0);
    }
}
