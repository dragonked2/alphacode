//! # alphacode-render-core
//!
//! Backend-neutral document/render model shared by alphacode's front-ends (the
//! ratatui TUI and the desktop GPU UI).
//!
//! The pipeline is split at the seam where backends actually differ:
//!
//! ```text
//!   text ─▶ parse_markdown ─▶ Document (neutral blocks/spans) ─▶ wrap ─▶ adapter ─▶ backend draw
//! ```
//!
//! Everything up to and including wrapping is shared here. Each front-end owns
//! only a thin adapter: it resolves [`model::StyleRole`] to concrete colors and
//! turns [`model::StyledLine`]s into its own draw primitives (`ratatui::Line`
//! for the TUI, glyph runs for the desktop), and supplies a
//! [`wrap::WidthMeasure`] for its width units.
//!
//! This model is extracted from the *working* TUI markdown renderer
//! (`alphacode-tui-markdown`); that renderer remains authoritative until this core
//! reaches parity, after which front-ends migrate onto it.

pub mod bidi;
pub mod markdown;
pub mod math;
pub mod model;
pub mod preprocess;
pub mod reasoning;
pub mod wrap;

pub use bidi::{
    ArabicShapingMode, contains_rtl, decompose_presentation_form, detect_base_direction,
    detect_dominant_direction, detect_line_base_direction, detect_line_dominant_direction,
    get_shaping_mode, is_arabic_char, is_arabic_transparent, is_native_shaping_active, is_rtl_char,
    line_contains_rtl, normalize_rtl_line, normalize_rtl_text, prepare_rtl_line,
    prepare_rtl_line_presentation, prepare_rtl_line_unicode, prepare_rtl_line_with_base,
    prepare_rtl_lines, prepare_rtl_text, prepare_rtl_text_presentation, prepare_rtl_text_unicode,
    prepare_rtl_text_with_base, reorder_rtl_line_with_base, reorder_rtl_lines_with_base,
    set_shaping_mode_override, set_thread_shaping_mode, shape_arabic_chars, shape_rtl_line,
    shape_rtl_text, terminal_supports_native_shaping, with_arabic_shaping_mode,
};
pub use markdown::parse_markdown;
pub use math::{render_display_latex, render_inline_latex};
pub use model::{
    Alignment, Block, BlockKind, Document, FillRole, StyleRole, StyledLine, StyledSpan, TextAttrs,
};
pub use preprocess::{escape_currency_dollars, normalize_latex_math};
pub use reasoning::{
    REASONING_SENTINEL, reasoning_line_markup, reasoning_partial_markup,
    reasoning_summary_line_markup,
};
pub use wrap::{ColumnWidth, WidthMeasure, wrap_line, wrap_lines};

#[cfg(test)]
mod tests;
