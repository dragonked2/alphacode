#[cfg(test)]
#[allow(clippy::module_inception)]
mod tests {
    use super::super::*;
    use clap::Parser;

    #[test]
    fn verify_args_parse() {
        // Verify that the Args struct parses without error.
        let args = Args::try_parse_from(["alphacode"]);
        assert!(args.is_ok(), "default args should parse: {:?}", args.err());
    }

    #[test]
    fn verify_serve_command() {
        let args = Args::try_parse_from(["alphacode", "serve"]).unwrap();
        assert!(matches!(args.command, Some(Command::Serve { .. })));
    }

    #[test]
    fn verify_version_command() {
        let args = Args::try_parse_from(["alphacode", "version"]).unwrap();
        assert!(matches!(args.command, Some(Command::Version { .. })));
    }

    #[test]
    fn verify_run_command() {
        let args = Args::try_parse_from(["alphacode", "run", "hello"]).unwrap();
        assert!(matches!(args.command, Some(Command::Run { .. })));
    }

    #[test]
    fn verify_repl_command() {
        let args = Args::try_parse_from(["alphacode", "repl"]).unwrap();
        assert!(matches!(args.command, Some(Command::Repl)));
    }

    #[test]
    fn verify_update_command() {
        let args = Args::try_parse_from(["alphacode", "update"]).unwrap();
        assert!(matches!(args.command, Some(Command::Update)));
    }

    #[test]
    fn verify_provider_flag() {
        let args = Args::try_parse_from(["alphacode", "--provider", "openai"]).unwrap();
        assert_eq!(args.provider, ProviderChoice::Openai);
    }

    #[test]
    fn verify_model_flag() {
        let args = Args::try_parse_from(["alphacode", "--model", "gpt-4"]).unwrap();
        assert_eq!(args.model.as_deref(), Some("gpt-4"));
    }

    #[test]
    fn verify_resume_flag() {
        let args = Args::try_parse_from(["alphacode", "--resume"]).unwrap();
        assert!(args.resume.is_some());
    }

    #[test]
    fn verify_telemetry_command() {
        let args = Args::try_parse_from(["alphacode", "telemetry", "status"]).unwrap();
        assert!(matches!(args.command, Some(Command::Telemetry(_))));
    }
}
