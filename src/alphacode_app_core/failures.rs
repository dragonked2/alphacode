//! Tool failure analysis: helps the model debug its own failures.
//!
//! When a tool returns an error, the model usually retries with the same
//! arguments a few times before trying something new.  Three of those retries
//! is the modal pattern in our traces, and most of them fail for reasons the
//! model could have anticipated from the first error message: a missing
//! directory, an unhandled non-zero exit, a permissions failure on a write.
//!
//! This module classifies tool errors into a small set of root causes and
//! builds a one-line hint that the agent can prepend to the next turn, so
//! the model spends its tokens on a different attempt rather than another
//! identical attempt.  The hint is intentionally short (<200 chars) and is
//! omitted when the cause is already obvious or when retries have already
//! been attempted for the same root cause this turn.
//!
//! ## Categories
//!
//! - `MissingPath`: a path the model referenced does not exist.  Hint:
//!   "the path was not found; run `ls` on the parent to discover the right
//!   one".
//! - `PermissionDenied`: a permissions failure on a write / exec.  Hint:
//!   "this is a permissions error; check the file mode or sudo".
//! - `Timeout`: an operation exceeded its wall-clock budget.  Hint:
//!   "this command timed out; consider splitting it or raising the deadline".
//! - `SyntaxError`: shell / JSON / regex parse failure.  Hint: usually the
//!   actual error message is the only useful signal, so we forward it.
//! - `NetworkError`: connection refused / DNS / TLS.  Hint: "the network
//!   dropped; retry in a moment".
//! - `OutOfDisk` / `OutOfMemory`: rare but distinct.  Hint: "host is out of
//!   X; free some or shrink the request".
//! - `Unknown`: any other error.  The raw message is forwarded verbatim.
//!
//! All categories are derived from string-matching the error text.  This is
//! deliberately conservative — we never want to misclassify a recoverable
//! error as fatal — and the `Confidence::Low` variants emit no hint so the
//! model can decide on its own.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

/// The classification of a tool error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FailureKind {
    MissingPath,
    PermissionDenied,
    Timeout,
    SyntaxError,
    NetworkError,
    OutOfDisk,
    OutOfMemory,
    AlreadyExists,
    InvalidArgument,
    NotImplemented,
    Cancelled,
    Unknown,
}

/// How confident we are in the classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Confidence {
    /// The category is well-supported by markers in the error text.
    High,
    /// Best guess; caller should still show the model the raw message.
    Low,
}

/// One classified error and the hint we'll surface.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureAnalysis {
    pub kind: FailureKind,
    pub confidence: Confidence,
    pub hint: String,
    /// Truncated original error so the model can read it without bloating
    /// the next request.
    pub excerpt: String,
}

/// Counters so the model can see "you've hit this N times already this turn".
#[derive(Default)]
struct Counters {
    per_kind: HashMap<FailureKind, u32>,
}

static COUNTERS: OnceLock<Mutex<Counters>> = OnceLock::new();

fn counters() -> &'static Mutex<Counters> {
    COUNTERS.get_or_init(|| Mutex::new(Counters::default()))
}

/// Reset the per-turn counters.  Called at the start of every agent turn so
/// counts are scoped to the turn in which the errors occurred, not the whole
/// session.
pub fn reset_turn_counters() {
    if let Some(c) = COUNTERS.get()
        && let Ok(mut g) = c.lock() {
            g.per_kind.clear();
        }
}

/// Classify an error message and produce a hint.  Returns `None` if the
/// error is too short or too generic to be worth hinting about.
pub fn analyze(error_text: &str) -> Option<FailureAnalysis> {
    let text = error_text.trim();
    if text.is_empty() || text.len() < 8 {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    let kind = classify(&lower);
    let confidence = if kind == FailureKind::Unknown {
        Confidence::Low
    } else {
        Confidence::High
    };
    // Bump the per-turn counter
    let count = {
        let c = counters();
        let mut g = c.lock().unwrap_or_else(|p| p.into_inner());
        let entry = g.per_kind.entry(kind).or_insert(0);
        *entry = entry.saturating_add(1);
        *entry
    };

    let hint = build_hint(kind, count, text);
    let excerpt = truncate(text, 240);

    // If the hint is empty (e.g. low-confidence Unknown), still emit the
    // excerpt so the caller can show the model the raw text.
    if hint.is_empty() && confidence == Confidence::Low {
        return None;
    }

    Some(FailureAnalysis {
        kind,
        confidence,
        hint,
        excerpt,
    })
}

fn classify(lower: &str) -> FailureKind {
    // Order matters: check the more specific patterns first.
    if contains_any(lower, &["no such file", "does not exist", "not found", "cannot find", "could not find", "doesn't exist"]) {
        return FailureKind::MissingPath;
    }
    if contains_any(lower, &["permission denied", "access is denied", "eperm", "eacces", "not permitted", "operation not permitted"]) {
        return FailureKind::PermissionDenied;
    }
    if contains_any(lower, &["timed out", "timeout exceeded", "deadline exceeded", "operation timed out"]) {
        return FailureKind::Timeout;
    }
    if contains_any(lower, &["syntax error", "unexpected token", "parse error", "invalid json", "json parse", "yaml parse", "regex parse", "malformed"]) {
        return FailureKind::SyntaxError;
    }
    if contains_any(lower, &["connection refused", "connection reset", "connection aborted", "broken pipe", "network is unreachable", "host unreachable", "could not resolve", "couldn't resolve", "dns error", "tls handshake", "connection closed", "eof before message"]) {
        return FailureKind::NetworkError;
    }
    if contains_any(lower, &["no space left", "enospc", "disk full", "out of disk"]) {
        return FailureKind::OutOfDisk;
    }
    if contains_any(lower, &["out of memory", "cannot allocate", "enomem"]) {
        return FailureKind::OutOfMemory;
    }
    if contains_any(lower, &["already exists", "file exists", "eexist"]) {
        return FailureKind::AlreadyExists;
    }
    if contains_any(lower, &["invalid argument", "einval", "bad argument", "illegal argument"]) {
        return FailureKind::InvalidArgument;
    }
    if contains_any(lower, &["not implemented", "todo", "unimplemented"]) {
        return FailureKind::NotImplemented;
    }
    if contains_any(lower, &["cancelled", "canceled", "aborted by user", "user interrupted"]) {
        return FailureKind::Cancelled;
    }
    FailureKind::Unknown
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

fn build_hint(kind: FailureKind, count: u32, raw: &str) -> String {
    let already = if count > 1 {
        format!(" (already tried {}x this turn)", count)
    } else {
        String::new()
    };
    match kind {
        FailureKind::MissingPath => format!(
            "Hint: the path is wrong. Run `ls` on the parent directory before retrying{}.",
            already
        ),
        FailureKind::PermissionDenied => format!(
            "Hint: this is a permissions error. Check the file mode or use a different path{}.",
            already
        ),
        FailureKind::Timeout => format!(
            "Hint: the operation timed out. Try a smaller scope or raise the deadline{}.",
            already
        ),
        FailureKind::SyntaxError => format!(
            "Hint: the input has a syntax error. Read the error position carefully{}.",
            already
        ),
        FailureKind::NetworkError => {
            // Network errors are transient; we don't pile on retry pressure.
            String::new()
        }
        FailureKind::OutOfDisk => "Hint: host is out of disk space; free some and retry.".into(),
        FailureKind::OutOfMemory => "Hint: host is out of memory; shrink the request.".into(),
        FailureKind::AlreadyExists => {
            "Hint: target already exists. Use a different name or delete it first.".into()
        }
        FailureKind::InvalidArgument => {
            "Hint: an argument is invalid; review the input shape against the schema.".into()
        }
        FailureKind::NotImplemented => "Hint: this functionality is not implemented; pick a different approach.".into(),
        FailureKind::Cancelled => String::new(),
        FailureKind::Unknown => {
            // For unknown errors, forward a tiny excerpt of the raw text so
            // the model can self-debug.
            let ex = truncate(raw, 160);
            format!("Hint: error context: {}", ex)
        }
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    // Find a safe char boundary at or below max
    let mut end = max;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    let mut s = text[..end].to_string();
    s.push('…');
    s
}

/// Snapshot the per-turn counters for diagnostics.  Useful when the model
/// reports "I keep getting errors" and we want to know which kind.
pub fn snapshot() -> std::collections::HashMap<FailureKind, u32> {
    let c = counters();
    let g = c.lock().unwrap_or_else(|p| p.into_inner());
    g.per_kind.clone()
}

/// Module-level one-shot warning latch.  When the model has hit the same
/// failure kind 3+ times in a turn, [`escalate`] returns true so the caller
/// can interrupt with a stronger hint instead of letting the loop continue.
static ESCALATED: AtomicBool = AtomicBool::new(false);

pub fn should_escalate(kind: FailureKind) -> bool {
    if ESCALATED.swap(true, Ordering::Relaxed) {
        return false;
    }
    let c = counters();
    if let Ok(g) = c.lock()
        && let Some(&n) = g.per_kind.get(&kind) {
            return n >= 3;
        }
    false
}

pub fn clear_escalation() {
    ESCALATED.store(false, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_missing_path() {
        let a = analyze("cat: src/main.rs: No such file or directory").unwrap();
        assert_eq!(a.kind, FailureKind::MissingPath);
        assert_eq!(a.confidence, Confidence::High);
        assert!(a.hint.contains("path is wrong"));
    }

    #[test]
    fn classify_permission_denied() {
        let a = analyze("EACCES: permission denied, open '/etc/passwd'").unwrap();
        assert_eq!(a.kind, FailureKind::PermissionDenied);
    }

    #[test]
    fn classify_timeout() {
        let a = analyze("connection timed out after 30000ms").unwrap();
        assert_eq!(a.kind, FailureKind::Timeout);
    }

    #[test]
    fn classify_syntax_error() {
        let a = analyze("SyntaxError: Unexpected token } at position 42").unwrap();
        assert_eq!(a.kind, FailureKind::SyntaxError);
    }

    #[test]
    fn classify_network_error() {
        let a = analyze("connection reset by peer").unwrap();
        assert_eq!(a.kind, FailureKind::NetworkError);
    }

    #[test]
    fn classify_out_of_disk() {
        let a = analyze("ENOSPC: no space left on device").unwrap();
        assert_eq!(a.kind, FailureKind::OutOfDisk);
    }

    #[test]
    fn unknown_returns_none_or_low_confidence() {
        let a = analyze("z").unwrap();
        assert_eq!(a.kind, FailureKind::Unknown);
        assert_eq!(a.confidence, Confidence::Low);
    }

    #[test]
    fn empty_error_returns_none() {
        assert!(analyze("").is_none());
    }

    #[test]
    fn counter_accumulates_per_kind() {
        reset_turn_counters();
        analyze("No such file: a");
        analyze("No such file: b");
        analyze("No such file: c");
        let snap = snapshot();
        assert_eq!(snap.get(&FailureKind::MissingPath), Some(&3));
    }

    #[test]
    fn hint_includes_already_tried_after_two_failures() {
        reset_turn_counters();
        analyze("No such file: a");
        let a = analyze("No such file: b").unwrap();
        assert!(a.hint.contains("already tried 2x"));
    }

    #[test]
    fn escalation_triggers_after_three() {
        clear_escalation();
        reset_turn_counters();
        analyze("permission denied");
        analyze("permission denied");
        analyze("permission denied");
        assert!(should_escalate(FailureKind::PermissionDenied));
    }

    #[test]
    fn truncate_handles_multibyte() {
        let s = "résumé résumé résumé";
        let t = truncate(s, 8);
        assert!(t.ends_with('…'));
        // 8 bytes is mid-rune, must back off to a safe boundary
        assert!(s.is_char_boundary(t.trim_end_matches('…').len()));
    }
}