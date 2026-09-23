use std::io::{IsTerminal, Write};
use std::process::Command as ProcessCommand;
use std::sync::Once;

static GITHUB_STAR_INIT: Once = Once::new();

const REPO_OWNER: &str = "dragonked2";
const REPO_NAME: &str = "alphacode";
const REPO_URL: &str = "https://github.com/dragonked2/alphacode";
const FLAG_FILE: &str = ".github_starred";
const CONSENT_FILE: &str = ".github_star_consent";
const SKIP_ENV: &str = "ALPHACODE_SKIP_STAR_PROMPT";

fn flag_path() -> Option<std::path::PathBuf> {
    crate::alphacode_storage::alphacode_dir()
        .ok()
        .map(|d| d.join(FLAG_FILE))
}

fn consent_path() -> Option<std::path::PathBuf> {
    crate::alphacode_storage::alphacode_dir()
        .ok()
        .map(|d| d.join(CONSENT_FILE))
}

fn already_attempted() -> bool {
    flag_path().map(|p| p.exists()).unwrap_or(true)
}

fn mark_attempted() {
    if let Some(path) = flag_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, "1");
    }
}

fn has_consented() -> bool {
    consent_path().map(|p| p.exists()).unwrap_or(false)
}

fn mark_consented() {
    if let Some(path) = consent_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, "1");
    }
}

fn gh_installed_and_authed() -> bool {
    let which = if cfg!(target_os = "windows") {
        ProcessCommand::new("where").arg("gh").output()
    } else {
        ProcessCommand::new("which").arg("gh").output()
    };

    let Ok(output) = which else {
        return false;
    };

    if !output.status.success() {
        return false;
    }

    // Verify gh is authenticated
    ProcessCommand::new("gh")
        .args(["auth", "status"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn try_star() -> bool {
    ProcessCommand::new("gh")
        .args([
            "api",
            "-X",
            "PUT",
            &format!("/user/starred/{}/{}", REPO_OWNER, REPO_NAME),
        ])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn open_repo_page_best_effort() {
    // Best-effort: let the user star manually in the browser.
    // Run on a detached thread so a slow browser launch never blocks startup.
    let url = REPO_URL.to_string();
    std::thread::Builder::new()
        .name("alphacode-open-repo".to_string())
        .spawn(move || {
            let _ = open::that(url);
        })
        .ok();
}

pub fn spawn_background_star() {
    GITHUB_STAR_INIT.call_once(|| {
        std::thread::Builder::new()
            .name("alphacode-github-star".to_string())
            .spawn(|| {
                if already_attempted() {
                    return;
                }
                if !gh_installed_and_authed() {
                    return;
                }
                if try_star() {
                    mark_attempted();
                }
            })
            .ok();
    });
}

/// Normalize raw user input to a consent decision.
///
/// Returns `Some(true)` for agree, `Some(false)` for disagree, `None` for
/// unrecognized input (caller should re-prompt).
fn normalize_choice(input: &str) -> Option<bool> {
    match input.trim().to_lowercase().as_str() {
        "y" | "yes" | "yep" | "yeah" | "agree" | "agree!" | "1" => Some(true),
        "n" | "no" | "nope" | "disagree" | "dis-agree" | "dis" | "2" | "q" | "quit" | "exit" => {
            Some(false)
        }
        _ => None,
    }
}

fn env_truthy(name: &str) -> bool {
    match std::env::var(name) {
        Ok(v) => {
            let v = v.trim().to_lowercase();
            matches!(v.as_str(), "1" | "true" | "yes" | "y" | "on")
        }
        Err(_) => false,
    }
}

fn env_present(name: &str) -> bool {
    std::env::var_os(name).is_some()
}

fn is_interactive_terminal() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// Returns true when this process should never block on a prompt:
/// tests, CI, explicit opt-out, or piped/non-TTY I/O.
fn should_skip_for_environment() -> bool {
    if cfg!(test) {
        return true;
    }
    if env_truthy(SKIP_ENV) {
        return true;
    }
    // Standard CI signal or explicit non-interactive mode.
    if env_truthy("CI") || env_present("ALPHACODE_NON_INTERACTIVE") {
        return true;
    }
    // `CI` is sometimes set to empty/false; treat any non-empty
    // value other than explicit false/0 as CI.
    if let Ok(ci) = std::env::var("CI") {
        let v = ci.trim().to_lowercase();
        if !v.is_empty() && !matches!(v.as_str(), "0" | "false" | "no" | "off") {
            return true;
        }
    }
    if !is_interactive_terminal() {
        return true;
    }
    false
}

/// Only block the main interactive TUI entry (`alphacode` with no subcommand,
/// plus a couple of interactive subcommands). Every scriptable / daemon /
/// machine-readable subcommand skips the prompt so automation never hangs.
fn is_prompt_eligible_command(args: &crate::cli::args::Args) -> bool {
    use crate::cli::args::Command;
    if args.quiet || args.internal_reload_helper {
        return false;
    }
    match &args.command {
        None => true,
        Some(Command::Repl) => true,
        Some(Command::Connect) => true,
        // Plain `alphacode --resume <id>` still has `command == None`,
        // so resume is covered by the `None` arm above.
        _ => false,
    }
}

fn print_star_prompt() {
    // Plain println!/print! (no ANSI) so the prompt renders on every terminal,
    // including Windows conhost without VT processing.
    println!();
    println!("================================================================");
    println!("  Support Alphacode - star us on GitHub?");
    println!("==================================================================");
    println!();
    println!("  If you find Alphacode useful, please star the project:");
    println!("    {}", REPO_URL);
    println!();
    println!("  Do you agree to star the project on GitHub?");
    println!("    [y] Yes, I agree  -> continue to Alphacode");
    println!("    [n] No, I disagree -> close Alphacode");
    println!();
}

/// Blocking consent gate. Call once at startup, before the TUI takes over.
///
/// - Already consented -> return immediately (normal startup).
/// - Non-interactive / CI / piped / subcommand -> return immediately.
/// - Agree -> persist consent, best-effort star, return (normal startup).
/// - Disagree (or EOF / too many invalid attempts) -> exit the process.
pub(crate) fn ensure_star_consent_or_exit(args: &crate::cli::args::Args) {
    // Fast path: user already agreed in a previous run.
    if has_consented() {
        spawn_background_star();
        return;
    }

    if !is_prompt_eligible_command(args) || should_skip_for_environment() {
        return;
    }

    print_star_prompt();

    const MAX_ATTEMPTS: usize = 5;
    for attempt in 0..MAX_ATTEMPTS {
        print!("  Choice [y/n]: ");
        let _ = std::io::stdout().flush();

        let mut input = String::new();
        match std::io::stdin().read_line(&mut input) {
            Ok(0) => {
                // EOF (e.g. stdin closed): fail closed per spec.
                println!();
                println!("  No input received. Closing Alphacode.");
                println!("  Star us later at {}", REPO_URL);
                std::process::exit(0);
            }
            Ok(_) => {}
            Err(_) => {
                println!();
                println!("  Could not read input. Closing Alphacode.");
                std::process::exit(0);
            }
        }

        match normalize_choice(&input) {
            Some(true) => {
                mark_consented();
                // Best-effort automatic star via `gh` when available.
                if gh_installed_and_authed() {
                    if try_star() {
                        mark_attempted();
                    } else {
                        // Authenticated but the API call failed (offline, etc.):
                        // still honor the consent and let the background
                        // retry handle it; open the repo so the user can
                        // star manually.
                        spawn_background_star();
                        open_repo_page_best_effort();
                    }
                } else {
                    // No `gh` CLI: open the repo page so the user can star
                    // manually in one click.
                    open_repo_page_best_effort();
                }
                println!();
                println!("  Thank you for supporting Alphacode!");
                println!();
                return;
            }
            Some(false) => {
                println!();
                println!("  You chose not to star {}. Closing Alphacode.", REPO_URL);
                println!("  Re-run `alphacode` anytime to reconsider.");
                std::process::exit(0);
            }
            None => {
                if attempt + 1 >= MAX_ATTEMPTS {
                    println!();
                    println!("  Too many invalid attempts. Closing Alphacode.");
                    std::process::exit(0);
                }
                println!("  Please type 'y' to agree or 'n' to disagree.");
            }
        }
    }

    // Unreachable (loop always returns or exits), but fail closed anyway.
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flag_path_is_some() {
        assert!(flag_path().is_some());
    }

    #[test]
    fn consent_path_is_some() {
        assert!(consent_path().is_some());
    }

    #[test]
    fn agree_inputs_map_to_true() {
        for input in ["y", "Y", "yes", "YES", "  Yes  ", "agree", "AGREE", "1"] {
            assert_eq!(normalize_choice(input), Some(true), "input={input:?}");
        }
    }

    #[test]
    fn disagree_inputs_map_to_false() {
        for input in [
            "n", "N", "no", "NO", "  No  ", "disagree", "DISAGREE", "2", "q", "quit", "exit",
        ] {
            assert_eq!(normalize_choice(input), Some(false), "input={input:?}");
        }
    }

    #[test]
    fn invalid_inputs_map_to_none() {
        for input in ["", " ", "maybe", "yesss", "11", "yn"] {
            assert_eq!(normalize_choice(input), None, "input={input:?}");
        }
    }

    #[test]
    fn skip_env_truthy_values_detected() {
        // env_truthy only depends on the env var; set/restore around the check.
        let key = "ALPHACODE_TEST_SKIP_STAR_PROMPT_TMP";
        for truthy in ["1", "true", "TRUE", "yes", "y", "on"] {
            unsafe { std::env::set_var(key, truthy) };
            assert!(env_truthy(key), "value={truthy:?}");
        }
        for falsy in ["0", "false", "no", "", "maybe"] {
            unsafe { std::env::set_var(key, falsy) };
            assert!(!env_truthy(key), "value={falsy:?}");
        }
        unsafe { std::env::remove_var(key) };
        assert!(!env_truthy(key));
    }
}
