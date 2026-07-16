//! ChatGPT OAuth credentials for the Codex Responses backend.
//!
//! The wire flow follows the public Codex CLI implementation: authorization
//! code + PKCE on localhost:1455, with the same device-code fallback used by
//! Codex when a loopback listener is unavailable.

use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, bail};
use base64::Engine as _;
use rand::RngCore as _;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use crate::auth::{AuthChannels, AuthUrlInfo, AuthUrlMode};

pub const AUTH_METHOD_ID: &str = "openai-codex";
pub const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
const ISSUER: &str = "https://auth.openai.com";
const CALLBACK_PORT: u16 = 1455;
const REFRESH_SKEW_SECS: i64 = 300;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Credentials {
    pub access_token: String,
    pub refresh_token: String,
    pub id_token: String,
    pub account_id: Option<String>,
    pub expires_at: i64,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    id_token: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_auth_id: String,
    #[serde(alias = "user_code", alias = "usercode")]
    user_code: String,
    #[serde(deserialize_with = "deserialize_interval")]
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct DeviceTokenResponse {
    authorization_code: String,
    code_verifier: String,
}

fn deserialize_interval<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error as _;
    let value = serde_json::Value::deserialize(deserializer)?;
    match value {
        serde_json::Value::String(s) => s.parse().map_err(D::Error::custom),
        serde_json::Value::Number(n) => n
            .as_u64()
            .ok_or_else(|| D::Error::custom("invalid interval")),
        _ => Err(D::Error::custom("invalid interval")),
    }
}

fn credential_path() -> PathBuf {
    crate::util::grok_home::grok_home().join("openai-auth.json")
}

pub fn is_configured() -> bool {
    load().is_ok()
}

pub fn load() -> anyhow::Result<Credentials> {
    let path = credential_path();
    let bytes = std::fs::read(&path)
        .with_context(|| format!("ChatGPT credentials not found at {}", path.display()))?;
    serde_json::from_slice(&bytes).context("invalid ChatGPT credential file")
}

pub fn logout() -> anyhow::Result<bool> {
    let path = credential_path();
    match std::fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err.into()),
    }
}

pub async fn ensure_fresh() -> anyhow::Result<Credentials> {
    let credentials = load()?;
    if credentials.expires_at > now_secs() + REFRESH_SKEW_SECS {
        return Ok(credentials);
    }
    refresh(credentials).await
}

pub async fn login(channels: AuthChannels, force_device: bool) -> anyhow::Result<Credentials> {
    if force_device {
        device_login(channels).await
    } else {
        match tokio::net::TcpListener::bind(("127.0.0.1", CALLBACK_PORT)).await {
            Ok(listener) => browser_login(listener, channels).await,
            Err(err) => {
                tracing::info!(error = %err, "ChatGPT OAuth loopback unavailable; using device code");
                device_login(channels).await
            }
        }
    }
}

async fn browser_login(
    listener: tokio::net::TcpListener,
    mut channels: AuthChannels,
) -> anyhow::Result<Credentials> {
    let verifier = random_urlsafe(32);
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    let state = random_urlsafe(32);
    let redirect_uri = format!("http://localhost:{CALLBACK_PORT}/auth/callback");
    let auth_url = authorize_url(&redirect_uri, &challenge, &state);

    if let Some(tx) = channels.url_tx.take() {
        let _ = tx.send(AuthUrlInfo {
            url: auth_url.clone(),
            mode: AuthUrlMode::Loopback,
        });
    }
    if let Err(err) = webbrowser::open(&auth_url) {
        tracing::debug!(error = %err, "failed to open ChatGPT OAuth URL");
    }

    let callback = tokio::time::timeout(Duration::from_secs(15 * 60), async {
        tokio::select! {
            result = receive_loopback(listener) => result,
            pasted = channels.code_rx.recv() => {
                pasted.ok_or_else(|| anyhow::anyhow!("ChatGPT login was cancelled"))
            }
        }
    })
    .await
    .context("ChatGPT login timed out")??;

    let (code, received_state) = parse_callback(&callback)?;
    if let Some(received_state) = received_state
        && received_state != state
    {
        bail!("ChatGPT login state mismatch");
    }
    exchange_code(&code, &redirect_uri, &verifier).await
}

async fn receive_loopback(listener: tokio::net::TcpListener) -> anyhow::Result<String> {
    let (mut stream, _) = listener.accept().await?;
    let mut buffer = vec![0u8; 16 * 1024];
    let read = stream.read(&mut buffer).await?;
    let request = std::str::from_utf8(&buffer[..read]).context("invalid OAuth callback")?;
    let target = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .context("invalid OAuth callback request")?;
    let callback = format!("http://localhost:{CALLBACK_PORT}{target}");
    let body = "ChatGPT login complete. You can close this window.";
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    Ok(callback)
}

async fn device_login(mut channels: AuthChannels) -> anyhow::Result<Credentials> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{ISSUER}/api/accounts/deviceauth/usercode"))
        .json(&serde_json::json!({ "client_id": CLIENT_ID }))
        .send()
        .await?
        .error_for_status()?
        .json::<DeviceCodeResponse>()
        .await?;
    let display_url = format!(
        "{ISSUER}/codex/device?user_code={}",
        urlencoding::encode(&response.user_code)
    );
    if let Some(tx) = channels.url_tx.take() {
        let _ = tx.send(AuthUrlInfo {
            url: display_url.clone(),
            mode: AuthUrlMode::Device,
        });
    }
    if let Err(err) = webbrowser::open(&display_url) {
        tracing::debug!(error = %err, "failed to open ChatGPT device URL");
    }

    let poll_url = format!("{ISSUER}/api/accounts/deviceauth/token");
    let started = tokio::time::Instant::now();
    let device_tokens = loop {
        let poll = client
            .post(&poll_url)
            .json(&serde_json::json!({
                "device_auth_id": response.device_auth_id,
                "user_code": response.user_code,
            }))
            .send()
            .await?;
        if poll.status().is_success() {
            break poll.json::<DeviceTokenResponse>().await?;
        }
        if !matches!(poll.status().as_u16(), 403 | 404) {
            bail!("ChatGPT device login failed with status {}", poll.status());
        }
        if started.elapsed() >= Duration::from_secs(15 * 60) {
            bail!("ChatGPT device login timed out");
        }
        tokio::time::sleep(Duration::from_secs(response.interval.max(1))).await;
    };

    exchange_code(
        &device_tokens.authorization_code,
        &format!("{ISSUER}/deviceauth/callback"),
        &device_tokens.code_verifier,
    )
    .await
}

async fn exchange_code(
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> anyhow::Result<Credentials> {
    let response = reqwest::Client::new()
        .post(format!("{ISSUER}/oauth/token"))
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("client_id", CLIENT_ID),
            ("code_verifier", verifier),
        ])
        .send()
        .await?
        .error_for_status()?
        .json::<TokenResponse>()
        .await?;
    let credentials = credentials_from_response(response, None)?;
    save(&credentials)?;
    Ok(credentials)
}

async fn refresh(current: Credentials) -> anyhow::Result<Credentials> {
    let response = reqwest::Client::new()
        .post(format!("{ISSUER}/oauth/token"))
        .json(&serde_json::json!({
            "client_id": CLIENT_ID,
            "grant_type": "refresh_token",
            "refresh_token": current.refresh_token,
        }))
        .send()
        .await?
        .error_for_status()?
        .json::<TokenResponse>()
        .await?;
    let credentials = credentials_from_response(response, Some(current))?;
    save(&credentials)?;
    Ok(credentials)
}

fn credentials_from_response(
    response: TokenResponse,
    previous: Option<Credentials>,
) -> anyhow::Result<Credentials> {
    let access_token = response
        .access_token
        .or_else(|| previous.as_ref().map(|p| p.access_token.clone()))
        .context("OAuth response did not include an access token")?;
    let refresh_token = response
        .refresh_token
        .or_else(|| previous.as_ref().map(|p| p.refresh_token.clone()))
        .context("OAuth response did not include a refresh token")?;
    let id_token = response
        .id_token
        .or_else(|| previous.as_ref().map(|p| p.id_token.clone()))
        .unwrap_or_default();
    let account_id = jwt_nested_claim(
        &id_token,
        "https://api.openai.com/auth",
        "chatgpt_account_id",
    )
    .or_else(|| {
        jwt_nested_claim(
            &access_token,
            "https://api.openai.com/auth",
            "chatgpt_account_id",
        )
    })
    // Keep accepting the older flat claim shape used by test issuers and
    // some existing Codex credentials.
    .or_else(|| jwt_claim(&id_token, "chatgpt_account_id"))
    .or_else(|| jwt_claim(&access_token, "chatgpt_account_id"))
    .or_else(|| previous.and_then(|p| p.account_id));
    let expires_at = jwt_claim_i64(&access_token, "exp").unwrap_or_else(|| now_secs() + 3600);
    Ok(Credentials {
        access_token,
        refresh_token,
        id_token,
        account_id,
        expires_at,
    })
}

fn save(credentials: &Credentials) -> anyhow::Result<()> {
    let path = credential_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec_pretty(credentials)?;
    let tmp = path.with_extension("json.tmp");
    let mut options = std::fs::OpenOptions::new();
    options.create(true).truncate(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    use std::io::Write as _;
    let mut file = options.open(&tmp)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(&bytes)?;
    file.sync_all()?;
    std::fs::rename(tmp, path)?;
    Ok(())
}

fn authorize_url(redirect_uri: &str, challenge: &str, state: &str) -> String {
    let mut url = url::Url::parse(&format!("{ISSUER}/oauth/authorize")).expect("static OAuth URL");
    url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", CLIENT_ID)
        .append_pair("redirect_uri", redirect_uri)
        .append_pair(
            "scope",
            "openid profile email offline_access api.connectors.read api.connectors.invoke",
        )
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("state", state)
        .append_pair("originator", "oh-my-grok");
    url.into()
}

fn parse_callback(input: &str) -> anyhow::Result<(String, Option<String>)> {
    let input = input.trim();
    if !input.contains("://") {
        return Ok((input.to_string(), None));
    }
    let url = url::Url::parse(input).context("invalid OAuth callback URL")?;
    let code = url
        .query_pairs()
        .find_map(|(key, value)| (key == "code").then(|| value.into_owned()))
        .context("OAuth callback did not include a code")?;
    let state = url
        .query_pairs()
        .find_map(|(key, value)| (key == "state").then(|| value.into_owned()));
    Ok((code, state))
}

fn jwt_claim(token: &str, name: &str) -> Option<String> {
    jwt_payload(token)?.get(name)?.as_str().map(str::to_string)
}

fn jwt_nested_claim(token: &str, namespace: &str, name: &str) -> Option<String> {
    jwt_payload(token)?
        .get(namespace)?
        .get(name)?
        .as_str()
        .map(str::to_string)
}

fn jwt_claim_i64(token: &str, name: &str) -> Option<i64> {
    jwt_payload(token)?.get(name)?.as_i64()
}

fn jwt_payload(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice(&decoded).ok()
}

fn random_urlsafe(bytes: usize) -> String {
    let mut data = vec![0u8; bytes];
    rand::rng().fill_bytes(&mut data);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_redirect_and_bare_code() {
        assert_eq!(
            parse_callback("http://localhost:1455/auth/callback?code=abc&state=xyz").unwrap(),
            ("abc".to_string(), Some("xyz".to_string()))
        );
        assert_eq!(parse_callback("abc").unwrap(), ("abc".to_string(), None));
    }

    #[test]
    fn authorize_url_uses_codex_pkce_contract() {
        let url = authorize_url("http://localhost:1455/auth/callback", "challenge", "state");
        assert!(url.contains("client_id=app_EMoamEEZ73f0CkXaXp7hrann"));
        assert!(url.contains("code_challenge=challenge"));
        assert!(url.contains("originator=oh-my-grok"));
    }
}
