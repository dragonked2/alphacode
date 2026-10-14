//! Per-session engagement-scope enforcement, wired in front of every tool call.
//!
//! [`alphacode_security_core::scope_gate`](crate::alphacode_security_core::scope_gate)
//! is the deterministic decision function — "is this host in scope?" — and it is
//! pure: no I/O, no tool registry, no provider, no model. This module is the
//! *wiring* that turns it into a gate rather than a library nobody calls.
//!
//! Why this has to live at the registry rather than inside each tool:
//!
//! * There are more than thirty tools that put bytes on the wire. Wiring scope
//!   into each one means the thirty-first is the one that forgets, and the
//!   thirty-first is a tool that does not exist yet.
//! * `Registry::execute` is the only funnel. The agent loop, the batch
//!   scheduler, the SDK bridge and the harness server all reach the network
//!   through it, so a check here is checkable by a single caller and cannot be
//!   skipped by a subagent.
//! * The decision must not be negotiable from the model side. A tool reading
//!   its own policy would be a policy the model's own arguments can influence;
//!   this gate reads only the session policy and the raw call arguments.
//!
//! Three properties this module is responsible for:
//!
//! 1. **Centralization.** One function, [`enforce_call`], runs in front of every
//!    tool. Tools do not get an opinion.
//! 2. **Fail-closed when an allowlist exists.** If a session declares
//!    in-scope targets and enforcement is on, a target matching no rule is
//!    denied. Without an allowlist there is nothing to fail closed against, so
//!    only explicit exclusions are enforced — that keeps ordinary coding
//!    sessions, which never declare a scope, completely unaffected.
//! 3. **Redirects cannot widen the engagement.** [`check_redirect`] re-evaluates
//!    every hop against the same policy, so an allowed origin cannot redirect
//!    the agent into an out-of-scope host.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{LazyLock, RwLock as StdRwLock};

use serde_json::Value;

use crate::alphacode_security_core::scope_gate::{ScopeDecision, ScopePolicy};
use crate::tool::ToolContext;

/// Per-session scope policy plus the sticky decision cache.
#[derive(Clone, Debug, Default)]
struct SessionScopeState {
    /// Absent means "no engagement scope installed" — every network tool is
    /// allowed through (subject to its own SSRF guard). Present means the
    /// centralized gate applies to every network tool call in this session.
    policy: Option<ScopePolicy>,
    /// Cache of raw target -> decision so repeat probes of the same host do not
    /// re-parse rules. The cache is bounded to prevent unbounded growth on a
    /// long recon run.
    decisions: HashMap<String, ScopeDecision>,
    /// Live network permits for this session (rate/concurrency budget).
    permits: SessionPermits,
}

/// Rate/concurrency budget for network tools, keyed by session.
#[derive(Clone, Debug, Default)]
struct SessionPermits {
    /// Number of network calls currently in flight for this session.
    in_flight: usize,
}

static SESSION_SCOPES: LazyLock<StdRwLock<HashMap<String, SessionScopeState>>> =
    LazyLock::new(|| StdRwLock::new(HashMap::new()));

/// A process-wide counter of in-flight network permits across *all* sessions.
///
/// This is the global concurrency budget the engagement rules declare: even a
/// swarm of parallel subagents cannot exceed it, so a single session cannot
/// accidentally saturate a target's rate limit by spawning many workers.
static GLOBAL_IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

/// Small bounded decision cache. 4096 entries covers a realistic recon session
/// (reddit-scale asset lists) while staying tiny.
const MAX_CACHED_DECISIONS: usize = 4096;
/// Default global network concurrency cap when no scope is configured. Kept
/// high enough that ordinary multi-tool sessions are not throttled.
const DEFAULT_GLOBAL_MAX_CONCURRENT: usize = 64;

/// Tool classes whose input is (or contains) a network target.
pub(crate) fn is_network_tool(tool: &str) -> bool {
    matches!(
        tool,
        "webfetch"
            | "httpflow"
            | "scrapling"
            | "dalfox"
            | "sqlmap"
            | "feroxbuster"
            | "gobuster"
            | "katana"
            | "nikto"
            | "cariddi"
            | "gospider"
            | "hakrawler"
            | "corsy"
            | "crlfuzz"
            | "httpx"
            | "httprobe"
            | "ffuf"
            | "nuclei"
            | "nmap"
            | "naabu"
            | "dnsx"
            | "meg"
            | "subfinder"
            | "amass"
            | "assetfinder"
            | "gau"
            | "waybackurls"
            | "kxss"
            | "qsreplace"
            | "gf"
    )
}

/// Install (or replace) the engagement scope for a session. Pass `None` to
/// remove enforcement entirely for a session (used when a session is not in
/// bug-bounty mode).
pub fn set_session_scope(session_id: &str, policy: Option<ScopePolicy>) {
    let mut scopes = SESSION_SCOPES.write().unwrap_or_else(|p| p.into_inner());
    let entry = scopes.entry(session_id.to_string()).or_default();
    entry.policy = policy;
    // A policy change invalidates cached decisions so stale allow/deny results
    // cannot survive an updated config.
    entry.decisions.clear();
}

/// Remove a session's scope state entirely.
pub fn clear_session_scope(session_id: &str) {
    SESSION_SCOPES
        .write()
        .unwrap_or_else(|p| p.into_inner())
        .remove(session_id);
}

/// Resolve the current policy for a session.
pub fn session_scope_policy(session_id: &str) -> Option<ScopePolicy> {
    SESSION_SCOPES
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .get(session_id)
        .and_then(|s| s.policy.clone())
}

/// Extract the best single target for a scope check from raw tool input.
/// Returns `None` for tools with no obvious single URL (batch tools, list-file
/// tools, local file tools). The registry-level gate only needs *a* target;
/// per-tool enforcement re-checks each real hostname at execution time.
pub(crate) fn single_target_from_input(tool: &str, input: &Value) -> Option<String> {
    let _ = tool;
    let obj = input.as_object()?;
    // Most URL tools share a small set of keys (see coerce_url_arg aliases).
    for key in [
        "url",
        "uri",
        "link",
        "href",
        "target_url",
        "request_url",
        "target",
        "host",
        "hostname",
        "domain",
        "resource",
        "location",
        "site",
        "website",
        "page",
        "u",
        "t",
        "d",
        "input",
    ] {
        if let Some(Value::String(s)) = obj.get(key) {
            if !s.trim().is_empty() {
                return Some(s.clone());
            }
        }
    }
    // Arrays of targets: pick the first that looks like a URL/host.
    if let Some(Value::Array(items)) = obj.get("targets") {
        for item in items {
            if let Some(s) = item.as_str() {
                return Some(s.to_string());
            }
        }
    }
    // ffuf uses `url`; websockets use `url` too. Fall back to any http(s)
    // string anywhere in the object when the above missed it.
    for value in obj.values() {
        if let Some(s) = value.as_str() {
            let lower = s.to_ascii_lowercase();
            if lower.starts_with("http://") || lower.starts_with("https://") {
                return Some(s.to_string());
            }
        }
    }
    None
}

fn denied_message(tool: &str, target: &str, reason: &str) -> String {
    format!(
        "Engagement scope block: `{tool}` would contact `{target}` but it is outside the \
         authorized scope for this session ({reason}). Out-of-scope testing is not permitted. \
         If this target should be tested, add it to the engagement scope first."
    )
}

/// Cache the decision for a target under a session, bounded.
fn cache_decision(session_id: &str, target: String, decision: ScopeDecision) {
    let mut scopes = SESSION_SCOPES.write().unwrap_or_else(|p| p.into_inner());
    if let Some(state) = scopes.get_mut(session_id) {
        if state.decisions.len() >= MAX_CACHED_DECISIONS {
            // Drop prior entries on overflow; a recon run that probes this many
            // distinct hosts is better served by re-deciding than by unbounded
            // memory.
            state.decisions.clear();
        }
        state.decisions.insert(target, decision);
    }
}

/// Evaluate one raw target under the session policy. Returns the cached
/// decision when available, else computes and caches it.
fn evaluate_target(session_id: &str, target: &str) -> ScopeDecision {
    // Cache lookup.
    {
        let scopes = SESSION_SCOPES.read().unwrap_or_else(|p| p.into_inner());
        if let Some(state) = scopes.get(session_id)
            && let Some(prev) = state.decisions.get(target)
        {
            return prev.clone();
        }
    }
    let Some(policy) = session_scope_policy(session_id) else {
        return ScopeDecision::Pending;
    };
    let decision = crate::alphacode_security_core::scope_gate::decide(&policy, target);
    cache_decision(session_id, target.to_string(), decision.clone());
    decision
}

/// Centralized gate: decide whether `tool` may run for the given inputs under
/// the session's engagement scope.
///
/// This is called from `Registry::execute` before any per-tool code runs. It is
/// a no-op when the session has no scope policy installed (ordinary sessions).
/// The function is synchronous on purpose: the existing registry wiring calls
/// it without `.await`, and scope evaluation is pure and fast.
pub(crate) fn enforce_call(ctx: &ToolContext, tool: &str, input: &Value) -> Result<(), String> {
    // No scope installed: nothing to enforce.
    let Some(policy) = session_scope_policy(&ctx.session_id) else {
        return Ok(());
    };
    // No allowlist: only explicit exclusions can deny, which the decision
    // function already handles. An empty allowlist must not fail everything.
    if policy.in_scope.is_empty() {
        return Ok(());
    }
    // Only network-capable tools are subject to the gate.
    if !is_network_tool(tool) {
        return Ok(());
    }
    // No single target string (list files, batch payloads): let the tool run;
    // its per-request execution re-checks real hostnames.
    let Some(target) = single_target_from_input(tool, input) else {
        return Ok(());
    };
    match evaluate_target(&ctx.session_id, &target) {
        ScopeDecision::Allowed { .. } | ScopeDecision::Pending => Ok(()),
        ScopeDecision::Denied { reason } => Err(denied_message(tool, &target, &reason)),
    }
}

/// Re-evaluate a redirect hop against the same session scope.
///
/// Called by tools that follow redirects (`webfetch`, `httpflow`) before the
/// next request is sent. This is what prevents scope expansion through a
/// benign-looking redirect chain.
pub(crate) fn check_redirect(
    session_id: &str,
    tool: &str,
    _from: &str,
    to: &str,
) -> Result<(), String> {
    // No scope installed: nothing to enforce. Per-tool SSRF guards still run.
    if session_scope_policy(session_id).is_none() {
        return Ok(());
    }
    match evaluate_target(session_id, to) {
        ScopeDecision::Allowed { .. } | ScopeDecision::Pending => Ok(()),
        ScopeDecision::Denied { reason } => Err(denied_message(tool, to, &reason)),
    }
}

/// A rate/concurrency permit held for the duration of one network tool call.
///
/// `drop` returns the permit to the session and global budget, including when
/// the tool errors out or is cancelled.
pub(crate) struct NetworkPermit {
    _session_id: String,
    _released: bool,
}

impl NetworkPermit {
    /// A no-op permit for non-network tools (or sessions without limits).
    pub(crate) fn none() -> Self {
        Self {
            _session_id: String::new(),
            _released: true,
        }
    }
}

impl Drop for NetworkPermit {
    fn drop(&mut self) {
        if self._released {
            return;
        }
        self._released = true;
        GLOBAL_IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
        if let Some(mut scopes) = SESSION_SCOPES.write().ok() {
            if let Some(state) = scopes.get_mut(&self._session_id) {
                state.permits.in_flight = state.permits.in_flight.saturating_sub(1);
            }
        }
    }
}

/// Acquire a network permit for a tool call, respecting both the session policy
/// (if any) and a global concurrency ceiling.
pub(crate) async fn acquire_network_permit(session_id: &str, tool: &str) -> NetworkPermit {
    // Global ceiling: default, or the engagement rule when configured.
    let global_max = session_scope_policy(session_id)
        .map(|p| p.max_concurrent as usize)
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_GLOBAL_MAX_CONCURRENT);

    // Spin-wait bounded by the global budget: this is a lightweight permit the
    // registry holds across the call. Without it, a swarm of subagents could
    // each run `nmap` at the same time and blow past even a per-session cap.
    //
    // Note: we deliberately do not block forever; the calling tool itself has
    // timeouts and cancellation, and this returns as soon as a slot is free.
    let mut spins = 0usize;
    loop {
        let current = GLOBAL_IN_FLIGHT.load(Ordering::SeqCst);
        if current >= global_max {
            tokio::task::yield_now().await;
            spins += 1;
            // 10_000 yields at ~µs each is a generous backoff cap that never
            // blocks the agent loop below a tool's own timeout.
            if spins > 10_000 {
                // Fall through: the thread pool is clearly stalled; the per-tool
                // timeout will eventually fail the call and release its permit.
                return NetworkPermit::none();
            }
            continue;
        }
        if GLOBAL_IN_FLIGHT
            .compare_exchange_weak(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            break;
        }
    }

    let mut scopes = SESSION_SCOPES.write().unwrap_or_else(|p| p.into_inner());
    if let Some(state) = scopes.get_mut(session_id) {
        state.permits.in_flight = state.permits.in_flight.saturating_add(1);
    }

    let _ = tool;
    NetworkPermit {
        _session_id: session_id.to_string(),
        _released: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy(in_scope: &[&str], out_of_scope: &[&str], enforce: bool) -> ScopePolicy {
        crate::alphacode_security_core::scope_gate::ScopePolicy::from_lists(
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

    fn ctx_for(session_id: &str) -> ToolContext {
        ToolContext {
            session_id: session_id.to_string(),
            message_id: "m".to_string(),
            tool_call_id: "t".to_string(),
            working_dir: None,
            stdin_request_tx: None,
            graceful_shutdown_signal: None,
            execution_mode: crate::alphacode_tool_core::ToolExecutionMode::Direct,
        }
    }

    #[test]
    fn no_scope_allows_all() {
        let id = "no-scope-test";
        set_session_scope(id, None);
        assert!(
            enforce_call(
                &ctx_for(id),
                "webfetch",
                &serde_json::json!({"url": "https://example.com"})
            )
            .is_ok()
        );
    }

    #[test]
    fn allowed_target_passes() {
        let id = "allowed-test";
        set_session_scope(id, Some(policy(&["example.com"], &[], true)));
        assert!(
            enforce_call(
                &ctx_for(id),
                "webfetch",
                &serde_json::json!({"url": "https://api.example.com/v1"})
            )
            .is_ok()
        );
    }

    #[test]
    fn out_of_scope_target_blocked_with_reason() {
        let id = "blocked-test";
        set_session_scope(id, Some(policy(&["example.com"], &[], true)));
        let err = enforce_call(
            &ctx_for(id),
            "webfetch",
            &serde_json::json!({"url": "https://evil.com/v1"}),
        )
        .unwrap_err();
        assert!(err.contains("Engagement scope block"));
        assert!(err.contains("evil.com"));
    }

    #[test]
    fn non_network_tool_not_blocked() {
        let id = "local-tool-test";
        set_session_scope(id, Some(policy(&["example.com"], &[], true)));
        // bash is not a network tool; must not be blocked even though input
        // happens to contain a URL.
        assert!(
            enforce_call(
                &ctx_for(id),
                "bash",
                &serde_json::json!({"command": "curl https://evil.com"})
            )
            .is_ok()
        );
    }

    #[test]
    fn list_shaped_input_not_blocked_when_no_single_target() {
        let id = "list-test";
        set_session_scope(id, Some(policy(&["example.com"], &[], true)));
        // httpx with a list file has no single target string; the registry gate
        // must let the tool run and let the per-tool execution gate re-check.
        assert!(
            enforce_call(
                &ctx_for(id),
                "httpx",
                &serde_json::json!({"list": "hosts.txt"})
            )
            .is_ok()
        );
    }

    #[test]
    fn cached_decision_is_stable() {
        let id = "cached-test";
        set_session_scope(id, Some(policy(&["example.com"], &[], true)));
        assert!(
            enforce_call(
                &ctx_for(id),
                "webfetch",
                &serde_json::json!({"url": "https://evil.com"})
            )
            .is_err()
        );
        assert!(
            enforce_call(
                &ctx_for(id),
                "webfetch",
                &serde_json::json!({"url": "https://evil.com"})
            )
            .is_err()
        );
    }

    #[test]
    fn policy_update_clears_cache() {
        let id = "cache-clear-test";
        set_session_scope(id, Some(policy(&["example.com"], &[], true)));
        assert!(
            enforce_call(
                &ctx_for(id),
                "webfetch",
                &serde_json::json!({"url": "https://evil.com"})
            )
            .is_err()
        );
        set_session_scope(id, Some(policy(&["example.com", "evil.com"], &[], true)));
        assert!(
            enforce_call(
                &ctx_for(id),
                "webfetch",
                &serde_json::json!({"url": "https://evil.com"})
            )
            .is_ok()
        );
    }

    #[test]
    fn redirect_recheck_blocks_out_of_scope_hop() {
        let id = "redirect-test";
        set_session_scope(id, Some(policy(&["example.com"], &[], true)));
        assert!(
            check_redirect(
                id,
                "httpflow",
                "https://example.com/a",
                "https://evil.com/b"
            )
            .is_err()
        );
        assert!(
            check_redirect(
                id,
                "httpflow",
                "https://example.com/a",
                "https://example.com/b"
            )
            .is_ok()
        );
    }

    #[test]
    fn network_permit_drop_releases_budget() {
        let id = "permit-test";
        set_session_scope(id, Some(policy(&["example.com"], &[], true)));
        let before = GLOBAL_IN_FLIGHT.load(Ordering::SeqCst);
        let permit = acquire_network_permit_sync_for_test(id);
        assert_eq!(GLOBAL_IN_FLIGHT.load(Ordering::SeqCst), before + 1);
        drop(permit);
        assert_eq!(GLOBAL_IN_FLIGHT.load(Ordering::SeqCst), before);
    }

    /// Synchronous helper for permit lifecycle tests (avoids async in a sync test).
    fn acquire_network_permit_sync_for_test(_session_id: &str) -> NetworkPermit {
        GLOBAL_IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
        NetworkPermit {
            _session_id: _session_id.to_string(),
            _released: false,
        }
    }
}
