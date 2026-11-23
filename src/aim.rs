//! Mouse movement -> right stick.
//!
//! A console game reads the stick once per game frame and turns the camera at a speed set by the
//! tilt. So the stick has to carry the mouse's *speed*. The steps:
//!
//! 1. Mouse counts are kept for one game frame (the window). Their sum over the window is the
//!    speed. One frame is the shortest window in which every count is seen by exactly one of the
//!    game's stick reads, however the reads line up with the mouse reports.
//! 2. A plain window is late at both ends of a movement: the stick only reaches its tilt once the
//!    window has filled, and stays tilted until it has emptied. Once enough counts have arrived
//!    to be sure the hand is really moving, a movement that just began is divided by the time
//!    since it began, and a gap much longer than the gaps between reports is taken as a stop.
//!    A few stray counts get no such trust, which is what keeps a resting hand from shaking the view.
//! 3. Speed becomes tilt through the inverse of the game's own stick curve, starting just past
//!    its dead zone, so camera speed follows hand speed.
//! 4. The tilt glides to each new value: briefly when the movement is trusted, longer when it is not.

use std::collections::VecDeque;

use crate::config::AimConfig;

/// Mouse counts per second that give full tilt at sensitivity 1.
pub const FULL_SPEED_AT_1: f32 = 6000.0;

/// Counts in the window needed for no trust (this many or fewer) and for full trust.
const TRUST_FROM: f32 = 2.0;
const TRUST_FULL: f32 = 12.0;
/// Shortest time a just-begun movement is divided by.
const MIN_SPAN: f64 = 0.004;
/// A trusted movement has stopped after this many usual report gaps without a report...
const STOP_GAPS: f64 = 4.0;
/// ...but never sooner than this (reports can arrive late).
const MIN_STOP: f64 = 0.010;
/// Glide time constants at smoothing 1: for a trusted movement, an untrusted one, and letting go.
const GLIDE_QUICK: f64 = 0.004;
const GLIDE_SLOW: f64 = 0.018;
const GLIDE_RELEASE: f64 = 0.012;
/// Share of full speed needed to start moving the stick, and to keep it moving.
const START_AT: f32 = 0.012;
const KEEP_AT: f32 = 0.004;

/// Everything the conversion needs, already worked out from the settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AimParams {
    /// Horizontal counts per second that mean full tilt.
    pub full_speed: f32,
    pub vertical: f32,
    pub deadzone: f32,
    pub curve: f32,
    /// One game frame, in seconds.
    pub window: f64,
    pub smoothing: f32,
    pub invert_y: bool,
}

impl AimParams {
    pub fn new(cfg: &AimConfig, game_fps: f32, aiming: bool) -> AimParams {
        let speed = cfg.sensitivity.max(0.01) * if aiming { cfg.aim_in.max(0.01) } else { 1.0 };
        AimParams {
            full_speed: FULL_SPEED_AT_1 / speed,
            vertical: cfg.vertical,
            deadzone: cfg.deadzone.clamp(0.0, 0.95),
            curve: cfg.curve.max(0.1),
            window: (1.0 / game_fps.max(1.0) as f64).clamp(0.008, 0.050),
            smoothing: cfg.smoothing.max(0.0),
            invert_y: cfg.invert_y,
        }
    }
}

#[derive(Debug)]
pub struct Aim {
    /// (time, dx, dy) of the mouse reports inside the window.
    ev: VecDeque<(f64, i32, i32)>,
    /// When the last report arrived.
    last: f64,
    /// When the movement now in the window began.
    move_start: f64,
    /// A gap this long after a report starts a new movement.
    stop_after: f64,
    window: f64,
    /// 0..1: how sure the counts in the window make us that this is a real movement.
    trust: f32,
    moving: bool,
    /// The stick now, in mouse sense (x right, y down), -1..1.
    sx: f32,
    sy: f32,
}

impl Default for Aim {
    fn default() -> Self {
        Aim::new()
    }
}

impl Aim {
    pub fn new() -> Aim {
        Aim {
            ev: VecDeque::new(),
            last: f64::NEG_INFINITY,
            move_start: 0.0,
            stop_after: f64::INFINITY,
            window: 1.0 / 30.0,
            trust: 0.0,
            moving: false,
            sx: 0.0,
            sy: 0.0,
        }
    }

    /// Forgets everything: the stick is centred at once.
    pub fn reset(&mut self) {
        *self = Aim::new();
    }

    /// A mouse report: `dx` right, `dy` down, in counts, at time `t` seconds.
    pub fn feed(&mut self, t: f64, dx: i32, dy: i32) {
        if dx == 0 && dy == 0 {
            return;
        }
        if t - self.last > self.stop_after.min(self.window) {
            self.ev.clear();
            self.move_start = t;
        }
        self.ev.push_back((t, dx, dy));
        self.last = t;
    }

    /// Advances to `now` (`dt` seconds since the last step) and returns the right stick as
    /// (x, y) in -1..1, +x right and +y up.
    pub fn step(&mut self, now: f64, dt: f64, p: &AimParams) -> (f32, f32) {
        self.window = p.window;
        while self.ev.front().is_some_and(|e| now - e.0 >= p.window) {
            self.ev.pop_front();
        }
        let (tx, ty) = self.target(now, p);
        let scale = p.smoothing as f64;
        let mut tau = GLIDE_QUICK + (GLIDE_SLOW - GLIDE_QUICK) * (1.0 - self.trust as f64);
        if tx == 0.0 && ty == 0.0 {
            tau = tau.min(GLIDE_RELEASE);
        }
        tau *= scale;
        let k = if tau > 1e-6 { (-dt.max(0.0) / tau).exp() as f32 } else { 0.0 };
        self.sx = self.sx * k + tx * (1.0 - k);
        self.sy = self.sy * k + ty * (1.0 - k);
        if tx == 0.0 && ty == 0.0 && self.sx.abs() < 0.004 && self.sy.abs() < 0.004 {
            self.sx = 0.0;
            self.sy = 0.0;
        }
        (self.sx, if p.invert_y { self.sy } else { -self.sy })
    }

    /// The tilt the counts in the window ask for right now, before the glide.
    fn target(&mut self, now: f64, p: &AimParams) -> (f32, f32) {
        if self.ev.is_empty() {
            self.moving = false;
            return (0.0, 0.0);
        }
        let mut counts = 0.0f32;
        let (mut sum_x, mut sum_y) = (0.0f32, 0.0f32);
        for &(_, dx, dy) in &self.ev {
            counts += (dx.abs() + dy.abs()) as f32;
            sum_x += dx as f32;
            sum_y += dy as f32 * p.vertical;
        }
        let mut span = p.window;
        if self.ev.len() >= 2 {
            let first = self.ev.front().map_or(now, |e| e.0);
            let gap = (self.last - first) / (self.ev.len() - 1) as f64;
            self.trust = ((counts - TRUST_FROM) / (TRUST_FULL - TRUST_FROM)).clamp(0.0, 1.0);
            self.stop_after = (STOP_GAPS * gap).max(MIN_STOP).min(p.window);
            if self.trust > 0.5 && now - self.last > self.stop_after {
                self.moving = false;
                return (0.0, 0.0);
            }
            let began = (now - self.move_start + gap).clamp(MIN_SPAN, p.window);
            span = p.window - (p.window - began) * self.trust as f64;
        } else {
            // One report says nothing about speed yet.
            self.trust = 0.0;
            self.stop_after = p.window;
        }
        if !self.moving && counts < 2.0 {
            return (0.0, 0.0); // a single stray count never moves the stick
        }
        let vx = sum_x / span as f32;
        let vy = sum_y / span as f32;
        let speed = vx.hypot(vy);
        let x = speed / p.full_speed;
        if x < if self.moving { KEEP_AT } else { START_AT } {
            self.moving = false;
            return (0.0, 0.0);
        }
        self.moving = true;
        let tilt = (p.deadzone + (1.0 - p.deadzone) * x.min(1.0).powf(1.0 / p.curve)).min(1.0);
        (vx / speed * tilt, vy / speed * tilt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TICK: f64 = 0.001;

    fn params(sensitivity: f32) -> AimParams {
        AimParams::new(&AimConfig { sensitivity, ..AimConfig::default() }, 30.0, false)
    }

    /// Mouse reports at `hz` from `start` to `stop`, moving at `speed(t)` counts/s (x, y).
    fn trace(speed: impl Fn(f64) -> (f64, f64), start: f64, stop: f64, hz: f64) -> Vec<(f64, i32, i32)> {
        let (mut out, mut ax, mut ay, mut t) = (Vec::new(), 0.0f64, 0.0f64, start);
        while t < stop {
            let (vx, vy) = speed(t);
            ax += vx / hz;
            ay += vy / hz;
            let (dx, dy) = (ax.trunc(), ay.trunc());
            ax -= dx;
            ay -= dy;
            if dx != 0.0 || dy != 0.0 {
                out.push((t, dx as i32, dy as i32));
            }
            t += 1.0 / hz;
        }
        out
    }

    /// Runs the events through the aim at 1 kHz and returns (time, x, y) for every step.
    fn run(events: &[(f64, i32, i32)], total: f64, p: &AimParams) -> Vec<(f64, f32, f32)> {
        let mut aim = Aim::new();
        let (mut out, mut i, mut t) = (Vec::new(), 0, 0.0);
        while t < total {
            while i < events.len() && events[i].0 <= t {
                aim.feed(events[i].0, events[i].1, events[i].2);
                i += 1;
            }
            let (x, y) = aim.step(t, TICK, p);
            out.push((t, x, y));
            t += TICK;
        }
        out
    }

    fn mag(o: &(f64, f32, f32)) -> f32 {
        o.1.hypot(o.2)
    }

    #[test]
    fn a_fast_swipe_starts_and_stops_within_a_frame() {
        let p = params(3.0);
        let out = run(&trace(|_| (4000.0, 0.0), 0.100, 0.400, 1000.0), 0.6, &p);
        let rise = out.iter().find(|o| mag(o) >= 0.9).expect("reaches full tilt").0 - 0.100;
        let fall = out.iter().find(|o| o.0 >= 0.400 && mag(o) <= 0.1).expect("lets go").0 - 0.400;
        assert!(rise < 0.015, "start delay {:.1} ms", rise * 1000.0);
        assert!(fall < 0.025, "stop delay {:.1} ms", fall * 1000.0);
        assert!(out.iter().filter(|o| (0.15..0.39).contains(&o.0)).all(|o| mag(o) > 0.99 && o.1 > 0.0));
    }

    /// Not a check: prints the delays for a report. Run with `cargo test delays -- --nocapture`.
    #[test]
    fn delays_report() {
        for (hz, sens) in [(1000.0, 3.0f32), (1000.0, 12.0), (125.0, 3.0)] {
            let p = params(sens);
            let out = run(&trace(|_| (4000.0, 0.0), 0.100, 0.400, hz), 0.6, &p);
            let rise = out.iter().find(|o| mag(o) >= 0.9).map_or(f64::NAN, |o| o.0 - 0.100);
            let fall = out.iter().find(|o| o.0 >= 0.400 && mag(o) <= 0.1).map_or(f64::NAN, |o| o.0 - 0.400);
            println!("fast swipe, {hz:.0} Hz mouse, speed {sens}: start {:.0} ms, stop {:.0} ms", rise * 1000.0, fall * 1000.0);
        }
    }

    #[test]
    fn steady_speed_gives_the_tilt_the_curve_asks_for() {
        for (sens, share) in [(3.0f32, 0.5f32), (3.0, 0.1), (1.0, 0.25), (8.0, 0.75)] {
            let p = params(sens);
            let speed = (p.full_speed * share) as f64;
            let out = run(&trace(|_| (speed, 0.0), 0.05, 1.0, 1000.0), 1.0, &p);
            let want = p.deadzone + (1.0 - p.deadzone) * share.powf(1.0 / p.curve);
            for o in out.iter().filter(|o| o.0 > 0.3 && o.0 < 0.95) {
                assert!((o.1 - want).abs() < 0.04, "sens {sens} share {share}: {} vs {want}", o.1);
                assert!(o.2.abs() < 0.01);
            }
        }
    }

    #[test]
    fn stray_counts_from_a_resting_hand_do_nothing() {
        // One count now and then, further apart than a frame, in every direction, at any mouse speed.
        for sens in [1.0, 3.0, 12.0] {
            let p = params(sens);
            let ev: Vec<_> = (0..40).map(|i| (0.05 + i as f64 * 0.047, [1, 0, -1, 0][i % 4], [0, 1, 0, -1][i % 4])).collect();
            let out = run(&ev, 2.0, &p);
            assert!(out.iter().all(|o| mag(o) == 0.0), "sens {sens}");
        }
    }

    #[test]
    fn slow_aiming_is_smooth() {
        // Looking up while the speed wanders and the hand wobbles sideways a little.
        let p = params(3.0);
        let v = |t: f64| {
            let s = 400.0 * (1.0 + 0.4 * (2.0 * std::f64::consts::PI * 1.7 * t).sin());
            (s * 0.06 * (2.0 * std::f64::consts::PI * 3.1 * t).sin(), -s)
        };
        let out = run(&trace(v, 0.05, 3.0, 1000.0), 3.0, &p);
        let steady: Vec<_> = out.iter().filter(|o| o.0 > 0.3 && o.0 < 2.95).collect();
        assert!(steady.iter().all(|o| o.2 > 0.25), "always looking up, never dropping out");
        let step = steady.windows(2).map(|w| (w[1].1 - w[0].1).abs() + (w[1].2 - w[0].2).abs()).sum::<f32>() / steady.len() as f32;
        assert!(step < 0.004, "average change per millisecond {step}");
        assert!(steady.iter().all(|o| o.1.abs() < 0.12), "sideways wobble stays small");
    }

    #[test]
    fn late_reports_do_not_cut_a_swipe_short() {
        // Every 20th report arrives 7 ms late: the stick must not drop out.
        let p = params(3.0);
        let mut ev = trace(|_| (2500.0, 0.0), 0.05, 1.0, 1000.0);
        for (i, e) in ev.iter_mut().enumerate() {
            if i % 20 == 0 {
                e.0 += 0.007;
            }
        }
        ev.sort_by(|a, b| a.0.total_cmp(&b.0));
        let out = run(&ev, 1.0, &p);
        assert!(out.iter().filter(|o| o.0 > 0.1 && o.0 < 0.95).all(|o| mag(o) > 0.9));
    }

    #[test]
    fn a_slow_mouse_still_works() {
        // 125 Hz mouse: reports 8 ms apart.
        let p = params(3.0);
        let out = run(&trace(|_| (4000.0, 0.0), 0.100, 0.400, 125.0), 0.6, &p);
        let rise = out.iter().find(|o| mag(o) >= 0.9).expect("reaches full tilt").0 - 0.100;
        assert!(rise < 0.030, "start delay {:.1} ms", rise * 1000.0);
        assert!(out.iter().filter(|o| (0.16..0.39).contains(&o.0)).all(|o| mag(o) > 0.95));
        assert!(out.last().is_some_and(|o| mag(o) == 0.0));
    }

    #[test]
    fn directions_vertical_scale_and_invert() {
        let run_dir = |cfg: AimConfig, vx: f64, vy: f64| {
            let p = AimParams::new(&cfg, 30.0, false);
            let out = run(&trace(|_| (vx, vy), 0.05, 0.5, 1000.0), 0.5, &p);
            let o = out[400];
            (o.1, o.2)
        };
        let base = AimConfig { sensitivity: 3.0, ..AimConfig::default() };
        let (x, y) = run_dir(base.clone(), -600.0, 0.0);
        assert!(x < -0.3 && y.abs() < 0.01, "mouse left = stick left");
        let (x, y) = run_dir(base.clone(), 0.0, -600.0);
        assert!(y > 0.3 && x.abs() < 0.01, "mouse up = stick up");
        let (_, y) = run_dir(AimConfig { invert_y: true, ..base.clone() }, 0.0, -600.0);
        assert!(y < -0.3, "inverted: mouse up = stick down");
        let (_, slow) = run_dir(AimConfig { vertical: 0.5, ..base.clone() }, 0.0, -600.0);
        let (_, normal) = run_dir(base, 0.0, -600.0);
        assert!(slow < normal, "lower vertical speed tilts less");
    }

    #[test]
    fn aiming_in_scales_the_speed() {
        let cfg = AimConfig { sensitivity: 3.0, aim_in: 0.5, ..AimConfig::default() };
        let hip = AimParams::new(&cfg, 30.0, false);
        let ads = AimParams::new(&cfg, 30.0, true);
        assert!((ads.full_speed - hip.full_speed * 2.0).abs() < 1.0);
    }

    #[test]
    fn reset_centres_the_stick() {
        let p = params(3.0);
        let mut aim = Aim::new();
        for i in 0..50 {
            aim.feed(i as f64 * 0.001, 5, 0);
            aim.step(i as f64 * 0.001, TICK, &p);
        }
        assert!(aim.step(0.050, TICK, &p).0 > 0.5);
        aim.reset();
        assert_eq!(aim.step(0.051, TICK, &p), (0.0, 0.0));
    }
}
