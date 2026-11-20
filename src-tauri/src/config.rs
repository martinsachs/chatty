//! `chatty.toml` im OS-Konfigurationsverzeichnis und Kontenliste `accounts.json`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::model::Account;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct GoogleConfig {
    pub client_id: String,
    /// Bei Desktop-Clients laut Google nicht vertraulich, wird beim Token-Tausch mitgesendet.
    #[serde(default)]
    pub client_secret: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct PollConfig {
    /// Offener Space, Sekunden.
    pub active_secs: u64,
    /// Space-Liste (Ungelesen), Sekunden.
    pub spaces_secs: u64,
}

impl Default for PollConfig {
    fn default() -> Self {
        Self {
            active_secs: 5,
            spaces_secs: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    /// "mattermost" oder "slack".
    pub theme: String,
    /// OS-Benachrichtigungen bei DMs und Erwähnungen.
    pub notifications: bool,
    pub google: Option<GoogleConfig>,
    pub poll: PollConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "mattermost".into(),
            notifications: true,
            google: None,
            poll: PollConfig::default(),
        }
    }
}

/// Verzeichnis für Konfiguration (Linux: ~/.config/chatty).
pub fn config_dir() -> Result<PathBuf> {
    let dir = dirs::config_dir()
        .ok_or_else(|| Error::other("Kein Konfigurationsverzeichnis gefunden"))?
        .join("chatty");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Verzeichnis für den Nachrichten-Cache (Linux: ~/.local/share/chatty).
pub fn data_dir() -> Result<PathBuf> {
    let dir = dirs::data_dir()
        .ok_or_else(|| Error::other("Kein Datenverzeichnis gefunden"))?
        .join("chatty");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn load_config(dir: &Path) -> Result<Config> {
    let path = dir.join("chatty.toml");
    if !path.exists() {
        return Ok(Config::default());
    }
    let text = std::fs::read_to_string(&path)?;
    toml::from_str(&text).map_err(|e| Error::other(format!("{}: {e}", path.display())))
}

pub fn save_config(dir: &Path, cfg: &Config) -> Result<()> {
    let text = toml::to_string_pretty(cfg).map_err(|e| Error::other(e.to_string()))?;
    let path = dir.join("chatty.toml");
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create(true).truncate(true);
    // Enthält ggf. das Google-Client-Secret: nur für den Nutzer lesbar, Rechte vor dem Inhalt setzen.
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut opts, 0o600);
    let mut file = opts.open(&path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    std::io::Write::write_all(&mut file, text.as_bytes())?;
    Ok(())
}

pub fn load_accounts(dir: &Path) -> Result<Vec<Account>> {
    let path = dir.join("accounts.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    Ok(serde_json::from_str(&std::fs::read_to_string(path)?)?)
}

pub fn save_accounts(dir: &Path, accounts: &[Account]) -> Result<()> {
    std::fs::write(
        dir.join("accounts.json"),
        serde_json::to_string_pretty(accounts)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::BackendKind;

    #[test]
    fn config_roundtrip_and_defaults() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(load_config(dir.path()).unwrap(), Config::default());

        std::fs::write(
            dir.path().join("chatty.toml"),
            "theme = \"slack\"\n[google]\nclient_id = \"abc\"\n",
        )
        .unwrap();
        let cfg = load_config(dir.path()).unwrap();
        assert_eq!(cfg.theme, "slack");
        assert_eq!(cfg.google.as_ref().unwrap().client_id, "abc");
        assert_eq!(cfg.poll, PollConfig::default());

        save_config(dir.path(), &cfg).unwrap();
        assert_eq!(load_config(dir.path()).unwrap(), cfg);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("chatty.toml"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[test]
    fn accounts_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        assert!(load_accounts(dir.path()).unwrap().is_empty());
        let acc = vec![Account {
            id: "a1".into(),
            kind: BackendKind::Mattermost,
            server: Some("https://mm.example.org".into()),
            label: "max".into(),
            login: Some("max".into()),
        }];
        save_accounts(dir.path(), &acc).unwrap();
        assert_eq!(load_accounts(dir.path()).unwrap(), acc);
    }
}
