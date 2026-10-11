/// Every frame buffer that gets the light-theme pass must also get the palette
/// pass.
///
/// This is a source-level guard because the bug it prevents is invisible to
/// ordinary tests: a render loop that calls `adapt_buffer_for_theme` but not
/// `adapt_buffer_for_palette` compiles cleanly, type-checks, renders perfectly
/// with the default palette, and passes every assertion. It only misbehaves
/// once a user sets `[display.colors]` in their config — and then only on that
/// one surface. `ui::draw` applies both passes, so the omission hid inside the
/// standalone loops that bypass it (the session picker and the swarm replay),
/// which made configured colors apply everywhere *except* the two places a
/// user is most likely to be looking at them.
///
/// The ordering is not negotiable either: the palette pass must run *after* the
/// theme flip so a color the user chose is not luminance-flipped into something
/// unreadable. So this asserts both presence and order.
///
/// Verified by reintroducing the original bug: dropping either call fails with
/// a message naming the file and the pass that went missing.
#[test]
fn every_theme_adapting_render_loop_also_adapts_the_palette() {
    // Every source file that adapts a frame for the light theme. If a new
    // standalone render loop is added, add it here as part of that change.
    let sources: &[(&str, &str)] = &[
        ("tui/ui.rs", include_str!("../ui.rs")),
        (
            "tui/session_picker.rs",
            include_str!("../session_picker.rs"),
        ),
        ("tui/app/replay.rs", include_str!("../app/replay.rs")),
    ];

    for (name, source) in sources {
        let theme = source.matches("adapt_buffer_for_theme(").count();
        assert!(
            theme > 0,
            "expected {name} to adapt at least one frame for the theme; if this \
             file no longer renders frames, move it out of this list rather than \
             deleting the guard"
        );

        let palette = source.matches("adapt_buffer_for_palette(").count();
        assert_eq!(
            palette, theme,
            "{name} adapts {theme} frame buffer(s) for the theme but only \
             {palette} for the palette. Every themed frame must also run \
             `adapt_buffer_for_palette`, or the user's `[display.colors]` \
             overrides silently do nothing on this surface."
        );

        let theme_at = source
            .find("adapt_buffer_for_theme(")
            .expect("checked above");
        let palette_at = source
            .find("adapt_buffer_for_palette(")
            .expect("checked above");
        assert!(
            theme_at < palette_at,
            "{name} runs `adapt_buffer_for_palette` before \
             `adapt_buffer_for_theme`. The palette pass must come second: the \
             theme flip exists to make alphacode's built-in dark palette usable \
             on light terminals, and letting it touch a user-configured color \
             turns a deliberately dark red into an unreadable pale one."
        );
    }
}
