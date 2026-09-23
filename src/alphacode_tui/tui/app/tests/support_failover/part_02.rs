use crate::alphacode_message_types::Message;

#[test]
fn test_cancel_pending_provider_failover_clears_countdown() {
    with_temp_alphacode_home(|| {
        write_test_config("[provider]\ncross_provider_failover = \"countdown\"\n");
        let (mut app, _active_provider) = create_switchable_test_app("claude");
        let prompt = crate::provider::ProviderFailoverPrompt {
            from_provider: "claude".to_string(),
            from_label: "Anthropic".to_string(),
            to_provider: "openai".to_string(),
            to_label: "OpenAI".to_string(),
            reason: "OAuth usage exhausted".to_string(),
            estimated_input_chars: 16_000,
            estimated_input_tokens: 4_000,
        };

        app.handle_turn_error(failover_error_message(&prompt));
        assert!(app.pending_provider_failover.is_some());

        app.cancel_pending_provider_failover("Provider auto-switch canceled");
        assert!(app.pending_provider_failover.is_none());
    });
}

#[test]
fn test_provider_failover_countdown_expires() {
    with_temp_alphacode_home(|| {
        write_test_config("[provider]\ncross_provider_failover = \"countdown\"\n");
        let (mut app, _active_provider) = create_switchable_test_app("claude");
        let prompt = crate::provider::ProviderFailoverPrompt {
            from_provider: "claude".to_string(),
            from_label: "Anthropic".to_string(),
            to_provider: "openai".to_string(),
            to_label: "OpenAI".to_string(),
            reason: "OAuth usage exhausted".to_string(),
            estimated_input_chars: 16_000,
            estimated_input_tokens: 4_000,
        };

        app.handle_turn_error(failover_error_message(&prompt));
        assert!(app.pending_provider_failover.is_some());

        std::thread::sleep(std::time::Duration::from_secs(2));
        app.maybe_progress_provider_failover_countdown();
        assert!(app.pending_provider_failover.is_none());
    });
}
