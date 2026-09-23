use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// Finding lifecycle stages.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FindingStage {
    Observation,
    Hypothesis,
    Candidate,
    Reproduction,
    Validation,
    ImpactConfirmation,
    DuplicateAnalysis,
    ProgramCheck,
    ConfirmedFinding,
    Report,
}

impl FindingStage {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Observation => "observation",
            Self::Hypothesis => "hypothesis",
            Self::Candidate => "candidate",
            Self::Reproduction => "reproduction",
            Self::Validation => "validation",
            Self::ImpactConfirmation => "impact_confirmation",
            Self::DuplicateAnalysis => "duplicate_analysis",
            Self::ProgramCheck => "program_check",
            Self::ConfirmedFinding => "confirmed_finding",
            Self::Report => "report",
        }
    }

    pub fn can_advance_to(&self, next: &FindingStage) -> bool {
        matches!(
            (self, next),
            (Self::Observation, Self::Hypothesis)
                | (Self::Hypothesis, Self::Candidate)
                | (Self::Candidate, Self::Reproduction)
                | (Self::Reproduction, Self::Validation)
                | (Self::Validation, Self::ImpactConfirmation)
                | (Self::ImpactConfirmation, Self::DuplicateAnalysis)
                | (Self::DuplicateAnalysis, Self::ProgramCheck)
                | (Self::ProgramCheck, Self::ConfirmedFinding)
                | (Self::ConfirmedFinding, Self::Report)
        )
    }
}

/// Severity assessment.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    #[default]
    Info,
    Low,
    Medium,
    High,
    Critical,
}

impl Severity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Critical => "critical",
        }
    }

    pub fn numeric_value(&self) -> u8 {
        match self {
            Self::Info => 0,
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
            Self::Critical => 4,
        }
    }
}

/// Vulnerability class categories.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulnerabilityClass {
    SqlInjection,
    #[serde(rename = "nosql_injection")]
    NoSqlInjection,
    CommandInjection,
    LdapInjection,
    Xss,
    DomXss,
    Xxe,
    Ssti,
    AuthenticationBypass,
    SessionFixation,
    WeakPasswordPolicy,
    MfaBypass,
    #[serde(rename = "idor_bola")]
    IdorBola,
    Bfla,
    PrivilegeEscalation,
    CrossTenantAccess,
    BusinessLogicFlaw,
    RaceCondition,
    PaymentManipulation,
    WorkflowBypass,
    SensitiveDataExposure,
    InformationDisclosure,
    Misconfiguration,
    InsecureDeserialization,
    OpenRedirect,
    #[serde(rename = "csrf")]
    Csrf,
    CorsMisconfiguration,
    Clickjacking,
    Ssrf,
    FileUpload,
    PathTraversal,
    Lfi,
    CryptographicWeakness,
    InsecureStorage,
    Custom(String),
}

impl VulnerabilityClass {
    pub fn as_str(&self) -> Cow<'_, str> {
        match self {
            Self::SqlInjection => Cow::Borrowed("sql_injection"),
            Self::NoSqlInjection => Cow::Borrowed("nosql_injection"),
            Self::CommandInjection => Cow::Borrowed("command_injection"),
            Self::LdapInjection => Cow::Borrowed("ldap_injection"),
            Self::Xss => Cow::Borrowed("xss"),
            Self::DomXss => Cow::Borrowed("dom_xss"),
            Self::Xxe => Cow::Borrowed("xxe"),
            Self::Ssti => Cow::Borrowed("ssti"),
            Self::AuthenticationBypass => Cow::Borrowed("authentication_bypass"),
            Self::SessionFixation => Cow::Borrowed("session_fixation"),
            Self::WeakPasswordPolicy => Cow::Borrowed("weak_password_policy"),
            Self::MfaBypass => Cow::Borrowed("mfa_bypass"),
            Self::IdorBola => Cow::Borrowed("idor_bola"),
            Self::Bfla => Cow::Borrowed("bfla"),
            Self::PrivilegeEscalation => Cow::Borrowed("privilege_escalation"),
            Self::CrossTenantAccess => Cow::Borrowed("cross_tenant_access"),
            Self::BusinessLogicFlaw => Cow::Borrowed("business_logic_flaw"),
            Self::RaceCondition => Cow::Borrowed("race_condition"),
            Self::PaymentManipulation => Cow::Borrowed("payment_manipulation"),
            Self::WorkflowBypass => Cow::Borrowed("workflow_bypass"),
            Self::SensitiveDataExposure => Cow::Borrowed("sensitive_data_exposure"),
            Self::InformationDisclosure => Cow::Borrowed("information_disclosure"),
            Self::Misconfiguration => Cow::Borrowed("misconfiguration"),
            Self::InsecureDeserialization => Cow::Borrowed("insecure_deserialization"),
            Self::OpenRedirect => Cow::Borrowed("open_redirect"),
            Self::Csrf => Cow::Borrowed("csrf"),
            Self::CorsMisconfiguration => Cow::Borrowed("cors_misconfiguration"),
            Self::Clickjacking => Cow::Borrowed("clickjacking"),
            Self::Ssrf => Cow::Borrowed("ssrf"),
            Self::FileUpload => Cow::Borrowed("file_upload"),
            Self::PathTraversal => Cow::Borrowed("path_traversal"),
            Self::Lfi => Cow::Borrowed("lfi"),
            Self::CryptographicWeakness => Cow::Borrowed("cryptographic_weakness"),
            Self::InsecureStorage => Cow::Borrowed("insecure_storage"),
            Self::Custom(name) => Cow::Owned(name.clone()),
        }
    }
}

/// Confidence level in a finding.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Confidence {
    Low,
    Medium,
    High,
    Certain,
}

impl Confidence {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Certain => "certain",
        }
    }

    pub fn numeric_value(&self) -> f64 {
        match self {
            Self::Low => 0.25,
            Self::Medium => 0.50,
            Self::High => 0.75,
            Self::Certain => 1.0,
        }
    }
}

/// A security boundary that may have been violated.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoundaryViolation {
    pub boundary_type: String,
    pub description: String,
    pub evidence: String,
}

/// Impact description for a finding.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ImpactDescription {
    pub confidentiality: Option<String>,
    pub integrity: Option<String>,
    pub availability: Option<String>,
    pub business_impact: Option<String>,
    pub affected_users: Option<String>,
    pub data_sensitivity: Option<String>,
}

/// Reproduction steps for a finding.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ReproductionSteps {
    pub preconditions: Vec<String>,
    pub steps: Vec<String>,
    pub expected_behavior: String,
    pub actual_behavior: String,
    pub success_indicator: String,
}

/// A transition between finding stages.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StageTransition {
    pub from: Option<FindingStage>,
    pub to: FindingStage,
    pub timestamp: String,
    pub reason: String,
}

/// Duplicate analysis status.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DuplicateStatus {
    pub is_duplicate: bool,
    pub duplicate_of: Option<String>,
    pub confidence: f64,
    pub reasoning: String,
}

/// Scope check result for a finding.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScopeStatus {
    pub in_scope: bool,
    pub scope_reason: String,
    pub excluded_by_class: bool,
    pub requires_approval: bool,
}

/// A security finding with full lifecycle tracking.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Finding {
    pub id: String,
    pub stage: FindingStage,
    pub title: String,
    pub vuln_class: VulnerabilityClass,
    pub target: String,
    pub method: Option<String>,
    pub endpoint: Option<String>,
    pub parameter: Option<String>,
    pub severity: Severity,
    pub confidence: Confidence,
    pub auth_context: String,
    pub boundary_violations: Vec<BoundaryViolation>,
    pub impact: Option<ImpactDescription>,
    pub reproduction: Option<ReproductionSteps>,
    pub related_findings: Vec<String>,
    pub tags: Vec<String>,
    pub notes: Vec<String>,
    pub discovered_by: Option<String>,
    pub stage_history: Vec<StageTransition>,
    pub duplicate_status: Option<DuplicateStatus>,
    pub scope_status: Option<ScopeStatus>,
}

impl Finding {
    pub fn new(id: String, title: String, vuln_class: VulnerabilityClass, target: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id,
            stage: FindingStage::Observation,
            title,
            vuln_class,
            target,
            method: None,
            endpoint: None,
            parameter: None,
            severity: Severity::Info,
            confidence: Confidence::Low,
            auth_context: "unknown".to_string(),
            boundary_violations: Vec::new(),
            impact: None,
            reproduction: None,
            related_findings: Vec::new(),
            tags: Vec::new(),
            notes: Vec::new(),
            discovered_by: None,
            stage_history: vec![StageTransition {
                from: None,
                to: FindingStage::Observation,
                timestamp: now,
                reason: "Finding created".to_string(),
            }],
            duplicate_status: None,
            scope_status: None,
        }
    }

    pub fn advance(&mut self, reason: String) -> Result<(), String> {
        let next_stage = match &self.stage {
            FindingStage::Observation => FindingStage::Hypothesis,
            FindingStage::Hypothesis => FindingStage::Candidate,
            FindingStage::Candidate => FindingStage::Reproduction,
            FindingStage::Reproduction => FindingStage::Validation,
            FindingStage::Validation => FindingStage::ImpactConfirmation,
            FindingStage::ImpactConfirmation => FindingStage::DuplicateAnalysis,
            FindingStage::DuplicateAnalysis => FindingStage::ProgramCheck,
            FindingStage::ProgramCheck => FindingStage::ConfirmedFinding,
            FindingStage::ConfirmedFinding => FindingStage::Report,
            FindingStage::Report => return Err("Already in final stage".to_string()),
        };

        if !self.stage.can_advance_to(&next_stage) {
            return Err(format!(
                "Cannot advance from '{}' to '{}'",
                self.stage.as_str(),
                next_stage.as_str()
            ));
        }

        let now = chrono::Utc::now().to_rfc3339();
        self.stage_history.push(StageTransition {
            from: Some(self.stage.clone()),
            to: next_stage.clone(),
            timestamp: now,
            reason,
        });
        self.stage = next_stage;
        Ok(())
    }

    pub fn add_note(&mut self, note: String) {
        self.notes.push(note);
    }

    pub fn link_related(&mut self, finding_id: String) {
        if !self.related_findings.contains(&finding_id) {
            self.related_findings.push(finding_id);
        }
    }

    pub fn is_duplicate_of(&self, other: &Finding) -> bool {
        if self.endpoint.is_none() || other.endpoint.is_none() {
            return false;
        }
        self.vuln_class == other.vuln_class
            && self.endpoint == other.endpoint
            && self.method == other.method
            && self.parameter == other.parameter
            && self.target == other.target
    }

    pub fn priority_score(&self) -> f64 {
        let severity_weight = self.severity.numeric_value() as f64 / 4.0;
        let confidence_weight = self.confidence.numeric_value();
        let stage_weight = match self.stage {
            FindingStage::Observation => 0.1,
            FindingStage::Hypothesis => 0.2,
            FindingStage::Candidate => 0.35,
            FindingStage::Reproduction => 0.5,
            FindingStage::Validation => 0.65,
            FindingStage::ImpactConfirmation => 0.75,
            FindingStage::DuplicateAnalysis => 0.85,
            FindingStage::ProgramCheck => 0.9,
            FindingStage::ConfirmedFinding => 1.0,
            FindingStage::Report => 1.0,
        };
        let chain_weight = if self.related_findings.is_empty() {
            0.5
        } else {
            0.8
        };

        (severity_weight * 0.35
            + confidence_weight * 0.25
            + stage_weight * 0.25
            + chain_weight * 0.15)
            * 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_lifecycle_progression() {
        let mut finding = Finding::new(
            "f1".to_string(),
            "Test XSS".to_string(),
            VulnerabilityClass::Xss,
            "https://example.com".to_string(),
        );
        assert_eq!(finding.stage, FindingStage::Observation);

        finding.advance("Looks like XSS".to_string()).unwrap();
        assert_eq!(finding.stage, FindingStage::Hypothesis);

        finding
            .advance("Confirmed reflected input".to_string())
            .unwrap();
        assert_eq!(finding.stage, FindingStage::Candidate);
    }

    #[test]
    fn finding_cannot_skip_stages() {
        // can_advance_to must reject non-adjacent transitions even though
        // advance() itself only ever steps to the immediate next stage.
        assert!(!FindingStage::Observation.can_advance_to(&FindingStage::Candidate));
        assert!(!FindingStage::Observation.can_advance_to(&FindingStage::Validation));
        assert!(FindingStage::Observation.can_advance_to(&FindingStage::Hypothesis));

        let mut finding = Finding::new(
            "f1".to_string(),
            "Test".to_string(),
            VulnerabilityClass::Xss,
            "target".to_string(),
        );
        // Valid single-step advance succeeds.
        assert!(finding.advance("to hypothesis".to_string()).is_ok());
        assert_eq!(finding.stage, FindingStage::Hypothesis);

        // Advancing past the terminal stage fails.
        finding.stage = FindingStage::Report;
        assert!(finding.advance("past end".to_string()).is_err());
    }

    #[test]
    fn finding_priority_score() {
        let mut finding = Finding::new(
            "f1".to_string(),
            "Critical SQLi".to_string(),
            VulnerabilityClass::SqlInjection,
            "target".to_string(),
        );
        finding.severity = Severity::Critical;
        finding.confidence = Confidence::High;
        finding.stage = FindingStage::ConfirmedFinding;
        let score = finding.priority_score();
        assert!(
            score > 70.0,
            "Critical confirmed finding should have high score: {score}"
        );
    }

    #[test]
    fn finding_is_duplicate_of() {
        let mut f1 = Finding::new(
            "f1".to_string(),
            "XSS in search".to_string(),
            VulnerabilityClass::Xss,
            "https://example.com".to_string(),
        );
        f1.endpoint = Some("/search".to_string());
        f1.parameter = Some("q".to_string());

        let mut f2 = Finding::new(
            "f2".to_string(),
            "Reflected XSS".to_string(),
            VulnerabilityClass::Xss,
            "https://example.com".to_string(),
        );
        f2.endpoint = Some("/search".to_string());
        f2.parameter = Some("q".to_string());

        assert!(f1.is_duplicate_of(&f2));
    }

    #[test]
    fn severity_ordering() {
        assert!(Severity::Info < Severity::Low);
        assert!(Severity::Low < Severity::Medium);
        assert!(Severity::Medium < Severity::High);
        assert!(Severity::High < Severity::Critical);
    }

    #[test]
    fn confidence_values() {
        assert!((Confidence::Low.numeric_value() - 0.25).abs() < f64::EPSILON);
        assert!((Confidence::Medium.numeric_value() - 0.50).abs() < f64::EPSILON);
        assert!((Confidence::High.numeric_value() - 0.75).abs() < f64::EPSILON);
        assert!((Confidence::Certain.numeric_value() - 1.0).abs() < f64::EPSILON);
    }
}
