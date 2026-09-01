use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Map provider-side tool names to the canonical lowercase name used across
/// the TUI. This mirrors `alphacode_tool_types::resolve_tool_name` (plus the
/// Anthropic OAuth PascalCase aliases) so surfaces show one friendly name even
/// for nested `batch` subcalls that bypass the provider-side reverse map.
///
/// One intentional divergence: the core registry maps `grep`/`file_grep`/`Grep`
/// to `agentgrep` (the runtime replacement), but the display layer keeps them
/// as `grep` so summary arms for both the legacy `grep` schema (`pattern`/`path`)
/// and `agentgrep` (query/mode) stay distinguishable.
pub fn canonical_tool_name(name: &str) -> &str {
    match name {
        "communicate" => "swarm",
        "task" | "task_runner" | "Agent" => "subagent",
        "launch" => "open",
        "shell" | "shell_exec" | "Bash" => "bash",
        "Read" | "read_file" | "file_read" => "read",
        "Write" | "write_file" | "file_write" => "write",
        "Edit" | "edit_file" | "file_edit" => "edit",
        "MultiEdit" => "multiedit",
        "Patch" => "patch",
        "ApplyPatch" => "apply_patch",
        "Glob" | "file_glob" => "glob",
        "Grep" | "file_grep" => "grep",
        "todo_read" | "todo_write" | "todoread" | "todowrite" | "todos" => "todo",
        "skill" | "Skill" => "skill_manage",
        "ScheduleWakeup" => "schedule",
        other => other,
    }
}

/// Map provider-side tool names to the label shown on tool rows. Delegates to
/// [`canonical_tool_name`] so the two tables can never drift apart.
pub fn resolve_display_tool_name(name: &str) -> &str {
    canonical_tool_name(name)
}

pub fn is_edit_tool_name(name: &str) -> bool {
    matches!(
        canonical_tool_name(name),
        "write" | "edit" | "multiedit" | "patch" | "apply_patch"
    )
}

fn parse_nonzero_exit_code_line(line: &str) -> bool {
    let trimmed = line.trim();
    if let Some(rest) = trimmed.strip_prefix("Exit code:") {
        return rest
            .trim()
            .parse::<i32>()
            .map(|code| code != 0)
            .unwrap_or(false);
    }
    if let Some(rest) = trimmed.strip_prefix("--- Command finished with exit code:") {
        return rest
            .trim()
            .trim_end_matches('-')
            .trim()
            .parse::<i32>()
            .map(|code| code != 0)
            .unwrap_or(false);
    }
    false
}

fn display_prefix_by_width(s: &str, max_width: usize) -> &str {
    if max_width == 0 {
        return "";
    }
    // Fast path: ASCII strings are width-equal to byte length
    if s.is_ascii() {
        let end = max_width.min(s.len());
        return &s[..end];
    }
    let mut used = 0usize;
    let mut end = 0usize;
    for (idx, ch) in s.char_indices() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cw > max_width {
            break;
        }
        used += cw;
        end = idx + ch.len_utf8();
    }
    &s[..end]
}

fn display_suffix_by_width(s: &str, max_width: usize) -> &str {
    if max_width == 0 {
        return "";
    }
    // Fast path: ASCII strings are width-equal to byte length
    if s.is_ascii() {
        let start = s.len().saturating_sub(max_width);
        return &s[start..];
    }
    let mut used = 0usize;
    let mut start = s.len();
    for (idx, ch) in s.char_indices().rev() {
        let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + cw > max_width {
            break;
        }
        used += cw;
        start = idx;
    }
    &s[start..]
}

pub fn truncate_middle_display(s: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(s) <= max_width {
        return s.to_string();
    }
    if max_width == 0 {
        return String::new();
    }
    if max_width == 1 {
        return "…".to_string();
    }
    let remaining = max_width.saturating_sub(1);
    let head = remaining / 2 + remaining % 2;
    let tail = remaining / 2;
    let prefix = display_prefix_by_width(s, head);
    let suffix = display_suffix_by_width(s, tail);
    // Pre-allocate with exact capacity to avoid reallocations
    let mut result = String::with_capacity(prefix.len() + 1 + suffix.len());
    result.push_str(prefix);
    result.push('…');
    result.push_str(suffix);
    result
}

fn normalize_backticked_identifier(text: &str) -> String {
    text.replace('`', "").trim().to_string()
}

pub fn concise_tool_error_summary(content: &str) -> Option<String> {
    for raw_line in content.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let detail = line
            .strip_prefix("Error:")
            .or_else(|| line.strip_prefix("error:"))
            .or_else(|| line.strip_prefix("Failed:"))
            .map(str::trim);
        if let Some(detail) = detail {
            if let Some(field) = detail.strip_prefix("missing field ") {
                return Some(format!(
                    "invalid input: missing {}",
                    normalize_backticked_identifier(field)
                ));
            }
            if detail.starts_with("invalid type") || detail.starts_with("unknown variant") {
                return Some(format!("invalid input: {}", detail));
            }
            if detail.contains("source metadata") && detail.contains("was for") {
                return Some("build source changed before reload".to_string());
            }
            if detail.starts_with("Refusing to publish") {
                return Some("reload refused: rebuild against current source".to_string());
            }
            return Some(format!("error: {}", truncate_middle_display(detail, 80)));
        }

        if line.contains("Compile terminated by signal") {
            return Some(line.to_string());
        }
        if let Some(rest) = line.strip_prefix("Exit code:")
            && let Ok(code) = rest.trim().parse::<i32>()
            && code != 0
        {
            return Some(format!("exit {}", code));
        }
        if let Some(rest) = line.strip_prefix("--- Command finished with exit code:") {
            let code = rest.trim().trim_end_matches('-').trim();
            if code != "0" && !code.is_empty() {
                return Some(format!("exit {}", code));
            }
        }
    }

    None
}

pub fn tool_output_looks_failed(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Fast path: check the first few bytes before allocating
    if trimmed.starts_with('✗') || trimmed.starts_with("Error:") || trimmed.starts_with("error:") || trimmed.starts_with("Failed:") {
        return true;
    }
    let normalized = trimmed
        .strip_prefix('[')
        .and_then(|rest| rest.split_once("] "))
        .filter(|(label, _)| !label.is_empty() && !label.contains(['\n', '\r']))
        .map(|(_, rest)| rest)
        .unwrap_or(trimmed);
    let lower = normalized.to_ascii_lowercase();
    if concise_tool_error_summary(normalized).is_some()
        || lower.starts_with("error:")
        || lower.starts_with("failed:")
        || normalized.starts_with('✗')
    {
        return true;
    }

    normalized.lines().any(|line| {
        let line = line.trim();
        parse_nonzero_exit_code_line(line)
            || line.eq_ignore_ascii_case("Status: failed")
            || line.eq_ignore_ascii_case("failed to start")
            || line.eq_ignore_ascii_case("terminated")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonicalizes_edit_tool_names() {
        assert_eq!(canonical_tool_name("ApplyPatch"), "apply_patch");
        assert!(is_edit_tool_name("MultiEdit"));
        assert!(!is_edit_tool_name("read"));
    }

    #[test]
    fn resolves_display_names_for_oauth_pascal_case_aliases() {
        // Nested batch subcalls bypass the provider-side reverse map, so the
        // display layer must normalize the same PascalCase aliases the core
        // registry handles (issue #486 family).
        assert_eq!(resolve_display_tool_name("Read"), "read");
        assert_eq!(resolve_display_tool_name("Write"), "write");
        assert_eq!(resolve_display_tool_name("Edit"), "edit");
        assert_eq!(resolve_display_tool_name("Bash"), "bash");
        assert_eq!(resolve_display_tool_name("Grep"), "grep");
        assert_eq!(resolve_display_tool_name("Glob"), "glob");
        assert_eq!(resolve_display_tool_name("Agent"), "subagent");
        assert_eq!(resolve_display_tool_name("ScheduleWakeup"), "schedule");
        assert_eq!(resolve_display_tool_name("file_read"), "read");
        assert_eq!(resolve_display_tool_name("read_file"), "read");
        assert_eq!(resolve_display_tool_name("launch"), "open");
        assert_eq!(resolve_display_tool_name("shell"), "bash");
        assert_eq!(resolve_display_tool_name("todos"), "todo");
        assert_eq!(resolve_display_tool_name("Skill"), "skill_manage");
        assert_eq!(resolve_display_tool_name("skill"), "skill_manage");
    }

    #[test]
    fn display_name_stays_in_sync_with_canonical_name() {
        // The display label delegates to the canonical table; spot-check a few
        // provider-side spellings across both layers.
        for name in ["Read", "file_read", "Bash", "shell_exec", "Write", "Edit"] {
            assert_eq!(resolve_display_tool_name(name), canonical_tool_name(name));
        }
    }

    #[test]
    fn canonical_names_feed_edit_detection_consistently() {
        // PascalCase aliases canonicalize to the same lowercase names as the
        // native forms, so edit detection and summaries stay consistent.
        assert_eq!(canonical_tool_name("Read"), "read");
        assert_eq!(canonical_tool_name("Write"), "write");
        assert_eq!(canonical_tool_name("Edit"), "edit");
        assert_eq!(canonical_tool_name("Grep"), "grep");
        assert_eq!(canonical_tool_name("Agent"), "subagent");
        assert_eq!(canonical_tool_name("Skill"), "skill_manage");
        assert!(is_edit_tool_name("Write"));
        assert!(is_edit_tool_name("Edit"));
        assert!(is_edit_tool_name("Patch"));
        assert!(!is_edit_tool_name("Read"));
    }

    #[test]
    fn summarizes_tool_errors() {
        assert_eq!(
            concise_tool_error_summary("Error: missing field `command`").as_deref(),
            Some("invalid input: missing command")
        );
        assert_eq!(
            concise_tool_error_summary("--- Command finished with exit code: 2 ---").as_deref(),
            Some("exit 2")
        );
    }

    #[test]
    fn detects_failed_tool_output() {
        assert!(tool_output_looks_failed("Status: failed"));
        assert!(tool_output_looks_failed("Exit code: 1"));
        assert!(tool_output_looks_failed(
            "✗ demo.txt: failed to find expected lines"
        ));
        assert!(tool_output_looks_failed(
            "[apply_patch] ✗ demo.txt: failed to find expected lines"
        ));
        assert!(!tool_output_looks_failed("Exit code: 0"));
        assert!(!tool_output_looks_failed("completed successfully"));
    }
}
