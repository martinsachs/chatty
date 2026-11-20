//! chatty-native: Oberfläche mit iced statt WebView (ADR 08), auf demselben Kern wie die Tauri-Variante.
//!
//! `chatty-native --open <space-id>` öffnet den Space nach dem Start (Messung, Screenshots).
//! Mit `CHATTY_SHOT=<datei.ppm>` bleibt das Fenster unsichtbar (kein Fokus, keine Eingaben), speichert nach dem
//! Laden des Verlaufs (nach `CHATTY_SHOT_DELAY` Sekunden, Standard 2) ein Bild der App und beendet sich
//! (Prüfhilfe, RAM-Messung ohne sichtbares Fenster).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod look;
mod text;
mod updater;
mod view;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};

use chatty_core::app::Events;
use chatty_core::model::{Attachment, BackendEvent, Message as Msg, Page, Space};
use chatty_core::{App, AppInfo};
use iced::widget::{Id, image, markdown, operation, scrollable};
use iced::{Subscription, Task, keyboard, window};

use crate::look::Look;

/// Ereignis-Empfänger des Kerns; die Subscription holt ihn einmal ab.
static EVENTS: OnceLock<Mutex<Option<Events>>> = OnceLock::new();

const MESSAGES: Id = Id::new("messages");
const COMPOSER: Id = Id::new("composer");
/// Abstand zum oberen Rand, ab dem ältere Nachrichten geladen werden.
const LOAD_OLDER_PX: f32 = 80.0;
/// Schnellauswahl für Reaktionen.
pub const QUICK_EMOJI: [&str; 6] = ["👍", "❤️", "😂", "🎉", "👀", "✅"];

fn main() -> iced::Result {
    chatty_core::backend::init_tls();
    let shot = std::env::var_os("CHATTY_SHOT").is_some();
    iced::application(boot, update, view::view)
        .title(|s: &State| match s.current_space() {
            Some(sp) => format!("{} – chatty", sp.name),
            None => "chatty".into(),
        })
        .subscription(subscription)
        .theme(|s: &State| s.look.theme())
        .window(window::Settings {
            size: (1100.0, 700.0).into(),
            visible: !shot,
            ..window::Settings::default()
        })
        .run()
}

/// Rechte Leiste (eine Instanz; Größenunterschied der Varianten egal).
#[allow(clippy::large_enum_variant)]
pub enum Rhs {
    Thread {
        root: Msg,
        replies: Vec<Msg>,
        draft: String,
    },
    Search {
        query: String,
        results: Option<Vec<Msg>>,
    },
}

/// Formular „Konten und Einstellungen“.
#[derive(Default)]
pub struct Form {
    pub server: String,
    pub login: String,
    pub password: String,
    pub mfa: String,
    pub client_id: String,
    pub client_secret: String,
    pub busy: bool,
    pub message: Option<String>,
}

pub struct State {
    pub app: Arc<App>,
    pub info: Option<AppInfo>,
    pub look: Look,
    pub account: Option<String>,
    pub spaces: Vec<Space>,
    pub space: Option<String>,
    pub messages: Vec<Msg>,
    pub cursor: Option<String>,
    pub loading: bool,
    /// Zählt Ladevorgänge; Antworten zu älteren Ständen werden verworfen.
    seq: u64,
    pub draft: String,
    /// Nachricht, die gerade im Eingabefeld bearbeitet wird.
    pub editing: Option<Msg>,
    /// Gewählte Anhänge für die nächste Nachricht.
    pub files: Vec<PathBuf>,
    pub error: Option<String>,
    pub rhs: Option<Rhs>,
    pub search: String,
    /// Reaktions-Schnellauswahl offen an dieser Nachricht.
    pub picker: Option<String>,
    pub settings: Option<Form>,
    /// Geparster Text je Nachrichten-ID.
    pub md: HashMap<String, Vec<markdown::Item>>,
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
    Page {
        seq: u64,
        older: bool,
        result: Result<Page, String>,
    },
    Scrolled(scrollable::Viewport),
    Draft(String),
    Send,
    Sent(Result<Msg, String>),
    Edit(Msg),
    Edited(Result<Msg, String>),
    Delete(Msg),
    Deleted(String, Result<(), String>),
    Escape,
    PickFiles,
    Picked(Vec<PathBuf>),
    RemoveFile(usize),
    Save(Attachment),
    Saved(Result<bool, String>),
    Picker(Option<String>),
    React(Msg, String),
    Reacted(Result<(), String>),
    OpenThread(Msg),
    Thread(String, Result<Vec<Msg>, String>),
    ThreadDraft(String),
    ThreadSend,
    ThreadSent(Result<Msg, String>),
    CloseRhs,
    SearchInput(String),
    Search,
    Found(String, Result<Vec<Msg>, String>),
    Flag(String, Option<bool>, Option<bool>),
    Flagged(Result<(), String>),
    Image(String, Option<image::Handle>),
    Link(markdown::Uri),
    Settings(bool),
    FormInput(FormField, String),
    Theme(&'static str),
    Notifications(bool),
    AddMattermost,
    AddGoogle,
    SaveGoogleClient,
    RemoveAccount(String),
    Added(Result<String, String>),
    Focus(bool),
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
        account: None,
        spaces: Vec::new(),
        space: None,
        messages: Vec::new(),
        cursor: None,
        loading: false,
        seq: 0,
        draft: String::new(),
        editing: None,
        files: Vec::new(),
        error: None,
        rhs: None,
        search: String::new(),
        picker: None,
        settings: None,
        md: HashMap::new(),
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
    let check = if std::env::var_os("CHATTY_SHOT").is_some() {
        Task::none()
    } else {
        Task::perform(updater::check(), Message::UpdateFound)
    };
    (state, Task::batch([start, check]))
}

impl State {
    pub fn current_space(&self) -> Option<&Space> {
        self.spaces
            .iter()
            .find(|s| Some(&s.id) == self.space.as_ref())
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
                let img = app
                    .avatar(&path)
                    .await
                    .map(|a| image::Handle::from_bytes(a.data));
                (path, img)
            },
            |(p, i)| Message::Image(p, i),
        )
    }

    fn upsert(&mut self, m: Msg) -> Task<Message> {
        if Some(&m.space_id) != self.space.as_ref() {
            return Task::none();
        }
        let task = self.prepare(std::slice::from_ref(&m));
        if m.is_reply {
            if let Some(Rhs::Thread { root, replies, .. }) = &mut self.rhs
                && root.thread_id == m.thread_id
            {
                match replies.iter_mut().find(|x| x.id == m.id) {
                    Some(x) => *x = m.clone(),
                    None => replies.push(m.clone()),
                }
            }
            if self.counted_replies.insert(m.id.clone())
                && let Some(root) = self
                    .messages
                    .iter_mut()
                    .find(|x| x.thread_id.is_some() && x.thread_id == m.thread_id)
            {
                root.reply_count += 1;
            }
            return task;
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

fn update(s: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::Info(info) => {
            let first = info.accounts.first().map(|a| a.account.id.clone());
            let known = s
                .account
                .as_ref()
                .is_some_and(|a| info.accounts.iter().any(|x| &x.account.id == a));
            s.info = Some(info);
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
            if let Some((a, _)) = s.account_space() {
                let app = s.app.clone();
                tokio::spawn(async move { app.set_active(&a, None).await });
            }
            s.account = Some(id.clone());
            s.space = None;
            s.messages.clear();
            s.rhs = None;
            s.spaces = s.app.cached_spaces(&id).unwrap_or_default();
            sort_spaces(&mut s.spaces);
            s.load_spaces(id)
        }
        Message::Spaces(account, result) => {
            match result {
                Ok(mut spaces) if s.account.as_ref() == Some(&account) => {
                    sort_spaces(&mut spaces);
                    s.spaces = spaces;
                    if let Some(id) = s.open_on_start.take() {
                        return update(s, Message::OpenSpace(id));
                    }
                }
                Ok(_) => {}
                // Konto meldet sich evtl. erst noch an: beim Status-Ereignis erneut laden.
                Err(e) => s.error = Some(e),
            }
            Task::none()
        }
        Message::Emojis(account, names) => {
            s.custom_emojis.insert(account, names.into_iter().collect());
            Task::none()
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
            s.error = None;
            s.editing = None;
            s.picker = None;
            if matches!(s.rhs, Some(Rhs::Thread { .. })) {
                s.rhs = None;
            }
            if let Some(sp) = s.spaces.iter_mut().find(|x| x.id == id) {
                sp.unread = false;
                sp.mention_count = 0;
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
                                tokio::time::sleep(std::time::Duration::from_secs(shot_delay())),
                                |_| Message::Shoot,
                            ),
                        ])
                    } else {
                        to_bottom()
                    };
                    Task::batch([prep, scroll])
                }
                Err(e) => {
                    s.error = Some(e);
                    Task::none()
                }
            }
        }
        Message::Scrolled(vp) => {
            // anchor_bottom: absolute_offset zählt vom unteren Rand, reversed vom oberen.
            if vp.absolute_offset_reversed().y < LOAD_OLDER_PX && !s.loading {
                return s.load_page(true);
            }
            Task::none()
        }
        Message::Draft(t) => {
            s.draft = t;
            Task::none()
        }
        Message::Send => {
            let Some((account, space)) = s.account_space() else {
                return Task::none();
            };
            let text = s.draft.trim().to_string();
            if let Some(m) = s.editing.clone() {
                if text.is_empty() {
                    return Task::none();
                }
                return s.call(
                    move |app| async move { app.edit(&account, &m.space_id, &m.id, &text).await },
                    Message::Edited,
                );
            }
            if text.is_empty() && s.files.is_empty() {
                return Task::none();
            }
            let files = s.files.clone();
            s.call(
                move |app| async move { app.send(&account, &space, &text, None, &files).await },
                Message::Sent,
            )
        }
        Message::Sent(result) => match result {
            Ok(m) => {
                s.draft.clear();
                s.files.clear();
                Task::batch([s.upsert(m), to_bottom()])
            }
            Err(e) => {
                s.error = Some(e);
                Task::none()
            }
        },
        Message::Edit(m) => {
            s.draft = m.text.clone();
            s.editing = Some(m);
            operation::focus(COMPOSER)
        }
        Message::Edited(result) => match result {
            Ok(m) => {
                s.editing = None;
                s.draft.clear();
                s.upsert(m)
            }
            Err(e) => {
                s.error = Some(e);
                Task::none()
            }
        },
        Message::Escape => {
            if s.editing.take().is_some() {
                s.draft.clear();
            } else if s.picker.is_some() {
                s.picker = None;
            } else if s.settings.is_some() {
                s.settings = None;
            } else {
                s.rhs = None;
            }
            Task::none()
        }
        Message::Delete(m) => {
            let Some(account) = s.account.clone() else {
                return Task::none();
            };
            let id = m.id.clone();
            s.call(
                move |app| async move { app.delete(&account, &m.space_id, &m.id).await },
                move |r| Message::Deleted(id.clone(), r),
            )
        }
        Message::Deleted(id, result) => {
            match result {
                Ok(()) => {
                    s.messages.retain(|m| m.id != id);
                    if let Some(Rhs::Thread { replies, .. }) = &mut s.rhs {
                        replies.retain(|m| m.id != id);
                    }
                }
                Err(e) => s.error = Some(e),
            }
            Task::none()
        }
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
            s.files.extend(paths);
            Task::none()
        }
        Message::RemoveFile(i) => {
            if i < s.files.len() {
                s.files.remove(i);
            }
            Task::none()
        }
        Message::Save(att) => {
            let Some(account) = s.account.clone() else {
                return Task::none();
            };
            s.call(
                move |app| async move {
                    let Some(file) = rfd::AsyncFileDialog::new()
                        .set_file_name(&att.name)
                        .save_file()
                        .await
                    else {
                        return Ok(false);
                    };
                    app.save_attachment(&account, &att, file.path()).await?;
                    Ok(true)
                },
                Message::Saved,
            )
        }
        Message::Saved(result) => {
            if let Err(e) = result {
                s.error = Some(e);
            }
            Task::none()
        }
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
            if let Some(Rhs::Thread { root, replies, .. }) = &mut s.rhs {
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
                Message::Reacted,
            )
        }
        Message::Reacted(result) | Message::Flagged(result) => {
            if let Err(e) = result {
                s.error = Some(e);
            }
            Task::none()
        }
        Message::OpenThread(root) => {
            let (Some(account), Some(thread)) = (s.account.clone(), root.thread_id.clone()) else {
                return Task::none();
            };
            let root_id = root.id.clone();
            let space = root.space_id.clone();
            s.rhs = Some(Rhs::Thread {
                root,
                replies: Vec::new(),
                draft: String::new(),
            });
            s.call(
                move |app| async move { app.list_thread(&account, &space, &thread).await },
                move |r| Message::Thread(root_id.clone(), r),
            )
        }
        Message::Thread(root_id, result) => {
            match result {
                Ok(all) => {
                    let task = s.prepare(&all);
                    if let Some(Rhs::Thread { root, replies, .. }) = &mut s.rhs
                        && root.id == root_id
                    {
                        *replies = all.into_iter().filter(|m| m.id != root_id).collect();
                    }
                    return task;
                }
                Err(e) => s.error = Some(e),
            }
            Task::none()
        }
        Message::ThreadDraft(t) => {
            if let Some(Rhs::Thread { draft, .. }) = &mut s.rhs {
                *draft = t;
            }
            Task::none()
        }
        Message::ThreadSend => {
            let Some(account) = s.account.clone() else {
                return Task::none();
            };
            let Some(Rhs::Thread { root, draft, .. }) = &s.rhs else {
                return Task::none();
            };
            let text = draft.trim().to_string();
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
                if let Some(Rhs::Thread { draft, .. }) = &mut s.rhs {
                    draft.clear();
                }
                s.upsert(m)
            }
            Err(e) => {
                s.error = Some(e);
                Task::none()
            }
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
        Message::Found(query, result) => {
            match result {
                Ok(found) => {
                    let task = s.prepare(&found);
                    if let Some(Rhs::Search { query: q, results }) = &mut s.rhs
                        && *q == query
                    {
                        *results = Some(found);
                    }
                    return task;
                }
                Err(e) => s.error = Some(e),
            }
            Task::none()
        }
        Message::Flag(id, favorite, muted) => {
            let Some(account) = s.account.clone() else {
                return Task::none();
            };
            if let Some(sp) = s.spaces.iter_mut().find(|x| x.id == id) {
                if let Some(f) = favorite {
                    sp.favorite = f;
                }
                if let Some(m) = muted {
                    sp.muted = m;
                }
            }
            sort_spaces(&mut s.spaces);
            s.call(
                move |app| async move { app.set_space_flag(&account, &id, favorite, muted).await },
                Message::Flagged,
            )
        }
        Message::Image(path, img) => {
            s.images.insert(path, img);
            Task::none()
        }
        Message::Link(uri) => {
            if uri.starts_with("http://")
                || uri.starts_with("https://")
                || uri.starts_with("mailto:")
            {
                let _ = opener::open_browser(&uri);
            }
            Task::none()
        }
        Message::Settings(open) => {
            s.settings = open.then(|| Form {
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
            if let Err(e) = s.app.set_settings(name.into(), notifications) {
                s.error = Some(e.to_string());
            }
            if s.look.avatars {
                let msgs = s.messages.clone();
                return Task::batch([refresh_info(s), s.prepare(&msgs)]);
            }
            refresh_info(s)
        }
        Message::Notifications(on) => {
            let theme = s.info.as_ref().map(|i| i.theme.clone()).unwrap_or_default();
            if let Err(e) = s.app.set_settings(theme, on) {
                s.error = Some(e.to_string());
            }
            refresh_info(s)
        }
        Message::AddMattermost => {
            let Some(f) = &mut s.settings else {
                return Task::none();
            };
            f.busy = true;
            f.message = None;
            let (server, login, password, mfa) = (
                f.server.clone(),
                f.login.clone(),
                f.password.clone(),
                f.mfa.clone(),
            );
            f.password.clear();
            s.call(
                move |app| async move {
                    app.add_mattermost_account(None, &server, login, password, Some(mfa))
                        .await
                        .map(|a| a.id)
                },
                Message::Added,
            )
        }
        Message::AddGoogle => {
            let Some(f) = &mut s.settings else {
                return Task::none();
            };
            f.busy = true;
            f.message = Some("Anmeldung im Browser abschließen …".into());
            s.call(
                move |app| async move {
                    app.add_google_account(None, |url| {
                        let _ = opener::open_browser(&url);
                    })
                    .await
                    .map(|a| a.id)
                },
                Message::Added,
            )
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
        Message::RemoveAccount(id) => s.call(
            move |app| async move { app.remove_account(&id).await.map(|_| String::new()) },
            Message::Added,
        ),
        Message::Added(result) => {
            if let Some(f) = &mut s.settings {
                f.busy = false;
                f.message = Some(match &result {
                    Ok(id) if id.is_empty() => "Konto entfernt.".into(),
                    Ok(_) => "Konto angemeldet.".into(),
                    Err(e) => e.clone(),
                });
            }
            match result {
                Ok(id) if !id.is_empty() => {
                    let select = update(s, Message::SelectAccount(id));
                    Task::batch([refresh_info(s), select])
                }
                _ => refresh_info(s),
            }
        }
        Message::Event(account, mut event) => {
            if let Some(n) = s.app.handle_event(&account, &mut event) {
                notify(n);
            }
            if s.account.as_ref() != Some(&account) {
                return if matches!(event, BackendEvent::Status { .. }) {
                    refresh_info(s)
                } else {
                    Task::none()
                };
            }
            match event {
                BackendEvent::MessageCreated { message } => {
                    let visible = s.focused && Some(&message.space_id) == s.space.as_ref();
                    let read = if visible && !message.mine {
                        let (a, sp) = (account.clone(), message.space_id.clone());
                        s.call(
                            move |app| async move { app.mark_read(&a, &sp).await },
                            |_| Message::Ignore,
                        )
                    } else {
                        Task::none()
                    };
                    Task::batch([s.upsert(message), read])
                }
                BackendEvent::MessageUpdated { message } => s.upsert(message),
                BackendEvent::MessageDeleted { message_id, .. } => {
                    s.messages.retain(|m| m.id != message_id);
                    if let Some(Rhs::Thread { replies, .. }) = &mut s.rhs {
                        replies.retain(|m| m.id != message_id);
                    }
                    Task::none()
                }
                BackendEvent::SpaceUpdated { mut space } => {
                    if Some(&space.id) == s.space.as_ref() && s.focused {
                        space.unread = false;
                        space.mention_count = 0;
                    }
                    match s.spaces.iter_mut().find(|x| x.id == space.id) {
                        Some(x) => *x = space,
                        None => s.spaces.push(space),
                    }
                    sort_spaces(&mut s.spaces);
                    Task::none()
                }
                BackendEvent::Status { online, .. } => {
                    let mut tasks = vec![refresh_info(s)];
                    if online {
                        s.error = None;
                        tasks.push(s.load_spaces(account));
                        if s.space.is_some() {
                            tasks.push(s.load_page(false));
                        }
                    }
                    Task::batch(tasks)
                }
            }
        }
        Message::Focus(focused) => {
            s.focused = focused;
            s.app.set_focused(focused);
            Task::none()
        }
        Message::UpdateFound(release) => {
            s.update_available = release;
            Task::none()
        }
        Message::InstallUpdate => match s.update_available.clone() {
            Some(r) => Task::perform(updater::install(r), Message::Updated),
            None => Task::none(),
        },
        Message::Updated(result) => match result {
            Ok(()) => updater::restart(),
            Err(e) => {
                s.error = Some(format!("Update: {e}"));
                Task::none()
            }
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

/// Favoriten zuerst, dann nach letzter Aktivität.
fn sort_spaces(spaces: &mut [Space]) {
    spaces.sort_by(|a, b| {
        b.favorite
            .cmp(&a.favorite)
            .then(b.last_active.cmp(&a.last_active))
    });
}

fn subscription(_s: &State) -> Subscription<Message> {
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
        _ => Message::Ignore,
    });
    Subscription::batch([events, focus, keys])
}
