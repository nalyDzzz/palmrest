//! The loop that turns keyboard and mouse events into controller output, about 1000 times a second.

use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::aim::{Aim, AimParams};
use crate::config::{Config, Layout, Mode};
use crate::keys::Input;
use crate::mapper::Mapper;
use crate::output::{Output, Sent, VirtualDs4};
use crate::pad::PadState;
use crate::win;

/// Time between passes while playing.
const TICK: f64 = 0.001;
/// Time between passes while there is nothing to send.
const IDLE_TICK: f64 = 0.02;
/// How often the window in front is checked.
const FOREGROUND_EVERY: f64 = 0.05;
/// An unchanged controller state is sent again this often.
const RESEND_EVERY: f64 = 0.1;

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
    Quit,
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
        }));
        win::start_input(tx.clone())?;
        let shared = status.clone();
        let own = tx.clone();
        let thread = std::thread::Builder::new().name("engine".into()).spawn(move || run(rx, own, shared, cfg, layout)).map_err(|e| e.to_string())?;
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

fn connect() -> Result<Box<dyn Output>, String> {
    VirtualDs4::connect().map(|d| Box::new(d) as Box<dyn Output>)
}

fn run(rx: Receiver<Msg>, tx: Sender<Msg>, shared: Arc<Mutex<Status>>, mut cfg: Config, layout: Layout) {
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
    let (mut mouse_reports, mut moving_since, mut mouse_rate) = (0u32, None::<f64>, 0.0f32);
    let mut game_window = 0isize;
    let (mut picture, mut pictures_saved) = (None::<Result<String, String>>, 0u32);

    'run: loop {
        // Full speed while playing, testing, or while the last state still has to be delivered.
        let busy = live || test.is_some() || last_sent.is_none();
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
                        mouse_reports += 1;
                        moving_since.get_or_insert(t);
                    }
                }
                Msg::Input { t, input, down } => {
                    // A release is always passed on, so a key held while focus was lost lets go.
                    if live || !down {
                        if let Some(mode) = mapper.input(input, down, t) {
                            mode_sound(&cfg, mode);
                        }
                        urgent = true;
                    }
                }
                Msg::Wheel { t, input } => {
                    if live {
                        if let Some(mode) = mapper.input(input, true, t) {
                            mode_sound(&cfg, mode);
                        }
                        urgent = true;
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
                Msg::PictureSaved(result) => {
                    pictures_saved += result.is_ok() as u32;
                    picture = Some(result);
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
                }
            }
            game_window = if live { hwnd } else { 0 };
            win::set_live(live, armed || front);
            if live {
                win::confine_cursor(hwnd); // again on every check: Windows drops the confinement easily
            }
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

        let resend = last_sent.is_none_or(|(sent, at)| sent != pad || now - at >= RESEND_EVERY);
        if resend {
            last_sent = Some((pad, now));
            if let Ok(out) = &mut output {
                match out.send(&pad) {
                    Ok(Sent::Taken) => {}
                    Ok(Sent::Again) => last_sent = None, // not delivered: the next pass sends it again
                    Err(e) => output = Err(e),
                }
            }
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
            if let Ok(mut s) = shared.lock() {
                s.armed = armed;
                s.front = front;
                s.live = live;
                s.mode = mapper.mode();
                s.pad = pad;
                s.output_error = output.as_ref().err().cloned();
                s.mouse_rate = mouse_rate;
                s.testing = test.is_some();
                s.held = if live { mapper.held_bits() } else { [0; Input::COUNT.div_ceil(64)] };
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
