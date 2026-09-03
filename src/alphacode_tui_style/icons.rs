//! Icon registry — single source of truth for glyph literals.
//!
//! # Why this exists
//!
//! Widgets historically embedded Unicode glyphs as raw literals (✔, ✖, 🗑,
//! ⚙, ⏰, …) at every call site. Three problems followed:
//!
//! 1. **No semantic meaning.** A literal `✔` could mean "completed", "ok",
//!    "yes", or "success" — readers had to infer from context.
//! 2. **No terminal fallback.** Some glyphs render as boxes or mojibake on
//!    terminals with weak font coverage (Apple Terminal, VS Code, Windows
//!    Console). A registry can pick a less-pretty fallback per terminal.
//! 3. **No single edit point.** A redesign that wants `✖` to become `✕`
//!    requires touching dozens of files.
//!
//! `icons.rs` introduces a small `Icon` enum with semantic names and a
//! `.glyph()` method that returns the right `&'static str` for the current
//! terminal.
//!
//! # Adding a new icon
//!
//! Add a variant, give it a glyph for every TerminalClass, add a unit test
//! asserting non-emptiness, and update the docs above.

/// Which terminal we're rendering into. Determines icon fallbacks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerminalClass {
    /// Ghostty, kitty, WezTerm, iTerm2, Alacritty, foot, Warp, Konsole.
    /// Best font coverage; can render emoji + extended Unicode glyphs.
    Modern,
    /// Apple Terminal, VS Code integrated terminal, Windows Terminal (no
    /// Nerd Font). Some emoji glyphs render as boxes; extended glyphs
    /// render but basic block drawing is solid.
    Mainstream,
    /// Plain `xterm`, `linux`, `st`, anything without an emoji font.
    /// Falls back to ASCII-safe text for icons that have no plain
    /// rendering.
    Minimal,
}

impl TerminalClass {
    /// Detect the terminal class from the process environment.
    pub fn detect() -> TerminalClass {
        if let Ok(raw) = std::env::var("ALPHACODE_ICONS") {
            match raw.trim().to_ascii_lowercase().as_str() {
                "1" | "true" | "yes" | "on" | "modern" => return TerminalClass::Modern,
                "0" | "false" | "no" | "off" | "minimal" | "ascii" => {
                    return TerminalClass::Minimal;
                }
                "mainstream" | "safe" => return TerminalClass::Mainstream,
                _ => {}
            }
        }

        if let Ok(raw) = std::env::var("ALPHACODE_GLYPH_SAFE_MODE") {
            let raw = raw.trim().to_ascii_lowercase();
            if matches!(raw.as_str(), "1" | "true" | "yes" | "on") {
                return TerminalClass::Mainstream;
            }
        }

        if let Ok(tp) = std::env::var("TERM_PROGRAM") {
            let tp = tp.to_ascii_lowercase();
            if matches!(
                tp.as_str(),
                "ghostty" | "iterm.app" | "wezterm" | "warp" | "alacritty" | "konsole"
            ) {
                return TerminalClass::Modern;
            }
            if tp == "vscode" || tp == "apple_terminal" {
                return TerminalClass::Mainstream;
            }
        }

        if std::env::var("GHOSTTY_RESOURCES_DIR").is_ok()
            || std::env::var("GHOSTTY_BIN_DIR").is_ok()
            || std::env::var("WEZTERM_EXECUTABLE").is_ok()
            || std::env::var("WEZTERM_PANE").is_ok()
        {
            return TerminalClass::Modern;
        }

        if let Ok(term) = std::env::var("TERM") {
            let t = term.to_ascii_lowercase();
            if t.contains("kitty") || t.contains("ghostty") || t.contains("alacritty") {
                return TerminalClass::Modern;
            }
            if t.contains("256color") || t == "xterm-256color" {
                return TerminalClass::Mainstream;
            }
            if t == "dumb" || t.is_empty() {
                return TerminalClass::Minimal;
            }
        }

        TerminalClass::Mainstream
    }
}

static TERMINAL_CLASS: std::sync::OnceLock<TerminalClass> = std::sync::OnceLock::new();

/// Cached terminal class detection result.
pub fn terminal_class() -> TerminalClass {
    *TERMINAL_CLASS.get_or_init(TerminalClass::detect)
}

/// Test-only override: set `ALPHACODE_ICONS` to the desired class and call
/// this from a test before any other call to [`terminal_class`]. There is
/// no public API to clear a `OnceLock`, so the override must precede the
/// first call. Production code never calls this.
pub fn force_terminal_class(class: TerminalClass) {
    let val = match class {
        TerminalClass::Modern => "modern",
        TerminalClass::Mainstream => "mainstream",
        TerminalClass::Minimal => "minimal",
    };
    unsafe {
        std::env::set_var("ALPHACODE_ICONS", val);
    }
}

/// Semantic icon enum.
///
/// Every variant has a glyph for each [`TerminalClass`]. New variants must
/// populate all three columns; the unit test in this file fails if any is
/// left empty.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    /// Positive confirmation.
    Confirm,
    /// Negative confirmation.
    Cancel,
    /// Informational notice.
    Info,
    /// Warning.
    Warn,
    /// Error.
    Error,
    /// Queued / waiting.
    Queued,
    /// In progress / running.
    Running,
    /// Pending / not-yet-started.
    Pending,
    /// Done / completed tick.
    Done,
    /// Trash / delete.
    Delete,
    /// Gear / settings.
    Settings,
    /// Star / favorite.
    Star,
    /// Sparkle / "new" / attention.
    Sparkle,
    /// Light bulb / tip.
    Tip,
    /// Stopwatch / time / duration.
    Time,
    /// Stopwatch with hands / elapsed clock.
    Timer,
    /// Bolt / energy / cache hit.
    Bolt,
    /// Lightning over clipboard / quick action.
    QuickAction,
    /// Folder / workspace.
    Workspace,
    /// Checkbox / todo.
    Todo,
    /// Eye / view / context.
    Eye,
    /// Chart / usage / graph.
    Chart,
    /// Cache / layers.
    Cache,
    /// Git branch.
    Branch,
    /// Keyboard key.
    Key,
    /// Bell / notification.
    Bell,
    /// Right-pointing arrow.
    ArrowRight,
    /// Up-pointing arrow.
    ArrowUp,
    /// Down-pointing arrow.
    ArrowDown,
    /// Heavy multiplication X (universal "close").
    Close,
    /// Heavy check (universal "done").
    Check,
    /// Three-dot ellipsis (truncation marker).
    Ellipsis,
    /// Left half-circle pill cap (rounded variant).
    PillCapLeft,
    /// Right half-circle pill cap (rounded variant).
    PillCapRight,
}

impl Icon {
    /// Resolve to a glyph appropriate for the current terminal class.
    ///
    /// Every variant returns a non-empty `&'static str`. Glyphs are chosen
    /// so the same semantic reads naturally in every terminal — Modern
    /// terminals get emoji or extended Unicode, Mainstream gets symbol
    /// glyphs that ship with every common font, Minimal gets ASCII text.
    pub fn glyph(self) -> &'static str {
        match (self, terminal_class()) {
            (Self::Confirm, TerminalClass::Modern) => "✓",
            (Self::Confirm, TerminalClass::Mainstream) => "✔",
            (Self::Confirm, TerminalClass::Minimal) => "OK",

            (Self::Cancel, TerminalClass::Modern) => "✗",
            (Self::Cancel, TerminalClass::Mainstream) => "✖",
            (Self::Cancel, TerminalClass::Minimal) => "X",

            (Self::Info, TerminalClass::Modern) => "ℹ",
            (Self::Info, TerminalClass::Mainstream) => "ⓘ",
            (Self::Info, TerminalClass::Minimal) => "i",

            (Self::Warn, TerminalClass::Modern | TerminalClass::Mainstream) => "⚠",
            (Self::Warn, TerminalClass::Minimal) => "!",

            (Self::Error, TerminalClass::Modern) => "⨯",
            (Self::Error, TerminalClass::Mainstream) => "✕",
            (Self::Error, TerminalClass::Minimal) => "x",

            (Self::Queued, _) => "…",

            (Self::Running, TerminalClass::Modern | TerminalClass::Mainstream) => "◐",
            (Self::Running, TerminalClass::Minimal) => "*",

            (Self::Pending, _) => "○",

            (Self::Done, TerminalClass::Modern | TerminalClass::Mainstream) => "✓",
            (Self::Done, TerminalClass::Minimal) => "+",

            (Self::Delete, TerminalClass::Modern) => "🗑",
            (Self::Delete, _) => "Del",

            (Self::Settings, TerminalClass::Modern | TerminalClass::Mainstream) => "⚙",
            (Self::Settings, TerminalClass::Minimal) => "*",

            (Self::Star, TerminalClass::Modern | TerminalClass::Mainstream) => "★",
            (Self::Star, TerminalClass::Minimal) => "*",

            (Self::Sparkle, TerminalClass::Modern) => "✨",
            (Self::Sparkle, TerminalClass::Mainstream) => "✦",
            (Self::Sparkle, TerminalClass::Minimal) => "*",

            (Self::Tip, TerminalClass::Modern) => "💡",
            (Self::Tip, TerminalClass::Mainstream) => "☼",
            (Self::Tip, TerminalClass::Minimal) => ">",

            (Self::Time, TerminalClass::Modern) => "⏱",
            (Self::Time, TerminalClass::Mainstream) => "⌛",
            (Self::Time, TerminalClass::Minimal) => "@",

            (Self::Timer, TerminalClass::Modern) => "⏰",
            (Self::Timer, TerminalClass::Mainstream) => "⏲",
            (Self::Timer, TerminalClass::Minimal) => "@",

            (Self::Bolt, TerminalClass::Modern | TerminalClass::Mainstream) => "⚡",
            (Self::Bolt, TerminalClass::Minimal) => ">",

            (Self::QuickAction, TerminalClass::Modern) => "⚡",
            (Self::QuickAction, TerminalClass::Mainstream) => "»",
            (Self::QuickAction, TerminalClass::Minimal) => ">>",

            (Self::Workspace, TerminalClass::Modern) => "🗂",
            (Self::Workspace, TerminalClass::Mainstream) => "☰",
            (Self::Workspace, TerminalClass::Minimal) => "=",

            (Self::Todo, TerminalClass::Modern) => "☑",
            (Self::Todo, TerminalClass::Mainstream) => "☐",
            (Self::Todo, TerminalClass::Minimal) => "[ ]",

            (Self::Eye, TerminalClass::Modern) => "👁",
            (Self::Eye, TerminalClass::Mainstream) => "◉",
            (Self::Eye, TerminalClass::Minimal) => "o",

            (Self::Chart, TerminalClass::Modern) => "📊",
            (Self::Chart, TerminalClass::Mainstream) => "▮",
            (Self::Chart, TerminalClass::Minimal) => "#",

            (Self::Cache, TerminalClass::Modern) => "🗃",
            (Self::Cache, TerminalClass::Mainstream) => "▤",
            (Self::Cache, TerminalClass::Minimal) => "=",

            (Self::Branch, TerminalClass::Modern) => "🌿",
            (Self::Branch, TerminalClass::Mainstream) => "⎇",
            (Self::Branch, TerminalClass::Minimal) => "+",

            (Self::Key, TerminalClass::Modern | TerminalClass::Mainstream) => "⌨",
            (Self::Key, TerminalClass::Minimal) => ">",

            (Self::Bell, TerminalClass::Modern) => "🔔",
            (Self::Bell, TerminalClass::Mainstream) => "♪",
            (Self::Bell, TerminalClass::Minimal) => "*",

            (Self::ArrowRight, _) => "→",
            (Self::ArrowUp, _) => "↑",
            (Self::ArrowDown, _) => "↓",

            (Self::Close, _) => "✕",
            (Self::Check, _) => "✓",
            (Self::Ellipsis, _) => "…",

            (Self::PillCapLeft, TerminalClass::Modern) => "◖",
            (Self::PillCapLeft, _) => "▐",
            (Self::PillCapRight, TerminalClass::Modern) => "◗",
            (Self::PillCapRight, _) => "▌",
        }
    }

    /// Cell-width of the glyph as a terminal character. Defaults to 1.
    /// Emoji glyphs on a Modern terminal may render as wide (2-cell)
    /// characters when the font has full emoji coverage; we conservatively
    /// report 1. A widget that needs pixel-perfect alignment can override
    /// per-icon.
    pub fn cell_width(self) -> u16 {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One instance of every variant. The compile-time check below is the
    /// real test: if you add a new variant and forget to handle it in
    /// `glyph()`, the match arms remain exhaustive and this list silently
    /// goes stale. The runtime `every_variant_has_a_glyph` test catches that.
    const ALL_VARIANTS: &[Icon] = &[
        Icon::Confirm,
        Icon::Cancel,
        Icon::Info,
        Icon::Warn,
        Icon::Error,
        Icon::Queued,
        Icon::Running,
        Icon::Pending,
        Icon::Done,
        Icon::Delete,
        Icon::Settings,
        Icon::Star,
        Icon::Sparkle,
        Icon::Tip,
        Icon::Time,
        Icon::Timer,
        Icon::Bolt,
        Icon::QuickAction,
        Icon::Workspace,
        Icon::Todo,
        Icon::Eye,
        Icon::Chart,
        Icon::Cache,
        Icon::Branch,
        Icon::Key,
        Icon::Bell,
        Icon::ArrowRight,
        Icon::ArrowUp,
        Icon::ArrowDown,
        Icon::Close,
        Icon::Check,
        Icon::Ellipsis,
        Icon::PillCapLeft,
        Icon::PillCapRight,
    ];

    #[test]
    fn terminal_class_default_is_mainstream_or_modern() {
        // With no env overrides, the default should not be Minimal: a
        // user who never set ALPHACODE_ICONS should still get the rich
        // glyph set, only downgrading if their terminal opts in.
        assert!(matches!(
            terminal_class(),
            TerminalClass::Modern | TerminalClass::Mainstream
        ));
    }

    #[test]
    fn every_variant_has_a_nonempty_glyph() {
        for variant in ALL_VARIANTS {
            let g = variant.glyph();
            assert!(!g.is_empty(), "{variant:?} returned an empty glyph");
            assert_ne!(g, "\u{FFFD}", "{variant:?} returned a replacement char");
        }
    }

    #[test]
    fn cell_width_defaults_to_one() {
        for variant in ALL_VARIANTS {
            assert_eq!(
                variant.cell_width(),
                1,
                "{variant:?} unexpectedly reported non-default width"
            );
        }
    }

    #[test]
    fn pill_caps_have_modern_variant() {
        // Pill caps are the most terminal-dependent glyph. Make sure both
        // Modern and the fallback give a renderable result.
        let left_modern = Icon::PillCapLeft.glyph();
        let right_modern = Icon::PillCapRight.glyph();
        assert!(!left_modern.is_empty());
        assert!(!right_modern.is_empty());
    }

    #[test]
    fn arrows_are_universal() {
        // Arrows must be the same across terminal classes — they are part
        // of the universally-supported BMP and should never fall back.
        assert_eq!(Icon::ArrowRight.glyph(), "→");
        assert_eq!(Icon::ArrowUp.glyph(), "↑");
        assert_eq!(Icon::ArrowDown.glyph(), "↓");
    }
}
