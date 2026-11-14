//! Concrete execution of the bug-bounty task graph through first-party tools.
//!
//! [`BugBountyOrchestrator::execute_phase_tasks`] accepts an executor closure,
//! and until now the only "executor" wired into the CLI was a placeholder that
//! marked every task completed without running anything. This module is the
//! real one: it hydrates each task's parameters from the orchestrator's
//! accumulated state (subdomains, live hosts, discovered URLs), drives the
//! actual binaries via [`crate::alphacode_app_core::tool::bounty_bridge`],
//! persists raw output files, and feeds results back through `complete_task`
//! so the orchestrator's accumulators stay the single source of truth.
//!
//! Hydration is data-driven — a task's `params` are a *template* (they contain
//! empty target lists) and this module fills them from prior phases' results.
//! Every translation is explicit below so that a tool with a different
//! contract (dnsx wants `targets`, katana wants exactly one `url`, ffuf needs
//! a `wordlist` file) gets exactly what its schema says it needs.
//!
//! Durability: state is saved to `state.json` after every phase and restored
//! on `--resume`, so a crash mid-run continues where it stopped instead of
//! re-paying for the recon phases.

use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use crate::alphacode_app_core::bugbounty_orchestrator::{
    BountyPhase, BountyTask, BugBountyOrchestrator, TaskCoverage,
};
use crate::alphacode_app_core::tool::bounty_bridge::run_task_tool;
use crate::alphacode_tool_core::{ToolContext, ToolExecutionMode};

/// How many hosts the http probe fans over in one call.
const HTTPX_TARGET_CAP: usize = 150;
/// Seeds a crawl/content-discovery task fans out over. Each seed runs as a
/// separate bounded call, so this bounds a whole crawler task's worst case.
const CRAWLER_SEED_CAP: usize = 8;
/// Directory brute-force seeds (the wordlist is small, so a few more).
const DIR_SCAN_SEED_CAP: usize = 10;
/// URLs with parameters are what sqlmap should target; level-2/risk-2 calls
/// are expensive, so stay low.
const SQLMAP_URL_CAP: usize = 4;
/// dalfox per-seed calls are cheap and bounded; a dozen covers the tail.
const XSS_SEED_CAP: usize = 12;
/// nuclei accepts a target list, so one call can carry a wide sweep.
const NUCLEI_TARGET_CAP: usize = 120;
/// corsy / crlfuzz read URL lists directly.
const MISC_URL_CAP: usize = 60;
/// nikto per-host runtime (~minutes per host) is the largest of the lot.
const NIKTO_HOST_CAP: usize = 8;
/// gospider seeds.
const GOPIDER_SEED_CAP: usize = 12;
/// ffuf endpoints per call.
const FFUF_ENDPOINT_CAP: usize = 10;
/// Auth-matrix probing targets.
const MATRIX_TARGET_CAP: usize = 25;

/// Tools whose output is one URL per line in the standard layout.
///
/// For these, the text handed to `complete_task` (and therefore the
/// orchestrator's URL accumulators) is the cleaned URL token per line rather
/// than the raw decorated stdout — otherwise httpx metadata like
/// `[200] [Title]` would pollute every downstream seed list.
const URL_LINE_TOOLS: &[&str] = &[
    "httpx",
    "httprobe",
    "katana",
    "gau",
    "waybackurls",
    "gospider",
    "hakrawler",
    "ffuf",
];

// --- embedded micro-wordlists -----------------------------------------------
//
// ffuf ships no wordlist of its own, and gobuster/feroxbuster default to a
// Linux path that does not exist on Windows. To make the brute-force tasks
// actually run anywhere, the executor materialises these into the output dir
// once per run. They are deliberately small: ~100 requests per seed keeps the
// whole task inside its `estimated_duration` envelope on a normal target.

pub const PATH_WORDLIST: &[&str] = &[
    "admin",
    "administrator",
    "api",
    "api/v1",
    "api/v2",
    "app",
    "apps",
    "assets",
    "auth",
    "backup",
    "backups",
    "blog",
    "cgi-bin",
    "config",
    "console",
    "dashboard",
    "db",
    "debug",
    "dev",
    "docs",
    "download",
    "downloads",
    "editor",
    ".env",
    ".env.bak",
    ".git/config",
    ".git/HEAD",
    "graph",
    "graphql",
    "health",
    "help",
    "home",
    "images",
    "img",
    "install",
    "internal",
    ".DS_Store",
    "js",
    "legacy",
    "login",
    "logout",
    "log",
    "logs",
    "main",
    "manage",
    "manager",
    "media",
    "member",
    "members",
    "metrics",
    ".htaccess",
    "new",
    "old",
    "panel",
    "phpinfo.php",
    "portal",
    "private",
    "profile",
    "public",
    "register",
    "release",
    "report",
    "reports",
    "repository",
    "reset",
    "resources",
    "restore",
    "robots.txt",
    "search",
    "secret",
    "secrets",
    "secure",
    "server-status",
    "service",
    "services",
    "settings",
    "setup",
    "signin",
    "signup",
    "site",
    "sitemap.xml",
    "sources",
    "sql",
    "src",
    "static",
    "status",
    "storage",
    "staging",
    "swagger",
    "swagger.json",
    "swagger/ui",
    "sys",
    "system",
    "temp",
    "template",
    "templates",
    "test",
    "tests",
    "theme",
    "themes",
    "tmp",
    "token",
    "tools",
    "upload",
    "uploads",
    "user",
    "users",
    "v1",
    "v2",
    "vendor",
    "version",
    "wp-admin",
    "wp-config.php",
    "wp-content",
    "wp-login.php",
    "xmlrpc.php",
];

/// Parameter names for the ffuf parameter-discovery task.
pub const PARAM_WORDLIST: &[&str] = &[
    "id", "user", "username", "name", "q", "s", "search", "query", "page", "p", "cat", "category",
    "type", "action", "cmd", "command", "email", "password", "pass", "token", "key", "debug",
    "test", "view", "file", "filename", "path", "dir", "lang", "redirect", "return", "url", "next",
    "callback", "ref", "continue", "domain", "host", "ip", "account", "admin", "role", "group",
    "sort", "order", "limit", "offset", "filter", "status", "date", "from", "to", "start", "end",
    "time", "size", "count", "format", "output", "export", "download",
];

// --- state persistence ---------------------------------------------------------

/// What the executor writes between phases and restores on `--resume`.
///
/// This is the whole orchestrator state minus the evidence structures: the
/// verified-finding gate (`record_verified_finding`) cross-checks proofs
/// against exchanges recorded in the same process, so persisting half of that
/// relationship would silently loosen the gate. Resumed historical scanner
/// output remains unverified; completed tasks are not automatically re-run to
/// recreate proof. Fresh observations must pass the normal evidence gate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorSnapshot {
    #[serde(default)]
    pub intelligence: crate::alphacode_security_core::investigation::InvestigationIntelligence,
    pub target: String,
    pub current_phase: BountyPhase,
    pub tasks: BTreeMap<String, BountyTask>,
    pub subdomains: BTreeSet<String>,
    pub urls: BTreeSet<String>,
    pub ports: BTreeSet<u16>,
    pub services: BTreeMap<String, String>,
    pub vulnerabilities: BTreeMap<String, String>,
    #[serde(default)]
    pub execution_coverage: Vec<TaskCoverage>,
}

const STATE_FILE: &str = "state.json";

/// Atomically write `bytes` to `path` (temp file + rename) so a crash cannot
/// leave a truncated `state.json` behind.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(parent).map_err(|e| {
        anyhow!(
            "failed to create temporary file in {}: {e}",
            parent.display()
        )
    })?;
    tmp.write_all(bytes)
        .map_err(|e| anyhow!("failed to write {}: {e}", path.display()))?;
    tmp.as_file()
        .sync_all()
        .map_err(|e| anyhow!("failed to sync {}: {e}", path.display()))?;
    // Persist replaces the destination without deleting the previous snapshot
    // first. Unique temporary names also avoid collisions between task writers.
    tmp.persist(path)
        .map_err(|e| anyhow!("failed to replace {}: {}", path.display(), e.error))?;
    Ok(())
}

/// Serialize the orchestrator's accumulators and task graph to `dir/state.json`.
pub async fn save_state(orchestrator: &BugBountyOrchestrator, dir: &Path) -> Result<()> {
    let snapshot = OrchestratorSnapshot {
        intelligence: orchestrator.intelligence.read().await.clone(),
        target: orchestrator.target.clone(),
        current_phase: orchestrator.get_current_phase().await,
        tasks: orchestrator
            .tasks
            .read()
            .await
            .clone()
            .into_iter()
            .collect(),
        subdomains: orchestrator
            .subdomains
            .read()
            .await
            .iter()
            .cloned()
            .collect(),
        urls: orchestrator.urls.read().await.iter().cloned().collect(),
        ports: orchestrator.ports.read().await.iter().copied().collect(),
        services: orchestrator
            .services
            .read()
            .await
            .clone()
            .into_iter()
            .collect(),
        execution_coverage: orchestrator.execution_coverage.read().await.clone(),
        vulnerabilities: orchestrator
            .vulnerabilities
            .read()
            .await
            .clone()
            .into_iter()
            .collect(),
    };
    let bytes = serde_json::to_vec_pretty(&snapshot)?;
    atomic_write(&dir.join(STATE_FILE), &bytes)
}

/// Restore a snapshot written by [`save_state`].
///
/// Returns `Ok(false)` when no state file exists (fresh run) and `Ok(true)`
/// when one was loaded. A state file belonging to a different target is
/// refused rather than merged silently.
pub async fn load_state(orchestrator: &BugBountyOrchestrator, dir: &Path) -> Result<bool> {
    let path = dir.join(STATE_FILE);
    if !path.exists() {
        return Ok(false);
    }
    let bytes =
        std::fs::read(&path).map_err(|e| anyhow!("failed to read {}: {e}", path.display()))?;
    let snapshot: OrchestratorSnapshot =
        serde_json::from_slice(&bytes).map_err(|e| anyhow!("state file is corrupt: {e}"))?;
    if snapshot.target != orchestrator.target {
        return Err(anyhow!(
            "state file belongs to target `{}`, not `{}`",
            snapshot.target,
            orchestrator.target
        ));
    }

    snapshot
        .intelligence
        .check_integrity()
        .map_err(|err| anyhow!("invalid investigation snapshot: {err}"))?;
    *orchestrator.intelligence.write().await = snapshot.intelligence;
    *orchestrator.current_phase.write().await = snapshot.current_phase;
    *orchestrator.tasks.write().await = snapshot.tasks.into_iter().collect();
    *orchestrator.subdomains.write().await = snapshot.subdomains.into_iter().collect();
    *orchestrator.urls.write().await = snapshot.urls.into_iter().collect();
    *orchestrator.ports.write().await = snapshot.ports.into_iter().collect();
    *orchestrator.services.write().await = snapshot.services.into_iter().collect();
    *orchestrator.vulnerabilities.write().await = snapshot.vulnerabilities.into_iter().collect();
    *orchestrator.execution_coverage.write().await = snapshot.execution_coverage;
    Ok(true)
}

/// Reconcile per-call crash journals with the last phase snapshot. Disk
/// journals can be newer, and retain an in-flight call as indeterminate.
async fn restore_execution_audits(orchestrator: &BugBountyOrchestrator, dir: &Path) -> Result<()> {
    let audit_dir = dir.join("coverage");
    if !audit_dir.exists() {
        return Ok(());
    }
    let mut audits: BTreeMap<String, TaskCoverage> = orchestrator
        .execution_coverage
        .read()
        .await
        .iter()
        .cloned()
        .map(|entry| (entry.input_manifest.clone(), entry))
        .collect();
    for entry in std::fs::read_dir(&audit_dir)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json")
            || name.ends_with(".inputs.json")
        {
            continue;
        }
        let audit: TaskCoverage = serde_json::from_slice(&std::fs::read(&path)?)
            .map_err(|err| anyhow!("invalid execution audit {}: {err}", path.display()))?;
        if audit.target == orchestrator.target {
            audits.insert(audit.input_manifest.clone(), audit);
        }
    }
    *orchestrator.execution_coverage.write().await = audits.into_values().collect();
    Ok(())
}

// --- target hydration ----------------------------------------------------------

/// A URL recovered from prior task output, trimmed to the address itself.
///
/// httpx decorates lines (`https://host [200] [Title]`) and crawler output can
/// carry annotations. Taking the first whitespace-delimited token keeps the
/// round-trip honest: downstream tools receive a URL they can actually use.
fn clean_url_line(line: &str) -> Option<String> {
    // `split_whitespace` already skips surrounding whitespace; a leading
    // trim here is redundant and clippy-flagged.
    let token = line.split_whitespace().next()?;
    if token.starts_with("http://") || token.starts_with("https://") {
        Some(token.trim_end_matches(',').to_string())
    } else {
        None
    }
}

/// The target's root URL, used as the fallback seed when nothing else ran.
fn target_root(target: &str) -> String {
    let trimmed = target.trim();
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    }
}

/// Everything the hydration step needs about what previous phases found.
#[derive(Debug, Default, Clone)]
struct Seeds {
    /// Live HTTP URLs, from the http-probe task's result first.
    live: Vec<String>,
    /// URLs carrying query parameters, prioritized for injection work.
    with_params: Vec<String>,
    /// Every URL seen, deduplicated.
    all_urls: Vec<String>,
}

/// Build the seed set from completed task results and the URL accumulator.
async fn collect_seeds(orchestrator: &BugBountyOrchestrator) -> Seeds {
    let mut live = Vec::new();
    let mut with_params = Vec::new();
    let mut all_urls = Vec::new();
    let mut seen_live = HashSet::new();
    let mut seen_params = HashSet::new();
    let mut seen_all = HashSet::new();
    let tasks = orchestrator.tasks.read().await;
    let mut results: Vec<_> = tasks
        .values()
        .filter(|task| task.completed && task.result.is_some())
        .collect();
    // Stable priority and task identity determine capped selection, never the
    // randomized iteration order of a HashMap.
    results.sort_by(|a, b| {
        let a_live = matches!(a.tool.as_str(), "httpx" | "httprobe");
        let b_live = matches!(b.tool.as_str(), "httpx" | "httprobe");
        b_live
            .cmp(&a_live)
            .then_with(|| a.priority.cmp(&b.priority))
            .then_with(|| a.id.cmp(&b.id))
    });
    for task in results {
        let only_live = matches!(task.tool.as_str(), "httpx" | "httprobe");
        for line in task.result.as_deref().unwrap_or_default().lines() {
            if let Some(url) = clean_url_line(line) {
                if url.contains('?') && seen_params.insert(url.clone()) {
                    with_params.push(url.clone());
                }
                if only_live && seen_live.insert(url.clone()) {
                    live.push(url.clone());
                }
                if seen_all.insert(url.clone()) {
                    all_urls.push(url);
                }
            }
        }
    }
    drop(tasks);
    let mut discovered: Vec<_> = orchestrator.urls.read().await.iter().cloned().collect();
    discovered.sort();
    for item in discovered {
        if let Some(url) = clean_url_line(&item) {
            if url.contains('?') && seen_params.insert(url.clone()) {
                with_params.push(url.clone());
            }
            if seen_all.insert(url.clone()) {
                all_urls.push(url);
            }
        }
    }
    if all_urls.is_empty() {
        all_urls.push(target_root(&orchestrator.target));
    }
    Seeds {
        live,
        with_params,
        all_urls,
    }
}

/// Dedup + cap helper preserving discovery order.
fn cap_unique(items: &[&String], cap: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        if out.len() >= cap {
            break;
        }
        if seen.insert(item.as_str()) {
            out.push((*item).clone());
        }
    }
    out
}

/// Odd but common shape: most of these seeds passed as `Vec<String>`.
fn cap_strings(items: Vec<String>, cap: usize) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        if out.len() >= cap {
            break;
        }
        if seen.insert(item.clone()) {
            out.push(item);
        }
    }
    out
}

/// Materialise an embedded wordlist into the output dir (once per run).
fn materialise_wordlist(dir: &Path, name: &str, entries: &[&str]) -> Result<PathBuf> {
    let path = dir.join(name);
    if !path.exists() {
        atomic_write(&path, entries.join("\n").as_bytes())?;
    }
    Ok(path)
}

/// Insert a per-call timeout override when the tool models one; the
/// orchestrator's `estimated_duration` reflects the whole task, and a bounded
/// call keeps each seed inside that envelope.
fn seed_call_timeout(obj: &mut serde_json::Map<String, Value>, secs: u64) {
    obj.entry("timeout".to_string())
        .or_insert_with(|| serde_json::json!(secs));
}

/// Build the concrete tool-call inputs for a task.
///
/// Returns one Value per invocation. Single-URL tools (katana, sqlmap,
/// gobuster, …) fan out over a capped seed list; list-tolerant tools (httpx,
/// nuclei, corsy, …) get one call carrying the hydrated list.
async fn build_tool_inputs(
    task: &BountyTask,
    orchestrator: &BugBountyOrchestrator,
    dir: &Path,
    seeds: &Seeds,
) -> Result<Vec<Value>> {
    let target = orchestrator.target.trim().to_string();
    let root = target_root(&target);
    let mut params = task.params.clone();

    // The orchestrator's `status_codes: true` would be rejected by httpx's
    // deserializer (it models an enum string); give it a real code list.
    if let Some(obj) = params.as_object_mut()
        && obj.get("status_codes").is_some_and(Value::is_boolean)
    {
        obj.insert(
            "status_codes".to_string(),
            Value::String("200,204,301,302,400,401,403,404,405,500,502,503".to_string()),
        );
    }

    match (task.tool.as_str(), task.id.as_str()) {
        ("dnsx", _) => {
            // dnsx has no `-d`/domain flag in this schema: targets arrive as
            // `-d comma,list`.
            if let Some(obj) = params.as_object_mut() {
                obj.remove("domain");
                obj.insert(
                    "targets".to_string(),
                    Value::Array(vec![Value::String(target.clone())]),
                );
            }
            Ok(vec![params])
        }
        ("httpx", "matrix_auth_states") => {
            // Endpoints first (where auth differentials matter), live hosts
            // second. The credential side of the matrix needs configured
            // credentials — none exist here — so this call records the
            // unauthenticated state explicitly as such.
            let mut seeds_vec = collect_all_urls(seeds);
            seeds_vec.truncate(MATRIX_TARGET_CAP);
            if seeds_vec.is_empty() {
                return Err(anyhow!(
                    "no endpoints discovered yet; the auth matrix needs the surface phase first"
                ));
            }
            if let Some(obj) = params.as_object_mut() {
                obj.remove("url");
                obj.remove("auth_matrix");
                obj.insert(
                    "targets".to_string(),
                    Value::Array(seeds_vec.into_iter().map(Value::String).collect()),
                );
            }
            Ok(vec![params])
        }
        ("httpx", _) => {
            // httpx is the task that produces the live list. Seed it with the
            // discovered subdomains so the probe is not a one-URL stub.
            let mut listed = substrate_subdomains(orchestrator).await;
            listed.extend(seeds.live.iter().cloned());
            let listed = cap_strings(listed, HTTPX_TARGET_CAP);
            if listed.is_empty() {
                return Err(anyhow!(
                    "no targets for httpx — reconnaissance found nothing"
                ));
            }
            if let Some(obj) = params.as_object_mut() {
                obj.remove("targets");
                obj.insert(
                    "targets".to_string(),
                    Value::Array(listed.into_iter().map(Value::String).collect()),
                );
            }
            Ok(vec![params])
        }
        ("httprobe", _) => {
            if let Some(obj) = params.as_object_mut() {
                obj.remove("targets");
                obj.insert(
                    "targets".to_string(),
                    Value::Array(
                        substrate_subdomains(orchestrator)
                            .await
                            .into_iter()
                            .map(Value::String)
                            .collect(),
                    ),
                );
            }
            Ok(vec![params])
        }
        ("katana", _) => {
            // katana takes exactly one seed URL per call.
            let mut seed_list = cap_strings(collect_all_urls(seeds), CRAWLER_SEED_CAP);
            if seed_list.is_empty() {
                seed_list.push(root.clone());
            }
            let mut inputs = Vec::new();
            for url in seed_list {
                let mut call = params.clone();
                if let Some(obj) = call.as_object_mut() {
                    obj.remove("url");
                    obj.remove("js_crawl"); // not a katana flag in this schema
                    obj.insert("url".to_string(), Value::String(url));
                    seed_call_timeout(obj, 60);
                }
                inputs.push(call);
            }
            Ok(inputs)
        }
        ("gau", _) | ("waybackurls", _) => {
            if let Some(obj) = params.as_object_mut() {
                obj.entry("domain".to_string())
                    .or_insert(Value::String(target.clone()));
            }
            Ok(vec![params])
        }
        ("feroxbuster", _) => {
            let wordlist = materialise_wordlist(dir, "wordlist_paths.txt", PATH_WORDLIST)?;
            let mut seed_list = cap_strings(collect_all_urls(seeds), CRAWLER_SEED_CAP);
            if seed_list.is_empty() {
                seed_list.push(root.clone());
            }
            let mut inputs = Vec::new();
            for url in seed_list {
                let mut call = params.clone();
                if let Some(obj) = call.as_object_mut() {
                    obj.remove("url");
                    obj.insert("url".to_string(), Value::String(url));
                    obj.insert(
                        "wordlist".to_string(),
                        Value::String(wordlist.display().to_string()),
                    );
                    obj.insert("threads".to_string(), serde_json::json!(20));
                    seed_call_timeout(obj, 30);
                }
                inputs.push(call);
            }
            Ok(inputs)
        }
        ("gobuster", _) => {
            let wordlist = materialise_wordlist(dir, "wordlist_paths.txt", PATH_WORDLIST)?;
            let mut seed_list = cap_strings(collect_all_urls(seeds), DIR_SCAN_SEED_CAP);
            if seed_list.is_empty() {
                seed_list.push(root.clone());
            }
            let mut inputs = Vec::new();
            for url in seed_list {
                let mut call = params.clone();
                if let Some(obj) = call.as_object_mut() {
                    obj.remove("url");
                    obj.insert("url".to_string(), Value::String(url));
                    obj.insert(
                        "wordlist".to_string(),
                        Value::String(wordlist.display().to_string()),
                    );
                    obj.insert("threads".to_string(), serde_json::json!(20));
                }
                inputs.push(call);
            }
            Ok(inputs)
        }
        ("nuclei", _) => {
            let listed = cap_unique(
                &seeds
                    .live
                    .iter()
                    .chain(seeds.all_urls.iter())
                    .collect::<Vec<_>>(),
                NUCLEI_TARGET_CAP,
            );
            if listed.is_empty() {
                return Err(anyhow!(
                    "no live hosts to scan; run the surface phase first"
                ));
            }
            if let Some(obj) = params.as_object_mut() {
                obj.remove("target");
                obj.insert(
                    "target".to_string(),
                    Value::Array(listed.into_iter().map(Value::String).collect()),
                );
            }
            Ok(vec![params])
        }
        ("nikto", _) => {
            let mut seed_list = match seeds.live.is_empty() {
                true => vec![root.clone()],
                false => cap_strings(seeds.live.clone(), NIKTO_HOST_CAP),
            };
            if seed_list.is_empty() {
                seed_list.push(root.clone());
            }
            let inputs = seed_list
                .into_iter()
                .map(|url| {
                    let mut call = serde_json::json!({ "url": url });
                    seed_call_timeout(call.as_object_mut().expect("object"), 120);
                    call
                })
                .collect();
            Ok(inputs)
        }
        ("dalfox", _) => {
            // XSS needs URLs with parameters when there are any; live hosts
            // are the honest fallback (dalfox will crawl the one seed).
            let base = if !seeds.with_params.is_empty() {
                seeds.with_params.clone()
            } else if !seeds.live.is_empty() {
                seeds.live.clone()
            } else {
                vec![root.clone()]
            };
            let inputs = cap_strings(base, XSS_SEED_CAP)
                .into_iter()
                .map(|url| {
                    let mut call = serde_json::json!({ "url": url });
                    seed_call_timeout(call.as_object_mut().expect("object"), 30);
                    call
                })
                .collect();
            Ok(inputs)
        }
        ("corsy", _) | ("crlfuzz", _) => {
            let urls = cap_strings(collect_all_urls(seeds), MISC_URL_CAP);
            if urls.is_empty() {
                return Err(anyhow!("no URLs discovered yet"));
            }
            if let Some(obj) = params.as_object_mut() {
                obj.remove("url");
                obj.insert(
                    "url".to_string(),
                    Value::Array(urls.into_iter().map(Value::String).collect()),
                );
            }
            Ok(vec![params])
        }
        ("sqlmap", _) => {
            let base = if !seeds.with_params.is_empty() {
                seeds.with_params.clone()
            } else if !seeds.live.is_empty() {
                seeds.live.clone()
            } else {
                vec![root.clone()]
            };
            let mut inputs = Vec::new();
            for url in cap_strings(base, SQLMAP_URL_CAP) {
                let mut call = params.clone();
                if let Some(obj) = call.as_object_mut() {
                    obj.remove("url");
                    obj.insert("url".to_string(), Value::String(url));
                }
                inputs.push(call);
            }
            Ok(inputs)
        }
        ("gospider", _) => {
            let urls = cap_strings(collect_all_urls(seeds), GOPIDER_SEED_CAP);
            if urls.is_empty() {
                return Err(anyhow!(
                    "no URLs discovered yet; the surface phase has not run"
                ));
            }
            if let Some(obj) = params.as_object_mut() {
                obj.remove("url");
                // gospider's schema flags JS source parsing as `jsluice`.
                obj.remove("js");
                obj.insert("jsluice".to_string(), Value::Bool(true));
                obj.insert(
                    "url".to_string(),
                    Value::Array(urls.into_iter().map(Value::String).collect()),
                );
            }
            Ok(vec![params])
        }
        ("ffuf", "surface_ffuf_params") => {
            let wordlist = materialise_wordlist(dir, "wordlist_params.txt", PARAM_WORDLIST)?;
            let mut seed_list = match seeds.live.is_empty() {
                true => vec![root.clone()],
                false => cap_strings(seeds.live.clone(), FFUF_ENDPOINT_CAP),
            };
            if seed_list.is_empty() {
                seed_list.push(root.clone());
            }
            let mut inputs = Vec::new();
            for url in seed_list {
                let mut call = params.clone();
                if let Some(obj) = call.as_object_mut() {
                    obj.remove("url");
                    // The orchestrator's matcher/sizes template fields are not
                    // part of ffuf's tool schema; drop them rather than feed
                    // deserializers types they did not declare.
                    obj.remove("matcher");
                    obj.remove("sizes");
                    obj.insert("url".to_string(), Value::String(format!("{url}/?FUZZ=1")));
                    obj.insert(
                        "wordlist".to_string(),
                        Value::String(wordlist.display().to_string()),
                    );
                    obj.insert("threads".to_string(), serde_json::json!(20));
                    seed_call_timeout(obj, 30);
                }
                inputs.push(call);
            }
            Ok(inputs)
        }
        ("ffuf", _) => {
            // Value-fuzzing: put FUZZ where a path segment goes, on live hosts
            // (or known endpoints when parameter fuzzing already ran).
            let wordlist = materialise_wordlist(dir, "wordlist_paths.txt", PATH_WORDLIST)?;
            let mut seed_list = if !seeds.with_params.is_empty() {
                cap_strings(seeds.with_params.clone(), FFUF_ENDPOINT_CAP)
            } else {
                cap_strings(seeds.live.clone(), FFUF_ENDPOINT_CAP)
            };
            if seed_list.is_empty() {
                seed_list.push(root.clone());
            }
            let mut inputs = Vec::new();
            for url in seed_list {
                let mut call = params.clone();
                if let Some(obj) = call.as_object_mut() {
                    obj.remove("url");
                    // The orchestrator's matcher/sizes template fields are not
                    // part of ffuf's tool schema; drop them.
                    obj.remove("matcher");
                    obj.remove("sizes");
                    obj.insert("url".to_string(), Value::String(format!("{url}/FUZZ")));
                    obj.insert(
                        "wordlist".to_string(),
                        Value::String(wordlist.display().to_string()),
                    );
                    obj.insert("threads".to_string(), serde_json::json!(20));
                    seed_call_timeout(obj, 30);
                }
                inputs.push(call);
            }
            Ok(inputs)
        }
        _ => Ok(vec![params]),
    }
}

/// Merge every URL source into one deduplicated candidate list.
fn collect_all_urls(seeds: &Seeds) -> Vec<String> {
    let mut seen = HashSet::new();
    seeds
        .live
        .iter()
        .chain(&seeds.all_urls)
        .filter(|url| seen.insert(url.as_str()))
        .cloned()
        .collect()
}

/// Deterministic fallback seed selection; discovery does not establish scope.
async fn substrate_subdomains(orchestrator: &BugBountyOrchestrator) -> Vec<String> {
    let root = target_root(&orchestrator.target);
    let mut out = vec![root.clone()];
    let mut seen = HashSet::from([root]);
    let mut subs: Vec<_> = orchestrator
        .subdomains
        .read()
        .await
        .iter()
        .cloned()
        .collect();
    subs.sort();
    for sub in subs {
        if out.len() >= HTTPX_TARGET_CAP {
            break;
        }
        if !sub.is_empty() {
            let url = format!("https://{sub}");
            if seen.insert(url.clone()) {
                out.push(url);
            }
        }
    }
    out
}

// --- execution -----------------------------------------------------------------

/// One tool execution outcome for the run report.
#[derive(Debug)]
pub struct TaskOutcome {
    pub task_id: String,
    pub ok: bool,
    pub note: String,
}

fn truncate_utf8_bytes(text: &mut String, max_bytes: usize) {
    if text.len() > max_bytes {
        let mut end = max_bytes;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        text.truncate(end);
    }
}

/// Execute one orchestrator task: hydrate its parameters from the current
/// state, run the tool against every hydrated input, persist raw output, and
/// return the text `complete_task` should ingest.
///
/// The returned string is deliberately shaped for
/// `ingest_tool_output`: a bounded aggregate is saved to `<task_id>.txt`,
/// per-call returned output is retained under `coverage/`, and URL-class
/// ingest text carries one cleaned URL per line. Tool capture limits apply.
async fn execute_task(
    task: &BountyTask,
    orchestrator: &BugBountyOrchestrator,
    ctx: &ToolContext,
    dir: &Path,
) -> Result<String> {
    let interrupted = {
        let coverage = orchestrator.execution_coverage.read().await;
        coverage
            .iter()
            .find(|attempt| {
                attempt.target == orchestrator.target
                    && attempt.task_id == task.id
                    && attempt.in_flight_input_index.is_some()
            })
            .map(|attempt| {
                (
                    attempt.in_flight_input_index,
                    attempt.input_manifest.clone(),
                )
            })
    };
    if let Some((index, manifest)) = interrupted {
        return Err(anyhow!(
            "{}: prior input {:?} has an indeterminate outcome; inspect actual state and resolve its coverage journal before replay; manifest: {manifest}",
            task.id,
            index
        ));
    }
    let seeds = collect_seeds(orchestrator).await;
    let inputs = build_tool_inputs(task, orchestrator, dir, &seeds).await?;
    if inputs.is_empty() {
        return Err(anyhow!(
            "no {} inputs hydrated — run the prerequisite phase first",
            task.tool
        ));
    }

    use sha2::{Digest, Sha256};
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let identity = format!("{:x}", Sha256::digest(task.id.as_bytes()));
    let prefix = format!("{}-{stamp}", &identity[..16]);
    let audit_dir = dir.join("coverage");
    std::fs::create_dir_all(&audit_dir)?;
    let manifest_name = format!("coverage/{prefix}.inputs.json");
    atomic_write(
        &dir.join(&manifest_name),
        &serde_json::to_vec_pretty(&inputs)?,
    )?;
    let audit_path = audit_dir.join(format!("{prefix}.json"));
    let mut coverage = TaskCoverage {
        target: orchestrator.target.clone(),
        task_id: task.id.clone(),
        tool: task.tool.clone(),
        inputs_planned: inputs.len(),
        inputs_attempted: 0,
        successful_calls: 0,
        in_flight_input_index: None,
        failed_input_indices: Vec::new(),
        deferred_input_indices: (0..inputs.len()).collect(),
        omitted_observation_lines: 0,
        aggregate_output_truncated: false,
        stop_reason: "execution interrupted before completion".to_string(),
        input_manifest: manifest_name,
        raw_artifacts: Vec::new(),
    };
    // The audit exists before the first external call, so an interrupted run
    // leaves an explicit unresolved attempt rather than silently losing it.
    atomic_write(&audit_path, &serde_json::to_vec_pretty(&coverage)?)?;
    let is_url_tool = URL_LINE_TOOLS.contains(&task.tool.as_str());
    let mut raw_text = String::new();
    let mut lines = Vec::new();
    let mut seen_lines = HashSet::new();
    let mut consecutive_failures = 0u32;
    let mut stop_reason = "all hydrated calls attempted".to_string();
    for (i, input) in inputs.iter().enumerate() {
        coverage.inputs_attempted = i + 1;
        coverage.in_flight_input_index = Some(i);
        coverage.deferred_input_indices = (i + 1..inputs.len()).collect();
        atomic_write(&audit_path, &serde_json::to_vec_pretty(&coverage)?)?;
        let call_ctx = ToolContext {
            session_id: ctx.session_id.clone(),
            message_id: ctx.message_id.clone(),
            tool_call_id: format!("{}-{prefix}-{i}", ctx.tool_call_id),
            working_dir: ctx.working_dir.clone(),
            stdin_request_tx: None,
            graceful_shutdown_signal: None,
            execution_mode: ToolExecutionMode::Direct,
        };
        let (output, failed) = match run_task_tool(&task.tool, input.clone(), call_ctx).await {
            Ok(out) => (out.output, false),
            Err(err) => (format!("[error] {err}"), true),
        };
        // Preserve the returned transcript separately before creating a bounded
        // aggregate. Tool-internal output limits still apply.
        let artifact = format!("coverage/{prefix}-{i}.txt");
        atomic_write(&dir.join(&artifact), output.as_bytes())?;
        coverage.raw_artifacts.push(artifact);
        coverage.in_flight_input_index = None;
        raw_text.push_str(&output);
        raw_text.push('\n');
        if failed {
            coverage.failed_input_indices.push(i);
            consecutive_failures += 1;
        } else {
            coverage.successful_calls += 1;
            consecutive_failures = 0;
            for line in output.lines() {
                let text = if is_url_tool {
                    clean_url_line(line)
                } else {
                    let line = line.trim();
                    (!line.is_empty()).then(|| line.to_string())
                };
                if let Some(text) = text
                    && !seen_lines.contains(&text)
                {
                    if lines.len() < 2_000 {
                        seen_lines.insert(text.clone());
                        lines.push(text);
                    } else {
                        coverage.omitted_observation_lines += 1;
                    }
                }
            }
        }
        if raw_text.len() > 256 * 1024 {
            coverage.aggregate_output_truncated = true;
        }
        truncate_utf8_bytes(&mut raw_text, 256 * 1024);
        let binary_missing = failed && output.contains("failed to run ");
        let should_stop = binary_missing || consecutive_failures >= 2 || lines.len() >= 2_000;
        if should_stop {
            stop_reason = if binary_missing {
                "tool could not start"
            } else if consecutive_failures >= 2 {
                "consecutive tool failures"
            } else {
                "observation budget reached"
            }
            .to_string();
            coverage.stop_reason = stop_reason.clone();
        }
        atomic_write(&audit_path, &serde_json::to_vec_pretty(&coverage)?)?;
        if should_stop {
            break;
        }
    }
    coverage.stop_reason = stop_reason;
    atomic_write(&audit_path, &serde_json::to_vec_pretty(&coverage)?)?;
    let partial = coverage.is_partial();
    let successes = coverage.successful_calls;
    let failures = coverage.failed_input_indices.len();
    let deferred = coverage.deferred_input_indices.len();
    orchestrator.execution_coverage.write().await.push(coverage);
    let raw_path = dir.join(format!("{}.txt", task.id));
    atomic_write(&raw_path, raw_text.as_bytes())?;
    if successes == 0 {
        return Err(anyhow!(
            "{}: all attempted executions failed; {deferred} hydrated calls deferred; audit: {}",
            task.tool,
            audit_path.display()
        ));
    }
    let ingest_text = lines.join("\n");
    let notice = if partial {
        format!(
            "\n[coverage] PARTIAL: {successes} calls succeeded, {failures} failed, {deferred} deferred; audit: {}",
            audit_path.display()
        )
    } else {
        String::new()
    };
    Ok(format!("{ingest_text}{notice}"))
}

/// Run the complete workflow against `output_dir`, phase by phase.
///
/// `resume` restores prior phases from `state.json` (skipping any task
/// already recorded as completed) before executing what remains. Every phase
/// boundary saves state, so a crash anywhere after phase one resumes at the
/// matching phase without re-running earlier work.
pub async fn run_workflow(
    orchestrator: &BugBountyOrchestrator,
    dir: &Path,
    resume: bool,
) -> Result<Vec<TaskOutcome>> {
    std::fs::create_dir_all(dir)
        .map_err(|e| anyhow!("failed to create output directory {}: {e}", dir.display()))?;

    if resume {
        match load_state(orchestrator, dir).await {
            Ok(true) => {}
            Ok(false) => {
                // No prior state: initialize from the task graph below.
                orchestrator.initialize().await?;
            }
            Err(err) => {
                // A corrupt snapshot should not silently restart the whole
                // hunt — surface it so the caller can choose to rerun fresh.
                return Err(err);
            }
        }
    } else {
        orchestrator.initialize().await?;
    };
    restore_execution_audits(orchestrator, dir).await?;
    // Every later phase re-reads live state; a restored run simply sees most
    // tasks already completed and executes only what remains.

    use sha2::{Digest, Sha256};
    let session_id = format!(
        "bounty:{:x}",
        Sha256::digest(format!(
            "{}:{}",
            orchestrator.target,
            std::fs::canonicalize(dir)?.display()
        ))
    );
    crate::engagement_runtime::ensure_started(
        &session_id,
        &format!("Assess {}", orchestrator.target),
    )?;
    let policy = crate::alphacode_security_core::ScopePolicy::from_lists(
        std::slice::from_ref(&orchestrator.target),
        &[],
        true,
        &[],
        0,
        6,
    );
    crate::engagement_runtime::update(&session_id, |state| {
        state.policy = Some(policy.clone());
        state.research.scope = vec![orchestrator.target.clone()];
        Ok(())
    })?;
    crate::tool::scope::set_session_scope(&session_id, Some(policy));
    let base_ctx = ToolContext {
        session_id: session_id.clone(),
        message_id: "orchestrator".to_string(),
        tool_call_id: "bugbounty".to_string(),
        working_dir: Some(dir.to_path_buf()),
        stdin_request_tx: None,
        graceful_shutdown_signal: None,
        execution_mode: ToolExecutionMode::Direct,
    };

    let phases = [
        BountyPhase::Reconnaissance,
        BountyPhase::AttackSurfaceMapping,
        BountyPhase::VulnerabilityDiscovery,
        BountyPhase::Exploitation,
    ];

    let mut outcomes: Vec<TaskOutcome> = Vec::new();
    for phase in phases {
        let wave_results = orchestrator
            .execute_phase_tasks(phase, |task: &BountyTask| {
                let task = task.clone();
                let ctx = base_ctx.clone();
                async move { execute_task(&task, orchestrator, &ctx, dir).await }
            })
            .await;

        for (id, result) in wave_results {
            match result {
                Ok(result_text) => {
                    let note = result_text
                        .lines()
                        .rfind(|l| !l.trim().is_empty())
                        .unwrap_or("")
                        .to_string();
                    outcomes.push(TaskOutcome {
                        task_id: id,
                        ok: true,
                        note,
                    });
                }
                Err(err) => outcomes.push(TaskOutcome {
                    task_id: id,
                    ok: false,
                    note: err.to_string(),
                }),
            }
        }

        // The scanner graph publishes its discovery view into the same durable
        // engagement used by the agent, proof reviewer and exports. It never
        // promotes keyword candidates into verified findings.
        let intelligence = orchestrator.intelligence.read().await.clone();
        crate::engagement_runtime::update(&session_id, |state| {
            state.research.investigation = intelligence;
            Ok(())
        })?;

        orchestrator.advance_phase().await?;
        save_state(orchestrator, dir).await?;
    }

    write_report(orchestrator, dir, &outcomes).await?;
    if let Some(state) = crate::engagement_runtime::read(&session_id)? {
        atomic_write(
            &dir.join("engagement.json"),
            &serde_json::to_vec_pretty(&state.summary())?,
        )?;
    }
    save_state(orchestrator, dir).await?;
    Ok(outcomes)
}

/// Write a concise `report.md` next to the raw outputs.
///
/// It contains only what was actually observed: counts from the orchestrator,
/// per-task outcomes, and where each task's raw output lives. No severity
/// synthesis, no inferred findings.
async fn write_report(
    orchestrator: &BugBountyOrchestrator,
    dir: &Path,
    outcomes: &[TaskOutcome],
) -> Result<()> {
    let summary = orchestrator.get_summary().await;
    let mut md = String::from("# Bug bounty run report\n\n");
    md.push_str(&format!("Target: {}\n", orchestrator.target));
    md.push_str(&format!(
        "Subdomains: {} · URLs: {} · Ports: {} · Services: {} · Candidates (unverified): {} · Verified findings: {}\n\n",
        summary.subdomains_found,
        summary.urls_found,
        summary.ports_found,
        summary.services_found,
        summary.vulnerabilities_found,
        summary.verified_findings_found,
    ));

    if summary.vulnerabilities_found > 0 {
        md.push_str("## Vulnerability candidates (unverified)\n\n");
        let vulns = orchestrator.vulnerabilities.read().await;
        let mut keys: Vec<&String> = vulns.keys().collect();
        keys.sort();
        for key in keys {
            md.push_str(&format!("- `{key}` — {}\n", vulns[key]));
        }
        drop(vulns);
        md.push_str(
            "\nThese are keyword matches in scanner stdout, not findings. Each one must be \
             reproduced by hand (and supported by an actual recorded exchange) before it is \
             reported.\n",
        );
    }

    md.push_str("\n## Coverage and unresolved execution\n\n");
    md.push_str("Task completion means usable tool output was returned. It does not establish complete asset, authorization, or vulnerability coverage. Hydration caps, tool-internal limits and untested business flows still apply. Full discovered inventories remain in `state.json`.\n\n");
    let mut coverage = orchestrator.execution_coverage.read().await.clone();
    coverage.sort_by(|a, b| {
        a.task_id
            .cmp(&b.task_id)
            .then_with(|| a.input_manifest.cmp(&b.input_manifest))
    });
    for attempt in &coverage {
        let state = if attempt.is_partial() {
            "PARTIAL / requires disposition"
        } else {
            "hydrated calls finished"
        };
        md.push_str(&format!("- `{}`: {state}; {}/{} attempted, {} succeeded, {} failed, {} deferred, {} observation lines omitted; aggregate truncated: {}; reason: {}. Manifest: `{}`.\n", attempt.task_id, attempt.inputs_attempted, attempt.inputs_planned, attempt.successful_calls, attempt.failed_input_indices.len(), attempt.deferred_input_indices.len(), attempt.omitted_observation_lines, attempt.aggregate_output_truncated, attempt.stop_reason, attempt.input_manifest));
        if let Some(index) = attempt.in_flight_input_index {
            md.push_str(&format!("  - Input {index} has an indeterminate outcome after interruption. Inspect its state before repeating any write.\n"));
        }
    }
    if coverage.is_empty() {
        md.push_str("No per-call execution audit is available for these historical tasks; coverage is unknown.\n");
    }
    md.push_str("\nFailed and deferred inputs remain open work. A retry requires reviewing scope, budget and possible prior side effects; no automatic replay is authorized by this audit.\n");
    atomic_write(
        &dir.join("coverage.json"),
        &serde_json::to_vec_pretty(&coverage)?,
    )?;
    {
        let intelligence = orchestrator.intelligence.read().await;
        md.push_str("\n## Investigation intelligence\n\n");
        md.push_str(&format!(
            "{} canonical assets; {} investigation leads; {} assets without a validated investigation; {} chain experiments passing evidence checks.\n\n",
            intelligence.assets().len(), intelligence.leads().len(),
            intelligence.coverage_gaps().len(), intelligence.chain_transitions().len(),
        ));
        md.push_str("Scanner leads remain unverified until a controlled experiment passes the observed-exchange proof gate. Evidence checks establish the recorded differential; vulnerability impact and chain causality require independent review. A validated experiment does not imply complete coverage of an asset. Historical investigation state is retained on resume; current-process finding counts still require fresh proof.\n\n");
        for lead in intelligence.leads().values() {
            md.push_str(&format!(
                "- `{}`: {:?}; supporting artifacts: {}; contradictions: {}; blocker: {}.\n",
                lead.id,
                lead.status,
                lead.supporting.len(),
                lead.contradicting.len(),
                lead.blocker.as_deref().unwrap_or("none")
            ));
        }
    }
    md.push_str("\n## Task outcomes\n\n");
    for outcome in outcomes {
        let mark = if outcome.ok { "ok" } else { "failed" };
        let note = if outcome.note.is_empty() {
            "(no summary)"
        } else {
            &outcome.note
        };
        md.push_str(&format!("- `{}` — {mark}: {note}\n", outcome.task_id));
    }

    md.push_str("\n## Raw output files\n\n");
    md.push_str("One bounded `<task_id>.txt` per task, `state.json` for `--resume`, `coverage.json` for attempt coverage, and `coverage/` for input manifests, per-call returned output and interruption journals. Tool-internal output limits still apply. Manifests and raw transcripts may contain sensitive target data; handle them like the engagement evidence.\n");
    atomic_write(&dir.join("report.md"), md.as_bytes())
}

// --- tests ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn captured_output_truncates_at_utf8_boundaries() {
        let mut text = format!("{}é-end", "a".repeat(256 * 1024 - 1));
        truncate_utf8_bytes(&mut text, 256 * 1024);
        assert_eq!(text, "a".repeat(256 * 1024 - 1));

        let mut text = "é".to_string();
        truncate_utf8_bytes(&mut text, 2);
        assert_eq!(text, "é");
        truncate_utf8_bytes(&mut text, 1);
        assert!(text.is_empty());
    }

    async fn hydrate(
        tool: &str,
        id: &str,
        params: Value,
        dir: &Path,
        seeds: &Seeds,
    ) -> Result<Vec<Value>> {
        let orchestrator = BugBountyOrchestrator::new("test.example");
        let task = BountyTask {
            id: id.to_string(),
            name: id.to_string(),
            description: String::new(),
            tool: tool.to_string(),
            params,
            phase: BountyPhase::Reconnaissance,
            completed: false,
            result: None,
            dependencies: vec![],
            priority: 1,
            estimated_duration: 60,
            retry_count: 0,
            max_retries: 1,
        };
        build_tool_inputs(&task, &orchestrator, dir, seeds).await
    }

    fn seeds_with_params_and_urls() -> Seeds {
        Seeds {
            live: vec![
                "https://one.example".to_string(),
                "https://two.example".to_string(),
            ],
            with_params: vec!["https://one.example/page?id=1".to_string()],
            all_urls: vec![
                "https://one.example".to_string(),
                "https://two.example".to_string(),
                "https://one.example/page?id=1".to_string(),
            ],
        }
    }

    #[tokio::test]
    async fn dnsx_receives_targets_not_domain() {
        let inputs = hydrate(
            "dnsx",
            "recon_dnsx",
            json!({"domain": "test.example"}),
            &std::env::temp_dir(),
            &Seeds::default(),
        )
        .await
        .expect("hydrate");
        assert_eq!(inputs.len(), 1);
        assert!(inputs[0]["targets"].is_array(), "dnsx wants `targets`");
        assert!(
            inputs[0].get("domain").is_none(),
            "`domain` must not leak through"
        );
    }

    #[tokio::test]
    async fn katana_is_fanned_out_over_single_urls() {
        let inputs = hydrate(
            "katana",
            "surface_katana",
            json!({"url": [], "depth": 3, "js_crawl": true}),
            &std::env::temp_dir(),
            &seeds_with_params_and_urls(),
        )
        .await
        .expect("hydrate");
        assert!(inputs.len() >= 2, "one input per seed");
        for input in &inputs {
            assert!(input["url"].is_string(), "katana receives one url: {input}");
            assert!(input.get("js_crawl").is_none(), "unsupported field dropped");
        }
    }

    #[tokio::test]
    async fn ffuf_gets_a_materialised_wordlist() {
        let dir = std::env::temp_dir().join("alphacode-hydrate-test");
        std::fs::create_dir_all(&dir).unwrap();
        let inputs = hydrate(
            "ffuf",
            "surface_ffuf_params",
            json!({"url": [], "matcher": {"status": [200]}}),
            &dir,
            &seeds_with_params_and_urls(),
        )
        .await
        .expect("hydrate");
        assert!(!inputs.is_empty());
        let wordlist = inputs[0]["wordlist"].as_str().expect("wordlist path");
        assert!(Path::new(wordlist).exists(), "wordlist materialised");
        assert!(
            inputs[0]["url"].as_str().unwrap().contains("FUZZ"),
            "FUZZ placeholder present"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn sqlmap_targets_parameterised_urls_with_a_low_cap() {
        let inputs = hydrate(
            "sqlmap",
            "exploit_sqlmap",
            json!({"url": [], "batch": true, "level": 2, "risk": 2}),
            &std::env::temp_dir(),
            &seeds_with_params_and_urls(),
        )
        .await
        .expect("hydrate");
        // Only the parameterized URL in the fixture qualifies for level 2.
        assert_eq!(inputs.len(), 1, "one parameterized url");
        assert_eq!(
            inputs[0]["url"].as_str().unwrap(),
            "https://one.example/page?id=1"
        );
        assert_eq!(inputs[0]["batch"], json!(true));
    }

    #[tokio::test]
    async fn status_code_bool_is_translated_into_a_list() {
        let inputs = hydrate(
            "httpx",
            "surface_httpx",
            json!({"targets": [], "title": true, "tech_detect": true, "status_codes": true}),
            &std::env::temp_dir(),
            &Seeds {
                live: vec!["https://one.example".to_string()],
                with_params: vec![],
                all_urls: vec!["https://one.example".to_string()],
            },
        )
        .await
        .expect("hydrate");
        let codes = inputs[0]["status_codes"].as_str().expect("string");
        assert!(!codes.is_empty());
        // httpx seeds from the discovered subdomain root *and* the known live
        // hosts — here the target root plus the one fixture URL.
        let targets = inputs[0]["targets"].as_array().expect("targets list");
        assert_eq!(targets.len(), 2, "target root + live host");
        assert!(
            targets.contains(&json!("https://one.example")),
            "live host present"
        );
    }

    #[tokio::test]
    async fn state_round_trip_preserves_accumulators() {
        let dir = std::env::temp_dir().join(format!("alphacode-state-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let orch = BugBountyOrchestrator::new("rt.example");
        orch.initialize().await.unwrap();
        orch.complete_task(
            "recon_subfinder",
            "alpha.rt.example\nbeta.rt.example".to_string(),
        )
        .await
        .unwrap();
        save_state(&orch, &dir).await.unwrap();

        let revived = BugBountyOrchestrator::new("rt.example");
        assert!(load_state(&revived, &dir).await.unwrap(), "restored");
        assert_eq!(revived.subdomains.read().await.len(), 2);
        let completed = revived.get_completed_tasks().await;
        assert!(completed.iter().any(|t| t.id == "recon_subfinder"));

        // Cross-target restore is refused rather than merged silently.
        let wrong = BugBountyOrchestrator::new("other.example");
        assert!(load_state(&wrong, &dir).await.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn clean_url_line_strips_httpx_decoration() {
        assert_eq!(
            clean_url_line("https://host.example [200] [Some Title]"),
            Some("https://host.example".to_string())
        );
        assert_eq!(clean_url_line("[INF] starting"), None);
        assert_eq!(clean_url_line(""), None);
    }
    #[tokio::test]
    async fn investigation_state_round_trips_and_legacy_snapshots_default() {
        let dir = tempfile::tempdir().unwrap();
        let original = BugBountyOrchestrator::new("lab.example");
        original
            .ingest_tool_output("katana", "https://lab.example/api/object")
            .await;
        original
            .ingest_tool_output(
                "nuclei",
                "[medium] [template] https://lab.example/api/object vulnerability",
            )
            .await;
        assert_eq!(original.intelligence.read().await.leads().len(), 1);
        save_state(&original, dir.path()).await.unwrap();
        let restored = BugBountyOrchestrator::new("lab.example");
        assert!(load_state(&restored, dir.path()).await.unwrap());
        assert_eq!(restored.intelligence.read().await.leads().len(), 1);
        assert_eq!(restored.verified_finding_count().await, 0);
        let snapshot_path = dir.path().join(STATE_FILE);
        let mut legacy: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&snapshot_path).unwrap()).unwrap();
        legacy.as_object_mut().unwrap().remove("intelligence");
        std::fs::write(&snapshot_path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let legacy_runtime = BugBountyOrchestrator::new("lab.example");
        assert!(load_state(&legacy_runtime, dir.path()).await.unwrap());
        assert!(legacy_runtime.intelligence.read().await.leads().is_empty());
    }
}
