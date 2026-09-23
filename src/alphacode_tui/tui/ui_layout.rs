use ratatui::prelude::*;

pub(crate) use crate::alphacode_tui_render::chrome::{
    MAX_CENTERED_CONTENT_WIDTH, align_if_unset, centered_content_block_width,
    left_aligned_content_inset, left_pad_lines_to_block_width,
};
pub(super) use crate::alphacode_tui_render::chrome::{clear_area, draw_right_rail_chrome};

pub(super) fn right_rail_border_style(focused: bool, focus_color: Color) -> Style {
    crate::alphacode_tui_render::chrome::right_rail_border_style(
        focused,
        focus_color,
        super::theme_support::dim_color(),
    )
}
