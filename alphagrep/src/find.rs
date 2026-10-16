//! File-name ranking: the `find` mode.
//!
//! Find answers "which file am I looking for" without reading file contents at
//! all. Candidates are scored per ranking term so that a caller can see why a
//! file ranked where it did, and the walk reuses the same scope rules as every
//! other mode.

use crate::cli::FindArgs;
use crate::fsutil::{self, SearchScope};
use globset::{Glob, GlobMatcher};
use serde::Serialize;
use std::path::Path;

/// One ranked file.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FoundFile {
    /// Search-root-relative path.
    pub path: String,
    /// Ranking score; higher is better.
    pub score: f32,
    /// Human-readable scoring reasons.
    pub reasons: Vec<String>,
}

/// Result of a `find` run. Errors are reported through `note` because the
/// caller-facing entry point is infallible by contract.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FindResult {
    /// Root the search ran under.
    pub root: String,
    /// Ranking terms as supplied.
    pub query: Vec<String>,
    /// Ranked files, best first.
    pub files: Vec<FoundFile>,
    /// Files inspected.
    pub files_scanned: usize,
    /// Diagnostic text, such as an invalid glob or a truncation notice.
    pub note: Option<String>,
}

/// One prepared ranking term.
struct Token {
    display: String,
    lowered: String,
    matcher: Option<GlobMatcher>,
}

/// Rank files under `root` by name similarity to `args.query_parts`.
pub fn run_find(root: &Path, args: &FindArgs) -> FindResult {
    let mut result = FindResult {
        root: root.display().to_string(),
        query: args.query_parts.clone(),
        files: Vec::new(),
        files_scanned: 0,
        note: None,
    };

    let scope = match SearchScope::new(root)
        .with_glob(args.glob.as_deref())
        .and_then(|scope| scope.with_file_type(args.file_type.as_deref()))
    {
        Ok(scope) => scope.with_flags(args.hidden, args.no_ignore),
        Err(err) => {
            result.note = Some(err);
            return result;
        }
    };
    if !scope.root.exists() {
        result.note = Some(format!(
            "search root {} does not exist",
            scope.root.display()
        ));
        return result;
    }

    let tokens = prepare_tokens(&args.query_parts);
    let mut ranked: Vec<FoundFile> = Vec::new();
    for file in fsutil::collect_files(&scope) {
        result.files_scanned += 1;
        let (score, reasons) = if tokens.is_empty() {
            (0.0, vec!["no ranking terms".to_string()])
        } else {
            score_path(&file.relative, &tokens)
        };
        if !tokens.is_empty() && score <= 0.0 {
            continue;
        }
        ranked.push(FoundFile {
            path: file.relative,
            score,
            reasons,
        });
    }

    ranked.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.path.cmp(&right.path))
    });

    let matched = ranked.len();
    let limit = args.max_files;
    if limit == 0 {
        ranked.clear();
        result.note = Some(format!("{matched} files matched but max_files is 0"));
    } else if matched > limit {
        ranked.truncate(limit);
        result.note = Some(format!(
            "{matched} files matched; showing the top {limit} (raise max_files for more)"
        ));
    }
    if tokens.is_empty() {
        result.note = Some(match result.note {
            Some(existing) => format!("{existing}; no ranking terms, files are in path order"),
            None => "no ranking terms, files are in path order".to_string(),
        });
    }

    result.files = ranked;
    result
}

fn prepare_tokens(raw_tokens: &[String]) -> Vec<Token> {
    raw_tokens
        .iter()
        .filter_map(|raw| {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                return None;
            }
            let lowered = trimmed.to_ascii_lowercase();
            let matcher = if lowered.contains('*') || lowered.contains('?') {
                Glob::new(&lowered).ok().map(|glob| glob.compile_matcher())
            } else {
                None
            };
            Some(Token {
                display: trimmed.to_string(),
                lowered,
                matcher,
            })
        })
        .collect()
}

/// The path facts a single candidate is scored against; derived once per file
/// and reused for every ranking term.
#[derive(Clone, Copy)]
struct PathFacts<'a> {
    relative: &'a str,
    lowered_path: &'a str,
    components: &'a [&'a str],
    name: &'a str,
    stem: &'a str,
    extension: &'a str,
    words: &'a [String],
}

/// Score one relative path against every ranking term.
fn score_path(relative: &str, tokens: &[Token]) -> (f32, Vec<String>) {
    let lowered_path = relative.to_ascii_lowercase();
    let components: Vec<&str> = lowered_path.split('/').collect();
    let name = components.last().copied().unwrap_or("");
    let (stem, extension) = split_name(name);
    let words = split_words(&stem);
    let depth = relative.matches('/').count();
    let facts = PathFacts {
        relative,
        lowered_path: &lowered_path,
        components: &components,
        name,
        stem: &stem,
        extension,
        words: &words,
    };

    let mut score = 0.0f32;
    let mut reasons = Vec::new();
    for token in tokens {
        let (points, reason) = score_term(&facts, token);
        if points > 0.0 {
            score += points;
            reasons.push(format!("{reason} (+{points:.0})"));
        }
    }

    if score > 0.0 {
        let bonus = (6usize.saturating_sub(depth.min(6))) as f32 * 1.5;
        if bonus > 0.0 {
            score += bonus;
            reasons.push(format!("shallow path (+{bonus:.1})"));
        }
    }
    (score, reasons)
}

/// Score a single term; returns the awarded points plus a reason.
fn score_term(facts: &PathFacts<'_>, token: &Token) -> (f32, String) {
    let PathFacts {
        relative,
        lowered_path,
        components,
        name,
        stem,
        extension,
        words,
    } = *facts;

    if let Some(matcher) = &token.matcher {
        if matcher.is_match(name) {
            return (95.0, format!("glob {:?} matches name", token.display));
        }
        if matcher.is_match(relative) {
            return (45.0, format!("glob {:?} matches path", token.display));
        }
        return (0.0, String::new());
    }

    let short = token.lowered.chars().count() < 3;
    if token.lowered == name {
        return (130.0, "exact file name".to_string());
    }
    if token.lowered == stem {
        return (110.0, "exact file stem".to_string());
    }
    if words.contains(&token.lowered) {
        return (70.0, "file name word".to_string());
    }
    if token.lowered == extension {
        return (35.0, "extension".to_string());
    }
    if components
        .iter()
        .any(|component| *component == token.lowered)
    {
        return (50.0, "path component".to_string());
    }
    if !short && name.contains(&token.lowered) {
        return (55.0, "file name contains".to_string());
    }
    if !short && lowered_path.contains(&token.lowered) {
        return (25.0, "path contains".to_string());
    }
    if !short && is_subsequence(&token.lowered, stem) {
        return (14.0, "name subsequence".to_string());
    }
    if !short && is_subsequence(&token.lowered, &lowered_path.replace('/', " ")) {
        return (7.0, "path subsequence".to_string());
    }
    (0.0, String::new())
}

fn split_name(name: &str) -> (String, &str) {
    match name.rsplit_once('.') {
        Some((stem, extension)) if !stem.is_empty() => (stem.to_string(), extension),
        _ => (name.to_string(), ""),
    }
}

/// Split a stem into lowercase words on separators and camel-case boundaries.
fn split_words(stem: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut previous_lowercase = false;
    for character in stem.chars() {
        if character.is_alphanumeric() {
            let is_upper = character.is_uppercase();
            if is_upper && previous_lowercase && !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            current.push(character.to_ascii_lowercase());
            previous_lowercase = character.is_lowercase() || character.is_numeric();
        } else {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            previous_lowercase = false;
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut haystack_chars = haystack.chars();
    needle
        .chars()
        .all(|wanted| haystack_chars.any(|candidate| candidate == wanted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn args(terms: &[&str]) -> FindArgs {
        FindArgs {
            query_parts: terms.iter().map(|term| term.to_string()).collect(),
            file_type: None,
            json: false,
            paths_only: false,
            debug_score: false,
            max_files: 10,
            hidden: false,
            no_ignore: true,
            path: None,
            glob: None,
        }
    }

    fn fixture() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(dir.path().join("src/alpha")).expect("mkdir");
        fs::write(dir.path().join("src/alpha/parser.rs"), "// parser\n").expect("write");
        fs::write(dir.path().join("src/alpha/parse_tests.rs"), "// tests\n").expect("write");
        fs::write(dir.path().join("src/render.rs"), "// render\n").expect("write");
        fs::write(dir.path().join("README.md"), "# readme\n").expect("write");
        dir
    }

    #[test]
    fn exact_stem_outranks_a_partial_match() {
        let dir = fixture();
        let result = run_find(dir.path(), &args(&["render"]));
        assert_eq!(result.files[0].path, "src/render.rs");
        assert!(result.files[0].score > 100.0);
    }

    #[test]
    fn whole_word_matches_outrank_mid_word_substrings() {
        let dir = fixture();
        // `parse` is a whole word inside `parse_tests.rs` but only a prefix of
        // the single word `parser`, so the whole-word match ranks first.
        let result = run_find(dir.path(), &args(&["parse"]));
        let paths: Vec<&str> = result.files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(
            paths,
            vec!["src/alpha/parse_tests.rs", "src/alpha/parser.rs"]
        );

        // A term that is an exact stem still wins outright.
        let exact = run_find(dir.path(), &args(&["parser"]));
        assert_eq!(exact.files[0].path, "src/alpha/parser.rs");
    }

    #[test]
    fn unrelated_terms_return_nothing() {
        let dir = fixture();
        let result = run_find(dir.path(), &args(&["zebra"]));
        assert!(result.files.is_empty());
    }

    #[test]
    fn glob_terms_match_against_names() {
        let dir = fixture();
        let result = run_find(dir.path(), &args(&["*_tests.rs"]));
        assert_eq!(result.files[0].path, "src/alpha/parse_tests.rs");
    }

    #[test]
    fn empty_terms_list_everything_in_path_order() {
        let dir = fixture();
        let result = run_find(dir.path(), &args(&[]));
        assert_eq!(result.files.len(), 4);
        assert!(result.note.is_some());
    }

    #[test]
    fn an_invalid_glob_is_reported_through_the_note() {
        let dir = fixture();
        let mut glob_args = args(&["render"]);
        glob_args.glob = Some("[".to_string());
        let result = run_find(dir.path(), &glob_args);
        assert!(result.files.is_empty());
        assert!(result.note.expect("note").contains("invalid glob"));
    }
}
