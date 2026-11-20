//! Zuordnung Mattermost-Emoji-Name ↔ Unicode für die häufigsten Reaktionen.

/// (Name, Emoji). Der erste Name je Emoji ist der, den wir beim Reagieren senden.
const TABLE: &[(&str, &str)] = &[
    ("+1", "👍"),
    ("thumbsup", "👍"),
    ("-1", "👎"),
    ("thumbsdown", "👎"),
    ("smile", "😄"),
    ("smiley", "😃"),
    ("grinning", "😀"),
    ("grin", "😁"),
    ("laughing", "😆"),
    ("satisfied", "😆"),
    ("joy", "😂"),
    ("rofl", "🤣"),
    ("sweat_smile", "😅"),
    ("slightly_smiling_face", "🙂"),
    ("upside_down_face", "🙃"),
    ("wink", "😉"),
    ("blush", "😊"),
    ("innocent", "😇"),
    ("heart_eyes", "😍"),
    ("kissing_heart", "😘"),
    ("stuck_out_tongue", "😛"),
    ("sunglasses", "😎"),
    ("nerd_face", "🤓"),
    ("thinking", "🤔"),
    ("thinking_face", "🤔"),
    ("hugs", "🤗"),
    ("hugging_face", "🤗"),
    ("confused", "😕"),
    ("slightly_frowning_face", "🙁"),
    ("disappointed", "😞"),
    ("cry", "😢"),
    ("sob", "😭"),
    ("scream", "😱"),
    ("angry", "😠"),
    ("rage", "😡"),
    ("partying_face", "🥳"),
    ("see_no_evil", "🙈"),
    ("face_palm", "🤦"),
    ("facepalm", "🤦"),
    ("shrug", "🤷"),
    ("heart", "❤️"),
    ("tada", "🎉"),
    ("white_check_mark", "✅"),
    ("heavy_check_mark", "✔️"),
    ("x", "❌"),
    ("eyes", "👀"),
    ("rocket", "🚀"),
    ("pray", "🙏"),
    ("ok_hand", "👌"),
    ("clap", "👏"),
    ("muscle", "💪"),
    ("wave", "👋"),
    ("raised_hands", "🙌"),
    ("fire", "🔥"),
    ("100", "💯"),
    ("star", "⭐"),
    ("zap", "⚡"),
    ("bulb", "💡"),
    ("warning", "⚠️"),
    ("question", "❓"),
    ("exclamation", "❗"),
    ("heavy_plus_sign", "➕"),
    ("coffee", "☕"),
    ("beers", "🍻"),
    ("bug", "🐛"),
];

/// Ohne Variationsselektor (U+FE0F) vergleichen, damit "❤" und "❤️" gleich sind.
fn strip_vs(s: &str) -> String {
    s.chars().filter(|c| *c != '\u{FE0F}').collect()
}

/// Mattermost-Name → Unicode; unbekannte Namen als ":name:".
pub fn to_unicode(name: &str) -> String {
    TABLE
        .iter()
        .find(|(n, _)| *n == name)
        .map_or_else(|| format!(":{name}:"), |(_, e)| (*e).to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_known() {
        assert_eq!(to_unicode("+1"), "👍");
        assert_eq!(to_unicode("thumbsup"), "👍");
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
            assert_eq!(to_unicode(&to_name(e)), *e);
        }
    }
}
