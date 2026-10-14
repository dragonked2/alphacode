use super::*;
use crate::alphacode_tui_style::palette::{Role, role_color};
use crate::alphacode_tui_style::role::themed_rgb;
use ratatui::style::Modifier;

#[cfg(test)]
use super::text::spans_width;

/// Most todo rows the compact widget shows before collapsing to "+N more".
const MAX_WIDGET_ITEMS: usize = 5;
/// Design caps for the pip meter; the real budget is whatever width is left
/// in the header after everything else has been placed.
const WIDGET_PIP_CAP: usize = 12;
const EXPANDED_PIP_CAP: usize = 16;
/// A row keeps its optional parts (priority marker, confidence, "(blocked)")
/// only while at least this many columns remain for the title.
const MIN_TITLE_COLS: usize = 10;
/// A group header keeps its confidence / loop suffixes only while at least this
/// many columns remain for the group name.
const MIN_GROUP_NAME_COLS: usize = 8;
/// Left-aligned partial blocks for 1/8 .. 7/8 of a cell.
const PARTIAL_BLOCKS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];

// ---------------------------------------------------------------------------
// Swarm plan projection
// ---------------------------------------------------------------------------

/// Map swarm plan items into the todo-widget model so the persistent info
/// widget renders live plan state (this is the durable surface backing the
/// transient 3s "Swarm plan synced" status notice).
///
/// Plan statuses use the scheduler vocabulary (`queued`, `ready`, `running`,
/// `running_stale`, `done`, `failed`, `stopped`, `crashed`, ...) while the todo
/// renderer only distinguishes `in_progress`/`completed`/`cancelled`/other.
/// Without normalization, `running` plan tasks render as open `○` items and
/// sort *after* completed work, so large plans hide all live activity behind
/// the "+N more" footer.
pub(crate) fn swarm_plan_todos(items: &[crate::plan::PlanItem]) -> Vec<crate::todo::TodoItem> {
    items
        .iter()
        .map(|item| crate::todo::TodoItem {
            content: item.content.clone(),
            status: normalize_plan_status_for_todo(&item.status),
            priority: item.priority.clone(),
            id: item.id.clone(),
            group: None,
            blocked_by: item.blocked_by.clone(),
            assigned_to: item.assigned_to.clone(),
            confidence: None,
            completion_confidence: None,
            confidence_history: Vec::new(),
        })
        .collect()
}

/// Collapse the scheduler's status vocabulary onto the todo renderer's:
/// active → `in_progress` (▶ amber, sorts first), terminal success →
/// `completed` (✓), terminal failure → `cancelled` (✗), runnable →
/// `pending` (○). Matching ignores case and surrounding whitespace; statuses
/// the todo renderer already understands (and any arbitrary strings) pass
/// through unchanged. Blocked items still get their ⊳ marker from `blocked_by`.
fn normalize_plan_status_for_todo(status: &str) -> String {
    match status.trim().to_ascii_lowercase().as_str() {
        "running" | "running_stale" => "in_progress".to_string(),
        "done" => "completed".to_string(),
        "failed" | "stopped" | "crashed" => "cancelled".to_string(),
        "queued" | "ready" | "todo" | "blocked" => "pending".to_string(),
        _ => status.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Theme
// ---------------------------------------------------------------------------
//
// Everything except the decorative progress gradient resolves through palette
// roles (or the terminal's own foreground), so `/theme` retunes the widget and
// light themes keep their contrast instead of inheriting fixed RGB literals.

fn tone_success() -> Color {
    role_color(Role::Success)
}
fn tone_warning() -> Color {
    role_color(Role::Warning)
}
fn tone_danger() -> Color {
    role_color(Role::Error)
}
fn tone_muted() -> Color {
    role_color(Role::MutedText)
}
fn tone_faint() -> Color {
    themed_rgb(Role::Border, (-10, -10, 5))
}

fn style_success() -> Style {
    Style::default().fg(tone_success())
}
fn style_warning() -> Style {
    Style::default().fg(tone_warning())
}
fn style_danger() -> Style {
    Style::default().fg(tone_danger())
}
fn style_muted() -> Style {
    Style::default().fg(tone_muted())
}
fn style_dim() -> Style {
    Style::default()
        .fg(tone_muted())
        .add_modifier(Modifier::DIM)
}
fn style_faint() -> Style {
    Style::default().fg(tone_faint())
}
/// Emphasis in the terminal's own foreground color.
fn style_title() -> Style {
    Style::default().add_modifier(Modifier::BOLD)
}

fn spans_cols(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|span| span.content.width()).sum()
}

// ---------------------------------------------------------------------------
// Confidence and goals
// ---------------------------------------------------------------------------

fn todo_confidence_weight(priority: &str) -> u32 {
    match priority {
        "high" => 3,
        "medium" => 2,
        _ => 1,
    }
}

fn todo_display_confidence(todo: &crate::todo::TodoItem) -> Option<u8> {
    if todo.status == "completed" {
        todo.completion_confidence.or(todo.confidence)
    } else {
        todo.confidence
    }
}

fn aggregate_todo_confidence<'a>(
    todos: impl IntoIterator<Item = &'a crate::todo::TodoItem>,
) -> Option<u8> {
    let mut weighted_sum = 0u32;
    let mut total_weight = 0u32;
    for todo in todos.into_iter().filter(|todo| todo.status != "cancelled") {
        let Some(score) = todo_display_confidence(todo) else {
            continue;
        };
        let weight = todo_confidence_weight(&todo.priority);
        weighted_sum += u32::from(score) * weight;
        total_weight += weight;
    }
    if total_weight == 0 {
        None
    } else {
        Some(((weighted_sum + total_weight / 2) / total_weight) as u8)
    }
}

fn confidence_style(score: Option<u8>) -> Style {
    match score {
        Some(90..=100) => style_success(),
        Some(70..=89) => style_warning(),
        Some(_) => style_danger(),
        None => style_dim(),
    }
}

fn confidence_label(score: Option<u8>) -> String {
    score
        .map(|score| format!("{}%", score))
        .unwrap_or_else(|| "?%".to_string())
}

/// Find the goal assessment recorded for a todo group (`None` = the
/// ungrouped/flat list). Group labels are compared after trimming, matching
/// how the todo tool normalizes them.
fn goal_for_group<'a>(
    goals: &'a [crate::todo::TodoGoal],
    group: Option<&str>,
) -> Option<&'a crate::todo::TodoGoal> {
    let key = group.map(str::trim).filter(|group| !group.is_empty());
    goals.iter().find(|goal| {
        goal.group
            .as_deref()
            .map(str::trim)
            .filter(|group| !group.is_empty())
            == key
    })
}

/// Color for a closed feedback loop score: green when progress has a credible
/// metric to iterate against, red when it is low (below the reframe-nudge
/// threshold), amber in between.
fn loop_style(score: u8) -> Style {
    if score >= crate::todo::LOW_CLOSED_FEEDBACK_LOOP {
        style_success()
    } else if score >= crate::todo::LOW_CLOSED_FEEDBACK_LOOP.saturating_sub(20) {
        style_warning()
    } else {
        style_danger()
    }
}

/// Append a " · loop N%" suffix describing a goal's closed feedback loop.
fn push_goal_loop_suffix(spans: &mut Vec<Span<'static>>, goal: &crate::todo::TodoGoal) {
    let Some(score) = goal.closed_feedback_loop else {
        return;
    };
    spans.push(Span::styled(" · ", style_faint()));
    spans.push(Span::styled("loop ", style_dim()));
    spans.push(Span::styled(format!("{}%", score), loop_style(score)));
}

/// Display width of the suffix `push_goal_loop_suffix` would render for this
/// goal (0 when it renders nothing), so header truncation can reserve room.
fn goal_loop_suffix_width(goal: &crate::todo::TodoGoal) -> u16 {
    match goal.closed_feedback_loop {
        Some(score) => (" · loop ".width() + format!("{score}%").width()) as u16,
        None => 0,
    }
}

fn todo_confidence_suffix_width(todo: &crate::todo::TodoItem) -> u16 {
    (" · ".width() + confidence_label(todo_display_confidence(todo)).width()) as u16
}

fn push_todo_confidence_suffix(spans: &mut Vec<Span<'static>>, todo: &crate::todo::TodoItem) {
    let score = todo_display_confidence(todo);
    spans.push(Span::styled(" · ", style_faint()));
    spans.push(Span::styled(
        confidence_label(score),
        confidence_style(score),
    ));
}

fn aggregate_confidence_suffix_width(score: Option<u8>) -> u16 {
    match score {
        Some(score) => (" · confidence ".width() + confidence_label(Some(score)).width()) as u16,
        None => 0,
    }
}

fn push_aggregate_confidence_suffix(spans: &mut Vec<Span<'static>>, score: Option<u8>) {
    let Some(score) = score else {
        return;
    };
    spans.push(Span::styled(" \u{00b7} ", style_faint()));
    spans.push(Span::styled("confidence ", style_dim()));
    spans.push(Span::styled(
        confidence_label(Some(score)),
        confidence_style(Some(score)),
    ));
}

// ---------------------------------------------------------------------------
// Counting and the pip meter
// ---------------------------------------------------------------------------

/// One pass over the list, so every surface that reports progress agrees.
///
/// The buckets are disjoint and sum to `total`. That matters because the widget
/// reports the same list several ways (pip meter, `n/m` counter, compact
/// summary, group headers) and they must not disagree: "open" is never derived
/// as `total - completed`, which would silently fold in-progress and cancelled
/// work into the open count.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct TodoCounts {
    pub total: usize,
    pub completed: usize,
    pub in_progress: usize,
    pub cancelled: usize,
    pub open: usize,
}

impl TodoCounts {
    /// Items that can still be finished. This is the denominator for the `n/m`
    /// counter: cancelled work is not outstanding work, so abandoning an item
    /// should advance the fraction rather than stall it below 1 forever.
    pub(crate) fn actionable(self) -> usize {
        self.total.saturating_sub(self.cancelled)
    }

    /// Fraction of actionable work that is done, or `None` when nothing is
    /// actionable (so callers draw no bar rather than a misleading one).
    fn progress(self) -> Option<f64> {
        let actionable = self.actionable();
        (actionable > 0).then(|| self.completed as f64 / actionable as f64)
    }
}

fn count_iter<'a>(todos: impl Iterator<Item = &'a crate::todo::TodoItem>) -> TodoCounts {
    let mut counts = TodoCounts::default();
    for todo in todos {
        counts.total += 1;
        match todo.status.as_str() {
            "completed" => counts.completed += 1,
            "in_progress" => counts.in_progress += 1,
            "cancelled" => counts.cancelled += 1,
            _ => {}
        }
    }
    // Everything the renderer does not recognize (`pending`, `blocked`, or an
    // arbitrary status a plan invented) is open work.
    counts.open = counts
        .total
        .saturating_sub(counts.completed + counts.in_progress + counts.cancelled);
    counts
}

pub(crate) fn count_todos(todos: &[crate::todo::TodoItem]) -> TodoCounts {
    count_iter(todos.iter())
}

/// Scale the four buckets into `max_pips` columns.
///
/// The render order is fixed (done, active, open, cancelled) but importance is
/// not: when there are fewer columns than states present, the states that
/// describe *remaining* work win, because those are what a reader is checking.
/// The result never exceeds `max_pips`, so the meter cannot shove the rest of
/// the header off the panel.
fn collapse_pips(counts: TodoCounts, max_pips: usize) -> [usize; 4] {
    let sizes = [
        counts.completed,
        counts.in_progress,
        counts.open,
        counts.cancelled,
    ];
    /// Most informative first: what is running, what is left, what is
    /// finished, what was abandoned.
    const BY_IMPORTANCE: [usize; 4] = [1, 2, 0, 3];

    let mut pips = [0usize; 4];
    let present = sizes.iter().filter(|count| **count > 0).count();

    if present >= max_pips {
        // Too narrow for a proportional meter: one pip each to as many states
        // as fit, most important first.
        for index in BY_IMPORTANCE {
            if sizes[index] == 0 || pips.iter().sum::<usize>() >= max_pips {
                continue;
            }
            pips[index] = 1;
        }
        return pips;
    }

    // Every state present keeps at least one pip. A single in-progress item in
    // a list of 200 is exactly the state the user is watching, and
    // proportional rounding alone would round it away.
    for (index, &count) in sizes.iter().enumerate() {
        if count == 0 {
            continue;
        }
        let exact = (count as f64 / counts.total as f64) * max_pips as f64;
        pips[index] = (exact.round() as usize).max(1);
    }

    // Rounding each bucket up can overshoot. Trim the widest first so the
    // small buckets survive; `present < max_pips` guarantees this converges.
    let mut over = pips.iter().sum::<usize>().saturating_sub(max_pips);
    while over > 0 {
        let Some(widest) = pips
            .iter()
            .enumerate()
            .filter(|(_, count)| **count > 1)
            .max_by_key(|(_, count)| **count)
            .map(|(index, _)| index)
        else {
            break;
        };
        pips[widest] -= 1;
        over -= 1;
    }

    pips
}

/// Build a compact pip-dot status meter for a set of todos.
///
/// Each todo becomes one pip: green filled = completed, amber filled =
/// in_progress, hollow = open, dim dot = cancelled. A list that fits in
/// `width_pips` columns gets an exact 1:1 pip per todo; only larger lists
/// collapse to a proportional summary. `width_pips` is a hard budget: callers
/// derive it from the room actually left in the header, so the meter can never
/// push the row past the panel edge.
fn push_todo_pips(spans: &mut Vec<Span<'static>>, data: &InfoWidgetData, width_pips: usize) {
    let counts = count_todos(&data.todos);
    if counts.total == 0 || width_pips == 0 {
        return;
    }

    let buckets = if counts.total <= width_pips {
        [
            counts.completed,
            counts.in_progress,
            counts.open,
            counts.cancelled,
        ]
    } else {
        collapse_pips(counts, width_pips)
    };
    if buckets.iter().sum::<usize>() == 0 {
        return;
    }

    spans.push(Span::raw("  "));

    // Status order: done, active, open, cancelled.
    let styling = [
        ("●", style_success()),
        ("●", style_warning()),
        ("○", style_dim()),
        ("·", style_faint()),
    ];
    for (count, (glyph, style)) in buckets.into_iter().zip(styling) {
        for _ in 0..count {
            spans.push(Span::styled(glyph, style));
        }
    }
}

// ---------------------------------------------------------------------------
// Progress bar
// ---------------------------------------------------------------------------

/// Linear interpolation between two f64 values.
fn lerp_f64(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

/// Three-segment gradient: warm amber → green → teal. Position-based, so a
/// given cell keeps its color as the bar fills instead of the whole gradient
/// being re-stretched over the filled part every frame.
fn gradient_color(t: f64) -> Color {
    const STOPS: [(f64, f64, f64); 4] = [
        (238.0, 128.0, 88.0),
        (188.0, 208.0, 108.0),
        (108.0, 230.0, 158.0),
        (98.0, 218.0, 208.0),
    ];
    let t = if t.is_finite() {
        t.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let scaled = t * 3.0;
    let seg = (scaled.floor() as usize).min(2);
    let local = scaled - seg as f64;
    let (a, b) = (STOPS[seg], STOPS[seg + 1]);
    rgb(
        lerp_f64(a.0, b.0, local).round() as u8,
        lerp_f64(a.1, b.1, local).round() as u8,
        lerp_f64(a.2, b.2, local).round() as u8,
    )
}

/// Progress bar with 1/8-cell resolution and a fixed per-cell gradient.
///
/// Honest by construction: 0 draws no fill, anything nonzero draws at least a
/// sliver, and only exactly 100% fills the last cell.
fn progress_bar_spans(progress: f64, width: usize) -> Vec<Span<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let p = if progress.is_finite() {
        progress.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let total = width * 8;
    let mut eighths = (p * total as f64).round() as usize;
    if p > 0.0 && eighths == 0 {
        eighths = 1;
    }
    if p < 1.0 && eighths >= total {
        eighths = total - 1;
    }
    let full = eighths / 8;
    let part = eighths % 8;

    let cell_color = |i: usize| {
        gradient_color(if width <= 1 {
            1.0
        } else {
            i as f64 / (width - 1) as f64
        })
    };

    let mut spans = Vec::with_capacity(width + 1);
    for i in 0..full {
        spans.push(Span::styled("█", Style::default().fg(cell_color(i))));
    }
    let mut used = full;
    if part > 0 {
        spans.push(Span::styled(
            PARTIAL_BLOCKS[part - 1].to_string(),
            Style::default().fg(cell_color(full)),
        ));
        used += 1;
    }
    let empty = width.saturating_sub(used);
    if empty > 0 {
        spans.push(Span::styled("░".repeat(empty), style_faint()));
    }
    spans
}

/// A full-width progress line indented by two columns, or `None` when there is
/// nothing actionable to measure or no room for a meaningful bar.
fn progress_line(counts: TodoCounts, width: u16, min_bar: usize) -> Option<Line<'static>> {
    let bar_width = usize::from(width.saturating_sub(2));
    if bar_width < min_bar {
        return None;
    }
    let progress = counts.progress()?;
    let mut spans = vec![Span::raw("  ")];
    spans.extend(progress_bar_spans(progress, bar_width));
    Some(Line::from(spans))
}

// ---------------------------------------------------------------------------
// Grouping and ordering
// ---------------------------------------------------------------------------

/// Normalize a todo's group label, treating empty/whitespace as ungrouped.
fn todo_group_key(todo: &crate::todo::TodoItem) -> Option<String> {
    todo.group
        .as_deref()
        .map(str::trim)
        .filter(|group| !group.is_empty())
        .map(|group| group.to_string())
}

type TodoGroup<'a> = (Option<String>, Vec<&'a crate::todo::TodoItem>);

/// Partition todos into ordered groups, preserving the order groups first
/// appear. Ungrouped items collapse into a trailing `None` bucket. Returns
/// `None` when no todo declares a group, so callers fall back to the flat list.
fn grouped_todos(todos: &[crate::todo::TodoItem]) -> Option<Vec<TodoGroup<'_>>> {
    if !todos.iter().any(|todo| todo_group_key(todo).is_some()) {
        return None;
    }
    let mut groups: Vec<TodoGroup<'_>> = Vec::new();
    for todo in todos {
        let key = todo_group_key(todo);
        if let Some(entry) = groups.iter_mut().find(|(existing, _)| *existing == key) {
            entry.1.push(todo);
        } else {
            groups.push((key, vec![todo]));
        }
    }
    // Keep the ungrouped bucket last; sort_by_key is stable so named groups
    // retain their first-seen order.
    groups.sort_by_key(|(key, _)| key.is_none());
    Some(groups)
}

fn status_sort_rank(status: &str) -> u8 {
    match status {
        "in_progress" => 0,
        "pending" => 1,
        "completed" => 2,
        "cancelled" => 3,
        _ => 4,
    }
}

/// Blocked, not-yet-finished work. Shown with ⊳ and sorted behind runnable
/// items of the same status.
fn is_blocked(todo: &crate::todo::TodoItem) -> bool {
    !todo.blocked_by.is_empty() && !matches!(todo.status.as_str(), "completed" | "cancelled")
}

/// Active work first, then runnable open items, then blocked ones, then
/// finished and abandoned work. The sort is stable, so ties keep list order.
fn sort_todos_by_status<'a>(todos: &[&'a crate::todo::TodoItem]) -> Vec<&'a crate::todo::TodoItem> {
    let mut sorted: Vec<&crate::todo::TodoItem> = todos.to_vec();
    sorted.sort_by_key(|todo| (status_sort_rank(&todo.status), is_blocked(todo)));
    sorted
}

/// Does any todo carry a confidence score? When none do, a `?%` on every row is
/// pure noise; when some do, the unscored ones are worth flagging.
fn any_scored<'a>(todos: impl IntoIterator<Item = &'a crate::todo::TodoItem>) -> bool {
    todos
        .into_iter()
        .any(|todo| todo_display_confidence(todo).is_some())
}

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct RowOpts {
    /// Show the `‼` marker for high-priority items (expanded widget).
    priority_marker: bool,
    /// Leading-space depth used when items sit under a group header.
    indent: usize,
    /// Show `?%` for items without a score (only meaningful when peers have one).
    flag_unscored: bool,
}

/// Render one todo as a line.
///
/// Optional parts (`(blocked)`, confidence, priority marker) are dropped in
/// that order whenever keeping them would leave the title fewer than
/// `MIN_TITLE_COLS` columns, so a narrow panel loses decoration before it loses
/// the thing the row exists to say. All widths are display columns, never byte
/// lengths.
fn push_todo_item_line(
    lines: &mut Vec<Line<'static>>,
    todo: &crate::todo::TodoItem,
    inner: Rect,
    opts: RowOpts,
) {
    let blocked = is_blocked(todo);
    let (icon, icon_style) = if blocked {
        ("⊳", style_warning().add_modifier(Modifier::DIM))
    } else {
        match todo.status.as_str() {
            "completed" => ("\u{2713}", style_success()),
            "in_progress" => ("\u{25b6}", style_warning().add_modifier(Modifier::BOLD)),
            "cancelled" => ("\u{2717}", style_danger().add_modifier(Modifier::DIM)),
            _ => ("\u{25cb}", style_muted()),
        }
    };
    let icon_span = format!("{icon} ");

    let marker = if opts.priority_marker && todo.priority == "high" {
        "\u{203c} "
    } else {
        ""
    };
    let blocked_label = if blocked { " (blocked)" } else { "" };
    let show_conf = opts.flag_unscored || todo_display_confidence(todo).is_some();
    let conf_cols = if show_conf {
        usize::from(todo_confidence_suffix_width(todo))
    } else {
        0
    };

    let width = usize::from(inner.width);
    let fixed = opts.indent + icon_span.width();
    let reserved = |blocked_kept: bool, conf_kept: bool, marker_kept: bool| {
        fixed
            + if blocked_kept {
                blocked_label.width()
            } else {
                0
            }
            + if conf_kept { conf_cols } else { 0 }
            + if marker_kept { marker.width() } else { 0 }
    };
    let fits =
        |b: bool, c: bool, m: bool| width.saturating_sub(reserved(b, c, m)) >= MIN_TITLE_COLS;

    let (mut keep_blocked, mut keep_conf, mut keep_marker) = (true, show_conf, true);
    if !fits(keep_blocked, keep_conf, keep_marker) {
        keep_blocked = false;
        if !fits(keep_blocked, keep_conf, keep_marker) {
            keep_conf = false;
            if !fits(keep_blocked, keep_conf, keep_marker) {
                keep_marker = false;
            }
        }
    }

    let max_len = width.saturating_sub(reserved(keep_blocked, keep_conf, keep_marker));
    let content: String = if max_len == 0 {
        String::new()
    } else {
        truncate_smart(&todo.content, max_len).to_string()
    };

    let content_style = match todo.status.as_str() {
        "completed" => style_dim().add_modifier(Modifier::CROSSED_OUT),
        "in_progress" => style_title(),
        _ if blocked => style_dim(),
        _ => style_muted(),
    };

    let mut spans = Vec::new();
    if opts.indent > 0 {
        spans.push(Span::raw(" ".repeat(opts.indent)));
    }
    spans.push(Span::styled(icon_span, icon_style));
    if keep_marker && !marker.is_empty() {
        spans.push(Span::styled(
            marker,
            style_danger().add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(content, content_style));
    if keep_conf {
        push_todo_confidence_suffix(&mut spans, todo);
    }
    if keep_blocked && !blocked_label.is_empty() {
        spans.push(Span::styled(blocked_label, style_dim()));
    }
    lines.push(Line::from(spans));
}

fn footer_line(remaining: usize, remaining_done: usize) -> Line<'static> {
    let text = if remaining_done == remaining {
        format!("  +{remaining} done")
    } else if remaining_done > 0 {
        format!("  +{remaining} more ({remaining_done} done)")
    } else {
        format!("  +{remaining} more")
    };
    Line::from(Span::styled(text, style_dim()))
}

// ---------------------------------------------------------------------------
// Headers
// ---------------------------------------------------------------------------

/// Header label for the todo slot: "Plan" when the items are the shared
/// swarm plan projection, "Todos" for the session's own private list.
fn todos_widget_label(data: &InfoWidgetData) -> &'static str {
    if data.todos_are_swarm_plan {
        "Plan"
    } else {
        "Todos"
    }
}

/// `Todos 2/5  ●●▶○○ · confidence 82% · loop 70%`, fitted to `width`.
///
/// The label and counter always render. Everything else is placed against the
/// room actually left: confidence first, then the loop score (each only if the
/// pips would still have a few columns afterwards), and the pip meter takes
/// what remains up to `pip_cap`.
fn todos_header_spans(
    data: &InfoWidgetData,
    counts: TodoCounts,
    width: u16,
    pip_cap: usize,
    goal: Option<&crate::todo::TodoGoal>,
) -> Vec<Span<'static>> {
    let mut spans = vec![
        Span::styled(format!("{} ", todos_widget_label(data)), style_title()),
        Span::styled(
            format!("{}/{}", counts.completed, counts.actionable()),
            style_muted(),
        ),
    ];
    let mut room = usize::from(width).saturating_sub(spans_cols(&spans));

    let confidence = aggregate_todo_confidence(&data.todos);
    let confidence_cols = usize::from(aggregate_confidence_suffix_width(confidence));
    let loop_cols = goal.map_or(0, |g| usize::from(goal_loop_suffix_width(g)));

    // Two columns of spacer plus a few pips is the least worth drawing.
    let reserve = 2 + counts.total.min(4);
    let show_confidence = confidence_cols > 0 && room >= confidence_cols + reserve;
    if show_confidence {
        room -= confidence_cols;
    }
    let show_loop = loop_cols > 0 && room >= loop_cols + reserve;
    if show_loop {
        room -= loop_cols;
    }

    push_todo_pips(&mut spans, data, room.saturating_sub(2).min(pip_cap));
    if show_confidence {
        push_aggregate_confidence_suffix(&mut spans, confidence);
    }
    if show_loop && let Some(goal) = goal {
        push_goal_loop_suffix(&mut spans, goal);
    }
    spans
}

fn push_group_header(
    lines: &mut Vec<Line<'static>>,
    name: &str,
    items: &[&crate::todo::TodoItem],
    goal: Option<&crate::todo::TodoGoal>,
    inner: Rect,
) {
    // Same denominator as the widget header: cancelled work is not outstanding.
    let counts = count_iter(items.iter().copied());
    let counter = format!(" {}/{}", counts.completed, counts.actionable());
    let confidence = aggregate_todo_confidence(items.iter().copied());
    let confidence_cols = usize::from(aggregate_confidence_suffix_width(confidence));
    let loop_cols = goal.map_or(0, |g| usize::from(goal_loop_suffix_width(g)));

    let mut room = usize::from(inner.width).saturating_sub(counter.width());
    let show_confidence = confidence_cols > 0 && room >= confidence_cols + MIN_GROUP_NAME_COLS;
    if show_confidence {
        room -= confidence_cols;
    }
    let show_loop = loop_cols > 0 && room >= loop_cols + MIN_GROUP_NAME_COLS;
    if show_loop {
        room -= loop_cols;
    }

    let highlight = items.iter().any(|t| t.status == "in_progress");
    let name_style = if highlight {
        style_warning().add_modifier(Modifier::BOLD)
    } else {
        style_title()
    };
    let name = if room == 0 {
        String::new()
    } else {
        truncate_smart(name, room).to_string()
    };

    let mut spans = vec![
        Span::styled(name, name_style),
        Span::styled(counter, style_muted()),
    ];
    if show_confidence {
        push_aggregate_confidence_suffix(&mut spans, confidence);
    }
    if show_loop && let Some(goal) = goal {
        push_goal_loop_suffix(&mut spans, goal);
    }
    lines.push(Line::from(spans));
}

// ---------------------------------------------------------------------------
// Bodies
// ---------------------------------------------------------------------------

/// Render todos partitioned by group, honoring a `max_lines` budget that counts
/// both group headers and item rows. Returns the rendered lines, the number of
/// todo items actually shown, and how many of the *hidden* items are completed
/// (for the "+N more (M done)" footer).
fn render_grouped_todo_lines(
    groups: &[TodoGroup<'_>],
    goals: &[crate::todo::TodoGoal],
    inner: Rect,
    show_priority_marker: bool,
    max_lines: usize,
) -> (Vec<Line<'static>>, usize, usize) {
    let opts = RowOpts {
        priority_marker: show_priority_marker,
        indent: 2,
        flag_unscored: groups
            .iter()
            .any(|(_, items)| any_scored(items.iter().copied())),
    };
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut shown = 0usize;
    let mut hidden_done = 0usize;

    for (group, items) in groups {
        let sorted = sort_todos_by_status(items);
        let mut shown_here = 0usize;

        // A header needs room for at least one item beneath it. Spending the
        // last line of the budget on a heading that describes nothing costs a
        // row and tells the reader less than the "+N more" footer would.
        if lines.len() + 1 < max_lines {
            let header_name = group.as_deref().unwrap_or("Other");
            let goal = goal_for_group(goals, group.as_deref());
            push_group_header(&mut lines, header_name, items, goal, inner);
            for todo in &sorted {
                if lines.len() >= max_lines {
                    break;
                }
                push_todo_item_line(&mut lines, todo, inner, opts);
                shown_here += 1;
            }
        }

        shown += shown_here;
        hidden_done += sorted[shown_here..]
            .iter()
            .filter(|t| t.status == "completed")
            .count();
    }
    (lines, shown, hidden_done)
}

/// Grouped body plus overflow footer, never exceeding `rows` lines. Content is
/// preferred over the footer: the item budget only shrinks to make room for the
/// footer while something is still shown.
fn grouped_body(
    groups: &[TodoGroup<'_>],
    goals: &[crate::todo::TodoGoal],
    inner: Rect,
    show_priority_marker: bool,
    cap: usize,
    rows: usize,
) -> Vec<Line<'static>> {
    let total: usize = groups.iter().map(|(_, items)| items.len()).sum();
    let rows = rows.max(1);
    let mut limit = cap.max(1).min(rows);
    let mut previous: Option<Vec<Line<'static>>> = None;

    loop {
        let (mut lines, shown, hidden_done) =
            render_grouped_todo_lines(groups, goals, inner, show_priority_marker, limit);
        let hidden = total - shown;
        if hidden == 0 {
            return lines;
        }
        if shown == 0 {
            // Shrinking emptied the body; the previous render was more useful.
            return previous.unwrap_or(lines);
        }
        if lines.len() < rows {
            lines.push(footer_line(hidden, hidden_done));
            return lines;
        }
        if limit <= 1 {
            return lines;
        }
        previous = Some(lines);
        limit -= 1;
    }
}

/// Flat body plus overflow footer, never exceeding `rows` lines.
fn flat_body(
    sorted: &[&crate::todo::TodoItem],
    inner: Rect,
    opts: RowOpts,
    cap: usize,
    rows: usize,
) -> Vec<Line<'static>> {
    let total = sorted.len();
    let rows = rows.max(1);
    let mut limit = cap.max(1).min(rows);
    // Make room for the footer when the list overflows a full panel.
    if total > limit && limit == rows && limit > 1 {
        limit -= 1;
    }

    let mut lines = Vec::new();
    for todo in sorted.iter().take(limit) {
        push_todo_item_line(&mut lines, todo, inner, opts);
    }
    if total > limit && lines.len() < rows {
        let hidden = &sorted[limit..];
        let done = hidden.iter().filter(|t| t.status == "completed").count();
        lines.push(footer_line(hidden.len(), done));
    }
    lines
}

/// Header + (optional bar) + body shared by the widget and expanded forms.
fn render_todos_body(
    data: &InfoWidgetData,
    inner: Rect,
    pip_cap: usize,
    show_bar: bool,
    priority_marker: bool,
    item_cap: usize,
    body_rows: impl Fn(usize) -> usize,
) -> Vec<Line<'static>> {
    let counts = count_todos(&data.todos);
    let groups = grouped_todos(&data.todos);
    // In grouped mode each group header carries its own goal; in the flat list
    // the whole list is one implicit goal and its loop score lives on the header.
    let flat_goal = if groups.is_none() {
        goal_for_group(&data.todo_goals, None)
    } else {
        None
    };

    let mut lines = vec![Line::from(todos_header_spans(
        data,
        counts,
        inner.width,
        pip_cap,
        flat_goal,
    ))];
    if show_bar && inner.height >= 5 {
        lines.extend(progress_line(counts, inner.width, 4));
    }

    let rows = body_rows(lines.len());
    let body = match groups {
        Some(groups) => grouped_body(
            &groups,
            &data.todo_goals,
            inner,
            priority_marker,
            item_cap,
            rows,
        ),
        None => {
            let refs: Vec<&crate::todo::TodoItem> = data.todos.iter().collect();
            let opts = RowOpts {
                priority_marker,
                indent: 0,
                flag_unscored: any_scored(&data.todos),
            };
            flat_body(&sort_todos_by_status(&refs), inner, opts, item_cap, rows)
        }
    };
    lines.extend(body);
    lines
}

// ---------------------------------------------------------------------------
// Public renderers
// ---------------------------------------------------------------------------

/// Render todos widget content. An empty list renders nothing so the widget
/// collapses instead of holding a placeholder line.
///
/// Every row is budgeted against `inner.height`, including the progress bar
/// and the "+N more" footer, so the content never overflows its panel.
pub(super) fn render_todos_widget(data: &InfoWidgetData, inner: Rect) -> Vec<Line<'static>> {
    if data.todos.is_empty() {
        return Vec::new();
    }
    let height = usize::from(inner.height);
    render_todos_body(
        data,
        inner,
        WIDGET_PIP_CAP,
        true,
        false,
        MAX_WIDGET_ITEMS,
        |used| height.saturating_sub(used),
    )
}

/// Expanded form: priority markers and up to `MAX_TODO_LINES` lines in total
/// (header, items and footer included).
pub(super) fn render_todos_expanded(data: &InfoWidgetData, inner: Rect) -> Vec<Line<'static>> {
    if data.todos.is_empty() {
        return Vec::new();
    }
    render_todos_body(
        data,
        inner,
        EXPANDED_PIP_CAP,
        false,
        true,
        usize::MAX,
        |used| MAX_TODO_LINES.saturating_sub(used),
    )
}

/// Compact summary row, degraded to fit `width`: drop the loop score, then the
/// confidence, then the "total" figure, and finally fall back to glyphs.
fn compact_summary_spans(
    counts: TodoCounts,
    confidence: Option<u8>,
    goal: Option<&crate::todo::TodoGoal>,
    width: usize,
) -> Vec<Span<'static>> {
    let separator = || Span::styled(" · ", style_faint());
    // The state counts sum to the total, so the row reconciles on its own
    // instead of leaving the reader to infer how much is done.
    let build = |show_total: bool, show_confidence: bool, show_loop: bool| {
        let mut spans: Vec<Span<'static>> = Vec::new();
        if show_total {
            spans.push(Span::styled(
                format!("{} total", counts.total),
                style_muted(),
            ));
            spans.push(separator());
        }
        spans.push(Span::styled(
            format!("{} active", counts.in_progress),
            style_warning(),
        ));
        spans.push(separator());
        spans.push(Span::styled(format!("{} open", counts.open), style_muted()));
        spans.push(separator());
        spans.push(Span::styled(
            format!("{} done", counts.completed),
            style_success(),
        ));
        // Only surfaced when it happened: most lists never cancel anything, and
        // a permanent "0 cancelled" would just crowd the row.
        if counts.cancelled > 0 {
            spans.push(separator());
            spans.push(Span::styled(
                format!("{} cancelled", counts.cancelled),
                style_danger().add_modifier(Modifier::DIM),
            ));
        }
        if show_confidence {
            push_aggregate_confidence_suffix(&mut spans, confidence);
        }
        if show_loop && let Some(goal) = goal {
            push_goal_loop_suffix(&mut spans, goal);
        }
        spans
    };

    for (total, conf, loop_score) in [
        (true, true, true),
        (true, true, false),
        (true, false, false),
        (false, false, false),
    ] {
        let spans = build(total, conf, loop_score);
        if spans_cols(&spans) <= width {
            return spans;
        }
    }

    let mut spans = vec![
        Span::styled(format!("▶{}", counts.in_progress), style_warning()),
        Span::raw(" "),
        Span::styled(format!("○{}", counts.open), style_muted()),
        Span::raw(" "),
        Span::styled(format!("✓{}", counts.completed), style_success()),
    ];
    if counts.cancelled > 0 {
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("✗{}", counts.cancelled),
            style_danger().add_modifier(Modifier::DIM),
        ));
    }
    spans
}

pub(super) fn render_todos_compact(data: &InfoWidgetData, inner: Rect) -> Vec<Line<'static>> {
    // An empty list renders nothing, matching the expanded and widget forms.
    // The rail only shows this panel when there is work to report, so a
    // placeholder would be permanent noise in every other session.
    if data.todos.is_empty() {
        return Vec::new();
    }
    let counts = count_todos(&data.todos);
    let summary = compact_summary_spans(
        counts,
        aggregate_todo_confidence(&data.todos),
        goal_for_group(&data.todo_goals, None),
        usize::from(inner.width),
    );

    let mut lines = vec![
        Line::from(Span::styled(todos_widget_label(data), style_title())),
        Line::from(summary),
    ];
    // Inline progress bar when there is enough width (matches the widget bar).
    if let Some(bar) = progress_line(counts, inner.width.saturating_sub(2), 8) {
        lines.push(bar);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todo(status: &str) -> crate::todo::TodoItem {
        crate::todo::TodoItem {
            content: "work".to_string(),
            status: status.to_string(),
            priority: "medium".to_string(),
            id: status.to_string(),
            ..crate::todo::TodoItem::default()
        }
    }

    fn list(statuses: &[&str]) -> Vec<crate::todo::TodoItem> {
        statuses.iter().map(|status| todo(status)).collect()
    }

    fn data(statuses: &[&str]) -> InfoWidgetData {
        InfoWidgetData {
            todos: list(statuses),
            ..InfoWidgetData::default()
        }
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    fn cols(line: &Line<'_>) -> usize {
        unicode_width::UnicodeWidthStr::width(line_text(line).as_str())
    }

    fn pip_text(data: &InfoWidgetData, width_pips: usize) -> String {
        let mut spans = Vec::new();
        push_todo_pips(&mut spans, data, width_pips);
        spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect::<String>()
            .trim()
            .to_string()
    }

    fn rect(width: u16) -> Rect {
        Rect::new(0, 0, width, 24)
    }

    fn grouped(labels: &[(&str, &str)]) -> Vec<crate::todo::TodoItem> {
        labels
            .iter()
            .map(|(group, status)| crate::todo::TodoItem {
                group: Some((*group).to_string()),
                ..todo(status)
            })
            .collect()
    }

    /// The buckets partition the list. Any status the renderer does not know
    /// about is open work, not silently dropped.
    #[test]
    fn the_four_buckets_account_for_every_todo() {
        let counts = count_todos(&list(&[
            "completed",
            "in_progress",
            "pending",
            "cancelled",
            "blocked",
            "something-a-plan-invented",
        ]));
        assert_eq!(counts.total, 6);
        assert_eq!(counts.completed, 1);
        assert_eq!(counts.in_progress, 1);
        assert_eq!(counts.cancelled, 1);
        assert_eq!(counts.open, 3, "unknown statuses count as open work");
        assert_eq!(
            counts.completed + counts.in_progress + counts.cancelled + counts.open,
            counts.total
        );
    }

    /// The bug: `open` used to be `total - completed`, which counted the item
    /// the agent is working on right now as untouched.
    #[test]
    fn in_progress_work_is_not_also_counted_as_open() {
        let counts = count_todos(&list(&["in_progress", "pending"]));
        assert_eq!(counts.open, 1);
        assert_eq!(counts.in_progress, 1);
    }

    /// Cancelled work is not outstanding, so it leaves the denominator.
    #[test]
    fn cancelling_the_last_item_completes_the_counter() {
        let counts = count_todos(&list(&["completed", "completed", "cancelled"]));
        assert_eq!(
            (counts.completed, counts.actionable()),
            (2, 2),
            "2/2, not 2/3 forever"
        );
        assert_eq!(counts.progress(), Some(1.0));
    }

    #[test]
    fn an_empty_list_counts_to_zero() {
        let counts = count_todos(&[]);
        assert_eq!(counts, TodoCounts::default());
        assert_eq!(counts.actionable(), 0);
        assert_eq!(counts.progress(), None);
    }

    #[test]
    fn a_small_list_gets_one_pip_per_todo() {
        let pips = pip_text(&data(&["completed", "in_progress", "pending"]), 10);
        assert_eq!(pips, "●●○", "done, then active, then open");
    }

    #[test]
    fn a_cancelled_todo_is_visible_but_de_emphasised() {
        let pips = pip_text(&data(&["completed", "cancelled"]), 10);
        assert_eq!(pips, "●·");
    }

    /// A single active item in a large list is exactly what the user is
    /// watching; proportional rounding alone would round it away to nothing.
    #[test]
    fn collapsing_never_hides_a_state_that_has_work_in_it() {
        let mut statuses = vec!["completed"; 199];
        statuses.push("in_progress");
        let pips = pip_text(&data(&statuses), 8);
        assert!(
            pips.contains('●'),
            "the lone in-progress item must still get a pip: {pips}"
        );
        assert_eq!(pips.chars().count(), 8, "and the budget is still respected");
    }

    #[test]
    fn collapsing_respects_the_pip_budget() {
        for width in 1..=16usize {
            let statuses = ["completed", "in_progress", "pending", "cancelled"].repeat(30);
            let pips = pip_text(&data(&statuses), width);
            assert!(
                pips.chars().count() <= width,
                "width {width} overflowed to {} pips",
                pips.chars().count()
            );
        }
    }

    /// The budget is hard: a list slightly larger than the room collapses
    /// rather than spilling past the panel edge.
    #[test]
    fn a_list_just_over_budget_collapses_instead_of_overflowing() {
        let pips = pip_text(&data(&["pending"; 11]), 6);
        assert!(pips.chars().count() <= 6, "{pips}");
        assert!(!pips.is_empty());
    }

    #[test]
    fn a_zero_width_meter_renders_nothing() {
        assert_eq!(pip_text(&data(&["pending"]), 0), "");
        assert_eq!(pip_text(&data(&[]), 10), "");
        let mut spans = Vec::new();
        push_todo_pips(&mut spans, &data(&["pending"]), 0);
        assert!(spans.is_empty(), "no stray spacer when no pips are drawn");
    }

    /// A header on the last available line describes nothing. Spending the row
    /// on the "+N more" footer tells the reader strictly more.
    #[test]
    fn a_group_header_is_never_rendered_without_an_item_under_it() {
        let todos = grouped(&[("alpha", "pending"), ("beta", "pending")]);
        let groups = grouped_todos(&todos).expect("todos declare groups");
        for max_lines in 1..=4usize {
            let (lines, shown, _) =
                render_grouped_todo_lines(&groups, &[], rect(60), false, max_lines);
            assert!(lines.len() <= max_lines, "budget {max_lines} overflowed");
            if !lines.is_empty() {
                assert!(
                    shown > 0,
                    "budget {max_lines} rendered {} header-only line(s)",
                    lines.len()
                );
            }
        }
    }

    #[test]
    fn groups_render_their_items_when_the_budget_allows() {
        let todos = grouped(&[("alpha", "pending"), ("alpha", "completed")]);
        let groups = grouped_todos(&todos).expect("todos declare groups");
        let (lines, shown, hidden_done) =
            render_grouped_todo_lines(&groups, &[], rect(60), false, 8);
        assert_eq!(shown, 2);
        assert_eq!(hidden_done, 0);
        assert_eq!(lines.len(), 3, "one header plus two items");
    }

    #[test]
    fn hidden_completed_items_are_reported_per_group() {
        let todos = grouped(&[
            ("alpha", "pending"),
            ("alpha", "completed"),
            ("beta", "completed"),
        ]);
        let groups = grouped_todos(&todos).expect("todos declare groups");
        // Budget fits alpha's header + its pending item only.
        let (_, shown, hidden_done) = render_grouped_todo_lines(&groups, &[], rect(60), false, 2);
        assert_eq!(shown, 1);
        assert_eq!(hidden_done, 2);
    }

    /// The ungrouped bucket sorts last so named goals lead, and named groups
    /// keep the order they first appear in.
    #[test]
    fn ungrouped_todos_collapse_into_a_trailing_bucket() {
        let mut todos = grouped(&[("beta", "pending"), ("alpha", "pending")]);
        todos.push(todo("pending"));
        let groups = grouped_todos(&todos).expect("some todos declare groups");
        let keys: Vec<Option<&str>> = groups.iter().map(|(key, _)| key.as_deref()).collect();
        assert_eq!(keys, vec![Some("beta"), Some("alpha"), None]);
    }

    #[test]
    fn a_flat_list_has_no_groups() {
        assert!(grouped_todos(&list(&["pending", "completed"])).is_none());
    }

    /// Blank and whitespace-only labels are not groups; treating them as one
    /// would render an empty header.
    #[test]
    fn a_blank_group_label_is_not_a_group() {
        let todos = vec![crate::todo::TodoItem {
            group: Some("   ".to_string()),
            ..todo("pending")
        }];
        assert!(grouped_todos(&todos).is_none());
    }

    #[test]
    fn active_work_sorts_ahead_of_everything_else() {
        let todos = list(&["cancelled", "completed", "pending", "in_progress"]);
        let refs: Vec<&crate::todo::TodoItem> = todos.iter().collect();
        let order: Vec<&str> = sort_todos_by_status(&refs)
            .iter()
            .map(|todo| todo.status.as_str())
            .collect();
        assert_eq!(
            order,
            vec!["in_progress", "pending", "completed", "cancelled"]
        );
    }

    /// Runnable work is what a reader can act on, so blocked items of the same
    /// status wait behind it.
    #[test]
    fn blocked_items_sort_behind_runnable_ones() {
        let blocked = crate::todo::TodoItem {
            id: "blocked".to_string(),
            blocked_by: vec!["x".to_string()],
            ..todo("pending")
        };
        let free = crate::todo::TodoItem {
            id: "free".to_string(),
            ..todo("pending")
        };
        let refs = vec![&blocked, &free];
        let order: Vec<&str> = sort_todos_by_status(&refs)
            .iter()
            .map(|t| t.id.as_str())
            .collect();
        assert_eq!(order, vec!["free", "blocked"]);
    }

    /// The scheduler's vocabulary is wider than the renderer's. Anything that
    /// is actually running must land on `in_progress`, or live plan activity
    /// sorts below finished work and disappears behind "+N more".
    #[test]
    fn running_plan_items_normalize_to_active_todos() {
        for status in ["running", "running_stale"] {
            assert_eq!(normalize_plan_status_for_todo(status), "in_progress");
        }
        assert_eq!(normalize_plan_status_for_todo("done"), "completed");
        for status in ["failed", "stopped", "crashed"] {
            assert_eq!(normalize_plan_status_for_todo(status), "cancelled");
        }
        for status in ["queued", "ready", "todo", "blocked"] {
            assert_eq!(normalize_plan_status_for_todo(status), "pending");
        }
    }

    #[test]
    fn plan_status_matching_ignores_case_and_whitespace() {
        assert_eq!(normalize_plan_status_for_todo(" Running "), "in_progress");
        assert_eq!(normalize_plan_status_for_todo("DONE"), "completed");
        assert_eq!(normalize_plan_status_for_todo("whatever"), "whatever");
    }

    /// Every status the plan projection emits must be one the renderer scores,
    /// otherwise plan items fall into the catch-all rank and sort unpredictably.
    #[test]
    fn normalized_plan_statuses_are_all_known_to_the_sorter() {
        for status in [
            "running",
            "running_stale",
            "done",
            "failed",
            "stopped",
            "crashed",
            "queued",
            "ready",
            "todo",
            "blocked",
        ] {
            let normalized = normalize_plan_status_for_todo(status);
            assert!(
                status_sort_rank(&normalized) < 4,
                "`{status}` normalizes to `{normalized}`, which the sorter does not rank"
            );
        }
    }

    #[test]
    fn confidence_is_weighted_by_priority() {
        let high = crate::todo::TodoItem {
            priority: "high".to_string(),
            confidence: Some(90),
            ..todo("pending")
        };
        let low = crate::todo::TodoItem {
            priority: "low".to_string(),
            confidence: Some(30),
            ..todo("pending")
        };
        // (90*3 + 30*1) / 4 = 75, not the unweighted mean of 60.
        assert_eq!(aggregate_todo_confidence([&high, &low]), Some(75));
    }

    /// A cancelled item's confidence says nothing about the work that remains.
    #[test]
    fn cancelled_todos_do_not_drag_the_confidence_score() {
        let live = crate::todo::TodoItem {
            confidence: Some(80),
            ..todo("pending")
        };
        let dead = crate::todo::TodoItem {
            confidence: Some(0),
            ..todo("cancelled")
        };
        assert_eq!(aggregate_todo_confidence([&live, &dead]), Some(80));
    }

    #[test]
    fn a_list_with_no_scores_has_no_aggregate() {
        assert_eq!(aggregate_todo_confidence(&list(&["pending"])), None);
        assert_eq!(aggregate_todo_confidence(&[]), None);
    }

    /// Completion confidence supersedes the planning-time estimate once the
    /// item is done, but only for completed items.
    #[test]
    fn a_finished_todo_reports_the_confidence_it_finished_with() {
        let done = crate::todo::TodoItem {
            confidence: Some(40),
            completion_confidence: Some(95),
            ..todo("completed")
        };
        assert_eq!(todo_display_confidence(&done), Some(95));

        let open = crate::todo::TodoItem {
            confidence: Some(40),
            completion_confidence: Some(95),
            ..todo("pending")
        };
        assert_eq!(todo_display_confidence(&open), Some(40));
    }

    /// Every suffix reserves width for truncation. If the reservation is short,
    /// the row overflows the panel and ratatui clips the score.
    #[test]
    fn suffix_widths_match_what_gets_rendered() {
        let scored = crate::todo::TodoItem {
            confidence: Some(7),
            ..todo("pending")
        };
        for item in [&scored, &todo("pending")] {
            let mut spans = Vec::new();
            push_todo_confidence_suffix(&mut spans, item);
            assert_eq!(
                spans_width(&spans),
                todo_confidence_suffix_width(item) as usize
            );
        }
    }

    #[test]
    fn goal_suffix_width_matches_what_gets_rendered() {
        let goal = crate::todo::TodoGoal {
            closed_feedback_loop: Some(100),
            ..crate::todo::TodoGoal::default()
        };
        let mut spans = Vec::new();
        push_goal_loop_suffix(&mut spans, &goal);
        assert_eq!(spans_width(&spans), goal_loop_suffix_width(&goal) as usize);

        let unscored = crate::todo::TodoGoal::default();
        let mut spans = Vec::new();
        push_goal_loop_suffix(&mut spans, &unscored);
        assert!(spans.is_empty());
        assert_eq!(goal_loop_suffix_width(&unscored), 0);
    }

    #[test]
    fn aggregate_confidence_suffix_width_matches_what_gets_rendered() {
        let mut spans = Vec::new();
        push_aggregate_confidence_suffix(&mut spans, Some(42));
        assert_eq!(
            spans_width(&spans),
            aggregate_confidence_suffix_width(Some(42)) as usize
        );

        let mut spans = Vec::new();
        push_aggregate_confidence_suffix(&mut spans, None);
        assert!(spans.is_empty());
        assert_eq!(aggregate_confidence_suffix_width(None), 0);
    }

    /// A goal is matched by trimmed label, matching how the todo tool
    /// normalizes groups; the flat list is the `None` goal.
    #[test]
    fn goals_match_their_group_after_trimming() {
        let goals = vec![
            crate::todo::TodoGoal {
                group: Some("  alpha  ".to_string()),
                closed_feedback_loop: Some(60),
                ..crate::todo::TodoGoal::default()
            },
            crate::todo::TodoGoal {
                group: None,
                closed_feedback_loop: Some(10),
                ..crate::todo::TodoGoal::default()
            },
        ];
        assert_eq!(
            goal_for_group(&goals, Some("alpha")).and_then(|g| g.closed_feedback_loop),
            Some(60)
        );
        assert_eq!(
            goal_for_group(&goals, None).and_then(|g| g.closed_feedback_loop),
            Some(10)
        );
        // An empty label is the ungrouped list, not a group named "".
        assert_eq!(
            goal_for_group(&goals, Some("   ")).and_then(|g| g.closed_feedback_loop),
            Some(10)
        );
        assert!(goal_for_group(&goals, Some("beta")).is_none());
    }

    /// Rows must fit the panel at every width. Optional parts are dropped
    /// before the title is squeezed, so the only floor is the fixed prefix
    /// (indent + icon).
    #[test]
    fn todo_rows_fit_the_panel_at_every_width() {
        let item = crate::todo::TodoItem {
            content: "a very long todo title that will certainly need truncating".to_string(),
            priority: "high".to_string(),
            confidence: Some(88),
            blocked_by: vec!["other".to_string()],
            ..todo("pending")
        };
        let opts = RowOpts {
            priority_marker: true,
            indent: 2,
            flag_unscored: true,
        };
        for width in 4..=80u16 {
            let mut lines = Vec::new();
            push_todo_item_line(&mut lines, &item, rect(width), opts);
            assert!(
                cols(&lines[0]) <= width as usize,
                "width {width}: row rendered {} columns",
                cols(&lines[0])
            );
        }
    }

    /// Decoration is shed before the title is squeezed.
    #[test]
    fn a_narrow_row_drops_decoration_before_the_title() {
        let item = crate::todo::TodoItem {
            content: "implement the thing".to_string(),
            priority: "high".to_string(),
            confidence: Some(88),
            blocked_by: vec!["other".to_string()],
            ..todo("pending")
        };
        let opts = RowOpts {
            priority_marker: true,
            indent: 0,
            flag_unscored: true,
        };
        let mut wide = Vec::new();
        push_todo_item_line(&mut wide, &item, rect(60), opts);
        let wide = line_text(&wide[0]);
        assert!(wide.contains("(blocked)") && wide.contains("88%") && wide.contains('‼'));

        let mut narrow = Vec::new();
        push_todo_item_line(&mut narrow, &item, rect(24), opts);
        let narrow = line_text(&narrow[0]);
        assert!(!narrow.contains("(blocked)"), "{narrow}");
        assert!(narrow.contains("implement"), "title survives: {narrow}");
    }

    /// A blocked item that has been cancelled is cancelled, not blocked.
    #[test]
    fn a_cancelled_item_is_not_shown_as_blocked() {
        let item = crate::todo::TodoItem {
            blocked_by: vec!["other".to_string()],
            ..todo("cancelled")
        };
        let mut lines = Vec::new();
        push_todo_item_line(
            &mut lines,
            &item,
            rect(60),
            RowOpts {
                priority_marker: false,
                indent: 0,
                flag_unscored: false,
            },
        );
        let text = line_text(&lines[0]);
        assert!(text.starts_with('✗'), "{text}");
        assert!(!text.contains("blocked"), "{text}");
    }

    /// `?%` on every row of a list nobody has scored is noise; once some rows
    /// carry a score the unscored ones are worth flagging.
    #[test]
    fn unknown_confidence_is_only_flagged_when_peers_have_scores() {
        let item = todo("pending");
        let render = |flag: bool| {
            let mut lines = Vec::new();
            push_todo_item_line(
                &mut lines,
                &item,
                rect(60),
                RowOpts {
                    priority_marker: false,
                    indent: 0,
                    flag_unscored: flag,
                },
            );
            line_text(&lines[0])
        };
        assert!(!render(false).contains("?%"));
        assert!(render(true).contains("?%"));
    }

    #[test]
    fn the_widget_label_distinguishes_a_shared_plan_from_a_private_list() {
        let mut data = data(&["pending"]);
        assert_eq!(todos_widget_label(&data), "Todos");
        data.todos_are_swarm_plan = true;
        assert_eq!(todos_widget_label(&data), "Plan");
    }

    /// The compact row is the only surface that shows all four states at once,
    /// so it is where a miscount is most visible.
    #[test]
    fn the_compact_summary_reconciles_with_the_total() {
        let lines = render_todos_compact(&data(&["completed", "in_progress", "pending"]), rect(80));
        let text = line_text(&lines[1]);
        assert!(text.contains("3 total"), "{text}");
        assert!(text.contains("1 active"), "{text}");
        assert!(text.contains("1 open"), "{text}");
        assert!(text.contains("1 done"), "{text}");
        assert!(!text.contains("cancelled"), "nothing was cancelled: {text}");
    }

    #[test]
    fn the_compact_summary_reports_cancelled_work_when_there_is_any() {
        let lines = render_todos_compact(&data(&["completed", "cancelled"]), rect(80));
        let text = line_text(&lines[1]);
        assert!(text.contains("1 cancelled"), "{text}");
    }

    /// The summary degrades to fit instead of running off a narrow rail.
    #[test]
    fn the_compact_summary_fits_narrow_rails() {
        let data = data(&["completed", "in_progress", "pending", "cancelled"]);
        for width in 12..=60u16 {
            let lines = render_todos_compact(&data, rect(width));
            assert!(
                cols(&lines[1]) <= width as usize,
                "width {width}: summary rendered {} columns: {}",
                cols(&lines[1]),
                line_text(&lines[1])
            );
        }
    }

    #[test]
    fn an_empty_list_renders_nothing_at_all() {
        let empty = data(&[]);
        assert!(render_todos_compact(&empty, rect(80)).is_empty());
        assert!(render_todos_widget(&empty, rect(80)).is_empty());
        assert!(render_todos_expanded(&empty, rect(80)).is_empty());
    }

    /// The header fraction is the same one `count_todos` reports, and it stays
    /// within its own denominator.
    #[test]
    fn the_header_counter_never_exceeds_its_denominator() {
        for statuses in [
            vec!["completed", "cancelled"],
            vec!["in_progress"],
            vec!["completed", "completed", "pending"],
            vec!["cancelled", "cancelled"],
        ] {
            let counts = count_todos(&list(&statuses));
            assert!(
                counts.completed <= counts.actionable(),
                "{statuses:?} reported {}/{}",
                counts.completed,
                counts.actionable()
            );
        }
    }

    /// Group headers use the same denominator as the widget header.
    #[test]
    fn a_group_header_counter_excludes_cancelled_work() {
        let todos = grouped(&[("alpha", "completed"), ("alpha", "cancelled")]);
        let refs: Vec<&crate::todo::TodoItem> = todos.iter().collect();
        let mut lines = Vec::new();
        push_group_header(&mut lines, "alpha", &refs, None, rect(60));
        assert!(
            line_text(&lines[0]).contains("1/1"),
            "{}",
            line_text(&lines[0])
        );
    }

    /// The header must fit the panel once there is room for its fixed part,
    /// with every optional piece in play.
    #[test]
    fn the_header_fits_its_width() {
        let mut data = data(&[
            "completed",
            "in_progress",
            "pending",
            "pending",
            "cancelled",
            "pending",
            "pending",
            "pending",
            "pending",
            "pending",
            "pending",
            "pending",
            "pending",
        ]);
        data.todos[0].confidence = Some(85);
        let goal = crate::todo::TodoGoal {
            closed_feedback_loop: Some(70),
            ..crate::todo::TodoGoal::default()
        };
        let counts = count_todos(&data.todos);
        for width in 12..=90u16 {
            let line = Line::from(todos_header_spans(&data, counts, width, 16, Some(&goal)));
            assert!(
                cols(&line) <= width as usize,
                "width {width}: header rendered {} columns: {}",
                cols(&line),
                line_text(&line)
            );
        }
    }

    #[test]
    fn the_progress_bar_is_honest_about_progress() {
        let text = |p: f64, w: usize| -> String {
            progress_bar_spans(p, w)
                .iter()
                .map(|s| s.content.as_ref())
                .collect()
        };
        assert_eq!(text(0.0, 4), "░░░░");
        assert_eq!(text(1.0, 4), "████");
        assert_eq!(text(0.5, 4), "██░░");
        assert!(
            text(0.999, 10).contains('░')
                || text(0.999, 10).chars().any(|c| PARTIAL_BLOCKS.contains(&c))
        );
        assert!(
            text(0.001, 10).starts_with('▏'),
            "a sliver of progress is visible"
        );
        assert_eq!(text(f64::NAN, 4), "░░░░");
        assert_eq!(text(0.5, 0), "");
    }

    #[test]
    fn the_progress_bar_fills_exactly_its_width() {
        for width in 1..=40usize {
            for step in 0..=20 {
                let spans = progress_bar_spans(step as f64 / 20.0, width);
                let text: String = spans.iter().map(|s| s.content.as_ref()).collect();
                assert_eq!(text.chars().count(), width, "width {width} step {step}");
            }
        }
    }

    /// Every rendering path must respect the panel height it is given,
    /// including the progress bar and the "+N more" footer.
    #[test]
    fn the_widget_stays_within_its_panel_height() {
        let grouped_data = InfoWidgetData {
            todos: grouped(&[
                ("alpha", "pending"),
                ("alpha", "in_progress"),
                ("beta", "completed"),
                ("beta", "pending"),
                ("gamma", "pending"),
            ]),
            ..InfoWidgetData::default()
        };
        let flat_data = data(&["pending"; 12]);
        for data in [&grouped_data, &flat_data] {
            for height in 2..=14u16 {
                let lines = render_todos_widget(data, Rect::new(0, 0, 60, height));
                assert!(
                    lines.len() <= height as usize,
                    "height {height}: rendered {} lines",
                    lines.len()
                );
            }
        }
    }

    /// An overflowing list reports what it hid, and the footer is the last row.
    #[test]
    fn an_overflowing_list_ends_with_a_footer_that_fits() {
        let lines = render_todos_widget(&data(&["pending"; 12]), Rect::new(0, 0, 60, 9));
        assert!(lines.len() <= 9);
        let last = line_text(lines.last().expect("rendered something"));
        assert!(last.contains("more"), "{last}");
    }

    #[test]
    fn the_expanded_form_stays_within_the_line_budget() {
        for data in [data(&["pending"; 30]), data(&["pending"; 3])] {
            let lines = render_todos_expanded(&data, rect(60));
            assert!(
                lines.len() <= MAX_TODO_LINES,
                "rendered {} lines against a budget of {MAX_TODO_LINES}",
                lines.len()
            );
        }
    }
}
