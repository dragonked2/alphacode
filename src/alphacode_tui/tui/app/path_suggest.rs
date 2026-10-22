//! `@`-mention path completion for the composer.
//!
//! The composer already completes slash commands. File references are the
//! other half of a modern coding-agent prompt: writing `@src/main.rs` should
//! be as cheap as invoking `/model` is, and the reference should reach the
//! model as a path it can actually open.
//!
//! The scan is deliberately bounded and cached: it runs on the keystroke
//! path, so an unbounded recursive walk of a large repository would be felt
//! as input latency.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// How long a directory scan stays fresh.
///
/// Long enough that a burst of keystrokes reuses one scan, short enough that
/// a newly created file shows up without the user having to restart.
const SCAN_TTL: Duration = Duration::from_millis(1500);

/// Maximum number of filesystem entries a single scan visits.
///
/// The scan runs on the keystroke path, so it has to stay short enough that
/// the first `@` keystroke does not visibly stall. A typical project root is
/// a few hundred entries, so this covers the useful depth while keeping the
/// cold cost in the tens of milliseconds.
const MAX_VISITED: usize = 1200;

/// Maximum directory depth below the scan root.
const MAX_DEPTH: usize = 3;

/// Maximum number of candidates handed to the palette.
const MAX_CANDIDATES: usize = 200;

/// Directories that are never worth walking.
const SKIPPED_DIRS: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "target",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".venv",
    "venv",
    "__pycache__",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
    ".gradle",
    ".idea",
    ".vscode",
    ".cache",
    "vendor",
    "coverage",
    ".mypy",
];

/// What a candidate points at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PathKind {
    File,
    Directory,
}

/// A file or directory the user can mention.
#[derive(Clone, Debug)]
pub struct PathCandidate {
    /// Path relative to the scan root, using `/` separators. Directories
    /// carry a trailing `/` so the palette reads like a tree.
    pub path: String,
    /// Lowercased copy of `path`.
    ///
    /// The ranking runs on every frame while the palette is open, and
    /// lowercasing a thousand paths per frame is a lot of garbage for a
    /// result that never changes. The scan computes it once.
    lower_path: String,
    /// Lowercased file name (the last path segment, without the trailing
    /// separator).
    lower_name: String,
    pub kind: PathKind,
}

impl PathCandidate {
    fn new(path: String, kind: PathKind) -> Self {
        let lower_path = path.to_lowercase();
        let lower_name = lower_path
            .trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or(&lower_path)
            .to_string();
        Self {
            path,
            lower_path,
            lower_name,
            kind,
        }
    }
}

/// The `@`-mention token the caret currently sits in.
#[derive(Clone, Debug)]
pub struct MentionToken {
    /// Byte offset of the `@` inside the composer buffer.
    pub start: usize,
    /// The token text, including the leading `@`.
    pub text: String,
}

impl MentionToken {
    /// The path fragment typed after the `@`.
    pub fn fragment(&self) -> &str {
        self.text.strip_prefix('@').unwrap_or(&self.text)
    }
}

/// Locate the `@`-mention token the caret sits in.
///
/// Returns `None` unless the caret is inside a whitespace-delimited word
/// that starts with `@` and has at least one character after it. The
/// one-character minimum keeps ordinary prose ("email me @bob") from popping
/// a file palette over the user's draft.
pub fn mention_at(input: &str, cursor: usize) -> Option<MentionToken> {
    let cursor = cursor.min(input.len());
    if cursor > 0 && !input.is_char_boundary(cursor) {
        return None;
    }
    let head = &input[..cursor];
    // Walk back to the start of the current word.
    let mut start = cursor;
    for (idx, ch) in head.char_indices().rev() {
        if ch.is_whitespace() {
            break;
        }
        start = idx;
    }
    let token = &input[start..cursor];
    if token.len() < 2 || !token.starts_with('@') {
        return None;
    }
    Some(MentionToken {
        start,
        text: token.to_string(),
    })
}

/// A cached directory scan.
struct ScanCache {
    root: PathBuf,
    at: Instant,
    candidates: std::sync::Arc<Vec<PathCandidate>>,
}

fn scan_cache() -> &'static Mutex<Option<ScanCache>> {
    static CACHE: OnceLock<Mutex<Option<ScanCache>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// The directory the mention paths are relative to.
pub fn scan_root() -> Option<PathBuf> {
    std::env::current_dir().ok()
}

/// Walk the root directory, collecting files and directories.
fn scan(root: &Path) -> Vec<PathCandidate> {
    let mut out: Vec<PathCandidate> = Vec::new();
    let mut stack: Vec<(PathBuf, usize)> = vec![(root.to_path_buf(), 0usize)];
    let mut visited = 0usize;

    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut children: Vec<_> = entries.flatten().collect();
        // Stable order keeps the palette from reshuffling between scans.
        children.sort_by_key(|entry| entry.file_name());

        for entry in children {
            visited += 1;
            if visited > MAX_VISITED {
                return out;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            // Follow symlinks so a linked source tree is visible; the depth
            // cap keeps a symlink cycle from turning into an infinite walk.
            let Ok(metadata) = std::fs::metadata(&path) else {
                continue;
            };
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            if metadata.is_dir() {
                out.push(PathCandidate::new(
                    format!("{relative}/"),
                    PathKind::Directory,
                ));
                if depth < MAX_DEPTH && !SKIPPED_DIRS.contains(&name.as_str()) {
                    stack.push((path, depth + 1));
                }
            } else if metadata.is_file() {
                out.push(PathCandidate::new(relative, PathKind::File));
            }
        }
    }

    out
}

/// Cached scan of the mention root.
///
/// The scan is shared behind an `Arc` so a cache hit costs one pointer copy
/// rather than a clone of every candidate.
fn cached_candidates() -> std::sync::Arc<Vec<PathCandidate>> {
    static EMPTY: std::sync::OnceLock<std::sync::Arc<Vec<PathCandidate>>> =
        std::sync::OnceLock::new();
    let empty = || {
        EMPTY
            .get_or_init(|| std::sync::Arc::new(Vec::new()))
            .clone()
    };

    let Some(root) = scan_root() else {
        return empty();
    };

    let mut cache = match scan_cache().lock() {
        Ok(cache) => cache,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(entry) = cache.as_ref()
        && entry.root == root
        && entry.at.elapsed() < SCAN_TTL
    {
        return entry.candidates.clone();
    }

    let candidates = std::sync::Arc::new(scan(&root));
    *cache = Some(ScanCache {
        root,
        at: Instant::now(),
        candidates: candidates.clone(),
    });
    candidates
}

/// Rank a candidate against the typed fragment.
///
/// Returns `None` when the candidate does not match at all. Lower is better:
/// a basename prefix beats a basename substring, which beats a match anywhere
/// in the path, which beats a typo-tolerant fuzzy match.
fn rank(candidate: &PathCandidate, fragment: &str) -> Option<(u8, usize)> {
    if fragment.is_empty() {
        return Some((3, candidate.path.len()));
    }
    let lower_fragment = fragment.to_lowercase();
    if candidate.lower_name.starts_with(&lower_fragment) {
        return Some((0, candidate.path.len()));
    }
    if candidate.lower_name.contains(&lower_fragment) {
        return Some((1, candidate.path.len()));
    }
    if candidate.lower_path.contains(&lower_fragment) {
        return Some((2, candidate.path.len()));
    }
    if crate::alphacode_tui::tui::fuzzy::fuzzy_score(&lower_fragment, &candidate.lower_path)
        .is_some()
    {
        return Some((3, candidate.path.len()));
    }
    None
}

/// Path candidates matching the typed fragment, best match first.
pub fn candidates(fragment: &str) -> Vec<PathCandidate> {
    let all = cached_candidates();
    let mut scored: Vec<(u8, usize, PathCandidate)> = Vec::new();
    for candidate in all.iter() {
        let Some((bucket, len)) = rank(candidate, fragment) else {
            continue;
        };
        scored.push((bucket, len, candidate.clone()));
    }
    scored.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(&b.1))
            .then_with(|| a.2.path.cmp(&b.2.path))
    });
    scored
        .into_iter()
        .take(MAX_CANDIDATES)
        .map(|(_, _, candidate)| candidate)
        .collect()
}

/// Label shown in the palette, including the leading `@`.
pub fn label(candidate: &PathCandidate) -> String {
    format!("@{}", candidate.path)
}

/// Short description shown next to the label.
pub fn description(candidate: &PathCandidate) -> &'static str {
    match candidate.kind {
        PathKind::File => "File",
        PathKind::Directory => "Directory",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mention_needs_an_at_and_a_character() {
        assert!(mention_at("@s", 2).is_some());
        assert!(mention_at("@", 1).is_none());
        assert!(mention_at("hello", 5).is_none());
        assert!(mention_at("email me @bo", 12).is_some());
    }

    #[test]
    fn mention_reports_the_token_start() {
        let mention = mention_at("look at @src/mai", 16).expect("mention");
        assert_eq!(mention.start, 8);
        assert_eq!(mention.text, "@src/mai");
        assert_eq!(mention.fragment(), "src/mai");
    }

    #[test]
    fn mention_stops_at_whitespace() {
        let mention = mention_at("@src/main.rs and more", 12).expect("mention");
        assert_eq!(mention.text, "@src/main.rs");
    }

    #[test]
    fn mention_ignores_a_bare_at_sign() {
        assert!(mention_at("a @ b", 4).is_none());
    }

    #[test]
    fn ranking_prefers_basename_prefixes() {
        let file = PathCandidate::new("src/main.rs".into(), PathKind::File);
        let nested = PathCandidate::new("docs/src/main.rs.notes".into(), PathKind::File);
        assert!(rank(&file, "main") < rank(&nested, "main"));
    }

    #[test]
    fn a_candidate_reports_its_lowercased_name() {
        let candidate = PathCandidate::new("Src/Main.RS".into(), PathKind::File);
        assert_eq!(candidate.lower_name, "main.rs");
        assert_eq!(candidate.lower_path, "src/main.rs");
    }
}
