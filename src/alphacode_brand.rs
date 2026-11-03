//! Canonical product identity for user-visible chrome.
//!
//! Historically every surface spelled the product name itself: the header
//! wordmark, the splash banner, the status bar, the terminal window title and
//! the side-panel session widget each embedded their own literal, and they had
//! drifted (`alphacode` vs `Alphacode` vs `ALPHACODE`). Reading the name from
//! one module makes that class of drift impossible and gives a single edit
//! point for a future rename.
//!
//! # Display strings vs. identifiers
//!
//! Only [`DISPLAY_NAME`], [`MARK`] and [`HOMEPAGE`] are display values and are
//! free to change. [`SLUG`] is the lowercase token that already appears inside
//! machine-facing and sentence-facing contexts — the executable name, the
//! `alphacode doctor` invocation, the docs/URL stem. Anything that is a
//! *protocol* or *interface* value is deliberately **not** here and stays
//! lowercase at its own definition site:
//!
//! - environment variables (`ALPHACODE_*`),
//! - process-title prefixes parsed elsewhere (`alphacode:c:` — see
//!   [`crate::alphacode_base::process_title`] and
//!   [`crate::alphacode_base::dictation`]),
//! - provider ids, route ids, config keys, on-disk file and directory names.
//!
//! Do not route those through this module.

/// Canonical product display name — the spelling every piece of user-facing
/// chrome should use (header wordmark, splash, widget titles, window title).
pub const DISPLAY_NAME: &str = "AlphaCode";

/// Lowercase slug used where a conventional binary/sentence token is expected
/// (`alphacode`, `alphacode doctor`, `alphacode/octopus`).
pub const SLUG: &str = "alphacode";

/// Compact brand mark paired with the name in dense chrome (composer border,
/// widget titles). A single codepoint with default text presentation and no
/// VS16, so it cannot invalidate ratatui's cell-width cache mid-frame the way
/// an emoji-presentation sequence can.
pub const MARK: &str = "\u{25c6}"; // ◆

/// Project homepage, shown in the header on wide terminals.
pub const HOMEPAGE: &str = "https://alphacli.github.io/";

/// Shared warm-gold identity used by banners, focus and session chrome.
pub const GOLD: (u8, u8, u8) = (212, 175, 55);
/// Selected surfaces keep gold quiet enough for readable body text.
pub const GOLD_SURFACE: (u8, u8, u8) = (59, 50, 30);

/// Narrowest wordmark, used once the ASCII-art tiers cannot fit (roughly
/// 17 columns and up). Kept here, rather than inline in each banner tier list,
/// because the header and the splash screen each carry their own copy of the
/// tier art and the two had already drifted — the splash's copy was a *raw*
/// string, so it rendered the literal characters `\u{2039}` instead of `‹`.
/// A shared `&str` cannot regress that way.
pub const WORDMARK_MINIMAL: &str = "‹ AlphaCode ›";

/// Spaced-letter wordmark tier, one step wider than [`WORDMARK_MINIMAL`].
/// Shared by the header and splash tier lists for the same reason: the two
/// copies cannot drift if there is only one.
pub const WORDMARK_MONOSPACE: &str = "A L P H A C O D E";

/// Full block-letter wordmark -- the widest art tier (6 rows, 73 cells).
///
/// This art previously lived in three hand-maintained copies: the CLI
/// startup banner, the TUI header, and the TUI splash. They drifted. The CLI
/// copy was edited into ragged, unreadable glyph runs (rows of 33/32/41
/// cells spelling nothing -- the console showed `╔═╗╔═╗╔╦╗` garbage instead
/// of the wordmark), and the splash's narrowest tier was written as a raw
/// string so it printed its own definition text. One definition makes both
/// failures impossible.
///
/// The rows are **plain text, with no embedded SGR escapes**. Colour is the
/// renderer's job (`BrandTheme::gradient_rows` draws the gradient across the
/// rows); art that carried its own escapes broke in two ways at once: the
/// tier selector measured the escape bytes as printable cells and therefore
/// thought this art was 362 columns wide, and ratatui -- which treats `Span`
/// text as literal cells -- painted the escape sequences instead of applying
/// them. Keep this array free of `\u{1b}`.
pub const WORDMARK_BLOCK: &[&str] = &[
    r" █████╗ ██╗     ██████╗ ██╗  ██╗ █████╗  ██████╗ ██████╗ ██████╗ ███████╗",
    r"██╔══██╗██║     ██╔══██╗██║  ██║██╔══██╗██╔════╝██╔═══██╗██╔══██╗██╔════╝",
    r"███████║██║     ██████╔╝███████║███████║██║     ██║   ██║██║  ██║█████╗  ",
    r"██╔══██║██║     ██╔═══╝ ██╔══██║██╔══██║██║     ██║   ██║██║  ██║██╔══╝  ",
    r"██║  ██║███████╗██║     ██║  ██║██║  ██║╚██████╗╚██████╔╝██████╔╝███████╗",
    r"╚═╝  ╚═╝╚══════╝╚═╝     ╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝ ╚═════╝ ╚═════╝ ╚══════╝",
];

/// Condensed block letters retain the full banner's six-row silhouette at 53 cells.
pub const WORDMARK_COMPACT: &[&str] = &[
    r" ███╗ ██    ████╗ ██ ██  ███╗  ███╗  ███╗ ████╗ █████",
    r"██ ██ ██    ██ ██ ██ ██ ██ ██ ██    ██ ██ ██ ██ ██   ",
    r"█████ ██    ████╝ █████ █████ ██    ██ ██ ██ ██ ████ ",
    r"██ ██ ██    ██    ██ ██ ██ ██ ██    ██ ██ ██ ██ ██   ",
    r"██ ██ █████ ██    ██ ██ ██ ██  ███╝  ███╝ ████╝ █████",
    r"╚╝ ╚╝ ╚═══╝ ╚╝    ╚╝ ╚╝ ╚╝ ╚╝  ╚══╝  ╚══╝ ╚═══╝ ╚═══╝",
];

/// Narrow block letters keep the same family and height at 35 cells.
pub const WORDMARK_NARROW: &[&str] = &[
    r"▄█▄ █   ██▄ █ █ ▄█▄ ▄██ ▄█▄ ██▄ ███",
    r"█ █ █   █ █ █ █ █ █ █   █ █ █ █ █  ",
    r"███ █   ██▀ ███ ███ █   █ █ █ █ ██ ",
    r"█ █ █   █   █ █ █ █ █   █ █ █ █ █  ",
    r"█ █ ███ █   █ █ █ █ ▀██ ▀█▀ ██▀ ███",
    r"▀ ▀ ▀▀▀ ▀   ▀ ▀ ▀ ▀  ▀▀  ▀  ▀▀  ▀▀▀",
];

/// One-row tiers, materialized as slices so they can sit in
/// [`WORDMARK_LADDER`] alongside the multi-row art.
pub const WORDMARK_MONOSPACE_ROWS: &[&str] = &[WORDMARK_MONOSPACE];
/// See [`WORDMARK_MONOSPACE_ROWS`].
pub const WORDMARK_MINIMAL_ROWS: &[&str] = &[WORDMARK_MINIMAL];

/// Art tiers in selection order, widest first. A renderer picks the first tier
/// whose rows fit its available width, so the CLI banner, the TUI header, and
/// the TUI splash always agree on which wordmark a given terminal width gets.
///
/// Every tier's rows share one display width -- ragged rows would misalign the
/// block art -- which the module tests assert.
pub const WORDMARK_LADDER: [&[&str]; 5] = [
    WORDMARK_BLOCK,
    WORDMARK_COMPACT,
    WORDMARK_NARROW,
    WORDMARK_MONOSPACE_ROWS,
    WORDMARK_MINIMAL_ROWS,
];

/// `"AlphaCode ◆"`-style label with the mark leading, ready to drop into a
/// bordered widget title. Kept here so every widget title that shows the brand
/// uses identical spacing.
pub fn mark_label() -> String {
    format!("{MARK} {DISPLAY_NAME}")
}

/// Pick the widest wordmark tier that fits `width` display columns.
///
/// This is the one selection rule every surface shares: the CLI banner, the
/// TUI header and the TUI splash all call it, so a given terminal width always
/// yields the same art in all three. Returns `None` when even the narrowest
/// tier overruns, which lets a caller fall back to a plain text label instead
/// of wrapping the art.
pub fn wordmark_for(width: usize) -> Option<&'static [&'static str]> {
    WORDMARK_LADDER
        .iter()
        .copied()
        .find(|tier| tier.iter().all(|row| row_width(row) <= width))
}

/// Pick a single-row wordmark tier, for chrome that must occupy exactly one row
/// (a section rule, a picker title, a status strip).
///
/// This exists for callers with a hard one-row budget, **not** as the general
/// "persistent chrome" selector. The startup header is persistent too, and it
/// deliberately uses [`wordmark_for`] instead so the real logo is drawn: routing
/// it here collapsed a six-row wordmark into the `A L P H A C O D E` text tier,
/// which made the art look like it had disappeared.
pub fn compact_wordmark_for(width: usize) -> Option<&'static [&'static str]> {
    [WORDMARK_MONOSPACE_ROWS, WORDMARK_MINIMAL_ROWS]
        .into_iter()
        .find(|tier| tier.iter().all(|row| row_width(row) <= width))
}

/// Display width of one wordmark row, measured in terminal cells rather than
/// chars so double-width glyphs cannot shear the art.
fn row_width(row: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(row)
}

/// Branded terminal window title.
///
/// The window title is one of the few places the product name earns its space:
/// it is what window switchers, tab bars and the menubar show. The session
/// label follows a separator so the brand stays readable at any truncation.
pub fn window_title(label: Option<&str>, is_selfdev: bool) -> String {
    let suffix = if is_selfdev { " [self-dev]" } else { "" };
    match label.map(str::trim).filter(|label| !label.is_empty()) {
        Some(label) => format!("{DISPLAY_NAME} \u{00b7} {label}{suffix}"),
        None => format!("{DISPLAY_NAME}{suffix}"),
    }
}

/// Brand label padded for a ratatui `Block` title (`" ◆ AlphaCode "`).
pub fn widget_title() -> String {
    format!(" {MARK} {DISPLAY_NAME} ")
}

/// Branded section title for a widget that shows one named section, e.g.
/// `" ◆ AlphaCode · Session "`. Keeping the section name means the widget still
/// says what it is; the brand just rides alongside it.
pub fn section_title(section: &str) -> String {
    let section = section.trim();
    if section.is_empty() {
        return widget_title();
    }
    format!(" {MARK} {DISPLAY_NAME} \u{00b7} {section} ") // ·
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_uses_canonical_casing() {
        assert_eq!(DISPLAY_NAME, "AlphaCode");
        assert_eq!(SLUG, "alphacode");
    }

    #[test]
    fn mark_is_a_single_width_one_codepoint_without_vs16() {
        assert_eq!(MARK.chars().count(), 1, "mark must be one codepoint");
        assert!(
            !MARK.contains('\u{fe0f}'),
            "mark must not carry a VS16 emoji-presentation selector"
        );
        assert_eq!(
            unicode_width::UnicodeWidthStr::width(MARK),
            1,
            "mark must occupy exactly one cell"
        );
    }

    #[test]
    fn widget_title_is_padded_and_branded() {
        assert_eq!(widget_title(), " \u{25c6} AlphaCode ");
        assert_eq!(mark_label(), "\u{25c6} AlphaCode");
    }

    #[test]
    fn section_title_keeps_the_section_name_after_the_brand() {
        let title = section_title("Session");
        assert_eq!(title, " \u{25c6} AlphaCode \u{00b7} Session ");
        assert!(title.contains(DISPLAY_NAME));
        assert!(title.contains("Session"));
        // Fits the narrowest side-panel widget (MIN_WIDGET_WIDTH = 35) with
        // room for the surrounding border: 23 cells of title.
        assert!(
            unicode_width::UnicodeWidthStr::width(title.as_str()) <= 30,
            "section title must stay compact for narrow panels: {title:?}"
        );
    }

    #[test]
    fn section_title_falls_back_to_the_widget_title_when_section_is_empty() {
        assert_eq!(section_title(""), widget_title());
        assert_eq!(section_title("   "), widget_title());
    }

    #[test]
    fn wordmark_tiers_are_branded_and_render_their_glyphs() {
        assert_eq!(WORDMARK_MINIMAL, "\u{2039} AlphaCode \u{203a}");
        assert!(WORDMARK_MINIMAL.contains(DISPLAY_NAME));
        // The splash tier list once held this text in a *raw* string, so the
        // escapes never resolved and the narrowest banner printed its own
        // source text. Pin literal absence of backslash-u so that cannot come
        // back through either const.
        for wordmark in [WORDMARK_MINIMAL, WORDMARK_MONOSPACE] {
            assert!(
                !wordmark.contains("\\u"),
                "wordmark must be a real string, not raw escape text: {wordmark:?}"
            );
            assert!(
                !wordmark.contains("alphacode"),
                "wordmarks are display chrome and must use the branded spelling: {wordmark:?}"
            );
        }
        assert!(
            unicode_width::UnicodeWidthStr::width(WORDMARK_MINIMAL) <= 20,
            "minimal wordmark must fit the narrowest header tier"
        );
    }

    #[test]
    fn wordmark_ladder_is_wellformed_and_ordered_widest_first() {
        // Every tier must be non-empty and internally aligned: ragged rows
        // would shear the block-letter art. This is exactly the defect the
        // CLI's private copy had (rows of 33/32/41 cells spelling nothing),
        // and it is invisible until someone renders it.
        let mut previous_width = usize::MAX;
        for tier in WORDMARK_LADDER {
            assert!(
                !tier.is_empty(),
                "a wordmark tier must have at least one row"
            );
            let widths: Vec<usize> = tier
                .iter()
                .map(|row| unicode_width::UnicodeWidthStr::width(*row))
                .collect();
            let width = widths[0];
            assert!(
                widths.iter().all(|w| *w == width),
                "rows within a tier must share one width: {widths:?}"
            );
            assert!(
                width < previous_width,
                "ladder must be ordered widest-first: {width} after {previous_width}"
            );
            previous_width = width;
        }

        // The two text tiers must actually name the product; the art tiers
        // cannot be checked by substring, so pin their row counts instead to
        // catch an accidental truncation.
        assert_eq!(WORDMARK_LADDER.len(), 5, "the response ladder has 5 tiers");
        assert_eq!(WORDMARK_BLOCK.len(), 6, "block art is six rows");
        assert_eq!(WORDMARK_COMPACT.len(), 6, "compact art is six rows");
        assert_eq!(WORDMARK_NARROW.len(), 6, "narrow art is six rows");
        assert_eq!(WORDMARK_LADDER[3], WORDMARK_MONOSPACE_ROWS);
        assert_eq!(WORDMARK_LADDER[4], WORDMARK_MINIMAL_ROWS);

        // Row widths the renderers rely on when choosing a tier: the CLI
        // compares against terminal width, and the TUI subtracts a frame inset.
        assert_eq!(
            unicode_width::UnicodeWidthStr::width(WORDMARK_BLOCK[0]),
            73,
            "block art must stay 73 cells so width-based tier selection is stable"
        );
        assert_eq!(
            unicode_width::UnicodeWidthStr::width(WORDMARK_COMPACT[0]),
            53,
            "compact art must stay 53 cells"
        );
    }

    #[test]
    fn wordmark_for_picks_the_widest_fitting_tier() {
        assert_eq!(wordmark_for(200), Some(WORDMARK_BLOCK));
        assert_eq!(wordmark_for(73), Some(WORDMARK_BLOCK));
        // One column short of the block art falls to the compact tier.
        assert_eq!(wordmark_for(72), Some(WORDMARK_COMPACT));
        assert_eq!(wordmark_for(17), Some(WORDMARK_MONOSPACE_ROWS));
        // `‹ AlphaCode ›` is 13 cells.
        assert_eq!(wordmark_for(13), Some(WORDMARK_MINIMAL_ROWS));
        // Narrower than the minimal mark: no art at all, so callers can
        // degrade to a text label instead of wrapping.
        assert_eq!(wordmark_for(12), None);
        assert_eq!(wordmark_for(0), None);
    }

    /// Wordmark art must be plain text. A generator once baked per-cell SGR
    /// escapes into the block art, which broke two separate things: the tier
    /// selector measured the escape bytes as printable cells (reporting the art
    /// as 362 columns wide, so it was almost never selected and was centred
    /// from a bogus width), and ratatui -- which renders `Span` text as literal
    /// cells -- painted the escapes as glyphs instead of applying them.
    ///
    /// Colour belongs to the renderer (`BrandTheme::gradient_rows`), never to
    /// the art.
    #[test]
    fn wordmark_art_carries_no_ansi_escapes() {
        for tier in WORDMARK_LADDER {
            for row in tier {
                assert!(
                    !row.contains('\u{1b}'),
                    "wordmark rows must be plain text, found an ESC sequence: {row:?}"
                );
                assert!(
                    !row.contains("\\x1b"),
                    "wordmark rows must not contain literal escape text: {row:?}"
                );
            }
        }
    }

    /// Every width from 0 to 200 must resolve to a tier that actually fits, or
    /// to no tier at all. This is the property the CLI banner, the TUI header
    /// and the TUI splash all rely on: a renderer draws whatever this returns
    /// without re-checking widths, so a tier that overruns its own selection
    /// width would wrap the logo on screen.
    ///
    /// Sweeping every width (rather than a handful of boundary samples) is
    /// deliberate — the previous defect here was a tier whose rows were ragged,
    /// which passes any check that only looks at its first row.
    #[test]
    fn normal_terminal_widths_keep_the_block_letter_family() {
        for width in [35, 40, 53, 60, 73, 100, 200] {
            let art = wordmark_for(width).expect("block banner fits");
            assert_eq!(art.len(), 6, "width {width} must retain six-row block art");
            assert!(art.iter().any(|row| row.contains('█')));
        }
    }

    #[test]
    fn wordmark_for_never_returns_a_tier_wider_than_requested() {
        for width in 0..=200 {
            match wordmark_for(width) {
                Some(tier) => {
                    assert!(!tier.is_empty(), "empty tier selected at width {width}");
                    for row in tier {
                        let row_width = unicode_width::UnicodeWidthStr::width(*row);
                        assert!(
                            row_width <= width,
                            "tier row {row:?} is {row_width} cells \
                             but was selected for width {width}"
                        );
                    }
                }
                None => {
                    // Nothing fits only when even the narrowest tier overruns.
                    let narrowest = WORDMARK_LADDER[WORDMARK_LADDER.len() - 1];
                    let narrowest_width = narrowest
                        .iter()
                        .map(|row| unicode_width::UnicodeWidthStr::width(*row))
                        .max()
                        .unwrap_or(0);
                    assert!(
                        width < narrowest_width,
                        "width {width} fits the narrowest tier ({narrowest_width} cells) \
                         but selection returned none"
                    );
                }
            }
        }
    }

    /// The persistent (single-row) selector must satisfy the same contract, and
    /// must only ever return a text tier — never the splash art.
    #[test]
    fn compact_wordmark_for_never_returns_a_tier_wider_than_requested() {
        for width in 0..=200 {
            if let Some(tier) = compact_wordmark_for(width) {
                assert_eq!(tier.len(), 1, "persistent tiers are single-row at {width}");
                let row_width = unicode_width::UnicodeWidthStr::width(tier[0]);
                assert!(
                    row_width <= width,
                    "compact tier {:?} is {row_width} cells but was selected for width {width}",
                    tier[0]
                );
            }
        }
    }

    /// A renderer must never have to wrap or truncate the art: at every width,
    /// the tier chosen for persistent chrome is never wider than the tier chosen
    /// for the splash. Otherwise the header would clip a logo the splash shows
    /// in full.
    #[test]
    fn compact_selection_never_exceeds_the_splash_selection() {
        for width in 0..=200 {
            if let Some(compact) = compact_wordmark_for(width) {
                let compact_width = unicode_width::UnicodeWidthStr::width(compact[0]);
                let splash =
                    wordmark_for(width).expect("a fitting compact tier implies a splash tier");
                let splash_width = splash
                    .iter()
                    .map(|row| unicode_width::UnicodeWidthStr::width(*row))
                    .max()
                    .unwrap_or(0);
                assert!(
                    compact_width <= splash_width,
                    "compact tier ({compact_width}) is wider than \
                     splash tier ({splash_width}) at {width}"
                );
            }
        }
    }

    #[test]
    fn compact_wordmark_for_never_returns_multi_row_art() {
        // Persistent chrome must not spend several rows on a logo.
        for width in [200, 73, 40, 20] {
            let tier = compact_wordmark_for(width).expect("a text tier must fit");
            assert_eq!(tier.len(), 1, "persistent tiers are single-row");
            assert!(
                !std::ptr::eq(tier, WORDMARK_BLOCK) && !std::ptr::eq(tier, WORDMARK_COMPACT),
                "persistent chrome must not use splash art at width {width}"
            );
        }
        // `A L P H A C O D E` is 17 cells, `‹ AlphaCode ›` is 13.
        assert_eq!(compact_wordmark_for(17), Some(WORDMARK_MONOSPACE_ROWS));
        assert_eq!(compact_wordmark_for(16), Some(WORDMARK_MINIMAL_ROWS));
        assert_eq!(compact_wordmark_for(12), None);
    }

    #[test]
    fn window_title_uses_branded_spelling_and_keeps_the_label() {
        assert_eq!(window_title(None, false), "AlphaCode");
        assert_eq!(window_title(Some("  "), false), "AlphaCode");
        assert_eq!(
            window_title(Some("Release planning"), false),
            "AlphaCode \u{00b7} Release planning"
        );
        assert_eq!(
            window_title(Some("Release planning"), true),
            "AlphaCode \u{00b7} Release planning [self-dev]"
        );
        // The whole point: no lowercase drift in window chrome.
        assert!(!window_title(Some("x"), false).contains("alphacode"));
    }
}
