//! A real controller connected to this computer, read alongside the keyboard and mouse.
//!
//! Some things in the game are made for two thumbs: flying, mostly. With a DualSense or a
//! DualShock 4 connected to the PC, by cable or Bluetooth, what it does is added to what the
//! keys and the mouse do, and both go out through the one virtual controller (`PadState::with`).
//! Pick the real one up and it works, put it down and the keys work, with nothing to switch.
//!
//! It counts from the moment it is touched until a key is next pressed or the mouse next moved
//! (`Real`). So one that lies on the desk on a trigger, or on its side against a stick, does
//! nothing by its own weight while the game is played on the keyboard.
//!
//! The controller is read from its own HID reports, which arrive whether or not our window is
//! in front. Its rumble, adaptive triggers, motion sensors and touchpad surface are not carried
//! over: the virtual controller only has buttons, sticks and triggers to give.

use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
use std::sync::mpsc::Sender;

use crate::engine::Msg;
use crate::pad::{Button, PadState};

/// The controllers that are read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    DualSense,
    DualShock4,
}

const SONY: u32 = 0x054C;

/// Which controller a USB vendor and product number is, if it is one that is read.
pub fn kind_of(vendor: u32, product: u32) -> Option<Kind> {
    match (vendor, product) {
        // The DualSense and the DualSense Edge.
        (SONY, 0x0CE6 | 0x0DF2) => Some(Kind::DualSense),
        // The DualShock 4 in its two versions, and Sony's USB wireless adapter for it.
        (SONY, 0x05C4 | 0x09CC | 0x0BA0) => Some(Kind::DualShock4),
        _ => None,
    }
}

/// A controller made by the ViGEmBus driver and not a real one: ours, or another program's.
/// Reading our own would feed what we send back into what we send.
///
/// The driver gives its devices a revision in their name ("...&pid_05c4&rev_0100#..."), which
/// Windows never does for a real USB or Bluetooth device.
pub fn is_virtual(path: &str) -> bool {
    path.to_ascii_lowercase().contains("&rev_")
}

/// Whether a device path is a Bluetooth one: it carries the number of Bluetooth's HID service.
fn is_bluetooth(path: &str) -> bool {
    path.to_ascii_lowercase().contains("00001124-0000-1000-8000-00805f9b34fb")
}

/// What to call a controller in the window.
fn name_of(kind: Kind, path: &str) -> &'static str {
    match (kind, is_bluetooth(path)) {
        (Kind::DualSense, false) => "DualSense, by cable",
        (Kind::DualSense, true) => "DualSense, by Bluetooth",
        (Kind::DualShock4, false) => "DualShock 4, by cable",
        (Kind::DualShock4, true) => "DualShock 4, by Bluetooth",
    }
}

/// Reads one input report, as Windows hands it over (the report's number first), into a
/// controller state. `None` for anything that is not a report of the controller's state.
///
/// Both controllers send the same things in two orders. The DualSense's full report has the
/// sticks, the two triggers, a counter and then the buttons. The DualShock 4's report, and the
/// short one a DualSense sends over Bluetooth until something asks it for full ones, has the
/// sticks, the buttons and then the triggers. Over Bluetooth the full reports have another
/// number and start a byte or two further in.
pub fn parse(kind: Kind, report: &[u8]) -> Option<PadState> {
    // Where the sticks, the buttons and the triggers start.
    let (sticks, buttons, triggers) = match (kind, *report.first()?, report.len()) {
        (Kind::DualSense, 0x01, 64) => (1, 8, 5),
        (Kind::DualSense, 0x31, _) => (2, 9, 6),
        (Kind::DualSense, 0x01, _) | (Kind::DualShock4, 0x01, _) => (1, 5, 8),
        (Kind::DualShock4, 0x11, _) => (3, 7, 10),
        _ => return None,
    };
    if report.len() < sticks.max(buttons).max(triggers) + 4 {
        return None;
    }
    // A stick axis: 128 is the middle, and down is the larger number.
    let axis = |i: usize| ((report[i] as f32 - 128.0) / 127.0).clamp(-1.0, 1.0);
    let mut pad = PadState {
        lx: axis(sticks),
        ly: 0.0 - axis(sticks + 1),
        rx: axis(sticks + 2),
        ry: 0.0 - axis(sticks + 3),
        l2: report[triggers] as f32 / 255.0,
        r2: report[triggers + 1] as f32 / 255.0,
        buttons: 0,
    };
    let held = [report[buttons], report[buttons + 1], report[buttons + 2]];
    for (byte, bit, button) in [
        (0, 0x10, Button::Square),
        (0, 0x20, Button::Cross),
        (0, 0x40, Button::Circle),
        (0, 0x80, Button::Triangle),
        (1, 0x01, Button::L1),
        (1, 0x02, Button::R1),
        (1, 0x04, Button::L2),
        (1, 0x08, Button::R2),
        (1, 0x10, Button::Create),
        (1, 0x20, Button::Options),
        (1, 0x40, Button::L3),
        (1, 0x80, Button::R3),
        (2, 0x01, Button::Ps),
        (2, 0x02, Button::Touchpad),
    ] {
        if held[byte] & bit != 0 {
            pad.buttons |= button.bit();
        }
    }
    // The d-pad is one number: 0 is up, then clockwise, and 8 is nothing.
    let (up, right, down, left) = (Button::DpadUp.bit(), Button::DpadRight.bit(), Button::DpadDown.bit(), Button::DpadLeft.bit());
    pad.buttons |= match held[0] & 0x0F {
        0 => up,
        1 => up | right,
        2 => right,
        3 => right | down,
        4 => down,
        5 => down | left,
        6 => left,
        7 => left | up,
        _ => 0,
    };
    Some(pad)
}

/// How far a stick or trigger has to move, of its whole travel, to count as touched. A stick at
/// rest wobbles by a hundredth or so.
const TOUCHED_BY: f32 = 0.06;

/// Whether a controller has been touched between two readings: a button went down or up, or a
/// stick or trigger is somewhere else.
fn touched(before: &PadState, now: &PadState) -> bool {
    let moves = [now.lx - before.lx, now.ly - before.ly, now.rx - before.rx, now.ry - before.ry, now.l2 - before.l2, now.r2 - before.r2];
    before.buttons != now.buttons || moves.iter().any(|by| by.abs() > TOUCHED_BY)
}

/// How long after a press on the real controller the virtual one makes the same press.
const ECHO_AFTER: f64 = 0.024;

/// The real controller's buttons as they are passed on to the virtual one: each press a moment
/// late, each release at once. Sticks and triggers are not held back at all.
///
/// The wait is for Remote Play programs that read every controller and follow whichever pressed
/// a button last, as Veyra does. Such a program has both of ours: the real one and the virtual
/// one. A press on the real one makes it follow the real one, from which it gets that press
/// directly, so the wait costs nothing. When the virtual one makes the same press a moment
/// later, the program follows the virtual one again and stays there, and that is the one that
/// carries the keys, the mouse and the real controller's sticks. Made at the same moment, the
/// two presses would reach the program in one batch and in no promised order, and it could end
/// up on the real controller, deaf to the keyboard until a key that is a button is pressed.
///
/// A trigger is not a button to those programs, so L2 and R2 are passed on at once.
#[derive(Default)]
pub struct Echo {
    /// Per button: since when the real controller has held it.
    held_since: [Option<f64>; Button::ALL.len()],
}

impl Echo {
    /// Takes the buttons the real controller holds at `now` and gives the ones to pass on.
    pub fn pass(&mut self, buttons: u32, now: f64) -> u32 {
        let mut out = 0;
        for (button, since) in Button::ALL.into_iter().zip(&mut self.held_since) {
            if buttons & button.bit() == 0 {
                *since = None;
            } else if matches!(button, Button::L2 | Button::R2) || now - *since.get_or_insert(now) >= ECHO_AFTER {
                out |= button.bit();
            }
        }
        out
    }

    /// Whether a press is still waiting to be passed on at `now`.
    pub fn waiting(&self, now: f64) -> bool {
        self.held_since.iter().flatten().any(|since| now - since < ECHO_AFTER)
    }

    pub fn clear(&mut self) {
        *self = Echo::default();
    }
}

/// The real controller as the engine keeps it: what it last said, and what of that goes out.
#[derive(Default)]
pub struct Real {
    /// Its last reading, and when that came.
    reading: Option<(Reading, f64)>,
    /// How it was when it was last touched, to tell the next touch by, and when that was.
    as_touched: Option<PadState>,
    touched: Option<f64>,
    echo: Echo,
}

/// A controller that has said nothing for this long has stopped answering (out of range, or
/// its battery ran out mid-flight) and lets go of everything it held.
const SILENT_AFTER: f64 = 5.0 * SAY_AGAIN_EVERY;

impl Real {
    /// The reader thread has news: a reading, or `None` when the controller has gone.
    pub fn heard(&mut self, reading: Option<Reading>, now: f64) {
        match (reading, self.as_touched) {
            (Some(r), Some(before)) if touched(&before, &r.pad) => (self.as_touched, self.touched) = (Some(r.pad), Some(now)),
            // Just connected: however it lies there is its rest, until it is touched.
            (Some(r), None) => self.as_touched = Some(r.pad),
            (Some(_), Some(_)) => {}
            (None, _) => (self.as_touched, self.touched) = (None, None),
        }
        self.reading = reading.map(|r| (r, now));
    }

    /// Whether it is the real controller that is in the hands: it was touched more recently
    /// than a key was pressed or the mouse moved, which last happened at `keys_used`.
    pub fn in_hands(&self, keys_used: f64) -> bool {
        self.reading.is_some() && self.touched.is_some_and(|touched| touched > keys_used)
    }

    /// `pad`, which is what the keys and the mouse are doing, with the real controller added
    /// while it is in the hands. With `wanted` off, or once the controller has gone or fallen
    /// silent, or while it is not the one in the hands, that is `pad` alone.
    pub fn added_to(&mut self, pad: PadState, wanted: bool, keys_used: f64, now: f64) -> PadState {
        if self.reading.is_some_and(|(_, heard)| !wanted || now - heard > SILENT_AFTER) {
            self.heard(None, now);
        }
        match self.reading.filter(|_| self.in_hands(keys_used)) {
            Some((reading, _)) => pad.with(PadState { buttons: self.echo.pass(reading.pad.buttons, now), ..reading.pad }),
            None => {
                self.echo.clear();
                pad
            }
        }
    }

    /// Whether a press is waiting to be passed on, so that the engine must look again shortly.
    pub fn waiting(&self, now: f64) -> bool {
        self.reading.is_some() && self.echo.waiting(now)
    }

    /// What the controller that is connected is called.
    pub fn name(&self) -> Option<&'static str> {
        self.reading.map(|(reading, _)| reading.name)
    }
}

/// Whether a controller should be read at all (the setting in the window).
static WANTED: AtomicBool = AtomicBool::new(false);
/// Test aid (`--dry --virtual-controllers`): the look-only window reads a ViGEmBus controller as
/// if it were a real one, so the whole path from a controller to the window can be watched
/// without owning one. Only with `--dry`, which sends nothing: a copy that sends would be
/// reading what it sends.
pub static VIRTUAL_TOO: AtomicBool = AtomicBool::new(false);

pub fn set_wanted(wanted: bool) {
    WANTED.store(wanted, Relaxed);
}

/// A controller that is connected, and what it is doing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reading {
    pub name: &'static str,
    pub pad: PadState,
}

/// A state that has not changed is still sent this often, so the engine can tell a controller
/// at rest from one that has stopped answering.
pub const SAY_AGAIN_EVERY: f64 = 0.1;

/// Starts the thread that looks for a controller and reads it. What it reads arrives as
/// `Msg::Controller`: a reading whenever the state changes and every `SAY_AGAIN_EVERY` seconds
/// when it does not, and `None` when the controller goes away.
///
/// `virtual_too` is a test aid: ViGEmBus controllers count as well, so the reading can be tried
/// without a real controller.
pub fn start(tx: Sender<Msg>, virtual_too: bool) {
    let _ = std::thread::Builder::new().name("controller".into()).spawn(move || {
        loop {
            let found = if WANTED.load(Relaxed) { device::find(virtual_too) } else { None };
            let Some(found) = found else {
                std::thread::sleep(std::time::Duration::from_millis(1000));
                continue;
            };
            read(&found, &tx);
            if tx.send(Msg::Controller(None)).is_err() {
                return;
            }
            // Unplugged, switched off or no longer wanted: look again in a moment.
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
    });
}

/// Reads one controller until it goes away or is no longer wanted.
fn read(found: &device::Found, tx: &Sender<Msg>) {
    use std::io::Read;
    let Ok(mut file) = device::open(&found.path) else {
        return;
    };
    let name = name_of(found.kind, &found.path);
    let mut report = vec![0u8; found.report_bytes.max(16)];
    let (mut last, mut said) = (None::<PadState>, 0.0f64);
    while WANTED.load(Relaxed) {
        // Waits for the next report. A controller sends them all the time, used or not.
        let Ok(got) = file.read(&mut report) else {
            return;
        };
        let Some(pad) = parse(found.kind, &report[..got]) else {
            continue;
        };
        let now = crate::win::now();
        if last != Some(pad) || now - said >= SAY_AGAIN_EVERY {
            (last, said) = (Some(pad), now);
            if tx.send(Msg::Controller(Some(Reading { name, pad }))).is_err() {
                return;
            }
        }
    }
}

/// Finding and opening the controller's HID device.
mod device {
    use std::ffi::c_void;
    use std::mem::size_of;

    use windows::Win32::Devices::HumanInterfaceDevice::{HIDP_CAPS, HidP_GetCaps, PHIDP_PREPARSED_DATA};
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::UI::Input::{
        GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RID_DEVICE_INFO, RIDI_DEVICEINFO, RIDI_DEVICENAME, RIDI_PREPARSEDDATA, RIM_TYPEHID,
    };

    use super::Kind;

    pub struct Found {
        pub kind: Kind,
        pub path: String,
        /// How long its input reports are, as Windows hands them over.
        pub report_bytes: usize,
    }

    /// One piece of what Windows knows about a device, as bytes.
    unsafe fn info(device: HANDLE, what: windows::Win32::UI::Input::RAW_INPUT_DEVICE_INFO_COMMAND, unit: usize) -> Option<Vec<u8>> {
        unsafe {
            let mut size = 0u32;
            GetRawInputDeviceInfoW(Some(device), what, None, &mut size);
            if size == 0 {
                return None;
            }
            // (The size is in characters for the name and in bytes for everything else.)
            let mut data = vec![0u8; size as usize * unit];
            let got = GetRawInputDeviceInfoW(Some(device), what, Some(data.as_mut_ptr() as *mut c_void), &mut size);
            (got != u32::MAX && got != 0).then_some(data)
        }
    }

    /// The first controller connected that is one of the kinds read.
    pub fn find(virtual_too: bool) -> Option<Found> {
        unsafe {
            let entry = size_of::<RAWINPUTDEVICELIST>() as u32;
            let mut count = 0u32;
            GetRawInputDeviceList(None, &mut count, entry);
            let mut list = vec![RAWINPUTDEVICELIST::default(); count as usize];
            let got = GetRawInputDeviceList(Some(list.as_mut_ptr()), &mut count, entry);
            if got == u32::MAX {
                return None;
            }
            list.truncate(got as usize);
            for device in list.into_iter().filter(|d| d.dwType == RIM_TYPEHID) {
                let Some(about) = info(device.hDevice, RIDI_DEVICEINFO, 1) else {
                    continue;
                };
                if about.len() < size_of::<RID_DEVICE_INFO>() {
                    continue;
                }
                let about = std::ptr::read_unaligned(about.as_ptr() as *const RID_DEVICE_INFO);
                let hid = about.Anonymous.hid;
                // Usage page 1, usage 5: a game pad. (A controller can have other HID parts too.)
                let Some(kind) = super::kind_of(hid.dwVendorId, hid.dwProductId).filter(|_| hid.usUsagePage == 1 && hid.usUsage == 5) else {
                    continue;
                };
                let Some(name) = info(device.hDevice, RIDI_DEVICENAME, 2) else {
                    continue;
                };
                let wide: Vec<u16> = name.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).take_while(|&c| c != 0).collect();
                let path = String::from_utf16_lossy(&wide);
                if super::is_virtual(&path) && !virtual_too {
                    continue;
                }
                let Some(parsed) = info(device.hDevice, RIDI_PREPARSEDDATA, 1) else {
                    continue;
                };
                let mut caps = HIDP_CAPS::default();
                if HidP_GetCaps(PHIDP_PREPARSED_DATA(parsed.as_ptr() as isize), &mut caps).is_err() {
                    continue;
                }
                return Some(Found { kind, path, report_bytes: caps.InputReportByteLength as usize });
            }
            None
        }
    }

    /// Opens the device for reading, leaving it open to every other program as well.
    pub fn open(path: &str) -> std::io::Result<std::fs::File> {
        use std::os::windows::fs::OpenOptionsExt;
        const SHARE_READ_AND_WRITE: u32 = 3;
        std::fs::OpenOptions::new().read(true).share_mode(SHARE_READ_AND_WRITE).open(path)
    }
}

/// Test aid (`--controller-test [--virtual] [--seconds N]`): reads the controller for a while and
/// prints what it sees whenever that changes. Nothing is sent anywhere.
pub fn test(virtual_too: bool, seconds: f64) {
    match device::find(virtual_too) {
        Some(found) => println!("found {} ({:?}), reports of {} bytes, {}", name_of(found.kind, &found.path), found.kind, found.report_bytes, found.path),
        None => println!("no controller found"),
    }
    let (tx, rx) = std::sync::mpsc::channel();
    set_wanted(true);
    start(tx, virtual_too);
    let started = std::time::Instant::now();
    let (mut readings, mut last) = (0u32, None::<Option<Reading>>);
    while started.elapsed().as_secs_f64() < seconds {
        if let Ok(Msg::Controller(reading)) = rx.recv_timeout(std::time::Duration::from_millis(50)) {
            readings += reading.is_some() as u32;
            if last != Some(reading) {
                last = Some(reading);
                match reading {
                    Some(r) => {
                        let held: Vec<&str> = Button::ALL.into_iter().filter(|b| r.pad.pressed(*b)).map(|b| b.name()).collect();
                        println!("{}: left {:+.2} {:+.2} right {:+.2} {:+.2} l2 {:.2} r2 {:.2} buttons {held:?}", r.name, r.pad.lx, r.pad.ly, r.pad.rx, r.pad.ry, r.pad.l2, r.pad.r2);
                    }
                    None => println!("gone"),
                }
            }
        }
    }
    println!("{readings} readings in {seconds} seconds");
    println!("done");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::to_ds4;

    /// A report of `len` bytes with `bytes` at `at`.
    fn report(len: usize, id: u8, at: usize, bytes: &[u8]) -> Vec<u8> {
        let mut r = vec![0u8; len];
        r[0] = id;
        r[at..at + bytes.len()].copy_from_slice(bytes);
        r
    }

    #[test]
    fn a_dualsense_report_is_read_by_cable_and_by_bluetooth() {
        // Left stick right and up, right stick left and down, L2 a fifth, R2 all the way,
        // d-pad down-left with cross and triangle, R1 with R2 and options, the PS button.
        let state = [255, 0, 0, 255, 51, 255, 7, 0x05 | 0x20 | 0x80, 0x02 | 0x08 | 0x20, 0x01];
        let by_cable = parse(Kind::DualSense, &report(64, 0x01, 1, &state)).unwrap();
        let by_bluetooth = parse(Kind::DualSense, &report(78, 0x31, 2, &state)).unwrap();
        assert_eq!(by_cable, by_bluetooth);
        let p = by_cable;
        assert_eq!((p.lx, p.ly, p.rx, p.ry), (1.0, 1.0, -1.0, -1.0));
        assert_eq!((p.l2, p.r2), (0.2, 1.0));
        let held: Vec<Button> = Button::ALL.into_iter().filter(|b| p.pressed(*b)).collect();
        assert_eq!(held, [Button::Cross, Button::Triangle, Button::R1, Button::R2, Button::Options, Button::Ps, Button::DpadDown, Button::DpadLeft]);
        // The short report it sends over Bluetooth at first: sticks, buttons, triggers.
        let short = [255, 0, 0, 255, 0x05 | 0x20 | 0x80, 0x02 | 0x08 | 0x20, 0x01, 51, 255];
        assert_eq!(parse(Kind::DualSense, &report(78, 0x01, 1, &short)), Some(p));
    }

    #[test]
    fn a_dualshock_4_report_is_read_by_cable_and_by_bluetooth() {
        let state = [128, 128, 200, 128, 0x08 | 0x10, 0x01 | 0x40, 0x02, 0, 128];
        let by_cable = parse(Kind::DualShock4, &report(64, 0x01, 1, &state)).unwrap();
        assert_eq!(parse(Kind::DualShock4, &report(547, 0x11, 3, &state)), Some(by_cable));
        let p = by_cable;
        assert_eq!((p.lx, p.ly, p.ry), (0.0, 0.0, 0.0));
        assert!((p.rx - 72.0 / 127.0).abs() < 1e-6 && (p.r2 - 128.0 / 255.0).abs() < 1e-6 && p.l2 == 0.0);
        let held: Vec<Button> = Button::ALL.into_iter().filter(|b| p.pressed(*b)).collect();
        assert_eq!(held, [Button::Square, Button::L1, Button::L3, Button::Touchpad]);
    }

    #[test]
    fn what_is_not_a_state_report_is_left_alone() {
        assert_eq!(parse(Kind::DualSense, &[]), None);
        assert_eq!(parse(Kind::DualSense, &report(64, 0x05, 1, &[1, 2, 3])), None, "another report number");
        assert_eq!(parse(Kind::DualShock4, &[0x01, 128, 128]), None, "cut short");
        assert_eq!(parse(Kind::DualShock4, &report(64, 0x31, 1, &[0])), None);
    }

    #[test]
    fn what_is_read_goes_out_again_unchanged() {
        // Every stick position and trigger pull of a real controller comes out of the virtual
        // one as the same number (but for the very end of a stick, which the virtual one does
        // not have: 0 becomes 1).
        for value in 0..=255u8 {
            let p = parse(Kind::DualSense, &report(64, 0x01, 1, &[value, value, value, value, value, value, 0, 8, 0, 0])).unwrap();
            let out = to_ds4(&p);
            assert_eq!((out.thumb_lx, out.thumb_ly, out.thumb_rx, out.thumb_ry), [value.max(1); 4].into(), "stick at {value}");
            assert_eq!((out.trigger_l, out.trigger_r), (value, value), "trigger at {value}");
        }
        // And every button as itself.
        for (byte, bit) in (0..3).flat_map(|byte| (0..8).map(move |bit| (byte, 1u8 << bit))).filter(|&(byte, bit)| !(byte == 0 && bit < 0x10) && !(byte == 2 && bit > 0x02)) {
            let mut state = [128, 128, 128, 128, 8, 0, 0, 0, 0];
            state[4 + byte] |= bit;
            let p = parse(Kind::DualShock4, &report(64, 0x01, 1, &state)).unwrap();
            let out = to_ds4(&p);
            let sent = [(out.buttons & 0xFF) as u8, (out.buttons >> 8) as u8, out.special];
            assert_eq!(sent[byte] & bit, bit, "byte {byte} bit {bit:#04x}");
            assert_eq!(p.buttons.count_ones(), 1);
        }
        for hat in 0..=8u8 {
            let p = parse(Kind::DualShock4, &report(64, 0x01, 1, &[128, 128, 128, 128, hat, 0, 0, 0, 0])).unwrap();
            assert_eq!((to_ds4(&p).buttons & 0xF) as u8, hat, "d-pad {hat}");
        }
    }

    #[test]
    fn only_sony_controllers_are_read_and_never_a_virtual_one() {
        assert_eq!(kind_of(0x054C, 0x0CE6), Some(Kind::DualSense));
        assert_eq!(kind_of(0x054C, 0x0DF2), Some(Kind::DualSense));
        assert_eq!(kind_of(0x054C, 0x09CC), Some(Kind::DualShock4));
        assert_eq!(kind_of(0x045E, 0x028E), None, "an Xbox controller");
        // The name ViGEmBus gave our own virtual DualShock 4 on the computer this was written on.
        assert!(is_virtual(r"\\?\hid#vid_054c&pid_05c4&rev_0100#2&18dd2c13&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}"));
        // The form Windows gives a real one, by cable and by Bluetooth.
        assert!(!is_virtual(r"\\?\HID#VID_054C&PID_0CE6&MI_03#8&2b8b8d5b&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}"));
        let bluetooth = r"\\?\HID#{00001124-0000-1000-8000-00805f9b34fb}_VID&0002054c_PID&0ce6#9&1a2b3c4d&0&0000#{4d1e55b2-f16f-11cf-88cb-001111000030}";
        assert!(!is_virtual(bluetooth));
        assert_eq!(name_of(Kind::DualSense, bluetooth), "DualSense, by Bluetooth");
    }

    #[test]
    fn the_real_controller_counts_from_a_touch_until_the_next_key() {
        let reading = |pad: PadState| Some(Reading { name: "DualSense, by cable", pad });
        let keys = PadState { ly: 1.0, buttons: Button::L3.bit(), ..PadState::default() };
        let mut real = Real::default();
        assert_eq!(real.added_to(keys, true, 0.0, 0.0), keys, "no controller: the keys alone");
        assert_eq!((real.name(), real.in_hands(0.0)), (None, false));
        // Plugged in while it lies on the desk on its right trigger, a stick a little off its
        // middle and wobbling: connected, and nothing of it goes out.
        let lying = PadState { rx: 0.05, r2: 0.3, buttons: Button::R2.bit(), ..PadState::default() };
        real.heard(reading(lying), 1.0);
        real.heard(reading(PadState { rx: 0.07, r2: 0.31, ..lying }), 1.2);
        assert_eq!(real.name(), Some("DualSense, by cable"));
        assert!(!real.in_hands(0.5));
        assert_eq!(real.added_to(keys, true, 0.5, 1.2), keys);
        // Picked up: right stick over, R2 half way, cross pressed. It counts at once.
        let flying = PadState { rx: 0.8, ry: -0.3, r2: 0.5, buttons: Button::Cross.bit() | Button::R2.bit(), ..PadState::default() };
        real.heard(reading(flying), 2.0);
        assert!(real.in_hands(0.5));
        let out = real.added_to(keys, true, 0.5, 2.0);
        assert_eq!((out.ly, out.rx, out.ry, out.r2), (1.0, 0.8, -0.3, 0.5), "sticks and triggers at once, next to the keys");
        assert!(out.pressed(Button::L3) && out.pressed(Button::R2) && !out.pressed(Button::Cross), "the button a moment later");
        assert!(real.waiting(2.0));
        let out = real.added_to(keys, true, 0.5, 2.0 + ECHO_AFTER);
        assert!(out.pressed(Button::Cross) && !real.waiting(2.0 + ECHO_AFTER));
        // Held just like that for a long time, saying so now and then: it still counts.
        real.heard(reading(flying), 9.0);
        assert!(real.added_to(keys, true, 0.5, 9.1).pressed(Button::Cross));
        // Put down with the trigger still half pulled by its weight, and a key pressed (at 10):
        // the keys alone again, and the trigger pulls nothing.
        real.heard(reading(PadState { r2: 0.5, buttons: Button::R2.bit(), ..PadState::default() }), 9.5);
        real.heard(reading(PadState { r2: 0.5, buttons: Button::R2.bit(), ..PadState::default() }), 10.2);
        assert!(!real.in_hands(10.0));
        assert_eq!(real.added_to(keys, true, 10.0, 10.2), keys);
        // Touched again: back at once.
        real.heard(reading(PadState { lx: -1.0, ..PadState::default() }), 11.0);
        assert_eq!(real.added_to(PadState::default(), true, 10.0, 11.0).lx, -1.0);
    }

    #[test]
    fn the_real_controller_lets_go_when_it_falls_silent_is_unplugged_or_is_not_wanted() {
        let held = Some(Reading { name: "DualShock 4, by Bluetooth", pad: PadState { ly: 1.0, r2: 1.0, buttons: Button::R2.bit(), ..PadState::default() } });
        let none = PadState::default();
        let mut real = Real::default();
        let mut pick_up = |real: &mut Real, at: f64| {
            real.heard(Some(Reading { pad: none, ..held.unwrap() }), at);
            real.heard(held, at + 0.1);
            assert_eq!(real.added_to(none, true, 0.0, at + 0.1).ly, 1.0);
        };
        pick_up(&mut real, 1.0);
        // Out of range with the stick forward: nothing heard for too long, and it lets go.
        assert_eq!(real.added_to(none, true, 0.0, 1.1 + SILENT_AFTER - 0.01).ly, 1.0);
        assert_eq!(real.added_to(none, true, 0.0, 1.1 + SILENT_AFTER + 0.01), none);
        assert_eq!(real.name(), None);
        // Back, as it comes back: still held the same way. That is no touch, so it stays out
        // until something on it moves.
        real.heard(held, 3.0);
        assert_eq!(real.added_to(none, true, 0.0, 3.0), none);
        real.heard(Some(Reading { pad: none, ..held.unwrap() }), 3.2);
        assert!(real.in_hands(0.0));
        // Unplugged.
        pick_up(&mut real, 5.0);
        real.heard(None, 5.2);
        assert_eq!(real.added_to(none, true, 0.0, 5.2), none);
        // Turned off in the settings.
        pick_up(&mut real, 7.0);
        assert_eq!(real.added_to(none, false, 0.0, 7.2), none);
        assert_eq!(real.name(), None);
    }

    #[test]
    fn a_press_is_passed_on_a_moment_late_and_a_release_at_once() {
        let (cross, l2) = (Button::Cross.bit(), Button::L2.bit());
        let mut echo = Echo::default();
        assert_eq!(echo.pass(cross | l2, 1.000), l2, "a trigger is passed on at once, a button is not");
        assert!(echo.waiting(1.010));
        assert_eq!(echo.pass(cross | l2, 1.010), l2);
        assert_eq!(echo.pass(cross | l2, 1.000 + ECHO_AFTER), cross | l2);
        assert!(!echo.waiting(1.000 + ECHO_AFTER));
        assert_eq!(echo.pass(l2, 1.100), l2, "let go: gone at once");
        assert_eq!(echo.pass(cross, 1.200), 0, "pressed again: late again");
        // A press shorter than the wait is never passed on. (The program following the real
        // controller has had it from there.)
        assert_eq!(echo.pass(0, 1.210), 0);
        assert_eq!(echo.pass(0, 1.300), 0);
        assert!(!echo.waiting(1.300));
    }
}
