use super::{Tool, ToolContext, ToolOutput};
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::Path;

const MAX_ENTRIES: usize = 100;
const DEFAULT_IGNORE: &[&str] = &[
    "node_modules",
    "__pycache__",
    ".git",
    "dist",
    "build",
    "target",
    ".next",
    ".nuxt",
    "venv",
    ".venv",
    "coverage",
    ".cache",
];

pub struct LsTool;

impl LsTool {
    pub fn new() -> Self {
        Self
    }
}

#[derive(Deserialize)]
struct LsInput {
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    ignore: Option<Vec<String>>,
}

struct DirEntry {
    name: String,
    is_dir: bool,
    depth: usize,
}

#[async_trait]
impl Tool for LsTool {
    fn name(&self) -> &str {
        "ls"
    }

    fn description(&self) -> &str {
        "List directory contents with tree structure. Use before read/edit to discover file paths."
    }

    fn parameters_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "intent": super::intent_schema_property(),
                "path": {
                    "type": "string",
                    "description": "Directory path."
                },
                "ignore": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Ignore patterns."
                }
            }
        })
    }

    fn execution_class(&self, _input: &Value) -> super::ToolExecutionClass {
        super::ToolExecutionClass::ReadOnly
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let params: LsInput = serde_json::from_value(input)?;

        let base_path = params.path.clone().unwrap_or_else(|| ".".to_string());
        let base = ctx.resolve_path_guarded(Path::new(&base_path))?;
        let ignore_extra = params.ignore.clone();

        if !base.exists() {
            return Err(anyhow::anyhow!("Directory not found: {}", base_path));
        }

        if !base.is_dir() {
            return Err(anyhow::anyhow!("Not a directory: {}", base_path));
        }

        let entries = tokio::task::spawn_blocking(move || {
            let mut ignore_patterns: Vec<String> =
                DEFAULT_IGNORE.iter().map(|s| s.to_string()).collect();
            if let Some(extra) = ignore_extra {
                ignore_patterns.extend(extra);
            }

            let mut entries: Vec<DirEntry> = Vec::new();
            // Keep one lookahead row so the truncation notice distinguishes an
            // exact 100-entry directory from a longer listing.
            collect_entries(&base, 0, &ignore_patterns, &mut entries, MAX_ENTRIES + 1)?;
            Ok::<_, anyhow::Error>(entries)
        })
        .await??;

        let truncated = entries.len() > MAX_ENTRIES;
        let entries: Vec<_> = entries.into_iter().take(MAX_ENTRIES).collect();

        let mut output = String::new();
        output.push_str(&format!("{}/\n", base_path));

        for entry in &entries {
            let indent = "  ".repeat(entry.depth);
            let suffix = if entry.is_dir { "/" } else { "" };
            output.push_str(&format!("{}{}{}\n", indent, entry.name, suffix));
        }

        if truncated {
            output.push_str(&format!("\n... truncated at {} entries", MAX_ENTRIES));
        }

        let file_count = entries.iter().filter(|e| !e.is_dir).count();
        let dir_count = entries.iter().filter(|e| e.is_dir).count();
        output.push_str(&format!(
            "\n{} files, {} directories",
            file_count, dir_count
        ));

        Ok(ToolOutput::new(output))
    }
}

/// Fast string-based ignore matching that avoids compiling a glob regex
/// per file per directory.  Handles the common patterns in `DEFAULT_IGNORE`:
/// - Simple name match (`node_modules`) — handled by caller equality check.
/// - Glob with `**` prefix suffix (`**/target`) — substring match.
/// - Glob with `*` suffix (`*.log`) — suffix match after last `.`.
/// - Glob with `*` in middle (`*.pyc`) — simple prefix/suffix split.
///   Falls back to exact match for anything else.
fn name_matches_ignore(name: &str, pattern: &str) -> bool {
    if let Some(suffix) = pattern.strip_prefix("**/") {
        // **/target  →  match "target" anywhere
        return name == suffix || name.contains(suffix);
    }
    if let Some(ext) = pattern.strip_prefix("*.") {
        // *.pyc  →  name ends with ".pyc"
        return name.ends_with(ext)
            && name.len() > ext.len() + 1
            && name.as_bytes()[name.len() - ext.len() - 1] == b'.';
    }
    if let Some(stripped) = pattern.strip_suffix("/*") {
        // dir/* → match dir prefix
        return name.starts_with(stripped);
    }
    false
}

fn collect_entries(
    dir: &Path,
    depth: usize,
    ignore: &[String],
    entries: &mut Vec<DirEntry>,
    max: usize,
) -> Result<()> {
    if entries.len() >= max {
        return Ok(());
    }

    // Retain only the lexically first entries that can fit in the result.
    // A Vec of the whole directory made `ls` use unbounded memory before its
    // 100-entry output cap could take effect.
    let capacity = max.saturating_sub(entries.len()).saturating_add(1);
    let mut items: BTreeMap<(bool, std::ffi::OsString), (std::fs::DirEntry, bool)> =
        BTreeMap::new();
    for item in std::fs::read_dir(dir)? {
        let Ok(item) = item else { continue };
        let name = item.file_name();
        let name_lossy = name.to_string_lossy();

        if ignore.iter().any(|pattern| {
            name_lossy == pattern.as_str() || name_matches_ignore(&name_lossy, pattern)
        }) || (name_lossy.starts_with('.') && name_lossy != "." && name_lossy != "..")
        {
            continue;
        }

        let is_dir = item.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        let key = (!is_dir, name);
        if items.len() < capacity {
            items.insert(key, (item, is_dir));
        } else if items
            .last_key_value()
            .is_some_and(|(largest, _)| key.cmp(largest).is_lt())
        {
            items.pop_last();
            items.insert(key, (item, is_dir));
        }
    }

    for ((_, name), (item, is_dir)) in items {
        if entries.len() >= max {
            break;
        }

        let name = name.to_string_lossy().to_string();

        entries.push(DirEntry {
            name: name.clone(),
            is_dir,
            depth: depth + 1,
        });

        if is_dir && depth < 5 {
            let path = item.path();
            collect_entries(&path, depth + 1, ignore, entries, max)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_lookahead_distinguishes_exact_limit_from_truncation() {
        let temp = tempfile::tempdir().expect("tempdir");
        for i in 0..=MAX_ENTRIES {
            std::fs::write(temp.path().join(format!("file-{i:03}.txt")), "x")
                .expect("create fixture");
        }

        let mut entries = Vec::new();
        collect_entries(temp.path(), 0, &[], &mut entries, MAX_ENTRIES + 1)
            .expect("collect entries");
        assert!(entries.len() > MAX_ENTRIES);
        assert_eq!(
            entries
                .into_iter()
                .take(MAX_ENTRIES)
                .collect::<Vec<_>>()
                .len(),
            MAX_ENTRIES
        );

        let exact = tempfile::tempdir().expect("exact-limit tempdir");
        for i in 0..MAX_ENTRIES {
            std::fs::write(exact.path().join(format!("file-{i:03}.txt")), "x")
                .expect("create fixture");
        }
        let mut entries = Vec::new();
        collect_entries(exact.path(), 0, &[], &mut entries, MAX_ENTRIES + 1)
            .expect("collect entries");
        assert_eq!(entries.len(), MAX_ENTRIES);
    }
}
