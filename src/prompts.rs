//! Finding the game's button prompts in its picture.
//!
//! GTA 6 draws a prompt as an evenly lit disc with the button's dark mark inside it: a triangle,
//! circle, cross or square, or the letter L or R for a stick. The disc is white, or grey when
//! the action is dimmed, and it has a dark rim, so it stands apart even from a white wall.
//!
//! 1. Take everything at least as bright as some level and split it into connected areas. At a
//!    level between the rim's darkness and the face's brightness, a disc's face is an area of its
//!    own: the rim cuts it off from whatever is around it. Since discs come in several
//!    brightnesses, this is done at a few levels.
//! 2. Keep the areas that are disc-sized and round, with rim all the way round.
//! 3. Read the dark pattern inside each on a 15 x 15 grid and compare it with the six known marks.

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

/// Is the vehicle name card on screen? GTA 6 shows it just above the minimap for a few seconds
/// when you get into a vehicle: a dark see-through panel with the maker's name and, under it in
/// white, the model's name, centred.
///
/// The panel is see-through, so its brightness depends on what is behind it. What does not
/// change is the white model name: a band of bright text in a fixed row, centred on the panel,
/// with clean margins left and right and an even tone behind it. Built from two examples, and
/// checked against 31 pictures without the card.
pub fn name_card(rgb: &[u8], w: usize, h: usize) -> bool {
    if w < 320 || h < 180 || rgb.len() < w * h * 3 {
        return false;
    }
    let (sx, sy) = (w as f32 / 1920.0, h as f32 / 1080.0);
    let at = |x: f32, y: f32| ((x * sx).round() as usize, (y * sy).round() as usize);
    // The name area of the card, in a 1920 x 1080 picture.
    let (x0, y0) = at(34.0, 762.0);
    let (x1, y1) = at(332.0, 840.0);
    let (model_top, model_bottom) = (at(0.0, 802.0).1 - y0, at(0.0, 828.0).1 - y0);
    let (aw, ah) = (x1 - x0, y1 - y0);
    if aw < 40 || ah < 12 || x1 >= w || y1 >= h {
        return false;
    }
    let lum = |x: usize, y: usize| {
        let p = &rgb[((y0 + y) * w + x0 + x) * 3..];
        ((p[0] as u16 + p[1] as u16 + p[2] as u16) / 3) as u8
    };
    // The panel's own tone: the middle brightness of the area.
    let mut histogram = [0u32; 256];
    for y in 0..ah {
        for x in 0..aw {
            histogram[lum(x, y) as usize] += 1;
        }
    }
    let mut seen = 0u32;
    let base = histogram.iter().position(|n| {
        seen += n;
        seen * 2 >= (aw * ah) as u32
    });
    let base = base.unwrap_or(0) as i32;
    let text = |x: usize, y: usize| lum(x, y) as i32 > base + 60;

    // Text in the model-name rows: how much, where, and how wide.
    let (mut in_rows, mut first, mut last) = (0u32, usize::MAX, 0usize);
    let mut column_sum = 0u64;
    let mut columns = 0u32;
    for x in 0..aw {
        let mut any = false;
        for y in model_top..model_bottom {
            if text(x, y) {
                in_rows += 1;
                any = true;
            }
        }
        if any {
            first = first.min(x);
            last = x;
            column_sum += x as u64;
            columns += 1;
        }
    }
    if columns == 0 {
        return false;
    }
    let share = in_rows as f32 / ((model_bottom - model_top) * aw) as f32;
    let centre_off = column_sum as f32 / columns as f32 - aw as f32 / 2.0;
    let width = (last - first) as f32;
    // Margins: the outer strips of the area, over its full height, carry no text.
    let strip = ((16.0 * sx) as usize).max(2);
    let margin_text = (0..ah).flat_map(|y| (0..strip).chain(aw - strip..aw).map(move |x| (x, y))).filter(|&(x, y)| text(x, y)).count();
    let margins = margin_text as f32 / (2 * strip * ah) as f32;
    // Behind the text the panel is one even tone.
    let (mut n, mut sum, mut squares) = (0.0f64, 0.0f64, 0.0f64);
    for y in 0..ah {
        for x in 0..aw {
            if !text(x, y) {
                let v = lum(x, y) as f64;
                n += 1.0;
                sum += v;
                squares += v * v;
            }
        }
    }
    let spread = if n > 0.0 { (squares / n - (sum / n).powi(2)).max(0.0).sqrt() } else { 99.0 };

    (0.06..=0.30).contains(&share) && centre_off.abs() <= 8.0 * sx && margins <= 0.03 && spread <= 18.0 && (40.0 * sx..=260.0 * sx).contains(&width)
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

/// The brightness levels at which the picture is split: white discs stand alone at the high
/// ones, dimmed grey discs at the low ones. The darkest prompts, for actions that are not
/// available at all, would need a lower level still: they are left out, as the lowest levels
/// cost the most time and those prompts matter least.
const LEVELS: [u8; 4] = [225, 190, 155, 120];
/// Inside a disc, this much darker than its face counts as part of the mark.
const MARK_DARKER_BY: i16 = 60;
/// Disc diameter in a 1080-line picture: smallest and largest accepted.
const MIN_DIAMETER: f32 = 17.0;
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
    let mut found: Vec<Found> = Vec::new();
    if w < 320 || h < 180 || rgb.len() < w * h * 3 {
        return found;
    }
    let lum: Vec<u8> = rgb.chunks_exact(3).take(w * h).map(|p| ((p[0] as u16 + p[1] as u16 + p[2] as u16) / 3) as u8).collect();
    let mut area = vec![0u32; w * h];
    let mut stack: Vec<u32> = Vec::new();
    for level in LEVELS {
        for disc in discs_at(&lum, w, h, level, &mut area, &mut stack) {
            // The same disc usually stands alone at more than one level: keep its best reading.
            match found.iter_mut().find(|f| (f.x - disc.x).abs() < disc.diameter / 2.0 && (f.y - disc.y).abs() < disc.diameter / 2.0) {
                Some(seen) if seen.score < disc.score => *seen = disc,
                Some(_) => {}
                None => found.push(disc),
            }
        }
    }
    found
}

/// The prompts whose face stands alone when the picture is cut at `level`. `area` and `stack`
/// are scratch space, reused between levels.
fn discs_at(lum: &[u8], w: usize, h: usize, level: u8, area: &mut [u32], stack: &mut Vec<u32>) -> Vec<Found> {
    let mut found = Vec::new();
    let scale = h as f32 / 1080.0;
    let (min_d, max_d) = ((MIN_DIAMETER * scale) as i32, (MAX_DIAMETER * scale).ceil() as i32);
    area.fill(0);
    let mut next_area = 0u32;
    for start in 0..w * h {
        if lum[start] < level || area[start] != 0 {
            continue;
        }
        // Walk the connected area, noting its bounding box, size and brightness.
        next_area += 1;
        let id = next_area;
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
        let (mut count, mut total) = (0u32, 0u64);
        area[start] = id;
        stack.push(start as u32);
        while let Some(i) = stack.pop() {
            let i = i as usize;
            let (x, y) = (i % w, i / w);
            x0 = x0.min(x);
            x1 = x1.max(x);
            y0 = y0.min(y);
            y1 = y1.max(y);
            count += 1;
            total += lum[i] as u64;
            let mut visit = |n: usize| {
                if area[n] == 0 && lum[n] >= level {
                    area[n] = id;
                    stack.push(n as u32);
                }
            };
            // Straight neighbours only: a thin rim must not be crossed diagonally.
            if x > 0 {
                visit(i - 1);
            }
            if x + 1 < w {
                visit(i + 1);
            }
            if y > 0 {
                visit(i - w);
            }
            if y + 1 < h {
                visit(i + w);
            }
        }
        let (bw, bh) = ((x1 - x0 + 1) as i32, (y1 - y0 + 1) as i32);
        if bw < min_d || bh < min_d || bw > max_d || bh > max_d || (bw - bh).abs() > (3.0 * scale.max(1.0)) as i32 {
            continue;
        }
        let (cx, cy) = (x0 as f32 + bw as f32 / 2.0 - 0.5, y0 as f32 + bh as f32 / 2.0 - 0.5);
        let r = (bw + bh) as f32 / 4.0;
        if cx - r - 4.0 < 0.0 || cy - r - 4.0 < 0.0 || cx + r + 4.0 >= w as f32 || cy + r + 4.0 >= h as f32 {
            continue;
        }
        // A disc's face fills a good part of its circle. The mark takes the rest, and a square
        // or circle mark also cuts off the face inside it, leaving about a third.
        let covered = count as f32 / (std::f32::consts::PI * r * r);
        if !(0.30..=1.02).contains(&covered) {
            continue;
        }
        let face = (total / count as u64) as i16;
        let around = |radius: f32| {
            (0..48).map(move |i| {
                let a = i as f32 * std::f32::consts::TAU / 48.0;
                ((cy + radius * a.sin()).round() as usize) * w + (cx + radius * a.cos()).round() as usize
            })
        };
        // Just inside the edge it is face all the way round; just outside it, rim all the way round.
        let face_ring = around(0.86 * r).filter(|&i| area[i] == id).count() as f32 / 48.0;
        let rim_ring = around(r + 1.5 * scale.max(1.0)).filter(|&i| lum[i] < level).count() as f32 / 48.0;
        // (A square mark's corners reach nearly to the edge, so the face ring is allowed some gaps.)
        if face_ring < 0.72 || rim_ring < 0.85 {
            continue;
        }
        let mut grid: Grid = [[false; GRID]; GRID];
        let half = (GRID - 1) as f32 / 2.0;
        for (gy, row) in grid.iter_mut().enumerate() {
            for (gx, cell) in row.iter_mut().enumerate() {
                let x = (cx + (gx as f32 - half) / half * GRID_REACH * r).round() as usize;
                let y = (cy + (gy as f32 - half) / half * GRID_REACH * r).round() as usize;
                *cell = (lum[y * w + x] as i16) < face - MARK_DARKER_BY;
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

    /// Checks the name-card test against real frames (same setup as `real_frames`).
    #[test]
    #[ignore]
    fn real_frames_name_card() {
        let with_card = ["1795461508_driving", "1123_100027_should_be_drive"];
        let (mut frames, mut wrong) = (0, 0);
        for dir in std::env::var("PS5RS_FRAMES").expect("set PS5RS_FRAMES").split(';') {
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
                let (seen, want) = (name_card(&rgb, w, h), with_card.contains(&name.as_str()));
                if seen != want {
                    println!("WRONG {name}: card seen {seen}, card there {want}");
                    wrong += 1;
                }
                frames += 1;
            }
        }
        println!("{frames} frames checked for the name card, {wrong} wrong");
        assert!(frames > 0);
        assert_eq!(wrong, 0);
    }

    #[test]
    fn a_name_card_is_seen_and_plain_scenery_is_not() {
        // A 1920 x 1080 picture of blocky scenery; optionally a dark panel with a centred white bar
        // of "text" where the model name goes.
        let (w, h) = (1920usize, 1080usize);
        let draw = |card: Option<(usize, usize)>| {
            let mut rgb = picture(w, h, &[]);
            if let Some((left, right)) = card {
                for y in 760..874 {
                    for x in 32..334 {
                        let v = if (806..824).contains(&y) && (left..right).contains(&x) && (x / 3) % 2 == 0 { 245 } else { 45 };
                        rgb[(y * w + x) * 3..(y * w + x) * 3 + 3].copy_from_slice(&[v, v, v]);
                    }
                }
            }
            rgb
        };
        assert!(name_card(&draw(Some((120, 246))), w, h), "a centred name");
        assert!(name_card(&draw(Some((90, 276))), w, h), "a longer centred name");
        assert!(!name_card(&draw(None), w, h), "no card");
        assert!(!name_card(&draw(Some((40, 150))), w, h), "text pushed to one side is not the card");
        assert!(!name_card(&[0u8; 30], 4, 2));
    }

    /// Checks the detector against real frames, if they are on this machine. Set PS5RS_FRAMES to
    /// one or more folders (separated by `;`) and run with `-- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_frames() {
        use Glyph::*;
        // What is on screen in the frames that have prompts, judged by eye. Every other frame has none.
        // The second list is prompts that are half-lit or cut off: finding them is welcome, not required.
        let expected: [(&str, &[Glyph], &[Glyph]); 14] = [
            ("1795460287_on_foot", &[Triangle], &[Square]),
            ("1795460357_on_foot", &[Circle, Cross, Square], &[]),
            ("1123_100027_should_be_drive", &[Cross], &[]),
            ("1123_100338_should_be_foot", &[LeftStick, LeftStick, RightStick, RightStick], &[]),
            ("1123_100733_should_be_foot", &[Triangle], &[]),
            ("1123_100734_should_be_drive", &[Triangle], &[]),
            ("1123_100734_should_be_foot", &[Triangle], &[]), // fading out, half see-through
            ("1795461500_driving", &[], &[Triangle]), // dark grey: not available
            ("1795461508_driving", &[], &[Triangle]),
            ("1795461544_driving", &[Triangle], &[Square]), // the square's inside is shaded
            ("1795461643_driving", &[Triangle], &[Square]), // the square is half-lit
            ("1795461708_on_foot", &[Cross, Square], &[]),
            ("1795461720_on_foot", &[Cross, Square], &[]),
            ("1795461463_on_foot", &[], &[]),
        ];
        let dirs = std::env::var("PS5RS_FRAMES").expect("set PS5RS_FRAMES");
        let (mut frames, mut slowest, mut wrong) = (0, 0.0f64, 0u32);
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
                let (mut want, may): (Vec<Glyph>, Vec<Glyph>) =
                    expected.iter().find(|(n, _, _)| *n == name).map_or((Vec::new(), Vec::new()), |(_, g, m)| (g.to_vec(), m.to_vec()));
                want.sort();
                // Take the optional ones out before comparing.
                let mut required = glyphs.clone();
                for extra in &may {
                    if required.len() > want.len() {
                        if let Some(i) = required.iter().position(|g| g == extra) {
                            required.remove(i);
                        }
                    }
                }
                let right = required == want;
                wrong += !right as u32;
                println!("{} {name}: {:?}{}", if right { "ok  " } else { "DIFF" }, found.iter().map(|f| format!("{:?} {:.2} d{:.0}", f.glyph, f.score, f.diameter)).collect::<Vec<_>>(), if right { String::new() } else { format!("  wanted {want:?}") });
                frames += 1;
            }
        }
        println!("{frames} frames checked, {wrong} wrong, slowest search {slowest:.1} ms");
        assert!(frames > 0);
        assert_eq!(wrong, 0);
    }
}
