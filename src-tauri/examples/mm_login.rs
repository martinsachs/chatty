//! Testwerkzeug: Session-Token des Testnutzers im Keyring ablegen bzw. löschen.
//!
//! `cargo run --example mm_login -- <konto-id> [--delete]`
//! Erwartet `CHATTY_MM_PROXY_URL`, `CHATTY_MM_USER`, `CHATTY_MM_PASSWORD` (aus `src-tauri/target/mm-test.env`).

use chatty_lib::backend::mattermost::MattermostBackend;
use chatty_lib::error::{Error, Result};

#[tokio::main]
async fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let id = args
        .first()
        .cloned()
        .ok_or_else(|| Error::other("Aufruf: mm_login <konto-id> [--delete]"))?;
    if args.iter().any(|a| a == "--delete") {
        chatty_lib::secrets::delete(&id)?;
        println!("Keyring-Eintrag {id} gelöscht");
        return Ok(());
    }
    let env = |k: &str| std::env::var(k).map_err(|_| Error::other(format!("{k} fehlt")));
    MattermostBackend::login(
        id.clone(),
        env("CHATTY_MM_PROXY_URL")?,
        env("CHATTY_MM_USER")?,
        env("CHATTY_MM_PASSWORD")?,
        None,
    )
    .await?;
    println!("Token für {id} im Keyring");
    Ok(())
}
