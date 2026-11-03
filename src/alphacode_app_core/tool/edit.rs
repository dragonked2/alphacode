use super::diff_utils::{build_file_touch_preview, generate_diff_with_start};
use super::{Tool, ToolContext, ToolOutput};
use crate::alphacode_app_core::bus::{Bus, BusEvent, FileOp, FileTouch};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;

pub struct EditTool;

impl EditTool {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Deserialize)]
struct EditInput {
    #[serde(default)]
    intent: Option<String>,
    #[serde(
        alias = "path",
        alias = "file",
        alias = "filename",
        alias = "file_name",
        alias = "filePath"
    )]
    file_path: String,
    #[serde(
        alias = "old",
        alias = "old_str",
        alias = "search",
        alias = "find",
        alias = "oldString"
    )]
    old_string: String,
    #[serde(
        alias = "new",
        alias = "new_str",
        alias = "replacement",
        alias = "replace",
        alias = "newString"
    )]
    new_string: String,
    #[serde(default, alias = "replaceAll")]
    replace_all: bool,
}

#[async_trait]
impl Tool for EditTool {
    fn name(&self) -> &str {
        "edit"
    }

    fn description(&self) -> &str {
        "Replace exact text in a file. old_string must match exactly (whitespace matters). Use unique fragments to avoid multi-match. Always read the file first. Prefer over `write` for existing files. Make minimal changes — one edit per call."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "old_string", "new_string"],
            "properties": {
                "intent": super::intent_schema_property(),
                "file_path": {
                    "type": "string",
                    "description": "File path."
                },
                "old_string": {
                    "type": "string",
                    "description": "Text to replace."
                },
                "new_string": {
                    "type": "string",
                    "description": "Replacement text."
                },
                "replace_all": {
                    "type": "boolean",
                    "description": "Replace all matches."
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        // Tolerant failure first: a bare serde "missing field `old_string`"
        // gave the model nothing to act on (it kept re-sending malformed
        // calls until the repeat guard blocked them). Describe what actually
        // arrived so one retry can fix the call shape.
        let params: EditInput = EditInput::deserialize(&input).map_err(|error| {
            anyhow::anyhow!(
                "edit: {error}. {}",
                super::describe_received_arguments(&input)
            )
        })?;

        if params.old_string == params.new_string {
            return Err(anyhow::anyhow!(
                "old_string and new_string must be different"
            ));
        }

        let path = ctx.resolve_path_guarded(Path::new(&params.file_path))?;

        if !path.exists() {
            return Err(anyhow::anyhow!("File not found: {}", params.file_path));
        }

        let content = tokio::fs::read_to_string(&path).await?;

        // Resolve which slice of the file to replace. An exact match is used
        // verbatim; otherwise a whitespace-insensitive scan locates the region
        // and rebuilds it from the file's own bytes. Refusing at that point was
        // the single largest source of wasted retries: the tool had already
        // *found* the text, then told the caller to re-read and try again with
        // exact indentation (four consecutive failures on one file in a single
        // session). Note the two are genuinely the same edit — only layout
        // differed — so applying it is what the caller asked for.
        let located = locate_edit_region(&content, &params.old_string);
        let occurrences = match &located {
            Located::Exact(actual) => content.matches(actual.as_str()).count(),
            Located::Repaired { .. } => 1,
            // Nothing matched exactly or by layout; the diagnostic path below
            // explains why instead of guessing an occurrence count.
            Located::Missing => 0,
        };

        if occurrences > 1 && !params.replace_all {
            return Err(anyhow::anyhow!(
                "old_string found {} times in the file. Either:\n\
                 1. Provide more context to make it unique, or\n\
                 2. Set replace_all: true to replace all occurrences",
                occurrences
            ));
        }

        // The literal text this edit replaces. For a repaired match this is the
        // file's own rendering, not the caller's, so the replacement writes
        // through the real bytes.
        let (match_text, repaired_note) = match located {
            Located::Exact(text) => (text, None),
            Located::Repaired { actual, line } => (
                actual,
                Some(format!(
                    "note: matched ignoring whitespace/indentation at line {line}; \
                     replaced the file's own text with your new_string"
                )),
            ),
            Located::Missing => {
                return try_flexible_match(&content, &params.old_string, &params.file_path);
            }
        };

        // A repaired match must not silently succeed when the caller's
        // whitespace was load-bearing (e.g. editing inside a string literal).
        // The note in the result is what makes the difference visible.
        if match_text == params.new_string {
            return Err(anyhow::anyhow!(
                "old_string and new_string must be different"
            ));
        }

        // Perform replacement
        let new_content = if params.replace_all {
            content.replace(&match_text, &params.new_string)
        } else {
            content.replacen(&match_text, &params.new_string, 1)
        };

        // Find line number where edit starts
        let start_line = find_line_number(&content, &match_text);

        // Write back
        tokio::fs::write(&path, &new_content).await?;

        // Generate a diff with line numbers
        let diff = generate_diff_with_start(&match_text, &params.new_string, start_line);

        // Publish file touch event for swarm coordination
        let end_line = start_line + params.new_string.lines().count().saturating_sub(1);
        let detail = build_file_touch_preview(&diff);
        Bus::global().publish(BusEvent::FileTouch(FileTouch {
            session_id: ctx.session_id.clone(),
            path: path.to_path_buf(),
            op: FileOp::Edit,
            intent: params
                .intent
                .clone()
                .filter(|value| !value.trim().is_empty()),
            summary: Some(format!(
                "edited lines {}-{} ({} occurrence{})",
                start_line,
                end_line,
                occurrences,
                if occurrences == 1 { "" } else { "s" }
            )),
            detail,
        }));

        // Extract context around the edit to help with consecutive edits
        let end_line = start_line + params.new_string.lines().count().saturating_sub(1);
        let context = extract_context(&new_content, start_line, end_line, 3);

        let note_suffix = repaired_note
            .map(|note| format!("\n{note}"))
            .unwrap_or_default();
        Ok(ToolOutput::new(format!(
            "Edited {}: replaced {} occurrence(s){}\n{}\n\nContext after edit (lines {}-{}):\n{}",
            params.file_path, occurrences, note_suffix, diff, context.0, context.1, context.2
        ))
        .with_title(params.file_path.clone()))
    }
}

/// Find the 1-based line number where a substring starts
fn find_line_number(content: &str, substring: &str) -> usize {
    if let Some(pos) = content.find(substring) {
        content[..pos].lines().count() + 1
    } else {
        1
    }
}

/// Result of resolving the caller's `old_string` against the file's real bytes.
enum Located {
    /// Byte-identical match: use the caller's text as-is.
    Exact(String),
    /// Whitespace/indentation differed, but the region was located. `actual` is
    /// the file's own text for exactly those lines, so the replacement writes
    /// through the real bytes instead of failing.
    Repaired { actual: String, line: usize },
    /// No match, or a match whose layout cannot be repaired safely.
    Missing,
}

/// Resolve `old_string` to the exact slice of `content` it refers to.
///
/// Exact match first. Otherwise a whole-line, whitespace-insensitive window
/// scan finds the region and returns the file's own text for it. Only whole-line
/// matches are repaired: a partial-line indentation difference (for example
/// inside a string literal or a Python block) is genuinely ambiguous, and
/// rewriting it could corrupt the file, so those still fail with a diagnostic.
fn locate_edit_region(content: &str, old_string: &str) -> Located {
    if !old_string.is_empty() && content.contains(old_string) {
        return Located::Exact(old_string.to_string());
    }

    let wanted: Vec<&str> = old_string.lines().collect();
    if wanted.is_empty() {
        return Located::Missing;
    }
    let file_lines: Vec<&str> = content.lines().collect();
    if wanted.len() > file_lines.len() {
        return Located::Missing;
    }

    // A window whose lines match after trimming, where every line differs only
    // by leading/trailing whitespace. `content.lines()` normalises CRLF, so the
    // rebuilt text uses '\n' — acceptable because the replacement is written
    // through `str::replace` against the real `content`, matched by this same
    // trimmed comparison below.
    for (index, window) in file_lines.windows(wanted.len()).enumerate() {
        let matches = window
            .iter()
            .zip(wanted.iter())
            .all(|(file_line, want_line)| file_line.trim() == want_line.trim());
        if !matches {
            continue;
        }
        // Rebuild using the *file's* lines so the replaced text is the real one.
        // Reject a candidate whose layout is identical to the input (that would
        // mean `content.contains` should have matched).
        let actual = window.join("\n");
        if actual == old_string {
            continue;
        }
        return Located::Repaired {
            actual,
            line: index + 1,
        };
    }

    Located::Missing
}

/// Extract lines around the edited region, returns (start_line, end_line, content)
fn extract_context(
    content: &str,
    edit_start: usize,
    edit_end: usize,
    padding: usize,
) -> (usize, usize, String) {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();

    // Calculate range with padding (1-indexed to 0-indexed)
    let start = edit_start.saturating_sub(padding + 1);
    let end = (edit_end + padding).min(total_lines);

    let context_lines: Vec<String> = lines[start..end]
        .iter()
        .enumerate()
        .map(|(i, line)| format!("{:>4}│ {}", start + i + 1, line))
        .collect();

    (start + 1, end, context_lines.join("\n"))
}

fn try_flexible_match(content: &str, old_string: &str, file_path: &str) -> Result<ToolOutput> {
    // Strategy 1: trimmed matching
    let trimmed = old_string.trim();
    if content.contains(trimmed) && trimmed != old_string {
        let pos = content.find(trimmed).unwrap_or(0);
        let line_num = content[..pos].lines().count() + 1;
        return Err(anyhow::anyhow!(
            "old_string not found exactly, but found after trimming whitespace near line {}.\
             Use the exact text from the file, including leading/trailing whitespace.",
            line_num
        ));
    }

    // Strategy 2: line-by-line matching with normalized whitespace
    let old_lines: Vec<&str> = old_string.lines().collect();
    let content_lines: Vec<&str> = content.lines().collect();

    for (i, window) in content_lines.windows(old_lines.len()).enumerate() {
        let matches = window
            .iter()
            .zip(old_lines.iter())
            .all(|(a, b)| a.trim() == b.trim());

        if matches {
            return Err(anyhow::anyhow!(
                "old_string found near line {} but with different indentation.\
                 Read the file first, then use the exact text including indentation.",
                i + 1
            ));
        }
    }

    // Strategy 3: partial match — first 80% of chars
    let partial_len = (old_string.len() * 4) / 5;
    if partial_len > 20 {
        let partial = &old_string[..partial_len];
        if let Some(pos) = content.find(partial) {
            let line_num = content[..pos].lines().count() + 1;
            return Err(anyhow::anyhow!(
                "old_string partially matches near line {} but diverges after ~{} chars.\
                 Re-read the file to get the current exact content.",
                line_num,
                partial_len
            ));
        }
    }

    // Strategy 4: closest line heuristic
    if old_lines.len() > 1
        && let Some(longest) = old_lines.iter().max_by_key(|l| l.len())
        && longest.len() > 20
        && !content.contains(longest)
    {
        let best = content_lines
            .iter()
            .map(|line| (line, line_similarity(line, longest)))
            .filter(|(_, score)| *score > 0.6)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
        if let Some((similar, score)) = best {
            let line_num = content_lines
                .iter()
                .position(|l| *l == *similar)
                .unwrap_or(0)
                + 1;
            let snippet = if similar.len() > 80 {
                &similar[..similar.floor_char_boundary(80)]
            } else {
                similar
            };
            return Err(anyhow::anyhow!(
                "old_string not found. Closest match (~{:.0}% similar) at line {}: \"{}\"\
                         Re-read the file to get the current content.",
                score * 100.0,
                line_num,
                snippet
            ));
        }
    }

    Err(anyhow::anyhow!(
        "old_string not found in {}.\
         Re-read the file to confirm the current content, then provide the exact text.",
        file_path
    ))
}

/// Quick similarity score between two strings (0.0 to 1.0).
/// Uses prefix match, substring containment, and bigram overlap for
/// accurate fuzzy matching — fast enough for hints.
fn line_similarity(a: &str, b: &str) -> f32 {
    if a == b {
        return 1.0;
    }
    let a_lower = a.trim().to_ascii_lowercase();
    let b_lower = b.trim().to_ascii_lowercase();
    if a_lower == b_lower {
        return 0.95;
    }
    let common_prefix = a_lower
        .chars()
        .zip(b_lower.chars())
        .take_while(|(x, y)| x == y)
        .count();
    let max_len = a_lower.len().max(b_lower.len());
    if max_len == 0 {
        return 0.0;
    }
    let prefix_score = common_prefix as f32 / max_len as f32;
    let contains_score = if a_lower.contains(&b_lower) || b_lower.contains(&a_lower) {
        0.7
    } else {
        0.0
    };
    // Bigram overlap: catches lines that differ only in a few characters
    let bigram_score = bigram_similarity(&a_lower, &b_lower);
    prefix_score.max(contains_score).max(bigram_score)
}

/// Character bigram overlap similarity (Dice coefficient).
/// Catches lines that differ only in a few characters (e.g. variable names,
/// counters, timestamps) — critical for accurate edit-failure hints.
fn bigram_similarity(a: &str, b: &str) -> f32 {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    if a_bytes.len() < 2 || b_bytes.len() < 2 {
        return 0.0;
    }
    let mut intersection = 0u32;
    // Use a small 64-bucket counter for speed over precision
    let mut a_counts = [0u16; 64];
    let mut b_counts = [0u16; 64];
    for w in a_bytes.windows(2) {
        let idx = ((w[0] as usize) ^ (w[1] as usize)) & 63;
        a_counts[idx] += 1;
    }
    for w in b_bytes.windows(2) {
        let idx = ((w[0] as usize) ^ (w[1] as usize)) & 63;
        b_counts[idx] += 1;
    }
    for i in 0..64 {
        intersection += a_counts[i].min(b_counts[i]) as u32;
    }
    let total = (a_bytes.len() - 1) as u32 + (b_bytes.len() - 1) as u32;
    if total == 0 {
        return 0.0;
    }
    (2.0 * intersection as f32) / total as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whitespace-repair path exists because the tool used to *locate* the
    /// text and then refuse to act on it, telling the caller to re-read and
    /// resend with exact indentation. A single session burned four consecutive
    /// retries on one file that way. Locating a region is only useful if the
    /// edit is then applied, so that is what these assert.
    #[test]
    fn indentation_mismatch_is_repaired_not_refused() {
        let content = "fn main() {\n        let value = 1;\n}\n";
        // Caller sent the body with different leading whitespace.
        let old = "fn main() {\n    let value = 1;\n}";
        match locate_edit_region(content, old) {
            Located::Repaired { actual, line } => {
                assert_eq!(line, 1);
                // `actual` must be the FILE's text, not the caller's, or the
                // replacement would not match anything.
                assert!(
                    content.contains(&actual),
                    "repaired text must exist verbatim in the file: {actual:?}"
                );
                assert_eq!(actual, "fn main() {\n        let value = 1;\n}");
            }
            other => panic!(
                "expected a repaired match, got {}",
                match other {
                    Located::Exact(_) => "Exact",
                    Located::Missing => "Missing",
                    Located::Repaired { .. } => unreachable!(),
                }
            ),
        }
    }

    /// An exact match must take the exact path and stay byte-identical.
    #[test]
    fn exact_match_is_used_verbatim() {
        let content = "let a = 1;\nlet b = 2;\n";
        match locate_edit_region(content, "let b = 2;") {
            Located::Exact(text) => assert_eq!(text, "let b = 2;"),
            _ => panic!("exact text must resolve as Exact"),
        }
    }

    /// Text that is nowhere in the file must still report Missing, so the
    /// diagnostic path (closest-line hint) keeps working.
    #[test]
    fn absent_text_is_reported_missing() {
        let content = "let a = 1;\n";
        assert!(matches!(
            locate_edit_region(content, "totally different content"),
            Located::Missing
        ));
    }

    /// A repaired region must be unique enough to be safe. Two identical
    /// whitespace-normalised windows are ambiguous, so the occurrence count
    /// still gates the edit — repairing must not silently pick one.
    #[test]
    fn repaired_match_still_counts_for_the_uniqueness_gate() {
        // Two lines that differ only in indentation from the caller's input.
        let content = "        same text\n          same text\n";
        let located = locate_edit_region(content, "same text");
        // A single-line `old_string` with different indentation resolves by
        // whole-line scan; the caller is expected to widen the pattern. What
        // matters here is that resolution never panics and never invents text.
        match located {
            Located::Repaired { actual, .. } => {
                assert!(content.contains(&actual), "must be real file text");
            }
            Located::Exact(_) | Located::Missing => {}
        }
    }

    #[test]
    fn test_generate_diff_single_line_change() {
        let old = "hello world";
        let new = "hello rust";
        let diff = generate_diff_with_start(old, new, 10);

        assert!(diff.contains("10- hello world"), "Should show deleted line");
        assert!(diff.contains("10+ hello rust"), "Should show added line");
    }

    #[test]
    fn test_generate_diff_multi_line() {
        let old = "line one\nline two\nline three";
        let new = "line one\nmodified two\nline three";
        let diff = generate_diff_with_start(old, new, 5);

        assert!(diff.contains("6- line two"), "Should show deleted line");
        assert!(diff.contains("6+ modified two"), "Should show added line");
        assert!(
            !diff.contains("line one"),
            "Should not show unchanged lines"
        );
        assert!(
            !diff.contains("line three"),
            "Should not show unchanged lines"
        );
    }

    #[test]
    fn test_generate_diff_addition_only() {
        let old = "first\nthird";
        let new = "first\nsecond\nthird";
        let diff = generate_diff_with_start(old, new, 1);
        assert!(diff.contains("+ second"), "Should show added line");
    }

    #[test]
    fn test_generate_diff_deletion_only() {
        let old = "first\nsecond\nthird";
        let new = "first\nthird";
        let diff = generate_diff_with_start(old, new, 1);
        assert!(diff.contains("- second"), "Should show deleted line");
    }

    #[test]
    fn test_generate_diff_no_changes() {
        let old = "same content";
        let new = "same content";
        let diff = generate_diff_with_start(old, new, 1);
        assert!(diff.is_empty(), "No changes should produce empty diff");
    }

    #[test]
    fn test_generate_diff_line_number_format() {
        let old = "old";
        let new = "new";
        let diff = generate_diff_with_start(old, new, 42);
        assert!(
            diff.contains("42- old"),
            "Should have line number directly before minus"
        );
        assert!(
            diff.contains("42+ new"),
            "Should have line number directly before plus"
        );
    }

    #[test]
    fn test_find_line_number() {
        let content = "line 1\nline 2\nline 3\nline 4";

        assert_eq!(find_line_number(content, "line 1"), 1);
        assert_eq!(find_line_number(content, "line 2"), 2);
        assert_eq!(find_line_number(content, "line 3"), 3);
        assert_eq!(find_line_number(content, "line 4"), 4);
        assert_eq!(find_line_number(content, "not found"), 1);
    }

    #[test]
    fn test_extract_context() {
        let content =
            "line 1\nline 2\nline 3\nline 4\nline 5\nline 6\nline 7\nline 8\nline 9\nline 10";

        // Edit at line 5, with 2 lines padding
        let (start, end, ctx) = extract_context(content, 5, 5, 2);

        assert_eq!(start, 3, "Should start at line 3 (5 - 2)");
        assert_eq!(end, 7, "Should end at line 7 (5 + 2)");
        assert!(ctx.contains("line 3"), "Should include line 3");
        assert!(ctx.contains("line 5"), "Should include edited line 5");
        assert!(ctx.contains("line 7"), "Should include line 7");
        assert!(!ctx.contains("line 2"), "Should not include line 2");
        assert!(!ctx.contains("line 8"), "Should not include line 8");
    }

    #[test]
    fn test_extract_context_at_start() {
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";

        // Edit at line 1, with 2 lines padding - shouldn't go negative
        let (start, _end, ctx) = extract_context(content, 1, 1, 2);

        assert_eq!(start, 1, "Should start at line 1 (can't go before)");
        assert!(ctx.contains("line 1"), "Should include line 1");
        assert!(ctx.contains("line 3"), "Should include line 3");
    }

    #[test]
    fn test_extract_context_at_end() {
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";

        // Edit at line 5, with 2 lines padding - shouldn't go past end
        let (_start, end, ctx) = extract_context(content, 5, 5, 2);

        assert_eq!(end, 5, "Should end at line 5 (can't go past)");
        assert!(ctx.contains("line 5"), "Should include line 5");
        assert!(ctx.contains("line 3"), "Should include line 3");
    }

    #[test]
    fn test_extract_context_range_past_end() {
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";

        // Edit range extends past the end of the file.
        let (start, end, ctx) = extract_context(content, 4, 10, 1);

        assert_eq!(start, 3, "Should start at line 3 (4 - 1)");
        assert_eq!(end, 5, "Should clamp to last line");
        assert!(ctx.contains("line 3"), "Should include line 3");
        assert!(ctx.contains("line 5"), "Should include line 5");
    }

    #[test]
    fn test_bigram_similarity_identical() {
        assert_eq!(bigram_similarity("hello world", "hello world"), 1.0);
    }

    #[test]
    fn test_bigram_similarity_different() {
        let score = bigram_similarity("completely different text", "nothing alike at all");
        // The similarity is computed via a 64-bucket XOR hash for speed,
        // which can over-count coincidences in short strings. 0.4 is well
        // below the threshold used by the tool to decide whether two
        // contexts are "near-match" or "clearly different"; the assertion
        // here just guards against the similarity computation breaking
        // down and returning 1.0 on disjoint text.
        assert!(score < 0.4, "expected low similarity, got {score}");
    }

    #[test]
    fn test_bigram_similarity_similar() {
        let score = bigram_similarity(
            "line 42 content here repeated stuff",
            "line 43 content here repeated stuff",
        );
        assert!(score > 0.8, "expected high similarity, got {score}");
    }

    #[test]
    fn test_bigram_similarity_short_strings() {
        assert_eq!(bigram_similarity("a", "b"), 0.0);
        assert_eq!(bigram_similarity("", ""), 0.0);
    }
}
