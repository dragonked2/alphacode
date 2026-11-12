//! Keep URL query parameters intact when invoking a shell.
//!
//! Plain curl commands with ampersand-separated parameters can be quoted
//! automatically. Other commands with URL query operators require explicit
//! quoting before the shell runs.

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

/// Quote a plain curl URL whose ampersands separate query parameters.
/// Keep ambiguous shell commands on the normal validation path.
pub fn quote_curl_query_url(command: &str) -> Option<String> {
    if command
        .bytes()
        .any(|byte| byte.is_ascii_control() && byte != b'\t')
    {
        return None;
    }
    let mut words = command.split([' ', '\t']).filter(|word| !word.is_empty());
    if !matches!(words.next()?, "curl" | "curl.exe") {
        return None;
    }

    let url = words.by_ref().find(|word| {
        !matches!(
            *word,
            "-s" | "-S"
                | "-sS"
                | "-f"
                | "-fsS"
                | "-L"
                | "-I"
                | "-v"
                | "--silent"
                | "--show-error"
                | "--fail"
                | "--location"
                | "--head"
                | "--verbose"
        )
    })?;
    if words.next().is_some()
        || !(url.starts_with("https://") || url.starts_with("http://"))
        || !url
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b":/?&=._~-+@,".contains(&byte))
    {
        return None;
    }

    let (_, query) = url.split_once('?')?;
    let (_, extra_parameters) = query.split_once('&')?;
    if !extra_parameters.split('&').all(|parameter| {
        parameter.split_once('=').is_some_and(|(key, _)| {
            !key.is_empty()
                && key
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || b"._~-".contains(&byte))
        })
    }) {
        return None;
    }

    let trimmed = command.trim_end_matches([' ', '\t']);
    let start = trimmed.len() - url.len();
    Some(format!(
        "{}\"{url}\"{}",
        &command[..start],
        &command[trimmed.len()..]
    ))
}

/// Detect Windows-URL-safety issues in a command string. Cheap,
/// deterministic, no network.
pub fn scan_for_shell_url_issues(command: &str) -> ShellUrlSafety {
    let Some(warning) = detect_url_with_shell_chars(command) else {
        return ShellUrlSafety::Clean;
    };
    let mut details = vec![warning];
    if let Some(detail) = detect_findstr_with_url(command) {
        details.push(detail);
    }
    if let Some(detail) = detect_powershell_pipe_with_url(command) {
        details.push(detail);
    }
    ShellUrlSafety::Warning(format!(
        "Command not run: quote the entire URL, for example `curl \"https://example.test/path?a=1&b=2\"`, or use `webfetch`. {}.",
        details.join(". ")
    ))
}

fn detect_url_with_shell_chars(command: &str) -> Option<String> {
    // Find any URL that contains &, |, <, >, ^, or ! in the query string
    // (i.e. after the `?`).
    let bytes = command.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        // Look for `http://` or `https://`
        if bytes[i..].starts_with(b"http://") || bytes[i..].starts_with(b"https://") {
            let url_start = i;
            // Find end of URL: whitespace, a quote, a closing parenthesis,
            // or end-of-string. Matching surrounding quotes make operators
            // literal data, so do not block those commands.
            let quote = i
                .checked_sub(1)
                .and_then(|previous| bytes.get(previous).copied())
                .filter(|byte| matches!(byte, b'"' | b'\''));
            let mut j = i;
            while j < bytes.len()
                && !bytes[j].is_ascii_whitespace()
                && bytes[j] != b'"'
                && bytes[j] != b'\''
                && (quote.is_some() || bytes[j] != b')')
            {
                j += 1;
            }
            let quoted = quote.is_some_and(|quote| bytes.get(j) == Some(&quote));
            let url = std::str::from_utf8(&bytes[url_start..j]).ok()?;
            // Check if there's a query string
            if !quoted && let Some(q) = url.find('?') {
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
                        "URL contains shell metacharacter(s) {:?} in an unquoted query string; the shell will split or redirect the command instead of passing the full URL",
                        found
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
    fn plain_curl_query_is_quoted_without_changing_the_url() {
        let command = "curl -fsS https://example.test/?a=1&b=two&empty= ";
        let quoted = quote_curl_query_url(command).unwrap();
        assert_eq!(
            quoted,
            "curl -fsS \"https://example.test/?a=1&b=two&empty=\" "
        );
        assert!(scan_for_shell_url_issues(&quoted).runs_immediately());
    }

    #[test]
    fn ambiguous_curl_commands_are_not_rewritten() {
        for command in [
            "curl https://example.test/?a=1&whoami",
            "curl https://example.test/?a=1&&b=2",
            "curl https://example.test/?a=1&b=2 | jq .",
            "curl https://example.test/?a=1&b=2\nwhoami",
            "curl https://example.test/?a=1&b=$(whoami)",
            "curl https://example.test/?a=1&b=%PATH%",
            "echo https://example.test/?a=1&b=2",
        ] {
            assert!(quote_curl_query_url(command).is_none(), "{command}");
            assert!(
                !scan_for_shell_url_issues(command).runs_immediately(),
                "{command}"
            );
        }
    }

    #[test]
    fn quoted_url_with_parentheses_and_query_passes() {
        let command = "curl \"https://example.test/?a=one&b=(two)\"";
        assert!(scan_for_shell_url_issues(command).runs_immediately());
    }

    #[test]
    fn clean_linux_command_passes() {
        let v =
            scan_for_shell_url_issues("curl -s \"https://api.example.com/users?a=1&b=2\" | jq .");
        assert!(v.runs_immediately());
    }

    #[test]
    fn url_with_ampersand_in_query_triggers_warning() {
        let v = scan_for_shell_url_issues("curl https://api.example.com/?a=1&b=2");
        match v {
            ShellUrlSafety::Warning(s) => {
                assert!(s.contains("metacharacter"));
                assert!(s.contains("Command not run"));
            }
            _ => panic!("expected Warning"),
        }
    }

    #[test]
    fn url_with_pipe_in_query_triggers_warning() {
        let v = scan_for_shell_url_issues("curl https://api.example.com/?q=a|b");
        assert!(!v.runs_immediately());
    }

    #[test]
    fn findstr_with_url_triggers_warning() {
        let v =
            scan_for_shell_url_issues("curl https://api.example.com/?a=1&b=2 | findstr \"200\"");
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
            "curl https://api.example.com/?a=1&b=2 | powershell Select-String pattern",
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
