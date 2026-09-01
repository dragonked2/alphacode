//! Mapping from parsed CLI arguments to an initial process title.
//!
//! This logic depends on the clap `Args`/`Command` types defined in `cli`, so
//! it lives in the CLI layer. The low-level title-setting primitives it uses
//! (`compact_process_title`, `session_name`, `set_title`) live in the
//! `process_title` core module.

use crate::cli::args::{AmbientCommand, Args, Command};
use crate::alphacode_base::process_title::{compact_process_title, session_name, set_title};

pub(crate) fn initial_title(args: &Args) -> String {
    match &args.command {
        Some(Command::Serve { .. }) => "alphacode:server".to_string(),
        Some(Command::Acp) => "alphacode acp".to_string(),
        Some(Command::Server { .. }) => "alphacode server".to_string(),
        Some(Command::Connect) => "alphacode:client".to_string(),
        Some(Command::Run { .. }) => "alphacode run".to_string(),
        Some(Command::Login { .. }) => "alphacode login".to_string(),
        Some(Command::Account { .. }) => "alphacode account".to_string(),
        Some(Command::Repl) => "alphacode repl".to_string(),
        Some(Command::Update) => "alphacode update".to_string(),
        Some(Command::Telemetry(_)) => "alphacode telemetry".to_string(),
        Some(Command::Version { .. }) => "alphacode version".to_string(),
        Some(Command::Usage { .. }) => "alphacode usage".to_string(),
        Some(Command::SelfDev { .. }) => "alphacode:selfdev".to_string(),
        Some(Command::Debug { .. }) => "alphacode debug".to_string(),
        Some(Command::Auth(_)) => "alphacode auth".to_string(),
        Some(Command::Provider(_)) => "alphacode provider".to_string(),
        Some(Command::Memory(_)) => "alphacode memory".to_string(),
        Some(Command::Session(_)) => "alphacode session".to_string(),
        Some(Command::Ambient(subcommand)) => match subcommand {
            AmbientCommand::RunVisible => "alphacode ambient visible".to_string(),
            _ => "alphacode ambient".to_string(),
        },
        Some(Command::Cloud(_)) => "alphacode cloud".to_string(),
        Some(Command::Pair { .. }) => "alphacode pair".to_string(),
        Some(Command::Permissions) => "alphacode permissions".to_string(),
        Some(Command::Transcript { .. }) => "alphacode transcript".to_string(),
        Some(Command::Dictate { .. }) => "alphacode dictate".to_string(),
        Some(Command::SetupHotkey {
            listen_macos_hotkey,
            notify_cli_launch,
            listen_windows_hotkey,
            uninstall,
        }) => {
            if *listen_macos_hotkey || *listen_windows_hotkey {
                "alphacode hotkey listener".to_string()
            } else if notify_cli_launch.is_some() {
                "alphacode shortcut reminder".to_string()
            } else if *uninstall {
                "alphacode hotkey uninstall".to_string()
            } else {
                "alphacode hotkey setup".to_string()
            }
        }
        Some(Command::Browser { .. }) => "alphacode browser".to_string(),
        Some(Command::Replay { .. }) => "alphacode replay".to_string(),
        Some(Command::Model(_)) => "alphacode model".to_string(),
        Some(Command::ProviderTestCoverage { .. }) => "alphacode provider-test-coverage".to_string(),
        Some(Command::ProviderDoctor { .. }) => "alphacode provider-doctor".to_string(),
        Some(Command::AuthTest { .. }) => "alphacode auth-test".to_string(),
        Some(Command::Restart { .. }) => "alphacode restart".to_string(),
        Some(Command::Menubar { .. }) => "alphacode menubar".to_string(),
        Some(Command::SetupLauncher) => "alphacode setup-launcher".to_string(),
        None => {
            if let Some(resume) = args.resume.as_deref().filter(|resume| !resume.is_empty()) {
                let prefix = if crate::cli::selfdev::client_selfdev_requested() {
                    "alphacode:d:"
                } else {
                    "alphacode:c:"
                };
                compact_process_title(prefix, Some(&session_name(resume)))
            } else if crate::cli::selfdev::client_selfdev_requested() {
                "alphacode:selfdev".to_string()
            } else {
                "alphacode:client".to_string()
            }
        }
    }
}

pub(crate) fn set_initial_title(args: &Args) {
    set_title(initial_title(args));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_base::storage::lock_test_env;
    use clap::Parser;

    const SELFDEV_ENV: &str = crate::alphacode_selfdev_types::CLIENT_SELFDEV_ENV;

    fn with_selfdev_env_removed<T>(f: impl FnOnce() -> T) -> T {
        let _guard = lock_test_env();
        let previous = std::env::var_os(SELFDEV_ENV);
        crate::alphacode_core::env::remove_var(SELFDEV_ENV);
        let result = f();
        if let Some(value) = previous {
            crate::alphacode_core::env::set_var(SELFDEV_ENV, value);
        }
        result
    }

    #[test]
    fn initial_title_labels_server() {
        with_selfdev_env_removed(|| {
            let args = Args::parse_from(["alphacode", "serve"]);
            assert_eq!(initial_title(&args), "alphacode:server");
        });
    }

    #[test]
    fn initial_title_labels_resume_client_with_short_name() {
        with_selfdev_env_removed(|| {
            let args = Args::parse_from(["alphacode", "--resume", "session_fox_123"]);
            assert_eq!(initial_title(&args), "alphacode:c:fox");
        });
    }

    #[test]
    fn initial_title_labels_selfdev_command() {
        with_selfdev_env_removed(|| {
            let args = Args::parse_from(["alphacode", "self-dev"]);
            assert_eq!(initial_title(&args), "alphacode:selfdev");
        });
    }

    #[test]
    fn initial_title_labels_windows_hotkey_listener() {
        let args = Args::parse_from(["alphacode", "setup-hotkey", "--listen-windows-hotkey"]);
        assert_eq!(initial_title(&args), "alphacode hotkey listener");
    }

    #[test]
    fn initial_title_labels_hotkey_uninstall() {
        let args = Args::parse_from(["alphacode", "setup-hotkey", "--uninstall"]);
        assert_eq!(initial_title(&args), "alphacode hotkey uninstall");
    }
}
