//! Finding the game's button prompts in its picture.
//!
//! GTA 6 draws a prompt as an evenly lit face with the button's dark mark on it. The face is a
//! disc for the four face buttons (triangle, circle, cross, square), the sticks (L, R) and the
//! stick clicks (L3, R3). For L1 and R1 it is a badge with a flat top and rounded lower corners,
//! for L2 and R2 one with rounded upper corners and a flat bottom. The face is white, or grey
//! when the action is dimmed, and it has a dark rim, so it stands apart even from a white wall.
//!
//! 1. Take everything at least as bright as some level and split it into connected areas. At a
//!    level between the rim's darkness and the face's brightness, a face is an area of its own:
//!    the rim cuts it off from whatever is around it. Since faces come in several brightnesses,
//!    this is done at a few levels.
//! 2. Keep the areas that have the size and outline of a prompt, with rim around them.
//! 3. Read the dark pattern inside each on a 15 x 15 grid and compare it with the known marks
//!    for that outline.

/// A button mark the game can show.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Glyph {
    Triangle,
    Circle,
    Cross,
    Square,
    LeftStick,
    RightStick,
    L1,
    R1,
    L2,
    R2,
    L3,
    R3,
}

impl Glyph {
    pub const ALL: [Glyph; 12] = [
        Glyph::Triangle,
        Glyph::Circle,
        Glyph::Cross,
        Glyph::Square,
        Glyph::LeftStick,
        Glyph::RightStick,
        Glyph::L1,
        Glyph::R1,
        Glyph::L2,
        Glyph::R2,
        Glyph::L3,
        Glyph::R3,
    ];
}

/// The outline of a prompt's bright face.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outline {
    /// Round: the face buttons, the sticks, and the stick clicks L3 and R3.
    Disc,
    /// Flat on top with rounded lower corners, wider than tall: L1 and R1.
    Shoulder,
    /// Rounded upper corners and flat below, about as wide as tall: L2 and R2.
    Trigger,
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
pub struct Steady {
    /// Per mark: looks in a row that found it, looks in a row that did not, and whether it is shown.
    marks: [(u8, u8, bool); Glyph::ALL.len()],
    /// Looks in a row that must miss a mark before it is dropped.
    patience: u8,
}

impl Default for Steady {
    fn default() -> Self {
        Steady { marks: Default::default(), patience: 3 }
    }
}

impl Steady {
    /// How many looks in a row must miss a mark before it is dropped: three unless the looks
    /// come quickly, when three would be over in a blink.
    pub fn set_patience(&mut self, looks: u8) {
        self.patience = looks.max(3);
    }

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
                *shown &= *misses < self.patience;
            }
        }
    }

    pub fn clear(&mut self) {
        self.marks = Default::default();
    }

    /// The marks to show, always in the same order.
    pub fn shown(&self) -> Vec<Glyph> {
        Glyph::ALL.iter().zip(&self.marks).filter(|(_, m)| m.2).map(|(g, _)| *g).collect()
    }
}

/// Follows prompts from look to look, so that a key hint can sit on a prompt and move with it.
///
/// A prompt found close to where one of the same kind was at the look before is the same
/// prompt. It counts as there once two looks have found it: a single look can be scenery that
/// happens to look like a button. It stays for a short while after it was last seen: prompts
/// drop out for a look or two while the camera moves. Its place only changes when it has moved
/// by more than a pixel or two, so that a hint on it does not shimmer.
#[derive(Default)]
pub struct Tracks {
    /// Each prompt as it is shown, the looks that found it, and when the last of them was.
    list: Vec<(Found, u8, f64)>,
}

impl Tracks {
    /// Takes in what a look at time `now` found. Prompts not seen for `keep` seconds are forgotten.
    pub fn look(&mut self, found: &[Found], now: f64, keep: f64) {
        self.list.retain(|(_, _, seen)| now - seen <= keep);
        let known = self.list.len();
        let mut taken = vec![false; known];
        for f in found {
            // The same prompt has moved less than its own width since the last look.
            let off = |shown: &Found| (shown.x - f.x).abs().max((shown.y - f.y).abs());
            let nearest = (0..known).filter(|&i| !taken[i] && self.list[i].0.glyph == f.glyph && off(&self.list[i].0) < f.diameter).min_by(|&a, &b| off(&self.list[a].0).total_cmp(&off(&self.list[b].0)));
            match nearest {
                Some(i) => {
                    taken[i] = true;
                    let (shown, looks, seen) = &mut self.list[i];
                    if off(shown) > 1.5 {
                        (shown.x, shown.y) = (f.x, f.y);
                    }
                    if (shown.diameter - f.diameter).abs() > 2.0 {
                        shown.diameter = f.diameter;
                    }
                    shown.score = f.score;
                    *looks = looks.saturating_add(1);
                    *seen = now;
                }
                None => self.list.push((*f, 1, now)),
            }
        }
    }

    pub fn clear(&mut self) {
        self.list.clear();
    }

    /// The prompts that are there at `now`, always in the same order.
    pub fn there(&self, now: f64, keep: f64) -> Vec<Found> {
        self.list.iter().filter(|(_, looks, seen)| *looks >= 2 && now - seen <= keep).map(|(shown, _, _)| *shown).collect()
    }
}

/// A prompt found in the picture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Found {
    pub glyph: Glyph,
    /// Centre of the prompt's face, in pixels.
    pub x: f32,
    pub y: f32,
    /// How large the face is: a disc's diameter, a badge's longer side.
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

/// A badge's corner counts as square when this much of it is face, and as rounded off when no
/// more than this much is.
const CORNER_SQUARE: f32 = 0.60;
const CORNER_ROUNDED: f32 = 0.40;

/// The dark pattern of each mark on the grid, as the game draws them, and the outline it comes
/// in. All are read off saved game pictures. A mark can have more than one pattern: the game
/// draws the square small in its lists and larger and bolder in the weapon wheel and the menus.
const PATTERNS: [(Glyph, Outline, [&str; GRID]); 14] = [
    (
        Glyph::Triangle,
        Outline::Disc,
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
        Outline::Disc,
        [
            "......###......",
            "....#######....",
            "..###########..",
            ".####.....####.",
            ".###.......###.",
            ".##.........##.",
            ".##.........##.",
            ".##.........##.",
            ".##.........##.",
            ".##.........##.",
            ".###.......###.",
            ".####.....####.",
            "..###########..",
            "....#######....",
            "......###......",
        ],
    ),
    (
        Glyph::Cross,
        Outline::Disc,
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
        Outline::Disc,
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
        // The small square of the lists.
        Glyph::Square,
        Outline::Disc,
        [
            "...............",
            "...............",
            "..###########..",
            "..###########..",
            "..##.......##..",
            "..##.......##..",
            "..##.......##..",
            "..##.......##..",
            "..##.......##..",
            "..##.......##..",
            "..##.......##..",
            "..###########..",
            "..###########..",
            "...............",
            "...............",
        ],
    ),
    (
        // The larger, bolder square of the weapon wheel and the menus. It covers nearly all of
        // the circle's ring: what sets it apart are its corners.
        Glyph::Square,
        Outline::Disc,
        [
            "...............",
            ".#############.",
            ".#############.",
            ".###.......###.",
            ".###........##.",
            ".###........##.",
            ".###........##.",
            ".###........##.",
            ".###........##.",
            ".###........##.",
            ".###........##.",
            ".#############.",
            ".#############.",
            ".#############.",
            "...............",
        ],
    ),
    (
        Glyph::LeftStick,
        Outline::Disc,
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
        Outline::Disc,
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
    (
        Glyph::L1,
        Outline::Shoulder,
        [
            "...............",
            "...##.....##...",
            "...##....###...",
            "...##...####...",
            "...##...####...",
            "...##...####...",
            "...##...#.##...",
            "...##.....##...",
            "...##.....##...",
            "...##.....##...",
            "...##.....##...",
            "...######.##...",
            "...######.##...",
            "...######.##...",
            "...............",
        ],
    ),
    (
        Glyph::R1,
        Outline::Shoulder,
        [
            "...............",
            ".######.....##.",
            ".######....###.",
            ".#######..####.",
            ".##..####.####.",
            ".##...###.####.",
            ".##...###.#.##.",
            ".#######....##.",
            ".######.....##.",
            ".##..##.....##.",
            ".##..##.....##.",
            ".##..###....##.",
            ".##..###....##.",
            ".##...###...##.",
            ".##....##...##.",
        ],
    ),
    (
        Glyph::L2,
        Outline::Trigger,
        [
            "...............",
            "...............",
            "##........###..",
            "###.....######.",
            "###.....#######",
            "###.....##...##",
            "###..........##",
            "###.........###",
            "###.......###..",
            "###.......###..",
            "#######.#######",
            "#######.#######",
            "#######.#######",
            "...............",
            "...............",
        ],
    ),
    (
        Glyph::R2,
        Outline::Trigger,
        [
            "...............",
            "...............",
            "#####.....####.",
            "######...######",
            "#######..######",
            "##...##.##...##",
            "##...##......##",
            "#######......##",
            "#####......###.",
            "##.###....###..",
            "##..###..######",
            "##...##.#######",
            "##...##.#######",
            "...............",
            "...............",
        ],
    ),
    (
        Glyph::L3,
        Outline::Disc,
        [
            "...............",
            "...............",
            "...............",
            ".##.....######.",
            ".##.....######.",
            ".##.......###..",
            ".##......####..",
            ".##......#####.",
            ".##.........##.",
            ".##....##...##.",
            ".######.######.",
            ".######..#####.",
            "..........##...",
            "...............",
            "...............",
        ],
    ),
    (
        Glyph::R3,
        Outline::Disc,
        [
            "...............",
            "...............",
            "...............",
            ".#####..######.",
            ".######.######.",
            ".##..##...###..",
            ".##..##...###..",
            ".######..#####.",
            ".#####......##.",
            ".#####..##..##.",
            ".##.###.######.",
            ".##..##..#####.",
            "..........##...",
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

/// Which mark a dark pattern on a face of this outline is, if it is clearly one of them.
fn classify(seen: &Grid, outline: Outline) -> Option<(Glyph, f32)> {
    // A mark with several patterns scores as its best one.
    let mut scores: Vec<(Glyph, f32)> = Vec::new();
    for (glyph, _, rows) in PATTERNS.iter().filter(|(_, o, _)| *o == outline) {
        let score = alike_nearby(seen, &pattern(rows));
        match scores.iter_mut().find(|(g, _)| g == glyph) {
            Some(known) => known.1 = known.1.max(score),
            None => scores.push((*glyph, score)),
        }
    }
    scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    let (best, second) = (scores[0], scores[1]);
    (best.1 >= MATCH_AT_LEAST && best.1 - second.1 >= LEAD_AT_LEAST).then_some(best)
}

#[cfg(test)]
fn find(rgb: &[u8], w: usize, h: usize) -> Vec<Found> {
    find_with(rgb, w, h, || {})
}

/// Finds the prompts in a picture given as RGB bytes, rows from the top. The brightness levels
/// are searched side by side, each on a thread of its own, which takes about half the time of
/// one after the other; `on_thread` runs first on each of those threads (to lower its priority,
/// say).
pub fn find_with(rgb: &[u8], w: usize, h: usize, on_thread: impl Fn() + Sync) -> Vec<Found> {
    let mut found: Vec<Found> = Vec::new();
    if w < 320 || h < 180 || rgb.len() < w * h * 3 {
        return found;
    }
    let lum: Vec<u8> = rgb.chunks_exact(3).take(w * h).map(|p| ((p[0] as u16 + p[1] as u16 + p[2] as u16) / 3) as u8).collect();
    let (lum, on_thread) = (&lum, &on_thread);
    let per_level: Vec<Vec<Found>> = std::thread::scope(|scope| {
        let searches: Vec<_> = LEVELS
            .iter()
            .map(|&level| {
                scope.spawn(move || {
                    on_thread();
                    prompts_at(lum, w, h, level)
                })
            })
            .collect();
        searches.into_iter().map(|search| search.join().unwrap_or_default()).collect()
    });
    for at_level in per_level {
        for disc in at_level {
            // The same face usually stands alone at more than one level: keep its best reading.
            match found.iter_mut().find(|f| (f.x - disc.x).abs() < disc.diameter / 2.0 && (f.y - disc.y).abs() < disc.diameter / 2.0) {
                Some(seen) if seen.score < disc.score => *seen = disc,
                Some(_) => {}
                None => found.push(disc),
            }
        }
    }
    found
}

/// The prompts whose face stands alone when the picture is cut at `level`.
fn prompts_at(lum: &[u8], w: usize, h: usize, level: u8) -> Vec<Found> {
    let mut found = Vec::new();
    // Which connected area each pixel belongs to (0: none yet), and the pixels still to walk.
    let mut area = vec![0u32; w * h];
    let mut stack: Vec<u32> = Vec::new();
    let scale = h as f32 / 1080.0;
    let (min_d, max_d) = ((MIN_DIAMETER * scale) as i32, (MAX_DIAMETER * scale).ceil() as i32);
    // How far outside a face its rim is looked for, and how far from the picture's edge a face
    // must stay so that there is something to look at.
    let rim_out = 1.5 * scale.max(1.0);
    let edge = rim_out.ceil() as usize + 3;
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
        if bw < min_d || bh < min_d || bw > max_d || bh > max_d {
            continue;
        }
        if x0 < edge || y0 < edge || x1 + edge >= w || y1 + edge >= h {
            continue;
        }
        // The outline shows in the corners of the bounding box: a disc leaves all four empty, a
        // shoulder badge fills the upper two, a trigger badge the lower two.
        let member = |x: usize, y: usize| area[y * w + x] == id;
        let k = ((0.15 * bw.min(bh) as f32).round() as usize).max(2);
        let corner = |left: bool, top: bool| {
            let filled = (0..k * k).filter(|n| member(if left { x0 + n % k } else { x1 - n % k }, if top { y0 + n / k } else { y1 - n / k })).count();
            filled as f32 / (k * k) as f32
        };
        let (top, bottom) = ([corner(true, true), corner(false, true)], [corner(true, false), corner(false, false)]);
        let square = |pair: [f32; 2]| pair[0].min(pair[1]) >= CORNER_SQUARE;
        let rounded = |pair: [f32; 2]| pair[0].max(pair[1]) <= CORNER_ROUNDED;
        let as_wide_as_tall = (bw - bh).abs() <= (3.0 * scale.max(1.0)) as i32;
        let wider = bw * 100 >= bh * 112 && bw * 100 <= bh * 145;
        let outline = if wider && square(top) && rounded(bottom) {
            Outline::Shoulder
        } else if as_wide_as_tall && rounded(top) && square(bottom) {
            Outline::Trigger
        } else if as_wide_as_tall && !square(top) && !square(bottom) {
            Outline::Disc
        } else {
            continue;
        };
        let (cx, cy) = (x0 as f32 + bw as f32 / 2.0 - 0.5, y0 as f32 + bh as f32 / 2.0 - 0.5);
        let face = (total / count as u64) as i16;
        // Half the face's width and height, and the size to report.
        let (rx, ry, diameter) = if outline == Outline::Disc {
            let r = (bw + bh) as f32 / 4.0;
            // A disc's face fills a good part of its circle. The mark takes the rest, and a square
            // or circle mark also cuts off the face inside it, leaving about a third.
            let covered = count as f32 / (std::f32::consts::PI * r * r);
            if !(0.30..=1.02).contains(&covered) {
                continue;
            }
            let around = |radius: f32| {
                (0..48).map(move |i| {
                    let a = i as f32 * std::f32::consts::TAU / 48.0;
                    ((cy + radius * a.sin()).round() as usize) * w + (cx + radius * a.cos()).round() as usize
                })
            };
            // Just inside the edge it is face all the way round; just outside it, rim all the way round.
            let face_ring = around(0.86 * r).filter(|&i| area[i] == id).count() as f32 / 48.0;
            let rim_ring = around(r + rim_out).filter(|&i| lum[i] < level).count() as f32 / 48.0;
            // (A square mark's corners reach nearly to the edge, so the face ring is allowed some gaps.)
            if face_ring < 0.72 || rim_ring < 0.85 {
                continue;
            }
            (r, r, 2.0 * r)
        } else {
            // A badge's face fills its box but for the two rounded corners and the letters.
            let filled = count as f32 / (bw * bh) as f32;
            if !(0.45..=0.90).contains(&filled) {
                continue;
            }
            let shoulder = outline == Outline::Shoulder;
            let out = rim_out.round() as usize;
            let half = (bh / 2) as usize;
            // The flat edge is face from end to end. It is judged one row in: the outermost row
            // is where the face fades into the rim.
            let (flat_row, beyond_flat, beyond_round) = if shoulder { (y0 + 1, y0 - out, y1 + out) } else { (y1 - 1, y1 + out, y0 - out) };
            let flat = (x0..=x1).filter(|&x| member(x, flat_row)).count() as f32 / bw as f32;
            // Rim: past the flat edge, past both sides where they run straight, and past the
            // middle of the rounded edge.
            let straight_rows = if shoulder { y0..y0 + half } else { y1 + 1 - half..y1 + 1 };
            let quarter = (bw / 4) as usize;
            let rim = (x0..=x1)
                .map(|x| (x, beyond_flat))
                .chain(straight_rows.flat_map(|y| [(x0 - out, y), (x1 + out, y)]))
                .chain((x0 + quarter..=x1 - quarter).map(|x| (x, beyond_round)));
            let (mut dark, mut all) = (0u32, 0u32);
            for (x, y) in rim {
                all += 1;
                dark += (lum[y * w + x] < level) as u32;
            }
            if flat < 0.85 || (dark as f32) < 0.85 * all as f32 {
                continue;
            }
            (bw as f32 / 2.0, bh as f32 / 2.0, bw.max(bh) as f32)
        };
        let mut grid: Grid = [[false; GRID]; GRID];
        let half = (GRID - 1) as f32 / 2.0;
        for (gy, row) in grid.iter_mut().enumerate() {
            for (gx, cell) in row.iter_mut().enumerate() {
                let x = (cx + (gx as f32 - half) / half * GRID_REACH * rx).round() as usize;
                let y = (cy + (gy as f32 - half) / half * GRID_REACH * ry).round() as usize;
                *cell = (lum[y * w + x] as i16) < face - MARK_DARKER_BY;
            }
        }
        if let Some((glyph, score)) = classify(&grid, outline) {
            found.push(Found { glyph, x: cx, y: cy, diameter, score });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pattern_is_itself_and_no_other() {
        for (glyph, outline, rows) in &PATTERNS {
            assert_eq!(classify(&pattern(rows), *outline).map(|m| m.0), Some(*glyph));
        }
        // No mark may pass for another of the same outline, even when it sits a cell off: a
        // perfect drawing of one must beat every other by a wide margin. (The closest pairs are
        // the ones that differ in one letter only, L3 and R3, L2 and R2, and the circle and the
        // bold square, which differ in the corners.)
        for (i, (a, outline_a, rows_a)) in PATTERNS.iter().enumerate() {
            for (b, _, rows_b) in PATTERNS[i + 1..].iter().filter(|(b, o, _)| o == outline_a && b != a) {
                let likeness = alike_nearby(&pattern(rows_a), &pattern(rows_b));
                println!("{a:?} vs {b:?}: {likeness:.2}");
                assert!(likeness < 0.76, "{a:?} and {b:?} are too alike: {likeness}");
            }
        }
        for outline in [Outline::Disc, Outline::Shoulder, Outline::Trigger] {
            assert_eq!(classify(&[[false; GRID]; GRID], outline), None, "an empty face is no prompt");
        }
    }

    #[test]
    fn a_followed_prompt_needs_two_looks_moves_with_its_prompt_and_stays_a_moment() {
        let at = |glyph, x: f32, y: f32| Found { glyph, x, y, diameter: 26.0, score: 0.9 };
        let places = |t: &Tracks, now| t.there(now, 0.3).iter().map(|f| (f.glyph, f.x, f.y)).collect::<Vec<_>>();
        let mut t = Tracks::default();
        t.look(&[at(Glyph::Cross, 100.0, 100.0)], 0.0, 0.3);
        assert!(places(&t, 0.0).is_empty(), "one look is not enough");
        t.look(&[at(Glyph::Cross, 101.0, 100.0)], 0.1, 0.3);
        assert_eq!(places(&t, 0.1), [(Glyph::Cross, 100.0, 100.0)], "a pixel of wobble does not move it");
        t.look(&[at(Glyph::Cross, 112.0, 104.0)], 0.2, 0.3);
        assert_eq!(places(&t, 0.2), [(Glyph::Cross, 112.0, 104.0)], "it follows its prompt");
        // Missed for a look or two: still there. Missed for longer than the stay: gone.
        t.look(&[], 0.3, 0.3);
        assert_eq!(places(&t, 0.45).len(), 1);
        assert!(places(&t, 0.55).is_empty());
        t.look(&[at(Glyph::Cross, 112.0, 104.0)], 0.6, 0.3);
        assert!(places(&t, 0.6).is_empty(), "after it is forgotten it needs two looks again");
        // A jump further than the prompt's own width is another prompt; so is another mark in the same place.
        let mut t = Tracks::default();
        t.look(&[at(Glyph::Cross, 100.0, 100.0)], 0.0, 0.3);
        t.look(&[at(Glyph::Cross, 160.0, 100.0), at(Glyph::Circle, 100.0, 100.0)], 0.1, 0.3);
        assert!(places(&t, 0.1).is_empty());
        // Two of the same mark side by side stay two.
        let mut t = Tracks::default();
        for now in [0.0, 0.1] {
            t.look(&[at(Glyph::L1, 100.0, 100.0), at(Glyph::L1, 145.0, 100.0)], now, 0.3);
        }
        assert_eq!(places(&t, 0.1), [(Glyph::L1, 100.0, 100.0), (Glyph::L1, 145.0, 100.0)]);
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
        // With quick looks it takes more of them to drop a mark.
        s.set_patience(6);
        s.look(&[Cross]);
        s.look(&[Cross]);
        for _ in 0..5 {
            s.look(&[]);
        }
        assert_eq!(s.shown(), [Cross]);
        s.look(&[]);
        assert!(s.shown().is_empty());
    }

    /// A picture of grey-brown blocks, with prompts drawn at (cx, cy): a disc of `diameter`, or
    /// a badge that high (a shoulder badge is wider than that, as in the game).
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
            let (_, outline, rows) = PATTERNS.iter().find(|(g, _, _)| *g == glyph).unwrap();
            let mark = pattern(rows);
            let ry = diameter as f32 / 2.0;
            let rx = if *outline == Outline::Shoulder { ry * 28.0 / 22.0 } else { ry };
            for y in cy - diameter..=cy + diameter {
                for x in cx - diameter..=cx + diameter {
                    let (dx, dy) = (x as f32 - cx as f32, y as f32 - cy as f32);
                    // A badge is a box with two corners rounded off: the lower two or the upper two.
                    let bend = 0.7 * ry;
                    let (in_x, in_y) = (dx.abs() - (rx - bend), if *outline == Outline::Shoulder { dy } else { -dy } - (ry - bend));
                    let inside = match outline {
                        Outline::Disc => dx.hypot(dy) <= ry,
                        _ => dx.abs() <= rx && dy.abs() <= ry && !(in_x > 0.0 && in_y > 0.0 && in_x.hypot(in_y) > bend),
                    };
                    if !inside {
                        continue;
                    }
                    // Which grid cell this pixel falls in, the same way `find` reads it.
                    let half = (GRID - 1) as f32 / 2.0;
                    let (gx, gy) = ((dx / (GRID_REACH * rx) * half + half).round(), (dy / (GRID_REACH * ry) * half + half).round());
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
            (Glyph::L3, 620, 200, 30),
            (Glyph::R3, 820, 260, 30),
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
    fn shoulder_and_trigger_badges_are_found_and_told_apart() {
        // At the game's own size: the letters on a badge are too fine to draw any smaller.
        let (w, h) = (1920usize, 1080usize);
        let placed = [(Glyph::L1, 64, 62, 22), (Glyph::R1, 448, 62, 22), (Glyph::L2, 1821, 673, 26), (Glyph::R2, 1866, 673, 26), (Glyph::R2, 856, 364, 28), (Glyph::L3, 1866, 881, 26)];
        let found = find(&picture(w, h, &placed), w, h);
        assert_eq!(found.len(), placed.len(), "{found:?}");
        for (glyph, x, y, _) in placed {
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
        // The settings page that lists every button: two of each shoulder button, a stick click
        // by "Look behind" and one by "Pause preview". The drawing of the controller in the middle
        // has a cross button on it that reads as a prompt, and the two "Restore" prompts are dimmed.
        const EVERY_BUTTON: &[Glyph] = &[L1, L1, R1, R1, L2, R2, Triangle, Square, Circle, Circle, Cross, L3, R3, R3, LeftStick, RightStick];
        const PHOTO_MODE: &[Glyph] = &[LeftStick, RightStick, Triangle, Cross, Circle, L1, R1, L2, R2, L3];
        let expected: [(&str, &[Glyph], &[Glyph]); 27] = [
            ("1795460287_on_foot", &[Triangle], &[Square]),
            ("1795460357_on_foot", &[Circle, Cross, Square], &[]),
            ("1123_100027_should_be_drive", &[Cross], &[]),
            ("1123_100338_should_be_foot", &[LeftStick, LeftStick, RightStick, RightStick], &[]),
            ("1123_100733_should_be_foot", &[Triangle], &[]),
            ("1123_100734_should_be_drive", &[Triangle], &[]),
            ("1123_100734_should_be_foot", &[Triangle], &[]), // fading out, half see-through
            ("1795461500_driving", &[], &[Triangle]), // dark grey: not available
            ("1795461508_driving", &[], &[Triangle]),
            ("1795461544_driving", &[Triangle, R1, Square], &[]), // weapon wheel in a car
            ("1795461643_driving", &[Triangle], &[Square]), // the square is half-lit
            ("1795461708_on_foot", &[Cross, Square], &[]),
            ("1795461720_on_foot", &[Cross, Square], &[]),
            ("1795461463_on_foot", &[L1], &[]),
            ("1795463103_driving", &[LeftStick, RightStick], &[]), // hot-wiring
            ("1795463106_driving", &[LeftStick, RightStick], &[]),
            ("1795463161_on_foot", &[Square], &[Triangle]), // break in; clone key is dimmed
            ("1795463252_on_foot", &[Triangle, R1, L2, R2, Square], &[]), // weapon wheel
            ("1795463737_on_foot", PHOTO_MODE, &[Square]),
            ("1795464338_on_foot", PHOTO_MODE, &[Square]),
            ("1795464624_driving", &[Cross, L1, R1, R3, Square], &[]), // radio menu
            ("1795464689_on_foot", EVERY_BUTTON, &[Cross, Triangle, Square]),
            ("1795464692_on_foot", EVERY_BUTTON, &[Cross, Triangle, Square]),
            ("1795464718_on_foot", EVERY_BUTTON, &[Cross, Triangle, Square]),
            ("1795467833_on_foot", &[R1, Square, Triangle], &[]), // weapon wheel: its square and triangle are the bold ones
            ("1795467907_on_foot", &[R1, R3, Square, Triangle, Cross], &[Circle]), // with a weapon's card; "Drop" is dimmed
            ("1795467968_on_foot", &[R1, R3, Square, Triangle, Cross], &[Circle]),
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
                println!("{} {name}: {:?}{}", if right { "ok  " } else { "DIFF" }, found.iter().map(|f| format!("{:?} {:.2} d{:.0} @{:.0},{:.0}", f.glyph, f.score, f.diameter, f.x, f.y)).collect::<Vec<_>>(), if right { String::new() } else { format!("  wanted {want:?}") });
                frames += 1;
            }
        }
        println!("{frames} frames checked, {wrong} wrong, slowest search {slowest:.1} ms");
        assert!(frames > 0);
        assert_eq!(wrong, 0);
    }
}
