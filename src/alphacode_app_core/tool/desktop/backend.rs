//! xa11y backend wrapper.
//!
//! All xa11y calls are isolated here so the rest of the desktop module never
//! imports `xa11y` directly — this keeps the abstraction boundary clean and
//! makes it easy to swap backends later.

use super::element::{ELEMENT_STORE, ElementInfo, ElementStore};
use anyhow::{Result, bail};
use base64::Engine as _;
use std::time::Duration;
use xa11y::AppExt as _;

// ---------------------------------------------------------------------------
// Application / window listing
// ---------------------------------------------------------------------------

/// Minimal application info for list_windows.
pub struct AppInfo {
    pub name: String,
    pub pid: Option<u32>,
    pub is_foreground: bool,
    pub windows: Vec<String>,
}

pub fn list_applications() -> Result<Vec<AppInfo>> {
    let apps = xa11y::App::list().map_err(map_xa11y_error)?;
    let mut result = Vec::with_capacity(apps.len());

    for app in &apps {
        let windows: Vec<String> = app
            .children()
            .unwrap_or_default()
            .iter()
            .filter_map(|child| child.name.clone())
            .collect();

        // Check if this app reports focused state via its data
        let is_fg = app.data.states.focused;

        result.push(AppInfo {
            name: app.name.clone(),
            pid: app.pid,
            is_foreground: is_fg,
            windows,
        });
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Snapshot
// ---------------------------------------------------------------------------

pub struct SnapshotResult {
    pub application_name: String,
    pub windows: Vec<WindowSnapshot>,
    pub element_count: usize,
}

pub struct WindowSnapshot {
    pub title: String,
    pub elements: Vec<ElementInfo>,
}

pub fn snapshot(
    application: Option<&str>,
    window: Option<&str>,
    depth: usize,
    interactive_only: bool,
    visible_only: bool,
) -> Result<SnapshotResult> {
    let app = resolve_application(application)?;
    let children = app.children().map_err(map_xa11y_error)?;
    // Record which application each element belongs to so `element_id` based
    // actions can re-resolve inside the same app later (wrong-app clicks were
    // the failure mode when the user switched applications between calls).
    let app_name = app.name.clone();

    let mut windows = Vec::new();
    let mut total_elements = 0;

    for child in &children {
        // Filter by window title if specified
        if let Some(win_filter) = window {
            if let Some(ref title) = child.name {
                if !title.to_lowercase().contains(&win_filter.to_lowercase()) {
                    continue;
                }
            } else {
                continue;
            }
        }

        // Skip non-window elements
        let role_str = format!("{:?}", child.role);
        if !role_str.contains("Window")
            && !role_str.contains("Dialog")
            && !role_str.contains("Frame")
        {
            continue;
        }

        let title = child
            .name
            .clone()
            .unwrap_or_else(|| "(untitled)".to_string());
        let elements = collect_elements(
            child,
            Some(app_name.clone()),
            depth,
            interactive_only,
            visible_only,
        )?;
        total_elements += elements.len();

        windows.push(WindowSnapshot { title, elements });
    }

    Ok(SnapshotResult {
        application_name: app.name.clone(),
        windows,
        element_count: total_elements,
    })
}

/// Recursively collect elements from an xa11y element tree.
fn collect_elements(
    element: &xa11y::Element,
    app_name: Option<String>,
    max_depth: usize,
    interactive_only: bool,
    visible_only: bool,
) -> Result<Vec<ElementInfo>> {
    let mut result = Vec::new();
    collect_elements_recursive(
        element,
        &app_name,
        0,
        max_depth,
        interactive_only,
        visible_only,
        &mut result,
    )?;
    Ok(result)
}

#[allow(clippy::too_many_arguments)]
fn collect_elements_recursive(
    element: &xa11y::Element,
    app_name: &Option<String>,
    current_depth: usize,
    max_depth: usize,
    interactive_only: bool,
    visible_only: bool,
    out: &mut Vec<ElementInfo>,
) -> Result<()> {
    if current_depth > max_depth {
        return Ok(());
    }

    // Filter by visibility
    if visible_only && !element.states.visible {
        return Ok(());
    }

    let is_interactive = is_interactive_role(element.role)
        || element.states.focusable
        || !element.actions.is_empty();

    if interactive_only && !is_interactive {
        // Still recurse into children in case they contain interactive elements
        if let Ok(children) = element.children() {
            for child in &children {
                collect_elements_recursive(
                    child,
                    app_name,
                    current_depth + 1,
                    max_depth,
                    interactive_only,
                    visible_only,
                    out,
                )?;
            }
        }
        return Ok(());
    }

    let store = ELEMENT_STORE.get_or_init(|| std::sync::Mutex::new(ElementStore::new()));
    let info = store
        .lock()
        .map_err(|_| anyhow::anyhow!("element store lock poisoned"))?
        .register(element, app_name.clone());
    out.push(info);

    // Recurse into children
    if let Ok(children) = element.children() {
        for child in &children {
            collect_elements_recursive(
                child,
                app_name,
                current_depth + 1,
                max_depth,
                interactive_only,
                visible_only,
                out,
            )?;
        }
    }

    Ok(())
}

fn is_interactive_role(role: xa11y::Role) -> bool {
    use xa11y::Role::*;
    matches!(
        role,
        Button
            | CheckBox
            | RadioButton
            | TextField
            | TextArea
            | ComboBox
            | Link
            | Tab
            | MenuItem
            | Menu
            | Slider
            | Switch
            | SpinButton
            | ListItem
            | TreeItem
            | ScrollThumb
    )
}

// ---------------------------------------------------------------------------
// Element find
// ---------------------------------------------------------------------------

pub fn find_elements(
    application: Option<&str>,
    window: Option<&str>,
    role_filter: Option<&str>,
    name_filter: Option<&str>,
    value_filter: Option<&str>,
) -> Result<Vec<ElementInfo>> {
    let app = resolve_application(application)?;
    let children = app.children().map_err(map_xa11y_error)?;
    // Record which application each element belongs to so `element_id` based
    // actions re-resolve inside the same application later.
    let app_name = Some(app.name.clone());

    let mut matches = Vec::new();

    for child in &children {
        // Filter by window
        if let Some(win_filter) = window {
            if let Some(ref title) = child.name {
                if !title.to_lowercase().contains(&win_filter.to_lowercase()) {
                    continue;
                }
            } else {
                continue;
            }
        }

        // Descend only into window-like containers — mirrors the snapshot
        // gate (Window | Dialog | Frame) plus Pane: Electron/Chromium apps
        // regularly expose their top level as Pane or Frame, and a
        // Window/Dialog-only gate silently yielded zero matches for them.
        let role_str = format!("{:?}", child.role);
        let window_like = ["Window", "Dialog", "Frame", "Pane"]
            .iter()
            .any(|role| role_str.contains(role));
        if !window_like {
            continue;
        }

        find_in_subtree(
            child,
            &app_name,
            role_filter,
            name_filter,
            value_filter,
            &mut matches,
        )?;
    }

    Ok(matches)
}

fn find_in_subtree(
    element: &xa11y::Element,
    app_name: &Option<String>,
    role_filter: Option<&str>,
    name_filter: Option<&str>,
    value_filter: Option<&str>,
    out: &mut Vec<ElementInfo>,
) -> Result<()> {
    if element_matches(element, role_filter, name_filter, value_filter) {
        let store = ELEMENT_STORE.get_or_init(|| std::sync::Mutex::new(ElementStore::new()));
        let info = store
            .lock()
            .map_err(|_| anyhow::anyhow!("element store lock poisoned"))?
            .register(element, app_name.clone());
        out.push(info);
    }

    if let Ok(children) = element.children() {
        for child in &children {
            find_in_subtree(child, app_name, role_filter, name_filter, value_filter, out)?;
        }
    }

    Ok(())
}

fn element_matches(
    element: &xa11y::Element,
    role_filter: Option<&str>,
    name_filter: Option<&str>,
    value_filter: Option<&str>,
) -> bool {
    if let Some(role_str) = role_filter {
        let element_role = format!("{:?}", element.role);
        if !element_role
            .to_lowercase()
            .contains(&role_str.to_lowercase())
        {
            return false;
        }
    }

    if let Some(name_str) = name_filter {
        match &element.name {
            Some(name) => {
                if !name.to_lowercase().contains(&name_str.to_lowercase()) {
                    return false;
                }
            }
            None => return false,
        }
    }

    if let Some(value_str) = value_filter {
        match &element.value {
            Some(value) => {
                if !value.to_lowercase().contains(&value_str.to_lowercase()) {
                    return false;
                }
            }
            None => return false,
        }
    }

    true
}

// ---------------------------------------------------------------------------
// Element info by ID
// ---------------------------------------------------------------------------

pub fn get_element_info(element_id: &str) -> Result<ElementInfo> {
    let store = ELEMENT_STORE.get_or_init(|| std::sync::Mutex::new(ElementStore::new()));
    let store = store
        .lock()
        .map_err(|_| anyhow::anyhow!("element store lock poisoned"))?;
    store
        .get(element_id)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!(
            "ElementNotFound: element_id '{element_id}' not found or stale. Run desktop action='find' to get a fresh element reference."
        ))
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

pub fn click_element(element_id: &str) -> Result<String> {
    let element = resolve_element(element_id)?;
    element.press().map_err(map_xa11y_error)?;
    let role = &element.role;
    Ok(format!(
        "Clicked element '{}' [{role:?}].",
        element_name(&element)
    ))
}

pub fn click_coordinates(x: f64, y: f64) -> Result<String> {
    let sim = xa11y::input_sim().map_err(map_xa11y_error)?;
    // Round, don't truncate: truncation systematically off-targets clicks on
    // DPI-scaled displays (physical-pixel bounds from screenshots/UIA).
    let point = xa11y::Point {
        x: x.round() as i32,
        y: y.round() as i32,
    };
    sim.mouse().click(point).map_err(map_xa11y_error)?;
    Ok(format!("Clicked at ({x:.0}, {y:.0})."))
}

pub fn focus_element(element_id: &str) -> Result<String> {
    let element = resolve_element(element_id)?;
    element.focus().map_err(map_xa11y_error)?;
    let role = &element.role;
    Ok(format!(
        "Focused element '{}' [{role:?}].",
        element_name(&element)
    ))
}

pub fn focus_application(app_name: &str) -> Result<String> {
    let app = resolve_application(Some(app_name))?;
    app.as_element().focus().map_err(map_xa11y_error)?;
    Ok(format!("Focused application '{}'.", app.name))
}

pub fn type_text_element(element_id: &str, text: &str) -> Result<String> {
    let element = resolve_element(element_id)?;
    element.type_text(text).map_err(map_xa11y_error)?;
    Ok(format!(
        "Typed {} characters into element '{}'.",
        text.chars().count(),
        element_name(&element)
    ))
}

pub fn type_text_focused(text: &str) -> Result<String> {
    let sim = xa11y::input_sim().map_err(map_xa11y_error)?;
    sim.keyboard().type_text(text).map_err(map_xa11y_error)?;
    Ok(format!(
        "Typed {} characters into focused element.",
        text.chars().count()
    ))
}

pub fn press_key(keys: &str) -> Result<String> {
    let sim = xa11y::input_sim().map_err(map_xa11y_error)?;
    let parsed_keys = parse_key_chord(keys)?;
    let keyboard = sim.keyboard();
    if parsed_keys.len() == 1 {
        keyboard
            .press(parsed_keys[0].clone())
            .map_err(map_xa11y_error)?;
    } else if let Some((main, held)) = parsed_keys.split_last() {
        keyboard
            .chord(main.clone(), held)
            .map_err(map_xa11y_error)?;
    }
    Ok(format!("Pressed '{keys}'."))
}

pub fn scroll_element(element_id: &str, _dx: i32, dy: i32) -> Result<String> {
    let element = resolve_element(element_id)?;
    let point = element
        .bounds
        .as_ref()
        .map(|b| xa11y::Point {
            x: b.x + (b.width as i32) / 2,
            y: b.y + (b.height as i32) / 2,
        })
        .ok_or_else(|| anyhow::anyhow!("Element has no bounds for scrolling"))?;

    let sim = xa11y::input_sim().map_err(map_xa11y_error)?;
    let delta = xa11y::ScrollDelta::new(_dx, dy);
    sim.mouse().scroll(point, delta).map_err(map_xa11y_error)?;
    Ok(format!(
        "Scrolled element '{}' dy={dy}.",
        element_name(&element)
    ))
}

pub fn scroll_coordinates(x: f64, y: f64, dx: i32, dy: i32) -> Result<String> {
    let sim = xa11y::input_sim().map_err(map_xa11y_error)?;
    let point = xa11y::Point {
        x: x.round() as i32,
        y: y.round() as i32,
    };
    let delta = xa11y::ScrollDelta::new(dx, dy);
    sim.mouse().scroll(point, delta).map_err(map_xa11y_error)?;
    Ok(format!("Scrolled at ({x:.0}, {y:.0}) dx={dx} dy={dy}."))
}

pub fn scroll_at_pointer(dx: i32, dy: i32) -> Result<String> {
    let sim = xa11y::input_sim().map_err(map_xa11y_error)?;
    let delta = xa11y::ScrollDelta::new(dx, dy);
    let point = xa11y::Point { x: 0, y: 0 };
    sim.mouse().scroll(point, delta).map_err(map_xa11y_error)?;
    Ok(format!("Scrolled dx={dx} dy={dy}."))
}
pub fn toggle_element(element_id: &str) -> Result<String> {
    let element = resolve_element(element_id)?;
    element.toggle().map_err(map_xa11y_error)?;
    let role = &element.role;
    Ok(format!(
        "Toggled element '{}' [{role:?}].",
        element_name(&element)
    ))
}
pub fn expand_element(element_id: &str) -> Result<String> {
    let element = resolve_element(element_id)?;
    element.expand().map_err(map_xa11y_error)?;
    let role = &element.role;
    Ok(format!(
        "Expanded element '{}' [{role:?}].",
        element_name(&element)
    ))
}
pub fn collapse_element(element_id: &str) -> Result<String> {
    let element = resolve_element(element_id)?;
    element.collapse().map_err(map_xa11y_error)?;
    let role = &element.role;
    Ok(format!(
        "Collapsed element '{}' [{role:?}].",
        element_name(&element)
    ))
}

// ---------------------------------------------------------------------------
// Screenshot
// ---------------------------------------------------------------------------

pub struct ScreenshotResult {
    pub text: String,
    pub image_base64: String,
}

pub fn screenshot(application: Option<&str>) -> Result<ScreenshotResult> {
    if let Some(app_name) = application {
        let app = resolve_application(Some(app_name))?;
        let children = app.children().map_err(map_xa11y_error)?;
        let first_window = children.iter().find(|c| {
            let role = format!("{:?}", c.role);
            role.contains("Window") || role.contains("Dialog")
        });

        if let Some(window) = first_window {
            let img = xa11y::screenshot_element(window).map_err(map_xa11y_error)?;
            let png_bytes = rgba_to_png(&img.pixels, img.width, img.height)?;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
            return Ok(ScreenshotResult {
                text: format!(
                    "Screenshot of '{}' captured ({}x{}).",
                    app_name, img.width, img.height
                ),
                image_base64: b64,
            });
        }
    }

    // Full screen capture
    let img = xa11y::screenshot().map_err(map_xa11y_error)?;
    let png_bytes = rgba_to_png(&img.pixels, img.width, img.height)?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    Ok(ScreenshotResult {
        text: format!(
            "Full screen screenshot captured ({}x{}).",
            img.width, img.height
        ),
        image_base64: b64,
    })
}

/// Convert raw RGBA8 pixels to PNG bytes.
fn rgba_to_png(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
    let img = image::RgbaImage::from_raw(width, height, pixels.to_vec())
        .ok_or_else(|| anyhow::anyhow!("Failed to create RGBA image from screenshot pixels"))?;
    let mut buf = std::io::Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| anyhow::anyhow!("PNG encoding failed: {e}"))?;
    Ok(buf.into_inner())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Resolve an application by name, or the foreground app when no name is
/// given.
///
/// On Windows the accessibility layer exposes one top-level element per
/// process whose *name is the mutable window title* (`list_apps` yields one
/// top-level Window element per PID). An exact `App::by_name("Firefox")`
/// therefore misses the very application the agent just saw in
/// `list_windows` output (`<page> — Mozilla Firefox`). Preferred order:
/// exact platform match (fast path) → case-insensitive → substring, with
/// multi-process ambiguity reported as an error listing the candidates.
fn resolve_application(name: Option<&str>) -> Result<xa11y::App> {
    match name {
        None => xa11y::App::find(Duration::from_secs(1), |data| data.states.focused)
            .map_err(map_xa11y_error),
        Some(app_name) if app_name.trim().is_empty() => bail!(
            "application name is empty; pass a window-title fragment (e.g. 'Firefox') or omit \
             `application` to target the foreground app"
        ),
        Some(app_name) => resolve_application_by_name(app_name),
    }
}

/// Case-insensitive, substring-tolerant application lookup. The ranked
/// matching logic lives in [`match_app_candidates`] so it stays unit-testable
/// without a live accessibility tree.
fn resolve_application_by_name(query: &str) -> Result<xa11y::App> {
    // Fast path: the platform's own exact match.
    if let Ok(app) = xa11y::App::by_name(query, Duration::from_secs(3)) {
        return Ok(app);
    }
    let provider = xa11y::provider().map_err(map_xa11y_error)?;
    let mut apps = xa11y::App::list_with(provider).map_err(map_xa11y_error)?;
    let names: Vec<String> = apps.iter().map(|app| app.name.trim().to_string()).collect();

    let ranked = match_app_candidates(&names, query);
    let Some(first) = ranked.first().copied() else {
        return Err(anyhow::anyhow!(
            "ElementNotFound: no running application matches '{query}' (exact or substring).{}",
            running_apps_hint(&names)
        ));
    };

    // Windows reports one entry per top-level window, so several candidates
    // can belong to the same process — only genuinely different processes are
    // ambiguous.
    let mut distinct_pids: Vec<Option<u32>> = Vec::new();
    for &idx in &ranked {
        if !distinct_pids.contains(&apps[idx].pid) {
            distinct_pids.push(apps[idx].pid);
        }
    }
    if distinct_pids.len() > 1 {
        let candidates = ranked
            .iter()
            .filter(|&&idx| !names[idx].is_empty())
            .map(|&idx| format!("  - pid={:?} name='{}'", apps[idx].pid, names[idx]))
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "ElementNotFound: '{query}' matches {count} applications from different processes; \
             be more specific (full window title or a longer fragment):\n{candidates}",
            count = candidates.lines().count()
        );
    }

    Ok(apps.swap_remove(first))
}

/// Rank app names for a fuzzy query: case-insensitive exact matches win
/// outright (when a process's window title is exactly what was asked for,
/// substring candidates are noise and would only manufacture ambiguity);
/// otherwise substring matches in either direction (`query ⊂ name` covers
/// 'Firefox' → '<page> — Mozilla Firefox'; `name ⊂ query` covers an agent
/// pasting yesterday's full title after the page shortened it). Returns
/// indices into `names`; empty/blank names never match.
fn match_app_candidates(names: &[String], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    let mut exact = Vec::new();
    let mut contains = Vec::new();
    for (idx, name) in names.iter().enumerate() {
        let lower = name.trim().to_lowercase();
        if lower.is_empty() {
            continue;
        }
        if lower == query {
            exact.push(idx);
        } else if lower.contains(&query) || query.contains(&lower) {
            contains.push(idx);
        }
    }
    if !exact.is_empty() {
        return exact;
    }
    contains
}

/// "These are the apps that are running" appendix for not-found errors — turn
/// a dead end into a glanceable correction.
fn running_apps_hint(names: &[String]) -> String {
    let mut listed: Vec<&str> = Vec::new();
    for name in names {
        let trimmed = name.trim();
        if trimmed.is_empty() || listed.iter().any(|seen| seen.eq_ignore_ascii_case(trimmed)) {
            continue;
        }
        listed.push(trimmed);
        if listed.len() >= 8 {
            break;
        }
    }
    if listed.is_empty() {
        return " No applications are visible to the accessibility layer right now; run desktop \
                action='list_windows' to check."
            .to_string();
    }
    format!(
        " Applications currently visible: {}.",
        listed
            .iter()
            .map(|name| format!("'{name}'"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

/// Resolve an element by its ephemeral ID.
fn resolve_element(element_id: &str) -> Result<xa11y::Element> {
    let info = get_element_info(element_id)?;

    // Try to find the element via the application. Tolerant resolution
    // matters here too: the stored app name is the window title, which may
    // have changed since registration.
    let app = match info.app_name.as_deref() {
        Some(name) if !name.trim().is_empty() => resolve_application(Some(name))?,
        _ => xa11y::App::find(Duration::from_secs(1), |data| data.states.focused)
            .map_err(map_xa11y_error)?,
    };

    // Build a selector from the element info
    let selector = build_selector(&info);

    // Try the locator approach
    let locator = app.locator(&selector);
    match locator.element() {
        Ok(el) => Ok(el),
        Err(xa11y::Error::SelectorNotMatched { .. }) => {
            bail!(
                "StaleElement: element_id '{element_id}' matched '{}' but the selector '{}' \
                 no longer resolves. The UI may have changed. Re-run desktop action='find' to get a fresh reference.",
                info.name.as_deref().unwrap_or("(unnamed)"),
                selector
            );
        }
        Err(e) => Err(map_xa11y_error(e)),
    }
}

/// Build an xa11y selector string from ElementInfo.
fn build_selector(info: &ElementInfo) -> String {
    let role_snake = info.role.to_lowercase();
    if let Some(ref name) = info.name {
        let safe_name = name.replace('"', "\\\"");
        format!("{role_snake}[name=\"{safe_name}\"]")
    } else {
        role_snake
    }
}

/// Parse an app-level selector — `application[name="X"]` / `window[name="X"]`
/// (optionally with whitespace and single quotes) — into the app name.
///
/// This is the exact shape xa11y's own not-found diagnostics print
/// (`application[name="<name>"]`), so agents copy it back into the tool
/// verbatim. The selector grammar has no escape for quotes inside attribute
/// values (a11y-core rejects such names), so the value is taken literally.
pub(crate) fn parse_application_selector(selector: &str) -> Option<String> {
    let trimmed = selector.trim();
    let lower = trimmed.to_ascii_lowercase();
    for prefix in ["application[", "window["] {
        if !lower.starts_with(prefix) {
            continue;
        }
        let rest = trimmed[prefix.len()..].trim_start();
        let rest = rest.strip_prefix("name")?.trim_start().strip_prefix('=')?;
        let attr = rest.trim_start();
        let quote = *attr.as_bytes().first()?;
        if quote != b'"' && quote != b'\'' {
            return None;
        }
        let value = attr.get(1..)?;
        let end = value.find(quote as char)?;
        let name = value[..end].trim();
        if name.is_empty() {
            return None;
        }
        return Some(name.to_string());
    }
    None
}

/// Locate an element from an xa11y selector and register it so follow-up
/// actions can use the returned `element_id`.
///
/// App-level selectors resolve the application tolerantly (exact →
/// case-insensitive → substring over the running apps) and hand out its
/// top-level element; other selectors resolve inside the focused
/// application. App-level resolution bypasses `App::locator`, which only
/// matches *descendants* of the app node — a locator search for
/// `application[...]` could therefore never return the app itself.
pub fn register_selector(selector: &str) -> Result<ElementInfo> {
    let (element, owning_app) = match parse_application_selector(selector) {
        Some(name) => {
            let app = resolve_application(Some(&name))?;
            (app.as_element(), app.name)
        }
        None => {
            let app = resolve_application(None)?;
            let element = app.locator(selector).element().map_err(map_xa11y_error)?;
            (element, app.name)
        }
    };

    let store = ELEMENT_STORE.get_or_init(|| std::sync::Mutex::new(ElementStore::new()));
    let info = store
        .lock()
        .map_err(|_| anyhow::anyhow!("element store lock poisoned"))?
        .register(&element, Some(owning_app));
    Ok(info)
}

/// Click via an xa11y selector.
///
/// App-level targets (`application[name="..."]`) are activated rather than
/// pressed: Window/Application role elements usually have no `press` action,
/// so the flow is focus → mouse click on the window's center (skipped when
/// bounds are unknown). Other selectors resolve inside the focused
/// application and are pressed like stored elements.
pub fn click_selector(selector: &str) -> Result<String> {
    let info = register_selector(selector)?;
    let element = resolve_element(&info.element_id)?;

    if element.press().is_ok() {
        return Ok(format!(
            "Clicked element '{}' from selector '{selector}'.",
            element_name(&element)
        ));
    }

    element.focus().map_err(map_xa11y_error)?;
    let center = element.data().bounds.map(|b| xa11y::Point {
        x: b.x + (b.width as i32) / 2,
        y: b.y + (b.height as i32) / 2,
    });
    if let Some(point) = center {
        let sim = xa11y::input_sim().map_err(map_xa11y_error)?;
        sim.mouse().click(point).map_err(map_xa11y_error)?;
    }
    Ok(format!(
        "Activated '{}' from selector '{selector}' (focus{}).",
        element_name(&element),
        if center.is_some() { " + click" } else { "" }
    ))
}

fn element_name(element: &xa11y::Element) -> String {
    element
        .name
        .clone()
        .unwrap_or_else(|| "(unnamed)".to_string())
}

/// Map xa11y errors to anyhow errors with useful diagnostics.
fn map_xa11y_error(err: xa11y::Error) -> anyhow::Error {
    match err {
        xa11y::Error::PermissionDenied { instructions } => {
            anyhow::anyhow!("PermissionDenied: {instructions}")
        }
        xa11y::Error::AccessibilityNotEnabled { app, instructions } => {
            anyhow::anyhow!("AccessibilityNotEnabled for '{app}': {instructions}")
        }
        xa11y::Error::SelectorNotMatched {
            selector,
            diagnosis,
        } => {
            let diag = diagnosis
                .map(|d| format!("\nDiagnosis: {d:?}"))
                .unwrap_or_default();
            anyhow::anyhow!(
                "ElementNotFound: selector '{selector}' did not match any element.{diag}"
            )
        }
        xa11y::Error::ElementStale { selector } => {
            anyhow::anyhow!(
                "StaleElement: selector '{selector}' targeted a stale node. Re-run desktop action='find'."
            )
        }
        xa11y::Error::Timeout { elapsed, diagnosis } => {
            let diag = diagnosis
                .map(|d| format!("\nDiagnosis: {d:?}"))
                .unwrap_or_default();
            anyhow::anyhow!("Timeout after {:.1}s.{diag}", elapsed.as_secs_f64())
        }
        xa11y::Error::ActionNotSupported { action, role } => {
            anyhow::anyhow!("UnsupportedAction: '{action}' not supported on role '{role:?}'.")
        }
        xa11y::Error::Unsupported { feature } => {
            anyhow::anyhow!("Unsupported: {feature} is not available on this platform.")
        }
        xa11y::Error::Platform { code, message } => {
            anyhow::anyhow!("PlatformError (code {code}): {message}")
        }
        other => anyhow::anyhow!("xa11y error: {other}"),
    }
}

/// Parse a key chord string like "ctrl+c", "alt+tab", "enter" into xa11y Keys.
fn parse_key_chord(chord: &str) -> Result<Vec<xa11y::Key>> {
    let parts: Vec<&str> = chord.split('+').map(|s| s.trim()).collect();
    let mut keys = Vec::with_capacity(parts.len());

    for part in &parts {
        let key = match part.to_lowercase().as_str() {
            "ctrl" | "control" => xa11y::Key::Ctrl,
            "alt" | "option" => xa11y::Key::Alt,
            "shift" => xa11y::Key::Shift,
            "meta" | "cmd" | "command" | "super" | "win" | "logo" => xa11y::Key::Meta,
            "enter" | "return" => xa11y::Key::Enter,
            "tab" => xa11y::Key::Tab,
            "escape" | "esc" => xa11y::Key::Escape,
            "backspace" => xa11y::Key::Backspace,
            "delete" | "del" => xa11y::Key::Delete,
            "insert" => xa11y::Key::Insert,
            "up" => xa11y::Key::ArrowUp,
            "down" => xa11y::Key::ArrowDown,
            "left" => xa11y::Key::ArrowLeft,
            "right" => xa11y::Key::ArrowRight,
            "home" => xa11y::Key::Home,
            "end" => xa11y::Key::End,
            "pageup" | "page_up" => xa11y::Key::PageUp,
            "pagedown" | "page_down" => xa11y::Key::PageDown,
            "space" => xa11y::Key::Space,
            "f1" => xa11y::Key::F(1),
            "f2" => xa11y::Key::F(2),
            "f3" => xa11y::Key::F(3),
            "f4" => xa11y::Key::F(4),
            "f5" => xa11y::Key::F(5),
            "f6" => xa11y::Key::F(6),
            "f7" => xa11y::Key::F(7),
            "f8" => xa11y::Key::F(8),
            "f9" => xa11y::Key::F(9),
            "f10" => xa11y::Key::F(10),
            "f11" => xa11y::Key::F(11),
            "f12" => xa11y::Key::F(12),
            s if s.len() == 1 => xa11y::Key::Char(s.chars().next().unwrap()),
            other => bail!(
                "Unknown key: '{other}'. Valid keys: ctrl, alt, shift, meta, enter, tab, escape, backspace, delete, arrows, f1-f12, or a single character."
            ),
        };
        keys.push(key);
    }

    if keys.is_empty() {
        bail!("Empty key chord");
    }

    Ok(keys)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_key() {
        let keys = parse_key_chord("enter").unwrap();
        assert_eq!(keys.len(), 1);
    }

    #[test]
    fn parse_modifier_chord() {
        let keys = parse_key_chord("ctrl+c").unwrap();
        assert_eq!(keys.len(), 2);
    }

    #[test]
    fn parse_invalid_key() {
        let result = parse_key_chord("notakey");
        assert!(result.is_err());
    }

    #[test]
    fn build_selector_with_name() {
        let info = ElementInfo {
            element_id: "test".to_string(),
            role: "Button".to_string(),
            name: Some("Save".to_string()),
            value: None,
            description: None,
            bounds: None,
            states: "enabled".to_string(),
            actions: vec![],
            app_name: None,
        };
        let sel = build_selector(&info);
        assert_eq!(sel, "button[name=\"Save\"]");
    }

    #[test]
    fn build_selector_without_name() {
        let info = ElementInfo {
            element_id: "test".to_string(),
            role: "TextField".to_string(),
            name: None,
            value: None,
            description: None,
            bounds: None,
            states: "enabled".to_string(),
            actions: vec![],
            app_name: None,
        };
        let sel = build_selector(&info);
        assert_eq!(sel, "textfield");
    }

    #[test]
    fn is_interactive_detects_buttons() {
        assert!(is_interactive_role(xa11y::Role::Button));
        assert!(is_interactive_role(xa11y::Role::TextField));
        assert!(!is_interactive_role(xa11y::Role::StaticText));
    }

    #[test]
    fn app_matching_prefers_exact_then_substring() {
        let names = vec![
            "Mozilla Firefox".to_string(),
            "Notes — Slack | Mozilla Firefox".to_string(),
            "Settings".to_string(),
            String::new(),
            "Firefox".to_string(),
        ];

        // A case-insensitive exact match short-circuits: substring candidates
        // are not considered alongside it.
        let ranked = match_app_candidates(&names, "firefox");
        let ranked_names: Vec<&str> = ranked.iter().map(|&i| names[i].as_str()).collect();
        assert_eq!(ranked_names, vec!["Firefox"]);

        // Same, regardless of the query's own casing.
        assert_eq!(match_app_candidates(&names, "FIREFOX"), vec![4]);

        // Substring fallback in both directions when nothing is exact.
        assert_eq!(match_app_candidates(&names, "slack"), vec![1]);

        // Query longer than the current titles (agent pasted a full title
        // from earlier) still matches via the reverse containment: both
        // "Mozilla Firefox" and "Firefox" are fragments of the query.
        assert_eq!(
            match_app_candidates(&names, "Mozilla Firefox Nightly"),
            vec![0, 4]
        );

        // Blank queries and empty app names never participate.
        assert!(match_app_candidates(&names, "   ").is_empty());
        let one_empty = vec![String::new()];
        assert!(match_app_candidates(&one_empty, "x").is_empty());
    }

    #[test]
    fn application_selector_parsing() {
        // The exact shape xa11y prints in its not-found diagnosis.
        assert_eq!(
            parse_application_selector(r#"application[name="Firefox"]"#),
            Some("Firefox".to_string())
        );
        // Whitespace, single quotes, and the window alias are tolerated.
        assert_eq!(
            parse_application_selector(r#" window[ name = 'Notepad' ] "#),
            Some("Notepad".to_string())
        );
        // Non-app selectors are rejected so they resolve inside the focused
        // application instead.
        assert_eq!(parse_application_selector(r#"button[name="Save"]"#), None);
        assert_eq!(parse_application_selector("textfield"), None);
        // Empty and unterminated values are rejected.
        assert_eq!(parse_application_selector(r#"application[name=""]"#), None);
        assert_eq!(parse_application_selector(r#"application[name="X"#), None);
    }
}
