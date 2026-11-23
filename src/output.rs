//! Where the controller state goes.
//!
//! Today: a virtual DualShock 4 made with the ViGEmBus driver, which a Remote Play program
//! (Veyra, chiaki-ng, Sony's own) picks up like a real controller and forwards to the console.

use vigem_client::{Client, DS4Report, DualShock4Wired, TargetId};

use crate::pad::{Button, PadState};

/// What became of a state that was sent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sent {
    Taken,
    /// Not taken this time: send it again shortly.
    Again,
}

pub trait Output: Send {
    fn send(&mut self, pad: &PadState) -> Result<Sent, String>;
}

/// Windows error "no more items".
const ERROR_NO_MORE_ITEMS: u32 = 259;

// DualShock 4 report layout, as the ViGEmBus driver expects it.
const DS4_SQUARE: u16 = 1 << 4;
const DS4_CROSS: u16 = 1 << 5;
const DS4_CIRCLE: u16 = 1 << 6;
const DS4_TRIANGLE: u16 = 1 << 7;
const DS4_L1: u16 = 1 << 8;
const DS4_R1: u16 = 1 << 9;
const DS4_L2: u16 = 1 << 10;
const DS4_R2: u16 = 1 << 11;
const DS4_SHARE: u16 = 1 << 12;
const DS4_OPTIONS: u16 = 1 << 13;
const DS4_L3: u16 = 1 << 14;
const DS4_R3: u16 = 1 << 15;
const DS4_SPECIAL_PS: u8 = 1 << 0;
const DS4_SPECIAL_TOUCHPAD: u8 = 1 << 1;
/// The d-pad is one number in the low four bits: 0 = up, then clockwise, 8 = nothing.
const DS4_DPAD_NONE: u16 = 8;

fn axis(v: f32) -> u8 {
    (128.0 + v.clamp(-1.0, 1.0) * 127.0).round().clamp(0.0, 255.0) as u8
}

fn trigger(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

pub fn to_ds4(pad: &PadState) -> DS4Report {
    let mut buttons = 0u16;
    for (ours, theirs) in [
        (Button::Square, DS4_SQUARE),
        (Button::Cross, DS4_CROSS),
        (Button::Circle, DS4_CIRCLE),
        (Button::Triangle, DS4_TRIANGLE),
        (Button::L1, DS4_L1),
        (Button::R1, DS4_R1),
        (Button::L2, DS4_L2),
        (Button::R2, DS4_R2),
        (Button::Create, DS4_SHARE),
        (Button::Options, DS4_OPTIONS),
        (Button::L3, DS4_L3),
        (Button::R3, DS4_R3),
    ] {
        if pad.pressed(ours) {
            buttons |= theirs;
        }
    }
    let x = pad.pressed(Button::DpadRight) as i32 - pad.pressed(Button::DpadLeft) as i32;
    let y = pad.pressed(Button::DpadUp) as i32 - pad.pressed(Button::DpadDown) as i32;
    buttons |= match (x, y) {
        (0, 1) => 0,
        (1, 1) => 1,
        (1, 0) => 2,
        (1, -1) => 3,
        (0, -1) => 4,
        (-1, -1) => 5,
        (-1, 0) => 6,
        (-1, 1) => 7,
        _ => DS4_DPAD_NONE,
    };
    let mut special = 0u8;
    if pad.pressed(Button::Ps) {
        special |= DS4_SPECIAL_PS;
    }
    if pad.pressed(Button::Touchpad) {
        special |= DS4_SPECIAL_TOUCHPAD;
    }
    DS4Report {
        thumb_lx: axis(pad.lx),
        thumb_ly: axis(-pad.ly), // the DualShock's stick axes point down
        thumb_rx: axis(pad.rx),
        thumb_ry: axis(-pad.ry),
        buttons,
        special,
        // A pressed trigger button with no travel reads as not pressed in some games.
        trigger_l: trigger(if pad.pressed(Button::L2) { pad.l2.max(1.0) } else { pad.l2 }),
        trigger_r: trigger(if pad.pressed(Button::R2) { pad.r2.max(1.0) } else { pad.r2 }),
    }
}

pub struct VirtualDs4 {
    target: DualShock4Wired<Client>,
}

impl VirtualDs4 {
    /// Plugs in a virtual DualShock 4. Fails if the ViGEmBus driver is not installed.
    pub fn connect() -> Result<VirtualDs4, String> {
        let client = Client::connect().map_err(|e| format!("The ViGEmBus controller driver is not available ({e})."))?;
        let mut target = DualShock4Wired::new(client, TargetId::DUALSHOCK4_WIRED);
        target.plugin().map_err(|e| format!("Could not plug in the virtual controller ({e})."))?;
        target.wait_ready().map_err(|e| format!("The virtual controller did not become ready ({e})."))?;
        Ok(VirtualDs4 { target })
    }
}

impl Output for VirtualDs4 {
    fn send(&mut self, pad: &PadState) -> Result<Sent, String> {
        match self.target.update(&to_ds4(pad)) {
            Ok(()) => Ok(Sent::Taken),
            // The driver hands a report straight to a waiting reader and has nowhere to keep it
            // when none is waiting (just after plugging in, or just after the previous report).
            // It answers "no more items" and drops the report, so it has to be sent again.
            Err(vigem_client::Error::WinError(ERROR_NO_MORE_ITEMS)) => Ok(Sent::Again),
            Err(e) => Err(format!("The virtual controller stopped accepting input ({e}).")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resting_controller_is_centred() {
        assert_eq!(to_ds4(&PadState::default()), DS4Report::default());
    }

    #[test]
    fn sticks_reach_both_ends_and_y_is_flipped() {
        let r = to_ds4(&PadState { lx: 1.0, ly: 1.0, rx: -1.0, ry: -1.0, ..PadState::default() });
        assert_eq!((r.thumb_lx, r.thumb_ly), (255, 1), "right and up");
        assert_eq!((r.thumb_rx, r.thumb_ry), (1, 255), "left and down");
        let r = to_ds4(&PadState { rx: 5.0, ..PadState::default() });
        assert_eq!(r.thumb_rx, 255, "out of range is clamped");
    }

    #[test]
    fn buttons_dpad_and_triggers() {
        let mut pad = PadState::default();
        for b in [Button::Cross, Button::R2, Button::L3, Button::Ps, Button::DpadUp, Button::DpadRight] {
            pad.press(b);
        }
        let r = to_ds4(&pad);
        assert_eq!(r.buttons & 0xF, 1, "up + right");
        assert_eq!(r.buttons & !0xF, DS4_CROSS | DS4_R2 | DS4_L3);
        assert_eq!(r.special, DS4_SPECIAL_PS);
        assert_eq!((r.trigger_l, r.trigger_r), (0, 255));
        let mut pad = PadState::default();
        pad.press(Button::DpadLeft);
        pad.press(Button::DpadRight);
        assert_eq!(to_ds4(&pad).buttons & 0xF, DS4_DPAD_NONE, "opposite directions cancel");
    }
}
