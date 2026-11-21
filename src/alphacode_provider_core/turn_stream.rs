//! Validate the protocol between a provider stream and a turn consumer.

use super::EventStream;
use crate::alphacode_message_types::StreamEvent;
use futures::StreamExt;
use serde_json::value::RawValue;
use std::collections::HashSet;

#[derive(Default)]
enum MessageState {
    #[default]
    Receiving,
    ReceivingTool(String),
    Ended,
}

#[derive(Default)]
struct Attempt {
    message: MessageState,
    tool_ids: HashSet<String>,
    native_tool_requested: bool,
}

pub(crate) const INCOMPLETE_STREAM: &str =
    "Unexpected EOF: provider stream ended before a completion marker";

/// Each request owns its stream and attempt state. Provider-managed sessions
/// can send many steps before their single final completion marker.
pub(crate) fn guard_stream(stream: EventStream, provider_manages_tools: bool) -> EventStream {
    Box::pin(futures::stream::unfold(
        (stream, Attempt::default(), false),
        move |(mut stream, mut attempt, failed)| async move {
            if failed {
                return None;
            }
            loop {
                let event = match stream.next().await {
                    Some(Ok(event)) => event,
                    Some(Err(error)) => return Some((Err(error), (stream, attempt, true))),
                    None if matches!(attempt.message, MessageState::Ended) => return None,
                    None => {
                        return Some((
                            Err(anyhow::anyhow!(INCOMPLETE_STREAM)),
                            (stream, attempt, true),
                        ));
                    }
                };
                match attempt.accept(&event, provider_manages_tools) {
                    Ok(true) => return Some((Ok(event), (stream, attempt, false))),
                    Ok(false) => continue,
                    Err(message) => {
                        return Some((
                            Err(anyhow::anyhow!("Invalid provider stream: {message}")),
                            (stream, attempt, true),
                        ));
                    }
                }
            }
        },
    ))
}

impl Attempt {
    fn accept(
        &mut self,
        event: &StreamEvent,
        provider_manages_tools: bool,
    ) -> Result<bool, &'static str> {
        use MessageState::*;
        use StreamEvent::*;

        // A provider rollback starts a new attempt on the same transport.
        // The consumer clears its corresponding transcript accumulators.
        if matches!(event, RetryRollback { .. }) {
            if provider_manages_tools || self.native_tool_requested || matches!(self.message, Ended)
            {
                return Err("retry requested after possible side effects or completion");
            }
            *self = Self::default();
            return Ok(true);
        }

        if matches!(self.message, Ended) {
            return Ok(match event {
                // SDKs can report usage, results, and session metadata after
                // the last text delta. Errors still invalidate the request.
                TokenUsage { .. } | SessionId(_) | UpstreamProvider { .. } | Error { .. } => true,
                ToolResult { .. } => provider_manages_tools,
                _ => false,
            });
        }

        match event {
            ToolUseStart { id, name } => {
                if !matches!(self.message, Receiving) {
                    return Err("a tool call started before the previous call ended");
                }
                if id.trim().is_empty() || name.trim().is_empty() {
                    return Err("tool call is missing its ID or name");
                }
                if !self.tool_ids.insert(id.clone()) {
                    return Err("duplicate tool call ID");
                }
                self.message = ReceivingTool(String::new());
            }
            ToolInputDelta(delta) => {
                let ReceivingTool(input) = &mut self.message else {
                    return Err("tool arguments arrived outside a tool call");
                };
                input.push_str(delta);
            }
            ToolUseEnd => {
                let ReceivingTool(input) = &self.message else {
                    return Err("tool call ended before it started");
                };
                if !valid_arguments(input, 0) {
                    return Err("tool arguments must contain a complete JSON object");
                }
                self.message = Receiving;
            }
            NativeToolCall { request_id, .. } => {
                if request_id.trim().is_empty() || !self.tool_ids.insert(request_id.clone()) {
                    return Err("missing or duplicate native tool request ID");
                }
                self.native_tool_requested = true;
            }
            MessageEnd { .. } => {
                if matches!(self.message, ReceivingTool(_)) {
                    return Err("message ended with an unfinished tool call");
                }
                self.message = Ended;
            }
            _ => {}
        }
        Ok(true)
    }
}

fn valid_arguments(input: &str, depth: u8) -> bool {
    let input = input.trim();
    if matches!(input, "" | "null") {
        // Some providers omit deltas for a no-argument tool.
        return depth == 0;
    }
    if depth >= 4 {
        return false;
    }
    match input.as_bytes()[0] {
        b'{' => serde_json::from_str::<&RawValue>(input).is_ok(),
        // Preserve the existing compatibility for complete encoded objects.
        b'[' if depth == 0 => serde_json::from_str::<[&RawValue; 1]>(input)
            .is_ok_and(|[inner]| valid_arguments(inner.get(), depth + 1)),
        b'"' => serde_json::from_str::<String>(input)
            .is_ok_and(|inner| valid_arguments(&inner, depth + 1)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn end() -> StreamEvent {
        StreamEvent::MessageEnd { stop_reason: None }
    }

    fn start(id: &str) -> StreamEvent {
        StreamEvent::ToolUseStart {
            id: id.into(),
            name: "write".into(),
        }
    }

    fn events(events: Vec<StreamEvent>) -> EventStream {
        Box::pin(futures::stream::iter(events.into_iter().map(Ok)))
    }

    #[tokio::test]
    async fn eof_after_text_or_completed_tool_arguments_is_an_error() {
        for input in [
            vec![StreamEvent::TextDelta("partial".into())],
            vec![start("a"), StreamEvent::ToolUseEnd],
        ] {
            let output: Vec<_> = guard_stream(events(input), false).collect().await;
            assert_eq!(
                output.last().unwrap().as_ref().unwrap_err().to_string(),
                INCOMPLETE_STREAM
            );
        }
    }

    #[tokio::test]
    async fn a_completed_message_ignores_late_output_and_repeated_end_but_keeps_metadata() {
        let output: Vec<_> = guard_stream(
            events(vec![
                end(),
                start("late"),
                end(),
                StreamEvent::SessionId("session".into()),
            ]),
            false,
        )
        .collect()
        .await;
        assert!(matches!(
            output.as_slice(),
            [
                Ok(StreamEvent::MessageEnd { .. }),
                Ok(StreamEvent::SessionId(_))
            ]
        ));
    }

    #[test]
    fn rejects_malformed_arguments_and_duplicate_ids() {
        let mut attempt = Attempt::default();
        assert!(attempt.accept(&start("a"), false).unwrap());
        assert!(
            attempt
                .accept(
                    &StreamEvent::ToolInputDelta(r#"{"file_path":"a""#.into()),
                    false
                )
                .unwrap()
        );
        assert!(attempt.accept(&end(), false).is_err());
        assert!(attempt.accept(&StreamEvent::ToolUseEnd, false).is_err());
        assert!(
            attempt
                .accept(&StreamEvent::ToolInputDelta("}".into()), false)
                .unwrap()
        );
        assert!(attempt.accept(&StreamEvent::ToolUseEnd, false).unwrap());
        assert!(attempt.accept(&start("a"), false).is_err());
        assert!(
            attempt
                .accept(&StreamEvent::RetryRollback { attempt: 2, max: 3 }, false)
                .unwrap()
        );
        assert!(attempt.accept(&start("a"), false).unwrap());
        assert!(!valid_arguments(r#""{\"file_path\":\"a\"""#, 0));
    }
}
