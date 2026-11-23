//! Held keys and mouse buttons -> buttons, triggers and the left stick.

use crate::config::{Action, Layout, Mode};
use crate::keys::Input;
use crate::pad::{Button, PadState};

/// How long one wheel notch holds its action, in seconds.
const WHEEL_PULSE: f64 = 0.07;
/// Steering with keys while driving: stick travel per second when turning in and when letting go,
/// and the smallest tilt used (the game ignores the middle of the stick).
const STEER_IN: f32 = 5.0;
const STEER_OUT: f32 = 9.0;
const STEER_MIN: f32 = 0.3;

pub struct Mapper {
    layout: Layout,
    mode: Mode,
    held: Vec<bool>,
    pulses: Vec<(Input, f64)>,
    smooth_steering: bool,
    steer: f32,
}

impl Mapper {
    pub fn new(layout: Layout, smooth_steering: bool) -> Mapper {
        Mapper { layout, mode: Mode::Foot, held: vec![false; Input::COUNT], pulses: Vec::new(), smooth_steering, steer: 0.0 }
    }

    /// Swaps in new layouts. Keys that are held stay held.
    pub fn set_layout(&mut self, layout: Layout, smooth_steering: bool) {
        self.layout = layout;
        self.smooth_steering = smooth_steering;
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    /// A key or mouse button went down or up, or the wheel turned a notch (`down` is ignored for
    /// the wheel). Returns the new layout if this press switched it.
    pub fn input(&mut self, input: Input, down: bool, now: f64) -> Option<Mode> {
        if input.index() >= Input::COUNT {
            return None;
        }
        if input.is_wheel() {
            self.pulses.retain(|p| p.0 != input);
            self.pulses.push((input, now + WHEEL_PULSE));
            return self.switch_for(input);
        }
        let was = std::mem::replace(&mut self.held[input.index()], down);
        if down && !was { self.switch_for(input) } else { None }
    }

    fn switch_for(&mut self, input: Input) -> Option<Mode> {
        let before = self.mode;
        // Read the actions of the layout in use when the key went down: one press, one switch.
        for act in self.layout.actions(before, input).to_vec() {
            match act {
                Action::ModeToggle => self.mode = before.other(),
                Action::ModeFoot => self.mode = Mode::Foot,
                Action::ModeDrive => self.mode = Mode::Drive,
                _ => {}
            }
        }
        (self.mode != before).then_some(self.mode)
    }

    /// Lets go of everything, as when the game window loses focus.
    pub fn release_all(&mut self) {
        self.held.iter_mut().for_each(|h| *h = false);
        self.pulses.clear();
        self.steer = 0.0;
    }

    /// True while the aim button (L2) is held in the on-foot layout.
    pub fn aiming(&self) -> bool {
        self.mode == Mode::Foot
            && (0..Input::COUNT).any(|i| {
                self.held[i] && self.layout.actions(self.mode, Input(i as u16)).contains(&Action::Press(Button::L2))
            })
    }

    /// The controller right now. `aim` is the right stick from the mouse.
    pub fn state(&mut self, now: f64, dt: f64, aim: (f32, f32)) -> PadState {
        self.pulses.retain(|p| p.1 > now);
        let mut pad = PadState::default();
        let (mut lx, mut ly) = (0.0f32, 0.0f32);
        let held = (0..Input::COUNT).filter(|&i| self.held[i]).map(|i| Input(i as u16));
        for input in held.chain(self.pulses.iter().map(|p| p.0)) {
            for act in self.layout.actions(self.mode, input) {
                match act {
                    Action::Press(b) => pad.press(*b),
                    Action::LsUp => ly += 1.0,
                    Action::LsDown => ly -= 1.0,
                    Action::LsLeft => lx -= 1.0,
                    Action::LsRight => lx += 1.0,
                    Action::ModeToggle | Action::ModeFoot | Action::ModeDrive => {}
                }
            }
        }
        lx = lx.clamp(-1.0, 1.0);
        ly = ly.clamp(-1.0, 1.0);
        let len = lx.hypot(ly);
        if len > 1.0 {
            lx /= len;
            ly /= len;
        }
        if self.mode == Mode::Drive && self.smooth_steering {
            lx = self.ease_steering(lx, dt as f32);
        } else {
            self.steer = 0.0;
        }
        pad.lx = lx;
        pad.ly = ly;
        pad.rx = aim.0.clamp(-1.0, 1.0);
        pad.ry = aim.1.clamp(-1.0, 1.0);
        pad
    }

    /// A key is all or nothing. Ease the stick towards it, quicker when letting go.
    fn ease_steering(&mut self, target: f32, dt: f32) -> f32 {
        let turning_in = target != 0.0 && self.steer * target >= 0.0;
        let rate = if turning_in { STEER_IN } else { STEER_OUT } * dt.max(0.0);
        self.steer += (target - self.steer).clamp(-rate, rate);
        if target != 0.0 && self.steer * target >= 0.0 && self.steer.abs() < STEER_MIN {
            self.steer = STEER_MIN * target.signum();
        }
        self.steer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn mapper() -> Mapper {
        let (layout, warnings) = Layout::compile(&Config::default());
        assert!(warnings.is_empty());
        Mapper::new(layout, true)
    }

    fn key(name: &str) -> Input {
        Input::from_name(name).unwrap()
    }

    #[test]
    fn wasd_moves_the_left_stick_on_a_unit_circle() {
        let mut m = mapper();
        m.input(key("W"), true, 0.0);
        let p = m.state(0.0, 0.001, (0.0, 0.0));
        assert_eq!((p.lx, p.ly), (0.0, 1.0));
        m.input(key("D"), true, 0.0);
        let p = m.state(0.0, 0.001, (0.0, 0.0));
        assert!((p.lx.hypot(p.ly) - 1.0).abs() < 1e-5 && p.lx > 0.7 && p.ly > 0.7);
        m.input(key("S"), true, 0.0); // W + S cancel
        let p = m.state(0.0, 0.001, (0.0, 0.0));
        assert_eq!((p.lx, p.ly), (1.0, 0.0));
    }

    #[test]
    fn buttons_triggers_and_the_right_stick() {
        let mut m = mapper();
        m.input(Input::MOUSE_LEFT, true, 0.0);
        m.input(key("Space"), true, 0.0);
        let p = m.state(0.0, 0.001, (0.25, -0.5));
        assert!(p.pressed(Button::R2) && p.pressed(Button::Cross) && !p.pressed(Button::L2));
        assert_eq!((p.r2, p.l2), (1.0, 0.0));
        assert_eq!((p.rx, p.ry), (0.25, -0.5));
        m.input(Input::MOUSE_LEFT, false, 0.0);
        assert!(!m.state(0.0, 0.001, (0.0, 0.0)).pressed(Button::R2));
    }

    #[test]
    fn f_switches_layout_once_per_press_and_w_changes_meaning() {
        let mut m = mapper();
        m.input(key("W"), true, 0.0);
        assert_eq!(m.input(key("F"), true, 0.0), Some(Mode::Drive));
        assert_eq!(m.input(key("F"), true, 0.0), None, "key repeat does not switch again");
        let p = m.state(0.0, 0.001, (0.0, 0.0));
        assert!(p.pressed(Button::Triangle) && p.pressed(Button::R2), "F is still Triangle, W is now the throttle");
        assert_eq!(p.ly, 0.0);
        m.input(key("F"), false, 0.0);
        assert_eq!(m.input(key("F"), true, 0.0), Some(Mode::Foot));
        assert_eq!(m.input(key("CapsLock"), true, 0.0), Some(Mode::Drive));
    }

    #[test]
    fn release_all_lets_go_of_everything() {
        let mut m = mapper();
        for k in ["W", "Space", "LShift"] {
            m.input(key(k), true, 0.0);
        }
        m.input(Input::MOUSE_RIGHT, true, 0.0);
        m.release_all();
        assert_eq!(m.state(0.0, 0.001, (0.0, 0.0)), PadState::default());
    }

    #[test]
    fn a_wheel_notch_is_a_short_press() {
        let mut cfg = Config::default();
        cfg.keys.foot.insert("WheelUp".into(), crate::config::Bound::One("triangle".into()));
        let mut m = Mapper::new(Layout::compile(&cfg).0, true);
        m.input(Input::WHEEL_UP, true, 1.0);
        assert!(m.state(1.03, 0.001, (0.0, 0.0)).pressed(Button::Triangle));
        assert!(!m.state(1.08, 0.001, (0.0, 0.0)).pressed(Button::Triangle));
    }

    #[test]
    fn aiming_is_the_aim_button_on_foot() {
        let mut m = mapper();
        assert!(!m.aiming());
        m.input(Input::MOUSE_RIGHT, true, 0.0);
        assert!(m.aiming());
        m.set_mode(Mode::Drive);
        assert!(!m.aiming(), "right click is L1 while driving");
    }

    #[test]
    fn steering_eases_in_while_driving_only() {
        let mut m = mapper();
        m.input(key("D"), true, 0.0);
        assert_eq!(m.state(0.0, 0.001, (0.0, 0.0)).lx, 1.0, "on foot a key is full tilt at once");
        m.set_mode(Mode::Drive);
        let first = m.state(0.0, 0.001, (0.0, 0.0)).lx;
        assert!((first - STEER_MIN).abs() < 0.02, "starts just past the dead middle: {first}");
        let mut lx = first;
        for i in 1..200 {
            lx = m.state(i as f64 * 0.001, 0.001, (0.0, 0.0)).lx;
        }
        assert_eq!(lx, 1.0, "full lock within 0.2 s");
        m.input(key("D"), false, 0.2);
        m.input(key("A"), true, 0.2);
        let mut t = 0.2;
        while lx > -1.0 && t < 1.0 {
            lx = m.state(t, 0.001, (0.0, 0.0)).lx;
            t += 0.001;
        }
        assert!(t < 0.6, "full opposite lock in {:.2} s", t - 0.2);
    }
}
