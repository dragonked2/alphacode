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
//! `/bugbounty doctor` is the read-only first step: it reports present and
//! missing tools with install guidance. When a known Go tool is invoked and is
//! missing, the tool runner attempts to set up Go and install that tool before
//! retrying it.
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
    // Additional recon tools
    ToolSpec {
        binary: "unfurl",
        purpose: "URL parsing and normalization",
        install: "go: go install -v github.com/tomnomnom/unfurl@latest",
    },
    ToolSpec {
        binary: "meg",
        purpose: "URL extraction at scale",
        install: "go: go install -v github.com/tomnomnom/meg@latest",
    },
    ToolSpec {
        binary: "gf",
        purpose: "Grep patterns for vulnerability discovery",
        install: "go: go install -v github.com/tomnomnom/gf@latest",
    },
    ToolSpec {
        binary: "feroxbuster",
        purpose: "Fast content discovery",
        install: "go: go install -v github.com/epi052/feroxbuster@latest",
    },
    ToolSpec {
        binary: "hakrawler",
        purpose: "Web crawler with JavaScript parsing",
        install: "go: go install -v github.com/hakluke/hakrawler@latest",
    },
    ToolSpec {
        binary: "gospider",
        purpose: "Fast web spider",
        install: "go: go install -v github.com/jaeles-project/gospider@latest",
    },
    ToolSpec {
        binary: "dalfox",
        purpose: "XSS scanner and payload generator",
        install: "go: go install -v github.com/hahwul/dalfox/v2@latest",
    },
    ToolSpec {
        binary: "kxss",
        purpose: "XSS reflection finder",
        install: "go: go install -v github.com/Emoe/kxss@latest",
    },
    ToolSpec {
        binary: "corsy",
        purpose: "CORS misconfiguration scanner",
        install: "go: go install -v github.com/s0md3v/Corsy@latest",
    },
    ToolSpec {
        binary: "crlfuzz",
        purpose: "CRLF injection scanner",
        install: "go: go install -v github.com/dwisiswant0/crlfuzz/cmd/crlfuzz@latest",
    },
    ToolSpec {
        binary: "cariddi",
        purpose: "Web crawler and scanner",
        install: "go: go install -v github.com/edoardottt/cariddi/cmd/cariddi@latest",
    },
    ToolSpec {
        binary: "httprobe",
        purpose: "Probe live hosts",
        install: "go: go install -v github.com/tomnomnom/httprobe@latest",
    },
    ToolSpec {
        binary: "qsreplace",
        purpose: "Query string parameter replacer",
        install: "go: go install -v github.com/tomnomnom/qsreplace@latest",
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
pub fn which(binary: &str) -> ToolStatus {
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
            // Skip *relative* PATH entries. They resolve against the process
            // working directory, so a `./nuclei` file in the cwd would be
            // reported as an installed tool — and, because `CreateProcess`
            // applies the same rule, would then actually be the binary that
            // gets executed. That is the classic cwd-hijack, and a doctor
            // that cannot see it is worse than no doctor.
            let dir_path = std::path::Path::new(dir);
            if dir_path.is_relative() {
                continue;
            }
            let candidate = dir_path.join(binary);
            if candidate.is_file() && is_executable(&candidate) {
                return ToolStatus::Present {
                    path: candidate.to_string_lossy().into_owned(),
                };
            }
            if cfg!(windows) {
                // Try every extension in PATHEXT, not just `.exe`, so `.cmd`
                // and `.bat` shims are not reported as missing.
                for ext in pathexts() {
                    let with_ext = candidate.with_extension(ext);
                    if with_ext.is_file() {
                        return ToolStatus::Present {
                            path: with_ext.to_string_lossy().into_owned(),
                        };
                    }
                }
            }
        }
    }
    // `go install` writes to ~/go/bin, which is not on PATH by default on
    // Windows (and frequently not on Linux/macOS). Without this, a perfectly
    // successful install is reported as still missing, and the installer's own
    // post-install verification fails forever.
    if let Some(go_bin) = super::bugbounty_install::go_bin_dir() {
        let candidate = go_bin.join(binary);
        if candidate.is_file() && is_executable(&candidate) {
            return ToolStatus::Present {
                path: candidate.to_string_lossy().into_owned(),
            };
        }
        if cfg!(windows) {
            for ext in pathexts() {
                let with_ext = candidate.with_extension(ext);
                if with_ext.is_file() {
                    return ToolStatus::Present {
                        path: with_ext.to_string_lossy().into_owned(),
                    };
                }
            }
        }
    }
    ToolStatus::Missing
}

/// Extensions to try on Windows, defaulting to `.exe` when PATHEXT is unset.
fn pathexts() -> Vec<&'static str> {
    const DEFAULT: &[&str] = &["exe"];
    match std::env::var("PATHEXT") {
        Ok(v) if !v.trim().is_empty() => {
            let exts: Vec<&'static str> = v
                .split(';')
                .filter_map(|e| {
                    let e = e.trim().trim_start_matches('.').to_ascii_lowercase();
                    // PATHEXT is a handful of short, process-stable strings,
                    // so interning them once is a bounded leak and avoids
                    // re-deriving the list on every lookup.
                    if e.is_empty() {
                        None
                    } else {
                        Some(&*Box::leak(e.into_boxed_str()))
                    }
                })
                .collect();
            if exts.is_empty() {
                DEFAULT.to_vec()
            } else {
                exts
            }
        }
        _ => DEFAULT.to_vec(),
    }
}

/// A file is only usable as a tool if it is a real, executable regular file.
///
/// `Path::is_file` follows symlinks and, on Unix, is true for *any* regular
/// file — including a non-executable text file left behind by a failed
/// install. Reporting that as "present" makes the recon tool fail later at
/// exec time with a confusing `EACCES`, so check the execute bit.
fn is_executable(path: &std::path::Path) -> bool {
    let Ok(meta) = path.metadata() else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        // Windows has no execute bit; extension is the only signal, and the
        // caller already filtered on it.
        true
    }
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
        "\nThis command does not install anything. Install missing tools with the bundled script:\n\
         `bash scripts/install_bugbounty_tools.sh all` (or `go` / `apt` / `pip` / `nuclei` / `wordlists`).\n\
         Verify afterwards: `bash scripts/install_bugbounty_tools.sh --verify`.\n\
         Or copy an individual install hint above and run it in your shell.",
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
