//! Shared diff rendering and file-touch preview utilities for file-editing tools.
//!
//! These helpers were previously duplicated across `write`, `edit`, `multiedit`,
//! `patch`, and `apply_patch`.  Centralising them cuts ~200 lines of boilerplate
//! and ensures consistent formatting.

use similar::{ChangeTag, TextDiff};

/// Maximum number of diff lines returned by [`generate_diff_summary`].
pub const DIFF_MAX_LINES: usize = 30;

/// Maximum size, in bytes, of content [`generate_diff_summary`] will be asked
/// to diff.
///
/// The rendered diff is capped at [`DIFF_MAX_LINES`] lines, so past a certain
/// input size the output is a truncated "everything changed" summary either way.
/// The real cost is upstream: `similar` builds its line index over the whole
/// input, and callers such as the `write` tool must first hold the old content
/// in memory to diff against. That work is proportional to input size and is
/// pure overhead once the rendered output cannot grow. 1 MiB is far larger
/// than any file whose diff is meaningfully shown, and small enough that the
/// worst case stays a bounded allocation.
pub const DIFF_MAX_INPUT_BYTES: usize = 1024 * 1024;

/// Whether an on-disk file is small enough that loading it for a diff is
/// worthwhile. Checks metadata so the content is never read just to measure it.
pub async fn file_within_diff_size_limit(path: &std::path::Path) -> bool {
    match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata.is_file() && metadata.len() <= DIFF_MAX_INPUT_BYTES as u64,
        // If the size cannot be determined, do not read the file: the fallback
        // (no diff) is strictly safer than an unbounded allocation.
        Err(_) => false,
    }
}

/// Whether in-memory content is small enough to diff.
pub fn content_within_diff_size_limit(content: &str) -> bool {
    content.len() <= DIFF_MAX_INPUT_BYTES
}

const TOUCH_PREVIEW_MAX_LINES: usize = 6;
const TOUCH_PREVIEW_MAX_BYTES: usize = 240;

/// Generate a compact, human-readable diff: `42- old` / `42+ new`.
///
/// At most [`DIFF_MAX_LINES`] non-empty change lines are included; the output
/// is truncated with `...` when the limit is reached.
pub fn generate_diff_summary(old: &str, new: &str) -> String {
    if old == new {
        return String::new();
    }
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
                let _ =
                    std::fmt::write(&mut output, format_args!("{}- {}\n", old_line - 1, content));
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
                let _ =
                    std::fmt::write(&mut output, format_args!("{}+ {}\n", new_line - 1, content));
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
    if old == new {
        return String::new();
    }
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

        let _ = std::fmt::write(
            &mut output,
            format_args!("{}{} {}\n", line_num, prefix, content),
        );
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
        let old = (1..=35)
            .map(|i| format!("old {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let new = (1..=35)
            .map(|i| format!("new {}", i))
            .collect::<Vec<_>>()
            .join("\n");
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
        let long = (0..20)
            .map(|i| format!("{}+ line {}", i, i))
            .collect::<Vec<_>>()
            .join("\n");
        let preview = build_file_touch_preview(&long).unwrap();
        assert!(preview.contains("…"));
    }
}

#[cfg(test)]
mod diff_size_limit_tests {
    use super::*;

    #[test]
    fn content_within_limit_accepts_small_and_rejects_large() {
        assert!(content_within_diff_size_limit("small"));
        let at_limit = "a".repeat(DIFF_MAX_INPUT_BYTES);
        assert!(content_within_diff_size_limit(&at_limit));
        let over_limit = "a".repeat(DIFF_MAX_INPUT_BYTES + 1);
        assert!(!content_within_diff_size_limit(&over_limit));
    }

    /// The point of the cap: a file past the limit must be rejected on metadata
    /// alone, so its content is never read into memory.
    #[tokio::test]
    async fn oversized_file_is_rejected_without_reading() {
        let dir = std::env::temp_dir().join("ac_diff_limit_test");
        tokio::fs::create_dir_all(&dir).await.expect("create dir");
        let path = dir.join("big.txt");
        tokio::fs::write(&path, vec![b'x'; DIFF_MAX_INPUT_BYTES + 1])
            .await
            .expect("write file");

        assert!(!file_within_diff_size_limit(&path).await);

        // A file that does not exist must also fail closed rather than reading.
        assert!(!file_within_diff_size_limit(&dir.join("missing.txt")).await);

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn small_file_is_within_limit() {
        let dir = std::env::temp_dir().join("ac_diff_limit_ok_test");
        tokio::fs::create_dir_all(&dir).await.expect("create dir");
        let path = dir.join("small.txt");
        tokio::fs::write(&path, b"hello").await.expect("write file");

        assert!(file_within_diff_size_limit(&path).await);

        tokio::fs::remove_dir_all(&dir).await.ok();
    }
}
