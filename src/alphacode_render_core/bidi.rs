//! Bidirectional text and Arabic script shaping support for terminal display.
//!
//! Standard terminals draw glyphs cell-by-cell from left to right (LTR) and
//! lack native font shaping engines. When Arabic or other right-to-left (RTL)
//! text is output directly:
//! 1. Characters appear in reverse visual order (LTR instead of RTL).
//! 2. Arabic cursive letters appear unjoined / isolated instead of connected.
//!
//! This module resolves both issues by:
//! 1. Reshaping Arabic characters into their contextual Presentation Forms-B
//!    (isolated, initial, medial, final) and forming essential ligatures (Lam-Alif).
//! 2. Reordering RTL runs using the Unicode Bidirectional Algorithm (UAX #9)
//!    via `unicode_bidi`, so that the visual string passed to Ratatui / crossterm
//!    renders correctly from right to left while preserving embedded LTR runs
//!    (English words, numbers, code snippets).
//! 3. Preserving diacritics / tashkeel attached to their respective base characters
//!    during visual reversal so marks never float, detach, or collide.
//! 4. Preserving span styles in Ratatui `Line<'static>` across visual reordering.

use ratatui::style::Style;
use ratatui::text::{Line, Span};
use std::borrow::Cow;
use unicode_bidi::{BidiInfo, Level};

/// Shaping and layout modes for Arabic text output in terminals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArabicShapingMode {
    /// Automatically adapt to the terminal environment.
    /// Modern terminals (Windows Terminal / `WT_SESSION`, VS Code, WezTerm, Ghostty, Kitty)
    /// use native OpenType / DirectWrite shaping with standard Unicode (U+0600..U+06FF).
    /// Legacy terminals use Presentation Forms-B (U+FE70..U+FEFF).
    Auto,
    /// Preserve standard Arabic Unicode (U+0600..U+06FF) for native terminal shaping.
    StandardUnicode,
    /// Convert Arabic characters into Presentation Forms-B (U+FE70..U+FEFF) for legacy terminals.
    PresentationForms,
}

thread_local! {
    static THREAD_SHAPING_MODE: std::cell::Cell<Option<ArabicShapingMode>> = const { std::cell::Cell::new(None) };
}

static GLOBAL_SHAPING_MODE: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(0);
// 0: Auto, 1: StandardUnicode, 2: PresentationForms

/// Temporarily set the Arabic shaping mode for the duration of a closure on the current thread.
pub fn with_arabic_shaping_mode<F, R>(mode: ArabicShapingMode, f: F) -> R
where
    F: FnOnce() -> R,
{
    THREAD_SHAPING_MODE.with(|cell| {
        let prev = cell.get();
        cell.set(Some(mode));
        let result = f();
        cell.set(prev);
        result
    })
}

/// Set the thread-local Arabic shaping mode override for the current thread.
pub fn set_thread_shaping_mode(mode: Option<ArabicShapingMode>) {
    THREAD_SHAPING_MODE.with(|cell| {
        cell.set(mode);
    });
}

/// Set the global Arabic shaping mode override for all threads.
pub fn set_shaping_mode_override(mode: Option<ArabicShapingMode>) {
    let val = match mode {
        None | Some(ArabicShapingMode::Auto) => 0,
        Some(ArabicShapingMode::StandardUnicode) => 1,
        Some(ArabicShapingMode::PresentationForms) => 2,
    };
    GLOBAL_SHAPING_MODE.store(val, std::sync::atomic::Ordering::SeqCst);
}

/// Query the currently configured shaping mode (thread-local if set, else global).
pub fn get_shaping_mode() -> ArabicShapingMode {
    if let Some(thread_mode) = THREAD_SHAPING_MODE.with(|c| c.get())
        && thread_mode != ArabicShapingMode::Auto
    {
        return thread_mode;
    }
    match GLOBAL_SHAPING_MODE.load(std::sync::atomic::Ordering::SeqCst) {
        1 => ArabicShapingMode::StandardUnicode,
        2 => ArabicShapingMode::PresentationForms,
        _ => ArabicShapingMode::Auto,
    }
}

/// Snapshot of environment variables used to detect native terminal shaping.
/// Exposed so cross-platform terminal detection can be unit-tested cleanly
/// without mutating process-global environment variables.
#[derive(Debug, Default, Clone, Copy)]
pub struct TerminalShapingEnv<'a> {
    pub alphacode_arabic_shaping: Option<&'a str>,
    pub wt_session: Option<&'a str>,
    pub conemu_pid: Option<&'a str>,
    pub term_program: Option<&'a str>,
    pub term: Option<&'a str>,
    pub colorterm: Option<&'a str>,
    pub vte_version: Option<&'a str>,
    pub konsole_version: Option<&'a str>,
    pub kitty_pid: Option<&'a str>,
    pub ghostty_resources_dir: Option<&'a str>,
    pub wezterm_executable: Option<&'a str>,
    pub alacritty_window_id: Option<&'a str>,
    pub iterm_session_id: Option<&'a str>,
    pub warp_session: Option<&'a str>,
    pub gnome_terminal_service: Option<&'a str>,
    pub tilix_id: Option<&'a str>,
    pub terminator_uuid: Option<&'a str>,
}

#[inline]
fn env_is_present(val: Option<&str>) -> bool {
    val.is_some_and(|v| !v.trim().is_empty())
}

#[inline]
fn is_dumb_legacy_term(term: &str) -> bool {
    let t = term.trim().to_ascii_lowercase();
    matches!(
        t.as_str(),
        "dumb" | "linux" | "vt100" | "vt102" | "vt220" | "vt320" | "ansi" | "cons25" | "cygwin"
    ) || t.starts_with("vt10")
        || t.starts_with("vt22")
        || t.starts_with("vt32")
}

/// Pure terminal native-shaping detector for a given environment snapshot.
///
/// Modern terminals on Windows (Windows Terminal, ConEmu, VS Code, Mintty, WezTerm),
/// macOS (iTerm2, Apple Terminal, Warp, Ghostty, WezTerm, Kitty, Alacritty), and
/// Linux (GNOME Terminal / VTE, Konsole, kitty, foot, alacritty, Tilix, SSH/tmux with
/// forwarded `TERM` or `COLORTERM=truecolor`) use native OpenType / DirectWrite /
/// CoreText / HarfBuzz Arabic shaping. Dumb/legacy consoles (raw `conhost.exe` without
/// Windows Terminal, `TERM=linux`, `TERM=vt100`, `TERM=dumb`) fall back to
/// Presentation Forms-B + BiDi visual reordering.
pub fn detect_native_shaping_from(env: &TerminalShapingEnv<'_>) -> bool {
    // 1. Explicit override has highest priority
    if let Some(val) = env.alphacode_arabic_shaping {
        let v = val.trim().to_ascii_lowercase();
        if matches!(
            v.as_str(),
            "standard" | "unicode" | "native" | "true" | "1" | "on" | "yes"
        ) {
            return true;
        }
        if matches!(
            v.as_str(),
            "presentation" | "forms" | "legacy" | "false" | "0" | "off" | "no"
        ) {
            return false;
        }
    }

    // 2. Explicit dumb/legacy TERM values never have native shaping
    if let Some(term) = env.term
        && is_dumb_legacy_term(term)
    {
        return false;
    }

    // 3. Windows modern terminals (Windows Terminal, ConEmu/Cmder)
    if env_is_present(env.wt_session) || env_is_present(env.conemu_pid) {
        return true;
    }

    // 4. Known modern TERM_PROGRAM values across Windows, macOS, and Linux
    if let Some(prog) = env.term_program {
        let p = prog.trim().to_ascii_lowercase();
        if matches!(
            p.as_str(),
            "vscode"
                | "wezterm"
                | "ghostty"
                | "kitty"
                | "alacritty"
                | "iterm.app"
                | "apple_terminal"
                | "warpterminal"
                | "warp"
                | "mintty"
                | "rio"
                | "hyper"
                | "tabby"
                | "contour"
                | "zed"
        ) {
            return true;
        }
    }

    // 5. Terminal-specific markers (also survive subshells and tmux)
    if env_is_present(env.vte_version)
        || env_is_present(env.gnome_terminal_service)
        || env_is_present(env.konsole_version)
        || env_is_present(env.kitty_pid)
        || env_is_present(env.ghostty_resources_dir)
        || env_is_present(env.wezterm_executable)
        || env_is_present(env.alacritty_window_id)
        || env_is_present(env.iterm_session_id)
        || env_is_present(env.warp_session)
        || env_is_present(env.tilix_id)
        || env_is_present(env.terminator_uuid)
    {
        return true;
    }

    // 6. Modern terminal TERM values (also forwarded across SSH)
    if let Some(term) = env.term {
        let t = term.trim().to_ascii_lowercase();
        if t.contains("kitty")
            || t.contains("alacritty")
            || t.contains("wezterm")
            || t.contains("ghostty")
            || t == "foot"
            || t.starts_with("foot-")
            || t.contains("konsole")
            || t.contains("rio")
            || t.contains("contour")
        {
            return true;
        }
    }

    // 7. Truecolor terminals (e.g. modern Linux/macOS terminals, tmux, SSH with COLORTERM)
    if let Some(ct) = env.colorterm {
        let c = ct.trim().to_ascii_lowercase();
        if matches!(c.as_str(), "truecolor" | "24bit") {
            return true;
        }
    }

    false
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.trim().is_empty())
}

/// Check if the current terminal supports native OpenType / DirectWrite Arabic shaping.
pub fn terminal_supports_native_shaping() -> bool {
    let alphacode_arabic_shaping = non_empty_env("ALPHACODE_ARABIC_SHAPING");
    let wt_session = non_empty_env("WT_SESSION");
    let conemu_pid = non_empty_env("ConEmuPID");
    let term_program = non_empty_env("TERM_PROGRAM");
    let term = non_empty_env("TERM");
    let colorterm = non_empty_env("COLORTERM");
    let vte_version = non_empty_env("VTE_VERSION");
    let konsole_version = non_empty_env("KONSOLE_VERSION")
        .or_else(|| non_empty_env("KONSOLE_DBUS_SERVICE"))
        .or_else(|| non_empty_env("KONSOLE_DBUS_SESSION"));
    let kitty_pid = non_empty_env("KITTY_PID").or_else(|| non_empty_env("KITTY_WINDOW_ID"));
    let ghostty_resources_dir =
        non_empty_env("GHOSTTY_RESOURCES_DIR").or_else(|| non_empty_env("GHOSTTY_BIN_DIR"));
    let wezterm_executable =
        non_empty_env("WEZTERM_EXECUTABLE").or_else(|| non_empty_env("WEZTERM_PANE"));
    let alacritty_window_id =
        non_empty_env("ALACRITTY_WINDOW_ID").or_else(|| non_empty_env("ALACRITTY_SOCKET"));
    let iterm_session_id =
        non_empty_env("ITERM_SESSION_ID").or_else(|| non_empty_env("ITERM_PROFILE"));
    let warp_session =
        non_empty_env("WARP_IS_LOCAL_SHELL_SESSION").or_else(|| non_empty_env("WARP_HONOR_PS1"));
    let gnome_terminal_service =
        non_empty_env("GNOME_TERMINAL_SERVICE").or_else(|| non_empty_env("GNOME_TERMINAL_SCREEN"));
    let tilix_id = non_empty_env("TILIX_ID");
    let terminator_uuid = non_empty_env("TERMINATOR_UUID");

    detect_native_shaping_from(&TerminalShapingEnv {
        alphacode_arabic_shaping: alphacode_arabic_shaping.as_deref(),
        wt_session: wt_session.as_deref(),
        conemu_pid: conemu_pid.as_deref(),
        term_program: term_program.as_deref(),
        term: term.as_deref(),
        colorterm: colorterm.as_deref(),
        vte_version: vte_version.as_deref(),
        konsole_version: konsole_version.as_deref(),
        kitty_pid: kitty_pid.as_deref(),
        ghostty_resources_dir: ghostty_resources_dir.as_deref(),
        wezterm_executable: wezterm_executable.as_deref(),
        alacritty_window_id: alacritty_window_id.as_deref(),
        iterm_session_id: iterm_session_id.as_deref(),
        warp_session: warp_session.as_deref(),
        gnome_terminal_service: gnome_terminal_service.as_deref(),
        tilix_id: tilix_id.as_deref(),
        terminator_uuid: terminator_uuid.as_deref(),
    })
}

/// Determine whether native terminal shaping is currently active.
pub fn is_native_shaping_active() -> bool {
    match get_shaping_mode() {
        ArabicShapingMode::StandardUnicode => true,
        ArabicShapingMode::PresentationForms => false,
        ArabicShapingMode::Auto => terminal_supports_native_shaping(),
    }
}

/// Check if a character belongs to an RTL script (Arabic, Hebrew, Syriac, Thaana, etc.).
pub fn is_rtl_char(c: char) -> bool {
    matches!(
        c as u32,
        0x0590..=0x05FF // Hebrew
        | 0x0600..=0x06FF // Arabic
        | 0x0750..=0x077F // Arabic Supplement
        | 0x08A0..=0x08FF // Arabic Extended-A
        | 0xFB50..=0xFDFF // Arabic Presentation Forms-A
        | 0xFE70..=0xFEFF // Arabic Presentation Forms-B
        | 0x0700..=0x074F // Syriac
        | 0x0780..=0x07BF // Thaana
        | 0x0800..=0x083F // Samaritan
        | 0x0840..=0x085F // Mandaic
    )
}

/// Check if a string contains any RTL characters.
pub fn contains_rtl(text: &str) -> bool {
    text.chars().any(is_rtl_char)
}

/// Check if a Ratatui line contains any RTL characters.
pub fn line_contains_rtl(line: &Line) -> bool {
    line.spans.iter().any(|s| contains_rtl(&s.content))
}

/// Check if a character is Arabic (requires cursive shaping or is an Arabic mark/presentation form).
pub fn is_arabic_char(c: char) -> bool {
    matches!(
        c as u32,
        0x0600..=0x06FF | 0x0750..=0x077F | 0x08A0..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF
    )
}

/// Check if a character is an Arabic transparent mark (tashkeel / harakat / diacritics).
pub fn is_arabic_transparent(c: char) -> bool {
    matches!(
        c as u32,
        0x0610..=0x061A // Honorifics and Quranic marks
        | 0x064B..=0x065F // Fathatan, Dammatan, Kasratan, Fatha, Damma, Kasra, Shadda, Sukun, etc.
        | 0x0670 // Superscript / Dagger Alif
        | 0x06D6..=0x06DC // Quranic annotation marks (excluding U+06DD End of Ayah)
        | 0x06DF..=0x06E8 // Quranic annotation marks (excluding U+06DE Rub El Hizb)
        | 0x06EA..=0x06ED // Quranic annotation marks (excluding U+06E9 Place of Sajdah)
        | 0x08D3..=0x08E1 // Arabic Extended-A marks
        | 0x08E3..=0x08FF // Arabic Extended-A marks
    )
}

/// Decompose compatibility Presentation Forms-A and Forms-B characters to base Arabic characters.
/// This guarantees that text that already contains presentation forms or ligatures can be
/// cleanly, idempotently shaped without disconnecting adjacent letters.
pub fn decompose_presentation_form(c: char) -> &'static str {
    match c {
        // Lam-Alif ligatures (Forms-B)
        '\u{FEF5}' | '\u{FEF6}' => "\u{0644}\u{0622}", // Lam + Madda on Alif
        '\u{FEF7}' | '\u{FEF8}' => "\u{0644}\u{0623}", // Lam + Hamza on Alif
        '\u{FEF9}' | '\u{FEFA}' => "\u{0644}\u{0625}", // Lam + Hamza below Alif
        '\u{FEFB}' | '\u{FEFC}' => "\u{0644}\u{0627}", // Lam + Plain Alif

        // Individual letters (Forms-B)
        '\u{FE80}' => "\u{0621}",              // Hamza
        '\u{FE81}' | '\u{FE82}' => "\u{0622}", // Madda on Alif
        '\u{FE83}' | '\u{FE84}' => "\u{0623}", // Hamza on Alif
        '\u{FE85}' | '\u{FE86}' => "\u{0624}", // Hamza on Waw
        '\u{FE87}' | '\u{FE88}' => "\u{0625}", // Hamza below Alif
        '\u{FE89}'..='\u{FE8C}' => "\u{0626}", // Hamza on Yaa
        '\u{FE8D}' | '\u{FE8E}' => "\u{0627}", // Alif
        '\u{FE8F}'..='\u{FE92}' => "\u{0628}", // Baa
        '\u{FE93}' | '\u{FE94}' => "\u{0629}", // Taa Marbuta
        '\u{FE95}'..='\u{FE98}' => "\u{062A}", // Taa
        '\u{FE99}'..='\u{FE9C}' => "\u{062B}", // Thaa
        '\u{FE9D}'..='\u{FEA0}' => "\u{062C}", // Jeem
        '\u{FEA1}'..='\u{FEA4}' => "\u{062D}", // Haa
        '\u{FEA5}'..='\u{FEA8}' => "\u{062E}", // Khaa
        '\u{FEA9}' | '\u{FEAA}' => "\u{062F}", // Dal
        '\u{FEAB}' | '\u{FEAC}' => "\u{0630}", // Dhal
        '\u{FEAD}' | '\u{FEAE}' => "\u{0631}", // Raa
        '\u{FEAF}' | '\u{FEB0}' => "\u{0632}", // Zay
        '\u{FEB1}'..='\u{FEB4}' => "\u{0633}", // Seen
        '\u{FEB5}'..='\u{FEB8}' => "\u{0634}", // Sheen
        '\u{FEB9}'..='\u{FEBC}' => "\u{0635}", // Saad
        '\u{FEBD}'..='\u{FEC0}' => "\u{0636}", // Daad
        '\u{FEC1}'..='\u{FEC4}' => "\u{0637}", // Tah
        '\u{FEC5}'..='\u{FEC8}' => "\u{0638}", // Zah
        '\u{FEC9}'..='\u{FECC}' => "\u{0639}", // Ayn
        '\u{FECD}'..='\u{FED0}' => "\u{063A}", // Ghayn
        '\u{FED1}'..='\u{FED4}' => "\u{0641}", // Faa
        '\u{FED5}'..='\u{FED8}' => "\u{0642}", // Qaaf
        '\u{FED9}'..='\u{FEDC}' => "\u{0643}", // Kaaf
        '\u{FEDD}'..='\u{FEE0}' => "\u{0644}", // Laam
        '\u{FEE1}'..='\u{FEE4}' => "\u{0645}", // Meem
        '\u{FEE5}'..='\u{FEE8}' => "\u{0646}", // Noon
        '\u{FEE9}'..='\u{FEEC}' => "\u{0647}", // Haa
        '\u{FEED}' | '\u{FEEE}' => "\u{0648}", // Waw
        '\u{FEEF}' | '\u{FEF0}' => "\u{0649}", // Alif Maqsura
        '\u{FEF1}'..='\u{FEF4}' => "\u{064A}", // Yaa

        // Forms-B Tashkeel
        '\u{FE70}' | '\u{FE71}' => "\u{064B}", // Fathatan
        '\u{FE72}' => "\u{064C}",              // Dammatan
        '\u{FE74}' => "\u{064D}",              // Kasratan
        '\u{FE76}' | '\u{FE77}' => "\u{064E}", // Fatha
        '\u{FE78}' | '\u{FE79}' => "\u{064F}", // Damma
        '\u{FE7A}' | '\u{FE7B}' => "\u{0650}", // Kasra
        '\u{FE7C}' | '\u{FE7D}' => "\u{0651}", // Shadda
        '\u{FE7E}' | '\u{FE7F}' => "\u{0652}", // Sukun

        // Extended letters (Forms-A)
        '\u{FB50}' | '\u{FB51}' => "\u{0671}", // Alif Wasla
        '\u{FB56}'..='\u{FB59}' => "\u{067E}", // Peh
        '\u{FB7A}'..='\u{FB7D}' => "\u{0686}", // Tcheh
        '\u{FB8A}' | '\u{FB8B}' => "\u{0698}", // Zhe
        '\u{FB8E}'..='\u{FB91}' => "\u{06A9}", // Keheh
        '\u{FB92}'..='\u{FB95}' => "\u{06AF}", // Gaf
        '\u{FBFC}'..='\u{FBFF}' => "\u{06CC}", // Farsi Yeh
        '\u{FBAE}' | '\u{FBAF}' => "\u{06D2}", // Yeh Barree
        _ => "",
    }
}

/// Normalize all Arabic presentation forms in `text` back to standard Arabic Unicode (U+0600..U+06FF).
pub fn normalize_rtl_text(text: &str) -> Cow<'_, str> {
    if !text
        .chars()
        .any(|c| matches!(c as u32, 0xFB50..=0xFDFF | 0xFE70..=0xFEFF))
    {
        return Cow::Borrowed(text);
    }
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let decomp = decompose_presentation_form(c);
        if decomp.is_empty() {
            out.push(c);
        } else {
            out.push_str(decomp);
        }
    }
    Cow::Owned(out)
}

/// Normalize all Arabic presentation forms in a Ratatui `Line<'static>` back to standard Arabic Unicode.
pub fn normalize_rtl_line(mut line: Line<'static>) -> Line<'static> {
    for span in &mut line.spans {
        if span
            .content
            .chars()
            .any(|c| matches!(c as u32, 0xFB50..=0xFDFF | 0xFE70..=0xFEFF))
        {
            span.content = normalize_rtl_text(&span.content).into_owned().into();
        }
    }
    line
}

/// Detect the natural base direction of a text run according to UAX #9 rules P2/P3.
pub fn detect_base_direction(text: &str) -> Level {
    for c in text.chars() {
        let bidi_class = unicode_bidi::bidi_class(c);
        if matches!(
            bidi_class,
            unicode_bidi::BidiClass::R | unicode_bidi::BidiClass::AL
        ) {
            return Level::rtl();
        }
        if matches!(bidi_class, unicode_bidi::BidiClass::L) {
            return Level::ltr();
        }
    }
    Level::ltr()
}

/// Detect the base direction of a Ratatui `Line`.
pub fn detect_line_base_direction(line: &Line) -> Level {
    for span in &line.spans {
        for c in span.content.chars() {
            let bidi_class = unicode_bidi::bidi_class(c);
            if matches!(
                bidi_class,
                unicode_bidi::BidiClass::R | unicode_bidi::BidiClass::AL
            ) {
                return Level::rtl();
            }
            if matches!(bidi_class, unicode_bidi::BidiClass::L) {
                return Level::ltr();
            }
        }
    }
    Level::ltr()
}

/// Check whether a non-whitespace token looks like a URL, file path, CLI flag,
/// domain/filename, or code identifier (e.g. `src/main.rs`, `https://...`, `foo()`).
fn token_is_technical_identifier(chars: &[char]) -> bool {
    if chars.is_empty() {
        return false;
    }
    let has_slash = chars.iter().any(|&c| c == '/' || c == '\\');
    if has_slash {
        return true;
    }
    if chars.contains(&'_') {
        return true;
    }
    if chars.windows(2).any(|w| w == [':', ':']) {
        return true;
    }
    if chars.len() >= 2 && chars[0] == '-' && (chars[1] == '-' || chars[1].is_ascii_alphabetic()) {
        return true;
    }
    if chars
        .windows(2)
        .any(|w| w[0] == '.' && w[1].is_ascii_alphanumeric())
    {
        return true;
    }
    if chars[0].is_ascii_alphanumeric() && chars.contains(&'(') && chars.contains(&')') {
        return true;
    }
    false
}

fn dominant_direction_from_chars(chars: impl Iterator<Item = char>) -> Level {
    let mut ltr_count = 0usize;
    let mut rtl_count = 0usize;
    let mut first_strong: Option<Level> = None;

    let mut token: Vec<char> = Vec::new();
    let flush_token = |token: &mut Vec<char>,
                       ltr_count: &mut usize,
                       rtl_count: &mut usize,
                       first_strong: &mut Option<Level>| {
        if token.is_empty() {
            return;
        }
        let mut t_ltr = 0usize;
        let mut t_rtl = 0usize;
        for &c in token.iter() {
            let bidi_class = unicode_bidi::bidi_class(c);
            if matches!(
                bidi_class,
                unicode_bidi::BidiClass::R | unicode_bidi::BidiClass::AL
            ) {
                t_rtl += 1;
                if first_strong.is_none() {
                    *first_strong = Some(Level::rtl());
                }
            } else if matches!(bidi_class, unicode_bidi::BidiClass::L) {
                t_ltr += 1;
                if first_strong.is_none() {
                    *first_strong = Some(Level::ltr());
                }
            }
        }
        if token_is_technical_identifier(token) {
            *ltr_count += t_ltr.min(3);
            *rtl_count += t_rtl.min(3);
        } else {
            *ltr_count += t_ltr;
            *rtl_count += t_rtl;
        }
        token.clear();
    };

    for c in chars {
        if c.is_whitespace() {
            flush_token(
                &mut token,
                &mut ltr_count,
                &mut rtl_count,
                &mut first_strong,
            );
        } else {
            token.push(c);
        }
    }
    flush_token(
        &mut token,
        &mut ltr_count,
        &mut rtl_count,
        &mut first_strong,
    );

    if ltr_count > rtl_count {
        Level::ltr()
    } else if rtl_count > ltr_count {
        Level::rtl()
    } else {
        first_strong.unwrap_or_else(Level::ltr)
    }
}

/// Detect the dominant script direction of text by counting strong LTR vs RTL characters.
/// Technical tokens (URLs, file paths, filenames, code identifiers) are capped so an
/// embedded path or URL inside an Arabic sentence does not flip the paragraph to LTR.
///
/// If LTR characters outnumber RTL characters, returns Level::ltr().
/// If RTL characters outnumber LTR characters, returns Level::rtl().
/// If tied or no strong characters, falls back to the first strong character (UAX #9 rules P2/P3),
/// or Level::ltr() if no strong characters exist.
pub fn detect_dominant_direction(text: &str) -> Level {
    dominant_direction_from_chars(text.chars())
}

/// Detect the dominant script direction of a Ratatui `Line`.
pub fn detect_line_dominant_direction(line: &Line) -> Level {
    dominant_direction_from_chars(line.spans.iter().flat_map(|span| span.content.chars()))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum JoiningType {
    None,
    Right, // Connects to preceding (right) character only (e.g. Alif, Dal, Raa, Waw)
    Dual,  // Connects to preceding and following characters (e.g. Baa, Seen, Meem)
}

#[derive(Clone, Copy, Debug)]
struct ArabicForms {
    isolated: char,
    final_form: char,
    initial: char,
    medial: char,
    joining: JoiningType,
}

impl ArabicForms {
    const fn right(isolated: char, final_form: char) -> Self {
        Self {
            isolated,
            final_form,
            initial: isolated,
            medial: final_form,
            joining: JoiningType::Right,
        }
    }

    const fn dual(isolated: char, final_form: char, initial: char, medial: char) -> Self {
        Self {
            isolated,
            final_form,
            initial,
            medial,
            joining: JoiningType::Dual,
        }
    }

    const fn none(isolated: char) -> Self {
        Self {
            isolated,
            final_form: isolated,
            initial: isolated,
            medial: isolated,
            joining: JoiningType::None,
        }
    }
}

fn get_arabic_forms(c: char) -> Option<ArabicForms> {
    match c {
        '\u{0621}' => Some(ArabicForms::none('\u{FE80}')), // Hamza
        '\u{0622}' => Some(ArabicForms::right('\u{FE81}', '\u{FE82}')), // Madda on Alif
        '\u{0623}' => Some(ArabicForms::right('\u{FE83}', '\u{FE84}')), // Hamza on Alif
        '\u{0624}' => Some(ArabicForms::right('\u{FE85}', '\u{FE86}')), // Hamza on Waw
        '\u{0625}' => Some(ArabicForms::right('\u{FE87}', '\u{FE88}')), // Hamza below Alif
        '\u{0626}' => Some(ArabicForms::dual(
            '\u{FE89}', '\u{FE8A}', '\u{FE8B}', '\u{FE8C}',
        )), // Hamza on Yaa
        '\u{0627}' => Some(ArabicForms::right('\u{FE8D}', '\u{FE8E}')), // Plain Alif
        '\u{0628}' => Some(ArabicForms::dual(
            '\u{FE8F}', '\u{FE90}', '\u{FE91}', '\u{FE92}',
        )), // Baa
        '\u{0629}' => Some(ArabicForms::right('\u{FE93}', '\u{FE94}')), // Taa Marbuta
        '\u{062A}' => Some(ArabicForms::dual(
            '\u{FE95}', '\u{FE96}', '\u{FE97}', '\u{FE98}',
        )), // Taa
        '\u{062B}' => Some(ArabicForms::dual(
            '\u{FE99}', '\u{FE9A}', '\u{FE9B}', '\u{FE9C}',
        )), // Thaa
        '\u{062C}' => Some(ArabicForms::dual(
            '\u{FE9D}', '\u{FE9E}', '\u{FE9F}', '\u{FEA0}',
        )), // Jeem
        '\u{062D}' => Some(ArabicForms::dual(
            '\u{FEA1}', '\u{FEA2}', '\u{FEA3}', '\u{FEA4}',
        )), // Haa
        '\u{062E}' => Some(ArabicForms::dual(
            '\u{FEA5}', '\u{FEA6}', '\u{FEA7}', '\u{FEA8}',
        )), // Khaa
        '\u{062F}' => Some(ArabicForms::right('\u{FEA9}', '\u{FEAA}')), // Dal
        '\u{0630}' => Some(ArabicForms::right('\u{FEAB}', '\u{FEAC}')), // Dhal
        '\u{0631}' => Some(ArabicForms::right('\u{FEAD}', '\u{FEAE}')), // Raa
        '\u{0632}' => Some(ArabicForms::right('\u{FEAF}', '\u{FEB0}')), // Zay
        '\u{0633}' => Some(ArabicForms::dual(
            '\u{FEB1}', '\u{FEB2}', '\u{FEB3}', '\u{FEB4}',
        )), // Seen
        '\u{0634}' => Some(ArabicForms::dual(
            '\u{FEB5}', '\u{FEB6}', '\u{FEB7}', '\u{FEB8}',
        )), // Sheen
        '\u{0635}' => Some(ArabicForms::dual(
            '\u{FEB9}', '\u{FEBA}', '\u{FEBB}', '\u{FEBC}',
        )), // Saad
        '\u{0636}' => Some(ArabicForms::dual(
            '\u{FEBD}', '\u{FEBE}', '\u{FEBF}', '\u{FEC0}',
        )), // Daad
        '\u{0637}' => Some(ArabicForms::dual(
            '\u{FEC1}', '\u{FEC2}', '\u{FEC3}', '\u{FEC4}',
        )), // Tah
        '\u{0638}' => Some(ArabicForms::dual(
            '\u{FEC5}', '\u{FEC6}', '\u{FEC7}', '\u{FEC8}',
        )), // Zah
        '\u{0639}' => Some(ArabicForms::dual(
            '\u{FEC9}', '\u{FECA}', '\u{FECB}', '\u{FECC}',
        )), // Ayn
        '\u{063A}' => Some(ArabicForms::dual(
            '\u{FECD}', '\u{FECE}', '\u{FECF}', '\u{FED0}',
        )), // Ghayn
        '\u{0640}' => Some(ArabicForms::dual(
            '\u{0640}', '\u{0640}', '\u{0640}', '\u{0640}',
        )), // Tatweel
        '\u{0641}' => Some(ArabicForms::dual(
            '\u{FED1}', '\u{FED2}', '\u{FED3}', '\u{FED4}',
        )), // Faa
        '\u{0642}' => Some(ArabicForms::dual(
            '\u{FED5}', '\u{FED6}', '\u{FED7}', '\u{FED8}',
        )), // Qaaf
        '\u{0643}' => Some(ArabicForms::dual(
            '\u{FED9}', '\u{FEDA}', '\u{FEDB}', '\u{FEDC}',
        )), // Kaaf
        '\u{0644}' => Some(ArabicForms::dual(
            '\u{FEDD}', '\u{FEDE}', '\u{FEDF}', '\u{FEE0}',
        )), // Laam
        '\u{0645}' => Some(ArabicForms::dual(
            '\u{FEE1}', '\u{FEE2}', '\u{FEE3}', '\u{FEE4}',
        )), // Meem
        '\u{0646}' => Some(ArabicForms::dual(
            '\u{FEE5}', '\u{FEE6}', '\u{FEE7}', '\u{FEE8}',
        )), // Noon
        '\u{0647}' => Some(ArabicForms::dual(
            '\u{FEE9}', '\u{FEEA}', '\u{FEEB}', '\u{FEEC}',
        )), // Haa
        '\u{0648}' => Some(ArabicForms::right('\u{FEED}', '\u{FEEE}')), // Waw
        '\u{0649}' => Some(ArabicForms::right('\u{FEEF}', '\u{FEF0}')), // Alif Maqsura
        '\u{064A}' => Some(ArabicForms::dual(
            '\u{FEF1}', '\u{FEF2}', '\u{FEF3}', '\u{FEF4}',
        )), // Yaa
        '\u{0671}' => Some(ArabicForms::right('\u{FB50}', '\u{FB51}')), // Alif Wasla

        // Persian / Urdu additions
        '\u{067E}' => Some(ArabicForms::dual(
            '\u{FB56}', '\u{FB57}', '\u{FB58}', '\u{FB59}',
        )), // Peh
        '\u{0686}' => Some(ArabicForms::dual(
            '\u{FB7A}', '\u{FB7B}', '\u{FB7C}', '\u{FB7D}',
        )), // Tcheh
        '\u{0698}' => Some(ArabicForms::right('\u{FB8A}', '\u{FB8B}')), // Zhe
        '\u{06AF}' => Some(ArabicForms::dual(
            '\u{FB92}', '\u{FB93}', '\u{FB94}', '\u{FB95}',
        )), // Gaf
        '\u{06A9}' => Some(ArabicForms::dual(
            '\u{FB8E}', '\u{FB8F}', '\u{FB90}', '\u{FB91}',
        )), // Keheh (Farsi Kaf)
        '\u{06CC}' => Some(ArabicForms::dual(
            '\u{FBFC}', '\u{FBFD}', '\u{FBFE}', '\u{FBFF}',
        )), // Farsi Yeh
        '\u{06D2}' => Some(ArabicForms::right('\u{FBAE}', '\u{FBAF}')), // Yeh Barree

        // Zero-width control characters
        '\u{200D}' => Some(ArabicForms::dual(
            '\u{200D}', '\u{200D}', '\u{200D}', '\u{200D}',
        )), // ZWJ: Zero-Width Joiner
        '\u{200C}' => Some(ArabicForms::none('\u{200C}')), // ZWNJ: Zero-Width Non-Joiner

        _ => None,
    }
}

/// Check if two characters form a Lam-Alif ligature.
fn is_lam_alif(lam: char, alif: char) -> Option<(char, char)> {
    if lam != '\u{0644}' {
        return None;
    }
    match alif {
        '\u{0622}' => Some(('\u{FEF5}', '\u{FEF6}')), // Madda on Alif (Isolated, Final)
        '\u{0623}' => Some(('\u{FEF7}', '\u{FEF8}')), // Hamza on Alif
        '\u{0625}' => Some(('\u{FEF9}', '\u{FEFA}')), // Hamza below Alif
        '\u{0627}' | '\u{0671}' => Some(('\u{FEFB}', '\u{FEFC}')), // Plain Alif / Alif Wasla
        _ => None,
    }
}

/// Reshape an Arabic character sequence into presentation forms based on cursive joining context.
/// Normalizes any existing presentation forms first so already-shaped text or ligatures
/// are cleanly and idempotently shaped without disconnecting adjacent letters.
pub fn shape_arabic_chars<T: Clone>(chars_with_meta: &[(char, T)]) -> Vec<(char, T)> {
    // 1. Normalize existing presentation forms to base characters
    let mut normalized = Vec::with_capacity(chars_with_meta.len());
    for (c, meta) in chars_with_meta {
        let decomp = decompose_presentation_form(*c);
        if decomp.is_empty() {
            normalized.push((*c, meta.clone()));
        } else {
            for ch in decomp.chars() {
                normalized.push((ch, meta.clone()));
            }
        }
    }

    let len = normalized.len();
    let mut result: Vec<(char, T)> = Vec::with_capacity(len);
    let mut i = 0;

    while i < len {
        let (ch, meta) = &normalized[i];

        // If not Arabic, pass through as-is
        let forms = match get_arabic_forms(*ch) {
            Some(f) => f,
            None => {
                result.push((*ch, meta.clone()));
                i += 1;
                continue;
            }
        };

        // Check for Lam-Alif ligature
        if *ch == '\u{0644}' {
            // Find next non-transparent character
            let mut j = i + 1;
            while j < len && is_arabic_transparent(normalized[j].0) {
                j += 1;
            }
            if j < len {
                let next_char = normalized[j].0;
                if let Some((iso_la, fin_la)) = is_lam_alif(*ch, next_char) {
                    // Check if previous character connects to Lam
                    let mut prev_connects = false;
                    let mut k = i;
                    while k > 0 {
                        k -= 1;
                        let prev_c = normalized[k].0;
                        if is_arabic_transparent(prev_c) {
                            continue;
                        }
                        if let Some(prev_forms) = get_arabic_forms(prev_c) {
                            prev_connects = prev_forms.joining == JoiningType::Dual;
                        }
                        break;
                    }

                    let ligature = if prev_connects { fin_la } else { iso_la };
                    result.push((ligature, meta.clone()));

                    // Preserve any transparent marks between Lam and Alif
                    result.extend_from_slice(&normalized[(i + 1)..j]);

                    // Preserve any transparent marks immediately following Alif
                    let mut next_k = j + 1;
                    while next_k < len && is_arabic_transparent(normalized[next_k].0) {
                        next_k += 1;
                    }
                    result.extend_from_slice(&normalized[(j + 1)..next_k]);

                    i = next_k;
                    continue;
                }
            }
        }

        // Determine whether previous connects to current
        let mut prev_connects = false;
        let mut k = i;
        while k > 0 {
            k -= 1;
            let prev_c = normalized[k].0;
            if is_arabic_transparent(prev_c) {
                continue;
            }
            if let Some(prev_forms) = get_arabic_forms(prev_c) {
                prev_connects = prev_forms.joining == JoiningType::Dual;
            }
            break;
        }

        // Determine whether next connects to current
        let mut next_connects = false;
        let mut m = i + 1;
        while m < len {
            let next_c = normalized[m].0;
            if is_arabic_transparent(next_c) {
                m += 1;
                continue;
            }
            if let Some(next_forms) = get_arabic_forms(next_c) {
                next_connects = next_forms.joining == JoiningType::Dual
                    || next_forms.joining == JoiningType::Right;
            }
            break;
        }

        // Select presentation form
        let shaped_char = match (prev_connects, next_connects, forms.joining) {
            (true, true, JoiningType::Dual) => forms.medial,
            (false, true, JoiningType::Dual) => forms.initial,
            (true, _, JoiningType::Dual | JoiningType::Right) => forms.final_form,
            _ => forms.isolated,
        };

        result.push((shaped_char, meta.clone()));
        i += 1;
    }

    result
}

/// Mirror common paired characters in RTL context.
pub fn mirror_bidi_char(c: char) -> char {
    match c {
        '(' => ')',
        ')' => '(',
        '[' => ']',
        ']' => '[',
        '{' => '}',
        '}' => '{',
        '<' => '>',
        '>' => '<',
        '«' => '»',
        '»' => '«',
        '‹' => '›',
        '›' => '‹',
        _ => c,
    }
}

/// Shape Arabic characters in a string into Presentation Forms-B without performing visual reordering.
pub fn shape_rtl_text(text: &str) -> String {
    if !contains_rtl(text) {
        return text.to_string();
    }
    let chars_with_idx: Vec<(char, usize)> =
        text.chars().enumerate().map(|(i, c)| (c, i)).collect();
    let shaped = shape_arabic_chars(&chars_with_idx);
    shaped.into_iter().map(|(c, _)| c).collect()
}

/// Shape Arabic characters in a `Line<'static>` into Presentation Forms-B while preserving styles.
pub fn shape_rtl_line(line: Line<'static>) -> Line<'static> {
    if !line_contains_rtl(&line) {
        return line;
    }

    let alignment = line.alignment;
    let mut chars_with_style: Vec<(char, Style)> = Vec::new();
    for span in &line.spans {
        for c in span.content.chars() {
            chars_with_style.push((c, span.style));
        }
    }

    if chars_with_style.is_empty() {
        return line;
    }

    let shaped = shape_arabic_chars(&chars_with_style);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut current_text = String::new();
    let mut current_style: Option<Style> = None;

    for (c, style) in shaped {
        emit_styled_char(&mut spans, &mut current_text, &mut current_style, c, style);
    }
    if !current_text.is_empty() {
        spans.push(Span::styled(
            current_text,
            current_style.unwrap_or_default(),
        ));
    }

    let mut result = Line::from(spans);
    if let Some(align) = alignment {
        result = result.alignment(align);
    }
    result
}

#[inline]
fn emit_styled_char(
    spans: &mut Vec<Span<'static>>,
    current_text: &mut String,
    current_style: &mut Option<Style>,
    c: char,
    style: Style,
) {
    if let Some(cur_s) = *current_style {
        if cur_s == style {
            current_text.push(c);
        } else {
            spans.push(Span::styled(std::mem::take(current_text), cur_s));
            current_text.push(c);
            *current_style = Some(style);
        }
    } else {
        current_text.push(c);
        *current_style = Some(style);
    }
}

const LRI: char = '\u{2066}';
const PDI: char = '\u{2069}';

fn should_isolate_ltr_slice(chars: &[char]) -> bool {
    if chars.is_empty() {
        return false;
    }
    let has_rtl = chars.iter().any(|&c| is_rtl_char(c));
    let has_ltr = chars
        .iter()
        .any(|&c| matches!(unicode_bidi::bidi_class(c), unicode_bidi::BidiClass::L));
    !has_rtl && has_ltr && token_is_technical_identifier(chars)
}

/// Build a sequence with synthetic Unicode directional isolates (`LRI`..`PDI`) around
/// LTR technical tokens (URLs, paths, CLI flags, code identifiers) and LTR code spans
/// so UAX #9 keeps leading/trailing `/`, `.`, `-`, and `()` intact inside RTL prose.
fn build_isolated_bidi_chars(
    chars: &[char],
    is_code_cell: impl Fn(usize) -> bool,
) -> Vec<(char, Option<usize>)> {
    let n = chars.len();
    let mut out = Vec::with_capacity(n + 8);
    let mut i = 0usize;

    while i < n {
        if is_code_cell(i) {
            let start = i;
            while i < n && is_code_cell(i) {
                i += 1;
            }
            let slice = &chars[start..i];
            let has_rtl = slice.iter().any(|&c| is_rtl_char(c));
            let has_ltr = slice
                .iter()
                .any(|&c| matches!(unicode_bidi::bidi_class(c), unicode_bidi::BidiClass::L));
            if !has_rtl && has_ltr {
                out.push((LRI, None));
                for (offset, &c) in slice.iter().enumerate() {
                    out.push((c, Some(start + offset)));
                }
                out.push((PDI, None));
            } else {
                for (offset, &c) in slice.iter().enumerate() {
                    out.push((c, Some(start + offset)));
                }
            }
            continue;
        }

        if chars[i].is_whitespace() {
            out.push((chars[i], Some(i)));
            i += 1;
            continue;
        }

        let tok_start = i;
        while i < n && !chars[i].is_whitespace() && !is_code_cell(i) {
            i += 1;
        }
        let tok_end = i;
        let token = &chars[tok_start..tok_end];

        // Keep sentence punctuation and outer/unmatched brackets or quotes outside
        // the isolate so Arabic prose brackets like `(نسخة الـ TUI/CLI).` pair
        // properly at the paragraph level, while balanced tokens like `foo()` stay
        // intact inside the isolate.
        let mut inner_start = 0usize;
        let mut inner_end = token.len();
        while inner_end > inner_start
            && matches!(
                token[inner_end - 1],
                '.' | ',' | '،' | '؛' | ':' | '!' | '?' | '؟'
            )
        {
            inner_end -= 1;
        }
        while inner_end - inner_start >= 2 {
            let first = token[inner_start];
            let last = token[inner_end - 1];
            if matches!(
                (first, last),
                ('(', ')') | ('[', ']') | ('{', '}') | ('"', '"') | ('\'', '\'') | ('<', '>')
            ) {
                inner_start += 1;
                inner_end -= 1;
            } else {
                break;
            }
        }
        loop {
            if inner_start >= inner_end {
                break;
            }
            let slice = &token[inner_start..inner_end];
            let peel_left = match slice[0] {
                '(' => {
                    slice.iter().filter(|&&c| c == '(').count()
                        > slice.iter().filter(|&&c| c == ')').count()
                }
                '[' => {
                    slice.iter().filter(|&&c| c == '[').count()
                        > slice.iter().filter(|&&c| c == ']').count()
                }
                '{' => {
                    slice.iter().filter(|&&c| c == '{').count()
                        > slice.iter().filter(|&&c| c == '}').count()
                }
                '"' => slice.iter().filter(|&&c| c == '"').count() % 2 == 1,
                '\'' => slice.iter().filter(|&&c| c == '\'').count() % 2 == 1,
                _ => false,
            };
            if peel_left {
                inner_start += 1;
                continue;
            }
            let peel_right = match slice[slice.len() - 1] {
                ')' => {
                    slice.iter().filter(|&&c| c == ')').count()
                        > slice.iter().filter(|&&c| c == '(').count()
                }
                ']' => {
                    slice.iter().filter(|&&c| c == ']').count()
                        > slice.iter().filter(|&&c| c == '[').count()
                }
                '}' => {
                    slice.iter().filter(|&&c| c == '}').count()
                        > slice.iter().filter(|&&c| c == '{').count()
                }
                '"' => slice.iter().filter(|&&c| c == '"').count() % 2 == 1,
                '\'' => slice.iter().filter(|&&c| c == '\'').count() % 2 == 1,
                _ => false,
            };
            if peel_right {
                inner_end -= 1;
                continue;
            }
            break;
        }

        if should_isolate_ltr_slice(&token[inner_start..inner_end]) {
            for (offset, &c) in token.iter().enumerate().take(inner_start) {
                out.push((c, Some(tok_start + offset)));
            }
            out.push((LRI, None));
            for (offset, &c) in token.iter().enumerate().take(inner_end).skip(inner_start) {
                out.push((c, Some(tok_start + offset)));
            }
            out.push((PDI, None));
            for (offset, &c) in token.iter().enumerate().skip(inner_end) {
                out.push((c, Some(tok_start + offset)));
            }
        } else {
            for (offset, &c) in token.iter().enumerate() {
                out.push((c, Some(tok_start + offset)));
            }
        }
    }

    out
}

/// Perform BiDi visual reordering on an already-shaped `Line<'static>` using the given base level.
/// Extended grapheme clusters (base characters followed by diacritics/tashkeel) are preserved
/// in storage order during reversal so marks attach properly in terminal cell buffers.
pub fn reorder_rtl_line_with_base(line: Line<'static>, base_level: Option<Level>) -> Line<'static> {
    if !line_contains_rtl(&line) {
        return line;
    }

    let alignment = line.alignment;
    let mut chars_with_style: Vec<(char, Style)> = Vec::new();
    for span in &line.spans {
        for c in span.content.chars() {
            chars_with_style.push((c, span.style));
        }
    }

    if chars_with_style.is_empty() {
        return line;
    }

    let raw_chars: Vec<char> = chars_with_style.iter().map(|(c, _)| *c).collect();
    let bidi_chars =
        build_isolated_bidi_chars(&raw_chars, |idx| chars_with_style[idx].1.bg.is_some());
    let bidi_str: String = bidi_chars.iter().map(|(c, _)| *c).collect();
    let base = base_level.or_else(|| Some(detect_line_dominant_direction(&line)));
    let bidi_info = BidiInfo::new(&bidi_str, base);

    let mut byte_to_bidi_idx = Vec::with_capacity(bidi_str.len());
    for (bidi_idx, (c, _)) in bidi_chars.iter().enumerate() {
        for _ in 0..c.len_utf8() {
            byte_to_bidi_idx.push(bidi_idx);
        }
    }

    let mut reordered_spans: Vec<Span<'static>> = Vec::new();
    let mut current_text = String::new();
    let mut current_style: Option<Style> = None;

    for para in &bidi_info.paragraphs {
        let (levels, runs) = bidi_info.visual_runs(para, para.range.clone());

        for run in runs {
            let is_rtl_run = levels[run.start].is_rtl();
            let run_str = &bidi_str[run.clone()];

            if is_rtl_run {
                // Group run characters into grapheme clusters (base char + combining marks).
                // In an RTL run, we reverse the ORDER OF CLUSTERS, but KEEP COMBINING MARKS
                // IN STORAGE ORDER (following their base character) so the terminal attaches them properly.
                let mut clusters: Vec<Vec<(char, Style)>> = Vec::new();
                for (byte_offset, c) in run_str.char_indices() {
                    let abs_byte = run.start + byte_offset;
                    let Some(char_idx) = bidi_chars[byte_to_bidi_idx[abs_byte]].1 else {
                        continue;
                    };
                    let style = chars_with_style[char_idx].1;

                    if is_arabic_transparent(c)
                        && let Some(last) = clusters.last_mut()
                    {
                        last.push((c, style));
                    } else {
                        let displayed_c = mirror_bidi_char(c);
                        clusters.push(vec![(displayed_c, style)]);
                    }
                }

                // Emit clusters in reverse visual order
                for cluster in clusters.into_iter().rev() {
                    for (c, style) in cluster {
                        emit_styled_char(
                            &mut reordered_spans,
                            &mut current_text,
                            &mut current_style,
                            c,
                            style,
                        );
                    }
                }
            } else {
                // LTR run: emit in natural forward order
                for (byte_offset, c) in run_str.char_indices() {
                    let abs_byte = run.start + byte_offset;
                    let Some(char_idx) = bidi_chars[byte_to_bidi_idx[abs_byte]].1 else {
                        continue;
                    };
                    let style = chars_with_style[char_idx].1;
                    emit_styled_char(
                        &mut reordered_spans,
                        &mut current_text,
                        &mut current_style,
                        c,
                        style,
                    );
                }
            }
        }
    }

    if !current_text.is_empty() {
        let s = current_style.unwrap_or_default();
        reordered_spans.push(Span::styled(current_text, s));
    }

    let mut result_line = Line::from(reordered_spans);
    if let Some(align) = alignment {
        result_line = result_line.alignment(align);
    }
    result_line
}

/// Reorder a slice of lines using the given base level.
pub fn reorder_rtl_lines_with_base(
    lines: Vec<Line<'static>>,
    base_level: Option<Level>,
) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .map(|l| reorder_rtl_line_with_base(l, base_level))
        .collect()
}

#[derive(Debug, Clone)]
struct VisualCluster {
    text: String,
    vis_start_col: usize,
    vis_width: usize,
    orig_start_byte: usize,
    orig_end_byte: usize,
    is_rtl: bool,
}

/// Shape a single logical line (no `\n`) and compute its visual clusters along with
/// each cluster's display column and original byte range in `line`.
fn prepare_rtl_visual_clusters(
    line: &str,
    base_level: Option<Level>,
) -> (Vec<VisualCluster>, Level) {
    use unicode_width::UnicodeWidthStr;

    if line.is_empty() {
        return (Vec::new(), base_level.unwrap_or_else(Level::ltr));
    }

    let chars_with_ranges: Vec<(char, (usize, usize))> = line
        .char_indices()
        .map(|(start, c)| (c, (start, start + c.len_utf8())))
        .collect();
    let shaped = shape_arabic_chars(&chars_with_ranges);
    let shaped_chars: Vec<char> = shaped.iter().map(|(c, _)| *c).collect();
    let bidi_chars = build_isolated_bidi_chars(&shaped_chars, |_| false);
    let bidi_str: String = bidi_chars.iter().map(|(c, _)| *c).collect();

    let resolved_base = base_level.unwrap_or_else(|| detect_dominant_direction(line));
    let bidi_info = BidiInfo::new(&bidi_str, Some(resolved_base));

    let mut byte_to_bidi_idx = Vec::with_capacity(bidi_str.len());
    for (bidi_idx, (c, _)) in bidi_chars.iter().enumerate() {
        for _ in 0..c.len_utf8() {
            byte_to_bidi_idx.push(bidi_idx);
        }
    }

    let mut visual_clusters: Vec<VisualCluster> = Vec::with_capacity(shaped.len());
    let mut vis_col = 0usize;

    for para in &bidi_info.paragraphs {
        let (levels, runs) = bidi_info.visual_runs(para, para.range.clone());

        for run in runs {
            let is_rtl_run = levels[run.start].is_rtl();
            let run_str = &bidi_str[run.clone()];

            if is_rtl_run {
                let mut clusters: Vec<(String, usize, usize)> = Vec::new();
                for (byte_offset, c) in run_str.char_indices() {
                    let abs_byte = run.start + byte_offset;
                    let Some(shaped_idx) = bidi_chars[byte_to_bidi_idx[abs_byte]].1 else {
                        continue;
                    };
                    let (orig_start, orig_end) = shaped[shaped_idx].1;

                    if is_arabic_transparent(c)
                        && let Some(last) = clusters.last_mut()
                    {
                        last.0.push(c);
                        last.1 = last.1.min(orig_start);
                        last.2 = last.2.max(orig_end);
                    } else {
                        let mut s = String::new();
                        s.push(mirror_bidi_char(c));
                        clusters.push((s, orig_start, orig_end));
                    }
                }

                for (text, orig_start_byte, orig_end_byte) in clusters.into_iter().rev() {
                    let vis_width = UnicodeWidthStr::width(text.as_str());
                    visual_clusters.push(VisualCluster {
                        text,
                        vis_start_col: vis_col,
                        vis_width,
                        orig_start_byte,
                        orig_end_byte,
                        is_rtl: true,
                    });
                    vis_col += vis_width;
                }
            } else {
                let mut clusters: Vec<(String, usize, usize)> = Vec::new();
                for (byte_offset, c) in run_str.char_indices() {
                    let abs_byte = run.start + byte_offset;
                    let Some(shaped_idx) = bidi_chars[byte_to_bidi_idx[abs_byte]].1 else {
                        continue;
                    };
                    let (orig_start, orig_end) = shaped[shaped_idx].1;

                    if is_arabic_transparent(c)
                        && let Some(last) = clusters.last_mut()
                    {
                        last.0.push(c);
                        last.1 = last.1.min(orig_start);
                        last.2 = last.2.max(orig_end);
                    } else {
                        let mut s = String::new();
                        s.push(c);
                        clusters.push((s, orig_start, orig_end));
                    }
                }

                for (text, orig_start_byte, orig_end_byte) in clusters {
                    let vis_width = UnicodeWidthStr::width(text.as_str());
                    visual_clusters.push(VisualCluster {
                        text,
                        vis_start_col: vis_col,
                        vis_width,
                        orig_start_byte,
                        orig_end_byte,
                        is_rtl: false,
                    });
                    vis_col += vis_width;
                }
            }
        }
    }

    (visual_clusters, resolved_base)
}

/// Measure terminal cell width by summing extended grapheme cluster widths.
/// `unicode-width` 0.2 treats Lam-Alif (`"لا"`) as width 1 on a whole `&str`,
/// whereas terminal cell grids and Ratatui place one cell per grapheme cluster.
#[inline]
fn cell_display_width(text: &str) -> usize {
    use unicode_segmentation::UnicodeSegmentation;
    use unicode_width::UnicodeWidthStr;
    text.graphemes(true).map(UnicodeWidthStr::width).sum()
}

/// Compute the visual terminal column for a logical `byte_offset` inside a single line `text`,
/// matching the output of [`prepare_rtl_text`].
pub fn visual_cursor_col_for_text(text: &str, byte_offset: usize) -> usize {
    let clamped = byte_offset.min(text.len());
    let mut safe_byte = clamped;
    while safe_byte > 0 && !text.is_char_boundary(safe_byte) {
        safe_byte -= 1;
    }

    if !contains_rtl(text) {
        return cell_display_width(&text[..safe_byte]);
    }

    if is_native_shaping_active() {
        let prefix = normalize_rtl_text(&text[..safe_byte]);
        return cell_display_width(prefix.as_ref());
    }

    let (clusters, base) = prepare_rtl_visual_clusters(text, None);
    if clusters.is_empty() {
        return 0;
    }

    // Cursor at end of line: sit at the trailing edge of the logically last cluster
    if safe_byte >= text.len() {
        if let Some(last_logical) = clusters.iter().max_by_key(|c| c.orig_end_byte) {
            return if last_logical.is_rtl {
                last_logical.vis_start_col
            } else {
                last_logical.vis_start_col + last_logical.vis_width
            };
        }
        return if base.is_rtl() {
            0
        } else {
            clusters.iter().map(|c| c.vis_width).sum()
        };
    }

    // Find the cluster containing or starting at `safe_byte`
    if let Some(cluster) = clusters
        .iter()
        .find(|c| safe_byte >= c.orig_start_byte && safe_byte < c.orig_end_byte)
    {
        return if safe_byte == cluster.orig_start_byte {
            if cluster.is_rtl {
                cluster.vis_start_col + cluster.vis_width
            } else {
                cluster.vis_start_col
            }
        } else {
            // Inside a multi-codepoint cluster (e.g. Lam-Alif or base+tashkeel):
            // place caret at its trailing edge.
            if cluster.is_rtl {
                cluster.vis_start_col
            } else {
                cluster.vis_start_col + cluster.vis_width
            }
        };
    }

    // Fallback: after the closest preceding cluster
    if let Some(prev) = clusters
        .iter()
        .filter(|c| c.orig_end_byte <= safe_byte)
        .max_by_key(|c| c.orig_end_byte)
    {
        return if prev.is_rtl {
            prev.vis_start_col
        } else {
            prev.vis_start_col + prev.vis_width
        };
    }

    0
}

/// Map a visual terminal column in a prepared single line `text` back to a logical
/// UTF-8 byte offset in `text` (for mouse click hit-testing in the composer).
pub fn byte_offset_for_visual_col_in_text(text: &str, target_col: usize) -> usize {
    use unicode_segmentation::UnicodeSegmentation;

    if text.is_empty() {
        return 0;
    }

    if !contains_rtl(text) || is_native_shaping_active() {
        let mut col = 0usize;
        let mut byte_idx = 0usize;
        for grapheme in text.graphemes(true) {
            let cw = if is_native_shaping_active() {
                cell_display_width(normalize_rtl_text(grapheme).as_ref())
            } else {
                cell_display_width(grapheme)
            };
            if cw == 0 {
                byte_idx += grapheme.len();
                continue;
            }
            if target_col < col + cw {
                if (target_col - col).saturating_mul(2) >= cw {
                    return byte_idx + grapheme.len();
                }
                return byte_idx;
            }
            col += cw;
            byte_idx += grapheme.len();
        }
        return text.len();
    }

    let (clusters, base) = prepare_rtl_visual_clusters(text, None);
    if clusters.is_empty() {
        return 0;
    }
    let total_width: usize = clusters.iter().map(|c| c.vis_width).sum();
    if target_col >= total_width {
        return if base.is_rtl() { 0 } else { text.len() };
    }

    for cluster in &clusters {
        if cluster.vis_width == 0 {
            continue;
        }
        let end_col = cluster.vis_start_col + cluster.vis_width;
        if target_col < end_col {
            let right_half =
                (target_col - cluster.vis_start_col).saturating_mul(2) >= cluster.vis_width;
            return if cluster.is_rtl {
                if right_half {
                    cluster.orig_start_byte
                } else {
                    cluster.orig_end_byte
                }
            } else if right_half {
                cluster.orig_end_byte
            } else {
                cluster.orig_start_byte
            };
        }
    }

    if base.is_rtl() { 0 } else { text.len() }
}

/// Prepare a plain text string containing Arabic / RTL for terminal output with a specified base level.
/// In modern terminals (or StandardUnicode mode), preserves standard Arabic characters (U+0600..U+06FF)
/// and normalizes any presentation forms, allowing the terminal's native OpenType/DirectWrite engine
/// to connect cursive letters naturally without fixed-cell gaps.
/// In legacy presentation forms mode, performs contextual shaping and BiDi visual reordering.
pub fn prepare_rtl_text_with_base(text: &str, base_level: Option<Level>) -> Cow<'_, str> {
    if !contains_rtl(text) {
        return Cow::Borrowed(text);
    }

    if is_native_shaping_active() {
        return normalize_rtl_text(text);
    }

    prepare_rtl_text_presentation(text, base_level)
}

/// Force standard Unicode preparation, preserving standard Arabic characters (U+0600..U+06FF).
pub fn prepare_rtl_text_unicode(text: &str) -> Cow<'_, str> {
    if !contains_rtl(text) {
        Cow::Borrowed(text)
    } else {
        normalize_rtl_text(text)
    }
}

/// Force preparation using Presentation Forms-B for legacy dumb terminals.
pub fn prepare_rtl_text_presentation(text: &str, base_level: Option<Level>) -> Cow<'_, str> {
    if !contains_rtl(text) {
        return Cow::Borrowed(text);
    }

    let mut out = String::with_capacity(text.len());
    let mut first = true;

    for line in text.split('\n') {
        if !first {
            out.push('\n');
        }
        first = false;

        if !contains_rtl(line) {
            out.push_str(line);
            continue;
        }

        let (clusters, _) = prepare_rtl_visual_clusters(line, base_level);
        for cluster in clusters {
            out.push_str(&cluster.text);
        }
    }

    Cow::Owned(out)
}

/// Prepare a plain text string containing Arabic / RTL for terminal output.
/// If `text` contains no RTL characters, it is returned borrowed without allocation.
pub fn prepare_rtl_text(text: &str) -> Cow<'_, str> {
    prepare_rtl_text_with_base(text, None)
}

/// Prepare a Ratatui `Line<'static>` containing Arabic / RTL for terminal rendering with an explicit base level.
pub fn prepare_rtl_line_with_base(line: Line<'static>, base_level: Option<Level>) -> Line<'static> {
    if !line_contains_rtl(&line) {
        return line;
    }

    if is_native_shaping_active() {
        return normalize_rtl_line(line);
    }

    prepare_rtl_line_presentation(line, base_level)
}

/// Force preparation using Presentation Forms-B on a Ratatui `Line<'static>` for legacy dumb terminals.
pub fn prepare_rtl_line_presentation(
    line: Line<'static>,
    base_level: Option<Level>,
) -> Line<'static> {
    if !line_contains_rtl(&line) {
        return line;
    }
    let shaped = shape_rtl_line(line);
    let base = base_level.or_else(|| Some(detect_line_dominant_direction(&shaped)));
    reorder_rtl_line_with_base(shaped, base)
}

/// Force standard Unicode preparation on a Ratatui `Line<'static>`.
pub fn prepare_rtl_line_unicode(line: Line<'static>) -> Line<'static> {
    if !line_contains_rtl(&line) {
        line
    } else {
        normalize_rtl_line(line)
    }
}

/// Prepare a Ratatui `Line<'static>` containing Arabic / RTL for terminal rendering.
pub fn prepare_rtl_line(line: Line<'static>) -> Line<'static> {
    prepare_rtl_line_with_base(line, None)
}

/// Prepare a slice of Ratatui lines for RTL display.
pub fn prepare_rtl_lines(lines: Vec<Line<'static>>) -> Vec<Line<'static>> {
    lines.into_iter().map(prepare_rtl_line).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_rtl_detection() {
        assert!(is_rtl_char('م'));
        assert!(is_rtl_char('א'));
        assert!(!is_rtl_char('a'));
        assert!(!is_rtl_char('1'));
        assert!(contains_rtl("مرحبا بك"));
        assert!(!contains_rtl("Hello world!"));
    }

    #[test]
    fn test_arabic_shaping_marhaba() {
        // "مرحبا" -> Meem (initial), Raa (final), Haa (initial), Baa (medial), Alif (final)
        let chars: Vec<(char, ())> = "مرحبا".chars().map(|c| (c, ())).collect();
        let shaped = shape_arabic_chars(&chars);
        let shaped_str: String = shaped.iter().map(|(c, _)| *c).collect();

        let expected = "\u{FEE3}\u{FEAE}\u{FEA3}\u{FE92}\u{FE8E}"; // ﻣﺮﺣﺒﺎ
        assert_eq!(shaped_str, expected);
    }

    #[test]
    fn test_lam_alif_ligature() {
        // "لا" -> Lam-Alif isolated: \u{FEFB}
        let chars: Vec<(char, ())> = "لا".chars().map(|c| (c, ())).collect();
        let shaped = shape_arabic_chars(&chars);
        assert_eq!(shaped.len(), 1);
        assert_eq!(shaped[0].0, '\u{FEFB}'); // ﻻ
    }

    #[test]
    fn test_lam_alif_variants() {
        // Alif Madda: \u{FEF5}
        let chars_madda: Vec<(char, ())> = "لآ".chars().map(|c| (c, ())).collect();
        let shaped_madda = shape_arabic_chars(&chars_madda);
        assert_eq!(shaped_madda[0].0, '\u{FEF5}');

        // Alif Hamza above: \u{FEF7}
        let chars_hamza_above: Vec<(char, ())> = "لأ".chars().map(|c| (c, ())).collect();
        let shaped_ha = shape_arabic_chars(&chars_hamza_above);
        assert_eq!(shaped_ha[0].0, '\u{FEF7}');

        // Alif Hamza below: \u{FEF9}
        let chars_hamza_below: Vec<(char, ())> = "لإ".chars().map(|c| (c, ())).collect();
        let shaped_hb = shape_arabic_chars(&chars_hamza_below);
        assert_eq!(shaped_hb[0].0, '\u{FEF9}');
    }

    #[test]
    fn test_tashkeel_cluster_preservation() {
        with_arabic_shaping_mode(ArabicShapingMode::PresentationForms, || {
            // "مَرْحَبًا" with tashkeel: Meem+Fatha, Raa+Sukun, Haa+Fatha, Baa+Fathatan, Alif
            let text = "مَرْحَبًا";
            let prepared = prepare_rtl_text(text);
            let rendered = prepared.as_ref();

            // In the visually reversed string for LTR terminal rendering:
            // Diacritics must follow their respective base characters in storage order, not precede them.
            let raa_pos = rendered.find('\u{FEAE}').expect("Raa final present");
            let sukun_pos = rendered.find('\u{0652}').expect("Sukun present");
            assert!(sukun_pos > raa_pos, "Sukun must immediately follow Raa");

            let meem_pos = rendered.find('\u{FEE3}').expect("Meem initial present");
            let fatha_pos = rendered.rfind('\u{064E}').expect("Fatha present");
            assert!(fatha_pos > meem_pos, "Fatha must immediately follow Meem");
        });
    }

    #[test]
    fn test_prepare_rtl_text_arabic_presentation() {
        with_arabic_shaping_mode(ArabicShapingMode::PresentationForms, || {
            let text = "مرحبا";
            let prepared = prepare_rtl_text(text);
            assert_ne!(prepared.as_ref(), text);
            assert!(prepared.contains('\u{FEE3}')); // Meem initial
            assert!(prepared.contains('\u{FE8E}')); // Alif final
        });
    }

    #[test]
    fn test_prepare_rtl_text_standard_unicode() {
        with_arabic_shaping_mode(ArabicShapingMode::StandardUnicode, || {
            // Standard Arabic should be preserved without conversion to Presentation Forms
            let text = "مرحبا بك";
            let prepared = prepare_rtl_text(text);
            assert_eq!(prepared.as_ref(), "مرحبا بك");
            assert!(!prepared.contains('\u{FEE3}')); // No Presentation Forms
            assert!(!prepared.contains('\u{FE8E}'));

            // Decomposes existing presentation forms to base Arabic
            let pasted_forms = "\u{FEE3}\u{FEAE}\u{FEA3}\u{FE92}\u{FE8E}"; // ﻣﺮﺣﺒﺎ
            let decomp = prepare_rtl_text(pasted_forms);
            assert_eq!(decomp.as_ref(), "مرحبا");
        });
    }

    #[test]
    fn test_dominant_direction_mixed_english_arabic() {
        // Line with mostly English and some Arabic: dominant direction should be LTR
        let text = "Alphacode, can you explain? أنا أتساءل";
        assert_eq!(detect_dominant_direction(text), Level::ltr());

        // In PresentationForms mode, the English question with trailing '?' must stay intact
        // and '?' must not be flipped to the start of the line.
        with_arabic_shaping_mode(ArabicShapingMode::PresentationForms, || {
            let prepared = prepare_rtl_text(text);
            let rendered = prepared.as_ref();
            assert!(
                rendered.contains("Alphacode, can you explain?"),
                "English question with ? must remain intact: got '{}'",
                rendered
            );
            assert!(
                !rendered.starts_with('?'),
                "Question mark must NOT be flipped to the start: got '{}'",
                rendered
            );
        });

        // In StandardUnicode mode, characters stay in logical order
        with_arabic_shaping_mode(ArabicShapingMode::StandardUnicode, || {
            let prepared = prepare_rtl_text(text);
            assert_eq!(prepared.as_ref(), text);
        });
    }

    #[test]
    fn test_dominant_direction_arabic_dominant() {
        // Line with mostly Arabic and some English: dominant direction should be RTL
        let text = "هل يمكنك شرح كود Alphacode في هذا الملف؟";
        assert_eq!(detect_dominant_direction(text), Level::rtl());
    }

    #[test]
    fn test_prepare_rtl_text_english_stays_unchanged() {
        let text = "Hello world 123!";
        let prepared = prepare_rtl_text(text);
        assert_eq!(prepared.as_ref(), text);
        assert!(matches!(prepared, Cow::Borrowed(_)));
    }

    #[test]
    fn test_prepare_rtl_line_preserves_styles() {
        let style_arabic = Style::default().bold();
        let style_english = Style::default().italic();
        let line = Line::from(vec![
            Span::styled("مرحبا ", style_arabic),
            Span::styled("world", style_english),
        ]);
        let prepared = prepare_rtl_line(line);
        assert!(!prepared.spans.is_empty());
        assert!(prepared.spans.iter().any(|s| s.style == style_arabic));
        assert!(prepared.spans.iter().any(|s| s.style == style_english));
    }

    #[test]
    fn test_bidi_mirroring() {
        let text = "(مرحبا)";
        let prepared = prepare_rtl_text(text);
        assert!(prepared.starts_with('(') || prepared.ends_with(')'));
    }

    #[test]
    fn test_numbers_stay_ltr_in_rtl() {
        let text = "في عام 2026 تم الإطلاق";
        let prepared = prepare_rtl_text(text);
        // The digits '2026' should remain in '2026' order, NOT '6202'
        assert!(prepared.contains("2026"));
    }

    #[test]
    fn test_cross_platform_terminal_native_shaping_detection() {
        // Windows: Windows Terminal, ConEmu, VS Code, Mintty, WezTerm
        assert!(detect_native_shaping_from(&TerminalShapingEnv {
            wt_session: Some("1234-5678"),
            ..Default::default()
        }));
        assert!(detect_native_shaping_from(&TerminalShapingEnv {
            conemu_pid: Some("9420"),
            ..Default::default()
        }));
        assert!(detect_native_shaping_from(&TerminalShapingEnv {
            term_program: Some("vscode"),
            ..Default::default()
        }));
        assert!(detect_native_shaping_from(&TerminalShapingEnv {
            term_program: Some("mintty"),
            ..Default::default()
        }));

        // Raw Windows conhost.exe (no WT_SESSION, no ConEmuPID, no TERM_PROGRAM, no TERM)
        assert!(!detect_native_shaping_from(&TerminalShapingEnv::default()));

        // macOS: iTerm.app, Apple_Terminal, WarpTerminal, Ghostty, WezTerm
        for prog in [
            "iTerm.app",
            "Apple_Terminal",
            "WarpTerminal",
            "ghostty",
            "WezTerm",
        ] {
            assert!(
                detect_native_shaping_from(&TerminalShapingEnv {
                    term_program: Some(prog),
                    term: Some("xterm-256color"),
                    ..Default::default()
                }),
                "macOS TERM_PROGRAM={prog} should enable native shaping"
            );
        }

        // Linux: GNOME Terminal / VTE, Konsole, kitty, foot, alacritty, Tilix
        assert!(detect_native_shaping_from(&TerminalShapingEnv {
            vte_version: Some("7600"),
            term: Some("xterm-256color"),
            ..Default::default()
        }));
        assert!(detect_native_shaping_from(&TerminalShapingEnv {
            konsole_version: Some("240200"),
            term: Some("xterm-256color"),
            ..Default::default()
        }));
        for term in [
            "xterm-kitty",
            "foot",
            "foot-extra",
            "alacritty",
            "xterm-ghostty",
            "wezterm",
        ] {
            assert!(
                detect_native_shaping_from(&TerminalShapingEnv {
                    term: Some(term),
                    ..Default::default()
                }),
                "Linux TERM={term} should enable native shaping"
            );
        }

        // SSH / tmux forwarding COLORTERM=truecolor or inheriting outer env
        assert!(detect_native_shaping_from(&TerminalShapingEnv {
            term: Some("tmux-256color"),
            colorterm: Some("truecolor"),
            ..Default::default()
        }));
        assert!(detect_native_shaping_from(&TerminalShapingEnv {
            term: Some("tmux-256color"),
            wt_session: Some("abc"),
            ..Default::default()
        }));

        // Dumb / legacy terminals fall back to Presentation Forms-B
        for legacy_term in ["linux", "vt100", "vt220", "dumb", "ansi"] {
            assert!(
                !detect_native_shaping_from(&TerminalShapingEnv {
                    term: Some(legacy_term),
                    colorterm: Some("truecolor"),
                    ..Default::default()
                }),
                "Legacy TERM={legacy_term} must fall back to presentation forms"
            );
        }

        // Explicit ALPHACODE_ARABIC_SHAPING overrides everything
        assert!(detect_native_shaping_from(&TerminalShapingEnv {
            alphacode_arabic_shaping: Some("standard"),
            term: Some("vt100"),
            ..Default::default()
        }));
        assert!(!detect_native_shaping_from(&TerminalShapingEnv {
            alphacode_arabic_shaping: Some("presentation"),
            wt_session: Some("1234"),
            ..Default::default()
        }));
    }

    #[test]
    fn test_lam_alif_with_tashkeel_and_arabic_indic_digits() {
        with_arabic_shaping_mode(ArabicShapingMode::PresentationForms, || {
            // Lam + Fatha + Alif ("لَا") and connected Baa + Kasra + Lam + Fatha + Alif ("بِلَا")
            let la_fatha = "لَا";
            let shaped = shape_rtl_text(la_fatha);
            assert!(
                shaped.contains('\u{FEFB}'),
                "expected isolated Lam-Alif in {shaped:?}"
            );
            assert!(
                shaped.contains('\u{064E}'),
                "expected Fatha preserved in {shaped:?}"
            );

            let bila = "بِلَا";
            let shaped_bila = shape_rtl_text(bila);
            assert!(
                shaped_bila.contains('\u{FE91}'),
                "expected initial Baa in {shaped_bila:?}"
            );
            assert!(
                shaped_bila.contains('\u{FEFC}'),
                "expected final Lam-Alif in {shaped_bila:?}"
            );
            assert!(
                shaped_bila.contains('\u{0650}'),
                "expected Kasra in {shaped_bila:?}"
            );
            assert!(
                shaped_bila.contains('\u{064E}'),
                "expected Fatha in {shaped_bila:?}"
            );

            // Visual order keeps tashkeel after its ligature
            let prepared = prepare_rtl_text("لَا إله");
            let lig_pos = prepared.find('\u{FEFB}').expect("Lam-Alif ligature");
            let fatha_pos = prepared.find('\u{064E}').expect("Fatha mark");
            assert!(fatha_pos > lig_pos);

            // Arabic-Indic digits stay LTR
            let indic = prepare_rtl_text("الإصدار ١٢٣.٤٥ في ٢٠٢٦");
            assert!(indic.contains("١٢٣.٤٥"), "got {indic:?}");
            assert!(indic.contains("٢٠٢٦"), "got {indic:?}");
        });
    }

    #[test]
    fn test_urls_paths_flags_and_code_inside_arabic_sentences() {
        // Embedded long path/URL must NOT flip an Arabic sentence to LTR
        let path_sentence = "راجع الملف src/alphacode_render_core/bidi.rs الآن";
        assert_eq!(detect_dominant_direction(path_sentence), Level::rtl());

        let url_sentence = "افتح الرابط https://example.com/docs/ar?q=1&b=2 للتفاصيل";
        assert_eq!(detect_dominant_direction(url_sentence), Level::rtl());

        with_arabic_shaping_mode(ArabicShapingMode::PresentationForms, || {
            let path_out = prepare_rtl_text("افتح /etc/hosts الآن");
            assert!(
                path_out.contains("/etc/hosts"),
                "leading slash corrupted: {path_out:?}"
            );

            let url_out = prepare_rtl_text("افتح https://example.com/ الآن");
            assert!(
                url_out.contains("https://example.com/"),
                "trailing slash corrupted: {url_out:?}"
            );

            let fn_out = prepare_rtl_text("استخدم الدالة foo() هنا");
            assert!(
                fn_out.contains("foo()"),
                "function parens corrupted: {fn_out:?}"
            );

            let flag_out = prepare_rtl_text("مرر --verbose هنا");
            assert!(
                flag_out.contains("--verbose"),
                "CLI flag corrupted: {flag_out:?}"
            );

            // Inline code span with Style::bg
            let line = Line::from(vec![
                Span::raw("استخدم "),
                Span::styled("bar()", Style::default().bg(ratatui::style::Color::Black)),
                Span::raw(" هنا"),
            ]);
            let prepared_line = prepare_rtl_line(line);
            let plain: String = prepared_line
                .spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect();
            assert!(
                plain.contains("bar()"),
                "inline code span corrupted: {plain:?}"
            );
        });
    }

    #[test]
    fn test_arabic_utf8_boundaries_and_visual_cursor_never_panic() {
        let samples = [
            "",
            "\u{064E}",
            "مَرْحَبًا بِكُمْ فِي ٢٠٢٦",
            "لَا إِلهَ إِلَّا اللَّه",
            "بِلَا حُدُودٍ ۝",
            "Mixed عربي 123 src/bidi.rs",
            "\u{FEFB}\u{FEE3}\u{FEAE}\u{FEA3}\u{FE92}\u{FE8E}",
        ];
        for mode in [
            ArabicShapingMode::StandardUnicode,
            ArabicShapingMode::PresentationForms,
        ] {
            with_arabic_shaping_mode(mode, || {
                for sample in samples {
                    let _ = prepare_rtl_text(sample);
                    for byte_pos in 0..=sample.len() + 3 {
                        let col = visual_cursor_col_for_text(sample, byte_pos);
                        let mapped_byte = byte_offset_for_visual_col_in_text(sample, col);
                        assert!(sample.is_char_boundary(mapped_byte));
                    }
                }
            });
        }
    }
}
