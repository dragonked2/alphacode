//! Content search: the `grep` mode.
//!
//! The engine is intentionally self-contained: it walks the scoped file set with
//! [`crate::fsutil`] and matches each line with the [`regex`] crate instead of
//! shelling out to an external binary. That keeps results deterministic, works
//! on machines without ripgrep installed, and lets the caller receive plain
//! `String` errors that the agent tool can surface verbatim.
//!
//! The tree walk stays serial so the file order is deterministic, but reading
//! and scanning each file is independent work and runs on rayon's thread pool.
//! The results are re-assembled in walk order, so the output is identical to
//! the serial implementation's.

use crate::cli::GrepArgs;
use crate::fsutil::{self, SearchScope};
use rayon::prelude::*;
use regex::Regex;
use serde::Serialize;
use std::path::Path;

/// Match lines kept per file before elision kicks in.
pub const MAX_MATCH_LINES_PER_FILE: usize = 40;
/// Files with matches kept before the result set is called truncated.
pub const MAX_FILES_WITH_MATCHES: usize = 200;
/// Longest match line stored verbatim.
pub const MAX_MATCH_LINE_LENGTH: usize = 240;

/// One matching line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrepMatch {
    /// 1-based line number.
    pub line: usize,
    /// 1-based column of the first match on the line.
    pub column: usize,
    /// Trimmed line text.
    pub text: String,
}

/// All matches in one file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GrepFile {
    /// Search-root-relative path.
    pub path: String,
    /// Kept matches, capped at [`MAX_MATCH_LINES_PER_FILE`].
    pub matches: Vec<GrepMatch>,
    /// Matches dropped from `matches` because of the per-file cap.
    pub omitted: usize,
}

/// Result of a `grep` run.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GrepResult {
    /// Root the search ran under.
    pub root: String,
    /// Pattern as the caller supplied it.
    pub query: String,
    /// Whether the pattern was matched literally.
    pub literal: bool,
    /// Files with matches.
    pub files: Vec<GrepFile>,
    /// Number of files with matches.
    pub total_files: usize,
    /// Number of matching lines across all files.
    pub total_matches: usize,
    /// Matching lines not present in `files` because of caps.
    pub omitted_matches: usize,
    /// Whether the file cap stopped the walk early.
    pub truncated: bool,
    /// Files inspected.
    pub files_scanned: usize,
}

/// One file's scan outcome, produced independently by the parallel pass.
struct FileScan {
    /// Search-root-relative path.
    path: String,
    /// Kept matches, capped at [`MAX_MATCH_LINES_PER_FILE`].
    matches: Vec<GrepMatch>,
    /// Matches dropped from `matches` because of the per-file cap.
    omitted: usize,
    /// Matching lines in the file, including the ones dropped by the cap.
    total: usize,
}

/// Read one file and collect its matches, or `None` when the file is
/// unreadable, oversized, binary, or holds no match.
fn scan_file(matcher: &Regex, file: &fsutil::WalkedFile) -> Option<FileScan> {
    let text = fsutil::read_text(&file.absolute)?;
    let mut matches = Vec::new();
    let mut omitted = 0usize;
    let mut total = 0usize;
    for (index, line) in text.lines().enumerate() {
        let Some(found) = matcher.find(line) else {
            continue;
        };
        total += 1;
        if matches.len() < MAX_MATCH_LINES_PER_FILE {
            matches.push(GrepMatch {
                line: index + 1,
                column: found.start() + 1,
                text: clip_line(line),
            });
        } else {
            omitted += 1;
        }
    }
    if total == 0 {
        return None;
    }
    Some(FileScan {
        path: file.relative.clone(),
        matches,
        omitted,
        total,
    })
}

/// Search `root` for the pattern in `args`.
pub fn run_grep(root: &Path, args: &GrepArgs) -> Result<GrepResult, String> {
    let query = args.query.clone();
    if query.trim().is_empty() {
        return Err("grep requires a non-empty pattern".to_string());
    }
    let pattern = if args.regex {
        query.clone()
    } else {
        regex::escape(&query)
    };
    let matcher =
        Regex::new(&pattern).map_err(|err| format!("invalid regex pattern {query:?}: {err}"))?;

    let scope = SearchScope::new(root)
        .with_glob(args.glob.as_deref())?
        .with_file_type(args.file_type.as_deref())?
        .with_flags(args.hidden, args.no_ignore);
    if !scope.root.exists() {
        return Err(format!(
            "search root {} does not exist",
            scope.root.display()
        ));
    }

    let mut result = GrepResult {
        root: scope.root.display().to_string(),
        query,
        literal: !args.regex,
        files: Vec::new(),
        total_files: 0,
        total_matches: 0,
        omitted_matches: 0,
        truncated: false,
        files_scanned: 0,
    };

    // The walk stays serial so the file order is deterministic.
    let files = fsutil::collect_files(&scope);
    result.files_scanned = files.len();

    // Reading and scanning each file is independent work, so it runs on rayon's
    // thread pool. `collect` keeps the walk order, and the re-assembly below
    // re-applies the caps in that same order, so the result is identical to the
    // serial implementation's.
    let scans: Vec<Option<FileScan>> = files
        .par_iter()
        .map(|file| scan_file(&matcher, file))
        .collect();

    for scan in scans.into_iter().flatten() {
        if result.files.len() >= MAX_FILES_WITH_MATCHES {
            result.truncated = true;
            result.omitted_matches += scan.total;
            continue;
        }
        result.total_matches += scan.total;
        result.omitted_matches += scan.omitted;
        result.files.push(GrepFile {
            path: scan.path,
            matches: scan.matches,
            omitted: scan.omitted,
        });
    }
    result.total_files = result.files.len();
    Ok(result)
}

/// Trim a line's trailing whitespace and clip it to a readable width.
pub fn clip_line(line: &str) -> String {
    clip_to(line.trim_end(), MAX_MATCH_LINE_LENGTH)
}

/// Clip text to `limit` characters, appending an ellipsis when cut.
pub fn clip_to(text: &str, limit: usize) -> String {
    if limit == 0 {
        return String::new();
    }
    if text.chars().count() <= limit {
        return text.to_string();
    }
    let mut clipped: String = text.chars().take(limit - 1).collect();
    clipped.push('…');
    clipped
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn args(query: &str) -> GrepArgs {
        GrepArgs {
            query: query.to_string(),
            regex: false,
            file_type: None,
            json: false,
            paths_only: false,
            hidden: false,
            no_ignore: true,
            path: None,
            glob: None,
        }
    }

    #[test]
    fn literal_search_finds_matches_with_line_numbers() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("a.rs"), "let alpha = 1;\nlet beta = 2;\n").expect("write");
        fs::write(dir.path().join("b.txt"), "alpha\n").expect("write");

        let result = run_grep(dir.path(), &args("alpha")).expect("grep");
        assert_eq!(result.total_matches, 2);
        assert_eq!(result.files.len(), 2);
        let a = result
            .files
            .iter()
            .find(|file| file.path == "a.rs")
            .expect("a");
        assert_eq!(a.matches[0].line, 1);
        assert_eq!(a.matches[0].column, 5);
    }

    #[test]
    fn literal_search_does_not_treat_patterns_as_regex() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::write(dir.path().join("a.rs"), "a.b\naxb\n").expect("write");

        let result = run_grep(dir.path(), &args("a.b")).expect("grep");
        assert_eq!(result.total_matches, 1);
    }

    #[test]
    fn invalid_regex_reports_the_pattern() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut regex_args = args("(");
        regex_args.regex = true;
        let error = run_grep(dir.path(), &regex_args).expect_err("should fail");
        assert!(error.contains("invalid regex pattern"));
    }

    #[test]
    fn type_and_glob_filters_narrow_the_scan() {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir(dir.path().join("src")).expect("mkdir");
        fs::write(dir.path().join("src/a.rs"), "needle\n").expect("write");
        fs::write(dir.path().join("src/a.py"), "needle\n").expect("write");

        let mut rust_args = args("needle");
        rust_args.file_type = Some("rs".to_string());
        let result = run_grep(dir.path(), &rust_args).expect("grep");
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].path, "src/a.rs");
    }

    #[test]
    fn missing_root_is_an_error() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("nope");
        let error = run_grep(&missing, &args("x")).expect_err("should fail");
        assert!(error.contains("does not exist"));
    }
}
