use crate::alphacode_tui::tui::brand_ux::{BrandTheme, SpinnerStyle};
use crate::alphacode_tui::tui::color_support::rgb;
use ratatui::prelude::*;

/// Enhanced input renderer with better visual feedback.
///
/// Renders the user input area with:
/// - A framed box with mode-aware gradient borders
/// - A mode indicator line showing current input mode
/// - A breathing cursor block that softly pulses while idle, so the input
///   always reads as "ready to type" even when no character has been pressed
/// - Proper Unicode-safe cursor positioning
/// - Contextual help hints below the input, with a tiny typed-byte counter
pub struct EnhancedInput;

impl EnhancedInput {
    /// Render the input area with rich formatting.
    ///
    /// Returns 4–5 lines: mode indicator, top border, input, bottom border,
    /// and (when idle) a help-hints line.
    pub fn render(
        input: &str,
        cursor_pos: usize,
        width: usize,
        is_processing: bool,
        mode: InputMode,
    ) -> Vec<Line<'static>> {
        Self::render_with_pulse(input, cursor_pos, width, is_processing, mode, 0.0)
    }

    /// Like [`render`](Self::render), but with an explicit breathing-pulse
    /// phase in [0, 1]. The cursor block and the corner glow key off this so
    /// the input feels alive even when nothing else is moving.
    pub fn render_with_pulse(
        input: &str,
        cursor_pos: usize,
        width: usize,
        is_processing: bool,
        mode: InputMode,
        pulse: f32,
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::with_capacity(5);

        // Mode indicator
        lines.push(Self::render_mode_indicator(mode, is_processing));

        // Top border with gradient
        lines.push(Self::render_border(width, mode, true, pulse));

        // Input text with cursor
        lines.push(Self::render_input_with_cursor(
            input,
            cursor_pos,
            width,
            is_processing,
            mode,
            pulse,
        ));

        // Bottom border with gradient
        lines.push(Self::render_border(width, mode, false, pulse));

        // Help hints (only when idle)
        if !is_processing {
            lines.push(Self::render_help_hints(mode, input));
        }

        lines
    }

    /// Render a gradient border line (top or bottom) with mode-aware coloring.
    ///
    /// Uses pre-allocated spans to avoid per-character allocation.
    /// The top border is slightly brighter than the bottom to create a
    /// subtle 3D framing effect. The corner glyphs pick up the `pulse` so the
    /// active input "breathes" softly at idle.
    fn render_border(width: usize, _mode: InputMode, is_top: bool, pulse: f32) -> Line<'static> {
        let (corner_l, corner_r) = if is_top {
            ("╭", "╮")
        } else {
            ("╰", "╯")
        };
        let border_width = width.min(80);
        let content_width = border_width.saturating_sub(2);
        let intensity = if is_top { 1.0 } else { 0.5 };
        // Pulse 0..1 -> 0.85..1.15 brightness multiplier for the corners.
        let corner_boost = 0.85 + 0.30 * pulse.clamp(0.0, 1.0);

        let gradient = BrandTheme::gradient();
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(4);

        // Left corner — colored with gradient start, gently brightened on pulse
        spans.push(Span::styled(
            corner_l,
            Style::default()
                .fg(Self::scale_color(gradient[0], corner_boost))
                .add_modifier(if pulse > 0.5 {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ));

        // Horizontal bar — batch into gradient segments (one span per color)
        let seg_count = gradient.len().min(content_width);
        let seg_size = content_width / seg_count;
        let remainder = content_width - seg_size * seg_count;
        for seg in 0..seg_count {
            let n = seg_size + if seg < remainder { 1 } else { 0 };
            if n == 0 {
                continue;
            }
            let color = gradient[seg % gradient.len()];
            // Apply intensity dimming
            let (r, g, b) = match color {
                Color::Rgb(r, g, b) => (r as f32, g as f32, b as f32),
                _ => (100.0, 100.0, 100.0),
            };
            let dimmed = rgb(
                (r * intensity) as u8,
                (g * intensity) as u8,
                (b * intensity) as u8,
            );
            spans.push(Span::styled(
                std::iter::repeat_n("─", n).collect::<String>(),
                Style::default().fg(dimmed),
            ));
        }

        // Right corner — colored with gradient end
        spans.push(Span::styled(
            corner_r,
            Style::default()
                .fg(Self::scale_color(
                    gradient[gradient.len() - 1],
                    corner_boost,
                ))
                .add_modifier(if pulse > 0.5 {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ));

        Line::from(spans)
    }

    /// Multiply an RGB color by a scalar, clamping the result. Returns the
    /// input unchanged for non-RGB colors.
    #[inline]
    fn scale_color(color: Color, factor: f32) -> Color {
        match color {
            Color::Rgb(r, g, b) => {
                let r = (r as f32 * factor).clamp(0.0, 255.0) as u8;
                let g = (g as f32 * factor).clamp(0.0, 255.0) as u8;
                let b = (b as f32 * factor).clamp(0.0, 255.0) as u8;
                rgb(r, g, b)
            }
            other => other,
        }
    }

    /// Render mode indicator line with gradient background.
    ///
    /// Shows the current input mode (chat/shell/command/search) with a
    /// brand marker and contextual keyboard hints.
    fn render_mode_indicator(mode: InputMode, is_processing: bool) -> Line<'static> {
        let mut spans = Vec::with_capacity(8);

        // Brand marker with gradient animation
        spans.push(Span::styled(
            "◆",
            Style::default()
                .fg(BrandTheme::gradient_color(0))
                .add_modifier(Modifier::BOLD),
        ));

        // Mode badge — icon + text with mode-specific color
        let (icon, mode_text, mode_color) = match mode {
            InputMode::Chat => (" 💬 ", "chat", BrandTheme::accent()),
            InputMode::Shell => (" ⚡ ", "shell", BrandTheme::success()),
            InputMode::Command => (" ⌘ ", "command", BrandTheme::warning()),
            InputMode::Search => (" 🔍 ", "search", BrandTheme::info()),
        };

        spans.push(Span::styled(icon, Style::default().fg(mode_color)));
        spans.push(Span::styled(
            mode_text,
            Style::default().fg(mode_color).add_modifier(Modifier::BOLD),
        ));

        if is_processing {
            spans.push(Span::styled(
                " · thinking",
                Style::default()
                    .fg(BrandTheme::warning())
                    .add_modifier(Modifier::ITALIC),
            ));
        }

        // Dim separator before hints
        spans.push(Span::styled("  │ ", Style::default().fg(BrandTheme::dim())));

        // Contextual hints
        spans.push(Span::styled(
            "Esc ",
            Style::default()
                .fg(BrandTheme::accent())
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            "commands  ",
            Style::default().fg(BrandTheme::dim()),
        ));
        spans.push(Span::styled(
            "↑↓ ",
            Style::default()
                .fg(BrandTheme::accent())
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            "history",
            Style::default().fg(BrandTheme::dim()),
        ));

        Line::from(spans)
    }

    /// Render input text with a properly positioned cursor.
    ///
    /// The cursor is placed at `cursor_pos` (byte offset into `input`).
    /// Unicode text is sliced by character, not by byte, so multi-byte
    /// characters are never split. When the input exceeds the available
    /// width, a window around the cursor is shown with ellipsis markers.
    ///
    /// The cursor block picks up the `pulse` so it breathes softly when the
    /// input is idle; while the agent is processing it is replaced with a
    /// `SpinnerStyle::Braille` glyph so the user sees an in-flight indicator
    /// inside the input box itself.
    fn render_input_with_cursor(
        input: &str,
        cursor_pos: usize,
        width: usize,
        is_processing: bool,
        mode: InputMode,
        pulse: f32,
    ) -> Line<'static> {
        let mut spans = Vec::with_capacity(4);
        let prefix_width = 2; // "▸ " / "⊞ " / etc.
        let content_width = width.saturating_sub(prefix_width);

        // Mode-aware prefix icon
        let (prefix, prefix_color) = match mode {
            InputMode::Shell => ("⊞ ", BrandTheme::success()),
            InputMode::Command => ("⌘ ", BrandTheme::warning()),
            InputMode::Search => ("🔍 ", BrandTheme::info()),
            InputMode::Chat => ("▸ ", BrandTheme::accent()),
        };
        spans.push(Span::styled(
            prefix,
            Style::default()
                .fg(if is_processing {
                    BrandTheme::warning()
                } else {
                    prefix_color
                })
                .add_modifier(Modifier::BOLD),
        ));

        if input.is_empty() {
            // Placeholder text
            let placeholder = if is_processing {
                "thinking..."
            } else {
                match mode {
                    InputMode::Chat => "type a message...",
                    InputMode::Shell => "enter shell command...",
                    InputMode::Command => "type / for commands...",
                    InputMode::Search => "search prompt history...",
                }
            };
            spans.push(Span::styled(
                placeholder,
                Style::default()
                    .fg(BrandTheme::dim())
                    .add_modifier(Modifier::ITALIC),
            ));
        } else {
            // Convert input to a char-indexed slice for safe truncation
            let chars: Vec<char> = input.chars().collect();
            let char_count = chars.len();

            // Map cursor_pos (byte offset) to char index
            let cursor_char = input[..cursor_pos.min(input.len())].chars().count();

            // Determine the visible window of characters
            let mut visible_start;
            let mut visible_end;
            let total_display_width: usize = chars
                .iter()
                .map(|c| unicode_width::UnicodeWidthChar::width(*c).unwrap_or(0))
                .sum();

            if total_display_width <= content_width.saturating_sub(1) {
                // Everything fits — show all characters
                visible_start = 0;
                visible_end = char_count;
            } else if cursor_char < content_width / 2 {
                // Cursor near the left — show from the start, clip right
                visible_start = 0;
                // Find how many chars fit
                let mut used_w = 0;
                visible_end = char_count;
                for (i, c) in chars.iter().enumerate() {
                    let cw = unicode_width::UnicodeWidthChar::width(*c).unwrap_or(0);
                    if used_w + cw > content_width.saturating_sub(1) {
                        visible_end = i;
                        break;
                    }
                    used_w += cw;
                }
            } else if cursor_char + content_width / 2 >= char_count {
                // Cursor near the right — show from near the end, clip left
                visible_end = char_count;
                let mut used_w = 0;
                visible_start = 0;
                for (i, c) in chars.iter().enumerate().rev() {
                    let cw = unicode_width::UnicodeWidthChar::width(*c).unwrap_or(0);
                    if used_w + cw > content_width.saturating_sub(1) {
                        visible_start = i + 1;
                        break;
                    }
                    used_w += cw;
                }
            } else {
                // Cursor in the middle — center the window
                let half = content_width / 2;
                visible_start = cursor_char.saturating_sub(half);
                visible_end = (cursor_char + half).min(char_count);
            }

            // Leading ellipsis if clipped from left
            if visible_start > 0 {
                spans.push(Span::styled("…", Style::default().fg(BrandTheme::dim())));
            }

            // Visible characters
            let display_color = rgb(220, 220, 220);
            let mut visible_text = String::with_capacity(content_width * 3);
            for c in &chars[visible_start..visible_end] {
                visible_text.push(*c);
            }
            spans.push(Span::styled(
                visible_text,
                Style::default().fg(display_color),
            ));

            // Trailing ellipsis if clipped from right
            if visible_end < char_count {
                spans.push(Span::styled("…", Style::default().fg(BrandTheme::dim())));
            }

            // Cursor block (breathing) or in-flight spinner glyph
            if is_processing {
                // Render one frame of the braille spinner so the input reads
                // as "still working" instead of a frozen bar.
                let frame = (pulse * 10.0) as usize;
                let (frames, _divisor) = SpinnerStyle::Braille.frames();
                let idx = frame % frames.len();
                let spinner_color = BrandTheme::gradient_color(idx + SpinnerStyle::Braille.bias());
                spans.push(Span::styled(
                    frames[idx],
                    Style::default()
                        .fg(spinner_color)
                        .add_modifier(Modifier::BOLD),
                ));
            } else {
                // Soft breathing cursor: brighter when pulse is high.
                let cursor_color = match mode {
                    InputMode::Shell => BrandTheme::success(),
                    InputMode::Command => BrandTheme::warning(),
                    InputMode::Search => BrandTheme::info(),
                    InputMode::Chat => BrandTheme::accent(),
                };
                let boost = 0.7 + 0.5 * pulse.clamp(0.0, 1.0);
                let boosted = Self::scale_color(cursor_color, boost);
                spans.push(Span::styled("█", Style::default().fg(boosted)));
            }
        }

        Line::from(spans)
    }

    /// Render contextual help hints below the input box.
    ///
    /// Each mode shows only the relevant keyboard shortcuts, styled
    /// with the mode's accent color for bold keys and dim for labels.
    /// A small typed-character counter sits at the right edge so users
    /// can see how much they have written without leaving the input.
    fn render_help_hints(mode: InputMode, input: &str) -> Line<'static> {
        let dim = BrandTheme::dim();
        let bright = BrandTheme::dim_bright();

        let mut hints: Vec<Span<'static>> = match mode {
            InputMode::Chat => vec![
                Span::styled(
                    "  ↵ ",
                    Style::default()
                        .fg(BrandTheme::success())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("send", Style::default().fg(dim)),
                Span::styled("  │  ", Style::default().fg(BrandTheme::dim())),
                Span::styled("⇧↵ ", Style::default().fg(bright)),
                Span::styled("newline", Style::default().fg(dim)),
                Span::styled("  │  ", Style::default().fg(BrandTheme::dim())),
                Span::styled("⌃L ", Style::default().fg(bright)),
                Span::styled("clear", Style::default().fg(dim)),
            ],
            InputMode::Shell => vec![
                Span::styled(
                    "  ↵ ",
                    Style::default()
                        .fg(BrandTheme::success())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("execute", Style::default().fg(dim)),
                Span::styled("  │  ", Style::default().fg(BrandTheme::dim())),
                Span::styled("Esc ", Style::default().fg(bright)),
                Span::styled("cancel", Style::default().fg(dim)),
                Span::styled("  │  ", Style::default().fg(BrandTheme::dim())),
                Span::styled("↑↓ ", Style::default().fg(bright)),
                Span::styled("history", Style::default().fg(dim)),
            ],
            InputMode::Command => vec![
                Span::styled("  ↑↓ ", Style::default().fg(bright)),
                Span::styled("navigate", Style::default().fg(dim)),
                Span::styled("  │  ", Style::default().fg(BrandTheme::dim())),
                Span::styled(
                    "↵ ",
                    Style::default()
                        .fg(BrandTheme::success())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("select", Style::default().fg(dim)),
                Span::styled("  │  ", Style::default().fg(BrandTheme::dim())),
                Span::styled("Esc ", Style::default().fg(bright)),
                Span::styled("cancel", Style::default().fg(dim)),
            ],
            InputMode::Search => vec![
                Span::styled(
                    "  ↵ ",
                    Style::default()
                        .fg(BrandTheme::success())
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled("search", Style::default().fg(dim)),
                Span::styled("  │  ", Style::default().fg(BrandTheme::dim())),
                Span::styled("Esc ", Style::default().fg(bright)),
                Span::styled("cancel", Style::default().fg(dim)),
            ],
        };

        // Tiny byte counter on the right; only when there is something to count.
        if !input.is_empty() {
            let bytes = input.len();
            let label = if bytes == 1 { "byte" } else { "bytes" };
            hints.push(Span::styled("    ", Style::default().fg(dim)));
            hints.push(Span::styled(
                format!("{bytes} {label}"),
                Style::default()
                    .fg(BrandTheme::dim())
                    .add_modifier(Modifier::ITALIC),
            ));
        }

        Line::from(hints)
    }
}

/// Input mode
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    Chat,
    Shell,
    Command,
    Search,
}

/// Enhanced output formatter for assistant responses.
///
/// Provides lightweight line-level styling detection: code fences,
/// headings, list items, blockquotes, and plain text are each
/// rendered with distinct colors without requiring a full markdown
/// parser.
pub struct EnhancedOutput;

impl EnhancedOutput {
    /// Format a multi-line response with per-line styling.
    pub fn format_response(text: &str, _width: usize) -> Vec<Line<'static>> {
        text.lines().map(Self::format_line).collect()
    }

    /// Format a single output line with context-aware styling.
    fn format_line(text: &str) -> Line<'static> {
        let (content, style) = if text.starts_with("```") {
            (
                text,
                Style::default()
                    .fg(BrandTheme::accent())
                    .add_modifier(Modifier::BOLD),
            )
        } else if text.starts_with("# ") {
            (
                text,
                Style::default()
                    .fg(BrandTheme::warning())
                    .add_modifier(Modifier::BOLD),
            )
        } else if let Some(rest) = text.strip_prefix("- ").or_else(|| text.strip_prefix("* ")) {
            return Line::from(vec![
                Span::styled("• ", Style::default().fg(BrandTheme::accent())),
                Span::styled(rest.to_string(), Style::default().fg(rgb(220, 220, 220))),
            ]);
        } else if let Some(rest) = text.strip_prefix("> ") {
            return Line::from(vec![
                Span::styled("▸ ", Style::default().fg(BrandTheme::dim())),
                Span::styled(
                    rest.to_string(),
                    Style::default()
                        .fg(BrandTheme::dim_bright())
                        .add_modifier(Modifier::ITALIC),
                ),
            ]);
        } else {
            (text, Style::default().fg(rgb(220, 220, 220)))
        };
        Line::from(Span::styled(content.to_string(), style))
    }
}

/// Copy success/failure badge rendered as a single styled line.
pub struct CopyBadge;

impl CopyBadge {
    /// Render a copy-success badge: ✓ copied: <label>
    pub fn success(text: &str) -> Line<'static> {
        Line::from(vec![
            Span::styled(
                " ✓ ",
                Style::default()
                    .fg(BrandTheme::success())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("copied: {}", text),
                Style::default().fg(BrandTheme::dim_bright()),
            ),
        ])
    }

    /// Render a copy-failure badge: ✗ copy failed: <label>
    pub fn error(text: &str) -> Line<'static> {
        Line::from(vec![
            Span::styled(
                " ✗ ",
                Style::default()
                    .fg(BrandTheme::error())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("copy failed: {}", text),
                Style::default().fg(BrandTheme::error()),
            ),
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enhanced_input_render() {
        let lines = EnhancedInput::render("hello", 5, 80, false, InputMode::Chat);
        assert!(!lines.is_empty());
        // Should have: mode indicator, top border, input, bottom border, help hints
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_enhanced_input_processing() {
        let lines = EnhancedInput::render("", 0, 80, true, InputMode::Chat);
        assert!(!lines.is_empty());
        // Processing: no help hints line
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn test_enhanced_input_unicode_safe() {
        // Unicode characters should not be split
        let input = "こんにちは世界";
        let lines = EnhancedInput::render(input, input.len(), 40, false, InputMode::Chat);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_enhanced_input_narrow_terminal() {
        // Even on a 20-col terminal, input should render without panicking
        let lines =
            EnhancedInput::render("a long message that wraps", 25, 20, false, InputMode::Chat);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_enhanced_input_all_modes() {
        for mode in [
            InputMode::Chat,
            InputMode::Shell,
            InputMode::Command,
            InputMode::Search,
        ] {
            let lines = EnhancedInput::render("test", 4, 80, false, mode);
            assert!(!lines.is_empty());
        }
    }

    #[test]
    fn test_enhanced_output_format() {
        let lines = EnhancedOutput::format_response("# Hello\n- item\n> quote", 80);
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_copy_badge_success() {
        let line = CopyBadge::success("selection");
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_copy_badge_error() {
        let line = CopyBadge::error("clipboard busy");
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_enhanced_input_with_pulse() {
        let lines = EnhancedInput::render_with_pulse("hi", 2, 80, false, InputMode::Chat, 0.5);
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_enhanced_input_processing_shows_spinner() {
        let lines = EnhancedInput::render_with_pulse("hi", 2, 80, true, InputMode::Chat, 1.5);
        // The input line should now contain a spinner glyph from the Braille set
        let spinner_seen = lines.iter().any(|line| {
            line.spans.iter().any(|s| {
                matches!(
                    s.content.as_ref(),
                    "⠋" | "⠙" | "⠹" | "⠸" | "⠼" | "⠴" | "⠦" | "⠧" | "⠇" | "⠏"
                )
            })
        });
        assert!(
            spinner_seen,
            "processing input should render a braille spinner glyph"
        );
    }

    #[test]
    fn test_help_hints_include_byte_counter() {
        let lines = EnhancedInput::render("hello world", 11, 80, false, InputMode::Chat);
        // The help-hints line should mention "11 bytes" because the input has 11 bytes.
        let counter_seen = lines.iter().any(|line| {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            text.contains("11 bytes")
        });
        assert!(
            counter_seen,
            "help hints should include a byte counter when the input is non-empty"
        );
    }

    #[test]
    fn test_help_hints_skip_byte_counter_when_empty() {
        let lines = EnhancedInput::render("", 0, 80, false, InputMode::Chat);
        let counter_seen = lines.iter().any(|line| {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            text.contains("bytes")
        });
        assert!(
            !counter_seen,
            "help hints should not mention bytes for an empty input"
        );
    }
}
