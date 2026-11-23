// No console window behind the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod aim;
mod config;
mod engine;
mod hook;
mod keys;
mod mapper;
mod output;
mod pad;
mod ui;
mod win;

use eframe::egui;

const TITLE: &str = "PS5 Remote Sim";

fn main() {
    if std::env::args().any(|a| a == "--pad-test") {
        return pad_test();
    }
    if std::env::args().any(|a| a == "--mouse-test") {
        return mouse_test();
    }
    let path = config::config_path();
    let (cfg, load_error) = config::load(&path);
    let engine = match engine::Engine::start(cfg.clone()) {
        Ok(engine) => engine,
        Err(error) => return fatal(&format!("Could not start reading the keyboard and mouse: {error}")),
    };
    let mut viewport = egui::ViewportBuilder::default().with_title(TITLE).with_inner_size([1280.0, 860.0]).with_min_inner_size([820.0, 520.0]);
    // Test aids: `--no-focus` opens the window without taking the keyboard, `--at x,y` places it,
    // `--layout drive` starts on the driving layout, `--select F` opens with that key's editor showing.
    let args: Vec<String> = std::env::args().collect();
    let after = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    if after("--layout").as_deref() == Some("drive") {
        engine.send(engine::Msg::SetMode(config::Mode::Drive));
    }
    let select = after("--select").and_then(|name| keys::Input::from_name(&name));
    if args.iter().any(|a| a == "--no-focus") {
        viewport = viewport.with_active(false);
    }
    if let Some(at) = args.iter().position(|a| a == "--at").and_then(|i| args.get(i + 1)) {
        let mut xy = at.split(',').filter_map(|v| v.trim().parse::<f32>().ok());
        if let (Some(x), Some(y)) = (xy.next(), xy.next()) {
            viewport = viewport.with_position([x, y]);
        }
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    let created = Box::new(move |cc: &eframe::CreationContext<'_>| -> Result<Box<dyn eframe::App>, Box<dyn std::error::Error + Send + Sync>> {
        let mut app = ui::App::new(cc, engine, cfg, path, load_error);
        app.select(select);
        Ok(Box::new(app))
    });
    if let Err(error) = eframe::run_native(TITLE, options, created) {
        fatal(&format!("Could not open the window: {error}"));
    }
    // Test aid: how many mouse movement reports reached us while the window was open.
    if args.iter().any(|a| a == "--count-mouse") {
        println!("mouse reports seen: {}", win::MOUSE_REPORTS.load(std::sync::atomic::Ordering::Relaxed));
    }
}

/// A message box, because a release build has no console to print to.
fn fatal(text: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};
    use windows::core::HSTRING;
    eprintln!("{text}");
    unsafe {
        MessageBoxW(None, &HSTRING::from(text), &HSTRING::from(TITLE), MB_OK | MB_ICONERROR);
    }
}

/// `--mouse-test`: for 4 seconds, counts the mouse movement reports that arrive. Nothing is taken
/// from Windows and no controller is plugged in.
fn mouse_test() {
    use std::sync::atomic::Ordering::Relaxed;
    let (tx, rx) = std::sync::mpsc::channel();
    if let Err(e) = win::start_input(tx) {
        return println!("FAILED {e}");
    }
    win::ALWAYS_REPORT_MOUSE.store(true, Relaxed);
    println!("listening");
    let start = std::time::Instant::now();
    let (mut reports, mut counts, mut other) = (0u32, 0i64, 0u32);
    while start.elapsed() < std::time::Duration::from_secs(4) {
        match rx.recv_timeout(std::time::Duration::from_millis(50)) {
            Ok(engine::Msg::Mouse { dx, dy, .. }) => {
                reports += 1;
                counts += (dx.abs() + dy.abs()) as i64;
            }
            Ok(_) => other += 1,
            Err(_) => {}
        }
    }
    println!("mouse reports {reports}, counts {counts}, other messages {other}");
}

/// `--pad-test`: plugs in the virtual controller and steps through every control, printing each
/// step, so another program can check that what arrives is what was sent. No keyboard or mouse is read.
fn pad_test() {
    use output::Output;
    use pad::{Button, PadState};
    use std::time::Duration;

    let mut pad_out = match output::VirtualDs4::connect() {
        Ok(p) => p,
        Err(e) => return println!("FAILED {e}"),
    };
    let hold = Duration::from_millis(300);
    let mut step = |name: &str, state: PadState| {
        println!("{name}");
        let started = std::time::Instant::now();
        loop {
            match pad_out.send(&state) {
                Ok(output::Sent::Taken) => break,
                Ok(output::Sent::Again) if started.elapsed() < Duration::from_secs(2) => std::thread::sleep(Duration::from_millis(1)),
                Ok(output::Sent::Again) => break println!("FAILED nobody is reading the controller"),
                Err(e) => break println!("FAILED {e}"),
            }
        }
        std::thread::sleep(hold);
    };
    std::thread::sleep(Duration::from_millis(1500)); // let readers find the new controller
    step("neutral", PadState::default());
    for b in Button::ALL {
        let mut s = PadState::default();
        s.press(b);
        step(b.name(), s);
    }
    step("left_stick_right", PadState { lx: 1.0, ..PadState::default() });
    step("left_stick_up", PadState { ly: 1.0, ..PadState::default() });
    step("right_stick_left", PadState { rx: -1.0, ..PadState::default() });
    step("right_stick_down", PadState { ry: -1.0, ..PadState::default() });
    step("right_stick_half_right", PadState { rx: 0.5, ..PadState::default() });
    step("neutral", PadState::default());
    println!("done");
}
