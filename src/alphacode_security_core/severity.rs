//! Evidence-based severity assessment and report rendering.
//!
//! The motivating session described a finding as *Critical* before the
//! production-impact chain was established. This module makes that order
//! explicit: severity is a **function of demonstrated evidence**, not of the
//! vulnerability category. A code-level defect without a demonstrated impact
//! chain receives a severity ceiling appropriate to its evidence, and the
//! assessment names exactly what evidence is missing to justify going
//! higher.
//!
//! The assessment separates eight things the report must not conflate
//! ([`SeverityAssessment::assess`]):
//!
//! 1. the underlying defect,
//! 2. the conditions needed to reach it,
//! 3. the demonstrated behavior,
//! 4. the demonstrated security impact,
//! 5. plausible but unverified consequences,
//! 6. existing mitigations,
//! 7. the proposed severity and rationale,
//! 8. the remaining evidence needed for a higher rating.
//!
//! Where a program asks for CVSS, [`CvssInputs`] lets the caller record the
//! six base-metric groups individually; the score is computed from those
//! inputs rather than asserted. No program policy, bounty eligibility, or
//! production behavior is ever invented here — those fields are `None` and
//! rendered as "not established" unless the caller supplies them.

use serde::{Deserialize, Serialize};

use super::finding::{Finding, Severity, VulnerabilityClass};

/// What the available evidence actually demonstrates. The lowest tier that
/// applies caps the severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceTier {
    /// Suspicious code was seen. Nothing executed.
    PatternObserved,
    /// Relevant call paths and preconditions were traced.
    PathTraced,
    /// A test or reproduction demonstrates the specific local defect.
    Reproduced,
    /// The claimed security consequence was demonstrated separately from the
    /// local defect.
    ImpactValidated,
    /// The proposed remediation passes its regression tests.
    FixVerified,
}

impl EvidenceTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PatternObserved => "pattern_observed",
            Self::PathTraced => "path_traced",
            Self::Reproduced => "reproduced",
            Self::ImpactValidated => "impact_validated",
            Self::FixVerified => "fix_verified",
        }
    }
}

/// CVSS v3.1 base-metric groups, recorded individually. `None` means "not
/// assessed" — the renderer says so rather than guessing.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct CvssInputs {
    /// "network" | "adjacent" | "local" | "physical"
    pub attack_vector: Option<String>,
    /// "low" | "high"
    pub attack_complexity: Option<String>,
    /// "none" | "low" | "high"
    pub privileges_required: Option<String>,
    /// "none" | "required"
    pub user_interaction: Option<String>,
    /// "unchanged" | "changed"
    pub scope: Option<String>,
    /// "high" | "low" | "none"
    pub confidentiality: Option<String>,
    pub integrity: Option<String>,
    pub availability: Option<String>,
}

impl CvssInputs {
    /// Whether every base metric has been supplied. CVSS scores are only
    /// reportable when this is true; partial inputs are rendered as an
    /// incomplete vector, not scored.
    pub fn is_complete(&self) -> bool {
        self.attack_vector.is_some()
            && self.attack_complexity.is_some()
            && self.privileges_required.is_some()
            && self.user_interaction.is_some()
            && self.scope.is_some()
            && self.confidentiality.is_some()
            && self.integrity.is_some()
            && self.availability.is_some()
    }

    /// Render the vector string, with `X` for unscored metrics.
    pub fn vector_string(&self) -> String {
        let pick = |v: &Option<String>| {
            v.as_deref()
                .map(str::to_string)
                .unwrap_or_else(|| "X".to_string())
        };
        format!(
            "CVSS:3.1/AV:{}/AC:{}/PR:{}/UI:{}/S:{}/C:{}/I:{}/A:{}",
            pick(&self.attack_vector),
            pick(&self.attack_complexity),
            pick(&self.privileges_required),
            pick(&self.user_interaction),
            pick(&self.scope),
            pick(&self.confidentiality),
            pick(&self.integrity),
            pick(&self.availability),
        )
    }

    /// A conservative qualitative severity derived from the *inputs*.
    ///
    /// This is deliberately a simple, transparent mapping rather than a
    /// re-implementation of the CVSS rounding formula: it reads the metrics
    /// and says which band they suggest, and it refuses to rate above the
    /// inputs. When inputs are incomplete it returns `None`.
    pub fn qualitative(&self) -> Option<Severity> {
        if !self.is_complete() {
            return None;
        }
        let av = self.attack_vector.as_deref().unwrap_or("");
        let pr = self.privileges_required.as_deref().unwrap_or("");
        let ui = self.user_interaction.as_deref().unwrap_or("");
        let c = self.confidentiality.as_deref().unwrap_or("");
        let i = self.integrity.as_deref().unwrap_or("");
        let a = self.availability.as_deref().unwrap_or("");

        let impact = [c, i, a];
        let all_high = impact.iter().all(|&x| x == "high");
        let any_high = impact.contains(&"high");

        // Network + no privs + no UI + any high impact => critical band.
        if av == "network" && pr == "none" && ui == "none" && all_high {
            return Some(Severity::Critical);
        }
        if av == "network" && pr == "none" && ui == "none" && any_high {
            return Some(Severity::High);
        }
        if any_high || (av == "network" && pr == "none") {
            return Some(Severity::High);
        }
        if pr == "low" || av == "adjacent" || any_high {
            return Some(Severity::Medium);
        }
        Some(Severity::Low)
    }
}

/// The full, evidence-separated severity assessment.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeverityAssessment {
    /// Highest severity the available evidence supports.
    pub proposed: Severity,
    /// Ceiling imposed by the evidence tier, before any CVSS adjustment.
    pub evidence_ceiling: Severity,
    /// The defect itself, described without impact inflation.
    pub underlying_defect: String,
    /// Conditions needed to reach the defect.
    pub preconditions: Vec<String>,
    /// Behavior actually demonstrated (with evidence).
    pub demonstrated_behavior: String,
    /// Security impact actually demonstrated — empty when not established.
    pub demonstrated_impact: String,
    /// Plausible-but-unverified consequences, explicitly labeled.
    pub hypothetical_impacts: Vec<String>,
    /// Existing mitigations or recovery mechanisms.
    pub mitigations: Vec<String>,
    /// Why this severity.
    pub rationale: String,
    /// Evidence still needed to justify a higher severity.
    pub evidence_needed_for_higher: Vec<String>,
    /// CVSS inputs when a program asked for them; `None` = not assessed.
    pub cvss: Option<CvssInputs>,
    /// Any program policy context supplied by the caller. Never invented.
    pub program_policy_context: Option<String>,
}

/// The maximum severity the evidence tier supports.
///
/// The table encodes the mission's core rule: a defect that has only been
/// *seen in code* cannot be rated as though its impact were demonstrated,
/// and Critical requires an end-to-end validated impact.
pub fn ceiling_for_evidence(tier: EvidenceTier) -> Severity {
    match tier {
        // Suspicious pattern: hypothesis only. Info/Low at most.
        EvidenceTier::PatternObserved => Severity::Low,
        // Call paths traced but not executed: still code review evidence.
        EvidenceTier::PathTraced => Severity::Medium,
        // Local defect reproduced: the behavior is real, but impact may not be.
        EvidenceTier::Reproduced => Severity::High,
        // Security consequence demonstrated end to end.
        EvidenceTier::ImpactValidated => Severity::Critical,
        // Fix verified does not raise impact; it raises confidence.
        EvidenceTier::FixVerified => Severity::Critical,
    }
}

impl SeverityAssessment {
    /// Build an assessment from evidence. Fails closed in both directions:
    /// the proposed severity can never exceed the evidence ceiling, and a
    /// hypothetical impact never counts toward it.
    pub fn assess(input: &SeverityAssessmentInput) -> Self {
        let ceiling = ceiling_for_evidence(input.evidence_tier);

        // Start from the category's "natural" severity if supplied, but cap it.
        let mut proposed = input
            .category_suggested_severity
            .clone()
            .unwrap_or(Severity::Info);
        if proposed > ceiling {
            proposed = ceiling.clone();
        }
        // No demonstrated impact at all: cannot exceed Medium, even with a
        // reproduced defect, because impact is what severity measures.
        if input.demonstrated_impact.trim().is_empty() && proposed > Severity::Medium {
            proposed = Severity::Medium;
        }

        let mut rationale = format!(
            "Evidence tier: {} (ceiling {}). {}",
            input.evidence_tier.as_str(),
            ceiling.as_str(),
            input.rationale_basis
        );
        if input
            .category_suggested_severity
            .as_ref()
            .is_some_and(|s| *s > ceiling)
        {
            rationale.push_str(&format!(
                " The category-natural rating ({}) was capped by available evidence.",
                input.category_suggested_severity.as_ref().unwrap().as_str()
            ));
        }

        // CVSS, when fully assessed, can lower the rating further; its band
        // is intersected with the ceiling. It can never raise a rating.
        let cvss = input.cvss.clone();
        if let Some(inputs) = &cvss {
            if let Some(band) = inputs.qualitative() {
                if band < proposed {
                    proposed = band;
                }
            } else if !inputs.is_complete() {
                // Partially assessed: keep the vector but do not let it
                // affect the rating, and say so in the rationale.
                rationale.push_str(
                    " CVSS inputs are incomplete; the vector is recorded but not scored.",
                );
            }
        }
        Self {
            proposed,
            evidence_ceiling: ceiling,
            underlying_defect: input.underlying_defect.clone(),
            preconditions: input.preconditions.clone(),
            demonstrated_behavior: input.demonstrated_behavior.clone(),
            demonstrated_impact: input.demonstrated_impact.clone(),
            hypothetical_impacts: input.hypothetical_impacts.clone(),
            mitigations: input.mitigations.clone(),
            rationale,
            evidence_needed_for_higher: missing_evidence(input),
            cvss,
            program_policy_context: input.program_policy_context.clone(),
        }
    }
}

/// Input bundle for [`SeverityAssessment::assess`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SeverityAssessmentInput {
    pub evidence_tier: EvidenceTier,
    /// The severity the vulnerability *category* would suggest if impact
    /// were proven. Capped by evidence; `None` means "derive from evidence".
    pub category_suggested_severity: Option<Severity>,
    pub underlying_defect: String,
    pub preconditions: Vec<String>,
    pub demonstrated_behavior: String,
    pub demonstrated_impact: String,
    pub hypothetical_impacts: Vec<String>,
    pub mitigations: Vec<String>,
    pub rationale_basis: String,
    pub cvss: Option<CvssInputs>,
    pub program_policy_context: Option<String>,
}

/// What is still missing to justify a higher severity — computed from the
/// actual gaps, so "what would this need to become Critical?" always has a
/// concrete answer.
fn missing_evidence(input: &SeverityAssessmentInput) -> Vec<String> {
    let mut needed = Vec::new();
    if input.evidence_tier < EvidenceTier::Reproduced {
        needed.push(
            "a minimal reproduction demonstrating the specific defect (not just the pattern)"
                .to_string(),
        );
    }
    if input.demonstrated_impact.trim().is_empty() {
        needed.push(
            "end-to-end demonstration of the security consequence, separate from the local \
             defect"
                .to_string(),
        );
    }
    if input.evidence_tier < EvidenceTier::ImpactValidated {
        needed.push(
            "established production-impact chain: attacker reachability, preconditions, and \
             the asset actually affected"
                .to_string(),
        );
    }
    if input.mitigations.is_empty() {
        needed.push("review of existing mitigations and recovery mechanisms".to_string());
    }
    if input
        .cvss
        .as_ref()
        .map(|c| !c.is_complete())
        .unwrap_or(true)
    {
        needed.push(
            "complete CVSS base metrics if the program scores severity with CVSS".to_string(),
        );
    }
    needed
}

/// One section of the rendered report.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ReportSection {
    pub heading: String,
    pub body: String,
}

/// Everything a report needs. Fields are plain strings so the renderer works
/// from any source (finding record, session, or manual entry).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FindingReport {
    pub title: String,
    pub summary: String,
    pub affected_component: String,
    pub revision: String,
    pub technical_details: String,
    pub vulnerable_code_path: String,
    pub preconditions: Vec<String>,
    pub reproduction_steps: Vec<String>,
    pub actual_result: String,
    pub expected_result: String,
    pub evidence_and_logs: String,
    pub demonstrated_impact: String,
    pub limitations: Vec<String>,
    pub remediation: String,
    pub regression_test_result: String,
    pub severity_assessment: SeverityAssessment,
    pub scope_considerations: String,
}

impl FindingReport {
    /// Render the report as markdown sections. Required headings are fixed
    /// so every report carries the same evidence discipline; empty evidence
    /// renders as an explicit "not established" rather than being omitted.
    pub fn render(&self) -> String {
        fn section(out: &mut String, heading: &str, body: &str) {
            out.push_str(&format!("## {heading}\n\n"));
            if body.trim().is_empty() {
                out.push_str("_Not established._\n\n");
            } else {
                out.push_str(body.trim());
                out.push_str("\n\n");
            }
        }

        let mut out = String::new();
        out.push_str(&format!("# {}\n\n", self.title));
        section(&mut out, "Summary", &self.summary);
        section(
            &mut out,
            "Affected component and revision",
            &format!("{}\n\nRevision: {}", self.affected_component, self.revision),
        );
        section(&mut out, "Technical details", &self.technical_details);
        section(
            &mut out,
            "Exact vulnerable code path",
            &self.vulnerable_code_path,
        );

        let pre = self
            .preconditions
            .iter()
            .map(|p| format!("- {p}"))
            .collect::<Vec<_>>()
            .join("\n");
        section(&mut out, "Preconditions and attack prerequisites", &pre);

        let repro = self
            .reproduction_steps
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{}. {s}", i + 1))
            .collect::<Vec<_>>()
            .join("\n");
        section(&mut out, "Reproduction steps", &repro);

        section(
            &mut out,
            "Actual and expected results",
            &format!(
                "**Actual:** {}\n\n**Expected:** {}",
                empty_as_not(&self.actual_result),
                empty_as_not(&self.expected_result)
            ),
        );
        section(&mut out, "Evidence and logs", &self.evidence_and_logs);
        section(&mut out, "Demonstrated impact", &self.demonstrated_impact);

        let limits = if self.limitations.is_empty() {
            "None declared — see evidence tiers above.".to_string()
        } else {
            self.limitations
                .iter()
                .map(|l| format!("- {l}"))
                .collect::<Vec<_>>()
                .join("\n")
        };
        section(&mut out, "Limitations and unverified assumptions", &limits);

        section(&mut out, "Remediation", &self.remediation);
        section(
            &mut out,
            "Regression-test results",
            &self.regression_test_result,
        );

        // Severity rationale — always present when an assessment exists.
        let sev = &self.severity_assessment;
        let mut sev_body = format!(
            "**Proposed severity: {}** (evidence ceiling: {})\n\n{}\n\n",
            sev.proposed.as_str(),
            sev.evidence_ceiling.as_str(),
            sev.rationale
        );
        sev_body.push_str(&format!("- Underlying defect: {}\n", sev.underlying_defect));
        sev_body.push_str(&format!(
            "- Demonstrated behavior: {}\n",
            empty_as_not(&sev.demonstrated_behavior)
        ));
        sev_body.push_str(&format!(
            "- Demonstrated impact: {}\n",
            empty_as_not(&sev.demonstrated_impact)
        ));
        if !sev.hypothetical_impacts.is_empty() {
            sev_body.push_str("- Plausible but unverified:\n");
            for h in &sev.hypothetical_impacts {
                sev_body.push_str(&format!("  - {h}\n"));
            }
        }
        if !sev.mitigations.is_empty() {
            sev_body.push_str("- Existing mitigations:\n");
            for m in &sev.mitigations {
                sev_body.push_str(&format!("  - {m}\n"));
            }
        }
        sev_body.push_str("- Evidence needed for a higher rating:\n");
        for needed in &sev.evidence_needed_for_higher {
            sev_body.push_str(&format!("  - {needed}\n"));
        }
        if let Some(cvss) = &sev.cvss {
            sev_body.push_str(&format!("- CVSS vector: `{}`\n", cvss.vector_string()));
            if !cvss.is_complete() {
                sev_body.push_str("- CVSS score: _not scored (incomplete inputs)_\n");
            }
        } else {
            sev_body.push_str("- CVSS: _not assessed_\n");
        }
        match &sev.program_policy_context {
            Some(ctx) => sev_body.push_str(&format!("- Program policy context: {ctx}\n")),
            None => sev_body.push_str(
                "- Program policy context: _not established (no program policy was provided)_\n",
            ),
        }
        section(&mut out, "Severity rationale", &sev_body);

        section(
            &mut out,
            "Scope and testing-policy considerations",
            &self.scope_considerations,
        );
        out
    }

    /// Build a report directly from a [`Finding`]'s record, so a rendered
    /// report is tied to the same evidence that gated the finding.
    pub fn from_finding(
        finding: &Finding,
        revision: &str,
        assessment: SeverityAssessment,
        summary: impl Into<String>,
        evidence_and_logs: impl Into<String>,
    ) -> Self {
        let (repro_steps, actual, expected) = match &finding.reproduction {
            Some(r) => (
                r.steps.clone(),
                r.actual_behavior.clone(),
                r.expected_behavior.clone(),
            ),
            None => (Vec::new(), String::new(), String::new()),
        };
        Self {
            title: finding.title.clone(),
            summary: summary.into(),
            affected_component: finding.target.clone(),
            revision: revision.to_string(),
            technical_details: finding.notes.join("\n"),
            vulnerable_code_path: finding
                .endpoint
                .clone()
                .unwrap_or_else(|| finding.target.clone()),
            preconditions: {
                let mut p = finding
                    .reproduction
                    .as_ref()
                    .map(|r| r.preconditions.clone())
                    .unwrap_or_default();
                p.push(format!("auth context: {}", finding.auth_context));
                p
            },
            reproduction_steps: repro_steps,
            actual_result: actual,
            expected_result: expected,
            evidence_and_logs: evidence_and_logs.into(),
            demonstrated_impact: assessment.demonstrated_impact.clone(),
            limitations: assessment.hypothetical_impacts.clone(),
            remediation: finding
                .notes
                .iter()
                .find(|n| n.starts_with("remediation:"))
                .cloned()
                .unwrap_or_default(),
            regression_test_result: String::new(),
            severity_assessment: assessment,
            scope_considerations: finding
                .scope_status
                .as_ref()
                .map(|s| format!("in scope: {} — {}", s.in_scope, s.scope_reason))
                .unwrap_or_else(|| "Scope not recorded for this finding.".to_string()),
        }
    }
}

fn empty_as_not(text: &str) -> String {
    if text.trim().is_empty() {
        "_not established_".to_string()
    } else {
        text.to_string()
    }
}

/// Convenience: does the finding's class suggest a high "natural" severity
/// that reviewers tend to anchor on? Used to make anchoring explicit in the
/// rationale ("the category suggests Critical, but evidence allows High").
pub fn category_natural_severity(class: &VulnerabilityClass) -> Severity {
    match class {
        VulnerabilityClass::CommandInjection
        | VulnerabilityClass::AuthenticationBypass
        | VulnerabilityClass::PrivilegeEscalation
        | VulnerabilityClass::Ssrf
        | VulnerabilityClass::SqlInjection => Severity::Critical,
        VulnerabilityClass::IdorBola
        | VulnerabilityClass::MissingAuthorization
        | VulnerabilityClass::BrokenAccessControl
        | VulnerabilityClass::InsecureDeserialization
        | VulnerabilityClass::Xxe => Severity::High,
        VulnerabilityClass::InformationDisclosure
        | VulnerabilityClass::OpenRedirect
        | VulnerabilityClass::Misconfiguration
        | VulnerabilityClass::SecurityMisconfiguration => Severity::Medium,
        _ => Severity::Medium,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_security_core::finding::{Finding, FindingStage, ReproductionSteps};

    fn input(tier: EvidenceTier) -> SeverityAssessmentInput {
        SeverityAssessmentInput {
            evidence_tier: tier,
            category_suggested_severity: Some(Severity::Critical),
            underlying_defect: "loadedKeyMu.RLock() not released on success path".into(),
            preconditions: vec!["attacker can trigger Sign()".into()],
            demonstrated_behavior: "standalone RWMutex reproduction: writer blocked 2s".into(),
            demonstrated_impact: String::new(),
            hypothetical_impacts: vec![
                "production signer might block key rotation (not demonstrated)".into(),
            ],
            mitigations: Vec::new(),
            rationale_basis: "reproduction demonstrates the lock leak in isolation".into(),
            cvss: None,
            program_policy_context: None,
        }
    }

    /// The mission's headline rule: a Critical-sounding category with only a
    /// pattern observed must not come out Critical.
    #[test]
    fn a_pattern_alone_cannot_produce_critical() {
        let assessment = SeverityAssessment::assess(&input(EvidenceTier::PatternObserved));
        assert_eq!(assessment.proposed, Severity::Low);
        assert!(
            assessment.rationale.contains("capped"),
            "{}",
            assessment.rationale
        );
        assert!(assessment.proposed < Severity::Critical);
    }

    /// Reproducing the local defect without demonstrating impact must not be
    /// called Critical — this is the exact regression from the session.
    #[test]
    fn a_reproduced_defect_without_demonstrated_impact_is_capped() {
        // Reproduced + some impact: capped at High, never Critical.
        let mut inp = input(EvidenceTier::Reproduced);
        inp.demonstrated_impact = "Sign() returns with the read lock held".into();
        let assessment = SeverityAssessment::assess(&inp);
        assert_eq!(
            assessment.proposed,
            Severity::High,
            "category Critical + reproduced defect must cap at High"
        );
        // No demonstrated impact at all => never above Medium regardless of tier.
        let mut inp = input(EvidenceTier::ImpactValidated);
        inp.demonstrated_impact = String::new();
        let assessment = SeverityAssessment::assess(&inp);
        assert_eq!(assessment.proposed, Severity::Medium);
    }

    #[test]
    fn validated_impact_can_justify_critical() {
        let mut inp = input(EvidenceTier::ImpactValidated);
        inp.demonstrated_impact =
            "attacker-triggered Sign() blocks key rotation across the signer fleet".into();
        inp.mitigations = vec!["none identified".into()];
        inp.cvss = Some(CvssInputs {
            attack_vector: Some("network".into()),
            attack_complexity: Some("low".into()),
            privileges_required: Some("none".into()),
            user_interaction: Some("none".into()),
            scope: Some("unchanged".into()),
            confidentiality: Some("high".into()),
            integrity: Some("high".into()),
            availability: Some("high".into()),
        });
        let assessment = SeverityAssessment::assess(&inp);
        assert_eq!(assessment.proposed, Severity::Critical);
        assert!(
            assessment.evidence_needed_for_higher.is_empty(),
            "all evidence supplied, nothing should be missing: {:?}",
            assessment.evidence_needed_for_higher
        );
    }

    /// Missing evidence is surfaced, not glossed: every assessment with gaps
    /// names them.
    #[test]
    fn missing_evidence_is_always_listed() {
        let assessment = SeverityAssessment::assess(&input(EvidenceTier::PatternObserved));
        assert!(!assessment.evidence_needed_for_higher.is_empty());
        assert!(
            assessment
                .evidence_needed_for_higher
                .iter()
                .any(|e| e.contains("reproduction"))
        );
    }

    #[test]
    fn incomplete_cvss_inputs_are_recorded_but_not_scored() {
        let mut inp = input(EvidenceTier::Reproduced);
        inp.cvss = Some(CvssInputs {
            attack_vector: Some("network".into()),
            ..Default::default()
        });
        let assessment = SeverityAssessment::assess(&inp);
        let cvss = assessment.cvss.expect("recorded");
        assert!(!cvss.is_complete());
        assert!(cvss.vector_string().contains("/AC:X/"));
        assert!(assessment.rationale.contains("incomplete"));
    }

    #[test]
    fn complete_high_band_cvss_lowers_a_lower_band_rating() {
        let mut inp = input(EvidenceTier::Reproduced);
        inp.category_suggested_severity = Some(Severity::Medium);
        inp.demonstrated_impact = "some impact".into();
        // Complete CVSS inputs that map to Low.
        inp.cvss = Some(CvssInputs {
            attack_vector: Some("local".into()),
            attack_complexity: Some("high".into()),
            privileges_required: Some("high".into()),
            user_interaction: Some("required".into()),
            scope: Some("unchanged".into()),
            confidentiality: Some("low".into()),
            integrity: Some("none".into()),
            availability: Some("none".into()),
        });
        let assessment = SeverityAssessment::assess(&inp);
        // Local/high-PR/UI-required/low-C => maps to Low or Medium, <= proposed.
        assert!(assessment.proposed <= Severity::Medium, "{assessment:?}");
    }

    #[test]
    fn program_policy_is_never_invented() {
        let assessment = SeverityAssessment::assess(&input(EvidenceTier::Reproduced));
        assert!(assessment.program_policy_context.is_none());
        let report = FindingReport {
            title: "t".into(),
            summary: "s".into(),
            affected_component: "c".into(),
            revision: "r".into(),
            technical_details: String::new(),
            vulnerable_code_path: String::new(),
            preconditions: vec![],
            reproduction_steps: vec![],
            actual_result: String::new(),
            expected_result: String::new(),
            evidence_and_logs: String::new(),
            demonstrated_impact: String::new(),
            limitations: vec![],
            remediation: String::new(),
            regression_test_result: String::new(),
            severity_assessment: assessment,
            scope_considerations: String::new(),
        };
        let rendered = report.render();
        assert!(rendered.contains("not established"), "{rendered}");
        assert!(rendered.contains("no program policy was provided"));
        assert!(rendered.contains("## Severity rationale"));
        assert!(rendered.contains("## Limitations"));
        assert!(rendered.contains("## Evidence and logs"));
        assert!(rendered.contains("## Regression-test results"));
    }

    /// Reports rendered from a finding carry the same stage/evidence as the
    /// gated finding record.
    #[test]
    fn a_report_built_from_a_finding_keeps_its_evidence() {
        let mut finding = Finding::new(
            "f1".into(),
            "Leaked read lock in signer".into(),
            VulnerabilityClass::RaceCondition,
            "circlefin/arc-remote-signer".into(),
        );
        finding.stage = FindingStage::Candidate;
        finding.reproduction = Some(ReproductionSteps {
            preconditions: vec!["signer running".into()],
            steps: vec!["call Sign()".into()],
            expected_behavior: "lock released".into(),
            actual_behavior: "writer blocked".into(),
            success_indicator: "watchdog prints RESULT line".into(),
        });
        let assessment = SeverityAssessment::assess(&input(EvidenceTier::Reproduced));
        let report = FindingReport::from_finding(
            &finding,
            "abc123",
            assessment,
            "summary",
            "go run main.go output",
        );
        assert_eq!(report.revision, "abc123");
        assert_eq!(report.actual_result, "writer blocked");
        let rendered = report.render();
        assert!(rendered.contains("writer blocked"));
        assert!(rendered.contains("Reproduction steps"));
    }

    #[test]
    fn category_natural_severity_is_explicit_about_anchor() {
        assert_eq!(
            category_natural_severity(&VulnerabilityClass::CommandInjection),
            Severity::Critical
        );
        assert_eq!(
            category_natural_severity(&VulnerabilityClass::OpenRedirect),
            Severity::Medium
        );
    }
}
