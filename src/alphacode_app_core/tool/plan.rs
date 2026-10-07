use crate::alphacode_tool_core::{Tool, ToolContext};
use crate::alphacode_tool_types::ToolOutput;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

const MAX_PLAN_FILE_PREVIEW_BYTES: usize = 8_000;
const MAX_PLAN_DIRECTORY_ENTRIES: usize = 500;

/// Plan Mode tool - read-only exploration of the codebase.
///
/// This tool is specifically for planning and exploration. It can:
/// - Read files and directories without modification
/// - Search code patterns
/// - Analyze dependencies
/// - Review architecture
///
/// The key difference from other tools: this tool explicitly forbids any
/// writes, edits, or modifications. It's for thinking before acting.
pub struct PlanModeTool;

#[async_trait]
impl Tool for PlanModeTool {
    fn name(&self) -> &str {
        "plan"
    }

    fn description(&self) -> &str {
        "Plan Mode: read-only exploration of the codebase. Use this to understand the project structure, analyze code, review architecture, and plan changes BEFORE making them. This tool cannot write, edit, or modify any files. It is for thinking, not acting."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "required": ["action"],
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["read_file", "list_dir", "search", "analyze", "summary"],
                    "description": "Read-only action to perform."
                },
                "path": {
                    "type": "string",
                    "description": "File or directory path to read/analyze."
                },
                "pattern": {
                    "type": "string",
                    "description": "Search pattern (for 'search' action)."
                },
                "query": {
                    "type": "string",
                    "description": "Analysis query (for 'analyze' action)."
                }
            }
        })
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let action = input
            .get("action")
            .and_then(|v| v.as_str())
            .unwrap_or("summary");

        let workdir_path = ctx.resolve_path_guarded(Path::new("."))?;
        let workdir = workdir_path.to_string_lossy().into_owned();

        match action {
            "read_file" => {
                let path = input.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                let full_path = ctx.resolve_path_guarded(Path::new(path))?;

                let read_preview = async {
                    use tokio::io::AsyncReadExt as _;

                    let metadata = tokio::fs::metadata(&full_path).await?;
                    let file = tokio::fs::File::open(&full_path).await?;
                    let mut bytes = Vec::with_capacity(MAX_PLAN_FILE_PREVIEW_BYTES + 1);
                    let mut limited = file.take((MAX_PLAN_FILE_PREVIEW_BYTES + 1) as u64);
                    limited.read_to_end(&mut bytes).await?;
                    Ok::<_, std::io::Error>((metadata.len(), bytes))
                }
                .await;

                match read_preview {
                    Ok((total_bytes, mut bytes)) => {
                        let truncated = total_bytes > MAX_PLAN_FILE_PREVIEW_BYTES as u64;
                        bytes.truncate(MAX_PLAN_FILE_PREVIEW_BYTES);
                        // If a valid UTF-8 codepoint was split by the byte
                        // limit, back up to its start rather than rendering a
                        // replacement character at the end of the preview.
                        if truncated {
                            while std::str::from_utf8(&bytes).is_err() {
                                bytes.pop();
                            }
                        }
                        let preview = String::from_utf8_lossy(&bytes);
                        let note = if truncated {
                            format!(
                                "\n\n[File truncated: {total_bytes} total bytes, showing the first {} bytes]",
                                bytes.len()
                            )
                        } else {
                            String::new()
                        };
                        Ok(ToolOutput::new(format!(
                            "📄 {} ({total_bytes} bytes):\n\n{}{note}",
                            full_path.display(),
                            preview
                        )))
                    }
                    Err(e) => Ok(ToolOutput::new(format!(
                        "Error reading {}: {e}",
                        full_path.display()
                    ))),
                }
            }
            "list_dir" => {
                let path = input.get("path").and_then(|v| v.as_str()).unwrap_or(".");
                let full_path = ctx.resolve_path_guarded(Path::new(path))?;

                let list_directory = async {
                    let mut entries = tokio::fs::read_dir(&full_path).await?;
                    let mut dirs = Vec::new();
                    let mut files = Vec::new();
                    let mut truncated = false;
                    let mut visible_entries = 0usize;
                    let mut scanned_entries = 0usize;
                    while let Some(entry) = entries.next_entry().await? {
                        scanned_entries += 1;
                        if scanned_entries > MAX_PLAN_DIRECTORY_ENTRIES * 4 {
                            truncated = true;
                            break;
                        }
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.starts_with('.') {
                            continue;
                        }
                        if visible_entries == MAX_PLAN_DIRECTORY_ENTRIES {
                            truncated = true;
                            break;
                        }
                        visible_entries += 1;
                        let metadata = entry.metadata().await.ok();
                        let is_dir = metadata.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                        let size = metadata.as_ref().map(|m| m.len()).unwrap_or(0);
                        if is_dir {
                            dirs.push(format!("  📁 {name}/"));
                        } else {
                            let size_str = if size > 1024 * 1024 {
                                format!(" ({:.1}MB)", size as f64 / 1024.0 / 1024.0)
                            } else if size > 1024 {
                                format!(" ({:.1}KB)", size as f64 / 1024.0)
                            } else {
                                format!(" ({size}B)")
                            };
                            files.push(format!("  📄 {name}{size_str}"));
                        }
                    }
                    Ok::<_, std::io::Error>((dirs, files, truncated))
                }
                .await;

                match list_directory {
                    Ok((mut dirs, mut files, truncated)) => {
                        dirs.sort();
                        files.sort();
                        let mut output = format!(
                            "📁 {} ({} dirs, {} files{}):\n\n",
                            full_path.display(),
                            dirs.len(),
                            files.len(),
                            if truncated {
                                ", first 500 entries shown"
                            } else {
                                ""
                            }
                        );
                        for d in &dirs {
                            output.push_str(d);
                            output.push('\n');
                        }
                        for f in &files {
                            output.push_str(f);
                            output.push('\n');
                        }
                        Ok(ToolOutput::new(output))
                    }
                    Err(e) => Ok(ToolOutput::new(format!(
                        "Error listing {}: {e}",
                        full_path.display()
                    ))),
                }
            }
            "search" => {
                let pattern = input.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
                if pattern.is_empty() {
                    return Ok(ToolOutput::new("Error: 'pattern' required.".to_string()));
                }

                let mut command = tokio::process::Command::new("rg");
                command.args([
                    "--line-number",
                    "--no-heading",
                    "--color=never",
                    "--max-columns=400",
                    "--max-columns-preview",
                    // Restrict per-file output as a second guard in addition
                    // to the shared process capture cap.
                    "--max-count=31",
                    "--glob=*.rs",
                    "--glob=*.ts",
                    "--glob=*.js",
                    "--glob=*.py",
                    "--glob=*.go",
                    "--glob=*.toml",
                    "--glob=*.json",
                    "--glob=*.yaml",
                    "--glob=*.yml",
                    "--glob=*.md",
                    "--",
                    pattern,
                    &workdir,
                ]);
                let output = super::recon_common::run_command_bounded(
                    command,
                    "ripgrep",
                    Duration::from_secs(30),
                )
                .await;

                match output {
                    Ok(out) if out.status.success() || out.status.code() == Some(1) => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let lines: Vec<&str> = stdout.lines().collect();
                        let shown = lines.len().min(30);
                        let display = if shown == 0 {
                            "0 matches.".to_string()
                        } else {
                            let count = if lines.len() > 30 {
                                format!("{} matches (showing first 30)", lines.len())
                            } else {
                                format!("{} matches", lines.len())
                            };
                            format!("{count}:\n\n{}", lines[..shown].join("\n"))
                        };
                        Ok(ToolOutput::new(display))
                    }
                    Ok(out) => {
                        let detail = String::from_utf8_lossy(&out.stderr);
                        let detail = if detail.trim().is_empty() {
                            String::from_utf8_lossy(&out.stdout).into_owned()
                        } else {
                            detail.into_owned()
                        };
                        Ok(ToolOutput::new(format!(
                            "Search failed ({}): {}",
                            out.status,
                            crate::alphacode_core::util::truncate_str(detail.trim(), 800)
                        )))
                    }
                    Err(error) => Ok(ToolOutput::new(format!("Search error: {error}"))),
                }
            }
            "analyze" => {
                let query = input
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("general");

                let mut output = String::from("🔍 Project Analysis:\n\n");

                const MAX_ANALYZED_ENTRIES: usize = 2_000;
                let mut counts: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                let mut entries_scanned = 0usize;
                let mut analysis_truncated = false;
                if let Ok(mut entries) = tokio::fs::read_dir(&workdir_path).await {
                    loop {
                        let entry = match entries.next_entry().await {
                            Ok(Some(entry)) => entry,
                            Ok(None) | Err(_) => break,
                        };
                        entries_scanned += 1;
                        if entries_scanned > MAX_ANALYZED_ENTRIES {
                            analysis_truncated = true;
                            break;
                        }
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.starts_with('.') || name == "target" || name == "node_modules" {
                            continue;
                        }
                        let ext = Path::new(&name)
                            .extension()
                            .and_then(|e| e.to_str())
                            .unwrap_or("other")
                            .to_string();
                        *counts.entry(ext).or_insert(0) += 1;
                    }
                }

                output.push_str("File types:\n");
                let mut sorted: Vec<_> = counts.into_iter().collect();
                sorted.sort_by(|a, b| b.1.cmp(&a.1));
                for (ext, count) in sorted.iter().take(15) {
                    output.push_str(&format!("  .{ext}: {count} files\n"));
                }
                if analysis_truncated {
                    output.push_str("\nAnalysis was capped after 2,000 directory entries.\n");
                }

                output.push_str(&format!("\nQuery: {query}"));
                output.push_str(
                    "\n\nNote: This is a read-only analysis. Use other tools to make changes.",
                );

                Ok(ToolOutput::new(output))
            }
            "summary" => {
                let mut output = String::from("📋 Plan Mode Summary:\n\n");
                output.push_str("This is a READ-ONLY exploration tool.\n\n");
                output.push_str("Available actions:\n");
                output.push_str("  • read_file - Read a file's contents\n");
                output.push_str("  • list_dir - List directory contents\n");
                output.push_str("  • search - Search for patterns in code\n");
                output.push_str("  • analyze - Analyze project structure\n");
                output.push_str("  • summary - Show this help\n\n");
                output.push_str("Use this tool to explore and plan before making changes.\n");
                output.push_str("No files will be modified by this tool.");

                Ok(ToolOutput::new(output))
            }
            _ => Ok(ToolOutput::new(format!(
                "Unknown action '{action}'. Use: read_file, list_dir, search, analyze, summary."
            ))),
        }
    }
}
