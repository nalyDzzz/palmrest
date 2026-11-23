//! Reading one thing from the game picture: is the minimap on screen?
//!
//! The minimap is a flat, see-through panel with a clean edge, always in the same place. So along
//! its border the brightness steps between a pixel just inside and a pixel just outside, nearly
//! everywhere. With no minimap, the same line runs through ordinary scenery and steps only here
//! and there. The panel is also much flatter inside than scenery is. Measured on real 1080p
//! frames: with the minimap, 89-96 % of border points step clearly, the average step is 42-73
//! brightness levels and the brightness inside varies by 10-20; without it, 23-59 %, 8-29 and 30-65.

use serde::{Deserialize, Serialize};

/// A grey picture, one byte per pixel, rows from the top.
pub struct Gray {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Gray {
    fn at(&self, x: usize, y: usize) -> f32 {
        self.px[y * self.w + x] as f32
    }
}

/// A rectangle given as shares (0..1) of the 16:9 game picture.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Area {
    pub left: f32,
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
}

impl Area {
    /// Where GTA 6 draws the minimap with default HUD settings.
    pub const GTA6_MINIMAP: Area = Area { left: 30.0 / 1920.0, top: 882.0 / 1080.0, right: 334.0 / 1920.0, bottom: 1052.0 / 1080.0 };

    /// The rectangle in pixels (left, top, right, bottom) inside a picture of this size.
    pub fn pixels(&self, picture_w: usize, picture_h: usize) -> (usize, usize, usize, usize) {
        let px = |share: f32, size: usize| (share.clamp(0.0, 1.0) * size as f32).round() as usize;
        (px(self.left, picture_w), px(self.top, picture_h), px(self.right, picture_w), px(self.bottom, picture_h))
    }
}

impl Default for Area {
    fn default() -> Self {
        Area::GTA6_MINIMAP
    }
}

/// How far from the border the two compared pixels are, for a picture of this height.
pub fn reach(picture_h: usize) -> usize {
    ((picture_h as f32 / 270.0).round() as usize).max(3)
}

/// What the border of a rectangle looks like.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    /// Average brightness step across the border, 0..255.
    pub step: f32,
    /// Share of border points where the step is clear, 0..1.
    pub clear: f32,
    /// How much the brightness varies inside the rectangle (standard deviation, 0..128).
    pub rough: f32,
}

impl Reading {
    /// `Some(true)`: the panel is there. `Some(false)`: it is not. `None`: too close to call
    /// (it may be fading in or out).
    pub fn verdict(&self) -> Option<bool> {
        // All three must agree before the panel is taken to be there: wrongly seeing it is the
        // mistake that matters, since it can put driving keys under someone on foot.
        if self.clear >= 0.78 && self.step >= 34.0 && self.rough <= 26.0 {
            Some(true)
        } else if self.clear <= 0.68 || self.step <= 30.0 {
            Some(false)
        } else {
            None
        }
    }
}

/// Measures the border of the rectangle (left, top, right, bottom) in `img`. `d` is how far in
/// and out of the border to look. `None` if the picture does not reach `d` pixels past the
/// rectangle on every side, or the rectangle is too small to measure.
pub fn measure(img: &Gray, rect: (usize, usize, usize, usize), d: usize) -> Option<Reading> {
    let (x0, y0, x1, y1) = rect;
    if x1 <= x0 || y1 <= y0 || x0 < d || y0 < d || x1 + d >= img.w || y1 + d >= img.h || img.px.len() < img.w * img.h {
        return None;
    }
    let inset = ((x1 - x0) as f32 * 0.12) as usize; // keep off the rounded corners
    if x1 - x0 <= 2 * inset + 6 || y1 - y0 <= 2 * inset + 6 {
        return None;
    }
    let (mut total, mut clear, mut count) = (0.0f32, 0u32, 0u32);
    let mut add = |inside: f32, outside: f32| {
        let step = (inside - outside).abs();
        total += step;
        clear += (step > 10.0) as u32;
        count += 1;
    };
    for x in (x0 + inset..x1 - inset).step_by(3) {
        add(img.at(x, y0 + d), img.at(x, y0 - d));
        add(img.at(x, y1 - d), img.at(x, y1 + d));
    }
    for y in (y0 + inset..y1 - inset).step_by(3) {
        add(img.at(x0 + d, y), img.at(x0 - d, y));
        add(img.at(x1 - d, y), img.at(x1 + d, y));
    }
    // Brightness spread inside, away from the border.
    let (mut n, mut sum, mut squares) = (0.0f64, 0.0f64, 0.0f64);
    for y in (y0 + inset..y1 - inset).step_by(2) {
        for x in (x0 + inset..x1 - inset).step_by(2) {
            let v = img.at(x, y) as f64;
            n += 1.0;
            sum += v;
            squares += v * v;
        }
    }
    let mean = sum / n;
    let rough = (squares / n - mean * mean).max(0.0).sqrt() as f32;
    Some(Reading { step: total / count as f32, clear: clear as f32 / count as f32, rough })
}

/// Whether the minimap is on screen, as far as the last few looks can tell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Seen {
    Visible,
    Hidden,
    /// The game picture cannot be read (window minimized, not looked at yet).
    Unknown,
}

/// Turns single looks into a steady answer: the answer changes only when two looks in a row agree,
/// and becomes unknown after three looks in a row that could not read the picture.
pub struct Tracker {
    seen: Seen,
    candidate: Option<bool>,
    failures: u8,
}

impl Default for Tracker {
    fn default() -> Self {
        Tracker { seen: Seen::Unknown, candidate: None, failures: 0 }
    }
}

impl Tracker {
    pub fn seen(&self) -> Seen {
        self.seen
    }

    /// Forget everything, as when the game window is no longer in front.
    pub fn reset(&mut self) {
        *self = Tracker::default();
    }

    /// One look at the picture: a reading, or `None` if the picture could not be read.
    pub fn look(&mut self, reading: Option<Reading>) -> Seen {
        match reading {
            None => {
                self.candidate = None;
                self.failures = self.failures.saturating_add(1);
                if self.failures >= 3 {
                    self.seen = Seen::Unknown;
                }
            }
            Some(r) => {
                self.failures = 0;
                let verdict = r.verdict();
                if let Some(v) = verdict {
                    if self.candidate == Some(v) {
                        self.seen = if v { Seen::Visible } else { Seen::Hidden };
                    }
                }
                self.candidate = verdict;
            }
        }
        self.seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 480x270 picture of noisy scenery, optionally with a flat lighter panel where the minimap goes.
    fn picture(panel: bool, seed: u32) -> (Gray, (usize, usize, usize, usize)) {
        let (w, h) = (480usize, 270usize);
        let mut state = seed.wrapping_mul(2654435761).max(1);
        let mut noise = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state % 256) as u8
        };
        // Scenery: blocks of similar brightness, like roads and walls, with a little grain.
        let mut px = vec![0u8; w * h];
        for by in 0..h.div_ceil(18) {
            for bx in 0..w.div_ceil(18) {
                let base = 40 + noise() % 150;
                for y in by * 18..((by + 1) * 18).min(h) {
                    for x in bx * 18..((bx + 1) * 18).min(w) {
                        px[y * w + x] = base.saturating_add(noise() % 9);
                    }
                }
            }
        }
        let rect = Area::GTA6_MINIMAP.pixels(w, h);
        if panel {
            for y in rect.1..rect.3 {
                for x in rect.0..rect.2 {
                    // See-through panel: scenery pulled towards light grey.
                    px[y * w + x] = (px[y * w + x] as f32 * 0.25 + 215.0 * 0.75) as u8;
                }
            }
        }
        (Gray { w, h, px }, rect)
    }

    #[test]
    fn a_panel_is_seen_and_scenery_is_not() {
        for seed in 1..40 {
            let (img, rect) = picture(true, seed);
            let with = measure(&img, rect, reach(img.h)).unwrap();
            assert_eq!(with.verdict(), Some(true), "seed {seed}: {with:?}");
            let (img, rect) = picture(false, seed);
            let without = measure(&img, rect, reach(img.h)).unwrap();
            assert_eq!(without.verdict(), Some(false), "seed {seed}: {without:?}");
        }
    }

    #[test]
    fn the_measured_ranges_from_real_frames_give_the_right_verdict() {
        for (step, clear, rough) in [(42.2, 0.95, 10.2), (72.5, 0.96, 20.2), (44.3, 0.89, 9.9), (44.0, 0.92, 18.0)] {
            assert_eq!(Reading { step, clear, rough }.verdict(), Some(true));
        }
        for (step, clear, rough) in [(10.9, 0.35, 30.3), (28.5, 0.59, 46.6), (8.1, 0.23, 46.5), (18.0, 0.57, 64.8)] {
            assert_eq!(Reading { step, clear, rough }.verdict(), Some(false));
        }
        assert_eq!(Reading { step: 36.0, clear: 0.72, rough: 15.0 }.verdict(), None, "in between: fading");
        assert_eq!(Reading { step: 40.0, clear: 0.85, rough: 45.0 }.verdict(), None, "a clear edge around busy scenery is not the panel");
    }

    #[test]
    fn a_rectangle_too_close_to_the_edge_cannot_be_measured() {
        let img = Gray { w: 100, h: 60, px: vec![0; 6000] };
        assert!(measure(&img, (0, 10, 60, 50), 3).is_none());
        assert!(measure(&img, (10, 10, 98, 50), 3).is_none());
        assert!(measure(&img, (10, 10, 60, 50), 3).is_some());
    }

    #[test]
    fn the_tracker_needs_two_looks_that_agree() {
        let yes = Some(Reading { step: 50.0, clear: 0.95, rough: 12.0 });
        let no = Some(Reading { step: 12.0, clear: 0.30, rough: 50.0 });
        let maybe = Some(Reading { step: 36.0, clear: 0.72, rough: 20.0 });
        let mut t = Tracker::default();
        assert_eq!(t.look(yes), Seen::Unknown, "one look is not enough");
        assert_eq!(t.look(yes), Seen::Visible);
        assert_eq!(t.look(no), Seen::Visible, "one odd look changes nothing");
        assert_eq!(t.look(yes), Seen::Visible);
        assert_eq!(t.look(no), Seen::Visible);
        assert_eq!(t.look(maybe), Seen::Visible, "a doubtful look breaks the run");
        assert_eq!(t.look(no), Seen::Visible);
        assert_eq!(t.look(no), Seen::Hidden);
        assert_eq!(t.look(None), Seen::Hidden);
        assert_eq!(t.look(None), Seen::Hidden);
        assert_eq!(t.look(None), Seen::Unknown, "three unreadable looks: we no longer know");
    }

    /// Checks the detector against real frames, if they are on this machine. Set PS5RS_FRAMES to a
    /// folder of 16:9 screenshots named like Rewire's `misses` files and run with `-- --ignored`.
    #[test]
    #[ignore]
    fn real_frames() {
        let dir = std::env::var("PS5RS_FRAMES").expect("set PS5RS_FRAMES to the folder of frames");
        // Frames in which the minimap is on screen, judged by eye.
        let visible = ["092307", "092413", "092419", "100027", "100338", "100345", "100629"];
        let mut checked = 0;
        for entry in std::fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("png") {
                continue;
            }
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let mut decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
            decoder.set_transformations(png::Transformations::normalize_to_color8());
            let mut reader = decoder.read_info().unwrap();
            let mut buf = vec![0; reader.output_buffer_size().unwrap()];
            let info = reader.next_frame(&mut buf).unwrap();
            let (w, h, n) = (info.width as usize, info.height as usize, info.color_type.samples());
            let px = buf[..w * h * n].chunks(n).map(|p| ((p[0] as u32 + p[1] as u32 + p[2] as u32) / 3) as u8).collect();
            let img = Gray { w, h, px };
            let reading = measure(&img, Area::GTA6_MINIMAP.pixels(w, h), reach(h)).unwrap();
            let expect = visible.iter().any(|id| name.contains(id));
            println!("{name}: step {:.1} clear {:.2} rough {:.1} -> {:?} (expected {expect})", reading.step, reading.clear, reading.rough, reading.verdict());
            assert_eq!(reading.verdict(), Some(expect), "{name}");
            checked += 1;
        }
        assert!(checked > 0, "no frames found");
    }
}
