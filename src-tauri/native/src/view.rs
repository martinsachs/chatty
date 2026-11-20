//! Ansicht: Seitenleiste, Verlauf, rechte Leiste (Thread/Suche), Einstellungen.

use chatty_core::app::THEMES;
use chatty_core::model::{Message as Msg, SpaceKind};
use iced::widget::{
    Column, button, checkbox, column, container, hover, image, markdown, pick_list, right, row,
    scrollable, space, text, text_input,
};
use iced::{Alignment, Element, Length};

use crate::look::Look;
use crate::text::{initials, size};
use crate::{COMPOSER, FormField, MESSAGES, Message, QUICK_EMOJI, Rhs, State};

pub fn view(s: &State) -> Element<'_, Message> {
    if s.settings.is_some() {
        return settings(s);
    }
    let mut main = row![sidebar(s), center(s)].height(Length::Fill);
    if let Some(rhs) = &s.rhs {
        main = main.push(right_panel(s, rhs));
    }
    main.into()
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
    col = col.push(space().height(6));
    for sp in &s.spaces {
        let icon = match sp.kind {
            SpaceKind::Channel => "#",
            SpaceKind::Direct => "●",
            SpaceKind::Group => "◆",
        };
        let fav = if sp.favorite { " ★" } else { "" };
        let muted = if sp.muted { " 🔕" } else { "" };
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
        let active = s.space.as_ref() == Some(&sp.id);
        let item = button(
            text(format!("{icon} {}{status}{fav}{muted}{badge}", sp.name))
                .font(look.font(bold))
                .color(look.sidebar_text)
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
                small(look, if sp.muted { "🔔" } else { "🔕" }).on_press(Message::Flag(
                    sp.id.clone(),
                    None,
                    Some(!sp.muted)
                )),
            ]
            .spacing(2),
        );
        col = col.push(hover(item, flags));
    }
    container(scrollable(col).height(Length::Fill))
        .width(260)
        .height(Length::Fill)
        .style(move |_| container::background(look.sidebar_bg))
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

fn center(s: &State) -> Element<'_, Message> {
    let look = &s.look;
    let Some(space) = s.current_space() else {
        let hint = if s.info.as_ref().is_some_and(|i| i.accounts.is_empty()) {
            "Noch kein Konto – links „Konten und Einstellungen“."
        } else {
            "Kanal links auswählen."
        };
        return container(text(hint).font(look.font(false)))
            .center(Length::Fill)
            .into();
    };
    let header = container(
        row![
            text(space.name.clone())
                .font(look.font(true))
                .size(16)
                .color(look.header_text)
                .width(Length::Fill),
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
    if s.cursor.is_none() && !s.loading && !s.messages.is_empty() {
        list = list.push(text(format!("Anfang von {}", space.name)).color(look.muted));
    } else if s.loading {
        list = list.push(text("Lade …").color(look.muted));
    }
    let mut prev: Option<&Msg> = None;
    for m in &s.messages {
        // Folgenachricht desselben Absenders innerhalb von 5 Minuten: ohne Kopf und Bild.
        let compact =
            prev.is_some_and(|p| p.sender.id == m.sender.id && m.created - p.created < 5 * 60_000);
        list = list.push(message(s, m, false, compact));
        prev = Some(m);
    }
    let messages = scrollable(list)
        .id(MESSAGES)
        .anchor_bottom()
        .on_scroll(Message::Scrolled)
        .height(Length::Fill)
        .width(Length::Fill);

    let mut col = column![header];
    if let Some(r) = &s.update_available {
        col = col.push(
            container(
                row![
                    text(format!("Neue Version {} verfügbar.", r.version)).width(Length::Fill),
                    button(text("Installieren und neu starten")).on_press(Message::InstallUpdate),
                ]
                .align_y(Alignment::Center),
            )
            .padding([4, 16]),
        );
    }
    col = col.push(messages);
    if let Some(e) = &s.error {
        col = col.push(container(text(e.clone()).color(look.error)).padding([4, 16]));
    }
    col.push(composer(s, &space.name)).into()
}

fn composer<'a>(s: &'a State, space: &str) -> Element<'a, Message> {
    let look = &s.look;
    let mut col = column![].spacing(4);
    if !s.files.is_empty() {
        let mut chips = row![].spacing(6);
        for (i, f) in s.files.iter().enumerate() {
            let name = f
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            chips = chips.push(small(look, "×").on_press(Message::RemoveFile(i)));
            chips = chips.push(text(name).size(13));
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
    let mut input = text_input(&format!("Nachricht an {space}"), &s.draft)
        .id(COMPOSER)
        .font(look.font(false))
        .padding(10);
    if s.online() {
        input = input.on_input(Message::Draft).on_submit(Message::Send);
    }
    let prompt = text(look.prompt).font(look.font(false));
    col = col.push(
        row![
            prompt,
            input,
            small(look, "📎").on_press(Message::PickFiles),
        ]
        .spacing(6)
        .align_y(Alignment::Center),
    );
    container(col)
        .padding(8)
        .style(move |_| container::background(look.composer_bg))
        .into()
}

/// Eine Nachricht: Profilbild (je Optik), Kopf, Text, Anhänge, Reaktionen, Antworten; Aktionen beim Überfahren.
fn message<'a>(s: &'a State, m: &'a Msg, in_thread: bool, compact: bool) -> Element<'a, Message> {
    let look = &s.look;
    let time = chrono::DateTime::from_timestamp_millis(m.created)
        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default();
    let body: Element<'a, Message> = match s.md.get(&m.id) {
        Some(items) => markdown::view(items, look.markdown()).map(Message::Link),
        None => text(&m.text).font(look.font(false)).into(),
    };
    let body: Element<'a, Message> = if m.mentions_me && !m.mine {
        container(body)
            .style(move |_| container::Style {
                border: iced::Border {
                    color: look.mention,
                    width: 0.0,
                    radius: 0.0.into(),
                },
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
                        image(h.clone()).width(18).height(18),
                        text(r.count.to_string()).size(13)
                    ]
                    .spacing(4)
                    .into(),
                    None => text(format!("{} {}", r.emoji, r.count)).size(13).into(),
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
    if m.mine {
        actions = actions.push(small(look, "✎").on_press(Message::Edit(m.clone())));
        actions = actions.push(small(look, "🗑").on_press(Message::Delete(m.clone())));
    }
    hover(
        container(line).padding([2, 0]).width(Length::Fill),
        right(actions),
    )
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

fn right_panel<'a>(s: &'a State, rhs: &'a Rhs) -> Element<'a, Message> {
    let look = &s.look;
    let (title, body): (String, Element<'a, Message>) = match rhs {
        Rhs::Thread {
            root,
            replies,
            draft,
        } => {
            let mut list = column![message(s, root, true, false)].spacing(4).padding(8);
            for r in replies {
                list = list.push(message(s, r, true, false));
            }
            let input = text_input("Antworten …", draft)
                .on_input(Message::ThreadDraft)
                .on_submit(Message::ThreadSend)
                .padding(8);
            (
                "Thread".into(),
                column![
                    scrollable(list).height(Length::Fill),
                    container(input).padding(8)
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
                                    text(format!("{where_} · {}", m.sender.name))
                                        .size(12)
                                        .color(look.muted),
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
                chatty_core::model::BackendKind::Google => "Google Chat",
                chatty_core::model::BackendKind::Mattermost => "Mattermost",
            };
            col = col.push(
                row![
                    text(format!(
                        "{} ({kind}{})",
                        a.account.label,
                        if a.online { "" } else { ", getrennt" }
                    ))
                    .width(Length::Fill),
                    button(text("Entfernen"))
                        .on_press(Message::RemoveAccount(a.account.id.clone())),
                ]
                .align_y(Alignment::Center),
            );
        }
    }

    // Mattermost
    let can_add = !f.busy && !f.server.is_empty() && !f.login.is_empty() && !f.password.is_empty();
    col = col.push(text("Mattermost-Konto hinzufügen").font(look.font(true)));
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
                .on_press_maybe((configured && !f.busy).then_some(Message::AddGoogle)),
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
    let notifications = s.info.as_ref().is_none_or(|i| i.notifications);
    col = col.push(
        checkbox(notifications)
            .label("Desktop-Benachrichtigungen bei Direktnachrichten und Erwähnungen")
            .on_toggle(Message::Notifications),
    );
    if let Some(m) = &f.message {
        col = col.push(text(m.clone()).color(look.muted));
    }
    container(scrollable(col))
        .center_x(Length::Fill)
        .height(Length::Fill)
        .into()
}
