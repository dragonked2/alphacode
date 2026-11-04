use super::*;

#[derive(Debug, Clone)]
pub(super) struct FileBrowser {
    request: u64,
    directory: String,
    loading: bool,
}

impl AccountPicker {
    pub fn file_browser(request: u64, directory: String) -> Self {
        let mut picker = Self::action_list("Add skill", Vec::new());
        picker.file_browser = Some(FileBrowser {
            request,
            directory,
            loading: true,
        });
        picker
    }

    pub fn is_file_browser_request(&self, request: u64) -> bool {
        self.file_browser
            .as_ref()
            .is_some_and(|browser| browser.request == request)
    }

    pub fn set_file_browser_items(&mut self, directory: String, items: Vec<AccountPickerItem>) {
        if let Some(browser) = &mut self.file_browser {
            browser.directory = directory;
            browser.loading = false;
        }
        self.items = items;
        self.build_search_index();
        self.apply_filter();
    }

    pub(super) fn render_file_browser(&mut self, frame: &mut Frame) {
        let browser = self.file_browser.as_ref().expect("file browser mode");
        let area = centered_rect(92, 84, frame.area());
        let footer = if area.width >= 76 {
            " ↑↓ select · Enter open / import · type to filter · Esc clear / close "
        } else {
            " ↑↓ select · Enter open · Esc close "
        };
        let block = Block::default()
            .title(" Add skill · .md / .txt ")
            .title_bottom(footer)
            .borders(Borders::ALL)
            .border_type(ratatui::widgets::BorderType::Rounded)
            .border_style(Style::default().fg(panel_border_active()))
            .style(Style::default().bg(panel_bg()));
        let inner = block.inner(area);
        frame.render_widget(ratatui::widgets::Clear, area);
        frame.render_widget(block, area);
        let rows = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(2),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(inner);
        frame.render_widget(
            Paragraph::new(truncate_with_ellipsis(
                &browser.directory,
                rows[0].width as usize,
            ))
            .style(Style::default().fg(muted())),
            rows[0],
        );
        frame.render_widget(
            Paragraph::new(format!(
                "Filter: {}",
                if self.filter.is_empty() {
                    "type a filename…"
                } else {
                    &self.filter
                }
            ))
            .style(Style::default().fg(if self.filter.is_empty() {
                muted()
            } else {
                panel_border_active()
            })),
            rows[1],
        );
        self.last_action_list_area = Some(rows[2]);
        if browser.loading || self.filtered.is_empty() {
            let text = if browser.loading {
                "Opening folder…"
            } else if self.filter.is_empty() {
                "No .md or .txt files in this folder. Choose a folder to continue."
            } else {
                "No matches. Press Esc to clear the filter."
            };
            frame.render_widget(
                Paragraph::new(text)
                    .wrap(Wrap { trim: false })
                    .style(Style::default().fg(muted())),
                rows[2],
            );
        } else {
            let start = self.visible_window_start(rows[2].height as usize);
            let lines: Vec<Line> = self
                .filtered
                .iter()
                .enumerate()
                .skip(start)
                .take(rows[2].height as usize)
                .map(|(visible, index)| {
                    let item = &self.items[*index];
                    let folder = item.provider_id == "Folders";
                    let suffix = if folder { "/" } else { "" };
                    let prefix = if visible == self.selected {
                        "› "
                    } else {
                        "  "
                    };
                    let title = truncate_with_ellipsis(
                        &format!("{prefix}{}{suffix}", item.title),
                        rows[2].width as usize,
                    );
                    let style = if visible == self.selected {
                        Style::default()
                            .bg(selected_bg())
                            .fg(panel_border_active())
                            .bold()
                    } else {
                        Style::default().fg(if folder {
                            panel_border_active()
                        } else {
                            crate::alphacode_tui_style::role_color(
                                crate::alphacode_tui_style::Role::AiText,
                            )
                        })
                    };
                    Line::styled(title, style)
                })
                .collect();
            frame.render_widget(Paragraph::new(lines), rows[2]);
        }
        frame.render_widget(
            Paragraph::new(if browser.loading {
                "Loading…".into()
            } else {
                format!(
                    "{} shown · folders first · files in this folder only",
                    self.filtered.len()
                )
            })
            .style(Style::default().fg(muted_dark())),
            rows[3],
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    #[test]
    fn file_browser_renders_without_account_panels_and_keeps_filter_during_load() {
        for (width, height) in [(120, 32), (80, 24), (50, 12), (24, 8)] {
            let mut picker = AccountPicker::file_browser(42, "C:/skills/世界".into());
            picker
                .handle_overlay_key(KeyCode::Char('m'), KeyModifiers::NONE)
                .unwrap();
            picker.set_file_browser_items(
                "C:/skills/世界".into(),
                vec![
                    AccountPickerItem::action(
                        "Folders",
                        "Folders",
                        "..",
                        "Open folder",
                        AccountPickerCommand::SubmitInput("/add --browse ..".into()),
                    ),
                    AccountPickerItem::action(
                        "Files",
                        "Files",
                        "my-skill.md",
                        "Import skill",
                        AccountPickerCommand::SubmitInput("/add my-skill.md".into()),
                    ),
                    AccountPickerItem::action(
                        "Files",
                        "Files",
                        "other.txt",
                        "Import skill",
                        AccountPickerCommand::SubmitInput("/add other.txt".into()),
                    ),
                ],
            );
            assert_eq!(picker.filter, "m");
            assert_eq!(picker.filtered.len(), 1);
            assert_eq!(picker.selected_item().unwrap().title, "my-skill.md");
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| picker.render(frame)).unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(!text.contains("Overview"));
            assert!(!text.contains("account"));
            assert!(text.contains("Add skill"));
            let list = picker.last_action_list_area.unwrap();
            assert_eq!(picker.visible_index_for_action_row(0, list.height), Some(0));
            assert!(
                matches!(picker.handle_overlay_key(KeyCode::Enter, KeyModifiers::NONE).unwrap(), OverlayAction::Execute(AccountPickerCommand::SubmitInput(command)) if command == "/add my-skill.md")
            );
        }
    }
}
