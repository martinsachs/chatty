//! Zuordnung Mattermost-Emoji-Name ↔ Unicode (vollständige Liste aus `emoji_table.rs`) und Ersetzen von
//! Kurzcodes wie `:smile:` im Nachrichtentext.

use std::collections::HashMap;
use std::sync::OnceLock;

use super::emoji_table::TABLE;

fn by_name() -> &'static HashMap<&'static str, &'static str> {
    static MAP: OnceLock<HashMap<&str, &str>> = OnceLock::new();
    MAP.get_or_init(|| TABLE.iter().copied().collect())
}

/// Ohne Variationsselektor (U+FE0F) vergleichen, damit "❤" und "❤️" gleich sind.
fn strip_vs(s: &str) -> String {
    s.chars().filter(|c| *c != '\u{FE0F}').collect()
}

/// Mattermost-Name → Unicode; unbekannte Namen (eigene Server-Emojis) als ":name:".
pub fn to_unicode(name: &str) -> String {
    by_name()
        .get(name)
        .map_or_else(|| format!(":{name}:"), |e| (*e).to_string())
}

/// Unicode (oder ":name:") → Mattermost-Name.
pub fn to_name(emoji: &str) -> String {
    if let Some(name) = emoji.strip_prefix(':').and_then(|s| s.strip_suffix(':'))
        && !name.is_empty()
    {
        return name.to_string();
    }
    let wanted = strip_vs(emoji);
    TABLE
        .iter()
        .find(|(_, e)| strip_vs(e) == wanted)
        .map_or_else(|| emoji.to_string(), |(n, _)| (*n).to_string())
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '+' | '-')
}

/// Bekannte Kurzcodes (`:smile:`) durch Unicode ersetzen, außer in `Code` und ```Codeblöcken```.
/// Unbekannte (eigene Server-Emojis) bleiben stehen; die Oberfläche zeigt sie als Bild.
pub fn replace_shortcodes(text: &str) -> String {
    if !text.contains(':') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        // Code unverändert übernehmen.
        if let Some(after) = rest.strip_prefix("```") {
            let end = after.find("```").map_or(rest.len(), |i| i + 6);
            out.push_str(&rest[..end]);
            rest = &rest[end..];
            continue;
        }
        if let Some(after) = rest.strip_prefix('`') {
            let end = after.find('`').map_or(rest.len(), |i| i + 2);
            out.push_str(&rest[..end]);
            rest = &rest[end..];
            continue;
        }
        if let Some(after) = rest.strip_prefix(':') {
            let len = after
                .find(|c: char| !is_name_char(c))
                .unwrap_or(after.len());
            if len > 0
                && after[len..].starts_with(':')
                && let Some(e) = by_name().get(&after[..len])
            {
                out.push_str(e);
                rest = &after[len + 1..];
                continue;
            }
        }
        let c = rest.chars().next().unwrap_or_default();
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_known() {
        assert_eq!(to_unicode("+1"), "👍");
        assert_eq!(to_unicode("thumbsup"), "👍");
        assert_eq!(to_unicode("white_check_mark"), "✅");
        assert_eq!(to_name("👍"), "+1");
        assert_eq!(to_name(&to_unicode("tada")), "tada");
    }

    #[test]
    fn variation_selector_ignored() {
        assert_eq!(to_name("❤"), "heart");
        assert_eq!(to_name("❤️"), "heart");
    }

    #[test]
    fn unknown_names() {
        assert_eq!(to_unicode("partyparrot"), ":partyparrot:");
        assert_eq!(to_name(":partyparrot:"), "partyparrot");
    }

    #[test]
    fn every_emoji_maps_back_to_a_name_with_same_emoji() {
        for (_, e) in TABLE {
            assert_eq!(strip_vs(&to_unicode(&to_name(e))), strip_vs(e));
        }
    }

    #[test]
    fn shortcodes_in_text() {
        assert_eq!(replace_shortcodes("ok :+1: :smile:!"), "ok 👍 😄!");
        assert_eq!(replace_shortcodes("Zeit 12:30:45"), "Zeit 12:30:45");
        assert_eq!(
            replace_shortcodes(":partyparrot: :tada:"),
            ":partyparrot: 🎉"
        );
        assert_eq!(replace_shortcodes("`:smile:` :smile:"), "`:smile:` 😄");
        assert_eq!(
            replace_shortcodes("```\n:smile:\n```:smile:"),
            "```\n:smile:\n```😄"
        );
        assert_eq!(replace_shortcodes("a::gibtsnicht: :"), "a::gibtsnicht: :");
        assert_eq!(replace_shortcodes("offen `:smile:"), "offen `:smile:");
    }
}
