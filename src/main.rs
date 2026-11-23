// No console window behind the app in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod aim;
mod config;
mod engine;
mod hook;
mod keys;
mod mapper;
mod output;
mod overlay;
mod pad;
mod prompts;
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
    // Test aid: `--overlay-test <window handle>` shows the label over that window for a few
    // seconds, switching between the two layouts, with nothing else running.
    if let Some(i) = std::env::args().position(|a| a == "--overlay-test") {
        let hwnd = std::env::args().nth(i + 1).and_then(|v| v.parse::<isize>().ok()).unwrap_or(0);
        let state = std::sync::Arc::new(std::sync::Mutex::new(overlay::OverlayState { show: true, game_window: hwnd, mode: config::Mode::Foot, pull: Vec::new(), prompts: Vec::new(), spots: Vec::new() }));
        if let Err(e) = overlay::start(state.clone()) {
            return println!("FAILED {e}");
        }
        for step in 0..10 {
            std::thread::sleep(std::time::Duration::from_millis(900));
            if let Ok(mut s) = state.lock() {
                use keys::sign;
                use prompts::Glyph::*;
                s.mode = if step % 2 == 0 { config::Mode::Drive } else { config::Mode::Foot };
                // Second half of the test: with prompt hints, a different set each step.
                s.prompts = match step {
                    2 => vec![(Triangle, "E".to_string())],
                    3 => vec![(Triangle, "F".to_string()), (Circle, format!("Ctrl / {}", sign::BACKSPACE)), (Cross, format!("{} / {}", sign::SPACE, sign::ENTER)), (Square, "R".to_string())],
                    4 => vec![(LeftStick, "WASD".to_string()), (RightStick, "Mouse".to_string()), (L2, sign::MOUSE_RIGHT.to_string()), (R2, sign::MOUSE_LEFT.to_string())],
                    5 => vec![(L1, format!("{} / {}4", sign::TAB, sign::MOUSE)), (R3, format!("C / {}", sign::MOUSE_MIDDLE)), (L3, format!("{} / {}", sign::SHIFT, sign::CAPS_LOCK))],
                    6 => vec![(Square, format!("{}{}{}{}", sign::UP, sign::LEFT, sign::DOWN, sign::RIGHT)), (R1, format!("{}{}", sign::MOUSE_MIDDLE, sign::UP))],
                    7 => vec![(Cross, sign::SPACE.to_string())],
                    _ => Vec::new(),
                };
                // The last steps: triggers set to a part pull, alone and beside a prompt.
                s.pull = match step {
                    7 => vec![(L2, "40%".to_string()), (R2, "70%".to_string())],
                    8 => vec![(R2, "30%".to_string())],
                    _ => Vec::new(),
                };
            }
        }
        return println!("done");
    }
    // Test aid: `--prompt-test <window handle>` runs the real chain on that window for a few
    // seconds: copy its picture, find the prompts, steady them, look up the keys, show the label.
    if let Some(i) = std::env::args().position(|a| a == "--prompt-test") {
        let hwnd = std::env::args().nth(i + 1).and_then(|v| v.parse::<isize>().ok()).unwrap_or(0);
        let cfg = config::Config::default();
        let state = std::sync::Arc::new(std::sync::Mutex::new(overlay::OverlayState { show: true, game_window: hwnd, ..Default::default() }));
        if let Err(e) = overlay::start(state.clone()) {
            return println!("FAILED {e}");
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let looker = win::start_prompt_watcher(tx);
        // `--checks <n>`: looks a second, as the setting in the window.
        let checks = std::env::args().position(|a| a == "--checks").and_then(|i| std::env::args().nth(i + 1)).and_then(|v| v.parse::<f64>().ok()).unwrap_or(4.0);
        let stays = engine::hint_stays(1.0 / checks);
        let (mut steady, mut tracks) = (prompts::Steady::default(), prompts::Tracks::default());
        let (mut looks, mut took) = (0u32, 0.0f32);
        let mut last_line = String::new();
        let started = std::time::Instant::now();
        let mut next = 0.0f64;
        while started.elapsed() < std::time::Duration::from_secs(4) {
            if started.elapsed().as_secs_f64() >= next {
                next += 1.0 / checks;
                let _ = looker.try_send(hwnd);
            }
            if let Ok(engine::Msg::Look(Some(look))) = rx.recv_timeout(std::time::Duration::from_millis(2)) {
                let now = started.elapsed().as_secs_f64();
                let found: Vec<prompts::Glyph> = look.prompts.iter().map(|f| f.glyph).collect();
                steady.look(&found);
                tracks.look(&look.prompts, now, stays);
                looks += 1;
                took += look.took;
                // Every prompt that is followed gets its hint drawn on it, as in play.
                let spots: Vec<overlay::Spot> = tracks
                    .there(now, stays)
                    .iter()
                    .filter_map(|f| {
                        let key = engine::key_on_prompt(&cfg, config::Mode::Foot, f.glyph)?;
                        Some(overlay::Spot { glyph: f.glyph, key, x: f.x.round() as i32, y: f.y.round() as i32, diameter: f.diameter.round() as i32 })
                    })
                    .take(overlay::SPOTS)
                    .collect();
                // (Said only when it changes: whoever reads this may not read it until the end.)
                let line = format!("found {found:?} name card {} -> on-prompt hints {:?}", look.name_card, spots.iter().map(|s| (s.key.as_str(), s.x, s.y)).collect::<Vec<_>>());
                if line != last_line {
                    println!("{line}");
                    last_line = line;
                }
                if let Ok(mut s) = state.lock() {
                    s.spots = spots;
                }
            }
        }
        println!("{looks} looks in 4 seconds ({checks} a second asked for), {:.1} ms each", took / looks.max(1) as f32 * 1000.0);
        return println!("done");
    }
    // Test aid: `--save-picture <window handle> <file>` saves that window's picture once.
    let argv: Vec<String> = std::env::args().collect();
    if let Some(i) = argv.iter().position(|a| a == "--save-picture") {
        let hwnd = argv.get(i + 1).and_then(|v| v.parse::<isize>().ok()).unwrap_or(0);
        let path = std::path::PathBuf::from(argv.get(i + 2).cloned().unwrap_or_default());
        return match win::save_window_picture(hwnd, &path) {
            Ok(path) => println!("saved {path}"),
            Err(why) => println!("FAILED {why}"),
        };
    }
    // Test aid: `--dry` opens the window only to be looked at (see `win::DRY`).
    if std::env::args().any(|a| a == "--dry") {
        win::DRY.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    let path = config::config_path();
    let (cfg, load_error) = config::load(&path);
    let engine = match engine::Engine::start(cfg.clone()) {
        Ok(engine) => engine,
        Err(error) => return fatal(&format!("Could not start reading the keyboard and mouse: {error}")),
    };
    let mut viewport = egui::ViewportBuilder::default().with_title(TITLE).with_inner_size([1280.0, 860.0]).with_min_inner_size([820.0, 520.0]);
    // Test aids: `--no-focus` opens the window without taking the keyboard, `--at x,y` places it,
    // `--layout drive` starts on the driving layout, `--select F` opens with that key's editor showing,
    // `--group triggers` with that group of settings showing.
    let args: Vec<String> = std::env::args().collect();
    let after = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    if after("--layout").as_deref() == Some("drive") {
        engine.send(engine::Msg::SetMode(config::Mode::Drive));
    }
    let select = after("--select").and_then(|name| keys::Input::from_name(&name));
    let group = after("--group");
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
        if let Some(group) = &group {
            app.show_group(group);
        }
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
