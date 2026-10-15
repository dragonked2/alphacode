use std::ffi::OsStr;

/// Mutate the process environment for alphacode runtime configuration.
///
/// Rust 2024 makes environment mutation unsafe because it can race with
/// concurrent environment access in foreign code. alphacode intentionally mutates
/// process-local env vars to coordinate provider/runtime bootstrap before or
/// during task execution. We centralize that unsafety here so call sites remain
/// auditable.
pub fn set_var<K, V>(key: K, value: V)
where
    K: AsRef<OsStr>,
    V: AsRef<OsStr>,
{
    // SAFETY: alphacode treats these mutations as process-global configuration.
    // They are a pre-existing design choice used throughout startup, auth,
    // provider bootstrap, tests, and self-dev flows. Centralizing the unsafe
    // operation here makes the Rust 2024 requirement explicit without
    // scattering unsafe blocks across hundreds of call sites.
    unsafe {
        std::env::set_var(key, value);
    }
}

/// Remove a process environment variable used by alphacode runtime configuration.
pub fn remove_var<K>(key: K)
where
    K: AsRef<OsStr>,
{
    // SAFETY: see `set_var` above; this is the corresponding centralized
    // removal operation for the same process-global configuration surface.
    unsafe {
        std::env::remove_var(key);
    }
}

/// Environment variables that describe how alphacode resolves and routes a
/// request.
///
/// AlphaCode sets several of these itself during startup and provider
/// activation (see `provider_catalog` and `provider::activation`), and the
/// process environment is then inherited by every child process. These
/// variables are alphacode's own runtime configuration, not part of the
/// environment contract that tools run under.
///
/// Tool subprocesses therefore get a process environment without them: a shell
/// command, browser driver, or recon tool has no use for alpha's provider and
/// routing state, and including it would expose internal configuration as
/// ordinary tool output.
///
/// Deliberately excluded: tool plumbing tools do rely on
/// (`ALPHACODE_SCRATCH_DIR`), self-dev/build plumbing, and credential
/// variables. Those are separate concerns with their own blast radius.
pub const INTERNAL_ROUTING_ENV_VARS: &[&str] = &[
    // Active identity
    "ALPHACODE_MODEL",
    "ALPHACODE_PROVIDER",
    "ALPHACODE_ACTIVE_PROVIDER",
    "ALPHACODE_RUNTIME_PROVIDER",
    "ALPHACODE_PROVIDER_PROFILE_ACTIVE",
    "ALPHACODE_PROVIDER_PROFILE_NAME",
    "ALPHACODE_NAMED_PROVIDER_PROFILE",
    "ALPHACODE_INITIAL_PROVIDER_EXPLICIT",
    // Per-role model selection
    "ALPHACODE_AMBIENT_MODEL",
    "ALPHACODE_AMBIENT_PROVIDER",
    "ALPHACODE_AUTOJUDGE_MODEL",
    "ALPHACODE_AUTOREVIEW_MODEL",
    "ALPHACODE_SWARM_MODEL",
    "ALPHACODE_MEMORY_MODEL",
    "ALPHACODE_ISSUE_DRIVEN_LIVE_MODEL",
    "ALPHACODE_ISSUE_DRIVEN_LIVE_PROVIDER",
    "ALPHACODE_OPENCODE_ZEN_LIVE_TOOL_MODEL",
    // Provider model pins
    "ALPHACODE_ANTHROPIC_MODEL",
    "ALPHACODE_ANTIGRAVITY_MODEL",
    "ALPHACODE_BEDROCK_MODEL",
    "ALPHACODE_CLAUDE_CLI_MODEL",
    "ALPHACODE_COPILOT_MODEL",
    "ALPHACODE_CURSOR_MODEL",
    "ALPHACODE_GEMINI_MODEL",
    "ALPHACODE_OPENAI_MODEL",
    "ALPHACODE_OPENAI_COMPAT_DEFAULT_MODEL",
    "ALPHACODE_MEMORY_EMBEDDING_MODEL",
    // OpenRouter routing internals
    "ALPHACODE_OPENROUTER_MODEL",
    "ALPHACODE_OPENROUTER_STATIC_MODELS",
    "ALPHACODE_OPENROUTER_MODEL_CATALOG",
    "ALPHACODE_OPENROUTER_PROVIDER",
    "ALPHACODE_OPENROUTER_PROVIDER_FEATURES",
    "ALPHACODE_OPENROUTER_TRANSPORT_STATE",
    "ALPHACODE_OPENROUTER_API_BASE",
    "ALPHACODE_OPENROUTER_ENV_FILE",
    "ALPHACODE_OPENROUTER_CONTEXT_WINDOW",
    "ALPHACODE_OPENROUTER_MAX_TOKENS",
    "ALPHACODE_OPENROUTER_CACHE_NAMESPACE",
    "ALPHACODE_OPENROUTER_NO_FALLBACK",
    "ALPHACODE_OPENROUTER_ALLOW_NO_AUTH",
    "ALPHACODE_OPENROUTER_AUTH_HEADER",
    "ALPHACODE_OPENROUTER_AUTH_HEADER_NAME",
    "ALPHACODE_OPENROUTER_DYNAMIC_BEARER_PROVIDER",
    "ALPHACODE_CROSS_PROVIDER_FAILOVER",
    "ALPHACODE_SAME_PROVIDER_ACCOUNT_FAILOVER",
    // Catalog / bootstrap routing state
    "ALPHACODE_REMOTE_BOOTSTRAP_MODEL_CATALOG",
    "ALPHACODE_DISABLE_CATALOG_REFRESH",
    "ALPHACODE_LOG_SERVED_MODEL",
];

/// AlphaCode's own copies of credential material.
///
/// Every one of these is a *secret or a pointer to one*. AlphaCode holds them
/// in its own process environment for provider auth, and the environment is
/// inherited by tool subprocesses, so a tool output could contain them. Tool
/// output is rendered straight into the transcript, so anything in it should
/// be something the user is comfortable reading and copying.
///
/// Scope is deliberately narrow: only `ALPHACODE_*`. A user's own
/// `OPENAI_API_KEY` / `ANTHROPIC_API_KEY` / `GITHUB_TOKEN` are theirs to place
/// in the environment of the tools they run, and alphacode does not get to
/// decide that. The `ALPHACODE_`-prefixed entries are alphacode's working
/// copies of the same material.
pub const ALPHACODE_CREDENTIAL_ENV_VARS: &[&str] = &[
    "ALPHACODE_API_KEY",
    "ALPHACODE_ANTHROPIC_API_KEY",
    "ALPHACODE_ANTIGRAVITY_CLIENT_SECRET",
    "ALPHACODE_BING_API_KEY",
    "ALPHACODE_BING_API_KEY_ENV",
    "ALPHACODE_COPILOT_ALLOW_GH_AUTH_TOKEN",
    "ALPHACODE_DISCORD_BOT_TOKEN",
    "ALPHACODE_TELEGRAM_BOT_TOKEN",
    "ALPHACODE_JADE_RELAY_TOKEN",
    "ALPHACODE_JADE_RELAY_TOKEN_ID",
    "ALPHACODE_SMTP_PASSWORD",
    "ALPHACODE_OPENAI_COMPAT_API_KEY_NAME",
    "ALPHACODE_OPENROUTER_API_KEY_NAME",
    "ALPHACODE_OPENROUTER_AUTH_HEADER",
    "ALPHACODE_OPENROUTER_AUTH_HEADER_NAME",
    "ALPHACODE_OPENROUTER_DYNAMIC_BEARER_PROVIDER",
    "ALPHACODE_AUTH_LIFECYCLE_CEREBRAS_API_KEY",
    "ALPHACODE_AUTH_LIFECYCLE_OPENCODE_API_KEY",
];

/// Anything that can drop an environment variable from a not-yet-spawned
/// process.
///
/// Implemented for both `std::process::Command` and
/// `tokio::process::Command` so the environment filter is a single call
/// regardless of which runtime a tool spawns through.
pub trait CommandEnv {
    fn remove_env(&mut self, key: &str);
}

impl CommandEnv for std::process::Command {
    fn remove_env(&mut self, key: &str) {
        self.env_remove(key);
    }
}

impl CommandEnv for tokio::process::Command {
    fn remove_env(&mut self, key: &str) {
        self.env_remove(key);
    }
}

/// Build a command whose child process does not inherit alphacode's internal
/// configuration.
///
/// Removes both [`INTERNAL_ROUTING_ENV_VARS`] (routing and model identity)
/// and [`ALPHACODE_CREDENTIAL_ENV_VARS`] (alpha's working copies of secrets).
/// Apply this to processes the agent can start — shell, python, browser,
/// scrapers, recon tools. Do **not** apply it to alphacode's own respawn paths
/// (`/reload`, self-dev, ACP bootstrap): those start a new AlphaCode process
/// that needs this configuration to rebuild its provider state.
///
/// Tool output is presented to the user as ordinary program output, so the
/// tool environment should contain what tools need and nothing that belongs to
/// alpha's internals.
pub fn strip_internal_env<T: CommandEnv>(command: &mut T) {
    for key in INTERNAL_ROUTING_ENV_VARS {
        command.remove_env(key);
    }
    for key in ALPHACODE_CREDENTIAL_ENV_VARS {
        command.remove_env(key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routing_list_has_no_duplicates_and_no_blank_keys() {
        let mut seen = std::collections::HashSet::new();
        for key in INTERNAL_ROUTING_ENV_VARS {
            assert!(!key.trim().is_empty(), "blank routing env key");
            assert!(
                key.starts_with("ALPHACODE_"),
                "{key} is not alphacode-owned"
            );
            assert!(seen.insert(*key), "duplicate routing env key: {key}");
        }
    }

    /// Regression guard for the routing variables that were being reported as
    /// ordinary tool output.
    #[test]
    fn the_reported_leak_vars_are_all_covered() {
        for key in [
            "ALPHACODE_OPENROUTER_MODEL",
            "ALPHACODE_OPENROUTER_STATIC_MODELS",
            "ALPHACODE_OPENROUTER_MODEL_CATALOG",
        ] {
            assert!(
                INTERNAL_ROUTING_ENV_VARS.contains(&key),
                "{key} is still inherited by tool subprocesses"
            );
        }
    }

    /// Tool plumbing must survive the filter or tool output paths break.
    #[test]
    fn tool_plumbing_is_preserved() {
        for key in ["ALPHACODE_SCRATCH_DIR", "ALPHACODE_READY_FD"] {
            assert!(
                !INTERNAL_ROUTING_ENV_VARS.contains(&key),
                "{key} is tool plumbing and must remain available"
            );
        }
    }

    #[test]
    fn credential_list_is_alpha_owned_and_secret_bearing() {
        let mut seen = std::collections::HashSet::new();
        for key in ALPHACODE_CREDENTIAL_ENV_VARS {
            assert!(
                key.starts_with("ALPHACODE_"),
                "{key} is not alphacode-owned"
            );
            assert!(!key.trim().is_empty(), "blank credential env key");
            assert!(seen.insert(*key), "duplicate credential env key: {key}");
        }
    }

    /// The user's own credentials are theirs to expose to the tools they run;
    /// alpha must not remove them.
    #[test]
    fn user_owned_credentials_are_left_alone() {
        for key in [
            "OPENAI_API_KEY",
            "ANTHROPIC_API_KEY",
            "GITHUB_TOKEN",
            "OPENROUTER_API_KEY",
            "AWS_SECRET_ACCESS_KEY",
        ] {
            assert!(
                !ALPHACODE_CREDENTIAL_ENV_VARS.contains(&key),
                "{key} belongs to the user and must not be removed from tool processes"
            );
        }
    }

    /// The two lists must not overlap; a duplicate would be dead weight and a
    /// signal that one of them was edited carelessly.
    #[test]
    fn routing_and_credential_lists_are_disjoint() {
        for key in ALPHACODE_CREDENTIAL_ENV_VARS {
            assert!(
                !INTERNAL_ROUTING_ENV_VARS.contains(key),
                "{key} appears in both lists"
            );
        }
    }

    #[test]
    fn filter_removes_the_vars_from_the_child_environment() {
        let key = "ALPHACODE_OPENROUTER_MODEL";
        // SAFETY: single-threaded test scope; production code centralizes
        // env mutation in this module's helpers for good reason.
        unsafe {
            std::env::set_var(key, "routing-placeholder");
        }
        let mut command = std::process::Command::new("program-name-not-executed");
        strip_internal_env(&mut command);
        let queued: Vec<String> = command
            .get_envs()
            .map(|(k, _)| k.to_string_lossy().into_owned())
            .collect();
        assert!(
            !queued.iter().any(|k| k == key),
            "{key} was still queued for the child process"
        );
        unsafe {
            std::env::remove_var(key);
        }
    }
}
