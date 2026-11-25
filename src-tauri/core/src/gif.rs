//! GIF-Suche über eine GIPHY-kompatible API (z. B. GIPHY, heypster). Aus, solange `[gif]` in
//! `chatty.toml` fehlt; Suchbegriff und IP-Adresse gehen an den eingetragenen Anbieter.

use std::time::Duration;

use base64::Engine;
use serde::{Deserialize, Serialize};

use crate::backend::{init_tls, read_limited};
use crate::error::{Error, Result};

const RESULT_LIMIT: &str = "24";
/// Nur jugendfreie Ergebnisse (GIPHY-Stufe „g“), passend für Arbeits-Chats.
const RATING: &str = "g";
const MAX_RESPONSE: usize = 2 << 20;
/// Vorschaubilder lädt die Suche für jedes Ergebnis ohne Klick.
const MAX_PREVIEW: usize = 1 << 20;
const TIMEOUT: Duration = Duration::from_secs(15);

/// Anbieter aus `chatty.toml` (`[gif]`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GifConfig {
    /// Basis-URL der API, z. B. "https://api.giphy.com/v1/gifs"; nur https.
    pub api_url: String,
    pub api_key: String,
}

/// Suchergebnis für die Oberfläche.
#[derive(Debug, Clone, Serialize)]
pub struct Gif {
    pub title: String,
    /// Adresse, die als Nachricht verschickt wird.
    pub url: String,
    /// Vorschau als data:-URL: die CSP der Oberfläche lässt keine fremden Bildquellen zu.
    pub preview: String,
}

#[derive(Deserialize)]
struct SearchResponse {
    data: Vec<ApiGif>,
}

#[derive(Deserialize)]
struct ApiGif {
    #[serde(default)]
    title: String,
    images: ApiImages,
}

#[derive(Deserialize)]
struct ApiImages {
    original: Option<ApiImage>,
    downsized: Option<ApiImage>,
    fixed_width_small: Option<ApiImage>,
    preview_gif: Option<ApiImage>,
}

#[derive(Deserialize)]
struct ApiImage {
    url: String,
}

/// Ergebnis vor dem Laden der Vorschau.
#[derive(Debug, PartialEq)]
struct Candidate {
    title: String,
    url: String,
    preview_url: String,
}

/// Nur https-Adressen der API akzeptieren (Schlüssel und Suchbegriff gehen sonst im Klartext übers Netz).
pub fn require_https(url: &str) -> Result<()> {
    let parsed =
        url::Url::parse(url).map_err(|e| Error::other(format!("Ungültige GIF-API-URL: {e}")))?;
    if parsed.scheme() != "https" {
        return Err(Error::other("GIF-API-URL muss mit https:// beginnen"));
    }
    Ok(())
}

fn is_https(url: &str) -> bool {
    url::Url::parse(url).is_ok_and(|u| u.scheme() == "https")
}

/// Versandadresse: verkleinerte Fassung, sonst Original; Vorschau: kleine, sonst Mini-Vorschau.
fn candidates(body: &[u8]) -> Result<Vec<Candidate>> {
    let response: SearchResponse = serde_json::from_slice(body)?;
    Ok(response
        .data
        .into_iter()
        .filter_map(|gif| {
            let images = gif.images;
            let url = images.downsized.or(images.original)?.url;
            let preview_url = images.fixed_width_small.or(images.preview_gif)?.url;
            (is_https(&url) && is_https(&preview_url)).then_some(Candidate {
                title: gif.title,
                url,
                preview_url,
            })
        })
        .collect())
}

fn client() -> Result<reqwest::Client> {
    init_tls();
    Ok(reqwest::Client::builder()
        .user_agent(concat!("chatty/", env!("CARGO_PKG_VERSION")))
        .timeout(TIMEOUT)
        .build()?)
}

async fn checked(resp: reqwest::Response) -> Result<reqwest::Response> {
    let status = resp.status();
    if status.is_success() {
        return Ok(resp);
    }
    Err(Error::Api {
        status: status.as_u16(),
        body: format!("GIF-Anbieter antwortet mit {status}"),
    })
}

async fn preview(client: &reqwest::Client, url: &str) -> Result<String> {
    let resp = checked(client.get(url).send().await?).await?;
    let mime = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    if !matches!(
        mime.as_str(),
        "image/gif" | "image/webp" | "image/png" | "image/jpeg"
    ) {
        return Err(Error::other(format!("Vorschau ist kein Bild: {mime}")));
    }
    let data = read_limited(resp, MAX_PREVIEW).await?;
    let encoded = base64::engine::general_purpose::STANDARD.encode(data);
    Ok(format!("data:{mime};base64,{encoded}"))
}

/// GIFs zu `query` suchen; Ergebnisse ohne ladbare Vorschau fallen weg.
pub async fn search(config: &GifConfig, query: &str) -> Result<Vec<Gif>> {
    require_https(&config.api_url)?;
    let client = client()?;
    let url = format!("{}/search", config.api_url.trim_end_matches('/'));
    let params = [
        ("api_key", config.api_key.as_str()),
        ("q", query),
        ("limit", RESULT_LIMIT),
        ("rating", RATING),
    ];
    let resp = checked(client.get(url).query(&params).send().await?).await?;
    let found = candidates(&read_limited(resp, MAX_RESPONSE).await?)?;
    let previews =
        futures_util::future::join_all(found.iter().map(|c| preview(&client, &c.preview_url)))
            .await;
    Ok(found
        .into_iter()
        .zip(previews)
        .filter_map(|(c, preview)| {
            Some(Gif {
                title: c.title,
                url: c.url,
                preview: preview.ok()?,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_downsized_and_small_preview_and_skips_plain_http() {
        let body = br#"{"data": [
            {"title": "Cat", "images": {
                "original": {"url": "https://media.example/cat.gif"},
                "downsized": {"url": "https://media.example/cat-d.gif"},
                "fixed_width_small": {"url": "https://media.example/cat-s.gif"}}},
            {"title": "Dog", "images": {
                "original": {"url": "https://media.example/dog.gif"},
                "preview_gif": {"url": "https://media.example/dog-p.gif"}}},
            {"title": "Insecure", "images": {
                "original": {"url": "http://media.example/x.gif"},
                "preview_gif": {"url": "https://media.example/x-p.gif"}}},
            {"title": "No preview", "images": {"original": {"url": "https://media.example/y.gif"}}}
        ]}"#;
        let found = candidates(body).unwrap();
        assert_eq!(
            found,
            vec![
                Candidate {
                    title: "Cat".into(),
                    url: "https://media.example/cat-d.gif".into(),
                    preview_url: "https://media.example/cat-s.gif".into(),
                },
                Candidate {
                    title: "Dog".into(),
                    url: "https://media.example/dog.gif".into(),
                    preview_url: "https://media.example/dog-p.gif".into(),
                },
            ]
        );
    }

    #[test]
    fn api_url_needs_https() {
        assert!(require_https("https://api.giphy.com/v1/gifs").is_ok());
        assert!(require_https("http://api.giphy.com/v1/gifs").is_err());
        assert!(require_https("api.giphy.com").is_err());
    }
}
