//! chatty-Kern: Backends, Cache, Konfiguration und Kontoverwaltung, ohne Oberfläche.
//! Die Tauri-Variante (`chatty`) und die native Variante (`chatty-native`) bauen darauf auf (ADR 08).

pub mod app;
pub mod backend;
pub mod config;
pub mod error;
pub mod gif;
pub mod model;
pub mod secrets;
pub mod store;

pub use app::{AccountInfo, Alert, App, AppInfo, Notification};
