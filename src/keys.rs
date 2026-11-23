//! Keyboard keys and mouse buttons, and the names the config file uses for them.

/// The characters that stand for a drawn sign in a key's overlay name.
pub mod sign {
    pub const BACKSPACE: char = '\u{232B}';
    pub const ENTER: char = '\u{23CE}';
    pub const SHIFT: char = '\u{21E7}';
    pub const TAB: char = '\u{21E5}';
    pub const CAPS_LOCK: char = '\u{21EA}';
    pub const SPACE: char = '\u{2423}';
    pub const UP: char = '\u{2191}';
    pub const DOWN: char = '\u{2193}';
    pub const LEFT: char = '\u{2190}';
    pub const RIGHT: char = '\u{2192}';
    /// A mouse with its left button, right button or wheel filled in, and one with none.
    pub const MOUSE_LEFT: char = '\u{E000}';
    pub const MOUSE_RIGHT: char = '\u{E001}';
    pub const MOUSE_MIDDLE: char = '\u{E002}';
    pub const MOUSE: char = '\u{E003}';
    pub const ALL: [char; 14] = [BACKSPACE, ENTER, SHIFT, TAB, CAPS_LOCK, SPACE, UP, DOWN, LEFT, RIGHT, MOUSE_LEFT, MOUSE_RIGHT, MOUSE_MIDDLE, MOUSE];
}

/// One place on a keyboard row: a key and its width in key units, or empty space.
pub enum Slot {
    K(&'static str, f32),
    Gap(f32),
}
use Slot::{Gap, K};

/// The main block, 15 units wide.
pub const MAIN: [&[Slot]; 6] = [
    &[K("Esc", 1.0), Gap(1.0), K("F1", 1.0), K("F2", 1.0), K("F3", 1.0), K("F4", 1.0), Gap(0.5), K("F5", 1.0), K("F6", 1.0), K("F7", 1.0), K("F8", 1.0), Gap(0.5), K("F9", 1.0), K("F10", 1.0), K("F11", 1.0), K("F12", 1.0)],
    &[K("Grave", 1.0), K("1", 1.0), K("2", 1.0), K("3", 1.0), K("4", 1.0), K("5", 1.0), K("6", 1.0), K("7", 1.0), K("8", 1.0), K("9", 1.0), K("0", 1.0), K("Minus", 1.0), K("Equals", 1.0), K("Backspace", 2.0)],
    &[K("Tab", 1.5), K("Q", 1.0), K("W", 1.0), K("E", 1.0), K("R", 1.0), K("T", 1.0), K("Y", 1.0), K("U", 1.0), K("I", 1.0), K("O", 1.0), K("P", 1.0), K("LBracket", 1.0), K("RBracket", 1.0), K("Backslash", 1.5)],
    &[K("CapsLock", 1.75), K("A", 1.0), K("S", 1.0), K("D", 1.0), K("F", 1.0), K("G", 1.0), K("H", 1.0), K("J", 1.0), K("K", 1.0), K("L", 1.0), K("Semicolon", 1.0), K("Quote", 1.0), K("Enter", 2.25)],
    &[K("LShift", 2.25), K("Z", 1.0), K("X", 1.0), K("C", 1.0), K("V", 1.0), K("B", 1.0), K("N", 1.0), K("M", 1.0), K("Comma", 1.0), K("Period", 1.0), K("Slash", 1.0), K("RShift", 2.75)],
    &[K("LCtrl", 1.5), Gap(1.25), K("LAlt", 1.25), K("Space", 6.25), K("RAlt", 1.25), Gap(2.0), K("RCtrl", 1.5)],
];
/// The block to the right of it, 3 units wide.
pub const NAV: [&[Slot]; 6] = [
    &[],
    &[K("Insert", 1.0), K("Home", 1.0), K("PageUp", 1.0)],
    &[K("Delete", 1.0), K("End", 1.0), K("PageDown", 1.0)],
    &[],
    &[Gap(1.0), K("Up", 1.0)],
    &[K("Left", 1.0), K("Down", 1.0), K("Right", 1.0)],
];
/// The main block's width in key units, and the space between the blocks.
pub const MAIN_UNITS: f32 = 15.0;
pub const BLOCK_GAP: f32 = 0.45;

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

    /// The short name to show for the key: what is printed on it, more or less.
    pub fn label(self) -> String {
        let name = self.name();
        let short = match name.as_str() {
            "LShift" | "RShift" => "Shift",
            "LCtrl" | "RCtrl" => "Ctrl",
            "LAlt" => "Alt",
            "RAlt" => "Alt Gr",
            "CapsLock" => "Caps",
            "Backspace" => "Bksp",
            "PageUp" => "PgUp",
            "PageDown" => "PgDn",
            "Insert" => "Ins",
            "Delete" => "Del",
            "Grave" => "`",
            "Minus" => "-",
            "Equals" => "=",
            "LBracket" => "[",
            "RBracket" => "]",
            "Backslash" => "\\",
            "Semicolon" => ";",
            "Quote" => "'",
            "Comma" => ",",
            "Period" => ".",
            "Slash" => "/",
            "MouseLeft" => "Left click",
            "MouseRight" => "Right click",
            "MouseMiddle" => "Wheel click",
            "WheelUp" => "Wheel up",
            "WheelDown" => "Wheel down",
            "Mouse4" => "Side, back",
            "Mouse5" => "Side, front",
            other => other,
        };
        short.to_string()
    }

    /// The key as the overlay shows it: the sign printed on the key where it has one (Backspace,
    /// Enter, Shift, Tab, Caps Lock, Space, the arrows), a little mouse for the mouse buttons,
    /// and otherwise its short name. The signs are single characters that the overlay draws
    /// itself (see `sign`); for the mouse they come from Unicode's private range.
    pub fn sign(self) -> String {
        match self.name().as_str() {
            "Backspace" => sign::BACKSPACE.to_string(),
            "Enter" => sign::ENTER.to_string(),
            "LShift" | "RShift" => sign::SHIFT.to_string(),
            "Tab" => sign::TAB.to_string(),
            "CapsLock" => sign::CAPS_LOCK.to_string(),
            "Space" => sign::SPACE.to_string(),
            "Up" => sign::UP.to_string(),
            "Down" => sign::DOWN.to_string(),
            "Left" => sign::LEFT.to_string(),
            "Right" => sign::RIGHT.to_string(),
            "MouseLeft" => sign::MOUSE_LEFT.to_string(),
            "MouseRight" => sign::MOUSE_RIGHT.to_string(),
            "MouseMiddle" => sign::MOUSE_MIDDLE.to_string(),
            "WheelUp" => format!("{}{}", sign::MOUSE_MIDDLE, sign::UP),
            "WheelDown" => format!("{}{}", sign::MOUSE_MIDDLE, sign::DOWN),
            "Mouse4" => format!("{}4", sign::MOUSE),
            "Mouse5" => format!("{}5", sign::MOUSE),
            _ => self.label(),
        }
    }

    /// Where the key sits on a keyboard: its left and right edge and its row, in key units from
    /// the top left of the main block. `None` for the mouse and for keys not on the drawn keyboard.
    pub fn span(self) -> Option<(f32, f32, f32)> {
        let name = self.name();
        for (block, left) in [(&MAIN, 0.0), (&NAV, MAIN_UNITS + BLOCK_GAP)] {
            for (row, slots) in block.iter().enumerate() {
                let mut x = left;
                for slot in slots.iter() {
                    match slot {
                        K(key, width) if *key == name => return Some((x, x + width, row as f32)),
                        K(_, width) | Gap(width) => x += width,
                    }
                }
            }
        }
        None
    }

    /// How far the key is from a hand resting at `home` (a place on the keyboard, as `span`
    /// gives it), measured to the nearest part of the key: the space bar is long, and a thumb
    /// reaches its near end. The two main mouse buttons are always under the other hand, so
    /// they count as nearest of all; the wheel and the side buttons, which not every mouse
    /// has, come after the keys around the hand.
    pub fn reach(self, home: (f32, f32)) -> f32 {
        match self {
            Input::MOUSE_LEFT | Input::MOUSE_RIGHT => 0.0,
            Input::MOUSE_MIDDLE | Input::WHEEL_UP | Input::WHEEL_DOWN => 2.5,
            Input::MOUSE_4 | Input::MOUSE_5 => 3.0,
            key => match key.span() {
                Some((left, right, row)) => (home.0.clamp(left, right) - home.0).hypot(row - home.1),
                None => 30.0,
            },
        }
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
