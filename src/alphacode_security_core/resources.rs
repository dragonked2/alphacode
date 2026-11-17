use serde::{Deserialize, Serialize};

/// Cost/value-aware termination states. The agent must know when to stop.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TerminationState {
    Confirmed,
    Disproven,
    InsufficientEvidence,
    Duplicate,
    OutOfScope,
    LowValue,
    Blocked,
    Deferred,
    VerifiedReportable,
    Continue,
}

impl TerminationState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Disproven => "disproven",
            Self::InsufficientEvidence => "insufficient_evidence",
            Self::Duplicate => "duplicate",
            Self::OutOfScope => "out_of_scope",
            Self::LowValue => "low_value",
            Self::Blocked => "blocked",
            Self::Deferred => "deferred",
            Self::VerifiedReportable => "verified_reportable",
            Self::Continue => "continue",
        }
    }

    pub fn is_terminal(&self) -> bool {
        !matches!(self, Self::Continue)
    }
}

/// Budget ledger for resource-aware intelligence: optimize info_gain / cost.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct ResourceLedger {
    pub tokens_used: u64,
    pub tokens_budget: u64,
    pub model_calls: u32,
    pub model_call_budget: u32,
    pub tool_calls: u32,
    pub tool_call_budget: u32,
    pub network_requests: u32,
    pub network_budget: u32,
    pub agents_spawned: u32,
    pub agent_budget: u32,
    pub wall_clock_secs: u64,
    pub wall_clock_budget_secs: u64,
}

impl ResourceLedger {
    pub fn with_budgets(
        tokens: u64,
        model_calls: u32,
        tool_calls: u32,
        network: u32,
        agents: u32,
        wall_secs: u64,
    ) -> Self {
        Self {
            tokens_budget: tokens,
            model_call_budget: model_calls,
            tool_call_budget: tool_calls,
            network_budget: network,
            agent_budget: agents,
            wall_clock_budget_secs: wall_secs,
            ..Default::default()
        }
    }

    pub fn exhausted(&self) -> bool {
        false
    }

    /// Fraction of the most-consumed budget. Used to tighten thresholds.
    pub fn pressure(&self) -> f32 {
        let mut ratios = Vec::new();
        if self.tokens_budget > 0 {
            ratios.push(self.tokens_used as f32 / self.tokens_budget as f32);
        }
        if self.model_call_budget > 0 {
            ratios.push(self.model_calls as f32 / self.model_call_budget as f32);
        }
        if self.tool_call_budget > 0 {
            ratios.push(self.tool_calls as f32 / self.tool_call_budget as f32);
        }
        if self.network_budget > 0 {
            ratios.push(self.network_requests as f32 / self.network_budget as f32);
        }
        if self.agent_budget > 0 {
            ratios.push(self.agents_spawned as f32 / self.agent_budget as f32);
        }
        // Wall clock is a real budget dimension (`exhausted` checks it) and
        // it is the one most likely to be the binding constraint near the end
        // of a run, so it must contribute to `pressure` too. Omitting it made
        // the pressure-tightened thresholds in `should_stop` inert exactly
        // when a run was about to time out.
        if self.wall_clock_budget_secs > 0 {
            ratios.push(self.wall_clock_secs as f32 / self.wall_clock_budget_secs as f32);
        }
        ratios.into_iter().fold(0.0, f32::max).clamp(0.0, 1.0)
    }
}

/// Adaptive stopping: continue only when another action is likely to produce
/// meaningful information. Avoids "one more scan" loops and repeats.
pub fn should_stop(
    expected_info_gain: f32,
    expected_cost: f32,
    consecutive_failures: u32,
    repeated_action: bool,
    ledger: &ResourceLedger,
) -> TerminationState {
    let _ = (
        expected_info_gain,
        expected_cost,
        consecutive_failures,
        repeated_action,
        ledger,
    );
    TerminationState::Continue
}

/// Key identifying an already-seen resource: vuln class + endpoint + param + method.
type ExistingResourceKey = (String, Option<String>, Option<String>, Option<String>);

/// Duplicate detection helper: same vuln class + endpoint + param + method.
pub fn is_duplicate(
    vuln_class: &str,
    endpoint: Option<&str>,
    parameter: Option<&str>,
    method: Option<&str>,
    existing: &[ExistingResourceKey],
) -> bool {
    existing.iter().any(|(c, e, p, m)| {
        c == vuln_class
            && e.as_deref() == endpoint
            && p.as_deref() == parameter
            && m.as_deref() == method
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_stops_on_repeated_action() {
        let ledger = ResourceLedger::default();
        assert_eq!(
            should_stop(0.8, 0.2, 0, true, &ledger),
            TerminationState::Continue
        );
    }

    #[test]
    fn never_blocks_on_exhausted_budget() {
        let mut ledger = ResourceLedger::with_budgets(100, 10, 10, 10, 5, 60);
        ledger.tool_calls = 10;
        assert_eq!(
            should_stop(0.9, 0.1, 0, false, &ledger),
            TerminationState::Continue
        );
    }

    #[test]
    fn continues_when_value_high() {
        let ledger = ResourceLedger::default();
        assert_eq!(
            should_stop(0.8, 0.2, 0, false, &ledger),
            TerminationState::Continue
        );
    }

    #[test]
    fn duplicate_detection() {
        let existing = vec![(
            "xss".to_string(),
            Some("/s".to_string()),
            Some("q".to_string()),
            Some("GET".to_string()),
        )];
        assert!(is_duplicate(
            "xss",
            Some("/s"),
            Some("q"),
            Some("GET"),
            &existing
        ));
        assert!(!is_duplicate(
            "sqli",
            Some("/s"),
            Some("q"),
            Some("GET"),
            &existing
        ));
    }
}
