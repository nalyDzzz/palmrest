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
    LineTo, MoveToEx, NULL_BRUSH, PS_SOLID, Polygon, Rectangle, RoundRect, SelectObject, SetBkMode, SetTextColor, TRANSPARENT, WHITE_BRUSH,
    WHITE_PEN,
};
use windows::Win32::Graphics::Gdi::CreatePen;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetMessageW, HWND_TOPMOST, IsIconic, MSG, RegisterClassExW, SW_HIDE,
    SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetTimer, SetWindowPos, ShowWindow, TranslateMessage, ULW_ALPHA,
    UpdateLayeredWindow, WM_TIMER, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT, WS_POPUP,
};
use windows::core::w;

use crate::config::Mode;
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

/// The most on-prompt hints shown at once.
const SPOTS: usize = 10;
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
        Glyph::LeftStick | Glyph::RightStick => (0xB7, 0xA6, 0xFF),
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

/// Brings the on-prompt hints in line with the state: one small window per prompt that holds
/// still, centred on the prompt (the key takes the place of the button's mark) and kept inside
/// the game picture, so a prompt at the edge of the screen still gets a whole hint.
unsafe fn refresh_spots(state: &OverlayState) {
    unsafe {
        let mut pool = SPOT_WINDOWS.lock().unwrap_or_else(|p| p.into_inner());
        let picture = if state.show { picture_rect(state.game_window) } else { None };
        let mut origin = POINT::default();
        let placed = picture.is_some() && ClientToScreen(HWND(state.game_window as *mut c_void), &mut origin).as_bool();
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
            let hint = if redraw { draw_hint(&spot.key, tint(spot.glyph), spot.diameter, scale) } else { None };
            let (w, h) = match &hint {
                Some(surface) => (surface.w, surface.h),
                None if redraw => continue, // could not draw
                None => {
                    let mut own = RECT::default();
                    let _ = windows::Win32::UI::WindowsAndMessaging::GetWindowRect(window, &mut own);
                    (own.right - own.left, own.bottom - own.top)
                }
            };
            // One round end of the hint covers the prompt; the rest grows away from the game's own
            // words. In the lists at the right edge the words are left of the prompt, so the hint
            // grows right. Everywhere else the words are right of it, so the hint grows left.
            let at_right_edge = origin.x + spot.x > picture.right - (picture.right - picture.left) / 10;
            let x = if at_right_edge { origin.x + spot.x - h / 2 } else { origin.x + spot.x + h / 2 - w };
            let x = x.clamp(picture.left, (picture.right - w).max(picture.left));
            let y = (origin.y + spot.y - h / 2).clamp(picture.top, (picture.bottom - h).max(picture.top));
            if let Some(surface) = &hint {
                let blend = BLENDFUNCTION { BlendOp: AC_SRC_OVER as u8, BlendFlags: 0, SourceConstantAlpha: 255, AlphaFormat: AC_SRC_ALPHA as u8 };
                let (at, extent) = (POINT { x, y }, SIZE { cx: w, cy: h });
                let _ = UpdateLayeredWindow(window, None, Some(&at), Some(&extent), Some(surface.dc), Some(&POINT::default()), COLORREF(0), Some(&blend), ULW_ALPHA);
                let _ = ShowWindow(window, SW_SHOWNOACTIVATE);
            } else if shown.as_ref().is_some_and(|(_, at)| *at != (x, y)) {
                let _ = UpdateLayeredWindow(window, None, Some(&POINT { x, y }), None, None, None, COLORREF(0), None, ULW_ALPHA);
            }
            *shown = Some((spot.clone(), (x, y)));
            let _ = SetWindowPos(window, Some(HWND_TOPMOST), 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE);
        }
    }
}

/// Draws one on-prompt hint: a dark pill at least as large as the prompt, with the key in the
/// button's colour.
unsafe fn draw_hint(key: &str, color: (u8, u8, u8), diameter: i32, scale: f32) -> Option<Surface> {
    unsafe {
        let px = |v: f32| (v * scale).round() as i32;
        let s = SMOOTH;
        let font: HFONT = CreateFontW(-px(16.0) * s, 0, 0, 0, 700, 0, 0, 0, DEFAULT_CHARSET, FONT_OUTPUT_PRECISION(0), FONT_CLIP_PRECISION(0), ANTIALIASED_QUALITY, 0, w!("Bahnschrift"));
        let mut wide: Vec<u16> = key.encode_utf16().collect();
        let probe = Surface::new(8, 8)?;
        let previous = SelectObject(probe.dc, font.into());
        let mut extent = SIZE::default();
        let _ = GetTextExtentPoint32W(probe.dc, &wide, &mut extent);
        SelectObject(probe.dc, previous);
        let h = (diameter + px(4.0)).max(px(24.0));
        let w = ((extent.cx + s - 1) / s + px(16.0)).max(h);
        let out = Surface::new(w, h)?;
        let stencil = Surface::new(w * s, h * s)?;
        SelectObject(stencil.dc, GetStockObject(WHITE_BRUSH));
        SelectObject(stencil.dc, GetStockObject(WHITE_PEN));
        let _ = RoundRect(stencil.dc, 0, 0, w * s, h * s, h * s, h * s);
        lay(&out, &stencil, BACKING, 0.94);
        stencil.pixels().fill(0);
        SetBkMode(stencil.dc, TRANSPARENT);
        SetTextColor(stencil.dc, COLORREF(0x00FF_FFFF));
        let previous = SelectObject(stencil.dc, font.into());
        let mut area = RECT { left: 0, top: 0, right: w * s, bottom: h * s };
        DrawTextW(stencil.dc, &mut wide, &mut area, DT_SINGLELINE | DT_VCENTER | DT_CENTER | DT_NOPREFIX);
        SelectObject(stencil.dc, previous);
        lay(&out, &stencil, color, 1.0);
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
        let key_w: Vec<i32> = prompts.iter().map(|(_, keys)| width_of(keys)).collect();
        SelectObject(probe.dc, previous_font);

        let (h, pad, dot, mark) = (px(30.0).max(14), px(12.0), px(8.0).max(4), px(17.0).max(9));
        let (gap, tight) = (px(15.0), px(6.0));
        let w = pad + dot + px(8.0) + name_w + key_w.iter().map(|k| gap + mark + tight + k).sum::<i32>() + pad;

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
        for ((glyph, _), key_width) in prompts.iter().zip(&key_w) {
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
            }
            lay(&out, &stencil, tint(*glyph), 1.0);
            x += mark + tight;
            key_left.push(x);
            x += key_width;
        }
        // ...and beside it the key, all in one pass.
        stencil.pixels().fill(0);
        for (((_, keys), left), key_width) in prompts.iter().zip(&key_left).zip(&key_w) {
            text_at(keys, *left, left + key_width + 2, font, false);
        }
        lay(&out, &stencil, ON_FOOT, 1.0);

        let _ = DeleteObject(pen.into());
        let _ = DeleteObject(font.into());
        let _ = DeleteObject(small.into());
        Some(out)
    }
}
