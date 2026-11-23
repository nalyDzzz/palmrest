//! Which key and mouse events are taken away from Windows, and what they turn into.
//!
//! This is the decision only, with no Windows calls, so it can be tested. The rules:
//! - Nothing is taken unless the takeover is live (turned on, and a game window is in front).
//! - Only keys that are bound are taken. Alt combinations always pass, so Alt+Tab keeps working.
//! - A release is taken only if its press was taken, so no program ever sees half a key press.
//! - The on/off key works whenever the takeover is on or a game window is in front.

use crate::keys::Input;

/// What the rest of the program is told about an event.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Out {
    Nothing,
    /// The on/off key was pressed.
    Toggle,
    /// A bound key or mouse button went down or up.
    Input { input: Input, down: bool },
    /// The wheel turned one notch.
    Wheel(Input),
}

/// The situation an event arrives in.
#[derive(Clone, Copy, Debug)]
pub struct Env {
    /// Turned on and a game window is in front: keys and mouse belong to the controller.
    pub live: bool,
    /// Turned on, or a game window is in front: the on/off key is ours.
    pub can_toggle: bool,
    pub toggle: Input,
}

pub struct Hook {
    taken: [bool; Input::COUNT],
    toggle_down: bool,
}

impl Default for Hook {
    fn default() -> Self {
        Hook::new()
    }
}

impl Hook {
    pub const fn new() -> Hook {
        Hook { taken: [false; Input::COUNT], toggle_down: false }
    }

    /// A key event. Returns whether to take it away from Windows, and what to report.
    pub fn key(&mut self, env: &Env, bound: impl Fn(Input) -> bool, input: Input, down: bool, alt: bool) -> (bool, Out) {
        if input == env.toggle {
            if !env.can_toggle && !self.toggle_down {
                return (false, Out::Nothing);
            }
            let fire = down && !self.toggle_down;
            self.toggle_down = down;
            return (true, if fire { Out::Toggle } else { Out::Nothing });
        }
        let i = input.index();
        if down {
            if env.live && !alt && bound(input) {
                let first = !std::mem::replace(&mut self.taken[i], true);
                return (true, if first { Out::Input { input, down: true } } else { Out::Nothing });
            }
            return (false, Out::Nothing);
        }
        if std::mem::replace(&mut self.taken[i], false) {
            return (true, Out::Input { input, down: false });
        }
        (false, Out::Nothing)
    }

    /// A mouse button event. While live every click is taken, bound or not, so the game window
    /// underneath never reacts to it.
    pub fn button(&mut self, env: &Env, bound: impl Fn(Input) -> bool, input: Input, down: bool) -> (bool, Out) {
        let i = input.index();
        if down {
            if !env.live {
                return (false, Out::Nothing);
            }
            self.taken[i] = true;
            return (true, if bound(input) { Out::Input { input, down: true } } else { Out::Nothing });
        }
        if std::mem::replace(&mut self.taken[i], false) {
            return (true, if bound(input) { Out::Input { input, down: false } } else { Out::Nothing });
        }
        (false, Out::Nothing)
    }

    /// A wheel notch.
    pub fn wheel(&mut self, env: &Env, bound: impl Fn(Input) -> bool, input: Input) -> (bool, Out) {
        if !env.live {
            return (false, Out::Nothing);
        }
        (true, if bound(input) { Out::Wheel(input) } else { Out::Nothing })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: Input = Input(b'W' as u16);
    const K: Input = Input(b'K' as u16);
    const F10: Input = Input(0x79);

    fn env(live: bool, can_toggle: bool) -> Env {
        Env { live, can_toggle, toggle: F10 }
    }

    fn bound(i: Input) -> bool {
        i == W || i == Input::MOUSE_LEFT
    }

    #[test]
    fn nothing_is_taken_when_not_live() {
        let mut h = Hook::default();
        assert_eq!(h.key(&env(false, false), bound, W, true, false), (false, Out::Nothing));
        assert_eq!(h.key(&env(false, false), bound, W, false, false), (false, Out::Nothing));
        assert_eq!(h.button(&env(false, false), bound, Input::MOUSE_LEFT, true), (false, Out::Nothing));
        assert_eq!(h.wheel(&env(false, false), bound, Input::WHEEL_UP), (false, Out::Nothing));
    }

    #[test]
    fn bound_keys_are_taken_while_live_and_repeats_are_quiet() {
        let mut h = Hook::default();
        let e = env(true, true);
        assert_eq!(h.key(&e, bound, W, true, false), (true, Out::Input { input: W, down: true }));
        assert_eq!(h.key(&e, bound, W, true, false), (true, Out::Nothing), "key repeat");
        assert_eq!(h.key(&e, bound, W, false, false), (true, Out::Input { input: W, down: false }));
        assert_eq!(h.key(&e, bound, K, true, false), (false, Out::Nothing), "unbound keys pass");
    }

    #[test]
    fn a_release_follows_its_press_even_after_focus_is_lost() {
        let mut h = Hook::default();
        h.key(&env(true, true), bound, W, true, false);
        // The game window lost focus while W was held: the release is still ours.
        assert_eq!(h.key(&env(false, true), bound, W, false, false), (true, Out::Input { input: W, down: false }));
        // And a press that passed through has its release pass through too.
        h.key(&env(false, true), bound, W, true, false);
        assert_eq!(h.key(&env(true, true), bound, W, false, false), (false, Out::Nothing));
    }

    #[test]
    fn alt_combinations_always_pass() {
        let mut h = Hook::default();
        assert_eq!(h.key(&env(true, true), bound, W, true, true), (false, Out::Nothing));
    }

    #[test]
    fn the_toggle_key_fires_once_per_press_and_only_when_it_is_ours() {
        let mut h = Hook::default();
        assert_eq!(h.key(&env(false, false), bound, F10, true, false), (false, Out::Nothing), "F10 belongs to other programs");
        assert_eq!(h.key(&env(false, false), bound, F10, false, false), (false, Out::Nothing));
        assert_eq!(h.key(&env(false, true), bound, F10, true, false), (true, Out::Toggle));
        assert_eq!(h.key(&env(true, true), bound, F10, true, false), (true, Out::Nothing), "held");
        assert_eq!(h.key(&env(true, true), bound, F10, false, false), (true, Out::Nothing));
        assert_eq!(h.key(&env(true, true), bound, F10, true, false), (true, Out::Toggle));
    }

    #[test]
    fn every_click_is_taken_while_live() {
        let mut h = Hook::default();
        let e = env(true, true);
        assert_eq!(h.button(&e, bound, Input::MOUSE_LEFT, true), (true, Out::Input { input: Input::MOUSE_LEFT, down: true }));
        assert_eq!(h.button(&e, bound, Input::MOUSE_LEFT, false), (true, Out::Input { input: Input::MOUSE_LEFT, down: false }));
        assert_eq!(h.button(&e, bound, Input::MOUSE_MIDDLE, true), (true, Out::Nothing), "unbound click is still kept from the game window");
        assert_eq!(h.button(&env(false, true), bound, Input::MOUSE_MIDDLE, false), (true, Out::Nothing));
        assert_eq!(h.wheel(&e, bound, Input::WHEEL_UP), (true, Out::Nothing));
    }
}
