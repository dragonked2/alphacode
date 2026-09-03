use super::diff_utils::generate_diff_summary;
use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::Path;

pub struct MultiEditTool;

impl MultiEditTool {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Deserialize)]
struct MultiEditInput {
    file_path: String,
    edits: Vec<EditOperation>,
}

#[derive(Deserialize)]
struct EditOperation {
    old_string: String,
    new_string: String,
    #[serde(default)]
    replace_all: bool,
}

#[async_trait]
impl Tool for MultiEditTool {
    fn name(&self) -> &str {
        "multiedit"
    }

    fn description(&self) -> &str {
        "Apply multiple edits to one file in one call. Edits are applied top-to-bottom; each old_string must be unique. Read the file first. If any edit fails, the whole batch fails — use small batches."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["file_path", "edits"],
            "properties": {
                "intent": super::intent_schema_property(),
                "file_path": {
                    "type": "string",
                    "description": "The path to the file to edit"
                },
                "edits": {
                    "type": "array",
                    "description": "Array of edit operations to apply sequentially",
                    "items": {
                        "type": "object",
                        "required": ["old_string", "new_string"],
                        "properties": {
                            "old_string": {
                                "type": "string",
                                "description": "The text to find and replace"
                            },
                            "new_string": {
                                "type": "string",
                                "description": "The replacement text"
                            },
                            "replace_all": {
                                "type": "boolean",
                                "description": "Replace all occurrences (default: false)"
                            }
                        }
                    },
                    "minItems": 1
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let params: MultiEditInput = serde_json::from_value(input)?;

        let path = ctx.resolve_path(Path::new(&params.file_path));

        if !path.exists() {
            return Err(anyhow::anyhow!("File not found: {}", params.file_path));
        }

        let original_content = tokio::fs::read_to_string(&path).await?;
        let mut content = original_content.clone();
        let mut applied = Vec::new();
        let mut failed = Vec::new();

        for (i, edit) in params.edits.iter().enumerate() {
            if edit.old_string == edit.new_string {
                failed.push(format!("Edit {}: old_string equals new_string", i + 1));
                continue;
            }

            let occurrences = content.matches(&edit.old_string).count();

            if occurrences == 0 {
                failed.push(format!("Edit {}: old_string not found", i + 1));
                continue;
            }

            if occurrences > 1 && !edit.replace_all {
                failed.push(format!(
                    "Edit {}: found {} occurrences, use replace_all or be more specific",
                    i + 1,
                    occurrences
                ));
                continue;
            }

            // Apply the edit
            if edit.replace_all {
                content = content.replace(&edit.old_string, &edit.new_string);
                applied.push(format!(
                    "Edit {}: replaced {} occurrences",
                    i + 1,
                    occurrences
                ));
            } else {
                content = content.replacen(&edit.old_string, &edit.new_string, 1);
                applied.push(format!("Edit {}: replaced 1 occurrence", i + 1));
            }
        }

        // Write the result
        tokio::fs::write(&path, &content).await?;

        // Format output
        let mut output = format!("Edited {}\n\n", params.file_path);

        if !applied.is_empty() {
            output.push_str("Applied:\n");
            for msg in &applied {
                output.push_str(&format!("  ✓ {}\n", msg));
            }
        }

        if !failed.is_empty() {
            output.push_str("\nFailed:\n");
            for msg in &failed {
                output.push_str(&format!("  ✗ {}\n", msg));
            }
        }

        output.push_str(&format!(
            "\nTotal: {} applied, {} failed\n",
            applied.len(),
            failed.len()
        ));

        // Generate diff summary
        if !applied.is_empty() {
            output.push_str("\nDiff:\n");
            output.push_str(&generate_diff_summary(&original_content, &content));
        }

        Ok(ToolOutput::new(output).with_title(params.file_path.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn test_generate_diff_summary_multiple_edits() {
        let old = "line 1\nline 2\nline 3\nline 4\nline 5";
        let new = "line 1\nmodified 2\nline 3\nmodified 4\nline 5";
        let diff = generate_diff_summary(old, new);

        // Should show both changed lines with correct line numbers
        assert!(diff.contains("2- line 2"), "Should show line 2 deleted");
        assert!(diff.contains("2+ modified 2"), "Should show line 2 added");
        assert!(diff.contains("4- line 4"), "Should show line 4 deleted");
        assert!(diff.contains("4+ modified 4"), "Should show line 4 added");
    }

    #[test]
    fn test_generate_diff_summary_truncation() {
        // Create old and new with more than 30 changed lines
        let old = (1..=35)
            .map(|i| format!("old line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let new = (1..=35)
            .map(|i| format!("new line {}", i))
            .collect::<Vec<_>>()
            .join("\n");
        let diff = generate_diff_summary(&old, &new);

        assert!(diff.contains("..."), "Should truncate after 30 lines");
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
}
