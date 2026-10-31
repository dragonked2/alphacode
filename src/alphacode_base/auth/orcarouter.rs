//! OrcaRouter credential seam: pasted API keys and OAuth 2.0 + PKCE login.
//!
//! OrcaRouter is an OpenAI-compatible model gateway. It accepts two credential
//! shapes, and this module owns both of them so the rest of the app only ever
//! deals with "an OrcaRouter key in `orcarouter.env`":
//!
//! - **API-key login** (`orcarouter`): the user pastes an `sk-orca-...` key
//!   from the console.
//! - **Account login** (`orcarouter-oauth`): the browser consent screen at
//!   `https://www.orcarouter.ai/auth` returns a one-time code to a loopback
//!   listener; `POST /api/v1/auth/keys` exchanges it (PKCE, S256, no client
//!   secret) for the same kind of durable key.
//!
//! Two details are easy to get wrong and are encoded here on purpose:
//!
//! 1. Authentication and inference live on *different* origins. The exchange is
//!    `https://www.orcarouter.ai/api/v1/auth/keys`; `api.orcarouter.ai/v1` only
//!    serves inference and 404s on `/auth/keys`. Origins are never derived from
//!    one another — each has its own override.
//! 2. The PKCE grant yields a durable key, **not** a refresh token. There is no
//!    refresh endpoint, so a `401` means "run the connect flow again"; it is
//!    recorded against the exact key generation that was rejected so a late
//!    failure can never mark a freshly re-authorized key as broken.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Canonical provider/profile id shared by both OrcaRouter login choices.
pub const PROFILE_ID: &str = "orcarouter";
/// Where the durable key lives (env var and env file), matching the profile
/// declared in `provider_catalog` so every existing credential path works.
pub const API_KEY_ENV: &str = "ORCAROUTER_API_KEY";
pub const ENV_FILE: &str = "orcarouter.env";
/// Consent + exchange origin. Overridable for self-hosted deployments.
pub const DEFAULT_AUTH_BASE: &str = "https://www.orcarouter.ai";
/// Inference origin (the profile's `api_base`).
pub const DEFAULT_API_BASE: &str = "https://api.orcarouter.ai/v1";
pub const AUTHORIZE_PATH: &str = "/auth";
pub const EXCHANGE_PATH: &str = "/api/v1/auth/keys";
/// Label shown on the consent screen.
pub const APP_NAME: &str = "Alphacode";
/// Requested grant. Anything else is refused before a code is minted.
pub const SCOPE: &str = "api";

/// How long the browser is given to come back with a code before the CLI
/// offers the manual-paste fallback.
pub const CALLBACK_WAIT_SECS: u64 = 180;
/// Only used to render a copyable `callback_url` when no loopback port could be
/// bound; in that case the browser flow is completed by hand anyway.
pub const FALLBACK_CALLBACK_PORT: u16 = 53682;
const EXCHANGE_TIMEOUT_SECS: u64 = 30;
const MAX_MODELS_REAUTH_NOTE: &str = "orcarouter-reauth.json";

fn env_non_empty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Normalize an origin: require HTTPS, or plain HTTP for loopback only (the
/// documented dev exception), and strip trailing slashes.
fn normalize_origin(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let parsed = url::Url::parse(trimmed).ok()?;
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return None;
    }
    match parsed.scheme() {
        "https" => Some(trimmed.to_string()),
        "http" => {
            let host = parsed.host_str()?;
            let loopback = host.eq_ignore_ascii_case("localhost")
                || host == "127.0.0.1"
                || host == "[::1]"
                || host == "::1";
            loopback.then(|| trimmed.to_string())
        }
        _ => None,
    }
}

/// Authentication origin: explicit override, then the shared self-hosted
/// fallback, then the public default. Invalid overrides are ignored (with a
/// warning) instead of silently downgrading to HTTP.
pub fn auth_base() -> String {
    for name in ["ORCA_AUTH_BASE_URL", "ORCA_BASE_URL"] {
        if let Some(raw) = env_non_empty(name) {
            if let Some(origin) = normalize_origin(&raw) {
                return origin;
            }
            crate::alphacode_logging::warn(&format!(
                "Ignoring invalid {}: use https://... (or http://localhost), without credentials or a query.",
                name
            ));
        }
    }
    DEFAULT_AUTH_BASE.to_string()
}

/// Inference origin. Deliberately independent of [`auth_base`]: the public
/// deployment uses two hosts, so neither may be derived from the other.
pub fn api_base() -> String {
    if let Some(raw) = env_non_empty("ORCA_API_BASE_URL") {
        if let Some(origin) = normalize_origin(&raw) {
            return ensure_v1(&origin);
        }
        crate::alphacode_logging::warn(
            "Ignoring invalid ORCA_API_BASE_URL: use https://... (or http://localhost), without credentials or a query.",
        );
    }
    DEFAULT_API_BASE.to_string()
}

fn ensure_v1(origin: &str) -> String {
    if origin.ends_with("/v1") {
        origin.to_string()
    } else {
        format!("{origin}/v1")
    }
}

pub fn exchange_url(auth_base: &str) -> String {
    format!("{}{}", auth_base.trim_end_matches('/'), EXCHANGE_PATH)
}

/// One PKCE attempt. The verifier never leaves the process and must never be
/// logged; [`Debug`] is redacted so an accidental `{:?}` is harmless.
#[derive(Clone, PartialEq, Eq)]
pub struct PkceAttempt {
    pub verifier: String,
    pub challenge: String,
    pub state: String,
}

impl std::fmt::Debug for PkceAttempt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PkceAttempt")
            .field("verifier", &"[redacted]")
            .field("challenge", &self.challenge)
            .field("state", &self.state)
            .finish()
    }
}

/// Fresh verifier, challenge and state for one authorization attempt.
///
/// The verifier is 32 bytes from the cryptographic RNG via the shared OAuth
/// helper, and a new attempt always gets new values — reusing one across
/// attempts is the classic PKCE mistake.
pub fn begin_attempt() -> (PkceAttempt, String) {
    let (verifier, challenge) = crate::alphacode_base::auth::oauth::generate_pkce_public();
    let state = crate::alphacode_base::auth::oauth::generate_state_public();
    (
        PkceAttempt {
            verifier,
            challenge: challenge.clone(),
            state: state.clone(),
        },
        challenge,
    )
}

/// Build the consent URL. Only the S256 challenge and the opaque state travel
/// here; the verifier is presented only to the exchange endpoint.
pub fn authorize_url(auth_base: &str, redirect_uri: &str, attempt: &PkceAttempt) -> String {
    let mut url = url::Url::parse(&format!(
        "{}{}",
        auth_base.trim_end_matches('/'),
        AUTHORIZE_PATH
    ))
    .unwrap_or_else(|_| url::Url::parse("https://www.orcarouter.ai/auth").expect("static URL"));
    url.query_pairs_mut()
        .append_pair("callback_url", redirect_uri)
        .append_pair("code_challenge", &attempt.challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &attempt.state)
        .append_pair("app_name", APP_NAME)
        .append_pair("scope", SCOPE);
    url.to_string()
}

/// The key returned by a successful exchange.
#[derive(Clone, Serialize, Deserialize)]
pub struct IssuedKey {
    pub key: String,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub scope: Option<String>,
}

impl std::fmt::Debug for IssuedKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IssuedKey")
            .field("key", &redact_key(&self.key))
            .field("user_id", &self.user_id)
            .field("scope", &self.scope)
            .finish()
    }
}

#[derive(Serialize)]
struct ExchangeRequest<'a> {
    code: &'a str,
    code_verifier: &'a str,
    code_challenge_method: &'static str,
}

/// Exchange an authorization code for a durable key.
///
/// Presents the verifier and asserts S256 again so a downgraded challenge can
/// never be redeemed silently. Error bodies are not echoed because the server
/// may include credentials, and successful responses are size-limited.
pub async fn exchange_code(
    auth_base: &str,
    attempt: &PkceAttempt,
    code: &str,
) -> Result<IssuedKey> {
    let code = code.trim();
    if code.is_empty() {
        anyhow::bail!("No OrcaRouter authorization code was provided.");
    }

    let client = crate::provider::shared_http_client();
    let mut response = client
        .post(exchange_url(auth_base))
        .header("Content-Type", "application/json")
        .timeout(Duration::from_secs(EXCHANGE_TIMEOUT_SECS))
        .json(&ExchangeRequest {
            code,
            code_verifier: &attempt.verifier,
            code_challenge_method: "S256",
        })
        .send()
        .await
        .context("Failed to reach the OrcaRouter token endpoint")?;

    let status = response.status();
    if !status.is_success() {
        // A server may echo the authorization code or verifier in its error
        // body. Never print an untrusted token endpoint response.
        anyhow::bail!(
            "OrcaRouter authorization exchange failed (HTTP {}). The code may be expired or already used — run the login again.",
            status.as_u16()
        );
    }
    const MAX_EXCHANGE_BODY_BYTES: usize = 64 * 1024;
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .context("Failed to read OrcaRouter authorization response")?
    {
        if body.len().saturating_add(chunk.len()) > MAX_EXCHANGE_BODY_BYTES {
            anyhow::bail!("OrcaRouter authorization response is too large.");
        }
        body.extend_from_slice(&chunk);
    }
    let issued: IssuedKey = serde_json::from_slice(&body)
        .context("OrcaRouter token endpoint returned unexpected JSON")?;
    if issued.key.trim().is_empty() {
        anyhow::bail!("OrcaRouter token endpoint returned an empty key.");
    }
    if let Some(scope) = issued.scope.as_deref()
        && !scope.eq_ignore_ascii_case(SCOPE)
    {
        // A narrower grant may not allow inference; say so instead of failing
        // later with an opaque 403.
        crate::alphacode_logging::warn(&format!(
            "OrcaRouter granted scope '{}' rather than '{}'; some endpoints may be unavailable.",
            scope, SCOPE
        ));
    }
    Ok(issued)
}

/// Accept a full callback URL, a query string, or a bare code from the manual
/// fallback, returning `(code, state)`.
pub fn parse_callback_input(input: &str) -> (String, Option<String>) {
    let trimmed = input.trim();
    if let Ok(url) = url::Url::parse(trimmed) {
        let code = url
            .query_pairs()
            .find(|(key, _)| key == "code")
            .map(|(_, value)| value.to_string())
            .unwrap_or_default();
        let state = url
            .query_pairs()
            .find(|(key, _)| key == "state")
            .map(|(_, value)| value.to_string());
        if !code.is_empty() {
            return (code, state);
        }
    }
    if trimmed.contains("code=") {
        let query = trimmed.trim_start_matches('?');
        let mut code = String::new();
        let mut state = None;
        for pair in query.split('&') {
            if let Some(value) = pair.strip_prefix("code=") {
                code = value.to_string();
            } else if let Some(value) = pair.strip_prefix("state=") {
                state = Some(value.to_string());
            }
        }
        if !code.is_empty() {
            return (code, state);
        }
    }
    (trimmed.to_string(), None)
}

/// Load the stored key (env var wins over the env file, like every other
/// provider in this codebase).
pub fn load_key() -> Option<String> {
    crate::provider_catalog::load_api_key_from_env_or_config(API_KEY_ENV, ENV_FILE)
        .map(|key| key.trim().to_string())
        .filter(|key| !key.is_empty())
}

/// Persist a key through the project's existing secret store (owner-only
/// permissions, `key=value` env file, process env updated in place).
pub fn save_key(key: &str) -> Result<()> {
    let key = key.trim();
    if key.is_empty() {
        anyhow::bail!("No OrcaRouter API key provided.");
    }
    crate::provider_catalog::save_env_value_to_env_file(API_KEY_ENV, ENV_FILE, Some(key))?;
    // A new credential generation is exactly what cancels a pending
    // reauthentication requirement for this provider.
    clear_needs_reauth();
    Ok(())
}

pub fn clear_key() -> Result<()> {
    crate::provider_catalog::save_env_value_to_env_file(API_KEY_ENV, ENV_FILE, None)?;
    clear_needs_reauth();
    Ok(())
}

/// Warn (never reject) on an unexpected prefix: an `sk-orca-` prefix is a
/// shape hint, not proof of validity, and pasted keys may be rotated formats.
pub fn key_prefix_warning(key: &str) -> Option<String> {
    if key.trim_start().starts_with("sk-orca-") {
        None
    } else {
        Some(
            "OrcaRouter keys usually start with 'sk-orca-'. Saving anyway; the first request will confirm whether it works."
                .to_string(),
        )
    }
}

/// Show only a short prefix so logs, errors and the TUI can name the credential
/// without exposing it.
pub fn redact_key(key: &str) -> String {
    let trimmed = key.trim();
    if trimmed.len() <= 12 {
        return "*".repeat(trimmed.len().max(1));
    }
    let head = trimmed.get(..9).unwrap_or("sk-orca-");
    format!("{head}…")
}

/// Stable, non-reversible fingerprint of a key. Used to bind a rejection to one
/// exact credential generation without persisting the secret.
pub fn key_fingerprint(key: &str) -> String {
    use std::hash::{DefaultHasher, Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    key.trim().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ReauthRecord {
    /// Fingerprint of the key that the relay rejected with 401.
    rejected_fingerprint: String,
    #[serde(default)]
    rejected_at_ms: i64,
}

fn reauth_path() -> Result<PathBuf> {
    Ok(crate::storage::alphacode_dir()?.join(MAX_MODELS_REAUTH_NOTE))
}

/// Record that `rejected_key` was refused with 401.
///
/// Generation safety comes from storing the *fingerprint*: once the user
/// re-authorizes, the stored key changes, `needs_reauth()` no longer matches,
/// and a late failure from an older request cannot mark the new key broken.
pub fn mark_needs_reauth(rejected_key: &str) {
    let record = ReauthRecord {
        rejected_fingerprint: key_fingerprint(rejected_key),
        rejected_at_ms: chrono::Utc::now().timestamp_millis(),
    };
    if let Ok(path) = reauth_path() {
        let _ = crate::storage::write_json_secret(&path, &record);
    }
}

/// Whether the *currently stored* credential generation is known-rejected.
pub fn needs_reauth() -> bool {
    let Some(current) = load_key() else {
        return false;
    };
    let Ok(path) = reauth_path() else {
        return false;
    };
    crate::storage::read_json::<ReauthRecord>(&path)
        .map(|record| record.rejected_fingerprint == key_fingerprint(&current))
        .unwrap_or(false)
}

pub fn clear_needs_reauth() {
    if let Ok(path) = reauth_path() {
        let _ = std::fs::remove_file(path);
    }
}

/// True when `api_base` points at OrcaRouter's inference endpoint.
pub fn is_orcarouter_api_base(candidate: &str) -> bool {
    crate::provider_catalog::normalize_api_base(candidate)
        .zip(crate::provider_catalog::normalize_api_base(&api_base()))
        .is_some_and(|(candidate, configured)| candidate == configured)
}

/// Record a 401 from the inference endpoint against the currently stored key.
///
/// OrcaRouter keys do not refresh: a 401 means the key was revoked or expired,
/// so the only recovery is a fresh login. Binding the record to the key that was
/// actually rejected keeps a late failure from an old request from marking a key
/// that has since been replaced. Generation-safe, best-effort, never panics.
pub fn note_unauthorized(api_base: &str, rejected_key: &str) {
    if !is_orcarouter_api_base(api_base) {
        return;
    }
    if load_key().as_deref() != Some(rejected_key.trim()) {
        return;
    }
    if !needs_reauth() {
        crate::alphacode_logging::warn(
            "OrcaRouter rejected the stored key with 401; run /login or `alphacode login --provider orcarouter-oauth`.",
        );
    }
    mark_needs_reauth(rejected_key);
}

/// Human-facing message for the current credential state, if a login is needed.
pub fn reauth_notice() -> Option<&'static str> {
    needs_reauth().then_some(
        "OrcaRouter rejected the saved key (401). Keys do not refresh — run `/login` and choose OrcaRouter - Auth to mint a new one.",
    )
}

#[cfg(test)]
#[path = "orcarouter_tests.rs"]
mod tests;
