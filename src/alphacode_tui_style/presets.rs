//! Curated built-in theme presets.
//!
//! Before this module the only way to restyle the TUI was to write 39 hex
//! values into `[display.colors]` by hand. That is a lot of work to get a
//! coherent result, and an incoherent palette is worse than the default one:
//! the roles are related (diff-add should agree with success, todo-pending with
//! warning), so picking them independently tends to produce a set that fights
//! itself.
//!
//! A preset is therefore not 39 free colors. It is a small [`ThemeSeed`] — the
//! ~14 colors a terminal theme actually defines — and [`ThemeSeed::palette`]
//! derives all 39 roles from it. Adding a theme means transcribing its published
//! palette, not inventing role assignments, and every theme stays internally
//! consistent by construction.
//!
//! Presets are applied as *user overrides*, so they flow through the existing
//! [`crate::alphacode_tui_style::palette::adapt_buffer_for_palette`] pass. That
//! means a preset also recolors the hundreds of ad hoc `rgb(...)` literals in
//! widgets, not just the named roles — picking a theme restyles the whole TUI.

use super::palette::{Palette, Role};

/// An 8-bit RGB triple, matching the palette module's representation.
pub type Rgb = (u8, u8, u8);

/// The colors a terminal theme actually publishes.
///
/// Deliberately close to the conventional 16-color terminal palette plus a few
/// surface tones, because that is the form every upstream theme documents. Roles
/// are derived from these rather than listed, so a transcription error is a
/// wrong *color*, never a wrong or missing *role*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeSeed {
    /// Stable config key, e.g. `tokyo-night`.
    pub id: &'static str,
    pub display_name: &'static str,
    /// One-line description shown by `/theme`.
    pub description: &'static str,
    /// Whether this theme is designed for a dark terminal background.
    pub is_dark: bool,

    /// Primary body text.
    pub fg: Rgb,
    /// Secondary text: labels, tool names, quotes.
    pub fg_muted: Rgb,
    /// Lowest-emphasis text: hints, separators, diff context.
    pub fg_subtle: Rgb,

    /// Panel background (message panels, code blocks, tool rows).
    pub surface: Rgb,
    /// Raised background (selected row, progress trough).
    pub surface_alt: Rgb,
    /// Borders and rules.
    pub border: Rgb,

    pub red: Rgb,
    pub orange: Rgb,
    pub yellow: Rgb,
    pub green: Rgb,
    pub cyan: Rgb,
    pub blue: Rgb,
    pub magenta: Rgb,
    /// Brand accent: headings highlights, spinner, memory.
    pub accent: Rgb,
}

impl ThemeSeed {
    /// Derive the full 39-role palette.
    ///
    /// Every role is assigned, so a preset never leaves a role sitting on the
    /// built-in default and clashing with the rest of the theme. The mapping is
    /// the single place role semantics are tied to palette slots; keeping it
    /// here (rather than per theme) is what makes the themes consistent with
    /// each other.
    pub fn palette(&self) -> Palette {
        let mut palette = Palette::default();
        for (role, rgb) in self.role_assignments() {
            palette.set(role, rgb);
        }
        palette
    }

    /// Role → color for this seed. Exposed so tests can assert full coverage.
    pub fn role_assignments(&self) -> [(Role, Rgb); 39] {
        [
            // Conversation
            (Role::User, self.blue),
            (Role::UserText, self.fg),
            (Role::UserBg, self.surface),
            (Role::Ai, self.green),
            (Role::AiText, self.fg),
            (Role::System, self.magenta),
            // Tools
            (Role::Tool, self.fg_muted),
            (Role::ToolBg, self.surface),
            (Role::FileLink, self.cyan),
            // Emphasis
            (Role::Dim, self.fg_subtle),
            (Role::Accent, self.accent),
            (Role::Pending, self.fg_muted),
            (Role::Queued, self.yellow),
            (Role::Asap, self.cyan),
            // Header
            (Role::HeaderIcon, self.cyan),
            (Role::HeaderName, self.blue),
            (Role::HeaderSession, self.fg),
            (Role::ModelName, self.magenta),
            // Status
            (Role::Success, self.green),
            (Role::Warning, self.orange),
            (Role::Error, self.red),
            (Role::Info, self.blue),
            // Chrome
            (Role::Border, self.border),
            (Role::SelectionBg, self.surface_alt),
            (Role::CodeBg, self.surface),
            // Markdown
            (Role::Heading, self.blue),
            (Role::Link, self.cyan),
            (Role::Quote, self.fg_muted),
            // Progress
            (Role::Spinner, self.accent),
            (Role::ProgressFill, self.green),
            (Role::ProgressBg, self.surface_alt),
            // Diffs
            (Role::DiffAdd, self.green),
            (Role::DiffRemove, self.red),
            (Role::DiffContext, self.fg_subtle),
            // Swarm
            (Role::SwarmAgent, self.cyan),
            (Role::SwarmTask, self.yellow),
            // Memory and todos
            (Role::Memory, self.accent),
            (Role::TodoDone, self.green),
            (Role::TodoPending, self.yellow),
        ]
    }
}

/// Every built-in preset, in menu order: dark themes first, then light.
pub const PRESETS: &[ThemeSeed] = &[
    // --- Premium dark themes ---
    ALPHACODE,
    AURORA,
    NEON_NOIR,
    EMBER,
    TOKYO_NIGHT_STORM,
    TOKYO_NIGHT,
    CATPPUCCIN_MOCHA,
    ONE_DARK,
    DRACULA,
    NORD,
    GRUVBOX_DARK,
    ROSE_PINE,
    GITHUB_DARK,
    KANAGAWA_WAVE,
    AYU_MIRAGE,
    EVERFOREST_DARK,
    SYNTHWAVE_84,
    PALENIGHT,
    MONOKAI_PRO,
    MONOKAI_PRO_SPECTRUM,
    NIGHT_OWL,
    SOLARIZED_DARK,
    OXOCARBON_DARK,
    MODUS_VIVENDI,
    EVERBLUSH,
    PENUMBRA_DARK,
    NOVA_DARK,
    // --- Light themes ---
    FROST,
    CATPPUCCIN_LATTE,
    GITHUB_LIGHT,
    SOLARIZED_LIGHT,
    ONE_LIGHT,
    NORD_LIGHT,
    EVERFOREST_LIGHT,
    AYU_LIGHT,
    GRUVBOX_LIGHT,
    QUIET_LIGHT,
];

/// Look up a preset by id, tolerating separator and case differences so
/// `Tokyo Night`, `tokyo_night`, and `tokyo-night` all resolve.
pub fn preset_by_id(id: &str) -> Option<&'static ThemeSeed> {
    let normalized = normalize_id(id);
    PRESETS
        .iter()
        .find(|preset| normalize_id(preset.id) == normalized)
}

fn normalize_id(id: &str) -> String {
    id.trim().to_ascii_lowercase().replace(['_', ' '], "-")
}

/// The palette for a preset id, or `None` when the id is unknown.
pub fn preset_palette(id: &str) -> Option<Palette> {
    preset_by_id(id).map(ThemeSeed::palette)
}

pub const ALPHACODE: ThemeSeed = ThemeSeed {
    id: "alphacode",
    display_name: "AlphaCode",
    description: "Premium deep-space dark with vibrant cyan-blue accents",
    is_dark: true,
    fg: (224, 230, 244),
    fg_muted: (142, 155, 185),
    fg_subtle: (82, 92, 118),
    surface: (18, 22, 38),
    surface_alt: (30, 36, 58),
    border: (50, 58, 82),
    red: (255, 105, 105),
    orange: (255, 175, 95),
    yellow: (255, 210, 80),
    green: (95, 225, 150),
    cyan: (85, 210, 240),
    blue: (110, 188, 255),
    magenta: (195, 155, 255),
    accent: (195, 155, 255),
};

pub const TOKYO_NIGHT: ThemeSeed = ThemeSeed {
    id: "tokyo-night",
    display_name: "Tokyo Night",
    description: "Deep indigo with soft neon accents",
    is_dark: true,
    fg: (192, 202, 245),
    fg_muted: (154, 165, 206),
    fg_subtle: (86, 95, 137),
    surface: (31, 35, 53),
    surface_alt: (41, 46, 66),
    border: (65, 72, 104),
    red: (247, 118, 142),
    orange: (255, 158, 100),
    yellow: (224, 175, 104),
    green: (158, 206, 106),
    cyan: (125, 207, 255),
    blue: (122, 162, 247),
    magenta: (187, 154, 247),
    accent: (187, 154, 247),
};

pub const CATPPUCCIN_MOCHA: ThemeSeed = ThemeSeed {
    id: "catppuccin-mocha",
    display_name: "Catppuccin Mocha",
    description: "Warm pastel dark, low contrast and easy on the eyes",
    is_dark: true,
    fg: (205, 214, 244),
    fg_muted: (166, 173, 200),
    fg_subtle: (108, 112, 134),
    surface: (24, 24, 37),
    surface_alt: (49, 50, 68),
    border: (69, 71, 90),
    red: (243, 139, 168),
    orange: (250, 179, 135),
    yellow: (249, 226, 175),
    green: (166, 227, 161),
    cyan: (137, 220, 235),
    blue: (137, 180, 250),
    magenta: (203, 166, 247),
    accent: (203, 166, 247),
};

pub const ONE_DARK: ThemeSeed = ThemeSeed {
    id: "one-dark",
    display_name: "One Dark",
    description: "The Atom classic: balanced, neutral, highly legible",
    is_dark: true,
    fg: (171, 178, 191),
    fg_muted: (130, 137, 151),
    fg_subtle: (92, 99, 112),
    surface: (33, 37, 43),
    surface_alt: (44, 49, 58),
    border: (62, 68, 81),
    red: (224, 108, 117),
    orange: (209, 154, 102),
    yellow: (229, 192, 123),
    green: (152, 195, 121),
    cyan: (86, 182, 194),
    blue: (97, 175, 239),
    magenta: (198, 120, 221),
    accent: (198, 120, 221),
};

pub const DRACULA: ThemeSeed = ThemeSeed {
    id: "dracula",
    display_name: "Dracula",
    description: "High-saturation dark with vivid accents",
    is_dark: true,
    fg: (248, 248, 242),
    fg_muted: (189, 195, 214),
    fg_subtle: (98, 114, 164),
    surface: (40, 42, 54),
    surface_alt: (68, 71, 90),
    border: (98, 114, 164),
    red: (255, 85, 85),
    orange: (255, 184, 108),
    yellow: (241, 250, 140),
    green: (80, 250, 123),
    cyan: (139, 233, 253),
    // Dracula publishes no distinct blue; purple is its cool accent.
    blue: (189, 147, 249),
    magenta: (255, 121, 198),
    accent: (255, 121, 198),
};

pub const NORD: ThemeSeed = ThemeSeed {
    id: "nord",
    display_name: "Nord",
    description: "Arctic blue-grey, muted and calm",
    is_dark: true,
    // `nord0` is the base surface, not `nord1`. Using `nord1` here would push
    // the dim tone (`nord3`) to roughly 1.4:1 against it, which is unreadable;
    // the published comment tone is used for `fg_subtle` instead.
    fg: (216, 222, 233),
    fg_muted: (143, 155, 175),
    fg_subtle: (123, 136, 161),
    surface: (46, 52, 64),
    surface_alt: (59, 66, 82),
    border: (67, 76, 94),
    red: (191, 97, 106),
    orange: (208, 135, 112),
    yellow: (235, 203, 139),
    green: (163, 190, 140),
    cyan: (136, 192, 208),
    blue: (129, 161, 193),
    magenta: (180, 142, 173),
    accent: (136, 192, 208),
};

pub const GRUVBOX_DARK: ThemeSeed = ThemeSeed {
    id: "gruvbox-dark",
    display_name: "Gruvbox Dark",
    description: "Retro warm earth tones, heavy contrast",
    is_dark: true,
    fg: (235, 219, 178),
    fg_muted: (189, 174, 147),
    fg_subtle: (146, 131, 116),
    surface: (50, 48, 47),
    surface_alt: (60, 56, 54),
    border: (80, 73, 69),
    red: (251, 73, 52),
    orange: (254, 128, 25),
    yellow: (250, 189, 47),
    green: (184, 187, 38),
    cyan: (142, 192, 124),
    blue: (131, 165, 152),
    magenta: (211, 134, 155),
    accent: (211, 134, 155),
};

pub const ROSE_PINE: ThemeSeed = ThemeSeed {
    id: "rose-pine",
    display_name: "Rosé Pine",
    description: "Muted plum and rose, soho vibes",
    is_dark: true,
    fg: (224, 222, 244),
    fg_muted: (144, 140, 170),
    fg_subtle: (110, 106, 134),
    surface: (31, 29, 46),
    surface_alt: (38, 35, 58),
    border: (64, 61, 82),
    red: (235, 111, 146),
    orange: (235, 188, 186),
    yellow: (246, 193, 119),
    // Rosé Pine has no true green; `pine` is its cool signal color.
    green: (49, 116, 143),
    cyan: (156, 207, 216),
    blue: (156, 207, 216),
    magenta: (196, 167, 231),
    accent: (196, 167, 231),
};

pub const GITHUB_DARK: ThemeSeed = ThemeSeed {
    id: "github-dark",
    display_name: "GitHub Dark",
    description: "Familiar, high-legibility, professional",
    is_dark: true,
    fg: (201, 209, 217),
    fg_muted: (139, 148, 158),
    fg_subtle: (110, 118, 129),
    surface: (22, 27, 34),
    surface_alt: (33, 38, 45),
    border: (48, 54, 61),
    red: (255, 123, 114),
    orange: (255, 166, 87),
    yellow: (210, 153, 34),
    green: (63, 185, 80),
    cyan: (57, 197, 207),
    blue: (88, 166, 255),
    magenta: (188, 140, 255),
    accent: (188, 140, 255),
};

pub const SOLARIZED_DARK: ThemeSeed = ThemeSeed {
    id: "solarized-dark",
    display_name: "Solarized Dark",
    description: "Precision-tuned contrast, the original science-y palette",
    is_dark: true,
    // Solarized names its tones by lightness, not by prominence: on a dark
    // background `base1` is *emphasis*, brighter than the `base0` body text.
    // The de-emphasised tones are therefore base00 and base01, going down.
    fg: (131, 148, 150),
    fg_muted: (101, 123, 131),
    fg_subtle: (88, 110, 117),
    surface: (0, 43, 54),
    surface_alt: (7, 54, 66),
    border: (88, 110, 117),
    red: (220, 50, 47),
    orange: (203, 75, 22),
    yellow: (181, 137, 0),
    green: (133, 153, 0),
    cyan: (42, 161, 152),
    blue: (38, 139, 210),
    magenta: (211, 54, 130),
    accent: (108, 113, 196),
};

pub const CATPPUCCIN_LATTE: ThemeSeed = ThemeSeed {
    id: "catppuccin-latte",
    display_name: "Catppuccin Latte",
    description: "Soft pastel light, the Mocha counterpart",
    is_dark: false,
    fg: (76, 79, 105),
    fg_muted: (108, 111, 133),
    fg_subtle: (156, 160, 176),
    surface: (230, 233, 239),
    surface_alt: (204, 208, 218),
    border: (188, 192, 204),
    red: (210, 15, 57),
    orange: (254, 100, 11),
    yellow: (223, 142, 29),
    green: (64, 160, 43),
    cyan: (23, 146, 153),
    blue: (30, 102, 245),
    magenta: (136, 57, 239),
    accent: (136, 57, 239),
};

pub const GITHUB_LIGHT: ThemeSeed = ThemeSeed {
    id: "github-light",
    display_name: "GitHub Light",
    description: "Clean, neutral, maximum legibility in bright rooms",
    is_dark: false,
    fg: (36, 41, 47),
    fg_muted: (87, 96, 106),
    fg_subtle: (110, 119, 129),
    surface: (246, 248, 250),
    surface_alt: (234, 238, 242),
    border: (208, 215, 222),
    red: (207, 34, 46),
    orange: (188, 76, 0),
    yellow: (154, 103, 0),
    green: (26, 127, 55),
    cyan: (27, 124, 131),
    blue: (9, 105, 218),
    magenta: (130, 80, 223),
    accent: (130, 80, 223),
};

pub const SOLARIZED_LIGHT: ThemeSeed = ThemeSeed {
    id: "solarized-light",
    display_name: "Solarized Light",
    description: "The warm-paper counterpart to Solarized Dark",
    is_dark: false,
    // Mirrored against Solarized Dark: on a light background the *darker* tone
    // is the prominent one, so emphasis runs base00 → base0 → base1.
    fg: (101, 123, 131),
    fg_muted: (131, 148, 150),
    fg_subtle: (147, 161, 161),
    surface: (253, 246, 227),
    surface_alt: (238, 232, 213),
    border: (147, 161, 161),
    red: (220, 50, 47),
    orange: (203, 75, 22),
    yellow: (181, 137, 0),
    green: (133, 153, 0),
    cyan: (42, 161, 152),
    blue: (38, 139, 210),
    magenta: (211, 54, 130),
    accent: (108, 113, 196),
};

pub const KANAGAWA_WAVE: ThemeSeed = ThemeSeed {
    id: "kanagawa-wave",
    display_name: "Kanagawa Wave",
    description: "Ink-wash dark with jewel-toned accents",
    is_dark: true,
    fg: (220, 215, 186),
    fg_muted: (200, 192, 147),
    fg_subtle: (114, 113, 105),
    surface: (31, 31, 40),
    surface_alt: (42, 42, 55),
    border: (54, 54, 70),
    red: (232, 36, 36),
    orange: (255, 158, 59),
    yellow: (230, 195, 132),
    green: (152, 187, 108),
    cyan: (106, 149, 137),
    blue: (126, 156, 216),
    magenta: (149, 127, 184),
    accent: (126, 156, 216),
};

pub const AYU_MIRAGE: ThemeSeed = ThemeSeed {
    id: "ayu-mirage",
    display_name: "Ayu Mirage",
    description: "Dusty twilight blues with warm saturated highlights",
    is_dark: true,
    fg: (203, 204, 198),
    fg_muted: (138, 145, 153),
    fg_subtle: (92, 103, 115),
    surface: (31, 36, 48),
    surface_alt: (42, 50, 64),
    border: (62, 70, 89),
    red: (240, 113, 120),
    orange: (255, 173, 102),
    yellow: (255, 209, 115),
    green: (170, 217, 76),
    cyan: (149, 230, 203),
    blue: (115, 208, 255),
    magenta: (215, 161, 249),
    accent: (115, 208, 255),
};

pub const EVERFOREST_DARK: ThemeSeed = ThemeSeed {
    id: "everforest-dark",
    display_name: "Everforest Dark",
    description: "Soft green-grey forest tones, calm and natural",
    is_dark: true,
    fg: (211, 198, 170),
    fg_muted: (133, 146, 137),
    fg_subtle: (94, 106, 100),
    surface: (30, 35, 38),
    surface_alt: (45, 53, 59),
    border: (75, 86, 92),
    red: (230, 126, 128),
    orange: (230, 152, 117),
    yellow: (219, 188, 127),
    green: (167, 192, 128),
    cyan: (131, 192, 146),
    blue: (127, 187, 179),
    magenta: (214, 153, 182),
    accent: (131, 192, 146),
};

pub const SYNTHWAVE_84: ThemeSeed = ThemeSeed {
    id: "synthwave-84",
    display_name: "Synthwave '84",
    description: "Neon-outrun purples with electric cyan and pink",
    is_dark: true,
    fg: (244, 244, 244),
    fg_muted: (179, 157, 251),
    fg_subtle: (93, 78, 122),
    surface: (36, 27, 47),
    surface_alt: (47, 36, 64),
    border: (72, 59, 102),
    red: (254, 68, 80),
    orange: (254, 128, 25),
    yellow: (249, 248, 113),
    green: (114, 241, 184),
    cyan: (54, 249, 246),
    blue: (90, 200, 250),
    magenta: (255, 100, 130),
    accent: (54, 249, 246),
};

pub const PALENIGHT: ThemeSeed = ThemeSeed {
    id: "palenight",
    display_name: "Palenight",
    description: "Material dark with periwinkle blues and lavender",
    is_dark: true,
    fg: (149, 157, 203),
    fg_muted: (103, 110, 149),
    fg_subtle: (86, 93, 128),
    surface: (41, 45, 62),
    surface_alt: (51, 55, 71),
    border: (59, 66, 97),
    red: (240, 113, 120),
    orange: (247, 140, 108),
    yellow: (255, 203, 107),
    green: (195, 232, 141),
    cyan: (137, 221, 255),
    blue: (130, 170, 255),
    magenta: (199, 146, 234),
    accent: (130, 170, 255),
};

pub const MONOKAI_PRO: ThemeSeed = ThemeSeed {
    id: "monokai-pro",
    display_name: "Monokai Pro",
    description: "High-contrast dark with candy-bright syntax colors",
    is_dark: true,
    fg: (252, 252, 250),
    fg_muted: (158, 154, 158),
    fg_subtle: (100, 96, 100),
    surface: (45, 42, 46),
    surface_alt: (58, 55, 64),
    border: (74, 71, 82),
    red: (255, 97, 136),
    orange: (252, 152, 103),
    yellow: (255, 216, 102),
    green: (169, 220, 118),
    cyan: (120, 220, 232),
    blue: (122, 162, 247),
    magenta: (171, 157, 242),
    accent: (120, 220, 232),
};

pub const NIGHT_OWL: ThemeSeed = ThemeSeed {
    id: "night-owl",
    display_name: "Night Owl",
    description: "Deep-space navy with crisp cool accents",
    is_dark: true,
    fg: (214, 222, 235),
    fg_muted: (99, 119, 119),
    fg_subtle: (58, 78, 102),
    surface: (1, 22, 39),
    surface_alt: (14, 41, 60),
    border: (29, 59, 83),
    red: (239, 83, 80),
    orange: (247, 140, 108),
    yellow: (255, 235, 149),
    green: (34, 197, 94),
    cyan: (33, 199, 168),
    blue: (130, 170, 255),
    magenta: (199, 146, 234),
    accent: (130, 170, 255),
};

pub const ONE_LIGHT: ThemeSeed = ThemeSeed {
    id: "one-light",
    display_name: "One Light",
    description: "Atom classic: crisp white with clean muted colors",
    is_dark: false,
    fg: (56, 58, 66),
    fg_muted: (144, 145, 153),
    fg_subtle: (185, 187, 192),
    surface: (250, 250, 250),
    surface_alt: (239, 239, 241),
    border: (225, 226, 230),
    red: (228, 86, 73),
    orange: (193, 132, 1),
    yellow: (152, 104, 1),
    green: (80, 161, 79),
    cyan: (1, 132, 188),
    blue: (64, 120, 242),
    magenta: (166, 38, 164),
    accent: (64, 120, 242),
};

pub const NORD_LIGHT: ThemeSeed = ThemeSeed {
    id: "nord-light",
    display_name: "Nord Light",
    description: "Polar daylight: pale grey-blues with nord accents",
    is_dark: false,
    fg: (46, 52, 64),
    fg_muted: (76, 86, 106),
    fg_subtle: (138, 147, 165),
    surface: (229, 233, 240),
    surface_alt: (216, 222, 233),
    border: (195, 202, 219),
    red: (191, 97, 106),
    orange: (208, 135, 112),
    yellow: (235, 203, 139),
    green: (163, 190, 140),
    cyan: (136, 192, 208),
    blue: (129, 161, 193),
    magenta: (180, 142, 173),
    accent: (129, 161, 193),
};

pub const EVERFOREST_LIGHT: ThemeSeed = ThemeSeed {
    id: "everforest-light",
    display_name: "Everforest Light",
    description: "Warm paper cream with soft forest greens",
    is_dark: false,
    fg: (92, 106, 114),
    fg_muted: (133, 146, 137),
    fg_subtle: (157, 169, 160),
    surface: (253, 246, 227),
    surface_alt: (244, 237, 218),
    border: (211, 198, 170),
    red: (248, 85, 82),
    orange: (245, 125, 38),
    yellow: (223, 160, 0),
    green: (141, 161, 1),
    cyan: (53, 167, 124),
    blue: (58, 148, 197),
    magenta: (223, 105, 186),
    accent: (58, 148, 197),
};

pub const AYU_LIGHT: ThemeSeed = ThemeSeed {
    id: "ayu-light",
    display_name: "Ayu Light",
    description: "Bright daylight with cool teal and salmon accents",
    is_dark: false,
    fg: (92, 103, 115),
    fg_muted: (138, 145, 153),
    fg_subtle: (171, 176, 182),
    surface: (250, 250, 250),
    surface_alt: (237, 240, 242),
    border: (217, 222, 226),
    red: (240, 113, 120),
    orange: (255, 143, 64),
    yellow: (242, 174, 73),
    green: (134, 179, 0),
    cyan: (76, 191, 153),
    blue: (54, 163, 217),
    magenta: (163, 122, 204),
    accent: (54, 163, 217),
};

pub const GRUVBOX_LIGHT: ThemeSeed = ThemeSeed {
    id: "gruvbox-light",
    display_name: "Gruvbox Light",
    description: "Warm parchment with the classic earth-toned contrast",
    is_dark: false,
    fg: (60, 56, 54),
    fg_muted: (102, 92, 84),
    fg_subtle: (146, 131, 116),
    surface: (251, 241, 199),
    surface_alt: (235, 219, 178),
    border: (213, 196, 161),
    red: (157, 0, 6),
    orange: (175, 58, 3),
    yellow: (181, 118, 20),
    green: (121, 116, 14),
    cyan: (66, 123, 88),
    blue: (7, 102, 120),
    magenta: (143, 63, 113),
    accent: (143, 63, 113),
};

pub const TOKYO_NIGHT_STORM: ThemeSeed = ThemeSeed {
    id: "tokyo-night-storm",
    display_name: "Tokyo Night Storm",
    description: "The richer, deeper variant of Tokyo Night with stormy skies",
    is_dark: true,
    fg: (205, 214, 244),
    fg_muted: (147, 153, 178),
    fg_subtle: (75, 83, 115),
    surface: (24, 25, 38),
    surface_alt: (37, 39, 54),
    border: (58, 64, 90),
    red: (247, 118, 142),
    orange: (255, 158, 100),
    yellow: (224, 175, 104),
    green: (158, 206, 106),
    cyan: (125, 207, 255),
    blue: (122, 162, 247),
    magenta: (187, 154, 247),
    accent: (187, 154, 247),
};

pub const MONOKAI_PRO_SPECTRUM: ThemeSeed = ThemeSeed {
    id: "monokai-pro-spectrum",
    display_name: "Monokai Pro Spectrum",
    description: "The iconic Monokai Pro with a vivid spectrum filter",
    is_dark: true,
    fg: (252, 252, 250),
    fg_muted: (162, 158, 162),
    fg_subtle: (105, 101, 105),
    surface: (45, 42, 46),
    surface_alt: (58, 55, 64),
    border: (74, 71, 82),
    red: (255, 95, 98),
    orange: (255, 160, 104),
    yellow: (255, 216, 102),
    green: (171, 220, 116),
    cyan: (104, 214, 219),
    blue: (130, 153, 247),
    magenta: (180, 137, 245),
    accent: (130, 153, 247),
};

pub const OXOCARBON_DARK: ThemeSeed = ThemeSeed {
    id: "oxocarbon-dark",
    display_name: "Oxocarbon Dark",
    description: "IBM-inspired dark with carbon blues and bright orange",
    is_dark: true,
    fg: (217, 224, 238),
    fg_muted: (156, 169, 191),
    fg_subtle: (99, 114, 140),
    surface: (25, 25, 25),
    surface_alt: (39, 39, 39),
    border: (60, 60, 60),
    red: (255, 91, 99),
    orange: (254, 156, 80),
    yellow: (255, 224, 103),
    green: (4, 217, 143),
    cyan: (111, 209, 224),
    blue: (146, 220, 255),
    magenta: (245, 178, 255),
    accent: (111, 209, 224),
};

pub const MODUS_VIVENDI: ThemeSeed = ThemeSeed {
    id: "modus-vivendi",
    display_name: "Modus Vivendi",
    description: "High-contrast readable theme for long coding sessions",
    is_dark: true,
    fg: (230, 230, 230),
    fg_muted: (150, 150, 150),
    fg_subtle: (100, 100, 100),
    surface: (18, 18, 18),
    surface_alt: (36, 36, 36),
    border: (60, 60, 60),
    red: (255, 80, 80),
    orange: (255, 140, 60),
    yellow: (255, 210, 0),
    green: (0, 220, 100),
    cyan: (0, 190, 220),
    blue: (100, 150, 255),
    magenta: (200, 100, 255),
    accent: (100, 150, 255),
};

pub const EVERBLUSH: ThemeSeed = ThemeSeed {
    id: "everblush",
    display_name: "Everblush",
    description: "Dark, muted, and easy-on-the-eyes with warm pastel accents",
    is_dark: true,
    fg: (209, 215, 209),
    fg_muted: (123, 137, 137),
    fg_subtle: (76, 87, 87),
    surface: (32, 37, 38),
    surface_alt: (43, 48, 49),
    border: (63, 68, 71),
    red: (238, 92, 92),
    orange: (232, 141, 87),
    yellow: (230, 193, 109),
    green: (105, 210, 135),
    cyan: (88, 192, 212),
    blue: (103, 163, 228),
    magenta: (195, 134, 213),
    accent: (105, 210, 135),
};

pub const PENUMBRA_DARK: ThemeSeed = ThemeSeed {
    id: "penumbra-dark",
    display_name: "Penumbra Dark",
    description: "Softer, more subdued dark with warm earth undertones",
    is_dark: true,
    fg: (227, 227, 219),
    fg_muted: (153, 153, 141),
    fg_subtle: (95, 95, 86),
    surface: (30, 30, 26),
    surface_alt: (42, 42, 37),
    border: (62, 62, 56),
    red: (220, 93, 93),
    orange: (224, 158, 82),
    yellow: (218, 190, 108),
    green: (138, 190, 112),
    cyan: (105, 186, 196),
    blue: (125, 158, 210),
    magenta: (177, 138, 192),
    accent: (125, 158, 210),
};

pub const NOVA_DARK: ThemeSeed = ThemeSeed {
    id: "nova-dark",
    display_name: "Nova Dark",
    description: "A modern dark with cool blue-slate and bright cyan",
    is_dark: true,
    fg: (217, 224, 233),
    fg_muted: (140, 155, 175),
    fg_subtle: (80, 92, 112),
    surface: (20, 24, 34),
    surface_alt: (30, 36, 52),
    border: (52, 62, 82),
    red: (255, 107, 107),
    orange: (255, 167, 95),
    yellow: (255, 220, 90),
    green: (115, 218, 148),
    cyan: (86, 210, 235),
    blue: (120, 172, 255),
    magenta: (190, 148, 255),
    accent: (86, 210, 235),
};

pub const QUIET_LIGHT: ThemeSeed = ThemeSeed {
    id: "quiet-light",
    display_name: "Quiet Light",
    description: "Gentle warm-light theme, minimal and distraction-free",
    is_dark: false,
    fg: (65, 60, 50),
    fg_muted: (130, 120, 110),
    fg_subtle: (175, 165, 155),
    surface: (248, 244, 238),
    surface_alt: (235, 228, 220),
    border: (210, 200, 190),
    red: (210, 70, 65),
    orange: (200, 130, 45),
    yellow: (180, 145, 20),
    green: (80, 150, 70),
    cyan: (40, 140, 150),
    blue: (50, 110, 200),
    magenta: (150, 70, 180),
    accent: (50, 110, 200),
};

/// Elite "Aurora" theme: deep midnight with a sweeping aurora gradient of
/// cyan → violet → magenta → coral. Designed for high information density:
/// the panel background is one of the darkest in any built-in preset, so the
/// cyan/magenta accents read like northern lights against the night sky.
///
/// The blue slot is pushed to cyan (210) so user messages feel electric; the
/// accent slot is a saturated violet (195) that links with the magenta
/// memory role for a coherent aurora narrative.
pub const AURORA: ThemeSeed = ThemeSeed {
    id: "aurora",
    display_name: "Aurora",
    description: "Midnight aurora: cyan, violet, and magenta drift across deep space",
    is_dark: true,
    fg: (224, 232, 248),
    fg_muted: (146, 158, 192),
    fg_subtle: (90, 102, 132),
    surface: (12, 16, 30),
    surface_alt: (22, 28, 50),
    border: (44, 52, 84),
    red: (255, 102, 130),
    orange: (255, 168, 110),
    yellow: (255, 220, 120),
    green: (110, 232, 168),
    cyan: (110, 220, 240),
    blue: (110, 200, 255),
    magenta: (210, 140, 255),
    accent: (180, 130, 255),
};

/// Elite "Neon Noir" theme: synthwave palette — hot pink, electric cyan, and
/// deep purple against a near-black surface. Bold and dense, reads almost
/// like an arcade marquee at idle.
///
/// fg_subtle and surface are both near-black so the borders and dim text
/// recede; the saturated primary colors are tuned to stay readable against
/// the dark surface without luminance clipping.
pub const NEON_NOIR: ThemeSeed = ThemeSeed {
    id: "neon-noir",
    display_name: "Neon Noir",
    description: "Synthwave hot pink, electric cyan, and violet on near-black",
    is_dark: true,
    fg: (232, 232, 248),
    fg_muted: (160, 152, 192),
    fg_subtle: (108, 100, 140),
    surface: (10, 8, 22),
    surface_alt: (24, 18, 42),
    border: (54, 38, 84),
    red: (255, 90, 168),
    orange: (255, 142, 110),
    yellow: (250, 224, 110),
    green: (90, 240, 200),
    cyan: (90, 230, 255),
    blue: (120, 170, 255),
    magenta: (220, 110, 255),
    accent: (255, 92, 198),
};

/// Elite "Frost" theme: high-key light theme with cool blue accents.
/// Re-tunes the foreground/background relationship for users who prefer
/// working in bright environments but want the cyan-blue brand vibe.
pub const FROST: ThemeSeed = ThemeSeed {
    id: "frost",
    display_name: "Frost",
    description: "High-key cool light theme, near-white surface with icy blue accents",
    is_dark: false,
    fg: (28, 36, 52),
    fg_muted: (88, 100, 124),
    fg_subtle: (148, 160, 184),
    surface: (244, 248, 254),
    surface_alt: (228, 234, 244),
    border: (188, 200, 220),
    red: (210, 60, 78),
    orange: (210, 122, 50),
    yellow: (180, 132, 24),
    green: (52, 152, 96),
    cyan: (40, 152, 184),
    blue: (52, 108, 220),
    magenta: (148, 80, 200),
    accent: (52, 108, 220),
};

/// Elite "Ember" theme: warm dusk with amber, rose, and copper highlights.
/// Built for users who find pure cool palettes fatiguing during long
/// sessions. Success and accent sit on the same warm lane so the
/// "go" affordance stays hot.
pub const EMBER: ThemeSeed = ThemeSeed {
    id: "ember",
    display_name: "Ember",
    description: "Warm dusk: amber, rose, and copper highlights on deep cocoa",
    is_dark: true,
    fg: (240, 222, 204),
    fg_muted: (172, 142, 124),
    fg_subtle: (118, 96, 86),
    surface: (28, 20, 18),
    surface_alt: (40, 30, 26),
    border: (70, 54, 48),
    red: (240, 100, 100),
    orange: (242, 152, 92),
    yellow: (244, 196, 96),
    green: (148, 200, 124),
    cyan: (108, 188, 200),
    blue: (130, 158, 220),
    magenta: (220, 142, 184),
    accent: (242, 152, 92),
};

#[cfg(test)]
#[path = "presets_tests.rs"]
mod presets_tests;
