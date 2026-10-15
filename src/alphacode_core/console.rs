//! Console/terminal capability detection — the single source of truth for
//! "may I emit ANSI?", "how wide is the terminal?", and "can it render
//! box-drawing characters?".
//!
//! Every terminal-facing renderer in alphacode (CLI banners, status lines,
//! tables, spinners, slash-command output) must consult this module before
//! producing escape sequences. Rendering ANSI into a pipe, a log file, a CI
//! job's captured stdout, or a `--json` consumer produces literal
//! `\u{1b}[38;2;…m` garbage that no downstream parser can recover.
//!
//! Three independent signals can suppress styling, and they are checked in
//! increasing precedence:
//!
//! 1. **Capability** — is stderr even a terminal, and can it accept escapes?
//!    On legacy Windows consoles the answer is no (issue #498).
//! 2. **Environment** — `NO_COLOR` (the cross-tool convention),
//!    `ALPHACODE_NO_COLOR`, `TERM=dumb`, and `CLICOLOR=0`.
//! 3. **Process state** — quiet mode suppresses incidental output.
//!
//! `CLICOLOR_FORCE` / `FORCE_COLOR` override all three, so users who pipe
//! into a pager that *does* understand color still get it.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, Ordering};

/// Environment variable that suppresses styling, mirroring `NO_COLOR`.
pub const NO_COLOR_ENV: &str = "NO_COLOR";
/// Alphacode-specific styling opt-out.
pub const NO_COLOR_ENV_ALPHACODE: &str = "ALPHACODE_NO_COLOR";
/// Forces styling even when stdout/stderr is not a terminal.
pub const FORCE_COLOR_ENV: &str = "CLICOLOR_FORCE";
/// Secondary force-color variable, honored by many modern CLIs.
pub const FORCE_COLOR_ENV_ALT: &str = "FORCE_COLOR";
/// Classic opt-out honored by BSD `ls` and friends.
pub const CLICOLOR_ENV: &str = "CLICOLOR";
/// Explicit ASCII-only rendering opt-in.
pub const ASCII_ONLY_ENV: &str = "ALPHACODE_ASCII";
/// Overrides the detected terminal width (used by tests and by callers that
/// already know the real width, e.g. a renderer inside the TUI).
pub const WIDTH_ENV: &str = "ALPHACODE_WIDTH";

/// Fallback width when nothing can be detected (pipes, CI, dumb terminals).
pub const FALLBACK_WIDTH: usize = 80;
/// Lower bound for any layout math. Below this almost nothing is legible and
/// renderers should switch to their single-column fallback.
pub const MIN_USABLE_WIDTH: usize = 20;
/// Upper bound; wider is clamped so a stray 10,000-column report still lines
/// up instead of running off the screen.
pub const MAX_USABLE_WIDTH: usize = 200;

// ── Test / runtime overrides ───────────────────────────────────────────────
//
// `OnceLock` first-init pinning is not enough here: a full test binary almost
// always has some other test touching these first, so a later pin would
// silently no-op. Overrides are therefore separate atomics that win over the
// cached detection result. Production code never writes them.

/// 0 = unset, 1 = force-enabled, 2 = force-disabled.
static ANSI_OVERRIDE: AtomicU8 = AtomicU8::new(0);
/// 0 = unset, 1 = force-enabled, 2 = force-disabled.
static UNICODE_OVERRIDE: AtomicU8 = AtomicU8::new(0);
/// 0 = unset, otherwise the forced width + 1 so that `0` can mean "unset".
static WIDTH_OVERRIDE: AtomicU8 = AtomicU8::new(0);

/// Test seam: pin [`ansi_enabled`] on/off. Pass `None` to restore detection.
#[doc(hidden)]
pub fn set_ansi_enabled_override(enabled: Option<bool>) {
    let value = match enabled {
        Some(true) => 1,
        Some(false) => 2,
        None => 0,
    };
    ANSI_OVERRIDE.store(value, Ordering::Relaxed);
}

/// Test seam: pin Unicode box-drawing support on/off.
#[doc(hidden)]
pub fn set_unicode_override(enabled: Option<bool>) {
    let value = match enabled {
        Some(true) => 1,
        Some(false) => 2,
        None => 0,
    };
    UNICODE_OVERRIDE.store(value, Ordering::Relaxed);
}

/// Test seam: pin the reported terminal width.
#[doc(hidden)]
pub fn set_width_override(width: Option<usize>) {
    let value = match width {
        Some(w) if w <= 254 => (w as u8).wrapping_add(1),
        Some(_) => u8::MAX,
        None => 0,
    };
    WIDTH_OVERRIDE.store(value, Ordering::Relaxed);
}

/// Serialize any test that mutates one of the capability overrides above.
///
/// These overrides are process-global, and `cargo test` runs test functions on
/// parallel threads. Without this lock, a test that pins `ansi_enabled(false)`
/// to prove "no escapes when styling is off" silently disables styling for an
/// unrelated test running at the same moment, and the failure lands somewhere
/// that has nothing to do with the code that broke.
///
/// Hold the guard for the whole scope that mutates an override:
///
/// ```ignore
/// let _guard = crate::alphacode_core::console::test_override_lock();
/// console::set_ansi_enabled_override(Some(false));
/// // ... assertions ...
/// console::set_ansi_enabled_override(None);
/// ```
///
/// Drop the guard only after the overrides are restored.
#[doc(hidden)]
pub fn test_override_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| std::sync::Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Best-effort: enable ANSI (virtual terminal processing) on the stderr
/// console, then report whether ANSI output is safe to emit.
///
/// On non-Windows this is true exactly when stderr is a terminal. On Windows
/// it attempts to switch the console to VT mode first (a no-op on Windows
/// Terminal and modern conhost, which already support it) and returns false
/// when the console cannot accept escape sequences, so callers can fall back
/// to plain text instead of printing escape garbage.
pub fn stderr_supports_ansi() -> bool {
    use std::io::IsTerminal;
    if !std::io::stderr().is_terminal() {
        return false;
    }

    #[cfg(windows)]
    {
        enable_stderr_vt_processing()
    }
    #[cfg(not(windows))]
    {
        true
    }
}

#[cfg(windows)]
fn enable_stderr_vt_processing() -> bool {
    use windows_sys::Win32::System::Console::{
        CONSOLE_MODE, ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle,
        STD_ERROR_HANDLE, SetConsoleMode,
    };

    unsafe {
        let handle = GetStdHandle(STD_ERROR_HANDLE);
        if handle.is_null() || handle == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return false;
        }
        let mut mode: CONSOLE_MODE = 0;
        if GetConsoleMode(handle, &mut mode) == 0 {
            return false;
        }
        if mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING != 0 {
            return true;
        }
        SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
    }
}

/// Whether the user has explicitly asked for color, regardless of TTY state.
///
/// `CLICOLOR_FORCE` and `FORCE_COLOR` are the de-facto standard opt-ins; a
/// non-zero, non-empty value enables styling even into a pipe.
pub fn color_forced() -> bool {
    fn truthy(raw: Option<std::ffi::OsString>) -> bool {
        match raw {
            None => false,
            Some(value) => {
                let value = value.to_string_lossy();
                let value = value.trim();
                !value.is_empty() && value != "0" && !value.eq_ignore_ascii_case("false")
            }
        }
    }

    truthy(std::env::var_os(FORCE_COLOR_ENV)) || truthy(std::env::var_os(FORCE_COLOR_ENV_ALT))
}

/// Whether styling has been explicitly disabled by the environment.
///
/// Honors `NO_COLOR` (any non-empty value), `ALPHACODE_NO_COLOR`, `TERM=dumb`,
/// and `CLICOLOR=0`.
pub fn color_disabled_by_env() -> bool {
    if std::env::var_os(NO_COLOR_ENV).is_some_and(|v| !v.is_empty()) {
        return true;
    }
    if std::env::var(NO_COLOR_ENV_ALPHACODE).is_ok_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    }) {
        return true;
    }
    if std::env::var("TERM").is_ok_and(|v| v.eq_ignore_ascii_case("dumb")) {
        return true;
    }
    if std::env::var(CLICOLOR_ENV).is_ok_and(|v| v.trim() == "0") {
        return true;
    }
    false
}

/// The authoritative "may I emit ANSI escapes?" decision for CLI output.
///
/// Every style-producing helper must route through this. Returns `true` only
/// when one of the following holds:
///
/// * `CLICOLOR_FORCE` / `FORCE_COLOR` is set (user knows best), or
/// * stderr is a terminal, the console can accept escapes, and nothing in the
///   environment has opted out of color.
///
/// The result is cached after the first call, because it performs an ioctl on
/// Windows; [`set_ansi_enabled_override`] exists for tests that need to change
/// the answer afterwards.
pub fn ansi_enabled() -> bool {
    match ANSI_OVERRIDE.load(Ordering::Relaxed) {
        1 => return true,
        2 => return false,
        _ => {}
    }
    *ANSI_CACHE.get_or_init(|| {
        if color_forced() {
            return true;
        }
        if color_disabled_by_env() {
            return false;
        }
        if quiet_requested() {
            return false;
        }
        stderr_supports_ansi()
    })
}

/// Whether the process is in quiet mode (suppresses incidental output).
fn quiet_requested() -> bool {
    std::env::var("ALPHACODE_QUIET")
        .map(|value| value == "1" || value.eq_ignore_ascii_case("true"))
        .unwrap_or(false)
}

static ANSI_CACHE: OnceLock<bool> = OnceLock::new();

/// Whether the terminal can render box-drawing and block characters.
///
/// Historically this was inferred from `LANG`/`LC_ALL`/`LC_CTYPE`/`TERM`
/// containing "utf", with a blanket `cfg!(windows)` escape hatch. That hatch
/// was wrong in both directions: Windows *conhost* with a legacy code page
/// renders `─` as mojibake, while a Windows user running under Cygwin/MSYS in
/// a UTF-8 shell got ASCII because `LANG` was unset. Detection now also
/// accepts an explicit `ALPHACODE_ASCII` opt-out and the same
/// `CLICOLOR_FORCE`-style overrides the color path uses.
pub fn unicode_supported() -> bool {
    match UNICODE_OVERRIDE.load(Ordering::Relaxed) {
        1 => return true,
        2 => return false,
        _ => {}
    }
    *UNICODE_CACHE.get_or_init(detect_unicode_support)
}

fn detect_unicode_support() -> bool {
    if std::env::var(ASCII_ONLY_ENV).is_ok_and(|v| {
        matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    }) {
        return false;
    }
    // `NO_COLOR` users are frequently non-TTY / screen-reader / log contexts
    // where Unicode chrome is as unwanted as color.
    if color_disabled_by_env() && !color_forced() {
        return false;
    }

    for key in ["LANG", "LC_ALL", "LC_CTYPE"] {
        if std::env::var(key).is_ok_and(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("utf-8") || value.contains("utf8")
        }) {
            return true;
        }
    }
    if std::env::var("TERM").is_ok_and(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("utf-8") || value.contains("utf8")
    }) {
        return true;
    }

    #[cfg(windows)]
    {
        // Modern Windows consoles (Windows Terminal, conhost with a UTF-8
        // code page) handle box drawing natively. A user who explicitly
        // asked for ASCII still gets ASCII via `ALPHACODE_ASCII` above.
        windows_console_is_utf8()
    }
    #[cfg(not(windows))]
    {
        // On Unix, a UTF-8 locale is the norm; fall back to true so we don't
        // ship a minus sign where a box character belongs.
        true
    }
}

#[cfg(windows)]
fn windows_console_is_utf8() -> bool {
    use windows_sys::Win32::Globalization::{CP_UTF8, GetACP, GetOEMCP};
    use windows_sys::Win32::System::Console::GetConsoleCP;

    unsafe {
        let cp = GetConsoleCP();
        if cp == CP_UTF8 {
            return true;
        }
        // A redirected/non-console handle reports 0; fall back to the process
        // ANSI/OEM code pages before giving up.
        cp != 0 && (GetACP() == CP_UTF8 || GetOEMCP() == CP_UTF8)
    }
}

static UNICODE_CACHE: OnceLock<bool> = OnceLock::new();

/// Usable width of the terminal in columns, clamped to a sane range.
///
/// Resolution order: an explicit override (test seam), `ALPHACODE_WIDTH`,
/// `COLUMNS`, then a real ioctl. Falls back to [`FALLBACK_WIDTH`] so callers
/// never have to handle `None`.
///
/// Cached: a renderer may call this once per line, and the ioctl does not
/// change mid-frame.
pub fn terminal_width() -> usize {
    let override_value = WIDTH_OVERRIDE.load(Ordering::Relaxed);
    if override_value != 0 {
        return clamp_width(override_value as usize - 1);
    }
    *WIDTH_CACHE.get_or_init(|| {
        if let Some(width) = env_width() {
            return clamp_width(width);
        }
        match crossterm::terminal::size() {
            Ok((cols, _)) if cols > 0 => clamp_width(cols as usize),
            _ => FALLBACK_WIDTH,
        }
    })
}

fn env_width() -> Option<usize> {
    for key in [WIDTH_ENV, "COLUMNS"] {
        if let Some(width) = std::env::var(key)
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            && width > 0
        {
            return Some(width);
        }
    }
    None
}

fn clamp_width(width: usize) -> usize {
    width.clamp(MIN_USABLE_WIDTH, MAX_USABLE_WIDTH)
}

static WIDTH_CACHE: OnceLock<usize> = OnceLock::new();

/// Usable terminal height in rows, or `None` when it cannot be determined.
///
/// Unlike width, height is not cached: it changes whenever the user resizes,
/// and layout decisions (how many rows to show) must track a resize.
pub fn terminal_height() -> Option<usize> {
    crossterm::terminal::size()
        .ok()
        .map(|(_, rows)| rows as usize)
        .filter(|rows| *rows > 0)
}

/// Clamp a requested render width to what the terminal can actually show.
///
/// Renderers should use this instead of `terminal_width()` directly so that a
/// caller asking for more space than exists gets a safe value instead of
/// silently overflowing and wrapping into the next line.
pub fn fit_width(requested: usize) -> usize {
    requested.min(terminal_width())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// Serialize env mutation across tests (process env is global).
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn env_scope(vars: &[(&str, Option<&str>)], body: impl FnOnce()) {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        let saved: Vec<(String, Option<String>)> = vars
            .iter()
            .map(|(k, _)| ((*k).to_string(), std::env::var(k).ok()))
            .collect();
        for (k, v) in vars {
            match v {
                Some(val) => unsafe { std::env::set_var(k, val) },
                None => unsafe { std::env::remove_var(k) },
            }
        }
        body();
        for (k, v) in saved {
            match v {
                Some(val) => unsafe { std::env::set_var(&k, val) },
                None => unsafe { std::env::remove_var(&k) },
            }
        }
    }

    #[test]
    fn no_color_disables_styling() {
        env_scope(&[(NO_COLOR_ENV, Some("1"))], || {
            assert!(color_disabled_by_env());
        });
        env_scope(&[(NO_COLOR_ENV, Some(""))], || {
            // An empty NO_COLOR is explicitly *not* a request (per no-color.org).
            assert!(!color_disabled_by_env());
        });
        env_scope(&[(NO_COLOR_ENV_ALPHACODE, Some("on"))], || {
            assert!(color_disabled_by_env());
        });
    }

    #[test]
    fn dumb_terminal_and_clicolor_zero_disable_styling() {
        env_scope(&[("TERM", Some("dumb"))], || {
            assert!(color_disabled_by_env());
        });
        env_scope(&[(CLICOLOR_ENV, Some("0"))], || {
            assert!(color_disabled_by_env());
        });
        env_scope(&[(CLICOLOR_ENV, Some("1"))], || {
            assert!(!color_disabled_by_env());
        });
    }

    #[test]
    fn force_color_wins_over_clicolor_zero() {
        env_scope(
            &[
                (FORCE_COLOR_ENV, Some("1")),
                (CLICOLOR_ENV, Some("0")),
                (NO_COLOR_ENV, Some("1")),
            ],
            || {
                assert!(color_forced());
            },
        );
        env_scope(&[(FORCE_COLOR_ENV, Some("0"))], || {
            assert!(!color_forced());
        });
    }

    #[test]
    fn ascii_opt_out_disables_unicode_chrome() {
        env_scope(&[(ASCII_ONLY_ENV, Some("1"))], || {
            assert!(!detect_unicode_support());
        });
        env_scope(
            &[(ASCII_ONLY_ENV, None), ("LANG", Some("en_US.UTF-8"))],
            || {
                assert!(detect_unicode_support());
            },
        );
    }

    #[test]
    fn width_is_clamped_to_a_usable_range() {
        assert_eq!(clamp_width(0), MIN_USABLE_WIDTH);
        assert_eq!(clamp_width(5), MIN_USABLE_WIDTH);
        assert_eq!(clamp_width(120), 120);
        assert_eq!(clamp_width(10_000), MAX_USABLE_WIDTH);
    }

    #[test]
    fn override_pins_width_without_touching_env() {
        let _guard = test_override_lock();
        set_width_override(Some(57));
        assert_eq!(terminal_width(), 57);
        set_width_override(Some(100_000));
        assert_eq!(terminal_width(), MAX_USABLE_WIDTH);
        set_width_override(None);
    }

    #[test]
    fn fit_width_never_exceeds_detected_width() {
        let _guard = test_override_lock();
        set_width_override(Some(50));
        assert_eq!(fit_width(10), 10);
        assert_eq!(fit_width(500), 50);
        set_width_override(None);
    }

    #[test]
    fn ansi_override_beats_detection() {
        let _guard = test_override_lock();
        let detected = ansi_enabled();
        set_ansi_enabled_override(Some(false));
        assert!(!ansi_enabled());
        set_ansi_enabled_override(Some(true));
        assert!(ansi_enabled());
        set_ansi_enabled_override(None);
        assert_eq!(ansi_enabled(), detected);
    }

    #[test]
    fn unicode_override_beats_detection() {
        let _guard = test_override_lock();
        let detected = unicode_supported();
        set_unicode_override(Some(false));
        assert!(!unicode_supported());
        set_unicode_override(Some(true));
        assert!(unicode_supported());
        set_unicode_override(None);
        assert_eq!(unicode_supported(), detected);
    }
}
