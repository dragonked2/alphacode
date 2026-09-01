use crate::alphacode_tool_core::{Tool, ToolContext};
use crate::alphacode_tool_types::ToolOutput;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::{Value, json};

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

        let workdir = ctx
            .working_dir
            .as_ref()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| ".".to_string());

        match action {
            "read_file" => {
                let path = input
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");
                let full_path = if std::path::Path::new(path).is_absolute() {
                    std::path::PathBuf::from(path)
                } else {
                    std::path::PathBuf::from(&workdir).join(path)
                };

                match std::fs::read_to_string(&full_path) {
                    Ok(content) => {
                        let total_len = content.len();
                        let preview = if total_len > 8000 {
                            format!(
                                "{}...\n\n[File truncated: {} total chars, showing first 8000]",
                                &content[..8000],
                                total_len
                            )
                        } else {
                            content
                        };
                        Ok(ToolOutput::new(format!(
                            "📄 {} ({} chars):\n\n{}",
                            full_path.display(),
                            total_len,
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
                let path = input
                    .get("path")
                    .and_then(|v| v.as_str())
                    .unwrap_or(".");
                let full_path = if std::path::Path::new(path).is_absolute() {
                    std::path::PathBuf::from(path)
                } else {
                    std::path::PathBuf::from(&workdir).join(path)
                };

                match std::fs::read_dir(&full_path) {
                    Ok(entries) => {
                        let mut dirs = Vec::new();
                        let mut files = Vec::new();
                        for entry in entries.flatten() {
                            let name = entry.file_name().to_string_lossy().to_string();
                            if name.starts_with('.') {
                                continue;
                            }
                            let metadata = entry.metadata().ok();
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
                        dirs.sort();
                        files.sort();
                        let mut output = format!(
                            "📁 {} ({} dirs, {} files):\n\n",
                            full_path.display(),
                            dirs.len(),
                            files.len()
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
                let pattern = input
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if pattern.is_empty() {
                    return Ok(ToolOutput::new("Error: 'pattern' required.".to_string()));
                }

                use std::process::Command;
                let output = Command::new("grep")
                    .args([
                        "-rn",
                        "--include=*.rs",
                        "--include=*.ts",
                        "--include=*.js",
                        "--include=*.py",
                        "--include=*.go",
                        "--include=*.toml",
                        "--include=*.json",
                        "--include=*.yaml",
                        "--include=*.yml",
                        "--include=*.md",
                        pattern,
                        &workdir,
                    ])
                    .output();

                match output {
                    Ok(out) => {
                        let stdout = String::from_utf8_lossy(&out.stdout);
                        let lines: Vec<&str> = stdout.lines().collect();
                        let total = lines.len();
                        let display = if total > 30 {
                            format!(
                                "{} matches (showing first 30 of {}):\n\n{}",
                                total,
                                total,
                                lines[..30].join("\n")
                            )
                        } else {
                            format!("{} matches:\n\n{}", total, stdout)
                        };
                        Ok(ToolOutput::new(display))
                    }
                    Err(e) => Ok(ToolOutput::new(format!("Search error: {e}"))),
                }
            }
            "analyze" => {
                let query = input
                    .get("query")
                    .and_then(|v| v.as_str())
                    .unwrap_or("general");

                let mut output = String::from("🔍 Project Analysis:\n\n");

                let mut counts: std::collections::HashMap<String, usize> =
                    std::collections::HashMap::new();
                if let Ok(entries) = std::fs::read_dir(&workdir) {
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().to_string();
                        if name.starts_with('.') || name == "target" || name == "node_modules" {
                            continue;
                        }
                        let ext = std::path::Path::new(&name)
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

                output.push_str(&format!("\nQuery: {query}"));
                output.push_str("\n\nNote: This is a read-only analysis. Use other tools to make changes.");

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
