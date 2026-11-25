//! Optiken: Farben und Schrift je Name aus `chatty.toml` (wie die Tauri-Variante).

use iced::font::Weight;
use iced::widget::{button, markdown};
use iced::{Color, Font, Theme, color};

/// Farben und Schrift je Optik; Namen wie in `chatty.toml` (`theme`).
pub struct Look {
    pub name: &'static str,
    pub bg: Color,
    pub text: Color,
    pub muted: Color,
    pub accent: Color,
    pub mention: Color,
    pub error: Color,
    pub sidebar_bg: Color,
    pub sidebar_text: Color,
    pub sidebar_active: Color,
    pub header_bg: Color,
    pub header_text: Color,
    pub composer_bg: Color,
    pub mono: bool,
    pub prompt: &'static str,
    pub nicks: &'static [Color],
    pub link: Color,
    pub code_bg: Color,
    pub border: Color,
    /// Profilbilder neben den Nachrichten (Retro-Optiken: nein).
    pub avatars: bool,
    /// Retro-Optiken: ungelesene Kanäle mit „*“ und dieser Farbe statt fett (Festbreitenschrift).
    pub unread: Option<Color>,
}

pub const NICKS_LIGHT: &[Color] = &[
    color!(0x009300),
    color!(0xfc0000),
    color!(0x7f0000),
    color!(0x9c009c),
    color!(0xfc7f00),
    color!(0x00009c),
    color!(0x009393),
    color!(0x2a6f97),
];
pub const NICKS_DOS: &[Color] = &[color!(0xffff55)];

impl Look {
    pub fn from_name(name: &str) -> Look {
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
            link: color!(0x386fe5),
            code_bg: color!(0xf0f1f3),
            border: color!(0xdddee2),
            avatars: true,
            unread: None,
        };
        match name {
            "slack" => Look {
                name: "slack",
                text: color!(0x1d1c1d),
                accent: color!(0x007a5a),
                mention: color!(0x1264a3),
                link: color!(0x1264a3),
                sidebar_bg: color!(0x3f0e40),
                sidebar_active: color!(0x1164a3),
                header_text: color!(0x1d1c1d),
                ..base
            },
            // ehex.de im Dark Mode: Navy, Blau --color-highlight #5271ff, Grün --color-action #00bd81.
            "ehex" => Look {
                name: "ehex",
                bg: color!(0x00051a),
                text: color!(0xd0d2dd),
                muted: color!(0x989db3),
                accent: color!(0x00bd81),
                mention: color!(0x5271ff),
                error: color!(0xff6b6b),
                sidebar_bg: color!(0x13172a),
                sidebar_text: color!(0xd0d2dd),
                sidebar_active: color!(0x2b3a7a),
                header_bg: color!(0x00051a),
                header_text: color!(0xffffff),
                composer_bg: color!(0x00051a),
                link: color!(0x5271ff),
                code_bg: color!(0x13172a),
                border: color!(0x343a56),
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
                link: color!(0x0000fc),
                border: color!(0x808080),
                avatars: false,
                unread: Some(color!(0xfc0000)),
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
                link: color!(0xffffff),
                code_bg: color!(0x000080),
                border: color!(0x55ffff),
                avatars: false,
                unread: Some(color!(0xffff55)),
            },
            // Messenger um 2001: Windows-2000-Grau, grüner Akzent; Ton nur in der Tauri-Variante.
            "seequ" => Look {
                name: "seequ",
                text: color!(0x000000),
                muted: color!(0x6b6b6b),
                accent: color!(0x2e9a3a),
                mention: color!(0xd42020),
                error: color!(0xd42020),
                sidebar_bg: color!(0xffffff),
                sidebar_text: color!(0x000000),
                sidebar_active: color!(0xc8e6c0),
                header_bg: color!(0x0a246a),
                header_text: color!(0xffffff),
                composer_bg: color!(0xd4d0c8),
                link: color!(0x0000cc),
                code_bg: color!(0xf4f4f0),
                border: color!(0x808080),
                avatars: false,
                ..base
            },
            _ => base,
        }
    }

    pub fn theme(&self) -> Theme {
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

    pub fn font(&self, bold: bool) -> Font {
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

    pub fn nick(&self, name: &str) -> Color {
        let h = name
            .bytes()
            .fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize));
        self.nicks[h % self.nicks.len()]
    }

    pub fn item_style(&self, active: bool) -> button::Style {
        button::Style {
            background: active.then_some(self.sidebar_active.into()),
            text_color: self.sidebar_text,
            ..button::Style::default()
        }
    }
}

impl Look {
    /// Markdown-Darstellung passend zur Optik.
    pub fn markdown(&self) -> markdown::Settings {
        let mut style = markdown::Style::from_palette(self.theme().palette());
        style.font = self.font(false);
        style.link_color = self.link;
        style.inline_code_color = self.text;
        style.inline_code_highlight.background = self.code_bg.into();
        markdown::Settings::with_text_size(14, style)
    }
}
