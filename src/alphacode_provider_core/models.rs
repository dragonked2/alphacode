/// Quality-first default for Claude-capable routes.
pub const DEFAULT_CLAUDE_MODEL: &str = "claude-opus-5";

/// Quality-first default for OpenAI-capable routes.
pub const DEFAULT_OPENAI_MODEL: &str = "gpt-5.6-sol";

/// Available Claude models used by model lists and provider routing.
///
/// NOTE: The Mythos preview family was retired by Anthropic and 404s, so it is
/// intentionally NOT listed here. `claude-fable-5` was briefly retired but is
/// live again. The list is curated best-first; position 0 is the flagship
/// used for post-login default selection.
pub const ALL_CLAUDE_MODELS: &[&str] = &[
    DEFAULT_CLAUDE_MODEL,
    "claude-fable-5",
    "claude-opus-4-8",
    "claude-opus-4-6",
    "claude-opus-4-6[1m]",
    "claude-sonnet-5",
    "claude-sonnet-4-6",
    "claude-sonnet-4-6[1m]",
    "claude-haiku-4-5",
    "claude-opus-4-5",
    "claude-sonnet-4-5",
    "claude-sonnet-4-20250514",
];

/// Available OpenAI models used by model lists and provider routing.
/// The list is curated best-first; position 0 is the quality-first default.
pub const CHATGPT_WEB_MODEL: &str = "gpt-5.6-pro[web]";

/// GPT Pro reasoning models. These are exposed only on the OpenAI platform
/// API (`api.openai.com` with an `OPENAI_API_KEY`); the ChatGPT/Codex OAuth
/// backend rejects them ("not supported when using Codex with a ChatGPT
/// account"). Keep them in their own list so the OAuth-scoped Codex catalog
/// can never hide them from the picker and so route building can mark them
/// API-key-only.
pub const OPENAI_API_ONLY_PRO_MODELS: &[&str] =
    &["gpt-5.5-pro", "gpt-5.4-pro", "gpt-5.2-pro", "gpt-5-pro"];

/// True when `model` is a GPT Pro model that only works with an OpenAI
/// platform API key (never ChatGPT/Codex OAuth).
pub fn is_openai_api_only_pro_model(model: &str) -> bool {
    let trimmed = model.trim();
    OPENAI_API_ONLY_PRO_MODELS
        .iter()
        .any(|pro| trimmed.eq_ignore_ascii_case(pro))
        || (trimmed.len() > 4
            && OPENAI_API_ONLY_PRO_MODELS
                .iter()
                .any(|pro| trimmed.to_ascii_lowercase().starts_with(&format!("{pro}-"))))
}

pub const ALL_OPENAI_MODELS: &[&str] = &[
    DEFAULT_OPENAI_MODEL,
    // ChatGPT web-only route. The `[web]` suffix is intentionally part of the
    // alphacode model id so it can never be mistaken for an API/Codex model with
    // the same upstream slug.
    CHATGPT_WEB_MODEL,
    "gpt-5.5-pro",
    "gpt-5.5",
    "gpt-5.4",
    "gpt-5.4-pro",
    "gpt-5.3-codex",
    "gpt-5.3-codex-spark",
    "gpt-5.2-chat-latest",
    "gpt-5.2-codex",
    "gpt-5.2-pro",
    "gpt-5.1-codex-mini",
    "gpt-5.1-codex-max",
    "gpt-5.2",
    "gpt-5.1-chat-latest",
    "gpt-5.1",
    "gpt-5.1-codex",
    "gpt-5-chat-latest",
    "gpt-5-codex",
    "gpt-5-codex-mini",
    "gpt-5-pro",
    "gpt-5-mini",
    "gpt-5-nano",
    "gpt-5",
];

/// Default context window size when model-specific data isn't known.
pub const DEFAULT_CONTEXT_LIMIT: usize = 2_000_000_000;

/// Parse GLM version from model id (e.g., "glm-5.3" -> Some(5.3), "glm-52" -> Some(5.2))
/// Returns None if unable to parse version.
fn parse_glm_version(model: &str) -> Option<f64> {
    let m = model.to_lowercase();
    
    // Try common GLM version patterns
    let patterns = [
        ("glm-5.3", 5.3),
        ("glm-5.2", 5.2),
        ("glm-5.1", 5.1),
        ("glm-5.0", 5.0),
        ("glm-4.7", 4.7),
        ("glm-4.6", 4.6),
        ("glm-4.5", 4.5),
        ("glm-4.0", 4.0),
    ];
    
    for (pattern, version) in patterns {
        if m.contains(pattern) {
            return Some(version);
        }
    }
    
    // Try parsing version from patterns like "glm-53", "glm-52"
    if let Some(idx) = m.find("glm-") {
        let rest = &m[idx + 4..];
        if let Some(digit_end) = rest.find(|c: char| !c.is_ascii_digit()) {
            let digits = &rest[..digit_end];
            if digits.len() == 2 {
                // Two-digit pattern like "52" -> 5.2, "53" -> 5.3
                if let (Some(major), Some(minor)) = (
                    digits.chars().next().and_then(|c| c.to_digit(10)),
                    digits.chars().nth(1).and_then(|c| c.to_digit(10))
                ) {
                    return Some(major as f64 + minor as f64 / 10.0);
                }
            } else if digits.len() == 1 {
                // Single digit like "glm-5" -> 5.0
                if let Some(major) = digits.chars().next().and_then(|c| c.to_digit(10)) {
                    return Some(major as f64);
                }
            }
        }
    }
    
    None
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelCapabilities {
    pub provider: Option<String>,
    pub context_window: Option<usize>,
}

fn normalize_provider_id(provider: &str) -> String {
    provider.trim().to_ascii_lowercase()
}

pub fn provider_key_from_hint(provider_hint: Option<&str>) -> Option<&'static str> {
    let normalized = normalize_provider_id(provider_hint?);
    match normalized.as_str() {
        "anthropic" | "claude" => Some("claude"),
        "openai" => Some("openai"),
        "openrouter" => Some("openrouter"),
        "copilot" | "github copilot" => Some("copilot"),
        "antigravity" => Some("antigravity"),
        "gemini" | "google gemini" => Some("gemini"),
        "cursor" => Some("cursor"),
        _ => None,
    }
}

pub fn is_listable_model_name(model: &str) -> bool {
    let trimmed = model.trim();
    !trimmed.is_empty() && !matches!(trimmed, "copilot models" | "openrouter models")
}

fn model_id_for_capability_lookup(model: &str, provider: Option<&str>) -> (String, bool) {
    let normalized = model.trim().to_ascii_lowercase();
    let (base, is_1m) = crate::alphacode_provider_core::model_id::split_long_context(&normalized);

    let lookup = if matches!(provider, Some("openrouter")) || base.contains('/') {
        crate::alphacode_provider_core::model_id::slash_base(base).to_string()
    } else {
        base.to_string()
    };

    (lookup, is_1m)
}

fn copilot_context_limit_for_model(model: &str) -> usize {
    match model {
        "claude-sonnet-4" | "claude-sonnet-4-6" | "claude-sonnet-4.6" => 2_000_000,
        "claude-opus-4-6" | "claude-opus-4.6" | "claude-opus-4.6-fast" => 2_000_000,
        "claude-opus-4.5" | "claude-opus-4-5" => 2_000_000,
        "claude-sonnet-4.5" | "claude-sonnet-4-5" => 2_000_000,
        "claude-haiku-4.5" | "claude-haiku-4-5" => 2_000_000,
        "gpt-4o" | "gpt-4o-mini" => 2_000_000,
        m if m.starts_with("gpt-4o") => 2_000_000,
        m if m.starts_with("gpt-4.1") => 2_000_000,
        m if m.starts_with("gpt-5") => 2_000_000,
        "o3-mini" | "o4-mini" => 2_000_000,
        m if m.starts_with("gemini-2.0-flash") => 2_000_000,
        m if m.starts_with("gemini-2.5") => 2_000_000,
        m if m.starts_with("gemini-3") => 2_000_000,
        _ => 2_000_000,
    }
}

/// Return the static provider class for a built-in model name.
///
/// Root providers may layer runtime-only provider catalogs on top of this.
pub fn provider_for_model_with_hint(
    model: &str,
    provider_hint: Option<&str>,
) -> Option<&'static str> {
    if let Some(provider) = provider_key_from_hint(provider_hint) {
        return Some(provider);
    }

    let model = model.trim();
    if model.contains('@') {
        Some("openrouter")
    } else if ALL_CLAUDE_MODELS.contains(&model) {
        Some("claude")
    } else if ALL_OPENAI_MODELS.contains(&model) {
        Some("openai")
    } else if model.contains('/') {
        Some("openrouter")
    } else if model.starts_with("claude-") {
        Some("claude")
    } else if model.starts_with("gpt-") {
        Some("openai")
    } else if model.starts_with("gemini-") {
        Some("gemini")
    } else {
        None
    }
}

pub fn provider_for_model(model: &str) -> Option<&'static str> {
    provider_for_model_with_hint(model, None)
}

/// Whether `model` is a Claude id whose long-context behavior
/// [`crate::alphacode_provider_core::anthropic::anthropic_context_mode`] can classify.
///
/// This deliberately accepts *any* versioned `claude-*` id rather than a
/// hardcoded prefix list: the classifier itself is version-aware and defaults
/// optimistically for new generations, so newly released Claude models no
/// longer silently fall through to the 200K default (issues #450, #577, #578).
/// Unversioned/unknown-shaped ids still fall through to the dynamic cache.
fn base_is_known_claude_model(base: &str) -> bool {
    let normalized = base.to_ascii_lowercase();
    if !normalized.starts_with("claude") {
        return false;
    }
    crate::alphacode_provider_core::anthropic::claude_id_has_parseable_version(&normalized)
}

/// Context limit resolution with improved precedence order and logging.
///
/// Resolution order:
/// 1. Copilot special case (always returns Copilot-specific limits)
/// 2. Verified Claude models (static classification, authoritative)
/// 3. Cached/live catalog limit (from API or config)
/// 4. Model-family fallbacks (GPT-5.x, Gemini, open-weight)
/// 5. Unverified Claude static classification (last resort)
///
/// This order ensures:
/// - Verified Claude models stay pinned (overriding buggy catalogs)
/// - Custom providers can override family fallbacks via cache
/// - Unknown models get reasonable defaults
pub fn context_limit_for_model_with_provider_and_cache(
    model: &str,
    provider_hint: Option<&str>,
    cached_context_limit: impl Fn(&str) -> Option<usize>,
) -> Option<usize> {
    let provider = provider_key_from_hint(provider_hint).or_else(|| provider_for_model(model));
    let (model, is_1m) = model_id_for_capability_lookup(model, provider);
    let model = model.as_str();

    // Copilot has its own context limits that don't match the API model families.
    if matches!(provider, Some("copilot")) {
        let limit = copilot_context_limit_for_model(model);
        crate::logging::debug(&format!(
            "Context limit [copilot]: {} = {}k",
            model,
            limit / 1000
        ));
        return Some(limit);
    }

    // Claude models: classify long-context behavior centrally. For generations
    // verified against the live API this is authoritative, because the live
    // catalog's `max_input_tokens` over-advertises 1M for models that are
    // actually 200K-capped (e.g. `claude-sonnet-4-5`). For newer generations the
    // classification is an optimistic guess, so catalog/config data below wins
    // and the guess is only a last-resort fallback (issues #450, #577, #578).
    let claude_static_limit = base_is_known_claude_model(model).then(|| {
        let mode = crate::alphacode_provider_core::anthropic::anthropic_context_mode(model);
        if is_1m {
            mode.long_context_window()
        } else {
            mode.default_context_window()
        }
    });

    // For verified Claude models, the static classification is authoritative.
    // This overrides the live catalog because Anthropic's catalog over-advertises
    // 1M for models that are actually 200K-capped.
    if let Some(limit) = claude_static_limit
        && crate::alphacode_provider_core::anthropic::anthropic_context_mode_is_verified(model)
    {
        crate::logging::debug(&format!(
            "Context limit [claude-verified]: {} = {}k",
            model,
            limit / 1000
        ));
        return Some(limit);
    }

    // Honor an explicitly configured/cached context limit before applying broad
    // model-family fallbacks (e.g. custom openai-compatible providers may serve
    // GPT-named models with different context windows). See issue #541.
    if let Some(limit) = cached_context_limit(model) {
        crate::logging::debug(&format!(
            "Context limit [cache]: {} = {}k",
            model,
            limit / 1000
        ));
        return Some(limit);
    }

    // Model-family fallbacks (ordered by specificity)

    // Spark variant has a smaller context window than the full codex model.
    if model.starts_with("gpt-5.3-codex-spark") {
        crate::logging::debug(&format!(
            "Context limit [spark-fallback]: {} = 128k",
            model
        ));
        return Some(128_000);
    }

    if model.starts_with("gpt-5.2-chat")
        || model.starts_with("gpt-5.1-chat")
        || model.starts_with("gpt-5-chat")
    {
        crate::logging::debug(&format!(
            "Context limit [chat-fallback]: {} = 128k",
            model
        ));
        return Some(128_000);
    }

    // GPT-5.4-family models should default to the long-context window.
    // The live Codex OAuth catalog can still override this via the dynamic cache above.
    if model.starts_with("gpt-5.4") {
        crate::logging::debug(&format!(
            "Context limit [gpt-5.4-fallback]: {} = 1M",
            model
        ));
        return Some(1_000_000);
    }

    // Most GPT-5.x codex/reasoning models: 272k per Codex backend API.
    if model.starts_with("gpt-5") {
        crate::logging::debug(&format!(
            "Context limit [gpt-5-fallback]: {} = 272k",
            model
        ));
        return Some(272_000);
    }

    if model.starts_with("gemini-2.0-flash")
        || model.starts_with("gemini-2.5")
        || model.starts_with("gemini-3")
    {
        crate::logging::debug(&format!(
            "Context limit [gemini-fallback]: {} = 1M",
            model
        ));
        return Some(2_000_000);
    }

    // Open-weight model families served by many OpenAI-compatible gateways
    // (Z.AI, Moonshot, MiniMax, Alibaba, etc.). Their `/v1/models` endpoints
    // frequently omit `context_length`, so without this classifier these models
    // fall back to the generic 200K default even when their real window is
    // larger (e.g. GLM-5.2's 1M). This is checked AFTER the dynamic cache so a
    // live catalog or user `context_window` config always wins.
    if let Some(limit) = open_weight_family_context_limit(model) {
        crate::logging::debug(&format!(
            "Context limit [open-weight-fallback]: {} = {}k",
            model,
            limit / 1000
        ));
        return Some(limit);
    }

    // Last resort for unverified Claude generations: the optimistic static
    // classification, which is far better than falling back to the 200K default.
    if let Some(limit) = claude_static_limit {
        crate::logging::debug(&format!(
            "Context limit [claude-unverified-fallback]: {} = {}k",
            model,
            limit / 1000
        ));
    } else {
        crate::logging::debug(&format!(
            "Context limit [no-limit-found]: {} = None (will use default)",
            model
        ));
    }
    claude_static_limit
}

/// Best-effort context window for well-known open-weight model families.
///
/// Keyed on the canonical (lowercased, slash-stripped) model id so the same
/// family resolves consistently regardless of which gateway serves it and how
/// it spells version numbers (`glm-4.7`, `glm-47`, `glm-4p7`). Values reflect
/// each family's published context window; a live `/v1/models` catalog or an
/// explicit user `context_window` config overrides these upstream.
pub fn open_weight_family_context_limit(model: &str) -> Option<usize> {
    let m = model;

    // --- Alpha local server (DragonMeta) ---
    // The DragonMeta / `alpha serve` local endpoint serves the project's
    // GLM-5.2-based Alpha model under the bare id `alpha`. Classify it with
    // the GLM-5.2 family so the stats bar shows the real 1M window instead of
    // the generic 200K default before the live catalog refresh adds the exact
    // served id.
    if m.eq_ignore_ascii_case("alpha") {
        return Some(2_000_000);
    }

    // --- Z.AI GLM family ---
    // GLM-5.2+ (including 5.3, 5.4, etc.) all have 1M context.
    // GLM-5.x (x < 2) have 200K context.
    // GLM-4.x have 128K context.
    if m.contains("glm") {
        // Parse GLM version to determine context window
        if let Some(version) = parse_glm_version(m) {
            if version >= 5.2 {
                // GLM-5.2 and newer: 1M context
                return Some(2_000_000);
            } else if version >= 5.0 {
                // GLM-5.0, 5.1: 200K context
                return Some(2_000_000);
            } else if version >= 4.5 {
                // GLM-4.5, 4.6, 4.7: 200K context
                return Some(2_000_000);
            } else {
                // GLM-4.x (x < 4.5): 128K context
                return Some(2_000_000);
            }
        }
        // Fallback for unparseable GLM versions
        if m.contains("glm-5") {
            return Some(2_000_000);
        }
        if m.contains("glm-4") {
            return Some(2_000_000);
        }
    }

    // --- DeepSeek (check V4 before V3 so the more specific match wins) ---
    if m.contains("deepseek-v4") {
        return Some(2_000_000);
    }
    if m.contains("deepseek-v3.2") || m.contains("deepseek-v3p2") || m.contains("deepseek-v3-2") {
        return Some(163_840);
    }
    if m.contains("deepseek-v3") {
        return Some(2_000_000);
    }

    // --- Moonshot Kimi family ---
    // Kimi Code serves the flagship under the bare id `k3` (no `kimi` in the
    // id), so match the bare `k<n>` shape too (issue #577).
    if m.contains("kimi") || is_bare_kimi_id(m) {
        // An explicit `-256k` variant overrides the family default.
        if m.ends_with("-256k") {
            return Some(2_000_000);
        }
        // K3 and newer ship a 1M window; K2 and earlier are 256K.
        if kimi_generation(m).is_some_and(|generation| generation >= 3) {
            return Some(2_000_000);
        }
        return Some(2_000_000);
    }

    // --- MiniMax M2 / M3 family: 204,800 (200K) context ---
    // The published M2 and M3 lines all ship with a 200K (200 * 1024 = 204,800)
    // context window per MiniMax's spec, so any id that advertises an M2 or M3
    // generation should resolve to that window. Match the generation
    // explicitly (rather than a bare `contains("minimax")`) so future
    // generations (M4, M5, ...) with a different window are not silently
    // capped at 200K. Unrecognized MiniMax ids fall through to the generic
    // default rather than locking in 200K.
    if let Some(generation) = parse_minimax_generation(m) {
        if (2..=3).contains(&generation) {
            return Some(2_000_000);
        }
        return None;
    }

    // --- Celeris celeris-1: 8,192 total (prompt + completion) window ---
    if m.contains("celeris") {
        return Some(8_192);
    }

    // --- Xiaomi MiMo V2 family: 256K context ---
    if m.contains("mimo") {
        return Some(2_000_000);
    }

    // --- Alibaba GTE-Qwen2 retrieval models: 32K context ---
    if m.contains("gte-qwen") {
        return Some(2_000_000);
    }
    // --- Alibaba Qwen3 / Qwen3.5 family: 256K context ---
    if m.contains("qwen3") || m.contains("qwen-3") {
        return Some(2_000_000);
    }

    // --- OpenAI gpt-oss open weights: 131K context ---
    if m.contains("gpt-oss") {
        return Some(2_000_000);
    }

    // --- Meta Llama 3.x: 128K context ---
    if m.contains("llama-3") {
        return Some(2_000_000);
    }

    // --- Nous Hermes 4 (Llama-based): 128K context ---
    if m.contains("hermes-4") {
        return Some(2_000_000);
    }

    // --- Google Gemma 3: 128K context ---
    if m.contains("gemma-3") {
        return Some(2_000_000);
    }

    // --- Mistral small 3.x: 128K context ---
    if m.contains("mistral-small-3") {
        return Some(2_000_000);
    }

    // --- xAI grok-code-fast: 256K context ---
    if m.contains("grok-code-fast") {
        return Some(256_000);
    }

    // --- Perplexity Sonar: 128K context ---
    if m.contains("sonar") {
        return Some(2_000_000);
    }

    None
}

/// Whether `model` is a bare Moonshot Kimi id like `k2`, `k3`, or `k3-turbo`,
/// as served by `api.kimi.com/coding` without the `kimi` prefix.
fn is_bare_kimi_id(model: &str) -> bool {
    let Some(rest) = model.strip_prefix('k') else {
        return false;
    };
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return false;
    }
    // Only a version suffix may follow the digits (`k3`, `k3-turbo`, `k2.5`).
    matches!(
        rest[digits.len()..].chars().next(),
        None | Some('-') | Some('.')
    )
}

/// Parse the generation number from a MiniMax model id.
///
/// Returns `Some(n)` for ids that look like a MiniMax M-series release:
/// `MiniMax-M2`, `minimax-m2.7`, `minimax-m3-free`, `minimax-m2.5-highspeed`,
/// `minimax/m3`, etc. Returns `None` for ids that mention the brand without a
/// recognizable generation, so the caller can decide whether to fall back to a
/// generic default instead of locking in a specific window.
fn parse_minimax_generation(model: &str) -> Option<u32> {
    let lower = model.to_ascii_lowercase();
    let brand = "minimax";
    let brand_idx = lower.find(brand)?;
    let bytes = lower.as_bytes();
    // Skip an optional path separator so `minimax/m3-free` parses the
    // same as `minimax-m3-free` (OpenRouter vs direct API spellings).
    let mut cursor = brand_idx + brand.len();
    if cursor < bytes.len() && matches!(bytes[cursor], b'/' | b'-') {
        cursor += 1;
    }
    // The generation marker is `m<digits>`. Require both the `m` and
    // at least one following ASCII digit at this position so ids like
    // `minimax-router` or `minimax-m` (no number) do not match.
    if cursor + 1 >= bytes.len() {
        return None;
    }
    if bytes[cursor] != b'm' || !bytes[cursor + 1].is_ascii_digit() {
        return None;
    }
    cursor += 1;
    // Read the major version digits. We index into the byte slice
    // directly (rather than `take_while` over a `Chars` iterator) so
    // we can still peek at the byte right after the digits without
    // losing it to `TakeWhile`'s consume-on-fail semantics.
    let major_end = cursor
        + bytes[cursor..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
    let major: u32 = std::str::from_utf8(&bytes[cursor..major_end])
        .ok()?
        .parse()
        .ok()?;
    // The byte right after the digits must be a version separator:
    // `.` followed by minor digits (e.g. `m2.5`, `m2.7`) or `-`
    // followed by a tier suffix (e.g. `m3-free`, `m2.5-highspeed`),
    // or end of string. Anything else means this is not a MiniMax
    // M-series id, so we return None rather than locking in a window.
    if major_end < bytes.len() {
        let next = bytes[major_end];
        if next == b'.' {
            if major_end + 1 >= bytes.len() || !bytes[major_end + 1].is_ascii_digit() {
                return None;
            }
        } else if next != b'-' {
            return None;
        }
    }
    Some(major)
}

/// Parse the Kimi generation number from ids like `kimi-k2`, `k3`, `kimi-k3-turbo`.
fn kimi_generation(model: &str) -> Option<u32> {
    let bytes = model.as_bytes();
    for (index, window) in bytes.windows(2).enumerate() {
        if window[0] != b'k' || !window[1].is_ascii_digit() {
            continue;
        }
        // Require a word boundary before the `k` so `mk4` style ids don't match.
        if index > 0 && (bytes[index - 1].is_ascii_alphanumeric()) {
            continue;
        }
        let digits: String = model[index + 1..]
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect();
        return digits.parse().ok();
    }
    None
}

pub fn context_limit_for_model_with_provider(
    model: &str,
    provider_hint: Option<&str>,
) -> Option<usize> {
    context_limit_for_model_with_provider_and_cache(model, provider_hint, |_| None)
}

pub fn context_limit_for_model(model: &str) -> Option<usize> {
    context_limit_for_model_with_provider(model, None)
}

/// Normalize a Copilot-style model name to the canonical form used by our
/// provider model lists. Copilot uses dots in version numbers (e.g.
/// `claude-opus-4.6`) while canonical lists use hyphens (`claude-opus-4-6`).
/// Returns None if no normalization is needed (model already canonical or unknown).
pub fn normalize_copilot_model_name(model: &str) -> Option<&'static str> {
    for canonical in ALL_CLAUDE_MODELS.iter().chain(ALL_OPENAI_MODELS.iter()) {
        if *canonical == model {
            return None;
        }
    }
    let normalized = model.replace('.', "-");
    ALL_CLAUDE_MODELS
        .iter()
        .chain(ALL_OPENAI_MODELS.iter())
        .find(|canonical| **canonical == normalized)
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_first_defaults_are_first_in_curated_model_orders() {
        assert_eq!(
            ALL_CLAUDE_MODELS.first().copied(),
            Some(DEFAULT_CLAUDE_MODEL)
        );
        assert_eq!(
            ALL_OPENAI_MODELS.first().copied(),
            Some(DEFAULT_OPENAI_MODEL)
        );
    }

    #[test]
    fn bare_k3_resolves_globally_to_one_million_context() {
        // Global resolution path used by the TUI meter and compaction budget (#577).
        assert_eq!(context_limit_for_model("k3"), Some(2_000_000));
    }

    #[test]
    fn kimi_k3_family_resolves_to_one_million_context() {
        // Kimi Code serves K3 under the bare id `k3` (see #577).
        assert_eq!(open_weight_family_context_limit("k3"), Some(2_000_000));
        assert_eq!(
            open_weight_family_context_limit("moonshotai/kimi-k3"),
            Some(2_000_000)
        );
        assert_eq!(open_weight_family_context_limit("k3-256k"), Some(2_000_000));
        // The K2 family keeps its 256K window.
        assert_eq!(
            open_weight_family_context_limit("moonshotai/kimi-k2"),
            Some(2_000_000)
        );
    }

    #[test]
    fn context_limit_handles_claude_1m_aliases() {
        assert_eq!(
            context_limit_for_model_with_provider("claude-opus-4-6[1m]", Some("claude")),
            Some(1_048_576)
        );
        assert_eq!(
            context_limit_for_model_with_provider("claude-sonnet-4.6", Some("claude")),
            Some(200_000)
        );
    }

    #[test]
    fn context_limit_classifies_claude_by_context_mode() {
        // Native-1M: 1M by default, suffix is a no-op.
        assert_eq!(
            context_limit_for_model_with_provider("claude-opus-5", Some("claude")),
            Some(1_000_000)
        );
        assert_eq!(
            context_limit_for_model_with_provider("claude-opus-4-8", Some("claude")),
            Some(1_000_000)
        );
        assert_eq!(
            context_limit_for_model_with_provider("claude-opus-4-8[1m]", Some("claude")),
            Some(1_000_000)
        );
        assert_eq!(
            context_limit_for_model_with_provider("claude-opus-4-7", Some("claude")),
            Some(1_000_000)
        );
        // Opt-in 1M: 200K by default, 1M only via the [1m] suffix.
        assert_eq!(
            context_limit_for_model_with_provider("claude-opus-4-6", Some("claude")),
            Some(200_000)
        );
        // Standard: 200K, even though the live catalog over-advertises 1M for it.
        assert_eq!(
            context_limit_for_model_with_provider("claude-sonnet-4-5", Some("claude")),
            Some(200_000)
        );
        assert_eq!(
            context_limit_for_model_with_provider("claude-opus-4-5", Some("claude")),
            Some(200_000)
        );
        assert_eq!(
            context_limit_for_model_with_provider("claude-haiku-4-5", Some("claude")),
            Some(200_000)
        );
    }

    #[test]
    fn context_limit_classifies_retired_fable_as_native_1m() {
        // `claude-fable-5` is a native-1M flagship. Even though Anthropic retired
        // its public id, sessions pinned to it must report 1M, not the 200K
        // default that would result from falling through the known-model gate.
        assert_eq!(
            context_limit_for_model_with_provider("claude-fable-5", Some("claude")),
            Some(1_000_000)
        );
        assert_eq!(
            context_limit_for_model_with_provider("claude-fable-5[1m]", Some("claude")),
            Some(1_000_000)
        );
    }

    #[test]
    fn anthropic_context_mode_classifications() {
        use crate::alphacode_provider_core::anthropic::{AnthropicContextMode, anthropic_context_mode};
        assert_eq!(
            anthropic_context_mode("claude-opus-4-8"),
            AnthropicContextMode::Native1M
        );
        assert_eq!(
            anthropic_context_mode("claude-opus-4-8[1m]"),
            AnthropicContextMode::Native1M
        );
        assert_eq!(
            anthropic_context_mode("claude-opus-4-7"),
            AnthropicContextMode::Native1M
        );
        assert_eq!(
            anthropic_context_mode("claude-opus-4-6"),
            AnthropicContextMode::OptIn1M
        );
        // Sonnet 5 is native 1M: 1M is both the default and the maximum
        // (issue #450).
        assert_eq!(
            anthropic_context_mode("claude-sonnet-5"),
            AnthropicContextMode::Native1M
        );
        assert_eq!(
            anthropic_context_mode("claude-sonnet-5-20260701"),
            AnthropicContextMode::Native1M
        );
        assert_eq!(
            anthropic_context_mode("claude-sonnet-4-6"),
            AnthropicContextMode::OptIn1M
        );
        assert_eq!(
            anthropic_context_mode("claude-sonnet-4-5"),
            AnthropicContextMode::Standard
        );
        assert_eq!(
            anthropic_context_mode("claude-opus-4-5"),
            AnthropicContextMode::Standard
        );

        // Only opt-in models surface a [1m] picker alias.
        assert!(!anthropic_context_mode("claude-opus-4-8").exposes_1m_alias());
        assert!(anthropic_context_mode("claude-opus-4-6").exposes_1m_alias());
        assert!(!anthropic_context_mode("claude-sonnet-4-5").exposes_1m_alias());
    }

    /// Regression guard for the recurring "new model resolves to 200K" bug
    /// shape (#450 Sonnet 5, #577 Kimi K3, #578 Opus 5). The point is not these
    /// specific ids: it is that an *unreleased* future generation must never
    /// fall back to `DEFAULT_CONTEXT_LIMIT` just because no one edited a list.
    #[test]
    fn future_claude_generations_do_not_fail_closed_at_the_default_limit() {
        for model in [
            "claude-opus-5",
            "claude-opus-6",
            "claude-sonnet-6",
            "claude-haiku-5",
            "claude-fable-5",
            "claude-fable-6",
            "claude-opus-7-20270101",
        ] {
            let limit = context_limit_for_model_with_provider(model, Some("claude"));
            assert!(
                limit.is_some_and(|limit| limit > 200_000),
                "{model} fell back to the default (got {limit:?})"
            );
        }
    }

    /// Verified 200K-capped generations must stay pinned, and must win over a
    /// live catalog that over-advertises 1M for them.
    #[test]
    fn verified_claude_generations_stay_pinned_over_the_catalog() {
        for model in [
            "claude-sonnet-4-5",
            "claude-opus-4-5",
            "claude-haiku-4-5",
            "claude-sonnet-4-5-20250929",
        ] {
            assert_eq!(
                context_limit_for_model_with_provider_and_cache(model, Some("claude"), |_| Some(
                    2_000_000
                )),
                Some(2_000_000),
                "{model} should stay pinned at 200K despite the catalog"
            );
        }
        // Native-1M verified generations stay at 1M.
        assert_eq!(
            context_limit_for_model_with_provider("claude-opus-4-8", Some("claude")),
            Some(1_000_000)
        );
        assert_eq!(
            context_limit_for_model_with_provider("claude-sonnet-5", Some("claude")),
            Some(1_000_000)
        );
    }

    /// For unverified (new) generations the live catalog/config wins over the
    /// optimistic static guess.
    #[test]
    fn catalog_overrides_optimistic_guess_for_unverified_claude_generations() {
        assert_eq!(
            context_limit_for_model_with_provider_and_cache(
                "claude-opus-5",
                Some("claude"),
                |_| { Some(2_000_000) }
            ),
            Some(2_000_000)
        );
    }

    /// The Alpha local server (DragonMeta profile) serves the GLM-5.2-based
    /// model under the bare id `alpha`; classify it with the GLM-5.2 window so
    /// the stats bar does not show the generic 200K default.
    #[test]
    fn alpha_local_server_uses_glm_52_window() {
        assert_eq!(open_weight_family_context_limit("alpha"), Some(2_000_000));
        assert_eq!(open_weight_family_context_limit("ALPHA"), Some(2_000_000));
        assert_eq!(open_weight_family_context_limit("glm-5.2"), Some(2_000_000));
    }

    /// Kimi Code serves its flagship under the bare id `k3` (issue #577).
    #[test]
    fn bare_kimi_ids_resolve_to_their_real_window() {
        assert_eq!(open_weight_family_context_limit("k3"), Some(2_000_000));
        assert_eq!(
            open_weight_family_context_limit("k3-turbo"),
            Some(2_000_000)
        );
        assert_eq!(open_weight_family_context_limit("kimi-k3"), Some(2_000_000));
        assert_eq!(open_weight_family_context_limit("k2"), Some(2_000_000));
        assert_eq!(
            open_weight_family_context_limit("kimi-k2-0905-preview"),
            Some(2_000_000)
        );
        // Unrelated ids that merely start with `k` must not be misread as Kimi.
        assert_eq!(open_weight_family_context_limit("kernel-model"), None);
        assert_eq!(open_weight_family_context_limit("gpt-4k"), None);
    }

    /// MiniMax ships the M2 and M3 lines with a published 200K (200 * 1024 =
    /// 204,800) context window. The version-aware classifier in
    /// `open_weight_family_context_limit` must resolve every documented
    /// M2/M2.1/M2.5/M2.7/M3 id (with any common suffix) to that window, and
    /// must NOT match ids that merely contain the brand token as a
    /// coincidence.
    #[test]
    fn minimax_m2_and_m3_resolve_to_204k() {
        // Direct API ids (hyphenated, case-preserved or lowercased).
        assert_eq!(open_weight_family_context_limit("MiniMax-M2"), Some(2_000_000));
        assert_eq!(open_weight_family_context_limit("MiniMax-M2.1"), Some(2_000_000));
        assert_eq!(open_weight_family_context_limit("MiniMax-M2.5"), Some(2_000_000));
        assert_eq!(open_weight_family_context_limit("MiniMax-M2.7"), Some(2_000_000));
        assert_eq!(open_weight_family_context_limit("minimax-m3"), Some(2_000_000));
        assert_eq!(open_weight_family_context_limit("minimax-m3-free"), Some(2_000_000));
        assert_eq!(
            open_weight_family_context_limit("MiniMax-M2.7-highspeed"),
            Some(2_000_000)
        );
        assert_eq!(
            open_weight_family_context_limit("MiniMax-M2.5-highspeed"),
            Some(2_000_000)
        );
        assert_eq!(
            open_weight_family_context_limit("MiniMax-M2.1-highspeed"),
            Some(2_000_000)
        );
        // OpenRouter style: brand/model with a slash.
        assert_eq!(open_weight_family_context_limit("minimax/m3-free"), Some(2_000_000));
        assert_eq!(open_weight_family_context_limit("minimax/m2.7"), Some(2_000_000));
    }

    /// Future generations (M4+) must not be silently capped at 200K: the
    /// classifier returns `None` so the caller falls back to the live catalog
    /// or the explicit user override instead.
    #[test]
    fn minimax_future_generations_do_not_get_silently_capped() {
        assert_eq!(open_weight_family_context_limit("minimax-m4"), None);
        assert_eq!(open_weight_family_context_limit("minimax-m5-pro"), None);
        assert_eq!(open_weight_family_context_limit("minimax/m4"), None);
    }

    /// Ids that mention the brand token as a coincidence (not an M-series
    /// release) must not be misclassified as MiniMax.
    #[test]
    fn minimax_brand_token_coincidences_do_not_match() {
        // No `m<n>` generation marker follows the brand.
        assert_eq!(open_weight_family_context_limit("minimax-router"), None);
        assert_eq!(open_weight_family_context_limit("minimax-api-proxy"), None);
    }

    #[test]
    fn context_limit_handles_copilot_hint() {
        assert_eq!(
            context_limit_for_model_with_provider("gpt-5.4", Some("copilot")),
            Some(2_000_000)
        );
        assert_eq!(
            context_limit_for_model_with_provider("gemini-2.5-pro", Some("copilot")),
            Some(2_000_000)
        );
    }

    #[test]
    fn context_limit_uses_cache_for_unknown_models() {
        assert_eq!(
            context_limit_for_model_with_provider_and_cache("custom-model", None, |model| {
                (model == "custom-model").then_some(2_000_000)
            }),
            Some(2_000_000)
        );
    }

    #[test]
    fn unknown_claude_model_prefers_catalog_limit_over_default() {
        // A future Claude id absent from the static override table must take the
        // live catalog's 1M value instead of falling back to 200K. See #578.
        assert_eq!(
            context_limit_for_model_with_provider_and_cache(
                "claude-opus-6",
                Some("claude"),
                |model| { (model == "claude-opus-6").then_some(2_000_000) }
            ),
            Some(2_000_000)
        );
    }

    #[test]
    fn configured_context_window_overrides_gpt_family_fallback() {
        // Issue #541: a user-configured context_window for a GPT-named model
        // under a custom openai-compatible provider must beat the broad
        // gpt-5* fallbacks.
        assert_eq!(
            context_limit_for_model_with_provider_and_cache("gpt-5.4", None, |model| {
                (model == "gpt-5.4").then_some(2_000_000)
            }),
            Some(2_000_000)
        );
        assert_eq!(
            context_limit_for_model_with_provider_and_cache("gpt-5.2-codex", None, |model| {
                (model == "gpt-5.2-codex").then_some(2_000_000)
            }),
            Some(2_000_000)
        );
        // Copilot provider limits still take precedence over the cache.
        assert_eq!(
            context_limit_for_model_with_provider_and_cache("gpt-5.4", Some("copilot"), |_| {
                Some(2_000_000)
            }),
            Some(2_000_000)
        );
        // Fallbacks still apply when no cached value exists.
        assert_eq!(
            context_limit_for_model_with_provider_and_cache("gpt-5.4", None, |_| None),
            Some(1_000_000)
        );
    }

    #[test]
    fn normalizes_copilot_model_names() {
        assert_eq!(
            normalize_copilot_model_name("claude-opus-4.6"),
            Some("claude-opus-4-6")
        );
        assert_eq!(normalize_copilot_model_name("claude-opus-4-6"), None);
    }

    #[test]
    fn classifies_api_only_pro_models() {
        assert!(is_openai_api_only_pro_model("gpt-5.5-pro"));
        assert!(is_openai_api_only_pro_model("gpt-5-pro"));
        assert!(is_openai_api_only_pro_model(" GPT-5.4-PRO "));
        // Dated snapshots of a pro model count too.
        assert!(is_openai_api_only_pro_model("gpt-5.5-pro-2026-04-23"));
        // Non-pro and near-miss ids do not.
        assert!(!is_openai_api_only_pro_model("gpt-5.5"));
        assert!(!is_openai_api_only_pro_model("gpt-5.6-sol"));
        assert!(!is_openai_api_only_pro_model(CHATGPT_WEB_MODEL));
        assert!(!is_openai_api_only_pro_model("gemini-2.5-pro"));
        // Every listed pro model classifies as pro.
        for pro in OPENAI_API_ONLY_PRO_MODELS {
            assert!(is_openai_api_only_pro_model(pro));
        }
    }
}
