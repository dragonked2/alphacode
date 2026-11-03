pub(crate) use crate::alphacode_tui_style::color::*;

/// Resolve the historical UI literals through the same semantic roles used by
/// new widgets. Exact aliases preserve status meaning and bring old screens
/// into the shared theme without recoloring arbitrary image/gradient values.
pub(crate) fn rgb(r: u8, g: u8, b: u8) -> ratatui::style::Color {
    use crate::alphacode_tui_style::palette::{Role, role_color};
    let role = match (r, g, b) {
        (125, 195, 255)
        | (150, 195, 255)
        | (138, 180, 248)
        | (148, 188, 255)
        | (140, 180, 255)
        | (148, 215, 255) => Role::Info,
        (138, 235, 188)
        | (122, 226, 194)
        | (205, 168, 255)
        | (188, 208, 248)
        | (108, 215, 255)
        | (110, 175, 155) => Role::Accent,
        (108, 230, 158) | (100, 200, 100) | (120, 180, 120) => Role::Success,
        (255, 118, 118) | (255, 110, 110) | (255, 100, 100) | (220, 100, 100) | (255, 80, 80) => {
            Role::Error
        }
        (255, 195, 88) | (255, 200, 100) | (255, 215, 78) | (255, 170, 80) => Role::Warning,
        (48, 54, 76) | (60, 68, 90) | (52, 63, 76) => Role::Border,
        (75, 82, 100) | (78, 85, 108) => Role::PanelBorderMuted,
        (138, 142, 165) | (130, 140, 160) | (140, 140, 150) | (100, 100, 110) => Role::MutedText,
        (18, 22, 38) => Role::UserBg,
        (24, 28, 46) | (20, 24, 32) => Role::CodeBg,
        (32, 38, 62) => Role::SelectionBg,
        (245, 248, 255) | (235, 238, 248) => Role::AiText,
        (168, 178, 208) | (180, 180, 190) | (200, 200, 210) => Role::Tool,
        _ => return crate::alphacode_tui_style::color::rgb(r, g, b),
    };
    role_color(role)
}
