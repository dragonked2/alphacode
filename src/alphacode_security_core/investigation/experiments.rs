//! Typed, serializable experiment records for an investigation lead.
//!
//! Store labels and fingerprints, not request bodies, cookies, or credentials.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

const MAX_ATTEMPTS_PER_PLAN: u8 = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttemptOutcome {
    Supports,
    Contradicts,
    Inconclusive,
    Blocked,
    Paused,
}

impl AttemptOutcome {
    fn requires_evidence(self) -> bool {
        matches!(self, Self::Supports | Self::Contradicts)
    }

    fn requires_cause(self) -> bool {
        matches!(self, Self::Inconclusive | Self::Blocked | Self::Paused)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NegativeCause {
    ExpectedBehavior,
    ControlNotReached,
    AuthenticationUnavailable,
    RateLimited,
    ToolFailure,
    CacheOrRouting,
    Other,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestDimension {
    Identity,
    Ownership,
    Tenant,
    Method,
    ApiVersion,
    WorkflowState,
    ContentType,
    Cache,
    Other(String),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestProfile {
    /// HTTP method only; credentials and request bodies must not be stored here.
    pub method: String,
    /// Human-readable test-account label, never a token or password.
    pub actor_label: Option<String>,
    pub object_owner_label: Option<String>,
    pub workflow_state: String,
    pub dimension: TestDimension,
}

impl RequestProfile {
    fn validate(&self) -> Result<(), String> {
        if self.method.trim().is_empty()
            || self.method.chars().any(char::is_control)
            || self.workflow_state.trim().is_empty()
            || self
                .actor_label
                .as_ref()
                .is_some_and(|s| s.trim().is_empty())
            || self
                .object_owner_label
                .as_ref()
                .is_some_and(|s| s.trim().is_empty())
        {
            return Err("request profile requires a method and workflow state".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExperimentPlan {
    pub expected_secure_behavior: String,
    pub positive_control: String,
    pub negative_control: String,
    pub success_oracle: String,
    pub reopen_condition: String,
    pub request: RequestProfile,
    /// Bounded plan-local budget; does not authorize requests by itself.
    pub attempt_budget: u8,
}

impl ExperimentPlan {
    fn validate(&self) -> Result<(), String> {
        self.request.validate()?;
        if [
            &self.expected_secure_behavior,
            &self.positive_control,
            &self.negative_control,
            &self.success_oracle,
            &self.reopen_condition,
        ]
        .iter()
        .any(|value| value.trim().is_empty())
            || self.attempt_budget == 0
            || self.attempt_budget > MAX_ATTEMPTS_PER_PLAN
        {
            return Err("experiment plan is incomplete or exceeds its attempt budget".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExperimentAttempt {
    pub id: String,
    /// SHA-256 of a redacted capture; the capture itself lives in an artifact.
    pub request_fingerprint: String,
    pub outcome: AttemptOutcome,
    pub observation: String,
    pub evidence_ids: BTreeSet<String>,
    pub negative_cause: Option<NegativeCause>,
    pub at: String,
}

impl ExperimentAttempt {
    fn validate(&self) -> Result<(), String> {
        if self.id.trim().is_empty()
            || !is_sha256(&self.request_fingerprint)
            || self.observation.trim().is_empty()
            || chrono::DateTime::parse_from_rfc3339(&self.at).is_err()
            || (self.outcome.requires_evidence() && self.evidence_ids.is_empty())
            || (self.outcome.requires_cause() && self.negative_cause.is_none())
        {
            return Err("experiment attempt is missing a valid observation or provenance".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClueStage {
    Observed,
    Correlated,
    Hypothesized,
    Tested,
    Resolved,
    Rejected,
    Paused,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClueStep {
    pub clue_id: String,
    pub stage: ClueStage,
    pub note: String,
    pub evidence_ids: BTreeSet<String>,
    pub at: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReassessmentDisposition {
    Pending,
    Continue,
    /// Start a separate lead; the current lead keeps its immutable plan/history.
    NewExperiment,
    /// Follow a separate lead/frontier rather than continuing this plan.
    Pivot,
    Pause,
    Block,
    Close,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reassessment {
    pub reviewed_attempts: BTreeSet<String>,
    pub cause: NegativeCause,
    pub reason: String,
    pub disposition: ReassessmentDisposition,
    pub decision: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LeadResearch {
    pub plan: Option<ExperimentPlan>,
    pub attempts: Vec<ExperimentAttempt>,
    pub clues: Vec<ClueStep>,
    pub reassessments: Vec<Reassessment>,
}

impl LeadResearch {
    pub fn reassessment_pending(&self) -> bool {
        self.reassessments
            .last()
            .is_some_and(|item| item.disposition == ReassessmentDisposition::Pending)
    }

    pub fn set_plan(&mut self, plan: ExperimentPlan) -> Result<(), String> {
        plan.validate()?;
        if !self.attempts.is_empty() {
            return Err("an experiment plan cannot change after attempts are recorded".into());
        }
        self.plan = Some(plan);
        Ok(())
    }

    pub fn record_attempt(&mut self, attempt: ExperimentAttempt) -> Result<(), String> {
        attempt.validate()?;
        let plan = self.plan.as_ref().ok_or("experiment plan is required")?;
        if self.attempts.len() >= usize::from(plan.attempt_budget)
            || self.attempts.iter().any(|old| old.id == attempt.id)
        {
            return Err("experiment attempt id is duplicated or plan budget is exhausted".into());
        }
        if self.reassessment_pending() {
            return Err("resolve the pending reassessment before another attempt".into());
        }
        if self
            .reassessments
            .last()
            .is_some_and(|item| item.disposition != ReassessmentDisposition::Continue)
        {
            return Err(
                "the last reassessment did not authorize continuing this experiment plan".into(),
            );
        }
        self.attempts.push(attempt);
        Ok(())
    }

    pub fn record_clue(&mut self, step: ClueStep) -> Result<(), String> {
        if step.clue_id.trim().is_empty()
            || step.note.trim().is_empty()
            || chrono::DateTime::parse_from_rfc3339(&step.at).is_err()
        {
            return Err("clue step requires an id, note, and RFC 3339 timestamp".into());
        }
        self.clues.push(step);
        Ok(())
    }

    pub fn request_reassessment(
        &mut self,
        reviewed_attempts: BTreeSet<String>,
        cause: NegativeCause,
        reason: String,
        at: String,
    ) -> Result<(), String> {
        if self.reassessment_pending()
            || reviewed_attempts.is_empty()
            || reason.trim().is_empty()
            || chrono::DateTime::parse_from_rfc3339(&at).is_err()
            || reviewed_attempts
                .iter()
                .any(|id| !self.attempts.iter().any(|attempt| &attempt.id == id))
        {
            return Err(
                "reassessment requires known attempts, a reason, and a valid timestamp".into(),
            );
        }
        self.reassessments.push(Reassessment {
            reviewed_attempts,
            cause,
            reason,
            disposition: ReassessmentDisposition::Pending,
            decision: None,
            updated_at: at,
        });
        Ok(())
    }

    pub fn resolve_reassessment(
        &mut self,
        disposition: ReassessmentDisposition,
        decision: String,
        at: String,
    ) -> Result<(), String> {
        if disposition == ReassessmentDisposition::Pending
            || decision.trim().is_empty()
            || chrono::DateTime::parse_from_rfc3339(&at).is_err()
        {
            return Err("reassessment resolution requires a decision and final disposition".into());
        }
        let pending = self
            .reassessments
            .last_mut()
            .filter(|item| item.disposition == ReassessmentDisposition::Pending)
            .ok_or("there is no pending reassessment")?;
        pending.disposition = disposition;
        pending.decision = Some(decision);
        pending.updated_at = at;
        Ok(())
    }

    pub fn check_integrity(&self, evidence_ids: &BTreeSet<String>) -> Result<(), String> {
        if let Some(plan) = &self.plan {
            plan.validate()?;
        } else if !self.attempts.is_empty() {
            return Err("experiment attempts exist without a plan".into());
        }
        if self
            .plan
            .as_ref()
            .is_some_and(|plan| self.attempts.len() > usize::from(plan.attempt_budget))
        {
            return Err("experiment history exceeds its plan budget".into());
        }

        let mut attempts = BTreeMap::new();
        for attempt in &self.attempts {
            attempt.validate()?;
            if attempt
                .evidence_ids
                .iter()
                .any(|id| !evidence_ids.contains(id))
                || attempts.insert(attempt.id.as_str(), attempt).is_some()
            {
                return Err(
                    "experiment history has duplicate ids or broken evidence references".into(),
                );
            }
        }

        for clue in &self.clues {
            if clue.clue_id.trim().is_empty()
                || clue.note.trim().is_empty()
                || chrono::DateTime::parse_from_rfc3339(&clue.at).is_err()
                || clue
                    .evidence_ids
                    .iter()
                    .any(|id| !evidence_ids.contains(id))
            {
                return Err("clue history has invalid fields or broken evidence references".into());
            }
        }

        for (index, item) in self.reassessments.iter().enumerate() {
            if item.reviewed_attempts.is_empty()
                || item
                    .reviewed_attempts
                    .iter()
                    .any(|id| !attempts.contains_key(id.as_str()))
                || item.reason.trim().is_empty()
                || chrono::DateTime::parse_from_rfc3339(&item.updated_at).is_err()
                || (item.disposition == ReassessmentDisposition::Pending
                    && (index + 1 != self.reassessments.len() || item.decision.is_some()))
                || (item.disposition != ReassessmentDisposition::Pending
                    && item
                        .decision
                        .as_deref()
                        .is_none_or(|decision| decision.trim().is_empty()))
            {
                return Err("reassessment history is incomplete or out of order".into());
            }
        }
        Ok(())
    }
}

fn is_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> ExperimentPlan {
        ExperimentPlan {
            expected_secure_behavior: "deny non-owner access".into(),
            positive_control: "owner reads the private marker".into(),
            negative_control: "missing account is denied".into(),
            success_oracle: "inspect the private marker in the response".into(),
            reopen_condition: "new endpoint revision or identity".into(),
            request: RequestProfile {
                method: "GET".into(),
                actor_label: Some("test-actor-b".into()),
                object_owner_label: Some("test-owner-a".into()),
                workflow_state: "object exists and is private".into(),
                dimension: TestDimension::Ownership,
            },
            attempt_budget: 3,
        }
    }

    fn attempt(id: &str, evidence_ids: BTreeSet<String>) -> ExperimentAttempt {
        ExperimentAttempt {
            id: id.into(),
            request_fingerprint: "a".repeat(64),
            outcome: AttemptOutcome::Inconclusive,
            observation: "The request was rate-limited before the handler ran".into(),
            evidence_ids,
            negative_cause: Some(NegativeCause::RateLimited),
            at: "2026-11-13T00:00:00Z".into(),
        }
    }

    #[test]
    fn reassessment_blocks_more_attempts_until_a_decision_is_recorded() {
        let mut research = LeadResearch::default();
        research.set_plan(plan()).unwrap();
        research
            .record_attempt(attempt("a1", BTreeSet::new()))
            .unwrap();
        research
            .request_reassessment(
                BTreeSet::from(["a1".into()]),
                NegativeCause::RateLimited,
                "The application path was not reached".into(),
                "2026-11-13T00:01:00Z".into(),
            )
            .unwrap();
        assert!(research.reassessment_pending());
        assert!(
            research
                .record_attempt(attempt("a2", BTreeSet::new()))
                .is_err()
        );

        research
            .resolve_reassessment(
                ReassessmentDisposition::Pause,
                "Wait for the authorized rate-limit window".into(),
                "2026-11-13T00:02:00Z".into(),
            )
            .unwrap();
        assert!(!research.reassessment_pending());
        assert!(
            research
                .record_attempt(attempt("a2", BTreeSet::new()))
                .is_err()
        );

        research
            .request_reassessment(
                BTreeSet::from(["a1".into()]),
                NegativeCause::RateLimited,
                "The authorized rate-limit window has reopened".into(),
                "2026-11-13T00:03:00Z".into(),
            )
            .unwrap();
        research
            .resolve_reassessment(
                ReassessmentDisposition::Continue,
                "Resume the same bounded experiment plan".into(),
                "2026-11-13T00:04:00Z".into(),
            )
            .unwrap();
        research
            .record_attempt(attempt("a2", BTreeSet::new()))
            .unwrap();
        research
            .check_integrity(&BTreeSet::new())
            .expect("valid research history should round-trip");
    }

    #[test]
    fn persisted_attempts_reject_missing_evidence_and_unbounded_plans() {
        let mut research = LeadResearch::default();
        research.set_plan(plan()).unwrap();
        let mut supported = attempt("a1", BTreeSet::from(["missing".into()]));
        supported.outcome = AttemptOutcome::Supports;
        supported.negative_cause = None;
        research.attempts.push(supported);
        assert!(research.check_integrity(&BTreeSet::new()).is_err());

        let mut unbounded = plan();
        unbounded.attempt_budget = u8::MAX;
        assert!(unbounded.validate().is_err());
    }
}
