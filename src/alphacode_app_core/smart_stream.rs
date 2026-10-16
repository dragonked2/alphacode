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
///
/// Tuned against both hard cases: log lines that differ only in a counter or
/// timestamp score ~0.87–0.89 and should still collapse, while genuinely
/// distinct output must not. A false positive silently deletes a line of real
/// tool output, so the threshold is high and the tools whose output is a list
/// of answers disable fuzzy dedup outright (see [`StreamFilter::for_tool`]).
const FUZZY_DEDUP_THRESHOLD: f64 = 0.85;

/// Minimum length (in chars) before fuzzy dedup may suppress a line.
///
/// Below this length a line is mostly structure, so the bigram ratio compares
/// shape rather than content and two distinct short lines can sit near the
/// threshold. Only exact matches dedup under this length.
const MIN_FUZZY_LEN: usize = 16;

/// Marker prefixed to a count-bearing header line whose evidence was trimmed.
///
/// A header such as `137 match(es) in 4 file(s)` promises the model a list of
/// matches. When that list is trimmed, the count has to be corrected *on the
/// same line*: an explanation appended at the end of the output is the first
/// thing a later truncation removes, so the count would keep its false promise.
/// Correcting the header makes the mismatch impossible to miss.
const PARTIAL_MARKER: &str = "[partial] ";

/// Wording appended to that header, kept compact for the same reason.
const PARTIAL_NOTE: &str = " — not all lines shown";

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
    /// Whether `dedup` may also collapse *similar* (not just identical) lines.
    ///
    /// Enabled only for log-shaped tools. For tools whose output is a list of
    /// answers (grep hits, file contents, search results) a near-match is a
    /// distinct piece of evidence, so this stays off and only exact duplicates
    /// are removed.
    pub fuzzy_dedup: bool,
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
            "alphagrep" => (200, 40_000),
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
        // Every line an analysis tool emits is a deliberate answer: a grep hit,
        // a source line, a search result. Collapsing two of them because they
        // merely *look* similar deletes evidence and desynchronises the tool's
        // reported count from the body the model receives. Those tools keep
        // exact-duplicate removal only; fuzzy dedup stays for the log-shaped
        // tools where near-identical progress output is genuinely redundant.
        let fuzzy_dedup = !matches!(tool_name, "alphagrep" | "read" | "session_search");
        Self {
            tool_name: tool_name.to_string(),
            max_lines,
            max_chars,
            dedup: true,
            fuzzy_dedup,
            summarize: true,
        }
    }

    /// Filter output through this configuration.
    pub fn filter(&self, raw: &str) -> FilterResult {
        let original_lines = raw.lines().count();
        let original_chars = raw.len();

        if original_chars <= self.max_chars && original_lines <= self.max_lines {
            // Small outputs skip noise filtering and summarization, but
            // blank-line collapsing and dedup still apply so repeated lines
            // and blank runs never reach the context regardless of size.
            let mut kept: Vec<String> = Vec::new();
            let mut seen: Vec<(String, u32)> = Vec::new();
            let mut consecutive_blanks = 0usize;
            let mut removed = 0u32;

            for line in raw.lines() {
                let is_blank = line.trim().is_empty();
                if is_blank {
                    consecutive_blanks += 1;
                    if consecutive_blanks > MAX_CONSECUTIVE_BLANKS {
                        removed += 1;
                        continue;
                    }
                } else {
                    consecutive_blanks = 0;
                }

                if self.dedup && !is_blank {
                    let normalized = normalize_line(line);
                    if !normalized.is_empty() {
                        let mut is_dup = false;
                        for (seen_norm, count) in &mut seen {
                            if seen_norm == &normalized {
                                *count += 1;
                                if *count > 3 {
                                    is_dup = true;
                                }
                                break;
                            }
                        }
                        if !is_dup && self.fuzzy_dedup && seen.len() < 128 {
                            // Errors and warnings are the highest-value line in
                            // any log, and two of them can be near-identical
                            // while both matter (e.g. the same error for two
                            // files). Exact-match dedup above still applies;
                            // fuzzy dedup must never remove one.
                            let protect = is_high_signal(line);
                            for (seen_norm, _) in seen.iter().rev().take(32) {
                                if !protect && fuzzy_similar(&normalized, seen_norm) {
                                    is_dup = true;
                                    break;
                                }
                            }
                        }
                        if is_dup {
                            removed += 1;
                            continue;
                        }
                        seen.push((normalized, 1));
                    }
                }

                kept.push(line.to_string());
            }

            if removed == 0 {
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

            // Exact-duplicate removal and blank collapsing can drop lines even
            // for a small output, so a count-bearing header needs the same
            // correction the large path applies.
            mark_partial_headers(&mut kept);

            let mut filtered = kept.join("\n");
            if raw.ends_with('\n') {
                filtered.push('\n');
            }
            let filtered_lines = filtered.lines().count();
            let filtered_chars = filtered.len();
            return FilterResult {
                filtered,
                original_lines,
                filtered_lines,
                original_chars,
                filtered_chars,
                was_filtered: true,
                summary: None,
            };
        }

        let mut kept: Vec<String> = Vec::new();
        let mut seen: Vec<(String, u32)> = Vec::new();
        let mut skipped_duplicates = 0u32;
        let mut skipped_noise = 0u32;
        let mut consecutive_blanks = 0usize;
        // Set when input lines are discarded for any reason (dedup, noise, or
        // the line cap). Drives [`mark_partial_headers`] below.
        let mut input_lines_dropped = false;

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
                    if !is_dup && self.fuzzy_dedup && seen.len() < 128 {
                        for (seen_norm, _) in seen.iter().rev().take(32) {
                            if fuzzy_similar(&normalized, seen_norm) {
                                is_dup = true;
                                break;
                            }
                        }
                    }
                    if is_dup {
                        skipped_duplicates += 1;
                        input_lines_dropped = true;
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
                input_lines_dropped = true;
                continue;
            }

            kept.push(line.to_string());

            // Check limits. Reaching the cap means the remaining lines are
            // dropped without being seen, which also makes the output partial.
            if kept.len() >= self.max_lines {
                input_lines_dropped = true;
                break;
            }
        }

        if input_lines_dropped {
            mark_partial_headers(&mut kept);
        }

        let filtered: String = kept.join("\n");

        // Truncate to max chars if needed.
        //
        // The char budget is applied *before* the summary is measured, so
        // `filtered_chars` and the reduction percentage describe what the model
        // actually receives. Measuring after appending the summary made every
        // result read as over budget and understated the saving.
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
    NOISE_PATTERNS.iter().any(|pattern| {
        if !trimmed.starts_with(pattern) {
            return false;
        }
        // A pattern made purely of decoration characters (dashes, dots, box
        // drawing) only marks the line as noise when the *whole* line is
        // decoration. Otherwise real output would be deleted: a diff header
        // (`--- a/src/main.rs`), an elision note (`... 42 more`), or a table
        // row (`│ key │ value │`) all begin with characters that also begin a
        // separator line, but each carries content the model needs.
        if pattern.chars().all(is_decoration_char) {
            return trimmed.chars().all(is_decoration_char);
        }
        // Word-like markers ("Loading", "Compiling") are genuinely noise
        // whenever they lead the line.
        true
    })
}

/// Characters that carry no meaning on their own — box drawing, rules, and
/// bullet/arrow decoration used to format progress output.
fn is_decoration_char(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '.' | '='
                | '-'
                | '_'
                | '*'
                | '~'
                | '─'
                | '═'
                | '╔'
                | '╗'
                | '╚'
                | '╝'
                | '│'
                | '└'
                | '┘'
                | '├'
                | '┤'
                | '┌'
                | '┐'
                | '>'
                | '<'
        )
}

/// Mark count-bearing header lines whose promised evidence was trimmed.
///
/// Tool output commonly opens with a summary such as `137 match(es) in 4
/// file(s)`. That count is a promise about the lines beneath it, and everything
/// downstream — including the model — reads it as ground truth. When this
/// filter then removes lines, the promise silently becomes false: the model
/// sees "137 matches" above a shorter list and concludes some are missing.
///
/// The marker is placed on the header line itself rather than appended to the
/// end of the output, because a trailing note is the first thing any later
/// truncation drops.
fn mark_partial_headers(lines: &mut [String]) {
    for line in lines.iter_mut() {
        if line.starts_with(PARTIAL_MARKER) {
            continue;
        }
        if is_count_header(line.trim_start()) {
            *line = format!("{PARTIAL_MARKER}{line}{PARTIAL_NOTE}");
        }
    }
}

/// Whether `s` is a summary line that promises a set of lines.
///
/// Deliberately narrow. It accepts two shapes only:
///
/// * `<count> <unit>...` — `137 match(es) in 4 file(s)`
/// * `<verb> ... <unit>` — `Found 12 results in 3 files`
///
/// The unit token requirement is what keeps real content out. A grep hit such
/// as `  137: match text` also begins with a digit and contains "match", but
/// its leading token is `137:` — not a pure count — so it is not a promise and
/// is left untouched. Likewise a source line `let matches = 3;` is not, because
/// "let" is not a counting verb.
fn is_count_header(s: &str) -> bool {
    /// Nouns that name the thing being counted.
    const UNITS: &[&str] = &[
        "match",
        "matches",
        "file",
        "files",
        "result",
        "results",
        "hit",
        "hits",
        "occurrence",
        "occurrences",
    ];

    /// Leading alphabetic part of a token: `match(es)` → `match`.
    fn unit_of(token: &str) -> String {
        token
            .chars()
            .take_while(|c| c.is_ascii_alphabetic())
            .collect::<String>()
            .to_lowercase()
    }

    let mut tokens = s.split_whitespace();
    let Some(first) = tokens.next() else {
        return false;
    };

    // Shape 1: `<count> <unit>...`
    if !first.is_empty() && first.bytes().all(|b| b.is_ascii_digit()) {
        let Some(unit_token) = tokens.next() else {
            return false;
        };
        return UNITS.contains(&unit_of(unit_token).as_str());
    }

    // Shape 2: `<verb> ... <unit>`
    if !matches!(first.to_lowercase().as_str(), "found" | "showing" | "total") {
        return false;
    }
    tokens.any(|token| UNITS.contains(&unit_of(token).as_str()))
}

/// Normalize a line for deduplication (collapse whitespace, lowercase).
///
/// Folds ASCII space and lowercase in one pass. The previous
/// `split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()` made two
/// heap allocations and two full copies per line, and this runs once per line
/// of every tool output that reaches the filter.
fn normalize_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut pending_space = false;
    for ch in line.chars() {
        if ch.is_whitespace() {
            // Only emit a separator once the next real character arrives, so
            // leading and trailing whitespace does not survive.
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        // Non-ASCII is left alone: `to_lowercase` can change a char's byte
        // length, which would corrupt the bigram window over UTF-8 text.
        out.extend(ch.to_lowercase());
    }
    out
}

/// Quick cosine-like similarity between two normalized lines.
/// Uses character bigram overlap — fast enough for hot-path dedup and
/// accurate enough to catch lines that differ only in a variable or count.
fn fuzzy_similar(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    // Short lines carry too few bigrams for the ratio to be meaningful: two
    // unrelated identifiers of the same shape score far above any safe
    // threshold. Only exact matches dedup below this length.
    if a.len() < MIN_FUZZY_LEN || b.len() < MIN_FUZZY_LEN {
        return false;
    }
    if !length_compatible(a, b) {
        return false;
    }
    // Character-bigram Jaccard similarity over the *exact* sets of bigrams.
    //
    // The previous implementation hashed bigrams into 64 buckets, so unrelated
    // bigrams collided and inflated the score — `pub mod x;` and `pub mod y;`
    // measured ~0.85 and one of two genuinely different grep results was
    // silently dropped. Comparing the real sets removes that class of
    // false positive entirely.
    let a_bigrams = char_bigrams(a);
    let b_bigrams = char_bigrams(b);
    let shared = bigram_intersection(&a_bigrams, &b_bigrams);
    // Both sets are deduplicated, so |A ∪ B| = |A| + |B| − |A ∩ B|.
    let union = a_bigrams.len() + b_bigrams.len() - shared;
    if union == 0 {
        return false;
    }
    shared as f64 / union as f64 >= FUZZY_DEDUP_THRESHOLD
}

/// The set of character bigrams in a string.
///
/// Stored as a sorted, deduplicated vector rather than a hash set: tool-output
/// lines are short (tens of chars), and a sorted `Vec` makes the set
/// intersection/union allocation-free on the dedup hot path.
fn char_bigrams(s: &str) -> Vec<(u8, u8)> {
    let bytes = s.as_bytes();
    let mut bigrams: Vec<(u8, u8)> = Vec::with_capacity(bytes.len().saturating_sub(1));
    for window in bytes.windows(2) {
        bigrams.push((window[0], window[1]));
    }
    bigrams.sort_unstable();
    bigrams.dedup();
    bigrams
}

/// Count of bigrams shared by two sorted, deduplicated bigram sets.
/// Both inputs must come from [`char_bigrams`].
fn bigram_intersection(a: &[(u8, u8)], b: &[(u8, u8)]) -> usize {
    let (mut i, mut j, mut shared) = (0usize, 0usize, 0usize);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Equal => {
                shared += 1;
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => i += 1,
            std::cmp::Ordering::Greater => j += 1,
        }
    }
    shared
}

/// Cheap length gate before the bigram work.
///
/// Two lines whose normalized lengths differ by more than a quarter cannot
/// reach the similarity threshold, so the 64-bucket comparison is skipped for
/// them. This is a pure speed filter — it never changes which lines are
/// considered duplicates, only how long it takes to find out.
fn length_compatible(a: &str, b: &str) -> bool {
    let (long, short) = if a.len() >= b.len() {
        (a.len(), b.len())
    } else {
        (b.len(), a.len())
    };
    long * 4 <= short * 5
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
        let lines = [
            "Loading...".to_string(),
            "═══════════════════".to_string(),
            "Real content here".to_string(),
            "│ table border".to_string(),
            "└─ last item".to_string(),
        ];
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

    /// The single-pass rewrite of `normalize_line` must be indistinguishable
    /// from the `split_whitespace().join(" ")` it replaced — a subtly different
    /// normalizer would silently change which lines count as duplicates.
    #[test]
    fn normalize_line_matches_the_previous_implementation() {
        let cases = [
            "Hello World",
            "  leading and trailing   ",
            "tabs\tand\nnewlines\r\nhere",
            "multiple    inner     spaces",
            "",
            "   ",
            "MiXeD CaSe 123",
            "trailing only    ",
            "    leading only",
            "unicode ünïcödé  ok",
        ];
        for case in cases {
            let expected = case
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
            assert_eq!(normalize_line(case), expected, "case: {case:?}");
        }
    }

    /// The length gate must not reject anything the bigram comparison would
    /// have accepted, or near-duplicate log lines would survive deduplication.
    #[test]
    fn length_gate_never_rejects_a_true_duplicate() {
        // Worst case for the gate: the two lines sit exactly at the threshold.
        for base in [
            "connection to host closed",
            "resolved address for example com",
            "retrying the request after backoff",
        ] {
            for delta in -2i32..=2 {
                let a = base.to_string();
                let b = format!("{base}{}", "x".repeat(delta.max(0) as usize));
                if fuzzy_similar(&a, &b) {
                    assert!(
                        length_compatible(&a, &b),
                        "gate rejected a true match: {a:?} / {b:?}"
                    );
                }
            }
        }
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
        let blank_count = result
            .filtered
            .lines()
            .filter(|l| l.trim().is_empty())
            .count();
        assert!(
            blank_count <= 2,
            "expected at most 2 consecutive blanks, got {} in: {}",
            blank_count,
            result.filtered
        );
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
    fn char_bigrams_basic() {
        let bigrams = char_bigrams("hello");
        // "he", "el", "ll", "lo" — deduplicated and sorted.
        assert_eq!(
            bigrams,
            vec![(b'e', b'l'), (b'h', b'e'), (b'l', b'l'), (b'l', b'o'),]
        );
        // Repeated bigrams collapse to one entry.
        assert_eq!(char_bigrams("aaaa").len(), 1);
        assert!(char_bigrams("").is_empty());
        assert!(char_bigrams("x").is_empty());
    }

    /// Fuzzy dedup must never fire on the tools whose output is a list of
    /// answers. Two grep hits that differ only in a path or identifier are
    /// two distinct pieces of evidence, and dropping one made the tool's
    /// reported match count disagree with the body the model received.
    #[test]
    fn analysis_tools_do_not_fuzzy_dedup_distinct_lines() {
        assert!(!StreamFilter::for_tool("alphagrep").fuzzy_dedup);
        assert!(!StreamFilter::for_tool("read").fuzzy_dedup);
        assert!(!StreamFilter::for_tool("session_search").fuzzy_dedup);
        // Log-shaped tools keep it.
        assert!(StreamFilter::for_tool("bash").fuzzy_dedup);

        // The exact shape that used to lose a line: two paths differing by
        // one character, both real grep hits.
        let output = "src/a.rs:12:    foo();\nsrc/b.rs:12:    foo();\n";
        let result = filter_output("alphagrep", output);
        assert!(
            result.filtered.contains("src/a.rs:12:    foo();"),
            "first hit dropped: {}",
            result.filtered
        );
        assert!(
            result.filtered.contains("src/b.rs:12:    foo();"),
            "second hit dropped: {}",
            result.filtered
        );
    }

    /// Even for log tools, near-identical *error* lines are all kept — two
    /// incidents that read the same are still two incidents.
    #[test]
    fn fuzzy_dedup_never_drops_high_signal_lines() {
        let output = "\
error[E0428]: the name `Rule` is defined multiple times
error[E0428]: the name `Rule2` is defined multiple times
";
        let result = filter_output("bash", output);
        assert!(
            result.filtered.contains("`Rule` "),
            "first error dropped: {}",
            result.filtered
        );
        assert!(
            result.filtered.contains("`Rule2`"),
            "second error dropped: {}",
            result.filtered
        );
    }

    /// Short distinct lines must never be treated as duplicates of each other.
    #[test]
    fn fuzzy_similar_rejects_distinct_short_lines() {
        assert!(!fuzzy_similar("pub mod x;", "pub mod y;"));
        assert!(!fuzzy_similar("let a = 1;", "let b = 2;"));
        // Identical short lines are still duplicates (exact path).
        assert!(fuzzy_similar("pub mod x;", "pub mod x;"));
    }

    /// Decoration-only noise patterns must not swallow real output that merely
    /// starts with the same characters. Every "kept" case below was verified to
    /// be deleted by the previous `starts_with`-only implementation.
    #[test]
    fn noise_matching_does_not_eat_real_lines() {
        // Pure decoration is still noise.
        assert!(is_noise("──────────────"));
        assert!(is_noise("..."));
        assert!(is_noise("===="));
        assert!(is_noise("Loading config"));

        // Real content that begins with decoration characters must survive:
        // a unified-diff header, an elision note, a boxed table row, and a
        // banner that carries a word.
        assert!(!is_noise("---- a/src/main.rs"));
        assert!(!is_noise("... 42 more"));
        assert!(!is_noise("│ key │ value │"));
        assert!(!is_noise("════ done ════"));
    }

    /// A count-bearing header must be corrected when lines are removed, so the
    /// model never sees a promise ("137 matches") above a shorter list without
    /// a visible explanation. This is the "reported N but saw fewer" failure.
    #[test]
    fn count_header_is_marked_when_evidence_is_trimmed() {
        let output = "137 match(es) in 4 file(s) for \"foo\"\n\
                      same\nsame\nsame\nsame\nsame\n";
        let result = filter_output("bash", output);
        assert!(
            result.filtered.starts_with("[partial] 137 match(es)"),
            "header was not corrected: {}",
            result.filtered
        );
        assert!(result.filtered.contains("not all lines shown"));
    }

    /// When nothing is dropped the header keeps its original, accurate form.
    #[test]
    fn count_header_untouched_when_nothing_is_trimmed() {
        let output = "137 match(es) in 4 file(s) for \"foo\"\n\
                      src/a.rs:1: foo\nsrc/b.rs:2: foo\n";
        let result = filter_output("bash", output);
        assert!(
            !result.filtered.contains("[partial]"),
            "header wrongly marked: {}",
            result.filtered
        );
        assert!(result.filtered.contains("137 match(es) in 4 file(s)"));
    }

    /// The header predicate must not mistake real content for a count. A grep
    /// hit like `  137: match text` also starts with a digit and contains
    /// "match", and marking it would corrupt the very evidence being protected.
    #[test]
    fn count_header_detection_ignores_real_content() {
        // Genuine summaries.
        assert!(is_count_header("137 match(es) in 4 file(s)"));
        assert!(is_count_header("Found 12 results in 3 files"));
        assert!(is_count_header("5 hits"));
        assert!(is_count_header("showing 1 of 5 matches"));
        assert!(is_count_header("12 files"));

        // Real content that must not be treated as a promise.
        assert!(!is_count_header("137: match text"));
        assert!(!is_count_header("let matches = 3;"));
        assert!(!is_count_header("Suite result: ok. 21 passed"));
        assert!(!is_count_header("1234567890"));
        assert!(!is_count_header(""));
    }
}
