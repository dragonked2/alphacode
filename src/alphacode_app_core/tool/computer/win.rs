//! Tier 2: application and window management via System Events / NSWorkspace.

use super::osa;
use crate::alphacode_tool_types::ToolOutput;
use anyhow::Result;

pub fn list_apps() -> Result<ToolOutput> {
    let script = "tell application \"System Events\" to get name of every application process whose background only is false";
    let res = osa::run_applescript(script)?;
    let apps: Vec<String> = res.split(", ").map(|s| s.trim().to_string()).collect();
    Ok(ToolOutput::new(format!(
        "Running apps ({}):\n{}",
        apps.len(),
        apps.join("\n")
    ))
    .with_title("list_apps"))
}

pub fn activate_app(app: &str) -> Result<ToolOutput> {
    osa::run_applescript(&format!(
        "tell application {} to activate",
        osa::as_quote(app)
    ))?;
    Ok(ToolOutput::new(format!("activated {app}")))
}

pub fn hide_app(app: &str) -> Result<ToolOutput> {
    osa::run_applescript(&format!(
        "tell application \"System Events\" to set visible of (first process whose name is {}) to false",
        osa::as_quote(app)
    ))?;
    Ok(ToolOutput::new(format!("hid {app}")))
}

pub fn quit_app(app: &str) -> Result<ToolOutput> {
    // `quit` blocks if the app shows a modal (e.g. an unsaved-changes sheet), so
    // use a short timeout and report that case instead of hanging the agent.
    match osa::run_applescript_timeout(
        &format!("tell application {} to quit", osa::as_quote(app)),
        std::time::Duration::from_secs(8),
    ) {
        Ok(_) => Ok(ToolOutput::new(format!("quit {app}"))),
        Err(e) if e.to_string().contains("timed out") => Ok(ToolOutput::new(format!(
            "{app} did not quit within 8s — it is likely showing a dialog (e.g. unsaved changes). \
             Handle the dialog (screenshot + click, or press a key), then quit again."
        ))),
        Err(e) => Err(e),
    }
}

/// List on-screen windows with CG window ids, owners, titles, and bounds.
pub fn list_windows() -> Result<ToolOutput> {
    // JXA can read the CG window list with ids reliably.
    let script = r#"
ObjC.import('CoreGraphics');
ObjC.import('Foundation');
var opts = $.kCGWindowListOptionOnScreenOnly | $.kCGWindowListExcludeDesktopElements;
var arr = $.CGWindowListCopyWindowInfo(opts, $.kCGNullWindowID);
var n = $.CFArrayGetCount(arr);
var out = [];
for (var i = 0; i < n; i++) {
  var d = $.CFArrayGetValueAtIndex(arr, i);
  var dict = ObjC.castRefToObject(d);
  var id = dict.objectForKey($('kCGWindowNumber'));
  var owner = dict.objectForKey($('kCGWindowOwnerName'));
  var name = dict.objectForKey($('kCGWindowName'));
  var b = dict.objectForKey($('kCGWindowBounds'));
  var bx = ObjC.deepUnwrap(b) || {};
  var ownerS = owner ? ObjC.unwrap(owner) : '';
  var nameS = name ? ObjC.unwrap(name) : '';
  var idN = id ? ObjC.unwrap(id) : '';
  out.push(idN + '\t' + ownerS + '\t' + (nameS||'') + '\t@(' + (bx.X|0) + ',' + (bx.Y|0) + ' ' + (bx.Width|0) + 'x' + (bx.Height|0) + ')');
}
out.join('\n');
"#;
    let res = osa::run_jxa(script)?;
    Ok(ToolOutput::new(format!(
        "Windows (id  owner  title  bounds):\n{}",
        if res.trim().is_empty() {
            "(none)"
        } else {
            &res
        }
    ))
    .with_title("list_windows"))
}

/// Window ops that target a window of an app by its (1-based) index or title.
/// We address via System Events AX windows of the owning process.
pub fn focus_window(app: &str) -> Result<ToolOutput> {
    osa::run_applescript(&format!(
        "tell application \"System Events\" to perform action \"AXRaise\" of (front window of (first process whose name is {}))",
        osa::as_quote(app)
    ))?;
    // also bring app forward
    let _ = activate_app(app);
    Ok(ToolOutput::new(format!("focused front window of {app}")))
}

pub fn move_window(app: &str, x: f64, y: f64) -> Result<ToolOutput> {
    osa::run_applescript(&format!(
        "tell application \"System Events\" to set position of front window of (first process whose name is {}) to {{{x}, {y}}}",
        osa::as_quote(app),
        x = x as i64,
        y = y as i64
    ))?;
    Ok(ToolOutput::new(format!(
        "moved {app} front window to ({x:.0},{y:.0})"
    )))
}

pub fn resize_window(app: &str, w: f64, h: f64) -> Result<ToolOutput> {
    osa::run_applescript(&format!(
        "tell application \"System Events\" to set size of front window of (first process whose name is {}) to {{{w}, {h}}}",
        osa::as_quote(app),
        w = w as i64,
        h = h as i64
    ))?;
    Ok(ToolOutput::new(format!(
        "resized {app} front window to {w:.0}x{h:.0}"
    )))
}

pub fn minimize_window(app: &str) -> Result<ToolOutput> {
    osa::run_applescript(&format!(
        "tell application \"System Events\" to set value of attribute \"AXMinimized\" of front window of (first process whose name is {}) to true",
        osa::as_quote(app)
    ))?;
    Ok(ToolOutput::new(format!("minimized {app} front window")))
}

pub fn close_window(app: &str) -> Result<ToolOutput> {
    osa::run_applescript(&format!(
        "tell application \"System Events\" to perform action \"AXPress\" of (button 1 of front window of (first process whose name is {}))",
        osa::as_quote(app)
    ))?;
    Ok(ToolOutput::new(format!("closed {app} front window")))
}
