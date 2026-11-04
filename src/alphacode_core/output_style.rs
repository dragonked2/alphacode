use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};
use unicode_properties::emoji::{
    EmojiStatus, UnicodeEmoji, is_emoji_presentation_selector, is_regional_indicator,
    is_text_presentation_selector, is_zwj,
};
use unicode_segmentation::UnicodeSegmentation;

static EMOJI_ENABLED: AtomicBool = AtomicBool::new(true);

#[macro_export]
macro_rules! terminal_print {
    ($($arg:tt)*) => {{
        let message = ::std::format!($($arg)*);
        ::std::print!("{}", $crate::output_style::terminal_text(&message));
    }};
}

#[macro_export]
macro_rules! terminal_println {
    () => { ::std::println!() };
    ($($arg:tt)*) => {{
        let message = ::std::format!($($arg)*);
        ::std::println!("{}", $crate::output_style::terminal_text(&message));
    }};
}

#[macro_export]
macro_rules! terminal_eprint {
    ($($arg:tt)*) => {{
        let message = ::std::format!($($arg)*);
        ::std::eprint!("{}", $crate::output_style::terminal_text(&message));
    }};
}

#[macro_export]
macro_rules! terminal_eprintln {
    () => { ::std::eprintln!() };
    ($($arg:tt)*) => {{
        let message = ::std::format!($($arg)*);
        ::std::eprintln!("{}", $crate::output_style::terminal_text(&message));
    }};
}

/// Set whether terminal-facing output may contain emoji.
pub fn set_emoji_enabled(enabled: bool) {
    EMOJI_ENABLED.store(enabled, Ordering::Relaxed);
}

/// Return whether terminal-facing output may contain emoji.
pub fn emoji_enabled() -> bool {
    EMOJI_ENABLED.load(Ordering::Relaxed)
}

/// Adapt terminal-facing text to the configured emoji preference.
pub fn terminal_text(text: &str) -> Cow<'_, str> {
    terminal_text_with_emoji(text, emoji_enabled())
}

/// Adapt terminal-facing text using an explicit emoji preference.
pub fn terminal_text_with_emoji(text: &str, enabled: bool) -> Cow<'_, str> {
    if enabled || text.is_ascii() || !contains_emoji(text) {
        Cow::Borrowed(text)
    } else {
        Cow::Owned(replace_emoji_with_ascii(text))
    }
}

/// Replace emoji grapheme clusters with compact ASCII markers.
pub fn replace_emoji_with_ascii(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    for grapheme in text.graphemes(true) {
        if grapheme_is_emoji(grapheme) {
            output.push_str(emoji_ascii_fallback(grapheme));
        } else {
            output.push_str(grapheme);
        }
    }
    output
}

fn contains_emoji(text: &str) -> bool {
    text.graphemes(true).any(grapheme_is_emoji)
}

fn emoji_ascii_fallback(grapheme: &str) -> &'static str {
    if grapheme
        .chars()
        .any(|ch| matches!(ch, '\u{2713}' | '\u{2714}' | '\u{2705}'))
    {
        "+"
    } else if grapheme
        .chars()
        .any(|ch| matches!(ch, '\u{2715}' | '\u{2717}' | '\u{274c}' | '\u{274e}'))
    {
        "x"
    } else if grapheme
        .chars()
        .any(|ch| matches!(ch, '\u{26a0}' | '\u{1f6a8}'))
    {
        "!"
    } else if grapheme
        .chars()
        .any(|ch| matches!(ch, '\u{27a1}' | '\u{1f449}' | '\u{279c}' | '\u{27a3}'))
    {
        "->"
    } else if grapheme
        .chars()
        .any(|ch| matches!(ch, '\u{2b05}' | '\u{1f448}'))
    {
        "<-"
    } else {
        "*"
    }
}

const COLOR_RESET: &str = "\u{1b}[0m";
use super::output_enhance::PaletteToken;

fn tone(text: &str, token: PaletteToken, bold: bool) -> String {
    let (r, g, b) = super::output_enhance::pick_for_terminal_bg(token);
    if bold {
        truecolor_bold(text, r, g, b)
    } else {
        truecolor(text, r, g, b)
    }
}

/// Format with a color. Returns a new String (not a Cow) so callers can chain formatting.
///
/// The escape sequence is dropped when styling is off (piped output, `NO_COLOR`,
/// or a console that cannot render it), so callers always receive text they can
/// print verbatim.
pub fn colorized(text: impl AsRef<str>, c: impl AsRef<str>) -> String {
    if !super::console::ansi_enabled() {
        return text.as_ref().to_string();
    }
    let mut s = String::new();
    s.push_str(c.as_ref());
    s.push_str(text.as_ref());
    s.push_str(COLOR_RESET);
    s
}

/// Wrap text in a truecolor ANSI sequence for rich color output.
///
/// Gated like [`colorized`]: without styling support the text is returned as-is.
pub fn truecolor(text: impl AsRef<str>, r: u8, g: u8, b: u8) -> String {
    if !super::console::ansi_enabled() {
        return text.as_ref().to_string();
    }
    format!("\u{1b}[38;2;{r};{g};{b}m{}\u{1b}[0m", text.as_ref())
}

/// Bold + truecolor for emphasis.
///
/// Gated like [`colorized`]: without styling support the text is returned as-is.
pub fn truecolor_bold(text: impl AsRef<str>, r: u8, g: u8, b: u8) -> String {
    if !super::console::ansi_enabled() {
        return text.as_ref().to_string();
    }
    format!(
        "\u{1b}[1m\u{1b}[38;2;{r};{g};{b}m{}\u{1b}[0m",
        text.as_ref()
    )
}

/// Status indicators.
pub fn status_success() -> &'static str {
    "+"
}
pub fn status_warning() -> &'static str {
    "!"
}
pub fn status_failure() -> &'static str {
    "x"
}
pub fn status_loading() -> &'static str {
    "\u{27f3}"
}

/// Colorized status indicators for richer terminal output.
pub fn status_success_colored() -> String {
    tone("+", PaletteToken::Success, true)
}
pub fn status_warning_colored() -> String {
    tone("!", PaletteToken::Warn, true)
}
pub fn status_failure_colored() -> String {
    tone("x", PaletteToken::Error, true)
}
pub fn status_info_colored() -> String {
    tone("i", PaletteToken::Info, true)
}

/// Premium status badges with truecolor and box-drawing decorations.
pub fn badge_success(text: &str) -> String {
    tone(&format!(" + {text}"), PaletteToken::Success, true)
}
pub fn badge_warning(text: &str) -> String {
    tone(&format!(" ! {text}"), PaletteToken::Warn, true)
}
pub fn badge_error(text: &str) -> String {
    tone(&format!(" x {text}"), PaletteToken::Error, true)
}
pub fn badge_info(text: &str) -> String {
    tone(&format!(" i {text}"), PaletteToken::Info, true)
}

/// Styled label-value pairs for structured output.
pub fn label_value(label: &str, value: &str) -> String {
    format!(
        "{} {value}",
        tone(&format!("{label}:"), PaletteToken::Muted, false)
    )
}

/// A horizontal separator line.
pub fn separator(width: usize) -> String {
    let glyph = if super::console::unicode_supported() {
        "─"
    } else {
        "-"
    };
    tone(&glyph.repeat(width), PaletteToken::Muted, false)
}

/// Center the shared brand wordmark for a human-facing terminal.
///
/// Returns an empty string when no tier fits (or when the console cannot render
/// the glyphs and even the plain display name is too wide), so callers degrade
/// to a text heading rather than wrapping the art.
///
/// This lives in core — next to [`console`](crate::alphacode_core::console),
/// which answers the two questions a renderer must ask first ("may I emit
/// Unicode?" and "how wide is the terminal?") — because every surface needs it:
/// the `run` banner, the interactive REPL, and any future non-TUI entry point.
/// The previous CLI-only copy meant the REPL and other layers had no shared path
/// to the wordmark and simply printed a bare heading instead.
pub fn startup_banner(width: usize) -> String {
    startup_banner_with_unicode(width, crate::alphacode_core::console::unicode_supported())
}

/// [`startup_banner`] with the Unicode capability passed in explicitly.
///
/// Split out so the capability branch is testable without touching the
/// process-global console cache (and so callers that have already resolved the
/// answer need not ask again).
pub fn startup_banner_with_unicode(width: usize, unicode: bool) -> String {
    use unicode_width::UnicodeWidthStr;

    let rows: Option<&[&str]> = if unicode {
        crate::alphacode_brand::wordmark_for(width)
    } else if width >= crate::alphacode_brand::WORDMARK_ASCII_BLOCK_WIDTH {
        // Big pure-ASCII block art: keeps the banner Claude-Code-style and big
        // on terminals that cannot render Unicode box-drawing characters
        // (legacy consoles, dumb terminals, `TERM` without UTF-8), instead of
        // degrading to the tiny monospace tier.
        Some(crate::alphacode_brand::WORDMARK_ASCII_BLOCK)
    } else if width >= crate::alphacode_brand::WORDMARK_MONOSPACE.len() {
        Some(crate::alphacode_brand::WORDMARK_MONOSPACE_ROWS)
    } else if width >= crate::alphacode_brand::DISPLAY_NAME.len() {
        Some(&[crate::alphacode_brand::DISPLAY_NAME])
    } else {
        None
    };
    let Some(rows) = rows else {
        return String::new();
    };
    let mut lines = rows
        .iter()
        .map(|row| {
            let padding = width.saturating_sub(UnicodeWidthStr::width(*row)) / 2;
            format!("{}{row}", " ".repeat(padding))
        })
        .collect::<Vec<_>>();
    lines.push(if unicode { "─" } else { "-" }.repeat(width));
    lines.join("\n")
}

/// Render the shared cyan banner with styling when the terminal supports it.
pub fn styled_startup_banner(width: usize) -> String {
    use crate::alphacode_core::console;
    let banner = startup_banner(width);
    if !console::ansi_enabled() || banner.is_empty() {
        return banner;
    }
    let lines: Vec<&str> = banner.lines().collect();
    let art_height = lines.len().saturating_sub(1);
    lines
        .iter()
        .enumerate()
        .map(|(row_index, row)| {
            if row_index == art_height {
                return console::styled_line(row);
            }
            let mut styled = String::new();
            let mut previous = None;
            for glyph in row.chars() {
                let color = crate::alphacode_brand::wordmark_ink(glyph, row_index, art_height);
                if previous != Some(color) {
                    let (r, g, b) = color;
                    styled.push_str(&format!("\u{1b}[38;2;{r};{g};{b}m"));
                    previous = Some(color);
                }
                styled.push(glyph);
            }
            styled.push_str("\u{1b}[0m");
            styled
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Copyright notice for terminal output.
pub fn copyright_notice(version: &str) -> String {
    let year = 2026;
    format!(
        "{} {}",
        tone(
            crate::alphacode_brand::DISPLAY_NAME,
            PaletteToken::Accent,
            true
        ),
        tone(
            &format!("v{version}  © {year} AlphaCode. All rights reserved."),
            PaletteToken::Muted,
            false
        )
    )
}

/// Aligned header labels -- no visual clutter.
pub fn aligned_header(label: &str, value: &str) -> String {
    let label = super::output_enhance::pad_end_to(label, 10);
    format!("{} {value}", tone(&label, PaletteToken::Muted, false))
}

/// Clean prompt formatting.
pub fn prompt_header(title: &str) -> String {
    format!("{}\n", tone(title, PaletteToken::Accent, true))
}
pub fn prompt_field(label: &str, value: &str) -> String {
    format!("{}\n{value}\n", tone(label, PaletteToken::Muted, false))
}
pub fn prompt_subtle(text: &str) -> String {
    tone(text, PaletteToken::Muted, false)
}
pub fn prompt_action(text: &str) -> String {
    tone(text, PaletteToken::Accent, false)
}

/// Concise progress messages.
pub fn progress(text: &str) -> String {
    tone(text, PaletteToken::Accent, false)
}

/// Formatted progress with a spinner-style indicator.
pub fn progress_spinner(text: &str) -> String {
    let marker = if super::console::unicode_supported() {
        "⟳"
    } else {
        "..."
    };
    tone(&format!("{marker} {text}"), PaletteToken::Accent, false)
}

/// Rewrite errors to be short, actionable, and readable.
pub fn error_reason(reason: &str) -> String {
    reason
        .lines()
        .filter(|line| !line.is_empty() && !line.trim().starts_with('['))
        .take(3)
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn error_message(reason: &str) -> String {
    tone(
        &format!(" x {}", error_reason(reason)),
        PaletteToken::Error,
        true,
    )
}
pub fn error_action(reason: &str) -> String {
    let reason = reason.lines().next().unwrap_or(reason);
    tone(reason, PaletteToken::Warn, false)
}

/// Collapse multiple success messages.
pub fn success_message(text: &str) -> String {
    tone(text, PaletteToken::Success, false)
}

/// Multi-line info block with a colored left border.
pub fn info_block(title: &str, body: &str) -> String {
    let mut out = String::new();
    let rule = if super::console::unicode_supported() {
        "│"
    } else {
        "|"
    };
    out.push_str(&tone(
        &format!("  {rule} {title}"),
        PaletteToken::Accent,
        true,
    ));
    out.push('\n');
    for line in body.lines() {
        out.push_str(&format!(
            "{} {line}\n",
            tone(&format!("  {rule}"), PaletteToken::Muted, false)
        ));
    }
    out
}

fn grapheme_is_emoji(grapheme: &str) -> bool {
    let has_text_selector = grapheme.chars().any(is_text_presentation_selector);
    let has_emoji_selector = grapheme.chars().any(is_emoji_presentation_selector);
    if has_text_selector && !has_emoji_selector {
        return false;
    }

    let has_emoji_char = grapheme.chars().any(UnicodeEmoji::is_emoji_char);
    let regional_indicators = grapheme
        .chars()
        .filter(|ch| is_regional_indicator(*ch))
        .count();
    has_emoji_selector
        || grapheme.contains('\u{20E3}')
        || regional_indicators >= 2
        || (has_emoji_char && grapheme.chars().any(is_zwj))
        || grapheme.chars().any(|ch| {
            matches!(
                ch.emoji_status(),
                EmojiStatus::EmojiPresentation
                    | EmojiStatus::EmojiPresentationAndModifierBase
                    | EmojiStatus::EmojiPresentationAndEmojiComponent
                    | EmojiStatus::EmojiPresentationAndModifierAndEmojiComponent
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_clusters_use_readable_ascii_fallbacks() {
        assert_eq!(
            replace_emoji_with_ascii(
                "\u{1f41d} ready \u{2705} warning \u{26a0}\u{fe0f} failed \u{274c} \
                 family \u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}\u{200d}\u{1f466} \
                 tone \u{1f44b}\u{1f3fd} flag \u{1f1fa}\u{1f1f8} key \u{2619}\u{fe0f}\u{20e3}"
            ),
            "* ready + warning ! failed x family * tone * flag * key *"
        );
    }

    #[test]
    fn non_emoji_unicode_is_preserved() {
        assert_eq!(
            replace_emoji_with_ascii(
                "box \u{2500}\u{2502} arrows \u{2192}\u{2190}\u{2194} CJK \u{4e2d}\u{6587} \
                 math \u{03b1} \u{00a9} \u{00ae} \u{2713} \u{2717} \u{26a0}"
            ),
            "box \u{2500}\u{2502} arrows \u{2192}\u{2190}\u{2194} CJK \u{4e2d}\u{6587} \
             math \u{03b1} \u{00a9} \u{00ae} \u{2713} \u{2717} \u{26a0}"
        );
        assert_eq!(
            replace_emoji_with_ascii("text heart \u{2665}\u{fe0e}"),
            "text heart \u{2665}\u{fe0e}"
        );
    }
}
