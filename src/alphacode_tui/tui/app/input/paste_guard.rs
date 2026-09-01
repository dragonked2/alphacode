//! Guard against the stray Enter key event some terminals (Windows Terminal /
//! conhost) deliver immediately after a bracketed paste that ends with a
//! newline. Without this, pasting multi-line text submitted the chat (#544).
//!
//! Paste events and key events are both handled on the TUI event-loop thread,
//! so a thread-local timestamp is sufficient and keeps `App` untouched.
//!
//! The suppression window is intentionally generous (500 ms) so that even
//! very large pastes on slow terminals or remote SSH sessions don't leak a
//! trailing Enter into a submit.  We also track whether a paste is *active*
//! (bracketed paste started but not yet finished) and suppress Enter for the
//! entire duration plus the trailing window.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// How long after the *last* bracketed-paste event we keep swallowing bare
/// Enter keystrokes.  500 ms is generous enough for large pastes over slow
/// SSH connections while still feeling instant to the user.
const PASTE_ENTER_SUPPRESS_WINDOW: Duration = Duration::from_millis(500);

/// Extra safety: after we *stop* seeing paste events, keep ignoring Enter
/// for this additional window so late-arriving Enter from conhost is caught.
const PASTE_POST_WINDOW: Duration = Duration::from_millis(200);

thread_local! {
    /// Timestamp of the most recent bracketed-paste event (start or end).
    static LAST_PASTE: Cell<Option<Instant>> = const { Cell::new(None) };
    /// Number of newlines counted in the current paste so we can scale the
    /// suppression window proportionally for very long pastes.
    static PASTE_NEWLINE_COUNT: Cell<usize> = const { Cell::new(0) };
}

/// Global flag: true while a bracketed-paste sequence is in flight.
/// Checked from the key handler to suppress Enter even before the paste
/// event's timestamp has been recorded.
static PASTE_IN_FLIGHT: AtomicBool = AtomicBool::new(false);

/// Called when the terminal starts a bracketed-paste sequence.
#[allow(dead_code)]
pub(super) fn note_paste_start() {
    PASTE_IN_FLIGHT.store(true, Ordering::Relaxed);
    LAST_PASTE.with(|cell| cell.set(Some(Instant::now())));
    PASTE_NEWLINE_COUNT.with(|cell| cell.set(0));
}

/// Record that a bracketed-paste event was just handled (content delivered).
/// The suppression window resets on every paste chunk so that a slow,
/// multi-chunk paste never leaks through.
pub(super) fn note_paste() {
    PASTE_IN_FLIGHT.store(true, Ordering::Relaxed);
    LAST_PASTE.with(|cell| cell.set(Some(Instant::now())));
}

/// Record that a bracketed-paste sequence has finished.
#[allow(dead_code)]
pub(super) fn note_paste_end() {
    PASTE_IN_FLIGHT.store(false, Ordering::Relaxed);
    // Keep LAST_PASTE so the trailing suppression window still fires.
}

/// Increment the newline counter for the current paste.  Used to scale the
/// suppression window for very long pastes.
pub(super) fn count_paste_newlines(newlines: usize) {
    PASTE_NEWLINE_COUNT.with(|cell| cell.set(cell.get() + newlines));
}

/// Returns true when we are currently inside a bracketed-paste sequence.
#[allow(dead_code)]
pub(super) fn is_paste_in_flight() -> bool {
    PASTE_IN_FLIGHT.load(Ordering::Relaxed)
}

/// Compute the effective suppression duration, scaling with paste length.
/// A paste with 0-10 newlines uses the base window; longer pastes scale
/// linearly up to 8x (4 s) so that even very large pastes over slow SSH
/// links never leak a trailing Enter into a submit. Without this, a 5 000
/// line paste split across multiple `paste start/end` events can easily
/// exceed the original 3x cap (1.5 s) and submit itself line by line.
fn effective_suppress_window() -> Duration {
    let newlines = PASTE_NEWLINE_COUNT.with(|cell| cell.get());
    let multiplier = if newlines <= 10 {
        1.0
    } else if newlines <= 100 {
        1.0 + (newlines as f64 - 10.0) / 90.0 // linear 1.0→2.0
    } else if newlines <= 1_000 {
        2.0 + ((newlines as f64 - 100.0) / 900.0) * 2.0 // linear 2.0→4.0
    } else {
        // 1 000+ newlines: cap at 8x base window. At 500 ms base that is
        // 4 s, comfortably wider than any sane chunked paste delivery
        // interval over SSH.
        4.0 + ((newlines as f64 - 1_000.0) / 4_000.0).min(4.0) // 4.0→8.0
    };
    Duration::from_millis((PASTE_ENTER_SUPPRESS_WINDOW.as_millis() as f64 * multiplier) as u64)
}

/// Returns true (and consumes the marker) when a bare Enter arrives within the
/// suppression window after a paste, meaning it belongs to the paste rather
/// than being a user submit.
pub(super) fn consume_paste_trailing_enter() -> bool {
    // If a paste is actively in flight, always suppress Enter.
    if PASTE_IN_FLIGHT.load(Ordering::Relaxed) {
        return true;
    }
    LAST_PASTE.with(|cell| {
        cell.take()
            .is_some_and(|at| at.elapsed() < effective_suppress_window() + PASTE_POST_WINDOW)
    })
}

/// Test hook: age the recorded paste so a subsequent Enter submits normally.
#[cfg(test)]
pub(in crate::alphacode_tui::tui::app) fn expire_for_test() {
    LAST_PASTE.with(|cell| cell.set(None));
    PASTE_IN_FLIGHT.store(false, Ordering::Relaxed);
}

/// Media type for image file extensions accepted by drag-and-drop paste.
pub(super) fn image_media_type(path: &std::path::Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "bmp" => Some("image/bmp"),
        "tif" | "tiff" => Some("image/tiff"),
        _ => None,
    }
}

