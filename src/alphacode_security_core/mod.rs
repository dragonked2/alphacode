pub mod action;
pub mod arbitration;
pub mod build_readiness;
pub mod chain;
pub mod concurrency_audit;
pub mod context;
pub mod coverage;
pub mod evidence;
pub mod finding;
pub mod hypothesis;
pub mod hypothesis_set;
pub mod knowledge;
pub mod learning;
pub mod noise;
pub mod observation;
pub mod operator;
pub mod orchestrator;
pub mod prioritization;
pub mod proof;
pub mod repro_harness;
pub mod research_session;
pub mod resources;
pub mod scope;
pub mod severity;
pub mod skill_router;
pub mod state;
pub mod trajectory;
pub mod validation;
pub mod verification_quality;
pub mod web3;
pub mod workspace;

pub use action::{Action, ActionClass, ActionSpace};
pub use arbitration::{
    ArbitratedContribution, ArbitrationVerdict, ContributionScore, VerifierAssignment,
    VerifierVerdict,
};
pub use build_readiness::{
    BuildBlocker, BuildReadiness, EvidenceTier as BuildReadinessTier, TargetLanguage,
    assess_build_readiness,
};
pub use chain::{AttackChain, ChainLink, ChainStatus};
pub use concurrency_audit::{
    AuditLanguage, ChecklistItem, LockSymmetryCandidate, LockSymmetryVerdict, SCANNER_MAX_SEVERITY,
    concurrency_checklist, scan_go_lock_symmetry,
};
pub use context::{EngagementPhase, EngagementSummary, SecurityContext, TimelineEvent};
pub use coverage::{
    CoverageStats, CoverageTracker, EndpointCoverage, TechCoverage, TestRecord, TestResult,
};
pub use evidence::{
    Evidence, EvidenceData, EvidenceItem, EvidenceKind, EvidenceMetadata, ReproductionStep,
};
pub use finding::{
    BoundaryViolation, Confidence, DuplicateStatus, Finding, FindingStage, ImpactDescription,
    ReproductionSteps, ScopeStatus, Severity, StageTransition, VulnerabilityClass,
};
pub use hypothesis::{
    EvidenceType, Hypothesis, HypothesisEvidence, HypothesisOutcome, HypothesisResult,
    HypothesisStatus,
};
pub use hypothesis_set::HypothesisSet;
pub use knowledge::{KnowledgeEntry, KnowledgeStore};
pub use learning::{ActionOutcomeStats, AfterActionReview, FailureKind, Lesson};
pub use noise::NoiseLevel;
pub use observation::{
    EvidenceGraph, EvidenceLink, EvidenceReliability, Interpretation, NormalizedFact,
    RawObservation,
};
pub use operator::{AgentAssignment, OperatorAction, OperatorDecision, SecurityOperator};
pub use orchestrator::{AdaptiveCycle, AdaptiveSnapshot};
pub use prioritization::{
    PrioritizationTarget, PrioritizedComponent, PrioritizedPlan, PrioritizerConfig,
    ReviewDepthHint, RiskFactor, RiskFactorEvidence, prioritize,
};
pub use proof::{
    BaselineComparison, HttpExchange, ObservedDifference, ProofDefect, ProofObject, ProofStep,
    ProofVerdict, StatusClass, describe_verdict, is_sensitive_path,
};
pub use repro_harness::{
    BlockingObservation, DEFAULT_TIMEOUT, ReproLanguage, ReproOutcome, ReproResult, ReproWorkspace,
    go_rwlock_fixed_template, go_rwlock_leak_template, interpret_blocking, run_bounded,
    rust_blocking_test_template,
};
pub use research_session::{
    CHECKPOINT_SCHEMA_VERSION, ComponentRef, CoverageEntry, CoverageLedger, PhaseTiming,
    RejectedHypothesisRecord, ResearchSession, ReviewDepth, ReviewedRepository, SessionLoadError,
    TOOL_FAILURE_MESSAGE_LIMIT, ToolFailureRecord, WorkflowPhase,
};
pub use resources::{ResourceLedger, TerminationState};
pub use scope::{
    AttackSurfaceSummary, DiscoveredEndpoint, DiscoveredHost, DiscoveredParameter, DiscoveredPort,
    DiscoveredSubdomain, EngagementMode, EngagementRules, LiveScope, ParameterLocation,
    ScopeObservation, ScopeVerdict, TechCategory, TechnologyFingerprint,
};
pub use severity::{
    CvssInputs, EvidenceTier as SeverityEvidenceTier, FindingReport, ReportSection,
    SeverityAssessment, SeverityAssessmentInput, category_natural_severity, ceiling_for_evidence,
};
pub use skill_router::{SecuritySkillDescriptor, SkillFamily, SkillRoute, SkillRouter};
pub use state::{
    Assumption, Contradiction, Objective, Provenance, ResourceConsumption, StrategyRecord,
    Uncertainty, VerificationStatus, WorldState,
};
pub use trajectory::{
    CachedDecision, DecisionCache, Trajectory, TrajectorySnapshot, TrajectoryStep,
};
pub use validation::{
    AdversarialReview, GateResult, QualityControl, ValidationEngine, ValidationGate,
    ValidationResult,
};
pub use verification_quality::{
    FalsePositiveDefense, NegativeHypothesis, VerificationQuality, VerificationState,
};
pub use workspace::{
    FsError, FsErrorContext, FsErrorKind, VerifiedWrite, Workspace, classify_io_error,
};

// Re-export web3 types (separate module with many types)
pub use web3::{
    AssetFlowGraph, Capability, ContractInfo, EconomicOutcome, EvidenceTier, ExploitChain,
    ExploitStep, FlowEdge, GateVerdict, InvariantViolation, ProtocolModel, ProtocolState,
    ProxyType, SignatureBindings, SuppressionReason, Web3Confidence, Web3Finding, Web3Gate,
    Web3Invariant,
};
