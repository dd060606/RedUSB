//! USB HID keyboard usage codes, modifier bits and Ducky key names.

/// Keyboard/Keypad usage page IDs (HID Usage Tables, section 10).
#[allow(dead_code)]
pub mod usage {
    pub const NONE: u8 = 0x00;
    pub const A: u8 = 0x04;
    pub const B: u8 = 0x05;
    pub const C: u8 = 0x06;
    pub const D: u8 = 0x07;
    pub const E: u8 = 0x08;
    pub const F: u8 = 0x09;
    pub const G: u8 = 0x0A;
    pub const H: u8 = 0x0B;
    pub const I: u8 = 0x0C;
    pub const J: u8 = 0x0D;
    pub const K: u8 = 0x0E;
    pub const L: u8 = 0x0F;
    pub const M: u8 = 0x10;
    pub const N: u8 = 0x11;
    pub const O: u8 = 0x12;
    pub const P: u8 = 0x13;
    pub const Q: u8 = 0x14;
    pub const R: u8 = 0x15;
    pub const S: u8 = 0x16;
    pub const T: u8 = 0x17;
    pub const U: u8 = 0x18;
    pub const V: u8 = 0x19;
    pub const W: u8 = 0x1A;
    pub const X: u8 = 0x1B;
    pub const Y: u8 = 0x1C;
    pub const Z: u8 = 0x1D;
    pub const KEY_1: u8 = 0x1E;
    pub const KEY_2: u8 = 0x1F;
    pub const KEY_3: u8 = 0x20;
    pub const KEY_4: u8 = 0x21;
    pub const KEY_5: u8 = 0x22;
    pub const KEY_6: u8 = 0x23;
    pub const KEY_7: u8 = 0x24;
    pub const KEY_8: u8 = 0x25;
    pub const KEY_9: u8 = 0x26;
    pub const KEY_0: u8 = 0x27;
    pub const ENTER: u8 = 0x28;
    pub const ESCAPE: u8 = 0x29;
    pub const BACKSPACE: u8 = 0x2A;
    pub const TAB: u8 = 0x2B;
    pub const SPACE: u8 = 0x2C;
    pub const MINUS: u8 = 0x2D;
    pub const EQUAL: u8 = 0x2E;
    pub const LEFT_BRACKET: u8 = 0x2F;
    pub const RIGHT_BRACKET: u8 = 0x30;
    pub const BACKSLASH: u8 = 0x31;
    pub const NON_US_HASH: u8 = 0x32;
    pub const SEMICOLON: u8 = 0x33;
    pub const QUOTE: u8 = 0x34;
    pub const GRAVE: u8 = 0x35;
    pub const COMMA: u8 = 0x36;
    pub const DOT: u8 = 0x37;
    pub const SLASH: u8 = 0x38;
    pub const CAPS_LOCK: u8 = 0x39;
    pub const F1: u8 = 0x3A;
    pub const F12: u8 = 0x45;
    pub const PRINT_SCREEN: u8 = 0x46;
    pub const SCROLL_LOCK: u8 = 0x47;
    pub const PAUSE: u8 = 0x48;
    pub const INSERT: u8 = 0x49;
    pub const HOME: u8 = 0x4A;
    pub const PAGE_UP: u8 = 0x4B;
    pub const DELETE: u8 = 0x4C;
    pub const END: u8 = 0x4D;
    pub const PAGE_DOWN: u8 = 0x4E;
    pub const RIGHT_ARROW: u8 = 0x4F;
    pub const LEFT_ARROW: u8 = 0x50;
    pub const DOWN_ARROW: u8 = 0x51;
    pub const UP_ARROW: u8 = 0x52;
    pub const NUM_LOCK: u8 = 0x53;
    pub const NON_US_BACKSLASH: u8 = 0x64;
    pub const APPLICATION: u8 = 0x65;
}

/// Modifier bits of the first byte of a boot keyboard report.
pub mod modifier {
    pub const LEFT_CTRL: u8 = 0x01;
    pub const LEFT_SHIFT: u8 = 0x02;
    pub const LEFT_ALT: u8 = 0x04;
    pub const LEFT_GUI: u8 = 0x08;
    pub const RIGHT_CTRL: u8 = 0x10;
    pub const RIGHT_SHIFT: u8 = 0x20;
    /// AltGr on international layouts.
    pub const RIGHT_ALT: u8 = 0x40;
    pub const RIGHT_GUI: u8 = 0x80;
}

/// One key press: a usage ID plus the modifiers held with it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct KeyStroke {
    pub usage: u8,
    pub modifiers: u8,
}

impl KeyStroke {
    pub const fn new(usage: u8, modifiers: u8) -> Self {
        Self { usage, modifiers }
    }

    pub const fn key(usage: u8) -> Self {
        Self::new(usage, 0)
    }

    pub const fn shifted(usage: u8) -> Self {
        Self::new(usage, modifier::LEFT_SHIFT)
    }

    pub const fn alt_gr(usage: u8) -> Self {
        Self::new(usage, modifier::RIGHT_ALT)
    }

    pub const fn with(self, modifiers: u8) -> Self {
        Self::new(self.usage, self.modifiers | modifiers)
    }
}

const MODIFIER_NAMES: &[(&str, u8)] = &[
    ("CTRL", modifier::LEFT_CTRL),
    ("CONTROL", modifier::LEFT_CTRL),
    ("SHIFT", modifier::LEFT_SHIFT),
    ("ALT", modifier::LEFT_ALT),
    ("GUI", modifier::LEFT_GUI),
    ("WINDOWS", modifier::LEFT_GUI),
    ("COMMAND", modifier::LEFT_GUI),
    ("ALTGR", modifier::RIGHT_ALT),
];

const KEY_NAMES: &[(&str, u8)] = &[
    ("ENTER", usage::ENTER),
    ("RETURN", usage::ENTER),
    ("ESC", usage::ESCAPE),
    ("ESCAPE", usage::ESCAPE),
    ("BACKSPACE", usage::BACKSPACE),
    ("TAB", usage::TAB),
    ("SPACE", usage::SPACE),
    ("CAPSLOCK", usage::CAPS_LOCK),
    ("PRINTSCREEN", usage::PRINT_SCREEN),
    ("SCROLLLOCK", usage::SCROLL_LOCK),
    ("PAUSE", usage::PAUSE),
    ("BREAK", usage::PAUSE),
    ("INSERT", usage::INSERT),
    ("HOME", usage::HOME),
    ("PAGEUP", usage::PAGE_UP),
    ("DELETE", usage::DELETE),
    ("DEL", usage::DELETE),
    ("END", usage::END),
    ("PAGEDOWN", usage::PAGE_DOWN),
    ("RIGHT", usage::RIGHT_ARROW),
    ("RIGHTARROW", usage::RIGHT_ARROW),
    ("LEFT", usage::LEFT_ARROW),
    ("LEFTARROW", usage::LEFT_ARROW),
    ("DOWN", usage::DOWN_ARROW),
    ("DOWNARROW", usage::DOWN_ARROW),
    ("UP", usage::UP_ARROW),
    ("UPARROW", usage::UP_ARROW),
    ("NUMLOCK", usage::NUM_LOCK),
    ("MENU", usage::APPLICATION),
    ("APP", usage::APPLICATION),
];

/// Looks up a modifier by its Ducky name (case-insensitive).
pub fn modifier_by_name(name: &str) -> Option<u8> {
    MODIFIER_NAMES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, m)| *m)
}

/// Looks up a key by its Ducky name, `F1`..`F12`, or a single letter / digit.
pub fn key_by_name(name: &str) -> Option<u8> {
    if let Some(u) = KEY_NAMES
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(name))
        .map(|(_, u)| *u)
    {
        return Some(u);
    }
    let bytes = name.as_bytes();
    if bytes.len() == 1 {
        return match bytes[0] {
            b'a'..=b'z' => Some(usage::A + bytes[0] - b'a'),
            b'A'..=b'Z' => Some(usage::A + bytes[0] - b'A'),
            b'1'..=b'9' => Some(usage::KEY_1 + bytes[0] - b'1'),
            b'0' => Some(usage::KEY_0),
            _ => None,
        };
    }
    if (bytes[0] == b'F' || bytes[0] == b'f')
        && bytes.len() <= 3
        && let Ok(n) = name[1..].parse::<u8>()
        && (1..=12).contains(&n)
    {
        return Some(usage::F1 + n - 1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_case_insensitive() {
        assert_eq!(modifier_by_name("ctrl"), Some(modifier::LEFT_CTRL));
        assert_eq!(modifier_by_name("Gui"), Some(modifier::LEFT_GUI));
        assert_eq!(key_by_name("enter"), Some(usage::ENTER));
        assert_eq!(key_by_name("DownArrow"), Some(usage::DOWN_ARROW));
        assert_eq!(key_by_name("nope"), None);
    }

    #[test]
    fn function_and_character_keys() {
        assert_eq!(key_by_name("F1"), Some(usage::F1));
        assert_eq!(key_by_name("f12"), Some(usage::F12));
        assert_eq!(key_by_name("F13"), None);
        assert_eq!(key_by_name("F0"), None);
        assert_eq!(key_by_name("t"), Some(usage::T));
        assert_eq!(key_by_name("R"), Some(usage::R));
        assert_eq!(key_by_name("0"), Some(usage::KEY_0));
        assert_eq!(key_by_name("5"), Some(usage::KEY_5));
        assert_eq!(key_by_name("-"), None);
    }
}
