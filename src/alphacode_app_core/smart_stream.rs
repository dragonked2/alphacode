//! Smart Stream — intelligent output filtering for tool outputs.
//!
//! Prevents context window bloat by filtering high-volume tool outputs to
//! only high-signal information. This is the same problem that tools like
//! `smart_pipe` solve: raw command output (fuzzer results, search results,
//! build logs) can flood the LLM context with thousands of lines of noise.
//!
//! The filter works by:
//! 1. Analyzing output structure (headers, error patterns, repeated lines)
//! 2. Deduplicating repeated/similar lines
//! 3. Preserving error lines and high-signal markers
//! 4. Summarizing large outputs with statistics
//! 5. Applying per-tool heuristic rules

use std::collections::HashMap;

// ── Configuration ─────────────────────────────────────────────────────────

/// Maximum lines to keep before summarizing.
const MAX_LINES_BEFORE_SUMMARY: usize = 200;

/// Maximum characters for the filtered output.
const MAX_CHARS: usize = 50_000;

/// Lines shorter than this are considered "noise" (blank lines, separators).
const MIN_SIGNAL_LINE_LEN: usize = 3;

// ── High-signal patterns ──────────────────────────────────────────────────

/// Patterns that indicate high-signal content (errors, warnings, results).
const HIGH_SIGNAL_PATTERNS: &[&str] = &[
    "error",
    "Error",
    "ERROR",
    "warning",
    "Warning",
    "WARNING",
    "fatal",
    "FATAL",
    "panic",
    "Panic",
    "failed",
    "Failed",
    "FAILED",
    "denied",
    "Denied",
    "DENIED",
    "critical",
    "Critical",
    "CRITICAL",
    "vulnerability",
    "Vulnerability",
    "CVE",
    "exploit",
    "Exploit",
    "leaked",
    "Leaked",
    "LEAKED",
    "injection",
    "Injection",
    "overflow",
    "Overflow",
    "bypass",
    "Bypass",
    "found",
    "Found",
    "FOUND",
    "result",
    "Result",
    "RESULT",
    "success",
    "Success",
    "SUCCESS",
    "http://",
    "https://",
    "200 OK",
    "201 Created",
    "301",
    "302",
    "403",
    "404",
    "500",
];

// ── Noise patterns ────────────────────────────────────────────────────────

/// Patterns that indicate low-signal content (progress, debug).
const NOISE_PATTERNS: &[&str] = &[
    "Loading",
    "Fetching",
    "Connecting",
    "Downloading",
    "...",
    "───",
    "═══",
    "╔══",
    "╚══",
    "│",
    "└─",
    "├─",
];

// ── Filter Configuration ──────────────────────────────────────────────────

/// Per-tool filter configuration.
#[derive(Debug, Clone)]
pub struct StreamFilter {
    /// Tool name for which this filter applies.
    pub tool_name: String,
    /// Maximum lines to keep.
    pub max_lines: usize,
    /// Maximum characters.
    pub max_chars: usize,
    /// Whether to deduplicate repeated lines.
    pub dedup: bool,
    /// Whether to summarize large outputs.
    pub summarize: bool,
}

impl StreamFilter {
    /// Create a filter with defaults for a given tool.
    pub fn for_tool(tool_name: &str) -> Self {
        let max_lines = match tool_name {
            "bash" => 150,
            "webfetch" => 100,
            "websearch" => 50,
            "agentgrep" => 200,
            "read" => 300,
            _ => MAX_LINES_BEFORE_SUMMARY,
        };
        Self {
            tool_name: tool_name.to_string(),
            max_lines,
            max_chars: MAX_CHARS,
            dedup: true,
            summarize: true,
        }
    }

    /// Filter output through this configuration.
    pub fn filter(&self, raw: &str) -> FilterResult {
        let original_lines = raw.lines().count();
        let original_chars = raw.len();

        if original_chars <= self.max_chars && original_lines <= self.max_lines {
            return FilterResult {
                filtered: raw.to_string(),
                original_lines,
                filtered_lines: original_lines,
                original_chars,
                filtered_chars: original_chars,
                was_filtered: false,
                summary: None,
            };
        }

        let mut kept: Vec<String> = Vec::new();
        let mut seen: HashMap<String, u32> = HashMap::new();
        let mut skipped_duplicates = 0u32;

        for line in raw.lines() {
            // Dedup check
            if self.dedup {
                let normalized = normalize_line(line);
                if !normalized.is_empty() {
                    let count = seen.entry(normalized).or_insert(0);
                    *count += 1;
                    if *count > 3 {
                        // After 3 occurrences, skip duplicates
                        skipped_duplicates += 1;
                        continue;
                    }
                }
            }

            // Keep high-signal lines
            if is_high_signal(line) {
                kept.push(line.to_string());
                continue;
            }

            // Skip pure noise
            if line.len() < MIN_SIGNAL_LINE_LEN || is_noise(line) {
                continue;
            }

            kept.push(line.to_string());

            // Check limits
            if kept.len() >= self.max_lines {
                break;
            }
        }

        let filtered: String = kept.join("\n");

        // Truncate to max chars if needed
        let filtered = if filtered.len() > self.max_chars {
            let truncated: String = filtered.chars().take(self.max_chars - 100).collect();
            format!("{}\n\n[Truncated at {} chars]", truncated, self.max_chars)
        } else {
            filtered
        };

        let filtered_lines = filtered.lines().count();
        let filtered_chars = filtered.len();

        let summary = Some(format!(
            "Filtered: {} → {} lines ({}%), {} → {} chars ({}%). Skipped {} duplicate lines.",
            original_lines,
            filtered_lines,
            (filtered_lines as f64 / original_lines.max(1) as f64 * 100.0) as u32,
            original_chars,
            filtered_chars,
            (filtered_chars as f64 / original_chars.max(1) as f64 * 100.0) as u32,
            skipped_duplicates,
        ));

        FilterResult {
            filtered,
            original_lines,
            filtered_lines,
            original_chars,
            filtered_chars,
            was_filtered: true,
            summary,
        }
    }
}

// ── Filter Result ─────────────────────────────────────────────────────────

/// Result of filtering tool output.
#[derive(Debug, Clone)]
pub struct FilterResult {
    /// The filtered output.
    pub filtered: String,
    /// Number of lines in the original output.
    pub original_lines: usize,
    /// Number of lines in the filtered output.
    pub filtered_lines: usize,
    /// Number of characters in the original output.
    pub original_chars: usize,
    /// Number of characters in the filtered output.
    pub filtered_chars: usize,
    /// Whether any filtering was applied.
    pub was_filtered: bool,
    /// Optional summary of what was filtered.
    pub summary: Option<String>,
}

impl FilterResult {
    /// Get the filtered output, or the original if no filtering was needed.
    pub fn output(&self) -> &str {
        &self.filtered
    }

    /// Check if filtering reduced the output significantly (> 50%).
    pub fn significant_reduction(&self) -> bool {
        self.filtered_chars < self.original_chars / 2
    }
}

// ── Helper Functions ──────────────────────────────────────────────────────

/// Check if a line contains high-signal patterns.
fn is_high_signal(line: &str) -> bool {
    let lower = line.to_lowercase();
    HIGH_SIGNAL_PATTERNS.iter().any(|pattern| {
        let plower = pattern.to_lowercase();
        lower.contains(&plower)
    })
}

/// Check if a line is noise (progress indicators, separators, etc.).
fn is_noise(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return true;
    }
    NOISE_PATTERNS.iter().any(|pattern| trimmed.starts_with(pattern))
}

/// Normalize a line for deduplication (collapse whitespace, lowercase).
fn normalize_line(line: &str) -> String {
    line.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

// ── Public API ────────────────────────────────────────────────────────────

/// Filter tool output with default settings for the given tool name.
pub fn filter_output(tool_name: &str, raw: &str) -> FilterResult {
    StreamFilter::for_tool(tool_name).filter(raw)
}

/// Quick check if output needs filtering.
pub fn needs_filtering(tool_name: &str, raw: &str) -> bool {
    let filter = StreamFilter::for_tool(tool_name);
    raw.len() > filter.max_chars || raw.lines().count() > filter.max_lines
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_output_not_filtered() {
        let output = "Hello\nWorld\n";
        let result = filter_output("bash", output);
        assert!(!result.was_filtered);
        assert_eq!(result.filtered, output);
    }

    #[test]
    fn large_output_filtered() {
        let output: String = (0..500).map(|i| format!("Line {} some content here\n", i)).collect();
        let result = filter_output("bash", &output);
        assert!(result.was_filtered);
        assert!(result.filtered_lines < 500);
        assert!(result.summary.is_some());
    }

    #[test]
    fn high_signal_lines_preserved() {
        let mut lines: Vec<String> = (0..300).map(|i| format!("Noise line {}", i)).collect();
        lines.insert(50, "Error: something failed".to_string());
        lines.insert(100, "CRITICAL: vulnerability found".to_string());
        let output = lines.join("\n");

        let result = filter_output("bash", &output);
        assert!(result.was_filtered);
        assert!(result.filtered.contains("Error: something failed"));
        assert!(result.filtered.contains("CRITICAL: vulnerability found"));
    }

    #[test]
    fn dedup_removes_repeated_lines() {
        let mut lines = Vec::new();
        for _ in 0..10 {
            lines.push("Same line repeated".to_string());
        }
        lines.push("Unique line".to_string());
        let output = lines.join("\n");

        let result = filter_output("bash", &output);
        assert!(result.was_filtered);
        // Should have deduped the repeated line
        assert!(result.filtered.contains("Unique line"));
    }

    #[test]
    fn noise_lines_filtered() {
        let mut lines = Vec::new();
        lines.push("Loading...".to_string());
        lines.push("═══════════════════".to_string());
        lines.push("Real content here".to_string());
        lines.push("│ table border".to_string());
        lines.push("└─ last item".to_string());
        let output = lines.join("\n");

        let result = filter_output("bash", &output);
        assert!(result.filtered.contains("Real content here"));
    }

    #[test]
    fn significant_reduction_detection() {
        let output: String = (0..1000).map(|i| format!("Line {}\n", i)).collect();
        let result = filter_output("bash", &output);
        assert!(result.significant_reduction());
    }

    #[test]
    fn needs_filtering_works() {
        assert!(!needs_filtering("bash", "short output"));
        let long: String = (0..500).map(|i| format!("Line {}\n", i)).collect();
        assert!(needs_filtering("bash", &long));
    }

    #[test]
    fn tool_specific_limits() {
        let websearch_filter = StreamFilter::for_tool("websearch");
        assert_eq!(websearch_filter.max_lines, 50);

        let read_filter = StreamFilter::for_tool("read");
        assert_eq!(read_filter.max_lines, 300);
    }

    #[test]
    fn empty_output() {
        let result = filter_output("bash", "");
        assert!(!result.was_filtered);
        assert_eq!(result.filtered_lines, 0);
    }

    #[test]
    fn normalize_line_collapses_whitespace() {
        assert_eq!(normalize_line("  hello   world  "), "hello world");
        assert_eq!(normalize_line("FOO bar"), "foo bar");
    }
}
