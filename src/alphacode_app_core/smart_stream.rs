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



// ── Configuration ─────────────────────────────────────────────────────────

/// Maximum lines to keep before summarizing.
const MAX_LINES_BEFORE_SUMMARY: usize = 200;

/// Maximum characters for the filtered output.
const MAX_CHARS: usize = 50_000;

/// Lines shorter than this are considered "noise" (blank lines, separators).
const MIN_SIGNAL_LINE_LEN: usize = 3;

/// Maximum consecutive blank lines to keep (excess blanks waste tokens).
const MAX_CONSECUTIVE_BLANKS: usize = 2;

/// Similarity threshold for fuzzy deduplication (0.0 = no match, 1.0 = identical).
/// Lines with normalized Levenshtein distance below this are considered duplicates.
const FUZZY_DEDUP_THRESHOLD: f64 = 0.85;

// ── High-signal patterns ──────────────────────────────────────────────────

/// Patterns that indicate high-signal content (errors, warnings, results).
const HIGH_SIGNAL_PATTERNS: &[&str] = &[
    // Errors / failures
    "error",
    "Error",
    "ERROR",
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
    // Warnings
    "warning",
    "Warning",
    "WARNING",
    "deprecated",
    "Deprecated",
    // Security
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
    // Results / findings
    "found",
    "Found",
    "FOUND",
    "result",
    "Result",
    "RESULT",
    "success",
    "Success",
    "SUCCESS",
    "passing",
    "failing",
    // HTTP status codes
    "http://",
    "https://",
    "200 OK",
    "201 Created",
    "301",
    "302",
    "400",
    "401",
    "403",
    "404",
    "429",
    "500",
    "502",
    "503",
    // Build/test signals
    "test result:",
    "FAILED",
    "PASSED",
    "compilation error",
    "cargo error",
    "npm error",
];

// ── Noise patterns ────────────────────────────────────────────────────────

/// Patterns that indicate low-signal content (progress, debug, decoration).
const NOISE_PATTERNS: &[&str] = &[
    "Loading",
    "Fetching",
    "Connecting",
    "Downloading",
    "Uploading",
    "Installing",
    "Compiling",
    "Building",
    "Processing",
    "Waiting",
    "Retrying",
    "Reconnecting",
    "...",
    "───",
    "═══",
    "╔══",
    "╚══",
    "║",
    "│",
    "└─",
    "├─",
    "┌─",
    "─┤",
    "├──",
    "=>>",
    "<<=",
    "****",
    "====",
    "----",
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
        let (max_lines, max_chars) = match tool_name {
            // Bash: build logs are extremely noisy; cap aggressively.
            "bash" => (150, 40_000),
            // Web content: already truncated by webfetch, keep moderate.
            "webfetch" => (100, 30_000),
            // Search results: small per-result snippets compound fast.
            "websearch" => (50, 15_000),
            // Grep output can be very large; keep generous line count but
            // moderate char limit since matched lines tend to be long.
            "agentgrep" => (200, 40_000),
            // Read tool: code content is high-value, keep generous.
            "read" => (300, 50_000),
            // Session search: search snippets are compact but numerous.
            "session_search" => (100, 20_000),
            // Browser: HTML pages are large and redundant.
            "browser" => (80, 25_000),
            // Batch: inner tool outputs are already filtered individually.
            "batch" => (MAX_LINES_BEFORE_SUMMARY, MAX_CHARS),
            _ => (MAX_LINES_BEFORE_SUMMARY, MAX_CHARS),
        };
        Self {
            tool_name: tool_name.to_string(),
            max_lines,
            max_chars,
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
        let mut seen: Vec<(String, u32)> = Vec::new();
        let mut skipped_duplicates = 0u32;
        let mut skipped_noise = 0u32;
        let mut consecutive_blanks = 0usize;

        for line in raw.lines() {
            // Collapse consecutive blank lines to save tokens.
            let is_blank = line.trim().is_empty();
            if is_blank {
                consecutive_blanks += 1;
                if consecutive_blanks > MAX_CONSECUTIVE_BLANKS {
                    continue;
                }
            } else {
                consecutive_blanks = 0;
            }

            // Dedup check (exact + fuzzy)
            if self.dedup && !is_blank {
                let normalized = normalize_line(line);
                if !normalized.is_empty() {
                    let mut is_dup = false;
                    // Check exact match first (fast path)
                    for (seen_norm, count) in &mut seen {
                        if seen_norm == &normalized {
                            *count += 1;
                            if *count > 3 {
                                is_dup = true;
                            }
                            break;
                        }
                    }
                    // Fuzzy dedup: check if this line is similar to a recent line
                    if !is_dup && seen.len() < 128 {
                        for (seen_norm, _) in seen.iter().rev().take(32) {
                            if fuzzy_similar(&normalized, seen_norm) {
                                is_dup = true;
                                break;
                            }
                        }
                    }
                    if is_dup {
                        skipped_duplicates += 1;
                        continue;
                    }
                    seen.push((normalized, 1));
                }
            }

            // Keep high-signal lines
            if is_high_signal(line) {
                kept.push(line.to_string());
                continue;
            }

            // Skip pure noise
            if line.len() < MIN_SIGNAL_LINE_LEN || is_noise(line) {
                skipped_noise += 1;
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
            let mut cut = self.max_chars.saturating_sub(150);
            while cut > 0 && !filtered.is_char_boundary(cut) {
                cut -= 1;
            }
            // Try to cut at a newline for cleaner truncation
            let cut = match filtered[..cut].rfind('\n') {
                Some(nl) if nl > cut / 2 => nl,
                _ => cut,
            };
            let tokens_saved = (original_chars - cut) / 4;
            format!(
                "{}\n\n[Output truncated: saved ~{}k tokens — {} → {} chars ({}% reduction)]",
                &filtered[..cut],
                tokens_saved / 1000,
                original_chars,
                cut,
                ((original_chars - cut) as f64 / original_chars.max(1) as f64 * 100.0) as u32,
            )
        } else {
            filtered
        };

        let filtered_lines = filtered.lines().count();
        let filtered_chars = filtered.len();

        let summary = Some(format!(
            "Filtered: {} → {} lines ({}%), {} → {} chars ({}%). Deduped: {}, Noise: {}.",
            original_lines,
            filtered_lines,
            (filtered_lines as f64 / original_lines.max(1) as f64 * 100.0) as u32,
            original_chars,
            filtered_chars,
            (filtered_chars as f64 / original_chars.max(1) as f64 * 100.0) as u32,
            skipped_duplicates,
            skipped_noise,
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
    NOISE_PATTERNS
        .iter()
        .any(|pattern| trimmed.starts_with(pattern))
}

/// Normalize a line for deduplication (collapse whitespace, lowercase).
fn normalize_line(line: &str) -> String {
    line.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Quick cosine-like similarity between two normalized lines.
/// Uses character bigram overlap — fast enough for hot-path dedup and
/// accurate enough to catch lines that differ only in a variable or count.
fn fuzzy_similar(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    // Short lines are compared by prefix: if the first 80% of chars match,
    // treat as similar (catches e.g. "line 42 content" vs "line 43 content").
    let min_len = a.len().min(b.len());
    if min_len < 10 {
        return false;
    }
    let prefix_len = (min_len * 4) / 5;
    let shared_prefix = a[..prefix_len.min(a.len())] == b[..prefix_len.min(b.len())];
    if !shared_prefix {
        return false;
    }
    // For longer lines, compute character bigram overlap ratio.
    let a_bigrams = char_bigram_count(a);
    let b_bigrams = char_bigram_count(b);
    // Bigram arrays are , so  yields . Dereference
    // each side so  (which takes  by value) is callable.
    let intersection: u32 = a_bigrams
        .iter()
        .zip(b_bigrams.iter())
        .map(|(x, y)| (*x).min(*y))
        .sum();
    let union: u32 = a_bigrams
        .iter()
        .zip(b_bigrams.iter())
        .map(|(x, y)| (*x).max(*y))
        .sum();
    if union == 0 {
        return false;
    }
    let similarity = intersection as f64 / union as f64;
    similarity >= FUZZY_DEDUP_THRESHOLD
}

/// Count character bigrams in a string (space-optimized: 64 buckets).
fn char_bigram_count(s: &str) -> [u32; 64] {
    let mut counts = [0u32; 64];
    let bytes: Vec<u8> = s.bytes().collect();
    for window in bytes.windows(2) {
        let idx = ((window[0] as usize) ^ (window[1] as usize)) & 63;
        counts[idx] += 1;
    }
    counts
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
        let output: String = (0..500)
            .map(|i| format!("Line {} some content here\n", i))
            .collect();
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

    #[test]
    fn fuzzy_similar_catches_near_duplicates() {
        assert!(fuzzy_similar(
            "line 42 content here repeated stuff",
            "line 43 content here repeated stuff"
        ));
        assert!(fuzzy_similar("same text exactly", "same text exactly"));
        // Short lines should not match
        assert!(!fuzzy_similar("short", "shoet"));
        // Completely different lines should not match
        assert!(!fuzzy_similar(
            "completely different content here",
            "nothing alike at all here either"
        ));
    }

    #[test]
    fn fuzzy_similar_catches_changed_numbers() {
        // Common pattern: repeated log lines with only a counter or timestamp changed
        assert!(fuzzy_similar(
            "2024-01-15 10:30:00 INFO Processing item 1234",
            "2024-01-15 10:30:01 INFO Processing item 1235"
        ));
    }

    #[test]
    fn consecutive_blank_lines_collapsed() {
        let output = "line1\n\n\n\n\nline2";
        let result = filter_output("bash", output);
        // Should collapse 5 blank lines to at most 2
        let blank_count = result.filtered.lines().filter(|l| l.trim().is_empty()).count();
        assert!(blank_count <= 2, "expected at most 2 consecutive blanks, got {} in: {}", blank_count, result.filtered);
    }

    #[test]
    fn tool_specific_char_limits() {
        let websearch = StreamFilter::for_tool("websearch");
        assert_eq!(websearch.max_chars, 15_000);
        let webfetch = StreamFilter::for_tool("webfetch");
        assert_eq!(webfetch.max_chars, 30_000);
        let bash = StreamFilter::for_tool("bash");
        assert_eq!(bash.max_chars, 40_000);
    }

    #[test]
    fn char_bigram_count_basic() {
        let counts = char_bigram_count("hello");
        assert!(counts.iter().any(|&c| c > 0));
        let empty = char_bigram_count("");
        assert!(empty.iter().all(|&c| c == 0));
    }
}
