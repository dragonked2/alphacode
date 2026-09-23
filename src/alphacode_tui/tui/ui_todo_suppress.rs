//! Tracks whether a todo card was rendered in the chat viewport on the current
//! frame. The side-panel Todos widget checks this to avoid showing duplicate
//! todo data when the chat already displays a full todo card inline.
//!
//! The flag is reset every frame and set by the message renderer when it
//! encounters a `role == "todos"` message that overlaps the visible viewport.

use std::sync::atomic::{AtomicBool, Ordering};

/// Whether a todo card was rendered in the chat viewport this frame.
static TODO_CARD_VISIBLE: AtomicBool = AtomicBool::new(false);

/// Call at the start of each render frame to reset the flag.
pub(crate) fn reset_frame() {
    TODO_CARD_VISIBLE.store(false, Ordering::Relaxed);
}

/// Call from the message renderer when a todo card is visible in the viewport.
pub(crate) fn note_todo_card_visible() {
    TODO_CARD_VISIBLE.store(true, Ordering::Relaxed);
}

/// Returns true if a todo card was rendered in the chat viewport this frame.
/// The side-panel Todos widget uses this to suppress itself and avoid
/// showing the same todo data twice.
pub(crate) fn is_todo_card_visible() -> bool {
    TODO_CARD_VISIBLE.load(Ordering::Relaxed)
}
