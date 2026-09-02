//! Progressive disclosure: return full specs for advanced actions on demand,
//! so the always-on tool schema stays small.

use crate::alphacode_tool_types::ToolOutput;
use anyhow::Result;

const MOUSE: &str = "\
mouse actions (visible coordinate input; moves the real cursor):
- move        {x, y}
- click       {x?, y?}            click at point (or current cursor position)
- double_click{x?, y?}
- right_click {x?, y?}
- drag        {x, y, to_x, to_y}
- scroll      {x?, y?, dx, dy}    dy>0 scrolls up
- cursor      {}                  report current cursor position";

const KEYBOARD: &str = "\
keyboard actions (go to the focused app):
- type     {text}                 type a UTF-8 string
- key      {keys}                 chord, e.g. 'cmd+space', 'return', 'ctrl+shift+t'
- key_down {keys}                 hold a key/chord down
- key_up   {keys}                 release a key/chord";

const OBSERVE: &str = "\
observe actions (see the screen):
- screenshot        {}                 full main display; reports point/pixel scale
- window_screenshot {window_id}        capture one window (even if occluded)
- ocr               {region?:[x,y,w,h]} recognize on-screen text + bounding boxes (Vision)
- ui                {app?, depth?}     dump the Accessibility tree; element paths shown as #a.b.c";

const AX: &str = "\
accessibility actions (BACKGROUND control; no cursor movement, app need not be frontmost).
Element handle = {app:\"AppName\", path:[child indices from the front window]} from find_element/ui:
- find_element   {app, role?, title?, value?, depth?}  -> matching elements with paths
- element_at     {app, x, y}                            -> deepest element at a point
- press          {element}                              -> AXPress (background click)
- set_value      {element, value}                       -> set a field's value (background type)
- get_value      {element}
- perform_action {element, ax_action}                   -> any AX action, e.g. 'AXShowMenu'
- select_menu    {app, menu_path:[\"File\",\"Save\"]}      -> drive the menu bar";

const WINDOWS: &str = "\
window actions (act on an app's front window; AX-based, can target background windows):
- list_windows   {}                    all on-screen windows with ids/owners/bounds
- focus_window   {app}                 raise + activate the app's front window
- move_window    {app, x, y}
- resize_window  {app, w, h}
- minimize_window{app}
- close_window   {app}";

const APPS: &str = "\
app actions:
- list_apps    {}            running (non-background) apps
- activate_app {app}         bring an app to the front
- hide_app     {app}         hide an app (no quit)
- quit_app     {app}         quit an app";

const CLIPBOARD: &str = "\
clipboard actions:
- get_clipboard {}
- set_clipboard {text}";

const SCRIPTING: &str = "\
scripting actions (headless control of scriptable apps; no UI, no cursor):
- run_applescript {script}   run AppleScript, returns its result
- run_jxa         {script}   run JavaScript-for-Automation
- wait_for        {app, contains, timeout_ms?}  poll an app's AX tree until text appears";

const SYSTEM: &str = "\
system actions:
- notify         {text, title?}   post a Notification Center banner
- system_state   {}               battery / date / power summary
- set_brightness {level}          0..1 (needs the `brightness` cli)";

const SETUP: &str = "\
setup actions:
- check_permissions {}   report Accessibility / Screen Recording / Swift status
- setup             {}   request permissions, deep-link to the right Settings panes, poll until ready

Note: the Accessibility toggle itself cannot be enabled programmatically (macOS security);
setup gets you one click away.";

fn section(cat: &str) -> Option<&'static str> {
    Some(match cat {
        "mouse" => MOUSE,
        "keyboard" => KEYBOARD,
        "observe" => OBSERVE,
        "ax" => AX,
        "windows" => WINDOWS,
        "apps" => APPS,
        "clipboard" => CLIPBOARD,
        "scripting" => SCRIPTING,
        "system" => SYSTEM,
        "setup" => SETUP,
        _ => return None,
    })
}

pub fn discover(category: Option<&str>) -> Result<ToolOutput> {
    let cat = category.unwrap_or("all");
    let body = if cat == "all" {
        [
            OBSERVE, MOUSE, KEYBOARD, AX, WINDOWS, APPS, CLIPBOARD, SCRIPTING, SYSTEM, SETUP,
        ]
        .join("\n\n")
    } else if let Some(s) = section(cat) {
        s.to_string()
    } else {
        format!(
            "Unknown category '{cat}'. Valid: mouse, keyboard, observe, ax, windows, apps, \
             clipboard, scripting, system, setup, all."
        )
    };
    Ok(ToolOutput::new(format!(
        "macos_computer_use actions — category '{cat}'. All actions are fields on the same `macos_computer_use` tool.\n\n{body}\n\n\
         Default policy: this is the user's own live machine, so stay out of their way. \
         Prefer background AX/scripting actions over visible coordinate input when the target \
         element is resolvable; only fall back to click/type (which move your cursor and steal \
         focus) when AX can't reach it. Do not move the cursor, change the frontmost app, or \
         move/resize/activate windows unless the user asked or the task strictly requires it. \
         Act only on the task you were given — never take proactive control of the desktop \
         upfront. When a visible action is unavoidable, do the minimum and restore focus."
    ))
    .with_title("discover"))
}
