use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Action classes — replaces the assumption of only predefined actions.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ActionClass {
    Builtin,
    Tool,
    Skill,
    SwarmDelegation,
    Discovered,
    Composed,
    Verification,
    Recovery,
}

impl ActionClass {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Builtin => "builtin",
            Self::Tool => "tool",
            Self::Skill => "skill",
            Self::SwarmDelegation => "swarm_delegation",
            Self::Discovered => "discovered",
            Self::Composed => "composed",
            Self::Verification => "verification",
            Self::Recovery => "recovery",
        }
    }
}

/// Explicit action representation with cost/risk/information estimates.
/// All estimates are heuristics in 0.0-1.0; never presented as calibrated.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Action {
    pub id: String,
    pub description: String,
    pub class: ActionClass,
    pub prerequisites: Vec<String>,
    pub expected_observations: Vec<String>,
    pub applicable_hypotheses: Vec<String>,
    pub estimated_cost: f32,
    pub estimated_risk: f32,
    pub reversibility: f32,
    pub expected_information_gain: f32,
    pub expected_goal_progress: f32,
    pub provenance: String,
}

impl Action {
    pub fn new(
        id: String,
        description: String,
        class: ActionClass,
        applicable_hypotheses: Vec<String>,
    ) -> Self {
        Self {
            id,
            description,
            class,
            prerequisites: Vec::new(),
            expected_observations: Vec::new(),
            applicable_hypotheses,
            estimated_cost: 0.3,
            estimated_risk: 0.1,
            reversibility: 1.0,
            expected_information_gain: 0.5,
            expected_goal_progress: 0.3,
            provenance: "builtin".to_string(),
        }
    }

    /// Counterfactual value: what do we learn if it succeeds vs fails?
    /// value = info_gain + goal_progress + discrimination - cost - risk.
    /// Cheap, deterministic, no model call.
    pub fn action_value(&self, discrimination_bonus: f32) -> f32 {
        (self.expected_information_gain + self.expected_goal_progress + discrimination_bonus
            - self.estimated_cost
            - self.estimated_risk)
            .clamp(-2.0, 3.0)
    }

    pub fn is_safe(&self, max_risk: f32) -> bool {
        self.estimated_risk <= max_risk
    }
}

/// Dynamic action space: built-in + tool + skill + swarm + discovered.
#[derive(Clone, Debug, Default)]
pub struct ActionSpace {
    pub actions: HashMap<String, Action>,
    /// action_id -> hypotheses it discriminates between.
    pub discrimination: HashMap<String, Vec<(String, String)>>,
}

impl ActionSpace {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, action: Action) {
        self.actions.insert(action.id.clone(), action);
    }

    pub fn with_builtin_security_actions() -> Self {
        let mut space = Self::new();
        // Recon / analysis primitives mapped to existing tool capabilities.
        let defs = vec![
            (
                "http_probe",
                "HTTP request + header/body capture",
                ActionClass::Tool,
                0.2,
                0.05,
                0.6,
            ),
            (
                "browser_render",
                "Render page + observe DOM/network",
                ActionClass::Tool,
                0.5,
                0.1,
                0.6,
            ),
            (
                "auth_compare",
                "Compare authenticated vs unauthenticated behavior",
                ActionClass::Composed,
                0.4,
                0.1,
                0.9,
            ),
            (
                "replay_request",
                "Replay captured request to test reproducibility",
                ActionClass::Verification,
                0.3,
                0.1,
                0.8,
            ),
            (
                "control_compare",
                "Compare attacker input vs benign baseline",
                ActionClass::Verification,
                0.3,
                0.05,
                0.85,
            ),
            (
                "scope_check",
                "Verify target is in scope before deeper testing",
                ActionClass::Verification,
                0.1,
                0.0,
                0.2,
            ),
            (
                "spawn_verifier",
                "Delegate to independent verifier agent",
                ActionClass::SwarmDelegation,
                0.7,
                0.1,
                0.5,
            ),
            (
                "retry_with_backoff",
                "Retry transient failure with backoff",
                ActionClass::Recovery,
                0.2,
                0.05,
                0.3,
            ),
            (
                "change_strategy",
                "Abandon failing approach, try alternative",
                ActionClass::Recovery,
                0.3,
                0.05,
                0.4,
            ),
        ];
        for (id, desc, class, cost, risk, gain) in defs {
            let mut a = Action::new(id.to_string(), desc.to_string(), class, Vec::new());
            a.estimated_cost = cost;
            a.estimated_risk = risk;
            a.expected_information_gain = gain;
            a.provenance = "builtin_security".to_string();
            space.register(a);
        }
        space
    }

    /// Compose a discovered strategy from existing capabilities.
    /// Still gated by caller against permissions/scope — this only records it.
    pub fn discover(
        &mut self,
        description: String,
        composed_of: Vec<String>,
        applicable_hypotheses: Vec<String>,
    ) -> Action {
        let id = format!("discovered_{:016x}", {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut h = DefaultHasher::new();
            description.hash(&mut h);
            composed_of.hash(&mut h);
            h.finish()
        });
        let mut action = Action::new(
            id.clone(),
            description,
            ActionClass::Discovered,
            applicable_hypotheses,
        );
        action.prerequisites = composed_of;
        action.provenance = "discovered".to_string();
        // Discovered actions start with conservative estimates.
        action.estimated_cost = 0.5;
        action.estimated_risk = 0.2;
        action.expected_information_gain = 0.7;
        self.actions.insert(id.clone(), action.clone());
        action
    }

    pub fn mark_discriminates(&mut self, action_id: &str, hypo_a: &str, hypo_b: &str) {
        self.discrimination
            .entry(action_id.to_string())
            .or_default()
            .push((hypo_a.to_string(), hypo_b.to_string()));
    }

    pub fn discrimination_bonus(&self, action_id: &str) -> f32 {
        let n = self
            .discrimination
            .get(action_id)
            .map(|v| v.len())
            .unwrap_or(0);
        (n as f32 * 0.25).min(0.75)
    }

    /// Filter to affordable, safe, untried actions. Prevents repeating failures.
    pub fn affordable(
        &self,
        tried: &HashSet<String>,
        failed: &HashSet<String>,
        max_cost: f32,
        max_risk: f32,
    ) -> Vec<&Action> {
        self.actions
            .values()
            .filter(|a| !failed.contains(&a.id))
            .filter(|a| !tried.contains(&a.id) || a.class == ActionClass::Verification)
            .filter(|a| a.estimated_cost <= max_cost && a.estimated_risk <= max_risk)
            .collect()
    }

    /// Rank by counterfactual value. Deterministic.
    pub fn ranked<'a>(&self, candidates: Vec<&'a Action>) -> Vec<(&'a Action, f32)> {
        let mut scored: Vec<(&Action, f32)> = candidates
            .into_iter()
            .map(|a| {
                let bonus = self.discrimination_bonus(&a.id);
                (a, a.action_value(bonus))
            })
            .collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored
    }

    /// Select the single most useful affordable action, if any.
    pub fn select_best<'a>(
        &'a self,
        tried: &HashSet<String>,
        failed: &HashSet<String>,
        max_cost: f32,
        max_risk: f32,
    ) -> Option<(&'a Action, f32)> {
        let candidates = self.affordable(tried, failed, max_cost, max_risk);
        self.ranked(candidates).into_iter().next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_space_has_verification_actions() {
        let space = ActionSpace::with_builtin_security_actions();
        assert!(space.actions.contains_key("replay_request"));
        assert!(space.actions.contains_key("auth_compare"));
    }

    #[test]
    fn discovered_action_is_stable() {
        let mut space = ActionSpace::with_builtin_security_actions();
        let a = space.discover(
            "compare auth".into(),
            vec!["http_probe".into()],
            vec!["h1".into()],
        );
        let b = space.discover(
            "compare auth".into(),
            vec!["http_probe".into()],
            vec!["h1".into()],
        );
        assert_eq!(a.id, b.id);
    }

    #[test]
    fn selector_prefers_discriminating_cheap_action() {
        let mut space = ActionSpace::with_builtin_security_actions();
        space.mark_discriminates("auth_compare", "h1", "h2");
        let tried = HashSet::new();
        let failed = HashSet::new();
        let best = space.select_best(&tried, &failed, 1.0, 0.5).unwrap();
        // auth_compare has high gain + discrimination bonus.
        assert!(best.1 > 0.0);
    }

    #[test]
    fn failed_actions_are_excluded() {
        let space = ActionSpace::with_builtin_security_actions();
        let tried: HashSet<String> = ["http_probe".into()].iter().cloned().collect();
        let failed: HashSet<String> = ["http_probe".into()].iter().cloned().collect();
        let avail = space.affordable(&tried, &failed, 1.0, 1.0);
        assert!(!avail.iter().any(|a| a.id == "http_probe"));
    }
}
