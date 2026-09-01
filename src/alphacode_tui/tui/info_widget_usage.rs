use super::{InfoWidgetData, UsageInfo, UsageProvider};
use crate::alphacode_tui::tui::color_support::rgb;
use ratatui::prelude::*;
use unicode_width::UnicodeWidthStr;

pub(super) fn render_usage_widget(data: &InfoWidgetData, inner: Rect) -> Vec<Line<'static>> {
    let Some(info) = &data.usage_info else {
        return Vec::new();
    };
    if !info.available {
        return Vec::new();
    }

    match info.provider {
        UsageProvider::Copilot => {
            vec![Line::from(vec![Span::styled(
                format!(
                    "{} in + {} out",
                    format_tokens(info.input_tokens),
                    format_tokens(info.output_tokens)
                ),
                Style::default().fg(rgb(140, 140, 150)),
            )])]
        }
        UsageProvider::CostBased => {
            vec![
                Line::from(vec![
                    Span::styled("💰 ", Style::default().fg(rgb(140, 180, 255))),
                    Span::styled(
                        format!("${:.4}", info.total_cost),
                        Style::default().fg(rgb(180, 180, 190)).bold(),
                    ),
                ]),
                Line::from(vec![Span::styled(
                    format!(
                        "{} in + {} out",
                        format_tokens(info.input_tokens),
                        format_tokens(info.output_tokens)
                    ),
                    Style::default().fg(rgb(140, 140, 150)),
                )]),
            ]
        }
        _ => {
            let five_hr_used = (info.five_hour * 100.0).round().clamp(0.0, 100.0) as u8;
            let seven_day_used = (info.seven_day * 100.0).round().clamp(0.0, 100.0) as u8;
            let five_hr_left = 100u8.saturating_sub(five_hr_used);
            let seven_day_left = 100u8.saturating_sub(seven_day_used);

            let five_hr_reset = info
                .five_hour_resets_at
                .as_deref()
                .map(crate::usage::format_reset_time);
            let seven_day_reset = info
                .seven_day_resets_at
                .as_deref()
                .map(crate::usage::format_reset_time);

            let mut lines = Vec::new();
            let label = info.provider.label();
            if !label.is_empty() {
                lines.push(Line::from(vec![Span::styled(
                    format!("{} limits", label),
                    Style::default()
                        .fg(rgb(140, 140, 150))
                        .add_modifier(ratatui::style::Modifier::DIM),
                )]));
            }
            if let Some(primary_label) = info.primary_limit_label.as_deref() {
                lines.push(render_labeled_bar(
                    primary_label,
                    five_hr_used,
                    five_hr_left,
                    five_hr_reset.as_deref(),
                    inner.width,
                ));
            }
            if let Some(secondary_label) = info.secondary_limit_label.as_deref() {
                lines.push(render_labeled_bar(
                    secondary_label,
                    seven_day_used,
                    seven_day_left,
                    seven_day_reset.as_deref(),
                    inner.width,
                ));
            }
            if let Some(spark_usage) = info.spark {
                let spark_used = (spark_usage * 100.0).round().clamp(0.0, 100.0) as u8;
                let spark_left = 100u8.saturating_sub(spark_used);
                let spark_reset = info
                    .spark_resets_at
                    .as_deref()
                    .map(crate::usage::format_reset_time);
                lines.push(render_labeled_bar(
                    "Spark",
                    spark_used,
                    spark_left,
                    spark_reset.as_deref(),
                    inner.width,
                ));
            }
            lines
        }
    }
}

pub(super) fn render_usage_compact(info: &UsageInfo, width: u16) -> Vec<Line<'static>> {
    if !info.available {
        return Vec::new();
    }

    if matches!(info.provider, UsageProvider::CostBased) {
        return vec![Line::from(vec![Span::styled(
            format!(
                "${:.4} · {} in + {} out",
                info.total_cost,
                format_tokens(info.input_tokens),
                format_tokens(info.output_tokens)
            ),
            Style::default().fg(rgb(140, 140, 150)),
        )])];
    }

    let five_hr_used = (info.five_hour * 100.0).round().clamp(0.0, 100.0) as u8;
    let seven_day_used = (info.seven_day * 100.0).round().clamp(0.0, 100.0) as u8;
    let five_hr_left = 100u8.saturating_sub(five_hr_used);
    let seven_day_left = 100u8.saturating_sub(seven_day_used);
    let five_hr_reset = info
        .five_hour_resets_at
        .as_deref()
        .map(crate::usage::format_reset_time);
    let seven_day_reset = info
        .seven_day_resets_at
        .as_deref()
        .map(crate::usage::format_reset_time);

    let mut lines = Vec::new();
    let label = info.provider.label();
    if !label.is_empty() {
        lines.push(Line::from(vec![Span::styled(
            format!("{} limits", label),
            Style::default()
                .fg(rgb(140, 140, 150))
                .add_modifier(ratatui::style::Modifier::DIM),
        )]));
    }
    if let Some(primary_label) = info.primary_limit_label.as_deref() {
        lines.push(render_labeled_bar(
            primary_label,
            five_hr_used,
            five_hr_left,
            five_hr_reset.as_deref(),
            width,
        ));
    }
    if let Some(secondary_label) = info.secondary_limit_label.as_deref() {
        lines.push(render_labeled_bar(
            secondary_label,
            seven_day_used,
            seven_day_left,
            seven_day_reset.as_deref(),
            width,
        ));
    }
    if let Some(spark_usage) = info.spark {
        let spark_used = (spark_usage * 100.0).round().clamp(0.0, 100.0) as u8;
        let spark_left = 100u8.saturating_sub(spark_used);
        let spark_reset = info
            .spark_resets_at
            .as_deref()
            .map(crate::usage::format_reset_time);
        lines.push(render_labeled_bar(
            "Spark",
            spark_used,
            spark_left,
            spark_reset.as_deref(),
            width,
        ));
    }
    lines
}

fn render_labeled_bar(
    label: &str,
    used_pct: u8,
    left_pct: u8,
    reset_time: Option<&str>,
    width: u16,
) -> Line<'static> {
    // Smooth gradient color based on usage level
    let color = if left_pct <= 10 {
        rgb(255, 80, 80)   // Critical red
    } else if left_pct <= 25 {
        rgb(255, 120, 80)  // Warning orange
    } else if left_pct <= 50 {
        rgb(255, 200, 100) // Caution yellow
    } else if left_pct <= 75 {
        rgb(150, 210, 130) // Good green
    } else {
        rgb(100, 220, 150) // Excellent green
    };

    const LABEL_WIDTH: usize = 7;
    const MIN_BAR_WIDTH: usize = 4;
    const MAX_BAR_WIDTH: usize = 14; // Slightly wider for better visual

    let total = usize::from(width);

    let full_suffix = match reset_time {
        Some(reset) if left_pct == 0 => format!(" resets {}", reset),
        Some(reset) => format!(" {}% left · {}", left_pct, reset),
        None => format!(" {}% left", left_pct),
    };
    // On narrow widgets keep the reset visible and progressively shorten the
    // percentage wording before sacrificing the bar. The exhausted wording is
    // already compact and remains unchanged.
    let suffix = match reset_time {
        Some(reset) if left_pct > 0 => {
            let compact = format!(" {}% · {}", left_pct, reset);
            let reset_only = format!(" · {}", reset);
            let budget = total.saturating_sub(LABEL_WIDTH + MIN_BAR_WIDTH);
            if UnicodeWidthStr::width(full_suffix.as_str()) <= budget {
                full_suffix
            } else if UnicodeWidthStr::width(compact.as_str()) <= budget {
                compact
            } else {
                reset_only
            }
        }
        _ => full_suffix,
    };
    // Even the shortest wording outruns a sufficiently narrow widget, and an
    // unclamped suffix pushes the row past the panel edge — the bar and label
    // both collapse to nothing first, so the suffix is the last thing holding
    // the row to its budget.
    let suffix = super::text::truncate_width(&suffix, total).to_string();
    let suffix_width = UnicodeWidthStr::width(suffix.as_str());

    // Columns, not `char`s: a wide label (or one padded with `{:<n$}`, which
    // counts characters) reserves fewer columns than it draws.
    let label_budget = LABEL_WIDTH.min(total.saturating_sub(suffix_width));
    let visible_label = super::text::truncate_width(label, label_budget);
    let label_pad = label_budget.saturating_sub(UnicodeWidthStr::width(visible_label));
    let padded_label = format!("{visible_label}{}", " ".repeat(label_pad));

    let bar_width = total
        .saturating_sub(label_budget + suffix_width)
        .min(MAX_BAR_WIDTH);

    let filled = (((used_pct as f32 / 100.0) * bar_width as f32).round() as usize).min(bar_width);
    let empty = bar_width.saturating_sub(filled);

    // Gradient-filled bar: lower blocks on the left, full block on the right
    // for a smoother, more premium visual feel.
    let gradient_blocks = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let bar_filled: String = if filled > 0 {
        let mut chars = String::with_capacity(filled);
        for i in 0..filled {
            let level = if filled == 1 { 7 } else { ((i as f32 / (filled - 1) as f32) * 7.0).round() as usize };
            chars.push(gradient_blocks[level.min(7)]);
        }
        chars
    } else {
        String::new()
    };
    let bar_empty = "░".repeat(empty);

    Line::from(vec![
        Span::styled(padded_label, Style::default().fg(rgb(140, 140, 150))),
        Span::styled(bar_filled, Style::default().fg(color)),
        Span::styled(bar_empty, Style::default().fg(rgb(35, 38, 48))),
        Span::styled(suffix, Style::default().fg(color)),
    ])
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
    /// budget is clipped by ratatui — potentially mid-glyph, since every cell
    /// of the meter is multi-byte.
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

    /// Labels come from the provider, so a CJK or emoji one must reserve the
    /// columns it actually draws rather than the characters it contains.
    #[test]
    fn a_wide_label_is_measured_in_columns() {
        for width in 0..=40u16 {
            let text = line_text(&render_labeled_bar("週次上限", 50, 50, None, width));
            assert!(
                UnicodeWidthStr::width(text.as_str()) <= usize::from(width),
                "width {width} rendered {} columns: {text:?}",
                UnicodeWidthStr::width(text.as_str())
            );
        }
    }

    /// The meter must not claim more progress than there is; rounding at
    /// small bar widths used to be able to fill a cell past the end.
    #[test]
    fn the_meter_never_overfills() {
        for width in 0..=40u16 {
            for used in 0..=100u8 {
                let line = render_labeled_bar("5-hour", used, 100 - used, None, width);
                let text = line_text(&line);
                let filled = text.matches('█').count();
                let empty = text.matches('░').count();
                assert!(
                    filled + empty <= 14,
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
        assert!(!text.contains('░'), "no empty cells remain at 100%: {text:?}");
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
    /// must not divide by zero or render a negative fraction.
    #[test]
    fn a_zero_limit_does_not_panic() {
        let text = line_text(&render_context_usage_line("ctx", 100, 0, 40));
        assert!(!text.is_empty());
        let pill = line_text(&render_usage_pill(100, 0, 24));
        assert!(UnicodeWidthStr::width(pill.as_str()) <= 24);
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

pub(super) fn render_usage_pill(
    used_tokens: usize,
    limit_tokens: usize,
    width: u16,
) -> Line<'static> {
    let safe_limit = limit_tokens.max(1);
    let bar_width = (width as usize).min(24);
    if bar_width == 0 {
        return Line::default();
    }

    let mut used_cells = ((used_tokens as f64 / safe_limit as f64) * bar_width as f64)
        .round()
        .max(0.0) as usize;
    if used_cells > bar_width {
        used_cells = bar_width;
    }

    let used_pct = ((used_tokens as f64 / safe_limit as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8;
    let left_pct = 100u8.saturating_sub(used_pct);
    let used_color = if left_pct <= 10 {
        rgb(255, 80, 80)   // Critical
    } else if left_pct <= 25 {
        rgb(255, 120, 80)  // Warning
    } else if left_pct <= 50 {
        rgb(255, 200, 100) // Caution
    } else if left_pct <= 75 {
        rgb(150, 210, 130) // Good
    } else {
        rgb(100, 220, 150) // Excellent
    };

    let empty_cells = bar_width.saturating_sub(used_cells);
    let mut spans = Vec::new();
    spans.push(Span::styled(
        "█".repeat(used_cells),
        Style::default().fg(used_color),
    ));
    if empty_cells > 0 {
        spans.push(Span::styled(
            "░".repeat(empty_cells),
            Style::default().fg(rgb(40, 42, 50)),
        ));
    }
    Line::from(spans)
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

    let tokens = format!(
        "{}/{}",
        format_token_k(used_tokens),
        format_token_k(limit_tokens)
    );
    let used_pct = ((used_tokens as f64 / limit_tokens.max(1) as f64) * 100.0)
        .round()
        .clamp(0.0, 100.0) as u8;
    let left_pct = 100u8.saturating_sub(used_pct);
    let token_color = if left_pct <= 10 {
        rgb(255, 80, 80)   // Critical
    } else if left_pct <= 25 {
        rgb(255, 120, 80)  // Warning
    } else if left_pct <= 50 {
        rgb(255, 200, 100) // Caution
    } else if left_pct <= 75 {
        rgb(150, 210, 130) // Good
    } else {
        rgb(100, 220, 150) // Excellent
    };

    // Spend the row left to right — label, then counter, then bar — and stop
    // when the budget runs out. The label and counter used to be pushed
    // unconditionally, so a narrow rail overflowed before the bar was even
    // considered.
    let mut spans = Vec::new();
    let mut remaining = total;

    // The label says which meter this is, so it is truncated rather than
    // dropped: a clipped name still identifies the row.
    let label_text = super::text::truncate_width(&format!("{label} "), remaining).to_string();
    remaining -= UnicodeWidthStr::width(label_text.as_str());
    spans.push(Span::styled(
        label_text,
        Style::default().fg(rgb(140, 140, 150)),
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

    if remaining >= 3 {
        spans.extend(render_usage_pill(used_tokens, limit_tokens, remaining as u16).spans);
    }
    Line::from(spans)
}

fn format_token_k(tokens: usize) -> String {
    if tokens >= 1000 {
        format!("{}k", tokens / 1000)
    } else {
        format!("{}", tokens)
    }
}

fn format_tokens(tokens: u64) -> String {
    if tokens >= 1_000_000 {
        format!("{:.1}M", tokens as f64 / 1_000_000.0)
    } else if tokens >= 1_000 {
        format!("{:.1}K", tokens as f64 / 1_000.0)
    } else {
        format!("{}", tokens)
    }
}

