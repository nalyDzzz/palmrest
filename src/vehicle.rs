//! When to switch between the on-foot and driving layouts, with the minimap as evidence.
//!
//! With the game's Minimap setting on Dynamic, the minimap is on screen while driving and hidden
//! while walking around. It also shows on foot during combat and after a d-pad press, so on its
//! own it does not mean "in a vehicle". It is used to check the key instead:
//!
//! - The get-in key (F) on foot, minimap hidden: do not switch yet. Switch to driving keys when the
//!   minimap appears. If it has not appeared after a while, there was no vehicle: nothing changes.
//! - On driving keys and the minimap goes away for a couple of seconds: switch to on-foot keys
//!   (thrown out, vehicle destroyed). Not if a menu, the map or the phone was just opened, since
//!   those hide the minimap too.
//! - The layout is never switched to driving without a key press. Driving keys on foot are the
//!   costly mistake (the throttle key fires the weapon), so every doubt is settled towards on foot.
//! - A switch made by hand (Caps Lock, or in the window) always happens at once and is left alone.
//! - If the picture cannot be read, the key is simply trusted, as if this check were off.

use crate::config::Mode;
use crate::vision::Seen;

/// How long after the get-in key the minimap may take to appear.
pub const ENTER_WAIT: f64 = 8.0;
/// How long the minimap must be gone before driving keys are given up.
pub const LOST_AFTER: f64 = 2.0;
/// A minimap that vanishes this soon after a menu key is taken to be hidden by the menu.
pub const MENU_GRACE: f64 = 3.0;

/// How a layout switch was asked for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asked {
    /// By a key that also presses a button: the get-in / get-out key.
    ByActionKey,
    /// By a key that only switches layout, or from the window.
    ByHand,
}

pub struct VehicleCheck {
    enabled: bool,
    seen: Seen,
    /// When `seen` last changed.
    since: f64,
    /// The get-in key was pressed: switch to driving if the minimap appears before this time.
    waiting_until: Option<f64>,
    /// On driving keys with the minimap seen: its disappearance means the vehicle is gone.
    watch_loss: bool,
    menu_at: f64,
    /// The minimap went away right after a menu key: wait for it to come back.
    in_menu: bool,
}

impl VehicleCheck {
    pub fn new(enabled: bool) -> VehicleCheck {
        VehicleCheck { enabled, seen: Seen::Unknown, since: 0.0, waiting_until: None, watch_loss: false, menu_at: f64::NEG_INFINITY, in_menu: false }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.waiting_until = None;
            self.watch_loss = false;
        }
    }

    /// True between the get-in key and the minimap appearing (or the wait running out).
    pub fn waiting(&self) -> bool {
        self.waiting_until.is_some()
    }

    /// The minimap's state as of `now`.
    pub fn minimap(&mut self, now: f64, seen: Seen) {
        if seen != self.seen {
            if self.seen == Seen::Visible && seen == Seen::Hidden && now - self.menu_at <= MENU_GRACE {
                self.in_menu = true;
            }
            self.seen = seen;
            self.since = now;
        }
    }

    /// A key that opens a menu, the map, the phone or the weapon wheel was pressed.
    pub fn menu_key(&mut self, now: f64) {
        self.menu_at = now;
    }

    /// A switch to `to` was asked for. True: switch now. False: wait, `poll` will say when.
    pub fn asked(&mut self, now: f64, to: Mode, how: Asked) -> bool {
        self.waiting_until = None;
        self.in_menu = false;
        if to == Mode::Foot || !self.enabled {
            self.watch_loss = false;
            return true;
        }
        match (how, self.seen) {
            (Asked::ByHand, seen) => {
                // Left alone until the minimap has been seen at least once.
                self.watch_loss = seen == Seen::Visible;
                true
            }
            (Asked::ByActionKey, Seen::Hidden) => {
                self.waiting_until = Some(now + ENTER_WAIT);
                false
            }
            (Asked::ByActionKey, Seen::Visible) => {
                // Already on screen (combat, a mission): it cannot confirm anything, trust the key.
                self.watch_loss = true;
                true
            }
            (Asked::ByActionKey, Seen::Unknown) => {
                self.watch_loss = false;
                true
            }
        }
    }

    /// Call on every pass with the layout in use. Returns the layout to switch to, if any.
    pub fn poll(&mut self, now: f64, mode: Mode) -> Option<Mode> {
        if !self.enabled {
            return None;
        }
        if let Some(deadline) = self.waiting_until {
            if mode == Mode::Drive {
                self.waiting_until = None; // switched some other way meanwhile
            } else {
                match self.seen {
                    Seen::Visible => {
                        self.waiting_until = None;
                        self.watch_loss = true;
                        return Some(Mode::Drive);
                    }
                    Seen::Unknown => {
                        // The picture stopped being readable: fall back to trusting the key.
                        self.waiting_until = None;
                        self.watch_loss = false;
                        return Some(Mode::Drive);
                    }
                    Seen::Hidden if now >= deadline => self.waiting_until = None, // no vehicle
                    Seen::Hidden => {}
                }
            }
        }
        if mode == Mode::Drive {
            if self.seen == Seen::Visible {
                self.watch_loss = true;
                self.in_menu = false;
            }
            if self.watch_loss && !self.in_menu && self.seen == Seen::Hidden && now - self.since >= LOST_AFTER {
                self.watch_loss = false;
                return Some(Mode::Foot);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use Asked::{ByActionKey, ByHand};
    use Mode::{Drive, Foot};
    use Seen::{Hidden, Unknown, Visible};

    fn on_foot() -> VehicleCheck {
        let mut c = VehicleCheck::new(true);
        c.minimap(0.0, Hidden);
        c
    }

    /// Driving, minimap seen, at time 10.
    fn driving() -> VehicleCheck {
        let mut c = on_foot();
        assert!(!c.asked(5.0, Drive, ByActionKey));
        c.minimap(7.0, Visible);
        assert_eq!(c.poll(7.0, Foot), Some(Drive));
        assert_eq!(c.poll(10.0, Drive), None);
        c
    }

    #[test]
    fn getting_in_switches_when_the_minimap_appears() {
        let mut c = on_foot();
        assert!(!c.asked(1.0, Drive, ByActionKey), "not yet");
        assert!(c.waiting());
        assert_eq!(c.poll(2.0, Foot), None);
        c.minimap(4.0, Visible);
        assert_eq!(c.poll(4.0, Foot), Some(Drive));
        assert!(!c.waiting());
        assert_eq!(c.poll(4.1, Drive), None);
    }

    #[test]
    fn the_get_in_key_with_no_vehicle_changes_nothing() {
        let mut c = on_foot();
        assert!(!c.asked(1.0, Drive, ByActionKey));
        assert_eq!(c.poll(1.0 + ENTER_WAIT - 0.1, Foot), None);
        assert_eq!(c.poll(1.0 + ENTER_WAIT + 0.1, Foot), None);
        assert!(!c.waiting(), "the wait is over");
        c.minimap(20.0, Visible); // a fight starts later
        assert_eq!(c.poll(20.0, Foot), None, "the minimap alone never switches to driving");
    }

    #[test]
    fn a_minimap_already_on_screen_cannot_confirm_so_the_key_is_trusted() {
        let mut c = VehicleCheck::new(true);
        c.minimap(0.0, Visible);
        assert!(c.asked(1.0, Drive, ByActionKey));
        // When the fight ends and the minimap fades, a wrong guess is put right.
        c.minimap(30.0, Hidden);
        assert_eq!(c.poll(30.0 + LOST_AFTER + 0.1, Drive), Some(Foot));
    }

    #[test]
    fn an_unreadable_picture_or_the_check_being_off_means_trusting_the_key() {
        let mut c = VehicleCheck::new(true);
        assert!(c.asked(1.0, Drive, ByActionKey), "nothing seen yet");
        assert_eq!(c.poll(60.0, Drive), None);
        let mut c = VehicleCheck::new(false);
        c.minimap(0.0, Hidden);
        assert!(c.asked(1.0, Drive, ByActionKey));
        assert_eq!(c.poll(60.0, Drive), None, "off: never switches by itself");
        // The picture becomes unreadable during the wait: switch, as the key asked.
        let mut c = on_foot();
        assert!(!c.asked(1.0, Drive, ByActionKey));
        c.minimap(2.0, Unknown);
        assert_eq!(c.poll(2.0, Foot), Some(Drive));
    }

    #[test]
    fn losing_the_minimap_while_driving_goes_back_to_on_foot() {
        let mut c = driving();
        c.minimap(12.0, Hidden);
        assert_eq!(c.poll(12.0 + LOST_AFTER - 0.1, Drive), None, "a blink is ignored");
        assert_eq!(c.poll(12.0 + LOST_AFTER + 0.1, Drive), Some(Foot));
        assert_eq!(c.poll(20.0, Foot), None);
        c.minimap(25.0, Visible);
        assert_eq!(c.poll(25.0, Foot), None, "and it does not switch back by itself");
    }

    #[test]
    fn a_menu_hiding_the_minimap_is_not_leaving_the_vehicle() {
        let mut c = driving();
        c.menu_key(12.0);
        c.minimap(12.8, Hidden);
        assert_eq!(c.poll(40.0, Drive), None, "paused for a long time: still driving keys");
        c.minimap(41.0, Visible);
        assert_eq!(c.poll(41.0, Drive), None);
        // Later the minimap goes with no menu key: that is the vehicle going.
        c.minimap(50.0, Hidden);
        assert_eq!(c.poll(50.0 + LOST_AFTER + 0.1, Drive), Some(Foot));
    }

    #[test]
    fn switching_by_hand_happens_at_once_and_is_left_alone() {
        let mut c = on_foot();
        assert!(c.asked(1.0, Drive, ByHand));
        assert_eq!(c.poll(60.0, Drive), None, "driving keys by hand with no minimap: your call");
        // Once the minimap has been seen, its loss counts again.
        c.minimap(61.0, Visible);
        assert_eq!(c.poll(61.0, Drive), None);
        c.minimap(70.0, Hidden);
        assert_eq!(c.poll(70.0 + LOST_AFTER + 0.1, Drive), Some(Foot));
        // By hand to on-foot keys while the minimap is on screen (hot-wiring needs both sticks).
        let mut c = driving();
        assert!(c.asked(11.0, Foot, ByHand));
        assert_eq!(c.poll(60.0, Foot), None);
    }

    #[test]
    fn getting_out_is_immediate_and_cancels_a_wait() {
        let mut c = driving();
        assert!(c.asked(11.0, Foot, ByActionKey));
        assert_eq!(c.poll(30.0, Foot), None);
        let mut c = on_foot();
        assert!(!c.asked(1.0, Drive, ByActionKey));
        assert!(c.asked(2.0, Drive, ByHand), "Caps Lock during the wait switches now");
        assert_eq!(c.poll(2.0, Drive), None);
        assert!(!c.waiting());
    }
}
