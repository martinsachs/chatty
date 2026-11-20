//! Testwerkzeug: Backend-Ereignisse eines Mattermost-Kontos mitschneiden.
//!
//! `cargo run --example mm_watch -- <konto-id> <server-url> <sekunden>`
//! Das Token muss im Keyring liegen (App-Konto oder `mm_login`).

use std::sync::Arc;
use std::time::{Duration, Instant};

use chatty_lib::backend::ChatBackend;
use chatty_lib::backend::mattermost::MattermostBackend;
use chatty_lib::error::{Error, Result};
use chatty_lib::model::{Account, BackendKind};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [id, server, secs] = args.as_slice() else {
        return Err(Error::other(
            "Aufruf: mm_watch <konto-id> <server-url> <sekunden>",
        ));
    };
    let secs: u64 = secs
        .parse()
        .map_err(|_| Error::other("Sekunden ungültig"))?;
    let backend = Arc::new(
        MattermostBackend::restore(Account {
            id: id.clone(),
            kind: BackendKind::Mattermost,
            server: Some(server.clone()),
            label: id.clone(),
            login: None,
        })
        .await?,
    );
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
    let (_active, active_rx) = tokio::sync::watch::channel(None);
    let task = backend.start_events(tx, active_rx);
    let start = Instant::now();
    let end = tokio::time::Instant::now() + Duration::from_secs(secs);
    while let Ok(Some((_, event))) = tokio::time::timeout_at(end, rx.recv()).await {
        let line = serde_json::to_string(&event)?;
        let short: String = line.chars().take(160).collect();
        println!("{:>6.1}s {short}", start.elapsed().as_secs_f64());
    }
    task.abort();
    println!("Ende nach {} s", start.elapsed().as_secs());
    Ok(())
}
