//! Parallel execution of independent read-only tool calls.
//!
//! The turn loop (`turn_loops.rs`) and the streaming loop
//! (`turn_streaming_mpsc.rs`) both execute tool calls in a simple sequential
//! `for` loop. When a model emits 3-5 parallel reads/greps (modern providers
//! do this routinely), each 200-500 ms tool call stacks to 1-2.5 s of wall
//! time that should be ~max(single call).
//!
//! This module partitions a batch of tool calls into consecutive runs of
//! [`ToolExecutionClass::ReadOnly`] tools and executes each maximal run
//! concurrently, capped at `PARALLEL_READ_ONLY_CAP` permits. Mutating,
//! exclusive, background, and external-effect calls remain serialized and
//! preserve their original ordering. Results are assembled in the original
//! call order regardless of completion order, because some providers
//! (Anthropic in particular) are stricter about result ordering matching
//! call ordering.

use crate::alphacode_message_types::ToolCall;
use crate::alphacode_tool_core::ToolExecutionClass;

/// Maximum number of read-only tools that may execute concurrently within a
/// single batch. Conservative default; the `batch` tool already caps its own
/// fan-out at 4 (`MAX_CONCURRENT_READ_ONLY` in `tool/batch.rs`).
pub const PARALLEL_READ_ONLY_CAP: usize = 4;

/// A single tool call awaiting execution, with its original position in the
/// batch preserved so results can be reassembled in order.
#[derive(Clone, Debug)]
pub struct PendingToolCall {
    pub index: usize,
    pub call: ToolCall,
}

/// Partition `calls` into consecutive runs. Each run is either:
/// - `Parallel`: a maximal run of `ReadOnly` tools (length >= 2).
/// - `Serial`: a single non-ReadOnly call, or a run of length 1.
///
/// Runs of length 1 are never parallelized even if ReadOnly, because the
/// overhead of spawning a task for a single call is higher than executing
/// it inline.
pub struct ExecutionPlan {
    pub runs: Vec<Run>,
}

#[derive(Clone, Debug)]
pub enum Run {
    /// Two or more ReadOnly calls that may execute concurrently.
    Parallel(Vec<PendingToolCall>),
    /// One or more calls that must execute sequentially in this order.
    Serial(Vec<PendingToolCall>),
}

impl ExecutionPlan {
    /// Build an execution plan from a slice of tool calls.
    pub fn build(calls: &[ToolCall], classes: &[ToolExecutionClass]) -> ExecutionPlan {
        assert_eq!(
            calls.len(),
            classes.len(),
            "ExecutionPlan::build requires parallel slices"
        );
        let mut runs: Vec<Run> = Vec::new();
        let mut i = 0;
        while i < calls.len() {
            if classes[i] == ToolExecutionClass::ReadOnly {
                // Collect the maximal run of ReadOnly calls.
                let start = i;
                while i < calls.len() && classes[i] == ToolExecutionClass::ReadOnly {
                    i += 1;
                }
                let run: Vec<PendingToolCall> = (start..i)
                    .map(|idx| PendingToolCall {
                        index: idx,
                        call: calls[idx].clone(),
                    })
                    .collect();
                if run.len() >= 2 {
                    runs.push(Run::Parallel(run));
                } else {
                    runs.push(Run::Serial(run));
                }
            } else {
                // Collect the maximal run of consecutive non-ReadOnly calls.
                // They must execute sequentially in this order, so one Serial
                // run covers them all (a run of length 1 behaves identically
                // to a single call).
                let start = i;
                while i < calls.len() && classes[i] != ToolExecutionClass::ReadOnly {
                    i += 1;
                }
                let run: Vec<PendingToolCall> = (start..i)
                    .map(|idx| PendingToolCall {
                        index: idx,
                        call: calls[idx].clone(),
                    })
                    .collect();
                runs.push(Run::Serial(run));
            }
        }
        ExecutionPlan { runs }
    }

    /// Total number of tool calls covered by this plan.
    pub fn total_calls(&self) -> usize {
        self.runs
            .iter()
            .map(|r| match r {
                Run::Parallel(v) => v.len(),
                Run::Serial(v) => v.len(),
            })
            .sum()
    }

    /// Whether the plan actually parallelizes anything.
    pub fn has_parallel_runs(&self) -> bool {
        self.runs.iter().any(|r| matches!(r, Run::Parallel(_)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alphacode_message_types::ToolCall;

    fn make_call(name: &str, id: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            input: serde_json::json!({}),
            intent: None,
            thought_signature: None,
        }
    }

    #[test]
    fn parallel_run_of_readonly_tools() {
        let calls = vec![
            make_call("read", "1"),
            make_call("read", "2"),
            make_call("read", "3"),
        ];
        let classes = vec![
            ToolExecutionClass::ReadOnly,
            ToolExecutionClass::ReadOnly,
            ToolExecutionClass::ReadOnly,
        ];
        let plan = ExecutionPlan::build(&calls, &classes);
        assert_eq!(plan.runs.len(), 1);
        assert!(matches!(&plan.runs[0], Run::Parallel(v) if v.len() == 3));
    }

    #[test]
    fn mixed_run_partitions_correctly() {
        let calls = vec![
            make_call("read", "1"),
            make_call("read", "2"),
            make_call("write", "3"),
            make_call("bash", "4"),
            make_call("ls", "5"),
        ];
        let classes = vec![
            ToolExecutionClass::ReadOnly,
            ToolExecutionClass::ReadOnly,
            ToolExecutionClass::Mutating,
            ToolExecutionClass::Mutating,
            ToolExecutionClass::ReadOnly,
        ];
        let plan = ExecutionPlan::build(&calls, &classes);
        assert_eq!(plan.runs.len(), 3);
        assert!(matches!(&plan.runs[0], Run::Parallel(v) if v.len() == 2));
        assert!(matches!(&plan.runs[1], Run::Serial(v) if v.len() == 2));
        assert!(matches!(&plan.runs[2], Run::Serial(v) if v.len() == 1));
    }

    #[test]
    fn single_readonly_is_serial() {
        let calls = vec![make_call("read", "1")];
        let classes = vec![ToolExecutionClass::ReadOnly];
        let plan = ExecutionPlan::build(&calls, &classes);
        assert_eq!(plan.runs.len(), 1);
        assert!(matches!(&plan.runs[0], Run::Serial(_)));
    }

    #[test]
    fn empty_input_yields_empty_plan() {
        let calls: Vec<ToolCall> = vec![];
        let classes: Vec<ToolExecutionClass> = vec![];
        let plan = ExecutionPlan::build(&calls, &classes);
        assert!(plan.runs.is_empty());
        assert!(!plan.has_parallel_runs());
    }
}
