//! Google OAuth 2.0 device-code flow; tokens persisted in the user config dir.
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::PathBuf, time::Duration};

const SCOPE: &str = "https://www.googleapis.com/auth/youtube.readonly";
const DEVICE_URL: &str = "https://oauth2.googleapis.com/device/code";
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

fn creds() -> Result<(String, String)> {
    let var = |k| std::env::var(k).with_context(|| format!("set {k} (Google OAuth client)"));
    Ok((var("TERMUSIC_CLIENT_ID")?, var("TERMUSIC_CLIENT_SECRET")?))
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
    std::fs::write(&p, serde_json::to_vec(t)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600))?;
    }
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

/// Run the device flow; `on_code(url, user_code)` tells the user where to authorize.
pub async fn login(on_code: impl Fn(&str, &str)) -> Result<()> {
    let (id, secret) = creds()?;
    let d: Value = reqwest::Client::new()
        .post(DEVICE_URL)
        .form(&[("client_id", id.as_str()), ("scope", SCOPE)])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let field = |k: &str| {
        d[k].as_str()
            .map(str::to_owned)
            .context("bad device response")
    };
    let (device, url, code) = (
        field("device_code")?,
        field("verification_url")?,
        field("user_code")?,
    );
    on_code(&url, &code);
    let mut interval = d["interval"].as_u64().unwrap_or(5);
    let deadline = now() + d["expires_in"].as_u64().unwrap_or(600);
    while now() < deadline {
        tokio::time::sleep(Duration::from_secs(interval)).await;
        let r = post(&[
            ("client_id", &id),
            ("client_secret", &secret),
            ("device_code", &device),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ])
        .await?;
        if let Some(t) = parse(&r, "") {
            return save(&t);
        }
        match r["error"].as_str() {
            Some("authorization_pending") => {}
            Some("slow_down") => interval += 5,
            e => bail!("login failed: {}", e.unwrap_or("unknown")),
        }
    }
    bail!("login timed out")
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
}
