use anyhow::Result;
use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::collections::HashMap;

use crate::alphacode_tui_account_picker::{
    AccountPickerCommand, AccountPickerItem, AccountPickerSummary, AccountProviderKind,
};

#[path = "file_browser.rs"]
mod file_browser;
#[path = "overlay_render.rs"]
mod render_support;
use render_support::{
    ActionSection, account_count_summary, account_is_active, action_icon, action_kind_badge,
    action_kind_help, action_section, centered_rect, command_preview, compact_item_title, hotkey,
    metric_span, provider_header_line, provider_style, truncate_with_ellipsis,
};

fn panel_bg() -> Color {
    crate::alphacode_tui_style::role_color(crate::alphacode_tui_style::Role::CodeBg)
}
fn panel_border() -> Color {
    crate::alphacode_tui_style::role_color(crate::alphacode_tui_style::Role::Border)
}
fn panel_border_active() -> Color {
    crate::alphacode_tui_style::role_color(crate::alphacode_tui_style::Role::Accent)
}
fn section_border() -> Color {
    crate::alphacode_tui_style::role_color(crate::alphacode_tui_style::Role::PanelBorderMuted)
}
fn selected_bg() -> Color {
    crate::alphacode_tui_style::role_color(crate::alphacode_tui_style::Role::SelectionBg)
}
fn muted() -> Color {
    crate::alphacode_tui_style::role_color(crate::alphacode_tui_style::Role::MutedText)
}
fn muted_dark() -> Color {
    crate::alphacode_tui_style::role_color(crate::alphacode_tui_style::Role::Dim)
}
const OVERLAY_PERCENT_X: u16 = 88;
const OVERLAY_PERCENT_Y: u16 = 74;

#[derive(Debug, Clone)]
pub struct AccountPicker {
    title: String,
    items: Vec<AccountPickerItem>,
    filtered: Vec<usize>,
    ordered: Vec<usize>,
    search_text: Vec<String>,
    file_browser: Option<file_browser::FileBrowser>,
    selected: usize,
    filter: String,
    summary: Option<AccountPickerSummary>,
    last_action_list_area: Option<Rect>,
}

pub enum OverlayAction {
    Continue,
    Close,
    Execute(AccountPickerCommand),
}

impl AccountPicker {
    /// Keyboard, filtering and layout for non-account action lists.
    pub fn action_list(title: impl Into<String>, items: Vec<AccountPickerItem>) -> Self {
        let mut picker = Self::new(title, items);
        picker.summary = None;
        picker
    }
    pub fn new(title: impl Into<String>, items: Vec<AccountPickerItem>) -> Self {
        Self::with_summary(title, items, AccountPickerSummary::default())
    }

    pub fn debug_memory_profile(&self) -> serde_json::Value {
        let items_estimate_bytes: usize = self.items.iter().map(estimate_item_bytes).sum();
        let filtered_estimate_bytes = self.filtered.capacity() * std::mem::size_of::<usize>();
        let filter_bytes = self.filter.capacity();
        let title_bytes = self.title.capacity();
        let summary_estimate_bytes = self
            .summary
            .as_ref()
            .map(estimate_summary_bytes)
            .unwrap_or(0);
        let total_estimate_bytes = items_estimate_bytes
            + filtered_estimate_bytes
            + filter_bytes
            + title_bytes
            + summary_estimate_bytes;

        serde_json::json!({
            "items_count": self.items.len(),
            "filtered_count": self.filtered.len(),
            "selected": self.selected,
            "title_bytes": title_bytes,
            "filter_bytes": filter_bytes,
            "summary_estimate_bytes": summary_estimate_bytes,
            "items_estimate_bytes": items_estimate_bytes,
            "filtered_estimate_bytes": filtered_estimate_bytes,
            "total_estimate_bytes": total_estimate_bytes,
        })
    }

    pub fn with_summary(
        title: impl Into<String>,
        items: Vec<AccountPickerItem>,
        summary: AccountPickerSummary,
    ) -> Self {
        let mut picker = Self {
            title: title.into(),
            items,
            filtered: Vec::new(),
            ordered: Vec::new(),
            search_text: Vec::new(),
            file_browser: None,
            selected: 0,
            filter: String::new(),
            summary: Some(summary),
            last_action_list_area: None,
        };
        picker.build_search_index();
        picker.apply_filter();
        picker
    }

    fn selected_item(&self) -> Option<&AccountPickerItem> {
        self.filtered
            .get(self.selected)
            .and_then(|idx| self.items.get(*idx))
    }

    fn visible_window_start(&self, available_items: usize) -> usize {
        self.selected
            .saturating_sub(available_items.saturating_sub(1).min(available_items / 2))
    }

    fn visible_index_for_action_row(&self, row: u16, list_height: u16) -> Option<usize> {
        if self.filtered.is_empty() {
            return None;
        }

        let available_items = (list_height as usize).max(1);
        if self.file_browser.is_some() {
            let index = self.visible_window_start(available_items) + row as usize;
            return (index < self.filtered.len()).then_some(index);
        }
        let start = self.visible_window_start(available_items);
        let end = (start + available_items).min(self.filtered.len());
        let mut current_provider: Option<&str> = None;
        let mut rendered_row = 0u16;

        for visible_idx in start..end {
            let item = &self.items[self.filtered[visible_idx]];
            if current_provider != Some(item.provider_id.as_str()) {
                current_provider = Some(item.provider_id.as_str());
                if rendered_row == row {
                    return None;
                }
                rendered_row = rendered_row.saturating_add(1);
                if rendered_row >= list_height {
                    return None;
                }
            }

            if rendered_row == row {
                return Some(visible_idx);
            }
            rendered_row = rendered_row.saturating_add(1);
            if rendered_row > row {
                return None;
            }
        }

        None
    }

    fn build_search_index(&mut self) {
        if self.file_browser.is_some() {
            // Directory entries already arrive in folder/name order. Search
            // filenames only; generic labels such as "Import skill" must not
            // make every file match an otherwise unrelated query.
            self.search_text = self
                .items
                .iter()
                .map(|item| item.title.to_lowercase())
                .collect();
            self.ordered = (0..self.items.len()).collect();
            return;
        }
        self.search_text = self
            .items
            .iter()
            .map(|item| {
                format!(
                    "{} {} {} {} {}",
                    item.provider_id,
                    item.provider_label,
                    item.title,
                    item.subtitle,
                    crate::alphacode_tui_account_picker::action_kind_label(&item.command)
                )
                .to_lowercase()
            })
            .collect();
        self.ordered = (0..self.items.len()).collect();
        let provider_order = self.provider_order();
        self.ordered.sort_by(|left, right| {
            let a = &self.items[*left];
            let b = &self.items[*right];
            provider_order
                .get(&a.provider_id)
                .cmp(&provider_order.get(&b.provider_id))
                .then_with(|| action_section(a).cmp(&action_section(b)))
                .then_with(|| a.title.cmp(&b.title))
                .then_with(|| left.cmp(right))
        });
    }

    fn apply_filter(&mut self) {
        let filter = self.filter.to_lowercase();
        let needles: Vec<_> = filter.split_whitespace().collect();
        self.filtered.clear();
        self.filtered
            .extend(self.ordered.iter().copied().filter(|idx| {
                needles
                    .iter()
                    .all(|needle| self.search_text[*idx].contains(needle))
            }));
        if self.selected >= self.filtered.len() {
            self.selected = self.filtered.len().saturating_sub(1);
        }
    }

    fn provider_order(&self) -> HashMap<String, usize> {
        let mut order = HashMap::new();
        let mut next = 0usize;
        for item in &self.items {
            if order.contains_key(&item.provider_id) {
                continue;
            }
            let rank = if item.provider_id == "defaults" {
                usize::MAX / 2
            } else {
                let current = next;
                next += 1;
                current
            };
            order.insert(item.provider_id.clone(), rank);
        }
        order
    }

    fn filtered_provider_switch_count(&self, provider_id: &str) -> usize {
        self.filtered
            .iter()
            .filter(|idx| {
                let item = &self.items[**idx];
                item.provider_id == provider_id
                    && matches!(action_section(item), ActionSection::Switch)
            })
            .count()
    }

    fn filtered_provider_secondary_count(&self, provider_id: &str) -> usize {
        self.filtered
            .iter()
            .filter(|idx| {
                let item = &self.items[**idx];
                item.provider_id == provider_id
                    && !matches!(action_section(item), ActionSection::Switch)
            })
            .count()
    }

    fn select_prev_provider_group(&mut self) {
        let Some(current_idx) = self.filtered.get(self.selected).copied() else {
            return;
        };
        let current_provider = self.items[current_idx].provider_id.as_str();
        let mut target = None;

        for pos in (0..self.selected).rev() {
            let provider_id = self.items[self.filtered[pos]].provider_id.as_str();
            if provider_id != current_provider {
                target = Some(pos);
                break;
            }
        }

        let Some(mut pos) = target else {
            return;
        };
        let provider_id = self.items[self.filtered[pos]].provider_id.clone();
        while pos > 0 && self.items[self.filtered[pos - 1]].provider_id == provider_id {
            pos -= 1;
        }
        self.selected = pos;
    }

    fn select_next_provider_group(&mut self) {
        let Some(current_idx) = self.filtered.get(self.selected).copied() else {
            return;
        };
        let current_provider = self.items[current_idx].provider_id.as_str();

        for pos in (self.selected + 1)..self.filtered.len() {
            let provider_id = self.items[self.filtered[pos]].provider_id.as_str();
            if provider_id != current_provider {
                self.selected = pos;
                break;
            }
        }
    }

    fn provider_overview_line(&self) -> Line<'static> {
        if self.summary.is_none() {
            return Line::from("Any filename is supported; contents must define a skill.");
        }
        let mut seen = Vec::new();
        let mut stats: HashMap<String, (String, usize, usize)> = HashMap::new();

        for item in &self.items {
            if matches!(item.provider_id.as_str(), "defaults" | "account-flow") {
                continue;
            }
            if !stats.contains_key(&item.provider_id) {
                seen.push(item.provider_id.clone());
                stats.insert(
                    item.provider_id.clone(),
                    (item.provider_label.clone(), 0, 0),
                );
            }
            if let Some((_, accounts, actions)) = stats.get_mut(&item.provider_id) {
                if matches!(action_section(item), ActionSection::Switch) {
                    *accounts += 1;
                } else {
                    *actions += 1;
                }
            }
        }

        let mut spans = vec![Span::styled(
            "Providers ",
            Style::default().fg(muted_dark()),
        )];
        let mut first = true;
        for provider_id in seen {
            let Some((label, accounts, actions)) = stats.get(&provider_id) else {
                continue;
            };
            if !first {
                spans.push(Span::styled(" | ", Style::default().fg(muted_dark())));
            }
            first = false;
            let summary = if *accounts > 0 {
                format!("{} {}", label, account_count_summary(*accounts))
            } else {
                format!(
                    "{} {} control{}",
                    label,
                    actions,
                    if *actions == 1 { "" } else { "s" }
                )
            };
            spans.push(Span::styled(summary, provider_style(&provider_id)));
        }
        if first {
            spans.push(Span::styled(
                "No providers available",
                Style::default().fg(muted()),
            ));
        }
        Line::from(spans)
    }

    pub fn handle_overlay_key(
        &mut self,
        code: KeyCode,
        modifiers: KeyModifiers,
    ) -> Result<OverlayAction> {
        match code {
            KeyCode::Esc => {
                if !self.filter.is_empty() {
                    self.filter.clear();
                    self.apply_filter();
                    return Ok(OverlayAction::Continue);
                }
                return Ok(OverlayAction::Close);
            }
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                return Ok(OverlayAction::Close);
            }
            KeyCode::Up => {
                self.selected = self.selected.saturating_sub(1);
            }
            KeyCode::Down => {
                let max = self.filtered.len().saturating_sub(1);
                self.selected = (self.selected + 1).min(max);
            }
            KeyCode::Left => {
                self.select_prev_provider_group();
            }
            KeyCode::Right => {
                self.select_next_provider_group();
            }
            KeyCode::PageUp => {
                self.selected = self.selected.saturating_sub(6);
            }
            KeyCode::PageDown => {
                let max = self.filtered.len().saturating_sub(1);
                self.selected = (self.selected + 6).min(max);
            }
            KeyCode::Home => {
                self.selected = 0;
            }
            KeyCode::End => {
                self.selected = self.filtered.len().saturating_sub(1);
            }
            KeyCode::Backspace => {
                if let Some((index, _)) =
                    unicode_segmentation::UnicodeSegmentation::grapheme_indices(
                        self.filter.as_str(),
                        true,
                    )
                    .next_back()
                {
                    self.filter.truncate(index);
                    self.apply_filter();
                }
            }
            KeyCode::Enter => {
                if let Some(item) = self.selected_item() {
                    return Ok(OverlayAction::Execute(item.command.clone()));
                }
                return Ok(OverlayAction::Continue);
            }
            KeyCode::Char(c)
                if !modifiers.contains(KeyModifiers::CONTROL)
                    && !modifiers.contains(KeyModifiers::ALT) =>
            {
                self.filter.push(c);
                self.apply_filter();
            }
            _ => {}
        }
        Ok(OverlayAction::Continue)
    }

    pub fn handle_overlay_mouse(&mut self, mouse: MouseEvent) {
        let Some(list_inner) = self.last_action_list_area else {
            return;
        };
        let inside_list = mouse.column >= list_inner.x
            && mouse.column < list_inner.x.saturating_add(list_inner.width)
            && mouse.row >= list_inner.y
            && mouse.row < list_inner.y.saturating_add(list_inner.height);

        match mouse.kind {
            MouseEventKind::ScrollUp if inside_list => {
                self.selected = self.selected.saturating_sub(1);
            }
            MouseEventKind::ScrollDown if inside_list => {
                let max = self.filtered.len().saturating_sub(1);
                self.selected = (self.selected + 1).min(max);
            }
            MouseEventKind::Down(MouseButton::Left) if inside_list => {
                let row = mouse.row.saturating_sub(list_inner.y);
                if let Some(visible_idx) = self.visible_index_for_action_row(row, list_inner.height)
                {
                    self.selected = visible_idx;
                }
            }
            _ => {}
        }
    }

    pub fn render(&mut self, frame: &mut Frame) {
        if self.file_browser.is_some() {
            self.render_file_browser(frame);
            return;
        }
        let area = centered_rect(OVERLAY_PERCENT_X, OVERLAY_PERCENT_Y, frame.area());

        let block = Block::default()
            .title(format!(" {} ", self.title))
            .title_bottom(Line::from(vec![
                hotkey(" Enter "),
                Span::styled(" run  ", Style::default().fg(muted_dark())),
                hotkey(" Up/Down "),
                Span::styled(" navigate  ", Style::default().fg(muted_dark())),
                hotkey(" Click "),
                Span::styled(" select  ", Style::default().fg(muted_dark())),
                hotkey(" type "),
                Span::styled(" filter  ", Style::default().fg(muted_dark())),
                hotkey(" Esc "),
                Span::styled(" clear / close ", Style::default().fg(muted_dark())),
            ]))
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(panel_border()));
        frame.render_widget(block, area);

        let inner = Rect {
            x: area.x + 1,
            y: area.y + 1,
            width: area.width.saturating_sub(2),
            height: area.height.saturating_sub(2),
        };
        let header_height = if inner.height < 20 { 3 } else { 7 };
        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(header_height),
                Constraint::Min(6),
                Constraint::Length(1),
            ])
            .split(inner);

        self.render_header(frame, rows[0]);

        let narrow = rows[1].width < 72;
        let body = Layout::default()
            .direction(if narrow {
                Direction::Vertical
            } else {
                Direction::Horizontal
            })
            .constraints(if narrow {
                [Constraint::Percentage(55), Constraint::Percentage(45)]
            } else {
                [Constraint::Percentage(58), Constraint::Percentage(42)]
            })
            .split(rows[1]);

        self.render_action_list(frame, body[0]);
        self.render_detail_pane(frame, body[1]);

        let footer = Paragraph::new(Line::from(vec![
            Span::styled("Focus ", Style::default().fg(muted_dark())),
            Span::styled(
                if self.summary.is_none() {
                    "Select a file to import or a folder to browse. Esc closes without importing."
                } else {
                    "saved accounts stay surfaced here; click actions to focus them, use Left/Right to jump provider groups, or use `/account <provider> settings` for the full text view."
                },
                Style::default().fg(muted()),
            ),
        ]));
        frame.render_widget(footer, rows[2]);
    }

    fn render_header(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default()
            .title(Span::styled(
                " Overview ",
                Style::default()
                    .fg(crate::alphacode_tui_style::role_color(
                        crate::alphacode_tui_style::Role::AiText,
                    ))
                    .bold(),
            ))
            .borders(Borders::ALL)
            .style(Style::default().bg(panel_bg()))
            .border_style(Style::default().fg(section_border()));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let lines = vec![
            Line::from(vec![
                Span::styled("Filter ", Style::default().fg(muted_dark())),
                Span::styled(
                    if self.filter.is_empty() {
                        if self.summary.is_none() {
                            "type to filter files"
                        } else {
                            "type provider or account name"
                        }
                        .to_string()
                    } else {
                        self.filter.clone()
                    },
                    if self.filter.is_empty() {
                        Style::default()
                            .fg(crate::alphacode_tui_style::role_color(
                                crate::alphacode_tui_style::Role::MutedText,
                            ))
                            .italic()
                    } else {
                        Style::default().fg(crate::alphacode_tui_style::role_color(
                            crate::alphacode_tui_style::Role::AiText,
                        ))
                    },
                ),
                Span::styled(
                    format!("  -  {} results", self.filtered.len()),
                    Style::default().fg(muted_dark()),
                ),
            ]),
            self.provider_overview_line(),
            self.summary_line(),
            self.defaults_line(),
        ];

        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }

    fn render_action_list(&mut self, frame: &mut Frame, area: Rect) {
        let title = if self.filtered.is_empty() {
            if self.summary.is_none() {
                " Files & Folders "
            } else {
                " Providers & Quick Actions "
            }
            .to_string()
        } else {
            format!(" Actions ({}/{}) ", self.selected + 1, self.filtered.len())
        };
        let block = Block::default()
            .title(Span::styled(
                title,
                Style::default()
                    .fg(crate::alphacode_tui_style::role_color(
                        crate::alphacode_tui_style::Role::AiText,
                    ))
                    .bold(),
            ))
            .borders(Borders::ALL)
            .style(Style::default().bg(panel_bg()))
            .border_style(Style::default().fg(panel_border_active()));
        let list_inner = block.inner(area);
        frame.render_widget(block, area);
        self.last_action_list_area = Some(list_inner);

        let available_items = (list_inner.height as usize).max(1);
        let start = self.visible_window_start(available_items);
        let end = (start + available_items).min(self.filtered.len());

        let mut lines = Vec::new();
        if self.filtered.is_empty() {
            lines.push(Line::from(Span::styled(
                if self.summary.is_none() {
                    "No matching files or folders."
                } else {
                    "No matching account or provider actions."
                },
                Style::default()
                    .fg(crate::alphacode_tui_style::role_color(
                        crate::alphacode_tui_style::Role::MutedText,
                    ))
                    .italic(),
            )));
            lines.push(Line::from(Span::styled(
                if self.summary.is_none() {
                    "Clear the filter or browse another directory."
                } else {
                    "Try `openai`, `claude`, an account label, `login`, or `default`."
                },
                Style::default().fg(muted()),
            )));
        } else {
            let mut current_provider: Option<&str> = None;
            for visible_idx in start..end {
                let idx = self.filtered[visible_idx];
                let item = &self.items[idx];
                let selected = visible_idx == self.selected;

                if current_provider != Some(item.provider_id.as_str()) {
                    current_provider = Some(item.provider_id.as_str());
                    lines.push(provider_header_line(
                        &item.provider_label,
                        self.filtered_provider_switch_count(&item.provider_id),
                        self.filtered_provider_secondary_count(&item.provider_id),
                        &item.provider_id,
                    ));
                }

                let row_style = if selected {
                    Style::default().bg(selected_bg())
                } else {
                    Style::default()
                };
                let (icon, icon_color) = action_icon(item);
                let title = compact_item_title(item);
                let meta_width = list_inner.width.saturating_sub(16) as usize;
                let meta = truncate_with_ellipsis(&item.subtitle, meta_width);
                lines.push(Line::from(vec![
                    Span::styled(
                        if selected { "> " } else { "  " },
                        row_style.fg(crate::alphacode_tui_style::role_color(
                            crate::alphacode_tui_style::Role::AiText,
                        )),
                    ),
                    Span::styled(format!("{} ", icon), row_style.fg(icon_color).bold()),
                    Span::styled(
                        truncate_with_ellipsis(&title, 22),
                        row_style.fg(crate::alphacode_tui_style::role_color(
                            crate::alphacode_tui_style::Role::AiText,
                        )),
                    ),
                    Span::styled(" - ", row_style.fg(muted_dark())),
                    Span::styled(meta, row_style.fg(muted())),
                ]));
            }
        }

        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), list_inner);
    }

    fn render_detail_pane(&self, frame: &mut Frame, area: Rect) {
        let title = self
            .selected_item()
            .map(|item| format!(" {} ", item.provider_label))
            .unwrap_or_else(|| " Details ".to_string());
        let block = Block::default()
            .title(Span::styled(
                title,
                Style::default()
                    .fg(crate::alphacode_tui_style::role_color(
                        crate::alphacode_tui_style::Role::AiText,
                    ))
                    .bold(),
            ))
            .borders(Borders::ALL)
            .style(Style::default().bg(panel_bg()))
            .border_style(Style::default().fg(section_border()));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let Some(item) = self.selected_item() else {
            frame.render_widget(
                Paragraph::new("No action selected").style(Style::default().fg(
                    crate::alphacode_tui_style::role_color(crate::alphacode_tui_style::Role::Dim),
                )),
                inner,
            );
            return;
        };

        if self.summary.is_none() {
            frame.render_widget(
                Paragraph::new(vec![
                    Line::from(item.title.clone()),
                    Line::from(""),
                    Line::from(item.subtitle.clone()),
                    Line::from(""),
                    Line::from("Enter opens a folder or imports the selected file."),
                    Line::from("Existing installations are preserved."),
                ])
                .wrap(Wrap { trim: false }),
                inner,
            );
            return;
        }

        let provider_items: Vec<&AccountPickerItem> = self
            .items
            .iter()
            .filter(|candidate| candidate.provider_id == item.provider_id)
            .collect();
        let mut account_items: Vec<&AccountPickerItem> = provider_items
            .iter()
            .copied()
            .filter(|candidate| matches!(action_section(candidate), ActionSection::Switch))
            .collect();
        account_items.sort_by(|left, right| {
            account_is_active(right)
                .cmp(&account_is_active(left))
                .then_with(|| compact_item_title(left).cmp(&compact_item_title(right)))
        });
        let mut secondary_items: Vec<&AccountPickerItem> = provider_items
            .iter()
            .copied()
            .filter(|candidate| !matches!(action_section(candidate), ActionSection::Switch))
            .filter(|candidate| candidate.title != item.title)
            .collect();
        secondary_items.sort_by(|left, right| {
            action_section(left)
                .cmp(&action_section(right))
                .then_with(|| compact_item_title(left).cmp(&compact_item_title(right)))
        });
        secondary_items.truncate(6);
        let (kind_label, kind_color) = action_kind_badge(&item.command);

        let mut lines = vec![
            Line::from(vec![
                Span::styled("Provider ", Style::default().fg(muted_dark())),
                Span::styled(
                    item.provider_label.clone(),
                    provider_style(&item.provider_id),
                ),
            ]),
            Line::from(vec![
                Span::styled("Saved accounts ", Style::default().fg(muted_dark())),
                Span::styled(
                    account_count_summary(account_items.len()),
                    Style::default()
                        .fg(crate::alphacode_tui_style::role_color(
                            crate::alphacode_tui_style::Role::AiText,
                        ))
                        .bold(),
                ),
            ]),
            Line::from(""),
            Line::from(vec![Span::styled(
                "Quick switch",
                Style::default().fg(muted_dark()).bold(),
            )]),
        ];

        if account_items.is_empty() {
            lines.push(Line::from(vec![Span::styled(
                "No saved accounts for this provider yet.",
                Style::default().fg(muted()),
            )]));
        } else {
            for account in &account_items {
                let is_selected = account.title == item.title;
                let bullet = if account_is_active(account) { "*" } else { "o" };
                let note = if is_selected { "  [selected]" } else { "" };
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("{} ", bullet),
                        Style::default().fg(if account_is_active(account) {
                            crate::alphacode_tui_style::role_color(
                                crate::alphacode_tui_style::Role::Success,
                            )
                        } else {
                            muted_dark()
                        }),
                    ),
                    Span::styled(
                        compact_item_title(account),
                        Style::default()
                            .fg(crate::alphacode_tui_style::role_color(
                                crate::alphacode_tui_style::Role::AiText,
                            ))
                            .bold(),
                    ),
                    Span::styled(
                        note.to_string(),
                        Style::default().fg(crate::alphacode_tui_style::role_color(
                            crate::alphacode_tui_style::Role::Accent,
                        )),
                    ),
                ]));
                lines.push(Line::from(vec![Span::styled(
                    format!(
                        "  {}",
                        truncate_with_ellipsis(
                            &account.subtitle,
                            inner.width.saturating_sub(3) as usize,
                        )
                    ),
                    Style::default().fg(muted()),
                )]));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Selected action",
            Style::default().fg(muted_dark()).bold(),
        )]));
        lines.push(Line::from(vec![
            Span::styled(kind_label, Style::default().fg(kind_color).bold()),
            Span::styled(" - ", Style::default().fg(muted_dark())),
            Span::styled(
                item.title.clone(),
                Style::default()
                    .fg(crate::alphacode_tui_style::role_color(
                        crate::alphacode_tui_style::Role::AiText,
                    ))
                    .bold(),
            ),
        ]));
        lines.push(Line::from(vec![Span::styled(
            item.subtitle.clone(),
            Style::default().fg(muted()),
        )]));
        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Runs",
            Style::default().fg(muted_dark()).bold(),
        )]));
        lines.push(Line::from(vec![Span::styled(
            command_preview(&item.command),
            Style::default().fg(crate::alphacode_tui_style::role_color(
                crate::alphacode_tui_style::Role::AiText,
            )),
        )]));
        lines.push(Line::from(vec![Span::styled(
            action_kind_help(&item.command),
            Style::default().fg(muted()),
        )]));

        if !secondary_items.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled(
                "Other controls",
                Style::default().fg(muted_dark()).bold(),
            )]));
            for related in secondary_items {
                lines.push(Line::from(vec![
                    Span::styled("- ", Style::default().fg(muted_dark())),
                    Span::styled(
                        compact_item_title(related),
                        Style::default().fg(crate::alphacode_tui_style::role_color(
                            crate::alphacode_tui_style::Role::AiText,
                        )),
                    ),
                ]));
            }
        }

        lines.push(Line::from(""));
        lines.push(Line::from(vec![Span::styled(
            "Press Enter to run this action.",
            Style::default().fg(crate::alphacode_tui_style::role_color(
                crate::alphacode_tui_style::Role::Accent,
            )),
        )]));

        frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), inner);
    }

    fn summary_line(&self) -> Line<'static> {
        if let Some(summary) = &self.summary {
            let mut spans = vec![
                metric_span(
                    "ready",
                    summary.ready_count,
                    crate::alphacode_tui_style::role_color(
                        crate::alphacode_tui_style::Role::Success,
                    ),
                ),
                Span::raw("  "),
                metric_span(
                    "attention",
                    summary.attention_count,
                    Color::Rgb(255, 192, 120),
                ),
                Span::raw("  "),
                metric_span(
                    "setup",
                    summary.setup_count,
                    crate::alphacode_tui_style::role_color(
                        crate::alphacode_tui_style::Role::MutedText,
                    ),
                ),
                Span::raw("  "),
                metric_span(
                    "providers",
                    summary.provider_count,
                    crate::alphacode_tui_style::role_color(crate::alphacode_tui_style::Role::Info),
                ),
            ];
            if summary.named_account_count > 0 {
                spans.push(Span::raw("  "));
                spans.push(metric_span(
                    "accounts",
                    summary.named_account_count,
                    crate::alphacode_tui_style::role_color(
                        crate::alphacode_tui_style::Role::Accent,
                    ),
                ));
            }
            return Line::from(spans);
        }

        Line::from(vec![Span::styled(
            format!("{} actions available", self.filtered.len()),
            Style::default().fg(muted()),
        )])
    }

    fn defaults_line(&self) -> Line<'static> {
        let Some(summary) = &self.summary else {
            return Line::from(vec![Span::styled(
                "Type to filter; Up/Down selects; Enter opens; Esc closes.",
                Style::default().fg(muted()),
            )]);
        };

        let provider = summary.default_provider.as_deref().unwrap_or("auto");
        let model = summary
            .default_model
            .as_deref()
            .unwrap_or("provider default");

        Line::from(vec![
            Span::styled("Defaults ", Style::default().fg(muted_dark())),
            Span::styled("provider ", Style::default().fg(muted_dark())),
            Span::styled(
                provider.to_string(),
                Style::default().fg(crate::alphacode_tui_style::role_color(
                    crate::alphacode_tui_style::Role::AiText,
                )),
            ),
            Span::styled("  -  model ", Style::default().fg(muted_dark())),
            Span::styled(
                model.to_string(),
                Style::default().fg(crate::alphacode_tui_style::role_color(
                    crate::alphacode_tui_style::Role::AiText,
                )),
            ),
        ])
    }
}

fn estimate_optional_string_bytes(value: &Option<String>) -> usize {
    value.as_ref().map(|value| value.capacity()).unwrap_or(0)
}

fn estimate_command_bytes(command: &AccountPickerCommand) -> usize {
    match command {
        AccountPickerCommand::SubmitInput(value) => value.capacity(),
        AccountPickerCommand::OpenAccountCenter { provider_filter }
        | AccountPickerCommand::OpenAddReplaceFlow { provider_filter } => {
            estimate_optional_string_bytes(provider_filter)
        }
        AccountPickerCommand::PromptValue {
            prompt,
            command_prefix,
            empty_value,
            status_notice,
        } => {
            prompt.capacity()
                + command_prefix.capacity()
                + estimate_optional_string_bytes(empty_value)
                + status_notice.capacity()
        }
        AccountPickerCommand::Switch { label, .. }
        | AccountPickerCommand::Login { label, .. }
        | AccountPickerCommand::Remove { label, .. } => label.capacity(),
        AccountPickerCommand::PromptNew { .. } => 0,
    }
}

fn estimate_item_bytes(item: &AccountPickerItem) -> usize {
    item.provider_id.capacity()
        + item.provider_label.capacity()
        + item.title.capacity()
        + item.subtitle.capacity()
        + estimate_command_bytes(&item.command)
}

fn estimate_summary_bytes(summary: &AccountPickerSummary) -> usize {
    estimate_optional_string_bytes(&summary.default_provider)
        + estimate_optional_string_bytes(&summary.default_model)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_action_list_renders_filters_and_selects_without_account_copy() {
        for (width, height) in [(120, 40), (60, 22)] {
            let mut picker = AccountPicker::action_list(
                "Add skill",
                vec![AccountPickerItem::action(
                    "Files",
                    "Files",
                    "my notes.txt",
                    "C:/skills/my notes.txt",
                    AccountPickerCommand::SubmitInput("/add C:/skills/my notes.txt".into()),
                )],
            );
            let mut terminal =
                ratatui::Terminal::new(ratatui::backend::TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| picker.render(frame)).unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(text.contains("my notes.txt"));
            assert!(!text.contains("Saved accounts"));
            picker
                .handle_overlay_key(KeyCode::Char('n'), KeyModifiers::empty())
                .unwrap();
            assert!(
                matches!(picker.handle_overlay_key(KeyCode::Enter, KeyModifiers::empty()).unwrap(), OverlayAction::Execute(AccountPickerCommand::SubmitInput(command)) if command == "/add C:/skills/my notes.txt")
            );
            picker
                .handle_overlay_key(KeyCode::Esc, KeyModifiers::empty())
                .unwrap();
            assert!(matches!(
                picker
                    .handle_overlay_key(KeyCode::Esc, KeyModifiers::empty())
                    .unwrap(),
                OverlayAction::Close
            ));
        }
    }
    use ratatui::{Terminal, backend::TestBackend, widgets::Paragraph};

    #[test]
    fn narrow_account_picker_keeps_the_action_list_readable() {
        let mut picker = AccountPicker::new("Accounts", vec![]);
        let mut terminal = Terminal::new(TestBackend::new(60, 24)).expect("terminal");
        terminal.draw(|frame| picker.render(frame)).expect("render");
        assert!(picker.last_action_list_area.unwrap().width >= 40);
    }

    #[test]
    fn filter_accepts_navigation_letters_and_enter_keeps_empty_search_open() {
        let mut picker = AccountPicker::new("Accounts", vec![]);
        for ch in "qgjkJGK".chars() {
            assert!(matches!(
                picker
                    .handle_overlay_key(KeyCode::Char(ch), KeyModifiers::NONE)
                    .unwrap(),
                OverlayAction::Continue
            ));
        }
        assert_eq!(picker.filter, "qgjkJGK");
        picker.filter.push_str("e\u{301}");
        picker
            .handle_overlay_key(KeyCode::Backspace, KeyModifiers::NONE)
            .unwrap();
        assert_eq!(picker.filter, "qgjkJGK");
        assert!(matches!(
            picker
                .handle_overlay_key(KeyCode::Enter, KeyModifiers::NONE)
                .unwrap(),
            OverlayAction::Continue
        ));
    }

    #[test]
    fn test_account_picker_preserves_underlying_background_outside_panels() {
        let mut picker = AccountPicker::new(
            " Accounts ",
            vec![AccountPickerItem::action(
                "openai",
                "OpenAI",
                "Add account",
                "Start login flow",
                AccountPickerCommand::SubmitInput("/account openai add default".to_string()),
            )],
        );

        let backend = TestBackend::new(40, 12);
        let mut terminal = Terminal::new(backend).expect("failed to create terminal");
        terminal
            .draw(|frame| {
                let area = frame.area();
                let fill = vec![Line::from("X".repeat(area.width as usize)); area.height as usize];
                frame.render_widget(Paragraph::new(fill), area);
                picker.render(frame);
            })
            .expect("draw failed");

        let overlay = centered_rect(
            OVERLAY_PERCENT_X,
            OVERLAY_PERCENT_Y,
            Rect::new(0, 0, 40, 12),
        );
        let probe = &terminal.backend().buffer()[(overlay.x + overlay.width - 3, overlay.y + 2)];
        assert_eq!(probe.symbol(), "X");
        assert_ne!(probe.bg, Color::Rgb(18, 21, 30));
    }

    #[test]
    fn test_account_picker_mouse_click_selects_visible_action_after_group_header() {
        let mut picker = AccountPicker::new(
            " Accounts ",
            vec![
                AccountPickerItem::action(
                    "openai",
                    "OpenAI",
                    "Provider settings",
                    "configured",
                    AccountPickerCommand::SubmitInput("/account openai settings".to_string()),
                ),
                AccountPickerItem::action(
                    "openai",
                    "OpenAI",
                    "Login / refresh",
                    "OAuth",
                    AccountPickerCommand::SubmitInput("/account openai login".to_string()),
                ),
            ],
        );

        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).expect("failed to create terminal");
        terminal
            .draw(|frame| picker.render(frame))
            .expect("draw failed");

        let list_area = picker
            .last_action_list_area
            .expect("render should record action list area");

        let initially_selected = picker.selected;
        picker.handle_overlay_mouse(crate::alphacode_tui::tui::mouse_event(
            MouseEventKind::Down(MouseButton::Left),
            list_area.x + 1,
            list_area.y,
            KeyModifiers::empty(),
        ));
        assert_eq!(
            picker.selected, initially_selected,
            "provider group header rows should not be selectable"
        );

        let expected_first_action = picker.items[picker.filtered[0]].title.clone();
        // Row 0 is the provider group header; row 1 is the first sorted action.
        picker.handle_overlay_mouse(crate::alphacode_tui::tui::mouse_event(
            MouseEventKind::Down(MouseButton::Left),
            list_area.x + 1,
            list_area.y + 1,
            KeyModifiers::empty(),
        ));

        assert_eq!(
            picker.selected_item().map(|item| item.title.as_str()),
            Some(expected_first_action.as_str())
        );
    }

    #[test]
    fn test_prompt_value_command_preview_shows_placeholder() {
        let preview = command_preview(&AccountPickerCommand::PromptValue {
            prompt: "Enter default model".to_string(),
            command_prefix: "/account default-model".to_string(),
            empty_value: Some("clear".to_string()),
            status_notice: "editing".to_string(),
        });

        assert!(preview.contains("/account default-model <value>"));
        assert!(preview.contains("clear"));
    }

    #[test]
    fn test_account_picker_sorts_switches_before_settings() {
        let picker = AccountPicker::new(
            " Accounts ",
            vec![
                AccountPickerItem::action(
                    "openai",
                    "OpenAI",
                    "Provider settings",
                    "configured",
                    AccountPickerCommand::SubmitInput("/account openai settings".to_string()),
                ),
                AccountPickerItem::action(
                    "openai",
                    "OpenAI",
                    "Switch account `work`",
                    "user@example.com - valid - active",
                    AccountPickerCommand::SubmitInput("/account openai switch work".to_string()),
                ),
                AccountPickerItem::action(
                    "defaults",
                    "Global",
                    "Default provider",
                    "Current: auto",
                    AccountPickerCommand::PromptValue {
                        prompt: "provider".to_string(),
                        command_prefix: "/account default-provider".to_string(),
                        empty_value: Some("auto".to_string()),
                        status_notice: "editing".to_string(),
                    },
                ),
            ],
        );

        let ordered_titles: Vec<String> = picker
            .filtered
            .iter()
            .map(|idx| picker.items[*idx].title.clone())
            .collect();

        assert_eq!(ordered_titles[0], "Switch account `work`");
        assert_eq!(ordered_titles[1], "Provider settings");
        assert_eq!(ordered_titles[2], "Default provider");
    }

    #[test]
    fn test_account_picker_left_right_jump_by_provider_group() {
        let mut picker = AccountPicker::new(
            " Accounts ",
            vec![
                AccountPickerItem::action(
                    "claude",
                    "Claude",
                    "Switch account `work`",
                    "a@example.com - valid - active",
                    AccountPickerCommand::SubmitInput("/account claude switch work".to_string()),
                ),
                AccountPickerItem::action(
                    "claude",
                    "Claude",
                    "Provider settings",
                    "configured",
                    AccountPickerCommand::SubmitInput("/account claude settings".to_string()),
                ),
                AccountPickerItem::action(
                    "openai",
                    "OpenAI",
                    "Switch account `default`",
                    "b@example.com - valid - active",
                    AccountPickerCommand::SubmitInput("/account openai switch default".to_string()),
                ),
            ],
        );

        picker.selected = 1;
        let _ = picker.handle_overlay_key(KeyCode::Right, KeyModifiers::empty());
        assert_eq!(
            picker.items[picker.filtered[picker.selected]].provider_id,
            "openai"
        );

        let _ = picker.handle_overlay_key(KeyCode::Left, KeyModifiers::empty());
        assert_eq!(
            picker.items[picker.filtered[picker.selected]].provider_id,
            "claude"
        );
        assert_eq!(picker.selected, 0);
    }
}
