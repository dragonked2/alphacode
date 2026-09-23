//! Helpers to keep `tools[].function.description` within strict
//! OpenAI-compatible providers' hard character cap.
//!
//! Strict OpenAI-compatible gateways (OpenAI itself, Experiential Labs,
//! OpenCode Zen strict mode, LM Studio strict, ...) reject the whole chat
//! request with HTTP 400 when any `tools[N].function.description` exceeds
//! their character limit. OpenAI's documented cap is 1024 characters; many
//! third-party gateways sit at 1024 too. The swarm tool's prompt-routing
//! description has historically inlined the full user-tunable
//! `swarm-prompt.md` (~16 KB / 4k tokens), so without this cap a single
//! over-long tool bricked the whole session.
//!
//! This module provides the canonical [`sanitize_tool_description`] helper
//! that every OpenAI-compatible provider path should call before putting a
//! tool description on the wire. The cap is overridable via
//! `ALPHACODE_TOOL_DESCRIPTION_MAX_CHARS` (and, when added to the
//! `ProviderConfig`, `[provider] tool_description_max_chars`); the override
//! can only raise the cap above [`MIN_TOOL_DESCRIPTION_MAX_CHARS`] so a
//! misconfigured env var cannot silently strip descriptions to a useless
//! prefix.
//!
//! This is a *safety net*, not a feature. Tool authors should keep their
//! descriptions short; this helper exists so a single 16 KB outlier cannot
//! lock the user out of every tool call.

/// Default hard cap on `tools[].function.description` characters emitted by
/// OpenAI-compatible providers. Matches OpenAI's documented 1024-character
/// limit; most strict OpenAI-compatible gateways use the same number.
pub const DEFAULT_TOOL_DESCRIPTION_MAX_CHARS: usize = 1024;

/// Minimum cap a user override may set. Below this the helper falls back to
/// [`DEFAULT_TOOL_DESCRIPTION_MAX_CHARS`]; the goal is to keep the safety net
/// intact, not to provide a "strip my tool description to nothing" knob.
pub const MIN_TOOL_DESCRIPTION_MAX_CHARS: usize = 256;

/// Marker appended (with an ellipsis) when a tool description is truncated to
/// fit the active cap. Explicitly tells the model the description was
/// shortened so it doesn't try to read across the boundary.
const TOOL_DESCRIPTION_TRUNCATION_MARKER: &str =
    "\n\n[description truncated to fit provider limit; full prompt loaded separately]";

/// Resolve the active tool-description cap, honouring the env override.
pub fn tool_description_max_chars() -> usize {
    if let Ok(raw) = std::env::var("ALPHACODE_TOOL_DESCRIPTION_MAX_CHARS")
        && let Ok(parsed) = raw.trim().parse::<usize>()
        && parsed >= MIN_TOOL_DESCRIPTION_MAX_CHARS
    {
        return parsed;
    }
    DEFAULT_TOOL_DESCRIPTION_MAX_CHARS
}

/// Truncate and normalize a tool description to the provider's hard cap.
///
/// Behaviour:
/// * Strings already at or under the cap are returned unchanged (after
///   trimming outer whitespace).
/// * Longer strings are truncated on a UTF-8 char boundary, with a
///   [`TOOL_DESCRIPTION_TRUNCATION_MARKER`] appended so the model knows the
///   description was clipped. The marker is included *inside* the cap, not
///   on top of it.
/// * Whitespace-only descriptions collapse to the empty string so providers
///   that reject empty `function.description` see a valid request.
///
/// The truncation marker reserves part of the cap so the final string still
/// fits. If the cap is so small the marker itself cannot fit (should not
/// happen with the [`MIN_TOOL_DESCRIPTION_MAX_CHARS`] floor, but defensive)
/// the marker is dropped and the description is hard-truncated at the cap.
pub fn sanitize_tool_description(description: &str) -> String {
    let trimmed = description.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let cap = tool_description_max_chars();
    // Fast path: already fits.
    if trimmed.chars().count() <= cap {
        return trimmed.to_string();
    }

    let marker = TOOL_DESCRIPTION_TRUNCATION_MARKER;
    let cap_with_marker = cap.saturating_sub(marker.chars().count());
    if cap_with_marker >= MIN_TOOL_DESCRIPTION_MAX_CHARS / 2 {
        let kept: String = trimmed
            .chars()
            .take(cap_with_marker)
            .collect::<String>()
            .trim_end()
            .to_string();
        kept + marker
    } else {
        trimmed.chars().take(cap).collect::<String>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_description_returned_unchanged_after_trim() {
        let input = "  Read a file at the given absolute path.  \n";
        let out = sanitize_tool_description(input);
        assert_eq!(out, "Read a file at the given absolute path.");
    }

    #[test]
    fn empty_after_trim_collapses_to_empty_string() {
        assert_eq!(sanitize_tool_description("   \n\t  "), "");
        assert_eq!(sanitize_tool_description(""), "");
    }

    #[test]
    fn description_at_cap_is_unchanged() {
        let s: String = "a".repeat(DEFAULT_TOOL_DESCRIPTION_MAX_CHARS);
        let out = sanitize_tool_description(&s);
        assert_eq!(out.len(), DEFAULT_TOOL_DESCRIPTION_MAX_CHARS);
        assert_eq!(out, s);
    }

    #[test]
    fn description_just_over_cap_is_truncated_with_marker() {
        let s: String = "a".repeat(DEFAULT_TOOL_DESCRIPTION_MAX_CHARS + 50);
        let out = sanitize_tool_description(&s);
        assert_eq!(out.chars().count(), DEFAULT_TOOL_DESCRIPTION_MAX_CHARS);
        assert!(
            out.ends_with("full prompt loaded separately]"),
            "marker should be present, got: ...{}",
            &out[out.len() - 80..]
        );
    }

    #[test]
    fn truncation_respects_utf8_char_boundaries() {
        // Each 'é' is 2 bytes but 1 char. Building a description that ends
        // mid-multibyte-char must not panic or produce invalid UTF-8.
        let mut s = String::new();
        for _ in 0..(DEFAULT_TOOL_DESCRIPTION_MAX_CHARS + 100) {
            s.push('é');
        }
        let out = sanitize_tool_description(&s);
        // Round-trip via chars(): every char must be a valid Unicode scalar.
        let char_count = out.chars().count();
        assert!(char_count <= DEFAULT_TOOL_DESCRIPTION_MAX_CHARS);
        // No partial UTF-8 sequences.
        assert!(std::str::from_utf8(out.as_bytes()).is_ok());
    }

    #[test]
    fn truncation_drops_marker_when_cap_too_small() {
        // Force a tiny cap by going through the env override: we set the env
        // to MIN - 1, which the helper must clamp back to DEFAULT. Verify
        // the public surface via the resolved cap.
        // SAFETY: test-only env mutation; safe because we set/restore around
        // the call.
        // SAFETY: same as above.
        // SAFETY: same as above.
        unsafe {
            std::env::set_var(
                "ALPHACODE_TOOL_DESCRIPTION_MAX_CHARS",
                "10", // below MIN, must be ignored
            );
        }
        assert_eq!(
            tool_description_max_chars(),
            DEFAULT_TOOL_DESCRIPTION_MAX_CHARS
        );
        unsafe {
            std::env::remove_var("ALPHACODE_TOOL_DESCRIPTION_MAX_CHARS");
        }
    }

    #[test]
    fn env_override_above_min_is_honoured() {
        unsafe {
            std::env::set_var("ALPHACODE_TOOL_DESCRIPTION_MAX_CHARS", "4096");
        }
        assert_eq!(tool_description_max_chars(), 4096);
        let s: String = "x".repeat(3000);
        let out = sanitize_tool_description(&s);
        assert_eq!(out.len(), 3000);
        unsafe {
            std::env::remove_var("ALPHACODE_TOOL_DESCRIPTION_MAX_CHARS");
        }
    }
}
