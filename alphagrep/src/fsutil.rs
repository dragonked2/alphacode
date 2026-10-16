//! Shared filesystem plumbing: scoped, ignore-aware walking and text reads.
//!
//! Every mode in alphagrep starts from the same picture of a repository: a
//! root directory, optional glob/type filters, and the user's ignore settings.
//! This module owns that picture so grep, find, outline, and trace cannot drift
//! apart in what they consider a searchable file.

use globset::{Glob, GlobMatcher};
use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

/// Files larger than this are skipped: they are almost never source code and
/// reading them would blow up the caller's memory budget.
pub const MAX_SCAN_BYTES: u64 = 8 * 1024 * 1024;

/// Upper bound on how many files a single search walks.
pub const MAX_WALKED_FILES: usize = 20_000;

/// Directory names that are never worth searching, even with `--hidden`.
const VCS_DIRECTORIES: [&str; 3] = [".git", ".hg", ".svn"];

/// A file that survived scope filtering.
#[derive(Debug, Clone)]
pub struct WalkedFile {
    /// Absolute path, used for reads.
    pub absolute: PathBuf,
    /// Search-root-relative path with forward slashes, used for display.
    pub relative: String,
}

/// A glob filter, either matched against the whole relative path or, when the
/// pattern has no separator, against the file name only.
#[derive(Debug, Clone)]
pub struct PathGlob {
    matcher: GlobMatcher,
    basename_only: bool,
}

impl PathGlob {
    /// Compile one glob pattern.
    pub fn new(pattern: &str) -> Result<Self, String> {
        let trimmed = pattern.trim();
        if trimmed.is_empty() {
            return Err("glob pattern is empty".to_string());
        }
        let matcher = Glob::new(trimmed)
            .map_err(|err| format!("invalid glob {trimmed:?}: {err}"))?
            .compile_matcher();
        Ok(Self {
            matcher,
            basename_only: !trimmed.contains('/') && !trimmed.contains('\\'),
        })
    }

    /// Test one candidate path.
    pub fn is_match(&self, relative: &str, file_name: &str) -> bool {
        if self.basename_only {
            self.matcher.is_match(file_name)
        } else {
            self.matcher.is_match(relative)
        }
    }
}

/// Everything that decides which files a search may look at.
#[derive(Debug, Clone)]
pub struct SearchScope {
    /// Directory (or single file) the search starts from.
    pub root: PathBuf,
    /// Optional positive glob filter.
    pub include: Option<PathGlob>,
    /// Optional `!`-prefixed exclusion glob.
    pub exclude: Option<PathGlob>,
    /// Optional file-type filter already resolved to lowercase extensions.
    pub extensions: Option<&'static [&'static str]>,
    /// Whether hidden entries are included.
    pub hidden: bool,
    /// Whether ignore files are ignored.
    pub no_ignore: bool,
    /// Walk cap.
    pub max_files: usize,
}

impl SearchScope {
    /// Start a scope for a root path.
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_path_buf(),
            include: None,
            exclude: None,
            extensions: None,
            hidden: false,
            no_ignore: false,
            max_files: MAX_WALKED_FILES,
        }
    }

    /// Apply a glob argument. Patterns starting with `!` become exclusions.
    pub fn with_glob(mut self, glob: Option<&str>) -> Result<Self, String> {
        let Some(pattern) = glob.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(self);
        };
        if let Some(excluded) = pattern.strip_prefix('!') {
            self.exclude = Some(PathGlob::new(excluded)?);
        } else {
            self.include = Some(PathGlob::new(pattern)?);
        }
        Ok(self)
    }

    /// Apply a file-type argument such as `rs` or `markdown`.
    pub fn with_file_type(mut self, file_type: Option<&str>) -> Result<Self, String> {
        let Some(name) = file_type.map(str::trim).filter(|value| !value.is_empty()) else {
            return Ok(self);
        };
        let extensions = type_extensions(name)?;
        self.extensions = Some(extensions);
        Ok(self)
    }

    /// Apply the hidden/ignore flags.
    pub fn with_flags(mut self, hidden: bool, no_ignore: bool) -> Self {
        self.hidden = hidden;
        self.no_ignore = no_ignore;
        self
    }

    /// Whether a candidate path belongs to this scope.
    pub fn accepts(&self, relative: &str, file_name: &str) -> bool {
        if let Some(exclude) = &self.exclude
            && exclude.is_match(relative, file_name)
        {
            return false;
        }
        if let Some(include) = &self.include
            && !include.is_match(relative, file_name)
        {
            return false;
        }
        if let Some(extensions) = self.extensions
            && !type_matches(file_name, extensions)
        {
            return false;
        }
        true
    }

    /// Human-readable scope summary used in notes and debug output.
    pub fn describe(&self) -> String {
        let mut parts = vec![format!("root={}", self.root.display())];
        if let Some(include) = &self.include {
            parts.push(format!("glob={}", include.matcher.glob()));
        }
        if let Some(exclude) = &self.exclude {
            parts.push(format!("exclude={}", exclude.matcher.glob()));
        }
        if let Some(extensions) = self.extensions {
            parts.push(format!("type={}", extensions.join("|")));
        }
        if self.hidden {
            parts.push("hidden".to_string());
        }
        if self.no_ignore {
            parts.push("no-ignore".to_string());
        }
        parts.join(" ")
    }
}

/// Collect the files a mode should look at, in deterministic path order.
///
/// A scope rooted at a regular file yields exactly that file, which is how the
/// outline and single-file trace paths reach their target.
pub fn collect_files(scope: &SearchScope) -> Vec<WalkedFile> {
    if scope.root.is_file() {
        let file_name = file_name_of(&scope.root);
        if !scope.accepts(&file_name, &file_name) {
            return Vec::new();
        }
        return vec![WalkedFile {
            relative: file_name,
            absolute: scope.root.clone(),
        }];
    }

    let mut builder = WalkBuilder::new(&scope.root);
    builder.standard_filters(!scope.no_ignore);
    builder.hidden(!scope.hidden);
    builder.follow_links(false);
    builder.sort_by_file_path(|left, right| left.cmp(right));
    builder.filter_entry(|entry| {
        if entry.depth() == 0 {
            return true;
        }
        let name = entry.file_name().to_string_lossy();
        !VCS_DIRECTORIES.iter().any(|directory| name == *directory)
    });

    let mut files = Vec::new();
    for entry in builder.build() {
        let Ok(entry) = entry else { continue };
        let is_file = entry
            .file_type()
            .map(|kind| kind.is_file())
            .unwrap_or(false);
        if !is_file {
            continue;
        }
        let path = entry.path();
        let relative = relative_string(&scope.root, path);
        let file_name = file_name_of(path);
        if !scope.accepts(&relative, &file_name) {
            continue;
        }
        files.push(WalkedFile {
            absolute: path.to_path_buf(),
            relative,
        });
        if files.len() >= scope.max_files {
            break;
        }
    }
    files
}

/// Read a text file, returning `None` for unreadable, oversized, or binary
/// content. Invalid UTF-8 is replaced rather than rejected so legacy sources
/// still match.
pub fn read_text(path: &Path) -> Option<String> {
    let metadata = std::fs::metadata(path).ok()?;
    if metadata.len() > MAX_SCAN_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes.iter().take(8192).any(|byte| *byte == 0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Path relative to `root`, always with `/` separators.
pub fn relative_string(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    let mut text = relative.to_string_lossy().replace('\\', "/");
    if text.is_empty() {
        text = ".".to_string();
    }
    text
}

/// File name of a path, or the whole path when it has none.
pub fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

/// Compare two display paths that may come from different producers, which mix
/// `/` and `\` on Windows.
pub fn same_path(left: &str, right: &str) -> bool {
    let normalize = |value: &str| value.trim().replace('\\', "/").to_ascii_lowercase();
    normalize(left) == normalize(right)
}

/// Extensions understood by `--type`, mirroring the names ripgrep users know.
pub fn type_extensions(name: &str) -> Result<&'static [&'static str], String> {
    let normalized = name.trim().to_ascii_lowercase();
    let extensions: &'static [&'static str] = match normalized.as_str() {
        "rs" | "rust" => &["rs"],
        "py" | "python" => &["py", "pyi"],
        "js" | "javascript" | "mjs" => &["js", "mjs", "cjs"],
        "jsx" => &["jsx"],
        "ts" | "typescript" => &["ts", "mts", "cts"],
        "tsx" => &["tsx"],
        "go" | "golang" => &["go"],
        "java" => &["java"],
        "kt" | "kotlin" => &["kt", "kts"],
        "c" => &["c", "h"],
        "cc" | "cpp" | "c++" | "cxx" => &["cc", "cpp", "cxx", "hpp", "hh", "hxx", "h"],
        "cs" | "csharp" => &["cs"],
        "rb" | "ruby" => &["rb", "rake", "gemspec"],
        "php" => &["php"],
        "sh" | "bash" | "shell" => &["sh", "bash", "zsh", "ksh"],
        "ps1" | "powershell" => &["ps1", "psm1"],
        "sql" => &["sql"],
        "html" => &["html", "htm"],
        "css" => &["css"],
        "scss" => &["scss"],
        "md" | "markdown" => &["md", "markdown", "mdx"],
        "json" | "jsonc" => &["json", "jsonc"],
        "toml" => &["toml"],
        "yaml" | "yml" => &["yaml", "yml"],
        "xml" => &["xml"],
        "txt" | "text" => &["txt"],
        "swift" => &["swift"],
        "lua" => &["lua"],
        "dart" => &["dart"],
        "scala" | "sc" => &["scala", "sc"],
        "ex" | "elixir" => &["ex", "exs"],
        "erl" | "erlang" => &["erl", "hrl"],
        "hs" | "haskell" => &["hs"],
        "ml" | "ocaml" => &["ml", "mli"],
        "pl" | "perl" => &["pl", "pm"],
        "r" => &["r"],
        "zig" => &["zig"],
        "nim" => &["nim"],
        "vue" => &["vue"],
        "svelte" => &["svelte"],
        "proto" | "protobuf" => &["proto"],
        "tf" | "terraform" => &["tf", "tfvars"],
        "gradle" | "groovy" => &["gradle", "groovy"],
        "docker" | "dockerfile" => &["dockerfile"],
        "make" | "makefile" => &["makefile", "mk"],
        "ini" | "cfg" | "conf" => &["ini", "cfg", "conf"],
        "lock" => &["lock"],
        _ => {
            return Err(format!(
                "unknown file type {name:?}; try one of: rs, py, js, ts, tsx, go, java, c, cpp, cs, rb, php, sh, sql, html, css, md, json, toml, yaml, xml, txt, or omit it"
            ));
        }
    };
    Ok(extensions)
}

/// Whether a file name matches one of the extensions of a type filter.
fn type_matches(file_name: &str, extensions: &[&str]) -> bool {
    let lowered = file_name.to_ascii_lowercase();
    let extension = lowered.rsplit_once('.').map(|(_, tail)| tail).unwrap_or("");
    extensions
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(extension) || lowered == *candidate)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glob_without_separator_matches_the_file_name() {
        let glob = PathGlob::new("*.rs").expect("glob");
        assert!(glob.is_match("src/lib.rs", "lib.rs"));
        assert!(!glob.is_match("src/lib.toml", "lib.toml"));
    }

    #[test]
    fn glob_with_separator_matches_the_relative_path() {
        let glob = PathGlob::new("src/**/*.rs").expect("glob");
        assert!(glob.is_match("src/a/b.rs", "b.rs"));
        assert!(!glob.is_match("tests/a.rs", "a.rs"));
    }

    #[test]
    fn exclusion_globs_are_split_off() {
        let scope = SearchScope::new(Path::new("."))
            .with_glob(Some("!target/**"))
            .expect("scope");
        assert!(scope.include.is_none());
        assert!(scope.exclude.is_some());
        assert!(!scope.accepts("target/debug/app.rs", "app.rs"));
        assert!(scope.accepts("src/main.rs", "main.rs"));
    }

    #[test]
    fn file_type_filter_accepts_alias_names_and_special_files() {
        let scope = SearchScope::new(Path::new("."))
            .with_file_type(Some("rust"))
            .expect("scope");
        assert!(scope.accepts("src/lib.rs", "lib.rs"));
        assert!(!scope.accepts("src/lib.py", "lib.py"));

        let docker = SearchScope::new(Path::new("."))
            .with_file_type(Some("dockerfile"))
            .expect("scope");
        assert!(docker.accepts("Dockerfile", "Dockerfile"));
    }

    #[test]
    fn unknown_file_type_is_rejected_with_a_hint() {
        let error = type_extensions("cobol").expect_err("should fail");
        assert!(error.contains("unknown file type"));
    }
}
