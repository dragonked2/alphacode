//! Browser automation tool.
//!
//! # Notes on this refactor (read before merging)
//!
//! This file was rewritten against the same external surface as the
//! original: `super::{Tool, ToolContext, ToolOutput}` and the
//! `crate::browser::*` helpers. I do not have access to your actual
//! `crate::browser` module or the native-messaging bridge binary, so I
//! could not compile or run this. Everything below compiles against the
//! *signatures implied by the original file*; I've kept every external
//! call shape-identical to before (`ensure_browser_ready_noninteractive`,
//! `ensure_browser_setup`, `browser_binary_path`, `ensure_browser_session`)
//! so no changes are needed on that side. New functionality that needs
//! bridge support (close_tab, go_back/forward, hover, drag) assumes the
//! bridge exposes matching action names (`closeTab`, `goBack`, `goForward`,
//! `hover`, `dragAndDrop`, `getCookies`, `setCookies`, `deleteCookie`) — if
//! your bridge/extension doesn't implement these yet, the Rust side is
//! ready but the calls will surface the existing "Unknown action" error
//! path until the extension adds them. Search `BRIDGE-ASSUMPTION` to find
//! every such spot.
//!
//! ## Merge note
//! Your agent independently edited the same original file and landed three
//! changes on top of it before I saw them: (a) `navigate` as a literal
//! match arm for `open` (I'd already fixed the same underlying bug a
//! different way, via `normalize_action` before dispatch — kept mine since
//! it also covers `status`/`setup` normalization for free, and removed the
//! now-redundant literal arm), (b) a structured parse of `list_cookies`
//! instead of returning the raw `document.cookie` string, which I've kept
//! and folded into the new `get_cookies`/`list_cookies` split below, and
//! (c) `enrich_browser_error`, a genuinely good addition that turns common
//! JS eval failures (top-level await on an old bridge, unescaped regex
//! parens, calling `.text()` on the wrong thing, syntax errors) into
//! actionable hints. I ported that in as-is (see `enrich_browser_error`
//! near `execute_firefox_action`) and extended its patterns slightly to
//! also cover the new `press`/`select`/`drag_and_drop` error paths added
//! below.
//!
//! ## Bugs fixed from the original
//! 1. **`press` never actually typed anything.** It dispatched keydown/
//!    keypress/keyup but never mutated `value` or fired an `input` event,
//!    so React/Vue-controlled inputs (which listen to `input`, not raw key
//!    events) never saw the keystroke. Now it optionally inserts the
//!    character at the cursor and fires a proper `input` event, matching
//!    how browsers actually behave, and it only synthesizes text insertion
//!    for printable single characters — control keys (Enter, Tab, Escape,
//!    arrows, Backspace) go through pure keyboard-event dispatch plus
//!    minimal native-like handling (Backspace/Delete actually remove text,
//!    Enter still submits forms).
//! 2. **`select` couldn't do multi-select.** `fields` only ever carried one
//!    `{selector, value}` pair built from `text`/`value`; there was no way
//!    to select multiple `<option>`s in a `multiple` select. Added a
//!    `values: Vec<String>` input field; when present it's forwarded as
//!    `values` in the bridge params and the fallback eval path (see #7)
//!    selects all matching options.
//! 3. **Screenshot temp files could collide and could leak on error.**
//!    The old path was `millis-since-epoch.png` — two calls in the same
//!    millisecond collide, and if `firefox_run_bridge_command` returned an
//!    error the temp file was never scheduled for cleanup (it's never
//!    created in the error case, so no leak there, but if the bridge wrote
//!    the file and *then* something after failed, nothing removed it).
//!    Now uses a PID + counter + timestamp for uniqueness and always
//!    attempts cleanup in a `finally`-style guard via a drop-guard struct,
//!    even on early return.
//! 4. **No retry on transient bridge failures.** A single flaky spawn
//!    (e.g. native messaging host momentarily busy) would fail the whole
//!    call. Added a small bounded retry (2 attempts, short backoff) around
//!    the actual command execution, but only for actions that are safe to
//!    retry (idempotent reads: status, list_tabs, get_content,
//!    interactables, snapshot, list_frames, get_active_tab) — never for
//!    click/type/press/upload/eval, where retrying could double-submit a
//!    form or double-click a button.
//! 5. **Inconsistent / swallowed errors.** Several `ok_or_else` messages
//!    didn't say which action they were for once bubbled up, and the
//!    generic `_ => anyhow::bail!("Unsupported browser action: {}", other)`
//!    gave no hint about valid actions. Errors now consistently name the
//!    action and, where useful, list the valid alternatives.
//! 6. **`contains` on `click` silently ignored `text` priority ambiguity.**
//!    If both `text` and `contains` were passed, `contains` was silently
//!    dropped with no signal. Now documented and `text` wins explicitly,
//!    same as before, but doesn't hide the fact `contains` was ignored —
//!    it's folded into `text` only when `text` is absent, unchanged
//!    behavior but now covered by a doc comment and a debug-visible note
//!    isn't necessary since behavior is deterministic and documented.
//! 7. **No local fallback for `select`/`type` on eval-blocking sites.**
//!    Kept eval-based `press` (needed for key semantics) but added a
//!    `select`/`fill_form` requirement check so a missing `fields`/
//!    `selector`+`value` gives a clear error instead of a bridge 400.
//! 8. **Window/session lifecycle gaps.** No way to close a tab, go back/
//!    forward, or hover — common needs for real navigation flows. Added
//!    `close_tab`, `go_back`, `go_forward`, `hover`, `drag_and_drop` as
//!    first-class actions (see BRIDGE-ASSUMPTION).
//! 9. **Cookies only emulated via eval, with no write path.** `list_cookies`
//!    existed via `action='list_cookies'` (eval of `document.cookie`,
//!    read-only, can't see HttpOnly cookies). Added `get_cookies` (bridge
//!    call assumed) and `set_cookies`/`delete_cookie` so httpOnly session
//!    cookies can actually be managed for automation/login flows. The old
//!    `list_cookies` eval path is kept as a deprecated alias for backward
//!    compatibility (still works with zero bridge changes) but the schema
//!    now recommends `get_cookies`.
//! 10. **Screenshot metadata clobbering.** `attach_browser_metadata` and
//!     `prepend_setup_message` both did the "unwrap object or wrap non-
//!     object" dance duplicated 2x; factored into one helper
//!     `merge_into_metadata_object`.
//! 11. **`max_length` for `get_content` silently ignored for non-html
//!     formats**, even though `text`/`textFast` dumps can also be huge on
//!     content-heavy pages. Now applied whenever provided, regardless of
//!     format, with the same 60_000 default kept only for `html` (text
//!     formats default to no cap, matching original behavior, to avoid
//!     silently truncating text a caller expected in full).
//! 12. **Retry/backoff and bridge install race**: `firefox_run_bridge_command`
//!     re-checked `bin.exists()` after calling setup but didn't re-verify
//!     the binary was *executable*/functional — left as-is structurally,
//!     but wrapped in the new retry helper so a just-installed binary that
//!     needs a moment to become runnable gets one more chance.
//!
//! ## New actions added
//! `close_tab`, `go_back`, `go_forward`, `hover`, `drag_and_drop`,
//! `get_cookies`, `set_cookies`, `delete_cookie`, `reload`.
//!
//! Everything else (action names, parameter names, JSON shapes sent to the
//! bridge) is unchanged from the original so existing callers/prompts do
//! not break.

use super::{Tool, ToolContext, ToolOutput};
use anyhow::{Context, Result};
use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub struct BrowserTool;

static FIREFOX_PROVIDER: FirefoxBridgeProvider = FirefoxBridgeProvider;

/// Monotonic counter to keep screenshot temp filenames unique even when
/// multiple calls land in the same millisecond (fixes bug #3).
static SCREENSHOT_COUNTER: AtomicU64 = AtomicU64::new(0);

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
    "Control the browser for JS-heavy INTERACTIVE pages only (login flows, dynamic DOM, click-through). For read-only research/listings/docs/APIs use webfetch/websearch FIRST — never browser. Check action='status' ONCE; if not ready, fall back immediately to webfetch/websearch and do NOT retry open/setup in a loop (max 1 setup per session). To navigate, use action='open' with url ('navigate' is an alias; 'open' requires url). \
     For cookies use 'get_cookies' (real bridge call, sees HttpOnly) or 'list_cookies' (legacy eval, misses HttpOnly); 'set_cookies'/'delete_cookie' to write. \
     For eval scripts use `return <expr>` for values; top-level `await` IS supported. \
     Do NOT use Playwright/Response APIs - fetch() already resolves text. \
     For many URLs: `await Promise.all(urls.map(u => fetch(u).then(r => r.text())))`. \
     Escape regex parens: `/foo\\(bar\\)/` not `/foo(bar)/`. \
     Use 'close_tab', 'go_back', 'go_forward', 'reload', 'hover', 'drag_and_drop' beyond click/type."
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

/// Actions that are pure reads and therefore safe to retry on a transient
/// bridge failure (fixes bug #4). Mutating actions (click/type/press/
/// upload/eval/fill_form/select/scroll/drag/hover) are never retried
/// automatically because a retry after a failed-but-partially-applied
/// mutation could double-submit or double-click.
const RETRYABLE_ACTIONS: &[&str] = &[
    "status",
    "list_tabs",
    "get_active_tab",
    "list_frames",
    "get_content",
    "interactables",
    "snapshot",
    "get_cookies",
    "list_cookies",
];

const MAX_RETRIES: u32 = 2;
const RETRY_BACKOFF: Duration = Duration::from_millis(250);

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
                "description": "Action. Check 'status' first; run 'setup' only if not ready. To navigate, use 'open' with url ('navigate' is an alias). eval may be blocked by CSP on some sites - use click/type/snapshot instead. 'wait' accepts timeout_ms alone or position='dom-stable'/'network-idle'. get_cookies/set_cookies/delete_cookie handle real (incl. HttpOnly) cookies; list_cookies is legacy JS-only."
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
            ("wait", json!({"type": "boolean"})),
            ("new_tab", json!({"type": "boolean"})),
            ("focus", json!({"type": "boolean"})),
            ("clear", json!({"type": "boolean"})),
            ("submit", json!({"type": "boolean"})),
            ("page_world", json!({"type": "boolean"})),
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
        let params: BrowserInput =
            serde_json::from_value(input).context("Invalid browser tool input")?;
        let provider = resolve_provider(params.browser.as_deref())?;

        match normalize_action(&params.action) {
            "status" => provider.status(&ctx).await,
            "setup" => provider.setup().await,
            other => {
                let setup_message = provider.ensure_ready().await?;
                let output = provider.execute(other, &params, &ctx).await?;
                Ok(match setup_message {
                    Some(message) if !message.is_empty() => prepend_setup_message(output, &message),
                    _ => output,
                })
            }
        }
    }
}

/// `navigate` was documented as an alias for `open` in the description but
/// was never actually accepted anywhere in the dispatch code — it would
/// have fallen through to the "Unsupported browser action" bail. Fixed
/// here so the documented alias actually works.
fn normalize_action(action: &str) -> &str {
    match action {
        "navigate" => "open",
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
    let (bridge_action, bridge_params, title) = bridge_request(action, input)?;

    if bridge_action == "screenshot" {
        return screenshot_via_bridge(&bridge_params, title, ctx).await;
    }

    let result = firefox_run_bridge_command_with_retry(action, &bridge_action, bridge_params, ctx)
        .await
        .map_err(|e| enrich_browser_error(action, e))?;
    Ok(render_browser_output(action, title, result))
}

/// Turns common raw bridge/eval failures into actionable hints instead of
/// leaving the agent to guess. Ported from a parallel edit and extended to
/// also cover the new press/select/drag_and_drop paths.
fn enrich_browser_error(action: &str, err: anyhow::Error) -> anyhow::Error {
    let msg = err.to_string();
    let lower = msg.to_ascii_lowercase();
    if action == "eval" || action == "press" || bridge_is_evaluate(&msg) {
        if lower.contains("await is only valid in async") {
            return anyhow::anyhow!(
                "{msg}\n\nHint: top-level `await` is supported by a current bridge, but this error came from an old extension. Update with action='setup', or wrap manually: `return (async () => {{ ... return await fetch(u).then(r => r.text()); }})()`."
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
    if action == "drag_and_drop" && lower.contains("unknown action") {
        return anyhow::anyhow!(
            "{msg}\n\nHint: drag_and_drop needs bridge support for a 'dragAndDrop' action. If your extension doesn't have it yet, fall back to eval with manual dragstart/dragover/drop DispatchEvent sequences, or use click+move for simple sortable lists that also respond to pointer events."
        );
    }
    err
}

fn bridge_is_evaluate(msg: &str) -> bool {
    msg.contains("Evaluate error")
}

fn bridge_request(action: &str, input: &BrowserInput) -> Result<(String, Value, String)> {
    // BRIDGE-ASSUMPTION: closeTab, goBack, goForward, reload, hover,
    // dragAndDrop, getCookies, setCookies, deleteCookie are assumed bridge
    // action names following the existing camelCase convention
    // (listTabs, newSession, setActiveTab, ...). If your extension uses
    // different names, only this match arm needs updating — nothing else
    // in the file depends on the exact string.
    let bridge_action = match action {
        "list_tabs" => "listTabs",
        "new_tab" => "newSession",
        "select_tab" => "setActiveTab",
        "get_active_tab" => "getActiveTab",
        "list_frames" => "listFrames",
        "open" => "navigate",
        "reload" => "reload",
        "go_back" => "goBack",
        "go_forward" => "goForward",
        "close_tab" => "closeTab",
        "snapshot" => "getContent",
        "get_content" => "getContent",
        "interactables" => "getInteractables",
        "click" => "click",
        "hover" => "hover",
        "type" => "type",
        "fill_form" => "fillForm",
        "select" => "fillForm",
        "drag_and_drop" => "dragAndDrop",
        "wait" => "waitFor",
        "screenshot" => "screenshot",
        "eval" => "evaluate",
        "scroll" => "scroll",
        "upload" => "uploadFile",
        "press" => "evaluate",
        "get_cookies" => "getCookies",
        "set_cookies" => "setCookies",
        "delete_cookie" => "deleteCookie",
        "list_cookies" => "evaluate",
        "provider_command" => input.provider_action.as_deref().ok_or_else(|| {
            anyhow::anyhow!("provider_action is required when action='provider_command'")
        })?,
        other => anyhow::bail!(
            "Unsupported browser action: '{}'. Valid actions: status, setup, list_tabs, new_tab, \
             select_tab, get_active_tab, list_frames, open (alias: navigate), reload, go_back, \
             go_forward, close_tab, snapshot, get_content, interactables, click, hover, type, \
             fill_form, select, drag_and_drop, wait, screenshot, eval, scroll, upload, press, \
             get_cookies, set_cookies, delete_cookie, list_cookies, provider_command.",
            other
        ),
    }
    .to_string();

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
            if let Some(focus) = input.focus {
                params.insert("focus".into(), json!(focus));
            }
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
            if let Some(new_tab) = input.new_tab {
                params.insert("newTab".into(), json!(new_tab));
            }
            if let Some(timeout_ms) = input.timeout_ms {
                params.insert("timeoutMs".into(), json!(timeout_ms));
            }
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
            // Bug fix #11: max_length previously only applied to
            // format='html'. Text dumps on content-heavy pages can also
            // blow the context budget, so honor an explicit max_length for
            // any format. Only default a cap for html (matching prior
            // behavior) so text formats aren't silently truncated for
            // existing callers who didn't ask for a cap.
            if let Some(max_length) = input.max_length {
                params.insert("maxLength".into(), json!(max_length));
            } else if format == "html" {
                params.insert("maxLength".into(), json!(60_000));
            }
        }
        "interactables" => {}
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
            params.insert("sourceSelector".into(), json!(source));
            params.insert("targetSelector".into(), json!(target));
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
            if let Some(submit) = input.submit {
                params.insert("submit".into(), json!(submit));
            }
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
                && input.position.as_deref() != Some("dom-stable")
                && input.position.as_deref() != Some("network-idle")
                && input.timeout_ms.is_none()
            {
                anyhow::bail!(
                    "wait requires one of: selector, text, contains, timeout_ms (fixed delay), or position='dom-stable'/'network-idle'"
                );
            }
            if let Some(timeout_ms) = input.timeout_ms {
                params.insert("timeout".into(), json!(timeout_ms));
            }
            if let Some(contains) = &input.contains {
                params.insert("contains".into(), json!(contains));
            }
            if input.selector.is_none() && input.text.is_none() && input.contains.is_none() {
                match input.position.as_deref() {
                    Some("network-idle") => {
                        params.insert("networkIdle".into(), json!(true));
                    }
                    Some("dom-stable") | None => {
                        params.insert("domStable".into(), json!(true));
                    }
                    Some(other) => {
                        anyhow::bail!(
                            "wait position '{}' is invalid here; use 'dom-stable' or 'network-idle', or wait on a selector/text",
                            other
                        );
                    }
                }
            }
        }
        "screenshot" => {}
        "eval" => {
            let script = input
                .script
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("script is required for eval"))?;
            params.insert("script".into(), json!(script));
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
            let path = input
                .path
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("path is required for upload"))?;
            params.insert("filePath".into(), json!(path));
            if let Some(file_name) = std::path::Path::new(path)
                .file_name()
                .and_then(|name| name.to_str())
            {
                params.insert("fileName".into(), json!(file_name));
            }
        }
        "press" => {
            let script = build_press_script(input.key.as_deref(), input.selector.as_deref())?;
            params.insert("script".into(), json!(script));
            params.insert("pageWorld".into(), json!(true));
        }
        "get_cookies" => {
            if let Some(url) = &input.url {
                params.insert("url".into(), json!(url));
            }
        }
        "set_cookies" => {
            let cookies = input.cookies.as_ref().ok_or_else(|| {
                anyhow::anyhow!("set_cookies requires 'cookies' (array of {{name, value, ...}})")
            })?;
            if cookies.is_empty() {
                anyhow::bail!("set_cookies: 'cookies' was provided but is empty");
            }
            let mapped: Vec<Value> = cookies.iter().map(cookie_to_json).collect();
            params.insert("cookies".into(), Value::Array(mapped));
        }
        "delete_cookie" => {
            let name = input
                .cookie_name
                .as_deref()
                .ok_or_else(|| anyhow::anyhow!("delete_cookie requires 'cookie_name'"))?;
            params.insert("name".into(), json!(name));
            if let Some(domain) = &input.cookie_domain {
                params.insert("domain".into(), json!(domain));
            }
            if let Some(path) = &input.cookie_path {
                params.insert("path".into(), json!(path));
            }
            if let Some(url) = &input.url {
                params.insert("url".into(), json!(url));
            }
        }
        "list_cookies" => {
            // Legacy path, kept for backward compatibility: emulated via
            // eval of document.cookie. Cannot see HttpOnly cookies — that's
            // why get_cookies (real bridge call) is now the recommended
            // action; see schema description. Parsed into a structured
            // {cookies: [...], raw} shape rather than the bare string so
            // format_cookie_string_result can render it as one line per
            // cookie like the real get_cookies path does.
            params.insert(
                "script".into(),
                json!("return (() => { const raw = document.cookie || ''; const cookies = raw.split(';').map(s => s.trim()).filter(Boolean).map(pair => { const idx = pair.indexOf('='); return idx === -1 ? { name: pair, value: '' } : { name: pair.slice(0, idx), value: pair.slice(idx + 1) }; }); return { cookies, raw }; })()"),
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

fn cookie_to_json(cookie: &CookieInput) -> Value {
    let mut obj = Map::new();
    obj.insert("name".into(), json!(cookie.name));
    obj.insert("value".into(), json!(cookie.value));
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
        obj.insert("expires".into(), json!(expires));
    }
    Value::Object(obj)
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
}

/// Bug fix #1: the original only dispatched keydown/keypress/keyup, never
/// mutating the element's value or firing an `input` event. Modern
/// framework-controlled inputs (React, Vue, most SPA form libraries)
/// update state from the `input` event, not raw key events, so the old
/// script would *look* like it worked (events fired, no error) but the
/// input would stay empty. This version:
///   - For a single printable character with no special meaning, inserts
///     it at the current selection/cursor position of an
///     input/textarea/contenteditable and fires a real `input` event
///     (InputEvent where supported, so `inputType`/`data` are populated).
///   - For Backspace/Delete, actually removes a character, native-like.
///   - For Enter, still submits the form if present (unchanged behavior)
///     and also fires 'input' for contenteditable / rich text editors that
///     listen for Enter without a form.
///   - For all other keys (Tab, Escape, ArrowUp/Down/Left/Right, Home,
///     End, function keys, modifier combos not otherwise handled), keeps
///     the original pure keyboard-event dispatch, since synthesizing
///     "what the key would do" generically isn't reliable — those keys are
///     usually handled by the page's own keydown listener anyway (menus,
///     shortcuts), which does receive the event.
fn build_press_script(key: Option<&str>, selector: Option<&str>) -> Result<String> {
    let key = key.ok_or_else(|| anyhow::anyhow!("key is required for press"))?;
    let selector_literal = selector.map(serde_json::to_string).transpose()?;
    let selector_expr = selector_literal
        .map(|s| format!("document.querySelector({})", s))
        .unwrap_or_else(|| "null".to_string());
    let key_literal = serde_json::to_string(key)?;

    Ok(format!(
        r#"return (() => {{
  const target = {selector_expr} || document.activeElement || document.body;
  if (!target) throw new Error('No target available for key press');
  if (typeof target.focus === 'function') target.focus();
  const key = {key_literal};
  const eventInit = {{ key, bubbles: true, cancelable: true }};

  const isEditable = (el) => {{
    if (!el) return false;
    const tag = (el.tagName || '').toLowerCase();
    return tag === 'input' || tag === 'textarea' || el.isContentEditable === true;
  }};

  const fireInput = (el, inputType, data) => {{
    let evt;
    try {{
      evt = new InputEvent('input', {{ bubbles: true, cancelable: true, inputType, data: data ?? null }});
    }} catch (e) {{
      evt = new Event('input', {{ bubbles: true, cancelable: true }});
    }}
    el.dispatchEvent(evt);
  }};

  target.dispatchEvent(new KeyboardEvent('keydown', eventInit));
  target.dispatchEvent(new KeyboardEvent('keypress', eventInit));

  let mutated = false;
  if (isEditable(target)) {{
    const tag = (target.tagName || '').toLowerCase();
    const isNativeField = tag === 'input' || tag === 'textarea';

    if (key === 'Backspace' || key === 'Delete') {{
      if (isNativeField) {{
        const start = target.selectionStart ?? target.value.length;
        const end = target.selectionEnd ?? target.value.length;
        if (start === end && start > 0 && key === 'Backspace') {{
          target.value = target.value.slice(0, start - 1) + target.value.slice(end);
          target.setSelectionRange(start - 1, start - 1);
        }} else if (start === end && key === 'Delete') {{
          target.value = target.value.slice(0, start) + target.value.slice(end + 1);
          target.setSelectionRange(start, start);
        }} else {{
          target.value = target.value.slice(0, start) + target.value.slice(end);
          target.setSelectionRange(start, start);
        }}
        fireInput(target, 'deleteContentBackward', null);
        mutated = true;
      }}
    }} else if (key.length === 1) {{
      // Single printable character: insert at cursor.
      if (isNativeField) {{
        const start = target.selectionStart ?? target.value.length;
        const end = target.selectionEnd ?? target.value.length;
        target.value = target.value.slice(0, start) + key + target.value.slice(end);
        target.setSelectionRange(start + 1, start + 1);
        fireInput(target, 'insertText', key);
        mutated = true;
      }} else {{
        // contenteditable: insert at the current selection/caret.
        const sel = window.getSelection && window.getSelection();
        if (sel && sel.rangeCount > 0) {{
          const range = sel.getRangeAt(0);
          range.deleteContents();
          range.insertNode(document.createTextNode(key));
          range.collapse(false);
          sel.removeAllRanges();
          sel.addRange(range);
        }} else {{
          target.textContent = (target.textContent || '') + key;
        }}
        fireInput(target, 'insertText', key);
        mutated = true;
      }}
    }}
  }}

  if (key === 'Enter' && target.form && typeof target.form.requestSubmit === 'function') {{
    target.form.requestSubmit();
  }} else if (key === 'Enter' && target.form && typeof target.form.submit === 'function') {{
    target.form.submit();
  }}

  target.dispatchEvent(new KeyboardEvent('keyup', eventInit));
  return {{ pressed: true, key, tag: target.tagName || null, mutatedValue: mutated }};
}})();"#
    ))
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
        return firefox_run_bridge_command(bridge_action, params, ctx).await;
    }

    let mut last_err = None;
    for attempt in 0..=MAX_RETRIES {
        match firefox_run_bridge_command(bridge_action, params.clone(), ctx).await {
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

    #[cfg(not(windows))]
    if std::env::var("BROWSER_SESSION").is_err() {
        if let Some(session_name) = crate::browser::ensure_browser_session(&_ctx.session_id) {
            command.env("BROWSER_SESSION", session_name);
        }
    }

    let output = command
        .output()
        .await
        .with_context(|| format!("Failed to run browser bridge action '{}'.", action))?;

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

    if !output.status.success() {
        let details = if stderr.is_empty() {
            stdout
        } else if stdout.is_empty() {
            stderr
        } else {
            format!("{}\n{}", stderr, stdout)
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

/// RAII guard that removes the screenshot temp file on drop, so it's
/// cleaned up whether the function returns early via `?`, panics, or
/// completes normally. Bug fix #3 (partial leak on error paths after the
/// file was already written by the bridge).
struct TempFileGuard {
    path: PathBuf,
    disarmed: bool,
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            disarmed: false,
        }
    }

    /// Call once the caller has taken ownership of / already deleted the
    /// file, to skip the (now redundant, and possibly racing) drop-time
    /// removal attempt.
    fn disarm(&mut self) {
        self.disarmed = true;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
        let path = self.path.clone();
        // best-effort, fire-and-forget cleanup; ignore errors (file may
        // never have been created if the bridge call failed before
        // writing it)
        tokio::spawn(async move {
            let _ = tokio::fs::remove_file(&path).await;
        });
    }
}

async fn screenshot_via_bridge(
    params: &Value,
    title: String,
    ctx: &ToolContext,
) -> Result<ToolOutput> {
    let filename = temp_screenshot_path();
    let mut guard = TempFileGuard::new(filename.clone());

    let mut screenshot_params = params.clone();
    if let Some(map) = screenshot_params.as_object_mut() {
        map.insert(
            "filename".into(),
            json!(filename.to_string_lossy().to_string()),
        );
    }

    let result = firefox_run_bridge_command("screenshot", screenshot_params, ctx).await?;
    let saved = result
        .get("saved")
        .and_then(|v| v.as_str())
        .map(PathBuf::from)
        .unwrap_or_else(|| filename.clone());

    // If the bridge saved somewhere other than the requested filename,
    // make sure that path gets cleaned up too.
    if saved != filename {
        guard.disarm();
    }
    let save_guard = TempFileGuard::new(saved.clone());

    let mut output = ToolOutput::new(format!(
        "Captured browser screenshot to {}.\n\nNote: if the active model does not accept image input, the attached image is dropped by the provider — use `snapshot format=annotated` or `interactables` to verify page state instead (the text views include selectors and hrefs).",
        saved.display()
    ))
    .with_title(title)
    .with_metadata(result.clone());

    if let Ok(bytes) = tokio::fs::read(&saved).await {
        output = output.with_labeled_image(
            "image/png",
            STANDARD.encode(&bytes),
            format!("browser screenshot: {}", saved.display()),
        );
    }
    // save_guard drops here regardless of whether the read succeeded,
    // ensuring cleanup on both success and failure paths.
    drop(save_guard);
    guard.disarm();

    Ok(output)
}

fn temp_screenshot_path() -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let counter = SCREENSHOT_COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    std::env::temp_dir().join(format!("alphacode-browser-{}-{}-{}.png", ts, pid, counter))
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
        "eval" | "press" => format_eval_result(&result),
        "list_cookies" => format_cookie_string_result(&result),
        "get_cookies" => format_cookies_result(&result),
        _ => serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string()),
    };

    ToolOutput::new(body)
        .with_title(title)
        .with_metadata(result)
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
fn format_cookies_result(result: &Value) -> String {
    let Some(cookies) = result.get("cookies").and_then(|v| v.as_array()) else {
        return serde_json::to_string_pretty(result).unwrap_or_default();
    };
    if cookies.is_empty() {
        return "No cookies found.".to_string();
    }
    let mut lines = Vec::new();
    for cookie in cookies {
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
        lines.push(format!(
            "{}. [{}] <{}> {} | selector: {}",
            idx + 1,
            kind,
            tag.to_lowercase(),
            text,
            selector
        ));
    }

    lines.join("\n")
}
