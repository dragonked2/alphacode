//! Lightweight, language-agnostic structure extraction.
//!
//! alphagrep deliberately avoids parser dependencies. Instead it recognises the
//! declaration shapes that dominate real source files with anchored regular
//! expressions and then measures each declaration's body with a brace or
//! indentation scan. The result is a best-effort outline: cheap to compute,
//! dependency free, and honest about its limits for formats it does not know.

use regex::Regex;
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, OnceLock};

/// Hard cap on how many items one file may contribute.
pub const MAX_ITEMS: usize = 600;

/// One declaration found in a file. Line numbers are 1-based and inclusive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StructureItem {
    /// Short kind label such as `fn`, `struct`, `class`, or `heading`.
    pub kind: &'static str,
    /// Name (or best available label) of the declaration.
    pub label: String,
    /// First line of the declaration.
    pub start_line: usize,
    /// Last line of the measured body.
    pub end_line: usize,
    /// Coarse nesting depth, derived from indentation.
    pub depth: usize,
    /// Trimmed source line, handy for disambiguation.
    pub signature: String,
}

/// Language family used to pick detection rules and body measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    Rust,
    Python,
    JavaScript,
    Go,
    CFamily,
    Shell,
    Ruby,
    Php,
    Sql,
    Markdown,
    Config,
    Generic,
}

impl Language {
    /// Pick a language family from a file extension.
    pub fn from_path(path: &Path) -> Self {
        let extension = path
            .extension()
            .map(|value| value.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        match extension.as_str() {
            "rs" => Self::Rust,
            "py" | "pyi" => Self::Python,
            "js" | "mjs" | "cjs" | "jsx" | "ts" | "tsx" | "mts" | "cts" | "vue" | "svelte" => {
                Self::JavaScript
            }
            "go" => Self::Go,
            "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" | "cs" | "java" | "kt"
            | "kts" | "swift" | "scala" | "sc" | "m" | "mm" | "dart" | "zig" | "nim" | "lua"
            | "proto" | "gradle" | "groovy" => Self::CFamily,
            "sh" | "bash" | "zsh" | "ksh" | "fish" | "ps1" | "psm1" => Self::Shell,
            "rb" | "rake" | "gemspec" => Self::Ruby,
            "php" => Self::Php,
            "sql" => Self::Sql,
            "md" | "markdown" | "mdx" => Self::Markdown,
            "toml" | "yaml" | "yml" | "ini" | "cfg" | "conf" | "properties" | "json" | "jsonc" => {
                Self::Config
            }
            _ => Self::Generic,
        }
    }

    /// Display name used in summaries.
    pub fn label(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::JavaScript => "javascript",
            Self::Go => "go",
            Self::CFamily => "c-family",
            Self::Shell => "shell",
            Self::Ruby => "ruby",
            Self::Php => "php",
            Self::Sql => "sql",
            Self::Markdown => "markdown",
            Self::Config => "config",
            Self::Generic => "generic",
        }
    }
}

/// One detection rule: a kind label plus an anchored pattern with the name in
/// capture group 1 (or group 2 for headings).
struct Rule {
    kind: &'static str,
    pattern: &'static str,
}

/// Extract the structure of `text`.
pub fn extract_structure(text: &str, path: &Path) -> Vec<StructureItem> {
    let language = Language::from_path(path);
    let rules = compiled_rules(language);
    let lines: Vec<&str> = text.lines().collect();
    let mut items: Vec<StructureItem> = Vec::new();

    for (index, line) in lines.iter().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || is_comment_like(trimmed, language) {
            continue;
        }
        for rule in rules.iter() {
            let kind = rule.kind;
            let Some(captures) = rule.regex.captures(line) else {
                continue;
            };
            let Some(label) = capture_label(kind, &captures) else {
                continue;
            };
            if is_rejected_match(kind, trimmed, language) {
                break;
            }
            let start_line = index + 1;
            if items.iter().any(|item| item.start_line == start_line) {
                break;
            }
            let end_index = body_end_index(&lines, language, index);
            let depth = if kind == "heading" {
                // Markdown nesting follows the heading level; every other rule
                // approximates nesting from indentation.
                heading_level(line).unwrap_or(1).saturating_sub(1)
            } else {
                (indent_width(line) / 4).min(8)
            };
            items.push(StructureItem {
                kind,
                label: safe_label(&label),
                start_line,
                end_line: end_index + 1,
                depth,
                signature: truncate(trimmed, 120),
            });
            if items.len() >= MAX_ITEMS {
                return items;
            }
            break;
        }
    }
    items
}

/// Normalize a label so rendered item and region headers stay parseable by the
/// alphacode session harness, which splits headers on ` @ `.
pub fn safe_label(raw: &str) -> String {
    let mut out = String::new();
    let mut pending_space = false;
    for character in raw.trim().chars() {
        if character.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(if character == '@' { '#' } else { character });
        if out.chars().count() >= 64 {
            break;
        }
    }
    out
}

/// Collapse whitespace and cut a line to `limit` characters.
pub fn truncate(text: &str, limit: usize) -> String {
    let mut out = String::new();
    let mut pending_space = false;
    for character in text.trim().chars() {
        if character.is_whitespace() {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        if out.chars().count() >= limit {
            out.push('…');
            return out;
        }
        out.push(character);
    }
    out
}

/// One compiled rule: the item kind and its matcher.
struct CompiledRule {
    kind: &'static str,
    regex: Regex,
}
/// Compiled rules per language, cached on first use.
type RuleCache = Mutex<HashMap<Language, Arc<Vec<CompiledRule>>>>;

fn compiled_rules(language: Language) -> Arc<Vec<CompiledRule>> {
    static CACHE: OnceLock<RuleCache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    if let Ok(guard) = cache.lock()
        && let Some(rules) = guard.get(&language)
    {
        return Arc::clone(rules);
    }
    let compiled: Vec<CompiledRule> = rules_for(language)
        .iter()
        .filter_map(|rule| {
            Regex::new(rule.pattern).ok().map(|regex| CompiledRule {
                kind: rule.kind,
                regex,
            })
        })
        .collect();
    let compiled = Arc::new(compiled);
    if let Ok(mut guard) = cache.lock() {
        guard.insert(language, Arc::clone(&compiled));
    }
    compiled
}

fn capture_label(kind: &str, captures: &regex::Captures<'_>) -> Option<String> {
    let group = if kind == "heading" { 2 } else { 1 };
    let raw = captures.get(group)?.as_str().trim();
    if raw.is_empty() {
        return None;
    }
    Some(raw.to_string())
}

fn is_comment_like(trimmed: &str, language: Language) -> bool {
    if trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with("--")
    {
        return true;
    }
    trimmed.starts_with('#') && language != Language::Markdown
}

/// Statement keywords that must never be mistaken for a declaration.
const STATEMENT_KEYWORDS: [&str; 25] = [
    "return", "if", "else", "for", "while", "switch", "case", "break", "continue", "do", "new",
    "delete", "throw", "catch", "sizeof", "assert", "typedef", "using", "import", "export", "let",
    "await", "yield", "emit", "goto",
];

fn is_rejected_match(kind: &str, trimmed: &str, language: Language) -> bool {
    if kind != "fn" || language != Language::CFamily {
        return false;
    }
    let first_word = trimmed.split_whitespace().next().unwrap_or("");
    STATEMENT_KEYWORDS.contains(&first_word)
}

fn indent_width(line: &str) -> usize {
    let mut width = 0;
    for character in line.chars() {
        match character {
            ' ' => width += 1,
            '\t' => width += 4,
            _ => break,
        }
    }
    width
}

fn body_end_index(lines: &[&str], language: Language, index: usize) -> usize {
    match language {
        Language::Markdown => heading_end_index(lines, index),
        Language::Python | Language::Ruby | Language::Config => indent_end_index(lines, index),
        Language::Sql => statement_end_index(lines, index),
        _ => brace_end_index(lines, index),
    }
}

fn brace_end_index(lines: &[&str], index: usize) -> usize {
    let limit = (index + 400).min(lines.len().saturating_sub(1));
    let mut depth = 0i32;
    let mut opened = false;
    for position in index..=limit {
        let delta = brace_delta(lines[position]);
        depth += delta;
        if delta > 0 {
            opened = true;
        }
        if opened && depth <= 0 {
            return trim_trailing_blank_lines(lines, position, index);
        }
    }
    if opened {
        return trim_trailing_blank_lines(lines, limit, index);
    }
    trim_trailing_blank_lines(lines, index, index)
}

fn indent_end_index(lines: &[&str], index: usize) -> usize {
    let base = indent_width(lines[index]);
    let limit = (index + 400).min(lines.len().saturating_sub(1));
    for position in (index + 1)..=limit {
        let line = lines[position];
        if line.trim().is_empty() {
            continue;
        }
        if indent_width(line) <= base {
            return trim_trailing_blank_lines(lines, position.saturating_sub(1), index);
        }
    }
    trim_trailing_blank_lines(lines, limit, index)
}

fn statement_end_index(lines: &[&str], index: usize) -> usize {
    let limit = (index + 200).min(lines.len().saturating_sub(1));
    for position in index..=limit {
        let trimmed = lines[position].trim_end();
        if trimmed.ends_with(';') || trimmed.ends_with("$$") {
            return trim_trailing_blank_lines(lines, position, index);
        }
    }
    trim_trailing_blank_lines(lines, limit, index)
}

fn heading_end_index(lines: &[&str], index: usize) -> usize {
    let level = heading_level(lines[index]).unwrap_or(1);
    let limit = lines.len().saturating_sub(1);
    for position in (index + 1)..=limit {
        if let Some(next_level) = heading_level(lines[position])
            && next_level <= level
        {
            return trim_trailing_blank_lines(lines, position.saturating_sub(1), index);
        }
    }
    trim_trailing_blank_lines(lines, limit, index)
}

fn heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let hashes = trimmed
        .chars()
        .take_while(|character| *character == '#')
        .count();
    if (1..=6).contains(&hashes) && trimmed.chars().nth(hashes) == Some(' ') {
        Some(hashes)
    } else {
        None
    }
}

fn trim_trailing_blank_lines(lines: &[&str], from: usize, floor: usize) -> usize {
    let mut position = from.max(floor).min(lines.len().saturating_sub(1));
    while position > floor && lines[position].trim().is_empty() {
        position -= 1;
    }
    position
}

/// Net brace depth change of a line, ignoring comments and string contents.
/// Raw strings and unusual literals can confuse this; it is a heuristic.
fn brace_delta(line: &str) -> i32 {
    let mut depth = 0i32;
    let mut chars = line.chars().peekable();
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    let mut quote: Option<char> = None;
    while let Some(current) = chars.next() {
        if in_line_comment {
            break;
        }
        if let Some(active) = quote {
            if current == '\\' {
                let _ = chars.next();
                continue;
            }
            if current == active {
                quote = None;
            }
            continue;
        }
        if in_block_comment {
            if current == '*' && chars.peek() == Some(&'/') {
                let _ = chars.next();
                in_block_comment = false;
            }
            continue;
        }
        match current {
            '"' | '\'' | '`' => quote = Some(current),
            '/' => match chars.peek() {
                Some('/') => in_line_comment = true,
                Some('*') => {
                    let _ = chars.next();
                    in_block_comment = true;
                }
                _ => {}
            },
            '{' => depth += 1,
            '}' => depth -= 1,
            _ => {}
        }
    }
    depth
}

fn rules_for(language: Language) -> &'static [Rule] {
    match language {
        Language::Rust => RUST_RULES,
        Language::Python => PYTHON_RULES,
        Language::JavaScript => JAVASCRIPT_RULES,
        Language::Go => GO_RULES,
        Language::CFamily => C_FAMILY_RULES,
        Language::Shell => SHELL_RULES,
        Language::Ruby => RUBY_RULES,
        Language::Php => PHP_RULES,
        Language::Sql => SQL_RULES,
        Language::Markdown => MARKDOWN_RULES,
        Language::Config => CONFIG_RULES,
        Language::Generic => GENERIC_RULES,
    }
}

const RUST_RULES: &[Rule] = &[
    Rule {
        kind: "impl",
        pattern: r"^[ \t]*impl(?:[ \t]*<[^>]*>)?[ \t]+(.+?)[ \t]*(?:where[^{]*)?\{",
    },
    Rule {
        kind: "fn",
        pattern: r#"^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?(?:default[ \t]+)?(?:async[ \t]+)?(?:unsafe[ \t]+)?(?:const[ \t]+)?(?:extern[ \t]+"[^"]*"[ \t]+)?fn[ \t]+([A-Za-z_][A-Za-z0-9_]*)"#,
    },
    Rule {
        kind: "struct",
        pattern: r"^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?struct[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    },
    Rule {
        kind: "enum",
        pattern: r"^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?enum[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    },
    Rule {
        kind: "trait",
        pattern: r"^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?(?:unsafe[ \t]+)?trait[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    },
    Rule {
        kind: "macro",
        pattern: r"^[ \t]*macro_rules![ \t]*([A-Za-z_][A-Za-z0-9_]*)",
    },
    Rule {
        kind: "mod",
        pattern: r"^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?mod[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    },
    Rule {
        kind: "const",
        pattern: r"^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?(?:const|static)[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    },
    Rule {
        kind: "type",
        pattern: r"^[ \t]*(?:pub(?:\([^)]*\))?[ \t]+)?type[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    },
];

const PYTHON_RULES: &[Rule] = &[
    Rule {
        kind: "class",
        pattern: r"^[ \t]*class[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    },
    Rule {
        kind: "def",
        pattern: r"^[ \t]*(?:async[ \t]+)?def[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    },
    Rule {
        kind: "const",
        pattern: r"^([A-Z][A-Z0-9_]{2,})[ \t]*[:=]",
    },
];

const JAVASCRIPT_RULES: &[Rule] = &[
    Rule {
        kind: "class",
        pattern: r"^[ \t]*(?:export[ \t]+)?(?:default[ \t]+)?(?:abstract[ \t]+)?class[ \t]+([A-Za-z_$][A-Za-z0-9_$]*)",
    },
    Rule {
        kind: "interface",
        pattern: r"^[ \t]*(?:export[ \t]+)?interface[ \t]+([A-Za-z_$][A-Za-z0-9_$]*)",
    },
    Rule {
        kind: "enum",
        pattern: r"^[ \t]*(?:export[ \t]+)?(?:const[ \t]+)?enum[ \t]+([A-Za-z_$][A-Za-z0-9_$]*)",
    },
    Rule {
        kind: "type",
        pattern: r"^[ \t]*(?:export[ \t]+)?type[ \t]+([A-Za-z_$][A-Za-z0-9_$]*)[ \t]*=",
    },
    Rule {
        kind: "fn",
        pattern: r#"^[ \t]*(?:export[ \t]+)?(?:default[ \t]+)?(?:async[ \t]+)?function[ \t]*\*?[ \t]*([A-Za-z_$][A-Za-z0-9_$]*)"#,
    },
    Rule {
        kind: "fn",
        pattern: r"^[ \t]*(?:export[ \t]+)?(?:const|let|var)[ \t]+([A-Za-z_$][A-Za-z0-9_$]*)[ \t]*=[ \t]*(?:async[ \t]*)?(?:function\b|\()",
    },
    Rule {
        kind: "const",
        pattern: r"^[ \t]*(?:export[ \t]+)?(?:const|let|var)[ \t]+([A-Za-z_$][A-Za-z0-9_$]*)[ \t]*=",
    },
];

const GO_RULES: &[Rule] = &[
    Rule {
        kind: "fn",
        pattern: r"^[ \t]*func[ \t]+(?:\([^)]*\)[ \t]*)?([A-Za-z_][A-Za-z0-9_]*)",
    },
    Rule {
        kind: "struct",
        pattern: r"^[ \t]*type[ \t]+([A-Za-z_][A-Za-z0-9_]*)[ \t]+struct\b",
    },
    Rule {
        kind: "interface",
        pattern: r"^[ \t]*type[ \t]+([A-Za-z_][A-Za-z0-9_]*)[ \t]+interface\b",
    },
    Rule {
        kind: "type",
        pattern: r"^[ \t]*type[ \t]+([A-Za-z_][A-Za-z0-9_]*)[ \t]+",
    },
    Rule {
        kind: "const",
        pattern: r"^[ \t]*(?:const|var)[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    },
];

const C_FAMILY_RULES: &[Rule] = &[
    Rule {
        kind: "class",
        pattern: r"^[ \t]*(?:(?:public|private|protected|internal|final|open|abstract|sealed|partial|static|export|data|record|template|inline)[ \t]+)*class[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    },
    Rule {
        kind: "interface",
        pattern: r"^[ \t]*(?:(?:public|private|protected|internal|final|open|abstract|sealed|partial|static|export)[ \t]+)*interface[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    },
    Rule {
        kind: "enum",
        pattern: r"^[ \t]*(?:(?:public|private|protected|internal|final|open|abstract|sealed|partial|static|export)[ \t]+)*enum[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    },
    Rule {
        kind: "struct",
        pattern: r"^[ \t]*(?:(?:public|private|protected|internal|final|open|abstract|sealed|partial|static|export)[ \t]+)*struct[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    },
    Rule {
        kind: "namespace",
        pattern: r"^[ \t]*(?:public[ \t]+)?namespace[ \t]+([A-Za-z_][A-Za-z0-9_:]*)",
    },
    Rule {
        kind: "macro",
        pattern: r"^[ \t]*#define[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    },
    Rule {
        kind: "type",
        pattern: r"^[ \t]*typedef[^;\r\n]*?[ \t]([A-Za-z_][A-Za-z0-9_]*)[ \t]*;",
    },
    Rule {
        kind: "fn",
        pattern: r"^[ \t]*(?:(?:public|private|protected|internal|static|inline|virtual|override|final|constexpr|consteval|explicit|extern|mutable|unsafe|async|suspend|open|noexcept|delegate)[ \t]+)*(?:[A-Za-z_][A-Za-z0-9_:<>,\[\]\*&]*[ \t\*&]+)+([A-Za-z_][A-Za-z0-9_]*)[ \t]*\([^;{}]*\)[ \t]*(?:const[ \t]*)?(?:noexcept[ \t]*)?(?:override[ \t]*)?(?:->[^{;]*)?(?:[ \t]*\{|[ \t]*;)",
    },
];

const SHELL_RULES: &[Rule] = &[
    Rule {
        kind: "fn",
        pattern: r"^[ \t]*function[ \t]+([A-Za-z_][A-Za-z0-9_-]*)",
    },
    Rule {
        kind: "fn",
        pattern: r"^[ \t]*([A-Za-z_][A-Za-z0-9_-]*)[ \t]*\(\)[ \t]*(?:\{|$)",
    },
    Rule {
        kind: "const",
        pattern: r"^([A-Za-z_][A-Za-z0-9_]*)[ \t]*=[^=]",
    },
];

const RUBY_RULES: &[Rule] = &[
    Rule {
        kind: "class",
        pattern: r"^[ \t]*class[ \t]+([A-Za-z_][A-Za-z0-9_:]*)",
    },
    Rule {
        kind: "module",
        pattern: r"^[ \t]*module[ \t]+([A-Za-z_][A-Za-z0-9_:]*)",
    },
    Rule {
        kind: "def",
        pattern: r"^[ \t]*def[ \t]+(?:self\.)?([A-Za-z_][A-Za-z0-9_]*[?!=]?)",
    },
];

const PHP_RULES: &[Rule] = &[
    Rule {
        kind: "class",
        pattern: r"^[ \t]*(?:(?:final|abstract|readonly)[ \t]+)*(?:class|interface|trait|enum)[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    },
    Rule {
        kind: "fn",
        pattern: r"^[ \t]*(?:(?:public|private|protected|static|final|abstract|readonly)[ \t]+)*function[ \t]+([A-Za-z_][A-Za-z0-9_]*)",
    },
];

const SQL_RULES: &[Rule] = &[
    Rule {
        kind: "table",
        pattern: r#"(?i)^[ \t]*create[ \t]+(?:or[ \t]+replace[ \t]+)?(?:temp(?:orary)?[ \t]+)?table[ \t]+(?:if[ \t]+not[ \t]+exists[ \t]+)?["\[`]?([A-Za-z_][A-Za-z0-9_.]*)"#,
    },
    Rule {
        kind: "view",
        pattern: r#"(?i)^[ \t]*create[ \t]+(?:or[ \t]+replace[ \t]+)?(?:materialized[ \t]+)?view[ \t]+["\[`]?([A-Za-z_][A-Za-z0-9_.]*)"#,
    },
    Rule {
        kind: "index",
        pattern: r#"(?i)^[ \t]*create[ \t]+(?:unique[ \t]+)?index[ \t]+(?:if[ \t]+not[ \t]+exists[ \t]+)?["\[`]?([A-Za-z_][A-Za-z0-9_.]*)"#,
    },
    Rule {
        kind: "fn",
        pattern: r#"(?i)^[ \t]*create[ \t]+(?:or[ \t]+replace[ \t]+)?(?:procedure|function)[ \t]+["\[`]?([A-Za-z_][A-Za-z0-9_.]*)"#,
    },
    Rule {
        kind: "type",
        pattern: r#"(?i)^[ \t]*create[ \t]+(?:type|domain)[ \t]+["\[`]?([A-Za-z_][A-Za-z0-9_.]*)"#,
    },
];

const MARKDOWN_RULES: &[Rule] = &[Rule {
    kind: "heading",
    pattern: r"^(#{1,6})[ \t]+(.+?)[ \t]*#*[ \t]*$",
}];

const CONFIG_RULES: &[Rule] = &[
    Rule {
        kind: "section",
        pattern: r"^[ \t]*\[\[?([^\]\r\n]+)\]\]?",
    },
    Rule {
        kind: "key",
        pattern: r#"^[ \t]*"([A-Za-z_][A-Za-z0-9_.-]*)"[ \t]*:"#,
    },
    Rule {
        kind: "key",
        pattern: r"^([A-Za-z_][A-Za-z0-9_.-]*)[ \t]*[:=](?:[ \t]|$)",
    },
];

const GENERIC_RULES: &[Rule] = &[
    Rule {
        kind: "heading",
        pattern: r"^(#{1,6})[ \t]+(.+?)[ \t]*#*[ \t]*$",
    },
    Rule {
        kind: "section",
        pattern: r"^[ \t]*\[([^\]\r\n]+)\]",
    },
    Rule {
        kind: "key",
        pattern: r"^([A-Za-z_][A-Za-z0-9_.-]*)[ \t]*[:=](?:[ \t]|$)",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_functions_and_types_are_detected_with_brace_bodies() {
        let source = concat!(
            "pub struct Point {\n",
            "    pub x: i32,\n",
            "}\n",
            "\n",
            "impl Point {\n",
            "    pub fn origin() -> Self {\n",
            "        Self { x: 0 }\n",
            "    }\n",
            "}\n",
        );
        let items = extract_structure(source, Path::new("point.rs"));
        let labels: Vec<(&str, &str)> = items
            .iter()
            .map(|item| (item.kind, item.label.as_str()))
            .collect();
        assert!(labels.contains(&("struct", "Point")));
        assert!(labels.contains(&("impl", "Point")));
        assert!(labels.contains(&("fn", "origin")));

        let origin = items
            .iter()
            .find(|item| item.label == "origin")
            .expect("origin");
        assert_eq!(origin.start_line, 6);
        assert_eq!(origin.end_line, 8);
        assert_eq!(origin.depth, 1);
    }

    #[test]
    fn python_bodies_stop_at_dedent() {
        let source = concat!(
            "class Runner:\n",
            "    def start(self):\n",
            "        self.go()\n",
            "        return True\n",
            "\n",
            "def helper():\n",
            "    pass\n",
        );
        let items = extract_structure(source, Path::new("runner.py"));
        let start = items
            .iter()
            .find(|item| item.label == "start")
            .expect("start");
        assert_eq!((start.start_line, start.end_line), (2, 4));
        let helper = items
            .iter()
            .find(|item| item.label == "helper")
            .expect("helper");
        assert_eq!((helper.start_line, helper.end_line), (6, 7));
    }

    #[test]
    fn markdown_headings_end_before_the_next_peer_heading() {
        let source = "# One\ntext\n## Nested\nmore\n# Two\ntail\n";
        let items = extract_structure(source, Path::new("readme.md"));
        let one = items.iter().find(|item| item.label == "One").expect("one");
        assert_eq!((one.start_line, one.end_line), (1, 4));
        assert_eq!(one.depth, 0);
        let nested = items
            .iter()
            .find(|item| item.label == "Nested")
            .expect("nested");
        assert_eq!(nested.depth, 1);
    }

    #[test]
    fn labels_never_contain_the_region_separator() {
        assert_eq!(
            safe_label("user@example.com @ 3-4"),
            "user#example.com # 3-4"
        );
    }
}
