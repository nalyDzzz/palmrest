//! Finding the game's button prompts in its picture.
//!
//! GTA 6 draws a prompt as a solid white disc with the button's dark mark inside it: a triangle,
//! circle, cross or square, or the letter L or R for a stick. A white disc of that size is rare in
//! scenery, and the mark inside is always the same drawing, so two plain steps find them:
//!
//! 1. Collect the near-white areas of the picture and keep those that are disc-sized, round, and
//!    ringed by non-white.
//! 2. Read the dark pattern inside each on a 15 x 15 grid and compare it with the six known marks.
//!
//! Dimmed prompts (grey discs, for an action that is not available) are not found, on purpose.

/// A button mark the game can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Glyph {
    Triangle,
    Circle,
    Cross,
    Square,
    LeftStick,
    RightStick,
}

impl Glyph {
    pub const ALL: [Glyph; 6] = [Glyph::Triangle, Glyph::Circle, Glyph::Cross, Glyph::Square, Glyph::LeftStick, Glyph::RightStick];
}

/// Turns single looks into a steady list: a prompt is shown once two looks in a row have found
/// it, and dropped once three in a row have not. Prompts flicker as the camera moves, and a
/// hint that blinks is worse than none.
#[derive(Default)]
pub struct Steady {
    /// Per mark: looks in a row that found it, looks in a row that did not, and whether it is shown.
    marks: [(u8, u8, bool); 6],
}

impl Steady {
    pub fn look(&mut self, found: &[Glyph]) {
        for (i, glyph) in Glyph::ALL.iter().enumerate() {
            let (hits, misses, shown) = &mut self.marks[i];
            if found.contains(glyph) {
                *hits = hits.saturating_add(1);
                *misses = 0;
                *shown |= *hits >= 2;
            } else {
                *misses = misses.saturating_add(1);
                *hits = 0;
                *shown &= *misses < 3;
            }
        }
    }

    pub fn clear(&mut self) {
        *self = Steady::default();
    }

    /// The marks to show, always in the same order.
    pub fn shown(&self) -> Vec<Glyph> {
        Glyph::ALL.iter().zip(&self.marks).filter(|(_, m)| m.2).map(|(g, _)| *g).collect()
    }
}

/// A prompt found in the picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Found {
    pub glyph: Glyph,
    /// Centre of the disc, in pixels.
    pub x: f32,
    pub y: f32,
    pub diameter: f32,
    /// How well the mark matched, 0..1.
    pub score: f32,
}

/// A channel this bright in all three colours counts as white.
const WHITE: u8 = 222;
/// Inside a disc, this dark counts as part of the mark.
const DARK: u8 = 120;
/// Disc diameter in a 1080-line picture: smallest and largest accepted.
const MIN_DIAMETER: f32 = 20.0;
const MAX_DIAMETER: f32 = 46.0;
/// The grid the mark is read on, and how far from the centre it reaches (share of the radius).
const GRID: usize = 15;
const GRID_REACH: f32 = 0.72;
/// A mark must match its pattern this well, and beat the runner-up by this much.
const MATCH_AT_LEAST: f32 = 0.60;
const LEAD_AT_LEAST: f32 = 0.10;

/// The dark pattern of each mark on the grid, as the game draws them.
const PATTERNS: [(Glyph, [&str; GRID]); 6] = [
    (
        Glyph::Triangle,
        [
            "......###......",
            "......###......",
            ".....#####.....",
            "....###.###....",
            "....##...##....",
            "...###...###...",
            "..###.....###..",
            "..##.......##..",
            ".###.......###.",
            ".##.........##.",
            "###############",
            "###############",
            "...............",
            "...............",
            "...............",
        ],
    ),
    (
        Glyph::Circle,
        [
            "...............",
            "....#######....",
            "..####...####..",
            "..##.......##..",
            ".##.........##.",
            ".##.........##.",
            ".##.........##.",
            ".##.........##.",
            ".##.........##.",
            ".##.........##.",
            ".##.........##.",
            "..##.......##..",
            "..####...####..",
            "....#######....",
            "...............",
        ],
    ),
    (
        Glyph::Cross,
        [
            "...............",
            "...............",
            "..##.......##..",
            "..###.....###..",
            "...###...###...",
            "....###.###....",
            ".....#####.....",
            "......###......",
            ".....#####.....",
            "....###.###....",
            "...###...###...",
            "..###.....###..",
            "..##.......##..",
            "...............",
            "...............",
        ],
    ),
    (
        Glyph::Square,
        [
            "...............",
            "...............",
            ".#############.",
            ".#############.",
            ".##.........##.",
            ".##.........##.",
            ".##.........##.",
            ".##.........##.",
            ".##.........##.",
            ".##.........##.",
            ".##.........##.",
            ".#############.",
            ".#############.",
            "...............",
            "...............",
        ],
    ),
    (
        Glyph::LeftStick,
        [
            "...............",
            "...............",
            "...............",
            ".....##........",
            ".....##........",
            ".....##........",
            ".....##........",
            ".....##........",
            ".....##........",
            ".....##........",
            ".....######....",
            ".....######....",
            "...............",
            "...............",
            "...............",
        ],
    ),
    (
        Glyph::RightStick,
        [
            "...............",
            "...............",
            "...............",
            ".....#####.....",
            ".....######....",
            ".....##..##....",
            ".....##..##....",
            ".....#####.....",
            ".....#####.....",
            ".....##.##.....",
            ".....##..##....",
            ".....##...##...",
            "...............",
            "...............",
            "...............",
        ],
    ),
];

type Grid = [[bool; GRID]; GRID];

fn pattern(rows: &[&str; GRID]) -> Grid {
    let mut grid = [[false; GRID]; GRID];
    for (y, row) in rows.iter().enumerate() {
        for (x, c) in row.bytes().enumerate() {
            grid[y][x] = c == b'#';
        }
    }
    grid
}

/// The four corner cells of the grid lie just outside the disc: they say nothing about the mark.
fn counts(x: usize, y: usize) -> bool {
    let (dx, dy) = (x as i32 - 7, y as i32 - 7);
    dx * dx + dy * dy < 98
}

/// How alike two patterns are: shared dark cells over all dark cells (0..1).
fn alike(a: &Grid, b: &Grid) -> f32 {
    let (mut both, mut either) = (0u32, 0u32);
    for y in 0..GRID {
        for x in 0..GRID {
            if counts(x, y) {
                both += (a[y][x] && b[y][x]) as u32;
                either += (a[y][x] || b[y][x]) as u32;
            }
        }
    }
    if either == 0 { 0.0 } else { both as f32 / either as f32 }
}

/// The pattern moved by (dx, dy) cells. Cells moved in from outside are empty.
fn moved(grid: &Grid, dx: i32, dy: i32) -> Grid {
    let mut out = [[false; GRID]; GRID];
    for y in 0..GRID as i32 {
        for x in 0..GRID as i32 {
            let (sx, sy) = (x - dx, y - dy);
            if (0..GRID as i32).contains(&sx) && (0..GRID as i32).contains(&sy) {
                out[y as usize][x as usize] = grid[sy as usize][sx as usize];
            }
        }
    }
    out
}

/// How alike two patterns are at best when one may sit a cell off in any direction. A disc's
/// centre is only known to about a pixel, which is about one grid cell.
fn alike_nearby(seen: &Grid, known: &Grid) -> f32 {
    let mut best = 0.0f32;
    for dy in -1..=1 {
        for dx in -1..=1 {
            best = best.max(alike(&moved(seen, dx, dy), known));
        }
    }
    best
}

/// Which mark a dark pattern is, if it is clearly one of them.
fn classify(seen: &Grid) -> Option<(Glyph, f32)> {
    let mut scores: Vec<(Glyph, f32)> = PATTERNS.iter().map(|(g, rows)| (*g, alike_nearby(seen, &pattern(rows)))).collect();
    scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    let (best, second) = (scores[0], scores[1]);
    (best.1 >= MATCH_AT_LEAST && best.1 - second.1 >= LEAD_AT_LEAST).then_some(best)
}

/// Finds the prompts in a picture given as RGB bytes, rows from the top.
pub fn find(rgb: &[u8], w: usize, h: usize) -> Vec<Found> {
    let mut found = Vec::new();
    if w < 320 || h < 180 || rgb.len() < w * h * 3 {
        return found;
    }
    let scale = h as f32 / 1080.0;
    let (min_d, max_d) = ((MIN_DIAMETER * scale) as i32, (MAX_DIAMETER * scale).ceil() as i32);
    let white: Vec<bool> = rgb.chunks_exact(3).take(w * h).map(|p| p[0] >= WHITE && p[1] >= WHITE && p[2] >= WHITE).collect();
    let lum = |x: usize, y: usize| {
        let p = &rgb[(y * w + x) * 3..];
        ((p[0] as u16 + p[1] as u16 + p[2] as u16) / 3) as u8
    };
    let mut seen = vec![false; w * h];
    let mut stack: Vec<u32> = Vec::new();
    for start in 0..w * h {
        if !white[start] || seen[start] {
            continue;
        }
        // Walk the whole connected white area, noting only its bounding box.
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
        seen[start] = true;
        stack.push(start as u32);
        while let Some(i) = stack.pop() {
            let (x, y) = (i as usize % w, i as usize / w);
            x0 = x0.min(x);
            x1 = x1.max(x);
            y0 = y0.min(y);
            y1 = y1.max(y);
            for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let n = ny * w + nx;
                    if white[n] && !seen[n] {
                        seen[n] = true;
                        stack.push(n as u32);
                    }
                }
            }
        }
        let (bw, bh) = ((x1 - x0 + 1) as i32, (y1 - y0 + 1) as i32);
        if bw < min_d || bh < min_d || bw > max_d || bh > max_d || (bw - bh).abs() > (4.0 * scale.max(1.0)) as i32 {
            continue;
        }
        let (cx, cy) = (x0 as f32 + bw as f32 / 2.0 - 0.5, y0 as f32 + bh as f32 / 2.0 - 0.5);
        let r = (bw + bh) as f32 / 4.0;
        if cx - 1.4 * r < 0.0 || cy - 1.4 * r < 0.0 || cx + 1.4 * r >= w as f32 || cy + 1.4 * r >= h as f32 {
            continue;
        }
        // Round and standing alone: white all the way round near its rim, not white just outside it.
        let ring = |k: f32| {
            let hits = (0..48)
                .filter(|i| {
                    let a = *i as f32 * std::f32::consts::TAU / 48.0;
                    let (x, y) = ((cx + k * r * a.cos()).round() as usize, (cy + k * r * a.sin()).round() as usize);
                    white[y * w + x]
                })
                .count();
            hits as f32 / 48.0
        };
        if ring(0.86) < 0.85 || ring(1.28) > 0.20 {
            continue;
        }
        let mut grid: Grid = [[false; GRID]; GRID];
        let half = (GRID - 1) as f32 / 2.0;
        for (gy, row) in grid.iter_mut().enumerate() {
            for (gx, cell) in row.iter_mut().enumerate() {
                let x = (cx + (gx as f32 - half) / half * GRID_REACH * r).round() as usize;
                let y = (cy + (gy as f32 - half) / half * GRID_REACH * r).round() as usize;
                *cell = lum(x, y) < DARK;
            }
        }
        if let Some((glyph, score)) = classify(&grid) {
            found.push(Found { glyph, x: cx, y: cy, diameter: 2.0 * r, score });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pattern_is_itself_and_no_other() {
        for (glyph, rows) in &PATTERNS {
            assert_eq!(classify(&pattern(rows)).map(|m| m.0), Some(*glyph));
        }
        // No mark may pass for another, even when it sits a cell off: a perfect drawing of one
        // must beat every other by a wide margin.
        for (i, (a, rows_a)) in PATTERNS.iter().enumerate() {
            for (b, rows_b) in &PATTERNS[i + 1..] {
                let likeness = alike_nearby(&pattern(rows_a), &pattern(rows_b));
                println!("{a:?} vs {b:?}: {likeness:.2}");
                assert!(likeness < 0.70, "{a:?} and {b:?} are too alike: {likeness}");
            }
        }
        assert_eq!(classify(&[[false; GRID]; GRID]), None, "an empty disc is no prompt");
    }

    #[test]
    fn a_prompt_must_be_seen_twice_to_show_and_missed_three_times_to_go() {
        use Glyph::*;
        let mut s = Steady::default();
        s.look(&[Triangle]);
        assert!(s.shown().is_empty(), "once is not enough");
        s.look(&[Triangle, Cross]);
        assert_eq!(s.shown(), [Triangle]);
        s.look(&[Cross]);
        s.look(&[Cross]);
        assert_eq!(s.shown(), [Triangle, Cross], "two misses: still shown");
        s.look(&[Cross]);
        assert_eq!(s.shown(), [Cross], "three misses: gone");
        s.look(&[Cross, Triangle]);
        assert_eq!(s.shown(), [Cross], "coming back needs two looks again");
        s.clear();
        assert!(s.shown().is_empty());
    }

    /// A picture of grey-brown blocks, with a prompt disc of `diameter` drawn at (cx, cy).
    fn picture(w: usize, h: usize, discs: &[(Glyph, usize, usize, usize)]) -> Vec<u8> {
        let mut rgb = vec![0u8; w * h * 3];
        let mut state = 12345u32;
        for by in 0..h.div_ceil(24) {
            for bx in 0..w.div_ceil(24) {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                let base = 40 + (state >> 24) as u8 % 140;
                for y in by * 24..((by + 1) * 24).min(h) {
                    for x in bx * 24..((bx + 1) * 24).min(w) {
                        rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[base, base.saturating_sub(8), base.saturating_sub(16)]);
                    }
                }
            }
        }
        for &(glyph, cx, cy, diameter) in discs {
            let mark = pattern(&PATTERNS.iter().find(|(g, _)| *g == glyph).unwrap().1);
            let r = diameter as f32 / 2.0;
            for y in cy - diameter..=cy + diameter {
                for x in cx - diameter..=cx + diameter {
                    let (dx, dy) = (x as f32 - cx as f32, y as f32 - cy as f32);
                    if dx.hypot(dy) > r {
                        continue;
                    }
                    // Which grid cell this pixel falls in, the same way `find` reads it.
                    let half = (GRID - 1) as f32 / 2.0;
                    let (gx, gy) = ((dx / (GRID_REACH * r) * half + half).round(), (dy / (GRID_REACH * r) * half + half).round());
                    let dark = (0.0..GRID as f32).contains(&gx) && (0.0..GRID as f32).contains(&gy) && mark[gy as usize][gx as usize];
                    let v = if dark { 25 } else { 250 };
                    rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[v, v, v]);
                }
            }
        }
        rgb
    }

    #[test]
    fn prompts_are_found_and_named_at_several_sizes() {
        let (w, h) = (960usize, 540usize);
        let placed = [
            (Glyph::Triangle, 100, 100, 28),
            (Glyph::Circle, 300, 120, 26),
            (Glyph::Cross, 500, 300, 30),
            (Glyph::Square, 700, 400, 32),
            (Glyph::LeftStick, 200, 400, 28),
            (Glyph::RightStick, 420, 450, 28),
        ];
        // Drawn for a 1080-line picture, placed in a picture half that size: sizes still fit.
        let discs: Vec<_> = placed.iter().map(|&(g, x, y, d)| (g, x, y, d / 2 + 6)).collect();
        let found = find(&picture(w, h, &discs), w, h);
        assert_eq!(found.len(), discs.len(), "{found:?}");
        for (glyph, x, y, _) in discs {
            let hit = found.iter().find(|f| (f.x - x as f32).abs() < 3.0 && (f.y - y as f32).abs() < 3.0).expect("found where it was drawn");
            assert_eq!(hit.glyph, glyph);
        }
    }

    #[test]
    fn scenery_alone_gives_nothing() {
        let (w, h) = (960usize, 540usize);
        assert!(find(&picture(w, h, &[]), w, h).is_empty());
        // Large white areas (sky, walls) and a white disc of the wrong size are not prompts.
        let mut rgb = picture(w, h, &[]);
        for y in 0..200 {
            for x in 0..w {
                rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[255, 255, 255]);
            }
        }
        assert!(find(&rgb, w, h).is_empty());
        assert!(find(&[0u8; 30], 4, 2).is_empty(), "a picture too small to hold a prompt");
    }

    /// Checks the detector against real frames, if they are on this machine. Set PS5RS_FRAMES to
    /// one or more folders (separated by `;`) and run with `-- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_frames() {
        use Glyph::*;
        // What is on screen in the frames that have prompts, judged by eye. Every other frame has none.
        let expected: [(&str, &[Glyph]); 6] = [
            ("1795460287_on_foot", &[Triangle]),
            ("1795460357_on_foot", &[Circle, Cross, Square]),
            ("1123_100027_should_be_drive", &[Cross]),
            ("1123_100338_should_be_foot", &[LeftStick, LeftStick, RightStick, RightStick]),
            ("1123_100733_should_be_foot", &[Triangle]),
            ("1123_100734_should_be_drive", &[Triangle]),
        ];
        let dirs = std::env::var("PS5RS_FRAMES").expect("set PS5RS_FRAMES");
        let (mut frames, mut slowest) = (0, 0.0f64);
        for dir in dirs.split(';') {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("png") {
                    continue;
                }
                let name = path.file_stem().unwrap().to_string_lossy().to_string();
                let mut decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(&path).unwrap()));
                decoder.set_transformations(png::Transformations::normalize_to_color8());
                let mut reader = decoder.read_info().unwrap();
                let mut buf = vec![0; reader.output_buffer_size().unwrap()];
                let info = reader.next_frame(&mut buf).unwrap();
                let (w, h, n) = (info.width as usize, info.height as usize, info.color_type.samples());
                let rgb: Vec<u8> = buf[..w * h * n].chunks(n).flat_map(|p| [p[0], p[1], p[2]]).collect();
                let started = std::time::Instant::now();
                let found = find(&rgb, w, h);
                slowest = slowest.max(started.elapsed().as_secs_f64() * 1000.0);
                let mut glyphs: Vec<Glyph> = found.iter().map(|f| f.glyph).collect();
                glyphs.sort();
                let want: Vec<Glyph> = expected.iter().find(|(n, _)| *n == name).map_or(Vec::new(), |(_, g)| g.to_vec());
                println!("{name}: {:?}", found.iter().map(|f| format!("{:?} {:.2}", f.glyph, f.score)).collect::<Vec<_>>());
                assert_eq!(glyphs, want, "{name}");
                frames += 1;
            }
        }
        println!("{frames} frames checked, slowest search {slowest:.1} ms");
        assert!(frames > 0);
    }
}
