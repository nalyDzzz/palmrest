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

const TITLE: &str = "Palmrest";

fn main() {
    if std::env::args().any(|a| a == "--pad-test") {
        return pad_test();
    }
    if std::env::args().any(|a| a == "--mouse-test") {
        return mouse_test();
    }
    // Test aid: `--latency-test <program> [--sensitivity <n>]`, see `latency_test`.
    if let Some(i) = std::env::args().position(|a| a == "--latency-test") {
        let program = std::env::args().nth(i + 1).unwrap_or_default();
        let sensitivity = std::env::args().position(|a| a == "--sensitivity").and_then(|i| std::env::args().nth(i + 1)).and_then(|v| v.parse().ok());
        return latency_test(&program, sensitivity.unwrap_or(config::AimConfig::default().sensitivity));
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
    // With `--layout drive` the driving keys are in use.
    if let Some(i) = std::env::args().position(|a| a == "--prompt-test") {
        let hwnd = std::env::args().nth(i + 1).and_then(|v| v.parse::<isize>().ok()).unwrap_or(0);
        let cfg = config::Config::default();
        let drive = std::env::args().collect::<Vec<_>>().windows(2).any(|w| w[0] == "--layout" && w[1] == "drive");
        let mode = if drive { config::Mode::Drive } else { config::Mode::Foot };
        let state = std::sync::Arc::new(std::sync::Mutex::new(overlay::OverlayState { show: true, game_window: hwnd, mode, ..Default::default() }));
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
                        let key = engine::key_on_prompt(&cfg, mode, f.glyph)?;
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
    if let Some(icon) = window_icon() {
        viewport = viewport.with_icon(icon);
    }
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

/// The icon for the window's corner and the task bar, from the picture built into the program.
fn window_icon() -> Option<egui::IconData> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(&include_bytes!("../assets/palmrest-256.png")[..]));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut pixels).ok()?;
    let samples = info.color_type.samples();
    let rgba = pixels[..info.width as usize * info.height as usize * samples].chunks(samples).flat_map(|p| [p[0], p[1], p[2], if samples == 4 { p[3] } else { 255 }]).collect();
    Some(egui::IconData { rgba, width: info.width, height: info.height })
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

/// `--latency-test <program>`: the real engine on the default settings (no overlay, no sounds,
/// the settings file is not read), taking over while `<program>` is in front, and taking key
/// presses and mouse buttons made by a program as well, so `tools/latency_bench.py` can make them
/// and time what reaches the controller. Says `live yes` / `live no` as the takeover starts and
/// stops. Runs until a line or the end of its input arrives, then prints, for every report a key
/// or button caused, `timing <event reached the hook> <driver took the report>`, and for every
/// mouse movement report `mouse <reached the app> <dx> <dy>`, times in seconds of the
/// high-resolution counter, which the script reads too.
fn latency_test(program: &str, sensitivity: f32) {
    use std::sync::atomic::Ordering::Relaxed;
    let mut cfg = config::Config::default();
    cfg.targets = vec![program.trim().to_ascii_lowercase()];
    cfg.sounds = false;
    cfg.overlay.enabled = false;
    cfg.aim.sensitivity = sensitivity;
    win::ACCEPT_INJECTED.store(true, Relaxed);
    if let Ok(mut timings) = engine::TIMINGS.lock() {
        *timings = Some(Vec::new());
    }
    if let Ok(mut log) = engine::MOUSE_LOG.lock() {
        *log = Some(Vec::new());
    }
    let mut engine = match engine::Engine::start(cfg) {
        Ok(engine) => engine,
        Err(e) => return println!("FAILED {e}"),
    };
    engine.send(engine::Msg::Arm(true));
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    std::thread::spawn(move || {
        let _ = std::io::stdin().read_line(&mut String::new());
        let _ = stop_tx.send(());
    });
    println!("ready sensitivity {sensitivity}");
    let (mut live, mut error) = (false, None::<String>);
    while let Err(std::sync::mpsc::RecvTimeoutError::Timeout) = stop_rx.recv_timeout(std::time::Duration::from_millis(5)) {
        let status = engine.status();
        if status.live != live {
            live = status.live;
            println!("live {}", if live { "yes" } else { "no" });
        }
        if status.output_error != error {
            error = status.output_error;
            if let Some(e) = &error {
                println!("FAILED {e}");
            }
        }
    }
    engine.stop();
    let start = win::clock_start();
    for (event, sent) in engine::TIMINGS.lock().ok().and_then(|mut t| t.take()).unwrap_or_default() {
        println!("timing {:.7} {:.7}", start + event, start + sent);
    }
    for (t, dx, dy) in engine::MOUSE_LOG.lock().ok().and_then(|mut m| m.take()).unwrap_or_default() {
        println!("mouse {:.7} {dx} {dy}", start + t);
    }
    println!("done");
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
