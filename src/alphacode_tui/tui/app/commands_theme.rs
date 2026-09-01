//! `/theme`: browse and apply the built-in color presets.
//!
//! [`crate::alphacode_tui_style::presets`] holds the palettes; this is the front
//! end for them. It exists because the alternative — telling a user to hand-write
//! 39 hex values into `[display.colors]` — is not a theme system anyone will use.
//!
//! Applying a theme writes `display.preset` and reinstalls the live palette, so
//! the change is visible on the next frame without a restart. Per-role
//! `[display.colors]` overrides keep winning over the preset, which is why the
//! two commands are separate: `/theme` picks the base, `/colors` retunes it.

use super::{App, DisplayMessage};
use crate::alphacode_tui_style::presets::{PRESETS, ThemeSeed, preset_by_id};
use serde::Serialize;

/// Title attached to the `/theme` listing message so the message renderer can
/// recognize it and draw truecolor preview swatches instead of plain text.
/// Defined next to the renderer; imported here so the two never drift.
use crate::alphacode_tui::tui::ui::THEME_PREVIEW_TITLE;

/// Row of the serialized theme listing. Kept small and explicit (rather than
/// serializing the whole seed) so the renderer contract stays stable.
#[derive(Serialize)]
struct ThemePreviewRow<'a> {
    id: &'a str,
    dark: bool,
    description: &'a str,
    /// Representative colors for the preview bar, in display order.
    swatch: [(u8, u8, u8); 8],
}

/// Build the swatch bar for a preset: text, surface, accent, then the six
/// semantic hues. Mirrors the order the renderer draws in `ui_messages.rs`.
fn swatch_for(preset: &ThemeSeed) -> [(u8, u8, u8); 8] {
    [
        preset.fg,
        preset.surface,
        preset.accent,
        preset.red,
        preset.yellow,
        preset.green,
        preset.blue,
        preset.magenta,
    ]
}
const USAGE: &str = "Usage:\n  \
    /theme                  List the built-in themes\n  \
    /theme <name>           Apply a theme (saved to config, applied immediately)\n  \
    /theme off              Drop back to alphacode's built-in palette";

/// What the user asked for. Split out from [`handle_theme_command`] so the
/// parsing can be tested without building an [`App`].
#[derive(Debug, Clone, PartialEq, Eq)]
enum Action<'a> {
    List,
    Apply(&'a str),
    Clear,
}

/// Whether this input is the `/theme` command, and what follows it.
///
/// Returns `None` for anything else, including `/themes` and `/themed` — a
/// handler that claimed those would silently shadow a future command.
fn claim(trimmed: &str) -> Option<&str> {
    let rest = trimmed.strip_prefix("/theme")?;
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    Some(rest.trim())
}

fn parse(rest: &str) -> Action<'_> {
    match rest.split_whitespace().next() {
        None | Some("list") => Action::List,
        Some("off") | Some("none") | Some("reset") | Some("default") => Action::Clear,
        Some(name) => Action::Apply(name),
    }
}

pub(super) fn handle_theme_command(app: &mut App, trimmed: &str) -> bool {
    let Some(rest) = claim(trimmed) else {
        return false;
    };
    match parse(rest) {
        Action::List => list_themes(app),
        Action::Clear => clear_theme(app),
        Action::Apply(name) => apply_theme(app, name),
    }
    true
}

/// The preset currently in effect, if any.
fn active() -> Option<&'static ThemeSeed> {
    let configured = &crate::config::config().display.preset;
    preset_by_id(configured)
}

fn list_themes(app: &mut App) {
    // The listing is pushed as a small JSON payload tagged with a title so the
    // message renderer can draw truecolor preview bars for each theme (system
    // message content is otherwise forced to the system color). The renderer
    // falls back to a readable plain-text table for anything it cannot parse.
    let active_id = active().map(|p| p.id);
    let payload = ThemeListPayload {
        active: active_id,
        rows: PRESETS
            .iter()
            .map(|preset| ThemePreviewRow {
                id: preset.id,
                dark: preset.is_dark,
                description: preset.description,
                swatch: swatch_for(preset),
            })
            .collect(),
    };
    let json = serde_json::to_string(&payload).expect("serialize theme listing");
    app.push_display_message(DisplayMessage {
        role: "system".to_string(),
        content: json,
        tool_calls: Vec::new(),
        duration_secs: None,
        title: Some(THEME_PREVIEW_TITLE.to_string()),
        tool_data: None,
    });
}

/// The serialized theme listing shape; see [`ThemePreviewRow`].
#[derive(Serialize)]
struct ThemeListPayload<'a> {
    active: Option<&'a str>,
    rows: Vec<ThemePreviewRow<'a>>,
}

fn apply_theme(app: &mut App, name: &str) {
    let Some(preset) = preset_by_id(name) else {
        let known = PRESETS
            .iter()
            .map(|p| p.id)
            .collect::<Vec<_>>()
            .join(", ");
        app.push_display_message(DisplayMessage::error(format!(
            "Unknown theme '{name}'.\n\nAvailable: {known}\n\n{USAGE}"
        )));
        return;
    };

    match persist(preset.id) {
        Ok(()) => {
            // Overrides outrank the preset, so a user who has been tuning
            // individual roles would otherwise wonder why part of the theme did
            // not take effect.
            let overrides = crate::config::config().display.colors.len();
            let note = if overrides == 0 {
                String::new()
            } else {
                format!(
                    "\n{overrides} per-role override(s) in `[display.colors]` still \
                     take precedence — `/colors reset` clears them."
                )
            };
            app.push_display_message(DisplayMessage::system(format!(
                "Theme set to {} ({}). Applied immediately.{note}",
                preset.display_name,
                if preset.is_dark { "dark" } else { "light" }
            )));
        }
        Err(error) => app.push_display_message(DisplayMessage::error(format!(
            "Failed to save theme '{}': {error}",
            preset.id
        ))),
    }
}

fn clear_theme(app: &mut App) {
    match persist("") {
        Ok(()) => app.push_display_message(DisplayMessage::system(
            "Theme cleared. Back to alphacode's built-in palette.".to_string(),
        )),
        Err(error) => app.push_display_message(DisplayMessage::error(format!(
            "Failed to clear the theme: {error}"
        ))),
    }
}

/// Write `display.preset`, save, and reinstall the live palette.
///
/// Reload-then-patch-then-save (rather than serializing cached state) so a
/// concurrent config edit by another alphacode session is not clobbered. This
/// mirrors `commands_colors::persist` deliberately: the two commands write
/// adjacent keys and must not each invent their own save discipline.
fn persist(preset: &str) -> anyhow::Result<()> {
    let mut config = crate::config::Config::load();
    config.display.preset = preset.to_string();
    config.save()?;
    crate::alphacode_tui::tui::theme_detect::init_palette();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claims_the_bare_command_and_its_arguments() {
        assert_eq!(claim("/theme"), Some(""));
        assert_eq!(claim("/theme "), Some(""));
        assert_eq!(claim("/theme nord"), Some("nord"));
        assert_eq!(claim("/theme  tokyo-night  "), Some("tokyo-night"));
    }

    /// A prefix-greedy handler would shadow any future command starting with
    /// `theme`, and would make `/themes` do something surprising.
    #[test]
    fn does_not_claim_unrelated_commands() {
        for input in ["/themes", "/themed", "/them", "/colors", "theme", "//theme"] {
            assert_eq!(claim(input), None, "`{input}` should not be claimed");
        }
    }

    #[test]
    fn a_bare_command_lists() {
        assert_eq!(parse(""), Action::List);
        assert_eq!(parse("list"), Action::List);
    }

    #[test]
    fn every_spelling_of_off_clears_the_theme() {
        for word in ["off", "none", "reset", "default"] {
            assert_eq!(parse(word), Action::Clear, "`{word}` should clear");
        }
    }

    #[test]
    fn a_name_is_applied() {
        assert_eq!(parse("nord"), Action::Apply("nord"));
        // Extra words are ignored rather than rejected: the first token is the
        // theme, and a stray argument should not cost the user the command.
        assert_eq!(parse("nord extra"), Action::Apply("nord"));
    }

    /// `off` is a reserved word, so a preset may never be named it — that theme
    /// would be unreachable.
    #[test]
    fn no_preset_collides_with_a_reserved_word() {
        for word in ["list", "off", "none", "reset", "default"] {
            assert!(
                preset_by_id(word).is_none(),
                "`{word}` is a reserved subcommand and cannot also be a theme id"
            );
        }
    }

    #[test]
    fn usage_text_documents_every_subcommand() {
        assert!(USAGE.contains("/theme <name>"));
        assert!(USAGE.contains("off"));
    }
}
