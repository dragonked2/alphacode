//! Deterministic risk classification for shell commands.
//!
//! # Why this exists
//!
//! alphacode executes `bash` tool calls with no gate of its own: the only check in
//! `ToolRegistry::execute` is an opt-in external `pre_tool` hook, which is off
//! by default. A model that decides to run `rm -rf ~` is obeyed immediately.
//! That is issue #604, where a user lost their home directory.
//!
//! # Design
//!
//! This crate is **stage 1** of a two-stage cascade: a cheap, deterministic,
//! high-recall filter. It never calls a model and never touches the network, so
//! it costs nothing on the overwhelmingly common safe path. Stage 2 (the
//! reflection gate) only runs when this returns something other than
//! [`RiskLevel::Safe`].
//!
//! Two deliberate choices:
//!
//! 1. **Classify by blast radius, not by command name.** A denylist of
//!    `rm -rf` misses `find -delete`, `shred`, `truncate`, `dd`, and `>file`.
//!    We ask "what would this destroy, and can it be undone" instead.
//! 2. **Bias hard toward recall.** A false positive costs one reflection turn.
//!    A false negative costs a home directory. When parsing is ambiguous we
//!    escalate rather than allow.
//!
//! # Honest limitations
//!
//! This is defense in depth, not a sandbox. A determined or unlucky
//! `sh -c "$(printf ...)"` can defeat any static parser, which is exactly why
//! [`RiskLevel::Confirm`] is a reflection prompt rather than a hard block, and
//! why the catastrophic tier is a small, absolute, path-based deny that does
//! not depend on parsing the command correctly.

mod gate;
mod paths;
mod shell_url_safety;
mod tokenize;

// `paths_tests` / `tokenize_tests` are attached to their own modules (they use
// `super::` to reach private helpers), so they are not registered here.
#[cfg(test)]
#[path = "bypass_tests.rs"]
mod bypass_tests;
// The other half of the contract: a gate that denies ordinary work is as broken
// as one that admits destructive work, and a `Catastrophic` verdict cannot be
// justified past.
#[cfg(test)]
#[path = "false_positive_tests.rs"]
mod false_positive_tests;

pub use gate::{GateOutcome, Justification, gate};
pub use paths::{ProtectedPaths, is_catastrophic_target};
#[allow(unused_imports)]
pub use shell_url_safety::{ShellUrlSafety, scan_for_shell_url_issues};
pub use tokenize::{Token, tokenize};

/// How dangerous a command looks, and therefore how much scrutiny it earns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RiskLevel {
    /// No destructive potential detected. Run immediately, no overhead.
    Safe,
    /// Destructive but bounded (inside the working directory, recoverable via
    /// git, or under a temp dir). Run, but record it.
    Low,
    /// Irreversible and reaches outside the working directory. Requires the
    /// model to re-justify against the user's actual request before running.
    Confirm,
    /// Would destroy the user's home, root, or credentials. Never runs, and no
    /// amount of model justification can unlock it.
    Catastrophic,
}

impl RiskLevel {
    /// Whether execution may proceed without a reflection turn.
    pub fn runs_immediately(self) -> bool {
        matches!(self, RiskLevel::Safe | RiskLevel::Low)
    }

    /// Whether any confirmation could ever unlock this.
    pub fn is_absolute_deny(self) -> bool {
        matches!(self, RiskLevel::Catastrophic)
    }
}

/// A specific reason a command was flagged, used to explain the refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskFinding {
    pub level: RiskLevel,
    /// Human-readable explanation, shown to the model verbatim.
    pub reason: String,
    /// The concrete path or argument that triggered this, when there is one.
    pub target: Option<String>,
}

/// The full verdict for one command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RiskAssessment {
    pub level: RiskLevel,
    pub findings: Vec<RiskFinding>,
}

impl RiskAssessment {
    fn safe() -> Self {
        Self {
            level: RiskLevel::Safe,
            findings: Vec::new(),
        }
    }

    fn from_findings(findings: Vec<RiskFinding>) -> Self {
        let level = findings
            .iter()
            .map(|f| f.level)
            .max()
            .unwrap_or(RiskLevel::Safe);
        Self { level, findings }
    }

    /// The refusal text shown to the model, phrased to force a comparison
    /// against what the user actually asked for rather than a yes/no reflex.
    pub fn explanation(&self) -> String {
        let mut out = String::new();
        for finding in &self.findings {
            out.push_str("- ");
            out.push_str(&finding.reason);
            if let Some(target) = &finding.target {
                out.push_str(&format!(" (target: {target})"));
            }
            out.push('\n');
        }
        out
    }
}

/// Context needed to judge blast radius. Supplied by the caller because this
/// crate deliberately does no I/O of its own beyond path inspection.
#[derive(Debug, Clone, Default)]
pub struct RiskContext {
    /// The tool call's working directory, if any.
    pub working_dir: Option<std::path::PathBuf>,
    /// The user's home directory.
    pub home_dir: Option<std::path::PathBuf>,
}

impl RiskContext {
    pub fn from_env(working_dir: Option<std::path::PathBuf>) -> Self {
        Self {
            working_dir,
            home_dir: dirs_home(),
        }
    }
}

/// Resolve the user's home directory.
///
/// `$HOME` wins when set so a caller can override it (and so the Unix shell's
/// own notion of `~` is honoured), but it must not be the only source: on
/// Windows `HOME` is normally unset and the home directory lives in
/// `USERPROFILE`. Falling through to `dirs::home_dir()` — the idiom used
/// everywhere else in this codebase — keeps the catastrophic tier armed there.
/// Without the fallback `home_dir` is `None` on Windows, and every home-path
/// protection (`~`, `~/.ssh`, `~/.aws`, `~/.gnupg`) silently degrades to
/// "not catastrophic".
fn dirs_home() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME")
        .filter(|h| !h.is_empty())
        .map(std::path::PathBuf::from)
        .or_else(dirs::home_dir)
}

/// Case-insensitive membership test for [`DESTRUCTIVE_COMMANDS`].
///
/// Windows command names are case-insensitive and PowerShell cmdlets are
/// conventionally PascalCase (`Remove-Item`), so a case-sensitive lookup left
/// every PowerShell destructive cmdlet unmatched — the exact class of command
/// the bash tool's own schema tells the model to use.
fn is_destructive_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    // `mkfs.ext4`, `wipefs.sha256`, `del.exe` are the real program names on
    // disk; the bare stem is what the list holds.
    let stem = lower.split_once('.').map(|(s, _)| s).unwrap_or(&lower);
    DESTRUCTIVE_COMMANDS.contains(&lower.as_str()) || DESTRUCTIVE_COMMANDS.contains(&stem)
}

/// Commands that destroy data as their primary purpose.
///
/// Presence here does not by itself mean danger: `rm` inside the working
/// directory is routine. It means "inspect the targets".
const DESTRUCTIVE_COMMANDS: &[&str] = &[
    // POSIX. Matched on the stem too, so `mkfs.ext4` and `wipefs.sha256`
    // resolve to their `mkfs` / `wipefs` entries.
    "rm",
    "rmdir",
    "shred",
    "unlink",
    "truncate",
    "dd",
    "mkfs",
    "fdisk",
    "parted",
    "wipefs",
    "srm",
    // Windows commands. `del`/`erase` remove files, `rd` removes trees,
    // `format`/`diskpart` destroy volumes. These were missing entirely, so
    // `del /f /s /q C:\Users\me\Documents` classified as Safe on the platform
    // where the gate matters most.
    "del",
    "erase",
    "rd",
    "format",
    "diskpart",
    "cipher",
    "fsutil",
    "del.exe",
    // PowerShell cmdlets. `Remove-Item -Recurse -Force` is `rm -rf`, and the
    // bash tool's own schema tells the model to reach for
    // `powershell -Command '...'`, so omitting these left the documented
    // Windows path completely unguarded.
    "remove-item",
    "clear-content",
    "set-content",
    "out-file",
    "new-item",
    "format-volume",
    "initialize-disk",
    "remove-partition",
    "clear-disk",
    "remove-vm",
    "stop-vm",
    "set-mppreference",
    "takeown",
];

/// Commands that run another command. The real program is one of their
/// arguments, so `sudo rm -rf ~` must be unwrapped before classification or the
/// destructive verb is never seen at all.
///
/// `eval` is deliberately absent: it is not a transparent wrapper but a
/// *string* evaluator, so `eval "rm -rf ~"` must be handled by the
/// opaque-shell path instead. Listing it here consumed the token and left the
/// quoted script as the "program name", which matched nothing.
const WRAPPER_COMMANDS: &[&str] = &[
    "sudo", "doas", "env", "nice", "ionice", "time", "timeout", "nohup", "xargs", "command",
    "builtin", "exec", "setsid", "stdbuf", "chroot", "su", "watch",
];

/// Wrapper options that consume the following word as their value.
const WRAPPER_FLAGS_WITH_VALUES: &[&str] = &[
    "-n",
    "-u",
    "-s",
    "-c",
    "-k",
    "--signal",
    "--adjustment",
    "--user",
];

/// Shells, which take their program from a string argument we cannot parse
/// reliably. Treated as opaque rather than assumed safe.
///
/// The Windows shells belong here too. `powershell -Command "Remove-Item
/// -Recurse -Force $env:USERPROFILE"` was previously invisible to this gate
/// (and the tool schema actively *instructs* the model to reach for
/// `powershell -Command` / `cmd.exe /C`), so it is a bypass on the platform
/// where it is most dangerous.
const SHELL_COMMANDS: &[&str] = &[
    "sh",
    "bash",
    "zsh",
    "dash",
    "ksh",
    "fish",
    // Windows shells + string evaluators.
    "powershell",
    "powershell.exe",
    "pwsh",
    "pwsh.exe",
    "cmd",
    "cmd.exe",
    "eval",
    "source",
];

/// Whether `name`, appearing in *program position*, is a shell or string
/// evaluator whose real program cannot be parsed reliably.
///
/// `.` (the POSIX `source` builtin) is handled here rather than in
/// [`SHELL_COMMANDS`] because that list is also consulted in *argument*
/// position by the hidden-verb scan. With `.` in the list, every bare `.`
/// argument matched, so `ls .`, `ls -la .`, `find . -type f`, `grep -r foo .`
/// and `cargo fmt .` were all hard-denied as `Catastrophic` — among the most
/// common commands an agent emits. As a program name, `. script.sh` is still
/// correctly treated as opaque.
fn is_shell_program(name: &str) -> bool {
    SHELL_COMMANDS.contains(&name) || name == "."
}

/// Commands that are destructive only with specific flags.
const CONDITIONALLY_DESTRUCTIVE: &[(&str, &[&str])] = &[
    ("find", &["-delete", "-exec"]),
    ("git", &["clean"]),
    ("chmod", &["-R"]),
    ("chown", &["-R"]),
    // `sed -i` rewrites its input in place. `sed` is therefore excluded from
    // [`READ_ONLY_COMMANDS`], which is only coherent if the `-i` form is
    // actually graded — without this it classified as Safe.
    ("sed", &["-i"]),
];

/// Programs that only read: inspecting them cannot damage anything, so their
/// path arguments are not write targets and must not be graded as such.
///
/// Kept deliberately tight. Anything that can write is excluded, even in a
/// mode that is usually read-only:
/// * `sed` and `awk` — `sed -i` rewrites in place;
/// * `install`, `tee`, `dd` — write by design (`dd` is already destructive);
/// * every program in [`DESTRUCTIVE_COMMANDS`] and
///   [`CONDITIONALLY_DESTRUCTIVE`], which are graded by their own rules;
/// * interpreters (`python`, `perl`, `node`) — arbitrary side effects.
///
/// An unknown program is treated as *not* read-only, so this can only ever
/// preserve today's behaviour for anything not deliberately listed.
const READ_ONLY_COMMANDS: &[&str] = &[
    // Listing / navigating.
    "ls",
    "dir",
    "pwd",
    "tree",
    "find",
    "locate",
    // Reading file contents.
    "cat",
    "bat",
    "type",
    "head",
    "tail",
    "tac",
    "less",
    "more",
    "nl",
    "od",
    "xxd",
    "hexdump",
    "strings",
    "column",
    // Searching.
    "grep",
    "egrep",
    "fgrep",
    "rg",
    "ag",
    "ack",
    // Text transforms that cannot write without a redirect (graded separately).
    "sort",
    "uniq",
    "cut",
    "tr",
    "rev",
    "wc",
    // Metadata.
    "file",
    "stat",
    "du",
    "df",
    "basename",
    "dirname",
    "realpath",
    "readlink",
    "md5sum",
    "sha1sum",
    "sha256sum",
    "cksum",
    "diff",
    "cmp",
    // Environment / identity.
    "echo",
    "printf",
    "env",
    "printenv",
    "whoami",
    "id",
    "hostname",
    "uname",
    "date",
    "which",
    "where",
    "whereis",
    "jq",
    "man",
    "info",
];

/// Case-insensitive membership test for [`READ_ONLY_COMMANDS`].
///
/// Matches on the stem like [`is_destructive_name`], so `ls.exe` and
/// `find.exe` resolve to their entries.
fn is_read_only_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let stem = lower.split_once('.').map(|(s, _)| s).unwrap_or(&lower);
    READ_ONLY_COMMANDS.contains(&lower.as_str()) || READ_ONLY_COMMANDS.contains(&stem)
}

/// Whether a token is a `NAME=value` environment assignment rather than a word.
///
/// Requires a valid shell identifier before the `=`. A loose `contains('=')`
/// would also swallow legitimate path arguments like `a=b.txt` or
/// `?filter=name`, hiding the real program.
fn is_assignment(text: &str) -> bool {
    let Some((name, _)) = text.split_once('=') else {
        return false;
    };
    if name.is_empty() {
        return false;
    }
    let mut chars = name.chars();
    let first = chars.next().expect("name is non-empty");
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Drop leading `VAR=value` tokens from a segment.
fn strip_leading_assignments(mut tokens: &[Token]) -> &[Token] {
    while let Some(first) = tokens.first() {
        if is_assignment(&first.text) {
            tokens = &tokens[1..];
        } else {
            break;
        }
    }
    tokens
}

/// Assess a single shell command string.
///
/// This is the crate's entry point and is intentionally total: any input,
/// including garbage, produces an assessment rather than an error.
pub fn assess(command: &str, ctx: &RiskContext) -> RiskAssessment {
    let mut findings = Vec::new();

    for segment in tokenize::split_segments(command) {
        assess_segment(&segment, ctx, &mut findings);
    }

    if findings.is_empty() {
        return RiskAssessment::safe();
    }
    RiskAssessment::from_findings(findings)
}

fn assess_segment(tokens: &[Token], ctx: &RiskContext, findings: &mut Vec<RiskFinding>) {
    // Leading `VAR=value` assignments set the environment for the command that
    // follows and are not the command. They must be stripped *before* the
    // wrapper loop: the loop only skipped them after a wrapper program, so a
    // segment that begins with an assignment never got unwrapped and its
    // `program_name` became the assignment itself. `LANG=C rm -rf ~` therefore
    // classified as Safe.
    let mut tokens = strip_leading_assignments(tokens);

    // Strip wrapper programs (`sudo`, `env`, `xargs`, ...) so the destructive
    // verb underneath is the one we classify. Without this, any common prefix
    // is a complete bypass.
    let mut wrapped_by: Option<String> = None;
    loop {
        let Some(first) = tokens.first() else {
            // Ran off the end while unwrapping: the payload is invisible.
            if let Some(wrapper) = wrapped_by {
                findings.push(RiskFinding {
                    level: RiskLevel::Confirm,
                    reason: format!(
                        "`{wrapper}` runs another command that could not be \
                         identified statically"
                    ),
                    target: None,
                });
            }
            return;
        };
        let name = first.basename();
        if !WRAPPER_COMMANDS.contains(&name.as_str()) {
            break;
        }
        wrapped_by = Some(name.clone());
        // Skip the wrapper plus its own options and `VAR=value` assignments,
        // landing on the wrapped program. Options that take a separate value
        // (`nice -n 10`, `timeout 5`) must consume that value too.
        let rest = &tokens[1..];
        let mut idx = 0;
        while idx < rest.len() {
            let token = &rest[idx];
            if token.is_operator || is_assignment(&token.text) {
                idx += 1;
                continue;
            }
            if token.is_flag_for(&name) {
                idx += 1;
                // A short flag known to take an argument consumes the next word.
                if WRAPPER_FLAGS_WITH_VALUES.contains(&token.text.as_str()) && idx < rest.len() {
                    idx += 1;
                }
                continue;
            }
            // A bare number is an operand of the wrapper itself (`timeout 5`),
            // not the program to run.
            if token.text.chars().all(|c| c.is_ascii_digit() || c == '.') {
                idx += 1;
                continue;
            }
            break;
        }
        tokens = &rest[idx..];
    }
    let Some(program) = tokens.first() else {
        // A wrapper with nothing recognizable after it hides its payload.
        if let Some(wrapper) = wrapped_by {
            findings.push(RiskFinding {
                level: RiskLevel::Confirm,
                reason: format!(
                    "`{wrapper}` runs another command that could not be \
                     identified statically"
                ),
                target: None,
            });
        }
        return;
    };
    let program_name = program.basename();

    // `diskpart /s <file>` executes a language of partition and volume
    // operations from the file; the script path is not the affected target.
    // Its contents are opaque here, so require confirmation before any
    // diskpart invocation rather than grading the script file as a write path.
    if program_name
        .split_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(&program_name)
        == "diskpart"
    {
        findings.push(RiskFinding {
            level: RiskLevel::Confirm,
            reason: "`diskpart` can repartition or format disks, and its effects cannot be determined statically".to_string(),
            target: tokens
                .iter()
                .skip(1)
                .find(|token| !token.is_flag_for(&program_name) && !token.is_operator)
                .map(|token| token.text.clone()),
        });
        return;
    }

    // `mkfs.ext4`, `mkfs.xfs`, `mke2fs`, `wipefs.sha256` etc. are the real
    // program names on disk — the bare stem in the list never matches, so
    // `mkfs.ext4 /dev/sda1` classified as Safe. `is_destructive_name` also
    // lowercases, because Windows command names are case-insensitive and
    // PowerShell cmdlets are conventionally PascalCase.
    let is_destructive = is_destructive_name(&program_name);

    // Defense in depth for wrappers whose option grammar we could not parse.
    // The unwrap loop stops at the first word it does not recognise as a
    // wrapper operand, so `chroot /tmp/jail rm -rf ~`, `su - root -c "..."` and
    // `xargs -I {} rm -rf ~` all leave a *non-program* in slot 0 and hide the
    // destructive verb behind it. Rather than trying to model every wrapper's
    // flag grammar, scan the whole segment for a known destructive verb; if one
    // is present the segment is unsafe no matter what we think runs first.
    //
    // Two exclusions keep this from denying ordinary work. Before, *any*
    // occurrence of a shell name anywhere in the segment was Catastrophic, so
    // `grep -rn "sh" src/`, `git commit -m "fix the sh wrapper"` and
    // `echo "run bash later"` were all hard-denied -- and Catastrophic is
    // explicitly "no amount of model justification can unlock it", so the agent
    // had no way forward. Measured on one session that denied 6 of 7 ordinary
    // commands purely for *mentioning* a shell:
    //
    //   * A word that was quoted in the original command is data, not code. A
    //     shell name in a grep pattern, a commit message or an echo body is
    //     never executed. `docker run img sh -c "rm -rf ~"` still fires: the
    //     `sh` there is its own unquoted token, and the quoted part is the
    //     script, which the program-position branch below assesses directly.
    //   * A program that only reads cannot execute anything, so its arguments
    //     are inert by construction -- as long as it is not in one of its
    //     destructive forms. `find -exec sh -c "..."` is exactly why `find`
    //     needs the conditional-flag carve-out.
    let conditional_flags_now = CONDITIONALLY_DESTRUCTIVE
        .iter()
        .find(|(name, _)| *name == program_name)
        .map(|(_, flags)| *flags);
    let in_destructive_form = conditional_flags_now
        .is_some_and(|flags| tokens.iter().any(|t| flags.contains(&t.text.as_str())));
    let program_is_inert = !in_destructive_form
        && READ_ONLY_COMMANDS.contains(&program_name.as_str())
        && !is_shell_program(&program_name);

    if !is_destructive && !is_shell_program(&program_name) && !program_is_inert {
        let hidden = tokens.iter().find_map(|t| {
            // A quoted argument can itself be a whole command string
            // (`su - root -c "rm -rf ~"`), so the scan has to look *inside*
            // each token as well as at the token itself.
            let mut words: Vec<String> = t.text.split_whitespace().map(str::to_string).collect();
            words.push(t.basename());
            for name in words {
                let name = name.rsplit(['/', '\\']).next().unwrap_or(&name).to_string();
                let is_destructive_word = is_destructive_name(&name);
                let is_shell_word = SHELL_COMMANDS.contains(&name.as_str());
                // Quoted text is inert data unless the word *itself* is the
                // quoted shell invocation, which the token's basename still
                // shows: `"sh" -c "..."` on a system that quotes the program.
                if is_destructive_word || (is_shell_word && !t.was_quoted) {
                    return Some(name);
                }
            }
            None
        });
        if let Some(name) = hidden {
            findings.push(RiskFinding {
                level: RiskLevel::Catastrophic,
                reason: format!(
                    "`{name}` appears inside a `{program_name}` invocation whose \
                     real command line could not be resolved statically"
                ),
                target: None,
            });
            return;
        }
    }

    // A shell invoked with an inline script is opaque to this parser. Assess
    // the script text too, so `sh -c "rm -rf ~"` is not a free pass.
    if is_shell_program(&program_name) {
        for token in tokens
            .iter()
            .skip(1)
            .filter(|t| !t.is_flag_for(&program_name))
        {
            for segment in tokenize::split_segments(&token.text) {
                assess_segment(&segment, ctx, findings);
            }
        }
        return;
    }

    // `is_destructive` (including the `mkfs.ext4`-style stem match) is computed
    // once, above, before the shell handling.
    let conditional_flags = CONDITIONALLY_DESTRUCTIVE
        .iter()
        .find(|(name, _)| *name == program_name)
        .map(|(_, flags)| *flags);

    let triggered = if is_destructive {
        true
    } else if let Some(flags) = conditional_flags {
        tokens.iter().any(|t| flags.contains(&t.text.as_str()))
    } else {
        false
    };

    // Output redirection truncates a file even with a harmless program.
    let redirect_targets: Vec<&Token> = tokens
        .iter()
        .filter(|t| t.is_truncating_redirect_target)
        .collect();

    if !triggered && redirect_targets.is_empty() {
        return;
    }

    // A read-only program has no write targets among its operands, so grading
    // them as if it did is wrong. `is_catastrophic_target` protects everything
    // under `C:\Users` that is outside the working directory, so grading the
    // operands of a plain lookup turned routine commands into hard denials:
    //
    //   ls ~/go/bin 2>/dev/null
    //   where -a httpx; find ~/ -maxdepth 4 -iname "httpx.exe" 2>/dev/null
    //
    // were both reported as "would destroy a protected path" — the `2>/dev/null`
    // redirect is a write, so the segment was inspected at all, and then every
    // *read* path in it was graded as a write target. The redirect target is
    // still graded (a redirect really does truncate), which is why this only
    // skips the non-redirect operands.
    let read_only = is_read_only_name(&program_name) && !triggered;

    let mut targets: Vec<&Token> = if read_only {
        Vec::new()
    } else {
        tokens
            .iter()
            .skip(1)
            .filter(|t| !t.is_flag_for(&program_name) && !t.is_operator)
            .collect()
    };
    targets.extend(redirect_targets.iter().copied());

    // A destructive command fed by a pipe takes its operands from the previous
    // command's output, which we cannot enumerate. `find ~ -type f | xargs rm`
    // is a real deletion of home contents that neither segment reveals on its
    // own, so escalate rather than trust the visible arguments.
    if triggered && tokens.first().is_some_and(|t| t.receives_pipe) {
        findings.push(RiskFinding {
            level: RiskLevel::Confirm,
            reason: format!(
                "`{program_name}` deletes paths supplied by a pipe, so the set \
                 of affected files cannot be checked before it runs"
            ),
            target: None,
        });
    }

    // A destructive program with no parsable target is more suspicious, not
    // less: we could not see what it would touch.
    if triggered && targets.is_empty() {
        findings.push(RiskFinding {
            level: RiskLevel::Confirm,
            reason: format!(
                "`{program_name}` is destructive but its target could not be \
                 determined statically, so its blast radius is unknown"
            ),
            target: None,
        });
        return;
    }

    let recursive = tokens
        .iter()
        .any(|t| t.is_recursive_flag_for(&program_name));

    for target in targets {
        // `dd`-style `key=value` operands hide the path from a naive scan.
        let raw = target
            .text
            .split_once('=')
            .filter(|(key, _)| matches!(*key, "of" | "if" | "seek" | "conv"))
            .map(|(_, value)| value)
            .unwrap_or(&target.text);
        let expanded = paths::expand(raw, ctx);
        if let Some(finding) = paths::classify_target(&expanded, raw, recursive, ctx) {
            findings.push(finding);
        }
    }
}
