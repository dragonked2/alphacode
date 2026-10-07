//! Shared helpers for the external recon tools.
//!
//! Every `subfinder`/`httpx`/`katana`/`dnsx`/`ffuf`/`gau`/`waybackurls`
//! wrapper has the same three problems, and solving them once here is both
//! smaller and more reliable than fixing them seven times:
//!
//! 1. **Unbounded execution.** A bare `Command::output().await` has no
//!    deadline and no `kill_on_drop`, so a tool that wedges on a network stall
//!    pins the agent turn forever. [`run_bounded`] applies a hard timeout and
//!    kills the child on expiry.
//! 2. **Unbounded output.** `.output()` buffers all of stdout *and* stderr in
//!    memory before anything is parsed. `gau` against a large domain routinely
//!    produces 10^5-10^6 URLs, and ffuf against a large wordlist more.
//!    [`run_bounded`] now caps captured process output, while [`parse_lines`]
//!    applies the smaller user-facing result cap.
//! 3. **Wrong flag names silently fatal.** These are all `goflags` or stdlib
//!    `flag` programs: an unrecognised flag makes them print usage and exit 2.
//!    Crucially, goflags directs flag errors to **stdout**, not stderr, so
//!    reporting only stderr produces the useless message
//!    `katana exited with error: ` with nothing after it.
//!    [`describe_failure`] looks at both streams.
//!
//! Plus input validation ([`validate_hostname`], [`validate_target`]): a domain
//! beginning with `-` is parsed as a flag by every one of these tools, so
//! `{"domain": "-o"}` silently changes behaviour instead of erroring.

use std::time::Duration;

use anyhow::Result;
use std::path::Path;

/// Default wall-clock budget for a single recon invocation.
///
/// Generous, because `subfinder -all` and `gau` legitimately talk to dozens of
/// passive sources, but finite — the point is that a *hang* must surface as an
/// error rather than never returning.
pub const DEFAULT_TOOL_TIMEOUT: Duration = Duration::from_secs(300);

/// Cap on retained output lines. Beyond this we truncate and say so, rather
/// than silently handing the model a truncated list it may treat as complete.
pub const MAX_OUTPUT_LINES: usize = 5_000;

/// Cap on retained output bytes, applied in addition to [`MAX_OUTPUT_LINES`].
pub const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;

/// Hard per-stream process capture limit. Parsing retains only a small fraction
/// of this, but allowing some headroom avoids rejecting ordinary large scans.
const MAX_CAPTURE_BYTES: usize = 16 * 1024 * 1024;

/// Add Alphacode identification to scanners that accept repeated `-H` request
/// headers. Preserve an explicitly supplied User-Agent.
pub fn append_default_user_agent_header(args: &mut Vec<String>, headers: &[String]) {
    let has_user_agent = headers.iter().any(|header| {
        header
            .split_once(':')
            .is_some_and(|(name, _)| name.trim().eq_ignore_ascii_case("user-agent"))
    });
    if !has_user_agent {
        args.push("-H".to_string());
        args.push(format!(
            "User-Agent: {}",
            crate::alphacode_provider_core::ALPHACODE_USER_AGENT
        ));
    }
}

/// Run an external tool with a hard deadline, killing the child on expiry.
///
/// Returns the captured output, or a human-readable error naming the tool and
/// the timeout. Never hangs, and never leaves an orphan process behind.
pub async fn run_bounded(
    program: &str,
    args: &[String],
    timeout: Duration,
) -> Result<std::process::Output, String> {
    run_bounded_in(program, args, None, timeout).await
}

/// Variant of [`run_bounded`] that runs the child from a chosen working
/// directory. The same timeout, cancellation, and per-stream output caps apply.
pub async fn run_bounded_in(
    program: &str,
    args: &[String],
    working_dir: Option<&Path>,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let child = spawn_recon_tool(program, args, working_dir).await?;
    wait_bounded(child, program, timeout).await
}

/// Spawn a known recon program, automatically setting up a missing Go tool
/// once and retrying. Only names with a Go install plan are eligible; arbitrary
/// command arguments and non-Go tools never trigger an install.
async fn spawn_recon_tool(
    program: &str,
    args: &[String],
    working_dir: Option<&Path>,
) -> Result<tokio::process::Child, String> {
    let mut cmd = recon_command(program, args, working_dir);
    match cmd.spawn() {
        Ok(child) => Ok(child),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let Some(step) = automatic_go_install_plan(program) else {
                return Err(format!("failed to run {program}: {error}"));
            };

            let outcome = crate::alphacode_app_core::bugbounty_install::install_one(&step).await;
            match outcome {
                crate::alphacode_app_core::bugbounty_install::InstallOutcome::Installed {
                    ..
                }
                | crate::alphacode_app_core::bugbounty_install::InstallOutcome::AlreadyPresent {
                    ..
                } => {
                    let mut retry = recon_command(program, args, working_dir);
                    retry.spawn().map_err(|retry_error| {
                        format!(
                            "Could not launch {program} after successful setup: {retry_error}. Check `alphacode bugbounty doctor {program}`."
                        )
                    })
                }
                outcome => Err(format!(
                    "Automatic setup for `{program}` did not complete: {}. Retry with `alphacode bugbounty install {program}`.",
                    crate::alphacode_app_core::bugbounty_install::install_failure_description(
                        &outcome
                    )
                )),
            }
        }
        Err(error) => Err(format!("failed to run {program}: {error}")),
    }
}

fn automatic_go_install_plan(
    program: &str,
) -> Option<crate::alphacode_app_core::bugbounty_install::InstallStep> {
    crate::alphacode_app_core::bugbounty_install::plan_for(program).filter(|step| {
        step.family == crate::alphacode_app_core::bugbounty_install::InstallFamily::Go
    })
}

fn recon_command(
    program: &str,
    args: &[String],
    working_dir: Option<&Path>,
) -> tokio::process::Command {
    // Prefer the Go build over a same-named executable on PATH (notably the
    // Python `httpx` CLI), and find Go tools in the default Go bin directory.
    let mut cmd = tokio::process::Command::new(resolve_program(program));
    cmd.args(args)
        // Without this, dropping the `Child` on timeout detaches the process
        // instead of killing it and it keeps running in the background.
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(working_dir) = working_dir {
        cmd.current_dir(working_dir);
    }
    cmd
}

/// Run a configured command with the same timeout and output guarantees as
/// [`run_bounded`]. This lets tools keep command-specific environment and
/// working-directory setup without reintroducing unbounded `output()` calls.
pub async fn run_command_bounded(
    mut cmd: tokio::process::Command,
    program: &str,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    cmd.kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let child = cmd
        .spawn()
        .map_err(|error| format!("failed to run {program}: {error}"))?;
    wait_bounded(child, program, timeout).await
}

/// Collect a spawned child's output with a deadline and per-stream byte cap.
/// The child must have piped stdout/stderr and `kill_on_drop` enabled.
pub async fn wait_bounded(
    mut child: tokio::process::Child,
    program: &str,
    timeout: Duration,
) -> Result<std::process::Output, String> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| format!("failed to capture {program} stdout"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| format!("failed to capture {program} stderr"))?;
    let (overflow_tx, mut overflow_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut stdout_task = tokio::spawn(capture_stream(stdout, overflow_tx.clone()));
    let mut stderr_task = tokio::spawn(capture_stream(stderr, overflow_tx));

    let wait_for_output = async {
        let status = tokio::select! {
            status = child.wait() => status
                .map_err(|error| format!("failed waiting for {program}: {error}"))?,
            Some(()) = overflow_rx.recv() => {
                return Err(output_limit_error(program));
            }
        };
        let stdout = tokio::select! {
            result = &mut stdout_task => result
                .map_err(|error| format!("failed reading {program} stdout: {error}"))?
                .map_err(|error| format!("failed reading {program} stdout: {error}"))?,
            Some(()) = overflow_rx.recv() => return Err(output_limit_error(program)),
        };
        let stderr = tokio::select! {
            result = &mut stderr_task => result
                .map_err(|error| format!("failed reading {program} stderr: {error}"))?
                .map_err(|error| format!("failed reading {program} stderr: {error}"))?,
            Some(()) = overflow_rx.recv() => return Err(output_limit_error(program)),
        };
        Ok::<_, String>((status, stdout, stderr))
    };

    let (status, (stdout, stdout_truncated), (stderr, stderr_truncated)) =
        match tokio::time::timeout(timeout, wait_for_output).await {
            Err(_) => {
                // The wait future was cancelled by the timeout. Kill the child
                // before returning so it cannot outlive the tool.
                let _ = child.kill().await;
                stdout_task.abort();
                stderr_task.abort();
                return Err(format!(
                    "{program} timed out after {}s and was terminated. \
                     Narrow the target or raise the tool's own timeout.",
                    timeout.as_secs()
                ));
            }
            Ok(Err(error)) => {
                let _ = child.kill().await;
                stdout_task.abort();
                stderr_task.abort();
                return Err(error);
            }
            Ok(Ok(output)) => output,
        };

    debug_assert!(!stdout_truncated && !stderr_truncated);

    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

async fn capture_stream<R>(
    mut stream: R,
    overflow_tx: tokio::sync::mpsc::UnboundedSender<()>,
) -> std::io::Result<(Vec<u8>, bool)>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    use tokio::io::AsyncReadExt as _;

    let mut captured = Vec::with_capacity(MAX_CAPTURE_BYTES.min(64 * 1024));
    let mut truncated = false;
    let mut chunk = [0u8; 16 * 1024];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            break;
        }
        let remaining = MAX_CAPTURE_BYTES.saturating_sub(captured.len());
        let keep = read.min(remaining);
        captured.extend_from_slice(&chunk[..keep]);
        if keep < read && !truncated {
            truncated = true;
            let _ = overflow_tx.send(());
        }
    }
    Ok((captured, truncated))
}

fn output_limit_error(program: &str) -> String {
    format!(
        "{program} produced more than {} MiB of output; it was stopped and partial results were discarded to protect memory and avoid treating incomplete results as complete. Narrow the target or filter the output, then retry.",
        MAX_CAPTURE_BYTES / (1024 * 1024)
    )
}

/// Build the error text for a non-zero exit.
///
/// goflags (subfinder, httpx, katana, dnsx, gau) writes flag-parse errors to
/// **stdout**; stdlib `flag` (waybackurls) writes them to stderr. Checking only
/// stderr therefore loses the entire diagnostic for most of these tools, and
/// the model is left with an empty message and a retry-suppression counter
/// ticking up. Prefer stderr, fall back to stdout, and include both when both
/// say something useful.
pub fn describe_failure(tool: &str, output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = stderr.trim();
    let stdout = stdout.trim();

    let detail = if !stderr.is_empty() {
        // Flag errors are short; prefer stderr but drop a duplicated stdout
        // banner rather than repeating the whole usage block twice.
        crate::alphacode_core::util::truncate_str(stderr, 800).to_string()
    } else if !stdout.is_empty() {
        crate::alphacode_core::util::truncate_str(stdout, 800).to_string()
    } else {
        "no diagnostic output".to_string()
    };

    format!("{tool} exited with {}: {detail}", output.status)
}

/// Reject a hostname that would be misparsed as a CLI flag or split into
/// multiple arguments.
///
/// All of these tools use Go's `flag` package, which:
/// * treats any argument starting with `-` as a flag, so a bare positional
///   domain of `-t` silently becomes a flag *value* or a mode switch;
/// * stops parsing at `--`, which is why a leading dash is otherwise
///   unrecoverable for positional arguments.
///
/// We reject rather than escape, because a "hostname" containing whitespace or
/// a newline is never legitimate and silently rewriting it would hide a caller
/// bug that produces bogus recon results.
pub fn validate_hostname(domain: &str) -> Result<String> {
    let d = domain.trim();
    if d.is_empty() {
        return Err(anyhow::anyhow!("domain must not be empty"));
    }
    if d.starts_with('-') {
        return Err(anyhow::anyhow!(
            "invalid domain `{d}`: it starts with `-`, which the target tool would parse as a flag"
        ));
    }
    if d.contains(char::is_whitespace) {
        return Err(anyhow::anyhow!(
            "invalid domain `{d}`: it contains whitespace, which the target tool would split into multiple arguments"
        ));
    }
    if d.contains('\0') {
        return Err(anyhow::anyhow!("invalid domain: contains a NUL byte"));
    }
    // A hostname is labels of alphanumerics/hyphens, or a wildcard/concatenation
    // for bruteforce. Allow dots, hyphens, underscores, and a leading `*.`.
    if !d
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '*'))
    {
        return Err(anyhow::anyhow!(
            "invalid domain `{d}`: only letters, digits, `.`, `-`, `_` and `*` are allowed"
        ));
    }
    if d.contains("..") {
        return Err(anyhow::anyhow!(
            "invalid domain `{d}`: contains an empty label"
        ));
    }
    Ok(d.to_string())
}

/// Validate a URL or host target for a recon tool.
///
/// Less strict than [`validate_hostname`] because a scheme, port, path and
/// query string are all legitimate, but the flag-injection and whitespace
/// rules still apply.
pub fn validate_target(target: &str) -> Result<String> {
    let t = target.trim();
    if t.is_empty() {
        return Err(anyhow::anyhow!("target must not be empty"));
    }
    if t.starts_with('-') {
        return Err(anyhow::anyhow!(
            "invalid target `{t}`: it starts with `-`, which the target tool would parse as a flag"
        ));
    }
    if t.contains(char::is_whitespace) {
        return Err(anyhow::anyhow!(
            "invalid target `{t}`: it contains whitespace, which the target tool would split into multiple arguments"
        ));
    }
    if t.contains('\0') {
        return Err(anyhow::anyhow!("invalid target: contains a NUL byte"));
    }
    if !t.contains('.') && !t.contains(':') {
        return Err(anyhow::anyhow!(
            "invalid target `{t}`: expected a hostname or URL containing a dot"
        ));
    }
    Ok(t.to_string())
}

/// A path that already exists on disk, validated so it cannot smuggle in
/// option-looking content. Used for wordlists and `-list` inputs.
pub fn validate_file_arg(path: &str, what: &str) -> Result<String> {
    let p = path.trim();
    if p.is_empty() {
        return Err(anyhow::anyhow!("{what} must not be empty"));
    }
    if p.contains('\0') {
        return Err(anyhow::anyhow!("{what} contains a NUL byte"));
    }
    Ok(p.to_string())
}

/// Split tool output into cleaned, capped lines.
///
/// * drops blank lines and the bare `-` stdin sentinel, which several of these
///   tools echo back and which would otherwise appear as a "result" of `-`
/// * caps at [`MAX_OUTPUT_LINES`] and [`MAX_OUTPUT_BYTES`]
/// * reports the true total so a truncated result is never mistaken for a
///   complete one — silently dropping results is how recon misses endpoints
///
/// `str::lines` already strips the `\r` of a `\r\n` pair, so CRLF output from
/// a Windows-built binary needs no special handling.
pub fn parse_lines(raw: &[u8]) -> (Vec<String>, usize, bool) {
    let text = String::from_utf8_lossy(raw);
    let mut lines: Vec<String> = Vec::new();
    let mut total = 0usize;
    let mut truncated = false;
    let mut bytes = 0usize;

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line == "-" {
            continue;
        }
        total += 1;
        if lines.len() >= MAX_OUTPUT_LINES || bytes.saturating_add(line.len()) > MAX_OUTPUT_BYTES {
            truncated = true;
            continue;
        }
        bytes += line.len();
        lines.push(line.to_string());
    }
    (lines, total, truncated)
}

/// Render the truncation notice, if any, so truncation is explicit.
pub fn truncation_notice(shown: usize, total: usize) -> String {
    if shown >= total {
        return String::new();
    }
    format!(
        "\n[showing {shown} of {total} results — output was capped; narrow the target or use a filter for complete results]"
    )
}

/// Actionable install hint for a recon binary.
///
/// Delegated to the installer module so there is one source of truth for how
/// each tool is obtained. This hint is used if automatic setup does not work.
pub fn install_hint(binary: &str) -> String {
    crate::alphacode_app_core::bugbounty_install::install_hint_for(binary)
}

/// Resolve the program to run for a recon binary, preferring a Go-installed
/// one over whatever `PATH` resolves first.
///
/// `httpx` is the motivating case: the Python HTTP client installs a CLI of
/// exactly the same name, and whichever comes first on `PATH` wins. On a
/// machine with both installed, the bare name `httpx` runs the Python client,
/// which exits 0 for `httpx <url>` and prints `Usage: httpx [OPTIONS] URL` —
/// so its output was previously accepted as if it were a scan.
///
/// The Go toolchain writes to `~/go/bin`, which is not on `PATH` by default on
/// Windows, so the Go install is frequently present *and* unreachable by name.
/// Consult that directory explicitly before falling back to `PATH`.
///
/// Returns the bare name when nothing better can be found, so the caller's
/// error message still names the binary the user has to fix.
pub fn resolve_program(binary: &str) -> String {
    let exe = if cfg!(windows) {
        format!("{binary}.exe")
    } else {
        binary.to_string()
    };
    if let Some(dir) = crate::alphacode_app_core::bugbounty_install::go_bin_dir() {
        let candidate = dir.join(&exe);
        if candidate.is_file() {
            return candidate.to_string_lossy().to_string();
        }
    }
    binary.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_install_is_limited_to_known_go_security_tools() {
        for binary in ["dalfox", "ffuf"] {
            let step = automatic_go_install_plan(binary)
                .unwrap_or_else(|| panic!("{binary} should have a Go install plan"));
            assert_eq!(
                step.family,
                crate::alphacode_app_core::bugbounty_install::InstallFamily::Go
            );
            assert_eq!(step.args.first().map(String::as_str), Some("install"));
        }
        assert!(automatic_go_install_plan("unknown-tool").is_none());
        assert!(automatic_go_install_plan("curl").is_none());
    }

    #[test]
    fn resolve_program_prefers_the_go_bin_copy_when_present() {
        // `go_bin_dir` honours GOBIN, so this pins the lookup deterministically
        // without depending on what the developer happens to have installed.
        let dir = tempfile::Builder::new()
            .prefix("alphacode-gobin-")
            .tempdir()
            .expect("tempdir");
        let exe = if cfg!(windows) { "httpx.exe" } else { "httpx" };
        let planted = dir.path().join(exe);
        std::fs::write(&planted, b"not really httpx").expect("write");

        // SAFETY: single-threaded for the duration of this assertion; the test
        // harness runs this test alone unless the suite is run concurrently.
        unsafe { std::env::set_var("GOBIN", dir.path()) };
        let resolved = resolve_program("httpx");
        unsafe { std::env::remove_var("GOBIN") };

        assert_eq!(
            resolved,
            planted.to_string_lossy().to_string(),
            "the Go-installed httpx must win over whatever PATH resolves first"
        );
    }

    #[test]
    fn resolve_program_falls_back_to_the_bare_name() {
        unsafe { std::env::set_var("GOBIN", "definitely-not-a-real-dir") };
        let resolved = resolve_program("httpx");
        unsafe { std::env::remove_var("GOBIN") };
        assert_eq!(resolved, "httpx", "must still name the binary to fix");
    }

    #[test]
    fn leading_dash_domain_is_rejected() {
        // These are real flag names for these tools, so accepting them would
        // silently change what the tool does.
        for bad in ["-t", "-o", "-providers=otx", "-dates", "-json", "-oJ"] {
            assert!(validate_hostname(bad).is_err(), "accepted flag {bad}");
        }
    }

    #[test]
    fn normal_hostnames_and_wildcards_are_accepted() {
        for good in [
            "example.com",
            "api.example.co.uk",
            "*.example.com",
            "xn--bcher-kva.example",
            "my_host.example.com",
        ] {
            assert!(validate_hostname(good).is_ok(), "rejected {good}");
        }
    }

    #[test]
    fn whitespace_and_nul_are_rejected() {
        assert!(validate_hostname("example.com -o /tmp").is_err());
        assert!(validate_hostname("example.com\trm").is_err());
        assert!(validate_hostname("example.com\nrm -rf /").is_err());
        assert!(validate_hostname("").is_err());
        assert!(validate_hostname("   ").is_err());
    }

    #[test]
    fn urls_are_accepted_but_flag_like_urls_are_not() {
        assert!(validate_target("https://example.com/api?a=1").is_ok());
        assert!(validate_target("example.com:8443").is_ok());
        assert!(validate_target("-example.com").is_err());
        assert!(validate_target("https://exa mple.com").is_err());
    }

    #[test]
    fn parse_lines_caps_and_reports_the_truth() {
        let raw = format!("a\nb\n-\n\nc\n{}", "x\n".repeat(MAX_OUTPUT_LINES + 50)).into_bytes();
        let (lines, total, truncated) = parse_lines(&raw);
        assert!(truncated, "expected truncation");
        assert!(lines.len() <= MAX_OUTPUT_LINES);
        // The true count is reported so a capped result is never read as
        // "only these N existed".
        assert!(total > MAX_OUTPUT_LINES, "total={total}");
        assert!(truncation_notice(lines.len(), total).contains("of"));
        assert!(
            !lines.iter().any(|l| l == "-"),
            "stdin sentinel leaked through"
        );
    }

    #[test]
    fn parse_lines_does_not_flag_small_output() {
        let (lines, total, truncated) = parse_lines(b"a\nb\r\nc\n");
        assert_eq!(lines, vec!["a", "b", "c"]);
        assert_eq!(total, 3);
        assert!(!truncated);
        assert!(truncation_notice(3, 3).is_empty());
    }
}
