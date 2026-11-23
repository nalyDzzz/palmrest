//! A small see-through label on top of the game: which key layout is in use, and the keyboard
//! or mouse key for each button prompt the game currently has on screen.
//!
//! It is a window of its own that sits above the game window. Clicks pass straight through it, it
//! never takes the keyboard, and it has no taskbar entry. Because it is a separate window, the
//! game's own picture is untouched: nothing here ends up in a capture of the game window.
//!
//! Windows' plain drawing calls know nothing about see-through pixels, so each shape and each
//! piece of text is first drawn white on black at three times the size, and that drawing is then
//! used as a stencil to lay the wanted colour, smoothly edged, into the label.

use std::ffi::c_void;
use std::mem::size_of;
use std::sync::{Arc, Mutex, OnceLock};

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, ANTIALIASED_QUALITY, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, ClientToScreen, CreateCompatibleDC,
    CreateDIBSection, CreateFontW, DEFAULT_CHARSET, DIB_RGB_COLORS, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DeleteDC, DeleteObject,
    DT_CENTER, DrawTextW, Ellipse, FONT_CLIP_PRECISION, FONT_OUTPUT_PRECISION, GetStockObject, GetTextExtentPoint32W, HBITMAP, HDC, HFONT,
    LineTo, MoveToEx, NULL_BRUSH, PS_SOLID, Polygon, Polyline, Rectangle, RoundRect, SelectObject, SetBkMode, SetTextColor, TRANSPARENT,
    WHITE_BRUSH, WHITE_PEN,
};
use windows::Win32::Graphics::Gdi::{CreatePen, CreateRoundRectRgn, SelectClipRgn};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW, HWND_TOPMOST, IsIconic, MSG, RegisterClassExW, SW_HIDE,
    SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetTimer, SetWindowPos, ShowWindow, TranslateMessage, ULW_ALPHA,
    UpdateLayeredWindow, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::w;

use crate::config::Mode;
use crate::keys::sign;
use crate::prompts::Glyph;

/// What the label should show. The engine keeps this up to date.
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayState {
    pub show: bool,
    /// The game window to sit on top of.
    pub game_window: isize,
    pub mode: Mode,
    /// The button prompts on screen, each with the key to press for it.
    pub prompts: Vec<(Glyph, String)>,
    /// Prompts that are holding still: the key is drawn right on each of them.
    pub spots: Vec<Spot>,
}

/// A key hint that sits on one prompt.
#[derive(Clone, Debug, PartialEq)]
pub struct Spot {
    pub glyph: Glyph,
    pub key: String,
    /// Centre of the prompt in the game window, in pixels from its top left.
    pub x: i32,
    pub y: i32,
    pub diameter: i32,
}

/// The most on-prompt hints shown at once. The game's busiest pages (photo mode, the list of
/// every button in the settings) show ten to twenty prompts.
pub const SPOTS: usize = 24;
/// The windows for on-prompt hints, with what each shows and where. Only the overlay thread uses it.
static SPOT_WINDOWS: Mutex<Vec<(isize, Option<(Spot, (i32, i32))>)>> = Mutex::new(Vec::new());

impl Default for OverlayState {
    fn default() -> Self {
        OverlayState { show: false, game_window: 0, mode: Mode::Foot, prompts: Vec::new(), spots: Vec::new() }
    }
}

/// How often the label checks whether it should change or move, in milliseconds.
const CHECK_EVERY: u32 = 50;
/// The stencils are drawn this many times larger and shrunk, which smooths their edges.
const SMOOTH: i32 = 3;
const BACKING: (u8, u8, u8) = (0x12, 0x14, 0x1C);
const ON_FOOT: (u8, u8, u8) = (0xE8, 0xEA, 0xF2);
const DRIVING: (u8, u8, u8) = (0xFF, 0xB3, 0x47);

/// The colour of each button's mark: the ones printed on PlayStation pads.
fn tint(glyph: Glyph) -> (u8, u8, u8) {
    match glyph {
        Glyph::Triangle => (0x2F, 0xD6, 0xA6),
        Glyph::Circle => (0xFF, 0x5C, 0x6C),
        Glyph::Cross => (0x6E, 0xA8, 0xFF),
        Glyph::Square => (0xF0, 0x8F, 0xD6),
        Glyph::LeftStick | Glyph::RightStick | Glyph::L3 | Glyph::R3 => (0xB7, 0xA6, 0xFF),
        Glyph::L1 | Glyph::R1 | Glyph::L2 | Glyph::R2 => (0xE8, 0xC2, 0x7A),
    }
}

static STATE: OnceLock<Arc<Mutex<OverlayState>>> = OnceLock::new();

/// Starts the label's own thread. It shows nothing until the state says so.
pub fn start(state: Arc<Mutex<OverlayState>>) -> Result<(), String> {
    STATE.set(state).map_err(|_| "the overlay was already started".to_string())?;
    std::thread::Builder::new()
        .name("overlay".into())
        .spawn(|| unsafe {
            let Ok(instance) = GetModuleHandleW(None) else { return };
            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(window_proc),
                hInstance: instance.into(),
                lpszClassName: w!("Ps5RemoteSimOverlay"),
                ..Default::default()
            };
            if RegisterClassExW(&class) == 0 {
                return;
            }
            // Layered: per-pixel see-through. Transparent: clicks go to whatever is underneath.
            // No-activate and tool window: never takes the keyboard, no taskbar entry.
            let style = WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW;
            let Ok(window) = CreateWindowExW(style, w!("Ps5RemoteSimOverlay"), w!(""), WS_POPUP, 0, 0, 1, 1, None, None, Some(instance.into()), None)
            else {
                return;
            };
            // One small window per on-prompt hint, same kind as the label.
            if let Ok(mut pool) = SPOT_WINDOWS.lock() {
                for _ in 0..SPOTS {
                    if let Ok(spot) = CreateWindowExW(style, w!("Ps5RemoteSimOverlay"), w!(""), WS_POPUP, 0, 0, 1, 1, None, None, Some(instance.into()), None) {
                        pool.push((spot.0 as isize, None));
                    }
                }
            }
            SetTimer(Some(window), 1, CHECK_EVERY, None);
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// What is on screen now, so nothing is redrawn or moved without need. Only the overlay thread uses it.
static SHOWN: Mutex<Option<Shown>> = Mutex::new(None);

#[derive(Clone, PartialEq)]
struct Shown {
    mode: Mode,
    prompts: Vec<(Glyph, String)>,
    /// Height of the game picture the label was drawn for.
    picture_h: i32,
    at: (i32, i32),
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_TIMER {
        unsafe { refresh(hwnd) };
        return LRESULT(0);
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Brings the label in line with the state: shown or hidden, right text, right place.
unsafe fn refresh(hwnd: HWND) {
    let state = STATE.get().and_then(|s| s.lock().ok().map(|s| s.clone())).unwrap_or_default();
    unsafe { refresh_spots(&state) };
    let mut shown = SHOWN.lock().unwrap_or_else(|p| p.into_inner());
    let place = if state.show { unsafe { picture_rect(state.game_window) } } else { None };
    let Some(picture) = place else {
        if shown.take().is_some() {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
        }
        return;
    };
    let picture_h = picture.bottom - picture.top;
    let scale = (picture_h as f32 / 1080.0).clamp(0.5, 3.0);
    let redraw = shown.as_ref().is_none_or(|s| s.mode != state.mode || s.picture_h != picture_h || s.prompts != state.prompts);
    let label = if redraw { unsafe { draw_label(state.mode, &state.prompts, scale) } } else { None };
    // Bottom left of the game picture, just right of the minimap. The bottom right is where the
    // game lists its own prompts, and the bottom middle is for subtitles.
    let margin = (26.0 * scale) as i32;
    let inset = (352.0 * scale) as i32;
    let size = match (&label, shown.as_ref()) {
        (Some(l), _) => (l.w, l.h),
        (None, Some(_)) => (0, 0), // unchanged: only the position may need updating
        (None, None) => return,    // could not draw
    };
    unsafe {
        if let Some(label) = &label {
            let at = POINT { x: picture.left + inset, y: picture.bottom - margin - size.1 };
            let extent = SIZE { cx: label.w, cy: label.h };
            let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
            let _ = UpdateLayeredWindow(hwnd, None, Some(&at), Some(&extent), Some(label.dc), Some(&POINT::default()), COLORREF(0), Some(&blend), ULW_ALPHA);
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            *shown = Some(Shown { mode: state.mode, prompts: state.prompts.clone(), picture_h, at: (at.x, at.y) });
        } else if let Some(s) = shown.as_mut() {
            // Same label: follow the game window if it moved.
            let mut own = RECT::default();
            let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(hwnd, &mut own);
            let at = (picture.left + inset, picture.bottom - margin - (own.bottom - own.top));
            if at != s.at {
                let _ = UpdateLayeredWindow(hwnd, None, Some(&POINT { x: at.0, y: at.1 }), None, None, None, COLORREF(0), None, ULW_ALPHA);
                s.at = at;
            }
        }
        // Stay above the game even if it re-asserts itself.
        let _ = SetWindowPos(hwnd, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

/// Where a hint sits on the screen.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Place {
    /// The middle of the prompt the hint belongs to.
    prompt_x: i32,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    /// Which way the hint grows from its prompt when its key is wider than the prompt.
    grows_right: bool,
}

/// Makes hints in one row give way to each other, and keeps them between `left` and `right`.
///
/// The game shows some buttons side by side ("L2 R2 Zoom"), closer together than two hints are
/// wide. A hint that grows left moves further left to clear its right-hand neighbour; any other
/// moves right to clear its left-hand neighbour. Either way the hints stay in the order of their
/// prompts, touch each other, and between them still cover the prompts, and they still grow away
/// from the game's own words.
fn spread(places: &mut [Place], left: i32, right: i32, gap: i32) {
    let same_row = |a: &Place, b: &Place| a.y < b.y + b.h && b.y < a.y + a.h;
    let mut order: Vec<usize> = (0..places.len()).collect();
    order.sort_by_key(|&i| places[i].prompt_x);
    // The room a hint has: from the end of its nearest left-hand neighbours to the start of its
    // right-hand ones, in its row.
    let floor = |places: &[Place], n: usize, otherwise: i32| order[..n].iter().filter(|&&j| same_row(&places[j], &places[order[n]])).map(|&j| places[j].x + places[j].w + gap).max().unwrap_or(otherwise);
    let ceiling = |places: &[Place], n: usize, otherwise: i32| order[n + 1..].iter().filter(|&&j| same_row(&places[j], &places[order[n]])).map(|&j| places[j].x - gap).min().unwrap_or(otherwise);
    for (n, &i) in order.iter().enumerate().rev() {
        if !places[i].grows_right {
            places[i].x = places[i].x.min(ceiling(places, n, i32::MAX) - places[i].w);
        }
    }
    for (n, &i) in order.iter().enumerate() {
        places[i].x = places[i].x.max(floor(places, n, i32::MIN));
    }
    // A row pushed past the edge of the picture moves back in as a whole.
    for (n, &i) in order.iter().enumerate().rev() {
        places[i].x = places[i].x.min(ceiling(places, n, right) - places[i].w);
    }
    for (n, &i) in order.iter().enumerate() {
        places[i].x = places[i].x.max(floor(places, n, left));
    }
}

/// Brings the on-prompt hints in line with the state: one small window per prompt that holds
/// still, covering the prompt (the key takes the place of the button's mark) and kept inside
/// the game picture, so a prompt at the edge of the screen still gets a whole hint.
unsafe fn refresh_spots(state: &OverlayState) {
    unsafe {
        let mut pool = SPOT_WINDOWS.lock().unwrap_or_else(|p| p.into_inner());
        let picture = if state.show { picture_rect(state.game_window) } else { None };
        let mut origin = POINT::default();
        let placed = picture.is_some() && ClientToScreen(HWND(state.game_window as *mut c_void), &mut origin).as_bool();
        // First what each hint looks like and where it would sit if it were alone...
        let mut hints: Vec<(usize, Option<Surface>)> = Vec::new();
        let mut places: Vec<Place> = Vec::new();
        for (i, (hwnd, shown)) in pool.iter_mut().enumerate() {
            let window = HWND(*hwnd as *mut c_void);
            let (Some(picture), true, Some(spot)) = (picture, placed, state.spots.get(i)) else {
                if shown.take().is_some() {
                    let _ = ShowWindow(window, SW_HIDE);
                }
                continue;
            };
            let scale = ((picture.bottom - picture.top) as f32 / 1080.0).clamp(0.5, 3.0);
            let redraw = shown.as_ref().is_none_or(|(s, _)| s.glyph != spot.glyph || s.key != spot.key || s.diameter != spot.diameter);
            let hint = if redraw { draw_hint(&spot.key, spot.glyph, spot.diameter, scale) } else { None };
            let (w, h) = match &hint {
                Some(surface) => (surface.w, surface.h),
                None if redraw => continue, // could not draw
                None => {
                    let mut own = RECT::default();
                    let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(window, &mut own);
                    (own.right - own.left, own.bottom - own.top)
                }
            };
            // One end of the hint covers the prompt; the rest grows away from the game's own
            // words. In the lists at the right edge the words are left of the prompt, so the hint
            // grows right. Everywhere else the words are right of it, so the hint grows left.
            let prompt_x = origin.x + spot.x;
            let grows_right = prompt_x > picture.right - (picture.right - picture.left) / 10;
            let x = if grows_right { prompt_x - h / 2 } else { prompt_x + h / 2 - w };
            let y = (origin.y + spot.y - h / 2).clamp(picture.top, (picture.bottom - h).max(picture.top));
            hints.push((i, hint));
            places.push(Place { prompt_x, x, y, w, h, grows_right });
        }
        // ...then neighbours make room for each other...
        if let Some(picture) = picture {
            spread(&mut places, picture.left, picture.right, 1);
        }
        // ...and every hint is drawn and put in its place.
        for ((i, hint), Place { x, y, w, h, .. }) in hints.into_iter().zip(places) {
            let (hwnd, shown) = &mut pool[i];
            let window = HWND(*hwnd as *mut c_void);
            if let Some(surface) = &hint {
                let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
                let (at, extent) = (POINT { x, y }, SIZE { cx: w, cy: h });
                let _ = UpdateLayeredWindow(window, None, Some(&at), Some(&extent), Some(surface.dc), Some(&POINT::default()), COLORREF(0), Some(&blend), ULW_ALPHA);
                let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
            } else if shown.as_ref().is_some_and(|(_, at)| *at != (x, y)) {
                let _ = UpdateLayeredWindow(window, None, Some(&POINT { x, y }), None, None, None, COLORREF(0), None, ULW_ALPHA);
            }
            *shown = Some((state.spots[i].clone(), (x, y)));
            let _ = SetWindowPos(window, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

/// A stretch of a key's name: plain text, or one sign that is drawn (see `keys::sign`).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Run<'a> {
    Text(&'a str),
    Sign(char),
}

fn runs(text: &str) -> Vec<Run<'_>> {
    let mut out = Vec::new();
    let mut from = 0;
    for (i, c) in text.char_indices() {
        if sign::ALL.contains(&c) {
            if from < i {
                out.push(Run::Text(&text[from..i]));
            }
            out.push(Run::Sign(c));
            from = i + c.len_utf8();
        }
    }
    if from < text.len() {
        out.push(Run::Text(&text[from..]));
    }
    out
}

/// How wide and high a sign is drawn beside text of a font `em` high.
fn sign_size(sign: char, em: i32) -> (i32, i32) {
    // Height as a share of the font's, and width as a multiple of that height.
    let (height, shape) = match sign {
        sign::MOUSE_LEFT | sign::MOUSE_RIGHT | sign::MOUSE_MIDDLE | sign::MOUSE => (1.3, 0.68),
        sign::BACKSPACE => (0.72, 1.5),
        sign::ENTER => (0.72, 1.2),
        sign::SHIFT | sign::CAPS_LOCK => (0.84, 0.95),
        sign::TAB => (0.66, 1.5),
        sign::SPACE => (0.52, 2.3),
        sign::UP | sign::DOWN => (0.8, 0.75),
        _ => (0.62, 1.3),
    };
    let h = (height * em as f32).round() as i32;
    ((h as f32 * shape).round() as i32, h)
}

/// How thick a sign's lines are for a font `em` high: about as thick as the font's own strokes.
fn sign_stroke(em: i32) -> i32 {
    ((0.105 * em as f32).round() as i32).max(1)
}

/// The pieces of a key's name in a row: each with where it starts and how wide it is, and the
/// width of the whole. The font must be selected into `dc`.
unsafe fn laid_out<'a>(dc: HDC, text: &'a str, em: i32) -> (Vec<(Run<'a>, i32, i32)>, i32) {
    unsafe {
        let mut placed: Vec<(Run<'a>, i32, i32)> = Vec::new();
        let mut x = 0;
        for run in runs(text) {
            // A sign keeps a little air from its neighbour, unless a space in the text already gives it.
            if let Some((before, _, _)) = placed.last() {
                let spaced = matches!(before, Run::Text(t) if t.ends_with(' ')) || matches!(run, Run::Text(t) if t.starts_with(' '));
                if !spaced {
                    x += (0.16 * em as f32).round() as i32;
                }
            }
            let width = match run {
                Run::Text(t) => {
                    let wide: Vec<u16> = t.encode_utf16().collect();
                    let mut extent = SIZE::default();
                    let _ = GetTextExtentPoint32W(dc, &wide, &mut extent);
                    extent.cx
                }
                Run::Sign(c) => sign_size(c, em).0,
            };
            placed.push((run, x, width));
            x += width;
        }
        (placed, x)
    }
}

/// Draws a key's name, signs and all, white into a stencil: at the left of `area` or in its
/// middle, and in the middle of its height.
unsafe fn draw_keys(dc: HDC, font: HFONT, text: &str, em: i32, area: RECT, centred: bool) {
    unsafe {
        let previous_font = SelectObject(dc, font.into());
        let (placed, width) = laid_out(dc, text, em);
        let left = if centred { area.left + (area.right - area.left - width) / 2 } else { area.left };
        let stroke = sign_stroke(em);
        let pen = CreatePen(PS_SOLID, stroke, COLORREF(0x00FF_FFFF));
        let previous_pen = SelectObject(dc, pen.into());
        let previous_brush = SelectObject(dc, GetStockObject(NULL_BRUSH));
        for (run, x, w) in placed {
            match run {
                Run::Text(t) => {
                    let mut wide: Vec<u16> = t.encode_utf16().collect();
                    let mut at = RECT { left: left + x, top: area.top, right: left + x + w + em, bottom: area.bottom };
                    DrawTextW(dc, &mut wide, &mut at, DT_SINGLELINE | DT_VCENTER | DT_LEFT | DT_NOPREFIX);
                }
                Run::Sign(c) => {
                    let (w, h) = sign_size(c, em);
                    draw_sign(dc, c, left + x, (area.top + area.bottom - h) / 2, w, h, stroke);
                }
            }
        }
        SelectObject(dc, previous_brush);
        SelectObject(dc, previous_pen);
        SelectObject(dc, previous_font);
        let _ = DeleteObject(pen.into());
    }
}

/// Draws one sign in a box. The pen for its lines is selected, and no brush: shapes stay hollow
/// except where a part is filled on purpose (the pressed button of a mouse).
unsafe fn draw_sign(dc: HDC, sign: char, x: i32, y: i32, w: i32, h: i32, stroke: i32) {
    unsafe {
        // The lines are `stroke` thick around the points given: keep them inside the box.
        let (x, y, w, h) = (x + stroke / 2, y + stroke / 2, w - stroke, h - stroke);
        let at = |u: f32, v: f32| POINT { x: x + (u * w as f32).round() as i32, y: y + (v * h as f32).round() as i32 };
        let points = |list: &[(f32, f32)]| list.iter().map(|&(u, v)| at(u, v)).collect::<Vec<POINT>>();
        let line = |list: &[(f32, f32)]| {
            let _ = Polyline(dc, &points(list));
        };
        let outline = |list: &[(f32, f32)]| {
            let _ = Polygon(dc, &points(list));
        };
        match sign {
            sign::BACKSPACE => {
                outline(&[(0.0, 0.5), (0.32, 0.0), (1.0, 0.0), (1.0, 1.0), (0.32, 1.0)]);
                line(&[(0.5, 0.3), (0.8, 0.7)]);
                line(&[(0.8, 0.3), (0.5, 0.7)]);
            }
            sign::ENTER => {
                line(&[(1.0, 0.0), (1.0, 0.64), (0.0, 0.64)]);
                line(&[(0.3, 0.28), (0.0, 0.64), (0.3, 1.0)]);
            }
            sign::SHIFT => outline(&[(0.5, 0.0), (1.0, 0.52), (0.72, 0.52), (0.72, 1.0), (0.28, 1.0), (0.28, 0.52), (0.0, 0.52)]),
            sign::CAPS_LOCK => {
                outline(&[(0.5, 0.0), (1.0, 0.5), (0.72, 0.5), (0.72, 0.7), (0.28, 0.7), (0.28, 0.5), (0.0, 0.5)]);
                line(&[(0.28, 1.0), (0.72, 1.0)]);
            }
            sign::TAB => {
                line(&[(0.0, 0.5), (0.78, 0.5)]);
                line(&[(0.52, 0.08), (0.8, 0.5), (0.52, 0.92)]);
                line(&[(1.0, 0.0), (1.0, 1.0)]);
            }
            sign::SPACE => line(&[(0.0, 0.0), (0.0, 1.0), (1.0, 1.0), (1.0, 0.0)]),
            sign::UP => {
                line(&[(0.5, 1.0), (0.5, 0.0)]);
                line(&[(0.0, 0.46), (0.5, 0.0), (1.0, 0.46)]);
            }
            sign::DOWN => {
                line(&[(0.5, 0.0), (0.5, 1.0)]);
                line(&[(0.0, 0.54), (0.5, 1.0), (1.0, 0.54)]);
            }
            sign::LEFT => {
                line(&[(1.0, 0.5), (0.0, 0.5)]);
                line(&[(0.38, 0.0), (0.0, 0.5), (0.38, 1.0)]);
            }
            sign::RIGHT => {
                line(&[(0.0, 0.5), (1.0, 0.5)]);
                line(&[(0.62, 0.0), (1.0, 0.5), (0.62, 1.0)]);
            }
            mouse => {
                // The body, round at both ends; a line under the buttons and one between them.
                let (a, b) = (at(0.0, 0.0), at(1.0, 1.0));
                let _ = RoundRect(dc, a.x, a.y, b.x + 1, b.y + 1, w, w);
                line(&[(0.0, 0.44), (1.0, 0.44)]);
                let white = GetStockObject(WHITE_BRUSH);
                if mouse == sign::MOUSE_MIDDLE {
                    // The wheel, filled.
                    let (p, q) = (at(0.33, 0.1), at(0.67, 0.36));
                    let hollow = SelectObject(dc, white);
                    let _ = RoundRect(dc, p.x, p.y, q.x + 1, q.y + 1, q.x - p.x, q.x - p.x);
                    SelectObject(dc, hollow);
                } else {
                    line(&[(0.5, 0.0), (0.5, 0.44)]);
                }
                // The button meant, filled, as far as the body goes.
                let button = match mouse {
                    sign::MOUSE_LEFT => Some((at(0.0, 0.0), at(0.5, 0.44))),
                    sign::MOUSE_RIGHT => Some((at(0.5, 0.0), at(1.0, 0.44))),
                    _ => None,
                };
                if let Some((p, q)) = button {
                    let body = CreateRoundRectRgn(a.x, a.y, b.x + 2, b.y + 2, w, w);
                    SelectClipRgn(dc, Some(body));
                    let hollow = SelectObject(dc, white);
                    let _ = Rectangle(dc, p.x, p.y, q.x + 1, q.y + 1);
                    SelectObject(dc, hollow);
                    SelectClipRgn(dc, None);
                    let _ = DeleteObject(body.into());
                }
            }
        }
    }
}

/// Is this mark drawn on a badge (L1, R1, L2, R2) and not on a disc?
fn on_a_badge(glyph: Glyph) -> bool {
    matches!(glyph, Glyph::L1 | Glyph::R1 | Glyph::L2 | Glyph::R2)
}

/// Draws one on-prompt hint: a dark backing at least as large as the prompt, with the key in
/// the button's colour. The backing has round ends on a disc and squarer corners on a badge, so
/// that it covers either whole.
unsafe fn draw_hint(key: &str, glyph: Glyph, diameter: i32, scale: f32) -> Option<Surface> {
    unsafe {
        let px = |v: f32| (v * scale).round() as i32;
        let s = SMOOTH;
        let em = px(16.0) * s;
        let font: HFONT = CreateFontW(-em, 0, 0, 0, 700, 0, 0, 0, DEFAULT_CHARSET, FONT_OUTPUT_PRECISION(0), FONT_CLIP_PRECISION(0), ANTIALIASED_QUALITY, 0, w!("Bahnschrift"));
        let probe = Surface::new(8, 8)?;
        let previous = SelectObject(probe.dc, font.into());
        let key_width = laid_out(probe.dc, key, em).1;
        SelectObject(probe.dc, previous);
        let h = (diameter + px(4.0)).max(px(24.0));
        let w = ((key_width + s - 1) / s + px(16.0)).max(h);
        let out = Surface::new(w, h)?;
        let stencil = Surface::new(w * s, h * s)?;
        SelectObject(stencil.dc, GetStockObject(WHITE_BRUSH));
        SelectObject(stencil.dc, GetStockObject(WHITE_PEN));
        let bend = if on_a_badge(glyph) { px(9.0) * s } else { h * s };
        let _ = RoundRect(stencil.dc, 0, 0, w * s, h * s, bend, bend);
        // Nearly solid: the button's own mark must not show through under the key.
        lay(&out, &stencil, BACKING, 0.98);
        stencil.pixels().fill(0);
        SetBkMode(stencil.dc, TRANSPARENT);
        SetTextColor(stencil.dc, COLORREF(0x00FF_FFFF));
        draw_keys(stencil.dc, font, key, em, RECT { left: 0, top: 0, right: w * s, bottom: h * s }, true);
        lay(&out, &stencil, tint(glyph), 1.0);
        let _ = DeleteObject(font.into());
        Some(out)
    }
}

/// The 16:9 game picture inside the game window, in screen coordinates. `None` if the window is
/// gone or minimized.
unsafe fn picture_rect(game_window: isize) -> Option<RECT> {
    unsafe {
        let window = HWND(game_window as *mut c_void);
        if game_window == 0 || IsIconic(window).as_bool() {
            return None;
        }
        let mut client = RECT::default();
        GetClientRect(window, &mut client).ok()?;
        let (w, h) = (client.right, client.bottom);
        if w < 320 || h < 180 {
            return None;
        }
        let mut origin = POINT::default();
        if !ClientToScreen(window, &mut origin).as_bool() {
            return None;
        }
        let (pw, ph) = if w * 9 <= h * 16 { (w, w * 9 / 16) } else { (h * 16 / 9, h) };
        let (left, top) = (origin.x + (w - pw) / 2, origin.y + (h - ph) / 2);
        Some(RECT { left, top, right: left + pw, bottom: top + ph })
    }
}

/// A drawing surface Windows can draw on and whose pixels can be read and written directly.
struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    bits: *mut u32,
    w: i32,
    h: i32,
}

impl Surface {
    unsafe fn new(w: i32, h: i32) -> Option<Surface> {
        unsafe {
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // rows from the top
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits: *mut c_void = std::ptr::null_mut();
            let bitmap = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() || bits.is_null() {
                let _ = DeleteObject(bitmap.into());
                return None;
            }
            SelectObject(dc, bitmap.into());
            let surface = Surface { dc, bitmap, bits: bits as *mut u32, w, h };
            surface.pixels().fill(0);
            Some(surface)
        }
    }

    #[allow(clippy::mut_from_ref)]
    fn pixels(&self) -> &mut [u32] {
        // The bitmap's memory lives as long as the surface and is only touched from its thread.
        unsafe { std::slice::from_raw_parts_mut(self.bits, (self.w * self.h) as usize) }
    }
}

impl Drop for Surface {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteDC(self.dc);
            let _ = DeleteObject(self.bitmap.into());
        }
    }
}

/// Lays `color` into `out` wherever the stencil is white, as strongly as `strength` says (0..1).
/// The stencil is `SMOOTH` times the size of `out`; averaging its pixels gives smooth edges.
fn lay(out: &Surface, stencil: &Surface, color: (u8, u8, u8), strength: f32) {
    let (pixels, mask) = (out.pixels(), stencil.pixels());
    let n = (SMOOTH * SMOOTH) as u32;
    for y in 0..out.h {
        for x in 0..out.w {
            let mut white = 0u32;
            for sy in 0..SMOOTH {
                let row = ((y * SMOOTH + sy) * stencil.w + x * SMOOTH) as usize;
                for sx in 0..SMOOTH as usize {
                    white += mask[row + sx] & 0xFF;
                }
            }
            let cover = white as f32 / (255.0 * n as f32) * strength;
            if cover <= 0.0 {
                continue;
            }
            // Pixels are kept with colour already multiplied by opacity, as layered windows need.
            let p = &mut pixels[(y * out.w + x) as usize];
            let old = [(*p & 0xFF) as f32, (*p >> 8 & 0xFF) as f32, (*p >> 16 & 0xFF) as f32, (*p >> 24) as f32];
            let new = [color.2 as f32 * cover, color.1 as f32 * cover, color.0 as f32 * cover, 255.0 * cover];
            let mix = |i: usize| (new[i] + old[i] * (1.0 - cover)).round().clamp(0.0, 255.0) as u32;
            *p = mix(0) | mix(1) << 8 | mix(2) << 16 | mix(3) << 24;
        }
    }
}

/// Draws the label: a dark rounded backing, a dot and the layout's name, then for every prompt
/// on screen the button's mark and the key to press for it.
unsafe fn draw_label(mode: Mode, prompts: &[(Glyph, String)], scale: f32) -> Option<Surface> {
    unsafe {
        let (name, color) = match mode {
            Mode::Foot => ("ON FOOT", ON_FOOT),
            Mode::Drive => ("DRIVING", DRIVING),
        };
        let px = |v: f32| (v * scale).round() as i32;
        let s = SMOOTH;
        let face = |size: f32, weight: i32| -> HFONT {
            CreateFontW(
                -px(size) * s,
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                FONT_OUTPUT_PRECISION(0),
                FONT_CLIP_PRECISION(0),
                ANTIALIASED_QUALITY,
                0,
                w!("Bahnschrift"),
            )
        };
        let (font, small) = (face(16.0, 600), face(11.0, 700));
        // Measure every piece of text to size the label.
        let probe = Surface::new(8, 8)?;
        let previous_font = SelectObject(probe.dc, font.into());
        let width_of = |text: &str| {
            let wide: Vec<u16> = text.encode_utf16().collect();
            let mut extent = SIZE::default();
            let _ = GetTextExtentPoint32W(probe.dc, &wide, &mut extent);
            (extent.cx + s - 1) / s
        };
        let name_w = width_of(name);
        let em = px(16.0) * s;
        let key_w: Vec<i32> = prompts.iter().map(|(_, keys)| (laid_out(probe.dc, keys, em).1 + s - 1) / s).collect();
        SelectObject(probe.dc, previous_font);

        let (h, pad, dot, mark) = (px(30.0).max(14), px(12.0), px(8.0).max(4), px(17.0).max(9));
        let (gap, tight) = (px(15.0), px(6.0));
        // A mark with two characters in it (L1, R2, L3...) needs a wider frame than a single shape.
        let two_characters = |glyph: Glyph| on_a_badge(glyph) || matches!(glyph, Glyph::L3 | Glyph::R3);
        let mark_w: Vec<i32> = prompts.iter().map(|(glyph, _)| if two_characters(*glyph) { px(25.0).max(13) } else { mark }).collect();
        let w = pad + dot + px(8.0) + name_w + key_w.iter().zip(&mark_w).map(|(k, m)| gap + m + tight + k).sum::<i32>() + pad;

        let out = Surface::new(w, h)?;
        let stencil = Surface::new(w * s, h * s)?;
        let pen = CreatePen(PS_SOLID, ((2.0 * scale).round() as i32).max(1) * s, COLORREF(0x00FF_FFFF));
        SetBkMode(stencil.dc, TRANSPARENT);
        SetTextColor(stencil.dc, COLORREF(0x00FF_FFFF));
        let text_at = |text: &str, left: i32, right: i32, font: HFONT, centred: bool| {
            let mut wide: Vec<u16> = text.encode_utf16().collect();
            let previous = SelectObject(stencil.dc, font.into());
            let mut area = RECT { left: left * s, top: 0, right: right * s, bottom: h * s };
            let across = if centred { DT_CENTER } else { DT_LEFT };
            DrawTextW(stencil.dc, &mut wide, &mut area, DT_SINGLELINE | DT_VCENTER | across | DT_NOPREFIX);
            SelectObject(stencil.dc, previous);
        };

        // Backing.
        SelectObject(stencil.dc, GetStockObject(WHITE_BRUSH));
        SelectObject(stencil.dc, GetStockObject(WHITE_PEN));
        let _ = RoundRect(stencil.dc, 0, 0, w * s, h * s, h * s, h * s);
        lay(&out, &stencil, BACKING, 0.82);
        // Dot and layout name, in the layout's colour.
        stencil.pixels().fill(0);
        let (cx, cy) = ((pad + dot / 2) * s, h * s / 2);
        let _ = Ellipse(stencil.dc, cx - dot * s / 2, cy - dot * s / 2, cx + dot * s / 2, cy + dot * s / 2);
        let mut x = pad + dot + px(8.0);
        text_at(name, x, x + name_w + 2, font, false);
        lay(&out, &stencil, color, 1.0);
        x += name_w;

        // Each prompt: the button's mark as an outline in its own colour...
        let mut key_left = Vec::new();
        for (((glyph, _), key_width), mark_width) in prompts.iter().zip(&key_w).zip(&mark_w) {
            x += gap;
            stencil.pixels().fill(0);
            SelectObject(stencil.dc, GetStockObject(NULL_BRUSH));
            SelectObject(stencil.dc, pen.into());
            let (cx, cy, r) = ((x * s + mark * s / 2) as f32, (h * s / 2) as f32, (mark * s / 2) as f32);
            let at = |dx: f32, dy: f32| POINT { x: (cx + dx * r).round() as i32, y: (cy + dy * r).round() as i32 };
            match glyph {
                Glyph::Triangle => {
                    let _ = Polygon(stencil.dc, &[at(0.0, -0.82), at(0.9, 0.68), at(-0.9, 0.68)]);
                }
                Glyph::Circle => {
                    let (a, b) = (at(-0.8, -0.8), at(0.8, 0.8));
                    let _ = Ellipse(stencil.dc, a.x, a.y, b.x, b.y);
                }
                Glyph::Square => {
                    let (a, b) = (at(-0.72, -0.72), at(0.72, 0.72));
                    let _ = Rectangle(stencil.dc, a.x, a.y, b.x, b.y);
                }
                Glyph::Cross => {
                    for (from, to) in [(at(-0.7, -0.7), at(0.7, 0.7)), (at(-0.7, 0.7), at(0.7, -0.7))] {
                        let _ = MoveToEx(stencil.dc, from.x, from.y, None);
                        let _ = LineTo(stencil.dc, to.x, to.y);
                    }
                }
                Glyph::LeftStick | Glyph::RightStick => {
                    let (a, b) = (at(-0.95, -0.95), at(0.95, 0.95));
                    let _ = Ellipse(stencil.dc, a.x, a.y, b.x, b.y);
                    text_at(if *glyph == Glyph::LeftStick { "L" } else { "R" }, x, x + mark, small, true);
                }
                Glyph::L1 | Glyph::R1 | Glyph::L2 | Glyph::R2 | Glyph::L3 | Glyph::R3 => {
                    // The button's name in a frame: squarish for a badge, round-ended for a stick click.
                    let name = match glyph {
                        Glyph::L1 => "L1",
                        Glyph::R1 => "R1",
                        Glyph::L2 => "L2",
                        Glyph::R2 => "R2",
                        Glyph::L3 => "L3",
                        _ => "R3",
                    };
                    let (top, bottom) = ((h - mark) * s / 2, (h + mark) * s / 2);
                    let bend = if on_a_badge(*glyph) { px(7.0) * s } else { mark * s };
                    let _ = RoundRect(stencil.dc, x * s + s, top, (x + mark_width) * s - s, bottom, bend, bend);
                    text_at(name, x, x + mark_width, small, true);
                }
            }
            lay(&out, &stencil, tint(*glyph), 1.0);
            x += mark_width + tight;
            key_left.push(x);
            x += key_width;
        }
        // ...and beside it the key, all in one pass.
        stencil.pixels().fill(0);
        for (((_, keys), left), key_width) in prompts.iter().zip(&key_left).zip(&key_w) {
            draw_keys(stencil.dc, font, keys, em, RECT { left: left * s, top: 0, right: (left + key_width + 2) * s, bottom: h * s }, false);
        }
        lay(&out, &stencil, ON_FOOT, 1.0);

        let _ = DeleteObject(pen.into());
        let _ = DeleteObject(font.into());
        let _ = DeleteObject(small.into());
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A hint `w` wide and 30 high whose prompt is at `prompt_x`, placed as it would be alone.
    fn hint(prompt_x: i32, y: i32, w: i32, grows_right: bool) -> Place {
        Place { prompt_x, x: if grows_right { prompt_x - 15 } else { prompt_x + 15 - w }, y, w, h: 30, grows_right }
    }

    fn spans(places: &[Place]) -> Vec<(i32, i32)> {
        places.iter().map(|p| (p.x, p.x + p.w)).collect()
    }

    #[test]
    fn a_key_name_splits_into_text_and_signs() {
        use crate::keys::Input;
        let name = |key: &str| Input::from_name(key).unwrap().sign();
        assert_eq!(runs("Ctrl"), [Run::Text("Ctrl")]);
        assert_eq!(runs(&name("Backspace")), [Run::Sign(sign::BACKSPACE)]);
        assert_eq!(runs(&format!("{} / {}", name("Space"), name("Enter"))), [Run::Sign(sign::SPACE), Run::Text(" / "), Run::Sign(sign::ENTER)]);
        assert_eq!(runs(&name("Mouse4")), [Run::Sign(sign::MOUSE), Run::Text("4")]);
        assert_eq!(runs(&name("WheelUp")), [Run::Sign(sign::MOUSE_MIDDLE), Run::Sign(sign::UP)]);
        // Every sign has a size that leaves room for its lines.
        for c in sign::ALL {
            let (w, h) = sign_size(c, 48);
            assert!(w > 3 * sign_stroke(48) && h > 3 * sign_stroke(48), "{c:?}");
        }
    }

    #[test]
    fn a_hint_alone_stays_on_its_prompt_and_inside_the_picture() {
        let mut places = [hint(500, 100, 50, false), hint(1890, 300, 60, true), hint(10, 500, 50, false)];
        spread(&mut places, 0, 1920, 1);
        assert_eq!(spans(&places), [(465, 515), (1860, 1920), (0, 50)]);
    }

    #[test]
    fn neighbours_at_the_right_edge_give_way_to_the_right() {
        // "Zoom L2 R2": the prompts are 45 apart, the hints 51 wide.
        let mut places = [hint(1821, 658, 51, true), hint(1866, 658, 51, true)];
        spread(&mut places, 0, 1920, 1);
        assert_eq!(spans(&places), [(1806, 1857), (1858, 1909)], "the first stays on its prompt, the second starts where it ends");
        // With no room left at the edge, the row moves back in as a whole.
        let mut places = [hint(1821, 658, 70, true), hint(1866, 658, 70, true)];
        spread(&mut places, 0, 1920, 1);
        assert_eq!(spans(&places), [(1779, 1849), (1850, 1920)]);
    }

    #[test]
    fn neighbours_elsewhere_give_way_to_the_left() {
        // "L2 R2 Zoom": the words are on the right, so the right hint stays and the left one moves.
        let mut places = [hint(300, 900, 51, false), hint(345, 900, 51, false)];
        spread(&mut places, 0, 1920, 1);
        assert_eq!(spans(&places), [(257, 308), (309, 360)]);
        // A wide hint that reaches over its left-hand neighbour's prompt: the neighbour's hint
        // goes in front of it, in the order of the prompts.
        let mut places = [hint(300, 900, 30, false), hint(345, 900, 90, false)];
        spread(&mut places, 0, 1920, 1);
        assert_eq!(spans(&places), [(239, 269), (270, 360)]);
    }

    #[test]
    fn hints_in_other_rows_or_far_apart_are_left_alone() {
        let mut places = [hint(1866, 672, 51, true), hint(1866, 724, 51, true), hint(64, 62, 34, false), hint(448, 62, 34, false)];
        let before = places;
        spread(&mut places, 0, 1920, 1);
        assert_eq!(places, before);
    }
}
