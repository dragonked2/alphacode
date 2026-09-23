//! Render and install global "launch a new alphacode" hotkeys on Linux/niri.
//!
//! Unlike macOS, a Wayland client cannot grab system-wide hotkeys: the
//! `global-hotkey` crate only works on X11/macOS. The portable, correct
//! mechanism on Wayland is to ask the **compositor** to bind the key, so on niri
//! we generate `bind` lines and splice them into the user's
//! `~/.config/niri/config.kdl` `binds { }` block. niri watches its config and
//! hot-reloads on save, so the bindings take effect without a restart.
//!
//! Two layers, mirroring the macOS module:
//!
//! * The **pure** renderer ([`render_niri_block`], [`chord_to_niri_bind`]) turns
//!   resolved launch hotkeys into the exact KDL text we manage. This is what the
//!   unit tests assert, so the bindings the user sees are exactly what we can
//!   check without touching their machine.
//! * The **install** glue ([`splice_managed_block`]) replaces our marked region
//!   inside the existing `binds { }` block (or inserts one), leaving every other
//!   line untouched.
//!
//! The managed region is delimited by sentinel comments so re-installs are
//! idempotent and a user can hand-remove it cleanly:
//!
//! ```text
//!     // >>> alphacode launch hotkeys (managed) >>>
//!     Alt+Semicolon hotkey-overlay-title="alphacode: home" { spawn "sh" "-c" "..."; }
//!     // <<< alphacode launch hotkeys (managed) <<<
//! ```

use crate::alphacode_setup_hints::keymap::KeyChord;

/// Opening sentinel for the managed bind region inside `binds { }`.
pub(crate) const NIRI_BLOCK_BEGIN: &str = "// >>> alphacode launch hotkeys (managed) >>>";
/// Closing sentinel for the managed bind region inside `binds { }`.
pub(crate) const NIRI_BLOCK_END: &str = "// <<< alphacode launch hotkeys (managed) <<<";

/// One resolved hotkey ready to render as a niri bind: the chord, the target
/// directory, a human label, and whether it is a self-dev session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NiriHotkey {
    pub chord: KeyChord,
    /// Concrete directory to launch alphacode in (already resolved from any
    /// `$HOME`/`$LAST_DIR` sentinel).
    pub dir: String,
    /// Short human label, e.g. the repo's directory name.
    pub label: String,
    /// Pass the `self-dev` subcommand.
    pub self_dev: bool,
}

/// Map a alphacode modifier+key chord onto niri's KDL key syntax.
///
/// niri uses `+`-joined modifiers followed by an XKB key name, e.g.
/// `Alt+Semicolon`, `Super+Shift+Apostrophe`. We translate alphacode's `cmd`
/// modifier to `Super` (the Wayland super/meta key) since there is no Command
/// key on Linux. Returns `None` for keys niri cannot name.
pub(crate) fn chord_to_niri_bind(chord: &KeyChord) -> Option<String> {
    let key = niri_key_name(&chord.key)?;
    let mut parts: Vec<&str> = Vec::new();
    // alphacode `cmd` == macOS Command == Wayland Super.
    if chord.cmd {
        parts.push("Super");
    }
    if chord.ctrl {
        parts.push("Ctrl");
    }
    if chord.alt {
        parts.push("Alt");
    }
    if chord.shift {
        parts.push("Shift");
    }
    let mods = parts.join("+");
    if mods.is_empty() {
        Some(key)
    } else {
        Some(format!("{mods}+{key}"))
    }
}

/// Translate a canonical alphacode key token into the XKB key name niri expects.
/// Returns `None` for tokens with no stable niri spelling.
fn niri_key_name(key: &str) -> Option<String> {
    let named = match key {
        ";" => "Semicolon",
        "'" => "Apostrophe",
        "[" => "bracketleft",
        "]" => "bracketright",
        "\\" => "backslash",
        "/" => "slash",
        "," => "comma",
        "." => "period",
        "-" => "minus",
        "=" => "equal",
        "`" => "grave",
        "left" => "Left",
        "right" => "Right",
        "up" => "Up",
        "down" => "Down",
        "pageup" => "Page_Up",
        "pagedown" => "Page_Down",
        "home" => "Home",
        "end" => "End",
        "insert" => "Insert",
        "delete" => "Delete",
        "backspace" => "BackSpace",
        "enter" => "Return",
        "esc" => "Escape",
        "tab" => "Tab",
        "space" => "space",
        other => {
            // Single letters: niri accepts the lowercase XKB name (`a`..`z`).
            if other.len() == 1 && other.chars().all(|c| c.is_ascii_alphanumeric()) {
                return Some(other.to_string());
            }
            // Function keys f1..f24 -> F1..F24.
            if let Some(rest) = other.strip_prefix('f')
                && !rest.is_empty()
                && rest.chars().all(|c| c.is_ascii_digit())
            {
                return Some(format!("F{rest}"));
            }
            return None;
        }
    };
    Some(named.to_string())
}

/// Escape a string for inclusion inside a KDL double-quoted string.
fn kdl_escape(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}

/// POSIX-shell single-quote escaping for an argument passed via `sh -c`.
fn sh_single_quote(input: &str) -> String {
    format!("'{}'", input.replace('\'', r#"'\''"#))
}

/// Build the `sh -c` command string a bind runs to open alphacode in `dir`.
///
/// We `cd` into the directory (falling back to `$HOME` if it has since been
/// removed), then launch alphacode via the user's terminal. The terminal is chosen
/// by `terminal` (e.g. `kitty`); we pass it the alphacode executable directly.
fn launch_shell_command(
    exe_path: &str,
    terminal: &str,
    dir: &str,
    chord: &str,
    self_dev: bool,
) -> String {
    let dir_q = sh_single_quote(dir);
    let exe_q = sh_single_quote(exe_path);
    let term_q = sh_single_quote(terminal);
    let chord_q = sh_single_quote(chord);
    let subcmd = if self_dev { " self-dev" } else { "" };
    // cd with $HOME fallback, then exec the terminal running alphacode.
    format!(
        "if [ -d {dir_q} ]; then cd {dir_q}; else cd \"$HOME\"; fi; exec {term_q} {exe_q} --spawn-hotkey {chord_q}{subcmd}",
        dir_q = dir_q,
        term_q = term_q,
        exe_q = exe_q,
        chord_q = chord_q,
        subcmd = subcmd,
    )
}

/// Render a single niri `bind` line for one hotkey, or `None` if the chord
/// cannot be expressed in niri.
pub(crate) fn render_niri_bind_line(
    hotkey: &NiriHotkey,
    exe_path: &str,
    terminal: &str,
    indent: &str,
) -> Option<String> {
    let bind = chord_to_niri_bind(&hotkey.chord)?;
    let title = if hotkey.self_dev {
        format!("alphacode: {} (self-dev)", hotkey.label)
    } else {
        format!("alphacode: {}", hotkey.label)
    };
    let shell = launch_shell_command(
        exe_path,
        terminal,
        &hotkey.dir,
        &hotkey.chord.canonical(),
        hotkey.self_dev,
    );
    Some(format!(
        "{indent}{bind} hotkey-overlay-title=\"{title}\" {{ spawn \"sh\" \"-c\" \"{shell}\"; }}",
        indent = indent,
        bind = bind,
        title = kdl_escape(&title),
        shell = kdl_escape(&shell),
    ))
}

/// Render the full managed block (sentinels + one bind per hotkey), indented to
/// sit inside `binds { }`. Hotkeys niri cannot express are skipped. Returns
/// `None` when no hotkey could be rendered.
pub(crate) fn render_niri_block(
    hotkeys: &[NiriHotkey],
    exe_path: &str,
    terminal: &str,
    indent: &str,
) -> Option<String> {
    let mut lines: Vec<String> = Vec::new();
    for hk in hotkeys {
        if let Some(line) = render_niri_bind_line(hk, exe_path, terminal, indent) {
            lines.push(line);
        }
    }
    if lines.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str(indent);
    out.push_str(NIRI_BLOCK_BEGIN);
    out.push('\n');
    for line in &lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(indent);
    out.push_str(NIRI_BLOCK_END);
    Some(out)
}

/// Result of splicing the managed block into a config: the new text plus whether
/// anything actually changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SpliceResult {
    pub text: String,
    pub changed: bool,
}

/// Splice `block` (a fully-rendered managed region, no trailing newline) into
/// `config`'s `binds { }` section.
///
/// Behavior:
/// - If a previous managed region (between the sentinels) exists, replace it in
///   place. This keeps re-installs idempotent and position-stable.
/// - Otherwise, insert the block just inside the opening `binds {` line.
/// - If there is no `binds {` block at all, append a fresh `binds { ... }` at
///   the end of the file.
///
/// Returns `changed = false` (and the original text) when the existing managed
/// region already equals `block`, so callers can skip a no-op write.
pub(crate) fn splice_managed_block(config: &str, block: &str) -> SpliceResult {
    // 1) Replace an existing managed region if present.
    if let (Some(begin_idx), Some(end_line_end)) = find_managed_region(config) {
        // begin_idx is the byte offset of the start of the BEGIN line; the
        // managed region runs through the end of the END line (including its
        // trailing newline). Re-emit `block` plus that newline so the result is
        // byte-identical to a fresh insert (keeps re-installs idempotent).
        let before = &config[..begin_idx];
        let after = &config[end_line_end..];
        let new_text = format!("{before}{block}\n{after}");
        let changed = new_text != config;
        return SpliceResult {
            text: new_text,
            changed,
        };
    }

    // 2) Insert just inside an existing `binds {` block.
    if let Some(insert_at) = binds_block_insert_point(config) {
        let before = &config[..insert_at];
        let after = &config[insert_at..];
        // Terminate the block with a newline so the END sentinel never runs into
        // the following bind line (which would later swallow it on replace).
        let new_text = format!("{before}{block}\n{after}");
        return SpliceResult {
            text: new_text,
            changed: true,
        };
    }

    // 3) No binds block: append a new one.
    let mut new_text = config.to_string();
    if !new_text.is_empty() && !new_text.ends_with('\n') {
        new_text.push('\n');
    }
    new_text.push_str("\nbinds {\n");
    new_text.push_str(block);
    new_text.push('\n');
    new_text.push_str("}\n");
    SpliceResult {
        text: new_text,
        changed: true,
    }
}

/// Find the byte range of an existing managed region: `(start_of_BEGIN_line,
/// end_of_END_line_including_newline)`. Returns `None` if either sentinel is
/// missing.
fn find_managed_region(config: &str) -> (Option<usize>, Option<usize>) {
    let Some(begin_pos) = config.find(NIRI_BLOCK_BEGIN) else {
        return (None, None);
    };
    // Back up to the start of the BEGIN line (include its indentation).
    let line_start = config[..begin_pos].rfind('\n').map(|i| i + 1).unwrap_or(0);

    let Some(end_pos) = config[begin_pos..].find(NIRI_BLOCK_END) else {
        return (Some(line_start), None);
    };
    let end_abs = begin_pos + end_pos;
    // Extend through the rest of the END line, including its trailing newline.
    let line_end = match config[end_abs..].find('\n') {
        Some(nl) => end_abs + nl + 1,
        None => config.len(),
    };
    (Some(line_start), Some(line_end))
}

/// Byte offset just after the first `binds {` opening line's newline, i.e. the
/// point to insert new binds so they land inside the block. Returns `None` if no
/// `binds {` block exists.
fn binds_block_insert_point(config: &str) -> Option<usize> {
    for (idx, line) in line_offsets(config) {
        let trimmed = line.trim_start();
        if trimmed.starts_with("binds") && trimmed.trim_end().ends_with('{') {
            // Insert right after this line's terminating newline.
            let after_line = idx + line.len();
            // line includes no newline; advance past it if present.
            return Some(if config[after_line..].starts_with('\n') {
                after_line + 1
            } else {
                after_line
            });
        }
    }
    None
}

/// Iterate `(byte_offset, line_without_newline)` pairs.
fn line_offsets(s: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, ch) in s.char_indices() {
        if ch == '\n' {
            out.push((start, &s[start..i]));
            start = i + 1;
        }
    }
    if start < s.len() {
        out.push((start, &s[start..]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(s: &str) -> KeyChord {
        KeyChord::parse(s).unwrap()
    }

    fn hk(chord_str: &str, dir: &str, label: &str, self_dev: bool) -> NiriHotkey {
        NiriHotkey {
            chord: chord(chord_str),
            dir: dir.to_string(),
            label: label.to_string(),
            self_dev,
        }
    }

    #[test]
    fn maps_common_chords_to_niri_syntax() {
        // cmd maps to Super on Linux.
        assert_eq!(
            chord_to_niri_bind(&chord("cmd+;")).unwrap(),
            "Super+Semicolon"
        );
        assert_eq!(
            chord_to_niri_bind(&chord("cmd+shift+'")).unwrap(),
            "Super+Shift+Apostrophe"
        );
        assert_eq!(
            chord_to_niri_bind(&chord("alt+[")).unwrap(),
            "Alt+bracketleft"
        );
        assert_eq!(
            chord_to_niri_bind(&chord("ctrl+\\")).unwrap(),
            "Ctrl+backslash"
        );
        assert_eq!(chord_to_niri_bind(&chord("alt+b")).unwrap(), "Alt+b");
    }

    #[test]
    fn rejects_unmappable_keys() {
        // An empty/odd token has no niri name.
        assert!(niri_key_name("scrolllock").is_none());
    }

    #[test]
    fn renders_bind_line_with_cd_and_terminal() {
        let line = render_niri_bind_line(
            &hk("alt+;", "/home/jeremy/alphacode", "alphacode", true),
            "/home/jeremy/.local/bin/alphacode",
            "kitty",
            "    ",
        )
        .unwrap();
        assert!(line.contains("Alt+Semicolon"));
        assert!(line.contains("self-dev"));
        assert!(line.contains("hotkey-overlay-title=\"alphacode: alphacode (self-dev)\""));
        assert!(line.contains("spawn \"sh\" \"-c\""));
        assert!(line.contains("/home/jeremy/alphacode"));
        assert!(line.starts_with("    "));
    }

    #[test]
    fn render_block_wraps_sentinels() {
        let block = render_niri_block(
            &[
                hk("alt+;", "/home/u", "home", false),
                hk("alt+'", "/home/u/proj", "proj", false),
            ],
            "/bin/alphacode",
            "kitty",
            "    ",
        )
        .unwrap();
        assert!(block.starts_with("    // >>> alphacode launch hotkeys (managed) >>>"));
        assert!(
            block
                .trim_end()
                .ends_with("// <<< alphacode launch hotkeys (managed) <<<")
        );
        assert_eq!(block.matches("spawn \"sh\"").count(), 2);
    }

    #[test]
    fn splice_inserts_into_existing_binds_block() {
        let cfg = "binds {\n    Alt+Tab { focus-window-previous; }\n}\n";
        let block = render_niri_block(
            &[hk("alt+;", "/home/u", "home", false)],
            "/bin/alphacode",
            "kitty",
            "    ",
        )
        .unwrap();
        let res = splice_managed_block(cfg, &block);
        assert!(res.changed);
        assert!(res.text.contains(NIRI_BLOCK_BEGIN));
        assert!(res.text.contains("Alt+Tab { focus-window-previous; }"));
        // Managed block sits after the binds { line.
        let binds_idx = res.text.find("binds {").unwrap();
        let begin_idx = res.text.find(NIRI_BLOCK_BEGIN).unwrap();
        assert!(begin_idx > binds_idx);
    }

    #[test]
    fn splice_replaces_existing_managed_region_in_place() {
        let block_v1 = render_niri_block(
            &[hk("alt+;", "/home/u", "home", false)],
            "/bin/alphacode",
            "kitty",
            "    ",
        )
        .unwrap();
        let cfg = format!("binds {{\n{block_v1}\n    Alt+Tab {{ focus-window-previous; }}\n}}\n");

        let block_v2 = render_niri_block(
            &[hk("alt+;", "/home/u/newproj", "newproj", false)],
            "/bin/alphacode",
            "kitty",
            "    ",
        )
        .unwrap();
        let res = splice_managed_block(&cfg, &block_v2);
        assert!(res.changed);
        // Only one managed region.
        assert_eq!(res.text.matches(NIRI_BLOCK_BEGIN).count(), 1);
        assert!(res.text.contains("newproj"));
        assert!(!res.text.contains("\"home\""));
        // Untouched bind preserved.
        assert!(res.text.contains("Alt+Tab { focus-window-previous; }"));
    }

    #[test]
    fn splice_is_idempotent() {
        let block = render_niri_block(
            &[hk("alt+;", "/home/u", "home", false)],
            "/bin/alphacode",
            "kitty",
            "    ",
        )
        .unwrap();
        let cfg = "binds {\n    Alt+Tab { focus-window-previous; }\n}\n";
        let first = splice_managed_block(cfg, &block);
        let second = splice_managed_block(&first.text, &block);
        assert!(!second.changed);
        assert_eq!(first.text, second.text);
    }

    #[test]
    fn splice_appends_binds_block_when_missing() {
        let block = render_niri_block(
            &[hk("alt+;", "/home/u", "home", false)],
            "/bin/alphacode",
            "kitty",
            "    ",
        )
        .unwrap();
        let cfg = "// no binds here\noutput \"eDP-1\" {}\n";
        let res = splice_managed_block(cfg, &block);
        assert!(res.changed);
        assert!(res.text.contains("binds {"));
        assert!(res.text.contains(NIRI_BLOCK_BEGIN));
    }

    #[test]
    fn shell_command_cds_and_self_devs() {
        let s = launch_shell_command(
            "/bin/alphacode",
            "kitty",
            "/home/u/proj",
            "cmd+shift+'",
            true,
        );
        assert!(s.contains("cd '/home/u/proj'"));
        assert!(s.contains("exec 'kitty' '/bin/alphacode' --spawn-hotkey"));
        assert!(s.contains("'cmd+shift+'\\''' self-dev"));
        assert!(s.contains("\"$HOME\""));
    }
}
