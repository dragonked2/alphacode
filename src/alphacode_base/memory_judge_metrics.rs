//! Attribution + measurement of "no-LLM memory mode" conversions.
//!
//! Memory is only reliably productive when the LLM precision judge (the
//! listwise consensus rerank) decides what to surface. Whenever a turn surfaces
//! (or suppresses) memories WITHOUT that judge, it has "converted" to no-LLM
//! mode. Some conversions are intended (the user explicitly opted out of the
//! sidecar); most are silent degradations we want to drive to zero (lost login,
//! judge transport failures, unparseable judge responses, etc.).
//!
//! # Why this is exhaustive
//!
//! Every memory surfacing turn ends in exactly one [`JudgeDecision`]. The enum
//! is the single source of truth for "all the ways memory can decide". Because
//! the recording site is a single `match`-free call and the variants are a
//! closed Rust enum, a new code path that surfaces memory cannot ship without
//! choosing a variant here. New paths => add a variant => the dashboards and the
//! `is_no_llm()` / `is_degradation()` classification force you to declare intent.
//!
//! # The metric
//!
//! - conversion rate (all)        = no_llm_decisions / total_decisions
//! - DEGRADATION conversion rate  = degraded_decisions / total_decisions
//!
//! The number we drive to 0 is the *degradation* rate. The intended-opt-out rate
//! is expected to be >0 only when a user deliberately disables the sidecar.

use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

/// Every terminal outcome of a memory surfacing turn, w.r.t. the LLM judge.
///
/// EXHAUSTIVE: adding a memory surfacing path requires adding a variant here.
/// Group A = LLM judge actually ran (the productive path). Group B = no-LLM
/// (a "conversion"); each B variant declares whether it is intended or a
/// degradation via [`JudgeDecision::is_degradation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum JudgeDecision {
    // ---- Group A: LLM judge ran (NOT a conversion) ----
    /// The consensus rerank ran and at least one judge produced a usable ballot;
    /// the surfaced set is the judged result. The productive path.
    JudgeRan,

    // ---- Group B: no-LLM (a conversion to no-LLM mode) ----
    /// User explicitly disabled the sidecar (`memory_sidecar_enabled = false`).
    /// INTENDED: the only conversion that is by-design, not a degradation.
    OptedOut,
    /// Cadence gate: re-surfaced the previously judge-verified set without a
    /// fresh rerank this turn. INTENDED (still high precision; rides the last
    /// judged result), so not counted as degradation.
    CadenceCarry,
    /// Sidecar mode is on but no LLM backend is reachable (logged out / lost
    /// provider access). Memory went dormant. DEGRADATION.
    NoBackend,
    /// The consensus rerank fired but EVERY judge failed (transport error /
    /// timeout). The rerank surfaced nothing and the caller carried the last
    /// judge-verified set. DEGRADATION.
    AllJudgesFailed,
    /// The (single-judge) rerank fired but the judge response was unparseable
    /// garbage. The rerank surfaced nothing and the caller carried the last
    /// judge-verified set. DEGRADATION.
    JudgeUnparseable,
    /// The (single-judge) rerank fired but the judge transport errored. The
    /// rerank surfaced nothing and the caller carried the last judge-verified
    /// set. DEGRADATION.
    JudgeTransportError,
}

impl JudgeDecision {
    /// Stable snake_case label used in logs / dashboards.
    pub fn label(self) -> &'static str {
        match self {
            JudgeDecision::JudgeRan => "judge_ran",
            JudgeDecision::OptedOut => "opted_out",
            JudgeDecision::CadenceCarry => "cadence_carry",
            JudgeDecision::NoBackend => "no_backend",
            JudgeDecision::AllJudgesFailed => "all_judges_failed",
            JudgeDecision::JudgeUnparseable => "judge_unparseable",
            JudgeDecision::JudgeTransportError => "judge_transport_error",
        }
    }

    /// Whether this outcome surfaced/suppressed memory WITHOUT the LLM judge,
    /// i.e. a conversion to no-LLM memory mode.
    pub fn is_no_llm(self) -> bool {
        !matches!(self, JudgeDecision::JudgeRan)
    }

    /// Whether this conversion is an UNINTENDED degradation (the kind we drive to
    /// zero), as opposed to an intended no-LLM outcome (explicit opt-out or a
    /// cadence carry that rides a prior judge verdict).
    pub fn is_degradation(self) -> bool {
        matches!(
            self,
            JudgeDecision::NoBackend
                | JudgeDecision::AllJudgesFailed
                | JudgeDecision::JudgeUnparseable
                | JudgeDecision::JudgeTransportError
        )
    }

    /// All variants, for iteration in snapshots/tests. Kept in sync with the
    /// enum by the exhaustiveness test in this module.
    pub const ALL: [JudgeDecision; 7] = [
        JudgeDecision::JudgeRan,
        JudgeDecision::OptedOut,
        JudgeDecision::CadenceCarry,
        JudgeDecision::NoBackend,
        JudgeDecision::AllJudgesFailed,
        JudgeDecision::JudgeUnparseable,
        JudgeDecision::JudgeTransportError,
    ];

    /// Map a rerank's self-reported [`RerankOutcome`](crate::memory_rerank::RerankOutcome)
    /// onto the attribution variant. This is the bridge that turns the rerank's
    /// (now non-surfacing) judge failures into counted degradations.
    pub fn from_rerank_outcome(outcome: crate::memory_rerank::RerankOutcome) -> Self {
        use crate::memory_rerank::RerankOutcome;
        match outcome {
            RerankOutcome::Judged => JudgeDecision::JudgeRan,
            RerankOutcome::AllJudgesFailed => JudgeDecision::AllJudgesFailed,
            RerankOutcome::Unparseable => JudgeDecision::JudgeUnparseable,
            RerankOutcome::TransportError => JudgeDecision::JudgeTransportError,
        }
    }
}

// One atomic per variant, indexed by `decision_index`.
static COUNTS: [AtomicU64; 7] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

fn decision_index(d: JudgeDecision) -> usize {
    match d {
        JudgeDecision::JudgeRan => 0,
        JudgeDecision::OptedOut => 1,
        JudgeDecision::CadenceCarry => 2,
        JudgeDecision::NoBackend => 3,
        JudgeDecision::AllJudgesFailed => 4,
        JudgeDecision::JudgeUnparseable => 5,
        JudgeDecision::JudgeTransportError => 6,
    }
}

/// Record one memory surfacing decision. Call EXACTLY ONCE per surfacing turn,
/// at the single attribution site in the memory agent. Also writes a structured
/// line to the memory event log so conversions are attributable per session.
pub fn record(decision: JudgeDecision, session_id: &str, candidate_count: usize) {
    COUNTS[decision_index(decision)].fetch_add(1, Ordering::Relaxed);
    crate::memory_log::log_judge_decision(
        session_id,
        decision.label(),
        decision.is_no_llm(),
        decision.is_degradation(),
        candidate_count,
    );
    if decision.is_degradation() {
        // Loud, rate-limited alarm: a degradation conversion is a bug to fix.
        crate::logging::event_rate_limited(
            crate::logging::LogLevel::Warn,
            "memory_no_llm_degradation",
            std::time::Duration::from_secs(60),
            "MEMORY_NO_LLM_DEGRADATION",
            vec![
                ("session_id", session_id.to_string()),
                ("path", decision.label().to_string()),
                ("candidates", candidate_count.to_string()),
            ],
        );
    }
}

/// Per-variant count.
pub fn count(decision: JudgeDecision) -> u64 {
    COUNTS[decision_index(decision)].load(Ordering::Relaxed)
}

/// Reset all counters (test/maintenance only).
pub fn reset() {
    for c in COUNTS.iter() {
        c.store(0, Ordering::Relaxed);
    }
}

/// Aggregate snapshot of all memory-judge decisions seen so far.
#[derive(Debug, Clone, Serialize, Default)]
pub struct JudgeMetricsSnapshot {
    /// Per-variant counts keyed by stable label.
    pub by_decision: std::collections::BTreeMap<String, u64>,
    /// Total surfacing decisions recorded.
    pub total: u64,
    /// Decisions that ran the LLM judge (the productive path).
    pub judge_ran: u64,
    /// Decisions that converted to no-LLM mode (intended + degraded).
    pub no_llm_total: u64,
    /// Intended no-LLM conversions (explicit opt-out, cadence carry).
    pub no_llm_intended: u64,
    /// UNINTENDED no-LLM degradations: the number we drive to zero.
    pub no_llm_degraded: u64,
    /// no_llm_total / total, in [0, 1]. Overall conversion rate.
    pub conversion_rate: f64,
    /// no_llm_degraded / total, in [0, 1]. The headline metric to minimize.
    pub degradation_rate: f64,
}

/// Build a snapshot of the current counters.
pub fn snapshot() -> JudgeMetricsSnapshot {
    let mut by_decision = std::collections::BTreeMap::new();
    let mut total = 0u64;
    let mut judge_ran = 0u64;
    let mut no_llm_total = 0u64;
    let mut no_llm_intended = 0u64;
    let mut no_llm_degraded = 0u64;

    for d in JudgeDecision::ALL {
        let c = count(d);
        by_decision.insert(d.label().to_string(), c);
        total += c;
        if d == JudgeDecision::JudgeRan {
            judge_ran += c;
        }
        if d.is_no_llm() {
            no_llm_total += c;
            if d.is_degradation() {
                no_llm_degraded += c;
            } else {
                no_llm_intended += c;
            }
        }
    }

    let denom = total.max(1) as f64;
    JudgeMetricsSnapshot {
        by_decision,
        total,
        judge_ran,
        no_llm_total,
        no_llm_intended,
        no_llm_degraded,
        conversion_rate: no_llm_total as f64 / denom,
        degradation_rate: no_llm_degraded as f64 / denom,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_array_matches_enum_and_indices() {
        // ALL is complete and indices are unique + in range.
        assert_eq!(JudgeDecision::ALL.len(), 7);
        let mut seen = [false; 7];
        for d in JudgeDecision::ALL {
            let i = decision_index(d);
            assert!(!seen[i], "duplicate index for {:?}", d);
            seen[i] = true;
        }
        assert!(seen.iter().all(|&b| b), "every index covered");
    }

    #[test]
    fn classification_is_consistent() {
        // JudgeRan is the only non-conversion.
        assert!(!JudgeDecision::JudgeRan.is_no_llm());
        for d in JudgeDecision::ALL {
            if d != JudgeDecision::JudgeRan {
                assert!(d.is_no_llm(), "{:?} should be a conversion", d);
            }
            // Degradation implies conversion.
            if d.is_degradation() {
                assert!(d.is_no_llm());
            }
        }
        // Intended conversions are opt-out and cadence carry only.
        assert!(!JudgeDecision::OptedOut.is_degradation());
        assert!(!JudgeDecision::CadenceCarry.is_degradation());
        // The four degradations we drive to zero.
        for d in [
            JudgeDecision::NoBackend,
            JudgeDecision::AllJudgesFailed,
            JudgeDecision::JudgeUnparseable,
            JudgeDecision::JudgeTransportError,
        ] {
            assert!(d.is_degradation(), "{:?} should be a degradation", d);
        }
    }

    #[test]
    fn snapshot_computes_rates() {
        reset();
        record(JudgeDecision::JudgeRan, "s", 5);
        record(JudgeDecision::JudgeRan, "s", 5);
        record(JudgeDecision::OptedOut, "s", 3); // intended conversion
        record(JudgeDecision::NoBackend, "s", 4); // degradation
        let snap = snapshot();
        assert_eq!(snap.total, 4);
        assert_eq!(snap.judge_ran, 2);
        assert_eq!(snap.no_llm_total, 2);
        assert_eq!(snap.no_llm_intended, 1);
        assert_eq!(snap.no_llm_degraded, 1);
        assert!((snap.conversion_rate - 0.5).abs() < 1e-9);
        assert!((snap.degradation_rate - 0.25).abs() < 1e-9);
        reset();
    }
}
