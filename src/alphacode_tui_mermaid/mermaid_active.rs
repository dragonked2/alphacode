use super::ACTIVE_DIAGRAMS_MAX;
use crate::alphacode_tui_mermaid::DiagramInfo;
use std::sync::{LazyLock, Mutex};

/// Active diagrams for info widget display
/// Updated during markdown rendering, queried by info_widget_data()
static ACTIVE_DIAGRAMS: LazyLock<Mutex<Vec<ActiveDiagram>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

/// Ephemeral diagram preview for in-flight streaming markdown.
/// This should never persist once a streaming segment is committed.
static STREAMING_PREVIEW_DIAGRAM: LazyLock<Mutex<Option<ActiveDiagram>>> =
    LazyLock::new(|| Mutex::new(None));

/// Info about an active diagram (for info widget)
#[derive(Clone)]
struct ActiveDiagram {
    hash: u64,
    width: u32,
    height: u32,
    label: Option<String>,
}

fn to_diagram_info(diagram: ActiveDiagram) -> DiagramInfo {
    DiagramInfo {
        hash: diagram.hash,
        width: diagram.width,
        height: diagram.height,
        label: diagram.label,
    }
}

fn to_active_diagram(diagram: DiagramInfo) -> ActiveDiagram {
    ActiveDiagram {
        hash: diagram.hash,
        width: diagram.width,
        height: diagram.height,
        label: diagram.label,
    }
}

pub fn register_active_diagram(hash: u64, width: u32, height: u32, label: Option<String>) {
    if let Ok(mut diagrams) = ACTIVE_DIAGRAMS.lock() {
        if let Some(pos) = diagrams.iter().position(|d| d.hash == hash) {
            let mut existing = diagrams.remove(pos);
            existing.width = width;
            existing.height = height;
            if label.is_some() {
                existing.label = label;
            }
            diagrams.push(existing);
        } else {
            diagrams.push(ActiveDiagram {
                hash,
                width,
                height,
                label,
            });
        }
        while diagrams.len() > ACTIVE_DIAGRAMS_MAX {
            diagrams.remove(0);
        }
    }
}

/// Register or replace the current streaming preview diagram.
pub fn set_streaming_preview_diagram(hash: u64, width: u32, height: u32, label: Option<String>) {
    if let Ok(mut preview) = STREAMING_PREVIEW_DIAGRAM.lock() {
        *preview = Some(ActiveDiagram {
            hash,
            width,
            height,
            label,
        });
    }
}

/// Clear the current streaming preview diagram.
pub fn clear_streaming_preview_diagram() {
    if let Ok(mut preview) = STREAMING_PREVIEW_DIAGRAM.lock() {
        *preview = None;
    }
}

/// Get active diagrams for info widget display
pub fn get_active_diagrams() -> Vec<DiagramInfo> {
    let preview = STREAMING_PREVIEW_DIAGRAM
        .lock()
        .ok()
        .and_then(|preview| preview.clone());
    let preview_hash = preview.as_ref().map(|d| d.hash);

    let mut out = Vec::new();
    if let Some(diagram) = preview {
        out.push(to_diagram_info(diagram));
    }

    if let Ok(diagrams) = ACTIVE_DIAGRAMS.lock() {
        out.extend(
            diagrams
                .iter()
                .rev()
                .filter(|d| Some(d.hash) != preview_hash)
                .cloned()
                .map(to_diagram_info),
        );
    }

    out
}

/// Snapshot active diagrams (internal order) for temporary overrides in tests/debug
pub fn snapshot_active_diagrams() -> Vec<DiagramInfo> {
    ACTIVE_DIAGRAMS
        .lock()
        .ok()
        .map(|diagrams| diagrams.iter().cloned().map(to_diagram_info).collect())
        .unwrap_or_default()
}

/// Restore active diagrams from a snapshot
pub fn restore_active_diagrams(snapshot: Vec<DiagramInfo>) {
    if let Ok(mut diagrams) = ACTIVE_DIAGRAMS.lock() {
        diagrams.clear();
        diagrams.extend(snapshot.into_iter().map(to_active_diagram));
        while diagrams.len() > ACTIVE_DIAGRAMS_MAX {
            diagrams.remove(0);
        }
    }
}

pub fn active_diagram_count() -> usize {
    ACTIVE_DIAGRAMS
        .lock()
        .ok()
        .map(|diagrams| diagrams.len())
        .unwrap_or(0)
}

/// Clear active diagrams (call at start of render cycle)
pub fn clear_active_diagrams() {
    if let Ok(mut diagrams) = ACTIVE_DIAGRAMS.lock() {
        diagrams.clear();
    }
    clear_streaming_preview_diagram();
}

#[cfg(test)]
use std::sync::MutexGuard;

#[cfg(test)]
static ACTIVE_DIAGRAM_TEST_LOCK: std::sync::OnceLock<Mutex<()>> = std::sync::OnceLock::new();

#[cfg(test)]
thread_local! {
    /// How many [`active_diagram_test_lock`] guards this thread currently
    /// holds. See the function for why the lock has to be reentrant.
    static ACTIVE_DIAGRAM_TEST_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Reentrant test lock guarding the process-global active-diagram registry.
///
/// The registry is a `static`, so a test that registers diagrams while a
/// sibling test clears or restores them observes an empty or reordered list.
/// `test_diagram_focus_toggle_and_pan` failed exactly that way: a sibling
/// test's `clear_active_diagrams()` landed between its `register_active_diagram`
/// calls and the keypress that reads the count back. Every test that touches
/// the registry directly must hold this.
///
/// Reentrant on purpose: tests bracket their body with a cleanup clear, and
/// holding the guard at both ends must not self-deadlock on the non-reentrant
/// mutex. Only the outermost acquisition takes the lock; nested ones just bump
/// the depth.
#[cfg(test)]
pub fn active_diagram_test_lock() -> ActiveDiagramTestGuard {
    let outer = if ACTIVE_DIAGRAM_TEST_DEPTH.with(|depth| depth.get()) == 0 {
        Some(
            ACTIVE_DIAGRAM_TEST_LOCK
                .get_or_init(|| Mutex::new(()))
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        )
    } else {
        None
    };
    ACTIVE_DIAGRAM_TEST_DEPTH.with(|depth| depth.set(depth.get() + 1));
    ActiveDiagramTestGuard { _outer: outer }
}

/// Guard for [`active_diagram_test_lock`]; the outermost one releases the lock.
#[cfg(test)]
pub struct ActiveDiagramTestGuard {
    _outer: Option<MutexGuard<'static, ()>>,
}

#[cfg(test)]
impl Drop for ActiveDiagramTestGuard {
    fn drop(&mut self) {
        ACTIVE_DIAGRAM_TEST_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
        // `_outer` is dropped right after, which unlocks for outermost drops.
    }
}
