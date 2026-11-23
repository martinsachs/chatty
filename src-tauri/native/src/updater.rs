//! Selbst-Update für chatty-native (ADR 08): `latest-native.json` aus dem neuesten GitHub-Release, signierte
//! Datei je Plattform, Prüfung mit demselben minisign-Schlüssel wie der Tauri-Updater.
//! Windows/macOS: die laufende Binärdatei wird ersetzt (NSIS installiert ohne Admin-Rechte, .app gehört dem
//! Nutzer). Linux: nur als AppImage – die AppImage-Datei (`$APPIMAGE`) wird ersetzt; deb-Installationen
//! aktualisiert die Paketverwaltung.
//!
//! Manifest:
//! `{"version":"0.2.3","platforms":{"linux-x86_64":{"url":"…","signature":"<Base64 der .sig>"},…}}`
//!
//! Downgrade-Schutz: Der signierte Kommentar der Signatur nennt den Dateinamen
//! (`file:chatty-native-<version>-<plattform>[.exe|.AppImage]`). Er muss zu Version und Plattform aus dem
//! Manifest passen, sonst ließe sich eine alte, gültig signierte Datei als neue Version ausliefern.

use std::collections::HashMap;

use base64::Engine;
use serde::Deserialize;

const MANIFEST: &str =
    "https://github.com/martinsachs/chatty/releases/latest/download/latest-native.json";
/// Obergrenze für die Update-Datei (AppImage ~40 MiB); schützt vor Speicherfluten vor der Signaturprüfung.
const MAX_UPDATE: usize = 300 << 20;
/// Öffentlicher Schlüssel aus `tauri.conf.json` (eine Quelle für beide Varianten).
const TAURI_CONF: &str = include_str!("../../tauri.conf.json");

#[derive(Debug, Clone, Deserialize)]
pub struct Release {
    pub version: String,
    pub url: String,
    pub signature: String,
}

#[derive(Deserialize)]
struct Manifest {
    version: String,
    platforms: HashMap<String, Asset>,
}

#[derive(Deserialize)]
struct Asset {
    url: String,
    signature: String,
}

/// Plattform-Schlüssel wie im Manifest, z. B. `linux-x86_64`, `windows-x86_64`, `darwin-aarch64`.
fn platform() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    format!("{os}-{}", std::env::consts::ARCH)
}

fn version(v: &str) -> Vec<u64> {
    v.trim_start_matches('v')
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// Neuere Version für diese Plattform, sonst `None` (auch bei Netz- oder Formatfehlern: kein Hinweis).
pub async fn check() -> Option<Release> {
    let manifest: Manifest = reqwest::get(MANIFEST).await.ok()?.json().await.ok()?;
    newer(manifest, env!("CARGO_PKG_VERSION"))
}

fn newer(m: Manifest, current: &str) -> Option<Release> {
    if version(&m.version) <= version(current) {
        return None;
    }
    let asset = m.platforms.get(&platform())?;
    Some(Release {
        version: m.version,
        url: asset.url.clone(),
        signature: asset.signature.clone(),
    })
}

fn public_key() -> Result<minisign_verify::PublicKey, String> {
    let conf: serde_json::Value = serde_json::from_str(TAURI_CONF).map_err(|e| e.to_string())?;
    let b64 = conf["plugins"]["updater"]["pubkey"]
        .as_str()
        .ok_or("pubkey fehlt in tauri.conf.json")?;
    decode_key(b64)
}

/// Schlüssel und Signaturen liegen wie bei Tauri als Base64 der minisign-Textdatei vor.
fn b64_text(b64: &str) -> Result<String, String> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|e| e.to_string())?;
    String::from_utf8(raw).map_err(|e| e.to_string())
}

fn decode_key(b64: &str) -> Result<minisign_verify::PublicKey, String> {
    minisign_verify::PublicKey::decode(&b64_text(b64)?).map_err(|e| e.to_string())
}

/// Prüft Signatur und signierten Kommentar; liefert den signierten Dateinamen.
fn verify(key: &minisign_verify::PublicKey, data: &[u8], sig_b64: &str) -> Result<String, String> {
    let sig = minisign_verify::Signature::decode(&b64_text(sig_b64)?).map_err(|e| e.to_string())?;
    key.verify(data, &sig, false)
        .map_err(|e| format!("Signatur ungültig: {e}"))?;
    sig.trusted_comment()
        .split('\t')
        .find_map(|p| p.strip_prefix("file:"))
        .map(str::to_string)
        .ok_or_else(|| "Signatur ohne Dateinamen".into())
}

/// Gehört der signierte Dateiname zu dieser Version und Plattform?
fn signed_for(file: &str, version: &str, platform: &str) -> bool {
    file.strip_prefix(&format!(
        "chatty-native-{}-{platform}",
        version.trim_start_matches('v')
    ))
    .is_some_and(|rest| matches!(rest, "" | ".exe" | ".AppImage"))
}

/// Laden, Signatur prüfen, laufende Datei ersetzen.
pub async fn install(r: Release) -> Result<(), String> {
    if !r.url.starts_with("https://") {
        return Err("Update-Adresse ohne https – abgelehnt".into());
    }
    let resp = reqwest::get(&r.url)
        .await
        .and_then(|resp| resp.error_for_status())
        .map_err(|e| e.to_string())?;
    let data = chatty_core::backend::read_limited(resp, MAX_UPDATE)
        .await
        .map_err(|e| e.to_string())?;
    let file = verify(&public_key()?, &data, &r.signature)?;
    if !signed_for(&file, &r.version, &platform()) {
        return Err(format!(
            "Signatur gehört zu „{file}“, nicht zu Version {} – Update abgelehnt",
            r.version
        ));
    }
    if cfg!(target_os = "linux") {
        let Some(appimage) = std::env::var_os("APPIMAGE") else {
            return Err(
                "Nur die AppImage aktualisiert sich selbst; deb über die Paketverwaltung.".into(),
            );
        };
        // Neben der Zieldatei schreiben, dann umbenennen (gleiches Dateisystem, atomar).
        let target = std::path::PathBuf::from(appimage);
        let tmp = target.with_extension("update");
        write_executable(&tmp, &data)?;
        return std::fs::rename(&tmp, &target).map_err(|e| e.to_string());
    }
    let tmp = std::env::temp_dir().join(format!("chatty-native-{}", r.version));
    write_executable(&tmp, &data)?;
    let result = self_replace::self_replace(&tmp).map_err(|e| e.to_string());
    let _ = std::fs::remove_file(&tmp);
    result
}

fn write_executable(path: &std::path::Path, data: &[u8]) -> Result<(), String> {
    std::fs::write(path, data).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Neue Version starten und diese beenden.
pub fn restart<T>() -> iced::Task<T> {
    // In der AppImage zeigt current_exe in das eingehängte Abbild; neu starten über die Datei selbst.
    let exe = std::env::var_os("APPIMAGE")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::current_exe().ok());
    if let Some(exe) = exe {
        let _ = std::process::Command::new(exe).spawn();
    }
    iced::exit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(version("0.10.0") > version("0.9.9"));
        assert!(version("v1.2.3") == version("1.2.3"));
    }

    #[test]
    fn picks_platform_only_when_newer() {
        let manifest = |v: &str| Manifest {
            version: v.into(),
            platforms: HashMap::from([(
                platform(),
                Asset {
                    url: "u".into(),
                    signature: "s".into(),
                },
            )]),
        };
        assert!(newer(manifest("9.0.0"), "0.1.1").is_some());
        assert!(newer(manifest("0.1.1"), "0.1.1").is_none());
    }

    /// Schlüssel und Signatur aus `tauri signer generate/sign` (Wegwerf-Schlüssel, privater Teil gelöscht).
    #[test]
    fn verifies_tauri_signer_signature() {
        let key = decode_key(include_str!("../tests/fixtures/update.pub")).unwrap();
        let data = include_bytes!("../tests/fixtures/update.bin");
        let sig = include_str!("../tests/fixtures/update.bin.sig");
        assert_eq!(verify(&key, data, sig).as_deref(), Ok("data.bin"));
        assert!(verify(&key, b"manipuliert", sig).is_err());
    }

    #[test]
    fn signature_bound_to_version_and_platform() {
        assert!(signed_for(
            "chatty-native-0.2.3-linux-x86_64.AppImage",
            "0.2.3",
            "linux-x86_64"
        ));
        assert!(signed_for(
            "chatty-native-0.2.3-windows-x86_64.exe",
            "v0.2.3",
            "windows-x86_64"
        ));
        assert!(signed_for(
            "chatty-native-0.2.3-darwin-aarch64",
            "0.2.3",
            "darwin-aarch64"
        ));
        // Alte Datei (ohne bzw. mit anderer Version) unter neuer Versionsnummer: abgelehnt.
        assert!(!signed_for(
            "chatty-native-linux-x86_64.AppImage",
            "9.9.9",
            "linux-x86_64"
        ));
        assert!(!signed_for(
            "chatty-native-0.2.3-linux-x86_64.AppImage",
            "0.2.30",
            "linux-x86_64"
        ));
        assert!(!signed_for(
            "chatty-native-0.2.30-linux-x86_64.AppImage",
            "0.2.3",
            "linux-x86_64"
        ));
        assert!(!signed_for(
            "chatty-native-0.2.3-darwin-x86_64",
            "0.2.3",
            "darwin-aarch64"
        ));
    }

    #[test]
    fn embedded_public_key_decodes() {
        assert!(public_key().is_ok());
    }
}
