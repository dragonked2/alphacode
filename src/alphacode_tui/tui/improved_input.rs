use crate::alphacode_tui::tui::brand_ux::{BrandTheme, SpinnerStyle};
use crate::alphacode_tui::tui::color_support::rgb;
use ratatui::prelude::*;

/// Enhanced input renderer with better visual feedback.
///
/// Renders the user input area with:
/// - A framed box with mode-aware gradient borders
/// - A mode indicator line showing current input mode
/// - A breathing cursor block that softly pulses while idle
/// - Proper Unicode-safe cursor positioning
/// - Contextual help hints with typed-byte counter
/// - Input validation feedback with inline error display
/// - Auto-complete suggestions rendered inline
/// - Input masking support (for passwords/secrets)
pub struct EnhancedInput;

/// Validation result for user input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationResult {
    /// Whether the input is valid.
    pub valid: bool,
    /// Error message if invalid.
    pub error: Option<String>,
    /// Warning message (non-blocking).
    pub warning: Option<String>,
}

impl ValidationResult {
    /// Create a valid validation result.
    pub fn valid() -> Self {
        Self {
            valid: true,
            error: None,
            warning: None,
        }
    }

    /// Create an invalid validation result with an error.
    pub fn error(error: impl Into<String>) -> Self {
        Self {
            valid: false,
            error: Some(error.into()),
            warning: None,
        }
    }

    /// Create a validation result with a warning.
    pub fn warning(warning: impl Into<String>) -> Self {
        Self {
            valid: true,
            error: None,
            warning: Some(warning.into()),
        }
    }
}

/// Auto-complete suggestion.
#[derive(Debug, Clone)]
pub struct AutoComplete {
    /// The suggested text.
    pub text: String,
    /// Short description shown alongside the suggestion.
    pub description: Option<String>,
    /// Whether this is the top match.
    pub primary: bool,
}

/// Input validation configuration.
#[derive(Debug, Clone, Default)]
pub struct InputConfig {
    /// Maximum input length. None for unlimited.
    pub max_length: Option<usize>,
    /// Minimum input length. None for no minimum.
    pub min_length: Option<usize>,
    /// Allowed character pattern description (for error messages).
    pub allowed_chars: Option<&'static str>,
    /// Whether the input is secret (masks input with •).
    pub secret: bool,
    /// Placeholder text when empty and not processing.
    pub placeholder: Option<&'static str>,
    /// Validation function.
    pub validate: Option<fn(&str) -> ValidationResult>,
}

/// Auto-complete result.
#[derive(Debug, Clone)]
pub struct AutoCompleteResult {
    /// The auto-completed text (full suggestion).
    pub completed: String,
    /// The remaining suffix the user still needs to type.
    pub suffix: String,
    /// All matching suggestions.
    pub matches: Vec<AutoComplete>,
    /// Index of the selected match.
    pub selected: usize,
}

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
        Self::render_with_config(
            input,
            cursor_pos,
            width,
            is_processing,
            mode,
            pulse,
            InputConfig::default(),
            None,
        )
    }

    /// Render the input area with full configuration including validation
    /// and auto-complete.
    // Render config API takes one arg per option; keep the explicit signature.
    #[allow(clippy::too_many_arguments)]
    pub fn render_with_config(
        input: &str,
        cursor_pos: usize,
        width: usize,
        is_processing: bool,
        mode: InputMode,
        pulse: f32,
        config: InputConfig,
        autocomplete: Option<&AutoCompleteResult>,
    ) -> Vec<Line<'static>> {
        let mut lines = Vec::with_capacity(6);

        // Mode indicator
        lines.push(Self::render_mode_indicator(mode, is_processing));

        // Validation feedback line (shown when there's an error and not processing)
        if !is_processing
            && let Some(validation) = config.validate.and_then(|f| {
                let r = f(input);
                if !r.valid || r.warning.is_some() {
                    Some(r)
                } else {
                    None
                }
            })
        {
            lines.push(Self::render_validation(&validation));
        }

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
            &config,
        ));

        // Bottom border with gradient
        lines.push(Self::render_border(width, mode, false, pulse));

        // Auto-complete bar (shown when there are suggestions)
        if let Some(ac) = autocomplete
            && !ac.matches.is_empty()
            && !is_processing
        {
            lines.push(Self::render_autocomplete(ac, width));
        }

        // Help hints (only when idle)
        if !is_processing {
            lines.push(Self::render_help_hints(mode, input));
        }

        lines
    }

    /// Validate input and return the result. Returns None if no validator
    /// is configured.
    pub fn validate(input: &str, config: &InputConfig) -> Option<ValidationResult> {
        config.validate.map(|f| f(input))
    }

    /// Mask input for secret fields (replaces characters with •).
    pub fn mask_input(input: &str) -> String {
        if input.is_empty() {
            return String::new();
        }
        "•".repeat(input.chars().count())
    }

    /// Compute auto-complete suffix for the current input.
    ///
    /// Given a list of candidates and the current input, returns the
    /// common prefix of all matches that start with the input, plus
    /// the remaining suffix the user needs to type.
    pub fn compute_autocomplete(input: &str, candidates: &[&str]) -> Option<AutoCompleteResult> {
        if input.is_empty() || candidates.is_empty() {
            return None;
        }

        let matches: Vec<AutoComplete> = candidates
            .iter()
            .filter(|c| c.starts_with(input))
            .map(|c| AutoComplete {
                text: c.to_string(),
                description: None,
                primary: false,
            })
            .collect();

        if matches.is_empty() {
            return None;
        }

        // Find common prefix of all matches beyond what the user typed
        let common_prefix = matches
            .iter()
            .map(|m| m.text.as_str())
            .reduce(|a, b| {
                let len = a
                    .chars()
                    .zip(b.chars())
                    .take_while(|(ca, cb)| ca == cb)
                    .count();
                let end = a.char_indices().nth(len).map(|(i, _)| i).unwrap_or(a.len());
                &a[..end]
            })
            .unwrap_or_default();

        let suffix = common_prefix.strip_prefix(input).unwrap_or("").to_string();

        Some(AutoCompleteResult {
            completed: common_prefix.to_string(),
            suffix,
            matches,
            selected: 0,
        })
    }

    /// Render a validation feedback line.
    fn render_validation(validation: &ValidationResult) -> Line<'static> {
        let mut spans = Vec::new();

        if !validation.valid {
            spans.push(Span::styled(
                "  ✗ ",
                Style::default()
                    .fg(BrandTheme::error())
                    .add_modifier(Modifier::BOLD),
            ));
            if let Some(ref error) = validation.error {
                spans.push(Span::styled(
                    error.clone(),
                    Style::default().fg(BrandTheme::error()),
                ));
            }
        } else if let Some(ref warning) = validation.warning {
            spans.push(Span::styled(
                "  ⚠ ",
                Style::default()
                    .fg(BrandTheme::warning())
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                warning.clone(),
                Style::default().fg(BrandTheme::warning()),
            ));
        }

        Line::from(spans)
    }

    /// Render auto-complete suggestions as a single line.
    fn render_autocomplete(ac: &AutoCompleteResult, width: usize) -> Line<'static> {
        let mut spans = Vec::new();

        // Tab indicator
        spans.push(Span::styled(
            "  ↹ ",
            Style::default()
                .fg(BrandTheme::accent())
                .add_modifier(Modifier::BOLD),
        ));

        // Completed portion
        if !ac.completed.is_empty() {
            spans.push(Span::styled(
                ac.completed.clone(),
                Style::default()
                    .fg(BrandTheme::accent())
                    .add_modifier(Modifier::ITALIC),
            ));
        }

        // Suffix (to be completed)
        if !ac.suffix.is_empty() {
            spans.push(Span::styled(
                ac.suffix.clone(),
                Style::default()
                    .fg(BrandTheme::accent())
                    .add_modifier(Modifier::ITALIC | Modifier::DIM),
            ));
        }

        // Match count
        if ac.matches.len() > 1 {
            spans.push(Span::styled(
                format!("  ({}/{})", ac.selected + 1, ac.matches.len()),
                Style::default().fg(BrandTheme::dim()),
            ));
        }

        // Truncate if too long
        let total_width: usize = spans
            .iter()
            .map(|s| unicode_width::UnicodeWidthStr::width(s.content.as_ref()))
            .sum();

        if total_width > width {
            let mut result = Vec::new();
            let mut current_w = 0;
            for span in spans {
                let w = unicode_width::UnicodeWidthStr::width(span.content.as_ref());
                if current_w + w > width {
                    break;
                }
                result.push(span);
                current_w += w;
            }
            return Line::from(result);
        }

        Line::from(spans)
    }

    /// Render a gradient border line (top or bottom) with mode-aware coloring.
    fn render_border(width: usize, _mode: InputMode, is_top: bool, pulse: f32) -> Line<'static> {
        let (corner_l, corner_r) = if is_top {
            ("╭", "╮")
        } else {
            ("╰", "╯")
        };
        let border_width = width.min(80);
        let content_width = border_width.saturating_sub(2);
        let intensity = if is_top { 1.0 } else { 0.5 };
        let corner_boost = 0.85 + 0.30 * pulse.clamp(0.0, 1.0);

        let gradient = BrandTheme::gradient();
        let mut spans: Vec<Span<'static>> = Vec::with_capacity(4);

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

        let seg_count = gradient.len().min(content_width);
        let seg_size = content_width / seg_count;
        let remainder = content_width - seg_size * seg_count;
        for seg in 0..seg_count {
            let n = seg_size + if seg < remainder { 1 } else { 0 };
            if n == 0 {
                continue;
            }
            let color = gradient[seg % gradient.len()];
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

    /// Multiply an RGB color by a scalar, clamping the result.
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
    fn render_mode_indicator(mode: InputMode, is_processing: bool) -> Line<'static> {
        let mut spans = Vec::with_capacity(8);

        spans.push(Span::styled(
            "◆",
            Style::default()
                .fg(BrandTheme::gradient_color(0))
                .add_modifier(Modifier::BOLD),
        ));

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

        spans.push(Span::styled("  │ ", Style::default().fg(BrandTheme::dim())));

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
    fn render_input_with_cursor(
        input: &str,
        cursor_pos: usize,
        width: usize,
        is_processing: bool,
        mode: InputMode,
        pulse: f32,
        config: &InputConfig,
    ) -> Line<'static> {
        let mut spans = Vec::with_capacity(4);
        let prefix_width = 2;
        let content_width = width.saturating_sub(prefix_width);

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

        // Determine display text (masked or raw)
        let display_text = if config.secret && !input.is_empty() && !is_processing {
            Self::mask_input(input)
        } else {
            input.to_string()
        };

        if display_text.is_empty() {
            let placeholder = if is_processing {
                Some("thinking...")
            } else {
                config.placeholder.or(match mode {
                    InputMode::Chat => Some("type a message..."),
                    InputMode::Shell => Some("enter shell command..."),
                    InputMode::Command => Some("type / for commands..."),
                    InputMode::Search => Some("search prompt history..."),
                })
            };
            spans.push(Span::styled(
                placeholder.unwrap_or(""),
                Style::default()
                    .fg(BrandTheme::dim())
                    .add_modifier(Modifier::ITALIC),
            ));
        } else {
            let chars: Vec<char> = display_text.chars().collect();
            let char_count = chars.len();

            let cursor_char = input[..cursor_pos.min(input.len())].chars().count();

            let mut visible_start;
            let mut visible_end;
            let total_display_width: usize = chars
                .iter()
                .map(|c| unicode_width::UnicodeWidthChar::width(*c).unwrap_or(0))
                .sum();

            if total_display_width <= content_width.saturating_sub(1) {
                visible_start = 0;
                visible_end = char_count;
            } else if cursor_char < content_width / 2 {
                visible_start = 0;
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
                let half = content_width / 2;
                visible_start = cursor_char.saturating_sub(half);
                visible_end = (cursor_char + half).min(char_count);
            }

            if visible_start > 0 {
                spans.push(Span::styled("…", Style::default().fg(BrandTheme::dim())));
            }

            let display_color = rgb(220, 220, 220);
            let mut visible_text = String::with_capacity(content_width * 3);
            for c in &chars[visible_start..visible_end] {
                visible_text.push(*c);
            }
            spans.push(Span::styled(
                visible_text,
                Style::default().fg(display_color),
            ));

            if visible_end < char_count {
                spans.push(Span::styled("…", Style::default().fg(BrandTheme::dim())));
            }

            if is_processing {
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

/// Empty state renderer for professional "no data" displays.
pub struct EmptyState;

impl EmptyState {
    /// Render an empty state with icon, title, and hint.
    pub fn render(icon: &str, title: &str, hint: &str, width: usize) -> Vec<Line<'static>> {
        let mut result = Vec::new();

        let title_line = format!("{}  {}", icon, title);
        let pad = width.saturating_sub(title_line.len() + 4) / 2;
        result.push(Line::from(Span::styled(
            format!("{}{}{}", " ".repeat(pad), title_line, " ".repeat(pad),),
            Style::default()
                .fg(BrandTheme::dim_bright())
                .add_modifier(Modifier::BOLD),
        )));

        let hint_pad = width.saturating_sub(hint.len() + 4) / 2;
        result.push(Line::from(Span::styled(
            format!("{}{}{}", " ".repeat(hint_pad), hint, " ".repeat(hint_pad),),
            Style::default()
                .fg(BrandTheme::dim())
                .add_modifier(Modifier::ITALIC),
        )));

        result
    }
}

/// Professional notification/toast renderer.
pub struct Notification;

impl Notification {
    /// Render a success notification.
    pub fn success(message: &str) -> Line<'_> {
        Line::from(vec![
            Span::styled(
                " ✓ ",
                Style::default()
                    .fg(BrandTheme::success())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(message, Style::default().fg(BrandTheme::dim_bright())),
        ])
    }

    /// Render an error notification.
    pub fn error(message: &str) -> Line<'_> {
        Line::from(vec![
            Span::styled(
                " ✗ ",
                Style::default()
                    .fg(BrandTheme::error())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(message, Style::default().fg(BrandTheme::error())),
        ])
    }

    /// Render a warning notification.
    pub fn warning(message: &str) -> Line<'_> {
        Line::from(vec![
            Span::styled(
                " ⚠ ",
                Style::default()
                    .fg(BrandTheme::warning())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(message, Style::default().fg(BrandTheme::warning())),
        ])
    }

    /// Render an info notification.
    pub fn info(message: &str) -> Line<'_> {
        Line::from(vec![
            Span::styled(
                " ℹ ",
                Style::default()
                    .fg(BrandTheme::info())
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(message, Style::default().fg(BrandTheme::dim_bright())),
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
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_enhanced_input_processing() {
        let lines = EnhancedInput::render("", 0, 80, true, InputMode::Chat);
        assert!(!lines.is_empty());
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn test_enhanced_input_unicode_safe() {
        let input = "こんにちは世界";
        let lines = EnhancedInput::render(input, input.len(), 40, false, InputMode::Chat);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_enhanced_input_narrow_terminal() {
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
        let counter_seen = lines.iter().any(|line| {
            let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
            text.contains("11 bytes")
        });
        assert!(counter_seen, "help hints should include a byte counter");
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
            "help hints should not mention bytes for empty input"
        );
    }

    #[test]
    fn test_validation_valid() {
        let result = ValidationResult::valid();
        assert!(result.valid);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_validation_error() {
        let result = ValidationResult::error("input too short");
        assert!(!result.valid);
        assert_eq!(result.error, Some("input too short".to_string()));
    }

    #[test]
    fn test_validation_warning() {
        let result = ValidationResult::warning("proceed with caution");
        assert!(result.valid);
        assert_eq!(result.warning, Some("proceed with caution".to_string()));
    }

    #[test]
    fn test_mask_input() {
        assert_eq!(EnhancedInput::mask_input("hello"), "•••••");
        assert_eq!(EnhancedInput::mask_input(""), "");
        assert_eq!(EnhancedInput::mask_input("héllo"), "•••••");
    }

    #[test]
    fn test_autocomplete_basic() {
        let candidates = vec!["hello world", "hello there", "help me"];
        let result = EnhancedInput::compute_autocomplete("he", &candidates).unwrap();
        assert!(!result.matches.is_empty());
        assert!(result.matches.len() >= 3);
    }

    #[test]
    fn test_autocomplete_no_match() {
        let candidates = vec!["hello", "world"];
        let result = EnhancedInput::compute_autocomplete("xyz", &candidates);
        assert!(result.is_none());
    }

    #[test]
    fn test_autocomplete_empty_input() {
        let candidates = vec!["hello"];
        let result = EnhancedInput::compute_autocomplete("", &candidates);
        assert!(result.is_none());
    }

    #[test]
    fn test_empty_state_render() {
        let lines = EmptyState::render("📭", "No sessions", "Start a new session to begin", 60);
        assert!(!lines.is_empty());
    }

    #[test]
    fn test_notification_success() {
        let line = Notification::success("Done");
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_notification_error() {
        let line = Notification::error("Failed");
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_notification_warning() {
        let line = Notification::warning("Careful");
        assert!(!line.spans.is_empty());
    }

    #[test]
    fn test_notification_info() {
        let line = Notification::info("Note");
        assert!(!line.spans.is_empty());
    }
}
