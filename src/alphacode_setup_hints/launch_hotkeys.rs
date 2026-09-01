//! Resolve the global "launch a new alphacode" hotkeys from config into a concrete
//! list of (chord, shell command, script file) tuples.
//!
//! There are two layers here:
//!
//! * The **pure** resolver ([`resolve_launch_hotkeys`]) turns a
//!   [`LaunchHotkeysConfig`] plus the support-file paths into a list of
//!   [`ResolvedLaunchHotkey`]s. An empty config reproduces alphacode's historical
//!   three built-in hotkeys (home / last project / self-dev), so existing
//!   installs behave identically until auto-import bakes a richer per-repo
//!   mapping. This layer is unit-tested without touching the machine.
//! * The **macOS** glue ([`chord_to_global_hotkey`]) maps a parsed
//!   [`KeyChord`] onto a `global_hotkey::HotKey` so the launchd listener can
//!   register it.
//!
//! Keeping the resolver pure means the chord/dir layout the user sees is exactly
//! what we can assert in tests, and the listener stays a thin dispatcher.

use crate::alphacode_config_types::{LaunchHotkeyEntry, LaunchHotkeysConfig};
#[cfg(any(test, target_os = "macos"))]
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::alphacode_setup_hints::keymap::KeyChord;

/// POSIX single-quote escaping (turns `'` into `'\''`). Local copy so this
/// module compiles on every platform without pulling in the macOS-only
/// `macos_terminal` module.
fn escape_shell_single_quotes(input: &str) -> String {
    input.replace('\'', r#"'\''"#)
}

/// Build a alphacode launch snippet that pauses on non-zero exit so the user can
/// read the error before the terminal closes. Mirrors the macOS launcher's
/// behavior; kept local so the resolver is platform-independent.
#[cfg(any(test, target_os = "macos"))]
fn paused_alphacode_shell_command_with_args(exe_path: &str, args: &[String]) -> String {
    let escaped_exe = escape_shell_single_quotes(exe_path);
    let mut arg_str = String::new();
    for arg in args {
        arg_str.push_str(" '");
        arg_str.push_str(&escape_shell_single_quotes(arg));
        arg_str.push('\'');
    }
    format!(
        r#"if [ ! -x '{exe}' ]; then printf 'alphacode executable not found.\n'; exit 127; fi; '{exe}'{args}; status=$?; if [ "$status" -ne 0 ]; then printf '\nAlphacode exited with status %s.\n' "$status"; printf 'Press Enter to close... '; read -r _; fi; exit "$status""#,
        exe = escaped_exe,
        args = arg_str,
    )
}

/// One entry in the listener's `plan.json`: the chord to register and the script
/// to run when it fires. Written by the installer, read by the launchd listener,
/// so the listener stays a thin dispatcher that never re-parses config.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg(any(test, target_os = "macos"))]
pub(crate) struct PlanEntry {
    pub chord: String,
    pub script: String,
}

/// A fully-resolved launch hotkey ready to be turned into a script and a chord
/// registration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedLaunchHotkey {
    /// alphacode-style chord string, e.g. `cmd+;`.
    pub chord: String,
    /// Stable file name for this entry's launch script.
    pub script_file_name: String,
    /// Shell snippet that `cd`s into the target directory (with a `$HOME`
    /// fallback) before launching alphacode.
    pub cd_prefix: String,
    /// Original configured directory target. May be a sentinel such as `$HOME`,
    /// `$LAST_DIR`, or `$LAST_REPO`; direct launchers resolve it at fire time.
    pub dir: String,
    /// Extra CLI args passed to alphacode (e.g. `self-dev`).
    pub args: Vec<String>,
    /// Human label for notices.
    pub label: String,
}

/// `cd` snippet that reads a directory from `dir_file` at fire time, falling
/// back to `$HOME` when the file is missing or the directory no longer exists.
fn cd_from_dir_file(dir_file: &str) -> String {
    let escaped = escape_shell_single_quotes(dir_file);
    format!(
        "__jc_dir=\"$(cat '{escaped}' 2>/dev/null)\"; if [ -n \"$__jc_dir\" ] && [ -d \"$__jc_dir\" ]; then cd \"$__jc_dir\"; else cd \"$HOME\"; fi; "
    )
}

/// `cd` snippet for a fixed absolute directory, still falling back to `$HOME` if
/// the directory has since been deleted/moved (so a stale baked path never
/// leaves the user in a broken shell).
fn cd_to_fixed_dir(dir: &str) -> String {
    let escaped = escape_shell_single_quotes(dir);
    format!("if [ -d '{escaped}' ]; then cd '{escaped}'; else cd \"$HOME\"; fi; ")
}

/// Translate one config entry's `dir` (path or sentinel) into a `cd` prefix.
fn cd_prefix_for_dir(dir: &str, last_dir_file: &str, last_repo_file: &str) -> String {
    match dir {
        "$HOME" => "cd \"$HOME\"; ".to_string(),
        "$LAST_DIR" => cd_from_dir_file(last_dir_file),
        "$LAST_REPO" => cd_from_dir_file(last_repo_file),
        path => cd_to_fixed_dir(path),
    }
}

/// A filesystem-safe, stable script name for a chord, e.g. `cmd+[` ->
/// `launch_alphacode_cmd_bracketleft.sh`. Collisions are avoided by appending the
/// slot index, since two distinct entries could in theory normalize the same.
fn script_name_for(chord: &str, index: usize) -> String {
    let mut slug = String::new();
    for ch in chord.chars() {
        match ch {
            'a'..='z' | '0'..='9' => slug.push(ch),
            'A'..='Z' => slug.push(ch.to_ascii_lowercase()),
            '+' => slug.push('_'),
            ';' => slug.push_str("semicolon"),
            '\'' => slug.push_str("quote"),
            '[' => slug.push_str("bracketleft"),
            ']' => slug.push_str("bracketright"),
            '\\' => slug.push_str("backslash"),
            '/' => slug.push_str("slash"),
            ',' => slug.push_str("comma"),
            '.' => slug.push_str("period"),
            '-' => slug.push_str("minus"),
            '=' => slug.push_str("equal"),
            '`' => slug.push_str("backtick"),
            _ => slug.push('x'),
        }
    }
    format!("launch_alphacode_{index}_{slug}.sh")
}

/// The built-in default entries, used when config has none. Mirrors alphacode's
/// historical `Cmd+;` / `Cmd+'` / `Cmd+Shift+'` layout.
pub(crate) fn default_launch_entries() -> Vec<LaunchHotkeyEntry> {
    vec![
        LaunchHotkeyEntry {
            chord: "cmd+;".to_string(),
            dir: "$HOME".to_string(),
            label: "home".to_string(),
            self_dev: false,
        },
        LaunchHotkeyEntry {
            chord: "cmd+'".to_string(),
            dir: "$LAST_DIR".to_string(),
            label: "last project".to_string(),
            self_dev: false,
        },
        LaunchHotkeyEntry {
            chord: "cmd+shift+'".to_string(),
            dir: "$LAST_REPO".to_string(),
            label: "self-dev".to_string(),
            self_dev: true,
        },
    ]
}

/// Resolve a config into concrete launch hotkeys. Empty config -> built-in
/// defaults. Invalid chords are skipped (logged by the caller). Duplicate chords
/// keep the first occurrence so a later malformed edit cannot shadow an earlier
/// binding.
pub(crate) fn resolve_launch_hotkeys(
    config: &LaunchHotkeysConfig,
    exe_path: &str,
    last_dir_file: &str,
    last_repo_file: &str,
) -> Vec<ResolvedLaunchHotkey> {
    let entries: Vec<LaunchHotkeyEntry> = if config.entries.is_empty() {
        default_launch_entries()
    } else {
        config.entries.clone()
    };

    let mut seen_chords: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        let Some(chord) = KeyChord::parse(&entry.chord) else {
            continue;
        };
        let canonical = chord.canonical();
        if seen_chords.contains(&canonical) {
            continue;
        }
        seen_chords.push(canonical.clone());

        let cd_prefix = cd_prefix_for_dir(&entry.dir, last_dir_file, last_repo_file);
        let args: Vec<String> = if entry.self_dev {
            vec!["self-dev".to_string()]
        } else {
            Vec::new()
        };
        let _ = exe_path; // shell command is built by the installer; exe kept for symmetry
        out.push(ResolvedLaunchHotkey {
            chord: canonical,
            script_file_name: script_name_for(&entry.chord, index),
            cd_prefix,
            dir: entry.dir.clone(),
            args,
            label: if entry.label.is_empty() {
                entry.dir.clone()
            } else {
                entry.label.clone()
            },
        });
    }
    out
}

/// Resolve a configured hotkey directory into a concrete cwd for direct spawns.
/// Missing/stale dynamic targets fall back to `$HOME`, matching the shell-script
/// launcher behavior.
pub(crate) fn resolve_target_dir(dir: &str, last_dir_file: &str, last_repo_file: &str) -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    match dir {
        "$HOME" => home,
        "$LAST_DIR" => read_existing_dir(last_dir_file).unwrap_or(home),
        "$LAST_REPO" => read_existing_dir(last_repo_file).unwrap_or(home),
        path => {
            let expanded = if let Some(rest) = path.strip_prefix("~/") {
                home.join(rest)
            } else {
                PathBuf::from(path)
            };
            if expanded.is_dir() { expanded } else { home }
        }
    }
}

fn read_existing_dir(file: &str) -> Option<PathBuf> {
    let text = std::fs::read_to_string(file).ok()?;
    let path = PathBuf::from(text.trim());
    path.is_dir().then_some(path)
}

/// Build the full shell command (run inside the freshly opened terminal) for a
/// resolved hotkey.
#[cfg(any(test, target_os = "macos"))]
pub(crate) fn shell_command_for(entry: &ResolvedLaunchHotkey, exe_path: &str) -> String {
    format!(
        "{}{}",
        entry.cd_prefix,
        paused_alphacode_shell_command_with_args(exe_path, &entry.args)
    )
}

/// Map a alphacode key token onto a `global_hotkey` `Code`. Returns `None` for
/// tokens we cannot register (the caller logs and skips).
#[cfg(target_os = "macos")]
pub(crate) fn key_token_to_code(key: &str) -> Option<global_hotkey::hotkey::Code> {
    use global_hotkey::hotkey::Code;
    Some(match key {
        ";" => Code::Semicolon,
        "'" => Code::Quote,
        "[" => Code::BracketLeft,
        "]" => Code::BracketRight,
        "\\" => Code::Backslash,
        "/" => Code::Slash,
        "," => Code::Comma,
        "." => Code::Period,
        "-" => Code::Minus,
        "=" => Code::Equal,
        "`" => Code::Backquote,
        "a" => Code::KeyA,
        "b" => Code::KeyB,
        "c" => Code::KeyC,
        "d" => Code::KeyD,
        "e" => Code::KeyE,
        "f" => Code::KeyF,
        "g" => Code::KeyG,
        "h" => Code::KeyH,
        "i" => Code::KeyI,
        "j" => Code::KeyJ,
        "k" => Code::KeyK,
        "l" => Code::KeyL,
        "m" => Code::KeyM,
        "n" => Code::KeyN,
        "o" => Code::KeyO,
        "p" => Code::KeyP,
        "q" => Code::KeyQ,
        "r" => Code::KeyR,
        "s" => Code::KeyS,
        "t" => Code::KeyT,
        "u" => Code::KeyU,
        "v" => Code::KeyV,
        "w" => Code::KeyW,
        "x" => Code::KeyX,
        "y" => Code::KeyY,
        "z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        _ => return None,
    })
}

/// Map a parsed [`KeyChord`] onto a `global_hotkey::HotKey`. Returns `None` if
/// the key token is not registerable.
#[cfg(target_os = "macos")]
pub(crate) fn chord_to_global_hotkey(chord: &KeyChord) -> Option<global_hotkey::hotkey::HotKey> {
    use global_hotkey::hotkey::{HotKey, Modifiers};
    let code = key_token_to_code(&chord.key)?;
    let mut mods = Modifiers::empty();
    if chord.cmd {
        mods |= Modifiers::META;
    }
    if chord.ctrl {
        mods |= Modifiers::CONTROL;
    }
    if chord.alt {
        mods |= Modifiers::ALT;
    }
    if chord.shift {
        mods |= Modifiers::SHIFT;
    }
    Some(HotKey::new(Some(mods), code))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(entries: Vec<LaunchHotkeyEntry>) -> LaunchHotkeysConfig {
        LaunchHotkeysConfig {
            enabled: Some(true),
            entries,
            imported: true,
        }
    }

    #[test]
    fn empty_config_reproduces_three_builtins() {
        let resolved = resolve_launch_hotkeys(
            &LaunchHotkeysConfig::default(),
            "/bin/alphacode",
            "/last_dir",
            "/last_repo",
        );
        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved[0].chord, "cmd+;");
        assert!(resolved[0].cd_prefix.contains("cd \"$HOME\""));
        assert!(resolved[0].args.is_empty());
        assert_eq!(resolved[1].chord, "cmd+'");
        assert!(resolved[1].cd_prefix.contains("/last_dir"));
        assert_eq!(resolved[2].chord, "cmd+shift+'");
        assert!(resolved[2].cd_prefix.contains("/last_repo"));
        assert_eq!(resolved[2].args, vec!["self-dev".to_string()]);
    }

    #[test]
    fn fixed_dir_entry_cds_to_path_with_home_fallback() {
        let resolved = resolve_launch_hotkeys(
            &cfg(vec![LaunchHotkeyEntry {
                chord: "cmd+[".to_string(),
                dir: "/Users/jeremy/proj".to_string(),
                label: "proj".to_string(),
                self_dev: false,
            }]),
            "/bin/alphacode",
            "/last_dir",
            "/last_repo",
        );
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].chord, "cmd+[");
        assert!(resolved[0].cd_prefix.contains("/Users/jeremy/proj"));
        assert!(resolved[0].cd_prefix.contains("cd \"$HOME\""));
        assert_eq!(resolved[0].label, "proj");
    }

    #[test]
    fn duplicate_chords_keep_first() {
        let resolved = resolve_launch_hotkeys(
            &cfg(vec![
                LaunchHotkeyEntry {
                    chord: "cmd+[".to_string(),
                    dir: "/a".to_string(),
                    label: String::new(),
                    self_dev: false,
                },
                LaunchHotkeyEntry {
                    chord: "cmd+[".to_string(),
                    dir: "/b".to_string(),
                    label: String::new(),
                    self_dev: false,
                },
            ]),
            "/bin/alphacode",
            "/last_dir",
            "/last_repo",
        );
        assert_eq!(resolved.len(), 1);
        assert!(resolved[0].cd_prefix.contains("/a"));
    }

    #[test]
    fn invalid_chord_is_skipped() {
        let resolved = resolve_launch_hotkeys(
            &cfg(vec![
                LaunchHotkeyEntry {
                    chord: "none".to_string(),
                    dir: "/a".to_string(),
                    label: String::new(),
                    self_dev: false,
                },
                LaunchHotkeyEntry {
                    chord: "cmd+]".to_string(),
                    dir: "/b".to_string(),
                    label: String::new(),
                    self_dev: false,
                },
            ]),
            "/bin/alphacode",
            "/last_dir",
            "/last_repo",
        );
        assert_eq!(resolved.len(), 1);
        assert_eq!(resolved[0].chord, "cmd+]");
    }

    #[test]
    fn script_names_are_unique_and_safe() {
        let resolved = resolve_launch_hotkeys(
            &cfg(vec![
                LaunchHotkeyEntry {
                    chord: "cmd+[".to_string(),
                    dir: "/a".to_string(),
                    label: String::new(),
                    self_dev: false,
                },
                LaunchHotkeyEntry {
                    chord: "cmd+]".to_string(),
                    dir: "/b".to_string(),
                    label: String::new(),
                    self_dev: false,
                },
                LaunchHotkeyEntry {
                    chord: "cmd+\\".to_string(),
                    dir: "/c".to_string(),
                    label: String::new(),
                    self_dev: false,
                },
            ]),
            "/bin/alphacode",
            "/last_dir",
            "/last_repo",
        );
        let names: Vec<&str> = resolved
            .iter()
            .map(|r| r.script_file_name.as_str())
            .collect();
        assert_eq!(names.len(), 3);
        let mut sorted = names.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 3, "script names must be unique: {names:?}");
        for n in &names {
            assert!(n.ends_with(".sh"));
            assert!(!n.contains(['[', ']', '\\', '/', ';', '\'']));
        }
    }

    #[test]
    fn shell_command_includes_cd_and_exe() {
        let resolved = resolve_launch_hotkeys(
            &cfg(vec![LaunchHotkeyEntry {
                chord: "cmd+[".to_string(),
                dir: "/Users/jeremy/proj".to_string(),
                label: "proj".to_string(),
                self_dev: false,
            }]),
            "/bin/alphacode",
            "/last_dir",
            "/last_repo",
        );
        let cmd = shell_command_for(&resolved[0], "/bin/alphacode");
        assert!(cmd.contains("/Users/jeremy/proj"));
        assert!(cmd.contains("/bin/alphacode"));
    }
}
