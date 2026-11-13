use crate::alphacode_app_core::message::{ContentBlock, ToolCall};
use crate::alphacode_app_core::tool::ToolOutput;
use crate::terminal_println as println;

/// Hard ceiling on a single tool result stored in conversation history.
///
/// This is a *prompt budget*, not a transport limit. Every retained character
/// is re-sent on every subsequent request in the turn, so a 1 MiB cap let one
/// `grep -r` or failed build dump ~250k tokens into history that the model then
/// re-reads on each iteration. History is capped per result; tools that
/// genuinely need large output should write to a file and read targeted
/// sections, which the truncation notice already tells the model to do.
///
/// 48k chars (~12k tokens) keeps a full stack trace or test summary intact
/// while preventing any single call from dominating the window.
pub(super) const MAX_TOOL_OUTPUT_CHARS_FOR_HISTORY: usize = 48 * 1024;

/// Cap tool output for history. Uses smart truncation: preserves the last
/// N lines (which typically contain the final result/summary) and the first
/// N lines (which typically contain setup/context), with a clear truncation
/// notice in the middle. This is much more useful than naive head-only
/// truncation which loses the actual result of long commands.
pub(super) fn cap_tool_output_for_history(tool_name: &str, mut output: ToolOutput) -> ToolOutput {
    let char_count = output.output.chars().count();
    if char_count <= MAX_TOOL_OUTPUT_CHARS_FOR_HISTORY {
        return output;
    }

    let original_chars = char_count;
    // Smart truncation: keep ~40% from head, ~40% from tail, ~20% for notice
    let half = MAX_TOOL_OUTPUT_CHARS_FOR_HISTORY / 2;
    let head_limit = (half * 4 / 10).min(256 * 1024);
    let tail_limit = (half * 4 / 10).min(256 * 1024);
    let head: String = output.output.chars().take(head_limit).collect();
    let tail: String = output
        .output
        .chars()
        .rev()
        .take(tail_limit)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    let dropped = original_chars - head_limit - tail_limit;
    output.output = format!(
        "{}\n\n[truncated: `{}` produced {} chars; kept head+tail. Redirect large logs to a file and read the section you need.]\n\n{}",
        head.trim_end(),
        tool_name,
        original_chars,
        tail.trim_start(),
    );
    // Surface the cut on the transcript row (see guard_context_overflow for
    // the context-budget variant of the same marker).
    output.title = Some(match output.title.take() {
        Some(existing) => format!("{existing} · truncated {}k chars", dropped / 1000),
        None => format!("truncated {}k chars", dropped / 1000),
    });
    output
}

pub(super) fn cap_sdk_tool_content_for_history(tool_name: &str, content: String) -> String {
    // SDK results need the same final diagnostics as native tools. Head-only
    // clipping discarded test summaries and errors at the end of long logs.
    cap_tool_output_for_history(tool_name, ToolOutput::new(content)).output
}

/// Build rendered side-pane images from a tool output's attached images.
///
/// This mirrors how `render_messages_and_images` derives images from persisted
/// session history (source = ToolResult), so live-streamed images match what a
/// later History reload would produce. `tool_name` and `tool_input` provide the
/// label fallback (e.g. the `read` tool's `file_path`); `tool_call_id` anchors
/// the image to its tool message in the transcript.
pub(super) fn tool_output_side_pane_images(
    tool_call_id: &str,
    tool_name: &str,
    tool_input: &serde_json::Value,
    output: &ToolOutput,
) -> Vec<crate::alphacode_session_types::RenderedImage> {
    if output.images.is_empty() {
        return Vec::new();
    }
    let fallback_label = tool_input
        .get("file_path")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    output
        .images
        .iter()
        .map(|img| crate::alphacode_session_types::RenderedImage {
            media_type: img.media_type.clone(),
            data: img.data.clone(),
            label: img
                .label
                .as_ref()
                .map(|label| label.trim().to_string())
                .filter(|label| !label.is_empty())
                .or_else(|| fallback_label.clone()),
            source: crate::alphacode_session_types::RenderedImageSource::ToolResult {
                tool_name: tool_name.to_string(),
            },
            anchor: Some(
                crate::alphacode_session_types::RenderedImageAnchor::ToolCall {
                    id: tool_call_id.to_string(),
                },
            ),
        })
        .collect()
}

pub(super) fn tool_output_to_content_blocks(
    tool_use_id: String,
    output: ToolOutput,
) -> Vec<ContentBlock> {
    let mut blocks = vec![ContentBlock::ToolResult {
        tool_use_id,
        content: output.output,
        is_error: None,
    }];
    for img in output.images {
        blocks.push(ContentBlock::Image {
            media_type: img.media_type,
            data: img.data,
        });
        if let Some(label) = img.label.filter(|label| !label.trim().is_empty()) {
            blocks.push(ContentBlock::Text {
                text: format!(
                    "[Attached image associated with the preceding tool result: {}]",
                    label
                ),
                cache_control: None,
            });
        }
    }
    blocks
}

pub(super) fn print_tool_summary(tool: &ToolCall) {
    match tool.name.as_str() {
        "bash" => {
            if let Some(cmd) = tool.input.get("command").and_then(|v| v.as_str()) {
                let short = if cmd.len() > 60 {
                    format!("{}...", crate::util::truncate_str(cmd, 60))
                } else {
                    cmd.to_string()
                };
                println!("$ {}", short);
            }
        }
        "read" | "write" | "edit" => {
            if let Some(path) = tool.input.get("file_path").and_then(|v| v.as_str()) {
                println!("{}", path);
            }
        }
        "glob" | "grep" => {
            if let Some(pattern) = tool.input.get("pattern").and_then(|v| v.as_str()) {
                println!("'{}'", pattern);
            }
        }
        "ls" => {
            let path = tool
                .input
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            println!("{}", path);
        }
        _ => {}
    }
}

/// Maximum number of lines a headless tool-result preview shows.
const TOOL_PREVIEW_MAX_LINES: usize = 8;

/// Maximum number of characters a headless tool-result preview shows.
const TOOL_PREVIEW_MAX_CHARS: usize = 2000;

/// Render a tool result for the headless console.
///
/// The console used to print only the *first line* of a tool's output, so a
/// result that had been truncated looked exactly like a complete one: the
/// reader had no way to tell that the tool had produced more. The preview now
/// shows a bounded slice of the output and states exactly how much was left
/// out, so a truncated result is never mistaken for a complete one.
pub(super) fn format_tool_output_preview(output: &str) -> String {
    let trimmed = output.trim_end();
    if trimmed.is_empty() {
        return "(no output)".to_string();
    }

    let all_lines: Vec<&str> = trimmed.lines().collect();
    let total_lines = all_lines.len();
    let total_chars = trimmed.chars().count();

    let mut preview = String::new();
    let mut used_chars = 0usize;
    let mut shown_lines = 0usize;

    for line in &all_lines {
        if shown_lines >= TOOL_PREVIEW_MAX_LINES || used_chars >= TOOL_PREVIEW_MAX_CHARS {
            break;
        }
        let remaining = TOOL_PREVIEW_MAX_CHARS - used_chars;
        let line_chars = line.chars().count();
        let clipped = if line_chars > remaining {
            crate::util::truncate_str(line, remaining).to_string()
        } else {
            (*line).to_string()
        };
        let clipped_chars = clipped.chars().count();
        let line_was_clipped = clipped_chars < line_chars;
        preview.push_str(&clipped);
        used_chars += clipped_chars;
        shown_lines += 1;
        if line_was_clipped {
            break;
        }
        if shown_lines < total_lines {
            preview.push('\n');
        }
    }

    let hidden_lines = total_lines.saturating_sub(shown_lines);
    let hidden_chars = total_chars.saturating_sub(used_chars);
    if hidden_lines > 0 || hidden_chars > 0 {
        if !preview.is_empty() && !preview.ends_with('\n') {
            preview.push('\n');
        }
        preview.push_str(&format!(
            "… truncated: {hidden_lines} more line{}, {hidden_chars} more char{}",
            if hidden_lines == 1 { "" } else { "s" },
            if hidden_chars == 1 { "" } else { "s" },
        ));
    }
    preview
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_tool_output_leaves_small_output_unchanged() {
        let output = ToolOutput::new("short output");
        let capped = cap_tool_output_for_history("bash", output.clone());
        assert_eq!(capped.output, output.output);
    }

    #[test]
    fn cap_tool_output_adds_visible_truncation_notice() {
        let output = ToolOutput::new("x".repeat(MAX_TOOL_OUTPUT_CHARS_FOR_HISTORY + 10));
        let capped = cap_tool_output_for_history("bash", output);
        assert!(capped.output.len() < MAX_TOOL_OUTPUT_CHARS_FOR_HISTORY + 1_000);
        assert!(capped.output.contains("[truncated: `bash` produced"));
        assert!(capped.output.contains("Redirect large logs to a file"));
        // The notice is paid on every retained result, so it must stay short.
        assert!(
            capped.output.len() < 200_000,
            "truncated result must not stay near the old 1 MiB ceiling"
        );
    }

    /// A single oversized tool result must not be able to dominate the prompt.
    #[test]
    fn history_cap_is_a_prompt_budget_not_a_transport_limit() {
        // The cap is a constant, so this is checked at compile time: a regression
        // that raises MAX_TOOL_OUTPUT_CHARS_FOR_HISTORY above 64 KiB fails the
        // build instead of shipping a prompt that re-sends the oversized result
        // every turn. (const blocks cannot format, so the message is static.)
        const { assert!(MAX_TOOL_OUTPUT_CHARS_FOR_HISTORY <= 64 * 1024) };
    }

    #[test]
    fn cap_sdk_tool_content_adds_same_notice() {
        let capped = cap_sdk_tool_content_for_history(
            "custom",
            "y".repeat(MAX_TOOL_OUTPUT_CHARS_FOR_HISTORY + 10),
        );
        assert!(capped.contains("[truncated: `custom` produced"));
        assert!(capped.contains("kept head+tail"));
    }

    #[test]
    fn sdk_and_native_caps_preserve_unicode_diagnostics_and_metadata() {
        let content = format!(
            "setup started\n{}\nFINAL: tests failed — 修复 🦀",
            "中🦀e\u{301}".repeat(MAX_TOOL_OUTPUT_CHARS_FOR_HISTORY),
        );
        let metadata = serde_json::json!({"process_exit_code": 1});
        let output = ToolOutput::new(content.clone())
            .with_metadata(metadata.clone())
            .with_image("image/png", "test-image");
        let native = cap_tool_output_for_history("bash", output);
        let sdk = cap_sdk_tool_content_for_history("bash", content);
        assert_eq!(sdk, native.output);
        assert!(sdk.starts_with("setup started\n"));
        assert!(sdk.ends_with("FINAL: tests failed — 修复 🦀"));
        assert!(sdk.chars().count() <= MAX_TOOL_OUTPUT_CHARS_FOR_HISTORY);
        assert_eq!(native.metadata, Some(metadata));
        assert_eq!(native.images.len(), 1);
        assert_eq!(native.images[0].data, "test-image");
    }

    #[test]
    fn sdk_cap_leaves_short_output_unchanged() {
        let content = "success — 完成 🦀\n";
        assert_eq!(
            cap_sdk_tool_content_for_history("custom", content.into()),
            content
        );
    }

    #[test]
    fn tool_output_preview_shows_short_output_verbatim() {
        let preview = format_tool_output_preview("done");
        assert_eq!(preview, "done");
        assert!(!preview.contains("truncated"));
    }

    #[test]
    fn tool_output_preview_reports_empty_output() {
        assert_eq!(format_tool_output_preview("   \n  "), "(no output)");
    }

    #[test]
    fn tool_output_preview_states_how_much_was_left_out() {
        // A truncated result must never look like a complete one: the reader
        // has to be told that the tool produced more.
        let output = (1..=40)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let preview = format_tool_output_preview(&output);
        assert!(
            preview.contains("truncated"),
            "expected a truncation notice, got {preview:?}"
        );
        assert!(
            preview.contains("more line"),
            "expected the hidden line count, got {preview:?}"
        );
        assert!(
            preview.lines().count() <= TOOL_PREVIEW_MAX_LINES + 1,
            "the preview must stay bounded, got {} lines",
            preview.lines().count()
        );
    }

    #[test]
    fn tool_output_preview_clips_a_single_long_line() {
        let output = "x".repeat(TOOL_PREVIEW_MAX_CHARS * 2);
        let preview = format_tool_output_preview(&output);
        assert!(preview.contains("truncated"));
        assert!(
            preview.chars().count() < TOOL_PREVIEW_MAX_CHARS + 100,
            "the preview must stay bounded"
        );
    }
}
