//! Integrationstest gegen einen echten Server (z. B. Docker `mattermost/mattermost-preview`).
//!
//! Testserver und Umgebungsvariablen: `scripts/mm-test-server.sh` (schreibt `src-tauri/target/mm-test.env`).
//! Aufruf: `set -a; . target/mm-test.env; set +a; cargo test -p chatty-core mattermost::it -- --ignored --nocapture`.
//! Legt Keyring-Einträge `chatty-it-*` an und löscht sie am Ende wieder.

use std::sync::Arc;
use std::time::Duration;

use reqwest::Method;
use serde_json::json;
use tokio::sync::{mpsc, watch};

use super::MattermostBackend;
use crate::backend::ChatBackend;
use crate::error::{Error, Result};
use crate::model::{Account, Attachment, BackendEvent, BackendKind, SpaceKind, UploadFile};
use crate::secrets;

type Rx = mpsc::UnboundedReceiver<(String, BackendEvent)>;

fn env(key: &str) -> Result<String> {
    std::env::var(key).map_err(|_| Error::other(format!("{key} fehlt")))
}

async fn wait_for(
    rx: &mut Rx,
    what: &str,
    pred: impl Fn(&BackendEvent) -> bool,
) -> Result<BackendEvent> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        let left = deadline.saturating_duration_since(tokio::time::Instant::now());
        match tokio::time::timeout(left, rx.recv()).await {
            Ok(Some((_, ev))) if pred(&ev) => {
                println!("event ok: {what}");
                return Ok(ev);
            }
            Ok(Some(_)) => {}
            _ => return Err(Error::other(format!("kein Ereignis: {what}"))),
        }
    }
}

fn check(cond: bool, what: &str) -> Result<()> {
    if cond {
        println!("ok: {what}");
        Ok(())
    } else {
        Err(Error::other(format!("fehlgeschlagen: {what}")))
    }
}

async fn scenario() -> Result<()> {
    let url = env("CHATTY_MM_URL")?;
    let me = MattermostBackend::login(
        "chatty-it-1".into(),
        format!("{url}/"),
        env("CHATTY_MM_USER")?,
        env("CHATTY_MM_PASSWORD")?,
        None,
    )
    .await?;
    let anna = MattermostBackend::login(
        "chatty-it-2".into(),
        url.clone(),
        env("CHATTY_MM_USER2")?,
        env("CHATTY_MM_PASSWORD2")?,
        Some(String::new()),
    )
    .await?;
    println!("login ok: {}", me.account().label);

    // Kanal nur mit anna: fehlt in der Liste, ist per Suche findbar und beitretbar.
    let team = anna
        .my_teams()
        .await?
        .into_iter()
        .next()
        .ok_or(Error::other("kein Team"))?;
    let name = format!("it-{:08x}", rand::random::<u32>());
    let created: serde_json::Value = anna
        .post_json(
            "/channels",
            &serde_json::json!({ "team_id": team.id, "name": name, "display_name": name, "type": "O" }),
        )
        .await?;
    let foreign = created["id"].as_str().unwrap_or_default().to_string();
    check(
        !me.list_spaces().await?.iter().any(|s| s.id == foreign),
        "fremder Kanal nicht in der Liste",
    )?;
    check(
        me.browse_spaces(&name)
            .await?
            .iter()
            .any(|s| s.id == foreign),
        "Suche findet fremden Kanal",
    )?;
    check(me.join_space(&foreign).await?.id == foreign, "beigetreten")?;
    check(
        me.list_spaces().await?.iter().any(|s| s.id == foreign),
        "Kanal nach Beitritt in der Liste",
    )?;
    check(
        !me.browse_spaces(&name)
            .await?
            .iter()
            .any(|s| s.id == foreign),
        "Suche zeigt beigetretenen Kanal nicht mehr",
    )?;

    // Wiederherstellen mit gespeichertem Token, falsches Token → Auth.
    let me = Arc::new(MattermostBackend::restore(me.account().clone()).await?);
    let mut bad: Account = me.account().clone();
    bad.id = "chatty-it-bad".into();
    secrets::set(&bad.id, "ungueltig")?;
    check(
        matches!(MattermostBackend::restore(bad).await, Err(Error::Auth(_))),
        "restore mit falschem Token → Auth",
    )?;

    let spaces = me.list_spaces().await?;
    println!(
        "spaces: {:?}",
        spaces.iter().map(|s| (&s.name, s.kind)).collect::<Vec<_>>()
    );
    let dev = spaces
        .iter()
        .find(|s| s.name.ends_with("/ Dev"))
        .ok_or_else(|| Error::other("Kanal Dev fehlt"))?
        .id
        .clone();
    let anna_name = anna.me().await?.name;
    check(
        spaces
            .iter()
            .any(|s| s.kind == SpaceKind::Direct && s.name == anna_name),
        "DM mit Partnername",
    )?;
    check(
        spaces
            .iter()
            .filter(|s| s.kind == SpaceKind::Direct)
            .count()
            == 1,
        "DMs dedupliziert trotz zwei Teams",
    )?;
    me.mark_read(&dev).await?;

    let (tx, mut rx) = mpsc::unbounded_channel();
    let (_active_tx, active_rx) = watch::channel(None);
    let task = me
        .clone()
        .start_events(tx, active_rx, watch::channel(true).1);
    wait_for(&mut rx, "online", |e| {
        matches!(e, BackendEvent::Status { online: true, .. })
    })
    .await?;

    // Fremde Nachricht mit Erwähnung → MessageCreated + SpaceUpdated(unread).
    let my_name = me.me().await?.name;
    let calls_before = me.call_count();
    let incoming = anna
        .send(&dev, &format!("Hallo @{my_name} zebrafisch"), None, vec![])
        .await?;
    let ev = wait_for(
        &mut rx,
        "posted",
        |e| matches!(e, BackendEvent::MessageCreated { message } if message.id == incoming.id),
    )
    .await?;
    if let BackendEvent::MessageCreated { message } = ev {
        check(
            message.mentions_me && !message.mine && message.sender.name == anna_name,
            "posted: mentions_me, Absender",
        )?;
    }
    wait_for(&mut rx, "space unread", |e| {
        matches!(e, BackendEvent::SpaceUpdated { space } if space.id == dev && space.unread && space.mention_count > 0)
    })
    .await?;
    check(
        me.call_count() == calls_before,
        "posted in bekanntem Kanal: kein REST-Aufruf",
    )?;

    me.mark_read(&dev).await?;
    wait_for(
        &mut rx,
        "channel_viewed",
        |e| matches!(e, BackendEvent::SpaceUpdated { space } if space.id == dev && !space.unread),
    )
    .await?;

    // Thread-Antwort ohne Erwähnung: Kanal bleibt gelesen.
    let reply = anna
        .send(
            &dev,
            "Antwort ohne Erwähnung",
            incoming.thread_id.as_deref(),
            vec![],
        )
        .await?;
    wait_for(
        &mut rx,
        "Thread-Antwort",
        |e| matches!(e, BackendEvent::MessageCreated { message } if message.id == reply.id),
    )
    .await?;
    wait_for(&mut rx, "Space nach Thread-Antwort", |e| {
        matches!(e, BackendEvent::SpaceUpdated { space }
            if space.id == dev && !space.unread && space.mention_count == 0)
    })
    .await?;

    // Neue DM von bert: unbekannter Kanal wird einmal geladen.
    let bert = MattermostBackend::login(
        "chatty-it-3".into(),
        url.clone(),
        env("CHATTY_MM_USER3")?,
        env("CHATTY_MM_PASSWORD3")?,
        None,
    )
    .await?;
    let ids = json!([me.me.id, bert.me.id]);
    let dm: super::api::ApiChannel = bert
        .send_req(bert.req(Method::POST, "/channels/direct").json(&ids))
        .await?
        .json()
        .await?;
    let before = me.call_count();
    bert.send(&dm.id, "Hallo von bert", None, vec![]).await?;
    wait_for(
        &mut rx,
        "neue DM",
        |e| matches!(e, BackendEvent::SpaceUpdated { space } if space.id == dm.id && space.unread),
    )
    .await?;
    let delta = me.call_count() - before;
    println!("REST-Aufrufe für neue DM: {delta}");
    check(delta >= 1, "neue DM: Kanal nachgeladen")?;

    // Emoji-Kurzcode im Text wird Unicode, Code bleibt unverändert.
    let emo = anna
        .send(&dev, "Super :tada: `:tada:`", None, vec![])
        .await?;
    let ev = wait_for(
        &mut rx,
        "posted mit Emoji",
        |e| matches!(e, BackendEvent::MessageCreated { message } if message.id == emo.id),
    )
    .await?;
    if let BackendEvent::MessageCreated { message } = ev {
        check(
            message.text == "Super 🎉 `:tada:`",
            "Kurzcode → Unicode, nicht in Code",
        )?;
    }

    // Status-Emoji des DM-Partners: per WebSocket (user_updated) in der DM sichtbar.
    let status = json!({ "emoji": "palm_tree", "text": "Urlaub" });
    anna.send_req(
        anna.req(Method::PUT, "/users/me/status/custom")
            .json(&status),
    )
    .await?;
    wait_for(&mut rx, "user_updated → DM mit Status", |e| {
        matches!(e, BackendEvent::SpaceUpdated { space }
            if space.kind == SpaceKind::Direct && space.status_emoji.as_deref() == Some("🌴"))
    })
    .await?;
    anna.send_req(anna.req(Method::DELETE, "/users/me/status/custom"))
        .await?;

    // Favorit und Stumm: beim Server gespeichert, per WebSocket gemeldet, in der Liste sichtbar.
    check(
        me.set_favorite(&dev, true).await?,
        "set_favorite beim Server",
    )?;
    wait_for(
        &mut rx,
        "preferences_changed",
        |e| matches!(e, BackendEvent::SpaceUpdated { space } if space.id == dev && space.favorite),
    )
    .await?;
    check(me.set_muted(&dev, true).await?, "set_muted beim Server")?;
    wait_for(
        &mut rx,
        "channel_member_updated",
        |e| matches!(e, BackendEvent::SpaceUpdated { space } if space.id == dev && space.muted),
    )
    .await?;
    let listed = me.list_spaces().await?;
    let d = listed.iter().find(|s| s.id == dev);
    check(
        d.is_some_and(|s| s.favorite && s.muted && s.msg_count > 0),
        "Liste: Favorit, stumm, msg_count",
    )?;
    me.set_favorite(&dev, false).await?;
    me.set_muted(&dev, false).await?;
    let d = me.list_spaces().await?.into_iter().find(|s| s.id == dev);
    check(
        d.is_some_and(|s| !s.favorite && !s.muted),
        "Favorit und Stumm zurückgesetzt",
    )?;

    // Kanal-Header (Titel, Links): per WebSocket (channel_updated) und in der Liste.
    let header = "Doku: [Wiki](https://example.org/wiki)";
    me.send_req(
        me.req(Method::PUT, &format!("/channels/{dev}/patch"))
            .json(&json!({ "header": header })),
    )
    .await?;
    wait_for(&mut rx, "channel_updated → Header", |e| {
        matches!(e, BackendEvent::SpaceUpdated { space } if space.id == dev && space.header == header)
    })
    .await?;
    let d = me.list_spaces().await?.into_iter().find(|s| s.id == dev);
    check(d.is_some_and(|s| s.header == header), "Liste: Header")?;

    // Eigene Nachricht mit Anhang, Antwort im Thread.
    let file = UploadFile {
        name: "hallo.txt".into(),
        mime: "text/plain".into(),
        data: b"Inhalt 123".to_vec(),
    };
    let root = me.send(&dev, "Wurzel mit Datei", None, vec![file]).await?;
    check(
        root.mine && root.attachments.len() == 1 && !root.is_reply,
        "send mit Anhang",
    )?;
    let reply = me
        .send(&dev, "Antwort", root.thread_id.as_deref(), vec![])
        .await?;
    check(
        reply.is_reply && reply.thread_id == root.thread_id,
        "Antwort im Thread",
    )?;
    let thread = me.list_thread(&dev, &root.id).await?;
    check(
        thread.len() == 2 && thread[0].id == root.id && thread[1].id == reply.id,
        "list_thread aufsteigend",
    )?;
    let page = me.list_messages(&dev, None).await?;
    let listed_root = page.messages.iter().find(|m| m.id == root.id);
    check(
        listed_root.is_some_and(|m| m.reply_count == 1),
        "Hauptverlauf: Wurzel mit reply_count 1",
    )?;
    check(
        page.messages
            .iter()
            .any(|m| m.id == reply.id && m.is_reply && m.thread_id == root.thread_id),
        "Hauptverlauf mit Antwort, als Antwort markiert",
    )?;
    check(
        page.messages
            .windows(2)
            .all(|w| w[0].created <= w[1].created),
        "Hauptverlauf aufsteigend",
    )?;

    let data = me.download(&root.attachments[0]).await?;
    check(data == b"Inhalt 123", "download")?;
    check(
        !crate::backend::is_preview_image(&root.attachments[0]),
        "Text-Anhang ohne Vorschau",
    )?;

    // Bild: Vorschau vom Server (verkleinertes JPEG, entsteht evtl. erst kurz nach dem Hochladen).
    let png = UploadFile {
        name: "bild.png".into(),
        mime: "image/png".into(),
        data: include_bytes!("../../../../icons/128x128.png").to_vec(),
    };
    let pic = me.send(&dev, "Bild", None, vec![png]).await?;
    let att = &pic.attachments[0];
    check(
        crate::backend::is_preview_image(att),
        "Bild-Anhang mit Vorschau",
    )?;
    let mut preview = None;
    for _ in 0..20 {
        if let Ok(Some(p)) = me.preview(&att.id).await {
            preview = Some(p);
            break;
        }
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
    check(
        preview.is_some_and(|p| p.mime.starts_with("image/") && !p.data.is_empty()),
        "preview",
    )?;
    check(
        me.preview("../users/me").await.is_err(),
        "Vorschau-ID mit ../ abgelehnt",
    )?;

    let edited = me.edit(&dev, &root.id, "Wurzel bearbeitet").await?;
    check(
        edited.edited.is_some() && edited.text == "Wurzel bearbeitet",
        "edit",
    )?;
    wait_for(
        &mut rx,
        "post_edited",
        |e| matches!(e, BackendEvent::MessageUpdated { message } if message.id == root.id),
    )
    .await?;

    for emoji in ["👍", "🤔", "❤️", "🎉"] {
        me.react(&dev, &root.id, emoji).await?;
        wait_for(&mut rx, &format!("reaction_added {emoji}"), |e| {
            matches!(e, BackendEvent::MessageUpdated { message }
                if message.id == root.id && message.reactions.iter().any(|r| r.emoji == emoji && r.me))
        })
        .await?;
    }
    anna.react(&dev, &root.id, "👍").await?;
    wait_for(&mut rx, "reaction count 2", |e| {
        matches!(e, BackendEvent::MessageUpdated { message }
            if message.reactions.iter().any(|r| r.emoji == "👍" && r.count == 2 && r.me))
    })
    .await?;
    me.unreact(&dev, &root.id, "👍").await?;
    wait_for(&mut rx, "reaction_removed", |e| {
        matches!(e, BackendEvent::MessageUpdated { message }
            if message.reactions.iter().any(|r| r.emoji == "👍" && r.count == 1 && !r.me))
    })
    .await?;

    let hits = me.search("zebrafisch").await?;
    check(hits.iter().any(|m| m.id == incoming.id), "search")?;

    // Seitenweise laden.
    for i in 0..55 {
        anna.send(&dev, &format!("Fülltext {i}"), None, vec![])
            .await?;
    }
    let first = me.list_messages(&dev, None).await?;
    check(
        first.messages.len() == 50 && first.next_cursor.is_some(),
        "Seite 1: 50 + Cursor",
    )?;
    let second = me.list_messages(&dev, first.next_cursor.clone()).await?;
    let oldest_first = first
        .messages
        .first()
        .map(|m| m.created)
        .unwrap_or_default();
    check(
        !second.messages.is_empty() && second.messages.iter().all(|m| m.created <= oldest_first),
        "Seite 2 älter als Seite 1",
    )?;

    me.delete(&dev, &root.id).await?;
    wait_for(
        &mut rx,
        "post_deleted",
        |e| matches!(e, BackendEvent::MessageDeleted { message_id, .. } if *message_id == root.id),
    )
    .await?;

    task.abort();

    // Sicherheit: Datei-ID mit Pfadteilen abgelehnt; Logout beendet die Sitzung serverseitig.
    let evil = Attachment {
        id: "../users/me".into(),
        name: "x".into(),
        mime: None,
        size: None,
        url: None,
    };
    check(
        me.download(&evil).await.is_err(),
        "Datei-ID mit ../ abgelehnt",
    )?;
    anna.logout().await?;
    check(
        matches!(anna.list_spaces().await, Err(Error::Auth(_))),
        "nach Logout ist das Token ungültig",
    )?;
    if let Ok(proxy) = std::env::var("CHATTY_MM_PROXY_URL") {
        reconnect_via_proxy(&proxy).await?;
    }
    Ok(())
}

fn docker(args: &[&str]) -> Result<()> {
    let ok = std::process::Command::new("docker")
        .args(args)
        .stdout(std::process::Stdio::null())
        .status()
        .map_err(|e| Error::other(e.to_string()))?
        .success();
    check(ok, &format!("docker {}", args.join(" ")))
}

/// Abriss über den Proxy: stabile Sitzung verbindet neu, kurze Sitzung meldet offline (N4).
async fn reconnect_via_proxy(proxy: &str) -> Result<()> {
    let backend = Arc::new(
        MattermostBackend::restore(Account {
            id: "chatty-it-1".into(),
            kind: BackendKind::Mattermost,
            server: Some(proxy.to_string()),
            label: "proxy".into(),
            login: None,
        })
        .await?,
    );
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (_active_tx, active_rx) = watch::channel(None);
    let task = backend.start_events(tx, active_rx, watch::channel(true).1);
    let online = |e: &BackendEvent| matches!(e, BackendEvent::Status { online: true, .. });
    wait_for(&mut rx, "online über Proxy", online).await?;
    println!("warte 35 s auf stabile Sitzung …");
    tokio::time::sleep(Duration::from_secs(35)).await;
    docker(&["restart", "-t", "0", "chatty-mm-proxy"])?;
    wait_for(&mut rx, "nach Abriss wieder online", online).await?;
    // Kurze Sitzung: sofort wieder trennen → offline wird gemeldet statt still zu schleifen.
    docker(&["restart", "-t", "0", "chatty-mm-proxy"])?;
    wait_for(&mut rx, "kurze Sitzung → offline", |e| {
        matches!(e, BackendEvent::Status { online: false, .. })
    })
    .await?;
    wait_for(&mut rx, "danach wieder online", online).await?;
    task.abort();
    Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "braucht laufenden Mattermost-Server"]
async fn mattermost_end_to_end() -> Result<()> {
    let result = scenario().await;
    for id in ["chatty-it-1", "chatty-it-2", "chatty-it-3", "chatty-it-bad"] {
        secrets::delete(id)?;
    }
    result
}
