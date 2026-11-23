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
    /// The game is asking for the left stick: keys that are the left stick on foot are the left
    /// stick now, whatever the layout says. This is what makes hot-wiring work on driving keys.
    stick_wanted: bool,
    /// The get-in key does not switch to the driving layout by itself: something else will.
    hold_get_in: bool,
    /// How far the keys for L2 and R2 pull them, 0..1, as set with the wheel (see `turn_dial`).
    pull: [f32; 2],
    /// When the dial was last turned or a trigger key last held: some time after, the pull is
    /// all the way again.
    pull_used: f64,
    /// When a key for L2 and one for R2 last went down: the wheel sets the later one.
    pulled_at: [f64; 2],
}

/// L2 and R2, in the order `pull` keeps them.
const TRIGGERS: [Button; 2] = [Button::L2, Button::R2];

impl Mapper {
    pub fn new(layout: Layout, smooth_steering: bool) -> Mapper {
        Mapper {
            layout,
            mode: Mode::Foot,
            held: vec![false; Input::COUNT],
            pulses: Vec::new(),
            smooth_steering,
            steer: 0.0,
            stick_wanted: false,
            hold_get_in: false,
            pull: [1.0; 2],
            pull_used: 0.0,
            pulled_at: [0.0; 2],
        }
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

    /// Whether the game is asking for the left stick right now (see `stick_wanted`).
    pub fn set_stick_wanted(&mut self, wanted: bool) {
        self.stick_wanted = wanted;
    }

    /// Whether a key that gets into a vehicle should leave the switch to driving keys to
    /// something else (the vehicle's name appearing). Getting out, and keys that only switch
    /// layout, are not affected.
    pub fn set_hold_get_in(&mut self, hold: bool) {
        self.hold_get_in = hold;
    }

    /// What an input does right now: its actions in the layout in use, or, while the game asks
    /// for the left stick, its on-foot actions if those are the left stick.
    fn doing(&self, input: Input) -> &[Action] {
        if self.stick_wanted && self.mode == Mode::Drive {
            let on_foot = self.layout.actions(Mode::Foot, input);
            if on_foot.iter().any(|a| matches!(a, Action::LsUp | Action::LsDown | Action::LsLeft | Action::LsRight)) {
                return on_foot;
            }
        }
        self.layout.actions(self.mode, input)
    }

    /// A key or mouse button went down or up, or the wheel turned a notch (`down` is ignored for
    /// the wheel). Returns the new layout if this press switched it.
    pub fn input(&mut self, input: Input, down: bool, now: f64) -> Option<Mode> {
        if input.index() >= Input::COUNT {
            return None;
        }
        if input.is_wheel() {
            // As the triggers' dial the wheel does nothing else, except that a wheel direction
            // with a job of its own keeps that job while no trigger key is held.
            let held = self.triggers_held();
            if self.layout.dial.is_some() && (held != [false, false] || self.doing(input).is_empty()) {
                self.turn_dial(input == Input::WHEEL_UP, held, now);
                return None;
            }
            self.pulses.retain(|p| p.0 != input);
            self.pulses.push((input, now + WHEEL_PULSE));
            return self.switch_for(input);
        }
        let was = std::mem::replace(&mut self.held[input.index()], down);
        if down && !was {
            for (i, trigger) in TRIGGERS.iter().enumerate() {
                if self.doing(input).contains(&Action::Press(*trigger)) {
                    self.pulled_at[i] = now;
                }
            }
            return self.switch_for(input);
        }
        None
    }

    /// Which of L2 and R2 have a key held for them right now.
    fn triggers_held(&self) -> [bool; 2] {
        let mut held = [false; 2];
        for input in (0..Input::COUNT).filter(|&i| self.held[i]).map(|i| Input(i as u16)) {
            for (i, trigger) in TRIGGERS.iter().enumerate() {
                held[i] |= self.doing(input).contains(&Action::Press(*trigger));
            }
        }
        held
    }

    /// One notch of the wheel as the triggers' dial: up for more pull, down for less, never
    /// less than one notch's worth. It sets the trigger whose key is held (the one pressed last
    /// if both are), so the two can be given different pulls; with neither held it sets both,
    /// ahead of the press.
    fn turn_dial(&mut self, up: bool, held: [bool; 2], now: f64) {
        let Some((step, _)) = self.layout.dial else { return };
        let which: &[usize] = match held {
            [true, true] if self.pulled_at[0] > self.pulled_at[1] => &[0],
            [true, true] | [false, true] => &[1],
            [true, false] => &[0],
            [false, false] => &[0, 1],
        };
        for &i in which {
            let wanted = self.pull[i] + if up { step } else { -step };
            // Whole notches, so that as many up as down ends where it began.
            self.pull[i] = ((wanted / step).round() * step).clamp(step, 1.0);
        }
        self.pull_used = now;
    }

    /// How far the keys for L2 and R2 pull them right now, 0..1.
    pub fn pull(&self) -> [f32; 2] {
        self.pull
    }

    fn switch_for(&mut self, input: Input) -> Option<Mode> {
        let before = self.mode;
        // Read the actions of the layout in use when the key went down: one press, one switch.
        let actions = self.layout.actions(before, input).to_vec();
        let mut to = before;
        for act in &actions {
            match act {
                Action::ModeToggle => to = before.other(),
                Action::ModeFoot => to = Mode::Foot,
                Action::ModeDrive => to = Mode::Drive,
                _ => {}
            }
        }
        // A key that also presses a button is the get-in key when it would switch to driving.
        let gets_in = to == Mode::Drive && actions.iter().any(|a| !a.switches_layout());
        if self.hold_get_in && gets_in {
            return None;
        }
        self.mode = to;
        (to != before).then_some(to)
    }

    /// The keys and buttons held right now, one bit per input.
    pub fn held_bits(&self) -> [u64; Input::COUNT.div_ceil(64)] {
        let mut bits = [0u64; Input::COUNT.div_ceil(64)];
        for (i, _) in self.held.iter().enumerate().filter(|(_, h)| **h) {
            bits[i / 64] |= 1 << (i % 64);
        }
        for (input, _) in &self.pulses {
            bits[input.index() / 64] |= 1 << (input.index() % 64);
        }
        bits
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
            for act in self.doing(input) {
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
        // The triggers go as far as the dial says, and the dial lets go of itself: a pull left
        // at a third would make the next shot or the next start a puzzle.
        let pulled = [pad.pressed(Button::L2), pad.pressed(Button::R2)];
        match self.layout.dial {
            Some(_) if pulled != [false, false] => self.pull_used = now,
            Some((_, reset_after)) if now - self.pull_used <= reset_after as f64 => {}
            _ => self.pull = [1.0; 2],
        }
        if pulled[0] {
            pad.l2 = self.pull[0];
        }
        if pulled[1] {
            pad.r2 = self.pull[1];
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

    fn mapper_with_dial(change: impl Fn(&mut Config)) -> Mapper {
        let mut cfg = Config::default();
        cfg.triggers.wheel_dial = true;
        change(&mut cfg);
        Mapper::new(Layout::compile(&cfg).0, true)
    }

    /// The pull of L2 and R2 in tenths, as the controller would get them with both keys held.
    fn tenths(m: &Mapper) -> [i32; 2] {
        m.pull().map(|v| (v * 10.0).round() as i32)
    }

    #[test]
    fn the_wheel_does_not_touch_the_triggers_unless_it_is_their_dial() {
        let mut m = mapper();
        m.input(Input::WHEEL_DOWN, true, 0.0);
        m.input(Input::MOUSE_LEFT, true, 0.0);
        m.input(Input::WHEEL_DOWN, true, 0.0);
        assert_eq!(m.state(0.0, 0.001, (0.0, 0.0)).r2, 1.0);
    }

    #[test]
    fn the_dial_sets_the_pull_ahead_of_a_press_and_during_it() {
        let mut m = mapper_with_dial(|_| {});
        // Nothing held: both triggers, ahead of the press.
        for _ in 0..7 {
            m.input(Input::WHEEL_DOWN, true, 0.0);
        }
        assert_eq!(tenths(&m), [3, 3]);
        m.input(Input::MOUSE_LEFT, true, 0.1); // R2
        let p = m.state(0.1, 0.001, (0.0, 0.0));
        assert!(p.pressed(Button::R2) && (p.r2 - 0.3).abs() < 1e-5, "{}", p.r2);
        assert_eq!(p.l2, 0.0, "a trigger nobody presses stays up");
        // While R2 is held the wheel squeezes R2 only.
        m.input(Input::WHEEL_UP, true, 0.2);
        m.input(Input::WHEEL_UP, true, 0.2);
        assert_eq!(tenths(&m), [3, 5]);
        // Then L2 as well: it was pressed last, so the wheel is its dial now, and each keeps its own pull.
        m.input(Input::MOUSE_RIGHT, true, 0.3);
        m.input(Input::WHEEL_DOWN, true, 0.3);
        assert_eq!(tenths(&m), [2, 5]);
        let p = m.state(0.3, 0.001, (0.0, 0.0));
        assert!((p.l2 - 0.2).abs() < 1e-5 && (p.r2 - 0.5).abs() < 1e-5);
        // Never below one notch, never above all the way.
        for _ in 0..20 {
            m.input(Input::WHEEL_DOWN, true, 0.4);
        }
        assert_eq!(tenths(&m), [1, 5]);
        for _ in 0..20 {
            m.input(Input::WHEEL_UP, true, 0.4);
        }
        assert_eq!(tenths(&m), [10, 5]);
    }

    #[test]
    fn the_pull_is_all_the_way_again_a_while_after_the_triggers_are_let_go() {
        let mut m = mapper_with_dial(|_| {});
        m.input(Input::WHEEL_DOWN, true, 0.0);
        m.input(Input::MOUSE_LEFT, true, 1.0);
        assert!((m.state(50.0, 0.001, (0.0, 0.0)).r2 - 0.9).abs() < 1e-5, "it stays for as long as the key is held");
        m.input(Input::MOUSE_LEFT, false, 50.0);
        m.state(52.0, 0.001, (0.0, 0.0));
        assert_eq!(tenths(&m), [9, 9], "and a little longer, for the next press");
        m.state(53.5, 0.001, (0.0, 0.0));
        assert_eq!(tenths(&m), [10, 10]);
        // Turning the dial off puts it back at once.
        m.input(Input::WHEEL_DOWN, true, 60.0);
        assert_eq!(tenths(&m), [9, 9]);
        m.set_layout(Layout::compile(&Config::default()).0, true);
        m.state(60.0, 0.001, (0.0, 0.0));
        assert_eq!(tenths(&m), [10, 10]);
    }

    #[test]
    fn a_wheel_direction_with_a_job_keeps_it_until_a_trigger_is_held() {
        let mut m = mapper_with_dial(|cfg| cfg.bind(Mode::Foot, Input::WHEEL_DOWN, &[Action::Press(Button::DpadDown)]));
        m.input(Input::WHEEL_DOWN, true, 0.0);
        assert!(m.state(0.01, 0.001, (0.0, 0.0)).pressed(Button::DpadDown));
        assert_eq!(tenths(&m), [10, 10]);
        m.input(Input::WHEEL_UP, true, 0.0);
        assert_eq!(tenths(&m), [10, 10], "already all the way");
        m.input(Input::MOUSE_LEFT, true, 1.0);
        m.input(Input::WHEEL_DOWN, true, 1.0);
        assert_eq!(tenths(&m), [10, 9]);
        assert!(!m.state(1.01, 0.001, (0.0, 0.0)).pressed(Button::DpadDown));
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
    fn when_the_game_asks_for_the_left_stick_the_driving_keys_give_it() {
        let mut m = mapper();
        m.set_mode(Mode::Drive);
        m.input(key("W"), true, 0.0);
        m.input(key("S"), true, 0.0);
        let p = m.state(0.0, 0.001, (0.0, 0.0));
        assert!(p.pressed(Button::R2) && p.pressed(Button::L2), "driving: W and S are throttle and brake");
        m.input(key("S"), false, 0.0);
        m.set_stick_wanted(true);
        m.input(key("Space"), true, 0.0);
        let p = m.state(0.0, 0.001, (0.0, 0.0));
        assert!(!p.pressed(Button::R2), "hot-wiring: W is the stick, not the throttle");
        assert_eq!(p.ly, 1.0);
        assert!(p.pressed(Button::Cross), "keys that are not the stick on foot keep their driving job");
        m.set_stick_wanted(false);
        assert!(m.state(0.0, 0.001, (0.0, 0.0)).pressed(Button::R2), "prompt gone: W is the throttle again");
        // On foot nothing changes either way.
        m.set_mode(Mode::Foot);
        m.set_stick_wanted(true);
        assert_eq!(m.state(0.0, 0.001, (0.0, 0.0)).ly, 1.0);
    }

    #[test]
    fn the_get_in_key_can_leave_the_switch_to_something_else() {
        let mut m = mapper();
        m.set_hold_get_in(true);
        assert_eq!(m.input(key("F"), true, 0.0), None, "getting in: no switch yet");
        assert_eq!(m.mode(), Mode::Foot);
        assert!(m.state(0.0, 0.001, (0.0, 0.0)).pressed(Button::Triangle), "the key still presses its button");
        m.input(key("F"), false, 0.0);
        assert_eq!(m.input(key("CapsLock"), true, 0.0), Some(Mode::Drive), "switching by hand still works");
        assert_eq!(m.input(key("F"), true, 0.0), Some(Mode::Foot), "getting out switches at once");
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
