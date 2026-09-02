pub mod color;
pub mod harmony;
pub mod icons;
pub mod palette;
pub mod presets;
pub mod role;
pub mod theme;
pub mod theme_mode;
pub mod tokens;

pub use color::{ColorCapability, clear_buf, color_capability, has_truecolor, indexed_to_rgb, rgb};
pub use harmony::{Criterion, HarmonyReport, Oklab, analyze as analyze_harmony, analyze_active};
pub use icons::{Icon, TerminalClass, force_terminal_class, terminal_class};
pub use palette::{ALL_ROLES, Palette, Role, palette, role_color, set_palette};
pub use presets::{PRESETS, ThemeSeed, preset_by_id, preset_palette};
pub use role::{
    is_overridden as is_role_overridden, themed as role_themed, themed_rgb as role_themed_rgb,
};
pub use theme_mode::{
    ThemeMode, adapt_buffer, adapt_buffer_for_theme, adapt_color_for_theme, is_light_theme,
    set_theme_mode, theme_mode,
};
pub use tokens::{Frame, Glyph, Indent, Radius, ResolvedFrame, Spacing, tokens_for};

/// Restore the terminal, logging any failure instead of printing it.
///
/// `ratatui::restore()` reports failures with `eprintln!`. On a dead terminal
/// (closed window, dropped SSH) the restore fails with EIO and stderr is
/// equally dead, so that `eprintln!` itself panics inside
/// `std::io::stdio::print_to`. The panic hook can then record a live session as
/// crashed and clobber its snapshot.
///
/// Always prefer this over `ratatui::restore()` on cleanup and orphan-exit
/// paths. See issue #599 (and #129, the same class via another path).
pub fn restore_terminal_quietly() {
    if let Err(error) = ratatui::try_restore() {
        crate::alphacode_logging::warn(&format!("failed to restore terminal: {error}"));
    }
}
