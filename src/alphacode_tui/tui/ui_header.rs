use super::box_utils::render_double_rounded_box;
use super::changelog::get_unseen_changelog_entries;
use super::{
    TuiState, accent_color, dim_color, header_name_color,
    shorten_model_name,
};
#[cfg(test)]
use super::{semver, warning_color};
use crate::alphacode_tui::auth::AuthStatus;
#[cfg(test)]
use crate::alphacode_tui::auth::AuthState;
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

fn unseen_changelog_entries() -> Vec<String> {
    #[cfg(test)]
    {
        if let Ok(guard) = unseen_changelog_entries_override().lock()
            && let Some(entries) = guard.clone()
        {
            return entries;
        }
    }
    get_unseen_changelog_entries().clone()
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
            let display_prefix = if *prefix == "gpt" { "GPT".to_string() } else { capitalize(prefix) };
            let inner = &stem[prefix.len()..];
            if inner.is_empty() {
                display_prefix
            } else {
                format!("{}{}", display_prefix, title_case_tokens(inner))
            }
        }
        None => title_case_tokens(stem),
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
    let label = if trimmed.is_empty() { "OpenRouter" } else { trimmed };
    format!("{}: {}", label, prettify_slashed_model_name(short))
}

fn format_model_name(short: &str, provider_name: &str) -> String {
    if short.contains('/') {
        return label_slashed_model(short, provider_name);
    }

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
        Some(first) if first.is_ascii_alphabetic() => {
            first.to_uppercase().chain(chars).collect()
        }
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

    if parts.is_empty() { stripped.to_string() } else { parts.join(" ") }
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
            openai: app.active_dual_credential(crate::alphacode_provider_core::ActiveProvider::OpenAI),
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
    let shown = if configured.is_empty() { specs } else { configured };

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
            if compat.requires_api_key { "api-key" } else { "local" }
        }
        other
            if crate::provider_catalog::resolve_openai_compatible_profile_selection(other)
                .is_some()
                || crate::provider_catalog::openai_compatible_profile_id_for_display_name(other)
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
            return format!("~{}", rest);
        }
    }
    path.to_string()
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
    semver().split('-').next().unwrap_or_else(semver).to_string()
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
/// running character total at or under `fit_width`. Returns whether it fit.
fn push_if_fits<'a>(
    spans: &mut Vec<Span<'a>>,
    running_len: &mut usize,
    fit_width: usize,
    text: String,
    style: Style,
) -> bool {
    let len = text.chars().count();
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

/// Theme hue cycle shared by the gradient wordmark and the brand word.
/// Vibrant, modern gradient that reads as premium.
fn brand_gradient_colors() -> [Color; 6] {
    [
        rgb(108, 215, 255), // cyan
        rgb(118, 198, 255), // blue
        rgb(205, 168, 255), // purple
        rgb(255, 148, 205), // pink
        rgb(108, 235, 158), // green
        rgb(255, 195, 88),  // amber
    ]
}

/// Render `text` as bold spans whose foreground cycles through the brand's
/// vibrant gradient, one color per character. Falls back to a single
/// `header_name_color` span when the gradient is empty or the text is empty.
fn gradient_text_spans(text: &str) -> Vec<Span<'static>> {
    let gradient = brand_gradient_colors();
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

/// ASCII-art "ALPHACODE" wordmark for the persistent header, widest variant
/// first. Each entry is a complete banner; `build_alpha_banner` picks the
/// first one that fits the available width. Widths are measured from the art
/// itself rather than hardcoded so editing a row cannot silently start
/// overflowing.
///
/// - `BANNER_BLOCK` (73 cols): full box-drawing block letters.
/// - `BANNER_COMPACT` (63 cols): smaller filled-block letters for narrower panes.
/// - `BANNER_MONOSPACE` (43 cols): plain monospace lettering, no glyph art.
/// - `BANNER_MINIMAL` (25 cols): bare wordmark with separators, for very
///   narrow terminals.
const BANNER_BLOCK: &[&str] = &[
    r" █████╗ ██╗     ██████╗ ██╗  ██╗ █████╗  ██████╗ ██████╗ ██████╗ ███████╗",
    r"██╔══██╗██║     ██╔══██╗██║  ██║██╔══██╗██╔════╝██╔═══██╗██╔══██╗██╔════╝",
    r"███████║██║     ██████╔╝███████║███████║██║     ██║   ██║██║  ██║█████╗  ",
    r"██╔══██║██║     ██╔═══╝ ██╔══██║██╔══██║██║     ██║   ██║██║  ██║██╔══╝  ",
    r"██║  ██║███████╗██║     ██║  ██║██║  ██║╚██████╗╚██████╔╝██████╔╝███████╗",
    r"╚═╝  ╚═╝╚══════╝╚═╝     ╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝",
];

const BANNER_COMPACT: &[&str] = &[
    r"  ██   ██     █████  ██  ██   ██    █████  ████  █████  ██████ ",
    r" ████  ██     ██  ██ ██  ██  ████  ██     ██  ██ ██  ██ ██     ",
    r"██  ██ ██     █████  ██████ ██  ██ ██     ██  ██ ██  ██ █████  ",
    r"██████ ██     ██     ██  ██ ██████ ██     ██  ██ ██  ██ ██     ",
    r"██  ██ ██████ ██     ██  ██ ██  ██  █████  ████  █████  ██████ ",
];

const BANNER_MONOSPACE: &[&str] = &[r"A L P H A C O D E"];

const BANNER_MINIMAL: &[&str] = &[r"‹ alphacode ›"];

const ALPHA_BANNERS: [&[&str]; 4] = [BANNER_BLOCK, BANNER_COMPACT, BANNER_MONOSPACE, BANNER_MINIMAL];

/// Build the ASCII-art "ALPHACODE" wordmark for the header, or an empty vec when
/// even the narrowest variant cannot fit.
///
/// `width` is the full header width; the same 4-column headroom the model line
/// reserves is applied here so the banner never wraps once the render area
/// subtracts its side margins.
fn build_alpha_banner(width: usize) -> Vec<Line<'static>> {
    let fit_width = width.saturating_sub(4);
    let Some(art) = ALPHA_BANNERS.iter().find(|art| {
        art.iter()
            .all(|row| unicode_width::UnicodeWidthStr::width(*row) <= fit_width)
    }) else {
        return Vec::new();
    };

    // Gradient wordmark: each banner row is tinted with the next brand hue
    // (cyan -> blue -> purple -> pink -> green -> amber) so the logo reads
    // as a flowing multi-color gradient that adapts to the active preset
    // instead of a single flat color.
    let gradient = brand_gradient_colors();
    let mut lines: Vec<Line<'static>> = art
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let style = Style::default().fg(gradient[index % gradient.len()]).bold();
            Line::from(Span::styled(row.to_string(), style)).alignment(Alignment::Left)
        })
        .collect();
    // Separate the wordmark from the status rows below it.
    lines.push(Line::from("").alignment(Alignment::Left));
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

/// The brand line: gradient "alphacode" word, version stamp, optional
/// "self-dev" suffix for canary builds, and any status badges collected by
/// `collect_status_items`. Only rendered when the ASCII banner above it didn't
/// fit (`show_wordmark`), so the brand name isn't shown twice back to back.
fn build_brand_line(app: &dyn TuiState, align: Alignment, show_wordmark: bool) -> Line<'static> {
    let mut spans = if show_wordmark {
        gradient_text_spans("alphacode")
    } else {
        Vec::new()
    };
    if !spans.is_empty() {
        // Always pair the wordmark with the running version so the brand stamp
        // doubles as a build identity ("alphacode v1.0.0").
        let version = crate::alphacode_build_meta::version();
        spans.push(Span::styled(
            format!(" v{}", version),
            Style::default().fg(dim_color()),
        ));
    }
    if app.is_canary() {
        let prefix = if spans.is_empty() { "" } else { " " };
        spans.push(Span::styled(format!("{}self-dev", prefix), Style::default().fg(dim_color())));
    }
    let status_items = collect_status_items(app);
    if !status_items.is_empty() {
        let prefix = if spans.is_empty() { "" } else { " · " };
        spans.push(Span::styled(
            format!("{}{}", prefix, status_items.join(" · ")),
            Style::default().fg(dim_color()),
        ));
    }
    Line::from(spans).alignment(align)
}

/// Gradient separator line that visually divides the header from content.
/// Uses smooth color blending across the brand gradient for a premium feel.
fn build_gradient_separator(width: usize) -> Line<'static> {
    let gradient = brand_gradient_colors();
    let total_chars = width.min(120);
    // Pre-compute blended colors for all positions
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
    // Group adjacent same-color cells into batched spans for fewer allocations
    let chars = ['─', '┄', '┈', '╌'];
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(total_chars / 4 + 1);
    let mut run_start = 0;
    for i in 1..=total_chars {
        if i == total_chars || colors[i] != colors[i - 1] {
            let n = i - run_start;
            let ch = chars[run_start % chars.len()];
            let text: String = std::iter::repeat_n(ch, n).collect();
            spans.push(Span::styled(
                text,
                Style::default().fg(colors[run_start]).add_modifier(Modifier::DIM),
            ));
            run_start = i;
        }
    }
    Line::from(spans).alignment(Alignment::Left)
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
    let upstream = if model_is_placeholder { None } else { app.upstream_provider() };

    let mut spans: Vec<Span> = Vec::new();
    let mut len = nice_model.chars().count();

    if !model_is_placeholder {
        push_if_fits(
            &mut spans,
            &mut len,
            fit_width,
            "\u{2500} /model to switch \u{00b7} ".to_string(),
            Style::default().fg(rgb(88, 95, 118)),
        );
    }
    if !provider_label.is_empty() {
        push_if_fits(
            &mut spans,
            &mut len,
            fit_width,
            format!("{} \u{00b7} ", provider_label),
            Style::default().fg(rgb(88, 95, 118)),
        );
    }

    spans.push(Span::styled(
        nice_model.to_string(),
        // Match the info widget's model accent (pink, bold) instead of plain
        // white so the model reads as a distinct, styled element.
        Style::default().fg(rgb(255, 148, 205)).add_modifier(Modifier::BOLD),
    ));

    if let Some(upstream) = upstream.as_deref() {
        push_if_fits(
            &mut spans,
            &mut len,
            fit_width,
            format!(" \u{2192} {}", upstream),
            Style::default().fg(rgb(88, 95, 118)),
        );
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

    let banner = build_alpha_banner(w);
    let banner_rendered = !banner.is_empty();
    let mut lines: Vec<Line> = banner;
    lines.push(build_brand_line(app, align, !banner_rendered));
    // Visual separator between header brand and content area
    lines.push(build_gradient_separator(w));

    if let Some(model_line) =
        build_model_line(app, &model, &nice_model, auth, active, fit_width)
    {
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
    if text.chars().count() > w {
        text = format!("mcp: {} servers", mcps.len());
    }

    Some(Line::from(Span::styled(text, Style::default().fg(rgb(88, 95, 118)))).alignment(align))
}

fn build_working_dir_line(app: &dyn TuiState, w: usize, align: Alignment) -> Option<Line<'static>> {
    let dir = app.working_dir()?;
    let text = abbreviate_home(&dir);
    if let Some(branch) = app.git_branch() {
        let with_branch = format!("\u{250c} {}  \u{2442} {}", text, branch);
        if with_branch.chars().count() <= w {
            // Render with colored branch
            let dir_part = format!("\u{250c} {}", text);
            let branch_part = format!("  \u{2442} {}", branch);
            let spans = vec![
                Span::styled(dir_part, Style::default().fg(rgb(128, 138, 158))),
                Span::styled(branch_part, Style::default().fg(rgb(108, 198, 118))),
            ];
            // Ensure total width fits
            let total_width: usize = spans.iter().map(|s| s.content.len()).sum();
            if total_width <= w {
                return Some(Line::from(spans).alignment(align));
            }
        }
    }
    Some(Line::from(Span::styled(
        format!("\u{250c} {}", text),
        Style::default().fg(rgb(128, 138, 158)),
    )).alignment(align))
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
                Style::default().fg(dim_color()),
            ))
        })
        .collect();
    if has_more {
        content.push(Line::from(Span::styled(
            format!("  …{} more · /changelog to see all", new_entries.len() - display_count),
            Style::default().fg(dim_color()),
        )));
    }
    if content.is_empty() {
        return Vec::new();
    }

    render_double_rounded_box(
        "Updates",
        content,
        w.saturating_sub(2),
        Style::default().fg(dim_color()),
        Style::default().fg(accent_color()).bold(),
    )
    .into_iter()
    .map(|line| line.alignment(Alignment::Left))
    .collect()
}

// ---------------------------------------------------------------------------
// Combined entry point
// ---------------------------------------------------------------------------

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
    use std::sync::OnceLock;

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

    fn ensure_test_alphacode_home_if_unset() {
        static TEST_HOME: OnceLock<std::path::PathBuf> = OnceLock::new();

        if std::env::var_os("ALPHACODE_HOME").is_some() {
            return;
        }

        let path = TEST_HOME.get_or_init(|| {
            let path =
                std::env::temp_dir().join(format!("alphacode-test-home-{}", std::process::id()));
            let _ = std::fs::create_dir_all(&path);
            path
        });
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
            .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect::<String>())
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
            non_empty.iter().all(|line| line.alignment == Some(Alignment::Left)),
            "persistent header should be left aligned: {non_empty:?}"
        );
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
            non_empty.iter().all(|line| line.alignment == Some(Alignment::Left)),
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

    #[test]
    fn persistent_header_labels_server_and_client_versions() {
        let mut app = create_test_app();
        app.set_remote_server_identity_for_tests(
            Some("blazing"),
            Some("🔥"),
            Some("v0.14.2-dev (old1234)"),
            Some("session_fox_1705012345678"),
        );

        let lines = rendered_header_lines(&app, 120);
        let server_line = lines.iter().find(|line| line.contains("server:")).expect("server line");
        let client_line = lines.iter().find(|line| line.contains("client:")).expect("client line");

        // Clean version-only labels: no pet/server names, no emoji.
        assert!(
            server_line.contains("server: v0.14.2-dev"),
            "server line should carry the compact server version: {server_line}"
        );
        let client_version = compact_version_label(crate::alphacode_build_meta::version());
        assert!(
            client_line.contains(&format!("client: {}", client_version)),
            "client line should carry the compact client version: {client_line}"
        );
        assert!(
            !server_line.contains("Blazing") && !server_line.contains('🔥'),
            "server line must not show the pet name or emoji: {server_line}"
        );
        assert!(
            !client_line.contains("Fox") && !client_line.contains('🦊'),
            "client line must not show the session name or emoji: {client_line}"
        );
    }

    #[test]
    fn persistent_header_collapses_matching_server_and_client_versions() {
        let mut app = create_test_app();
        // The real-world single-install case: server and client report the
        // exact same full build string, so the header must not repeat it.
        let full_version = crate::alphacode_build_meta::version().to_string();
        app.set_remote_server_identity_for_tests(
            Some("blazing"),
            None,
            Some(&full_version),
            Some("session_fox_1705012345678"),
        );

        let lines = rendered_header_lines(&app, 120);
        let version_lines: Vec<&String> = lines
            .iter()
            .filter(|line| line.contains("server:") || line.contains("client:"))
            .collect();

        // Identical versions collapse to a single "server/client:" line.
        assert_eq!(version_lines.len(), 1, "matching versions should render one line: {lines:?}");
        let compact = compact_version_label(&full_version);
        assert!(
            version_lines[0].contains(&format!("server/client: {}", compact)),
            "collapsed line should carry the shared compact version: {}",
            version_lines[0]
        );
    }

    #[test]
    fn persistent_header_keeps_git_hash_when_semvers_match_but_builds_differ() {
        let mut app = create_test_app();
        let client_semver = compact_version_label(crate::alphacode_build_meta::version());
        let fake_server_version = format!("{} (0000000)", client_semver);
        app.set_remote_server_identity_for_tests(
            Some("blazing"),
            None,
            Some(&fake_server_version),
            Some("session_fox_1705012345678"),
        );

        let lines = rendered_header_lines(&app, 160);
        let server_line = lines.iter().find(|line| line.contains("server:")).expect("server line");
        let client_line = lines.iter().find(|line| line.contains("client:")).expect("client line");

        assert!(
            server_line.contains("(0000000)"),
            "same-semver mismatch should keep the server git hash: {server_line}"
        );
        assert!(
            client_line.contains(&format!("client: {}", crate::alphacode_build_meta::version())),
            "same-semver mismatch should keep the client git hash: {client_line}"
        );
    }

    #[test]
    fn persistent_header_omits_server_version_when_too_narrow() {
        let mut app = create_test_app();
        app.set_remote_server_identity_for_tests(
            Some("blazing"),
            Some("🔥"),
            Some("v0.14.2-dev (old1234)"),
            Some("session_fox_1705012345678"),
        );

        let lines = rendered_header_lines(&app, 12);
        assert!(
            !lines.iter().any(|line| line.contains("v0.14.2")),
            "narrow widths should drop the server version entirely: {lines:?}"
        );
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
            !lines.iter().any(|line| line.contains("client:") && line.contains(" · v")),
            "local mode client line should not carry a version label: {lines:?}"
        );
    }

    #[test]
    fn persistent_header_client_line_is_clean_version_only() {
        let mut app = create_test_app();
        app.set_remote_server_identity_for_tests(
            Some("blazing"),
            Some("🔥"),
            Some("v0.14.2-dev (old1234)"),
            Some("session_ram_1705012345678"),
        );
        app.set_connection_type_for_tests(Some("https/sse"));

        let lines = rendered_header_lines(&app, 120);
        let client_line = lines.iter().find(|line| line.contains("client:")).expect("client line");

        let client_version = compact_version_label(crate::alphacode_build_meta::version());
        assert!(
            client_line.contains(&format!("client: {}", client_version)),
            "client line should carry the compact client version: {client_line}"
        );
        // No session name, no animal emoji, no connection icon — just the version.
        assert!(
            !client_line.contains("Ram")
                && !client_line.contains('🐏')
                && !client_line.contains('🌐')
                && !client_line.contains('🔌'),
            "client line should be clean version-only: {client_line}"
        );
    }

    #[test]
    fn prettify_model_id_title_cases_unknown_models() {
        assert_eq!(prettify_model_id("claude-fable-5"), "Claude Fable 5");
        assert_eq!(prettify_model_id("grok-code-fast-1"), "Grok Code Fast 1");
        assert_eq!(prettify_model_id("kimi_k2"), "Kimi K2");
        assert_eq!(prettify_model_id("gemini-3-pro-preview"), "Gemini 3 Pro Preview");
        assert_eq!(prettify_model_id("deepseek-chat"), "Deepseek Chat");
        assert_eq!(prettify_model_id("mistral-large-2411"), "Mistral Large 2411");
        assert_eq!(prettify_model_id("o3-mini"), "O3 Mini");
        // Vowel-less short segments read as acronyms.
        assert_eq!(prettify_model_id("glm-4.6"), "GLM 4.6");
        assert_eq!(prettify_model_id("qwq-32b"), "QWQ 32B");
        // Parameter sizes are uppercased.
        assert_eq!(prettify_model_id("llama-3.3-70b"), "Llama 3.3 70B");
        assert_eq!(prettify_model_id("mixtral-8x7b"), "Mixtral 8X7B");
        // Long digit runs (snapshot dates) are dropped.
        assert_eq!(prettify_model_id("claude-fable-5-20260101"), "Claude Fable 5");
        // Placeholders and slashed ids pass through untouched.
        assert_eq!(prettify_model_id("loading session…"), "loading session…");
        assert_eq!(prettify_model_id("deepseek/deepseek-chat"), "deepseek/deepseek-chat");
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
            assert_eq!(header_model_display_name(input, ""), expected, "model id {input:?}");
        }

        // Slashed ids keep the provider label form.
        assert_eq!(
            header_model_display_name("deepseek/deepseek-chat", "OpenRouter"),
            "OpenRouter: deepseek/deepseek-chat"
        );
        // Placeholders pass through untouched.
        assert_eq!(header_model_display_name("loading session…", ""), "loading session…");
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
        assert!(rendered.contains("anthropic(oauth+key*)"), "rendered: {rendered}");

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
        app.set_remote_startup_phase(crate::alphacode_tui::tui::app::RemoteStartupPhase::LoadingSession);

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

        assert!(rendered.contains("anthropic(oauth)"), "rendered: {rendered}");
        assert!(rendered.contains("openai(key)"), "rendered: {rendered}");
        // Providers the user has no credentials for stay out of the header.
        assert!(!rendered.contains("openrouter"), "rendered: {rendered}");
        assert!(!rendered.contains("copilot"), "rendered: {rendered}");
        assert!(!rendered.contains("○"), "rendered: {rendered}");
    }

    #[test]
    fn auth_status_lines_list_all_providers_when_nothing_configured() {
        let lines = build_auth_status_lines(&AuthStatus::default(), ActiveCredentialOverrides::default());
        assert!(!lines.is_empty(), "all providers should be listed: {lines:?}");
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
                Some(value) => crate::alphacode_core::env::set_var("ALPHACODE_RUNTIME_PROVIDER", value),
                None => crate::alphacode_core::env::remove_var("ALPHACODE_RUNTIME_PROVIDER"),
            }
            flatten(&build_auth_status_lines(&auth, ActiveCredentialOverrides::default()))
        };

        // Auto prefers OAuth: the star must sit on oauth, matching the header
        // provider tag's active-route answer.
        let rendered = rendered_with(None);
        assert!(rendered.contains("anthropic(oauth*+key)"), "rendered: {rendered}");

        // Pinning the API key moves the star, keeping both surfaces consistent.
        let rendered = rendered_with(Some("claude-api"));
        assert!(rendered.contains("anthropic(oauth+key*)"), "rendered: {rendered}");

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
            "NVIDIA NIM: nvidia/nemotron-3-super-120b-a12b"
        );
        // The public aggregator still reads "OpenRouter".
        assert_eq!(
            format_model_name("anthropic/claude-sonnet-4", "OpenRouter"),
            "OpenRouter: anthropic/claude-sonnet-4"
        );
        // Missing provider name falls back to "OpenRouter" rather than an empty label.
        assert_eq!(format_model_name("deepseek/deepseek-chat", ""), "OpenRouter: deepseek/deepseek-chat");
        // Non-slashed models are unaffected by the provider label.
        assert_eq!(format_model_name("claude-opus-4-6", "OpenRouter"), "Claude Opus");
    }

    #[test]
    fn alpha_banners_are_row_width_consistent_and_fit_in_priority_order() {
        // Every banner's rows must share one display width (ragged rows would
        // misalign the block-letter art), and each banner must actually spell
        // out the brand rather than being a broken/legacy placeholder.
        for art in ALPHA_BANNERS {
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

        // Widest variant is tried first, so a wide terminal gets the full
        // block-letter wordmark.
        let wide = build_alpha_banner(120);
        assert!(!wide.is_empty());
        let wide_text = flatten(&wide);
        assert!(wide_text.contains('█'), "wide banner should use block glyphs: {wide_text}");

        // A very narrow terminal falls back to the minimal wordmark rather
        // than rendering nothing.
        let narrow = build_alpha_banner(20);
        assert!(!narrow.is_empty(), "minimal banner should still fit at width 20");
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

        // Wide enough for the ASCII banner: the block art renders as visual
        // glyphs but does not contain the literal text "alphacode", so the
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
        assert!(
            narrow_text.to_lowercase().contains("alphacode"),
            "fallback brand line should show the wordmark: {narrow_text}"
        );
    }
}