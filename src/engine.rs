//! The loop that turns keyboard and mouse events into controller output, about 1000 times a second.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::aim::{Aim, AimParams};
use crate::config::{Action, Config, Layout, Mode};
use crate::gamepad::{self, Reading, Real};
use crate::keys::Input;
use crate::mapper::Mapper;
use crate::output::{Output, Sent, VirtualDs4};
use crate::overlay::{OverlayState, Spot};
use crate::prompts::{Found, Glyph, Steady, Tracks};
use crate::pad::{Button, PadState};
use crate::win;

/// Time between passes while playing.
const TICK: f64 = 0.001;
/// Time between passes while there is nothing to send.
const IDLE_TICK: f64 = 0.02;
/// How often the window in front is checked.
const FOREGROUND_EVERY: f64 = 0.05;
/// An unchanged controller state is sent again this often.
const RESEND_EVERY: f64 = 0.1;
/// A key hint stays this long after its prompt was last seen, or a look and a half if that is
/// longer: prompts drop out for a moment while the camera moves.
const HINT_STAYS: f64 = 0.3;
/// A prompt listed in the label is dropped after going unseen for about this long.
const LISTED_STAYS: f64 = 0.75;
/// Mouse movement of at least this many counts within this many seconds means a hand is on the
/// mouse. Less can be the desk being bumped while the real controller is held.
const MOUSE_IN_HAND: (i32, f64) = (40, 0.25);

/// Test aid (`--latency-test`): when `Some`, every controller report caused by a key, mouse
/// button or wheel event is noted as (when the event reached the hook, when the driver took the
/// report), in `win::now` seconds.
pub static TIMINGS: Mutex<Option<Vec<(f64, f64)>>> = Mutex::new(None);

/// Test aid (`--latency-test`): when `Some`, every mouse movement report taken while live, as
/// (when it reached the app in `win::now` seconds, dx, dy).
pub static MOUSE_LOG: Mutex<Option<Vec<(f64, i32, i32)>>> = Mutex::new(None);

fn note_timing(event: f64) {
    if let Ok(mut timings) = TIMINGS.lock() {
        if let Some(list) = timings.as_mut() {
            list.push((event, win::now()));
        }
    }
}

/// Everything sent to the engine: input events from the hooks, and requests from the window.
#[derive(Debug)]
pub enum Msg {
    Mouse { t: f64, dx: i32, dy: i32 },
    Input { t: f64, input: Input, down: bool },
    Wheel { t: f64, input: Input },
    /// The on/off key was pressed.
    Toggle,
    Arm(bool),
    SetConfig(Box<Config>),
    SetMode(Mode),
    /// Hold the right stick at this tilt for a while, to find the game's dead zone.
    TestTilt { tilt: f32, seconds: f64 },
    ReconnectOutput,
    /// The save-picture key was pressed.
    SavePicture,
    /// A picture was saved (its path), or could not be (why).
    PictureSaved(Result<String, String>),
    /// What one look at the game picture found (`None`: the picture could not be read).
    Look(Option<Look>),
    /// What a real controller connected to the computer is doing (`None`: it has gone).
    Controller(Option<Reading>),
    Quit,
}

/// What one look at the game picture found.
#[derive(Debug)]
pub struct Look {
    /// The button prompts on screen, with where each is in the game window.
    pub prompts: Vec<Found>,
    /// The vehicle name card is showing.
    pub name_card: bool,
    /// How long copying the picture and searching it took, in seconds.
    pub took: f32,
}

/// What the window shows.
#[derive(Clone, Debug)]
pub struct Status {
    /// Turned on by the user.
    pub armed: bool,
    /// A game window is in front.
    pub front: bool,
    /// Turned on and in front: keys and mouse are the controller.
    pub live: bool,
    pub mode: Mode,
    pub pad: PadState,
    /// Why the controller output is not working, if it is not.
    pub output_error: Option<String>,
    /// Entries in the config file that were not understood.
    pub warnings: Vec<String>,
    /// Mouse reports per second while moving, for the user's information.
    pub mouse_rate: f32,
    pub testing: bool,
    /// The keys and mouse buttons held right now, one bit per input.
    pub held: [u64; Input::COUNT.div_ceil(64)],
    /// How the last save-picture press went, and how many pictures were saved this session.
    pub picture: Option<Result<String, String>>,
    pub pictures_saved: u32,
    /// How many times a second the game picture is really being searched for prompts (0: not
    /// at the moment), and how long one search takes, in milliseconds.
    pub checks_per_second: f32,
    pub check_ms: f32,
    /// The real controller whose input is being added, if one is connected and wanted.
    pub controller: Option<&'static str>,
}

impl Status {
    pub fn is_held(&self, input: Input) -> bool {
        self.held.get(input.index() / 64).is_some_and(|w| w >> (input.index() % 64) & 1 == 1)
    }
}

pub struct Engine {
    tx: Sender<Msg>,
    status: Arc<Mutex<Status>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Engine {
    pub fn start(cfg: Config) -> Result<Engine, String> {
        let (tx, rx) = channel();
        let (layout, warnings) = Layout::compile(&cfg);
        win::set_bound(&layout);
        let status = Arc::new(Mutex::new(Status {
            armed: false,
            front: false,
            live: false,
            mode: Mode::Foot,
            pad: PadState::default(),
            output_error: None,
            warnings,
            mouse_rate: 0.0,
            testing: false,
            held: [0; Input::COUNT.div_ceil(64)],
            picture: None,
            pictures_saved: 0,
            checks_per_second: 0.0,
            check_ms: 0.0,
            controller: None,
        }));
        win::start_input(tx.clone())?;
        let dry = win::DRY.load(std::sync::atomic::Ordering::Relaxed);
        let virtual_too = dry && gamepad::VIRTUAL_TOO.load(std::sync::atomic::Ordering::Relaxed);
        if !dry || virtual_too {
            gamepad::set_wanted(cfg.controller.pass_through);
            gamepad::start(tx.clone(), virtual_too);
        }
        let shared = status.clone();
        let own = tx.clone();
        // The label on top of the game. If it cannot start, the app works without it.
        let overlay = Arc::new(Mutex::new(OverlayState::default()));
        let _ = crate::overlay::start(overlay.clone());
        let looker = win::start_prompt_watcher(tx.clone());
        let thread = std::thread::Builder::new()
            .name("engine".into())
            .spawn(move || run(rx, own, shared, cfg, layout, overlay, looker))
            .map_err(|e| e.to_string())?;
        Ok(Engine { tx, status, thread: Some(thread) })
    }

    pub fn send(&self, msg: Msg) {
        let _ = self.tx.send(msg);
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }

    /// Stops the engine: everything is released and the virtual controller unplugged.
    pub fn stop(&mut self) {
        self.send(Msg::Quit);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop();
    }
}

/// How long a hint stays after its prompt was last seen, when looks come `gap` seconds apart.
pub fn hint_stays(gap: f64) -> f64 {
    (1.5 * gap).max(HINT_STAYS)
}

fn connect() -> Result<Box<dyn Output>, String> {
    if win::DRY.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("no controller: the window was opened only to be looked at (--dry)".to_string());
    }
    VirtualDs4::connect().map(|d| Box::new(d) as Box<dyn Output>)
}

/// The controller button a prompt's mark stands for. `None` for the sticks, which are not buttons.
fn button_of(glyph: Glyph) -> Option<Button> {
    Some(match glyph {
        Glyph::Triangle => Button::Triangle,
        Glyph::Circle => Button::Circle,
        Glyph::Cross => Button::Cross,
        Glyph::Square => Button::Square,
        Glyph::L1 => Button::L1,
        Glyph::R1 => Button::R1,
        Glyph::L2 => Button::L2,
        Glyph::R2 => Button::R2,
        Glyph::L3 => Button::L3,
        Glyph::R3 => Button::R3,
        Glyph::LeftStick | Glyph::RightStick => return None,
    })
}

/// The keys to show for a prompt in a layout, nearest the hand first: "E / F", "WASD", "Mouse",
/// with the signs of `Input::sign` for keys that have one. `None` if nothing is bound to it.
pub fn keys_shown(cfg: &Config, mode: Mode, glyph: Glyph) -> Option<String> {
    match (glyph, button_of(glyph)) {
        (_, Some(button)) => {
            let keys: Vec<String> = cfg.keys_for(mode, Action::Press(button)).into_iter().take(2).map(|k| k.sign()).collect();
            (!keys.is_empty()).then(|| keys.join(" / "))
        }
        (Glyph::RightStick, None) => Some("Mouse".to_string()),
        (_, None) => {
            // A left-stick prompt on screen hands the stick to the on-foot keys in either layout
            // (hot-wiring from the driver's seat: `Mapper::set_stick_wanted`), so those are the ones to show.
            let keys: Vec<String> = [Action::LsUp, Action::LsLeft, Action::LsDown, Action::LsRight]
                .into_iter()
                .filter_map(|a| cfg.keys_for(Mode::Foot, a).first().map(|k| k.sign()))
                .collect();
            let letters = keys.iter().all(|k| k.chars().count() == 1);
            (!keys.is_empty()).then(|| keys.join(if letters { "" } else { " " }))
        }
    }
}

/// The one key to draw on a prompt itself: the first of its keys, the one nearest the hand.
pub fn key_on_prompt(cfg: &Config, mode: Mode, glyph: Glyph) -> Option<String> {
    match button_of(glyph) {
        Some(button) => cfg.keys_for(mode, Action::Press(button)).first().map(|k| k.sign()),
        None => keys_shown(cfg, mode, glyph),
    }
}

#[allow(clippy::too_many_arguments)]
fn run(
    rx: Receiver<Msg>,
    tx: Sender<Msg>,
    shared: Arc<Mutex<Status>>,
    mut cfg: Config,
    layout: Layout,
    overlay: Arc<Mutex<OverlayState>>,
    looker: std::sync::mpsc::SyncSender<isize>,
) {
    win::fine_timers();
    let mut warnings = shared.lock().map(|s| s.warnings.clone()).unwrap_or_default();
    let mut mapper = Mapper::new(layout, cfg.steering.smooth);
    let mut aim = Aim::new();
    let mut output = connect();
    let (mut armed, mut front, mut live) = (false, false, false);
    let mut test: Option<(f32, f64)> = None;
    let mut last_step = win::now();
    let (mut next_foreground, mut next_status) = (0.0f64, 0.0f64);
    let mut last_sent: Option<(PadState, f64)> = None;
    // The first key or button event whose report has not been taken by the driver yet.
    let mut waiting_since: Option<f64> = None;
    let (mut mouse_reports, mut moving_since, mut mouse_rate) = (0u32, None::<f64>, 0.0f32);
    let mut game_window = 0isize;
    let (mut picture, mut pictures_saved) = (None::<Result<String, String>>, 0u32);
    let (mut prompts, mut next_look) = (Steady::default(), 0.0f64);
    // The name card: looks in a row that saw it, and whether it counts as showing.
    let (mut card_looks, mut card_showing) = (0u8, false);
    // The prompts on screen, followed from look to look, for the hints drawn on them.
    let mut tracks = Tracks::default();
    // When the last look came in, the usual time from one look to the next, and how long a look takes.
    let (mut last_look, mut look_gap, mut look_took) = (0.0f64, 0.0f64, 0.0f32);
    mapper.set_hold_get_in(cfg.switch_on_name_card);
    // A real controller connected to the computer, and when the keys or the mouse were last used.
    let mut real = Real::default();
    let mut keys_used = 0.0f64;
    // Mouse movement lately: since when it is being added up, and how many counts so far.
    let mut mouse_moved = (0.0f64, 0i32);

    'run: loop {
        // Full speed while playing, testing, while the last state still has to be delivered, or
        // while a press on the real controller waits to be passed on.
        let busy = live || test.is_some() || last_sent.is_none() || real.waiting(win::now());
        let tick = if busy { TICK } else { IDLE_TICK };
        let wait = (last_step + tick - win::now()).max(0.0);
        let mut next = match rx.recv_timeout(Duration::from_secs_f64(wait)) {
            Ok(msg) => Some(msg),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        // Buttons go out at once; mouse movement waits for the next 1 ms pass.
        let mut urgent = false;
        while let Some(msg) = next {
            match msg {
                Msg::Mouse { t, dx, dy } => {
                    if live {
                        aim.feed(t, dx, dy);
                        if let Ok(mut log) = MOUSE_LOG.lock() {
                            if let Some(list) = log.as_mut() {
                                list.push((t, dx, dy));
                            }
                        }
                        mouse_reports += 1;
                        moving_since.get_or_insert(t);
                        if t - mouse_moved.0 > MOUSE_IN_HAND.1 {
                            mouse_moved = (t, 0);
                        }
                        mouse_moved.1 += dx.abs() + dy.abs();
                        if mouse_moved.1 >= MOUSE_IN_HAND.0 {
                            keys_used = t;
                        }
                    }
                }
                Msg::Input { t, input, down } => {
                    // A release is always passed on, so a key held while focus was lost lets go.
                    if live || !down {
                        if let Some(mode) = mapper.input(input, down, t) {
                            mode_sound(&cfg, mode);
                        }
                        urgent = true;
                        waiting_since.get_or_insert(t);
                        if down {
                            keys_used = t;
                        }
                    }
                }
                Msg::Wheel { t, input } => {
                    if live {
                        if let Some(mode) = mapper.input(input, true, t) {
                            mode_sound(&cfg, mode);
                        }
                        urgent = true;
                        waiting_since.get_or_insert(t);
                        keys_used = t;
                    }
                }
                Msg::Toggle => {
                    armed = !armed;
                    if cfg.sounds {
                        win::beep(if armed { 880 } else { 440 }, 60);
                    }
                    next_foreground = 0.0;
                    urgent = true;
                }
                Msg::Arm(on) => {
                    armed = on;
                    next_foreground = 0.0;
                    urgent = true;
                }
                Msg::SetConfig(new) => {
                    cfg = *new;
                    let (layout, w) = Layout::compile(&cfg);
                    win::set_bound(&layout);
                    mapper.set_layout(layout, cfg.steering.smooth);
                    mapper.set_hold_get_in(cfg.switch_on_name_card);
                    gamepad::set_wanted(cfg.controller.pass_through);
                    warnings = w;
                    next_foreground = 0.0;
                }
                Msg::SetMode(mode) => {
                    mapper.set_mode(mode);
                    urgent = true;
                }
                Msg::TestTilt { tilt, seconds } => {
                    test = Some((tilt.clamp(-1.0, 1.0), win::now() + seconds.clamp(0.0, 30.0)));
                    urgent = true;
                }
                Msg::ReconnectOutput => {
                    drop(std::mem::replace(&mut output, Err(String::new()))); // unplug the old one first
                    output = connect();
                    last_sent = None;
                }
                Msg::SavePicture => {
                    if live && game_window != 0 {
                        // Copying and writing the picture takes a moment: not on this thread.
                        let name = format!("{}_{}.png", unix_seconds(), if mapper.mode() == Mode::Drive { "driving" } else { "on_foot" });
                        let (path, hwnd, done, sounds) = (crate::config::pictures_dir().join(name), game_window, tx.clone(), cfg.sounds);
                        std::thread::spawn(move || {
                            let result = win::save_window_picture(hwnd, &path);
                            if sounds {
                                win::beep(if result.is_ok() { 1500 } else { 300 }, 50);
                            }
                            let _ = done.send(Msg::PictureSaved(result));
                        });
                    }
                }
                Msg::Look(look) => {
                    // A look that could not read the picture says nothing either way.
                    if let (true, Some(look)) = (live, look) {
                        let mut marks: Vec<Glyph> = look.prompts.iter().map(|f| f.glyph).collect();
                        marks.sort();
                        marks.dedup();
                        // How fast the looks really come: as asked, or as fast as the picture can be searched.
                        let now = win::now();
                        let gap = (now - last_look).clamp(0.0, 1.0);
                        look_gap = if look_gap == 0.0 || last_look == 0.0 { 1.0 / cfg.overlay.checks_per_second as f64 } else { 0.8 * look_gap + 0.2 * gap };
                        last_look = now;
                        look_took = if look_took == 0.0 { look.took } else { 0.8 * look_took + 0.2 * look.took };
                        prompts.set_patience((LISTED_STAYS / look_gap).ceil().min(200.0) as u8);
                        prompts.look(&marks);
                        tracks.look(&look.prompts, now, hint_stays(look_gap));
                        next_status = 0.0; // tell the overlay at once: a hint should not trail its prompt
                        // Hot-wiring and the like: while the game shows the left stick, give it the stick.
                        mapper.set_stick_wanted(prompts.shown().contains(&Glyph::LeftStick));
                        // The card counts as showing after two looks in a row, and acts once, when it appears.
                        card_looks = if look.name_card { card_looks.saturating_add(1) } else { 0 };
                        let showing = card_looks >= 2;
                        if showing && !card_showing && cfg.switch_on_name_card && mapper.mode() == Mode::Foot {
                            mapper.set_mode(Mode::Drive);
                            mode_sound(&cfg, Mode::Drive);
                        }
                        card_showing = showing;
                    }
                }
                Msg::PictureSaved(result) => {
                    pictures_saved += result.is_ok() as u32;
                    picture = Some(result);
                }
                Msg::Controller(reading) => {
                    real.heard(reading, win::now());
                    urgent = true; // a stick moved or a button changed: out at once, like a key
                }
                Msg::Quit => break 'run,
            }
            next = rx.try_recv().ok();
        }

        let now = win::now();
        if !urgent && now - last_step < tick {
            continue;
        }
        let dt = now - last_step;
        last_step = now;
        win::heartbeat();

        if now >= next_foreground {
            next_foreground = now + FOREGROUND_EVERY;
            let (hwnd, program) = win::foreground();
            front = cfg.targets.iter().any(|t| *t == program);
            let want = armed && front;
            if want != live {
                live = want;
                if !live {
                    mapper.release_all();
                    aim.reset();
                    win::release_cursor();
                    prompts.clear();
                    mapper.set_stick_wanted(false);
                    (card_looks, card_showing) = (0, false);
                    tracks.clear();
                    (last_look, look_gap, look_took) = (0.0, 0.0, 0.0);
                }
            }
            game_window = if live { hwnd } else { 0 };
            win::set_live(live, armed || front);
            if live {
                win::confine_cursor(hwnd); // again on every check: Windows drops the confinement easily
            }
        }

        // The picture is looked at for prompt hints, and for the name card if that switches layout.
        let hints = cfg.overlay.enabled && cfg.overlay.prompts;
        if live && (hints || cfg.switch_on_name_card) && now >= next_look {
            next_look = now + 1.0 / cfg.overlay.checks_per_second as f64;
            let _ = looker.try_send(game_window); // dropped if the last look is still running
        }
        if !hints {
            prompts.clear();
            mapper.set_stick_wanted(false);
            tracks.clear();
        }

        let mut pad = PadState::default();
        if live {
            let params = AimParams::new(&cfg.aim, cfg.game_fps, mapper.aiming());
            let stick = aim.step(now, dt, &params);
            pad = mapper.state(now, dt, stick);
        }
        if let Some((tilt, until)) = test {
            if now < until {
                pad.rx = tilt;
            } else {
                test = None;
            }
        }

        // The real controller, while it is the one in the hands, is added to whatever the keys
        // and the mouse are doing, and also while they are doing nothing at all: turned off, or
        // with another window in front. Whether the game gets it then is up to the Remote Play
        // program, as for any controller.
        let pad = real.added_to(pad, cfg.controller.pass_through, keys_used, now);
        // And while it is, nothing is drawn on the game: a key on a prompt would hide the
        // button to press.
        let in_hands = real.in_hands(keys_used);

        let resend = last_sent.is_none_or(|(sent, at)| sent != pad || now - at >= RESEND_EVERY);
        if resend {
            last_sent = Some((pad, now));
            if let Ok(out) = &mut output {
                match out.send(&pad) {
                    Ok(Sent::Taken) => {
                        if let Some(t) = waiting_since.take() {
                            note_timing(t);
                        }
                    }
                    Ok(Sent::Again) => last_sent = None, // not delivered: the next pass sends it again
                    Err(e) => output = Err(e),
                }
            }
        } else {
            waiting_since = None; // the event changed nothing that is sent
        }

        if now >= next_status {
            next_status = now + 0.016;
            if let Some(since) = moving_since {
                if now - since >= 0.5 {
                    mouse_rate = mouse_reports as f32 / (now - since) as f32;
                    mouse_reports = 0;
                    moving_since = None;
                }
            }
            if let Ok(mut o) = overlay.lock() {
                let mode = mapper.mode();
                // Prompts that can be followed get their key drawn on them; the others are listed in the label.
                let spots: Vec<Spot> = if cfg.overlay.on_prompt {
                    tracks
                        .there(now, hint_stays(look_gap))
                        .iter()
                        .filter_map(|f| {
                            let key = key_on_prompt(&cfg, mode, f.glyph)?;
                            Some(Spot { glyph: f.glyph, key, x: f.x.round() as i32, y: f.y.round() as i32, diameter: f.diameter.round() as i32 })
                        })
                        // Any beyond what the overlay can draw are listed in the label instead.
                        .take(crate::overlay::SPOTS)
                        .collect()
                } else {
                    Vec::new()
                };
                let listed = prompts.shown().into_iter().filter(|g| !spots.iter().any(|s| s.glyph == *g));
                let shown = listed.filter_map(|g| keys_shown(&cfg, mode, g).map(|keys| (g, keys))).collect();
                // A trigger set to a part pull is always worth showing: it changes what its key does.
                let pull = [Glyph::L2, Glyph::R2].into_iter().zip(mapper.pull()).filter(|(_, far)| *far < 0.995).map(|(g, far)| (g, format!("{:.0}%", far * 100.0))).collect();
                let next = OverlayState { show: live && cfg.overlay.enabled && !in_hands, game_window, mode, pull, prompts: shown, spots };
                if *o != next {
                    *o = next;
                }
            }
            if let Ok(mut s) = shared.lock() {
                s.armed = armed;
                s.front = front;
                s.live = live;
                s.mode = mapper.mode();
                s.pad = pad;
                s.output_error = output.as_ref().err().cloned();
                s.mouse_rate = mouse_rate;
                s.testing = test.is_some();
                s.controller = real.name();
                s.held = if live { mapper.held_bits() } else { [0; Input::COUNT.div_ceil(64)] };
                let looking = last_look > 0.0 && now - last_look < 1.5 && look_gap > 0.0;
                (s.checks_per_second, s.check_ms) = if looking { ((1.0 / look_gap) as f32, look_took * 1000.0) } else { (0.0, 0.0) };
                if s.pictures_saved != pictures_saved || s.picture != picture {
                    s.pictures_saved = pictures_saved;
                    s.picture = picture.clone();
                }
                if s.warnings != warnings {
                    s.warnings = warnings.clone();
                }
            }
        }
    }

    if let Ok(mut o) = overlay.lock() {
        o.show = false;
    }
    // Leave nothing held: hooks off, cursor free, controller centred (then unplugged when dropped).
    win::set_live(false, false);
    win::release_cursor();
    if let Ok(out) = &mut output {
        let _ = out.send(&PadState::default());
    }
}

fn unix_seconds() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

fn mode_sound(cfg: &Config, mode: Mode) {
    if cfg.sounds {
        match mode {
            Mode::Drive => win::beep(1000, 70),
            Mode::Foot => win::beep(500, 70),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_mark_has_its_keys_and_the_prompt_gets_the_nearest_one() {
        use crate::keys::sign;
        let cfg = Config::default();
        let one = |c: char| Some(c.to_string());
        assert_eq!(keys_shown(&cfg, Mode::Foot, Glyph::Cross), Some(format!("{} / {}", sign::SPACE, sign::ENTER)), "Space is nearer the hand than Enter");
        assert_eq!(key_on_prompt(&cfg, Mode::Foot, Glyph::Cross), one(sign::SPACE));
        assert_eq!(key_on_prompt(&cfg, Mode::Foot, Glyph::Circle).as_deref(), Some("Ctrl"));
        assert_eq!(key_on_prompt(&cfg, Mode::Foot, Glyph::L2), one(sign::MOUSE_RIGHT));
        assert_eq!(key_on_prompt(&cfg, Mode::Foot, Glyph::R2), one(sign::MOUSE_LEFT));
        assert_eq!(key_on_prompt(&cfg, Mode::Drive, Glyph::R2).as_deref(), Some("W"), "in a vehicle R2 is the throttle");
        assert_eq!(key_on_prompt(&cfg, Mode::Drive, Glyph::L2).as_deref(), Some("S"));
        assert_eq!(key_on_prompt(&cfg, Mode::Foot, Glyph::L3), one(sign::SHIFT));
        assert_eq!(key_on_prompt(&cfg, Mode::Foot, Glyph::L1), one(sign::TAB));
        assert_eq!(key_on_prompt(&cfg, Mode::Foot, Glyph::LeftStick).as_deref(), Some("WASD"));
        assert_eq!(key_on_prompt(&cfg, Mode::Drive, Glyph::LeftStick).as_deref(), Some("WASD"), "hot-wiring: the left-stick prompt hands the stick to the on-foot keys");
        assert_eq!(key_on_prompt(&cfg, Mode::Foot, Glyph::RightStick).as_deref(), Some("Mouse"));
        // Every mark the finder knows leads to a key in both default layouts.
        for glyph in Glyph::ALL {
            for mode in [Mode::Foot, Mode::Drive] {
                let keys = keys_shown(&cfg, mode, glyph);
                println!("{glyph:?} {mode:?}: {keys:?} / on the prompt {:?}", key_on_prompt(&cfg, mode, glyph));
                assert!(keys.is_some(), "{glyph:?} in {mode:?}");
            }
        }
    }
}
