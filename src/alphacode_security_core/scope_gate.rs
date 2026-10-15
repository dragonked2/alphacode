//! Deterministic engagement-scope enforcement.
//!
//! This is the single source of truth for "is this host/URL in scope?" for
//! every network-capable tool. It is pure decision logic with no I/O, no tool
//! registration, and no provider dependence, so it is unit-testable in
//! isolation and cannot be influenced by an LLM, a target-controlled response,
//! or a misbehaving tool.
//!
//! Policy is fail-closed **when an allowlist actually exists**: a target that
//! matches no in-scope rule is denied in an enforcing engagement. Explicit
//! out-of-scope rules always win over in-scope rules, so redirects and
//! discovered subdomains can never silently widen the engagement.
//!
//! See `alphacode_app_core::tool::scope` for the per-session wiring that turns
//! this module into a gate in front of every tool call.

use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr};

/// A parsed, human-authored scope rule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ScopePattern {
    /// Exact hostname match. `Host("api.example.com")` matches only
    /// `api.example.com`, not `example.com` or `x.api.example.com`.
    Host(String),
    /// Domain and all subdomains: `Domain("example.com")` matches
    /// `example.com` and `api.example.com`.
    Domain(String),
    /// Subdomains only, not the apex: `SubdomainsOf("example.com")` (from
    /// `*.example.com`) matches `api.example.com` but not `example.com`.
    SubdomainsOf(String),
    /// Exact IP address (v4 or v6).
    Ip(IpAddr),
    /// IPv4 CIDR range, e.g. `203.0.113.0/24`. Network bits are stored
    /// pre-masked so matching is a single shift + compare.
    Ipv4Cidr { network: u32, prefix: u8 },
    /// URL-path prefix, e.g. `https://example.com/api`. Restricts both the
    /// host and the path prefix; useful for engagement scopes that allow only
    /// an API prefix of a shared host.
    UrlPrefix(String),
}

impl ScopePattern {
    /// Parse one human-authored rule into a pattern. Accepts, in order:
    /// `203.0.113.5/24`, an IP literal, `*.example.com`, a full URL prefix,
    /// and everything else as a hostname/domain.
    pub fn parse(input: &str) -> Option<ScopePattern> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Some(cidr) = parse_ipv4_cidr(trimmed) {
            return Some(cidr);
        }
        if let Ok(ip) = trimmed.parse::<IpAddr>() {
            return Some(ScopePattern::Ip(ip));
        }
        if let Some(suffix) = trimmed.strip_prefix("*.") {
            let suffix = suffix.trim();
            if !suffix.is_empty() {
                return Some(ScopePattern::SubdomainsOf(suffix.to_ascii_lowercase()));
            }
        }
        if trimmed.contains("://") {
            return Some(ScopePattern::UrlPrefix(trimmed.to_string()));
        }
        normalize_host(trimmed).map(ScopePattern::Domain)
    }

    /// Human-readable form for blocking messages and logs.
    pub fn describe(&self) -> String {
        match self {
            ScopePattern::Host(h) => format!("host:{h}"),
            ScopePattern::Domain(d) => format!("domain:{d}"),
            ScopePattern::SubdomainsOf(d) => format!("*.{d}"),
            ScopePattern::Ip(ip) => format!("ip:{ip}"),
            ScopePattern::Ipv4Cidr { prefix, .. } => {
                format!("ipv4_cidr:/{prefix}")
            }
            ScopePattern::UrlPrefix(p) => format!("url_prefix:{p}"),
        }
    }
}

/// Engagement scope: the parsed equivalent of `[scope]` config, plus the
/// derived decision knobs (`fail_closed`).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ScopePolicy {
    /// Authorized in-scope patterns. Empty means "no allowlist declared".
    #[serde(default)]
    pub in_scope: Vec<ScopePattern>,
    /// Explicit exclusions, evaluated before in-scope and always winning.
    #[serde(default)]
    pub out_of_scope: Vec<ScopePattern>,
    /// Block unmatched targets. Only meaningful when `in_scope` is non-empty;
    /// with no allowlist there is nothing to fail closed against, and the gate
    /// only enforces explicit exclusions.
    pub fail_closed: bool,
    /// Excluded vulnerability classes (class-agnostic scope rule).
    #[serde(default)]
    pub excluded_vuln_classes: Vec<String>,
    /// Max requests per second during active testing.
    pub max_rate_rps: u32,
    /// Max concurrent network operations.
    pub max_concurrent: u32,
}

impl ScopePolicy {
    /// Build a policy from raw rule lists. `fail_closed` is normalized here so
    /// callers cannot create a policy that denies everything by enabling
    /// enforcement but forgetting to declare any allowlist.
    pub fn from_lists(
        in_scope: &[String],
        out_of_scope: &[String],
        enforce: bool,
        excluded_vuln_classes: &[String],
        max_rate_rps: u32,
        max_concurrent: u32,
    ) -> ScopePolicy {
        let in_scope: Vec<ScopePattern> = in_scope
            .iter()
            .filter_map(|s| ScopePattern::parse(s))
            .collect();
        let out_of_scope: Vec<ScopePattern> = out_of_scope
            .iter()
            .filter_map(|s| ScopePattern::parse(s))
            .collect();
        let fail_closed = enforce && !in_scope.is_empty();
        ScopePolicy {
            in_scope,
            out_of_scope,
            fail_closed,
            excluded_vuln_classes: excluded_vuln_classes.to_vec(),
            max_rate_rps,
            max_concurrent,
        }
    }
}

/// Outcome of evaluating one target against the policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScopeDecision {
    /// The target matches an in-scope rule.
    Allowed { matched: String },
    /// The target is explicitly denied.
    Denied { reason: String },
    /// No rule matched and the policy is not fail-closed. Callers that need
    /// strictness treat Pending as Denied when `fail_closed` is true, but the
    /// decision function itself never fabricates a reason.
    Pending,
}

/// Reason [`decide`] records when a fail-closed policy matched no rule at all.
///
/// This is deliberately distinct from an explicit out-of-scope rule. The
/// engagement never named the target either way, but the situations need
/// different words and different fixes: an excluded host is told to stop, while
/// an unmatched one is told the engagement simply does not cover it yet. Callers
/// that need the distinction should use [`ScopeDecision::is_unmatched`] rather
/// than matching on this text.
pub const UNMATCHED_FAIL_CLOSED: &str = "no in-scope rule matched this target";

impl ScopeDecision {
    /// Whether this denial came from a fail-closed policy matching no rule,
    /// rather than from an explicit out-of-scope rule.
    pub fn is_unmatched(&self) -> bool {
        matches!(self, ScopeDecision::Denied { reason } if reason == UNMATCHED_FAIL_CLOSED)
    }
}

/// Extract a canonical hostname from a URL, bare host, `host:port`, or
/// bracketed IPv6 literal. Returns `None` for empty/ambiguous input.
pub fn normalize_host(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }
    let after_scheme = match trimmed.find("://") {
        Some(idx) => &trimmed[idx + 3..],
        None => trimmed,
    };
    let authority = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    let host = authority.rsplit('@').next().unwrap_or(authority);
    let host = match host.rfind(':') {
        // Strip a trailing port only when the remainder is one numeric run
        // (a bare IPv6 like `::1` has colons that are not a port).
        Some(idx) if host[idx + 1..].chars().all(|c| c.is_ascii_digit()) => &host[..idx],
        _ => host,
    };
    let host = host.trim();
    // Unbracket IPv6; then lowercase, trimming a trailing root dot.
    let host = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_ascii_lowercase();
    let host = host.strip_suffix('.').unwrap_or(&host).to_string();
    if host.is_empty() { None } else { Some(host) }
}

/// Best-effort IP literal parse of a raw target. Prefers a direct parse (which
/// preserves bare IPv6 such as `::1`), then falls back to the normalized host
/// so `http://10.0.0.1/x` and `10.0.0.1:8443` both resolve to the address.
fn target_ip(target: &str) -> Option<IpAddr> {
    let trimmed = target.trim();
    if let Some(candidate) = trimmed.strip_prefix('[').and_then(|s| s.strip_suffix(']'))
        && let Ok(ip) = candidate.parse::<IpAddr>()
    {
        return Some(ip);
    }
    if let Ok(ip) = trimmed.parse::<IpAddr>() {
        return Some(ip);
    }
    normalize_host(trimmed)?.parse::<IpAddr>().ok()
}

/// Evaluate `target` (a URL, host, or IP) against the policy.
pub fn decide(policy: &ScopePolicy, target: &str) -> ScopeDecision {
    let host = normalize_host(target);
    let ip = target_ip(target);

    // Explicit exclusions always win, so a redirect into an excluded host can
    // never be converted into an "allowed" by an overlapping in-scope rule.
    for pattern in &policy.out_of_scope {
        if pattern_matches(pattern, host.as_deref(), ip.as_ref(), target) {
            return ScopeDecision::Denied {
                reason: format!("out-of-scope rule `{}`", pattern.describe()),
            };
        }
    }
    for pattern in &policy.in_scope {
        if pattern_matches(pattern, host.as_deref(), ip.as_ref(), target) {
            return ScopeDecision::Allowed {
                matched: pattern.describe(),
            };
        }
    }
    if policy.fail_closed {
        ScopeDecision::Denied {
            reason: UNMATCHED_FAIL_CLOSED.to_string(),
        }
    } else {
        ScopeDecision::Pending
    }
}

fn pattern_matches(
    pattern: &ScopePattern,
    host: Option<&str>,
    ip: Option<&IpAddr>,
    raw: &str,
) -> bool {
    match pattern {
        ScopePattern::Host(expected) => host == Some(expected.as_str()),
        ScopePattern::Domain(expected) => host
            .map(|h| h == expected || h.ends_with(&format!(".{expected}")))
            .unwrap_or(false),
        ScopePattern::SubdomainsOf(expected) => host
            .map(|h| h.ends_with(&format!(".{expected}")))
            .unwrap_or(false),
        ScopePattern::Ip(expected) => ip == Some(expected),
        ScopePattern::Ipv4Cidr { network, prefix } => {
            if *prefix == 0 {
                return matches!(ip, Some(IpAddr::V4(_)));
            }
            matches!(
                ip,
                Some(IpAddr::V4(v4)) if (u32::from(*v4) >> (32 - *prefix)) == (*network >> (32 - *prefix))
            )
        }
        ScopePattern::UrlPrefix(prefix) => raw.starts_with(prefix.as_str()),
    }
}

pub fn parse_ipv4_cidr(input: &str) -> Option<ScopePattern> {
    let (ip, prefix_text) = input.split_once('/')?;
    let prefix: u8 = prefix_text.trim().parse().ok()?;
    if prefix > 32 {
        return None;
    }
    let addr: Ipv4Addr = ip.trim().parse().ok()?;
    // For /0 the network value is irrelevant (every address matches); the
    // mask is not used by matching, but the shift here must not overflow.
    let network = if prefix == 0 {
        u32::from(addr)
    } else {
        u32::from(addr) & (u32::MAX << (32 - prefix))
    };
    Some(ScopePattern::Ipv4Cidr { network, prefix })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(in_scope: &[&str], out_of_scope: &[&str], enforce: bool) -> ScopePolicy {
        ScopePolicy::from_lists(
            &in_scope.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            &out_of_scope
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            enforce,
            &[],
            10,
            5,
        )
    }

    #[test]
    fn normalize_host_handles_urls_bare_hosts_and_ports() {
        assert_eq!(
            normalize_host("https://API.Example.com:8443/v1/x").as_deref(),
            Some("api.example.com")
        );
        assert_eq!(
            normalize_host("api.example.com").as_deref(),
            Some("api.example.com")
        );
        assert_eq!(
            normalize_host("example.com.").as_deref(),
            Some("example.com")
        );
        assert_eq!(
            normalize_host("user@example.com/v1").as_deref(),
            Some("example.com")
        );
        assert_eq!(normalize_host("  "), None);
        assert_eq!(normalize_host(""), None);
    }

    #[test]
    fn pattern_parse_covers_all_rule_kinds() {
        assert_eq!(
            ScopePattern::parse("203.0.113.5/24"),
            Some(ScopePattern::Ipv4Cidr {
                network: 0xcb007100,
                prefix: 24
            })
        );
        assert_eq!(
            ScopePattern::parse("203.0.113.5"),
            Some(ScopePattern::Ip(IpAddr::V4(Ipv4Addr::new(203, 0, 113, 5))))
        );
        assert_eq!(
            ScopePattern::parse("*.example.com"),
            Some(ScopePattern::SubdomainsOf("example.com".into()))
        );
        assert_eq!(
            ScopePattern::parse("https://example.com/api"),
            Some(ScopePattern::UrlPrefix("https://example.com/api".into()))
        );
        assert_eq!(
            ScopePattern::parse("api.example.com"),
            Some(ScopePattern::Domain("api.example.com".into()))
        );
        assert_eq!(ScopePattern::parse(""), None);
    }

    #[test]
    fn domain_rule_matches_apex_and_subdomains() {
        let p = policy(&["example.com"], &[], true);
        assert_eq!(
            decide(&p, "https://example.com/x"),
            ScopeDecision::Allowed {
                matched: "domain:example.com".into()
            }
        );
        assert!(matches!(
            decide(&p, "api.example.com"),
            ScopeDecision::Allowed { .. }
        ));
    }

    #[test]
    fn wildcard_rule_does_not_match_apex() {
        let p = policy(&["*.example.com"], &[], true);
        assert!(matches!(
            decide(&p, "api.example.com"),
            ScopeDecision::Allowed { .. }
        ));
        assert!(matches!(
            decide(&p, "example.com"),
            ScopeDecision::Denied { .. }
        ));
    }

    #[test]
    fn out_of_scope_overrides_in_scope() {
        let p = policy(&["example.com"], &["admin.example.com"], true);
        assert!(matches!(
            decide(&p, "admin.example.com"),
            ScopeDecision::Denied { .. }
        ));
        assert!(matches!(
            decide(&p, "example.com"),
            ScopeDecision::Allowed { .. }
        ));
    }

    #[test]
    fn fail_closed_blocks_unmatched_targets() {
        let p = policy(&["example.com"], &[], true);
        assert!(matches!(
            decide(&p, "evil.com"),
            ScopeDecision::Denied { .. }
        ));
    }

    #[test]
    fn non_enforcing_policy_returns_pending_for_unmatched() {
        let p = policy(&["example.com"], &[], false);
        assert_eq!(decide(&p, "evil.com"), ScopeDecision::Pending);
        // Out-of-scope still enforced even when not fail-closed.
        let p2 = policy(&[], &["forbidden.com"], false);
        assert!(matches!(
            decide(&p2, "forbidden.com"),
            ScopeDecision::Denied { .. }
        ));
    }

    #[test]
    fn no_allowlist_never_fails_closed() {
        // enroll=true with zero in-scope rules must NOT deny everything.
        let p = policy(&[], &[], true);
        assert_eq!(decide(&p, "anything.test"), ScopeDecision::Pending);
    }

    #[test]
    fn ip_and_cidr_rules_match() {
        let p = policy(&["203.0.113.0/24"], &[], true);
        assert!(matches!(
            decide(&p, "203.0.113.9"),
            ScopeDecision::Allowed { .. }
        ));
        assert!(matches!(
            decide(&p, "203.0.114.9"),
            ScopeDecision::Denied { .. }
        ));

        let p = policy(&["10.0.0.1"], &[], true);
        assert!(matches!(
            decide(&p, "http://10.0.0.1/x"),
            ScopeDecision::Allowed { .. }
        ));
        assert!(matches!(
            decide(&p, "10.0.0.1:8443"),
            ScopeDecision::Allowed { .. }
        ));
        assert!(matches!(
            decide(&p, "http://[2001:db8::1]/x"),
            ScopeDecision::Denied { .. }
        ));
    }

    #[test]
    fn url_prefix_restricts_path() {
        let p = policy(&["https://example.com/api"], &[], true);
        assert!(matches!(
            decide(&p, "https://example.com/api/v1/users"),
            ScopeDecision::Allowed { .. }
        ));
        assert!(matches!(
            decide(&p, "https://example.com/admin"),
            ScopeDecision::Denied { .. }
        ));
    }

    #[test]
    fn ipv4_cidr_prefix_zero_matches_all_v4() {
        let p = policy(&["0.0.0.0/0"], &[], true);
        assert!(matches!(
            decide(&p, "203.0.113.5"),
            ScopeDecision::Allowed { .. }
        ));
        assert!(matches!(
            decide(&p, "8.8.8.8"),
            ScopeDecision::Allowed { .. }
        ));
    }

    #[test]
    fn ipv4_cidr_high_prefix_does_not_overflow_or_widen() {
        // /32 is one specific address; /31 covers exactly two adjacent /24s.
        let p = policy(&["192.0.2.7/32"], &[], true);
        assert!(matches!(
            decide(&p, "192.0.2.7"),
            ScopeDecision::Allowed { .. }
        ));
        assert!(matches!(
            decide(&p, "192.0.2.8"),
            ScopeDecision::Denied { .. }
        ));

        let p = policy(&["192.0.2.8/31"], &[], true);
        assert!(matches!(
            decide(&p, "192.0.2.8"),
            ScopeDecision::Allowed { .. }
        ));
        assert!(matches!(
            decide(&p, "192.0.2.9"),
            ScopeDecision::Allowed { .. }
        ));
        assert!(matches!(
            decide(&p, "192.0.2.10"),
            ScopeDecision::Denied { .. }
        ));
    }

    #[test]
    fn ip_rule_matches_ipv6_url() {
        let p = policy(&["2001:db8::1"], &[], true);
        assert!(matches!(
            decide(&p, "http://[2001:db8::1]/x"),
            ScopeDecision::Allowed { .. }
        ));
    }
}
