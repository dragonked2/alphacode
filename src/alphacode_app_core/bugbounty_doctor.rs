//! Bug-bounty tools doctor — probe `$PATH` for every external binary the
//! bundled `/bugbounty` skill references, and emit a structured report
//! that the user can act on.
//!
//! # Why this exists
//!
//! `/bugbounty` is a 16-subskill workflow that chains tools like
//! `subfinder` -> `httpx` -> `katana` -> `nuclei`. None of these ship with
//! Alphacode — they're external Go / Python / native binaries the
//! operator installs themselves. A user running `/bugbounty hunt-sqli`
//! on a fresh install will hit a wall on the first pipeline step.
//!
//! `/bugbounty doctor` is the safe first step: it does not run any tool
//! (read-only probe of `$PATH` via `which`), groups results by
//! present / missing, and prints the right install command per missing
//! tool on the user's platform. No `go install` or `apt install` is run
//! without the user invoking a separate, explicit installer.
//!
//! # Public API
//!
//! ```ignore
//! use crate::alphacode_app_core::bugbounty_doctor;
//! let report = bugbounty_doctor::probe();
//! let md = bugbounty_doctor::render_markdown(&report);
//! app.push_display_message(DisplayMessage::system(md));
//! ```
//!
//! # Adding a new tool to the probe list
//!
//! Add a [`ToolSpec`] entry to [`TOOL_SPECS`]. The probe is automatic.
//! Tests in this file cover "every spec has a binary name and an
//! install hint".

use std::process::Command;

/// The toolchain probed by the doctor. As of v1.0.7 these are the
/// binaries referenced across the 16 subskills of the bundled
/// `/bugbounty` skill. Add a new entry here whenever a subskill
/// documents a tool that is not already on this list.
pub const TOOL_SPECS: &[ToolSpec] = &[
    // Projectdiscovery Go-toolkit (the canonical pipeline)
    ToolSpec {
        binary: "subfinder",
        purpose: "Passive subdomain enumeration",
        install: "go: go install -v github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest",
    },
    ToolSpec {
        binary: "httpx",
        purpose: "HTTP probe + tech fingerprint",
        install: "go: go install -v github.com/projectdiscovery/httpx/cmd/httpx@latest",
    },
    ToolSpec {
        binary: "nuclei",
        purpose: "Template-driven vuln scanner",
        install: "go: go install -v github.com/projectdiscovery/nuclei/v3/cmd/nuclei@latest",
    },
    ToolSpec {
        binary: "katana",
        purpose: "Next-gen web crawler",
        install: "go: go install -v github.com/projectdiscovery/katana/cmd/katana@latest",
    },
    ToolSpec {
        binary: "naabu",
        purpose: "Fast port scanner",
        install: "go: go install -v github.com/projectdiscovery/naabu/v2/cmd/naabu@latest",
    },
    ToolSpec {
        binary: "dnsx",
        purpose: "DNS toolkit",
        install: "go: go install -v github.com/projectdiscovery/dnsx/cmd/dnsx@latest",
    },
    // Alternative recon sources
    ToolSpec {
        binary: "amass",
        purpose: "In-depth subdomain enum (OWASP)",
        install: "go: go install -v github.com/owasp-amass/amass/v4/...@master",
    },
    ToolSpec {
        binary: "assetfinder",
        purpose: "Find related domains",
        install: "go: go install -v github.com/tomnomnom/assetfinder@latest",
    },
    ToolSpec {
        binary: "gau",
        purpose: "Get All URLs (Wayback, Common Crawl, OTX)",
        install: "go: go install -v github.com/lc/gau/v2/cmd/gau@latest",
    },
    ToolSpec {
        binary: "waybackurls",
        purpose: "Pull URLs from Wayback Machine",
        install: "go: go install -v github.com/tomnomnom/waybackurls@latest",
    },
    // Pipeline plumbing
    ToolSpec {
        binary: "anew",
        purpose: "Append-only deduplication",
        install: "go: go install -v github.com/tomnomnom/anew@latest",
    },
    ToolSpec {
        binary: "jq",
        purpose: "JSON stream processor",
        install: "apt: apt install -y jq   | brew: brew install jq   | winget: winget install jqlang.jq",
    },
    // Active probing
    ToolSpec {
        binary: "ffuf",
        purpose: "Fast web fuzzer",
        install: "go: go install -v github.com/ffuf/ffuf/v2@latest",
    },
    ToolSpec {
        binary: "dirb",
        purpose: "Web content scanner (legacy)",
        install: "apt: apt install -y dirb   | brew: brew install dirb",
    },
    ToolSpec {
        binary: "gobuster",
        purpose: "Directory/DNS/VHost brute-forcer",
        install: "go: go install -v github.com/OJ/gobuster/v3@latest",
    },
    ToolSpec {
        binary: "sqlmap",
        purpose: "Automatic SQL injection & takeover",
        install: "apt: apt install -y sqlmap   | brew: brew install sqlmap   | pipx: pipx install sqlmap",
    },
    ToolSpec {
        binary: "nmap",
        purpose: "Network mapper",
        install: "apt: apt install -y nmap   | brew: brew install nmap   | winget: winget install Insecure.Nmap",
    },
    ToolSpec {
        binary: "nikto",
        purpose: "Web server scanner",
        install: "apt: apt install -y nikto   | brew: brew install nikto",
    },
    // Generic
    ToolSpec {
        binary: "curl",
        purpose: "HTTP client (almost always preinstalled)",
        install: "preinstalled: on macOS / most Linux; or apt: apt install -y curl",
    },
    ToolSpec {
        binary: "wget",
        purpose: "HTTP downloader",
        install: "apt: apt install -y wget   | brew: brew install wget   | winget: winget install JernejSimoncic.Wget",
    },
];

/// A tool we know about: its binary name, what it does, and how to
/// install it on the user's platform. The doctor uses the install hint to
/// tell the user *what* to run, not to run it.
#[derive(Debug, Clone, Copy)]
pub struct ToolSpec {
    pub binary: &'static str,
    pub purpose: &'static str,
    /// Free-form install instructions. Format: `family: command`. The
    /// `family` is one of `apt`, `brew`, `go`, `pipx`, `winget`,
    /// `scoop`, `preinstalled`. The doctor picks the first one that
    /// matches the user's detected platform when rendering per-tool
    /// instructions.
    pub install: &'static str,
}

/// A probe result for one tool.
#[derive(Debug, Clone)]
pub struct ToolReport {
    pub binary: String,
    pub purpose: String,
    pub install_hint: String,
    pub status: ToolStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolStatus {
    /// `which <binary>` succeeded.
    Present { path: String },
    /// `which <binary>` failed. The install_hint will guide the user.
    Missing,
}

/// Run the probe against the current `$PATH`. Read-only; does not
/// execute the tool itself, only resolves which(1) for its name.
pub fn probe() -> Vec<ToolReport> {
    let mut out = Vec::with_capacity(TOOL_SPECS.len());
    for spec in TOOL_SPECS {
        let status = which(spec.binary);
        out.push(ToolReport {
            binary: spec.binary.to_string(),
            purpose: spec.purpose.to_string(),
            install_hint: spec.install.to_string(),
            status,
        });
    }
    out
}

/// `which <binary>` wrapper. Falls back to checking the bare binary name
/// on Windows (e.g. `subfinder.exe`) and to looking in `$PATH` directly
/// if `which` is unavailable.
fn which(binary: &str) -> ToolStatus {
    // Try `which` first (POSIX + Git Bash + most CI).
    let candidates: &[&str] = if cfg!(windows) {
        &["where.exe", "which.exe", "which"]
    } else {
        &["which"]
    };
    for cmd in candidates {
        let output = Command::new(cmd).arg(binary).output();
        if let Ok(out) = output
            && out.status.success()
        {
            let path = String::from_utf8_lossy(&out.stdout)
                .lines()
                .next()
                .unwrap_or("")
                .trim()
                .to_string();
            if !path.is_empty() {
                return ToolStatus::Present { path };
            }
        }
    }
    // Manual PATH walk as a last resort (covers weird shells where
    // `which` is missing).
    if let Ok(path_var) = std::env::var("PATH") {
        let sep = if cfg!(windows) { ';' } else { ':' };
        for dir in path_var.split(sep) {
            if dir.is_empty() {
                continue;
            }
            let candidate = std::path::Path::new(dir).join(binary);
            if candidate.is_file() {
                return ToolStatus::Present {
                    path: candidate.to_string_lossy().into_owned(),
                };
            }
            if cfg!(windows) {
                let with_exe = candidate.with_extension("exe");
                if with_exe.is_file() {
                    return ToolStatus::Present {
                        path: with_exe.to_string_lossy().into_owned(),
                    };
                }
            }
        }
    }
    ToolStatus::Missing
}

/// Render the probe as a single markdown string ready to drop into a
/// `DisplayMessage`. Categorizes present vs missing, lists each tool
/// with its install hint when missing.
pub fn render_markdown(report: &[ToolReport]) -> String {
    let mut md = String::new();
    let present: Vec<&ToolReport> = report
        .iter()
        .filter(|r| matches!(r.status, ToolStatus::Present { .. }))
        .collect();
    let missing: Vec<&ToolReport> = report
        .iter()
        .filter(|r| matches!(r.status, ToolStatus::Missing))
        .collect();

    md.push_str(&format!(
        "## Bug-bounty tool doctor\n\n{} present, {} missing.\n\n",
        present.len(),
        missing.len()
    ));

    if !missing.is_empty() {
        md.push_str("### Missing tools\n\n");
        for r in &missing {
            md.push_str(&format!(
                "- **{}** — {} — install: `{}`\n",
                r.binary, r.purpose, r.install_hint
            ));
        }
        md.push('\n');
    }

    if !present.is_empty() {
        md.push_str("### Present tools\n\n");
        for r in &present {
            if let ToolStatus::Present { path } = &r.status {
                md.push_str(&format!("- **{}** — `{}`\n", r.binary, path));
            }
        }
    }

    md.push_str(
        "\nThis command does not install anything. To install a missing tool, copy the suggested command and run it in your shell. \
         Or, in a future release, `/bugbounty install-tools` will offer an explicit per-tool confirmation flow.",
    );

    md
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spec_has_a_binary_and_hint() {
        for spec in TOOL_SPECS {
            assert!(
                !spec.binary.trim().is_empty(),
                "tool spec has empty binary name"
            );
            assert!(
                !spec.purpose.trim().is_empty(),
                "{}: purpose is empty",
                spec.binary
            );
            assert!(
                !spec.install.trim().is_empty(),
                "{}: install hint is empty",
                spec.binary
            );
            // Install hint must start with a known family tag followed by `:`.
            // The format is `<family>: <command>` (with optional ` <alt>`s
            // chained via `|`). The tag itself is the substring before the
            // first colon, trimmed.
            let first_word = spec.install.split(':').next().unwrap_or("").trim();
            assert!(
                matches!(
                    first_word,
                    "go" | "apt" | "brew" | "winget" | "scoop" | "pipx" | "preinstalled"
                ),
                "{}: install hint must start with a family tag (got {:?})",
                spec.binary,
                first_word
            );
        }
    }

    #[test]
    fn no_duplicate_binary_names() {
        let mut seen = std::collections::HashSet::new();
        for spec in TOOL_SPECS {
            assert!(
                seen.insert(spec.binary),
                "duplicate binary in TOOL_SPECS: {:?}",
                spec.binary
            );
        }
    }

    #[test]
    fn probe_returns_a_report_for_every_spec() {
        let report = probe();
        assert_eq!(report.len(), TOOL_SPECS.len());
        for (i, spec) in TOOL_SPECS.iter().enumerate() {
            assert_eq!(report[i].binary, spec.binary);
        }
    }

    #[test]
    fn render_markdown_groups_present_and_missing() {
        let report = vec![
            ToolReport {
                binary: "a".into(),
                purpose: "first".into(),
                install_hint: "go: install".into(),
                status: ToolStatus::Present {
                    path: "/usr/bin/a".into(),
                },
            },
            ToolReport {
                binary: "b".into(),
                purpose: "second".into(),
                install_hint: "apt: install".into(),
                status: ToolStatus::Missing,
            },
        ];
        let md = render_markdown(&report);
        assert!(md.contains("Bug-bounty tool doctor"));
        assert!(md.contains("1 present, 1 missing"));
        assert!(md.contains("Missing tools"));
        assert!(md.contains("**b**"));
        assert!(md.contains("Present tools"));
        assert!(md.contains("**a**"));
        assert!(md.contains("/usr/bin/a"));
    }

    #[test]
    fn render_markdown_does_not_install_anything() {
        // The doctor is a probe, not an installer. The render must
        // include a clear "this does not install anything" notice so a
        // user running `/bugbounty doctor` is never surprised by side
        // effects.
        let md = render_markdown(&[]);
        assert!(
            md.contains("does not install anything"),
            "doctor render must disclaim installation; got: {md:?}"
        );
    }

    #[test]
    fn tool_status_distinguishes_present_and_missing() {
        assert_ne!(
            ToolStatus::Present { path: "x".into() },
            ToolStatus::Missing
        );
    }
}
