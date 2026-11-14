//! Exercise the production (non-cfg(test)) style library in fresh processes.
//! Preferences are cached at startup, so changing the parent environment or
//! sharing a process between cases would not test the actual startup contract.
use alphacode::alphacode_tui_style::color::{
    ColorCapability, color_capability, no_color_enabled, reduced_motion_requested, rgb,
};
use ratatui::style::Color;
use std::process::Command;

#[test]
fn production_render_preferences_respect_accessibility_and_terminal_capabilities() {
    let cases = [
        (None, None, None, "truecolor", false, false, true),
        (Some("1"), None, None, "truecolor", true, true, true),
        // NO_COLOR uses presence of a nonempty value, not boolean parsing.
        (Some("0"), None, None, "truecolor", true, true, true),
        (Some(""), None, None, "truecolor", false, false, true),
        (None, Some(" YES "), None, "truecolor", true, true, true),
        (None, Some("off"), None, "truecolor", false, false, true),
        (None, None, Some("On"), "truecolor", false, true, true),
        (None, None, Some("false"), "truecolor", false, false, true),
        (Some("1"), None, Some("off"), "truecolor", true, true, true),
        (None, None, None, "", false, false, false),
    ];
    for (index, (no_color, app_no_color, reduced, colorterm, mono, motion, truecolor)) in
        cases.into_iter().enumerate()
    {
        let mut child = Command::new(std::env::current_exe().unwrap());
        child.args(["--ignored", "--exact", "render_preferences_child"]);
        for key in [
            "NO_COLOR",
            "ALPHACODE_NO_COLOR",
            "ALPHACODE_REDUCED_MOTION",
            "TERM_PROGRAM",
            "GHOSTTY_RESOURCES_DIR",
            "GHOSTTY_BIN_DIR",
            "WEZTERM_EXECUTABLE",
            "WEZTERM_PANE",
        ] {
            child.env_remove(key);
        }
        for (key, value) in [
            ("NO_COLOR", no_color),
            ("ALPHACODE_NO_COLOR", app_no_color),
            ("ALPHACODE_REDUCED_MOTION", reduced),
        ] {
            if let Some(value) = value {
                child.env(key, value);
            }
        }
        let output = child
            .env("COLORTERM", colorterm)
            .env("TERM", "xterm-256color")
            .env("ALPHACODE_GLYPH_SAFE_MODE", "off")
            .env(
                "ALPHACODE_RENDER_PROBE",
                format!("{mono},{motion},{truecolor}"),
            )
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "case {index}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
#[ignore = "subprocess helper invoked with explicit preferences by the parent test"]
fn render_preferences_child() {
    let expected = std::env::var("ALPHACODE_RENDER_PROBE").expect("run through parent test");
    let expected: Vec<_> = expected
        .split(',')
        .map(|v| v.parse::<bool>().unwrap())
        .collect();
    assert_eq!(no_color_enabled(), expected[0]);
    assert_eq!(reduced_motion_requested(), expected[1]);
    assert_eq!(
        color_capability() == ColorCapability::TrueColor,
        expected[2]
    );
    let color = rgb(27, 120, 230);
    if expected[0] {
        assert_eq!(color, Color::Reset);
    } else if expected[2] {
        assert_eq!(color, Color::Rgb(27, 120, 230));
    } else {
        assert!(matches!(color, Color::Indexed(_)));
    }
}
