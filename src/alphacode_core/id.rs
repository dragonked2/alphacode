use std::collections::HashSet;
use uuid::Uuid;

pub fn new_id(prefix: &str) -> String {
    format!("{prefix}_{}", Uuid::new_v4())
}

/// Server/location names with their icons.
///
/// Servers now use location nouns while sessions use client/entity nouns,
/// producing names like "harbor fox" or "observatory otter".
///
/// Icon constraints match `SESSION_NAMES`: single codepoints with default
/// emoji presentation (no VS16), see the comment there.
const SERVER_MODIFIERS: &[(&str, &str)] = &[
    // Natural places
    ("cove", "🌊"),
    ("grove", "🌳"),
    ("meadow", "🌾"),
    ("marsh", "🌿"),
    ("lake", "🛶"),
    ("river", "🚣"),
    ("creek", "💧"),
    ("brook", "💧"),
    ("cliff", "🧗"),
    ("peak", "🗻"),
    ("summit", "🚠"),
    ("forest", "🌲"),
    ("garden", "🌷"),
    ("island", "🌴"),
    ("desert", "🌵"),
    ("beach", "🏄"),
    // Built places
    ("harbor", "⚓"),
    ("camp", "⛺"),
    ("forge", "🔥"),
    ("citadel", "🏯"),
    ("station", "🚉"),
    ("observatory", "🔭"),
    ("workshop", "🔨"),
    ("lighthouse", "🗼"),
    ("temple", "⛪"),
    ("castle", "🏰"),
    ("bridge", "🌉"),
    ("fountain", "⛲"),
    ("stadium", "🎪"),
    ("factory", "🏭"),
    ("pagoda", "🛕"),
    ("hut", "🛖"),
];

/// Session/client names with their icons.
const SESSION_NAMES: &[(&str, &str)] = &[
    // Animals, nature companions, and client entities. Every emoji here is a single, widely-supported
    // codepoint (Unicode <= 12.0, no ZWJ sequences) with *default emoji
    // presentation* (no VS16 / U+FE0F needed). Text-default codepoints that rely
    // on VS16 render as monochrome outlines or tofu in macOS window titles
    // (Ghostty/Terminal tab and titlebar fonts ignore the selector), so they are
    // banned by `session_icons_render_as_single_safe_glyphs`.
    ("ant", "🐜"),
    ("bat", "🦇"),
    ("bird", "🐦"),
    ("bug", "🐛"),
    ("cat", "🐱"),
    ("chicken", "🐔"),
    ("chick", "🐥"),
    ("chipmunk", "🌰"),
    ("cow", "🐄"),
    ("crocodile", "🐊"),
    ("cricket", "🦗"),
    ("dog", "🐕"),
    ("dove", "🤍"),
    ("eagle", "🦅"),
    ("fish", "🐟"),
    ("fox", "🦊"),
    ("giraffe", "🦒"),
    ("hamster", "🐹"),
    ("ladybug", "🐞"),
    ("lobster", "🦞"),
    ("mosquito", "🦟"),
    ("owl", "🦉"),
    ("ox", "🐂"),
    ("pig", "🐷"),
    ("rat", "🐀"),
    ("ram", "🐏"),
    ("rooster", "🐓"),
    ("shrimp", "🦐"),
    ("sauropod", "🦕"),
    ("blowfish", "🐡"),
    ("buffalo", "🐃"),
    ("butterfly", "🦋"),
    ("badger", "🦡"),
    ("bear", "🐻"),
    ("crab", "🦀"),
    ("deer", "🦌"),
    ("duck", "🦆"),
    ("frog", "🐸"),
    ("goat", "🐐"),
    ("lion", "🦁"),
    ("wolf", "🐺"),
    ("horse", "🐴"),
    ("koala", "🐨"),
    ("llama", "🦙"),
    ("mouse", "🐭"),
    ("otter", "🦦"),
    ("panda", "🐼"),
    ("peacock", "🦚"),
    ("penguin", "🐧"),
    ("shark", "🦈"),
    ("sheep", "🐑"),
    ("sloth", "🦥"),
    ("snail", "🐌"),
    ("snake", "🐍"),
    ("spider", "🧶"),
    ("squid", "🦑"),
    ("swan", "🦢"),
    ("t-rex", "🦖"),
    ("tiger", "🐯"),
    ("turkey", "🦃"),
    ("whale", "🐋"),
    ("turtle", "🐢"),
    ("rabbit", "🐰"),
    ("parrot", "🦜"),
    ("jaguar", "🐆"),
    ("lizard", "🦎"),
    ("monkey", "🐒"),
    ("gorilla", "🦍"),
    ("orangutan", "🦧"),
    ("camel", "🐫"),
    ("elephant", "🐘"),
    ("rhino", "🦏"),
    ("hippo", "🦛"),
    ("boar", "🐗"),
    ("unicorn", "🦄"),
    ("kangaroo", "🦘"),
    ("hedgehog", "🦔"),
    ("skunk", "🦨"),
    ("raccoon", "🦝"),
    ("flamingo", "🦩"),
    ("dolphin", "🐬"),
    ("octopus", "🐙"),
    ("scorpion", "🦂"),
    ("zebra", "🦓"),
    ("stallion", "🐎"),
    ("dromedary", "🐪"),
    ("hog", "🐖"),
    ("kitten", "🐈"),
    ("poodle", "🐩"),
    ("hare", "🐇"),
    ("vole", "🐁"),
    ("dragon", "🐉"),
    ("humpback", "🐳"),
    ("guppy", "🐠"),
    ("nautilus", "🐚"),
    ("hatchling", "🐣"),
    ("wyvern", "🐲"),
    ("calf", "🐮"),
    ("macaque", "🐵"),
    ("tigress", "🐅"),
    // Additional terminal-safe identities. These deliberately stay on Unicode
    // 12 or older so they work in terminal tabs and window titles without a
    // bundled emoji font. `bee` is intentionally absent: 🐝 is reserved for the
    // global swarm marker rather than an individual client.
    ("puppy", "🐶"),
    ("duckling", "🐤"),
    ("mizaru", "🙈"),
    ("kikazaru", "🙉"),
    ("iwazaru", "🙊"),
    ("retriever", "🦮"),
    ("pawprint", "🐾"),
    ("piglet", "🐽"),
    ("bonehound", "🦴"),
    ("sabertooth", "🦷"),
    ("microbe", "🦠"),
    ("mushroom", "🍄"),
    ("cactus", "🌵"),
    ("clover", "🍀"),
    ("sunflower", "🌻"),
    ("hibiscus", "🌺"),
    ("blossom", "🌸"),
    ("daisy", "🌼"),
    ("tulip", "🌷"),
    ("rose", "🌹"),
    ("maple", "🍁"),
    ("seedling", "🌱"),
    ("evergreen", "🌲"),
    ("palmtree", "🌴"),
    ("herb", "🌿"),
];

/// Default session icon for identities the icon table does not know about
/// (e.g. new UUID-based sessions, or legacy names that predate the table).
const DEFAULT_SESSION_ICON: &str = "💫";

/// Per-identity icons for new UUID-based sessions. We hash the first byte of
/// the UUID into this pool so each session gets a stable, distinct visual
/// identity even though the name itself is opaque. Icons here are
/// alphacode-themed (geometric, modern, professional) and avoid the animal
/// tokens that older legacy sessions carried.
const UUID_ICONS: &[&str] = &[
    "◆", // ◆
    "◇", // ◇
    "●", // ●
    "○", // ○
    "▲", // ▲
    "△", // △
    "■", // ■
    "□", // □
    "★", // ★
    "☆", // ☆
    "✦", // ✦
    "✧", // ✧
    "⬢", // ⬢
    "⬡", // ⬡
    "◉", // ◉
    "◎", // ◎
];

/// Pick a stable icon for a UUID by hashing the first hex byte into the
/// `UUID_ICONS` pool. Used for new sessions; the legacy animal name table
/// (`SESSION_NAMES`) still wins for old on-disk sessions.
fn uuid_icon(uuid: &str) -> &'static str {
    // First two chars are a hex byte; parse to index. Fall back to the
    // default icon if parsing fails (e.g. legacy "island puppy" short names
    // that happen to match a UUID pattern but use a different scheme).
    let byte = u8::from_str_radix(uuid.get(..2).unwrap_or("00"), 16).unwrap_or(0);
    UUID_ICONS[(byte as usize) % UUID_ICONS.len()]
}

/// Get an emoji icon for a session/client name word.
///
/// `SESSION_NAMES` is retained as a lookup table for legacy on-disk sessions
/// whose short names are still one of the old animal tokens. New sessions
/// use UUIDs and resolve to a stable per-session geometric icon via
/// [`uuid_icon`]; truly unknown inputs fall through to `DEFAULT_SESSION_ICON`.
pub fn session_icon(name: &str) -> &'static str {
    SESSION_NAMES
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, icon)| *icon)
        .unwrap_or_else(|| {
            // Hex-only short names are UUIDs; everything else (e.g. legacy
            // animal names that slipped past the table, or completely
            // unrecognised input) gets the generic default.
            if name.len() >= 32 && name.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
                // Strip hyphens and look at the first hex byte.
                let compact: String = name.chars().filter(|c| *c != '-').collect();
                uuid_icon(&compact)
            } else {
                DEFAULT_SESSION_ICON
            }
        })
}

/// Per-identity icons for new UUID-based servers. Same hashing strategy as
/// `UUID_ICONS` so a server gets a stable geometric mark even though its
/// short name is now a UUID.
const SERVER_UUID_ICONS: &[&str] = &[
    "▲", // ▲
    "▼", // ▼
    "◆", // ◆
    "◇", // ◇
    "●", // ●
    "○", // ○
    "■", // ■
    "□", // □
    "◢", // ◢
    "◣", // ◣
    "◤", // ◤
    "◥", // ◥
];

/// Pick a stable icon for a server UUID by hashing the first hex byte.
fn server_uuid_icon(uuid: &str) -> &'static str {
    let byte = u8::from_str_radix(uuid.get(..2).unwrap_or("00"), 16).unwrap_or(0);
    SERVER_UUID_ICONS[(byte as usize) % SERVER_UUID_ICONS.len()]
}

/// Get an emoji icon for a server/location name word.
///
/// `SERVER_MODIFIERS` retains the legacy location-noun table so old on-disk
/// servers still get their harbors/forests/etc. New servers are identified by
/// UUID and pick a stable geometric mark via [`server_uuid_icon`].
pub fn server_icon(name: &str) -> &'static str {
    SERVER_MODIFIERS
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, icon)| *icon)
        .unwrap_or_else(|| {
            if name.len() >= 32 && name.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
                let compact: String = name.chars().filter(|c| *c != '-').collect();
                server_uuid_icon(&compact)
            } else {
                "🔮"
            }
        })
}

/// Generate a memorable server name using a UUID.
/// Returns (full_id, short_name) where:
/// - full_id is the storage identifier like "server_550e8400-e29b-41d4-a716-446655440000"
/// - short_name is the UUID portion
pub fn new_memorable_server_id() -> (String, String) {
    let short_name = Uuid::new_v4().to_string();
    let full_id = format!("server_{short_name}");
    (full_id, short_name)
}

/// Try to extract the memorable name from a server ID
/// e.g., "server_550e8400-e29b-41d4-a716-446655440000" -> Some("550e8400-e29b-41d4-a716-446655440000")
/// Also accepts the legacy "server_blazing_1234567890..." form.
/// Returns None for empty input, the bare prefix, or anything that doesn't
/// start with "server_".
#[cfg(test)]
pub fn extract_server_name(server_id: &str) -> Option<&str> {
    let rest = server_id.strip_prefix("server_")?;
    if rest.is_empty() {
        return None;
    }
    if let Some(pos) = rest.find('_') {
        return Some(&rest[..pos]);
    }
    Some(rest)
}

/// Generate a memorable session name
/// Returns (full_id, short_name) where:
/// - full_id is the storage identifier like "session_550e8400-e29b-41d4-a716-446655440000"
/// - short_name is the memorable part like "550e8400-e29b-41d4-a716-446655440000"
pub fn new_memorable_session_id() -> (String, String) {
    new_memorable_session_id_avoiding(&HashSet::new())
}

/// Generate a memorable session identity that avoids names already held by
/// active sessions. Each identity is a v4 UUID, so collisions only happen when
/// `used_names` explicitly contains a UUID we just emitted; in that case the
/// function regenerates until it finds an unused one. Exhaustion after a
/// reasonable number of attempts degrades to reuse rather than blocking
/// session creation.
pub fn new_memorable_session_id_avoiding(used_names: &HashSet<String>) -> (String, String) {
    for _ in 0..16 {
        let short_name = Uuid::new_v4().to_string();
        if !used_names.contains(&short_name) {
            return (format!("session_{short_name}"), short_name);
        }
    }
    // All 16 attempts collided (essentially impossible in practice). Fall
    // back to a final UUID to keep session creation non-blocking.
    let short_name = Uuid::new_v4().to_string();
    (format!("session_{short_name}"), short_name)
}

/// Try to extract the memorable name from a session ID
/// e.g., "session_fox_1234567890" -> Some("fox") (legacy animal format)
/// e.g., "session_550e8400-e29b-41d4-a716-446655440000" -> Some("550e8400-e29b-41d4-a716-446655440000") (current UUID format)
/// Returns None for empty input, the bare prefix, or anything that doesn't
/// start with "session_".
pub fn extract_session_name(session_id: &str) -> Option<&str> {
    let rest = session_id.strip_prefix("session_")?;
    if rest.is_empty() {
        return None;
    }
    // Session names are the first token after the prefix.
    // This supports old IDs (session_name_ts) and new IDs that are
    // either a UUID (session_<uuid>) or the old random form
    // (session_name_ts_rand).
    if let Some(pos) = rest.find('_') {
        return Some(&rest[..pos]);
    }
    // No underscore after the prefix means the entire remainder is the
    // name (e.g. session_<uuid> with no separator).
    Some(rest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_memorable_session_id_is_uuid() {
        let (full_id, short_name) = new_memorable_session_id();

        // Full ID is "session_<uuid>" — no animal token, no timestamp.
        assert!(full_id.starts_with("session_"));
        assert!(
            Uuid::parse_str(&short_name).is_ok(),
            "short name must be a UUID"
        );
        assert_eq!(full_id, format!("session_{short_name}"));
    }

    #[test]
    fn test_extract_session_name_handles_legacy_and_uuid() {
        // Legacy animal-token format still parses (so old on-disk sessions
        // continue to display their original name).
        assert_eq!(extract_session_name("session_fox_1234567890"), Some("fox"));
        assert_eq!(
            extract_session_name("session_fox_1234567890_deadbeefcafebabe"),
            Some("fox")
        );
        assert_eq!(
            extract_session_name("session_blue-whale_1234567890"),
            Some("blue-whale")
        );
        // Pure UUID format parses as the whole UUID token.
        assert_eq!(
            extract_session_name("session_550e8400-e29b-41d4-a716-446655440000"),
            Some("550e8400-e29b-41d4-a716-446655440000")
        );
        // Numeric-only legacy format still extracts the first token.
        assert_eq!(
            extract_session_name("session_1234567890_9876543210"),
            Some("1234567890")
        );
        assert_eq!(extract_session_name("invalid"), None);
        assert_eq!(extract_session_name("session_"), None);
    }

    #[test]
    fn test_unique_session_ids() {
        let ids: std::collections::HashSet<String> =
            (0..512).map(|_| new_memorable_session_id().0).collect();
        assert_eq!(
            ids.len(),
            512,
            "session IDs should stay unique in tight bursts"
        );
    }

    #[test]
    fn test_legacy_names_still_have_icons() {
        // The animal-token table is retained so that sessions whose short
        // name is still a legacy animal keep their original icon.
        for (name, expected_icon) in SESSION_NAMES {
            let icon = session_icon(name);
            assert_eq!(icon, *expected_icon, "Icon mismatch for '{}'", name);
            assert_ne!(icon, "💫", "Name '{}' should have a specific icon", name);
        }
    }

    #[test]
    fn test_uuid_sessions_get_stable_geometric_icons() {
        // Same UUID => same icon. Different UUIDs typically get different
        // icons (collision chance is 1/UUID_ICONS.len()).
        let a = "550e8400-e29b-41d4-a716-446655440000";
        let b = "ffffffff-ffff-ffff-ffff-ffffffffffff";
        assert_ne!(session_icon(a), "💫");
        assert_eq!(session_icon(a), session_icon(a));
        // The 0x55 prefix and 0xff prefix should map to different slots
        // in any reasonable pool of 16 entries.
        assert_ne!(session_icon(a), session_icon(b));
    }

    #[test]
    fn test_unknown_input_falls_back_to_default_icon() {
        // Names that aren't in either legacy table and aren't UUID-shaped hit
        // the default `💫` fallback. (The legacy tables still cover the old
        // animal/location names; unknown UUIDs are handled by the
        // `test_uuid_sessions_get_stable_geometric_icons` test above.)
        assert_eq!(session_icon("not-in-any-table"), "💫");
        assert_eq!(session_icon(""), "💫");
        assert_eq!(session_icon("a"), "💫");
    }

    #[test]
    fn test_server_uuid_icon_stable_and_distinct() {
        let a = "550e8400-e29b-41d4-a716-446655440000";
        let b = "ffffffff-ffff-ffff-ffff-ffffffffffff";
        assert_ne!(server_icon(a), "🔮");
        assert_eq!(server_icon(a), server_icon(a));
        assert_ne!(server_icon(a), server_icon(b));
    }

    #[test]
    fn avoiding_allocator_respects_used_names() {
        // With a small used_names set, allocator must not reuse them.
        let mut used = HashSet::new();
        for _ in 0..32 {
            let (id, name) = new_memorable_session_id_avoiding(&used);
            assert!(id.starts_with("session_"));
            assert!(Uuid::parse_str(&name).is_ok());
            assert!(used.insert(name), "allocator reused an in-use UUID");
        }
        assert_eq!(used.len(), 32);
    }

    /// Returns true for emoji that commonly fail to render as a single glyph on
    /// older terminal fonts or in window titles: ZWJ sequences (split into
    /// pieces), codepoints added in Unicode 13.0 or later (rendered as tofu
    /// boxes on fonts that predate them), and VS16 variation sequences
    /// (text-default codepoints + U+FE0F, which macOS window/tab title fonts
    /// render as monochrome outlines or tofu because the title renderer
    /// ignores the emoji-presentation selector - the Ghostty-on-macOS bug).
    /// We avoid a broad block range here because the Supplemental Symbols
    /// block mixes safe Unicode 11/12 emoji (otter, sloth) with risky Unicode
    /// 13+ ones (mammoth, beaver), so we list the unsafe codepoints
    /// explicitly.
    fn is_fragile_emoji(emoji: &str) -> bool {
        // Unicode 13.0+ additions in the Supplemental Symbols block (U+1F900..U+1F9FF).
        const UNSAFE_SUPPLEMENTAL: &[u32] = &[
            0x1F9A3, // 🦣 mammoth (13.0)
            0x1F9A4, // 🦤 dodo (13.0)
            0x1F9AB, // 🦫 beaver (13.0)
            0x1F9AC, // 🦬 bison (13.0)
            0x1F9AD, // 🦭 seal (13.0)
        ];
        emoji.chars().any(|c| {
            let cp = c as u32;
            c == '\u{200D}'
                // VS16: emoji needing it are text-default and misrender in titles.
                || c == '\u{FE0F}'
                // Symbols and Pictographs Extended-A (entirely Unicode 13+).
                || (0x1FA70..=0x1FAFF).contains(&cp)
                || UNSAFE_SUPPLEMENTAL.contains(&cp)
        })
    }

    #[test]
    fn session_icons_render_as_single_safe_glyphs() {
        for (name, emoji) in SESSION_NAMES {
            assert!(
                !is_fragile_emoji(emoji),
                "session name '{}' uses fragile emoji '{}' (ZWJ or Unicode 13+); \
                 pick a single widely-supported codepoint instead",
                name,
                emoji
            );
        }
    }

    #[test]
    fn session_names_and_icons_are_unique() {
        let mut names = std::collections::HashSet::new();
        let mut icons = std::collections::HashSet::new();
        for (name, emoji) in SESSION_NAMES {
            assert!(names.insert(*name), "duplicate session name '{}'", name);
            assert!(
                icons.insert(*emoji),
                "duplicate session icon '{}' (reused by '{}')",
                emoji,
                name
            );
        }
    }

    #[test]
    fn server_icons_render_as_single_safe_glyphs() {
        for (name, emoji) in SERVER_MODIFIERS {
            assert!(
                !is_fragile_emoji(emoji),
                "server name '{}' uses fragile emoji '{}' (ZWJ or Unicode 13+); \
                 pick a single widely-supported codepoint instead",
                name,
                emoji
            );
        }
    }

    #[test]
    fn test_new_memorable_server_id_is_uuid() {
        let (full_id, short_name) = new_memorable_server_id();

        // Full ID is "server_<uuid>" — no location noun, no timestamp.
        assert!(full_id.starts_with("server_"));
        assert!(
            Uuid::parse_str(&short_name).is_ok(),
            "short name must be a UUID"
        );
        assert_eq!(full_id, format!("server_{short_name}"));
    }

    #[test]
    fn test_extract_server_name() {
        // Legacy location-noun format still parses (so old on-disk servers
        // continue to display their original name).
        assert_eq!(
            extract_server_name("server_blazing_1234567890"),
            Some("blazing")
        );
        assert_eq!(
            extract_server_name("server_blazing_1234567890_deadbeefcafebabe"),
            Some("blazing")
        );
        assert_eq!(
            extract_server_name("server_rising_1234567890"),
            Some("rising")
        );
        // Pure UUID format parses as the whole UUID token.
        assert_eq!(
            extract_server_name("server_550e8400-e29b-41d4-a716-446655440000"),
            Some("550e8400-e29b-41d4-a716-446655440000")
        );
        assert_eq!(extract_server_name("invalid"), None);
        assert_eq!(extract_server_name("server_"), None);
    }

    #[test]
    fn test_unique_server_ids() {
        let ids: std::collections::HashSet<String> =
            (0..256).map(|_| new_memorable_server_id().0).collect();
        assert_eq!(
            ids.len(),
            256,
            "server IDs should stay unique in tight bursts"
        );
    }

    #[test]
    fn test_legacy_modifiers_still_have_icons() {
        for (name, expected_icon) in SERVER_MODIFIERS {
            let icon = server_icon(name);
            assert_eq!(icon, *expected_icon, "Icon mismatch for '{}'", name);
            assert_ne!(
                icon, "🔮",
                "Modifier '{}' should have a specific icon",
                name
            );
        }
    }
}
