use super::inline_interactive_ui::format_elapsed;
use super::selection_highlight::highlight_line_selection;
use super::tools_ui::{get_tool_activity_detail, summarize_batch_running_tools_compact};
use super::visual_debug::{self, FrameCaptureBuilder};
use super::{
    ProcessingStatus, TuiState, accent_color, ai_color, animated_tool_color, asap_color,
    border_color, dim_color, error_color, info_color, muted_text_color, queued_color,
    rainbow_prompt_color, success_color, user_color, warning_color,
};
use crate::alphacode_tui::message::ConnectionPhase;
use crate::alphacode_tui::tui::app;
use crate::alphacode_tui::tui::brand_ux::BrandTheme;
use crate::alphacode_tui::tui::color_support::rgb;
use crate::alphacode_tui::tui::detect_kv_cache_problem;
use crate::alphacode_tui::tui::info_widget::occasional_status_tip;
use crate::alphacode_tui::tui::layout_utils;
use crate::alphacode_tui::tui::session_facts;
use ratatui::{prelude::*, style::Modifier, widgets::Paragraph};
use unicode_segmentation::UnicodeSegmentation;

fn shell_mode_color() -> Color {
    success_color()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ComposerMode {
    Chat,
    SlashCommand,
    ShellLocal,
    ShellRemote,
}

impl ComposerMode {
    fn is_shell(self) -> bool {
        matches!(self, Self::ShellLocal | Self::ShellRemote)
    }
}

fn composer_mode(input: &str, is_remote_mode: bool) -> ComposerMode {
    if app::extract_input_shell_command(input).is_some() {
        if is_remote_mode {
            ComposerMode::ShellRemote
        } else {
            ComposerMode::ShellLocal
        }
    } else if input.trim_start().starts_with('/') {
        ComposerMode::SlashCommand
    } else {
        ComposerMode::Chat
    }
}

fn shell_mode_hint(mode: ComposerMode) -> Option<&'static str> {
    match mode {
        ComposerMode::ShellLocal => Some("  \u{2699} shell \u{00b7} Enter runs locally"),
        ComposerMode::ShellRemote => Some("  \u{2699} shell \u{00b7} Enter runs on server"),
        _ => None,
    }
}

/// Prepare a transient status string for rendering inside the TUI.
///
/// Two jobs, both of them load-bearing:
///
/// 1. **Strip terminal escapes.** Ratatui writes cell symbols verbatim; it has
///    no SGR parser. Any escape sequence that reaches a `Span` is displayed as
///    the literal characters `ESC [ 3 8 ; 2 ; …`, which is how a raw-ANSI
///    status notice once printed `ESC[38;2;90;200;130mPasted 12 lines…` into
///    the status line. Stripping here makes the class of bug impossible rather
///    than fixing one call site at a time.
/// 2. **Collapse zero-width formatting characters.** A variation selector
///    changes the glyph the terminal picks and can invalidate ratatui's cell
///    width cache mid-frame, producing a one-frame flicker. `⚠️` (U+26A0
///    U+FE0F) is normalized to bare `⚠`, which is what the TUI renders
///    everywhere else.
///
/// The result also has newlines folded to spaces: notices live on a single
/// status row, and an embedded `\n` would silently clip the rest of the text.
fn normalize_repaint_sensitive_notice_text(text: &str) -> String {
    let stripped = crate::alphacode_base::message::strip_ansi_escape_sequences(text);
    stripped.replace(['\n', '\r'], " ").replace("⚠️", "⚠")
}

fn command_suggestion_window_start(selected: usize, suggestion_count: usize) -> usize {
    if suggestion_count <= app::COMMAND_SUGGESTION_VISIBLE_LIMIT {
        0
    } else {
        selected
            .saturating_add(1)
            .saturating_sub(app::COMMAND_SUGGESTION_VISIBLE_LIMIT)
            .min(suggestion_count.saturating_sub(app::COMMAND_SUGGESTION_VISIBLE_LIMIT))
    }
}

/// Where the command-suggestion popover should render, given the input chunk.
///
/// Suggestions are an overlay: they never participate in the vertical layout,
/// so opening the palette never moves the transcript, the status line, or the
/// pinned footer. Preferred position is directly below the input (over blank
/// rows / the pinned footer); when the input sits at the bottom of the screen
/// the popover flips above the input, covering the last transcript rows
/// instead of reflowing them.
fn command_suggestions_overlay_rect(
    input_area: Rect,
    line_count: u16,
    frame_area: Rect,
) -> Option<Rect> {
    if line_count == 0 || input_area.width == 0 {
        return None;
    }
    let below_start = input_area.y.saturating_add(input_area.height);
    let space_below = frame_area.bottom().saturating_sub(below_start);
    if space_below >= line_count {
        return Some(Rect::new(
            input_area.x,
            below_start,
            input_area.width,
            line_count,
        ));
    }
    let space_above = input_area.y.saturating_sub(frame_area.y);
    let above_rows = line_count.min(space_above);
    if above_rows >= line_count.min(space_below.max(1)) && above_rows > 0 {
        return Some(Rect::new(
            input_area.x,
            input_area.y - above_rows,
            input_area.width,
            above_rows,
        ));
    }
    if space_below > 0 {
        return Some(Rect::new(
            input_area.x,
            below_start,
            input_area.width,
            space_below,
        ));
    }
    None
}

/// Whether the command palette is active for the current composer state.
fn command_suggestions_active(app: &dyn TuiState, suggestions: &[(String, &'static str)]) -> bool {
    let mode = composer_mode(app.input(), app.is_remote_mode());
    !suggestions.is_empty()
        && matches!(mode, ComposerMode::SlashCommand | ComposerMode::Chat)
        && (matches!(mode, ComposerMode::SlashCommand) || !app.is_processing())
}

/// Keep the selected command visible when a small terminal cannot fit the
/// whole palette. The keyboard hint gets the last row whenever there is room.
fn fit_command_suggestion_lines_to_height(
    mut lines: Vec<Line<'static>>,
    max_height: usize,
) -> Vec<Line<'static>> {
    if lines.len() <= max_height || max_height == 0 {
        lines.truncate(max_height);
        return lines;
    }

    let footer = lines.pop();
    let footer_rows = usize::from(max_height > 1);
    let content_height = max_height.saturating_sub(footer_rows);
    let selected = lines.iter().position(|line| {
        line.spans
            .first()
            .is_some_and(|span| span.content.as_ref().starts_with('▸'))
    });
    let max_start = lines.len().saturating_sub(content_height);
    let start = selected
        .unwrap_or(0)
        .saturating_sub(content_height.saturating_sub(1))
        .min(max_start);
    let visible: Vec<Line<'static>> = lines.into_iter().skip(start).take(content_height).collect();
    let mut fitted = visible;
    if footer_rows > 0
        && let Some(footer) = footer
    {
        fitted.push(footer);
    }
    fitted
}

/// Draw the Ctrl+R reverse prompt-history search overlay. Reuses the
/// command-palette positioning: floats below (or above) the input without
/// reserving layout height. Shows the query line plus the match list with the
/// selected row highlighted.
pub(super) fn draw_prompt_history_search_overlay(
    frame: &mut Frame,
    app: &dyn TuiState,
    area: Rect,
) {
    let Some(view) = app.prompt_history_search() else {
        return;
    };
    const VISIBLE_LIMIT: usize = 8;

    let accent = Style::default().fg(BrandTheme::accent());
    let dim = Style::default().fg(BrandTheme::dim());
    let normal = Style::default().fg(BrandTheme::info());

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(vec![
        Span::styled("(history search) ", dim),
        Span::styled(view.query.clone(), accent),
        Span::styled("█", accent),
        Span::styled(
            "  ↑↓ select · ↵ insert · Esc cancel",
            Style::default().fg(BrandTheme::dim()),
        ),
    ]));

    if view.matches.is_empty() {
        if view.query.trim().is_empty() {
            lines.push(Line::from(Span::styled("  type to search history", dim)));
        } else {
            lines.push(Line::from(Span::styled("  no matches", dim)));
        }
    } else {
        // Keep the selected row inside the visible window.
        let window_start = view
            .selected
            .saturating_sub(VISIBLE_LIMIT.saturating_sub(1))
            .min(
                view.matches
                    .len()
                    .saturating_sub(VISIBLE_LIMIT.min(view.matches.len())),
            );
        let visible = view
            .matches
            .iter()
            .enumerate()
            .skip(window_start)
            .take(VISIBLE_LIMIT);
        for (index, preview) in visible {
            let is_selected = index == view.selected;
            let marker = if is_selected { "▸ " } else { "  " };
            let style = if is_selected { accent } else { normal };
            let mut spans = vec![
                Span::styled(marker.to_string(), style),
                Span::styled(preview.clone(), style),
            ];
            if index == window_start + VISIBLE_LIMIT - 1
                && view.matches.len() > window_start + VISIBLE_LIMIT
            {
                spans.push(Span::styled(
                    format!(
                        "  +{} more",
                        view.matches.len() - (window_start + VISIBLE_LIMIT)
                    ),
                    dim,
                ));
            }
            lines.push(Line::from(spans));
        }
    }

    let Some(rect) = command_suggestions_overlay_rect(area, lines.len() as u16, frame.area())
    else {
        return;
    };
    lines = lines
        .into_iter()
        .map(|line| truncate_line_with_ellipsis(line, area.width as usize))
        .collect();
    lines.truncate(rect.height as usize);
    frame.render_widget(ratatui::widgets::Clear, rect);
    frame.render_widget(Paragraph::new(lines), rect);
}

/// Draw the command-suggestion popover as a late overlay pass.
///
/// Called after the chunked layout (and info widgets) have rendered so the
/// palette floats over existing rows instead of reserving layout height.
pub(super) fn draw_command_suggestions_overlay(frame: &mut Frame, app: &dyn TuiState, area: Rect) {
    let suggestions = app.command_suggestions();
    if !command_suggestions_active(app, &suggestions) {
        return;
    }
    let mut lines = command_suggestion_lines(app, &suggestions);
    let Some(rect) = command_suggestions_overlay_rect(area, lines.len() as u16, frame.area())
    else {
        return;
    };
    lines = lines
        .into_iter()
        .map(|line| truncate_line_with_ellipsis(line, area.width as usize))
        .collect();
    lines = fit_command_suggestion_lines_to_height(lines, rect.height as usize);
    if app.centered_mode() {
        lines = lines
            .into_iter()
            .map(|l| l.alignment(Alignment::Center))
            .collect();
    }
    frame.render_widget(ratatui::widgets::Clear, rect);
    frame.render_widget(Paragraph::new(lines), rect);
}

fn command_suggestion_lines(
    app: &dyn TuiState,
    suggestions: &[(String, &'static str)],
) -> Vec<Line<'static>> {
    // Highlight the characters of each command that the typed query matched.
    // We only highlight the command token itself (the part before the first
    // space), matched against the corresponding leading token of the input.
    let needle = command_suggestion_needle(app.input());
    let highlight = |cmd: &str, base: Style| -> Vec<Span<'static>> {
        highlight_command_spans(cmd, needle.as_deref(), base)
    };

    let mut lines = Vec::new();
    let accent_style = Style::default().fg(BrandTheme::accent());
    let selected_style = Style::default()
        .fg(BrandTheme::accent())
        .add_modifier(Modifier::BOLD);
    let unselected_style = Style::default().fg(BrandTheme::info());
    let desc_selected_style = Style::default().fg(BrandTheme::accent());
    let desc_unselected_style = Style::default().fg(BrandTheme::dim());

    if suggestions.len() == 1 {
        let (cmd, desc) = &suggestions[0];
        let mut spans = vec![Span::styled("▸ ", selected_style)];
        spans.extend(highlight(cmd, accent_style));
        spans.push(Span::styled(format!("  {}", desc), accent_style));
        lines.push(Line::from(spans));
    } else if !suggestions.is_empty() {
        let selected = app
            .command_suggestion_selected()
            .min(suggestions.len().saturating_sub(1));
        let window_start = command_suggestion_window_start(selected, suggestions.len());
        let limited: Vec<_> = suggestions
            .iter()
            .skip(window_start)
            .take(app::COMMAND_SUGGESTION_VISIBLE_LIMIT)
            .collect();
        let window_end = window_start + limited.len();
        let more_count = suggestions.len().saturating_sub(window_end);
        let selected_visible = selected.saturating_sub(window_start);

        for (i, (cmd, desc)) in limited.iter().enumerate() {
            let is_selected = i == selected_visible;
            let command_style = if is_selected {
                selected_style
            } else {
                unselected_style
            };
            let description_style = if is_selected {
                desc_selected_style
            } else {
                desc_unselected_style
            };
            let mut row = vec![Span::styled(
                if is_selected { "▸ " } else { "  " },
                command_style,
            )];
            row.extend(highlight(cmd, command_style));
            row.push(Span::styled(format!("  {}", desc), description_style));
            if i == 0 && window_start > 0 {
                row.push(Span::styled(
                    format!("  ↑{}", window_start),
                    Style::default().fg(BrandTheme::dim()),
                ));
            }
            if i + 1 == limited.len() && more_count > 0 {
                row.push(Span::styled(
                    format!("  +{} more", more_count),
                    Style::default().fg(BrandTheme::dim()),
                ));
            }
            lines.push(Line::from(row));
        }
    }
    if !suggestions.is_empty() {
        let selected = app
            .command_suggestion_selected()
            .min(suggestions.len().saturating_sub(1));
        let is_complete_command = suggestions
            .get(selected)
            .is_some_and(|(cmd, _)| cmd == app.input().trim());
        let enter_action = if is_complete_command {
            "Enter run"
        } else {
            "Enter fill"
        };
        let navigation_hint = if suggestions.len() > 1 {
            "↑↓ move · Tab complete"
        } else {
            "Tab complete"
        };
        lines.push(Line::from(Span::styled(
            format!("  {} · {}", navigation_hint, enter_action),
            Style::default().fg(BrandTheme::dim()),
        )));
    }
    lines
}

/// Extract the slash-command portion of the typed input that should be matched
/// against suggestion command tokens for highlighting purposes.
fn command_suggestion_needle(input: &str) -> Option<String> {
    let trimmed = input.trim_start();
    if !trimmed.starts_with('/') {
        return None;
    }
    Some(trimmed.to_string())
}

/// Build spans for a suggestion command, recoloring the characters that the
/// fuzzy matcher aligned with the typed query. Falls back to a single
/// unhighlighted span when there is nothing (useful) to highlight.
fn highlight_command_spans(cmd: &str, needle: Option<&str>, base: Style) -> Vec<Span<'static>> {
    let positions: Vec<usize> = match needle {
        Some(n) if !n.is_empty() && n != "/" => {
            let mut positions = crate::alphacode_tui::tui::fuzzy::fuzzy_match_positions(n, cmd);
            // The leading '/' of a command token is structurally part of the
            // decoration, never part of the matched keyword — render it dimmed
            // like the rest of the command chrome, not brightened.
            positions.retain(|pos| *pos != 0);
            positions
        }
        _ => Vec::new(),
    };
    if positions.is_empty() {
        return vec![Span::styled(cmd.to_string(), base)];
    }

    // Recolor (rather than underline) the matched characters so they stay in
    // the command palette's own hue: matched chars are a brighter version of
    // the line's base color, while unmatched chars are dimmed so the match
    // visually pops.
    let highlight_style = base
        .fg(brighten_command_color(base.fg))
        .add_modifier(Modifier::BOLD);
    let rest_style = base.fg(dim_command_color(base.fg));
    let chars: Vec<char> = cmd.chars().collect();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut run_start = 0usize;
    let mut run_is_match = !chars.is_empty() && positions.contains(&0);
    for i in 1..=chars.len() {
        let cur_is_match = i < chars.len() && positions.contains(&i);
        if cur_is_match != run_is_match || i == chars.len() {
            let chunk: String = chars[run_start..i].iter().collect();
            spans.push(Span::styled(
                chunk,
                if run_is_match {
                    highlight_style
                } else {
                    rest_style
                },
            ));
            run_start = i;
            run_is_match = cur_is_match;
        }
    }
    spans
}

/// Blend a palette color toward white to emphasize a matched character while
/// keeping its original hue.
pub(crate) fn brighten_command_color(color: Option<Color>) -> Color {
    match color {
        Some(Color::Rgb(r, g, b)) => {
            let lift = |c: u8| -> u8 { c.saturating_add((255 - c) / 2) };
            rgb(lift(r), lift(g), lift(b))
        }
        _ => rgb(255, 255, 255),
    }
}

/// Blend a palette color toward black so unmatched characters recede.
pub(crate) fn dim_command_color(color: Option<Color>) -> Color {
    match color {
        Some(Color::Rgb(r, g, b)) => rgb(r / 2, g / 2, b / 2),
        other => other.unwrap_or_else(dim_color),
    }
}

pub(super) fn input_hint_line_height(app: &dyn TuiState) -> u16 {
    // Command suggestions render as an overlay (see
    // `draw_command_suggestions_overlay`) and intentionally reserve no layout
    // height: opening the palette must not move the transcript or footer.
    if command_suggestions_active(app, &app.command_suggestions()) {
        return 0;
    }
    let mode = composer_mode(app.input(), app.is_remote_mode());

    u16::from(
        shell_mode_hint(mode).is_some()
            || app.next_prompt_new_session_armed()
            || (app.is_processing() && !app.input().is_empty()),
    )
}

pub(super) fn send_mode_reserved_width(app: &dyn TuiState) -> usize {
    let (icon, _) = send_mode_indicator(app);
    if icon.is_empty() {
        0
    } else {
        unicode_width::UnicodeWidthStr::width(icon) + 1
    }
}

pub(super) fn input_prompt(app: &dyn TuiState) -> (String, Color) {
    let mode = composer_mode(app.input(), app.is_remote_mode());
    if mode.is_shell() {
        ("$ ".to_string(), BrandTheme::success())
    } else if app.is_processing() {
        ("\u{2026} ".to_string(), BrandTheme::warning())
    } else {
        // An active skill deliberately does NOT get a mode prefix. Echoing the
        // skill name inside the composer ("> bugbounty") pushed the typed text
        // right and read like leftover slash-command text stuck in the input.
        // The active skill is already reported by the system message, the
        // header, and `clear_skill`, so the composer stays uncluttered.
        ("> ".to_string(), BrandTheme::model())
    }
}

pub(crate) fn input_prompt_len(app: &dyn TuiState, _next_prompt: usize) -> usize {
    let (prompt_char, _) = input_prompt(app);
    unicode_width::UnicodeWidthStr::width(prompt_char.as_str())
}

pub(crate) fn next_input_prompt_number(app: &dyn TuiState) -> usize {
    app.display_user_message_count() + 1
}

pub(super) fn wrapped_input_line_count(
    app: &dyn TuiState,
    area_width: u16,
    next_prompt: usize,
) -> usize {
    let reserved_width = send_mode_reserved_width(app);
    let prompt_len = input_prompt_len(app, next_prompt);
    let line_width = (area_width as usize).saturating_sub(prompt_len + reserved_width);
    if line_width == 0 {
        return 1;
    }

    // Keep prompt numbering out of the composer: the numbered queued rows are
    // only meaningful once a prompt has been sent. This also leaves a stable,
    // easy-to-scan mode prefix at the edit point.
    let num_str = String::new();
    let (prompt_char, caret_color) = input_prompt(app);
    let (lines, _, _) = wrap_input_text(
        app.input(),
        app.cursor_pos(),
        line_width,
        &num_str,
        &prompt_char,
        caret_color,
        prompt_len,
    );
    lines.len().max(1)
}

pub(super) fn pending_prompt_count(app: &dyn TuiState) -> usize {
    let pending_count = if app.is_processing() {
        app.pending_soft_interrupts().len()
    } else {
        0
    };
    let interleave = app.is_processing()
        && app
            .interleave_message()
            .map(|msg| !msg.is_empty())
            .unwrap_or(false);
    app.queued_messages().len() + pending_count + if interleave { 1 } else { 0 }
}

pub(super) fn pending_queue_preview(app: &dyn TuiState) -> Vec<String> {
    let mut previews = Vec::new();
    if app.is_processing() {
        for msg in app.pending_soft_interrupts() {
            if !msg.is_empty() {
                let normalized = normalize_repaint_sensitive_notice_text(msg);
                previews.push(format!("↻ {}", truncate_preview(&normalized, 120)));
            }
        }
        if let Some(msg) = app.interleave_message()
            && !msg.is_empty()
        {
            let normalized = normalize_repaint_sensitive_notice_text(msg);
            previews.push(format!("⚡ {}", truncate_preview(&normalized, 120)));
        }
    }
    for msg in app.queued_messages() {
        let normalized = normalize_repaint_sensitive_notice_text(msg);
        previews.push(format!("⏳ {}", truncate_preview(&normalized, 120)));
    }
    previews
}

/// Truncate a preview string to `max_width` terminal columns,
/// collapsing internal newlines to spaces for a single-line preview.
/// When truncated, appends an ellipsis.
fn truncate_preview(text: &str, max_width: usize) -> String {
    let single_line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = single_line.trim();
    if unicode_width::UnicodeWidthStr::width(trimmed) <= max_width {
        trimmed.to_string()
    } else {
        if max_width == 0 {
            return String::new();
        }
        if max_width < 3 {
            return ".".repeat(max_width);
        }

        let prefix_width = max_width - 3;
        let mut width = 0;
        let mut prefix = String::new();
        for grapheme in unicode_segmentation::UnicodeSegmentation::graphemes(trimmed, true) {
            let grapheme_width = unicode_width::UnicodeWidthStr::width(grapheme);
            if width + grapheme_width > prefix_width {
                break;
            }
            prefix.push_str(grapheme);
            width += grapheme_width;
        }
        format!("{prefix}...")
    }
}

pub(super) fn draw_queued(frame: &mut Frame, app: &dyn TuiState, area: Rect, start_num: usize) {
    let mut items: Vec<(QueuedMsgType, &str)> = Vec::new();
    if app.is_processing() {
        for msg in app.pending_soft_interrupts() {
            if !msg.is_empty() {
                items.push((QueuedMsgType::Pending, msg.as_str()));
            }
        }
        if let Some(msg) = app.interleave_message()
            && !msg.is_empty()
        {
            items.push((QueuedMsgType::Interleave, msg));
        }
    }
    for msg in app.queued_messages() {
        items.push((QueuedMsgType::Queued, msg.as_str()));
    }

    let pending_count = items.len();
    let lines: Vec<Line> = items
        .iter()
        .take(3)
        .enumerate()
        .map(|(i, (msg_type, msg))| {
            let normalized_msg = normalize_repaint_sensitive_notice_text(msg);
            let distance = pending_count.saturating_sub(i);
            let num_color = rainbow_prompt_color(distance);
            let (indicator, indicator_color, msg_color, dim) = match msg_type {
                QueuedMsgType::Pending => {
                    ("↻", BrandTheme::warning(), BrandTheme::warning(), false)
                }
                QueuedMsgType::Interleave => {
                    ("⚡", BrandTheme::accent(), BrandTheme::accent(), false)
                }
                QueuedMsgType::Queued => ("⏳", BrandTheme::info(), BrandTheme::info(), true),
            };
            let number = (start_num + i).to_string();
            let more_suffix = if i == 2 && pending_count > 3 {
                format!("  +{} more", pending_count - 3)
            } else {
                String::new()
            };
            let prefix_width = unicode_width::UnicodeWidthStr::width(number.as_str())
                + 2
                + unicode_width::UnicodeWidthStr::width(indicator);
            let suffix_width = unicode_width::UnicodeWidthStr::width(more_suffix.as_str());
            let preview_width = (area.width as usize)
                .saturating_sub(prefix_width)
                .saturating_sub(suffix_width);
            let truncated = truncate_preview(&normalized_msg, preview_width);
            let mut msg_style = Style::default().fg(msg_color);
            if dim {
                msg_style = msg_style.dim();
            }
            let mut spans = vec![
                Span::styled(number, Style::default().fg(num_color)),
                Span::raw(" "),
                Span::styled(indicator, Style::default().fg(indicator_color)),
                Span::raw(" "),
                Span::styled(truncated, msg_style),
            ];
            if !more_suffix.is_empty() {
                spans.push(Span::styled(more_suffix, Style::default().fg(dim_color())));
            }
            let mut line = Line::from(spans);
            // Protect rows on very narrow terminals when the prompt number and
            // mode marker use more columns than a normal short prefix.
            super::viewport::truncate_line_in_place_to_width(&mut line, area.width as usize);
            line
        })
        .collect();

    let paragraph = if app.centered_mode() {
        Paragraph::new(
            lines
                .iter()
                .map(|line| line.clone().alignment(Alignment::Center))
                .collect::<Vec<_>>(),
        )
    } else {
        Paragraph::new(lines)
    };
    frame.render_widget(paragraph, area);
}

fn format_stream_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.0}k", tokens as f64 / 1_000.0)
    } else {
        tokens.to_string()
    }
}

fn occasional_session_history_warning(
    total_tokens: u64,
    compaction_count: usize,
    context_limit: Option<usize>,
    width: usize,
    elapsed_secs: u64,
) -> Option<String> {
    let context_limit = context_limit.and_then(|limit| u64::try_from(limit).ok());
    let token_threshold = context_limit.unwrap_or(250_000).max(1);

    if total_tokens < token_threshold || width < 64 {
        return None;
    }

    // This is not current context usage, so keep it an occasional nudge rather
    // than a persistent warning. The first reminder is never shown before the
    // session has processed a full model context window, then it reappears at
    // context-window-sized token intervals as the session grows. Keep the
    // existing time gate as a short visibility window within each interval.
    let token_interval = token_threshold;
    let token_window = (token_interval / 20).clamp(10_000, 100_000);
    if (total_tokens - token_threshold) % token_interval >= token_window {
        return None;
    }
    if elapsed_secs % 300 >= 10 {
        return None;
    }

    let tokens = format_stream_tokens(total_tokens);
    let compactions = if compaction_count > 0 {
        format!(
            " and {compaction_count} compact{}",
            if compaction_count == 1 { "" } else { "s" }
        )
    } else {
        String::new()
    };

    Some(format!(
        "Session history: {tokens} tokens processed{compactions}; /clear starts fresh context"
    ))
}

fn connection_phase_label(phase: &ConnectionPhase) -> String {
    match phase {
        ConnectionPhase::Authenticating => "refreshing auth".to_string(),
        ConnectionPhase::Connecting => "connecting".to_string(),
        ConnectionPhase::SendingRequest => "working".to_string(),
        ConnectionPhase::WaitingForResponse => "waiting for response".to_string(),
        ConnectionPhase::Streaming => "streaming".to_string(),
        ConnectionPhase::Retrying { attempt, max } => format!("retrying {}/{}", attempt, max),
    }
}

fn display_connection_type(connection_type: &str) -> String {
    match connection_type.trim() {
        "https/sse" => "https".to_string(),
        "websocket/persistent-fresh" => "websocket".to_string(),
        "websocket/persistent-reuse" => "existing websocket".to_string(),
        other => other.to_string(),
    }
}

fn normalize_status_detail(detail: &str) -> Option<String> {
    let trimmed = detail.trim();
    if trimmed.is_empty() {
        return None;
    }

    Some(
        match trimmed {
            "fresh websocket" => "opening websocket",
            "reusing websocket" => "using existing websocket",
            "websocket healthcheck" => "verifying websocket",
            "https fallback" => "using https fallback",
            other => other,
        }
        .to_string(),
    )
}

fn transport_label_overlaps(left: &str, right: &str) -> bool {
    let left = left.trim().to_ascii_lowercase();
    let right = right.trim().to_ascii_lowercase();
    !left.is_empty()
        && !right.is_empty()
        && (left == right || left.contains(&right) || right.contains(&left))
}

fn collect_transport_context_labels(
    detail: Option<String>,
    connection: Option<String>,
    upstream: Option<String>,
) -> Vec<String> {
    let mut labels = Vec::new();

    if let Some(detail) = detail.filter(|detail| !detail.trim().is_empty()) {
        labels.push(detail);
    }

    if let Some(connection) = connection.filter(|conn| !conn.trim().is_empty()) {
        let overlaps_existing = labels
            .iter()
            .any(|existing| transport_label_overlaps(existing, &connection));
        if !overlaps_existing {
            labels.push(connection);
        }
    }

    if let Some(upstream) = upstream
        .map(|upstream| upstream.trim().to_string())
        .filter(|upstream| !upstream.is_empty())
    {
        labels.push(format!("via {}", upstream));
    }

    labels
}

fn transport_context_labels(app: &dyn TuiState) -> Vec<String> {
    collect_transport_context_labels(
        app.status_detail()
            .and_then(|detail| normalize_status_detail(&detail)),
        app.connection_type()
            .map(|conn| display_connection_type(&conn))
            .filter(|conn| !conn.is_empty()),
        app.upstream_provider(),
    )
}

fn append_transport_context(status_text: &mut String, app: &dyn TuiState) {
    for label in transport_context_labels(app) {
        status_text.push_str(&format!(" · {}", label));
    }
}

fn streaming_liveness_label(
    time_str: String,
    stale_secs: Option<f32>,
    stream_message_ended: bool,
) -> String {
    if stream_message_ended {
        return time_str;
    }
    match stale_secs {
        Some(s) if s > 15.0 => format!("\u{26a0} stalled {:.0}s \u{00b7} {}", s, time_str),
        Some(s) if s > 5.0 => format!("(idle {:.0}s) \u{00b7} {}", s, time_str),
        Some(s) if s > 2.0 => format!("(quiet {:.0}s) \u{00b7} {}", s, time_str),
        _ => time_str,
    }
}

fn batch_progress_state(
    batch_prog: Option<crate::bus::BatchProgress>,
    initial_total: Option<usize>,
) -> (usize, usize, Option<String>) {
    match batch_prog {
        Some(progress) => (progress.completed, progress.total, progress.last_completed),
        None => (0, initial_total.unwrap_or(0), None),
    }
}

fn batch_running_summary(batch_prog: &crate::bus::BatchProgress) -> Option<String> {
    summarize_batch_running_tools_compact(&batch_prog.running)
}

fn append_batch_progress_spans(
    spans: &mut Vec<Span<'static>>,
    anim_color: Color,
    batch_prog: Option<crate::bus::BatchProgress>,
    initial_total: Option<usize>,
) {
    let running_summary = batch_prog.as_ref().and_then(batch_running_summary);
    let (completed, total, last_completed) = batch_progress_state(batch_prog, initial_total);

    if total > 0 {
        spans.push(Span::styled(
            format!(" · {}/{} done", completed, total),
            Style::default().fg(anim_color).bold(),
        ));
        // Inline gradient progress bar so the batch's position within the run
        // is visible at a glance, not just as two numbers. Sized to fit a
        // normal-width terminal; collapses gracefully on narrow panes.
        let frac = completed as f32 / total as f32;
        let bar_width = 10usize;
        let filled = ((frac * bar_width as f32).round() as usize).clamp(0, bar_width);
        spans.push(Span::styled(" ", Style::default()));
        if filled > 0 {
            spans.push(Span::styled(
                "█".repeat(filled),
                Style::default().fg(anim_color),
            ));
        }
        let empty = bar_width - filled;
        if empty > 0 {
            spans.push(Span::styled(
                "░".repeat(empty),
                Style::default().fg(dim_color()),
            ));
        }
        spans.push(Span::styled(
            format!(" {:>3}%", (frac * 100.0).round() as u8),
            Style::default().fg(anim_color),
        ));
    }

    if let Some(running) = running_summary {
        spans.push(Span::styled(
            format!(" · running: {}", running),
            Style::default().fg(dim_color()),
        ));
    }

    if let Some(tool_name) = last_completed.filter(|_| completed < total) {
        spans.push(Span::styled(
            format!(" · last done: {}", tool_name),
            Style::default().fg(dim_color()),
        ));
    }
}

/// One independently-droppable piece of a status row, tagged with the priority
/// that decides what survives when the terminal is too narrow to show it all.
///
/// Status rows used to be a single long concatenation that a trailing ellipsis
/// clipped from the right, so whatever sat at the end (context usage, cost, the
/// queued suffix) vanished first regardless of importance. Building rows from
/// segments lets the renderer drop the least useful piece first and guarantees
/// the high-priority facts survive down to very narrow widths.
struct StatusSegment {
    /// Lower values survive longer when space runs out.
    priority: u8,
    text: String,
    /// Shorter substitute tried when `text` does not fit, e.g. token totals
    /// without the per-direction breakdown or a model name without its
    /// provider prefix.
    short: Option<String>,
    color: Color,
    bold: bool,
}

const STATUS_SEGMENT_GAP: &str = " · ";

/// Below this width the header session strip may have dropped fragments to fit,
/// so the idle bottom row falls back to the full stats line. Above it the
/// session numbers (model, tokens, ctx, queue) live in the header strip and the
/// bottom row stays a clean activity/tip line.
const NARROW_STATUS_FALLBACK_WIDTH: u16 = 56;

impl StatusSegment {
    fn new(priority: u8, text: impl Into<String>, color: Color) -> Self {
        Self {
            priority,
            text: text.into(),
            short: None,
            color,
            bold: false,
        }
    }

    fn short(mut self, short: impl Into<String>) -> Self {
        self.short = Some(short.into());
        self
    }

    fn bold(mut self) -> Self {
        self.bold = true;
        self
    }

    /// The widest variant of this segment that fits in `budget` columns, or
    /// `None` when even the short form is too long.
    fn fit(&self, budget: usize) -> Option<&str> {
        use unicode_width::UnicodeWidthStr;
        if UnicodeWidthStr::width(self.text.as_str()) <= budget {
            return Some(self.text.as_str());
        }
        let short = self.short.as_deref()?;
        (UnicodeWidthStr::width(short) <= budget).then_some(short)
    }

    fn style(&self) -> Style {
        let style = Style::default().fg(self.color);
        if self.bold { style.bold() } else { style }
    }
}

/// Fit status segments into `max_width` columns, keeping as many as possible
/// by priority while rendering the survivors in their original display order.
///
/// Lower `priority` wins space first; ties are resolved by display order. A
/// dropped segment frees both its own width and the separator that would have
/// preceded it, so the joined line always lands exactly within budget instead
/// of being clipped mid-text by an ellipsis.
fn fit_status_segments(segments: &[StatusSegment], max_width: usize) -> Line<'static> {
    use unicode_width::UnicodeWidthStr;

    if max_width == 0 {
        return Line::default();
    }
    let gap_width = UnicodeWidthStr::width(STATUS_SEGMENT_GAP);

    let mut by_priority: Vec<usize> = (0..segments.len()).collect();
    by_priority.sort_by_key(|&index| (segments[index].priority, index));

    let mut kept: Vec<(usize, String)> = Vec::with_capacity(segments.len());
    let mut used = 0usize;
    for index in by_priority {
        let segment = &segments[index];
        let separator = if kept.is_empty() { 0 } else { gap_width };
        let budget = max_width.saturating_sub(used + separator);
        let Some(text) = segment.fit(budget) else {
            continue;
        };
        used += separator + UnicodeWidthStr::width(text);
        kept.push((index, text.to_string()));
    }

    kept.sort_by_key(|(index, _)| *index);
    let mut spans: Vec<Span<'static>> = Vec::with_capacity(kept.len() * 2);
    for (position, (index, text)) in kept.iter().enumerate() {
        if position > 0 {
            spans.push(Span::styled(
                STATUS_SEGMENT_GAP,
                Style::default().fg(dim_color()),
            ));
        }
        spans.push(Span::styled(text.clone(), segments[*index].style()));
    }
    Line::from(spans)
}

/// Compact `ctx=` fragment shared by the idle stats line and the tip line.
/// `42%` when the context window is known, the raw token count otherwise.
fn idle_context_fragment(app: &dyn TuiState, input_tokens: u64) -> String {
    match app.context_limit() {
        Some(limit) if limit > 0 => {
            let pct = ((input_tokens as f64 / limit as f64) * 100.0).round() as u64;
            format!("{}%", pct.min(999))
        }
        _ => format_stream_tokens(input_tokens),
    }
}

/// Return the compact context label shared by the idle tip and stats rows.
fn idle_context_suffix(app: &dyn TuiState) -> Option<String> {
    let (total_in, _) = app.total_session_tokens()?;
    (total_in > 0).then(|| format!(" · ctx={}", idle_context_fragment(app, total_in)))
}

/// Reserve space for context before selecting an idle tip, so truncation of a
/// long tip can never remove the context value that follows it.
fn idle_status_tip_line(
    app: &dyn TuiState,
    max_width: usize,
    elapsed_seconds: u64,
) -> Option<Line<'static>> {
    use unicode_width::UnicodeWidthStr;

    let suffix = idle_context_suffix(app);
    let suffix_width = suffix
        .as_deref()
        .map(UnicodeWidthStr::width)
        .unwrap_or_default();
    let tip_width = max_width.saturating_sub(suffix_width);
    let tip = occasional_status_tip(tip_width, elapsed_seconds)?;
    Some(compose_idle_status_tip(&tip, suffix.as_deref(), max_width))
}

fn compose_idle_status_tip(tip: &str, suffix: Option<&str>, max_width: usize) -> Line<'static> {
    use unicode_width::UnicodeWidthStr;

    let suffix_width = suffix.map(UnicodeWidthStr::width).unwrap_or_default();
    let tip_width = max_width.saturating_sub(suffix_width);
    let tip = truncate_preview(tip, tip_width);
    let mut spans = vec![Span::styled(tip, Style::default().fg(dim_color()))];
    if let Some(suffix) = suffix {
        spans.push(Span::styled(
            suffix.to_string(),
            Style::default().fg(muted_text_color()),
        ));
    }
    truncate_line_with_ellipsis(Line::from(spans), max_width)
}

/// Build the always-on idle session-stats line shown under the input: session
/// tokens, per-direction tokens, context usage, model identity, estimated cost
/// or provider window usage (when reported), and compaction count.
///
/// The row is assembled from priority-tagged segments and fitted to the
/// available width, so context usage and token totals are never pushed off the
/// edge by lower-value detail — that detail is dropped first instead.
///
/// Returns `None` for a fresh session (nothing to show yet) so the rotating
/// tips keep the line friendly instead of a zeroed counter.
fn idle_session_stats_line(app: &dyn TuiState, max_width: usize) -> Option<Line<'static>> {
    let (total_in, total_out) = app.total_session_tokens()?;
    let total = total_in.saturating_add(total_out);
    if total == 0 {
        return None;
    }

    let model_short = super::shorten_model_name(&app.provider_model());
    let context = (total_in > 0).then(|| idle_context_fragment(app, total_in));

    // Below this width even the shortest useful arrangement crowds itself; the
    // compact builder's hard-reserved `tokens · ctx` layout reads better.
    const COMPACT_FALLBACK_WIDTH: usize = 28;
    if max_width < COMPACT_FALLBACK_WIDTH {
        return Some(compact_idle_session_stats_line(
            &model_short,
            total,
            context.as_deref(),
            max_width,
        ));
    }

    let provider = app.provider_name();
    let provider_display = if provider.is_empty() {
        "auto".to_string()
    } else {
        provider.to_string()
    };

    // Priority 1 (context usage) > 2 (session tokens) > 3 (model identity) >
    // 4 (provider billing/window) > 5 (estimated cost, housekeeping).
    let mut segments: Vec<StatusSegment> = Vec::with_capacity(7);
    if !model_short.is_empty() {
        segments.push(
            StatusSegment::new(
                3,
                format!("{} \u{00b7} {}", provider_display, model_short),
                user_color(),
            )
            .short(model_short.clone())
            .bold(),
        );
    }
    segments.push(
        StatusSegment::new(
            2,
            format!(
                "\u{2301} {} total \u{00b7} \u{2191}{} \u{00b7} \u{2193}{}",
                format_stream_tokens(total),
                format_stream_tokens(total_in),
                format_stream_tokens(total_out),
            ),
            muted_text_color(),
        )
        .short(format!("\u{2301} {}", format_stream_tokens(total))),
    );
    if let Some(context) = context.as_deref() {
        segments.push(StatusSegment::new(
            1,
            format!("ctx={}", context),
            muted_text_color(),
        ));
    }

    let data = app.info_widget_data();
    if let Some(info) = data.usage_info.as_ref().filter(|info| info.available) {
        let usage = if matches!(info.provider, super::info_widget::UsageProvider::CostBased)
            && info.total_cost > 0.0
        {
            Some(format!("billed ${:.2}", info.total_cost))
        } else {
            let mut parts = Vec::new();
            if let Some(label) = info
                .primary_limit_label
                .as_deref()
                .filter(|_| info.five_hour > 0.0)
            {
                parts.push(format!("{} {:.0}%", label, info.five_hour * 100.0));
            }
            if let Some(label) = info
                .secondary_limit_label
                .as_deref()
                .filter(|_| info.seven_day > 0.0)
            {
                parts.push(format!("{} {:.0}%", label, info.seven_day * 100.0));
            }
            (!parts.is_empty()).then(|| parts.join(STATUS_SEGMENT_GAP))
        };
        if let Some(usage) = usage {
            segments.push(StatusSegment::new(4, usage, muted_text_color()));
        }
    }

    segments.push(StatusSegment::new(
        5,
        format!(
            "est. ${} @ ${}/M",
            super::info_widget::format_token_value_usd(total),
            super::info_widget::NOTIONAL_USD_PER_MTOK
        ),
        success_color(),
    ));

    let compactions = app.session_compaction_count();
    if compactions > 0 {
        segments.push(StatusSegment::new(
            5,
            format!(
                "{} compact{}",
                compactions,
                if compactions == 1 { "" } else { "s" }
            ),
            muted_text_color(),
        ));
    }

    let recovered = crate::alphacode_app_core::agent::provider_errors_auto_recovered();
    if recovered > 0 {
        segments.push(StatusSegment::new(
            5,
            format!("{} recovered", recovered),
            muted_text_color(),
        ));
    }

    // The status dot is always drawn, so the segments get only the width left
    // over after it.
    const DOT_WIDTH: usize = 2; // "● "
    let stats = fit_status_segments(&segments, max_width.saturating_sub(DOT_WIDTH));
    if stats.spans.is_empty() {
        return None;
    }

    let mut spans = vec![Span::styled(
        "\u{25cf} ",
        Style::default()
            .fg(success_color())
            .add_modifier(Modifier::BOLD),
    )];
    spans.extend(stats.spans);
    Some(Line::from(spans))
}

/// Build the narrow-terminal session summary, reserving width for token and
/// context metrics before spending the remainder on a shortened model name.
fn compact_idle_session_stats_line(
    model: &str,
    total_tokens: u64,
    context: Option<&str>,
    max_width: usize,
) -> Line<'static> {
    use unicode_width::UnicodeWidthStr;

    if max_width == 0 {
        return Line::default();
    }

    let token_count = format_stream_tokens(total_tokens);
    let metrics = match context {
        Some(context) if max_width < 24 => format!("{token_count} · ctx{context}"),
        Some(context) => format!("{token_count} tok · ctx={context}"),
        None if max_width < 24 => token_count,
        None => format!("{token_count} tok"),
    };
    let marker = "● ";
    let separator = " · ";
    let metrics_width = UnicodeWidthStr::width(metrics.as_str());
    let marker_width = UnicodeWidthStr::width(marker);
    let separator_width = UnicodeWidthStr::width(separator);
    let model_budget = max_width.saturating_sub(marker_width + separator_width + metrics_width);

    let mut spans = vec![Span::styled(
        marker,
        Style::default()
            .fg(success_color())
            .add_modifier(Modifier::BOLD),
    )];
    if model_budget >= 6 && !model.is_empty() {
        let model = truncate_preview(model, model_budget);
        if !model.is_empty() {
            spans.push(Span::styled(
                model,
                Style::default()
                    .fg(user_color())
                    .add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(separator));
        }
    }
    spans.push(Span::styled(
        metrics,
        Style::default().fg(muted_text_color()),
    ));
    truncate_line_with_ellipsis(Line::from(spans), max_width)
}

pub(super) fn draw_status(frame: &mut Frame, app: &dyn TuiState, area: Rect, pending_count: usize) {
    let elapsed = app.elapsed().map(|d| d.as_secs_f32()).unwrap_or(0.0);
    let stale_secs = app.time_since_activity().map(|d| d.as_secs_f32());
    let (cache_read, cache_creation) = app.streaming_cache_tokens();
    let user_turn_count = app.display_user_message_count();
    let (streaming_input_tokens, _) = app.streaming_tokens();
    let provider_name = app.provider_name();
    let upstream_provider = app.upstream_provider();
    let cache_ttl = app.cache_ttl_status();
    let kv_cache_problem = detect_kv_cache_problem(
        &provider_name,
        upstream_provider.as_deref(),
        user_turn_count,
        streaming_input_tokens,
        cache_read,
        cache_creation,
        cache_ttl.as_ref(),
    );

    let queued_suffix = if pending_count > 0 {
        format!(" · +{} queued", pending_count)
    } else {
        String::new()
    };

    let line = if let Some(build_progress) = crate::build::read_build_progress() {
        let spinner = super::activity_indicator(elapsed, 12.5);
        Line::from(vec![
            Span::styled(spinner, Style::default().fg(warning_color())),
            Span::styled(
                format!(" {}", build_progress),
                Style::default().fg(warning_color()),
            ),
        ])
    } else if let Some(remaining) = app.rate_limit_remaining() {
        let secs = remaining.as_secs();
        let spinner = super::activity_indicator(elapsed, 4.0);
        let time_str = if secs >= 3600 {
            let hours = secs / 3600;
            let mins = (secs % 3600) / 60;
            format!("{}h {}m", hours, mins)
        } else if secs >= 60 {
            let mins = secs / 60;
            let s = secs % 60;
            format!("{}m {}s", mins, s)
        } else {
            format!("{}s", secs)
        };
        Line::from(vec![
            Span::styled(spinner, Style::default().fg(warning_color())),
            Span::styled(
                format!(
                    " Rate limited. Auto-retry in {}...{}",
                    time_str, queued_suffix
                ),
                Style::default().fg(warning_color()),
            ),
        ])
    } else if app.is_processing() {
        let spinner = super::activity_indicator(elapsed, 12.5);

        match app.status() {
            ProcessingStatus::Idle => Line::from(""),
            ProcessingStatus::Sending => {
                let mut spans = vec![
                    Span::styled(spinner, Style::default().fg(ai_color())),
                    Span::styled(
                        format!(" Working/thinking… {}", format_elapsed(elapsed)),
                        Style::default().fg(dim_color()),
                    ),
                ];
                push_queued_suffix(&mut spans, &queued_suffix);
                Line::from(spans)
            }
            ProcessingStatus::Connecting(ref phase) => {
                let mut label = format!(
                    " {}… {}",
                    connection_phase_label(phase),
                    format_elapsed(elapsed)
                );
                append_transport_context(&mut label, app);
                // "Suspiciously long" is measured per connection attempt, not
                // across the whole turn, so later round-trips don't immediately
                // render yellow just because the turn has been running a while.
                let phase_elapsed = app
                    .connection_phase_elapsed()
                    .map_or(elapsed, |d| d.as_secs_f32());
                let label_color = match phase {
                    crate::message::ConnectionPhase::Retrying { .. } => warning_color(),
                    crate::message::ConnectionPhase::Authenticating if phase_elapsed > 10.0 => {
                        warning_color()
                    }
                    crate::message::ConnectionPhase::Connecting if phase_elapsed > 10.0 => {
                        warning_color()
                    }
                    crate::message::ConnectionPhase::SendingRequest if phase_elapsed > 10.0 => {
                        warning_color()
                    }
                    _ => dim_color(),
                };
                let mut spans = vec![
                    Span::styled(spinner, Style::default().fg(ai_color())),
                    Span::styled(label, Style::default().fg(label_color)),
                ];
                push_queued_suffix(&mut spans, &queued_suffix);
                Line::from(spans)
            }
            ProcessingStatus::Thinking(_start) => {
                let mut label = format!(" thinking… {}", format_elapsed(elapsed));
                append_transport_context(&mut label, app);
                let mut spans = vec![
                    Span::styled(spinner, Style::default().fg(ai_color())),
                    Span::styled(label, Style::default().fg(dim_color())),
                ];
                push_queued_suffix(&mut spans, &queued_suffix);
                Line::from(spans)
            }
            ProcessingStatus::Streaming => {
                let time_str = format_elapsed(elapsed);
                let (input_tokens, output_tokens) = app.streaming_tokens();
                let stream_message_ended = app.stream_message_ended();

                // Built as priority-tagged segments so a narrow row degrades
                // gracefully: transport detail and per-direction tokens drop
                // first, while the cache warning, liveness label, throughput,
                // and context usage survive (the same `ctx=` reading as the
                // idle line, so it reads identically in both states).
                let mut segments: Vec<StatusSegment> = Vec::with_capacity(6);
                segments.push(StatusSegment::new(
                    2,
                    streaming_liveness_label(time_str, stale_secs, stream_message_ended),
                    muted_text_color(),
                ));
                if let Some(tps) = app.output_tps() {
                    segments.push(StatusSegment::new(
                        3,
                        format!("{:.1} tok/s", tps),
                        muted_text_color(),
                    ));
                }
                if input_tokens > 0 {
                    let ctx = match app.context_limit() {
                        Some(limit) if limit > 0 => {
                            let pct = ((input_tokens as f64 / limit as f64) * 100.0).round() as u64;
                            format!("ctx={}%", pct.min(999))
                        }
                        _ => format!("ctx={}", format_stream_tokens(input_tokens)),
                    };
                    segments.push(StatusSegment::new(1, ctx, muted_text_color()));
                }
                if input_tokens > 0 || output_tokens > 0 {
                    segments.push(
                        StatusSegment::new(
                            4,
                            format!(
                                "\u{2191}{} \u{00b7} \u{2193}{}",
                                format_stream_tokens(input_tokens),
                                format_stream_tokens(output_tokens)
                            ),
                            muted_text_color(),
                        )
                        .short(format!("\u{2193}{}", format_stream_tokens(output_tokens))),
                    );
                }
                for label in transport_context_labels(app) {
                    segments.push(StatusSegment::new(6, label, dim_color()));
                }
                if let Some(problem) = kv_cache_problem {
                    segments.insert(
                        0,
                        StatusSegment::new(
                            0,
                            format!("\u{26a0} {}", cache_miss_label(problem)),
                            warning_color(),
                        )
                        .bold(),
                    );
                }

                let spans = streaming_status_spans(
                    spinner,
                    segments,
                    queued_suffix_for_segment(&queued_suffix),
                    area.width as usize,
                );
                Line::from(spans)
            }
            ProcessingStatus::WaitingForNetwork { listener } => {
                let mut spans = vec![
                    Span::styled("↻ ", Style::default().fg(warning_color())),
                    Span::styled(
                        format!(
                            "network disconnected, waiting to retry · {} · {}",
                            listener,
                            format_elapsed(elapsed)
                        ),
                        Style::default().fg(warning_color()),
                    ),
                ];
                push_queued_suffix(&mut spans, &queued_suffix);
                Line::from(spans)
            }
            ProcessingStatus::RunningTool(ref name) => {
                // The leading glyph is a single pulsing dot so the eye lands
                // on the tool name first; the trailing marquee is a thin
                // gradient sweep that reads as liveness without competing
                // with the bold tool label.
                let trailing_width = 4;
                let anim_color = animated_tool_color(elapsed);
                let decorative = crate::perf::tui_policy().enable_decorative_animations;
                // When decorative animations are disabled we still nudge the bar
                // forward at a slow "liveness" rate so a long-running tool (e.g.
                // bash) reads as alive instead of frozen.
                let bar_speed = if decorative {
                    1.6
                } else {
                    crate::alphacode_tui_style::theme::LIVENESS_INDICATOR_FPS
                        / trailing_width as f32
                };
                let progress = elapsed * bar_speed % 1.0;
                let head_pos = ((progress * trailing_width as f32) as usize) % trailing_width;
                // Trailing marquee: a gradient sweep `▁▂▃▄▃▂▁`-style wave
                // where the brightest cell travels along a thin underscore
                // track. The wave is built as discrete span steps so the
                // highlight can carry the brand accent while the dim cells
                // sit in `dim_color`, giving the bar a polished "alive" feel
                // without the harshness of a single blinking dot.
                let marquee_glyphs = ['\u{2581}', '\u{2582}', '\u{2583}', '\u{2584}'];
                let mut trailing_spans: Vec<Span<'static>> = Vec::with_capacity(trailing_width);
                let dim = dim_color();
                for i in 0..trailing_width {
                    // Distance from the moving head (0..trailing_width-1).
                    // Cell at the head is the brightest; cells further behind
                    // fade into the dim color.
                    let distance = (head_pos + trailing_width - i) % trailing_width;
                    let glyph = if distance == 0 {
                        '\u{2585}' // bright head: top-half block
                    } else {
                        marquee_glyphs[trailing_width - 1 - distance]
                    };
                    let style = if distance == 0 {
                        Style::default().fg(anim_color)
                    } else {
                        Style::default().fg(dim)
                    };
                    trailing_spans.push(Span::styled(glyph.to_string(), style));
                }

                let batch_prog = app.batch_progress();
                let is_batch = name == "batch";
                // For batch: compute initial total from the streaming tool call input
                let batch_total_initial = if is_batch {
                    app.streaming_tool_calls()
                        .last()
                        .and_then(|tc| tc.input.get("tool_calls"))
                        .and_then(|v| v.as_array())
                        .map(|a| a.len())
                } else {
                    None
                };
                let tool_detail = if is_batch {
                    None // batch always uses progress display
                } else {
                    app.streaming_tool_calls()
                        .last()
                        .map(get_tool_activity_detail)
                        .filter(|s| !s.is_empty())
                };
                let experimental_notice = app.active_experimental_feature_notice();
                let subagent = app.subagent_status();

                // Leading marker: a single pulsing dot (● / ◉ alternating on
                // a 1s cycle) so the tool name has a clear visual anchor
                // without the noisy "──●" track on both sides.
                let pulse_phase = (elapsed * 2.0) as i32;
                let pulse_glyph = if pulse_phase % 2 == 0 {
                    "\u{25C9}" // ◉ - filled circle with center dot
                } else {
                    "\u{25CB}" // ○ - hollow ring
                };
                let mut spans: Vec<Span<'static>> = vec![
                    Span::styled(pulse_glyph, Style::default().fg(anim_color)),
                    Span::styled(" ", Style::default()),
                    Span::styled(name.to_string(), Style::default().fg(anim_color).bold()),
                    Span::styled(" ", Style::default()),
                ];
                spans.extend(trailing_spans);

                // For batch tool: show "completed/total · last_tool" progress
                // right after the marquee; it is the point of the row, so it
                // is never a candidate for dropping.
                if is_batch {
                    append_batch_progress_spans(
                        &mut spans,
                        anim_color,
                        batch_prog,
                        batch_total_initial,
                    );
                }

                // Everything after the head competes for the remaining width
                // by priority, so a long tool detail can no longer push the
                // elapsed time, the queued count, or the cache warning off
                // the row — the least useful piece is dropped first.
                let mut segments: Vec<StatusSegment> = Vec::with_capacity(7);
                if let Some(detail) = tool_detail {
                    segments.push(StatusSegment::new(2, detail, dim_color()));
                }
                if let Some(notice) = experimental_notice {
                    segments.push(
                        StatusSegment::new(1, format!("\u{26a0} {}", notice), warning_color())
                            .bold(),
                    );
                }
                if let Some(status) = subagent {
                    segments.push(StatusSegment::new(4, format!("({})", status), dim_color()));
                }
                if let Some(problem) = kv_cache_problem {
                    segments.push(StatusSegment::new(
                        3,
                        format!("\u{26a0} {}", cache_miss_label(problem)),
                        warning_color(),
                    ));
                }
                segments.push(StatusSegment::new(5, format_elapsed(elapsed), dim_color()));
                if let Some(label) = queued_suffix_for_segment(&queued_suffix) {
                    segments.push(StatusSegment::new(6, label, queued_color()));
                }
                for label in transport_context_labels(app) {
                    segments.push(StatusSegment::new(7, label, dim_color()));
                }
                segments.push(StatusSegment::new(
                    8,
                    format!("{} bg", crate::alphacode_tui_core::keybind::alt_chord("B")),
                    dim_color(),
                ));

                let head_width: usize = spans
                    .iter()
                    .map(|span| unicode_width::UnicodeWidthStr::width(span.content.as_ref()))
                    .sum();
                let fitted = fit_status_segments(
                    &segments,
                    (area.width as usize).saturating_sub(head_width),
                );
                if !fitted.spans.is_empty() {
                    spans.push(Span::styled(
                        STATUS_SEGMENT_GAP,
                        Style::default().fg(dim_color()),
                    ));
                    spans.extend(fitted.spans);
                }
                Line::from(spans)
            }
        }
    } else if let Some((total_in, total_out)) = app.total_session_tokens() {
        let total = total_in.saturating_add(total_out);
        if let Some(warning) = occasional_session_history_warning(
            total,
            app.session_compaction_count(),
            app.context_limit(),
            area.width as usize,
            app.animation_elapsed() as u64,
        ) {
            let severe_token_threshold = app
                .context_limit()
                .and_then(|limit| u64::try_from(limit).ok())
                .map(|limit| limit.saturating_mul(3))
                .unwrap_or(1_000_000);
            let warning_color =
                if total >= severe_token_threshold || app.session_compaction_count() >= 3 {
                    error_color()
                } else {
                    warning_color()
                };
            Line::from(vec![
                Span::styled("⚠ ", Style::default().fg(warning_color)),
                Span::styled(warning, Style::default().fg(warning_color)),
            ])
        } else if let Some(tip) =
            idle_status_tip_line(app, area.width as usize, app.animation_elapsed() as u64)
        {
            // Keep the context-usage segment visible even while a tip shows:
            // tips used to displace the `ctx=` reading entirely, hiding the
            // most important number for up to 12s at a time.
            tip
        } else if area.width < NARROW_STATUS_FALLBACK_WIDTH {
            // The session numbers (model, tokens, ctx, queue) live in the
            // header strip now, so the bottom row stays a clean activity/tip
            // line. On narrow terminals the header strip may have dropped
            // fragments, so fall back to the stats line there.
            idle_session_stats_line(app, area.width as usize).unwrap_or_default()
        } else {
            Line::from("")
        }
    } else if let Some(tip) =
        idle_status_tip_line(app, area.width as usize, app.animation_elapsed() as u64)
    {
        tip
    } else {
        Line::from("")
    };

    crate::memory::check_staleness();
    let line = truncate_line_with_ellipsis(line, area.width as usize);

    // Subtle background highlight for the status line to visually separate it
    // from the transcript above. Uses a very dim brand-tinted background.
    // `right_fact_cell_is_blank` knows this exact colour, so the session-fact
    // stack can still overlay the row's empty right-hand suffix.
    if area.height > 0 && area.width > 0 {
        let status_bg = STATUS_ROW_BG;
        for x in area.left()..area.right() {
            for y in area.top()..area.bottom() {
                let cell = &mut frame.buffer_mut()[(x, y)];
                if cell.symbol() == " " || cell.symbol().is_empty() {
                    cell.set_bg(status_bg);
                }
            }
        }
    }

    if app.centered_mode() {
        frame.render_widget(Paragraph::new(line.alignment(Alignment::Center)), area);
        return;
    }
    frame.render_widget(Paragraph::new(line), area);
}

fn truncate_line_with_ellipsis(mut line: Line<'static>, max_width: usize) -> Line<'static> {
    if line.width() <= max_width {
        return line;
    }
    if max_width == 0 {
        return Line::default();
    }

    let ellipsis_style = line
        .spans
        .iter()
        .rev()
        .find(|span| !span.content.is_empty())
        .map(|span| span.style)
        .unwrap_or_else(|| Style::default().fg(dim_color()));
    super::viewport::truncate_line_in_place_to_width(&mut line, max_width - 1);
    line.spans.push(Span::styled("…", ellipsis_style));
    line
}

/// Append the "+N queued" suffix span (in the queued accent color) when there
/// are queued follow-up messages. Centralizes the repeated check/styling shared
/// by every processing-status branch in `draw_status`.
fn push_queued_suffix(spans: &mut Vec<Span<'static>>, queued_suffix: &str) {
    if !queued_suffix.is_empty() {
        spans.push(Span::styled(
            queued_suffix.to_string(),
            Style::default().fg(queued_color()),
        ));
    }
}

/// Same queued info as `queued_suffix`, but as a standalone segment text (the
/// leading `" · "` is the fitter's job to re-insert between kept segments).
fn queued_suffix_for_segment(queued_suffix: &str) -> Option<String> {
    let label = queued_suffix
        .trim_start_matches(' ')
        .trim_start_matches('·');
    let label = label.trim();
    (!label.is_empty()).then(|| label.to_string())
}

/// Compact, human-readable cache-miss label: "12k cache miss", "800 cache
/// miss", or a bare "kv cache miss" when the provider didn't report a size.
fn cache_miss_label(problem: crate::alphacode_tui::tui::KvCacheProblem) -> String {
    let miss_tokens = problem.affected_tokens.unwrap_or(0);
    let miss_str = if miss_tokens >= 1000 {
        format!("{}k", miss_tokens / 1000)
    } else if miss_tokens > 0 {
        format!("{}", miss_tokens)
    } else {
        "kv".to_string()
    };
    format!("{} cache miss", miss_str)
}

/// Assemble the streaming status row: activity spinner, then as many
/// priority-tagged segments as fit in `max_width`, then the queued suffix
/// (which outranks transport detail but yields to the core readings).
fn streaming_status_spans(
    spinner: &'static str,
    mut segments: Vec<StatusSegment>,
    queued_label: Option<String>,
    max_width: usize,
) -> Vec<Span<'static>> {
    use unicode_width::UnicodeWidthStr;

    let mut spans = Vec::with_capacity(6);
    spans.push(Span::styled(
        spinner,
        Style::default().fg(ai_color()).add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::raw(" "));
    if let Some(label) = queued_label {
        segments.push(StatusSegment::new(5, label, queued_color()));
    }
    let prefix_width = UnicodeWidthStr::width(spinner) + 1; // spinner + gap
    let fitted = fit_status_segments(&segments, max_width.saturating_sub(prefix_width));
    spans.extend(fitted.spans);
    spans
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::style::Modifier;

    #[test]
    fn right_fact_stack_shifts_up_as_a_unit_when_bottom_row_is_occupied() {
        let area = Rect::new(0, 0, 40, 5);
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        for x in 12..40 {
            buffer[(x, 4)].set_symbol("x");
        }
        let lines = ["oauth", "model", "dir", "context"]
            .into_iter()
            .map(|text| RightFactLine::new(vec![Span::raw(text)]).expect("fact line"))
            .collect();

        let placements = right_fact_placements(
            &buffer,
            lines,
            0,
            5,
            0,
            40,
            Rect::new(0, 0, 40, 3),
            false,
            None,
        );
        let rows = placements
            .iter()
            .map(|placement| placement.area.y)
            .collect::<Vec<_>>();
        assert_eq!(rows, vec![0, 1, 2, 3]);
        assert!(placements.iter().all(|placement| placement.area.y != 4));
    }

    #[test]
    fn right_fact_stack_never_leaves_an_occupied_row_between_facts() {
        let area = Rect::new(0, 0, 40, 7);
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        buffer[(39, 4)].set_symbol("x");
        let lines = ["oauth", "model", "dir", "context"]
            .into_iter()
            .map(|text| RightFactLine::new(vec![Span::raw(text)]).expect("fact line"))
            .collect();

        let placements = right_fact_placements(
            &buffer,
            lines,
            0,
            7,
            0,
            40,
            Rect::new(0, 0, 40, 5),
            false,
            None,
        );
        let rows = placements
            .iter()
            .map(|placement| placement.area.y)
            .collect::<Vec<_>>();
        assert_eq!(rows, vec![0, 1, 2, 3]);
        assert_eq!(
            placements
                .iter()
                .map(|placement| placement.line.spans[0].content.as_ref())
                .collect::<Vec<_>>(),
            vec!["oauth", "model", "dir", "context"]
        );
    }

    #[test]
    fn right_fact_stack_collision_state_space_is_contiguous_or_hidden() {
        const HEIGHT: u16 = 8;
        const STACK_HEIGHT: u16 = 4;

        for occupied_mask in 0_u16..(1 << HEIGHT) {
            let area = Rect::new(0, 0, 40, HEIGHT);
            let mut buffer = ratatui::buffer::Buffer::empty(area);
            for row in 0..HEIGHT {
                if occupied_mask & (1 << row) != 0 {
                    buffer[(39, row)].set_symbol("x");
                }
            }
            let lines = ["oauth", "model", "dir", "context"]
                .into_iter()
                .map(|text| RightFactLine::new(vec![Span::raw(text)]).expect("fact line"))
                .collect();

            let placements = right_fact_placements(
                &buffer,
                lines,
                0,
                HEIGHT,
                0,
                40,
                Rect::new(0, 0, 40, HEIGHT),
                false,
                None,
            );
            let expected_top = (0..=HEIGHT - STACK_HEIGHT).rev().find(|&start| {
                (start..start + STACK_HEIGHT).all(|row| occupied_mask & (1 << row) == 0)
            });

            match expected_top {
                Some(start) => {
                    assert_eq!(
                        placements.len(),
                        STACK_HEIGHT as usize,
                        "mask {occupied_mask:08b}"
                    );
                    assert_eq!(
                        placements
                            .iter()
                            .map(|placement| placement.area.y)
                            .collect::<Vec<_>>(),
                        (start..start + STACK_HEIGHT).collect::<Vec<_>>(),
                        "mask {occupied_mask:08b}"
                    );
                    assert_eq!(
                        placements
                            .iter()
                            .map(|placement| placement.line.spans[0].content.as_ref())
                            .collect::<Vec<_>>(),
                        vec!["oauth", "model", "dir", "context"],
                        "mask {occupied_mask:08b}"
                    );
                }
                None => assert!(placements.is_empty(), "mask {occupied_mask:08b}"),
            }
        }
    }

    #[test]
    fn right_fact_stack_treats_styled_blank_cells_as_occupied() {
        let area = Rect::new(0, 0, 32, 2);
        let mut buffer = ratatui::buffer::Buffer::empty(area);
        for x in 12..32 {
            buffer[(x, 1)].set_bg(Color::Blue);
        }
        let line = RightFactLine::new(vec![Span::raw("context")]).expect("fact line");
        let placements = right_fact_placements(
            &buffer,
            vec![line],
            0,
            2,
            0,
            32,
            Rect::new(0, 0, 32, 1),
            false,
            None,
        );
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].area.y, 0);
    }

    #[test]
    fn right_fact_stack_never_draws_over_the_input_cursor() {
        let area = Rect::new(0, 0, 32, 2);
        let buffer = ratatui::buffer::Buffer::empty(area);
        let line = RightFactLine::new(vec![Span::raw("context")]).expect("fact line");
        let placements = right_fact_placements(
            &buffer,
            vec![line],
            0,
            2,
            0,
            32,
            Rect::new(0, 0, 32, 1),
            false,
            Some(Position::new(28, 1)),
        );
        assert_eq!(placements.len(), 1);
        assert_eq!(placements[0].area.y, 0);
    }

    #[test]
    fn overscroll_provider_display_is_credential_neutral() {
        // The credential (OAuth vs API key) is reported by the adjacent auth
        // chip from canonical resolution; the provider name must not bake in a
        // credential or the two can contradict (e.g. "Claude OAuth · API key").
        assert_eq!(overscroll_provider_display("claude"), "Claude");
        assert_eq!(overscroll_provider_display("anthropic"), "Anthropic");
        assert!(!overscroll_provider_display("claude").contains("OAuth"));
        assert!(!overscroll_provider_display("anthropic").contains("API"));
    }

    #[test]
    fn session_history_warning_is_clear_and_occasional() {
        assert!(occasional_session_history_warning(249_999, 0, None, 100, 0).is_none());
        assert!(occasional_session_history_warning(300_000, 0, None, 63, 0).is_none());
        assert!(occasional_session_history_warning(300_000, 0, None, 100, 10).is_none());
        assert!(occasional_session_history_warning(199_999, 0, Some(200_000), 100, 0).is_none());
        assert!(occasional_session_history_warning(300_000, 0, Some(400_000), 100, 0).is_none());
        assert!(occasional_session_history_warning(450_000, 0, Some(400_000), 100, 0).is_none());

        let warning = occasional_session_history_warning(2_500_000, 4, Some(500_000), 100, 0)
            .expect("large sessions should get a brief reminder");
        assert!(warning.contains("Session history: 2.5M tokens processed and 4 compacts"));
        assert!(warning.contains("/clear starts fresh context"));
        assert!(!warning.contains("Context usage"));
    }

    #[test]
    fn compact_session_stats_preserve_context_and_fit_narrow_widths() {
        for width in [20, 32, 48, 80] {
            let line = compact_idle_session_stats_line(
                "claude-sonnet-long-model-name",
                10_000,
                Some("42%"),
                width,
            );
            let text = joined(&line.spans);

            assert!(
                line.width() <= width,
                "line exceeded {width} columns: {text:?}"
            );
            assert!(
                text.contains("42%"),
                "context was clipped at {width}: {text:?}"
            );
            assert!(
                text.contains("10k"),
                "token total was clipped at {width}: {text:?}"
            );
        }
    }

    #[test]
    fn compact_session_stats_respect_zero_width() {
        let line = compact_idle_session_stats_line("model", 42, Some("10%"), 0);
        assert!(line.spans.is_empty());
    }

    #[test]
    fn fit_status_segments_keeps_high_priority_fragments_at_narrow_widths() {
        let segments = vec![
            StatusSegment::new(3, "claude \u{00b7} sonnet", user_color()).short("sonnet"),
            StatusSegment::new(
                2,
                "\u{2301} 24k total \u{00b7} \u{2191}18k \u{00b7} \u{2193}6k",
                muted_text_color(),
            )
            .short("\u{2301} 24k"),
            StatusSegment::new(1, "ctx=42%", muted_text_color()),
        ];

        for width in 1..=48 {
            let line = fit_status_segments(&segments, width);
            assert!(
                line.width() <= width,
                "width {width}: {:?}",
                joined(&line.spans)
            );
        }

        let narrow = joined(&fit_status_segments(&segments, 10).spans);
        assert!(narrow.contains("ctx=42%"), "{narrow}");
        assert!(!narrow.contains("24k total"), "{narrow}");

        // 48 columns is not enough for the provider-prefixed model name *and*
        // the full token breakdown, so the fitter shortens the model first.
        let squeezed = joined(&fit_status_segments(&segments, 48).spans);
        assert!(squeezed.contains("sonnet"), "{squeezed}");
        assert!(squeezed.contains("\u{2191}18k"), "{squeezed}");
        assert!(squeezed.contains("ctx=42%"), "{squeezed}");

        // With room to spare, every segment renders in its full form.
        let wide = joined(&fit_status_segments(&segments, 60).spans);
        assert!(wide.contains("claude \u{00b7} sonnet"), "{wide}");
        assert!(wide.contains("\u{2191}18k"), "{wide}");
        assert!(wide.contains("ctx=42%"), "{wide}");
    }

    #[test]
    fn fit_status_segments_prefers_short_forms_before_dropping() {
        let segments = vec![
            StatusSegment::new(
                2,
                "\u{2301} 24k total \u{00b7} \u{2191}18k \u{00b7} \u{2193}6k",
                muted_text_color(),
            )
            .short("\u{2301} 24k"),
            StatusSegment::new(1, "ctx=42%", muted_text_color()),
        ];

        // The long token form needs 24 columns, the short one 5; with 16 the
        // fitter must fall back to the short form rather than drop the tokens.
        let line = fit_status_segments(&segments, 16);
        assert_eq!(joined(&line.spans), "\u{2301} 24k \u{00b7} ctx=42%");
        assert!(line.width() <= 16);
    }

    #[test]
    fn fit_status_segments_orders_survivors_by_display_not_priority() {
        let segments = vec![
            StatusSegment::new(2, "model", user_color()),
            StatusSegment::new(1, "ctx=42%", muted_text_color()),
        ];
        let line = fit_status_segments(&segments, 40);
        assert_eq!(joined(&line.spans), "model \u{00b7} ctx=42%");
    }

    #[test]
    fn fit_status_segments_handles_zero_width() {
        let line = fit_status_segments(&[StatusSegment::new(1, "x", dim_color())], 0);
        assert!(line.spans.is_empty());
    }

    #[test]
    fn idle_status_tip_truncates_before_the_context_suffix() {
        let line = compose_idle_status_tip(
            "A long tip that should leave room for session context",
            Some(" · ctx=42%"),
            36,
        );
        let text = joined(&line.spans);

        assert!(line.width() <= 36, "line exceeded its width: {text:?}");
        assert!(
            text.ends_with(" · ctx=42%"),
            "context was clipped: {text:?}"
        );
        assert!(
            text.contains("..."),
            "long tip should be shortened: {text:?}"
        );
    }

    #[test]
    fn command_suggestion_window_start_scrolls_after_visible_limit() {
        let limit = app::COMMAND_SUGGESTION_VISIBLE_LIMIT;
        assert_eq!(command_suggestion_window_start(0, limit + 3), 0);
        assert_eq!(command_suggestion_window_start(limit - 1, limit + 3), 0);
        assert_eq!(command_suggestion_window_start(limit, limit + 3), 1);
        assert_eq!(command_suggestion_window_start(limit + 2, limit + 3), 3);
    }

    #[test]
    fn command_suggestions_overlay_prefers_space_below_input() {
        let frame = Rect::new(0, 0, 80, 20);
        let input_area = Rect::new(0, 10, 80, 1);

        assert_eq!(
            command_suggestions_overlay_rect(input_area, 3, frame),
            Some(Rect::new(0, 11, 80, 3))
        );
    }

    #[test]
    fn command_suggestions_overlay_flips_above_at_terminal_bottom() {
        let frame = Rect::new(0, 0, 80, 20);
        let input_area = Rect::new(0, 19, 80, 1);

        assert_eq!(
            command_suggestions_overlay_rect(input_area, 3, frame),
            Some(Rect::new(0, 16, 80, 3))
        );
    }

    #[test]
    fn command_suggestions_overlay_handles_zero_lines_and_no_space() {
        let frame = Rect::new(0, 0, 80, 20);
        let input_area = Rect::new(0, 10, 80, 1);
        assert_eq!(command_suggestions_overlay_rect(input_area, 0, frame), None);

        // Input fills the whole frame: nowhere to float, so no popover.
        let full = Rect::new(0, 0, 80, 20);
        assert_eq!(command_suggestions_overlay_rect(full, 3, frame), None);
    }

    /// Concatenated text of a span list. The batch progress row is assembled
    /// from several styled fragments (count, bar, percentage, status), so tests
    /// assert the rendered text rather than an internal span count.
    fn joined(spans: &[Span<'static>]) -> String {
        spans.iter().map(|span| span.content.as_ref()).collect()
    }

    /// `rgb()` quantizes on 256-color terminals; normalize so color assertions
    /// work on every host.
    fn as_rgb_triple(color: Color) -> (u8, u8, u8) {
        match color {
            Color::Rgb(r, g, b) => (r, g, b),
            Color::Indexed(index) => crate::alphacode_tui_style::color::indexed_to_rgb(index),
            other => panic!("expected an RGB-mappable color, got {other:?}"),
        }
    }

    #[test]
    fn batch_progress_spans_use_batch_chroma_for_initial_count() {
        let mut spans = Vec::new();
        let anim_color = rgb(12, 34, 56);

        append_batch_progress_spans(&mut spans, anim_color, None, Some(3));

        assert_eq!(joined(&spans), " · 0/3 done ░░░░░░░░░░   0%");
        let count = spans
            .iter()
            .find(|span| span.content.as_ref().contains("0/3 done"))
            .expect("count span");
        assert_eq!(count.style.fg, Some(anim_color));
        assert!(count.style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn batch_progress_spans_make_last_completed_explicit() {
        let mut spans = Vec::new();

        append_batch_progress_spans(
            &mut spans,
            rgb(120, 130, 140),
            Some(crate::bus::BatchProgress {
                session_id: "s".to_string(),
                tool_call_id: "tc".to_string(),
                total: 3,
                completed: 1,
                last_completed: Some("read".to_string()),
                running: Vec::new(),
                subcalls: Vec::new(),
            }),
            Some(3),
        );

        assert_eq!(
            joined(&spans),
            " · 1/3 done ███░░░░░░░  33% · last done: read"
        );
    }

    #[test]
    fn batch_progress_spans_hide_last_completed_when_batch_finished() {
        let mut spans = Vec::new();

        append_batch_progress_spans(
            &mut spans,
            rgb(120, 130, 140),
            Some(crate::bus::BatchProgress {
                session_id: "s".to_string(),
                tool_call_id: "tc".to_string(),
                total: 3,
                completed: 3,
                last_completed: Some("read".to_string()),
                running: Vec::new(),
                subcalls: Vec::new(),
            }),
            Some(3),
        );

        // A finished batch reports only its position; "last done" would be
        // redundant once the bar already reads 100%.
        assert_eq!(joined(&spans), " · 3/3 done ██████████ 100%");
    }

    #[test]
    fn batch_progress_spans_show_running_subcall_detail() {
        let mut spans = Vec::new();

        append_batch_progress_spans(
            &mut spans,
            rgb(120, 130, 140),
            Some(crate::bus::BatchProgress {
                session_id: "s".to_string(),
                tool_call_id: "tc".to_string(),
                total: 2,
                completed: 0,
                last_completed: None,
                running: vec![crate::message::ToolCall {
                    id: "batch-1-bash".to_string(),
                    name: "bash".to_string(),
                    input: serde_json::json!({"command": "cargo test -p alphacode"}),
                    intent: None,
                    thought_signature: None,
                }],
                subcalls: Vec::new(),
            }),
            Some(2),
        );

        assert_eq!(
            joined(&spans),
            " · 0/2 done ░░░░░░░░░░   0% · running: #1 bash"
        );
    }

    #[test]
    fn batch_progress_spans_show_multiple_running_subcalls() {
        let mut spans = Vec::new();

        append_batch_progress_spans(
            &mut spans,
            rgb(120, 130, 140),
            Some(crate::bus::BatchProgress {
                session_id: "s".to_string(),
                tool_call_id: "tc".to_string(),
                total: 3,
                completed: 0,
                last_completed: None,
                running: vec![
                    crate::message::ToolCall {
                        id: "batch-2-grep".to_string(),
                        name: "grep".to_string(),
                        input: serde_json::json!({"pattern": "foo", "path": "src"}),
                        intent: None,
                        thought_signature: None,
                    },
                    crate::message::ToolCall {
                        id: "batch-1-bash".to_string(),
                        name: "bash".to_string(),
                        input: serde_json::json!({"command": "cargo build --release --workspace"}),
                        intent: None,
                        thought_signature: None,
                    },
                    crate::message::ToolCall {
                        id: "batch-3-read".to_string(),
                        name: "read".to_string(),
                        input: serde_json::json!({"file_path": "README.md"}),
                        intent: None,
                        thought_signature: None,
                    },
                ],
                subcalls: Vec::new(),
            }),
            Some(3),
        );

        assert_eq!(
            joined(&spans),
            " · 0/3 done ░░░░░░░░░░   0% · running: #1 bash +2"
        );
    }

    #[test]
    fn connection_phase_waiting_label_is_generic_response_wait() {
        assert_eq!(
            connection_phase_label(&ConnectionPhase::WaitingForResponse),
            "waiting for response"
        );
    }

    #[test]
    fn streaming_liveness_label_shows_quiet_stream_warning_before_message_end() {
        assert_eq!(
            streaming_liveness_label("4.2s".to_string(), Some(3.4), false),
            "(quiet 3s) · 4.2s"
        );
        assert_eq!(
            streaming_liveness_label("12.0s".to_string(), Some(12.1), false),
            "(idle 12s) · 12.0s"
        );
        // Past the stall threshold the warning escalates with a ⚠ marker.
        assert_eq!(
            streaming_liveness_label("20.0s".to_string(), Some(16.2), false),
            "⚠ stalled 16s · 20.0s"
        );
    }

    #[test]
    fn streaming_liveness_label_suppresses_quiet_stream_warning_after_message_end() {
        assert_eq!(
            streaming_liveness_label("4.2s".to_string(), Some(3.4), true),
            "4.2s"
        );
        assert_eq!(
            streaming_liveness_label("12.0s".to_string(), Some(12.1), true),
            "12.0s"
        );
    }

    #[test]
    fn streaming_status_spans_keep_spinner_while_finalizing() {
        let spans = streaming_status_spans(
            "⠋",
            vec![StatusSegment::new(2, "4.2s", muted_text_color())],
            Some("+1 queued".to_string()),
            80,
        );

        assert_eq!(spans[0].content.as_ref(), "⠋");
        assert_eq!(spans[1].content.as_ref(), " ");
        let joined = joined(&spans);
        assert_eq!(joined, "⠋ 4.2s · +1 queued");
    }

    #[test]
    fn streaming_status_spans_keep_spinner_after_message_end_while_finalizing() {
        let spans = streaming_status_spans(
            "⠋",
            vec![StatusSegment::new(2, "finalizing", muted_text_color())],
            None,
            80,
        );

        assert_eq!(spans[0].content.as_ref(), "⠋");
        let joined = joined(&spans);
        assert_eq!(joined, "⠋ finalizing");
    }

    /// On a narrow row the streaming status must degrade by priority instead
    /// of clipping its tail: the queued count (a lower-priority hint) yields
    /// before the liveness label, and the whole row stays within budget.
    #[test]
    fn streaming_status_spans_drop_low_priority_segments_before_clipping() {
        let build = |width: usize| {
            streaming_status_spans(
                "⠋",
                vec![
                    StatusSegment::new(1, "ctx=42%", muted_text_color()),
                    StatusSegment::new(6, "localhost:8080", dim_color()),
                ],
                Some("+3 queued".to_string()),
                width,
            )
        };

        let wide = joined(&build(80));
        assert!(wide.contains("ctx=42%"), "{wide}");
        assert!(wide.contains("localhost:8080"), "{wide}");
        assert!(wide.contains("+3 queued"), "{wide}");

        // Just enough for the context reading and the queued count: the
        // transport detail must be the one that yields, and nothing may
        // overflow into an ellipsis clip.
        let narrow = build(24);
        let text = joined(&narrow);
        assert!(text.contains("ctx=42%"), "{text}");
        assert!(!text.contains("localhost:8080"), "{text}");
        assert!(
            narrow
                .iter()
                .map(|span| unicode_width::UnicodeWidthStr::width(span.content.as_ref()))
                .sum::<usize>()
                <= 24,
            "{text}"
        );
    }

    #[test]
    fn push_queued_suffix_appends_only_when_present() {
        let mut spans: Vec<Span<'static>> = Vec::new();
        push_queued_suffix(&mut spans, "");
        assert!(spans.is_empty(), "empty suffix should add no span");

        push_queued_suffix(&mut spans, " · +2 queued");
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content.as_ref(), " · +2 queued");
        assert_eq!(spans[0].style.fg, Some(queued_color()));
    }

    #[test]
    fn display_connection_type_uses_reader_friendly_labels() {
        assert_eq!(display_connection_type("https/sse"), "https");
        assert_eq!(
            display_connection_type("websocket/persistent-fresh"),
            "websocket"
        );
        assert_eq!(
            display_connection_type("websocket/persistent-reuse"),
            "existing websocket"
        );
    }

    #[test]
    fn normalize_status_detail_uses_reader_friendly_labels() {
        assert_eq!(
            normalize_status_detail("fresh websocket").as_deref(),
            Some("opening websocket")
        );
        assert_eq!(
            normalize_status_detail("reusing websocket").as_deref(),
            Some("using existing websocket")
        );
        assert_eq!(
            normalize_status_detail("websocket healthcheck").as_deref(),
            Some("verifying websocket")
        );
        assert_eq!(
            normalize_status_detail("https fallback").as_deref(),
            Some("using https fallback")
        );
    }

    #[test]
    fn collect_transport_context_labels_dedupes_overlapping_transport_text() {
        assert_eq!(
            collect_transport_context_labels(
                normalize_status_detail("reusing websocket"),
                Some(display_connection_type("websocket/persistent-reuse")),
                Some("OpenRouter".to_string())
            ),
            vec![
                "using existing websocket".to_string(),
                "via OpenRouter".to_string()
            ]
        );

        assert_eq!(
            collect_transport_context_labels(
                normalize_status_detail("https fallback"),
                Some(display_connection_type("https/sse")),
                None,
            ),
            vec!["using https fallback".to_string()]
        );
    }

    #[test]
    fn composer_mode_detects_shell_input_before_commands() {
        assert_eq!(
            composer_mode(" ! cargo test ", false),
            ComposerMode::ShellLocal
        );
        assert_eq!(
            composer_mode("! cargo test", true),
            ComposerMode::ShellRemote
        );
        assert_eq!(composer_mode(" /help", false), ComposerMode::SlashCommand);
        assert_eq!(composer_mode("hello", false), ComposerMode::Chat);
    }

    #[test]
    fn shell_mode_hint_reflects_execution_target() {
        // The hint is glyph-prefixed so the mode is recognizable at a glance,
        // and it must name where the command actually runs.
        assert_eq!(
            shell_mode_hint(ComposerMode::ShellLocal),
            Some("  ⚙ shell · Enter runs locally")
        );
        assert_eq!(
            shell_mode_hint(ComposerMode::ShellRemote),
            Some("  ⚙ shell · Enter runs on server")
        );
        assert_eq!(shell_mode_hint(ComposerMode::Chat), None);
    }

    #[test]
    fn empty_composer_placeholder_teaches_shortcuts_and_fits_narrow_widths() {
        let wide = empty_composer_placeholder(80, "> ");
        assert!(wide.contains("/ commands"));
        assert!(wide.contains("F1 help"));

        for width in 0..=80 {
            let placeholder = empty_composer_placeholder(width, "> ");
            assert!(
                unicode_width::UnicodeWidthStr::width(placeholder.as_str()) <= width,
                "placeholder {placeholder:?} exceeds {width} columns"
            );
        }
        assert!(empty_composer_placeholder(0, "> ").is_empty());
    }

    /// While a turn is running the composer must not invite input. It used to
    /// show the same "Ask alphacode anything…" text as the idle state, which
    /// read as "type here" while keystrokes were only being queued.
    #[test]
    fn processing_placeholder_reports_status_instead_of_inviting_input() {
        // `input_prompt` returns the ellipsis glyph while a turn is running.
        let busy = empty_composer_placeholder(80, "\u{2026} ");
        assert!(
            !busy.contains("Ask"),
            "a running turn must not show an input prompt: {busy:?}"
        );
        assert!(busy.contains("Working"), "got {busy:?}");

        for width in 0..=80 {
            let placeholder = empty_composer_placeholder(width, "\u{2026} ");
            assert!(
                unicode_width::UnicodeWidthStr::width(placeholder.as_str()) <= width,
                "placeholder {placeholder:?} exceeds {width} columns"
            );
        }
        assert!(empty_composer_placeholder(0, "\u{2026} ").is_empty());
    }

    /// The shell color must read as a deliberate accent, distinct from the
    /// default composer text. Asserting the semantic property (not one exact
    /// RGB triple) keeps the test meaningful when the palette is retuned.
    #[test]
    fn shell_mode_color_is_distinct() {
        let shell = as_rgb_triple(shell_mode_color());
        let text = as_rgb_triple(rgb(180, 180, 190));
        let distance = (shell.0 as f32 - text.0 as f32).powi(2)
            + (shell.1 as f32 - text.1 as f32).powi(2)
            + (shell.2 as f32 - text.2 as f32).powi(2);
        assert!(
            distance.sqrt() > 40.0,
            "shell color {shell:?} is too close to composer text {text:?}"
        );
        // It is a green: shell mode reads as "terminal", not as an error.
        assert!(
            shell.1 > shell.0 && shell.1 > shell.2,
            "shell color {shell:?} should read green"
        );
    }

    #[test]
    fn normalize_repaint_sensitive_notice_text_drops_warning_variation_selector() {
        assert_eq!(
            normalize_repaint_sensitive_notice_text("⚠️ File activity: read lines 1-9"),
            "⚠ File activity: read lines 1-9"
        );
        assert_eq!(
            normalize_repaint_sensitive_notice_text("all clear"),
            "all clear"
        );
    }

    #[test]
    fn normalize_repaint_sensitive_notice_text_strips_terminal_escapes() {
        // The regression that motivated this: a paste notice used to embed a
        // raw SGR sequence, and ratatui — which has no escape parser — drew the
        // escape itself as text.
        let raw = "\u{1b}[38;2;90;200;130mPasted 12 lines (3.0 KB)\u{1b}[0m";
        let clean = normalize_repaint_sensitive_notice_text(raw);
        assert_eq!(clean, "Pasted 12 lines (3.0 KB)");
        assert!(!clean.contains('\u{1b}'));

        // OSC hyperlinks, DCS, and 8-bit CSI forms all have to go too.
        for raw in [
            "\u{1b}]8;;https://example.com\u{7}link\u{1b}]8;;\u{7}",
            "\u{9b}31mred",
            "\u{1b}Pignored\u{1b}\\done",
        ] {
            let clean = normalize_repaint_sensitive_notice_text(raw);
            assert!(
                !clean.contains('\u{1b}') && !clean.contains('\u{9b}'),
                "escape survived: {raw:?} -> {clean:?}"
            );
        }
    }

    #[test]
    fn normalize_repaint_sensitive_notice_text_folds_newlines() {
        // Notices render on one status row; an embedded newline would clip
        // everything after it with no visual indication that it was there.
        let clean = normalize_repaint_sensitive_notice_text("first\r\nsecond");
        assert_eq!(clean, "first  second");
        assert!(!clean.contains('\n') && !clean.contains('\r'));
    }

    #[test]
    fn normalize_repaint_sensitive_notice_text_preserves_ordinary_text() {
        // Stripping must not eat content the user actually meant to see.
        for text in [
            "Login: waiting...",
            "Copied selection",
            "Fast mode: on",
            "Account: switched to work",
            "model=claude-opus-4-5 (1,204 tokens in / 88 out)",
            "emoji stays: 🚀",
        ] {
            assert_eq!(normalize_repaint_sensitive_notice_text(text), text);
        }
    }
}

/// Build the spans for the notification line. Returns empty vec when there is nothing to show.
/// This is the single source of truth for notification content - both the layout height
/// calculation (via `has_notification`) and the renderer call this.
pub(super) fn build_notification_spans(app: &dyn TuiState) -> Vec<Span<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    let push_sep = |spans: &mut Vec<Span<'static>>| {
        if !spans.is_empty() {
            spans.push(Span::styled(" · ", Style::default().fg(dim_color())));
        }
    };

    if let Some(selection) = app.copy_selection_status() {
        let pane_label = selection.pane.label();
        let label = if selection.has_action {
            if selection.selected_lines > 1 {
                format!(
                    "{} selection · {} chars · {} lines · Enter/Y copy · Esc exit",
                    pane_label, selection.selected_chars, selection.selected_lines
                )
            } else {
                format!(
                    "{} selection · {} chars · Enter/Y copy · Esc exit",
                    pane_label, selection.selected_chars
                )
            }
        } else if selection.dragging {
            format!(
                "{} selection · dragging… · Enter/Y copy · Esc exit",
                pane_label
            )
        } else {
            format!("{} selection · drag to copy", pane_label)
        };
        spans.push(Span::styled(label, Style::default().fg(info_color())));
    }

    if let Some(flicker_notice) = super::recent_flicker_ui_notice() {
        let copy_badge_ui = app.copy_badge_ui();
        let copy_badge_now = std::time::Instant::now();
        let key = super::FLICKER_NOTICE_COPY_KEY;
        let alt_style = if copy_badge_ui.alt_active {
            Style::default().fg(accent_color()).bold()
        } else {
            Style::default().fg(dim_color())
        };
        let shift_style = if copy_badge_ui.shift_active {
            Style::default().fg(accent_color()).bold()
        } else {
            Style::default().fg(dim_color())
        };
        let key_style = if copy_badge_ui.key_is_active(key, copy_badge_now) {
            Style::default().fg(accent_color()).bold()
        } else {
            Style::default().fg(dim_color())
        };

        push_sep(&mut spans);
        spans.push(Span::styled(
            flicker_notice.summary,
            Style::default().fg(warning_color()),
        ));
        push_sep(&mut spans);
        spans.push(Span::styled(
            flicker_notice.hint,
            Style::default().fg(info_color()),
        ));
        spans.push(Span::raw(" "));
        if let Some(success) = copy_badge_ui.feedback_for_key(key, copy_badge_now) {
            let feedback_style = if success {
                Style::default().fg(ai_color()).bold()
            } else {
                Style::default().fg(error_color()).bold()
            };
            let feedback_text = if success {
                "✓ Copied! "
            } else {
                "✗ Copy failed "
            };
            spans.push(Span::styled(feedback_text, feedback_style));
        }
        spans.push(Span::styled(
            super::viewport::copy_badge_alt_badge(),
            alt_style,
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled("[⇧]", shift_style));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("[{}]", key.to_ascii_uppercase()),
            key_style,
        ));
    }

    if let Some(notice) = app.status_notice() {
        push_sep(&mut spans);
        spans.push(Span::styled(
            normalize_repaint_sensitive_notice_text(&notice),
            Style::default().fg(accent_color()),
        ));
    }

    // Learned-keybinding nudge: distinct bright color + bold so the user reads
    // it as "the system noticed I'm not using a shortcut", not a normal status.
    if let Some(hint) = app.learn_hint() {
        push_sep(&mut spans);
        spans.push(Span::styled(
            normalize_repaint_sensitive_notice_text(&hint),
            Style::default().fg(accent_color()).bold(),
        ));
    }

    // Inline hotkey feedback: what a rarely-used chord just did, or the nearest
    // binding for an unbound chord. Cool cyan so it reads as informational.
    if let Some(feedback) = app.hotkey_feedback() {
        push_sep(&mut spans);
        spans.push(Span::styled(
            normalize_repaint_sensitive_notice_text(&feedback),
            Style::default().fg(info_color()),
        ));
    }

    if !app.is_processing() {
        let info = app.info_widget_data();
        if let Some(schedule_notice) =
            crate::alphacode_tui::tui::scheduled_notification_text(info.ambient_info.as_ref())
        {
            push_sep(&mut spans);
            spans.push(Span::styled(
                schedule_notice,
                Style::default().fg(info_color()),
            ));
        }

        if let Some(cache_info) = app.cache_ttl_status() {
            if cache_info.is_cold {
                let tokens_str = cache_info
                    .cached_tokens
                    .map(|t| {
                        if t >= 1_000_000 {
                            format!(" ({:.1}M tok)", t as f64 / 1_000_000.0)
                        } else if t >= 1_000 {
                            format!(" ({}K tok)", t / 1000)
                        } else {
                            format!(" ({} tok)", t)
                        }
                    })
                    .unwrap_or_default();
                push_sep(&mut spans);
                spans.push(Span::styled(
                    format!("🧊 cache cold{}", tokens_str),
                    Style::default().fg(info_color()),
                ));
                // Small gray "how long ago it went cold" hint, e.g. `1h 1m`.
                spans.push(Span::styled(
                    format!(
                        " {}",
                        crate::alphacode_tui::tui::format_compact_age(cache_info.cold_for_secs)
                    ),
                    Style::default().fg(dim_color()),
                ));
            } else if cache_info.expiring_soon() {
                let tokens_str = cache_info
                    .cached_tokens
                    .map(|t| {
                        if t >= 1_000 {
                            format!(" {}K", t / 1000)
                        } else {
                            format!(" {}", t)
                        }
                    })
                    .unwrap_or_default();
                // Above a minute, a raw seconds count is hard to read at a
                // glance; show minutes granularity instead.
                let remaining = cache_info.remaining_secs;
                let time_str = if remaining > 60 {
                    format!("{}m", remaining.div_ceil(60))
                } else {
                    format!("{}s", remaining)
                };
                push_sep(&mut spans);
                spans.push(Span::styled(
                    format!("⏳ cache {}{}", time_str, tokens_str),
                    Style::default().fg(warning_color()),
                ));
            }
        }
    }

    if app.has_stashed_input() {
        push_sep(&mut spans);
        spans.push(Span::styled(
            "📋 stash",
            Style::default().fg(warning_color()),
        ));
    }

    spans
}

pub(super) fn draw_notification(frame: &mut Frame, app: &dyn TuiState, area: Rect) {
    let spans = build_notification_spans(app);
    if spans.is_empty() {
        return;
    }
    let line = truncate_line_with_ellipsis(Line::from(spans), area.width as usize);
    let aligned_line = if app.centered_mode() {
        line.alignment(Alignment::Center)
    } else {
        line
    };
    frame.render_widget(Paragraph::new(aligned_line), area);
}

/// Draw the elastic overscroll status line, revealed below the input when the
/// user scrolls past the bottom of the transcript. Shows model, provider,
/// access method, reasoning level, and context usage percentage, with a live
/// `(overscroll x.x)` countdown pinned to the right so users can see the line
/// is temporary and rebounds away on its own.
pub(super) fn draw_overscroll_status(frame: &mut Frame, app: &dyn TuiState, area: Rect) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let data = app.info_widget_data();

    let sep = || Span::styled(" · ", Style::default().fg(dim_color()));

    // The countdown is the priority affordance: it explains the line exists and
    // is going away. Build it first so it always gets space on the right edge.
    let countdown: Option<Span> = app.chat_overscroll_remaining().map(|secs| {
        Span::styled(
            format!("(overscroll {:.1})", secs.max(0.0)),
            Style::default().fg(muted_text_color()).italic(),
        )
    });

    let mut spans: Vec<Span> = Vec::new();

    // Model
    let model = data
        .model
        .clone()
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| app.provider_model());
    if !model.is_empty() && !overscroll_is_placeholder(&model) {
        spans.push(Span::styled(
            session_facts::pretty_model(&model),
            Style::default().fg(user_color()).bold(),
        ));
        // Reasoning level shown inline next to the model, e.g. " high".
        if let Some(effort) = data
            .reasoning_effort
            .as_deref()
            .and_then(overscroll_short_reasoning)
        {
            spans.push(Span::styled(
                format!(" {}", effort),
                Style::default().fg(muted_text_color()),
            ));
        }
    }

    // Provider
    let provider = data
        .provider_name
        .clone()
        .filter(|p| !p.is_empty())
        .unwrap_or_else(|| app.provider_name());
    if !provider.is_empty() && !overscroll_is_runtime_placeholder(&provider) {
        if !spans.is_empty() {
            spans.push(sep());
        }
        spans.push(Span::styled(
            overscroll_provider_display(&provider),
            Style::default().fg(info_color()),
        ));
    }

    // Access method (auth)
    if let Some((label, color)) = overscroll_auth_label(data.auth_method) {
        if !spans.is_empty() {
            spans.push(sep());
        }
        spans.push(Span::styled(label.to_string(), Style::default().fg(color)));
    }

    // Context usage as a rounded bar
    if let Some((used, limit)) = overscroll_context_usage(&data) {
        if !spans.is_empty() {
            spans.push(sep());
        }
        spans.push(Span::styled(
            format!(
                "{}/{} ",
                overscroll_format_tokens(used),
                overscroll_format_tokens(limit)
            ),
            Style::default().fg(muted_text_color()),
        ));
        spans.extend(overscroll_context_bar(used, limit, 10));
    }

    // Working directory last, shown as a home-relative path, with the git
    // branch alongside when available.
    if let Some(dir) = app.working_dir().and_then(|d| overscroll_dir_label(&d)) {
        if !spans.is_empty() {
            spans.push(sep());
        }
        spans.push(Span::styled(" ", Style::default().fg(info_color())));
        spans.push(Span::styled(dir, Style::default().fg(muted_text_color())));
        if let Some(branch) = overscroll_git_branch(&data) {
            spans.push(Span::styled(
                format!("  {branch}"),
                Style::default().fg(success_color()),
            ));
        }
    }

    let total_width = area.width as usize;

    // No countdown active: just render the info line (centered or not) as before.
    let Some(countdown) = countdown else {
        if spans.is_empty() {
            return;
        }
        let line = Line::from(overscroll_truncate_spans(spans, total_width));
        let aligned_line = if app.centered_mode() {
            line.alignment(Alignment::Center)
        } else {
            line
        };
        frame.render_widget(Paragraph::new(aligned_line), area);
        return;
    };

    let countdown_width = unicode_width::UnicodeWidthStr::width(countdown.content.as_ref());

    // Tight width: if there is not even room for the countdown plus a single
    // space of breathing room, drop the info entirely and just show the
    // countdown (truncated as a last resort). The affordance survives.
    if total_width <= countdown_width + 1 {
        let countdown_line = Line::from(overscroll_truncate_spans(vec![countdown], total_width))
            .alignment(Alignment::Right);
        frame.render_widget(Paragraph::new(countdown_line), area);
        return;
    }

    // Reserve the countdown on the right; the info line gets the rest and is
    // truncated to fit so the two never collide.
    let gap = 1u16;
    let right_w = countdown_width as u16;
    let left_w = area.width.saturating_sub(right_w);
    let left_area = Rect {
        width: left_w.saturating_sub(gap),
        ..area
    };
    let right_area = Rect {
        x: area.x + left_w,
        width: right_w,
        ..area
    };

    if !spans.is_empty() {
        let avail = left_area.width as usize;
        let info_line = Line::from(overscroll_truncate_spans(spans, avail));
        let info_line = if app.centered_mode() {
            info_line.alignment(Alignment::Center)
        } else {
            info_line
        };
        frame.render_widget(Paragraph::new(info_line), left_area);
    }

    let countdown_line = Line::from(vec![countdown]).alignment(Alignment::Right);
    frame.render_widget(Paragraph::new(countdown_line), right_area);
}

/// Truncate a list of spans to at most `max_width` display columns, appending a
/// single-cell ellipsis when content is dropped. Preserves per-span styling.
fn overscroll_truncate_spans(spans: Vec<Span<'static>>, max_width: usize) -> Vec<Span<'static>> {
    use unicode_width::UnicodeWidthStr;
    if max_width == 0 {
        return Vec::new();
    }
    let total: usize = spans.iter().map(|s| s.content.width()).sum();
    if total <= max_width {
        return spans;
    }
    // Leave room for a trailing ellipsis.
    let budget = max_width.saturating_sub(1);
    let mut out: Vec<Span<'static>> = Vec::new();
    let mut used = 0usize;
    for span in spans {
        let w = span.content.width();
        if used + w <= budget {
            used += w;
            out.push(span);
            continue;
        }
        // Partial: take as many chars as fit within the remaining budget.
        let remaining = budget - used;
        if remaining > 0 {
            let mut taken = String::new();
            let mut tw = 0usize;
            for ch in span.content.chars() {
                let cw = UnicodeWidthStr::width(ch.to_string().as_str());
                if tw + cw > remaining {
                    break;
                }
                tw += cw;
                taken.push(ch);
            }
            if !taken.is_empty() {
                out.push(Span::styled(taken, span.style));
            }
        }
        break;
    }
    out.push(Span::styled("…", Style::default().fg(dim_color())));
    out
}

/// Format a working dir path home-relative (~/foo/bar), keeping the last 2 segments.
/// Compact git branch label for the status line and fact stack. Truncated so
/// long branch names cannot crowd out the other facts.
fn overscroll_git_branch(
    data: &crate::alphacode_tui::tui::info_widget::InfoWidgetData,
) -> Option<String> {
    let branch = data.git_info.as_ref()?.branch.trim();
    if branch.is_empty() {
        return None;
    }
    let mut label: String = branch.chars().take(24).collect();
    if branch.chars().count() > 24 {
        label.push('…');
    }
    Some(label)
}

fn overscroll_dir_label(path: &str) -> Option<String> {
    session_facts::dir_label_short(path)
}

/// Placeholder header strings used during remote startup; not real model names.
fn overscroll_is_placeholder(model: &str) -> bool {
    let m = model.trim().to_ascii_lowercase();
    m.starts_with("connecting")
        || m.starts_with("loading")
        || m == "connected"
        || m.contains("connecting to server")
}

/// Inert runtime provider labels (remote/replay/test-harness) shown before the
/// real provider name is known; not real provider names.
fn overscroll_is_runtime_placeholder(provider: &str) -> bool {
    matches!(
        provider.trim().to_ascii_lowercase().as_str(),
        "remote" | "replay" | "test-harness"
    )
}

fn overscroll_provider_display(provider: &str) -> String {
    match provider.to_ascii_lowercase().as_str() {
        // Keep provider labels credential-neutral: the adjacent auth chip
        // (`overscroll_auth_label`) reports OAuth vs API key from the canonical
        // credential resolution. Baking a credential into the provider name
        // used to produce contradictions like "Claude OAuth · API key" when
        // the Anthropic route was pinned to the API key.
        "claude" => "Claude".to_string(),
        "anthropic" => "Anthropic".to_string(),
        "openai" => "OpenAI".to_string(),
        "openrouter" => "OpenRouter".to_string(),
        "opencode" => "OpenCode".to_string(),
        "gemini" => "Gemini".to_string(),
        "copilot" => "GitHub Copilot".to_string(),
        "cursor" => "Cursor".to_string(),
        "bedrock" => "AWS Bedrock".to_string(),
        "antigravity" => "Antigravity".to_string(),
        _ => provider.to_string(),
    }
}

fn overscroll_auth_label(
    method: crate::alphacode_tui::tui::info_widget::AuthMethod,
) -> Option<(&'static str, Color)> {
    use crate::alphacode_tui::tui::info_widget::AuthMethod;
    match method {
        AuthMethod::Unknown => None,
        AuthMethod::ApiKey | AuthMethod::AnthropicApiKey | AuthMethod::OpenAIApiKey => {
            Some(("API key", muted_text_color()))
        }
        AuthMethod::OpenRouterApiKey | AuthMethod::OpenCodeApiKey => {
            Some(("API key", info_color()))
        }
        // All OAuth flows share one accent: the provider name right next to the
        // chip already carries the brand identity, so per-provider hues here
        // just read as noise against the rest of the chrome.
        AuthMethod::AnthropicOAuth
        | AuthMethod::OpenAIOAuth
        | AuthMethod::CopilotOAuth
        | AuthMethod::GeminiOAuth => Some(("OAuth", accent_color())),
    }
}

fn overscroll_short_reasoning(effort: &str) -> Option<&str> {
    let effort = effort.trim();
    if effort.is_empty() {
        return None;
    }
    Some(match effort {
        "max" => "max",
        "xhigh" => "xhigh",
        "high" => "high",
        "medium" => "medium",
        "low" => "low",
        "none" => "none",
        other => other,
    })
}

fn overscroll_context_usage(
    data: &crate::alphacode_tui::tui::info_widget::InfoWidgetData,
) -> Option<(usize, usize)> {
    let used_tokens = if let Some(observed) = data.observed_context_tokens {
        observed as usize
    } else {
        let info = data.context_info.as_ref()?;
        if info.total_chars == 0 {
            return None;
        }
        info.estimated_tokens()
    };
    let limit = data
        .context_limit
        .unwrap_or(crate::provider::DEFAULT_CONTEXT_LIMIT)
        .max(1);
    Some((used_tokens, limit))
}

/// Format a token count compactly: 1234 -> "1.2k", 200000 -> "200k", 1_500_000 -> "1.5M".
fn overscroll_format_tokens(tokens: usize) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 10_000 {
        format!("{}k", tokens / 1000)
    } else if tokens >= 1_000 {
        format!("{:.1}k", tokens as f64 / 1000.0)
    } else {
        tokens.to_string()
    }
}

/// Render a compact rounded progress bar (◖████░░◗) plus a percentage label.
fn overscroll_context_bar(used: usize, limit: usize, cells: usize) -> Vec<Span<'static>> {
    let limit = limit.max(1);
    let ratio = (used as f64 / limit as f64).clamp(0.0, 1.0);
    let pct = (ratio * 100.0).round() as u16;
    let filled = (ratio * cells as f64).round() as usize;
    let filled = filled.min(cells);

    // Thresholds match the info widget usage bar: the meter reddens as the
    // remaining context shrinks, and the track sits on the shared border role.
    let left_pct = 100u16.saturating_sub(pct);
    let fill_color = if left_pct <= 20 {
        error_color()
    } else if left_pct <= 50 {
        warning_color()
    } else {
        success_color()
    };
    let track_color = border_color();

    let mut spans = Vec::with_capacity(cells + 2);
    // Slim segmented pill (▰ filled / ▱ empty) reads thinner than full blocks.
    spans.push(Span::styled(
        "▰".repeat(filled),
        Style::default().fg(fill_color),
    ));
    spans.push(Span::styled(
        "▱".repeat(cells.saturating_sub(filled)),
        Style::default().fg(track_color),
    ));
    spans.push(Span::styled(
        format!(" {}%", pct),
        Style::default().fg(fill_color).bold(),
    ));
    spans
}

const RIGHT_FACT_CONTEXT_CELLS: usize = 6;
const RIGHT_FACT_GAP: u16 = 2;
const RIGHT_FACT_PAD: u16 = 1;

/// Columns the session-fact stack may occupy on the rows it can dock on,
/// including its left gutter and right pad. Chrome decoration that paints
/// across such a row (the transcript-gap gradient rule) must stop short of
/// this band: the stack only docks on rows whose suffix cells are genuinely
/// blank, so a full-width rule makes the row impassable and the stack either
/// settles higher in the transcript or hides entirely.
pub(super) fn right_fact_reserved_width(app: &dyn TuiState) -> u16 {
    if app.chat_overscroll_active() {
        // The legacy overscroll row owns the facts while it is revealed; the
        // stack stands down and needs no band.
        return 0;
    }
    right_fact_lines(app)
        .iter()
        .map(|line| {
            line.width
                .saturating_add(RIGHT_FACT_GAP)
                .saturating_add(RIGHT_FACT_PAD)
        })
        .max()
        .unwrap_or(0)
}
const RIGHT_FACT_TRANSCRIPT_ROWS: u16 = 4;

/// Maximum number of rows the compact session-fact stack may occupy, and the
/// budget [`right_fact_lines`] sizes itself against.
///
/// The stack ships up to five facts, but a three-row scan block is the most the
/// transcript tail can spare, so exactly [`RIGHT_FACT_ROWS`] lines are
/// rendered. One row is reserved for the session token/cost fact (see
/// `right_fact_lines`); the remaining three are available to the descriptive
/// facts and are claimed in priority order.
const RIGHT_FACT_ROWS: usize = 4;

fn right_fact_neutral_style() -> Style {
    Style::default().fg(muted_text_color())
}

#[derive(Clone)]
struct RightFactLine {
    spans: Vec<Span<'static>>,
    width: u16,
}

impl RightFactLine {
    fn new(spans: Vec<Span<'static>>) -> Option<Self> {
        use unicode_width::UnicodeWidthStr;
        let width: usize = spans.iter().map(|span| span.content.width()).sum();
        let width = u16::try_from(width).ok()?;
        (width > 0).then_some(Self { spans, width })
    }
}

#[derive(Clone)]
struct RightFactPlacement {
    line: RightFactLine,
    area: Rect,
}

/// Draw session facts as a bottom-anchored stack in otherwise unused cells on
/// the right side of the composer chrome. When chrome rows are occupied, the
/// stack may climb into at most the last few transcript rows, but only where
/// the final rendered buffer has a genuinely blank suffix. It never reflows or
/// overwrites transcript, status, notification, inline UI, or input content.
pub(super) fn draw_right_fact_stack(
    frame: &mut Frame,
    app: &dyn TuiState,
    messages_area: Rect,
    input_area: Rect,
    transcript_scrollbar_visible: bool,
    input_cursor: Option<Position>,
) {
    // The legacy overscroll row owns these same facts while it is visible.
    // Standing down here avoids duplicates and keeps its elastic reveal from
    // changing transcript-tail overlays mid-gesture. Users with overscroll off
    // get the compact stack continuously.
    if app.chat_overscroll_active() || input_area.width == 0 || input_area.height == 0 {
        return;
    }

    let lines = right_fact_lines(app);
    if lines.is_empty() {
        return;
    }

    // Never composite into a live transcript while a turn is processing. Its
    // tail changes every frame, so even collision-safe facts could appear to
    // jump as streaming rows arrive. Chrome rows remain available.
    let transcript_rows = if app.auto_scroll_paused() || app.is_processing() {
        0
    } else {
        RIGHT_FACT_TRANSCRIPT_ROWS.min(messages_area.height)
    };
    let top = messages_area.bottom().saturating_sub(transcript_rows);
    let bottom = input_area.bottom();
    if top >= bottom {
        return;
    }

    let placements = {
        let buffer = frame.buffer_mut();
        right_fact_placements(
            buffer,
            lines,
            top,
            bottom,
            input_area.x,
            input_area.right(),
            messages_area,
            transcript_scrollbar_visible,
            input_cursor,
        )
    };

    for placement in placements {
        frame.render_widget(
            Paragraph::new(Line::from(placement.line.spans)),
            placement.area,
        );
    }
}

/// Build fact rows in their visual top-to-bottom order: provider/auth, model,
/// directory, then context usage. Placement walks this list in reverse so the
/// context row is anchored nearest the input whenever space permits.
fn right_fact_lines(app: &dyn TuiState) -> Vec<RightFactLine> {
    let data = app.info_widget_data();
    let sep = || Span::styled(" · ", right_fact_neutral_style());
    let mut lines = Vec::with_capacity(4);

    let mut access = Vec::new();
    let provider = data
        .provider_name
        .clone()
        .filter(|provider| !provider.trim().is_empty())
        .unwrap_or_else(|| app.provider_name());
    if !provider.is_empty() && !overscroll_is_runtime_placeholder(&provider) {
        access.push(Span::styled(
            overscroll_provider_display(&provider),
            right_fact_neutral_style(),
        ));
    }
    if let Some((label, _)) = overscroll_auth_label(data.auth_method) {
        if !access.is_empty() {
            access.push(sep());
        }
        access.push(Span::styled(label.to_string(), right_fact_neutral_style()));
    }
    if let Some(line) = RightFactLine::new(access) {
        lines.push(line);
    }

    let model = data
        .model
        .clone()
        .filter(|model| !model.trim().is_empty())
        .unwrap_or_else(|| app.provider_model());
    if !model.is_empty() && !overscroll_is_placeholder(&model) {
        let mut spans = vec![Span::styled(
            session_facts::pretty_model(&model),
            right_fact_neutral_style(),
        )];
        if let Some(effort) = data
            .reasoning_effort
            .as_deref()
            .and_then(overscroll_short_reasoning)
        {
            spans.push(Span::styled(
                format!(" {effort}"),
                right_fact_neutral_style(),
            ));
        }
        if let Some(line) = RightFactLine::new(spans) {
            lines.push(line);
        }
    }

    if let Some(dir) = app
        .working_dir()
        .and_then(|path| overscroll_dir_label(&path))
    {
        let mut spans = vec![Span::styled(dir, right_fact_neutral_style())];
        if let Some(branch) = overscroll_git_branch(&data) {
            spans.push(Span::styled(
                format!("  {branch}"),
                right_fact_neutral_style(),
            ));
        }
        if let Some(line) = RightFactLine::new(spans) {
            lines.push(line);
        }
    }

    // Context usage is the lowest-priority descriptive fact, and one row of the
    // budget is reserved for the session token/cost fact below. When the higher
    // priority facts already claim every descriptive row (provider+auth, model
    // and directory all present), context steps aside rather than crowding cost
    // out of the stack. It remains visible on the context-usage widget and in
    // the header, so nothing is lost.
    let context_has_budget =
        lines.len() + 1 < RIGHT_FACT_ROWS || right_fact_session_stats(app, &data).is_none();
    if context_has_budget && let Some((used, limit)) = overscroll_context_usage(&data) {
        let mut spans = vec![Span::styled(
            format!(
                "{}/{} ",
                overscroll_format_tokens(used),
                overscroll_format_tokens(limit)
            ),
            right_fact_neutral_style(),
        )];
        spans.extend(overscroll_context_bar(
            used,
            limit,
            RIGHT_FACT_CONTEXT_CELLS,
        ));
        if let Some(line) = RightFactLine::new(spans) {
            lines.push(line);
        }
    }

    // Session token/cost fact. This row is *reserved*: it is the only surface
    // that reports session cost and token totals once a transcript exists, because
    // the idle status line below the composer is only drawn while the session is
    // idle with no conversation. Gating it on a spare 5th slot (`lines.len() < 4`)
    // made it dead whenever provider+auth occupied a row, which is the common
    // case — the stack filled with provider · model · directory · context and the
    // cost silently disappeared for the rest of the session. The stack therefore
    // budgets [`RIGHT_FACT_ROWS`] rows: at most three descriptive facts plus this
    // one. Context usage is the fact dropped when all four claimants are present.
    if lines.len() < RIGHT_FACT_ROWS
        && let Some(line) = right_fact_session_stats(app, &data)
    {
        lines.push(line);
    }

    lines
}

/// Session token + estimated cost fact for the compact stack. Like the other
/// facts it stays neutral gray (only the context meter is colored).
fn right_fact_session_stats(
    app: &dyn TuiState,
    data: &super::info_widget::InfoWidgetData,
) -> Option<RightFactLine> {
    let (total_in, total_out) = app.total_session_tokens()?;
    if total_in == 0 && total_out == 0 {
        return None;
    }
    let mut spans = vec![Span::styled(
        format!(
            "⌁ ↑{} ↓{}",
            overscroll_format_tokens(usize::try_from(total_in).unwrap_or(0)),
            overscroll_format_tokens(usize::try_from(total_out).unwrap_or(0)),
        ),
        right_fact_neutral_style(),
    )];
    // Dollar figure. A metered transport reports what it actually billed; a
    // free-tier or subscription transport has no per-token price, so it reports
    // the notional value of its tokens at the UI reference rate rather than a dead
    // `$0.00` or nothing at all. This is the same convention the idle status line
    // and the usage widget already use (`info_widget::format_token_value_usd`), so
    // the three surfaces cannot disagree.
    let dollars = data
        .usage_info
        .as_ref()
        .filter(|info| info.available)
        .map(|info| {
            if matches!(info.provider, super::info_widget::UsageProvider::CostBased)
                && info.total_cost > 0.0
            {
                format!("${:.2}", info.total_cost)
            } else {
                format!(
                    "est. {}",
                    super::info_widget::format_token_value_usd(total_in.saturating_add(total_out))
                )
            }
        });
    if let Some(dollars) = dollars {
        spans.push(Span::styled(
            format!(" · {dollars}"),
            right_fact_neutral_style(),
        ));
    }
    RightFactLine::new(spans)
}

#[allow(clippy::too_many_arguments)]
fn right_fact_placements(
    buffer: &ratatui::buffer::Buffer,
    lines: Vec<RightFactLine>,
    top: u16,
    bottom: u16,
    left: u16,
    right: u16,
    messages_area: Rect,
    transcript_scrollbar_visible: bool,
    protected_position: Option<Position>,
) -> Vec<RightFactPlacement> {
    if top >= bottom || left >= right || lines.is_empty() {
        return Vec::new();
    }

    let Ok(block_height) = u16::try_from(lines.len()) else {
        return Vec::new();
    };
    if bottom.saturating_sub(top) < block_height {
        return Vec::new();
    }

    // Facts are one visual object. Probe complete consecutive blocks from the
    // bottom upward; if any row collides, move the entire stack rather than
    // skipping that row and letting unrelated content split the facts apart.
    let mut block_bottom = bottom;
    loop {
        let Some(block_top) = block_bottom.checked_sub(block_height) else {
            return Vec::new();
        };
        if block_top < top {
            return Vec::new();
        }

        let areas = lines
            .iter()
            .enumerate()
            .map(|(index, line)| {
                let row = block_top + index as u16;
                right_fact_area_on_row(
                    buffer,
                    line,
                    row,
                    left,
                    right,
                    messages_area,
                    transcript_scrollbar_visible,
                    protected_position,
                )
            })
            .collect::<Option<Vec<_>>>();
        if let Some(areas) = areas {
            return lines
                .into_iter()
                .zip(areas)
                .map(|(line, area)| RightFactPlacement { line, area })
                .collect();
        }

        if block_top == top {
            return Vec::new();
        }
        block_bottom = block_bottom.saturating_sub(1);
    }
}

#[allow(clippy::too_many_arguments)]
fn right_fact_area_on_row(
    buffer: &ratatui::buffer::Buffer,
    line: &RightFactLine,
    row: u16,
    left: u16,
    right: u16,
    messages_area: Rect,
    transcript_scrollbar_visible: bool,
    protected_position: Option<Position>,
) -> Option<Rect> {
    let row_right =
        if transcript_scrollbar_visible && row >= messages_area.y && row < messages_area.bottom() {
            right.saturating_sub(1)
        } else {
            right
        };

    let required = line
        .width
        .saturating_add(RIGHT_FACT_GAP)
        .saturating_add(RIGHT_FACT_PAD);
    if row_right.saturating_sub(left) < required {
        return None;
    }

    let fact_right = row_right.saturating_sub(RIGHT_FACT_PAD);
    let fact_left = fact_right.saturating_sub(line.width);
    let probe_left = fact_left.saturating_sub(RIGHT_FACT_GAP);
    if probe_left < left
        || !(probe_left..row_right).all(|x| {
            protected_position != Some(Position::new(x, row))
                && right_fact_cell_is_blank(&buffer[(x, row)])
        })
    {
        return None;
    }

    Some(Rect::new(fact_left, row, line.width, 1))
}

// Background `draw_status` paints across its whole row as a visual separator
/// between the status line and the transcript above. It is decoration this app
/// owns, not content, so the session-fact stack may composite over it - hence the
/// single named exemption instead of relaxing the "styled blank is occupied"
/// rule generally. Hoisted here so the fill and the exemption cannot drift
/// apart.
const STATUS_ROW_BG: Color = Color::Rgb(18, 20, 28);

fn right_fact_cell_is_blank(cell: &ratatui::buffer::Cell) -> bool {
    cell.symbol().trim().is_empty()
        && (cell.bg == Color::Reset || cell.bg == STATUS_ROW_BG)
        && cell.modifier.is_empty()
        && cell.diff_option != ratatui::buffer::CellDiffOption::Skip
}

pub(super) fn draw_input(
    frame: &mut Frame,
    app: &dyn TuiState,
    area: Rect,
    next_prompt: usize,
    debug_capture: &mut Option<FrameCaptureBuilder>,
) -> Option<Position> {
    let input_text = app.input();
    let cursor_pos = app.cursor_pos();

    let mode = composer_mode(input_text, app.is_remote_mode());
    // Command suggestions render later as an overlay pass
    // (`draw_command_suggestions_overlay`), never inline: they must not shift
    // the input rows or anything below them.
    let has_suggestions = command_suggestions_active(app, &app.command_suggestions());

    let (prompt_char, caret_color) = input_prompt(app);
    // Match `wrapped_input_line_count` and mouse hit-testing: all three use the
    // same display-column width for the mode prefix, with no prompt counter in
    // the edit line.
    let num_str = String::new();
    let prompt_len = input_prompt_len(app, next_prompt);
    let reserved_width = send_mode_reserved_width(app);
    let line_width = (area.width as usize).saturating_sub(prompt_len + reserved_width);

    if line_width == 0 {
        return None;
    }

    let (all_lines, cursor_line, cursor_col) = wrap_input_text(
        input_text,
        cursor_pos,
        line_width,
        &num_str,
        &prompt_char,
        caret_color,
        prompt_len,
    );

    let mut lines: Vec<Line> = Vec::new();
    let mut hint_shown = false;
    let mut hint_line: Option<String> = None;
    if has_suggestions {
        // Overlay pass owns the palette rows; no inline hint line competes
        // with it.
    } else if let Some(shell_hint) = shell_mode_hint(mode) {
        hint_shown = true;
        hint_line = Some(shell_hint.trim().to_string());
        lines.push(Line::from(Span::styled(
            shell_hint,
            Style::default().fg(BrandTheme::success()),
        )));
    } else if app.next_prompt_new_session_armed() {
        hint_shown = true;
        let hint = "  \u{2197} Next prompt opens a new session";
        hint_line = Some(hint.trim().to_string());
        lines.push(Line::from(Span::styled(
            hint,
            Style::default()
                .fg(BrandTheme::info())
                .add_modifier(Modifier::ITALIC),
        )));
    } else if app.is_processing() && !input_text.is_empty() {
        hint_shown = true;
        let hint = if app.queue_mode() {
            "  Ctrl/Cmd+Enter to send now  |  Enter to queue"
        } else {
            "  Ctrl/Cmd+Enter to queue  |  Enter to interleave"
        };
        hint_line = Some(hint.trim().to_string());
        lines.push(Line::from(Span::styled(
            hint,
            Style::default()
                .fg(BrandTheme::dim_bright())
                .add_modifier(Modifier::ITALIC),
        )));
    }

    if let Some(capture) = debug_capture {
        capture.rendered_text.input_area = input_text.to_string();
        if let Some(hint) = &hint_line {
            capture.rendered_text.input_hint = Some(hint.clone());
        }
        visual_debug::check_shift_enter_anomaly(
            capture,
            app.is_processing(),
            input_text,
            hint_shown,
        );
    }

    let suggestions_offset = lines.len();
    let total_input_lines = all_lines.len();
    let visible_height = area.height as usize;

    // Scroll offset for the input. We do NOT render an internal
    // scrollbar here: the chat viewport's main scrollbar (rendered
    // by `ui_viewport::render_messages_with_scrollbar`) is the ONE
    // scroll control the user sees. The input's only job is to keep
    // the cursor on screen.
    //
    // If everything fits, no scroll. If content overflows, pin the
    // bottom: the last `available_for_input` lines (those ending
    // with the cursor) are shown. This matches every modern TUI
    // editor (vim, emacs, micro, helix) and is what the user
    // expects when they paste 30 lines and then press Enter.
    let scroll_offset = if total_input_lines + suggestions_offset <= visible_height {
        0
    } else {
        let available_for_input = visible_height.saturating_sub(suggestions_offset).max(1);
        if total_input_lines <= available_for_input {
            0
        } else {
            // Pin to bottom: show lines [total - available, total).
            total_input_lines.saturating_sub(available_for_input)
        }
    };

    for line in all_lines.into_iter().skip(scroll_offset) {
        lines.push(line);
        if lines.len() >= visible_height {
            break;
        }
    }
    let visible_input_rows = lines.len().saturating_sub(suggestions_offset);

    let centered = app.centered_mode();

    // Register the composer's visible rows with the shared copy-selection
    // machinery so mouse drags can select and copy the text being typed
    // (issue #430). The prompt prefix / continuation indent is excluded via
    // per-row left margins, so hit-testing and the copied text only ever
    // cover the typed text, never the prompt decoration.
    if visible_input_rows > 0 {
        let (wrapped_plain, raw_plain, line_map) =
            input_copy_snapshot_parts(input_text, line_width);
        let left_margins: Vec<u16> = (0..visible_input_rows)
            .map(|rel| {
                let abs = scroll_offset + rel;
                let text_width = wrapped_plain
                    .get(abs)
                    .map(|text| unicode_width::UnicodeWidthStr::width(text.as_str()))
                    .unwrap_or(0);
                let mut margin = prompt_len;
                if centered {
                    margin += (area.width as usize).saturating_sub(prompt_len + text_width) / 2;
                }
                margin.min(area.width as usize) as u16
            })
            .collect();
        let input_rows_area = Rect::new(
            area.x,
            area.y.saturating_add(suggestions_offset as u16),
            area.width,
            visible_input_rows as u16,
        );
        super::record_input_copy_snapshot(
            wrapped_plain,
            raw_plain,
            line_map,
            scroll_offset,
            scroll_offset + visible_input_rows,
            input_rows_area,
            &left_margins,
        );
    }

    // Highlight an active copy selection over the composer text, mirroring the
    // chat/side-pane selection rendering. Columns are selection-space (typed
    // text only), so shift them right by the prompt width for display.
    if let Some(range) = app.copy_selection_range().filter(|range| {
        range.start.pane == crate::alphacode_tui::tui::CopySelectionPane::Input
            && range.end.pane == crate::alphacode_tui::tui::CopySelectionPane::Input
    }) {
        let (start, end) = if (range.start.abs_line, range.start.column)
            <= (range.end.abs_line, range.end.column)
        {
            (range.start, range.end)
        } else {
            (range.end, range.start)
        };
        for rel in 0..visible_input_rows {
            let abs = scroll_offset + rel;
            if abs < start.abs_line || abs > end.abs_line {
                continue;
            }
            if let Some(line) = lines.get_mut(suggestions_offset + rel) {
                let start_col = if abs == start.abs_line {
                    prompt_len + start.column
                } else {
                    prompt_len
                };
                let end_col = if abs == end.abs_line {
                    prompt_len + end.column
                } else {
                    line.width()
                };
                *line = highlight_line_selection(line, start_col, end_col);
            }
        }
    }

    // Show a dim inline preview for a unique recent-prompt match. It is added
    // after selection painting so preview text is never copied or highlighted
    // as if it were part of the user's actual draft.
    if !has_suggestions
        && !mode.is_shell()
        && !app.is_processing()
        && cursor_pos == input_text.len()
        && app.copy_selection_range().is_none()
        && let Some(last_line) =
            lines.get_mut(suggestions_offset + cursor_line.saturating_sub(scroll_offset))
        && let Some(suffix) =
            app.input_history_completion_suffix(line_width.saturating_sub(cursor_col))
    {
        last_line.spans.push(Span::styled(
            suffix,
            Style::default()
                .fg(dim_color())
                .add_modifier(Modifier::ITALIC),
        ));
    }

    // Subtle top border on the input area for visual separation from the
    // transcript above. Uses a dim gradient that matches the brand theme.
    // A one-row composer has no interior to separate: its only row *is* the text
    // row, so a full-width rule paints over the prompt itself and permanently
    // denies that row to the session-fact stack (which is specified to
    // composite its context meter onto `input_area.bottom() - 1`).
    if area.height > 1 && area.width > 2 {
        // NOTE: static phase (0.0) rather than `animation_elapsed()` — a
        // breathing border would be frozen mid-breath between full frames by
        // the idle-animation partial repaint, which only patches the animated
        // rows and copies everything else from the previous frame.
        let border_line = crate::alphacode_tui::tui::brand_ux::BrandTheme::breathing_separator(
            area.width as usize,
            0.0,
        );
        let border_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1,
        };
        frame.render_widget(Paragraph::new(border_line), border_area);
    }

    let paragraph = if centered {
        Paragraph::new(
            lines
                .iter()
                .map(|l| l.clone().alignment(Alignment::Center))
                .collect::<Vec<_>>(),
        )
    } else {
        Paragraph::new(lines.clone())
    };
    frame.render_widget(paragraph, area);

    let cursor_screen_line = cursor_line.saturating_sub(scroll_offset) + suggestions_offset;
    let cursor_y = area.y + (cursor_screen_line as u16).min(area.height.saturating_sub(1));

    let cursor_x = if centered {
        let actual_line_width = lines
            .get(cursor_screen_line)
            .map(|l| l.width())
            .unwrap_or(prompt_len);
        let center_offset = (area.width as usize).saturating_sub(actual_line_width) / 2;
        let cursor_offset = prompt_len + cursor_col;
        area.x + center_offset as u16 + cursor_offset as u16
    } else {
        area.x + prompt_len as u16 + cursor_col as u16
    };

    let cursor = Position::new(cursor_x, cursor_y);
    frame.set_cursor_position(cursor);
    draw_send_mode_indicator(frame, app, area);
    Some(cursor)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WrappedInputSegment {
    text: String,
    start_grapheme: usize,
    end_grapheme: usize,
    display_width: usize,
}

/// Wrapped/raw text plus the wrapped-to-raw line map for the composer's
/// copy-selection snapshot. Wrapped rows mirror `wrap_input_segments` exactly
/// (the same function that lays out the rendered rows), while raw lines are
/// the logical `\n`-separated input lines, so copying a selection that spans
/// a soft wrap yields the original text without injected line breaks.
fn input_copy_snapshot_parts(
    input: &str,
    line_width: usize,
) -> (Vec<String>, Vec<String>, Vec<super::WrappedLineMap>) {
    let segments = wrap_input_segments(input, line_width);
    let raw_lines: Vec<String> = input.split('\n').map(str::to_owned).collect();

    // (raw_line, display_col) at each grapheme boundary of the input.
    let mut boundaries = Vec::with_capacity(input.graphemes(true).count() + 1);
    let mut raw_line = 0usize;
    let mut col = 0usize;
    boundaries.push((raw_line, col));
    for grapheme in input.graphemes(true) {
        if grapheme.ends_with('\n') {
            raw_line += 1;
            col = 0;
        } else {
            col += unicode_width::UnicodeWidthStr::width(grapheme);
        }
        boundaries.push((raw_line, col));
    }

    let mut wrapped_plain = Vec::with_capacity(segments.len());
    let mut line_map = Vec::with_capacity(segments.len());
    for segment in &segments {
        let (raw_line, start_col) = boundaries
            .get(segment.start_grapheme)
            .copied()
            .unwrap_or((0, 0));
        line_map.push(super::WrappedLineMap {
            raw_line,
            start_col,
            end_col: start_col + segment.display_width,
        });
        wrapped_plain.push(segment.text.clone());
    }
    (wrapped_plain, raw_lines, line_map)
}

fn wrap_input_segments(input: &str, line_width: usize) -> Vec<WrappedInputSegment> {
    let graphemes: Vec<&str> = input.graphemes(true).collect();
    if graphemes.is_empty() {
        return vec![WrappedInputSegment {
            text: String::new(),
            start_grapheme: 0,
            end_grapheme: 0,
            display_width: 0,
        }];
    }

    let mut segments = Vec::new();
    let mut pos = 0;
    let mut grapheme_count = 0;

    while pos <= graphemes.len() {
        let newline_pos = graphemes[pos..]
            .iter()
            .position(|grapheme| grapheme.ends_with('\n'));
        let segment_end = match newline_pos {
            Some(rel_pos) => pos + rel_pos,
            None => graphemes.len(),
        };

        let segment = &graphemes[pos..segment_end];
        let mut seg_pos = 0;
        loop {
            let mut display_width = 0;
            let mut end = seg_pos;
            while end < segment.len() {
                let cw = unicode_width::UnicodeWidthStr::width(segment[end]);
                if display_width + cw > line_width {
                    break;
                }
                display_width += cw;
                end += 1;
            }
            if end == seg_pos && seg_pos < segment.len() {
                end = seg_pos + 1;
                display_width = unicode_width::UnicodeWidthStr::width(segment[seg_pos]);
            }

            let text = segment[seg_pos..end].concat();
            let start_grapheme = grapheme_count;
            let end_grapheme = grapheme_count + (end - seg_pos);
            segments.push(WrappedInputSegment {
                text,
                start_grapheme,
                end_grapheme,
                display_width,
            });
            grapheme_count = end_grapheme;

            if end >= segment.len() {
                break;
            }
            seg_pos = end;
        }

        if newline_pos.is_some() {
            grapheme_count += 1;
            pos = segment_end + 1;
        } else {
            break;
        }
    }

    segments
}

fn cursor_col_for_segment(segment: &WrappedInputSegment, cursor_grapheme_pos: usize) -> usize {
    let graphemes_before = cursor_grapheme_pos.saturating_sub(segment.start_grapheme);
    segment
        .text
        .graphemes(true)
        .take(graphemes_before)
        .map(unicode_width::UnicodeWidthStr::width)
        .sum()
}

fn grapheme_offset_for_clicked_column(
    text: &str,
    target_col: usize,
    display_width: usize,
) -> usize {
    if target_col >= display_width {
        return text.graphemes(true).count();
    }

    let mut display_col = 0;
    let mut graphemes_before = 0;
    for grapheme in text.graphemes(true) {
        let cw = unicode_width::UnicodeWidthStr::width(grapheme);
        if cw == 0 {
            graphemes_before += 1;
            continue;
        }
        if target_col < display_col + cw {
            if (target_col - display_col).saturating_mul(2) >= cw {
                graphemes_before += 1;
            }
            return graphemes_before;
        }
        display_col += cw;
        graphemes_before += 1;
    }

    graphemes_before
}

pub(crate) fn input_cursor_pos_from_screen(
    app: &dyn TuiState,
    area: Rect,
    next_prompt: usize,
    column: u16,
    row: u16,
) -> Option<usize> {
    if !layout_utils::point_in_rect(column, row, area) {
        return None;
    }

    let input_text = app.input();
    let reserved_width = send_mode_reserved_width(app);
    let prompt_len = input_prompt_len(app, next_prompt);
    let line_width = (area.width as usize).saturating_sub(prompt_len + reserved_width);
    if line_width == 0 {
        return Some(app.cursor_pos().min(input_text.len()));
    }

    let wrapped_lines = wrap_input_segments(input_text, line_width);
    let hint_lines = input_hint_line_height(app) as usize;
    let visible_height = area.height as usize;
    let total_input_lines = wrapped_lines.len().max(1);

    let scroll_offset = if total_input_lines + hint_lines <= visible_height {
        0
    } else {
        let available_for_input = visible_height.saturating_sub(hint_lines).max(1);
        if total_input_lines <= available_for_input {
            0
        } else {
            total_input_lines.saturating_sub(available_for_input)
        }
    };

    let screen_line = row.saturating_sub(area.y) as usize;
    if screen_line < hint_lines {
        return None;
    }

    let max_visible_input_lines = visible_height.saturating_sub(hint_lines).max(1);
    let input_screen_line = screen_line.saturating_sub(hint_lines);
    let line_index = (scroll_offset
        + input_screen_line.min(max_visible_input_lines.saturating_sub(1)))
    .min(wrapped_lines.len().saturating_sub(1));
    let segment = &wrapped_lines[line_index];

    let actual_line_width = prompt_len + segment.display_width;
    let text_start_x = if app.centered_mode() {
        let center_offset = (area.width as usize).saturating_sub(actual_line_width) / 2;
        area.x as usize + center_offset + prompt_len
    } else {
        area.x as usize + prompt_len
    };
    let target_col = column.saturating_sub(text_start_x as u16) as usize;
    let grapheme_offset =
        grapheme_offset_for_clicked_column(&segment.text, target_col, segment.display_width);
    let grapheme_index = segment.start_grapheme + grapheme_offset;

    Some(crate::alphacode_tui::tui::core::grapheme_index_to_byte_offset(input_text, grapheme_index))
}

pub(crate) fn wrap_input_text(
    input: &str,
    cursor_pos: usize,
    line_width: usize,
    num_str: &str,
    prompt_char: &str,
    caret_color: Color,
    prompt_len: usize,
) -> (Vec<Line<'static>>, usize, usize) {
    // Every span built below is owned (`to_string()` / `segment.text.clone()`),
    // so the returned Lines never borrow `prompt_char`. Dropping the lifetime
    // lets callers build the prefix at runtime — it now embeds the active
    // skill name — without borrowing a local binding that dies before the
    // frame is drawn.
    let cursor_grapheme_pos =
        crate::alphacode_tui::tui::core::byte_offset_to_grapheme_index(input, cursor_pos);
    let wrapped_segments = wrap_input_segments(input, line_width);
    let mut lines: Vec<Line> = Vec::new();
    let mut cursor_line = 0;
    let mut cursor_col = 0;
    let mut found_cursor = false;

    // Keep the composer actionable without adding vertical chrome. On wide
    // terminals the placeholder teaches the slash palette and F1 help; on
    // narrow terminals it falls back to a short prompt that still fits.
    let empty_placeholder = if input.is_empty() {
        let placeholder = empty_composer_placeholder(line_width, prompt_char);
        (!placeholder.is_empty()).then_some(placeholder)
    } else {
        None
    };

    for (idx, segment) in wrapped_segments.iter().enumerate() {
        if !found_cursor
            && cursor_grapheme_pos >= segment.start_grapheme
            && cursor_grapheme_pos <= segment.end_grapheme
        {
            cursor_line = idx;
            cursor_col = cursor_col_for_segment(segment, cursor_grapheme_pos);
            found_cursor = true;
        }

        if idx == 0 {
            let num_color = rainbow_prompt_color(0);
            let mut first_row_spans: Vec<Span<'static>> = vec![
                Span::styled(num_str.to_string(), Style::default().fg(num_color)),
                Span::styled(prompt_char.to_string(), Style::default().fg(caret_color)),
            ];
            if let Some(placeholder) = &empty_placeholder {
                // Select by terminal display width so wide glyphs can never
                // push the hint into the send-mode indicator column.
                first_row_spans.push(Span::styled(
                    placeholder.clone(),
                    Style::default()
                        .fg(dim_color())
                        .add_modifier(Modifier::ITALIC),
                ));
            } else {
                first_row_spans.push(Span::raw(segment.text.clone()));
            }
            lines.push(Line::from(first_row_spans));
        } else {
            lines.push(Line::from(vec![
                Span::raw(" ".repeat(prompt_len)),
                Span::raw(segment.text.clone()),
            ]));
        }
    }

    if !found_cursor {
        cursor_line = wrapped_segments.len().saturating_sub(1);
        cursor_col = wrapped_segments
            .last()
            .map(|segment| segment.display_width)
            .unwrap_or(0);
    }

    (lines, cursor_line, cursor_col)
}

/// Return the most useful empty-composer prompt that fits the available text
/// columns. The full hint teaches the command palette and direct help shortcut;
// narrower terminals progressively keep the prompt readable.
///
/// While a turn is running the composer is read-only, so it gets status text
/// instead. Showing "Ask alphacode anything…" next to the busy glyph was a
/// real trap: it read as "type here", keystrokes were queued against a turn
/// that could not accept them yet, and nothing on screen distinguished a
/// prompt from a spinner.
///
/// Busy state is derived from the prompt glyph the caller already resolved
/// (`input_prompt` returns the ellipsis while processing) rather than taking
/// another parameter, which would have churned every caller and test for a
/// value the caller already has.
fn empty_composer_placeholder(max_width: usize, prompt_char: &str) -> String {
    let busy = prompt_char.trim_start().starts_with('\u{2026}');
    let candidates: &[&str] = if busy {
        &[
            "Working… · Esc interrupts",
            "Working… · Esc interrupts",
            "Working…",
            "Thinking…",
        ]
    } else {
        &[
            "Ask alphacode anything… · / commands · F1 help",
            "Ask anything… · / commands · F1 help",
            "Ask anything… · / commands · /theme look",
            "Ask anything… · / commands",
            "Ask anything…",
            "Type…",
        ]
    };
    candidates
        .iter()
        .copied()
        .find(|candidate| unicode_width::UnicodeWidthStr::width(*candidate) <= max_width)
        .unwrap_or("")
        .to_string()
}

fn send_mode_indicator(app: &dyn TuiState) -> (&'static str, Color) {
    let mode = composer_mode(app.input(), app.is_remote_mode());
    if mode.is_shell() {
        ("$", shell_mode_color())
    } else if app.next_prompt_new_session_armed() {
        ("↗", info_color())
    } else if app.queue_mode() {
        ("⏳", queued_color())
    } else if let Some(ref conn) = app.connection_type() {
        let lower = conn.to_lowercase();
        if lower.contains("websocket") {
            ("󰌘", success_color())
        } else if lower.contains("subprocess") || lower.contains("cli") {
            ("󰆍", accent_color())
        } else {
            ("󰖟", info_color())
        }
    } else {
        ("", asap_color())
    }
}

fn draw_send_mode_indicator(frame: &mut Frame, app: &dyn TuiState, area: Rect) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let indicator_area = Rect {
        x: area.x,
        y: area.y + area.height.saturating_sub(1),
        width: area.width,
        height: 1,
    };

    let (icon, color) = send_mode_indicator(app);
    if !icon.is_empty() {
        let line = Line::from(Span::styled(icon, Style::default().fg(color)));
        let paragraph = Paragraph::new(line).alignment(Alignment::Right);
        frame.render_widget(paragraph, indicator_area);
    }
}

#[derive(Clone, Copy)]
enum QueuedMsgType {
    Pending,
    Interleave,
    Queued,
}
