//! Gemini pure protocol types and helpers (compatibility shim).
//!
//! The Gemini provider *runtime* (`GeminiProvider`) now lives in the
//! downstream `alphacode-provider-gemini-runtime` crate so provider edits do not
//! rebuild the base -> app-core -> tui spine. The binary's composition root
//! registers it via [`crate::provider::external`]. This module keeps the pure
//! request/response types and helpers (from `alphacode-provider-gemini`)
//! importable at their historical `crate::provider::gemini::*` paths.

pub use crate::alphacode_provider_gemini::{
    AVAILABLE_MODELS, CODE_ASSIST_API_VERSION, CODE_ASSIST_ENDPOINT, ClientMetadata,
    CodeAssistGenerateRequest, CodeAssistGenerateResponse, DEFAULT_MODEL, GEMINI_API_ENDPOINT,
    GEMINI_API_VERSION, GeminiCandidate, GeminiContent, GeminiFunctionCall,
    GeminiFunctionCallingConfig, GeminiFunctionDeclaration, GeminiFunctionResponse, GeminiPart,
    GeminiPromptFeedback, GeminiRuntimeState, GeminiTool, GeminiToolConfig, GeminiUsageMetadata,
    GeminiUserTier, IneligibleTier, InlineData, LoadCodeAssistRequest, LoadCodeAssistResponse,
    LongRunningOperationResponse, OnboardUserRequest, OnboardUserResponse, ProjectRef,
    USER_TIER_FREE, VertexGenerateContentRequest, VertexGenerateContentResponse, build_contents,
    build_system_instruction_with_tool_guard, build_tools, choose_onboard_tier, client_metadata,
    extract_gemini_model_ids, gemini_fallback_models, google_cloud_project_from_env,
    ineligible_or_project_error, is_gemini_model_id, load_code_assist_request,
    merge_gemini_model_lists, validate_load_code_assist_response,
};
