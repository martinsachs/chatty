//! App-Zustand, Ereignisschleife und Tauri-Kommandos.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindowBuilder};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::{RwLock, mpsc, watch};

use crate::backend::google::GoogleBackend;
use crate::backend::mattermost::MattermostBackend;
use crate::backend::{Avatar, ChatBackend, EventSender};
use crate::config::{self, Config, GoogleConfig};
use crate::error::{Error, Result};
use crate::model::{
    Account, Attachment, BackendEvent, BackendKind, Message, Page, Space, SpaceKind, UploadFile,
};
use crate::secrets;
use crate::store::Store;

struct Running {
    backend: Arc<dyn ChatBackend>,
    active: watch::Sender<Option<String>>,
    task: tauri::async_runtime::JoinHandle<()>,
}

pub struct AppState {
    cfg_dir: PathBuf,
    config: Mutex<Config>,
    accounts: Mutex<Vec<Account>>,
    running: RwLock<HashMap<String, Running>>,
    store: Mutex<Store>,
    tx: EventSender,
    /// Offener Space je Konto (für Benachrichtigungen).
    active: Mutex<HashMap<String, String>>,
    /// Profilbilder je "konto/nutzer"; die Zelle lädt jedes Bild nur einmal.
    avatars: Mutex<HashMap<String, AvatarCell>>,
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

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "datei".into())
}

type AvatarCell = Arc<tokio::sync::OnceCell<Option<Avatar>>>;

impl AppState {
    fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.store.lock().unwrap_or_else(|p| p.into_inner())
    }

    async fn backend(&self, account: &str) -> Result<Arc<dyn ChatBackend>> {
        self.running
            .read()
            .await
            .get(account)
            .map(|r| r.backend.clone())
            .ok_or_else(|| Error::Auth("Konto nicht angemeldet".into()))
    }

    async fn start(&self, backend: Arc<dyn ChatBackend>) {
        let id = backend.account().id.clone();
        let (active, active_rx) = watch::channel(None);
        let task = backend.clone().start_events(self.tx.clone(), active_rx);
        if let Some(old) = self.running.write().await.insert(
            id,
            Running {
                backend,
                active,
                task,
            },
        ) {
            old.task.abort();
        }
    }

    fn upsert_account(&self, account: Account) -> Result<()> {
        let mut list = self.accounts.lock().unwrap_or_else(|p| p.into_inner());
        match list.iter_mut().find(|a| a.id == account.id) {
            Some(a) => *a = account,
            None => list.push(account),
        }
        config::save_accounts(&self.cfg_dir, &list)
    }
}

/// Erstellt den Zustand und startet die Ereignisschleife sowie die gespeicherten Konten.
pub fn setup(app: &AppHandle) -> Result<()> {
    let cfg_dir = config::config_dir()?;
    let config = config::load_config(&cfg_dir)?;
    let accounts = config::load_accounts(&cfg_dir)?;
    let store = Store::open(&config::data_dir()?.join("cache.sqlite"))?;
    let (tx, rx) = mpsc::unbounded_channel();

    app.manage(AppState {
        cfg_dir,
        config: Mutex::new(config.clone()),
        accounts: Mutex::new(accounts.clone()),
        running: RwLock::new(HashMap::new()),
        store: Mutex::new(store),
        tx,
        active: Mutex::new(HashMap::new()),
        avatars: Mutex::new(HashMap::new()),
        picked: Picked::default(),
    });

    tauri::async_runtime::spawn(event_loop(app.clone(), rx));

    for account in accounts {
        let app = app.clone();
        let config = config.clone();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AppState>();
            let result: Result<Arc<dyn ChatBackend>> = match account.kind {
                BackendKind::Google => match config.google.clone() {
                    Some(g) => GoogleBackend::restore(account.clone(), g, config.poll.clone())
                        .await
                        .map(|b| Arc::new(b) as Arc<dyn ChatBackend>),
                    None => Err(Error::other("Google-Client fehlt in chatty.toml")),
                },
                BackendKind::Mattermost => MattermostBackend::restore(account.clone())
                    .await
                    .map(|b| Arc::new(b) as Arc<dyn ChatBackend>),
            };
            let event = match result {
                Ok(b) => {
                    state.start(b).await;
                    BackendEvent::Status {
                        online: true,
                        detail: None,
                    }
                }
                Err(e) => BackendEvent::Status {
                    online: false,
                    detail: Some(e.to_string()),
                },
            };
            let _ = state.tx.send((account.id, event));
        });
    }
    Ok(())
}

#[derive(Clone, Serialize)]
struct ChatEvent<'a> {
    account: &'a str,
    event: &'a BackendEvent,
}

async fn event_loop(app: AppHandle, mut rx: mpsc::UnboundedReceiver<(String, BackendEvent)>) {
    while let Some((account, mut event)) = rx.recv().await {
        let state = app.state::<AppState>();
        let cached = {
            let mut store = state.store();
            if let BackendEvent::SpaceUpdated { space } = &mut event {
                let _ = store.apply_flags(&account, std::slice::from_mut(space));
            }
            match &event {
                BackendEvent::MessageCreated { message }
                | BackendEvent::MessageUpdated { message } => {
                    store.put_messages(&account, std::slice::from_ref(message))
                }
                BackendEvent::MessageDeleted { message_id, .. } => {
                    store.delete_message(&account, message_id)
                }
                BackendEvent::SpaceUpdated { space } => store.put_space(&account, space),
                BackendEvent::Status { .. } => Ok(()),
            }
        };
        if let Err(e) = cached {
            eprintln!("Cache: {e}");
        }
        if let BackendEvent::MessageCreated { message } = &event {
            notify(&app, &state, &account, message);
        }
        let _ = app.emit(
            "chat-event",
            ChatEvent {
                account: &account,
                event: &event,
            },
        );
    }
}

/// OS-Benachrichtigung bei DM oder Erwähnung, wenn der Space nicht gerade sichtbar ist.
fn notify(app: &AppHandle, state: &AppState, account: &str, m: &Message) {
    if m.mine || m.is_reply && !m.mentions_me {
        return;
    }
    if !state.config.lock().map(|c| c.notifications).unwrap_or(true) {
        return;
    }
    let focused = app
        .webview_windows()
        .values()
        .any(|w| w.is_focused().unwrap_or(false));
    let visible = state
        .active
        .lock()
        .map(|a| a.get(account) == Some(&m.space_id))
        .unwrap_or(false);
    if focused && visible {
        return;
    }
    let space = state.store().space(account, &m.space_id).ok().flatten();
    let is_dm = space.as_ref().is_some_and(|s| s.kind != SpaceKind::Channel);
    let muted = space.as_ref().is_some_and(|s| s.muted);
    if !(is_dm && !muted || m.mentions_me) {
        return;
    }
    let title = match &space {
        Some(s) if s.kind == SpaceKind::Channel => format!("{} in {}", m.sender.name, s.name),
        _ => m.sender.name.clone(),
    };
    let body: String = m.text.chars().take(200).collect();
    let _ = app.notification().builder().title(title).body(body).show();
}

// ---------- Kommandos ----------

type Cmd<T> = std::result::Result<T, Error>;

#[derive(Serialize)]
pub struct AppInfo {
    theme: String,
    notifications: bool,
    google_configured: bool,
    /// Client-ID zum Vorbelegen des Formulars; das Secret geht nicht ans Frontend.
    google_client_id: String,
    accounts: Vec<AccountInfo>,
}

#[derive(Serialize)]
pub struct AccountInfo {
    #[serde(flatten)]
    account: Account,
    online: bool,
}

#[tauri::command]
pub async fn app_info(state: State<'_, AppState>) -> Cmd<AppInfo> {
    let cfg = state
        .config
        .lock()
        .map_err(|_| Error::other("lock"))?
        .clone();
    let accounts = state
        .accounts
        .lock()
        .map_err(|_| Error::other("lock"))?
        .clone();
    let running = state.running.read().await;
    Ok(AppInfo {
        theme: cfg.theme,
        notifications: cfg.notifications,
        google_configured: cfg.google.as_ref().is_some_and(|g| !g.client_id.is_empty()),
        google_client_id: cfg.google.map(|g| g.client_id).unwrap_or_default(),
        accounts: accounts
            .into_iter()
            .map(|a| AccountInfo {
                online: running.contains_key(&a.id),
                account: a,
            })
            .collect(),
    })
}

#[tauri::command]
pub fn set_settings(state: State<'_, AppState>, theme: String, notifications: bool) -> Cmd<()> {
    if !["mattermost", "slack", "ehex"].contains(&theme.as_str()) {
        return Err(Error::other("Unbekanntes Theme"));
    }
    let mut cfg = state.config.lock().map_err(|_| Error::other("lock"))?;
    cfg.theme = theme;
    cfg.notifications = notifications;
    config::save_config(&state.cfg_dir, &cfg)
}

/// Google-OAuth-Client setzen und in `chatty.toml` speichern. Leeres Secret behält das bisherige,
/// leere Client-ID entfernt den Client.
#[tauri::command]
pub fn set_google_client(
    state: State<'_, AppState>,
    client_id: String,
    client_secret: String,
) -> Cmd<()> {
    let (client_id, client_secret) = (client_id.trim(), client_secret.trim());
    let mut cfg = state.config.lock().map_err(|_| Error::other("lock"))?;
    cfg.google = if client_id.is_empty() {
        None
    } else {
        let old = cfg
            .google
            .take()
            .map(|g| g.client_secret)
            .unwrap_or_default();
        Some(GoogleConfig {
            client_id: client_id.into(),
            client_secret: if client_secret.is_empty() {
                old
            } else {
                client_secret.into()
            },
        })
    };
    config::save_config(&state.cfg_dir, &cfg)
}

fn new_id() -> String {
    format!("{:016x}", rand::random::<u64>())
}

#[tauri::command]
pub async fn add_google_account(
    app: AppHandle,
    state: State<'_, AppState>,
    account_id: Option<String>,
) -> Cmd<Account> {
    let cfg = state
        .config
        .lock()
        .map_err(|_| Error::other("lock"))?
        .clone();
    let google = cfg
        .google
        .filter(|g| !g.client_id.is_empty())
        .ok_or_else(|| Error::other("Google-Client fehlt in chatty.toml (siehe usage.md)"))?;
    let id = account_id.unwrap_or_else(new_id);
    let backend = GoogleBackend::login(id, google, cfg.poll, move |url| {
        let _ = tauri_plugin_opener::open_url(url, None::<&str>);
    })
    .await?;
    let _ = &app;
    let account = backend.account().clone();
    state.upsert_account(account.clone())?;
    state.start(Arc::new(backend)).await;
    Ok(account)
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
    let id = account_id.unwrap_or_else(new_id);
    let mfa = mfa.filter(|m| !m.trim().is_empty());
    let backend = MattermostBackend::login(
        id,
        server.trim_end_matches('/').to_string(),
        login,
        password,
        mfa,
    )
    .await?;
    let account = backend.account().clone();
    state.upsert_account(account.clone())?;
    state.start(Arc::new(backend)).await;
    Ok(account)
}

#[tauri::command]
pub async fn remove_account(state: State<'_, AppState>, account: String) -> Cmd<()> {
    if let Some(r) = state.running.write().await.remove(&account) {
        r.task.abort();
    }
    secrets::delete(&account)?;
    state.store().delete_account(&account)?;
    let prefix = format!("{account}/");
    state
        .avatars
        .lock()
        .map_err(|_| Error::other("lock"))?
        .retain(|k, _| !k.starts_with(&prefix));
    let mut list = state.accounts.lock().map_err(|_| Error::other("lock"))?;
    list.retain(|a| a.id != account);
    config::save_accounts(&state.cfg_dir, &list)
}

#[tauri::command]
pub async fn browse_spaces(
    state: State<'_, AppState>,
    account: String,
    query: String,
) -> Cmd<Vec<Space>> {
    state.backend(&account).await?.browse_spaces(&query).await
}

#[tauri::command]
pub async fn join_space(state: State<'_, AppState>, account: String, space: String) -> Cmd<Space> {
    let mut space = state.backend(&account).await?.join_space(&space).await?;
    let store = state.store();
    store.apply_flags(&account, std::slice::from_mut(&mut space))?;
    store.put_space(&account, &space)?;
    Ok(space)
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
    let backend = state.backend(&account).await?;
    let fav_local = match favorite {
        Some(on) => !backend.set_favorite(&space, on).await?,
        None => false,
    };
    let mute_local = match muted {
        Some(on) => !backend.set_muted(&space, on).await?,
        None => false,
    };
    let store = state.store();
    if fav_local || mute_local {
        store.set_flag(
            &account,
            &space,
            favorite.filter(|_| fav_local),
            muted.filter(|_| mute_local),
        )?;
    }
    // Cache sofort angleichen (Benachrichtigungen lesen ihn).
    if let Some(mut s) = store.space(&account, &space)? {
        if let Some(f) = favorite {
            s.favorite = f;
        }
        if let Some(m) = muted {
            s.muted = m;
        }
        store.put_space(&account, &s)?;
    }
    Ok(())
}

/// Namen eigener Server-Emojis des Kontos (leer, wenn das Backend keine kennt).
#[tauri::command]
pub async fn custom_emojis(state: State<'_, AppState>, account: String) -> Cmd<Vec<String>> {
    state.backend(&account).await?.custom_emojis().await
}

#[tauri::command]
pub fn cached_spaces(state: State<'_, AppState>, account: String) -> Cmd<Vec<Space>> {
    state.store().spaces(&account)
}

#[tauri::command]
pub async fn list_spaces(state: State<'_, AppState>, account: String) -> Cmd<Vec<Space>> {
    let mut spaces = state.backend(&account).await?.list_spaces().await?;
    let mut store = state.store();
    store.apply_flags(&account, &mut spaces)?;
    store.put_spaces(&account, &spaces)?;
    Ok(spaces)
}

#[tauri::command]
pub fn cached_messages(
    state: State<'_, AppState>,
    account: String,
    space: String,
) -> Cmd<Vec<Message>> {
    state.store().messages(&account, &space, 50)
}

#[tauri::command]
pub async fn list_messages(
    state: State<'_, AppState>,
    account: String,
    space: String,
    cursor: Option<String>,
) -> Cmd<Page> {
    let page = state
        .backend(&account)
        .await?
        .list_messages(&space, cursor)
        .await?;
    state.store().put_messages(&account, &page.messages)?;
    Ok(page)
}

#[tauri::command]
pub async fn list_thread(
    state: State<'_, AppState>,
    account: String,
    space: String,
    thread: String,
) -> Cmd<Vec<Message>> {
    state
        .backend(&account)
        .await?
        .list_thread(&space, &thread)
        .await
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
    let mut uploads = Vec::new();
    for path in state.picked.paths(&files)? {
        let name = file_name(&path);
        let data = tokio::fs::read(&path).await?;
        uploads.push(UploadFile {
            mime: mime_for(&name).into(),
            name,
            data,
        });
    }
    let backend = state.backend(&account).await?;
    let msg = backend
        .send(&space, &text, thread.as_deref(), uploads)
        .await?;
    state.picked.remove(&files);
    state
        .store()
        .put_messages(&account, std::slice::from_ref(&msg))?;
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
        .set_file_name(&attachment.name)
        .save_file(move |path| {
            let _ = tx.send(path);
        });
    let Some(path) = rx.await.ok().flatten() else {
        return Ok(false);
    };
    let path = path.into_path().map_err(|e| Error::other(e.to_string()))?;
    let data = state.backend(&account).await?.download(&attachment).await?;
    tokio::fs::write(path, data).await?;
    Ok(true)
}

fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        "txt" | "log" => "text/plain",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
}

#[tauri::command]
pub async fn edit_message(
    state: State<'_, AppState>,
    account: String,
    space: String,
    id: String,
    text: String,
) -> Cmd<Message> {
    let msg = state
        .backend(&account)
        .await?
        .edit(&space, &id, &text)
        .await?;
    state
        .store()
        .put_messages(&account, std::slice::from_ref(&msg))?;
    Ok(msg)
}

#[tauri::command]
pub async fn delete_message(
    state: State<'_, AppState>,
    account: String,
    space: String,
    id: String,
) -> Cmd<()> {
    state.backend(&account).await?.delete(&space, &id).await?;
    state.store().delete_message(&account, &id)
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
    let b = state.backend(&account).await?;
    if add {
        b.react(&space, &id, &emoji).await
    } else {
        b.unreact(&space, &id, &emoji).await
    }
}

#[tauri::command]
pub async fn search(
    state: State<'_, AppState>,
    account: String,
    query: String,
) -> Cmd<Vec<Message>> {
    state.backend(&account).await?.search(&query).await
}

#[tauri::command]
pub async fn mark_read(state: State<'_, AppState>, account: String, space: String) -> Cmd<()> {
    state.backend(&account).await?.mark_read(&space).await
}

/// Meldet den offenen Space (Polling-Priorität, Benachrichtigungen).
#[tauri::command]
pub async fn set_active(
    state: State<'_, AppState>,
    account: String,
    space: Option<String>,
) -> Cmd<()> {
    if let Ok(mut a) = state.active.lock() {
        match &space {
            Some(s) => a.insert(account.clone(), s.clone()),
            None => a.remove(&account),
        };
    }
    if let Some(r) = state.running.read().await.get(&account) {
        let _ = r.active.send(space);
    }
    Ok(())
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
        let response = match avatar(&app, &path).await {
            Some(a) => tauri::http::Response::builder()
                .header("Content-Type", a.mime)
                .header("Cache-Control", "max-age=86400")
                .body(a.data),
            None => tauri::http::Response::builder()
                .status(404)
                .body(Vec::new()),
        };
        responder.respond(response.unwrap_or_default());
    });
}

/// Pfade: `<konto>/<nutzer>` (Profilbild) oder `<konto>/emoji/<name>` (eigenes Server-Emoji).
async fn avatar(app: &AppHandle, path: &str) -> Option<Avatar> {
    let (account, rest) = path.split_once('/')?;
    let valid = |s: &str, extra: &[char]| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || extra.contains(&c))
    };
    let emoji = rest.strip_prefix("emoji/");
    let ok = match emoji {
        Some(name) => valid(name, &['+']),
        None => valid(rest, &[]),
    };
    if !valid(account, &[]) || !ok {
        return None;
    }
    let state = app.state::<AppState>();
    let cell = state
        .avatars
        .lock()
        .ok()?
        .entry(path.to_string())
        .or_default()
        .clone();
    // Fehler (z. B. Konto noch nicht angemeldet) werden nicht gemerkt, der nächste Abruf versucht es neu.
    cell.get_or_try_init(|| async {
        let backend = state.backend(account).await?;
        match emoji {
            Some(name) => backend.custom_emoji(name).await,
            None => backend.avatar(rest).await,
        }
    })
    .await
    .ok()?
    .clone()
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
