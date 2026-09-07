//! Render helpers for the model browser — Phase 2c of the TUI 100x plan.
//!
//! Provides:
//! - 3-column layout renderer (facets | rows | detail)
//! - 1..9 hotkey jump for the visible rows
//! - Tab to cycle sort modes
//! - Facet chip layout with counts and active markers
//!
//! # Layout
//!
//! ```text
//!   keys: Tab sort | 1-9 jump | f facet | Ctrl+O default | Ctrl+N favorite | Esc close
//!   \u{250c}\u{2500} /model \u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2510}
//!   \u{2502} FACETS       \u{2502} MODELS                       \u{2502} DETAIL                            \u{2502}
//!   \u{2502} \u{25cb} Anthropic  \u{2502} 1. claude-opus-4-6  \u{2190}current\u{2502} Provider   Anthropic              \u{2502}
//!   \u{2502} \u{25cb} OpenAI     \u{2502} 2. claude-sonnet-4-5       \u{2502} Auth       OAuth                   \u{2502}
//!   \u{2502} \u{25cb} Gemini     \u{2502} 3. gpt-5.5                 \u{2502} Context    200k                    \u{2502}
//!   \u{2502} \u{25cb} OpenRouter \u{2502} 4. gpt-5.4-mini   [fast]   \u{2502} $/M        $3 / $15                \u{2502}
//!   \u{2502} \u{25cb} ...        \u{2502} ...                        \u{2502} Tools      \u{2713} Vision \u{2713} Reasoning \u{2502}
//!   \u{2514}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2500}\u{2518}
//!   Sort: For you \u{2022} 12 models \u{2022} type / to search
//! ```

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use crossterm::event::KeyCode;

#[allow(unused_imports)]
use crate::alphacode_tui::tui::model_browser::{
    BrowserRow, Capability, FacetState, ModelBrowserState, ModelTier, SortMode,
};
use crate::alphacode_tui::tui::RouteDetailSeverity;

/// A column-key event produced by the picker when the user types 1..9.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserKey {
    /// Move cursor to the n-th visible row (clamped).
    Jump(usize),
    /// Cycle to the next sort mode.
    CycleSort,
    /// Toggle the provider chip with the given label (1..n).
    ToggleProvider(usize),
    /// Toggle the tier chip with the given index.
    ToggleTier(usize),
    /// Toggle the capability chip with the given index.
    ToggleCapability(usize),
    /// Clear every facet.
    ClearFacets,
}

/// Top hotkey hint printed above the browser box.
pub fn top_hint_line() -> Line<'static> {
    Line::from(vec![
        Span::styled(
            " ",
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(
            "Tab",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" sort  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "1-9",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" jump  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "p",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" provider  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "t",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" tier  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "c",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" capability  ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            "Esc",
            Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
        ),
        Span::styled(" close", Style::default().fg(Color::DarkGray)),
    ])
}

/// Bottom status hint that summarises the visible count and sort mode.
pub fn bottom_hint_line(state: &ModelBrowserState) -> Line<'static> {
    let count = state.filtered.len();
    let total = state.rows.len();
    let mut spans = vec![Span::styled(
        format!(" Sort: {} ", state.sort.label()),
        Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )];
    spans.push(Span::styled(
        format!("\u{2022} {count}/{total} models  "),
        Style::default().fg(Color::Gray),
    ));
    if state.loading.is_some() {
        spans.push(Span::styled(
            "\u{2022} loading\u{2026}",
            Style::default().fg(Color::Yellow),
        ));
    } else if state.facets.active_count() > 0 {
        spans.push(Span::styled(
            format!("\u{2022} {} filter(s) ", state.facets.active_count()),
            Style::default().fg(Color::Magenta),
        ));
    }
    Line::from(spans)
}

/// Render the facets column.
pub fn render_facets(
    state: &ModelBrowserState,
    providers: &[String],
    area: Rect,
    buf: &mut Buffer,
) {
    let items: Vec<ListItem> = providers
        .iter()
        .take(area.height as usize)
        .enumerate()
        .map(|(idx, p)| {
            let count = state
                .rows
                .iter()
                .filter(|r| r.provider == *p)
                .count();
            let active = state.facets.providers.contains(p);
            let marker = if active { "\u{25cf}" } else { "\u{25cb}" };
            let color = if active {
                Color::Magenta
            } else {
                Color::DarkGray
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("{} ", marker),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{:>2}. ", idx + 1),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(p.clone(), Style::default().fg(Color::White)),
                Span::styled(
                    format!("  ({count})"),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(" PROVIDER ", Style::default().fg(Color::Cyan)));
    let list = List::new(items).block(block);
    ratatui::prelude::Widget::render(list, area, buf);
}

/// Render the models column.
pub fn render_models(
    state: &ModelBrowserState,
    area: Rect,
    buf: &mut Buffer,
) {
    let height = area.height as usize;
    let start = state.selected.saturating_sub(height / 2);
    let end = (start + height).min(state.filtered.len());

    let items: Vec<ListItem> = state.filtered[start..end]
        .iter()
        .enumerate()
        .map(|(idx, &row_idx)| {
            let row = &state.rows[row_idx];
            let absolute = start + idx;
            let is_selected = absolute == state.selected;
            let n = absolute + 1;
            let num_pad = format!("{:>3}. ", n.min(999));

            let mut spans = vec![
                Span::styled(
                    num_pad,
                    if is_selected {
                        Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
                Span::styled(
                    row.pretty_name().to_string(),
                    if is_selected {
                        Style::default()
                            .fg(Color::Cyan)
                            .add_modifier(Modifier::BOLD)
                    } else if row.is_favorite {
                        Style::default().fg(Color::Magenta)
                    } else {
                        Style::default().fg(Color::White)
                    },
                ),
            ];
            if row.is_current {
                spans.push(Span::styled(
                    "  \u{2190}current",
                    Style::default().fg(Color::Green).add_modifier(Modifier::BOLD),
                ));
            }
            if row.is_default {
                spans.push(Span::styled(
                    "  default",
                    Style::default().fg(Color::Cyan).add_modifier(Modifier::ITALIC),
                ));
            }
            let tier_label = format!(" [{}]", row.tier.label());
            spans.push(Span::styled(
                tier_label,
                Style::default().fg(tier_color(row.tier)),
            ));
            ListItem::new(Line::from(spans))
        })
        .collect();

    let block = Block::default()
        .borders(Borders::RIGHT)
        .border_style(Style::default().fg(Color::DarkGray))
        .title(Span::styled(
            format!(" MODELS ({}) ", state.filtered.len()),
            Style::default().fg(Color::Cyan),
        ));

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(35, 40, 55))
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("\u{25b8} ");
    let mut state_for_list = ListState::default().with_selected(Some(
        state.selected.saturating_sub(start),
    ));
    StatefulWidget::render(list, area, buf, &mut state_for_list);
}

/// Render the detail column for the currently-selected row.
pub fn render_detail(
    state: &ModelBrowserState,
    area: Rect,
    buf: &mut Buffer,
) {
    let mut lines: Vec<Line> = Vec::new();
    if let Some(row) = state.selected_row() {
        lines.push(Line::from(Span::styled(
            row.pretty_name().to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));
        lines.push(detail_row("Provider", &row.provider, Color::White));
        lines.push(detail_row("Auth", &row.api_method, Color::White));
        if let Some(c) = row.context_window {
            lines.push(detail_row(
                "Context",
                &format!("{}k tokens", c / 1000),
                Color::White,
            ));
        }
        if let Some(c) = row.cost_in_micros {
            let dollars = c as f64 / 1_000_000.0;
            lines.push(detail_row(
                "$/M in/out",
                &format!("${:.2}", dollars),
                if dollars < 1.0 { Color::Green } else { Color::White },
            ));
        }
        if !row.capabilities.is_empty() {
            let caps: Vec<&str> = row.capabilities.iter().map(|c| c.label()).collect();
            lines.push(detail_row("Caps", &caps.join(" \u{2022} "), Color::White));
        }
        if let Some(detail) = &row.detail {
            lines.push(Line::from(""));
            let style = match row.detail_severity {
                RouteDetailSeverity::Unavailable => {
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
                }
                RouteDetailSeverity::Warn => Style::default().fg(Color::Yellow),
                RouteDetailSeverity::Info => Style::default().fg(Color::Cyan),
                RouteDetailSeverity::None => Style::default().fg(Color::Gray),
            };
            lines.push(Line::from(Span::styled(detail.clone(), style)));
        }
    } else {
        lines.push(Line::from(Span::styled(
            "(no selection)",
            Style::default().fg(Color::DarkGray),
        )));
    }
    let para = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::NONE)
            .title(Span::styled(" DETAIL ", Style::default().fg(Color::Cyan))),
    );
    para.render(area, buf);
}

fn detail_row(label: &str, value: &str, color: Color) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{:<12} ", label),
            Style::default().fg(Color::DarkGray),
        ),
        Span::styled(value.to_string(), Style::default().fg(color)),
    ])
}

fn tier_color(tier: ModelTier) -> Color {
    match tier {
        ModelTier::Free => Color::Green,
        ModelTier::Fast => Color::Yellow,
        ModelTier::Standard => Color::White,
        ModelTier::Premium => Color::Magenta,
    }
}

/// Compute the visible provider list, sorted by count descending.
pub fn visible_providers(state: &ModelBrowserState) -> Vec<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for r in &state.rows {
        let entry = counts.iter_mut().find(|(p, _)| p == &r.provider);
        match entry {
            Some((_, c)) => *c += 1,
            None => counts.push((r.provider.clone(), 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    counts.into_iter().map(|(p, _)| p).collect()
}

/// Render the entire browser into a single area. Caller is responsible for
/// surrounding the browser with its hint lines.
pub fn render_browser(state: &ModelBrowserState, area: Rect, buf: &mut Buffer) {
    if area.width < 30 || area.height < 5 {
        return;
    }
    let providers = visible_providers(state);
    let facets_w = (area.width / 4).clamp(14, 22);
    let detail_w = (area.width / 3).clamp(20, 36);
    let models_w = area.width.saturating_sub(facets_w + detail_w);

    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(facets_w),
            Constraint::Length(models_w),
            Constraint::Length(detail_w),
        ])
        .split(area);

    render_facets(state, &providers, chunks[0], buf);
    render_models(state, chunks[1], buf);
    render_detail(state, chunks[2], buf);
}

/// Map a key to a `BrowserKey`. Caller is expected to filter for key events.
pub fn map_key(code: KeyCode) -> Option<BrowserKey> {
    match code {
        KeyCode::Tab => Some(BrowserKey::CycleSort),
        KeyCode::Esc => Some(BrowserKey::ClearFacets),
        KeyCode::Char(c @ '1'..='9') => {
            let n = (c as u8 - b'0') as usize;
            Some(BrowserKey::Jump(n))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_tui::tui::model_browser::ModelBrowserState;
    use crate::alphacode_tui::tui::PickerOption;
    use ratatui::backend::TestBackend;
    use std::collections::HashSet;

    fn opt(p: &str) -> PickerOption {
        PickerOption::new(p.to_string(), "x".into(), true, String::new(), Some(3000))
    }

    fn row_with(name: &str, provider: &str, tier: ModelTier) -> BrowserRow {
        BrowserRow {
            model: name.into(),
            provider: provider.into(),
            api_method: "x".into(),
            available: true,
            tier,
            capabilities: vec![Capability::Tools],
            context_window: Some(200_000),
            cost_in_micros: Some(3_000_000),
            cost_out_micros: Some(15_000_000),
            avg_response_ms: 0,
            usage_count: 0,
            last_used_secs: None,
            is_favorite: false,
            is_current: false,
            is_default: false,
            created_date: None,
            detail: None,
            detail_severity: RouteDetailSeverity::None,
            score_for_you: 0.0,
            score_newest: 0.0,
            score_cheapest: 0.0,
            score_fastest: 0.0,
        }
    }

    #[test]
    fn top_hint_includes_shortcuts() {
        let line = top_hint_line();
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("Tab"));
        assert!(text.contains("1-9"));
        assert!(text.contains("Esc"));
    }

    #[test]
    fn bottom_hint_reports_count() {
        let state = ModelBrowserState::new(vec![row_with("a", "X", ModelTier::Standard)]);
        let line = bottom_hint_line(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("1/1"));
    }

    #[test]
    fn bottom_hint_reports_loading() {
        let mut state = ModelBrowserState::skeleton();
        state.loading = Some(8);
        let line = bottom_hint_line(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("loading"));
    }

    #[test]
    fn bottom_hint_reports_filters() {
        let mut state = ModelBrowserState::new(vec![row_with("a", "X", ModelTier::Standard)]);
        state.facets.providers.insert("X".into());
        let line = bottom_hint_line(&state);
        let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("filter"));
    }

    #[test]
    fn visible_providers_dedups() {
        let rows = vec![
            row_with("a", "Anthropic", ModelTier::Standard),
            row_with("b", "Anthropic", ModelTier::Standard),
            row_with("c", "OpenAI", ModelTier::Standard),
        ];
        let state = ModelBrowserState::new(rows);
        let providers = visible_providers(&state);
        assert_eq!(providers.len(), 2);
    }

    #[test]
    fn map_key_handles_digits() {
        assert!(matches!(map_key(KeyCode::Char('1')), Some(BrowserKey::Jump(1))));
        assert!(matches!(map_key(KeyCode::Char('9')), Some(BrowserKey::Jump(9))));
        assert!(map_key(KeyCode::Char('0')).is_none());
    }

    #[test]
    fn map_key_handles_specials() {
        assert!(matches!(map_key(KeyCode::Tab), Some(BrowserKey::CycleSort)));
        assert!(matches!(map_key(KeyCode::Esc), Some(BrowserKey::ClearFacets)));
    }

    #[test]
    fn render_browser_paints_three_columns() {
        let backend = TestBackend::new(120, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = ModelBrowserState::new(vec![
            row_with("a", "Anthropic", ModelTier::Premium),
            row_with("b", "OpenAI", ModelTier::Standard),
        ]);
        terminal
            .draw(|f| {
                let area = f.area();
                render_browser(&state, area, f.buffer_mut());
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                s.push_str(buf[(x, y)].symbol());
            }
        }
        assert!(s.contains("Anthropic"));
        assert!(s.contains("OpenAI"));
    }

    #[test]
    fn render_browser_handles_narrow_terminal() {
        let backend = TestBackend::new(20, 5);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = ModelBrowserState::new(vec![row_with("a", "X", ModelTier::Standard)]);
        terminal
            .draw(|f| {
                render_browser(&state, f.area(), f.buffer_mut());
            })
            .unwrap();
        // No assertion needed; just check it doesn't panic.
    }

    #[test]
    fn tier_color_distinguishes_tiers() {
        assert_ne!(tier_color(ModelTier::Free), tier_color(ModelTier::Premium));
    }

    #[test]
    fn empty_state_does_not_panic() {
        let backend = TestBackend::new(80, 12);
        let mut terminal = Terminal::new(backend).unwrap();
        let state = ModelBrowserState::new(vec![]);
        terminal
            .draw(|f| {
                render_browser(&state, f.area(), f.buffer_mut());
            })
            .unwrap();
    }

    #[test]
    fn detail_panel_shows_cost() {
        let row = row_with("a", "Anthropic", ModelTier::Premium);
        let mut state = ModelBrowserState::new(vec![row]);
        state.selected = 0;
        state.recompute_filtered();
        let backend = TestBackend::new(80, 16);
        let mut terminal = Terminal::new(backend).unwrap();
        terminal
            .draw(|f| {
                render_browser(&state, f.area(), f.buffer_mut());
            })
            .unwrap();
        let buf = terminal.backend().buffer().clone();
        let mut s = String::new();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                s.push_str(buf[(x, y)].symbol());
            }
        }
        assert!(s.contains("Context"));
        assert!(s.contains("$"));
    }

    #[test]
    fn facet_state_helpers() {
        let mut facets = FacetState::default();
        assert!(facets.is_empty());
        facets.providers.insert("X".into());
        facets.text = "cl".into();
        assert_eq!(facets.active_count(), 2);
        facets.clear();
        assert!(facets.is_empty());
    }

    #[test]
    fn sort_label_includes_all_modes() {
        for mode in [SortMode::ForYou, SortMode::Newest, SortMode::Cheapest, SortMode::Fastest, SortMode::Alpha] {
            assert!(!mode.label().is_empty());
        }
    }
}