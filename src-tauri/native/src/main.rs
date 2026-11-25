//! chatty-native: Oberfläche mit iced statt WebView (ADR 08), auf demselben Kern wie die Tauri-Variante.
//! Abweichungen zur Tauri-Oberfläche: `konzept/drift-native-tauri.md`.
//!
//! `chatty-native --open <space-id>` öffnet den Space nach dem Start (Messung, Screenshots).
//! Mit `CHATTY_SHOT=<datei.ppm>` bleibt das Fenster unsichtbar (kein Fokus, keine Eingaben), speichert nach dem
//! Laden des Verlaufs (nach `CHATTY_SHOT_DELAY` Sekunden, Standard 2) ein Bild der App und beendet sich
//! (Prüfhilfe, RAM-Messung ohne sichtbares Fenster).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod look;
mod prefs;
mod text;
mod updater;
mod view;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use chatty_core::app::Events;
use chatty_core::backend::is_preview_image;
use chatty_core::model::{
    Attachment, BackendEvent, BackendKind, Message as Msg, Page, Space, SpaceKind,
};
use chatty_core::{App, AppInfo};
use iced::widget::{Id, image, markdown, operation, scrollable, text_editor};
use iced::{Subscription, Task, event, keyboard, mouse, window};

use crate::look::Look;
use crate::prefs::{Prefs, SIDEBAR_DEFAULT, SIDEBAR_MAX, SIDEBAR_MIN, Sort, ZOOMS};

/// Ereignis-Empfänger des Kerns; die Subscription holt ihn einmal ab.
static EVENTS: OnceLock<Mutex<Option<Events>>> = OnceLock::new();

const MESSAGES: Id = Id::new("messages");
const COMPOSER: Id = Id::new("composer");
/// Abstand zum oberen Rand, ab dem ältere Nachrichten geladen werden.
const LOAD_OLDER_PX: f32 = 80.0;
/// Ab diesem Abstand zum Ende erscheint der Knopf „↓“ (wie Tauri).
const SHOW_DOWN_PX: f32 = 300.0;
/// Höchstens so viele Nachrichten bleiben geladen, wenn man wieder unten ist (ADR 05, nur Mattermost).
const WINDOW: usize = 300;
/// Schnellauswahl für Reaktionen (wie `QUICK_EMOJI` in `src/lib/format.ts`).
pub const QUICK_EMOJI: [&str; 12] = [
    "👍", "❤️", "😄", "🎉", "👀", "✅", "🙏", "🚀", "😂", "🤔", "👎", "🔥",
];
const INITIAL_SIZE: iced::Size = iced::Size::new(1100.0, 700.0);
const MIN_SIZE: iced::Size = iced::Size::new(720.0, 480.0);

/// Direktnachrichten ohne Nachricht seit so vielen Tagen ausblenden (wie Tauri).
pub const DM_MAX_AGE_MS: i64 = 90 * 86_400_000;

fn main() -> iced::Result {
    chatty_core::backend::init_tls();
    iced::application(boot, update, view::view)
        .title(|s: &State| match s.current_space() {
            Some(sp) => format!("{} – chatty", sp.name),
            None => "chatty".into(),
        })
        .subscription(subscription)
        .theme(|s: &State| s.look.theme())
        .scale_factor(|s: &State| s.prefs.zoom)
        // Unsichtbar und zentriert starten; `boot` setzt 2/3 des Bildschirms und zeigt das Fenster dann.
        .window(window::Settings {
            size: INITIAL_SIZE,
            min_size: Some(MIN_SIZE),
            position: window::Position::Centered,
            visible: false,
            // App-Icon wie die Tauri-Variante (src-tauri/icons, aus app-icon.svg erzeugt).
            icon: window::icon::from_file_data(include_bytes!("../../icons/128x128.png"), None)
                .ok(),
            ..window::Settings::default()
        })
        .run()
}

/// Rechte Leiste.
#[allow(clippy::large_enum_variant)]
pub enum Rhs {
    Thread {
        root: Msg,
        replies: Vec<Msg>,
    },
    Search {
        query: String,
        results: Option<Vec<Msg>>,
    },
}

/// Formular „Konten und Einstellungen“.
#[derive(Default)]
pub struct Form {
    /// Gesetzt: vorhandenes Mattermost-Konto neu anmelden.
    pub relogin: Option<String>,
    pub server: String,
    pub login: String,
    pub password: String,
    pub mfa: String,
    pub client_id: String,
    pub client_secret: String,
    pub busy: bool,
    pub message: Option<String>,
    /// Konto, dessen Entfernen gerade bestätigt werden soll.
    pub confirm_remove: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToastKind {
    Info,
    Error,
    Message,
}

pub struct Toast {
    pub id: u64,
    pub text: String,
    pub kind: ToastKind,
    pub action: Option<(String, Message)>,
}

pub struct State {
    pub app: Arc<App>,
    pub info: Option<AppInfo>,
    pub look: Look,
    pub prefs: Prefs,
    /// Seitenleiste wird gerade per Maus verbreitert.
    pub resizing: bool,
    pub account: Option<String>,
    pub spaces: Vec<Space>,
    /// Erwähnungen je Konto und Space (Zähler an den Konten in der Seitenleiste).
    pub mentions: HashMap<String, HashMap<String, u32>>,
    pub filter: String,
    filter_seq: u64,
    /// Beitretbare Kanäle passend zum Filter (nur Mattermost).
    pub joinable: Vec<Space>,
    pub space: Option<String>,
    pub messages: Vec<Msg>,
    pub cursor: Option<String>,
    pub loading: bool,
    /// Zählt Ladevorgänge; Antworten zu älteren Ständen werden verworfen.
    seq: u64,
    pub composer: text_editor::Content,
    pub thread_composer: text_editor::Content,
    /// Nachricht, die gerade im Eingabefeld bearbeitet wird.
    pub editing: Option<Msg>,
    /// Gewählte Anhänge für die nächste Nachricht.
    pub files: Vec<PathBuf>,
    /// Vorschaubilder gewählter Bilddateien.
    pub thumbs: HashMap<PathBuf, image::Handle>,
    /// Formatierte Vorschau des Eingabetexts (an, wenn `Some`).
    pub preview: Option<Vec<markdown::Item>>,
    /// Nachricht im Markier-Modus: Text in einem schreibgeschützten Editor zum Auswählen und Kopieren.
    pub selecting: Option<(String, text_editor::Content)>,
    pub sending: bool,
    pub show_down: bool,
    pub rhs: Option<Rhs>,
    pub search: String,
    /// Reaktions-Schnellauswahl offen an dieser Nachricht.
    pub picker: Option<String>,
    /// Nachricht, deren Löschen gerade bestätigt werden soll.
    pub confirm_delete: Option<String>,
    pub settings: Option<Form>,
    pub toasts: Vec<Toast>,
    toast_seq: u64,
    /// Geparster Text je Nachrichten-ID.
    pub md: HashMap<String, Vec<markdown::Item>>,
    /// Erste Zeile des Kanal-Headers (Quelle, geparst) für den Kopf des Verlaufs.
    topic: (String, Vec<markdown::Item>),
    /// Profilbilder und eigene Emojis je Pfad (`konto/nutzer`, `konto/emoji/name`); `None` = nicht vorhanden.
    pub images: HashMap<String, Option<image::Handle>>,
    pub custom_emojis: HashMap<String, HashSet<String>>,
    counted_replies: HashSet<String>,
    focused: bool,
    /// Aus `--open`: nach dem ersten Laden der Kanalliste öffnen.
    open_on_start: Option<String>,
    pub update_available: Option<updater::Release>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Info(AppInfo),
    Event(String, BackendEvent),
    Spaces(String, Result<Vec<Space>, String>),
    Emojis(String, Vec<String>),
    SelectAccount(String),
    OpenSpace(String),
    /// Hinweis „Öffnen“: Konto wechseln und Space öffnen.
    OpenIn(String, String),
    Page {
        seq: u64,
        older: bool,
        result: Result<Page, String>,
    },
    Scrolled(scrollable::Viewport),
    ToBottom,
    Compose(text_editor::Action),
    Send,
    Sent(Result<Msg, String>),
    EditLast,
    Edit(Msg),
    Edited(Result<Msg, String>),
    ConfirmDelete(Option<String>),
    Delete(Msg),
    Deleted(String, Result<(), String>),
    Escape,
    PickFiles,
    Picked(Vec<PathBuf>),
    /// Strg+V im Eingabefeld: Bild aus der Zwischenablage holen (Text fügt der Editor selbst ein).
    PasteImage,
    Pasted(Option<Vec<u8>>),
    Preview,
    /// Markier-Modus für eine Nachricht an/aus.
    Select(Option<Msg>),
    SelectAction(text_editor::Action),
    Copy(String),
    RemoveFile(usize),
    Save(Attachment),
    Saved(String, Result<bool, String>),
    Picker(Option<String>),
    React(Msg, String),
    Done(Result<(), String>),
    OpenThread(Msg),
    Thread(String, Result<Vec<Msg>, String>),
    ThreadCompose(text_editor::Action),
    ThreadSend,
    ThreadSent(Result<Msg, String>),
    CloseRhs,
    SearchInput(String),
    Search,
    Found(String, Result<Vec<Msg>, String>),
    Flag(String, Option<bool>, Option<bool>),
    /// Favorit/Stumm fehlgeschlagen: alter Zustand (favorite, muted) und Fehler.
    FlagFailed(String, bool, bool, String),
    Filter(String),
    Browse(u64),
    Joinable(u64, Vec<Space>),
    Join(String),
    Joined(Result<Space, String>),
    Sort(Sort),
    ResizeStart,
    ResizeMove(f32),
    ResizeEnd,
    ResizeReset,
    Image(String, Option<image::Handle>),
    Link(markdown::Uri),
    Settings(bool),
    FormInput(FormField, String),
    Theme(&'static str),
    /// Schriftgröße (Stufe aus `ZOOMS`).
    Zoom(f32),
    /// Strg + Plus/Minus: eine Stufe größer/kleiner.
    ZoomStep(i32),
    Notifications(bool),
    AddMattermost,
    AddGoogle(Option<String>),
    Relogin(String),
    SaveGoogleClient,
    ConfirmRemove(Option<String>),
    RemoveAccount(String),
    Added(Result<String, String>),
    CloseToast(u64),
    ToastAction(u64, Box<Message>),
    Focus(bool),
    /// Startgröße: Fenster, Bildschirmgröße, Position des zentrierten Startfensters.
    WindowSize(window::Id, Option<iced::Size>, Option<iced::Point>),
    UpdateFound(Option<updater::Release>),
    InstallUpdate,
    Updated(Result<(), String>),
    /// Prüfhilfe `CHATTY_SHOT`: Bild aufnehmen bzw. gespeichert.
    Shoot,
    Shot(window::Screenshot),
    Ignore,
}

#[derive(Debug, Clone, Copy)]
pub enum FormField {
    Server,
    Login,
    Password,
    Mfa,
    ClientId,
    ClientSecret,
}

fn boot() -> (State, Task<Message>) {
    let (app, rx) = match App::open() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("chatty: {e}");
            std::process::exit(1);
        }
    };
    EVENTS.get_or_init(|| Mutex::new(Some(rx)));
    let look = Look::from_name(&app.config().theme);
    let state = State {
        app: app.clone(),
        info: None,
        look,
        prefs: prefs::load(),
        resizing: false,
        account: None,
        spaces: Vec::new(),
        mentions: HashMap::new(),
        filter: String::new(),
        filter_seq: 0,
        joinable: Vec::new(),
        space: None,
        messages: Vec::new(),
        cursor: None,
        loading: false,
        seq: 0,
        composer: text_editor::Content::new(),
        thread_composer: text_editor::Content::new(),
        editing: None,
        files: Vec::new(),
        thumbs: HashMap::new(),
        preview: None,
        selecting: None,
        sending: false,
        show_down: false,
        rhs: None,
        search: String::new(),
        picker: None,
        confirm_delete: None,
        settings: None,
        toasts: Vec::new(),
        toast_seq: 0,
        md: HashMap::new(),
        topic: (String::new(), Vec::new()),
        images: HashMap::new(),
        custom_emojis: HashMap::new(),
        counted_replies: HashSet::new(),
        focused: true,
        open_on_start: std::env::args().skip_while(|a| a != "--open").nth(1),
        update_available: None,
    };
    let start = Task::perform(
        async move {
            app.restore_all();
            app.info().await
        },
        Message::Info,
    );
    let shot = std::env::var_os("CHATTY_SHOT").is_some();
    let check = if shot {
        Task::none()
    } else {
        Task::perform(updater::check(), Message::UpdateFound)
    };
    // Prüfhilfe CHATTY_SHOT: Fenster bleibt unsichtbar.
    let size = if shot {
        Task::none()
    } else {
        window::latest().and_then(|id| {
            window::monitor_size(id).then(move |monitor| {
                window::position(id).map(move |pos| Message::WindowSize(id, monitor, pos))
            })
        })
    };
    (state, Task::batch([start, check, size]))
}

impl State {
    /// Kanal-Header des offenen Kanals neu parsen, wenn sich seine erste Zeile geändert hat.
    fn sync_topic(&mut self) {
        let line = self
            .current_space()
            .and_then(|sp| sp.header.lines().map(str::trim).find(|l| !l.is_empty()))
            .unwrap_or_default();
        if line != self.topic.0 {
            let items = markdown::parse(&text::hard_breaks(line)).collect();
            self.topic = (line.to_string(), items);
        }
    }

    /// Formatierte Vorschau dem Eingabetext nachführen, wenn sie offen ist.
    fn update_preview(&mut self) {
        if let Some(items) = &mut self.preview {
            *items = markdown::parse(&text::hard_breaks(self.composer.text().trim())).collect();
        }
    }

    /// Geparster Kanal-Header für die Ansicht, leer ohne Header.
    pub fn topic(&self) -> &[markdown::Item] {
        &self.topic.1
    }

    pub fn current_space(&self) -> Option<&Space> {
        self.spaces
            .iter()
            .find(|s| Some(&s.id) == self.space.as_ref())
    }

    pub fn current_kind(&self) -> Option<BackendKind> {
        let info = self.info.as_ref()?;
        info.accounts
            .iter()
            .find(|a| Some(&a.account.id) == self.account.as_ref())
            .map(|a| a.account.kind)
    }

    pub fn online(&self) -> bool {
        self.info
            .as_ref()
            .and_then(|i| {
                i.accounts
                    .iter()
                    .find(|a| Some(&a.account.id) == self.account.as_ref())
            })
            .is_some_and(|a| a.online)
    }

    /// Summe der Erwähnungen eines Kontos.
    pub fn mention_total(&self, account: &str) -> u32 {
        self.mentions
            .get(account)
            .map(|m| m.values().sum())
            .unwrap_or(0)
    }

    /// Bild-Pfad für ":name:", wenn es ein eigenes Server-Emoji des Kontos ist.
    pub fn custom_emoji(&self, emoji: &str) -> Option<String> {
        let name = emoji.strip_prefix(':')?.strip_suffix(':')?;
        let account = self.account.as_ref()?;
        self.custom_emojis
            .get(account)?
            .contains(name)
            .then(|| format!("{account}/emoji/{name}"))
    }

    pub fn image(&self, path: &str) -> Option<&image::Handle> {
        self.images.get(path)?.as_ref()
    }

    fn set_mentions(&mut self, account: &str, spaces: &[Space]) {
        self.mentions.insert(
            account.to_string(),
            spaces
                .iter()
                .map(|s| (s.id.clone(), s.mention_count))
                .collect(),
        );
    }

    /// In-App-Hinweis; verschwindet nach 5 s (Fehler 8 s).
    fn toast(
        &mut self,
        text: impl Into<String>,
        kind: ToastKind,
        action: Option<(String, Message)>,
    ) -> Task<Message> {
        self.toast_seq += 1;
        let id = self.toast_seq;
        self.toasts.push(Toast {
            id,
            text: text.into(),
            kind,
            action,
        });
        if self.toasts.len() > 5 {
            self.toasts.remove(0);
        }
        let secs = if kind == ToastKind::Error { 8 } else { 5 };
        Task::perform(tokio::time::sleep(Duration::from_secs(secs)), move |_| {
            Message::CloseToast(id)
        })
    }

    fn error(&mut self, e: impl Into<String>) -> Task<Message> {
        self.toast(e, ToastKind::Error, None)
    }

    fn load_spaces(&self, account: String) -> Task<Message> {
        let app = self.app.clone();
        let emojis = {
            let app = app.clone();
            let account = account.clone();
            Task::perform(
                async move {
                    let e = app.custom_emojis(&account).await.unwrap_or_default();
                    (account, e)
                },
                |(a, e)| Message::Emojis(a, e),
            )
        };
        let spaces = Task::perform(
            async move {
                let r = app.list_spaces(&account).await.map_err(|e| e.to_string());
                (account, r)
            },
            |(a, r)| Message::Spaces(a, r),
        );
        Task::batch([spaces, emojis])
    }

    fn load_page(&mut self, older: bool) -> Task<Message> {
        let (Some(account), Some(space)) = (self.account.clone(), self.space.clone()) else {
            return Task::none();
        };
        let cursor = if older { self.cursor.clone() } else { None };
        if older && cursor.is_none() {
            return Task::none();
        }
        self.loading = true;
        let seq = self.seq;
        let app = self.app.clone();
        Task::perform(
            async move {
                app.list_messages(&account, &space, cursor)
                    .await
                    .map_err(|e| e.to_string())
            },
            move |result| Message::Page { seq, older, result },
        )
    }

    /// Text parsen und fehlende Profilbilder/Emojis anstoßen.
    fn prepare(&mut self, msgs: &[Msg]) -> Task<Message> {
        let mut tasks = Vec::new();
        for m in msgs {
            self.md.insert(
                m.id.clone(),
                markdown::parse(&text::hard_breaks(&m.text)).collect(),
            );
            if self.look.avatars
                && let Some(acc) = self.account.clone()
            {
                tasks.push(self.fetch_image(format!("{acc}/{}", m.sender.id)));
            }
            for r in &m.reactions {
                if let Some(path) = self.custom_emoji(&r.emoji) {
                    tasks.push(self.fetch_image(path));
                }
            }
            if let Some(acc) = self.account.clone() {
                for a in m.attachments.iter().filter(|a| is_preview_image(a)) {
                    tasks.push(self.fetch_image(preview_path(&acc, a)));
                }
            }
        }
        Task::batch(tasks)
    }

    fn fetch_image(&mut self, path: String) -> Task<Message> {
        if self.images.contains_key(&path) {
            return Task::none();
        }
        self.images.insert(path.clone(), None);
        let app = self.app.clone();
        Task::perform(
            async move {
                // Vorschauen größer zulassen als Profilbilder und Emojis.
                let max = if path.contains("/file/") { 4096 } else { 2048 };
                let img = app
                    .image(&path)
                    .await
                    .filter(|a| small_image(&a.data, max))
                    .map(|a| image::Handle::from_bytes(a.data));
                (path, img)
            },
            |(p, i)| Message::Image(p, i),
        )
    }

    /// `created`: neu eingegangen (Antwortzähler nur dann erhöhen, wie Tauri).
    fn upsert(&mut self, m: Msg, created: bool) -> Task<Message> {
        if Some(&m.space_id) != self.space.as_ref() {
            return Task::none();
        }
        let task = self.prepare(std::slice::from_ref(&m));
        if let Some(Rhs::Thread { root, replies }) = &mut self.rhs
            && m.thread_id.is_some()
            && root.thread_id == m.thread_id
        {
            if root.id == m.id {
                *root = m.clone();
            } else if m.is_reply {
                match replies.iter_mut().find(|x| x.id == m.id) {
                    Some(x) => *x = m.clone(),
                    None => replies.push(m.clone()),
                }
            }
        }
        if m.is_reply {
            if created
                && self.counted_replies.insert(m.id.clone())
                && let Some(root) = self
                    .messages
                    .iter_mut()
                    .find(|x| !x.is_reply && x.thread_id.is_some() && x.thread_id == m.thread_id)
            {
                root.reply_count += 1;
            }
            // Mattermost: Antworten stehen auch im Hauptverlauf (als Antwort markiert), Google nur im Thread.
            let inline = self
                .account
                .as_deref()
                .is_some_and(|a| self.app.inline_replies(a));
            if !inline {
                return task;
            }
        }
        match self.messages.iter_mut().find(|x| x.id == m.id) {
            Some(x) => *x = m,
            None => {
                self.messages.push(m);
                self.messages.sort_by_key(|m| m.created);
            }
        }
        task
    }

    /// ADR 05: wieder unten angekommen, ältere über `WINDOW` verwerfen (nur Mattermost: Cursor = Post-ID).
    fn trim_to_window(&mut self) {
        if self.messages.len() <= WINDOW
            || self.loading
            || self.current_kind() != Some(BackendKind::Mattermost)
        {
            return;
        }
        let drop = self.messages.len() - WINDOW;
        self.messages.drain(..drop);
        self.cursor = self.messages.first().map(|m| m.id.clone());
    }

    /// Neue Nachricht in einem nicht sichtbaren Space: ungelesen, Zähler und Hinweis (wie Tauri `markUnread`).
    fn mark_unread(&mut self, account: &str, m: &Msg) -> Task<Message> {
        let current = self.account.as_deref() == Some(account);
        let known = if current {
            self.spaces
                .iter_mut()
                .find(|s| s.id == m.space_id)
                .map(|s| {
                    s.unread = true;
                    s.last_active = s.last_active.max(m.created);
                    (s.name.clone(), s.kind, s.muted)
                })
        } else {
            self.app
                .cached_spaces(account)
                .ok()
                .and_then(|l| l.into_iter().find(|s| s.id == m.space_id))
                .map(|s| (s.name, s.kind, s.muted))
        };
        let Some((name, kind, muted)) = known else {
            return Task::none();
        };
        if !(m.mentions_me || kind != SpaceKind::Channel && !muted) {
            return Task::none();
        }
        if current && let Some(s) = self.spaces.iter_mut().find(|s| s.id == m.space_id) {
            s.mention_count += 1;
        }
        *self
            .mentions
            .entry(account.to_string())
            .or_default()
            .entry(m.space_id.clone())
            .or_default() += 1;
        let where_ = if kind == SpaceKind::Channel {
            format!(" in {name}")
        } else {
            String::new()
        };
        let text: String = m.text.chars().take(120).collect();
        self.toast(
            format!("{}{where_}: {text}", m.sender.name),
            ToastKind::Message,
            Some((
                "Öffnen".into(),
                Message::OpenIn(account.to_string(), m.space_id.clone()),
            )),
        )
    }

    fn mark_read_current(&mut self) -> Task<Message> {
        let (Some(account), Some(space)) = (self.account.clone(), self.space.clone()) else {
            return Task::none();
        };
        let Some(s) = self.spaces.iter_mut().find(|s| s.id == space) else {
            return Task::none();
        };
        if !s.unread && s.mention_count == 0 {
            return Task::none();
        }
        s.unread = false;
        s.mention_count = 0;
        if let Some(m) = self.mentions.get_mut(&account) {
            m.insert(space.clone(), 0);
        }
        self.call(
            move |app| async move { app.mark_read(&account, &space).await },
            |_| Message::Ignore,
        )
    }

    fn account_space(&self) -> Option<(String, String)> {
        Some((self.account.clone()?, self.space.clone()?))
    }

    /// Ruft eine Kernfunktion im Hintergrund auf; Fehler als Text.
    fn call<T: Send + 'static, F>(
        &self,
        f: impl FnOnce(Arc<App>) -> F + Send + 'static,
        done: impl Fn(Result<T, String>) -> Message + Send + 'static,
    ) -> Task<Message>
    where
        F: Future<Output = chatty_core::error::Result<T>> + Send + 'static,
    {
        let app = self.app.clone();
        Task::perform(async move { f(app).await.map_err(|e| e.to_string()) }, done)
    }
}

/// Pfad der Bildvorschau eines Anhangs in `State::images` (siehe `App::image`).
pub fn preview_path(account: &str, a: &Attachment) -> String {
    format!("{account}/file/{}", a.id)
}

/// Bilder nur bis `max`×`max` dekodieren: eine kleine Datei mit riesigen Abmessungen
/// (Dekompressionsbombe) belegte sonst Hunderte MiB.
/// Bild aus der Zwischenablage als PNG; `None` ohne Bild oder wenn sie auch Text enthält (der gewinnt,
/// wie in Tauri). Blockiert, daher in `spawn_blocking`.
fn clipboard_png() -> Option<Vec<u8>> {
    let mut cb = arboard::Clipboard::new().ok()?;
    if cb.get_text().is_ok_and(|t| !t.trim().is_empty()) {
        return None;
    }
    let i = cb.get_image().ok()?;
    let rgba = img::RgbaImage::from_raw(i.width as u32, i.height as u32, i.bytes.into_owned())?;
    let mut out = std::io::Cursor::new(Vec::new());
    rgba.write_to(&mut out, img::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

fn small_image(data: &[u8], max: u32) -> bool {
    img::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_dimensions().ok())
        .is_some_and(|(w, h)| w <= max && h <= max)
}

fn update(s: &mut State, message: Message) -> Task<Message> {
    let task = handle(s, message);
    s.sync_topic();
    task
}

fn handle(s: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::Info(info) => {
            for a in &info.accounts {
                if !s.mentions.contains_key(&a.account.id) {
                    let cached = s.app.cached_spaces(&a.account.id).unwrap_or_default();
                    s.set_mentions(&a.account.id, &cached);
                }
            }
            let first = info.accounts.first().map(|a| a.account.id.clone());
            let known = s
                .account
                .as_ref()
                .is_some_and(|a| info.accounts.iter().any(|x| &x.account.id == a));
            let empty = info.accounts.is_empty();
            s.info = Some(info);
            // Wie Tauri: ohne Konto gleich die Einstellungen öffnen.
            if empty && s.settings.is_none() && std::env::var_os("CHATTY_SHOT").is_none() {
                return update(s, Message::Settings(true));
            }
            match (known, first) {
                (false, Some(id)) => update(s, Message::SelectAccount(id)),
                (false, None) => {
                    s.account = None;
                    s.spaces.clear();
                    s.space = None;
                    s.messages.clear();
                    Task::none()
                }
                _ => Task::none(),
            }
        }
        Message::SelectAccount(id) => {
            if s.account.as_ref() == Some(&id) {
                return Task::none();
            }
            if let Some((a, _)) = s.account_space() {
                let app = s.app.clone();
                tokio::spawn(async move { app.set_active(&a, None).await });
            }
            s.account = Some(id.clone());
            s.space = None;
            s.messages.clear();
            s.rhs = None;
            s.filter.clear();
            s.joinable.clear();
            s.spaces = s.app.cached_spaces(&id).unwrap_or_default();
            s.load_spaces(id)
        }
        Message::Spaces(account, result) => {
            // Fehler: Konto meldet sich evtl. erst noch an, beim Status-Ereignis wird neu geladen.
            if let Ok(spaces) = result {
                s.set_mentions(&account, &spaces);
                if s.account.as_ref() == Some(&account) {
                    s.spaces = spaces;
                    if let Some(id) = s.open_on_start.take() {
                        return update(s, Message::OpenSpace(id));
                    }
                }
            }
            Task::none()
        }
        Message::Emojis(account, names) => {
            s.custom_emojis.insert(account, names.into_iter().collect());
            Task::none()
        }
        Message::OpenIn(account, space) => {
            let select = update(s, Message::SelectAccount(account));
            Task::batch([select, update(s, Message::OpenSpace(space))])
        }
        Message::OpenSpace(id) => {
            let Some(account) = s.account.clone() else {
                return Task::none();
            };
            s.seq += 1;
            s.space = Some(id.clone());
            s.messages = s.app.cached_messages(&account, &id).unwrap_or_default();
            let cached = s.messages.clone();
            let prep = s.prepare(&cached);
            s.cursor = None;
            s.editing = None;
            s.picker = None;
            s.confirm_delete = None;
            s.show_down = false;
            if matches!(s.rhs, Some(Rhs::Thread { .. })) {
                s.rhs = None;
            }
            if let Some(sp) = s.spaces.iter_mut().find(|x| x.id == id) {
                sp.unread = false;
                sp.mention_count = 0;
            }
            if let Some(m) = s.mentions.get_mut(&account) {
                m.insert(id.clone(), 0);
            }
            let app = s.app.clone();
            let side = Task::perform(
                async move {
                    app.set_active(&account, Some(id.clone())).await;
                    let _ = app.mark_read(&account, &id).await;
                },
                |_| Message::Ignore,
            );
            Task::batch([s.load_page(false), side, prep, operation::focus(COMPOSER)])
        }
        Message::Page { seq, older, result } => {
            if seq != s.seq {
                return Task::none();
            }
            s.loading = false;
            match result {
                Ok(page) => {
                    let prep = s.prepare(&page.messages);
                    if older {
                        let known: HashSet<_> = s.messages.iter().map(|m| m.id.clone()).collect();
                        let mut msgs: Vec<_> = page
                            .messages
                            .into_iter()
                            .filter(|m| !known.contains(&m.id))
                            .collect();
                        msgs.append(&mut s.messages);
                        s.messages = msgs;
                        // Antworten zu Startnachrichten der älteren Seite liegen in den neueren Seiten.
                        chatty_core::model::count_replies(&mut s.messages);
                    } else {
                        s.messages = page.messages;
                    }
                    s.cursor = page.next_cursor;
                    let scroll = if older {
                        Task::none()
                    } else if std::env::var_os("CHATTY_SHOT").is_some() {
                        Task::batch([
                            to_bottom(),
                            Task::perform(
                                tokio::time::sleep(Duration::from_secs(shot_delay())),
                                |_| Message::Shoot,
                            ),
                        ])
                    } else {
                        to_bottom()
                    };
                    Task::batch([prep, scroll])
                }
                Err(e) => s.error(format!("Verlauf laden: {e}")),
            }
        }
        Message::Scrolled(vp) => {
            // anchor_bottom: absolute_offset zählt vom unteren Rand, reversed vom oberen.
            let from_bottom = vp.absolute_offset().y;
            s.show_down = from_bottom > SHOW_DOWN_PX;
            if from_bottom < 40.0 {
                s.trim_to_window();
            }
            if vp.absolute_offset_reversed().y < LOAD_OLDER_PX && !s.loading {
                return s.load_page(true);
            }
            Task::none()
        }
        Message::ToBottom => {
            s.show_down = false;
            to_bottom()
        }
        Message::Compose(action) => {
            s.composer.perform(action);
            s.update_preview();
            Task::none()
        }
        Message::Preview => {
            s.preview = if s.preview.is_some() {
                None
            } else {
                Some(Vec::new())
            };
            s.update_preview();
            Task::none()
        }
        Message::Send => {
            let Some((account, space)) = s.account_space() else {
                return Task::none();
            };
            if s.sending {
                return Task::none();
            }
            let text = s.composer.text().trim().to_string();
            if let Some(m) = s.editing.clone() {
                if text.is_empty() || text == m.text {
                    s.editing = None;
                    s.composer = text_editor::Content::new();
                    return Task::none();
                }
                s.sending = true;
                return s.call(
                    move |app| async move { app.edit(&account, &m.space_id, &m.id, &text).await },
                    Message::Edited,
                );
            }
            if text.is_empty() && s.files.is_empty() {
                return Task::none();
            }
            s.sending = true;
            let files = s.files.clone();
            s.call(
                move |app| async move { app.send(&account, &space, &text, None, &files).await },
                Message::Sent,
            )
        }
        Message::Sent(result) => {
            s.sending = false;
            match result {
                // Erst nach Erfolg leeren, damit ein Fehlschlag nichts verliert (wie Tauri).
                Ok(m) => {
                    s.composer = text_editor::Content::new();
                    s.files.clear();
                    s.thumbs.clear();
                    s.update_preview();
                    Task::batch([s.upsert(m, true), to_bottom()])
                }
                Err(e) => s.error(e),
            }
        }
        Message::EditLast => match s.messages.iter().rev().find(|m| m.mine).cloned() {
            Some(m) => update(s, Message::Edit(m)),
            None => Task::none(),
        },
        Message::Edit(m) => {
            s.composer = text_editor::Content::with_text(&m.text);
            s.composer
                .perform(text_editor::Action::Move(text_editor::Motion::DocumentEnd));
            s.editing = Some(m);
            operation::focus(COMPOSER)
        }
        Message::Edited(result) => {
            s.sending = false;
            match result {
                Ok(m) => {
                    s.editing = None;
                    s.composer = text_editor::Content::new();
                    s.upsert(m, false)
                }
                Err(e) => s.error(e),
            }
        }
        Message::Escape => {
            if s.selecting.take().is_some() {
            } else if s.editing.take().is_some() {
                s.composer = text_editor::Content::new();
            } else if s.picker.is_some() || s.confirm_delete.is_some() {
                s.picker = None;
                s.confirm_delete = None;
            } else if s.settings.is_some() {
                s.settings = None;
            } else {
                s.rhs = None;
            }
            Task::none()
        }
        Message::ConfirmDelete(id) => {
            s.confirm_delete = id;
            Task::none()
        }
        Message::Delete(m) => {
            s.confirm_delete = None;
            let Some(account) = s.account.clone() else {
                return Task::none();
            };
            let id = m.id.clone();
            s.call(
                move |app| async move { app.delete(&account, &m.space_id, &m.id).await },
                move |r| Message::Deleted(id.clone(), r),
            )
        }
        Message::Deleted(id, result) => match result {
            Ok(()) => {
                s.messages.retain(|m| m.id != id);
                if let Some(Rhs::Thread { replies, .. }) = &mut s.rhs {
                    replies.retain(|m| m.id != id);
                }
                Task::none()
            }
            Err(e) => s.error(e),
        },
        Message::PickFiles => Task::perform(
            async {
                rfd::AsyncFileDialog::new()
                    .pick_files()
                    .await
                    .unwrap_or_default()
                    .into_iter()
                    .map(|f| f.path().to_path_buf())
                    .collect()
            },
            Message::Picked,
        ),
        Message::Picked(paths) => {
            for p in &paths {
                if let Some(a) = chatty_core::app::local_preview(p)
                    && small_image(&a.data, 4096)
                {
                    s.thumbs
                        .insert(p.clone(), image::Handle::from_bytes(a.data));
                }
            }
            s.files.extend(paths);
            operation::focus(COMPOSER)
        }
        Message::RemoveFile(i) => {
            if i < s.files.len() {
                let p = s.files.remove(i);
                s.thumbs.remove(&p);
            }
            Task::none()
        }
        Message::PasteImage => Task::perform(
            async {
                tokio::task::spawn_blocking(clipboard_png)
                    .await
                    .ok()
                    .flatten()
            },
            Message::Pasted,
        ),
        Message::Pasted(None) => Task::none(),
        Message::Pasted(Some(png)) => match s.app.paste_image(&png) {
            Ok(path) => update(s, Message::Picked(vec![path])),
            Err(e) => s.error(e.to_string()),
        },
        Message::Select(m) => {
            s.selecting = m.map(|m| {
                let mut c = text_editor::Content::with_text(&m.text);
                c.perform(text_editor::Action::SelectAll);
                (m.id, c)
            });
            Task::none()
        }
        Message::SelectAction(action) => {
            if let Some((_, c)) = &mut s.selecting
                && !action.is_edit()
            {
                c.perform(action);
            }
            Task::none()
        }
        Message::Copy(text) => Task::batch([
            iced::clipboard::write(text),
            update(s, Message::Select(None)),
            s.toast("Text kopiert", ToastKind::Info, None),
        ]),
        Message::Save(att) => {
            let Some(account) = s.account.clone() else {
                return Task::none();
            };
            let name = att.name.clone();
            s.call(
                move |app| async move {
                    let Some(file) = rfd::AsyncFileDialog::new()
                        .set_file_name(chatty_core::model::safe_file_name(&att.name))
                        .save_file()
                        .await
                    else {
                        return Ok(false);
                    };
                    app.save_attachment(&account, &att, file.path()).await?;
                    Ok(true)
                },
                move |r| Message::Saved(name.clone(), r),
            )
        }
        Message::Saved(name, result) => match result {
            Ok(true) => s.toast(format!("{name} gespeichert"), ToastKind::Info, None),
            Ok(false) => Task::none(),
            Err(e) => s.error(e),
        },
        Message::Picker(id) => {
            s.picker = id;
            Task::none()
        }
        Message::React(m, emoji) => {
            let Some(account) = s.account.clone() else {
                return Task::none();
            };
            s.picker = None;
            let add = !m.reactions.iter().any(|r| r.emoji == emoji && r.me);
            // sofort anzeigen; das Server-Ereignis korrigiert
            let apply = |x: &mut Msg| {
                match x.reactions.iter_mut().find(|r| r.emoji == emoji) {
                    Some(r) if add => {
                        r.count += 1;
                        r.me = true;
                    }
                    Some(r) => {
                        r.count = r.count.saturating_sub(1);
                        r.me = false;
                    }
                    None if add => x.reactions.push(chatty_core::model::Reaction {
                        emoji: emoji.clone(),
                        count: 1,
                        me: true,
                    }),
                    None => {}
                }
                x.reactions.retain(|r| r.count > 0);
            };
            if let Some(x) = s.messages.iter_mut().find(|x| x.id == m.id) {
                apply(x);
            }
            if let Some(Rhs::Thread { root, replies }) = &mut s.rhs {
                if root.id == m.id {
                    apply(root);
                }
                if let Some(x) = replies.iter_mut().find(|x| x.id == m.id) {
                    apply(x);
                }
            }
            let e = emoji.clone();
            s.call(
                move |app| async move { app.react(&account, &m.space_id, &m.id, &e, add).await },
                Message::Done,
            )
        }
        Message::Done(result) => match result {
            Ok(()) => Task::none(),
            Err(e) => s.error(e),
        },
        // Aus einer Antwort im Hauptverlauf heraus mit deren Startnachricht (wie Tauri `openThread`).
        Message::OpenThread(from) => {
            let (Some(account), Some(thread)) = (s.account.clone(), from.thread_id.clone()) else {
                return Task::none();
            };
            let root = if from.is_reply {
                s.messages
                    .iter()
                    .find(|m| !m.is_reply && m.id == thread)
                    .cloned()
                    .unwrap_or(from)
            } else {
                from
            };
            let space = root.space_id.clone();
            s.rhs = Some(Rhs::Thread {
                root,
                replies: Vec::new(),
            });
            s.thread_composer = text_editor::Content::new();
            let t = thread.clone();
            s.call(
                move |app| async move { app.list_thread(&account, &space, &t).await },
                move |r| Message::Thread(thread.clone(), r),
            )
        }
        Message::Thread(thread, result) => match result {
            Ok(all) => {
                let task = s.prepare(&all);
                if let Some(Rhs::Thread { root, replies }) = &mut s.rhs
                    && root.thread_id.as_deref() == Some(thread.as_str())
                {
                    if let Some(start) = all.iter().find(|m| !m.is_reply) {
                        *root = start.clone();
                    }
                    let root_id = root.id.clone();
                    *replies = all.into_iter().filter(|m| m.id != root_id).collect();
                }
                task
            }
            Err(e) => s.error(e),
        },
        Message::ThreadCompose(action) => {
            s.thread_composer.perform(action);
            Task::none()
        }
        Message::ThreadSend => {
            let Some(account) = s.account.clone() else {
                return Task::none();
            };
            let Some(Rhs::Thread { root, .. }) = &s.rhs else {
                return Task::none();
            };
            let text = s.thread_composer.text().trim().to_string();
            let (space, thread) = (root.space_id.clone(), root.thread_id.clone());
            if text.is_empty() {
                return Task::none();
            }
            s.call(
                move |app| async move {
                    app.send(&account, &space, &text, thread.as_deref(), &[])
                        .await
                },
                Message::ThreadSent,
            )
        }
        Message::ThreadSent(result) => match result {
            Ok(m) => {
                s.thread_composer = text_editor::Content::new();
                s.upsert(m, true)
            }
            Err(e) => s.error(e),
        },
        Message::CloseRhs => {
            s.rhs = None;
            Task::none()
        }
        Message::SearchInput(q) => {
            s.search = q;
            Task::none()
        }
        Message::Search => {
            let Some(account) = s.account.clone() else {
                return Task::none();
            };
            let query = s.search.trim().to_string();
            if query.is_empty() {
                return Task::none();
            }
            s.rhs = Some(Rhs::Search {
                query: query.clone(),
                results: None,
            });
            let q = query.clone();
            s.call(
                move |app| async move { app.search(&account, &q).await },
                move |r| Message::Found(query.clone(), r),
            )
        }
        Message::Found(query, result) => match result {
            Ok(found) => {
                let task = s.prepare(&found);
                if let Some(Rhs::Search { query: q, results }) = &mut s.rhs
                    && *q == query
                {
                    *results = Some(found);
                }
                task
            }
            Err(e) => s.error(e),
        },
        Message::Flag(id, favorite, muted) => {
            let Some(account) = s.account.clone() else {
                return Task::none();
            };
            let Some(sp) = s.spaces.iter_mut().find(|x| x.id == id) else {
                return Task::none();
            };
            let before = (sp.favorite, sp.muted);
            if let Some(f) = favorite {
                sp.favorite = f;
            }
            if let Some(m) = muted {
                sp.muted = m;
            }
            // Bei Fehler zurücksetzen (wie Tauri `setFlag`).
            let id2 = id.clone();
            s.call(
                move |app| async move { app.set_space_flag(&account, &id, favorite, muted).await },
                move |r| match r {
                    Err(e) => Message::FlagFailed(id2.clone(), before.0, before.1, e),
                    Ok(()) => Message::Ignore,
                },
            )
        }
        Message::FlagFailed(id, favorite, muted, e) => {
            if let Some(sp) = s.spaces.iter_mut().find(|x| x.id == id) {
                sp.favorite = favorite;
                sp.muted = muted;
            }
            s.error(e)
        }
        Message::Filter(q) => {
            s.filter = q;
            s.filter_seq += 1;
            s.joinable.clear();
            let seq = s.filter_seq;
            if s.filter.trim().is_empty()
                || s.current_kind() != Some(BackendKind::Mattermost)
                || !s.online()
            {
                return Task::none();
            }
            // Wie Tauri: Server-Suche nach Kanälen erst nach 300 ms Tipp-Pause.
            Task::perform(tokio::time::sleep(Duration::from_millis(300)), move |_| {
                Message::Browse(seq)
            })
        }
        Message::Browse(seq) => {
            let Some(account) = s.account.clone() else {
                return Task::none();
            };
            if seq != s.filter_seq {
                return Task::none();
            }
            let q = s.filter.trim().to_string();
            let app = s.app.clone();
            Task::perform(
                async move { app.browse_spaces(&account, &q).await.unwrap_or_default() },
                move |found| Message::Joinable(seq, found),
            )
        }
        Message::Joinable(seq, found) => {
            if seq == s.filter_seq {
                s.joinable = found;
            }
            Task::none()
        }
        Message::Join(id) => {
            let Some(account) = s.account.clone() else {
                return Task::none();
            };
            s.call(
                move |app| async move { app.join_space(&account, &id).await },
                Message::Joined,
            )
        }
        Message::Joined(result) => match result {
            Ok(space) => {
                let id = space.id.clone();
                s.spaces.retain(|x| x.id != id);
                s.spaces.push(space);
                s.filter.clear();
                s.joinable.clear();
                update(s, Message::OpenSpace(id))
            }
            Err(e) => s.error(e),
        },
        Message::Sort(sort) => {
            s.prefs.sort = sort;
            prefs::save(&s.prefs);
            Task::none()
        }
        Message::ResizeStart => {
            s.resizing = true;
            Task::none()
        }
        Message::ResizeMove(x) => {
            if s.resizing {
                s.prefs.sidebar_width = x.clamp(SIDEBAR_MIN, SIDEBAR_MAX);
            }
            Task::none()
        }
        Message::ResizeEnd => {
            if s.resizing {
                s.resizing = false;
                prefs::save(&s.prefs);
            }
            Task::none()
        }
        Message::ResizeReset => {
            s.prefs.sidebar_width = SIDEBAR_DEFAULT;
            prefs::save(&s.prefs);
            Task::none()
        }
        Message::Image(path, img) => {
            s.images.insert(path, img);
            Task::none()
        }
        Message::Zoom(z) => {
            s.prefs.zoom = if ZOOMS.contains(&z) { z } else { 1.0 };
            prefs::save(&s.prefs);
            Task::none()
        }
        Message::ZoomStep(step) => {
            let i = ZOOMS.iter().position(|z| *z == s.prefs.zoom).unwrap_or(2) as i32;
            let z = ZOOMS[(i + step).clamp(0, ZOOMS.len() as i32 - 1) as usize];
            update(s, Message::Zoom(z))
        }
        Message::Link(uri) => {
            if uri.starts_with("http://") || uri.starts_with("https://") {
                let _ = opener::open_browser(&uri);
            } else if let Some(addr) = uri.strip_prefix("mailto:") {
                // Ohne Parameter: manche Mailprogramme hängen per `?attach=/pfad` lokale Dateien an.
                let addr = addr.split('?').next().unwrap_or_default();
                let _ = opener::open_browser(format!("mailto:{addr}"));
            }
            Task::none()
        }
        Message::Settings(open) => {
            s.settings = open.then(|| Form {
                server: "https://".into(),
                client_id: s
                    .info
                    .as_ref()
                    .map(|i| i.google_client_id.clone())
                    .unwrap_or_default(),
                ..Form::default()
            });
            Task::none()
        }
        Message::FormInput(field, value) => {
            if let Some(f) = &mut s.settings {
                let target = match field {
                    FormField::Server => &mut f.server,
                    FormField::Login => &mut f.login,
                    FormField::Password => &mut f.password,
                    FormField::Mfa => &mut f.mfa,
                    FormField::ClientId => &mut f.client_id,
                    FormField::ClientSecret => &mut f.client_secret,
                };
                *target = value;
            }
            Task::none()
        }
        Message::Theme(name) => {
            s.look = Look::from_name(name);
            let notifications = s.info.as_ref().is_none_or(|i| i.notifications);
            let saved = s.app.set_settings(name.into(), notifications);
            let mut tasks = vec![refresh_info(s)];
            if let Err(e) = saved {
                tasks.push(s.error(e.to_string()));
            }
            if s.look.avatars {
                let msgs = s.messages.clone();
                tasks.push(s.prepare(&msgs));
            }
            Task::batch(tasks)
        }
        Message::Notifications(on) => {
            let theme = s.info.as_ref().map(|i| i.theme.clone()).unwrap_or_default();
            match s.app.set_settings(theme, on) {
                Ok(()) => refresh_info(s),
                Err(e) => s.error(e.to_string()),
            }
        }
        Message::AddMattermost => {
            let Some(f) = &mut s.settings else {
                return Task::none();
            };
            f.busy = true;
            f.message = None;
            let (id, server, login, password, mfa) = (
                f.relogin.clone(),
                f.server.clone(),
                f.login.clone(),
                f.password.clone(),
                f.mfa.clone(),
            );
            f.password.clear();
            f.mfa.clear();
            s.call(
                move |app| async move {
                    app.add_mattermost_account(id, &server, login, password, Some(mfa))
                        .await
                        .map(|a| a.id)
                },
                Message::Added,
            )
        }
        Message::AddGoogle(id) => {
            let Some(f) = &mut s.settings else {
                return Task::none();
            };
            f.busy = true;
            f.message = Some("Anmeldung im Browser abschließen …".into());
            s.call(
                move |app| async move {
                    app.add_google_account(id, |url| {
                        let _ = opener::open_browser(&url);
                    })
                    .await
                    .map(|a| a.id)
                },
                Message::Added,
            )
        }
        Message::Relogin(id) => {
            let account = s
                .info
                .as_ref()
                .and_then(|i| i.accounts.iter().find(|a| a.account.id == id))
                .map(|a| a.account.clone());
            match account {
                Some(a) if a.kind == BackendKind::Google => update(s, Message::AddGoogle(Some(id))),
                Some(a) => {
                    if let Some(f) = &mut s.settings {
                        f.relogin = Some(id);
                        f.server = a.server.unwrap_or_else(|| "https://".into());
                        f.login = a.login.unwrap_or_default();
                        f.message = Some(format!("{} neu anmelden: Passwort eingeben.", a.label));
                    }
                    Task::none()
                }
                None => Task::none(),
            }
        }
        Message::SaveGoogleClient => {
            if let Some(f) = &mut s.settings {
                let r = s.app.set_google_client(&f.client_id, &f.client_secret);
                f.client_secret.clear();
                f.message = Some(match r {
                    Ok(()) => "Google-Client gespeichert.".into(),
                    Err(e) => e.to_string(),
                });
            }
            refresh_info(s)
        }
        Message::ConfirmRemove(id) => {
            if let Some(f) = &mut s.settings {
                f.confirm_remove = id;
            }
            Task::none()
        }
        Message::RemoveAccount(id) => {
            if let Some(f) = &mut s.settings {
                f.confirm_remove = None;
            }
            s.mentions.remove(&id);
            s.call(
                move |app| async move { app.remove_account(&id).await.map(|_| String::new()) },
                Message::Added,
            )
        }
        Message::Added(result) => {
            if let Some(f) = &mut s.settings {
                f.busy = false;
                f.message = Some(match &result {
                    Ok(id) if id.is_empty() => "Konto entfernt.".into(),
                    Ok(_) => "Konto angemeldet.".into(),
                    Err(e) => e.clone(),
                });
                if result.is_ok() {
                    f.relogin = None;
                }
            }
            match result {
                Ok(id) if !id.is_empty() => {
                    s.account = None;
                    let select = update(s, Message::SelectAccount(id));
                    Task::batch([refresh_info(s), select])
                }
                _ => refresh_info(s),
            }
        }
        Message::CloseToast(id) => {
            s.toasts.retain(|t| t.id != id);
            Task::none()
        }
        Message::ToastAction(id, action) => {
            s.toasts.retain(|t| t.id != id);
            update(s, *action)
        }
        Message::Event(account, mut event) => {
            if let Some(n) = s.app.handle_event(&account, &mut event).notification {
                notify(n);
            }
            let current = s.account.as_ref() == Some(&account);
            match event {
                BackendEvent::MessageCreated { message } => {
                    let visible =
                        current && s.focused && Some(&message.space_id) == s.space.as_ref();
                    let mut tasks = Vec::new();
                    if !message.mine {
                        if visible {
                            let (a, sp) = (account.clone(), message.space_id.clone());
                            tasks.push(s.call(
                                move |app| async move { app.mark_read(&a, &sp).await },
                                |_| Message::Ignore,
                            ));
                        } else {
                            tasks.push(s.mark_unread(&account, &message));
                        }
                    }
                    if current {
                        tasks.push(s.upsert(message, true));
                    }
                    Task::batch(tasks)
                }
                BackendEvent::MessageUpdated { message } if current => s.upsert(message, false),
                BackendEvent::MessageUpdated { .. } => Task::none(),
                BackendEvent::MessageDeleted { message_id, .. } => {
                    if current {
                        s.messages.retain(|m| m.id != message_id);
                        if let Some(Rhs::Thread { replies, .. }) = &mut s.rhs {
                            replies.retain(|m| m.id != message_id);
                        }
                    }
                    Task::none()
                }
                BackendEvent::SpaceUpdated { mut space } => {
                    let visible = current && Some(&space.id) == s.space.as_ref() && s.focused;
                    if visible {
                        space.unread = false;
                        space.mention_count = 0;
                    }
                    s.mentions
                        .entry(account.clone())
                        .or_default()
                        .insert(space.id.clone(), space.mention_count);
                    if current {
                        match s.spaces.iter_mut().find(|x| x.id == space.id) {
                            Some(x) => *x = space,
                            None => s.spaces.push(space),
                        }
                    }
                    Task::none()
                }
                // Online-Status zeigt nur die Tauri-Variante (konzept/drift-native-tauri.md).
                BackendEvent::OwnPresence { .. } => Task::none(),
                BackendEvent::Status { online, detail } => {
                    let mut tasks = vec![refresh_info(s)];
                    if online {
                        tasks.push(s.load_spaces(account.clone()));
                        if current && s.space.is_some() {
                            tasks.push(s.load_page(false));
                        }
                    } else if let Some(d) = detail {
                        let label = s
                            .info
                            .as_ref()
                            .and_then(|i| i.accounts.iter().find(|a| a.account.id == account))
                            .map(|a| a.account.label.clone())
                            .unwrap_or_else(|| "Konto".into());
                        tasks.push(s.error(format!("{label}: {d}")));
                    }
                    Task::batch(tasks)
                }
            }
        }
        Message::Focus(focused) => {
            s.focused = focused;
            s.app.set_focused(focused);
            if focused {
                return s.mark_read_current();
            }
            Task::none()
        }
        Message::WindowSize(id, monitor, pos) => {
            let mut tasks = Vec::new();
            if let Some(m) = monitor {
                let size = iced::Size::new(
                    (m.width * 2.0 / 3.0).max(MIN_SIZE.width),
                    (m.height * 2.0 / 3.0).max(MIN_SIZE.height),
                );
                tasks.push(window::resize(id, size));
                // Mitte des zentrierten Startfensters beibehalten (bleibt auf demselben Bildschirm).
                if let Some(p) = pos {
                    let center_x = p.x + INITIAL_SIZE.width / 2.0;
                    let center_y = p.y + INITIAL_SIZE.height / 2.0;
                    tasks.push(window::move_to(
                        id,
                        iced::Point::new(center_x - size.width / 2.0, center_y - size.height / 2.0),
                    ));
                }
            }
            tasks.push(window::set_mode(id, window::Mode::Windowed));
            Task::batch(tasks)
        }
        Message::UpdateFound(release) => {
            s.update_available = release;
            Task::none()
        }
        Message::InstallUpdate => match s.update_available.clone() {
            Some(r) => {
                let info = s.toast("Update wird geladen …", ToastKind::Info, None);
                Task::batch([info, Task::perform(updater::install(r), Message::Updated)])
            }
            None => Task::none(),
        },
        Message::Updated(result) => match result {
            Ok(()) => updater::restart(),
            Err(e) => s.error(format!("Update: {e}")),
        },
        Message::Shoot => window::latest()
            .and_then(window::screenshot)
            .map(Message::Shot),
        Message::Shot(shot) => {
            if let Some(path) = std::env::var_os("CHATTY_SHOT") {
                let mut ppm =
                    format!("P6 {} {} 255\n", shot.size.width, shot.size.height).into_bytes();
                for px in shot.rgba.as_chunks::<4>().0 {
                    ppm.extend_from_slice(&px[..3]);
                }
                if let Err(e) = std::fs::write(path, ppm) {
                    eprintln!("Bild: {e}");
                }
            }
            iced::exit()
        }
        Message::Ignore => Task::none(),
    }
}

/// OS-Benachrichtigung in eigenem Thread (D-Bus/WinRT blockieren kurz).
fn notify(n: chatty_core::Notification) {
    std::thread::spawn(move || {
        let _ = notify_rust::Notification::new()
            .appname("chatty")
            .summary(&n.title)
            .body(&n.body)
            .show();
    });
}

/// Wartezeit vor dem Bild in Sekunden (`CHATTY_SHOT_DELAY`, Standard 2; länger für RAM-Messungen).
fn shot_delay() -> u64 {
    std::env::var("CHATTY_SHOT_DELAY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2)
}

/// Ans Ende des Verlaufs. Bei `anchor_bottom` zählt der Versatz vom unteren Rand, also START.
fn to_bottom() -> Task<Message> {
    operation::snap_to(MESSAGES, scrollable::RelativeOffset::START)
}

fn refresh_info(s: &State) -> Task<Message> {
    let app = s.app.clone();
    Task::perform(async move { app.info().await }, Message::Info)
}

fn subscription(s: &State) -> Subscription<Message> {
    let events = Subscription::run(|| {
        let rx = EVENTS.get().and_then(|m| m.lock().ok()?.take());
        iced::futures::stream::unfold(rx, |rx| async move {
            let mut rx = rx?;
            let (account, event) = rx.recv().await?;
            Some((Message::Event(account, event), Some(rx)))
        })
    });
    let focus = window::events().map(|(_, e)| match e {
        window::Event::Focused => Message::Focus(true),
        window::Event::Unfocused => Message::Focus(false),
        _ => Message::Ignore,
    });
    let keys = keyboard::listen().map(|e| match e {
        keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            ..
        } => Message::Escape,
        // Strg/Cmd + Plus/Minus/0: Schriftgröße wie im Browser.
        keyboard::Event::KeyPressed {
            key: keyboard::Key::Character(c),
            modifiers,
            ..
        } if modifiers.command() && !modifiers.alt() => match c.as_str() {
            "+" | "=" => Message::ZoomStep(1),
            "-" => Message::ZoomStep(-1),
            "0" => Message::Zoom(1.0),
            _ => Message::Ignore,
        },
        _ => Message::Ignore,
    });
    let mut subs = vec![events, focus, keys];
    // Seitenleiste ziehen: Mausbewegung nur abonnieren, solange gezogen wird.
    if s.resizing {
        subs.push(event::listen_with(|e, _, _| match e {
            iced::Event::Mouse(mouse::Event::CursorMoved { position }) => {
                Some(Message::ResizeMove(position.x))
            }
            iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                Some(Message::ResizeEnd)
            }
            _ => None,
        }));
    }
    Subscription::batch(subs)
}

#[cfg(test)]
mod tests {
    #[test]
    fn app_icon_decodes() {
        let icon = include_bytes!("../../icons/128x128.png");
        assert!(iced::window::icon::from_file_data(icon, None).is_ok());
    }

    #[test]
    fn rejects_huge_images() {
        assert!(super::small_image(
            include_bytes!("../../icons/128x128.png"),
            2048
        ));
        // PNG-Kopf mit 30000×30000 (Dekompressionsbombe), ohne Bilddaten.
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&30000u32.to_be_bytes());
        png.extend_from_slice(&30000u32.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        assert!(!super::small_image(&png, 4096));
        assert!(!super::small_image(b"kein bild", 4096));
    }
}
