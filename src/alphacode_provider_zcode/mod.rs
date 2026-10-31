//! Shared constants and model catalog for the ZCode (Z.ai coding plan)
//! provider. The model list mirrors the GLM models ZCode exposes on the
//! coding plan; ids are sent to the wire exactly as listed here.

pub const PROVIDER_ID: &str = "zcode";

pub const DISPLAY_NAME: &str = "ZCode (Z.ai Coding Plan)";

/// Anthropic-compatible Messages API endpoint for the coding plan. Matches
/// ZCode's own traffic: base `https://api.z.ai/api/anthropic` + `/v1/messages`.
pub const MESSAGES_URL: &str = "https://api.z.ai/api/anthropic/v1/messages";

/// Anthropic-compatible model catalog endpoint (`GET`, `x-api-key` auth).
pub const MODELS_URL: &str = "https://api.z.ai/api/anthropic/v1/models";

pub const DEFAULT_MODEL: &str = "GLM-5.3";

/// Models offered by the Z.ai coding plan, in picker preference order.
pub const MODELS: [&str; 10] = [
    "GLM-5.3",
    "GLM-5.3-Flash",
    "GLM-5.2",
    "GLM-5.1",
    "GLM-5",
    "GLM-5-Turbo",
    "GLM-4.7",
    "GLM-4.7-Flash",
    "GLM-4.6",
    "GLM-4.5",
];

/// Models that must not be auto-selected as the default (low-tier flashes).
const NON_DEFAULT_MODELS: [&str; 3] = ["GLM-5.3-Flash", "GLM-4.7-Flash", "GLM-5-Turbo"];

pub fn is_default_candidate(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    !NON_DEFAULT_MODELS.iter().any(|m| m.to_lowercase() == lower)
}

/// Normalize user-typed model ids to the official casing (zcode sends
/// `GLM-5.3` style ids on the wire).
pub fn normalize_model_id(model: &str) -> String {
    let lower = model.trim().to_ascii_lowercase();
    MODELS
        .iter()
        .find(|known| known.to_ascii_lowercase() == lower)
        .map(|known| (*known).to_string())
        .unwrap_or_else(|| model.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_common_aliases() {
        assert_eq!(normalize_model_id("glm-5.3"), "GLM-5.3");
        assert_eq!(normalize_model_id(" GLM-5.3-Flash "), "GLM-5.3-Flash");
        assert_eq!(normalize_model_id("glm-4.7-flash"), "GLM-4.7-Flash");
        // Unknown ids pass through untouched.
        assert_eq!(normalize_model_id("glm-9"), "glm-9");
    }

    #[test]
    fn default_model_is_flagship() {
        assert!(is_default_candidate(DEFAULT_MODEL));
        assert!(!is_default_candidate("GLM-5.3-Flash"));
    }
}
