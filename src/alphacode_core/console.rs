//! Console/terminal ANSI capability helpers.
//!
//! Rendering ANSI escapes before the console supports them shows literal
//! `←[90m` garbage on legacy Windows consoles (issue #498). These helpers let
//! early startup output decide whether color is safe, and opportunistically
//! enable VT processing the same way modern CLIs do.

/// Best-effort: enable ANSI (virtual terminal processing) on the stderr
/// console, then report whether ANSI output is safe to emit.
///
/// On non-Windows this is true exactly when stderr is a terminal. On Windows
/// it attempts to switch the console to VT mode first (a no-op on Windows
/// Terminal and modern conhost, which already support it) and returns false
/// when the console cannot accept escape sequences, so callers can fall back
/// to plain text instead of printing escape garbage.
pub fn stderr_supports_ansi() -> bool {
    use std::io::IsTerminal;
    if !std::io::stderr().is_terminal() {
        return false;
    }

    #[cfg(windows)]
    {
        enable_stderr_vt_processing()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

#[cfg(windows)]
fn enable_stderr_vt_processing() -> bool {
    use windows_sys::Win32::System::Console::{
        CONSOLE_MODE, ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle,
        STD_ERROR_HANDLE, SetConsoleMode,
    };

    unsafe {
        let handle = GetStdHandle(STD_ERROR_HANDLE);
        if handle.is_null() || handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return false;
        }
        let mut mode: CONSOLE_MODE = 0;
        if GetConsoleMode(handle, &mut mode) == 0 {
            return false;
        }
        if mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0 {
            return true;
        }
        SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
    }
}

// ── Professional Console Formatting ────────────────────────

/// ANSI escape codes for console formatting.
pub mod ansi {
    pub const RESET: &str = "\x1b[0m";
    pub const BOLD: &str = "\x1b[1m";
    pub const DIM: &str = "\x1b[2m";
    pub const ITALIC: &str = "\x1b[3m";
    pub const UNDERLINE: &str = "\x1b[4m";
    pub const RED: &str = "\x1b[31m";
    pub const GREEN: &str = "\x1b[32m";
    pub const YELLOW: &str = "\x1b[33m";
    pub const BLUE: &str = "\x1b[34m";
    pub const MAGENTA: &str = "\x1b[35m";
    pub const CYAN: &str = "\x1b[36m";
    pub const WHITE: &str = "\x1b[37m";
    pub const GRAY: &str = "\x1b[90m";
    pub const BG_DARK: &str = "\x1b[48;2;18;20;30m";
}

/// A professional console output builder providing structured, colorized,
/// and consistently formatted terminal output.
pub struct Console;

impl Console {
    /// Render a section header with decoration.
    pub fn section(title: &str, width: usize) -> String {
        let underline = "─".repeat(width.saturating_sub(title.len() + 4));
        format!("{}  {}  {}{}", ansi::DIM, title, underline, ansi::RESET)
    }

    /// Render a key-value pair with consistent alignment.
    pub fn key_value(key: &str, value: &str) -> String {
        let pad = 14_usize.saturating_sub(key.len());
        format!("  {}{}:{}", key, " ".repeat(pad), value)
    }

    /// Render a key-value pair with a colored value.
    pub fn key_value_colored(key: &str, value: &str, color: &str) -> String {
        let pad = 14_usize.saturating_sub(key.len());
        format!(
            "  {}{}: {}{}{}",
            key,
            " ".repeat(pad),
            color,
            value,
            ansi::RESET
        )
    }

    /// Render a success confirmation.
    pub fn success(message: &str) -> String {
        format!("{}  ✓  {}{}", ansi::GREEN, message, ansi::RESET)
    }

    /// Render an error message.
    pub fn error(message: &str) -> String {
        format!("{}  ✗  {}{}", ansi::RED, message, ansi::RESET)
    }

    /// Render a warning message.
    pub fn warning(message: &str) -> String {
        format!("{}  ⚠  {}{}", ansi::YELLOW, message, ansi::RESET)
    }

    /// Render an informational message.
    pub fn info(message: &str) -> String {
        format!("{}  ℹ  {}{}", ansi::BLUE, message, ansi::RESET)
    }

    /// Render a hint (dimmed) message.
    pub fn hint(message: &str) -> String {
        format!("{}  {}{}", ansi::DIM, message, ansi::RESET)
    }

    /// Render a bold title.
    pub fn title(message: &str) -> String {
        format!("{}{}{}", ansi::BOLD, message, ansi::RESET)
    }

    /// Render a subtle/dimmed line.
    pub fn subtle(message: &str) -> String {
        format!("{}{}{}", ansi::DIM, message, ansi::RESET)
    }

    /// Render a horizontal rule.
    pub fn rule(width: usize) -> String {
        "─".repeat(width)
    }

    /// Render a bordered box with title and content lines.
    pub fn box_text(title: &str, lines: &[&str], width: usize) -> Vec<String> {
        let mut result = Vec::new();
        let inner_w = width.saturating_sub(2);
        let border_top = format!("┌{}┐", "─".repeat(inner_w));
        let border_mid = format!("│{}│", " ".repeat(inner_w));
        let border_bot = format!("└{}┘", "─".repeat(inner_w));

        result.push(format!("{}  {}  {}", ansi::DIM, title, ansi::RESET));
        result.push(border_top);
        for line in lines {
            let content_w = inner_w.saturating_sub(2);
            let truncated: String = line.chars().take(content_w).collect();
            let padded = format!("{truncated:content_w$}", content_w = content_w);
            result.push(format!("│ {padded} │"));
        }
        result.push(border_mid);
        result.push(border_bot);
        result
    }

    /// Render a table with headers and rows.
    pub fn table(headers: &[&str], rows: &[Vec<&str>], widths: &[usize]) -> Vec<String> {
        let mut result = Vec::new();

        // Header row
        result.push(Self::render_row(headers, widths, true));

        // Separator
        result.push(Self::separator(widths));

        // Data rows
        for row in rows {
            result.push(Self::render_row(row, widths, false));
        }

        result
    }

    /// Render a progress bar with label and percentage.
    pub fn progress_bar(label: &str, progress: f32, width: usize) -> String {
        let filled = (progress.clamp(0.0, 1.0) * width as f32).round() as usize;
        let empty = width.saturating_sub(filled);
        let bar = format!("[{}{}]", "█".repeat(filled.min(width)), "░".repeat(empty));
        let pct = (progress * 100.0) as u32;
        let color = if progress >= 1.0 {
            ansi::GREEN
        } else if progress > 0.5 {
            ansi::BLUE
        } else if progress > 0.0 {
            ansi::YELLOW
        } else {
            ansi::GRAY
        };
        format!(
            "  {} {} {}{}{}{}",
            label,
            color,
            bar,
            ansi::RESET,
            ansi::DIM,
            format!(" {:.0}%", pct)
        )
    }

    /// Render a loading line with animated spinner frame.
    pub fn loading(label: &str, frame: usize) -> String {
        const SPINNERS: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        format!("  {} {}", SPINNERS[frame % SPINNERS.len()], label)
    }

    /// Render a multi-line error block with optional fix hint.
    pub fn error_block(what: &str, why: &str, fix: Option<&str>) -> String {
        let mut out = String::new();
        out.push_str(&Self::error(what));
        out.push('\n');
        out.push_str(&format!("  {}", why));
        if let Some(fix) = fix {
            out.push('\n');
            out.push_str(&Self::info(&format!("fix: {}", fix)));
        }
        out
    }

    /// Render a version banner.
    pub fn version_banner(name: &str, version: &str, width: usize) -> String {
        let _inner = width.saturating_sub(name.len() + version.len() + 6);
        format!(
            "{}  {} v{}  {}{}{}",
            ansi::BG_DARK,
            ansi::BOLD,
            version,
            ansi::DIM,
            name,
            ansi::RESET,
        )
    }

    /// Clear the terminal screen.
    pub fn clear_screen() -> String {
        format!("\x1b[2J\x1b[H")
    }

    /// Move cursor to home position.
    pub fn cursor_home() -> String {
        "\x1b[H".to_string()
    }

    /// Hide the cursor.
    pub fn hide_cursor() -> String {
        "\x1b[?25l".to_string()
    }

    /// Show the cursor.
    pub fn show_cursor() -> String {
        "\x1b[?25h".to_string()
    }

    /// Save cursor position.
    pub fn cursor_save() -> String {
        "\x1b7".to_string()
    }

    /// Restore cursor position.
    pub fn cursor_restore() -> String {
        "\x1b8".to_string()
    }

    /// Move cursor up N lines.
    pub fn cursor_up(n: usize) -> String {
        format!("\x1b[{}A", n)
    }

    /// Move cursor down N lines.
    pub fn cursor_down(n: usize) -> String {
        format!("\x1b[{}B", n)
    }

    /// Clear from cursor to end of screen.
    pub fn clear_after() -> String {
        "\x1b[0J".to_string()
    }

    /// Clear current line.
    pub fn clear_line() -> String {
        "\x1b[2K".to_string()
    }

    fn render_row(cells: &[&str], widths: &[usize], is_header: bool) -> String {
        let prefix = if is_header { ansi::BOLD } else { "" };
        let suffix = if is_header { ansi::RESET } else { "" };
        let mut result = String::new();
        for (i, cell) in cells.iter().enumerate() {
            let w = widths.get(i).copied().unwrap_or(15);
            let display = if cell.chars().count() > w.saturating_sub(1) {
                let truncated: String = cell.chars().take(w.saturating_sub(2)).collect();
                format!("{truncated}…")
            } else {
                cell.to_string()
            };
            result.push_str(&format!("{prefix} {:<w$} {suffix}  ", display, w = w));
        }
        result
    }

    fn separator(widths: &[usize]) -> String {
        let mut result = String::new();
        for w in widths {
            result.push_str(&format!(" {} ", "─".repeat(w.saturating_sub(2))));
        }
        result
    }
}
