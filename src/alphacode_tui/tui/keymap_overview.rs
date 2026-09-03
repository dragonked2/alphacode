//! Centralized keymap overview — single source of truth for the keybinding
//! rows shown in `/help`, `/keys`, and any future keymap picker.
//!
//! # Why this exists
//!
//! `/help` previously had 30+ hardcoded `key_entry("Ctrl+X", "...")` calls
//! scattered through `ui_overlays::draw_help_overlay`. Three problems:
//!
//! 1. **No single edit point.** Renaming a binding or adding a new one
//!    meant finding the right `key_entry` line in a 600-line function.
//! 2. **Inconsistent formatting.** Some entries used `alt("T")` to resolve
//!    "Alt+T" with the platform-correct glyph (⌥ on macOS); others typed
//!    `"Alt+T"` literally; some used `"Cmd/Super+K / J"` shorthand.
//! 3. **No test surface.** The data was inseparable from the renderer, so
//!    a regression in any chord string went unnoticed.
//!
//! This module exposes a `KeymapOverview` value listing every
//! `(category, (chord, description))` triple the help overlay shows.
//! `draw_help_overlay` reads from it; tests assert non-empty
//! categories, distinct chords, and matching descriptions.
//!
//! # Adding a new binding
//!
//! Add a `(Category::Foo, "Chord", "Description")` triple to the right
//! category in [`overview`]. If the chord is platform-conditional,
//! use [`crate::alphacode_tui_core::keybind::alt_chord`] or
//! [`crate::alphacode_tui_core::keybind::format_binding`] inside the
//! description so the helper resolves it correctly. The tests in this
//! file do not assert specific chord strings (they are platform-aware)
//! but they do fail if any description is empty.

/// Top-level grouping for keybind rows. The order of variants here is
/// the order categories appear in `/help`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Category {
    Navigation,
    DiagramsAndDiffs,
    AgentControls,
    InputAndHistory,
    SessionAndResume,
}

impl Category {
    /// Human-readable heading rendered at the top of each section in
    /// `/help`. Match the historical wording so existing help-overlay
    /// screenshots and tests stay byte-identical.
    pub const fn heading(self) -> &'static str {
        match self {
            Self::Navigation => "Navigation",
            Self::DiagramsAndDiffs => "Diagrams & Diffs",
            Self::AgentControls => "Agent Controls",
            Self::InputAndHistory => "Input & History",
            Self::SessionAndResume => "Session & Resume",
        }
    }
}

/// One row in the keymap: the chord (a string the renderer draws
/// directly) and a one-line description.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeymapRow {
    /// Display label. May be a literal "Ctrl+J" or a chord-rendered
    /// "Alt+T" via `alt_chord()`. Stored as a static `&'static str` so
    /// the table is `const`-evaluable and cheap to clone.
    pub chord: &'static str,
    /// One-line description of the action.
    pub description: &'static str,
}

impl KeymapRow {
    pub const fn new(chord: &'static str, description: &'static str) -> Self {
        Self { chord, description }
    }
}

/// One section of the keymap.
#[derive(Debug, Clone, Copy)]
pub struct KeymapSection {
    pub category: Category,
    pub rows: &'static [KeymapRow],
}

impl KeymapSection {
    pub const fn new(category: Category, rows: &'static [KeymapRow]) -> Self {
        Self { category, rows }
    }
}

/// The full keymap. Order is preserved across categories and within each
/// category so the rendered output is deterministic.
///
/// Where possible, chord strings are written so that
/// `KeyBinding::matches` semantics are obvious from the printed label.
/// Platform-conditional chords (Alt-vs-Cmd on macOS) are resolved at
/// render time by calling the platform-aware helpers from
/// `crate::alphacode_tui_core::keybind`; the static strings here
/// document the *non-macOS* default, which `/help` already used.
pub const OVERVIEW: &[KeymapSection] = &[
    KeymapSection::new(
        Category::Navigation,
        &[
            KeymapRow::new("PageUp / PageDown", "Scroll history"),
            KeymapRow::new("Up / Down", "Scroll history (when input empty)"),
            KeymapRow::new(
                "Ctrl+J / Ctrl+K",
                "Jump to next / previous user prompt (also Ctrl+] / Ctrl+[)",
            ),
            KeymapRow::new(
                "Ctrl+Shift+J / Ctrl+Shift+K",
                "Scroll history down / up one line",
            ),
            KeymapRow::new(
                "Cmd/Super+K / J",
                "Jump to previous / next user prompt (macOS, if forwarded)",
            ),
            KeymapRow::new("Ctrl+1..4", "Resize side panel to 25/50/75/100%"),
            KeymapRow::new("Ctrl+5..9", "Jump by recency (5 = 5th most recent)"),
        ],
    ),
    KeymapSection::new(
        Category::DiagramsAndDiffs,
        &[
            KeymapRow::new("Alt+M", "Toggle side panel (or diagram pane if empty)"),
            KeymapRow::new("Alt+T", "Toggle diagram position (side/top)"),
            KeymapRow::new("Alt+Shift+I", "Show/hide inline images (persists)"),
            KeymapRow::new("Ctrl+H / Ctrl+L", "Focus chat / diagram / diffs"),
            KeymapRow::new(
                "Ctrl+L",
                "Clear the view, keep context (/cls; no pane focused)",
            ),
            KeymapRow::new("Ctrl+Left / Right", "Cycle diagrams (when diagram focused)"),
            KeymapRow::new("h/j/k/l / arrows", "Pan diagram (when focused)"),
            KeymapRow::new("[ / ]", "Zoom diagram (when focused)"),
            KeymapRow::new("+ / -", "Resize diagram pane"),
            KeymapRow::new("Alt+G / /diff", "Cycle diff mode"),
        ],
    ),
    KeymapSection::new(
        Category::AgentControls,
        &[
            KeymapRow::new("Ctrl+T", "Open the model picker"),
            KeymapRow::new("Ctrl+Y", "Accept fallback model after an error"),
            KeymapRow::new("Ctrl+C", "Interrupt the current turn (session preserved)"),
            KeymapRow::new("Esc", "Back / cancel current dialog"),
            KeymapRow::new("Ctrl+Tab / Ctrl+Shift+Tab", "Switch model next / prev"),
            KeymapRow::new(
                "Cmd+Right / Cmd+Left",
                "Increase / decrease reasoning effort (macOS)",
            ),
            KeymapRow::new(
                "Alt+Right / Alt+Left",
                "Increase / decrease reasoning effort",
            ),
        ],
    ),
    KeymapSection::new(
        Category::InputAndHistory,
        &[
            KeymapRow::new("Enter", "Submit prompt"),
            KeymapRow::new(
                "Shift+Enter",
                "Insert newline (requires kitty keyboard protocol)",
            ),
            KeymapRow::new("Up / Down", "Cycle prompt history (when input non-empty)"),
            KeymapRow::new("Ctrl+R", "Reverse prompt-history search"),
            KeymapRow::new("Alt+U / Alt+D", "Page up / down in history"),
            KeymapRow::new("Ctrl+G", "Bookmark current prompt"),
            KeymapRow::new("/", "Open slash-command palette when input is empty"),
        ],
    ),
    KeymapSection::new(
        Category::SessionAndResume,
        &[
            KeymapRow::new("/resume", "Browse and resume previous sessions"),
            KeymapRow::new("/save [label]", "Bookmark session for /resume"),
            KeymapRow::new("/unsave", "Remove bookmark from current session"),
            KeymapRow::new(
                "Ctrl+Q",
                "Quit (session is preserved; resume with alphacode --resume)",
            ),
            KeymapRow::new("F1", "Open this keymap overlay (planned; not yet bound)"),
        ],
    ),
];

/// Flatten the overview to `(chord, description)` pairs in declaration
/// order. Convenient for tests that want to assert "every chord has a
/// description" without threading through categories.
pub fn flat_rows() -> Vec<(Category, &'static str, &'static str)> {
    let mut out = Vec::new();
    for section in OVERVIEW {
        for row in section.rows {
            out.push((section.category, row.chord, row.description));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overview_is_nonempty() {
        assert!(
            !OVERVIEW.is_empty(),
            "Keymap overview must have at least one section"
        );
    }

    #[test]
    fn every_section_has_rows() {
        for section in OVERVIEW {
            assert!(
                !section.rows.is_empty(),
                "section {:?} has no rows",
                section.category
            );
        }
    }

    #[test]
    fn every_row_has_a_description() {
        for (cat, chord, desc) in flat_rows() {
            assert!(
                !desc.trim().is_empty(),
                "{:?}: chord {:?} has empty description",
                cat,
                chord
            );
        }
    }

    #[test]
    fn every_chord_is_nonempty() {
        for (_cat, chord, _desc) in flat_rows() {
            assert!(
                !chord.trim().is_empty(),
                "found a row with an empty chord string"
            );
        }
    }

    #[test]
    fn categories_appear_in_canonical_order() {
        let order: Vec<Category> = OVERVIEW.iter().map(|s| s.category).collect();
        let canonical = [
            Category::Navigation,
            Category::DiagramsAndDiffs,
            Category::AgentControls,
            Category::InputAndHistory,
            Category::SessionAndResume,
        ];
        for (i, expected) in canonical.iter().enumerate() {
            assert_eq!(order[i], *expected, "category {i} out of order");
        }
    }

    #[test]
    fn no_duplicate_chords_within_a_section() {
        // Cross-section duplicates are allowed (different context, same
        // chord is fine — e.g. Ctrl+J used in nav and history), but
        // within a single section a duplicate is almost always a typo.
        for section in OVERVIEW {
            let mut seen = std::collections::HashSet::new();
            for row in section.rows {
                assert!(
                    seen.insert(row.chord),
                    "{:?}: duplicate chord {:?}",
                    section.category,
                    row.chord
                );
            }
        }
    }

    #[test]
    fn heading_is_nonempty_for_every_category() {
        for cat in [
            Category::Navigation,
            Category::DiagramsAndDiffs,
            Category::AgentControls,
            Category::InputAndHistory,
            Category::SessionAndResume,
        ] {
            assert!(!cat.heading().is_empty(), "{:?} has an empty heading", cat);
        }
    }
}
