//! chatty-native: Prototyp mit iced statt WebView (ADR 08).
//!
//! Umfang: gespeicherte Konten (aus `accounts.json` und Schlüsselbund, wie die Tauri-Variante), Kanal-
//! liste, Verlauf mit Nachladen beim Hochscrollen, Senden, Live-Ereignisse, Optik aus `chatty.toml`.
//! Konten anlegen, Threads, Reaktionen, Anhänge, Suche und Benachrichtigungen fehlen noch.
//!
//! `chatty-native --open <space-id>` öffnet den Space nach dem Start (Messung, Screenshots).
//! Mit `CHATTY_SHOT=<datei.ppm>` bleibt das Fenster unsichtbar (kein Fokus, keine Eingaben), speichert nach dem
//! Laden des Verlaufs (nach `CHATTY_SHOT_DELAY` Sekunden, Standard 2) ein Bildschirmfoto der App und beendet
//! sich (Prüfhilfe, RAM-Messung ohne sichtbares Fenster).

use std::sync::{Arc, Mutex, OnceLock};

use chatty_core::app::Events;
use chatty_core::model::{BackendEvent, Message as Msg, Page, Space, SpaceKind};
use chatty_core::{App, AppInfo};
use iced::font::Weight;
use iced::widget::{Id, button, column, container, operation, row, scrollable, text, text_input};
use iced::{Color, Element, Font, Length, Subscription, Task, Theme, color, window};

/// Ereignis-Empfänger des Kerns; die Subscription holt ihn einmal ab.
static EVENTS: OnceLock<Mutex<Option<Events>>> = OnceLock::new();

const MESSAGES: Id = Id::new("messages");
const COMPOSER: Id = Id::new("composer");
/// Abstand zum oberen Rand, ab dem ältere Nachrichten geladen werden.
const LOAD_OLDER_PX: f32 = 80.0;

fn main() -> iced::Result {
    chatty_core::backend::init_tls();
    let shot = std::env::var_os("CHATTY_SHOT").is_some();
    iced::application(boot, update, view)
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

struct State {
    app: Arc<App>,
    info: Option<AppInfo>,
    look: Look,
    account: Option<String>,
    spaces: Vec<Space>,
    space: Option<String>,
    messages: Vec<Msg>,
    cursor: Option<String>,
    loading: bool,
    /// Zählt Ladevorgänge; Antworten zu älteren Ständen werden verworfen.
    seq: u64,
    draft: String,
    error: Option<String>,
    /// Aus `--open`: nach dem ersten Laden der Kanalliste öffnen.
    open_on_start: Option<String>,
}

#[derive(Debug, Clone)]
enum Message {
    Info(AppInfo),
    Event(String, BackendEvent),
    Spaces(String, Result<Vec<Space>, String>),
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
    Focus(bool),
    /// Prüfhilfe `CHATTY_SHOT`: Bild aufnehmen bzw. gespeichert.
    Shoot,
    Shot(window::Screenshot),
    Ignore,
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
        error: None,
        open_on_start: std::env::args().skip_while(|a| a != "--open").nth(1),
    };
    let task = Task::perform(
        async move {
            app.restore_all();
            app.info().await
        },
        Message::Info,
    );
    (state, task)
}

impl State {
    fn current_space(&self) -> Option<&Space> {
        self.spaces
            .iter()
            .find(|s| Some(&s.id) == self.space.as_ref())
    }

    fn load_spaces(&self, account: String) -> Task<Message> {
        let app = self.app.clone();
        Task::perform(
            async move {
                let r = app.list_spaces(&account).await.map_err(|e| e.to_string());
                (account, r)
            },
            |(a, r)| Message::Spaces(a, r),
        )
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

    fn upsert(&mut self, m: Msg) {
        if m.is_reply || Some(&m.space_id) != self.space.as_ref() {
            return;
        }
        match self.messages.iter_mut().find(|x| x.id == m.id) {
            Some(x) => *x = m,
            None => {
                self.messages.push(m);
                self.messages.sort_by_key(|m| m.created);
            }
        }
    }
}

fn update(s: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::Info(info) => {
            let first = info.accounts.first().map(|a| a.account.id.clone());
            s.info = Some(info);
            if let (None, Some(id)) = (&s.account, first) {
                return update(s, Message::SelectAccount(id));
            }
            Task::none()
        }
        Message::SelectAccount(id) => {
            s.account = Some(id.clone());
            s.space = None;
            s.messages.clear();
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
        Message::OpenSpace(id) => {
            let Some(account) = s.account.clone() else {
                return Task::none();
            };
            s.seq += 1;
            s.space = Some(id.clone());
            s.messages = s.app.cached_messages(&account, &id).unwrap_or_default();
            s.cursor = None;
            s.error = None;
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
            Task::batch([s.load_page(false), side, operation::focus(COMPOSER)])
        }
        Message::Page { seq, older, result } => {
            if seq != s.seq {
                return Task::none();
            }
            s.loading = false;
            match result {
                Ok(page) => {
                    if older {
                        let known: std::collections::HashSet<_> =
                            s.messages.iter().map(|m| m.id.clone()).collect();
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
                    if older {
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
                    }
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
            let (Some(account), Some(space)) = (s.account.clone(), s.space.clone()) else {
                return Task::none();
            };
            let text = s.draft.trim().to_string();
            if text.is_empty() {
                return Task::none();
            }
            let app = s.app.clone();
            Task::perform(
                async move {
                    app.send(&account, &space, &text, None, &[])
                        .await
                        .map_err(|e| e.to_string())
                },
                Message::Sent,
            )
        }
        Message::Sent(result) => {
            match result {
                Ok(m) => {
                    s.draft.clear();
                    s.upsert(m);
                    return to_bottom();
                }
                Err(e) => s.error = Some(e),
            }
            Task::none()
        }
        Message::Event(account, mut event) => {
            if let Some(n) = s.app.handle_event(&account, &mut event) {
                // Prototyp: noch keine OS-Benachrichtigung.
                eprintln!("Benachrichtigung: {}: {}", n.title, n.body);
            }
            if s.account.as_ref() != Some(&account) {
                return refresh_info(s);
            }
            match event {
                BackendEvent::MessageCreated { message }
                | BackendEvent::MessageUpdated { message } => s.upsert(message),
                BackendEvent::MessageDeleted { message_id, .. } => {
                    s.messages.retain(|m| m.id != message_id)
                }
                BackendEvent::SpaceUpdated { mut space } => {
                    if Some(&space.id) == s.space.as_ref() {
                        space.unread = false;
                        space.mention_count = 0;
                    }
                    match s.spaces.iter_mut().find(|x| x.id == space.id) {
                        Some(x) => *x = space,
                        None => s.spaces.push(space),
                    }
                    sort_spaces(&mut s.spaces);
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
                    return Task::batch(tasks);
                }
            }
            Task::none()
        }
        Message::Focus(focused) => {
            s.app.set_focused(focused);
            Task::none()
        }
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
    Subscription::batch([events, focus])
}

// ---------- Ansicht ----------

fn view(s: &State) -> Element<'_, Message> {
    row![sidebar(s), center(s)].height(Length::Fill).into()
}

fn sidebar(s: &State) -> Element<'_, Message> {
    let look = &s.look;
    let mut col = column![].spacing(2).padding(8);
    if let Some(info) = &s.info {
        for a in &info.accounts {
            let label = format!(
                "{}{}",
                a.account.label,
                if a.online { "" } else { " (getrennt)" }
            );
            let active = s.account.as_ref() == Some(&a.account.id);
            col = col.push(
                button(text(label).font(look.font(true)).color(look.sidebar_text))
                    .on_press(Message::SelectAccount(a.account.id.clone()))
                    .style(move |_, _| look.item_style(active))
                    .width(Length::Fill),
            );
        }
        if info.accounts.is_empty() {
            col = col.push(
                text("Kein Konto. Konten zuerst in der Tauri-Variante anlegen.")
                    .color(look.sidebar_text),
            );
        }
    }
    col = col.push(text("").size(6));
    for sp in &s.spaces {
        let icon = match sp.kind {
            SpaceKind::Channel => "#",
            SpaceKind::Direct => "●",
            SpaceKind::Group => "◆",
        };
        let fav = if sp.favorite { " ★" } else { "" };
        let badge = if sp.mention_count > 0 {
            format!(" ({})", sp.mention_count)
        } else {
            String::new()
        };
        let bold = sp.unread && !sp.muted || sp.mention_count > 0;
        let active = s.space.as_ref() == Some(&sp.id);
        col = col.push(
            button(
                text(format!("{icon} {}{fav}{badge}", sp.name))
                    .font(look.font(bold))
                    .color(look.sidebar_text)
                    .wrapping(text::Wrapping::None),
            )
            .on_press(Message::OpenSpace(sp.id.clone()))
            .style(move |_, _| look.item_style(active))
            .padding([3, 8])
            .width(Length::Fill),
        );
    }
    container(scrollable(col).height(Length::Fill))
        .width(260)
        .height(Length::Fill)
        .style(move |_| container::background(look.sidebar_bg))
        .into()
}

fn center(s: &State) -> Element<'_, Message> {
    let look = &s.look;
    let Some(space) = s.current_space() else {
        return container(text("Kanal links auswählen.").font(look.font(false)))
            .center(Length::Fill)
            .into();
    };
    let header = container(text(space.name.clone()).font(look.font(true)).size(16))
        .padding([8, 16])
        .width(Length::Fill)
        .style(move |_| container::background(look.header_bg).color(look.header_text));

    let mut list = column![].spacing(2).padding([8, 16]);
    if s.cursor.is_none() && !s.loading && !s.messages.is_empty() {
        list = list.push(text(format!("Anfang von {}", space.name)).color(look.muted));
    } else if s.loading {
        list = list.push(text("Lade …").color(look.muted));
    }
    for m in &s.messages {
        list = list.push(message_line(look, m));
    }
    let messages = scrollable(list)
        .id(MESSAGES)
        .anchor_bottom()
        .on_scroll(Message::Scrolled)
        .height(Length::Fill)
        .width(Length::Fill);

    let online = s
        .info
        .as_ref()
        .and_then(|i| {
            i.accounts
                .iter()
                .find(|a| Some(&a.account.id) == s.account.as_ref())
        })
        .is_some_and(|a| a.online);
    let mut input = text_input(&format!("Nachricht an {}", space.name), &s.draft)
        .id(COMPOSER)
        .font(look.font(false))
        .padding(10);
    if online {
        input = input.on_input(Message::Draft).on_submit(Message::Send);
    }
    let prompt = text(look.prompt).font(look.font(false));
    let composer = container(
        row![prompt, input]
            .spacing(6)
            .align_y(iced::Alignment::Center),
    )
    .padding(8)
    .style(move |_| container::background(look.composer_bg));

    let mut col = column![header, messages];
    if let Some(e) = &s.error {
        col = col.push(container(text(e.clone()).color(look.error)).padding([4, 16]));
    }
    col.push(composer).into()
}

fn message_line<'a>(look: &'a Look, m: &'a Msg) -> Element<'a, Message> {
    let time = chrono::DateTime::from_timestamp_millis(m.created)
        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default();
    let body = if m.edited.is_some() {
        format!("{} (bearbeitet)", m.text)
    } else {
        m.text.clone()
    };
    let body_color = if m.mentions_me && !m.mine {
        look.mention
    } else {
        look.text
    };
    row![
        text(format!("[{time}]"))
            .font(look.font(false))
            .color(look.muted),
        text(format!("<{}>", m.sender.name))
            .font(look.font(true))
            .color(look.nick(&m.sender.name)),
        text(body)
            .font(look.font(false))
            .color(body_color)
            .width(Length::Fill),
    ]
    .spacing(8)
    .into()
}

// ---------- Optiken ----------

/// Farben und Schrift je Optik; Namen wie in `chatty.toml` (`theme`).
struct Look {
    name: &'static str,
    bg: Color,
    text: Color,
    muted: Color,
    accent: Color,
    mention: Color,
    error: Color,
    sidebar_bg: Color,
    sidebar_text: Color,
    sidebar_active: Color,
    header_bg: Color,
    header_text: Color,
    composer_bg: Color,
    mono: bool,
    prompt: &'static str,
    nicks: &'static [Color],
}

const NICKS_LIGHT: &[Color] = &[
    color!(0x009300),
    color!(0xfc0000),
    color!(0x7f0000),
    color!(0x9c009c),
    color!(0xfc7f00),
    color!(0x00009c),
    color!(0x009393),
    color!(0x2a6f97),
];
const NICKS_DOS: &[Color] = &[color!(0xffff55)];

impl Look {
    fn from_name(name: &str) -> Look {
        let base = Look {
            name: "mattermost",
            bg: color!(0xffffff),
            text: color!(0x3f4350),
            muted: color!(0x8b8e97),
            accent: color!(0x1c58d9),
            mention: color!(0x386fe5),
            error: color!(0xd24b4e),
            sidebar_bg: color!(0x1e325c),
            sidebar_text: color!(0xffffff),
            sidebar_active: color!(0x3b4c75),
            header_bg: color!(0xffffff),
            header_text: color!(0x3f4350),
            composer_bg: color!(0xffffff),
            mono: false,
            prompt: "",
            nicks: NICKS_LIGHT,
        };
        match name {
            "slack" => Look {
                name: "slack",
                text: color!(0x1d1c1d),
                accent: color!(0x007a5a),
                mention: color!(0x1264a3),
                sidebar_bg: color!(0x3f0e40),
                sidebar_active: color!(0x1164a3),
                header_text: color!(0x1d1c1d),
                ..base
            },
            "ehex" => Look {
                name: "ehex",
                text: color!(0x111827),
                accent: color!(0x5a40dd),
                mention: color!(0xf726a0),
                sidebar_bg: color!(0x1b1442),
                sidebar_active: color!(0x463a8a),
                header_bg: color!(0x100b28),
                header_text: color!(0xffffff),
                ..base
            },
            "mirc" => Look {
                name: "mirc",
                text: color!(0x000000),
                muted: color!(0x7f7f7f),
                accent: color!(0x000080),
                mention: color!(0xfc0000),
                error: color!(0xfc0000),
                sidebar_bg: color!(0xffffff),
                sidebar_text: color!(0x000000),
                sidebar_active: color!(0xc0c0ff),
                header_bg: color!(0x000080),
                header_text: color!(0xffffff),
                composer_bg: color!(0xc0c0c0),
                mono: true,
                ..base
            },
            "dos" => Look {
                name: "dos",
                bg: color!(0x0000aa),
                text: color!(0x55ffff),
                muted: color!(0xffffff),
                accent: color!(0x00aaaa),
                mention: color!(0xffff55),
                error: color!(0xff5555),
                sidebar_bg: color!(0x0000aa),
                sidebar_text: color!(0x55ffff),
                sidebar_active: color!(0x00aaaa),
                header_bg: color!(0x00aaaa),
                header_text: color!(0x000000),
                composer_bg: color!(0x000000),
                mono: true,
                prompt: "C:\\CHATTY>",
                nicks: NICKS_DOS,
            },
            _ => base,
        }
    }

    fn theme(&self) -> Theme {
        Theme::custom(
            self.name,
            iced::theme::Palette {
                background: self.bg,
                text: self.text,
                primary: self.accent,
                success: color!(0x06d6a0),
                warning: color!(0xffbc1f),
                danger: self.error,
            },
        )
    }

    fn font(&self, bold: bool) -> Font {
        let base = if self.mono {
            Font::MONOSPACE
        } else {
            Font::DEFAULT
        };
        Font {
            weight: if bold { Weight::Bold } else { Weight::Normal },
            ..base
        }
    }

    fn nick(&self, name: &str) -> Color {
        let h = name
            .bytes()
            .fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize));
        self.nicks[h % self.nicks.len()]
    }

    fn item_style(&self, active: bool) -> button::Style {
        button::Style {
            background: active.then_some(self.sidebar_active.into()),
            text_color: self.sidebar_text,
            ..button::Style::default()
        }
    }
}
