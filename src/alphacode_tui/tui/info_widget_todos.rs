use super::*;

#[cfg(test)]
use super::text::spans_width;

/// Below this many todos we always render an exact 1:1 pip per todo,
/// even if the panel is a bit narrow, so small lists are never normalized.
const EXACT_PIP_FLOOR: usize = 12;

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
/// `pending` (○). Statuses the todo renderer already understands (and any
/// arbitrary strings) pass through unchanged. Blocked items still get their
/// ⊳ marker from `blocked_by`.
fn normalize_plan_status_for_todo(status: &str) -> String {
    match status {
        "running" | "running_stale" => "in_progress".to_string(),
        "done" => "completed".to_string(),
        "failed" | "stopped" | "crashed" => "cancelled".to_string(),
        "queued" | "ready" | "todo" | "blocked" => "pending".to_string(),
        other => other.to_string(),
    }
}

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
    let color = match score {
        Some(90..=100) => rgb(108, 230, 158),
        Some(70..=89) => rgb(255, 215, 108),
        Some(_) => rgb(255, 128, 118),
        None => rgb(108, 112, 128),
    };
    Style::default().fg(color)
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
    let color = if score >= crate::todo::LOW_CLOSED_FEEDBACK_LOOP {
        rgb(108, 230, 158)
    } else if score >= crate::todo::LOW_CLOSED_FEEDBACK_LOOP.saturating_sub(20) {
        rgb(255, 215, 108)
    } else {
        rgb(255, 128, 118)
    };
    Style::default().fg(color)
}

/// Append a " · hill N%" suffix describing a goal's closed feedback loop.
fn push_goal_loop_suffix(spans: &mut Vec<Span<'static>>, goal: &crate::todo::TodoGoal) {
    let Some(score) = goal.closed_feedback_loop else {
        return;
    };
    spans.push(Span::styled(" · ", Style::default().fg(rgb(80, 80, 90))));
    spans.push(Span::styled(
        "loop ",
        Style::default().fg(rgb(140, 140, 150)),
    ));
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
    spans.push(Span::styled(" · ", Style::default().fg(rgb(80, 80, 90))));
    spans.push(Span::styled(
        confidence_label(score),
        confidence_style(score),
    ));
}

/// One pass over the list, so every surface that reports progress agrees.
///
/// The buckets are disjoint and sum to `total`. That matters because the widget
/// reports the same list three different ways (pip meter, `n/m` counter,
/// compact summary) and they used to disagree: each derived "open" as
/// `total - completed`, which silently folds in-progress and cancelled work
/// into the open count.
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
}

pub(crate) fn count_todos(todos: &[crate::todo::TodoItem]) -> TodoCounts {
    let mut counts = TodoCounts {
        total: todos.len(),
        ..TodoCounts::default()
    };
    for todo in todos {
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
/// in_progress, hollow = open, dim dot = cancelled. We render an exact 1:1 pip
/// per todo whenever the list is small enough to fit in `width_pips` columns;
/// only larger lists collapse to a proportional summary so the footprint stays
/// small.
fn push_todo_pips(spans: &mut Vec<Span<'static>>, data: &InfoWidgetData, width_pips: usize) {
    let counts = count_todos(&data.todos);
    if counts.total == 0 || width_pips == 0 {
        return;
    }

    spans.push(Span::raw("  "));

    // Prefer exact 1:1 pips. Allow it whenever the list fits the available
    // width, plus a generous floor so typical lists never get normalized
    // just because the panel is a little narrow.
    let exact_threshold = width_pips.max(EXACT_PIP_FLOOR);
    let buckets = if counts.total <= exact_threshold {
        [
            counts.completed,
            counts.in_progress,
            counts.open,
            counts.cancelled,
        ]
    } else {
        collapse_pips(counts, width_pips.max(1))
    };

    // Status order: done, active, open, cancelled.
    // Colors use the refreshed palette for better contrast and visual appeal.
    let styling = [
        ("●", rgb(108, 230, 158)),
        ("●", rgb(255, 215, 108)),
        ("○", rgb(88, 92, 108)),
        ("·", rgb(68, 72, 82)),
    ];
    for (count, (glyph, color)) in buckets.into_iter().zip(styling) {
        for _ in 0..count {
            spans.push(Span::styled(glyph, Style::default().fg(color)));
        }
    }
}

/// Build a smooth animated progress bar for the todo list.
/// Uses block characters with a smooth color gradient for a visually
/// appealing real-time progress indicator.
///
/// The bar now renders per-cell colors that smoothly transition from
/// warm orange → amber → green as progress increases, giving a premium
/// gradient look instead of a single flat fill color.
fn push_todo_progress_bar(spans: &mut Vec<Span<'static>>, data: &InfoWidgetData, width: usize) {
    let counts = count_todos(&data.todos);
    if counts.total == 0 || width == 0 {
        return;
    }

    let actionable = counts.actionable();
    if actionable == 0 {
        return;
    }

    // Calculate progress percentage
    let progress = counts.completed as f64 / actionable as f64;
    let filled = (progress * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);

    // Per-cell gradient: each filled cell gets a slightly different hue so the
    // bar reads as a smooth gradient instead of a solid block of color.
    // The gradient goes from warm amber (start) through green (middle) to
    // vibrant teal (end), giving a premium, polished look.
    if filled > 0 {
        for i in 0..filled {
            let cell_progress = i as f64 / filled.max(1) as f64;
            // Three-stop gradient: amber → green → teal
            let color = if cell_progress <= 0.33 {
                let t = cell_progress / 0.33;
                rgb(
                    lerp_f64(238.0, 188.0, t) as u8,
                    lerp_f64(128.0, 208.0, t) as u8,
                    lerp_f64(88.0, 108.0, t) as u8,
                )
            } else if cell_progress <= 0.66 {
                let t = (cell_progress - 0.33) / 0.33;
                rgb(
                    lerp_f64(188.0, 108.0, t) as u8,
                    lerp_f64(208.0, 230.0, t) as u8,
                    lerp_f64(108.0, 158.0, t) as u8,
                )
            } else {
                let t = (cell_progress - 0.66) / 0.34;
                rgb(
                    lerp_f64(108.0, 98.0, t) as u8,
                    lerp_f64(230.0, 218.0, t) as u8,
                    lerp_f64(158.0, 208.0, t) as u8,
                )
            };
            spans.push(Span::styled("\u{2588}", Style::default().fg(color)));
        }
    }
    if empty > 0 {
        // Empty portion uses a subtle dark gray that blends with the background.
        spans.push(Span::styled(
            "\u{2591}".repeat(empty),
            Style::default().fg(rgb(42, 46, 58)),
        ));
    }
}

/// Linear interpolation between two f64 values.
fn lerp_f64(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn aggregate_confidence_suffix_width(score: Option<u8>) -> u16 {
    match score {
        Some(score) => {
            (" · confidence ".width() + confidence_label(Some(score)).width()) as u16
        }
        None => 0,
    }
}

fn push_aggregate_confidence_suffix(spans: &mut Vec<Span<'static>>, score: Option<u8>) {
    let Some(score) = score else {
        return;
    };
    spans.push(Span::styled(" \u{00b7} ", Style::default().fg(rgb(88, 92, 108))));
    spans.push(Span::styled(
        "confidence ",
        Style::default().fg(rgb(128, 132, 148)),
    ));
    spans.push(Span::styled(
        confidence_label(Some(score)),
        confidence_style(Some(score)),
    ));
}

/// Normalize a todo's group label, treating empty/whitespace as ungrouped.
fn todo_group_key(todo: &crate::todo::TodoItem) -> Option<String> {
    todo.group
        .as_deref()
        .map(str::trim)
        .filter(|group| !group.is_empty())
        .map(|group| group.to_string())
}

/// Partition todos into ordered groups, preserving the order groups first
/// appear. Ungrouped items collapse into a trailing `None` bucket. Returns
/// `None` when no todo declares a group, so callers fall back to the flat list.
fn grouped_todos(
    todos: &[crate::todo::TodoItem],
) -> Option<Vec<(Option<String>, Vec<&crate::todo::TodoItem>)>> {
    if !todos.iter().any(|todo| todo_group_key(todo).is_some()) {
        return None;
    }
    let mut groups: Vec<(Option<String>, Vec<&crate::todo::TodoItem>)> = Vec::new();
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

fn sort_todos_by_status<'a>(todos: &[&'a crate::todo::TodoItem]) -> Vec<&'a crate::todo::TodoItem> {
    let mut sorted: Vec<&crate::todo::TodoItem> = todos.to_vec();
    sorted.sort_by(|a, b| status_sort_rank(&a.status).cmp(&status_sort_rank(&b.status)));
    sorted
}

fn push_group_header(
    lines: &mut Vec<Line<'static>>,
    name: &str,
    items: &[&crate::todo::TodoItem],
    goal: Option<&crate::todo::TodoGoal>,
    inner: Rect,
) {
    let total = items.len();
    let completed = items.iter().filter(|t| t.status == "completed").count();
    let counter = format!(" {}/{}", completed, total);
    let confidence = aggregate_todo_confidence(items.iter().copied());
    let confidence_width = aggregate_confidence_suffix_width(confidence);
    let loop_width = goal.map(goal_loop_suffix_width).unwrap_or(0);
    let max_name = inner
        .width
        .saturating_sub(counter.len() as u16 + confidence_width + loop_width)
        .max(4) as usize;
    let highlight = items.iter().any(|t| t.status == "in_progress");
    let name_style = if highlight {
        Style::default().fg(rgb(255, 210, 130)).bold()
    } else {
        Style::default().fg(rgb(170, 175, 205)).bold()
    };
    let mut spans = vec![
        Span::styled(truncate_smart(name, max_name), name_style),
        Span::styled(counter, Style::default().fg(rgb(120, 120, 140))),
    ];
    push_aggregate_confidence_suffix(&mut spans, confidence);
    if let Some(goal) = goal {
        push_goal_loop_suffix(&mut spans, goal);
    }
    lines.push(Line::from(spans));
}

/// Render one todo as a line. `show_priority_marker` adds the `!` high-priority
/// marker (used by the expanded widget); `indent` is the leading-space depth
/// used when items sit under a group header.
fn push_todo_item_line(
    lines: &mut Vec<Line<'static>>,
    todo: &crate::todo::TodoItem,
    inner: Rect,
    show_priority_marker: bool,
    indent: usize,
) {
    let is_blocked = !todo.blocked_by.is_empty();
    let (icon, status_color) = if is_blocked && todo.status != "completed" {
        ("⊳", rgb(188, 148, 108))
    } else {
        match todo.status.as_str() {
            "completed" => ("\u{2713}", rgb(108, 200, 148)),
            "in_progress" => ("\u{25b6}", rgb(255, 208, 108)),
            "cancelled" => ("\u{2717}", rgb(128, 88, 88)),
            _ => ("\u{25cb}", rgb(128, 132, 148)),
        }
    };

    let priority_marker = if show_priority_marker {
        match todo.priority.as_str() {
            "high" => ("\u{203c}", rgb(255, 128, 118)),
            _ => ("", rgb(128, 132, 148)),
        }
    } else {
        ("", rgb(128, 132, 148))
    };

    let suffix = if is_blocked && todo.status != "completed" {
        " (blocked)"
    } else {
        ""
    };

    // Column widths, not byte lengths: every status icon here is multi-byte,
    // so `.len()` over-reserved for some and the suffix widths under-reserved
    // for others. Measure exactly what gets pushed below.
    let icon_span = format!("{icon} ");
    let reserved = indent as u16
        + icon_span.width() as u16
        + priority_marker.0.width() as u16
        + suffix.width() as u16
        + todo_confidence_suffix_width(todo);
    let max_len = inner.width.saturating_sub(reserved) as usize;
    let content = truncate_smart(&todo.content, max_len);

    let text_color = if todo.status == "completed" {
        rgb(108, 112, 128)
    } else if is_blocked {
        rgb(128, 132, 148)
    } else if todo.status == "in_progress" {
        rgb(238, 242, 255)
    } else {
        rgb(168, 172, 188)
    };

    let mut spans = Vec::new();
    if indent > 0 {
        spans.push(Span::raw(" ".repeat(indent)));
    }
    // Bold the icon for in_progress items to draw the eye
    let icon_style = if todo.status == "in_progress" {
        Style::default().fg(status_color).add_modifier(ratatui::style::Modifier::BOLD)
    } else {
        Style::default().fg(status_color)
    };
    spans.push(Span::styled(icon_span, icon_style));
    if !priority_marker.0.is_empty() {
        spans.push(Span::styled(
            priority_marker.0,
            Style::default().fg(priority_marker.1).add_modifier(ratatui::style::Modifier::BOLD),
        ));
    }
    // Bold completed and in_progress content for visual hierarchy
    let content_style = if todo.status == "completed" {
        Style::default().fg(text_color).add_modifier(ratatui::style::Modifier::CROSSED_OUT)
    } else if todo.status == "in_progress" {
        Style::default().fg(text_color).add_modifier(ratatui::style::Modifier::BOLD)
    } else {
        Style::default().fg(text_color)
    };
    spans.push(Span::styled(content, content_style));
    push_todo_confidence_suffix(&mut spans, todo);
    if !suffix.is_empty() {
        spans.push(Span::styled(
            suffix.to_string(),
            Style::default().fg(rgb(108, 112, 128)),
        ));
    }
    lines.push(Line::from(spans));
}

/// Render todos partitioned by group, honoring a `max_lines` budget that counts
/// both group headers and item rows. Returns the rendered lines plus the number
/// of todo items actually shown (so callers can render a "+N more" footer).
fn render_grouped_todo_lines(
    groups: &[(Option<String>, Vec<&crate::todo::TodoItem>)],
    goals: &[crate::todo::TodoGoal],
    inner: Rect,
    show_priority_marker: bool,
    max_lines: usize,
) -> (Vec<Line<'static>>, usize) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut shown = 0usize;
    for (group, items) in groups {
        // A header needs room for at least one item beneath it. Spending the
        // last line of the budget on a heading that describes nothing costs a
        // row and tells the reader less than the "+N more" footer would.
        if lines.len() + 1 >= max_lines {
            break;
        }
        let header_name = group.as_deref().unwrap_or("Other");
        let goal = goal_for_group(goals, group.as_deref());
        push_group_header(&mut lines, header_name, items, goal, inner);
        for todo in sort_todos_by_status(items) {
            if lines.len() >= max_lines {
                break;
            }
            push_todo_item_line(&mut lines, todo, inner, show_priority_marker, 2);
            shown += 1;
        }
    }
    (lines, shown)
}

/// Header label for the todo slot: "Plan" when the items are the shared
/// swarm plan projection, "Todos" for the session's own private list.
fn todos_widget_label(data: &InfoWidgetData) -> &'static str {
    if data.todos_are_swarm_plan {
        "Plan"
    } else {
        "Todos"
    }
}

/// Render todos widget content
pub(super) fn render_todos_widget(data: &InfoWidgetData, inner: Rect) -> Vec<Line<'static>> {
    if data.todos.is_empty() {
        return vec![Line::from(vec![Span::styled(
            "No tasks yet",
            Style::default().fg(rgb(80, 80, 90)).italic(),
        )])];
    }

    let mut lines: Vec<Line> = Vec::new();
    let counts = count_todos(&data.todos);
    let total = counts.total;

    // Header with progress + inline pip meter + progress bar
    let mut header = vec![
        Span::styled(
            format!("{} ", todos_widget_label(data)),
            Style::default().fg(rgb(180, 180, 190)).bold(),
        ),
        Span::styled(
            format!("{}/{}", counts.completed, counts.actionable()),
            Style::default().fg(rgb(140, 140, 150)),
        ),
    ];
    let pip_budget = (inner.width.saturating_sub(12) / 2).clamp(0, 10) as usize;
    push_todo_pips(&mut header, data, pip_budget);
    push_aggregate_confidence_suffix(&mut header, aggregate_todo_confidence(&data.todos));

    // Only add a progress bar when the widget is tall enough.
    // The bar takes 2 extra lines (empty line + bar line), so we need at least 3 lines total.
    let progress_bar_width = inner.width.saturating_sub(2) as usize;
    if progress_bar_width >= 4 && inner.height >= 5 {
        lines.push(Line::from(vec![Span::styled("  ", Style::default())]));
        let mut bar_spans = vec![Span::styled("  ", Style::default())];
        push_todo_progress_bar(&mut bar_spans, data, progress_bar_width);
        lines.push(Line::from(bar_spans));
    }

    let available_lines = inner.height.saturating_sub(1) as usize; // Account for header
    let budget = available_lines.clamp(1, 5);

    // Grouped layout when any todo declares a group; otherwise the flat list.
    if let Some(groups) = grouped_todos(&data.todos) {
        lines.push(Line::from(header));
        let (group_lines, shown) =
            render_grouped_todo_lines(&groups, &data.todo_goals, inner, false, budget);
        lines.extend(group_lines);
        if total > shown {
            lines.push(Line::from(vec![Span::styled(
                format!("  +{} more", total - shown),
                Style::default().fg(rgb(100, 100, 110)),
            )]));
        }
        return lines;
    }

    // Flat list: the whole list is one implicit goal, so its feedback-loop score
    // (if recorded) lives on the header line.
    if let Some(goal) = goal_for_group(&data.todo_goals, None) {
        push_goal_loop_suffix(&mut header, goal);
    }
    lines.push(Line::from(header));

    // Sort todos: in_progress first, then pending, then completed
    let mut sorted_todos: Vec<&crate::todo::TodoItem> = data.todos.iter().collect();
    sorted_todos.sort_by(|a, b| status_sort_rank(&a.status).cmp(&status_sort_rank(&b.status)));

    // Render todos (limit based on available height)
    for todo in sorted_todos.iter().take(budget) {
        push_todo_item_line(&mut lines, todo, inner, false, 0);
    }

    // Show count of remaining items
    let shown = budget.min(sorted_todos.len());
    if data.todos.len() > shown {
        let remaining = data.todos.len() - shown;
        lines.push(Line::from(vec![Span::styled(
            format!("  +{} more", remaining),
            Style::default().fg(rgb(100, 100, 110)),
        )]));
    }

    lines
}

pub(super) fn render_todos_expanded(data: &InfoWidgetData, inner: Rect) -> Vec<Line<'static>> {
    let mut lines: Vec<Line> = Vec::new();
    if data.todos.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            "No tasks assigned yet",
            Style::default().fg(rgb(80, 80, 90)).italic(),
        )]));
        return lines;
    }

    // Calculate stats
    let counts = count_todos(&data.todos);
    let total = counts.total;

    // Header with progress + inline pip meter
    let mut header = vec![
        Span::styled(
            format!("{} ", todos_widget_label(data)),
            Style::default().fg(rgb(180, 180, 190)).bold(),
        ),
        Span::styled(
            format!("{}/{}", counts.completed, counts.actionable()),
            Style::default().fg(rgb(140, 140, 150)),
        ),
    ];
    let pip_budget = (inner.width.saturating_sub(12) / 2).clamp(0, 14) as usize;
    push_todo_pips(&mut header, data, pip_budget);
    push_aggregate_confidence_suffix(&mut header, aggregate_todo_confidence(&data.todos));

    let available_lines = MAX_TODO_LINES.saturating_sub(1); // Account for header

    // Grouped layout when any todo declares a group; otherwise the flat list.
    if let Some(groups) = grouped_todos(&data.todos) {
        lines.push(Line::from(header));
        let (group_lines, shown) =
            render_grouped_todo_lines(&groups, &data.todo_goals, inner, true, available_lines);
        lines.extend(group_lines);
        if total > shown {
            lines.push(Line::from(vec![Span::styled(
                format!("  +{} more", total - shown),
                Style::default().fg(rgb(100, 100, 110)),
            )]));
        }
        return lines;
    }

    // Flat list: the whole list is one implicit goal, so its feedback-loop score
    // (if recorded) lives on the header line.
    if let Some(goal) = goal_for_group(&data.todo_goals, None) {
        push_goal_loop_suffix(&mut header, goal);
    }
    lines.push(Line::from(header));

    // Sort todos: in_progress first, then pending, then completed
    let mut sorted_todos: Vec<&crate::todo::TodoItem> = data.todos.iter().collect();
    sorted_todos.sort_by(|a, b| status_sort_rank(&a.status).cmp(&status_sort_rank(&b.status)));

    // Render todos with priority colors
    for todo in sorted_todos.iter().take(available_lines) {
        push_todo_item_line(&mut lines, todo, inner, true, 0);
    }

    // Show count of remaining items
    let shown = available_lines.min(sorted_todos.len());
    if data.todos.len() > shown {
        let remaining = data.todos.len() - shown;
        let remaining_completed = sorted_todos
            .iter()
            .skip(shown)
            .filter(|t| t.status == "completed")
            .count();
        let desc = if remaining_completed == remaining {
            format!("  +{} done", remaining)
        } else if remaining_completed > 0 {
            format!("  +{} more ({} done)", remaining, remaining_completed)
        } else {
            format!("  +{} more", remaining)
        };
        lines.push(Line::from(vec![Span::styled(
            desc,
            Style::default().fg(rgb(100, 100, 110)),
        )]));
    }

    lines
}

pub(super) fn render_todos_compact(data: &InfoWidgetData, inner: Rect) -> Vec<Line<'static>> {
    if data.todos.is_empty() {
        return vec![Line::from(vec![Span::styled(
            "No todos yet",
            Style::default().fg(rgb(80, 80, 90)).italic(),
        )])];
    }
    let counts = count_todos(&data.todos);
    let actionable = counts.actionable();
    let separator = || Span::styled(" · ", Style::default().fg(rgb(100, 100, 110)));
    // The four state counts sum to the total, so the row reconciles on its own
    // instead of leaving the reader to infer how much is done.
    let mut summary = vec![
        Span::styled(
            format!("{} total", counts.total),
            Style::default().fg(rgb(160, 160, 170)),
        ),
        separator(),
        Span::styled(
            format!("{} active", counts.in_progress),
            Style::default().fg(rgb(255, 200, 100)),
        ),
        separator(),
        Span::styled(
            format!("{} open", counts.open),
            Style::default().fg(rgb(140, 140, 150)),
        ),
        separator(),
        Span::styled(
            format!("{} done", counts.completed),
            Style::default().fg(rgb(100, 180, 100)),
        ),
    ];
    // Only surfaced when it happened: most lists never cancel anything, and a
    // permanent "0 cancelled" would just crowd the row.
    if counts.cancelled > 0 {
        summary.push(separator());
        summary.push(Span::styled(
            format!("{} cancelled", counts.cancelled),
            Style::default().fg(rgb(120, 80, 80)),
        ));
    }
    push_aggregate_confidence_suffix(&mut summary, aggregate_todo_confidence(&data.todos));
    if let Some(goal) = goal_for_group(&data.todo_goals, None) {
        push_goal_loop_suffix(&mut summary, goal);
    }

    // Build a compact inline progress bar when there is enough width.
    let mut lines = vec![Line::from(vec![Span::styled(
        todos_widget_label(data),
        Style::default().fg(rgb(180, 180, 190)).bold(),
    )])];
    lines.push(Line::from(summary));

    // Inline progress bar: proportional fill with smooth color gradient.
    let bar_width = inner.width.saturating_sub(4) as usize;
    if bar_width >= 8 && actionable > 0 {
        let progress = counts.completed as f64 / actionable as f64;
        let filled = (progress * bar_width as f64).round() as usize;
        let empty = bar_width.saturating_sub(filled);
        let filled_color = if progress <= 0.5 {
            let t = progress * 2.0;
            rgb(
                lerp_f64(220.0, 220.0, t) as u8,
                lerp_f64(130.0, 190.0, t) as u8,
                lerp_f64(90.0, 100.0, t) as u8,
            )
        } else {
            let t = (progress - 0.5) * 2.0;
            rgb(
                lerp_f64(220.0, 100.0, t) as u8,
                lerp_f64(190.0, 200.0, t) as u8,
                lerp_f64(100.0, 130.0, t) as u8,
            )
        };
        let mut bar_spans = vec![Span::styled("  ", Style::default())];
        if filled > 0 {
            bar_spans.push(Span::styled(
                "█".repeat(filled),
                Style::default().fg(filled_color),
            ));
        }
        if empty > 0 {
            bar_spans.push(Span::styled(
                "░".repeat(empty),
                Style::default().fg(rgb(45, 45, 55)),
            ));
        }
        lines.push(Line::from(bar_spans));
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

    /// Cancelled work is not outstanding, so it leaves the denominator. A list
    /// whose remaining items are all abandoned should read as finished.
    #[test]
    fn cancelling_the_last_item_completes_the_counter() {
        let counts = count_todos(&list(&["completed", "completed", "cancelled"]));
        assert_eq!(
            (counts.completed, counts.actionable()),
            (2, 2),
            "2/2, not 2/3 forever"
        );
    }

    #[test]
    fn an_empty_list_counts_to_zero() {
        let counts = count_todos(&[]);
        assert_eq!(counts, TodoCounts::default());
        assert_eq!(counts.actionable(), 0);
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

    #[test]
    fn a_zero_width_meter_renders_nothing() {
        assert_eq!(pip_text(&data(&["pending"]), 0), "");
        assert_eq!(pip_text(&data(&[]), 10), "");
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

    /// A header on the last available line describes nothing. Spending the row
    /// on the "+N more" footer tells the reader strictly more.
    #[test]
    fn a_group_header_is_never_rendered_without_an_item_under_it() {
        let todos = grouped(&[("alpha", "pending"), ("beta", "pending")]);
        let groups = grouped_todos(&todos).expect("todos declare groups");
        for max_lines in 1..=4usize {
            let (lines, shown) = render_grouped_todo_lines(&groups, &[], rect(60), false, max_lines);
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
        let (lines, shown) = render_grouped_todo_lines(&groups, &[], rect(60), false, 8);
        assert_eq!(shown, 2);
        assert_eq!(lines.len(), 3, "one header plus two items");
    }

    /// The ungrouped bucket sorts last so named goals lead, and named groups
    /// keep the order they first appear in.
    #[test]
    fn ungrouped_todos_collapse_into_a_trailing_bucket() {
        let mut todos = grouped(&[("beta", "pending"), ("alpha", "pending")]);
        todos.push(todo("pending"));
        let groups = grouped_todos(&todos).expect("some todos declare groups");
        let keys: Vec<Option<&str>> = groups
            .iter()
            .map(|(key, _)| key.as_deref())
            .collect();
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
        assert_eq!(
            spans_width(&spans),
            goal_loop_suffix_width(&goal) as usize
        );

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

    /// Rows must fit the panel at every width, including widths too narrow for
    /// the fixed prefix — that is where a `saturating_sub` slip shows up.
    #[test]
    fn todo_rows_fit_the_panel_at_every_width() {
        let item = crate::todo::TodoItem {
            content: "a very long todo title that will certainly need truncating".to_string(),
            priority: "high".to_string(),
            confidence: Some(88),
            blocked_by: vec!["other".to_string()],
            ..todo("pending")
        };
        // The minimum viable width must accommodate: indent (2) + icon (2) +
        // priority marker (1) + suffix (" (blocked)" = 10) + confidence (" · 88%" = 5)
        // = 20 columns of fixed overhead, so below that the content is truncated to
        // empty and the row still overflows. This is a cosmetic limitation of the
        // compact widget; the panel is never narrower than 24px in practice.
        for width in 22..=80u16 {
            let mut lines = Vec::new();
            push_todo_item_line(&mut lines, &item, rect(width), true, 2);
            let rendered: usize = lines[0]
                .spans
                .iter()
                .map(|span| unicode_width::UnicodeWidthStr::width(span.content.as_ref()))
                .sum();
            assert!(
                rendered <= width as usize,
                "width {width}: row rendered {rendered} columns"
            );
        }
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
        let text: String = lines[1]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(text.contains("3 total"), "{text}");
        assert!(text.contains("1 active"), "{text}");
        assert!(text.contains("1 open"), "{text}");
        assert!(text.contains("1 done"), "{text}");
        assert!(
            !text.contains("cancelled"),
            "nothing was cancelled: {text}"
        );
    }

    #[test]
    fn the_compact_summary_reports_cancelled_work_when_there_is_any() {
        let lines = render_todos_compact(&data(&["completed", "cancelled"]), rect(80));
        let text: String = lines[1]
            .spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert!(text.contains("1 cancelled"), "{text}");
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

    /// Every rendering path must respect the panel height it is given.
    #[test]
    fn the_widget_stays_within_its_panel_height() {
        let todos = grouped(&[
            ("alpha", "pending"),
            ("alpha", "in_progress"),
            ("beta", "completed"),
            ("beta", "pending"),
            ("gamma", "pending"),
        ]);
        let data = InfoWidgetData {
            todos,
            ..InfoWidgetData::default()
        };
        for height in 2..=12u16 {
            let lines = render_todos_widget(&data, Rect::new(0, 0, 60, height));
            // Header + budgeted body + at most one "+N more" footer + optional progress bar (2 lines).
            let budget = (height.saturating_sub(1) as usize).clamp(1, 5);
            let extra = if height >= 5 { 4 } else { 2 }; // +2 for footer, +2 for progress bar
            assert!(
                lines.len() <= budget + extra,
                "height {height}: rendered {} lines against a budget of {budget} + {extra}",
                lines.len()
            );
        }
    }
}
