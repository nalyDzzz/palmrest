//! Keyboard keys and mouse buttons, and the names the config file uses for them.

/// A key or mouse button. 0..=255 are Windows virtual-key codes; the mouse inputs follow.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Input(pub u16);

impl Input {
    pub const MOUSE_LEFT: Input = Input(256);
    pub const MOUSE_RIGHT: Input = Input(257);
    pub const MOUSE_MIDDLE: Input = Input(258);
    pub const MOUSE_4: Input = Input(259);
    pub const MOUSE_5: Input = Input(260);
    pub const WHEEL_UP: Input = Input(261);
    pub const WHEEL_DOWN: Input = Input(262);
    /// One past the largest input id.
    pub const COUNT: usize = 263;

    pub const LALT: Input = Input(0xA4);

    pub fn index(self) -> usize {
        self.0 as usize
    }

    pub fn is_wheel(self) -> bool {
        self == Input::WHEEL_UP || self == Input::WHEEL_DOWN
    }

    pub fn is_mouse(self) -> bool {
        self.0 >= 256
    }

    pub fn from_vk(vk: u32) -> Option<Input> {
        (vk < 256).then_some(Input(vk as u16))
    }

    pub fn from_name(name: &str) -> Option<Input> {
        let n = name.trim();
        if n.len() == 1 {
            let c = n.as_bytes()[0].to_ascii_uppercase();
            if c.is_ascii_uppercase() || c.is_ascii_digit() {
                return Some(Input(c as u16));
            }
        }
        NAMED.iter().find(|(s, _)| s.eq_ignore_ascii_case(n)).map(|&(_, id)| Input(id))
    }

    pub fn name(self) -> String {
        if let Some((s, _)) = NAMED.iter().find(|&&(_, id)| id == self.0) {
            return (*s).to_string();
        }
        let c = self.0 as u8;
        if self.0 < 256 && (c.is_ascii_uppercase() || c.is_ascii_digit()) {
            return (c as char).to_string();
        }
        format!("Key{}", self.0)
    }
}

/// Every input that has a name other than a single letter or digit.
const NAMED: &[(&str, u16)] = &[
    ("Esc", 0x1B),
    ("Tab", 0x09),
    ("CapsLock", 0x14),
    ("Space", 0x20),
    ("Enter", 0x0D),
    ("Backspace", 0x08),
    ("LShift", 0xA0),
    ("RShift", 0xA1),
    ("LCtrl", 0xA2),
    ("RCtrl", 0xA3),
    ("LAlt", 0xA4),
    ("RAlt", 0xA5),
    ("Left", 0x25),
    ("Up", 0x26),
    ("Right", 0x27),
    ("Down", 0x28),
    ("Insert", 0x2D),
    ("Delete", 0x2E),
    ("Home", 0x24),
    ("End", 0x23),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
    ("F1", 0x70),
    ("F2", 0x71),
    ("F3", 0x72),
    ("F4", 0x73),
    ("F5", 0x74),
    ("F6", 0x75),
    ("F7", 0x76),
    ("F8", 0x77),
    ("F9", 0x78),
    ("F10", 0x79),
    ("F11", 0x7A),
    ("F12", 0x7B),
    ("Minus", 0xBD),
    ("Equals", 0xBB),
    ("LBracket", 0xDB),
    ("RBracket", 0xDD),
    ("Backslash", 0xDC),
    ("Semicolon", 0xBA),
    ("Quote", 0xDE),
    ("Comma", 0xBC),
    ("Period", 0xBE),
    ("Slash", 0xBF),
    ("Grave", 0xC0),
    ("Numpad0", 0x60),
    ("Numpad1", 0x61),
    ("Numpad2", 0x62),
    ("Numpad3", 0x63),
    ("Numpad4", 0x64),
    ("Numpad5", 0x65),
    ("Numpad6", 0x66),
    ("Numpad7", 0x67),
    ("Numpad8", 0x68),
    ("Numpad9", 0x69),
    ("MouseLeft", 256),
    ("MouseRight", 257),
    ("MouseMiddle", 258),
    ("Mouse4", 259),
    ("Mouse5", 260),
    ("WheelUp", 261),
    ("WheelDown", 262),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_round_trip() {
        for id in 0..Input::COUNT as u16 {
            let name = Input(id).name();
            if !name.starts_with("Key") {
                assert_eq!(Input::from_name(&name), Some(Input(id)), "{name}");
            }
        }
    }

    #[test]
    fn letters_digits_and_case() {
        assert_eq!(Input::from_name("w"), Some(Input(b'W' as u16)));
        assert_eq!(Input::from_name("7"), Some(Input(b'7' as u16)));
        assert_eq!(Input::from_name("lshift"), Some(Input(0xA0)));
        assert_eq!(Input::from_name("mouseleft"), Some(Input::MOUSE_LEFT));
        assert_eq!(Input::from_name("NoSuchKey"), None);
    }
}
