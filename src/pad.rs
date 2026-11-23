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

    pub fn press(&mut self, b: Button) {
        self.buttons |= b.bit();
        match b {
            Button::L2 => self.l2 = 1.0,
            Button::R2 => self.r2 = 1.0,
            _ => {}
        }
    }
}
