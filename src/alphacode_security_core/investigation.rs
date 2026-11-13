//! Durable, deterministic investigation relationships for the live hunt runtime.
//! This module performs no network requests and never infers authorization.
use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::proof::{HttpExchange, ProofObject};
use super::scope_gate::{ScopeDecision, ScopePolicy, decide, normalize_host};

mod experiments;
pub use experiments::{
    AttemptOutcome, ClueStage, ClueStep, ExperimentAttempt, ExperimentPlan, LeadResearch,
    NegativeCause, Reassessment, ReassessmentDisposition, RequestProfile, TestDimension,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvestigationStatus {
    Discovered,
    Hypothesized,
    Investigating,
    Validated,
    Rejected,
    Blocked,
    Supported,
    Inconclusive,
    Paused,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SurfaceAsset {
    pub locator: String,
    pub sources: BTreeSet<String>,
    pub first_seen: String,
    pub last_seen: String,
}

/// Relationships describe observations, never inherited permission.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SurfaceRelationship {
    pub from: String,
    pub to: String,
    pub relation: String,
    pub evidence_id: String,
}

/// Reference a redacted artifact, not a raw response, cookie or credential.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InvestigationEvidence {
    pub id: String,
    pub source: String,
    pub artifact: String,
    pub sha256: String,
    pub collected_at: String,
    pub redacted: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InvestigationLead {
    pub id: String,
    pub asset: String,
    pub category: String,
    pub status: InvestigationStatus,
    pub identity: Option<String>,
    pub preconditions: Vec<String>,
    pub expected_secure_behavior: String,
    pub suspected_behavior: String,
    pub verification_steps: Vec<String>,
    pub supporting: BTreeSet<String>,
    pub contradicting: BTreeSet<String>,
    pub blocker: Option<String>,
    pub disposition: Option<String>,
    /// Observable effects are distinct from independently established security impact.
    #[serde(default)]
    pub observed_effects: Vec<String>,
    pub demonstrated_impact: Vec<String>,
    pub plausible_impact: Vec<String>,
    /// Integer utility units make ranking stable across platforms.
    pub information_gain: u32,
    pub impact_potential: u32,
    pub cost: u32,
    pub risk: u32,
    pub history: Vec<InvestigationTransition>,
    /// Structured experiment history augments this lead, not a second authority.
    #[serde(default)]
    pub research: LeadResearch,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InvestigationTransition {
    pub from: InvestigationStatus,
    pub to: InvestigationStatus,
    pub reason: String,
    pub evidence: BTreeSet<String>,
    pub at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VerifiedTransition {
    pub from: String,
    pub to: String,
    pub prerequisite: String,
    pub capability: String,
    pub evidence: BTreeSet<String>,
    pub verified_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct InvestigationIntelligence {
    assets: BTreeMap<String, SurfaceAsset>,
    relationships: BTreeSet<SurfaceRelationship>,
    evidence: BTreeMap<String, InvestigationEvidence>,
    leads: BTreeMap<String, InvestigationLead>,
    chain_transitions: Vec<VerifiedTransition>,
    #[serde(default)]
    validation_captures: BTreeMap<String, String>,
}

fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Queries/fragments/userinfo are deliberately excluded from persisted locators.
/// Query variants must be distinguished by redacted evidence, not secret values.
pub(crate) fn canonical_asset(input: &str) -> Option<String> {
    if input.contains("://") {
        let mut url = url::Url::parse(input).ok()?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            return None;
        }
        url.set_username("").ok()?;
        url.set_password(None).ok()?;
        url.set_query(None);
        url.set_fragment(None);
        Some(url.to_string())
    } else {
        normalize_host(input)
    }
}

fn capture_fingerprint(capture: &HttpExchange) -> Result<String, String> {
    // Reimporting through another tool or changing method/header casing is
    // still the same experiment, not independent transition evidence.
    let mut headers: Vec<_> = capture
        .response_headers
        .iter()
        .map(|(name, value)| (name.to_ascii_lowercase(), value))
        .collect();
    headers.sort();
    let bytes = serde_json::to_vec(&(
        capture.method.to_ascii_uppercase(),
        &capture.url,
        &capture.request_body,
        capture.status,
        headers,
        &capture.response_body,
    ))
    .map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

impl InvestigationIntelligence {
    /// Detect broken cross-references before replacing a live engagement on resume.
    pub fn check_integrity(&self) -> Result<(), String> {
        for (id, lead) in &self.leads {
            if id != &lead.id
                || !self.assets.contains_key(&lead.asset)
                || lead
                    .supporting
                    .intersection(&lead.contradicting)
                    .next()
                    .is_some()
                || lead
                    .supporting
                    .union(&lead.contradicting)
                    .any(|id| !self.evidence.contains_key(id))
            {
                return Err("investigation snapshot has broken lead references".into());
            }
            if lead.status == InvestigationStatus::Validated
                && !self.validation_captures.contains_key(id)
            {
                return Err("validated investigation is missing its capture fingerprint".into());
            }
            if lead.status == InvestigationStatus::Blocked
                && lead.blocker.as_deref().is_none_or(|s| s.trim().is_empty())
            {
                return Err("blocked investigation is missing its reason".into());
            }
            self.check_research_integrity(lead)?;
        }
        for edge in &self.relationships {
            if !self.assets.contains_key(&edge.from)
                || !self.assets.contains_key(&edge.to)
                || !self.evidence.contains_key(&edge.evidence_id)
            {
                return Err("investigation snapshot has broken asset relationships".into());
            }
        }
        for edge in &self.chain_transitions {
            if [&edge.from, &edge.to].iter().any(|id| {
                self.leads
                    .get(*id)
                    .is_none_or(|lead| lead.status != InvestigationStatus::Validated)
            }) || edge.evidence.is_empty()
                || edge
                    .evidence
                    .iter()
                    .any(|id| !self.evidence.contains_key(id))
            {
                return Err("investigation snapshot has broken chain references".into());
            }
        }
        Ok(())
    }

    fn check_research_integrity(&self, lead: &InvestigationLead) -> Result<(), String> {
        let evidence_ids = self.evidence.keys().cloned().collect();
        lead.research.check_integrity(&evidence_ids)
    }

    pub fn assets(&self) -> &BTreeMap<String, SurfaceAsset> {
        &self.assets
    }

    pub fn leads(&self) -> &BTreeMap<String, InvestigationLead> {
        &self.leads
    }

    pub fn chain_transitions(&self) -> &[VerifiedTransition] {
        &self.chain_transitions
    }

    /// Merge discovery provenance, retaining the earliest observation.
    pub fn discover(&mut self, locator: &str, source: &str) -> Option<String> {
        if source.trim().is_empty() {
            return None;
        }
        let key = canonical_asset(locator)?;
        let at = now();
        let asset = self
            .assets
            .entry(key.clone())
            .or_insert_with(|| SurfaceAsset {
                locator: key.clone(),
                sources: BTreeSet::new(),
                first_seen: at.clone(),
                last_seen: at.clone(),
            });
        asset.sources.insert(source.to_string());
        asset.last_seen = at;
        Some(key)
    }

    /// Immutable evidence identities prevent silently rewriting an old decision.
    pub fn record_evidence(&mut self, mut item: InvestigationEvidence) -> Result<bool, String> {
        if item.id.trim().is_empty()
            || item.source.trim().is_empty()
            || item.artifact.trim().is_empty()
            || chrono::DateTime::parse_from_rfc3339(&item.collected_at).is_err()
            || !item.redacted
            || item.sha256.len() != 64
            || !item.sha256.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err("evidence requires provenance, timestamp, redaction and SHA-256".into());
        }
        item.sha256.make_ascii_lowercase();
        if let Some(old) = self.evidence.get(&item.id) {
            return if old == &item {
                Ok(false)
            } else {
                Err("evidence identity is immutable".into())
            };
        }
        self.evidence.insert(item.id.clone(), item);
        Ok(true)
    }

    pub fn relate(&mut self, edge: SurfaceRelationship) -> Result<bool, String> {
        if !self.assets.contains_key(&edge.from)
            || !self.assets.contains_key(&edge.to)
            || !self.evidence.contains_key(&edge.evidence_id)
            || edge.relation.trim().is_empty()
        {
            return Err("relationship requires known assets and evidence".into());
        }
        Ok(self.relationships.insert(edge))
    }

    /// Stable semantic identity suppresses duplicate leads across tools/retries.
    pub fn propose(&mut self, asset: &str, category: &str, claim: &str) -> Result<String, String> {
        let asset = canonical_asset(asset).ok_or("invalid asset")?;
        if !self.assets.contains_key(&asset)
            || category.trim().is_empty()
            || claim.trim().is_empty()
        {
            return Err("lead requires a discovered asset, category and claim".into());
        }
        let identity = serde_json::to_vec(&(&asset, category.trim(), claim.trim()))
            .map_err(|e| e.to_string())?;
        let id = format!("lead:{:x}", Sha256::digest(identity));
        self.leads
            .entry(id.clone())
            .or_insert_with(|| InvestigationLead {
                id: id.clone(),
                asset,
                category: category.trim().into(),
                status: InvestigationStatus::Discovered,
                identity: None,
                preconditions: Vec::new(),
                expected_secure_behavior: String::new(),
                suspected_behavior: claim.trim().into(),
                verification_steps: Vec::new(),
                supporting: BTreeSet::new(),
                contradicting: BTreeSet::new(),
                blocker: None,
                disposition: None,
                observed_effects: Vec::new(),
                demonstrated_impact: Vec::new(),
                plausible_impact: Vec::new(),
                information_gain: 1,
                impact_potential: 1,
                cost: 1,
                risk: 0,
                history: Vec::new(),
                research: LeadResearch::default(),
            });
        Ok(id)
    }

    /// Configure an experiment without exposing mutable lifecycle/evidence fields.
    pub fn plan(
        &mut self,
        id: &str,
        expected: String,
        steps: Vec<String>,
        identity: Option<String>,
        preconditions: Vec<String>,
    ) -> Result<(), String> {
        let lead = self.leads.get_mut(id).ok_or("unknown lead")?;
        if !matches!(
            lead.status,
            InvestigationStatus::Discovered
                | InvestigationStatus::Hypothesized
                | InvestigationStatus::Blocked
                | InvestigationStatus::Inconclusive
        ) {
            return Err("cannot change an active or concluded experiment".into());
        }
        if expected.trim().is_empty()
            || steps.is_empty()
            || steps.iter().any(|s| s.trim().is_empty())
        {
            return Err("experiment requires secure behavior and verification steps".into());
        }
        lead.expected_secure_behavior = expected;
        lead.verification_steps = steps;
        lead.identity = identity;
        lead.preconditions = preconditions;
        Ok(())
    }

    /// Add the bounded differential-test plan to a lead. A plan records intent
    /// and controls; it does not authorize requests. Execution still requires
    /// the explicit scope-checked transition to `Investigating`.
    pub fn set_experiment_plan(&mut self, id: &str, plan: ExperimentPlan) -> Result<(), String> {
        let lead = self.leads.get_mut(id).ok_or("unknown lead")?;
        if matches!(
            lead.status,
            InvestigationStatus::Validated | InvestigationStatus::Rejected
        ) {
            return Err("reopen a concluded lead before changing its experiment plan".into());
        }
        lead.research.set_plan(plan)
    }

    /// Record a completed attempt and attach only its evidence-backed outcome
    /// to the parent lead. The attempt is committed first; evidence references
    /// and opposite dispositions are checked before either record is changed.
    pub fn record_experiment_attempt(
        &mut self,
        id: &str,
        attempt: ExperimentAttempt,
    ) -> Result<(), String> {
        let lead = self.leads.get_mut(id).ok_or("unknown lead")?;
        if lead.status != InvestigationStatus::Investigating {
            return Err("experiment attempts require an authorized active investigation".into());
        }
        if attempt
            .evidence_ids
            .iter()
            .any(|evidence_id| !self.evidence.contains_key(evidence_id))
        {
            return Err("experiment attempt references unknown evidence".into());
        }
        let opposite = match attempt.outcome {
            AttemptOutcome::Supports => &lead.contradicting,
            AttemptOutcome::Contradicts => &lead.supporting,
            _ => return lead.research.record_attempt(attempt),
        };
        if attempt
            .evidence_ids
            .iter()
            .any(|evidence_id| opposite.contains(evidence_id))
        {
            return Err("experiment evidence already has the opposite disposition".into());
        }

        let outcome = attempt.outcome;
        let evidence_ids = attempt.evidence_ids.clone();
        lead.research.record_attempt(attempt)?;
        match outcome {
            AttemptOutcome::Supports => lead.supporting.extend(evidence_ids),
            AttemptOutcome::Contradicts => lead.contradicting.extend(evidence_ids),
            _ => unreachable!("non-evidential outcomes returned before this point"),
        }
        Ok(())
    }

    /// Append a clue step only when its cited artifacts are in the immutable
    /// evidence store for this investigation.
    pub fn record_research_clue(&mut self, id: &str, clue: ClueStep) -> Result<(), String> {
        if clue
            .evidence_ids
            .iter()
            .any(|evidence_id| !self.evidence.contains_key(evidence_id))
        {
            return Err("research clue references unknown evidence".into());
        }
        let lead = self.leads.get_mut(id).ok_or("unknown lead")?;
        if matches!(
            lead.status,
            InvestigationStatus::Validated | InvestigationStatus::Rejected
        ) {
            return Err("reopen a concluded lead before adding a clue".into());
        }
        lead.research.record_clue(clue)
    }

    /// Ask for a checkpoint over known attempts. This is a research workflow
    /// gate, not a finding verdict or a change to the lead's security status.
    pub fn request_reassessment(
        &mut self,
        id: &str,
        reviewed_attempts: BTreeSet<String>,
        cause: NegativeCause,
        reason: String,
    ) -> Result<(), String> {
        let lead = self.leads.get_mut(id).ok_or("unknown lead")?;
        if lead.status != InvestigationStatus::Investigating {
            return Err("reassessment requires an active investigation".into());
        }
        lead.research
            .request_reassessment(reviewed_attempts, cause, reason, now())
    }

    pub fn resolve_reassessment(
        &mut self,
        id: &str,
        disposition: ReassessmentDisposition,
        decision: String,
    ) -> Result<(), String> {
        let lead = self.leads.get_mut(id).ok_or("unknown lead")?;
        if lead.status != InvestigationStatus::Investigating {
            return Err("reassessment resolution requires an active investigation".into());
        }
        lead.research
            .resolve_reassessment(disposition, decision, now())
    }

    pub fn attach_evidence(
        &mut self,
        id: &str,
        evidence_id: &str,
        supports: bool,
    ) -> Result<bool, String> {
        if !self.evidence.contains_key(evidence_id) {
            return Err("unknown evidence".into());
        }
        let lead = self.leads.get_mut(id).ok_or("unknown lead")?;
        if matches!(
            lead.status,
            InvestigationStatus::Validated | InvestigationStatus::Rejected
        ) {
            return Err("reopen a concluded lead before changing its evidence".into());
        }
        if (if supports {
            &lead.contradicting
        } else {
            &lead.supporting
        })
        .contains(evidence_id)
        {
            return Err("evidence already has the opposite disposition".into());
        }
        Ok((if supports {
            &mut lead.supporting
        } else {
            &mut lead.contradicting
        })
        .insert(evidence_id.into()))
    }

    fn change(lead: &mut InvestigationLead, to: InvestigationStatus, reason: &str) {
        lead.history.push(InvestigationTransition {
            from: lead.status,
            to,
            reason: reason.into(),
            evidence: lead
                .supporting
                .union(&lead.contradicting)
                .cloned()
                .collect(),
            at: now(),
        });
        lead.status = to;
        lead.disposition = Some(reason.into());
        lead.blocker = (to == InvestigationStatus::Blocked).then(|| reason.into());
    }

    /// Only an explicit matching allow rule authorizes an investigation.
    pub fn transition(
        &mut self,
        id: &str,
        to: InvestigationStatus,
        reason: &str,
        policy: &ScopePolicy,
    ) -> Result<(), String> {
        let lead = self.leads.get_mut(id).ok_or("unknown lead")?;
        if reason.trim().is_empty() {
            return Err("transition needs a reason".into());
        }
        if lead.research.reassessment_pending() {
            return Err("resolve the pending reassessment before changing lead status".into());
        }
        let legal = matches!(
            (lead.status, to),
            (
                InvestigationStatus::Discovered,
                InvestigationStatus::Hypothesized
            ) | (
                InvestigationStatus::Hypothesized,
                InvestigationStatus::Investigating
            ) | (
                InvestigationStatus::Investigating,
                InvestigationStatus::Rejected
            ) | (
                InvestigationStatus::Investigating,
                InvestigationStatus::Blocked
            ) | (
                InvestigationStatus::Hypothesized,
                InvestigationStatus::Blocked
            )
        );
        if !legal {
            return Err(
                "illegal investigation transition; validation and reopening have dedicated gates"
                    .into(),
            );
        }
        if to == InvestigationStatus::Hypothesized
            && (lead.expected_secure_behavior.is_empty() || lead.verification_steps.is_empty())
        {
            return Err("hypothesis needs a discriminating experiment".into());
        }
        if to == InvestigationStatus::Investigating
            && !matches!(decide(policy, &lead.asset), ScopeDecision::Allowed { .. })
        {
            return Err("asset lacks explicit authorization".into());
        }
        if to == InvestigationStatus::Rejected && lead.contradicting.is_empty() {
            return Err(
                "rejection requires contradictory evidence; tool failure is a blocker".into(),
            );
        }
        Self::change(lead, to, reason);
        Ok(())
    }

    /// A terminal/blocked theory may reopen only on previously unassociated evidence.
    pub fn reopen(&mut self, id: &str, evidence_id: &str, reason: &str) -> Result<(), String> {
        if !self.evidence.contains_key(evidence_id) || reason.trim().is_empty() {
            return Err("reopening needs new evidence and a reason".into());
        }
        let digest = &self.evidence[evidence_id].sha256;
        let lead = self.leads.get_mut(id).ok_or("unknown lead")?;
        if lead.supporting.union(&lead.contradicting).any(|old| {
            self.evidence
                .get(old)
                .is_some_and(|item| &item.sha256 == digest)
        }) {
            return Err("relabeling an existing artifact is not fresh evidence".into());
        }
        if !matches!(
            lead.status,
            InvestigationStatus::Rejected
                | InvestigationStatus::Blocked
                | InvestigationStatus::Validated
        ) || lead.supporting.contains(evidence_id)
            || lead.contradicting.contains(evidence_id)
        {
            return Err("reopening requires a concluded/blocked lead and fresh evidence".into());
        }
        // Retain old contradictions and history; do not silently erase them.
        lead.supporting.insert(evidence_id.into());
        lead.observed_effects.clear();
        lead.demonstrated_impact.clear();
        Self::change(lead, InvestigationStatus::Hypothesized, reason);
        self.validation_captures.remove(id);
        // Invalidate current chain edges when a constituent finding is reopened.
        self.chain_transitions
            .retain(|edge| edge.from != id && edge.to != id);
        Ok(())
    }

    pub fn validate(
        &mut self,
        id: &str,
        proof: &ProofObject,
        observed: &[HttpExchange],
        policy: &ScopePolicy,
    ) -> Result<(), String> {
        let lead = self.leads.get_mut(id).ok_or("unknown lead")?;
        if lead.status != InvestigationStatus::Investigating
            || lead.research.reassessment_pending()
            || lead.supporting.is_empty()
            || !lead.contradicting.is_empty()
        {
            return Err(
                "validation needs active investigation, evidence and no unresolved contradictions"
                    .into(),
            );
        }
        if !matches!(decide(policy, &lead.asset), ScopeDecision::Allowed { .. }) {
            return Err("asset lacks explicit authorization".into());
        }
        if proof.baseline.as_ref().is_none_or(|pair| {
            !matches!(
                decide(policy, &pair.baseline.url),
                ScopeDecision::Allowed { .. }
            ) || !matches!(
                decide(policy, &pair.treatment.url),
                ScopeDecision::Allowed { .. }
            )
        }) {
            return Err("proof control and treatment must both be explicitly authorized".into());
        }
        let verdict = proof.verify_report_evidence(Some(observed));
        if !verdict.verified {
            return Err("proof failed the observed-exchange report gate".into());
        }
        // Require the proof to concern this asset, rather than a different valid finding.
        if proof
            .test_exchange
            .as_ref()
            .and_then(|exchange| canonical_asset(&exchange.url))
            .as_deref()
            != Some(lead.asset.as_str())
        {
            return Err("proof does not reference the hypothesis asset".into());
        }
        let capture = proof
            .test_exchange
            .as_ref()
            .ok_or("missing test exchange")?;
        let fingerprint = capture_fingerprint(capture)?;
        self.validation_captures.insert(id.into(), fingerprint);
        // Persist only structured, report-safe observations, never URL query
        // values or caller-authored baseline labels from the proof verdict.
        lead.observed_effects = vec![format!(
            "Controlled response effect verified on {}; security boundary and impact require independent review",
            lead.asset
        )];
        Self::change(
            lead,
            InvestigationStatus::Validated,
            "observed-exchange proof gate passed",
        );
        Ok(())
    }

    /// Evidence-check a proposed connection separately from its endpoint findings.
    /// This proves a differential experiment, not the semantics of the claimed
    /// prerequisite or capability; independent boundary/causality review is required.
    /// The caller supplies an explicit prerequisite and capability experiment;
    /// matching vulnerability labels alone never creates a chain.
    pub fn verify_chain_transition(
        &mut self,
        mut edge: VerifiedTransition,
        proof: &ProofObject,
        observed: &[HttpExchange],
        policy: &ScopePolicy,
    ) -> Result<(), String> {
        if edge.from == edge.to
            || edge.prerequisite.trim().is_empty()
            || edge.capability.trim().is_empty()
            || edge.evidence.is_empty()
            || edge
                .evidence
                .iter()
                .any(|id| !self.evidence.contains_key(id))
        {
            return Err(
                "chain needs distinct findings, prerequisite, capability and evidence".into(),
            );
        }
        for id in [&edge.from, &edge.to] {
            let lead = self.leads.get(id).ok_or("unknown chain endpoint")?;
            if lead.status != InvestigationStatus::Validated
                || !matches!(decide(policy, &lead.asset), ScopeDecision::Allowed { .. })
            {
                return Err("chain endpoints must be validated and explicitly authorized".into());
            }
        }
        if proof.baseline.as_ref().is_none_or(|pair| {
            !matches!(
                decide(policy, &pair.baseline.url),
                ScopeDecision::Allowed { .. }
            ) || !matches!(
                decide(policy, &pair.treatment.url),
                ScopeDecision::Allowed { .. }
            )
        }) {
            return Err("chain control and treatment must both be explicitly authorized".into());
        }
        let capture = proof
            .test_exchange
            .as_ref()
            .ok_or("missing chain test exchange")?;
        let fingerprint = capture_fingerprint(capture)?;
        if self
            .validation_captures
            .values()
            .any(|old| old == &fingerprint)
        {
            return Err(
                "chain requires a separate transition experiment, not an endpoint proof replay"
                    .into(),
            );
        }
        let target = &self.leads[&edge.to].asset;
        if proof
            .test_exchange
            .as_ref()
            .and_then(|e| canonical_asset(&e.url))
            .as_ref()
            != Some(target)
            || !proof.verify_report_evidence(Some(observed)).verified
        {
            return Err("chain transition needs an observed proof on the destination asset".into());
        }
        if self
            .chain_transitions
            .iter()
            .any(|old| old.from == edge.from && old.to == edge.to)
        {
            return Err("chain transition already recorded".into());
        }
        edge.verified_at = now();
        self.chain_transitions.push(edge);
        Ok(())
    }

    /// Utility estimates guide planning only; they cannot raise evidence confidence.
    pub fn prioritize(
        &mut self,
        id: &str,
        gain: u32,
        impact: u32,
        cost: u32,
        risk: u32,
    ) -> Result<(), String> {
        let lead = self.leads.get_mut(id).ok_or("unknown lead")?;
        lead.information_gain = gain;
        lead.impact_potential = impact;
        lead.cost = cost;
        lead.risk = risk;
        Ok(())
    }

    /// Resolve a contradiction explicitly, preserving the original evidence in history.
    pub fn resolve_contradiction(
        &mut self,
        id: &str,
        contradiction: &str,
        resolution: &str,
        reason: &str,
    ) -> Result<(), String> {
        if !self.evidence.contains_key(resolution)
            || contradiction == resolution
            || reason.trim().is_empty()
        {
            return Err("resolution needs separate evidence and an explanation".into());
        }
        let lead = self.leads.get_mut(id).ok_or("unknown lead")?;
        if lead.status != InvestigationStatus::Investigating
            || !lead.contradicting.contains(contradiction)
        {
            return Err(
                "resolution requires an active investigation and recorded contradiction".into(),
            );
        }
        lead.supporting.insert(resolution.into());
        let status = lead.status;
        Self::change(lead, status, reason);
        lead.contradicting.remove(contradiction);
        Ok(())
    }

    /// Deterministic bounded selection; blocked/rejected work is never retried implicitly.
    pub fn next(
        &self,
        policy: &ScopePolicy,
        max_cost: u32,
        max_risk: u32,
    ) -> Option<&InvestigationLead> {
        self.leads
            .values()
            .filter(|lead| {
                matches!(
                    lead.status,
                    InvestigationStatus::Hypothesized | InvestigationStatus::Supported
                ) && !lead.research.reassessment_pending()
                    && lead.cost <= max_cost
                    && lead.risk <= max_risk
                    && matches!(decide(policy, &lead.asset), ScopeDecision::Allowed { .. })
            })
            .max_by(|a, b| {
                let utility = |lead: &InvestigationLead| {
                    u128::from(lead.information_gain) + u128::from(lead.impact_potential)
                };
                (utility(a) * (u128::from(b.cost) + 1) * (u128::from(b.risk) + 1))
                    .cmp(&(utility(b) * (u128::from(a.cost) + 1) * (u128::from(a.risk) + 1)))
                    .then_with(|| b.id.cmp(&a.id))
            })
    }

    /// Discovered assets without any validated investigation remain coverage gaps.
    pub fn coverage_gaps(&self) -> Vec<&SurfaceAsset> {
        let validated: BTreeSet<_> = self
            .leads
            .values()
            .filter(|l| l.status == InvestigationStatus::Validated)
            .map(|l| l.asset.as_str())
            .collect();
        self.assets
            .values()
            .filter(|asset| !validated.contains(asset.locator.as_str()))
            .collect()
    }

    /// Render a compact status line for checkpoints; this is not a finding report.
    pub fn research_summary(&self) -> String {
        if self.assets.is_empty() && self.leads.is_empty() {
            return String::new();
        }

        let states = [
            (InvestigationStatus::Discovered, "discovered"),
            (InvestigationStatus::Hypothesized, "hypothesized"),
            (InvestigationStatus::Investigating, "investigating"),
            (InvestigationStatus::Supported, "supported"),
            (InvestigationStatus::Validated, "validated experiments"),
            (InvestigationStatus::Rejected, "rejected"),
            (InvestigationStatus::Blocked, "blocked"),
            (InvestigationStatus::Inconclusive, "inconclusive"),
            (InvestigationStatus::Paused, "paused"),
        ];
        let mut counts = Vec::new();
        for (status, label) in states {
            let count = self
                .leads
                .values()
                .filter(|lead| lead.status == status)
                .count();
            if count > 0 {
                counts.push(format!("{label}: {count}"));
            }
        }
        let pending = self
            .leads
            .values()
            .filter(|lead| lead.research.reassessment_pending())
            .count();
        format!(
            "Investigation tracking (validated experiments are not confirmed findings): {} asset(s), {} lead(s) [{}], {} asset(s) without a validated experiment, {pending} pending reassessment(s).\n",
            self.assets.len(),
            self.leads.len(),
            counts.join(", "),
            self.coverage_gaps().len(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> ScopePolicy {
        ScopePolicy::from_lists(&["https://lab.example/api".into()], &[], true, &[], 1, 1)
    }

    fn fixture() -> (InvestigationIntelligence, String) {
        let mut state = InvestigationIntelligence::default();
        state
            .discover("https://lab.example/api/object", "katana")
            .unwrap();
        let id = state
            .propose(
                "https://lab.example/api/object",
                "authorization",
                "cross-account access",
            )
            .unwrap();
        state
            .plan(
                &id,
                "deny a non-owner".into(),
                vec!["compare owner and non-owner test accounts".into()],
                Some("test-user-b".into()),
                vec!["known private object owned by A".into()],
            )
            .unwrap();
        (state, id)
    }

    fn evidence(id: &str) -> InvestigationEvidence {
        InvestigationEvidence {
            id: id.into(),
            source: "httpflow".into(),
            artifact: format!("evidence/{id}.json"),
            sha256: format!("{:x}", Sha256::digest(id.as_bytes())),
            collected_at: now(),
            redacted: true,
        }
    }

    #[test]
    fn discovery_merges_provenance_without_retaining_query_secrets() {
        let mut state = InvestigationIntelligence::default();
        let first = state
            .discover(
                "https://user:secret@LAB.example/api?token=secret#secret",
                "katana",
            )
            .unwrap();
        let second = state.discover("https://lab.example/api", "gau").unwrap();
        assert_eq!(first, second);
        assert_eq!(state.assets.len(), 1);
        assert_eq!(state.assets[&first].sources.len(), 2);
        assert!(!serde_json::to_string(&state).unwrap().contains("secret"));
        assert!(state.discover("file:///etc/passwd", "tool").is_none());
    }

    #[test]
    fn state_machine_rejects_skips_missing_scope_and_unbacked_conclusions() {
        let (mut state, id) = fixture();
        assert!(
            state
                .transition(
                    &id,
                    InvestigationStatus::Validated,
                    "scanner says yes",
                    &policy()
                )
                .is_err()
        );
        assert!(
            state
                .transition(&id, InvestigationStatus::Investigating, "skip", &policy())
                .is_err()
        );
        state
            .transition(
                &id,
                InvestigationStatus::Hypothesized,
                "discriminating experiment planned",
                &policy(),
            )
            .unwrap();
        assert!(
            state
                .transition(
                    &id,
                    InvestigationStatus::Investigating,
                    "no policy",
                    &ScopePolicy::default()
                )
                .is_err()
        );
        state
            .transition(
                &id,
                InvestigationStatus::Investigating,
                "explicitly scoped",
                &policy(),
            )
            .unwrap();
        assert!(
            state
                .transition(
                    &id,
                    InvestigationStatus::Rejected,
                    "empty scanner output",
                    &policy()
                )
                .is_err()
        );
        assert!(
            state
                .validate(&id, &ProofObject::new(&id), &[], &policy())
                .is_err()
        );
        state
            .transition(
                &id,
                InvestigationStatus::Blocked,
                "test identity unavailable",
                &policy(),
            )
            .unwrap();
        assert!(state.next(&policy(), 10, 10).is_none());
        assert_eq!(
            state.leads[&id].blocker.as_deref(),
            Some("test identity unavailable")
        );
    }

    #[test]
    fn rejected_leads_need_fresh_evidence_and_keep_their_history() {
        let (mut state, id) = fixture();
        state.record_evidence(evidence("negative-control")).unwrap();
        state
            .attach_evidence(&id, "negative-control", false)
            .unwrap();
        state
            .transition(&id, InvestigationStatus::Hypothesized, "plan", &policy())
            .unwrap();
        state
            .transition(&id, InvestigationStatus::Investigating, "test", &policy())
            .unwrap();
        state
            .transition(
                &id,
                InvestigationStatus::Rejected,
                "ownership control refutes access",
                &policy(),
            )
            .unwrap();
        assert!(state.reopen(&id, "negative-control", "repeat").is_err());
        state.record_evidence(evidence("new-observation")).unwrap();
        state
            .reopen(&id, "new-observation", "new configuration observed")
            .unwrap();
        assert_eq!(state.leads[&id].history.len(), 4);
        assert!(state.leads[&id].contradicting.contains("negative-control"));
        assert!(
            state
                .validate(&id, &ProofObject::new(&id), &[], &policy())
                .is_err()
        );
    }

    #[test]
    fn immutable_evidence_rejects_unredacted_and_overwritten_artifacts() {
        let mut state = InvestigationIntelligence::default();
        let item = evidence("e1");
        assert!(state.record_evidence(item.clone()).unwrap());
        assert!(!state.record_evidence(item.clone()).unwrap());
        let mut changed = item;
        changed.sha256 = "b".repeat(64);
        assert!(state.record_evidence(changed.clone()).is_err());
        changed.id = "e2".into();
        changed.redacted = false;
        assert!(state.record_evidence(changed).is_err());
    }

    #[test]
    fn serialized_state_preserves_blockers_relations_and_deduplication() {
        let (mut state, id) = fixture();
        let host = state.discover("lab.example", "dnsx").unwrap();
        state.record_evidence(evidence("e1")).unwrap();
        let edge = SurfaceRelationship {
            from: host,
            to: state.leads[&id].asset.clone(),
            relation: "hosts".into(),
            evidence_id: "e1".into(),
        };
        assert!(state.relate(edge.clone()).unwrap());
        assert!(!state.relate(edge).unwrap());
        state
            .transition(&id, InvestigationStatus::Hypothesized, "plan", &policy())
            .unwrap();
        state
            .transition(
                &id,
                InvestigationStatus::Blocked,
                "missing account",
                &policy(),
            )
            .unwrap();
        let bytes = serde_json::to_vec(&state).unwrap();
        let mut restored: InvestigationIntelligence = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.relationships.len(), 1);
        assert_eq!(
            restored.leads[&id].blocker.as_deref(),
            Some("missing account")
        );
        assert_eq!(
            restored
                .propose(
                    "https://lab.example/api/object",
                    "authorization",
                    "cross-account access"
                )
                .unwrap(),
            id
        );
        assert_eq!(restored.coverage_gaps().len(), 2);
    }

    #[test]
    fn ranking_is_stable_and_scope_exclusions_win() {
        let (mut state, id) = fixture();
        state
            .transition(&id, InvestigationStatus::Hypothesized, "plan", &policy())
            .unwrap();
        assert_eq!(state.next(&policy(), 1, 0).unwrap().id, id);
        assert!(state.next(&policy(), 0, 0).is_none());
        let mut excluded = policy();
        excluded.out_of_scope = excluded.in_scope.clone();
        assert!(state.next(&excluded, 10, 10).is_none());
        assert!(
            state
                .transition(
                    &id,
                    InvestigationStatus::Investigating,
                    "excluded",
                    &excluded
                )
                .is_err()
        );
    }
    fn controlled_proof(url: &str, marker: &str) -> (ProofObject, Vec<HttpExchange>) {
        use super::super::proof::{BaselineComparison, ProofEffect, ProofStep};
        let control = HttpExchange {
            method: "GET".into(),
            url: url.into(),
            request_body: None,
            status: 403,
            response_headers: Vec::new(),
            response_body: Some("denied".into()),
            source_tool: "lab".into(),
        };
        let treatment = HttpExchange {
            status: 200,
            response_body: Some(marker.into()),
            ..control.clone()
        };
        let mut proof = ProofObject::new("lab-proof");
        proof.test_exchange = Some(treatment.clone());
        proof.expected_behavior = "deny test non-owner".into();
        proof.observed_behavior = "controlled private marker returned".into();
        proof.reproduction = vec![ProofStep {
            action: "compare two controlled test identities".into(),
            expected_observation: marker.into(),
            tool: Some("lab".into()),
        }];
        proof.effect = Some(ProofEffect::BodyMarker {
            marker: marker.into(),
        });
        proof.baseline = Some(BaselineComparison::new(
            "non-owner control",
            control.clone(),
            treatment.clone(),
        ));
        (proof, vec![control, treatment])
    }

    fn activate(state: &mut InvestigationIntelligence, id: &str) {
        state.record_evidence(evidence(id)).unwrap();
        state.attach_evidence(id, id, true).unwrap();
        state
            .transition(id, InvestigationStatus::Hypothesized, "plan", &policy())
            .unwrap();
        state
            .transition(
                id,
                InvestigationStatus::Investigating,
                "authorized lab test",
                &policy(),
            )
            .unwrap();
    }

    #[test]
    fn validation_requires_actual_captures_and_matches_the_affected_asset() {
        let (mut state, id) = fixture();
        activate(&mut state, &id);
        let (proof, captures) =
            controlled_proof("https://lab.example/api/object", "private-test-marker");
        assert!(state.validate(&id, &proof, &[], &policy()).is_err());
        let (unrelated, other_captures) =
            controlled_proof("https://lab.example/api/other", "other-marker");
        assert!(
            state
                .validate(&id, &unrelated, &other_captures, &policy())
                .is_err()
        );
        state.validate(&id, &proof, &captures, &policy()).unwrap();
        assert_eq!(state.leads[&id].status, InvestigationStatus::Validated);
        assert!(!state.leads[&id].observed_effects.is_empty());
        assert!(state.leads[&id].demonstrated_impact.is_empty());
        assert!(state.leads[&id].plausible_impact.is_empty());
        assert!(state.coverage_gaps().is_empty());
    }

    #[test]
    fn chains_require_validated_endpoints_and_a_separate_observed_experiment() {
        let (mut state, first) = fixture();
        let asset = state
            .discover("https://lab.example/api/second", "lab")
            .unwrap();
        let second = state
            .propose(&asset, "authorization", "second controlled effect")
            .unwrap();
        state
            .plan(
                &second,
                "deny access".into(),
                vec!["compare accounts".into()],
                None,
                Vec::new(),
            )
            .unwrap();
        let edge = VerifiedTransition {
            from: first.clone(),
            to: second.clone(),
            prerequisite: "controlled capability from first finding".into(),
            capability: "second private marker accessible".into(),
            evidence: BTreeSet::from([first.clone()]),
            verified_at: String::new(),
        };
        let (second_proof, second_captures) = controlled_proof(&asset, "second-marker");
        assert!(
            state
                .verify_chain_transition(edge.clone(), &second_proof, &second_captures, &policy())
                .is_err()
        );
        activate(&mut state, &first);
        let (first_proof, first_captures) =
            controlled_proof("https://lab.example/api/object", "first-marker");
        state
            .validate(&first, &first_proof, &first_captures, &policy())
            .unwrap();
        activate(&mut state, &second);
        state
            .validate(&second, &second_proof, &second_captures, &policy())
            .unwrap();
        assert!(
            state
                .verify_chain_transition(edge.clone(), &second_proof, &second_captures, &policy())
                .is_err()
        );
        let (chain_proof, chain_captures) = controlled_proof(&asset, "separate-transition-marker");
        assert!(
            state
                .verify_chain_transition(edge.clone(), &chain_proof, &[], &policy())
                .is_err()
        );
        state
            .verify_chain_transition(edge, &chain_proof, &chain_captures, &policy())
            .unwrap();
        assert_eq!(state.chain_transitions.len(), 1);
        state
            .record_evidence(evidence("new-configuration"))
            .unwrap();
        state
            .reopen(&first, "new-configuration", "configuration changed")
            .unwrap();
        assert!(state.chain_transitions.is_empty());
    }
    #[test]
    fn corrupt_references_are_detected_before_resume() {
        let (mut state, id) = fixture();
        state.check_integrity().unwrap();
        state
            .leads
            .get_mut(&id)
            .unwrap()
            .supporting
            .insert("missing-artifact".into());
        assert!(state.check_integrity().is_err());
    }

    #[test]
    fn deterministic_fixture_measures_inventory_and_lead_retention() {
        let started = std::time::Instant::now();
        let mut flat_inventory = BTreeSet::new();
        let mut upgraded = InvestigationIntelligence::default();
        for i in 0..200 {
            let url = format!("https://lab.example/api/object?fixture_variant={i}");
            flat_inventory.insert(url.clone());
            upgraded
                .discover(&url, if i % 2 == 0 { "katana" } else { "gau" })
                .unwrap();
        }
        let id = upgraded
            .propose(
                "https://lab.example/api/object",
                "authorization",
                "controlled cross-account comparison needed",
            )
            .unwrap();
        let bytes = serde_json::to_vec(&upgraded).unwrap();
        let resumed: InvestigationIntelligence = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(flat_inventory.len(), 200);
        assert_eq!(resumed.assets.len(), 1);
        assert_eq!(resumed.assets.values().next().unwrap().sources.len(), 2);
        assert_eq!(resumed.leads[&id].status, InvestigationStatus::Discovered);
        assert_eq!(resumed.coverage_gaps().len(), 1);
        eprintln!(
            "inventory fixture: raw URL variants={}, canonical paths={}, retained unverified leads={}, elapsed_us={}",
            flat_inventory.len(),
            resumed.assets.len(),
            resumed.leads.len(),
            started.elapsed().as_micros()
        );
    }
    #[test]
    fn artifact_aliases_do_not_reopen_a_rejected_hypothesis() {
        let (mut state, id) = fixture();
        activate(&mut state, &id);
        let negative = evidence("negative");
        state.record_evidence(negative.clone()).unwrap();
        state.attach_evidence(&id, "negative", false).unwrap();
        state
            .transition(
                &id,
                InvestigationStatus::Rejected,
                "controlled refutation",
                &policy(),
            )
            .unwrap();
        let mut alias = negative;
        alias.id = "new-id-same-content".into();
        state.record_evidence(alias).unwrap();
        assert!(
            state
                .reopen(&id, "new-id-same-content", "renamed artifact")
                .is_err()
        );
    }

    #[test]
    fn capture_identity_does_not_change_with_tool_or_http_casing() {
        let (_, captures) = controlled_proof("https://lab.example/api/object", "private-marker");
        let original = captures[1].clone();
        let mut reimported = original.clone();
        reimported.source_tool = "another-tool".into();
        reimported.method = "get".into();
        assert_eq!(
            capture_fingerprint(&original).unwrap(),
            capture_fingerprint(&reimported).unwrap()
        );
    }

    #[test]
    fn explicit_contradiction_resolution_keeps_evidence_history() {
        let (mut state, id) = fixture();
        activate(&mut state, &id);
        state.record_evidence(evidence("contradiction")).unwrap();
        state.attach_evidence(&id, "contradiction", false).unwrap();
        state.record_evidence(evidence("resolution")).unwrap();
        state
            .resolve_contradiction(
                &id,
                "contradiction",
                "resolution",
                "controlled fixture explains conflicting result",
            )
            .unwrap();
        let lead = &state.leads[&id];
        assert!(lead.contradicting.is_empty());
        assert!(lead.supporting.contains("resolution"));
        assert!(
            lead.history
                .last()
                .unwrap()
                .evidence
                .contains("contradiction")
        );
        state.check_integrity().unwrap();
    }

    #[test]
    fn ranking_respects_information_gain_cost_and_risk() {
        let (mut state, first) = fixture();
        state
            .transition(
                &first,
                InvestigationStatus::Hypothesized,
                "first plan",
                &policy(),
            )
            .unwrap();
        let second = state
            .propose(
                "https://lab.example/api/object",
                "workflow",
                "separate protected operation",
            )
            .unwrap();
        state
            .plan(
                &second,
                "enforce transition precondition".into(),
                vec!["compare valid and invalid local workflow state".into()],
                None,
                Vec::new(),
            )
            .unwrap();
        state
            .transition(
                &second,
                InvestigationStatus::Hypothesized,
                "second plan",
                &policy(),
            )
            .unwrap();
        state.prioritize(&second, 10, 10, 1, 0).unwrap();
        assert_eq!(state.next(&policy(), 10, 10).unwrap().id, second);
        state.prioritize(&second, 10, 10, 100, 10).unwrap();
        assert_eq!(state.next(&policy(), 10, 10).unwrap().id, first);
    }

    #[test]
    fn research_summary_is_empty_without_state_and_labels_experiment_coverage() {
        assert!(
            InvestigationIntelligence::default()
                .research_summary()
                .is_empty()
        );

        let (state, _) = fixture();
        let summary = state.research_summary();
        assert!(summary.contains("1 asset(s), 1 lead(s)"));
        assert!(summary.contains("discovered: 1"));
        assert!(summary.contains("1 asset(s) without a validated experiment"));
        assert!(summary.contains("not confirmed findings"));
    }

    #[test]
    fn structured_research_uses_the_authorized_parent_and_evidence_store() {
        let (mut state, id) = fixture();
        activate(&mut state, &id);
        state
            .set_experiment_plan(
                &id,
                ExperimentPlan {
                    expected_secure_behavior: "deny non-owner access".into(),
                    positive_control: "owner sees the private marker".into(),
                    negative_control: "missing object is denied".into(),
                    success_oracle: "compare the complete protected field".into(),
                    reopen_condition: "endpoint or ownership state changes".into(),
                    request: RequestProfile {
                        method: "GET".into(),
                        actor_label: Some("test-account-b".into()),
                        object_owner_label: Some("test-account-a".into()),
                        workflow_state: "known private fixture".into(),
                        dimension: TestDimension::Ownership,
                    },
                    attempt_budget: 2,
                },
            )
            .unwrap();

        let positive = "research-positive";
        state.record_evidence(evidence(positive)).unwrap();
        let attempt = ExperimentAttempt {
            id: "attempt-1".into(),
            request_fingerprint: format!("{:x}", Sha256::digest(b"redacted-capture-1")),
            outcome: AttemptOutcome::Supports,
            observation: "The unauthorized actor received the private marker".into(),
            evidence_ids: BTreeSet::from([positive.into()]),
            negative_cause: None,
            at: now(),
        };
        state.record_experiment_attempt(&id, attempt).unwrap();
        assert!(state.leads[&id].supporting.contains(positive));

        state
            .record_research_clue(
                &id,
                ClueStep {
                    clue_id: "same-handler".into(),
                    stage: ClueStage::Tested,
                    note: "The response matched the controlled private marker".into(),
                    evidence_ids: BTreeSet::from([positive.into()]),
                    at: now(),
                },
            )
            .unwrap();
        state
            .request_reassessment(
                &id,
                BTreeSet::from(["attempt-1".into()]),
                NegativeCause::Other,
                "Review the control and attribution before further tests".into(),
            )
            .unwrap();
        assert!(
            state
                .transition(
                    &id,
                    InvestigationStatus::Blocked,
                    "attempt to bypass pending review",
                    &policy(),
                )
                .is_err()
        );

        state
            .resolve_reassessment(
                &id,
                ReassessmentDisposition::Continue,
                "Control and actor attribution were rechecked".into(),
            )
            .unwrap();
        let negative = "research-negative";
        state.record_evidence(evidence(negative)).unwrap();
        state
            .record_experiment_attempt(
                &id,
                ExperimentAttempt {
                    id: "attempt-2".into(),
                    request_fingerprint: format!("{:x}", Sha256::digest(b"redacted-capture-2")),
                    outcome: AttemptOutcome::Contradicts,
                    observation: "The matched unauthorized request was denied".into(),
                    evidence_ids: BTreeSet::from([negative.into()]),
                    negative_cause: None,
                    at: now(),
                },
            )
            .unwrap();
        assert!(state.leads[&id].contradicting.contains(negative));
        assert_eq!(state.leads[&id].research.attempts.len(), 2);
        state.check_integrity().unwrap();
    }
}
