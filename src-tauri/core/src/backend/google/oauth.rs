//! OAuth 2.0 für installierte Apps: Loopback-Redirect, PKCE (S256), Refresh-Token.
//! Tokens werden nie geloggt oder in Fehlertexte übernommen.

use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::Mutex;

use crate::config::GoogleConfig;
use crate::error::{Error, Result};

pub const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

pub const SCOPES: &[&str] = &[
    "https://www.googleapis.com/auth/chat.spaces.readonly",
    "https://www.googleapis.com/auth/chat.messages",
    "https://www.googleapis.com/auth/chat.memberships.readonly",
    "https://www.googleapis.com/auth/chat.users.readstate",
    "https://www.googleapis.com/auth/directory.readonly",
    "https://www.googleapis.com/auth/userinfo.profile",
    "https://www.googleapis.com/auth/userinfo.email",
];

/// Token 60 s vor Ablauf erneuern.
const REFRESH_MARGIN: Duration = Duration::from_secs(60);

const PAGE_OK: &str = "<!doctype html><html lang=\"de\"><meta charset=\"utf-8\"><title>chatty</title>\
<body style=\"font-family:sans-serif;padding:2em\"><h2>Anmeldung abgeschlossen</h2>\
<p>Dieses Fenster kann geschlossen werden.</p></body></html>";
const PAGE_ERR: &str = "<!doctype html><html lang=\"de\"><meta charset=\"utf-8\"><title>chatty</title>\
<body style=\"font-family:sans-serif;padding:2em\"><h2>Anmeldung fehlgeschlagen</h2>\
<p>Fenster schließen und in chatty erneut versuchen.</p></body></html>";

/// Zufallswert als base64url ohne Padding (`n` Bytes).
pub fn random_b64(n: usize) -> String {
    let bytes: Vec<u8> = (0..n).map(|_| rand::random::<u8>()).collect();
    URL_SAFE_NO_PAD.encode(bytes)
}

/// PKCE-Challenge zum Verifier (S256).
pub fn pkce_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

pub fn auth_url(
    client_id: &str,
    redirect_uri: &str,
    challenge: &str,
    state: &str,
) -> Result<String> {
    let scope = SCOPES.join(" ");
    let url = url::Url::parse_with_params(
        AUTH_URL,
        &[
            ("client_id", client_id),
            ("redirect_uri", redirect_uri),
            ("response_type", "code"),
            ("scope", scope.as_str()),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
            ("state", state),
            ("access_type", "offline"),
            ("prompt", "consent"),
        ],
    )
    .map_err(|e| Error::other(e.to_string()))?;
    Ok(url.into())
}

/// Wertet die erste Zeile des Redirect-Requests aus (`GET /?state=..&code=.. HTTP/1.1`).
/// `Ok(None)`: kein OAuth-Redirect (z. B. favicon.ico) oder fremder `state` – beides wird ignoriert, damit ein
/// anderer lokaler Prozess die Anmeldung nicht mit einem falschen Request abbrechen kann.
pub fn parse_callback(request_line: &str, expected_state: &str) -> Result<Option<String>> {
    let target = request_line.split_whitespace().nth(1).unwrap_or("/");
    let url = url::Url::parse(&format!("http://127.0.0.1{target}"))
        .map_err(|e| Error::Auth(format!("Ungültiger Redirect: {e}")))?;
    let mut code = None;
    let mut state = None;
    let mut error = None;
    for (k, v) in url.query_pairs() {
        match k.as_ref() {
            "code" => code = Some(v.into_owned()),
            "state" => state = Some(v.into_owned()),
            "error" => error = Some(v.into_owned()),
            _ => {}
        }
    }
    if code.is_none() && state.is_none() && error.is_none() {
        return Ok(None);
    }
    if state.as_deref() != Some(expected_state) {
        return Ok(None);
    }
    if let Some(e) = error {
        return Err(Error::Auth(format!("Google meldet: {e}")));
    }
    code.map(Some)
        .ok_or_else(|| Error::Auth("Kein Code im Redirect".into()))
}

const CONN_TIMEOUT: Duration = Duration::from_secs(5);

/// Nimmt Verbindungen an, bis der OAuth-Redirect kommt; antwortet mit einer kleinen Seite.
pub async fn wait_for_code(
    listener: &TcpListener,
    state: &str,
    timeout: Duration,
) -> Result<String> {
    let fut = async {
        loop {
            let (mut sock, _) = listener.accept().await?;
            let mut buf = Vec::new();
            // Je Verbindung höchstens kurz warten: eine stumme Verbindung blockiert sonst den Redirect.
            let read = tokio::time::timeout(CONN_TIMEOUT, async {
                let mut chunk = [0u8; 2048];
                while !buf.windows(2).any(|w| w == b"\r\n") && buf.len() < 16 * 1024 {
                    let n = sock.read(&mut chunk).await?;
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
                Ok::<_, std::io::Error>(())
            })
            .await;
            if !matches!(read, Ok(Ok(()))) {
                continue;
            }
            let text = String::from_utf8_lossy(&buf);
            let line = text.lines().next().unwrap_or("");
            let result = parse_callback(line, state);
            let (status, page) = match &result {
                Ok(None) => ("404 Not Found", ""),
                Ok(Some(_)) => ("200 OK", PAGE_OK),
                Err(_) => ("400 Bad Request", PAGE_ERR),
            };
            let resp = format!(
                "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
                page.len()
            );
            // Antwort an den Browser ist Kür; Fehler hier nicht entscheidend.
            let _ = sock.write_all(resp.as_bytes()).await;
            let _ = sock.shutdown().await;
            match result {
                Ok(None) => continue,
                Ok(Some(code)) => return Ok(code),
                Err(e) => return Err(e),
            }
        }
    };
    tokio::time::timeout(timeout, fut)
        .await
        .map_err(|_| Error::Auth("Zeitüberschreitung bei der Google-Anmeldung".into()))?
}

/// Kein `Debug`: enthält Tokens.
#[derive(Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub expires_in: Option<u64>,
    #[serde(default)]
    pub refresh_token: Option<String>,
}

#[derive(Deserialize)]
struct TokenError {
    #[serde(default)]
    error: String,
    #[serde(default)]
    error_description: String,
}

async fn token_request(
    http: &reqwest::Client,
    url: &str,
    form: &[(&str, &str)],
) -> Result<TokenResponse> {
    let resp = http.post(url).form(form).send().await?;
    let status = resp.status();
    let body = resp.text().await?;
    if status.is_success() {
        return Ok(serde_json::from_str(&body)?);
    }
    // Fehlerantworten des Token-Endpunkts enthalten keine Tokens.
    let err: TokenError = serde_json::from_str(&body).unwrap_or(TokenError {
        error: String::new(),
        error_description: String::new(),
    });
    if matches!(
        err.error.as_str(),
        "invalid_grant" | "invalid_client" | "unauthorized_client"
    ) {
        return Err(Error::Auth(format!(
            "Google: {} {} – bitte neu anmelden",
            err.error, err.error_description
        )));
    }
    Err(Error::Api {
        status: status.as_u16(),
        body,
    })
}

pub async fn exchange_code(
    http: &reqwest::Client,
    token_url: &str,
    cfg: &GoogleConfig,
    code: &str,
    verifier: &str,
    redirect_uri: &str,
) -> Result<TokenResponse> {
    let mut form = vec![
        ("client_id", cfg.client_id.as_str()),
        ("code", code),
        ("code_verifier", verifier),
        ("redirect_uri", redirect_uri),
        ("grant_type", "authorization_code"),
    ];
    if !cfg.client_secret.is_empty() {
        form.push(("client_secret", cfg.client_secret.as_str()));
    }
    token_request(http, token_url, &form).await
}

async fn refresh(
    http: &reqwest::Client,
    token_url: &str,
    cfg: &GoogleConfig,
    refresh_token: &str,
) -> Result<TokenResponse> {
    let mut form = vec![
        ("client_id", cfg.client_id.as_str()),
        ("refresh_token", refresh_token),
        ("grant_type", "refresh_token"),
    ];
    if !cfg.client_secret.is_empty() {
        form.push(("client_secret", cfg.client_secret.as_str()));
    }
    token_request(http, token_url, &form).await
}

/// Access-Token mit Ablaufzeit; erneuert sich über das Refresh-Token.
pub struct Tokens {
    http: reqwest::Client,
    token_url: String,
    cfg: GoogleConfig,
    refresh_token: String,
    current: Mutex<Option<(String, Instant)>>,
}

impl Tokens {
    pub fn new(
        http: reqwest::Client,
        token_url: String,
        cfg: GoogleConfig,
        refresh_token: String,
    ) -> Self {
        Self {
            http,
            token_url,
            cfg,
            refresh_token,
            current: Mutex::new(None),
        }
    }

    /// Übernimmt ein frisch erhaltenes Access-Token (nach dem Login).
    pub async fn set(&self, tok: &TokenResponse) {
        *self.current.lock().await = Some((tok.access_token.clone(), expiry(tok)));
    }

    /// Gültiges Access-Token, bei Bedarf erneuert. Parallele Aufrufer warten auf eine Erneuerung.
    pub async fn access(&self) -> Result<String> {
        let mut cur = self.current.lock().await;
        if let Some((tok, exp)) = cur.as_ref()
            && Instant::now() + REFRESH_MARGIN < *exp
        {
            return Ok(tok.clone());
        }
        let r = refresh(&self.http, &self.token_url, &self.cfg, &self.refresh_token).await?;
        *cur = Some((r.access_token.clone(), expiry(&r)));
        Ok(r.access_token)
    }

    /// Refresh-Token bei Google widerrufen (beim Entfernen des Kontos).
    pub async fn revoke(&self) -> Result<()> {
        self.http
            .post("https://oauth2.googleapis.com/revoke")
            .form(&[("token", self.refresh_token.as_str())])
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }

    /// Nach HTTP 401: Token verwerfen, falls es noch das aktuelle ist.
    pub async fn invalidate(&self, token: &str) {
        let mut cur = self.current.lock().await;
        if cur.as_ref().is_some_and(|(t, _)| t == token) {
            *cur = None;
        }
    }
}

fn expiry(tok: &TokenResponse) -> Instant {
    Instant::now() + Duration::from_secs(tok.expires_in.unwrap_or(3600))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_rfc7636_vector() {
        // RFC 7636, Anhang B
        assert_eq!(
            pkce_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
        let v = random_b64(32);
        assert_eq!(v.len(), 43);
        assert_ne!(v, random_b64(32));
    }

    #[test]
    fn auth_url_contains_params() {
        let u = auth_url("cid.apps", "http://127.0.0.1:4711", "chal", "st").unwrap();
        let parsed = url::Url::parse(&u).unwrap();
        let q: std::collections::HashMap<_, _> = parsed.query_pairs().into_owned().collect();
        assert!(u.starts_with(AUTH_URL));
        assert_eq!(q["client_id"], "cid.apps");
        assert_eq!(q["redirect_uri"], "http://127.0.0.1:4711");
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["code_challenge"], "chal");
        assert_eq!(q["state"], "st");
        assert_eq!(q["access_type"], "offline");
        assert_eq!(q["prompt"], "consent");
        assert_eq!(q["response_type"], "code");
        assert!(q["scope"].contains("https://www.googleapis.com/auth/chat.messages "));
        assert_eq!(q["scope"].split(' ').count(), SCOPES.len());
    }

    #[test]
    fn callback_parsing() {
        assert_eq!(
            parse_callback("GET /?state=abc&code=4%2F0Ab&scope=x HTTP/1.1", "abc").unwrap(),
            Some("4/0Ab".into())
        );
        assert_eq!(
            parse_callback("GET /favicon.ico HTTP/1.1", "abc").unwrap(),
            None
        );
        // Fremder state: ignorieren statt abbrechen (lokaler Prozess könnte sonst die Anmeldung stören).
        assert!(matches!(
            parse_callback("GET /?state=xyz&code=c HTTP/1.1", "abc"),
            Ok(None)
        ));
        assert!(matches!(
            parse_callback("GET /?state=abc&error=access_denied HTTP/1.1", "abc"),
            Err(Error::Auth(m)) if m.contains("access_denied")
        ));
        assert!(matches!(
            parse_callback("GET /?state=abc HTTP/1.1", "abc"),
            Err(Error::Auth(_))
        ));
    }

    #[tokio::test]
    async fn loopback_receives_code() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let client = tokio::spawn(async move {
            let mut pages = Vec::new();
            for path in ["/favicon.ico", "/?state=evil&error=x", "/?state=s1&code=c1"] {
                let mut s = tokio::net::TcpStream::connect(("127.0.0.1", port))
                    .await
                    .unwrap();
                s.write_all(format!("GET {path} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes())
                    .await
                    .unwrap();
                let mut out = String::new();
                s.read_to_string(&mut out).await.unwrap();
                pages.push(out);
            }
            pages
        });
        let code = wait_for_code(&listener, "s1", Duration::from_secs(5))
            .await
            .unwrap();
        assert_eq!(code, "c1");
        let pages = client.await.unwrap();
        assert!(pages[0].starts_with("HTTP/1.1 404"));
        assert!(pages[1].starts_with("HTTP/1.1 404"));
        assert!(pages[2].contains("Anmeldung abgeschlossen"));
    }

    #[tokio::test]
    async fn loopback_timeout() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let r = wait_for_code(&listener, "s", Duration::from_millis(50)).await;
        assert!(matches!(r, Err(Error::Auth(_))));
    }
}
