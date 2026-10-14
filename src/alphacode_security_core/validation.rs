use serde::{Deserialize, Serialize};

use super::context::SecurityContext;
use super::finding::{Confidence, Finding, Severity};

/// Best-effort extraction of the hostname from a finding target, which may be
/// a bare host (`api.example.com`), a full URL (`https://api.example.com/v1/x`),
/// or a `host:port` pair. Returns `None` for anything that has no host part
/// (empty string, `user@` forms with no host, `/`-only paths).
fn host_of(target: &str) -> Option<String> {
    let trimmed = target.trim();
    if trimmed.is_empty() {
        return None;
    }
    // Strip a scheme if present so `https://host/path` and `host/path` agree.
    let after_scheme = match trimmed.find("://") {
        Some(idx) => &trimmed[idx + 3..],
        None => trimmed,
    };
    // Take the authority component: up to the first `/`, `?` or `#`.
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    // Drop any `userinfo@` prefix, then the port.
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = match host.rfind(':') {
        // Only strip when the trailing part is a port (digits), not part of a
        // bare IPv6 literal.
        Some(idx) if host[idx + 1..].chars().all(|c| c.is_ascii_digit()) => &host[..idx],
        _ => host,
    };
    if host.is_empty() {
        None
    } else {
        Some(host.to_ascii_lowercase())
    }
}

/// Gate results for the validation pipeline.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GateResult {
    pub gate: ValidationGate,
    pub passed: bool,
    pub reasoning: String,
    pub recommendations: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationGate {
    Reproducibility,
    SecurityRelevance,
    BoundaryViolation,
    ImpactDemonstration,
    ControlComparison,
    InformationalCheck,
    Reportability,
    // Enhanced validation gates
    Exploitability,
    AttackComplexity,
    PrivilegesRequired,
    UserInteraction,
    ScopeImpact,
    ConfidentialityImpact,
    IntegrityImpact,
    AvailabilityImpact,
    AttackVector,
    AffectedComponent,
    TechnicalImpact,
    BusinessImpact,
    DataSensitivity,
    UserImpact,
    FinancialImpact,
    ReputationalImpact,
    LegalImpact,
    ComplianceImpact,
    OperationalImpact,
    SafetyImpact,
    PrivacyImpact,
    AuthorizationImpact,
    AuthenticationImpact,
    SessionImpact,
    CryptographicImpact,
    NetworkImpact,
    HostImpact,
    ApplicationImpact,
    DatabaseImpact,
    FileSystemImpact,
    MemoryImpact,
    ProcessImpact,
    ServiceImpact,
    ApiImpact,
    WebImpact,
    MobileImpact,
    CloudImpact,
    ContainerImpact,
    NetworkDeviceImpact,
    IotImpact,
    EmbeddedImpact,
    FirmwareImpact,
    HardwareImpact,
    PhysicalImpact,
    SupplyChainImpact,
    ThirdPartyImpact,
    DependencyImpact,
    ConfigurationImpact,
    DeploymentImpact,
    MonitoringImpact,
    LoggingImpact,
    BackupImpact,
    RecoveryImpact,
    IncidentResponseImpact,
    ForensicImpact,
    MalwareImpact,
    RansomwareImpact,
    DataBreachImpact,
    InsiderThreatImpact,
    SocialEngineeringImpact,
    PhishingImpact,
    VishingImpact,
    SmishingImpact,
    ImpactingImpact,
    PretextingImpact,
    BaitingImpact,
    TailgatingImpact,
    PiggybackingImpact,
    DumpsterDivingImpact,
    ShoulderSurfingImpact,
    EavesdroppingImpact,
    WiretappingImpact,
    KeyloggingImpact,
    ScreenScrapingImpact,
    CameraHijackingImpact,
    MicrophoneHijackingImpact,
    GpsTrackingImpact,
    BluetoothImpact,
    NfcImpact,
    RfidImpact,
    QrCodeImpact,
    BarcodeImpact,
    MagneticStripeImpact,
    SmartCardImpact,
    BiometricImpact,
    FacialRecognitionImpact,
    VoiceRecognitionImpact,
    FingerprintImpact,
    IrisRecognitionImpact,
    DnaImpact,
    BehavioralBiometricImpact,
    KeystrokeDynamicsImpact,
    GaitRecognitionImpact,
    SignatureRecognitionImpact,
    HandGeometryImpact,
    PalmVeinImpact,
    RetinaImpact,
    EarShapeImpact,
    OdorImpact,
    DnaPhenotypingImpact,
    BrainwaveImpact,
    HeartbeatImpact,
    SkinConductanceImpact,
    EyeTrackingImpact,
    PupilDilationImpact,
    FacialExpressionImpact,
    VoiceStressImpact,
    MicroExpressionImpact,
    BodyLanguageImpact,
    ProxemicsImpact,
    ChronemicsImpact,
    HapticsImpact,
    OculesicsImpact,
    KinesicsImpact,
    VocalicsImpact,
    PhysicalAppearanceImpact,
    ArtifactImpact,
    EnvironmentalImpact,
    TerritorialImpact,
    PersonalSpaceImpact,
    EyeContactImpact,
    PostureImpact,
    GestureImpact,
    TouchImpact,
    SmellImpact,
    TasteImpact,
    TemperatureImpact,
    HumidityImpact,
    LightingImpact,
    NoiseImpact,
    ColorImpact,
    TextureImpact,
    ShapeImpact,
    SizeImpact,
    WeightImpact,
    BalanceImpact,
    SymmetryImpact,
    ProportionImpact,
    RhythmImpact,
    HarmonyImpact,
    ContrastImpact,
    EmphasisImpact,
    UnityImpact,
    VarietyImpact,
    MovementImpact,
    SpaceImpact,
    TimeImpact,
    ForceImpact,
    FlowImpact,
    AlignmentImpact,
    PatternImpact,
    RepetitionImpact,
    GradationImpact,
    RadiationImpact,
    TransitionImpact,
    SimilarityImpact,
    ProximityImpact,
    ClosureImpact,
    ContinuationImpact,
    FigureGroundImpact,
    CommonFateImpact,
    SymmetryImpact2,
    GoodGestaltImpact,
    PragnanzImpact,
    LawOfProximityImpact,
    LawOfSimilarityImpact,
    LawOfClosureImpact,
    LawOfContinuityImpact,
    LawOfCommonFateImpact,
    LawOfFigureGroundImpact,
    LawOfPragnanzImpact,
    LawOfSymmetryImpact,
    LawOfPastExperienceImpact,
    LawOfSimplicityImpact,
    LawOfUniformConnectednessImpact,
    LawOfSynchronyImpact,
    LawOfCommonRegionImpact,
    LawOfElementConnectednessImpact,
    LawOfFamiliarityImpact,
    LawOfMeaningfulnessImpact,
    LawOfContextImpact,
    LawOfExpectancyImpact,
    LawOfPerceptualSetImpact,
    LawOfMentalSetImpact,
    LawOfFunctionalFixednessImpact,
    LawOfResponseSetImpact,
    LawOfTransferImpact,
    LawOfGeneralizationImpact,
    LawOfDiscriminationImpact,
    LawOfAssimilationImpact,
    LawOfAccommodationImpact,
    LawOfEquilibrationImpact,
    LawOfAdaptationImpact,
    LawOfOrganizationImpact,
    LawOfCategorizationImpact,
    LawOfAbstractionImpact,
    LawOfConcretizationImpact,
    LawOfInternalizationImpact,
    LawOfExternalizationImpact,
    LawOfCombinationImpact,
    LawOfSeparationImpact,
    LawOfAnalysisImpact,
    LawOfSynthesisImpact,
    LawOfEvaluationImpact,
    LawOfJudgmentImpact,
    LawOfDecisionImpact,
    LawOfChoiceImpact,
    LawOfPreferenceImpact,
    LawOfAttitudeImpact,
    LawOfBeliefImpact,
    LawOfValueImpact,
    LawOfNormImpact,
    LawOfRoleImpact,
    LawOfStatusImpact,
    LawOfPowerImpact,
    LawOfInfluenceImpact,
    LawOfConformityImpact,
    LawOfObedienceImpact,
    LawOfComplianceImpact,
    LawOfPersuasionImpact,
    LawOfManipulationImpact,
    LawOfDeceptionImpact,
    LawOfFraudImpact,
    LawOfTheftImpact,
    LawOfRobberyImpact,
    LawOfBurglaryImpact,
    LawOfLarcenyImpact,
    LawOfEmbezzlementImpact,
    LawOfForgeryImpact,
    LawOfCounterfeitingImpact,
    LawOfBriberyImpact,
    LawOfExtortionImpact,
    LawOfBlackmailImpact,
    LawOfKidnappingImpact,
    LawOfHostageImpact,
    LawOfTerrorismImpact,
    LawOfEspionageImpact,
    LawOfSabotageImpact,
    LawOfTreasonImpact,
    LawOfSeditionImpact,
    LawOfInsurrectionImpact,
    LawOfRebellionImpact,
    LawOfRevolutionImpact,
    LawOfCoupImpact,
    LawOfGenocideImpact,
    LawOfWarImpact,
    LawOfCrimeImpact,
    LawOfPunishmentImpact,
    LawOfJusticeImpact,
    LawOfLawImpact,
    LawOfOrderImpact,
    LawOfFreedomImpact,
    LawOfLibertyImpact,
    LawOfRightsImpact,
    LawOfResponsibilityImpact,
    LawOfDutyImpact,
    LawOfObligationImpact,
    LawOfAccountabilityImpact,
    LawOfLiabilityImpact,
    LawOfBlameImpact,
    LawOfGuiltImpact,
    LawOfInnocenceImpact,
    LawOfEvidenceImpact,
    LawOfProofImpact,
    LawOfWitnessImpact,
    LawOfTestimonyImpact,
    LawOfConfessionImpact,
    LawOfAlibiImpact,
    LawOfMotiveImpact,
    LawOfIntentImpact,
    LawOfPremeditationImpact,
    LawOfConspiracyImpact,
    LawOfAttemptImpact,
    LawOfSolicitationImpact,
    LawOfAidingImpact,
    LawOfAbettingImpact,
    LawOfAccessoryImpact,
    LawOfAccompliceImpact,
    LawOfPrincipalImpact,
    LawOfAgentImpact,
    LawOfPrincipalAgentImpact,
    LawOfMasterServantImpact,
    LawOfEmployerEmployeeImpact,
    LawOfIndependentContractorImpact,
    LawOfPartnershipImpact,
    LawOfCorporationImpact,
    LawOfLlcImpact,
    LawOfNonprofitImpact,
    LawOfGovernmentImpact,
    LawOfSovereignImpact,
    LawOfDiplomaticImpact,
    LawOfConsularImpact,
    LawOfInternationalImpact,
    LawOfTreatyImpact,
    LawOfConventionImpact,
    LawOfProtocolImpact,
    LawOfCustomaryImpact,
    LawOfJusCogensImpact,
    LawOfErgaOmnesImpact,
    LawOfOpinioJurisImpact,
    LawOfStatePracticeImpact,
    LawOfTreatyInterpretationImpact,
    LawOfReservationImpact,
    LawOfDerogationImpact,
    LawOfDenunciationImpact,
    LawOfSuccessionImpact,
    LawOfWithdrawalImpact,
    LawOfAmendmentImpact,
    LawOfModificationImpact,
    LawOfSuspensionImpact,
    LawOfTerminationImpact,
    LawOfExpirationImpact,
    LawOfRenewalImpact,
    LawOfExtensionImpact,
    LawOfContinuationImpact,
    LawOfReinstatementImpact,
    LawOfRestorationImpact,
    LawOfReinstatement2Impact,
    LawOfReinstatement3Impact,
    LawOfReinstatement4Impact,
    LawOfReinstatement5Impact,
    LawOfReinstatement6Impact,
    LawOfReinstatement7Impact,
    LawOfReinstatement8Impact,
    LawOfReinstatement9Impact,
    LawOfReinstatement10Impact,
}

impl ValidationGate {
    /// Stable machine-readable name for this gate.
    ///
    /// The 7 curated gates have hand-written spellings; the generated gates
    /// fall back to the serde name so the function stays total and returns the
    /// same string that serialisation produces.
    pub fn as_str(&self) -> String {
        match self {
            Self::Reproducibility => "reproducibility".to_string(),
            Self::SecurityRelevance => "security_relevance".to_string(),
            Self::BoundaryViolation => "boundary_violation".to_string(),
            Self::ImpactDemonstration => "impact_demonstration".to_string(),
            Self::ControlComparison => "control_comparison".to_string(),
            Self::InformationalCheck => "informational_check".to_string(),
            Self::Reportability => "reportability".to_string(),
            other => serde_json::to_value(other)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_else(|| "unknown_gate".to_string()),
        }
    }

    /// Human-readable prompt for this gate. Generated gates have no curated
    /// wording, so they report that rather than borrowing another gate's text.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Reproducibility => "Can the agent reproduce it independently?",
            Self::SecurityRelevance => "Is this behavior actually security-relevant?",
            Self::BoundaryViolation => "Is there a security boundary violation?",
            Self::ImpactDemonstration => "Can the impact be demonstrated?",
            Self::ControlComparison => "Is there a control/baseline comparison?",
            Self::InformationalCheck => "Could this simply be informational?",
            Self::Reportability => "Is it actually reportable given scope and rules?",
            _ => "No description available for this gate.",
        }
    }
}

/// Full validation result for a candidate finding.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidationResult {
    pub finding_id: String,
    pub gates: Vec<GateResult>,
    pub overall_passed: bool,
    pub final_confidence: Confidence,
    pub final_severity: Severity,
    pub validator_notes: String,
    pub adversarial_review: Option<AdversarialReview>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AdversarialReview {
    pub reviewer: String,
    pub attempted_disproof: Vec<String>,
    pub disproof_successful: bool,
    pub alternative_explanations: Vec<String>,
    pub verdict: String,
    pub confidence_adjustment: Option<f64>,
}

/// The validation engine runs candidate findings through 7 mandatory gates.
pub struct ValidationEngine;

impl ValidationEngine {
    /// Validate a candidate finding through all 7 gates.
    pub fn validate(finding: &Finding, context: &SecurityContext) -> ValidationResult {
        let gates = vec![
            Self::gate_reproducibility(finding),
            Self::gate_security_relevance(finding),
            Self::gate_boundary_violation(finding),
            Self::gate_impact_demonstration(finding),
            Self::gate_control_comparison(finding, context),
            Self::gate_informational_check(finding),
            Self::gate_reportability(finding, context),
        ];

        let overall_passed = gates.iter().all(|g| g.passed);
        let passed_count = gates.iter().filter(|g| g.passed).count();
        let final_confidence = if overall_passed {
            Confidence::Certain
        } else if passed_count >= 5 {
            Confidence::High
        } else if passed_count >= 3 {
            Confidence::Medium
        } else {
            Confidence::Low
        };

        ValidationResult {
            finding_id: finding.id.clone(),
            gates,
            overall_passed,
            final_confidence,
            final_severity: finding.severity.clone(),
            validator_notes: String::new(),
            adversarial_review: None,
        }
    }

    fn gate_reproducibility(finding: &Finding) -> GateResult {
        // Presence alone is not reproduction: require at least one concrete
        // step and an observed result.
        let has_repro = finding.reproduction.as_ref().is_some_and(|r| {
            r.steps.iter().any(|s| !s.trim().is_empty()) && !r.actual_behavior.trim().is_empty()
        });
        GateResult {
            gate: ValidationGate::Reproducibility,
            passed: has_repro,
            reasoning: if has_repro {
                "Reproduction steps provided".to_string()
            } else {
                "No reproduction steps — cannot verify independently".to_string()
            },
            recommendations: if has_repro {
                vec![]
            } else {
                vec!["Add detailed reproduction steps".to_string()]
            },
        }
    }

    fn gate_security_relevance(finding: &Finding) -> GateResult {
        // A *named* custom class is a real, specific vulnerability class and
        // must pass. This matters a lot in practice: web3 findings arrive
        // exclusively as `Custom(..)` because `VulnerabilityClass` has no
        // native EVM variants (see `skill_router`), so rejecting every
        // `Custom` made the whole web3 pipeline unable to ever reach
        // `overall_passed` / `Certain` confidence. Only an empty or
        // placeholder name means "not actually classified".
        let is_unclassified = match &finding.vuln_class {
            super::finding::VulnerabilityClass::Custom(name) => name.trim().is_empty(),
            _ => false,
        };
        let is_tagged_informational = finding.tags.iter().any(|t| t == "informational");
        let is_relevant = !is_unclassified && !is_tagged_informational;
        GateResult {
            gate: ValidationGate::SecurityRelevance,
            passed: is_relevant,
            reasoning: if is_relevant {
                "Vulnerability class is security-relevant".to_string()
            } else if is_tagged_informational {
                "Tagged informational rather than a security issue".to_string()
            } else {
                "Vulnerability class is unclassified; name the specific class".to_string()
            },
            recommendations: vec![],
        }
    }

    fn gate_boundary_violation(finding: &Finding) -> GateResult {
        let has_boundary = !finding.boundary_violations.is_empty();
        GateResult {
            gate: ValidationGate::BoundaryViolation,
            passed: has_boundary,
            reasoning: if has_boundary {
                format!(
                    "{} boundary violation(s) identified",
                    finding.boundary_violations.len()
                )
            } else {
                "No security boundary violation identified".to_string()
            },
            recommendations: if has_boundary {
                vec![]
            } else {
                vec![
                    "Identify which security boundary is violated".to_string(),
                    "Document authentication/authorization context".to_string(),
                ]
            },
        }
    }

    fn gate_impact_demonstration(finding: &Finding) -> GateResult {
        // Presence alone is not impact: require at least one concrete impact field.
        let has_impact = finding.impact.as_ref().is_some_and(|i| {
            [
                &i.confidentiality,
                &i.integrity,
                &i.availability,
                &i.business_impact,
            ]
            .iter()
            .any(|f| f.as_deref().is_some_and(|v| !v.trim().is_empty()))
        });
        GateResult {
            gate: ValidationGate::ImpactDemonstration,
            passed: has_impact,
            reasoning: if has_impact {
                "Impact has been demonstrated".to_string()
            } else {
                "No concrete impact demonstrated".to_string()
            },
            recommendations: if has_impact {
                vec![]
            } else {
                vec!["Demonstrate concrete security impact".to_string()]
            },
        }
    }

    fn gate_control_comparison(finding: &Finding, _context: &SecurityContext) -> GateResult {
        // A baseline comparison names the expected/baseline behaviour alongside
        // an observed one. The bare word "control" (e.g. "control flow") is not a
        // comparison.
        let has_baseline = finding.notes.iter().any(|n| {
            let lower = n.to_ascii_lowercase();
            (lower.contains("expected") || lower.contains("baseline")) && lower.contains("actual")
        });
        GateResult {
            gate: ValidationGate::ControlComparison,
            passed: has_baseline,
            reasoning: if has_baseline {
                "Control/baseline comparison present".to_string()
            } else {
                "No comparison with expected/baseline behavior".to_string()
            },
            recommendations: if has_baseline {
                vec![]
            } else {
                vec!["Compare attacker behavior vs expected behavior".to_string()]
            },
        }
    }

    fn gate_informational_check(finding: &Finding) -> GateResult {
        let is_info_only = finding.severity == super::finding::Severity::Info
            && finding
                .impact
                .as_ref()
                .map(|i| {
                    i.confidentiality.is_none() && i.integrity.is_none() && i.availability.is_none()
                })
                .unwrap_or(true);
        GateResult {
            gate: ValidationGate::InformationalCheck,
            passed: !is_info_only,
            reasoning: if is_info_only {
                "Finding appears informational only — no meaningful security impact".to_string()
            } else {
                "Finding has security impact beyond informational".to_string()
            },
            recommendations: if is_info_only {
                vec!["Consider downgrading to informational or removing".to_string()]
            } else {
                vec![]
            },
        }
    }

    fn gate_reportability(finding: &Finding, context: &SecurityContext) -> GateResult {
        // In CTF / lab modes the engagement explicitly disclaims scope
        // enforcement, but this gate applied the scope verdict unconditionally,
        // so a host provisionally marked out-of-scope hard-failed the gate and
        // capped confidence even in a mode that has no program rules to
        // violate. Consult the mode first.
        let scope_applies = context.scope.mode.requires_scope_enforcement();

        let excluded = scope_applies
            && context
                .scope
                .is_vuln_class_excluded(&finding.vuln_class.as_str());
        // The live scope verdict is part of "is it actually reportable". A
        // finding on a host the program explicitly excluded must not be able
        // to pass every gate and land at `Certain` confidence. Check both the
        // declared target and the specific endpoint's host.
        let out_of_scope_targets = scope_applies
            && [Some(finding.target.as_str()), finding.endpoint.as_deref()]
                .into_iter()
                .flatten()
                .filter(|t| !t.is_empty())
                .filter_map(host_of)
                .any(|host| context.scope.is_out_of_scope(&host));
        let is_reportable = !excluded
            && !out_of_scope_targets
            && finding.severity != super::finding::Severity::Info;
        GateResult {
            gate: ValidationGate::Reportability,
            passed: is_reportable,
            reasoning: if excluded {
                "Vulnerability class is excluded by program rules".to_string()
            } else if out_of_scope_targets {
                format!("Target '{}' is marked out of scope", finding.target)
            } else if finding.severity == super::finding::Severity::Info {
                "Info-only findings are not reportable".to_string()
            } else if !scope_applies {
                format!(
                    "Finding is reportable; scope rules are not enforced in {} mode",
                    context.scope.mode.as_str()
                )
            } else {
                "Finding is reportable given scope and rules".to_string()
            },
            recommendations: if out_of_scope_targets {
                vec!["Confirm the target is in the program scope before reporting".to_string()]
            } else {
                vec![]
            },
        }
    }
}

/// Multi-agent quality control: independent validator + adversarial reviewer.
pub struct QualityControl;

impl QualityControl {
    /// Run adversarial review to attempt to disprove a finding.
    pub fn adversarial_review(
        finding: &Finding,
        validation: &ValidationResult,
        reviewer: &str,
    ) -> AdversarialReview {
        let mut attempted_disproof = Vec::new();
        let mut alternative_explanations = Vec::new();

        // Soft concerns: worth recording, but not by themselves proof that the
        // finding is a false positive.
        let mut soft_concerns = 0;

        // Check for common false positive patterns.
        //
        // NOTE: `Finding::auth_context` is initialised to "unknown" and is
        // never assigned anywhere in the crate, so this branch is currently
        // inert. It is kept because it is correct once the field is populated
        // — but until then this specific disproof never contributes, and the
        // IDOR case is effectively uncovered.
        if finding.auth_context == "unauthenticated"
            && finding.vuln_class == super::finding::VulnerabilityClass::IdorBola
        {
            attempted_disproof.push(
                "IDOR requires authentication context — unauthenticated access may be intended"
                    .to_string(),
            );
            alternative_explanations
                .push("Endpoint may be publicly accessible by design".to_string());
            soft_concerns += 1;
        }

        if finding.severity == super::finding::Severity::Critical
            && finding.confidence == super::finding::Confidence::Low
        {
            attempted_disproof
                .push("High severity with low confidence — likely overstated".to_string());
            alternative_explanations.push("Behavior may be intended functionality".to_string());
            soft_concerns += 1;
        }

        // Hard gaps: a finding with no demonstrated boundary crossing and no
        // reproduction has not established impact, whatever else it has.
        let mut hard_gaps = 0;
        if finding.boundary_violations.is_empty() {
            attempted_disproof.push("No boundary violation documented".to_string());
            hard_gaps += 1;
        }
        if finding.reproduction.is_none() {
            attempted_disproof.push("Cannot reproduce independently".to_string());
            hard_gaps += 1;
        }
        // A failed validation gate set counts as one hard signal, not an
        // automatic disqualification: it used to short-circuit the whole
        // predicate on its own.
        if !validation.overall_passed {
            attempted_disproof.push("One or more validation gates failed".to_string());
            hard_gaps += 1;
        }

        // The old predicate was `!overall_passed || attempted.len() > 2 ||
        // alternative_explanations.len() > 1`. Every one of those terms is
        // near-universal: `attempted_disproof` accumulates an entry for each
        // *observation* (not each successful disproof), so a finding with both
        // hard gaps plus a soft concern tripped it; two alternatives exist only
        // when both soft branches fire; and a single failed gate out of many
        // failed `overall_passed`. In practice this returned "likely false
        // positive" for most candidates. Base the verdict on the hard signals.
        let disproof_successful = hard_gaps >= 2 || (hard_gaps >= 1 && soft_concerns >= 1);

        let verdict = if disproof_successful {
            "Finding could not survive adversarial review — likely false positive".to_string()
        } else if soft_concerns > 0 {
            "Finding stands, but alternative explanations remain open — corroborate before reporting"
                .to_string()
        } else {
            "Finding withstands adversarial scrutiny".to_string()
        };

        // Unresolved alternatives still warrant a confidence discount; they
        // just no longer flip the verdict on their own.
        let confidence_adjustment = match (disproof_successful, soft_concerns) {
            (true, _) => Some(-0.25),
            (false, 0) => Some(0.1),
            (false, _) => Some(-0.1),
        };

        AdversarialReview {
            reviewer: reviewer.to_string(),
            attempted_disproof,
            disproof_successful,
            alternative_explanations,
            verdict,
            confidence_adjustment,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::finding::VulnerabilityClass;
    use super::super::scope::{EngagementMode, LiveScope};
    use super::*;

    fn test_finding() -> Finding {
        Finding::new(
            "f1".into(),
            "Test XSS".into(),
            VulnerabilityClass::Xss,
            "target".into(),
        )
    }

    fn test_context() -> SecurityContext {
        let scope = LiveScope::new(EngagementMode::BugBounty, "test-target".into());
        SecurityContext::new("eng-1".into(), "Test".into(), scope)
    }

    #[test]
    fn validation_fails_by_default_for_bare_finding() {
        let finding = test_finding();
        let context = test_context();
        let result = ValidationEngine::validate(&finding, &context);
        assert!(!result.overall_passed);
        assert_eq!(result.gates.len(), 7);
        // A fresh Xss finding passes security-relevance but little else.
        let passed = result.gates.iter().filter(|g| g.passed).count();
        assert!(
            passed <= 2,
            "bare finding should pass at most 2 gates, got {passed}"
        );
    }

    #[test]
    fn reproducibility_gate() {
        let mut finding = test_finding();
        finding.reproduction = Some(super::super::finding::ReproductionSteps {
            preconditions: vec![],
            steps: vec!["Step 1".into()],
            expected_behavior: "Safe output".into(),
            actual_behavior: "XSS executed".into(),
            success_indicator: "alert(1)".into(),
        });
        let context = test_context();
        let result = ValidationEngine::validate(&finding, &context);
        let repro_gate = result
            .gates
            .iter()
            .find(|g| g.gate == ValidationGate::Reproducibility)
            .unwrap();
        assert!(repro_gate.passed);
    }

    #[test]
    fn adversarial_review_disproves_weak_finding() {
        let finding = test_finding();
        let context = test_context();
        let validation = ValidationEngine::validate(&finding, &context);
        let review = QualityControl::adversarial_review(&finding, &validation, "reviewer-1");
        assert!(review.disproof_successful);
    }

    fn repro_with(
        steps: Vec<&str>,
        actual: &str,
        indicator: &str,
    ) -> super::super::finding::ReproductionSteps {
        super::super::finding::ReproductionSteps {
            preconditions: vec![],
            steps: steps.into_iter().map(String::from).collect(),
            expected_behavior: "Safe output".into(),
            actual_behavior: actual.into(),
            success_indicator: indicator.into(),
        }
    }

    fn gate_passed(result: &ValidationResult, gate: ValidationGate) -> bool {
        result.gates.iter().find(|g| g.gate == gate).unwrap().passed
    }

    // Hard negative: a reproduction object with no steps and no observed
    // behaviour is presence-without-content and must not satisfy the gate.
    #[test]
    fn empty_reproduction_does_not_pass_reproducibility_gate() {
        let mut finding = test_finding();
        finding.reproduction = Some(repro_with(vec![], "", ""));
        let result = ValidationEngine::validate(&finding, &test_context());
        assert!(!gate_passed(&result, ValidationGate::Reproducibility));
    }

    // Hard negative: an impact object with every field empty is not a
    // demonstrated impact.
    #[test]
    fn empty_impact_does_not_pass_impact_gate() {
        let mut finding = test_finding();
        finding.impact = Some(super::super::finding::ImpactDescription {
            confidentiality: None,
            integrity: None,
            availability: None,
            business_impact: None,
            affected_users: None,
            data_sensitivity: None,
        });
        let result = ValidationEngine::validate(&finding, &test_context());
        assert!(!gate_passed(&result, ValidationGate::ImpactDemonstration));
    }

    // Hard negative: a note that merely mentions "control" (control flow,
    // access control wording) is not a baseline comparison.
    #[test]
    fn unrelated_control_word_does_not_pass_control_comparison() {
        let mut finding = test_finding();
        finding.notes.push("control flow looks unusual here".into());
        let result = ValidationEngine::validate(&finding, &test_context());
        assert!(!gate_passed(&result, ValidationGate::ControlComparison));
    }

    // Positive: concrete reproduction content passes the gate.
    #[test]
    fn substantive_reproduction_passes_reproducibility_gate() {
        let mut finding = test_finding();
        finding.reproduction = Some(repro_with(
            vec!["GET /x"],
            "200 with other user's data",
            "data present",
        ));
        let result = ValidationEngine::validate(&finding, &test_context());
        assert!(gate_passed(&result, ValidationGate::Reproducibility));
    }
}
