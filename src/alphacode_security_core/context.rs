use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::chain::AttackChain;
use super::coverage::CoverageTracker;
use super::coverage::TestRecord;
use super::evidence::Evidence;
use super::finding::{Finding, FindingStage, Severity};
use super::hypothesis::{Hypothesis, HypothesisStatus};
use super::knowledge::KnowledgeStore;
use super::scope::LiveScope;

/// The central security engagement state.
///
/// This is the single source of truth for an ongoing security engagement.
/// Everything is dynamically populated from live target analysis.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SecurityContext {
    /// Unique engagement identifier.
    pub engagement_id: String,
    /// Human-readable engagement name.
    pub name: String,
    /// Current live scope (dynamically discovered).
    pub scope: LiveScope,
    /// All hypotheses generated during this engagement.
    pub hypotheses: Vec<Hypothesis>,
    /// All findings (observation through confirmed).
    pub findings: Vec<Finding>,
    /// Attack chains linking multiple findings.
    pub chains: Vec<AttackChain>,
    /// Coverage tracking.
    pub coverage: CoverageTracker,
    /// Knowledge store populated during this engagement.
    pub knowledge: KnowledgeStore,
    /// Evidence indexed by finding_id.
    pub evidence: HashMap<String, Evidence>,
    /// Engagement timeline events.
    pub timeline: Vec<TimelineEvent>,
    /// Current phase of the engagement.
    pub phase: EngagementPhase,
    /// Agent assignments: agent_id → assigned tasks.
    pub agent_assignments: HashMap<String, Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngagementPhase {
    Recon,
    Analysis,
    Exploitation,
    Validation,
    Chaining,
    Reporting,
}

impl EngagementPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Recon => "recon",
            Self::Analysis => "analysis",
            Self::Exploitation => "exploitation",
            Self::Validation => "validation",
            Self::Chaining => "chaining",
            Self::Reporting => "reporting",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub timestamp: String,
    pub event_type: String,
    pub description: String,
    pub agent: Option<String>,
    pub related_ids: Vec<String>,
}

impl SecurityContext {
    pub fn new(engagement_id: String, name: String, scope: LiveScope) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            engagement_id,
            name,
            scope,
            hypotheses: Vec::new(),
            findings: Vec::new(),
            chains: Vec::new(),
            coverage: CoverageTracker::new(),
            knowledge: KnowledgeStore::new(),
            evidence: HashMap::new(),
            timeline: vec![TimelineEvent {
                timestamp: now,
                event_type: "engagement_started".to_string(),
                description: "Security engagement initialized".to_string(),
                agent: None,
                related_ids: Vec::new(),
            }],
            phase: EngagementPhase::Recon,
            agent_assignments: HashMap::new(),
        }
    }

    /// Add a hypothesis to the engagement (dedups by id).
    pub fn add_hypothesis(&mut self, h: Hypothesis) {
        if !self.hypotheses.iter().any(|e| e.id == h.id) {
            self.hypotheses.push(h);
        }
    }

    /// Get all hypotheses with a given status.
    pub fn hypotheses_by_status(&self, status: &HypothesisStatus) -> Vec<&Hypothesis> {
        self.hypotheses
            .iter()
            .filter(|h| h.status == *status)
            .collect()
    }

    /// Add a finding to the engagement (dedups by id).
    pub fn add_finding(&mut self, f: Finding) {
        if !self.findings.iter().any(|e| e.id == f.id) {
            self.findings.push(f);
        }
    }

    /// Get findings at a specific stage.
    pub fn findings_by_stage(&self, stage: &FindingStage) -> Vec<&Finding> {
        self.findings.iter().filter(|f| f.stage == *stage).collect()
    }

    /// Get confirmed findings.
    pub fn confirmed_findings(&self) -> Vec<&Finding> {
        self.findings_by_stage(&FindingStage::ConfirmedFinding)
    }

    /// Add an attack chain (dedups by id).
    pub fn add_chain(&mut self, chain: AttackChain) {
        if !self.chains.iter().any(|c| c.id == chain.id) {
            self.chains.push(chain);
        }
    }

    /// Record evidence for a finding, merging with existing items.
    pub fn record_evidence(&mut self, finding_id: String, evidence: Evidence) {
        self.evidence
            .entry(finding_id)
            .and_modify(|existing| {
                existing
                    .evidence_items
                    .extend(evidence.evidence_items.clone());
                existing
                    .reproduction_trace
                    .extend(evidence.reproduction_trace.clone());
                existing.metadata.total_steps = existing.reproduction_trace.len() as u32;
            })
            .or_insert(evidence);
    }

    /// Assign a task to an agent.
    pub fn assign_agent(&mut self, agent_id: String, task: String) {
        self.agent_assignments
            .entry(agent_id)
            .or_default()
            .push(task);
    }

    /// Record a test result in coverage.
    pub fn record_test(&mut self, record: TestRecord) {
        self.coverage.record_test(record);
    }

    /// Add a knowledge entry.
    pub fn add_knowledge(&mut self, entry: super::knowledge::KnowledgeEntry) {
        self.knowledge.add(entry);
    }

    /// Record a timeline event.
    pub fn record_event(&mut self, event_type: String, description: String, agent: Option<String>) {
        let now = chrono::Utc::now().to_rfc3339();
        self.timeline.push(TimelineEvent {
            timestamp: now,
            event_type,
            description,
            agent,
            related_ids: Vec::new(),
        });
    }

    /// Check whether a phase transition is a valid forward step.
    pub fn can_advance_to(&self, next: &EngagementPhase) -> bool {
        matches!(
            (&self.phase, next),
            (EngagementPhase::Recon, EngagementPhase::Analysis)
                | (EngagementPhase::Analysis, EngagementPhase::Exploitation)
                | (EngagementPhase::Exploitation, EngagementPhase::Validation)
                | (EngagementPhase::Validation, EngagementPhase::Chaining)
                | (EngagementPhase::Chaining, EngagementPhase::Reporting)
        )
    }

    /// Advance the engagement phase (only valid forward steps).
    pub fn advance_phase(&mut self, phase: EngagementPhase) -> Result<(), String> {
        if !self.can_advance_to(&phase) {
            return Err(format!(
                "Cannot advance phase from '{}' to '{}'",
                self.phase.as_str(),
                phase.as_str()
            ));
        }
        self.record_event(
            "phase_change".to_string(),
            format!("Phase changed to {}", phase.as_str()),
            None,
        );
        self.phase = phase;
        Ok(())
    }

    /// Get total severity distribution of findings.
    pub fn severity_distribution(&self) -> HashMap<Severity, usize> {
        let mut dist = HashMap::new();
        for f in &self.findings {
            *dist.entry(f.severity.clone()).or_insert(0) += 1;
        }
        dist
    }

    /// Get summary statistics.
    pub fn summary(&self) -> EngagementSummary {
        EngagementSummary {
            engagement_id: self.engagement_id.clone(),
            name: self.name.clone(),
            phase: self.phase.as_str().to_string(),
            surface_size: self.scope.attack_surface_size(),
            total_hypotheses: self.hypotheses.len(),
            active_hypotheses: self
                .hypotheses_by_status(&HypothesisStatus::InProgress)
                .len()
                + self
                    .hypotheses_by_status(&HypothesisStatus::SelectedForTesting)
                    .len(),
            supported_hypotheses: self
                .hypotheses_by_status(&HypothesisStatus::Supported)
                .len(),
            total_findings: self.findings.len(),
            confirmed_findings: self.confirmed_findings().len(),
            total_chains: self.chains.len(),
            validated_chains: self
                .chains
                .iter()
                .filter(|c| c.status == super::chain::ChainStatus::FullyValidated)
                .count(),
            knowledge_entries: self.knowledge.count(),
            timeline_events: self.timeline.len(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngagementSummary {
    pub engagement_id: String,
    pub name: String,
    pub phase: String,
    pub surface_size: usize,
    pub total_hypotheses: usize,
    pub active_hypotheses: usize,
    pub supported_hypotheses: usize,
    pub total_findings: usize,
    pub confirmed_findings: usize,
    pub total_chains: usize,
    pub validated_chains: usize,
    pub knowledge_entries: usize,
    pub timeline_events: usize,
}

#[cfg(test)]
mod tests {
    use super::super::finding::VulnerabilityClass;
    use super::super::hypothesis::Hypothesis;
    use super::super::scope::EngagementMode;
    use super::*;

    fn test_context() -> SecurityContext {
        let scope = LiveScope::new(EngagementMode::BugBounty, "test-target".into());
        SecurityContext::new("eng-1".into(), "Test Engagement".into(), scope)
    }

    #[test]
    fn context_lifecycle() {
        let mut ctx = test_context();
        assert_eq!(ctx.phase, EngagementPhase::Recon);
        assert!(ctx.advance_phase(EngagementPhase::Analysis).is_ok());
        assert_eq!(ctx.phase, EngagementPhase::Analysis);
        assert_eq!(ctx.timeline.len(), 2);
        // Skipping phases is rejected.
        assert!(ctx.advance_phase(EngagementPhase::Reporting).is_err());
        assert_eq!(ctx.phase, EngagementPhase::Analysis);
    }

    #[test]
    fn context_hypotheses() {
        let mut ctx = test_context();
        let h = Hypothesis::new(
            "h1".into(),
            "Test hypothesis".into(),
            VulnerabilityClass::Xss,
            "/test".into(),
            "input reflected".into(),
        );
        ctx.add_hypothesis(h);
        assert_eq!(ctx.hypotheses.len(), 1);
        assert_eq!(
            ctx.hypotheses_by_status(&HypothesisStatus::Proposed).len(),
            1
        );
    }

    #[test]
    fn context_findings() {
        let mut ctx = test_context();
        let f = Finding::new(
            "f1".into(),
            "Test finding".into(),
            VulnerabilityClass::Xss,
            "target".into(),
        );
        ctx.add_finding(f);
        assert_eq!(ctx.findings.len(), 1);
        assert_eq!(ctx.confirmed_findings().len(), 0);
    }

    #[test]
    fn context_summary() {
        let ctx = test_context();
        let summary = ctx.summary();
        assert_eq!(summary.engagement_id, "eng-1");
        assert_eq!(summary.total_findings, 0);
    }
}
