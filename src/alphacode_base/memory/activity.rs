use super::*;
use crate::memory_types::PipelineState;
use std::sync::Mutex;
use std::time::Duration;

/// Global memory activity state - updated by sidecar, read by info widget
static MEMORY_ACTIVITY: Mutex<Option<MemoryActivity>> = Mutex::new(None);

/// Maximum number of recent events to keep
const MAX_RECENT_EVENTS: usize = 10;

/// Staleness timeout: auto-reset to Idle if state has been non-Idle for this long
const STALENESS_TIMEOUT_SECS: u64 = 180;
const FOUND_RELEVANT_TIMEOUT_SECS: u64 = 15;

/// Get current memory activity state
pub fn get_activity() -> Option<MemoryActivity> {
    MEMORY_ACTIVITY.lock().ok().and_then(|guard| guard.clone())
}

pub fn activity_snapshot() -> Option<crate::protocol::MemoryActivitySnapshot> {
    MEMORY_ACTIVITY.lock().ok().and_then(|guard| guard.as_ref().map(memory_activity_snapshot))
}

pub fn apply_remote_activity_snapshot(snapshot: &crate::protocol::MemoryActivitySnapshot) {
    if let Ok(mut guard) = MEMORY_ACTIVITY.lock() {
        // Snapshots contain no event history; do not mix previous local/remote histories.
        let recent_events = Vec::new();
        let now = Instant::now();
        let state_since = now
            .checked_sub(Duration::from_millis(snapshot.state_age_ms))
            .unwrap_or(now);

        *guard = Some(MemoryActivity {
            state: private_state(from_snapshot_state(&snapshot.state)),
            state_since,
            pipeline: snapshot.pipeline.as_ref().map(from_snapshot_pipeline),
            recent_events,
        });
    }
}

/// Update the memory activity state
fn private_state(state: MemoryState) -> MemoryState {
    match state {
        MemoryState::Extracting { .. } => MemoryState::Extracting { reason: "Memory extraction".into() },
        MemoryState::Maintaining { .. } => MemoryState::Maintaining { phase: "Graph maintenance".into() },
        MemoryState::ToolAction { .. } => MemoryState::ToolAction { action: "Memory tool".into(), detail: Default::default() },
        other => other,
    }
}

pub fn set_state(state: MemoryState) {
    let state = private_state(state);
    if let Ok(mut guard) = MEMORY_ACTIVITY.lock() {
        if let Some(activity) = guard.as_mut() {
            activity.state = state;
            activity.state_since = Instant::now();
        } else {
            *guard = Some(MemoryActivity {
                state,
                state_since: Instant::now(),
                pipeline: None,
                recent_events: Vec::new(),
            });
        }
    }
}

/// Add an event to the activity log
pub fn add_event(mut kind: MemoryEventKind) {
    crate::memory_log::log_event(&kind);
    // The global activity view may be shared across sessions. Store no raw text.
    match &mut kind {
        MemoryEventKind::SidecarRelevant { memory_preview } | MemoryEventKind::MemorySurfaced { memory_preview } => *memory_preview = "Memory matched".into(),
        MemoryEventKind::MemoryInjected { preview, items, .. } => { *preview = "Memory injected".into(); items.clear(); }
        MemoryEventKind::Error { message } => *message = "Memory operation failed".into(),
        MemoryEventKind::ToolRemembered { content, .. } => *content = "[private]".into(),
        MemoryEventKind::ToolRecalled { query, .. } => *query = "[private]".into(),
        MemoryEventKind::MaintenanceTagInferred { tag, .. } => *tag = "[private]".into(),
        MemoryEventKind::ExtractionStarted { reason } => *reason = "Memory extraction".into(),
        MemoryEventKind::ToolForgot { id } => id.clear(),
        MemoryEventKind::ToolTagged { id, tags } => { id.clear(); tags.clear(); }
        MemoryEventKind::ToolLinked { from, to } => { from.clear(); to.clear(); }
        _ => {}
    }

    if let Ok(mut guard) = MEMORY_ACTIVITY.lock() {
        let event = MemoryEvent {
            kind,
            timestamp: Instant::now(),
            detail: None,
        };

        if let Some(activity) = guard.as_mut() {
            activity.recent_events.insert(0, event);
            activity.recent_events.truncate(MAX_RECENT_EVENTS);
        } else {
            *guard = Some(MemoryActivity {
                state: MemoryState::Idle,
                state_since: Instant::now(),
                pipeline: None,
                recent_events: vec![event],
            });
        }
    }
}

/// Start a new pipeline run (called at the beginning of each memory check)
pub fn pipeline_start() {
    if let Ok(mut guard) = MEMORY_ACTIVITY.lock() {
        if let Some(activity) = guard.as_mut() {
            activity.pipeline = Some(PipelineState::new());
        } else {
            *guard = Some(MemoryActivity {
                state: MemoryState::Idle,
                state_since: Instant::now(),
                pipeline: Some(PipelineState::new()),
                recent_events: Vec::new(),
            });
        }
    }
}

/// Update pipeline step status
#[expect(
    clippy::collapsible_if,
    reason = "Memory activity updates keep optional state transitions explicit"
)]
pub fn pipeline_update(f: impl FnOnce(&mut PipelineState)) {
    if let Ok(mut guard) = MEMORY_ACTIVITY.lock() {
        if let Some(activity) = guard.as_mut() {
            if let Some(pipeline) = activity.pipeline.as_mut() {
                f(pipeline);
                if let Some((done, total)) = pipeline.verify_progress {
                    pipeline.verify_progress = Some((done.min(total), total));
                }
                activity.state_since = Instant::now();
            }
        }
    }
}

/// A UI watchdog, not evidence that a worker completed or was canceled.
fn expired_state(activity: &MemoryActivity) -> bool {
    let limit = match &activity.state {
        MemoryState::Idle => return false,
        MemoryState::FoundRelevant { .. } => FOUND_RELEVANT_TIMEOUT_SECS,
        _ => STALENESS_TIMEOUT_SECS,
    };
    activity.state_since.elapsed().as_secs() >= limit
}

fn expire_step(status: &mut StepStatus) {
    if matches!(&*status, StepStatus::Running) { *status = StepStatus::Error; }
    else if matches!(&*status, StepStatus::Pending) { *status = StepStatus::Skipped; }
}

pub fn check_staleness() -> bool {
    let expired = {
        let Ok(mut guard) = MEMORY_ACTIVITY.lock() else { return false; };
        let Some(activity) = guard.as_mut() else { return false; };
        if !expired_state(activity) { return false; }
        if let Some(pipeline) = activity.pipeline.as_mut() {
            expire_step(&mut pipeline.search);
            expire_step(&mut pipeline.verify);
            expire_step(&mut pipeline.inject);
            expire_step(&mut pipeline.maintain);
        }
        activity.state = MemoryState::Idle;
        activity.state_since = Instant::now();
        true
    };
    if expired { crate::logging::info("Memory activity display expired; worker completion is unverified"); }
    expired
}

/// Clear activity (reset to idle with no events)
pub fn clear_activity() {
    if let Ok(mut guard) = MEMORY_ACTIVITY.lock() {
        *guard = None;
    }
}

/// Record injection metadata without copying private memory into diagnostic UI
/// events. The actual conversation/prompt remains the source of content.
pub fn record_injected_prompt(prompt: &str, count: usize, age_ms: u64) {
    crate::telemetry::record_memory_injected(count, age_ms);
    let preview = format!("{count} memory item(s) injected");
    let items: Vec<InjectedMemoryItem> = Vec::new();
    add_event(MemoryEventKind::MemoryInjected {
        count, prompt_chars: prompt.chars().count(), age_ms,
        preview: preview.clone(), items,
    });
    add_event(MemoryEventKind::MemorySurfaced { memory_preview: preview });
}

fn memory_activity_snapshot(activity: &MemoryActivity) -> crate::protocol::MemoryActivitySnapshot {
    crate::protocol::MemoryActivitySnapshot {
        state: snapshot_state(&activity.state),
        state_age_ms: activity.state_since.elapsed().as_millis() as u64,
        pipeline: activity.pipeline.as_ref().map(snapshot_pipeline),
    }
}

fn snapshot_state(state: &MemoryState) -> crate::protocol::MemoryStateSnapshot {
    match state {
        MemoryState::Idle => crate::protocol::MemoryStateSnapshot::Idle,
        MemoryState::Embedding => crate::protocol::MemoryStateSnapshot::Embedding,
        MemoryState::SidecarChecking { count } => {
            crate::protocol::MemoryStateSnapshot::SidecarChecking { count: *count }
        }
        MemoryState::FoundRelevant { count } => {
            crate::protocol::MemoryStateSnapshot::FoundRelevant { count: *count }
        }
        MemoryState::Extracting { reason } => crate::protocol::MemoryStateSnapshot::Extracting {
            reason: reason.clone(),
        },
        MemoryState::Maintaining { phase } => crate::protocol::MemoryStateSnapshot::Maintaining {
            phase: phase.clone(),
        },
        MemoryState::ToolAction { action, detail } => {
            crate::protocol::MemoryStateSnapshot::ToolAction {
                action: action.clone(),
                detail: detail.clone(),
            }
        }
    }
}

fn snapshot_pipeline(pipeline: &PipelineState) -> crate::protocol::MemoryPipelineSnapshot {
    crate::protocol::MemoryPipelineSnapshot {
        search: snapshot_step_status(&pipeline.search),
        search_result: pipeline.search_result.as_ref().map(snapshot_step_result),
        verify: snapshot_step_status(&pipeline.verify),
        verify_result: pipeline.verify_result.as_ref().map(snapshot_step_result),
        verify_progress: pipeline.verify_progress,
        inject: snapshot_step_status(&pipeline.inject),
        inject_result: pipeline.inject_result.as_ref().map(snapshot_step_result),
        maintain: snapshot_step_status(&pipeline.maintain),
        maintain_result: pipeline.maintain_result.as_ref().map(snapshot_step_result),
    }
}

fn snapshot_step_status(status: &StepStatus) -> crate::protocol::MemoryStepStatusSnapshot {
    match status {
        StepStatus::Pending => crate::protocol::MemoryStepStatusSnapshot::Pending,
        StepStatus::Running => crate::protocol::MemoryStepStatusSnapshot::Running,
        StepStatus::Done => crate::protocol::MemoryStepStatusSnapshot::Done,
        StepStatus::Error => crate::protocol::MemoryStepStatusSnapshot::Error,
        StepStatus::Skipped => crate::protocol::MemoryStepStatusSnapshot::Skipped,
    }
}

fn snapshot_step_result(result: &StepResult) -> crate::protocol::MemoryStepResultSnapshot {
    crate::protocol::MemoryStepResultSnapshot {
        summary: result.summary.clone(),
        latency_ms: result.latency_ms,
    }
}

fn from_snapshot_state(snapshot: &crate::protocol::MemoryStateSnapshot) -> MemoryState {
    match snapshot {
        crate::protocol::MemoryStateSnapshot::Idle => MemoryState::Idle,
        crate::protocol::MemoryStateSnapshot::Embedding => MemoryState::Embedding,
        crate::protocol::MemoryStateSnapshot::SidecarChecking { count } => {
            MemoryState::SidecarChecking { count: *count }
        }
        crate::protocol::MemoryStateSnapshot::FoundRelevant { count } => {
            MemoryState::FoundRelevant { count: *count }
        }
        crate::protocol::MemoryStateSnapshot::Extracting { reason } => MemoryState::Extracting {
            reason: reason.clone(),
        },
        crate::protocol::MemoryStateSnapshot::Maintaining { phase } => MemoryState::Maintaining {
            phase: phase.clone(),
        },
        crate::protocol::MemoryStateSnapshot::ToolAction { action, detail } => {
            MemoryState::ToolAction {
                action: action.clone(),
                detail: detail.clone(),
            }
        }
    }
}

fn from_snapshot_pipeline(snapshot: &crate::protocol::MemoryPipelineSnapshot) -> PipelineState {
    PipelineState {
        search: from_snapshot_step_status(&snapshot.search),
        search_result: snapshot
            .search_result
            .as_ref()
            .map(from_snapshot_step_result),
        verify: from_snapshot_step_status(&snapshot.verify),
        verify_result: snapshot
            .verify_result
            .as_ref()
            .map(from_snapshot_step_result),
        verify_progress: snapshot.verify_progress.map(|(done, total)| (done.min(total), total)),
        inject: from_snapshot_step_status(&snapshot.inject),
        inject_result: snapshot
            .inject_result
            .as_ref()
            .map(from_snapshot_step_result),
        maintain: from_snapshot_step_status(&snapshot.maintain),
        maintain_result: snapshot
            .maintain_result
            .as_ref()
            .map(from_snapshot_step_result),
        started_at: Instant::now(),
    }
}

fn from_snapshot_step_status(snapshot: &crate::protocol::MemoryStepStatusSnapshot) -> StepStatus {
    match snapshot {
        crate::protocol::MemoryStepStatusSnapshot::Pending => StepStatus::Pending,
        crate::protocol::MemoryStepStatusSnapshot::Running => StepStatus::Running,
        crate::protocol::MemoryStepStatusSnapshot::Done => StepStatus::Done,
        crate::protocol::MemoryStepStatusSnapshot::Error => StepStatus::Error,
        crate::protocol::MemoryStepStatusSnapshot::Skipped => StepStatus::Skipped,
    }
}

fn from_snapshot_step_result(snapshot: &crate::protocol::MemoryStepResultSnapshot) -> StepResult {
    StepResult {
        summary: snapshot.summary.clone(),
        latency_ms: snapshot.latency_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stale_steps_are_never_marked_successful() {
        let mut running = StepStatus::Running;
        expire_step(&mut running);
        assert!(matches!(running, StepStatus::Error));
        let mut pending = StepStatus::Pending;
        expire_step(&mut pending);
        assert!(matches!(pending, StepStatus::Skipped));
    }
}
