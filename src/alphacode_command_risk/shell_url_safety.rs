//! Detect the "URL with `&` and `=` in a powershell / findstr chain"
//! trap before a bash tool call fires.
//!
//! # Why this exists
//!
//! During a real security audit, a Windows shell invocation like:
//!
//!     curl -A "Mozilla/5.0 (compatible) SecurityAudit" \
//!       "https://example.com/api?a=1&b=2&c=3" | powershell -NoProfile \
//!       -Command "Get-Content" | findstr "match"
//!
//! was rejected by the shell with a confusing parse error. The actual
//! failure was that the `&` in the URL was interpreted by `cmd.exe` as
//! "background this command and run the next one", the `"` after the URL
//! closed a quote-pair prematurely, and `findstr` ran on an empty stream.
//!
//! The two root causes are:
//!
//! 1. The `&` and `|` in the URL get consumed by the shell.
//! 2. The `findstr` + `powershell` chain has subtle quoting rules that
//!    differ from `grep` / `awk` on Linux.
//!
//! # What this does
//!
//! Given a command string, detect whether it contains:
//! - A URL with `&`, `|`, `>`, `<`, `^`, `(`, `)`, `!` in the query string
//! - A `findstr` invocation (the Windows equivalent of grep that trips
//!   people up with `|` inside double-quoted strings)
//! - A `powershell` chain receiving piped content
//!
//! When any of these is detected, return a `Warning` with concrete
//! remediation. The warning is returned to the model before the command
//! runs, mirroring the destructive-gate pattern.
//!
//! This is NOT a hard block. The model may legitimately need `&` in a
//! command (e.g. `cmd1 & cmd2`). The gate only triggers when the `&` is
//! inside a URL query string, which is almost always wrong on Windows.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellUrlSafety {
    Clean,
    Warning(String),
}

impl ShellUrlSafety {
    pub fn runs_immediately(&self) -> bool {
        matches!(self, ShellUrlSafety::Clean)
    }
}

/// Detect Windows-URL-safety issues in a command string. Cheap,
/// deterministic, no network.
pub fn scan_for_shell_url_issues(command: &str) -> ShellUrlSafety {
    // Only relevant on Windows-style commands. We don't try to be
    // platform-perfect; we just look for the highest-signal patterns.
    let mut warnings: Vec<String> = Vec::new();

    // 1. URL with shell metacharacters in the query string
    if let Some(w) = detect_url_with_shell_chars(command) {
        warnings.push(w);
    }

    // 2. findstr + piped URL
    if let Some(w) = detect_findstr_with_url(command) {
        warnings.push(w);
    }

    // 3. powershell receiving piped URL
    if let Some(w) = detect_powershell_pipe_with_url(command) {
        warnings.push(w);
    }

    if warnings.is_empty() {
        return ShellUrlSafety::Clean;
    }

    let mut s = String::from(
        "Detected a Windows shell command pattern that frequently breaks \
         when the argument contains a URL with query parameters. On Windows, \
         `&` and `|` in URLs are interpreted by cmd.exe, breaking the command. \
         Remediation:\n",
    );
    for w in warnings {
        s.push_str(&format!("- {w}\n"));
    }
    s.push_str(
        "\nPrefer Python scripts with `requests` for HTTP work on Windows, \
         or write the URL to a file and use `--data-urlencode` / `Invoke-WebRequest` \
         from PowerShell for a portable quote helper.",
    );
    ShellUrlSafety::Warning(s)
}

fn detect_url_with_shell_chars(command: &str) -> Option<String> {
    // Find any URL that contains &, |, <, >, ^, (, ), ! in the query string
    // (i.e. after the `?`).
    let bytes = command.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Look for `http://` or `https://`
        if i + 8 <= bytes.len()
            && (&bytes[i..i + 7] == b"http://" || &bytes[i..i + 8] == b"https://")
        {
            let url_start = i;
            // Find end of URL: whitespace, ", ', end-of-string
            let mut j = i;
            while j < bytes.len()
                && !bytes[j].is_ascii_whitespace()
                && bytes[j] != b'"'
                && bytes[j] != b'\''
                && bytes[j] != b')'
            {
                j += 1;
            }
            let url = std::str::from_utf8(&bytes[url_start..j]).ok()?;
            // Check if there's a query string
            if let Some(q) = url.find('?') {
                let query = &url[q..];
                let bad_chars: &[char] = &['&', '|', '<', '>', '^', '!'];
                let mut found = Vec::new();
                for c in bad_chars {
                    if query.contains(*c) {
                        found.push(*c);
                    }
                }
                if !found.is_empty() {
                    return Some(format!(
                        "URL contains shell metacharacter(s) {:?} in query \
                         string. cmd.exe will interpret these. Either escape \
                         with `^` (caret), or pass via env var: \
                         `set URL={} && curl \"%URL%\"`",
                        found, url
                    ));
                }
            }
            i = j;
        } else {
            i += 1;
        }
    }
    None
}

fn detect_findstr_with_url(command: &str) -> Option<String> {
    let lower = command.to_ascii_lowercase();
    if !lower.contains("findstr") {
        return None;
    }
    if !contains_url(command) {
        return None;
    }
    Some(
        "`findstr` is being applied to a URL. The common Windows trap is \
         that the URL's `&` is interpreted by cmd.exe, and findstr runs on \
         an empty pipe. Use `findstr /R` with explicit patterns, or move to \
         a Python script that does the URL fetch + pattern match in one step."
            .to_string(),
    )
}

fn detect_powershell_pipe_with_url(command: &str) -> Option<String> {
    let lower = command.to_ascii_lowercase();
    let has_ps = lower.contains("powershell")
        || lower.contains("invoke-webrequest")
        || lower.contains("iwr ");
    if !has_ps {
        return None;
    }
    // PowerShell chained with `| findstr` or `| Select-String` on a URL
    // tends to fail on Windows when the URL has `&` in it.
    if (lower.contains("findstr") || lower.contains("select-string")) && contains_url(command) {
        return Some(
            "PowerShell is receiving piped output from a URL. The Windows \
             trap: `&` in the URL terminates the command at the shell \
             level. Use `Invoke-WebRequest -Uri $url -OutFile out.html` \
             then `Get-Content out.html | Select-String pattern`."
                .to_string(),
        );
    }
    None
}

fn contains_url(s: &str) -> bool {
    s.contains("http://") || s.contains("https://")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_linux_command_passes() {
        let v = scan_for_shell_url_issues("curl -s https://api.example.com/users | jq .");
        assert!(v.runs_immediately());
    }

    #[test]
    fn url_with_ampersand_in_query_triggers_warning() {
        let v = scan_for_shell_url_issues("curl \"https://api.example.com/?a=1&b=2\"");
        match v {
            ShellUrlSafety::Warning(s) => {
                assert!(s.contains("metacharacter"));
            }
            _ => panic!("expected Warning"),
        }
    }

    #[test]
    fn url_with_pipe_in_query_triggers_warning() {
        let v = scan_for_shell_url_issues("curl \"https://api.example.com/?q=a|b\"");
        assert!(!v.runs_immediately());
    }

    #[test]
    fn findstr_with_url_triggers_warning() {
        let v = scan_for_shell_url_issues("curl https://api.example.com | findstr \"200\"");
        match v {
            ShellUrlSafety::Warning(s) => {
                assert!(s.contains("findstr"));
            }
            _ => panic!("expected Warning"),
        }
    }

    #[test]
    fn powershell_pipe_with_url_triggers_warning() {
        let v = scan_for_shell_url_issues(
            "curl https://api.example.com | powershell Select-String pattern",
        );
        match v {
            ShellUrlSafety::Warning(s) => {
                assert!(s.contains("PowerShell"));
            }
            _ => panic!("expected Warning"),
        }
    }

    #[test]
    fn clean_powershell_no_url_passes() {
        let v = scan_for_shell_url_issues("powershell Get-Process | Select-String chrome");
        assert!(v.runs_immediately());
    }
}
