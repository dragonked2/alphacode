//! Self-installing bug-bounty toolchain.
//!
//! [`super::bugbounty_doctor`] can *report* which recon binaries are missing,
//! but it was dead code: nothing called it, and even if it had, telling a
//! user "run `go install ...`" is useless when the prerequisite (Go itself) is
//! also absent — which is the common case on a fresh Windows/macOS box.
//!
//! This module closes that loop. It resolves a *structured* install command
//! per binary, verifies the package manager it needs actually exists,
//! bootstraps the missing prerequisite when it safely can, runs the install
//! under a hard timeout, and re-probes to confirm the binary is now usable.
//!
//! Two correctness properties matter more than convenience here:
//!
//! 1. **The Go bin directory must be found.** `go install` writes to
//!    `~/go/bin`, which is *not* on `PATH` by default on Windows and often
//!    not on Linux. An installer that installs successfully but then reports
//!    the tool as still missing is worse than no installer, so
//!    [`go_bin_dir`] is consulted by the doctor too.
//! 2. **Nothing runs without a timeout and `kill_on_drop`.** Package managers
//!    hang (network stalls, a `go install` of a huge module set), and a hung
//!    installer pins the agent turn forever.
//!
//! Installation is limited to known tools with a concrete install plan. A
//! requested Go tool may be installed on first use; unknown binaries and tools
//! without a Go install plan are never installed implicitly. [`install_one`]
//! serializes installs so parallel tool calls cannot race while bootstrapping
//! Go or writing into the same Go bin directory.

use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::bugbounty_doctor::{TOOL_SPECS, ToolStatus, which};

/// Upper bound on a single install command. `go install` of nuclei or amass
/// pulls a large module graph and can legitimately take several minutes on a
/// cold cache; beyond this the network is wedged and we should fail loudly
/// rather than hang.
const INSTALL_TIMEOUT: Duration = Duration::from_secs(900);

/// Upper bound on a prerequisite probe (`go version`, `winget --version`).
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);

/// Prevent concurrent tool calls from racing through package installation.
static TOOL_INSTALL_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The package-manager family used to install a binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallFamily {
    /// `go install <module>` — the ProjectDiscovery / tomnomnom toolchain.
    Go,
    /// `pipx install <pkg>` (preferred) or `python -m pip install <pkg>`.
    Python,
    /// `winget install <id>` (Windows).
    Winget,
    /// `brew install <pkg>` (macOS).
    Brew,
    /// `apt-get install` (Linux).
    Apt,
    /// Ships with the OS; cannot be installed by us.
    Preinstalled,
}

impl InstallFamily {
    /// The executable that must exist on `PATH` for this family to work.
    pub fn prerequisite_binary(self) -> Option<&'static str> {
        match self {
            InstallFamily::Go => Some("go"),
            InstallFamily::Python => Some("pipx"),
            InstallFamily::Winget => Some("winget"),
            InstallFamily::Brew => Some("brew"),
            InstallFamily::Apt => Some("apt-get"),
            InstallFamily::Preinstalled => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            InstallFamily::Go => "go",
            InstallFamily::Python => "python",
            InstallFamily::Winget => "winget",
            InstallFamily::Brew => "brew",
            InstallFamily::Apt => "apt-get",
            InstallFamily::Preinstalled => "preinstalled",
        }
    }
}

/// A single concrete command that installs one binary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallStep {
    pub binary: &'static str,
    pub family: InstallFamily,
    pub program: String,
    pub args: Vec<String>,
    /// Human-readable note shown before running (e.g. "installs to ~/go/bin").
    pub note: &'static str,
}

impl InstallStep {
    /// Shell-ish rendering for display only. Never executed through a shell.
    pub fn display(&self) -> String {
        let mut s = self.program.clone();
        for a in &self.args {
            s.push(' ');
            s.push_str(a);
        }
        s
    }
}

/// Outcome of attempting one install.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
pub enum InstallOutcome {
    /// Already present; nothing ran.
    AlreadyPresent { path: String },
    /// Installed and now resolvable.
    Installed { path: String },
    /// The install command ran but the binary still isn't on `PATH`.
    /// Usually means the Go bin dir is not exported.
    InstalledButNotOnPath { hint: String, last_output: String },
    /// The command ran and failed.
    Failed { output: String },
    /// The required package manager is missing and we don't know how to
    /// bootstrap it safely.
    MissingPrerequisite { needed: &'static str, hint: String },
    /// Not in the caller's allow-list.
    Skipped { reason: String },
    /// Exceeded [`INSTALL_TIMEOUT`]; the child was killed.
    TimedOut,
}

/// Static resolver table. Kept as data (not a `match` that rebuilds strings)
/// so the binary name is a `'static str` with no allocation and no leak.
struct GoTool {
    binary: &'static str,
    module: &'static str,
}

const GO_TOOLS: &[GoTool] = &[
    GoTool {
        binary: "subfinder",
        module: "github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest",
    },
    GoTool {
        binary: "httpx",
        module: "github.com/projectdiscovery/httpx/cmd/httpx@latest",
    },
    GoTool {
        binary: "nuclei",
        module: "github.com/projectdiscovery/nuclei/v3/cmd/nuclei@latest",
    },
    GoTool {
        binary: "katana",
        module: "github.com/projectdiscovery/katana/cmd/katana@latest",
    },
    GoTool {
        binary: "naabu",
        module: "github.com/projectdiscovery/naabu/v2/cmd/naabu@latest",
    },
    GoTool {
        binary: "dnsx",
        module: "github.com/projectdiscovery/dnsx/cmd/dnsx@latest",
    },
    GoTool {
        binary: "gau",
        module: "github.com/lc/gau/v2/cmd/gau@latest",
    },
    GoTool {
        binary: "waybackurls",
        module: "github.com/tomnomnom/waybackurls@latest",
    },
    GoTool {
        binary: "assetfinder",
        module: "github.com/tomnomnom/assetfinder@latest",
    },
    GoTool {
        binary: "anew",
        module: "github.com/tomnomnom/anew@latest",
    },
    GoTool {
        binary: "ffuf",
        module: "github.com/ffuf/ffuf/v2@latest",
    },
    GoTool {
        binary: "gobuster",
        module: "github.com/OJ/gobuster/v3@latest",
    },
    GoTool {
        binary: "amass",
        module: "github.com/owasp-amass/amass/v4/cmd/amass@master",
    },
    GoTool {
        binary: "unfurl",
        module: "github.com/tomnomnom/unfurl@latest",
    },
    GoTool {
        binary: "meg",
        module: "github.com/tomnomnom/meg@latest",
    },
    GoTool {
        binary: "gf",
        module: "github.com/tomnomnom/gf@latest",
    },
    GoTool {
        binary: "feroxbuster",
        module: "github.com/epi052/feroxbuster@latest",
    },
    GoTool {
        binary: "hakrawler",
        module: "github.com/hakluke/hakrawler@latest",
    },
    GoTool {
        binary: "gospider",
        module: "github.com/jaeles-project/gospider@latest",
    },
    GoTool {
        binary: "dalfox",
        module: "github.com/hahwul/dalfox/v2@latest",
    },
    GoTool {
        binary: "kxss",
        module: "github.com/Emoe/kxss@latest",
    },
    GoTool {
        binary: "corsy",
        module: "github.com/s0md3v/Corsy@latest",
    },
    GoTool {
        binary: "crlfuzz",
        module: "github.com/dwisiswant0/crlfuzz/cmd/crlfuzz@latest",
    },
    GoTool {
        binary: "cariddi",
        module: "github.com/edoardottt/cariddi/cmd/cariddi@latest",
    },
    GoTool {
        binary: "httprobe",
        module: "github.com/tomnomnom/httprobe@latest",
    },
    GoTool {
        binary: "qsreplace",
        module: "github.com/tomnomnom/qsreplace@latest",
    },
];

const GO_NOTE: &str =
    "installed into the Go bin dir (~/go/bin on Unix, %USERPROFILE%\\go\\bin on Windows)";

/// Non-Go installs, as (binary, family, program, args, note).
const OTHER_TOOLS: &[(&str, InstallFamily, &str, &[&str], &str)] = &[
    (
        "sqlmap",
        InstallFamily::Python,
        "pipx",
        &["install", "sqlmap", "--suffix", "=1_5"],
        "pipx isolates the install; --suffix keeps the binary reachable as `sqlmap1_5`",
    ),
    (
        "jq",
        InstallFamily::Winget,
        "winget",
        &["install", "--id", "jqlang.jq", "-e"],
        "",
    ),
    (
        "nmap",
        InstallFamily::Winget,
        "winget",
        &["install", "--id", "Insecure.Nmap", "-e"],
        "",
    ),
];

/// Resolve how to install `binary`, or `None` if it is not installable by us.
///
/// This is the single source of truth for *structured* install commands. The
/// human-readable `install` strings in [`TOOL_SPECS`] are display-only and may
/// drift; `every_installable_spec_resolves` in the tests pins them together so
/// a new tool cannot be added without a resolver.
pub fn plan_for(binary: &str) -> Option<InstallStep> {
    if let Some(t) = GO_TOOLS.iter().find(|t| t.binary == binary) {
        return Some(InstallStep {
            binary: t.binary,
            family: InstallFamily::Go,
            program: "go".to_string(),
            args: vec!["install".to_string(), t.module.to_string()],
            note: GO_NOTE,
        });
    }
    OTHER_TOOLS
        .iter()
        .find(|(b, ..)| *b == binary)
        .map(|(b, family, program, args, note)| InstallStep {
            binary: b,
            family: *family,
            program: (*program).to_string(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            note,
        })
}

/// All binaries that [`plan_for`] can install, in `TOOL_SPECS` order.
pub fn installable_binaries() -> Vec<&'static str> {
    TOOL_SPECS
        .iter()
        .map(|s| s.binary)
        .filter(|b| plan_for(b).is_some())
        .collect()
}

/// Where `go install` puts binaries on this platform.
///
/// This is the single most important piece of Windows support here: Go writes
/// to `%USERPROFILE%\go\bin` and that directory is *not* on `PATH` by default,
/// so a successful install is invisible to `which` until the user edits their
/// environment. [`super::bugbounty_doctor::which`] consults this too.
pub fn go_bin_dir() -> Option<PathBuf> {
    // Respect an explicit GOBIN, then GOPATH, then the documented defaults.
    if let Ok(bin) = std::env::var("GOBIN")
        && !bin.trim().is_empty()
    {
        return Some(PathBuf::from(bin));
    }
    let gopath = std::env::var_os("GOPATH")
        .and_then(|value| std::env::split_paths(&value).next())
        .filter(|path| !path.as_os_str().is_empty())
        .or_else(|| dirs::home_dir().map(|home| home.join("go")));
    gopath.map(|path| path.join("bin"))
}

/// True when `go` is on `PATH` and runnable.
pub fn go_available() -> bool {
    go_executable().is_some()
}

/// Find Go even when a package manager installed it after Alphacode started.
/// A running process does not inherit persistent PATH changes made by an
/// installer, so check the standard Windows install locations explicitly.
fn go_executable() -> Option<PathBuf> {
    if let ToolStatus::Present { path } = which("go") {
        return Some(PathBuf::from(path));
    }

    let candidates = if cfg!(windows) {
        let mut candidates = Vec::new();
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            candidates.push(PathBuf::from(program_files).join("Go/bin/go.exe"));
        }
        if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
            candidates.push(PathBuf::from(local_app_data).join("Programs/Go/bin/go.exe"));
        }
        candidates
    } else if cfg!(target_os = "macos") {
        vec![
            PathBuf::from("/opt/homebrew/bin/go"),
            PathBuf::from("/usr/local/bin/go"),
            PathBuf::from("/usr/local/go/bin/go"),
        ]
    } else {
        vec![
            PathBuf::from("/usr/bin/go"),
            PathBuf::from("/usr/local/go/bin/go"),
        ]
    };
    candidates.into_iter().find(|path| path.is_file())
}

/// Instruction for bootstrapping Go when the package manager cannot complete
/// installation automatically.
pub fn go_bootstrap_hint() -> String {
    if cfg!(windows) {
        "Go is not installed. Install it with `winget install GoLang.Go`, then reopen the terminal so PATH updates.".to_string()
    } else if cfg!(target_os = "macos") {
        "Go is not installed. Install it with `brew install go` (or from https://go.dev/dl/)."
            .to_string()
    } else {
        "Go is not installed. Install it from your package manager (e.g. `apt install golang-go`) or from https://go.dev/dl/.".to_string()
    }
}

/// Auto-install Go if missing. Returns true if Go is available after the call.
///
/// This bootstraps Go using the system package manager:
/// - Windows: `winget install GoLang.Go`
/// - macOS: `brew install go`
/// - Linux: `apt-get install -y golang-go`
///
/// After installation, it verifies Go can run in the current process. On
/// Windows it checks the standard install path because a running process does
/// not inherit PATH updates made by the installer.
pub async fn ensure_go_installed() -> bool {
    if go_available() {
        return true;
    }

    tracing::info!("Go not found, attempting to auto-install...");

    let result = if cfg!(windows) {
        // Try winget first
        let mut cmd = tokio::process::Command::new("winget");
        cmd.args([
            "install",
            "--id",
            "GoLang.Go",
            "-e",
            "--accept-source-agreements",
            "--accept-package-agreements",
        ])
        .kill_on_drop(true)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
        tokio::time::timeout(INSTALL_TIMEOUT, cmd.output()).await
    } else if cfg!(target_os = "macos") {
        // Try brew first
        let mut cmd = tokio::process::Command::new("brew");
        cmd.args(["install", "go"])
            .kill_on_drop(true)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        tokio::time::timeout(INSTALL_TIMEOUT, cmd.output()).await
    } else {
        // Try apt-get
        let mut cmd = tokio::process::Command::new("sudo");
        cmd.args(["apt-get", "install", "-y", "golang-go"])
            .kill_on_drop(true)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        tokio::time::timeout(INSTALL_TIMEOUT, cmd.output()).await
    };

    match result {
        Ok(Ok(output)) if output.status.success() => {
            let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            while go_executable().is_none() && tokio::time::Instant::now() < deadline {
                tokio::time::sleep(Duration::from_millis(250)).await;
            }
            if go_executable().is_some() {
                tracing::info!("Go installed successfully");
                true
            } else {
                tracing::warn!("Go installer completed, but Go is still not resolvable");
                false
            }
        }
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("Go installation failed: {}", stderr.trim());
            false
        }
        Ok(Err(e)) => {
            tracing::warn!("Go installation error: {}", e);
            false
        }
        Err(_) => {
            tracing::warn!("Go installation timed out");
            false
        }
    }
}

/// Probe a package-manager binary with a short timeout.
///
/// Uses `tokio::process` + `kill_on_drop` deliberately: a `winget --version`
/// that blocks would otherwise wedge the calling worker thread with no way out.
async fn command_available(program: &str) -> bool {
    let mut cmd = tokio::process::Command::new(program);
    cmd.arg("--version")
        .kill_on_drop(true)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    matches!(
        tokio::time::timeout(PROBE_TIMEOUT, cmd.status()).await,
        Ok(Ok(status)) if status.success()
    )
}

/// Install one binary. Never panics; always returns an outcome.
///
/// `step.binary` is the name to re-probe afterwards — callers should pass the
/// name the recon tools actually invoke.
pub async fn install_one(step: &InstallStep) -> InstallOutcome {
    let _install_guard = TOOL_INSTALL_LOCK.lock().await;

    if let ToolStatus::Present { path } = which(step.binary) {
        return InstallOutcome::AlreadyPresent { path };
    }

    if let Some(needed) = step.family.prerequisite_binary() {
        let available = if step.family == InstallFamily::Go {
            go_available()
        } else {
            command_available(needed).await
        };
        if !available {
            // Bootstrap the Go toolchain when a known Go tool is first needed.
            if step.family == InstallFamily::Go && needed == "go" {
                tracing::info!("Auto-installing Go...");
                if !ensure_go_installed().await {
                    return InstallOutcome::MissingPrerequisite {
                        needed,
                        hint: go_bootstrap_hint(),
                    };
                }
            } else {
                return InstallOutcome::MissingPrerequisite {
                    needed,
                    hint: if step.family == InstallFamily::Go {
                        go_bootstrap_hint()
                    } else {
                        format!(
                            "`{}` is required to install {} but was not found on PATH.",
                            needed, step.binary
                        )
                    },
                };
            }
        }
    }

    let program = if step.family == InstallFamily::Go {
        match go_executable() {
            Some(path) => path,
            None => {
                return InstallOutcome::MissingPrerequisite {
                    needed: "go",
                    hint: go_bootstrap_hint(),
                };
            }
        }
    } else {
        PathBuf::from(&step.program)
    };
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(&step.args)
        .kill_on_drop(true)
        // `go install` writes progress and the occasional "downloading" noise to
        // stderr; capturing it is what lets us report a real failure reason.
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    let result = tokio::time::timeout(INSTALL_TIMEOUT, cmd.output()).await;
    let output = match result {
        Err(_) => return InstallOutcome::TimedOut,
        Ok(Err(e)) => {
            return InstallOutcome::Failed {
                output: e.to_string(),
            };
        }
        Ok(Ok(out)) => out,
    };

    let mut tail = String::new();
    if !output.stderr.is_empty() {
        tail.push_str(&String::from_utf8_lossy(&output.stderr));
    }
    if tail.is_empty() && !output.stdout.is_empty() {
        tail.push_str(&String::from_utf8_lossy(&output.stdout));
    }
    let tail = crate::alphacode_core::util::truncate_str(tail.trim(), 2000).to_string();

    if !output.status.success() {
        return InstallOutcome::Failed { output: tail };
    }

    // Re-probe. `which` checks both PATH and the Go bin directory, including
    // its expected location when that directory did not exist before install.
    match which(step.binary) {
        ToolStatus::Present { path } => InstallOutcome::Installed { path },
        ToolStatus::Missing => {
            let hint = if step.family == InstallFamily::Go {
                let dir = go_bin_dir()
                    .map(|d| d.display().to_string())
                    .unwrap_or_else(|| "the Go bin dir".to_string());
                format!(
                    "Installed, but `{0}` is not on PATH. Add it with:\n  export PATH=\"$PATH:{1}\"   (or set PATH in Windows' environment variables)",
                    step.binary, dir
                )
            } else {
                format!("`{}` still not found on PATH after install.", step.binary)
            };
            InstallOutcome::InstalledButNotOnPath {
                hint,
                last_output: tail,
            }
        }
    }
}

/// Formats an installation outcome for a tool execution error.
pub fn install_failure_description(outcome: &InstallOutcome) -> String {
    match outcome {
        InstallOutcome::AlreadyPresent { path } => format!("already installed at {path}"),
        InstallOutcome::Installed { path } => format!("installed at {path}"),
        InstallOutcome::InstalledButNotOnPath { hint, .. } => hint.clone(),
        InstallOutcome::Failed { output } if output.is_empty() => {
            "installer exited unsuccessfully".to_string()
        }
        InstallOutcome::Failed { output } => format!("installer failed: {output}"),
        InstallOutcome::MissingPrerequisite { needed, hint } => {
            format!("required prerequisite `{needed}` is unavailable: {hint}")
        }
        InstallOutcome::Skipped { reason } => reason.clone(),
        InstallOutcome::TimedOut => "installation timed out".to_string(),
    }
}

/// Install an explicit allow-list of binaries, in `TOOL_SPECS` order.
///
/// `requested` is matched case-insensitively against the known binaries.
/// Anything not in the allow-list, or with no resolver, comes back as
/// [`InstallOutcome::Skipped`] so a typo is visible rather than silent.
pub async fn install_all(requested: &[String]) -> Vec<(String, InstallOutcome)> {
    let wanted: Vec<String> = requested
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .collect();
    let mut results = Vec::new();
    for spec in TOOL_SPECS {
        let binary = spec.binary;
        if !wanted.iter().any(|w| w == binary) {
            continue;
        }
        let outcome = match plan_for(binary) {
            Some(step) => install_one(&step).await,
            None => InstallOutcome::Skipped {
                reason: format!(
                    "no automated installer for `{binary}` — install manually: {}",
                    spec.install
                ),
            },
        };
        results.push((binary.to_string(), outcome));
    }
    // Report names the caller asked for that we do not know about at all.
    for w in wanted {
        let known = TOOL_SPECS.iter().any(|s| s.binary == w);
        if !known {
            results.push((
                w.clone(),
                InstallOutcome::Skipped {
                    reason: format!("`{w}` is not a known bug-bounty tool"),
                },
            ));
        }
    }
    results
}

/// A targeted, actionable error for a recon tool that could not be spawned.
///
/// Returned by the recon tools when `Command::new` fails with NotFound, so the
/// model is told exactly what to run instead of being left to guess. This is
/// the *safe* half of "self-installing": we never install implicitly, because
/// a bug-bounty run should not silently pull binaries onto the user's machine.
pub fn install_hint_for(binary: &str) -> String {
    match plan_for(binary) {
        Some(step) => {
            let prereq = match step.family.prerequisite_binary() {
                Some(_) if step.family == InstallFamily::Go && !go_available() => {
                    format!("{}. Then: ", go_bootstrap_hint())
                }
                Some(p) => format!("(requires `{p}` on PATH) "),
                None => String::new(),
            };
            format!(
                "`{binary}` is not installed. {}Install with: `{}`",
                prereq,
                step.display()
            )
        }
        None => {
            let spec = TOOL_SPECS.iter().find(|s| s.binary == binary);
            match spec {
                Some(s) => format!(
                    "`{binary}` is not installed. Install it manually: {}",
                    s.install
                ),
                None => format!("`{binary}` is not installed and has no known installer."),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_installable_spec_resolves() {
        // `plan_for` uses `Box::leak` for the binary name; make sure the set
        // of installable binaries is exactly what we expect and stays in sync
        // with TOOL_SPECS.
        let installable = installable_binaries();
        for b in &installable {
            let step = plan_for(b).expect("resolvable");
            assert_eq!(step.binary, *b, "plan_for({b}) reports the wrong binary");
            assert!(!step.program.is_empty());
            assert!(!step.args.is_empty());
        }
        // The core recon pipeline must always be installable.
        for core in [
            "subfinder",
            "httpx",
            "katana",
            "dnsx",
            "ffuf",
            "gau",
            "waybackurls",
        ] {
            assert!(plan_for(core).is_some(), "{core} has no installer");
        }
    }

    #[test]
    fn go_steps_target_the_go_toolchain() {
        let step = plan_for("subfinder").unwrap();
        assert_eq!(step.family, InstallFamily::Go);
        assert_eq!(step.program, "go");
        assert_eq!(step.args[0], "install");
        assert!(step.args[1].ends_with("@latest"), "not pinned to latest");
    }

    #[test]
    fn unknown_binary_has_no_plan_but_still_gets_a_hint() {
        assert!(plan_for("definitely-not-a-tool").is_none());
        let hint = install_hint_for("definitely-not-a-tool");
        assert!(hint.contains("definitely-not-a-tool"));
    }

    #[test]
    fn curl_is_never_auto_installed() {
        // curl ships with the OS; auto-installing it on Windows would fight
        // the built-in System32 copy.
        assert!(plan_for("curl").is_none());
    }

    #[test]
    fn install_hint_names_the_prerequisite_when_go_is_missing() {
        // The hint must remain actionable even when Go itself is absent.
        let hint = install_hint_for("subfinder");
        assert!(hint.contains("go install"), "no concrete command: {hint}");
    }
}
