//! Shared Google OAuth2 token endpoint helpers.
//!
//! Gemini, Antigravity, and Gmail/Google auth all refresh against the same
//! `https://oauth2.googleapis.com/token` endpoint with a
//! `grant_type=refresh_token` form post. This module owns that HTTP exchange
//! once; provider modules keep only their provider-specific concerns
//! (storage format, extra metadata like project id, refresh-state record
//! keys).

use anyhow::{Context, Result};
use serde::Deserialize;

pub const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

#[derive(Debug, Deserialize)]
struct GoogleTokenResponse {
    access_token: String,
    #[serde(default)]
    refresh_token: Option<String>,
    expires_in: i64,
}

/// Result of a successful refresh-token grant.
pub struct RefreshedGoogleToken {
    pub access_token: String,
    /// New refresh token if Google rotated it; otherwise the token passed in.
    pub refresh_token: String,
    /// Absolute expiry in unix milliseconds.
    pub expires_at_ms: i64,
}

/// Exchange a refresh token for a new access token at the Google token
/// endpoint. `provider_label` is used in error messages (for example
/// "Gemini" or "Antigravity").
pub async fn refresh_access_token(
    provider_label: &str,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
    user_agent: Option<&str>,
) -> Result<RefreshedGoogleToken> {
    let client = crate::provider::shared_http_client();
    let mut request = client.post(GOOGLE_TOKEN_URL).form(&[
        ("grant_type", "refresh_token"),
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("refresh_token", refresh_token),
    ]);
    if let Some(user_agent) = user_agent {
        request = request.header(reqwest::header::USER_AGENT, user_agent);
    }

    let resp = request
        .send()
        .await
        .with_context(|| format!("Failed to refresh {provider_label} OAuth token"))?;

    if !resp.status().is_success() {
        let body = crate::util::http_error_body(resp, "HTTP error").await;
        anyhow::bail!("{provider_label} token refresh failed: {}", body.trim());
    }

    let token_resp: GoogleTokenResponse = resp
        .json()
        .await
        .with_context(|| format!("Failed to parse {provider_label} refresh response"))?;

    Ok(RefreshedGoogleToken {
        access_token: token_resp.access_token,
        refresh_token: token_resp
            .refresh_token
            .unwrap_or_else(|| refresh_token.to_string()),
        expires_at_ms: chrono::Utc::now().timestamp_millis() + (token_resp.expires_in * 1000),
    })
}
