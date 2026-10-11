use serde::{Deserialize, Serialize};
use std::borrow::Cow;

use super::proof::{ProofObject, ProofVerdict};

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
    // Modern vulnerability classes
    PrototypePollution,
    GraphQlInjection,
    JwtAttack,
    WebsocketHijacking,
    SubdomainTakeover,
    CachePoisoning,
    HttpRequestSmuggling,
    OAuthMisconfiguration,
    SamlInjection,
    LdapInjectionAdvanced,
    TemplateInjection,
    EsiInjection,
    HttpResponseSplitting,
    SessionRaceCondition,
    MassAssignment,
    InsecureDirectObjectReference,
    ServerSideRequestForgery,
    ClientSideRequestForgery,
    DOMClobbering,
    PostMessageVulnerability,
    WebCacheDeception,
    HostHeaderInjection,
    PasswordResetPoisoning,
    EmailHeaderInjection,
    UnicodeNormalization,
    BufferOverflow,
    IntegerOverflow,
    FormatString,
    UseAfterFree,
    DoubleFree,
    RaceConditionFile,
    SymlinkAttack,
    HardcodedCredentials,
    WeakCryptography,
    InsufficientLogging,
    ExcessiveDataExposure,
    ImproperInputValidation,
    MissingAuthorization,
    BrokenAccessControl,
    SecurityMisconfiguration,
    VulnerableComponents,
    InsufficientMonitoring,
    ApiAbuse,
    RateLimitBypass,
    PaginationAbuse,
    FilterBypass,
    EncodingBypass,
    WafBypass,
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
            // Modern vulnerability classes
            Self::PrototypePollution => Cow::Borrowed("prototype_pollution"),
            Self::GraphQlInjection => Cow::Borrowed("graphql_injection"),
            Self::JwtAttack => Cow::Borrowed("jwt_attack"),
            Self::WebsocketHijacking => Cow::Borrowed("websocket_hijacking"),
            Self::SubdomainTakeover => Cow::Borrowed("subdomain_takeover"),
            Self::CachePoisoning => Cow::Borrowed("cache_poisoning"),
            Self::HttpRequestSmuggling => Cow::Borrowed("http_request_smuggling"),
            Self::OAuthMisconfiguration => Cow::Borrowed("oauth_misconfiguration"),
            Self::SamlInjection => Cow::Borrowed("saml_injection"),
            Self::LdapInjectionAdvanced => Cow::Borrowed("ldap_injection_advanced"),
            Self::TemplateInjection => Cow::Borrowed("template_injection"),
            Self::EsiInjection => Cow::Borrowed("esi_injection"),
            Self::HttpResponseSplitting => Cow::Borrowed("http_response_splitting"),
            Self::SessionRaceCondition => Cow::Borrowed("session_race_condition"),
            Self::MassAssignment => Cow::Borrowed("mass_assignment"),
            Self::InsecureDirectObjectReference => {
                Cow::Borrowed("insecure_direct_object_reference")
            }
            Self::ServerSideRequestForgery => Cow::Borrowed("server_side_request_forgery"),
            Self::ClientSideRequestForgery => Cow::Borrowed("client_side_request_forgery"),
            Self::DOMClobbering => Cow::Borrowed("dom_clobbering"),
            Self::PostMessageVulnerability => Cow::Borrowed("post_message_vulnerability"),
            Self::WebCacheDeception => Cow::Borrowed("web_cache_deception"),
            Self::HostHeaderInjection => Cow::Borrowed("host_header_injection"),
            Self::PasswordResetPoisoning => Cow::Borrowed("password_reset_poisoning"),
            Self::EmailHeaderInjection => Cow::Borrowed("email_header_injection"),
            Self::UnicodeNormalization => Cow::Borrowed("unicode_normalization"),
            Self::BufferOverflow => Cow::Borrowed("buffer_overflow"),
            Self::IntegerOverflow => Cow::Borrowed("integer_overflow"),
            Self::FormatString => Cow::Borrowed("format_string"),
            Self::UseAfterFree => Cow::Borrowed("use_after_free"),
            Self::DoubleFree => Cow::Borrowed("double_free"),
            Self::RaceConditionFile => Cow::Borrowed("race_condition_file"),
            Self::SymlinkAttack => Cow::Borrowed("symlink_attack"),
            Self::HardcodedCredentials => Cow::Borrowed("hardcoded_credentials"),
            Self::WeakCryptography => Cow::Borrowed("weak_cryptography"),
            Self::InsufficientLogging => Cow::Borrowed("insufficient_logging"),
            Self::ExcessiveDataExposure => Cow::Borrowed("excessive_data_exposure"),
            Self::ImproperInputValidation => Cow::Borrowed("improper_input_validation"),
            Self::MissingAuthorization => Cow::Borrowed("missing_authorization"),
            Self::BrokenAccessControl => Cow::Borrowed("broken_access_control"),
            Self::SecurityMisconfiguration => Cow::Borrowed("security_misconfiguration"),
            Self::VulnerableComponents => Cow::Borrowed("vulnerable_components"),
            Self::InsufficientMonitoring => Cow::Borrowed("insufficient_monitoring"),
            Self::ApiAbuse => Cow::Borrowed("api_abuse"),
            Self::RateLimitBypass => Cow::Borrowed("rate_limit_bypass"),
            Self::PaginationAbuse => Cow::Borrowed("pagination_abuse"),
            Self::FilterBypass => Cow::Borrowed("filter_bypass"),
            Self::EncodingBypass => Cow::Borrowed("encoding_bypass"),
            Self::WafBypass => Cow::Borrowed("waf_bypass"),
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
    /// The structured proof backing this finding.
    ///
    /// Added after the fact with [`Finding::attach_proof`]. It is `None` for a
    /// fresh observation, and [`Finding::advance`] refuses to walk a finding
    /// from `ProgramCheck` into `ConfirmedFinding` without one that
    /// verifies — that transition is the point at which a finding starts being
    /// reported as real, so it is the point where the evidence gate belongs.
    #[serde(default)]
    pub proof: Option<ProofObject>,
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
            proof: None,
        }
    }

    /// Attach the structured proof backing this finding.
    pub fn attach_proof(&mut self, proof: ProofObject) {
        self.proof = Some(proof);
    }

    /// Verify this finding's proof *on its own terms* — that the required
    /// evidence is present and internally consistent.
    ///
    /// This is the weaker check, suitable for gating a lifecycle transition
    /// where no session transcript is in scope. It does **not** confirm the
    /// request was ever sent; use [`Finding::is_reportable`] for that.
    pub fn verify_proof(&self) -> ProofVerdict {
        match self.proof.as_ref() {
            Some(proof) => proof.verify(),
            None => ProofVerdict {
                verified: false,
                defects: vec![super::proof::ProofDefect::MissingTestExchange],
                observations: vec!["no proof object was attached to this finding".to_string()],
            },
        }
    }

    /// Whether this finding may be treated as reportable, checked against the
    /// exchanges actually observed in the session.
    ///
    /// Fail-closed: unverified findings are never reportable, and a finding
    /// with no proof at all is not reportable.
    pub fn is_reportable(&self, observed: &[super::proof::HttpExchange]) -> bool {
        match self.proof.as_ref() {
            Some(proof) => proof.verify_against(observed).verified,
            None => false,
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

        // The evidence gate. `ConfirmedFinding` is the stage at which a finding
        // stops being a hypothesis and starts being reported as real, so it is
        // the last point at which a missing proof can be caught. Requiring a
        // verified proof here is what stops "found SQLi in /search" from being
        // promoted on the strength of pattern-matching a single response.
        //
        // This checks the proof on its own terms; `advance` is synchronous and
        // has no access to the session. Callers confirm the request was really
        // sent via `is_reportable` at report time, which is the stricter,
        // history-backed check.
        if next_stage == FindingStage::ConfirmedFinding {
            let verdict = self.verify_proof();
            if !verdict.verified {
                let reasons: Vec<&str> = verdict.defects.iter().map(|d| d.as_str()).collect();
                return Err(format!(
                    "Refusing to confirm finding '{}': proof did not verify ({})",
                    self.id,
                    reasons.join("; ")
                ));
            }
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
    use super::super::proof::{BaselineComparison, ProofStep};
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

    // --- evidence gate -----------------------------------------------------

    fn exchange(
        method: &str,
        url: &str,
        status: u16,
        body: &str,
    ) -> super::super::proof::HttpExchange {
        super::super::proof::HttpExchange {
            method: method.to_string(),
            url: url.to_string(),
            request_body: None,
            status,
            response_headers: Vec::new(),
            response_body: Some(body.to_string()),
            source_tool: "httpflow".to_string(),
        }
    }

    /// A finding with no proof attached must not be reportable, and must not
    /// be promotable to `ConfirmedFinding`.
    ///
    /// This is the transition that decides whether something gets reported as
    /// a real vulnerability, so it is where "no evidence" has to stop.
    #[test]
    fn a_finding_without_a_proof_cannot_be_confirmed() {
        let mut finding = Finding::new(
            "f1".to_string(),
            "SQL injection in /search".to_string(),
            VulnerabilityClass::SqlInjection,
            "https://example.com".to_string(),
        );

        let verdict = finding.verify_proof();
        assert!(!verdict.verified, "a finding with no proof must not verify");

        // Walk it up to the last stage that is still permitted.
        for _ in 0..7 {
            finding
                .advance("walked forward".to_string())
                .expect("allowed");
        }
        assert_eq!(finding.stage, FindingStage::ProgramCheck);

        let err = finding
            .advance("promote to a reportable finding".to_string())
            .expect_err("a finding with no proof must not be confirmed");
        assert!(err.contains("proof did not verify"), "{err}");
        assert_eq!(
            finding.stage,
            FindingStage::ProgramCheck,
            "the stage must not move when the gate refuses"
        );
        assert!(!finding.is_reportable(&[]));
    }

    /// A complete, verified proof must let the finding through — the gate has
    /// to be able to pass a real finding, not just block everything.
    #[test]
    fn a_finding_with_a_verified_proof_can_be_confirmed() {
        let baseline = exchange("GET", "https://example.com/api/search", 403, "forbidden");
        let treatment = exchange(
            "GET",
            "https://example.com/api/search?q=%27%20OR%201=1--",
            500,
            "SQL syntax error near 'OR'",
        );
        let mut finding = Finding::new(
            "f1".to_string(),
            "SQL injection in /search".to_string(),
            VulnerabilityClass::SqlInjection,
            "https://example.com".to_string(),
        );
        finding.attach_proof(ProofObject {
            finding_id: "f1".to_string(),
            test_exchange: Some(treatment.clone()),
            reproduction: vec![ProofStep {
                action: "GET /api/search?q=%27%20OR%201=1--".to_string(),
                expected_observation: "a SQL error or a result set".to_string(),
                tool: Some("httpflow".to_string()),
            }],
            expected_behavior: "the search rejects the malformed query with 400".to_string(),
            observed_behavior: "the server returned 500 with a SQL syntax error".to_string(),
            baseline: Some(BaselineComparison::new(
                "unauthenticated",
                baseline,
                treatment.clone(),
            )),
        });

        for _ in 0..7 {
            finding
                .advance("walked forward".to_string())
                .expect("allowed");
        }
        finding
            .advance("proof verified".to_string())
            .expect("a verified proof must be allowed through the gate");
        assert_eq!(finding.stage, FindingStage::ConfirmedFinding);

        // Reportable only against a history that actually contains the request.
        assert!(finding.is_reportable(std::slice::from_ref(&treatment)));
        assert!(
            !finding.is_reportable(&[exchange("GET", "https://example.com/other", 500, "x")]),
            "a proof citing an unobserved request is not reportable"
        );
    }

    /// A proof that is internally complete but cites a request that was never
    /// sent must not be reportable.
    #[test]
    fn reportability_cross_checks_against_observed_activity() {
        let baseline = exchange("GET", "https://example.com/api/search", 403, "forbidden");
        let treatment = exchange(
            "GET",
            "https://example.com/api/search?q=1",
            500,
            "SQL error",
        );
        let mut finding = Finding::new(
            "f2".to_string(),
            "SQLi".to_string(),
            VulnerabilityClass::SqlInjection,
            "https://example.com".to_string(),
        );
        finding.attach_proof(ProofObject {
            finding_id: "f2".to_string(),
            test_exchange: Some(treatment),
            reproduction: vec![ProofStep {
                action: "GET /api/search?q=1".to_string(),
                expected_observation: "a SQL error".to_string(),
                tool: None,
            }],
            expected_behavior: "400".to_string(),
            observed_behavior: "500 with a SQL error".to_string(),
            baseline: Some(BaselineComparison::new(
                "unauthenticated",
                baseline,
                exchange(
                    "GET",
                    "https://example.com/api/search?q=1",
                    500,
                    "SQL error",
                ),
            )),
        });

        assert!(
            !finding.is_reportable(&[]),
            "an empty history means the request cannot be confirmed as sent"
        );
    }
}
