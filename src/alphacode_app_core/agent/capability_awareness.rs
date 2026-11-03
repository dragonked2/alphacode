use super::Agent;

impl Agent {
    /// Read names only; capability awareness must not construct every schema.
    pub(super) async fn session_capability_names(&self) -> Vec<String> {
        crate::tool::capabilities::eligible_names(
            self.registry.tool_names().await,
            self.allowed_tools.as_ref(),
            &self.disabled_tools,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{Message, StreamEvent, ToolDefinition};
    use crate::provider::{EventStream, Provider};
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Request {
        dynamic: String,
        tools: Vec<String>,
    }

    #[derive(Default, Clone)]
    struct RecordingProvider {
        requests: Arc<Mutex<Vec<Request>>>,
        tool_once: Arc<std::sync::atomic::AtomicBool>,
    }

    #[async_trait::async_trait]
    impl Provider for RecordingProvider {
        async fn complete(
            &self,
            _messages: &[Message],
            _tools: &[ToolDefinition],
            _system: &str,
            _resume: Option<&str>,
        ) -> anyhow::Result<EventStream> {
            if self
                .tool_once
                .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                return Ok(Box::pin(tokio_stream::iter([
                    Ok(StreamEvent::ToolUseStart {
                        id: "denied-write".into(),
                        name: "write".into(),
                    }),
                    Ok(StreamEvent::ToolInputDelta(
                        r#"{"file_path":"must-not-exist","content":"denied"}"#.into(),
                    )),
                    Ok(StreamEvent::ToolUseEnd),
                    Ok(StreamEvent::MessageEnd { stop_reason: None }),
                ])));
            }
            Ok(Box::pin(tokio_stream::iter([
                Ok(StreamEvent::TextDelta("Inventory checked.".into())),
                Ok(StreamEvent::MessageEnd { stop_reason: None }),
            ])))
        }
        async fn complete_split(
            &self,
            messages: &[Message],
            tools: &[ToolDefinition],
            system_static: &str,
            system_dynamic: &str,
            resume: Option<&str>,
        ) -> anyhow::Result<EventStream> {
            self.requests.lock().unwrap().push(Request {
                dynamic: system_dynamic.to_string(),
                tools: tools.iter().map(|tool| tool.name.clone()).collect(),
            });
            self.complete(messages, tools, system_static, resume).await
        }
        fn name(&self) -> &str {
            "test"
        }
        fn fork(&self) -> Arc<dyn Provider> {
            Arc::new(self.clone())
        }
    }

    #[tokio::test]
    async fn denied_tool_records_policy_result_in_both_turn_paths() {
        let _guard = crate::storage::lock_test_env();
        for streaming in [false, true] {
            let provider = Arc::new(RecordingProvider::default());
            provider
                .tool_once
                .store(true, std::sync::atomic::Ordering::SeqCst);
            let mut agent = Agent::new(provider, crate::tool::Registry::empty());
            agent.allowed_tools = Some(["read".into()].into_iter().collect());
            if streaming {
                let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
                agent
                    .run_once_streaming_mpsc("Try writing", Vec::new(), None, tx)
                    .await
                    .unwrap();
            } else {
                agent.run_once_capture("Try writing").await.unwrap();
            }
            assert!(agent.session.messages.iter().flat_map(|m| &m.content).any(|block| matches!(block,
                crate::message::ContentBlock::ToolResult { tool_use_id, content, is_error: Some(true) }
                if tool_use_id == "denied-write" && content.contains("session tool allowlist") && !content.contains("session interrupted")
            )));
        }
    }

    #[tokio::test]
    async fn cli_and_tui_requests_receive_current_inventory_without_loading_hidden_schemas() {
        let _guard = crate::storage::lock_test_env();
        for streaming in [false, true] {
            let provider = Arc::new(RecordingProvider::default());
            let registry = crate::tool::Registry::empty();
            registry
                .register(
                    "tool_help".into(),
                    Arc::new(crate::tool::tool_help::ToolHelpTool::for_registry(
                        &registry,
                    )),
                )
                .await;
            let mut agent = Agent::new(provider.clone(), registry.clone());
            for step in 0..3 {
                if step == 1 {
                    registry
                        .register(
                            "ghidra".into(),
                            Arc::new(crate::tool::tool_help::ToolHelpTool::new()),
                        )
                        .await;
                } else if step == 2 {
                    registry.unregister("ghidra").await;
                }
                if streaming {
                    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
                    agent
                        .run_once_streaming_mpsc(
                            "Inspect this session's available tool inventory",
                            Vec::new(),
                            None,
                            tx,
                        )
                        .await
                        .unwrap();
                } else {
                    agent
                        .run_once_capture("Inspect this session's available tool inventory")
                        .await
                        .unwrap();
                }
                let requests = provider.requests.lock().unwrap();
                let request = requests.last().expect("provider request");
                assert!(request.dynamic.contains("# Live session capabilities"));
                let inventory = request
                    .dynamic
                    .rsplit("# Live session capabilities")
                    .next()
                    .unwrap();
                assert_eq!(inventory.contains("ghidra"), step == 1);
                assert!(!request.tools.iter().any(|name| name == "ghidra"));
                assert!(request.tools.iter().any(|name| name == "tool_help"));
            }
        }
    }
}
