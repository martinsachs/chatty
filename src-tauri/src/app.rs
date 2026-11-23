//! Tauri-Schicht: Kommandos, Ereignisse an die WebView, Dialoge, Fenster und das `avatar://`-Schema.
//! Die Logik liegt in `chatty_core::App` (ADR 08).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use chatty_core::app::file_name;
use chatty_core::error::{Error, Result};
use chatty_core::model::{Account, Attachment, BackendEvent, Message, Page, Space};
use chatty_core::{App, AppInfo};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_notification::NotificationExt;

pub struct AppState {
    app: Arc<App>,
    /// Im Dialog gewählte Dateien; das Frontend kennt nur IDs und Namen (ADR 06).
    picked: Picked,
}

/// Für das Frontend: gewählte Datei ohne Pfad.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct PickedFile {
    pub id: u64,
    pub name: String,
}

/// Gewählte Dateipfade unter fortlaufenden IDs; gültig bis zum erfolgreichen Senden.
#[derive(Default)]
pub struct Picked(Mutex<(u64, HashMap<u64, PathBuf>)>);

impl Picked {
    fn lock(&self) -> std::sync::MutexGuard<'_, (u64, HashMap<u64, PathBuf>)> {
        self.0.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn add(&self, paths: Vec<PathBuf>) -> Vec<PickedFile> {
        let mut guard = self.lock();
        let (next, map) = &mut *guard;
        paths
            .into_iter()
            .map(|path| {
                *next += 1;
                let name = file_name(&path);
                map.insert(*next, path);
                PickedFile { id: *next, name }
            })
            .collect()
    }

    pub fn paths(&self, ids: &[u64]) -> Result<Vec<PathBuf>> {
        let guard = self.lock();
        ids.iter()
            .map(|id| {
                guard
                    .1
                    .get(id)
                    .cloned()
                    .ok_or_else(|| Error::other("Datei nicht mehr verfügbar, bitte neu wählen"))
            })
            .collect()
    }

    pub fn remove(&self, ids: &[u64]) {
        let mut guard = self.lock();
        for id in ids {
            guard.1.remove(id);
        }
    }
}

/// Erstellt den Zustand und startet die Ereignisschleife sowie die gespeicherten Konten.
pub fn setup(handle: &AppHandle) -> Result<()> {
    let (app, rx) = App::open()?;
    handle.manage(AppState {
        app: app.clone(),
        picked: Picked::default(),
    });
    tauri::async_runtime::spawn(event_loop(handle.clone(), app.clone(), rx));
    tauri::async_runtime::spawn(async move { app.restore_all() });
    size_main_window(handle);
    Ok(())
}

/// Hauptfenster auf 2/3 des Bildschirms, auf dem es aufgeht, zentriert; erst danach zeigen (kein Springen).
/// Abgelöste Fenster behalten ihre Größe.
fn size_main_window(handle: &AppHandle) {
    let Some(w) = handle.get_webview_window("main") else {
        return;
    };
    if let Ok(Some(m)) = w.current_monitor().or_else(|_| w.primary_monitor()) {
        let size = m.size();
        let _ = w.set_size(tauri::PhysicalSize::new(
            size.width * 2 / 3,
            size.height * 2 / 3,
        ));
        let _ = w.center();
    }
    let _ = w.show();
}

#[derive(Clone, Serialize)]
struct ChatEvent<'a> {
    account: &'a str,
    event: &'a BackendEvent,
}

async fn event_loop(handle: AppHandle, app: Arc<App>, mut rx: chatty_core::app::Events) {
    while let Some((account, mut event)) = rx.recv().await {
        if let Some(n) = app.handle_event(&account, &mut event) {
            let _ = handle
                .notification()
                .builder()
                .title(n.title)
                .body(n.body)
                .show();
        }
        let _ = handle.emit(
            "chat-event",
            ChatEvent {
                account: &account,
                event: &event,
            },
        );
    }
}

// ---------- Kommandos ----------

type Cmd<T> = std::result::Result<T, Error>;

#[tauri::command]
pub async fn app_info(state: State<'_, AppState>) -> Cmd<AppInfo> {
    Ok(state.app.info().await)
}

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, theme: String, notifications: bool) -> Cmd<()> {
    state.app.set_settings(theme, notifications)
}

#[tauri::command]
pub fn set_google_client(
    state: State<'_, AppState>,
    client_id: String,
    client_secret: String,
) -> Cmd<()> {
    state.app.set_google_client(&client_id, &client_secret)
}

#[tauri::command]
pub async fn add_google_account(
    state: State<'_, AppState>,
    account_id: Option<String>,
) -> Cmd<Account> {
    state
        .app
        .add_google_account(account_id, |url| {
            let _ = tauri_plugin_opener::open_url(url, None::<&str>);
        })
        .await
}

#[tauri::command]
pub async fn add_mattermost_account(
    state: State<'_, AppState>,
    account_id: Option<String>,
    server: String,
    login: String,
    password: String,
    mfa: Option<String>,
) -> Cmd<Account> {
    state
        .app
        .add_mattermost_account(account_id, &server, login, password, mfa)
        .await
}

#[tauri::command]
pub async fn remove_account(state: State<'_, AppState>, account: String) -> Cmd<()> {
    state.app.remove_account(&account).await
}

#[tauri::command]
pub async fn browse_spaces(
    state: State<'_, AppState>,
    account: String,
    query: String,
) -> Cmd<Vec<Space>> {
    state.app.browse_spaces(&account, &query).await
}

#[tauri::command]
pub async fn join_space(state: State<'_, AppState>, account: String, space: String) -> Cmd<Space> {
    state.app.join_space(&account, &space).await
}

/// Favorit bzw. Stumm setzen: beim Server, sonst lokal.
#[tauri::command]
pub async fn set_space_flag(
    state: State<'_, AppState>,
    account: String,
    space: String,
    favorite: Option<bool>,
    muted: Option<bool>,
) -> Cmd<()> {
    state
        .app
        .set_space_flag(&account, &space, favorite, muted)
        .await
}

#[tauri::command]
pub async fn custom_emojis(state: State<'_, AppState>, account: String) -> Cmd<Vec<String>> {
    state.app.custom_emojis(&account).await
}

#[tauri::command]
pub fn cached_spaces(state: State<'_, AppState>, account: String) -> Cmd<Vec<Space>> {
    state.app.cached_spaces(&account)
}

#[tauri::command]
pub async fn list_spaces(state: State<'_, AppState>, account: String) -> Cmd<Vec<Space>> {
    state.app.list_spaces(&account).await
}

#[tauri::command]
pub fn cached_messages(
    state: State<'_, AppState>,
    account: String,
    space: String,
) -> Cmd<Vec<Message>> {
    state.app.cached_messages(&account, &space)
}

#[tauri::command]
pub async fn list_messages(
    state: State<'_, AppState>,
    account: String,
    space: String,
    cursor: Option<String>,
) -> Cmd<Page> {
    state.app.list_messages(&account, &space, cursor).await
}

#[tauri::command]
pub async fn list_thread(
    state: State<'_, AppState>,
    account: String,
    space: String,
    thread: String,
) -> Cmd<Vec<Message>> {
    state.app.list_thread(&account, &space, &thread).await
}

#[tauri::command]
pub async fn send_message(
    state: State<'_, AppState>,
    account: String,
    space: String,
    text: String,
    thread: Option<String>,
    files: Vec<u64>,
) -> Cmd<Message> {
    let paths = state.picked.paths(&files)?;
    let msg = state
        .app
        .send(&account, &space, &text, thread.as_deref(), &paths)
        .await?;
    state.picked.remove(&files);
    Ok(msg)
}

/// Öffnen-Dialog im Backend; liefert IDs statt Pfaden (ADR 06).
#[tauri::command]
pub async fn pick_files(app: AppHandle, state: State<'_, AppState>) -> Cmd<Vec<PickedFile>> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_files(move |paths| {
        let _ = tx.send(paths);
    });
    let paths: Vec<PathBuf> = rx
        .await
        .ok()
        .flatten()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .collect();
    Ok(state.picked.add(paths))
}

/// Speichern-Dialog im Backend, schreibt den Anhang selbst. `false` = abgebrochen.
#[tauri::command]
pub async fn save_attachment(
    app: AppHandle,
    state: State<'_, AppState>,
    account: String,
    attachment: Attachment,
) -> Cmd<bool> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_file_name(chatty_core::model::safe_file_name(&attachment.name))
        .save_file(move |path| {
            let _ = tx.send(path);
        });
    let Some(path) = rx.await.ok().flatten() else {
        return Ok(false);
    };
    let path = path.into_path().map_err(|e| Error::other(e.to_string()))?;
    state
        .app
        .save_attachment(&account, &attachment, &path)
        .await?;
    Ok(true)
}

#[tauri::command]
pub async fn edit_message(
    state: State<'_, AppState>,
    account: String,
    space: String,
    id: String,
    text: String,
) -> Cmd<Message> {
    state.app.edit(&account, &space, &id, &text).await
}

#[tauri::command]
pub async fn delete_message(
    state: State<'_, AppState>,
    account: String,
    space: String,
    id: String,
) -> Cmd<()> {
    state.app.delete(&account, &space, &id).await
}

#[tauri::command]
pub async fn react(
    state: State<'_, AppState>,
    account: String,
    space: String,
    id: String,
    emoji: String,
    add: bool,
) -> Cmd<()> {
    state.app.react(&account, &space, &id, &emoji, add).await
}

#[tauri::command]
pub async fn search(
    state: State<'_, AppState>,
    account: String,
    query: String,
) -> Cmd<Vec<Message>> {
    state.app.search(&account, &query).await
}

#[tauri::command]
pub async fn mark_read(state: State<'_, AppState>, account: String, space: String) -> Cmd<()> {
    state.app.mark_read(&account, &space).await
}

/// Meldet den offenen Space (Polling-Priorität, Benachrichtigungen).
#[tauri::command]
pub async fn set_active(
    state: State<'_, AppState>,
    account: String,
    space: Option<String>,
) -> Cmd<()> {
    state.app.set_active(&account, space).await;
    Ok(())
}

/// Fensterfokus neu bestimmen (bei jedem Fokus- oder Schließ-Ereignis eines Fensters). Ausgewertet wird
/// der Zustand aller Fenster, damit die Reihenfolge der Ereignisse beim Fensterwechsel keine Rolle spielt.
pub fn window_focus_changed(app: &AppHandle) {
    let any = app
        .webview_windows()
        .values()
        .any(|w| w.is_focused().unwrap_or(false));
    if let Some(state) = app.try_state::<AppState>() {
        state.app.set_focused(any);
    }
}

/// Öffnet einen Space in einem eigenen, kleinen Fenster.
#[tauri::command]
pub fn open_space_window(app: AppHandle, account: String, space: String, title: String) -> Cmd<()> {
    let label = format!("space-{}", hex::encode(format!("{account}/{space}")));
    if let Some(w) = app.get_webview_window(&label) {
        let _ = w.set_focus();
        return Ok(());
    }
    // Hash-Route: SvelteKit sieht Pfad "/", das Frontend liest den Hash.
    let url = format!("#popout/{}/{}", urlencode(&account), urlencode(&space));
    WebviewWindowBuilder::new(&app, label, WebviewUrl::App(url.into()))
        .title(title)
        .inner_size(480.0, 640.0)
        .build()
        .map_err(|e| Error::other(e.to_string()))?;
    Ok(())
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// URL-Schema `avatar://localhost/<konto>/<nutzer>` (Windows: `http://avatar.localhost/...`):
/// lädt Profilbilder über das Backend, weil der Server sie nur mit Token herausgibt.
pub fn avatar_protocol(
    ctx: tauri::UriSchemeContext<'_, tauri::Wry>,
    request: tauri::http::Request<Vec<u8>>,
    responder: tauri::UriSchemeResponder,
) {
    let app = ctx.app_handle().clone();
    let path = request.uri().path().trim_start_matches('/').to_string();
    tauri::async_runtime::spawn(async move {
        let avatar = match app.try_state::<AppState>() {
            Some(state) => state.app.avatar(&path).await,
            None => None,
        };
        let response = match avatar {
            // Nur Bildtypen ausliefern (kein text/html oder SVG mit Skript vom Server), kein Sniffing.
            Some(a) => tauri::http::Response::builder()
                .header(
                    "Content-Type",
                    if matches!(
                        a.mime.as_str(),
                        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
                    ) {
                        a.mime.as_str()
                    } else {
                        "application/octet-stream"
                    },
                )
                .header("X-Content-Type-Options", "nosniff")
                .header("Content-Security-Policy", "default-src 'none'")
                .header("Cache-Control", "max-age=86400")
                .body(a.data),
            None => tauri::http::Response::builder()
                .status(404)
                .body(Vec::new()),
        };
        responder.respond(response.unwrap_or_default());
    });
}

#[cfg(test)]
mod picked_tests {
    use super::*;

    #[test]
    fn picked_ids_until_removed() {
        let picked = Picked::default();
        let files = picked.add(vec![
            PathBuf::from("/tmp/a.txt"),
            PathBuf::from("/tmp/b.png"),
        ]);
        assert_eq!(
            files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
            ["a.txt", "b.png"]
        );
        let ids: Vec<u64> = files.iter().map(|f| f.id).collect();
        assert_eq!(picked.paths(&ids).unwrap().len(), 2);
        assert!(picked.paths(&[999]).is_err(), "unbekannte ID");
        assert_eq!(
            picked.paths(&ids).unwrap().len(),
            2,
            "lesen verbraucht nicht"
        );
        picked.remove(&ids);
        assert!(picked.paths(&ids[..1]).is_err(), "nach dem Senden entfernt");
    }
}
