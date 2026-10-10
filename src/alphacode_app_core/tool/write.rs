use super::diff_utils::{
    DIFF_MAX_INPUT_BYTES, build_file_touch_preview, content_within_diff_size_limit,
    file_within_diff_size_limit, generate_diff_summary,
};
use super::{Tool, ToolContext, ToolOutput};
use crate::alphacode_app_core::bus::{Bus, BusEvent, FileOp, FileTouch};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::Path;
use tokio::io::AsyncWriteExt;

pub struct WriteTool;

impl WriteTool {
    pub fn new() -> Self {
        Self
    }
}

/// `intent`/`append` are only ever carried through to the file-touch event and
/// write mode, so the text fields that matter are recovered by the shared coercion ladder in
/// `execute` rather than by a single-shot `from_value`.
struct WriteInput {
    intent: Option<String>,
    file_path: String,
    content: String,
    append: bool,
}

/// Whether the argument stream was cut short and the payload rebuilt from
/// what survived.
///
/// A `write` body is the single most likely argument in a session to exceed
/// the provider's `max_tokens`, so this is not a rare corner. When it happens
/// the recovered content is a *prefix* of what the model meant to write, and
/// the distinction matters: writing a prefix to a new file loses nothing, but
/// writing one over an existing file destroys content that cannot be
/// recovered. The second case is refused rather than performed.
fn input_was_truncated(input: &Value) -> bool {
    input
        .get(crate::alphacode_message_types::TRUNCATED_INPUT_MARKER)
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// Whether the payload carried a `content` field under any accepted spelling.
///
/// Used to tell "the model asked for an empty file" apart from "the model
/// forgot the body". The two are very different: one is a completed request,
/// the other is a mistake worth reporting.
fn input_has_content_field(input: &Value) -> bool {
    [
        "content",
        "text",
        "data",
        "body",
        "file_content",
        "contents",
        "new_content",
        "newContent",
    ]
    .iter()
    .any(|key| input.get(*key).and_then(Value::as_str).is_some())
}

/// The error for a `write` whose arguments were cut off before the body arrived.
///
/// Distinct from [`missing_content_error`] because the fix is different. The
/// model did send a body; it was truncated by the provider's output token
/// limit. Told only "missing field `content`", it resends the identical
/// oversized call — which truncates again, which is the loop the repeat guard
/// eventually has to break.
fn truncated_body_error(file_path: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "Nothing was written to `{file_path}`: this call's arguments were cut off by the output \
         token limit before `content` finished arriving, so no usable body was recovered. Re-send \
         a smaller first chunk, then pass `append: true` on later chunks rather than resending the \
         identical oversized call."
    )
}

/// The error for a `write` that named its destination but not its body.
fn missing_content_error(file_path: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "missing field `content` for file `{file_path}`. \
         Provide the file body as `content`, e.g. \
         {{\"file_path\": \"{file_path}\", \"content\": \"...\"}}"
    )
}

#[async_trait]
impl Tool for WriteTool {
    fn name(&self) -> &str {
        "write"
    }

    fn description(&self) -> &str {
        "Create or overwrite a file. Destructive: replaces entire content. For large files, write the first chunk normally, then send each later chunk with append=true; do not resend the whole file. Use `edit` for targeted replacements. Write only what solves the problem — no boilerplate or unused scaffolding."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "content"],
            "properties": {
                "intent": super::intent_schema_property(),
                "file_path": {
                    "type": "string",
                    "description": "File path."
                },
                "content": {
                    "type": "string",
                    "description": "File content."
                },
                "append": {
                    "type": "boolean",
                    "description": "Append this content exactly to the end of the file instead of replacing it. Use true for every chunk after the first; include a newline when chunks should be separated."
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        // Captured before `extract_write_input` normalizes the payload away.
        let salvaged = input_was_truncated(&input);
        // `file_path` is recovered first so a payload that named the file under
        // an alias — or arrived as a truncated stream — reaches the content
        // lookup with the destination known. Without the path, the old
        // single-shot `from_value` failed with "missing field `file_path`" and
        // the model retried the identical call until the repeat guard fired.
        let file_path = super::coerce_text_arg(
            &input,
            "write",
            "file_path",
            &[
                "path",
                "file",
                "filename",
                "file_name",
                "filePath",
                "filepath",
                "file-path",
                "dest",
                "destination",
            ],
        )?;
        // No single-string fallback here, unlike the path lookup above. `write` takes
        // two text arguments from one payload, so "the only string field is
        // `file_path`" means the body is *missing* — and adopting the path as
        // the content would write a file containing its own path. The path is
        // excluded from the search too, so `{"file_path": "a.rs", "text": "x"}`
        // cannot pick up the wrong one.
        let content = super::coerce_text_field(
            &input,
            "write",
            "content",
            &[
                "text",
                "data",
                "body",
                "file_content",
                "contents",
                "new_content",
                "newContent",
            ],
            false,
            super::PATH_EXAMPLE,
        )
        // An empty file is a legitimate request, so only a genuinely absent
        // field is an error — and it must name the file, not just the field.
        .or_else(|_| {
            if input_has_content_field(&input) {
                Ok(String::new())
            } else if salvaged {
                // The call *did* name the file; the body was cut off. Say so,
                // because the generic "missing field `content`" is what the
                // model has already seen once and will simply retry verbatim.
                Err(truncated_body_error(&file_path))
            } else {
                Err(missing_content_error(&file_path))
            }
        })?;
        let params = WriteInput {
            intent: input
                .get("intent")
                .and_then(Value::as_str)
                .map(str::to_string),
            file_path,
            content,
            append: input
                .get("append")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        };

        let path = ctx.resolve_path_guarded(Path::new(&params.file_path))?;

        // Create parent directories if needed
        if let Some(parent) = path.parent()
            && !parent.exists()
        {
            tokio::fs::create_dir_all(parent).await?;
        }

        let existed = path.exists();
        // Refuse to replace real content with a truncated prefix. Everything
        // that survived the cutoff is already on disk nowhere else, so this
        // has to fail loudly rather than guess. Writing the prefix to a *new*
        // file is safe and worth doing — the model reads its own output, sees
        // the warning, and continues the file on the next call.
        if salvaged && existed && !params.append {
            let existing = tokio::fs::metadata(&path)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            if existing > 0 {
                return Err(anyhow::anyhow!(
                    "Refusing to overwrite the existing {existing}-byte file `{}`: this call's \
                     arguments were cut off by the output token limit, so the recovered `content` is \
                     only a prefix. Nothing was written. Re-send `write` with a shorter `content` \
                     (or use `edit` to apply the change in pieces) so the file is not left \
                     half-written.",
                    params.file_path
                ));
            }
        }

        // Check if file existed before and read old content for diff.
        //
        // The previous content is only used to render a human-readable diff, and
        // that diff is capped at DIFF_MAX_LINES. Reading a multi-megabyte file
        // into memory purely to throw almost all of it away costs both the
        // allocation and the O(n*m) diff work, so skip the read entirely once
        // the file is past the point where the diff would be truncated anyway.
        // The size is checked from metadata first, so the content is never
        // loaded in the first place.
        let old_content = if !params.append && existed && file_within_diff_size_limit(&path).await {
            tokio::fs::read_to_string(&path).await.ok()
        } else {
            None
        };
        // A missing old snapshot is ambiguous: either the file is new, or it is
        // too large to diff. Distinguish them so the output does not claim a
        // large file was "created" when it was only overwritten.
        let diff_skipped = !params.append && existed && old_content.is_none();

        // Append mode makes oversized files recoverable across several tool
        // calls without asking the model to resend or replace earlier chunks.
        if params.append {
            let mut file = tokio::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .await?;
            file.write_all(params.content.as_bytes()).await?;
        } else {
            tokio::fs::write(&path, &params.content).await?;
        }

        let _new_len = params.content.len();
        let line_count = params.content.lines().count();
        // Skip the diff when it was skipped above, and also when the *new*
        // content alone is too large to diff against nothing.
        let diff = if diff_skipped || !content_within_diff_size_limit(&params.content) {
            format!(
                "(diff skipped: file exceeds the {} KB diff limit; use read to inspect changes)",
                DIFF_MAX_INPUT_BYTES / 1024
            )
        } else if let Some(old) = old_content.as_deref() {
            generate_diff_summary(old, &params.content)
        } else {
            generate_diff_summary("", &params.content)
        };
        let detail = build_file_touch_preview(&diff);

        // Publish file touch event for swarm coordination
        Bus::global().publish(BusEvent::FileTouch(FileTouch {
            session_id: ctx.session_id.clone(),
            path: path.to_path_buf(),
            op: FileOp::Write,
            intent: params
                .intent
                .clone()
                .filter(|value| !value.trim().is_empty()),
            summary: Some(if params.append {
                format!(
                    "appended {} bytes ({} lines)",
                    params.content.len(),
                    line_count
                )
            } else if existed {
                format!("overwrote file ({} lines)", line_count)
            } else {
                format!("created new file ({} lines)", line_count)
            }),
            detail: if params.append {
                Some(format!("appended {} bytes", params.content.len()))
            } else {
                detail
            },
        }));

        // Only reachable with `salvaged == true` when the target did not exist,
        // because the overwrite case is refused above. Lead with the warning:
        // the model reads this output, and it needs to know the file is a
        // prefix rather than the finished article.
        let warning = if salvaged {
            format!(
                "WARNING: this call's arguments were truncated by the output token limit, so only \
                 the first {} bytes of `content` were recovered. This file is INCOMPLETE — read it, \
                 then continue writing the remainder with `append=true`.\n",
                params.content.len()
            )
        } else {
            String::new()
        };

        if params.append {
            Ok(ToolOutput::new(format!(
                "Appended {} bytes ({} lines) to {}{}",
                params.content.len(),
                line_count,
                params.file_path,
                if salvaged {
                    "\nWARNING: the received chunk was truncated; append the remaining content in another chunk."
                } else {
                    ""
                }
            ))
            .with_title(params.file_path.clone()))
        } else if existed {
            Ok(ToolOutput::new(format!(
                "{warning}Updated {} ({} lines){}\n{}",
                params.file_path,
                line_count,
                if diff.is_empty() { "" } else { ":" },
                diff
            ))
            .with_title(params.file_path.clone()))
        } else {
            // For new files, show all lines as additions
            let diff = if content_within_diff_size_limit(&params.content) {
                generate_diff_summary("", &params.content)
            } else {
                format!(
                    "(diff skipped: file exceeds the {} KB diff limit; use read to inspect it)",
                    DIFF_MAX_INPUT_BYTES / 1024
                )
            };
            Ok(ToolOutput::new(format!(
                "{warning}Created {} ({} lines):\n{}",
                params.file_path, line_count, diff
            ))
            .with_title(params.file_path.clone()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Recover the body the same way `execute` does, without touching the
    /// filesystem.
    fn recovered_content(input: &Value) -> Result<String> {
        super::super::coerce_text_field(
            input,
            "write",
            "content",
            &[
                "text",
                "data",
                "body",
                "file_content",
                "contents",
                "new_content",
                "newContent",
            ],
            false,
            super::super::PATH_EXAMPLE,
        )
    }

    /// A bodyless `write` must be reported as a missing body, not satisfied
    /// with the destination.
    ///
    /// `coerce_text_arg`'s single-string fallback is right for tools that take
    /// one text argument, but `write` takes two. Given the surviving half of a
    /// truncated payload — exactly one string field, `file_path` — the fallback
    /// handed back `"src/main.rs"` as the *content*, and the tool wrote a file
    /// containing its own path, with a success message.
    #[test]
    fn a_bodyless_write_is_not_given_its_own_path_as_the_body() {
        for input in [
            json!({"file_path": "src/main.rs"}),
            json!({"path": "src/main.rs"}),
            json!({"file_path": "src/main.rs", "intent": "add a helper"}),
        ] {
            assert!(
                recovered_content(&input).is_err(),
                "the path was adopted as the body for {input}"
            );
        }
    }

    /// A body that *is* present must still be found under every accepted
    /// spelling, and never confused with the path sitting next to it.
    #[test]
    fn a_present_body_is_found_under_every_spelling() {
        for key in [
            "content",
            "text",
            "data",
            "body",
            "file_content",
            "contents",
            "new_content",
            "newContent",
        ] {
            let input = json!({"file_path": "src/main.rs", key: "fn main() {}"});
            assert_eq!(
                recovered_content(&input).unwrap(),
                "fn main() {}",
                "body not found under {key}"
            );
        }
        // Two string fields and no recognised body key: still an error, because
        // guessing between them would write one of them.
        assert!(
            recovered_content(&json!({"file_path": "a.rs", "whatever": "x"})).is_err(),
            "an unrecognised field was adopted as the body"
        );
    }

    #[test]
    fn test_generate_diff_summary_single_change() {
        let old = "hello world";
        let new = "hello rust";
        let diff = generate_diff_summary(old, new);

        // Compact format: "1- content" / "1+ content"
        assert!(diff.contains("1- hello world"), "Should show deleted line");
        assert!(diff.contains("1+ hello rust"), "Should show added line");
    }

    #[test]
    fn test_generate_diff_summary_multi_line() {
        let old = "line one\nline two\nline three";
        let new = "line one\nchanged two\nline three";
        let diff = generate_diff_summary(old, new);

        assert!(diff.contains("2- line two"), "Should show deleted line");
        assert!(diff.contains("2+ changed two"), "Should show added line");
        // Equal lines should not appear
        assert!(
            !diff.contains("line one"),
            "Should not show unchanged lines"
        );
    }

    #[test]
    fn test_generate_diff_summary_new_file() {
        let old = "";
        let new = "line one\nline two\nline three";
        let diff = generate_diff_summary(old, new);

        assert!(diff.contains("1+ line one"), "Should show line 1 added");
        assert!(diff.contains("2+ line two"), "Should show line 2 added");
        assert!(diff.contains("3+ line three"), "Should show line 3 added");
    }

    #[test]
    fn test_generate_diff_summary_truncation() {
        // Create old and new with more than 20 changed lines
        let old = (1..=25)
            .map(|i| format!("old line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let new = (1..=25)
            .map(|i| format!("new line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let diff = generate_diff_summary(&old, &new);

        assert!(diff.contains("..."), "Should truncate after 20 lines");
    }

    #[test]
    fn test_generate_diff_summary_line_number_format() {
        let old = "old";
        let new = "new";
        let diff = generate_diff_summary(old, new);

        // Compact format: no padding
        assert!(
            diff.contains("1- old"),
            "Should have line number directly before minus"
        );
        assert!(
            diff.contains("1+ new"),
            "Should have line number directly before plus"
        );
    }

    #[test]
    fn test_generate_diff_summary_empty_result() {
        let old = "same content";
        let new = "same content";
        let diff = generate_diff_summary(old, new);

        assert!(diff.is_empty(), "No changes should produce empty diff");
    }

    // ── Input recovery ─────────────────────────────────────────────────────
    //
    // Every one of these used to fail with a bare "missing field `file_path`"
    // or "missing field `content`", which the model then retried byte-for-byte
    // until the repeat guard blocked the call.

    #[test]
    fn recovers_the_path_from_the_shapes_models_emit() {
        let path_of = |input: &Value| {
            crate::alphacode_app_core::tool::coerce_text_arg(
                input,
                "write",
                "file_path",
                &[
                    "path",
                    "file",
                    "filename",
                    "file_name",
                    "filePath",
                    "filepath",
                    "file-path",
                    "dest",
                    "destination",
                ],
            )
        };
        // Documented key.
        assert_eq!(
            path_of(&json!({"file_path": "a.rs", "content": "x"})).unwrap(),
            "a.rs"
        );
        // Path aliases.
        for key in [
            "path",
            "file",
            "filename",
            "file_name",
            "filePath",
            "filepath",
            "file-path",
            "dest",
            "destination",
        ] {
            assert_eq!(
                path_of(&json!({ key: "a.rs", "content": "x" })).unwrap(),
                "a.rs",
                "path alias {key} rejected"
            );
        }
        // A bare string is a path, not a body: a bare string cannot carry
        // content, and inventing one here would be the worst possible guess.
        assert_eq!(path_of(&json!("a.rs")).unwrap(), "a.rs");
        assert_eq!(path_of(&json!([{"file_path": "a.rs"}])).unwrap(), "a.rs");
        // And a payload with nothing path-shaped is an error that names what
        // arrived.
        assert!(path_of(&json!({"depth": 3})).is_err());
    }

    #[test]
    fn a_missing_content_field_names_the_file() {
        let err = missing_content_error("src/main.rs").to_string();
        assert!(err.contains("missing field `content`"), "{err}");
        // Naming the destination is what lets the model fix the call without
        // re-reading the file list.
        assert!(err.contains("src/main.rs"), "{err}");
    }

    /// Writing an empty file is a legitimate request, so a present-but-empty
    /// `content` must not be reported as missing.
    #[test]
    fn an_explicitly_empty_content_field_is_valid() {
        assert!(!input_has_content_field(&json!({"file_path": "a.rs"})));
        assert!(input_has_content_field(
            &json!({"file_path": "a.rs", "content": ""})
        ));
        assert!(input_has_content_field(
            &json!({"file_path": "a.rs", "text": ""})
        ));
    }

    /// The truncation marker is what lets `write` tell a salvaged call from a
    /// complete one. A well-formed payload must never carry it, or every
    /// subsequent write to an existing file would be refused.
    #[test]
    fn a_complete_payload_is_not_marked_as_truncated() {
        assert!(!input_was_truncated(&json!({
            "file_path": "a.rs", "content": "x"
        })));
        assert!(input_was_truncated(&json!({
            "file_path": "a.rs",
            "content": "prefix",
            crate::alphacode_message_types::TRUNCATED_INPUT_MARKER: true
        })));
        // A marker that is present but false is not a truncation.
        assert!(!input_was_truncated(&json!({
            "file_path": "a.rs",
            crate::alphacode_message_types::TRUNCATED_INPUT_MARKER: false
        })));
    }

    /// The truncated-stream shape end to end: the path survives, the body does
    /// not, the call is refused rather than satisfied with the path, and
    /// nothing is written.
    ///
    /// This is the combination the two fixes exist for. Recovering the path is
    /// what lets the error name the file, and refusing the missing body is what
    /// stops `{"file_path": "a.rs"}` from becoming a file containing `"a.rs"`.
    #[test]
    fn a_truncated_payload_reports_the_file_it_could_not_finish_writing() {
        let recovered = crate::alphacode_message_types::ToolCall::parse_streamed_input_to_object(
            r#"{"file_path": "src/lib.rs", "content": "pub fn truncated"#,
        );
        assert!(input_was_truncated(&recovered), "marker not stamped");
        // The path is recoverable, which is what makes the error actionable.
        let path = super::super::coerce_text_arg(&recovered, "write", "file_path", &["path"])
            .expect("the surviving path is recovered");
        assert_eq!(path, "src/lib.rs");
        // The body is not, and the raw coercion must fail rather than fall back
        // to the only string in the payload.
        assert!(
            recovered_content(&recovered).is_err(),
            "the path was adopted as the body"
        );
        // The error the caller surfaces names the file and says the arguments
        // were cut off, so the model does not resend the identical call.
        let err = truncated_body_error(&path).to_string();
        assert!(
            err.contains("src/lib.rs"),
            "the error does not name the file: {err}"
        );
        assert!(err.contains("cut off"), "the error does not say why: {err}");
        // And the internal marker is never shown to the model as one of its own
        // keys, which would invite it to start supplying it.
        let described = super::super::describe_received_arguments(&recovered);
        assert!(
            !described.contains(crate::alphacode_message_types::TRUNCATED_INPUT_MARKER),
            "the truncation marker leaked into the error: {described}"
        );
    }
}
