//! Google OAuth 2.0 - bring-your-own-client model.
//!
//! Each user creates a Desktop OAuth client in their own Google Cloud project
//! and drops `oauth_client.json` into `%LOCALAPPDATA%\Archeio\`. Reasoning:
//! - No single bundled credential to leak in an open-source build.
//! - Per-user OAuth = per-user 10k-unit/day YouTube quota.
//! - No need for Google verification (each user "owns" their own app).
//!
//! Tokens are stored as JSON in Windows Credential Manager under
//! service="archeio", user="youtube-tokens".
//!
//! No PKCE here - Desktop OAuth clients ship with a `client_secret` which is
//! sufficient for the loopback flow per Google's installed-app guidance.

use std::time::Duration;

use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

use crate::error::{Error, Result};

const TOKEN_KEY_USER: &str = "youtube-tokens";
const SERVICE: &str = "archeio";
const SCOPE: &str = "https://www.googleapis.com/auth/youtube";

/// Shape of the oauth_client.json file Google Cloud Console produces for a
/// Desktop client. We only need a few fields; the rest is ignored.
#[derive(Debug, Clone, Deserialize)]
struct ClientFile {
    installed: ClientCreds,
}

#[derive(Debug, Clone, Deserialize)]
struct ClientCreds {
    client_id: String,
    client_secret: String,
}

/// What we actually persist in Credential Manager.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredTokens {
    access_token: String,
    refresh_token: String,
    /// Wall-clock instant the access token expires. We refresh ~30s early.
    expires_at: DateTime<Utc>,
}

/// Public status surface for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct OAuthStatus {
    pub connected: bool,
    /// True iff `oauth_client.json` exists at `%LOCALAPPDATA%\Archeio\`.
    /// When false, the UI tells the user to drop the file in.
    pub client_present: bool,
}

pub fn status() -> OAuthStatus {
    OAuthStatus {
        connected: load_tokens().ok().flatten().is_some(),
        client_present: client_file_path().exists(),
    }
}

/// Run the full Connect flow: read client creds, open browser, catch loopback
/// redirect, exchange code for tokens, store. Blocks until the browser flow
/// completes (or 5-minute timeout).
pub async fn connect() -> Result<()> {
    let creds = read_client()?;

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| Error::Other(format!("loopback bind: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| Error::Other(format!("loopback addr: {e}")))?
        .port();
    let redirect_uri = format!("http://127.0.0.1:{port}");

    let state: String = random_state();
    let auth_url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth\
         ?client_id={client_id}\
         &redirect_uri={redirect}\
         &response_type=code\
         &scope={scope}\
         &access_type=offline\
         &prompt=consent\
         &state={state}",
        client_id = urlencoding::encode(&creds.client_id),
        redirect = urlencoding::encode(&redirect_uri),
        scope = urlencoding::encode(SCOPE),
        state = urlencoding::encode(&state),
    );

    open_in_browser(&auth_url)?;
    tracing::info!("oauth: opened consent URL on port {port}");

    // Catch the redirect (5-minute deadline). Ignore favicon/etc. probes -
    // only the request carrying ?code= or ?error= counts.
    let (code, returned_state) = tokio::time::timeout(
        Duration::from_secs(300),
        wait_for_redirect(listener),
    )
    .await
    .map_err(|_| Error::Other("OAuth timeout - did you finish in the browser?".into()))??;

    if returned_state != state {
        return Err(Error::Other("OAuth state mismatch (CSRF) - try again".into()));
    }

    let tokens = exchange_code(&creds, &code, &redirect_uri).await?;
    store_tokens(&tokens)?;
    tracing::info!("oauth: tokens stored");
    Ok(())
}

pub fn disconnect() -> Result<()> {
    let entry = keyring::Entry::new(SERVICE, TOKEN_KEY_USER)?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(e.into()),
    }
}

/// Persist `oauth_client.json` from in-app pasted credentials. Same on-disk
/// format Google's Cloud Console produces, so users who already dropped a
/// file in keep working without changes.
pub fn save_client(client_id: &str, client_secret: &str) -> Result<()> {
    let id = client_id.trim();
    let secret = client_secret.trim();
    if id.is_empty() || secret.is_empty() {
        return Err(Error::Other(
            "Both Client ID and Client Secret are required.".into(),
        ));
    }
    let path = client_file_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let body = serde_json::json!({
        "installed": {
            "client_id": id,
            "client_secret": secret,
        }
    });
    let json = serde_json::to_vec_pretty(&body)
        .map_err(|e| Error::Other(format!("client serialize: {e}")))?;
    std::fs::write(&path, &json)?;
    Ok(())
}

/// Returns a valid access token, refreshing if the stored one is expired or
/// near-expired. Errors with NotConnected if there's no stored bundle.
pub async fn access_token() -> Result<String> {
    let mut tokens = load_tokens()?.ok_or(Error::OAuthNotConnected)?;
    let needs_refresh = Utc::now() >= tokens.expires_at - chrono::Duration::seconds(30);
    if needs_refresh {
        let creds = read_client()?;
        tokens = refresh(&creds, &tokens.refresh_token).await?;
        store_tokens(&tokens)?;
    }
    Ok(tokens.access_token)
}

fn read_client() -> Result<ClientCreds> {
    let path = client_file_path();
    if !path.exists() {
        return Err(Error::OAuthClientMissing);
    }
    let bytes = std::fs::read(&path)
        .map_err(|e| Error::Other(format!("read oauth_client.json: {e}")))?;
    let file: ClientFile = serde_json::from_slice(&bytes).map_err(|e| {
        Error::Other(format!(
            "oauth_client.json is not a Desktop client JSON ({e})"
        ))
    })?;
    Ok(file.installed)
}

fn client_file_path() -> std::path::PathBuf {
    crate::library::data_dir().join("oauth_client.json")
}

fn random_state() -> String {
    let mut rng = rand::thread_rng();
    (0..24)
        .map(|_| {
            let n: u8 = rng.gen_range(0..62);
            match n {
                0..=9 => (b'0' + n) as char,
                10..=35 => (b'a' + (n - 10)) as char,
                _ => (b'A' + (n - 36)) as char,
            }
        })
        .collect()
}

/// Accept loopback connections until one carries a `code=` query param.
/// Browsers may probe favicon/etc. - those get a 404 and we keep listening.
async fn wait_for_redirect(listener: TcpListener) -> Result<(String, String)> {
    loop {
        let (mut sock, _) = listener
            .accept()
            .await
            .map_err(|e| Error::Other(format!("loopback accept: {e}")))?;

        let mut reader = BufReader::new(&mut sock);
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).await.is_err() {
            continue;
        }

        // Drain the rest of the request headers so the browser doesn't stall.
        let mut header = String::new();
        loop {
            header.clear();
            match reader.read_line(&mut header).await {
                Ok(0) => break,
                Ok(_) if header == "\r\n" || header == "\n" => break,
                Ok(_) => continue,
                Err(_) => break,
            }
        }

        // request_line is "GET /?code=...&state=... HTTP/1.1\r\n".
        let path = request_line
            .split_whitespace()
            .nth(1)
            .unwrap_or("/")
            .to_string();

        let (code, state, error) = parse_query(&path);

        if let Some(err) = error {
            let body = format!(
                "<h1>Archeio - OAuth error</h1><p>{}</p><p>You can close this window.</p>",
                html_escape(&err)
            );
            let _ = write_http_response(&mut sock, 400, "text/html", &body).await;
            return Err(Error::Other(format!("OAuth error from Google: {err}")));
        }

        match (code, state) {
            (Some(code), Some(state)) => {
                let body =
                    "<h1>Archeio is connected.</h1><p>You can close this window.</p>".to_string();
                let _ = write_http_response(&mut sock, 200, "text/html", &body).await;
                return Ok((code, state));
            }
            _ => {
                let _ = write_http_response(&mut sock, 404, "text/plain", "not found").await;
                continue;
            }
        }
    }
}

fn parse_query(path: &str) -> (Option<String>, Option<String>, Option<String>) {
    let q = match path.split_once('?') {
        Some((_, q)) => q,
        None => return (None, None, None),
    };
    let mut code = None;
    let mut state = None;
    let mut error = None;
    for part in q.split('&') {
        let (k, v) = match part.split_once('=') {
            Some(kv) => kv,
            None => continue,
        };
        let v = urlencoding::decode(v).unwrap_or_default().into_owned();
        match k {
            "code" => code = Some(v),
            "state" => state = Some(v),
            "error" => error = Some(v),
            _ => {}
        }
    }
    (code, state, error)
}

async fn write_http_response(
    sock: &mut tokio::net::TcpStream,
    status: u16,
    content_type: &str,
    body: &str,
) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        _ => "OK",
    };
    let response = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}; charset=utf-8\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n{body}",
        len = body.len(),
    );
    sock.write_all(response.as_bytes()).await?;
    sock.flush().await?;
    Ok(())
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    /// Returned only on the initial authorization_code exchange. On refresh
    /// Google reuses the existing refresh_token, so we keep ours when missing.
    refresh_token: Option<String>,
    expires_in: i64,
}

async fn exchange_code(
    creds: &ClientCreds,
    code: &str,
    redirect_uri: &str,
) -> Result<StoredTokens> {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("code", code),
            ("client_id", creds.client_id.as_str()),
            ("client_secret", creds.client_secret.as_str()),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ])
        .send()
        .await
        .map_err(|e| Error::Other(format!("token exchange: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::Other(format!(
            "token exchange failed ({s}): {body}"
        )));
    }
    let tr: TokenResponse = resp
        .json()
        .await
        .map_err(|e| Error::Other(format!("token decode: {e}")))?;
    let refresh_token = tr.refresh_token.ok_or_else(|| {
        Error::Other(
            "Google did not return a refresh_token. Revoke the app at \
             myaccount.google.com/permissions and try Connect again."
                .into(),
        )
    })?;
    Ok(StoredTokens {
        access_token: tr.access_token,
        refresh_token,
        expires_at: Utc::now() + chrono::Duration::seconds(tr.expires_in),
    })
}

async fn refresh(creds: &ClientCreds, refresh_token: &str) -> Result<StoredTokens> {
    let client = reqwest::Client::new();
    let resp = client
        .post("https://oauth2.googleapis.com/token")
        .form(&[
            ("refresh_token", refresh_token),
            ("client_id", creds.client_id.as_str()),
            ("client_secret", creds.client_secret.as_str()),
            ("grant_type", "refresh_token"),
        ])
        .send()
        .await
        .map_err(|e| Error::Other(format!("token refresh: {e}")))?;

    if !resp.status().is_success() {
        let s = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(Error::Other(format!(
            "token refresh failed ({s}): {body}"
        )));
    }
    let tr: TokenResponse = resp
        .json()
        .await
        .map_err(|e| Error::Other(format!("token decode: {e}")))?;
    Ok(StoredTokens {
        access_token: tr.access_token,
        // Google reuses the existing refresh_token when it doesn't issue a new one.
        refresh_token: tr.refresh_token.unwrap_or_else(|| refresh_token.to_string()),
        expires_at: Utc::now() + chrono::Duration::seconds(tr.expires_in),
    })
}

fn store_tokens(t: &StoredTokens) -> Result<()> {
    let json = serde_json::to_string(t)
        .map_err(|e| Error::Other(format!("token serialize: {e}")))?;
    keyring::Entry::new(SERVICE, TOKEN_KEY_USER)?.set_password(&json)?;
    Ok(())
}

fn load_tokens() -> Result<Option<StoredTokens>> {
    let entry = keyring::Entry::new(SERVICE, TOKEN_KEY_USER)?;
    match entry.get_password() {
        Ok(s) => {
            let t: StoredTokens = serde_json::from_str(&s)
                .map_err(|e| Error::Other(format!("token parse: {e}")))?;
            Ok(Some(t))
        }
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn open_in_browser(url: &str) -> Result<()> {
    // We deliberately bypass tauri-plugin-opener here because that path is
    // bound to a Tauri AppHandle and this fn runs inside the connect command
    // chain where we don't want to thread the handle through.
    //
    // We do NOT use `cmd /C start "" <url>` - cmd's parser treats `&` as a
    // command separator even when it appears inside a URL argument, which
    // truncates the OAuth URL at the first `&` and Google rejects the
    // request with "Required parameter is missing: response_type".
    // `rundll32 url.dll,FileProtocolHandler` invokes the same default-browser
    // handler without going through cmd's parser.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", url])
            .creation_flags(0x0800_0000)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| Error::Other(format!("open browser: {e}")))?;
    }
    #[cfg(not(windows))]
    {
        let _ = url;
        return Err(Error::Other("OAuth flow only supported on Windows".into()));
    }
    Ok(())
}
