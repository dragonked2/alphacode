#[cfg(test)]
#[allow(clippy::module_inception)]
pub mod tests {

    use crate::alphacode_provider_core::Provider;
    use crate::alphacode_tui::tui::DisplayMessage;
    use crate::alphacode_tui::tui::app::App;
    use crate::alphacode_tui::tui::app::ContentBlock;
    use crate::alphacode_tui::tui::app::CopyBadgeUiState;
    use crate::alphacode_tui::tui::app::ImproveMode;
    use crate::alphacode_tui::tui::app::MouseScrollTarget;
    use crate::alphacode_tui::tui::app::PendingCatchupResume;
    use crate::alphacode_tui::tui::app::PendingLogin;
    use crate::alphacode_tui::tui::app::PendingReloadReconnectStatus;
    use crate::alphacode_tui::tui::app::PendingRemoteMessage;
    use crate::alphacode_tui::tui::app::ProcessingStatus;
    use crate::alphacode_tui::tui::app::Role;
    use crate::alphacode_tui::tui::app::SendAction;
    use crate::alphacode_tui::tui::app::Session;
    use crate::alphacode_tui::tui::app::SessionPickerMode;
    use crate::alphacode_tui::tui::app::auth;
    use crate::alphacode_tui::tui::app::commands;
    use crate::alphacode_tui::tui::app::handterm_native_scroll;
    use crate::alphacode_tui::tui::app::helpers;
    use crate::alphacode_tui::tui::app::helpers::is_context_limit_error;
    use crate::alphacode_tui::tui::app::helpers::is_request_payload_too_large_error;
    use crate::alphacode_tui::tui::app::helpers::mask_email;
    use crate::alphacode_tui::tui::app::input;
    use crate::alphacode_tui::tui::app::local;
    use crate::alphacode_tui::tui::app::model_context;
    use crate::alphacode_tui::tui::app::remote;
    use crate::alphacode_tui::tui::app::run_shell;

    // MouseEvent is available via super::* from app.rs (crossterm::event::MouseEvent)

    use anyhow::Result;
    use crossterm::event::MouseEvent;
    use crossterm::event::{KeyCode, KeyModifiers, MouseButton, MouseEventKind};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    // Stub implementations for test helpers that no longer exist but are still
    // referenced by legacy `include!`-ed test files.

    fn create_test_app_inner() -> App {
        ensure_test_alphacode_home_if_unset();
        clear_persisted_test_ui_state();
        crate::alphacode_tui::tui::ui::clear_test_render_state_for_tests();
        struct StubProvider;
        #[async_trait::async_trait]
        impl crate::alphacode_provider_core::Provider for StubProvider {
            async fn complete(
                &self,
                _messages: &[crate::alphacode_message_types::Message],
                _tools: &[crate::alphacode_message_types::ToolDefinition],
                _system: &str,
                _resume_session_id: Option<&str>,
            ) -> Result<crate::alphacode_provider_core::EventStream> {
                unimplemented!("StubProvider")
            }
            fn name(&self) -> &str {
                "stub"
            }
            fn model(&self) -> String {
                "stub-model".to_string()
            }
            fn fork(&self) -> Arc<dyn crate::alphacode_provider_core::Provider> {
                Arc::new(Self)
            }
        }
        let provider = Arc::new(StubProvider);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let registry = rt.block_on(crate::tool::Registry::new(provider.clone()));
        let mut app = App::new_for_test_harness(provider, registry);
        app.queue_mode = false;
        app.diff_mode = crate::config::DiffDisplayMode::Inline;
        app
    }

    fn create_fast_test_app() -> App {
        create_test_app_inner()
    }
    fn create_switchable_test_app(
        _provider: &str,
    ) -> (App, std::sync::Arc<std::sync::Mutex<String>>) {
        let app = create_test_app_inner();
        let active = std::sync::Arc::new(std::sync::Mutex::new(_provider.to_string()));
        (app, active)
    }
    fn create_gemini_test_app() -> App {
        create_test_app_inner()
    }

    fn write_test_config(_contents: &str) {}
    fn failover_error_message(prompt: &crate::provider::ProviderFailoverPrompt) -> String {
        prompt.to_error_message()
    }
    fn reload_persisted_background_tasks_note(session_id: &str) -> String {
        // Delegate to the real implementation instead of the previous empty
        // stub: `test_reload_persisted_background_tasks_note_mentions_running_task`
        // asserts the note mentions the registered task.
        crate::alphacode_app_core::tool::selfdev::persisted_background_tasks_note(session_id)
    }

    pub(crate) fn create_failing_model_switch_test_app() -> App {
        ensure_test_alphacode_home_if_unset();
        clear_persisted_test_ui_state();
        crate::alphacode_tui::tui::ui::clear_test_render_state_for_tests();

        struct FailingSwitchProvider;
        #[async_trait::async_trait]
        impl crate::alphacode_provider_core::Provider for FailingSwitchProvider {
            async fn complete(
                &self,
                _messages: &[crate::alphacode_message_types::Message],
                _tools: &[crate::alphacode_message_types::ToolDefinition],
                _system: &str,
                _resume_session_id: Option<&str>,
            ) -> Result<crate::alphacode_provider_core::EventStream> {
                unimplemented!("FailingSwitchProvider")
            }
            fn name(&self) -> &str {
                "failing-switch"
            }
            fn model(&self) -> String {
                "failing-model".to_string()
            }
            fn fork(&self) -> Arc<dyn crate::alphacode_provider_core::Provider> {
                Arc::new(Self)
            }
            fn model_routes(&self) -> Vec<crate::alphacode_provider_core::ModelRoute> {
                vec![crate::alphacode_provider_core::ModelRoute {
                    model: "failing-model".to_string(),
                    provider: "FailingProvider".to_string(),
                    api_method: "api".to_string(),
                    available: false,
                    detail: "credentials expired".to_string(),
                    cheapness: None,
                }]
            }
        }

        let provider = Arc::new(FailingSwitchProvider);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let registry = rt.block_on(crate::tool::Registry::new(provider.clone()));
        let mut app = App::new_for_test_harness(provider, registry);
        app.queue_mode = false;
        app.diff_mode = crate::config::DiffDisplayMode::Inline;
        app
    }

    pub(crate) fn create_auth_refresh_test_app() -> App {
        ensure_test_alphacode_home_if_unset();
        clear_persisted_test_ui_state();
        crate::alphacode_tui::tui::ui::clear_test_render_state_for_tests();

        struct AuthRefreshProvider;
        #[async_trait::async_trait]
        impl crate::alphacode_provider_core::Provider for AuthRefreshProvider {
            async fn complete(
                &self,
                _messages: &[crate::alphacode_message_types::Message],
                _tools: &[crate::alphacode_message_types::ToolDefinition],
                _system: &str,
                _resume_session_id: Option<&str>,
            ) -> Result<crate::alphacode_provider_core::EventStream> {
                unimplemented!("AuthRefreshProvider")
            }
            fn name(&self) -> &str {
                "auth-refresh"
            }
            fn model(&self) -> String {
                "auth-refresh-model".to_string()
            }
            fn fork(&self) -> Arc<dyn crate::alphacode_provider_core::Provider> {
                Arc::new(Self)
            }
        }

        let provider = Arc::new(AuthRefreshProvider);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let registry = rt.block_on(crate::tool::Registry::new(provider.clone()));
        let mut app = App::new_for_test_harness(provider, registry);
        app.queue_mode = false;
        app.diff_mode = crate::config::DiffDisplayMode::Inline;
        app
    }

    pub(crate) fn create_antigravity_picker_test_app() -> App {
        ensure_test_alphacode_home_if_unset();
        clear_persisted_test_ui_state();
        crate::alphacode_tui::tui::ui::clear_test_render_state_for_tests();

        struct AntigravityProvider;
        #[async_trait::async_trait]
        impl crate::alphacode_provider_core::Provider for AntigravityProvider {
            async fn complete(
                &self,
                _messages: &[crate::alphacode_message_types::Message],
                _tools: &[crate::alphacode_message_types::ToolDefinition],
                _system: &str,
                _resume_session_id: Option<&str>,
            ) -> Result<crate::alphacode_provider_core::EventStream> {
                unimplemented!("AntigravityProvider")
            }
            fn name(&self) -> &str {
                "antigravity"
            }
            fn model(&self) -> String {
                "antigravity-model".to_string()
            }
            fn fork(&self) -> Arc<dyn crate::alphacode_provider_core::Provider> {
                Arc::new(Self)
            }
            fn model_routes(&self) -> Vec<crate::alphacode_provider_core::ModelRoute> {
                vec![crate::alphacode_provider_core::ModelRoute {
                    model: "antigravity-model".to_string(),
                    provider: "Antigravity".to_string(),
                    api_method: "antigravity".to_string(),
                    available: true,
                    detail: "".to_string(),
                    cheapness: None,
                }]
            }
        }

        let provider = Arc::new(AntigravityProvider);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let registry = rt.block_on(crate::tool::Registry::new(provider.clone()));
        let mut app = App::new_for_test_harness(provider, registry);
        app.queue_mode = false;
        app.diff_mode = crate::config::DiffDisplayMode::Inline;
        app
    }

    pub(crate) fn create_login_smoke_model_app() -> App {
        ensure_test_alphacode_home_if_unset();
        clear_persisted_test_ui_state();
        crate::alphacode_tui::tui::ui::clear_test_render_state_for_tests();

        struct LoginSmokeProvider;
        #[async_trait::async_trait]
        impl crate::alphacode_provider_core::Provider for LoginSmokeProvider {
            async fn complete(
                &self,
                _messages: &[crate::alphacode_message_types::Message],
                _tools: &[crate::alphacode_message_types::ToolDefinition],
                _system: &str,
                _resume_session_id: Option<&str>,
            ) -> Result<crate::alphacode_provider_core::EventStream> {
                unimplemented!("LoginSmokeProvider")
            }
            fn name(&self) -> &str {
                "login-smoke"
            }
            fn model(&self) -> String {
                "login-smoke-model".to_string()
            }
            fn fork(&self) -> Arc<dyn crate::alphacode_provider_core::Provider> {
                Arc::new(Self)
            }
            fn model_routes(&self) -> Vec<crate::alphacode_provider_core::ModelRoute> {
                vec![crate::alphacode_provider_core::ModelRoute {
                    model: "login-smoke-model".to_string(),
                    provider: "LoginSmoke".to_string(),
                    api_method: "api".to_string(),
                    available: true,
                    detail: "recently added".to_string(),
                    cheapness: None,
                }]
            }
        }

        let provider = Arc::new(LoginSmokeProvider);
        let rt = tokio::runtime::Runtime::new().unwrap();
        let registry = rt.block_on(crate::tool::Registry::new(provider.clone()));
        let mut app = App::new_for_test_harness(provider, registry);
        app.queue_mode = false;
        app.diff_mode = crate::config::DiffDisplayMode::Inline;
        app
    }

    // Mock providers for auth-refresh tests in state_model_poke_03.rs
    struct AuthRefreshingMockProvider {
        logged_in: Arc<std::sync::Mutex<bool>>,
    }
    #[async_trait::async_trait]
    impl crate::alphacode_provider_core::Provider for AuthRefreshingMockProvider {
        async fn complete(
            &self,
            _messages: &[crate::alphacode_message_types::Message],
            _tools: &[crate::alphacode_message_types::ToolDefinition],
            _system: &str,
            _resume_session_id: Option<&str>,
        ) -> Result<crate::alphacode_provider_core::EventStream> {
            unimplemented!("AuthRefreshingMockProvider")
        }
        fn name(&self) -> &str {
            "auto-import"
        }
        fn model(&self) -> String {
            "test-model".to_string()
        }
        fn fork(&self) -> Arc<dyn crate::alphacode_provider_core::Provider> {
            Arc::new(Self {
                logged_in: Arc::clone(&self.logged_in),
            })
        }
        // The hot provider-init path calls `on_auth_changed` (not
        // `refresh_model_catalog`) for non-OpenAI-compatible providers, so this
        // is the hook that marks the credentials as live.
        fn on_auth_changed(&self) {
            *self.logged_in.lock().unwrap() = true;
        }
        fn model_routes(&self) -> Vec<crate::alphacode_provider_core::ModelRoute> {
            vec![crate::alphacode_provider_core::ModelRoute {
                model: "test-model".to_string(),
                provider: "auto-import".to_string(),
                api_method: "oauth".to_string(),
                available: true,
                detail: "updating model list".to_string(),
                cheapness: None,
            }]
        }
        async fn refresh_model_catalog(
            &self,
        ) -> Result<crate::alphacode_provider_core::ModelCatalogRefreshSummary> {
            Ok(crate::alphacode_provider_core::ModelCatalogRefreshSummary::default())
        }
    }

    struct AsyncAuthRefreshingMockProvider {
        started: Arc<AtomicBool>,
        completed: Arc<AtomicBool>,
        delay: Duration,
    }
    #[async_trait::async_trait]
    impl crate::alphacode_provider_core::Provider for AsyncAuthRefreshingMockProvider {
        async fn complete(
            &self,
            _messages: &[crate::alphacode_message_types::Message],
            _tools: &[crate::alphacode_message_types::ToolDefinition],
            _system: &str,
            _resume_session_id: Option<&str>,
        ) -> Result<crate::alphacode_provider_core::EventStream> {
            unimplemented!("AsyncAuthRefreshingMockProvider")
        }
        fn name(&self) -> &str {
            "auto-import"
        }
        fn model(&self) -> String {
            "test-model".to_string()
        }
        fn fork(&self) -> Arc<dyn crate::alphacode_provider_core::Provider> {
            Arc::new(Self {
                started: Arc::clone(&self.started),
                completed: Arc::clone(&self.completed),
                delay: self.delay,
            })
        }
        fn model_routes(&self) -> Vec<crate::alphacode_provider_core::ModelRoute> {
            vec![crate::alphacode_provider_core::ModelRoute {
                model: "test-model".to_string(),
                provider: "auto-import".to_string(),
                api_method: "oauth".to_string(),
                available: true,
                detail: "updating model list".to_string(),
                cheapness: None,
            }]
        }
        async fn refresh_model_catalog(
            &self,
        ) -> Result<crate::alphacode_provider_core::ModelCatalogRefreshSummary> {
            self.started.store(true, Ordering::SeqCst);
            std::thread::sleep(self.delay);
            self.completed.store(true, Ordering::SeqCst);
            Ok(crate::alphacode_provider_core::ModelCatalogRefreshSummary::default())
        }
    }

    include!("support_failover/part_01.rs");
    include!("support_failover/part_02.rs");

    include!("commands_accounts_01/part_01.rs");
    include!("commands_accounts_01/part_02.rs");
    include!("commands_accounts_02/part_01.rs");
    include!("commands_accounts_02/part_02.rs");

    include!("remote_events_reload_01/part_01.rs");
    include!("remote_events_reload_01/part_02.rs");
    include!("remote_events_reload_02/part_01.rs");
    include!("remote_events_reload_02/part_02.rs");
    include!("remote_events_reload_03/part_01.rs");
    include!("remote_events_reload_03/part_02.rs");

    include!("remote_startup_input_01/part_01.rs");
    include!("remote_startup_input_01/part_02.rs");
    include!("remote_startup_input_02/part_01.rs");
    include!("remote_startup_input_02/part_02.rs");
    include!("remote_startup_input_03/part_01.rs");
    include!("remote_startup_input_03/part_02.rs");

    include!("scroll_copy_01/part_01.rs");
    include!("scroll_copy_01/part_02.rs");
    include!("scroll_copy_02/part_01.rs");
    include!("scroll_copy_02/part_02.rs");

    include!("state_model_poke_01/part_01.rs");
    include!("state_model_poke_01/part_02.rs");
    include!("state_model_poke_02/part_01.rs");
    include!("state_model_poke_02/part_02.rs");

    include!("command_suggestions_cache.rs");
    include!("hotkey_feedback_e2e.rs");
    include!("image_placeholder_commands.rs");
    include!("input_copy_selection.rs");
    include!("interleave_images_guard.rs");
    include!("issue_496_input_routing.rs");
    include!("issue_497_copy_ctrl_c.rs");
    include!("issue_544_paste_enter.rs");
    include!("issue_605_clear_side_panel.rs");
    include!("onboarding_eval.rs");
    include!("onboarding_flow.rs");
    include!("onboarding_golden.rs");
    include!("onboarding_sim.rs");
    include!("prompt_history_cross_session.rs");
    include!("reasoning_region.rs");
    include!("remote_events_reload_04.rs");
    include!("remote_events_reload_05.rs");
    include!("remote_model_picker_hotkeys.rs");
    include!("remote_startup_input_04.rs");
    include!("scroll_copy_03.rs");
    include!("skill_invocation_multi_word.rs");
    include!("smoothness_benchmark.rs");
    include!("spinner_slash_commands.rs");
    include!("state_model_poke_03.rs");
    include!("swarm_plan_graph_inline.rs");
    include!("swarm_plan_no_inline_graph.rs");
    include!("terminal_setup_command.rs");
    include!("todo_card.rs");
}
