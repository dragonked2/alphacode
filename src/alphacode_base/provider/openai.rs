//! OpenAI provider shared helpers (compatibility shim).
//!
//! The OpenAI provider *runtime* (`OpenAIProvider`: Codex OAuth + API key,
//! Responses API over SSE and persistent WebSocket) now lives in the
//! downstream `alphacode-provider-openai-runtime` crate so provider edits do not
//! rebuild the base -> app-core -> tui spine. The binary's composition root
//! registers it via [`crate::provider::external`].
//!
//! Base keeps the pieces its own catalog/routing code shares with the runtime:
//! - the API base-URL resolution used by catalog fetches, and
//! - the extended prompt-cache-retention model predicate used by
//!   cache-TTL routing.

pub use crate::alphacode_provider_core::CredentialMode as OpenAICredentialMode;

const OPENAI_API_BASE: &str = "https://api.openai.com/v1";

/// Resolve the OpenAI Responses API base URL for **API-key** mode.
///
/// Defaults to `https://api.openai.com/v1`, but honors a user override so
/// the native `openai-api` provider can target a local/proxied Responses
/// API endpoint (issue #343). Checked in order:
/// `ALPHACODE_OPENAI_API_BASE`, `OPENAI_BASE_URL`, `OPENAI_API_BASE`.
///
/// The override must be an absolute `http(s)://` URL; anything else is
/// logged and ignored so a malformed value never silently breaks requests.
/// A `/responses` suffix is not expected here (it is appended by callers),
/// so a trailing `/responses` is trimmed to avoid `.../responses/responses`.
pub fn resolve_api_base() -> String {
    const OVERRIDE_VARS: [&str; 3] = [
        "ALPHACODE_OPENAI_API_BASE",
        "OPENAI_BASE_URL",
        "OPENAI_API_BASE",
    ];
    for var in OVERRIDE_VARS {
        let Ok(raw) = std::env::var(var) else {
            continue;
        };
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !(trimmed.starts_with("http://") || trimmed.starts_with("https://")) {
            crate::logging::warn(&format!(
                "Ignoring invalid {} '{}'; expected an absolute http(s):// URL",
                var, trimmed
            ));
            continue;
        }
        let normalized = trimmed
            .trim_end_matches('/')
            .trim_end_matches("/responses")
            .trim_end_matches('/');
        if normalized.is_empty() {
            crate::logging::warn(&format!(
                "Ignoring invalid {} '{}'; URL has no host/path",
                var, trimmed
            ));
            continue;
        }
        crate::logging::info(&format!(
            "OpenAI Responses API base overridden to '{}' via {}",
            normalized, var
        ));
        return normalized.to_string();
    }
    // Fall back to the active Codex Responses provider base URL from
    // `~/.codex/config.toml` so OpenAI-compatible API-key traffic honors a
    // gateway configured there, before defaulting to api.openai.com (#374).
    if let Some(base) = codex_config_responses_base() {
        crate::logging::info(&format!(
            "OpenAI Responses API base resolved to '{}' from ~/.codex/config.toml",
            base
        ));
        return base;
    }
    OPENAI_API_BASE.to_string()
}

/// Read the active Codex model_provider's `base_url` from
/// `~/.codex/config.toml` when it serves the Responses wire API.
///
/// Codex config shape:
/// ```toml
/// model_provider = "mygw"
/// [model_providers.mygw]
/// base_url = "https://gateway.example/v1"
/// wire_api = "responses"   # only "responses" is honored here
/// ```
/// Returns `None` when the file/keys are missing, the URL is not absolute
/// http(s), or the provider's `wire_api` is not `responses`.
fn codex_config_responses_base() -> Option<String> {
    let path = crate::storage::user_home_path(".codex/config.toml").ok()?;
    let contents = std::fs::read_to_string(&path).ok()?;
    let value: toml::Value = contents.parse().ok()?;

    let provider_name = value.get("model_provider")?.as_str()?.trim();
    if provider_name.is_empty() {
        return None;
    }
    let provider = value
        .get("model_providers")?
        .as_table()?
        .get(provider_name)?
        .as_table()?;

    // Only honor providers that speak the Responses wire API. When the key
    // is absent, Codex defaults to the Responses API for OpenAI-style
    // providers, so treat "missing" as eligible.
    if let Some(wire_api) = provider.get("wire_api").and_then(|v| v.as_str())
        && !wire_api.trim().eq_ignore_ascii_case("responses")
    {
        return None;
    }

    let base = provider.get("base_url")?.as_str()?.trim();
    if !(base.starts_with("http://") || base.starts_with("https://")) {
        crate::logging::warn(&format!(
            "Ignoring ~/.codex/config.toml base_url '{}' for provider '{}'; expected an absolute http(s):// URL",
            base, provider_name
        ));
        return None;
    }
    let normalized = base
        .trim_end_matches('/')
        .trim_end_matches("/responses")
        .trim_end_matches('/');
    if normalized.is_empty() {
        return None;
    }
    Some(normalized.to_string())
}

/// Whether `model_id` supports OpenAI's extended prompt-cache retention.
pub fn supports_extended_prompt_cache_retention(model_id: &str) -> bool {
    let model = model_id.trim().to_ascii_lowercase();
    model.starts_with("gpt-5.6")
        || model.starts_with("gpt-5.5")
        || model.starts_with("gpt-5.4")
        || model.starts_with("gpt-5.2")
        || model.starts_with("gpt-5.1")
        || model == "gpt-5"
}
