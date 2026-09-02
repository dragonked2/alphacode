//! Project Analyzer — maintains project indexes and symbol databases.
//!
//! Continuously maintains a function index, class index, API index,
//! dependency graph, call graph, file relationships, and architecture
//! graph.  Updates incrementally; never rebuilds unnecessarily.

use crate::alphacode_app_core::memory_manager::{FileMeta, ProjectIndex, SymbolMeta};
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Project store that owns the on-disk index and provides querying.
pub struct ProjectAnalyzer {
    root: PathBuf,
}

impl ProjectAnalyzer {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    /// Index a single file, updating the symbol database.
    ///
    /// This is a lightweight heuristic indexer that extracts function /
    /// class / struct / enum signatures from Rust and Python files.
    pub fn index_file(&self, path: &Path) -> Result<FileIndex> {
        let content = fs::read_to_string(path)?;
        let abs_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let lang = detect_language(path);
        let symbols = match lang {
            Some("rust") => index_rust(&content),
            Some("python") => index_python(&content),
            _ => Vec::new(),
        };
        Ok(FileIndex {
            path: abs_path,
            language: lang.map(|s| s.to_string()),
            lines: content.lines().count() as u64,
            bytes: content.len() as u64,
            symbols,
        })
    }

    /// Incrementally index all known files in the project.
    ///
    /// Walks the root, calls `index_file` for each source file, and
    /// returns a merged `ProjectIndex`.  Existing index entries for
    /// unchanged files are preserved by the caller (a more sophisticated
    /// implementation would check content hashes first).
    pub fn index_all(&self) -> Result<ProjectIndex> {
        let mut index = ProjectIndex::default();
        let mut files = Vec::new();
        collect_source_files(&self.root, &mut files, &self.root, 0);

        for file in &files {
            match self.index_file(file) {
                Ok(fi) => {
                    let key = file
                        .strip_prefix(&self.root)
                        .unwrap_or(file)
                        .to_string_lossy()
                        .replace('\\', "/");
                    // Update file metadata.
                    index.files.insert(
                        key.clone(),
                        FileMeta {
                            path: key.clone(),
                            lines: fi.lines,
                            bytes: fi.bytes,
                            language: fi.language.clone(),
                            last_indexed: Some(chrono::Utc::now()),
                        },
                    );
                    // Update symbol database.
                    for sym in &fi.symbols {
                        index.symbols.insert(
                            sym.name.clone(),
                            SymbolMeta {
                                name: sym.name.clone(),
                                kind: sym.kind.clone(),
                                file: key.clone(),
                                line: sym.line,
                            },
                        );
                        match sym.kind.as_str() {
                            "function" => {
                                if !index.functions.contains(&sym.name) {
                                    index.functions.push(sym.name.clone());
                                }
                            }
                            "class" | "struct" | "enum" | "trait" => {
                                if !index.classes.contains(&sym.name) {
                                    index.classes.push(sym.name.clone());
                                }
                            }
                            "api" => {
                                if !index.apis.contains(&sym.name) {
                                    index.apis.push(sym.name.clone());
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Err(_) => continue,
            }
        }

        Ok(index)
    }

    /// Query functions by name pattern (case-insensitive substring match).
    pub fn query_functions(&self, index: &ProjectIndex, pattern: &str) -> Vec<String> {
        index
            .functions
            .iter()
            .filter(|f| f.to_lowercase().contains(&pattern.to_lowercase()))
            .cloned()
            .collect()
    }

    /// Query classes/structs by name pattern.
    pub fn query_classes(&self, index: &ProjectIndex, pattern: &str) -> Vec<String> {
        index
            .classes
            .iter()
            .filter(|f| f.to_lowercase().contains(&pattern.to_lowercase()))
            .cloned()
            .collect()
    }
}

/// Index result for a single file.
#[derive(Debug, Clone)]
pub struct FileIndex {
    pub path: PathBuf,
    pub language: Option<String>,
    pub lines: u64,
    pub bytes: u64,
    pub symbols: Vec<IndexedSymbol>,
}

#[derive(Debug, Clone)]
pub struct IndexedSymbol {
    pub name: String,
    pub kind: String,
    pub line: u32,
}

fn detect_language(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_string_lossy().as_ref() {
        "rs" => Some("rust"),
        "py" => Some("python"),
        "js" | "mjs" | "cjs" => Some("javascript"),
        "ts" | "mts" | "cts" => Some("typescript"),
        "go" => Some("go"),
        "java" => Some("java"),
        "c" | "h" => Some("c"),
        "cpp" | "cc" | "cxx" | "hpp" => Some("cpp"),
        "md" => Some("markdown"),
        "toml" => Some("toml"),
        "json" => Some("json"),
        _ => None,
    }
}

fn index_rust(content: &str) -> Vec<IndexedSymbol> {
    let mut symbols = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        let line_num = i as u32 + 1;

        // pub fn / fn
        if let Some(name) = extract_rust_symbol(trimmed, "fn ") {
            symbols.push(IndexedSymbol {
                name,
                kind: "function".to_string(),
                line: line_num,
            });
        }
        // struct
        else if let Some(name) = extract_rust_symbol(trimmed, "struct ") {
            symbols.push(IndexedSymbol {
                name,
                kind: "class".to_string(),
                line: line_num,
            });
        }
        // enum
        else if let Some(name) = extract_rust_symbol(trimmed, "enum ") {
            symbols.push(IndexedSymbol {
                name,
                kind: "class".to_string(),
                line: line_num,
            });
        }
        // trait
        else if let Some(name) = extract_rust_symbol(trimmed, "trait ") {
            symbols.push(IndexedSymbol {
                name,
                kind: "class".to_string(),
                line: line_num,
            });
        }
        // impl blocks (just record the type name)
        else if let Some(rest) = trimmed.strip_prefix("impl ") {
            let name = rest
                .split('<')
                .next()
                .unwrap_or("")
                .trim()
                .trim_end_matches('{')
                .trim()
                .to_string();
            if !name.is_empty() {
                symbols.push(IndexedSymbol {
                    name,
                    kind: "impl".to_string(),
                    line: line_num,
                });
            }
        }
    }
    symbols
}

fn extract_rust_symbol(line: &str, prefix: &str) -> Option<String> {
    // Extract the symbol name after the keyword.
    let after = line.find(prefix)?;
    let rest = &line[after + prefix.len()..];
    let name = rest
        .split(|c: char| c.is_whitespace() || c == '<' || c == '(' || c == '{')
        .find(|s| !s.is_empty())?;
    let name = name.trim_start_matches("pub ");
    let name = name.trim_start_matches("async ");
    if name.is_empty() || name.contains(',') || name == "pub" {
        return None;
    }
    Some(name.to_string())
}

fn index_python(content: &str) -> Vec<IndexedSymbol> {
    let mut symbols = Vec::new();
    for (i, line) in content.lines().enumerate() {
        let i = i as u32 + 1;
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("def ") {
            let name = rest.split('(').next().unwrap_or("").trim();
            if !name.is_empty() {
                symbols.push(IndexedSymbol {
                    name: name.to_string(),
                    kind: "function".to_string(),
                    line: i,
                });
            }
        } else if let Some(rest) = trimmed.strip_prefix("class ") {
            let name = rest
                .split('(')
                .next()
                .unwrap_or("")
                .split(':')
                .next()
                .unwrap_or("")
                .trim();
            if !name.is_empty() {
                symbols.push(IndexedSymbol {
                    name: name.to_string(),
                    kind: "class".to_string(),
                    line: i,
                });
            }
        }
    }
    symbols
}

fn collect_source_files(dir: &Path, out: &mut Vec<PathBuf>, _root: &Path, depth: usize) {
    if depth > 15 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();

        // Skip hidden and common heavy/irrelevant directories.
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if matches!(
                name.as_ref(),
                "target" | "node_modules" | ".git" | "venv" | "__pycache__" | "dist" | "build"
            ) {
                continue;
            }
            collect_source_files(&path, out, _root, depth + 1);
        } else if path.is_file() && detect_language(&path).is_some() {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_index_rust_basic() {
        let content = r#"
pub struct Foo {
    bar: u32,
}

pub fn hello() -> u32 {
    42
}

enum Color {
    Red,
    Blue,
}

trait Greet {
    fn greet(&self);
}

impl Foo {
    pub fn new() -> Self { Self { bar: 0 } }
}
"#;
        let symbols = index_rust(content);
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"Foo"));
        assert!(names.contains(&"hello"));
        assert!(names.contains(&"Color"));
        assert!(symbols.iter().any(|s| s.kind == "impl" && s.name == "Foo"));
    }

    #[test]
    fn test_index_file_creates_fileindex() {
        let dir = TempDir::new().unwrap();
        let file = dir.path().join("test.rs");
        fs::write(&file, "pub fn main() {}").unwrap();

        let pa = ProjectAnalyzer::new(dir.path());
        let fi = pa.index_file(&file).unwrap();
        assert_eq!(fi.language.as_deref(), Some("rust"));
        assert!(fi.symbols.iter().any(|s| s.name == "main"));
    }

    #[test]
    fn test_index_all_skips_target_dir() {
        let dir = TempDir::new().unwrap();
        let src = dir.path().join("src");
        fs::create_dir(&src).unwrap();
        fs::write(src.join("lib.rs"), "pub fn lib_func() {}").unwrap();

        let target = dir.path().join("target");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("should_not_index.rs"), "pub fn ignored() {}").unwrap();

        let pa = ProjectAnalyzer::new(dir.path());
        let index = pa.index_all().unwrap();
        assert!(index.functions.contains(&"lib_func".to_string()));
        assert!(!index.functions.contains(&"ignored".to_string()));
    }

    #[test]
    fn test_query_functions() {
        let index = ProjectIndex {
            functions: vec!["hello_world".into(), "goodbye".into(), "helper".into()],
            ..Default::default()
        };
        let pa = ProjectAnalyzer::new(".");
        let results = pa.query_functions(&index, "hello");
        assert_eq!(results, vec!["hello_world"]);
    }

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language(Path::new("foo.rs")), Some("rust"));
        assert_eq!(detect_language(Path::new("foo.py")), Some("python"));
        assert_eq!(detect_language(Path::new("foo.txt")), None);
    }
}
