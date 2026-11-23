//! The window: status, aim settings, a live view of the controller, and the key layouts.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, FontId, Pos2, RichText, Sense, Stroke, Vec2};

use crate::config::{self, Action, Bindings, Config, Mode};
use crate::engine::{Engine, Msg, Status};
use crate::keys::Input;
use crate::pad::{Button, PadState};

const GREEN: Color32 = Color32::from_rgb(61, 220, 132);
const ORANGE: Color32 = Color32::from_rgb(255, 150, 60);
const RED: Color32 = Color32::from_rgb(255, 100, 100);
const GREY: Color32 = Color32::from_rgb(140, 140, 140);
const DIM: Color32 = Color32::from_rgb(70, 70, 78);
/// Changes are written to the file once the sliders have been left alone this long.
const SAVE_AFTER: Duration = Duration::from_millis(600);

pub struct App {
    engine: Engine,
    cfg: Config,
    path: PathBuf,
    load_error: Option<String>,
    save_error: Option<String>,
    unsaved_since: Option<Instant>,
    test_tilt: f32,
}

impl App {
    pub fn new(engine: Engine, cfg: Config, path: PathBuf, load_error: Option<String>) -> App {
        let test_tilt = cfg.aim.deadzone;
        App { engine, cfg, path, load_error, save_error: None, unsaved_since: None, test_tilt }
    }

    /// A setting changed: the engine uses it at once, the file follows shortly.
    fn changed(&mut self) {
        self.cfg.sanitize();
        self.engine.send(Msg::SetConfig(Box::new(self.cfg.clone())));
        self.unsaved_since = Some(Instant::now());
    }

    fn save_now(&mut self) {
        self.unsaved_since = None;
        // If the file on disk could not be read, keep a copy of it before writing over it.
        self.save_error = config::save(&self.path, &self.cfg, self.load_error.is_some()).err();
        if self.save_error.is_none() {
            self.load_error = None;
        }
    }

    fn reload(&mut self) {
        let (cfg, error) = config::load(&self.path);
        self.load_error = error;
        if self.load_error.is_none() {
            self.cfg = cfg;
            self.engine.send(Msg::SetConfig(Box::new(self.cfg.clone())));
            self.unsaved_since = None;
        }
    }

    fn header(&mut self, ui: &mut egui::Ui, status: &Status) {
        let key = &self.cfg.toggle_key;
        let (word, color, hint) = if status.live {
            ("PLAYING", GREEN, format!("Your keyboard and mouse are the controller. {key} or Alt+Tab gives them back."))
        } else if status.armed {
            ("READY", ORANGE, "Click on the game window to start.".to_string())
        } else {
            ("OFF", GREY, format!("Press {key} in the game window, or the button here, to turn it on."))
        };
        ui.horizontal(|ui| {
            ui.label(RichText::new(word).size(30.0).strong().color(color));
            ui.add_space(18.0);
            let label = if status.armed { "Turn off" } else { "Turn on" };
            if ui.button(label).clicked() {
                self.engine.send(Msg::Arm(!status.armed));
            }
            ui.add_space(18.0);
            ui.label("Layout:");
            for mode in [Mode::Foot, Mode::Drive] {
                if ui.selectable_label(status.mode == mode, mode.label()).clicked() {
                    self.engine.send(Msg::SetMode(mode));
                }
            }
        });
        ui.label(hint);
    }

    fn problems(&mut self, ui: &mut egui::Ui, status: &Status) {
        if let Some(error) = &status.output_error {
            ui.add_space(6.0);
            ui.colored_label(RED, format!("No controller is being sent. {error}"));
            ui.horizontal_wrapped(|ui| {
                ui.label("The virtual controller needs the ViGEmBus driver:");
                ui.hyperlink("https://github.com/nefarius/ViGEmBus/releases");
                if ui.button("Try again").clicked() {
                    self.engine.send(Msg::ReconnectOutput);
                }
            });
        }
        for text in self.load_error.iter().chain(self.save_error.iter()) {
            ui.colored_label(RED, text);
        }
        for warning in &status.warnings {
            ui.colored_label(ORANGE, format!("Settings file: {warning}"));
        }
    }

    fn aim_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("Aim");
        let mut changed = false;
        let a = &mut self.cfg.aim;
        let slider = |ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, name: &str, help: &str| {
            let moved = ui.add(egui::Slider::new(value, range).text(name)).on_hover_text(help).changed();
            ui.label(RichText::new(help).small().color(GREY));
            ui.add_space(4.0);
            moved
        };
        changed |= slider(ui, &mut a.sensitivity, 0.2..=12.0, "Mouse speed", "How far the camera turns for a mouse movement.");
        changed |= slider(ui, &mut a.vertical, 0.3..=2.0, "Up/down speed", "Vertical speed compared with sideways.");
        changed |= slider(ui, &mut a.aim_in, 0.2..=2.0, "Speed while aiming in", "Mouse speed while the aim button (L2) is held, compared with normal.");
        changed |= slider(ui, &mut a.deadzone, 0.0..=0.6, "Dead zone", "The part of the stick the game ignores. Too high: tiny movements jump. Too low: they do nothing.");
        changed |= slider(ui, &mut a.curve, 1.0..=3.0, "Response curve", "Undoes the game's stick curve. 1 is off. Higher makes slow movements turn more.");
        changed |= slider(ui, &mut a.smoothing, 0.0..=2.0, "Smoothing", "1 is normal. Lower reacts faster, higher is steadier.");
        changed |= ui.checkbox(&mut a.invert_y, "Invert up/down").changed();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Game runs at");
            for fps in [30.0, 60.0] {
                if ui.selectable_label(self.cfg.game_fps == fps, format!("{fps} fps")).clicked() {
                    self.cfg.game_fps = fps;
                    changed = true;
                }
            }
        });
        changed |= ui.checkbox(&mut self.cfg.steering.smooth, "Ease steering keys while driving").changed();
        changed |= ui.checkbox(&mut self.cfg.sounds, "Beep when switching").changed();
        if changed {
            self.changed();
        }
    }

    fn deadzone_finder(&mut self, ui: &mut egui::Ui, status: &Status) {
        ui.heading("Find the game's dead zone");
        ui.label(
            RichText::new(
                "Stand still in the game, press the button, and watch the camera. The right stick is held at this tilt for 6 seconds. \
                 The dead zone is the highest tilt at which the camera does not move.",
            )
            .small()
            .color(GREY),
        );
        ui.add(egui::Slider::new(&mut self.test_tilt, 0.0..=0.6).text("Test tilt"));
        ui.horizontal(|ui| {
            let label = if status.testing { "Holding..." } else { "Hold for 6 seconds" };
            if ui.add_enabled(!status.testing, egui::Button::new(label)).clicked() {
                self.engine.send(Msg::TestTilt { tilt: self.test_tilt, seconds: 6.0 });
            }
            if ui.button("Use as dead zone").clicked() {
                self.cfg.aim.deadzone = self.test_tilt;
                self.changed();
            }
        });
    }

    fn keys(&mut self, ui: &mut egui::Ui) {
        egui::CollapsingHeader::new(RichText::new("Keys").heading()).default_open(false).show(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open settings file").clicked() {
                    self.save_now();
                    let _ = std::process::Command::new("notepad.exe").arg(&self.path).spawn();
                }
                if ui.button("Reload settings file").clicked() {
                    self.reload();
                }
                if ui.button("Reset keys to defaults").clicked() {
                    self.cfg.keys = Config::default().keys;
                    self.changed();
                }
            });
            ui.label(RichText::new("Keys are changed in the settings file for now: edit, save, then Reload.").small().color(GREY));
            ui.add_space(4.0);
            let mut names: Vec<&String> = self.cfg.keys.foot.keys().chain(self.cfg.keys.drive.keys()).collect();
            names.sort_by_key(|n| (Input::from_name(n).map(|i| i.0), (*n).clone()));
            names.dedup();
            egui::Grid::new("keys").striped(true).num_columns(3).spacing([24.0, 4.0]).show(ui, |ui| {
                ui.label(RichText::new("Key").strong());
                ui.label(RichText::new("On foot").strong());
                ui.label(RichText::new("Driving").strong());
                ui.end_row();
                for name in names {
                    ui.label(Input::from_name(name).map_or_else(|| name.clone(), Input::name));
                    ui.label(describe(&self.cfg.keys.foot, name));
                    ui.label(describe(&self.cfg.keys.drive, name));
                    ui.end_row();
                }
            });
        });
    }
}

/// What a key does in one layout, in the game's words.
fn describe(bindings: &Bindings, key: &str) -> String {
    let Some(bound) = bindings.get(key) else {
        return String::new();
    };
    let parts: Vec<&str> = bound.names().into_iter().map(|n| Action::from_name(n).map_or("?", Action::label)).collect();
    parts.join(" + ")
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let status = self.engine.status();
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                self.header(ui, &status);
                self.problems(ui, &status);
                ui.separator();
                ui.columns(2, |columns| {
                    self.aim_settings(&mut columns[0]);
                    let right = &mut columns[1];
                    right.heading("What the console receives");
                    draw_pad(right, &status.pad);
                    if status.mouse_rate > 0.0 {
                        right.label(RichText::new(format!("Mouse: about {:.0} reports a second", status.mouse_rate)).small().color(GREY));
                    }
                    right.add_space(10.0);
                    self.deadzone_finder(right, &status);
                });
                ui.separator();
                self.keys(ui);
            });
        });
        if self.unsaved_since.is_some_and(|since| since.elapsed() >= SAVE_AFTER) {
            self.save_now();
        }
        ui.ctx().request_repaint_after(Duration::from_millis(33));
    }
}

impl Drop for App {
    fn drop(&mut self) {
        if self.unsaved_since.is_some() {
            self.save_now();
        }
    }
}

/// A simple picture of the controller: sticks, triggers and every button, lit while pressed.
fn draw_pad(ui: &mut egui::Ui, pad: &PadState) {
    let width = ui.available_width().clamp(260.0, 360.0);
    let (response, painter) = ui.allocate_painter(Vec2::new(width, 190.0), Sense::hover());
    let r = response.rect;
    let unit = width / 360.0;
    let at = |x: f32, y: f32| Pos2::new(r.left() + x * unit, r.top() + y * unit);
    let lit = |on: bool| if on { GREEN } else { DIM };
    let text = |p: Pos2, s: &str, on: bool| {
        painter.text(p, Align2::CENTER_CENTER, s, FontId::proportional(11.0 * unit.max(0.8)), if on { Color32::BLACK } else { GREY });
    };

    // Triggers (filled by how far they are pulled) and shoulder buttons.
    for (x, value, name, shoulder, shoulder_name) in [(40.0, pad.l2, "L2", Button::L1, "L1"), (250.0, pad.r2, "R2", Button::R1, "R1")] {
        let rect = egui::Rect::from_min_size(at(x, 4.0), Vec2::new(70.0 * unit, 16.0 * unit));
        painter.rect_filled(rect, 4.0, DIM);
        let mut fill = rect;
        fill.set_width(rect.width() * value.clamp(0.0, 1.0));
        painter.rect_filled(fill, 4.0, GREEN);
        text(rect.center(), name, value > 0.5);
        let rect = egui::Rect::from_min_size(at(x, 24.0), Vec2::new(70.0 * unit, 14.0 * unit));
        painter.rect_filled(rect, 4.0, lit(pad.pressed(shoulder)));
        text(rect.center(), shoulder_name, pad.pressed(shoulder));
    }

    // D-pad on the left, face buttons on the right.
    for (button, x, y, name) in [
        (Button::DpadUp, 60.0, 62.0, "^"),
        (Button::DpadDown, 60.0, 106.0, "v"),
        (Button::DpadLeft, 38.0, 84.0, "<"),
        (Button::DpadRight, 82.0, 84.0, ">"),
        (Button::Triangle, 300.0, 62.0, "Tri"),
        (Button::Cross, 300.0, 106.0, "X"),
        (Button::Square, 278.0, 84.0, "Sq"),
        (Button::Circle, 322.0, 84.0, "O"),
    ] {
        painter.circle_filled(at(x, y), 11.0 * unit, lit(pad.pressed(button)));
        text(at(x, y), name, pad.pressed(button));
    }

    // Middle buttons.
    for (button, x, y, w, name) in [
        (Button::Create, 118.0, 50.0, 34.0, "Create"),
        (Button::Options, 242.0, 50.0, 38.0, "Options"),
        (Button::Touchpad, 180.0, 62.0, 64.0, "Touchpad"),
        (Button::Ps, 180.0, 100.0, 26.0, "PS"),
    ] {
        let rect = egui::Rect::from_center_size(at(x, y), Vec2::new(w * unit, 16.0 * unit));
        painter.rect_filled(rect, 5.0, lit(pad.pressed(button)));
        text(rect.center(), name, pad.pressed(button));
    }

    // Sticks: the ring is the stick's range, the dot where it is, lit when clicked.
    for (x, sx, sy, click, name) in [(125.0, pad.lx, pad.ly, Button::L3, "L3"), (235.0, pad.rx, pad.ry, Button::R3, "R3")] {
        let centre = at(x, 148.0);
        let radius = 30.0 * unit;
        painter.circle_filled(centre, radius, lit(pad.pressed(click)));
        painter.circle_stroke(centre, radius, Stroke::new(1.0, GREY));
        let dot = Pos2::new(centre.x + sx.clamp(-1.0, 1.0) * radius, centre.y - sy.clamp(-1.0, 1.0) * radius);
        painter.line_segment([centre, dot], Stroke::new(1.5, ORANGE));
        painter.circle_filled(dot, 5.0 * unit, ORANGE);
        painter.text(at(x, 186.0), Align2::CENTER_CENTER, name, FontId::proportional(10.0 * unit.max(0.8)), GREY);
    }
}
