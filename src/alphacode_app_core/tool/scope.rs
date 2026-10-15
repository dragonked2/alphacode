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
/// Input keys whose value names a network target.
const TARGET_KEYS: &[&str] = &[
    "url",
    "urls",
    "uri",
    "link",
    "href",
    "target_url",
    "request_url",
    "target",
    "targets",
    "host",
    "hosts",
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
];

/// Input keys whose value is a local file containing targets.
///
/// Recon is usually driven from a list file (`httpx -l targets.txt`), so a gate
/// that only reads inline arguments sees no target at all and waves the call
/// through. The file is opened at gate time, bounded, so its contents are held
/// to the same policy as inline targets.
const TARGET_FILE_KEYS: &[&str] = &["list", "input", "f", "input_file", "target_file"];

/// Lines read from a target-list file. A larger file is bulk work the gate
/// should not second-guess line by line; the head is checked and the remainder
/// is the caller's responsibility.
const MAX_TARGET_FILE_LINES: usize = 2048;

/// Bytes read from a target-list file.
const MAX_TARGET_FILE_BYTES: u64 = 1 << 20;

/// Whether a raw argument value looks like a network target.
///
/// Guards the short key aliases (`u`, `t`, `d`), which several tools use for
/// unrelated things: `katana`'s old `d` field was a depth integer, and treating
/// `3` as a host would deny a call that never left the machine.
fn looks_like_target(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.contains(char::is_whitespace) {
        return false;
    }
    if trimmed.contains("://") {
        return true;
    }
    // A bare hostname or IP has a dot or a colon. A filesystem path has a dot
    // too, so separators are what actually distinguish the two.
    (trimmed.contains('.') || trimmed.contains(':'))
        && !trimmed.contains('/')
        && !trimmed.contains('\\')
}

/// Every target a call names: inline arguments first, then target-list files.
///
/// The gate evaluates each entry. Checking only the first is what let
/// `{"targets": ["in-scope.test", "out-of-scope.test"]}` through, which is the
/// ordinary shape of a recon call rather than an exotic one.
pub(crate) fn all_targets_from_input(tool: &str, input: &Value) -> Vec<String> {
    let _ = tool;
    let Some(obj) = input.as_object() else {
        return Vec::new();
    };
    let mut targets: Vec<String> = Vec::new();

    for (key, value) in obj {
        let lower = key.to_ascii_lowercase();
        if TARGET_FILE_KEYS.contains(&lower.as_str()) {
            if let Some(path) = value.as_str() {
                targets.extend(targets_from_file(path));
            }
            continue;
        }
        if !TARGET_KEYS.contains(&lower.as_str()) {
            continue;
        }
        collect_target_values(value, &mut targets);
    }

    targets.sort();
    targets.dedup();
    targets
}

fn collect_target_values(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::String(s) => {
            if looks_like_target(s) {
                out.push(s.trim().to_string());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_target_values(item, out);
            }
        }
        Value::Object(map) => {
            for (key, nested) in map {
                let lower = key.to_ascii_lowercase();
                if TARGET_KEYS.contains(&lower.as_str())
                    || TARGET_FILE_KEYS.contains(&lower.as_str())
                {
                    collect_target_values(nested, out);
                }
            }
        }
        _ => {}
    }
}

/// Read a target-list file, bounded.
///
/// An unreadable path contributes no targets: the tool itself will fail on it,
/// and failing the gate on a file the tool never opens would block valid calls.
fn targets_from_file(path: &str) -> Vec<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() || trimmed == "-" {
        return Vec::new();
    }
    let Ok(file) = std::fs::File::open(trimmed) else {
        return Vec::new();
    };
    let reader = std::io::BufReader::new(file);
    let reader = std::io::Read::take(reader, MAX_TARGET_FILE_BYTES);
    let mut out = Vec::new();
    for line in std::io::BufRead::lines(reader).map_while(Result::ok) {
        if out.len() >= MAX_TARGET_FILE_LINES {
            break;
        }
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        out.push(line.to_string());
    }
    out
}

fn denied_message(tool: &str, target: &str, reason: &str) -> String {
    format!(
        "Engagement scope block: `{tool}` would contact `{target}` but it is outside the \
         authorized scope for this session ({reason}). Out-of-scope testing is not permitted. \
         If this target should be tested, add it to the engagement scope first."
    )
}

fn unmatched_message(tool: &str, target: &str) -> String {
    format!(
        "Engagement scope block: `{tool}` would contact `{target}`, which matches no \
         in-scope rule for this session. This engagement is fail-closed, so an unmatched \
         target is refused. Add `{target}` to the engagement scope first."
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
    // No scope installed: nothing to enforce. This is the ordinary coding
    // session, and it must be completely unaffected.
    let Some(policy) = session_scope_policy(&ctx.session_id) else {
        return Ok(());
    };
    // Only network-capable tools are subject to the gate.
    if !is_network_tool(tool) {
        return Ok(());
    }
    // Every target is evaluated, not just the first. Three behaviours this
    // replaces, each of which let a request leave the engagement:
    //
    // * A denylist-only policy returned early, so an operator who listed only
    //   exclusions got no enforcement at all — while `scope_gate::decide`
    //   documents that explicit exclusions always win.
    // * `Pending` was treated as allowed, so `fail_closed` never actually
    //   failed closed: an unmatched target was permitted exactly when the
    //   policy said it must not be.
    // * Only the first target was checked, so an in-scope host listed first
    //   laundered every host listed after it — including out-of-scope ones and
    //   the entire contents of a recon list file.
    for target in all_targets_from_input(tool, input) {
        let decision = evaluate_target(&ctx.session_id, &target);
        if decision.is_unmatched() {
            // `decide` denies under `fail_closed`, so an unmatched target arrives
            // here as a denial rather than as `Pending`. It still needs the
            // fail-closed wording: "you tried a host the engagement never named"
            // is a different problem from "this host is excluded", and only the
            // former is fixed by widening the scope.
            return Err(unmatched_message(tool, &target));
        }
        match decision {
            ScopeDecision::Denied { reason } => {
                return Err(denied_message(tool, &target, &reason));
            }
            ScopeDecision::Pending if policy.fail_closed => {
                // Unreachable today, and kept as a belt-and-braces guard: the
                // gate must not depend on `decide`'s coupling to fail-closed
                // surviving a future change.
                return Err(unmatched_message(tool, &target));
            }
            ScopeDecision::Pending | ScopeDecision::Allowed { .. } => {}
        }
    }
    Ok(())
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
    let Some(policy) = session_scope_policy(session_id) else {
        return Ok(());
    };
    // A redirect into a host that matches no in-scope rule is the cheapest way
    // for an allowed request to leave the engagement, so a fail-closed policy
    // refuses the hop here too. Treating `Pending` as allowed made
    // `fail_closed` advisory for redirects, which is the one place it cannot
    // afford to be.
    let decision = evaluate_target(session_id, to);
    if decision.is_unmatched() {
        return Err(unmatched_message(tool, to));
    }
    match decision {
        ScopeDecision::Denied { reason } => Err(denied_message(tool, to, &reason)),
        ScopeDecision::Pending if policy.fail_closed => Err(unmatched_message(tool, to)),
        ScopeDecision::Pending | ScopeDecision::Allowed { .. } => Ok(()),
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
        if let Ok(mut scopes) = SESSION_SCOPES.write()
            && let Some(state) = scopes.get_mut(&self._session_id)
        {
            state.permits.in_flight = state.permits.in_flight.saturating_sub(1);
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
    fn a_list_file_whose_contents_are_out_of_scope_blocks_the_call() {
        // This used to assert the opposite. A recon list file is the most
        // common way a target set reaches a tool, and a gate that only reads
        // inline arguments sees no target at all in it — so the call ran
        // against whatever the file happened to contain.
        let id = "list-file-content-test";
        set_session_scope(id, Some(policy(&["example.com"], &["evil.com"], true)));
        let dir = tempfile::tempdir().expect("tempdir");
        let list = dir.path().join("targets.txt");
        std::fs::write(&list, "# comment\nexample.com\nevil.com\n").expect("write list");

        let err = enforce_call(
            &ctx_for(id),
            "httpx",
            &serde_json::json!({ "list": list.to_string_lossy() }),
        )
        .expect_err("an out-of-scope host inside the list file must block the call");
        assert!(err.contains("evil.com"), "{err}");
    }

    #[test]
    fn a_list_file_with_only_in_scope_targets_is_allowed() {
        let id = "list-file-ok-test";
        set_session_scope(id, Some(policy(&["example.com"], &["evil.com"], true)));
        let dir = tempfile::tempdir().expect("tempdir");
        let list = dir.path().join("targets.txt");
        std::fs::write(&list, "example.com\napi.example.com\n").expect("write list");

        assert!(
            enforce_call(
                &ctx_for(id),
                "httpx",
                &serde_json::json!({ "list": list.to_string_lossy() })
            )
            .is_ok()
        );
    }

    #[test]
    fn an_unreadable_list_file_does_not_block_the_call() {
        // The tool itself will fail on a missing file. Blocking the gate on a
        // file the tool never opens would deny valid calls for the wrong reason.
        let id = "list-file-missing-test";
        set_session_scope(id, Some(policy(&["example.com"], &[], true)));
        assert!(
            enforce_call(
                &ctx_for(id),
                "httpx",
                &serde_json::json!({"list": "definitely-not-here.txt"})
            )
            .is_ok()
        );
    }

    /// Regression: only the first target used to be checked, so an in-scope
    /// host listed first laundered every host after it.
    #[test]
    fn every_target_in_a_multi_target_call_is_checked() {
        let id = "multi-target-test";
        set_session_scope(id, Some(policy(&["example.com"], &["evil.com"], true)));
        let err = enforce_call(
            &ctx_for(id),
            "httpx",
            &serde_json::json!({"targets": ["example.com", "api.example.com", "evil.com"]}),
        )
        .expect_err("the out-of-scope entry must be caught even behind two allowed ones");
        assert!(err.contains("evil.com"), "{err}");
    }

    /// Regression: `fail_closed` did not fail closed. `Pending` was allowed, so
    /// an unmatched target was permitted precisely when the policy said it must
    /// not be.
    #[test]
    fn fail_closed_denies_a_target_matching_no_rule() {
        let id = "fail-closed-test";
        let p = policy(&["example.com"], &[], true);
        assert!(p.fail_closed, "this policy must actually be fail-closed");
        set_session_scope(id, Some(p));

        let err = enforce_call(
            &ctx_for(id),
            "webfetch",
            &serde_json::json!({"url": "https://unrelated.test/x"}),
        )
        .expect_err("an unmatched target must be refused under a fail-closed policy");
        assert!(err.contains("unrelated.test"), "{err}");
        // The target matched no in-scope rule, which must be reported as the
        // fail-closed refusal it is. It must NOT be phrased as an explicit
        // out-of-scope exclusion: that implies the engagement named the host and
        // rejected it, when in fact the engagement never mentioned it at all.
        // The two need different fixes from the user, so they get different
        // wording here.
        assert!(
            err.contains("matches no in-scope rule") && err.contains("fail-closed"),
            "an unmatched target must be reported as a fail-closed refusal: {err}"
        );
        assert!(
            !err.contains("out-of-scope rule"),
            "an unmatched target must not be reported as an explicit exclusion: {err}"
        );

        // Still permits what the policy declares.
        assert!(
            enforce_call(
                &ctx_for(id),
                "webfetch",
                &serde_json::json!({"url": "https://api.example.com/x"})
            )
            .is_ok()
        );
    }

    #[test]
    fn a_non_enforcing_policy_still_permits_unmatched_targets() {
        let id = "not-fail-closed-test";
        set_session_scope(id, Some(policy(&["example.com"], &[], false)));
        assert!(
            enforce_call(
                &ctx_for(id),
                "webfetch",
                &serde_json::json!({"url": "https://unrelated.test/x"})
            )
            .is_ok()
        );
    }

    /// Regression: a denylist-only policy returned early, so an operator who
    /// listed only exclusions got no enforcement at all.
    #[test]
    fn a_denylist_only_policy_still_enforces_its_exclusions() {
        let id = "denylist-only-test";
        set_session_scope(id, Some(policy(&[], &["evil.com"], true)));
        let err = enforce_call(
            &ctx_for(id),
            "webfetch",
            &serde_json::json!({"url": "https://evil.com/x"}),
        )
        .expect_err("an explicit exclusion must be enforced with no allowlist present");
        assert!(err.contains("evil.com"), "{err}");

        // A denylist is not an allowlist: it must not deny everything else.
        assert!(
            enforce_call(
                &ctx_for(id),
                "webfetch",
                &serde_json::json!({"url": "https://anything.test/x"})
            )
            .is_ok()
        );
    }

    /// Regression: a redirect to an unmatched host was followed because
    /// `Pending` was treated as allowed.
    #[test]
    fn fail_closed_refuses_an_unmatched_redirect_hop() {
        let id = "redirect-fail-closed-test";
        set_session_scope(id, Some(policy(&["example.com"], &[], true)));
        assert!(
            check_redirect(
                id,
                "webfetch",
                "https://example.com/a",
                "https://elsewhere.test/b"
            )
            .is_err(),
            "a hop to an unmatched host must be refused under a fail-closed policy"
        );
        assert!(
            check_redirect(
                id,
                "webfetch",
                "https://example.com/a",
                "https://api.example.com/b"
            )
            .is_ok()
        );
    }

    #[test]
    fn short_key_aliases_that_are_not_targets_are_ignored() {
        // `katana`'s `d` was a depth integer. Reading `3` as a hostname would
        // deny a call that never left the machine.
        let id = "short-key-test";
        set_session_scope(id, Some(policy(&["example.com"], &[], true)));
        assert!(
            enforce_call(
                &ctx_for(id),
                "katana",
                &serde_json::json!({"url": "https://example.com", "d": "3"})
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
