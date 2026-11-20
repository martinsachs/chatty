//! Tokens im Schlüsselbund des Betriebssystems (Secret Service, Credential Manager, Keychain).

use crate::error::Result;

const SERVICE: &str = "io.github.martinsachs.chatty";

fn entry(account_id: &str) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new(SERVICE, account_id)?)
}

pub fn set(account_id: &str, secret: &str) -> Result<()> {
    entry(account_id)?.set_password(secret)?;
    Ok(())
}

/// None, wenn kein Eintrag existiert.
pub fn get(account_id: &str) -> Result<Option<String>> {
    match entry(account_id)?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn delete(account_id: &str) -> Result<()> {
    match entry(account_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}
