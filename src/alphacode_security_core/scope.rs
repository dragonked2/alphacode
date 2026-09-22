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
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
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
    pub fn set_verdict(&mut self, hostname: String, verdict: ScopeVerdict) {
        if matches!(verdict, ScopeVerdict::OutOfScope { .. }) {
            self.out_of_scope.insert(hostname.clone());
        }
        self.scope_verdicts.insert(hostname, verdict);
    }

    /// Check if a hostname has been marked out of scope.
    pub fn is_out_of_scope(&self, hostname: &str) -> bool {
        if self.out_of_scope.contains(hostname) {
            return true;
        }
        matches!(
            self.scope_verdicts.get(hostname),
            Some(ScopeVerdict::OutOfScope { .. })
        )
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
