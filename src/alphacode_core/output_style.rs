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
const COLOR_BOLD: &str = "\u{1b}[1m";

// Truecolor ANSI sequences for rich gradients and premium formatting
const TC_CYAN: &str = "\u{1b}[38;2;80;220;215m";
const TC_BLUE: &str = "\u{1b}[38;2;130;180;255m";
const TC_PURPLE: &str = "\u{1b}[38;2;170;130;255m";
const TC_GREEN: &str = "\u{1b}[38;2;120;210;120m";
const TC_YELLOW: &str = "\u{1b}[38;2;240;220;100m";
const TC_RED: &str = "\u{1b}[38;2;240;80;80m";
const TC_GRAY: &str = "\u{1b}[38;2;160;165;175m";
const TC_GRAY_DIM: &str = "\u{1b}[38;2;100;105;115m";
const TC_WHITE: &str = "\u{1b}[38;2;230;235;245m";
const TC_BG_DARK: &str = "\u{1b}[48;2;18;20;30m";
const TC_GRADIENT_A: &str = "\u{1b}[38;2;130;180;255m";
const TC_GRADIENT_B: &str = "\u{1b}[38;2;150;160;255m";
const TC_GRADIENT_C: &str = "\u{1b}[38;2;170;140;255m";
const TC_GRADIENT_D: &str = "\u{1b}[38;2;190;120;255m";

/// Format with a color. Returns a new String (not a Cow) so callers can chain formatting.
pub fn colorized(text: impl AsRef<str>, c: impl AsRef<str>) -> String {
    let mut s = String::new();
    s.push_str(c.as_ref());
    s.push_str(text.as_ref());
    s.push_str(COLOR_RESET);
    s
}

/// Wrap text in a truecolor ANSI sequence for rich color output.
pub fn truecolor(text: impl AsRef<str>, r: u8, g: u8, b: u8) -> String {
    format!("\u{1b}[38;2;{r};{g};{b}m{}\u{1b}[0m", text.as_ref())
}

/// Bold + truecolor for emphasis.
pub fn truecolor_bold(text: impl AsRef<str>, r: u8, g: u8, b: u8) -> String {
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
    colorized("\u{2714}", "\u{1b}[38;5;10m")
}
pub fn status_warning_colored() -> String {
    colorized("\u{26a0}", "\u{1b}[38;5;226m")
}
pub fn status_failure_colored() -> String {
    colorized("\u{2718}", "\u{1b}[38;5;196m")
}
pub fn status_info_colored() -> String {
    colorized("\u{2139}", "\u{1b}[38;5;45m")
}

/// Premium status badges with truecolor and box-drawing decorations.
pub fn badge_success(text: &str) -> String {
    format!("{TC_GREEN}{COLOR_BOLD} \u{2713} {text}{COLOR_RESET}")
}
pub fn badge_warning(text: &str) -> String {
    format!("{TC_YELLOW}{COLOR_BOLD} \u{26a0} {text}{COLOR_RESET}")
}
pub fn badge_error(text: &str) -> String {
    format!("{TC_RED}{COLOR_BOLD} \u{2717} {text}{COLOR_RESET}")
}
pub fn badge_info(text: &str) -> String {
    format!("{TC_CYAN}{COLOR_BOLD} \u{2139} {text}{COLOR_RESET}")
}

/// Styled label-value pairs for structured output.
pub fn label_value(label: &str, value: &str) -> String {
    format!("{TC_GRAY}{label}:{COLOR_RESET} {TC_WHITE}{value}{COLOR_RESET}")
}

/// A horizontal separator line.
pub fn separator(width: usize) -> String {
    format!("{TC_GRAY_DIM}{}{COLOR_RESET}", "\u{2500}".repeat(width))
}

/// Compact header replacing the large ASCII logo. Enhanced with truecolor gradient.
pub fn compact_header(
    version: &str,
    provider: &str,
    model: &str,
    server: &str,
    workspace: &str,
) -> String {
    format!(
        "{tc_bg}{TC_GRADIENT_A}{COLOR_BOLD} \u{2554}\u{2550}\u{2557} {COLOR_RESET} {TC_GRADIENT_A}AlphaCode{COLOR_RESET} {TC_GRADIENT_B}v{version}{COLOR_RESET} {TC_GRAY_DIM}\u{2502}{COLOR_RESET} {TC_GRAY}Provider   {TC_CYAN}{provider}{COLOR_RESET} {TC_GRAY_DIM}\u{2502}{COLOR_RESET} {TC_GRAY}Model      {TC_WHITE}{COLOR_BOLD}{model}{COLOR_RESET} {TC_GRAY_DIM}\u{2502}{COLOR_RESET} {TC_GRAY}Server     {TC_WHITE}{server}{COLOR_RESET} {TC_GRAY_DIM}\u{2502}{COLOR_RESET} {TC_GRAY}Workspace  {TC_WHITE}{workspace}{COLOR_RESET} {TC_GREEN}{COLOR_BOLD}\u{25cf} Ready{COLOR_RESET}",
        tc_bg = TC_BG_DARK,
        version = version,
        provider = provider,
        model = model,
        server = server,
        workspace = workspace,
    )
}

/// Full startup banner with ASCII art and gradient coloring. Rendered once at launch.
pub fn startup_banner(version: &str, provider: &str, model: &str) -> String {
    let banner = format!(
        "{TC_BG_DARK}{TC_GRADIENT_A}\n\
         {TC_GRADIENT_A}     \u{2554}\u{2550}\u{2557}\u{2554}\u{2550}\u{2557}\u{2554}\u{2566}\u{2557}\
         \u{2554}\u{2550}\u{2550}\u{2557}\u{2554}\u{2550}\u{2557}\
         \u{2554}\u{2566}\u{2557}\u{2554}\u{2550}\u{2557}\
         \u{2554}\u{2550}\u{2557}\u{2554}\u{2550}\u{2557}{COLOR_RESET}\n\
         {TC_GRADIENT_B}     \u{2560}\u{2550}\u{255d}\u{2551} \u{2551}\u{2551}\
         \u{2563}\u{256a}\u{255d}\u{2551} \u{2551} \u{2551}\
         \u{2560}\u{2550}\u{2563}\u{2551} \u{2551} \u{2551}\
         \u{2560}\u{2550}\u{2563}\u{2551}{COLOR_RESET}\n\
         {TC_GRADIENT_C}     \u{2569}  \u{255a}\u{2550}\u{2569}\
         \u{2569} \u{2569}\u{255a}\u{2550}\u{2569}\
         \u{2569}\u{2569}\u{2569}\u{2569}\u{2569}\u{2569}\
         \u{2569}\u{2569}\u{2569} \u{2569} \u{255a}\u{2550}\u{2569}\
         \u{2569} \u{2569} \u{255a}\u{2550}\u{2569}\
         \u{255a}\u{2569}{COLOR_RESET}\n\
         {TC_GRADIENT_D}     {TC_GRAY_DIM}Multi-model orchestration \
         \u{00b7} swarm coordination{COLOR_RESET}\n\
         {TC_BG_DARK}{COLOR_RESET}"
    );
    let info_line = format!(
        "{TC_GRAY}  v{version}{COLOR_RESET} {TC_GRAY_DIM}\u{00b7}{COLOR_RESET} \
         {TC_WHITE}{provider}{COLOR_RESET} {TC_GRAY_DIM}\u{00b7}{COLOR_RESET} \
         {TC_CYAN}{model}{COLOR_RESET} {TC_GREEN}{COLOR_BOLD}\u{25cf} Ready{COLOR_RESET}"
    );
    format!("{banner}\n{info_line}")
}

/// Copyright notice for terminal output.
pub fn copyright_notice(version: &str) -> String {
    let year = 2026;
    format!(
        "{TC_CYAN}AlphaCode{COLOR_RESET} {TC_GRAY_DIM}v{version}{COLOR_RESET}  \
         {TC_GRAY_DIM}\u{00a9} {year} AlphaCode. All rights reserved.{COLOR_RESET}",
        version = version,
        year = year,
    )
}

/// Aligned header labels -- no visual clutter.
pub fn aligned_header(label: &str, value: &str) -> String {
    let pad = 10_usize.saturating_sub(label.len());
    format!(
        "{TC_GRAY}{label}{label_padding}{COLOR_RESET} {TC_WHITE}{value}{COLOR_RESET}",
        label_padding = " ".repeat(pad)
    )
}

/// Clean prompt formatting.
pub fn prompt_header(title: &str) -> String {
    format!("{COLOR_BOLD}{TC_CYAN}{title}{COLOR_RESET}\n")
}
pub fn prompt_field(label: &str, value: &str) -> String {
    format!("{TC_GRAY}{label}{COLOR_RESET}\n{TC_WHITE}{value}{COLOR_RESET}\n")
}
pub fn prompt_subtle(text: &str) -> String {
    format!("{TC_GRAY_DIM}{text}{COLOR_RESET}")
}
pub fn prompt_action(text: &str) -> String {
    format!("{TC_YELLOW}{text}{COLOR_RESET}")
}

/// Concise progress messages.
pub fn progress(text: &str) -> String {
    format!("{TC_BLUE}{text}{COLOR_RESET}")
}

/// Formatted progress with a spinner-style indicator.
pub fn progress_spinner(text: &str) -> String {
    format!("{TC_PURPLE}{COLOR_BOLD}\u{27f3}{COLOR_RESET} {TC_BLUE}{text}{COLOR_RESET}")
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
    format!(
        "{TC_RED}{COLOR_BOLD} \u{2717} {error}{COLOR_RESET}",
        error = error_reason(reason)
    )
}
pub fn error_action(reason: &str) -> String {
    let reason = reason.lines().next().unwrap_or(reason);
    format!("{TC_YELLOW}{reason}{COLOR_RESET}")
}

/// Collapse multiple success messages.
pub fn success_message(text: &str) -> String {
    format!("{TC_GREEN}{text}{COLOR_RESET}")
}

/// Multi-line info block with a colored left border.
pub fn info_block(title: &str, body: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "{TC_CYAN}{COLOR_BOLD}  \u{2503} {title}{COLOR_RESET}\n"
    ));
    for line in body.lines() {
        out.push_str(&format!(
            "{TC_CYAN}  \u{2503}{COLOR_RESET} {TC_GRAY}{line}{COLOR_RESET}\n"
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
