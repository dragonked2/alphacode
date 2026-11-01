//! Evidence-based checkpoints between tool work and assistant updates.
use super::*;
use std::collections::BTreeSet;

#[derive(Default)]
struct WorkSummary {
    commands: usize,
    reads: usize,
    edits: usize,
    other: usize,
    failed: usize,
    files: BTreeSet<String>,
    additions: usize,
    deletions: usize,
}

impl WorkSummary {
    fn observe(&mut self, tool: &ToolCall, output: &str) {
        let name = tools_ui::canonical_tool_name(&tool.name);
        if name == "batch"
            && let Some(calls) = tool.input.get("tool_calls").and_then(|v| v.as_array())
        {
            let results = tools_ui::parse_batch_sub_outputs_by_index(output);
            if results.is_empty() {
                if tools_ui::tool_output_looks_failed(output) {
                    self.failed += 1;
                } else {
                    self.other += 1;
                }
                return;
            }
            for (index, call) in calls.iter().enumerate() {
                if let Some(result) = results.get(&(index + 1)) {
                    if result.errored {
                        self.failed += 1;
                        continue;
                    }
                    let nested = ToolCall {
                        name: call
                            .get("tool")
                            .or_else(|| call.get("name"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_owned(),
                        input: tools_ui::batch_subcall_params(call),
                        ..Default::default()
                    };
                    // Nested batches are not supported by the batch tool.
                    if nested.name != "batch" {
                        self.observe(&nested, &result.content);
                    }
                }
            }
            return;
        }
        // The native patch tool can partially succeed. Its per-file result
        // lines are stronger evidence than paths mentioned in the input.
        if name == "patch"
            && output
                .lines()
                .any(|line| line.starts_with("✓ ") || line.starts_with("✗ "))
        {
            let mut succeeded = false;
            for line in output.lines() {
                if let Some((path, _)) = line
                    .strip_prefix("✓ ")
                    .and_then(|line| line.split_once(": "))
                {
                    self.files.insert(path.to_owned());
                    succeeded = true;
                } else if line.starts_with("✗ ") {
                    self.failed += 1;
                }
            }
            if succeeded {
                self.edits += 1;
                let (add, del) = crate::alphacode_tui::tui::ui_diff::diff_change_counts(output);
                self.additions = self.additions.saturating_add(add);
                self.deletions = self.deletions.saturating_add(del);
            }
            return;
        }
        if tools_ui::tool_output_looks_failed(output) {
            self.failed += 1;
            return;
        }
        match name {
            "bash" => self.commands += 1,
            "read" => self.reads += 1,
            "todo" => {}
            name if tools_ui::is_edit_tool_name(name) => {
                self.edits += 1;
                if let Some(path) = tool
                    .input
                    .get("file_path")
                    .or_else(|| tool.input.get("path"))
                    .and_then(|v| v.as_str())
                {
                    self.files.insert(path.to_owned());
                }
                // Patch tools may touch several files in one successful call.
                if let Some(patch) = tool
                    .input
                    .get("patch_text")
                    .or_else(|| tool.input.get("patch"))
                    .and_then(|v| v.as_str())
                {
                    for line in patch.lines() {
                        let path = [
                            "*** Update File: ",
                            "*** Add File: ",
                            "*** Delete File: ",
                            "+++ b/",
                            "--- a/",
                        ]
                        .iter()
                        .find_map(|prefix| line.strip_prefix(prefix));
                        if let Some(path) = path.filter(|p| !p.is_empty()) {
                            self.files.insert(path.to_owned());
                        }
                    }
                }
                // An overwrite's input alone does not reveal removed lines.
                // Count write lines only when its result contains a real diff.
                let (add, del) = if name == "write" {
                    crate::alphacode_tui::tui::ui_diff::diff_change_counts(output)
                } else {
                    super::diff_change_counts_for_tool(tool, output)
                };
                self.additions = self.additions.saturating_add(add);
                self.deletions = self.deletions.saturating_add(del);
            }
            _ => self.other += 1,
        }
    }

    fn line(&self, width: u16) -> Option<Line<'static>> {
        let mut parts = Vec::new();
        if self.commands > 0 {
            parts.push(format!(
                "Ran {} command{}",
                self.commands,
                if self.commands == 1 { "" } else { "s" }
            ));
        }
        if self.reads > 0 {
            parts.push(format!(
                "Read {} file{}",
                self.reads,
                if self.reads == 1 { "" } else { "s" }
            ));
        }
        if self.edits > 0 {
            if self.files.is_empty() {
                parts.push(format!(
                    "Made {} file edit{}",
                    self.edits,
                    if self.edits == 1 { "" } else { "s" }
                ));
            } else {
                parts.push(format!(
                    "Edited {} file{}",
                    self.files.len(),
                    if self.files.len() == 1 { "" } else { "s" }
                ));
            }
        }
        if self.other > 0 {
            parts.push(format!(
                "Used {} tool{}",
                self.other,
                if self.other == 1 { "" } else { "s" }
            ));
        }
        if self.failed > 0 {
            parts.push(format!(
                "{} failed action{}",
                self.failed,
                if self.failed == 1 { "" } else { "s" }
            ));
        }
        if parts.is_empty() {
            return None;
        }
        let mut spans = vec![Span::styled(
            format!("  {}", parts.join(" · ")),
            Style::default().fg(dim_color()),
        )];
        if self.additions > 0 || self.deletions > 0 {
            spans.push(Span::styled(" · ", Style::default().fg(dim_color())));
            spans.push(Span::styled(
                format!("+{}", self.additions),
                Style::default().fg(super::diff_add_color()),
            ));
            spans.push(Span::styled(
                format!(" -{}", self.deletions),
                Style::default().fg(super::diff_del_color()),
            ));
            spans.push(Span::styled(
                " edit lines",
                Style::default().fg(dim_color()),
            ));
        }
        Some(super::truncate_line_with_ellipsis_to_width(
            &Line::from(spans),
            usize::from(width.saturating_sub(1)),
        ))
    }
}

/// Keep turn totals out of the per-frame history/diff hot path. The display
/// version changes when completed tool results arrive, not on animation ticks.
pub(super) fn turn_changes(app: &dyn TuiState, width: u16) -> Option<Line<'static>> {
    type CacheEntry = (Option<String>, u64, usize, WorkSummary);
    thread_local! {
        static CACHE: std::cell::RefCell<Option<CacheEntry>> = const { std::cell::RefCell::new(None) };
    }
    CACHE.with(|slot| {
        let mut cache = slot.borrow_mut();
        let messages = app.display_messages();
        let session = app.current_session_id();
        let version = app.display_messages_version();
        let valid = session.is_some()
            && cache.as_ref().is_some_and(|(id, stamp, count, _)| {
                *id == session && *stamp == version && *count == messages.len()
            });
        if !valid {
            let mut summary = WorkSummary::default();
            for message in messages
                .iter()
                .rev()
                .take_while(|m| m.effective_role() != "user")
            {
                if let Some(tool) = message
                    .tool_data
                    .as_ref()
                    .filter(|_| message.effective_role() == "tool")
                {
                    summary.observe(tool, &message.content);
                }
            }
            summary.commands = 0;
            summary.reads = 0;
            summary.other = 0;
            summary.failed = 0;
            *cache = Some((session, version, messages.len(), summary));
        }
        cache.as_ref()?.3.line(width)
    })
}

pub(super) fn checkpoint(
    messages: &[DisplayMessage],
    before: usize,
    width: u16,
) -> Option<Line<'static>> {
    let mut summary = WorkSummary::default();
    for message in messages.get(..before)?.iter().rev() {
        match message.effective_role() {
            "tool" => {
                if let Some(tool) = &message.tool_data {
                    summary.observe(tool, &message.content);
                }
            }
            "meta" | "reasoning" => {}
            _ => break,
        }
    }
    summary.line(width)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn counts_unique_files_and_excludes_failed_edits() {
        let mut summary = WorkSummary::default();
        let edit = ToolCall {
            name: "edit".into(),
            input: json!({"file_path":"src/lib.rs", "old_string":"old\n", "new_string":"new\nextra\n"}),
            ..Default::default()
        };
        summary.observe(&edit, "Success");
        summary.observe(&edit, "Success");
        summary.observe(&edit, "Error: permission denied");
        summary.observe(
            &ToolCall {
                name: "bash".into(),
                ..Default::default()
            },
            "Exit code: 0",
        );
        let text = super::super::line_plain_text(&summary.line(160).unwrap());
        assert!(
            text.contains("Ran 1 command · Edited 1 file · 1 failed action"),
            "{text}"
        );
        assert!(text.contains("+4 -2 edit lines"), "{text}");
    }

    #[test]
    fn overwrite_does_not_invent_diff_totals() {
        let mut summary = WorkSummary::default();
        summary.observe(
            &ToolCall {
                name: "write".into(),
                input: json!({"file_path":"a", "content":"new\n"}),
                ..Default::default()
            },
            "Written successfully",
        );
        let text = super::super::line_plain_text(&summary.line(80).unwrap());
        assert!(text.contains("Edited 1 file"));
        assert!(!text.contains("edit lines"));
    }

    #[test]
    fn batch_summary_uses_individual_results() {
        let mut summary = WorkSummary::default();
        summary.observe(&ToolCall {
            name: "batch".into(),
            input: json!({"tool_calls": [
                {"tool": "bash", "parameters": {"command": "cargo check"}},
                {"tool": "edit", "parameters": {"file_path": "a", "old_string": "x", "new_string": "y"}}
            ]}),
            ..Default::default()
        }, "--- [1] bash ---\nExit code: 0\n--- [2] edit ---\nError: missing file\nCompleted: 1 succeeded, 1 failed");
        let text = super::super::line_plain_text(&summary.line(100).unwrap());
        assert!(text.contains("Ran 1 command"), "{text}");
        assert!(text.contains("1 failed action"), "{text}");
        assert!(!text.contains("Edited"), "{text}");
    }

    #[test]
    fn partial_patch_counts_only_confirmed_paths() {
        let mut summary = WorkSummary::default();
        summary.observe(&ToolCall {
            name: "patch".into(),
            input: json!({"patch_text": "--- a/good.rs\n+++ b/good.rs\n+x\n--- a/missing.rs\n+++ b/missing.rs\n+y"}),
            ..Default::default()
        }, "1 succeeded, 1 failed\n✓ good.rs: Updated\n✗ missing.rs: not found");
        let text = super::super::line_plain_text(&summary.line(120).unwrap());
        assert_eq!(
            summary.files.into_iter().collect::<Vec<_>>(),
            vec!["good.rs"]
        );
        assert!(text.contains("Edited 1 file · 1 failed action"), "{text}");
        assert!(!text.contains("edit lines"), "{text}");
    }

    #[test]
    fn checkpoints_stop_at_the_previous_update_and_fit_small_widths() {
        let mut tool = DisplayMessage::system("Exit code: 0");
        tool.role = "tool".into();
        tool.tool_data = Some(ToolCall {
            name: "bash".into(),
            ..Default::default()
        });
        let mut update = DisplayMessage::system("The check passed.");
        update.role = "assistant".into();
        let messages = vec![tool.clone(), update, tool];
        let line = checkpoint(&messages, 3, 80).unwrap();
        assert!(super::super::line_plain_text(&line).contains("Ran 1 command"));
        for width in 0..40 {
            assert!(checkpoint(&messages, 3, width).unwrap().width() <= usize::from(width));
        }
        assert!(checkpoint(&messages, 0, 80).is_none());
    }
}
