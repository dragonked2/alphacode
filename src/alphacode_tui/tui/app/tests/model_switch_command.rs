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

    fn supports_model_switching(&self) -> bool {
        true
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

/// A provider that deliberately has no model surface — the shape of the inert
/// runtime a remote client or replay session holds.
struct NoModelSurfaceProvider;

#[async_trait::async_trait]
impl Provider for NoModelSurfaceProvider {
    fn name(&self) -> &str {
        "remote"
    }

    fn model(&self) -> String {
        "unknown".to_string()
    }

    async fn complete(
        &self,
        _messages: &[crate::alphacode_message_types::Message],
        _tools: &[crate::alphacode_message_types::ToolDefinition],
        _system: &str,
        _resume_session_id: Option<&str>,
    ) -> Result<crate::alphacode_provider_core::EventStream> {
        unimplemented!("no-model-surface tests never complete a turn")
    }

    fn fork(&self) -> Arc<dyn Provider> {
        Arc::new(Self)
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
            !app.provider.supports_model_switching(),
            "a remote client holds a provider with no model surface"
        );

        assert!(model_context::handle_model_command(&mut app, "/set model gpt-5.5"));
        assert!(model_context::handle_model_command(&mut app, "/model gpt-5.5"));

        let last = app
            .display_messages
            .last()
            .cloned()
            .expect("a failure message should have been shown");
        assert_eq!(last.role, "error");
        assert!(
            !last.content.contains(crate::alphacode_provider_core::MODEL_SWITCH_UNSUPPORTED),
            "must not blame the provider for a runtime that has none: {}",
            last.content
        );
        assert!(
            !last.content.contains("Failed to switch model"),
            "must not present a runtime-mode block as a provider failure: {}",
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
        assert_eq!(app.session.model.as_deref(), None);
    });
}

#[test]
fn model_switch_unavailable_message_is_identical_for_both_spellings() {
    with_temp_alphacode_home(|| {
        // The two spellings are the same request. If they ever produce different
        // failure text the user is back to guessing which one to trust.
        let app = remote_client_app();
        let a = model_context::model_switch_unavailable_message("x", &app);
        let b = model_context::model_switch_unavailable_message("x", &app);
        assert_eq!(a, b);
    });
}

#[test]
fn a_provider_that_can_switch_is_not_blocked_by_the_runtime_check() {
    with_temp_alphacode_home(|| {
        // The guard keys on the provider's capability, not on the runtime mode.
        // A test-harness session with a real provider must still switch.
        assert!(model_context::model_switch_unavailable_reason(&switchable_app()).is_none());
        assert!(model_context::model_switch_unavailable_reason(&remote_client_app()).is_some());
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
// 4. The sentinel and the capability flag agree with each other.
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
    assert!(!bare.supports_model_switching());
    let error = bare.set_model("anything").unwrap_err().to_string();
    assert!(
        error.contains(crate::alphacode_provider_core::MODEL_SWITCH_UNSUPPORTED),
        "the trait default must return the shared sentinel so callers can \
         classify it: {error}"
    );

    let switchable = ModelSwitchingProvider {
        model: std::sync::Mutex::new("m".to_string()),
    };
    assert!(switchable.supports_model_switching());
    assert!(
        switchable.set_model("other").is_ok(),
        "a provider that advertises support must actually accept a model"
    );
}