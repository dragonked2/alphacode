//! Durable research-session checkpoints, coverage tracking, and negative
//! findings.
//!
//! Long multi-repository reviews die of context compaction: findings, paths,
//! proof code, and conclusions evaporate and get rebuilt (or re-litigated) in
//! later sessions. This module is the authoritative record that survives —
//! stored as JSON on disk via the session's own workspace, no new database.
//!
//! Three concerns share one record because they share one lifecycle:
//!
//! * **Checkpoints** ([`ResearchSession`]) — objective, scope, repositories
//!   and revisions, findings and their validation states, tool failures,
//!   outstanding tasks. Saved atomically (temp file + rename) so a partial
//!   write can never corrupt the authoritative record.
//! * **Coverage** ([`CoverageLedger`]) — which repository components were
//!   reviewed at which revision with which technique, and whether that
//!   review merely *inspected* the code or actually *validated* properties
//!   of it. Reading a function is not testing it, and the ledger keeps the
//!   two apart. A changed revision invalidates affected entries and stale
//!   evidence instead of being silently accepted.
//! * **Negative findings** ([`RejectedHypothesisRecord`]) — the attack
//!   surfaces that were investigated and came up empty, with what was checked
//!   and why the hypothesis died, so a dead end is a recorded result rather
//!   than a reason to re-walk the same code next session.
//!
//! Nothing here stores secrets: callers are responsible for scrubbing
//! credentials before recording, and [`ResearchSession::record_tool_failure`]
//! truncates messages to a size that discourages dumping whole environment
//! outputs into the record.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::finding::Finding;

/// Schema version of the on-disk record. Bump when a change would make old
/// records unreadable; loaders reject newer versions rather than guess.
pub const CHECKPOINT_SCHEMA_VERSION: u32 = 1;

/// One repository under review, pinned to a revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewedRepository {
    /// Repository identifier (URL or shorthand).
    pub repository: String,
    /// Exact revision reviewed (commit SHA or tag). Unknown revisions are
    /// recorded as `"unknown"` and treated as always-stale.
    pub revision: String,
    /// Branch that was checked out, when applicable.
    pub branch: Option<String>,
}

/// A component (module/package/file group) within a repository.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ComponentRef {
    pub repository: String,
    pub component: String,
}

/// What a prior review of a component actually established.
///
/// The distinction is the point: `Inspected` means code was read;
/// `Validated` means its security properties were tested (reproduction,
/// targeted analysis, or tests). Only `Validated` entries justify skipping
/// work, and even then only at the same revision.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewDepth {
    Inspected,
    Validated,
}

/// One recorded review of a component at a revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageEntry {
    pub component: ComponentRef,
    /// Revision the review was performed at.
    pub revision: String,
    /// Analysis techniques applied (e.g. "lock_unlock_symmetry",
    /// "authz_path_trace").
    pub techniques: BTreeSet<String>,
    pub depth: ReviewDepth,
    /// Findings that came out of this review.
    pub finding_ids: BTreeSet<String>,
    pub last_reviewed: DateTime<Utc>,
}

/// Persistent coverage ledger, one per repository set.
///
/// Query with [`CoverageLedger::status_for`]; record with
/// [`CoverageLedger::record_review`]. A forced-fresh review records a new
/// entry at the same revision without deleting history — the ledger is an
/// append-oriented record, not a cache to be trusted blindly.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CoverageLedger {
    pub entries: BTreeMap<String, CoverageEntry>,
}

/// One entry per component+revision. The technique is recorded *inside* the
/// entry (as a set), because depth is a property of the component at that
/// revision, not of each individual technique — reading a file and running a
/// reproduction both contribute to the same reviewed-at-this-revision record.
fn coverage_key(component: &ComponentRef, revision: &str) -> String {
    format!(
        "{}@{}#{}",
        component.repository, revision, component.component
    )
}

impl CoverageLedger {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a review of `component` at `revision`.
    ///
    /// Depth escalates monotonically per key: a validated review is never
    /// downgraded by a later inspection-only pass.
    pub fn record_review(
        &mut self,
        component: ComponentRef,
        revision: &str,
        technique: &str,
        depth: ReviewDepth,
        finding_ids: &[String],
        at: DateTime<Utc>,
    ) {
        let key = coverage_key(&component, revision);
        let entry = self.entries.entry(key).or_insert_with(|| CoverageEntry {
            component: component.clone(),
            revision: revision.to_string(),
            techniques: BTreeSet::new(),
            depth,
            finding_ids: BTreeSet::new(),
            last_reviewed: at,
        });
        entry.techniques.insert(technique.to_string());
        if depth == ReviewDepth::Validated {
            entry.depth = ReviewDepth::Validated;
        }
        entry.finding_ids.extend(finding_ids.iter().cloned());
        entry.last_reviewed = at;
    }

    /// What the ledger knows about this component at this exact revision.
    ///
    /// Returns `None` when the component was never reviewed *at this
    /// revision* — a review at a different commit is deliberately not
    /// matched, because code changes invalidate conclusions.
    pub fn status_for(&self, component: &ComponentRef, revision: &str) -> Option<&CoverageEntry> {
        self.entries
            .values()
            .find(|e| &e.component == component && e.revision == revision)
    }

    /// Components reviewed at any revision, for detecting stale evidence:
    /// reviewed elsewhere but not at the current revision.
    pub fn stale_for(
        &self,
        component: &ComponentRef,
        current_revision: &str,
    ) -> Vec<&CoverageEntry> {
        self.entries
            .values()
            .filter(|e| &e.component == component && e.revision != current_revision)
            .collect()
    }

    /// Entries recorded at a revision that no longer matches the repository's
    /// current revision. Callers use this to invalidate evidence after a
    /// repository updates.
    pub fn entries_invalidated_by(
        &self,
        repository: &str,
        new_revision: &str,
    ) -> Vec<&CoverageEntry> {
        self.entries
            .values()
            .filter(|e| e.component.repository == repository && e.revision != new_revision)
            .collect()
    }

    /// Total validated coverage at a revision, for resume summaries.
    pub fn validated_count_at(&self, repository: &str, revision: &str) -> usize {
        self.entries
            .values()
            .filter(|e| {
                e.component.repository == repository
                    && e.revision == revision
                    && e.depth == ReviewDepth::Validated
            })
            .count()
    }
}

/// A hypothesis that was investigated and not substantiated.
///
/// Recording this is what stops a dead end from being re-walked — and what
/// lets a later reader tell "we checked and it was clean" from "we never
/// checked".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RejectedHypothesisRecord {
    /// What was hypothesized, e.g. "RCE via template injection in worker".
    pub hypothesis: String,
    /// The attack surface the hypothesis targeted.
    pub attack_surface: String,
    /// Repository and revision where the investigation happened.
    pub repository: String,
    pub revision: String,
    /// Files/functions that were actually examined.
    pub code_examined: Vec<String>,
    /// Checks or experiments performed (tool commands, analyses).
    pub experiments: Vec<String>,
    /// Why the hypothesis was rejected — or why it remains unresolved
    /// (coverage gaps belong here rather than being omitted).
    pub rationale: String,
    /// Coverage gaps that remain; a rejected hypothesis with gaps is
    /// explicitly *not* the same as a proven-safe code path.
    pub coverage_gaps: Vec<String>,
    pub rejected_at: DateTime<Utc>,
}

/// A tool failure worth remembering: repeated failures of the same kind are
/// the exact behavior this record exists to prevent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ToolFailureRecord {
    pub tool: String,
    pub operation: String,
    /// Categorized error summary. Callers should scrub secrets; entries are
    /// truncated to [`TOOL_FAILURE_MESSAGE_LIMIT`] chars defensively.
    pub error: String,
    pub count: u64,
    pub last_seen: DateTime<Utc>,
}

/// Cap on stored error text per tool failure record.
pub const TOOL_FAILURE_MESSAGE_LIMIT: usize = 512;

/// The authoritative research-session record.
///
/// Save with [`ResearchSession::checkpoint`] (atomic), reload with
/// [`ResearchSession::load`], summarize for context with
/// [`ResearchSession::resume_summary`] — the summary is the intended way to
/// re-enter a session after compaction, so the whole record does not have to
/// be loaded into context.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ResearchSession {
    pub schema_version: u32,
    pub session_id: String,
    /// What this engagement is trying to establish.
    pub objective: String,
    /// Authorization and scope boundaries the work must stay inside.
    pub scope: Vec<String>,
    /// Repositories and pinned revisions under review.
    pub repos: Vec<ReviewedRepository>,
    /// Findings and their current validation states.
    pub findings: Vec<Finding>,
    /// Hypotheses investigated and not substantiated.
    pub rejected_hypotheses: Vec<RejectedHypothesisRecord>,
    /// What has been reviewed, at which revision, with which depth.
    pub coverage: CoverageLedger,
    /// Tool failures observed, deduplicated by (tool, operation).
    pub tool_failures: Vec<ToolFailureRecord>,
    /// Outstanding tasks / next actions.
    pub next_actions: Vec<String>,
    /// Cumulative time spent per workflow phase (Phase 12: know where a
    /// session's budget actually goes instead of guessing). Defaults when
    /// loading records written before it existed.
    #[serde(default)]
    pub timing: PhaseTiming,
    /// When the record was last checkpointed.
    pub updated_at: DateTime<Utc>,
}

/// Workflow phases whose time spend is worth tracking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowPhase {
    RepoSetup,
    SourceReview,
    Reproduction,
    Remediation,
    Reporting,
}

impl WorkflowPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::RepoSetup => "repo_setup",
            Self::SourceReview => "source_review",
            Self::Reproduction => "reproduction",
            Self::Remediation => "remediation",
            Self::Reporting => "reporting",
        }
    }
}

/// Cumulative seconds spent per phase, persisted with the checkpoint.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PhaseTiming {
    pub seconds: BTreeMap<WorkflowPhase, u64>,
}

impl PhaseTiming {
    pub fn add(&mut self, phase: WorkflowPhase, spent: Duration) {
        let entry = self.seconds.entry(phase).or_insert(0);
        *entry = entry.saturating_add(spent.as_secs());
    }

    pub fn total_secs(&self) -> u64 {
        self.seconds.values().sum()
    }

    /// One compact line — never a table — so it costs nothing in context.
    pub fn render(&self) -> String {
        if self.seconds.is_empty() {
            return "no phase timing recorded".to_string();
        }
        self.seconds
            .iter()
            .map(|(phase, secs)| format!("{}={}s", phase.as_str(), secs))
            .collect::<Vec<_>>()
            .join(" ")
    }
}

impl ResearchSession {
    /// Start a new session.
    pub fn new(session_id: impl Into<String>, objective: impl Into<String>) -> Self {
        Self {
            schema_version: CHECKPOINT_SCHEMA_VERSION,
            session_id: session_id.into(),
            objective: objective.into(),
            scope: Vec::new(),
            repos: Vec::new(),
            findings: Vec::new(),
            rejected_hypotheses: Vec::new(),
            coverage: CoverageLedger::new(),
            tool_failures: Vec::new(),
            next_actions: Vec::new(),
            timing: PhaseTiming::default(),
            updated_at: Utc::now(),
        }
    }

    /// Record time spent on a workflow phase. Callers pass the measured
    /// duration; this never fabricates one.
    pub fn record_phase_time(&mut self, phase: WorkflowPhase, spent: Duration) {
        self.timing.add(phase, spent);
    }

    /// Where this session's checkpoint lives inside a workspace root.
    pub fn checkpoint_path(root: &Path) -> PathBuf {
        root.join(".alphacode")
            .join("research")
            .join("session.json")
    }

    /// Where the human-readable findings ledger is exported.
    pub fn ledger_path(root: &Path) -> PathBuf {
        root.join(".alphacode").join("research").join("FINDINGS.md")
    }

    /// Checkpoint atomically: write to a sibling temp file, then rename over
    /// the destination. A crash mid-write leaves either the old file or the
    /// new one — never a truncated mix.
    pub fn checkpoint(&mut self, root: &Path) -> std::io::Result<PathBuf> {
        self.updated_at = Utc::now();
        let path = Self::checkpoint_path(root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("json.tmp");
        let serialized =
            serde_json::to_vec_pretty(self).map_err(|e| std::io::Error::other(e.to_string()))?;
        std::fs::write(&tmp, serialized)?;
        // Also verify the temp file round-trips before renaming: a corrupted
        // serialization must not be allowed to replace a good record.
        let readback = std::fs::read(&tmp)?;
        if readback.is_empty() {
            return Err(std::io::Error::other("checkpoint temp file wrote empty"));
        }
        std::fs::rename(&tmp, &path)?;
        Ok(path)
    }

    /// Load a checkpoint. Fails loudly on unreadable or newer-schema records
    /// rather than silently starting fresh and losing prior conclusions.
    pub fn load(root: &Path) -> Result<Self, SessionLoadError> {
        let path = Self::checkpoint_path(root);
        let bytes = std::fs::read(&path).map_err(|source| SessionLoadError::Io {
            path: path.clone(),
            source,
        })?;
        let session: Self =
            serde_json::from_slice(&bytes).map_err(|e| SessionLoadError::Deserialize {
                path: path.clone(),
                message: e.to_string(),
            })?;
        if session.schema_version > CHECKPOINT_SCHEMA_VERSION {
            return Err(SessionLoadError::NewerSchema {
                found: session.schema_version,
                supported: CHECKPOINT_SCHEMA_VERSION,
            });
        }
        Ok(session)
    }

    /// Add a repository under review, pinning its revision.
    pub fn track_repository(
        &mut self,
        repository: impl Into<String>,
        revision: impl Into<String>,
        branch: Option<String>,
    ) {
        let repository = repository.into();
        let revision = revision.into();
        if let Some(existing) = self.repos.iter_mut().find(|r| r.repository == repository) {
            existing.revision = revision;
            existing.branch = branch;
        } else {
            self.repos.push(ReviewedRepository {
                repository,
                revision,
                branch,
            });
        }
    }

    /// Record that a repository moved to a new revision, and collect the
    /// coverage entries whose evidence is now stale. The caller decides
    /// whether to re-review; this function only makes the invalidation
    /// visible instead of silent.
    pub fn invalidate_on_revision_change(
        &mut self,
        repository: &str,
        new_revision: &str,
    ) -> Vec<CoverageEntry> {
        let stale: Vec<CoverageEntry> = self
            .coverage
            .entries_invalidated_by(repository, new_revision)
            .into_iter()
            .cloned()
            .collect();
        self.track_repository(repository, new_revision, None);
        stale
    }

    /// Add a finding. Called immediately when a candidate is identified, so
    /// the finding survives even if the session dies mid-investigation.
    pub fn add_finding(&mut self, finding: Finding) {
        if !self.findings.iter().any(|f| f.id == finding.id) {
            self.findings.push(finding);
        }
    }

    pub fn finding_mut(&mut self, id: &str) -> Option<&mut Finding> {
        self.findings.iter_mut().find(|f| f.id == id)
    }

    /// Record a rejected or unresolved hypothesis.
    pub fn record_rejection(&mut self, record: RejectedHypothesisRecord) {
        self.rejected_hypotheses.push(record);
    }

    /// Record a tool failure, deduplicating by (tool, operation) and
    /// incrementing a count so repeated identical failures become visible
    /// instead of scrolling past.
    pub fn record_tool_failure(
        &mut self,
        tool: &str,
        operation: &str,
        error: &str,
        at: DateTime<Utc>,
    ) {
        let truncated: String = error.chars().take(TOOL_FAILURE_MESSAGE_LIMIT).collect();
        if let Some(existing) = self
            .tool_failures
            .iter_mut()
            .find(|f| f.tool == tool && f.operation == operation)
        {
            existing.count += 1;
            existing.error = truncated;
            existing.last_seen = at;
        } else {
            self.tool_failures.push(ToolFailureRecord {
                tool: tool.to_string(),
                operation: operation.to_string(),
                error: truncated,
                count: 1,
                last_seen: at,
            });
        }
    }

    /// Findings that survived the evidence gate — the only ones that may be
    /// reported as confirmed.
    pub fn confirmed_findings(&self) -> Vec<&Finding> {
        use super::finding::FindingStage;
        self.findings
            .iter()
            .filter(|f| {
                matches!(
                    f.stage,
                    FindingStage::ConfirmedFinding | FindingStage::Report
                )
            })
            .collect()
    }

    /// A concise re-entry summary after compaction or restart: enough to
    /// continue, without loading the whole record into context.
    pub fn resume_summary(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "Session {} — {}\n",
            self.session_id, self.objective
        ));
        out.push_str(&format!(
            "Repos: {}\n",
            self.repos
                .iter()
                .map(|r| format!("{}@{}", r.repository, r.revision))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        out.push_str(&format!(
            "Findings: {} total, {} confirmed; {} hypotheses rejected\n",
            self.findings.len(),
            self.confirmed_findings().len(),
            self.rejected_hypotheses.len()
        ));
        for repo in &self.repos {
            let validated = self
                .coverage
                .validated_count_at(&repo.repository, &repo.revision);
            out.push_str(&format!(
                "  {}@{}: {} validated components covered\n",
                repo.repository, repo.revision, validated
            ));
        }
        if !self.next_actions.is_empty() {
            out.push_str("Next actions:\n");
            for action in self.next_actions.iter().take(10) {
                out.push_str(&format!("  - {action}\n"));
            }
        }
        if !self.tool_failures.is_empty() {
            out.push_str(&format!(
                "Tool failures on record: {} (see checkpoint for detail)\n",
                self.tool_failures.len()
            ));
        }
        out.push_str(&format!("Time by phase: {}\n", self.timing.render()));
        out.push_str(&format!(
            "Last checkpoint: {}\n",
            self.updated_at.to_rfc3339()
        ));
        out
    }

    /// Export a human-readable findings ledger (FINDINGS.md equivalent).
    ///
    /// Only *confirmed* findings get a full entry; candidates are listed with
    /// their stage so the reader cannot mistake volume for validation.
    pub fn render_findings_ledger(&self) -> String {
        use super::finding::FindingStage;
        let mut out = String::new();
        out.push_str(&format!("# Findings — session {}\n\n", self.session_id));
        out.push_str(&format!("Objective: {}\n\n", self.objective));

        out.push_str("## Confirmed findings\n\n");
        let confirmed = self.confirmed_findings();
        if confirmed.is_empty() {
            out.push_str("_None. No finding has passed the evidence gate._\n\n");
        }
        for finding in confirmed {
            out.push_str(&format!(
                "### {} — {} (severity: {})\n\n",
                finding.id,
                finding.title,
                finding.severity.as_str()
            ));
            out.push_str(&format!(
                "- Repository/target: {}\n- Stage: {}\n- Confidence: {}\n",
                finding.target,
                finding.stage.as_str(),
                finding.confidence.as_str()
            ));
            if let Some(repro) = &finding.reproduction {
                out.push_str(&format!(
                    "- Expected: {}\n- Actual: {}\n",
                    repro.expected_behavior, repro.actual_behavior
                ));
            }
            out.push('\n');
        }

        out.push_str("## Candidates (not yet validated — do not report as confirmed)\n\n");
        let candidates: Vec<&Finding> = self
            .findings
            .iter()
            .filter(|f| {
                !matches!(
                    f.stage,
                    FindingStage::ConfirmedFinding | FindingStage::Report
                )
            })
            .collect();
        if candidates.is_empty() {
            out.push_str("_None._\n\n");
        }
        for finding in candidates {
            out.push_str(&format!(
                "- **{}** — {} (stage: {}, severity so far: {})\n",
                finding.id,
                finding.title,
                finding.stage.as_str(),
                finding.severity.as_str()
            ));
        }
        out.push('\n');

        out.push_str("## Rejected / unresolved hypotheses\n\n");
        if self.rejected_hypotheses.is_empty() {
            out.push_str("_None recorded._\n\n");
        }
        for rej in &self.rejected_hypotheses {
            out.push_str(&format!("### {}\n\n", rej.hypothesis));
            out.push_str(&format!(
                "- Surface: {} — repo {}@{}\n- Examined: {}\n- Experiments: {}\n- Rationale: {}\n",
                rej.attack_surface,
                rej.repository,
                rej.revision,
                rej.code_examined.join(", "),
                rej.experiments.join("; "),
                rej.rationale
            ));
            if !rej.coverage_gaps.is_empty() {
                out.push_str(&format!(
                    "- Coverage gaps: {}\n",
                    rej.coverage_gaps.join(", ")
                ));
            }
            out.push('\n');
        }
        out
    }

    /// Write the findings ledger next to the checkpoint. Atomic, same as the
    /// checkpoint itself.
    pub fn export_findings_ledger(&self, root: &Path) -> std::io::Result<PathBuf> {
        let path = Self::ledger_path(root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("md.tmp");
        std::fs::write(&tmp, self.render_findings_ledger())?;
        std::fs::rename(&tmp, &path)?;
        Ok(path)
    }
}

/// Why loading a checkpoint failed.
#[derive(Debug)]
pub enum SessionLoadError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    Deserialize {
        path: PathBuf,
        message: String,
    },
    NewerSchema {
        found: u32,
        supported: u32,
    },
}

impl std::fmt::Display for SessionLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "could not read checkpoint {}: {source}", path.display())
            }
            Self::Deserialize { path, message } => {
                write!(
                    f,
                    "checkpoint {} is not valid JSON: {message}",
                    path.display()
                )
            }
            Self::NewerSchema { found, supported } => write!(
                f,
                "checkpoint schema v{found} is newer than supported v{supported}"
            ),
        }
    }
}

impl std::error::Error for SessionLoadError {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_security_core::finding::VulnerabilityClass;

    fn temp_root() -> PathBuf {
        let dir = tempfile::Builder::new()
            .prefix("alphacode-session-")
            .tempdir()
            .expect("temp dir");
        let path = dir.path().to_path_buf();
        std::mem::forget(dir);
        path
    }

    fn sample_finding(id: &str) -> Finding {
        Finding::new(
            id.to_string(),
            "SQL injection in /search".to_string(),
            VulnerabilityClass::SqlInjection,
            "https://example.com".to_string(),
        )
    }

    /// A session can round-trip through disk and survive an interruption.
    #[test]
    fn checkpoint_and_load_round_trip() {
        let root = temp_root();
        let mut session = ResearchSession::new("s1", "review signer repos");
        session.track_repository("circlefin/arc-remote-signer", "abc123", None);
        session.add_finding(sample_finding("f1"));
        session.next_actions.push("review signing lifecycle".into());
        session.checkpoint(&root).expect("checkpoint");

        let loaded = ResearchSession::load(&root).expect("load");
        assert_eq!(loaded.session_id, "s1");
        assert_eq!(loaded.findings.len(), 1);
        assert_eq!(loaded.repos[0].revision, "abc123");
        assert_eq!(
            loaded.next_actions,
            vec!["review signing lifecycle".to_string()]
        );
    }

    /// A partially written checkpoint must never replace the good one: the
    /// write is temp-file + rename, so corrupting the temp file leaves the
    /// authoritative record intact.
    #[test]
    fn a_corrupt_checkpoint_does_not_replace_the_good_one() {
        let root = temp_root();
        let mut session = ResearchSession::new("s1", "objective");
        session.add_finding(sample_finding("f1"));
        session.checkpoint(&root).expect("first checkpoint");

        // Simulate a partial write: put garbage in the temp file. The real
        // checkpoint must still load.
        let path = ResearchSession::checkpoint_path(&root);
        std::fs::write(path.with_extension("json.tmp"), b"{\"tr").unwrap();
        let loaded = ResearchSession::load(&root).expect("original survives");
        assert_eq!(loaded.findings.len(), 1);
    }

    /// A checkpoint written by a newer schema is refused, not guessed at.
    #[test]
    fn a_newer_schema_checkpoint_is_rejected() {
        let root = temp_root();
        let mut session = ResearchSession::new("s1", "objective");
        session.checkpoint(&root).expect("checkpoint");
        let path = ResearchSession::checkpoint_path(&root);
        let mut value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        value["schema_version"] = serde_json::json!(CHECKPOINT_SCHEMA_VERSION + 1);
        std::fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(matches!(
            ResearchSession::load(&root),
            Err(SessionLoadError::NewerSchema { .. })
        ));
    }

    /// Confirmed findings survive reload — the stage gate is re-checked from
    /// serialized state, not remembered in memory.
    #[test]
    fn confirmed_findings_survive_reload() {
        let root = temp_root();
        let mut session = ResearchSession::new("s1", "objective");
        let mut finding = sample_finding("f1");
        for _ in 0..7 {
            finding.advance("walk".into()).expect("allowed");
        }
        finding.stage = crate::alphacode_security_core::finding::FindingStage::ConfirmedFinding;
        finding
            .stage_history
            .push(crate::alphacode_security_core::finding::StageTransition {
                from: Some(crate::alphacode_security_core::finding::FindingStage::ProgramCheck),
                to: crate::alphacode_security_core::finding::FindingStage::ConfirmedFinding,
                timestamp: Utc::now().to_rfc3339(),
                reason: "proof verified (test)".into(),
            });
        session.add_finding(finding);
        session.checkpoint(&root).expect("checkpoint");

        let loaded = ResearchSession::load(&root).expect("load");
        assert_eq!(loaded.confirmed_findings().len(), 1);
    }

    #[test]
    fn ledger_lists_candidates_separately_from_confirmed() {
        let mut session = ResearchSession::new("s1", "objective");
        let mut confirmed = sample_finding("f-confirmed");
        confirmed.stage = crate::alphacode_security_core::finding::FindingStage::ConfirmedFinding;
        session.add_finding(confirmed);
        session.add_finding(sample_finding("f-candidate"));

        let ledger = session.render_findings_ledger();
        let confirmed_pos = ledger.find("f-confirmed").expect("listed");
        let candidates_header = ledger.find("## Candidates").expect("section");
        assert!(
            confirmed_pos < candidates_header,
            "confirmed findings must appear in their own section"
        );
        let candidate_pos = ledger.find("f-candidate").expect("listed");
        assert!(candidate_pos > candidates_header);
        assert!(ledger.contains("do not report as confirmed"));
    }

    #[test]
    fn coverage_distinguishes_inspected_from_validated() {
        let mut ledger = CoverageLedger::new();
        let component = ComponentRef {
            repository: "repo".into(),
            component: "signer/keys".into(),
        };
        let at = Utc::now();
        ledger.record_review(
            component.clone(),
            "rev1",
            "lock_unlock_symmetry",
            ReviewDepth::Inspected,
            &[],
            at,
        );
        let entry = ledger.status_for(&component, "rev1").expect("entry");
        assert_eq!(entry.depth, ReviewDepth::Inspected);

        // Validation escalates and is never downgraded by later inspection.
        ledger.record_review(
            component.clone(),
            "rev1",
            "reproduction",
            ReviewDepth::Validated,
            &["f1".to_string()],
            at,
        );
        ledger.record_review(
            component.clone(),
            "rev1",
            "read_through",
            ReviewDepth::Inspected,
            &[],
            at,
        );
        let entry = ledger.status_for(&component, "rev1").expect("entry");
        assert_eq!(entry.depth, ReviewDepth::Validated);
        assert!(entry.finding_ids.contains("f1"));
        assert_eq!(ledger.validated_count_at("repo", "rev1"), 1);
    }

    /// A review at one revision does not count for another — this is what
    /// makes revision changes invalidate prior conclusions.
    #[test]
    fn coverage_is_revision_scoped() {
        let mut ledger = CoverageLedger::new();
        let component = ComponentRef {
            repository: "repo".into(),
            component: "consensus".into(),
        };
        ledger.record_review(
            component.clone(),
            "rev1",
            "technique",
            ReviewDepth::Validated,
            &[],
            Utc::now(),
        );
        assert!(ledger.status_for(&component, "rev1").is_some());
        assert!(ledger.status_for(&component, "rev2").is_none());
        assert_eq!(ledger.stale_for(&component, "rev2").len(), 1);

        let invalidated = ledger.entries_invalidated_by("repo", "rev2");
        assert_eq!(invalidated.len(), 1);
        assert_eq!(invalidated[0].revision, "rev1");
    }

    /// Moving a repository to a new revision surfaces stale coverage instead
    /// of silently accepting conclusions from old code.
    #[test]
    fn a_revision_change_invalidates_stale_coverage() {
        let mut session = ResearchSession::new("s1", "objective");
        session.track_repository("repo", "rev1", None);
        let component = ComponentRef {
            repository: "repo".into(),
            component: "signer".into(),
        };
        session.coverage.record_review(
            component,
            "rev1",
            "technique",
            ReviewDepth::Validated,
            &[],
            Utc::now(),
        );

        let stale = session.invalidate_on_revision_change("repo", "rev2");
        assert_eq!(stale.len(), 1, "old-revision coverage must be flagged");
        assert_eq!(session.repos[0].revision, "rev2");
    }

    /// Tool failures dedupe and count, so a repeated failure is visible as a
    /// pattern rather than a scroll of one-offs.
    #[test]
    fn repeated_tool_failures_count_up() {
        let mut session = ResearchSession::new("s1", "objective");
        let at = Utc::now();
        for _ in 0..3 {
            session.record_tool_failure("httpx", "probe", "connection refused", at);
        }
        assert_eq!(session.tool_failures.len(), 1);
        assert_eq!(session.tool_failures[0].count, 3);
    }

    /// Error text is truncated so a huge dump cannot bloat the record.
    #[test]
    fn tool_failure_messages_are_truncated() {
        let mut session = ResearchSession::new("s1", "objective");
        let huge = "x".repeat(TOOL_FAILURE_MESSAGE_LIMIT * 3);
        session.record_tool_failure("t", "op", &huge, Utc::now());
        assert_eq!(
            session.tool_failures[0].error.chars().count(),
            TOOL_FAILURE_MESSAGE_LIMIT
        );
    }

    #[test]
    fn resume_summary_is_concise_and_informative() {
        let mut session = ResearchSession::new("s9", "multi-repo review");
        session.track_repository("repo-a", "aaa", None);
        session.add_finding(sample_finding("f1"));
        session.next_actions.push("validate lock leak".into());
        let summary = session.resume_summary();
        assert!(summary.contains("repo-a@aaa"), "{summary}");
        assert!(summary.contains("Next actions"), "{summary}");
        assert!(summary.len() < 2000, "summary must stay small for context");
    }

    #[test]
    fn a_rejected_hypothesis_records_coverage_gaps() {
        let mut session = ResearchSession::new("s1", "objective");
        session.record_rejection(RejectedHypothesisRecord {
            hypothesis: "RCE via template injection".into(),
            attack_surface: "worker rendering pipeline".into(),
            repository: "repo".into(),
            revision: "rev1".into(),
            code_examined: vec!["worker/render.go".into()],
            experiments: vec!["crafted template payload through test harness".into()],
            rationale: "templates are sandboxed; payload was rendered inert".into(),
            coverage_gaps: vec!["email template path not exercised".into()],
            rejected_at: Utc::now(),
        });
        let ledger = session.render_findings_ledger();
        assert!(ledger.contains("RCE via template injection"));
        assert!(ledger.contains("Coverage gaps"));
        assert!(
            !ledger.contains("## Confirmed findings\n\n###"),
            "no confirmed findings should be invented"
        );
    }

    /// Phase timing accumulates across calls and survives a checkpoint
    /// round-trip, and appears as one compact line in the resume summary.
    #[test]
    fn phase_timing_accumulates_and_survives_checkpoints() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut session = ResearchSession::new("s1", "objective");
        session.record_phase_time(WorkflowPhase::RepoSetup, Duration::from_secs(40));
        session.record_phase_time(WorkflowPhase::RepoSetup, Duration::from_secs(20));
        session.record_phase_time(WorkflowPhase::Reproduction, Duration::from_secs(90));
        assert_eq!(session.timing.total_secs(), 150);

        session.checkpoint(dir.path()).expect("checkpoint");
        let loaded = ResearchSession::load(dir.path()).expect("load");
        assert_eq!(loaded.timing.total_secs(), 150);
        let summary = loaded.resume_summary();
        assert!(
            summary.contains("repo_setup=60s") && summary.contains("reproduction=90s"),
            "{summary}"
        );
    }

    /// A record written before `timing` existed must still load (serde
    /// default) rather than fail or lose the session.
    #[test]
    fn a_checkpoint_without_timing_field_still_loads() {
        let mut session = ResearchSession::new("s1", "objective");
        let mut value = serde_json::to_value(&session).expect("serialize");
        value.as_object_mut().expect("object").remove("timing");
        session = serde_json::from_value(value).expect("old checkpoint must load");
        assert_eq!(session.timing.total_secs(), 0);
    }
}
