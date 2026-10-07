//! Regression tests for command-risk gate bypasses.
//!
//! Every case here is a command that the gate previously classified as `Safe`
//! (or only weakly flagged) even though it destroys data. These are the worst
//! possible failures for this module: a gate that can be walked around stops
//! protecting anything, and the cost of the false negative is a user's data.

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

/// Level under an explicit context, for the cases that depend on where the
/// working directory sits relative to the home directory.
fn level_with(command: &str, ctx: &RiskContext) -> RiskLevel {
    assess(command, ctx).level
}

/// The gate must never classify any of these as `Safe`.
fn assert_not_safe(command: &str) {
    let level = level_of(command);
    assert_ne!(
        level,
        RiskLevel::Safe,
        "BYPASS: `{command}` was classified Safe"
    );
}

// --- C1: leading VAR=value assignment hid the destructive verb ----------------

#[test]
fn leading_env_assignment_does_not_hide_rm() {
    assert_not_safe("LANG=C rm -rf ~");
    assert_not_safe("LC_ALL=en_US.UTF-8 shred -u -rf /var/x");
    assert_not_safe("FOO=bar mkfs.ext4 /dev/sda1");
    // Multiple assignments in a row.
    assert_not_safe("A=1 B=2 rm -rf ~");
}

#[test]
fn assignment_like_path_arguments_are_still_classified() {
    // `contains('=')` would have eaten these; a real path with `=` in it must
    // not be mistaken for an environment assignment.
    assert_not_safe("rm -rf /opt/a=b.txt");
}

// --- C2: Windows shells and destructive commands were unknown -----------------

#[test]
fn windows_shell_inline_script_is_assessed() {
    assert_not_safe("powershell -Command \"Remove-Item -Recurse -Force $env:USERPROFILE\"");
    assert_not_safe("cmd.exe /C \"del /f /q C:\\\\Users\\\\me\\\\.aws\\\\credentials\"");
    assert_not_safe("pwsh -c \"Remove-Item -Recurse -Force ~\"");
    // A fully-qualified program path must resolve to its basename.
    assert_not_safe("C:\\Windows\\System32\\cmd.exe /C del /f /s /q C:\\Users");
}

#[test]
fn windows_destructive_commands_are_flagged() {
    assert_not_safe("del /f /s /q /var/x");
    assert_not_safe("erase C:\\temp");
    assert_not_safe("rd /s /q C:\\temp");
    assert_not_safe("format c:");
    assert_not_safe("diskpart /s script.txt");
    assert_eq!(
        level_of("diskpart /s script.txt"),
        RiskLevel::Confirm,
        "the script is not the target of diskpart's operations"
    );
}

// --- H7: eval/source short-circuited the opaque-shell path --------------------

#[test]
fn eval_of_a_quoted_destructive_script_is_not_safe() {
    assert_not_safe("eval \"rm -rf ~\"");
    // The unquoted form was already caught; both spellings must be.
    assert_not_safe("eval rm -rf ~");
    // Sourcing a *benign* script is routine and must stay allowed; the gate
    // inspects the command it can see, not the contents of a file it cannot.
    assert_eq!(
        level_of("source ./build.sh"),
        RiskLevel::Safe,
        "false positive on sourcing a benign script"
    );
}

// --- H8: wrapper option grammars hid the payload ----------------------------

#[test]
fn unparsed_wrapper_arguments_do_not_hide_the_payload() {
    assert_not_safe("chroot /tmp/jail rm -rf ~");
    assert_not_safe("xargs -I {} rm -rf ~");
    assert_not_safe("su - root -c \"rm -rf ~\"");
    assert_not_safe("sudo -u nobody rm -rf ~");
}

// --- H6: a glob inside a protected directory was only Confirm -----------------

#[test]
fn glob_inside_protected_directory_is_catastrophic() {
    // `~/.ssh/id_*` destroys private keys exactly as `~/.ssh/*` does. The old
    // check required the glob to be the entire final path component, so this
    // fell through to the Confirm tier, which is allowed to run.
    for cmd in ["rm -rf ~/.ssh/id_*", "rm -rf ~/.aws/credentials*"] {
        assert_eq!(
            level_of(cmd),
            RiskLevel::Catastrophic,
            "glob in a protected dir should be catastrophic: {cmd}"
        );
    }
    // System paths are spelled per-platform: a POSIX literal is not absolute
    // on Windows, so the test has to use the host's own form.
    let etc_ssh = if cfg!(windows) {
        r#"rm -rf C:\Windows\System32\drivers\etc\hosts*"#
    } else {
        "rm -rf /etc/ssh/sshd_config*"
    };
    assert_eq!(
        level_of(etc_ssh),
        RiskLevel::Catastrophic,
        "glob in a protected system dir should be catastrophic: {etc_ssh}"
    );
}

// --- C3: Windows protected paths ---------------------------------------------

#[test]
fn windows_system_paths_are_protected() {
    let cases = [
        "rm -rf C:/Windows",
        "rm -rf C:/Windows/System32",
        "rm -rf c:/windows",
        r"rm -rf C:\Windows",
        // A path containing a space has to be quoted, which is how anyone
        // would actually write it. Unquoted, the shell splits it into two
        // arguments and the second is a bare relative path.
        r#"rm -rf "C:\Program Files""#,
        "rm -rf C:/ProgramData",
    ];
    for cmd in cases {
        assert_eq!(
            level_of(cmd),
            RiskLevel::Catastrophic,
            "Windows system path not protected: {cmd}"
        );
    }
}

#[test]
fn git_bash_drive_mount_is_protected() {
    // `/c/...` is how Git Bash / MSYS spell the system drive. Missing this
    // mapping meant `rm -rf /c/Users/<me>` deleted the home directory on the
    // platform where that is most likely to be typed.
    assert_eq!(level_of("rm -rf /c/Users/tester"), RiskLevel::Catastrophic);
    assert_eq!(level_of("rm -rf /c/Windows"), RiskLevel::Catastrophic);
    assert_eq!(level_of("rm -rf /c/"), RiskLevel::Catastrophic);
}

// --- no-regression: routine work must stay allowed ---------------------------

#[test]
fn ordinary_bugbounty_commands_remain_allowed() {
    // The gate fired on routine authorized recon work in an earlier version,
    // which is just as broken: a gate that denies everything is ignored.
    let mut cases = vec![
        "subfinder -d example.com -silent",
        "httpx -l hosts.txt -silent",
        "curl -sS https://example.com -o out.html",
        "nmap -sV -p 80,443 example.com",
        "cat /etc/hosts",
        "ls -la",
        "cargo test",
        "sqlmap -u https://example.com --batch",
    ];
    // Redirect targets are host-absolute, otherwise the path would be
    // resolved against the working directory and look "outside" it. Stay out
    // of `C:\Users` too, which is a protected tree by design.
    if cfg!(windows) {
        cases.push(r"echo done > C:\work\proj\log.txt");
    } else {
        cases.push("echo done > /tmp/log");
    }
    for cmd in cases {
        assert_eq!(
            level_of(cmd),
            RiskLevel::Safe,
            "false positive on routine command: {cmd}"
        );
    }
}

#[test]
fn rm_in_the_working_directory_is_not_catastrophic() {
    // `rm` is destructive, but deleting build output inside the project is
    // routine and must not be escalated.
    assert_ne!(level_of("rm -rf target/debug"), RiskLevel::Catastrophic);
    assert_ne!(level_of("rm -f main.rs"), RiskLevel::Catastrophic);
}

/// A working directory under the home directory, which is the default on
/// Windows and therefore the shape that produced the false positives.
fn home_ctx() -> RiskContext {
    RiskContext {
        working_dir: Some(if cfg!(windows) {
            std::path::PathBuf::from("C:\\Users\\tester\\OneDrive\\Desktop\\repo")
        } else {
            std::path::PathBuf::from("/home/tester/repo")
        }),
        home_dir: Some(if cfg!(windows) {
            std::path::PathBuf::from("C:\\Users\\tester")
        } else {
            std::path::PathBuf::from("/home/tester")
        }),
    }
}

/// These are verbatim commands the gate denied as "would destroy a protected
/// path". Every segment is read-only (`where`, `echo`, `ls`, `find`, `head`),
/// and the only reason the segment was inspected at all was the `2>/dev/null`
/// redirect — a write to `/dev/null`. The read paths under the home directory
/// were then graded as write targets, and `is_catastrophic_target` protects
/// everything under `C:\Users` that is outside the working directory.
#[test]
fn read_only_lookup_commands_outside_the_working_directory_are_allowed() {
    let ctx = home_ctx();
    for cmd in [
        "ls -1 /c/Users/tester/bin/nuclei.exe && echo NUCLEI_OK",
        "where -a httpx 2>/dev/null; echo \"--- searching ---\"; ls \"$HOME/go/bin\" 2>/dev/null",
        "find /c/Users/tester -maxdepth 4 -iname \"httpx.exe\" 2>/dev/null | head -10",
        "cat ~/bin/httpx 2>/dev/null",
        "grep -r pattern ~/notes 2>/dev/null",
    ] {
        assert_eq!(
            level_with(cmd, &ctx),
            RiskLevel::Safe,
            "read-only command was escalated: {cmd}"
        );
    }
}

/// The fix must not weaken genuine writes. A redirect really does truncate, so
/// redirecting into the home directory stays flagged.
#[test]
fn redirecting_into_the_home_directory_is_still_flagged() {
    let ctx = home_ctx();
    assert_ne!(
        level_with("echo pwned > ~/notes.txt", &ctx),
        RiskLevel::Safe,
        "a redirect is a truncation and must still be graded"
    );
    assert_ne!(
        level_with("cat ~/notes.txt > ~/.bashrc", &ctx),
        RiskLevel::Safe,
        "overwriting a shell rc file must still be graded"
    );
}

/// `sed -i` rewrites in place, so `sed` is deliberately not read-only.
#[test]
fn in_place_editors_are_not_treated_as_read_only() {
    let ctx = home_ctx();
    assert_ne!(
        level_with("sed -i 's/a/b/' ~/notes.txt", &ctx),
        RiskLevel::Safe,
        "`sed -i` writes in place"
    );
}

/// `find` is only read-only without `-delete` / `-exec`; those keep their own
/// grading path via [`super::CONDITIONALLY_DESTRUCTIVE`].
#[test]
fn find_is_read_only_only_without_its_destructive_flags() {
    let ctx = home_ctx();
    assert_eq!(
        level_with("find /c/Users/tester -name '*.txt'", &ctx),
        RiskLevel::Safe
    );
    assert_ne!(
        level_with("find /c/Users/tester -name '*.txt' -delete", &ctx),
        RiskLevel::Safe,
        "`find -delete` must still be graded"
    );
}

/// On a default Windows install the project lives at
/// `C:\Users\<name>\...\<repo>`, so the prefix rule for `c:/users` matched the
/// agent's own working directory and made *every* in-project delete
/// catastrophic — including the one this very test asserts is fine.
#[test]
fn deletes_inside_a_working_directory_under_c_users_are_allowed() {
    let mut ctx = RiskContext {
        working_dir: Some(if cfg!(windows) {
            std::path::PathBuf::from("C:\\Users\\tester\\projects\\repo")
        } else {
            std::path::PathBuf::from("/home/tester/projects/repo")
        }),
        home_dir: Some(if cfg!(windows) {
            std::path::PathBuf::from("C:\\Users\\tester")
        } else {
            std::path::PathBuf::from("/home/tester")
        }),
    };
    // Home is under C:\Users too, so the credential rules still apply; these
    // are ordinary project files.
    for cmd in [
        "rm -f main.rs",
        "rm -rf target",
        "rm -rf src/alphacode_command_risk",
        "del Cargo.lock",
        "rm -rf src/agent",
    ] {
        assert_ne!(
            assess(cmd, &ctx).level,
            RiskLevel::Catastrophic,
            "in-project delete must not be catastrophic: {cmd}"
        );
    }

    // ...but the working-directory exemption must not extend to the rest of
    // `C:\Users`, nor to the user's home.
    ctx.working_dir = Some(std::path::PathBuf::from("C:\\work\\proj"));
    assert_eq!(
        assess(r"rm -rf C:\Users\Public", &ctx).level,
        RiskLevel::Catastrophic,
        "other profiles under C:\\Users stay protected"
    );
    assert_eq!(
        assess(r"rm -rf C:\Users\tester", &ctx).level,
        RiskLevel::Catastrophic,
        "the home directory stays protected"
    );
}

/// `rm -rf $USERPROFILE` reached the home directory through six different
/// spellings, none of which were resolved, so the check ran against an opaque
/// string and the command executed.
#[test]
fn home_indirection_variables_resolve_to_the_protected_home() {
    let ctx = RiskContext {
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
    };
    for cmd in [
        "rm -rf $HOME",
        "rm -rf ${HOME}",
        "rm -rf $USERPROFILE",
        "rm -rf ${USERPROFILE}",
        "rm -rf $env:USERPROFILE",
        "rm -rf %USERPROFILE%",
        "rm -rf $USERPROFILE/Documents",
    ] {
        assert_eq!(
            level_of_with(cmd, &ctx),
            RiskLevel::Catastrophic,
            "home indirection must resolve and be denied: {cmd}"
        );
    }
}

fn level_of_with(command: &str, ctx: &RiskContext) -> RiskLevel {
    assess(command, ctx).level
}

/// Windows command resolution is case-insensitive, so `RM -RF ~` must not slip
/// past a list of lowercase literals.
#[test]
fn command_matching_is_case_insensitive() {
    for cmd in [
        "RM -RF ~",
        "rm -rf ~",
        "DeL /f /s /q C:\\Users",
        "del /F /S /Q C:\\Users",
        "Rd /s /q C:\\Windows",
    ] {
        assert_ne!(
            level_of(cmd),
            RiskLevel::Safe,
            "mixed-case destructive command classified Safe: {cmd}"
        );
    }
    assert_eq!(level_of("DeL /f /s /q C:\\Users"), RiskLevel::Catastrophic);
    assert_eq!(level_of("Rd /s /q C:\\Windows"), RiskLevel::Catastrophic);
}

/// A bare `.` is an ordinary path argument, not a shell. With `.` in the
/// shell-command list it was matched in argument position too, so these
/// extremely common commands were hard-denied.
#[test]
fn a_bare_dot_argument_is_not_a_shell() {
    for cmd in [
        "ls .",
        "ls -la .",
        "cat .",
        "find . -type f -name x",
        "grep -r foo .",
        "cargo fmt .",
        "du -sh .",
        "tar -cf a.tar .",
    ] {
        assert_ne!(
            level_of(cmd),
            RiskLevel::Catastrophic,
            "a bare `.` argument must not hard-deny: {cmd}"
        );
    }
}

/// `&` is a command separator; without it `echo hi & del ...` was one segment
/// whose program was `echo`. Fixing the `.` handling must not reopen that.
#[test]
fn ampersand_separates_commands() {
    for cmd in [
        r"echo hi & del /f /s /q C:\Users\x",
        r"echo hi & rd /s /q C:\Windows",
        r"echo hi && rm -rf /",
    ] {
        assert_ne!(
            level_of(cmd),
            RiskLevel::Safe,
            "destructive command after `&` must not classify Safe: {cmd}"
        );
    }
}

/// The system-root rules were written for `c:` only, so every other drive was
/// unprotected.
#[test]
fn system_roots_are_protected_on_every_drive() {
    if !cfg!(windows) {
        return;
    }
    let ctx = ctx();
    for cmd in [
        r"rm -rf D:\",
        r"rm -rf D:\Windows",
        r"rm -rf D:\Users",
        r"rm -rf E:\Windows\System32",
        r"format d:",
        r"format z:",
    ] {
        assert_ne!(
            level_of_with(cmd, &ctx),
            RiskLevel::Safe,
            "non-C: drive root must not classify Safe: {cmd}"
        );
    }
    assert_eq!(level_of_with(r"rm -rf D:\", &ctx), RiskLevel::Catastrophic);
    assert_eq!(level_of_with(r"format d:", &ctx), RiskLevel::Catastrophic);
}

#[test]
fn catastrophic_tier_is_never_downgraded() {
    // Everything in the catastrophic list must classify at Catastrophic, not
    // merely "not safe" -- Confirm is allowed to execute.
    let mut cases = vec!["rm -rf /", "rm -rf ~", "rm -rf $HOME"];
    if cfg!(windows) {
        cases.push(r"rm -rf C:\Windows");
        cases.push(r"rm -rf C:\");
    } else {
        cases.push("rm -rf /etc");
        cases.push("dd if=/dev/zero of=/dev/sda");
    }
    for cmd in cases {
        assert_eq!(
            level_of(cmd),
            RiskLevel::Catastrophic,
            "expected Catastrophic: {cmd}"
        );
    }
}
