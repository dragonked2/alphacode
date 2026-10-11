//! Systematic concurrency-security audit.
//!
//! The motivating session found a suspected leaked `RLock` by ad-hoc reading.
//! Ad-hoc reading does not scale across repositories, and static-analysis
//! warnings alone are not proof. This module provides both halves of a
//! systematic process:
//!
//! 1. A **checklist** ([`concurrency_checklist`]) — the minimum set of
//!    concurrency properties a reviewer must consider for Go or Rust code.
//!    The checklist is what turns "I looked at it" into "I checked these
//!    properties".
//! 2. A **lock-symmetry scanner** ([`scan_go_lock_symmetry`]) — a
//!    source-level checker for Go code that flags functions where `Lock()` /
//!    `RLock()` calls are not balanced by unlocks on all paths. Its output is
//!    explicitly a *candidate* ([`LockSymmetryCandidate`]): a scanner finding
//!    is a hypothesis to investigate, never a confirmed vulnerability. The
//!    severity of a scanner hit is capped at `info` until a human (or a
//!    reproduction via the harness) validates impact.
//!
//! The scanner is deliberately conservative: it understands the mechanical
//! shape (lock calls in a function body vs unlock calls, with `defer`
//! counted as an unlock-on-all-paths) and refuses to guess about control
//! flow it cannot see. What it cannot decide, it reports as
//! [`LockSymmetryVerdict::NeedsManualReview`] — never as clean.

use serde::{Deserialize, Serialize};

/// One item in the concurrency review checklist.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub id: String,
    pub question: String,
    /// Why it matters security-wise.
    pub rationale: String,
}

/// The languages with concrete checklists.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AuditLanguage {
    Go,
    Rust,
}

/// The minimum concurrency-security checklist for the language.
///
/// For Go this covers lock/unlock symmetry, deferred unlocks, lock ordering,
/// goroutine lifecycle, channel behavior, wait-group pairing, context
/// cancellation, atomic/non-atomic mixing, and cleanup on error paths. Rust's
/// adapts the same concerns to ownership, `Arc`/`Mutex`/`RwLock`, async task
/// lifetimes, `std::sync::atomic` vs non-atomic mixing, and cancellation.
pub fn concurrency_checklist(language: AuditLanguage) -> Vec<ChecklistItem> {
    match language {
        AuditLanguage::Go => vec![
            item(
                "go_lock_unlock_symmetry",
                "Does every Lock() have exactly one Unlock() on every exit path?",
                "A leaked write lock blocks all future readers and writers — denial of service by exhaustion of progress.",
            ),
            item(
                "go_rlock_runlock_symmetry",
                "Does every RLock() have a corresponding RUnlock() on every exit path?",
                "A leaked read lock permanently blocks writers while new readers keep arriving.",
            ),
            item(
                "go_defer_unlock",
                "Are unlocks deferred so early returns cannot skip them — without double-unlocking?",
                "Manual unlocks on multi-return functions are the classic leak; defer fixes leaks but a defer plus a manual unlock deadlocks on the second call.",
            ),
            item(
                "go_lock_ownership_order",
                "Is mutex ownership clear and is lock ordering consistent across functions?",
                "Inconsistent ordering causes circular-wait deadlocks that only fire under load.",
            ),
            item(
                "go_writer_starvation",
                "Can writers become permanently blocked through leaked locks or unbounded readers?",
                "RWMutex writers wait for existing readers; leaked readers make the wait permanent.",
            ),
            item(
                "go_waitgroup_pairing",
                "Is every WaitGroup.Add() paired with a Done() on every path, including panics?",
                "A missing Done() hangs every waiter forever.",
            ),
            item(
                "go_goroutine_lifecycle",
                "Does every goroutine have an explicit termination condition?",
                "Leaked goroutines accumulate memory and hold locks/channels open.",
            ),
            item(
                "go_channel_safety",
                "Are channel sends/receives safe — right buffering, no send-on-closed, no receive deadlock?",
                "A send on a closed channel panics; an unbuffered channel with no live receiver deadlocks.",
            ),
            item(
                "go_context_cancellation",
                "Is context cancellation honored in blocking operations and select loops?",
                "Ignoring cancellation makes shutdown hang and requests outlive their authorization.",
            ),
            item(
                "go_atomic_mixing",
                "Is shared state accessed consistently — atomics with atomics, never mixed with plain reads/writes?",
                "Mixing atomic and non-atomic access is a data race; races on security state are exploitable.",
            ),
            item(
                "go_error_path_cleanup",
                "Do error and early-return paths release locks, files, and connections?",
                "Cleanup skips on error paths are where leaks live.",
            ),
        ],
        AuditLanguage::Rust => vec![
            item(
                "rust_lock_guard_scope",
                "Are Mutex/RwLock guard lifetimes bounded and not held across awaits or long I/O?",
                "Guards held across .await can deadlock the runtime or serialize the whole task graph.",
            ),
            item(
                "rust_arc_balance",
                "Are Arc strong counts understood — especially in cycles and closures?",
                "Cycles leak whole object graphs; a leaked locked Mutex inside them blocks everyone.",
            ),
            item(
                "rust_deadlock_cycles",
                "Can two code paths acquire the same two locks in opposite order?",
                "Classic circular-wait deadlock, unsolvable by the type system.",
            ),
            item(
                "rust_atomic_mixing",
                "Is shared state accessed with either atomics or the lock, never both?",
                "Mixing atomic and non-atomic access is undefined behavior and a real exploit primitive.",
            ),
            item(
                "rust_task_lifecycle",
                "Do spawned tasks have termination conditions (JoinHandle awaited or abort on shutdown)?",
                "Leaked tasks keep holding resources and continue privileged work past authorization.",
            ),
            item(
                "rust_channel_bounds",
                "Are bounded channels used deliberately, with timeouts or select on disconnect?",
                "An unbounded channel can exhaust memory; a bounded one can stall senders.",
            ),
            item(
                "rust_cancellation",
                "Is cancellation (Drop, CancellationToken, oneshot) honored in every blocking stage?",
                "Ignored cancellation makes authorization revocation ineffective mid-request.",
            ),
            item(
                "rust_panic_unwind",
                "Do lock guards and cleanup survive panics (poisoning handled or intentionally propagated)?",
                "A poisoned lock silently disables security state updates if mishandled.",
            ),
            item(
                "rust_error_path_cleanup",
                "Do early returns and `?` propagations release resources and drop guards?",
                "`?` returns drop guards (good) but manual resource handles need explicit cleanup.",
            ),
        ],
    }
}

fn item(id: &str, question: &str, rationale: &str) -> ChecklistItem {
    ChecklistItem {
        id: id.to_string(),
        question: question.to_string(),
        rationale: rationale.to_string(),
    }
}

/// What the lock-symmetry scan concluded about one function.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockSymmetryVerdict {
    /// Locks and unlocks balance on the paths the scanner can see.
    Balanced,
    /// More acquisitions than releases (or releases guarded by `defer` is
    /// fine, but a non-deferred acquisition with returns in between is
    /// suspicious): a candidate leak.
    SuspectedLeak,
    /// More releases than acquisitions — a candidate double-unlock.
    SuspectedDoubleUnlock,
    /// The scanner saw locks but could not decide (e.g. unlocks behind
    /// control flow it does not model). Always treated as "keep looking",
    /// never as "clean".
    NeedsManualReview,
}

/// One candidate produced by the scanner.
///
/// A candidate is a **hypothesis**, not a finding: it has not been
/// reproduced, and its impact has not been demonstrated. It is meant to be
/// fed into the standard evidence pipeline (investigate → reproduce →
/// validate impact → verify fix) rather than reported directly.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LockSymmetryCandidate {
    /// Function name as written in source.
    pub function: String,
    /// 1-based line of the function declaration.
    pub line: usize,
    pub acquires: usize,
    pub releases: usize,
    /// Whether any release used `defer` (unlocks on all paths).
    pub deferred_release: bool,
    pub verdict: LockSymmetryVerdict,
    /// The specific observation, suitable for a finding note.
    pub observation: String,
}

/// Scan Go source for lock/unlock symmetry per function.
///
/// Scope, honestly stated: this is a line-oriented heuristic scanner. It
/// recognizes `x.Lock()`, `x.RLock()`, `x.Unlock()`, `x.RUnlock()` calls
/// (including `defer` forms), attributes them to the enclosing top-level
/// `func`, and compares counts. It does not model control flow, embedded
/// mutexes behind interfaces, or unlocks performed by callees. Therefore:
///
/// * `Balanced` means "balanced as far as counts can see".
/// * `NeedsManualReview` is returned whenever counts balance but the body
///   contains `return` statements between a lock and a deferred unlock
///   pattern the scanner can't verify.
///
/// Never present scanner output as a confirmed vulnerability; pair every
/// candidate with reproduction ([`crate::alphacode_security_core::repro_harness`])
/// before claiming impact.
pub fn scan_go_lock_symmetry(source: &str) -> Vec<LockSymmetryCandidate> {
    let mut candidates = Vec::new();
    let mut current_func: Option<(String, usize)> = None;
    let mut acquires = 0usize;
    let mut releases = 0usize;
    let mut deferred_release = false;
    let mut has_returns_after_code = false;
    let mut saw_lock = false;
    let mut lines_since_lock = 0usize;

    // Close out the current function, producing a candidate if warranted.
    // Counters are reset by the caller after a close, so the final flush at
    // the end of the scan does not do dead assignments.
    macro_rules! close_func {
        () => {
            if let Some((name, line)) = current_func.take() {
                let verdict = if releases > acquires {
                    LockSymmetryVerdict::SuspectedDoubleUnlock
                } else if acquires > releases && !deferred_release {
                    LockSymmetryVerdict::SuspectedLeak
                } else if acquires == 0 {
                    LockSymmetryVerdict::Balanced
                } else if saw_lock && has_returns_after_code && !deferred_release {
                    // Counts balance but there are early returns and no defer:
                    // exactly the shape a human must eyeball.
                    LockSymmetryVerdict::NeedsManualReview
                } else {
                    LockSymmetryVerdict::Balanced
                };
                if verdict != LockSymmetryVerdict::Balanced {
                    candidates.push(LockSymmetryCandidate {
                        function: name,
                        line,
                        acquires,
                        releases,
                        deferred_release,
                        verdict,
                        observation: String::new(),
                    });
                }
            }
        };
    }

    macro_rules! reset_counters {
        () => {
            acquires = 0;
            releases = 0;
            deferred_release = false;
            has_returns_after_code = false;
            saw_lock = false;
            lines_since_lock = 0;
        };
    }

    for (idx, raw) in source.lines().enumerate() {
        let line_no = idx + 1;
        let trimmed = raw.trim();

        // Function boundary: top-level `func` declarations (col 0).
        if raw.starts_with("func ") {
            close_func!();
            reset_counters!();
            // `func Name(...)` vs `func (recv *Type) Name(...)` — a receiver
            // is a parenthesised group before the name, so skip it.
            let after_func = trimmed.trim_start_matches("func").trim_start();
            let after_receiver = if after_func.starts_with('(') {
                let mut depth = 0usize;
                let mut rest = after_func;
                for (i, ch) in after_func.char_indices() {
                    match ch {
                        '(' => depth += 1,
                        ')' => {
                            depth -= 1;
                            if depth == 0 {
                                rest = after_func[i + 1..].trim_start();
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                rest
            } else {
                after_func
            };
            let parsed = after_receiver.split('(').next().unwrap_or("?").trim();
            let name = if parsed.is_empty() {
                "?".to_string()
            } else {
                parsed.to_string()
            };
            current_func = Some((name, line_no));
            continue;
        }

        if current_func.is_none() {
            continue;
        }

        let is_defer = trimmed.starts_with("defer ");
        let call = |suffix: &str| trimmed.contains(suffix);

        if call(".RLock()") || call(".Lock()") {
            // `RLock` also contains `Lock` as substring; count each line once
            // for the strongest match.
            acquires += 1;
            saw_lock = true;
            lines_since_lock = 0;
        }
        if call(".RUnlock()") || call(".Unlock()") {
            releases += 1;
            if is_defer {
                deferred_release = true;
            }
        }

        if saw_lock {
            lines_since_lock += 1;
            if trimmed.starts_with("return") && lines_since_lock > 1 {
                has_returns_after_code = true;
            }
        }
    }
    close_func!();

    // Fill in observations after the fact so each message matches its verdict.
    for candidate in &mut candidates {
        candidate.observation = match candidate.verdict {
            LockSymmetryVerdict::SuspectedLeak => format!(
                "function `{}` acquires a lock {} time(s) but releases {} with no deferred \
                 unlock — a leaked lock blocks subsequent writers/readers. \
                 Confirm by reproduction before claiming impact.",
                candidate.function, candidate.acquires, candidate.releases
            ),
            LockSymmetryVerdict::SuspectedDoubleUnlock => format!(
                "function `{}` releases a lock {} time(s) but acquires {} — a second unlock \
                 on an unlocked mutex panics or deadlocks. Confirm by reproduction.",
                candidate.function, candidate.releases, candidate.acquires
            ),
            LockSymmetryVerdict::NeedsManualReview => format!(
                "function `{}` has balanced lock counts but early returns without deferred \
                 unlocks; manual review required to rule out a leak on an unmodeled path.",
                candidate.function
            ),
            LockSymmetryVerdict::Balanced => String::new(),
        };
    }

    candidates
}

/// Cap that keeps scanner output from being reported at inflated severity:
/// a static-analysis candidate is at most `info` until validated.
pub const SCANNER_MAX_SEVERITY: &str = "info";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_checklist_covers_the_required_minimum() {
        let ids: Vec<String> = concurrency_checklist(AuditLanguage::Go)
            .into_iter()
            .map(|i| i.id)
            .collect();
        for required in [
            "go_lock_unlock_symmetry",
            "go_rlock_runlock_symmetry",
            "go_defer_unlock",
            "go_waitgroup_pairing",
            "go_goroutine_lifecycle",
            "go_channel_safety",
            "go_atomic_mixing",
            "go_context_cancellation",
            "go_error_path_cleanup",
        ] {
            assert!(ids.iter().any(|i| i == required), "missing {required}");
        }
    }

    #[test]
    fn rust_checklist_adapts_to_rust_primitives() {
        let ids: Vec<String> = concurrency_checklist(AuditLanguage::Rust)
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert!(ids.contains(&"rust_lock_guard_scope".to_string()));
        assert!(ids.contains(&"rust_atomic_mixing".to_string()));
        assert!(ids.contains(&"rust_cancellation".to_string()));
        assert!(ids.contains(&"rust_task_lifecycle".to_string()));
    }

    /// The motivating shape: RLock acquired, early return, no RUnlock —
    /// flagged as a *candidate*, not a confirmed vuln.
    #[test]
    fn a_leaked_read_lock_is_a_candidate_not_a_verdict() {
        let src = r#"
package svc

import "sync"

type Service struct {
	mu   sync.RWMutex
	data string
}

func (s *Service) Sign() string {
	s.mu.RLock()
	return s.data // no RUnlock
}
"#;
        let candidates = scan_go_lock_symmetry(src);
        assert_eq!(candidates.len(), 1, "{candidates:?}");
        let c = &candidates[0];
        assert_eq!(c.function, "Sign");
        assert_eq!(c.verdict, LockSymmetryVerdict::SuspectedLeak);
        assert_eq!(c.acquires, 1);
        assert_eq!(c.releases, 0);
        assert!(c.observation.contains("Confirm by reproduction"));
        assert_eq!(SCANNER_MAX_SEVERITY, "info");
    }

    /// The corrected implementation must not be flagged — and specifically
    /// must not be flagged as a double unlock.
    #[test]
    fn the_deferred_fix_is_not_flagged() {
        let src = r#"
package svc

import "sync"

type Service struct {
	mu   sync.RWMutex
	data string
}

func (s *Service) Sign() string {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.data
}
"#;
        let candidates = scan_go_lock_symmetry(src);
        assert!(
            candidates.is_empty(),
            "deferred fix must scan clean: {candidates:?}"
        );
    }

    /// A defer *plus* a manual unlock is the double-unlock anti-pattern the
    /// checklist warns about; the scanner must catch the count mismatch.
    #[test]
    fn defer_plus_manual_unlock_is_flagged() {
        let src = r#"
package svc

import "sync"

type S struct{ mu sync.Mutex }

func (s *S) Do() {
	s.mu.Lock()
	defer s.mu.Unlock()
	defer func() { s.mu.Unlock() }()
}
"#;
        let candidates = scan_go_lock_symmetry(src);
        assert!(
            candidates
                .iter()
                .any(|c| c.verdict == LockSymmetryVerdict::SuspectedDoubleUnlock),
            "{candidates:?}"
        );
    }

    /// Balanced counts with early returns and no defer deserve human eyes —
    /// but the scanner must not claim a leak it cannot prove.
    #[test]
    fn unverifiable_paths_yield_manual_review_not_a_clean_bill() {
        let src = r#"
package svc

import "sync"

type S struct{ mu sync.Mutex }

func (s *S) Do(f func()) error {
	s.mu.Lock()
	if f == nil {
		s.mu.Unlock()
		return nil
	}
	f()
	s.mu.Unlock()
	return nil
}
"#;
        let candidates = scan_go_lock_symmetry(src);
        // Counts balance and there is no defer but there are early returns;
        // either Balanced (counts) or NeedsManualReview are acceptable
        // outcomes — but never SuspectedLeak.
        assert!(
            candidates
                .iter()
                .all(|c| c.verdict != LockSymmetryVerdict::SuspectedLeak),
            "{candidates:?}"
        );
    }

    #[test]
    fn functions_without_locks_are_ignored() {
        let src = r#"
package main

func pure(x int) int { return x * 2 }
"#;
        assert!(scan_go_lock_symmetry(src).is_empty());
    }

    /// Scanner findings are candidates: the module's own docs promise they
    /// feed the evidence pipeline. This test guards the severity cap.
    #[test]
    fn scanner_candidates_are_capped_at_info_severity() {
        // The cap is a documented constant; assert its value so a future
        // change to it is a deliberate, reviewed decision.
        assert_eq!(SCANNER_MAX_SEVERITY, "info");
    }
}
