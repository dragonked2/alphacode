//! Shared diff rendering and file-touch preview utilities for file-editing tools.
//!
//! These helpers were previously duplicated across `write`, `edit`, `multiedit`,
//! `patch`, and `apply_patch`.  Centralising them cuts ~200 lines of boilerplate
//! and ensures consistent formatting.

use similar::{ChangeTag, TextDiff};

/// Maximum number of diff lines returned by [`generate_diff_summary`].
pub const DIFF_MAX_LINES: usize = 30;

const TOUCH_PREVIEW_MAX_LINES: usize = 6;
const TOUCH_PREVIEW_MAX_BYTES: usize = 240;

/// Generate a compact, human-readable diff: `42- old` / `42+ new`.
///
/// At most [`DIFF_MAX_LINES`] non-empty change lines are included; the output
/// is truncated with `...` when the limit is reached.
pub fn generate_diff_summary(old: &str, new: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    let mut output = String::new();
    let mut lines_shown = 0usize;

    let mut old_line = 1usize;
    let mut new_line = 1usize;

    for change in diff.iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                old_line += 1;
                new_line += 1;
                continue;
            }
            ChangeTag::Delete => {
                let content = change.value().trim();
                old_line += 1;
                if content.is_empty() {
                    continue;
                }
                if lines_shown >= DIFF_MAX_LINES {
                    output.push_str("...\n");
                    break;
                }
                let _ = std::fmt::write(&mut output, format_args!("{}- {}\n", old_line - 1, content));
                lines_shown += 1;
            }
            ChangeTag::Insert => {
                let content = change.value().trim();
                new_line += 1;
                if content.is_empty() {
                    continue;
                }
                if lines_shown >= DIFF_MAX_LINES {
                    output.push_str("...\n");
                    break;
                }
                let _ = std::fmt::write(&mut output, format_args!("{}+ {}\n", new_line - 1, content));
                lines_shown += 1;
            }
        }
    }

    output.trim_end().to_string()
}

/// Generate a compact diff with line numbers starting at `start_line`.
///
/// This variant is used by `edit` and `patch` where the diff should show the
/// line numbers as they appear in the file rather than from line 1.
pub fn generate_diff_with_start(old: &str, new: &str, start_line: usize) -> String {
    let diff = TextDiff::from_lines(old, new);
    let mut output = String::new();
    let mut line_count = 0usize;

    let mut old_line = start_line;
    let mut new_line = start_line;

    for change in diff.iter_all_changes() {
        if line_count >= DIFF_MAX_LINES {
            output.push_str("... (diff truncated)\n");
            break;
        }

        let content = change.value().trim_end_matches('\n');
        let (prefix, line_num) = match change.tag() {
            ChangeTag::Delete => {
                let num = old_line;
                old_line += 1;
                if content.trim().is_empty() {
                    continue;
                }
                ("-", num)
            }
            ChangeTag::Insert => {
                let num = new_line;
                new_line += 1;
                if content.trim().is_empty() {
                    continue;
                }
                ("+", num)
            }
            ChangeTag::Equal => {
                old_line += 1;
                new_line += 1;
                continue;
            }
        };

        let _ = std::fmt::write(&mut output, format_args!("{}{} {}\n", line_num, prefix, content));
        line_count += 1;
    }

    output.trim_end().to_string()
}

/// Build a short preview of a diff string for the `FileTouch` event bus.
///
/// Returns `None` when the diff is empty.
pub fn build_file_touch_preview(diff: &str) -> Option<String> {
    let trimmed = diff.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut lines = trimmed.lines();
    let mut preview = lines
        .by_ref()
        .take(TOUCH_PREVIEW_MAX_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    let mut truncated = lines.next().is_some();

    if preview.len() > TOUCH_PREVIEW_MAX_BYTES {
        preview = crate::util::truncate_str(&preview, TOUCH_PREVIEW_MAX_BYTES)
            .trim_end()
            .to_string();
        truncated = true;
    }

    if truncated {
        preview.push_str("\n…");
    }

    Some(preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_summary_single_change() {
        let diff = generate_diff_summary("hello world", "hello rust");
        assert!(diff.contains("1- hello world"));
        assert!(diff.contains("1+ hello rust"));
    }

    #[test]
    fn diff_summary_multi_line() {
        let old = "line one\nline two\nline three";
        let new = "line one\nchanged two\nline three";
        let diff = generate_diff_summary(old, new);
        assert!(diff.contains("2- line two"));
        assert!(diff.contains("2+ changed two"));
        assert!(!diff.contains("line one"), "equal lines should be omitted");
    }

    #[test]
    fn diff_summary_new_file() {
        let diff = generate_diff_summary("", "a\nb\nc");
        assert!(diff.contains("1+ a"));
        assert!(diff.contains("2+ b"));
        assert!(diff.contains("3+ c"));
    }

    #[test]
    fn diff_summary_truncation() {
        let old = (1..=35).map(|i| format!("old {}", i)).collect::<Vec<_>>().join("\n");
        let new = (1..=35).map(|i| format!("new {}", i)).collect::<Vec<_>>().join("\n");
        let diff = generate_diff_summary(&old, &new);
        assert!(diff.contains("..."));
    }

    #[test]
    fn diff_summary_empty_when_equal() {
        assert!(generate_diff_summary("same", "same").is_empty());
    }

    #[test]
    fn diff_with_start_correct_offsets() {
        let diff = generate_diff_with_start("old", "new", 42);
        assert!(diff.contains("42- old"));
        assert!(diff.contains("42+ new"));
    }

    #[test]
    fn touch_preview_none_for_empty() {
        assert!(build_file_touch_preview("").is_none());
        assert!(build_file_touch_preview("  \n  ").is_none());
    }

    #[test]
    fn touch_preview_short_diff() {
        let preview = build_file_touch_preview("1+ added line\n2+ another").unwrap();
        assert!(preview.contains("1+ added line"));
        assert!(!preview.contains("…"));
    }

    #[test]
    fn touch_preview_long_diff_truncated() {
        let long = (0..20).map(|i| format!("{}+ line {}", i, i)).collect::<Vec<_>>().join("\n");
        let preview = build_file_touch_preview(&long).unwrap();
        assert!(preview.contains("…"));
    }
}
