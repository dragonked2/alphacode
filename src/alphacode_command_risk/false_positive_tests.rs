//! Regression tests for command-risk gate **false positives**.
//!
//! The companion to `bypass_tests.rs`, which protects against the gate being too
//! permissive. This file protects against it being too strict, which is just as
//! damaging in practice: a `Catastrophic` verdict is explicitly "no amount of
//! model justification can unlock it", so a false positive here is not a warning
//! the agent can route around -- it is a command the agent can never run.
//!
//! The specific regression: the defense-in-depth scan for a destructive verb
//! hidden behind an unparseable wrapper also matched *shell names anywhere in
//! the segment*. So a command that merely mentioned a shell was hard-denied.

use super::{RiskContext, RiskLevel, assess};

fn ctx() -> RiskContext {
    RiskContext {
        working_dir: Some(if cfg!(windows) {
            std::path::PathBuf::from("C:\\work\\proj")
        } else {
            std::path::PathBuf::from("/work/proj")
        }),
        home_dir: Some(if cfg!(windows) {
            std::path::PathBuf::from("C:\\Users\\tester")
        } else {
            std::path::PathBuf::from("/home/tester")
        }),
    }
}

fn level_of(command: &str) -> RiskLevel {
    assess(command, &ctx()).level
}

/// A shell name inside a quoted argument is data, not code.
///
/// These are the commands that were denied. Six of seven commands in the
/// original report were ordinary work refused purely for *mentioning* a shell:
/// a grep pattern, a commit message, an echo body.
#[test]
fn mentioning_a_shell_in_quoted_text_is_allowed() {
    for command in [
        r#"grep -rn "sh" src/"#,
        r#"git commit -m "fix the sh wrapper""#,
        r#"echo "run bash later""#,
        r#"grep -r "powershell" docs/"#,
        r#"echo "cmd.exe is documented here""#,
        r#"echo "eval is a builtin""#,
    ] {
        assert_ne!(
            level_of(command),
            RiskLevel::Catastrophic,
            "quoted text is data and must not deny the command: {command}"
        );
    }
}

/// A read-only program cannot execute anything, so a shell name in its argument
/// list is inert by construction.
#[test]
fn a_read_only_program_cannot_execute_a_shell() {
    for command in ["ls sh", "cat bash", "find . -name sh", "grep -rn eval ."] {
        assert_ne!(
            level_of(command),
            RiskLevel::Catastrophic,
            "a read-only program cannot run its arguments: {command}"
        );
    }
}

/// The exemptions above must not open a hole. Every one of these actually does
/// execute something, so every one must still be denied.
#[test]
fn the_exemptions_do_not_admit_real_execution() {
    for command in [
        // `find` is read-only except in these forms -- the whole reason the
        // conditional-flag carve-out exists. This one reaches `/`.
        "find . -exec sh -c 'rm -rf /' \\;",
        // A shell in an *unquoted* operand of a program that can run it.
        "docker run img sh -c 'rm -rf /'",
        "npm run sh",
        // The shell is not the program's program, so the inert/exempt branches
        // must not apply.
        "bash -c \"rm -rf ~\"",
        "powershell -Command \"Remove-Item -Recurse -Force $env:USERPROFILE\"",
    ] {
        assert_eq!(
            level_of(command),
            RiskLevel::Catastrophic,
            "this really does execute something and must stay denied: {command}"
        );
    }
}

/// `find` in a destructive form is still graded as a write, so its operands are
/// path-checked -- the read-only exemption must not swallow the whole segment.
///
/// `find . -delete` is deliberately not `Catastrophic`: it only removes files
/// inside the working directory, which is inside the blast radius this gate
/// accepts. What matters here is that the carve-out keeps the segment on the
/// write path at all.
#[test]
fn find_in_a_destructive_form_still_grades_its_operands() {
    // Below the working directory: allowed.
    assert_ne!(
        level_of("find . -delete"),
        RiskLevel::Catastrophic,
        "deleting inside the working directory is not catastrophic"
    );
    // Outside it, into the home directory: catastrophic.
    assert_eq!(
        level_of("find ~ -delete"),
        RiskLevel::Catastrophic,
        "the same command reaching the home directory must be catastrophic"
    );
}

/// A `#` comment must not disturb quote tracking for the rest of the command.
///
/// This is the real session command that was denied as a protected-path
/// destruction. The apostrophe in `# Check if it's ...` opened a quoted region
/// in a tokenizer with no comment handling, so every later quote fell out of
/// sync and `'​<meta[^>]*og:title[^>]*>​'` was split at its `>` into fragments
/// that were graded as truncating redirect targets.
#[test]
fn a_comment_does_not_desync_quote_tracking() {
    let command = r#"echo "=== Check for og:title meta ==="
# Check if it's actually returning the internal page
echo "$RESP" | grep -oP '<meta[^>]*og:title[^>]*>' | head -5
echo "$RESP" | grep -oP 'internal[^<"]{0,100}' | sort -u"#;
    assert_ne!(
        level_of(command),
        RiskLevel::Catastrophic,
        "a comment must not turn a quoted grep pattern into a write target"
    );
    // The pattern has to stay one token: splitting it at `>` is what produced
    // the junk targets in the first place.
    let tokens = super::tokenize(command);
    let words: Vec<&str> = tokens.iter().map(|token| token.text.as_str()).collect();
    assert!(
        words.contains(&"<meta[^>]*og:title[^>]*>"),
        "the quoted pattern must survive tokenization whole: {words:?}"
    );
}

/// A `#` only opens a comment at the start of a word, so fragments that merely
/// contain one still tokenize.
#[test]
fn a_hash_inside_a_word_is_not_a_comment() {
    assert_eq!(level_of("echo foo#bar"), RiskLevel::Safe);
    assert_eq!(
        level_of("curl -s 'https://x.test/a#/route'"),
        RiskLevel::Safe
    );
}

/// A glob in the working directory has the same blast radius as the concrete
/// path beside it, which this gate already grades as routine. When a session is
/// started in the home directory the working directory *is* `$HOME`, so every
/// relative target resolved to a parent of `path == home` and graded
/// catastrophic -- that is what denied ordinary commands in the reported
/// session.
#[test]
fn a_glob_in_the_working_directory_is_not_catastrophic() {
    let at_home = RiskContext {
        working_dir: ctx().home_dir.clone(),
        home_dir: ctx().home_dir.clone(),
    };
    for command in [
        "rm -rf '*.tmp'",
        "rm -rf *.log",
        r#"rm -rf ']*og:title[^*'"#,
    ] {
        let assessment = assess(command, &at_home);
        assert_ne!(
            assessment.level,
            RiskLevel::Catastrophic,
            "a glob in the session's own working directory is routine: {command} ({:?})",
            assessment.findings
        );
    }
}

/// A glob with no directory part and nowhere to resolve it against cannot be
/// claimed to sit under `/`: `Path::parent()` yielding an empty path must not be
/// read as the protected system root.
#[test]
fn a_bare_glob_with_no_working_directory_is_not_catastrophic() {
    let unknown_cwd = RiskContext {
        working_dir: None,
        home_dir: ctx().home_dir.clone(),
    };
    let assessment = assess("rm -rf '*.tmp'", &unknown_cwd);
    assert_ne!(
        assessment.level,
        RiskLevel::Catastrophic,
        "{:?}",
        assessment.findings
    );
}

/// A trailing `)` from a command substitution is not part of the redirect
/// target, so `2>/dev/null` inside `$(...)` must still be recognised as a sink
/// rather than graded as a write to an unknown path outside the working
/// directory.
#[test]
fn a_device_sink_closed_by_a_substitution_is_not_graded() {
    let assessment = assess(
        r#"RESP=$(curl -s -L --max-time 10 "https://x.test/y" 2>/dev/null)"#,
        &ctx(),
    );
    assert_eq!(
        assessment.level,
        RiskLevel::Safe,
        "{:?}",
        assessment.findings
    );
}

/// The three relaxations above must not open the holes they exist beside: a glob
/// that really does resolve into a protected directory stays denied, wherever
/// the session happens to be running from.
#[test]
fn globs_in_protected_directories_are_still_catastrophic() {
    for command in [
        "rm -rf ~/.ssh/*",
        "rm -rf ~/.ssh/id_*",
        "rm -rf ~/.aws/credentials*",
        "rm -rf ~/*",
    ] {
        assert_eq!(
            level_of(command),
            RiskLevel::Catastrophic,
            "still destroys a protected directory: {command}"
        );
    }
}

/// `was_quoted` exists only to answer "is this word executed?", so it must not
/// disturb path comparison: a quoted target has to grade identically to an
/// unquoted one, or quoting becomes a trivial bypass of the protected-path
/// checks.
#[test]
fn quoting_a_path_does_not_change_how_it_is_graded() {
    for (quoted, bare) in [
        (r#"rm -rf "$HOME""#, "rm -rf $HOME"),
        (r#"rm -rf '~'"#, "rm -rf ~"),
    ] {
        assert_eq!(
            level_of(quoted),
            level_of(bare),
            "quoting must not change the risk of a path target: {quoted}"
        );
        assert_eq!(
            level_of(quoted),
            RiskLevel::Catastrophic,
            "a protected-path target stays catastrophic when quoted: {quoted}"
        );
    }
}
