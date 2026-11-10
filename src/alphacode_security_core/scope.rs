use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Engagement mode determines the operating context.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngagementMode {
    #[default]
    BugBounty,
    Pentest,
    Ctf,
    SecurityLab,
    Research,
    Audit,
}

impl EngagementMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BugBounty => "bug_bounty",
            Self::Pentest => "pentest",
            Self::Ctf => "ctf",
            Self::SecurityLab => "security_lab",
            Self::Research => "research",
            Self::Audit => "audit",
        }
    }

    pub fn requires_scope_enforcement(&self) -> bool {
        matches!(
            self,
            Self::BugBounty | Self::Pentest | Self::Research | Self::Audit
        )
    }
}

/// A dynamically discovered technology fingerprint.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TechnologyFingerprint {
    pub name: String,
    pub version: Option<String>,
    pub category: TechCategory,
    pub confidence: f64,
    pub evidence: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TechCategory {
    Language,
    Framework,
    Server,
    Cms,
    Database,
    Cache,
    Waf,
    Cdn,
    Analytics,
    AuthProvider,
    Api,
    CdnOrProxy,
    Os,
    Unknown(String),
}

/// A dynamically discovered endpoint.
///
/// `Default` is derived so callers can build one with
/// `DiscoveredEndpoint { url, method, ..Default::default() }`. This struct has
/// ~130 optional capture fields; without it every new field breaks every
/// struct literal in the crate.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DiscoveredEndpoint {
    pub url: String,
    pub method: String,
    pub path: String,
    pub parameters: Vec<DiscoveredParameter>,
    pub status_code: Option<u16>,
    pub content_type: Option<String>,
    pub requires_auth: bool,
    pub discovered_by: String,
    pub noise_level: super::noise::NoiseLevel,
    // Enhanced endpoint fields
    pub response_time_ms: Option<u64>,
    pub response_size: Option<usize>,
    pub response_headers: Vec<(String, String)>,
    pub response_body: Option<String>,
    pub request_headers: Vec<(String, String)>,
    pub request_body: Option<String>,
    pub cookies: Vec<(String, String)>,
    pub session_tokens: Vec<String>,
    pub api_version: Option<String>,
    pub rate_limit_headers: Vec<(String, String)>,
    pub cors_headers: Vec<(String, String)>,
    pub security_headers: Vec<(String, String)>,
    pub waf_headers: Vec<(String, String)>,
    pub cdn_headers: Vec<(String, String)>,
    pub load_balancer_headers: Vec<(String, String)>,
    pub proxy_headers: Vec<(String, String)>,
    pub server_headers: Vec<(String, String)>,
    pub framework_headers: Vec<(String, String)>,
    pub language_headers: Vec<(String, String)>,
    pub database_headers: Vec<(String, String)>,
    pub cache_headers: Vec<(String, String)>,
    pub queue_headers: Vec<(String, String)>,
    pub message_broker_headers: Vec<(String, String)>,
    pub search_engine_headers: Vec<(String, String)>,
    pub analytics_headers: Vec<(String, String)>,
    pub tracking_headers: Vec<(String, String)>,
    pub advertising_headers: Vec<(String, String)>,
    pub social_media_headers: Vec<(String, String)>,
    pub payment_headers: Vec<(String, String)>,
    pub shipping_headers: Vec<(String, String)>,
    pub tax_headers: Vec<(String, String)>,
    pub fraud_headers: Vec<(String, String)>,
    pub risk_headers: Vec<(String, String)>,
    pub compliance_headers: Vec<(String, String)>,
    pub privacy_headers: Vec<(String, String)>,
    pub consent_headers: Vec<(String, String)>,
    pub cookie_consent_headers: Vec<(String, String)>,
    pub gdpr_headers: Vec<(String, String)>,
    pub ccpa_headers: Vec<(String, String)>,
    pub lgpd_headers: Vec<(String, String)>,
    pub pdpa_headers: Vec<(String, String)>,
    pub popia_headers: Vec<(String, String)>,
    pub dpa_headers: Vec<(String, String)>,
    #[serde(rename = "ePrivacy_headers")]
    pub e_privacy_headers: Vec<(String, String)>,
    pub nis2_headers: Vec<(String, String)>,
    pub dora_headers: Vec<(String, String)>,
    #[serde(rename = "pciDss_headers")]
    pub pci_dss_headers: Vec<(String, String)>,
    pub soc2_headers: Vec<(String, String)>,
    pub iso27001_headers: Vec<(String, String)>,
    pub iso27017_headers: Vec<(String, String)>,
    pub iso27018_headers: Vec<(String, String)>,
    pub iso27019_headers: Vec<(String, String)>,
    pub iso27020_headers: Vec<(String, String)>,
    pub iso27021_headers: Vec<(String, String)>,
    pub iso27022_headers: Vec<(String, String)>,
    pub iso27023_headers: Vec<(String, String)>,
    pub iso27024_headers: Vec<(String, String)>,
    pub iso27025_headers: Vec<(String, String)>,
    pub iso27026_headers: Vec<(String, String)>,
    pub iso27027_headers: Vec<(String, String)>,
    pub iso27028_headers: Vec<(String, String)>,
    pub iso27029_headers: Vec<(String, String)>,
    pub iso27030_headers: Vec<(String, String)>,
    pub iso27031_headers: Vec<(String, String)>,
    pub iso27032_headers: Vec<(String, String)>,
    pub iso27033_headers: Vec<(String, String)>,
    pub iso27034_headers: Vec<(String, String)>,
    pub iso27035_headers: Vec<(String, String)>,
    pub iso27036_headers: Vec<(String, String)>,
    pub iso27037_headers: Vec<(String, String)>,
    pub iso27038_headers: Vec<(String, String)>,
    pub iso27039_headers: Vec<(String, String)>,
    pub iso27040_headers: Vec<(String, String)>,
    pub iso27041_headers: Vec<(String, String)>,
    pub iso27042_headers: Vec<(String, String)>,
    pub iso27043_headers: Vec<(String, String)>,
    pub iso27044_headers: Vec<(String, String)>,
    pub iso27045_headers: Vec<(String, String)>,
    pub iso27046_headers: Vec<(String, String)>,
    pub iso27047_headers: Vec<(String, String)>,
    pub iso27048_headers: Vec<(String, String)>,
    pub iso27049_headers: Vec<(String, String)>,
    pub iso27050_headers: Vec<(String, String)>,
    pub iso27051_headers: Vec<(String, String)>,
    pub iso27052_headers: Vec<(String, String)>,
    pub iso27053_headers: Vec<(String, String)>,
    pub iso27054_headers: Vec<(String, String)>,
    pub iso27055_headers: Vec<(String, String)>,
    pub iso27056_headers: Vec<(String, String)>,
    pub iso27057_headers: Vec<(String, String)>,
    pub iso27058_headers: Vec<(String, String)>,
    pub iso27059_headers: Vec<(String, String)>,
    pub iso27060_headers: Vec<(String, String)>,
    pub iso27061_headers: Vec<(String, String)>,
    pub iso27062_headers: Vec<(String, String)>,
    pub iso27063_headers: Vec<(String, String)>,
    pub iso27064_headers: Vec<(String, String)>,
    pub iso27065_headers: Vec<(String, String)>,
    pub iso27066_headers: Vec<(String, String)>,
    pub iso27067_headers: Vec<(String, String)>,
    pub iso27068_headers: Vec<(String, String)>,
    pub iso27069_headers: Vec<(String, String)>,
    pub iso27070_headers: Vec<(String, String)>,
    pub iso27071_headers: Vec<(String, String)>,
    pub iso27072_headers: Vec<(String, String)>,
    pub iso27073_headers: Vec<(String, String)>,
    pub iso27074_headers: Vec<(String, String)>,
    pub iso27075_headers: Vec<(String, String)>,
    pub iso27076_headers: Vec<(String, String)>,
    pub iso27077_headers: Vec<(String, String)>,
    pub iso27078_headers: Vec<(String, String)>,
    pub iso27079_headers: Vec<(String, String)>,
    pub iso27080_headers: Vec<(String, String)>,
    pub iso27081_headers: Vec<(String, String)>,
    pub iso27082_headers: Vec<(String, String)>,
    pub iso27083_headers: Vec<(String, String)>,
    pub iso27084_headers: Vec<(String, String)>,
    pub iso27085_headers: Vec<(String, String)>,
    pub iso27086_headers: Vec<(String, String)>,
    pub iso27087_headers: Vec<(String, String)>,
    pub iso27088_headers: Vec<(String, String)>,
    pub iso27089_headers: Vec<(String, String)>,
    pub iso27090_headers: Vec<(String, String)>,
    pub iso27091_headers: Vec<(String, String)>,
    pub iso27092_headers: Vec<(String, String)>,
    pub iso27093_headers: Vec<(String, String)>,
    pub iso27094_headers: Vec<(String, String)>,
    pub iso27095_headers: Vec<(String, String)>,
    pub iso27096_headers: Vec<(String, String)>,
    pub iso27097_headers: Vec<(String, String)>,
    pub iso27098_headers: Vec<(String, String)>,
    pub iso27099_headers: Vec<(String, String)>,
    pub iso27100_headers: Vec<(String, String)>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DiscoveredParameter {
    pub name: String,
    pub location: ParameterLocation,
    pub value_type: Option<String>,
    pub sample_value: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ParameterLocation {
    Query,
    Body,
    Header,
    Cookie,
    Path,
    JsonBody,
    MultipartFormData,
}

/// A dynamically discovered host.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiscoveredHost {
    pub hostname: String,
    pub ip_addresses: Vec<String>,
    pub ports: Vec<DiscoveredPort>,
    pub technologies: Vec<TechnologyFingerprint>,
    pub services: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub struct DiscoveredPort {
    pub port: u16,
    pub protocol: String,
    pub service: String,
    pub version: Option<String>,
}

/// A dynamically discovered subdomain.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct DiscoveredSubdomain {
    pub subdomain: String,
    pub ip_addresses: Vec<String>,
    pub technologies: Vec<TechnologyFingerprint>,
    pub alive: bool,
}

/// Scope rules — only behavioral constraints, no target hardcoding.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EngagementRules {
    #[serde(default = "default_max_rate")]
    pub max_rate_rps: u32,
    #[serde(default = "default_true")]
    pub no_destructive_actions: bool,
    #[serde(default = "default_true")]
    pub require_user_approval: bool,
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: u32,
    #[serde(default)]
    pub excluded_vuln_classes: Vec<String>,
}

fn default_max_rate() -> u32 {
    10
}
fn default_true() -> bool {
    true
}
fn default_max_concurrent() -> u32 {
    5
}

impl Default for EngagementRules {
    fn default() -> Self {
        Self {
            max_rate_rps: default_max_rate(),
            no_destructive_actions: default_true(),
            require_user_approval: default_true(),
            max_concurrent: default_max_concurrent(),
            excluded_vuln_classes: Vec::new(),
        }
    }
}

/// Live scope — entirely dynamically populated from reconnaissance.
///
/// No URLs, IPs, or endpoints are hardcoded. Everything is discovered at runtime
/// from the target through active and passive reconnaissance.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct LiveScope {
    /// Engagement mode.
    #[serde(default)]
    pub mode: EngagementMode,
    /// Rules governing engagement behavior (rate limits, approvals).
    #[serde(default)]
    pub rules: EngagementRules,

    // --- All fields below are dynamically populated by recon agents ---
    /// Root target (e.g. "example.com") — the single seed from which
    /// everything else is discovered.
    pub target_root: Option<String>,

    /// Discovered subdomains (populated by subdomain enumeration).
    #[serde(default)]
    pub discovered_subdomains: Vec<DiscoveredSubdomain>,

    /// Discovered hosts (populated by port scanning, DNS resolution).
    #[serde(default)]
    pub discovered_hosts: Vec<DiscoveredHost>,

    /// Discovered endpoints (populated by crawling, fuzzing, JS analysis).
    #[serde(default)]
    pub discovered_endpoints: Vec<DiscoveredEndpoint>,

    /// Technology fingerprints found on the target (populated by tech detection).
    #[serde(default)]
    pub technologies: Vec<TechnologyFingerprint>,

    /// Out-of-scope targets discovered or reported at runtime.
    #[serde(default)]
    pub out_of_scope: HashSet<String>,

    /// Session notes and constraints provided by the user.
    #[serde(default)]
    pub notes: Vec<String>,

    /// Map of hostname → scope verdict, updated as recon progresses.
    #[serde(default)]
    pub scope_verdicts: HashMap<String, ScopeVerdict>,

    /// Observations pool: anything interesting found during recon
    /// that isn't yet an endpoint or host (e.g. leaked JS files,
    /// comments, error messages, API keys found in responses).
    #[serde(default)]
    pub observations: Vec<ScopeObservation>,
}

/// Verdict on whether a specific target is in scope.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScopeVerdict {
    InScope,
    OutOfScope { reason: String },
    Pending,
}

/// A raw observation from recon — not yet a finding, not yet classified.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScopeObservation {
    pub id: String,
    pub category: String,
    pub content: String,
    pub source: String,
    pub timestamp: String,
    pub tags: Vec<String>,
}

impl LiveScope {
    /// Create a new empty scope seeded with only a target root.
    pub fn new(mode: EngagementMode, target_root: String) -> Self {
        Self {
            mode,
            target_root: Some(target_root),
            ..Default::default()
        }
    }

    /// Add a discovered subdomain.
    pub fn add_subdomain(&mut self, sub: DiscoveredSubdomain) {
        if !self
            .discovered_subdomains
            .iter()
            .any(|s| s.subdomain == sub.subdomain)
        {
            self.discovered_subdomains.push(sub);
        }
    }

    /// Add a discovered host.
    pub fn add_host(&mut self, host: DiscoveredHost) {
        if !self
            .discovered_hosts
            .iter()
            .any(|h| h.hostname == host.hostname)
        {
            self.discovered_hosts.push(host);
        }
    }

    /// Add a discovered endpoint.
    pub fn add_endpoint(&mut self, ep: DiscoveredEndpoint) {
        if !self
            .discovered_endpoints
            .iter()
            .any(|e| e.url == ep.url && e.method == ep.method)
        {
            self.discovered_endpoints.push(ep);
        }
    }

    /// Add a technology fingerprint.
    pub fn add_technology(&mut self, tech: TechnologyFingerprint) {
        if !self
            .technologies
            .iter()
            .any(|t| t.name == tech.name && t.version == tech.version)
        {
            self.technologies.push(tech);
        }
    }

    /// Add an out-of-scope marker.
    pub fn add_out_of_scope(&mut self, target: String, reason: String) {
        self.notes
            .push(format!("Out of scope: {target} - {reason}"));
        self.scope_verdicts
            .insert(target.clone(), ScopeVerdict::OutOfScope { reason });
        self.out_of_scope.insert(target);
    }

    /// Record a raw observation.
    pub fn add_observation(&mut self, obs: ScopeObservation) {
        self.observations.push(obs);
    }

    /// Update scope verdict for a hostname.
    ///
    /// Re-marking a host back in scope clears the sticky `out_of_scope` set,
    /// otherwise a host that was provisionally excluded and later confirmed
    /// in-scope would stay blocked forever and the two structures would
    /// permanently disagree.
    pub fn set_verdict(&mut self, hostname: String, verdict: ScopeVerdict) {
        // DNS names are case-insensitive and a trailing root dot does not
        // change the host. Remove equivalent old entries before setting the
        // new verdict so a differently spelled update cannot leave a stale
        // exclusion behind.
        self.out_of_scope
            .retain(|host| !same_hostname(host, &hostname));
        self.scope_verdicts
            .retain(|host, _| !same_hostname(host, &hostname));
        if matches!(verdict, ScopeVerdict::OutOfScope { .. }) {
            self.out_of_scope.insert(hostname.clone());
        }
        self.scope_verdicts.insert(hostname, verdict);
    }

    /// Check if a hostname has been marked out of scope.
    pub fn is_out_of_scope(&self, hostname: &str) -> bool {
        if self
            .out_of_scope
            .iter()
            .any(|host| same_hostname(host, hostname))
        {
            return true;
        }
        self.scope_verdicts.iter().any(|(host, verdict)| {
            same_hostname(host, hostname) && matches!(verdict, ScopeVerdict::OutOfScope { .. })
        })
    }

    /// Check if a vulnerability class is excluded.
    pub fn is_vuln_class_excluded(&self, class: &str) -> bool {
        self.rules
            .excluded_vuln_classes
            .iter()
            .any(|c| c.eq_ignore_ascii_case(class))
    }

    /// Total number of discovered attack surface elements.
    pub fn attack_surface_size(&self) -> usize {
        self.discovered_subdomains.len()
            + self.discovered_hosts.len()
            + self.discovered_endpoints.len()
    }

    /// Get all unique technologies across top-level, hosts, and subdomains.
    pub fn all_technologies(&self) -> Vec<&TechnologyFingerprint> {
        let mut all: Vec<&TechnologyFingerprint> = self.technologies.iter().collect();
        for host in &self.discovered_hosts {
            for tech in &host.technologies {
                if !all
                    .iter()
                    .any(|t| t.name == tech.name && t.version == tech.version)
                {
                    all.push(tech);
                }
            }
        }
        for sub in &self.discovered_subdomains {
            for tech in &sub.technologies {
                if !all
                    .iter()
                    .any(|t| t.name == tech.name && t.version == tech.version)
                {
                    all.push(tech);
                }
            }
        }
        all
    }

    /// Generate a summary of discovered attack surface.
    pub fn surface_summary(&self) -> AttackSurfaceSummary {
        AttackSurfaceSummary {
            subdomains: self.discovered_subdomains.len(),
            hosts: self.discovered_hosts.len(),
            endpoints: self.discovered_endpoints.len(),
            technologies: self.technologies.len(),
            observations: self.observations.len(),
            out_of_scope: self.out_of_scope.len(),
        }
    }
}

fn same_hostname(left: &str, right: &str) -> bool {
    left.trim_end_matches('.')
        .eq_ignore_ascii_case(right.trim_end_matches('.'))
}

/// Summary statistics of the discovered attack surface.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AttackSurfaceSummary {
    pub subdomains: usize,
    pub hosts: usize,
    pub endpoints: usize,
    pub technologies: usize,
    pub observations: usize,
    pub out_of_scope: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_scope_new() {
        let scope = LiveScope::new(EngagementMode::BugBounty, "example.com".to_string());
        assert_eq!(scope.target_root.as_deref(), Some("example.com"));
        assert_eq!(scope.mode, EngagementMode::BugBounty);
        assert!(scope.discovered_subdomains.is_empty());
    }

    #[test]
    fn add_subdomain_deduplicates() {
        let mut scope = LiveScope::new(EngagementMode::BugBounty, "example.com".to_string());
        let sub = DiscoveredSubdomain {
            subdomain: "api.example.com".to_string(),
            ip_addresses: vec!["1.2.3.4".to_string()],
            technologies: vec![],
            alive: true,
        };
        scope.add_subdomain(sub.clone());
        scope.add_subdomain(sub);
        assert_eq!(scope.discovered_subdomains.len(), 1);
    }

    #[test]
    fn out_of_scope_tracking() {
        let mut scope = LiveScope::new(EngagementMode::BugBounty, "example.com".to_string());
        scope.add_out_of_scope(
            "admin.example.com".to_string(),
            "Explicit exclusion".to_string(),
        );
        assert!(scope.is_out_of_scope("admin.example.com"));
        assert!(!scope.is_out_of_scope("api.example.com"));
    }

    #[test]
    fn scope_verdicts_normalize_hostname_case_and_root_dot() {
        let mut scope = LiveScope::new(EngagementMode::BugBounty, "example.com".into());
        scope.add_out_of_scope("Admin.Example.com.".into(), "excluded".into());
        assert!(scope.is_out_of_scope("admin.example.com"));
        assert!(!scope.is_out_of_scope("otheradmin.example.com"));
        scope.set_verdict("ADMIN.EXAMPLE.COM".into(), ScopeVerdict::InScope);
        assert!(!scope.is_out_of_scope("admin.example.com."));
        scope.set_verdict(
            "admin.example.com".into(),
            ScopeVerdict::OutOfScope {
                reason: "excluded".into(),
            },
        );
        assert!(scope.is_out_of_scope("ADMIN.EXAMPLE.COM."));
    }

    #[test]
    fn vuln_class_exclusion() {
        let mut scope = LiveScope::new(EngagementMode::BugBounty, "example.com".to_string());
        scope.rules.excluded_vuln_classes = vec!["clickjacking".to_string()];
        assert!(scope.is_vuln_class_excluded("clickjacking"));
        assert!(!scope.is_vuln_class_excluded("xss"));
    }

    #[test]
    fn attack_surface_size() {
        let mut scope = LiveScope::new(EngagementMode::BugBounty, "example.com".to_string());
        scope.add_subdomain(DiscoveredSubdomain {
            subdomain: "api.example.com".to_string(),
            ip_addresses: vec![],
            technologies: vec![],
            alive: true,
        });
        scope.add_host(DiscoveredHost {
            hostname: "api.example.com".to_string(),
            ip_addresses: vec!["1.2.3.4".to_string()],
            ports: vec![],
            technologies: vec![],
            services: vec![],
        });
        assert_eq!(scope.attack_surface_size(), 2);
    }

    #[test]
    fn engagement_mode_defaults() {
        assert!(EngagementMode::BugBounty.requires_scope_enforcement());
        assert!(!EngagementMode::Ctf.requires_scope_enforcement());
        assert_eq!(EngagementMode::BugBounty.as_str(), "bug_bounty");
    }
}
