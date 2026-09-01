//! Coverage for the built-in theme presets.
//!
//! Presets are pure data, so the interesting failures are all transcription
//! mistakes: a role left unassigned, two themes sharing an id, a foreground
//! typed into a background slot. Those are exactly the errors that survive code
//! review and only show up as an unreadable terminal, so they are pinned here
//! as properties over the whole catalog rather than spot checks on one theme.

use super::{PRESETS, ThemeSeed, preset_by_id, preset_palette};
use crate::alphacode_tui_style::palette::{ALL_ROLES, Palette, Role};

/// Relative luminance per WCAG 2.x, used to sanity-check that text and its
/// surface are actually distinguishable.
fn luminance((r, g, b): (u8, u8, u8)) -> f64 {
    fn channel(value: u8) -> f64 {
        let v = f64::from(value) / 255.0;
        if v <= 0.040_45 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b)
}

fn contrast(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let (hi, lo) = {
        let (la, lb) = (luminance(a), luminance(b));
        if la >= lb { (la, lb) } else { (lb, la) }
    };
    (hi + 0.05) / (lo + 0.05)
}

// ---------------------------------------------------------------------------
// Catalog integrity
// ---------------------------------------------------------------------------

#[test]
fn the_catalog_is_not_empty() {
    assert!(!PRESETS.is_empty());
}

#[test]
fn preset_ids_are_unique() {
    for (i, preset) in PRESETS.iter().enumerate() {
        for other in &PRESETS[i + 1..] {
            assert_ne!(
                preset.id, other.id,
                "two presets share the id `{}`, so one is unreachable",
                preset.id
            );
        }
    }
}

/// Ids are config keys, so they must already be in the normalized form the
/// lookup produces — otherwise a preset in the catalog cannot be selected by
/// typing its own id.
#[test]
fn every_preset_is_reachable_by_its_own_id() {
    for preset in PRESETS {
        let found = preset_by_id(preset.id)
            .unwrap_or_else(|| panic!("preset `{}` cannot be looked up", preset.id));
        assert_eq!(found.id, preset.id);
    }
}

#[test]
fn display_names_and_descriptions_are_present() {
    for preset in PRESETS {
        assert!(
            !preset.display_name.trim().is_empty(),
            "`{}` has no display name",
            preset.id
        );
        assert!(
            !preset.description.trim().is_empty(),
            "`{}` has no description",
            preset.id
        );
    }
}

#[test]
fn the_catalog_offers_both_dark_and_light_themes() {
    assert!(PRESETS.iter().any(|p| p.is_dark), "no dark theme");
    assert!(PRESETS.iter().any(|p| !p.is_dark), "no light theme");
}

// ---------------------------------------------------------------------------
// Lookup
// ---------------------------------------------------------------------------

#[test]
fn lookup_tolerates_case_and_separator_differences() {
    for spelling in [
        "tokyo-night",
        "Tokyo-Night",
        "tokyo_night",
        "Tokyo Night",
        "  TOKYO-NIGHT  ",
    ] {
        assert_eq!(
            preset_by_id(spelling).map(|p| p.id),
            Some("tokyo-night"),
            "`{spelling}` should resolve to tokyo-night"
        );
    }
}

#[test]
fn an_unknown_id_resolves_to_nothing() {
    // An empty string is the default config value, so it must not accidentally
    // match the first entry.
    for unknown in ["", "   ", "not-a-theme", "tokyo"] {
        assert!(
            preset_by_id(unknown).is_none(),
            "`{unknown}` should not resolve to a preset"
        );
        assert!(preset_palette(unknown).is_none());
    }
}

// ---------------------------------------------------------------------------
// Role derivation
// ---------------------------------------------------------------------------

/// The whole point of deriving roles from a seed is that a theme can never be
/// partially defined. If a role were missed it would silently keep its built-in
/// default and clash with everything around it.
#[test]
fn every_role_is_assigned_exactly_once() {
    let assignments = ThemeSeed::role_assignments(&super::TOKYO_NIGHT);
    assert_eq!(assignments.len(), ALL_ROLES.len());

    for role in ALL_ROLES {
        let count = assignments.iter().filter(|(r, _)| r == role).count();
        assert_eq!(
            count,
            1,
            "role `{}` is assigned {count} times, expected exactly 1",
            role.key()
        );
    }
}

#[test]
fn a_preset_palette_overrides_every_role() {
    for preset in PRESETS {
        let palette = preset.palette();
        assert!(palette.has_overrides());
        for role in ALL_ROLES {
            assert!(
                palette.is_overridden(*role),
                "`{}` left role `{}` on the built-in default",
                preset.id,
                role.key()
            );
            assert_eq!(
                palette.rgb(*role),
                assigned(preset, *role),
                "`{}` stored the wrong color for `{}`",
                preset.id,
                role.key()
            );
        }
    }
}

fn assigned(preset: &ThemeSeed, role: Role) -> (u8, u8, u8) {
    preset
        .role_assignments()
        .into_iter()
        .find(|(r, _)| *r == role)
        .map(|(_, rgb)| rgb)
        .expect("every role is assigned")
}

/// Background roles must receive a surface tone. Assigning a foreground color
/// to a background slot is the single most damaging transcription error: it
/// produces a panel the same color as its own text.
#[test]
fn background_roles_receive_surface_tones() {
    for preset in PRESETS {
        for role in ALL_ROLES.iter().filter(|r| r.is_background()) {
            let color = assigned(preset, *role);
            assert!(
                color == preset.surface || color == preset.surface_alt,
                "`{}` assigned a non-surface color to background role `{}`",
                preset.id,
                role.key()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Legibility
// ---------------------------------------------------------------------------

/// Floors, not WCAG claims. The three text tones exist precisely because they
/// are *meant* to differ in prominence, so holding the dim tone to body-text
/// contrast would reject every real theme. Each floor is set low enough to
/// accept the published palettes and high enough to catch the error that
/// actually matters — a foreground transcribed into the same range as its
/// background, which lands near 1.0.
const MIN_BODY_CONTRAST: f64 = 3.0;
const MIN_MUTED_CONTRAST: f64 = 2.5;
const MIN_SUBTLE_CONTRAST: f64 = 1.8;

/// Body text has to hold up on both surfaces, because a selected or highlighted
/// row still draws its text in `fg`.
#[test]
fn body_text_is_legible_on_every_surface() {
    for preset in PRESETS {
        for (label, surface) in [
            ("surface", preset.surface),
            ("surface_alt", preset.surface_alt),
        ] {
            let ratio = contrast(preset.fg, surface);
            assert!(
                ratio >= MIN_BODY_CONTRAST,
                "`{}`: body text on {label} has contrast {ratio:.2}, below the \
                 {MIN_BODY_CONTRAST:.1} floor",
                preset.id
            );
        }
    }
}

/// The de-emphasised tones only ever sit on the panel surface, so that is the
/// pairing worth pinning.
#[test]
fn de_emphasised_text_stays_perceptible() {
    for preset in PRESETS {
        for (label, color, floor) in [
            ("fg_muted", preset.fg_muted, MIN_MUTED_CONTRAST),
            ("fg_subtle", preset.fg_subtle, MIN_SUBTLE_CONTRAST),
        ] {
            let ratio = contrast(color, preset.surface);
            assert!(
                ratio >= floor,
                "`{}`: {label} on surface has contrast {ratio:.2}, below the \
                 {floor:.1} floor",
                preset.id
            );
        }
    }
}

/// The tones must actually be ordered by prominence, or the emphasis hierarchy
/// the rest of the TUI relies on is inverted.
#[test]
fn the_three_text_tones_are_ordered_by_prominence() {
    for preset in PRESETS {
        let body = contrast(preset.fg, preset.surface);
        let muted = contrast(preset.fg_muted, preset.surface);
        let subtle = contrast(preset.fg_subtle, preset.surface);
        assert!(
            body > muted && muted > subtle,
            "`{}` has a broken emphasis hierarchy: fg {body:.2}, \
             fg_muted {muted:.2}, fg_subtle {subtle:.2}",
            preset.id
        );
    }
}

/// `is_dark` drives theme-aware decisions elsewhere, so it has to match the
/// palette it describes rather than be an independently-typed claim.
#[test]
fn the_dark_flag_matches_the_actual_surface_brightness() {
    for preset in PRESETS {
        let surface = luminance(preset.surface);
        if preset.is_dark {
            assert!(
                surface < 0.25,
                "`{}` claims to be dark but its surface luminance is {surface:.3}",
                preset.id
            );
        } else {
            assert!(
                surface > 0.5,
                "`{}` claims to be light but its surface luminance is {surface:.3}",
                preset.id
            );
        }
    }
}

/// Roles that carry opposite meanings must never render identically, or the
/// user cannot read a diff or a status line at all.
#[test]
fn opposed_roles_are_visually_distinct() {
    let opposed = [
        (Role::DiffAdd, Role::DiffRemove),
        (Role::Success, Role::Error),
        (Role::TodoDone, Role::TodoPending),
        (Role::Warning, Role::Error),
        (Role::User, Role::Ai),
    ];
    for preset in PRESETS {
        for (a, b) in opposed {
            assert_ne!(
                assigned(preset, a),
                assigned(preset, b),
                "`{}` renders `{}` and `{}` identically",
                preset.id,
                a.key(),
                b.key()
            );
        }
    }
}

/// Text drawn on a surface must not be the surface color. This catches a seed
/// where a foreground and a surface were transcribed to the same value.
#[test]
fn foreground_roles_are_never_their_own_background() {
    for preset in PRESETS {
        for role in ALL_ROLES.iter().filter(|r| !r.is_background()) {
            let color = assigned(preset, *role);
            assert_ne!(
                color,
                preset.surface,
                "`{}` renders `{}` invisibly against the panel surface",
                preset.id,
                role.key()
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Composition with `[display.colors]`
// ---------------------------------------------------------------------------

/// The reason `with_pairs` exists: a user who picks a preset *and* overrides one
/// role must keep the preset for the other 38. The old `from_pairs` path always
/// restarted from the built-in defaults, which would have silently discarded the
/// preset for every role the user did not restate.
#[test]
fn user_overrides_layer_on_top_of_a_preset_without_discarding_it() {
    let preset = super::TOKYO_NIGHT;
    let (palette, errors) = preset.palette().with_pairs([("error", "#ff0000")]);

    assert!(errors.is_empty(), "unexpected errors: {errors:?}");
    assert_eq!(
        palette.rgb(Role::Error),
        (255, 0, 0),
        "the explicit override should win"
    );

    for role in ALL_ROLES.iter().filter(|r| **r != Role::Error) {
        assert_eq!(
            palette.rgb(*role),
            assigned(&preset, *role),
            "role `{}` lost its preset color",
            role.key()
        );
    }
}

/// A bad entry must cost only that entry — not the preset, and not the other
/// overrides in the same table.
#[test]
fn a_malformed_override_does_not_disturb_the_preset() {
    let preset = super::NORD;
    let (palette, errors) = preset.palette().with_pairs([
        ("success", "not-a-color"),
        ("nonsense_role", "#112233"),
        ("warning", "#abcdef"),
    ]);

    assert_eq!(errors.len(), 2, "expected two rejections: {errors:?}");
    assert_eq!(palette.rgb(Role::Warning), (171, 205, 239));
    assert_eq!(
        palette.rgb(Role::Success),
        assigned(&preset, Role::Success),
        "a malformed value should leave the preset color in place"
    );
}

/// `from_pairs` is still used by callers with no preset, so its behaviour must
/// be unchanged by the refactor: default base, overrides applied.
#[test]
fn from_pairs_still_starts_from_the_built_in_defaults() {
    let (palette, errors) = Palette::from_pairs([("user", "#010203")]);
    assert!(errors.is_empty());
    assert_eq!(palette.rgb(Role::User), (1, 2, 3));
    assert_eq!(palette.rgb(Role::Ai), Role::Ai.default_rgb());
    assert!(!palette.is_overridden(Role::Ai));
}
