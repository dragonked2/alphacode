use super::{Registry, Tool, ToolContext, ToolExecutionClass, ToolOutput};
use crate::alphacode_app_core::bus::{BatchSubcallProgress, BatchSubcallState};
use crate::alphacode_app_core::message::ToolCall;
use anyhow::Result;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;

const MAX_PARALLEL: usize = 20;
const MAX_CONCURRENT_READ_ONLY: usize = 4;

pub(crate) fn generic_batch_schema() -> Value {
    json!({
        "type": "object",
        "required": ["tool_calls"],
        "properties": {
            "intent": super::intent_schema_property(),
            "tool_calls": {
                "type": "array",
                "items": {
                    "type": "object",
                    "required": ["tool"],
                    "properties": {
                        "tool": {
                            "type": "string",
                            "description": "Tool name."
                        },
                        "intent": super::intent_schema_property()
                    },
                    "additionalProperties": true
                },
                "minItems": 1,
                // Keep in sync with MAX_PARALLEL: a mismatch teaches the model
                // a wrong limit (accuracy) and rejects valid plans.
                "maxItems": 20
            }
        }
    })
}

#[derive(Debug)]
struct BatchExecutionProgress {
    states: Vec<BatchSubcallState>,
    running: HashSet<usize>,
    completed: usize,
    last_completed: Option<String>,
}

impl BatchExecutionProgress {
    fn new(total: usize) -> Self {
        Self {
            states: vec![BatchSubcallState::Pending; total],
            running: HashSet::new(),
            completed: 0,
            last_completed: None,
        }
    }
}

fn batch_tool_call(index: usize, tool_name: &str, parameters: &Value) -> ToolCall {
    ToolCall {
        id: format!("batch-{}-{}", index + 1, tool_name),
        name: tool_name.to_string(),
        input: parameters.clone(),
        intent: ToolCall::intent_from_input(parameters),
        thought_signature: None,
    }
}

fn ordered_batch_subcalls(
    subcalls: &[(usize, String, Value)],
    progress: &BatchExecutionProgress,
) -> Vec<BatchSubcallProgress> {
    subcalls
        .iter()
        .map(|(index, tool_name, parameters)| BatchSubcallProgress {
            index: index + 1,
            tool_call: batch_tool_call(*index, tool_name, parameters),
            state: progress
                .states
                .get(*index)
                .copied()
                .unwrap_or(BatchSubcallState::Pending),
        })
        .collect()
}

fn publish_batch_progress(
    ctx: &ToolContext,
    subcalls: &[(usize, String, Value)],
    progress: &BatchExecutionProgress,
) {
    let running = subcalls
        .iter()
        .filter(|(index, _, _)| progress.running.contains(index))
        .map(|(index, name, input)| batch_tool_call(*index, name, input))
        .collect();

    crate::bus::Bus::global().publish(crate::bus::BusEvent::BatchProgress(
        crate::bus::BatchProgress {
            session_id: ctx.session_id.clone(),
            tool_call_id: ctx.tool_call_id.clone(),
            total: subcalls.len(),
            completed: progress.completed,
            last_completed: progress.last_completed.clone(),
            running,
            subcalls: ordered_batch_subcalls(subcalls, progress),
        },
    ));
}

async fn record_batch_start(
    ctx: &ToolContext,
    subcalls: &[(usize, String, Value)],
    progress: &Mutex<BatchExecutionProgress>,
    index: usize,
) {
    let mut progress = progress.lock().await;
    if let Some(state) = progress.states.get_mut(index) {
        *state = BatchSubcallState::Running;
    }
    progress.running.insert(index);
    publish_batch_progress(ctx, subcalls, &progress);
}

async fn record_batch_completion(
    ctx: &ToolContext,
    subcalls: &[(usize, String, Value)],
    progress: &Mutex<BatchExecutionProgress>,
    index: usize,
    tool_name: &str,
    failed: bool,
) {
    let mut progress = progress.lock().await;
    progress.running.remove(&index);
    if let Some(state) = progress.states.get_mut(index) {
        *state = if failed {
            BatchSubcallState::Failed
        } else {
            BatchSubcallState::Succeeded
        };
    }
    progress.completed = progress.completed.saturating_add(1);
    progress.last_completed = Some(tool_name.to_string());
    publish_batch_progress(ctx, subcalls, &progress);
}

pub struct BatchTool {
    registry: Registry,
}

impl BatchTool {
    pub fn new(registry: Registry) -> Self {
        Self { registry }
    }
}

#[derive(Deserialize)]
struct BatchInput {
    tool_calls: Vec<ToolCallInput>,
}

#[derive(Deserialize, Clone)]
struct ToolCallInput {
    #[serde(alias = "name")]
    tool: String,
    #[serde(default)]
    parameters: Option<Value>,
}

impl ToolCallInput {
    fn resolved_parameters(self) -> (String, Value) {
        if let Some(params) = self.parameters {
            return (self.tool, params);
        }
        (self.tool, Value::Object(Default::default()))
    }
}

/// Try to fix common LLM mistakes in batch tool_calls:
/// - Parameters placed at the same level as "tool" instead of nested under "parameters"
/// - "name" used instead of "tool" for the tool name key
/// - "arguments", "args", or "input" used instead of "parameters"
fn normalize_batch_input(mut input: Value) -> Value {
    if let Some(calls) = input.get_mut("tool_calls").and_then(|v| v.as_array_mut()) {
        for call in calls.iter_mut() {
            if let Some(obj) = call.as_object_mut() {
                // Normalize "name" -> "tool" if the model used the wrong key
                if !obj.contains_key("tool")
                    && let Some(name_val) = obj.remove("name")
                {
                    obj.insert("tool".to_string(), name_val);
                }

                if !obj.contains_key("parameters") {
                    for alias in ["arguments", "args", "input"] {
                        if let Some(alias_val) = obj.remove(alias) {
                            obj.insert("parameters".to_string(), alias_val);
                            break;
                        }
                    }
                }

                // Canonical batch calls may keep the display intent beside
                // `parameters`. Forward it into the effective tool input so
                // live progress events and the nested tool execution retain it.
                let top_level_intent = obj
                    .get("intent")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|intent| !intent.is_empty())
                    .map(ToString::to_string);
                if let Some(intent) = top_level_intent
                    && let Some(params) = obj.get_mut("parameters").and_then(Value::as_object_mut)
                {
                    params.insert("intent".to_string(), Value::String(intent));
                }

                if !obj.contains_key("parameters") && obj.contains_key("tool") {
                    let tool_name = obj.get("tool").cloned();
                    let mut params = serde_json::Map::new();
                    let keys: Vec<String> = obj.keys().filter(|k| *k != "tool").cloned().collect();
                    for key in keys {
                        if let Some(val) = obj.remove(&key) {
                            params.insert(key, val);
                        }
                    }
                    if !params.is_empty() {
                        obj.insert("parameters".to_string(), Value::Object(params));
                    }
                    if let Some(name) = tool_name {
                        obj.insert("tool".to_string(), name);
                    }
                }
            }
        }
    }
    input
}

#[async_trait]
impl Tool for BatchTool {
    fn name(&self) -> &str {
        "batch"
    }

    fn description(&self) -> &str {
        "Run multiple independent tool calls efficiently. Read-only calls run concurrently (up to 4); mutating and external-effect calls stay ordered. Use when calls do not depend on each other. Each sub-call has its own error handling."
    }

    fn parameters_schema(&self) -> Value {
        generic_batch_schema()
    }

    async fn execute(&self, input: Value, ctx: ToolContext) -> Result<ToolOutput> {
        let input = normalize_batch_input(input);
        let params: BatchInput = serde_json::from_value(input)?;

        if params.tool_calls.is_empty() {
            return Err(anyhow::anyhow!("No tool calls provided"));
        }

        if params.tool_calls.len() > MAX_PARALLEL {
            return Err(anyhow::anyhow!(
                "Maximum {} parallel tool calls allowed",
                MAX_PARALLEL
            ));
        }

        // Check for disallowed tools
        for tc in &params.tool_calls {
            if Registry::resolve_tool_name(&tc.tool) == "batch" {
                return Err(anyhow::anyhow!("Cannot batch the 'batch' tool"));
            }
        }

        // Execute all tools in parallel, emitting progress events as each completes
        let num_tools = params.tool_calls.len();
        use futures::StreamExt;
        let subcalls: Arc<Vec<(usize, String, Value)>> = Arc::new(
            params
                .tool_calls
                .into_iter()
                .enumerate()
                .map(|(i, tc)| {
                    let (tool_name, parameters) = tc.resolved_parameters();
                    let tool_name = Registry::resolve_tool_name(&tool_name).to_string();
                    (i, tool_name, parameters)
                })
                .collect(),
        );
        let progress = Arc::new(Mutex::new(BatchExecutionProgress::new(num_tools)));

        // Start queued work as Pending. A subcall only becomes Running once
        // its future is polled and is about to invoke the underlying tool.
        {
            let progress = progress.lock().await;
            publish_batch_progress(&ctx, &subcalls, &progress);
        }

        let mut results: Vec<(usize, String, Result<ToolOutput>)> = Vec::with_capacity(num_tools);
        let mut read_group: Vec<(usize, String, Value)> = Vec::new();

        for (index, tool_name, parameters) in subcalls.iter() {
            let class = self
                .registry
                .execution_class(tool_name, parameters.clone())
                .await;
            if class == ToolExecutionClass::ReadOnly {
                read_group.push((*index, tool_name.clone(), parameters.clone()));
                continue;
            }

            // Flush the independent read-only prefix before any mutation. This
            // preserves the caller's ordering barrier for writes and external
            // effects while still overlapping safe work.
            let pending = std::mem::take(&mut read_group);
            let futures = pending.into_iter().map(|(i, name, input)| {
                let registry = self.registry.clone();
                let sub_ctx = ctx.for_subcall(format!("batch-{}-{}", i + 1, name));
                let progress = Arc::clone(&progress);
                let subcalls = Arc::clone(&subcalls);
                let progress_ctx = ctx.clone();
                async move {
                    record_batch_start(&progress_ctx, &subcalls, &progress, i).await;
                    let result = registry.execute(&name, input, sub_ctx).await;
                    record_batch_completion(
                        &progress_ctx,
                        &subcalls,
                        &progress,
                        i,
                        &name,
                        result.is_err(),
                    )
                    .await;
                    (i, name, result)
                }
            });
            let mut stream =
                futures::stream::iter(futures).buffer_unordered(MAX_CONCURRENT_READ_ONLY);
            while let Some((i, name, result)) = stream.next().await {
                results.push((i, name, result));
            }

            record_batch_start(&ctx, &subcalls, &progress, *index).await;
            let sub_ctx = ctx.for_subcall(format!("batch-{}-{}", index + 1, tool_name));
            let result = self
                .registry
                .execute(tool_name, parameters.clone(), sub_ctx)
                .await;
            record_batch_completion(
                &ctx,
                &subcalls,
                &progress,
                *index,
                tool_name,
                result.is_err(),
            )
            .await;
            results.push((*index, tool_name.clone(), result));
        }

        // Final read-only suffix.
        let pending = std::mem::take(&mut read_group);
        let futures = pending.into_iter().map(|(i, name, input)| {
            let registry = self.registry.clone();
            let sub_ctx = ctx.for_subcall(format!("batch-{}-{}", i + 1, name));
            let progress = Arc::clone(&progress);
            let subcalls = Arc::clone(&subcalls);
            let progress_ctx = ctx.clone();
            async move {
                record_batch_start(&progress_ctx, &subcalls, &progress, i).await;
                let result = registry.execute(&name, input, sub_ctx).await;
                record_batch_completion(
                    &progress_ctx,
                    &subcalls,
                    &progress,
                    i,
                    &name,
                    result.is_err(),
                )
                .await;
                (i, name, result)
            }
        });
        let mut stream = futures::stream::iter(futures).buffer_unordered(MAX_CONCURRENT_READ_ONLY);
        while let Some((i, name, result)) = stream.next().await {
            results.push((i, name, result));
        }

        // Restore original order for provider/model correlation.
        results.sort_by_key(|(i, _, _)| *i);

        // Format results: pre-size to avoid O(n^2) regrowth on large
        // fan-outs (20 x 50KB outputs). Estimate 4KB per tool upfront.
        let mut output = String::with_capacity(num_tools * 4096);
        let mut success_count = 0;
        let mut error_count = 0;
        let mut failed_tools = Vec::new();

        for (i, tool_name, result) in results {
            // Single `format!` per subcall (not per chunk) is fine; the
            // `with_capacity` above avoids regrowth on large outputs.
            output.push_str(&format!("--- [{}] {} ---\n", i + 1, tool_name));
            match result {
                Ok(out) => {
                    success_count += 1;
                    let max_per_tool = 50_000 / num_tools.max(1);
                    if out.output.len() > max_per_tool {
                        output.push_str(crate::util::truncate_str(&out.output, max_per_tool));
                        output.push_str("...\n(truncated)");
                    } else {
                        output.push_str(&out.output);
                    }
                }
                Err(e) => {
                    error_count += 1;
                    failed_tools.push(tool_name.clone());
                    // Same recovery-hint treatment as top-level tool errors:
                    // a sub-call that names its next step recovers in one
                    // retry instead of two.
                    output.push_str(&super::agent_facing_error(&tool_name, &e));
                }
            }
            output.push_str("\n\n");
        }

        if error_count > 0 {
            crate::logging::warn(&format!(
                "[tool:batch] {} of {} subcalls failed for {} in session {}: {}",
                error_count,
                num_tools,
                ctx.tool_call_id,
                ctx.session_id,
                failed_tools.join(", ")
            ));
        }

        output.push_str(&format!(
            "Completed: {} succeeded, {} failed",
            success_count, error_count
        ));

        Ok(ToolOutput::new(output))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_tool_core::{ToolContext, ToolExecutionMode};
    use async_trait::async_trait;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    #[derive(Default)]
    struct ProbeState {
        read_active: AtomicUsize,
        read_max: AtomicUsize,
        write_active: AtomicUsize,
        write_max: AtomicUsize,
        write_order: Mutex<Vec<String>>,
    }

    struct ProbeTool {
        class: ToolExecutionClass,
        state: Arc<ProbeState>,
    }

    #[async_trait]
    impl Tool for ProbeTool {
        fn name(&self) -> &str {
            match self.class {
                ToolExecutionClass::ReadOnly => "probe_read",
                _ => "probe_write",
            }
        }

        fn description(&self) -> &str {
            "scheduler probe"
        }

        fn parameters_schema(&self) -> Value {
            json!({
                "type": "object",
                "properties": { "id": { "type": "string" } }
            })
        }

        fn execution_class(&self, _input: &Value) -> ToolExecutionClass {
            self.class
        }

        async fn execute(&self, input: Value, _ctx: ToolContext) -> Result<ToolOutput> {
            let id = input
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
                .to_string();
            let (active, max) = match self.class {
                ToolExecutionClass::ReadOnly => (&self.state.read_active, &self.state.read_max),
                _ => (&self.state.write_active, &self.state.write_max),
            };
            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
            max.fetch_max(now, Ordering::SeqCst);
            if !matches!(self.class, ToolExecutionClass::ReadOnly) {
                self.state.write_order.lock().unwrap().push(id.clone());
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(ToolOutput::new(id))
        }
    }

    fn probe_context() -> ToolContext {
        ToolContext {
            session_id: "batch-scheduler-test".to_string(),
            message_id: "message".to_string(),
            tool_call_id: "batch".to_string(),
            working_dir: None,
            stdin_request_tx: None,
            graceful_shutdown_signal: None,
            execution_mode: ToolExecutionMode::Direct,
        }
    }

    #[test]
    fn batch_progress_distinguishes_pending_running_and_completed_calls() {
        let subcalls = vec![
            (0, "read".to_string(), json!({"file_path": "a.rs"})),
            (1, "read".to_string(), json!({"file_path": "b.rs"})),
            (2, "write".to_string(), json!({"file_path": "c.rs"})),
        ];
        let mut progress = BatchExecutionProgress::new(subcalls.len());
        assert_eq!(
            ordered_batch_subcalls(&subcalls, &progress)
                .iter()
                .map(|call| call.state)
                .collect::<Vec<_>>(),
            vec![
                BatchSubcallState::Pending,
                BatchSubcallState::Pending,
                BatchSubcallState::Pending,
            ]
        );

        progress.states[0] = BatchSubcallState::Succeeded;
        progress.states[1] = BatchSubcallState::Running;
        progress.running.insert(1);
        assert_eq!(
            ordered_batch_subcalls(&subcalls, &progress)
                .iter()
                .map(|call| call.state)
                .collect::<Vec<_>>(),
            vec![
                BatchSubcallState::Succeeded,
                BatchSubcallState::Running,
                BatchSubcallState::Pending,
            ]
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn batch_overlaps_read_only_groups_but_caps_and_serializes_writes() {
        // Batch subcalls are dispatched through `Registry::execute`, which runs
        // the process-global `pre_tool` gate. Serialize against tests that
        // install one, or a leaked policy script blocks the probes and the
        // concurrency assertions below fail nondeterministically.
        let _env_guard = crate::storage::lock_test_env();
        let state = Arc::new(ProbeState::default());
        let registry = Registry::empty();
        {
            let mut tools = registry.tools.write().await;
            tools.insert(
                "probe_read".to_string(),
                Arc::new(ProbeTool {
                    class: ToolExecutionClass::ReadOnly,
                    state: Arc::clone(&state),
                }),
            );
            tools.insert(
                "probe_write".to_string(),
                Arc::new(ProbeTool {
                    class: ToolExecutionClass::Mutating,
                    state: Arc::clone(&state),
                }),
            );
        }
        let batch = BatchTool::new(registry);
        let result = batch
            .execute(
                json!({
                    "tool_calls": [
                        {"tool": "probe_read", "parameters": {"id": "r1"}},
                        {"tool": "probe_read", "parameters": {"id": "r2"}},
                        {"tool": "probe_read", "parameters": {"id": "r3"}},
                        {"tool": "probe_read", "parameters": {"id": "r4"}},
                        {"tool": "probe_read", "parameters": {"id": "r5"}},
                        {"tool": "probe_write", "parameters": {"id": "w1"}},
                        {"tool": "probe_write", "parameters": {"id": "w2"}}
                    ]
                }),
                probe_context(),
            )
            .await
            .expect("batch probe should execute");

        let read_max = state.read_max.load(Ordering::SeqCst);
        let write_max = state.write_max.load(Ordering::SeqCst);
        assert!((2..=4).contains(&read_max), "read cap/overlap: {read_max}");
        assert_eq!(write_max, 1, "mutations must remain serialized");
        assert_eq!(
            *state.write_order.lock().unwrap(),
            vec!["w1".to_string(), "w2".to_string()]
        );
        assert!(result.output.contains("--- [1] probe_read ---"));
        assert!(result.output.contains("--- [7] probe_write ---"));
    }
}
