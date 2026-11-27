//! Antigravity provider runtime (Google Cloud Code backend multiplexing
//! Gemini/Claude/gpt-oss upstreams), moved out of `alphacode-base` so provider
//! edits compile only this crate plus a binary relink instead of rebuilding
//! the base -> app-core -> tui spine. The binary's composition root registers
//! [`AntigravityProvider`] with `crate::alphacode_base::provider::external` at startup.

/// Maximum number of retry attempts for a single Antigravity generateContent
/// call when the upstream returns 429 / RESOURCE_EXHAUSTED. Matches the
/// budget used by the Anthropic provider runtime.
const MAX_RETRIES: u32 = crate::alphacode_provider_core::retry::MAX_ATTEMPTS;

use crate::alphacode_base::auth::antigravity as antigravity_auth;
use crate::alphacode_message_types::{ConnectionPhase, Message, StreamEvent, ToolDefinition};
use crate::alphacode_provider_antigravity::{
    ANTIGRAVITY_API_BASES, AVAILABLE_MODELS, CatalogModel, CatalogSnapshot, DEFAULT_FALLBACK_MODEL,
    PersistedCatalog, X_GOOG_API_CLIENT, antigravity_compatible_schema, antigravity_user_agent,
    catalog_is_stale, catalog_model_detail, client_metadata_header, is_retryable_empty_turn,
    merge_antigravity_model_ids, remap_unsupported_model,
};
use crate::alphacode_provider_core::request_audit::AuditedSend as _;
use crate::alphacode_provider_core::{EventStream, Provider};
use crate::alphacode_provider_gemini::{
    CodeAssistGenerateRequest, CodeAssistGenerateResponse, GeminiFunctionCallingConfig,
    GeminiToolConfig, VertexGenerateContentRequest,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use uuid::Uuid;

const DEFAULT_MODEL: &str = "default";

pub struct AntigravityProvider {
    client: reqwest::Client,
    model: Arc<RwLock<String>>,
    fetched_catalog: Arc<RwLock<Vec<CatalogModel>>>,
    /// Backend-advertised default agent model id (from `fetchAvailableModels`).
    /// Used to resolve the `"default"` alias to a real model for inference.
    backend_default_model: Arc<RwLock<Option<String>>>,
}

impl Clone for AntigravityProvider {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            model: self.model.clone(),
            fetched_catalog: self.fetched_catalog.clone(),
            backend_default_model: self.backend_default_model.clone(),
        }
    }
}

static STARTUP_CATALOG_REFRESH_ACTIVE: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

impl AntigravityProvider {
    fn load_persisted_catalog() -> Option<PersistedCatalog> {
        crate::alphacode_base::provider::antigravity::load_persisted_catalog()
    }

    fn persist_catalog(snapshot: &CatalogSnapshot) {
        crate::alphacode_base::provider::antigravity::persist_catalog(snapshot);
    }

    /// Seed the in-memory catalog from disk and return whether a background
    /// refresh should be started (cache missing, empty, or stale).
    fn seed_cached_catalog(&self) -> bool {
        let Some(catalog) = Self::load_persisted_catalog() else {
            return true;
        };
        let is_stale = catalog_is_stale(&catalog.fetched_at_rfc3339);
        if is_stale {
            crate::alphacode_base::logging::info(
                "Loaded stale persisted Antigravity model catalog; a refresh will update it on next prefetch",
            );
        }
        if let Some(default_model_id) = catalog.default_model_id.clone() {
            *self
                .backend_default_model
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(default_model_id);
        }
        if let Ok(mut models) = self.fetched_catalog.write() {
            *models = catalog.models;
        }
        is_stale
    }

    /// Populate `fetched_catalog` in the background on daemon startup when
    /// Antigravity OAuth credentials already exist on disk and the local cache
    /// was missing or stale.
    fn spawn_startup_catalog_refresh_if_needed(&self) {
        if cfg!(test) || !antigravity_auth::has_cached_auth() {
            return;
        }
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return;
        };
        if STARTUP_CATALOG_REFRESH_ACTIVE
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            )
            .is_err()
        {
            return;
        }

        let provider = self.clone();
        handle.spawn(async move {
            struct Guard;
            impl Drop for Guard {
                fn drop(&mut self) {
                    STARTUP_CATALOG_REFRESH_ACTIVE
                        .store(false, std::sync::atomic::Ordering::Release);
                }
            }
            let _guard = Guard;

            if provider.prefetch_models().await.is_ok() && !provider.fetched_catalog().is_empty() {
                crate::alphacode_base::bus::Bus::global().publish_models_updated();
            }
        });
    }

    pub fn new() -> Self {
        let model =
            std::env::var("ALPHACODE_ANTIGRAVITY_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.into());

        let provider = Self {
            client: crate::alphacode_provider_core::shared_http_client(),
            model: Arc::new(RwLock::new(model)),
            fetched_catalog: Arc::new(RwLock::new(Vec::new())),
            backend_default_model: Arc::new(RwLock::new(None)),
        };
        if provider.seed_cached_catalog() {
            provider.spawn_startup_catalog_refresh_if_needed();
        }
        provider
    }

    fn fetched_catalog(&self) -> Vec<CatalogModel> {
        self.fetched_catalog
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn backend_default_model(&self) -> Option<String> {
        self.backend_default_model
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    /// Resolve the live OAuth credential the runtime would use, for the
    /// provider-doctor's native Antigravity driver.
    ///
    /// Antigravity authenticates exclusively via the Google OAuth tokens minted
    /// by `alphacode login --provider antigravity`; there is no API-key path. This
    /// loads (and refreshes if needed) those tokens through the exact same code
    /// path inference uses, returning only the resolved Google account email so
    /// the doctor can confirm the credential without ever surfacing the token
    /// itself.
    pub async fn resolve_account_for_doctor(&self) -> Result<String> {
        let tokens = antigravity_auth::load_or_refresh_tokens().await?;
        if tokens.access_token.trim().is_empty() {
            anyhow::bail!("resolved an empty Antigravity access token");
        }
        match antigravity_auth::fetch_email(&tokens.access_token).await {
            Ok(email) if !email.trim().is_empty() => Ok(email),
            _ => Ok(String::new()),
        }
    }

    /// Fetch the live Antigravity model catalog using the resolved credential.
    ///
    /// Mirrors [`Provider::prefetch_models`] but returns the available model ids
    /// to the caller (rather than only persisting them) so the doctor can assert
    /// the live `fetchAvailableModels` endpoint works and that the model under
    /// test is in the live catalog. The warm catalog is persisted exactly like
    /// the runtime's own prefetch so the rest of the process benefits.
    pub async fn fetch_live_model_ids_for_doctor(&self) -> Result<Vec<String>> {
        let snapshot = self.fetch_available_models().await?;
        if snapshot.models.is_empty() {
            anyhow::bail!("Antigravity model catalog returned no models");
        }
        Self::persist_catalog(&snapshot);
        if let Some(default_model_id) = snapshot.default_model_id.clone() {
            *self
                .backend_default_model
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(default_model_id);
        }
        let model_ids: Vec<String> = snapshot
            .models
            .iter()
            .filter(|model| model.available)
            .map(|model| model.id.clone())
            .collect();
        *self
            .fetched_catalog
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = snapshot.models;
        if model_ids.is_empty() {
            anyhow::bail!("Antigravity model catalog returned no available models");
        }
        Ok(model_ids)
    }

    /// Live catalog snapshot for the `/usage` report: per-model quota
    /// (`remaining_fraction_milli`) and reset times. Persists the warm catalog
    /// like the runtime prefetch so the rest of the process benefits.
    pub async fn fetch_catalog_snapshot_for_usage(&self) -> Result<CatalogSnapshot> {
        let snapshot = self.fetch_available_models().await?;
        if snapshot.models.is_empty() {
            anyhow::bail!("Antigravity model catalog returned no models");
        }
        Self::persist_catalog(&snapshot);
        Ok(snapshot)
    }

    /// Resolve a requested model id into a real backend model id. The literal
    /// alias `"default"` (and the empty string) is rejected by the
    /// `generateContent` endpoint with HTTP 404, so it is mapped to the
    /// backend-advertised default, then to a known-good catalog model, and
    /// finally to a hardcoded fallback.
    ///
    /// Note: this only resolves the `"default"` alias / empty input. An
    /// explicit model id from the user is honoured verbatim, except for ids
    /// the backend advertises but cannot actually service, which are remapped
    /// to an equivalent working id via [`remap_unsupported_model`].
    fn resolve_model_for_request(&self, model: &str) -> String {
        let trimmed = model.trim();
        if !trimmed.is_empty() && trimmed != DEFAULT_MODEL {
            return remap_unsupported_model(trimmed).to_string();
        }

        if let Some(backend_default) = self
            .backend_default_model()
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty() && id != DEFAULT_MODEL)
        {
            return remap_unsupported_model(&backend_default).to_string();
        }

        // No backend-advertised default: pick a usable catalog model. Prefer a
        // Gemini model, which works reliably with tool use on this backend.
        // Claude models on the Cloud Code backend currently reject alphacode's tool
        // schemas (they require JSON Schema draft 2020-12), so they are a poor
        // automatic default even when listed first in the catalog.
        let catalog = self.fetched_catalog();
        if let Some(gemini_model) = catalog
            .iter()
            .find(|model| {
                model.available
                    && model.id.trim() != DEFAULT_MODEL
                    && model.id.starts_with("gemini-")
            })
            .map(|model| model.id.clone())
        {
            return gemini_model;
        }
        if let Some(catalog_model) = catalog
            .iter()
            .find(|model| model.available && model.id.trim() != DEFAULT_MODEL)
            .map(|model| model.id.clone())
        {
            return catalog_model;
        }

        DEFAULT_FALLBACK_MODEL.to_string()
    }

    async fn fetch_available_models(&self) -> Result<CatalogSnapshot> {
        crate::alphacode_base::provider::antigravity::fetch_catalog_snapshot(&self.client).await
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "builds the complete Code Assist payload cleanly for both live calls and serialization tests"
    )]
    fn build_generate_content_request(
        &self,
        model: &str,
        project: String,
        messages: &[Message],
        tools: &[ToolDefinition],
        system: &str,
        resume_session_id: Option<&str>,
        force_function_call: bool,
        signature_policy: crate::alphacode_provider_gemini::SignaturePolicy,
    ) -> CodeAssistGenerateRequest {
        let resolved_model = self.resolve_model_for_request(model);
        let tools_is_empty = tools.is_empty();
        let mut built_tools = crate::alphacode_provider_gemini::build_tools(tools);
        // Normalize each tool's JSON schema for the specific Antigravity backend
        // path the resolved model uses. The Cloud Code backend forwards each
        // model family to a different upstream (Gemini-native, Gemini->Anthropic,
        // or an OpenAI-compatible bridge), and each upstream rejects a different
        // construct. See `gemini_compatible_schema` and `antigravity_compatible_schema`.
        if let Some(tools) = built_tools.as_mut() {
            for tool in tools.iter_mut() {
                for decl in tool.function_declarations.iter_mut() {
                    decl.parameters =
                        antigravity_compatible_schema(&decl.parameters, &resolved_model);
                }
            }
        }
        CodeAssistGenerateRequest {
            model: resolved_model,
            project,
            user_prompt_id: Uuid::new_v4().to_string(),
            request: VertexGenerateContentRequest {
                contents: crate::alphacode_provider_gemini::build_contents_with_signature_policy(
                    messages,
                    signature_policy,
                ),
                system_instruction:
                    crate::alphacode_provider_gemini::build_system_instruction_with_tool_guard(
                        system,
                        !tools_is_empty,
                    ),
                tools: built_tools,
                tool_config: if tools_is_empty {
                    None
                } else {
                    // On a transparent retry after a MALFORMED_FUNCTION_CALL, force
                    // function-calling mode `ANY` so the model must emit a real
                    // functionCall instead of the Python-style pseudo-code that
                    // triggered the malformed turn (the proven recovery for this
                    // failure mode). Normal turns use `AUTO`.
                    Some(GeminiToolConfig {
                        function_calling_config: GeminiFunctionCallingConfig {
                            mode: if force_function_call { "ANY" } else { "AUTO" },
                        },
                    })
                },
                session_id: resume_session_id
                    .filter(|value| !value.trim().is_empty())
                    .map(str::to_string),
            },
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the Code Assist call threads explicit per-request settings, including the signature policy, without hidden state"
    )]
    async fn generate_content(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolDefinition],
        system: &str,
        resume_session_id: Option<&str>,
        force_function_call: bool,
        signature_policy: crate::alphacode_provider_gemini::SignaturePolicy,
    ) -> Result<CodeAssistGenerateResponse> {
        let mut tokens = antigravity_auth::load_or_refresh_tokens().await?;
        let project = match tokens
            .project_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            Some(project_id) => project_id.to_string(),
            None => {
                let project_id = antigravity_auth::fetch_project_id(&tokens.access_token).await?;
                tokens.project_id = Some(project_id.clone());
                let _ = antigravity_auth::save_tokens(&tokens);
                project_id
            }
        };
        let request = self.build_generate_content_request(
            model,
            project,
            messages,
            tools,
            system,
            resume_session_id,
            force_function_call,
            signature_policy,
        );

        if crate::alphacode_provider_core::fingerprint::fingerprint_enabled() {
            let contents_value =
                serde_json::to_value(&request.request.contents).unwrap_or(Value::Null);
            let content_items = contents_value.as_array().cloned().unwrap_or_default();
            let system_value = request
                .request
                .system_instruction
                .as_ref()
                .and_then(|system| serde_json::to_value(system).ok());
            let tools_value = request
                .request
                .tools
                .as_ref()
                .and_then(|tools| serde_json::to_value(tools).ok());
            let payload = json!({
                "model": &request.model,
                "contents": contents_value,
                "system_instruction": system_value.as_ref(),
                "tools": tools_value.as_ref(),
                "tool_config": &request.request.tool_config,
            });
            crate::alphacode_provider_core::fingerprint::log_provider_canonical_input(
                "antigravity",
                model,
                "gemini_generate_content",
                &payload,
                &content_items,
                system_value.as_ref(),
                tools_value.as_ref(),
                request.request.tools.as_ref().map(|tools| tools.len()),
                &[
                    (
                        "session_id_present",
                        request.request.session_id.is_some().to_string(),
                    ),
                    ("project_present", (!request.project.is_empty()).to_string()),
                ],
            );
        }

        let send_request = |url: String| {
            let client = self.client.clone();
            let access_token = tokens.access_token.clone();
            let project = request.project.clone();
            let payload = &request;
            async move {
                client
                    .post(&url)
                    .bearer_auth(&access_token)
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .header(reqwest::header::USER_AGENT, antigravity_user_agent())
                    .header("x-goog-api-client", X_GOOG_API_CLIENT)
                    .header("x-goog-request-params", format!("project={}", project))
                    .header("x-goog-client-metadata", client_metadata_header())
                    .json(payload)
                    .audited_send()
                    .await
                    .context("Failed to send Antigravity generateContent request")
            }
        };

        // Try each Antigravity backend endpoint (`daily-cloudcode-pa.sandbox` first,
        // then `cloudcode-pa` and `autopush`). When one endpoint is rate-limited
        // (`429` / `RESOURCE_EXHAUSTED`) or unreachable, fail over immediately to
        // the remaining endpoints before paying the exponential backoff delay.
        let mut attempt: u32 = 0;
        loop {
            let mut last_quota_error: Option<(
                String,
                Option<crate::alphacode_provider_core::retry_after::RetryAfter>,
            )> = None;
            let mut last_network_error: Option<anyhow::Error> = None;

            for base_url in ANTIGRAVITY_API_BASES {
                let endpoint_url = format!("{base_url}/v1internal:generateContent");
                let response = match send_request(endpoint_url).await {
                    Ok(resp) => resp,
                    Err(err) => {
                        last_network_error = Some(err);
                        continue;
                    }
                };

                if response.status().is_success() {
                    return response
                        .json()
                        .await
                        .context("Failed to decode Antigravity generateContent response");
                }

                let status = response.status();
                let headers = response.headers().clone();
                let body =
                    crate::alphacode_base::util::http_error_body(response, "HTTP error").await;
                let body_trimmed = body.trim().to_string();
                let is_quota =
                    status.as_u16() == 429 || body_trimmed.contains("RESOURCE_EXHAUSTED");
                let hint = crate::alphacode_provider_core::retry_after::retry_after(&headers);

                if !is_quota {
                    return Err(
                        crate::alphacode_provider_core::retry_after::error_with_retry_after(
                            format!(
                                "Antigravity generateContent failed (HTTP {}): {}",
                                status, body_trimmed
                            ),
                            hint,
                        ),
                    );
                }

                last_quota_error = Some((
                    format!(
                        "Antigravity generateContent failed (HTTP {}): {}",
                        status, body_trimmed
                    ),
                    hint,
                ));
            }

            if let Some((err_msg, hint)) = last_quota_error {
                let long_cooldown = hint.as_ref().is_some_and(|h| {
                    h.remaining() > crate::alphacode_provider_core::retry_after::MAX_AUTOMATIC_WAIT
                });
                if attempt + 1 >= MAX_RETRIES || long_cooldown {
                    return Err(
                        crate::alphacode_provider_core::retry_after::error_with_retry_after(
                            err_msg, hint,
                        ),
                    );
                }
                let server_hint = hint.as_ref().map(|h| h.remaining());
                let delay = crate::alphacode_provider_core::retry::backoff_for(
                    crate::alphacode_provider_core::retry::RetryReason::RateLimited,
                    attempt,
                    server_hint,
                );
                crate::alphacode_base::logging::info(&format!(
                    "Antigravity quota exhausted across endpoints; retrying in {:?} (attempt {}/{})",
                    delay,
                    attempt + 2,
                    MAX_RETRIES,
                ));
                tokio::time::sleep(delay).await;
                attempt += 1;
                continue;
            }

            return Err(last_network_error.unwrap_or_else(|| {
                anyhow::anyhow!("Failed to send Antigravity generateContent request")
            }));
        }
    }
}

impl Default for AntigravityProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Provider for AntigravityProvider {
    async fn complete(
        &self,
        messages: &[Message],
        _tools: &[ToolDefinition],
        system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<EventStream> {
        let model = self
            .model
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        let messages = messages.to_vec();
        let tools = _tools.to_vec();
        let system = system.to_string();
        let resume_session_id = _resume_session_id.map(str::to_string);
        let provider = self.clone();
        let (tx, rx) = mpsc::channel::<Result<crate::alphacode_message_types::StreamEvent>>(100);

        crate::alphacode_provider_core::request_audit::spawn(async move {
            let _ = tx
                .send(Ok(StreamEvent::ConnectionType {
                    connection: "https".to_string(),
                }))
                .await;
            let _ = tx
                .send(Ok(StreamEvent::ConnectionPhase {
                    phase: ConnectionPhase::Authenticating,
                }))
                .await;
            let _ = tx
                .send(Ok(StreamEvent::ConnectionPhase {
                    phase: ConnectionPhase::WaitingForResponse,
                }))
                .await;
            // Gemini-3 function calls carry an opaque `thoughtSignature` that the
            // Cloud Code backend requires on replay. `build_contents` carries the
            // most recent real signature forward, but a history where *nothing*
            // was ever signed (imported/pre-signature sessions, or synthesized
            // calls from a route that never emits signatures) has none to carry,
            // and 400s on every single turn with no way out (issues #482, #518).
            // Self-heal the way the Anthropic runtime does for rejected reasoning
            // fields: on that specific error, retry once with tool calls
            // downgraded to plain text. Content is preserved, the turn completes,
            // and the model re-signs its new calls.
            let mut signature_policy =
                crate::alphacode_provider_gemini::SignaturePolicy::ReplayCarriedForward;
            let response = match provider
                .generate_content(
                    &model,
                    &messages,
                    &tools,
                    &system,
                    resume_session_id.as_deref(),
                    false,
                    signature_policy,
                )
                .await
            {
                Ok(response) => response,
                Err(err) => {
                    if !crate::alphacode_provider_gemini::is_missing_thought_signature_error(
                        &err.to_string(),
                    ) {
                        let _ = tx.send(Err(err)).await;
                        return;
                    }
                    crate::alphacode_base::logging::warn(
                        "Antigravity rejected unsigned function calls; retrying with tool calls downgraded to text",
                    );
                    signature_policy =
                        crate::alphacode_provider_gemini::SignaturePolicy::DowngradeToolCallsToText;
                    match provider
                        .generate_content(
                            &model,
                            &messages,
                            &tools,
                            &system,
                            resume_session_id.as_deref(),
                            false,
                            signature_policy,
                        )
                        .await
                    {
                        Ok(response) => response,
                        Err(retry_err) => {
                            let _ = tx.send(Err(retry_err)).await;
                            return;
                        }
                    }
                }
            };
            // Gemini-3 thinking models intermittently return an empty
            // `MALFORMED_FUNCTION_CALL` turn (pseudo-code instead of a clean
            // functionCall). It is transient, so transparently re-request a few
            // times before surfacing it; this turns a frequent hard failure into a
            // near-always-successful turn without the agent loop seeing the blip.
            // The retries force function-calling mode `ANY` so the model must emit
            // a real functionCall rather than the pseudo-code that failed.
            let mut response = response;
            let mut malformed_retries = 0u8;
            const MAX_MALFORMED_RETRIES: u8 = 2;
            while is_retryable_empty_turn(&response) && malformed_retries < MAX_MALFORMED_RETRIES {
                malformed_retries += 1;
                match provider
                    .generate_content(
                        &model,
                        &messages,
                        &tools,
                        &system,
                        resume_session_id.as_deref(),
                        true,
                        signature_policy,
                    )
                    .await
                {
                    Ok(retried) => response = retried,
                    Err(err) => {
                        let _ = tx.send(Err(err)).await;
                        return;
                    }
                }
            }
            let _ = tx
                .send(Ok(StreamEvent::ConnectionPhase {
                    phase: ConnectionPhase::Streaming,
                }))
                .await;
            if let Some(usage) = response
                .response
                .as_ref()
                .and_then(|r| r.usage_metadata.as_ref())
            {
                let _ = tx
                    .send(Ok(StreamEvent::TokenUsage {
                        input_tokens: usage.prompt_token_count,
                        output_tokens: usage.candidates_token_count,
                        cache_read_input_tokens: usage.cached_content_token_count,
                        cache_creation_input_tokens: None,
                    }))
                    .await;
            }
            let Some(candidate) = response
                .response
                .and_then(|r| r.candidates)
                .and_then(|mut c| c.drain(..).next())
            else {
                let _ = tx
                    .send(Err(anyhow::anyhow!(
                        "Antigravity returned no candidates for generateContent"
                    )))
                    .await;
                return;
            };
            // Track whether this candidate produced any usable output (text or a
            // tool call). Gemini-3 thinking models intermittently emit Python-style
            // pseudo-code instead of a clean functionCall and finish with
            // `MALFORMED_FUNCTION_CALL` (or a bare `OTHER`) and empty content. If we
            // silently end the turn the agent loop looks like it stalled with no
            // answer, so we surface an actionable error below instead.
            let mut produced_output = false;
            if let Some(content) = candidate.content {
                // Gemini 3 attaches a `thoughtSignature` to function-call parts
                // (and occasionally to a standalone preceding part). Emit tool
                // calls through the standard ToolUseStart/End path so alphacode
                // drives the multi-turn loop, and replay the signature via a
                // dedicated ToolUseSignature event so it can be persisted on the
                // ToolUse block and resent on later turns (required by the
                // Cloud Code backend, which rejects function calls missing it).
                let mut pending_signature: Option<String> = None;
                for part in content.parts {
                    let part_signature = part
                        .thought_signature
                        .as_ref()
                        .filter(|sig| !sig.is_empty())
                        .cloned();
                    if let Some(text) = part.text.filter(|text| !text.is_empty()) {
                        produced_output = true;
                        let _ = tx.send(Ok(StreamEvent::TextDelta(text))).await;
                    }
                    if let Some(function_call) = part.function_call {
                        produced_output = true;
                        let signature = part_signature.clone().or_else(|| pending_signature.take());
                        let raw_call_id = function_call
                            .id
                            .clone()
                            .unwrap_or_else(|| Uuid::new_v4().to_string());
                        let call_id =
                            crate::alphacode_message_types::sanitize_tool_id(&raw_call_id);
                        let _ = tx
                            .send(Ok(StreamEvent::ToolUseStart {
                                id: call_id,
                                name: function_call.name,
                            }))
                            .await;
                        let _ = tx
                            .send(Ok(StreamEvent::ToolInputDelta(
                                function_call.args.to_string(),
                            )))
                            .await;
                        let _ = tx.send(Ok(StreamEvent::ToolUseEnd)).await;
                        if let Some(signature) = signature {
                            let _ = tx.send(Ok(StreamEvent::ToolUseSignature(signature))).await;
                        }
                    } else if let Some(signature) = part_signature {
                        // Standalone signature part; remember it for the next
                        // function call in this candidate.
                        pending_signature = Some(signature);
                    }
                }
                // A thought signature that was never consumed by a following
                // function call (e.g. a pure-text reasoning turn) is still an
                // opaque reasoning signal. Surface it as a ThinkingSignatureDelta
                // rather than dropping it, so reasoning-aware consumers (and the
                // provider-doctor reasoning probe) can see the model reasoned.
                if let Some(signature) = pending_signature.take() {
                    let _ = tx
                        .send(Ok(StreamEvent::ThinkingSignatureDelta(signature)))
                        .await;
                }
            }

            // An abnormal finish (typically Gemini-3's intermittent
            // `MALFORMED_FUNCTION_CALL`, where the model writes pseudo-code rather
            // than a valid functionCall) that yielded no text and no tool call is a
            // dead turn: surface it as a retryable error instead of a silent empty
            // `MessageEnd` that looks like the agent gave up. `STOP`/`MAX_TOKENS`
            // are normal terminal reasons and are left to flow through as usual.
            if !produced_output {
                let abnormal = candidate
                    .finish_reason
                    .as_deref()
                    .map(|reason| {
                        !matches!(
                            reason.to_ascii_uppercase().as_str(),
                            "STOP" | "MAX_TOKENS" | "FINISH_REASON_UNSPECIFIED" | ""
                        )
                    })
                    .unwrap_or(false);
                if abnormal {
                    let reason = candidate.finish_reason.as_deref().unwrap_or("unknown");
                    let detail = candidate
                        .finish_message
                        .as_deref()
                        .filter(|msg| !msg.trim().is_empty())
                        .map(|msg| {
                            format!(
                                ": {}",
                                crate::alphacode_base::util::truncate_str(msg.trim(), 300)
                            )
                        })
                        .unwrap_or_default();
                    let _ = tx
                        .send(Err(anyhow::anyhow!(
                            "Antigravity returned no usable output (finish_reason={reason}){detail}"
                        )))
                        .await;
                    return;
                }
            }

            let _ = tx
                .send(Ok(StreamEvent::MessageEnd {
                    stop_reason: candidate.finish_reason.clone(),
                }))
                .await;
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }

    fn name(&self) -> &'static str {
        "antigravity"
    }

    fn model(&self) -> String {
        self.model
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn set_model(&self, model: &str) -> Result<()> {
        let trimmed = model.trim();
        if trimmed.is_empty() {
            anyhow::bail!("Antigravity model cannot be empty");
        }
        *self
            .model
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = trimmed.to_string();
        Ok(())
    }

    fn available_models(&self) -> Vec<&'static str> {
        AVAILABLE_MODELS.to_vec()
    }

    fn available_models_display(&self) -> Vec<String> {
        let catalog = self.fetched_catalog();
        let current_model = self.model();
        let custom_model = Some(current_model.trim())
            .filter(|model| !model.is_empty() && *model != DEFAULT_MODEL)
            .map(str::to_string);

        if catalog.is_empty() {
            return merge_antigravity_model_ids(
                AVAILABLE_MODELS
                    .iter()
                    .copied()
                    .map(str::to_string)
                    .chain(custom_model),
            );
        }

        let merged = merge_antigravity_model_ids(
            catalog
                .into_iter()
                .map(|model| model.id)
                .chain(custom_model.clone()),
        );
        if merged.is_empty() {
            return merge_antigravity_model_ids(
                AVAILABLE_MODELS
                    .iter()
                    .copied()
                    .map(str::to_string)
                    .chain(custom_model),
            );
        }
        merged
    }

    fn available_models_for_switching(&self) -> Vec<String> {
        self.available_models_display()
    }

    fn model_routes(&self) -> Vec<crate::alphacode_provider_core::ModelRoute> {
        let catalog = self.fetched_catalog();
        if !catalog.is_empty() {
            let routes: Vec<crate::alphacode_provider_core::ModelRoute> = catalog
                .into_iter()
                .filter(|model| {
                    let trimmed = model.id.trim();
                    !trimmed.is_empty() && trimmed != DEFAULT_MODEL
                })
                .map(|model| crate::alphacode_provider_core::ModelRoute {
                    model: model.id.clone(),
                    provider: "Antigravity".to_string(),
                    api_method: "https".to_string(),
                    available: model.available,
                    detail: catalog_model_detail(&model),
                    cheapness: None,
                })
                .collect();
            if !routes.is_empty() {
                return routes;
            }
        }

        self.available_models_display()
            .into_iter()
            .map(|model| crate::alphacode_provider_core::ModelRoute {
                model,
                provider: "Antigravity".to_string(),
                api_method: "https".to_string(),
                available: true,
                detail: "fallback catalog".to_string(),
                cheapness: None,
            })
            .collect()
    }

    fn on_auth_changed(&self) {
        let provider = self.clone();
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn(async move {
                if provider.prefetch_models().await.is_ok() {
                    crate::alphacode_base::bus::Bus::global().publish_models_updated();
                }
            });
        }
    }

    async fn prefetch_models(&self) -> Result<()> {
        match self.fetch_available_models().await {
            Ok(snapshot) => {
                if !snapshot.models.is_empty() {
                    crate::alphacode_base::logging::info(&format!(
                        "Discovered Antigravity models: {}{}",
                        snapshot
                            .models
                            .iter()
                            .map(|model| model.id.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                        snapshot
                            .default_model_id
                            .as_deref()
                            .map(|id| format!(" (default: {id})"))
                            .unwrap_or_default()
                    ));
                    Self::persist_catalog(&snapshot);
                    if let Some(default_model_id) = snapshot.default_model_id.clone() {
                        *self
                            .backend_default_model
                            .write()
                            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
                            Some(default_model_id);
                    }
                    *self
                        .fetched_catalog
                        .write()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = snapshot.models;
                }
            }
            Err(err) => {
                crate::alphacode_base::logging::warn(&format!(
                    "Antigravity model catalog refresh failed; keeping fallback list: {}",
                    err
                ));
            }
        }

        Ok(())
    }

    fn supports_compaction(&self) -> bool {
        false
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(Self {
            client: self.client.clone(),
            model: Arc::new(RwLock::new(self.model())),
            fetched_catalog: self.fetched_catalog.clone(),
            backend_default_model: self.backend_default_model.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_message_types::{ContentBlock, Role};
    use std::collections::HashSet;

    fn empty_antigravity_provider() -> AntigravityProvider {
        AntigravityProvider {
            client: crate::alphacode_provider_core::shared_http_client(),
            model: Arc::new(RwLock::new(DEFAULT_MODEL.to_string())),
            fetched_catalog: Arc::new(RwLock::new(Vec::new())),
            backend_default_model: Arc::new(RwLock::new(None)),
        }
    }

    fn assert_proto_safe_schema(value: &Value, path: &str) {
        const FORBIDDEN_KEYS: &[&str] = &[
            "additionalProperties",
            "patternProperties",
            "unevaluatedProperties",
            "propertyNames",
            "dependentRequired",
            "dependentSchemas",
            "contains",
            "minContains",
            "maxContains",
            "exclusiveMinimum",
            "exclusiveMaximum",
            "multipleOf",
            "examples",
            "if",
            "then",
            "else",
            "readOnly",
            "writeOnly",
            "deprecated",
            "contentMediaType",
            "contentEncoding",
            "$schema",
            "$id",
            "$ref",
            "$defs",
            "$anchor",
            "$dynamicRef",
            "$dynamicAnchor",
            "$vocabulary",
            "definitions",
            "$comment",
        ];

        match value {
            Value::Object(map) => {
                for (key, child) in map {
                    assert!(
                        !FORBIDDEN_KEYS.contains(&key.as_str()),
                        "forbidden schema keyword `{key}` at {path}"
                    );
                    let child_path = format!("{path}.{key}");
                    if key == "type" && !child.is_object() {
                        let ty = child.as_str().unwrap_or_else(|| {
                            panic!(
                                "non-string schema `type` at {child_path}: {child} (Cloud Code rejects arrays with Unknown name \"type\")"
                            )
                        });
                        assert!(
                            matches!(
                                ty,
                                "string" | "number" | "integer" | "boolean" | "array" | "object"
                            ),
                            "invalid OpenAPI schema `type` `{ty}` at {child_path}"
                        );
                    }
                    if key == "enum"
                        && let Some(items) = child.as_array()
                    {
                        for item in items {
                            assert!(!item.is_null(), "null entry inside `enum` at {child_path}");
                        }
                    }
                    assert_proto_safe_schema(child, &child_path);
                }
            }
            Value::Array(items) => {
                for (idx, item) in items.iter().enumerate() {
                    assert_proto_safe_schema(item, &format!("{path}[{idx}]"));
                }
            }
            _ => {}
        }
    }

    #[test]
    fn empty_catalog_exposes_fallback_models_and_never_collapses_to_default() {
        let provider = empty_antigravity_provider();

        let display = provider.available_models_display();
        assert_eq!(
            display,
            AVAILABLE_MODELS
                .iter()
                .map(|m| (*m).to_string())
                .collect::<Vec<_>>()
        );
        assert!(
            !display.iter().any(|m| m == "default"),
            "fallback display models must not advertise raw `default`: {display:?}"
        );

        let routes = provider.model_routes();
        assert_eq!(routes.len(), AVAILABLE_MODELS.len());
        assert!(routes.iter().all(|r| r.model != "default" && r.available));
        assert_eq!(
            provider.resolve_model_for_request("default"),
            DEFAULT_FALLBACK_MODEL
        );
    }

    #[test]
    fn populated_catalog_exposes_live_models_without_default_sentinel() {
        let provider = empty_antigravity_provider();
        *provider.backend_default_model.write().unwrap() = Some("claude-sonnet-4-6".to_string());
        *provider.fetched_catalog.write().unwrap() = vec![
            CatalogModel {
                id: "claude-sonnet-4-6".to_string(),
                display_name: Some("Claude Sonnet 4.6".to_string()),
                reset_time: None,
                tag_title: None,
                model_provider: Some("MODEL_PROVIDER_ANTHROPIC".to_string()),
                max_tokens: Some(250_000),
                max_output_tokens: Some(64_000),
                recommended: true,
                available: true,
                remaining_fraction_milli: Some(1000),
            },
            CatalogModel {
                id: "gemini-3-flash".to_string(),
                display_name: Some("Gemini 3 Flash".to_string()),
                reset_time: None,
                tag_title: None,
                model_provider: Some("MODEL_PROVIDER_GOOGLE".to_string()),
                max_tokens: Some(1_048_576),
                max_output_tokens: Some(65_536),
                recommended: true,
                available: true,
                remaining_fraction_milli: Some(1000),
            },
        ];

        let display = provider.available_models_display();
        assert_eq!(display, vec!["claude-sonnet-4-6", "gemini-3-flash"]);
        assert!(!display.iter().any(|m| m == "default"));

        let routes = provider.model_routes();
        assert_eq!(routes.len(), 2);
        assert!(routes.iter().all(|r| r.model != "default"));
        assert_eq!(
            provider.resolve_model_for_request("default"),
            "claude-sonnet-4-6"
        );
    }

    #[tokio::test]
    async fn serialized_generate_content_with_actual_advertised_tools_is_proto_safe() {
        let provider = empty_antigravity_provider();
        let registry =
            crate::alphacode_app_core::tool::Registry::new(Arc::new(provider.clone())).await;
        let mut tools = registry
            .definitions_filtered(None, |name| {
                crate::alphacode_app_core::tool::disclosure::tier_for(name)
                    == crate::alphacode_app_core::tool::disclosure::ToolTier::Core
            })
            .await;
        // Include the `mcp` management tool so the full 14-tool GUI payload is exercised.
        let mcp_tool = crate::alphacode_app_core::tool::mcp::McpManagementTool::new(Arc::new(
            tokio::sync::RwLock::new(crate::alphacode_app_core::mcp::McpManager::new()),
        ));
        tools.push(crate::alphacode_tool_core::Tool::to_definition(&mcp_tool));
        assert_eq!(
            tools.len(),
            14,
            "expected the 14 default advertised tools (13 core + mcp)"
        );

        let tool_names: HashSet<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(tool_names.contains("bash"));
        assert!(tool_names.contains("todo"));
        assert!(tool_names.contains("alphagrep"));
        assert!(tool_names.contains("mcp"));

        let messages = vec![Message {
            role: Role::User,
            content: vec![ContentBlock::Text {
                text: "Hello".to_string(),
                cache_control: None,
            }],
            timestamp: None,
            tool_duration_ms: None,
        }];

        for requested_model in [
            "default",
            "gemini-3-flash",
            "claude-sonnet-4-6",
            "gpt-oss-120b-medium",
        ] {
            let req = provider.build_generate_content_request(
                requested_model,
                "test-project".to_string(),
                &messages,
                &tools,
                "You are helpful.",
                None,
                false,
                crate::alphacode_provider_gemini::SignaturePolicy::ReplayCarriedForward,
            );
            let serialized = serde_json::to_value(&req).expect("serialize generateContent");
            let decls = serialized["request"]["tools"][0]["functionDeclarations"]
                .as_array()
                .expect("functionDeclarations array");
            assert_eq!(decls.len(), 14);

            for decl in decls {
                let name = decl["name"].as_str().unwrap_or("<unnamed>");
                assert_proto_safe_schema(&decl["parameters"], &format!("{requested_model}.{name}"));
            }

            // Spot-check `bash` nullable array types became scalar + `nullable: true`.
            let bash_decl = decls
                .iter()
                .find(|d| d["name"] == "bash")
                .expect("bash decl");
            assert_eq!(
                bash_decl["parameters"]["properties"]["timeout"]["type"],
                "integer"
            );
            assert_eq!(
                bash_decl["parameters"]["properties"]["timeout"]["nullable"],
                true
            );

            // Spot-check `todo` nested `items.properties.confidence` became scalar + `nullable: true`.
            let todo_decl = decls
                .iter()
                .find(|d| d["name"] == "todo")
                .expect("todo decl");
            assert_eq!(
                todo_decl["parameters"]["properties"]["todos"]["items"]["properties"]["confidence"]
                    ["type"],
                "integer"
            );
            assert_eq!(
                todo_decl["parameters"]["properties"]["todos"]["items"]["properties"]["confidence"]
                    ["nullable"],
                true
            );

            // Spot-check `alphagrep` keeps its property named `type`.
            let alphagrep_decl = decls
                .iter()
                .find(|d| d["name"] == "alphagrep")
                .expect("alphagrep decl");
            assert_eq!(
                alphagrep_decl["parameters"]["properties"]["type"]["type"],
                "string"
            );
        }
    }
}
