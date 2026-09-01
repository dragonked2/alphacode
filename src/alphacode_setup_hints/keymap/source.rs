//! Where a discovered key binding came from, and the binding record itself.

use serde::{Deserialize, Serialize};

use super::chord::KeyChord;

/// The origin of a discovered binding on the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KeySource {
    /// A macOS system-wide shortcut (`com.apple.symbolichotkeys`).
    MacosSystem,
    /// A binding declared by the terminal emulator (config or built-in default).
    Terminal,
    /// A binding declared by a third-party app that grabs global hotkeys before
    /// the terminal sees them: window managers (OmniWM, AeroSpace, yabai/skhd),
    /// automation tools (Hammerspoon), launchers (Raycast), etc. The specific
    /// app is named in [`DiscoveredBinding::tool`].
    ExternalApp,
}

impl KeySource {
    pub fn label(self) -> &'static str {
        match self {
            KeySource::MacosSystem => "macOS system shortcut",
            KeySource::Terminal => "terminal",
            KeySource::ExternalApp => "external app",
        }
    }
}

/// A key binding discovered on the machine that may intercept input before it
/// reaches alphacode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiscoveredBinding {
    /// The normalized chord this binding triggers on.
    pub chord: KeyChord,
    /// Which layer owns this binding.
    pub source: KeySource,
    /// What the binding does, e.g. "Spotlight: Show search" or
    /// "copy_to_clipboard:mixed".
    pub action: String,
    /// The raw declaration we parsed, for debugging (e.g. the original config
    /// line or the symbolic-hotkey id).
    pub raw: String,
    /// For [`KeySource::ExternalApp`], the human-facing name of the app that
    /// owns this binding (e.g. "OmniWM", "AeroSpace", "skhd"). Empty for the
    /// macOS system and terminal sources, where the source label is enough.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tool: String,
}
