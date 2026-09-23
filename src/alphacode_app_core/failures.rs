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
//! - `MissingPath`: a path the model referenced does not exist.
//! - `PermissionDenied`: a permissions failure on a write / exec.
//! - `Timeout`: an operation exceeded its wall-clock budget.
//! - `SyntaxError`: shell / JSON / regex parse failure.
//! - `NetworkError`: connection refused / DNS / TLS.
//! - `OutOfDisk` / `OutOfMemory`: rare but distinct.
//! - `AlreadyExists`: file or resource already present.
//! - `InvalidArgument`: bad argument to a tool or system call.
//! - `NotImplemented`: operation not supported.
//! - `Cancelled`: operation was cancelled or interrupted.
//! - `RateLimited`: API or service rate limit exceeded.
//! - `AuthExpired`: authentication token expired or invalid.
//! - `CircuitBreaker`: circuit breaker is open (service degraded).
//! - `DependencyMissing`: required dependency or package not found.
//! - `ConfigError`: configuration error or missing config value.
//! - `DataCorruption`: data integrity or checksum failure.
//! - `RaceCondition`: concurrent modification or TOCTOU violation.
//! - `ResourceExhausted`: resource pool, thread pool, or connection limit.
//! - `Unknown`: any other error.
//!
//! All categories are derived from string-matching the error text.  This is
//! deliberately conservative — we never want to misclassify a recoverable
//! error as fatal — and the `Confidence::Low` variants emit no hint so the
//! model can decide on its own.
//!
//! ## Pattern Detection & Escalation
//!
//! The module tracks per-kind failure counts within a turn.  If the same
//! failure kind occurs 3+ times, escalation logic kicks in:
//!
//! - `should_escalate()` returns `true` once per escalation cycle.
//! - `detect_failure_pattern()` analyzes the full failure sequence and
//!   returns a `FailurePattern` describing what's happening.
//! - `suggest_recovery_strategy()` returns an actionable recovery plan
//!   specific to the failure kind and repetition count.
//! - `detect_failure_chains()` identifies causal chains where failure A
//!   is likely causing failure B (e.g., MissingPath → PermissionDenied
//!   when retrying with sudo, or Timeout → NetworkError on retry storm).

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
    RateLimited,
    AuthExpired,
    CircuitBreaker,
    DependencyMissing,
    ConfigError,
    DataCorruption,
    RaceCondition,
    ResourceExhausted,
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
    pub recovery_strategy: String,
    /// Truncated original error so the model can read it without bloating
    /// the next request.
    pub excerpt: String,
}

/// Describes a detected pattern of repeated failures.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailurePattern {
    /// The dominant failure kind in the pattern.
    pub dominant_kind: FailureKind,
    /// How many times this kind occurred.
    pub count: u32,
    /// Human-readable description of what's happening.
    pub description: String,
    /// Suggested action to break the pattern.
    pub suggested_action: String,
}

/// A detected causal chain: failure A likely caused failure B.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureChain {
    pub from: FailureKind,
    pub to: FailureKind,
    pub explanation: String,
    pub break_suggestion: String,
}

/// Counters so the model can see "you've hit this N times already this turn".
#[derive(Default)]
struct Counters {
    per_kind: HashMap<FailureKind, u32>,
    /// Full ordered sequence of failure kinds this turn (for chain detection).
    sequence: Vec<FailureKind>,
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
        && let Ok(mut g) = c.lock()
    {
        g.per_kind.clear();
        g.sequence.clear();
    }
}

/// Classify an error message and produce a hint.  Returns `None` if the
/// error is empty.
pub fn analyze(error_text: &str) -> Option<FailureAnalysis> {
    let text = error_text.trim();
    if text.is_empty() {
        return None;
    }
    let lower = text.to_ascii_lowercase();
    let kind = classify(&lower);
    let confidence = if kind == FailureKind::Unknown {
        Confidence::Low
    } else {
        Confidence::High
    };
    // Bump the per-turn counter and record in sequence
    let count = {
        let c = counters();
        let mut g = c.lock().unwrap_or_else(|p| p.into_inner());
        let entry = g.per_kind.entry(kind).or_insert(0);
        *entry = entry.saturating_add(1);
        let count = *entry;
        g.sequence.push(kind);
        count
    };

    let hint = build_hint(kind, count, text);
    let recovery_strategy = suggest_recovery_strategy(kind, count, text);
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
        recovery_strategy,
        excerpt,
    })
}

fn classify(lower: &str) -> FailureKind {
    // Order matters: check the more specific patterns first.

    // Rate limiting — check before network error since rate limits often
    // manifest as HTTP 429 responses.
    if contains_any(
        lower,
        &[
            "rate limit",
            "too many requests",
            "429",
            "throttled",
            "rate exceeded",
            "back off",
            "retry after",
            "slow down",
        ],
    ) {
        return FailureKind::RateLimited;
    }

    // Auth failures — distinct from permission denied.
    if contains_any(
        lower,
        &[
            "unauthorized",
            "401",
            "token expired",
            "token invalid",
            "authentication failed",
            "auth failed",
            "invalid token",
            "expired token",
            "oauth",
            "bearer",
            "credential",
            "login required",
            "session expired",
        ],
    ) {
        return FailureKind::AuthExpired;
    }

    // Circuit breaker — service degradation pattern.
    if contains_any(
        lower,
        &[
            "circuit breaker",
            "circuit open",
            "circuit half-open",
            "service unavailable",
            "503",
            "too many failures",
            "fast fail",
            "breaker tripped",
        ],
    ) {
        return FailureKind::CircuitBreaker;
    }

    // Data corruption — integrity issues.
    if contains_any(
        lower,
        &[
            "checksum",
            "crc",
            "hash mismatch",
            "integrity",
            "corrupt",
            "corrupted",
            "data corruption",
            "invalid data",
            "unexpected data format",
            "truncated",
            "incomplete write",
        ],
    ) {
        return FailureKind::DataCorruption;
    }

    // Race condition — concurrent access issues.
    if contains_any(
        lower,
        &[
            "race condition",
            "concurrent modification",
            "file changed",
            "changed since",
            "stale file",
            "toctou",
            "lost update",
            "optimistic lock",
            "version conflict",
        ],
    ) {
        return FailureKind::RaceCondition;
    }

    // Resource exhaustion — distinct from OOM/disk.
    if contains_any(
        lower,
        &[
            "resource exhausted",
            "too many open files",
            "emfile",
            "enfile",
            "connection pool",
            "thread pool",
            "pool exhausted",
            "max connections",
            "limit reached",
            "no available",
            "all slots busy",
        ],
    ) {
        return FailureKind::ResourceExhausted;
    }

    // Dependency missing — package or module not found.
    if contains_any(
        lower,
        &[
            "module not found",
            "no module named",
            "import not found",
            "package not found",
            "dependency not found",
            "cannot find module",
            "unresolved import",
            "missing dependency",
            "not installed",
            "command not found",
            "executable not found",
        ],
    ) {
        return FailureKind::DependencyMissing;
    }

    // Configuration error — invalid or missing config.
    if contains_any(
        lower,
        &[
            "config error",
            "configuration error",
            "invalid config",
            "missing config",
            "missing key",
            "missing field",
            "required field",
            "missing value",
            "env var",
            "environment variable",
            "not set",
            "missing setting",
            "invalid option",
        ],
    ) {
        return FailureKind::ConfigError;
    }

    if contains_any(
        lower,
        &[
            "no such file",
            "does not exist",
            "not found",
            "cannot find",
            "could not find",
            "doesn't exist",
            "no matching files",
            "path does not exist",
            "unable to access",
            "cannot access",
            "can't access",
        ],
    ) {
        return FailureKind::MissingPath;
    }
    if contains_any(
        lower,
        &[
            "permission denied",
            "access is denied",
            "eperm",
            "eacces",
            "not permitted",
            "operation not permitted",
            "read-only file system",
            "erofs",
            "immutable",
            "immutable file",
        ],
    ) {
        return FailureKind::PermissionDenied;
    }
    if contains_any(
        lower,
        &[
            "timed out",
            "timeout",
            "timeout exceeded",
            "deadline exceeded",
            "operation timed out",
            "request timeout",
            "read timed out",
            "initial response timeout",
        ],
    ) {
        return FailureKind::Timeout;
    }
    if contains_any(
        lower,
        &[
            "syntax error",
            "unexpected token",
            "parse error",
            "invalid json",
            "json parse",
            "yaml parse",
            "regex parse",
            "malformed",
            "invalid syntax",
            "unexpected character",
            "unclosed string",
            "unclosed bracket",
            "unexpected end of input",
            "expected",
        ],
    ) {
        return FailureKind::SyntaxError;
    }
    if contains_any(
        lower,
        &[
            "connection refused",
            "connection reset",
            "connection aborted",
            "broken pipe",
            "network is unreachable",
            "host unreachable",
            "could not resolve",
            "couldn't resolve",
            "dns error",
            "tls handshake",
            "connection closed",
            "eof before message",
            "connection pool closed",
            "channel closed",
            "peer closed connection",
            "transport error",
        ],
    ) {
        return FailureKind::NetworkError;
    }
    if contains_any(
        lower,
        &[
            "no space left",
            "enospc",
            "disk full",
            "out of disk",
            "quota exceeded",
        ],
    ) {
        return FailureKind::OutOfDisk;
    }
    if contains_any(
        lower,
        &["out of memory", "cannot allocate", "enomem", "oom"],
    ) {
        return FailureKind::OutOfMemory;
    }
    if contains_any(
        lower,
        &[
            "already exists",
            "file exists",
            "eexist",
            "directory not empty",
        ],
    ) {
        return FailureKind::AlreadyExists;
    }
    if contains_any(
        lower,
        &[
            "invalid argument",
            "einval",
            "bad argument",
            "illegal argument",
            "invalid value",
            "out of range",
        ],
    ) {
        return FailureKind::InvalidArgument;
    }
    if contains_any(
        lower,
        &[
            "not implemented",
            "todo",
            "unimplemented",
            "unsupported operation",
        ],
    ) {
        return FailureKind::NotImplemented;
    }
    if contains_any(
        lower,
        &[
            "cancelled",
            "canceled",
            "aborted by user",
            "user interrupted",
            "interrupted",
        ],
    ) {
        return FailureKind::Cancelled;
    }
    FailureKind::Unknown
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|n| haystack.contains(n))
}

fn build_hint(kind: FailureKind, count: u32, raw: &str) -> String {
    let retry_nag = if count > 3 {
        format!(
            " [{}x this turn — you MUST try a completely different approach]",
            count
        )
    } else if count > 2 {
        format!(" [{}x this turn — try a different approach]", count)
    } else if count > 1 {
        format!(" [{}x this turn]", count)
    } else {
        String::new()
    };
    if kind == FailureKind::Unknown {
        // For unknown errors, forward a tiny excerpt so the model can self-debug.
        let ex = truncate(raw, 120);
        return format!("Hint: {ex}{retry_nag}");
    }
    let base: String = match kind {
        FailureKind::MissingPath => {
            if count > 3 {
                "Hint: path still wrong after many retries. STOP guessing — run `ls` on the exact parent, read the output, then use the precise filename from it.".into()
            } else if count > 2 {
                "Hint: path still wrong after retries. Use `ls` on the parent, then use the exact name from the listing.".into()
            } else {
                "Hint: path not found. Run `ls` on the parent directory first.".into()
            }
        }
        FailureKind::PermissionDenied => {
            if count > 2 {
                "Hint: permissions error persists. Stop retrying the same write — check `ls -la` output, fix ownership, or write to a directory you own.".into()
            } else {
                "Hint: permissions error. Check file ownership/mode, or use a path you own.".into()
            }
        }
        FailureKind::Timeout => {
            if count > 2 {
                "Hint: timed out repeatedly. You MUST narrow the scope dramatically — use specific line ranges, exact file paths, and avoid scanning entire directories.".into()
            } else if count > 1 {
                "Hint: timed out again. Use a smaller scope, narrower file range, or specific line numbers.".into()
            } else {
                "Hint: timed out. Try a smaller scope or add line range limits.".into()
            }
        }
        FailureKind::SyntaxError => {
            if count > 2 {
                "Hint: persistent syntax error. Read the file first with the read tool, find the exact error location, then fix precisely at that point.".into()
            } else {
                "Hint: syntax error. Check the exact error position and fix the malformed input."
                    .into()
            }
        }
        FailureKind::NetworkError => {
            // Network errors are transient; retries are handled upstream.
            String::new()
        }
        FailureKind::OutOfDisk => "Hint: disk full. Remove temp files or free space.".into(),
        FailureKind::OutOfMemory => {
            "Hint: out of memory. Reduce the scope of the operation.".into()
        }
        FailureKind::AlreadyExists => {
            if count > 1 {
                "Hint: file already exists. Use `write` to overwrite entirely, or use `edit` for surgical changes, or pick a different filename.".into()
            } else {
                "Hint: file already exists. Use `write` to overwrite, or pick a new name.".into()
            }
        }
        FailureKind::InvalidArgument => {
            "Hint: bad argument. Re-read the tool schema and fix the input shape.".into()
        }
        FailureKind::NotImplemented => {
            "Hint: not implemented here. Try a different tool or approach.".into()
        }
        FailureKind::Cancelled => String::new(),
        FailureKind::RateLimited => {
            if count > 2 {
                "Hint: rate limited repeatedly. STOP hammering the API — wait before retrying, reduce request frequency, or batch requests together.".into()
            } else if count > 1 {
                "Hint: rate limited again. Space out your requests — add a delay or reduce batch size.".into()
            } else {
                "Hint: rate limited. Wait a moment before retrying, or reduce request frequency.".into()
            }
        }
        FailureKind::AuthExpired => {
            if count > 1 {
                "Hint: auth still failing after retries. The token is invalid — refresh credentials, check the auth header format, or re-authenticate before retrying.".into()
            } else {
                "Hint: authentication failed. Refresh your token or check credentials.".into()
            }
        }
        FailureKind::CircuitBreaker => {
            if count > 2 {
                "Hint: circuit breaker still open after many retries. The service is degraded — switch to a fallback path, try a different endpoint, or wait for recovery.".into()
            } else if count > 1 {
                "Hint: circuit breaker open. Wait for the service to recover, or use an alternative endpoint.".into()
            } else {
                "Hint: service unavailable (circuit breaker open). Retry with exponential backoff.".into()
            }
        }
        FailureKind::DependencyMissing => {
            if count > 1 {
                "Hint: dependency still missing. Verify the correct package name, check the package manager source, or install it manually before retrying.".into()
            } else {
                "Hint: missing dependency. Install it first with the appropriate package manager.".into()
            }
        }
        FailureKind::ConfigError => {
            if count > 1 {
                "Hint: config error persists. Read the config file directly, verify all required keys are present, and check for typos in key names.".into()
            } else {
                "Hint: configuration error. Check that all required config keys are present and valid.".into()
            }
        }
        FailureKind::DataCorruption => {
            "Hint: data corruption detected. Verify file integrity, re-download or re-generate the data, and check for incomplete writes.".into()
        }
        FailureKind::RaceCondition => {
            if count > 1 {
                "Hint: race condition recurring. The file changed during your operation — re-read it fresh, and perform the edit atomically without intermediate steps.".into()
            } else {
                "Hint: race condition detected. Re-read the file and retry the operation atomically.".into()
            }
        }
        FailureKind::ResourceExhausted => {
            if count > 1 {
                "Hint: resource pool still exhausted. Close idle connections, reduce concurrency, or increase the pool limit before retrying.".into()
            } else {
                "Hint: resource exhausted. Reduce concurrency or increase resource limits.".into()
            }
        }
        FailureKind::Unknown => {
            // Handled above via early return; unreachable here.
            String::new()
        }
    };
    if base.is_empty() || retry_nag.is_empty() {
        base
    } else {
        format!("{base}{retry_nag}")
    }
}

/// Suggest a concrete recovery strategy for a failure kind at a given
/// repetition count.  This is more actionable than the hint — it tells the
/// model *what to do* rather than *what went wrong*.
pub fn suggest_recovery_strategy(kind: FailureKind, count: u32, raw: &str) -> String {
    match kind {
        FailureKind::MissingPath => {
            if count >= 3 {
                "Recovery: list every file in the project tree with `find . -type f`, identify the correct path, and copy-paste it exactly.".into()
            } else if count == 2 {
                "Recovery: run `ls -R` on the project root or use glob to discover the actual file layout.".into()
            } else {
                "Recovery: list the parent directory contents before referencing any file path.".into()
            }
        }
        FailureKind::PermissionDenied => {
            if count >= 3 {
                "Recovery: stop using the protected path entirely — write to /tmp or a user-writable directory instead.".into()
            } else {
                "Recovery: use `chmod` or `chown` if you have permission, or pick a writable path like ~/ or /tmp/.".into()
            }
        }
        FailureKind::Timeout => {
            if count >= 3 {
                "Recovery: break the operation into 3-5 smaller steps. Process one file at a time, or use line ranges like `read(file, offset=1, limit=50)`.".into()
            } else {
                "Recovery: narrow the scope — use exact file paths, limit line ranges, or filter to a specific directory.".into()
            }
        }
        FailureKind::SyntaxError => {
            "Recovery: use the read tool to see the current file content, identify the exact line/column of the error, then apply a minimal fix at that position.".into()
        }
        FailureKind::NetworkError => {
            "Recovery: retry with exponential backoff (1s, 2s, 4s). If the endpoint is a mirror, try the primary. Check DNS resolution and TLS certificate validity.".into()
        }
        FailureKind::OutOfDisk => {
            "Recovery: remove temporary files (`rm -rf /tmp/*`), compress large files, or reduce the output size of the current operation.".into()
        }
        FailureKind::OutOfMemory => {
            "Recovery: process data in chunks — read 100 lines at a time, stream output instead of buffering, or use a more memory-efficient algorithm.".into()
        }
        FailureKind::AlreadyExists => {
            if count >= 3 {
                "Recovery: use a unique filename with a timestamp or hash suffix, or delete the existing file first with `rm` before creating it.".into()
            } else {
                "Recovery: either overwrite with `write` (destructive), append, or pick a new filename.".into()
            }
        }
        FailureKind::InvalidArgument => {
            "Recovery: read the tool's parameter schema, check the type of each argument (string vs int vs bool), and ensure all required fields are present.".into()
        }
        FailureKind::NotImplemented => {
            "Recovery: find an alternative tool that supports this operation, or implement the functionality using lower-level primitives (shell commands, file I/O).".into()
        }
        FailureKind::Cancelled => String::new(),
        FailureKind::RateLimited => {
            if count >= 3 {
                "Recovery: implement exponential backoff (wait 2^count seconds), reduce request rate to <1/sec, or batch multiple operations into a single request.".into()
            } else {
                "Recovery: wait 1-2 seconds before retrying. If the limit is per-minute, wait 60s. Consider batching requests.".into()
            }
        }
        FailureKind::AuthExpired => {
            "Recovery: obtain a fresh token via the auth flow, verify the Authorization header format (Bearer <token>), and check that the token has the required scopes.".into()
        }
        FailureKind::CircuitBreaker => {
            if count >= 3 {
                "Recovery: the service is down — use a cached response, switch to a different service, or skip this operation entirely with a graceful fallback.".into()
            } else {
                "Recovery: wait for the circuit breaker half-open state (usually 30-60s), then retry once. If it fails, back off again.".into()
            }
        }
        FailureKind::DependencyMissing => {
            "Recovery: install the missing dependency with the appropriate package manager (npm install, pip install, cargo add), then retry the operation.".into()
        }
        FailureKind::ConfigError => {
            "Recovery: read the config file, verify all required keys are present with correct types, check environment variables are set, and validate against any schema.".into()
        }
        FailureKind::DataCorruption => {
            "Recovery: re-download or re-generate the corrupted data from source, verify checksums after transfer, and ensure writes completed fully before reading.".into()
        }
        FailureKind::RaceCondition => {
            "Recovery: read the file once, make all edits in memory, then write atomically in a single operation. Do not re-read between edits.".into()
        }
        FailureKind::ResourceExhausted => {
            if count >= 3 {
                "Recovery: close all idle connections, reduce parallelism to 1, increase resource limits (ulimit -n), or wait for the pool to drain before retrying.".into()
            } else {
                "Recovery: reduce concurrency, close idle connections, or increase the resource pool limit in configuration.".into()
            }
        }
        FailureKind::Unknown => {
            // For unknown errors, try to extract actionable info from the raw message.
            let lower = raw.to_ascii_lowercase();
            if lower.contains("exit code") {
                "Recovery: check the command's exit code semantics — non-zero does not always mean failure. Read stderr output for details.".into()
            } else if lower.contains("pipe") {
                "Recovery: check for broken pipe — the receiving process may have exited early. Use named pipes or temp files instead.".into()
            } else {
                "Recovery: try a completely different approach. If using a shell command, try a different command or tool entirely.".into()
            }
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
/// failure kind 3+ times in a turn, [`should_escalate`] returns true so the caller
/// can interrupt with a stronger hint instead of letting the loop continue.
static ESCALATED: AtomicBool = AtomicBool::new(false);

pub fn should_escalate(kind: FailureKind) -> bool {
    if ESCALATED.swap(true, Ordering::Relaxed) {
        return false;
    }
    let c = counters();
    if let Ok(g) = c.lock()
        && let Some(&n) = g.per_kind.get(&kind)
    {
        return n >= 3;
    }
    false
}

pub fn clear_escalation() {
    ESCALATED.store(false, Ordering::Relaxed);
}

/// Detect a failure pattern across the current turn's failures.
/// Returns `None` if no significant pattern is found.
pub fn detect_failure_pattern() -> Option<FailurePattern> {
    let c = counters();
    let g = c.lock().unwrap_or_else(|p| p.into_inner());

    // Find the dominant failure kind (highest count, ≥3).
    let (dominant, count) = g.per_kind.iter().max_by_key(|(_, c)| *c)?;

    if *count < 3 {
        return None;
    }

    let description = match *dominant {
        FailureKind::MissingPath => format!(
            "The model has failed to find a path {} times this turn, likely guessing filenames without checking the filesystem.",
            count
        ),
        FailureKind::PermissionDenied => format!(
            "The model has hit permission errors {} times, likely retrying the same write to a protected location.",
            count
        ),
        FailureKind::Timeout => format!(
            "The model has timed out {} times, likely attempting operations that are too large or broad in scope.",
            count
        ),
        FailureKind::SyntaxError => format!(
            "The model has produced syntax errors {} times, likely not reading the current content before editing.",
            count
        ),
        FailureKind::NetworkError => format!(
            "The model has encountered {} network errors — the service may be down or DNS is failing.",
            count
        ),
        FailureKind::RateLimited => format!(
            "The model has been rate-limited {} times — it is hammering the API without backoff.",
            count
        ),
        FailureKind::AuthExpired => format!(
            "The model has {} auth failures — credentials are stale or the token format is wrong.",
            count
        ),
        FailureKind::CircuitBreaker => format!(
            "The model has {} circuit breaker trips — the downstream service is degraded and recovering slowly.",
            count
        ),
        FailureKind::ResourceExhausted => format!(
            "The model has {} resource exhaustion errors — connection or thread pools are maxed out.",
            count
        ),
        _ => format!(
            "The model has hit {} failures of kind {:?} this turn.",
            count, dominant
        ),
    };

    let suggested_action = match dominant {
        FailureKind::MissingPath => "STOP guessing paths. Use `ls`, `find`, or glob to discover the actual file layout before any further references.".into(),
        FailureKind::PermissionDenied => "STOP retrying the protected operation. Switch to a user-writable path or modify permissions first.".into(),
        FailureKind::Timeout => "STOP attempting broad operations. Break the task into 5+ smaller steps with specific line ranges and file paths.".into(),
        FailureKind::SyntaxError => "STOP editing blindly. Read the file, find the exact error line, fix minimally, and verify with a parse check.".into(),
        FailureKind::NetworkError => "STOP retrying immediately. Wait 30s, check DNS, verify the endpoint URL, and try a different transport if available.".into(),
        FailureKind::RateLimited => "STOP all API calls for at least 60 seconds. Then resume with 1 request per 2 seconds max.".into(),
        FailureKind::AuthExpired => "STOP retrying with stale credentials. Refresh the token, verify scopes, and re-authenticate before any API calls.".into(),
        FailureKind::CircuitBreaker => "STOP calling this service. Use a cached response, alternative endpoint, or skip this operation gracefully.".into(),
        FailureKind::ResourceExhausted => "STOP creating new connections. Close idle ones, reduce concurrency to 1, and increase pool limits in config.".into(),
        _ => "Try a completely different approach — the current strategy is not working.".into(),
    };

    Some(FailurePattern {
        dominant_kind: *dominant,
        count: *count,
        description,
        suggested_action,
    })
}

/// Detect causal chains in the failure sequence.  For example, if
/// MissingPath errors are followed by PermissionDenied errors, the model
/// may be trying to work around path errors with sudo.
pub fn detect_failure_chains() -> Vec<FailureChain> {
    let c = counters();
    let g = c.lock().unwrap_or_else(|p| p.into_inner());
    let seq = &g.sequence;

    if seq.len() < 2 {
        return Vec::new();
    }

    let mut chains = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // Slide a window of 2 over the sequence and detect known patterns.
    for window in seq.windows(2) {
        let from = window[0];
        let to = window[1];
        if from == to {
            continue; // same-kind retries are not chains
        }
        let pair = (from, to);
        if seen.contains(&pair) {
            continue;
        }
        seen.insert(pair);

        let chain = match (from, to) {
            (FailureKind::MissingPath, FailureKind::PermissionDenied) => Some(FailureChain {
                from,
                to,
                explanation: "MissingPath → PermissionDenied: the model is likely using sudo to access a missing path, getting permission errors instead.".into(),
                break_suggestion: "Stop using sudo. The path does not exist — find the correct path first with `ls` or `find`.".into(),
            }),
            (FailureKind::PermissionDenied, FailureKind::MissingPath) => Some(FailureChain {
                from,
                to,
                explanation: "PermissionDenied → MissingPath: the model is switching paths after permission errors, but guessing wrong.".into(),
                break_suggestion: "List available directories you CAN write to, then use one of them instead of guessing paths.".into(),
            }),
            (FailureKind::Timeout, FailureKind::NetworkError) => Some(FailureChain {
                from,
                to,
                explanation: "Timeout → NetworkError: repeated timeouts likely caused a connection reset or network-level failure.".into(),
                break_suggestion: "Stop retrying the timed-out operation. Wait 30s, then retry with a much smaller scope.".into(),
            }),
            (FailureKind::NetworkError, FailureKind::Timeout) => Some(FailureChain {
                from,
                to,
                explanation: "NetworkError → Timeout: network issues are causing cascading timeouts on retries.".into(),
                break_suggestion: "Check DNS resolution, verify the endpoint is reachable, and use exponential backoff.".into(),
            }),
            (FailureKind::RateLimited, FailureKind::AuthExpired) => Some(FailureChain {
                from,
                to,
                explanation: "RateLimited → AuthExpired: after hitting rate limits, the token may have been revoked or invalidated.".into(),
                break_suggestion: "Refresh credentials after a rate limit hit, as the service may have invalidated the token.".into(),
            }),
            (FailureKind::RateLimited, FailureKind::CircuitBreaker) => Some(FailureChain {
                from,
                to,
                explanation: "RateLimited → CircuitBreaker: hammering the API triggered rate limiting, then the service tripped its circuit breaker.".into(),
                break_suggestion: "Stop all requests immediately. Wait for the circuit breaker to recover (30-60s) before resuming.".into(),
            }),
            (FailureKind::CircuitBreaker, FailureKind::NetworkError) => Some(FailureChain {
                from,
                to,
                explanation: "CircuitBreaker → NetworkError: the degraded service is now refusing connections at the network level.".into(),
                break_suggestion: "The service is down. Use cached data, alternative endpoints, or skip this operation entirely.".into(),
            }),
            (FailureKind::DependencyMissing, FailureKind::SyntaxError) => Some(FailureChain {
                from,
                to,
                explanation: "DependencyMissing → SyntaxError: missing dependency caused a parse error in the code that imports it.".into(),
                break_suggestion: "Install the missing dependency first, then re-run the operation that produces the syntax error.".into(),
            }),
            (FailureKind::ConfigError, FailureKind::PermissionDenied) => Some(FailureChain {
                from,
                to,
                explanation: "ConfigError → PermissionDenied: a misconfigured path or credential is causing permission errors.".into(),
                break_suggestion: "Check the config file for correct paths, credentials, and file permissions.".into(),
            }),
            (FailureKind::DataCorruption, FailureKind::Timeout) => Some(FailureChain {
                from,
                to,
                explanation: "DataCorruption → Timeout: corrupted data may be causing infinite loops or excessive processing time.".into(),
                break_suggestion: "Discard the corrupted data, re-download/re-generate it, then retry.".into(),
            }),
            (FailureKind::RaceCondition, FailureKind::SyntaxError) => Some(FailureChain {
                from,
                to,
                explanation: "RaceCondition → SyntaxError: the file changed during editing, producing a syntactically invalid result.".into(),
                break_suggestion: "Re-read the file, make all edits in one atomic write, and verify the result immediately.".into(),
            }),
            (FailureKind::ResourceExhausted, FailureKind::Timeout) => Some(FailureChain {
                from,
                to,
                explanation: "ResourceExhausted → Timeout: exhausted resources (connections, threads) are causing operations to hang.".into(),
                break_suggestion: "Close idle connections, reduce concurrency, and increase resource pool limits before retrying.".into(),
            }),
            _ => None,
        };

        if let Some(c) = chain {
            chains.push(c);
        }
    }

    chains
}

/// Get the full failure sequence for this turn.  Useful for external
/// diagnostics or logging.
pub fn get_failure_sequence() -> Vec<FailureKind> {
    let c = counters();
    let g = c.lock().unwrap_or_else(|p| p.into_inner());
    g.sequence.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_missing_path() {
        let a = analyze("cat: src/main.rs: No such file or directory").unwrap();
        assert_eq!(a.kind, FailureKind::MissingPath);
        assert_eq!(a.confidence, Confidence::High);
        assert!(a.hint.contains("path"));
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
    fn classify_rate_limited() {
        let a = analyze("429: rate limit exceeded, retry after 60s").unwrap();
        assert_eq!(a.kind, FailureKind::RateLimited);
    }

    #[test]
    fn classify_auth_expired() {
        let a = analyze("401: token expired, please re-authenticate").unwrap();
        assert_eq!(a.kind, FailureKind::AuthExpired);
    }

    #[test]
    fn classify_circuit_breaker() {
        let a = analyze("circuit breaker open, service unavailable").unwrap();
        assert_eq!(a.kind, FailureKind::CircuitBreaker);
    }

    #[test]
    fn classify_dependency_missing() {
        let a = analyze("ModuleNotFoundError: No module named 'requests'").unwrap();
        assert_eq!(a.kind, FailureKind::DependencyMissing);
    }

    #[test]
    fn classify_config_error() {
        let a = analyze("ConfigError: missing required field 'database.url'").unwrap();
        assert_eq!(a.kind, FailureKind::ConfigError);
    }

    #[test]
    fn classify_data_corruption() {
        let a = analyze("Checksum mismatch: expected abc, got def").unwrap();
        assert_eq!(a.kind, FailureKind::DataCorruption);
    }

    #[test]
    fn classify_race_condition() {
        let a = analyze("File changed since last read, race condition detected").unwrap();
        assert_eq!(a.kind, FailureKind::RaceCondition);
    }

    #[test]
    fn classify_resource_exhausted() {
        let a = analyze("EMFILE: too many open files").unwrap();
        assert_eq!(a.kind, FailureKind::ResourceExhausted);
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
    fn hint_includes_retry_nag_after_two_failures() {
        reset_turn_counters();
        analyze("No such file: a");
        let a = analyze("No such file: b").unwrap();
        assert!(a.hint.contains("2x this turn"));
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

    #[test]
    fn failure_pattern_detected_after_three() {
        reset_turn_counters();
        analyze("No such file: a");
        analyze("No such file: b");
        analyze("No such file: c");
        let pattern = detect_failure_pattern().unwrap();
        assert_eq!(pattern.dominant_kind, FailureKind::MissingPath);
        assert_eq!(pattern.count, 3);
    }

    #[test]
    fn no_pattern_below_threshold() {
        reset_turn_counters();
        analyze("No such file: a");
        analyze("No such file: b");
        assert!(detect_failure_pattern().is_none());
    }

    #[test]
    fn recovery_strategy_is_nonempty() {
        reset_turn_counters();
        let a = analyze("No such file: a").unwrap();
        assert!(!a.recovery_strategy.is_empty());
    }

    #[test]
    fn failure_chains_detected() {
        reset_turn_counters();
        analyze("permission denied");
        analyze("no such file");
        let chains = detect_failure_chains();
        assert!(!chains.is_empty());
        assert!(
            chains.iter().any(
                |c| c.from == FailureKind::PermissionDenied && c.to == FailureKind::MissingPath
            )
        );
    }

    #[test]
    fn no_chains_for_same_kind() {
        reset_turn_counters();
        analyze("No such file: a");
        analyze("No such file: b");
        let chains = detect_failure_chains();
        assert!(chains.is_empty());
    }

    #[test]
    fn failure_sequence_tracked() {
        reset_turn_counters();
        analyze("No such file: a");
        analyze("permission denied");
        analyze("timed out");
        let seq = get_failure_sequence();
        assert_eq!(
            seq,
            vec![
                FailureKind::MissingPath,
                FailureKind::PermissionDenied,
                FailureKind::Timeout,
            ]
        );
    }

    #[test]
    fn multiple_chains_detected() {
        reset_turn_counters();
        analyze("timeout after 30s");
        analyze("connection reset by peer");
        analyze("429: rate limit exceeded");
        analyze("503: service unavailable, circuit breaker open");
        let chains = detect_failure_chains();
        assert!(chains.len() >= 2);
    }

    #[test]
    fn rate_limited_takes_priority_over_network() {
        let a = analyze("429 too many requests, rate limit exceeded").unwrap();
        assert_eq!(a.kind, FailureKind::RateLimited);
    }

    #[test]
    fn auth_expired_takes_priority_over_permission() {
        let a = analyze("401 unauthorized: token expired, authentication failed").unwrap();
        assert_eq!(a.kind, FailureKind::AuthExpired);
    }

    #[test]
    fn circuit_breaker_takes_priority_over_network() {
        let a = analyze("circuit breaker open, connection refused").unwrap();
        assert_eq!(a.kind, FailureKind::CircuitBreaker);
    }
}
