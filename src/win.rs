//! The Windows side: reading the keyboard and mouse, pinning the cursor, finding the window in front.
//!
//! Keys and mouse buttons come through low-level hooks, because a hook can keep an event from
//! reaching other programs. Mouse movement comes through Raw Input, which gives the mouse's own
//! counts at its own report rate, untouched by pointer speed or acceleration settings.

use std::ffi::c_void;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering::Relaxed};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Mutex, OnceLock};

use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, GetDIBits,
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTONULL, MONITORINFO, MonitorFromPoint, MonitorFromWindow, ReleaseDC, SelectObject,
};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::Media::timeBeginPeriod;
use windows::Win32::System::Diagnostics::Debug::Beep;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Performance::{QueryPerformanceCounter, QueryPerformanceFrequency};
use windows::Win32::System::Threading::{
    GetCurrentThread, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW, SetThreadPriority,
    THREAD_PRIORITY_BELOW_NORMAL, THREAD_PRIORITY_HIGHEST,
};
use windows::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER, RID_INPUT, RIDEV_INPUTSINK, RIM_TYPEMOUSE, RegisterRawInputDevices,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, ClipCursor, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect, GetForegroundWindow, GetMessageW, GetWindowRect,
    GetWindowThreadProcessId, IsIconic, HWND_MESSAGE, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, RegisterClassExW, SetWindowsHookExW, TranslateMessage,
    WH_KEYBOARD_LL, WH_MOUSE_LL, WINDOW_EX_STYLE, WINDOW_STYLE, WM_INPUT, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN,
    SetTimer, WM_MBUTTONUP, WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_TIMER, WM_XBUTTONDOWN, WM_XBUTTONUP, WNDCLASSEXW,
};
use windows::core::{PWSTR, w};

use crate::config::Layout;
use crate::engine::Msg;
use crate::hook::{Env, Hook, Out};
use crate::keys::Input;

/// Event flag bits: the event was made by a program, not a device.
const LLKHF_INJECTED: u32 = 0x10;
const LLKHF_ALTDOWN: u32 = 0x20;
const LLMHF_INJECTED: u32 = 0x01;
/// Raw mouse flag: the report is an absolute position (tablets, remote desktop), not a movement.
const MOUSE_MOVE_ABSOLUTE: u16 = 0x01;
/// If the engine has not checked in for this long, the hooks stop taking keys.
const ENGINE_SILENT_AFTER: f64 = 0.3;
/// How often the claim on raw mouse input is renewed, in milliseconds.
const RENEW_MOUSE_EVERY: u32 = 250;

static LIVE: AtomicBool = AtomicBool::new(false);
/// Test aid: pass mouse movement on even when not live.
pub static ALWAYS_REPORT_MOUSE: AtomicBool = AtomicBool::new(false);
/// Mouse movement reports seen since the program started, live or not.
pub static MOUSE_REPORTS: AtomicU64 = AtomicU64::new(0);
static CAN_TOGGLE: AtomicBool = AtomicBool::new(false);
static TOGGLE: AtomicU32 = AtomicU32::new(0x79);
/// The save-picture key, or `NO_KEY`.
static PICTURE: AtomicU32 = AtomicU32::new(NO_KEY);
const NO_KEY: u32 = u32::MAX;
static BOUND: [AtomicU64; Input::COUNT.div_ceil(64)] = [const { AtomicU64::new(0) }; Input::COUNT.div_ceil(64)];
static HEARTBEAT_MS: AtomicU64 = AtomicU64::new(0);
static TX: OnceLock<Sender<Msg>> = OnceLock::new();
static HOOK: Mutex<Hook> = Mutex::new(Hook::new());

/// Seconds since the program started, from the high-resolution counter.
pub fn now() -> f64 {
    static BASE: OnceLock<(i64, i64)> = OnceLock::new();
    let (freq, start) = *BASE.get_or_init(|| {
        let (mut freq, mut start) = (1i64, 0i64);
        unsafe {
            let _ = QueryPerformanceFrequency(&mut freq);
            let _ = QueryPerformanceCounter(&mut start);
        }
        (freq.max(1), start)
    });
    let mut count = 0i64;
    unsafe {
        let _ = QueryPerformanceCounter(&mut count);
    }
    (count - start) as f64 / freq as f64
}

/// Asks Windows for 1 ms timer resolution, so short waits are short.
pub fn fine_timers() {
    unsafe {
        timeBeginPeriod(1);
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
    }
}

/// Tells the hooks whether keys and mouse belong to the controller right now.
pub fn set_live(live: bool, can_toggle: bool) {
    LIVE.store(live, Relaxed);
    CAN_TOGGLE.store(can_toggle, Relaxed);
}

/// The engine calls this on every pass. If it stops, the hooks let everything through.
pub fn heartbeat() {
    HEARTBEAT_MS.store((now() * 1000.0) as u64, Relaxed);
}

/// Tells the hooks which inputs are bound.
pub fn set_bound(layout: &Layout) {
    TOGGLE.store(layout.toggle.0 as u32, Relaxed);
    PICTURE.store(layout.picture.map_or(NO_KEY, |i| i.0 as u32), Relaxed);
    for (word, slot) in BOUND.iter().enumerate() {
        let mut bits = 0u64;
        for bit in 0..64 {
            let id = word * 64 + bit;
            if id < Input::COUNT && layout.is_bound(Input(id as u16)) {
                bits |= 1 << bit;
            }
        }
        slot.store(bits, Relaxed);
    }
}

fn is_bound(input: Input) -> bool {
    BOUND.get(input.index() / 64).is_some_and(|w| w.load(Relaxed) >> (input.index() % 64) & 1 == 1)
}

fn env() -> Env {
    let engine_alive = now() - HEARTBEAT_MS.load(Relaxed) as f64 / 1000.0 < ENGINE_SILENT_AFTER;
    Env {
        live: LIVE.load(Relaxed) && engine_alive,
        can_toggle: CAN_TOGGLE.load(Relaxed) && engine_alive,
        toggle: Input(TOGGLE.load(Relaxed) as u16),
        picture: Some(PICTURE.load(Relaxed)).filter(|k| *k != NO_KEY).map(|k| Input(k as u16)),
    }
}

fn report(out: Out) {
    let t = now();
    let msg = match out {
        Out::Nothing => return,
        Out::Toggle => Msg::Toggle,
        Out::Input { input, down } => Msg::Input { t, input, down },
        Out::Wheel(input) => Msg::Wheel { t, input },
        Out::SavePicture => Msg::SavePicture,
    };
    if let Some(tx) = TX.get() {
        let _ = tx.send(msg);
    }
}

fn hook() -> std::sync::MutexGuard<'static, Hook> {
    HOOK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == 0 {
        let k = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if k.flags.0 & LLKHF_INJECTED == 0 {
            if let Some(input) = Input::from_vk(k.vkCode) {
                let down = matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
                let alt = k.flags.0 & LLKHF_ALTDOWN != 0;
                let (take, out) = hook().key(&env(), is_bound, input, down, alt);
                report(out);
                if take {
                    return LRESULT(1);
                }
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

unsafe extern "system" fn mouse_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let msg = wparam.0 as u32;
    // Movement is by far the most common message and never ours: get out of its way first.
    if code == 0 && matches!(msg, WM_LBUTTONDOWN..=WM_XBUTTONUP) {
        let m = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
        if m.flags & LLMHF_INJECTED == 0 {
            let high = (m.mouseData >> 16) as u16;
            let side = if high == 1 { Input::MOUSE_4 } else { Input::MOUSE_5 };
            let (take, out) = match msg {
                WM_LBUTTONDOWN => hook().button(&env(), is_bound, Input::MOUSE_LEFT, true),
                WM_LBUTTONUP => hook().button(&env(), is_bound, Input::MOUSE_LEFT, false),
                WM_RBUTTONDOWN => hook().button(&env(), is_bound, Input::MOUSE_RIGHT, true),
                WM_RBUTTONUP => hook().button(&env(), is_bound, Input::MOUSE_RIGHT, false),
                WM_MBUTTONDOWN => hook().button(&env(), is_bound, Input::MOUSE_MIDDLE, true),
                WM_MBUTTONUP => hook().button(&env(), is_bound, Input::MOUSE_MIDDLE, false),
                WM_XBUTTONDOWN => hook().button(&env(), is_bound, side, true),
                WM_XBUTTONUP => hook().button(&env(), is_bound, side, false),
                WM_MOUSEWHEEL => hook().wheel(&env(), is_bound, if (high as i16) > 0 { Input::WHEEL_UP } else { Input::WHEEL_DOWN }),
                _ => (false, Out::Nothing),
            };
            report(out);
            if take {
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Asks Windows to send raw mouse movement to `window`, also while another program is in front.
///
/// A program can have only one window receiving raw mouse input, and whoever asks last wins.
/// The library that draws our settings window asks for itself when it starts, which silently
/// takes the mouse away from here. So this is asked again on a timer: after the first renewal
/// the mouse is ours for good.
fn claim_mouse(window: HWND) -> windows::core::Result<()> {
    // Usage page 1, usage 2 = mouse. INPUTSINK: reports arrive while another window is in front.
    let device = RAWINPUTDEVICE { usUsagePage: 1, usUsage: 2, dwFlags: RIDEV_INPUTSINK, hwndTarget: window };
    unsafe { RegisterRawInputDevices(&[device], size_of::<RAWINPUTDEVICE>() as u32) }
}

unsafe extern "system" fn raw_input_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_TIMER {
        let _ = claim_mouse(hwnd);
        return LRESULT(0);
    }
    if msg == WM_INPUT {
        let mut raw = RAWINPUT::default();
        let mut size = size_of::<RAWINPUT>() as u32;
        let got = unsafe {
            GetRawInputData(HRAWINPUT(lparam.0 as *mut c_void), RID_INPUT, Some(&mut raw as *mut RAWINPUT as *mut c_void), &mut size, size_of::<RAWINPUTHEADER>() as u32)
        };
        if got != u32::MAX && raw.header.dwType == RIM_TYPEMOUSE.0 {
            let m = unsafe { raw.data.mouse };
            if m.usFlags.0 & MOUSE_MOVE_ABSOLUTE == 0 && (m.lLastX != 0 || m.lLastY != 0) {
                MOUSE_REPORTS.fetch_add(1, Relaxed);
                if LIVE.load(Relaxed) || ALWAYS_REPORT_MOUSE.load(Relaxed) {
                    if let Some(tx) = TX.get() {
                        let _ = tx.send(Msg::Mouse { t: now(), dx: m.lLastX, dy: m.lLastY });
                    }
                }
            }
        }
    }
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Starts the thread that reads the keyboard and mouse. Events are sent to `tx`.
/// Test aid (`--dry`): the window opens only to be looked at. The keyboard and mouse are not
/// read and no controller is plugged in, so it can be opened while a game is being played with
/// another copy of the program.
pub static DRY: AtomicBool = AtomicBool::new(false);

pub fn start_input(tx: Sender<Msg>) -> Result<(), String> {
    TX.set(tx).map_err(|_| "input was already started".to_string())?;
    if DRY.load(Relaxed) {
        return Ok(());
    }
    let (ready_tx, ready_rx) = channel::<Result<(), String>>();
    std::thread::Builder::new()
        .name("input".into())
        .spawn(move || {
            let setup = (|| -> Result<(), String> {
                unsafe {
                    let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_HIGHEST);
                    let instance = GetModuleHandleW(None).map_err(|e| e.to_string())?;
                    let class = WNDCLASSEXW {
                        cbSize: size_of::<WNDCLASSEXW>() as u32,
                        lpfnWndProc: Some(raw_input_proc),
                        hInstance: instance.into(),
                        lpszClassName: w!("Ps5RemoteSimRawInput"),
                        ..Default::default()
                    };
                    if RegisterClassExW(&class) == 0 {
                        return Err("could not register the input window class".into());
                    }
                    let window = CreateWindowExW(
                        WINDOW_EX_STYLE(0),
                        w!("Ps5RemoteSimRawInput"),
                        w!("input"),
                        WINDOW_STYLE(0),
                        0,
                        0,
                        0,
                        0,
                        Some(HWND_MESSAGE),
                        None,
                        Some(instance.into()),
                        None,
                    )
                    .map_err(|e| format!("could not create the input window: {e}"))?;
                    claim_mouse(window).map_err(|e| format!("could not read the mouse: {e}"))?;
                    if SetTimer(Some(window), 1, RENEW_MOUSE_EVERY, None) == 0 {
                        return Err("could not start the mouse renewal timer".into());
                    }
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(instance.into()), 0).map_err(|e| format!("could not watch the keyboard: {e}"))?;
                    SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), Some(instance.into()), 0).map_err(|e| format!("could not watch the mouse buttons: {e}"))?;
                }
                Ok(())
            })();
            let ok = setup.is_ok();
            let _ = ready_tx.send(setup);
            if !ok {
                return;
            }
            // The hooks and the raw input window are served by this loop. They end with the process.
            let mut msg = MSG::default();
            unsafe {
                while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        })
        .map_err(|e| e.to_string())?;
    ready_rx.recv().map_err(|e| e.to_string())?
}

/// The window in front and the file name of its program, lower case ("" if it cannot be read).
pub fn foreground() -> (isize, String) {
    unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
            return (hwnd.0 as isize, String::new());
        };
        let mut buf = [0u16; 520];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len).is_ok();
        let _ = CloseHandle(process);
        if !ok {
            return (hwnd.0 as isize, String::new());
        }
        let path = String::from_utf16_lossy(&buf[..len as usize]);
        let name = path.rsplit(['\\', '/']).next().unwrap_or("").to_ascii_lowercase();
        (hwnd.0 as isize, name)
    }
}

/// Pins the cursor to one pixel where it cannot be seen, so it neither wanders over the game
/// picture nor clicks anything. Mouse movement still arrives through Raw Input.
pub fn confine_cursor(hwnd: isize) {
    unsafe {
        let hwnd = HWND(hwnd as *mut c_void);
        let mut window = RECT::default();
        let _ = GetWindowRect(hwnd, &mut window);
        let (x, y) = hide_point(hwnd, window);
        let _ = ClipCursor(Some(&RECT { left: x, top: y, right: x + 1, bottom: y + 1 }));
    }
}

pub fn release_cursor() {
    unsafe {
        let _ = ClipCursor(None);
    }
}

/// A pixel on the bottom or right edge of the window's monitor with no other monitor beyond it:
/// the arrow hangs off the screen there. Otherwise the middle of the window.
unsafe fn hide_point(hwnd: HWND, window: RECT) -> (i32, i32) {
    unsafe {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        if GetMonitorInfoW(monitor, &mut info).as_bool() {
            let m = info.rcMonitor;
            let (mid_x, mid_y) = ((m.left + m.right) / 2, (m.top + m.bottom) / 2);
            for (x, y, beyond_x, beyond_y) in [(mid_x, m.bottom - 1, 0, 8), (m.right - 1, mid_y, 8, 0)] {
                if MonitorFromPoint(POINT { x: x + beyond_x, y: y + beyond_y }, MONITOR_DEFAULTTONULL).is_invalid() {
                    return (x, y);
                }
            }
        }
        ((window.left + window.right) / 2, (window.top + window.bottom) / 2)
    }
}

/// Starts the thread that looks for button prompts in the game picture. Send it a window to have
/// that window's picture searched; the marks found come back as `Msg::Prompts`. A request made
/// while the previous look is still running is dropped.
pub fn start_prompt_watcher(tx: Sender<Msg>) -> std::sync::mpsc::SyncSender<isize> {
    let (ask, asked) = std::sync::mpsc::sync_channel::<isize>(1);
    let _ = std::thread::Builder::new().name("prompts".into()).spawn(move || {
        unsafe {
            let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
        }
        while let Ok(hwnd) = asked.recv() {
            let started = now();
            let found = grab_window(hwnd).map(|(w, h, rgb)| {
                let (w, h) = (w as usize, h as usize);
                // The search's own threads keep out of the game's way like this one does.
                let prompts = crate::prompts::find_with(&rgb, w, h, || unsafe {
                    let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
                });
                crate::engine::Look { prompts, name_card: crate::prompts::name_card(&rgb, w, h), took: (now() - started) as f32 }
            });
            if tx.send(Msg::Look(found)).is_err() {
                break;
            }
        }
    });
    ask
}

/// Copies a window's own picture (not whatever may be on top of it): width, height, and RGB
/// bytes, rows from the top. `None` if the window is minimized or cannot be copied.
///
/// This waits for the window's program to answer, so it must not run on a thread that matters.
pub fn grab_window(hwnd: isize) -> Option<(u32, u32, Vec<u8>)> {
    unsafe {
        let window = HWND(hwnd as *mut c_void);
        if hwnd == 0 || IsIconic(window).as_bool() {
            return None;
        }
        let mut client = RECT::default();
        GetClientRect(window, &mut client).ok()?;
        let (w, h) = (client.right, client.bottom);
        if w <= 0 || h <= 0 {
            return None;
        }
        let screen = GetDC(Some(window));
        let dc = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, w, h);
        ReleaseDC(Some(window), screen);
        let mut rgb = None;
        if !dc.is_invalid() && !bitmap.is_invalid() {
            let previous = SelectObject(dc, bitmap.into());
            // 1 = client area only, 2 = include what the graphics card drew.
            let copied = PrintWindow(window, dc, PRINT_WINDOW_FLAGS(3)).as_bool();
            SelectObject(dc, previous);
            let mut info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: w,
                    biHeight: -h, // negative: rows from the top
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bgrx = vec![0u8; w as usize * h as usize * 4];
            let rows = GetDIBits(dc, bitmap, 0, h as u32, Some(bgrx.as_mut_ptr() as *mut c_void), &mut info, DIB_RGB_COLORS);
            if copied && rows == h {
                rgb = Some(bgrx.chunks_exact(4).flat_map(|p| [p[2], p[1], p[0]]).collect());
            }
        }
        if !bitmap.is_invalid() {
            let _ = DeleteObject(bitmap.into());
        }
        if !dc.is_invalid() {
            let _ = DeleteDC(dc);
        }
        rgb.map(|rgb| (w as u32, h as u32, rgb))
    }
}

/// Saves a picture of the window as a PNG file. Returns the file's path.
pub fn save_window_picture(hwnd: isize, path: &std::path::Path) -> Result<String, String> {
    let (w, h, rgb) = grab_window(hwnd).ok_or("the game window could not be copied")?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("could not create {}: {e}", dir.display()))?;
    }
    let file = std::fs::File::create(path).map_err(|e| format!("could not create {}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(&rgb).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// A short tone, without holding up the caller.
pub fn beep(frequency: u32, milliseconds: u32) {
    std::thread::spawn(move || unsafe {
        let _ = Beep(frequency, milliseconds);
    });
}
