//! Bounded, reusable proof-of-concept execution harness.
//!
//! A reproduction attempt must never take the research session down with it.
//! Every run here is **bounded**: a wall-clock budget, captured stdout/stderr,
//! an exit code where one exists, and an explicit outcome that distinguishes
//! *the test failed*, *the test could not build*, *the process was killed at
//! the budget*, and *the toolchain was missing*. A timeout in particular is
//! recorded as a timeout — never silently promoted to "the defect is proven".
//! A blocked writer is an observation; whether it means anything depends on
//! what was expected and what the reproduction itself printed (see
//! [`interpret_blocking`]).
//!
//! The harness deliberately runs reproductions in **isolated temporary
//! workspaces** ([`ReproWorkspace`]) so nothing leaks into the reviewed
//! repository, and it kills the child process tree on timeout so a
//! deliberately-blocking test cannot accumulate orphans.
//!
//! Templates ([`go_rwlock_leak_template`], [`rust_blocking_test_template`])
//! exist so the same minimal reproductions are not rebuilt from scratch every
//! session; they embed their own watchdogs so they exit with a result line
//! instead of hanging.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Default wall-clock budget for one reproduction run.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

/// Largest stdout/stderr retained per stream, in bytes. A reproduction only
/// needs enough output to show its result line; capping keeps a runaway print
/// loop from eating memory.
const MAX_CAPTURE_BYTES: usize = 256 * 1024;

/// Language of a standalone reproduction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReproLanguage {
    Go,
    Rust,
    Shell,
}

impl ReproLanguage {
    /// The command prefix used to run the written source in its workspace.
    fn run_command(self, source_path: &Path) -> (String, Vec<String>) {
        match self {
            Self::Go => (
                "go".to_string(),
                vec!["run".into(), source_path.display().to_string()],
            ),
            // `cargo test` needs a manifest; the Rust template is a single
            // file, so it is compiled and run directly.
            Self::Rust => (
                "rustc".to_string(),
                vec![
                    "-O".into(),
                    source_path.display().to_string(),
                    "-o".into(),
                    source_path
                        .with_extension("")
                        .with_file_name("repro_bin")
                        .display()
                        .to_string(),
                ],
            ),
            Self::Shell => {
                if cfg!(windows) {
                    (
                        "cmd".to_string(),
                        vec!["/C".into(), source_path.display().to_string()],
                    )
                } else {
                    ("sh".to_string(), vec![source_path.display().to_string()])
                }
            }
        }
    }

    /// Conventional file name for the source in its workspace.
    pub fn source_file_name(self) -> &'static str {
        match self {
            Self::Go => "main.go",
            Self::Rust => "main.rs",
            Self::Shell => {
                if cfg!(windows) {
                    "repro.cmd"
                } else {
                    "repro.sh"
                }
            }
        }
    }
}

/// How a reproduction run ended. Every run lands in exactly one of these;
/// there is no "probably worked".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReproOutcome {
    /// Completed with exit code 0.
    Passed,
    /// Completed with a non-zero exit code. A *test failure* — this says the
    /// reproduction's assertions did not hold, not that a vulnerability
    /// exists.
    Failed { exit_code: i32 },
    /// Exceeded the time budget and was killed. Evidence of blocking only in
    /// the sense that something did not finish in time; see
    /// [`interpret_blocking`] before drawing conclusions.
    TimedOut,
    /// The process could not be started — missing toolchain, missing binary,
    /// permission denied. Nothing was exercised.
    SpawnError { message: String },
}

impl ReproOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Failed { .. } => "failed",
            Self::TimedOut => "timed_out",
            Self::SpawnError { .. } => "spawn_error",
        }
    }

    pub fn is_completion(&self) -> bool {
        matches!(self, Self::Passed | Self::Failed { .. })
    }
}

/// The full record of one bounded run: outcome, captured output, timing, and
/// the exact command — enough to save as evidence and to re-run.
#[derive(Clone, Debug)]
pub struct ReproResult {
    pub name: String,
    pub outcome: ReproOutcome,
    pub stdout: String,
    pub stderr: String,
    pub elapsed: Duration,
    pub command_line: String,
    /// Workspace the source was written to, when one was created.
    pub workspace: Option<PathBuf>,
}

/// Truncate a captured stream to [`MAX_CAPTURE_BYTES`], noting the truncation
/// so a reader knows output was cut rather than ending where shown.
fn capture_to_string(raw: Vec<u8>) -> String {
    if raw.len() <= MAX_CAPTURE_BYTES {
        return String::from_utf8_lossy(&raw).into_owned();
    }
    let mut out = String::from_utf8_lossy(&raw[..MAX_CAPTURE_BYTES]).into_owned();
    out.push_str(&format!(
        "\n[... {} bytes of output truncated ...]",
        raw.len() - MAX_CAPTURE_BYTES
    ));
    out
}

/// Run one command with a hard wall-clock budget.
///
/// On timeout the child is killed and reaped before returning, so no orphan
/// is left behind. Stdout/stderr are drained on dedicated threads so a child
/// blocked on a full pipe is still reaped correctly.
pub fn run_bounded(
    name: impl Into<String>,
    program: &str,
    args: &[String],
    cwd: Option<&Path>,
    timeout: Duration,
) -> ReproResult {
    let name = name.into();
    let command_line = std::iter::once(program.to_string())
        .chain(args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ");
    let started = Instant::now();

    let mut child = match Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(cwd.unwrap_or_else(|| Path::new(".")))
        .spawn()
    {
        Ok(child) => child,
        Err(err) => {
            return ReproResult {
                name,
                outcome: ReproOutcome::SpawnError {
                    message: err.to_string(),
                },
                stdout: String::new(),
                stderr: String::new(),
                elapsed: started.elapsed(),
                command_line,
                workspace: cwd.map(Path::to_path_buf),
            };
        }
    };

    let stdout_handle = child.stdout.take().map(drain_on_thread);
    let stderr_handle = child.stderr.take().map(drain_on_thread);

    let outcome = wait_with_deadline(&mut child, timeout);
    let stdout = stdout_handle
        .and_then(|h| h.join().ok())
        .map(capture_to_string)
        .unwrap_or_default();
    let stderr = stderr_handle
        .and_then(|h| h.join().ok())
        .map(capture_to_string)
        .unwrap_or_default();

    ReproResult {
        name,
        outcome,
        stdout,
        stderr,
        elapsed: started.elapsed(),
        command_line,
        workspace: cwd.map(Path::to_path_buf),
    }
}

/// Kill a child process and (best-effort) its descendants.
///
/// On Windows, `Child::kill` terminates only the direct process; the child's
/// own children keep running and keep inherited pipe handles open. Routing
/// through `taskkill /T` when available closes the whole tree; the direct
/// kill remains as the fallback when `taskkill` itself is unavailable.
fn kill_process_tree(child: &mut Child) {
    #[cfg(windows)]
    {
        use std::process::{Command, Stdio};
        let pid = child.id();
        let _ = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    let _ = child.kill();
}

/// Drain a pipe on its own thread so the child never deadlocks on a full
/// pipe buffer, and so both streams are fully read even after a kill.
fn drain_on_thread<R: Read + Send + 'static>(mut pipe: R) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        // Reads stop at MAX_CAPTURE_BYTES * 2 so a pathological child cannot
        // exhaust memory before the deadline fires; the excess is discarded
        // and `capture_to_string` marks truncation.
        let mut limited = (&mut pipe).take((MAX_CAPTURE_BYTES * 2) as u64);
        let _ = limited.read_to_end(&mut buf);
        buf
    })
}

/// Wait for the child, killing it (and reaping it) if the deadline passes.
fn wait_with_deadline(child: &mut Child, timeout: Duration) -> ReproOutcome {
    use wait_timeout::ChildExt;

    match child.wait_timeout(timeout) {
        Ok(Some(status)) => match status.code() {
            Some(0) => ReproOutcome::Passed,
            Some(code) => ReproOutcome::Failed { exit_code: code },
            // Killed by a signal on Unix; on Windows this is rare but a
            // non-zero outcome either way.
            None => ReproOutcome::Failed { exit_code: -1 },
        },
        Ok(None) => {
            // Best-effort kill of the direct child and, on Windows, its
            // whole process tree. Killing only `cmd.exe` would leave its
            // children (e.g. the blocking command it spawned) alive, holding
            // the inherited pipes open — which keeps this function blocked
            // in the output join long after the budget elapsed, the exact
            // "timeout hangs the session" failure this harness exists to
            // prevent.
            kill_process_tree(child);
            let _ = child.wait();
            ReproOutcome::TimedOut
        }
        Err(err) => ReproOutcome::SpawnError {
            message: format!("wait failed: {err}"),
        },
    }
}

/// An isolated temporary workspace for one reproduction attempt.
///
/// The source written here is saved as evidence alongside the result, so the
/// exact bytes that were executed survive the session.
pub struct ReproWorkspace {
    dir: tempfile::TempDir,
}

impl ReproWorkspace {
    /// Create an isolated workspace under the system temp directory.
    pub fn create(label: &str) -> std::io::Result<Self> {
        let safe: String = label
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect();
        let dir = tempfile::Builder::new()
            .prefix(&format!("alphacode-repro-{safe}-"))
            .tempdir()?;
        Ok(Self { dir })
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Write the reproduction source and return its path. The directory is
    /// removed when the workspace drops; callers that need the source to
    /// survive must copy it out (see [`ReproResult`]'s evidence export
    /// helpers in the research-session ledger).
    pub fn write_source(&self, language: ReproLanguage, source: &str) -> std::io::Result<PathBuf> {
        let path = self.dir.path().join(language.source_file_name());
        std::fs::write(&path, source)?;
        // Verify the write persisted — an unverified write must not be
        // reported as the executed source.
        let readback = std::fs::read_to_string(&path)?;
        if readback != source {
            return Err(std::io::Error::other(
                "reproduction source read back differently than written",
            ));
        }
        Ok(path)
    }

    /// Write source and run it, bounded by `timeout`.
    pub fn run(
        &self,
        name: impl Into<String>,
        language: ReproLanguage,
        source: &str,
        timeout: Duration,
    ) -> ReproResult {
        let name = name.into();
        let path = match self.write_source(language, source) {
            Ok(path) => path,
            Err(err) => {
                return ReproResult {
                    name,
                    outcome: ReproOutcome::SpawnError {
                        message: format!("could not persist reproduction source: {err}"),
                    },
                    stdout: String::new(),
                    stderr: String::new(),
                    elapsed: Duration::ZERO,
                    command_line: String::new(),
                    workspace: Some(self.dir.path().to_path_buf()),
                };
            }
        };
        let (program, args) = language.run_command(&path);
        let mut result = run_bounded(name, &program, &args, Some(self.dir.path()), timeout);
        result.workspace = Some(self.dir.path().to_path_buf());
        result
    }
}

/// What a timeout (or completion) means for a hypothesis about blocking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockingObservation {
    /// The reproduction expected blocking and the budget elapsed first.
    BlockedAsExpected,
    /// The reproduction expected blocking but the process completed anyway —
    /// the hypothesis is weakened or refuted.
    CompletedInstead,
    /// Nothing can be concluded: the run never started.
    Inconclusive,
}

/// Interpret a run against the hypothesis it was testing.
///
/// A timeout is **not** proof of a vulnerability. It proves only that the
/// bounded run did not finish; whether that matches the expected condition
/// depends on the reproduction's own contract, which the caller supplies as
/// `expected_blocking` together with the reproduction's result line (if it
/// printed one). The returned narrative is meant to be recorded verbatim in
/// the finding's evidence.
pub fn interpret_blocking(
    result: &ReproResult,
    expected_blocking: bool,
) -> (BlockingObservation, String) {
    match (&result.outcome, expected_blocking) {
        (ReproOutcome::TimedOut, true) => (
            BlockingObservation::BlockedAsExpected,
            format!(
                "'{}' did not complete within {:?} (killed at the budget). \
                 The reproduction expected a blocked operation, so the observation \
                 is consistent with the hypothesis — but a timeout alone establishes \
                 only that the run did not finish; the finding must name exactly \
                 which operation was blocked.",
                result.name, result.elapsed
            ),
        ),
        (ReproOutcome::TimedOut, false) => (
            BlockingObservation::Inconclusive,
            format!(
                "'{}' timed out after {:?}, but this reproduction was not \
                 expected to block. Treat as an environment or budget problem, \
                 not as evidence for the hypothesis.",
                result.name, result.elapsed
            ),
        ),
        (outcome, true) if outcome.is_completion() => (
            BlockingObservation::CompletedInstead,
            format!(
                "'{}' completed ({}) within {:?}, so the expected blocking did \
                 not occur under these conditions. The hypothesis is weakened; \
                 record the preconditions that differ before claiming otherwise.",
                result.name,
                outcome.as_str(),
                result.elapsed
            ),
        ),
        (ReproOutcome::SpawnError { message }, _) => (
            BlockingObservation::Inconclusive,
            format!(
                "'{}' never started: {message}. Nothing was exercised; do not \
                 report this as either supporting or refuting evidence.",
                result.name
            ),
        ),
        (outcome, false) => (
            BlockingObservation::Inconclusive,
            format!(
                "'{}' ended as {} after {:?}; this run tested a non-blocking \
                 expectation, so interpret via the exit code and output.",
                result.name,
                outcome.as_str(),
                result.elapsed
            ),
        ),
        // A failing completion while blocking was expected: the run finished
        // badly, which says more about the reproduction itself than about the
        // hypothesis. Never read this as a pass, and never as proof.
        (outcome, true) => (
            BlockingObservation::Inconclusive,
            format!(
                "'{}' completed as {} after {:?} while blocking was expected. A \
                 failing run may mean the reproduction itself broke (compile error, \
                 bad assertion); inspect the captured output before drawing any \
                 conclusion. Do not report a failed run as a pass.",
                result.name,
                outcome.as_str(),
                result.elapsed
            ),
        ),
    }
}

/// Minimal standalone reproduction of a leaked `RWMutex` read lock: `Sign`
/// takes `RLock()` and returns without `RUnlock()`, after which a legitimate
/// writer blocks. The embedded watchdog makes the program print a result line
/// and exit either way — a reproduction must never hang the session.
pub fn go_rwlock_leak_template() -> String {
    r#"package main

import (
	"fmt"
	"sync"
	"time"
)

// Service mirrors the shape under review: an RWMutex guarding cached state.
type Service struct {
	mu     sync.RWMutex
	loaded string
}

// Sign acquires the read lock and returns without releasing it — the
// suspected defect, reproduced in isolation.
func (s *Service) Sign() string {
	s.mu.RLock()
	return s.loaded // leaked RLock: no RUnlock on this path
}

func main() {
	svc := &Service{loaded: "key-material"}
	svc.Sign() // returns; the read lock is still held

	// Watchdog: a legitimate writer either acquires (hypothesis refuted) or
	// stays blocked until the budget elapses (blocking reproduced). Either
	// way the program exits with a result line.
	done := make(chan struct{})
	go func() {
		svc.mu.Lock()
		svc.mu.Unlock()
		close(done)
	}()

	select {
	case <-done:
		fmt.Println("RESULT: writer proceeded; no lock leak reproduced")
	case <-time.After(2 * time.Second):
		fmt.Println("RESULT: writer blocked for 2s after Sign returned; leaked RLock reproduced in isolation")
	}
}
"#
    .to_string()
}

/// The corrected variant of [`go_rwlock_leak_template`]: same shape, unlock
/// on every path via `defer`. Used as the regression case for the fix.
///
/// Written as its own literal rather than derived from the leak template by
/// string replacement, so it stays correct regardless of the leak variant's
/// exact comment wording or line endings.
pub fn go_rwlock_fixed_template() -> String {
    r#"package main

import (
	"fmt"
	"sync"
	"time"
)

// Service mirrors the shape under review: an RWMutex guarding cached state.
type Service struct {
	mu     sync.RWMutex
	loaded string
}

// Sign acquires the read lock and releases it on every path via defer —
// the corrected implementation.
func (s *Service) Sign() string {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.loaded
}

func main() {
	svc := &Service{loaded: "key-material"}
	svc.Sign() // returns; the read lock is released

	// Watchdog: a legitimate writer either acquires (fix works) or stays
	// blocked until the budget elapses (leak still present). Either way the
	// program exits with a result line.
	done := make(chan struct{})
	go func() {
		svc.mu.Lock()
		svc.mu.Unlock()
		close(done)
	}()

	select {
	case <-done:
		fmt.Println("RESULT: writer proceeded; no lock leak reproduced")
	case <-time.After(2 * time.Second):
		fmt.Println("RESULT: writer still blocked 2s after Sign returned; blocking persists (fix ineffective)")
	}
}
"#
    .to_string()
}

/// A Rust test demonstrating the same blocking shape (a guard held past a
/// return blocks the next locker) with a bounded wait so the test cannot
/// hang the runner. Drop the `_leaked` binding into an inner scope to model
/// the corrected implementation.
pub fn rust_blocking_test_template() -> String {
    r#"
#[test]
fn leaked_guard_blocks_subsequent_locker() {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    let m = Arc::new(Mutex::new(()));
    let m2 = Arc::clone(&m);

    // The "defect": the guard is intentionally held past this scope's
    // natural end by binding it at test scope.
    let _leaked = m.lock().unwrap();

    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _g = m2.lock().unwrap();
        let _ = tx.send(());
    });

    // Bounded wait: the test asserts the *expected blocking condition*
    // instead of hanging when the guard is held.
    let got = rx.recv_timeout(Duration::from_secs(2));
    assert!(
        got.is_err(),
        "a leaked guard should block the second locker within the 2s budget"
    );
}
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quick_command() -> (String, Vec<String>) {
        if cfg!(windows) {
            ("cmd".to_string(), vec!["/C".into(), "echo ok".into()])
        } else {
            ("sh".to_string(), vec!["-c".into(), "echo ok".into()])
        }
    }

    fn blocking_command(secs: u64) -> (String, Vec<String>) {
        if cfg!(windows) {
            (
                "cmd".to_string(),
                vec!["/C".into(), format!("ping -n {secs} 127.0.0.1 >nul")],
            )
        } else {
            ("sh".to_string(), vec!["-c".into(), format!("sleep {secs}")])
        }
    }

    fn failing_command() -> (String, Vec<String>) {
        if cfg!(windows) {
            ("cmd".to_string(), vec!["/C".into(), "exit 7".into()])
        } else {
            ("sh".to_string(), vec!["-c".into(), "exit 7".into()])
        }
    }

    #[test]
    fn a_completing_command_reports_passed_with_output() {
        let (prog, args) = quick_command();
        let result = run_bounded("quick", &prog, &args, None, Duration::from_secs(30));
        assert_eq!(result.outcome, ReproOutcome::Passed, "{result:?}");
        assert!(result.stdout.contains("ok"), "stdout: {}", result.stdout);
        assert!(result.elapsed < Duration::from_secs(25));
    }

    #[test]
    fn a_failing_command_reports_its_exit_code() {
        let (prog, args) = failing_command();
        let result = run_bounded("failing", &prog, &args, None, Duration::from_secs(30));
        assert_eq!(
            result.outcome,
            ReproOutcome::Failed { exit_code: 7 },
            "{result:?}"
        );
    }

    /// A deliberately blocking test must terminate within its budget, not
    /// hang the session.
    #[test]
    fn a_blocking_command_is_killed_at_the_budget() {
        let (prog, args) = blocking_command(60);
        let result = run_bounded("blocker", &prog, &args, None, Duration::from_millis(1_500));
        assert_eq!(result.outcome, ReproOutcome::TimedOut, "{result:?}");
        assert!(
            result.elapsed < Duration::from_secs(20),
            "kill must be prompt; took {:?}",
            result.elapsed
        );
    }

    #[test]
    fn a_missing_binary_is_a_spawn_error_not_a_failure() {
        let result = run_bounded(
            "missing",
            "definitely-not-a-real-binary-42",
            &[],
            None,
            Duration::from_secs(5),
        );
        assert!(matches!(result.outcome, ReproOutcome::SpawnError { .. }));
    }

    #[test]
    fn workspace_runs_write_and_cleanup() {
        let ws = ReproWorkspace::create("unit").expect("temp dir");
        let ws_path = ws.path().to_path_buf();
        let (prog, args) = quick_command();
        // Run the platform shell as the "language" would be run, via the
        // generic bounded path, then let the workspace drop.
        let _ = run_bounded(
            "in-workspace",
            &prog,
            &args,
            Some(ws_path.as_path()),
            Duration::from_secs(30),
        );
        assert!(ws_path.exists());
        drop(ws);
        assert!(!ws_path.exists(), "temp workspace must be cleaned up");
    }

    #[test]
    fn workspace_write_source_round_trips() {
        let ws = ReproWorkspace::create("roundtrip").expect("temp dir");
        let path = ws
            .write_source(ReproLanguage::Shell, "echo hello")
            .expect("write");
        let read = std::fs::read_to_string(path).expect("read back");
        assert_eq!(read, "echo hello");
    }

    #[test]
    fn timeout_interpretation_never_proves_a_vuln_by_itself() {
        let blocked = ReproResult {
            name: "rwmutex".into(),
            outcome: ReproOutcome::TimedOut,
            stdout: String::new(),
            stderr: String::new(),
            elapsed: Duration::from_secs(2),
            command_line: "go run main.go".into(),
            workspace: None,
        };
        let (obs, narrative) = interpret_blocking(&blocked, true);
        assert_eq!(obs, BlockingObservation::BlockedAsExpected);
        assert!(
            narrative.contains("timeout alone establishes"),
            "the narrative must state the limit of a timeout: {narrative}"
        );

        // Expected blocking but the process completed: hypothesis weakened.
        let completed = ReproResult {
            outcome: ReproOutcome::Passed,
            ..blocked.clone()
        };
        let (obs, narrative) = interpret_blocking(&completed, true);
        assert_eq!(obs, BlockingObservation::CompletedInstead);
        assert!(narrative.contains("weakened"), "{narrative}");

        // A spawn error is never evidence in either direction.
        let spawn_err = ReproResult {
            outcome: ReproOutcome::SpawnError {
                message: "no go".into(),
            },
            ..blocked
        };
        let (obs, _) = interpret_blocking(&spawn_err, true);
        assert_eq!(obs, BlockingObservation::Inconclusive);
    }

    #[test]
    fn the_fixed_go_template_actually_contains_the_defer() {
        let fixed = go_rwlock_fixed_template();
        assert!(fixed.contains("defer s.mu.RUnlock()"), "{fixed}");
        assert!(!fixed.contains("leaked RLock"), "{fixed}");
        let leaked = go_rwlock_leak_template();
        assert!(leaked.contains("s.mu.RLock()"));
        assert!(!leaked.contains("RUnlock()"));
    }

    /// When a Go toolchain is available, the leak template must reproduce
    /// blocking within its watchdog, and the fixed template must not. Skipped
    /// (not failed) when `go` is absent so the suite stays portable.
    #[test]
    fn go_templates_behave_as_documented_when_go_is_available() {
        if run_bounded(
            "probe",
            "go",
            &["version".to_string()],
            None,
            Duration::from_secs(15),
        )
        .outcome
            != ReproOutcome::Passed
        {
            eprintln!("skipping: no Go toolchain available");
            return;
        }
        let ws = ReproWorkspace::create("go-rwmutex").expect("temp dir");
        let timeout = Duration::from_secs(30);

        let leak = ws.run(
            "leak",
            ReproLanguage::Go,
            &go_rwlock_leak_template(),
            timeout,
        );
        assert!(
            leak.stdout.contains("leaked RLock reproduced"),
            "leak template should reproduce blocking: {leak:?}"
        );

        let fixed = ws.run(
            "fixed",
            ReproLanguage::Go,
            &go_rwlock_fixed_template(),
            timeout,
        );
        assert_eq!(fixed.outcome, ReproOutcome::Passed, "{fixed:?}");
        assert!(
            fixed.stdout.contains("writer proceeded"),
            "fixed template must not block: {fixed:?}"
        );
    }
}
