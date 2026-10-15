use super::{InfoWidgetData, UsageInfo, UsageProvider};
use crate::alphacode_tui_style::palette::{Role, role_color};
use crate::alphacode_tui_style::role::themed_rgb;
use ratatui::prelude::*;
use unicode_width::UnicodeWidthStr;

const LABEL_WIDTH: usize = 7;
const MIN_BAR_WIDTH: usize = 4;
const MAX_BAR_WIDTH: usize = 14;
const MAX_PILL_WIDTH: usize = 24;
/// Left-aligned partial blocks for 1/8 .. 7/8 of a cell.
const PARTIAL_BLOCKS: [char; 7] = ['▏', '▎', '▍', '▌', '▋', '▊', '▉'];

// ---------------------------------------------------------------------------
// Theme colors
// ---------------------------------------------------------------------------

/// Usage-level color shared by every limit/context bar.
///
/// Resolves through palette roles (`Error` → `Warning` → `Success`) so
/// `/theme` retunes these bars along with the rest of the UI.
fn usage_level_color(left_pct: u8) -> Color {
    match left_pct {
        0..=10 => role_color(Role::Error),
        11..=25 => themed_rgb(Role::Warning, (-10, -15, -30)),
        26..=50 => role_color(Role::Warning),
        51..=75 => themed_rgb(Role::Success, (-25, -10, 5)),
        _ => role_color(Role::Success),
    }
}

/// Track color for the unfilled portion of usage bars (border role, so light
/// themes keep contrast).
fn usage_track_color() -> Color {
    themed_rgb(Role::Border, (-10, -10, 5))
}

/// Label color for usage bars, from the muted-text role.
fn usage_label_color() -> Color {
    role_color(Role::MutedText)
}

// ---------------------------------------------------------------------------
// Quota math
// ---------------------------------------------------------------------------

/// Whole-percent used/left derived from a 0..=1 fraction.
///
/// Rounding is never allowed to lie about the extremes: a quota with anything
/// left over never reads "0% left" / 100% used, and non-finite input is
/// treated as unused instead of poisoning the display.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Quota {
    used: u8,
    left: u8,
}

impl Quota {
    fn from_fraction(fraction: f64) -> Self {
        let f = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut used = (f * 100.0).round() as u8;
        if f < 1.0 && used == 100 {
            used = 99;
        }
        Self {
            used,
            left: 100 - used,
        }
    }
}

/// `used / limit` clamped to `0..=1`; `0.0` when there is no limit.
fn usage_ratio(used: usize, limit: usize) -> f64 {
    if limit == 0 {
        0.0
    } else {
        (used as f64 / limit as f64).clamp(0.0, 1.0)
    }
}

// ---------------------------------------------------------------------------
// Meter
// ---------------------------------------------------------------------------

/// Draw a `width`-cell meter with 1/8-cell resolution at the leading edge.
///
/// Honest by construction: 0 draws nothing, anything nonzero draws at least a
/// sliver, and only an exactly full quota fills the last cell.
fn meter_spans(fraction: f64, width: usize, fill: Color) -> Vec<Span<'static>> {
    if width == 0 {
        return Vec::new();
    }
    let f = if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let total = width * 8;
    let mut eighths = (f * total as f64).round() as usize;
    if f > 0.0 && eighths == 0 {
        eighths = 1;
    }
    if f < 1.0 && eighths >= total {
        eighths = total - 1;
    }

    let full = eighths / 8;
    let part = eighths % 8;
    let empty = width - full - usize::from(part > 0);

    let mut filled = "█".repeat(full);
    if part > 0 {
        filled.push(PARTIAL_BLOCKS[part - 1]);
    }

    let mut spans = Vec::with_capacity(2);
    if !filled.is_empty() {
        spans.push(Span::styled(filled, Style::default().fg(fill)));
    }
    if empty > 0 {
        spans.push(Span::styled(
            "░".repeat(empty),
            Style::default().fg(usage_track_color()),
        ));
    }
    spans
}

// ---------------------------------------------------------------------------
// Public renderers
// ---------------------------------------------------------------------------

pub(super) fn render_usage_widget(data: &InfoWidgetData, inner: Rect) -> Vec<Line<'static>> {
    data.usage_info
        .as_ref()
        .map(|info| render_usage_lines(info, inner.width))
        .unwrap_or_default()
}

/// All usage rows for `info`, fitted to `width` columns.
pub(super) fn render_usage_lines(info: &UsageInfo, width: u16) -> Vec<Line<'static>> {
    if !info.available {
        return Vec::new();
    }
    match info.provider {
        UsageProvider::Copilot => vec![token_summary_line(info, width)],
        UsageProvider::CostBased => vec![
            Line::from(vec![
                Span::raw("💰 "),
                Span::styled(
                    clip(
                        &format_cost(f64::from(info.total_cost)),
                        width.saturating_sub(3),
                    ),
                    Style::default().fg(usage_label_color()).bold(),
                ),
            ]),
            token_summary_line(info, width),
        ],
        _ => render_limit_lines(info, width),
    }
}

/// Kept for tests that exercise the compact, single-line cost layout.
#[cfg(test)]
pub(super) fn render_usage_compact(info: &UsageInfo, width: u16) -> Vec<Line<'static>> {
    if info.available && matches!(info.provider, UsageProvider::CostBased) {
        return vec![Line::from(Span::styled(
            clip(
                &format!(
                    "{} · {} in + {} out",
                    format_cost(f64::from(info.total_cost)),
                    format_tokens(info.input_tokens),
                    format_tokens(info.output_tokens)
                ),
                width,
            ),
            Style::default().fg(usage_label_color()),
        ))];
    }
    render_usage_lines(info, width)
}

fn render_limit_lines(info: &UsageInfo, width: u16) -> Vec<Line<'static>> {
    let mut lines = Vec::new();

    let label = info.provider.label();
    if !label.is_empty() {
        lines.push(Line::from(Span::styled(
            clip(&format!("{label} limits"), width),
            Style::default()
                .fg(usage_label_color())
                .add_modifier(Modifier::DIM),
        )));
    }
    if let Some(primary) = info.primary_limit_label.as_deref() {
        push_bar(
            &mut lines,
            primary,
            f64::from(info.five_hour),
            info.five_hour_resets_at.as_deref(),
            width,
        );
    }
    if let Some(secondary) = info.secondary_limit_label.as_deref() {
        push_bar(
            &mut lines,
            secondary,
            f64::from(info.seven_day),
            info.seven_day_resets_at.as_deref(),
            width,
        );
    }
    if let Some(spark) = info.spark {
        push_bar(
            &mut lines,
            "Spark",
            f64::from(spark),
            info.spark_resets_at.as_deref(),
            width,
        );
    }
    lines
}

fn push_bar(
    lines: &mut Vec<Line<'static>>,
    label: &str,
    fraction: f64,
    resets_at: Option<&str>,
    width: u16,
) {
    let quota = Quota::from_fraction(fraction);
    let reset = resets_at.map(crate::usage::format_reset_time);
    lines.push(render_labeled_bar(
        label,
        quota.used,
        quota.left,
        reset.as_deref(),
        width,
    ));
}

/// Pick the richest suffix wording that fits `budget` columns, degrading
/// progressively (full → compact → reset-only). The last candidate is returned
/// even if it does not fit; the caller clamps it.
fn bar_suffix(left_pct: u8, reset: Option<&str>, budget: usize) -> String {
    let candidates: Vec<String> = match reset {
        Some(r) if left_pct == 0 => vec![format!(" resets {r}"), format!(" · {r}")],
        Some(r) => vec![
            format!(" {left_pct}% left · {r}"),
            format!(" {left_pct}% · {r}"),
            format!(" · {r}"),
        ],
        None => vec![format!(" {left_pct}% left"), format!(" {left_pct}%")],
    };
    let mut chosen = String::new();
    for candidate in candidates {
        let fits = UnicodeWidthStr::width(candidate.as_str()) <= budget;
        chosen = candidate;
        if fits {
            break;
        }
    }
    chosen
}

fn render_labeled_bar(
    label: &str,
    used_pct: u8,
    left_pct: u8,
    reset_time: Option<&str>,
    width: u16,
) -> Line<'static> {
    let color = usage_level_color(left_pct);
    let total = usize::from(width);

    let budget = total.saturating_sub(LABEL_WIDTH + MIN_BAR_WIDTH);
    let suffix = bar_suffix(left_pct, reset_time, budget);
    // Even the shortest wording can outrun a very narrow widget; the suffix is
    // the last thing holding the row to its budget, so clamp it.
    let suffix = super::text::truncate_width(&suffix, total).to_string();
    let suffix_width = UnicodeWidthStr::width(suffix.as_str());

    // Columns, not `char`s: a wide label would otherwise reserve fewer columns
    // than it draws.
    let label_budget = LABEL_WIDTH.min(total.saturating_sub(suffix_width));
    let visible_label = super::text::truncate_width(label, label_budget);
    let label_pad = label_budget.saturating_sub(UnicodeWidthStr::width(visible_label));
    let padded_label = format!("{visible_label}{}", " ".repeat(label_pad));

    let bar_width = total
        .saturating_sub(label_budget + suffix_width)
        .min(MAX_BAR_WIDTH);

    // Critical quotas are also bold, so urgency is not carried by color alone.
    let mut suffix_style = Style::default().fg(color);
    if left_pct <= 10 {
        suffix_style = suffix_style.bold();
    }

    let mut spans = vec![Span::styled(
        padded_label,
        Style::default().fg(usage_label_color()),
    )];
    spans.extend(meter_spans(f64::from(used_pct) / 100.0, bar_width, color));
    spans.push(Span::styled(suffix, suffix_style));
    Line::from(spans)
}

pub(super) fn render_usage_pill(
    used_tokens: usize,
    limit_tokens: usize,
    width: u16,
) -> Line<'static> {
    let bar_width = usize::from(width).min(MAX_PILL_WIDTH);
    // Without a limit there is nothing to measure against; a full red bar
    // would claim exhaustion that no one reported.
    if bar_width == 0 || limit_tokens == 0 {
        return Line::default();
    }
    let ratio = usage_ratio(used_tokens, limit_tokens);
    let color = usage_level_color(Quota::from_fraction(ratio).left);
    Line::from(meter_spans(ratio, bar_width, color))
}

pub(super) fn render_context_usage_line(
    label: &str,
    used_tokens: usize,
    limit_tokens: usize,
    width: u16,
) -> Line<'static> {
    let total = usize::from(width);
    if total == 0 {
        return Line::default();
    }

    let has_limit = limit_tokens > 0;
    let tokens = if has_limit {
        format!(
            "{}/{}",
            format_token_count(used_tokens as u64),
            format_token_count(limit_tokens as u64)
        )
    } else {
        format_token_count(used_tokens as u64)
    };
    let quota = Quota::from_fraction(usage_ratio(used_tokens, limit_tokens));
    let token_color = usage_level_color(quota.left);

    // Spend the row left to right — label, counter, bar — and stop when the
    // budget runs out.
    let mut spans = Vec::new();
    let mut remaining = total;

    // The label says which meter this is, so it is truncated rather than
    // dropped: a clipped name still identifies the row.
    let label_text = super::text::truncate_width(&format!("{label} "), remaining).to_string();
    remaining = remaining.saturating_sub(UnicodeWidthStr::width(label_text.as_str()));
    spans.push(Span::styled(
        label_text,
        Style::default().fg(usage_label_color()),
    ));

    // The counter only means anything whole: a clipped `40k/20` misreports the
    // limit, so it is dropped entirely when it does not fit.
    let tokens_text = format!("{tokens} ");
    let tokens_width = UnicodeWidthStr::width(tokens_text.as_str());
    if tokens_width <= remaining {
        remaining -= tokens_width;
        spans.push(Span::styled(
            tokens_text,
            Style::default().fg(token_color).bold(),
        ));
    }

    if has_limit && remaining >= 3 {
        spans.extend(render_usage_pill(used_tokens, limit_tokens, remaining as u16).spans);
    }
    Line::from(spans)
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

fn token_summary_line(info: &UsageInfo, width: u16) -> Line<'static> {
    Line::from(Span::styled(
        clip(
            &format!(
                "{} in + {} out",
                format_tokens(info.input_tokens),
                format_tokens(info.output_tokens)
            ),
            width,
        ),
        Style::default().fg(usage_label_color()),
    ))
}

fn clip(text: &str, width: u16) -> String {
    super::text::truncate_width(text, usize::from(width)).to_string()
}

/// `{:.1}` with a trailing `.0` dropped: `2.0 → "2"`, `1.5 → "1.5"`.
fn trim_decimal(value: f64, suffix: &str) -> String {
    let s = format!("{value:.1}");
    format!("{}{suffix}", s.strip_suffix(".0").unwrap_or(&s))
}

/// Compact token count that never rolls over into an ugly unit
/// (`999_999 → "1M"`, not `"1000k"`).
fn format_token_count(tokens: u64) -> String {
    match tokens {
        0..=999 => tokens.to_string(),
        1_000..=9_999 => trim_decimal(tokens as f64 / 1_000.0, "k"),
        10_000..=999_499 => format!("{}k", (tokens + 500) / 1_000),
        _ => trim_decimal(tokens as f64 / 1_000_000.0, "M"),
    }
}

fn format_tokens(tokens: u64) -> String {
    format_token_count(tokens)
}

/// Sub-dollar spend keeps four decimals so small costs stay visible; larger
/// amounts read as ordinary money.
fn format_cost(cost: f64) -> String {
    if !cost.is_finite() || cost <= 0.0 {
        "$0".to_string()
    } else if cost < 1.0 {
        format!("${cost:.4}")
    } else {
        format!("${cost:.2}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    fn spans_text(spans: &[Span<'_>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn usage_bar_shows_reset_countdown_before_exhaustion() {
        let text = line_text(&render_labeled_bar("5-hour", 38, 62, Some("4h 5m"), 40));

        assert!(text.contains("62% left · 4h 5m"));
        assert!(UnicodeWidthStr::width(text.as_str()) <= 40);
    }

    #[test]
    fn usage_bar_keeps_countdown_within_narrow_width() {
        let text = line_text(&render_labeled_bar("Weekly", 19, 81, Some("1d 4h"), 23));

        assert!(text.contains("81% · 1d 4h"));
        assert!(UnicodeWidthStr::width(text.as_str()) <= 23);
        assert!(text.contains('█') || text.contains('░'));
    }

    #[test]
    fn exhausted_usage_bar_preserves_resets_wording_and_width() {
        let text = line_text(&render_labeled_bar("5-hour", 100, 0, Some("12m"), 24));

        assert!(text.contains("resets 12m"));
        assert!(!text.contains("0% left"));
        assert!(UnicodeWidthStr::width(text.as_str()) <= 24);
    }

    #[test]
    fn openai_monthly_usage_renders_only_the_reported_window() {
        let info = UsageInfo {
            provider: UsageProvider::OpenAI,
            primary_limit_label: Some("Monthly".to_string()),
            five_hour: 1.0,
            available: true,
            ..Default::default()
        };

        let lines = render_usage_compact(&info, 40);
        let text = lines.iter().map(line_text).collect::<Vec<_>>().join("\n");

        assert!(text.contains("Monthly"));
        assert!(!text.contains("5-hour"));
        assert!(!text.contains("Weekly"));
        assert_eq!(lines.len(), 2); // Provider label plus one quota bar.
    }

    /// The bar is drawn into a fixed-width panel, so a row wider than its
    /// budget is clipped by ratatui, potentially mid-glyph.
    #[test]
    fn the_bar_fits_its_width_at_every_size() {
        for width in 0..=60u16 {
            for (used, left) in [(0u8, 100u8), (38, 62), (81, 19), (100, 0)] {
                for reset in [None, Some("4h 5m"), Some("12m"), Some("1d 4h")] {
                    let text = line_text(&render_labeled_bar("5-hour", used, left, reset, width));
                    assert!(
                        UnicodeWidthStr::width(text.as_str()) <= usize::from(width),
                        "width {width} used {used} reset {reset:?} rendered {} columns: {text:?}",
                        UnicodeWidthStr::width(text.as_str())
                    );
                }
            }
        }
    }

    /// Labels come from the provider, so a wide-script or emoji one must reserve
    /// the columns it actually draws rather than the characters it contains.
    /// Written as an escape so this file stays plain ASCII.
    #[test]
    fn a_wide_label_is_measured_in_columns() {
        const WIDE_LABEL: &str = "\u{9031}\u{6b21}\u{4e0a}\u{9650}";
        for width in 0..=40u16 {
            let text = line_text(&render_labeled_bar(WIDE_LABEL, 50, 50, None, width));
            assert!(
                UnicodeWidthStr::width(text.as_str()) <= usize::from(width),
                "width {width} rendered {} columns: {text:?}",
                UnicodeWidthStr::width(text.as_str())
            );
        }
    }

    /// The meter must not claim more progress than there is.
    #[test]
    fn the_meter_never_overfills() {
        for width in 0..=40u16 {
            for used in 0..=100u8 {
                let line = render_labeled_bar("5-hour", used, 100 - used, None, width);
                let text = line_text(&line);
                let filled = text.matches('█').count();
                let empty = text.matches('░').count();
                assert!(
                    filled + empty <= MAX_BAR_WIDTH,
                    "width {width} used {used} drew {filled}+{empty} cells"
                );
                if used == 0 {
                    assert_eq!(filled, 0, "width {width}: an unused quota shows no fill");
                }
            }
        }
    }

    /// A full quota should read as full, not as one cell short of it.
    #[test]
    fn an_exhausted_quota_fills_the_whole_meter() {
        let text = line_text(&render_labeled_bar("5-hour", 100, 0, None, 40));
        assert!(text.contains('█'));
        assert!(
            !text.contains('░'),
            "no empty cells remain at 100%: {text:?}"
        );
    }

    #[test]
    fn a_partly_used_meter_is_never_drawn_full() {
        let text = spans_text(&meter_spans(0.995, 10, Color::Red));
        assert!(
            text.contains(|c| PARTIAL_BLOCKS.contains(&c) || c == '░'),
            "99.5% must leave a visible remainder: {text:?}"
        );
    }

    #[test]
    fn the_meter_has_sub_cell_resolution() {
        assert_eq!(spans_text(&meter_spans(0.5, 4, Color::Red)), "██░░");
        // 2/32 of a 4-cell meter = 2/8 of one cell.
        assert_eq!(spans_text(&meter_spans(0.0625, 4, Color::Red)), "▎░░░");
        assert_eq!(spans_text(&meter_spans(0.0, 4, Color::Red)), "░░░░");
        assert_eq!(spans_text(&meter_spans(1.0, 4, Color::Red)), "████");
        // A nonzero quota is always visible, even when it rounds to nothing.
        assert!(spans_text(&meter_spans(0.0001, 4, Color::Red)).starts_with('▏'));
    }

    #[test]
    fn quota_rounding_does_not_lie_at_the_extremes() {
        assert_eq!(Quota::from_fraction(0.996), Quota { used: 99, left: 1 });
        assert_eq!(Quota::from_fraction(1.0), Quota { used: 100, left: 0 });
        assert_eq!(Quota::from_fraction(2.5), Quota { used: 100, left: 0 });
        assert_eq!(Quota::from_fraction(0.0), Quota { used: 0, left: 100 });
        assert_eq!(Quota::from_fraction(f64::NAN), Quota { used: 0, left: 100 });
        assert_eq!(Quota::from_fraction(-1.0), Quota { used: 0, left: 100 });
    }

    #[test]
    fn token_counts_never_roll_over_into_ugly_units() {
        assert_eq!(format_token_count(0), "0");
        assert_eq!(format_token_count(999), "999");
        assert_eq!(format_token_count(1_000), "1k");
        assert_eq!(format_token_count(1_500), "1.5k");
        assert_eq!(format_token_count(12_345), "12k");
        assert_eq!(format_token_count(200_000), "200k");
        assert_eq!(format_token_count(999_499), "999k");
        assert_eq!(format_token_count(999_999), "1M");
        assert_eq!(format_token_count(1_500_000), "1.5M");
    }

    #[test]
    fn cost_formatting_scales_with_amount() {
        assert_eq!(format_cost(0.0042), "$0.0042");
        assert_eq!(format_cost(12.3456), "$12.35");
        assert_eq!(format_cost(f64::NAN), "$0");
        assert_eq!(format_cost(-3.0), "$0");
    }

    #[test]
    fn the_context_line_fits_its_width() {
        for width in 0..=60u16 {
            let line = render_context_usage_line("ctx", 40_000, 200_000, width);
            let text = line_text(&line);
            assert!(
                UnicodeWidthStr::width(text.as_str()) <= usize::from(width),
                "width {width} rendered {} columns: {text:?}",
                UnicodeWidthStr::width(text.as_str())
            );
        }
    }

    /// A limit of zero is reported by providers that do not publish one; it
    /// must not divide by zero, draw a bogus full bar, or show `/0`.
    #[test]
    fn a_zero_limit_shows_usage_without_a_bar() {
        let text = line_text(&render_context_usage_line("ctx", 100, 0, 40));
        assert!(text.contains("100"));
        assert!(!text.contains('/'));
        assert!(!text.contains('█') && !text.contains('░'));
        assert!(line_text(&render_usage_pill(100, 0, 24)).is_empty());
    }

    /// Usage can exceed a stated limit (a provider revising it downward mid
    /// session); the meter saturates rather than running off the row.
    #[test]
    fn usage_beyond_the_limit_saturates() {
        let pill = line_text(&render_usage_pill(500_000, 200_000, 24));
        assert!(UnicodeWidthStr::width(pill.as_str()) <= 24);
        assert!(!pill.contains('░'), "an over-limit meter reads as full");
    }
}
