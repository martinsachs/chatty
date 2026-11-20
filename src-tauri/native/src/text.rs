//! Nachrichtentext für die Markdown-Darstellung vorbereiten.

/// Chat-Nachrichten trennen Zeilen mit einfachem Zeilenumbruch; Markdown würde sie zusammenziehen.
/// Außerhalb von Code-Blöcken wird daher jeder Umbruch ein harter Umbruch (zwei Leerzeichen am Zeilenende).
pub fn hard_breaks(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut in_code = false;
    let mut lines = text.lines().peekable();
    while let Some(line) = lines.next() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        }
        if in_code || line.trim_start().starts_with("```") {
            out.push_str(line);
        } else {
            out.push_str(&autolink(line));
        }
        if lines.peek().is_some() {
            if !in_code && !line.trim().is_empty() && !line.trim_start().starts_with("```") {
                out.push_str("  ");
            }
            out.push('\n');
        }
    }
    out
}

/// Nackte URLs als Markdown-Autolink (`<https://…>`), wie Mattermost sie verlinkt. Inline-Code, bereits
/// verlinkte Adressen (`](`, `<`) und Satzzeichen am Ende bleiben unberührt.
fn autolink(line: &str) -> String {
    let mut out = String::with_capacity(line.len() + 8);
    let mut in_code = false;
    for (i, word) in line.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        if word.matches('`').count() % 2 == 1 {
            in_code = !in_code;
        }
        let start = ["https://", "http://"]
            .iter()
            .filter_map(|p| word.find(p))
            .min();
        match start {
            Some(at)
                if !in_code
                    && !word.contains('`')
                    && !word[..at].ends_with(['(', '<', '['])
                    && !word.contains("](") =>
            {
                let (pre, rest) = word.split_at(at);
                let end = rest
                    .trim_end_matches(['.', ',', ';', ':', '!', '?', ')'])
                    .len();
                out.push_str(pre);
                out.push('<');
                out.push_str(&rest[..end]);
                out.push('>');
                out.push_str(&rest[end..]);
            }
            _ => out.push_str(word),
        }
    }
    out
}

/// Lokales Datum einer Zeit in Millisekunden.
pub fn date(ms: i64) -> Option<chrono::NaiveDate> {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|t| t.with_timezone(&chrono::Local).date_naive())
}

/// Uhrzeit `HH:MM` in Ortszeit.
pub fn time(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string())
        .unwrap_or_default()
}

/// Tagestrenner wie in der Tauri-Oberfläche: „Heute“, „Gestern“, sonst „Mittwoch, 18. November 2026“.
pub fn day(ms: i64) -> String {
    let Some(d) = date(ms) else {
        return String::new();
    };
    let today = chrono::Local::now().date_naive();
    day_label(d, today)
}

fn day_label(d: chrono::NaiveDate, today: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    const DAYS: [&str; 7] = [
        "Montag",
        "Dienstag",
        "Mittwoch",
        "Donnerstag",
        "Freitag",
        "Samstag",
        "Sonntag",
    ];
    const MONTHS: [&str; 12] = [
        "Januar",
        "Februar",
        "März",
        "April",
        "Mai",
        "Juni",
        "Juli",
        "August",
        "September",
        "Oktober",
        "November",
        "Dezember",
    ];
    if d == today {
        return "Heute".into();
    }
    if today.pred_opt() == Some(d) {
        return "Gestern".into();
    }
    format!(
        "{}, {}. {} {}",
        DAYS[d.weekday().num_days_from_monday() as usize],
        d.day(),
        MONTHS[d.month0() as usize],
        d.year()
    )
}

/// Dateigröße lesbar.
pub fn size(bytes: Option<u64>) -> String {
    match bytes {
        Some(b) if b >= 1 << 20 => format!("{:.1} MB", b as f64 / (1u64 << 20) as f64),
        Some(b) if b >= 1 << 10 => format!("{} KB", b >> 10),
        Some(b) => format!("{b} B"),
        None => String::new(),
    }
}

/// Initialen für Profilbild-Ersatz.
pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaks_outside_code_only() {
        assert_eq!(hard_breaks("a\nb"), "a  \nb");
        assert_eq!(hard_breaks("```\nx\ny\n```\nz"), "```\nx\ny\n```\nz");
        assert_eq!(hard_breaks("a\n\nb"), "a  \n\nb");
    }

    #[test]
    fn links_bare_urls() {
        assert_eq!(autolink("siehe https://x.de/a."), "siehe <https://x.de/a>.");
        assert_eq!(autolink("[t](https://x.de)"), "[t](https://x.de)");
        assert_eq!(autolink("`https://x.de`"), "`https://x.de`");
        assert_eq!(autolink("<https://x.de>"), "<https://x.de>");
    }

    #[test]
    fn day_labels() {
        let today = chrono::NaiveDate::from_ymd_opt(2026, 11, 21).unwrap();
        assert_eq!(day_label(today, today), "Heute");
        assert_eq!(day_label(today.pred_opt().unwrap(), today), "Gestern");
        let d = chrono::NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        assert_eq!(day_label(d, today), "Mittwoch, 18. November 2026");
    }

    #[test]
    fn formats() {
        assert_eq!(size(Some(2048)), "2 KB");
        assert_eq!(size(Some(3 << 20)), "3.0 MB");
        assert_eq!(initials("anna bert"), "AB");
    }
}
