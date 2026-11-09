//! Google OAuth 2.0 (loopback redirect); tokens persisted in the user config dir.
use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD as B64URL};
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
    crate::settings::config_dir().map(|d| d.join("tokens.json"))
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
/// The secret is optional (PKCE covers public clients), but Google Desktop clients still require it.
fn creds() -> Result<(String, Option<String>)> {
    if let Some(i) = std::env::var("TERMUSIC_CLIENT_ID")
        .ok()
        .filter(|i| !i.is_empty())
    {
        return Ok((i, std::env::var("TERMUSIC_CLIENT_SECRET").ok()));
    }
    let p = token_path()
        .context("no config dir")?
        .with_file_name("client_secret.json");
    let v: Value = serde_json::from_slice(&std::fs::read(&p).with_context(|| {
        format!(
            "set TERMUSIC_CLIENT_ID (and TERMUSIC_CLIENT_SECRET if your client has one) or save Google's client JSON to {}",
            p.display()
        )
    })?)?;
    creds_from_json(&v)
}

/// `(client_id, client_secret)` from Google's downloaded client JSON (`installed` section).
fn creds_from_json(v: &Value) -> Result<(String, Option<String>)> {
    let s = |k: &str| v["installed"][k].as_str().map(str::to_owned);
    Ok((
        s("client_id")
            .filter(|i| !i.is_empty())
            .context("bad client_secret.json: no installed.client_id")?,
        s("client_secret"),
    ))
}

/// `N` bytes from the OS CSRNG; feeds both the `state` and the PKCE verifier.
fn random_bytes<const N: usize>() -> Result<[u8; N]> {
    let mut b = [0u8; N];
    std::io::Read::read_exact(&mut std::fs::File::open("/dev/urandom")?, &mut b)?; // unix-only, like the player
    Ok(b)
}

/// PKCE code_verifier: 32 random bytes as base64url-no-pad (43 chars, RFC 7636 §4.1).
fn pkce_verifier() -> Result<String> {
    Ok(B64URL.encode(random_bytes::<32>()?))
}

/// PKCE S256 (RFC 7636): base64url-no-pad of the verifier's SHA-256.
fn code_challenge(verifier: &str) -> String {
    use sha2::Digest;
    B64URL.encode(sha2::Sha256::digest(verifier))
}

/// Google consent page for the loopback flow, carrying the CSRF `state` and PKCE challenge.
fn auth_url(id: &str, redirect: &str, state: &str, challenge: &str) -> Result<reqwest::Url> {
    Ok(reqwest::Url::parse_with_params(
        "https://accounts.google.com/o/oauth2/v2/auth",
        [
            ("client_id", id),
            ("redirect_uri", redirect),
            ("response_type", "code"),
            ("scope", SCOPE),
            ("access_type", "offline"),
            ("prompt", "consent"),
            ("state", state),
            ("code_challenge", challenge),
            ("code_challenge_method", "S256"),
        ],
    )?)
}

/// Token-endpoint form: `fields` plus client credentials; the secret only when we have one.
fn token_form<'a>(
    fields: &[(&'a str, &'a str)],
    id: &'a str,
    secret: Option<&'a str>,
) -> Vec<(&'a str, &'a str)> {
    let mut f = fields.to_vec();
    f.push(("client_id", id));
    if let Some(s) = secret.filter(|s| !s.is_empty()) {
        f.push(("client_secret", s));
    }
    f
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
    // No error_for_status: Google reports failures (e.g. invalid_grant) as HTTP 400 + JSON.
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
/// we also try to open it in the browser. Uses PKCE (S256), so a public client needs no secret.
pub async fn login(on_url: impl Fn(&str)) -> Result<()> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (id, secret) = creds()?;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let redirect = format!("http://127.0.0.1:{}", listener.local_addr()?.port());
    let state: String = random_bytes::<16>()?
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect();
    let verifier = pkce_verifier()?;
    let url = auth_url(&id, &redirect, &state, &code_challenge(&verifier))?;
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
    let r = post(&token_form(
        &[
            ("code", &code),
            ("code_verifier", &verifier),
            ("redirect_uri", &redirect),
            ("grant_type", "authorization_code"),
        ],
        &id,
        secret.as_deref(),
    ))
    .await?;
    match parse(&r, "") {
        Some(t) => save(&t),
        None => bail!(
            "login failed: {} {}",
            r["error"].as_str().unwrap_or("unknown"),
            r["error_description"].as_str().unwrap_or("")
        ),
    }
}

/// A valid access token, refreshing (and persisting) it when expired.
pub async fn access_token() -> Result<String> {
    let t = load().context("not logged in")?;
    if t.expires_at > now() + 60 {
        return Ok(t.access_token);
    }
    let (id, secret) = creds()?;
    let r = post(&token_form(
        &[
            ("refresh_token", &t.refresh_token),
            ("grant_type", "refresh_token"),
        ],
        &id,
        secret.as_deref(),
    ))
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

    #[test]
    fn pkce_rfc7636_vector() {
        assert_eq!(
            code_challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn auth_url_has_challenge() {
        let u = auth_url("id", "http://127.0.0.1:1", "st", "ch").unwrap();
        let q: Vec<_> = u
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .collect();
        for (k, v) in [
            ("code_challenge", "ch"),
            ("code_challenge_method", "S256"),
            ("state", "st"),
        ] {
            assert!(q.contains(&(k.into(), v.into())), "{k} missing");
        }
    }

    #[test]
    fn form_secret_optional() {
        let f = token_form(&[("grant_type", "x")], "id", None);
        assert_eq!(f, [("grant_type", "x"), ("client_id", "id")]);
        let f = token_form(&[("grant_type", "x")], "id", Some("s"));
        assert_eq!(f.last(), Some(&("client_secret", "s")));
        // An empty secret (e.g. TERMUSIC_CLIENT_SECRET="") counts as absent.
        assert_eq!(token_form(&[], "id", Some("")), [("client_id", "id")]);
    }

    #[test]
    fn verifier_is_rfc7636_shaped() {
        let (a, b) = (pkce_verifier().unwrap(), pkce_verifier().unwrap());
        assert_eq!(a.len(), 43);
        assert!(
            a.bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        );
        assert_ne!(a, b);
    }

    #[test]
    fn client_json_secret_optional() {
        let j = serde_json::json!({"installed": {"client_id": "i", "client_secret": "s"}});
        assert_eq!(creds_from_json(&j).unwrap(), ("i".into(), Some("s".into())));
        let j = serde_json::json!({"installed": {"client_id": "i"}});
        assert_eq!(creds_from_json(&j).unwrap(), ("i".into(), None));
        assert!(creds_from_json(&serde_json::json!({"web": {"client_id": "i"}})).is_err());
        assert!(creds_from_json(&serde_json::json!({"installed": {"client_id": ""}})).is_err());
    }
}
