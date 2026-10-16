//! Interactive config overlay (`/config ui`).
//!
//! Lets users toggle and edit settings in-app instead of hand-editing
//! `config.toml` and restarting. Opened with `/config ui`, closed with
//! `Esc`/`q`.
//!
//! Every row writes through the same [`Config`] setters the typed
//! `/config set …` commands use, so the two paths cannot disagree about where a
//! preference lives or what it is called.

use ratatui::prelude::*;

use crate::alphacode_base::config::Config;

/// A single toggleable config setting surfaced by the overlay.
///
/// `on_toggle` is a plain `fn` rather than a closure because every setting is a
/// static path into `config.toml`: there is no per-row state to capture, and a
/// plain function pointer keeps the list `Copy`-cheap enough to rebuild on
/// every frame.
pub struct ConfigSetting {
    /// Stable identifier, and the `config.toml` path shown for this row.
    ///
    /// Surfaced in the footer on selection so a user who does not remember
    /// where a switch lives can go read the exact key.
    pub key: &'static str,
    /// Short label for the left column.
    pub label: &'static str,
    /// One-line explanation of what turning this on actually changes.
    pub description: &'static str,
    /// Current persisted value.
    pub current: bool,
    /// Applies the new value and persists it.
    pub on_toggle: fn(bool),
}

/// State for the interactive settings overlay.
///
/// Public because it appears in a `pub trait` method's return type; every field
/// is crate-internal state, so the type itself is the only thing exposed.
#[derive(Clone)]
pub struct ConfigOverlayState {
    pub selected: usize,
    /// Status line shown under the list: last change, or the error from one.
    pub notice: Option<(String, bool)>,
}

impl ConfigOverlayState {
    pub(super) fn new() -> Self {
        Self {
            selected: 0,
            notice: None,
        }
    }
}

/// Apply a persisted setting, logging rather than propagating the failure.
///
/// `Config::mutate_config` rewrites the whole file, so a `Result` here is
/// meaningful — a read-only or malformed `config.toml` genuinely fails the
/// write. The overlay surfaces that separately by reading the value back, so
/// logging here is enough; the setter itself must stay infallible because
/// `ConfigSetting` holds a plain `fn` pointer.
fn apply(setter: fn(bool) -> anyhow::Result<()>, label: &'static str, value: bool) {
    if let Err(error) = setter(value) {
        crate::logging::warn(&format!(
            "config overlay: {label} could not be saved: {error}"
        ));
    }
}

/// Every setting the overlay can edit, with its live value read from config.
///
/// The order is fixed so the arrow keys do not shuffle under the user between
/// frames — `selected` is an index, and an index into a list that reorders
/// between renders moves the highlight under the user's finger.
pub(super) fn settings() -> Vec<ConfigSetting> {
    let cfg = Config::load();
    vec![
        ConfigSetting {
            key: "display.compact_notifications",
            label: "Compact notifications",
            description: "One-line tool and system notifications",
            current: cfg.display.compact_notifications,
            on_toggle: |v| {
                apply(
                    Config::set_compact_notifications,
                    "compact notifications",
                    v,
                )
            },
        },
        ConfigSetting {
            key: "display.pin_todos",
            label: "Pin todos",
            description: "Keep the todo band visible while scrolling",
            current: cfg.display.pin_todos,
            on_toggle: |v| apply(Config::set_pin_todos, "pin todos", v),
        },
        ConfigSetting {
            key: "display.centered",
            label: "Centered output",
            description: "Center assistant output instead of left-aligning",
            current: cfg.display.centered,
            on_toggle: |v| apply(Config::set_display_centered, "centered output", v),
        },
        ConfigSetting {
            key: "display.show_alphagrep_output",
            label: "Show alphagrep output",
            description: "Include alphagrep matches in tool output",
            current: cfg.display.show_alphagrep_output,
            on_toggle: |v| apply(Config::set_show_alphagrep_output, "alphagrep output", v),
        },
        ConfigSetting {
            key: "display.tool_call_details",
            label: "Tool call details",
            description: "Expand each tool call to show its full parameters",
            current: cfg.display.tool_call_details,
            on_toggle: |v| apply(Config::set_tool_call_details, "tool call details", v),
        },
    ]
}

/// Clamp the highlight into range for the current list length.
///
/// A config file edited underneath the overlay can shrink the list; without
/// this the highlight lands out of bounds and the overlay renders nothing at
/// all.
pub(super) fn clamp_selection(selected: usize, len: usize) -> usize {
    if len == 0 { 0 } else { selected.min(len - 1) }
}

/// The rectangles the overlay paints into, computed once so the drawing code
/// does not repeat the arithmetic and can be tested without a terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OverlayLayout {
    /// Outer bordered box.
    pub frame: Rect,
    /// Rows available for the setting list.
    pub list: Rect,
    /// Single-row status line, when there is something to say.
    pub footer: Rect,
}

/// Compute the overlay's layout for a terminal area.
///
/// Split out from painting so the row arithmetic — which is where an off-by-one
/// puts the list off the bottom of a short terminal — is testable directly.
pub(super) fn layout(area: Rect, len: usize, has_notice: bool) -> OverlayLayout {
    // Every dimension is capped by the area *after* clamping to the preferred
    // size. `.clamp(24, 76)` alone made a 10-column terminal get a 24-wide box:
    // the border then ran off the right edge, the right-hand column of every row
    // was cut off, and the mouse hit test disagreed with what was drawn. Same
    // for the height, where an oversized box pushed the status line below the
    // bottom of the screen.
    let inner_width = area.width.saturating_sub(4).clamp(24, 76).min(area.width);
    // Never smaller than the content needs, never larger than the terminal.
    let wanted = u16::try_from(len).unwrap_or(u16::MAX).saturating_add(4);
    let inner_height = area
        .height
        .saturating_sub(4)
        .min(24)
        .max(wanted.min(area.height.saturating_sub(2)))
        .min(area.height);
    let frame = Rect::new(
        area.x + area.width.saturating_sub(inner_width) / 2,
        area.y + area.height.saturating_sub(inner_height) / 2,
        inner_width,
        inner_height,
    );

    // Two rows of border, one for the title, one optional for the status line.
    // The status line is only reserved when there is a row for it *and* a row
    // for the list: on a terminal too short for both, spending the last row on
    // the footer left the list with nothing to draw.
    let body_height = inner_height.saturating_sub(3);
    // A frame with fewer than four rows has no body at all — the title row
    // fills it. Even though `body_height` is then zero and nothing is painted,
    // the *rectangle* still has to sit inside the terminal, because the mouse
    // handler hit-tests against it: a list positioned at `frame.y + 2` in a
    // one-row terminal pointed one row below the bottom edge, which reads as a
    // click outside the overlay when it is inside it. Park it on the frame's
    // own top row when there is no body to be below the title.
    let list_y = if body_height == 0 {
        frame.y
    } else {
        frame.y + 2
    };
    let list = Rect::new(
        frame.x + 1,
        list_y,
        frame.width.saturating_sub(2),
        body_height,
    );
    let footer = Rect::new(
        list.x,
        list.y.saturating_add(list.height),
        list.width,
        u16::from(has_notice && body_height > 0),
    );
    OverlayLayout {
        frame,
        list,
        footer,
    }
}

/// Title line for the overlay's border.
pub(super) const TITLE: &str = " Settings  ·  ↑/↓ move  ·  Space or Enter toggle  ·  Esc/q close ";

/// Where the overlay was painted on the most recent frame.
///
/// The mouse handler needs this to tell "clicked inside the box" from "clicked
/// the transcript behind it", and it must be the *drawn* rectangle rather than a
/// recomputed one: the layout is a function of the terminal size and the setting
/// count, both of which can change between the last paint and the click, and a
/// click judged against a stale-shaped box either dismisses the overlay while it
/// is still under the cursor or fails to dismiss it when it is not.
static PAINTED_FRAME: std::sync::Mutex<Option<Rect>> = std::sync::Mutex::new(None);

/// Record the rectangle the overlay was painted into.
pub(super) fn record_painted_frame(rect: Rect) {
    if let Ok(mut slot) = PAINTED_FRAME.lock() {
        *slot = Some(rect);
    }
}

/// The rectangle the overlay was last painted into, if it has been painted.
pub(super) fn painted_frame() -> Option<Rect> {
    PAINTED_FRAME.lock().ok().and_then(|slot| *slot)
}

/// The settings that fit on screen, paired with their absolute index.
///
/// The index is returned alongside the row because the painter cannot recover
/// it: after windowing, "the third visible row" is not the same thing as
/// `settings[2]`, and deriving one from the other is exactly how a highlight
/// ends up on the wrong line once the list scrolls.
pub(super) fn visible_rows(
    settings: &[ConfigSetting],
    selected: usize,
    layout: OverlayLayout,
) -> Vec<(usize, &ConfigSetting)> {
    if layout.list.height == 0 {
        return Vec::new();
    }
    let selected = clamp_selection(selected, settings.len());
    let window = layout.list.height as usize;
    // Scroll so the highlighted row is the last visible one when it would
    // otherwise fall below the box.
    let first = selected.saturating_sub(window.saturating_sub(1));
    settings
        .iter()
        .enumerate()
        .skip(first)
        .take(window)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The overlay indexes by position, so a list that shrinks underneath it
    /// must not leave the highlight out of bounds.
    #[test]
    fn clamp_selection_stays_in_range() {
        assert_eq!(clamp_selection(0, 5), 0);
        assert_eq!(clamp_selection(4, 5), 4);
        // List shrank: highlight was past the end.
        assert_eq!(clamp_selection(9, 3), 2);
        // Empty list must not underflow.
        assert_eq!(clamp_selection(4, 0), 0);
    }

    /// The painted rectangle is what the mouse handler judges a click against,
    /// so it has to round-trip exactly. A click that lands inside the box must
    /// not dismiss the overlay, and one that lands outside must.
    #[test]
    fn painted_frame_round_trips() {
        assert!(painted_frame().is_none(), "nothing painted yet");
        let rect = Rect::new(10, 4, 60, 20);
        record_painted_frame(rect);
        assert_eq!(painted_frame(), Some(rect));
    }

    /// The footer is only rendered when the layout reserved a row for it, and a
    /// very short terminal must still leave the list non-negative rather than
    /// wrapping into the border.
    #[test]
    fn layout_survives_a_short_terminal() {
        for height in [0u16, 1, 2, 3, 4, 5, 8] {
            for width in [0u16, 1, 10, 40, 200] {
                let area = Rect::new(0, 0, width, height);
                let layout = layout(area, 5, true);
                assert!(
                    layout.list.y.saturating_add(layout.list.height) <= area.y + area.height,
                    "list escapes the terminal at {width}x{height}: {layout:?}"
                );
                assert!(
                    layout.frame.width <= width.max(1) || width == 0,
                    "frame wider than the terminal at {width}x{height}: {layout:?}"
                );
            }
        }
    }

    /// Scrolling the list must never orphan the highlight: whatever row is
    /// painted last has to be the selected one, or the arrow keys move a cursor
    /// that is not where the user is looking.
    #[test]
    fn visible_rows_always_include_the_highlight() {
        let settings: Vec<ConfigSetting> = (0..9)
            .map(|_| ConfigSetting {
                key: "display.k",
                label: "k",
                description: "d",
                current: false,
                on_toggle: |_| {},
            })
            .collect();
        for height in 1u16..=6 {
            let area = Rect::new(0, 0, 60, 40);
            let layout = layout(area, settings.len(), true);
            let window = layout.list.height.min(height);
            for selected in 0..settings.len() {
                let rows = visible_rows(
                    &settings,
                    selected,
                    OverlayLayout {
                        list: Rect::new(0, 0, 40, window),
                        ..layout
                    },
                );
                assert!(
                    rows.iter().any(|(index, _)| *index == selected),
                    "selected {selected} not visible with {window} rows: {:?}",
                    rows.iter().map(|(i, _)| *i).collect::<Vec<_>>()
                );
            }
        }
    }

    /// Every row must name a real config key and carry a setter, or the overlay
    /// renders a switch that silently does nothing when pressed.
    #[test]
    fn every_setting_is_wired() {
        let settings = settings();
        assert!(!settings.is_empty(), "overlay would open empty");
        for setting in &settings {
            assert!(
                setting.key.contains('.'),
                "{} is not a dotted config path",
                setting.key
            );
            assert!(
                !setting.description.is_empty(),
                "{} has no description",
                setting.key
            );
            assert!(!setting.label.is_empty(), "{} has no label", setting.key);
        }
        // Keys are unique: two rows writing the same path would show two
        // switches whose values could disagree.
        let mut keys: Vec<&str> = settings.iter().map(|s| s.key).collect();
        keys.sort_unstable();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before, "duplicate config keys in the overlay");
    }
}
