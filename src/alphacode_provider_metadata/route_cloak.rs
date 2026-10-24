//! Cloaked identifiers for the bundled free route.
//!
//! The bundled free tier is an abuse surface: once its route identifiers were
//! readable as source literals, third parties extracted them and consumed the
//! free pool from outside the product. To raise that bar, the identifiers are
//! kept here **not** as greppable literals. Each is stored as a fused byte
//! table and reassembled only at runtime, on use, via [`reveal`]. Nothing
//! decoded is ever persisted; the tables neither contain nor expose the
//! operator, and no accessor returns enough of the fused form for a reader of
//! this file to shortcut the assembly.
//!
//! Guidelines for this module:
//! - Accessors return owned strings (or leaked `'static` strs for consts that
//!   require them) and are intentionally named by *role*, not by operator.
//! - Comments stay role-level on purpose — do not add upstream names here.
//! - When the upstream route changes, regenerate the tables with the same
//!   permutation (xor key, then positional offset) rather than editing returns.

use super::ALPHAX_FREE_PROFILE_ID;
use super::OpenAiCompatibleProfile;

/// Permutation key. Rotate it together with the tables if they are
/// regenerated; the two must never drift apart.
const UNMARK: [u8; 13] = [
    0xC3, 0x17, 0x8E, 0x5B, 0xD9, 0x2F, 0x74, 0xA1, 0x08, 0xE6, 0x93, 0x4C, 0x6D,
];

/// Reassemble one table entry.
///
/// Each byte is un-offset by its index, then un-mixed with the rotating key.
/// The construction is intentionally simple: the goal is that neither source
/// nor binary strings reveal the installed route, not strong cryptography —
/// a determined runtime debugger can still observe assembled values in memory.
fn reveal(encoded: &[u8]) -> String {
    let mut out = Vec::with_capacity(encoded.len());
    for (i, byte) in encoded.iter().enumerate() {
        let unoffset = byte.wrapping_sub(((i % 251) as u8).wrapping_add(1));
        out.push(unoffset ^ UNMARK[i % UNMARK.len()]);
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Reassemble a cloaked profile field (stored as one reference per entry).
fn reveal_chars(encoded: &str) -> String {
    let mut out = Vec::with_capacity(encoded.len());
    for (i, ch) in encoded.chars().enumerate() {
        let unoffset = (ch as u8).wrapping_sub(((i % 251) as u8).wrapping_add(1));
        out.push(unoffset ^ UNMARK[i % UNMARK.len()]);
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Assemble once and hand out for the process lifetime (`'static` call sites).
fn reveal_leaked(
    encoded: &'static [u8],
    cell: &'static std::sync::OnceLock<&'static str>,
) -> &'static str {
    cell.get_or_init(|| Box::leak(reveal(encoded).into_boxed_str()))
}

// ---- fused tables ---------------------------------------------------------

pub(crate) static CLOAKED_BASE: [u8; 31] = [
    0xAC, 0x65, 0xFD, 0x2F, 0xAF, 0x1B, 0x62, 0x96, 0x72, 0xA0, 0x05, 0x6E, 0x13, 0xB8, 0x8A, 0xF1,
    0x86, 0xCA, 0x59, 0x6F, 0xD5, 0x8E, 0xA6, 0xD4, 0x44, 0x26, 0xD2, 0x8E, 0x16, 0x58, 0xBF,
];
pub(crate) static CLOAKED_HOST: [u8; 11] = [
    0xA3, 0x69, 0xEA, 0x79, 0xB7, 0x4C, 0x1F, 0xD6, 0x2F, 0x91, 0x05,
];
pub(crate) static CLOAKED_GPATH: [u8; 12] = [
    0xED, 0x78, 0x01, 0x36, 0xFB, 0x4E, 0x1C, 0xDD, 0x76, 0x9B, 0xFD, 0x41,
];
pub(crate) static CLOAKED_ALIAS: [u8; 14] = [
    0xA9, 0x80, 0xE5, 0x38, 0xF9, 0x54, 0x08, 0xDD, 0x70, 0xD3, 0x00, 0x4A, 0x15, 0xB4,
];
pub(crate) static CLOAKED_ALIAS_EFFICIENT: [u8; 19] = [
    0xA9, 0x80, 0xE5, 0x38, 0xF9, 0x54, 0x08, 0xDD, 0x70, 0xD3, 0x01, 0x36, 0x18, 0xB8, 0x83, 0xF7,
    0x4F, 0xC9, 0x6E,
];
pub(crate) static CLOAKED_ALIAS_SMALL: [u8; 15] = [
    0xA9, 0x80, 0xE5, 0x38, 0xF9, 0x54, 0x08, 0xDD, 0x70, 0xD3, 0xEB, 0x2D, 0x19, 0xBD, 0x8A,
];
pub(crate) static CLOAKED_ALIAS_PREFIX: [u8; 9] =
    [0xA9, 0x80, 0xE5, 0x38, 0xF9, 0x54, 0x08, 0xDD, 0x70];
pub(crate) static CLOAKED_BRAND: [u8; 8] = [0x89, 0x80, 0xE5, 0x38, 0x9F, 0x46, 0x17, 0xCC];
pub(crate) static CLOAKED_PAT_GATEWAY_URL: [u8; 54] = [
    0xEC, 0x2A, 0xEA, 0x76, 0xF6, 0x16, 0x55, 0xD1, 0x85, 0x9C, 0xEE, 0x4B, 0x5F, 0x07, 0x47, 0xB1,
    0x83, 0xF8, 0x1A, 0x5F, 0xB0, 0x7F, 0xAD, 0x12, 0x29, 0x5D, 0x05, 0x44, 0x02, 0x50, 0xD4, 0x60,
    0x49, 0xB1, 0x8C, 0xB3, 0xE0, 0x99, 0x7E, 0x14, 0x75, 0xFA, 0x32, 0xD6, 0x3A, 0x81, 0xCC, 0x66,
    0x00, 0xF1, 0x44, 0x7B, 0x1F, 0x5E,
];
pub(crate) static CLOAKED_PAT_ROUTE: [u8; 27] = [
    0xEC, 0x2A, 0xEA, 0x76, 0xB7, 0x4C, 0x1F, 0xD6, 0x2E, 0x91, 0xF1, 0x44, 0x0F, 0xFA, 0x5B, 0xFF,
    0x87, 0xB5, 0x32, 0x6D, 0xAD, 0x3C, 0xD0, 0xD4, 0x7A, 0x4A, 0x03,
];
pub(crate) static CLOAKED_PAT_BRAND: [u8; 21] = [
    0xEC, 0x2A, 0xEA, 0x76, 0x8A, 0x53, 0x26, 0xD0, 0x6D, 0x93, 0xC6, 0x7F, 0x64, 0xAE, 0x87, 0xFA,
    0x4F, 0x02, 0x23, 0x3C, 0xD8,
];

// These two are planted directly into the const profile definition (which
// requires literal fields), so they are stored as printable references and
// decoded through [`reveal_chars`].
pub const CLOAKED_PROFILE_BASE: &str = "\u{ac}\u{65}\u{fd}\u{2f}\u{af}\u{1b}\u{62}\u{96}\u{72}\u{a0}\u{05}\u{6e}\u{13}\u{b8}\u{8a}\u{f1}\u{86}\u{ca}\u{59}\u{6f}\u{d5}\u{8e}\u{a6}\u{d4}\u{44}\u{26}\u{d2}\u{8e}\u{16}\u{58}\u{bf}";
pub const CLOAKED_PROFILE_ALIAS: &str =
    "\u{a9}\u{80}\u{e5}\u{38}\u{f9}\u{54}\u{08}\u{dd}\u{70}\u{d3}\u{00}\u{4a}\u{15}\u{b4}";

// ---- role-level accessors -------------------------------------------------

/// The bundled free route base URL.
pub fn free_route_base() -> String {
    reveal(&CLOAKED_BASE)
}

/// Host and path prefix of the bundled free route, for surface detection.
pub fn free_route_host() -> String {
    reveal(&CLOAKED_HOST)
}

pub fn free_route_path() -> String {
    reveal(&CLOAKED_GPATH)
}

/// The default automatic-routing model alias on the bundled free route.
pub fn free_route_default_model() -> String {
    reveal(&CLOAKED_ALIAS)
}

/// The unmetered alternate alias that is recognized but never rotated to.
pub fn free_route_efficient_alias() -> String {
    reveal(&CLOAKED_ALIAS_EFFICIENT)
}

/// The metered sibling alias used by regression tests (deliberately excluded
/// from the pool: pricing guard).
pub fn free_route_small_alias() -> String {
    reveal(&CLOAKED_ALIAS_SMALL)
}

/// The shared prefix of every alias family above.
pub fn free_route_auto_prefix() -> String {
    reveal(&CLOAKED_ALIAS_PREFIX)
}

/// The operator brand word surfaced only inside upstream error bodies (and
/// therefore only inside redaction inputs).
pub fn free_route_brand_word() -> String {
    reveal(&CLOAKED_BRAND)
}

/// Redaction pattern strings (built into `LazyLock` regexes by the caller).
pub fn gateway_url_redaction_pattern() -> &'static str {
    static CELL: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    reveal_leaked(&CLOAKED_PAT_GATEWAY_URL, &CELL)
}

pub fn route_alias_redaction_pattern() -> &'static str {
    static CELL: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    reveal_leaked(&CLOAKED_PAT_ROUTE, &CELL)
}

pub fn brand_redaction_pattern() -> &'static str {
    static CELL: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    reveal_leaked(&CLOAKED_PAT_BRAND, &CELL)
}

/// `'static` form of [`free_route_default_model`] for const-slice call sites.
pub fn leaked_free_route_default_model() -> &'static str {
    static CELL: std::sync::OnceLock<&'static str> = std::sync::OnceLock::new();
    reveal_leaked(&CLOAKED_ALIAS, &CELL)
}

// ---- profile-field unwrapping ---------------------------------------------

/// The endpoint a profile serves traffic from, with the bundled free profile's
/// fused field reassembled and all other profiles passed through untouched.
pub fn api_base_for(profile: &OpenAiCompatibleProfile) -> String {
    if profile.id == ALPHAX_FREE_PROFILE_ID {
        reveal(&CLOAKED_BASE)
    } else {
        profile.api_base.to_string()
    }
}

/// The profile's hardcoded default model, decoded for the bundled free profile.
pub fn default_model_for(profile: &OpenAiCompatibleProfile) -> Option<String> {
    if profile.id == ALPHAX_FREE_PROFILE_ID {
        Some(reveal(&CLOAKED_ALIAS))
    } else {
        profile.default_model.map(ToString::to_string)
    }
}

/// Reassemble a cloaked profile field stored as `&'static str` references.
pub fn decoded_profile_field(value: &str) -> String {
    reveal_chars(value)
}

/// User-facing endpoint label. The bundled free profile reports its product
/// gateway rather than the network route it resolves to.
pub fn display_endpoint_for(profile_id: &str, api_base: &str) -> String {
    if profile_id == ALPHAX_FREE_PROFILE_ID {
        "Alphax Free gateway".to_string()
    } else {
        api_base.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_field_form_decodes_consistently_with_tables() {
        // The profile-embedded per-reference form and the byte-table form must
        // be the same permutation: the profile value feeds the same accessors
        // in tests, and resolution must never disagree with itself.
        assert_eq!(
            decoded_profile_field(CLOAKED_PROFILE_BASE),
            free_route_base()
        );
        assert_eq!(
            decoded_profile_field(CLOAKED_PROFILE_ALIAS),
            leaked_free_route_default_model(),
        );
        assert_eq!(
            decoded_profile_field(CLOAKED_PROFILE_ALIAS),
            free_route_default_model(),
        );
    }

    #[test]
    fn decoded_values_are_formed_like_route_identifiers() {
        // Property-level checks only: the accessors must yield a usable https
        // endpoint and stable, non-empty ids without spelling them out here.
        let base = free_route_base();
        assert!(
            base.starts_with("https://"),
            "base must be usable: {base:?}"
        );
        let parsed = url::Url::parse(&base).expect("base must parse");
        assert!(parsed.host_str().is_some_and(|host| host.contains('.')));
        assert!(parsed.path().starts_with('/') && parsed.path().len() > 1);

        assert!(!free_route_default_model().is_empty());
        assert!(!free_route_efficient_alias().is_empty());
        assert!(!free_route_small_alias().is_empty());
        assert!(!free_route_brand_word().is_empty());
        assert_ne!(
            free_route_default_model(),
            free_route_efficient_alias(),
            "the aliases must continue to differ"
        );

        // Redaction patterns must compile.
        regex::Regex::new(gateway_url_redaction_pattern()).expect("gateway URL pattern");
        regex::Regex::new(route_alias_redaction_pattern()).expect("route pattern");
        regex::Regex::new(brand_redaction_pattern()).expect("brand pattern");
    }
}
