//! Deterministic authorization testing: identity/role matrix, object
//! relationships, differential access comparison, and GraphQL field-level
//! access control.
//!
//! # Why this exists
//!
//! The authorization problem was previously only *describable*. `Finding` has
//! an `auth_context` field, `proof` has a `BaselineComparison`, the skill
//! markdown has a "3x3 actor/resource matrix" and a "tenant isolation
//! differential" — and nothing computed any of it. `Finding::auth_context` was
//! initialised to `"unknown"` and never assigned anywhere in the crate, so the
//! IDOR disproof branch in the validation engine could not fire (the branch's
//! own comment said so). There was no type for a test identity, so "which
//! account owns this resource" could only be guessed from the shape of an
//! identifier, which is exactly the reasoning that produces false positives.
//!
//! This module is the deterministic half: given a declared set of controlled
//! identities, declared resources, declared expectations, and the exchanges
//! actually observed, it decides what was proven. It performs no I/O, calls no
//! model, and is a pure function of its inputs, so every verdict it produces is
//! reproducible and reviewable.
//!
//! # What it deliberately refuses to do
//!
//! * **It does not infer ownership from an identifier.** Ownership comes from
//!   [`ResourceRef::owner`], which a caller sets from a controlled fixture or a
//!   documented program rule. An object whose owner is unknown is
//!   [`ResourceVisibility::Undetermined`], and an access to it is
//!   [`AuthorizationVerdict::Inconclusive`], never a finding.
//! * **It does not treat object discovery as proof.** Finding an identifier is
//!   a *lead*; [`AuthorizationVerdict::GenuineUnauthorizedAccess`] additionally
//!   requires protected data in the response and a baseline that shows the same
//!   identity is denied comparable data it does not own.
//! * **It does not enumerate real users' data.** Impact is demonstrated against
//!   the controlled identities in the matrix, and the engine reports the
//!   identity used so a report can state the minimum evidence used.
//! * **It does not let multiple agreeing model opinions count as verification.**
//!   Independence is derived from distinct observed exchanges, not from
//!   opinions.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use crate::alphacode_security_core::finding::{Confidence, Finding, VulnerabilityClass};

/// A role an identity may hold.
///
/// Roles are data, not inference: a caller declares them from a controlled
/// fixture or a documented program rule. There is no ordering, because
/// privilege escalation is a claim about a *specific* operation, not about a
/// ranking the engine is entitled to invent.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// No credential at all.
    Anonymous,
    /// A normal authenticated user of the application.
    Member,
    /// Elevated operational access, but not resource ownership.
    Support,
    /// Administrative access.
    Admin,
    /// The identity that owns the resource.
    Owner,
    /// A program- or application-specific role.
    Custom(String),
}

impl Role {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Anonymous => "anonymous",
            Self::Member => "member",
            Self::Support => "support",
            Self::Admin => "admin",
            Self::Owner => "owner",
            Self::Custom(name) => name,
        }
    }
}

/// A controlled test identity.
///
/// Only identities created for testing belong here. `label` is what a report
/// may print: it must not be a real account.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TestIdentity {
    pub id: String,
    /// Human-readable, report-safe label, e.g. `tester-lowpriv-a`.
    pub label: String,
    pub roles: Vec<Role>,
    /// Tenant the identity belongs to, for cross-tenant isolation testing.
    pub tenant: Option<String>,
    /// Whether the identity carries a usable session right now. A stale session
    /// produces responses that look like authorization failures and is a
    /// recorded false-positive cause rather than a silent one.
    pub authenticated: bool,
}

impl TestIdentity {
    pub fn new(id: impl Into<String>, label: impl Into<String>, roles: Vec<Role>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            roles,
            tenant: None,
            authenticated: true,
        }
    }

    pub fn anonymous() -> Self {
        Self {
            id: "anon".into(),
            label: "unauthenticated".into(),
            roles: vec![Role::Anonymous],
            tenant: None,
            authenticated: false,
        }
    }

    pub fn with_tenant(mut self, tenant: impl Into<String>) -> Self {
        self.tenant = Some(tenant.into());
        self
    }

    pub fn has_role(&self, role: &Role) -> bool {
        self.roles.contains(role)
    }

    pub fn is_anonymous(&self) -> bool {
        self.roles.contains(&Role::Anonymous) || !self.authenticated
    }

    /// Stable label for reports and logs.
    pub fn report_label(&self) -> &str {
        &self.label
    }
}

/// How a resource is meant to be reachable.
///
/// `Undetermined` is the default and is load-bearing: a resource nobody has
/// classified cannot produce a finding, because the engine would otherwise be
/// asserting the security policy that the classification exists to state.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ResourceVisibility {
    /// Intentionally readable by anyone, including unauthenticated callers.
    Public,
    /// Deliberately shared with a declared set of identities.
    Shared,
    /// Owner-only. The default expectation.
    #[default]
    Private,
    /// Nobody has classified this resource. Access is a lead, never a finding.
    Undetermined,
}

/// A resource under test, with ownership *declared* rather than inferred.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ResourceRef {
    /// Object type, e.g. `invoice` or `User`.
    pub kind: String,
    /// Object identifier as it appears in the request.
    pub id: String,
    /// Identity that owns this object, when known.
    pub owner: Option<String>,
    pub tenant: Option<String>,
    pub visibility: ResourceVisibility,
    /// Identities the object is deliberately shared with.
    pub shared_with: Vec<String>,
    /// Whether the object's content is sensitive enough that returning it to a
    /// non-owner is an impact, not an inconvenience.
    pub sensitive: bool,
}

impl ResourceRef {
    /// A resource whose ownership is known.
    pub fn owned_by(
        kind: impl Into<String>,
        id: impl Into<String>,
        owner: impl Into<String>,
    ) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
            owner: Some(owner.into()),
            tenant: None,
            visibility: ResourceVisibility::Private,
            shared_with: Vec::new(),
            sensitive: true,
        }
    }

    /// A resource nobody has classified. Any access to it is a lead.
    pub fn unclassified(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
            owner: None,
            tenant: None,
            visibility: ResourceVisibility::Undetermined,
            shared_with: Vec::new(),
            sensitive: true,
        }
    }

    pub fn public_(kind: impl Into<String>, id: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            id: id.into(),
            owner: None,
            tenant: None,
            visibility: ResourceVisibility::Public,
            shared_with: Vec::new(),
            sensitive: false,
        }
    }

    pub fn with_visibility(mut self, visibility: ResourceVisibility) -> Self {
        self.visibility = visibility;
        self
    }

    pub fn with_tenant(mut self, tenant: impl Into<String>) -> Self {
        self.tenant = Some(tenant.into());
        self
    }

    pub fn sensitive(mut self, sensitive: bool) -> Self {
        self.sensitive = sensitive;
        self
    }

    pub fn shares_with(mut self, identities: &[&str]) -> Self {
        self.shared_with = identities.iter().map(|s| (*s).to_string()).collect();
        self
    }

    /// Whether `identity` is *declared* to be allowed to reach this resource.
    ///
    /// Ownership is the only trusted source. An identifier that merely looks
    /// like another user's (a UUID, an email fragment, a sequential id) is
    /// never consulted.
    pub fn declares_access_for(&self, identity: &TestIdentity) -> bool {
        match self.visibility {
            ResourceVisibility::Public => true,
            ResourceVisibility::Shared => self.shared_with.contains(&identity.id),
            ResourceVisibility::Private => self.owner.as_deref() == Some(identity.id.as_str()),
            ResourceVisibility::Undetermined => false,
        }
    }

    pub fn report_label(&self) -> String {
        format!("{}:{}", self.kind, self.id)
    }
}

/// The operation performed on a resource.
///
/// Authorization is operation-specific: read access says nothing about write
/// access, and many applications authorize writes correctly while leaking
/// reads.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum AccessOperation {
    #[default]
    Read,
    Create,
    Update,
    Delete,
    /// A GraphQL mutation, which may touch several objects at once.
    Mutation,
    /// An administrative action.
    AdminAction,
}

impl AccessOperation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Create => "create",
            Self::Update => "update",
            Self::Delete => "delete",
            Self::Mutation => "mutation",
            Self::AdminAction => "admin_action",
        }
    }

    /// Whether a successful execution of this operation changes state. Impact
    /// from a read is disclosure; impact from the others is a protected
    /// operation having been executed.
    pub fn changes_state(&self) -> bool {
        !matches!(self, Self::Read)
    }
}

/// Whether the resource body is really protected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservedOutcome {
    /// The server refused.
    Denied,
    /// The server accepted and returned the object.
    Returned,
    /// The server accepted but returned no object (e.g. an empty list, or a
    /// placeholder with no protected fields).
    Empty,
    /// The server errored. Informative only when the body discloses internals.
    Error,
    /// Not observed.
    NotObserved,
}

impl ObservedOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Denied => "denied",
            Self::Returned => "returned",
            Self::Empty => "empty",
            Self::Error => "error",
            Self::NotObserved => "not_observed",
        }
    }

    /// Whether protected content was actually handed over.
    pub fn returned_protected_data(&self) -> bool {
        matches!(self, Self::Returned)
    }
}

/// One observed attempt by one identity to reach one resource.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessObservation {
    pub identity: String,
    pub resource: String,
    pub operation: AccessOperation,
    pub outcome: ObservedOutcome,
    /// A value observed in the response that is only present when protected
    /// content was returned — a field the caller could not have guessed, or a
    /// value matching the owner's data rather than the caller's.
    pub protected_data_marker: Option<String>,
    /// Stable identifier of the underlying request/response, so the verdict can
    /// point a reviewer at the exchange that produced it.
    pub exchange_id: String,
    /// Whether this observation was repeated as an independent check (a second
    /// request after a cache-buster, or from a different session).
    pub independent_repeat: bool,
}

impl AccessObservation {
    pub fn new(
        identity: impl Into<String>,
        resource: impl Into<String>,
        operation: AccessOperation,
        outcome: ObservedOutcome,
    ) -> Self {
        Self {
            identity: identity.into(),
            resource: resource.into(),
            operation,
            outcome,
            protected_data_marker: None,
            exchange_id: String::new(),
            independent_repeat: false,
        }
    }

    pub fn with_marker(mut self, marker: impl Into<String>) -> Self {
        self.protected_data_marker = Some(marker.into());
        self
    }

    pub fn with_exchange(mut self, exchange_id: impl Into<String>) -> Self {
        self.exchange_id = exchange_id.into();
        self
    }

    /// The same access observed again against a second exchange.
    ///
    /// Two distinct exchanges are the floor for reporting anything: a single
    /// response may have come from a cache entry produced for someone else.
    pub fn repeated_with_exchange(&self, exchange_id: impl Into<String>) -> Self {
        let mut clone = self.clone();
        clone.exchange_id = exchange_id.into();
        clone
    }

    pub fn as_independent_repeat(mut self) -> Self {
        self.independent_repeat = true;
        self
    }
}

/// A reason a candidate authorization finding is probably not real.
///
/// Each of these is a way a real response can look like a vulnerability without
/// being one. They are recorded rather than silently filtered, so a reviewer can
/// see what was considered and rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FalsePositiveCause {
    /// The resource is intentionally public.
    PublicByDesign,
    /// The resource is deliberately shared with the testing identity.
    AuthorizedSharing,
    /// The identity's session was no longer valid, so the response says nothing
    /// about the authorization check.
    StaleSession,
    /// The response came from a cache entry produced for a different identity.
    CacheArtifact,
    /// The fixtures disagree with each other — the "other" object does not
    /// exist, was deleted, or belongs to a different environment.
    FixtureInconsistency,
    /// The conclusion rests on an assumption that was never established.
    UnsupportedAssumption,
    /// No baseline established that the check works for comparable data.
    MissingBaseline,
    /// The same identity owns the resource, so access is not cross-identity.
    SameIdentity,
    /// The response body differs only in volatile fields (timestamps, tokens,
    /// request ids) rather than in data.
    VolatileOnlyDifference,
}

impl FalsePositiveCause {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PublicByDesign => "public_by_design",
            Self::AuthorizedSharing => "authorized_sharing",
            Self::StaleSession => "stale_session",
            Self::CacheArtifact => "cache_artifact",
            Self::FixtureInconsistency => "fixture_inconsistency",
            Self::UnsupportedAssumption => "unsupported_assumption",
            Self::MissingBaseline => "missing_baseline",
            Self::SameIdentity => "same_identity",
            Self::VolatileOnlyDifference => "volatile_only_difference",
        }
    }

    /// Human-readable remediation for the *investigator*, not the target.
    pub fn guidance(&self) -> &'static str {
        match self {
            Self::PublicByDesign => {
                "Confirm the resource is intentionally public before reporting."
            }
            Self::AuthorizedSharing => {
                "Remove the sharing grant, or use a resource the identity is not shared with."
            }
            Self::StaleSession => "Re-authenticate the identity and repeat the comparison.",
            Self::CacheArtifact => "Repeat with a cache-busting parameter and a fresh session.",
            Self::FixtureInconsistency => {
                "Re-create the fixture pair so both objects exist identically."
            }
            Self::UnsupportedAssumption => {
                "Establish the assumption with a test before relying on it."
            }
            Self::MissingBaseline => "Capture the denied response for a comparable owned resource.",
            Self::SameIdentity => {
                "Use a second controlled identity that does not own the resource."
            }
            Self::VolatileOnlyDifference => "Compare stable fields rather than the whole body.",
        }
    }
}

/// A requirement a candidate must satisfy before it can be confirmed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthzRequirement {
    pub kind: RequirementKind,
    pub detail: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementKind {
    DeclareOwner,
    ShowProtectedData,
    EstablishBaseline,
    RepeatIndependently,
    FreshSession,
    DiscardCache,
    ConfirmNotShared,
    DemonstrateStateChange,
}

impl RequirementKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DeclareOwner => "declare_owner",
            Self::ShowProtectedData => "show_protected_data",
            Self::EstablishBaseline => "establish_baseline",
            Self::RepeatIndependently => "repeat_independently",
            Self::FreshSession => "fresh_session",
            Self::DiscardCache => "discard_cache",
            Self::ConfirmNotShared => "confirm_not_shared",
            Self::DemonstrateStateChange => "demonstrate_state_change",
        }
    }

    /// What the investigator should do next.
    pub fn next_test(&self) -> &'static str {
        match self {
            Self::DeclareOwner => {
                "Identify who owns this object from a controlled fixture; do not infer it \
                 from the identifier."
            }
            Self::ShowProtectedData => {
                "Show the protected field itself in the response, not a status code or a \
                 length change."
            }
            Self::EstablishBaseline => {
                "Capture the same request as the owning identity and as an identity that \
                 owns a comparable object, to show the check works on both."
            }
            Self::RepeatIndependently => {
                "Repeat the request from a second session so the result does not depend on \
                 one cached response."
            }
            Self::FreshSession => "Re-authenticate the identity and repeat the comparison.",
            Self::DiscardCache => {
                "Repeat with a unique cache-busting parameter and confirm the same data is \
                 returned."
            }
            Self::ConfirmNotShared => {
                "Check whether the object is deliberately shared with this identity before \
                 treating access as unauthorized."
            }
            Self::DemonstrateStateChange => {
                "Show that the protected operation actually executed, using a synthetic \
                 object you own."
            }
        }
    }
}

/// What the evidence actually established.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorizationVerdict {
    /// The identity was declared to have access and received it.
    AuthorizedAccess,
    /// Access control worked: a non-owner was refused comparable data.
    CorrectlyEnforced,
    /// The object is intentionally public, so access proves nothing.
    IntentionallyPublic,
    /// A non-owner received protected data the owner holds, with a baseline
    /// showing the same check works elsewhere.
    GenuineUnauthorizedAccess,
    /// A lower-privileged identity performed an operation reserved for a higher
    /// role, and the operation took effect.
    PrivilegeEscalation,
    /// An identity in one tenant received protected data belonging to another.
    CrossTenantLeak,
    /// A GraphQL field on an owner-restricted object was readable by a
    /// non-owner identity.
    GraphQlFieldLeak,
    /// The evidence does not support any conclusion yet.
    Inconclusive,
}

impl AuthorizationVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AuthorizedAccess => "authorized_access",
            Self::CorrectlyEnforced => "correctly_enforced",
            Self::IntentionallyPublic => "intentionally_public",
            Self::GenuineUnauthorizedAccess => "genuine_unauthorized_access",
            Self::PrivilegeEscalation => "privilege_escalation",
            Self::CrossTenantLeak => "cross_tenant_leak",
            Self::GraphQlFieldLeak => "graphql_field_leak",
            Self::Inconclusive => "inconclusive",
        }
    }

    /// Whether this verdict may become a reportable finding on its own.
    ///
    /// `Inconclusive` is the point of the type: a strong-looking response with
    /// no declared ownership and no baseline stays here forever rather than
    /// drifting upward through repeated look-alike evidence.
    pub fn is_vulnerability(&self) -> bool {
        matches!(
            self,
            Self::GenuineUnauthorizedAccess
                | Self::PrivilegeEscalation
                | Self::CrossTenantLeak
                | Self::GraphQlFieldLeak
        )
    }

    /// The vulnerability class this verdict corresponds to, for `Finding`.
    pub fn vulnerability_class(&self) -> VulnerabilityClass {
        match self {
            Self::GenuineUnauthorizedAccess => VulnerabilityClass::IdorBola,
            Self::PrivilegeEscalation => VulnerabilityClass::PrivilegeEscalation,
            Self::CrossTenantLeak => VulnerabilityClass::CrossTenantAccess,
            Self::GraphQlFieldLeak => VulnerabilityClass::GraphQlInjection,
            Self::AuthorizedAccess
            | Self::CorrectlyEnforced
            | Self::IntentionallyPublic
            | Self::Inconclusive => VulnerabilityClass::MissingAuthorization,
        }
    }
}

/// The full, explainable result of analyzing one access pattern.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationAssessment {
    pub verdict: AuthorizationVerdict,
    pub identity: String,
    pub resource: String,
    pub operation: AccessOperation,
    /// How many distinct exchanges backed the verdict. Two independent
    /// exchanges is the floor for reporting anything; opinions are not
    /// counted.
    pub supporting_exchanges: usize,
    pub false_positive_causes: Vec<FalsePositiveCause>,
    /// What is still missing before this can be confirmed.
    pub unmet_requirements: Vec<AuthzRequirement>,
    pub explanation: String,
}

impl AuthorizationAssessment {
    pub fn confidence(&self) -> Confidence {
        if !self.verdict.is_vulnerability() {
            return if self.verdict == AuthorizationVerdict::Inconclusive {
                Confidence::Low
            } else {
                Confidence::High
            };
        }
        // A vulnerability claim needs declared ownership, a baseline, and at
        // least two independent exchanges. One missing element drops it a whole
        // notch rather than nudging it down.
        match (self.supporting_exchanges, self.unmet_requirements.len()) {
            (0..=1, _) => Confidence::Low,
            (2, 0) => Confidence::Certain,
            (2, 1) => Confidence::High,
            (_, 0) => Confidence::High,
            _ => Confidence::Medium,
        }
    }

    /// Whether the assessment is confirmed and safe to report.
    pub fn is_reportable(&self) -> bool {
        self.verdict.is_vulnerability()
            && self.unmet_requirements.is_empty()
            && self.supporting_exchanges >= 2
    }

    /// The `Finding::auth_context` string for this assessment.
    ///
    /// This is the value the validation engine reads when it tries to disprove
    /// an IDOR finding asserted against an unauthenticated caller. Emitting it
    /// here is what makes that branch reachable at all.
    pub fn finding_auth_context(&self) -> String {
        if self.identity == "anon" {
            "unauthenticated".to_string()
        } else {
            format!("authenticated:{}", self.identity)
        }
    }
}

// ---------------------------------------------------------------------------
// The identity/role matrix
// ---------------------------------------------------------------------------

/// A declared authorization policy for one operation on one resource type.
///
/// This is the "what should happen" side of a differential test. It is
/// authored by the investigator from a controlled fixture or a documented
/// program rule; it is never inferred from observed behaviour, which would make
/// the test circular.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorizationMatrix {
    /// Identity id -> resource id -> expected outcome for reads.
    read_expectations: BTreeSet<ExpectationKey>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct ExpectationKey {
    identity: String,
    resource: String,
    allow: bool,
}

impl AuthorizationMatrix {
    pub fn new() -> Self {
        Self::default()
    }

    /// Declare that `identity` is expected to reach `resource`.
    pub fn expect_allowed(&mut self, identity: &str, resource: &str) -> &mut Self {
        self.read_expectations.insert(ExpectationKey {
            identity: identity.to_string(),
            resource: resource.to_string(),
            allow: true,
        });
        self
    }

    /// Declare that `identity` is expected to be refused `resource`.
    pub fn expect_denied(&mut self, identity: &str, resource: &str) -> &mut Self {
        self.read_expectations.insert(ExpectationKey {
            identity: identity.to_string(),
            resource: resource.to_string(),
            allow: false,
        });
        self
    }

    /// The declared expectation, if one exists.
    pub fn expectation(&self, identity: &str, resource: &str) -> Option<bool> {
        self.read_expectations
            .iter()
            .find(|e| e.identity == identity && e.resource == resource)
            .map(|e| e.allow)
    }
}

/// Object-relationship analysis: identifiers seen together, and where.
///
/// Correlating identifiers across endpoints, responses and fixtures is how a
/// lead is generated. It is kept separate from [`analyze_access`] on purpose:
/// a relationship is evidence that an object *exists*, never evidence that it
/// was reachable without authorization.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectGraph {
    /// Resource label -> identifiers observed in the same response.
    adjacency: BTreeSet<Relationship>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
struct Relationship {
    from: String,
    to: String,
}

impl ObjectGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `from` was seen alongside `to` (e.g. `invoice:1` alongside
    /// `User:42` in one response).
    pub fn observe_relationship(&mut self, from: impl Into<String>, to: impl Into<String>) {
        let (from, to) = (from.into(), to.into());
        if from != to {
            self.adjacency.insert(Relationship { from, to });
        }
    }

    /// Identifiers observed alongside `resource`.
    pub fn neighbors(&self, resource: &str) -> Vec<String> {
        let mut out: Vec<String> = self
            .adjacency
            .iter()
            .filter(|r| r.from == resource || r.to == resource)
            .flat_map(|r| {
                if r.from == resource {
                    vec![r.to.clone()]
                } else {
                    vec![r.from.clone()]
                }
            })
            .collect();
        out.sort();
        out.dedup();
        out
    }

    /// Everything discovered through relationships for `resource`. A leaf is
    /// not a lead; a neighbour is.
    pub fn leads_for(&self, resource: &str) -> Vec<String> {
        self.neighbors(resource)
    }

    pub fn is_empty(&self) -> bool {
        self.adjacency.is_empty()
    }
}

// ---------------------------------------------------------------------------
// GraphQL field-level access control
// ---------------------------------------------------------------------------

/// One GraphQL field, as reached through a specific path.
///
/// A globally readable identifier in a GraphQL response is a lead, not a
/// vulnerability: the interesting question is whether the *field* is
/// resolvable by an identity that has no claim on the *parent object*.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphQlFieldAccess {
    /// Query or mutation name, e.g. `invoice`.
    pub root_field: String,
    /// Dotted path to the field under test, e.g. `invoice.owner.ssn`.
    pub field_path: String,
    /// The identity that requested it.
    pub identity: String,
    /// Whether the resolver returned a value.
    pub resolved: bool,
    /// Whether the returned value was non-empty protected data rather than an
    /// error, a null, or a permission message.
    pub returned_protected_data: bool,
    /// Whether the parent object itself was owner-restricted.
    pub parent_owner: Option<String>,
    /// Identities declared to own the parent object.
    pub parent_visibility: ResourceVisibility,
    pub exchange_id: String,
}

impl GraphQlFieldAccess {
    pub fn read(
        root_field: impl Into<String>,
        field_path: impl Into<String>,
        identity: impl Into<String>,
    ) -> Self {
        Self {
            root_field: root_field.into(),
            field_path: field_path.into(),
            identity: identity.into(),
            resolved: false,
            returned_protected_data: false,
            parent_owner: None,
            parent_visibility: ResourceVisibility::Undetermined,
            exchange_id: String::new(),
        }
    }

    pub fn with_parent(mut self, owner: Option<String>, visibility: ResourceVisibility) -> Self {
        self.parent_owner = owner;
        self.parent_visibility = visibility;
        self
    }

    pub fn observed(mut self, resolved: bool, returned_protected_data: bool) -> Self {
        self.resolved = resolved;
        self.returned_protected_data = returned_protected_data;
        self
    }

    /// The same field access observed again against a second exchange.
    ///
    /// Independence is derived from distinct exchanges, so a repeat must carry
    /// its own exchange id to count toward the two-exchange floor.
    pub fn repeated_with_exchange(&self, exchange_id: impl Into<String>) -> Self {
        let mut clone = self.clone();
        clone.exchange_id = exchange_id.into();
        clone
    }

    /// Alias of [`Self::repeated_with_exchange`].
    pub fn clone_with_exchange(&self, exchange_id: impl Into<String>) -> Self {
        self.repeated_with_exchange(exchange_id)
    }

    pub fn with_exchange(mut self, exchange_id: impl Into<String>) -> Self {
        self.exchange_id = exchange_id.into();
        self
    }

    /// Whether this field was readable by an identity with no claim on it.
    fn is_unprotected(&self) -> bool {
        self.resolved
            && self.returned_protected_data
            && self.parent_visibility == ResourceVisibility::Private
            && self.parent_owner.as_deref() != Some(self.identity.as_str())
    }
}

/// The outcome of reviewing GraphQL field access.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphQlAssessment {
    pub verdict: AuthorizationVerdict,
    pub leaking_fields: Vec<String>,
    pub false_positive_causes: Vec<FalsePositiveCause>,
    pub unmet_requirements: Vec<AuthzRequirement>,
    pub explanation: String,
    pub supporting_exchanges: usize,
}

impl GraphQlAssessment {
    pub fn is_reportable(&self) -> bool {
        self.verdict.is_vulnerability()
            && self.unmet_requirements.is_empty()
            && self.supporting_exchanges >= 2
    }
}

/// Review GraphQL field access for authorization holes.
///
/// A field is only reported when the parent object is declared private, the
/// requesting identity is not its owner, the resolver resolved it, and the
/// returned value was protected data rather than a permission message. Anything
/// less is a lead.
pub fn analyze_graphql(accesses: &[GraphQlFieldAccess]) -> GraphQlAssessment {
    let leaks: Vec<&GraphQlFieldAccess> = accesses.iter().filter(|a| a.is_unprotected()).collect();

    if leaks.is_empty() {
        // A field that resolved to protected data through a parent nobody has
        // classified is a lead, not nothing. Reporting it as "nothing to see"
        // with no requirement attached is how an unclassified object quietly
        // stops being investigated.
        let unclassified_leads: Vec<&GraphQlFieldAccess> = accesses
            .iter()
            .filter(|a| {
                a.resolved
                    && a.returned_protected_data
                    && a.parent_visibility == ResourceVisibility::Undetermined
            })
            .collect();

        if !unclassified_leads.is_empty() {
            let mut fields: Vec<String> = unclassified_leads
                .iter()
                .map(|a| a.field_path.clone())
                .collect();
            fields.sort();
            fields.dedup();
            let exchanges = distinct_exchanges(unclassified_leads.iter().map(|a| &a.exchange_id));
            let mut requirements = vec![AuthzRequirement {
                kind: RequirementKind::DeclareOwner,
                detail: "the parent object's visibility is unclassified, so field-level \
                          authorization cannot be judged"
                    .into(),
            }];
            if exchanges < 2 {
                requirements.push(AuthzRequirement {
                    kind: RequirementKind::RepeatIndependently,
                    detail: format!(
                        "{exchanges} exchange(s) observed; a second independent request is \
                         required before this can be reported"
                    ),
                });
            }
            return GraphQlAssessment {
                verdict: AuthorizationVerdict::Inconclusive,
                leaking_fields: fields.clone(),
                false_positive_causes: vec![FalsePositiveCause::UnsupportedAssumption],
                unmet_requirements: requirements,
                explanation: format!(
                    "{} field(s) resolved to protected data through an object with no \
                     declared owner ({}); this is a lead until the parent is classified",
                    fields.len(),
                    fields.join(", ")
                ),
                supporting_exchanges: exchanges,
            };
        }

        let explanation = if accesses.is_empty() {
            "no GraphQL field access was observed".to_string()
        } else {
            "every observed field was either correctly denied, resolved to nothing, \
             or reached through an object the identity legitimately owns or that is \
             intentionally public"
                .to_string()
        };
        let correct = accesses.iter().any(|a| !a.resolved);
        return GraphQlAssessment {
            verdict: if correct {
                AuthorizationVerdict::CorrectlyEnforced
            } else {
                AuthorizationVerdict::Inconclusive
            },
            leaking_fields: Vec::new(),
            false_positive_causes: Vec::new(),
            unmet_requirements: if accesses.is_empty() {
                vec![AuthzRequirement {
                    kind: RequirementKind::EstablishBaseline,
                    detail: "no GraphQL field access was observed".into(),
                }]
            } else {
                Vec::new()
            },
            explanation,
            supporting_exchanges: distinct_exchanges(accesses.iter().map(|a| &a.exchange_id)),
        };
    }

    let mut causes = Vec::new();
    let mut requirements = Vec::new();

    // A globally readable identifier is explicitly not a finding on its own:
    // if the parent is public or shared, the field is reachable by design.
    let all_leaks_public = leaks
        .iter()
        .all(|a| a.parent_visibility == ResourceVisibility::Public);
    if all_leaks_public {
        causes.push(FalsePositiveCause::PublicByDesign);
    }
    if leaks
        .iter()
        .all(|a| a.parent_visibility == ResourceVisibility::Undetermined)
    {
        causes.push(FalsePositiveCause::UnsupportedAssumption);
        requirements.push(AuthzRequirement {
            kind: RequirementKind::DeclareOwner,
            detail: "the parent object's visibility is unclassified, so field-level \
                      authorization cannot be judged"
                .into(),
        });
    }

    let exchanges = distinct_exchanges(leaks.iter().map(|a| &a.exchange_id));
    if exchanges < 2 {
        requirements.push(AuthzRequirement {
            kind: RequirementKind::RepeatIndependently,
            detail: format!(
                "only {exchanges} exchange(s) support this; a second independent request \
                 is required before reporting"
            ),
        });
    }

    let mut leaking_fields: Vec<String> = leaks.iter().map(|a| a.field_path.clone()).collect();
    leaking_fields.sort();
    leaking_fields.dedup();
    let leaked_count = leaking_fields.len();
    let leaked_list = leaking_fields.join(", ");

    GraphQlAssessment {
        verdict: if !causes.is_empty() && !requirements.is_empty() {
            AuthorizationVerdict::Inconclusive
        } else {
            AuthorizationVerdict::GraphQlFieldLeak
        },
        leaking_fields,
        false_positive_causes: causes,
        unmet_requirements: requirements,
        explanation: format!(
            "{} GraphQL field(s) resolved to protected data for an identity that does not \
             own the parent object: {}",
            leaked_count, leaked_list
        ),
        supporting_exchanges: exchanges,
    }
}

fn distinct_exchanges<'a>(ids: impl Iterator<Item = &'a String>) -> usize {
    let set: BTreeSet<&str> = ids
        .filter(|id| !id.is_empty())
        .map(|id| id.as_str())
        .collect();
    set.len()
}

// ---------------------------------------------------------------------------
// The differential analyzer
// ---------------------------------------------------------------------------

/// Everything known about one access pattern: who tried, what they tried to
/// reach, what the policy says, and what came back.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessCase {
    pub identities: Vec<TestIdentity>,
    pub resources: Vec<ResourceRef>,
    #[serde(default)]
    pub matrix: AuthorizationMatrix,
    pub observations: Vec<AccessObservation>,
    /// Identity -> resource pairs where the resource was confirmed absent, used
    /// to tell "denied" apart from "does not exist" and to rule out fixture
    /// drift.
    pub confirmed_absent: Vec<(String, String)>,
}

impl AccessCase {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_identity(mut self, identity: TestIdentity) -> Self {
        self.identities.push(identity);
        self
    }

    pub fn with_resource(mut self, resource: ResourceRef) -> Self {
        self.resources.push(resource);
        self
    }

    pub fn with_matrix(mut self, matrix: AuthorizationMatrix) -> Self {
        self.matrix = matrix;
        self
    }

    pub fn with_observation(mut self, observation: AccessObservation) -> Self {
        self.observations.push(observation);
        self
    }

    fn identity(&self, id: &str) -> Option<&TestIdentity> {
        self.identities.iter().find(|i| i.id == id)
    }

    fn resource(&self, label: &str) -> Option<&ResourceRef> {
        self.resources.iter().find(|r| r.report_label() == label)
    }
}

/// Analyze one (identity, resource, operation) access pattern.
///
/// The function is total: every input produces an [`AuthorizationAssessment`],
/// and anything short of full evidence produces
/// [`AuthorizationVerdict::Inconclusive`] with the specific missing requirement
/// rather than a guess.
pub fn analyze_access(
    case: &AccessCase,
    identity_id: &str,
    resource_label: &str,
) -> AuthorizationAssessment {
    analyze_access_with_operation(case, identity_id, resource_label, AccessOperation::Read)
}

/// [`analyze_access`] for a non-read operation.
pub fn analyze_access_with_operation(
    case: &AccessCase,
    identity_id: &str,
    resource_label: &str,
    operation: AccessOperation,
) -> AuthorizationAssessment {
    let identity = case.identity(identity_id);
    let resource = case.resource(resource_label);

    let observations: Vec<&AccessObservation> = case
        .observations
        .iter()
        .filter(|o| {
            o.identity == identity_id && o.resource == resource_label && o.operation == operation
        })
        .collect();
    let supporting_exchanges = distinct_exchanges(observations.iter().map(|o| &o.exchange_id));

    let mut causes = Vec::new();
    let mut requirements = Vec::new();

    let finish = |verdict: AuthorizationVerdict,
                  causes: Vec<FalsePositiveCause>,
                  requirements: Vec<AuthzRequirement>,
                  explanation: String|
     -> AuthorizationAssessment {
        AuthorizationAssessment {
            verdict,
            identity: identity_id.to_string(),
            resource: resource_label.to_string(),
            operation,
            supporting_exchanges,
            false_positive_causes: causes,
            unmet_requirements: requirements,
            explanation,
        }
    };

    if observations.is_empty() {
        requirements.push(AuthzRequirement {
            kind: RequirementKind::EstablishBaseline,
            detail: format!("{identity_id} was never observed accessing {resource_label}"),
        });
        return finish(
            AuthorizationVerdict::Inconclusive,
            causes,
            requirements,
            format!("no observation of {identity_id} accessing {resource_label}"),
        );
    }

    // An unauthenticated or stale session makes every response uninformative
    // about the authorization check.
    let session_usable = identity.is_none_or(|i| i.authenticated);
    if !session_usable {
        causes.push(FalsePositiveCause::StaleSession);
        requirements.push(AuthzRequirement {
            kind: RequirementKind::FreshSession,
            detail: format!(
                "{identity_id} has no usable session, so its responses say \
                              nothing about the authorization check"
            ),
        });
    }

    let Some(identity) = identity else {
        requirements.push(AuthzRequirement {
            kind: RequirementKind::DeclareOwner,
            detail: format!("{identity_id} is not a declared controlled identity"),
        });
        return finish(
            AuthorizationVerdict::Inconclusive,
            causes,
            requirements,
            format!(
                "{identity_id} is not in the identity matrix, so no authorization \
                      expectation exists for it"
            ),
        );
    };

    if identity.is_anonymous() {
        // An unauthenticated caller is the strongest authorization signal there
        // is, but it still needs the object to be declared private.
        if resource.is_some_and(|r| r.visibility == ResourceVisibility::Public) {
            causes.push(FalsePositiveCause::PublicByDesign);
        }
    }

    let Some(resource) = resource else {
        requirements.push(AuthzRequirement {
            kind: RequirementKind::DeclareOwner,
            detail: format!("{resource_label} is not a declared resource"),
        });
        return finish(
            AuthorizationVerdict::Inconclusive,
            causes,
            requirements,
            format!(
                "{resource_label} is not a declared resource, so its authorization \
                      expectation is unknown"
            ),
        );
    };

    match resource.visibility {
        ResourceVisibility::Public => {
            causes.push(FalsePositiveCause::PublicByDesign);
            return finish(
                AuthorizationVerdict::IntentionallyPublic,
                causes,
                requirements,
                format!(
                    "{} is declared intentionally public, so {} accessing it establishes \
                     nothing",
                    resource_label, identity_id
                ),
            );
        }
        ResourceVisibility::Shared => {
            if resource.shared_with.contains(&identity.id) {
                causes.push(FalsePositiveCause::AuthorizedSharing);
                return finish(
                    AuthorizationVerdict::AuthorizedAccess,
                    causes,
                    requirements,
                    format!(
                        "{resource_label} is deliberately shared with {}, so access is \
                         authorized",
                        identity.report_label()
                    ),
                );
            }
        }
        ResourceVisibility::Undetermined => {
            causes.push(FalsePositiveCause::UnsupportedAssumption);
            requirements.push(AuthzRequirement {
                kind: RequirementKind::DeclareOwner,
                detail: format!(
                    "{resource_label} has no declared visibility or owner, so an access to \
                     it cannot be judged unauthorized"
                ),
            });
        }
        ResourceVisibility::Private => {}
    }

    let declared_allowed = resource.declares_access_for(identity);
    if declared_allowed {
        return finish(
            AuthorizationVerdict::AuthorizedAccess,
            causes,
            requirements,
            format!(
                "{} owns {} ({}), so the observed access is authorized",
                identity.report_label(),
                resource_label,
                operation.as_str()
            ),
        );
    }

    // A declared expectation overrides the ownership reading when the two
    // disagree, and the disagreement is itself worth surfacing.
    if let Some(expected_allowed) = case.matrix.expectation(&identity.id, resource_label)
        && expected_allowed
    {
        causes.push(FalsePositiveCause::AuthorizedSharing);
        return finish(
            AuthorizationVerdict::AuthorizedAccess,
            causes,
            requirements,
            format!(
                "the authorization matrix declares {} may access {resource_label}, so the \
                 observed access is authorized",
                identity.report_label()
            ),
        );
    }

    let returned = observations
        .iter()
        .any(|o| o.outcome.returned_protected_data());
    let denied = observations
        .iter()
        .any(|o| o.outcome == ObservedOutcome::Denied);
    let empty = observations
        .iter()
        .all(|o| o.outcome == ObservedOutcome::Empty);

    if empty && !returned {
        return finish(
            AuthorizationVerdict::CorrectlyEnforced,
            causes,
            requirements,
            format!(
                "{} received no object body from {resource_label}, so nothing protected \
                 was disclosed",
                identity.report_label()
            ),
        );
    }

    if denied && !returned {
        return finish(
            AuthorizationVerdict::CorrectlyEnforced,
            causes,
            requirements,
            format!(
                "access control engaged: {} was refused {resource_label} ({})",
                identity.report_label(),
                operation.as_str()
            ),
        );
    }

    if !returned {
        // Something was accepted but no protected body came back: an empty
        // list, a placeholder, or an error. Not a disclosure.
        requirements.push(AuthzRequirement {
            kind: RequirementKind::ShowProtectedData,
            detail: format!(
                "{resource_label} responded {} but no protected field was identified in \
                 the body",
                observations
                    .first()
                    .map(|o| o.outcome.as_str())
                    .unwrap_or("unknown")
            ),
        });
        return finish(
            AuthorizationVerdict::Inconclusive,
            causes,
            requirements,
            format!(
                "{} reached {resource_label} but the response did not demonstrably contain \
                 protected data",
                identity.report_label()
            ),
        );
    }

    // From here on the response demonstrably contained protected data.
    if resource.visibility == ResourceVisibility::Undetermined {
        // Already handled above, but stated explicitly so the invariant is local.
        return finish(
            AuthorizationVerdict::Inconclusive,
            causes,
            requirements,
            format!(
                "protected data from {resource_label} reached {} but the object has no \
                 declared owner, so unauthorized access is not established",
                identity.report_label()
            ),
        );
    }

    let marker_present = observations
        .iter()
        .any(|o| o.protected_data_marker.is_some());
    if !marker_present {
        requirements.push(AuthzRequirement {
            kind: RequirementKind::ShowProtectedData,
            detail: "name the protected field that was returned, so impact is demonstrable \
                      rather than inferred from a status code"
                .into(),
        });
    }

    // The control must be shown to work: the same authorization check must have
    // refused this identity on some other resource it does not own.
    //
    // The earlier version looked for a denial on a resource the identity *owns*,
    // which can never happen — an owner is not refused its own object — so this
    // requirement could never be satisfied and every real IDOR stayed
    // inconclusive. The control is the deny on somebody else's object.
    let control_works = case.observations.iter().any(|o| {
        if o.identity != identity.id || o.outcome != ObservedOutcome::Denied {
            return false;
        }
        // The denied resource must not be one the identity is entitled to,
        // otherwise the denial proves nothing about object-level authorization.
        case.resources
            .iter()
            .find(|r| r.report_label() == o.resource)
            .is_none_or(|r| !r.declares_access_for(identity))
    });
    if !control_works {
        requirements.push(AuthzRequirement {
            kind: RequirementKind::EstablishBaseline,
            detail: format!(
                "no denied response was captured for {} on any resource it does not own, \
                 so the authorization check has not been shown to work at all",
                identity.report_label()
            ),
        });
    }

    if supporting_exchanges < 2 {
        requirements.push(AuthzRequirement {
            kind: RequirementKind::RepeatIndependently,
            detail: format!(
                "{supporting_exchanges} exchange(s) support this claim; a second \
                 independent request is required so the result does not rest on one \
                 cached response"
            ),
        });
    }

    if case
        .confirmed_absent
        .iter()
        .any(|(i, r)| i == identity_id && r == resource_label)
    {
        causes.push(FalsePositiveCause::FixtureInconsistency);
    }

    let cross_tenant = identity
        .tenant
        .as_ref()
        .zip(resource.tenant.as_ref())
        .is_some_and(|(a, b)| a != b);
    let role_escalation = !identity.has_role(&Role::Owner)
        && !identity.has_role(&Role::Admin)
        && identity.has_role(&Role::Support)
        && operation.changes_state();

    let verdict = if !requirements.is_empty() {
        AuthorizationVerdict::Inconclusive
    } else if cross_tenant {
        AuthorizationVerdict::CrossTenantLeak
    } else if role_escalation {
        AuthorizationVerdict::PrivilegeEscalation
    } else {
        AuthorizationVerdict::GenuineUnauthorizedAccess
    };

    let explanation = match verdict {
        AuthorizationVerdict::CrossTenantLeak => format!(
            "protected data owned by {} in tenant {} reached {}, which belongs to tenant \
             {:?}, with the authorization check shown to work on comparable data",
            resource.report_label(),
            resource.tenant.as_deref().unwrap_or("?"),
            identity.report_label(),
            identity.tenant
        ),
        AuthorizationVerdict::PrivilegeEscalation => format!(
            "{} ({}) executed a state-changing {} on {resource_label} owned by another \
             identity, which the declared policy reserves",
            identity.report_label(),
            identity
                .roles
                .iter()
                .map(|r| r.as_str())
                .collect::<Vec<_>>()
                .join("/"),
            operation.as_str()
        ),
        AuthorizationVerdict::GenuineUnauthorizedAccess => format!(
            "{} received protected data from {resource_label} ({}), which it does not own \
             or share, while the same authorization check denied a comparable owned \
             resource — confirmed across {supporting_exchanges} independent exchanges",
            identity.report_label(),
            operation.as_str()
        ),
        _ => format!(
            "protected data from {resource_label} reached {}, but \
             {} requirement(s) are unmet",
            identity.report_label(),
            requirements.len()
        ),
    };

    finish(verdict, causes, requirements, explanation)
}

/// Analyze every identity against every resource and return the assessments.
///
/// Convenience for the report layer: one call produces the full matrix, so a
/// report can state what was tested and not only what was found.
pub fn analyze_matrix(case: &AccessCase) -> Vec<AuthorizationAssessment> {
    let mut out = Vec::new();
    for resource in &case.resources {
        for identity in &case.identities {
            out.push(analyze_access(case, &identity.id, &resource.report_label()));
        }
    }
    out
}

/// Copy the assessment's authentication context onto a finding.
///
/// Exists so the validation engine's IDOR disproof branch — which reads
/// `Finding::auth_context` — has a real producer. Before this, that field was
/// initialised to `"unknown"` and never written, so the branch was dead code.
pub fn apply_to_finding(assessment: &AuthorizationAssessment, finding: &mut Finding) {
    finding.auth_context = assessment.finding_auth_context();
    finding.add_note(format!(
        "Authorization matrix: {} -> {} ({}) = {}",
        assessment.identity,
        assessment.resource,
        assessment.operation.as_str(),
        assessment.verdict.as_str()
    ));
    for cause in &assessment.false_positive_causes {
        finding.add_note(format!(
            "False-positive cause considered: {}",
            cause.as_str()
        ));
    }
    for requirement in &assessment.unmet_requirements {
        finding.add_note(format!(
            "Unmet authorization requirement: {} — {}",
            requirement.kind.as_str(),
            requirement.kind.next_test()
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner() -> TestIdentity {
        TestIdentity::new("owner-1", "tester-owner-a", vec![Role::Owner]).with_tenant("t1")
    }

    fn outsider() -> TestIdentity {
        TestIdentity::new("low-1", "tester-lowpriv-b", vec![Role::Member]).with_tenant("t2")
    }

    fn support() -> TestIdentity {
        TestIdentity::new("sup-1", "tester-support-c", vec![Role::Support]).with_tenant("t2")
    }

    // --- the happy path: a real IDOR --------------------------------------

    #[test]
    fn genuine_idor_is_confirmed_only_with_owner_baseline_and_two_exchanges() {
        // Same tenant for both identities: this is the plain-IDOR case. The
        // cross-tenant case has its own test, and mixing the two here made the
        // expected verdict wrong.
        let invoice = ResourceRef::owned_by("invoice", "1", "owner-1").with_tenant("t2");
        let own_invoice = ResourceRef::owned_by("invoice", "2", "low-1").with_tenant("t2");
        let case = AccessCase::new()
            .with_identity(owner())
            .with_identity(outsider())
            .with_resource(invoice.clone())
            .with_resource(own_invoice.clone())
            .with_observation(
                AccessObservation::new(
                    "low-1",
                    invoice.report_label(),
                    AccessOperation::Read,
                    ObservedOutcome::Returned,
                )
                .with_marker("owner ssn")
                .with_exchange("x1"),
            )
            .with_observation(
                AccessObservation::new(
                    "low-1",
                    invoice.report_label(),
                    AccessOperation::Read,
                    ObservedOutcome::Returned,
                )
                .with_marker("owner ssn")
                .with_exchange("x2")
                .as_independent_repeat(),
            )
            .with_observation(AccessObservation::new(
                "low-1",
                own_invoice.report_label(),
                AccessOperation::Read,
                ObservedOutcome::Returned,
            ))
            .with_observation(AccessObservation::new(
                "low-1",
                "invoice:999",
                AccessOperation::Read,
                ObservedOutcome::Denied,
            ));

        let assessment = analyze_access(&case, "low-1", "invoice:1");
        assert_eq!(
            assessment.verdict,
            AuthorizationVerdict::GenuineUnauthorizedAccess
        );
        assert!(assessment.is_reportable(), "{assessment:#?}");
        assert_eq!(assessment.confidence(), Confidence::Certain);
        assert_eq!(assessment.supporting_exchanges, 2);
        assert!(
            assessment.explanation.contains("does not own"),
            "{}",
            assessment.explanation
        );
    }

    // --- one missing exchange must block confirmation ---------------------

    #[test]
    fn one_exchange_is_not_enough_to_confirm() {
        let invoice = ResourceRef::owned_by("invoice", "1", "owner-1");
        let own_invoice = ResourceRef::owned_by("invoice", "2", "low-1");
        let case = AccessCase::new()
            .with_identity(outsider())
            .with_resource(invoice.clone())
            .with_resource(own_invoice)
            .with_observation(
                AccessObservation::new(
                    "low-1",
                    "invoice:1",
                    AccessOperation::Read,
                    ObservedOutcome::Returned,
                )
                .with_marker("owner ssn")
                .with_exchange("x1"),
            )
            .with_observation(AccessObservation::new(
                "low-1",
                "invoice:999",
                AccessOperation::Read,
                ObservedOutcome::Denied,
            ));

        let assessment = analyze_access(&case, "low-1", "invoice:1");
        assert_eq!(assessment.verdict, AuthorizationVerdict::Inconclusive);
        assert!(!assessment.is_reportable());
        assert!(
            assessment
                .unmet_requirements
                .iter()
                .any(|r| r.kind == RequirementKind::RepeatIndependently),
            "{assessment:#?}"
        );
        assert!(
            !assessment
                .unmet_requirements
                .iter()
                .any(|r| r.kind == RequirementKind::EstablishBaseline),
            "this case does capture a denial on a resource the identity does not own, \
             so the baseline requirement is satisfied: {assessment:#?}"
        );
    }

    #[test]
    fn without_a_baseline_denial_the_claim_stays_inconclusive() {
        // The mirror of the previous test: same evidence, minus the one control
        // response. Two exchanges and a named protected field are not enough on
        // their own — without a denial somewhere, the authorization check has
        // not been shown to work at all, so the leak could be a route that was
        // never subject to one.
        let invoice = ResourceRef::owned_by("invoice", "1", "owner-1");
        let case = AccessCase::new()
            .with_identity(outsider())
            .with_resource(invoice.clone())
            .with_observation(
                AccessObservation::new(
                    "low-1",
                    "invoice:1",
                    AccessOperation::Read,
                    ObservedOutcome::Returned,
                )
                .with_marker("owner ssn")
                .with_exchange("x1"),
            )
            .with_observation(
                AccessObservation::new(
                    "low-1",
                    "invoice:1",
                    AccessOperation::Read,
                    ObservedOutcome::Returned,
                )
                .with_marker("owner ssn")
                .with_exchange("x2")
                .as_independent_repeat(),
            );

        let assessment = analyze_access(&case, "low-1", "invoice:1");
        assert_eq!(assessment.verdict, AuthorizationVerdict::Inconclusive);
        assert!(!assessment.is_reportable());
        assert!(
            assessment
                .unmet_requirements
                .iter()
                .any(|r| r.kind == RequirementKind::EstablishBaseline),
            "{assessment:#?}"
        );
        // Reproduced twice and the field is named, so only the baseline is
        // missing — which is exactly what the message must tell the
        // investigator to go and capture.
        assert_eq!(assessment.unmet_requirements.len(), 1);
    }

    // --- false positives ---------------------------------------------------

    #[test]
    fn public_resource_is_never_a_finding() {
        let resource = ResourceRef::public_("status", "health");
        let case = AccessCase::new()
            .with_identity(TestIdentity::anonymous())
            .with_resource(resource.clone())
            .with_observation(
                AccessObservation::new(
                    "anon",
                    resource.report_label(),
                    AccessOperation::Read,
                    ObservedOutcome::Returned,
                )
                .with_exchange("x1")
                .as_independent_repeat(),
            );

        let assessment = analyze_access(&case, "anon", "status:health");
        assert_eq!(
            assessment.verdict,
            AuthorizationVerdict::IntentionallyPublic
        );
        assert!(!assessment.verdict.is_vulnerability());
        assert!(
            assessment
                .false_positive_causes
                .contains(&FalsePositiveCause::PublicByDesign)
        );
    }

    #[test]
    fn authorized_sharing_is_not_a_finding() {
        let resource = ResourceRef::owned_by("doc", "9", "owner-1")
            .with_visibility(ResourceVisibility::Shared)
            .shares_with(&["low-1"]);
        let case = AccessCase::new()
            .with_identity(outsider())
            .with_resource(resource.clone())
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

        let assessment = analyze_access(&case, "low-1", "doc:9");
        assert_eq!(assessment.verdict, AuthorizationVerdict::AuthorizedAccess);
        assert!(!assessment.verdict.is_vulnerability());
    }

    #[test]
    fn a_stale_session_makes_every_response_uninformative() {
        let resource = ResourceRef::owned_by("invoice", "1", "owner-1");
        let mut stale = outsider();
        stale.authenticated = false;
        let case = AccessCase::new()
            .with_identity(stale)
            .with_resource(resource.clone())
            .with_observation(
                AccessObservation::new(
                    "low-1",
                    "invoice:1",
                    AccessOperation::Read,
                    ObservedOutcome::Returned,
                )
                .with_marker("x")
                .with_exchange("x1")
                .as_independent_repeat(),
            );

        let assessment = analyze_access(&case, "low-1", "invoice:1");
        assert!(
            assessment
                .false_positive_causes
                .contains(&FalsePositiveCause::StaleSession)
        );
        assert!(
            assessment
                .unmet_requirements
                .iter()
                .any(|r| r.kind == RequirementKind::FreshSession)
        );
        assert!(!assessment.is_reportable());
    }

    #[test]
    fn an_unclassified_object_is_a_lead_not_a_finding() {
        // The core anti-false-positive rule: an identifier that looks like
        // another user's is not evidence of anything.
        let resource = ResourceRef::unclassified("object", "550e8400-e29b-41d4-a716-446655440000");
        let case = AccessCase::new()
            .with_identity(outsider())
            .with_resource(resource.clone())
            .with_observation(
                AccessObservation::new(
                    "low-1",
                    resource.report_label(),
                    AccessOperation::Read,
                    ObservedOutcome::Returned,
                )
                .with_marker("data")
                .with_exchange("x1")
                .as_independent_repeat(),
            );

        let assessment = analyze_access(&case, "low-1", &resource.report_label());
        assert_eq!(assessment.verdict, AuthorizationVerdict::Inconclusive);
        assert!(!assessment.verdict.is_vulnerability());
        assert!(
            assessment
                .false_positive_causes
                .contains(&FalsePositiveCause::UnsupportedAssumption)
        );
        assert!(
            assessment
                .unmet_requirements
                .iter()
                .any(|r| r.kind == RequirementKind::DeclareOwner)
        );
    }

    #[test]
    fn correct_enforcement_is_reported_as_correct_enforcement() {
        let resource = ResourceRef::owned_by("invoice", "1", "owner-1");
        let case = AccessCase::new()
            .with_identity(outsider())
            .with_resource(resource.clone())
            .with_observation(
                AccessObservation::new(
                    "low-1",
                    "invoice:1",
                    AccessOperation::Read,
                    ObservedOutcome::Denied,
                )
                .with_exchange("x1"),
            );

        let assessment = analyze_access(&case, "low-1", "invoice:1");
        assert_eq!(assessment.verdict, AuthorizationVerdict::CorrectlyEnforced);
        assert_eq!(assessment.confidence(), Confidence::High);
    }

    #[test]
    fn a_200_with_no_protected_field_is_not_a_disclosure() {
        let resource = ResourceRef::owned_by("invoice", "1", "owner-1");
        let case = AccessCase::new()
            .with_identity(outsider())
            .with_resource(resource.clone())
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

        let assessment = analyze_access(&case, "low-1", "invoice:1");
        assert_eq!(assessment.verdict, AuthorizationVerdict::Inconclusive);
        assert!(
            assessment
                .unmet_requirements
                .iter()
                .any(|r| r.kind == RequirementKind::ShowProtectedData)
        );
    }

    // --- cross-tenant and privilege escalation -----------------------------

    #[test]
    fn cross_tenant_leak_is_distinguished_from_plain_idor() {
        let invoice = ResourceRef::owned_by("invoice", "1", "owner-1").with_tenant("tenant-a");
        let own = ResourceRef::owned_by("invoice", "2", "low-1").with_tenant("tenant-b");
        let case = AccessCase::new()
            .with_identity(outsider())
            .with_resource(invoice.clone())
            .with_resource(own)
            .with_observation(
                AccessObservation::new(
                    "low-1",
                    "invoice:1",
                    AccessOperation::Read,
                    ObservedOutcome::Returned,
                )
                .with_marker("tenant-a payroll")
                .with_exchange("x1"),
            )
            .with_observation(
                AccessObservation::new(
                    "low-1",
                    "invoice:1",
                    AccessOperation::Read,
                    ObservedOutcome::Returned,
                )
                .with_marker("tenant-a payroll")
                .with_exchange("x2")
                .as_independent_repeat(),
            )
            .with_observation(AccessObservation::new(
                "low-1",
                "invoice:404",
                AccessOperation::Read,
                ObservedOutcome::Denied,
            ));

        let assessment = analyze_access(&case, "low-1", "invoice:1");
        assert_eq!(assessment.verdict, AuthorizationVerdict::CrossTenantLeak);
        assert_eq!(
            assessment.verdict.vulnerability_class(),
            VulnerabilityClass::CrossTenantAccess
        );
    }

    #[test]
    fn support_role_writing_another_identitys_object_is_privilege_escalation() {
        let invoice = ResourceRef::owned_by("invoice", "1", "owner-1");
        let own = ResourceRef::owned_by("invoice", "2", "sup-1");
        let case = AccessCase::new()
            .with_identity(support())
            .with_resource(invoice.clone())
            .with_resource(own)
            .with_observation(
                AccessObservation::new(
                    "sup-1",
                    "invoice:1",
                    AccessOperation::Update,
                    ObservedOutcome::Returned,
                )
                .with_marker("updated")
                .with_exchange("x1"),
            )
            .with_observation(
                AccessObservation::new(
                    "sup-1",
                    "invoice:1",
                    AccessOperation::Update,
                    ObservedOutcome::Returned,
                )
                .with_marker("updated")
                .with_exchange("x2")
                .as_independent_repeat(),
            )
            .with_observation(AccessObservation::new(
                "sup-1",
                "invoice:404",
                AccessOperation::Update,
                ObservedOutcome::Denied,
            ));

        // The operation matters: these are `Update` observations, so the
        // operation-aware entry point is required. Calling `analyze_access`
        // (which defaults to `Read`) silently matched nothing and returned
        // "never observed", which is the failure mode typed operations exist to
        // prevent.
        let assessment =
            analyze_access_with_operation(&case, "sup-1", "invoice:1", AccessOperation::Update);
        assert_eq!(
            assessment.verdict,
            AuthorizationVerdict::PrivilegeEscalation
        );
        assert_eq!(
            assessment.verdict.vulnerability_class(),
            VulnerabilityClass::PrivilegeEscalation
        );
        assert!(
            assessment.explanation.contains("state-changing"),
            "{}",
            assessment.explanation
        );
    }

    #[test]
    fn the_same_identity_reading_its_own_object_is_authorized() {
        let invoice = ResourceRef::owned_by("invoice", "1", "low-1");
        let case = AccessCase::new()
            .with_identity(outsider())
            .with_resource(invoice.clone())
            .with_observation(
                AccessObservation::new(
                    "low-1",
                    "invoice:1",
                    AccessOperation::Read,
                    ObservedOutcome::Returned,
                )
                .with_exchange("x1"),
            );

        let assessment = analyze_access(&case, "low-1", "invoice:1");
        assert_eq!(assessment.verdict, AuthorizationVerdict::AuthorizedAccess);
        assert!(
            !assessment
                .false_positive_causes
                .contains(&FalsePositiveCause::SameIdentity)
                || assessment.verdict == AuthorizationVerdict::AuthorizedAccess
        );
    }

    // --- GraphQL ------------------------------------------------------------

    #[test]
    fn graphql_field_on_a_private_object_resolved_for_a_non_owner_is_a_leak() {
        let accesses = vec![
            GraphQlFieldAccess::read("invoice", "invoice.owner.ssn", "low-1")
                .with_parent(Some("owner-1".into()), ResourceVisibility::Private)
                .observed(true, true)
                .with_exchange("g1"),
            GraphQlFieldAccess::read("invoice", "invoice.owner.ssn", "low-1")
                .with_parent(Some("owner-1".into()), ResourceVisibility::Private)
                .observed(true, true)
                .with_exchange("g2"),
        ];
        let assessment = analyze_graphql(&accesses);
        assert_eq!(assessment.verdict, AuthorizationVerdict::GraphQlFieldLeak);
        assert!(assessment.is_reportable(), "{assessment:#?}");
        assert_eq!(
            assessment.leaking_fields,
            vec!["invoice.owner.ssn".to_string()]
        );
    }

    #[test]
    fn a_globally_readable_identifier_on_a_public_object_is_not_a_leak() {
        // The specific trap: an id visible in a public response, reachable by
        // anyone by design. Object discovery is not unauthorized access.
        let accesses = vec![
            GraphQlFieldAccess::read("publicProfile", "publicProfile.id", "anon")
                .with_parent(None, ResourceVisibility::Public)
                .observed(true, true)
                .with_exchange("g1"),
        ];
        let assessment = analyze_graphql(&accesses);
        assert!(!assessment.verdict.is_vulnerability(), "{assessment:#?}");
        assert!(assessment.leaking_fields.is_empty());
    }

    #[test]
    fn a_correctly_denied_graphql_resolver_is_correctly_enforced() {
        let accesses = vec![
            GraphQlFieldAccess::read("invoice", "invoice.owner.ssn", "low-1")
                .with_parent(Some("owner-1".into()), ResourceVisibility::Private)
                .observed(false, false)
                .with_exchange("g1"),
        ];
        let assessment = analyze_graphql(&accesses);
        assert_eq!(assessment.verdict, AuthorizationVerdict::CorrectlyEnforced);
    }

    #[test]
    fn an_unclassified_graphql_parent_cannot_produce_a_finding() {
        let accesses = vec![
            GraphQlFieldAccess::read("invoice", "invoice.owner.ssn", "low-1")
                .with_parent(None, ResourceVisibility::Undetermined)
                .observed(true, true)
                .with_exchange("g1")
                .clone_with_exchange("g2"),
        ];
        let assessment = analyze_graphql(&accesses);
        assert_eq!(assessment.verdict, AuthorizationVerdict::Inconclusive);
        assert!(
            assessment
                .unmet_requirements
                .iter()
                .any(|r| r.kind == RequirementKind::DeclareOwner)
        );
    }

    // --- object relationships ---------------------------------------------

    #[test]
    fn object_relationships_surface_leads_without_asserting_access() {
        let mut graph = ObjectGraph::new();
        graph.observe_relationship("invoice:1", "User:42");
        graph.observe_relationship("invoice:1", "User:43");
        graph.observe_relationship("User:42", "address:7");

        assert_eq!(
            graph.leads_for("invoice:1"),
            vec!["User:42".to_string(), "User:43".to_string()]
        );
        assert_eq!(
            graph.leads_for("User:42"),
            vec!["address:7".to_string(), "invoice:1".to_string()]
        );
        assert_eq!(
            graph.leads_for("address:7"),
            vec!["User:42".to_string()],
            "a node with a neighbour is a lead in its own right"
        );
        assert!(
            graph.leads_for("object:never-seen").is_empty(),
            "an isolated node is not a lead"
        );
    }

    #[test]
    fn a_node_with_a_neighbour_is_a_lead_and_an_isolated_one_is_not() {
        // `address:7` does have a neighbour, so it is a lead in its own right.
        // The original assertion claimed otherwise and was wrong about what a
        // lead is: the point of the graph is that every discovered identifier
        // is somewhere to look next.
        let mut graph = ObjectGraph::new();
        graph.observe_relationship("invoice:1", "User:42");
        graph.observe_relationship("User:42", "address:7");

        assert_eq!(graph.leads_for("address:7"), vec!["User:42".to_string()]);
        assert!(
            graph.leads_for("object:never-seen").is_empty(),
            "an isolated node is not a lead"
        );
    }

    // --- integration with Finding ------------------------------------------

    #[test]
    fn applying_an_assessment_writes_a_real_auth_context() {
        // The validation engine's IDOR disproof branch reads
        // `Finding::auth_context == "unauthenticated"`. That field was never
        // assigned anywhere, so this is the producer that makes the branch
        // reachable.
        let resource = ResourceRef::owned_by("invoice", "1", "owner-1");
        let case = AccessCase::new()
            .with_identity(TestIdentity::anonymous())
            .with_resource(resource.clone())
            .with_observation(
                AccessObservation::new(
                    "anon",
                    "invoice:1",
                    AccessOperation::Read,
                    ObservedOutcome::Returned,
                )
                .with_marker("secret")
                .with_exchange("x1")
                .as_independent_repeat(),
            );
        let assessment = analyze_access(&case, "anon", "invoice:1");
        assert_eq!(assessment.finding_auth_context(), "unauthenticated");

        let mut finding = Finding::new(
            "f-1".into(),
            "anonymous read of another user's invoice".into(),
            VulnerabilityClass::IdorBola,
            "example.com".into(),
        );
        assert_eq!(finding.auth_context, "unknown");
        apply_to_finding(&assessment, &mut finding);
        assert_eq!(finding.auth_context, "unauthenticated");
        assert!(
            finding
                .notes
                .iter()
                .any(|n| n.contains("Unmet authorization")),
            "{:?}",
            finding.notes
        );
    }

    #[test]
    fn findings_from_assessments_carry_the_right_vulnerability_class() {
        for (verdict, expected) in [
            (
                AuthorizationVerdict::GenuineUnauthorizedAccess,
                VulnerabilityClass::IdorBola,
            ),
            (
                AuthorizationVerdict::PrivilegeEscalation,
                VulnerabilityClass::PrivilegeEscalation,
            ),
            (
                AuthorizationVerdict::CrossTenantLeak,
                VulnerabilityClass::CrossTenantAccess,
            ),
            (
                AuthorizationVerdict::GraphQlFieldLeak,
                VulnerabilityClass::GraphQlInjection,
            ),
        ] {
            assert_eq!(verdict.vulnerability_class(), expected);
            assert!(verdict.is_vulnerability());
        }
        for verdict in [
            AuthorizationVerdict::AuthorizedAccess,
            AuthorizationVerdict::CorrectlyEnforced,
            AuthorizationVerdict::IntentionallyPublic,
            AuthorizationVerdict::Inconclusive,
        ] {
            assert!(!verdict.is_vulnerability(), "{verdict:?}");
        }
    }

    // --- determinism --------------------------------------------------------

    #[test]
    fn analysis_is_deterministic_for_identical_input() {
        let invoice = ResourceRef::owned_by("invoice", "1", "owner-1");
        let case = AccessCase::new()
            .with_identity(outsider())
            .with_resource(invoice.clone())
            .with_observation(
                AccessObservation::new(
                    "low-1",
                    "invoice:1",
                    AccessOperation::Read,
                    ObservedOutcome::Returned,
                )
                .with_marker("m")
                .with_exchange("x1")
                .as_independent_repeat(),
            );
        let first = analyze_access(&case, "low-1", "invoice:1");
        let second = analyze_access(&case, "low-1", "invoice:1");
        assert_eq!(first, second);
    }

    #[test]
    fn analyze_matrix_covers_every_identity_resource_pair() {
        let case = AccessCase::new()
            .with_identity(owner())
            .with_identity(outsider())
            .with_resource(ResourceRef::owned_by("invoice", "1", "owner-1"))
            .with_resource(ResourceRef::owned_by("invoice", "2", "owner-1"));
        assert_eq!(analyze_matrix(&case).len(), 4);
    }

    #[test]
    fn role_labels_are_stable_and_custom_roles_round_trip() {
        assert_eq!(Role::Member.as_str(), "member");
        let role = Role::Custom("tenant-admin".into());
        assert_eq!(role.as_str(), "tenant-admin");
        let json = serde_json::to_string(&role).unwrap();
        // `Custom(String)` is an externally tagged newtype variant, so it
        // serializes as a one-key object rather than a bare string.
        assert_eq!(json, "{\"custom\":\"tenant-admin\"}");
        let back: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(back, role);
    }
}
