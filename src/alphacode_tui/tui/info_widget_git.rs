use super::text::truncate_smart;
use super::{GitInfo, InfoWidgetData};
use crate::alphacode_tui::tui::color_support::rgb;
use ratatui::prelude::*;
use unicode_width::UnicodeWidthStr;

pub(super) fn render_git_widget(data: &InfoWidgetData, inner: Rect) -> Vec<Line<'static>> {
    let Some(info) = &data.git_info else {
        return Vec::new();
    };
    if !info.is_interesting() {
        return Vec::new();
    }

    let w = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();

    let mut parts: Vec<Span> = Vec::new();
    parts.push(Span::styled(" \u{2442} ", Style::default().fg(rgb(240, 160, 60))));

    // Measure what gets pushed, not what was counted in `char`s. The stats are
    // ASCII today, but budgeting in columns keeps the invariant uniform.
    let mut stats_width = 0usize;
    let mut stat_spans = Vec::new();
    if info.ahead > 0 {
        let text = format!(" \u{2191}{}", info.ahead);
        stats_width += text.width();
        stat_spans.push((text, rgb(100, 220, 140)));
    }
    if info.behind > 0 {
        let text = format!(" \u{2193}{}", info.behind);
        stats_width += text.width();
        stat_spans.push((text, rgb(255, 150, 110)));
    }
    if info.modified > 0 {
        let text = format!(" ~{}", info.modified);
        stats_width += text.width();
        stat_spans.push((text, rgb(255, 200, 80)));
    }
    if info.staged > 0 {
        let text = format!(" +{}", info.staged);
        stats_width += text.width();
        stat_spans.push((text, rgb(130, 230, 160)));
    }
    if info.untracked > 0 {
        let text = format!(" ?{}", info.untracked);
        stats_width += text.width();
        stat_spans.push((text, rgb(150, 155, 170)));
    }

    let branch_budget = w.saturating_sub(2 + stats_width);
    let branch_display = truncate_smart(&info.branch, branch_budget);
    parts.push(Span::styled(
        branch_display,
        Style::default()
            .fg(rgb(200, 200, 210))
            .add_modifier(Modifier::BOLD),
    ));

    // Reorder to match the stat_spans construction: ahead, behind, modified,
    // staged, untracked (the old code pushed modified/staged/untracked/ahead/behind).
    for (text, color) in stat_spans {
        parts.push(Span::styled(text, Style::default().fg(color)));
    }

    lines.push(Line::from(parts));

    let max_files = inner.height.saturating_sub(lines.len() as u16).min(5) as usize;
    for file in info.dirty_files.iter().take(max_files) {
        let display = truncate_smart(file, w.saturating_sub(4));
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(display, Style::default().fg(rgb(140, 140, 155))),
        ]));
    }
    if info.dirty_files.len() > max_files {
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("+{} more", info.dirty_files.len() - max_files),
                Style::default().fg(rgb(100, 100, 115)),
            ),
        ]));
    }

    lines
}

pub(super) fn render_git_compact(info: &GitInfo, width: u16) -> Vec<Line<'static>> {
    let w = width as usize;
    let mut parts: Vec<Span> = Vec::new();

    // Measure the stats that will actually be pushed, so the branch truncation
    // agrees with the row that gets rendered.
    let mut stats_width = 0usize;
    let mut stat_spans = Vec::new();
    if info.ahead > 0 {
        let text = format!(" ↑{}", info.ahead);
        stats_width += text.width();
        stat_spans.push((text, rgb(100, 200, 100)));
    }
    if info.behind > 0 {
        let text = format!(" ↓{}", info.behind);
        stats_width += text.width();
        stat_spans.push((text, rgb(255, 140, 100)));
    }
    if info.modified > 0 {
        let text = format!(" ~{}", info.modified);
        stats_width += text.width();
        stat_spans.push((text, rgb(240, 200, 80)));
    }
    if info.staged > 0 {
        let text = format!(" +{}", info.staged);
        stats_width += text.width();
        stat_spans.push((text, rgb(100, 200, 100)));
    }
    if info.untracked > 0 {
        let text = format!(" ?{}", info.untracked);
        stats_width += text.width();
        stat_spans.push((text, rgb(140, 140, 150)));
    }

    let icon_width = " ".width();
    let branch_budget = w.saturating_sub(icon_width + stats_width);
    let branch_display = truncate_smart(&info.branch, branch_budget);

    parts.push(Span::styled(" ", Style::default().fg(rgb(240, 160, 60))));
    parts.push(Span::styled(
        branch_display,
        Style::default().fg(rgb(160, 160, 170)),
    ));

    for (text, color) in stat_spans {
        parts.push(Span::styled(text, Style::default().fg(color)));
    }

    vec![Line::from(parts)]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line_text(lines: &[Line<'_>]) -> String {
        lines
            .iter()
            .flat_map(|line| line.spans.iter())
            .map(|span| span.content.as_ref())
            .collect()
    }

    /// `GitInfo` has no `Default` impl, so the helpers spell every field out.
    fn git(branch: &str) -> GitInfo {
        GitInfo {
            branch: branch.to_string(),
            modified: 0,
            staged: 0,
            untracked: 0,
            ahead: 0,
            behind: 0,
            dirty_files: Vec::new(),
        }
    }

    fn info(branch: &str) -> GitInfo {
        GitInfo {
            ahead: 3,
            behind: 2,
            modified: 12,
            staged: 4,
            untracked: 7,
            ..git(branch)
        }
    }

    /// The branch row is drawn into a fixed-width rail; the stats are reserved
    /// before the branch is truncated, so the two must agree on the same
    /// number or the row runs past the panel.
    #[test]
    fn the_compact_row_fits_its_width() {
        for width in 0..=60u16 {
            for branch in [
                "main",
                "feature/a-rather-long-branch-name-that-needs-truncating",
                "日本語のブランチ名",
            ] {
                let text = line_text(&render_git_compact(&info(branch), width));
                assert!(
                    UnicodeWidthStr::width(text.as_str()) <= usize::from(width),
                    "branch {branch:?} at width {width} rendered {} columns: {text:?}",
                    UnicodeWidthStr::width(text.as_str())
                );
            }
        }
    }

    /// A clean repo has no stats to reserve, so the branch gets the whole row.
    #[test]
    fn a_clean_repo_gives_the_branch_the_whole_row() {
        let text = line_text(&render_git_compact(&git("main"), 40));
        assert!(text.contains("main"));
        assert!(UnicodeWidthStr::width(text.as_str()) <= 40);
    }

    /// Every stat that was counted must also be rendered — the two used to be
    /// written out separately and could drift.
    #[test]
    fn every_counted_stat_is_rendered() {
        let text = line_text(&render_git_compact(&info("main"), 60));
        for expected in ["↑3", "↓2", "~12", "+4", "?7"] {
            assert!(text.contains(expected), "{expected} missing from {text:?}");
        }
    }

    #[test]
    fn stats_that_are_zero_are_not_rendered() {
        let clean = GitInfo {
            modified: 1,
            ..git("main")
        };
        let text = line_text(&render_git_compact(&clean, 40));
        assert!(text.contains("~1"));
        for absent in ["↑", "↓", "+", "?"] {
            assert!(!text.contains(absent), "{absent} should not render: {text:?}");
        }
    }
}

