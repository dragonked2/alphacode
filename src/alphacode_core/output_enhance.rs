//! `output_enhance` — UI/UX upgrade for terminal-facing output.
//!
//! This module sits on top of `output_style` and adds:
//!
//!   1. **Brand color tokens** — a small palette of named colors that
//!      every UI surface uses, so a rebrand propagates to everything
//!      at once.
//!
//!   2. **Box drawing** — UTF-8 box frames that auto-fallback to
//!      ASCII when the terminal can't render them.
//!
//!   3. **Status bar helpers** — short, scoped status-line renderers
//!      with consistent prefixes (`OK`, `WARN`, `ERR`, `INFO`).
//!
//!   4. **Banner helpers** — section headers for slash commands and
//!      long-running operations.
//!
//!   5. **Progress indicators** — spinners and counters that work
//!      with both emoji and no-emoji modes.
//!
//!   6. **Sub-line dividers** — clean visual separators between
//!      command output and assistant output.
//!
//! All helpers respect `output_style::emoji_enabled()` and degrade
//! gracefully to ASCII when needed. They never panic, never block,
//! and never print to a TTY without first detecting capability.

use crate::alphacode_core::console;
use crate::output_style::{emoji_enabled, terminal_text};

/// Brand color tokens. The values are 24-bit RGB. They are tuned
/// for a dark default terminal and a light default terminal — the
/// `pick_for_terminal_bg()` helper picks the right one.
pub mod palette {
    /// Alphacode primary blue (used for headings, prompts, links).
    pub const ACCENT: (u8, u8, u8) = (120, 180, 255);
    /// Alphacode success green.
    pub const SUCCESS: (u8, u8, u8) = (90, 200, 130);
    /// Alphacode warn amber.
    pub const WARN: (u8, u8, u8) = (240, 180, 80);
    /// Alphacode error red.
    pub const ERROR: (u8, u8, u8) = (240, 100, 100);
    /// Alphacode info cyan.
    pub const INFO: (u8, u8, u8) = (110, 200, 220);
    /// Alphacode neutral gray (used for hints, secondary text).
    pub const MUTED: (u8, u8, u8) = (140, 150, 165);
    /// Alphacode highlight magenta (for selected items, active markers).
    pub const HIGHLIGHT: (u8, u8, u8) = (220, 130, 220);

    /// Light-bg variants (slightly darker for contrast on white).
    pub const ACCENT_LIGHT: (u8, u8, u8) = (30, 100, 200);
    pub const SUCCESS_LIGHT: (u8, u8, u8) = (30, 140, 60);
    pub const WARN_LIGHT: (u8, u8, u8) = (180, 120, 0);
    pub const ERROR_LIGHT: (u8, u8, u8) = (200, 50, 50);
    pub const INFO_LIGHT: (u8, u8, u8) = (30, 130, 160);
    pub const MUTED_LIGHT: (u8, u8, u8) = (90, 100, 115);
    pub const HIGHLIGHT_LIGHT: (u8, u8, u8) = (160, 60, 160);
}

/// Detect whether the terminal background is light. We use
/// `COLORFGBG` (a Linux convention) and fall back to dark. The
/// result is cached after the first call.
///
/// The previous Windows heuristic ("no `WT_SESSION` ⇒ light") reported *light*
/// for PowerShell and Windows conhost, both of which default to a dark
/// profile — so every status line picked the light-background palette and came
/// out washed out. Modern Windows terminals are dark by default and there is
/// no reliable way to query conhost for the profile's background without
/// reading registry settings, so the only defensible default is dark.
pub fn is_light_terminal() -> bool {
    use std::sync::OnceLock;
    static CACHE: OnceLock<bool> = OnceLock::new();
    *CACHE.get_or_init(|| {
        // Linux/WSL convention: COLORFGBG is "fg;bg", bg is 0-15.
        if let Ok(s) = std::env::var("COLORFGBG")
            && let Some(bg) = s.split(';').next_back().and_then(|s| s.parse::<u16>().ok())
        {
            // Standard ANSI palette: 0..7 are dark, 8..15 are light.
            return bg >= 8;
        }
        // An explicit user preference always wins over any heuristic.
        if let Ok(v) = std::env::var("ALPHACODE_LIGHT_BG")
            && let Some(bg) = parse_bool(&v)
        {
            return bg;
        }
        // Windows Terminal, conhost, and PowerShell all default to dark.
        false
    })
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "light" => Some(true),
        "0" | "false" | "no" | "off" | "dark" => Some(false),
        _ => None,
    }
}

/// Pick a color from the palette appropriate for the terminal bg.
pub fn pick_for_terminal_bg(token: PaletteToken) -> (u8, u8, u8) {
    if is_light_terminal() {
        match token {
            PaletteToken::Accent => palette::ACCENT_LIGHT,
            PaletteToken::Success => palette::SUCCESS_LIGHT,
            PaletteToken::Warn => palette::WARN_LIGHT,
            PaletteToken::Error => palette::ERROR_LIGHT,
            PaletteToken::Info => palette::INFO_LIGHT,
            PaletteToken::Muted => palette::MUTED_LIGHT,
            PaletteToken::Highlight => palette::HIGHLIGHT_LIGHT,
        }
    } else {
        match token {
            PaletteToken::Accent => palette::ACCENT,
            PaletteToken::Success => palette::SUCCESS,
            PaletteToken::Warn => palette::WARN,
            PaletteToken::Error => palette::ERROR,
            PaletteToken::Info => palette::INFO,
            PaletteToken::Muted => palette::MUTED,
            PaletteToken::Highlight => palette::HIGHLIGHT,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteToken {
    Accent,
    Success,
    Warn,
    Error,
    Info,
    Muted,
    Highlight,
}

/// Render a 24-bit ANSI color escape for `(r, g, b)`.
///
/// Returns an **empty string** when the terminal cannot accept escapes
/// (`NO_COLOR`, `TERM=dumb`, quiet mode, a pipe, a log file, a CI job's
/// captured stderr). Callers can therefore concatenate the result
/// unconditionally — that is the only way to keep the escape sequence and the
/// text it styles in sync across every branch.
///
/// The returned value never carries a trailing reset; pair it with
/// [`reset`] so styled text cannot bleed into the next line.
pub fn rgb_escape(r: u8, g: u8, b: u8, fg: bool) -> String {
    if !console::ansi_enabled() {
        return String::new();
    }
    if fg {
        format!("\x1b[38;2;{};{};{}m", r, g, b)
    } else {
        format!("\x1b[48;2;{};{};{}m", r, g, b)
    }
}

/// The SGR sequence that clears all attributes, or `""` when styling is off.
///
/// A non-empty value emitted after text that was *not* styled (because styling
/// was off) would leave the terminal in a reset state for no reason, which is
/// harmless but wasteful — and, more importantly, would be the only escape in
/// an otherwise escape-free stream, which is exactly what breaks parsers.
pub fn reset() -> &'static str {
    if console::ansi_enabled() { RESET } else { "" }
}

/// Reset all ANSI attributes.
///
/// Prefer [`reset`]: this constant is unconditional and will emit an escape
/// even into a pipe.
pub const RESET: &str = "\x1b[0m";

/// Box-drawing characters with an ASCII fallback.
///
/// Capability comes from `crate::alphacode_core::console::unicode_supported`,
/// which considers the locale, the Windows console code page, an explicit
/// `ALPHACODE_ASCII` opt-out, and `NO_COLOR` (whose users are frequently in
/// log/screen-reader contexts where Unicode chrome is as unwelcome as color).
///
/// The previous heuristic ended with `|| cfg!(windows)`, which meant a
/// Windows console on a legacy code page rendered `─` as mojibake, while a
/// Cygwin/MSYS shell with a perfectly good UTF-8 locale got `+---+` because
/// `LANG` was unset. Both directions are now driven by evidence.
pub mod box_chars {
    use crate::alphacode_core::console;

    pub struct Chars {
        pub h: char,
        pub v: char,
        pub tl: char,
        pub tr: char,
        pub bl: char,
        pub br: char,
        pub t_down: char,
        pub t_up: char,
        pub t_left: char,
        pub t_right: char,
        pub h_thick: char,
        pub v_thick: char,
        pub block_full: char,
        pub block_light: char,
    }

    const UNICODE: Chars = Chars {
        h: '─',
        v: '│',
        tl: '┌',
        tr: '┐',
        bl: '└',
        br: '┘',
        t_down: '┬',
        t_up: '┴',
        t_left: '┤',
        t_right: '├',
        h_thick: '━',
        v_thick: '┃',
        block_full: '█',
        block_light: '░',
    };

    const ASCII: Chars = Chars {
        h: '-',
        v: '|',
        tl: '+',
        tr: '+',
        bl: '+',
        br: '+',
        t_down: '+',
        t_up: '+',
        t_left: '+',
        t_right: '+',
        h_thick: '=',
        v_thick: '#',
        block_full: '#',
        block_light: '.',
    };

    pub fn pick() -> &'static Chars {
        use std::sync::OnceLock;
        static CACHE: OnceLock<&'static Chars> = OnceLock::new();
        CACHE.get_or_init(|| pick_for(console::unicode_supported()))
    }

    /// Resolve the glyph set for an explicit capability answer.
    ///
    /// Split out from [`pick`] so the two tables can be tested directly:
    /// `pick` memoizes, so toggling the console override after the first call
    /// would otherwise silently have no effect on the test.
    pub fn pick_for(unicode: bool) -> &'static Chars {
        if unicode { &UNICODE } else { &ASCII }
    }
}

/// Display width of `text` in terminal cells.
///
/// This is the single measurement used by every column layout below. It is
/// never `text.len()` (bytes) and never `text.chars().count()` (code points):
/// both are wrong for any non-ASCII content, which in a CLI means every path,
/// commit message, model id, and error string a user pastes in.
pub fn display_width(text: &str) -> usize {
    unicode_width::UnicodeWidthStr::width(text)
}

/// Pad `text` on the right until it occupies exactly `width` cells.
///
/// Text already at or over the target width is returned untouched. Padding is
/// computed from display width, not byte or char count, so a column of CJK
/// labels lines up exactly the same as a column of ASCII ones — the old
/// `format!("{:<w$}")` misaligned every such row by one cell per wide glyph.
pub fn pad_end_to(text: &str, width: usize) -> String {
    let used = display_width(text);
    if used >= width {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + (width - used));
    out.push_str(text);
    out.extend(std::iter::repeat_n(' ', width - used));
    out
}

/// Pad `text` on the left until it occupies exactly `width` cells.
///
/// Use for numeric columns. Same display-width contract as [`pad_end_to`].
pub fn pad_start_to(text: &str, width: usize) -> String {
    let used = display_width(text);
    if used >= width {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + (width - used));
    out.extend(std::iter::repeat_n(' ', width - used));
    out.push_str(text);
    out
}

/// Truncate `text` to `max_width` display cells, appending `…` when it does
/// not fit.
///
/// The ellipsis is counted against the budget, so the result is *never* wider
/// than `max_width` — the previous `truncate_to_width(cell, w - 1) + "…"`
/// produced `w + 1` cells for every overflowing cell and pushed the rest of
/// the row out of alignment. `max_width == 0` yields `""` rather than a bare
/// ellipsis.
pub fn truncate_ellipsis(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }
    if display_width(text) <= max_width {
        return text.to_string();
    }
    // Reserve one cell for the ellipsis itself.
    let mut out = truncate_to_width(text, max_width - 1);
    out.push('…');
    out
}

/// Render a single-line box around a label. Used for slash-command
/// banners and section dividers.
///
/// Example: `box_top("security-audit · 12 findings", 48)`
/// ```text
/// ───── security-audit · 12 findings ─────────────────
/// ```
///
/// The rule is exactly `width` cells wide whenever the label leaves room for
/// at least the two one-cell gutters; below that it degrades to the label
/// alone rather than overflowing the terminal. The old calculation reserved
/// `label + 4` but only ever emitted `label + 2` cells of decoration, so the
/// divider came out two cells short of what callers were budgeting for and
/// adjacent rules lined up off-centre.
pub fn box_top(label: &str, width: usize) -> String {
    let bc = box_chars::pick();
    let label = terminal_text(label);
    let label_width = display_width(&label);
    // Two one-cell gutters around the label.
    let decoration = width.saturating_sub(label_width + 2);
    if decoration == 0 {
        return label.into_owned();
    }
    let left = decoration / 2;
    let right = decoration - left;
    let mut out = String::with_capacity(width);
    for _ in 0..left {
        out.push(bc.h);
    }
    out.push(' ');
    out.push_str(&label);
    out.push(' ');
    for _ in 0..right {
        out.push(bc.h);
    }
    out
}

/// Render a horizontal divider line. Optionally with a centered label.
pub fn divider(label: Option<&str>, width: usize) -> String {
    match label {
        None => {
            let bc = box_chars::pick();
            std::iter::repeat_n(bc.h, width).collect()
        }
        Some(text) => box_top(text, width),
    }
}

/// Status-line prefixes. The first column is the prefix glyph; the
/// second column is the matching palette token. Both emoji and
/// no-emoji variants are provided.
#[derive(Debug, Clone, Copy)]
pub struct StatusPrefix {
    pub emoji: &'static str,
    pub ascii: &'static str,
    pub token: PaletteToken,
}

pub const PREFIX_OK: StatusPrefix = StatusPrefix {
    emoji: "✓",
    ascii: "[OK]",
    token: PaletteToken::Success,
};
pub const PREFIX_WARN: StatusPrefix = StatusPrefix {
    emoji: "⚠",
    ascii: "[WARN]",
    token: PaletteToken::Warn,
};
pub const PREFIX_ERROR: StatusPrefix = StatusPrefix {
    emoji: "✗",
    ascii: "[ERR]",
    token: PaletteToken::Error,
};
pub const PREFIX_INFO: StatusPrefix = StatusPrefix {
    emoji: "ℹ",
    ascii: "[INFO]",
    token: PaletteToken::Info,
};
pub const PREFIX_DEBUG: StatusPrefix = StatusPrefix {
    emoji: "·",
    ascii: "[DBG]",
    token: PaletteToken::Muted,
};

/// Two-cell indent shared by every status and block renderer.
pub const STATUS_INDENT: usize = 2;
/// Width reserved for the leading status glyph column.
pub const STATUS_GLYPH_WIDTH: usize = 6;

/// Render a status line: `  <glyph>  <message>`.
///
/// The glyph column is padded so consecutive status lines align their
/// messages, whether they render `✓` (1 cell) or `[WARN]` (6 cells). The
/// two-cell indent matches the rest of the CLI's structured output, so a
/// block of status lines reads as one aligned column rather than a ragged mix.
pub fn status_line(prefix: StatusPrefix, message: &str) -> String {
    let (r, g, b) = pick_for_terminal_bg(prefix.token);
    let color = rgb_escape(r, g, b, true);
    let glyph = pad_end_to(&status_glyph(prefix), STATUS_GLYPH_WIDTH);
    format!(
        "{}{}{}{} {}",
        " ".repeat(STATUS_INDENT),
        color,
        glyph,
        reset(),
        terminal_text(message),
    )
}

/// The glyph a status prefix renders in the current emoji/Unicode mode.
pub fn status_glyph(prefix: StatusPrefix) -> String {
    let glyph = if emoji_enabled() {
        prefix.emoji
    } else {
        prefix.ascii
    };
    if console::unicode_supported() {
        glyph.to_string()
    } else {
        ascii_status_fallback(glyph).to_string()
    }
}

/// Strip a status prefix down to its ASCII skeleton for terminals that cannot
/// render checkmark/warning glyphs. `[OK]` / `[WARN]` already *are* ASCII;
/// everything else degrades to the bracketed tag.
fn ascii_status_fallback(glyph: &str) -> &str {
    if glyph.is_ascii() {
        glyph
    } else {
        match glyph {
            "\u{2713}" | "\u{2714}" | "\u{2705}" => "[OK]",
            "\u{26a0}" | "\u{1f6a8}" => "[WARN]",
            "\u{2717}" | "\u{2715}" | "\u{274c}" => "[ERR]",
            "\u{2139}" => "[INFO]",
            _ => "[--]",
        }
    }
}

/// Render an error line with a leading explanation.
///
/// Structured as *what happened* → *why* → *how to fix*, which is the order a
/// reader needs them in. The `why` body is indented to the status message
/// column and wrapped to the terminal, so a provider error containing a long
/// URL reads as one aligned block instead of an arbitrarily soft-wrapped line
/// with a random hanging indent.
pub fn error_block(what: &str, why: &str, fix: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str(&status_line(PREFIX_ERROR, what));
    out.push('\n');
    out.push_str(&wrap_indented(why, STATUS_INDENT + STATUS_GLYPH_WIDTH + 1));
    if let Some(fix) = fix {
        out.push('\n');
        out.push_str(&status_line(PREFIX_INFO, &format!("fix: {}", fix)));
    }
    out
}

/// Render a banner for a slash command output. Two lines, the
/// title and a subtitle. The title uses Accent color; the subtitle
/// uses Muted.
pub fn command_banner(title: &str, subtitle: Option<&str>, width: usize) -> String {
    let (r, g, b) = pick_for_terminal_bg(PaletteToken::Accent);
    let (mr, mg, mb) = pick_for_terminal_bg(PaletteToken::Muted);
    let rule = divider(None, console::fit_width(width));
    let mut out = String::new();
    out.push_str(&rule);
    out.push('\n');
    out.push_str(&format!(
        "{}{}{}\n",
        rgb_escape(r, g, b, true),
        terminal_text(title),
        reset(),
    ));
    if let Some(sub) = subtitle {
        out.push_str(&format!(
            "{}{}{}\n",
            rgb_escape(mr, mg, mb, true),
            terminal_text(sub),
            reset(),
        ));
    }
    out.push_str(&rule);
    out
}

/// Column width reserved for the key/shortcut in [`keyline`].
pub const KEYLINE_KEY_WIDTH: usize = 16;

/// Render a "key" line for slash commands. Used in help output to
/// show the shortcut next to the action.
///
/// Padding is display-width based, so a CJK key name lines up with the
/// descriptions exactly the way an ASCII one does.
pub fn keyline(key: &str, action: &str) -> String {
    let (r, g, b) = pick_for_terminal_bg(PaletteToken::Accent);
    let (mr, mg, mb) = pick_for_terminal_bg(PaletteToken::Muted);
    let key_cell = truncate_ellipsis(&terminal_text(key), KEYLINE_KEY_WIDTH);
    format!(
        "  {}{}{}  {}{}{}",
        rgb_escape(r, g, b, true),
        pad_end_to(&key_cell, KEYLINE_KEY_WIDTH),
        reset(),
        rgb_escape(mr, mg, mb, true),
        terminal_text(action),
        reset(),
    )
}

/// Render a table row. `widths` is the column widths; `cells` is
/// the cell text. If a cell overflows, it is truncated with an
/// ellipsis. Columns are left-aligned by default; set the matching
/// entry in `align_right` for numeric columns.
///
/// Cells are measured in display cells, so a column mixing ASCII, CJK, and
/// emoji stays rectangular. Every cell — truncated or not — occupies exactly
/// its declared width plus the two-cell gutter, which is what keeps the rows
/// below it aligned.
pub fn table_row(widths: &[usize], cells: &[&str], align_right: &[bool]) -> String {
    let mut out = String::new();
    for (i, cell) in cells.iter().enumerate() {
        let w = widths.get(i).copied().unwrap_or(20);
        let cell = truncate_ellipsis(&terminal_text(cell), w);
        if align_right.get(i).copied().unwrap_or(false) {
            out.push_str(&pad_start_to(&cell, w));
        } else {
            out.push_str(&pad_end_to(&cell, w));
        }
        out.push_str("  ");
    }
    out
}

/// Wrap `text` to the terminal width, indenting every line by `indent` cells.
///
/// Long provider error messages and tool stderr currently emit as one very
/// long line that the terminal soft-wraps at an arbitrary column, shredding
/// any indentation the caller applied. Wrapping here keeps the block
/// rectangular and the reading order intact.
pub fn wrap_indented(text: &str, indent: usize) -> String {
    let width = console::terminal_width().saturating_sub(indent).max(8);
    let pad = " ".repeat(indent);
    let mut out = String::new();
    for (i, line) in terminal_text(text).lines().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&pad);
        out.push_str(&wrap_line(line, width));
    }
    out
}

/// Word-wrap a single (newline-free) line to `width` display cells.
///
/// Breaks on whitespace where possible and mid-word only when a single word
/// is wider than the line, so long ids and URLs still get wrapped rather than
/// truncated.
pub fn wrap_line(line: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    if display_width(line) <= width {
        return line.to_string();
    }

    let mut out = String::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for word in split_keep_spaces(line) {
        let word_width = display_width(&word);
        if current_width > 0 && current_width + word_width > width {
            out.push_str(current.trim_end());
            out.push('\n');
            current.clear();
            current_width = 0;
        }
        // A word longer than the whole line: hard-break it by cells.
        if word_width > width {
            for chunk in break_to_width(&word, width) {
                out.push_str(&chunk);
                out.push('\n');
            }
            continue;
        }
        current.push_str(&word);
        current_width += word_width;
    }

    let tail = current.trim_end();
    if !tail.is_empty() {
        out.push_str(tail);
    }
    out
}

fn split_keep_spaces(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    for ch in line.chars() {
        current.push(ch);
        if ch.is_whitespace() {
            words.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn break_to_width(text: &str, width: usize) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut offset = 0usize;
    while display_width(&text[offset..]) > width {
        let head = truncate_to_width(&text[offset..], width);
        if head.is_empty() {
            break;
        }
        chunks.push(head.clone());
        offset += head.len();
    }
    let tail = &text[offset..];
    if !tail.is_empty() {
        chunks.push(tail.to_string());
    }
    chunks
}

/// Truncate a string to a given display width, breaking on grapheme
/// boundaries. The result has display width <= `max_width`.
pub fn truncate_to_width(s: &str, max_width: usize) -> String {
    use unicode_segmentation::UnicodeSegmentation;
    let mut out = String::new();
    let mut used = 0usize;
    for g in s.graphemes(true) {
        let w = unicode_width::UnicodeWidthStr::width(g);
        if used + w > max_width {
            break;
        }
        out.push_str(g);
        used += w;
    }
    out
}

/// Format a byte count with the appropriate unit (KB/MB/GB).
pub fn format_bytes(n: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{} {}", n, UNITS[0])
    } else {
        format!("{:.1} {}", v, UNITS[u])
    }
}

/// Format a duration in human-friendly form.
///
/// The previous implementation rounded each component independently, so
/// `59.6` rendered as `60.0s` and `3699.7` as `1h 60m` — values that read as
/// impossible and make a user distrust the number. Components are now rounded
/// with carry, so every result is always a real elapsed time.
pub fn format_duration(secs: f64) -> String {
    let total_ms = (secs.max(0.0) * 1000.0).round() as u64;
    if total_ms < 1_000 {
        return format!("{total_ms}ms");
    }
    let total_secs = (total_ms + 500) / 1_000;
    if total_secs < 60 {
        // One decimal below 10s keeps sub-second progress legible; above that
        // the decimal is noise in a status line.
        return if total_secs < 10 {
            format!("{:.1}s", total_ms as f64 / 1000.0)
        } else {
            format!("{total_secs}s")
        };
    }
    let total_mins = (total_secs + 30) / 60;
    if total_mins < 60 {
        let m = total_mins;
        let s = total_secs - m * 60;
        return if s == 0 {
            format!("{m}m")
        } else {
            format!("{m}m {s}s")
        };
    }
    let h = total_mins / 60;
    let m = total_mins - h * 60;
    if m == 0 {
        format!("{h}h")
    } else {
        format!("{h}h {m}m")
    }
}

/// Format a number with thousands separators.
pub fn format_count(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*c as char);
    }
    out
}

/// Format a token count the way the CLI and TUI status lines show it:
/// compact above 1,000 (`1.2k`, `3.4M`), exact below.
///
/// Progress indicators jump by thousands of tokens per tick; rendering
/// `1,204` in a line that refreshes several times a second makes the number
/// jitter and impossible to compare at a glance.
pub fn format_tokens(n: u64) -> String {
    const THOUSAND: f64 = 1_000.0;
    const MILLION: f64 = 1_000_000.0;
    const BILLION: f64 = 1_000_000_000.0;

    if n < 1_000 {
        return n.to_string();
    }
    if n < 10_000 {
        return format!("{:.1}k", n as f64 / THOUSAND);
    }
    if n < 1_000_000 {
        return format!("{}k", n / 1_000);
    }
    if n < 10_000_000 {
        return format!("{:.1}M", n as f64 / MILLION);
    }
    if n < BILLION as u64 {
        return format!("{}M", n / 1_000_000);
    }
    format!("{:.1}B", n as f64 / BILLION)
}

/// Spinner frames, both emoji and ASCII. The caller rotates them.
pub const SPINNER_EMOJI: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
pub const SPINNER_ASCII: &[&str] = &["-", "\\", "|", "/"];

/// Return the current spinner frame for the given tick.
///
/// Braille spinners are only usable when the terminal can render them, which
/// is a *capability* question, not an emoji-preference question: a user who
/// turns emoji off in config still wants a smooth spinner, and a user piping
/// into a log wants ASCII even with emoji enabled. Both signals are checked.
pub fn spinner_frame(tick: usize) -> &'static str {
    let frames = if emoji_enabled() && console::unicode_supported() {
        SPINNER_EMOJI
    } else {
        SPINNER_ASCII
    };
    frames[tick % frames.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_bytes_basic() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(2048), "2.0 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn format_duration_basic() {
        assert_eq!(format_duration(0.5), "500ms");
        assert_eq!(format_duration(5.5), "5.5s");
        assert_eq!(format_duration(125.0), "2m 5s");
        assert_eq!(format_duration(3700.0), "1h 2m");
    }

    #[test]
    fn format_duration_never_renders_an_impossible_component() {
        // The old formatter produced these: `59.6` -> "60.0s" and
        // `3699.7` -> "1h 60m". A status line that reports a duration nobody
        // can have lived through erodes trust in every other number it shows.
        assert_eq!(format_duration(59.6), "1m");
        assert_eq!(format_duration(3599.7), "1h");
        assert_eq!(format_duration(3699.7), "1h 2m");
        assert_eq!(format_duration(59.4), "59s");
        assert_eq!(format_duration(119.7), "2m");
        assert_eq!(format_duration(3600.0), "1h");
        // Negative / NaN input must not panic or render nonsense.
        assert_eq!(format_duration(-5.0), "0ms");
    }

    #[test]
    fn format_count_basic() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1000), "1,000");
        assert_eq!(format_count(1234567), "1,234,567");
    }

    #[test]
    fn format_tokens_is_compact_where_it_matters() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1_200), "1.2k");
        assert_eq!(format_tokens(48_000), "48k");
        assert_eq!(format_tokens(3_400_000), "3.4M");
        assert_eq!(format_tokens(42_000_000), "42M");
    }

    #[test]
    fn truncate_to_width_works() {
        let s = "hello world";
        let t = truncate_to_width(s, 7);
        assert!(unicode_width::UnicodeWidthStr::width(t.as_str()) <= 7);
    }

    /// A double-width fixture, written as escapes so this source file stays
    /// plain ASCII.
    ///
    /// Every ASCII character is at once 1 byte, 1 `char`, and 1 terminal cell.
    /// An ASCII-only fixture therefore cannot tell a correct width calculation
    /// apart from the three broken ones — byte count, char count, and cell
    /// count all agree on ASCII. Full-width glyphs are what make the difference
    /// observable, so the width tests below need at least one.
    ///
    /// U+65E5, U+672C, U+5F8C render two cells each.
    const WIDE: &str = "\u{65e5}\u{672c}\u{5f8c}\u{30d7}";

    #[test]
    fn truncate_to_width_counts_cells_not_chars() {
        // 4 glyphs = 8 cells. Truncating to 6 cells must keep exactly 3
        // glyphs, not 6.
        assert_eq!(display_width(WIDE), 8);
        let t = truncate_to_width(WIDE, 6);
        assert_eq!(display_width(&t), 6);
    }

    #[test]
    fn truncate_ellipsis_never_exceeds_the_budget() {
        // The ellipsis is reserved from the budget rather than appended after
        // it, so an overflowing cell can never widen its column.
        for width in 1..=24 {
            let long = "anthropic/claude-opus-4-5-20251101-preview";
            let out = truncate_ellipsis(long, width);
            assert!(
                display_width(&out) <= width,
                "width {width}: {out:?} is {} cells",
                display_width(&out),
            );
        }
        for width in 1..=9 {
            let out = truncate_ellipsis(WIDE, width);
            assert!(
                display_width(&out) <= width,
                "width {width}: {out:?} is {} cells",
                display_width(&out),
            );
        }
        assert_eq!(truncate_ellipsis("short", 10), "short");
        assert_eq!(truncate_ellipsis("anything", 0), "");
    }

    #[test]
    fn padding_measures_display_cells_not_bytes_or_chars() {
        // `format!("{:<8}", WIDE)` pads by char count: 4 glyphs is already 4,
        // so it emits a 8-char string that renders 8+ cells. These are the
        // actual column-alignment primitives, so they must be exact.
        assert_eq!(display_width(&pad_end_to(WIDE, 12)), 12);
        assert_eq!(display_width(&pad_start_to(WIDE, 12)), 12);
        assert_eq!(display_width(&pad_end_to("ab", 8)), 8);
        // Over-long input is returned untouched rather than truncated.
        assert_eq!(pad_end_to("abcdef", 3), "abcdef");
        assert_eq!(pad_start_to("abcdef", 3), "abcdef");
    }

    #[test]
    fn table_row_stays_rectangular_across_scripts() {
        let widths = [10, 8];
        let rows: Vec<Vec<&str>> = vec![
            vec!["anthropic", "claude-opus-4-5"],
            vec![WIDE, "gpt-5.1"],
            vec!["q", "a-much-longer-value-than-fits"],
        ];
        let mut rendered = Vec::new();
        for row in &rows {
            let line = strip_ansi(&table_row(&widths, row, &[false, false]));
            rendered.push(display_width(&line));
        }
        // Every row occupies the same number of cells: sum(widths) + 2 per
        // column of gutter. This is what `{:<width$}` got wrong.
        let expected = widths.iter().sum::<usize>() + widths.len() * 2;
        for (line_width, row) in rendered.iter().zip(&rows) {
            assert_eq!(*line_width, expected, "row {row:?} was not rectangular");
        }
    }

    #[test]
    fn right_aligned_columns_pad_on_the_left_by_display_width() {
        let widths = [8, 6];
        let line = strip_ansi(&table_row(&widths, &["tok", "1.2k"], &[false, true]));
        assert_eq!(display_width(&line), 16);
        assert!(line.ends_with("    1.2k"), "got {line:?}");
    }

    #[test]
    fn box_top_fills_the_requested_width() {
        let _guard = console::test_override_lock();
        console::set_unicode_override(Some(true));
        for width in [20usize, 40, 61] {
            let line = box_top("audit", width);
            assert_eq!(
                display_width(&line),
                width,
                "box_top(\"audit\", {width}) rendered {} cells",
                display_width(&line)
            );
        }
        // A label too wide for the requested rule degrades to the label
        // itself rather than overflowing the terminal.
        let cramped = box_top("an-extremely-long-label", 10);
        assert_eq!(
            display_width(&cramped),
            display_width("an-extremely-long-label")
        );
        console::set_unicode_override(None);
    }

    #[test]
    fn status_line_keeps_no_escapes_when_ansi_is_off() {
        let _guard = console::test_override_lock();
        // The contract that makes CLI output pipeable: with styling disabled,
        // no renderer may emit a single ESC byte.
        console::set_ansi_enabled_override(Some(false));
        let samples = [
            status_line(PREFIX_OK, "done"),
            status_line(PREFIX_ERROR, "failed"),
            error_block("what", "why", Some("fix")),
            command_banner("Title", Some("Subtitle"), 40),
            keyline("/help", "Show commands"),
            table_row(&[6, 6], &["a", "b"], &[false, false]),
        ];
        for sample in samples {
            assert!(
                !sample.contains('\u{1b}'),
                "escape leaked into a non-styled render: {sample:?}"
            );
        }
        console::set_ansi_enabled_override(None);
    }

    #[test]
    fn styled_renders_pair_every_escape_with_a_reset() {
        let _guard = console::test_override_lock();
        console::set_ansi_enabled_override(Some(true));
        let samples = [
            status_line(PREFIX_WARN, "careful"),
            command_banner("Title", Some("Subtitle"), 40),
            keyline("/help", "Show commands"),
            table_row(&[6, 6], &["a", "b"], &[false, false]),
        ];
        for sample in samples {
            let escapes = sample.matches('\u{1b}').count();
            let resets = sample.matches("\u{1b}[0m").count();
            assert_eq!(
                escapes,
                resets * 2,
                "every SGR needs a matching reset, got {escapes} escapes / {resets} resets in {sample:?}"
            );
        }
        console::set_ansi_enabled_override(None);
    }

    #[test]
    fn wrap_line_never_exceeds_the_requested_width() {
        let text = "the provider rejected the request because the api key expired on 2026-01-02";
        for width in [12usize, 20, 40, 79] {
            for line in wrap_line(text, width).lines() {
                assert!(
                    display_width(line) <= width,
                    "width {width}: {line:?} is {} cells",
                    display_width(line),
                );
            }
        }
        // A single unbreakable token is hard-broken rather than truncated.
        let long = "a".repeat(50);
        for line in wrap_line(&long, 10).lines() {
            assert_eq!(display_width(line), 10);
        }
    }

    #[test]
    fn wrap_line_preserves_words_when_it_can() {
        assert_eq!(wrap_line("hello world", 40), "hello world");
        // Already-short input is returned byte-for-byte, including whitespace:
        // wrapping must never silently trim content it was not asked to trim.
        assert_eq!(wrap_line("  ", 40), "  ");
        assert_eq!(wrap_line("", 40), "");
    }

    #[test]
    fn status_line_works() {
        let s = status_line(PREFIX_OK, "12 findings");
        assert!(s.contains("12 findings"));
    }

    #[test]
    fn status_glyph_degrades_to_ascii_without_unicode() {
        let _guard = console::test_override_lock();
        // Pin emoji *and* unicode so the test does not depend on the host's
        // configured emoji preference.
        let previous = crate::output_style::emoji_enabled();
        crate::output_style::set_emoji_enabled(true);

        console::set_unicode_override(Some(false));
        for prefix in [
            PREFIX_OK,
            PREFIX_WARN,
            PREFIX_ERROR,
            PREFIX_INFO,
            PREFIX_DEBUG,
        ] {
            assert!(
                status_glyph(prefix).is_ascii(),
                "{prefix:?} did not degrade to ASCII"
            );
        }

        console::set_unicode_override(Some(true));
        assert_eq!(status_glyph(PREFIX_OK), "✓");
        assert_eq!(status_glyph(PREFIX_ERROR), "✗");

        // Emoji disabled but Unicode fine: the bracketed ASCII tag is used,
        // which is already ASCII and must pass through untouched.
        crate::output_style::set_emoji_enabled(false);
        assert_eq!(status_glyph(PREFIX_WARN), "[WARN]");

        crate::output_style::set_emoji_enabled(previous);
        console::set_unicode_override(None);
    }

    #[test]
    fn spinner_cycles() {
        let a = spinner_frame(0);
        let b = spinner_frame(100);
        // ASCII mode falls back since the test env may or may not have emoji enabled.
        assert!(!a.is_empty() && !b.is_empty());
    }

    #[test]
    fn spinner_falls_back_without_unicode_support() {
        let _guard = console::test_override_lock();
        console::set_unicode_override(Some(false));
        assert!(spinner_frame(0).is_ascii());
        console::set_unicode_override(None);
    }

    #[test]
    fn box_chars_pick_returns_something() {
        let bc = box_chars::pick();
        assert!(!bc.h.is_whitespace());
    }

    #[test]
    fn box_chars_never_mix_systems() {
        // A box drawn with one set of corner glyphs and another set of
        // horizontals reads as corruption, so the two tables must stay
        // internally consistent for whichever set is selected. Tested through
        // `pick_for` because `pick` memoizes its answer.
        for (unicode, expect_unicode) in [(true, true), (false, false)] {
            let bc = box_chars::pick_for(unicode);
            let all = [
                bc.h,
                bc.v,
                bc.tl,
                bc.tr,
                bc.bl,
                bc.br,
                bc.t_down,
                bc.t_up,
                bc.t_left,
                bc.t_right,
                bc.h_thick,
                bc.v_thick,
                bc.block_full,
                bc.block_light,
            ];
            for glyph in all {
                assert_eq!(
                    glyph.is_ascii(),
                    !expect_unicode,
                    "glyph {glyph:?} belongs to the wrong glyph set (unicode={unicode})"
                );
            }
        }
    }

    /// Strip escape sequences for width assertions. Mirrors the production
    /// sanitizer without duplicating its escape parser.
    fn strip_ansi(text: &str) -> String {
        crate::alphacode_base::message::strip_ansi_escape_sequences(text)
    }
}
