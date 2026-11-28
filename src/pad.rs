//! The controller state that every output understands.

/// One digital control on a DualSense / DualShock 4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Button {
    Cross,
    Circle,
    Square,
    Triangle,
    L1,
    R1,
    L2,
    R2,
    L3,
    R3,
    Options,
    Create,
    Ps,
    Touchpad,
    DpadUp,
    DpadDown,
    DpadLeft,
    DpadRight,
}

impl Button {
    pub const ALL: [Button; 18] = [
        Button::Cross,
        Button::Circle,
        Button::Square,
        Button::Triangle,
        Button::L1,
        Button::R1,
        Button::L2,
        Button::R2,
        Button::L3,
        Button::R3,
        Button::Options,
        Button::Create,
        Button::Ps,
        Button::Touchpad,
        Button::DpadUp,
        Button::DpadDown,
        Button::DpadLeft,
        Button::DpadRight,
    ];

    pub const fn bit(self) -> u32 {
        1 << self as u8
    }

    /// The name used in the config file.
    pub const fn name(self) -> &'static str {
        match self {
            Button::Cross => "cross",
            Button::Circle => "circle",
            Button::Square => "square",
            Button::Triangle => "triangle",
            Button::L1 => "l1",
            Button::R1 => "r1",
            Button::L2 => "l2",
            Button::R2 => "r2",
            Button::L3 => "l3",
            Button::R3 => "r3",
            Button::Options => "options",
            Button::Create => "create",
            Button::Ps => "ps",
            Button::Touchpad => "touchpad",
            Button::DpadUp => "dpad_up",
            Button::DpadDown => "dpad_down",
            Button::DpadLeft => "dpad_left",
            Button::DpadRight => "dpad_right",
        }
    }

    /// What the game calls it on screen.
    pub const fn label(self) -> &'static str {
        match self {
            Button::Cross => "Cross",
            Button::Circle => "Circle",
            Button::Square => "Square",
            Button::Triangle => "Triangle",
            Button::L1 => "L1",
            Button::R1 => "R1",
            Button::L2 => "L2",
            Button::R2 => "R2",
            Button::L3 => "L3",
            Button::R3 => "R3",
            Button::Options => "Options",
            Button::Create => "Create",
            Button::Ps => "PS",
            Button::Touchpad => "Touchpad",
            Button::DpadUp => "D-pad up",
            Button::DpadDown => "D-pad down",
            Button::DpadLeft => "D-pad left",
            Button::DpadRight => "D-pad right",
        }
    }

    pub fn from_name(name: &str) -> Option<Button> {
        Button::ALL.into_iter().find(|b| b.name() == name)
    }
}

/// A full controller reading. Sticks are -1..1 with +x right and +y up; triggers are 0..1.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PadState {
    pub buttons: u32,
    pub lx: f32,
    pub ly: f32,
    pub rx: f32,
    pub ry: f32,
    pub l2: f32,
    pub r2: f32,
}

impl PadState {
    pub fn pressed(&self, b: Button) -> bool {
        self.buttons & b.bit() != 0
    }

    /// Lets go of a button again (a trigger lowered all the way, say).
    pub fn release(&mut self, b: Button) {
        self.buttons &= !b.bit();
        match b {
            Button::L2 => self.l2 = 0.0,
            Button::R2 => self.r2 = 0.0,
            _ => {}
        }
    }

    /// This controller and another held at once, as one: a button is down if either has it
    /// down, a trigger is pulled as far as the further of the two, and a stick is where the one
    /// pushed further from its middle is. (Adding the sticks up would let a worn stick at rest
    /// pull the other off its line.)
    pub fn with(self, other: PadState) -> PadState {
        let further = |a: (f32, f32), b: (f32, f32)| if b.0.hypot(b.1) > a.0.hypot(a.1) { b } else { a };
        let (lx, ly) = further((self.lx, self.ly), (other.lx, other.ly));
        let (rx, ry) = further((self.rx, self.ry), (other.rx, other.ry));
        PadState { buttons: self.buttons | other.buttons, lx, ly, rx, ry, l2: self.l2.max(other.l2), r2: self.r2.max(other.r2) }
    }

    pub fn press(&mut self, b: Button) {
        self.buttons |= b.bit();
        match b {
            Button::L2 => self.l2 = 1.0,
            Button::R2 => self.r2 = 1.0,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_controllers_held_at_once_are_one() {
        let mut keys = PadState { lx: 0.0, ly: 1.0, rx: 0.4, ry: 0.0, r2: 0.4, ..PadState::default() };
        keys.buttons = Button::R2.bit() | Button::Cross.bit();
        let mut real = PadState { lx: 0.05, ly: -0.02, rx: -1.0, ry: 0.0, l2: 0.7, r2: 0.1, ..PadState::default() };
        real.buttons = Button::L2.bit() | Button::Circle.bit();
        let both = keys.with(real);
        assert_eq!((both.lx, both.ly), (0.0, 1.0), "the stick pushed further wins whole: a resting one does not bend it");
        assert_eq!((both.rx, both.ry), (-1.0, 0.0));
        assert_eq!((both.l2, both.r2), (0.7, 0.4));
        for b in [Button::R2, Button::Cross, Button::L2, Button::Circle] {
            assert!(both.pressed(b), "{b:?}");
        }
        assert_eq!(both, real.with(keys), "whichever is named first");
        assert_eq!(keys.with(PadState::default()), keys);
    }
}
