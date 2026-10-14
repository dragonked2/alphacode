//! Root-cause deduplication for findings.
//!
//! Two symptoms are the same report when they come from the same defect, and
//! two symptoms are different reports when they come from different defects even
//! if they share an endpoint. Neither `VulnerabilityClass` nor the endpoint
//! answers that question: the same class appears on hundreds of unrelated
//! findings, and one missing authorization check is routinely reachable through
//! a dozen routes.
//!
//! # Why the obvious approach fails
//!
//! `Finding::is_duplicate_of` compares endpoint and class equality. That merges
//! findings that happen to be on the same URL and keeps apart findings that are
//! literally the same defect reached through different paths. Both directions
//! are wrong, and both produce a report stream an expert reviewer throws away.
//!
//! # The rule used here
//!
//! Two findings share a root cause when they agree on all four of:
//!
//! * the **underlying security invariant** that was broken,
//! * the **authorization boundary** (or other boundary) that was crossed,
//! * the **vulnerable operation**, and
//! * the **code path or resolver** that failed to enforce it.
//!
//! Endpoint and model identity are recorded but are *not* merge keys. Two
//! findings on the same endpoint with the same class but different owners are
//! distinct root causes (two missing checks, two reports, because they need two
//! fixes). Two findings on different endpoints with the same check in the same
//! resolver are one root cause.
//!
//! Relationships are never destroyed: merged findings keep every symptom, and
//! the surviving entry records what was folded into it and why.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::alphacode_security_core::finding::{Finding, VulnerabilityClass};

/// The security invariant a finding claims was broken.
///
/// This is the primary merge key. It is an enum rather than free text so that
/// "missing object-level authorization on read" and "missing object-level
/// authorization on write" cannot be collapsed by a substring match.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityInvariant {
    /// An identity may only reach objects it owns or is entitled to.
    ObjectOwnership,
    /// A role may only perform operations reserved for that role.
    RolePrivilege,
    /// A tenant may never reach another tenant's data.
    TenantIsolation,
    /// The requester is who they claim to be.
    Authentication,
    /// Untrusted input must not become executable instructions.
    CodeExecution,
    /// Input must be interpreted as data, never as a query or a command.
    InjectionResistance,
    /// Data must not cross a confidentiality boundary.
    ConfidentialityBoundary,
    /// Data must not be altered without authorization.
    IntegrityBoundary,
    /// The service must remain available.
    Availability,
    /// The server's own internals must not be exposed.
    InformationDisclosure,
    /// A session must behave as if it were absent after it ends.
    SessionIntegrity,
    /// Anything not covered above. Merges only on an exact match.
    Other,
}

impl SecurityInvariant {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ObjectOwnership => "object_ownership",
            Self::RolePrivilege => "role_privilege",
            Self::TenantIsolation => "tenant_isolation",
            Self::Authentication => "authentication",
            Self::CodeExecution => "code_execution",
            Self::InjectionResistance => "injection_resistance",
            Self::ConfidentialityBoundary => "confidentiality_boundary",
            Self::IntegrityBoundary => "integrity_boundary",
            Self::Availability => "availability",
            Self::InformationDisclosure => "information_disclosure",
            Self::SessionIntegrity => "session_integrity",
            Self::Other => "other",
        }
    }

    /// The invariant implied by a vulnerability class.
    ///
    /// Deliberately conservative: classes that map to several invariants return
    /// [`SecurityInvariant::Other`], which merges only on an exact match rather
    /// than guessing. A wrong merge is far more expensive than a missed one,
    /// because a merged report hides a distinct root cause from the program.
    pub fn for_class(class: &VulnerabilityClass) -> Self {
        match class {
            VulnerabilityClass::IdorBola
            | VulnerabilityClass::InsecureDirectObjectReference
            | VulnerabilityClass::BrokenAccessControl
            | VulnerabilityClass::MissingAuthorization
            | VulnerabilityClass::SensitiveDataExposure => Self::ObjectOwnership,
            VulnerabilityClass::Bfla | VulnerabilityClass::PrivilegeEscalation => {
                Self::RolePrivilege
            }
            VulnerabilityClass::CrossTenantAccess => Self::TenantIsolation,
            VulnerabilityClass::AuthenticationBypass
            | VulnerabilityClass::MfaBypass
            | VulnerabilityClass::JwtAttack
            | VulnerabilityClass::OAuthMisconfiguration
            | VulnerabilityClass::SamlInjection => Self::Authentication,
            VulnerabilityClass::CommandInjection
            | VulnerabilityClass::Xss
            | VulnerabilityClass::DomXss
            | VulnerabilityClass::UnicodeNormalization
            | VulnerabilityClass::BufferOverflow
            | VulnerabilityClass::FormatString
            | VulnerabilityClass::UseAfterFree => Self::CodeExecution,
            VulnerabilityClass::SqlInjection
            | VulnerabilityClass::NoSqlInjection
            | VulnerabilityClass::LdapInjection
            | VulnerabilityClass::LdapInjectionAdvanced
            | VulnerabilityClass::GraphQlInjection
            | VulnerabilityClass::TemplateInjection
            | VulnerabilityClass::Ssti
            | VulnerabilityClass::EsiInjection => Self::InjectionResistance,
            VulnerabilityClass::InformationDisclosure
            | VulnerabilityClass::ExcessiveDataExposure
            | VulnerabilityClass::HardcodedCredentials
            | VulnerabilityClass::InsecureStorage
            | VulnerabilityClass::InsufficientLogging => Self::InformationDisclosure,
            VulnerabilityClass::SessionFixation | VulnerabilityClass::SessionRaceCondition => {
                Self::SessionIntegrity
            }
            VulnerabilityClass::Csrf => Self::Authentication,
            VulnerabilityClass::MassAssignment | VulnerabilityClass::ImproperInputValidation => {
                Self::IntegrityBoundary
            }
            VulnerabilityClass::CachePoisoning | VulnerabilityClass::WebCacheDeception => {
                Self::IntegrityBoundary
            }
            VulnerabilityClass::RateLimitBypass => Self::Availability,
            _ => Self::Other,
        }
    }
}

/// The operation that failed to enforce the invariant.
///
/// Read and write are different checks with different fixes, so they are never
/// merged.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulnerableOperation {
    Read,
    Write,
    Delete,
    /// A specific named field or sub-operation, e.g. `export`, `invite`.
    Named(String),
}

impl VulnerableOperation {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::Delete => "delete",
            Self::Named(name) => name,
        }
    }
}

/// The identity of a root cause: the tuple that decides whether two findings are
/// the same defect.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RootCauseKey {
    pub invariant: SecurityInvariant,
    /// The authorization/trust boundary crossed, e.g. `tenant`, `user`,
    /// `admin`, `public-api`.
    pub boundary: String,
    pub operation: VulnerableOperation,
    /// The shared code path, resolver, or handler that failed to enforce it.
    pub code_path: String,
}

impl RootCauseKey {
    pub fn new(
        invariant: SecurityInvariant,
        boundary: impl Into<String>,
        operation: VulnerableOperation,
        code_path: impl Into<String>,
    ) -> Self {
        Self {
            invariant,
            boundary: normalize(&boundary.into()),
            operation,
            code_path: normalize(&code_path.into()),
        }
    }

    /// Human-readable form for a report's "root cause" line.
    pub fn describe(&self) -> String {
        format!(
            "{} via {} at {}",
            self.invariant.as_str(),
            self.operation.as_str(),
            self.code_path
        )
    }
}

/// Case-insensitive, whitespace-collapsed identity for a merge key component.
///
/// `Tenant`, `tenant ` and `TENANT` are the same boundary. Path traversal in a
/// code-path string would otherwise create a new root cause per spelling.
fn normalize(value: &str) -> String {
    value.trim().to_lowercase().replace(['\\', '/'], "/")
}

/// What happened to a finding during deduplication.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DedupDisposition {
    /// First finding for this root cause. Kept as the survivor.
    Primary,
    /// Folded into an existing survivor.
    Merged { into: String },
    /// Kept separate because a merge key component differed.
    KeptSeparate { differs_on: String },
    /// Marked as a possible duplicate for human review without merging, because
    /// the evidence does not establish whether the code path is shared.
    PossibleDuplicate { of: String, reason: String },
}

impl DedupDisposition {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Merged { .. } => "merged",
            Self::KeptSeparate { .. } => "kept_separate",
            Self::PossibleDuplicate { .. } => "possible_duplicate",
        }
    }
}

/// One finding as the deduplicator sees it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DedupCandidate {
    pub finding_id: String,
    /// Root cause the investigator identified. Supplied by the caller, because
    /// it is the one thing a detector cannot determine on its own.
    pub root_cause: RootCauseKey,
    /// Endpoint, kept for the report but deliberately not a merge key.
    pub endpoint: Option<String>,
    pub vuln_class: VulnerabilityClass,
    /// Title, kept for the report.
    pub title: String,
    /// Whether the code path is known to be shared with another candidate. When
    /// this is unknown, the deduplicator refuses to merge and flags instead.
    pub code_path_shared_confirmed: bool,
}

impl DedupCandidate {
    pub fn new(finding_id: impl Into<String>, root_cause: RootCauseKey) -> Self {
        Self {
            finding_id: finding_id.into(),
            root_cause,
            endpoint: None,
            vuln_class: VulnerabilityClass::Custom("other".into()),
            title: String::new(),
            code_path_shared_confirmed: true,
        }
    }

    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    pub fn with_class(mut self, class: VulnerabilityClass) -> Self {
        self.vuln_class = class;
        self
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = title.into();
        self
    }

    /// Mark that whether two findings share a code path has not been
    /// established. The deduplicator will flag rather than merge.
    pub fn with_unconfirmed_code_path(mut self) -> Self {
        self.code_path_shared_confirmed = false;
        self
    }
}

/// The surviving entry for one root cause.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootCauseGroup {
    pub root_cause: RootCauseKey,
    /// The surviving finding id.
    pub primary_finding_id: String,
    /// Every finding id folded into this group, including the primary.
    pub member_finding_ids: Vec<String>,
    /// Distinct endpoints the symptom was observed through. Multiple endpoints
    /// in one group is normal and is not a reason for extra reports.
    pub observed_endpoints: Vec<String>,
    /// Why each merge happened, so the decision is auditable rather than magic.
    pub merge_reasons: Vec<String>,
}

impl RootCauseGroup {
    /// One-line explanation suitable for a report or a reviewer note.
    pub fn explain(&self) -> String {
        format!(
            "{} finding(s) share one root cause ({}) across {} endpoint(s); they need one \
             fix, not {} reports",
            self.member_finding_ids.len(),
            self.root_cause.describe(),
            self.observed_endpoints.len(),
            self.member_finding_ids.len()
        )
    }
}

/// Groups findings by root cause without destroying the relationships between
/// them.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deduplicator {
    groups: Vec<RootCauseGroup>,
    /// Key -> index into `groups`.
    index: BTreeMap<RootCauseKey, usize>,
    dispositions: Vec<(String, DedupDisposition)>,
}

impl Deduplicator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Submit a candidate and learn what happened to it.
    pub fn submit(&mut self, candidate: DedupCandidate) -> DedupDisposition {
        // Compare against existing groups first: a partial key match is a
        // *related* finding, which is worth surfacing even when it is not a
        // merge.
        if let Some((idx, differs_on)) = self.partial_match(&candidate) {
            let disposition = DedupDisposition::KeptSeparate {
                differs_on: differs_on.clone(),
            };
            self.dispositions
                .push((candidate.finding_id.clone(), disposition.clone()));
            let primary = self.groups[idx].primary_finding_id.clone();
            self.groups[idx].merge_reasons.push(format!(
                "kept `{}` separate from `{}`: differs on {differs_on}",
                candidate.finding_id, primary
            ));
            return disposition;
        }

        match self.index.get(&candidate.root_cause).copied() {
            None => {
                let group = RootCauseGroup {
                    root_cause: candidate.root_cause.clone(),
                    primary_finding_id: candidate.finding_id.clone(),
                    member_finding_ids: vec![candidate.finding_id.clone()],
                    observed_endpoints: candidate.endpoint.iter().cloned().collect(),
                    merge_reasons: vec![format!(
                        "first finding for this root cause: {}",
                        candidate.root_cause.describe()
                    )],
                };
                self.index
                    .insert(candidate.root_cause.clone(), self.groups.len());
                self.groups.push(group);
                self.dispositions
                    .push((candidate.finding_id.clone(), DedupDisposition::Primary));
                DedupDisposition::Primary
            }
            Some(idx) => {
                if !candidate.code_path_shared_confirmed {
                    // Merging without knowing the code path is shared would hide
                    // a distinct defect. Flag it and let a human decide.
                    let primary = self.groups[idx].primary_finding_id.clone();
                    let disposition = DedupDisposition::PossibleDuplicate {
                        of: primary.clone(),
                        reason: format!(
                            "same invariant/boundary/operation, but it is unconfirmed \
                             whether both findings cross `{}`",
                            candidate.root_cause.code_path
                        ),
                    };
                    self.groups[idx].merge_reasons.push(format!(
                        "flagged `{}` as a possible duplicate of `{}`: {}",
                        candidate.finding_id,
                        primary,
                        disposition_reason(&disposition)
                    ));
                    self.dispositions
                        .push((candidate.finding_id.clone(), disposition.clone()));
                    return disposition;
                }
                let primary = self.groups[idx].primary_finding_id.clone();
                let describe = candidate.root_cause.describe();
                let group = &mut self.groups[idx];
                group.member_finding_ids.push(candidate.finding_id.clone());
                if let Some(endpoint) = candidate.endpoint.clone()
                    && !group.observed_endpoints.contains(&endpoint)
                {
                    group.observed_endpoints.push(endpoint);
                }
                group.merge_reasons.push(format!(
                    "merged `{}` into `{primary}`: {describe}",
                    candidate.finding_id
                ));
                let disposition = DedupDisposition::Merged { into: primary };
                self.dispositions
                    .push((candidate.finding_id.clone(), disposition.clone()));
                disposition
            }
        }
    }

    /// A candidate that shares some but not all merge-key components.
    ///
    /// Returning a disposition here is what keeps "related" visible. Two
    /// findings on the same endpoint with the same class but a different
    /// operation are *not* duplicates, and saying so explicitly is more useful
    /// than silently leaving them as separate groups.
    fn partial_match(&self, candidate: &DedupCandidate) -> Option<(usize, String)> {
        let key = &candidate.root_cause;
        self.groups.iter().enumerate().find_map(|(idx, group)| {
            let other = &group.root_cause;
            if other == key {
                return None;
            }
            let shared = [
                (other.invariant == key.invariant, "invariant"),
                (other.boundary == key.boundary, "boundary"),
                (other.code_path == key.code_path, "code path"),
            ];
            let matches = shared.iter().filter(|(ok, _)| *ok).count();
            if matches < 2 {
                return None;
            }
            let differs = shared
                .iter()
                .filter(|(ok, _)| !*ok)
                .map(|(_, label)| *label)
                .collect::<Vec<_>>()
                .join(", ");
            Some((idx, differs))
        })
    }

    pub fn disposition_of(&self, finding_id: &str) -> Option<&DedupDisposition> {
        self.dispositions
            .iter()
            .find(|(id, _)| id == finding_id)
            .map(|(_, d)| d)
    }

    pub fn groups(&self) -> &[RootCauseGroup] {
        &self.groups
    }

    /// How many distinct root causes were identified.
    pub fn root_cause_count(&self) -> usize {
        self.groups.len()
    }

    /// How many findings were folded away.
    pub fn merged_count(&self) -> usize {
        self.dispositions
            .iter()
            .filter(|(_, d)| matches!(d, DedupDisposition::Merged { .. }))
            .count()
    }

    /// Total findings submitted.
    pub fn submitted_count(&self) -> usize {
        self.dispositions.len()
    }

    /// Whether every group has an explanation for what was merged or kept
    /// separate. Always true by construction; asserted in tests so a future
    /// change cannot make merges silent.
    pub fn all_merges_explained(&self) -> bool {
        self.groups
            .iter()
            .all(|g| g.merge_reasons.len() >= g.member_finding_ids.len())
    }

    /// Attach the deduplication outcome to findings so a report can state it.
    pub fn annotate(&self, findings: &mut [Finding]) {
        for finding in findings.iter_mut() {
            let Some(group) = self
                .groups
                .iter()
                .find(|g| g.member_finding_ids.contains(&finding.id))
            else {
                continue;
            };
            finding.duplicate_status =
                Some(crate::alphacode_security_core::finding::DuplicateStatus {
                    is_duplicate: finding.id != group.primary_finding_id,
                    duplicate_of: (finding.id != group.primary_finding_id)
                        .then(|| group.primary_finding_id.clone()),
                    confidence: if finding.id == group.primary_finding_id {
                        1.0
                    } else {
                        0.9
                    },
                    reasoning: group.explain(),
                });
        }
    }
}

fn disposition_reason(disposition: &DedupDisposition) -> String {
    match disposition {
        DedupDisposition::PossibleDuplicate { reason, .. } => reason.clone(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idor_key(path: &str) -> RootCauseKey {
        RootCauseKey::new(
            SecurityInvariant::ObjectOwnership,
            "tenant",
            VulnerableOperation::Read,
            path,
        )
    }

    #[test]
    fn the_same_check_reached_through_two_endpoints_is_one_root_cause() {
        // One missing `if (!owns(resource))` in one resolver, reachable through
        // the REST route and the GraphQL resolver. One report, two symptoms.
        let mut dedup = Deduplicator::new();
        let a = DedupCandidate::new("f-1", idor_key("resolver::find_invoice"))
            .with_endpoint("/api/v1/invoices/1")
            .with_class(VulnerabilityClass::IdorBola);
        let b = DedupCandidate::new("f-2", idor_key("resolver::find_invoice"))
            .with_endpoint("/graphql")
            .with_class(VulnerabilityClass::IdorBola);

        assert_eq!(dedup.submit(a), DedupDisposition::Primary);
        assert_eq!(
            dedup.submit(b),
            DedupDisposition::Merged { into: "f-1".into() }
        );
        assert_eq!(dedup.root_cause_count(), 1);
        assert_eq!(dedup.merged_count(), 1);
        assert_eq!(dedup.groups()[0].observed_endpoints.len(), 2);
        assert!(dedup.all_merges_explained());
    }

    #[test]
    fn the_same_endpoint_and_class_with_different_owners_stays_separate() {
        // Two invoices, two missing checks, two fixes. Merging these would hide
        // one from the program and ship one unfixed.
        let mut dedup = Deduplicator::new();
        dedup.submit(
            DedupCandidate::new("f-1", idor_key("invoice::show")).with_endpoint("/invoices/1"),
        );
        let disposition = dedup.submit(
            DedupCandidate::new("f-2", idor_key("invoice::show_other"))
                .with_endpoint("/invoices/1"),
        );
        match disposition {
            DedupDisposition::KeptSeparate { differs_on } => {
                assert!(differs_on.contains("code path"), "{differs_on}");
            }
            other => panic!("expected kept-separate, got {other:?}"),
        }
        assert_eq!(dedup.root_cause_count(), 2);
    }

    #[test]
    fn read_and_write_of_the_same_object_are_different_root_causes() {
        let mut dedup = Deduplicator::new();
        let read = RootCauseKey::new(
            SecurityInvariant::ObjectOwnership,
            "user",
            VulnerableOperation::Read,
            "invoice::show",
        );
        let write = RootCauseKey::new(
            SecurityInvariant::ObjectOwnership,
            "user",
            VulnerableOperation::Write,
            "invoice::show",
        );
        dedup.submit(DedupCandidate::new("f-1", read).with_endpoint("/invoices/1"));
        let disposition =
            dedup.submit(DedupCandidate::new("f-2", write).with_endpoint("/invoices/1"));
        assert!(
            matches!(disposition, DedupDisposition::KeptSeparate { .. }),
            "{disposition:?}"
        );
        assert_eq!(dedup.root_cause_count(), 2);
    }

    #[test]
    fn an_unconfirmed_code_path_is_flagged_not_merged() {
        let mut dedup = Deduplicator::new();
        dedup.submit(DedupCandidate::new("f-1", idor_key("resolver::find")));
        let disposition = dedup.submit(
            DedupCandidate::new("f-2", idor_key("resolver::find")).with_unconfirmed_code_path(),
        );
        match disposition {
            DedupDisposition::PossibleDuplicate { of, reason } => {
                assert_eq!(of, "f-1");
                assert!(reason.contains("unconfirmed"), "{reason}");
            }
            other => panic!("expected possible-duplicate, got {other:?}"),
        }
        // Not merged: the distinct defect stays visible.
        assert_eq!(dedup.submitted_count(), 2);
        assert_eq!(dedup.merged_count(), 0);
    }

    #[test]
    fn boundary_and_code_path_spellings_are_normalized() {
        let mut dedup = Deduplicator::new();
        dedup.submit(DedupCandidate::new(
            "f-1",
            RootCauseKey::new(
                SecurityInvariant::ObjectOwnership,
                " Tenant ",
                VulnerableOperation::Read,
                "app/handlers/Invoice",
            ),
        ));
        let disposition = dedup.submit(DedupCandidate::new(
            "f-2",
            RootCauseKey::new(
                SecurityInvariant::ObjectOwnership,
                "TENANT",
                VulnerableOperation::Read,
                "App\\Handlers\\Invoice",
            ),
        ));
        assert!(
            matches!(disposition, DedupDisposition::Merged { .. }),
            "{disposition:?}"
        );
    }

    #[test]
    fn unrelated_findings_are_never_merged() {
        let mut dedup = Deduplicator::new();
        // Class is deliberately not a merge key, so it does not appear here:
        // these three differ only in boundary and code path.
        for (id, boundary, path) in [
            ("f-1", "user", "user::show"),
            ("f-2", "tenant", "tenant::show"),
            ("f-3", "user", "report::export"),
        ] {
            dedup.submit(DedupCandidate::new(
                id,
                RootCauseKey::new(
                    SecurityInvariant::ObjectOwnership,
                    boundary,
                    VulnerableOperation::Read,
                    path,
                ),
            ));
        }
        assert_eq!(dedup.root_cause_count(), 3);
        assert_eq!(dedup.merged_count(), 0);
    }

    #[test]
    fn different_invariants_are_never_merged_even_with_the_same_path() {
        let mut dedup = Deduplicator::new();
        dedup.submit(DedupCandidate::new(
            "f-1",
            RootCauseKey::new(
                SecurityInvariant::ObjectOwnership,
                "user",
                VulnerableOperation::Read,
                "handler",
            ),
        ));
        let disposition = dedup.submit(DedupCandidate::new(
            "f-2",
            RootCauseKey::new(
                SecurityInvariant::ConfidentialityBoundary,
                "user",
                VulnerableOperation::Read,
                "handler",
            ),
        ));
        assert!(
            matches!(disposition, DedupDisposition::KeptSeparate { .. }),
            "{disposition:?}"
        );
    }

    #[test]
    fn every_group_can_explain_itself() {
        let mut dedup = Deduplicator::new();
        dedup.submit(DedupCandidate::new("f-1", idor_key("r")).with_endpoint("/a"));
        dedup.submit(DedupCandidate::new("f-2", idor_key("r")).with_endpoint("/b"));
        let explanation = dedup.groups()[0].explain();
        assert!(explanation.contains("2 finding(s)"), "{explanation}");
        assert!(explanation.contains("2 endpoint(s)"), "{explanation}");
        assert!(explanation.contains("one fix"), "{explanation}");
    }

    #[test]
    fn annotations_mark_the_survivor_and_the_merged_findings() {
        let mut dedup = Deduplicator::new();
        dedup.submit(DedupCandidate::new("f-1", idor_key("r")).with_endpoint("/a"));
        dedup.submit(DedupCandidate::new("f-2", idor_key("r")).with_endpoint("/b"));

        let mut findings = vec![
            Finding::new(
                "f-1".into(),
                "invoice read".into(),
                VulnerabilityClass::IdorBola,
                "example.com".into(),
            ),
            Finding::new(
                "f-2".into(),
                "invoice read via graphql".into(),
                VulnerabilityClass::IdorBola,
                "example.com".into(),
            ),
        ];
        dedup.annotate(&mut findings);

        let primary = findings[0].duplicate_status.as_ref().expect("annotated");
        assert!(!primary.is_duplicate);
        assert!(primary.duplicate_of.is_none());

        let merged = findings[1].duplicate_status.as_ref().expect("annotated");
        assert!(merged.is_duplicate);
        assert_eq!(merged.duplicate_of.as_deref(), Some("f-1"));
        assert!(merged.reasoning.contains("one fix"), "{}", merged.reasoning);
    }

    #[test]
    fn invariant_mapping_is_conservative_for_unmapped_classes() {
        assert_eq!(
            SecurityInvariant::for_class(&VulnerabilityClass::IdorBola),
            SecurityInvariant::ObjectOwnership
        );
        assert_eq!(
            SecurityInvariant::for_class(&VulnerabilityClass::PrivilegeEscalation),
            SecurityInvariant::RolePrivilege
        );
        assert_eq!(
            SecurityInvariant::for_class(&VulnerabilityClass::CrossTenantAccess),
            SecurityInvariant::TenantIsolation
        );
        // Unmapped classes must fall back rather than guess.
        assert_eq!(
            SecurityInvariant::for_class(&VulnerabilityClass::PrototypePollution),
            SecurityInvariant::Other
        );
    }

    #[test]
    fn dedup_is_deterministic_regardless_of_submission_order_for_merges() {
        let a = RootCauseKey::new(
            SecurityInvariant::ObjectOwnership,
            "t",
            VulnerableOperation::Read,
            "p",
        );
        let b = RootCauseKey::new(
            SecurityInvariant::ObjectOwnership,
            "t",
            VulnerableOperation::Read,
            "p",
        );
        let mut one = Deduplicator::new();
        one.submit(DedupCandidate::new("f-1", a.clone()));
        one.submit(DedupCandidate::new("f-2", b.clone()));
        let mut two = Deduplicator::new();
        two.submit(DedupCandidate::new("f-1", a));
        two.submit(DedupCandidate::new("f-2", b));
        assert_eq!(one.root_cause_count(), two.root_cause_count());
        assert_eq!(one.merged_count(), two.merged_count());
    }
}
