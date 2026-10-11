//! Evidence-based, configurable prioritization for multi-repository reviews.
//!
//! A review that spreads attention evenly across repositories of very
//! different security significance under-scores the ones that matter. This
//! module scores components by *evidence-based risk factors* — key custody,
//! signing, consensus, network exposure, untrusted-input processing, and so
//! on — and returns an ordered plan where every ordering decision carries a
//! human-readable explanation.
//!
//! Two rules are baked in:
//!
//! * **Prioritization is ordering, never suppression.** Every component stays
//!   in the plan; a low-ranked component is reviewed later, not skipped.
//!   [`PrioritizedPlan::all_components`] always contains everything given to
//!   the planner.
//! * **The order is a function of evidence, not names.** Weights are
//!   configurable ([`PrioritizerConfig`]) and factors are attached per
//!   component by whatever the caller actually observed, so a different task
//!   or program scope produces a different, explainable order. The
//!   arc-remote-signer/malachite/arc-node ordering from the motivating
//!   session appears only as [`PrioritizerConfig::example_session_config`],
//!   a documented example, never as a hardcoded default.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A risk factor that raises a component's review priority, with the
/// evidence that attached it.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskFactor {
    /// Holds or processes private keys / key material.
    KeyCustody,
    /// Performs signing or other cryptographic operations.
    SigningOperations,
    /// Privilege or trust boundary crossing (authn/authz decisions).
    PrivilegeBoundary,
    /// Authentication and authorization logic.
    AuthnAuthz,
    /// Cryptographic primitives beyond ordinary signing (randomness, KDFs).
    Cryptography,
    /// Consensus or state-transition logic.
    Consensus,
    /// Functionality exposed to networks (servers, RPC, p2p).
    NetworkExposed,
    /// Asset movement or transaction execution.
    AssetMovement,
    /// Processes untrusted input (user data, external payloads).
    UntrustedInput,
    /// Contains existing findings — re-review with those in mind.
    ExistingFindings,
    /// Recently changed code — churn correlates with defects.
    RecentChanges,
    /// Reachable call paths from the attack surface.
    ReachableCallPath,
}

impl RiskFactor {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::KeyCustody => "key_custody",
            Self::SigningOperations => "signing_operations",
            Self::PrivilegeBoundary => "privilege_boundary",
            Self::AuthnAuthz => "authn_authz",
            Self::Cryptography => "cryptography",
            Self::Consensus => "consensus",
            Self::NetworkExposed => "network_exposed",
            Self::AssetMovement => "asset_movement",
            Self::UntrustedInput => "untrusted_input",
            Self::ExistingFindings => "existing_findings",
            Self::RecentChanges => "recent_changes",
            Self::ReachableCallPath => "reachable_call_path",
        }
    }
}

/// A component (repository or sub-module) to prioritize.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrioritizationTarget {
    /// Stable identifier, e.g. "arc-remote-signer/signing".
    pub id: String,
    /// Repository the component belongs to.
    pub repository: String,
    /// Risk factors with the concrete evidence that attaches each one.
    pub factors: Vec<RiskFactorEvidence>,
    /// Out-of-scope components are ranked last and flagged; they are never
    /// silently dropped and never actively tested.
    pub in_scope: bool,
}

/// A risk factor plus why it was attached.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RiskFactorEvidence {
    pub factor: RiskFactor,
    /// Free-text evidence, e.g. "Service.Sign() acquires loadedKeyMu.RLock".
    pub evidence: String,
}

/// Configurable weights per factor. Default weights encode the general
/// heuristic "custody and consensus outrank convenience"; override per task.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrioritizerConfig {
    pub weights: BTreeMap<RiskFactor, f64>,
    /// User-ordered overrides: component ids listed here sort first in this
    /// exact order, ahead of scoring. Lets an operator say "look at X first"
    /// regardless of heuristics — and the plan still explains why.
    pub user_order: Vec<String>,
    /// Review-depth hint per component id: "shallow" | "standard" | "deep".
    pub depth_overrides: BTreeMap<String, String>,
}

impl Default for PrioritizerConfig {
    fn default() -> Self {
        let mut weights = BTreeMap::new();
        // Custody and signing first — key material loss is usually the worst
        // outcome in the classes of systems this planner targets.
        weights.insert(RiskFactor::KeyCustody, 10.0);
        weights.insert(RiskFactor::SigningOperations, 9.0);
        weights.insert(RiskFactor::PrivilegeBoundary, 8.0);
        weights.insert(RiskFactor::Cryptography, 7.0);
        weights.insert(RiskFactor::Consensus, 7.0);
        weights.insert(RiskFactor::AuthnAuthz, 6.0);
        weights.insert(RiskFactor::AssetMovement, 6.0);
        weights.insert(RiskFactor::NetworkExposed, 5.0);
        weights.insert(RiskFactor::UntrustedInput, 5.0);
        weights.insert(RiskFactor::ReachableCallPath, 4.0);
        weights.insert(RiskFactor::ExistingFindings, 3.0);
        weights.insert(RiskFactor::RecentChanges, 2.0);
        Self {
            weights,
            user_order: Vec::new(),
            depth_overrides: BTreeMap::new(),
        }
    }
}

impl PrioritizerConfig {
    /// The example from the motivating session, kept as a *documented example*
    /// rather than a default: signer key lifecycle > consensus > EVM/tx.
    pub fn example_session_config() -> Self {
        Self {
            user_order: vec![
                "arc-remote-signer".to_string(),
                "malachite".to_string(),
                "arc-node".to_string(),
            ],
            ..Self::default()
        }
    }
}

/// One ranked component with the reasoning behind its place.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrioritizedComponent {
    pub target: PrioritizationTarget,
    pub score: f64,
    /// Human-readable reasons, ordered by contribution.
    pub rationale: Vec<String>,
    /// Suggested review depth.
    pub depth: ReviewDepthHint,
    /// Why this position specifically — includes ordering causes like
    /// "user override" and scope flags.
    pub ordering_reason: String,
}

/// Suggested depth of review for a component.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDepthHint {
    /// Look-around only; note suspicious spots for later.
    Shallow,
    /// Standard systematic review with the concurrency/ security checklists.
    Standard,
    /// Deep review: full call-path tracing, reproduction harness available,
    /// baseline comparisons expected.
    Deep,
}

impl ReviewDepthHint {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Shallow => "shallow",
            Self::Standard => "standard",
            Self::Deep => "deep",
        }
    }
}

/// The prioritized plan.
///
/// Every target given to the planner appears exactly once. `order` is the
/// recommended review sequence; `suppressed` is always empty by design and
/// exists as a guard for future changes: prioritization orders work, it does
/// not remove it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PrioritizedPlan {
    pub order: Vec<PrioritizedComponent>,
    /// Components that must not be actively tested (out of scope).
    pub do_not_test: Vec<String>,
}

impl PrioritizedPlan {
    /// Every component, in review order.
    pub fn all_components(&self) -> impl Iterator<Item = &PrioritizedComponent> {
        self.order.iter()
    }

    pub fn is_in_scope(&self, id: &str) -> bool {
        !self.do_not_test.iter().any(|x| x == id)
    }
}

/// Score and order components by weighted risk factors.
///
/// The plan explains itself: each component carries per-factor rationale and
/// an ordering reason, and user overrides are recorded as such rather than
/// being disguised as heuristic wins.
pub fn prioritize(
    targets: Vec<PrioritizationTarget>,
    config: &PrioritizerConfig,
) -> PrioritizedPlan {
    let mut scored: Vec<PrioritizedComponent> = targets
        .into_iter()
        .map(|target| {
            let mut rationale: Vec<(f64, String)> = Vec::new();
            let mut score = 0.0;
            for factor in &target.factors {
                let weight = config.weights.get(&factor.factor).copied().unwrap_or(1.0);
                score += weight;
                rationale.push((
                    weight,
                    format!(
                        "[{}] {} (weight {weight})",
                        factor.factor.as_str(),
                        factor.evidence
                    ),
                ));
            }
            if !target.in_scope {
                // Out-of-scope components are ranked below everything in
                // scope but kept in the plan.
                score -= 1_000.0;
            }
            rationale.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
            let depth = config
                .depth_overrides
                .get(&target.id)
                .and_then(|d| match d.as_str() {
                    "shallow" => Some(ReviewDepthHint::Shallow),
                    "deep" => Some(ReviewDepthHint::Deep),
                    "standard" => Some(ReviewDepthHint::Standard),
                    _ => None,
                })
                .unwrap_or({
                    if !target.in_scope {
                        ReviewDepthHint::Shallow
                    } else if score >= 15.0 {
                        ReviewDepthHint::Deep
                    } else if score >= 6.0 {
                        ReviewDepthHint::Standard
                    } else {
                        ReviewDepthHint::Shallow
                    }
                });
            let ordering_reason = if !target.in_scope {
                "out of scope: ranked last and excluded from active testing".to_string()
            } else {
                format!("weighted risk score {score:.1}")
            };
            PrioritizedComponent {
                target,
                score,
                rationale: rationale.into_iter().map(|(_, why)| why).collect(),
                depth,
                ordering_reason,
            }
        })
        .collect();

    // Stable scoring order (descending), then apply user overrides by
    // pulling named components to the front, preserving the user's exact
    // relative order.
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.target.id.cmp(&b.target.id))
    });

    if !config.user_order.is_empty() {
        let mut front: Vec<(usize, PrioritizedComponent)> = Vec::new();
        let mut rest: Vec<PrioritizedComponent> = Vec::new();
        for component in scored {
            let position = config
                .user_order
                .iter()
                .position(|id| *id == component.target.id);
            match position {
                Some(idx) => {
                    let mut component = component;
                    component.ordering_reason = format!(
                        "user override: explicitly ordered #{} (weighted score was {:.1})",
                        idx + 1,
                        component.score
                    );
                    front.push((idx, component));
                }
                None => rest.push(component),
            }
        }
        front.sort_by_key(|(idx, _)| *idx);
        scored = front.into_iter().map(|(_, c)| c).chain(rest).collect();
    }

    let do_not_test = scored
        .iter()
        .filter(|c| !c.target.in_scope)
        .map(|c| c.target.id.clone())
        .collect();

    PrioritizedPlan {
        order: scored,
        do_not_test,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(
        id: &str,
        repo: &str,
        factors: &[RiskFactor],
        in_scope: bool,
    ) -> PrioritizationTarget {
        PrioritizationTarget {
            id: id.to_string(),
            repository: repo.to_string(),
            factors: factors
                .iter()
                .map(|f| RiskFactorEvidence {
                    factor: f.clone(),
                    evidence: format!("evidence for {}", f.as_str()),
                })
                .collect(),
            in_scope,
        }
    }

    /// The motivating session's shape: key custody outranks consensus
    /// outranks general execution — because of the *factors*, not the names.
    #[test]
    fn key_custody_outranks_consensus_outranks_execution() {
        let plan = prioritize(
            vec![
                target(
                    "arc-node/evm",
                    "arc-node",
                    &[RiskFactor::NetworkExposed, RiskFactor::UntrustedInput],
                    true,
                ),
                target(
                    "malachite/consensus",
                    "malachite",
                    &[RiskFactor::Consensus, RiskFactor::NetworkExposed],
                    true,
                ),
                target(
                    "arc-remote-signer/signing",
                    "arc-remote-signer",
                    &[
                        RiskFactor::KeyCustody,
                        RiskFactor::SigningOperations,
                        RiskFactor::Cryptography,
                    ],
                    true,
                ),
            ],
            &PrioritizerConfig::default(),
        );
        let ids: Vec<&str> = plan.order.iter().map(|c| c.target.id.as_str()).collect();
        assert_eq!(
            ids,
            vec![
                "arc-remote-signer/signing",
                "malachite/consensus",
                "arc-node/evm"
            ],
            "{ids:?}"
        );
        // And every component remains in the plan.
        assert_eq!(plan.order.len(), 3);
    }

    /// The same components with different evidence produce a different order
    /// — the planner follows evidence, not hardcoded names.
    #[test]
    fn order_follows_evidence_not_names() {
        let plan = prioritize(
            vec![
                target("a", "repo-a", &[RiskFactor::RecentChanges], true),
                target("b", "repo-b", &[RiskFactor::KeyCustody], true),
            ],
            &PrioritizerConfig::default(),
        );
        assert_eq!(plan.order[0].target.id, "b");
    }

    /// User-defined priorities are respected and *labeled* as overrides.
    #[test]
    fn user_order_is_respected_and_explained() {
        let mut config = PrioritizerConfig::default();
        config.user_order = vec!["low-scored".into()];
        let plan = prioritize(
            vec![
                target("high-scored", "r", &[RiskFactor::KeyCustody], true),
                target("low-scored", "r", &[RiskFactor::RecentChanges], true),
            ],
            &config,
        );
        assert_eq!(plan.order[0].target.id, "low-scored");
        assert!(plan.order[0].ordering_reason.contains("user override"));
    }

    /// Out-of-scope components are kept in the plan, ranked last, and
    /// excluded from testing — never silently dropped.
    #[test]
    fn out_of_scope_components_are_kept_but_not_tested() {
        let plan = prioritize(
            vec![
                target("in-scope", "r", &[RiskFactor::UntrustedInput], true),
                target("out-of-scope", "r", &[RiskFactor::KeyCustody], false),
            ],
            &PrioritizerConfig::default(),
        );
        assert_eq!(plan.order.len(), 2, "out-of-scope must not be dropped");
        assert_eq!(plan.order.last().unwrap().target.id, "out-of-scope");
        assert!(!plan.is_in_scope("out-of-scope"));
        assert!(plan.is_in_scope("in-scope"));
        assert_eq!(plan.do_not_test, vec!["out-of-scope".to_string()]);
        // Depth for out-of-scope defaults to shallow.
        assert_eq!(plan.order.last().unwrap().depth, ReviewDepthHint::Shallow);
    }

    /// Lower-priority components are still present and reviewable —
    /// prioritization must never suppress findings-producing work entirely.
    #[test]
    fn lower_priority_components_are_not_suppressed() {
        let plan = prioritize(
            vec![
                target("crit", "r", &[RiskFactor::KeyCustody], true),
                target("minor", "r", &[RiskFactor::RecentChanges], true),
            ],
            &PrioritizerConfig::default(),
        );
        assert!(plan.all_components().any(|c| c.target.id == "minor"));
        assert_eq!(
            plan.order.last().unwrap().depth,
            ReviewDepthHint::Shallow,
            "minor components get less depth, not zero"
        );
    }

    /// Every ordering decision is explainable from the plan alone.
    #[test]
    fn every_component_carries_a_rationale() {
        let plan = prioritize(
            vec![target("x", "r", &[RiskFactor::Consensus], true)],
            &PrioritizerConfig::default(),
        );
        let component = &plan.order[0];
        assert!(!component.rationale.is_empty());
        assert!(component.rationale[0].contains("consensus"));
        assert!(!component.ordering_reason.is_empty());
    }

    /// Depth hints can be overridden by configuration.
    #[test]
    fn depth_overrides_are_respected() {
        let mut config = PrioritizerConfig::default();
        config.depth_overrides.insert("x".into(), "deep".into());
        let plan = prioritize(
            vec![target("x", "r", &[RiskFactor::RecentChanges], true)],
            &config,
        );
        assert_eq!(plan.order[0].depth, ReviewDepthHint::Deep);
    }

    /// The documented example config orders the motivating session's repos —
    /// as an example, explicitly.
    #[test]
    fn example_config_orders_the_motivating_session() {
        let plan = prioritize(
            vec![
                target("arc-node", "arc-node", &[RiskFactor::AssetMovement], true),
                target("malachite", "malachite", &[RiskFactor::Consensus], true),
                target(
                    "arc-remote-signer",
                    "arc-remote-signer",
                    &[RiskFactor::KeyCustody],
                    true,
                ),
            ],
            &PrioritizerConfig::example_session_config(),
        );
        let ids: Vec<&str> = plan.order.iter().map(|c| c.target.id.as_str()).collect();
        assert_eq!(ids, vec!["arc-remote-signer", "malachite", "arc-node"]);
    }
}
