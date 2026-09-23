use anyhow::{Result, anyhow};
use std::io::{self, IsTerminal, Write};
use std::panic;
use std::sync::OnceLock;

use crate::{id, session, telemetry, tui};

pub struct TuiRuntimeState {
    mouse_capture: bool,
    keyboard_enhanced: bool,
    focus_change: bool,
}

const INHERITED_MODES_ENV: &str = "ALPHACODE_TUI_INHERITED_MODES";
const INHERITED_THEME_ENV: &str = "ALPHACODE_TUI_INHERITED_THEME";

#[cfg(any(windows, test))]
const WINDOWS_VT_MOUSE_ENABLE: &[u8] = b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1015h\x1b[?1006h";
#[cfg(any(windows, test))]
const WINDOWS_VT_MOUSE_DISABLE: &[u8] = b"\x1b[?1006l\x1b[?1015l\x1b[?1003l\x1b[?1002l\x1b[?1000l";

static PANIC_HOOK_INSTALLED: OnceLock<()> = OnceLock::new();

#[cfg(windows)]
fn sync_windows_vt_mouse_capture(enabled: bool) -> io::Result<()> {
    let sequence = if enabled {
        WINDOWS_VT_MOUSE_ENABLE
    } else {
        WINDOWS_VT_MOUSE_DISABLE
    };

    let mut stdout = io::stdout().lock();
    stdout.write_all(sequence)?;
    stdout.flush()
}

#[cfg(not(windows))]
fn sync_windows_vt_mouse_capture(_enabled: bool) -> io::Result<()> {
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct InheritedTerminalModes {
    mouse_capture: bool,
    keyboard_enhanced: bool,
    focus_change: bool,
}

impl InheritedTerminalModes {
    fn encode(self) -> String {
        format!(
            "mouse={},keyboard={},focus={}",
            u8::from(self.mouse_capture),
            u8::from(self.keyboard_enhanced),
            u8::from(self.focus_change)
        )
    }

    fn decode(value: &str) -> Option<Self> {
        let mut modes = Self {
            mouse_capture: false,
            keyboard_enhanced: false,
            focus_change: false,
        };
        let mut seen = 0u8;

        for field in value.split(',') {
            let (name, raw) = field.split_once('=')?;
            let (bit, target) = match name {
                "mouse" => (1, &mut modes.mouse_capture),
                "keyboard" => (2, &mut modes.keyboard_enhanced),
                "focus" => (4, &mut modes.focus_change),
                _ => return None,
            };

            if seen & bit != 0 {
                return None;
            }

            *target = match raw {
                "0" => false,
                "1" => true,
                _ => return None,
            };
            seen |= bit;
        }

        (seen == 7).then_some(modes)
    }
}

fn has_terminal_exec_handoff(
    is_resuming: bool,
    inherited_modes: Option<InheritedTerminalModes>,
) -> bool {
    is_resuming && inherited_modes.is_some()
}

pub struct TuiRuntimeGuard {
    state: TuiRuntimeState,
    armed: bool,
}

#[cfg(test)]
thread_local! {
    static GUARD_DROP_RESTORES: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

impl TuiRuntimeGuard {
    fn new(state: TuiRuntimeState) -> Self {
        Self { state, armed: true }
    }

    pub fn finish(mut self, restore_terminal: bool) {
        cleanup_tui_runtime(&self.state, restore_terminal);
        self.armed = false;
    }

    pub fn finish_for_run_result(
        mut self,
        run_result: &crate::alphacode_tui::tui::RunResult,
        extra_exec: bool,
    ) {
        if run_result_will_exec(run_result, extra_exec) {
            export_tui_exec_handoff(&self.state);
        }

        cleanup_tui_runtime_for_run_result(&self.state, run_result, extra_exec);
        self.armed = false;
    }
}

impl Drop for TuiRuntimeGuard {
    fn drop(&mut self) {
        if self.armed {
            cleanup_tui_runtime(&self.state, true);
            self.armed = false;
            #[cfg(test)]
            GUARD_DROP_RESTORES.with(|counter| counter.set(counter.get() + 1));
        }
    }
}

pub fn set_current_session(session_id: &str) {
    crate::set_current_session(session_id);
}

pub fn get_current_session() -> Option<String> {
    crate::get_current_session()
}

fn should_record_panic_as_crash(status: &session::SessionStatus) -> bool {
    matches!(status, session::SessionStatus::Active)
}

fn restore_terminal_after_failed_init() {
    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(
        io::stderr(),
        crossterm::event::DisableBracketedPaste,
        crossterm::event::DisableFocusChange,
        crossterm::event::DisableMouseCapture,
        crossterm::cursor::Show
    );
    let _ = sync_windows_vt_mouse_capture(false);
    crate::alphacode_tui_style::restore_terminal_quietly();
}

pub fn install_panic_hook() {
    if PANIC_HOOK_INSTALLED.set(()).is_err() {
        return;
    }

    let default_hook = panic::take_hook();

    panic::set_hook(Box::new(move |info| {
        restore_terminal_after_failed_init();
        default_hook(info);

        if let Some(session_id) = get_current_session() {
            print_session_resume_hint(&session_id);

            if let Some((provider, model)) = telemetry::current_provider_model() {
                telemetry::record_crash(&provider, &model, telemetry::SessionEndReason::Panic);
            }

            if let Ok(mut session) = session::Session::load(&session_id)
                && should_record_panic_as_crash(&session.status)
            {
                session.mark_crashed(Some(format!("Panic: {}", info)));
                let _ = session.save();
            }
        }
    }));
}

pub fn mark_current_session_crashed(message: String) {
    if let Some(session_id) = get_current_session() {
        if let Some((provider, model)) = telemetry::current_provider_model() {
            telemetry::record_crash(&provider, &model, telemetry::SessionEndReason::Signal);
        }

        if let Ok(mut session) = session::Session::load(&session_id)
            && matches!(session.status, session::SessionStatus::Active)
        {
            session.mark_crashed(Some(message));
            let _ = session.save();
        }
    }
}

pub fn panic_payload_to_string(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

pub fn show_crash_resume_hint() {
    let crashed = session::find_recent_crashed_sessions();
    if crashed.is_empty() {
        return;
    }

    let ansi = crate::console::stderr_supports_ansi();
    let (yellow, bold, reset) = if ansi {
        ("\x1b[33m", "\x1b[1m", "\x1b[0m")
    } else {
        ("", "", "")
    };

    for line in crash_resume_hint_lines(&crashed, yellow, bold, reset) {
        eprintln!("{}", crate::output_style::terminal_text(&line));
    }
    eprintln!();
}

fn crash_resume_hint_lines(
    crashed: &[(String, String)],
    yellow: &str,
    bold: &str,
    reset: &str,
) -> Vec<String> {
    let Some((id, name)) = crashed.first() else {
        return Vec::new();
    };

    let session_label = id::extract_session_name(id).unwrap_or(name.as_str());

    if crashed.len() == 1 {
        vec![
            format!(
                "{yellow}💥 Session {bold}{session_label}{reset}{yellow} crashed. Resume with:{reset}  alphacode --resume {id}"
            ),
            format!("{yellow}   Or browse all:{reset} alphacode --resume"),
        ]
    } else {
        vec![
            format!(
                "{yellow}💥 {} sessions crashed recently. Most recent: {bold}{session_label}{reset}",
                crashed.len()
            ),
            format!("{yellow}   Resume with:{reset}  alphacode --resume {id}"),
            format!("{yellow}   List all:{reset}     alphacode --resume"),
        ]
    }
}

fn init_tui_terminal(inherited_terminal: bool) -> Result<ratatui::DefaultTerminal> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err(anyhow!(
            "alphacode TUI requires an interactive terminal (stdin/stdout must be a TTY)"
        ));
    }

    if inherited_terminal {
        init_tui_terminal_resume()
    } else {
        match panic::catch_unwind(panic::AssertUnwindSafe(ratatui::init)) {
            Ok(terminal) => Ok(terminal),
            Err(payload) => {
                restore_terminal_after_failed_init();
                Err(anyhow!(
                    "failed to initialize terminal: {}",
                    panic_payload_to_string(payload.as_ref())
                ))
            }
        }
    }
}

fn enable_terminal_modes(
    keyboard_enhancement_requested: bool,
    mouse_capture: bool,
    focus_change: bool,
    inherited_terminal: bool,
) -> Result<InheritedTerminalModes> {
    let keyboard_enhanced = if inherited_terminal {
        keyboard_enhancement_requested
    } else if keyboard_enhancement_requested {
        tui::enable_keyboard_enhancement()
    } else {
        false
    };

    crossterm::execute!(io::stdout(), crossterm::event::EnableBracketedPaste)?;

    if focus_change {
        crossterm::execute!(io::stdout(), crossterm::event::EnableFocusChange)?;
    }

    if mouse_capture {
        crossterm::execute!(io::stdout(), crossterm::event::EnableMouseCapture)?;
        if let Err(error) = sync_windows_vt_mouse_capture(true) {
            crate::logging::warn(&format!(
                "failed to enable Windows VT mouse tracking: {error}"
            ));
        }
    }

    Ok(InheritedTerminalModes {
        mouse_capture,
        keyboard_enhanced,
        focus_change,
    })
}

pub fn init_tui_runtime() -> Result<(ratatui::DefaultTerminal, TuiRuntimeGuard)> {
    let is_resuming = std::env::var_os("ALPHACODE_RESUMING").is_some();
    let inherited_theme = std::env::var(INHERITED_THEME_ENV).ok();
    let inherited_modes_raw = std::env::var(INHERITED_MODES_ENV).ok();
    let inherited_modes = inherited_modes_raw
        .as_deref()
        .and_then(InheritedTerminalModes::decode);
    let inherited_terminal = has_terminal_exec_handoff(is_resuming, inherited_modes);

    if inherited_terminal {
        crate::alphacode_tui::tui::theme_detect::init_theme_mode_for_resume(
            inherited_theme.as_deref(),
        );
    } else {
        crate::alphacode_tui::tui::theme_detect::init_theme_mode();
    }

    let terminal = init_tui_terminal(inherited_terminal)?;

    let runtime_setup = (|| -> Result<InheritedTerminalModes> {
        crate::alphacode_tui::tui::mermaid::install_alphacode_mermaid_hooks();
        crate::alphacode_tui::tui::markdown::install_alphacode_markdown_hooks();
        crate::alphacode_tui::tui::mermaid::init_picker();

        let perf_policy = crate::perf::tui_policy();
        let fallback_modes = InheritedTerminalModes {
            mouse_capture: perf_policy.enable_mouse_capture,
            keyboard_enhanced: perf_policy.enable_keyboard_enhancement,
            focus_change: perf_policy.enable_focus_change,
        };

        let modes = inherited_modes.unwrap_or(fallback_modes);
        let modes = enable_terminal_modes(
            modes.keyboard_enhanced,
            modes.mouse_capture,
            modes.focus_change,
            inherited_terminal,
        )?;

        crate::alphacode_core::env::remove_var(INHERITED_MODES_ENV);
        crate::alphacode_core::env::remove_var(INHERITED_THEME_ENV);

        crate::logging::info(&format!(
            "EVENT event=TUI_TERMINAL_MODES phase=initialized pid={} resuming={} handoff={} handoff_raw={} raw_mode={} mouse_capture={} keyboard_enhanced={} focus_change={} idempotent_modes_reasserted={}",
            std::process::id(),
            is_resuming,
            inherited_terminal,
            inherited_modes_raw.as_deref().unwrap_or("none"),
            crossterm::terminal::is_raw_mode_enabled().unwrap_or(false),
            modes.mouse_capture,
            modes.keyboard_enhanced,
            modes.focus_change,
            inherited_terminal,
        ));

        Ok(modes)
    })();

    match runtime_setup {
        Ok(modes) => Ok((
            terminal,
            TuiRuntimeGuard::new(TuiRuntimeState {
                mouse_capture: modes.mouse_capture,
                keyboard_enhanced: modes.keyboard_enhanced,
                focus_change: modes.focus_change,
            }),
        )),
        Err(error) => {
            restore_terminal_after_failed_init();
            Err(error)
        }
    }
}

fn cleanup_tui_runtime(state: &TuiRuntimeState, restore_terminal: bool) {
    crate::logging::info(&format!(
        "EVENT event=TUI_TERMINAL_MODES phase=cleanup pid={} restore_terminal={} raw_mode={} mouse_capture={} keyboard_enhanced={} focus_change={}",
        std::process::id(),
        restore_terminal,
        crossterm::terminal::is_raw_mode_enabled().unwrap_or(false),
        state.mouse_capture,
        state.keyboard_enhanced,
        state.focus_change,
    ));

    crate::alphacode_tui::tui::mermaid::clear_image_state();

    let image_cleanup = crate::alphacode_tui::tui::mermaid::take_terminal_image_cleanup_payload();
    if !image_cleanup.is_empty() {
        let mut stdout = io::stdout().lock();
        let _ = stdout.write_all(image_cleanup.as_bytes());
        let _ = stdout.flush();
    }

    if !restore_terminal {
        return;
    }

    {
        let mut stdout = io::stdout().lock();
        if let Err(error) = crossterm::execute!(
            stdout,
            crossterm::event::DisableBracketedPaste,
            crossterm::cursor::Show
        ) {
            crate::logging::warn(&format!("failed to restore terminal input state: {error}"));
        }
    }

    if state.focus_change
        && let Err(error) = crossterm::execute!(io::stdout(), crossterm::event::DisableFocusChange)
    {
        crate::logging::warn(&format!(
            "failed to disable terminal focus reporting: {error}"
        ));
    }

    if state.mouse_capture {
        if let Err(error) = sync_windows_vt_mouse_capture(false) {
            crate::logging::warn(&format!(
                "failed to disable Windows VT mouse capture: {error}"
            ));
        }

        if let Err(error) = crossterm::execute!(io::stdout(), crossterm::event::DisableMouseCapture)
        {
            crate::logging::warn(&format!("failed to disable mouse capture: {error}"));
        }
    }

    if state.keyboard_enhanced {
        tui::disable_keyboard_enhancement();
    }

    let _ = crossterm::terminal::disable_raw_mode();
    let _ = crossterm::execute!(io::stdout(), crossterm::terminal::LeaveAlternateScreen);
    crate::alphacode_tui_style::restore_terminal_quietly();
}

fn cleanup_tui_runtime_for_run_result(
    state: &TuiRuntimeState,
    run_result: &crate::alphacode_tui::tui::RunResult,
    extra_exec: bool,
) {
    cleanup_tui_runtime(state, !run_result_will_exec(run_result, extra_exec));
}

fn run_result_will_exec(
    run_result: &crate::alphacode_tui::tui::RunResult,
    extra_exec: bool,
) -> bool {
    extra_exec
        || run_result.reload_session.is_some()
        || run_result.rebuild_session.is_some()
        || run_result.update_session.is_some()
        || run_result.restart_session.is_some()
}

fn export_tui_exec_handoff(state: &TuiRuntimeState) {
    let modes = InheritedTerminalModes {
        mouse_capture: state.mouse_capture,
        keyboard_enhanced: state.keyboard_enhanced,
        focus_change: state.focus_change,
    };

    crate::alphacode_core::env::set_var(INHERITED_MODES_ENV, modes.encode());

    let theme = crate::alphacode_tui::tui::theme_detect::current_theme_label();
    crate::alphacode_core::env::set_var(INHERITED_THEME_ENV, theme);

    crate::logging::info(&format!(
        "EVENT event=TUI_TERMINAL_MODES phase=exec_handoff pid={} raw_mode={} modes={} theme={}",
        std::process::id(),
        crossterm::terminal::is_raw_mode_enabled().unwrap_or(false),
        modes.encode(),
        theme,
    ));
}

pub fn print_session_resume_hint(session_id: &str) {
    let _ = write_session_resume_hint(io::stderr().lock(), session_id);
    let _ = write_session_resume_hint(io::stdout().lock(), session_id);
}

fn write_session_resume_hint(mut writer: impl Write, session_id: &str) -> io::Result<()> {
    let session_name = id::extract_session_name(session_id).unwrap_or(session_id);
    let ansi = crate::console::stderr_supports_ansi();
    let (yellow, bold, reset) = if ansi {
        ("\x1b[33m", "\x1b[1m", "\x1b[0m")
    } else {
        ("", "", "")
    };

    writeln!(writer)?;
    writeln!(
        writer,
        "{yellow}Session {bold}{session_name}{reset}{yellow} - to resume:{reset}"
    )?;
    writeln!(writer, "  alphacode --resume {session_id}")?;
    writeln!(writer)?;
    Ok(())
}

fn init_tui_terminal_resume() -> Result<ratatui::DefaultTerminal> {
    use ratatui::{Terminal, backend::CrosstermBackend};

    crossterm::terminal::enable_raw_mode()
        .map_err(|error| anyhow!("failed to enable raw mode on resume: {error}"))?;

    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(error) => {
            let _ = crossterm::terminal::disable_raw_mode();
            return Err(anyhow!("failed to create terminal on resume: {error}"));
        }
    };

    if let Err(error) = terminal.clear() {
        let _ = crossterm::terminal::disable_raw_mode();
        return Err(anyhow!("failed to clear terminal on resume: {error}"));
    }

    Ok(terminal)
}

#[cfg(unix)]
pub fn signal_name(sig: i32) -> &'static str {
    match sig {
        libc::SIGHUP => "SIGHUP",
        libc::SIGINT => "SIGINT",
        libc::SIGQUIT => "SIGQUIT",
        libc::SIGILL => "SIGILL",
        libc::SIGABRT => "SIGABRT",
        libc::SIGKILL => "SIGKILL",
        libc::SIGSEGV => "SIGSEGV",
        libc::SIGPIPE => "SIGPIPE",
        libc::SIGALRM => "SIGALRM",
        libc::SIGTERM => "SIGTERM",
        _ => "unknown",
    }
}

#[cfg(not(unix))]
pub fn signal_name(_sig: i32) -> &'static str {
    "unknown"
}

#[cfg(unix)]
fn signal_crash_reason(sig: i32) -> String {
    match sig {
        libc::SIGHUP => "Terminal or window closed (SIGHUP)".to_string(),
        libc::SIGTERM => "Terminated (SIGTERM)".to_string(),
        libc::SIGINT => "Interrupted (SIGINT)".to_string(),
        libc::SIGQUIT => "Quit signal (SIGQUIT)".to_string(),
        _ => format!("Terminated by signal {} ({})", signal_name(sig), sig),
    }
}

#[cfg(unix)]
fn handle_termination_signal(sig: i32) -> ! {
    mark_current_session_crashed(signal_crash_reason(sig));
    restore_terminal_after_failed_init();

    if let Some(session_id) = get_current_session() {
        print_session_resume_hint(&session_id);
    }

    std::process::exit(128 + sig);
}

#[cfg(unix)]
pub fn spawn_session_signal_watchers() {
    use tokio::signal::unix::{SignalKind, signal};

    fn spawn_one(sig: i32, kind: SignalKind) {
        tokio::spawn(async move {
            let mut stream = match signal(kind) {
                Ok(stream) => stream,
                Err(error) => {
                    crate::logging::error(&format!(
                        "Failed to install {} handler: {}",
                        signal_name(sig),
                        error
                    ));
                    return;
                }
            };

            if stream.recv().await.is_some() {
                crate::logging::info(&format!("Received {} in TUI process", signal_name(sig)));
                handle_termination_signal(sig);
            }
        });
    }

    spawn_one(libc::SIGHUP, SignalKind::hangup());
    spawn_one(libc::SIGTERM, SignalKind::terminate());
    spawn_one(libc::SIGINT, SignalKind::interrupt());
    spawn_one(libc::SIGQUIT, SignalKind::quit());
}

#[cfg(not(unix))]
pub fn spawn_session_signal_watchers() {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_SESSION_LOCK: Mutex<()> = Mutex::new(());

    fn test_guard() -> TuiRuntimeGuard {
        TuiRuntimeGuard::new(TuiRuntimeState {
            mouse_capture: false,
            keyboard_enhanced: false,
            focus_change: false,
        })
    }

    #[test]
    fn inherited_terminal_modes_roundtrip() {
        let modes = InheritedTerminalModes {
            mouse_capture: true,
            keyboard_enhanced: false,
            focus_change: true,
        };

        assert_eq!(InheritedTerminalModes::decode(&modes.encode()), Some(modes));
    }

    #[test]
    fn inherited_terminal_modes_reject_malformed_values() {
        assert_eq!(InheritedTerminalModes::decode("mouse=1,keyboard=1"), None);
        assert_eq!(
            InheritedTerminalModes::decode("mouse=yes,keyboard=1,focus=1"),
            None
        );
        assert_eq!(
            InheritedTerminalModes::decode("mouse=1,mouse=0,keyboard=1,focus=1"),
            None
        );
    }

    #[test]
    fn inherited_terminal_modes_reject_unknown_fields() {
        assert_eq!(
            InheritedTerminalModes::decode("mouse=1,keyboard=1,focus=1,extra=0"),
            None
        );
    }

    #[test]
    fn resume_requires_valid_terminal_handoff_metadata() {
        let modes = InheritedTerminalModes {
            mouse_capture: true,
            keyboard_enhanced: true,
            focus_change: true,
        };

        assert!(has_terminal_exec_handoff(true, Some(modes)));
        assert!(!has_terminal_exec_handoff(true, None));
        assert!(!has_terminal_exec_handoff(false, Some(modes)));
    }

    #[test]
    fn every_exec_action_preserves_terminal_modes() {
        let with = |field: &str| {
            let mut result = crate::alphacode_tui::tui::RunResult::default();
            match field {
                "reload" => result.reload_session = Some("session_test".into()),
                "rebuild" => result.rebuild_session = Some("session_test".into()),
                "update" => result.update_session = Some("session_test".into()),
                "restart" => result.restart_session = Some("session_test".into()),
                _ => unreachable!(),
            }
            result
        };

        for field in ["reload", "rebuild", "update", "restart"] {
            assert!(
                run_result_will_exec(&with(field), false),
                "{field} must preserve terminal modes across exec"
            );
        }

        assert!(run_result_will_exec(
            &crate::alphacode_tui::tui::RunResult::default(),
            true
        ));
        assert!(!run_result_will_exec(
            &crate::alphacode_tui::tui::RunResult::default(),
            false
        ));
    }

    #[test]
    fn guard_drop_restores_terminal_when_not_finished() {
        GUARD_DROP_RESTORES.with(|counter| counter.set(0));

        {
            let _guard = test_guard();
        }

        let restores = GUARD_DROP_RESTORES.with(|counter| counter.get());
        assert_eq!(restores, 1);
    }

    #[test]
    fn guard_finish_disarms_drop_restore() {
        GUARD_DROP_RESTORES.with(|counter| counter.set(0));

        let guard = test_guard();
        guard.finish(true);

        let restores = GUARD_DROP_RESTORES.with(|counter| counter.get());
        assert_eq!(restores, 0);
    }

    #[test]
    fn test_session_recovery_tracking() {
        let _guard = TEST_SESSION_LOCK.lock().unwrap();
        set_current_session("test_session_123");

        let stored = get_current_session();
        assert_eq!(stored.as_deref(), Some("test_session_123"));
    }

    #[test]
    fn test_session_recovery_message_format() {
        let _guard = TEST_SESSION_LOCK.lock().unwrap();
        let test_session = "session_format_test_12345";
        set_current_session(test_session);

        if let Some(session_id) = get_current_session() {
            let mut output = Vec::new();
            write_session_resume_hint(&mut output, &session_id).unwrap();
            let output = String::from_utf8(output).unwrap();
            let expected_cmd = format!("alphacode --resume {}", session_id);
            assert!(output.contains(&expected_cmd));
            assert!(output.contains("to resume"));
            assert!(!session_id.is_empty());
        } else {
            panic!("Session ID should be set");
        }
    }

    #[test]
    fn session_resume_hint_writer_reports_closed_stderr_without_panicking() {
        struct ClosedWriter;

        impl Write for ClosedWriter {
            fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "stderr closed"))
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let error = write_session_resume_hint(ClosedWriter, "session_closed_pipe")
            .expect_err("closed stderr should be reported as an I/O error");
        assert_eq!(error.kind(), io::ErrorKind::BrokenPipe);
    }
}

#[cfg(any(windows, test))]
#[cfg(test)]
mod windows_terminal_tests {
    use super::*;

    #[test]
    fn windows_vt_mouse_modes_enable_and_disable_the_same_tracking_protocols() {
        let enable = String::from_utf8_lossy(WINDOWS_VT_MOUSE_ENABLE);
        let disable = String::from_utf8_lossy(WINDOWS_VT_MOUSE_DISABLE);

        for mode in ["1000", "1002", "1003", "1015", "1006"] {
            assert!(enable.contains(&format!("?{mode}h")));
            assert!(disable.contains(&format!("?{mode}l")));
        }
    }
}

#[cfg(test)]
mod panic_crash_labeling_tests {
    use super::*;

    #[test]
    fn active_session_is_still_labeled_crashed_on_panic() {
        assert!(should_record_panic_as_crash(
            &session::SessionStatus::Active
        ));
    }

    #[test]
    fn already_crashed_session_is_not_relabeled() {
        assert!(!should_record_panic_as_crash(
            &session::SessionStatus::Crashed {
                message: Some("earlier crash".to_string())
            }
        ));
    }

    #[test]
    fn completed_session_is_not_relabeled_by_a_dying_client() {
        for status in [
            session::SessionStatus::Closed,
            session::SessionStatus::Reloaded,
            session::SessionStatus::Compacted,
            session::SessionStatus::RateLimited,
            session::SessionStatus::Error {
                message: "unrelated".to_string(),
            },
        ] {
            assert!(
                !should_record_panic_as_crash(&status),
                "non-active status {status:?} must not be relabeled as crashed"
            );
        }
    }
}
