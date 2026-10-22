//! Recording of tool results produced by the streaming turn loop.
//!
//! The streaming path can execute a batch of tool calls either serially or
//! concurrently (see `super::parallel_tools`). Both paths funnel their
//! outcome through [`Agent::record_streamed_tool_result`] so the history
//! entry, the `ToolDone` event, and the side-pane image payload stay
//! identical no matter how the call was scheduled.

use super::*;
use crate::alphacode_app_core::protocol::ServerEvent;
use crate::alphacode_message_types::{ContentBlock, Role};

impl Agent {
    /// Record the outcome of a single tool call that ran inside the
    /// streaming turn loop.
    ///
    /// This mirrors the serial execution path: the output is capped for
    /// history, a `ToolDone` event is emitted, side-pane images are
    /// extracted for image-producing tools, and the result is appended to
    /// the conversation history as a `ToolResult` block.
    ///
    /// `index` is the position of the call inside the current batch and is
    /// used only for logging/diagnostics.
    pub(crate) async fn record_streamed_tool_result(
        &mut self,
        call: &ToolCall,
        result: anyhow::Result<crate::alphacode_tool_types::ToolOutput>,
        index: usize,
        event_tx: &tokio::sync::mpsc::UnboundedSender<ServerEvent>,
    ) {
        let tool_start = Instant::now();

        match result {
            Ok(output) => {
                let output = cap_tool_output_for_history(&call.name, output);
                let _ = event_tx.send(ServerEvent::ToolDone {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    output: output.output.clone(),
                    error: None,
                });

                let side_pane_images =
                    tool_output_side_pane_images(&call.id, &call.name, &call.input, &output);
                if !side_pane_images.is_empty() {
                    logging::info(&format!(
                        "SidePaneImages: emitting {} image(s) from tool '{}' (session={})",
                        side_pane_images.len(),
                        call.name,
                        self.session.id
                    ));
                    let _ = event_tx.send(ServerEvent::SidePaneImages {
                        session_id: self.session.id.clone(),
                        images: side_pane_images,
                    });
                }

                let blocks = tool_output_to_content_blocks(call.id.clone(), output);
                self.add_message_with_duration(
                    Role::User,
                    blocks,
                    Some(tool_start.elapsed().as_millis() as u64),
                );
            }
            Err(e) => {
                // Wrap with a tool-specific recovery hint before the error
                // enters history, so the model's next attempt targets the fix
                // instead of repeating the call.
                let error_msg = crate::tool::agent_facing_error(&call.name, &e);
                let _ = event_tx.send(ServerEvent::ToolDone {
                    id: call.id.clone(),
                    name: call.name.clone(),
                    output: error_msg.clone(),
                    error: Some(error_msg.clone()),
                });

                self.add_message_with_duration(
                    Role::User,
                    vec![ContentBlock::ToolResult {
                        tool_use_id: call.id.clone(),
                        content: error_msg,
                        is_error: Some(true),
                    }],
                    Some(tool_start.elapsed().as_millis() as u64),
                );
            }
        }

        logging::info(&format!(
            "Tool finished: {} at batch index {}",
            call.name, index
        ));
    }
}
