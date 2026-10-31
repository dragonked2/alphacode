//! ZCode (Z.ai coding plan) OAuth authentication.
//!
//! Mirrors the login flow used by the ZCode desktop CLI:
//!
//! 1. `POST https://zcode.z.ai/api/v1/oauth/cli/init` with a random hex poll
//!    token and body `{"provider":"zai"}` returns `{flow_id, authorize_url,
//!    expires_at, poll_interval_sec}`.
//! 2. The user opens `authorize_url` in a browser and signs in with their
//!    Z.ai / BigModel account.
//! 3. `GET https://zcode.z.ai/api/v1/oauth/cli/poll/{flow_id}` (same poll
//!    token) flips from `pending` to `ready`, carrying the long-lived Z.ai
//!    OAuth access token plus user info.
//!
//! Requests then follow ZCode's coding-plan path: the OAuth access token
//! authorizes the `api.z.ai` business endpoints, which resolve the
//! `zcode-api-key` project API key and mint a short-lived (5h) project access
//! token used as the `x-api-key` credential for the Anthropic-compatible
//! `https://api.z.ai/api/anthropic/v1/messages` endpoint.
//!
//! There is no refresh endpoint: ZCode itself re-runs the CLI login when the
//! OAuth access token stops working, so `ManualRelogin` is the honest refresh
//! story. All endpoints below were validated against the live services.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::sync::OnceLock;
use tokio::sync::RwLock;

/// Base URL for the ZCode CLI OAuth API.
const ZCODE_OAUTH_API_BASE: &str = "https://zcode.z.ai/api/v1";

/// Z.ai business API origin (org/project/API-key management + chat).
pub const ZAI_API_BASE: &str = "https://api.z.ai";

/// Anthropic-compatible Messages API base for the Z.ai coding plan. The full
/// endpoint is `{base}/v1/messages`, matching ZCode's own requests.
pub const ZCODE_ANTHROPIC_BASE_URL: &str = "https://api.z.ai/api/anthropic";

/// Exchanges the CLI-issued OAuth access token for a long-lived (30d) Z.ai
/// business session token. The OAuth token is *not* accepted by the business
/// API directly — ZCode's CLI performs this exact exchange first.
const ZAI_BUSINESS_LOGIN_URL: &str = "https://api.z.ai/api/auth/z/login";

/// OAuth provider id sent to the init endpoint; `zai` is the Z.ai family.
const OAUTH_PROVIDER_ID: &str = "zai";

/// Client identity sent when minting project access tokens, matching ZCode.
const CLIENT_TYPE: &str = "zcode";
const CLIENT_VERSION: &str = "1.0.0";

/// Name of the project API key ZCode creates/uses for coding-plan requests.
const ZCODE_API_KEY_NAME: &str = "zcode-api-key";

/// `usageScene` for the coding plan when creating the project API key.
const USAGE_SCENE_CODING_PLAN: i64 = 1;

/// Poll cadence fallback when the init response omits `poll_interval_sec`.
const DEFAULT_POLL_INTERVAL_SECS: u64 = 3;

/// Upper bound for the whole browser-polling login (ZCode allows 5 minutes).
const LOGIN_TIMEOUT_SECS: u64 = 300;

/// Refresh the cached project access token this many seconds before expiry.
const PROJECT_TOKEN_EXPIRY_BUFFER_SECS: i64 = 120;

/// Anonymous identity for the OAuth request signing key material. The ZCode
/// CLI derives a poll token from local randomness; we do the same.
fn new_poll_token() -> String {
    use rand::RngCore;
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------
// OAuth CLI flow types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZcodeUserInfo {
    pub user_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

/// Persisted Z.ai OAuth credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZcodeCredentials {
    /// Long-lived Z.ai OAuth access token (bearer for the business API).
    pub access_token: String,
    /// Refresh token when the server supplied one; currently unused because
    /// the service exposes no refresh endpoint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    /// ZCode session JWT (`zcodejwttoken`); kept for parity with ZCode's
    /// credential store but not required for coding-plan requests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zcode_token: Option<String>,
    pub user: ZcodeUserInfo,
    /// Epoch seconds when the file was written.
    pub saved_at: i64,
}

/// `POST /oauth/cli/init` response payload.
#[derive(Debug, Clone)]
pub struct ZcodeLoginInit {
    pub flow_id: String,
    pub authorize_url: String,
    pub expires_at: i64,
    pub poll_interval_sec: u64,
}

/// Result of one `GET /oauth/cli/poll/{flow_id}` call.
pub enum ZcodeLoginPoll {
    Pending,
    Failed(String),
    Ready(Box<ZcodeCredentials>),
}

/// Standard ZCode/Z.ai JSON envelope: `{"code": 0, "msg": ..., "data": ...}`.
#[derive(Debug, Deserialize)]
struct Envelope<T> {
    pub code: i64,
    #[serde(default)]
    pub msg: Option<String>,
    #[serde(default)]
    pub success: Option<bool>,
    pub data: Option<T>,
}

/// Typed marker for a business-envelope 401 so callers can distinguish
/// "credential rejected, re-exchange and retry" from transport failures.
/// The business endpoints answer auth failures with HTTP 200 + `code: 401`.
#[derive(Debug)]
struct BusinessAuthError;

impl std::fmt::Display for BusinessAuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Z.ai rejected the credential")
    }
}

impl std::error::Error for BusinessAuthError {}

fn is_business_auth_error(error: &anyhow::Error) -> bool {
    error
        .chain()
        .any(|cause| cause.downcast_ref::<BusinessAuthError>().is_some())
}

fn parse_envelope<T: for<'de> Deserialize<'de>>(body: &str) -> Result<T> {
    let parsed: Envelope<T> = serde_json::from_str(body).context("invalid response envelope")?;
    // The business endpoints observed in the wild use both `code: 0` and
    // `code: 200` for success, occasionally paired with `success: true`.
    let ok = parsed.code == 0 || parsed.code == 200 || parsed.success == Some(true);
    if !ok {
        let message = parsed.msg.unwrap_or_else(|| "unknown error".to_string());
        if parsed.code == 401 {
            return Err(anyhow::Error::new(BusinessAuthError).context(message));
        }
        bail!("Z.ai business error {}: {}", parsed.code, message);
    }
    parsed
        .data
        .ok_or_else(|| anyhow::anyhow!("missing response data"))
}

async fn read_body(response: reqwest::Response) -> Result<String> {
    let status = response.status();
    let text = response
        .text()
        .await
        .with_context(|| format!("failed to read response body (HTTP {status})"))?;
    if !status.is_success() {
        bail!("HTTP {status}: {}", truncate(&text, 300));
    }
    Ok(text)
}

fn truncate(value: &str, max: usize) -> Cow<'_, str> {
    if value.len() <= max {
        Cow::Borrowed(value)
    } else {
        Cow::Owned(format!("{}...", &value[..max]))
    }
}

/// Start the CLI OAuth flow. Returns the init payload (containing the
/// `authorize_url` to open in a browser) plus the poll token that must be
/// reused for every subsequent poll call, exactly like ZCode.
pub async fn initiate_cli_login(client: &reqwest::Client) -> Result<(ZcodeLoginInit, String)> {
    let poll_token = new_poll_token();
    let response = client
        .post(format!("{ZCODE_OAUTH_API_BASE}/oauth/cli/init"))
        .bearer_auth(&poll_token)
        .json(&serde_json::json!({ "provider": OAUTH_PROVIDER_ID }))
        .send()
        .await
        .context("failed to reach zcode.z.ai OAuth init endpoint")?;
    let body = read_body(response).await?;
    #[derive(Deserialize)]
    struct InitData {
        flow_id: String,
        authorize_url: String,
        #[allow(dead_code)]
        expires_at: i64,
        #[serde(default = "default_poll_interval")]
        poll_interval_sec: u64,
    }
    fn default_poll_interval() -> u64 {
        DEFAULT_POLL_INTERVAL_SECS
    }
    let data: InitData = parse_envelope(&body)?;
    if data.flow_id.trim().is_empty() || data.authorize_url.trim().is_empty() {
        bail!("OAuth init response is missing flow_id or authorize_url");
    }
    let interval = if data.poll_interval_sec == 0 {
        DEFAULT_POLL_INTERVAL_SECS
    } else {
        data.poll_interval_sec
    };
    Ok((
        ZcodeLoginInit {
            flow_id: data.flow_id,
            authorize_url: data.authorize_url,
            expires_at: data.expires_at,
            poll_interval_sec: interval,
        },
        poll_token,
    ))
}

/// Poll the CLI OAuth flow once.
pub async fn poll_cli_login(
    client: &reqwest::Client,
    poll_token: &str,
    flow_id: &str,
) -> Result<ZcodeLoginPoll> {
    let flow_id = urlencoding::encode(flow_id);
    let response = client
        .get(format!("{ZCODE_OAUTH_API_BASE}/oauth/cli/poll/{flow_id}"))
        .bearer_auth(poll_token)
        .send()
        .await
        .context("failed to poll zcode.z.ai OAuth endpoint")?;
    let body = read_body(response).await?;
    #[derive(Deserialize)]
    struct PollData {
        status: String,
        #[serde(default)]
        token: Option<String>,
        #[serde(default)]
        user: Option<ZcodeUserInfo>,
        #[serde(default)]
        zai: Option<TokenPair>,
    }
    #[derive(Deserialize, Default)]
    struct TokenPair {
        #[serde(default, alias = "accessToken")]
        access_token: Option<String>,
        #[serde(default, alias = "refreshToken")]
        refresh_token: Option<String>,
    }
    let data: PollData = parse_envelope(&body)?;
    match data.status.as_str() {
        "pending" => Ok(ZcodeLoginPoll::Pending),
        "failed" => Ok(ZcodeLoginPoll::Failed(
            "authorization was denied or expired".to_string(),
        )),
        "ready" => {
            let pair = data.zai.unwrap_or_default();
            let access_token = pair
                .access_token
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("ready response is missing the Z.ai access token")
                })?;
            let user = data
                .user
                .filter(|user| !user.user_id.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("ready response is missing user info"))?;
            Ok(ZcodeLoginPoll::Ready(Box::new(ZcodeCredentials {
                access_token,
                refresh_token: pair.refresh_token.filter(|value| !value.trim().is_empty()),
                zcode_token: data.token.filter(|value| !value.trim().is_empty()),
                user,
                saved_at: chrono::Utc::now().timestamp(),
            })))
        }
        other => bail!("unexpected OAuth poll status: {other}"),
    }
}

/// Drive the full browser login: init, then poll until ready/failed/timeout.
/// This is the function the CLI/TUI login flows call after printing the URL.
pub async fn wait_for_cli_login(
    client: &reqwest::Client,
    init: &ZcodeLoginInit,
    poll_token: &str,
) -> Result<ZcodeCredentials> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(LOGIN_TIMEOUT_SECS);
    loop {
        match poll_cli_login(client, poll_token, &init.flow_id).await? {
            ZcodeLoginPoll::Pending => {}
            ZcodeLoginPoll::Failed(reason) => bail!("ZCode login failed: {reason}"),
            ZcodeLoginPoll::Ready(credentials) => return Ok(*credentials),
        }
        if std::time::Instant::now() >= deadline {
            bail!("ZCode login timed out after {LOGIN_TIMEOUT_SECS}s");
        }
        tokio::time::sleep(std::time::Duration::from_secs(
            init.poll_interval_sec.min(10),
        ))
        .await;
    }
}

// ---------------------------------------------------------------------------
// Credential persistence
// ---------------------------------------------------------------------------

pub fn credentials_path() -> Result<std::path::PathBuf> {
    Ok(crate::storage::alphacode_dir()?.join("zcode_oauth.json"))
}

pub fn save_credentials(credentials: &ZcodeCredentials) -> Result<()> {
    let path = credentials_path()?;
    crate::storage::write_json_secret(&path, credentials)?;
    super::token_cache::cache().invalidate(&path);
    super::AuthStatus::invalidate_cache();
    Ok(())
}

pub fn load_credentials() -> Result<ZcodeCredentials> {
    let path = credentials_path()?;
    if !path.exists() {
        bail!("No ZCode login found. Run `alphacode login --provider zcode`.");
    }
    super::token_cache::load_cached(&path, || {
        crate::storage::harden_secret_file_permissions(&path);
        crate::storage::read_json(&path)
            .with_context(|| format!("Failed to read {}", path.display()))
    })
}

pub fn clear_credentials() -> Result<()> {
    let path = credentials_path()?;
    if path.exists() {
        std::fs::remove_file(&path)
            .with_context(|| format!("Failed to remove {}", path.display()))?;
    }
    super::token_cache::cache().invalidate(&path);
    clear_cached_project_token();
    clear_cached_business_token();
    super::AuthStatus::invalidate_cache();
    Ok(())
}

pub fn has_credentials() -> bool {
    credentials_path()
        .map(|path| path.exists())
        .unwrap_or(false)
}

/// Fast presence probe for hot paths (no parse, no network).
pub fn has_credentials_fast() -> bool {
    has_credentials()
}

// ---------------------------------------------------------------------------
// Project access token chain (the coding-plan credential)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Organization {
    #[serde(default, alias = "organizationId")]
    organization_id: Option<String>,
    #[serde(default, alias = "organizationName")]
    organization_name: Option<String>,
    #[serde(default)]
    projects: Vec<Project>,
}

#[derive(Debug, Deserialize)]
struct Project {
    project_id: String,
    #[serde(default, alias = "projectName")]
    project_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ApiKeyEntry {
    #[serde(default)]
    name: Option<String>,
    #[serde(default, alias = "apiKey")]
    api_key_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProjectAccessToken {
    #[serde(default)]
    enabled: Option<bool>,
    #[serde(default, alias = "accessToken")]
    access_token: Option<String>,
    #[serde(default, alias = "expiresIn")]
    expires_in_secs: Option<i64>,
    #[serde(default, alias = "expiresAt")]
    expires_at: Option<i64>,
}

/// Resolve the default organization and project, mirroring ZCode's selection:
/// the org/project named "默认机构"/"默认项目" (default), else the first with
/// any project.
fn pick_default_org_project(organizations: &[Organization]) -> Option<(&Organization, &Project)> {
    let org = organizations
        .iter()
        .filter(|org| !org.projects.is_empty())
        .find(|org| org.is_default())
        .or_else(|| organizations.iter().find(|org| !org.projects.is_empty()))?;
    let project = org
        .projects
        .iter()
        .find(|project| project.is_default())
        .or_else(|| org.projects.first())?;
    Some((org, project))
}

impl Organization {
    fn is_default(&self) -> bool {
        self.organization_name
            .as_deref()
            .is_some_and(|name| name.contains("默认"))
    }
}

impl Project {
    fn is_default(&self) -> bool {
        self.project_name
            .as_deref()
            .is_some_and(|name| name.contains("默认") || name.eq_ignore_ascii_case("default"))
    }
}

#[derive(Debug, Clone)]
struct CachedProjectToken {
    token: String,
    expires_at: i64,
}

fn project_token_cache() -> &'static RwLock<Option<CachedProjectToken>> {
    static CACHE: OnceLock<RwLock<Option<CachedProjectToken>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(None))
}

fn clear_cached_project_token() {
    if let Ok(mut guard) = project_token_cache().try_write() {
        *guard = None;
    }
}

/// Decode the `exp` claim of a JWT without verifying the signature (we only
/// need the expiry locally; the server validates the real signature).
fn jwt_expiry_seconds(token: &str) -> Option<i64> {
    use base64::Engine as _;
    let payload = token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE
        .decode(payload)
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&decoded).ok()?;
    value.get("exp").and_then(serde_json::Value::as_i64)
}

#[derive(Debug, Clone)]
struct CachedBusinessToken {
    token: String,
    expires_at: i64,
}

fn business_token_cache() -> &'static RwLock<Option<CachedBusinessToken>> {
    static CACHE: OnceLock<RwLock<Option<CachedBusinessToken>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(None))
}

fn clear_cached_business_token() {
    if let Ok(mut guard) = business_token_cache().try_write() {
        *guard = None;
    }
}

/// Exchange the stored OAuth access token for the Z.ai business session
/// token accepted by the `api/biz` endpoints. The OAuth token issued by the
/// CLI login is rejected by those endpoints directly (envelope 401 "token
/// expired or incorrect"); ZCode's CLI performs this exact exchange first.
/// The business token lives ~30 days, so it is cached until shortly before
/// expiry (or until a 401 forces a re-exchange).
async fn business_access_token(client: &reqwest::Client, oauth_token: &str) -> Result<String> {
    let now = chrono::Utc::now().timestamp();
    {
        let guard = business_token_cache().read().await;
        if let Some(cached) = guard.as_ref()
            && cached.expires_at > now + PROJECT_TOKEN_EXPIRY_BUFFER_SECS
        {
            return Ok(cached.token.clone());
        }
    }

    // Detect an expired OAuth token up front for an actionable message: the
    // CLI-issued token carries a short (~1h) `exp` claim.
    if let Some(exp) = jwt_expiry_seconds(oauth_token)
        && exp <= now
    {
        bail!(
            "ZCode OAuth login expired. Run /login (or `alphacode login --provider zcode`) to sign in again."
        );
    }

    #[derive(Deserialize)]
    struct BusinessLoginData {
        #[serde(default, alias = "accessToken")]
        access_token: Option<String>,
        #[serde(default)]
        expires_in: Option<i64>,
    }
    let response = client
        .post(ZAI_BUSINESS_LOGIN_URL)
        .json(&serde_json::json!({ "token": oauth_token }))
        .send()
        .await
        .context("failed to reach the Z.ai business login endpoint")?;
    let data: BusinessLoginData = parse_envelope(&read_body(response).await?).map_err(|error| {
        if is_business_auth_error(&error) {
            anyhow::Error::new(BusinessAuthError)
                .context("ZCode OAuth login was rejected; run /login to sign in again")
        } else {
            error
        }
    })?;
    let token = data
        .access_token
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("business login response missing access_token"))?;
    let expires_in = data.expires_in.unwrap_or(3600).max(60);
    *business_token_cache().write().await = Some(CachedBusinessToken {
        token: token.clone(),
        expires_at: now + expires_in,
    });
    Ok(token)
}

/// Resolve the org/project and the `zcode-api-key` API key id, creating the
/// key when absent, then mint a fresh project access token. Returns the token
/// and its epoch-seconds expiry.
async fn mint_project_access_token(
    client: &reqwest::Client,
    oauth_token: &str,
) -> Result<(String, i64)> {
    match mint_project_access_token_inner(client, oauth_token).await {
        Ok(result) => Ok(result),
        // A business 401 means the cached session token was rejected
        // (revoked server-side); re-exchange once before giving up.
        Err(error) if is_business_auth_error(&error) => {
            clear_cached_business_token();
            mint_project_access_token_inner(client, oauth_token).await
        }
        Err(error) => Err(error),
    }
}

async fn mint_project_access_token_inner(
    client: &reqwest::Client,
    oauth_token: &str,
) -> Result<(String, i64)> {
    let access_token = business_access_token(client, oauth_token).await?;
    let bearer = format!("Bearer {access_token}");

    // 1. Resolve organizations/projects.
    let response = client
        .get(format!("{ZAI_API_BASE}/api/biz/customer/getCustomerInfo"))
        .header("Authorization", &bearer)
        .send()
        .await
        .context("failed to fetch Z.ai customer info")?;
    #[derive(Deserialize)]
    struct CustomerInfo {
        #[serde(default)]
        organizations: Vec<Organization>,
    }
    let info: CustomerInfo = parse_envelope(&read_body(response).await?)?;
    let (org, project) = pick_default_org_project(&info.organizations)
        .ok_or_else(|| anyhow::anyhow!("no Z.ai organization/project found for this account"))?;
    let organization_id = org
        .organization_id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("Z.ai organization response missing organizationId"))?;

    let keys_url = format!(
        "{ZAI_API_BASE}/api/biz/v1/organization/{}/projects/{}/api_keys",
        urlencoding::encode(organization_id),
        urlencoding::encode(&project.project_id),
    );

    // 2. Find (or create) the zcode coding-plan API key.
    let response = client
        .get(&keys_url)
        .header("Authorization", &bearer)
        .send()
        .await
        .context("failed to list Z.ai project API keys")?;
    let keys: Vec<ApiKeyEntry> = parse_envelope(&read_body(response).await?).unwrap_or_default();
    let api_key_id = keys
        .iter()
        .find(|entry| entry.name.as_deref() == Some(ZCODE_API_KEY_NAME))
        .and_then(|entry| entry.api_key_id.clone())
        .filter(|id| !id.trim().is_empty());
    let api_key_id = match api_key_id {
        Some(id) => id,
        None => {
            let response = client
                .post(&keys_url)
                .header("Authorization", &bearer)
                .json(&serde_json::json!({
                    "name": ZCODE_API_KEY_NAME,
                    "usageScene": USAGE_SCENE_CODING_PLAN,
                }))
                .send()
                .await
                .context("failed to create the zcode-api-key project key")?;
            let entry: ApiKeyEntry = parse_envelope(&read_body(response).await?)?;
            entry
                .api_key_id
                .filter(|id| !id.trim().is_empty())
                .ok_or_else(|| anyhow::anyhow!("created API key response missing apiKey id"))?
        }
    };

    // 3. Mint the short-lived project access token.
    let response = client
        .post(format!("{keys_url}/{api_key_id}/access_tokens"))
        .header("Authorization", &bearer)
        .json(&serde_json::json!({
            "clientType": CLIENT_TYPE,
            "clientVersion": CLIENT_VERSION,
        }))
        .send()
        .await
        .context("failed to mint Z.ai project access token")?;
    let data: ProjectAccessToken = parse_envelope(&read_body(response).await?)?;
    if data.enabled == Some(false) {
        bail!("Z.ai project access token is disabled for this API key");
    }
    let token = data
        .access_token
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow::anyhow!("project access token response missing token"))?;
    let now = chrono::Utc::now().timestamp();
    let expires_at = data
        .expires_at
        .filter(|value| *value > now)
        .or_else(|| jwt_expiry_seconds(&token))
        .or_else(|| data.expires_in_secs.map(|secs| now + secs))
        .unwrap_or(now);
    Ok((token, expires_at))
}

/// Get a usable project access token for coding-plan requests, minting (or
/// re-minting) as needed. Cached process-wide until shortly before expiry.
pub async fn project_access_token(client: &reqwest::Client) -> Result<String> {
    let now = chrono::Utc::now().timestamp();
    {
        let guard = project_token_cache().read().await;
        if let Some(cached) = guard.as_ref()
            && cached.expires_at > now + PROJECT_TOKEN_EXPIRY_BUFFER_SECS
        {
            return Ok(cached.token.clone());
        }
    }
    let credentials = load_credentials()?;
    let (token, expires_at) = mint_project_access_token(client, &credentials.access_token).await?;
    *project_token_cache().write().await = Some(CachedProjectToken {
        token: token.clone(),
        expires_at,
    });
    Ok(token)
}

/// Invalidate the cached project token so the next request mints a fresh one.
/// Called by the runtime after a 401.
pub async fn invalidate_project_token() {
    *project_token_cache().write().await = None;
}

/// Convenience wrapper for probing whether the stored OAuth login can still
/// drive the business API (used by doctor/auth-status surfaces). Goes through
/// the business-token exchange, matching the request path.
pub async fn verify_credentials_live(client: &reqwest::Client) -> Result<()> {
    let credentials = load_credentials()?;
    let access_token = business_access_token(client, &credentials.access_token).await?;
    let response = client
        .get(format!("{ZAI_API_BASE}/api/biz/customer/getCustomerInfo"))
        .bearer_auth(&access_token)
        .send()
        .await
        .context("failed to reach Z.ai customer info endpoint")?;
    #[derive(Deserialize)]
    struct CustomerInfo {
        #[serde(default)]
        organizations: Vec<Organization>,
    }
    let info: CustomerInfo = parse_envelope(&read_body(response).await?)?;
    if info.organizations.is_empty() {
        bail!("Z.ai account has no organizations");
    }
    Ok(())
}
