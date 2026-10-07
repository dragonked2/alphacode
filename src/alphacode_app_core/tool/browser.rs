//! Browser automation tool (Firefox + the bundled AlphaCode Browser Agent
//! extension).
//!
//! # Bridge contract
//!
//! Every wire action name and parameter key below was verified against the
//! bundled `AlphaCode-Browser-Agent-1.6.1.xpi` (`background.js` = the
//! tab/native side, `content.js` = the page side). Several are *not* the
//! obvious guess, and a wrong guess fails at runtime as a bridge-side
//! validation error rather than a compile error, so re-verify this table
//! whenever the embedded XPI is bumped.
//!
//! | tool action | bridge action | notes |
//! |---|---|---|
//! | `list_tabs` | `listTabs` | |
//! | `new_tab` | `newSession` | alias of `createTab`; returns a flat `summarizeTab` |
//! | `select_tab` | `setActiveTab` | requires `tabId`; focuses the window unless `focus: false` |
//! | `get_active_tab` | `getActiveTab` | |
//! | `list_frames` | `listFrames` | |
//! | `open` | `navigate` | `newTab` creates a *background* tab (`active: focus === true`) |
//! | `reload` | `reload` | |
//! | `go_back` / `go_forward` | `back` / `forward` | **not** `goBack`/`goForward` |
//! | `close_tab` | `closeTab` | |
//! | `snapshot` | `getContent` + `format: annotated` | |
//! | `get_content` | `getContent` | length cap key is `maxChars` (`textMaxChars` for annotated), **not** `maxLength` |
//! | `interactables` | `getInteractables` | |
//! | `click` / `hover` / `type` | same names | `selector`, `text`, or `x`+`y` all resolve through `resolveElement` |
//! | `fill_form` / `select` | `fillForm` | `<select>` fields route to `selectOption` (`value` / `values`) |
//! | `drag_and_drop` | `drag` | params are `source`/`target`, **not** `sourceSelector`/`targetSelector` |
//! | `wait` | `waitFor` / `waitForStable` | `waitFor` has no quiet-period mode; `waitForStable` is a separate action |
//! | `screenshot` | `screenshot` | returns `dataUrl`; there is **no** `filename` param and **no** `saved` result |
//! | `eval` | `evaluate` | `pageWorld: true` runs in the page's own JS world |
//! | `scroll` | `scroll` | `scrollTo: {x, y}`, `position`, `behavior` |
//! | `upload` | `uploadFile` | needs base64 `files: [{name, type, data}]`; **no** `filePath` |
//! | `press` | `press` | native key handling: real value mutation, Tab/Escape/Enter, `form.requestSubmit()` |
//! | `get_cookies` | `listCookies` | singular cookie actions only; there is no batch `setCookies` |
//! | `set_cookies` | `setCookie` | fanned out, one call per cookie |
//! | `delete_cookie` | `removeCookie` | needs a `url`; `cookies.remove` matches on it |
//! | `list_cookies` | `evaluate` | legacy `document.cookie`; cannot see HttpOnly |
//!
//! # Tab isolation (bug fix #13)
//!
//! `resolveTabId` in the extension falls back to the **user's currently
//! active tab** whenever a request carries no `tabId`, and the bridge's
//! internal tab cache only lives ~900 ms. Left alone, the agent's
//! click/type/eval calls silently land in whatever tab the user is looking
//! at. So: `open` defaults to a new background tab, every session pins the
//! tab it opened or selected and that id is injected into follow-up
//! actions, an untargeted mutating action refuses instead of guessing, and
//! tab-loss recovery never repoints a targeted request at the active tab.
//!
//! # Bugs fixed
//! 1. **The tool did not compile.** Two tests passed a `&mut Map<String,
//!    Value>` to `apply_session_tab_pin`, which takes `&mut Value`, and one
//!    chained `.or_else(|| input.tab_id)` where clippy's
//!    `unnecessary_lazy_evaluations` (CI runs `-D warnings`) rejects it.
//! 2. **`screenshot` never returned an image.** The extension answers with
//!    `{tabId, dataUrl, method}`; there is no `filename` parameter and no
//!    `saved` field. The old code looked for `result.saved`, fell back to a
//!    temp path the bridge never writes, silently failed to read it, and
//!    then reported "Captured browser screenshot to <temp path>" — a file
//!    that did not exist. It now decodes `dataUrl` and attaches the image.
//! 3. **`upload` could not work.** `uploadFile` reads `files`/`file` with
//!    base64 payloads; the `filePath`/`fileName` keys we sent appear
//!    nowhere in the extension, so every upload died with "uploadFile
//!    requires file or files". The file is now read and base64-encoded here.
//! 4. **`max_length` was a no-op.** The content script clamps with
//!    `maxChars` (and `textMaxChars` for the annotated text section);
//!    `maxLength` appears nowhere in the extension.
//! 5. **`wait position='dom-stable'` / `'network-idle'` were no-ops.** Both
//!    were sent as `domStable`/`networkIdle`, which also appear nowhere;
//!    `waitFor` has no quiet-period mode, so it silently degraded into a
//!    `document.readyState` check that returned immediately. Both now route
//!    to the extension's real `waitForStable` action.
//! 6. **`wait` with only `timeout_ms` returned instantly** for the same
//!    reason (the readiness check was already true), despite the schema
//!    promising a fixed delay. A bare `timeout_ms` is now honoured locally.
//! 7. **`press` bypassed the extension's real key handling.** It was routed
//!    to `evaluate` with a hand-rolled script that assigned `el.value`
//!    directly — which React-controlled inputs ignore, since they override
//!    the DOM value setter — and that could not move focus with Tab or
//!    dismiss a dialog with Escape. The extension already ships a native
//!    `press` action built on the prototype value setter; use it.
//! 8. **`type submit=true` was silently dropped** — the content script's
//!    `typeInto` never looks at `submit`. A native Enter press (which does
//!    `form.requestSubmit()`) is now issued after the text lands.
//! 9. **In-place `open` could still hijack the user's tab.** `open` with an
//!    explicit `new_tab: false` and no `tab_id` navigates the active tab
//!    once `newTab: false` reaches `resolveTabId`; that case is now covered
//!    by the isolation guard too.
//! 10. **`screenshot` could raise the user's tab**: the extension's
//!     `captureVisibleTab` fallback does `tabs.update(tabId, {active:true})`,
//!     so an untargeted screenshot was not the pure read it looked like.
//! 11. **`missing_tab_error` false-positived on page errors.** The needle
//!     `"no tab"` matched "no table of contents" and `"error: tab"` matched
//!     "Error: Table not found", which then triggered a pointless tab
//!     re-resolution and replaced the real error with a tab hint. Matching
//!     now guards both token edges.
//! 12. **Unbounded, poison-fragile session pin map.** One entry per session
//!     id was kept for the life of the process and silently dropped every
//!     write if the mutex was ever poisoned. It is now bounded (oldest pin
//!     evicted) and recovers from poisoning.
//! 13. **A wedged bridge process hung the tool forever.** `output()` has no
//!     timeout; the child is now spawned with a deadline and killed.
//! 14. **`open` dumped the whole page into the tool result.** `navigate`
//!     returns a full annotated content dump by default, which landed in
//!     both the rendered text and the metadata on every navigation. It is
//!     now suppressed (`returnContent: false`) and `open` renders a short
//!     summary instead.
//! 15. **Action typos reported the wrong error.** An unknown action ran the
//!     readiness probe first, so on a not-ready bridge the agent was told
//!     "browser not ready" instead of "unknown action". The action is now
//!     validated up front against `KNOWN_ACTIONS`.
//! 16. **`provider_command` params were mutated.** The raw passthrough
//!     payload had the session tab pin injected into it, adding a `tabId`
//!     the caller never asked for; it is now left untouched.
//! 17. **A long `MAX_RETRIES` comment claimed 2 attempts** for a loop that
//!     runs 3; the comment now matches the code.

use super::{Tool, ToolContext, ToolOutput};
use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

pub struct BrowserTool;

static FIREFOX_PROVIDER: FirefoxBridgeProvider = FirefoxBridgeProvider;

/// Tracks consecutive read-only actions to detect verification loops.
///
/// When the agent makes 3+ consecutive read-only calls (snapshot, get_content,
/// list_tabs, etc.) without any state-changing action, it's likely stuck in a
/// verification loop. This counter helps detect that pattern and suggest a
/// different approach.
static READ_ONLY_COUNTER: Mutex<Option<HashMap<String, u32>>> = Mutex::new(None);

/// Maximum consecutive read-only actions before suggesting a different approach.
const MAX_READ_ONLY_ACTIONS: u32 = 3;

/// Check if the current action is read-only and should be counted.
fn is_read_only_action(action: &str) -> bool {
    matches!(
        action,
        "snapshot"
            | "get_content"
            | "list_tabs"
            | "get_active_tab"
            | "list_frames"
            | "interactables"
            | "screenshot"
            | "get_cookies"
            | "list_cookies"
    )
}

/// Check if the current action is state-changing.
fn is_state_changing_action(action: &str) -> bool {
    matches!(
        action,
        "open"
            | "click"
            | "hover"
            | "type"
            | "fill_form"
            | "select"
            | "drag_and_drop"
            | "press"
            | "scroll"
            | "upload"
            | "set_cookies"
            | "delete_cookie"
            | "close_tab"
            | "reload"
            | "go_back"
            | "go_forward"
    )
}

/// Increment the read-only counter for a session and return the new count.
///
/// Returns true if the agent should be warned about a verification loop.
fn increment_read_only_counter(session: &str) -> bool {
    let mut counter = READ_ONLY_COUNTER.lock().unwrap();
    let map = counter.get_or_insert_with(HashMap::new);
    let count = map.entry(session.to_string()).or_insert(0);
    *count += 1;
    *count >= MAX_READ_ONLY_ACTIONS
}

/// Reset the read-only counter for a session.
fn reset_read_only_counter(session: &str) {
    let mut counter = READ_ONLY_COUNTER.lock().unwrap();
    if let Some(map) = counter.as_mut() {
        map.remove(session);
    }
}

/// Get a warning message if the agent is in a verification loop.
fn get_verification_loop_warning(session: &str) -> Option<String> {
    let mut counter = READ_ONLY_COUNTER.lock().unwrap();
    if let Some(map) = counter.as_mut()
        && let Some(count) = map.get(session)
        && *count >= MAX_READ_ONLY_ACTIONS
    {
        return Some(format!(
            "WARNING: You have made {} consecutive read-only actions without any state-changing results. \
             You may be stuck in a verification loop. \
             Try a different approach: \
             1) If you're trying to verify something, make a state-changing action instead. \
             2) If you're exploring, try to find something actionable. \
             3) If you're stuck, report your findings and move on.",
            count
        ));
    }
    None
}

/// Session → agent-owned tab pin.
///
/// The browser bridge resolves an untargeted action to the *user's currently
/// active tab*, and its internal tab cache expires after ~900 ms and rejects
/// inactive tabs. Left unmanaged, the agent's click/type/eval calls silently
/// land on whatever tab the user is looking at. This pin records the tab the
/// agent opened or explicitly selected for its own session so every
/// follow-up action carries an explicit `tabId` and can never hijack the
/// user's tab.
static SESSION_TAB_PINS: Mutex<Option<HashMap<String, PinnedTab>>> = Mutex::new(None);

/// Insertion counter, used only to pick which pin to evict when the map is
/// full. Monotonic, so the smallest value is always the oldest pin.
static PIN_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Upper bound on tracked session pins.
///
/// Sessions are never torn down explicitly, so without a bound the map grows
/// by one entry per session id for the lifetime of the process. Well above any
/// realistic number of concurrently live sessions, low enough to stay trivial.
const MAX_TRACKED_SESSION_TABS: usize = 256;

/// A session's pinned tab plus the sequence number it was claimed at.
#[derive(Clone, Copy)]
struct PinnedTab {
    tab_id: i64,
    seq: u64,
}

/// Lock the pin map, recovering from poisoning.
///
/// A poisoned lock used to be swallowed with `.ok()`, which silently disabled
/// pinning for the rest of the process: every `remember_session_tab` became a
/// no-op and each mutating action then failed the isolation guard even though
/// the agent owned a tab. The map holds only plain data, so the poison is
/// meaningless and the inner value is perfectly usable.
fn session_tab_pins() -> MutexGuard<'static, Option<HashMap<String, PinnedTab>>> {
    SESSION_TAB_PINS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn session_tab_pin(session_id: &str) -> Option<i64> {
    session_tab_pins()
        .as_ref()
        .and_then(|pins| pins.get(session_id).map(|pin| pin.tab_id))
}

fn remember_session_tab(session_id: &str, tab_id: i64) {
    let seq = PIN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut guard = session_tab_pins();
    let pins = guard.get_or_insert_with(HashMap::new);
    if pins.len() >= MAX_TRACKED_SESSION_TABS
        && let Some(oldest) = pins
            .iter()
            .min_by_key(|(_, pin)| pin.seq)
            .map(|(id, _)| id.clone())
    {
        pins.remove(&oldest);
    }
    pins.insert(session_id.to_string(), PinnedTab { tab_id, seq });
}

fn forget_session_tab(session_id: &str, tab_id: i64) {
    let mut guard = session_tab_pins();
    if let Some(pins) = guard.as_mut()
        && pins.get(session_id).map(|pin| pin.tab_id) == Some(tab_id)
    {
        pins.remove(session_id);
    }
}

/// Inject the pinned session tab into `params` unless the caller already
/// supplied an explicit target (tab_id / fork). Returns true when a pin was
/// applied so callers can report the targeting they actually used.
fn apply_session_tab_pin(params: &mut Value, session_id: &str) -> bool {
    let Some(object) = params.as_object_mut() else {
        return false;
    };
    if object.contains_key("tabId") {
        return false;
    }
    if let Some(tab_id) = session_tab_pin(session_id) {
        object.insert("tabId".into(), json!(tab_id));
        return true;
    }
    false
}

impl BrowserTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for BrowserTool {
    fn default() -> Self {
        Self::new()
    }
}

fn browser_tool_description_text() -> &'static str {
    // Kept under the shared tool-description token cap: this text is sent on
    // every request, and `tool_descriptions_stay_under_token_cap` guards it.
    // Per-action detail lives in the `action` parameter description and in
    // runtime errors, which only cost tokens when they actually happen.
    "Browser control for JS-heavy INTERACTIVE pages only (login, dynamic DOM, click-through). \
     For read-only research, listings, docs, or APIs use webfetch/websearch FIRST. \
     Check action='status' ONCE; if not ready fall back immediately and do not loop on setup. \
     'open' (alias 'navigate') needs url and creates a NEW background tab - it never navigates \
     the user's tab, and your later click/type/eval stay scoped to your own tab. Navigation \
     returns no page body; follow with 'snapshot'. \
     Cookies: 'get_cookies' sees HttpOnly, 'set_cookies'/'delete_cookie' write, 'list_cookies' \
     is legacy eval only. \
     Locating elements: 'interactables' (alias 'find') prints index=N per element; pass that \
     index back with click/hover/type rather than guessing a selector, and add include_hidden \
     for collapsed or display:none targets. \
     Eval: end scripts with `return <expr>`; top-level await works; read responses with \
     `await (await fetch(u)).text()`. \
     Also: 'close_tab', 'go_back', 'go_forward', 'reload', 'hover', 'drag_and_drop'."
}

#[derive(Debug, Deserialize)]
struct BrowserInput {
    action: String,
    #[serde(default)]
    browser: Option<String>,
    #[serde(default)]
    provider_action: Option<String>,
    #[serde(default)]
    params: Option<Value>,
    #[serde(default)]
    url: Option<String>,
    #[serde(default)]
    tab_id: Option<i64>,
    #[serde(default)]
    window_id: Option<i64>,
    #[serde(default)]
    frame_id: Option<i64>,
    #[serde(default)]
    all_frames: Option<bool>,
    #[serde(default)]
    selector: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    values: Option<Vec<String>>,
    #[serde(default)]
    contains: Option<String>,
    #[serde(default)]
    script: Option<String>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
    #[serde(default)]
    target_selector: Option<String>,
    /// Nth match of the selector, 0-based.
    ///
    /// `interactables` numbers its results and the extension's `resolveElement`
    /// honours an `index`, but this tool had no way to pass one. An agent
    /// holding "3. [button] Sign in" had no faithful way to express the target,
    /// so it guessed a selector instead — which is how a correct answer turned
    /// into "Element not found".
    #[serde(default)]
    index: Option<i64>,
    /// Include elements that are present but not visible.
    ///
    /// The extension filters candidates through `visible()` unless told
    /// otherwise, so an element inside a collapsed section, a `display:none`
    /// widget, or an off-screen menu item resolved to nothing at all. Reaching
    /// those is sometimes exactly what is needed.
    #[serde(default, alias = "includeHidden")]
    include_hidden: Option<bool>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    wait: Option<bool>,
    #[serde(default)]
    new_tab: Option<bool>,
    #[serde(default)]
    focus: Option<bool>,
    #[serde(default)]
    clear: Option<bool>,
    #[serde(default)]
    submit: Option<bool>,
    #[serde(default)]
    page_world: Option<bool>,
    #[serde(default)]
    position: Option<String>,
    #[serde(default)]
    behavior: Option<String>,
    #[serde(default)]
    timeout_ms: Option<u64>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    max_length: Option<usize>,
    #[serde(default)]
    fields: Option<Vec<BrowserField>>,
    #[serde(default)]
    scroll_to: Option<ScrollTo>,
    #[serde(default)]
    cookie_name: Option<String>,
    #[serde(default)]
    cookie_domain: Option<String>,
    #[serde(default)]
    cookie_path: Option<String>,
    #[serde(default)]
    cookies: Option<Vec<CookieInput>>,
}

#[derive(Debug, Deserialize)]
struct BrowserField {
    selector: String,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    values: Option<Vec<String>>,
    #[serde(default)]
    checked: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct ScrollTo {
    #[serde(default)]
    x: Option<f64>,
    #[serde(default)]
    y: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct CookieInput {
    name: String,
    value: String,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    path: Option<String>,
    #[serde(default)]
    secure: Option<bool>,
    #[serde(default)]
    http_only: Option<bool>,
    #[serde(default)]
    expires: Option<f64>,
}

/// Every action the dispatch table accepts (aliases already normalized).
///
/// Kept separate from the schema `enum` so the pre-flight check in
/// `Tool::execute` can reject a typo *before* the readiness probe runs —
/// otherwise an unknown action on a not-ready bridge reports "browser not
/// ready" and sends the agent off installing a browser it already has. The
/// `schema_action_enum_matches_known_actions` test keeps it in sync with the
/// published schema.
const KNOWN_ACTIONS: &[&str] = &[
    "status",
    "setup",
    "list_tabs",
    "new_tab",
    "select_tab",
    "get_active_tab",
    "list_frames",
    "open",
    "reload",
    "go_back",
    "go_forward",
    "close_tab",
    "snapshot",
    "get_content",
    "interactables",
    "click",
    "hover",
    "type",
    "fill_form",
    "select",
    "drag_and_drop",
    "wait",
    "screenshot",
    "eval",
    "scroll",
    "upload",
    "press",
    "get_cookies",
    "set_cookies",
    "delete_cookie",
    "list_cookies",
    "provider_command",
];

/// Actions that are pure reads and therefore safe to retry on a transient
/// bridge failure. Mutating actions (click/type/press/upload/eval/
/// fill_form/select/scroll/drag/hover) are never retried automatically
/// because a retry after a failed-but-partially-applied mutation could
/// double-submit or double-click.
const RETRYABLE_ACTIONS: &[&str] = &[
    "status",
    "list_tabs",
    "get_active_tab",
    "list_frames",
    "get_content",
    "interactables",
    "snapshot",
    "wait",
    "get_cookies",
    "list_cookies",
];

/// Retries after the first attempt, so the loop below runs up to 3 times.
const MAX_RETRIES: u32 = 2;
const RETRY_BACKOFF: Duration = Duration::from_millis(250);

/// Actions that change page/tab state and are scoped to whichever tab the
/// bridge resolves. When neither the caller nor the session pin names a tab,
/// the bridge would target the user's currently active tab — so these refuse
/// with guidance instead of silently mutating the user's page (bug fix #13).
///
/// `screenshot` is in the list because it is not the pure read it looks like:
/// the extension's `captureVisibleTab` fallback path does
/// `tabs.update(tabId, { active: true })`, i.e. it raises the tab in front of
/// the user. (`open` is handled separately: it is only tab-scoped when the
/// caller asks for an in-place navigation, because its default creates its own
/// tab.)
const TAB_SCOPED_MUTATING_ACTIONS: &[&str] = &[
    "reload",
    "go_back",
    "go_forward",
    "close_tab",
    "click",
    "hover",
    "type",
    "fill_form",
    "select",
    "drag_and_drop",
    "scroll",
    "upload",
    "press",
    "eval",
    "screenshot",
];

#[async_trait]
trait BrowserProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn supported_browsers(&self) -> &'static [&'static str];

    async fn status(&self, ctx: &ToolContext) -> Result<ToolOutput>;
    async fn setup(&self) -> Result<ToolOutput>;
    async fn ensure_ready(&self) -> Result<Option<String>>;
    async fn execute(
        &self,
        action: &str,
        input: &BrowserInput,
        ctx: &ToolContext,
    ) -> Result<ToolOutput>;
}

struct FirefoxBridgeProvider;

#[async_trait]
impl BrowserProvider for FirefoxBridgeProvider {
    fn id(&self) -> &'static str {
        "firefox_agent_bridge"
    }

    fn supported_browsers(&self) -> &'static [&'static str] {
        &["auto", "firefox"]
    }

    async fn status(&self, ctx: &ToolContext) -> Result<ToolOutput> {
        Ok(attach_browser_metadata(
            firefox_status(self, ctx).await?,
            self.id(),
            "firefox",
        ))
    }

    async fn setup(&self) -> Result<ToolOutput> {
        Ok(attach_browser_metadata(
            firefox_setup(self).await?,
            self.id(),
            "firefox",
        ))
    }

    async fn ensure_ready(&self) -> Result<Option<String>> {
        ensure_firefox_ready().await
    }

    async fn execute(
        &self,
        action: &str,
        input: &BrowserInput,
        ctx: &ToolContext,
    ) -> Result<ToolOutput> {
        Ok(attach_browser_metadata(
            execute_firefox_action(self, action, input, ctx).await?,
            self.id(),
            "firefox",
        ))
    }
}

#[async_trait]
impl Tool for BrowserTool {
    fn name(&self) -> &str {
        "browser"
    }

    fn description(&self) -> &str {
        browser_tool_description_text()
    }

    fn parameters_schema(&self) -> Value {
        let mut properties = Map::new();
        properties.insert("intent".into(), super::intent_schema_property());
        properties.insert(
            "action".into(),
            json!({
                "type": "string",
                "enum": [
                    "status", "setup", "list_tabs", "new_tab", "select_tab", "get_active_tab",
                    "list_frames", "open", "reload", "go_back", "go_forward", "close_tab",
                    "snapshot", "get_content", "interactables", "click", "hover", "type",
                    "fill_form", "select", "drag_and_drop", "wait", "screenshot", "eval",
                    "scroll", "upload", "press", "get_cookies", "set_cookies", "delete_cookie",
                    "list_cookies", "provider_command"
                ],
                "description": "Action. Check 'status' first. 'open' takes url, opens a background tab and returns no page body - follow with 'snapshot'. 'wait' with only timeout_ms is a plain delay. 'press' sends a real key; 'type' with submit=true submits. 'upload' takes a local file path. Aliases: navigate/goto=open, evaluate=eval, set_files=upload."
            }),
        );
        properties.insert(
            "browser".into(),
            json!({
                "type": "string",
                "enum": ["auto", "firefox", "chrome", "safari", "edge"],
                "description": "Browser."
            }),
        );
        properties.insert(
            "provider_action".into(),
            json!({
                "type": "string",
                "description": "Provider command name (only used with action='provider_command')."
            }),
        );
        properties.insert(
            "params".into(),
            json!({
                "type": "object",
                "description": "Raw provider params (only used with action='provider_command')."
            }),
        );
        for (name, schema) in [
            ("url", json!({"type": "string"})),
            ("tab_id", json!({"type": "integer"})),
            (
                "window_id",
                json!({"type": "integer", "description": "Scope the action to one browser window when multiple agents share the browser."}),
            ),
            ("frame_id", json!({"type": "integer"})),
            ("all_frames", json!({"type": "boolean"})),
            (
                "selector",
                json!({"type": "string", "description": "CSS selector for the target element. Used by click, hover, type, select, scroll, and as the drag source for drag_and_drop."}),
            ),
            (
                "text",
                json!({"type": "string", "description": "Text to type, or text to match against for click/wait 'contains text' matching."}),
            ),
            (
                "contains",
                json!({"type": "string", "description": "Alias for text-matching on click/wait when a literal selector isn't known."}),
            ),
            ("script", json!({"type": "string"})),
            (
                "key",
                json!({"type": "string", "description": "Key name for 'press', e.g. 'Enter', 'Tab', 'Escape', 'Backspace', 'ArrowDown', or a single printable character."}),
            ),
            ("x", json!({"type": "number"})),
            ("y", json!({"type": "number"})),
            (
                "target_selector",
                json!({"type": "string", "description": "For drag_and_drop: CSS selector of the drop target. 'selector' is the drag source."}),
            ),
            (
                "index",
                json!({"type": "integer", "description": "0-based index of the selector match to act on. 'interactables' numbers its results, so index=2 targets the 3rd entry — use this instead of guessing a selector."}),
            ),
            (
                "include_hidden",
                json!({"type": "boolean", "description": "Act on elements that exist but are not visible (collapsed menus, display:none widgets, off-screen items). Default false."}),
            ),
            ("wait", json!({"type": "boolean"})),
            (
                "new_tab",
                json!({"type": "boolean", "description": "For open: true (default) opens the url in a new background tab; false navigates the agent's own current tab instead. Never used to target the user's active tab."}),
            ),
            (
                "focus",
                json!({"type": "boolean", "description": "For select_tab/new_tab: raise the tab in front of the user (default false — background). Only set true when the user explicitly wants to watch this tab."}),
            ),
            ("clear", json!({"type": "boolean"})),
            ("submit", json!({"type": "boolean"})),
            (
                "page_world",
                json!({"type": "boolean", "description": "For eval: run in the page's own JS world (true) to access page-defined functions/variables, form handlers, and framework state. The default isolated world cannot see anything the page itself defined — if the script touches page JS, set this true."}),
            ),
            ("position", json!({"type": "string"})),
            ("behavior", json!({"type": "string"})),
            ("timeout_ms", json!({"type": "integer"})),
            (
                "path",
                json!({"type": "string", "description": "Local file path for 'upload'."}),
            ),
            (
                "max_length",
                json!({"type": "integer", "description": "Max characters for get_content in any format (default 60000 for format='html', unlimited for text formats unless set)."}),
            ),
            (
                "value",
                json!({"type": "string", "description": "Option value for select (text is also accepted). For single-select only; use 'values' for multi-select."}),
            ),
            (
                "values",
                json!({"type": "array", "items": {"type": "string"}, "description": "Option values for a multi-select <select multiple>. Selects all matching options."}),
            ),
            (
                "cookie_name",
                json!({"type": "string", "description": "Cookie name, for delete_cookie."}),
            ),
            ("cookie_domain", json!({"type": "string"})),
            ("cookie_path", json!({"type": "string"})),
            (
                "cookies",
                json!({
                    "type": "array",
                    "description": "One or more cookies to set, for action='set_cookies'.",
                    "items": {
                        "type": "object",
                        "required": ["name", "value"],
                        "properties": {
                            "name": {"type": "string"},
                            "value": {"type": "string"},
                            "domain": {"type": "string"},
                            "path": {"type": "string"},
                            "secure": {"type": "boolean"},
                            "http_only": {"type": "boolean"},
                            "expires": {"type": "number", "description": "Unix timestamp (seconds) expiry."}
                        }
                    }
                }),
            ),
        ] {
            properties.insert(name.into(), schema);
        }
        properties.insert(
            "format".into(),
            json!({
                "type": "string",
                "enum": ["annotated", "text", "textFast", "html", "title"],
                "description": "Format."
            }),
        );
        properties.insert(
            "fields".into(),
            json!({
                "type": "array",
                "description": "Form fields for fill_form / select.",
                "items": {
                    "type": "object",
                    "required": ["selector"],
                    "properties": {
                        "selector": { "type": "string" },
                        "value": { "type": "string" },
                        "values": { "type": "array", "items": {"type": "string"}, "description": "For a multi-select field within fill_form." },
                        "checked": { "type": "boolean" }
                    }
                }
            }),
        );
        properties.insert(
            "scroll_to".into(),
            json!({
                "type": "object",
                "properties": {
                    "x": { "type": "number" },
                    "y": { "type": "number" }
                }
            }),
        );
        Value::Object(Map::from_iter([
            ("type".into(), json!("object")),
            ("required".into(), json!(["action"])),
            ("properties".into(), Value::Object(properties)),
        ]))
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        // Keep the raw input for the error path: a bare "invalid input" with
        // no field-level reason sends the agent guessing (and retrying blind).
        let params: BrowserInput = match serde_json::from_value(input.clone()) {
            Ok(params) => params,
            Err(error) => return Err(invalid_input_error(&input, &error)),
        };
        let provider = resolve_provider(params.browser.as_deref())?;
        let action = normalize_action(&params.action);

        // Validate the action *before* the readiness probe: `ensure_ready`
        // talks to Firefox and can take seconds, and when the bridge is not
        // running it reports "browser not ready" for every action — including
        // typos. A misspelled action used to hide behind that message and
        // send the agent into a pointless install loop.
        if !matches!(action, "status" | "setup") && !KNOWN_ACTIONS.contains(&action) {
            anyhow::bail!(unsupported_action_message(action));
        }

        // Verification loop detection: track consecutive read-only actions
        let session = ctx.session_id.as_str();
        let loop_warning = if is_state_changing_action(action) {
            reset_read_only_counter(session);
            None
        } else if is_read_only_action(action) {
            let should_warn = increment_read_only_counter(session);
            if should_warn {
                get_verification_loop_warning(session)
            } else {
                None
            }
        } else {
            None
        };

        match action {
            "status" => provider.status(&ctx).await,
            "setup" => provider.setup().await,
            other => {
                let setup_message = provider.ensure_ready().await?;
                let output = provider.execute(other, &params, &ctx).await?;
                let output = match setup_message {
                    Some(message) if !message.is_empty() => prepend_setup_message(output, &message),
                    _ => output,
                };
                // Append verification loop warning if detected
                if let Some(warning) = loop_warning {
                    Ok(ToolOutput::new(format!("{}\n\n{}", output.output, warning)))
                } else {
                    Ok(output)
                }
            }
        }
    }
}

/// Field-level reason plus a truncated echo of what was received, so a
/// mistyped browser argument fails with something actionable instead of a
/// bare "invalid input".
fn invalid_input_error(input: &Value, error: &serde_json::Error) -> anyhow::Error {
    let raw = serde_json::to_string(input).unwrap_or_default();
    let snippet: String = raw.chars().take(500).collect();
    anyhow::anyhow!(
        "Invalid browser tool input: {error}. Received{}: {snippet}",
        if raw.chars().count() > 500 {
            " (truncated)"
        } else {
            ""
        },
    )
}

/// Map action spellings models actually emit onto the canonical ones.
///
/// `navigate` was documented as an alias for `open` in the description but
/// was never accepted anywhere in the dispatch code — it would have fallen
/// through to the "Unsupported browser action" bail.
///
/// The rest are the Playwright / Puppeteer / Selenium names. A model
/// uploading a file writes `set_files` or `set_input_files` (Playwright's
/// `setInputFiles`) and was told the action did not exist, even though
/// `upload` does exactly that. Accepting the names outright is better than
/// rejecting a correct intent over vocabulary.
fn normalize_action(action: &str) -> &str {
    match action {
        "navigate" | "goto" | "visit" => "open",
        "back" => "go_back",
        "forward" => "go_forward",
        "close" => "close_tab",
        "content" | "text" | "html" => "get_content",
        "evaluate" | "js" | "javascript" | "run_script" => "eval",
        "press_key" | "key" | "keypress" => "press",
        "fill" => "fill_form",
        "set_files" | "setFiles" | "set_input_files" | "setInputFiles" | "upload_file"
        | "upload_files" | "uploadFile" | "attach_file" | "file_upload" => "upload",
        "frame_list" => "list_frames",
        "tab_list" => "list_tabs",
        "new_browser_tab" => "new_tab",
        "close_browser_tab" => "close_tab",
        // Locating an element is what `interactables` does. A model that asks
        // for `find` / `search` / `locate` wants the element list, not a
        // rejection: the previous behaviour refused the call outright, the
        // agent retried with a different invented name, and the click it was
        // trying to make never happened.
        "find" | "search" | "locate" | "find_element" | "query" | "elements" | "list_elements"
        | "list_interactables" => "interactables",
        other => other,
    }
}

fn merge_into_metadata_object(existing: Option<Value>) -> Map<String, Value> {
    match existing {
        Some(Value::Object(map)) => map,
        Some(other) => {
            let mut map = Map::new();
            map.insert("result".into(), other);
            map
        }
        None => Map::new(),
    }
}

fn prepend_setup_message(mut output: ToolOutput, message: &str) -> ToolOutput {
    output.output = format!("{}\n\n{}", message, output.output);
    if output.title.is_none() {
        output.title = Some("browser".to_string());
    }

    let mut metadata = merge_into_metadata_object(output.metadata.take());
    metadata.insert("setup_ran".into(), json!(true));
    output.metadata = Some(Value::Object(metadata));
    output
}

fn attach_browser_metadata(
    mut output: ToolOutput,
    backend: &'static str,
    browser: &'static str,
) -> ToolOutput {
    let mut metadata = merge_into_metadata_object(output.metadata.take());
    metadata.insert("backend".into(), json!(backend));
    metadata.insert("browser".into(), json!(browser));
    output.metadata = Some(Value::Object(metadata));
    output
}

fn resolve_provider(browser: Option<&str>) -> Result<&'static dyn BrowserProvider> {
    let browser = browser.unwrap_or("auto");
    if FIREFOX_PROVIDER.supported_browsers().contains(&browser) {
        return Ok(&FIREFOX_PROVIDER);
    }

    anyhow::bail!(
        "Browser backend '{}' is not wired into the built-in browser tool yet. Use auto/firefox for now. Supported: {}",
        browser,
        FIREFOX_PROVIDER.supported_browsers().join(", ")
    )
}

async fn firefox_status(
    provider: &FirefoxBridgeProvider,
    _ctx: &ToolContext,
) -> Result<ToolOutput> {
    let status = crate::browser::ensure_browser_ready_noninteractive().await?;
    let mut metadata = json!({
        "setup_complete": status.setup_complete,
        "binary_installed": status.binary_installed,
        "responding": status.responding,
        "compatible": status.compatible,
        "missing_actions": status.missing_actions,
        "ready": status.ready,
        "backend": if status.binary_installed || status.setup_complete || status.ready {
            provider.id()
        } else {
            "unconfigured"
        },
        "browser": "firefox",
    });

    if status.ready {
        return Ok(
            ToolOutput::new("Browser bridge is installed and responding.")
                .with_title("browser status")
                .with_metadata(metadata),
        );
    }

    if status.responding && !status.compatible {
        let missing = if status.missing_actions.is_empty() {
            "unknown required actions".to_string()
        } else {
            status.missing_actions.join(", ")
        };
        return Ok(ToolOutput::new(format!(
            "Browser bridge is connected, but the live Firefox extension is out of date and does not support required actions: {}. Use action='setup' only to repair or update the existing install. You do not need to run setup before every browser task.",
            missing
        ))
        .with_title("browser status")
        .with_metadata(metadata));
    }

    if status.binary_installed {
        return Ok(ToolOutput::new(
            "Browser bridge binaries are installed, but the live bridge is not responding. Use action='setup' only if you want to repair the existing install. You do not need to run setup before every browser task.",
        )
        .with_title("browser status")
        .with_metadata(metadata));
    }

    metadata["backend"] = json!("unconfigured");
    Ok(ToolOutput::new(
        "Browser bridge is not installed yet. Use action='setup' only for first-time install or repair. You do not need to run setup before every browser task.",
    )
    .with_title("browser status")
    .with_metadata(metadata))
}

async fn firefox_setup(provider: &FirefoxBridgeProvider) -> Result<ToolOutput> {
    let log = crate::browser::ensure_browser_setup().await?;
    let status = crate::browser::ensure_browser_ready_noninteractive().await?;
    let title = if status.ready {
        "browser setup"
    } else {
        "browser setup (incomplete)"
    };
    Ok(ToolOutput::new(log).with_title(title).with_metadata(json!({
        "setup_complete": status.setup_complete,
        "binary_installed": status.binary_installed,
        "responding": status.responding,
        "compatible": status.compatible,
        "missing_actions": status.missing_actions,
        "ready": status.ready,
        "backend": provider.id(),
        "browser": "firefox"
    })))
}

async fn ensure_firefox_ready() -> Result<Option<String>> {
    // A setup marker only proves that installation once completed. Always
    // verify the live bridge before launching an action because Firefox or
    // the extension may have stopped or become incompatible since then.
    let status = crate::browser::ensure_browser_ready_noninteractive().await?;
    if status.ready {
        return Ok(None);
    }

    let mut message = String::from(
        "Browser automation is not ready yet. Check action='status' ONCE to confirm, then STOP retrying browser open/setup (max 1 setup per session). For read-only research/listings/docs fall back immediately to webfetch/websearch — do not loop on browser actions until ready.\n",
    );
    if !status.binary_installed {
        message.push_str("Browser bridge binary is not installed yet.\n");
    } else if status.responding && !status.compatible {
        message.push_str("Browser bridge is connected, but the live Firefox extension is missing required actions.");
        if !status.missing_actions.is_empty() {
            message.push_str(&format!(
                " Missing actions: {}.",
                status.missing_actions.join(", ")
            ));
        }
        message.push('\n');
    } else {
        message.push_str("Browser bridge binaries are installed, but the live Firefox bridge is not responding.\n");
    }
    message.push_str(
        "Normal browser tool calls will not reopen the installer automatically anymore. Do not retry browser actions until status reports ready. Use webfetch for direct HTTP reads and websearch to discover correct URLs; only return to browser for JS-heavy interactive pages webfetch cannot render.",
    );
    anyhow::bail!(message)
}

async fn execute_firefox_action(
    _provider: &FirefoxBridgeProvider,
    action: &str,
    input: &BrowserInput,
    ctx: &ToolContext,
) -> Result<ToolOutput> {
    // Bug fix #13: scope every untargeted action to the tab this session
    // owns (opened via new_tab/open, or explicitly select_tab'd). Without
    // the pin the bridge resolves untargeted actions against the user's
    // currently active tab, so the agent would read, click, and type in the
    // tab the user is working in.
    let (bridge_action, mut bridge_params, title) = bridge_request(action, input)?;

    // `uploadFile` needs the file bytes inline (base64), so the payload is
    // built here rather than in the pure `bridge_request` mapping. Done
    // before the pin so the injected `tabId` lands in the final params.
    if action == "upload" {
        let mut upload = upload_params(input).await?;
        apply_common_targeting(&mut upload, input);
        bridge_params = Value::Object(upload);
    }

    // `provider_command` is a raw passthrough: the caller owns the whole wire
    // payload, so the pin must not inject a `tabId` it never asked for.
    let pinned = if action == "provider_command" {
        false
    } else {
        apply_session_tab_pin(&mut bridge_params, &ctx.session_id)
    };

    // Isolation guard: a mutating action with no explicit target and no
    // session pin would land on the user's currently active tab. Refuse and
    // tell the agent how to get its own tab instead.
    //
    // `open` is only tab-scoped for an *in-place* navigation. With
    // `new_tab: false` and no `tabId`, the extension takes the
    // `resolveTabId({})` branch and navigates whatever tab the user is
    // looking at — exactly the hijack this guard exists to stop. The default
    // (`newTab: true`) creates its own tab, so it stays allowed.
    let in_place_navigation =
        action == "open" && bridge_params.get("newTab") == Some(&json!(false));
    if !pinned
        && bridge_params.get("tabId").and_then(Value::as_i64).is_none()
        && (TAB_SCOPED_MUTATING_ACTIONS.contains(&action) || in_place_navigation)
    {
        let remedy = if in_place_navigation {
            "action='open' with new_tab=true (the default) creates a new tab, or pass the tab_id you mean to reuse"
        } else {
            "Open your own tab first with action='open' (creates a new tab by default), or pick one explicitly via action='list_tabs' + tab_id"
        };
        anyhow::bail!(
            "browser action '{action}' has no target tab and would act on the user's currently active tab. {remedy}. Never act on the user's tab unless they asked for it."
        );
    }

    if bridge_action == "screenshot" {
        return screenshot_via_bridge(&bridge_params, title, ctx).await;
    }

    // The content script's `waitFor` has no "just sleep" mode: with no
    // selector/text/url target its check falls through to
    // `document.readyState`, which is normally already true, so a bare
    // `timeout_ms` returned in ~0 ms despite the schema promising a fixed
    // delay. Honor it locally instead — no bridge round-trip, no page access.
    if is_fixed_delay_wait(input) {
        let waited_ms = input.timeout_ms.unwrap_or(0).min(MAX_FIXED_DELAY_WAIT_MS);
        if waited_ms > 0 {
            tokio::time::sleep(Duration::from_millis(waited_ms)).await;
        }
        return Ok(render_browser_output(
            action,
            title,
            json!({"waited": true, "ms": waited_ms, "source": "local"}),
        ));
    }

    // The wire has no batch cookie-set: one `setCookie` call per cookie.
    // Writes are never retried (a retry after a partial apply would duplicate
    // work). All cookies are validated up front so a bad entry fails before
    // anything is written instead of leaving a partial apply.
    if action == "set_cookies" {
        let cookies = input.cookies.as_ref().ok_or_else(|| {
            anyhow::anyhow!("set_cookies requires 'cookies' (array of {{name, value, ...}})")
        })?;
        let mut calls = Vec::with_capacity(cookies.len());
        for cookie in cookies {
            let mut params = set_cookie_params(cookie, input.url.as_deref())?;
            apply_common_targeting(&mut params, input);
            calls.push((cookie.name.clone(), Value::Object(params)));
        }
        // No tab pin here: `cookieAction` addresses cookies by url/domain and
        // never resolves a tab, so a `tabId` would be dead weight.
        let mut applied = Vec::with_capacity(calls.len());
        for (name, params) in calls {
            let one = firefox_run_bridge_command("setCookie", params, ctx)
                .await
                .map_err(|e| enrich_browser_error(action, e))?;
            applied.push(json!({"name": name, "result": one}));
        }
        return Ok(render_browser_output(
            action,
            title,
            json!({"set": applied}),
        ));
    }

    // `type`'s `submit` flag is silently ignored by the content script
    // (`typeInto` never reads it), so capture the targeting keys now — before
    // `bridge_params` is moved into the bridge call — and issue a native Enter
    // press afterwards, which is what actually calls `form.requestSubmit()`.
    let submit_press = if action == "type" && input.submit == Some(true) {
        let mut enter = Map::new();
        enter.insert("key".into(), json!("Enter"));
        enter.insert("submit".into(), json!(true));
        for key in ["tabId", "frameId", "allFrames"] {
            if let Some(value) = bridge_params.get(key) {
                enter.insert(key.into(), value.clone());
            }
        }
        Some(Value::Object(enter))
    } else {
        None
    };

    let mut result =
        firefox_run_bridge_command_with_retry(action, &bridge_action, bridge_params, ctx)
            .await
            .map_err(|e| enrich_browser_error(action, e))?;

    if let Some(press_params) = submit_press {
        let pressed = firefox_run_bridge_command("press", press_params, ctx)
            .await
            .map_err(|e| enrich_browser_error("press", e))?;
        if let Some(object) = result.as_object_mut() {
            object.insert(
                "submit".into(),
                json!({"submitted": true, "result": pressed}),
            );
        }
    }

    // Learn/maintain the session tab pin from the actions that create or
    // retarget the session's tab, and drop it when that tab is closed.
    match action {
        "new_tab" | "open" | "select_tab" => {
            if let Some(tab_id) = tab_id_from_value(&result) {
                remember_session_tab(&ctx.session_id, tab_id);
            }
        }
        "close_tab" => {
            let closed = tab_id_from_value(&result)
                .or(input.tab_id)
                .or_else(|| session_tab_pin(&ctx.session_id));
            if let Some(tab_id) = closed {
                forget_session_tab(&ctx.session_id, tab_id);
            }
        }
        _ => {}
    }

    let result = if pinned {
        annotate_pinned_targeting(result)
    } else {
        result
    };
    Ok(render_browser_output(action, title, result))
}

/// Ceiling for a locally honored `wait` delay, so a mistyped `timeout_ms`
/// cannot pin a tool call open for hours.
const MAX_FIXED_DELAY_WAIT_MS: u64 = 120_000;

/// True when `wait` asked for nothing but a delay.
///
/// The extension's `waitFor` resolves a targetless request against
/// `document.readyState`, so `timeout_ms` on its own never actually waits.
fn is_fixed_delay_wait(input: &BrowserInput) -> bool {
    input.timeout_ms.is_some()
        && input.selector.is_none()
        && input.text.is_none()
        && input.contains.is_none()
        && input.position.is_none()
}

/// Record in the tool metadata that an action was scoped to the session's
/// own tab, so operators can see the agent is not touching their tab.
fn annotate_pinned_targeting(mut result: Value) -> Value {
    if let Some(object) = result.as_object_mut() {
        object.insert("pinnedTab".to_string(), json!(true));
    }
    result
}

/// Turns common raw bridge/eval failures into actionable hints instead of
/// leaving the agent to guess. Only `eval` produces JavaScript syntax/runtime
/// errors now — `press` uses the extension's native key handling — so the
/// eval-specific hints are keyed on the action or on the bridge's own
/// `Evaluate error:` prefix.
fn enrich_browser_error(action: &str, err: anyhow::Error) -> anyhow::Error {
    let msg = err.to_string();
    let lower = msg.to_ascii_lowercase();
    if missing_tab_error(&msg) {
        return anyhow::anyhow!(
            "{msg}\n\nHint: the browser tab changed or closed (often after a bridge/server restart). \
             Run action='list_tabs', then action='select_tab' with a current tab_id, or action='open' \
             to create a fresh tab before retrying. \
             IMPORTANT: Always reuse existing authenticated tabs instead of opening new ones. \
             Check 'list_tabs' first to find tabs that are already logged in."
        );
    }
    if lower.contains("connection refused")
        || (lower.contains("websocket") && lower.contains("no connection could be made"))
    {
        return anyhow::anyhow!(
            "{msg}\n\nHint: the Firefox bridge is not connected. Run action='status' once, then action='setup' if it is not ready; avoid repeatedly retrying the browser action."
        );
    }
    if action == "eval" || bridge_is_evaluate(&msg) {
        if lower.contains("await is only valid in async") {
            return anyhow::anyhow!(
                "{msg}\n\nHint: top-level `await` is supported by a current bridge, but this error came from an old extension. Update with action='setup', or wrap manually: `return (async () => {{ ... return await fetch(u).then(r => r.text()); }})()`."
            );
        }
        if lower.contains("illegal return statement")
            || lower.contains("'return' outside of function")
            || lower.contains("\"return\" outside of function")
        {
            return anyhow::anyhow!(
                "{msg}\n\nHint: this Firefox extension used an older raw-eval path that rejected `return` and top-level `await`. Run browser action='setup' to install the current extension; AlphaCode also wraps new eval calls for backward compatibility."
            );
        }
        if lower.contains("unmatched") && lower.contains("regular expression") {
            return anyhow::anyhow!(
                "{msg}\n\nHint: a regex literal has an unescaped `)` or `/`. Escape literal parens (`\\(`, `\\)`) and slashes (`\\/`) inside `/.../`, or use `new RegExp(\"...\")` to avoid literal parsing."
            );
        }
        if lower.contains("is not a function") {
            return anyhow::anyhow!(
                "{msg}\n\nHint: `.text()` exists on fetch Response objects, not on plain strings/arrays. Use `await (await fetch(url)).text()`, and for many URLs use `await Promise.all(urls.map(u => fetch(u).then(r => r.text())))` — `.map(...).catch` is invalid because arrays have no `.catch`."
            );
        }
        if lower.contains("is not defined") || lower.contains("can't find variable") {
            return anyhow::anyhow!(
                "{msg}\n\nHint: the script referenced something the isolated eval world cannot see (a page function, variable, or form handler). Retry the same script with 'page_world': true so it runs in the page's own JS context; if the error persists there, the symbol genuinely does not exist on the page."
            );
        }
        if lower.contains("expected expression") {
            return anyhow::anyhow!(
                "{msg}\n\nHint: JS syntax error — usually a stray `)` / `}}` or an unfinished arrow function. End eval scripts with `return <value>` and check bracket balance. Prefer small scripts: snapshot first, then one focused eval."
            );
        }
    }
    if (action == "select" || action == "fill_form") && lower.contains("no element") {
        return anyhow::anyhow!(
            "{msg}\n\nHint: the selector didn't match anything at eval time. Run action='interactables' or a snapshot right before select/fill_form — the element may not exist yet (still loading) or the selector may target a wrapper rather than the actual <select>/<input>."
        );
    }
    if (action == "click" || action == "hover" || action == "type")
        && lower.contains("element not found")
    {
        return anyhow::anyhow!(
            "{msg}\n\nHint: nothing on the page matched the given selector/text/coordinates. \
             Run action='interactables' (or 'snapshot') first — 'interactables' prints an 'index=N' \
             for every interactive element, and passing that index back with the selector (or \
             'contains' for text) is more reliable than a hand-written selector. If the element is \
             present but hidden (collapsed menu, display:none), add include_hidden=true. If the page \
             was still loading, action='wait' for the text before retrying. Elements inside iframes \
             need action='list_frames' plus frame targeting."
        );
    }
    if action == "drag_and_drop" && lower.contains("unknown action") {
        return anyhow::anyhow!(
            "{msg}\n\nHint: drag_and_drop needs bridge support for a 'drag' action (extension 1.6.0+). Run action='status' to confirm, then action='setup' to update; until then, fall back to eval with manual dragstart/dragover/drop DispatchEvent sequences."
        );
    }
    // The extension rejects an `uploadFile` with no inline file specs, which is
    // the signature of a payload that never carried the bytes. Name the
    // requirement instead of leaving the agent to re-read the bridge source.
    if action == "upload" && lower.contains("requires file or files") {
        return anyhow::anyhow!(
            "{msg}\n\nHint: the bridge expects the file's bytes inline as 'files: [{{name, type, data}}]' with base64 data (it cannot read the agent's filesystem). Alphacode now sends that automatically; if you reached this via action='provider_command', pass 'files' yourself rather than a filesystem path."
        );
    }
    if action == "upload" && (lower.contains("no file input") || lower.contains("no such file")) {
        return anyhow::anyhow!(
            "{msg}\n\nHint: the page has no usable <input type=\"file\"> (it may be hidden, disabled, or inside a frame). Run action='interactables' to find a file input, pass its 'selector', and target the right frame via action='list_frames' + frame_id."
        );
    }
    err
}

fn bridge_is_evaluate(msg: &str) -> bool {
    msg.contains("Evaluate error")
}

/// Convert the public eval forms (`expr`, `return expr`, and top-level
/// `await`) into one expression that older bridge builds can evaluate too.
///
/// Firefox bridge 1.4.1 called raw `eval(code)`, where `return 1` and
/// top-level `await` are syntax errors. The returned async IIFE is an
/// expression, so old bridges can `await` it while current bridges still
/// receive the user's original semantics inside the function body.
fn bridge_compatible_eval_script(script: &str) -> String {
    let trimmed = script.trim();
    let source = if trimmed
        .get(.."javascript:".len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("javascript:"))
    {
        trimmed.get("javascript:".len()..).unwrap_or(trimmed)
    } else {
        trimmed
    }
    .trim();
    let starts_with_keyword = |keyword: &str| {
        source.strip_prefix(keyword).is_some_and(|rest| {
            rest.chars()
                .next()
                .is_none_or(|ch| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '$')
        })
    };
    const STATEMENT_KEYWORDS: &[&str] = &[
        "return", "await", "let", "const", "var", "if", "for", "while", "function", "class",
        "throw", "switch", "try", "do",
    ];
    let starts_with_statement = STATEMENT_KEYWORDS
        .iter()
        .any(|keyword| starts_with_keyword(keyword))
        || matches!(source.chars().next(), Some('{' | ';' | '/'))
        || source.starts_with("\"use strict\"")
        || source.starts_with("'use strict'");
    let body = if starts_with_statement {
        source.to_string()
    } else {
        format!("return (\n{source}\n);")
    };
    format!("(async () => {{\n{body}\n}})()")
}

/// Which bridge action backs a given logical action.
///
/// See the bridge contract table at the top of this file: the wire names are
/// not all derivable from the camelCase convention, and a wrong one fails as
/// "Unknown action" at runtime.
fn bridge_action_for(action: &str, input: &BrowserInput) -> Result<Cow<'static, str>> {
    let mapped = match action {
        "list_tabs" => "listTabs",
        "new_tab" => "newSession",
        "select_tab" => "setActiveTab",
        "get_active_tab" => "getActiveTab",
        "list_frames" => "listFrames",
        "open" => "navigate",
        "reload" => "reload",
        "go_back" => "back",
        "go_forward" => "forward",
        "close_tab" => "closeTab",
        "snapshot" => "getContent",
        "get_content" => "getContent",
        "interactables" => "getInteractables",
        "click" => "click",
        "hover" => "hover",
        "type" => "type",
        "fill_form" => "fillForm",
        "select" => "fillForm",
        "drag_and_drop" => "drag",
        "wait" => wait_bridge_action(input),
        "screenshot" => "screenshot",
        "eval" => "evaluate",
        "scroll" => "scroll",
        "upload" => "uploadFile",
        "press" => "press",
        "get_cookies" => "listCookies",
        "set_cookies" => "setCookie",
        "delete_cookie" => "removeCookie",
        "list_cookies" => "evaluate",
        // The caller names the wire action verbatim, so this one is owned
        // rather than borrowed from the static table above.
        "provider_command" => {
            return input
                .provider_action
                .clone()
                .map(Cow::Owned)
                .ok_or_else(|| {
                    anyhow::anyhow!("provider_action is required when action='provider_command'")
                });
        }
        other => anyhow::bail!(unsupported_action_message(other)),
    };
    Ok(Cow::Borrowed(mapped))
}

/// `wait` has to pick between two different extension actions.
///
/// `waitFor` waits for a *target* (selector/text/url/title/...) and, with no
/// target, falls through to a `document.readyState` check that is normally
/// already true. A quiet-period wait — what `position: 'dom-stable'` and the
/// `network-idle` approximation mean — is a separate action,
/// `waitForStable`. Sending `domStable`/`networkIdle` to `waitFor` (as this
/// used to) matched nothing in the extension and silently degraded to the
/// readyState check.
fn wait_bridge_action(input: &BrowserInput) -> &'static str {
    match input.position.as_deref() {
        Some("dom-stable") | Some("network-idle") => "waitForStable",
        _ => "waitFor",
    }
}

fn unsupported_action_message(action: &str) -> String {
    // A bare "valid actions:" dump forces the model to diff the whole list by
    // hand. Naming the near-misses gets the intended action accepted on the
    // next turn instead of another wrong guess.
    let mut message = format!("Unsupported browser action: '{action}'.");
    let aliases: &[(&str, &str)] = &[
        ("navigate", "open"),
        ("goto", "open"),
        ("visit", "open"),
        ("evaluate", "eval"),
        ("js", "eval"),
        ("javascript", "eval"),
        ("run_script", "eval"),
        ("back", "go_back"),
        ("forward", "go_forward"),
        ("close", "close_tab"),
        ("text", "get_content"),
        ("content", "get_content"),
        ("find", "interactables"),
        ("search", "interactables"),
        ("locate", "interactables"),
        ("query", "interactables"),
        ("elements", "interactables"),
        ("list_interactables", "interactables"),
        ("set_files", "upload"),
        ("set_input_files", "upload"),
        ("upload_file", "upload"),
        ("upload_files", "upload"),
        ("setFiles", "upload"),
        ("attach_file", "upload"),
        ("press_key", "press"),
        ("key", "press"),
        ("fill", "fill_form"),
        ("frame_list", "list_frames"),
        ("tab_list", "list_tabs"),
    ];
    let suggestions: Vec<String> = aliases
        .iter()
        .filter(|(alias, _)| alias.eq_ignore_ascii_case(action))
        .map(|(_, canonical)| format!("'{canonical}'"))
        .chain(
            super::Registry::closest_tool_names(action, KNOWN_ACTIONS)
                .into_iter()
                .map(|name| format!("'{name}'")),
        )
        .collect();
    let suggestions = dedupe_preserving_order(suggestions);
    if !suggestions.is_empty() {
        message.push_str(&format!(" Did you mean {}?", suggestions.join(", ")));
    }
    message.push_str(
        " Valid actions: status, setup, list_tabs, new_tab, select_tab, get_active_tab, \
         list_frames, open (alias: navigate), reload, go_back, go_forward, close_tab, snapshot, \
         get_content, interactables (alias: find), click, hover, type, fill_form, select, \
         drag_and_drop, wait, screenshot, eval, scroll, upload (aliases: set_files, set_input_files), \
         press, get_cookies, set_cookies, delete_cookie, list_cookies, provider_command.",
    );
    message
}

fn dedupe_preserving_order(items: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    items
        .into_iter()
        .filter(|item| seen.insert(item.clone()))
        .collect()
}

fn bridge_request(action: &str, input: &BrowserInput) -> Result<(String, Value, String)> {
    let bridge_action = bridge_action_for(action, input)?.into_owned();

    let mut params = Map::new();
    apply_common_targeting(&mut params, input);

    match action {
        "new_tab" => {
            if let Some(url) = &input.url {
                params.insert("url".into(), json!(url));
            }
            if let Some(timeout_ms) = input.timeout_ms {
                params.insert("timeoutMs".into(), json!(timeout_ms));
            }
        }
        "select_tab" => {
            let tab_id = input
                .tab_id
                .ok_or_else(|| anyhow::anyhow!("tab_id is required for select_tab"))?;
            params.insert("tabId".into(), json!(tab_id));
            // Bug fix #13: the bridge raises the tab in front of the user
            // unless focus is explicitly false. Selecting a tab is the agent
            // claiming a target for its own work — it must not steal the
            // foreground by default.
            params.insert("focus".into(), json!(input.focus.unwrap_or(false)));
        }
        "close_tab" => {
            // tab_id optional: bridge should default to the active tab if
            // omitted, consistent with how other actions default targeting.
        }
        "open" => {
            let url = input
                .url
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("url is required for open"))?;
            params.insert("url".into(), json!(url));
            params.insert("wait".into(), json!(input.wait.unwrap_or(true)));
            // Bug fix #13: default to a new background tab. Without this the
            // bridge resolved the *user's currently active tab* and navigated
            // it, hijacking whatever the user was doing. The agent's own tab
            // pin (apply_session_tab_pin) still routes follow-up actions to
            // the tab it owns; an explicit `new_tab: false` re-navigates the
            // pinned agent tab, not the user's tab. An explicit `tab_id`
            // still navigates that tab (the bridge checks newTab before
            // resolving tabId).
            let explicit_target = params.contains_key("tabId");
            params.insert(
                "newTab".into(),
                json!(match input.new_tab {
                    Some(value) => value,
                    None => !explicit_target,
                }),
            );
            if let Some(timeout_ms) = input.timeout_ms {
                params.insert("timeoutMs".into(), json!(timeout_ms));
            }
            // `navigate` otherwise ships a full annotated content dump back
            // with the navigation result, which landed in both the rendered
            // text and the tool metadata on every single `open`. Navigation
            // should not cost a whole page of context; `snapshot` /
            // `get_content` exist for that.
            params.insert("returnContent".into(), json!(false));
        }
        "reload" | "go_back" | "go_forward" => {
            if let Some(timeout_ms) = input.timeout_ms {
                params.insert("timeoutMs".into(), json!(timeout_ms));
            }
            params.insert("wait".into(), json!(input.wait.unwrap_or(true)));
        }
        "snapshot" => {
            params.insert("format".into(), json!("annotated"));
        }
        "get_content" => {
            let format = input.format.as_deref().unwrap_or("text");
            params.insert("format".into(), json!(format));
            // The content script clamps with `maxChars` (and `textMaxChars`
            // for the annotated text section). `maxLength` appears nowhere in
            // the extension, so the documented cap was silently a no-op for
            // every format. Only default a cap for html (as before) so text
            // formats are not truncated for callers who did not ask for it.
            if let Some(max_length) = input.max_length {
                params.insert("maxChars".into(), json!(max_length));
                if format == "annotated" {
                    params.insert("textMaxChars".into(), json!(max_length));
                }
            } else if format == "html" {
                params.insert("maxChars".into(), json!(60_000));
            }
        }
        "interactables" => {
            // The bridge filters on `text`, and `contains` is documented as the
            // alias for text-matching. Without this, `contains` was silently
            // dropped and `interactables` returned all 250 elements instead of
            // the handful matching the query — which is exactly what an agent
            // uses this action for, so it then had to pick a selector blind.
            if input.text.is_none()
                && let Some(contains) = &input.contains
            {
                params.insert("text".into(), json!(contains));
            }
        }
        "click" => {
            if input.selector.is_none()
                && input.text.is_none()
                && input.contains.is_none()
                && input.x.is_none()
                && input.y.is_none()
            {
                anyhow::bail!(
                    "click requires one of: selector, text, contains, or x/y coordinates"
                );
            }
            // `contains` acts as a text match in the bridge; `text` wins if
            // both are given (documented in the schema description).
            if input.text.is_none()
                && let Some(contains) = &input.contains
            {
                params.insert("text".into(), json!(contains));
            }
        }
        "hover" => {
            if input.selector.is_none() && input.x.is_none() && input.y.is_none() {
                anyhow::bail!("hover requires selector or x/y coordinates");
            }
        }
        "drag_and_drop" => {
            let source = input.selector.as_deref().ok_or_else(|| {
                anyhow::anyhow!("drag_and_drop requires 'selector' as the drag source")
            })?;
            let target = input.target_selector.as_deref().ok_or_else(|| {
                anyhow::anyhow!("drag_and_drop requires 'target_selector' as the drop target")
            })?;
            // The extension's `drag` handler reads `source`/`target` (each a
            // selector string or element descriptor), not sourceSelector/.
            params.insert("source".into(), json!(source));
            params.insert("target".into(), json!(target));
        }
        "type" => {
            let text = input
                .text
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("text is required for type"))?;
            params.insert("text".into(), json!(text));
            if let Some(clear) = input.clear {
                params.insert("clear".into(), json!(clear));
            }
            // `submit` is deliberately NOT forwarded here: the content
            // script's `typeInto` never reads it, so it was a silent no-op and
            // the agent believed the form had gone out. `execute_firefox_action`
            // issues a real Enter press once the text lands.
        }
        "fill_form" => {
            let fields = input
                .fields
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("fields are required for fill_form"))?;
            if fields.is_empty() {
                anyhow::bail!("fill_form requires at least one entry in 'fields'");
            }
            params.insert(
                "fields".into(),
                Value::Array(fields.iter().map(field_to_json).collect()),
            );
            // Unlike `type`, `fillForm` *does* honor a top-level `submit`
            // (it resolves a submit button and calls `form.requestSubmit()`).
            if let Some(submit) = input.submit {
                params.insert("submit".into(), json!(submit));
            }
        }
        "select" => {
            let selector = input
                .selector
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("selector is required for select"))?;
            // Bug fix #2: support multi-select via `values`. Single-value
            // path (`text` or `value`) is kept for backward compatibility.
            if let Some(values) = &input.values {
                if values.is_empty() {
                    anyhow::bail!("select: 'values' was provided but is empty");
                }
                params.insert(
                    "fields".into(),
                    json!([{ "selector": selector, "values": values }]),
                );
            } else {
                let value = input
                    .text
                    .as_deref()
                    .or(input.value.as_deref())
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "select requires the option value in 'text', 'value', or (for multi-select) 'values'"
                        )
                    })?;
                params.insert(
                    "fields".into(),
                    json!([{ "selector": selector, "value": value }]),
                );
            }
        }
        "wait" => {
            if input.selector.is_none()
                && input.text.is_none()
                && input.contains.is_none()
                && input.position.is_none()
                && input.timeout_ms.is_none()
            {
                anyhow::bail!(
                    "wait requires one of: selector, text, contains, timeout_ms (fixed delay), or position='dom-stable'/'network-idle'"
                );
            }
            // Reject an unknown position up front: the bridge action is chosen
            // from `position` (see `wait_bridge_action`), so a typo would
            // otherwise silently fall back to a plain `waitFor`.
            if let Some(position) = input.position.as_deref()
                && !matches!(position, "dom-stable" | "network-idle")
                && input.selector.is_none()
                && input.text.is_none()
                && input.contains.is_none()
            {
                anyhow::bail!(
                    "wait position '{position}' is invalid here; use 'dom-stable' or 'network-idle', or wait on a selector/text"
                );
            }
            // The extension reads `timeoutMs`/`intervalMs`, not `timeout`.
            // It is the same key for `waitFor` and `waitForStable`.
            if let Some(timeout_ms) = input.timeout_ms {
                params.insert("timeoutMs".into(), json!(timeout_ms));
            }
            // The extension matches `text`, not `contains`.
            if input.text.is_none()
                && let Some(contains) = &input.contains
            {
                params.insert("text".into(), json!(contains));
            }
            // Nothing left to insert for the quiet-period modes: the action
            // itself (`waitForStable`, not `waitFor`) is what carries them.
            // A bare `timeout_ms` is handled locally in
            // `execute_firefox_action` and never reaches the bridge.
        }
        "screenshot" => {}
        "eval" => {
            let script = input
                .script
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("script is required for eval"))?;
            if script.trim().is_empty() {
                anyhow::bail!("eval requires a non-empty script");
            }
            params.insert(
                "script".into(),
                json!(bridge_compatible_eval_script(script)),
            );
            if let Some(page_world) = input.page_world {
                params.insert("pageWorld".into(), json!(page_world));
            }
        }
        "scroll" => {
            if let Some(x) = input.x {
                params.insert("x".into(), json!(x));
            }
            if let Some(y) = input.y {
                params.insert("y".into(), json!(y));
            }
            if let Some(position) = &input.position {
                params.insert("position".into(), json!(position));
            }
            if let Some(behavior) = &input.behavior {
                params.insert("behavior".into(), json!(behavior));
            }
            if let Some(scroll_to) = &input.scroll_to {
                let mut target = Map::new();
                if let Some(x) = scroll_to.x {
                    target.insert("x".into(), json!(x));
                }
                if let Some(y) = scroll_to.y {
                    target.insert("y".into(), json!(y));
                }
                params.insert("scrollTo".into(), Value::Object(target));
            }
            if !params.contains_key("x")
                && !params.contains_key("y")
                && !params.contains_key("selector")
                && !params.contains_key("position")
                && !params.contains_key("scrollTo")
            {
                anyhow::bail!("scroll requires x/y, selector, position, or scroll_to");
            }
        }
        "upload" => {
            // Validation only. The real payload is built in
            // `execute_firefox_action` (`upload_params`), because
            // `uploadFile` needs the file's bytes inline as base64 and this
            // function is pure.
            input
                .path
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("path is required for upload"))?;
        }
        "press" => {
            // The extension has a real `press` action that mutates the value
            // through the *prototype* value setter (so React-controlled inputs
            // actually see it) and implements Tab/Escape/Enter/Space
            // semantics. It previously went to `evaluate` with a hand-rolled
            // script that assigned `el.value` directly — invisible to React,
            // and unable to move focus or close a dialog.
            let key = input
                .key
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("key is required for press"))?;
            if key.is_empty() {
                anyhow::bail!("press requires a non-empty key");
            }
            params.insert("key".into(), json!(key));
            // `submit: true` makes the extension call `form.requestSubmit()`
            // for Enter (and to activate a focused submit button).
            if let Some(submit) = input.submit {
                params.insert("submit".into(), json!(submit));
            }
        }
        "get_cookies" => {
            // cookieAction/listCookies reads {url, domain, name, storeId}.
            if let Some(url) = &input.url {
                params.insert("url".into(), json!(url));
            }
            if let Some(domain) = &input.cookie_domain {
                params.insert("domain".into(), json!(domain));
            }
            if let Some(name) = &input.cookie_name {
                params.insert("name".into(), json!(name));
            }
        }
        "set_cookies" => {
            // Validation only here: the wire has no batch set, so
            // `execute_firefox_action` fans out to one `setCookie` call per
            // cookie via `set_cookie_params`.
            let cookies = input.cookies.as_ref().ok_or_else(|| {
                anyhow::anyhow!("set_cookies requires 'cookies' (array of {{name, value, ...}})")
            })?;
            if cookies.is_empty() {
                anyhow::bail!("set_cookies: 'cookies' was provided but is empty");
            }
        }
        "delete_cookie" => {
            let name = input
                .cookie_name
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("delete_cookie requires 'cookie_name'"))?;
            params.insert("name".into(), json!(name));
            // cookieAction/removeCookie forwards to browser.cookies.remove,
            // which requires a URL (matched against the cookie's domain+path).
            let url = cookie_action_url(input.url.as_deref(), input.cookie_domain.as_deref(), input.cookie_path.as_deref()).ok_or_else(|| {
                anyhow::anyhow!(
                    "delete_cookie requires 'url' or 'cookie_domain' so the bridge can address the cookie"
                )
            })?;
            params.insert("url".into(), json!(url));
        }
        "list_cookies" => {
            // Legacy path, kept for backward compatibility: emulated via
            // eval of document.cookie. Cannot see HttpOnly cookies — that's
            // why get_cookies (real bridge call) is now the recommended
            // action; see schema description. Parsed into a structured
            // {cookies: [...], raw} shape rather than the bare string so
            // format_cookie_string_result can render it as one line per
            // cookie like the real get_cookies path does.
            let script = "return (() => { const raw = document.cookie || ''; const cookies = raw.split(';').map(s => s.trim()).filter(Boolean).map(pair => { const idx = pair.indexOf('='); return idx === -1 ? { name: pair, value: '' } : { name: pair.slice(0, idx), value: pair.slice(idx + 1) }; }); return { cookies, raw }; })()";
            params.insert(
                "script".into(),
                json!(bridge_compatible_eval_script(script)),
            );
        }
        "provider_command" => {
            if let Some(raw) = &input.params {
                return Ok((bridge_action, raw.clone(), format!("browser {}", action)));
            }
        }
        _ => {}
    }

    Ok((
        bridge_action,
        Value::Object(params),
        format!("browser {}", action),
    ))
}

/// Build the `uploadFile` payload for one local file.
///
/// The extension cannot read the agent's filesystem, so `uploadFile` expects
/// the bytes inline: `files: [{name, type, data}]` where `data` is base64 (an
/// optional `data:<mime>;base64,` prefix is tolerated) and is turned back into
/// a `File` by `base64ToFile`. The `filePath`/`fileName` keys this used to
/// send appear nowhere in the extension, so every upload failed with
/// "uploadFile requires file or files".
async fn upload_params(input: &BrowserInput) -> Result<Map<String, Value>> {
    let path = input
        .path
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("path is required for upload"))?;
    let file = Path::new(path);
    if !file.exists() {
        anyhow::bail!("upload: no such file: {path}");
    }
    if file.is_dir() {
        anyhow::bail!("upload: '{path}' is a directory, not a file");
    }
    let bytes = tokio::fs::read(file)
        .await
        .with_context(|| format!("upload: failed to read '{path}'"))?;
    if bytes.is_empty() {
        anyhow::bail!("upload: '{path}' is empty");
    }
    let name = file
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("upload")
        .to_string();
    let mut params = Map::new();
    params.insert(
        "files".into(),
        json!([{
            "name": name,
            "type": media_type_for(path),
            "data": STANDARD.encode(&bytes),
        }]),
    );
    // The extension picks the first visible `input[type=file]` when no
    // selector is given; forward the caller's `selector` (already added by
    // `apply_common_targeting`) so a specific input can be targeted.
    Ok(params)
}

/// Best-effort MIME type from a file extension.
///
/// The extension defaults to `application/octet-stream`, which some upload
/// endpoints reject outright, so the common cases are worth mapping. Anything
/// unrecognized stays `application/octet-stream` rather than guessing wrong.
fn media_type_for(path: &str) -> &'static str {
    let extension = Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "txt" | "log" | "md" => "text/plain",
        "csv" => "text/csv",
        "html" | "htm" => "text/html",
        "css" => "text/css",
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "xml" => "application/xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    }
}

fn field_to_json(field: &BrowserField) -> Value {
    let mut obj = Map::new();
    obj.insert("selector".into(), json!(field.selector));
    if let Some(value) = &field.value {
        obj.insert("value".into(), json!(value));
    }
    if let Some(values) = &field.values {
        obj.insert("values".into(), json!(values));
    }
    if let Some(checked) = field.checked {
        obj.insert("checked".into(), json!(checked));
    }
    Value::Object(obj)
}

/// Build the single-cookie params for one `setCookie` bridge call.
///
/// The extension has no batch endpoint: it reads a flat
/// {url, name, value, domain, path, secure, httpOnly, expirationDate,
/// sameSite, storeId} object per call. `browser.cookies.set` requires a
/// URL, so when the caller only gave a domain we synthesize an `https://`
/// URL from domain+path (a leading `.` is stripped first).
fn set_cookie_params(
    cookie: &CookieInput,
    default_url: Option<&str>,
) -> Result<Map<String, Value>> {
    let mut obj = Map::new();
    obj.insert("name".into(), json!(cookie.name));
    obj.insert("value".into(), json!(cookie.value));
    let url = match default_url {
        Some(explicit) => Some(explicit.to_string()),
        None => match cookie.domain.as_deref() {
            Some(domain) => cookie_action_url(None, Some(domain), cookie.path.as_deref()),
            None => None,
        },
    }
    .ok_or_else(|| {
        anyhow::anyhow!(
            "set_cookies: cookie '{}' needs 'url' or 'domain' so the bridge can address it",
            cookie.name
        )
    })?;
    obj.insert("url".into(), json!(url));
    if let Some(domain) = &cookie.domain {
        obj.insert("domain".into(), json!(domain));
    }
    if let Some(path) = &cookie.path {
        obj.insert("path".into(), json!(path));
    }
    if let Some(secure) = cookie.secure {
        obj.insert("secure".into(), json!(secure));
    }
    if let Some(http_only) = cookie.http_only {
        obj.insert("httpOnly".into(), json!(http_only));
    }
    if let Some(expires) = cookie.expires {
        obj.insert("expirationDate".into(), json!(expires));
    }
    Ok(obj)
}

/// Resolve the URL cookie actions need: explicit `url` wins, otherwise an
/// `https://` URL synthesized from domain+path (cookies.remove/set match on
/// it). Returns `None` when neither is available.
fn cookie_action_url(
    url: Option<&str>,
    domain: Option<&str>,
    path: Option<&str>,
) -> Option<String> {
    if let Some(url) = url {
        return Some(url.to_string());
    }
    let domain = domain?.trim().trim_start_matches('.');
    if domain.is_empty() {
        return None;
    }
    let mut out = String::with_capacity(domain.len() + 9);
    out.push_str("https://");
    out.push_str(domain);
    match path {
        Some(p) if p.starts_with('/') => out.push_str(p),
        Some(p) if !p.is_empty() => {
            out.push('/');
            out.push_str(p);
        }
        _ => out.push('/'),
    }
    Some(out)
}

fn apply_common_targeting(params: &mut Map<String, Value>, input: &BrowserInput) {
    if let Some(tab_id) = input.tab_id {
        params.insert("tabId".into(), json!(tab_id));
    }
    if let Some(window_id) = input.window_id {
        params.insert("windowId".into(), json!(window_id));
    }
    if let Some(frame_id) = input.frame_id {
        params.insert("frameId".into(), json!(frame_id));
    }
    if let Some(all_frames) = input.all_frames {
        params.insert("allFrames".into(), json!(all_frames));
    }
    if let Some(selector) = &input.selector {
        params.insert("selector".into(), json!(selector));
    }
    if let Some(text) = &input.text {
        params.insert("text".into(), json!(text));
    }
    // click/hover accept x/y coordinates (e.g. from a screenshot), and the
    // bridge resolves them via elementFromPoint. They were previously
    // accepted by validation but silently dropped here, so every
    // coordinate click failed with "Element not found".
    if let Some(x) = input.x {
        params.insert("x".into(), json!(x));
    }
    if let Some(y) = input.y {
        params.insert("y".into(), json!(y));
    }
    // The extension's `resolveElement` reads `index` to pick the nth match and
    // `includeHidden` to stop filtering candidates through `visible()`. Both
    // were accepted by the schema's spirit but had no parameter to carry them,
    // so an element that `interactables` had just listed could not be acted on.
    if let Some(index) = input.index {
        params.insert("index".into(), json!(index));
    }
    if let Some(include_hidden) = input.include_hidden {
        params.insert("includeHidden".into(), json!(include_hidden));
    }
}

/// Detect the bridge's target-resolution failures without treating ordinary
/// page errors as a missing browser tab.
///
/// A false positive here is expensive: it both replaces the real error with a
/// tab hint and (in `run_bridge_with_tab_recovery`) re-resolves the active tab
/// and replays the request. `ReferenceError: Tab is not defined` must not
/// match, and neither must page text that merely starts with a needle —
/// "no table of contents" contains "no tab", and "Error: Table not found"
/// contains "error: tab".
fn missing_tab_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    [
        "no active tab",
        "active tab not found",
        "tab not found",
        "invalid tab",
        "tab closed",
        "tab is closed",
        "no tab",
        "error: tab",
    ]
    .iter()
    .any(|needle| contains_phrase(&lower, needle))
}

/// Substring match on a whole-token basis: both the leading *and* trailing
/// edges must be non-alphanumeric (or the string boundary).
///
/// The leading guard stops `ReferenceError: Tab is not defined` from matching
/// `error: tab`. The trailing guard stops `no table` from matching `no tab`
/// and `Error: Table not found` from matching `error: tab`, while still
/// accepting the bridge's real messages, which continue with punctuation or a
/// space (`Error: Tab 199 no longer exists`, `tab closed: 42`).
fn contains_phrase(haystack: &str, needle: &str) -> bool {
    haystack.match_indices(needle).any(|(start, matched)| {
        let leading_ok = !haystack[..start]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric());
        let trailing_ok = !haystack[start + matched.len()..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphanumeric());
        leading_ok && trailing_ok
    })
}

/// Extract a Firefox tab id from the shapes returned by bridge versions.
fn tab_id_from_value(value: &Value) -> Option<i64> {
    let object = value.as_object()?;
    let direct = ["tabId", "tab_id", "id"]
        .into_iter()
        .find_map(|key| object.get(key))
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        });
    direct.or_else(|| {
        object
            .get("tab")
            .and_then(Value::as_object)
            .and_then(|tab| {
                ["tabId", "tab_id", "id"]
                    .into_iter()
                    .find_map(|key| tab.get(key))
            })
            .and_then(|value| {
                value
                    .as_i64()
                    .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
            })
    })
}

/// Ask the bridge for the current tab and add it to a request. A restarted
/// bridge/session can invalidate the caller's old tab id; resolving the active
/// tab once keeps evaluate and other targeted actions usable without retrying
/// arbitrary page mutations.
async fn recover_active_tab(params: &mut Value, ctx: &ToolContext) -> bool {
    let Ok(active) = firefox_run_bridge_command("getActiveTab", json!({}), ctx).await else {
        return false;
    };
    let Some(tab_id) = tab_id_from_value(&active) else {
        return false;
    };
    let Some(object) = params.as_object_mut() else {
        return false;
    };
    object.insert("tabId".to_string(), json!(tab_id));
    true
}

async fn run_bridge_with_tab_recovery(
    bridge_action: &str,
    params: Value,
    ctx: &ToolContext,
) -> Result<Value> {
    let first = firefox_run_bridge_command(bridge_action, params.clone(), ctx).await;
    let err = match first {
        Ok(value) => return Ok(value),
        Err(err) => err,
    };
    if !missing_tab_error(&err.to_string()) {
        return Err(err);
    }

    // Bug fix #13: never repoint a targeted or session-pinned request at
    // the user's active tab. When the request names a tabId and that tab is
    // gone, fail with the enrichment hint (list_tabs / select_tab / open)
    // so the agent re-establishes its own tab. Untargeted reads keep the
    // legacy active-tab recovery so a bridge restart doesn't strand them.
    if params.get("tabId").and_then(Value::as_i64).is_some() {
        return Err(err);
    }

    let mut recovered_params = params;
    if !recover_active_tab(&mut recovered_params, ctx).await {
        return Err(err);
    }
    firefox_run_bridge_command(bridge_action, recovered_params, ctx).await
}

/// Bug fix #4: bounded retry with backoff for idempotent read-only bridge
/// calls. Mutating actions are never retried — see `RETRYABLE_ACTIONS`.
async fn firefox_run_bridge_command_with_retry(
    logical_action: &str,
    bridge_action: &str,
    params: Value,
    ctx: &ToolContext,
) -> Result<Value> {
    if !RETRYABLE_ACTIONS.contains(&logical_action) {
        return run_bridge_with_tab_recovery(bridge_action, params, ctx).await;
    }

    let mut last_err = None;
    for attempt in 0..=MAX_RETRIES {
        match run_bridge_with_tab_recovery(bridge_action, params.clone(), ctx).await {
            Ok(value) => return Ok(value),
            Err(err) => {
                // Don't burn retries on errors that retrying can't fix
                // (bad input, unsupported action) — only on the kind of
                // failure that suggests a transient process/IPC hiccup.
                let msg = err.to_string();
                if msg.contains("Unknown action:") || msg.contains("is required") {
                    return Err(err);
                }
                last_err = Some(err);
                if attempt < MAX_RETRIES {
                    tokio::time::sleep(RETRY_BACKOFF * (attempt + 1)).await;
                }
            }
        }
    }
    Err(last_err
        .unwrap_or_else(|| anyhow::anyhow!("browser bridge command failed with no error detail")))
}

/// Hard deadline for one bridge CLI invocation.
///
/// `output()` waits forever, so a wedged native-messaging host (Firefox
/// closed mid-request, a stuck pipe) parked the tool call indefinitely with no
/// error and no way for the agent to move on. Generous, because a cold page
/// load plus `waitForStable` legitimately takes a while, but bounded.
const BRIDGE_COMMAND_TIMEOUT: Duration = Duration::from_secs(120);

async fn firefox_run_bridge_command(
    action: &str,
    params: Value,
    _ctx: &ToolContext,
) -> Result<Value> {
    let bin = crate::browser::browser_binary_path();
    if !bin.exists() {
        crate::browser::ensure_browser_setup().await?;
    }
    let bin = crate::browser::browser_binary_path();
    if !bin.exists() {
        anyhow::bail!(
            "Browser bridge installation failed. Ensure network connectivity and try again."
        );
    }

    let params_json = serde_json::to_string(&params)?;
    let mut command = tokio::process::Command::new(&bin);
    command.arg(action).arg(&params_json);
    command.stdin(std::process::Stdio::null());
    command.stdout(std::process::Stdio::piped());
    command.stderr(std::process::Stdio::piped());
    // Belt-and-braces with the explicit `kill()` below, so the child dies even
    // if this future is dropped mid-await.
    command.kill_on_drop(true);

    #[cfg(not(windows))]
    if std::env::var("BROWSER_SESSION").is_err()
        && let Some(session_name) = crate::browser::ensure_browser_session(&_ctx.session_id)
    {
        command.env("BROWSER_SESSION", session_name);
    }

    let child = command.spawn().map_err(|error| {
        // This used to report only "Failed to run browser bridge action 'X'."
        // — which names the action the caller asked for, not the thing that
        // actually broke, so it reads like the action is unsupported when the
        // real fault is a spawn that never happened. Name the cause and the
        // most common local reasons for it.
        let mut message = format!(
            "Browser bridge action '{action}' could not start: {error}."
        );
        let detail = match error.kind() {
            std::io::ErrorKind::PermissionDenied => {
                " The bridge binary exists but is not executable, or is blocked by antivirus / Windows Defender."
            }
            std::io::ErrorKind::NotFound => {
                " The bridge binary disappeared between the existence check and the spawn — an install or antivirus quarantine raced this call."
            }
            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::ResourceBusy => {
                " Another process holds the bridge binary open."
            }
            _ => "",
        };
        message.push_str(detail);
        message.push_str(
            " Run action='status' to check the bridge, then action='setup' to reinstall it.",
        );
        anyhow::anyhow!(message)
    })?;
    let label = format!("Browser bridge action '{action}'");
    let output = super::recon_common::wait_bounded(child, &label, BRIDGE_COMMAND_TIMEOUT)
        .await
        .map_err(|error| {
            if error.contains("timed out") {
                anyhow::anyhow!(
                    "Browser bridge action '{action}' did not respond within {}s. Firefox may be showing a modal dialog, or the native bridge host is stuck. Run action='status' to check the bridge, then action='setup' to repair it.",
                    BRIDGE_COMMAND_TIMEOUT.as_secs()
                )
            } else {
                anyhow::anyhow!(error)
            }
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        // Previously the empty case collapsed to `stdout` (i.e. ""), so a
        // bridge that died without writing anything reported
        // "failed: " with no cause at all. Name the exit status instead.
        let details = match (stdout.is_empty(), stderr.is_empty()) {
            (true, true) => format!("exit {} with no diagnostic output", output.status),
            (false, true) => stdout,
            (true, false) => stderr,
            (false, false) => format!("{stderr}\n{stdout}"),
        };
        if details.contains("Unknown action:") {
            anyhow::bail!(
                "The connected Firefox browser bridge is missing required support for action '{}'. This usually means the installed extension is older than the browser CLI expected by alphacode. Use browser action='status' to confirm, then action='setup' to repair or update the extension.\n\nOriginal bridge error: {}",
                action,
                details
            );
        }
        anyhow::bail!("Browser bridge action '{}' failed: {}", action, details);
    }

    if stdout.is_empty() {
        return Ok(json!({ "ok": true }));
    }

    serde_json::from_str(&stdout).or_else(|_| Ok(json!({ "raw": stdout })))
}

/// Screenshot support.
///
/// The extension's `screenshot` action returns `{tabId, dataUrl, method}`
/// where `dataUrl` is a `data:image/png;base64,...` string. There is no
/// `filename` parameter and no `saved` result, so the previous
/// implementation — ask the bridge to write a temp file, then look for
/// `result.saved` — never found anything, failed to read its own temp path,
/// attached no image, and still reported "Captured browser screenshot to
/// <path>" for a file that did not exist. Decoding `dataUrl` also removes the
/// temp file entirely, so there is nothing left to collide or leak.
async fn screenshot_via_bridge(
    params: &Value,
    title: String,
    ctx: &ToolContext,
) -> Result<ToolOutput> {
    let result = firefox_run_bridge_command("screenshot", params.clone(), ctx)
        .await
        .map_err(|error| enrich_browser_error("screenshot", error))?;

    let mut output = ToolOutput::new(String::new())
        .with_title(title)
        .with_metadata(result.clone());

    if let Some((media_type, base64)) = screenshot_data_url(&result) {
        let mut metadata = merge_into_metadata_object(output.metadata.take());
        metadata.insert(
            "image".into(),
            json!({"mediaType": media_type, "bytes": base64.len()}),
        );
        output.metadata = Some(Value::Object(metadata));
        output.output = format!(
            "Captured a browser screenshot ({} bytes, {}).",
            base64.len(),
            media_type
        );
        return Ok(output.with_labeled_image(media_type, base64, "browser screenshot".to_string()));
    }

    // Compatibility path for a CLI build that persists the capture itself and
    // reports where. The 1.6.0 extension does not do this, but reading it
    // costs nothing and keeps such a build working.
    if let Some(saved) = result.get("saved").and_then(Value::as_str)
        && let Ok(bytes) = tokio::fs::read(saved).await
    {
        let _ = tokio::fs::remove_file(saved).await;
        let encoded = STANDARD.encode(&bytes);
        let mut metadata = merge_into_metadata_object(output.metadata.take());
        metadata.insert(
            "image".into(),
            json!({"mediaType": "image/png", "bytes": bytes.len(), "savedTo": saved}),
        );
        output.metadata = Some(Value::Object(metadata));
        output.output = format!("Captured a browser screenshot ({} bytes).", bytes.len());
        return Ok(output.with_labeled_image(
            "image/png",
            encoded,
            "browser screenshot".to_string(),
        ));
    }

    anyhow::bail!(
        "screenshot: the browser bridge returned no image data. This usually means the extension stripped the capture (the popup 'screenshot' button reports only dataUrlLength) or the tab was not capturable. Run action='status' to confirm the bridge version, then action='setup' to repair it. For page state use action='snapshot' or action='interactables', which return text and selectors instead of pixels. Bridge response: {}",
        summarize_bridge_result(&result)
    )
}

/// Extract `(media_type, base64_payload)` from a `data:...;base64,` URL.
fn screenshot_data_url(result: &Value) -> Option<(String, String)> {
    let data_url = result
        .get("dataUrl")
        .or_else(|| result.get("data_url"))
        .and_then(Value::as_str)?;
    let (meta, payload) = data_url.split_once(',')?;
    if !meta.ends_with(";base64") {
        return None;
    }
    let media_type = meta
        .trim_start_matches("data:")
        .trim_end_matches(";base64")
        .to_ascii_lowercase();
    if media_type.is_empty() || payload.is_empty() {
        return None;
    }
    Some((
        if media_type == "image/jpg" {
            "image/jpeg".to_string()
        } else {
            media_type
        },
        payload.to_string(),
    ))
}

/// Short, log-safe rendering of a bridge response for error messages: a
/// screenshot data URL is megabytes of base64 and must never be inlined.
fn summarize_bridge_result(result: &Value) -> String {
    const MAX_SUMMARY: usize = 400;
    let mut summary = String::new();
    if let Some(object) = result.as_object() {
        for (key, value) in object {
            if !summary.is_empty() {
                summary.push_str(", ");
            }
            match value {
                Value::String(text) if text.len() > 80 => {
                    summary.push_str(&format!("{key}=<string {} chars>", text.len()));
                }
                other => {
                    let rendered = other.to_string();
                    if rendered.len() > 80 {
                        summary.push_str(&format!("{key}=<{}>", truncate_chars(&rendered, 80)));
                    } else {
                        summary.push_str(&format!("{key}={rendered}"));
                    }
                }
            }
        }
    } else {
        summary = truncate_chars(&result.to_string(), MAX_SUMMARY);
    }
    truncate_chars(&summary, MAX_SUMMARY)
}

fn truncate_chars(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

fn render_browser_output(action: &str, title: String, result: Value) -> ToolOutput {
    let body = match action {
        "snapshot" => result
            .get("content")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| serde_json::to_string_pretty(&result).unwrap_or_default()),
        "get_content" => format_content_result(&result),
        "interactables" => format_interactables_result(&result),
        // Only `eval` returns `{result, type}`. `press` returns the
        // extension's `{pressed, keys, results, element}`, which the default
        // JSON arm renders faithfully.
        "eval" => format_eval_result(&result),
        "open" => format_navigate_result(&result),
        "list_cookies" => format_cookie_string_result(&result),
        "get_cookies" => {
            // Extract domain filter from result metadata if provided
            let domain_filter = result.get("domain").and_then(|v| v.as_str());
            format_cookies_result(&result, domain_filter)
        }
        "set_cookies" => format_set_cookies_result(&result),
        _ => serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()),
    };

    ToolOutput::new(body)
        .with_title(title)
        .with_metadata(result)
}

/// Render a navigation result compactly.
///
/// `navigate` returns `{tab: {...}}`; a pretty-printed copy of that (plus the
/// content dump it used to include) buried the useful bits — url, title,
/// status — under a page of JSON.
fn format_navigate_result(result: &Value) -> String {
    let tab = result.get("tab").unwrap_or(result);
    let mut lines = Vec::new();
    if let Some(tab_id) = tab_id_from_value(tab) {
        lines.push(format!("tab_id: {tab_id}"));
    }
    if let Some(url) = tab.get("url").and_then(Value::as_str) {
        lines.push(format!("url: {url}"));
    }
    if let Some(page_title) = tab.get("title").and_then(Value::as_str)
        && !page_title.is_empty()
    {
        lines.push(format!("title: {page_title}"));
    }
    if let Some(status) = tab.get("status").and_then(Value::as_str) {
        lines.push(format!("status: {status}"));
    }
    if lines.is_empty() {
        return serde_json::to_string_pretty(result).unwrap_or_default();
    }
    lines.push(String::new());
    lines.push(
        "Note: use action='snapshot' (or 'get_content') to read the page; navigation no longer returns the page body to keep this result small."
            .to_string(),
    );
    lines.join("\n")
}

fn format_content_result(result: &Value) -> String {
    if let Some(content) = result.get("content").and_then(|v| v.as_str()) {
        return content.to_string();
    }
    if let Some(text) = result.get("text").and_then(|v| v.as_str()) {
        return text.to_string();
    }
    if let Some(html) = result.get("html").and_then(|v| v.as_str()) {
        return html.to_string();
    }
    if let Some(title) = result.get("title").and_then(|v| v.as_str()) {
        if let Some(url) = result.get("url").and_then(|v| v.as_str()) {
            return format!("{}\n{}", title, url);
        }
        return title.to_string();
    }
    serde_json::to_string_pretty(result).unwrap_or_default()
}

fn format_eval_result(result: &Value) -> String {
    let value = result.get("result").cloned().unwrap_or(Value::Null);
    let rendered = if let Some(s) = value.as_str() {
        s.to_string()
    } else {
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
    };

    let type_note = match result.get("type").and_then(|v| v.as_str()) {
        Some(kind) => format!("\n\n(type: {})", kind),
        None => String::new(),
    };

    // Warn loudly on undefined so the agent knows to `return` a value or
    // wrap the expression instead of mistaking null for a real result.
    if result.get("type").and_then(|v| v.as_str()) == Some("undefined") {
        return format!(
            "{}{}\n\nNote: script evaluated to undefined. If you expected a value, end the script with `return <expr>` (statements) or pass a bare expression, which is auto-returned.",
            rendered, type_note
        );
    }

    format!("{}{}", rendered, type_note)
}

/// Renders the legacy `list_cookies` (eval of document.cookie) result,
/// which comes back through the same shape as `eval`.
/// Renders the legacy `list_cookies` result. The eval script now returns a
/// structured `{result: {cookies: [...], raw}}` shape (parsed name/value
/// pairs) rather than the bare `document.cookie` string, so this can share
/// rendering with the real `get_cookies` path instead of dumping raw JSON.
fn format_cookie_string_result(result: &Value) -> String {
    let inner = result.get("result").cloned().unwrap_or(Value::Null);
    let cookies = inner
        .get("cookies")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    if cookies.is_empty() {
        return "No cookies visible to document.cookie (note: HttpOnly cookies are never visible this way — use action='get_cookies' instead).".to_string();
    }
    let mut lines = Vec::new();
    for cookie in &cookies {
        let name = cookie.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let value = cookie.get("value").and_then(|v| v.as_str()).unwrap_or("");
        lines.push(format!("{}={}", name, value));
    }
    lines.push(String::new());
    lines.push(
        "(via document.cookie — HttpOnly cookies are not visible this way; use action='get_cookies' to see all cookies)"
            .to_string(),
    );
    lines.join("\n")
}

/// Renders a real `get_cookies` bridge result (array of cookie objects).
///
/// When `domain_filter` is provided, only cookies whose domain contains the
/// filter string are shown. This prevents the 60k+ token dump that happens
/// when `get_cookies` returns every cookie from every domain.
fn format_cookies_result(result: &Value, domain_filter: Option<&str>) -> String {
    let Some(cookies) = result.get("cookies").and_then(|v| v.as_array()) else {
        return serde_json::to_string_pretty(result).unwrap_or_default();
    };
    if cookies.is_empty() {
        return "No cookies found.".to_string();
    }

    // Filter by domain if requested
    let filtered: Vec<&Value> = if let Some(filter) = domain_filter {
        cookies
            .iter()
            .filter(|c| {
                c.get("domain")
                    .and_then(|v| v.as_str())
                    .is_some_and(|d| d.contains(filter))
            })
            .collect()
    } else {
        cookies.iter().collect()
    };

    if filtered.is_empty() {
        return format!(
            "No cookies found for domain filter '{}'. Use get_cookies without a filter to see all cookies.",
            domain_filter.unwrap_or("")
        );
    }

    let mut lines = Vec::new();
    if let Some(filter) = domain_filter {
        lines.push(format!(
            "Cookies for domain '{}' ({} of {} total):",
            filter,
            filtered.len(),
            cookies.len()
        ));
    } else {
        lines.push(format!("Cookies ({} total):", filtered.len()));
    }
    for cookie in filtered {
        let name = cookie.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        let value = cookie.get("value").and_then(|v| v.as_str()).unwrap_or("");
        let domain = cookie.get("domain").and_then(|v| v.as_str()).unwrap_or("-");
        let http_only = cookie
            .get("httpOnly")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let secure = cookie
            .get("secure")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        lines.push(format!(
            "{}={} | domain: {} | httpOnly: {} | secure: {}",
            name, value, domain, http_only, secure
        ));
    }
    lines.join("\n")
}

/// Renders the fanned-out `set_cookies` result (`{set: [{name, result}]}`).
fn format_set_cookies_result(result: &Value) -> String {
    let Some(applied) = result.get("set").and_then(|v| v.as_array()) else {
        return serde_json::to_string_pretty(result).unwrap_or_default();
    };
    if applied.is_empty() {
        return "No cookies were set.".to_string();
    }
    let mut lines = Vec::with_capacity(applied.len() + 1);
    lines.push(format!("Set {} cookie(s):", applied.len()));
    for entry in applied {
        let name = entry.get("name").and_then(|v| v.as_str()).unwrap_or("?");
        lines.push(format!("- {}", name));
    }
    lines.join("\n")
}

fn format_interactables_result(result: &Value) -> String {
    let Some(elements) = result.get("elements").and_then(|v| v.as_array()) else {
        return serde_json::to_string_pretty(result).unwrap_or_default();
    };

    if elements.is_empty() {
        return "No interactable elements found.".to_string();
    }

    let mut lines = Vec::new();
    for (idx, element) in elements.iter().enumerate() {
        let kind = element
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("element");
        let tag = element.get("tag").and_then(|v| v.as_str()).unwrap_or("?");
        let text = element
            .get("text")
            .or_else(|| element.get("label"))
            .or_else(|| element.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let selector = element
            .get("selector")
            .and_then(|v| v.as_str())
            .unwrap_or("-");
        // Show the bridge's own 0-based index, not a 1-based ordinal. The
        // previous rendering printed "1., 2., 3." while the only way to address
        // a specific element was `index`, which the tool could not even accept —
        // so the agent read a number that meant nothing and guessed a selector.
        let index = element
            .get("index")
            .and_then(Value::as_i64)
            .unwrap_or(idx as i64);
        lines.push(format!(
            "index={index} [{}] <{}> {} | selector: {}",
            kind,
            tag.to_lowercase(),
            text,
            selector
        ));
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser_input(value: Value) -> BrowserInput {
        serde_json::from_value(value).expect("valid BrowserInput")
    }

    #[test]
    fn bare_eval_expression_is_auto_returned_for_legacy_bridges() {
        let script = bridge_compatible_eval_script("document.title");
        assert!(script.starts_with("(async () => {"));
        assert!(script.contains("return (\ndocument.title\n);"));
    }

    #[test]
    fn eval_return_statement_is_preserved_inside_async_iife() {
        let script = bridge_compatible_eval_script("return 1 + 1");
        assert!(script.contains("return 1 + 1"));
        assert!(!script.contains("return (return 1 + 1)"));
    }

    #[test]
    fn eval_top_level_await_is_preserved() {
        let script = bridge_compatible_eval_script(
            "const response = await fetch('/health'); return await response.text();",
        );
        assert!(script.contains("const response = await fetch('/health')"));
        assert!(script.contains("return await response.text()"));
    }

    #[test]
    fn eval_javascript_url_prefix_is_accepted() {
        let script = bridge_compatible_eval_script("JavaScript: return document.readyState");
        assert!(script.contains("return document.readyState"));
        assert!(!script.contains("javascript:"));
    }

    #[test]
    fn bridge_request_wraps_eval_script_and_preserves_targeting() {
        let input = browser_input(json!({
            "action": "eval",
            "script": "return document.title",
            "page_world": true,
            "tab_id": 42,
            "frame_id": 3
        }));
        let (action, params, _) = bridge_request("eval", &input).expect("bridge request");
        assert_eq!(action, "evaluate");
        assert_eq!(params["tabId"], 42);
        assert_eq!(params["frameId"], 3);
        assert_eq!(params["pageWorld"], true);
        assert!(
            params["script"]
                .as_str()
                .expect("serialized script")
                .starts_with("(async () => {")
        );
    }

    #[test]
    fn bridge_request_rejects_empty_eval_script() {
        let input = browser_input(json!({"action": "eval", "script": "  \n"}));
        let error = bridge_request("eval", &input).expect_err("empty script");
        assert!(error.to_string().contains("non-empty script"));
    }

    #[test]
    fn missing_tab_error_is_distinct_from_a_page_reference_error() {
        assert!(missing_tab_error("Error: Tab"));
        assert!(missing_tab_error("Error: Tab 199 no longer exists"));
        assert!(missing_tab_error("No active tab is available"));
        assert!(missing_tab_error("tab not found"));
        assert!(missing_tab_error("tab closed: 42"));
        assert!(!missing_tab_error("ReferenceError: Tab is not defined"));
    }

    /// A false positive here is expensive: it swallows the real error behind a
    /// tab hint *and* re-resolves the active tab and replays the request.
    /// `"no tab"` sits inside "no table of contents" and `"error: tab"` inside
    /// "Error: Table not found".
    #[test]
    fn missing_tab_error_ignores_page_text_that_merely_starts_the_same() {
        for message in [
            "Evaluate error: ReferenceError: noTable is not defined",
            "Error: Table not found on the page",
            "Evaluate error: TypeError: cannot read properties of undefined (reading 'tab')",
            "TypeError: tabs is not iterable",
        ] {
            assert!(
                !missing_tab_error(message),
                "{message} must not be read as a missing tab"
            );
        }
    }

    #[test]
    fn missing_tab_error_gets_actionable_recovery_hint() {
        let error = enrich_browser_error(
            "eval",
            anyhow::anyhow!("Browser bridge action 'evaluate' failed: Error: Tab"),
        );
        let message = error.to_string();
        assert!(message.contains("list_tabs"));
        assert!(message.contains("select_tab"));
    }

    #[test]
    fn tab_id_is_extracted_from_flat_and_nested_bridge_shapes() {
        assert_eq!(tab_id_from_value(&json!({"tabId": 17})), Some(17));
        assert_eq!(tab_id_from_value(&json!({"id": "18"})), Some(18));
        assert_eq!(tab_id_from_value(&json!({"tab": {"id": 19}})), Some(19));
        assert_eq!(
            tab_id_from_value(&json!({"url": "https://example.test"})),
            None
        );
    }

    /// Wire names verified against the bundled extension: `back`/`forward`
    /// (not goBack/goForward), `drag` (not dragAndDrop), singular cookie
    /// actions (not getCookies/setCookies/deleteCookie).
    #[test]
    fn bridge_action_names_match_the_extension_wire_protocol() {
        let cases = [
            ("go_back", "back"),
            ("go_forward", "forward"),
            ("drag_and_drop", "drag"),
            ("get_cookies", "listCookies"),
            ("set_cookies", "setCookie"),
            ("delete_cookie", "removeCookie"),
        ];
        for (logical, wire) in cases {
            let mut value = json!({"action": logical});
            if logical == "drag_and_drop" {
                value["selector"] = json!("#src");
                value["target_selector"] = json!("#dst");
            }
            if logical == "set_cookies" {
                value["cookies"] = json!([{"name": "n", "value": "v", "domain": "example.com"}]);
            }
            if logical == "delete_cookie" {
                value["cookie_name"] = json!("n");
                value["cookie_domain"] = json!("example.com");
            }
            let input = browser_input(value);
            let (action, _, _) = bridge_request(logical, &input).expect("bridge request");
            assert_eq!(action, wire, "logical action {logical}");
        }
    }

    /// `press` used to be routed to `evaluate` with a hand-rolled script,
    /// which assigned `el.value` directly — invisible to React-controlled
    /// inputs — and could not move focus (Tab) or close a dialog (Escape).
    /// The extension ships a native `press` action; use it.
    #[test]
    fn press_uses_the_native_key_action_not_eval() {
        let input = browser_input(json!({
            "action": "press",
            "key": "Enter",
            "selector": "#submit",
            "submit": true,
        }));
        let (action, params, _) = bridge_request("press", &input).expect("bridge request");
        assert_eq!(action, "press");
        assert_eq!(params["key"], "Enter");
        assert_eq!(params["submit"], true);
        assert_eq!(params["selector"], "#submit");
        assert!(
            params.get("script").is_none(),
            "press must not be smuggled through evaluate"
        );
    }

    #[test]
    fn press_requires_a_key() {
        let input = browser_input(json!({"action": "press"}));
        let error = bridge_request("press", &input).expect_err("missing key");
        assert!(error.to_string().contains("key is required"), "{error}");
    }

    /// `typeInto` in the content script never reads a `submit` param, so
    /// forwarding it was a lie. The submit is now a separate native Enter
    /// press issued after the text lands, so it must not be sent with `type`.
    #[test]
    fn type_does_not_forward_a_dead_submit_param() {
        let input = browser_input(json!({
            "action": "type",
            "text": "hello",
            "submit": true,
        }));
        let (action, params, _) = bridge_request("type", &input).expect("bridge request");
        assert_eq!(action, "type");
        assert_eq!(params["text"], "hello");
        assert!(
            params.get("submit").is_none(),
            "the content script ignores type/submit; the Enter press handles it"
        );
    }

    /// `fillForm` *does* honor a top-level `submit`, unlike `typeInto`.
    #[test]
    fn fill_form_forwards_submit_because_the_bridge_reads_it() {
        let input = browser_input(json!({
            "action": "fill_form",
            "fields": [{"selector": "#q", "value": "x"}],
            "submit": true,
        }));
        let (action, params, _) = bridge_request("fill_form", &input).expect("bridge request");
        assert_eq!(action, "fillForm");
        assert_eq!(params["submit"], true);
    }

    /// The content script clamps with `maxChars` / `textMaxChars`;
    /// `maxLength` appears nowhere in the extension, so the documented cap
    /// used to be silently ignored for every format.
    #[test]
    fn get_content_sends_the_extension_length_keys() {
        let input = browser_input(json!({
            "action": "get_content",
            "format": "text",
            "max_length": 1234,
        }));
        let (_, params, _) = bridge_request("get_content", &input).expect("bridge request");
        assert_eq!(params["maxChars"], 1234);
        assert!(params.get("maxLength").is_none());

        let annotated = browser_input(json!({
            "action": "get_content",
            "format": "annotated",
            "max_length": 999,
        }));
        let (_, params, _) = bridge_request("get_content", &annotated).expect("bridge request");
        assert_eq!(params["maxChars"], 999);
        assert_eq!(
            params["textMaxChars"], 999,
            "the annotated text section has its own cap"
        );

        // html keeps the historical 60k default; text keeps none.
        let html = browser_input(json!({"action": "get_content", "format": "html"}));
        let (_, params, _) = bridge_request("get_content", &html).expect("bridge request");
        assert_eq!(params["maxChars"], 60_000);
        let plain = browser_input(json!({"action": "get_content", "format": "text"}));
        let (_, params, _) = bridge_request("get_content", &plain).expect("bridge request");
        assert!(params.get("maxChars").is_none());
    }

    /// `waitFor` has no quiet-period mode, and `domStable`/`networkIdle` match
    /// nothing in the extension, so those waits silently degraded into a
    /// readyState check. They must select the `waitForStable` action.
    #[test]
    fn wait_quiet_period_modes_route_to_wait_for_stable() {
        for position in ["dom-stable", "network-idle"] {
            let input = browser_input(json!({
                "action": "wait",
                "position": position,
                "timeout_ms": 5000,
            }));
            let (action, params, _) = bridge_request("wait", &input).expect("bridge request");
            assert_eq!(action, "waitForStable", "position {position}");
            assert_eq!(params["timeoutMs"], 5000);
            assert!(params.get("domStable").is_none());
            assert!(params.get("networkIdle").is_none());
        }

        // A target wait still uses `waitFor`.
        let input = browser_input(json!({"action": "wait", "text": "Loaded"}));
        let (action, _, _) = bridge_request("wait", &input).expect("bridge request");
        assert_eq!(action, "waitFor");
    }

    #[test]
    fn wait_rejects_an_unknown_position() {
        let input = browser_input(json!({"action": "wait", "position": "dom-stabel"}));
        let error = bridge_request("wait", &input).expect_err("typo'd position");
        assert!(error.to_string().contains("dom-stabel"), "{error}");
    }

    /// `wait` with nothing but `timeout_ms` must be honored locally: the
    /// extension's `waitFor` resolves a targetless request against
    /// `document.readyState`, which is already true, so it returned instantly.
    #[test]
    fn bare_timeout_wait_is_detected_as_a_fixed_delay() {
        let delay = browser_input(json!({"action": "wait", "timeout_ms": 750}));
        assert!(is_fixed_delay_wait(&delay));

        // A target means it is a real bridge wait.
        for extra in [
            json!({"selector": "#done"}),
            json!({"text": "Done"}),
            json!({"contains": "Done"}),
            json!({"position": "dom-stable"}),
        ] {
            let mut value = json!({"action": "wait", "timeout_ms": 750});
            value
                .as_object_mut()
                .expect("object")
                .extend(extra.as_object().expect("object").clone());
            let input = browser_input(value);
            assert!(
                !is_fixed_delay_wait(&input),
                "{extra} should not be a bare delay"
            );
        }
    }

    /// The extension reads `files: [{name, type, data}]` with base64 data;
    /// `filePath` appears nowhere in it, so every upload failed with
    /// "uploadFile requires file or files".
    #[tokio::test]
    async fn upload_sends_inline_base64_files() {
        let dir =
            std::env::temp_dir().join(format!("alphacode-upload-test-{}", std::process::id()));
        tokio::fs::create_dir_all(&dir).await.expect("temp dir");
        let path = dir.join("payload.json");
        tokio::fs::write(&path, br#"{"a":1}"#)
            .await
            .expect("write fixture");

        let input = browser_input(json!({
            "action": "upload",
            "path": path.to_string_lossy(),
            "selector": "input[type=file]",
        }));
        let mut params = upload_params(&input).await.expect("upload params");
        apply_common_targeting(&mut params, &input);

        let files = params["files"].as_array().expect("files array");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0]["name"], "payload.json");
        assert_eq!(files[0]["type"], "application/json");
        assert_eq!(files[0]["data"], STANDARD.encode(br#"{"a":1}"#));
        assert_eq!(params["selector"], "input[type=file]");
        assert!(
            params.get("filePath").is_none(),
            "filePath is not a bridge parameter"
        );

        tokio::fs::remove_dir_all(&dir).await.ok();
    }

    #[tokio::test]
    async fn upload_reports_a_missing_file_clearly() {
        let input = browser_input(json!({
            "action": "upload",
            "path": "/definitely/not/here/alphacode-missing.bin",
        }));
        let error = upload_params(&input)
            .await
            .expect_err("missing file must fail");
        assert!(error.to_string().contains("no such file"), "{error}");
    }

    #[test]
    fn media_type_inference_covers_common_uploads() {
        assert_eq!(media_type_for("a/b/report.pdf"), "application/pdf");
        assert_eq!(media_type_for("shot.PNG"), "image/png");
        assert_eq!(media_type_for("notes.txt"), "text/plain");
        assert_eq!(media_type_for("noextension"), "application/octet-stream");
    }

    /// The reported failure was `Browser bridge action 'uploadFile' failed:
    /// Error: uploadFile requires file or files` — the extension rejecting a
    /// payload that carried a path instead of inline bytes.
    #[test]
    fn upload_bridge_error_names_the_inline_bytes_requirement() {
        let error = enrich_browser_error(
            "upload",
            anyhow::anyhow!(
                "Browser bridge action 'uploadFile' failed: Error: uploadFile requires file or files"
            ),
        );
        let message = error.to_string();
        assert!(message.contains("files"), "{message}");
        assert!(message.contains("base64"), "{message}");
    }

    #[test]
    fn upload_missing_file_input_suggests_finding_the_input() {
        let error = enrich_browser_error(
            "upload",
            anyhow::anyhow!(
                "Browser bridge action 'uploadFile' failed: Error: No file input found"
            ),
        );
        let message = error.to_string();
        assert!(message.contains("interactables"), "{message}");
        assert!(message.contains("selector"), "{message}");
    }

    /// `screenshot` returns `{tabId, dataUrl, method}`; the old code looked
    /// for a `saved` path that never exists and reported a temp file the
    /// bridge never wrote.
    #[test]
    fn screenshot_decodes_the_data_url_the_bridge_actually_returns() {
        let result = json!({
            "tabId": 12,
            "method": "captureTab",
            "dataUrl": "data:image/png;base64,aGVsbG8=",
        });
        let (media_type, payload) = screenshot_data_url(&result).expect("data url");
        assert_eq!(media_type, "image/png");
        assert_eq!(payload, "aGVsbG8=");
        assert_eq!(
            STANDARD.decode(payload).expect("decode"),
            b"hello".to_vec(),
            "the payload is already base64 and must be passed through as-is"
        );
    }

    #[test]
    fn screenshot_rejects_a_non_base64_or_missing_data_url() {
        assert!(screenshot_data_url(&json!({"dataUrl": "data:image/png,raw"})).is_none());
        assert!(screenshot_data_url(&json!({"dataUrl": "data:image/png;base64,"})).is_none());
        assert!(screenshot_data_url(&json!({"tabId": 3, "method": "captureTab"})).is_none());
    }

    #[test]
    fn bridge_error_summaries_never_inline_a_data_url() {
        let result = json!({
            "tabId": 3,
            "method": "captureTab",
            "dataUrl": format!("data:image/png;base64,{}", "A".repeat(5_000)),
        });
        let summary = summarize_bridge_result(&result);
        assert!(summary.contains("dataUrl=<string"), "{summary}");
        assert!(!summary.contains("AAAAA"), "must not inline the payload");
    }

    /// `open` used to return (and render) a full annotated content dump.
    #[test]
    fn open_suppresses_the_content_dump_and_renders_a_summary() {
        let input = browser_input(json!({"action": "open", "url": "https://example.test"}));
        let (_, params, _) = bridge_request("open", &input).expect("bridge request");
        assert_eq!(params["returnContent"], false);

        let rendered = format_navigate_result(&json!({"tab": {
            "tabId": 4,
            "url": "https://example.test/ok",
            "title": "OK",
            "status": "complete",
        }}));
        assert!(rendered.contains("tab_id: 4"), "{rendered}");
        assert!(
            rendered.contains("url: https://example.test/ok"),
            "{rendered}"
        );
        assert!(rendered.contains("status: complete"), "{rendered}");
        assert!(rendered.contains("snapshot"), "{rendered}");
    }

    #[test]
    fn schema_action_enum_matches_known_actions() {
        let schema = BrowserTool::new().parameters_schema();
        let published = schema["properties"]["action"]["enum"]
            .as_array()
            .expect("action enum")
            .iter()
            .map(|value| value.as_str().expect("string").to_string())
            .collect::<Vec<_>>();
        let known = KNOWN_ACTIONS
            .iter()
            .map(|action| (*action).to_string())
            .collect::<Vec<_>>();
        assert_eq!(
            published, known,
            "the published action enum and the dispatch table must not drift"
        );
    }

    #[test]
    fn unknown_action_is_rejected_by_name() {
        let input = browser_input(json!({"action": "clik"}));
        let error = bridge_request("clik", &input).expect_err("unknown action");
        let message = error.to_string();
        assert!(
            message.contains("Unsupported browser action: 'clik'"),
            "{message}"
        );
        assert!(message.contains("Valid actions"), "{message}");
    }

    /// Playwright spells file upload `setInputFiles`; Puppeteer/Selenium users
    /// write `set_files` / `setFiles`. All three used to be rejected outright
    /// even though `upload` does exactly what they mean.
    #[test]
    fn file_upload_aliases_normalize_to_upload() {
        for alias in [
            "set_files",
            "setFiles",
            "set_input_files",
            "setInputFiles",
            "upload_file",
            "upload_files",
            "uploadFile",
            "attach_file",
            "file_upload",
        ] {
            assert_eq!(normalize_action(alias), "upload", "alias {alias}");
            assert!(
                KNOWN_ACTIONS.contains(&normalize_action(alias)),
                "{alias} must normalize to a dispatchable action"
            );
        }
    }

    /// A normalized alias has to work end to end, not just reach the dispatch
    /// table: it must still build a real `uploadFile` bridge request.
    #[test]
    fn upload_alias_reaches_the_upload_file_bridge_action() {
        let input = browser_input(json!({"action": "set_files", "path": "payload.php"}));
        let (bridge_action, _params, _) =
            bridge_request(normalize_action("set_files"), &input).expect("bridge request");
        assert_eq!(bridge_action, "uploadFile");
    }

    #[test]
    fn other_playwright_style_aliases_normalize() {
        assert_eq!(normalize_action("evaluate"), "eval");
        assert_eq!(normalize_action("js"), "eval");
        assert_eq!(normalize_action("goto"), "open");
        assert_eq!(normalize_action("back"), "go_back");
        assert_eq!(normalize_action("forward"), "go_forward");
        assert_eq!(normalize_action("close"), "close_tab");
        assert_eq!(normalize_action("text"), "get_content");
        assert_eq!(normalize_action("press_key"), "press");
        // The canonical names must pass through untouched.
        for canonical in KNOWN_ACTIONS {
            assert_eq!(
                normalize_action(canonical),
                *canonical,
                "canonical action {canonical} must not be rewritten"
            );
        }
    }

    #[test]
    fn unknown_action_suggests_the_canonical_name() {
        // An exact alias match.
        let via_alias = unsupported_action_message("navigate");
        assert!(via_alias.contains("Did you mean"), "{via_alias}");
        assert!(via_alias.contains("'open'"), "{via_alias}");

        // A near-miss that only the edit-distance heuristic can catch.
        let via_fuzzy = unsupported_action_message("uploa");
        assert!(via_fuzzy.contains("Did you mean"), "{via_fuzzy}");
        assert!(via_fuzzy.contains("'upload'"), "{via_fuzzy}");

        // A name with no near match still gets the full list, never a bogus
        // suggestion and never a truncated message.
        let unrelated = unsupported_action_message("zzzzzz");
        assert!(!unrelated.contains("Did you mean"), "{unrelated}");
        assert!(unrelated.contains("Valid actions"), "{unrelated}");
    }

    #[test]
    fn provider_command_forwards_the_callers_wire_action() {
        let input = browser_input(json!({
            "action": "provider_command",
            "provider_action": "saveAsPDF",
            "params": {"landscape": true},
        }));
        let (action, params, _) =
            bridge_request("provider_command", &input).expect("bridge request");
        assert_eq!(action, "saveAsPDF");
        assert_eq!(params, json!({"landscape": true}));

        let missing = browser_input(json!({"action": "provider_command"}));
        let error = bridge_request("provider_command", &missing).expect_err("no provider action");
        assert!(
            error.to_string().contains("provider_action is required"),
            "{error}"
        );
    }

    /// The pin map used to grow one entry per session forever and silently
    /// stop working entirely if the mutex was ever poisoned.
    #[test]
    fn session_tab_pins_stay_bounded_and_keep_working() {
        let existing = {
            let guard = session_tab_pins();
            guard.as_ref().map(|pins| pins.len()).unwrap_or_default()
        };
        for index in 0..(MAX_TRACKED_SESSION_TABS + 32) {
            remember_session_tab(&format!("bounded-session-{index}"), index as i64);
        }
        let total = session_tab_pins()
            .as_ref()
            .map(|pins| pins.len())
            .unwrap_or_default();
        assert!(
            total <= MAX_TRACKED_SESSION_TABS,
            "pin map must stay bounded, got {total}"
        );
        // The most recent pin is always the one that survives.
        let newest = MAX_TRACKED_SESSION_TABS + 31;
        assert_eq!(
            session_tab_pin(&format!("bounded-session-{newest}")),
            Some(newest as i64)
        );
        // Leave the map roughly as we found it for the other pin tests.
        for index in 0..(MAX_TRACKED_SESSION_TABS + 32) {
            forget_session_tab(&format!("bounded-session-{index}"), index as i64);
        }
        let after = session_tab_pins()
            .as_ref()
            .map(|pins| pins.len())
            .unwrap_or_default();
        assert!(after <= existing.max(1), "cleanup left {after} pins behind");
    }

    #[test]
    fn session_tab_pins_survive_a_poisoned_mutex() {
        // A poisoned lock used to be swallowed, which silently disabled tab
        // pinning for the rest of the process.
        static POISON: Mutex<()> = Mutex::new(());
        let _ = std::panic::catch_unwind(|| {
            let _guard = POISON.lock().expect("poison");
            panic!("poison the pin mutex on purpose");
        });
        assert!(POISON.lock().is_err(), "the mutex should be poisoned now");
        POISON.clear_poison();

        let session = "poison-recovery-session";
        remember_session_tab(session, 4321);
        assert_eq!(session_tab_pin(session), Some(4321));
        forget_session_tab(session, 4321);
    }

    #[test]
    fn drag_params_use_source_and_target_keys() {
        let input = browser_input(json!({
            "action": "drag_and_drop",
            "selector": "#src",
            "target_selector": "#dst",
        }));
        let (action, params, _) = bridge_request("drag_and_drop", &input).expect("bridge request");
        assert_eq!(action, "drag");
        assert_eq!(params["source"], "#src");
        assert_eq!(params["target"], "#dst");
    }

    #[test]
    fn click_forwards_xy_coordinates() {
        let input = browser_input(json!({"action": "click", "x": 120, "y": 340}));
        let (_, params, _) = bridge_request("click", &input).expect("bridge request");
        assert_eq!(params["x"].as_f64(), Some(120.0));
        assert_eq!(params["y"].as_f64(), Some(340.0));
    }

    #[test]
    fn wait_uses_extension_timeout_key_and_text_alias() {
        let input = browser_input(json!({
            "action": "wait",
            "timeout_ms": 500,
            "contains": "loaded",
        }));
        let (_, params, _) = bridge_request("wait", &input).expect("bridge request");
        assert_eq!(params["timeoutMs"], 500);
        assert_eq!(params["text"], "loaded");
        assert!(params.get("timeout").is_none());
    }

    #[test]
    fn set_cookie_params_require_a_url_or_domain() {
        let cookie = CookieInput {
            name: "sid".to_string(),
            value: "abc".to_string(),
            domain: Some("example.com".to_string()),
            path: Some("/app".to_string()),
            secure: None,
            http_only: Some(true),
            expires: Some(1_700_000_000.0),
        };
        let params = set_cookie_params(&cookie, None).expect("cookie params");
        assert_eq!(params["url"], "https://example.com/app");
        assert_eq!(params["httpOnly"], true);
        assert_eq!(params["expirationDate"], 1_700_000_000.0);

        let bare = CookieInput {
            name: "x".to_string(),
            value: "y".to_string(),
            domain: None,
            path: None,
            secure: None,
            http_only: None,
            expires: None,
        };
        assert!(set_cookie_params(&bare, None).is_err());
        let with_url = set_cookie_params(&bare, Some("https://example.com/")).expect("url params");
        assert_eq!(with_url["url"], "https://example.com/");
    }

    #[test]
    fn cookie_action_url_prefers_explicit_url() {
        assert_eq!(
            cookie_action_url(Some("https://a.test/x"), Some("b.test"), None).as_deref(),
            Some("https://a.test/x")
        );
        assert_eq!(
            cookie_action_url(None, Some(".example.com"), Some("/sub")).as_deref(),
            Some("https://example.com/sub")
        );
        assert_eq!(cookie_action_url(None, None, None), None);
    }

    #[test]
    fn element_not_found_gets_recovery_hint_for_click() {
        let error = enrich_browser_error(
            "click",
            anyhow::anyhow!("Browser bridge action 'click' failed: Error: Element not found"),
        );
        let message = error.to_string();
        assert!(message.contains("snapshot"));
        assert!(message.contains("interactables"));
        // The hint must name the affordances that actually exist, or it sends
        // the agent looking for something the tool cannot do.
        assert!(message.contains("index"), "index not suggested: {message}");
        assert!(
            message.contains("include_hidden"),
            "include_hidden not suggested: {message}"
        );
    }

    /// `find` was rejected outright, so an agent trying to locate an element had
    /// to invent a different action name before it could even start.
    #[test]
    fn find_style_aliases_resolve_to_interactables() {
        for alias in [
            "find",
            "search",
            "locate",
            "find_element",
            "query",
            "elements",
            "list_interactables",
        ] {
            assert_eq!(
                normalize_action(alias),
                "interactables",
                "alias {alias} was not recognized"
            );
            // And it must survive the KNOWN_ACTIONS gate `execute` applies, or
            // the alias normalizes to a valid action and is then rejected.
            let canonical = normalize_action(alias);
            assert!(
                KNOWN_ACTIONS.contains(&canonical),
                "{alias} normalized to `{canonical}`, which is not a dispatchable action"
            );
            // `bridge_request` is reached with the already-normalized action,
            // exactly as `execute` calls it.
            let input = browser_input(json!({ "action": alias }));
            let (action, params, _) = bridge_request(canonical, &input).expect("should dispatch");
            assert_eq!(
                action, "getInteractables",
                "alias {alias} dispatched wrongly"
            );
            assert!(params.get("selector").is_none());
        }
    }

    #[test]
    fn unsupported_action_message_suggests_interactables_for_find() {
        let message = unsupported_action_message("find");
        assert!(message.contains("'interactables'"), "{message}");
    }

    /// `interactables` numbered its results 1., 2., 3. while the only way to
    /// address a specific element was an `index` the tool could not accept, so
    /// the agent read a number that meant nothing and guessed a selector.
    #[test]
    fn interactables_output_prints_the_index_it_accepts_back() {
        let result = json!({
            "elements": [
                { "index": 0, "type": "button", "tag": "BUTTON", "text": "Cancel", "selector": "#cancel" },
                { "index": 1, "type": "button", "tag": "BUTTON", "text": "Sign in", "selector": "#login" },
            ]
        });
        let rendered = format_interactables_result(&result);
        assert!(rendered.contains("index=0"), "{rendered}");
        assert!(rendered.contains("index=1"), "{rendered}");
        assert!(rendered.contains("Sign in"), "{rendered}");
        // The 1-based ordinal is gone; it could not be used.
        assert!(!rendered.contains("1. ["), "{rendered}");
    }

    /// The index the tool prints must be the one it forwards, or the round trip
    /// the output invites does not work.
    #[test]
    fn index_and_include_hidden_reach_the_bridge() {
        let input = browser_input(json!({
            "action": "click",
            "selector": "button",
            "index": 2,
            "include_hidden": true,
        }));
        let (_, params, _) = bridge_request("click", &input).expect("bridge request");
        assert_eq!(params["index"], json!(2));
        assert_eq!(params["includeHidden"], json!(true));

        // And absent unless asked for, so default behaviour is unchanged.
        let plain = browser_input(json!({"action": "click", "selector": "button"}));
        let (_, params, _) = bridge_request("click", &plain).expect("bridge request");
        assert!(params.get("index").is_none());
        assert!(params.get("includeHidden").is_none());
    }

    /// `contains` is documented as the text-matching alias, but `interactables`
    /// dropped it — so the one action whose purpose is locating an element
    /// ignored its own filter and returned everything.
    #[test]
    fn interactables_honors_contains_as_a_filter() {
        let input = browser_input(json!({"action": "interactables", "contains": "Sign in"}));
        let (_, params, _) = bridge_request("interactables", &input).expect("bridge request");
        assert_eq!(params["text"], json!("Sign in"));
    }

    #[test]
    fn click_requires_a_target() {
        let input = browser_input(json!({"action": "click"}));
        let error = bridge_request("click", &input).expect_err("click needs a target");
        assert!(
            error.to_string().contains("requires one of"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn eval_not_defined_suggests_page_world() {
        let error = enrich_browser_error(
            "eval",
            anyhow::anyhow!(
                "Browser bridge action 'evaluate' failed: ReferenceError: submitImport is not defined"
            ),
        );
        let message = error.to_string();
        assert!(message.contains("page_world"));
    }

    #[test]
    fn invalid_input_names_the_field_and_echoes_received() {
        let bad = json!({"action": "click", "x": "120"});
        let error = serde_json::from_value::<BrowserInput>(bad.clone()).expect_err("x as string");
        let message = invalid_input_error(&bad, &error).to_string();
        assert!(message.contains("Invalid browser tool input"), "{message}");
        assert!(message.contains("\"x\""), "{message}");
        assert!(message.contains("120"), "{message}");
    }

    // ---- Bug fix #13: active-tab isolation -------------------------------

    #[test]
    fn open_defaults_to_a_new_background_tab_instead_of_hijacking_the_active_tab() {
        let input = browser_input(json!({"action": "open", "url": "https://example.test"}));
        let (_, params, _) = bridge_request("open", &input).expect("bridge request");
        assert_eq!(
            params["newTab"], true,
            "open must not navigate the user's tab"
        );
        assert_eq!(params["wait"], true);
    }

    #[test]
    fn open_with_explicit_tab_id_reuses_that_tab() {
        let input = browser_input(json!(
            {"action": "open", "url": "https://example.test", "tab_id": 7}
        ));
        let (_, params, _) = bridge_request("open", &input).expect("bridge request");
        assert_eq!(params["newTab"], false);
        assert_eq!(params["tabId"], 7);
    }

    #[test]
    fn open_with_new_tab_false_keeps_in_place_navigation() {
        let input = browser_input(json!(
            {"action": "open", "url": "https://example.test", "new_tab": false}
        ));
        let (_, params, _) = bridge_request("open", &input).expect("bridge request");
        assert_eq!(params["newTab"], false);
    }

    #[test]
    fn select_tab_does_not_steal_foreground_by_default() {
        let input = browser_input(json!({"action": "select_tab", "tab_id": 9}));
        let (_, params, _) = bridge_request("select_tab", &input).expect("bridge request");
        assert_eq!(params["focus"], false, "select_tab must stay backgrounded");

        let input = browser_input(json!({"action": "select_tab", "tab_id": 9, "focus": true}));
        let (_, params, _) = bridge_request("select_tab", &input).expect("bridge request");
        assert_eq!(params["focus"], true);
    }

    #[test]
    fn session_tab_pin_lifecycle_scopes_and_releases_the_agent_tab() {
        let session = "pin-lifecycle-test-session";
        assert_eq!(session_tab_pin(session), None);

        remember_session_tab(session, 1234);
        assert_eq!(session_tab_pin(session), Some(1234));
        // A different session is unaffected.
        assert_eq!(session_tab_pin("pin-lifecycle-other-session"), None);

        // Forgetting a different tab id leaves the pin intact.
        forget_session_tab(session, 999);
        assert_eq!(session_tab_pin(session), Some(1234));

        forget_session_tab(session, 1234);
        assert_eq!(session_tab_pin(session), None);
    }

    #[test]
    fn apply_session_tab_pin_targets_the_session_tab_without_overriding_explicit_tab_id() {
        let session = "pin-apply-test-session";
        remember_session_tab(session, 55);

        // `apply_session_tab_pin` takes the serialized params (`&mut Value`),
        // which is what `execute_firefox_action` has on hand.
        let mut params = Value::Object(Map::new());
        assert!(apply_session_tab_pin(&mut params, session));
        assert_eq!(params["tabId"], 55);

        let mut explicit = Value::Object(Map::from_iter([("tabId".to_string(), json!(77))]));
        assert!(!apply_session_tab_pin(&mut explicit, session));
        assert_eq!(explicit["tabId"], 77);

        forget_session_tab(session, 55);
        assert_eq!(session_tab_pin(session), None);
    }

    #[test]
    fn annotate_pinned_targeting_marks_the_result_without_dropping_fields() {
        let annotated = annotate_pinned_targeting(json!({"tab": {"tabId": 5}}));
        assert_eq!(annotated["pinnedTab"], true);
        assert_eq!(annotated["tab"]["tabId"], 5);
    }

    #[test]
    fn tab_scoped_mutating_actions_cover_every_page_mutator() {
        // Every action that mutates state in whichever tab the bridge
        // resolves must be listed, or the agent could click/type/eval in
        // the user's tab when no pin exists.
        for action in [
            "reload",
            "go_back",
            "go_forward",
            "close_tab",
            "click",
            "hover",
            "type",
            "fill_form",
            "select",
            "drag_and_drop",
            "scroll",
            "upload",
            "press",
            "eval",
        ] {
            assert!(
                TAB_SCOPED_MUTATING_ACTIONS.contains(&action),
                "{action} must be tab-scoped"
            );
        }
        // Reads, status, and navigation that opens its own tab stay allowed
        // without a pin.
        for action in ["status", "list_tabs", "new_tab", "open", "snapshot"] {
            assert!(
                !TAB_SCOPED_MUTATING_ACTIONS.contains(&action),
                "{action} must stay allowed without a pin"
            );
        }
    }

    /// The extension's `captureVisibleTab` fallback does
    /// `tabs.update(tabId, {active: true})`, so an untargeted screenshot
    /// raises the tab in front of the user — it is not the pure read it looks
    /// like. `screenshot` is tab-scoped for exactly that reason.
    #[test]
    fn screenshot_is_tab_scoped_because_it_can_raise_the_users_tab() {
        assert!(TAB_SCOPED_MUTATING_ACTIONS.contains(&"screenshot"));
    }

    /// `open` with `new_tab: false` and no tab target reaches the extension's
    /// `resolveTabId({})` branch and navigates the user's active tab, so the
    /// guard in `execute_firefox_action` has to cover that case even though
    /// `open` is not in the list above.
    #[test]
    fn in_place_open_is_detected_as_tab_scoped() {
        let input = browser_input(json!({
            "action": "open",
            "url": "https://example.test",
            "new_tab": false,
        }));
        let (_, params, _) = bridge_request("open", &input).expect("bridge request");
        assert_eq!(params["newTab"], false);
        assert!(params.get("tabId").is_none());
        assert!(
            params["newTab"] == json!(false),
            "the guard keys off exactly this value"
        );

        // The default (own new tab) and an explicit tab target stay allowed.
        let fresh = browser_input(json!({"action": "open", "url": "https://example.test"}));
        let (_, params, _) = bridge_request("open", &fresh).expect("bridge request");
        assert_eq!(params["newTab"], true);
        let targeted = browser_input(json!({
            "action": "open",
            "url": "https://example.test",
            "tab_id": 3,
        }));
        let (_, params, _) = bridge_request("open", &targeted).expect("bridge request");
        assert_eq!(params["tabId"], 3);
    }

    /// `set_cookies` is not tab-scoped: `cookieAction` addresses cookies by
    /// url/domain and never resolves a tab, so it must not need a pin.
    #[test]
    fn cookie_actions_are_not_tab_scoped() {
        for action in [
            "get_cookies",
            "set_cookies",
            "delete_cookie",
            "list_cookies",
        ] {
            assert!(
                !TAB_SCOPED_MUTATING_ACTIONS.contains(&action),
                "{action} is url-addressed, not tab-addressed"
            );
        }
    }

    /// Only idempotent reads may be retried — a retried click/type could
    /// double-submit.
    #[test]
    fn only_idempotent_reads_are_retryable() {
        for action in [
            "click",
            "type",
            "press",
            "fill_form",
            "select",
            "drag_and_drop",
            "scroll",
            "upload",
            "eval",
            "set_cookies",
            "delete_cookie",
            "close_tab",
            "reload",
            "go_back",
            "go_forward",
        ] {
            assert!(
                !RETRYABLE_ACTIONS.contains(&action),
                "{action} must never be retried automatically"
            );
        }
        for action in [
            "status",
            "list_tabs",
            "get_active_tab",
            "list_frames",
            "get_content",
            "interactables",
            "snapshot",
            "wait",
            "get_cookies",
        ] {
            assert!(
                RETRYABLE_ACTIONS.contains(&action),
                "{action} is a read and may be retried"
            );
        }
    }
}
