//! Defense-bypass intelligence: a curated catalog of modern, in-scope-only
//! evasion tradecraft, plus fingerprint-driven ranking against a live target.
//!
//! Conceptually this module sits between recon and attacking: once the scope
//! layer fingerprints a defense stack (WAF/CDN/auth gateway), these functions
//! tell the operator *which* bypass families have historically been viable
//! against precisely that stack, ranked by how well the observed fingerprints
//! match. It is deliberately deterministic and model-free: the LLM receives
//! the ranked techniques as selectable skills/actions and still reasons about
//! them; this module only prevents the catalog being forgotten or
//! mis-prioritized mid-engagement.
//!
//! Everything here describes techniques against targets the engagement rules
//! have already admitted through `scope_gate`. Gating before ranking, never
//! after, is what keeps this catalog an operator aid rather than a scanner.

use serde::{Deserialize, Serialize};

use super::scope::LiveScope;

/// Bypass family groupings, used for reporting and dedup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BypassFamily {
    /// Trusted forwarding headers where the front door believes the override.
    HeaderSpoofing,
    /// Front/back proxy desync (CL.TE, TE.CL, TE.TE, HTTP/2 downgrade).
    RequestSmuggling,
    /// Unkeyed-header pollution and cache-deception path suffixes.
    CacheAbuse,
    /// Encoding/parser-differential payload mutation.
    EncodingObfuscation,
    /// GraphQL batching/aliasing and field-level gating leniency.
    GraphQlAbuse,
    /// JWT algorithm/claim confusion (alg-none, HS/RS swap, jku/kid).
    JwtConfusion,
    /// SSRF relayed through redirect chains and DNS rebinding.
    SsrfRelay,
    /// Method/action override headers that slip past route-level ACLs.
    MethodOverride,
}

impl BypassFamily {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HeaderSpoofing => "header_spoofing",
            Self::RequestSmuggling => "request_smuggling",
            Self::CacheAbuse => "cache_abuse",
            Self::EncodingObfuscation => "encoding_obfuscation",
            Self::GraphQlAbuse => "graphql_abuse",
            Self::JwtConfusion => "jwt_confusion",
            Self::SsrfRelay => "ssrf_relay",
            Self::MethodOverride => "method_override",
        }
    }
}

/// One defense-bypass technique from the curated catalog.
///
/// `Serialize` only: the catalog is static (`&'static` field data, which
/// serde cannot synthesize `Deserialize` for), and mutable deserialization
/// of curated tradecraft would be a fidelity bug anyway.
#[derive(Clone, Debug, Serialize)]
pub struct BypassTechnique {
    pub id: &'static str,
    pub family: BypassFamily,
    pub name: &'static str,
    /// Why it works and when it applies, in operator language.
    pub description: &'static str,
    /// Lowercase substrings that, appearing in a technology fingerprint,
    /// indicate this technique is likely applicable to that component.
    pub triggers: &'static [&'static str],
    /// Concrete first-probe shapes the operator should send first.
    pub probe_hints: &'static [&'static str],
    pub estimated_cost: f32,
    pub estimated_risk: f32,
    pub expected_information_gain: f32,
}

impl BypassTechnique {
    /// Does this technique's trigger list match a technology fingerprint?
    pub fn matches_technology(&self, technology_name: &str) -> bool {
        let lower = technology_name.to_lowercase();
        self.triggers.iter().any(|t| lower.contains(t))
    }
}

/// The catalog. Ordering matters only as documentation; ranking is done by
/// [`for_technology`] / [`rank_for_scope`], which sort deterministically.
pub const CATALOG: &[BypassTechnique] = &[
    BypassTechnique {
        id: "spoof_url_rewrite",
        family: BypassFamily::HeaderSpoofing,
        name: "Trusted URL-rewrite header spoofing",
        description: "Front proxies and WAFs that honor X-Original-URL / X-Rewrite-URL apply \
their ACL rules against the spoofed path while the backend routes by the real \
one. Send the protected path in the spoofed header and an adjacent allowed \
path in the URI, and read-only checks pass against the wrong resource.",
        triggers: &[
            "cloudflare",
            "akamai",
            "imperva",
            "modsecurity",
            "f5",
            "big-ip",
            "aws waf",
            "waf",
            "nginx",
            "haproxy",
            "envoy",
            "varnish",
            "squid",
        ],
        probe_hints: &[
            "GET /allowed HTTP/1.1 with X-Original-URL: /admin/secret",
            "GET /admin HTTP/1.1 with X-Rewrite-URL: /public/health",
        ],
        estimated_cost: 0.25,
        estimated_risk: 0.08,
        expected_information_gain: 0.85,
    },
    BypassTechnique {
        id: "spoof_ip_trust",
        family: BypassFamily::HeaderSpoofing,
        name: "Trusted client-IP header spoofing",
        description: "Rate limiters and IP allowlists that trust a forwarding header the \
client can inject forget that each hop applies different semantics to \
CF-Connecting-IP / True-Client-IP / X-Real-IP / X-Forwarded-For. Probe each \
header in turn with a constant value and rotate until the limiter's counter \
stops decreasing.",
        triggers: &[
            "cloudflare",
            "akamai",
            "fastly",
            "cloudfront",
            "imperva",
            "api-gateway",
            "gateway",
            "koa",
            "express",
            "nginx",
            "traefik",
        ],
        probe_hints: &[
            "Rotate candidate spoof headers one per request, keep value constant",
            "Compare X-RateLimit-* counters between rotated and baseline requests",
        ],
        estimated_cost: 0.2,
        estimated_risk: 0.05,
        expected_information_gain: 0.8,
    },
    BypassTechnique {
        id: "method_override",
        family: BypassFamily::MethodOverride,
        name: "HTTP method override past route ACLs",
        description: "Frameworks that honor X-HTTP-Method-Override / X-HTTP-Method / \
X-Method-Override let the WAF see GET while the application performs \
POST/PUT/DELETE, slipping write operations past read-only ACLs and past CSRF \
or authorization checks keyed on the public method.",
        triggers: &[
            "express", "koa", "django", "rails", "laravel", "symfony", "spring", "dotnet", "asp",
            "fastapi", "flask",
        ],
        probe_hints: &[
            "GET /user/42 with X-HTTP-Method-Override: DELETE",
            "POST /article with X-HTTP-Method: PUT and the PUT body",
        ],
        estimated_cost: 0.2,
        estimated_risk: 0.1,
        expected_information_gain: 0.75,
    },
    BypassTechnique {
        id: "clte_smuggle",
        family: BypassFamily::RequestSmuggling,
        name: "CL.TE request smuggling",
        description: "The front proxy trusts Content-Length while the backend trusts \
chunked Transfer-Encoding; a smuggled request prefix then poisons a \
subsequent victim's request. Confirm desync by observing whether a second \
probe on a fresh connection receives the smuggled suffix appended.",
        triggers: &[
            "nginx",
            "apache",
            "haproxy",
            "varnish",
            "squid",
            "envoy",
            "akamai",
            "cloudflare",
            "fastly",
            "trafficserver",
            "iis",
        ],
        probe_hints: &[
            "CL.TE: Content-Length: 13 + Transfer-Encoding: chunked with smuggled GET prefix",
            "Timing-first probe: truncate the body so the backend waits, measure the TTFB delta",
        ],
        estimated_cost: 0.55,
        estimated_risk: 0.3,
        expected_information_gain: 0.9,
    },
    BypassTechnique {
        id: "tete_obfuscation",
        family: BypassFamily::RequestSmuggling,
        name: "TE.TE desync via malformed TE headers",
        description: "Both sides claim to obey Transfer-Encoding, but only one parses the \
obfuscated variant (Transfer-Encoding: xchunked, chunked, duplicate headers, \
trailing whitespace). Disabling exactly one parser re-creates the CL/TE split \
and restores the desync.",
        triggers: &[
            "nginx",
            "apache",
            "varnish",
            "squid",
            "haproxy",
            "envoy",
            "cloudflare",
            "fastly",
            "akamai",
            "iis",
            "tomcat",
        ],
        probe_hints: &[
            "Transfer-encoding: xchunked so one side keeps CL and the other keeps TE",
            "Duplicate Transfer-Encoding headers where only the second copy is valid chunked",
        ],
        estimated_cost: 0.5,
        estimated_risk: 0.25,
        expected_information_gain: 0.85,
    },
    BypassTechnique {
        id: "h2_downgrade_desync",
        family: BypassFamily::RequestSmuggling,
        name: "HTTP/2 downgrade smuggling",
        description: "An H2 front end in front of an HTTP/1.1 backend: forbidden \
pseudos and attacker-controlled header names/values survive the downgrade and \
end up in raw backend requests. Probe with a transfer-encoding pseudo-header \
and with :path values that smuggle traversal, then observe the response queue.",
        triggers: &[
            "http2",
            "h2",
            "http3",
            "cloudfront",
            "nginx",
            "envoy",
            "haproxy",
            "cloudflare",
            "fastly",
            "apache",
        ],
        probe_hints: &[
            "H2 request carrying transfer-encoding: chunked as a forbidden pseudoheader",
            "Header-value smuggling through :path, then observe whether a victim's response shifts",
        ],
        estimated_cost: 0.6,
        estimated_risk: 0.3,
        expected_information_gain: 0.9,
    },
    BypassTechnique {
        id: "cache_deception",
        family: BypassFamily::CacheAbuse,
        name: "Web cache deception",
        description: "CDNs that bound the cache key to the static extension can be talked \
into caching an authenticated dynamic resource: request /account/info.css \
under a relaxed cache rule and it is keyed as a static asset. The attacker \
then fetches the same URL unauthenticated and receives the victim's private \
response body.",
        triggers: &[
            "cloudflare",
            "cloudfront",
            "fastly",
            "akamai",
            "cdn",
            "varnish",
            "nginx",
            "apache",
            "page-rule",
        ],
        probe_hints: &[
            "Request /account/info with an appended .css/.js/.ico under the victim session",
            "Re-fetch the same URL unauthenticated and diff the response bodies",
        ],
        estimated_cost: 0.3,
        estimated_risk: 0.08,
        expected_information_gain: 0.8,
    },
    BypassTechnique {
        id: "cache_poisoning_unkeyed",
        family: BypassFamily::CacheAbuse,
        name: "Unkeyed header cache poisoning",
        description: "The cache key omits a header the application reflects: discover \
which headers (X-Forwarded-Host, X-Forwarded-Proto, X-Original-URL, \
X-Forwarded-Scheme) make it into an absolute-URL context, poison one cached \
entry, and reproduce on every unrelated victim request that hits it.",
        triggers: &[
            "cloudflare",
            "cloudfront",
            "fastly",
            "akamai",
            "cdn",
            "varnish",
            "nginx",
            "apache",
            "edge",
            "wordpress",
        ],
        probe_hints: &[
            "Fuzz X-Forwarded-Host/Scheme and X-HTTP-Host-Override against reflected canonical URLs",
            "Confirm persistence by re-requesting through a distinct cache-busting query parameter",
        ],
        estimated_cost: 0.4,
        estimated_risk: 0.15,
        expected_information_gain: 0.85,
    },
    BypassTechnique {
        id: "jwt_alg_confusion",
        family: BypassFamily::JwtConfusion,
        name: "JWT algorithm confusion & claim injection",
        description: "Sign HS256 when the verifier expects RS256 (using the leaked public \
key), accept alg:none, relocate trust via jku/x5u, exploit kid path traversal \
or injection, or duplicate claims so a strict-but-blind library parses the \
wrong copy. Whichever key the verifier truly loads, own the claim parse order.",
        triggers: &[
            "jwt",
            "oauth",
            "oidc",
            "auth0",
            "okta",
            "keycloak",
            "cognito",
            "identityserver",
            "fusionauth",
            "jose",
        ],
        probe_hints: &[
            "Craft an alg:none token with a forged admin claim and observe acceptance",
            "Send overlapping duplicate exp/aud claims to force JSON duplicate-key divergence",
        ],
        estimated_cost: 0.35,
        estimated_risk: 0.1,
        expected_information_gain: 0.85,
    },
    BypassTechnique {
        id: "graphql_alias_batch",
        family: BypassFamily::GraphQlAbuse,
        name: "GraphQL alias/batch enumeration bypass",
        description: "Rate limiting that counts queries rather than resolved fields \
falls to N aliased copies of an introspection or object field packed into a \
single query; field-level authorization gaps similarly bow to aliased access \
once the exporter only gates the canonical field name.",
        triggers: &[
            "graphql",
            "hasura",
            "apollo",
            "graphene",
            "strawberry",
            "relay",
        ],
        probe_hints: &[
            "query { a: user(id: 1) b: user(id: 2) ... z: user(id: 26) }",
            "If aliases are capped, try JSON-array batched queries and GET-style [] responses",
        ],
        estimated_cost: 0.25,
        estimated_risk: 0.08,
        expected_information_gain: 0.8,
    },
    BypassTechnique {
        id: "ssrf_redirect_relay",
        family: BypassFamily::SsrfRelay,
        name: "SSRF via redirect chain relay",
        description: "URL validators check the first hop only: daisy-chain open \
redirects, or relay through a trusted endpoint that reflects a URL parameter, \
so every hop the validator sees is safe while the final hop the fetcher \
follows targets the internal or DNS-rebound address.",
        triggers: &[
            "lambda",
            "webhook",
            "serverless",
            "api",
            "gateway",
            "scraper",
            "preview",
            "meta",
            "bandwidth",
            "image",
        ],
        probe_hints: &[
            "Chain the url= parameter through known open redirects on the same origin",
            "DNS rebinding: TTL-0 A records alternating public and private addresses",
        ],
        estimated_cost: 0.5,
        estimated_risk: 0.15,
        expected_information_gain: 0.85,
    },
    BypassTechnique {
        id: "origin_parser_confusion",
        family: BypassFamily::EncodingObfuscation,
        name: "Front/back parser differential obfuscation",
        description: "The WAF and the application disagree on one grammar slot at a time: \
double URL-encoding, mixed-case Unicode folding, percent-encoded dot segments, \
or NUL truncation each make the two parsers disagree about which rule fired, \
so a payload lands only inside the application. Normalize one slot per probe \
and pair each result with the benign baseline.",
        triggers: &[
            "cloudflare",
            "akamai",
            "imperva",
            "aws waf",
            "modsecurity",
            "iis",
            "nginx",
            "apache",
            "tomcat",
            "spring",
            "dotnet",
            "php",
        ],
        probe_hints: &[
            "Iterate: %252e%252e%252f, overlong UTF-8 dots, %-encoded NUL truncation",
            "Keep a benign baseline request paired with every mutated probe for the diff",
        ],
        estimated_cost: 0.3,
        estimated_risk: 0.1,
        expected_information_gain: 0.75,
    },
];

/// Look a technique up by catalog id.
pub fn find(id: &str) -> Option<&'static BypassTechnique> {
    CATALOG.iter().find(|t| t.id == id)
}

/// Techniques whose triggers match a single technology fingerprint, sorted
/// by expected value (info gain − cost − risk), then id for determinism.
pub fn for_technology(technology_name: &str) -> Vec<&'static BypassTechnique> {
    let mut matched: Vec<&'static BypassTechnique> = CATALOG
        .iter()
        .filter(|t| t.matches_technology(technology_name))
        .collect();
    matched.sort_by(|a, b| {
        let va = a.expected_information_gain - a.estimated_cost - a.estimated_risk;
        let vb = b.expected_information_gain - b.estimated_cost - b.estimated_risk;
        vb.partial_cmp(&va)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(b.id))
    });
    matched
}

/// Rank the catalog against a live scope.
///
/// Scores combine: technology-fingerprint matches (from `all_technologies()`,
/// which covers per-host and per-subdomain fingerprints), the observed defense
/// headers on discovered endpoints (a strong direct signal), and endpoint
/// properties (auth, GraphQL, URL-parameter-bearing request bodies) that make
/// a given bypass family profitable rather than merely possible.
pub fn rank_for_scope(scope: &LiveScope) -> Vec<&'static BypassTechnique> {
    let mut scores: Vec<(&'static BypassTechnique, f32)> =
        CATALOG.iter().map(|t| (t, 0.0_f32)).collect();

    let bump = |scores: &mut Vec<(&'static BypassTechnique, f32)>, id: &str, add: f32| {
        for (t, s) in scores.iter_mut() {
            if t.id == id {
                *s += add;
            }
        }
    };

    for tech in scope.all_technologies() {
        for (t, s) in scores.iter_mut() {
            if t.matches_technology(&tech.name) {
                *s += 1.0;
            }
        }
    }

    for endpoint in &scope.discovered_endpoints {
        if !endpoint.waf_headers.is_empty() || !endpoint.cdn_headers.is_empty() {
            bump(&mut scores, "spoof_url_rewrite", 1.0);
            bump(&mut scores, "spoof_ip_trust", 0.8);
            bump(&mut scores, "origin_parser_confusion", 0.8);
        }
        if !endpoint.cache_headers.is_empty() {
            bump(&mut scores, "cache_deception", 1.0);
            bump(&mut scores, "cache_poisoning_unkeyed", 1.0);
        } else if !endpoint.response_headers.is_empty() {
            bump(&mut scores, "cache_deception", 0.5);
        }
        if !endpoint.rate_limit_headers.is_empty() {
            bump(&mut scores, "spoof_ip_trust", 1.0);
            bump(&mut scores, "graphql_alias_batch", 0.6);
        }
        if endpoint.url.contains("/graphql") || endpoint.path.contains("/graphql") {
            bump(&mut scores, "graphql_alias_batch", 1.0);
        }
        if endpoint.requires_auth {
            bump(&mut scores, "cache_deception", 0.4);
            bump(&mut scores, "jwt_alg_confusion", 0.4);
            bump(&mut scores, "cache_poisoning_unkeyed", 0.3);
        }
        // Anything that parses or fetches a client-supplied URL is relay-shaped.
        if let Some(body) = endpoint.request_body.as_deref() {
            let lower = body.to_lowercase();
            if lower.contains("http://") || lower.contains("https://") || lower.contains("url=") {
                bump(&mut scores, "ssrf_redirect_relay", 1.0);
            }
        }
    }

    scores.sort_by(|(a, sa), (b, sb)| {
        sb.partial_cmp(sa)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.id.cmp(b.id))
    });

    scores
        .into_iter()
        .filter(|(_, s)| *s > 0.0)
        .map(|(t, _)| t)
        .collect()
}

/// Builtin action definitions mirroring the catalog, consumed by
/// `ActionSpace::with_builtin_security_actions` so the adaptive cycle can
/// select bypass probes like any other recon/attack action. The ids are the
/// catalog ids; the `bypass_` prefix lives in the caller that registers them.
pub fn action_definitions() -> Vec<(&'static str, &'static str, f32, f32, f32)> {
    CATALOG
        .iter()
        .map(|t| {
            (
                t.id,
                t.description,
                t.estimated_cost,
                t.estimated_risk,
                t.expected_information_gain,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_security_core::scope::{
        DiscoveredEndpoint, EngagementMode, LiveScope, TechCategory, TechnologyFingerprint,
    };

    fn fingerprint(name: &str, category: TechCategory) -> TechnologyFingerprint {
        TechnologyFingerprint {
            name: name.to_string(),
            version: None,
            category,
            confidence: 0.9,
            evidence: "test".into(),
        }
    }

    #[test]
    fn waf_fingerprints_rank_spoofing_and_smuggling() {
        let mut scope = LiveScope::default();
        scope.mode = EngagementMode::BugBounty;
        scope
            .technologies
            .push(fingerprint("Cloudflare", TechCategory::Cdn));
        scope
            .technologies
            .push(fingerprint("ModSecurity", TechCategory::Waf));

        let ranked = rank_for_scope(&scope);
        assert!(!ranked.is_empty());
        let top_ids: Vec<&str> = ranked.iter().take(2).map(|t| t.id).collect();
        assert!(
            top_ids.contains(&"spoof_url_rewrite") || top_ids.contains(&"origin_parser_confusion"),
            "expected spoofing/parser techniques at the top, got {top_ids:?}"
        );
        // A WAF/CDN stack always implies smuggling applies behind it.
        assert!(
            ranked
                .iter()
                .any(|t| t.family == BypassFamily::RequestSmuggling),
            "smuggling family missing for proxy-stack fingerprint"
        );
    }

    #[test]
    fn cache_headers_bump_cache_family() {
        let mut scope = LiveScope::default();
        scope
            .technologies
            .push(fingerprint("unknown-origin", TechCategory::Os));
        let mut ep = DiscoveredEndpoint::default();
        ep.url = "https://t.example/account".into();
        ep.path = "/account".into();
        ep.cache_headers = vec![("X-Cache".into(), "MISS".into())];
        scope.discovered_endpoints.push(ep);

        let ranked = rank_for_scope(&scope);
        assert!(
            ranked.iter().all(|t| t.family == BypassFamily::CacheAbuse),
            "cache signals should select only cache-abuse techniques, got {:?}",
            ranked.iter().map(|t| t.id).collect::<Vec<_>>()
        );
    }

    #[test]
    fn ranking_is_deterministic() {
        let mut scope = LiveScope::default();
        scope
            .technologies
            .push(fingerprint("nginx", TechCategory::Server));
        let a = rank_for_scope(&scope)
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>();
        let b = rank_for_scope(&scope)
            .iter()
            .map(|t| t.id)
            .collect::<Vec<_>>();
        assert_eq!(a, b);
    }

    #[test]
    fn catalog_ids_are_unique_and_findable() {
        let ids: std::collections::HashSet<&str> = CATALOG.iter().map(|t| t.id).collect();
        assert_eq!(ids.len(), CATALOG.len());
        assert!(CATALOG.len() >= 10);
        assert!(find("spoof_url_rewrite").is_some());
        assert!(find("no-such-technique").is_none());
    }

    #[test]
    fn for_technology_matches_case_insensitively() {
        assert!(
            for_technology("CLOUDFRONT-CDN")
                .iter()
                .any(|t| t.id == "cache_deception")
        );
        assert!(
            for_technology("express")
                .iter()
                .any(|t| t.id == "method_override")
        );
        assert!(for_technology("Sqlite").is_empty());
    }

    #[test]
    fn action_definitions_mirror_catalog() {
        let defs = action_definitions();
        assert_eq!(defs.len(), CATALOG.len());
        for (id, desc, cost, risk, gain) in defs {
            assert!(!desc.is_empty());
            assert!((0.0..=1.0).contains(&cost));
            assert!((0.0..=1.0).contains(&risk));
            assert!((0.0..=1.0).contains(&gain));
            assert!(find(id).is_some(), "{id} must round-trip to the catalog");
        }
    }
}
