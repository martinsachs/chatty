//! Oberflächen-Einstellungen der nativen Variante (`native.json` im Konfigurationsverzeichnis), entspricht dem
//! `localStorage` der Tauri-Oberfläche: Breite der Seitenleiste, Sortierung, Schriftgröße.

use serde::{Deserialize, Serialize};

pub const SIDEBAR_MIN: f32 = 160.0;
pub const SIDEBAR_MAX: f32 = 520.0;
pub const SIDEBAR_DEFAULT: f32 = 240.0;
/// Stufen der Schriftgröße (Skalierung der ganzen Oberfläche), wie die Tauri-Variante.
pub const ZOOMS: [f32; 8] = [0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    /// Letzte Nachricht zuerst.
    #[default]
    Recent,
    Name,
    /// Meiste Nachrichten zuerst.
    Count,
}

impl Sort {
    pub const ALL: [Sort; 3] = [Sort::Recent, Sort::Name, Sort::Count];
}

impl std::fmt::Display for Sort {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Sort::Recent => "Neueste",
            Sort::Name => "Name",
            Sort::Count => "Meiste",
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub sidebar_width: f32,
    pub sort: Sort,
    pub zoom: f32,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            sidebar_width: SIDEBAR_DEFAULT,
            sort: Sort::Recent,
            zoom: 1.0,
        }
    }
}

fn path() -> Option<std::path::PathBuf> {
    chatty_core::config::config_dir()
        .ok()
        .map(|d| d.join("native.json"))
}

pub fn load() -> Prefs {
    let mut p: Prefs = path()
        .and_then(|p| std::fs::read(p).ok())
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    p.sidebar_width = p.sidebar_width.clamp(SIDEBAR_MIN, SIDEBAR_MAX);
    if !ZOOMS.contains(&p.zoom) {
        p.zoom = 1.0;
    }
    p
}

/// Fehler beim Speichern sind unkritisch (nächster Start mit Standardwerten).
pub fn save(p: &Prefs) {
    if let (Some(path), Ok(json)) = (path(), serde_json::to_vec_pretty(p)) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_fields_use_defaults() {
        let p: Prefs = serde_json::from_str(r#"{"sort":"name"}"#).unwrap();
        assert_eq!(p.sort, Sort::Name);
        assert_eq!(p.sidebar_width, SIDEBAR_DEFAULT);
        assert_eq!(p.zoom, 1.0);
    }
}
