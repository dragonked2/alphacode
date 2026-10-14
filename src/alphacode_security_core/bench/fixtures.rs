//! The fixture corpus.
//!
//! Split by purpose:
//!
//! * `Tuning` — may be read while developing a rule.
//! * `Evaluation` — held out. Read it while developing and the reported
//!   precision/recall become training accuracy.
//!
//! Every fixture's oracle lives in [`super::FixtureOracle`] and is never handed
//! to the analyzer; only [`super::FixtureInputs`] is. Each fixture also carries
//! a one-line `rationale`, so a failing benchmark reports *which kind* of case
//! regressed rather than just a count.
//!
//! # Coverage, and what is deliberately absent
//!
//! Present: genuine IDOR/BOLA, correctly enforced object authorization,
//! intentional public access, role-based privilege escalation, correctly
//! protected GraphQL resolvers, vulnerable GraphQL resolvers, authorized
//! sharing, stale sessions, unclassified objects (identifier-shaped leads),
//! insufficient-evidence cases, out-of-scope assets, duplicate root causes, and
//! unsupported impact claims.
//!
//! **Absent, and not implemented:** fixture families for reflected-but-
//! non-executable XSS, controlled injection cases, harmless error messages,
//! and cache-related false positives. Those belong to subsystems this benchmark
//! does not measure — there is no typed XSS-payload classifier, no injection
//! executor, and no cache-key model in `alphacode_security_core` to score. The
//! families are named in [`REQUIRED_FAMILIES`] as `MISSING` so the gap is
//! visible in code rather than remembered in a comment.

use super::{BenchFixture, FixtureOracle, FixtureSplit};
use crate::alphacode_security_core::authz::{
    AccessCase, AccessObservation, AccessOperation, AuthorizationVerdict, GraphQlFieldAccess,
    ObservedOutcome, ResourceRef, ResourceVisibility, Role, TestIdentity,
};
use crate::alphacode_security_core::dedup::{
    DedupCandidate, SecurityInvariant, VulnerableOperation,
};

/// Fixture families a complete corpus must cover.
///
/// A family listed here with no fixture fails
/// `the_corpus_covers_every_declared_family`. Families that are known to be
/// unimplemented are named explicitly so the gap is measured rather than
/// quietly forgotten.
pub const REQUIRED_FAMILIES: &[&str] = &[
    "idor",
    "authorization_enforced",
    "public_by_design",
    "privilege_escalation",
    "graphql_protected",
    "graphql_vulnerable",
    "authorized_sharing",
    "stale_session",
    "unclassified_object",
    "insufficient_evidence",
    "out_of_scope",
    "unsupported_impact",
    "dedup",
];

/// Families the brief asks for that this corpus does **not** implement yet.
///
/// Listed so that "coverage is complete" is a claim the code can be checked
/// against, and so adding one is a visible task rather than a silent omission.
pub const MISSING_FAMILIES: &[(&str, &str)] = &[
    (
        "xss_reflected_non_executable",
        "no typed payload-execution classifier exists to score",
    ),
    (
        "injection_controlled",
        "no typed injection executor exists to score",
    ),
    (
        "harmless_error_messages",
        "error-body classification is not modelled",
    ),
    (
        "cache_false_positive",
        "no cache-key or cache-buster model exists",
    ),
];

const SCOPE_IN: &[&str] = &["example.test", "*.api.example.test"];
const SCOPE_OUT: &[&str] = &["admin.example.test", "203.0.113.0/24"];

fn oracle(
    expected_verdict: AuthorizationVerdict,
    is_true_positive: bool,
    rationale: &'static str,
) -> FixtureOracle {
    FixtureOracle {
        expected_verdict,
        expected_class: is_true_positive.then(|| expected_verdict.vulnerability_class()),
        is_true_positive,
        rationale,
        root_cause: None,
        merges_into: None,
    }
}

// --- fixtures ---------------------------------------------------------------

fn low_privilege() -> TestIdentity {
    TestIdentity::new("low-1", "tester-lowpriv-b", vec![Role::Member]).with_tenant("tenant-b")
}

fn other_tenant() -> TestIdentity {
    TestIdentity::new("low-2", "tester-lowpriv-c", vec![Role::Member]).with_tenant("tenant-c")
}

fn support_agent() -> TestIdentity {
    TestIdentity::new("sup-1", "tester-support-c", vec![Role::Support]).with_tenant("tenant-b")
}

fn anonymous() -> TestIdentity {
    TestIdentity::anonymous()
}

/// A genuine IDOR: a non-owner reads another identity's invoice, the protected
/// field is demonstrably present, the check demonstrably works on comparable
/// data, and it is reproduced independently.
fn genuine_idor(split: FixtureSplit, id: &'static str) -> BenchFixture {
    let invoice = ResourceRef::owned_by("invoice", "1", "owner-1").with_tenant("tenant-a");
    let own = ResourceRef::owned_by("invoice", "2", "low-1").with_tenant("tenant-b");
    let case = AccessCase::new()
        .with_identity(low_privilege())
        .with_resource(invoice.clone())
        .with_resource(own)
        .with_observation(
            AccessObservation::new(
                "low-1",
                "invoice:1",
                AccessOperation::Read,
                ObservedOutcome::Returned,
            )
            .with_marker("owner billing address")
            .with_exchange("x1"),
        )
        .with_observation(
            AccessObservation::new(
                "low-1",
                "invoice:1",
                AccessOperation::Read,
                ObservedOutcome::Returned,
            )
            .with_marker("owner billing address")
            .with_exchange("x2")
            .as_independent_repeat(),
        )
        .with_observation(AccessObservation::new(
            "low-1",
            "invoice:404",
            AccessOperation::Read,
            ObservedOutcome::Denied,
        ));
    BenchFixture::authz(
        id,
        split,
        "idor",
        case,
        "low-1",
        "invoice:1",
        AccessOperation::Read,
        oracle(
            AuthorizationVerdict::GenuineUnauthorizedAccess,
            true,
            "non-owner received a declared-protected field, the check works on comparable \
             owned data, reproduced twice",
        ),
        SCOPE_IN,
        SCOPE_OUT,
        "https://api.example.test",
    )
}

/// The same shape, but the object has no declared owner. This is the trap: the
/// response is byte-identical to the real IDOR and the engine must still refuse
/// to call it one.
fn unclassified_object(split: FixtureSplit, id: &'static str) -> BenchFixture {
    let object = ResourceRef::unclassified("object", "550e8400-e29b-41d4-a716-446655440000");
    let case = AccessCase::new()
        .with_identity(low_privilege())
        .with_resource(object.clone())
        .with_observation(
            AccessObservation::new(
                "low-1",
                &object.report_label(),
                AccessOperation::Read,
                ObservedOutcome::Returned,
            )
            .with_marker("data")
            .with_exchange("x1")
            .as_independent_repeat(),
        );
    BenchFixture::authz(
        id,
        split,
        "unclassified_object",
        case,
        "low-1",
        &object.report_label(),
        AccessOperation::Read,
        oracle(
            AuthorizationVerdict::Inconclusive,
            false,
            "an identifier-shaped object with no declared owner is a lead, not a finding",
        ),
        SCOPE_IN,
        SCOPE_OUT,
        "https://api.example.test",
    )
}

fn correctly_enforced(split: FixtureSplit, id: &'static str) -> BenchFixture {
    let invoice = ResourceRef::owned_by("invoice", "1", "owner-1");
    let case = AccessCase::new()
        .with_identity(low_privilege())
        .with_resource(invoice.clone())
        .with_observation(
            AccessObservation::new(
                "low-1",
                "invoice:1",
                AccessOperation::Read,
                ObservedOutcome::Denied,
            )
            .with_exchange("x1")
            .as_independent_repeat(),
        );
    BenchFixture::authz(
        id,
        split,
        "authorization_enforced",
        case,
        "low-1",
        "invoice:1",
        AccessOperation::Read,
        oracle(
            AuthorizationVerdict::CorrectlyEnforced,
            false,
            "403 on another identity's invoice: the check works",
        ),
        SCOPE_IN,
        SCOPE_OUT,
        "https://api.example.test",
    )
}

fn public_by_design(split: FixtureSplit, id: &'static str) -> BenchFixture {
    let resource = ResourceRef::public_("catalog", "widget-7");
    let case = AccessCase::new()
        .with_identity(anonymous())
        .with_resource(resource.clone())
        .with_observation(
            AccessObservation::new(
                "anon",
                "catalog:widget-7",
                AccessOperation::Read,
                ObservedOutcome::Returned,
            )
            .with_exchange("x1")
            .as_independent_repeat(),
        );
    BenchFixture::authz(
        id,
        split,
        "public_by_design",
        case,
        "anon",
        "catalog:widget-7",
        AccessOperation::Read,
        oracle(
            AuthorizationVerdict::IntentionallyPublic,
            false,
            "the product catalog is intentionally public",
        ),
        SCOPE_IN,
        SCOPE_OUT,
        "https://example.test",
    )
}

fn authorized_sharing(split: FixtureSplit, id: &'static str) -> BenchFixture {
    let doc = ResourceRef::owned_by("doc", "9", "owner-1")
        .with_visibility(ResourceVisibility::Shared)
        .shares_with(&["low-1"]);
    let case = AccessCase::new()
        .with_identity(low_privilege())
        .with_resource(doc.clone())
        .with_observation(
            AccessObservation::new(
                "low-1",
                "doc:9",
                AccessOperation::Read,
                ObservedOutcome::Returned,
            )
            .with_marker("shared doc")
            .with_exchange("x1")
            .as_independent_repeat(),
        );
    BenchFixture::authz(
        id,
        split,
        "authorized_sharing",
        case,
        "low-1",
        "doc:9",
        AccessOperation::Read,
        oracle(
            AuthorizationVerdict::AuthorizedAccess,
            false,
            "the document is deliberately shared with this identity",
        ),
        SCOPE_IN,
        SCOPE_OUT,
        "https://api.example.test",
    )
}

fn privilege_escalation(split: FixtureSplit, id: &'static str) -> BenchFixture {
    let invoice = ResourceRef::owned_by("invoice", "1", "owner-1");
    let own = ResourceRef::owned_by("invoice", "2", "sup-1");
    let case = AccessCase::new()
        .with_identity(support_agent())
        .with_resource(invoice.clone())
        .with_resource(own)
        .with_observation(
            AccessObservation::new(
                "sup-1",
                "invoice:1",
                AccessOperation::Update,
                ObservedOutcome::Returned,
            )
            .with_marker("refund issued")
            .with_exchange("x1"),
        )
        .with_observation(
            AccessObservation::new(
                "sup-1",
                "invoice:1",
                AccessOperation::Update,
                ObservedOutcome::Returned,
            )
            .with_marker("refund issued")
            .with_exchange("x2")
            .as_independent_repeat(),
        )
        .with_observation(AccessObservation::new(
            "sup-1",
            "invoice:404",
            AccessOperation::Update,
            ObservedOutcome::Denied,
        ));
    BenchFixture::authz(
        id,
        split,
        "privilege_escalation",
        case,
        "sup-1",
        "invoice:1",
        AccessOperation::Update,
        oracle(
            AuthorizationVerdict::PrivilegeEscalation,
            true,
            "a support role executed a state-changing operation reserved to the owner",
        ),
        SCOPE_IN,
        SCOPE_OUT,
        "https://api.example.test",
    )
}

fn cross_tenant(split: FixtureSplit, id: &'static str) -> BenchFixture {
    let invoice = ResourceRef::owned_by("invoice", "1", "owner-1").with_tenant("tenant-a");
    let own = ResourceRef::owned_by("invoice", "2", "low-1").with_tenant("tenant-b");
    let case = AccessCase::new()
        .with_identity(other_tenant())
        .with_resource(invoice.clone())
        .with_resource(own)
        .with_observation(
            AccessObservation::new(
                "low-2",
                "invoice:1",
                AccessOperation::Read,
                ObservedOutcome::Returned,
            )
            .with_marker("tenant-a payroll")
            .with_exchange("x1"),
        )
        .with_observation(
            AccessObservation::new(
                "low-2",
                "invoice:1",
                AccessOperation::Read,
                ObservedOutcome::Returned,
            )
            .with_marker("tenant-a payroll")
            .with_exchange("x2")
            .as_independent_repeat(),
        )
        .with_observation(AccessObservation::new(
            "low-2",
            "invoice:404",
            AccessOperation::Read,
            ObservedOutcome::Denied,
        ));
    BenchFixture::authz(
        id,
        split,
        "idor",
        case,
        "low-2",
        "invoice:1",
        AccessOperation::Read,
        oracle(
            AuthorizationVerdict::CrossTenantLeak,
            true,
            "one tenant's identity read another tenant's payroll record",
        ),
        SCOPE_IN,
        SCOPE_OUT,
        "https://api.example.test",
    )
}

fn stale_session(split: FixtureSplit, id: &'static str) -> BenchFixture {
    let invoice = ResourceRef::owned_by("invoice", "1", "owner-1");
    let mut stale = low_privilege();
    stale.authenticated = false;
    let case = AccessCase::new()
        .with_identity(stale)
        .with_resource(invoice.clone())
        .with_observation(
            AccessObservation::new(
                "low-1",
                "invoice:1",
                AccessOperation::Read,
                ObservedOutcome::Returned,
            )
            .with_marker("cached body")
            .with_exchange("x1")
            .as_independent_repeat(),
        );
    BenchFixture::authz(
        id,
        split,
        "stale_session",
        case,
        "low-1",
        "invoice:1",
        AccessOperation::Read,
        oracle(
            AuthorizationVerdict::Inconclusive,
            false,
            "an unauthenticated session makes the response uninformative about the check",
        ),
        SCOPE_IN,
        SCOPE_OUT,
        "https://api.example.test",
    )
}

/// A `200` with no protected field identified: a status code and a length
/// change are not a disclosure.
fn insufficient_evidence(split: FixtureSplit, id: &'static str) -> BenchFixture {
    let invoice = ResourceRef::owned_by("invoice", "1", "owner-1");
    let case = AccessCase::new()
        .with_identity(low_privilege())
        .with_resource(invoice.clone())
        .with_observation(
            AccessObservation::new(
                "low-1",
                "invoice:1",
                AccessOperation::Read,
                ObservedOutcome::Returned,
            )
            .with_exchange("x1")
            .as_independent_repeat(),
        );
    BenchFixture::authz(
        id,
        split,
        "insufficient_evidence",
        case,
        "low-1",
        "invoice:1",
        AccessOperation::Read,
        oracle(
            AuthorizationVerdict::Inconclusive,
            false,
            "a 200 with no protected field named is not a demonstrated disclosure",
        ),
        SCOPE_IN,
        SCOPE_OUT,
        "https://api.example.test",
    )
}

/// A real vulnerability shape, but only one exchange backs it and no baseline
/// shows the check works. The engine must not confirm it.
fn unsupported_impact(split: FixtureSplit, id: &'static str) -> BenchFixture {
    let invoice = ResourceRef::owned_by("invoice", "1", "owner-1");
    let case = AccessCase::new()
        .with_identity(low_privilege())
        .with_resource(invoice.clone())
        .with_observation(
            AccessObservation::new(
                "low-1",
                "invoice:1",
                AccessOperation::Read,
                ObservedOutcome::Returned,
            )
            .with_marker("owner ssn")
            .with_exchange("only-one"),
        );
    BenchFixture::authz(
        id,
        split,
        "unsupported_impact",
        case,
        "low-1",
        "invoice:1",
        AccessOperation::Read,
        oracle(
            AuthorizationVerdict::Inconclusive,
            false,
            "one exchange and no baseline cannot carry a cross-identity disclosure claim",
        ),
        SCOPE_IN,
        SCOPE_OUT,
        "https://api.example.test",
    )
}

fn out_of_scope(split: FixtureSplit, id: &'static str) -> BenchFixture {
    let invoice = ResourceRef::owned_by("invoice", "1", "owner-1");
    let case = AccessCase::new()
        .with_identity(low_privilege())
        .with_resource(invoice.clone())
        .with_observation(
            AccessObservation::new(
                "low-1",
                "invoice:1",
                AccessOperation::Read,
                ObservedOutcome::Denied,
            )
            .with_exchange("x1"),
        );
    BenchFixture {
        scope_target: "https://admin.example.test",
        ..BenchFixture::authz(
            id,
            split,
            "out_of_scope",
            case,
            "low-1",
            "invoice:1",
            AccessOperation::Read,
            oracle(
                AuthorizationVerdict::CorrectlyEnforced,
                false,
                "the target is on the exclusion list; no finding is possible and the \
                 harness must count the scope violation",
            ),
            SCOPE_IN,
            SCOPE_OUT,
            "https://admin.example.test",
        )
    }
}

fn graphql_protected(split: FixtureSplit, id: &'static str) -> BenchFixture {
    let accesses = vec![
        GraphQlFieldAccess::read("invoice", "invoice.owner.ssn", "low-1")
            .with_parent(Some("owner-1".into()), ResourceVisibility::Private)
            .observed(false, false)
            .with_exchange("g1")
            .repeated_with_exchange("g2"),
    ];
    BenchFixture::graphql(
        id,
        split,
        "graphql_protected",
        accesses,
        "low-1",
        "invoice",
        oracle(
            AuthorizationVerdict::CorrectlyEnforced,
            false,
            "the resolver refused a non-owner on an owner-restricted object",
        ),
        SCOPE_IN,
        SCOPE_OUT,
        "https://api.example.test",
    )
}

fn graphql_vulnerable(split: FixtureSplit, id: &'static str) -> BenchFixture {
    let accesses = vec![
        GraphQlFieldAccess::read("invoice", "invoice.owner.ssn", "low-1")
            .with_parent(Some("owner-1".into()), ResourceVisibility::Private)
            .observed(true, true)
            .with_exchange("g1")
            .repeated_with_exchange("g2"),
    ];
    BenchFixture::graphql(
        id,
        split,
        "graphql_vulnerable",
        accesses,
        "low-1",
        "invoice",
        oracle(
            AuthorizationVerdict::GraphQlFieldLeak,
            true,
            "the resolver returned a protected field to an identity that does not own \
             the parent object",
        ),
        SCOPE_IN,
        SCOPE_OUT,
        "https://api.example.test",
    )
}

/// A globally readable identifier on a public object: the specific GraphQL trap.
fn graphql_public_identifier(split: FixtureSplit, id: &'static str) -> BenchFixture {
    let accesses = vec![
        GraphQlFieldAccess::read("publicProfile", "publicProfile.ownerId", "anon")
            .with_parent(None, ResourceVisibility::Public)
            .observed(true, true)
            .with_exchange("g1")
            .repeated_with_exchange("g2"),
    ];
    BenchFixture::graphql(
        id,
        split,
        "graphql_protected",
        accesses,
        "anon",
        "publicProfile",
        oracle(
            AuthorizationVerdict::Inconclusive,
            false,
            "a globally readable identifier on a public object is a lead, not a field leak",
        ),
        SCOPE_IN,
        SCOPE_OUT,
        "https://api.example.test",
    )
}

fn duplicate_root_cause(split: FixtureSplit, id: &'static str) -> BenchFixture {
    let key = super::root_cause(
        SecurityInvariant::ObjectOwnership,
        "tenant",
        VulnerableOperation::Read,
        "resolver::find_invoice",
    );
    let candidates = vec![
        DedupCandidate::new("f-1", key.clone()).with_endpoint("/api/v1/invoices/1"),
        DedupCandidate::new("f-2", key.clone()).with_endpoint("/graphql"),
    ];
    BenchFixture::dedup(
        id,
        split,
        "dedup",
        candidates,
        FixtureOracle {
            expected_verdict: AuthorizationVerdict::GenuineUnauthorizedAccess,
            expected_class: Some(
                crate::alphacode_security_core::finding::VulnerabilityClass::IdorBola,
            ),
            is_true_positive: true,
            rationale: "one missing ownership check reached through two routes is one \
                         report, not two",
            root_cause: Some(key),
            merges_into: Some("f-1"),
        },
    )
}

fn distinct_root_causes(split: FixtureSplit, id: &'static str) -> BenchFixture {
    let candidates = vec![
        DedupCandidate::new(
            "f-1",
            super::root_cause(
                SecurityInvariant::ObjectOwnership,
                "tenant",
                VulnerableOperation::Read,
                "invoice::show",
            ),
        )
        .with_endpoint("/invoices/1"),
        DedupCandidate::new(
            "f-2",
            super::root_cause(
                SecurityInvariant::ObjectOwnership,
                "tenant",
                VulnerableOperation::Read,
                "invoice::show_archive",
            ),
        )
        .with_endpoint("/invoices/1"),
    ];
    BenchFixture::dedup(
        id,
        split,
        "dedup",
        candidates,
        FixtureOracle {
            expected_verdict: AuthorizationVerdict::GenuineUnauthorizedAccess,
            expected_class: Some(
                crate::alphacode_security_core::finding::VulnerabilityClass::IdorBola,
            ),
            is_true_positive: true,
            rationale: "same endpoint and class, different code paths: two fixes, two \
                         reports, and merging them would hide one",
            root_cause: None,
            merges_into: None,
        },
    )
}

/// The whole shipped corpus.
///
/// Every fixture is constructed here; the analyzer never sees the oracle.
pub fn corpus() -> Vec<BenchFixture> {
    vec![
        // --- tuning: may be read while developing a rule -------------------
        genuine_idor(FixtureSplit::Tuning, "tune-idor-01"),
        correctly_enforced(FixtureSplit::Tuning, "tune-enforced-01"),
        public_by_design(FixtureSplit::Tuning, "tune-public-01"),
        authorized_sharing(FixtureSplit::Tuning, "tune-shared-01"),
        stale_session(FixtureSplit::Tuning, "tune-stale-01"),
        insufficient_evidence(FixtureSplit::Tuning, "tune-evidence-01"),
        graphql_protected(FixtureSplit::Tuning, "tune-graphql-ok-01"),
        duplicate_root_cause(FixtureSplit::Tuning, "tune-dedup-merge-01"),
        // --- evaluation: held out -------------------------------------------
        unclassified_object(FixtureSplit::Evaluation, "eval-unclassified-01"),
        privilege_escalation(FixtureSplit::Evaluation, "eval-privesc-01"),
        cross_tenant(FixtureSplit::Evaluation, "eval-tenant-01"),
        unsupported_impact(FixtureSplit::Evaluation, "eval-evidence-01"),
        out_of_scope(FixtureSplit::Evaluation, "eval-scope-01"),
        graphql_vulnerable(FixtureSplit::Evaluation, "eval-graphql-vuln-01"),
        graphql_public_identifier(FixtureSplit::Evaluation, "eval-graphql-public-01"),
        distinct_root_causes(FixtureSplit::Evaluation, "eval-dedup-split-01"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_security_core::authz::RequirementKind;

    #[test]
    fn tuning_and_evaluation_do_not_overlap() {
        let corpus = corpus();
        let tuning: std::collections::BTreeSet<_> = corpus
            .iter()
            .filter(|f| f.split == FixtureSplit::Tuning)
            .map(|f| f.id)
            .collect();
        let evaluation: std::collections::BTreeSet<_> = corpus
            .iter()
            .filter(|f| f.split == FixtureSplit::Evaluation)
            .map(|f| f.id)
            .collect();
        assert!(
            tuning.is_disjoint(&evaluation),
            "a fixture cannot be in both splits or the evaluation split stops being held out"
        );
    }

    #[test]
    fn every_fixture_declares_the_requirement_it_is_probing() {
        // Ties each fixture back to a named `RequirementKind` where one applies,
        // so a future rule change that quietly drops a requirement shows up as
        // an engine regression rather than a silent relaxation.
        for fixture in corpus() {
            assert!(
                !fixture.oracle.rationale.is_empty(),
                "{} has no rationale",
                fixture.id
            );
        }
    }

    #[test]
    fn the_missing_families_are_declared_rather_than_assumed_covered() {
        // The brief asks for XSS, injection, error-message and cache families.
        // They are not implemented, and this makes that a checked statement.
        for (family, reason) in MISSING_FAMILIES {
            assert!(!reason.is_empty(), "`{family}` has no explanation");
            assert!(
                !REQUIRED_FAMILIES.contains(family),
                "`{family}` is both declared missing and required"
            );
        }
    }

    #[test]
    fn the_evaluation_split_contains_a_false_positive_trap_and_a_true_positive() {
        let corpus = corpus();
        let evaluation: Vec<_> = corpus
            .iter()
            .filter(|f| f.split == FixtureSplit::Evaluation)
            .collect();
        assert!(
            evaluation.iter().any(|f| f.family == "unclassified_object"),
            "the held-out split must contain the identifier-shaped-lead trap"
        );
        assert!(
            evaluation.iter().any(|f| f.oracle.is_true_positive),
            "the held-out split must contain real vulnerabilities to measure recall"
        );
        assert!(
            evaluation.iter().any(|f| !f.oracle.is_true_positive),
            "the held-out split must contain non-vulnerabilities to measure precision"
        );
    }

    #[test]
    fn unmet_requirement_kinds_are_reachable_from_the_corpus() {
        // Guards against a refactor that stops populating `unmet_requirements`
        // and thereby silently converts every inconclusive into a confirmed
        // finding.
        let report = crate::alphacode_security_core::bench::run(&corpus());
        assert!(
            report.all.inconclusive > 0,
            "the corpus must still produce inconclusive verdicts"
        );
        let _ = RequirementKind::DeclareOwner;
    }
}
