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
    BountyPhase, BountyTask, BugBountyOrchestrator,
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
/// relationship would silently loosen the gate. Evidence is rebuilt by
/// re-running the scanner tasks, which the task graph tracks via
/// `completed`/`result` — exactly what this snapshot carries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestratorSnapshot {
    pub target: String,
    pub current_phase: BountyPhase,
    pub tasks: BTreeMap<String, BountyTask>,
    pub subdomains: BTreeSet<String>,
    pub urls: BTreeSet<String>,
    pub ports: BTreeSet<u16>,
    pub services: BTreeMap<String, String>,
    pub vulnerabilities: BTreeMap<String, String>,
}

const STATE_FILE: &str = "state.json";

/// Atomically write `bytes` to `path` (temp file + rename) so a crash cannot
/// leave a truncated `state.json` behind.
fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| anyhow!("failed to write {}: {e}", tmp.display()))?;
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
    std::fs::rename(&tmp, path).map_err(|e| anyhow!("failed to replace {}: {e}", path.display()))
}

/// Serialize the orchestrator's accumulators and task graph to `dir/state.json`.
pub async fn save_state(orchestrator: &BugBountyOrchestrator, dir: &Path) -> Result<()> {
    let snapshot = OrchestratorSnapshot {
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

    *orchestrator.current_phase.write().await = snapshot.current_phase;
    *orchestrator.tasks.write().await = snapshot.tasks.into_iter().collect();
    *orchestrator.subdomains.write().await = snapshot.subdomains.into_iter().collect();
    *orchestrator.urls.write().await = snapshot.urls.into_iter().collect();
    *orchestrator.ports.write().await = snapshot.ports.into_iter().collect();
    *orchestrator.services.write().await = snapshot.services.into_iter().collect();
    *orchestrator.vulnerabilities.write().await = snapshot.vulnerabilities.into_iter().collect();
    Ok(true)
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
    let mut live: Vec<String> = Vec::new();
    let mut with_params: Vec<String> = Vec::new();
    let mut all_urls: Vec<String> = Vec::new();

    // Task results are the primary source: this is where httpx confirmed
    // liveness and the crawlers produced endpoint lists.
    let tasks = orchestrator.tasks.read().await;
    let mut results: Vec<(&str, &String)> = tasks
        .iter()
        .filter(|(_, t)| t.completed && t.result.is_some())
        .map(|(_, t)| (t.tool.as_str(), t.result.as_ref().unwrap()))
        .collect();

    // http-family results first so `live` is ordered by confirmed liveness.
    results.sort_by_key(|(tool, _)| usize::from(!URL_LINE_TOOLS.contains(tool)));
    for (tool, result) in results.drain(..) {
        let only_live = tool == "httpx" || tool == "httprobe";
        for line in result.lines() {
            if let Some(url) = clean_url_line(line) {
                if url.contains('?') {
                    with_params.push(url.clone());
                }
                if only_live {
                    live.push(url.clone());
                }
                if !all_urls.contains(&url) {
                    all_urls.push(url);
                }
            }
        }
    }
    drop(tasks);

    // Backfill from the URL accumulator, through the same cleaner.
    for url in orchestrator.urls.read().await.iter() {
        if let Some(url) = clean_url_line(url)
            && !all_urls.contains(&url)
        {
            if url.contains('?') {
                with_params.push(url.clone());
            }
            all_urls.push(url);
        }
    }

    if all_urls.is_empty() {
        // Nothing has run yet. Degrade to the target root rather than
        // silently producing zero-width work.
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
    let mut out: Vec<String> = Vec::new();
    for url in seeds.live.iter().chain(seeds.all_urls.iter()) {
        if !out.contains(url) {
            out.push(url.clone());
        }
    }
    out
}

/// Subdomain fallback for the http probe when no URL source has run yet.
async fn substrate_subdomains(orchestrator: &BugBountyOrchestrator) -> Vec<String> {
    let mut out = vec![target_root(&orchestrator.target)];
    let subs = orchestrator.subdomains.read().await;
    for sub in subs.iter() {
        if out.len() >= HTTPX_TARGET_CAP {
            break;
        }
        if !sub.is_empty() {
            out.push(format!("https://{sub}"));
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

/// Execute one orchestrator task: hydrate its parameters from the current
/// state, run the tool against every hydrated input, persist raw output, and
/// return the text `complete_task` should ingest.
///
/// The returned string is deliberately shaped for
/// `ingest_tool_output` — the raw tool stdout is saved verbatim to
/// `<task_id>.txt`, while URL-class tools' ingest text carries one clean URL
/// per line so httpx decoration never leaks into the accumulators.
async fn execute_task(
    task: &BountyTask,
    orchestrator: &BugBountyOrchestrator,
    ctx: &ToolContext,
    dir: &Path,
) -> Result<String> {
    let seeds = collect_seeds(orchestrator).await;
    let inputs = build_tool_inputs(task, orchestrator, dir, &seeds).await?;
    if inputs.is_empty() {
        return Err(anyhow!(
            "no {} inputs hydrated — run the prerequisite phase first",
            task.tool
        ));
    }

    let is_url_tool = URL_LINE_TOOLS.contains(&task.tool.as_str());
    let mut raw_text = String::new();
    let mut lines: Vec<String> = Vec::new();
    let mut consecutive_failures = 0u32;
    let mut failures = 0usize;
    let mut successes = 0usize;

    for (i, input) in inputs.iter().enumerate() {
        let call_ctx = ToolContext {
            session_id: ctx.session_id.clone(),
            message_id: ctx.message_id.clone(),
            tool_call_id: format!("{}-{}", ctx.tool_call_id, i),
            working_dir: ctx.working_dir.clone(),
            stdin_request_tx: None,
            graceful_shutdown_signal: None,
            execution_mode: ToolExecutionMode::Direct,
        };
        match run_task_tool(&task.tool, input.clone(), call_ctx).await {
            Ok(out) => {
                successes += 1;
                consecutive_failures = 0;
                raw_text.push_str(&out.output);
                raw_text.push('\n');
                for line in out.output.lines() {
                    let shaped: Option<String> = if is_url_tool {
                        clean_url_line(line)
                    } else {
                        let trimmed = line.trim();
                        if trimmed.is_empty() {
                            None
                        } else {
                            Some(trimmed.to_string())
                        }
                    };
                    if let Some(text) = shaped
                        && !lines.contains(&text)
                    {
                        lines.push(text);
                    }
                }
            }
            Err(err) => {
                failures += 1;
                consecutive_failures += 1;
                let msg = err.to_string();
                raw_text.push_str(&format!("[error] {msg}\n"));
                // A missing binary will not fix itself inside this loop — stop
                // burning seeds on it and surface the install hint instead.
                let binary_missing =
                    msg.contains("failed to run ") || msg.contains("did not complete");
                if binary_missing || consecutive_failures >= 2 {
                    break;
                }
            }
        }
        if raw_text.len() > 256 * 1024 {
            raw_text.truncate(256 * 1024);
        }
        if lines.len() >= 2_000 {
            break;
        }
    }

    let raw_path = dir.join(format!("{}.txt", task.id));
    atomic_write(&raw_path, raw_text.as_bytes())?;

    if successes == 0 {
        let last_error = raw_text
            .lines()
            .rfind(|l| l.starts_with("[error]"))
            .unwrap_or("no output captured")
            .to_string();
        return Err(anyhow!(
            "{}: all executions failed; {last_error}",
            task.tool
        ));
    }

    let ingest_text = lines.join("\n");
    let notice = if failures > 0 {
        format!(
            "\n({successes} call(s) succeeded, {failures} failed; raw output: {})",
            raw_path.display()
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
    // Every later phase re-reads live state; a restored run simply sees most
    // tasks already completed and executes only what remains.

    let base_ctx = ToolContext {
        session_id: "bugbounty".to_string(),
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

        orchestrator.advance_phase().await?;
        save_state(orchestrator, dir).await?;
    }

    write_report(orchestrator, dir, &outcomes).await?;
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
    md.push_str("One `<task_id>.txt` per task, plus `state.json` for `--resume`.\n");
    atomic_write(&dir.join("report.md"), md.as_bytes())
}

// --- tests ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
}
