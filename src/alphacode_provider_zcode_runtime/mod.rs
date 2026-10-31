//! ZCode (Z.ai coding plan) provider runtime.
//!
//! Speaks the Anthropic Messages API against `https://api.z.ai/api/anthropic`
//! — the same endpoint, wire format, and credential chain the ZCode CLI uses:
//! the OAuth login stores a long-lived Z.ai access token, and each request
//! presents a short-lived project access token minted from the account's
//! `zcode-api-key` (`x-api-key` header + `anthropic-version: 2023-06-01`).
//! Request bodies are built with the shared Anthropic wire-format helpers so
//! tools, images, and system prompts serialize identically to the native
//! Claude runtime. The live model catalog comes from the authenticated
//! `GET /v1/models` endpoint on the same origin. The runtime is registered
//! with `crate::alphacode_base::provider::external` by the composition root.

use crate::alphacode_base::auth::zcode as zcode_auth;
use crate::alphacode_message_types::{Message as ChatMessage, StreamEvent, ToolDefinition};
use crate::alphacode_provider_anthropic::{
    ApiRequest, build_system_param, format_messages, format_tools,
};
use crate::alphacode_provider_core::{EventStream, Provider};
use crate::alphacode_provider_zcode::{
    DEFAULT_MODEL, DISPLAY_NAME, MESSAGES_URL, MODELS, MODELS_URL, normalize_model_id,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

const MAX_RETRIES: u32 = 3;
const RETRY_BASE_DELAY_MS: u64 = 1000;
const ANTHROPIC_VERSION: &str = "2023-06-01";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CatalogSource {
    None,
    Cached,
    Live,
}

#[derive(Deserialize, serde::Serialize)]
struct PersistedCatalog {
    models: Vec<String>,
    #[allow(dead_code)]
    fetched_at_rfc3339: String,
}

pub struct ZcodeProvider {
    client: reqwest::Client,
    model: Arc<RwLock<String>>,
    fetched_models: Arc<RwLock<Vec<String>>>,
    catalog_source: Arc<RwLock<CatalogSource>>,
}

impl ZcodeProvider {
    /// Construct from the stored ZCode OAuth login. Fails when the user has
    /// not logged in yet, which callers treat as "provider unavailable".
    pub fn new() -> Result<Self> {
        if !zcode_auth::has_credentials() {
            anyhow::bail!(
                "ZCode credentials not available. Run `alphacode login --provider zcode` first."
            );
        }
        Ok(Self::with_model(
            std::env::var("ALPHACODE_ZCODE_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string()),
        ))
    }

    /// Construct with an explicit starting model, skipping the credential
    /// presence check (project tokens are minted lazily per request).
    pub fn with_model(model: String) -> Self {
        let provider = Self {
            client: crate::alphacode_provider_core::shared_http_client(),
            model: Arc::new(RwLock::new(normalize_model_id(&model))),
            fetched_models: Arc::new(RwLock::new(Vec::new())),
            catalog_source: Arc::new(RwLock::new(CatalogSource::None)),
        };
        provider.seed_cached_catalog();
        provider
    }

    pub fn has_credentials() -> bool {
        zcode_auth::has_credentials()
    }

    fn max_output_tokens(&self, model: &str) -> u32 {
        if let Ok(value) = std::env::var("ALPHACODE_ZCODE_MAX_TOKENS")
            && let Ok(parsed) = value.trim().parse::<u32>()
        {
            return parsed;
        }
        // GLM coding-plan models accept large output budgets; 64K is safe
        // across the GLM-5 generation the plan serves, 32K for GLM-4.x.
        if model.to_ascii_lowercase().starts_with("glm-4") {
            32_768
        } else {
            65_536
        }
    }

    fn build_request_body(
        &self,
        system: &str,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
    ) -> Value {
        let model = self.model();
        let api_messages = format_messages(messages, false);
        let api_tools = format_tools(tools, false, false);
        let request = ApiRequest {
            model: model.clone(),
            max_tokens: self.max_output_tokens(&model),
            system: build_system_param(system, false, false),
            messages: api_messages,
            tools: if api_tools.is_empty() {
                None
            } else {
                Some(api_tools)
            },
            metadata: None,
            thinking: None,
            output_config: None,
            temperature: None,
            service_tier: None,
            stream: true,
        };
        serde_json::to_value(request).unwrap_or_else(|error| {
            crate::alphacode_base::logging::warn(&format!(
                "Failed to serialize ZCode request body: {error}"
            ));
            Value::Null
        })
    }

    // -----------------------------------------------------------------
    // Model catalog (live fetch with the project access token)
    // -----------------------------------------------------------------

    fn persisted_catalog_path() -> Result<std::path::PathBuf> {
        Ok(crate::alphacode_base::storage::app_config_dir()?.join("zcode_models_cache.json"))
    }

    fn load_persisted_catalog() -> Option<PersistedCatalog> {
        let path = Self::persisted_catalog_path().ok()?;
        crate::alphacode_base::storage::read_json(&path)
            .ok()
            .filter(|catalog: &PersistedCatalog| !catalog.models.is_empty())
    }

    fn persist_catalog(models: &[String]) {
        if models.is_empty() {
            return;
        }
        let Ok(path) = Self::persisted_catalog_path() else {
            return;
        };
        let payload = PersistedCatalog {
            models: models.to_vec(),
            fetched_at_rfc3339: Utc::now().to_rfc3339(),
        };
        if let Err(error) = crate::alphacode_base::storage::write_json(&path, &payload) {
            crate::alphacode_base::logging::warn(&format!(
                "Failed to persist ZCode model catalog {}: {}",
                path.display(),
                error
            ));
        }
    }

    fn seed_cached_catalog(&self) {
        if let Some(catalog) = Self::load_persisted_catalog()
            && let Ok(mut models) = self.fetched_models.try_write()
        {
            *models = catalog.models;
            if let Ok(mut source) = self.catalog_source.try_write() {
                *source = CatalogSource::Cached;
            }
        }
    }

    fn catalog_source_snapshot(&self) -> CatalogSource {
        self.catalog_source
            .try_read()
            .map(|source| *source)
            .unwrap_or(CatalogSource::None)
    }

    fn model_catalog_detail_impl(&self) -> String {
        match self.catalog_source_snapshot() {
            CatalogSource::Live => String::new(),
            CatalogSource::Cached => "cached live catalog".to_string(),
            CatalogSource::None => "catalog still loading".to_string(),
        }
    }

    /// Fetch the live coding-plan model catalog. Authenticated with the
    /// short-lived project access token exactly like chat requests
    /// (`x-api-key` + `anthropic-version`), matching ZCode's own traffic.
    pub async fn fetch_live_model_ids(&self) -> Result<Vec<String>> {
        let token = zcode_auth::project_access_token(&self.client).await?;
        let response = self
            .client
            .get(MODELS_URL)
            .header("x-api-key", &token)
            .header("anthropic-version", ANTHROPIC_VERSION)
            .send()
            .await
            .context("failed to reach the Z.ai coding plan model catalog")?;
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        if !status.is_success() {
            anyhow::bail!("ZCode model catalog error (HTTP {status}): {body}");
        }
        #[derive(Deserialize)]
        struct ModelsResponse {
            #[serde(default)]
            data: Vec<CatalogModel>,
        }
        #[derive(Deserialize)]
        struct CatalogModel {
            id: String,
        }
        let parsed: ModelsResponse =
            serde_json::from_str(&body).context("invalid ZCode model catalog response")?;
        let mut models: Vec<String> = parsed
            .data
            .into_iter()
            .map(|model| normalize_model_id(&model.id))
            .collect();
        models.sort_by_key(|model| {
            // Flagship preference order first, then catalog order for the rest.
            let rank = MODELS.iter().position(|known| known == model);
            (rank.is_none(), rank.unwrap_or(usize::MAX))
        });
        models.dedup();
        Ok(models)
    }

    /// Refresh the live catalog into the picker state and the disk cache.
    /// Keeps the previous list on failure so the picker never goes empty.
    pub async fn refresh_model_catalog(&self) {
        match self.fetch_live_model_ids().await {
            Ok(models) if !models.is_empty() => {
                if let Ok(mut fetched) = self.fetched_models.try_write() {
                    *fetched = models.clone();
                }
                if let Ok(mut source) = self.catalog_source.try_write() {
                    *source = CatalogSource::Live;
                }
                Self::persist_catalog(&models);
                crate::alphacode_base::bus::Bus::global().publish_models_updated();
            }
            Ok(_) => {
                crate::alphacode_base::logging::info(
                    "ZCode model catalog returned no models; keeping the previous list",
                );
            }
            Err(error) => {
                crate::alphacode_base::logging::info(&format!(
                    "ZCode model catalog fetch failed; keeping the previous list: {error}"
                ));
            }
        }
    }

    // -----------------------------------------------------------------
    // Chat streaming
    // -----------------------------------------------------------------

    async fn stream_request(&self, body: Value, tx: mpsc::Sender<Result<StreamEvent>>) {
        use crate::alphacode_message_types::ConnectionPhase;

        let model = self.model();
        let mut last_error: Option<anyhow::Error> = None;
        let mut attempted_auth_refresh = false;

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let delay = crate::alphacode_provider_core::attempt_tracker::retry_backoff_delay(
                    attempt,
                    RETRY_BASE_DELAY_MS,
                );
                crate::alphacode_base::logging::info(&format!(
                    "Retrying ZCode API request (attempt {}/{}) after {}ms",
                    attempt + 1,
                    MAX_RETRIES,
                    delay.as_millis()
                ));
                let _ = tx
                    .send(Ok(StreamEvent::ConnectionPhase {
                        phase: ConnectionPhase::Retrying {
                            attempt: attempt + 1,
                            max: MAX_RETRIES,
                        },
                    }))
                    .await;
                tokio::time::sleep(delay).await;
            }

            let token = match zcode_auth::project_access_token(&self.client).await {
                Ok(token) => token,
                Err(error) => {
                    let _ = tx.send(Err(error)).await;
                    return;
                }
            };

            // Retries use a fresh unpooled client so a fault that broke
            // attempt N (e.g. a corrupting middlebox) cannot repeat through
            // the shared idle pool.
            let attempt_client = if attempt == 0 {
                self.client.clone()
            } else {
                crate::alphacode_provider_core::fresh_transport_client()
            };

            let response = attempt_client
                .post(MESSAGES_URL)
                .header("x-api-key", &token)
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await;

            let response = match response {
                Ok(response) => response,
                Err(error) => {
                    let error_str = format!("{error:#}").to_lowercase();
                    if is_retryable_error(&error_str) && attempt + 1 < MAX_RETRIES {
                        last_error = Some(anyhow::anyhow!("ZCode API request failed: {error}"));
                        continue;
                    }
                    let _ = tx
                        .send(Err(anyhow::anyhow!("ZCode API request failed: {error}")))
                        .await;
                    return;
                }
            };

            let status = response.status();

            // The project access token is short-lived; on auth failure mint a
            // fresh one once before surfacing the error.
            if (status == reqwest::StatusCode::UNAUTHORIZED
                || status == reqwest::StatusCode::FORBIDDEN)
                && !attempted_auth_refresh
            {
                attempted_auth_refresh = true;
                zcode_auth::invalidate_project_token().await;
                crate::alphacode_base::logging::info("ZCode project token rejected, re-minting...");
                last_error = Some(anyhow::anyhow!("ZCode auth error (HTTP {status})"));
                continue;
            }

            if !status.is_success() {
                let body_text =
                    crate::alphacode_base::util::http_error_body(response, "HTTP error").await;
                let error_str =
                    format!("ZCode API error (HTTP {status}): {body_text}").to_lowercase();
                if is_retryable_error(&error_str) && attempt + 1 < MAX_RETRIES {
                    last_error = Some(anyhow::anyhow!(
                        "ZCode API error (HTTP {status}): {body_text}"
                    ));
                    continue;
                }
                let _ = tx
                    .send(Err(anyhow::anyhow!(
                        "ZCode API error (HTTP {status}): {body_text}"
                    )))
                    .await;
                return;
            }

            let _ = tx
                .send(Ok(StreamEvent::ConnectionType {
                    connection: format!("zcode-api ({model})"),
                }))
                .await;

            let (attempt_tx, attempt_guard) =
                crate::alphacode_provider_core::attempt_tracker::track_attempt_output(tx.clone());

            match process_sse_stream(response, attempt_tx).await {
                Ok(()) => {
                    let _ = attempt_guard.finish().await;
                    return;
                }
                Err(error) => {
                    let saw_output = attempt_guard.finish().await;
                    let error_str = format!("{error:#}").to_lowercase();
                    if is_retryable_error(&error_str) && attempt + 1 < MAX_RETRIES {
                        if saw_output {
                            let _ = tx
                                .send(Ok(StreamEvent::RetryRollback {
                                    attempt: attempt + 2,
                                    max: MAX_RETRIES,
                                }))
                                .await;
                        }
                        last_error = Some(error);
                        continue;
                    }
                    let _ = tx.send(Err(error)).await;
                    return;
                }
            }
        }

        if let Some(error) = last_error {
            let _ = tx
                .send(Err(anyhow::anyhow!(
                    "ZCode: failed after {MAX_RETRIES} retries: {error}"
                )))
                .await;
        }
    }
}

fn is_retryable_error(error_str: &str) -> bool {
    let lower = error_str.to_ascii_lowercase();
    crate::alphacode_provider_core::retry::is_retryable_message(&lower)
        || lower.contains("stream error")
        || lower.contains("stream read timeout")
}

// ---------------------------------------------------------------------------
// Anthropic SSE parsing (Messages API streaming format)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct UsageInfo {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
}

#[derive(Deserialize)]
struct SseEvent {
    #[serde(default)]
    r#type: String,
    #[serde(default)]
    delta: Option<Value>,
    #[serde(default)]
    content_block: Option<Value>,
    #[serde(default)]
    usage: Option<Value>,
    /// `message_start` nests the message (with its usage) under this key.
    #[serde(default)]
    message: Option<Value>,
}

#[derive(Default)]
struct SseState {
    input_tokens: u64,
    output_tokens: u64,
    cache_read_input_tokens: Option<u64>,
    cache_creation_input_tokens: Option<u64>,
    usage_sent: bool,
}

async fn process_sse_stream(
    response: reqwest::Response,
    tx: mpsc::Sender<Result<StreamEvent>>,
) -> Result<()> {
    use futures::StreamExt;

    let sse_chunk_timeout = crate::alphacode_base::provider::stream_idle_timeout();
    let mut stream = response.bytes_stream();
    let mut buffer = String::new();
    let mut state = SseState::default();

    loop {
        let chunk = match tokio::time::timeout(sse_chunk_timeout, stream.next()).await {
            Ok(Some(Ok(chunk))) => chunk,
            Ok(Some(Err(error))) => anyhow::bail!("Stream error: {error}"),
            Ok(None) => break,
            Err(_) => anyhow::bail!(
                "Stream read timeout: no data received for {} seconds",
                sse_chunk_timeout.as_secs()
            ),
        };

        buffer.push_str(&String::from_utf8_lossy(&chunk));

        while let Some(line_end) = buffer.find('\n') {
            let line = buffer[..line_end].trim_end_matches('\r').to_string();
            buffer = buffer[line_end + 1..].to_string();
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            let Some(data) = crate::alphacode_base::util::sse_data_line(&line) else {
                continue;
            };
            let Ok(event) = serde_json::from_str::<SseEvent>(data) else {
                continue;
            };

            match event.r#type.as_str() {
                "message_start" => {
                    if let Some(usage) = event
                        .usage
                        .or_else(|| {
                            event
                                .message
                                .as_ref()
                                .and_then(|message| message.get("usage").cloned())
                        })
                        .and_then(|u| serde_json::from_value::<UsageInfo>(u).ok())
                    {
                        state.input_tokens = usage.input_tokens.unwrap_or(0);
                        state.output_tokens = usage.output_tokens.unwrap_or(0);
                        state.cache_read_input_tokens = usage.cache_read_input_tokens;
                        state.cache_creation_input_tokens = usage.cache_creation_input_tokens;
                    }
                }
                "content_block_start" => {
                    if let Some(block) = &event.content_block {
                        let block_type = block.get("type").and_then(Value::as_str).unwrap_or("");
                        match block_type {
                            "thinking" => {
                                let _ = tx.send(Ok(StreamEvent::ThinkingStart)).await;
                            }
                            "tool_use" => {
                                let _ = tx
                                    .send(Ok(StreamEvent::ToolUseStart {
                                        id: block
                                            .get("id")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default()
                                            .to_string(),
                                        name: block
                                            .get("name")
                                            .and_then(Value::as_str)
                                            .unwrap_or_default()
                                            .to_string(),
                                    }))
                                    .await;
                            }
                            _ => {}
                        }
                    }
                }
                "content_block_delta" => {
                    if let Some(delta) = &event.delta {
                        match delta.get("type").and_then(Value::as_str).unwrap_or("") {
                            "text_delta" => {
                                if let Some(text) = delta
                                    .get("text")
                                    .and_then(Value::as_str)
                                    .filter(|t| !t.is_empty())
                                {
                                    let _ =
                                        tx.send(Ok(StreamEvent::TextDelta(text.to_string()))).await;
                                }
                            }
                            "input_json_delta" => {
                                if let Some(partial) =
                                    delta.get("partial_json").and_then(Value::as_str)
                                {
                                    let _ = tx
                                        .send(Ok(StreamEvent::ToolInputDelta(partial.to_string())))
                                        .await;
                                }
                            }
                            "thinking_delta" => {
                                if let Some(thinking) =
                                    delta.get("thinking").and_then(Value::as_str)
                                {
                                    let _ = tx
                                        .send(Ok(StreamEvent::ThinkingDelta(thinking.to_string())))
                                        .await;
                                }
                            }
                            "signature_delta" => {
                                if let Some(signature) =
                                    delta.get("signature").and_then(Value::as_str)
                                {
                                    let _ = tx
                                        .send(Ok(StreamEvent::ThinkingSignatureDelta(
                                            signature.to_string(),
                                        )))
                                        .await;
                                }
                            }
                            _ => {}
                        }
                    }
                }
                "content_block_stop" => {
                    // The stop event does not say which block closed; closing
                    // both is safe because only one block is ever open at a
                    // time in the Messages streaming format.
                    let _ = tx.send(Ok(StreamEvent::ToolUseEnd)).await;
                    let _ = tx.send(Ok(StreamEvent::ThinkingEnd)).await;
                }
                "message_delta" => {
                    if let Some(usage) = event
                        .usage
                        .and_then(|u| serde_json::from_value::<UsageInfo>(u).ok())
                    {
                        if usage.output_tokens.is_some() {
                            state.output_tokens = usage.output_tokens.unwrap_or(0);
                        }
                        state.cache_read_input_tokens = usage
                            .cache_read_input_tokens
                            .or(state.cache_read_input_tokens);
                        state.cache_creation_input_tokens = usage
                            .cache_creation_input_tokens
                            .or(state.cache_creation_input_tokens);
                    }
                    if let Some(stop_reason) = event
                        .delta
                        .as_ref()
                        .and_then(|d| d.get("stop_reason"))
                        .and_then(Value::as_str)
                    {
                        emit_usage(&tx, &state).await;
                        state.usage_sent = true;
                        let _ = tx
                            .send(Ok(StreamEvent::MessageEnd {
                                stop_reason: Some(stop_reason.to_string()),
                            }))
                            .await;
                    }
                }
                "message_stop" => {
                    if !state.usage_sent {
                        emit_usage(&tx, &state).await;
                    }
                    let _ = tx
                        .send(Ok(StreamEvent::MessageEnd { stop_reason: None }))
                        .await;
                    return Ok(());
                }
                "error" => {
                    let message = event
                        .delta
                        .as_ref()
                        .and_then(|d| d.get("message"))
                        .and_then(Value::as_str)
                        .unwrap_or("stream error");
                    anyhow::bail!("ZCode stream error: {message}");
                }
                _ => {}
            }
        }
    }

    // Stream ended without message_stop.
    if !state.usage_sent {
        emit_usage(&tx, &state).await;
    }
    let _ = tx
        .send(Ok(StreamEvent::MessageEnd { stop_reason: None }))
        .await;
    Ok(())
}

async fn emit_usage(tx: &mpsc::Sender<Result<StreamEvent>>, state: &SseState) {
    if state.input_tokens > 0 || state.output_tokens > 0 {
        let _ = tx
            .send(Ok(StreamEvent::TokenUsage {
                input_tokens: Some(state.input_tokens),
                output_tokens: Some(state.output_tokens),
                cache_read_input_tokens: state.cache_read_input_tokens,
                cache_creation_input_tokens: state.cache_creation_input_tokens,
            }))
            .await;
    }
}

#[async_trait]
impl Provider for ZcodeProvider {
    async fn complete(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<EventStream> {
        // Fail fast (before spawning) when the login is missing so callers
        // can fall back to another provider.
        zcode_auth::load_credentials()?;

        let body = self.build_request_body(system, messages, tools);

        let (tx, rx) = mpsc::channel::<Result<StreamEvent>>(100);
        let worker = ZcodeProvider {
            client: self.client.clone(),
            model: self.model.clone(),
            fetched_models: self.fetched_models.clone(),
            catalog_source: self.catalog_source.clone(),
        };
        tokio::spawn(async move {
            worker.stream_request(body, tx).await;
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    fn name(&self) -> &str {
        "zcode"
    }

    fn display_name(&self) -> String {
        DISPLAY_NAME.to_string()
    }

    fn model(&self) -> String {
        self.model
            .try_read()
            .map(|model| model.clone())
            .unwrap_or_else(|_| DEFAULT_MODEL.to_string())
    }

    fn set_model(&self, model: &str) -> Result<()> {
        let normalized = normalize_model_id(model);
        if normalized.is_empty() {
            anyhow::bail!("ZCode model cannot be empty");
        }
        if let Ok(mut current) = self.model.try_write() {
            *current = normalized;
            Ok(())
        } else {
            Err(anyhow::anyhow!(
                "Cannot change model while a request is in progress"
            ))
        }
    }

    fn available_models(&self) -> Vec<&'static str> {
        MODELS.to_vec()
    }

    fn available_models_display(&self) -> Vec<String> {
        if let Ok(models) = self.fetched_models.read()
            && !models.is_empty()
        {
            return models.clone();
        }
        MODELS.iter().map(|model| model.to_string()).collect()
    }

    fn available_models_for_switching(&self) -> Vec<String> {
        self.available_models_display()
    }

    async fn prefetch_models(&self) -> Result<()> {
        self.refresh_model_catalog().await;
        Ok(())
    }

    fn model_catalog_detail(&self) -> String {
        self.model_catalog_detail_impl()
    }

    fn supports_compaction(&self) -> bool {
        true
    }

    fn context_window(&self) -> usize {
        crate::alphacode_provider_core::context_limit_for_model_with_provider(
            &self.model(),
            Some(self.name()),
        )
        .unwrap_or(128_000)
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(ZcodeProvider {
            client: self.client.clone(),
            model: Arc::new(RwLock::new(self.model())),
            fetched_models: self.fetched_models.clone(),
            catalog_source: self.catalog_source.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_body_has_anthropic_shape() {
        let provider = ZcodeProvider::with_model("glm-5.3".to_string());
        let messages = vec![crate::alphacode_message_types::Message {
            role: crate::alphacode_message_types::Role::User,
            content: vec![crate::alphacode_message_types::ContentBlock::Text {
                text: "hello".to_string(),
                cache_control: None,
            }],
            timestamp: None,
            tool_duration_ms: None,
        }];
        let body = provider.build_request_body("be brief", &messages, &[]);
        assert_eq!(body["model"], "GLM-5.3");
        assert_eq!(body["stream"], true);
        assert_eq!(body["max_tokens"], 65_536);
        assert_eq!(
            body["messages"][0]["content"][0]["text"],
            serde_json::json!("hello")
        );
        // System is passed as Anthropic system blocks, not a message.
        assert!(body["system"].is_array());
        assert_eq!(body["messages"].as_array().map(Vec::len), Some(1));
    }

    #[tokio::test]
    async fn sse_stream_parses_anthropic_events() {
        let (tx, mut rx) = mpsc::channel::<Result<StreamEvent>>(64);
        let payload = concat!(
            "event: message_start\n",
            "data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":10,\"output_tokens\":1}}}\n\n",
            "data: {\"type\":\"content_block_start\",\"content_block\":{\"type\":\"text\"}}\n\n",
            "data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"hi\"}}\n\n",
            "data: {\"type\":\"content_block_stop\"}\n\n",
            "data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n",
            "data: {\"type\":\"message_stop\"}\n\n",
        );
        let response = http::Response::builder()
            .status(200)
            .header("content-type", "text/event-stream")
            .body(payload.as_bytes().to_vec())
            .expect("static response");
        let response: reqwest::Response = response.into();
        process_sse_stream(response, tx)
            .await
            .expect("stream should parse");
        let mut events = Vec::new();
        while let Ok(event) = rx.try_recv() {
            events.push(event.expect("event"));
        }
        assert!(
            events
                .iter()
                .any(|event| matches!(event, StreamEvent::TextDelta(text) if text == "hi"))
        );
        assert!(events.iter().any(|event| matches!(
            event,
            StreamEvent::MessageEnd {
                stop_reason: Some(reason)
            } if reason == "end_turn"
        )));
        assert!(events.iter().any(|event| matches!(
            event,
            StreamEvent::TokenUsage {
                input_tokens: Some(10),
                output_tokens: Some(2),
                ..
            }
        )));
    }
}
