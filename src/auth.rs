//! Google OAuth 2.0 (loopback redirect); tokens persisted in the user config dir.
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::PathBuf, time::Duration};

const SCOPE: &str = "https://www.googleapis.com/auth/youtube.readonly";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    /// Unix seconds.
    pub expires_at: u64,
}

pub fn token_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("termusic").join("tokens.json"))
}

pub fn is_logged_in() -> bool {
    token_path().is_some_and(|p| p.exists())
}

pub fn logout() -> Result<()> {
    if let Some(p) = token_path()
        && p.exists()
    {
        std::fs::remove_file(p)?;
    }
    Ok(())
}

/// Env vars win; otherwise Google's downloaded `client_secret.json` in the config dir.
fn creds() -> Result<(String, String)> {
    if let (Ok(i), Ok(s)) = (
        std::env::var("TERMUSIC_CLIENT_ID"),
        std::env::var("TERMUSIC_CLIENT_SECRET"),
    ) {
        return Ok((i, s));
    }
    let p = token_path()
        .context("no config dir")?
        .with_file_name("client_secret.json");
    let v: Value = serde_json::from_slice(&std::fs::read(&p).with_context(|| {
        format!(
            "set TERMUSIC_CLIENT_ID/SECRET or save Google's client JSON to {}",
            p.display()
        )
    })?)?;
    let c = &v["installed"];
    let f = |k: &str| {
        c[k].as_str()
            .map(str::to_owned)
            .context("bad client_secret.json")
    };
    Ok((f("client_id")?, f("client_secret")?))
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn load() -> Option<Tokens> {
    serde_json::from_slice(&std::fs::read(token_path()?).ok()?).ok()
}

fn save(t: &Tokens) -> Result<()> {
    let p = token_path().context("no config dir")?;
    std::fs::create_dir_all(p.parent().context("bad path")?)?;
    let mut o = std::fs::OpenOptions::new();
    o.write(true).create(true).truncate(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut o, 0o600); // never world-readable, even briefly
    std::io::Write::write_all(&mut o.open(&p)?, &serde_json::to_vec(t)?)?;
    Ok(())
}

/// Build tokens from a token-endpoint response; refresh responses omit the refresh token.
fn parse(r: &Value, old_refresh: &str) -> Option<Tokens> {
    Some(Tokens {
        access_token: r["access_token"].as_str()?.into(),
        refresh_token: r["refresh_token"].as_str().unwrap_or(old_refresh).into(),
        expires_at: now() + r["expires_in"].as_u64().unwrap_or(3600),
    })
}

async fn post(form: &[(&str, &str)]) -> Result<Value> {
    // No error_for_status: Google answers "pending" polls with HTTP 400 + JSON.
    Ok(reqwest::Client::new()
        .post(TOKEN_URL)
        .form(form)
        .send()
        .await?
        .json()
        .await?)
}

/// The `code` from the browser's redirect request line, if this request carries one.
fn code_from_request(req: &str, state: &str) -> Result<Option<String>> {
    let path = req.split_whitespace().nth(1).unwrap_or("/");
    let url = reqwest::Url::parse(&format!("http://localhost{path}"))?;
    let q = |k: &str| {
        url.query_pairs()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.into_owned())
    };
    if let Some(e) = q("error") {
        bail!("login failed: {e}");
    }
    match q("code") {
        Some(c) if q("state").as_deref() == Some(state) => Ok(Some(c)),
        Some(_) => bail!("login failed: state mismatch"),
        None => Ok(None), // e.g. /favicon.ico
    }
}

/// Loopback-redirect flow (Desktop-app clients). `on_url` receives the page to open;
/// we also try to open it in the browser. shortcut: no PKCE, add it if the client secret is ever shipped publicly.
pub async fn login(on_url: impl Fn(&str)) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (id, secret) = creds()?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let redirect = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
    let state = {
        let mut b = [0u8; 16];
        std::io::Read::read_exact(&mut std::fs::File::open("/dev/urandom")?, &mut b)?; // unix-only, like the player
        b.iter().map(|x| format!("{x:02x}")).collect::<String>()
    };
    let url = reqwest::Url::parse_with_params(
        "https://accounts.google.com/o/oauth2/v2/auth",
        [
            ("client_id", id.as_str()),
            ("redirect_uri", &redirect),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("state", &state),
        ],
    )?;
    on_url(url.as_str());
    let _ = std::process::Command::new(if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    })
    .arg(url.as_str())
    .stdout(std::process::Stdio::null())
    .stderr(std::process::Stdio::null())
    .spawn();
    let code = tokio::time::timeout(Duration::from_secs(300), async {
        loop {
            let (mut sock, _) = listener.accept().await?;
            let mut buf = [0u8; 4096];
            let n = sock.read(&mut buf).await?;
            let found = code_from_request(&String::from_utf8_lossy(&buf[..n]), &state);
            let body = if matches!(found, Ok(Some(_))) { "Signed in to termusic. You can close this tab." } else { "termusic" };
            let _ = sock
                .write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\n{body}").as_bytes())
                .await;
            if let Some(c) = found? {
                return Ok::<_, anyhow::Error>(c);
            }
        }
    })
    .await
    .context("login timed out")??;
    let r = post(&[
        ("code", &code),
        ("client_id", &id),
        ("client_secret", &secret),
        ("redirect_uri", &redirect),
        ("grant_type", "authorization_code"),
    ])
    .await?;
    match parse(&r, "") {
        Some(t) => save(&t),
        None => bail!("login failed: {}", r["error"].as_str().unwrap_or("unknown")),
    }
}

/// A valid access token, refreshing (and persisting) it when expired.
pub async fn access_token() -> Result<String> {
    let t = load().context("not logged in")?;
    if t.expires_at > now() + 60 {
        return Ok(t.access_token);
    }
    let (id, secret) = creds()?;
    let r = post(&[
        ("client_id", &id),
        ("client_secret", &secret),
        ("refresh_token", &t.refresh_token),
        ("grant_type", "refresh_token"),
    ])
    .await?;
    if let Some(n) = parse(&r, &t.refresh_token) {
        save(&n)?;
        return Ok(n.access_token);
    }
    if r["error"] == "invalid_grant" {
        logout()?; // revoked/expired for good: force a fresh login
    }
    bail!("session expired, sign in again")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refresh_keeps_old_refresh_token() {
        let r = serde_json::json!({"access_token": "a", "expires_in": 10});
        let t = parse(&r, "old").unwrap();
        assert_eq!(
            (t.access_token.as_str(), t.refresh_token.as_str()),
            ("a", "old")
        );
        assert!(parse(&serde_json::json!({"error": "x"}), "old").is_none());
    }

    #[test]
    fn redirect_parsing() {
        let ok = code_from_request("GET /?state=s&code=4%2Fabc HTTP/1.1", "s").unwrap();
        assert_eq!(ok.as_deref(), Some("4/abc"));
        assert_eq!(
            code_from_request("GET /favicon.ico HTTP/1.1", "s").unwrap(),
            None
        );
        assert!(code_from_request("GET /?code=x&state=bad HTTP/1.1", "s").is_err());
        assert!(code_from_request("GET /?error=access_denied HTTP/1.1", "s").is_err());
    }
}
