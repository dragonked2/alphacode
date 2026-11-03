//! Bug Bounty Agent — enhanced agent loop for bug bounty workflows.
//!
//! This module extends the standard agent loop with bug bounty-specific
//! capabilities: intelligent tool chaining, attack surface tracking,
//! vulnerability correlation, and explicit task tracking.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Bug bounty-specific agent context.
#[derive(Debug, Clone)]
pub struct BugBountyContext {
    /// The target being assessed
    pub target: Arc<RwLock<String>>,
    /// Current attack surface
    pub attack_surface: Arc<RwLock<AttackSurface>>,
    /// Discovered vulnerabilities
    pub vulnerabilities: Arc<RwLock<Vec<Vulnerability>>>,
    /// Tool execution history
    pub tool_history: Arc<RwLock<Vec<ToolExecution>>>,
    /// Pending tasks
    pub pending_tasks: Arc<RwLock<Vec<BountyTask>>>,
    /// Completed tasks
    pub completed_tasks: Arc<RwLock<Vec<BountyTask>>>,
    /// Current phase
    pub current_phase: Arc<RwLock<BountyPhase>>,
    /// Scope boundaries
    pub scope: Arc<RwLock<Scope>>,
    /// Recon data
    pub recon_data: Arc<RwLock<ReconData>>,
}

/// Attack surface information.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AttackSurface {
    /// Discovered subdomains
    pub subdomains: HashSet<String>,
    /// Live hosts
    pub live_hosts: HashSet<String>,
    /// Open ports
    pub open_ports: HashMap<String, Vec<u16>>,
    /// Discovered URLs
    pub urls: HashSet<String>,
    /// Discovered endpoints
    pub endpoints: HashSet<String>,
    /// Discovered parameters
    pub parameters: HashSet<String>,
    /// Technologies detected
    pub technologies: HashMap<String, Vec<String>>,
    /// Web servers
    pub web_servers: HashMap<String, String>,
    /// CDN providers
    pub cdn_providers: HashMap<String, String>,
    /// TLS info
    pub tls_info: HashMap<String, String>,
}

/// A discovered vulnerability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vulnerability {
    /// Unique identifier
    pub id: String,
    /// Vulnerability type
    pub vuln_type: VulnType,
    /// Affected URL/endpoint
    pub endpoint: String,
    /// Severity
    pub severity: Severity,
    /// Description
    pub description: String,
    /// Evidence
    pub evidence: String,
    /// Remediation
    pub remediation: String,
    /// CVSS score
    pub cvss: Option<f64>,
    /// CVE reference
    pub cve: Option<String>,
    /// Discovered at timestamp
    pub discovered_at: String,
}

/// Vulnerability types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VulnType {
    XSS,
    SQLi,
    SSRF,
    IDOR,
    CSRF,
    CORS,
    CRLF,
    SSTI,
    RCE,
    LFI,
    OpenRedirect,
    SubdomainTakeover,
    InformationDisclosure,
    SecurityMisconfiguration,
    AuthenticationBypass,
    AuthorizationBypass,
    RateLimiting,
    Other,
}

/// Severity levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Critical,
    High,
    Medium,
    Low,
    Info,
}

/// Tool execution record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecution {
    /// Tool name
    pub tool: String,
    /// Execution timestamp
    pub timestamp: String,
    /// Target
    pub target: String,
    /// Result summary
    pub result: String,
    /// Success
    pub success: bool,
    /// Duration in seconds
    pub duration: u64,
}

/// A bug bounty task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BountyTask {
    /// Task ID
    pub id: String,
    /// Task name
    pub name: String,
    /// Task description
    pub description: String,
    /// Tool to execute
    pub tool: String,
    /// Tool parameters
    pub params: serde_json::Value,
    /// Phase
    pub phase: BountyPhase,
    /// Completed
    pub completed: bool,
    /// Result
    pub result: Option<String>,
    /// Dependencies
    pub dependencies: Vec<String>,
    /// Priority
    pub priority: u32,
}

/// Bug bounty phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BountyPhase {
    Reconnaissance,
    AttackSurfaceMapping,
    VulnerabilityDiscovery,
    Exploitation,
    Reporting,
    Complete,
}

/// Scope boundaries.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Scope {
    /// In-scope domains
    pub in_scope: Vec<String>,
    /// Out-of-scope domains
    pub out_of_scope: Vec<String>,
    /// In-scope IP ranges
    pub in_scope_ips: Vec<String>,
    /// Out-of-scope IP ranges
    pub out_of_scope_ips: Vec<String>,
    /// In-scope URL patterns
    pub in_scope_patterns: Vec<String>,
    /// Out-of-scope URL patterns
    pub out_of_scope_patterns: Vec<String>,
    /// Forbidden actions
    pub forbidden_actions: Vec<String>,
    /// Severity focus
    pub severity_focus: Vec<Severity>,
}

/// Reconnaissance data.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReconData {
    /// DNS records
    pub dns_records: HashMap<String, Vec<String>>,
    /// WHOIS data
    pub whois: HashMap<String, String>,
    /// Certificate transparency logs
    pub ct_logs: Vec<String>,
    /// Wayback machine snapshots
    pub wayback_snapshots: Vec<String>,
    /// GitHub repos
    pub github_repos: Vec<String>,
    /// Social media
    pub social_media: Vec<String>,
    /// Employee info
    pub employee_info: Vec<String>,
    /// Technology stack
    pub technology_stack: Vec<String>,
}

impl BugBountyContext {
    /// Create a new bug bounty context for the given target.
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: Arc::new(RwLock::new(target.into())),
            attack_surface: Arc::new(RwLock::new(AttackSurface::default())),
            vulnerabilities: Arc::new(RwLock::new(Vec::new())),
            tool_history: Arc::new(RwLock::new(Vec::new())),
            pending_tasks: Arc::new(RwLock::new(Vec::new())),
            completed_tasks: Arc::new(RwLock::new(Vec::new())),
            current_phase: Arc::new(RwLock::new(BountyPhase::Reconnaissance)),
            scope: Arc::new(RwLock::new(Scope::default())),
            recon_data: Arc::new(RwLock::new(ReconData::default())),
        }
    }

    /// Add a discovered subdomain to the attack surface.
    pub async fn add_subdomain(&self, subdomain: String) {
        let mut surface = self.attack_surface.write().await;
        surface.subdomains.insert(subdomain);
    }

    /// Add a live host to the attack surface.
    pub async fn add_live_host(&self, host: String) {
        let mut surface = self.attack_surface.write().await;
        surface.live_hosts.insert(host);
    }

    /// Add a discovered URL to the attack surface.
    pub async fn add_url(&self, url: String) {
        let mut surface = self.attack_surface.write().await;
        surface.urls.insert(url);
    }

    /// Add a discovered vulnerability.
    pub async fn add_vulnerability(&self, vuln: Vulnerability) {
        let mut vulns = self.vulnerabilities.write().await;
        vulns.push(vuln);
    }

    /// Record a tool execution.
    pub async fn record_tool_execution(&self, execution: ToolExecution) {
        let mut history = self.tool_history.write().await;
        history.push(execution);
    }

    /// Add a pending task.
    pub async fn add_task(&self, task: BountyTask) {
        let mut tasks = self.pending_tasks.write().await;
        tasks.push(task);
    }

    /// Complete a task.
    pub async fn complete_task(&self, task_id: &str, result: String) -> Result<()> {
        let mut pending = self.pending_tasks.write().await;
        let mut completed = self.completed_tasks.write().await;

        if let Some(pos) = pending.iter().position(|t| t.id == task_id) {
            let mut task = pending.remove(pos);
            task.completed = true;
            task.result = Some(result);
            completed.push(task);
        }

        Ok(())
    }

    /// Get the current phase.
    pub async fn get_current_phase(&self) -> BountyPhase {
        *self.current_phase.read().await
    }

    /// Advance to the next phase.
    pub async fn advance_phase(&self) -> Result<()> {
        let mut phase = self.current_phase.write().await;
        *phase = match *phase {
            BountyPhase::Reconnaissance => BountyPhase::AttackSurfaceMapping,
            BountyPhase::AttackSurfaceMapping => BountyPhase::VulnerabilityDiscovery,
            BountyPhase::VulnerabilityDiscovery => BountyPhase::Exploitation,
            BountyPhase::Exploitation => BountyPhase::Reporting,
            BountyPhase::Reporting => BountyPhase::Complete,
            BountyPhase::Complete => BountyPhase::Complete,
        };
        Ok(())
    }

    /// Get a summary of the current state.
    pub async fn get_summary(&self) -> BugBountySummary {
        let surface = self.attack_surface.read().await;
        let vulns = self.vulnerabilities.read().await;
        let pending = self.pending_tasks.read().await;
        let completed = self.completed_tasks.read().await;
        let phase = *self.current_phase.read().await;

        BugBountySummary {
            target: self.target.read().await.clone(),
            current_phase: phase,
            subdomains: surface.subdomains.len(),
            live_hosts: surface.live_hosts.len(),
            urls: surface.urls.len(),
            endpoints: surface.endpoints.len(),
            parameters: surface.parameters.len(),
            vulnerabilities: vulns.len(),
            pending_tasks: pending.len(),
            completed_tasks: completed.len(),
        }
    }

    /// Check if a target is in scope.
    pub async fn is_in_scope(&self, target: &str) -> bool {
        use crate::alphacode_security_core::scope_gate::{ScopeDecision, ScopePolicy, decide};

        let scope = self.scope.read().await;
        let allowed: Vec<_> = scope
            .in_scope
            .iter()
            .chain(&scope.in_scope_ips)
            .chain(&scope.in_scope_patterns)
            .cloned()
            .collect();
        let excluded: Vec<_> = scope
            .out_of_scope
            .iter()
            .chain(&scope.out_of_scope_ips)
            .chain(&scope.out_of_scope_patterns)
            .cloned()
            .collect();
        let policy = ScopePolicy::from_lists(&allowed, &excluded, true, &[], 0, 0);
        // Pending (including an empty allowlist) never authorizes this context.
        matches!(decide(&policy, target), ScopeDecision::Allowed { .. })
    }

    /// Get all pending tasks sorted by priority.
    pub async fn get_pending_tasks_sorted(&self) -> Vec<BountyTask> {
        let pending = self.pending_tasks.read().await;
        let mut tasks: Vec<_> = pending.clone();
        tasks.sort_by_key(|t| t.priority);
        tasks
    }

    /// Get vulnerabilities by severity.
    pub async fn get_vulnerabilities_by_severity(&self, severity: Severity) -> Vec<Vulnerability> {
        let vulns = self.vulnerabilities.read().await;
        vulns
            .iter()
            .filter(|v| v.severity == severity)
            .cloned()
            .collect()
    }

    /// Get critical and high vulnerabilities.
    pub async fn get_critical_vulnerabilities(&self) -> Vec<Vulnerability> {
        let vulns = self.vulnerabilities.read().await;
        vulns
            .iter()
            .filter(|v| matches!(v.severity, Severity::Critical | Severity::High))
            .cloned()
            .collect()
    }
}

/// Summary of the bug bounty context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugBountySummary {
    /// Target
    pub target: String,
    /// Current phase
    pub current_phase: BountyPhase,
    /// Subdomains discovered
    pub subdomains: usize,
    /// Live hosts
    pub live_hosts: usize,
    /// URLs discovered
    pub urls: usize,
    /// Endpoints discovered
    pub endpoints: usize,
    /// Parameters discovered
    pub parameters: usize,
    /// Vulnerabilities found
    pub vulnerabilities: usize,
    /// Pending tasks
    pub pending_tasks: usize,
    /// Completed tasks
    pub completed_tasks: usize,
}

impl std::fmt::Display for BugBountySummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Bug Bounty Agent Summary")?;
        writeln!(f, "=========================")?;
        writeln!(f, "Target: {}", self.target)?;
        writeln!(f, "Phase: {:?}", self.current_phase)?;
        writeln!(f, "Subdomains: {}", self.subdomains)?;
        writeln!(f, "Live Hosts: {}", self.live_hosts)?;
        writeln!(f, "URLs: {}", self.urls)?;
        writeln!(f, "Endpoints: {}", self.endpoints)?;
        writeln!(f, "Parameters: {}", self.parameters)?;
        writeln!(f, "Vulnerabilities: {}", self.vulnerabilities)?;
        writeln!(
            f,
            "Tasks: {}/{} completed",
            self.completed_tasks,
            self.pending_tasks + self.completed_tasks
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_context_creation() {
        let ctx = BugBountyContext::new("example.com");
        assert_eq!(*ctx.target.read().await, "example.com");
    }

    #[tokio::test]
    async fn test_add_subdomain() {
        let ctx = BugBountyContext::new("example.com");
        ctx.add_subdomain("sub.example.com".to_string()).await;
        let summary = ctx.get_summary().await;
        assert_eq!(summary.subdomains, 1);
    }

    #[tokio::test]
    async fn test_add_vulnerability() {
        let ctx = BugBountyContext::new("example.com");
        ctx.add_vulnerability(Vulnerability {
            id: "vuln-1".to_string(),
            vuln_type: VulnType::XSS,
            endpoint: "https://example.com/search".to_string(),
            severity: Severity::High,
            description: "Reflected XSS".to_string(),
            evidence: "Payload reflected in response".to_string(),
            remediation: "Encode output".to_string(),
            cvss: Some(7.5),
            cve: None,
            discovered_at: "2026-01-01".to_string(),
        })
        .await;
        let summary = ctx.get_summary().await;
        assert_eq!(summary.vulnerabilities, 1);
    }

    #[tokio::test]
    async fn test_phase_advancement() {
        let ctx = BugBountyContext::new("example.com");
        assert_eq!(ctx.get_current_phase().await, BountyPhase::Reconnaissance);
        ctx.advance_phase().await.unwrap();
        assert_eq!(
            ctx.get_current_phase().await,
            BountyPhase::AttackSurfaceMapping
        );
    }

    #[tokio::test]
    async fn test_scope_check() {
        let ctx = BugBountyContext::new("example.com");
        let mut scope = ctx.scope.write().await;
        scope.in_scope.push("example.com".to_string());
        drop(scope);
        assert!(ctx.is_in_scope("https://example.com").await);
        assert!(!ctx.is_in_scope("https://evil.com").await);
    }

    #[tokio::test]
    async fn scope_rejects_domain_text_outside_the_actual_host() {
        let ctx = BugBountyContext::new("example.com");
        assert!(!ctx.is_in_scope("https://example.com").await);
        ctx.scope.write().await.in_scope.push("example.com".into());
        assert!(ctx.is_in_scope("https://api.example.com").await);
        for target in [
            "https://example.com.evil.test/",
            "https://evil.test/example.com",
            "https://evil.test/?next=example.com",
            "https://example.com@evil.test/",
            r"https://evil.test\@example.com/",
        ] {
            assert!(!ctx.is_in_scope(target).await, "{target}");
        }
    }

    #[tokio::test]
    async fn scope_honors_ip_and_url_exclusions() {
        let ctx = BugBountyContext::new("example.com");
        {
            let mut scope = ctx.scope.write().await;
            scope
                .in_scope_patterns
                .push("https://example.com/api".into());
            scope
                .out_of_scope_patterns
                .push("https://example.com/api/private".into());
            scope.in_scope_ips.push("192.0.2.0/24".into());
            scope.out_of_scope_ips.push("192.0.2.7".into());
        }
        assert!(ctx.is_in_scope("https://example.com/api/public").await);
        assert!(!ctx.is_in_scope("https://example.com/api/private/1").await);
        assert!(!ctx.is_in_scope("https://example.com/api-admin").await);
        assert!(ctx.is_in_scope("https://192.0.2.8").await);
        assert!(!ctx.is_in_scope("https://192.0.2.7").await);
        assert!(!ctx.is_in_scope("https://192.0.3.8").await);
    }
}
