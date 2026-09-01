//! Re-exports of the shared output-style helpers (emoji suppression and
//! color support, see #526).
//!
//! Items are re-exported explicitly rather than with a glob so the crate
//! boundary stays visible in the API surface.

pub use crate::alphacode_core::output_style::{
    compact_header, colorized, emoji_enabled, replace_emoji_with_ascii, set_emoji_enabled,
    status_failure, status_loading, status_success, status_warning, terminal_text,
    terminal_text_with_emoji,
};
