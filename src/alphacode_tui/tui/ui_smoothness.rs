//! Global anchor-stability (screen smoothness) recorder for the TUI.
//!
//! Feeds one [`AnchorFrame`] per rendered frame from the messages area of the
//! terminal buffer into an [`AnchorStabilityRecorder`], which classifies
//! jarring motion: content repositioning away from its transcript anchor,
//! insertions that push content down, large single-frame pops, blinks, and
//! mass reflows. Expected motion (user scrolls, resizes, uniform tail-follow
//! scrolling) is excluded by the recorder itself.
//!
//! Always on: the per-frame cost is one row-hash pass over the visible
//! messages area (tens of microseconds). Reports are served via the client
//! debug commands `smoothness` and `smoothness:reset`, so live sessions and
//! offscreen replays (`alphacode replay`) can both be benchmarked.

use crate::alphacode_tui_core::anchor_stability::{
    AnchorFrame, AnchorStabilityRecorder, BLANK_ROW_HASH,
};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::sync::{Mutex, OnceLock};

fn recorder() -> &'static Mutex<AnchorStabilityRecorder> {
    static RECORDER: OnceLock<Mutex<AnchorStabilityRecorder>> = OnceLock::new();
    RECORDER.get_or_init(|| Mutex::new(AnchorStabilityRecorder::new()))
}

/// Hash one buffer row's cell symbols within `area`. Returns
/// [`BLANK_ROW_HASH`] for visually blank rows.
///
/// Optimized to minimize per-cell overhead: uses a fast inline hasher
/// and early-exits on the first non-blank cell to avoid redundant
/// blank checks.
fn hash_row(buffer: &Buffer, area: Rect, y: u16) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut has_content = false;
    let left = area.left();
    let right = area.right();
    for x in left..right {
        if let Some(cell) = buffer.cell((x, y)) {
            let symbol = cell.symbol();
            // Fast path: skip blank checks once we've found any content
            if !has_content {
                has_content = !symbol.trim().is_empty();
            }
            symbol.hash(&mut hasher);
        }
    }
    if has_content {
        hasher.finish()
    } else {
        BLANK_ROW_HASH
    }
}

/// Build an [`AnchorFrame`] from a rendered buffer region. Shared by the live
/// recorder hook and offscreen benchmark tests (which feed frames into their
/// own local [`AnchorStabilityRecorder`] for deterministic assertions).
pub(crate) fn frame_from_buffer(
    buffer: &Buffer,
    messages_area: Rect,
    scroll_offset: usize,
    following_tail: bool,
) -> Option<AnchorFrame> {
    if messages_area.width == 0 || messages_area.height == 0 {
        return None;
    }
    let area = messages_area.intersection(*buffer.area());
    if area.width == 0 || area.height == 0 {
        return None;
    }
    let rows: Vec<u64> = (area.top()..area.bottom())
        .map(|y| hash_row(buffer, area, y))
        .collect();
    Some(AnchorFrame {
        rows,
        width: area.width,
        scroll_offset,
        following_tail,
        at: std::time::Instant::now(),
    })
}

/// Observe the rendered messages area for this frame. Called once per draw.
pub(super) fn observe_frame(
    buffer: &Buffer,
    messages_area: Rect,
    scroll_offset: usize,
    following_tail: bool,
) {
    let Some(frame) = frame_from_buffer(buffer, messages_area, scroll_offset, following_tail)
    else {
        return;
    };
    let mut rec = recorder()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    rec.observe(frame);
}

/// Pretty JSON anchor-stability report for the debug socket.
pub(crate) fn report_json() -> String {
    let rec = recorder()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    serde_json::to_string_pretty(&rec.report()).unwrap_or_else(|_| "{}".to_string())
}

/// Reset the recorder (e.g. to measure one turn or one replay in isolation).
pub(crate) fn reset() {
    let mut rec = recorder()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *rec = AnchorStabilityRecorder::new();
}
