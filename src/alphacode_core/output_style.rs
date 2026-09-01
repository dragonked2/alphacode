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
    if grapheme.chars().any(|ch| matches!(ch, '✓' | '✔' | '✅')) {
        "+"
    } else if grapheme
        .chars()
        .any(|ch| matches!(ch, '✕' | '✗' | '❌' | '❎'))
    {
        "x"
    } else if grapheme.chars().any(|ch| matches!(ch, '⚠' | '🚨')) {
        "!"
    } else if grapheme
        .chars()
        .any(|ch| matches!(ch, '➡' | '👉' | '➜' | '➤'))
    {
        "->"
    } else if grapheme.chars().any(|ch| matches!(ch, '⬅' | '👈')) {
        "<-"
    } else {
        "*"
    }
}

/// ANSI color palette — consistent across CLI and TUI.
const COLOR_BLUE: &str = "\u{1b}[38;5;30m";
const COLOR_GREEN: &str = "\u{1b}[38;5;10m";
const COLOR_YELLOW: &str = "\u{1b}[38;5;226m";
const COLOR_RED: &str = "\u{1b}[38;5;196m";
const COLOR_GRAY: &str = "\u{1b}[38;5;247m";
const COLOR_RESET: &str = "\u{1b}[0m";

/// Format with a color. Returns a new String (not a Cow) so callers can chain formatting.
pub fn colorized(text: impl AsRef<str>, c: impl AsRef<str>) -> String {
    let mut s = String::new();
    s.push_str(c.as_ref());
    s.push_str(text.as_ref());
    s.push_str(COLOR_RESET);
    s
}

/// Status indicators.
pub fn status_success() -> &'static str { "+" }
pub fn status_warning() -> &'static str { "!" }
pub fn status_failure() -> &'static str { "x" }
pub fn status_loading() -> &'static str { "⟳" }

/// Colorized status indicators for richer terminal output.
pub fn status_success_colored() -> String { colorized("✔", "[38;5;10m") }
pub fn status_warning_colored() -> String { colorized("⚠", "[38;5;226m") }
pub fn status_failure_colored() -> String { colorized("✘", "[38;5;196m") }
pub fn status_info_colored() -> String { colorized("ℹ", "[38;5;45m") }



/// Compact header replacing the large ASCII logo.
pub fn compact_header(version: &str, provider: &str, model: &str, server: &str, workspace: &str) -> String {
    format!(
        "{color_cyan}╔══ {color_blue}AlphaCode {version} {color_cyan}══╗{color_reset} {color_gray}│{color_reset} {color_gray}Provider   {provider}{color_reset} {color_gray}│{color_reset} {color_gray}Model      {model}{color_reset} {color_gray}│{color_reset} {color_gray}Server     {server}{color_reset} {color_gray}│{color_reset} {color_gray}Workspace  {workspace}{color_reset} {color_green}Ready{color_reset}",
        color_cyan = "\u{1b}[38;5;45m",
        color_blue = COLOR_BLUE,
        color_green = COLOR_GREEN,
        color_reset = COLOR_RESET,
        color_gray = COLOR_GRAY,
    )
}

/// Copyright notice for terminal output.
pub fn copyright_notice(version: &str) -> String {
    let year = 2025;
    format!(
        "{color_cyan}AlphaCode{color_reset} {color_gray}v{version}  \u{00a9} {year} AlphaCode. All rights reserved.{color_reset}",
        color_cyan = "\u{1b}[38;5;45m",
        color_reset = COLOR_RESET,
        color_gray = COLOR_GRAY,
        version = version,
        year = year,
    )
}

/// Aligned header labels — no visual clutter.
pub fn aligned_header(label: &str, value: &str) -> String {
    format!("{label}{label_padding} {value}", label_padding = " ".repeat(10))
}

/// Clean prompt formatting.
pub fn prompt_header(title: &str) -> String {
    format!("{title}\n")
}
pub fn prompt_field(label: &str, value: &str) -> String {
    format!("{label}\n{value}\n")
}
pub fn prompt_subtle(text: &str) -> String {
    colorized(text, COLOR_GRAY)
}
pub fn prompt_action(text: &str) -> String {
    colorized(text, COLOR_YELLOW)
}

/// Concise progress messages.
pub fn progress(text: &str) -> String {
    colorized(text, COLOR_BLUE)
}

/// Rewrite errors to be short, actionable, and readable.
pub fn error_reason(reason: &str) -> String {
    reason.lines()
        .filter(|line| !line.is_empty() && !line.trim().starts_with('['))
        .take(3)
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn error_message(reason: &str) -> String {
    format!("{COLOR_RED}{error}{COLOR_RESET}", error = error_reason(reason))
}
pub fn error_action(reason: &str) -> String {
    let reason = reason.lines().next().unwrap_or(reason);
    colorized(reason, COLOR_YELLOW)
}

/// Collapse multiple success messages.
pub fn success_message(text: &str) -> String {
    colorized(text, COLOR_GREEN)
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
                "🐝 ready ✅ warning ⚠️ failed ❌ family 👨‍👩‍👧‍👦 tone 👋🏽 flag 🇺🇸 key 1️⃣"
            ),
            "* ready + warning ! failed x family * tone * flag * key *"
        );
    }

    #[test]
    fn non_emoji_unicode_is_preserved() {
        assert_eq!(
            replace_emoji_with_ascii("box ─│ arrows →←↔ CJK 中文 math α © ® ✓ ✗ ⚠"),
            "box ─│ arrows →←↔ CJK 中文 math α © ® ✓ ✗ ⚠"
        );
        assert_eq!(replace_emoji_with_ascii("text heart ♥︎"), "text heart ♥︎");
    }
}
