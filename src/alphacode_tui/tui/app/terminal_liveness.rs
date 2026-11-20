//! Detection of an abandoned controlling terminal.
//!
//! A TUI client can outlive its terminal: if the terminal emulator dies
//! without delivering SIGHUP (or the signal arrives while the runtime is
//! wedged), the client keeps running headless forever, holding its full
//! transcript and ~80-150 MB of heap. Dozens of such orphans were observed
//! stacking up from spawned swarm windows. crossterm's `EventStream` returns
//! `None` after input EOF, and the run loops used to just sleep and retry,
//! so nothing ever exited.
//!
//! This module answers one question cheaply: "did the controlling terminal
//! this process started with go away?" On Linux the kernel resets `tty_nr`
//! in `/proc/self/stat` to 0 even while a stale fd still reports `isatty=true`.
//! On Windows the attached console answers `GetConsoleCP` independently of
//! stdin redirection. Other platforms rely on the terminal hangup signal.

use std::sync::OnceLock;

static INITIAL_TERMINAL_ATTACHED: OnceLock<bool> = OnceLock::new();

/// Record the startup controlling terminal. Called implicitly by
/// [`terminal_abandoned`], but callers may invoke it early (before any chance
/// of the terminal dying) for a more faithful baseline.
pub(crate) fn capture_initial_terminal() {
    let _ = INITIAL_TERMINAL_ATTACHED.get_or_init(|| terminal_attached().unwrap_or(false));
}

/// True when this process started with a controlling terminal that has since
/// disappeared. An unknown state leaves the client running.
pub(crate) fn terminal_abandoned() -> bool {
    capture_initial_terminal();
    INITIAL_TERMINAL_ATTACHED.get() == Some(&true) && terminal_attached() == Some(false)
}

#[cfg(target_os = "linux")]
fn terminal_attached() -> Option<bool> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    parse_tty_nr(&stat).map(|tty_nr| tty_nr != 0)
}

#[cfg(windows)]
fn terminal_attached() -> Option<bool> {
    use windows_sys::Win32::Foundation::ERROR_INVALID_HANDLE;
    use windows_sys::Win32::System::Console::GetConsoleCP;

    if unsafe { GetConsoleCP() } != 0 {
        return Some(true);
    }
    (std::io::Error::last_os_error().raw_os_error() == Some(ERROR_INVALID_HANDLE as i32))
        .then_some(false)
}

#[cfg(not(any(target_os = "linux", windows)))]
fn terminal_attached() -> Option<bool> {
    None
}

/// Extract the `tty_nr` field (field 7) from `/proc/<pid>/stat` content.
/// The comm field (2) may contain spaces and parentheses, so fields are
/// counted from after the *last* `)`.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn parse_tty_nr(stat: &str) -> Option<u64> {
    let after_comm = &stat[stat.rfind(')')? + 1..];
    // after_comm fields: state(3) ppid(4) pgrp(5) session(6) tty_nr(7) ...
    after_comm
        .split_whitespace()
        .nth(4)
        .and_then(|field| field.parse::<u64>().ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tty_nr_from_stat_line() {
        let stat = "12345 (alphacode) S 1 12345 12345 34823 12345 4194304 0 0 0 0 1 2 0 0 20";
        assert_eq!(parse_tty_nr(stat), Some(34823));
    }

    #[test]
    fn parses_tty_nr_with_hostile_comm() {
        let stat = "1 (a b) c) R 1 1 1 0 1 0";
        assert_eq!(parse_tty_nr(stat), Some(0));
    }

    #[test]
    fn missing_paren_yields_none() {
        assert_eq!(parse_tty_nr("garbage"), None);
    }
}
