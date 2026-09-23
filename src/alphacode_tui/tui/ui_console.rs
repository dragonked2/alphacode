//! Professional console output formatting for alphacode.
//!
//! Provides rich, colorized, structured output for:
//! - CLI command results
//! - TUI status messages
//! - Provider/model information
//! - Session statistics
//! - Error and success confirmations
//!
//! All output is designed to be:
//! - Immediately scannable (headers, key/value pairs, badges)
//! - Color-coded by semantic meaning (success, warning, error, info)
//! - Consistent across all user-facing surfaces

use crate::alphacode_tui::tui::brand_ux::BrandTheme;
use crate::alphacode_tui::tui::color_support::rgb;
use ratatui::style::Color;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use std::time::Duration;

/// Semantic color roles for console output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputColor {
    /// Success / confirmation (green).
    Success,
    /// Error / failure (red).
    Error,
    /// Warning / caution (amber).
    Warning,
    /// Informational (blue/cyan).
    Info,
    /// Primary brand accent (violet).
    Accent,
    /// Secondary / muted text (dim gray).
    Dim,
    /// Bright primary text (white/light gray).
    Bright,
}

impl OutputColor {
    fn to_color(self) -> Color {
        match self {
            Self::Success => BrandTheme::success(),
            Self::Error => BrandTheme::error(),
            Self::Warning => BrandTheme::warning(),
            Self::Info => BrandTheme::info(),
            Self::Accent => BrandTheme::accent(),
            Self::Dim => BrandTheme::dim(),
            Self::Bright => rgb(220, 220, 220),
        }
    }
}

/// A single styled line of console output.
#[derive(Debug, Clone)]
pub struct ConsoleLine {
    pub content: String,
    pub color: OutputColor,
    pub modifier: Modifier,
}

impl std::fmt::Display for ConsoleLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.content)
    }
}

impl ConsoleLine {
    pub fn new(content: impl Into<String>, color: OutputColor) -> Self {
        Self {
            content: content.into(),
            color,
            modifier: Modifier::empty(),
        }
    }

    pub fn bold(content: impl Into<String>, color: OutputColor) -> Self {
        Self {
            content: content.into(),
            color,
            modifier: Modifier::BOLD,
        }
    }

    pub fn italic(content: impl Into<String>, color: OutputColor) -> Self {
        Self {
            content: content.into(),
            color,
            modifier: Modifier::ITALIC,
        }
    }

    pub fn into_line(self) -> Line<'static> {
        Line::from(Span::styled(
            self.content,
            Style::default()
                .fg(self.color.to_color())
                .add_modifier(self.modifier),
        ))
    }
}

/// Gradient separator line for visual section breaks.
pub fn gradient_separator(width: usize) -> Line<'static> {
    let gradient = BrandTheme::gradient();
    let total_chars = width.min(120);
    let mut colors: Vec<Color> = Vec::with_capacity(total_chars);
    for i in 0..total_chars {
        let hue_t = i as f32 / total_chars as f32;
        let seg = hue_t * (gradient.len() - 1) as f32;
        let idx = seg.floor() as usize;
        let frac = seg - seg.floor();
        let c0 = gradient[idx.min(gradient.len() - 1)];
        let c1 = gradient[(idx + 1).min(gradient.len() - 1)];
        colors.push(blend_colors(c0, c1, frac));
    }
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(total_chars / 6 + 1);
    let mut run_start = 0;
    for i in 1..=total_chars {
        if i == total_chars || colors[i] != colors[i - 1] {
            let n = i - run_start;
            let text: String = std::iter::repeat_n('─', n).collect();
            spans.push(Span::styled(
                text,
                Style::default()
                    .fg(colors[run_start])
                    .add_modifier(Modifier::DIM),
            ));
            run_start = i;
        }
    }
    Line::from(spans)
}

/// Linearly interpolate between two colors.
fn blend_colors(a: Color, b: Color, t: f32) -> Color {
    let (r1, g1, b1) = match a {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => return a,
    };
    let (r2, g2, b2) = match b {
        Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
        _ => return b,
    };
    rgb(
        (r1 + (r2 - r1) * t) as u8,
        (g1 + (g2 - g1) * t) as u8,
        (b1 + (b2 - b1) * t) as u8,
    )
}

/// Professional banner for the application header.
pub fn app_banner(version: &str, width: usize) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let _bright = OutputColor::Bright;
    let dim = OutputColor::Dim;

    // Gradient wordmark
    let gradient = BrandTheme::gradient();
    let wordmark = "Alphacode";
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run_color = gradient[0];
    let mut run_text = String::new();
    for (i, ch) in wordmark.chars().enumerate() {
        let color = gradient[i % gradient.len()];
        if color != run_color && !run_text.is_empty() {
            spans.push(Span::styled(
                std::mem::take(&mut run_text),
                Style::default().fg(run_color).add_modifier(Modifier::BOLD),
            ));
            run_color = color;
        }
        run_text.push(ch);
    }
    if !run_text.is_empty() {
        spans.push(Span::styled(
            run_text,
            Style::default().fg(run_color).add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(
        format!(" v{}", version),
        Style::default().fg(BrandTheme::dim()),
    ));
    lines.push(Line::from(spans));
    lines.push(ConsoleLine::new("  Terminal-native AI coding agent", dim).into_line());
    lines.push(gradient_separator(width));

    lines
}

/// Professional section header for console output.
pub fn section_header(title: &str, width: usize) -> Vec<Line<'static>> {
    vec![
        ConsoleLine::bold(format!("  {} ", title), OutputColor::Accent).into_line(),
        gradient_separator(width),
    ]
}

/// Key-value pair display for structured console output.
pub fn key_value(key: &str, value: &str) -> ConsoleLine {
    ConsoleLine::new(format!("  {}: {}", key, value), OutputColor::Bright)
}

/// Key-value pair with highlighted value (e.g. for important settings).
pub fn key_value_highlighted(key: &str, value: &str, highlight_color: OutputColor) -> ConsoleLine {
    let content = format!("  {}: {} ", key, value);
    let mut line = ConsoleLine::new(content, highlight_color);
    line.modifier = Modifier::BOLD;
    line
}

/// Confirmation badge for successful operations.
pub fn success_badge(message: &str) -> ConsoleLine {
    ConsoleLine::bold(format!("  ✓  {}", message), OutputColor::Success)
}

/// Error badge for failed operations.
pub fn error_badge(message: &str) -> ConsoleLine {
    ConsoleLine::bold(format!("  ✖  {}", message), OutputColor::Error)
}

/// Warning badge for non-fatal issues.
pub fn warning_badge(message: &str) -> ConsoleLine {
    ConsoleLine::bold(format!("  ⚠  {}", message), OutputColor::Warning)
}

/// Info badge for informational messages.
pub fn info_badge(message: &str) -> ConsoleLine {
    ConsoleLine::bold(format!("  i  {}", message), OutputColor::Info)
}

/// Provider/model info card for console display.
pub fn provider_info_card(provider: &str, model: &str, status: &str) -> Vec<Line<'static>> {
    let status_color = match status {
        "active" | "ready" | "online" => OutputColor::Success,
        "error" | "offline" | "unavailable" => OutputColor::Error,
        _ => OutputColor::Info,
    };

    vec![
        ConsoleLine::bold(format!("  Provider: {}", provider), OutputColor::Accent).into_line(),
        ConsoleLine::new(format!("  Model:    {}", model), OutputColor::Bright).into_line(),
        ConsoleLine::bold(format!("  Status:   {}", status), status_color).into_line(),
    ]
}

/// Session statistics card.
pub fn session_stats_card(
    turn_count: u64,
    input_tokens: u64,
    output_tokens: u64,
    elapsed: Option<Duration>,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    lines.push(ConsoleLine::bold("  Session Statistics", OutputColor::Accent).into_line());
    lines.push(
        ConsoleLine::new(format!("  Turns:     {}", turn_count), OutputColor::Bright).into_line(),
    );
    lines.push(
        ConsoleLine::new(
            format!("  Input:     {} tokens", input_tokens),
            OutputColor::Bright,
        )
        .into_line(),
    );
    lines.push(
        ConsoleLine::new(
            format!("  Output:    {} tokens", output_tokens),
            OutputColor::Bright,
        )
        .into_line(),
    );

    if let Some(elapsed) = elapsed {
        let secs = elapsed.as_secs();
        let time_str = if secs < 60 {
            format!("{}s", secs)
        } else if secs < 3600 {
            format!("{}m {}s", secs / 60, secs % 60)
        } else {
            format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
        };
        lines.push(
            ConsoleLine::new(format!("  Elapsed:   {}", time_str), OutputColor::Bright).into_line(),
        );
    }

    if input_tokens > 0 || output_tokens > 0 {
        let total = input_tokens + output_tokens;
        let ratio = if total > 0 {
            output_tokens as f32 / total as f32 * 100.0
        } else {
            0.0
        };
        let ratio_color = if ratio > 70.0 {
            OutputColor::Success
        } else if ratio > 30.0 {
            OutputColor::Info
        } else {
            OutputColor::Warning
        };
        lines.push(
            ConsoleLine::new(format!("  Ratio:     {:.1}% output", ratio), ratio_color).into_line(),
        );
    }

    lines
}

/// Progress bar for long-running operations.
pub fn progress_bar(label: &str, progress: f32, width: usize) -> ConsoleLine {
    let filled = (progress * width as f32).round() as usize;
    let empty = width.saturating_sub(filled);
    let bar = format!(
        "[{}{}] {:.0}%",
        "█".repeat(filled.min(width)),
        "░".repeat(empty),
        progress * 100.0
    );
    let color = if progress >= 1.0 {
        OutputColor::Success
    } else if progress > 0.5 {
        OutputColor::Info
    } else {
        OutputColor::Warning
    };
    ConsoleLine::new(format!("  {} {}", label, bar), color)
}

/// Elapsed time formatter.
pub fn format_elapsed(duration: Duration) -> String {
    let secs = duration.as_secs();
    let ms = duration.subsec_millis();
    if secs < 60 {
        if secs == 0 && ms == 0 {
            "0ms".to_string()
        } else if secs == 0 {
            format!("{}ms", ms)
        } else if ms == 0 {
            format!("{}s", secs)
        } else {
            format!("{}.{:03}s", secs, ms)
        }
    } else if secs < 3600 {
        format!("{}m {}s", secs / 60, secs % 60)
    } else {
        format!("{}h {}m", secs / 3600, (secs % 3600) / 60)
    }
}

/// Token count formatter with units.
pub fn format_tokens(tokens: u64) -> String {
    if tokens < 1000 {
        format!("{}tok", tokens)
    } else if tokens < 1_000_000 {
        format!("{:.1}ktok", tokens as f64 / 1000.0)
    } else {
        format!("{:.1}Mtok", tokens as f64 / 1_000_000.0)
    }
}

/// Terminal width adapter for responsive console output.
pub fn adapt_width(width: usize, max_width: usize) -> usize {
    width.min(max_width).max(20)
}

// ── Professional Screen & Terminal Management ─────────────

/// ANSI escape sequences for terminal screen management.
pub mod screen {
    /// Clear entire screen and move cursor home.
    pub const CLEAR: &str = "\x1b[2J\x1b[H";
    /// Move cursor to home position.
    pub const HOME: &str = "\x1b[H";
    /// Hide cursor.
    pub const HIDE_CURSOR: &str = "\x1b[?25l";
    /// Show cursor.
    pub const SHOW_CURSOR: &str = "\x1b[?25h";
    /// Save cursor position.
    pub const SAVE: &str = "\x1b7";
    /// Restore cursor position.
    pub const RESTORE: &str = "\x1b8";
    /// Clear from cursor to end of screen.
    pub const CLEAR_AFTER: &str = "\x1b[0J";
    /// Clear current line.
    pub const CLEAR_LINE: &str = "\x1b[2K";
    /// Clear from cursor to beginning of screen.
    pub const CLEAR_BEFORE: &str = "\x1b[1J";
    /// Enable alternate screen (full-screen TUI).
    pub const ALT_SCREEN_ON: &str = "\x1b[?1049h";
    /// Disable alternate screen.
    pub const ALT_SCREEN_OFF: &str = "\x1b[?1049l";
    /// Enable bracketed paste mode.
    pub const BRACKETED_PASTE_ON: &str = "\x1b[?2004h";
    /// Disable bracketed paste mode.
    pub const BRACKETED_PASTE_OFF: &str = "\x1b[?2004l";
    /// Enable mouse tracking.
    pub const MOUSE_ON: &str = "\x1b[?1000h";
    /// Disable mouse tracking.
    pub const MOUSE_OFF: &str = "\x1b[?1000l";
    /// Enable focus tracking.
    pub const FOCUS_ON: &str = "\x1b[?1004h";
    /// Disable focus tracking.
    pub const FOCUS_OFF: &str = "\x1b[?1004l";
}

/// Professional status line renderer for the TUI bottom bar.
pub mod status {
    use super::*;

    /// Render a professional status line with left, center, and right sections.
    pub fn render_bar(
        left: &[Span<'static>],
        center: &[Span<'static>],
        right: &[Span<'static>],
        width: u16,
    ) -> Line<'static> {
        let left_w: usize = left
            .iter()
            .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
            .sum();
        let right_w: usize = right
            .iter()
            .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
            .sum();
        let center_w = (width as usize).saturating_sub(left_w + right_w);

        let mut spans: Vec<Span<'static>> = Vec::new();
        spans.extend_from_slice(left);

        if center_w > 0 && !center.is_empty() {
            let center_text: String = center.iter().map(|s| s.content.as_ref()).collect();
            let padded = format!("{:^width$}", center_text, width = center_w);
            spans.push(Span::raw(padded));
        } else if center_w > 0 {
            spans.push(Span::raw(" ".repeat(center_w)));
        }

        spans.extend_from_slice(right);
        Line::from(spans)
    }

    /// Render a simple single-section status line.
    pub fn render_simple(content: &[Span<'static>], width: u16) -> Line<'static> {
        let content_w: usize = content
            .iter()
            .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
            .sum();
        if content_w >= width as usize {
            return Line::from(content.to_vec());
        }
        let mut spans = Vec::with_capacity(content.len() + 1);
        spans.extend_from_slice(content);
        spans.push(Span::raw(" ".repeat(width as usize - content_w)));
        Line::from(spans)
    }
}

/// Professional table renderer for console output.
pub mod table {
    use super::*;

    /// Render a table with headers, rows, and column widths.
    pub fn render(headers: &[&str], rows: &[Vec<&str>], widths: &[usize]) -> Vec<Line<'static>> {
        let mut lines = Vec::new();

        // Header
        lines.push(Line::from(render_row(headers, widths, true)));

        // Separator
        lines.push(Line::from(render_separator(widths)));

        // Rows
        for row in rows {
            lines.push(Line::from(render_row(row, widths, false)));
        }

        lines
    }

    fn render_row(cells: &[&str], widths: &[usize], is_header: bool) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        let prefix_style = if is_header {
            Style::default()
                .fg(BrandTheme::accent())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        let _suffix_style = Style::default();

        for (i, cell) in cells.iter().enumerate() {
            let w = widths.get(i).copied().unwrap_or(15);
            let display = if cell.len() > w.saturating_sub(1) {
                format!("{}…", &cell[..w.saturating_sub(2)])
            } else {
                cell.to_string()
            };
            let padded = format!(" {:<w$} ", display, w = w);
            spans.push(Span::styled(padded, prefix_style));
        }
        spans
    }

    fn render_separator(widths: &[usize]) -> Vec<Span<'static>> {
        let mut spans = Vec::new();
        for w in widths {
            let sep = "─".repeat(w.saturating_sub(2));
            spans.push(Span::styled(
                format!(" {} ", sep),
                Style::default()
                    .fg(BrandTheme::dim())
                    .add_modifier(Modifier::DIM),
            ));
        }
        spans
    }
}

/// Professional progress bar renderer with animation support.
pub mod progress {
    use super::*;

    /// Render a progress bar as styled spans with gradient fill.
    pub fn render(label: &str, progress: f32, width: usize, frame: usize) -> Line<'static> {
        let filled = (progress.clamp(0.0, 1.0) * width as f32).round() as usize;
        let empty = width.saturating_sub(filled);
        let gradient = BrandTheme::gradient();

        let mut spans: Vec<Span<'static>> = Vec::new();

        // Left bracket
        spans.push(Span::styled("[", Style::default().fg(BrandTheme::dim())));

        // Filled portion with gradient
        if filled > 0 {
            let seg_count = gradient.len().min(filled);
            let seg_size = filled / seg_count;
            let remainder = filled - seg_size * seg_count;
            for seg in 0..seg_count {
                let n = seg_size + if seg < remainder { 1 } else { 0 };
                if n == 0 {
                    continue;
                }
                let color = gradient[(seg + frame) % gradient.len()];
                spans.push(Span::styled("█".repeat(n), Style::default().fg(color)));
            }
        }

        // Empty portion
        if empty > 0 {
            spans.push(Span::styled(
                "░".repeat(empty),
                Style::default().fg(BrandTheme::dim()),
            ));
        }

        // Right bracket
        spans.push(Span::styled("]", Style::default().fg(BrandTheme::dim())));

        // Percentage
        let pct = (progress * 100.0) as u32;
        let pct_color = if progress >= 1.0 {
            BrandTheme::success()
        } else if progress > 0.5 {
            BrandTheme::info()
        } else {
            BrandTheme::warning()
        };
        spans.push(Span::styled(
            format!(" {:.0}%", pct),
            Style::default().fg(pct_color).add_modifier(Modifier::BOLD),
        ));

        // Label
        if !label.is_empty() {
            spans.push(Span::styled(
                format!(" {}", label),
                Style::default().fg(BrandTheme::dim_bright()),
            ));
        }

        Line::from(spans)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn banner_has_version() {
        let lines = app_banner("1.0.52", 80);
        assert!(!lines.is_empty());
        assert!(lines[0].to_string().contains("1.0.52"));
    }

    #[test]
    fn section_header_format() {
        let lines = section_header("Providers", 80);
        assert_eq!(lines.len(), 2);
        assert!(lines[0].to_string().contains("Providers"));
    }

    #[test]
    fn success_badge_format() {
        let badge = success_badge("Login successful");
        assert!(badge.to_string().contains("Login successful"));
    }

    #[test]
    fn provider_card_format() {
        let lines = provider_info_card("openai", "gpt-5.6-sol", "active");
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn session_stats_format() {
        let lines = session_stats_card(10, 5000, 3000, None);
        assert!(lines.len() >= 4);
    }

    #[test]
    fn elapsed_formatting() {
        assert_eq!(format_elapsed(Duration::from_millis(50)), "50ms");
        assert_eq!(format_elapsed(Duration::from_secs(30)), "30s");
        assert_eq!(format_elapsed(Duration::from_secs(90)), "1m 30s");
        assert_eq!(format_elapsed(Duration::from_secs(3700)), "1h 1m");
    }

    #[test]
    fn token_formatting() {
        assert_eq!(format_tokens(500), "500tok");
        assert_eq!(format_tokens(1500), "1.5ktok");
        assert_eq!(format_tokens(1500000), "1.5Mtok");
    }

    #[test]
    fn progress_bar_complete() {
        let bar = progress_bar("Loading", 1.0, 20);
        assert!(bar.to_string().contains("100%"));
    }

    #[test]
    fn output_colors_have_values() {
        for color in [
            OutputColor::Success,
            OutputColor::Error,
            OutputColor::Warning,
            OutputColor::Info,
            OutputColor::Accent,
            OutputColor::Dim,
            OutputColor::Bright,
        ] {
            assert_ne!(color.to_color(), Color::Reset);
        }
    }

    #[test]
    fn screen_constants_are_not_empty() {
        assert_ne!(screen::CLEAR, "");
        assert_ne!(screen::HIDE_CURSOR, "");
        assert_ne!(screen::SHOW_CURSOR, "");
        assert_ne!(screen::ALT_SCREEN_ON, "");
        assert_ne!(screen::ALT_SCREEN_OFF, "");
    }

    #[test]
    fn status_bar_render() {
        let left = vec![Span::styled("left", Style::default())];
        let center = vec![Span::styled("center", Style::default())];
        let right = vec![Span::styled("right", Style::default())];
        let line = status::render_bar(&left, &center, &right, 80);
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn table_render() {
        let headers = ["Name", "Status", "Version"];
        let rows = vec![
            vec!["alphacode", "active", "1.0.0"],
            vec!["tool", "idle", "2.1.0"],
        ];
        let widths = &[12usize, 10, 10];
        let lines = table::render(&headers, &rows, widths);
        assert_eq!(lines.len(), 4); // header + separator + 2 rows
    }

    #[test]
    fn progress_render() {
        let line = progress::render("test", 0.5, 20, 0);
        assert!(!line.spans.is_empty());
    }
}
