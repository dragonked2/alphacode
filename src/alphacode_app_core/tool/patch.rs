use super::diff_utils::generate_diff_with_start;
use super::{Tool, ToolContext, ToolOutput};
use anyhow::{Context as _, Result, bail, ensure};
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::Path;
use tokio::io::AsyncWriteExt;

pub struct PatchTool;

impl PatchTool {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Debug)]
struct FilePatch<'a> {
    path: &'a str,
    hunks: Vec<Hunk<'a>>,
    is_new: bool,
    is_delete: bool,
}

#[derive(Debug)]
struct Hunk<'a> {
    old_start: usize,
    old_len: usize,
    lines: Vec<DiffLine<'a>>,
}

#[derive(Debug)]
struct DiffLine<'a> {
    kind: DiffKind,
    text: &'a str,
    has_newline: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DiffKind {
    Context,
    Remove,
    Add,
}

#[async_trait]
impl Tool for PatchTool {
    fn name(&self) -> &str {
        "patch"
    }

    fn description(&self) -> &str {
        "Apply a unified diff (---/+++ headers). Old lines and context must match the file at the given line numbers. Use apply_patch for Codex patches."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["patch_text"],
            "properties": {
                "patch_text": {
                    "type": "string",
                    "description": "Unified diff text."
                },
                "intent": super::intent_schema_property()
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        // Alias-tolerant extraction (parity with apply_patch): models often
        // send `patch`, `content`, or `diff` instead of `patch_text`.
        let patch_text = if let Some(obj) = input.as_object() {
            let mut found = None;
            for key in [
                "patch_text",
                "patch",
                "patch_content",
                "content",
                "diff",
                "text",
            ] {
                if let Some(value) = obj.get(key).and_then(|v| v.as_str()) {
                    found = Some(value);
                    break;
                }
            }
            if found.is_none() {
                // Rescue (parity with apply_patch): a confused model may file
                // the whole diff under the wrong key, most often `intent`.
                // Accept a string value that actually looks like a unified
                // diff instead of bouncing the call.
                for (key, value) in obj {
                    let Some(text) = value.as_str() else {
                        continue;
                    };
                    if looks_like_unified_diff(text) {
                        crate::logging::warn(&format!(
                            "patch: diff recovered from non-standard key '{key}'"
                        ));
                        found = Some(text);
                        break;
                    }
                }
            }
            match found {
                Some(text) => text,
                None => {
                    anyhow::bail!(
                        "missing field `patch_text`. {}. \
                         The diff itself belongs under `patch_text`; `intent` and similar \
                         labels are optional metadata, not the diff. Send the unified diff \
                         as a string under `patch_text`.",
                        super::describe_received_arguments(&input)
                    );
                }
            }
        } else {
            anyhow::bail!(
                "missing field `patch_text`: expected a JSON object with a `patch_text` string"
            );
        };
        let patches = parse_patch(patch_text)?;

        if patches.is_empty() {
            return Err(anyhow::anyhow!("No valid patches found in input"));
        }

        // Pre-flight catastrophic deny (parity with apply_patch): never touch
        // protected paths even though this tool takes unified diffs.
        let risk_ctx =
            crate::alphacode_command_risk::RiskContext::from_env(ctx.working_dir.clone());
        for patch in &patches {
            let resolved = ctx.resolve_path_guarded(Path::new(patch.path))?;
            if crate::alphacode_command_risk::is_catastrophic_target(&resolved, &risk_ctx) {
                return Err(anyhow::anyhow!(
                    "Refused: '{}' resolves to a protected path ({}). No changes applied.",
                    patch.path,
                    resolved.display()
                ));
            }
        }

        let mut results = Vec::new();
        let mut succeeded = 0usize;
        let mut failed = 0usize;
        let mut touched = Vec::new();

        for patch in patches {
            let resolved_path = ctx.resolve_path_guarded(Path::new(patch.path))?;
            let result = apply_patch_with_diff(&patch, &resolved_path).await;
            match result {
                Ok((msg, diff)) => {
                    succeeded += 1;
                    touched.push(patch.path);
                    if diff.is_empty() {
                        results.push(format!("✓ {}: {}", patch.path, msg));
                    } else {
                        results.push(format!("✓ {}: {}\n{}", patch.path, msg, diff));
                    }
                }
                Err(e) => {
                    failed += 1;
                    results.push(format!("✗ {}: {e:#}", patch.path));
                }
            }
        }

        if succeeded == 0 {
            return Err(anyhow::anyhow!(results.join("\n\n")));
        }
        let summary = format!(
            "{} succeeded, {} failed\n{}",
            succeeded,
            failed,
            results.join("\n\n")
        );
        let metadata = json!({
            "tool": "patch",
            "succeeded": succeeded,
            "failed": failed,
            "touched_paths": touched,
        });
        Ok(ToolOutput::new(summary)
            .with_file_paths(touched)
            .with_metadata(metadata))
    }
}

fn parse_patch(text: &str) -> Result<Vec<FilePatch<'_>>> {
    let mut patches = Vec::new();
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        let Some(old_file) = lines[i].strip_prefix("--- ") else {
            i += 1;
            continue;
        };
        let new_file = lines
            .get(i + 1)
            .and_then(|line| line.strip_prefix("+++ "))
            .with_context(|| format!("Expected a +++ file header at patch line {}", i + 2))?;
        let old_file = old_file.split('\t').next().unwrap_or_default();
        let new_file = new_file.split('\t').next().unwrap_or_default();
        let is_new = old_file == "/dev/null";
        let is_delete = new_file == "/dev/null";
        ensure!(
            !(old_file.is_empty() || new_file.is_empty() || is_new && is_delete),
            "Invalid file headers at patch line {}",
            i + 1
        );
        let path = if is_delete {
            old_file.strip_prefix("a/").unwrap_or(old_file)
        } else {
            new_file.strip_prefix("b/").unwrap_or(new_file)
        };
        i += 2;

        let mut hunks = Vec::new();
        loop {
            while lines.get(i) == Some(&"") {
                i += 1;
            }
            if !lines.get(i).is_some_and(|line| line.starts_with("@@")) {
                break;
            }
            let patch_line = i + 1;
            hunks.push(
                parse_hunk(&lines, &mut i)
                    .with_context(|| format!("{path}, patch line {patch_line}"))?,
            );
        }
        // A diff body ends at its declared counts. The next signed line can
        // only be another file header.
        if let Some(line) = lines.get(i)
            && line.starts_with([' ', '+', '-', '\\'])
            && *line != "-- "
            && !(line.starts_with("--- ")
                && lines
                    .get(i + 1)
                    .is_some_and(|line| line.starts_with("+++ ")))
        {
            bail!("{path}, patch line {}: unexpected line after hunk", i + 1);
        }
        if !hunks.is_empty() || is_new || is_delete {
            patches.push(FilePatch {
                path,
                hunks,
                is_new,
                is_delete,
            });
        }
    }

    Ok(patches)
}

/// Heuristic for "this string value is a unified diff, whatever key it came in
/// under". Ordinary prose never matches: it needs a real header or a hunk
/// header together with changed lines.
fn looks_like_unified_diff(text: &str) -> bool {
    let trimmed = text.trim_start();
    if trimmed.starts_with("diff --git") {
        return true;
    }
    if trimmed.starts_with("--- ") && text.contains("+++ ") {
        return true;
    }
    text.contains("@@") && (text.contains("\n+") || text.contains("\n-"))
}

fn parse_hunk<'a>(lines: &[&'a str], i: &mut usize) -> Result<Hunk<'a>> {
    let mut parts = lines[*i].split_whitespace();
    ensure!(parts.next() == Some("@@"), "Invalid hunk header");
    let (old_start, old_len) = parse_range(parts.next(), '-')?;
    let (_, new_len) = parse_range(parts.next(), '+')?;
    ensure!(
        parts.next() == Some("@@"),
        "Missing closing @@ in hunk header"
    );
    *i += 1;

    let mut old_remaining = old_len;
    let mut new_remaining = new_len;
    let mut body = Vec::new();
    while old_remaining != 0 || new_remaining != 0 {
        let line = lines.get(*i).context("Truncated hunk body")?;
        let (kind, text) = match line.as_bytes().first() {
            Some(b'-') => (DiffKind::Remove, &line[1..]),
            Some(b'+') => (DiffKind::Add, &line[1..]),
            Some(b' ') => (DiffKind::Context, &line[1..]),
            None => (DiffKind::Context, ""),
            _ => bail!("Invalid hunk body at patch line {}", *i + 1),
        };

        if kind != DiffKind::Add {
            old_remaining = old_remaining.checked_sub(1).context("Too many old lines")?;
        }
        if kind != DiffKind::Remove {
            new_remaining = new_remaining.checked_sub(1).context("Too many new lines")?;
        }
        *i += 1;
        let has_newline = lines.get(*i) != Some(&"\\ No newline at end of file");
        *i += usize::from(!has_newline);
        body.push(DiffLine {
            kind,
            text,
            has_newline,
        });
    }

    Ok(Hunk {
        old_start,
        old_len,
        lines: body,
    })
}

fn parse_range(range: Option<&str>, prefix: char) -> Result<(usize, usize)> {
    let range = range
        .and_then(|range| range.strip_prefix(prefix))
        .context("Missing line range in hunk header")?;
    let (start, count) = range.split_once(',').unwrap_or((range, "1"));
    let start = start.parse().context("Invalid line number")?;
    let count = count.parse().context("Invalid line count")?;
    ensure!(start != 0 || count == 0, "Nonempty range starts at line 0");
    Ok((start, count))
}

fn split_line_ending(line: &str) -> (&str, &'static str) {
    if let Some(text) = line.strip_suffix("\r\n") {
        (text, "\r\n")
    } else if let Some(text) = line.strip_suffix('\n') {
        (text, "\n")
    } else {
        (line, "")
    }
}

fn apply_hunks(content: &str, hunks: &[Hunk<'_>]) -> Result<String> {
    let source: Vec<&str> = content.split_inclusive('\n').collect();
    let default_ending = source
        .first()
        .map(|line| split_line_ending(line).1)
        .filter(|ending| !ending.is_empty())
        .unwrap_or("\n");
    let mut next_old = 0;
    let mut output = String::with_capacity(content.len());

    for hunk in hunks {
        // A zero-length range inserts after the named old line.
        let start = hunk.old_start - usize::from(hunk.old_len != 0);
        ensure!(
            start >= next_old && start <= source.len() && hunk.old_len <= source.len() - start,
            "Hunk range at old line {} is out of bounds or overlaps an earlier hunk",
            hunk.old_start
        );
        if start != next_old {
            ensure_line_boundary(&output)?;
            output.extend(source[next_old..start].iter().copied());
        }
        next_old = start;

        for line in &hunk.lines {
            if line.kind == DiffKind::Add {
                ensure_line_boundary(&output)?;
                output.push_str(line.text);
                if line.has_newline {
                    output.push_str(default_ending);
                }
                continue;
            }

            let original = source[next_old];
            next_old += 1;
            let (text, ending) = split_line_ending(original);
            ensure!(
                line.text == text && line.has_newline != ending.is_empty(),
                "Patch context does not match line {next_old}. Re-read the file and regenerate the diff."
            );
            if line.kind == DiffKind::Context {
                ensure_line_boundary(&output)?;
                output.push_str(original);
            }
        }
    }

    if next_old < source.len() {
        ensure_line_boundary(&output)?;
        output.extend(source[next_old..].iter().copied());
    }
    Ok(output)
}

fn ensure_line_boundary(output: &str) -> Result<()> {
    ensure!(
        output.is_empty() || output.ends_with('\n'),
        "A line without a trailing newline must be the final line"
    );
    Ok(())
}

/// Validate every hunk against the same file snapshot before applying changes.
async fn apply_patch_with_diff(patch: &FilePatch<'_>, path: &Path) -> Result<(String, String)> {
    let old_content = if patch.is_new {
        String::new()
    } else {
        tokio::fs::read_to_string(path).await?
    };
    let new_content = apply_hunks(&old_content, &patch.hunks)?;

    let status = if patch.is_delete {
        ensure!(
            new_content.is_empty(),
            "Delete hunk must remove the entire file"
        );
        tokio::fs::remove_file(path).await?;
        "deleted".to_string()
    } else if patch.is_new {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut file = tokio::fs::File::create_new(path).await?;
        file.write_all(new_content.as_bytes()).await?;
        file.flush().await?;
        "created".to_string()
    } else {
        if new_content != old_content {
            tokio::fs::write(path, &new_content).await?;
        }
        format!("modified ({} hunks)", patch.hunks.len())
    };

    let diff = generate_diff_with_start(&old_content, &new_content, 1);
    Ok((status, diff))
}

#[cfg(test)]
mod patch_extraction_tests {
    use super::{apply_hunks, apply_patch_with_diff, looks_like_unified_diff, parse_patch};
    use serde_json::json;

    #[test]
    fn unified_diff_rescued_from_intent_key() {
        let diff = "@@ -1,1 +1,1 @@\n-old\n+new\n";
        assert!(looks_like_unified_diff(diff));
        assert!(!looks_like_unified_diff(
            "fix the login bug by editing the auth module"
        ));
    }

    #[test]
    fn git_style_header_matches() {
        assert!(looks_like_unified_diff(
            "diff --git a/x.rs b/x.rs\n--- a/x.rs\n+++ b/x.rs\n@@ -1 +1 @@\n-a\n+b\n"
        ));
    }

    #[test]
    fn intent_label_prose_never_matches() {
        assert!(!looks_like_unified_diff(
            "intent: update the config loader to skip comment-only lines"
        ));
    }

    #[test]
    fn patch_tool_bail_names_patch_text_and_metadata_role() {
        // Drive the bail path through extraction failure is internal to
        // `execute`; here we at least assert the helper keeps prose out of the
        // rescue lane, which decides between fix and bounce.
        let input = json!({ "intent": "short UI label" });
        let obj = input.as_object().expect("object");
        assert!(
            obj.values()
                .filter_map(|v| v.as_str())
                .all(|text| !looks_like_unified_diff(text))
        );
    }

    #[tokio::test]
    async fn stale_later_hunk_leaves_the_file_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("note.txt");
        let original = "first\nskip\nactual\n";
        tokio::fs::write(&file, original).await.unwrap();
        let patches = parse_patch(
            "--- a/note.txt\n+++ b/note.txt\n@@ -1 +1 @@\n-first\n+new\n@@ -3 +3 @@\n-stale\n+wrong\n",
        )
        .unwrap();

        let error = apply_patch_with_diff(&patches[0], &file).await.unwrap_err();
        assert!(error.to_string().contains("does not match line 3"));
        assert_eq!(tokio::fs::read_to_string(&file).await.unwrap(), original);

        let corrected =
            parse_patch("--- a/note.txt\n+++ b/note.txt\n@@ -3 +3 @@\n-actual\n+correct\n")
                .unwrap();
        let (_, diff) = apply_patch_with_diff(&corrected[0], &file).await.unwrap();
        assert_eq!(diff, "3- actual\n3+ correct");
    }

    #[tokio::test]
    async fn stale_delete_does_not_remove_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("note.txt");
        tokio::fs::write(&file, "updated\n").await.unwrap();
        let patches = parse_patch("--- a/note.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-old\n").unwrap();
        assert!(apply_patch_with_diff(&patches[0], &file).await.is_err());
        assert_eq!(tokio::fs::read_to_string(file).await.unwrap(), "updated\n");
    }

    #[test]
    fn unified_diff_preserves_crlf_and_handles_header_like_text_and_eof() {
        let patches = parse_patch(
            "--- a/note.txt\n+++ b/note.txt\n@@ -1 +1 @@\n--- heading\n++++ new\n@@ -2 +2 @@\n-tail\n\\ No newline at end of file\n+updated\n\\ No newline at end of file\n",
        )
        .unwrap();
        assert_eq!(
            apply_hunks("-- heading\r\ntail", &patches[0].hunks).unwrap(),
            "+++ new\r\nupdated"
        );
    }

    #[test]
    fn unified_diff_handles_zero_length_ranges_and_rejects_out_of_bounds_ranges() {
        let patches =
            parse_patch("--- a/note.txt\n+++ b/note.txt\n@@ -1,0 +2 @@\n+inserted\n").unwrap();
        assert_eq!(
            apply_hunks("first\nlast\n", &patches[0].hunks).unwrap(),
            "first\ninserted\nlast\n"
        );
        assert!(apply_hunks("", &patches[0].hunks).is_err());
        assert!(
            parse_patch("--- a/note.txt\n+++ b/note.txt\n@@ -1,2 +1 @@\n-incomplete\n+new\n")
                .is_err()
        );
    }
}
