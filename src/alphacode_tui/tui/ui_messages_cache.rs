use super::*;

pub(super) use crate::alphacode_tui_messages::{
    centered_wrap_width, left_pad_lines_for_centered_mode,
};

pub(crate) fn get_cached_message_lines<F>(
    msg: &DisplayMessage,
    width: u16,
    diff_mode: crate::config::DiffDisplayMode,
    render: F,
) -> Vec<Line<'static>>
where
    F: FnOnce(&DisplayMessage, u16, crate::config::DiffDisplayMode) -> Vec<Line<'static>>,
{
    crate::alphacode_tui_messages::get_cached_message_lines(
        msg,
        width,
        diff_mode,
        crate::alphacode_tui_messages::MessageCacheContext {
            diagram_mode: crate::config::config().display.diagram_mode,
            centered: markdown::center_code_blocks(),
            mermaid_epoch: crate::alphacode_tui::tui::mermaid::deferred_render_epoch(),
            mermaid_aspect_bucket:
                crate::alphacode_tui::tui::mermaid::current_preferred_aspect_ratio_bucket(),
            show_alphagrep_output: crate::config::config().display.show_alphagrep_output,
            tool_call_details: crate::config::config().display.tool_call_details,
        },
        render,
    )
}

/// Zero-copy variant of [`get_cached_message_lines`].
///
/// Returns the shared `Arc<Vec<Line>>` so a per-frame cache *hit* does not
/// deep-clone every `Span`/`String` in the rendered message. The previous
/// code cloned the whole rendered message on every frame, for every message
/// on screen — the single most expensive per-frame allocation in the
/// transcript path (audit M13b).
///
/// Callers that only read the lines should prefer this and iterate the
/// `Arc` directly. Callers that mutate the lines still need an owned copy.
pub(crate) fn get_cached_message_lines_shared<F>(
    msg: &DisplayMessage,
    width: u16,
    diff_mode: crate::config::DiffDisplayMode,
    render: F,
) -> std::sync::Arc<Vec<Line<'static>>>
where
    F: FnOnce(&DisplayMessage, u16, crate::config::DiffDisplayMode) -> Vec<Line<'static>>,
{
    crate::alphacode_tui_messages::get_cached_message_lines_shared(
        msg,
        width,
        diff_mode,
        crate::alphacode_tui_messages::MessageCacheContext {
            diagram_mode: crate::config::config().display.diagram_mode,
            centered: markdown::center_code_blocks(),
            mermaid_epoch: crate::alphacode_tui::tui::mermaid::deferred_render_epoch(),
            mermaid_aspect_bucket:
                crate::alphacode_tui::tui::mermaid::current_preferred_aspect_ratio_bucket(),
            show_alphagrep_output: crate::config::config().display.show_alphagrep_output,
            tool_call_details: crate::config::config().display.tool_call_details,
        },
        render,
    )
}
