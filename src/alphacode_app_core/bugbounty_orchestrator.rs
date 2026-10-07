//! Bug Bounty Orchestrator — the "beast mode" intelligence layer.
//!
//! This module orchestrates the complete bug bounty workflow from target
//! understanding to vulnerability reporting. It chains tools intelligently,
//! never misses any task, and provides comprehensive attack surface mapping.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

/// The current phase of the bug bounty workflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BountyPhase {
    Reconnaissance,
    AttackSurfaceMapping,
    VulnerabilityDiscovery,
    Exploitation,
    Reporting,
    Complete,
}

impl BountyPhase {
    /// Get the next phase in the workflow.
    pub fn next(&self) -> Option<Self> {
        match self {
            Self::Reconnaissance => Some(Self::AttackSurfaceMapping),
            Self::AttackSurfaceMapping => Some(Self::VulnerabilityDiscovery),
            Self::VulnerabilityDiscovery => Some(Self::Exploitation),
            Self::Exploitation => Some(Self::Reporting),
            Self::Reporting => Some(Self::Complete),
            Self::Complete => None,
        }
    }

    /// Get a human-readable description of the phase.
    pub fn description(&self) -> &'static str {
        match self {
            Self::Reconnaissance => {
                "Reconnaissance: subdomain enumeration, port scanning, service discovery"
            }
            Self::AttackSurfaceMapping => {
                "Attack Surface Mapping: URL collection, content discovery, parameter extraction"
            }
            Self::VulnerabilityDiscovery => {
                "Vulnerability Discovery: template scanning, fuzzing, manual testing"
            }
            Self::Exploitation => {
                "Exploitation: targeted attacks based on discovered vulnerabilities"
            }
            Self::Reporting => "Reporting: comprehensive vulnerability documentation",
            Self::Complete => "Complete: all phases finished",
        }
    }
}

/// A single task in the bug bounty workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BountyTask {
    /// Unique task identifier
    pub id: String,
    /// Human-readable task name
    pub name: String,
    /// Task description
    pub description: String,
    /// The tool to execute for this task
    pub tool: String,
    /// Tool parameters
    pub params: serde_json::Value,
    /// The phase this task belongs to
    pub phase: BountyPhase,
    /// Whether this task has been completed
    pub completed: bool,
    /// Task result (populated after execution)
    pub result: Option<String>,
    /// Dependencies — tasks that must complete before this one
    pub dependencies: Vec<String>,
    /// Priority (lower = higher priority)
    pub priority: u32,
    /// Estimated duration in seconds
    pub estimated_duration: u64,
    /// Retry count
    pub retry_count: u32,
    /// Maximum retries
    pub max_retries: u32,
}

/// A plausible DNS hostname: labels of alphanumerics/hyphens/underscores,
/// separated by dots, with no whitespace and no scheme or path.
fn is_hostname(candidate: &str) -> bool {
    if candidate.is_empty() || candidate.len() > 253 || candidate.starts_with('.') {
        return false;
    }
    let candidate = candidate.strip_suffix('.').unwrap_or(candidate);
    if !candidate.contains('.') {
        return false;
    }
    candidate.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    })
}

/// Split `host:port`, tolerating bracketed IPv6 and an `http://` prefix.
/// Returns the **host** (without the port) and the port, or `None` when there
/// is no port to extract.
fn split_host_port(candidate: &str) -> Option<(String, u16)> {
    let trimmed = candidate.trim();
    let rest = trimmed
        .strip_prefix("http://")
        .or_else(|| trimmed.strip_prefix("https://"))
        .unwrap_or(trimmed);
    // Drop any path/query/fragment before looking for the port.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);

    let (host, port_text): (&str, &str) = if let Some(close) = authority.strip_prefix('[') {
        // Bracketed IPv6 literal: [::1]:8080. The brackets stay on the host so
        // `format!("{host}:{port}")` is still a valid authority.
        let host_end = close.find(']')? + 2; // +1 for ']' consumed, +1 to include it
        let host = authority.get(..host_end)?;
        let after = authority.get(host_end..)?;
        (host, after.strip_prefix(':')?)
    } else {
        authority.rsplit_once(':')?
    };

    let port: u16 = port_text.trim().parse().ok()?;
    (port > 0).then(|| (host.to_string(), port))
}

/// Recover a port number from a scanner line.
///
/// Accepts a bare `443`, a `host:443` pair, and the trailing `443/tcp` form
/// that nmap prints. Returns `None` for anything else so a line of banner
/// text is not silently turned into port 0.
fn parse_port(line: &str) -> Option<u16> {
    let first = line.trim();
    if let Some((_, port)) = first.rsplit_once(':') {
        let port = port.split('/').next().unwrap_or(port);
        return port.parse().ok().filter(|p: &u16| *p > 0);
    }
    // nmap: "443/tcp open https"
    let leading = first.split_whitespace().next().unwrap_or_default();
    let digits = leading.split('/').next().unwrap_or_default();
    digits.parse().ok().filter(|p: &u16| *p > 0)
}

/// Heuristic: does this line look like a scanner-reported vulnerability?
///
/// Requires a severity-ish or template-ish marker so that ordinary output
/// (headers, URLs, banners) is not recorded as a finding.
fn is_vulnerability_line(line: &str) -> bool {
    const MARKERS: &[&str] = &[
        "[critical]",
        "[high]",
        "[medium]",
        "[low]",
        "[info]",
        "vulnerability",
        "vulnerable",
        "found:",
        "reflected",
        "injection",
        "misconfiguration",
        "disclosure",
        "exploit",
    ];
    let lower = line.to_ascii_lowercase();
    MARKERS.iter().any(|marker| lower.contains(marker))
}

/// The bug bounty orchestrator — manages the complete workflow.
#[derive(Debug, Clone)]
pub struct BugBountyOrchestrator {
    /// The target domain or URL
    pub target: String,
    /// Current phase
    pub current_phase: Arc<RwLock<BountyPhase>>,
    /// All tasks in the workflow
    pub tasks: Arc<RwLock<HashMap<String, BountyTask>>>,
    /// Discovered subdomains
    pub subdomains: Arc<RwLock<HashSet<String>>>,
    /// Discovered URLs
    pub urls: Arc<RwLock<HashSet<String>>>,
    /// Discovered vulnerabilities
    pub vulnerabilities: Arc<RwLock<HashMap<String, String>>>,
    /// Discovered ports
    pub ports: Arc<RwLock<HashSet<u16>>>,
    /// Discovered services
    pub services: Arc<RwLock<HashMap<String, String>>>,
}

impl BugBountyOrchestrator {
    /// Create a new orchestrator for the given target.
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            current_phase: Arc::new(RwLock::new(BountyPhase::Reconnaissance)),
            tasks: Arc::new(RwLock::new(HashMap::new())),
            subdomains: Arc::new(RwLock::new(HashSet::new())),
            urls: Arc::new(RwLock::new(HashSet::new())),
            vulnerabilities: Arc::new(RwLock::new(HashMap::new())),
            ports: Arc::new(RwLock::new(HashSet::new())),
            services: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Check if a tool is available and provide alternatives if not.
    ///
    /// Returns the tool to use (either the original or an alternative).
    pub async fn resolve_tool(&self, tool: &str) -> String {
        use crate::alphacode_app_core::bugbounty_doctor::ToolStatus;
        use crate::alphacode_app_core::bugbounty_doctor::which;

        if let ToolStatus::Present { .. } = which(tool) {
            return tool.to_string();
        }

        // Tool not found, try alternatives
        let alternative = match tool {
            "gau" => "waybackurls",
            "waybackurls" => "gau",
            "katana" => "gau",
            "subfinder" => "assetfinder",
            "amass" => "subfinder",
            "assetfinder" => "subfinder",
            "dnsx" => "subfinder",
            "httpx" => "curl",
            "nuclei" => "httpx",
            _ => return tool.to_string(),
        };

        if let ToolStatus::Present { .. } = which(alternative) {
            tracing::info!(
                "Tool '{}' not found, using alternative '{}'",
                tool,
                alternative
            );
            return alternative.to_string();
        }

        // No alternative available, return original (will fail later)
        tracing::warn!("Tool '{}' not found and no alternative available", tool);
        tool.to_string()
    }

    /// Ensure all required tools are installed, auto-installing if possible.
    ///
    /// Goes through [`bugbounty_install::install_all`] rather than calling
    /// `plan_for` + `install_one` directly. The direct form bypassed the
    /// install module's stated safety contract — "callers must pass an explicit
    /// allow-list" — and would shell out to `go install` / `pipx install` for
    /// any name a task happened to carry, without checking it was a known tool.
    /// `install_all` constrains resolution to `TOOL_SPECS` and reports anything
    /// unrecognised as [`InstallOutcome::Skipped`] rather than dropping it.
    pub async fn ensure_tools_installed(&self) {
        use crate::alphacode_app_core::bugbounty_install::install_all;

        // Get list of tools used by tasks
        let tools: HashSet<String> = {
            let tasks = self.tasks.read().await;
            tasks.values().map(|t| t.tool.clone()).collect()
        };

        // Deterministic order: a HashSet iterates in randomized order per
        // process, which makes the install log unreproducible between runs.
        let mut requested: Vec<String> = tools.into_iter().collect();
        requested.sort();

        for (tool, outcome) in install_all(&requested).await {
            match outcome {
                    crate::alphacode_app_core::bugbounty_install::InstallOutcome::AlreadyPresent { .. } => {}
                    crate::alphacode_app_core::bugbounty_install::InstallOutcome::Installed { .. } => {
                        tracing::info!("Auto-installed tool: {}", tool);
                    }
                    crate::alphacode_app_core::bugbounty_install::InstallOutcome::InstalledButNotOnPath { hint, .. } => {
                        tracing::warn!("Tool {} installed but not on PATH: {}", tool, hint);
                    }
                    crate::alphacode_app_core::bugbounty_install::InstallOutcome::Failed { output } => {
                        tracing::warn!("Failed to install tool {}: {}", tool, output);
                    }
                    crate::alphacode_app_core::bugbounty_install::InstallOutcome::MissingPrerequisite { needed, hint } => {
                        tracing::warn!("Cannot install tool {}: missing prerequisite {}, {}", tool, needed, hint);
                    }
                    crate::alphacode_app_core::bugbounty_install::InstallOutcome::Skipped { reason } => {
                        tracing::debug!("Skipped installing tool {}: {}", tool, reason);
                    }
                    crate::alphacode_app_core::bugbounty_install::InstallOutcome::TimedOut => {
                        tracing::warn!("Timed out installing tool: {}", tool);
                    }
            }
        }
    }

    /// Initialize the orchestrator with default tasks for the target.
    pub async fn initialize(&self) -> Result<()> {
        let target = self.target.clone();
        let mut tasks = self.tasks.write().await;

        // Phase 1: Reconnaissance
        tasks.insert(
            "recon_subfinder".to_string(),
            BountyTask {
                id: "recon_subfinder".to_string(),
                name: "Subdomain Enumeration (subfinder)".to_string(),
                description: "Passive subdomain enumeration using subfinder".to_string(),
                tool: "subfinder".to_string(),
                params: serde_json::json!({"domain": target, "all": true}),
                phase: BountyPhase::Reconnaissance,
                completed: false,
                result: None,
                dependencies: vec![],
                priority: 1,
                estimated_duration: 120,
                retry_count: 0,
                max_retries: 3,
            },
        );

        tasks.insert(
            "recon_amass".to_string(),
            BountyTask {
                id: "recon_amass".to_string(),
                name: "Subdomain Enumeration (amass)".to_string(),
                description: "In-depth subdomain enumeration using amass".to_string(),
                tool: "amass".to_string(),
                params: serde_json::json!({"domain": target, "passive": true}),
                phase: BountyPhase::Reconnaissance,
                completed: false,
                result: None,
                dependencies: vec![],
                priority: 2,
                estimated_duration: 300,
                retry_count: 0,
                max_retries: 3,
            },
        );

        tasks.insert(
            "recon_assetfinder".to_string(),
            BountyTask {
                id: "recon_assetfinder".to_string(),
                name: "Asset Discovery (assetfinder)".to_string(),
                description: "Find related domains and subdomains".to_string(),
                tool: "assetfinder".to_string(),
                params: serde_json::json!({"domain": target}),
                phase: BountyPhase::Reconnaissance,
                completed: false,
                result: None,
                dependencies: vec![],
                priority: 3,
                estimated_duration: 60,
                retry_count: 0,
                max_retries: 3,
            },
        );

        tasks.insert(
            "recon_dnsx".to_string(),
            BountyTask {
                id: "recon_dnsx".to_string(),
                name: "DNS Enumeration (dnsx)".to_string(),
                description: "DNS record enumeration and resolution".to_string(),
                tool: "dnsx".to_string(),
                params: serde_json::json!({"domain": target}),
                phase: BountyPhase::Reconnaissance,
                completed: false,
                result: None,
                dependencies: vec![],
                priority: 4,
                estimated_duration: 60,
                retry_count: 0,
                max_retries: 3,
            },
        );

        // Phase 2: Attack Surface Mapping
        tasks.insert(
            "surface_httpx".to_string(),
            BountyTask {
                id: "surface_httpx".to_string(),
                name: "HTTP Probing (httpx)".to_string(),
                description: "Probe discovered subdomains for live HTTP services".to_string(),
                tool: "httpx".to_string(),
                params: serde_json::json!({"targets": [], "title": true, "tech_detect": true, "status_codes": true}),
                phase: BountyPhase::AttackSurfaceMapping,
                completed: false,
                result: None,
                dependencies: vec!["recon_subfinder".to_string(), "recon_amass".to_string()],
                priority: 1,
                estimated_duration: 120,
                retry_count: 0,
                max_retries: 3,
            },
        );

        tasks.insert(
            "surface_katana".to_string(),
            BountyTask {
                id: "surface_katana".to_string(),
                name: "Web Crawling (katana)".to_string(),
                description: "Crawl live hosts to discover URLs and endpoints".to_string(),
                tool: "katana".to_string(),
                params: serde_json::json!({"url": [], "depth": 3, "js_crawl": true}),
                phase: BountyPhase::AttackSurfaceMapping,
                completed: false,
                result: None,
                dependencies: vec!["surface_httpx".to_string()],
                priority: 2,
                estimated_duration: 300,
                retry_count: 0,
                max_retries: 3,
            },
        );

        tasks.insert(
            "surface_gau".to_string(),
            BountyTask {
                id: "surface_gau".to_string(),
                name: "URL Discovery (gau)".to_string(),
                description: "Get all URLs from Wayback Machine, Common Crawl, OTX".to_string(),
                tool: "gau".to_string(),
                params: serde_json::json!({"domain": target}),
                phase: BountyPhase::AttackSurfaceMapping,
                completed: false,
                result: None,
                dependencies: vec![],
                priority: 3,
                estimated_duration: 120,
                retry_count: 0,
                max_retries: 3,
            },
        );

        tasks.insert(
            "surface_waybackurls".to_string(),
            BountyTask {
                id: "surface_waybackurls".to_string(),
                name: "Wayback URLs (waybackurls)".to_string(),
                description: "Pull URLs from Wayback Machine".to_string(),
                tool: "waybackurls".to_string(),
                params: serde_json::json!({"domain": target}),
                phase: BountyPhase::AttackSurfaceMapping,
                completed: false,
                result: None,
                dependencies: vec![],
                priority: 4,
                estimated_duration: 60,
                retry_count: 0,
                max_retries: 3,
            },
        );

        tasks.insert(
            "surface_feroxbuster".to_string(),
            BountyTask {
                id: "surface_feroxbuster".to_string(),
                name: "Content Discovery (feroxbuster)".to_string(),
                description: "Discover hidden directories and files".to_string(),
                tool: "feroxbuster".to_string(),
                params: serde_json::json!({"url": [], "recursive": true, "depth": 3}),
                phase: BountyPhase::AttackSurfaceMapping,
                completed: false,
                result: None,
                dependencies: vec!["surface_httpx".to_string()],
                priority: 5,
                estimated_duration: 300,
                retry_count: 0,
                max_retries: 3,
            },
        );

        // Phase 3: Vulnerability Discovery
        tasks.insert(
            "vuln_nuclei".to_string(),
            BountyTask {
                id: "vuln_nuclei".to_string(),
                name: "Template Scanning (nuclei)".to_string(),
                description: "Scan for known vulnerabilities using nuclei templates".to_string(),
                tool: "nuclei".to_string(),
                params: serde_json::json!({"target": [], "severity": "critical,high,medium"}),
                phase: BountyPhase::VulnerabilityDiscovery,
                completed: false,
                result: None,
                dependencies: vec!["surface_httpx".to_string()],
                priority: 1,
                estimated_duration: 300,
                retry_count: 0,
                max_retries: 3,
            },
        );

        tasks.insert(
            "vuln_nikto".to_string(),
            BountyTask {
                id: "vuln_nikto".to_string(),
                name: "Web Server Scanning (nikto)".to_string(),
                description: "Scan for known web server vulnerabilities".to_string(),
                tool: "nikto".to_string(),
                params: serde_json::json!({"url": []}),
                phase: BountyPhase::VulnerabilityDiscovery,
                completed: false,
                result: None,
                dependencies: vec!["surface_httpx".to_string()],
                priority: 2,
                estimated_duration: 300,
                retry_count: 0,
                max_retries: 3,
            },
        );

        tasks.insert(
            "vuln_dalfox".to_string(),
            BountyTask {
                id: "vuln_dalfox".to_string(),
                name: "XSS Scanning (dalfox)".to_string(),
                description: "Scan for XSS vulnerabilities".to_string(),
                tool: "dalfox".to_string(),
                params: serde_json::json!({"url": []}),
                phase: BountyPhase::VulnerabilityDiscovery,
                completed: false,
                result: None,
                dependencies: vec!["surface_katana".to_string()],
                priority: 3,
                estimated_duration: 300,
                retry_count: 0,
                max_retries: 3,
            },
        );

        tasks.insert(
            "vuln_corsy".to_string(),
            BountyTask {
                id: "vuln_corsy".to_string(),
                name: "CORS Scanning (corsy)".to_string(),
                description: "Scan for CORS misconfigurations".to_string(),
                tool: "corsy".to_string(),
                params: serde_json::json!({"url": []}),
                phase: BountyPhase::VulnerabilityDiscovery,
                completed: false,
                result: None,
                dependencies: vec!["surface_httpx".to_string()],
                priority: 4,
                estimated_duration: 120,
                retry_count: 0,
                max_retries: 3,
            },
        );

        tasks.insert(
            "vuln_crlfuzz".to_string(),
            BountyTask {
                id: "vuln_crlfuzz".to_string(),
                name: "CRLF Scanning (crlfuzz)".to_string(),
                description: "Scan for CRLF injection vulnerabilities".to_string(),
                tool: "crlfuzz".to_string(),
                params: serde_json::json!({"url": []}),
                phase: BountyPhase::VulnerabilityDiscovery,
                completed: false,
                result: None,
                dependencies: vec!["surface_httpx".to_string()],
                priority: 5,
                estimated_duration: 120,
                retry_count: 0,
                max_retries: 3,
            },
        );

        // Phase 4: Exploitation
        tasks.insert(
            "exploit_sqlmap".to_string(),
            BountyTask {
                id: "exploit_sqlmap".to_string(),
                name: "SQL Injection (sqlmap)".to_string(),
                description: "Test for SQL injection vulnerabilities".to_string(),
                tool: "sqlmap".to_string(),
                params: serde_json::json!({"url": [], "batch": true, "level": 2, "risk": 2}),
                phase: BountyPhase::Exploitation,
                completed: false,
                result: None,
                dependencies: vec!["surface_katana".to_string()],
                priority: 1,
                estimated_duration: 600,
                retry_count: 0,
                max_retries: 3,
            },
        );

        tasks.insert(
            "exploit_gobuster".to_string(),
            BountyTask {
                id: "exploit_gobuster".to_string(),
                name: "Directory Bruteforce (gobuster)".to_string(),
                description: "Brute force directories and files".to_string(),
                tool: "gobuster".to_string(),
                params: serde_json::json!({"url": [], "mode": "dir"}),
                phase: BountyPhase::Exploitation,
                completed: false,
                result: None,
                dependencies: vec!["surface_httpx".to_string()],
                priority: 2,
                estimated_duration: 300,
                retry_count: 0,
                max_retries: 3,
            },
        );

        Ok(())
    }

    /// Get all tasks for a specific phase.
    pub async fn get_tasks_for_phase(&self, phase: BountyPhase) -> Vec<BountyTask> {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .filter(|t| t.phase == phase)
            .cloned()
            .collect()
    }

    /// Get all pending tasks.
    pub async fn get_pending_tasks(&self) -> Vec<BountyTask> {
        let tasks = self.tasks.read().await;
        tasks.values().filter(|t| !t.completed).cloned().collect()
    }

    /// Get all completed tasks.
    pub async fn get_completed_tasks(&self) -> Vec<BountyTask> {
        let tasks = self.tasks.read().await;
        tasks.values().filter(|t| t.completed).cloned().collect()
    }

    // ---------------------------------------------------------------------
    // Result accumulation
    //
    // `get_summary` reports counts from these five collections, but nothing
    // ever wrote to them: they were constructed empty and only ever read, so
    // every summary reported 0 subdomains / 0 URLs / 0 findings no matter how
    // much recon had actually run. The collectors below are fed from
    // `complete_task`, which is the single point where a successful task
    // result lands.
    // ---------------------------------------------------------------------

    /// Record one discovered subdomain. Returns `true` if it was new.
    pub async fn record_subdomain(&self, subdomain: impl Into<String>) -> bool {
        let subdomain = subdomain.into().trim().to_ascii_lowercase();
        if subdomain.is_empty() {
            return false;
        }
        self.subdomains.write().await.insert(subdomain)
    }

    /// Record one discovered URL. Returns `true` if it was new.
    pub async fn record_url(&self, url: impl Into<String>) -> bool {
        let url = url.into().trim().to_string();
        if url.is_empty() {
            return false;
        }
        self.urls.write().await.insert(url)
    }

    /// Record an open port. Returns `true` if it was new.
    pub async fn record_port(&self, port: u16) -> bool {
        self.ports.write().await.insert(port)
    }

    /// Record a service, e.g. `("example.com:443", "nginx")`. Returns `true` if new.
    pub async fn record_service(
        &self,
        host: impl Into<String>,
        service: impl Into<String>,
    ) -> bool {
        // `HashMap::insert` returns the *previous* value, so `is_none()` is
        // what "this was new" means.
        self.services
            .write()
            .await
            .insert(host.into(), service.into())
            .is_none()
    }

    /// Record a vulnerability keyed by its identifier. Returns `true` if new.
    pub async fn record_vulnerability(
        &self,
        id: impl Into<String>,
        detail: impl Into<String>,
    ) -> bool {
        self.vulnerabilities
            .write()
            .await
            .insert(id.into(), detail.into())
            .is_none()
    }

    /// Parse a finished tool's stdout into the accumulators.
    ///
    /// The recon tools all emit newline-delimited results, so this is a
    /// line-oriented ingest rather than a structured parse. Unrecognised and
    /// non-parseable lines are ignored: a summary that under-reports is better
    /// than one that fills up with usage banners and error text.
    pub async fn ingest_tool_output(&self, tool: &str, output: &str) {
        let tool = tool.to_ascii_lowercase();
        for raw in output.lines() {
            let line = raw.trim();
            if line.is_empty() {
                continue;
            }
            // Skip anything that is obviously not a result.
            if line.starts_with('[') && line.contains("INF]") {
                continue;
            }
            match tool.as_str() {
                "subfinder" => {
                    if is_hostname(line) {
                        self.record_subdomain(line).await;
                    }
                }
                "assetfinder" => {
                    if is_hostname(line) {
                        self.record_subdomain(line).await;
                    }
                }
                "amass" => {
                    // amass emits indented subdomains under section headers.
                    if is_hostname(line) {
                        self.record_subdomain(line).await;
                    }
                }
                "gau" | "waybackurls" | "katana" | "hakrawler" | "gospider" | "gospiderx"
                | "urls" => {
                    if line.starts_with("http://") || line.starts_with("https://") {
                        self.record_url(line).await;
                    }
                }
                "httpx" | "httprobe" | "unfurl" => {
                    // httpx emits real URLs; a bare `host:port` line still
                    // yields the port and service, but must not be recorded in
                    // `urls`, which is meant to hold URLs.
                    if line.starts_with("http://") || line.starts_with("https://") {
                        self.record_url(line).await;
                    }
                    if let Some((host, port)) = split_host_port(line) {
                        self.record_port(port).await;
                        self.record_service(host, "http").await;
                    } else if is_hostname(line) {
                        self.record_url(line).await;
                    }
                }
                "naabu" | "nmap" => {
                    if let Some(port) = parse_port(line) {
                        self.record_port(port).await;
                    }
                }
                "nuclei" | "nikto" | "sqlmap" | "dalfox" | "corsy" | "crlfuzz" => {
                    if is_vulnerability_line(line) {
                        self.record_vulnerability(
                            format!("{tool}:{}", self.vulnerabilities.read().await.len()),
                            crate::alphacode_core::util::truncate_str(line, 300).to_string(),
                        )
                        .await;
                    }
                }
                _ => {}
            }
        }
    }

    /// Mark a task as completed and fold its output into the accumulators.
    pub async fn complete_task(&self, task_id: &str, result: String) -> Result<()> {
        // The `write` guard is released before ingesting: `ingest_tool_output`
        // takes read locks on the accumulators, and holding this one across
        // those awaits would serialise every completion behind the slowest
        // parse.
        // Validate the id and set both fields under a single write guard, then
        // release it before ingesting: `ingest_tool_output` takes read locks on
        // the accumulators, and holding the tasks lock across those awaits would
        // serialise every completion behind the slowest parse. `result` is
        // cloned for storage so the original stays owned here.
        let tool = {
            let mut tasks = self.tasks.write().await;
            let task = tasks.get_mut(task_id).ok_or_else(|| {
                anyhow::anyhow!(
                    "unknown task `{task_id}`: it was never registered or has been removed"
                )
            })?;
            task.completed = true;
            task.result = Some(result.clone());
            task.tool.clone()
        };
        self.ingest_tool_output(&tool, &result).await;
        Ok(())
    }

    /// Get the current phase.
    pub async fn get_current_phase(&self) -> BountyPhase {
        *self.current_phase.read().await
    }

    /// Advance to the next phase.
    pub async fn advance_phase(&self) -> Result<()> {
        let mut phase = self.current_phase.write().await;
        if let Some(next) = phase.next() {
            *phase = next;
        }
        Ok(())
    }

    /// Get a summary of the orchestrator state.
    ///
    /// Takes all six read locks concurrently via `tokio::join!` rather than
    /// awaiting them one at a time; a sequential chain would hold each
    /// guard across the wait for the next.
    pub async fn get_summary(&self) -> OrchestratorSummary {
        let (tasks, subdomains, urls, vulnerabilities, ports, services, current_phase) = tokio::join!(
            self.tasks.read(),
            self.subdomains.read(),
            self.urls.read(),
            self.vulnerabilities.read(),
            self.ports.read(),
            self.services.read(),
            self.current_phase.read(),
        );
        let current_phase = *current_phase;

        let total_tasks = tasks.len();
        let completed_tasks = tasks.values().filter(|t| t.completed).count();
        let pending_tasks = total_tasks - completed_tasks;

        OrchestratorSummary {
            target: self.target.clone(),
            current_phase,
            total_tasks,
            completed_tasks,
            pending_tasks,
            subdomains_found: subdomains.len(),
            urls_found: urls.len(),
            vulnerabilities_found: vulnerabilities.len(),
            ports_found: ports.len(),
            services_found: services.len(),
        }
    }

    /// Get all tasks that are ready to execute (all dependencies completed).
    pub async fn get_ready_tasks(&self) -> Vec<BountyTask> {
        let tasks = self.tasks.read().await;
        tasks
            .values()
            .filter(|t| {
                !t.completed
                    && t.dependencies
                        .iter()
                        .all(|dep| tasks.get(dep).is_some_and(|d| d.completed))
            })
            .cloned()
            .collect()
    }

    /// Execute all ready tasks in parallel, respecting dependencies.
    ///
    /// This is the core performance improvement: independent tasks run
    /// concurrently instead of sequentially. Tasks with dependencies wait
    /// for their prerequisites to complete.
    pub async fn execute_ready_tasks<F, Fut>(&self, executor: F) -> Vec<(String, Result<String>)>
    where
        F: Fn(&BountyTask) -> Fut,
        Fut: std::future::Future<Output = Result<String>>,
    {
        use futures::stream::{self, StreamExt};

        let ready = self.get_ready_tasks().await;
        if ready.is_empty() {
            return Vec::new();
        }

        let results: Vec<(String, Result<String>)> = stream::iter(ready)
            .map(|task| {
                let executor = &executor;
                async move {
                    let result = executor(&task).await;
                    (task.id.clone(), result)
                }
            })
            .buffer_unordered(8) // Max 8 concurrent tasks
            .collect()
            .await;

        // Mark completed tasks
        for (id, result) in &results {
            if result.is_ok() {
                let _ = self
                    .complete_task(id, result.as_ref().unwrap().clone())
                    .await;
            }
        }

        results
    }

    /// Execute all tasks for a phase in parallel, honouring declared
    /// dependencies.
    ///
    /// Tasks are run in dependency *waves*: everything whose prerequisites are
    /// already complete runs concurrently, then the wave is committed and the
    /// next one is computed. This used to run every task in the phase at once
    /// regardless of `dependencies`, so e.g. `surface_katana` and
    /// `surface_feroxbuster` — both declared as depending on `surface_httpx` —
    /// were launched before their prerequisite had produced any output.
    ///
    /// A task whose dependency failed can never become ready, so if a wave
    /// completes nothing new the remainder is reported as blocked rather than
    /// spun on.
    pub async fn execute_phase_tasks<F, Fut>(
        &self,
        phase: BountyPhase,
        executor: F,
    ) -> Vec<(String, Result<String>)>
    where
        F: Fn(&BountyTask) -> Fut,
        Fut: std::future::Future<Output = Result<String>>,
    {
        use futures::stream::{self, StreamExt};

        let mut all_results: Vec<(String, Result<String>)> = Vec::new();
        // `get_tasks_for_phase` returns completed tasks too, which would make
        // every wave empty on a resumed run and report already-finished work
        // as blocked. Seed only what is still outstanding.
        let mut remaining: Vec<String> = self
            .get_tasks_for_phase(phase)
            .await
            .into_iter()
            .filter(|t| !t.completed)
            .map(|t| t.id)
            .collect();

        while !remaining.is_empty() {
            let ready_ids = self.ready_subset(&remaining).await;
            if ready_ids.is_empty() {
                // Nothing in this phase can proceed; every remaining task is
                // waiting on a prerequisite that will never complete.
                tracing::warn!(
                    "Phase {:?}: {} tasks blocked by unmet dependencies: {:?}",
                    phase,
                    remaining.len(),
                    remaining
                );
                for id in remaining {
                    all_results.push((
                        id,
                        Err(anyhow::anyhow!("blocked by unmet task dependencies")),
                    ));
                }
                break;
            }

            let tasks: Vec<BountyTask> = {
                let all = self.tasks.read().await;
                ready_ids
                    .iter()
                    .filter_map(|id| all.get(id).cloned())
                    .collect()
            };

            let wave: Vec<(String, Result<String>)> = stream::iter(tasks)
                .map(|task| {
                    let executor = &executor;
                    async move {
                        let result = executor(&task).await;
                        (task.id.clone(), result)
                    }
                })
                .buffer_unordered(6) // Max 6 concurrent tasks per wave
                .collect()
                .await;

            for (id, result) in &wave {
                if result.is_ok() {
                    let _ = self
                        .complete_task(id, result.as_ref().unwrap().clone())
                        .await;
                }
            }

            let finished: Vec<&String> = wave.iter().map(|(id, _)| id).collect();
            remaining.retain(|id| !finished.contains(&id));
            all_results.extend(wave);
        }

        all_results
    }

    /// Subset of `candidate_ids` whose dependencies are all complete.
    async fn ready_subset(&self, candidate_ids: &[String]) -> Vec<String> {
        let tasks = self.tasks.read().await;
        candidate_ids
            .iter()
            .filter(|id| {
                tasks.get(*id).is_some_and(|task| {
                    !task.completed
                        && task
                            .dependencies
                            .iter()
                            .all(|dep| tasks.get(dep).is_some_and(|d| d.completed))
                })
            })
            .cloned()
            .collect()
    }

    /// Run the complete workflow with parallel execution.
    ///
    /// This is the main entry point for automated bug bounty assessments.
    /// It processes all phases in order, running independent tasks in parallel.
    pub async fn run_parallel_workflow<F, Fut>(&self, executor: F) -> Result<()>
    where
        F: Fn(&BountyTask) -> Fut,
        Fut: std::future::Future<Output = Result<String>>,
    {
        let phases = [
            BountyPhase::Reconnaissance,
            BountyPhase::AttackSurfaceMapping,
            BountyPhase::VulnerabilityDiscovery,
            BountyPhase::Exploitation,
            BountyPhase::Reporting,
        ];

        for phase in &phases {
            tracing::info!("Starting phase: {:?}", phase);
            let results = self.execute_phase_tasks(*phase, &executor).await;
            let success = results.iter().filter(|(_, r)| r.is_ok()).count();
            let failed = results.len() - success;
            tracing::info!(
                "Phase {:?} complete: {} succeeded, {} failed",
                phase,
                success,
                failed
            );
            self.advance_phase().await?;
        }

        Ok(())
    }
}

/// Summary of the orchestrator state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorSummary {
    /// The target domain or URL
    pub target: String,
    /// Current phase
    pub current_phase: BountyPhase,
    /// Total number of tasks
    pub total_tasks: usize,
    /// Number of completed tasks
    pub completed_tasks: usize,
    /// Number of pending tasks
    pub pending_tasks: usize,
    /// Number of subdomains found
    pub subdomains_found: usize,
    /// Number of URLs found
    pub urls_found: usize,
    /// Number of vulnerabilities found
    pub vulnerabilities_found: usize,
    /// Number of ports found
    pub ports_found: usize,
    /// Number of services found
    pub services_found: usize,
}

impl std::fmt::Display for OrchestratorSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Bug Bounty Orchestrator Summary")?;
        writeln!(f, "===============================")?;
        writeln!(f, "Target: {}", self.target)?;
        writeln!(f, "Phase: {:?}", self.current_phase)?;
        writeln!(
            f,
            "Tasks: {}/{} completed",
            self.completed_tasks, self.total_tasks
        )?;
        writeln!(f, "Subdomains: {}", self.subdomains_found)?;
        writeln!(f, "URLs: {}", self.urls_found)?;
        writeln!(f, "Vulnerabilities: {}", self.vulnerabilities_found)?;
        writeln!(f, "Ports: {}", self.ports_found)?;
        writeln!(f, "Services: {}", self.services_found)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_orchestrator_creation() {
        let orchestrator = BugBountyOrchestrator::new("example.com");
        assert_eq!(orchestrator.target, "example.com");
    }

    #[tokio::test]
    async fn test_orchestrator_initialize() {
        let orchestrator = BugBountyOrchestrator::new("example.com");
        orchestrator.initialize().await.unwrap();
        let tasks = orchestrator.get_pending_tasks().await;
        assert!(!tasks.is_empty());
    }

    #[tokio::test]
    async fn test_phase_advancement() {
        let orchestrator = BugBountyOrchestrator::new("example.com");
        let phase = orchestrator.get_current_phase().await;
        assert_eq!(phase, BountyPhase::Reconnaissance);
        orchestrator.advance_phase().await.unwrap();
        let phase = orchestrator.get_current_phase().await;
        assert_eq!(phase, BountyPhase::AttackSurfaceMapping);
    }

    #[tokio::test]
    async fn test_task_completion() {
        let orchestrator = BugBountyOrchestrator::new("example.com");
        orchestrator.initialize().await.unwrap();
        orchestrator
            .complete_task("recon_subfinder", "Found 10 subdomains".to_string())
            .await
            .unwrap();
        let completed = orchestrator.get_completed_tasks().await;
        assert_eq!(completed.len(), 1);
    }

    /// A typo'd or stale task id used to return `Ok(())`, which is
    /// indistinguishable from success — the CLI `?`s on this and cannot tell.
    #[tokio::test]
    async fn completing_an_unknown_task_is_an_error() {
        let orchestrator = BugBountyOrchestrator::new("example.com");
        orchestrator.initialize().await.unwrap();
        let err = orchestrator
            .complete_task("recon_subfindr", "typo'd id".to_string())
            .await
            .expect_err("unknown task must not report success");
        assert!(err.to_string().contains("unknown task"), "{err}");
        assert!(
            orchestrator.get_completed_tasks().await.is_empty(),
            "a failed completion must not mark anything done"
        );
    }

    fn task(id: &str, phase: BountyPhase, deps: &[&str]) -> BountyTask {
        BountyTask {
            id: id.to_string(),
            name: id.to_string(),
            description: String::new(),
            tool: "noop".to_string(),
            params: serde_json::json!({}),
            phase,
            completed: false,
            result: None,
            dependencies: deps.iter().map(|d| (*d).to_string()).collect(),
            priority: 1,
            estimated_duration: 0,
            retry_count: 0,
            max_retries: 0,
        }
    }

    /// `execute_phase_tasks` used to run every task in the phase at once,
    /// ignoring `dependencies`. This pins that a dependent task does not start
    /// before its prerequisite has completed.
    #[tokio::test]
    async fn phase_tasks_respect_declared_dependencies() {
        use std::sync::Arc;
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

        let orchestrator = BugBountyOrchestrator::new("example.com");
        let phase = BountyPhase::AttackSurfaceMapping;
        {
            let mut tasks = orchestrator.tasks.write().await;
            tasks.insert("root".into(), task("root", phase, &[]));
            tasks.insert("child".into(), task("child", phase, &["root"]));
        }

        // `root` yields for long enough that an unordered scheduler would
        // certainly start `child` first, so the flag distinguishes the two
        // behaviours deterministically rather than by luck.
        let root_finished = Arc::new(AtomicBool::new(false));
        let child_saw_root_done = Arc::new(AtomicUsize::new(0));
        let (finished, observed) = (Arc::clone(&root_finished), Arc::clone(&child_saw_root_done));

        let results = orchestrator
            .execute_phase_tasks(phase, move |t: &BountyTask| {
                let finished = Arc::clone(&finished);
                let observed = Arc::clone(&observed);
                let id = t.id.clone();
                async move {
                    if id == "root" {
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        finished.store(true, Ordering::SeqCst);
                    } else {
                        // Record the flag as observed, so 1 means "root was
                        // already finished" and 0 means "child ran too early".
                        observed.store(
                            usize::from(finished.load(Ordering::SeqCst)),
                            Ordering::SeqCst,
                        );
                    }
                    Ok::<String, anyhow::Error>(format!("ran {id}"))
                }
            })
            .await;

        assert_eq!(results.len(), 2, "both tasks should have run: {results:?}");
        assert_eq!(
            child_saw_root_done.load(Ordering::SeqCst),
            1,
            "`child` must not start until `root` has completed"
        );
        assert_eq!(orchestrator.get_completed_tasks().await.len(), 2);
    }

    /// A task whose dependency fails can never become ready. The wave loop must
    /// report it rather than spinning forever.
    #[tokio::test]
    async fn tasks_with_failed_prerequisites_are_reported_as_blocked() {
        let orchestrator = BugBountyOrchestrator::new("example.com");
        let phase = BountyPhase::VulnerabilityDiscovery;
        {
            let mut tasks = orchestrator.tasks.write().await;
            tasks.insert("parent".into(), task("parent", phase, &[]));
            tasks.insert("child".into(), task("child", phase, &["parent"]));
        }

        let results = orchestrator
            .execute_phase_tasks(phase, |t: &BountyTask| {
                // The task reference must not escape into the returned future:
                // `Fut` is a single concrete type, so an `async move` block that
                // captured `t` would tie the future to the caller's borrow
                // lifetime. Copy the id out first.
                let id = t.id.clone();
                async move {
                    if id == "parent" {
                        Err(anyhow::anyhow!("deliberate failure"))
                    } else {
                        Ok::<String, anyhow::Error>("unreachable".to_string())
                    }
                }
            })
            .await;

        assert_eq!(
            results.len(),
            2,
            "blocked task must still be reported: {results:?}"
        );
        let child = results.iter().find(|(id, _)| id == "child").expect("child");
        assert!(
            child.1.is_err(),
            "a task whose prerequisite failed must be reported as an error, not run"
        );
    }

    /// Re-running a phase must not report already-completed tasks as blocked.
    #[tokio::test]
    async fn a_resumed_phase_does_not_reflag_completed_tasks() {
        let orchestrator = BugBountyOrchestrator::new("example.com");
        let phase = BountyPhase::Reconnaissance;
        {
            let mut tasks = orchestrator.tasks.write().await;
            tasks.insert("already".into(), task("already", phase, &[]));
        }
        orchestrator
            .complete_task("already", "done earlier".to_string())
            .await
            .expect("task is registered");

        let results = orchestrator
            .execute_phase_tasks(phase, |_t: &BountyTask| async move {
                Ok::<String, anyhow::Error>("ok".to_string())
            })
            .await;

        assert!(
            results.is_empty(),
            "nothing was pending, so nothing should be reported: {results:?}"
        );
    }

    /// `get_summary` read five accumulators that nothing ever wrote, so every
    /// summary reported zeros regardless of what recon found.
    #[tokio::test]
    async fn completing_a_task_populates_the_summary() {
        let orchestrator = BugBountyOrchestrator::new("example.com");
        {
            let mut tasks = orchestrator.tasks.write().await;
            let mut t = task("recon", BountyPhase::Reconnaissance, &[]);
            t.tool = "subfinder".into();
            tasks.insert("recon".into(), t);
        }

        orchestrator
            .complete_task(
                "recon",
                "api.example.com\nadmin.example.com\nnot a hostname!!\n".to_string(),
            )
            .await
            .expect("registered");

        let summary = orchestrator.get_summary().await;
        assert_eq!(
            summary.subdomains_found, 2,
            "valid subdomains should be counted and the junk line dropped: {summary:?}"
        );
    }

    #[tokio::test]
    async fn summary_counts_each_discovery_kind() {
        let orchestrator = BugBountyOrchestrator::new("example.com");
        orchestrator
            .ingest_tool_output("naabu", "example.com:80\n443/tcp open https\nnotaport\n")
            .await;
        orchestrator
            .ingest_tool_output("httpx", "https://example.com:8443\n")
            .await;
        orchestrator
            .ingest_tool_output("gau", "https://example.com/a\nftp://example.com/b\n")
            .await;
        orchestrator
            .ingest_tool_output(
                "nuclei",
                "[high] id: exposed-admin-panel\nplain text line\n",
            )
            .await;

        let summary = orchestrator.get_summary().await;
        // naabu contributes 80 and 443 (the "443/tcp open https" form), httpx
        // contributes 8443.
        assert_eq!(summary.ports_found, 3, "80, 443 and 8443: {summary:?}");
        assert_eq!(
            summary.urls_found, 2,
            "the httpx URL and the gau URL: {summary:?}"
        );
        assert_eq!(
            summary.vulnerabilities_found, 1,
            "one marker line: {summary:?}"
        );
    }

    #[tokio::test]
    async fn host_and_port_parsing_handles_the_awkward_forms() {
        assert_eq!(
            split_host_port("https://example.com:8443/path?q=1"),
            Some(("example.com".to_string(), 8443))
        );
        assert_eq!(
            split_host_port("[::1]:8080"),
            Some(("[::1]".to_string(), 8080))
        );
        assert_eq!(split_host_port("example.com"), None);

        assert_eq!(parse_port("443/tcp open https"), Some(443));
        assert_eq!(parse_port("example.com:22"), Some(22));
        assert_eq!(parse_port("no ports here"), None);
        assert_eq!(parse_port("0"), None);

        assert!(is_hostname("api.example.com"));
        assert!(!is_hostname("not a hostname"));
        assert!(!is_hostname("localhost"));
        assert!(!is_hostname("http://example.com/x"));
    }
}
