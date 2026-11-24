//! Ansicht: Seitenleiste, Verlauf, rechte Leiste (Thread/Suche), Einstellungen, Hinweise.
//! Vorbild ist die Tauri-Oberfläche (`src/lib/components/`), Abweichungen in `konzept/drift-native-tauri.md`.

use chatty_core::app::THEMES;
use chatty_core::backend::is_preview_image;
use chatty_core::model::{BackendKind, Message as Msg, Space, SpaceKind};
use iced::widget::text_editor::{Binding, KeyPress};
use iced::widget::{
    Column, bottom_right, button, checkbox, column, container, hover, image, markdown, mouse_area,
    pick_list, right, row, scrollable, space, stack, text, text_editor, text_input,
};
use iced::{Alignment, Color, Element, Length, keyboard, mouse};

use crate::look::Look;
use crate::prefs::Sort;
use crate::text::{date, day, initials, size, time};
use crate::{
    COMPOSER, DM_MAX_AGE_MS, FormField, MESSAGES, Message, QUICK_EMOJI, Rhs, State, ToastKind,
};

pub fn view(s: &State) -> Element<'_, Message> {
    let main: Element<'_, Message> = if s.settings.is_some() {
        settings(s)
    } else {
        let mut main = row![sidebar(s), resize_handle(s), center(s)].height(Length::Fill);
        if let Some(rhs) = &s.rhs {
            main = main.push(right_panel(s, rhs));
        }
        main.into()
    };
    if s.toasts.is_empty() {
        return main;
    }
    stack![main, toasts(s)].into()
}

// ---------- Seitenleiste ----------

/// Bereiche wie Tauri: Favoriten, Kanäle, Direktnachrichten; Filter, Ausblendung alter DMs, Sortierung.
fn sections(s: &State) -> [(&'static str, Vec<&Space>); 3] {
    let f = s.filter.trim().to_lowercase();
    let now = chrono::Utc::now().timestamp_millis();
    let visible = |sp: &Space| {
        if !f.is_empty() {
            return sp.name.to_lowercase().contains(&f);
        }
        sp.kind == SpaceKind::Channel
            || sp.favorite
            || sp.unread
            || Some(&sp.id) == s.space.as_ref()
            || sp.last_active >= now - DM_MAX_AGE_MS
    };
    let mut out = [
        ("Favoriten", Vec::new()),
        ("Kanäle", Vec::new()),
        ("Direktnachrichten", Vec::new()),
    ];
    for sp in s.spaces.iter().filter(|sp| visible(sp)) {
        let i = if sp.favorite {
            0
        } else if sp.kind == SpaceKind::Channel {
            1
        } else {
            2
        };
        out[i].1.push(sp);
    }
    let by_name = |a: &&Space, b: &&Space| a.name.to_lowercase().cmp(&b.name.to_lowercase());
    for (_, list) in &mut out {
        match s.prefs.sort {
            Sort::Recent => {
                list.sort_by(|a, b| b.last_active.cmp(&a.last_active).then(by_name(a, b)))
            }
            Sort::Count => list.sort_by(|a, b| b.msg_count.cmp(&a.msg_count).then(by_name(a, b))),
            Sort::Name => list.sort_by(by_name),
        }
    }
    out
}

fn sidebar(s: &State) -> Element<'_, Message> {
    let look = &s.look;
    let mut col = column![].spacing(2).padding(8);

    // Konten mit Erwähnungszähler und Verbindungszustand.
    if let Some(info) = &s.info {
        for a in &info.accounts {
            let total = s.mention_total(&a.account.id);
            let mut label = a.account.label.clone();
            if !a.online {
                label.push_str(" · getrennt");
            }
            if total > 0 {
                label.push_str(&format!(" ({total})"));
            }
            let active = s.account.as_ref() == Some(&a.account.id);
            col = col.push(
                button(
                    text(label)
                        .font(look.font(true))
                        .color(look.sidebar_text)
                        .wrapping(text::Wrapping::None),
                )
                .on_press(Message::SelectAccount(a.account.id.clone()))
                .style(move |_, _| look.item_style(active))
                .width(Length::Fill),
            );
        }
    }
    col = col.push(
        button(
            text("+ Konten und Einstellungen")
                .color(look.sidebar_text)
                .size(13),
        )
        .on_press(Message::Settings(true))
        .style(move |_, _| look.item_style(false))
        .padding([3, 8]),
    );

    // Filter und Sortierung.
    let filter_hint = if s.current_kind() == Some(BackendKind::Mattermost) {
        "Kanäle filtern / suchen"
    } else {
        "Kanäle filtern"
    };
    col = col.push(
        row![
            text_input(filter_hint, &s.filter)
                .on_input(Message::Filter)
                .padding(4)
                .size(13)
                .width(Length::Fill),
            pick_list(&Sort::ALL[..], Some(s.prefs.sort), Message::Sort)
                .text_size(13)
                .padding(4),
        ]
        .spacing(4)
        .padding([6, 0]),
    );

    for (title, items) in sections(s) {
        if items.is_empty() {
            continue;
        }
        col = col.push(section_title(look, title));
        for sp in items {
            col = col.push(space_item(s, sp));
        }
    }
    if !s.joinable.is_empty() {
        col = col.push(section_title(look, "Weitere Kanäle"));
        for sp in &s.joinable {
            col = col.push(
                button(
                    row![
                        text(format!("# {}", sp.name))
                            .color(look.sidebar_text)
                            .wrapping(text::Wrapping::None)
                            .width(Length::Fill),
                        text("Beitreten").size(12).color(look.sidebar_text),
                    ]
                    .spacing(6),
                )
                .on_press(Message::Join(sp.id.clone()))
                .style(move |_, _| look.item_style(false))
                .padding([3, 8])
                .width(Length::Fill),
            );
        }
    }
    container(scrollable(col).height(Length::Fill))
        .width(s.prefs.sidebar_width)
        .height(Length::Fill)
        .style(move |_| container::background(look.sidebar_bg))
        .into()
}

fn section_title<'a>(look: &'a Look, title: &str) -> Element<'a, Message> {
    container(
        text(title.to_uppercase())
            .size(12)
            .font(look.font(true))
            .color(look.sidebar_text),
    )
    .padding([6, 8])
    .into()
}

fn space_item<'a>(s: &'a State, sp: &'a Space) -> Element<'a, Message> {
    let look = &s.look;
    let icon = match sp.kind {
        SpaceKind::Channel => "#",
        SpaceKind::Direct => "●",
        SpaceKind::Group => "◆",
    };
    let status = sp
        .status_emoji
        .as_deref()
        .filter(|e| !e.starts_with(':'))
        .map(|e| format!(" {e}"))
        .unwrap_or_default();
    let badge = if sp.mention_count > 0 {
        format!(" ({})", sp.mention_count)
    } else {
        String::new()
    };
    let bold = sp.unread && !sp.muted || sp.mention_count > 0;
    let color = if sp.muted {
        iced::Color {
            a: 0.5,
            ..look.sidebar_text
        }
    } else {
        look.sidebar_text
    };
    // Retro-Optiken: „*“ vor ungelesenen Kanälen und eigene Farbe (wie Tauri `retro.css`).
    let (mark, color) = match look.unread {
        Some(c) if bold => ("* ", c),
        Some(_) => ("  ", color),
        None => ("", color),
    };
    let active = s.space.as_ref() == Some(&sp.id);
    let item = button(
        text(format!("{mark}{icon} {}{status}{badge}", sp.name))
            .font(look.font(bold))
            .color(color)
            .wrapping(text::Wrapping::None),
    )
    .on_press(Message::OpenSpace(sp.id.clone()))
    .style(move |_, _| look.item_style(active))
    .padding([3, 8])
    .width(Length::Fill);
    let flags = right(
        row![
            small(look, if sp.favorite { "★" } else { "☆" }).on_press(Message::Flag(
                sp.id.clone(),
                Some(!sp.favorite),
                None
            )),
            small(look, if sp.muted { "🔕" } else { "🔔" }).on_press(Message::Flag(
                sp.id.clone(),
                None,
                Some(!sp.muted)
            )),
        ]
        .spacing(2),
    );
    hover(item, flags)
}

/// Griff am rechten Rand der Seitenleiste: ziehen ändert die Breite, Doppelklick setzt sie zurück.
fn resize_handle(s: &State) -> Element<'_, Message> {
    let look = &s.look;
    let color = if s.resizing { look.accent } else { look.border };
    mouse_area(
        container(space())
            .width(4)
            .height(Length::Fill)
            .style(move |_| container::background(color)),
    )
    .on_press(Message::ResizeStart)
    .on_double_click(Message::ResizeReset)
    .interaction(mouse::Interaction::ResizingHorizontally)
    .into()
}

/// Kleiner Knopf für Aktionen (Seitenleiste, Nachrichten).
fn small<'a>(look: &'a Look, label: &'a str) -> button::Button<'a, Message> {
    button(text(label).size(13).color(look.text))
        .padding([1, 6])
        .style(move |_, _| button::Style {
            background: Some(look.bg.into()),
            text_color: look.text,
            border: iced::Border {
                color: look.border,
                width: 1.0,
                radius: 4.0.into(),
            },
            ..button::Style::default()
        })
}

// ---------- Kanal ----------

fn center(s: &State) -> Element<'_, Message> {
    let look = &s.look;
    let Some(space) = s.current_space() else {
        let hint = if s.info.as_ref().is_some_and(|i| i.accounts.is_empty()) {
            "Zuerst ein Konto hinzufügen."
        } else {
            "Kanal oder Direktnachricht links auswählen."
        };
        return container(text(hint).font(look.font(false)).color(look.muted))
            .center(Length::Fill)
            .into();
    };
    let header = container(
        row![
            topic_header(s, &space.name),
            text_input("Suchen", &s.search)
                .on_input(Message::SearchInput)
                .on_submit(Message::Search)
                .padding(4)
                .width(220),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    )
    .padding([6, 16])
    .width(Length::Fill)
    .style(move |_| container::background(look.header_bg).color(look.header_text));

    let mut list = column![].spacing(4).padding([8, 16]);
    if s.loading && s.messages.is_empty() {
        list = list.push(text("Lade …").color(look.muted));
    } else if s.cursor.is_none() && !s.loading && !s.messages.is_empty() {
        list = list.push(text(format!("Anfang von {}", space.name)).color(look.muted));
    }
    let mut prev: Option<&Msg> = None;
    for m in &s.messages {
        let new_day = prev.is_none_or(|p| date(p.created) != date(m.created));
        if new_day {
            list = list.push(day_separator(look, m.created));
        }
        // Folgenachricht desselben Absenders, gleicher Tag, < 5 min: ohne Kopf und Bild (wie Tauri).
        let compact = !new_day
            && prev.is_some_and(|p| {
                p.sender.id == m.sender.id
                    && p.is_reply == m.is_reply
                    && m.created - p.created < 5 * 60_000
            });
        list = list.push(message(s, m, false, compact));
        prev = Some(m);
    }
    let messages = scrollable(list)
        .id(MESSAGES)
        .anchor_bottom()
        .on_scroll(Message::Scrolled)
        .height(Length::Fill)
        .width(Length::Fill);
    let messages: Element<'_, Message> = if s.show_down {
        let down = button(text("↓").size(18).color(iced::Color::WHITE))
            .on_press(Message::ToBottom)
            .padding([4, 12])
            .style(move |_, _| button::Style {
                background: Some(look.accent.into()),
                text_color: iced::Color::WHITE,
                border: iced::Border {
                    radius: 18.0.into(),
                    ..iced::Border::default()
                },
                ..button::Style::default()
            });
        stack![messages, bottom_right(container(down).padding(16))].into()
    } else {
        messages.into()
    };

    let mut col = column![header];
    if let Some(r) = &s.update_available {
        col = col.push(
            container(
                row![
                    text(format!("Version {} verfügbar.", r.version)).width(Length::Fill),
                    button(text("Installieren")).on_press(Message::InstallUpdate),
                ]
                .align_y(Alignment::Center),
            )
            .padding([4, 16]),
        );
    }
    col.push(messages).push(composer(s, &space.name)).into()
}

fn day_separator(look: &Look, ms: i64) -> Element<'_, Message> {
    let line = || {
        container(space())
            .height(1)
            .width(Length::Fill)
            .style(move |_| container::background(look.border))
    };
    row![
        line(),
        text(day(ms))
            .size(12)
            .font(look.font(true))
            .color(look.text),
        line()
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .padding([8, 0])
    .into()
}

/// Mehrzeilige Eingabe wie Tauri: Enter sendet, Shift+Enter neue Zeile, ↑ im leeren Feld bearbeitet die
/// letzte eigene Nachricht, Esc bricht das Bearbeiten ab.
fn composer<'a>(s: &'a State, space: &str) -> Element<'a, Message> {
    let look = &s.look;
    let mut col = column![].spacing(4);
    if !s.files.is_empty() {
        let mut chips = row![].spacing(10);
        for (i, f) in s.files.iter().enumerate() {
            let name = f
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            chips = chips.push(
                row![
                    text(name).size(13),
                    small(look, "×").on_press(Message::RemoveFile(i))
                ]
                .spacing(4)
                .align_y(Alignment::Center),
            );
        }
        col = col.push(chips);
    }
    if s.editing.is_some() {
        col = col.push(
            text("Nachricht bearbeiten · Enter speichert · Esc bricht ab")
                .size(12)
                .color(look.muted),
        );
    }
    let empty = s.composer.text().trim().is_empty();
    let mut editor = text_editor(&s.composer)
        .id(COMPOSER)
        .placeholder(format!("Nachricht an {space}"))
        .font(look.font(false))
        .padding(10)
        .max_height(240.0);
    if s.online() {
        editor = editor
            .on_action(Message::Compose)
            .key_binding(move |kp| chat_keys(kp, empty, Message::Send, Some(Message::EditLast)));
    }
    let prompt = text(look.prompt).font(look.font(false));
    col = col.push(row![prompt, editor].spacing(6).align_y(Alignment::Center));
    let can_send = s.online() && !s.sending && (!empty || !s.files.is_empty());
    col = col.push(
        row![
            small(look, "📎").on_press(Message::PickFiles),
            text("Enter senden · Shift+Enter neue Zeile · ↑ letzte bearbeiten")
                .size(11)
                .color(look.muted)
                .width(Length::Fill),
            button(text(if s.editing.is_some() {
                "Speichern"
            } else {
                "Senden"
            }))
            .on_press_maybe(can_send.then_some(Message::Send))
            .padding([4, 12]),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    container(col)
        .padding(8)
        .style(move |_| container::background(look.composer_bg))
        .into()
}

/// Tastenbelegung der Eingabefelder.
fn chat_keys(
    kp: KeyPress,
    empty: bool,
    send: Message,
    edit_last: Option<Message>,
) -> Option<Binding<Message>> {
    match &kp.key {
        keyboard::Key::Named(keyboard::key::Named::Enter) if !kp.modifiers.shift() => {
            Some(Binding::Custom(send))
        }
        keyboard::Key::Named(keyboard::key::Named::ArrowUp) if empty && edit_last.is_some() => {
            edit_last.map(Binding::Custom)
        }
        _ => Binding::from_key_press(kp),
    }
}

/// Kanalname, darunter die erste Zeile des Kanal-Headers (Mattermost) mit klickbaren Links.
fn topic_header<'a>(s: &'a State, name: &str) -> Element<'a, Message> {
    let look = &s.look;
    let name = text(name.to_string())
        .font(look.font(true))
        .size(16)
        .color(look.header_text);
    if s.topic().is_empty() {
        return container(name).width(Length::Fill).into();
    }
    let mut style = look.markdown();
    style.text_size = 12.into();
    // Link wie im Verlauf, außer er geht im Kopf unter (mIRC: Blau auf Blau).
    let lum = |c: Color| 0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b;
    if (lum(look.link) - lum(look.header_bg)).abs() < 0.3 {
        style.style.link_color = look.header_text;
    }
    let topic = markdown::view(s.topic(), style).map(Message::Link);
    let fg = look.header_text;
    column![
        name,
        container(topic)
            .max_height(18)
            .clip(true)
            .style(move |_| container::Style::default().color(fg)),
    ]
    .width(Length::Fill)
    .into()
}

/// Eine Nachricht: Profilbild (je Optik), Kopf, Text, Anhänge, Reaktionen, Antworten; Aktionen beim Überfahren.
fn message<'a>(s: &'a State, m: &'a Msg, in_thread: bool, compact: bool) -> Element<'a, Message> {
    let look = &s.look;
    let time = time(m.created);
    let body: Element<'a, Message> = match s.md.get(&m.id) {
        // Nur Emojis: groß wie bei Mattermost/Slack.
        _ if crate::text::emoji_only(&m.text) => text(m.text.trim()).size(34).into(),
        Some(items) => markdown::view_with(items, look.markdown(), &EmojiViewer),
        None => text(&m.text).font(look.font(false)).into(),
    };
    let body: Element<'a, Message> = if m.mentions_me && !m.mine {
        container(body)
            .style(move |_| container::Style {
                background: Some(iced::Background::Color(iced::Color {
                    a: 0.12,
                    ..look.mention
                })),
                ..container::Style::default()
            })
            .into()
    } else {
        body
    };

    let mut content = Column::new().spacing(2).width(Length::Fill);
    let edited = if m.edited.is_some() {
        " (bearbeitet)"
    } else {
        ""
    };
    if look.avatars && compact {
        content = content.push(body);
        if m.edited.is_some() {
            content = content.push(text(edited).size(12).color(look.muted));
        }
    } else if look.avatars {
        content = content.push(
            row![
                text(m.sender.name.clone())
                    .font(look.font(true))
                    .color(look.text),
                text(format!("{time}{edited}")).size(12).color(look.muted),
            ]
            .spacing(8)
            .align_y(Alignment::End),
        );
        content = content.push(body);
    } else {
        content = content.push(
            row![
                text(format!("[{time}]"))
                    .font(look.font(false))
                    .color(look.muted),
                text(format!("<{}>", m.sender.name))
                    .font(look.font(true))
                    .color(look.nick(&m.sender.name)),
                container(body).width(Length::Fill),
                text(edited).size(12).color(look.muted),
            ]
            .spacing(8),
        );
    }
    if let Some(acc) = &s.account {
        let mut previews = row![].spacing(6);
        let mut any = false;
        for a in m.attachments.iter().filter(|a| is_preview_image(a)) {
            if let Some(h) = s.image(&crate::preview_path(acc, a)) {
                any = true;
                previews = previews.push(
                    button(
                        container(image(h.clone()).content_fit(iced::ContentFit::ScaleDown))
                            .max_width(400)
                            .max_height(300),
                    )
                    .on_press(Message::Save(a.clone()))
                    .padding(0)
                    .style(|_, _| button::Style::default()),
                );
            }
        }
        if any {
            content = content.push(previews);
        }
    }
    if !m.attachments.is_empty() {
        let mut files = row![].spacing(6);
        for a in &m.attachments {
            files = files.push(
                button(text(format!("📄 {} {}", a.name, size(a.size))).size(13))
                    .on_press(Message::Save(a.clone()))
                    .padding([2, 8])
                    .style(move |_, _| button::Style {
                        background: Some(look.code_bg.into()),
                        text_color: look.text,
                        ..button::Style::default()
                    }),
            );
        }
        content = content.push(files);
    }
    if !m.reactions.is_empty() {
        let mut rs = row![].spacing(4);
        for r in &m.reactions {
            let label: Element<'a, Message> =
                match s.custom_emoji(&r.emoji).and_then(|p| s.image(&p)) {
                    Some(h) => row![
                        image(h.clone()).width(22).height(22),
                        text(r.count.to_string()).size(13)
                    ]
                    .spacing(4)
                    .align_y(Alignment::Center)
                    .into(),
                    None => row![text(&r.emoji).size(18), text(r.count.to_string()).size(13)]
                        .spacing(4)
                        .align_y(Alignment::Center)
                        .into(),
                };
            let me = r.me;
            rs = rs.push(
                button(label)
                    .on_press(Message::React(m.clone(), r.emoji.clone()))
                    .padding([1, 8])
                    .style(move |_, _| button::Style {
                        background: Some(look.bg.into()),
                        text_color: look.text,
                        border: iced::Border {
                            color: if me { look.link } else { look.border },
                            width: 1.0,
                            radius: 10.0.into(),
                        },
                        ..button::Style::default()
                    }),
            );
        }
        content = content.push(rs);
    }
    if !in_thread && m.reply_count > 0 {
        let label = if m.reply_count == 1 {
            "1 Antwort".to_string()
        } else {
            format!("{} Antworten", m.reply_count)
        };
        content = content.push(
            button(text(label).size(13).color(look.link))
                .on_press(Message::OpenThread(m.clone()))
                .padding(0)
                .style(button::text),
        );
    }
    if s.picker.as_ref() == Some(&m.id) {
        let mut picker = row![].spacing(4);
        for e in QUICK_EMOJI {
            picker = picker.push(small(look, e).on_press(Message::React(m.clone(), e.to_string())));
        }
        content = content.push(picker);
    }

    let line: Element<'a, Message> = if look.avatars && compact {
        row![space().width(36), content].spacing(10).into()
    } else if look.avatars {
        row![avatar(s, m), content].spacing(10).into()
    } else {
        content.into()
    };
    // Thread-Antwort im Hauptverlauf (Mattermost): Balken links, wie Tauri `.msg.reply`.
    let line: Element<'a, Message> = if m.is_reply && !in_thread {
        let bar = container(space())
            .width(3)
            .height(Length::Fill)
            .style(move |_| container::background(look.link));
        row![bar, line].spacing(8).into()
    } else {
        line
    };

    let mut actions =
        row![
            small(look, "☺").on_press(Message::Picker(if s.picker.as_ref() == Some(&m.id) {
                None
            } else {
                Some(m.id.clone())
            }))
        ]
        .spacing(2);
    if !in_thread && m.thread_id.is_some() {
        actions = actions.push(small(look, "↩").on_press(Message::OpenThread(m.clone())));
    }
    let confirming = s.confirm_delete.as_ref() == Some(&m.id);
    if m.mine {
        actions = actions.push(small(look, "✎").on_press(Message::Edit(m.clone())));
        // Löschen mit Rückfrage (wie Tauri `confirm`): erster Klick fragt, zweiter löscht.
        actions = if confirming {
            actions
                .push(small(look, "Wirklich löschen?").on_press(Message::Delete(m.clone())))
                .push(small(look, "Abbrechen").on_press(Message::ConfirmDelete(None)))
        } else {
            actions.push(small(look, "🗑").on_press(Message::ConfirmDelete(Some(m.id.clone()))))
        };
    }
    let mut top = row![].align_y(Alignment::Start);
    if compact && look.avatars {
        // Uhrzeit beim Überfahren, wie Tauri `hover-time`.
        top = top.push(text(time).size(10).color(look.muted));
    }
    top = top.push(space().width(Length::Fill)).push(actions);
    let base = container(line).padding([2, 0]).width(Length::Fill);
    // Offene Rückfrage bleibt auch ohne Überfahren sichtbar.
    if confirming {
        return stack![base, top].into();
    }
    hover(base, top)
}

fn avatar<'a>(s: &'a State, m: &'a Msg) -> Element<'a, Message> {
    let look = &s.look;
    let path = s.account.as_ref().map(|a| format!("{a}/{}", m.sender.id));
    match path.as_deref().and_then(|p| s.image(p)) {
        Some(h) => image(h.clone()).width(36).height(36).into(),
        None => {
            let bg = look.nick(&m.sender.name);
            container(
                text(initials(&m.sender.name))
                    .size(13)
                    .color(iced::Color::WHITE),
            )
            .center(36)
            .style(move |_| container::Style {
                background: Some(bg.into()),
                border: iced::Border {
                    radius: 18.0.into(),
                    ..iced::Border::default()
                },
                ..container::Style::default()
            })
            .into()
        }
    }
}

// ---------- rechte Leiste ----------

fn right_panel<'a>(s: &'a State, rhs: &'a Rhs) -> Element<'a, Message> {
    let look = &s.look;
    let (title, body): (String, Element<'a, Message>) = match rhs {
        Rhs::Thread { root, replies } => {
            let mut list = column![message(s, root, true, false)].spacing(4).padding(8);
            for r in replies {
                list = list.push(message(s, r, true, false));
            }
            let empty = s.thread_composer.text().trim().is_empty();
            let mut editor = text_editor(&s.thread_composer)
                .placeholder("Antworten …")
                .padding(8)
                .max_height(160.0);
            if s.online() {
                editor = editor
                    .on_action(Message::ThreadCompose)
                    .key_binding(move |kp| chat_keys(kp, empty, Message::ThreadSend, None));
            }
            (
                "Thread".into(),
                column![
                    scrollable(list).height(Length::Fill),
                    container(editor).padding(8)
                ]
                .into(),
            )
        }
        Rhs::Search { query, results } => {
            let mut list = column![].spacing(6).padding(8);
            match results {
                None => list = list.push(text("Suche läuft …").color(look.muted)),
                Some(r) if r.is_empty() => {
                    list = list.push(text("Keine Treffer.").color(look.muted))
                }
                Some(r) => {
                    list = list.push(
                        text(format!("{} Treffer", r.len()))
                            .size(12)
                            .color(look.muted),
                    );
                    for m in r {
                        let where_ = s
                            .spaces
                            .iter()
                            .find(|x| x.id == m.space_id)
                            .map(|x| x.name.clone())
                            .unwrap_or_default();
                        list = list.push(
                            button(
                                column![
                                    text(format!("{where_} · {}", day(m.created)))
                                        .size(12)
                                        .color(look.muted),
                                    text(m.sender.name.clone())
                                        .font(look.font(true))
                                        .color(look.text),
                                    text(m.text.chars().take(200).collect::<String>())
                                        .color(look.text),
                                ]
                                .spacing(2),
                            )
                            .on_press(Message::OpenSpace(m.space_id.clone()))
                            .style(button::text)
                            .width(Length::Fill),
                        );
                    }
                }
            }
            (
                format!("Suche: {query}"),
                scrollable(list).height(Length::Fill).into(),
            )
        }
    };
    let header = container(
        row![
            text(title)
                .font(look.font(true))
                .color(look.header_text)
                .width(Length::Fill),
            button(text("✕").color(look.header_text))
                .on_press(Message::CloseRhs)
                .style(button::text),
        ]
        .align_y(Alignment::Center),
    )
    .padding([6, 12])
    .style(move |_| container::background(look.header_bg));
    container(column![header, body])
        .width(360)
        .height(Length::Fill)
        .style(move |_| container::Style {
            border: iced::Border {
                color: look.border,
                width: 1.0,
                radius: 0.0.into(),
            },
            ..container::background(look.bg)
        })
        .into()
}

// ---------- Hinweise ----------

fn toasts(s: &State) -> Element<'_, Message> {
    let look = &s.look;
    let mut col = column![].spacing(6).width(360);
    for t in &s.toasts {
        let accent = match t.kind {
            ToastKind::Error => look.error,
            ToastKind::Message => look.mention,
            ToastKind::Info => look.accent,
        };
        let mut r = row![
            text(t.text.clone())
                .size(13)
                .color(iced::Color::WHITE)
                .width(Length::Fill)
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if let Some((label, action)) = &t.action {
            r = r.push(
                button(text(label.clone()).size(13).color(iced::Color::WHITE))
                    .on_press(Message::ToastAction(t.id, Box::new(action.clone())))
                    .style(button::text),
            );
        }
        r = r.push(
            button(text("✕").size(13).color(iced::Color::WHITE))
                .on_press(Message::CloseToast(t.id))
                .style(button::text),
        );
        col = col.push(
            container(r)
                .padding([8, 12])
                .width(Length::Fill)
                .style(move |_| container::Style {
                    background: Some(iced::color!(0x1f1f1f).into()),
                    border: iced::Border {
                        color: accent,
                        width: 2.0,
                        radius: 6.0.into(),
                    },
                    ..container::Style::default()
                }),
        );
    }
    bottom_right(container(col).padding(16)).into()
}

// ---------- Einstellungen ----------

fn settings(s: &State) -> Element<'_, Message> {
    let look = &s.look;
    let Some(f) = &s.settings else {
        return space().into();
    };
    let field = |placeholder: &str, value: &str, field: FormField, secure: bool| {
        let t = text_input(placeholder, value).padding(8).secure(secure);
        if f.busy {
            t
        } else {
            t.on_input(move |v| Message::FormInput(field, v))
        }
    };

    let mut col = column![
        row![
            text("Konten und Einstellungen")
                .size(20)
                .width(Length::Fill),
            button(text("Schließen")).on_press(Message::Settings(false)),
        ]
        .align_y(Alignment::Center),
    ]
    .spacing(12)
    .padding(24)
    .max_width(640);

    // Konten
    col = col.push(text("Konten").font(look.font(true)));
    if let Some(info) = &s.info {
        if info.accounts.is_empty() {
            col = col.push(text("Noch keine Konten.").color(look.muted));
        }
        for a in &info.accounts {
            let kind = match a.account.kind {
                BackendKind::Google => "Google Chat",
                BackendKind::Mattermost => "Mattermost",
            };
            let id = a.account.id.clone();
            let actions: Element<'_, Message> = if f.confirm_remove.as_ref() == Some(&id) {
                row![
                    text("Token und Cache löschen?").color(look.error),
                    button(text("Entfernen")).on_press(Message::RemoveAccount(id.clone())),
                    button(text("Abbrechen")).on_press(Message::ConfirmRemove(None)),
                ]
                .spacing(6)
                .align_y(Alignment::Center)
                .into()
            } else {
                row![
                    button(text("Neu anmelden"))
                        .on_press_maybe((!f.busy).then(|| Message::Relogin(id.clone()))),
                    button(text("Entfernen")).on_press(Message::ConfirmRemove(Some(id.clone()))),
                ]
                .spacing(6)
                .into()
            };
            col = col.push(
                row![
                    text(format!(
                        "{} {} ({kind})",
                        if a.online { "●" } else { "○" },
                        a.account.label
                    ))
                    .width(Length::Fill),
                    actions,
                ]
                .align_y(Alignment::Center),
            );
        }
    }

    // Mattermost
    let can_add = !f.busy
        && f.server.len() > "https://".len()
        && !f.login.is_empty()
        && !f.password.is_empty();
    col = col.push(
        text(if f.relogin.is_some() {
            "Mattermost-Konto neu anmelden"
        } else {
            "Mattermost-Konto hinzufügen"
        })
        .font(look.font(true)),
    );
    col = col.push(field(
        "Server-URL (https://…)",
        &f.server,
        FormField::Server,
        false,
    ));
    col = col.push(field(
        "Benutzername oder E-Mail",
        &f.login,
        FormField::Login,
        false,
    ));
    col = col.push(field("Passwort", &f.password, FormField::Password, true));
    col = col.push(field(
        "MFA-Code (falls aktiv)",
        &f.mfa,
        FormField::Mfa,
        false,
    ));
    col = col
        .push(button(text("Anmelden")).on_press_maybe(can_add.then_some(Message::AddMattermost)));

    // Google
    col = col.push(text("Google Chat").font(look.font(true)));
    col = col.push(field(
        "OAuth-Client-ID",
        &f.client_id,
        FormField::ClientId,
        false,
    ));
    col = col.push(field(
        "Client-Secret (leer = unverändert)",
        &f.client_secret,
        FormField::ClientSecret,
        true,
    ));
    let configured = s.info.as_ref().is_some_and(|i| i.google_configured);
    col = col.push(
        row![
            button(text("Client speichern"))
                .on_press_maybe((!f.busy).then_some(Message::SaveGoogleClient)),
            button(text("Mit Google anmelden"))
                .on_press_maybe((configured && !f.busy).then_some(Message::AddGoogle(None))),
        ]
        .spacing(8),
    );

    // Darstellung
    col = col.push(text("Darstellung").font(look.font(true)));
    let current = THEMES.iter().copied().find(|t| *t == look.name);
    col = col.push(
        row![
            text("Optik").width(120),
            pick_list(&THEMES[..], current, Message::Theme)
        ]
        .align_y(Alignment::Center),
    );
    col = col.push(
        row![
            text("Schriftgröße").width(120),
            pick_list(
                ZOOM_LABELS,
                ZOOM_LABELS.iter().copied().find(|z| z.0 == s.prefs.zoom),
                |z| Message::Zoom(z.0)
            ),
            text("auch Strg + Plus/Minus/0").size(12).color(look.muted),
        ]
        .spacing(8)
        .align_y(Alignment::Center),
    );
    let notifications = s.info.as_ref().is_none_or(|i| i.notifications);
    col = col.push(
        checkbox(notifications)
            .label("Desktop-Benachrichtigungen bei Direktnachrichten und Erwähnungen")
            .on_toggle(Message::Notifications),
    );
    if f.busy {
        col = col.push(
            text("Bitte warten … (Google: Anmeldung im Browser abschließen)").color(look.muted),
        );
    }
    if let Some(m) = &f.message {
        col = col.push(text(m.clone()).color(look.muted));
    }
    container(scrollable(col))
        .center_x(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// Schriftgröße für die Auswahlliste („125 %“).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Zoom(f32);

impl std::fmt::Display for Zoom {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} %", (self.0 * 100.0).round())
    }
}

const ZOOM_LABELS: &[Zoom] = &[
    Zoom(0.8),
    Zoom(0.9),
    Zoom(1.0),
    Zoom(1.1),
    Zoom(1.25),
    Zoom(1.5),
    Zoom(1.75),
    Zoom(2.0),
];

/// Farb-Emoji-Schrift des Systems. Ohne ausdrückliche Wahl nimmt cosmic-text z. B. die Schwarz-Weiß-Glyphen
/// von DejaVu Sans, weil die in der Fallback-Liste vor Noto Color Emoji steht.
const EMOJI_FONT: iced::Font = iced::Font::with_name(if cfg!(target_os = "windows") {
    "Segoe UI Emoji"
} else if cfg!(target_os = "macos") {
    "Apple Color Emoji"
} else {
    "Noto Color Emoji"
});

/// Markdown wie üblich, Emojis im Fließtext aber farbig und etwas größer als der Text.
struct EmojiViewer;

impl<'a> markdown::Viewer<'a, Message> for EmojiViewer {
    fn on_link_click(url: markdown::Uri) -> Message {
        Message::Link(url)
    }

    fn paragraph(
        &self,
        settings: markdown::Settings,
        text: &markdown::Text,
    ) -> Element<'a, Message> {
        let size = settings.text_size * 1.3;
        let mut spans = Vec::new();
        for span in text.spans(settings.style).iter() {
            for (part, emoji) in crate::text::emoji_runs(&span.text) {
                let mut s = span.clone();
                s.text = part.to_string().into();
                if emoji {
                    s.font = Some(EMOJI_FONT);
                    s.size = Some(size);
                }
                spans.push(s);
            }
        }
        iced::widget::rich_text(spans)
            .size(settings.text_size)
            .on_link_click(Self::on_link_click)
            .into()
    }
}
