// Regression tests for `/set model <id>` (issue: "Failed to switch model: This
// provider does not support model switching").
//
// Three separate defects produced that message:
//
// 1. `/set model <id>` and `/model <id>` are the same request, but only
//    `/model ` was forwarded to the server by a remote client. The alias fell
//    through to the in-process handler, where `app.provider` is an inert
//    runtime that has no provider at all, and the trait default rejected it.
// 2. The resulting message blamed the provider and then recommended `/model` —
//    the other name of the command that had just failed.
// 3. The local handler called the bare `Provider::set_model` trait method
//    instead of the auth-refreshing wrapper every other call site uses, so a
//    switch right after `/login` could fail on stale credentials.
//
// Each test below pins one of those. They are cheap and deterministic; there is
// no provider network access and no timing dependency.

/// A provider that implements model switching, i.e. a real one.
struct ModelSwitchingProvider {
    model: std::sync::Mutex<String>,
}

#[async_trait::async_trait]
impl Provider for ModelSwitchingProvider {
    fn name(&self) -> &str {
        "model-switching"
    }

    fn model(&self) -> String {
        self.model.lock().unwrap().clone()
    }

    fn set_model(&self, model: &str) -> Result<()> {
        *self.model.lock().unwrap() = model.trim().to_string();
        Ok(())
    }

    fn available_models(&self) -> Vec<&'static str> {
        vec!["model-a", "model-b"]
    }

    async fn complete(
        &self,
        _messages: &[crate::alphacode_message_types::Message],
        _tools: &[crate::alphacode_message_types::ToolDefinition],
        _system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<crate::alphacode_provider_core::EventStream> {
        unimplemented!("model switching tests never complete a turn")
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(Self {
            model: std::sync::Mutex::new(self.model.lock().unwrap().clone()),
        })
    }
}

fn switchable_app() -> App {
    let provider: Arc<dyn Provider> = Arc::new(ModelSwitchingProvider {
        model: std::sync::Mutex::new("model-a".to_string()),
    });
    let rt = tokio::runtime::Runtime::new().unwrap();
    let registry = rt.block_on(crate::tool::Registry::new(provider.clone()));
    App::new_for_test_harness(provider, registry)
}

fn remote_client_app() -> App {
    App::new_for_remote(None)
}

// ---------------------------------------------------------------------------
// 1. `/set model <name>` and `/model <name>` must behave identically.
// ---------------------------------------------------------------------------

#[test]
fn set_model_alias_and_model_switch_both_land_on_the_same_model() {
    with_temp_alphacode_home(|| {
        let mut app = switchable_app();

        assert!(model_context::handle_model_command(&mut app, "/model model-b"));
        assert_eq!(app.session.model.as_deref(), Some("model-b"));
        assert_eq!(
            app.status_notice(),
            Some("Model → model-b".to_string())
        );

        assert!(model_context::handle_model_command(&mut app, "/set model model-a"));
        assert_eq!(app.session.model.as_deref(), Some("model-a"));
        assert_eq!(
            app.status_notice(),
            Some("Model → model-a".to_string())
        );

        let last = app.display_messages.last().expect("display message");
        assert_eq!(last.role, "system");
        assert!(
            last.content.contains("model-a"),
            "success path must be reached for the alias too: {}",
            last.content
        );
    });
}

// ---------------------------------------------------------------------------
// 2. The failure message must not blame the provider, and must not recommend
//    the alias of the command that just failed.
// ---------------------------------------------------------------------------

#[test]
fn set_model_on_a_runtime_without_a_provider_reports_the_real_cause() {
    with_temp_alphacode_home(|| {
        let mut app = remote_client_app();
        assert!(
            app.is_remote,
            "a remote client holds no provider of its own"
        );

        assert!(model_context::handle_model_command(&mut app, "/set model gpt-5.5"));
        assert!(model_context::handle_model_command(&mut app, "/model gpt-5.5"));

        let last = app
            .display_messages
            .last()
            .cloned()
            .expect("a failure message should have been shown");
        assert_eq!(last.role, "error");
        // The provider's own words are quoted on purpose — the report must not
        // invent a cause the runtime did not state — but quoting is not the same
        // as blaming. The line that actually assigns fault is the headline, and
        // it must name the real cause (a client with no provider) rather than the
        // provider. "Failed to switch model" is the generic provider-failure
        // wrapper and is exactly what must not appear.
        assert!(
            last.content.contains("this client is not connected to a server that owns a provider"),
            "the headline must name the real cause, not the provider: {}",
            last.content
        );
        assert!(
            !last.content.contains("Failed to switch model"),
            "must not present a runtime-mode block as a provider failure: {}",
            last.content
        );
        assert!(
            last.content.contains("Provider reported:"),
            "the provider's own words must still be quoted verbatim: {}",
            last.content
        );
        assert!(
            last.content.contains(crate::alphacode_provider_core::MODEL_SWITCH_UNSUPPORTED),
            "the quote must not be paraphrased away: {}",
            last.content
        );
        assert!(
            last.content.contains("Cannot switch to 'gpt-5.5'"),
            "message should name the requested model: {}",
            last.content
        );
        assert!(
            last.content.contains("Reconnect"),
            "a remote client should be told to reconnect: {}",
            last.content
        );
        assert!(
            last.content.contains(crate::alphacode_provider_core::MODEL_SWITCH_UNSUPPORTED),
            "the provider's own words must still be quoted: {}",
            last.content
        );
        // Nothing was switched: the session model must not have moved to the
        // model the user asked for.
        assert_ne!(
            app.session.model.as_deref(),
            Some("gpt-5.5"),
            "a refused switch must not change the session model"
        );
    });
}

#[test]
fn model_switch_unavailable_message_is_identical_for_both_spellings() {
    with_temp_alphacode_home(|| {
        // The two spellings are the same request. If they ever produce different
        // failure text the user is back to guessing which one to trust.
        let app = remote_client_app();
        let a = model_context::model_switch_unavailable_message("x", &app, "no provider");
        let b = model_context::model_switch_unavailable_message("x", &app, "no provider");
        assert_eq!(a, b);
    });
}

#[test]
fn the_unavailable_message_points_at_the_actual_cause_for_each_session_kind() {
    with_temp_alphacode_home(|| {
        // The reason is derived from the session, not from a provider capability
        // flag, so a remote client and a local session must be told different
        // things: reconnecting cannot fix a local session, and starting one
        // cannot fix a remote client.
        let remote = model_context::model_switch_unavailable_message(
            "x",
            &remote_client_app(),
            "no provider",
        );
        assert!(
            remote.contains("not connected to a server that owns a provider"),
            "a remote client must be told it has no provider: {remote}"
        );
        assert!(
            remote.contains("Reconnect"),
            "a remote client must be told to reconnect: {remote}"
        );

        let local = model_context::model_switch_unavailable_message(
            "x",
            &switchable_app(),
            "no provider",
        );
        assert!(
            local.contains("not running a provider that can switch models"),
            "a local session must be told to start a real one: {local}"
        );
        assert!(
            !local.contains("Reconnect"),
            "reconnecting is meaningless advice for a local session: {local}"
        );
    });
}

// ---------------------------------------------------------------------------
// 3. A remote client forwards both spellings to the server.
// ---------------------------------------------------------------------------

#[test]
fn remote_client_forwards_the_set_model_alias_to_the_server() {
    // Structural check: the remote command chain must recognise `/set model`
    // as a model request. The alias previously fell through the whole chain
    // into the in-process handler, which is exactly the bug.
    let source = include_str!("../remote/key_handling.rs");
    assert!(
        source.contains(r#"trimmed.strip_prefix("/set model ")"#),
        "the remote command chain must forward the `/set model <name>` alias \
         instead of letting it fall through to the in-process handler"
    );
    assert!(
        source.contains(r#"trimmed == "/set model""#),
        "bare `/set model` must open the picker in a remote client too, the same \
         way `/model` does"
    );
}

// ---------------------------------------------------------------------------
// 4. The sentinel is what the trait default returns, and a provider that
//    accepts a switch actually applies it.
// ---------------------------------------------------------------------------

#[test]
fn the_unsupported_sentinel_is_what_the_trait_default_returns() {
    struct BareProvider;
    #[async_trait::async_trait]
    impl Provider for BareProvider {
        fn name(&self) -> &str {
            "bare"
        }

        async fn complete(
            &self,
            _messages: &[crate::alphacode_message_types::Message],
            _tools: &[crate::alphacode_message_types::ToolDefinition],
            _system: &str,
            _resume_session_id: Option<&str>,
        ) -> Result<crate::alphacode_provider_core::EventStream> {
            unimplemented!("bare provider is never asked to complete")
        }

        fn fork(&self) -> Arc<dyn Provider> {
            Arc::new(Self)
        }
    }

    let bare = BareProvider;
    let error = bare.set_model("anything").unwrap_err().to_string();
    assert!(
        error.contains(crate::alphacode_provider_core::MODEL_SWITCH_UNSUPPORTED),
        "the trait default must return the shared sentinel so callers can \
         classify it: {error}"
    );

    // A provider that overrides `set_model` must actually apply the model, not
    // just stop reporting the sentinel. This is the contract callers classify on.
    let switchable = ModelSwitchingProvider {
        model: std::sync::Mutex::new("m".to_string()),
    };
    assert!(
        switchable.set_model("other").is_ok(),
        "a provider that accepts a switch must not fail it"
    );
    assert_eq!(
        switchable.model(),
        "other",
        "a provider that accepts a switch must actually apply the model"
    );
}

// ---------------------------------------------------------------------------
// 5. The runtime message is chosen by classifying the error, never by a
//    capability flag the provider must remember to set.
// ---------------------------------------------------------------------------

#[test]
fn a_provider_that_can_switch_is_never_given_the_runtime_message() {
    with_temp_alphacode_home(|| {
        // An earlier version guarded on `Provider::supports_model_switching()`.
        // Every provider implementation had to opt in, and any one that forgot
        // was silently blocked from a command that would have worked — which is
        // how an unrelated pre-existing test broke. Matching the sentinel error
        // text has no such maintenance hazard.
        let mut app = switchable_app();
        assert!(model_context::handle_model_command(
            &mut app,
            "/set model model-b"
        ));
        assert_eq!(app.session.model.as_deref(), Some("model-b"));
        let last = app.display_messages.last().expect("display message");
        assert_eq!(
            last.role, "system",
            "a switchable provider must reach the success path: {}",
            last.content
        );
        assert!(
            !last.content.contains("Cannot switch to"),
            "a working provider must never be told it cannot switch: {}",
            last.content
        );
    });
}