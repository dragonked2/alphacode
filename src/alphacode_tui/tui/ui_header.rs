use super::box_utils::render_double_rounded_box;
use super::changelog::get_unseen_changelog_entries;
use super::{
    TuiState, border_color, dim_color, header_name_color, muted_text_color, shorten_model_name,
};
#[cfg(test)]
use super::{semver, warning_color};
#[cfg(test)]
use crate::alphacode_tui::auth::AuthState;
use crate::alphacode_tui::auth::AuthStatus;
use crate::alphacode_tui::tui::brand_ux::BrandTheme;
use crate::alphacode_tui_style::rgb;
use ratatui::prelude::*;
#[cfg(test)]
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Changelog "unseen entries" access (test-overridable)
// ---------------------------------------------------------------------------

#[cfg(test)]
fn unseen_changelog_entries_override() -> &'static std::sync::Mutex<Option<Vec<String>>> {
    static OVERRIDE: OnceLock<std::sync::Mutex<Option<Vec<String>>>> = OnceLock::new();
    OVERRIDE.get_or_init(|| std::sync::Mutex::new(None))
}

fn unseen_changelog_entries() -> std::borrow::Cow<'static, [String]> {
    #[cfg(test)]
    {
        if let Ok(guard) = unseen_changelog_entries_override().lock()
            && let Some(entries) = guard.clone()
        {
            return std::borrow::Cow::Owned(entries);
        }
    }
    // The unseen-entry list is memoized and immutable for the process
    // lifetime (`OnceLock`), and the Updates box rebuilds on every draw,
    // so borrow it instead of cloning every subject string each frame.
    std::borrow::Cow::Borrowed(get_unseen_changelog_entries())
}

#[cfg(test)]
pub(crate) fn set_unseen_changelog_entries_override_for_tests(entries: Option<Vec<String>>) {
    let mut guard = unseen_changelog_entries_override()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *guard = entries;
}

// ---------------------------------------------------------------------------
// Small string helpers
// ---------------------------------------------------------------------------

pub(crate) fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().chain(chars).collect(),
    }
}

/// Compact form of a full build version string: `v0.25.19-dev (abc1234, dirty)`
/// becomes `v0.25.19-dev`. Used for the per-line server/client version labels.
#[cfg(test)]
fn compact_version_label(version: &str) -> String {
    let trimmed = version.trim();
    match trimmed.split_once(" (") {
        Some((head, _)) => head.trim().to_string(),
        None => trimmed.to_string(),
    }
}

/// Title-case hyphen/underscore separated tokens for display.
/// e.g. `3.8-max-free` -> ` 3.8 Max Free`
fn title_case_tokens(s: &str) -> String {
    let mut result = String::new();
    for token in s.split(['-', '_']) {
        if token.is_empty() {
            continue;
        }
        result.push(' ');
        let mut chars = token.chars();
        if let Some(first) = chars.next() {
            result.extend(first.to_uppercase());
        }
        result.extend(chars);
    }
    result
}

// ---------------------------------------------------------------------------
// Model name prettification
// ---------------------------------------------------------------------------

/// Well-known model-family prefixes that get title-cased display treatment
/// when they appear as the stem of a slashed model id (`provider/stem`).
const PRETTY_FAMILY_PREFIXES: &[&str] = &["qwen", "deepseek", "llama", "gemini", "gpt"];

/// Claude model families, in the order they're checked. Shared between
/// `format_model_name` (legacy short-form matcher) and
/// `header_model_display_name` (raw-id matcher) so the family list only
/// lives in one place.
const CLAUDE_FAMILIES: &[&str] = &["opus", "sonnet", "haiku"];

/// Whether a model id is actually an Anthropic model.
///
/// `CLAUDE_FAMILIES` matching is a substring test, so ids like
/// `deepseek-r1-sonnet-tweaked`, `qwen/sonnet-4-finetune`, or any local
/// fine-tune that borrowed the family word were labelled "Claude Sonnet" in
/// the header — telling the user they are on a model they never selected.
/// Require the id to actually be an Anthropic model: either it carries the
/// `claude` vendor prefix, or it is namespaced under `anthropic/`.
fn is_anthropic_model_id(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    if lower.starts_with("anthropic/") {
        return true;
    }
    if lower.starts_with("claude") {
        return true;
    }
    // A bare family word is only meaningful when the provider namespace says
    // Anthropic, which the two checks above already cover. Anything else — a
    // namespaced id from another vendor, a local fine-tune — is not ours to
    // rename.
    false
}

/// True when `model` mentions a Claude family word (opus/sonnet/haiku).
///
/// Used to keep non-Anthropic ids from being pretty-printed into something
/// that reads like a Claude model name (`my-sonnet-4-copy` -> "My Sonnet 4
/// Copy"); those ids are displayed verbatim instead.
fn mentions_claude_family_word(model: &str) -> bool {
    let lower = model.to_ascii_lowercase();
    CLAUDE_FAMILIES.iter().any(|family| lower.contains(family))
}

/// Well-known initialisms that contain vowels and would otherwise be
/// title-cased as words by `is_acronym_segment`.
const KNOWN_ACRONYMS: &[&str] = &["oss", "ai", "moe", "vl", "it", "fp8", "awq", "exp"];

/// Prettify slashed model ids (e.g. `qwen/qwen3.8-max-free` -> `Qwen 3.8 Max Free`).
/// Used in the header to make TokenRouter/OpenRouter-style models look polished.
fn prettify_slashed_model_name(model: &str) -> String {
    let Some((_provider, stem)) = model.split_once('/') else {
        return model.to_string();
    };
    let stem_lower = stem.to_ascii_lowercase();
    let matched_prefix = PRETTY_FAMILY_PREFIXES
        .iter()
        .find(|prefix| stem_lower.starts_with(*prefix));

    match matched_prefix {
        Some(prefix) => {
            let display_prefix = if *prefix == "gpt" {
                "GPT".to_string()
            } else {
                capitalize(prefix)
            };
            let inner = &stem[prefix.len()..];
            if inner.is_empty() {
                display_prefix
            } else {
                format!("{}{}", display_prefix, title_case_tokens(inner))
            }
        }
        None => title_case_tokens(stem).trim().to_string(),
    }
}

fn format_gpt_name(short: &str) -> String {
    let rest = short.trim_start_matches("gpt");
    if rest.is_empty() {
        return "GPT".to_string();
    }
    if let Some(idx) = rest.find("codex") {
        let version = &rest[..idx];
        return if version.is_empty() {
            "GPT Codex".to_string()
        } else {
            format!("GPT-{} Codex", version)
        };
    }
    format!("GPT-{}", rest)
}

/// Label a slashed model id (`provider/stem`) with the active provider's
/// display name instead of a hard-coded aggregator name, so the header
/// matches whichever OpenAI-compatible profile (OpenRouter, TokenRouter,
/// NVIDIA NIM, DeepSeek, ...) the user actually selected. Falls back to
/// "OpenRouter" when no provider name is known.
fn label_slashed_model(short: &str, provider_name: &str) -> String {
    let trimmed = provider_name.trim();
    let label = if trimmed.is_empty() {
        "OpenRouter"
    } else {
        trimmed
    };
    format!("{}: {}", label, prettify_slashed_model_name(short))
}

fn format_model_name(short: &str, provider_name: &str) -> String {
    if short.contains('/') {
        return label_slashed_model(short, provider_name);
    }

    if is_anthropic_model_id(short) {
        for family in CLAUDE_FAMILIES {
            if !short.contains(family) {
                continue;
            }
            if *family == "sonnet" && short.contains("3.5") {
                return "Claude 3.5 Sonnet".to_string();
            }
            if *family == "opus" && short.contains("4.5") {
                return "Claude 4.5 Opus".to_string();
            }
            return format!("Claude {}", capitalize(family));
        }
    }

    if short.starts_with("gpt") {
        // Only the numeric GPT families (gpt-4o, gpt-5.2-codex, ...) have a
        // curated form. Other gpt-prefixed ids (gpt-oss-120b) fall through to
        // the generic prettifier instead of producing "GPT-oss120b".
        let rest = short.trim_start_matches("gpt");
        let is_numeric_family = rest.is_empty() || rest.starts_with(|c: char| c.is_ascii_digit());
        if is_numeric_family {
            return format_gpt_name(short);
        }
    }

    short.to_string()
}

fn is_acronym_segment(part: &str) -> bool {
    if KNOWN_ACRONYMS.contains(&part.to_ascii_lowercase().as_str()) {
        return true;
    }
    // Short, all-alphabetic, and vowel-less segments read as initialisms:
    // glm, gpt, qwq, llm. Anything with a vowel (pro, max, mini, fable)
    // reads as a word and gets normal title-casing.
    part.len() <= 4
        && part.chars().all(|c| c.is_ascii_alphabetic())
        && !part
            .chars()
            .any(|c| matches!(c.to_ascii_lowercase(), 'a' | 'e' | 'i' | 'o' | 'u' | 'y'))
}

fn is_param_size_segment(part: &str) -> bool {
    // 70b / 8x7b / 32k style size or context markers.
    let Some(last) = part.chars().last() else {
        return false;
    };
    part.len() >= 2
        && matches!(last.to_ascii_lowercase(), 'b' | 'm' | 'k')
        && part[..part.len() - 1]
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == 'x')
        && part.chars().any(|c| c.is_ascii_digit())
}

fn is_snapshot_date_segment(part: &str) -> bool {
    part.len() >= 6 && part.chars().all(|c| c.is_ascii_digit())
}

fn title_case_segment(part: &str) -> String {
    let mut chars = part.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() => first.to_uppercase().chain(chars).collect(),
        Some(first) => first.to_string() + chars.as_str(),
        None => String::new(),
    }
}

/// Generic fallback for model ids with no curated pretty name: title-case the
/// hyphen/underscore segments (`claude-fable-5` -> `Claude Fable 5`). Date or
/// snapshot suffixes (6+ digit runs) are dropped, vowel-less short segments are
/// treated as acronyms (`glm` -> `GLM`), and parameter sizes are uppercased
/// (`70b` -> `70B`). Placeholder labels with spaces/ellipses pass through.
fn prettify_model_id(model: &str) -> String {
    if model.contains(' ') || model.contains('…') || model.contains('/') {
        // Already-pretty placeholders and slashed ids (provider/model) pass
        // through untouched; the header labels slashed ids with the active
        // provider instead of mangling them into a single stem.
        return model.to_string();
    }
    let stripped = model
        .strip_suffix(".gguf")
        .or_else(|| model.strip_suffix(".bin"))
        .unwrap_or(model);

    let parts: Vec<String> = stripped
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .filter(|part| !is_snapshot_date_segment(part))
        .map(|part| {
            if is_acronym_segment(part) || is_param_size_segment(part) {
                part.to_uppercase()
            } else {
                title_case_segment(part)
            }
        })
        .collect();

    if parts.is_empty() {
        stripped.to_string()
    } else {
        parts.join(" ")
    }
}

/// Extract the version from a Claude model id, e.g. "claude-opus-4-6" -> "4.6",
/// "claude-3-5-sonnet-latest" -> "3.5", "claude-haiku-4.5" -> "4.5". Snapshot
/// dates (6+ digit runs) are ignored.
fn claude_version_segment(raw: &str, family: &str) -> Option<String> {
    let digits: Vec<&str> = raw
        .split(['-', '_'])
        .filter(|part| *part != family)
        .filter(|part| {
            !part.is_empty()
                && part.len() < 6
                && part.chars().all(|c| c.is_ascii_digit() || c == '.')
                && part.chars().any(|c| c.is_ascii_digit())
        })
        .collect();
    match digits.as_slice() {
        [] => None,
        [single] => Some(single.to_string()),
        [major, minor, ..] => Some(format!(
            "{}.{}",
            major.trim_matches('.'),
            minor.trim_matches('.')
        )),
    }
}

/// Render "gpt-5.1-codex-max" -> "GPT-5.1 Codex Max" from raw segments
/// rather than the legacy mashed short form (which produced
/// "GPT-5.1codexmax"-style names).
fn format_gpt_id_from_segments(rest: &str) -> String {
    let mut segments = rest.split('-');
    let version = segments.next().unwrap_or_default();
    let mut name = format!("GPT-{}", version);
    for segment in segments.filter(|s| !s.is_empty()) {
        name.push(' ');
        name.push_str(&prettify_model_id(segment));
    }
    name
}

/// Final display name for the header model line: curated pretty names first
/// (Claude 4.5 Opus, GPT-5.2 Codex), generic title-cased prettification otherwise.
fn header_model_display_name(model: &str, provider_name: &str) -> String {
    let raw = model.trim();
    if let Some(name) = crate::alphacode_provider_metadata::internal_model_display_name(raw) {
        return name.to_string();
    }

    // Claude family ids ("claude-opus-4-6", "claude-3-5-sonnet-latest",
    // "claude-haiku-4.5") render as "Claude <version> <Family>" for any
    // version, instead of only the hardcoded 3.5/4.5 cases.
    if raw.starts_with("claude") {
        for family in CLAUDE_FAMILIES {
            if !raw.contains(family) {
                continue;
            }
            let family_pretty = capitalize(family);
            return match claude_version_segment(raw, family) {
                Some(version) => format!("Claude {} {}", version, family_pretty),
                None => format!("Claude {}", family_pretty),
            };
        }
    }

    // Slashed ids (provider/model) keep the provider label form; the active
    // provider name labels the line (OpenRouter fallback when unknown) instead
    // of shortening away the namespace.
    if raw.contains('/') {
        return label_slashed_model(raw, provider_name);
    }

    if let Some(rest) = raw.strip_prefix("gpt-")
        && rest.starts_with(|c: char| c.is_ascii_digit())
    {
        return format_gpt_id_from_segments(rest);
    }

    // A non-Anthropic id that merely mentions a Claude family word is a local
    // fine-tune or a foreign model wearing the name (`my-sonnet-4-copy`).
    // Pretty-printing it mints a capital "Sonnet" that the header then reads
    // as a Claude claim, so pass such ids through verbatim instead.
    if mentions_claude_family_word(raw) {
        return raw.to_string();
    }

    let short_model = shorten_model_name(raw);
    let curated = format_model_name(&short_model, provider_name);
    if curated == short_model {
        // No curated pretty name matched; title-case the raw model id
        // instead of showing the mangled short form (`claudefable5`).
        prettify_model_id(raw)
    } else {
        curated
    }
}

// ---------------------------------------------------------------------------
// Auth / credential display
// ---------------------------------------------------------------------------

#[cfg(test)]
fn auth_dot_color(state: AuthState) -> Color {
    use super::success_color;
    match state {
        AuthState::Available => success_color(),
        AuthState::Expired => warning_color(),
        AuthState::NotConfigured => dim_color(),
    }
}

#[cfg(test)]
fn auth_dot_char(state: AuthState) -> &'static str {
    match state {
        AuthState::Available => "●",
        AuthState::Expired => "◐",
        AuthState::NotConfigured => "○",
    }
}

/// Authoritative active credential per dual-auth provider, resolved by the app
/// from the live provider/remote server. `None` entries mean "unknown, fall
/// back to the cached `AuthStatus` + env heuristic".
#[derive(Clone, Copy, Default)]
pub(super) struct ActiveCredentialOverrides {
    anthropic: Option<crate::alphacode_base::auth::ActiveCredential>,
    openai: Option<crate::alphacode_base::auth::ActiveCredential>,
}

impl ActiveCredentialOverrides {
    fn from_app(app: &dyn TuiState) -> Self {
        Self {
            anthropic: app
                .active_dual_credential(crate::alphacode_provider_core::ActiveProvider::Claude),
            openai: app
                .active_dual_credential(crate::alphacode_provider_core::ActiveProvider::OpenAI),
        }
    }

    fn get(
        &self,
        provider: crate::alphacode_provider_core::ActiveProvider,
    ) -> Option<crate::alphacode_base::auth::ActiveCredential> {
        match provider {
            crate::alphacode_provider_core::ActiveProvider::Claude => self.anthropic,
            crate::alphacode_provider_core::ActiveProvider::OpenAI => self.openai,
            _ => None,
        }
    }
}

#[cfg(test)]
fn provider_label(name: &str, state: AuthState, method: Option<&str>) -> String {
    match (state, method) {
        (AuthState::NotConfigured, _) => name.to_string(),
        (_, Some(method)) if !method.is_empty() => format!("{}({})", name, method),
        _ => name.to_string(),
    }
}

/// The auth list is a credential *inventory* (what is configured), while the
/// provider tag above reports the *active* route. When both credentials are
/// configured, mark the active one with `*` so the two surfaces read as one
/// consistent story ("oauth*+key" = both configured, OAuth in use) instead of
/// an ambiguous "oauth+key" that looks like both are being used at once.
#[cfg(test)]
fn dual_method_label(
    provider: crate::alphacode_provider_core::ActiveProvider,
    auth: &AuthStatus,
    active: ActiveCredentialOverrides,
) -> Option<&'static str> {
    use crate::alphacode_tui::auth::{ActiveCredential, resolve_dual_credential_auth};
    let runtime_provider = std::env::var("ALPHACODE_RUNTIME_PROVIDER").ok();
    let resolved = resolve_dual_credential_auth(provider, auth, runtime_provider.as_deref())?;
    // Prefer the app's authoritative answer over the env heuristic.
    let active = active.get(provider).unwrap_or(resolved.active);
    Some(match (resolved.has_oauth, resolved.has_api_key) {
        (true, true) => match active {
            ActiveCredential::OAuth => "oauth*+key",
            ActiveCredential::ApiKey => "oauth+key*",
        },
        (true, false) => "oauth",
        (false, true) => "key",
        (false, false) => return None,
    })
}

/// Configured providers with their full labels, in display order.
#[cfg(test)]
fn auth_full_specs(
    auth: &AuthStatus,
    active: ActiveCredentialOverrides,
) -> Vec<(String, AuthState)> {
    use crate::alphacode_provider_core::ActiveProvider;

    let anthropic_label = provider_label(
        "anthropic",
        auth.anthropic.state,
        dual_method_label(ActiveProvider::Claude, auth, active),
    );
    let openai_label = provider_label(
        "openai",
        auth.openai,
        dual_method_label(ActiveProvider::OpenAI, auth, active),
    );
    let gemini_method = (auth.gemini != AuthState::NotConfigured).then_some("oauth");
    let gemini_label = provider_label("gemini", auth.gemini, gemini_method);

    vec![
        (anthropic_label, auth.anthropic.state),
        ("openrouter".to_string(), auth.openrouter),
        (openai_label, auth.openai),
        (provider_label("cursor", auth.cursor, None), auth.cursor),
        (provider_label("copilot", auth.copilot, None), auth.copilot),
        (gemini_label, auth.gemini),
        (
            provider_label("antigravity", auth.antigravity, None),
            auth.antigravity,
        ),
    ]
}

/// Vertical auth inventory: one line per provider. Configured providers get
/// green/yellow dots; unconfigured ones get a dim hollow dot so they read as
/// available-to-add without cluttering the `/login` heading. Retained for the
/// test suite; the live header no longer renders the per-provider list.
#[cfg(test)]
pub(super) fn build_auth_status_lines(
    auth: &AuthStatus,
    active: ActiveCredentialOverrides,
) -> Vec<Line<'static>> {
    let specs = auth_full_specs(auth, active);
    // Only list providers the user actually has credentials for. When nothing
    // is configured at all, fall back to the full list so the `/login` heading
    // still shows what can be added.
    let configured: Vec<_> = specs
        .iter()
        .filter(|(_, state)| *state != AuthState::NotConfigured)
        .cloned()
        .collect();
    let shown = if configured.is_empty() {
        specs
    } else {
        configured
    };

    shown
        .into_iter()
        .map(|(label, state)| {
            Line::from(vec![
                Span::styled(
                    auth_dot_char(state),
                    Style::default().fg(auth_dot_color(state)),
                ),
                Span::styled(format!(" {}", label), Style::default().fg(dim_color())),
            ])
        })
        .collect()
}

/// Resolve the short auth tag ("oauth" / "api-key" / "local" / "") shown next
/// to a provider name in the header's model line.
fn header_provider_auth_tag(
    name: &str,
    auth: &AuthStatus,
    active: ActiveCredentialOverrides,
) -> &'static str {
    // Anthropic and OpenAI share one credential-resolution source of truth so
    // the header tag never drifts from the info widget / model-switch line. We
    // route through the canonical ActiveProvider rather than matching display
    // strings, which is how this surface previously broke (name == "claude"
    // never matched a "anthropic"-only arm and the tag silently vanished).
    if let Some(provider) = crate::alphacode_provider_core::parse_provider_hint(name) {
        use crate::alphacode_provider_core::ActiveProvider;
        use crate::alphacode_tui::auth::{ActiveCredential, resolve_dual_credential_auth};

        let runtime_provider = std::env::var("ALPHACODE_RUNTIME_PROVIDER").ok();
        match resolve_dual_credential_auth(provider, auth, runtime_provider.as_deref()) {
            Some(resolved) => {
                // The app's live answer wins over the env heuristic; the env
                // var is frequently absent in the TUI client process.
                let credential = active.get(provider).unwrap_or(resolved.active);
                // Report exactly the credential the next request will use. The
                // "both configured" inventory now lives in the auth status line
                // (`oauth*+key`), so this tag never claims two credentials at
                // once -- that ambiguity is how "Claude OAuth" and "API key"
                // used to contradict each other across surfaces.
                return match credential {
                    ActiveCredential::OAuth => "oauth",
                    ActiveCredential::ApiKey => "api-key",
                };
            }
            // Provider recognized but no credentials configured: no tag.
            None if matches!(provider, ActiveProvider::Claude | ActiveProvider::OpenAI) => {
                return "";
            }
            None => {}
        }
    }

    match name {
        "copilot" => {
            if auth.copilot_has_api_token {
                "oauth"
            } else {
                ""
            }
        }
        "openrouter" => "api-key",
        "openai-compatible" => {
            let compat = crate::provider_catalog::resolve_openai_compatible_profile(
                crate::provider_catalog::OPENAI_COMPAT_PROFILE,
            );
            if compat.requires_api_key {
                "api-key"
            } else {
                "local"
            }
        }
        other
            if crate::provider_catalog::resolve_openai_compatible_profile_selection(other)
                .is_some()
                || crate::provider_catalog::openai_compatible_profile_id_for_display_name(
                    other,
                )
                .is_some() =>
        {
            "api-key"
        }
        _ => "",
    }
}

fn header_provider_label(
    provider_name: &str,
    auth: &AuthStatus,
    active: ActiveCredentialOverrides,
) -> String {
    let trimmed = provider_name.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    let name = trimmed.to_lowercase();
    let auth_tag = header_provider_auth_tag(&name, auth, active);
    if auth_tag.is_empty() {
        name
    } else {
        format!("{}:{}", auth_tag, name)
    }
}

#[cfg(test)]
fn configured_auth_count(auth: &AuthStatus) -> usize {
    [
        auth.alphacode,
        auth.anthropic.state,
        auth.openrouter,
        auth.azure,
        auth.openai,
        auth.cursor,
        auth.copilot,
        auth.gemini,
        auth.antigravity,
        auth.google,
    ]
    .into_iter()
    .filter(|state| *state != AuthState::NotConfigured)
    .count()
}

// ---------------------------------------------------------------------------
// Path / width helpers
// ---------------------------------------------------------------------------

fn abbreviate_home(path: &str) -> String {
    if let Some(home) = dirs::home_dir() {
        let home_str = home.display().to_string();
        if path == home_str {
            return "~".to_string();
        }
        if let Some(rest) = path.strip_prefix(&home_str) {
            if rest.starts_with('/') || rest.starts_with('\\') {
                return format!("~{}", rest);
            }
            if home_str.ends_with('/') || home_str.ends_with('\\') {
                return format!("~/{}", rest);
            }
        }
    }
    path.to_string()
}

/// Keep one-line header details inside their allotted terminal columns.
fn truncate_header_text(text: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthStr;

    if UnicodeWidthStr::width(text) <= max_width {
        return text.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }

    let target_width = max_width - 1;
    let mut used_width = 0;
    let mut truncated = String::new();
    for grapheme in unicode_segmentation::UnicodeSegmentation::graphemes(text, true) {
        let grapheme_width = UnicodeWidthStr::width(grapheme);
        if used_width + grapheme_width > target_width {
            break;
        }
        truncated.push_str(grapheme);
        used_width += grapheme_width;
    }
    truncated.push('…');
    truncated
}

#[cfg(test)]
fn truncate_to_width(text: &str, width: usize) -> String {
    let char_count = text.chars().count();
    if char_count <= width {
        return text.to_string();
    }
    match width {
        0 => String::new(),
        1 => "…".to_string(),
        _ => {
            let mut truncated: String = text.chars().take(width - 1).collect();
            truncated.push('…');
            truncated
        }
    }
}

#[cfg(test)]
fn choose_header_candidate(width: usize, candidates: Vec<String>) -> String {
    let mut last_non_empty = String::new();
    for candidate in candidates
        .into_iter()
        .filter(|candidate| !candidate.trim().is_empty())
    {
        if candidate.chars().count() <= width {
            return candidate;
        }
        last_non_empty = candidate;
    }
    truncate_to_width(&last_non_empty, width)
}

#[cfg(test)]
fn semver_core() -> String {
    semver()
        .split('-')
        .next()
        .unwrap_or_else(semver)
        .to_string()
}

#[cfg(test)]
fn semver_minor() -> String {
    let core = semver_core();
    match core.split('.').collect::<Vec<_>>().as_slice() {
        [major, minor, ..] => format!("{}.{}", major, minor),
        _ => core,
    }
}

#[cfg(test)]
fn version_display_candidates() -> Vec<String> {
    vec![
        format!("alphacode {}", semver()),
        format!("alphacode {}", semver_core()),
        format!("alphacode {}", semver_minor()),
        semver_minor(),
    ]
}

/// Push `text` onto `spans` (styled with `style`) only if doing so keeps the
/// running display width at or under `fit_width`. Returns whether it fit.
fn push_if_fits<'a>(
    spans: &mut Vec<Span<'a>>,
    running_len: &mut usize,
    fit_width: usize,
    text: String,
    style: Style,
) -> bool {
    let len = unicode_width::UnicodeWidthStr::width(text.as_str());
    if *running_len + len > fit_width {
        return false;
    }
    *running_len += len;
    spans.push(Span::styled(text, style));
    true
}

// ---------------------------------------------------------------------------
// Brand wordmark
// ---------------------------------------------------------------------------

/// Shared brand gradient from BrandTheme for consistent theming across all surfaces.
/// Returns the standard 16-stop gradient used by header, status bar, pickers, etc.
fn brand_gradient() -> [Color; 16] {
    BrandTheme::gradient()
}

/// Render `text` as bold spans whose foreground cycles through the brand's
/// vibrant gradient, one color per character. Falls back to a single
/// `header_name_color` span when the gradient is empty or the text is empty.
fn gradient_text_spans(text: &str) -> Vec<Span<'static>> {
    let gradient = brand_gradient();
    if text.is_empty() {
        return vec![Span::styled(
            text.to_string(),
            Style::default()
                .fg(header_name_color())
                .add_modifier(Modifier::BOLD),
        )];
    }
    // Group adjacent same-color chars into batched spans
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run_color = gradient[0];
    let mut run_text = String::new();
    for (i, ch) in text.chars().enumerate() {
        let color = gradient[i % gradient.len()];
        if color != run_color && !run_text.is_empty() {
            spans.push(Span::styled(
                std::mem::take(&mut run_text),
                Style::default().fg(run_color).add_modifier(Modifier::BOLD),
            ));
            run_color = color;
        }
        run_text.push(ch);
    }
    if !run_text.is_empty() {
        spans.push(Span::styled(
            run_text,
            Style::default().fg(run_color).add_modifier(Modifier::BOLD),
        ));
    }
    spans
}

/// Branded ASCII-art wordmark for the header, widest variant first.
///
/// The tier comes from the shared [`crate::alphacode_brand::wordmark_for`]
/// selector, so the header cannot disagree with the splash or the CLI banner
/// about which wordmark a given width receives. Each surface used to carry its
/// own copy of the tier list and they had already drifted (the header printed
/// `‹ alphacode ›` while the canonical wordmark is `‹ AlphaCode ›`).
///
/// This deliberately reads the **full** ladder, not a text-only subset. The
/// cyan block art *is* the logo, and the header has always drawn it on
/// terminals wide enough to hold it (73 cells). Routing the header through a
/// compact-only selector silently downgraded every wide terminal to the
/// one-row `A L P H A C O D E` mark, so the art stopped appearing anywhere it
/// was meant to be seen.
///
/// Returns an empty vec when even the narrowest tier cannot fit, so the caller
/// falls back to the text brand line instead of wrapping the art.
///
/// `width` is the full header width; the same 4-column headroom the model line
/// reserves is applied here so the banner never wraps once the render area
/// subtracts its side margins.
fn build_alpha_banner(width: usize) -> Vec<Line<'static>> {
    let fit_width = width.saturating_sub(4);
    let Some(art) = crate::alphacode_brand::wordmark_for(fit_width) else {
        return Vec::new();
    };

    let mut lines = BrandTheme::wordmark_lines(art);
    // Separate the wordmark from the status rows below it.
    lines.push(Line::from("").alignment(Alignment::Center));
    lines
}

// ---------------------------------------------------------------------------
// Persistent header
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(super) fn build_persistent_header(app: &dyn TuiState, width: u16) -> Vec<Line<'static>> {
    let auth = app.auth_status();
    let active = ActiveCredentialOverrides::from_app(app);
    build_persistent_header_with_auth(app, width, &auth, active)
}

/// Status badges shown after the brand word: replay/client mode, pending
/// update indicators, and the active performance-tier badge (if any).
fn collect_status_items(app: &dyn TuiState) -> Vec<&'static str> {
    let mut items = Vec::new();
    if app.is_replay() {
        items.push("replay");
    } else if app.is_remote_mode() {
        items.push("client");
    }
    if app.server_update_available() == Some(true) {
        items.push("srv↑");
    }
    if app.client_update_available() {
        items.push("cli↑");
    }
    if let Some(badge) = crate::perf::profile().tier.badge() {
        items.push(badge);
    }
    items
}

/// The brand line: gradient brand word, version stamp, optional
/// "self-dev" suffix for canary builds, and any status badges collected by
/// `collect_status_items`. Only rendered when the ASCII banner above it didn't
/// fit (`show_wordmark`), so the brand name isn't shown twice back to back.
fn build_brand_line(
    app: &dyn TuiState,
    align: Alignment,
    show_wordmark: bool,
    width: usize,
) -> Line<'static> {
    let budget = width.saturating_sub(1);
    let mut spans = Vec::new();
    let mut used = 0;
    if show_wordmark {
        let name = truncate_header_text(crate::alphacode_brand::DISPLAY_NAME, budget);
        used = unicode_width::UnicodeWidthStr::width(name.as_str());
        spans.extend(gradient_text_spans(&name));
    }
    // Operational state takes priority over decorative links and build labels.
    for item in collect_status_items(app) {
        let prefix = if spans.is_empty() { "" } else { " · " };
        let tone = if item.contains('↑') {
            BrandTheme::warning()
        } else {
            BrandTheme::dim_bright()
        };
        push_if_fits(
            &mut spans,
            &mut used,
            budget,
            format!("{prefix}{item}"),
            Style::default().fg(tone),
        );
    }
    if show_wordmark {
        push_if_fits(
            &mut spans,
            &mut used,
            budget,
            format!(" · v{}", crate::alphacode_build_meta::version()),
            Style::default().fg(dim_color()),
        );
    }
    if app.is_canary() {
        let prefix = if spans.is_empty() { "" } else { " · " };
        push_if_fits(
            &mut spans,
            &mut used,
            budget,
            format!("{prefix}self-dev"),
            Style::default().fg(dim_color()),
        );
    }
    if width >= 96 {
        let prefix = if spans.is_empty() { "" } else { " · " };
        push_if_fits(
            &mut spans,
            &mut used,
            budget,
            format!("{prefix}{}", crate::alphacode_brand::HOMEPAGE),
            Style::default()
                .fg(dim_color())
                .add_modifier(Modifier::UNDERLINED),
        );
    }
    Line::from(spans).alignment(align)
}

/// Gradient separator line that visually divides the header from content.
/// Uses smooth color blending across the brand gradient for a premium feel.
/// Static colors keep idle partial repainting stable.
///
/// Results are memoized per (width, brand gradient): the full-frame blend is
/// O(width) allocations plus a span per color run, and the separator is
/// rebuilt on every render pass even when the header has not changed.
fn build_gradient_separator(width: usize) -> Line<'static> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    type SeparatorCache = Mutex<HashMap<(usize, [Color; 16]), Line<'static>>>;
    static CACHE: OnceLock<SeparatorCache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let gradient = brand_gradient();
    let key = (width, gradient);
    if let Ok(map) = cache.lock()
        && let Some(line) = map.get(&key)
    {
        return line.clone();
    }

    let total_chars = width;
    let mut colors: Vec<Color> = Vec::with_capacity(total_chars);
    for i in 0..total_chars {
        let hue_t = i as f32 / total_chars as f32;
        let seg = hue_t * (gradient.len() - 1) as f32;
        let idx = seg.floor() as usize;
        let frac = seg - seg.floor();
        let c0 = gradient[idx.min(gradient.len() - 1)];
        let c1 = gradient[(idx + 1).min(gradient.len() - 1)];
        colors.push(blend_colors(c0, c1, frac));
    }
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(total_chars / 6 + 1);
    let mut run_start = 0;
    for i in 1..=total_chars {
        if i == total_chars || colors[i] != colors[i - 1] {
            let n = i - run_start;
            let text: String = std::iter::repeat_n('─', n).collect();
            spans.push(Span::styled(
                text,
                Style::default()
                    .fg(colors[run_start])
                    .add_modifier(Modifier::DIM),
            ));
            run_start = i;
        }
    }
    let line = Line::from(spans).alignment(Alignment::Left);
    if let Ok(mut map) = cache.lock() {
        if map.len() >= 32 {
            map.clear();
        }
        map.insert(key, line.clone());
    }
    line
}

/// Linearly interpolate between two colors.
fn blend_colors(a: Color, b: Color, t: f32) -> Color {
    let (r1, g1, b1) = match a {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => return a,
    };
    let (r2, g2, b2) = match b {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => return b,
    };
    rgb(
        (r1 + (r2 - r1) * t) as u8,
        (g1 + (g2 - g1) * t) as u8,
        (b1 + (b2 - b1) * t) as u8,
    )
}

/// Single model line: dim active-route method on the left, styled model name
/// in the middle, dim upstream/hint detail after. Each optional segment is
/// only appended if it fits within the header width.
fn build_model_line(
    app: &dyn TuiState,
    model: &str,
    nice_model: &str,
    auth: &AuthStatus,
    active: ActiveCredentialOverrides,
    fit_width: usize,
) -> Option<Line<'static>> {
    if nice_model.is_empty() {
        return None;
    }

    let model_is_placeholder = {
        let trimmed = model.trim();
        trimmed.is_empty()
            || trimmed == "connected"
            || trimmed.ends_with('…')
            || trimmed.starts_with("connecting")
    };

    let provider_label = if model_is_placeholder {
        String::new()
    } else {
        header_provider_label(&app.provider_name(), auth, active)
    };
    let upstream = if model_is_placeholder {
        None
    } else {
        app.upstream_provider()
    };

    if fit_width == 0 {
        return None;
    }
    let model_display = truncate_header_text(nice_model, fit_width);
    let mut spans: Vec<Span> = Vec::new();
    let mut len = unicode_width::UnicodeWidthStr::width(model_display.as_str());

    if !provider_label.is_empty() {
        push_if_fits(
            &mut spans,
            &mut len,
            fit_width,
            format!("{} \u{00b7} ", provider_label),
            Style::default().fg(dim_color()),
        );
    }

    // Compact activity marker before the model name: warm amber while a turn is
    // active and green while idle.
    let status_dot_color = if app.is_processing() {
        BrandTheme::warning() // warm amber for active
    } else {
        BrandTheme::success() // emerald for ready
    };
    let status_dot_char = if app.is_processing() {
        "\u{25c9}" // ◉ — active/processing
    } else {
        "\u{25cf}" // ● — ready/idle
    };
    let status_style = Style::default().fg(status_dot_color);
    push_if_fits(
        &mut spans,
        &mut len,
        fit_width,
        format!("{} ", status_dot_char),
        status_style,
    );

    // Model name with brand theme color for consistency
    let model_color = if app.is_processing() {
        BrandTheme::warning() // warm amber when processing
    } else {
        BrandTheme::model() // theme-aware model color
    };
    spans.push(Span::styled(
        model_display,
        Style::default()
            .fg(model_color)
            .add_modifier(Modifier::BOLD),
    ));

    if let Some(upstream) = upstream.as_deref() {
        push_if_fits(
            &mut spans,
            &mut len,
            fit_width,
            format!(" \u{2192} {}", upstream),
            Style::default().fg(dim_color()),
        );
    }

    // Show connection type indicator (ws/http) when available
    if let Some(conn_type) = app.connection_type() {
        let display_conn = match conn_type.trim() {
            t if t.contains("websocket") => "ws",
            t if t.contains("http") => "http",
            other => other,
        };
        if !display_conn.is_empty() {
            push_if_fits(
                &mut spans,
                &mut len,
                fit_width,
                format!(" {}", display_conn),
                Style::default()
                    .fg(dim_color())
                    .add_modifier(Modifier::ITALIC),
            );
        }
    }

    // The session facts (context usage, token totals, queue depth and provider
    // billing) deliberately do NOT live on this row. They used to render here
    // as a consolidated strip, which duplicated the right-hand fact stack and
    // stole the rows that stack probes for free space: with the header strip
    // present the stack could not find a collision-free block and rendered
    // nothing at all. The facts now live in a reserved right-hand column, so the
    // header keeps model identity, connection and the `/model` hint only.

    if !model_is_placeholder {
        // The separator and the hint it introduces fit or fail together. They
        // used to be pushed independently, so whenever the hint did not fit the
        // line still ended in a dangling " · " -- visible on every terminal
        // narrower than the hint, which read as a cut-off string rather than a
        // deliberate omission.
        const HINT: &str = "/model to switch";
        const SEPARATOR: &str = " \u{00b7} ";
        let needed = unicode_width::UnicodeWidthStr::width(SEPARATOR)
            + unicode_width::UnicodeWidthStr::width(HINT);
        if len + needed <= fit_width {
            push_if_fits(
                &mut spans,
                &mut len,
                fit_width,
                SEPARATOR.to_string(),
                Style::default().fg(border_color()),
            );
            push_if_fits(
                &mut spans,
                &mut len,
                fit_width,
                HINT.to_string(),
                Style::default()
                    .fg(muted_text_color())
                    .add_modifier(Modifier::ITALIC),
            );
        }
    }

    Some(Line::from(spans).alignment(Alignment::Left))
}

fn build_persistent_header_with_auth(
    app: &dyn TuiState,
    width: u16,
    auth: &AuthStatus,
    active: ActiveCredentialOverrides,
) -> Vec<Line<'static>> {
    let model = app.provider_model();
    let nice_model = header_model_display_name(&model, &app.provider_name());
    let align = Alignment::Left;
    let w = width as usize;
    let fit_width = w.saturating_sub(4);

    // Keep the header compact and stable throughout the conversation, avoiding
    // a large reflow when the first message arrives.
    let banner = build_alpha_banner(w);
    let banner_rendered = !banner.is_empty();
    let mut lines: Vec<Line> = banner
        .into_iter()
        .map(|line| line.alignment(align))
        .collect();
    lines.push(build_brand_line(
        app,
        align,
        !banner_rendered,
        width as usize,
    ));
    // Visual separator between header brand and content area.
    // NOTE: intentionally static rather than breathing. A time-dependent
    // separator breaks the idle-animation partial repaint, which reproduces
    // every cell outside the animated rows from the previous frame; a
    // breathing separator would freeze mid-breath between full frames.
    // The header's own lines are *wrapped* at `width - 1` (see
    // `prepare_header_cached`), so a separator drawn at exactly `width` would
    // always wrap: ratatui's `Paragraph` does not wrap, it clips, and the
    // `wrap_line` pass before it emits the overflow as a second row. The frame
    // then showed a full-width rule followed by a lone one-cell dash -- a
    // glitch that appeared at every terminal size. Match the wrap width.
    lines.push(build_gradient_separator(w.saturating_sub(1)));

    if let Some(model_line) = build_model_line(app, &model, &nice_model, auth, active, fit_width) {
        lines.push(model_line);
    }

    lines
}

// ---------------------------------------------------------------------------
// Secondary header
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) fn build_header_lines(app: &dyn TuiState, width: u16) -> Vec<Line<'static>> {
    build_secondary_header_lines(app, width)
}

fn build_mcp_line(app: &dyn TuiState, w: usize, align: Alignment) -> Option<Line<'static>> {
    const MAX_MCPS: usize = 4;

    let mcps = app.mcp_servers();
    if mcps.is_empty() {
        return None;
    }

    let shown: Vec<String> = mcps
        .iter()
        .take(MAX_MCPS)
        .map(|(name, count)| {
            if *count > 0 {
                format!("{} ({} tools)", name, count)
            } else {
                format!("{} (…)", name)
            }
        })
        .collect();

    let mut text = format!("mcp: {}", shown.join(", "));
    if mcps.len() > MAX_MCPS {
        text.push_str(&format!(" +{} more", mcps.len() - MAX_MCPS));
    }
    if unicode_width::UnicodeWidthStr::width(text.as_str()) > w {
        text = format!("mcp: {} servers", mcps.len());
    }
    text = truncate_header_text(&text, w);

    // Use brand theme accent for MCP indicator
    Some(
        Line::from(Span::styled(
            text,
            Style::default().fg(BrandTheme::accent()),
        ))
        .alignment(align),
    )
}

/// Keep the project end of a long path visible instead of spending the row
/// on the home-directory prefix. Clip only at complete grapheme boundaries.
fn compact_workspace_path(path: &str, width: usize) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;
    if path.width() <= width {
        return path.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut suffix = Vec::new();
    let mut used = 1;
    for part in path.graphemes(true).rev() {
        let cells = part.width();
        if used + cells > width {
            break;
        }
        suffix.push(part);
        used += cells;
    }
    format!("…{}", suffix.into_iter().rev().collect::<String>())
}

fn build_working_dir_line(app: &dyn TuiState, w: usize, align: Alignment) -> Option<Line<'static>> {
    if w == 0 {
        return None;
    }
    let dir = app.working_dir()?;
    let path = abbreviate_home(&dir);
    let label = if w >= 40 { "Workspace " } else { "in " };
    let label = truncate_header_text(label, w);
    let label_width = unicode_width::UnicodeWidthStr::width(label.as_str());
    let available = w.saturating_sub(label_width);
    let branch = app.git_branch().filter(|branch| !branch.trim().is_empty());
    let branch = branch
        .filter(|_| available >= 22)
        .map(|branch| {
            let branch = truncate_header_text(&branch, (available / 3).clamp(8, 28));
            format!(" · git {branch}")
        })
        .unwrap_or_default();
    let path_width =
        available.saturating_sub(unicode_width::UnicodeWidthStr::width(branch.as_str()));
    Some(
        Line::from(vec![
            Span::styled(label, Style::default().fg(dim_color())),
            Span::styled(
                compact_workspace_path(&path, path_width),
                Style::default().fg(BrandTheme::dim_bright()),
            ),
            Span::styled(branch, Style::default().fg(BrandTheme::accent())),
        ])
        .alignment(align),
    )
}

fn build_secondary_header_lines(app: &dyn TuiState, width: u16) -> Vec<Line<'static>> {
    let align = Alignment::Left;
    let w = width as usize;
    let mut lines: Vec<Line> = Vec::new();

    lines.extend(build_mcp_line(app, w, align));
    lines.extend(build_working_dir_line(app, w, align));
    lines.push(Line::from(""));

    lines
}

// ---------------------------------------------------------------------------
// "Updates" box
// ---------------------------------------------------------------------------

/// Build the "Updates" rounded box (unseen release notes) so it can be
/// rendered inside the top padding above the header. `max_lines` bounds the
/// total height including the box borders; entries beyond the budget are
/// collapsed into a "…N more" line. Returns an empty vec when there are no
/// unseen entries or the budget/width is too small for a box.
pub(super) fn build_updates_box_lines(width: u16, max_lines: usize) -> Vec<Line<'static>> {
    let w = width as usize;
    if w <= 20 || max_lines < 3 {
        return Vec::new();
    }
    let new_entries = unseen_changelog_entries();
    if new_entries.is_empty() {
        return Vec::new();
    }

    // Budget for content lines inside the box (borders take 2 lines).
    let content_budget = (max_lines - 2).min(8);
    let has_more = new_entries.len() > content_budget;
    let display_count = if has_more {
        content_budget.saturating_sub(1)
    } else {
        new_entries.len()
    };

    let mut content: Vec<Line> = new_entries
        .iter()
        .take(display_count)
        .map(|entry| {
            Line::from(Span::styled(
                format!("• {}", entry),
                Style::default().fg(BrandTheme::dim()),
            ))
        })
        .collect();
    if has_more {
        content.push(Line::from(Span::styled(
            format!(
                "  …{} more · /changelog to see all",
                new_entries.len() - display_count
            ),
            Style::default().fg(BrandTheme::dim()),
        )));
    }
    if content.is_empty() {
        return Vec::new();
    }

    render_double_rounded_box(
        "Updates",
        content,
        w.saturating_sub(2),
        Style::default().fg(border_color()),
        Style::default().fg(BrandTheme::accent()).bold(),
    )
    .into_iter()
    .map(|line| line.alignment(Alignment::Left))
    .collect()
}

// ---------------------------------------------------------------------------
// Combined entry point
// ---------------------------------------------------------------------------

/// Rows the always-visible banner band occupies above the transcript.
///
/// The banner is pinned to the top of the chat column (it no longer scrolls
/// away with the transcript). A normal 80x24 terminal shows the full cyan
/// logo, with fourteen rows reserved for the working interface. Short terminals
/// select a complete smaller tier rather than clipping letters.
pub(super) fn banner_band_height(width: u16, available_height: u16) -> u16 {
    if width < 12 || available_height < 12 {
        return 0;
    }
    // Keep a full logo on ordinary 80x24 terminals while reserving fourteen
    // rows for status, transcript, and composer. Short terminals use text.
    let budget = available_height.saturating_sub(14).clamp(2, 10);
    // Build only the banner (not the model/auth lines) to learn its real
    // height. `build_alpha_banner` is a pure width→art lookup.
    banner_wordmark(width, budget - 1).map_or(2, |art| art.len() as u16 + 1)
}

/// Keep the same responsive wordmark before, during, and after a conversation.
/// Terminal geometry, rather than message or processing state, determines size.
pub(super) fn conversation_banner_height(_app: &dyn TuiState, width: u16, height: u16) -> u16 {
    banner_band_height(width, height)
}

fn banner_wordmark(width: u16, height: u16) -> Option<&'static [&'static str]> {
    let fit_width = usize::from(width.saturating_sub(4));
    crate::alphacode_brand::WORDMARK_LADDER
        .iter()
        .copied()
        .find(|tier| {
            tier.len() <= usize::from(height)
                && tier
                    .iter()
                    .all(|row| unicode_width::UnicodeWidthStr::width(*row) <= fit_width)
        })
}

/// Build exactly `height` rows for the pinned banner band.
///
/// The band is drawn in its own layout chunk above the transcript, so it must
/// use only a complete wordmark tier that fits the reserved rows.
pub(super) fn build_banner_band_lines(
    app: &dyn TuiState,
    width: u16,
    height: u16,
) -> Vec<Line<'static>> {
    if height == 0 {
        return Vec::new();
    }
    let _ = app;
    let art_height = height.saturating_sub(1).max(1);
    let art = banner_wordmark(width, art_height).unwrap_or(&[crate::alphacode_brand::DISPLAY_NAME]);
    let mut lines = BrandTheme::wordmark_lines(art);
    if height > 1 {
        lines.resize(usize::from(height - 1), Line::default());
        lines.push(Line::from(Span::styled(
            "─".repeat(usize::from(width)),
            Style::default().fg(crate::alphacode_tui_style::theme::border_color()),
        )));
    }
    lines
}

/// Build both header sections from one authentication snapshot. Credential
/// discovery can touch several files on Windows, so the render path must not
/// repeat it for the persistent and secondary portions of the same frame.
pub(super) fn build_header_sections(
    app: &dyn TuiState,
    width: u16,
) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
    let auth = app.auth_status();
    let active = ActiveCredentialOverrides::from_app(app);
    (
        build_persistent_header_with_auth(app, width, &auth, active),
        build_secondary_header_lines(app, width),
    )
}

/// Number of wordmark rows at the start of the normal header, including its
/// separator row. The remaining status and session details are still useful
/// when the wordmark is pinned separately.
pub(super) fn header_wordmark_rows(width: u16) -> usize {
    build_alpha_banner(usize::from(width)).len()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_tui::auth::{AuthState, AuthStatus, ProviderAuth};
    use crate::alphacode_tui::message::Message;
    use crate::alphacode_tui::provider::{EventStream, Provider};
    use crate::alphacode_tui::tool::Registry;
    use anyhow::Result;
    use async_trait::async_trait;
    use std::sync::Arc;

    /// The pinned banner band shows full art when there is enough working space,
    /// never vanishes on a usable terminal, and never exceeds the wordmark art
    /// height (so it cannot leave a blank gap).
    #[test]
    fn banner_band_shows_full_art_on_a_normal_terminal_and_preserves_work_space() {
        // Tall and normal terminals keep complete art without an empty band.
        let tall = banner_band_height(100, 48);
        assert!(tall >= 1, "a tall terminal must reserve a banner row");
        assert_eq!(tall, 7);
        assert_eq!(banner_band_height(100, 24), 7);
        assert!(banner_band_height(80, 24) <= 24 - 14);
        assert!(tall <= build_alpha_banner(100).len() as u16);

        // Very short terminals get no band (nothing to pin without starving
        // the transcript).
        assert_eq!(banner_band_height(100, 8), 0);
        // Too narrow for any wordmark gets no band.
        assert_eq!(banner_band_height(6, 40), 0);
    }

    #[test]
    fn banner_selects_complete_art_for_the_available_height() {
        for height in 12..=60 {
            let band = banner_band_height(100, height);
            let art = banner_wordmark(100, band - 1).expect("wordmark fits");
            assert_eq!(art.len() + 1, usize::from(band));
            assert!(band <= height.saturating_sub(14).max(2));
            assert!(
                art == crate::alphacode_brand::WORDMARK_BLOCK
                    || art == crate::alphacode_brand::WORDMARK_COMPACT
                    || art == crate::alphacode_brand::WORDMARK_NARROW
                    || art == crate::alphacode_brand::WORDMARK_MONOSPACE_ROWS
                    || art == crate::alphacode_brand::WORDMARK_MINIMAL_ROWS
            );
        }
    }

    #[test]
    fn header_uses_the_public_label_for_internal_model_aliases() {
        let alias = crate::alphacode_provider_metadata::route_cloak::free_route_default_model();
        assert_eq!(
            header_model_display_name(&alias, ""),
            crate::alphacode_provider_metadata::internal_model_display_name(&alias).unwrap()
        );
    }

    /// Substring matching on "opus"/"sonnet"/"haiku" used to rename any model
    /// that happened to contain a family word, including local fine-tunes and
    /// other vendors' models that borrowed the name.
    #[test]
    fn only_genuine_anthropic_ids_are_labelled_claude() {
        for model in [
            "claude-opus-4-5-20251101",
            "claude-sonnet-4-6",
            "claude-haiku-4-5",
            "anthropic/claude-sonnet-4",
        ] {
            assert!(
                is_anthropic_model_id(model),
                "{model} is an Anthropic model and should be labelled as one"
            );
        }
        for model in [
            "qwen3:8b",
            "llama3.2:3b",
            "mistral-sonnet-finetune",
            "deepseek/sonnet-tweaked",
            "my-sonnet-4-copy",
            "alpha",
            "",
        ] {
            assert!(
                !is_anthropic_model_id(model),
                "{model} is not an Anthropic model and must not be renamed to Claude"
            );
        }
    }

    #[test]
    fn a_non_anthropic_id_keeps_its_own_name() {
        // The reported symptom end to end: a local model arrived labelled
        // "Claude Sonnet".
        for (model, provider) in [
            ("qwen3:8b", "ollama"),
            ("llama3.2:3b", "Ollama"),
            ("my-sonnet-4-copy", "Local"),
        ] {
            let label = header_model_display_name(model, provider);
            assert!(
                !label.contains("Claude") && !label.contains("Sonnet"),
                "{model} rendered as {label:?}"
            );
        }
    }

    #[test]
    fn anthropic_ids_still_render_their_curated_names() {
        for (model, provider, expected) in [
            ("claude-opus-4-5-20251101", "anthropic", "Claude 4.5 Opus"),
            ("claude-sonnet-4-5", "anthropic", "Claude 4.5 Sonnet"),
            ("claude-haiku-4-5", "anthropic", "Claude 4.5 Haiku"),
        ] {
            assert_eq!(
                header_model_display_name(model, provider),
                expected,
                "regression: {model} lost its curated display name"
            );
        }
    }

    struct MockProvider;

    #[async_trait]
    impl Provider for MockProvider {
        async fn complete(
            &self,
            _messages: &[Message],
            _tools: &[crate::message::ToolDefinition],
            _system: &str,
            _resume_session_id: Option<&str>,
        ) -> Result<EventStream> {
            Err(anyhow::anyhow!(
                "Mock provider should not be used for streaming completions in ui header tests"
            ))
        }

        fn name(&self) -> &str {
            "mock"
        }

        fn fork(&self) -> Arc<dyn Provider> {
            Arc::new(MockProvider)
        }
    }

    /// Point `ALPHACODE_HOME` at a shared scratch directory when it is unset.
    ///
    /// Serialized against tests that scope their own `ALPHACODE_HOME` under
    /// `lock_test_env`: without it this helper observes the gap between their
    /// cleanup and their next `set_var` and repoints `ALPHACODE_HOME` at the
    /// scratch home while that test is still running.
    ///
    /// The lock is never waited on. Header render tests hold the render-state
    /// lock for their whole body and only then build an app, so a blocking
    /// acquisition here inverts the order against a test that holds the env lock
    /// and is itself waiting for the render lock, and the pair deadlocks. The
    /// lock is skipped when this thread already owns it (owning it already makes
    /// the transition exclusive) and otherwise polled briefly.
    fn ensure_test_alphacode_home_if_unset() {
        if std::env::var_os("ALPHACODE_HOME").is_some() {
            return;
        }

        let held_by_this_thread = crate::storage::test_env_lock_held_by_current_thread();
        let _env_lock = if held_by_this_thread {
            None
        } else {
            crate::storage::try_test_env_lock_for(std::time::Duration::from_secs(2))
        };
        if !held_by_this_thread && _env_lock.is_none() {
            // Owned by another test right now: writing the scratch home would
            // land inside its scope, so leave the variable alone. Readers still
            // resolve to the scratch home while it is unset.
            return;
        }

        let path = crate::storage::shared_test_alphacode_home();
        crate::alphacode_core::env::set_var("ALPHACODE_HOME", path);
    }

    fn create_test_app() -> crate::alphacode_tui::tui::app::App {
        ensure_test_alphacode_home_if_unset();

        let provider: Arc<dyn Provider> = Arc::new(MockProvider);
        let rt = tokio::runtime::Runtime::new().expect("test runtime");
        let registry = rt.block_on(Registry::new(provider.clone()));
        crate::alphacode_tui::tui::app::App::new_for_test_harness(provider, registry)
    }

    fn rendered_header_lines(app: &crate::alphacode_tui::tui::app::App, width: u16) -> Vec<String> {
        build_persistent_header(app, width)
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect()
    }

    fn flatten(lines: &[Line<'_>]) -> String {
        lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect()
    }

    #[test]
    fn left_aligned_mode_keeps_persistent_header_left_aligned() {
        let mut app = create_test_app();
        app.set_centered(false);

        let lines = build_persistent_header(&app, 80);
        let non_empty: Vec<&Line<'_>> = lines
            .iter()
            .filter(|line| !line.spans.iter().all(|span| span.content.trim().is_empty()))
            .collect();

        assert!(!non_empty.is_empty(), "expected persistent header lines");
        assert!(
            non_empty
                .iter()
                .all(|line| line.alignment == Some(Alignment::Left)),
            "persistent header should be left aligned: {non_empty:?}"
        );
    }

    #[test]
    fn gradient_separator_produces_output() {
        let line = build_gradient_separator(80);
        assert!(
            !line.spans.is_empty(),
            "gradient separator should produce output"
        );
    }

    /// The separator must occupy exactly one row after wrapping.
    ///
    /// Header lines are wrapped at `width - 1` before rendering, so a
    /// separator drawn at exactly `width` always spilled its last cell onto an
    /// extra row: the frame showed the full rule followed by a lone dash, at
    /// every terminal size. `wrap_lines` is the same function
    /// `prepare_header_cached` uses, so this measures the real pipeline.
    #[test]
    fn gradient_separator_occupies_one_row_after_wrapping() {
        let app = create_test_app();
        for width in [200u16, 160, 120, 100, 80, 60, 40, 24] {
            let (persistent, _) = build_header_sections(&app, width);
            let separators: Vec<Line<'static>> = persistent
                .into_iter()
                .filter(|line| {
                    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
                    !text.is_empty() && text.chars().all(|ch| ch == '\u{2500}')
                })
                .collect();
            assert_eq!(
                separators.len(),
                1,
                "expected exactly one gradient separator at width {width}"
            );
            let wrapped = crate::alphacode_tui_markdown::wrap_lines(separators, width as usize);
            assert_eq!(
                wrapped.len(),
                1,
                "separator wrapped into {} rows at width {width}; \
                 it must stay one row so the header height is stable",
                wrapped.len()
            );
        }
    }

    /// A truncated header line must not end in a dangling separator.
    ///
    /// The middle dot before "/model to switch" was pushed independently of the
    /// hint, so on a terminal too narrow for the hint the model line still
    /// ended in " . " -- which reads as a string cut off mid-way rather than
    /// one that deliberately omitted a section.
    #[test]
    fn model_line_never_ends_with_a_dangling_separator() {
        let app = create_test_app();
        for width in [200u16, 120, 80, 60, 44, 40, 30, 24] {
            for line in rendered_header_lines(&app, width) {
                let trimmed = line.trim_end();
                assert!(
                    !trimmed.ends_with('\u{00b7}'),
                    "header line at width {width} ends with a dangling separator: {trimmed:?}"
                );
            }
        }
    }

    /// No header row may exceed the terminal width. ratatui's `Paragraph` does
    /// not wrap -- it clips -- so an over-wide row silently loses its tail, and
    /// the one-cell headroom the wrap pass reserves (`width - 1`) is what keeps
    /// the last column free of half-drawn borders.
    #[test]
    fn header_rows_never_exceed_the_terminal_width() {
        let app = create_test_app();
        for width in [200u16, 160, 120, 100, 80, 60, 40, 24] {
            let (mut persistent, secondary) = build_header_sections(&app, width);
            persistent.extend(secondary);
            for line in &persistent {
                assert!(
                    line.width() <= width as usize,
                    "header row is {} cells at terminal width {width}",
                    line.width()
                );
            }
        }
    }

    /// The separator used to stop at 120 columns, leaving a dead gap on wide
    /// terminals that read as a rendering glitch rather than a divider.
    #[test]
    fn gradient_separator_spans_the_full_header_width() {
        for width in [40usize, 80, 120, 200, 320] {
            let line = build_gradient_separator(width);
            assert_eq!(
                line.width(),
                width,
                "separator must reach the right edge at width {width}"
            );
        }
    }

    #[test]
    fn left_aligned_mode_keeps_secondary_header_left_aligned() {
        let mut app = create_test_app();
        app.set_centered(false);

        let lines = build_header_lines(&app, 80);
        let non_empty: Vec<&Line<'_>> = lines
            .iter()
            .filter(|line| !line.spans.iter().all(|span| span.content.trim().is_empty()))
            .collect();

        // The secondary header may legitimately be empty (no MCP servers, no
        // working dir); whatever renders must stay left aligned.
        assert!(
            non_empty
                .iter()
                .all(|line| line.alignment == Some(Alignment::Left)),
            "header detail lines should be left aligned: {non_empty:?}"
        );
        // Regression guard: the credential dot inventory and skills list no
        // longer clutter the header.
        let rendered = flatten(&lines);
        assert!(!rendered.contains("/login to add provider"), "{rendered}");
        assert!(!rendered.contains("skills:"), "{rendered}");
    }

    #[test]
    fn combined_header_sections_match_individual_builders() {
        let app = create_test_app();
        let (persistent, secondary) = build_header_sections(&app, 80);

        assert_eq!(persistent, build_persistent_header(&app, 80));
        assert_eq!(secondary, build_header_lines(&app, 80));
    }

    #[test]
    fn version_display_candidates_compact_for_narrow_width() {
        let rendered = choose_header_candidate(8, version_display_candidates());
        // Version-agnostic: at width 8 only the bare minor semver fits.
        assert_eq!(rendered, semver_minor());
    }

    /// The persistent header is deliberately compact: it carries the brand,
    /// connection status, and active model, and nothing else. Version identity
    /// moved to `/version` rather than costing two header lines on every frame.
    /// This test pins that so a reintroduced `server:`/`client:` line is caught.
    #[test]
    fn persistent_header_omits_version_lines() {
        for server_version in [
            "v0.14.2-dev (old1234)",
            // The single-install case: server and client are byte-identical.
            crate::alphacode_build_meta::version(),
        ] {
            let mut app = create_test_app();
            app.set_remote_server_identity_for_tests(
                Some("blazing"),
                Some("🔥"),
                Some(server_version),
                Some("session_fox_1705012345678"),
            );
            for width in [12u16, 80, 120, 160] {
                let lines = rendered_header_lines(&app, width);
                assert!(
                    !lines.iter().any(|line| line.contains("server:")),
                    "server version must not occupy a header line (width {width}): {lines:?}"
                );
                assert!(
                    !lines.iter().any(|line| line.contains("client:")),
                    "client version must not occupy a header line (width {width}): {lines:?}"
                );
            }
        }
    }

    #[test]
    fn persistent_header_local_mode_has_no_version_labels() {
        let app = create_test_app();
        let lines = rendered_header_lines(&app, 120);
        assert!(
            !lines.iter().any(|line| line.contains("server:")),
            "local mode should not render a server line: {lines:?}"
        );
        assert!(
            !lines
                .iter()
                .any(|line| line.contains("client:") && line.contains(" · v")),
            "local mode client line should not carry a version label: {lines:?}"
        );
    }

    /// The header's brand line reports the connection *transport* (the bare
    /// `client`/`https/sse` status item), never version or session identity.
    /// Version identity belongs to `/version`, and the session's pet name and
    /// emoji stay out of the chrome entirely.
    #[test]
    fn persistent_header_brand_line_reports_transport_without_identity() {
        let mut app = create_test_app();
        app.set_remote_server_identity_for_tests(
            Some("blazing"),
            Some("🔥"),
            Some("v0.14.2-dev (old1234)"),
            Some("session_ram_1705012345678"),
        );
        app.set_connection_type_for_tests(Some("https/sse"));

        let lines = rendered_header_lines(&app, 120);
        let client_line = lines
            .iter()
            .find(|line| line.contains("client"))
            .unwrap_or_else(|| panic!("brand/status line should be present: {lines:?}"));

        assert!(
            !client_line.contains("Ram") && !client_line.contains('🐏'),
            "header must not show the session name or its emoji: {client_line}"
        );
        assert!(
            !client_line.contains("v0.14.2") && !client_line.contains("Blazing"),
            "header must not show version or server identity: {client_line}"
        );
    }

    #[test]
    fn prettify_model_id_title_cases_unknown_models() {
        assert_eq!(prettify_model_id("claude-fable-5"), "Claude Fable 5");
        assert_eq!(prettify_model_id("grok-code-fast-1"), "Grok Code Fast 1");
        assert_eq!(prettify_model_id("kimi_k2"), "Kimi K2");
        assert_eq!(
            prettify_model_id("gemini-3-pro-preview"),
            "Gemini 3 Pro Preview"
        );
        assert_eq!(prettify_model_id("deepseek-chat"), "Deepseek Chat");
        assert_eq!(
            prettify_model_id("mistral-large-2411"),
            "Mistral Large 2411"
        );
        assert_eq!(prettify_model_id("o3-mini"), "O3 Mini");
        // Vowel-less short segments read as acronyms.
        assert_eq!(prettify_model_id("glm-4.6"), "GLM 4.6");
        assert_eq!(prettify_model_id("qwq-32b"), "QWQ 32B");
        // Parameter sizes are uppercased.
        assert_eq!(prettify_model_id("llama-3.3-70b"), "Llama 3.3 70B");
        assert_eq!(prettify_model_id("mixtral-8x7b"), "Mixtral 8X7B");
        // Long digit runs (snapshot dates) are dropped.
        assert_eq!(
            prettify_model_id("claude-fable-5-20260101"),
            "Claude Fable 5"
        );
        // Placeholders and slashed ids pass through untouched.
        assert_eq!(prettify_model_id("loading session…"), "loading session…");
        assert_eq!(
            prettify_model_id("deepseek/deepseek-chat"),
            "deepseek/deepseek-chat"
        );
        // Degenerate inputs survive.
        assert_eq!(prettify_model_id(""), "");
        assert_eq!(prettify_model_id("-"), "-");
    }

    #[test]
    fn header_model_display_name_sweeps_real_model_catalog() {
        // End-to-end through shorten_model_name + format_model_name +
        // prettify_model_id, over the model ids alphacode actually routes.
        let cases = [
            // Anthropic
            ("claude-opus-4-5-20251101", "Claude 4.5 Opus"),
            ("claude-opus-4.6", "Claude 4.6 Opus"),
            ("claude-opus-4-8", "Claude 4.8 Opus"),
            ("claude-sonnet-4-5", "Claude 4.5 Sonnet"),
            ("claude-sonnet-4", "Claude 4 Sonnet"),
            ("claude-3-5-sonnet-latest", "Claude 3.5 Sonnet"),
            ("claude-haiku-4-5", "Claude 4.5 Haiku"),
            ("claude-fable-5", "Claude Fable 5"),
            // OpenAI
            ("gpt-5.2-codex", "GPT-5.2 Codex"),
            ("gpt-5.1-codex-max", "GPT-5.1 Codex Max"),
            ("gpt-5.3-codex-spark", "GPT-5.3 Codex Spark"),
            ("gpt-5-mini", "GPT-5 Mini"),
            ("gpt-5.1-chat-latest", "GPT-5.1 Chat Latest"),
            ("gpt-4o", "GPT-4o"),
            ("gpt-4o-mini", "GPT-4o Mini"),
            ("gpt-oss-120b", "GPT OSS 120B"),
            ("o3-mini", "O3 Mini"),
            ("o4-mini", "O4 Mini"),
            // Google
            ("gemini-3-pro-preview", "Gemini 3 Pro Preview"),
            ("gemini-2.5-flash", "Gemini 2.5 Flash"),
            // xAI / Moonshot / Zhipu / DeepSeek / Minimax
            ("grok-code-fast-1", "Grok Code Fast 1"),
            ("kimi-k2.5", "Kimi K2.5"),
            ("kimi-k2p5-turbo", "Kimi K2p5 Turbo"),
            ("glm-4.6", "GLM 4.6"),
            ("deepseek-v4-flash", "Deepseek V4 Flash"),
            ("minimax-m2.7", "Minimax M2.7"),
            // Meta / Mistral / Qwen / community
            ("llama-3.3-70b", "Llama 3.3 70B"),
            ("mixtral-8x7b", "Mixtral 8X7B"),
            ("devstral-medium-2507", "Devstral Medium 2507"),
            ("qwen3-coder-plus", "Qwen3 Coder Plus"),
            ("composer-1.5", "Composer 1.5"),
            ("llama-3.1-8b-instant", "Llama 3.1 8B Instant"),
        ];
        for (input, expected) in cases {
            assert_eq!(
                header_model_display_name(input, ""),
                expected,
                "model id {input:?}"
            );
        }

        // Slashed ids keep the provider label form.
        assert_eq!(
            header_model_display_name("deepseek/deepseek-chat", "OpenRouter"),
            "OpenRouter: Deepseek Chat"
        );
        // Placeholders pass through untouched.
        assert_eq!(
            header_model_display_name("loading session…", ""),
            "loading session…"
        );
        assert_eq!(header_model_display_name("connected", ""), "Connected");
    }

    #[test]
    fn compact_version_label_strips_hash_suffix() {
        assert_eq!(
            compact_version_label("v0.25.19-dev (7e261bcc, dirty)"),
            "v0.25.19-dev"
        );
        assert_eq!(compact_version_label("v0.25.19 (abc1234)"), "v0.25.19");
        assert_eq!(compact_version_label(" v0.25.19 "), "v0.25.19");
    }

    #[test]
    fn configured_auth_count_includes_non_model_auth_surfaces() {
        let auth = AuthStatus {
            alphacode: AuthState::Available,
            anthropic: ProviderAuth {
                state: AuthState::Expired,
                has_oauth: true,
                oauth_state: AuthState::Expired,
                has_api_key: false,
            },
            azure: AuthState::Available,
            google: AuthState::Available,
            ..AuthStatus::default()
        };

        assert_eq!(configured_auth_count(&auth), 4);
    }

    #[test]
    fn header_provider_auth_tag_reports_active_credential_for_openai() {
        let _guard = crate::storage::lock_test_env();
        let prev = std::env::var_os("ALPHACODE_RUNTIME_PROVIDER");
        crate::alphacode_core::env::remove_var("ALPHACODE_RUNTIME_PROVIDER");
        let auth = AuthStatus {
            openai: AuthState::Available,
            openai_has_oauth: true,
            openai_has_api_key: true,
            ..AuthStatus::default()
        };

        // Auto mode prefers OAuth; the tag must report only the credential in
        // use (the auth inventory line carries the "both configured" detail).
        assert_eq!(
            header_provider_auth_tag("openai", &auth, ActiveCredentialOverrides::default()),
            "oauth"
        );
        if let Some(value) = prev {
            crate::alphacode_core::env::set_var("ALPHACODE_RUNTIME_PROVIDER", value);
        }
    }

    #[test]
    fn header_provider_auth_tag_prefers_app_resolved_credential_over_env() {
        let _guard = crate::storage::lock_test_env();
        let prev = std::env::var_os("ALPHACODE_RUNTIME_PROVIDER");
        // The TUI client usually does not inherit ALPHACODE_RUNTIME_PROVIDER, so the
        // env heuristic would answer "oauth" here; the app's resolution must win.
        crate::alphacode_core::env::remove_var("ALPHACODE_RUNTIME_PROVIDER");
        let both = AuthStatus {
            anthropic: ProviderAuth {
                // `state` must be set alongside the credential booleans:
                // `build_auth_status_lines` filters `NotConfigured` providers out
                // and falls back to the full "no credentials" list (issue #654).
                state: AuthState::Available,
                has_oauth: true,
                oauth_state: AuthState::Available,
                has_api_key: true,
            },
            ..AuthStatus::default()
        };
        let overrides = ActiveCredentialOverrides {
            anthropic: Some(crate::alphacode_base::auth::ActiveCredential::ApiKey),
            openai: None,
        };
        assert_eq!(
            header_provider_auth_tag("anthropic", &both, overrides),
            "api-key"
        );
        let rendered = flatten(&build_auth_status_lines(&both, overrides));
        assert!(
            rendered.contains("anthropic(oauth+key*)"),
            "rendered: {rendered}"
        );

        if let Some(value) = prev {
            crate::alphacode_core::env::set_var("ALPHACODE_RUNTIME_PROVIDER", value);
        }
    }

    #[test]
    fn header_provider_auth_tag_honors_runtime_selection_and_oauth_first() {
        let _guard = crate::storage::lock_test_env();
        let prev = std::env::var_os("ALPHACODE_RUNTIME_PROVIDER");

        let both = AuthStatus {
            anthropic: ProviderAuth {
                has_oauth: true,
                has_api_key: true,
                ..Default::default()
            },
            ..AuthStatus::default()
        };

        // Explicit API-key selection wins even when OAuth is available.
        crate::alphacode_core::env::set_var("ALPHACODE_RUNTIME_PROVIDER", "claude-api");
        assert_eq!(
            header_provider_auth_tag("anthropic", &both, ActiveCredentialOverrides::default()),
            "api-key"
        );

        // Explicit OAuth selection.
        crate::alphacode_core::env::set_var("ALPHACODE_RUNTIME_PROVIDER", "claude");
        assert_eq!(
            header_provider_auth_tag("anthropic", &both, ActiveCredentialOverrides::default()),
            "oauth"
        );

        // Auto (unset) prefers OAuth when both credentials are present.
        crate::alphacode_core::env::remove_var("ALPHACODE_RUNTIME_PROVIDER");
        assert_eq!(
            header_provider_auth_tag("anthropic", &both, ActiveCredentialOverrides::default()),
            "oauth"
        );

        // The "claude" display name resolves to the same Anthropic tagging.
        assert_eq!(
            header_provider_auth_tag("claude", &both, ActiveCredentialOverrides::default()),
            "oauth"
        );
        crate::alphacode_core::env::set_var("ALPHACODE_RUNTIME_PROVIDER", "claude-api");
        assert_eq!(
            header_provider_auth_tag("claude", &both, ActiveCredentialOverrides::default()),
            "api-key"
        );
        crate::alphacode_core::env::remove_var("ALPHACODE_RUNTIME_PROVIDER");

        // Auto falls back to the API key when no OAuth credential exists.
        let api_only = AuthStatus {
            anthropic: ProviderAuth {
                has_oauth: false,
                has_api_key: true,
                ..Default::default()
            },
            ..AuthStatus::default()
        };
        assert_eq!(
            header_provider_auth_tag("anthropic", &api_only, ActiveCredentialOverrides::default()),
            "api-key"
        );

        if let Some(value) = prev {
            crate::alphacode_core::env::set_var("ALPHACODE_RUNTIME_PROVIDER", value);
        } else {
            crate::alphacode_core::env::remove_var("ALPHACODE_RUNTIME_PROVIDER");
        }
    }

    #[test]
    fn build_persistent_header_prefers_configured_model_during_remote_connect() {
        let _guard = crate::storage::lock_test_env();
        let prev_model = std::env::var_os("ALPHACODE_MODEL");
        let prev_provider = std::env::var_os("ALPHACODE_PROVIDER");
        crate::alphacode_core::env::set_var("ALPHACODE_MODEL", "gpt-5.4");
        crate::alphacode_core::env::set_var("ALPHACODE_PROVIDER", "openai");

        let app = crate::alphacode_tui::tui::app::App::new_for_remote(None);
        let rendered = flatten(&build_persistent_header(&app, 80));

        assert!(rendered.contains("GPT-5.4"));
        assert!(!rendered.contains("connecting to server…"));

        if let Some(prev_model) = prev_model {
            crate::alphacode_core::env::set_var("ALPHACODE_MODEL", prev_model);
        } else {
            crate::alphacode_core::env::remove_var("ALPHACODE_MODEL");
        }
        if let Some(prev_provider) = prev_provider {
            crate::alphacode_core::env::set_var("ALPHACODE_PROVIDER", prev_provider);
        } else {
            crate::alphacode_core::env::remove_var("ALPHACODE_PROVIDER");
        }
    }

    #[test]
    fn build_header_lines_omits_placeholder_provider_label_when_unknown() {
        // Reads model/provider env-derived state: without the env lock, the
        // sibling test that sets ALPHACODE_MODEL=gpt-5.4 mid-flight leaks into this
        // render and the "loading session…" placeholder never appears. The
        // startup-phase label is also only rendered when no model hint is
        // known, so neutralize ALPHACODE_MODEL/ALPHACODE_PROVIDER for the duration
        // ("unknown" also suppresses the shared test home's config
        // default_model fallback, which another test may have persisted).
        let _guard = crate::storage::lock_test_env();
        let prev_model = std::env::var_os("ALPHACODE_MODEL");
        let prev_provider = std::env::var_os("ALPHACODE_PROVIDER");
        crate::alphacode_core::env::set_var("ALPHACODE_MODEL", "unknown");
        crate::alphacode_core::env::remove_var("ALPHACODE_PROVIDER");

        let mut app = crate::alphacode_tui::tui::app::App::new_for_remote(None);
        app.set_remote_startup_phase(
            crate::alphacode_tui::tui::app::RemoteStartupPhase::LoadingSession,
        );

        // The model line lives in the persistent header now; the startup phase
        // label renders there without a bogus "(unknown)" provider tag.
        let rendered = flatten(&build_persistent_header(&app, 80));

        if let Some(prev_model) = prev_model {
            crate::alphacode_core::env::set_var("ALPHACODE_MODEL", prev_model);
        } else {
            crate::alphacode_core::env::remove_var("ALPHACODE_MODEL");
        }
        if let Some(prev_provider) = prev_provider {
            crate::alphacode_core::env::set_var("ALPHACODE_PROVIDER", prev_provider);
        } else {
            crate::alphacode_core::env::remove_var("ALPHACODE_PROVIDER");
        }

        assert!(rendered.contains("loading session…"), "{rendered}");
        assert!(!rendered.contains("(unknown)"));
        assert!(!rendered.contains("(remote)"));
    }

    #[test]
    fn build_header_lines_hides_secondary_placeholder_during_brief_connecting_phase() {
        // Same env sensitivity as the placeholder test above: ALPHACODE_MODEL /
        // ALPHACODE_PROVIDER mutations from sibling tests change what renders.
        let _guard = crate::storage::lock_test_env();
        let app = crate::alphacode_tui::tui::app::App::new_for_remote(None);

        let rendered = flatten(&build_header_lines(&app, 80));

        assert!(
            !rendered.contains("connecting to server…"),
            "brief connecting placeholder should not render the secondary detail line"
        );
        assert!(!rendered.contains("(remote)"));
    }

    #[test]
    fn auth_status_lines_show_all_providers_with_state_dots() {
        let auth = AuthStatus {
            anthropic: ProviderAuth {
                state: AuthState::Expired,
                has_oauth: true,
                oauth_state: AuthState::Expired,
                has_api_key: false,
            },
            openai: AuthState::Available,
            openai_has_oauth: false,
            openai_has_api_key: true,
            ..AuthStatus::default()
        };

        let rendered = build_auth_status_lines(&auth, ActiveCredentialOverrides::default())
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect::<Vec<_>>()
            .join("\n");

        assert!(
            rendered.contains("anthropic(oauth)"),
            "rendered: {rendered}"
        );
        assert!(rendered.contains("openai(key)"), "rendered: {rendered}");
        // Providers the user has no credentials for stay out of the header.
        assert!(!rendered.contains("openrouter"), "rendered: {rendered}");
        assert!(!rendered.contains("copilot"), "rendered: {rendered}");
        assert!(!rendered.contains("○"), "rendered: {rendered}");
    }

    #[test]
    fn auth_status_lines_list_all_providers_when_nothing_configured() {
        let lines =
            build_auth_status_lines(&AuthStatus::default(), ActiveCredentialOverrides::default());
        assert!(
            !lines.is_empty(),
            "all providers should be listed: {lines:?}"
        );
    }

    #[test]
    fn auth_status_line_marks_active_credential_when_both_configured() {
        let _guard = crate::storage::lock_test_env();
        let prev = std::env::var_os("ALPHACODE_RUNTIME_PROVIDER");
        let auth = AuthStatus {
            anthropic: ProviderAuth {
                state: AuthState::Available,
                has_oauth: true,
                oauth_state: AuthState::Available,
                has_api_key: true,
            },
            ..AuthStatus::default()
        };

        let rendered_with = |runtime: Option<&str>| {
            match runtime {
                Some(value) => {
                    crate::alphacode_core::env::set_var("ALPHACODE_RUNTIME_PROVIDER", value)
                }
                None => crate::alphacode_core::env::remove_var("ALPHACODE_RUNTIME_PROVIDER"),
            }
            flatten(&build_auth_status_lines(
                &auth,
                ActiveCredentialOverrides::default(),
            ))
        };

        // Auto prefers OAuth: the star must sit on oauth, matching the header
        // provider tag's active-route answer.
        let rendered = rendered_with(None);
        assert!(
            rendered.contains("anthropic(oauth*+key)"),
            "rendered: {rendered}"
        );

        // Pinning the API key moves the star, keeping both surfaces consistent.
        let rendered = rendered_with(Some("claude-api"));
        assert!(
            rendered.contains("anthropic(oauth+key*)"),
            "rendered: {rendered}"
        );

        match prev {
            Some(value) => crate::alphacode_core::env::set_var("ALPHACODE_RUNTIME_PROVIDER", value),
            None => crate::alphacode_core::env::remove_var("ALPHACODE_RUNTIME_PROVIDER"),
        }
    }

    #[test]
    fn format_model_name_labels_slashed_models_with_active_provider() {
        // Regression for issue #329: a NVIDIA NIM model must be labeled with the
        // active provider's display name, not the fixed "OpenRouter" aggregator.
        assert_eq!(
            format_model_name("nvidia/nemotron-3-super-120b-a12b", "NVIDIA NIM"),
            "NVIDIA NIM: Nemotron 3 Super 120b A12b"
        );
        // The public aggregator still reads "OpenRouter".
        assert_eq!(
            format_model_name("anthropic/claude-sonnet-4", "OpenRouter"),
            "OpenRouter: Claude Sonnet 4"
        );
        // Missing provider name falls back to "OpenRouter" rather than an empty label.
        assert_eq!(
            format_model_name("deepseek/deepseek-chat", ""),
            "OpenRouter: Deepseek Chat"
        );
        // Non-slashed models are unaffected by the provider label.
        assert_eq!(
            format_model_name("claude-opus-4-6", "OpenRouter"),
            "Claude Opus"
        );
    }

    #[test]
    fn alpha_banners_are_row_width_consistent_and_fit_in_priority_order() {
        for width in [40, 80, 120, 200] {
            let lines = build_alpha_banner(width);
            assert!(lines.iter().all(|line| line.width() <= width));
            // The soft edge strokes and luminous cyan face remain distinct.
            let colors: std::collections::HashSet<_> = lines
                .iter()
                .flat_map(|line| line.spans.iter().filter_map(|span| span.style.fg))
                .collect();
            assert!(colors.len() >= 3);
        }
        // The header draws the shared ladder, and every tier must stay
        // internally aligned so it cannot shear the header layout.
        for art in crate::alphacode_brand::WORDMARK_LADDER {
            let widths: Vec<usize> = art
                .iter()
                .map(|row| unicode_width::UnicodeWidthStr::width(*row))
                .collect();
            let first = widths[0];
            assert!(
                widths.iter().all(|w| *w == first),
                "banner rows must share one width: {widths:?} in {art:?}"
            );
        }

        // A wide terminal gets the real logo: the cyan block art, not a
        // single-row text substitute. This is the regression that made the
        // banner look absent -- the header had been routed through the
        // compact-only selector, so the art never rendered anywhere.
        let wide = build_alpha_banner(120);
        assert!(!wide.is_empty());
        assert_eq!(wide[0].alignment, Some(Alignment::Center));
        let wide_text = flatten(&wide);
        assert_eq!(
            wide.len(),
            crate::alphacode_brand::WORDMARK_BLOCK.len() + 1,
            "wide banner should be the block art plus a separator row: {wide_text}"
        );
        assert!(
            wide_text.contains(crate::alphacode_brand::WORDMARK_BLOCK[2].trim()),
            "wide banner should be the block-letter wordmark: {wide_text}"
        );

        // Just past the block art's 73 cells the next tier down is the compact
        // art, which is still multi-row.
        let medium = build_alpha_banner(70);
        assert_eq!(
            medium.len(),
            crate::alphacode_brand::WORDMARK_COMPACT.len() + 1,
            "a 66-cell header should fall to the compact art"
        );

        // A very narrow terminal falls back to the minimal wordmark rather
        // than rendering nothing.
        let narrow = build_alpha_banner(20);
        assert!(
            !narrow.is_empty(),
            "minimal banner should still fit at width 20"
        );
        let narrow_text = flatten(&narrow);
        assert!(
            narrow_text.to_lowercase().contains("alphacode"),
            "narrow banner should still spell out the brand: {narrow_text}"
        );

        // Absurdly narrow widths (even the minimal banner doesn't fit) fall
        // back to the plain gradient brand line instead of overflowing.
        let tiny = build_alpha_banner(5);
        assert!(tiny.is_empty(), "no banner should fit at width 5: {tiny:?}");
    }

    #[test]
    fn brand_line_shows_wordmark_only_when_banner_is_absent() {
        let app = create_test_app();

        // The spaced wordmark does not contain the literal text "alphacode", so the
        // word count should be zero (the brand line only adds the wordmark
        // when the banner is absent).
        let wide_lines = build_persistent_header(&app, 120);
        let wide_text = flatten(&wide_lines);
        assert_eq!(
            wide_text.to_lowercase().matches("alphacode").count(),
            0,
            "wordmark should not appear as literal text when the banner fits: {wide_text}"
        );

        // Too narrow for any banner: the brand line must still spell out the
        // name so the header never goes blank.
        let narrow_lines = build_persistent_header(&app, 5);
        let narrow_text = flatten(&narrow_lines);
        // At 5 columns the brand line has a 4-cell budget, so the full name
        // cannot fit — it is truncated (`Alp...`) rather than dropped. Asserting
        // on the whole name made this test impossible to satisfy; the invariant
        // that matters is that the header is not blank and carries as much of
        // the brand as the width allows.
        assert!(
            !narrow_text.trim().is_empty(),
            "narrow header must not be blank: {narrow_text:?}"
        );
        // The line also carries the separator and version, so compare only the
        // leading alphanumeric run (the truncated brand) against the real name.
        let leading_brand: String = narrow_text
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_lowercase();
        assert!(
            !leading_brand.is_empty() && "alphacode".starts_with(&leading_brand),
            "narrow header should truncate the brand, not replace it: {narrow_text:?}"
        );
    }
}
