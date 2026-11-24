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

    /// Whether any configured budget dimension is used up.
    ///
    /// A zero budget means "unconfigured", not "zero allowed": a caller that
    /// never set a dimension must not have every action blocked by it.
    /// `>=` rather than `>` is deliberate — the ledger records completed work,
    /// so reaching exactly the budget leaves nothing for another action.
    pub fn exhausted(&self) -> bool {
        self.token_budget_exhausted()
            || self.model_call_budget_exhausted()
            || self.tool_call_budget_exhausted()
            || self.network_budget_exhausted()
            || self.agent_budget_exhausted()
            || self.wall_clock_budget_exhausted()
    }

    fn token_budget_exhausted(&self) -> bool {
        self.tokens_budget > 0 && self.tokens_used >= self.tokens_budget
    }

    fn model_call_budget_exhausted(&self) -> bool {
        self.model_call_budget > 0 && self.model_calls >= self.model_call_budget
    }

    fn tool_call_budget_exhausted(&self) -> bool {
        self.tool_call_budget > 0 && self.tool_calls >= self.tool_call_budget
    }

    fn network_budget_exhausted(&self) -> bool {
        self.network_budget > 0 && self.network_requests >= self.network_budget
    }

    fn agent_budget_exhausted(&self) -> bool {
        self.agent_budget > 0 && self.agents_spawned >= self.agent_budget
    }

    fn wall_clock_budget_exhausted(&self) -> bool {
        self.wall_clock_budget_secs > 0 && self.wall_clock_secs >= self.wall_clock_budget_secs
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

/// How many consecutive failures mean the current approach is not working.
///
/// Three is the smallest count where "it failed twice, but the third try might
/// differ" is no longer a reasonable hope: the caller has had two chances to
/// change the input and the outcome did not move.
pub const FAILURE_STREAK_LIMIT: u32 = 3;

/// Below this expected information gain another action is not worth its cost.
pub const MIN_USEFUL_INFO_GAIN: f32 = 0.05;

/// Cost at which value has to be genuinely high to justify another action.
pub const HIGH_COST: f32 = 1.0;

/// Adaptive stopping: continue only when another action is likely to produce
/// meaningful information. Avoids "one more scan" loops and repeats.
///
/// Order matters. Exhausted resources outrank everything: when the budget is
/// gone, the honest answer is that the remaining coverage was not completed,
/// which is exactly what [`TerminationState::Blocked`] records. A repeated
/// action next, because repeating something that already produced its answer
/// cannot produce a new one. Then the failure streak, then a value-versus-cost
/// decision.
pub fn should_stop(
    expected_info_gain: f32,
    expected_cost: f32,
    consecutive_failures: u32,
    repeated_action: bool,
    ledger: &ResourceLedger,
) -> TerminationState {
    if ledger.exhausted() {
        return TerminationState::Blocked;
    }

    // Repeating an action that already ran cannot add information, no matter
    // how promising the estimate claims to be.
    if repeated_action {
        return TerminationState::LowValue;
    }

    if consecutive_failures >= FAILURE_STREAK_LIMIT {
        return TerminationState::Blocked;
    }

    // A budget dimension in its last quarter raises the bar for spending more
    // of it. `pressure` is the most-consumed configured dimension, so this only
    // fires when the run genuinely is near a limit.
    let pressure = ledger.pressure();
    let gain_floor = MIN_USEFUL_INFO_GAIN * (1.0 + pressure);

    if expected_info_gain < gain_floor {
        return TerminationState::LowValue;
    }

    // Value must beat cost, not merely exist. Without this an action that costs
    // more than it can return still keeps the loop alive forever.
    if expected_cost > 0.0 && expected_info_gain < expected_cost {
        return TerminationState::LowValue;
    }

    // Under heavy pressure, only a clearly worthwhile action continues.
    if pressure >= 0.75 && expected_cost >= HIGH_COST && expected_info_gain < expected_cost * 2.0 {
        return TerminationState::LowValue;
    }

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

    fn ledger_with_room() -> ResourceLedger {
        ResourceLedger::with_budgets(1_000, 100, 100, 100, 10, 3_600)
    }

    #[test]
    fn exhausted_is_false_for_an_unconfigured_ledger() {
        // A default ledger sets no budgets. "Unconfigured" must not be read as
        // "zero remaining", or every fresh run would stop immediately.
        assert!(!ResourceLedger::default().exhausted());
    }

    #[test]
    fn exhausted_tracks_every_budget_dimension() {
        let base = ResourceLedger::with_budgets(100, 10, 10, 10, 5, 60);

        let mut tokens = base.clone();
        tokens.tokens_used = 100;
        assert!(tokens.exhausted(), "token budget");

        let mut model = base.clone();
        model.model_calls = 10;
        assert!(model.exhausted(), "model-call budget");

        let mut tools = base.clone();
        tools.tool_calls = 10;
        assert!(tools.exhausted(), "tool-call budget");

        let mut network = base.clone();
        network.network_requests = 10;
        assert!(network.exhausted(), "network budget");

        let mut agents = base.clone();
        agents.agents_spawned = 5;
        assert!(agents.exhausted(), "agent budget");

        let mut wall = base.clone();
        wall.wall_clock_secs = 60;
        assert!(wall.exhausted(), "wall-clock budget");
    }

    #[test]
    fn exhausted_is_false_while_budget_remains() {
        let mut ledger = ledger_with_room();
        ledger.tool_calls = 99;
        assert!(
            !ledger.exhausted(),
            "one call remaining must not read as exhausted"
        );
        ledger.tool_calls = 100;
        assert!(ledger.exhausted(), "reaching the budget is exhausted");
    }

    #[test]
    fn stopping_blocks_when_the_budget_is_gone() {
        let mut ledger = ledger_with_room();
        ledger.tool_calls = 100;
        assert_eq!(
            should_stop(0.9, 0.1, 0, false, &ledger),
            // Remaining coverage cannot be completed, and saying so is more
            // honest than reporting a clean surface.
            TerminationState::Blocked
        );
    }

    #[test]
    fn stopping_rejects_a_repeated_action() {
        let ledger = ledger_with_room();
        assert_eq!(
            should_stop(0.8, 0.2, 0, true, &ledger),
            TerminationState::LowValue
        );
    }

    #[test]
    fn stopping_blocks_after_a_failure_streak() {
        let ledger = ledger_with_room();
        assert_eq!(
            should_stop(0.9, 0.1, FAILURE_STREAK_LIMIT, false, &ledger),
            TerminationState::Blocked
        );
        assert_eq!(
            should_stop(0.9, 0.1, FAILURE_STREAK_LIMIT - 1, false, &ledger),
            TerminationState::Continue,
            "one short of the streak limit still permits another attempt"
        );
    }

    #[test]
    fn stopping_continues_when_value_clearly_exceeds_cost() {
        let ledger = ledger_with_room();
        assert_eq!(
            should_stop(0.8, 0.2, 0, false, &ledger),
            TerminationState::Continue
        );
    }

    #[test]
    fn stopping_rejects_an_action_that_costs_more_than_it_returns() {
        let ledger = ledger_with_room();
        assert_eq!(
            should_stop(0.1, 0.9, 0, false, &ledger),
            TerminationState::LowValue
        );
    }

    #[test]
    fn stopping_rejects_near_zero_gain() {
        let ledger = ledger_with_room();
        assert_eq!(
            should_stop(0.0, 0.0, 0, false, &ledger),
            TerminationState::LowValue
        );
    }

    #[test]
    fn near_a_limit_the_gain_floor_rises() {
        // 90% of the tool budget is spent, so cheap low-value probes stop.
        let mut ledger = ledger_with_room();
        ledger.tool_calls = 90;

        assert_eq!(
            should_stop(0.06, 0.01, 0, false, &ledger),
            TerminationState::LowValue,
            "a marginal gain is not worth spending the last tenth of the budget"
        );
        assert_eq!(
            should_stop(0.5, 0.1, 0, false, &ledger),
            TerminationState::Continue,
            "a substantial gain still justifies the remaining budget"
        );
    }

    #[test]
    fn is_duplicate_matches_every_identity_field() {
        let existing = vec![(
            "sqli".to_string(),
            Some("/search".to_string()),
            Some("q".to_string()),
            Some("GET".to_string()),
        )];
        assert!(is_duplicate(
            "sqli",
            Some("/search"),
            Some("q"),
            Some("GET"),
            &existing
        ));
        assert!(!is_duplicate(
            "xss",
            Some("/search"),
            Some("q"),
            Some("GET"),
            &existing
        ));
        assert!(!is_duplicate(
            "sqli",
            Some("/search"),
            Some("page"),
            Some("GET"),
            &existing
        ));
    }
}
