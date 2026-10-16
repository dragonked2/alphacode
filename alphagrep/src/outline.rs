//! Single-file structure summary: the `outline` mode.

use crate::cli::OutlineArgs;
use crate::fsutil;
use crate::harness::HarnessContext;
use crate::structure::{self, Language, StructureItem};
use serde::Serialize;
use std::path::{Path, PathBuf};

/// Items printed before eliding the tail.
pub const DEFAULT_MAX_ITEMS: usize = 200;

/// Result of an `outline` run.
#[derive(Debug, Clone, Serialize)]
pub struct OutlineResult {
    /// Path as shown to the caller (relative to the search root when possible).
    pub path: String,
    /// Detected language family.
    pub language: String,
    /// Items to print, possibly truncated by `max_items`.
    pub items: Vec<StructureItem>,
    /// Items found before truncation.
    pub total_items: usize,
    /// Lines in the file.
    pub lines: usize,
    /// Diagnostic text.
    pub note: Option<String>,
}

/// Outline a single file under `root`.
pub fn run_outline(root: &Path, args: &OutlineArgs) -> Result<OutlineResult, String> {
    let target = resolve_target(root, &args.file);
    if !target.exists() {
        return Err(format!(
            "no such file: {} (outline needs one existing file; use grep or find for directories)",
            target.display()
        ));
    }
    if target.is_dir() {
        return Err(format!(
            "{} is a directory; outline works on a single file (use grep or find to search directories)",
            target.display()
        ));
    }
    let Some(text) = fsutil::read_text(&target) else {
        return Err(format!(
            "{} could not be read as text (binary file or larger than {} bytes)",
            target.display(),
            fsutil::MAX_SCAN_BYTES
        ));
    };

    let language = Language::from_path(&target);
    let mut items = structure::extract_structure(&text, &target);
    let total_items = items.len();
    let display_path = fsutil::relative_string(root, &target);

    let mut note = None;
    let limit = args
        .max_items
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_MAX_ITEMS);
    if items.len() > limit {
        items.truncate(limit);
        note = Some(format!(
            "{total_items} items found; showing the first {limit} (set max_items to change)"
        ));
    }

    if let Some(context) = HarnessContext::load(args.context_json.as_deref()) {
        let known = context.known_regions_in(&display_path).len();
        if known > 0 {
            let suffix = format!("{known} region(s) of this file were already read this session");
            note = Some(match note {
                Some(existing) => format!("{existing}; {suffix}"),
                None => suffix,
            });
        }
    }

    Ok(OutlineResult {
        path: display_path,
        language: language.label().to_string(),
        items,
        total_items,
        lines: text.lines().count(),
        note,
    })
}

/// Resolve `file` against the search root.
///
/// `root` already accounts for the `--path` argument (the caller resolves it),
/// and absolute `file` values pass through untouched, which is what the agent
/// tool relies on when it hands over an already-resolved path.
fn resolve_target(root: &Path, file: &str) -> PathBuf {
    let trimmed = file.trim();
    let candidate = PathBuf::from(trimmed.trim_matches(['"', '\'']));
    if candidate.is_absolute() {
        candidate
    } else {
        root.join(candidate)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn args(file: &str) -> OutlineArgs {
        OutlineArgs {
            file: file.to_string(),
            json: false,
            max_items: None,
            path: None,
            context_json: None,
        }
    }

    #[test]
    fn outlines_a_rust_file_relative_to_the_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir(dir.path().join("src")).expect("mkdir");
        fs::write(
            dir.path().join("src/lib.rs"),
            "pub fn alpha() {}\npub struct Beta;\n",
        )
        .expect("write");

        let result = run_outline(dir.path(), &args("src/lib.rs")).expect("outline");
        assert_eq!(result.path, "src/lib.rs");
        assert_eq!(result.language, "rust");
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.items[0].kind, "fn");
        assert_eq!(result.items[1].kind, "struct");
    }

    #[test]
    fn absolute_paths_are_used_as_given() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("thing.py");
        fs::write(&file, "def run():\n    return 1\n").expect("write");

        let result = run_outline(dir.path(), &args(&file.display().to_string())).expect("outline");
        assert_eq!(result.items[0].label, "run");
    }

    #[test]
    fn directories_are_rejected_with_a_pointer_to_other_modes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let error = run_outline(dir.path(), &args(".")).expect_err("should fail");
        assert!(error.contains("grep or find"));
    }

    #[test]
    fn max_items_truncates_and_notes_the_rest() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut source = String::new();
        for index in 0..5 {
            source.push_str(&format!("fn function_{index}() {{}}\n"));
        }
        fs::write(dir.path().join("many.rs"), source).expect("write");

        let mut outline_args = args("many.rs");
        outline_args.max_items = Some(2);
        let result = run_outline(dir.path(), &outline_args).expect("outline");
        assert_eq!(result.items.len(), 2);
        assert_eq!(result.total_items, 5);
        assert!(result.note.expect("note").contains("showing the first 2"));
    }
}
