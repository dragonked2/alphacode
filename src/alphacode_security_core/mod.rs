pub mod chain;
pub mod context;
pub mod coverage;
pub mod evidence;
pub mod finding;
pub mod hypothesis;
pub mod knowledge;
pub mod noise;
pub mod operator;
pub mod scope;
pub mod skill_router;
pub mod validation;

pub use chain::{AttackChain, ChainLink, ChainStatus};
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
pub use knowledge::{KnowledgeEntry, KnowledgeStore};
pub use noise::NoiseLevel;
pub use operator::{AgentAssignment, OperatorAction, OperatorDecision, SecurityOperator};
pub use scope::{
    AttackSurfaceSummary, DiscoveredEndpoint, DiscoveredHost, DiscoveredParameter, DiscoveredPort,
    DiscoveredSubdomain, EngagementMode, EngagementRules, LiveScope, ParameterLocation,
    ScopeObservation, ScopeVerdict, TechCategory, TechnologyFingerprint,
};
pub use skill_router::{SecuritySkillDescriptor, SkillFamily, SkillRoute, SkillRouter};
pub use validation::{
    AdversarialReview, GateResult, QualityControl, ValidationEngine, ValidationGate,
    ValidationResult,
};
