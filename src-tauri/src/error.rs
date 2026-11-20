use serde::{Serialize, Serializer};

/// Gemeinsamer Fehlertyp für Kern und Backends.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Netzwerkfehler: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Anmeldung fehlgeschlagen: {0}")]
    Auth(String),
    #[error("Server antwortet {status}: {body}")]
    Api { status: u16, body: String },
    #[error("Datenbankfehler: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("Schlüsselbund: {0}")]
    Keyring(#[from] keyring::Error),
    #[error("Dateifehler: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn other(msg: impl Into<String>) -> Self {
        Error::Other(msg.into())
    }
}

// Tauri-Kommandos geben Fehler als Text ans Frontend.
impl Serialize for Error {
    fn serialize<S: Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}
