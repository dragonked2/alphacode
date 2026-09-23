pub const QUIET_ENV: &str = "ALPHACODE_QUIET";

pub fn set_quiet_enabled(enabled: bool) {
    if enabled {
        crate::alphacode_core::env::set_var(QUIET_ENV, "1");
    } else {
        crate::alphacode_core::env::remove_var(QUIET_ENV);
    }
}

pub fn quiet_enabled() -> bool {
    std::env::var(QUIET_ENV)
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

pub fn stderr_info(message: impl AsRef<str>) {
    if !quiet_enabled() {
        eprintln!("{}", crate::output_style::terminal_text(message.as_ref()));
    }
}

pub fn terminal_title(title: impl AsRef<str>) -> String {
    crate::output_style::terminal_text(title.as_ref()).into_owned()
}

pub fn stderr_blank_line() {
    if !quiet_enabled() {
        eprintln!();
    }
}

// ── Professional CLI Output Formatting ──────────────────────────────

/// ANSI color codes for professional CLI output.
#[allow(dead_code)]
mod ansi {
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

/// A professional CLI output builder for structured, colorized output.
pub struct CliOutput;

impl CliOutput {
    /// Render a section header with underline decoration.
    pub fn section(title: &str) -> String {
        format!("{}{}{}", ansi::BOLD, title, ansi::RESET)
    }

    /// Render a key-value pair with aligned columns.
    pub fn key_value(key: &str, value: &str) -> String {
        let pad = 12_usize.saturating_sub(key.len());
        format!("  {}{}:{}", key, " ".repeat(pad), value)
    }

    /// Render a key-value pair with the value highlighted in a specific color.
    pub fn key_value_colored(key: &str, value: &str, color: &str) -> String {
        let pad = 12_usize.saturating_sub(key.len());
        format!(
            "  {}{}: {}{}{}",
            key,
            " ".repeat(pad),
            color,
            value,
            ansi::RESET
        )
    }

    /// Render a success confirmation line.
    pub fn success(message: &str) -> String {
        format!("{}  ✓  {}{}", ansi::GREEN, message, ansi::RESET)
    }

    /// Render an error line.
    pub fn error(message: &str) -> String {
        format!("{}  ✗  {}{}", ansi::RED, message, ansi::RESET)
    }

    /// Render a warning line.
    pub fn warning(message: &str) -> String {
        format!("{}  ⚠  {}{}", ansi::YELLOW, message, ansi::RESET)
    }

    /// Render an informational line.
    pub fn info(message: &str) -> String {
        format!("{}  ℹ  {}{}", ansi::BLUE, message, ansi::RESET)
    }

    /// Render a dimmed/hint line.
    pub fn hint(message: &str) -> String {
        format!("{}  {}{}", ansi::DIM, message, ansi::RESET)
    }

    /// Render a bold title line.
    pub fn title(message: &str) -> String {
        format!("{}{}{}", ansi::BOLD, message, ansi::RESET)
    }

    /// Render a dimmed/subtle line.
    pub fn subtle(message: &str) -> String {
        format!("{}{}{}", ansi::DIM, message, ansi::RESET)
    }

    /// Render a horizontal rule.
    pub fn rule(width: usize) -> String {
        "─".repeat(width)
    }

    /// Render a bordered box around text.
    pub fn box_text(title: &str, lines: &[&str], width: usize) -> Vec<String> {
        let mut result = Vec::new();
        let border = "┌".to_string() + &"─".repeat(width.saturating_sub(2)) + "┐";
        let mid = "│".to_string() + &" ".repeat(width.saturating_sub(2)) + "│";
        let bot = "└".to_string() + &"─".repeat(width.saturating_sub(2)) + "┘";

        result.push(format!("{}  {}  {}", ansi::DIM, title, ansi::RESET));
        result.push(border);
        for line in lines {
            let content_w = width.saturating_sub(4);
            let truncated: String = line.chars().take(content_w).collect();
            let padded = format!("{truncated:content_w$}", content_w = content_w);
            result.push(format!("│ {padded} │"));
        }
        result.push(mid);
        result.push(bot);
        result
    }

    /// Render a table with headers and rows.
    pub fn table(headers: &[&str], rows: &[Vec<&str>], widths: &[usize]) -> Vec<String> {
        let mut result = Vec::new();

        // Header row
        let header_line = Self::render_table_row(headers, widths, true);
        result.push(header_line);

        // Separator
        let sep = Self::table_separator(widths);
        result.push(sep);

        // Data rows
        for row in rows {
            result.push(Self::render_table_row(row, widths, false));
        }

        result
    }

    fn render_table_row(cells: &[&str], widths: &[usize], is_header: bool) -> String {
        let mut result = String::new();
        let prefix = if is_header { ansi::BOLD } else { "" };
        let suffix = if is_header { ansi::RESET } else { "" };

        for (i, cell) in cells.iter().enumerate() {
            let w = widths.get(i).copied().unwrap_or(15);
            let char_count = cell.chars().count();
            let display = if char_count > w.saturating_sub(1) {
                let truncated: String = cell.chars().take(w.saturating_sub(2)).collect();
                format!("{truncated}…")
            } else {
                cell.to_string()
            };
            result.push_str(&format!("{prefix} {:<w$} {suffix}  ", display, w = w));
        }
        result
    }

    fn table_separator(widths: &[usize]) -> String {
        let mut result = String::new();
        for w in widths {
            result.push_str(&format!(" {} ", "─".repeat(w.saturating_sub(2))));
        }
        result
    }

    /// Render a progress bar with percentage.
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
            "  {} {} {}{} {} {:.0}%",
            label,
            color,
            bar,
            ansi::RESET,
            ansi::DIM,
            pct
        )
    }

    /// Render a spinner frame for loading states.
    pub fn spinner(frame: usize) -> &'static str {
        const FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        FRAMES[frame % FRAMES.len()]
    }

    /// Render a loading line with spinner.
    pub fn loading(label: &str, frame: usize) -> String {
        format!("  {} {}", Self::spinner(frame), label)
    }

    /// Render a confirmation prompt.
    pub fn confirm(message: &str) -> String {
        format!("  {} ? {} {} (y/n)", ansi::CYAN, ansi::RESET, message)
    }

    /// Render a multi-line error block with context.
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
        let _border = "═".repeat(width.saturating_sub(2));
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
}
