//! The destructive-command gate for the `bash` tool.
//!
//! Keeps the destructive-command policy and bash parameter schema together so
//! bash.rs only needs to delegate to this module.
//!
//! # Policy
//!
//! The previous version of this gate held every cross-directory destructive
//! command for a "reflection prompt", which the model had to answer before
//! the command would run. In practice this blocked routine authorized
//! security testing (`nmap`, `subfinder`, `nuclei`, `httpx`, `ffuf`, `curl`
//! against an in-scope target, etc.) because every such tool reaches outside
//! the working directory by definition. The gate is now reduced to a single
//! hard check: refuse commands that would destroy a protected path (home
//! directory, credential store, or system root). Everything else runs.

/// Apply the destructive-command gate, returning refusal text when the
/// command must not run as-issued. Only the catastrophic tier is held.
pub(super) fn destructive_command_refusal(
    command: &str,
    justification: Option<&str>,
    working_dir: Option<std::path::PathBuf>,
) -> Option<String> {
    let risk_ctx = crate::alphacode_command_risk::RiskContext::from_env(working_dir);

    let assessment = crate::alphacode_command_risk::assess(command, &risk_ctx);

    if assessment.level.runs_immediately() {
        return None;
    }

    let justification = crate::alphacode_command_risk::Justification {
        text: justification.map(str::to_owned),
    };

    match crate::alphacode_command_risk::gate(&assessment, &justification) {
        crate::alphacode_command_risk::GateOutcome::Allow => None,

        crate::alphacode_command_risk::GateOutcome::Deny { reason } => {
            crate::logging::warn(&format!(
                "[bash] destructive command denied: {}",
                summarize_command(command)
            ));

            Some(reason)
        }
    }
}

/// Keep command logging bounded so an unusually large shell command does not
/// flood the logs.
fn summarize_command(command: &str) -> String {
    const MAX_LOG_LEN: usize = 512;

    let command = command.trim();

    if command.len() <= MAX_LOG_LEN {
        command.to_owned()
    } else {
        format!("{}…", crate::util::truncate_str(command, MAX_LOG_LEN))
    }
}

/// The `bash` tool's JSON schema.
///
/// The `justification` field is preserved for backwards compatibility with
/// callers that still supply it, but it is no longer required: the
/// destructive gate no longer asks the model to re-justify a held command.
pub(super) fn bash_parameters_schema() -> serde_json::Value {
    let cmd_desc = if cfg!(windows) {
        "The command to execute in Git Bash (POSIX-compatible). \
Use POSIX syntax: ls, mv, rm, cp, grep, cat. \
Forward slashes for paths (C:/Users not C:\\Users). \
For PowerShell-specific operations, prefix with: \
powershell -Command '...'. \
For cmd.exe operations, prefix with: \
cmd.exe /C '...'. \
Never mix shell syntaxes."
    } else {
        "The bash command to execute. Put large temporary files under \
`$ALPHACODE_SCRATCH_DIR`, not `/tmp`."
    };

    serde_json::json!({
        "type": "object",
        "required": ["command"],
        "properties": {
            "intent": crate::tool::intent_schema_property(),

            "command": {
                "type": "string",
                "description": cmd_desc
            },

            "timeout": {
                "type": ["integer", "null"],
                "description": "Foreground wait in milliseconds. If exceeded, the command continues in the background; check its final status and output."
            },

            "run_in_background": {
                "type": ["boolean", "null"],
                "description": "Run in background. Emit `ALPHACODE_PROGRESS {json}` lines for progress reporting. Completion wakes the agent (same as `wake: true`) unless `wake` is explicitly false."
            },

            "auto_background": {
                "type": ["boolean", "null"],
                "description": "Move silent foreground commands to the background; agent turns wake on completion unless wake is false. Defaults true; set false for interactive commands."
            },

            "notify": {
                "type": ["boolean", "null"],
                "description": "Notify on completion. Defaults true when omitted or null; explicit false disables notifications."
            },

            "wake": {
                "type": ["boolean", "null"],
                "description": "Wake on completion. Defaults to true when `run_in_background` is set, so a finished background task resumes the agent instead of waiting to be noticed."
            },

            "justification": {
                "type": ["string", "null"],
                "description": "Optional. No longer required, but accepted for compatibility with earlier releases."
            }
        }
    })
}

#[cfg(test)]
mod log_tests {
    #[test]
    fn command_log_clipping_preserves_utf8_boundaries() {
        let command = format!("{}{}", "a".repeat(511), "界".repeat(20));
        let summary = super::summarize_command(&command);
        assert!(summary.ends_with('…'));
        assert!(summary.len() <= 515);
    }
}
