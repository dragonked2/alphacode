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
        // Windows Terminal / iTerm2 / most modern terminals default to dark.
        // PowerShell defaults to dark blue. cmd.exe defaults to light.
        if cfg!(windows) {
            // Heuristic: if TERM_PROGRAM is set, assume dark.
            if std::env::var("TERM_PROGRAM").is_ok() {
                return false;
            }
            return std::env::var("WT_SESSION").is_err();
        }
        false
    })
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
pub fn rgb_escape(r: u8, g: u8, b: u8, fg: bool) -> String {
    if fg {
        format!("\x1b[38;2;{};{};{}m", r, g, b)
    } else {
        format!("\x1b[48;2;{};{};{}m", r, g, b)
    }
}

/// Reset all ANSI attributes.
pub const RESET: &str = "\x1b[0m";

/// Box-drawing characters with ASCII fallback. We detect capability
/// from the LANG / LC_ALL / TERM env vars; if any of them contains
/// "UTF-8" or "utf8" we use Unicode, otherwise ASCII.
pub mod box_chars {
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
        CACHE.get_or_init(|| {
            let unicode = std::env::var("LANG")
                .map(|s| s.to_lowercase().contains("utf"))
                .unwrap_or(false)
                || std::env::var("LC_ALL")
                    .map(|s| s.to_lowercase().contains("utf"))
                    .unwrap_or(false)
                || std::env::var("LC_CTYPE")
                    .map(|s| s.to_lowercase().contains("utf"))
                    .unwrap_or(false)
                || std::env::var("TERM")
                    .map(|s| s.to_lowercase().contains("utf"))
                    .unwrap_or(false)
                || cfg!(windows); // Windows console renders box chars fine
            if unicode { &UNICODE } else { &ASCII }
        })
    }
}

/// Render a single-line box around a label. Used for slash-command
/// banners and section dividers.
///
/// Example: `box_top("security-audit · 12 findings")`
/// ```
/// ─── security-audit · 12 findings ─────────────────
/// ```
pub fn box_top(label: &str, width: usize) -> String {
    let bc = box_chars::pick();
    let label_width = unicode_width::UnicodeWidthStr::width(label);
    let pad = width.saturating_sub(label_width + 4).max(4);
    let left = pad / 2;
    let right = pad - left;
    let mut out = String::with_capacity(width + 2);
    for _ in 0..left {
        out.push(bc.h);
    }
    out.push(' ');
    out.push_str(&terminal_text(label));
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

/// Render a status line: `prefix message`. Respects emoji mode and
/// paints the prefix in the matching color (24-bit RGB).
pub fn status_line(prefix: StatusPrefix, message: &str) -> String {
    let (r, g, b) = pick_for_terminal_bg(prefix.token);
    let color = rgb_escape(r, g, b, true);
    let glyph = if emoji_enabled() {
        prefix.emoji
    } else {
        prefix.ascii
    };
    format!("{}{}{} {}", color, glyph, RESET, terminal_text(message))
}

/// Render an error line with a leading explanation. The caller
/// supplies a short "what" and a longer "why / how to fix".
pub fn error_block(what: &str, why: &str, fix: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str(&status_line(PREFIX_ERROR, what));
    out.push('\n');
    out.push_str(&format!("  {}", terminal_text(why)));
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
    let mut out = String::new();
    out.push_str(&divider(None, width));
    out.push('\n');
    out.push_str(&format!(
        "{}{}{}{}",
        rgb_escape(r, g, b, true),
        terminal_text(title),
        RESET,
        "\n",
    ));
    if let Some(sub) = subtitle {
        out.push_str(&format!(
            "{}{}{}{}",
            rgb_escape(mr, mg, mb, true),
            terminal_text(sub),
            RESET,
            "\n",
        ));
    }
    out.push_str(&divider(None, width));
    out
}

/// Render a "key" line for slash commands. Used in help output to
/// show the shortcut next to the action.
pub fn keyline(key: &str, action: &str) -> String {
    let (r, g, b) = pick_for_terminal_bg(PaletteToken::Accent);
    let (mr, mg, mb) = pick_for_terminal_bg(PaletteToken::Muted);
    format!(
        "  {}{:<14}{}  {}",
        rgb_escape(r, g, b, true),
        key,
        RESET,
        format_args!(
            "{}{}{}",
            rgb_escape(mr, mg, mb, true),
            terminal_text(action),
            RESET,
        )
    )
}

/// Render a table row. `widths` is the column widths; `cells` is
/// the cell text. If a cell overflows, it is truncated with an
/// ellipsis. The first column is left-aligned; the rest are
/// left-aligned by default. Use `align_right` for right-aligned
/// numeric columns.
pub fn table_row(widths: &[usize], cells: &[&str], align_right: &[bool]) -> String {
    let mut out = String::new();
    for (i, cell) in cells.iter().enumerate() {
        let w = widths.get(i).copied().unwrap_or(20);
        let display_w = unicode_width::UnicodeWidthStr::width(*cell);
        let truncated = if display_w > w {
            truncate_to_width(cell, w.saturating_sub(1)) + "…"
        } else {
            cell.to_string()
        };
        if align_right.get(i).copied().unwrap_or(false) {
            out.push_str(&format!("{:>width$}  ", truncated, width = w));
        } else {
            out.push_str(&format!("{:<width$}  ", truncated, width = w));
        }
    }
    out
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
pub fn format_duration(secs: f64) -> String {
    if secs < 1.0 {
        return format!("{}ms", (secs * 1000.0).round() as u64);
    }
    if secs < 60.0 {
        return format!("{:.1}s", secs);
    }
    if secs < 3600.0 {
        let m = (secs / 60.0).floor() as u64;
        let s = (secs % 60.0).round() as u64;
        return format!("{}m {}s", m, s);
    }
    let h = (secs / 3600.0).floor() as u64;
    let m = ((secs % 3600.0) / 60.0).round() as u64;
    format!("{}h {}m", h, m)
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

/// Spinner frames, both emoji and ASCII. The caller rotates them.
pub const SPINNER_EMOJI: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
pub const SPINNER_ASCII: &[&str] = &["-", "\\", "|", "/"];

/// Return the current spinner frame for the given tick.
pub fn spinner_frame(tick: usize) -> &'static str {
    let frames = if emoji_enabled() {
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
    fn format_count_basic() {
        assert_eq!(format_count(0), "0");
        assert_eq!(format_count(999), "999");
        assert_eq!(format_count(1000), "1,000");
        assert_eq!(format_count(1234567), "1,234,567");
    }

    #[test]
    fn truncate_to_width_works() {
        let s = "hello world";
        let t = truncate_to_width(s, 7);
        assert!(unicode_width::UnicodeWidthStr::width(t.as_str()) <= 7);
    }

    #[test]
    fn truncate_to_width_cjk() {
        let s = "日本語のテスト";
        let t = truncate_to_width(s, 6);
        // CJK characters are 2 cells wide; max 3 chars
        assert!(unicode_width::UnicodeWidthStr::width(t.as_str()) <= 6);
    }

    #[test]
    fn status_line_works() {
        let s = status_line(PREFIX_OK, "12 findings");
        assert!(s.contains("12 findings"));
    }

    #[test]
    fn spinner_cycles() {
        let a = spinner_frame(0);
        let b = spinner_frame(100);
        // ASCII mode falls back since the test env may or may not have emoji enabled.
        assert!(!a.is_empty() && !b.is_empty());
    }

    #[test]
    fn box_chars_pick_returns_something() {
        let bc = box_chars::pick();
        assert!(!bc.h.is_whitespace());
    }
}
