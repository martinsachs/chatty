//! Polling: offener Space häufig (Diff der neuesten Seite), Space-Liste seltener.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tokio::time::{MissedTickBehavior, interval, sleep};

use super::GoogleBackend;
use crate::backend::{ChatBackend, EventSender};
use crate::error::{Error, Result};
use crate::model::{BackendEvent, Message, Reaction, Space};

/// Je Nachricht: (Erstellzeit, Änderungszeit, Text, Reaktionen).
type Entry = (i64, Option<i64>, String, Vec<Reaction>);
pub type Snapshot = HashMap<String, Entry>;

pub fn snapshot(msgs: &[Message]) -> Snapshot {
    msgs.iter()
        .map(|m| {
            (
                m.id.clone(),
                (m.created, m.edited, m.text.clone(), m.reactions.clone()),
            )
        })
        .collect()
}

/// Vergleicht die neueste Seite mit dem vorherigen Stand. Gelöscht gilt nur, was im
/// Zeitfenster der aktuellen Seite liegt (ältere Nachrichten fallen sonst nur heraus).
pub fn diff(prev: &Snapshot, page: &[Message]) -> Vec<BackendEvent> {
    let mut events = Vec::new();
    for m in page {
        match prev.get(&m.id) {
            None => events.push(BackendEvent::MessageCreated { message: m.clone() }),
            Some((_, edited, text, reactions))
                if *edited != m.edited || *text != m.text || *reactions != m.reactions =>
            {
                events.push(BackendEvent::MessageUpdated { message: m.clone() });
            }
            Some(_) => {}
        }
    }
    let oldest = page.iter().map(|m| m.created).min();
    let space_id = page.first().map(|m| m.space_id.clone());
    if let (Some(oldest), Some(space_id)) = (oldest, space_id) {
        let mut gone: Vec<_> = prev
            .iter()
            .filter(|(id, (created, ..))| *created >= oldest && !page.iter().any(|m| &m.id == *id))
            .map(|(id, _)| id.clone())
            .collect();
        gone.sort();
        events.extend(
            gone.into_iter()
                .map(|message_id| BackendEvent::MessageDeleted {
                    space_id: space_id.clone(),
                    message_id,
                }),
        );
    }
    events
}

/// Geänderte oder neue Spaces gegenüber dem vorherigen Stand.
pub fn changed_spaces<'a>(prev: &HashMap<String, Space>, now: &'a [Space]) -> Vec<&'a Space> {
    now.iter().filter(|s| prev.get(&s.id) != Some(*s)).collect()
}

/// Fehler, die auf Verbindungs-/Quotenprobleme deuten (Backoff, Abbruch des Durchlaufs).
pub(super) fn is_transient(e: &Error) -> bool {
    match e {
        Error::Http(_) | Error::Auth(_) => true,
        Error::Api { status, .. } => *status == 429 || *status >= 500,
        _ => false,
    }
}

/// Als „offline“ melden? Nicht bei 429: das ist Quote, keine Verbindungsstörung (Backoff reicht).
fn reports_offline(e: &Error) -> bool {
    is_transient(e) && !matches!(e, Error::Api { status: 429, .. })
}

/// Den offenen Space in der Space-Abfrage überspringen? Nur, wenn ihn die schnelle Abfrage bedient,
/// also ein Fenster den Fokus hat (ADR 07).
fn skip_open_space(focused: bool, open: Option<&str>, space: &str) -> bool {
    focused && open == Some(space)
}

fn backoff(failures: u32) -> Duration {
    Duration::from_secs((5u64 << failures.min(6)).min(300))
}

pub(super) struct State {
    active: Option<String>,
    focused: bool,
    snap: Option<(String, Snapshot)>,
    spaces: Option<HashMap<String, Space>>,
}

pub async fn run(
    b: Arc<GoogleBackend>,
    tx: EventSender,
    mut active_rx: watch::Receiver<Option<String>>,
    mut focused_rx: watch::Receiver<bool>,
) {
    let account = b.account().id.clone();
    let mut active_tick = interval(Duration::from_secs(b.poll.active_secs.max(1)));
    let mut spaces_tick = interval(Duration::from_secs(b.poll.spaces_secs.max(1)));
    active_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    spaces_tick.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut st = State {
        active: active_rx.borrow().clone(),
        focused: *focused_rx.borrow(),
        snap: None,
        spaces: None,
    };
    let mut online = true;
    let mut failures = 0u32;

    loop {
        let result = tokio::select! {
            _ = active_tick.tick() => poll_active(&b, &mut st).await,
            _ = spaces_tick.tick() => poll_spaces(&b, &mut st).await,
            changed = active_rx.changed() => {
                if changed.is_err() {
                    return;
                }
                st.active = active_rx.borrow_and_update().clone();
                st.snap = None;
                poll_active(&b, &mut st).await
            }
            changed = focused_rx.changed() => {
                if changed.is_err() {
                    return;
                }
                let focused = *focused_rx.borrow_and_update();
                // Fokus zurück: erst die Space-Abfrage samt offenem Space nachholen, dann übernimmt
                // die schnelle Abfrage mit frischem Stand (keine doppelten Ereignisse).
                let result = if focused { poll_spaces(&b, &mut st).await } else { Ok(Vec::new()) };
                st.focused = focused;
                st.snap = None;
                result
            }
        };
        let mut out = Vec::new();
        match result {
            Ok(events) => {
                if !online {
                    online = true;
                    out.push(BackendEvent::Status {
                        online: true,
                        detail: None,
                    });
                }
                failures = 0;
                out.extend(events);
            }
            Err(e) if is_transient(&e) => {
                if online && reports_offline(&e) {
                    online = false;
                    out.push(BackendEvent::Status {
                        online: false,
                        detail: Some(e.to_string()),
                    });
                }
                failures += 1;
            }
            // z. B. 403/404 auf einen verlassenen Space: nicht offline melden.
            Err(_) => {}
        }
        for e in out {
            if tx.send((account.clone(), e)).is_err() {
                return;
            }
        }
        if tx.is_closed() {
            return;
        }
        if failures > 0 {
            sleep(backoff(failures - 1)).await;
        }
    }
}

async fn poll_active(b: &GoogleBackend, st: &mut State) -> Result<Vec<BackendEvent>> {
    // Ohne Fokus übernimmt die Space-Abfrage den offenen Space (ADR 07).
    let Some(space) = st.active.clone().filter(|_| st.focused) else {
        return Ok(Vec::new());
    };
    let page = b.latest_page(&space).await?;
    let events = match &st.snap {
        Some((s, prev)) if *s == space => diff(prev, &page),
        _ => Vec::new(),
    };
    st.snap = Some((space, snapshot(&page)));
    Ok(events)
}

#[cfg(test)]
impl State {
    pub(super) fn for_test(active: Option<String>, focused: bool) -> Self {
        Self {
            active,
            focused,
            snap: None,
            spaces: None,
        }
    }
}

pub(super) async fn poll_spaces(b: &GoogleBackend, st: &mut State) -> Result<Vec<BackendEvent>> {
    let spaces = b.list_spaces().await?;
    let mut events = Vec::new();
    if let Some(prev) = &st.spaces {
        for s in changed_spaces(prev, &spaces) {
            events.push(BackendEvent::SpaceUpdated { space: s.clone() });
            let Some(old) = prev.get(&s.id) else { continue };
            if skip_open_space(st.focused, st.active.as_deref(), &s.id)
                || s.last_active <= old.last_active
            {
                continue;
            }
            match b.messages_since(&s.id, old.last_active).await {
                Ok(msgs) => events.extend(
                    msgs.into_iter()
                        .map(|message| BackendEvent::MessageCreated { message }),
                ),
                Err(e) if is_transient(&e) => return Err(e),
                // z. B. 403/404: diesen Space überspringen, die anderen melden; sein neuer Stand wird
                // übernommen, damit er nicht bei jedem Durchlauf erneut abgefragt wird.
                Err(e) => eprintln!("Google: Nachrichten von {} nicht lesbar: {e}", s.id),
            }
        }
    }
    st.spaces = Some(spaces.into_iter().map(|s| (s.id.clone(), s)).collect());
    Ok(events)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{SpaceKind, User};

    #[test]
    fn quota_is_not_offline() {
        let api = |status| Error::Api {
            status,
            body: String::new(),
        };
        assert!(!reports_offline(&api(429)), "429: Backoff ja, offline nein");
        assert!(is_transient(&api(429)));
        assert!(reports_offline(&api(503)));
        assert!(reports_offline(&Error::Auth("x".into())));
        assert!(!reports_offline(&api(404)));
    }

    #[test]
    fn open_space_skipped_only_with_focus() {
        assert!(skip_open_space(true, Some("spaces/A"), "spaces/A"));
        assert!(!skip_open_space(false, Some("spaces/A"), "spaces/A"));
        assert!(!skip_open_space(true, Some("spaces/A"), "spaces/B"));
        assert!(!skip_open_space(true, None, "spaces/A"));
    }

    fn msg(id: &str, created: i64) -> Message {
        Message {
            id: id.into(),
            space_id: "spaces/A".into(),
            thread_id: None,
            is_reply: false,
            sender: User {
                id: "users/1".into(),
                name: "n".into(),
                avatar_url: None,
            },
            text: format!("text {id}"),
            created,
            edited: None,
            reactions: vec![],
            attachments: vec![],
            reply_count: 0,
            mentions_me: false,
            mine: false,
        }
    }

    #[test]
    fn diff_detects_changes() {
        let old = vec![msg("a", 1), msg("b", 2), msg("c", 3), msg("d", 4)];
        let prev = snapshot(&old);

        let mut edited = msg("c", 3);
        edited.edited = Some(10);
        edited.text = "neu".into();
        let mut reacted = msg("d", 4);
        reacted.reactions = vec![Reaction {
            emoji: "👍".into(),
            count: 1,
            me: false,
        }];
        // Seite verschoben: "a" fällt aus dem Fenster (nicht gelöscht), "b" gelöscht, "e" neu.
        let page = vec![edited, reacted, msg("e", 5)];
        let mut page2 = page.clone();
        page2.insert(0, msg("x", 2)); // Fenster beginnt bei 2 → "b" liegt im Fenster
        let ev = diff(&prev, &page2);

        let kinds: Vec<String> = ev
            .iter()
            .map(|e| match e {
                BackendEvent::MessageCreated { message } => format!("new {}", message.id),
                BackendEvent::MessageUpdated { message } => format!("upd {}", message.id),
                BackendEvent::MessageDeleted {
                    message_id,
                    space_id,
                } => format!("del {message_id} {space_id}"),
                _ => "?".into(),
            })
            .collect();
        assert_eq!(
            kinds,
            ["new x", "upd c", "upd d", "new e", "del b spaces/A"]
        );

        // Ohne "x" beginnt das Fenster bei 3: "a" und "b" sind nur herausgefallen.
        assert!(
            !diff(&prev, &page)
                .iter()
                .any(|e| matches!(e, BackendEvent::MessageDeleted { .. }))
        );
        // Unverändert → keine Ereignisse.
        assert!(diff(&prev, &old).is_empty());
        // Leere Seite → nichts löschen.
        assert!(diff(&prev, &[]).is_empty());
    }

    #[test]
    fn changed_spaces_detects_new_and_modified() {
        let s = |id: &str, t: i64, unread: bool| Space {
            id: id.into(),
            name: id.into(),
            kind: SpaceKind::Channel,
            unread,
            mention_count: 0,
            last_active: t,
            msg_count: 0,
            favorite: false,
            muted: false,
            status_emoji: None,
            header: String::new(),
        };
        let prev: HashMap<_, _> = [s("a", 1, false), s("b", 1, false)]
            .into_iter()
            .map(|x| (x.id.clone(), x))
            .collect();
        let now = vec![s("a", 1, false), s("b", 2, true), s("c", 1, false)];
        let ids: Vec<_> = changed_spaces(&prev, &now)
            .iter()
            .map(|x| x.id.as_str())
            .collect();
        assert_eq!(ids, ["b", "c"]);
    }

    #[test]
    fn backoff_grows_and_caps() {
        assert_eq!(backoff(0), Duration::from_secs(5));
        assert_eq!(backoff(1), Duration::from_secs(10));
        assert_eq!(backoff(10), Duration::from_secs(300));
        assert!(is_transient(&Error::Api {
            status: 429,
            body: String::new()
        }));
        assert!(is_transient(&Error::Api {
            status: 503,
            body: String::new()
        }));
        assert!(!is_transient(&Error::Api {
            status: 404,
            body: String::new()
        }));
    }
}
