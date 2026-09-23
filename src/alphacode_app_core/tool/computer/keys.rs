//! Layout-independent virtual keycode mapping for key chords.

use core_graphics::event::CGKeyCode;

/// Map a key name (already lowercased, single token) to a US virtual keycode.
/// Returns None for unknown keys.
pub fn keycode_for(key: &str) -> Option<CGKeyCode> {
    use core_graphics::event::KeyCode;
    let code = match key {
        "return" | "enter" => KeyCode::RETURN,
        "tab" => KeyCode::TAB,
        "space" => KeyCode::SPACE,
        "delete" | "backspace" => KeyCode::DELETE,
        "esc" | "escape" => KeyCode::ESCAPE,
        "left" => KeyCode::LEFT_ARROW,
        "right" => KeyCode::RIGHT_ARROW,
        "down" => KeyCode::DOWN_ARROW,
        "up" => KeyCode::UP_ARROW,
        "home" => KeyCode::HOME,
        "end" => KeyCode::END,
        "pageup" => KeyCode::PAGE_UP,
        "pagedown" => KeyCode::PAGE_DOWN,
        "forwarddelete" => KeyCode::FORWARD_DELETE,
        "f1" => 0x7A,
        "f2" => 0x78,
        "f3" => 0x63,
        "f4" => 0x76,
        "f5" => 0x60,
        "f6" => 0x61,
        "f7" => 0x62,
        "f8" => 0x64,
        "f9" => 0x65,
        "f10" => 0x6D,
        "f11" => 0x67,
        "f12" => 0x6F,
        other => return ansi_keycode(other),
    };
    Some(code)
}

/// US ANSI virtual keycodes for single letters, digits, and common punctuation.
/// Layout-independent hardware positions.
pub fn ansi_keycode(key: &str) -> Option<CGKeyCode> {
    let mut chars = key.chars();
    let first = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    let code: CGKeyCode = match first {
        'a' => 0x00,
        'b' => 0x0B,
        'c' => 0x08,
        'd' => 0x02,
        'e' => 0x0E,
        'f' => 0x03,
        'g' => 0x05,
        'h' => 0x04,
        'i' => 0x22,
        'j' => 0x26,
        'k' => 0x28,
        'l' => 0x25,
        'm' => 0x2E,
        'n' => 0x2D,
        'o' => 0x1F,
        'p' => 0x23,
        'q' => 0x0C,
        'r' => 0x0F,
        's' => 0x01,
        't' => 0x11,
        'u' => 0x20,
        'v' => 0x09,
        'w' => 0x0D,
        'x' => 0x07,
        'y' => 0x10,
        'z' => 0x06,
        '0' => 0x1D,
        '1' => 0x12,
        '2' => 0x13,
        '3' => 0x14,
        '4' => 0x15,
        '5' => 0x17,
        '6' => 0x16,
        '7' => 0x1A,
        '8' => 0x1C,
        '9' => 0x19,
        '-' => 0x1B,
        '=' => 0x18,
        '[' => 0x21,
        ']' => 0x1E,
        '\\' => 0x2A,
        ';' => 0x29,
        '\'' => 0x27,
        ',' => 0x2B,
        '.' => 0x2F,
        '/' => 0x2C,
        '`' => 0x32,
        _ => return None,
    };
    Some(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_named_keys() {
        assert_eq!(keycode_for("return"), Some(0x24));
        assert_eq!(keycode_for("space"), Some(0x31));
        assert_eq!(keycode_for("esc"), Some(0x35));
        assert_eq!(keycode_for("escape"), Some(0x35));
        assert_eq!(keycode_for("left"), Some(0x7B));
        assert_eq!(keycode_for("f5"), Some(0x60));
    }

    #[test]
    fn maps_letters_and_digits() {
        assert_eq!(keycode_for("a"), Some(0x00));
        assert_eq!(keycode_for("z"), Some(0x06));
        assert_eq!(keycode_for("0"), Some(0x1D));
        assert_eq!(keycode_for(","), Some(0x2B));
    }

    #[test]
    fn rejects_unknown_keys() {
        assert_eq!(ansi_keycode("nope"), None);
        assert_eq!(keycode_for("nope"), None);
    }
}
