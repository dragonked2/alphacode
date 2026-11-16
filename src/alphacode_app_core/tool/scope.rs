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
use std::path::Path;
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
    /// The earliest instant at which the *next* admission may start.
    ///
    /// This is a reservation, not a record of the previous admission. Two
    /// concurrent callers that both read "last admitted at T" would both compute
    /// the same wait, sleep together, and be admitted together — the declared
    /// rate was only honoured for strictly serial callers. Reserving the next
    /// slot while holding the write lock makes the spacing hold under
    /// concurrency, which is the case `max_rate_rps` exists for.
    next_admission: Option<std::time::Instant>,
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
            | "ctf_web_recon"
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

/// Programs that can put bytes on the wire when run through the `bash` tool.
///
/// `bash` is not in [`is_network_tool`] because most shell commands are local
/// (`cargo test`, `git status`, `rg`), and gating those would be false friction
/// in every scoped session. Instead each command segment's program is matched
/// here, and only network segments are checked.
const SHELL_NETWORK_PROGRAMS: &[&str] = &[
    "curl",
    "curl.exe",
    "wget",
    "wget.exe",
    "nc",
    "ncat",
    "netcat",
    "socat",
    "ssh",
    "scp",
    "sftp",
    "ftp",
    "telnet",
    "dig",
    "nslookup",
    "host",
    "ping",
    "tracert",
    "traceroute",
    "openssl",
    "httpx",
    "httprobe",
    "nuclei",
    "nmap",
    "naabu",
    "ffuf",
    "feroxbuster",
    "gobuster",
    "nikto",
    "sqlmap",
    "dalfox",
    "katana",
    "gospider",
    "hakrawler",
    "subfinder",
    "amass",
    "assetfinder",
    "dnsx",
    "gau",
    "waybackurls",
    "meg",
    "kxss",
    "qsreplace",
    "cariddi",
    "corsy",
    "crlfuzz",
    "gf",
];

/// Flags whose following token is a value, not a destination.
///
/// Without this table `curl -H 'Accept: */*' https://in-scope.test` would treat
/// the header value as a candidate target and `curl -o out.html URL` would treat
/// the local output path as one. Over-collecting values only causes false
/// denials, so the table stays explicit and the fallback stays conservative.
const SHELL_VALUE_FLAGS: &[&str] = &[
    "-H",
    "--header",
    "-d",
    "--data",
    "--data-raw",
    "--data-binary",
    "--data-urlencode",
    "-X",
    "--request",
    "-u",
    "--user",
    "-b",
    "--cookie",
    "-c",
    "--cookie-jar",
    "-o",
    "--output",
    "-w",
    "--write-out",
    "-A",
    "--user-agent",
    "-e",
    "--referer",
    "-F",
    "--form",
    "-T",
    "--upload-file",
    "-x",
    "--proxy",
    "-m",
    "--max-time",
    "--connect-timeout",
    "-t",
    "--threads",
    "-p",
    "--ports",
    "--wordlist",
    "-r",
    "--rate-limit",
    "-rl",
    "-timeout",
    "-concurrency",
    "--concurrency",
    "--output-dir",
];

/// Flags naming a file whose lines are destinations (`httpx -l targets.txt`).
const SHELL_LIST_FLAGS: &[&str] = &["-l", "--list", "-iL", "-list"];

/// Whether a tool runs caller-supplied shell commands.
pub(crate) fn is_shell_tool(tool: &str) -> bool {
    matches!(tool, "bash")
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
    // In-flight accounting stays (those calls are still running), but the
    // admission schedule belongs to the *old* rate and must not delay the new
    // one. Without this, switching from 1 req/s to a higher rate would keep
    // waiting on a reservation made under the previous policy.
    entry.permits.next_admission = None;
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

/// Marker appended to a truncated target list so the gate fails closed instead
/// of treating an unscanned tail as empty.
///
/// A sentinel cannot collide with a real target: hostnames may not contain `<`
/// or `>`, and a URL containing them would be percent-encoded by the time a
/// scanner read it.
const SCOPE_TRUNCATED_SENTINEL: &str = "<scope:truncated>";

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
///
/// Truncation is *not* silent. The gate is handed [`SCOPE_TRUNCATED_SENTINEL`]
/// so it can escalate; treating the cap as "the rest may be anything, let the
/// tool decide" would let a recon list place every out-of-scope host after line
/// [`MAX_TARGET_FILE_LINES`] and reach it.
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
    let mut truncated = false;
    for line in std::io::BufRead::lines(reader) {
        let Ok(line) = line else {
            // A read error mid-file leaves the remainder unknown, exactly like
            // hitting the cap.
            truncated = true;
            break;
        };
        if out.len() >= MAX_TARGET_FILE_LINES {
            truncated = true;
            break;
        }
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        out.push(line.to_string());
    }
    if truncated {
        out.push(SCOPE_TRUNCATED_SENTINEL.to_string());
    }
    out
}

fn target_list_truncated_message(tool: &str) -> String {
    format!(
        "Engagement scope block: `{tool}` was given a target list longer than the \
         scope gate scans ({MAX_TARGET_FILE_LINES} targets / {MAX_TARGET_FILE_BYTES} bytes), \
         so part of it could not be checked against the authorized scope. \
         Split the list into smaller files, or narrow the scope so the whole list is \
         in-scope, and retry."
    )
}

/// Marker returned when a shell segment's destination is only known at run
/// time (for example `curl "$TARGET"` or `curl $(cat url.txt)`).
///
/// Refusing these is the fail-closed choice: the gate cannot decide whether the
/// destination is in scope, and "unknown" must not silently mean "allowed".
const SHELL_DYNAMIC_TARGET: &str = "<shell:dynamic-target>";

/// Whether a word is a shell expansion whose value is not statically knowable.
fn is_dynamic_word(word: &str) -> bool {
    if word.contains("$(") || word.contains('`') || word.contains('$') {
        return true;
    }
    // Windows `cmd.exe` expands `%NAME%`. Require the closing delimiter so a
    // literal percent in a URL-encoded path (`/a%20b`) is not mistaken for one.
    match (word.find('%'), word.rfind('%')) {
        (Some(first), Some(last)) if last > first + 1 => {
            let name = &word[first + 1..last];
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        }
        _ => false,
    }
}

/// Tools that reach the network through a destination field but are deliberately
/// *not* classified as scanners.
///
/// They are excluded from [`is_network_tool`] because that predicate also drives
/// request pacing and probe-burst accounting: `open report.pdf` is a local file
/// operation and must not consume a network permit or be delayed by
/// `max_rate_rps`. Scope enforcement still applies to the network destination
/// when one is actually present.
const DESTINATION_URL_TOOLS: &[&str] = &["browser", "open"];

/// Network URLs a destination-field tool would actually load.
///
/// Only absolute `http`/`https`/`ws`/`wss` URLs count. A local path, a `file:`
/// URL or a `mailto:` target opens nothing on the network and must not be gated.
fn destination_urls(input: &Value) -> Vec<String> {
    let mut urls = Vec::new();
    let Some(object) = input.as_object() else {
        return urls;
    };
    for key in ["url", "target", "uri", "href", "link"] {
        let Some(value) = object.get(key).and_then(Value::as_str) else {
            continue;
        };
        let Ok(parsed) = url::Url::parse(value.trim()) else {
            continue;
        };
        if matches!(parsed.scheme(), "http" | "https" | "ws" | "wss") {
            urls.push(value.trim().to_string());
        }
    }
    urls
}

/// Network destinations named by one shell command line.
///
/// Only segments whose program is in [`SHELL_NETWORK_PROGRAMS`] contribute, so
/// ordinary local shell work is never gated. Within such a segment:
///
/// * `-l`/`--list` reads the named file with the same bounds as
///   [`targets_from_file`], resolved against the session working directory
///   because that is the directory the tool itself runs in;
/// * tokens consumed by [`SHELL_VALUE_FLAGS`] are data (headers, bodies, output
///   paths, credentials) and contribute nothing;
/// * a remaining operand that resolves to an existing local file is a file
///   operand (`ssh -i key`, `curl --data @body.json`), not a host;
/// * an unexpanded shell variable or command substitution in an operand is
///   reported as [`SHELL_DYNAMIC_TARGET`].
pub(crate) fn shell_network_targets(command: &str, working_dir: Option<&Path>) -> Vec<String> {
    let mut targets = Vec::new();
    for segment in crate::alphacode_command_risk::split_segments(command) {
        let mut words = segment
            .iter()
            .filter(|token| !token.is_operator && !token.is_truncating_redirect_target)
            .map(|token| token.text.as_str())
            .peekable();

        // Skip leading environment assignments (`FOO=bar curl ...`).
        let program = loop {
            let Some(word) = words.next() else {
                break None;
            };
            let is_assignment = word.split_once('=').is_some_and(|(name, _)| {
                !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
            });
            if is_assignment {
                continue;
            }
            break Some(word);
        };
        let Some(program) = program else {
            continue;
        };
        let program = program
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(program)
            .to_ascii_lowercase();
        if !SHELL_NETWORK_PROGRAMS.contains(&program.as_str()) {
            continue;
        }

        let mut after_separator = false;
        while let Some(word) = words.next() {
            if !after_separator {
                if word == "--" {
                    after_separator = true;
                    continue;
                }
                if SHELL_LIST_FLAGS.contains(&word) {
                    if let Some(path) = words.next() {
                        targets.extend(read_target_list(path, working_dir));
                    }
                    continue;
                }
                if SHELL_VALUE_FLAGS.contains(&word) {
                    let _ = words.next();
                    continue;
                }
                if word.starts_with('-') {
                    continue;
                }
            }
            push_shell_operand(word, working_dir, &mut targets);
        }
    }
    targets.sort();
    targets.dedup();
    targets
}

fn push_shell_operand(operand: &str, working_dir: Option<&Path>, targets: &mut Vec<String>) {
    let trimmed = operand.trim();
    if trimmed.is_empty() {
        return;
    }
    if is_dynamic_word(trimmed) {
        targets.push(SHELL_DYNAMIC_TARGET.to_string());
        return;
    }
    // `ssh user@host` names the destination in the authority part.
    let candidate = if trimmed.contains("://") {
        trimmed
    } else if let Some((_, host)) = trimmed.rsplit_once('@') {
        host
    } else {
        trimmed
    };
    if !looks_like_target(candidate) {
        return;
    }
    // A token that names an existing local file is a file argument (identity
    // file, request body, wordlist), not a host. Prefer the evidence on disk
    // over guessing from the spelling: `key.pem` and `api.example.com` are
    // indistinguishable by shape alone.
    if !candidate.contains("://") {
        let local = match working_dir {
            Some(base) => base.join(candidate),
            None => std::path::PathBuf::from(candidate),
        };
        if local.is_file() {
            return;
        }
    }
    targets.push(candidate.trim_start_matches("//").to_string());
}

/// Read a target list named on a shell command line, resolved the way the tool
/// itself would resolve it (relative to the session working directory).
fn read_target_list(path: &str, working_dir: Option<&Path>) -> Vec<String> {
    let candidate = std::path::Path::new(path.trim());
    let resolved = match working_dir {
        Some(base) if candidate.is_relative() => base.join(candidate),
        _ => candidate.to_path_buf(),
    };
    targets_from_file(&resolved.to_string_lossy())
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

fn dynamic_shell_target_message(tool: &str) -> String {
    format!(
        "Engagement scope block: `{tool}` would run a network command whose destination is \
         only known when the shell expands it (a variable or command substitution). The scope \
         gate cannot verify an unknown destination, so it is refused rather than assumed \
         in-scope. Write the target literally, or run the request through `webfetch`/`httpflow`."
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
    // Only network-capable tools and caller-supplied shell commands are
    // subject to the gate.
    let shell = is_shell_tool(tool);
    let destination_tool = DESTINATION_URL_TOOLS.contains(&tool);
    if !shell && !destination_tool && !is_network_tool(tool) {
        return Ok(());
    }
    // A shell command's destinations come from the command text, not from
    // argument names. `bash` is otherwise the cheapest way around this gate:
    // it can invoke every scanner above, plus `curl`, `ssh` and `nc`.
    let targets = if shell {
        let command = input
            .get("command")
            .and_then(Value::as_str)
            .or_else(|| input.as_str())
            .unwrap_or_default();
        shell_network_targets(command, ctx.working_dir.as_deref())
    } else if destination_tool {
        // `browser`/`open` load one URL. Non-network targets (a local path, a
        // `file:` or `mailto:` URI) are not network egress and are not gated.
        destination_urls(input)
    } else {
        all_targets_from_input(tool, input)
    };
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
    for target in targets {
        if target == SHELL_DYNAMIC_TARGET {
            return Err(dynamic_shell_target_message(tool));
        }
        // A scanner driven from a list file it was allowed to open would
        // otherwise reach every host in that file, including any past the scan
        // cap. The cap exists to bound gate cost, not to grant an unchecked
        // tail, so it must escalate rather than silently stop reading.
        if target == SCOPE_TRUNCATED_SENTINEL {
            return Err(target_list_truncated_message(tool));
        }
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
    let policy = session_scope_policy(session_id);
    // Global ceiling: default, or the engagement rule when configured.
    let global_max = policy
        .as_ref()
        .map(|p| p.max_concurrent as usize)
        .filter(|v| *v > 0)
        .unwrap_or(DEFAULT_GLOBAL_MAX_CONCURRENT);

    // Request pacing. `max_rate_rps` is the operator's instruction to the agent
    // about how fast the *target* may be hit; until now nothing in the runtime
    // enforced it, so a policy asking for 1 req/s was honoured only if the
    // model chose to.
    //
    // Applied as a minimum interval between admissions rather than a
    // token-bucket, because that is what a declared `max_rate_rps` means for
    // target safety: no burst above the stated rate. The wait happens *before*
    // taking a concurrency slot so a paced call does not occupy one while
    // sleeping.
    let min_interval = policy
        .as_ref()
        .map(|p| p.max_rate_rps)
        .filter(|rps| *rps > 0)
        .map(|rps| std::time::Duration::from_secs_f64(1.0 / f64::from(rps)));
    if let Some(interval) = min_interval {
        // Reserve this caller's admission slot while holding the write lock.
        //
        // Reading "when was the last call admitted" and sleeping outside the
        // lock let N concurrent callers read the *same* timestamp, sleep for the
        // *same* remaining interval, and then all be admitted together — a burst
        // is exactly what `max_rate_rps` is declared to prevent, and exactly the
        // situation a swarm creates. Reserving the next slot inside the lock
        // makes each admission extend the schedule, so concurrent callers are
        // admitted at `interval` spacing regardless of how many arrive at once.
        let wait = {
            let mut scopes = SESSION_SCOPES.write().unwrap_or_else(|p| p.into_inner());
            match scopes.get_mut(session_id) {
                Some(state) => {
                    let now = std::time::Instant::now();
                    let start = state
                        .permits
                        .next_admission
                        .map(|next| next.max(now))
                        .unwrap_or(now);
                    state.permits.next_admission = Some(start + interval);
                    start.saturating_duration_since(now)
                }
                // No state entry: the policy was cleared between the read above
                // and here. Nothing is being paced, so do not sleep.
                None => std::time::Duration::ZERO,
            }
        };
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }

    acquire_global_slot(session_id, tool, global_max, GLOBAL_SLOT_MAX_WAIT).await;

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

/// How long a call waits for a global concurrency slot before admitting itself.
const GLOBAL_SLOT_MAX_WAIT: std::time::Duration = std::time::Duration::from_secs(30);

/// Retry spacing while waiting for a global slot.
const GLOBAL_SLOT_RETRY_AFTER: std::time::Duration = std::time::Duration::from_millis(10);

/// Take one slot on the process-wide concurrency budget, waiting up to `max_wait`.
///
/// The ceiling exists so a swarm of subagents cannot each run `scanner` at once.
/// This used to give up after 10 000 `yield_now()` calls and return
/// `NetworkPermit::none()` — a permit that releases nothing. The effect was the
/// opposite of the intent: sustained saturation is the *only* condition that can
/// exhaust that loop, and it is exactly when the ceiling must hold, yet at that
/// moment every waiting call silently escaped it. Because `yield_now` does not
/// sleep, it also burned CPU while doing so.
///
/// # Invariant
/// This returns having incremented [`GLOBAL_IN_FLIGHT`] **exactly once**, on
/// every path — the fast path, and the path that gives up waiting. Admission and
/// accounting are therefore inseparable: a call can be late, but it can never be
/// invisible to the ceiling.
///
/// `max_wait` is a parameter rather than a constant so the give-up branch is
/// reachable from a test. It previously read `waited >= Duration::ZERO`, which is
/// true on the *first* failed compare-exchange — the wait never happened at all,
/// and combined with the uncounted permit above, every contended call escaped the
/// ceiling. A 30-second constant made that branch untestable, which is the only
/// reason the bug survived.
async fn acquire_global_slot(
    session_id: &str,
    tool: &str,
    global_max: usize,
    max_wait: std::time::Duration,
) {
    let mut waited = std::time::Duration::ZERO;
    loop {
        let current = GLOBAL_IN_FLIGHT.load(Ordering::SeqCst);
        if current < global_max
            && GLOBAL_IN_FLIGHT
                .compare_exchange_weak(current, current + 1, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
        {
            return;
        }
        if waited >= max_wait {
            // Still saturated after the full wait. Refusing the call would fail a
            // tool the ceiling is only supposed to pace, so admit it — but count
            // it, and say so.
            tracing::warn!(
                session_id,
                tool,
                in_flight = GLOBAL_IN_FLIGHT.load(Ordering::SeqCst),
                global_max,
                "network concurrency ceiling saturated for {max_wait:?}; admitting one extra counted call"
            );
            GLOBAL_IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
            return;
        }
        tokio::time::sleep(GLOBAL_SLOT_RETRY_AFTER).await;
        waited += GLOBAL_SLOT_RETRY_AFTER;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialize the tests that read [`GLOBAL_IN_FLIGHT`] as an absolute value.
    ///
    /// The counter is process-global by design — that *is* the ceiling — so any
    /// other test holding a permit concurrently shifts it, and an assertion of
    /// the form "counter == 1" then fails on a busy machine while the code under
    /// test is correct. Only the tests that treat it as a total need this; the
    /// relative-delta assertions are already stable.
    static PERMIT_COUNT_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Serializes tests that make assertions about how many permits are live.
    ///
    /// [`GLOBAL_IN_FLIGHT`] is process-wide, so two admission tests running on
    /// separate threads observe each other's permits and any absolute or
    /// delta assertion becomes a coin flip. A `tokio` mutex (unlike the std one
    /// above) can be held across `.await`, which is what the async admission
    /// tests need; the synchronous permit tests take it with `blocking_lock`.
    static ADMISSION_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    #[test]
    fn sync_tests_can_serialize_with_async_admission_tests() {
        // Documents that the async lock is usable from a synchronous test; the
        // permit tests rely on this to avoid observing a concurrent admission.
        let _guard = ADMISSION_TEST_LOCK.blocking_lock();
    }

    fn lock_permit_counts_for_tests() -> std::sync::MutexGuard<'static, ()> {
        PERMIT_COUNT_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Restore the process-global in-flight counter when a test finishes.
    ///
    /// [`GLOBAL_IN_FLIGHT`] is process state, not test state. A test that takes a
    /// slot through the raw [`acquire_global_slot`] helper (rather than through a
    /// `NetworkPermit`) must hand it back, or the counter only ever grows. Once
    /// it exceeds a later test's small configured ceiling, that test waits out the
    /// full `GLOBAL_SLOT_MAX_WAIT` for a slot that can never arrive — a 30-second
    /// stall per admission that is indistinguishable from a hang.
    ///
    /// This restores the absolute value rather than subtracting, so it is correct
    /// regardless of how many slots the test took.
    struct InFlightRestore(usize);

    impl Drop for InFlightRestore {
        fn drop(&mut self) {
            GLOBAL_IN_FLIGHT.store(self.0, Ordering::SeqCst);
        }
    }

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

    /// Non-network tools are unaffected even when their arguments contain a URL.
    ///
    /// This previously asserted that `bash` running `curl https://evil.com` was
    /// allowed, on the reasoning that `bash` "is not a network tool". That was a
    /// scope bypass rather than a property worth preserving: `bash` reaches every
    /// scanner and every HTTP client the agent has. The assertion now covers a
    /// tool that genuinely cannot open a socket plus a shell command that stays
    /// local, while shell egress is covered by
    /// `shell_network_segments_are_scope_checked`.
    #[test]
    fn non_network_tool_not_blocked() {
        let id = "local-tool-test";
        set_session_scope(id, Some(policy(&["example.com"], &[], true)));
        for tool in ["read", "write", "edit", "ls", "alphagrep", "todo"] {
            assert!(
                enforce_call(
                    &ctx_for(id),
                    tool,
                    &serde_json::json!({"path": "notes/evil.com.txt"}),
                )
                .is_ok(),
                "local tool `{tool}` must not be scope-gated"
            );
        }
        assert!(
            enforce_call(
                &ctx_for(id),
                "bash",
                &serde_json::json!({"command": "rg -n 'curl https://evil.com' docs/"})
            )
            .is_ok(),
            "a local shell command that only mentions a URL must not be gated"
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

    /// The give-up branch must admit a *counted* slot.
    ///
    /// This is the branch the old code broke twice: the guard was always true
    /// (so it never waited) and the permit it returned released nothing (so the
    /// admitted call vanished from the counter). The pre-existing saturation
    /// test cannot catch that — it releases the held slot after 50ms, so it
    /// exercises the wait-then-succeed path and never reaches this branch at all.
    /// Passing a tiny `max_wait` makes the branch reachable without sleeping 30s.
    #[tokio::test]
    async fn the_give_up_branch_admits_a_counted_slot() {
        let _serial = ADMISSION_TEST_LOCK.lock().await;
        let _restore = InFlightRestore(GLOBAL_IN_FLIGHT.load(Ordering::SeqCst));
        // Hold the test lock only for the synchronous setup; drop before the first await.
        let _counts = lock_permit_counts_for_tests();
        let id = "give-up-branch-test";
        drop(_counts);
        let mut limited = policy(&["example.com"], &[], true);
        limited.max_concurrent = 1;
        set_session_scope(id, Some(limited));

        // Saturate the global budget at the configured limit.
        let held = acquire_network_permit(id, "httpx").await;
        let saturated = GLOBAL_IN_FLIGHT.load(Ordering::SeqCst);

        // A zero wait reaches the give-up branch on the first iteration.
        acquire_global_slot(id, "httpx", saturated, std::time::Duration::ZERO).await;

        // The invariant: the counter must have increased. An uncounted permit
        // (the old `NetworkPermit::none()`) left it unchanged, which is how the
        // ceiling read as enforced while being exceeded.
        assert_eq!(
            GLOBAL_IN_FLIGHT.load(Ordering::SeqCst),
            saturated + 1,
            "giving up on the ceiling must still occupy a counted slot"
        );
        drop(held);
        clear_session_scope(id);
    }

    /// Wait-then-succeed must take the fast path and count exactly once.
    ///
    /// Guards against a regression where the escape branch is taken eagerly
    /// (the old always-true guard) and the slot is double-counted.
    #[tokio::test]
    async fn a_slot_is_taken_exactly_once_when_the_wait_succeeds() {
        let _serial = ADMISSION_TEST_LOCK.lock().await;
        let _restore = InFlightRestore(GLOBAL_IN_FLIGHT.load(Ordering::SeqCst));
        // Hold the test lock only for the synchronous setup; drop before the first await.
        let _counts = lock_permit_counts_for_tests();
        let id = "wait-succeeds-test";
        drop(_counts);
        let mut limited = policy(&["example.com"], &[], true);
        limited.max_concurrent = 1;
        set_session_scope(id, Some(limited));

        // Baseline *before* this test takes its own slot, so the final
        // expectation is "this test ends holding exactly one permit".
        let baseline = GLOBAL_IN_FLIGHT.load(Ordering::SeqCst);
        let held = acquire_network_permit(id, "httpx").await;
        let saturated = GLOBAL_IN_FLIGHT.load(Ordering::SeqCst);
        // A generous bound: with the slot released shortly, the loop must
        // succeed via compare-exchange rather than the escape branch.
        let release = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            drop(held);
        });
        // `saturated` (not +1) is the ceiling, so the fast path fails and the
        // call is forced down the wait loop; the release above then lets the
        // loop succeed via compare-exchange well inside the 5s bound.
        acquire_global_slot(id, "httpx", saturated, std::time::Duration::from_secs(5)).await;
        release.await.unwrap();

        // The held permit is released *during* the wait, and the waiting call
        // then takes its own slot: the net change is zero, and the test ends
        // holding exactly one permit. Asserting the net delta is therefore
        // wrong by construction — the invariant is the absolute end state
        // relative to the baseline, which is why this must be serialized
        // against other admission tests.
        assert_eq!(
            GLOBAL_IN_FLIGHT.load(Ordering::SeqCst),
            baseline + 1,
            "the waiting call must end holding exactly one counted slot"
        );
        clear_session_scope(id);
    }

    #[test]
    fn network_permit_drop_releases_budget() {
        let _counts = lock_permit_counts_for_tests();
        let id = "permit-test";
        set_session_scope(id, Some(policy(&["example.com"], &[], true)));
        let before = GLOBAL_IN_FLIGHT.load(Ordering::SeqCst);
        let permit = acquire_network_permit_sync_for_test(id);
        assert_eq!(GLOBAL_IN_FLIGHT.load(Ordering::SeqCst), before + 1);
        drop(permit);
        assert_eq!(GLOBAL_IN_FLIGHT.load(Ordering::SeqCst), before);
    }

    /// A recon list longer than the gate scans must not hand the tool an
    /// unchecked tail. The cap exists to bound gate cost; before this it also
    /// silently authorized everything past it.
    #[test]
    fn an_oversized_target_list_fails_closed_instead_of_being_partially_checked() {
        let dir = std::env::temp_dir().join(format!(
            "alphacode-scope-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let list = dir.join("targets.txt");
        // One in-scope host, then enough filler to exceed the line cap.
        let mut body = String::from(
            "in-scope.test
",
        );
        for index in 0..(MAX_TARGET_FILE_LINES + 10) {
            body.push_str(&format!(
                "host{index}.in-scope.test
"
            ));
        }
        std::fs::write(&list, body).unwrap();

        let id = "truncated-list-test";
        set_session_scope(id, Some(policy(&["in-scope.test"], &[], true)));
        let result = enforce_call(
            &ctx_for(id),
            "httpx",
            &serde_json::json!({ "list": list.display().to_string() }),
        );
        clear_session_scope(id);
        let _ = std::fs::remove_dir_all(&dir);

        let error = result.expect_err("a truncated target list must not pass unchecked");
        assert!(
            error.contains("longer than the scope gate scans"),
            "unexpected message: {error}"
        );
    }

    /// A list file within the cap is still evaluated host by host, so an
    /// out-of-scope entry anywhere in it is refused.
    #[test]
    fn an_in_cap_target_list_is_checked_host_by_host() {
        let dir = std::env::temp_dir().join(format!(
            "alphacode-scope-test-ok-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let list = dir.join("targets.txt");
        std::fs::write(
            &list,
            "in-scope.test
not-in-scope.test
",
        )
        .unwrap();

        let id = "in-cap-list-test";
        set_session_scope(id, Some(policy(&["in-scope.test"], &[], true)));
        let result = enforce_call(
            &ctx_for(id),
            "httpx",
            &serde_json::json!({ "list": list.display().to_string() }),
        );
        clear_session_scope(id);
        let _ = std::fs::remove_dir_all(&dir);

        assert!(
            result.is_err(),
            "an out-of-scope host in the list must be refused"
        );
    }

    /// Saturation must still produce a *counted* permit.
    ///
    /// The invariant is what the old code broke: it returned
    /// `NetworkPermit::none()` after a bounded spin, and that permit releases
    /// nothing, so a call admitted at saturation was invisible to the counter
    /// and the ceiling read as enforced while being exceeded.
    ///
    /// This cannot be observed by simply acquiring a permit while the ceiling is
    /// free -- that path always counted, before and after the fix. It saturates
    /// the ceiling at its configured limit first, so the acquisition below is
    /// forced down the contended branch.
    #[tokio::test]
    async fn a_permit_admitted_under_saturation_is_still_counted() {
        let _serial = ADMISSION_TEST_LOCK.lock().await;
        // Hold the test lock only for the synchronous setup; drop before the first await.
        let _counts = lock_permit_counts_for_tests();
        let id = "permit-saturation-test";
        drop(_counts);
        // A limit of 1 makes saturation trivially reachable and keeps the test
        // fast: the second acquisition must wait, then admit a counted slot.
        let mut limited = policy(&["example.com"], &[], true);
        limited.max_concurrent = 1;
        set_session_scope(id, Some(limited));

        // Occupy the only slot.
        let held = acquire_network_permit(id, "httpx").await;
        let baseline = GLOBAL_IN_FLIGHT.load(Ordering::SeqCst);

        // Release the held slot from another task shortly after the contended
        // acquisition starts waiting, so it proceeds via the wait path rather
        // than the uncontended fast path.
        let release = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            drop(held);
        });

        let contended = acquire_network_permit(id, "httpx").await;
        release.await.unwrap();

        // The contended acquisition must have taken a counting slot. Counting is
        // observable as "the counter is non-zero while this permit is alive";
        // an uncounted permit would leave it at zero once the other slot drops.
        assert_eq!(
            GLOBAL_IN_FLIGHT.load(Ordering::SeqCst),
            1,
            "a permit admitted under contention must still occupy a counted slot"
        );
        assert!(
            baseline >= 1,
            "the ceiling was saturated before the contended acquisition"
        );

        drop(contended);
        assert_eq!(
            GLOBAL_IN_FLIGHT.load(Ordering::SeqCst),
            0,
            "releasing the contended permit must return the slot"
        );
        clear_session_scope(id);
    }

    /// `max_rate_rps` must actually pace calls.
    ///
    /// It was declared on every policy and enforced nowhere, so a policy asking
    /// for a low rate was honoured only if the model chose to. Two consecutive
    /// admissions under a 1 req/s policy must be at least one interval apart.
    #[tokio::test]
    async fn a_declared_rate_limit_paces_consecutive_admissions() {
        let _serial = ADMISSION_TEST_LOCK.lock().await;
        // Holds two live permits, so it must not overlap the counter tests.
        // Hold the test lock only for the synchronous setup; drop before the first await.
        let _counts = lock_permit_counts_for_tests();
        let id = "rate-pacing-test";
        drop(_counts);
        // `policy()` builds with max_rate_rps = 10 in this module; use 1 here so
        // the interval is a full second and hard to confuse with scheduling noise.
        let mut policy = policy(&["example.com"], &[], true);
        policy.max_rate_rps = 1;
        set_session_scope(id, Some(policy));

        let first = acquire_network_permit(id, "httpx").await;
        let started = std::time::Instant::now();
        let second = acquire_network_permit(id, "httpx").await;
        let elapsed = started.elapsed();

        assert!(
            elapsed >= std::time::Duration::from_millis(900),
            "a 1 req/s policy must space admissions by ~1s; got {elapsed:?}"
        );
        drop(first);
        drop(second);
        clear_session_scope(id);
    }

    /// Synchronous helper for permit lifecycle tests (avoids async in a sync test).
    fn acquire_network_permit_sync_for_test(_session_id: &str) -> NetworkPermit {
        GLOBAL_IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
        NetworkPermit {
            _session_id: _session_id.to_string(),
            _released: false,
        }
    }

    /// `bash` was outside the gate entirely: it is not in `is_network_tool`, so a
    /// scoped session could leave the engagement with `curl https://evil.test`.
    /// The gate now reads shell segments, so an out-of-scope destination in a
    /// network program is refused while ordinary local commands still pass.
    #[test]
    fn shell_network_segments_are_scope_checked() {
        let id = "shell-scope-test";
        set_session_scope(id, Some(policy(&["in-scope.test"], &[], true)));

        for allowed in [
            "curl https://in-scope.test/api",
            "curl -sS -H 'Accept: application/json' https://in-scope.test/x -o out.json",
            "cargo test --lib",
            "git status",
            "rg --files",
        ] {
            assert!(
                enforce_call(
                    &ctx_for(id),
                    "bash",
                    &serde_json::json!({ "command": allowed })
                )
                .is_ok(),
                "expected allowed: {allowed}"
            );
        }

        for denied in [
            "curl https://evil.test/",
            "curl -s https://evil.test/a && curl https://in-scope.test/b",
            "wget -q https://evil.test/payload",
            "nmap -sV evil.test",
            "ssh root@evil.test",
            "echo hi; curl https://evil.test",
        ] {
            let error = enforce_call(
                &ctx_for(id),
                "bash",
                &serde_json::json!({ "command": denied }),
            )
            .expect_err(&format!("expected denial: {denied}"));
            assert!(
                error.contains("Engagement scope block"),
                "expected scope wording for {denied}: {error}"
            );
        }

        clear_session_scope(id);
    }

    /// A destination supplied through shell expansion is not verifiable at gate
    /// time. Treating "unknown" as allowed would make `curl "$TARGET"` a reliable
    /// bypass, so it must be refused with actionable wording.
    #[test]
    fn shell_dynamic_destination_fails_closed() {
        let id = "shell-dynamic-test";
        set_session_scope(id, Some(policy(&["in-scope.test"], &[], true)));
        for command in [
            r#"curl "$TARGET""#,
            "curl ${TARGET}",
            "curl $(cat target.txt)",
        ] {
            let error = enforce_call(
                &ctx_for(id),
                "bash",
                &serde_json::json!({ "command": command }),
            )
            .expect_err(command);
            assert!(
                error.contains("only known when the shell expands it"),
                "{command}: {error}"
            );
        }
        clear_session_scope(id);
    }

    /// Local command segments that merely *mention* a network program in an
    /// argument must not be denied: `grep curl notes.md` calls nothing.
    #[test]
    fn shell_program_must_be_the_segment_program() {
        let id = "shell-program-test";
        set_session_scope(id, Some(policy(&["in-scope.test"], &[], true)));
        for command in [
            "grep -rn curl docs/",
            "echo 'curl https://evil.test'",
            "rg 'nmap -sV' src/",
        ] {
            assert!(
                enforce_call(
                    &ctx_for(id),
                    "bash",
                    &serde_json::json!({ "command": command })
                )
                .is_ok(),
                "expected allowed: {command}"
            );
        }
        clear_session_scope(id);
    }

    /// Browser navigation and `open` also reach the network, and were absent from
    /// the network-tool class. They are now gated on their `url`/`target`.
    #[test]
    fn browser_and_open_destinations_are_gated() {
        let id = "browser-scope-test";
        set_session_scope(id, Some(policy(&["in-scope.test"], &[], true)));

        assert!(
            enforce_call(
                &ctx_for(id),
                "browser",
                &serde_json::json!({"action": "open", "url": "https://in-scope.test/a"})
            )
            .is_ok()
        );
        assert!(
            enforce_call(
                &ctx_for(id),
                "browser",
                &serde_json::json!({"action": "open", "url": "https://evil.test/a"})
            )
            .is_err()
        );
        assert!(
            enforce_call(
                &ctx_for(id),
                "open",
                &serde_json::json!({"action": "open", "target": "https://evil.test/"})
            )
            .is_err()
        );
        // Local targets and non-network schemes open nothing remotely and must
        // not be refused: `open` is also the operator's file/folder viewer.
        for local in [
            serde_json::json!({"action": "open", "target": "./report.pdf"}),
            serde_json::json!({"action": "reveal", "target": "C:/work/notes"}),
            serde_json::json!({"action": "open", "target": "file:///tmp/report.html"}),
            serde_json::json!({"action": "open", "target": "mailto:someone@example.test"}),
        ] {
            assert!(
                enforce_call(&ctx_for(id), "open", &local).is_ok(),
                "local open target must not be scope-gated: {local}"
            );
        }
        // A browser action that carries no URL (snapshot, list_tabs) is local
        // browser control and must not be gated either.
        assert!(
            enforce_call(
                &ctx_for(id),
                "browser",
                &serde_json::json!({"action": "snapshot"})
            )
            .is_ok()
        );
        clear_session_scope(id);
    }

    /// `max_rate_rps` must hold for *concurrent* callers, not only serial ones.
    ///
    /// The previous implementation read "when was the last call admitted" and
    /// slept outside the lock. N concurrent callers read the same timestamp and
    /// were admitted together, producing the exact burst the policy forbids.
    #[tokio::test]
    async fn declared_rate_limit_holds_for_concurrent_admissions() {
        let _serial = ADMISSION_TEST_LOCK.lock().await;
        let id = "rate-concurrency-test";
        // 2 req/s keeps the test short while still being measurable.
        let mut limited = policy(&["example.com"], &[], true);
        limited.max_rate_rps = 2;
        set_session_scope(id, Some(limited));

        let started = std::time::Instant::now();
        let admissions = (0..3).map(|_| {
            let id = id.to_string();
            tokio::spawn(async move { acquire_network_permit(&id, "httpx").await })
        });
        for admission in admissions {
            // Drop each permit as soon as it is granted: this test measures
            // admission *spacing*, and holding permits for its duration would
            // perturb every other admission test in the process.
            drop(admission.await.expect("admission task"));
        }
        let elapsed = started.elapsed();

        // Three admissions at 2 req/s must span at least two intervals (~1s).
        assert!(
            elapsed >= std::time::Duration::from_millis(900),
            "concurrent admissions must be paced, not batched; got {elapsed:?}"
        );

        clear_session_scope(id);
    }
}
