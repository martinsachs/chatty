//! Kontoverwaltung und Ablauf ohne Oberfläche: Konten laden und anmelden, Ereignisse cachen,
//! Benachrichtigungen entscheiden, Backend-Aufrufe mit Cache. Die Oberflächen (Tauri, nativ) rufen nur
//! diese Fassade auf.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use tokio::sync::{RwLock, mpsc, watch};

use crate::backend::google::GoogleBackend;
use crate::backend::mattermost::MattermostBackend;
use crate::backend::{Avatar, ChatBackend, EventSender, MAX_PREVIEW};
use crate::config::{self, Config, GoogleConfig};
use crate::error::{Error, Result};
use crate::gif::{Gif, GifConfig};
use crate::model::{
    Account, Attachment, BackendEvent, BackendKind, Message, Page, Space, SpaceKind, UploadFile,
};
use crate::secrets;
use crate::store::Store;

/// Bekannte Optiken (Namen wie in `chatty.toml`).
pub const THEMES: [&str; 5] = ["mattermost", "slack", "ehex", "mirc", "dos"];

/// Ereignisse aller Konten: (Konto-ID, Ereignis).
pub type Events = mpsc::UnboundedReceiver<(String, BackendEvent)>;

struct Running {
    backend: Arc<dyn ChatBackend>,
    active: watch::Sender<Option<String>>,
    task: tokio::task::JoinHandle<()>,
}

type AvatarCell = Arc<tokio::sync::OnceCell<Option<Avatar>>>;

pub struct App {
    cfg_dir: PathBuf,
    /// Aus der Zwischenablage eingefügte Bilder bis zum Senden (ADR 06: Anhänge als Pfade).
    paste_dir: PathBuf,
    config: Mutex<Config>,
    accounts: Mutex<Vec<Account>>,
    running: RwLock<HashMap<String, Running>>,
    store: Mutex<Store>,
    tx: EventSender,
    /// Offener Space je Konto (für Benachrichtigungen).
    active: Mutex<HashMap<String, String>>,
    /// Profilbilder und Emojis je Pfad; die Zelle lädt jedes Bild nur einmal.
    avatars: Mutex<HashMap<String, AvatarCell>>,
    /// Hat irgendein Fenster den Fokus? (Google-Polling, Benachrichtigungen; ADR 07)
    focused: watch::Sender<bool>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppInfo {
    pub theme: String,
    pub notifications: bool,
    pub google_configured: bool,
    /// Client-ID zum Vorbelegen des Formulars; das Secret geht nicht an die Oberfläche.
    pub google_client_id: String,
    /// Host des GIF-Anbieters (Hinweis in der Suche); None = GIF-Suche aus.
    pub gif_provider: Option<String>,
    /// API-URL zum Vorbelegen des Formulars; der Schlüssel geht nicht an die Oberfläche.
    pub gif_api_url: String,
    pub accounts: Vec<AccountInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AccountInfo {
    #[serde(flatten)]
    pub account: Account,
    pub online: bool,
}

/// Vom Kern entschiedene OS-Benachrichtigung; anzeigen muss die Oberfläche.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub title: String,
    pub body: String,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn new_id() -> String {
    format!("{:016x}", rand::random::<u64>())
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "datei".into())
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

/// Obergrenze für ein eingefügtes Bild (Bildschirmfotos großer Monitore als PNG).
pub const MAX_PASTE: usize = 32 << 20;

/// Bildtyp an den ersten Bytes erkennen (nicht am Namen), nur png/jpeg/gif/webp.
fn image_ext(data: &[u8]) -> Option<&'static str> {
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if data.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpg")
    } else if data.starts_with(b"GIF8") {
        Some("gif")
    } else if data.len() > 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

/// Vorschau einer lokalen Datei für die Eingabe: nur Bildtypen, höchstens [`MAX_PREVIEW`].
pub fn local_preview(path: &Path) -> Option<Avatar> {
    let mime = mime_for(&file_name(path));
    if !mime.starts_with("image/") || std::fs::metadata(path).ok()?.len() > MAX_PREVIEW as u64 {
        return None;
    }
    let data = std::fs::read(path).ok()?;
    Some(Avatar {
        data,
        mime: mime.into(),
    })
}

/// Eingefügte Bilder älter als ein Tag entfernen (nie gesendet, App beendet). Jüngere können zu einem
/// zweiten laufenden chatty gehören.
fn clean_pasted(dir: &Path) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() > 86_400);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

impl App {
    /// Lädt Konfiguration, Konten und Cache aus den Standardverzeichnissen. Die Ereignisse aller Konten
    /// kommen über den zurückgegebenen Empfänger; jedes Ereignis zuerst an [`App::handle_event`] geben.
    pub fn open() -> Result<(Arc<App>, Events)> {
        let cfg_dir = config::config_dir()?;
        let data = config::data_dir()?;
        let store = Store::open(&data.join("cache.sqlite"))?;
        Self::with(cfg_dir, data.join("paste"), store)
    }

    fn with(cfg_dir: PathBuf, paste_dir: PathBuf, store: Store) -> Result<(Arc<App>, Events)> {
        clean_pasted(&paste_dir);
        let config = config::load_config(&cfg_dir)?;
        let accounts = config::load_accounts(&cfg_dir)?;
        let (tx, rx) = mpsc::unbounded_channel();
        let app = App {
            cfg_dir,
            paste_dir,
            config: Mutex::new(config),
            accounts: Mutex::new(accounts),
            running: RwLock::new(HashMap::new()),
            store: Mutex::new(store),
            tx,
            active: Mutex::new(HashMap::new()),
            avatars: Mutex::new(HashMap::new()),
            focused: watch::channel(true).0,
        };
        Ok((Arc::new(app), rx))
    }

    /// Meldet alle gespeicherten Konten parallel an (Token aus dem Schlüsselbund). Ergebnis je Konto als
    /// `Status`-Ereignis. Aufruf innerhalb einer Tokio-Laufzeit.
    pub fn restore_all(self: &Arc<Self>) {
        let config = self.config();
        for account in self.accounts() {
            let app = self.clone();
            let config = config.clone();
            tokio::spawn(async move {
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
                        app.start(b).await;
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
                let _ = app.tx.send((account.id, event));
            });
        }
    }

    /// Ereignis in den Cache übernehmen (Flags ergänzen) und entscheiden, ob eine OS-Benachrichtigung
    /// fällig ist. Danach gibt die Oberfläche das Ereignis weiter bzw. zeigt es an.
    pub fn handle_event(&self, account: &str, event: &mut BackendEvent) -> Option<Notification> {
        let cached = {
            let mut store = self.store();
            if let BackendEvent::SpaceUpdated { space } = event {
                let _ = store.apply_flags(account, std::slice::from_mut(space));
            }
            match &*event {
                BackendEvent::MessageCreated { message }
                | BackendEvent::MessageUpdated { message } => {
                    store.put_messages(account, std::slice::from_ref(message))
                }
                BackendEvent::MessageDeleted { message_id, .. } => {
                    store.delete_message(account, message_id)
                }
                BackendEvent::SpaceUpdated { space } => store.put_space(account, space),
                BackendEvent::Status { .. } => Ok(()),
            }
        };
        if let Err(e) = cached {
            eprintln!("Cache: {e}");
        }
        match &*event {
            BackendEvent::MessageCreated { message } => self.notification(account, message),
            _ => None,
        }
    }

    /// OS-Benachrichtigung bei DM oder Erwähnung, wenn der Space nicht gerade sichtbar ist.
    fn notification(&self, account: &str, m: &Message) -> Option<Notification> {
        if m.mine || m.is_reply && !m.mentions_me || !lock(&self.config).notifications {
            return None;
        }
        let focused = *self.focused.borrow();
        let visible = lock(&self.active).get(account) == Some(&m.space_id);
        if focused && visible {
            return None;
        }
        let space = self.store().space(account, &m.space_id).ok().flatten();
        let is_dm = space.as_ref().is_some_and(|s| s.kind != SpaceKind::Channel);
        let muted = space.as_ref().is_some_and(|s| s.muted);
        if !(is_dm && !muted || m.mentions_me) {
            return None;
        }
        let title = match &space {
            Some(s) if s.kind == SpaceKind::Channel => format!("{} in {}", m.sender.name, s.name),
            _ => m.sender.name.clone(),
        };
        Some(Notification {
            title: crate::model::notification_text(&title),
            body: crate::model::notification_text(&m.text.chars().take(200).collect::<String>()),
        })
    }

    fn store(&self) -> MutexGuard<'_, Store> {
        lock(&self.store)
    }

    pub fn config(&self) -> Config {
        lock(&self.config).clone()
    }

    pub fn accounts(&self) -> Vec<Account> {
        lock(&self.accounts).clone()
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
        let task =
            backend
                .clone()
                .start_events(self.tx.clone(), active_rx, self.focused.subscribe());
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
        let mut list = lock(&self.accounts);
        match list.iter_mut().find(|a| a.id == account.id) {
            Some(a) => *a = account,
            None => list.push(account),
        }
        config::save_accounts(&self.cfg_dir, &list)
    }

    // ---------- Einstellungen und Konten ----------

    pub async fn info(&self) -> AppInfo {
        let cfg = self.config();
        let running = self.running.read().await;
        AppInfo {
            theme: cfg.theme,
            notifications: cfg.notifications,
            google_configured: cfg.google.as_ref().is_some_and(|g| !g.client_id.is_empty()),
            google_client_id: cfg.google.map(|g| g.client_id).unwrap_or_default(),
            gif_provider: cfg
                .gif
                .as_ref()
                .and_then(|g| url::Url::parse(&g.api_url).ok())
                .and_then(|u| u.host_str().map(str::to_string)),
            gif_api_url: cfg.gif.map(|g| g.api_url).unwrap_or_default(),
            accounts: self
                .accounts()
                .into_iter()
                .map(|a| AccountInfo {
                    online: running.contains_key(&a.id),
                    account: a,
                })
                .collect(),
        }
    }

    pub fn set_settings(&self, theme: String, notifications: bool) -> Result<()> {
        if !THEMES.contains(&theme.as_str()) {
            return Err(Error::other("Unbekanntes Theme"));
        }
        let mut cfg = lock(&self.config);
        cfg.theme = theme;
        cfg.notifications = notifications;
        config::save_config(&self.cfg_dir, &cfg)
    }

    /// GIF-Anbieter setzen und in `chatty.toml` speichern. Leerer Schlüssel behält den bisherigen,
    /// leere URL schaltet die GIF-Suche aus.
    pub fn set_gif_config(&self, api_url: &str, api_key: &str) -> Result<()> {
        let (api_url, api_key) = (api_url.trim(), api_key.trim());
        let mut cfg = lock(&self.config);
        cfg.gif = if api_url.is_empty() {
            None
        } else {
            crate::gif::require_https(api_url)?;
            let old = cfg.gif.take().map(|g| g.api_key).unwrap_or_default();
            let api_key = if api_key.is_empty() {
                old
            } else {
                api_key.into()
            };
            if api_key.is_empty() {
                return Err(Error::other("API-Schlüssel des GIF-Anbieters fehlt"));
            }
            Some(GifConfig {
                api_url: api_url.into(),
                api_key,
            })
        };
        config::save_config(&self.cfg_dir, &cfg)
    }

    pub async fn search_gifs(&self, query: &str) -> Result<Vec<Gif>> {
        let gif = self
            .config()
            .gif
            .ok_or_else(|| Error::other("GIF-Suche ist nicht eingerichtet"))?;
        crate::gif::search(&gif, query).await
    }

    /// Google-OAuth-Client setzen und in `chatty.toml` speichern. Leeres Secret behält das bisherige,
    /// leere Client-ID entfernt den Client.
    pub fn set_google_client(&self, client_id: &str, client_secret: &str) -> Result<()> {
        let (client_id, client_secret) = (client_id.trim(), client_secret.trim());
        let mut cfg = lock(&self.config);
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
        config::save_config(&self.cfg_dir, &cfg)
    }

    /// Google-Anmeldung im System-Browser (`open_url` öffnet die Anmeldeseite).
    pub async fn add_google_account(
        &self,
        account_id: Option<String>,
        open_url: impl Fn(String) + Send,
    ) -> Result<Account> {
        let cfg = self.config();
        let google = cfg
            .google
            .filter(|g| !g.client_id.is_empty())
            .ok_or_else(|| Error::other("Google-Client fehlt in chatty.toml (siehe usage.md)"))?;
        let id = account_id.unwrap_or_else(new_id);
        let backend = GoogleBackend::login(id, google, cfg.poll, open_url).await?;
        let account = backend.account().clone();
        self.upsert_account(account.clone())?;
        self.start(Arc::new(backend)).await;
        Ok(account)
    }

    pub async fn add_mattermost_account(
        &self,
        account_id: Option<String>,
        server: &str,
        login: String,
        password: String,
        mfa: Option<String>,
    ) -> Result<Account> {
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
        self.upsert_account(account.clone())?;
        self.start(Arc::new(backend)).await;
        Ok(account)
    }

    pub async fn remove_account(&self, account: &str) -> Result<()> {
        if let Some(r) = self.running.write().await.remove(account) {
            r.task.abort();
            // Sitzung serverseitig beenden, damit ein abgegriffenes Token nicht weiter gilt.
            if let Err(e) = r.backend.logout().await {
                eprintln!("chatty: Abmelden beim Server fehlgeschlagen: {e}");
            }
        }
        secrets::delete(account)?;
        self.store().delete_account(account)?;
        let prefix = format!("{account}/");
        lock(&self.avatars).retain(|k, _| !k.starts_with(&prefix));
        let mut list = lock(&self.accounts);
        list.retain(|a| a.id != account);
        config::save_accounts(&self.cfg_dir, &list)
    }

    // ---------- Spaces ----------

    pub async fn browse_spaces(&self, account: &str, query: &str) -> Result<Vec<Space>> {
        self.backend(account).await?.browse_spaces(query).await
    }

    pub async fn join_space(&self, account: &str, space: &str) -> Result<Space> {
        let mut space = self.backend(account).await?.join_space(space).await?;
        let store = self.store();
        store.apply_flags(account, std::slice::from_mut(&mut space))?;
        store.put_space(account, &space)?;
        Ok(space)
    }

    /// Favorit bzw. Stumm setzen: beim Server, sonst lokal.
    pub async fn set_space_flag(
        &self,
        account: &str,
        space: &str,
        favorite: Option<bool>,
        muted: Option<bool>,
    ) -> Result<()> {
        let backend = self.backend(account).await?;
        let fav_local = match favorite {
            Some(on) => !backend.set_favorite(space, on).await?,
            None => false,
        };
        let mute_local = match muted {
            Some(on) => !backend.set_muted(space, on).await?,
            None => false,
        };
        let store = self.store();
        if fav_local || mute_local {
            store.set_flag(
                account,
                space,
                favorite.filter(|_| fav_local),
                muted.filter(|_| mute_local),
            )?;
        }
        // Cache sofort angleichen (Benachrichtigungen lesen ihn).
        if let Some(mut s) = store.space(account, space)? {
            if let Some(f) = favorite {
                s.favorite = f;
            }
            if let Some(m) = muted {
                s.muted = m;
            }
            store.put_space(account, &s)?;
        }
        Ok(())
    }

    /// Namen eigener Server-Emojis des Kontos (leer, wenn das Backend keine kennt).
    pub async fn custom_emojis(&self, account: &str) -> Result<Vec<String>> {
        self.backend(account).await?.custom_emojis().await
    }

    pub fn cached_spaces(&self, account: &str) -> Result<Vec<Space>> {
        self.store().spaces(account)
    }

    pub async fn list_spaces(&self, account: &str) -> Result<Vec<Space>> {
        let mut spaces = self.backend(account).await?.list_spaces().await?;
        let mut store = self.store();
        store.apply_flags(account, &mut spaces)?;
        store.put_spaces(account, &spaces)?;
        Ok(spaces)
    }

    // ---------- Nachrichten ----------

    /// Zuletzt gesehener Verlauf aus dem Cache; bei Mattermost mit Thread-Antworten (wie `list_messages`).
    pub fn cached_messages(&self, account: &str, space: &str) -> Result<Vec<Message>> {
        self.store()
            .messages(account, space, 50, self.inline_replies(account))
    }

    /// Thread-Antworten stehen im Hauptverlauf (Mattermost); bei Google nur im Thread-Bereich.
    pub fn inline_replies(&self, account: &str) -> bool {
        lock(&self.accounts)
            .iter()
            .any(|a| a.id == account && a.kind == BackendKind::Mattermost)
    }

    pub async fn list_messages(
        &self,
        account: &str,
        space: &str,
        cursor: Option<String>,
    ) -> Result<Page> {
        let page = self
            .backend(account)
            .await?
            .list_messages(space, cursor)
            .await?;
        self.store().put_messages(account, &page.messages)?;
        Ok(page)
    }

    pub async fn list_thread(
        &self,
        account: &str,
        space: &str,
        thread: &str,
    ) -> Result<Vec<Message>> {
        self.backend(account)
            .await?
            .list_thread(space, thread)
            .await
    }

    /// Senden mit Anhängen von der Platte.
    pub async fn send(
        &self,
        account: &str,
        space: &str,
        text: &str,
        thread: Option<&str>,
        files: &[PathBuf],
    ) -> Result<Message> {
        let mut uploads = Vec::new();
        for path in files {
            let name = file_name(path);
            let data = tokio::fs::read(path).await?;
            uploads.push(UploadFile {
                mime: mime_for(&name).into(),
                name,
                data,
            });
        }
        let msg = self
            .backend(account)
            .await?
            .send(space, text, thread, uploads)
            .await?;
        self.store()
            .put_messages(account, std::slice::from_ref(&msg))?;
        for path in files.iter().filter(|p| p.starts_with(&self.paste_dir)) {
            let _ = std::fs::remove_file(path);
        }
        Ok(msg)
    }

    /// Bild aus der Zwischenablage als Datei ablegen, damit es wie ein gewählter Anhang gesendet wird.
    /// Typ nach Inhalt, Name `bild-<Zeit>.<endung>`.
    pub fn paste_image(&self, data: &[u8]) -> Result<PathBuf> {
        if data.len() > MAX_PASTE {
            return Err(Error::other("Bild zu groß (max. 32 MiB)"));
        }
        let ext =
            image_ext(data).ok_or_else(|| Error::other("Kein Bild (png, jpeg, gif, webp)"))?;
        config::private_dir(&self.paste_dir)?;
        let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let mut path = self.paste_dir.join(format!("bild-{stamp}.{ext}"));
        for n in 2.. {
            if !path.exists() {
                break;
            }
            path = self.paste_dir.join(format!("bild-{stamp}-{n}.{ext}"));
        }
        config::write_private(&path, data)?;
        Ok(path)
    }

    /// Anhang laden und nach `path` schreiben.
    pub async fn save_attachment(
        &self,
        account: &str,
        attachment: &Attachment,
        path: &Path,
    ) -> Result<()> {
        let data = self.backend(account).await?.download(attachment).await?;
        tokio::fs::write(path, data).await?;
        Ok(())
    }

    pub async fn edit(&self, account: &str, space: &str, id: &str, text: &str) -> Result<Message> {
        let msg = self.backend(account).await?.edit(space, id, text).await?;
        self.store()
            .put_messages(account, std::slice::from_ref(&msg))?;
        Ok(msg)
    }

    pub async fn delete(&self, account: &str, space: &str, id: &str) -> Result<()> {
        self.backend(account).await?.delete(space, id).await?;
        self.store().delete_message(account, id)
    }

    pub async fn react(
        &self,
        account: &str,
        space: &str,
        id: &str,
        emoji: &str,
        add: bool,
    ) -> Result<()> {
        let b = self.backend(account).await?;
        if add {
            b.react(space, id, emoji).await
        } else {
            b.unreact(space, id, emoji).await
        }
    }

    pub async fn search(&self, account: &str, query: &str) -> Result<Vec<Message>> {
        self.backend(account).await?.search(query).await
    }

    pub async fn mark_read(&self, account: &str, space: &str) -> Result<()> {
        self.backend(account).await?.mark_read(space).await
    }

    // ---------- Fenster-Zustand ----------

    /// Meldet den offenen Space (Polling-Priorität, Benachrichtigungen).
    pub async fn set_active(&self, account: &str, space: Option<String>) {
        {
            let mut a = lock(&self.active);
            match &space {
                Some(s) => a.insert(account.to_string(), s.clone()),
                None => a.remove(account),
            };
        }
        if let Some(r) = self.running.read().await.get(account) {
            let _ = r.active.send(space);
        }
    }

    /// Hat irgendein Fenster den Fokus? Meldet nur Änderungen weiter.
    pub fn set_focused(&self, any: bool) {
        self.focused
            .send_if_modified(|f| std::mem::replace(f, any) != any);
    }

    /// Profilbild (`<konto>/<nutzer>`) oder eigenes Server-Emoji (`<konto>/emoji/<name>`), je Pfad einmal
    /// geladen. Ungültige Pfade und Fehler liefern `None`; Fehler werden nicht gemerkt.
    pub async fn avatar(&self, path: &str) -> Option<Avatar> {
        let (account, rest) = path.split_once('/')?;
        let valid = valid_part;
        let emoji = rest.strip_prefix("emoji/");
        let ok = match emoji {
            Some(name) => valid(name, &['+']),
            None => valid(rest, &[]),
        };
        if !valid(account, &[]) || !ok {
            return None;
        }
        let cell = lock(&self.avatars)
            .entry(path.to_string())
            .or_default()
            .clone();
        cell.get_or_try_init(|| async {
            let backend = self.backend(account).await?;
            match emoji {
                Some(name) => backend.custom_emoji(name).await,
                None => backend.avatar(rest).await,
            }
        })
        .await
        .ok()?
        .clone()
    }

    /// Wie [`App::avatar`], dazu `<konto>/file/<anhang-id>`: Bildvorschau eines Anhangs. Vorschauen werden
    /// nicht gemerkt (zu groß für einen Cache ohne Obergrenze); die ID prüft das Backend.
    pub async fn image(&self, path: &str) -> Option<Avatar> {
        if let Some((account, id)) = path.split_once("/file/") {
            if !valid_part(account, &[]) {
                return None;
            }
            return self.backend(account).await.ok()?.preview(id).await.ok()?;
        }
        self.avatar(path).await
    }
}

/// Pfadteil aus Buchstaben, Ziffern, `-`, `_` und `extra`.
fn valid_part(s: &str, extra: &[char]) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || extra.contains(&c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::User;

    fn app() -> (tempfile::TempDir, Arc<App>) {
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open(&dir.path().join("c.sqlite")).unwrap();
        let (app, _rx) =
            App::with(dir.path().to_path_buf(), dir.path().join("paste"), store).unwrap();
        (dir, app)
    }

    fn msg(space: &str, mentions_me: bool) -> Message {
        Message {
            id: "m1".into(),
            space_id: space.into(),
            sender: User {
                id: "u".into(),
                name: "anna".into(),
                avatar_url: None,
            },
            text: "hallo".into(),
            created: 1,
            edited: None,
            mine: false,
            mentions_me,
            is_reply: false,
            thread_id: None,
            reply_count: 0,
            reactions: vec![],
            attachments: vec![],
        }
    }

    #[test]
    fn pastes_images_only() {
        let (d, app) = app();
        let png = b"\x89PNG\r\n\x1a\nrest";
        let a = app.paste_image(png).unwrap();
        let b = app.paste_image(png).unwrap();
        assert_ne!(a, b, "gleiche Sekunde, eigener Name");
        assert!(a.starts_with(d.path().join("paste")));
        assert_eq!(a.extension().unwrap(), "png");
        assert_eq!(std::fs::read(&a).unwrap(), png);
        assert_eq!(local_preview(&a).unwrap().mime, "image/png");
        assert!(
            app.paste_image(b"<svg onload=x>").is_err(),
            "kein Rasterbild"
        );
        let txt = d.path().join("a.txt");
        std::fs::write(&txt, "x").unwrap();
        assert!(local_preview(&txt).is_none(), "nur Bilder");
    }

    #[tokio::test]
    async fn notifies_mention_unless_visible() {
        let (_d, app) = app();
        let mut ev = BackendEvent::MessageCreated {
            message: msg("s1", true),
        };
        assert!(app.handle_event("a", &mut ev).is_some(), "Erwähnung");
        app.set_active("a", Some("s1".into())).await;
        assert!(app.handle_event("a", &mut ev).is_none(), "sichtbar");
        app.set_focused(false);
        assert!(app.handle_event("a", &mut ev).is_some(), "ohne Fokus");
        let mut plain = BackendEvent::MessageCreated {
            message: msg("s2", false),
        };
        assert!(
            app.handle_event("a", &mut plain).is_none(),
            "Kanal unbekannt, keine Erwähnung"
        );
    }

    #[test]
    fn rejects_unknown_theme() {
        let (_d, app) = app();
        assert!(app.set_settings("dos".into(), true).is_ok());
        assert!(app.set_settings("xyz".into(), true).is_err());
        assert_eq!(app.config().theme, "dos");
    }
}
