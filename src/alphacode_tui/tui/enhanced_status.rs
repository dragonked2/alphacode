use crate::alphacode_tui::tui::brand_ux::{BrandTheme, ProgressBar};
use ratatui::prelude::*;
use std::time::Duration;
use unicode_width::UnicodeWidthStr;

/// Ring buffer for tracking recent token throughput.
///
/// Uses a fixed-capacity Vec that evicts from the front when full.
/// The O(n) front-eviction is acceptable because max_samples is small
/// (typically 20–50) and this runs on the render path, not a hot loop.
pub struct TokenHistory {
    values: Vec<u64>,
    max_samples: usize,
    peak: u64,
}

impl TokenHistory {
    pub fn new(max_samples: usize) -> Self {
        Self {
            values: Vec::with_capacity(max_samples),
            max_samples,
            peak: 0,
        }
    }

    pub fn push(&mut self, value: u64) {
        if self.values.len() >= self.max_samples {
            self.values.remove(0);
        }
        self.values.push(value);
        self.peak = self.peak.max(value);
    }

    pub fn values(&self) -> &[u64] {
        &self.values
    }

    pub fn peak(&self) -> u64 {
        self.peak
    }
}

/// Format a token count compactly: 1234 → "1.2k", 999 → "999", 1234567 → "1.2M".
///
/// This reduces status bar clutter and improves readability at a glance.
fn format_tokens(n: u64) -> String {
    if n >= 1_000_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else if n >= 1_000 {
        format!("{:.1}k", n as f64 / 1_000.0)
    } else {
        format!("{}", n)
    }
}

/// Format elapsed seconds compactly: 1.23 → "1.2s", 65.4 → "1m5s".
fn format_elapsed(d: Duration) -> String {
    let secs = d.as_secs_f32();
    if secs < 60.0 {
        format!("{:.1}s", secs)
    } else {
        let mins = d.as_secs() / 60;
        let rem = d.as_secs() % 60;
        format!("{}m{}s", mins, rem)
    }
}

/// Enhanced status bar with rich information display
pub struct StatusBar;

impl StatusBar {
    /// Render the main status bar at the bottom of the screen.
    ///
    /// Uses compact token formatting (1.2k instead of 1234) and
    /// elides the label when there are no tokens to show.
    pub fn render(
        model: &str,
        provider: &str,
        input_tokens: u64,
        output_tokens: u64,
        elapsed: Option<Duration>,
        status: &str,
        width: usize,
    ) -> Line<'static> {
        let mut spans = Vec::with_capacity(16);
        let sep = || Span::styled(" │ ", Style::default().fg(BrandTheme::dim()));

        // Brand marker with animated gradient dot
        let gradient = BrandTheme::gradient();
        spans.push(Span::styled(
            "●",
            Style::default()
                .fg(gradient[0])
                .add_modifier(Modifier::BOLD),
        ));

        // Model name (truncated if needed, Unicode display-width safe)
        let model_display = {
            let display_width = model.width();
            if display_width > 24 {
                let mut width = 0;
                let truncated: String = model
                    .chars()
                    .take_while(|c| {
                        width += unicode_width::UnicodeWidthChar::width(*c).unwrap_or(0);
                        width <= 22
                    })
                    .collect();
                format!("{}…", truncated)
            } else {
                model.to_string()
            }
        };
        spans.push(Span::styled(
            format!(" {}", model_display),
            Style::default()
                .fg(BrandTheme::model())
                .add_modifier(Modifier::BOLD),
        ));

        // Provider (only when there's room and it adds value)
        if !provider.is_empty() && width > 60 {
            spans.push(Span::styled(
                format!(" ({})", provider),
                Style::default().fg(BrandTheme::provider()),
            ));
        }

        spans.push(sep());

        // Token counts — compact formatting with directional color and icons
        if input_tokens > 0 || output_tokens > 0 {
            spans.push(Span::styled(
                "⬆",
                Style::default().fg(BrandTheme::success()),
            ));
            spans.push(Span::styled(
                format_tokens(input_tokens),
                Style::default().fg(gradient[1]),
            ));
            spans.push(Span::styled(" ", Style::default()));
            spans.push(Span::styled(
                "⬇",
                Style::default().fg(BrandTheme::warning()),
            ));
            spans.push(Span::styled(
                format_tokens(output_tokens),
                Style::default().fg(gradient[3]),
            ));
        }

        // Elapsed time with adaptive formatting
        if let Some(elapsed) = elapsed {
            spans.push(sep());
            let elapsed_str = format_elapsed(elapsed);
            let elapsed_color = if elapsed.as_secs() > 120 {
                BrandTheme::warning()
            } else {
                BrandTheme::accent()
            };
            spans.push(Span::styled(
                elapsed_str,
                Style::default().fg(elapsed_color),
            ));
        }

        // Status (only when meaningful and there's room)
        if !status.is_empty() && width > 50 {
            spans.push(sep());
            spans.push(Span::styled(
                status.to_string(),
                Style::default().fg(BrandTheme::warning()),
            ));
        }

        // Fill remaining space with a subtle gradient dotted line
        let used_width: usize = spans
            .iter()
            .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
            .sum();
        if used_width < width {
            let remaining = width - used_width;
            let fill: String = std::iter::repeat_n("· ", remaining / 2 + 1)
                .take(remaining)
                .collect();
            spans.push(Span::styled(fill, Style::default().fg(BrandTheme::dim())));
        }

        Line::from(spans)
    }

    /// Render a streaming status with sparkline activity graph.
    ///
    /// Shows: spinner · model · TPS (color-coded) · sparkline · total tokens · elapsed
    pub fn streaming_with_sparkline(
        model: &str,
        tokens_per_second: f32,
        total_tokens: u64,
        elapsed: Duration,
        history: &TokenHistory,
        width: usize,
    ) -> Line<'static> {
        let mut spans = Vec::with_capacity(14);

        // Spinner with gradient color based on speed
        let spinner_frame = (elapsed.as_millis() / 150) as usize;
        spans.extend(ProgressBar::spinner_smooth(spinner_frame));

        // Model
        spans.push(Span::styled(
            format!(" {}", model),
            Style::default()
                .fg(BrandTheme::model())
                .add_modifier(Modifier::BOLD),
        ));

        // TPS — color-coded by throughput tier with adaptive precision
        let tps_color = if tokens_per_second > 80.0 {
            BrandTheme::success()
        } else if tokens_per_second > 40.0 {
            BrandTheme::info()
        } else if tokens_per_second > 15.0 {
            BrandTheme::warning()
        } else {
            BrandTheme::error()
        };
        let tps_str = if tokens_per_second >= 100.0 {
            format!("{:.0}", tokens_per_second)
        } else {
            format!("{:.1}", tokens_per_second)
        };
        spans.push(Span::styled(
            format!(" · {} tok/s", tps_str),
            Style::default().fg(tps_color),
        ));

        // Sparkline of recent token throughput
        if !history.values().is_empty() {
            let spark_width = 16.min(width.saturating_sub(65));
            spans.push(Span::styled(" ", Style::default()));
            spans.extend(BrandTheme::sparkline(
                history.values(),
                history.peak(),
                spark_width,
            ));
        }

        // Total tokens (compact)
        spans.push(Span::styled(
            format!(" · {} tok", format_tokens(total_tokens)),
            Style::default().fg(BrandTheme::info()),
        ));

        // Elapsed (compact)
        spans.push(Span::styled(
            format!(" · {}", format_elapsed(elapsed)),
            Style::default().fg(BrandTheme::accent()),
        ));

        Line::from(spans)
    }

    /// Render a streaming progress indicator.
    pub fn streaming_status(
        model: &str,
        tokens_per_second: f32,
        total_tokens: u64,
        elapsed: Duration,
    ) -> Line<'static> {
        let mut spans = Vec::with_capacity(6);

        // Spinner
        let spinner_frame = (elapsed.as_millis() / 200) as usize;
        spans.extend(ProgressBar::spinner(spinner_frame));

        // Model
        spans.push(Span::styled(
            format!(" {}", model),
            Style::default()
                .fg(BrandTheme::model())
                .add_modifier(Modifier::BOLD),
        ));

        // TPS
        let tps_color = if tokens_per_second > 80.0 {
            BrandTheme::success()
        } else if tokens_per_second > 40.0 {
            BrandTheme::info()
        } else if tokens_per_second > 15.0 {
            BrandTheme::warning()
        } else {
            BrandTheme::error()
        };
        spans.push(Span::styled(
            format!(" · {:.1} tok/s", tokens_per_second),
            Style::default().fg(tps_color),
        ));

        // Total tokens (compact)
        spans.push(Span::styled(
            format!(" · {} tok", format_tokens(total_tokens)),
            Style::default().fg(BrandTheme::info()),
        ));

        // Elapsed (compact)
        spans.push(Span::styled(
            format!(" · {}", format_elapsed(elapsed)),
            Style::default().fg(BrandTheme::accent()),
        ));

        Line::from(spans)
    }

    /// Render a tool execution status with optional progress bar.
    pub fn tool_status(tool_name: &str, progress: Option<f32>) -> Line<'static> {
        let mut spans = Vec::with_capacity(6);

        // Tool icon with gradient color
        let icon_color = match progress {
            Some(p) if p >= 1.0 => BrandTheme::success(),
            Some(_) => BrandTheme::accent(),
            None => BrandTheme::tool(),
        };
        spans.push(Span::styled("⚙ ", Style::default().fg(icon_color)));

        // Tool name
        spans.push(Span::styled(
            tool_name.to_string(),
            Style::default()
                .fg(BrandTheme::tool())
                .add_modifier(Modifier::BOLD),
        ));

        // Progress bar if available
        if let Some(progress) = progress {
            spans.push(Span::styled(" ", Style::default()));
            spans.extend(ProgressBar::render_compact(progress, 12));
        }

        Line::from(spans)
    }

    /// Render connection status with latency color coding.
    ///
    /// Displays a connection indicator with tiered latency feedback:
    /// - ● <50ms green   (excellent)
    /// - ● <200ms blue   (good)
    /// - ● <500ms yellow (acceptable)
    /// - ● >=500ms red   (slow)
    /// - ○ red            (disconnected)
    pub fn connection_status(connected: bool, latency: Option<Duration>) -> Line<'static> {
        let mut spans = Vec::with_capacity(6);

        let (icon, color) = if connected {
            let indicator_color = match latency {
                Some(lat) => {
                    let lat_ms = lat.as_millis();
                    if lat_ms < 50 {
                        BrandTheme::success()
                    } else if lat_ms < 200 {
                        BrandTheme::info()
                    } else if lat_ms < 500 {
                        BrandTheme::warning()
                    } else {
                        BrandTheme::error()
                    }
                }
                None => BrandTheme::info(), // Unknown latency — don't signal "excellent"
            };
            ("●", indicator_color)
        } else {
            ("○", BrandTheme::error())
        };
        spans.push(Span::styled(
            format!("{} ", icon),
            Style::default().fg(color),
        ));

        // Latency with tier-based color and compact formatting
        if let Some(latency) = latency {
            let (lat_str, lat_color) = match latency.as_millis() {
                0..=49 => (format!("{}ms", latency.as_millis()), BrandTheme::success()),
                50..=199 => (format!("{}ms", latency.as_millis()), BrandTheme::info()),
                200..=999 => (format!("{}ms", latency.as_millis()), BrandTheme::warning()),
                _ => {
                    let secs = latency.as_secs_f32();
                    (format!("{:.1}s", secs), BrandTheme::error())
                }
            };
            spans.push(Span::styled(lat_str, Style::default().fg(lat_color)));
        }

        // Connection label with subtle styling
        if connected {
            spans.push(Span::styled(
                " live",
                Style::default()
                    .fg(BrandTheme::dim_bright())
                    .add_modifier(Modifier::DIM),
            ));
        } else {
            spans.push(Span::styled(
                " disconnected",
                Style::default().fg(BrandTheme::error()),
            ));
        }

        Line::from(spans)
    }

    /// Render a compact connection dot — just the indicator, suitable for
    /// tight layouts where the full status line would overflow.
    pub fn connection_dot(connected: bool, latency: Option<Duration>) -> Span<'static> {
        let (icon, color) = if connected {
            let c = match latency {
                Some(lat) => {
                    let ms = lat.as_millis();
                    if ms < 50 {
                        BrandTheme::success()
                    } else if ms < 200 {
                        BrandTheme::info()
                    } else if ms < 500 {
                        BrandTheme::warning()
                    } else {
                        BrandTheme::error()
                    }
                }
                None => BrandTheme::info(),
            };
            ("●", c)
        } else {
            ("○", BrandTheme::error())
        };
        Span::styled(
            icon.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )
    }

    /// Render a connection HUD with latency graph and status indicators.
    ///
    /// Shows: connection dot · latency graph · latency value · connection type
    /// The latency graph shows the last N latency readings as a sparkline.
    pub fn connection_hud(
        connected: bool,
        latency: Option<Duration>,
        latency_history: &[Duration],
        connection_type: &str,
        width: usize,
    ) -> Line<'static> {
        let mut spans = Vec::with_capacity(12);

        // Connection dot with glow effect
        let dot = Self::connection_dot(connected, latency);
        spans.push(dot);
        spans.push(Span::styled(" ", Style::default()));

        // Latency graph (sparkline of recent latency readings)
        if !latency_history.is_empty() && width > 30 {
            let graph_width = 12.min(width.saturating_sub(40));
            let values: Vec<u64> = latency_history
                .iter()
                .map(|d| d.as_millis() as u64)
                .collect();
            let max_val = values.iter().copied().max().unwrap_or(100).max(1);

            // Color code the graph based on average latency
            let avg_ms = values.iter().sum::<u64>() as f32 / values.len() as f32;
            let graph_color = if avg_ms < 50.0 {
                BrandTheme::success()
            } else if avg_ms < 200.0 {
                BrandTheme::info()
            } else if avg_ms < 500.0 {
                BrandTheme::warning()
            } else {
                BrandTheme::error()
            };

            // Build sparkline
            const BLOCKS: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];
            let step = (values.len() as f32 / graph_width as f32).max(1.0);
            let mut graph_str = String::with_capacity(graph_width);
            for i in 0..graph_width {
                let idx = (i as f32 * step) as usize;
                let val = values[idx.min(values.len() - 1)];
                let normalized = (val as f32 / max_val as f32).clamp(0.0, 1.0);
                let block_idx = (normalized * 7.0).round() as usize;
                graph_str.push_str(BLOCKS[block_idx]);
            }
            spans.push(Span::styled(graph_str, Style::default().fg(graph_color)));
            spans.push(Span::styled(" ", Style::default()));
        }

        // Latency value with tier color
        if let Some(latency) = latency {
            let (lat_str, lat_color) = match latency.as_millis() {
                0..=49 => (format!("{}ms", latency.as_millis()), BrandTheme::success()),
                50..=199 => (format!("{}ms", latency.as_millis()), BrandTheme::info()),
                200..=999 => (format!("{}ms", latency.as_millis()), BrandTheme::warning()),
                _ => {
                    let secs = latency.as_secs_f32();
                    (format!("{:.1}s", secs), BrandTheme::error())
                }
            };
            spans.push(Span::styled(lat_str, Style::default().fg(lat_color)));
            spans.push(Span::styled(" ", Style::default()));
        }

        // Connection type with icon
        let (type_icon, type_color) = match connection_type.to_lowercase().as_str() {
            "websocket" | "ws" => ("🔌", BrandTheme::info()),
            "stdio" | "subprocess" | "cli" => ("⚙", BrandTheme::tool()),
            "tcp" => ("📡", BrandTheme::accent()),
            _ => ("🔗", BrandTheme::dim_bright()),
        };
        spans.push(Span::styled(
            format!("{} {}", type_icon, connection_type),
            Style::default().fg(type_color),
        ));

        Line::from(spans)
    }

    /// Render a thinking indicator with animated dots and model name.
    pub fn thinking_indicator(model: &str, elapsed: Duration) -> Line<'static> {
        let mut spans = Vec::with_capacity(6);

        // Animated thinking dots
        let phase = (elapsed.as_millis() / 200) as usize % 6;
        let dots = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴"];
        spans.push(Span::styled(
            dots[phase],
            Style::default()
                .fg(BrandTheme::accent())
                .add_modifier(Modifier::BOLD),
        ));

        // Model name
        spans.push(Span::styled(
            format!(" {}", model),
            Style::default()
                .fg(BrandTheme::model())
                .add_modifier(Modifier::BOLD),
        ));

        // Status text
        spans.push(Span::styled(
            " thinking",
            Style::default()
                .fg(BrandTheme::dim_bright())
                .add_modifier(Modifier::ITALIC),
        ));

        // Elapsed time
        if elapsed.as_secs() > 1 {
            spans.push(Span::styled(
                format!(" · {}", format_elapsed(elapsed)),
                Style::default().fg(BrandTheme::warning()),
            ));
        }

        Line::from(spans)
    }

    /// Render an error status with retry information.
    pub fn error_status(
        error_msg: &str,
        retry_count: Option<u32>,
        max_retries: Option<u32>,
    ) -> Line<'static> {
        let mut spans = Vec::with_capacity(8);

        // Error icon with pulsing effect
        spans.push(Span::styled(
            "✗ ",
            Style::default()
                .fg(BrandTheme::error())
                .add_modifier(Modifier::BOLD),
        ));

        // Error message (truncated if too long)
        let max_msg_width = 40;
        let msg_display = if error_msg.len() > max_msg_width {
            format!("{}…", &error_msg[..max_msg_width])
        } else {
            error_msg.to_string()
        };
        spans.push(Span::styled(
            msg_display,
            Style::default().fg(BrandTheme::error()),
        ));

        // Retry information
        if let (Some(count), Some(max)) = (retry_count, max_retries) {
            spans.push(Span::styled(
                format!(" (retry {}/{})", count, max),
                Style::default()
                    .fg(BrandTheme::warning())
                    .add_modifier(Modifier::ITALIC),
            ));
        }

        Line::from(spans)
    }

    /// Render a success status with checkmark.
    pub fn success_status(message: &str) -> Line<'static> {
        let mut spans = Vec::with_capacity(4);

        spans.push(Span::styled(
            "✓ ",
            Style::default()
                .fg(BrandTheme::success())
                .add_modifier(Modifier::BOLD),
        ));

        spans.push(Span::styled(
            message.to_string(),
            Style::default().fg(BrandTheme::success()),
        ));

        Line::from(spans)
    }
}

/// Compact status badge for inline display
pub struct StatusBadge;

impl StatusBadge {
    /// Render a colored badge with subtle background emphasis
    pub fn render(text: &str, color: Color) -> Span<'static> {
        Span::styled(
            format!(" {} ", text),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )
    }

    /// Render a success badge
    pub fn success(text: &str) -> Span<'static> {
        Self::render(text, BrandTheme::success())
    }

    /// Render a warning badge
    pub fn warning(text: &str) -> Span<'static> {
        Self::render(text, BrandTheme::warning())
    }

    /// Render an error badge
    pub fn error(text: &str) -> Span<'static> {
        Self::render(text, BrandTheme::error())
    }

    /// Render an info badge
    pub fn info(text: &str) -> Span<'static> {
        Self::render(text, BrandTheme::info())
    }

    /// Render a dim/muted badge for secondary information
    pub fn dim(text: &str) -> Span<'static> {
        Span::styled(
            format!(" {} ", text),
            Style::default()
                .fg(BrandTheme::dim())
                .add_modifier(Modifier::DIM),
        )
    }

    /// Render a gradient badge that sweeps through brand colors
    pub fn gradient(text: &str) -> Vec<Span<'static>> {
        BrandTheme::gradient_spans(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_bar_render() {
        let line = StatusBar::render(
            "claude-3-opus",
            "anthropic",
            1000,
            500,
            Some(Duration::from_secs(2)),
            "streaming",
            80,
        );
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_status_bar_compact_tokens() {
        let line = StatusBar::render("model", "p", 1500, 1234567, None, "", 80);
        // Token counts should be compact (1.5k, 1.2M) not raw numbers
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(
            text.contains("1.5k"),
            "expected compact token format, got: {}",
            text
        );
        assert!(
            text.contains("1.2M"),
            "expected compact token format, got: {}",
            text
        );
    }

    #[test]
    fn test_streaming_status() {
        let line = StatusBar::streaming_status("claude-3-opus", 45.5, 1234, Duration::from_secs(3));
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_streaming_with_sparkline() {
        let mut history = TokenHistory::new(20);
        for i in 0..10 {
            history.push((i * 10) as u64);
        }
        let line = StatusBar::streaming_with_sparkline(
            "claude-3-opus",
            45.5,
            1234,
            Duration::from_secs(3),
            &history,
            80,
        );
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_token_history_ring_buffer() {
        let mut history = TokenHistory::new(5);
        for i in 0..10 {
            history.push(i);
        }
        assert_eq!(history.values().len(), 5);
        assert_eq!(history.peak(), 9);
        assert_eq!(history.values(), &[5, 6, 7, 8, 9]);
    }

    #[test]
    fn test_format_tokens() {
        assert_eq!(format_tokens(0), "0");
        assert_eq!(format_tokens(999), "999");
        assert_eq!(format_tokens(1000), "1.0k");
        assert_eq!(format_tokens(1234), "1.2k");
        assert_eq!(format_tokens(999999), "1000.0k");
        assert_eq!(format_tokens(1000000), "1.0M");
        assert_eq!(format_tokens(1234567), "1.2M");
    }

    #[test]
    fn test_format_elapsed() {
        assert_eq!(format_elapsed(Duration::from_millis(500)), "0.5s");
        assert_eq!(format_elapsed(Duration::from_secs(5)), "5.0s");
        assert_eq!(format_elapsed(Duration::from_secs(65)), "1m5s");
        assert_eq!(format_elapsed(Duration::from_secs(3600)), "60m0s");
    }

    #[test]
    fn test_tool_status() {
        let line = StatusBar::tool_status("bash", Some(0.5));
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_status_badge() {
        let span = StatusBadge::success("ok");
        assert!(!span.content.is_empty());
    }

    #[test]
    fn test_connection_hud() {
        let latency_history = vec![
            Duration::from_millis(30),
            Duration::from_millis(45),
            Duration::from_millis(60),
        ];
        let line = StatusBar::connection_hud(
            true,
            Some(Duration::from_millis(45)),
            &latency_history,
            "websocket",
            80,
        );
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_thinking_indicator() {
        let line = StatusBar::thinking_indicator("claude-3-opus", Duration::from_secs(2));
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_error_status() {
        let line = StatusBar::error_status("Connection failed", Some(2), Some(3));
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_success_status() {
        let line = StatusBar::success_status("Task completed");
        assert!(!line.spans.is_empty());
    }
}
