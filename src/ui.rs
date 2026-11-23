//! The window: whether the takeover is on, a keyboard showing what every key does (click a key to
//! change it), the aim settings, and a live view of the controller the console receives.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, FontFamily, FontId, Galley, Pos2, Rect, RichText, Sense, Shape, Stroke, StrokeKind, Vec2};

use crate::config::{self, Action, Config, Mode, Triggers};
use crate::engine::{Engine, Msg, Status};
use crate::keys::Input;
use crate::pad::{Button, PadState};

// The window sits beside a game, so it stays dark and quiet. Colour is spent on two things only:
// the state (the blue of a DualSense light bar while playing) and which controller button a key
// presses (the four face buttons keep the colours printed on PlayStation pads).
const BG: Color32 = Color32::from_rgb(0x12, 0x14, 0x1C);
const PANEL: Color32 = Color32::from_rgb(0x1A, 0x1D, 0x28);
const KEY: Color32 = Color32::from_rgb(0x25, 0x2A, 0x3A);
const KEY_EMPTY: Color32 = Color32::from_rgb(0x17, 0x1A, 0x24);
const EDGE: Color32 = Color32::from_rgb(0x34, 0x3A, 0x4E);
const TEXT: Color32 = Color32::from_rgb(0xE8, 0xEA, 0xF2);
const MUTED: Color32 = Color32::from_rgb(0x7C, 0x84, 0x99);
const LIVE: Color32 = Color32::from_rgb(0x3F, 0xA9, 0xFF);
const READY: Color32 = Color32::from_rgb(0xFF, 0xB3, 0x47);
const BAD: Color32 = Color32::from_rgb(0xFF, 0x6B, 0x6B);
const DARK_TEXT: Color32 = Color32::from_rgb(0x0C, 0x0E, 0x14);

/// Changes are written to the file once the controls have been left alone this long.
const SAVE_AFTER: Duration = Duration::from_millis(600);

/// The colour that stands for a controller button everywhere in the window.
fn tint(action: Action) -> Color32 {
    match action {
        Action::Press(Button::Cross) => Color32::from_rgb(0x6E, 0xA8, 0xFF),
        Action::Press(Button::Circle) => Color32::from_rgb(0xFF, 0x5C, 0x6C),
        Action::Press(Button::Square) => Color32::from_rgb(0xF0, 0x8F, 0xD6),
        Action::Press(Button::Triangle) => Color32::from_rgb(0x2F, 0xD6, 0xA6),
        Action::Press(Button::L1 | Button::R1 | Button::L2 | Button::R2) | Action::Ease(..) => Color32::from_rgb(0xE8, 0xC2, 0x7A),
        Action::Press(Button::L3 | Button::R3) | Action::LsUp | Action::LsDown | Action::LsLeft | Action::LsRight => Color32::from_rgb(0xB7, 0xA6, 0xFF),
        Action::Press(Button::DpadUp | Button::DpadDown | Button::DpadLeft | Button::DpadRight) => Color32::from_rgb(0xC9, 0xCE, 0xDA),
        Action::Press(Button::Options | Button::Create | Button::Touchpad | Button::Ps) => Color32::from_rgb(0x9A, 0xA3, 0xB8),
        Action::ModeToggle | Action::ModeFoot | Action::ModeDrive => READY,
    }
}

fn mix(a: Color32, b: Color32, share_of_b: f32) -> Color32 {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * share_of_b).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

use crate::keys::Slot::{Gap, K};
use crate::keys::{BLOCK_GAP, MAIN, MAIN_UNITS, NAV};
const MOUSE: [&str; 7] = ["MouseLeft", "MouseRight", "MouseMiddle", "WheelUp", "WheelDown", "Mouse5", "Mouse4"];
const NAV_UNITS: f32 = 3.0;
const MOUSE_UNITS: f32 = 3.6;
const TOTAL_UNITS: f32 = MAIN_UNITS + BLOCK_GAP + NAV_UNITS + BLOCK_GAP + MOUSE_UNITS;

/// What is printed on the keycap.
fn legend(input: Input) -> String {
    input.label()
}

/// The groups the settings are sorted into. One shows at a time.
#[derive(Clone, Copy, PartialEq)]
enum Group {
    Aim,
    Driving,
    Triggers,
    Overlay,
}

impl Group {
    const ALL: [Group; 4] = [Group::Aim, Group::Driving, Group::Triggers, Group::Overlay];

    fn title(self) -> &'static str {
        match self {
            Group::Aim => "AIM",
            Group::Driving => "DRIVING",
            Group::Triggers => "TRIGGERS",
            Group::Overlay => "OVERLAY",
        }
    }

    /// What the group is about, in a line.
    fn about(self) -> &'static str {
        match self {
            Group::Aim => "How the mouse turns the camera.",
            Group::Driving => "The driving keys, and switching between them and the on-foot keys.",
            Group::Triggers => "For the few things in the game that want L2 or R2 held part of the way or let down slowly, like lifting weights. A key is all or nothing; these set how far it pulls.",
            Group::Overlay => "What is drawn on top of the game while you play.",
        }
    }
}

/// A slider with what it does written under it. Returns whether it was moved.
fn described(ui: &mut egui::Ui, enabled: bool, slider: egui::Slider<'_>, help: &str) -> bool {
    let moved = ui.add_enabled(enabled, slider).changed();
    note(ui, help);
    ui.add_space(3.0);
    moved
}

/// A line of explanation in small grey type.
fn note(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().color(MUTED));
}

pub struct App {
    engine: Engine,
    cfg: Config,
    path: PathBuf,
    load_error: Option<String>,
    save_error: Option<String>,
    unsaved_since: Option<Instant>,
    test_tilt: f32,
    /// The key being changed.
    selected: Option<Input>,
    /// The group of settings that is showing.
    group: Group,
    /// The typeface for keycaps, headings and the state word.
    display: FontFamily,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, engine: Engine, cfg: Config, path: PathBuf, load_error: Option<String>) -> App {
        let display = install_fonts(&cc.egui_ctx);
        apply_theme(&cc.egui_ctx);
        let test_tilt = cfg.aim.deadzone;
        App { engine, cfg, path, load_error, save_error: None, unsaved_since: None, test_tilt, selected: None, group: Group::Aim, display }
    }

    /// Opens with this key's editor showing.
    pub fn select(&mut self, input: Option<Input>) {
        self.selected = input;
    }

    /// Opens with this group of settings showing, by its title.
    pub fn show_group(&mut self, title: &str) {
        if let Some(group) = Group::ALL.into_iter().find(|g| g.title().eq_ignore_ascii_case(title)) {
            self.group = group;
        }
    }

    fn font(&self, size: f32) -> FontId {
        FontId::new(size, self.display.clone())
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

    fn reserved(&self, input: Input) -> Option<&'static str> {
        if Input::from_name(&self.cfg.toggle_key) == Some(input) {
            Some("On / Off")
        } else if !self.cfg.picture_key.is_empty() && Input::from_name(&self.cfg.picture_key) == Some(input) {
            Some("Picture")
        } else if input == Input::LALT {
            Some("Alt+Tab")
        } else {
            None
        }
    }

    /// The game's word for what a key does in a layout ("Jump"), or the button's name.
    fn job(&self, mode: Mode, actions: &[Action]) -> String {
        match self.cfg.labels.describe(mode, actions) {
            Some(name) => name.to_string(),
            None => actions.first().map_or(String::new(), |a| a.label().to_string()),
        }
    }

    fn header(&mut self, ui: &mut egui::Ui, status: &Status) {
        let key = self.cfg.toggle_key.clone();
        let (word, color, hint) = if status.live {
            ("PLAYING", LIVE, format!("Your keyboard and mouse are the controller. {key} or Alt+Tab gives them back."))
        } else if status.armed {
            ("READY", READY, "Click on the game window to start.".to_string())
        } else {
            ("OFF", MUTED, format!("Press {key} in the game window to turn it on."))
        };
        // The light bar: one line across the top in the colour of the state.
        let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 4.0), Sense::hover());
        ui.painter().rect_filled(bar, 2.0, if status.armed { color } else { EDGE });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new(word).font(self.font(34.0)).color(color));
            ui.add_space(14.0);
            ui.vertical(|ui| {
                ui.add_space(6.0);
                ui.label(RichText::new(hint).color(TEXT));
                let layout = RichText::new(format!("Layout in use: {}", status.mode.label())).small().color(MUTED);
                ui.label(layout);
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = if status.armed { format!("Turn off  ({key})") } else { format!("Turn on  ({key})") };
                if ui.add(egui::Button::new(RichText::new(label).font(self.font(15.0))).min_size(Vec2::new(150.0, 34.0))).clicked() {
                    self.engine.send(Msg::Arm(!status.armed));
                }
            });
        });
    }

    fn problems(&mut self, ui: &mut egui::Ui, status: &Status) {
        if let Some(error) = &status.output_error {
            ui.add_space(4.0);
            ui.colored_label(BAD, format!("No controller is being sent. {error}"));
            ui.horizontal_wrapped(|ui| {
                ui.label("The virtual controller needs the ViGEmBus driver:");
                ui.hyperlink("https://github.com/nefarius/ViGEmBus/releases");
                if ui.button("Try again").clicked() {
                    self.engine.send(Msg::ReconnectOutput);
                }
            });
        }
        for text in self.load_error.iter().chain(self.save_error.iter()) {
            ui.colored_label(BAD, text);
        }
        for warning in &status.warnings {
            ui.colored_label(READY, format!("Settings file: {warning}"));
        }
    }

    /// The keyboard and mouse, every key showing its job in the layout in use.
    fn keyboard(&mut self, ui: &mut egui::Ui, status: &Status) {
        let mode = status.mode;
        ui.horizontal(|ui| {
            ui.label(RichText::new("KEYS").font(self.font(13.0)).color(MUTED));
            ui.add_space(10.0);
            for m in [Mode::Foot, Mode::Drive] {
                let text = RichText::new(m.label()).font(self.font(15.0));
                if ui.selectable_label(mode == m, text).clicked() {
                    self.engine.send(Msg::SetMode(m));
                }
            }
            ui.add_space(10.0);
            ui.label(RichText::new("Click a key to change what it does.").small().color(MUTED));
        });
        ui.add_space(4.0);

        let unit = (ui.available_width() / TOTAL_UNITS).clamp(30.0, 62.0);
        let (response, painter) = ui.allocate_painter(Vec2::new(unit * TOTAL_UNITS, unit * 6.0), Sense::click());
        let origin = response.rect.min;
        let pad = (unit * 0.06).max(2.0);
        let mut keys: Vec<(Input, Rect)> = Vec::new();
        for (block, x0) in [(&MAIN, 0.0), (&NAV, MAIN_UNITS + BLOCK_GAP)] {
            for (row, slots) in block.iter().enumerate() {
                let mut x = x0;
                for slot in slots.iter() {
                    match slot {
                        Gap(w) => x += w,
                        K(name, w) => {
                            if let Some(input) = Input::from_name(name) {
                                let min = origin + Vec2::new(x * unit, row as f32 * unit);
                                keys.push((input, Rect::from_min_size(min, Vec2::new(w * unit, unit)).shrink(pad)));
                            }
                            x += w;
                        }
                    }
                }
            }
        }
        let mouse_x = MAIN_UNITS + BLOCK_GAP + NAV_UNITS + BLOCK_GAP;
        let row_h = unit * 5.55 / MOUSE.len() as f32;
        painter.text(origin + Vec2::new(mouse_x * unit + pad, unit * 0.22), Align2::LEFT_CENTER, "MOUSE", self.font(11.0), MUTED);
        for (i, name) in MOUSE.iter().enumerate() {
            if let Some(input) = Input::from_name(name) {
                let min = origin + Vec2::new(mouse_x * unit, unit * 0.45 + i as f32 * row_h);
                keys.push((input, Rect::from_min_size(min, Vec2::new(MOUSE_UNITS * unit, row_h)).shrink(pad)));
            }
        }

        let hovered = response.hover_pos().and_then(|p| keys.iter().find(|(_, r)| r.contains(p)).map(|(i, _)| *i));
        if response.clicked() {
            if let Some(input) = hovered.filter(|i| self.reserved(*i).is_none()) {
                self.selected = if self.selected == Some(input) { None } else { Some(input) };
            } else {
                self.selected = None;
            }
        }
        if hovered.is_some_and(|i| self.reserved(i).is_none()) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }

        for (input, rect) in &keys {
            let actions = self.cfg.actions_of(mode, *input);
            let reserved = self.reserved(*input);
            let main = actions.iter().copied().find(|a| !a.switches_layout()).or(actions.first().copied());
            let color = main.map(tint);
            let held = status.is_held(*input);
            let fill = match (reserved, color) {
                (Some(_), _) => BG,
                (None, Some(c)) if held => c,
                (None, Some(c)) => mix(KEY, c, 0.10),
                (None, None) => KEY_EMPTY,
            };
            let fill = if hovered == Some(*input) && !held && reserved.is_none() { mix(fill, TEXT, 0.08) } else { fill };
            let radius = (unit * 0.14).min(8.0);
            painter.rect_filled(*rect, radius, fill);
            if reserved.is_some() || color.is_none() {
                painter.rect_stroke(*rect, radius, Stroke::new(1.0, EDGE), StrokeKind::Inside);
            }
            if self.selected == Some(*input) {
                painter.rect_stroke(*rect, radius, Stroke::new(2.0, TEXT), StrokeKind::Inside);
            }
            let ink = if held { DARK_TEXT } else { TEXT };
            let quiet = if held { DARK_TEXT } else { MUTED };
            let wide = rect.width() > rect.height() * 2.6 && input.is_mouse();
            let inner = rect.shrink(unit * 0.11);
            let cap = self.fitted(&painter, &legend(*input), inner.width() * if wide { 0.5 } else { 0.62 }, (unit * 0.21).clamp(9.0, 12.0), quiet);
            let job = match reserved {
                Some(text) => text.to_string(),
                None => self.job(mode, &actions),
            };
            let glyph_r = (unit * 0.13).clamp(4.5, 8.0);
            let also_switches = actions.iter().any(|a| a.switches_layout()) && main.is_some_and(|a| !a.switches_layout());
            if wide {
                // Mouse rows: name on the left, job on the right, button mark at the far right.
                painter.galley(Pos2::new(inner.left(), inner.center().y - cap.size().y / 2.0), cap, quiet);
                let marks = if also_switches { glyph_r * 5.4 } else { glyph_r * 3.0 };
                let room = inner.width() * 0.5 - marks;
                let text = self.fitted(&painter, &job, room.max(10.0), (unit * 0.25).clamp(10.0, 14.0), ink);
                let right = inner.right() - marks;
                painter.galley(Pos2::new(right - text.size().x, inner.center().y - text.size().y / 2.0), text, ink);
                if let Some(a) = main {
                    self.glyph(&painter, Pos2::new(inner.right() - glyph_r, inner.center().y), glyph_r, a, if held { DARK_TEXT } else { tint(a) });
                }
            } else {
                painter.galley(inner.left_top(), cap, quiet);
                let room = if also_switches { inner.width() - glyph_r * 2.2 } else { inner.width() };
                let text = self.fitted(&painter, &job, room, (unit * 0.25).clamp(10.0, 14.0), if reserved.is_some() { MUTED } else { ink });
                painter.galley(Pos2::new(inner.left(), inner.bottom() - text.size().y), text, ink);
                if let Some(a) = main {
                    self.glyph(&painter, Pos2::new(inner.right() - glyph_r, inner.top() + glyph_r + 1.0), glyph_r, a, if held { DARK_TEXT } else { tint(a) });
                }
            }
            // A key that also switches layout carries a small marker: under its button mark, or
            // beside it on the wide mouse rows.
            if also_switches {
                let at = if wide {
                    Pos2::new(inner.right() - glyph_r * 3.6, inner.center().y)
                } else {
                    Pos2::new(inner.right() - glyph_r, inner.top() + glyph_r * 3.4)
                };
                self.glyph(&painter, at, glyph_r * 0.8, Action::ModeToggle, if held { DARK_TEXT } else { READY });
            }
        }
    }

    /// Lays text out in the display face, shrinking it until it fits the width.
    fn fitted(&self, painter: &egui::Painter, text: &str, width: f32, size: f32, color: Color32) -> Arc<Galley> {
        let mut size = size;
        loop {
            let galley = painter.layout_no_wrap(text.to_owned(), self.font(size), color);
            if galley.size().x <= width || size <= 7.5 {
                return galley;
            }
            size -= 0.5;
        }
    }

    /// The mark of a controller button, drawn rather than typed so it looks the same everywhere.
    fn glyph(&self, painter: &egui::Painter, c: Pos2, r: f32, action: Action, color: Color32) {
        let line = Stroke::new((r * 0.28).clamp(1.2, 2.2), color);
        let arrow = |dx: f32, dy: f32| {
            // A solid arrowhead pointing along (dx, dy).
            let tip = c + Vec2::new(dx, dy) * r;
            let back = c - Vec2::new(dx, dy) * r * 0.6;
            let side = Vec2::new(-dy, dx) * r * 0.85;
            painter.add(Shape::convex_polygon(vec![tip, back + side, back - side], color, Stroke::NONE));
        };
        let chevron = |dx: f32, dy: f32| {
            let tip = c + Vec2::new(dx, dy) * r * 0.7;
            let back = c - Vec2::new(dx, dy) * r * 0.3;
            let side = Vec2::new(-dy, dx) * r * 0.85;
            painter.add(Shape::line(vec![back + side, tip, back - side], line));
        };
        let word = |text: &str| {
            painter.text(c, Align2::CENTER_CENTER, text, self.font((r * 1.75).max(8.0)), color);
        };
        match action {
            Action::Press(Button::Cross) => {
                let d = r * 0.72;
                painter.line_segment([c + Vec2::new(-d, -d), c + Vec2::new(d, d)], line);
                painter.line_segment([c + Vec2::new(-d, d), c + Vec2::new(d, -d)], line);
            }
            Action::Press(Button::Circle) => {
                painter.circle_stroke(c, r * 0.82, line);
            }
            Action::Press(Button::Square) => {
                painter.rect_stroke(Rect::from_center_size(c, Vec2::splat(r * 1.5)), 0.0, line, StrokeKind::Middle);
            }
            Action::Press(Button::Triangle) => {
                let pts = vec![c + Vec2::new(0.0, -r * 0.9), c + Vec2::new(r * 0.9, r * 0.7), c + Vec2::new(-r * 0.9, r * 0.7)];
                painter.add(Shape::closed_line(pts, line));
            }
            Action::Press(Button::DpadUp) => arrow(0.0, -1.0),
            Action::Press(Button::DpadDown) => arrow(0.0, 1.0),
            Action::Press(Button::DpadLeft) => arrow(-1.0, 0.0),
            Action::Press(Button::DpadRight) => arrow(1.0, 0.0),
            Action::LsUp => chevron(0.0, -1.0),
            Action::LsDown => chevron(0.0, 1.0),
            Action::LsLeft => chevron(-1.0, 0.0),
            Action::LsRight => chevron(1.0, 0.0),
            Action::Press(Button::Options) => {
                for dy in [-0.55, 0.0, 0.55] {
                    painter.line_segment([c + Vec2::new(-r * 0.8, dy * r), c + Vec2::new(r * 0.8, dy * r)], line);
                }
            }
            Action::Press(Button::Touchpad) => {
                painter.rect_stroke(Rect::from_center_size(c, Vec2::new(r * 2.0, r * 1.2)), r * 0.3, line, StrokeKind::Middle);
            }
            Action::Press(Button::Create) => {
                for dx in [-0.55, 0.0, 0.55] {
                    painter.line_segment([c + Vec2::new(dx * r, -r * 0.7), c + Vec2::new(dx * r, r * 0.7)], line);
                }
            }
            Action::Press(b @ (Button::L1 | Button::R1 | Button::L2 | Button::R2 | Button::L3 | Button::R3 | Button::Ps)) => word(b.label()),
            Action::Ease(which, more) => {
                // The trigger's name with a minus or a plus: less pull, more pull.
                let name = match which {
                    Triggers::L2 => "L2",
                    Triggers::R2 => "R2",
                    Triggers::Both => "LR",
                };
                let text = format!("{name}{}", if more { "+" } else { "-" });
                painter.text(c + Vec2::new(-r * 0.5, 0.0), Align2::CENTER_CENTER, text, self.font((r * 1.45).max(8.0)), color);
            }
            Action::ModeToggle | Action::ModeFoot | Action::ModeDrive => {
                // Two arrows passing each other.
                let (up, down) = (c + Vec2::new(0.0, -r * 0.4), c + Vec2::new(0.0, r * 0.4));
                painter.line_segment([up + Vec2::new(-r, 0.0), up + Vec2::new(r, 0.0)], line);
                painter.line_segment([up + Vec2::new(r, 0.0), up + Vec2::new(r * 0.4, -r * 0.5)], line);
                painter.line_segment([down + Vec2::new(-r, 0.0), down + Vec2::new(r, 0.0)], line);
                painter.line_segment([down + Vec2::new(-r, 0.0), down + Vec2::new(-r * 0.4, r * 0.5)], line);
            }
        }
    }

    /// Below the keyboard: what the selected key does, and the choices for changing it.
    fn key_editor(&mut self, ui: &mut egui::Ui, status: &Status) {
        let mode = status.mode;
        let Some(input) = self.selected else {
            return;
        };
        let actions = self.cfg.actions_of(mode, input);
        let switches = actions.iter().any(|a| a.switches_layout());
        egui::Frame::new().fill(PANEL).corner_radius(8.0).inner_margin(12.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(legend(input)).font(self.font(22.0)).color(TEXT));
                ui.add_space(8.0);
                let now = if actions.is_empty() { "nothing yet".to_string() } else { self.job(mode, &actions) };
                ui.label(RichText::new(format!("{}: {now}", mode.label())).color(TEXT));
                ui.label(RichText::new("Pick its job below.").color(MUTED));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Done").clicked() {
                        self.selected = None;
                    }
                    if ui.button("Clear").clicked() {
                        self.cfg.bind(mode, input, &[]);
                        self.changed();
                    }
                });
            });
            ui.add_space(6.0);
            let combine = ui.input(|i| i.modifiers.ctrl);
            let mut next: Option<Vec<Action>> = None;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                for action in Action::ALL.into_iter().filter(|a| !a.switches_layout()) {
                    let on = actions.contains(&action);
                    let name = match self.cfg.labels.describe(mode, &[action]) {
                        Some(game) => format!("{game}  ·  {}", action.label()),
                        None => action.label().to_string(),
                    };
                    let text = RichText::new(name).color(if on { DARK_TEXT } else { tint(action) });
                    let button = egui::Button::new(text).fill(if on { tint(action) } else { KEY }).min_size(Vec2::new(0.0, 26.0));
                    if ui.add(button).clicked() {
                        let mut list: Vec<Action> = actions.iter().copied().filter(|a| !a.switches_layout()).collect();
                        if combine {
                            // Ctrl+click adds this button to the key, or takes it off again.
                            if on { list.retain(|a| *a != action) } else { list.push(action) }
                        } else {
                            list = vec![action];
                        }
                        list.extend(actions.iter().copied().filter(|a| a.switches_layout()));
                        next = Some(list);
                    }
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let mut also = switches;
                if ui.checkbox(&mut also, "Also switches between the on-foot and driving layouts").changed() {
                    let mut list: Vec<Action> = actions.iter().copied().filter(|a| !a.switches_layout()).collect();
                    if also {
                        list.push(Action::ModeToggle);
                    }
                    next = Some(list);
                }
                ui.label(RichText::new("Ctrl+click a job to press two buttons with one key.").small().color(MUTED));
            });
            if let Some(list) = next {
                self.cfg.bind(mode, input, &list);
                self.changed();
            }
        });
    }

    /// The settings, sorted into groups: a row of group names, and under it the one that is showing.
    fn settings(&mut self, ui: &mut egui::Ui, status: &Status) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for group in Group::ALL {
                let on = self.group == group;
                let name = RichText::new(group.title()).font(self.font(13.0)).color(if on { TEXT } else { MUTED });
                if ui.selectable_label(on, name).clicked() {
                    self.group = group;
                }
            }
        });
        ui.add_space(4.0);
        note(ui, self.group.about());
        ui.add_space(6.0);
        let changed = match self.group {
            Group::Aim => self.aim_settings(ui),
            Group::Driving => self.driving_settings(ui),
            Group::Triggers => self.trigger_settings(ui),
            Group::Overlay => self.overlay_settings(ui, status),
        };
        if changed {
            self.changed();
        }
    }

    fn aim_settings(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = false;
        let a = &mut self.cfg.aim;
        let mut slider = |ui: &mut egui::Ui, value: &mut f32, range: std::ops::RangeInclusive<f32>, name: &str, help: &str| {
            changed |= described(ui, true, egui::Slider::new(value, range).text(name), help);
        };
        slider(ui, &mut a.sensitivity, 0.2..=12.0, "Mouse speed", "How far the camera turns for a mouse movement.");
        slider(ui, &mut a.vertical, 0.3..=2.0, "Up/down speed", "Vertical speed compared with sideways.");
        slider(ui, &mut a.aim_in, 0.2..=2.0, "Speed while aiming in", "Mouse speed while the aim button (L2) is held, compared with normal.");
        slider(ui, &mut a.deadzone, 0.0..=0.6, "Dead zone", "The part of the stick the game ignores. Too high: tiny movements jump. Too low: they do nothing.");
        slider(ui, &mut a.curve, 1.0..=3.0, "Response curve", "Undoes the game's stick curve. 1 is off. Higher makes slow movements turn more.");
        slider(ui, &mut a.smoothing, 0.0..=2.0, "Smoothing", "1 is normal. Lower reacts faster, higher is steadier.");
        ui.horizontal(|ui| {
            changed |= ui.checkbox(&mut a.invert_y, "Invert up/down").changed();
            ui.add_space(12.0);
            ui.label("Game runs at");
            for fps in [30.0, 60.0] {
                if ui.selectable_label(self.cfg.game_fps == fps, format!("{fps} fps")).clicked() {
                    self.cfg.game_fps = fps;
                    changed = true;
                }
            }
        });
        changed
    }

    fn driving_settings(&mut self, ui: &mut egui::Ui) -> bool {
        let mut changed = ui.checkbox(&mut self.cfg.steering.smooth, "Ease steering keys while driving").changed();
        note(ui, "The steering keys lean into the stick the way a thumb does, instead of going to full lock at once.");
        ui.add_space(6.0);
        changed |= ui.checkbox(&mut self.cfg.switch_on_name_card, "Switch to driving keys when the vehicle's name appears (experimental)").changed();
        note(ui, "The get-in key then only gets in. The switch happens when the game shows the vehicle's name above the minimap. Getting out and the switch key work as before.");
        ui.add_space(6.0);
        changed |= ui.checkbox(&mut self.cfg.sounds, "Beep when the layout switches").changed();
        note(ui, "Also when the takeover turns on or off.");
        changed
    }

    fn trigger_settings(&mut self, ui: &mut egui::Ui) -> bool {
        let t = &mut self.cfg.triggers;
        let mut changed = ui.checkbox(&mut t.wheel_dial, "The mouse wheel sets how far L2 and R2 are pulled").changed();
        note(ui, "Wheel down: less. Wheel up: more. It sets the triggers whose keys are held, one alone or both together, and both when neither is held, ahead of the press.");
        ui.add_space(3.0);
        let notch = egui::Slider::new(&mut t.step, 0.05..=0.5).step_by(0.05).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)).text("One wheel notch");
        changed |= described(ui, t.wheel_dial, notch, "How much one notch changes the pull. Larger: fewer notches from all the way to nothing.");
        ui.add_space(4.0);
        ui.label("Keys can do the same.");
        note(ui, "Click a key on the keyboard above and give it one of the jobs L2 less, L2 more, R2 less, R2 more, L2 + R2 less or L2 + R2 more. The pull moves while the key is held and stays where it is when the key is let go.");
        ui.add_space(6.0);
        let lowering = egui::Slider::new(&mut t.lower_seconds, 0.0..=4.0).step_by(0.1).suffix(" s").text("Lowering takes");
        changed |= described(ui, true, lowering, "From all the way to nothing. A held trigger never comes down faster than this, however fast the wheel turns. 0 for at once.");
        let raising = egui::Slider::new(&mut t.raise_seconds, 0.0..=4.0).step_by(0.1).suffix(" s").text("Raising with a key takes");
        changed |= described(ui, true, raising, "From nothing to all the way, while a \"more\" key is held. 0 for at once. The wheel always raises at once.");
        let whole = egui::Slider::new(&mut t.reset_after, 1.0..=15.0).step_by(0.5).suffix(" s").text("A whole pull again after");
        changed |= described(ui, true, whole, "This long after the trigger keys are let go, they pull all the way again. The label on the game shows the pull while it is not.");
        changed
    }

    fn overlay_settings(&mut self, ui: &mut egui::Ui, status: &Status) -> bool {
        let o = &mut self.cfg.overlay;
        let mut changed = ui.checkbox(&mut o.enabled, "Show the layout in use on top of the game").changed();
        changed |= ui.add_enabled(o.enabled, egui::Checkbox::new(&mut o.prompts, "Show my key for button prompts the game puts on screen")).changed();
        changed |= ui.add_enabled(o.enabled && o.prompts, egui::Checkbox::new(&mut o.on_prompt, "Draw the key on the prompt itself")).changed();
        note(ui, "Off: the keys are listed in the label at the bottom left instead.");
        ui.add_space(6.0);
        // How often the game picture is searched, and what that really costs on this computer.
        let checks = egui::Slider::new(&mut o.checks_per_second, 2.0..=30.0).step_by(1.0).text("Prompt checks a second");
        let cost = if status.checks_per_second > 0.0 {
            format!("Now: {:.0} checks a second, {:.0} ms each. More checks: keys appear sooner and follow a prompt that moves. If the game's picture stutters, turn it down.", status.checks_per_second, status.check_ms)
        } else {
            "More checks: keys appear sooner and follow a prompt that moves, for more processor time. 4 is the calm setting.".to_string()
        };
        changed |= described(ui, o.enabled && o.prompts, checks, &cost);
        changed
    }

    fn deadzone_finder(&mut self, ui: &mut egui::Ui, status: &Status) {
        ui.label(RichText::new("FIND THE GAME'S DEAD ZONE").font(self.font(13.0)).color(MUTED));
        ui.label(
            RichText::new("Stand still in the game, press Hold, and watch the camera. The dead zone is the highest tilt at which it does not move.")
                .small()
                .color(MUTED),
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

    fn file_row(&mut self, ui: &mut egui::Ui, status: &Status) {
        ui.horizontal(|ui| {
            if ui.button("Reset keys to defaults").clicked() {
                self.cfg.keys = Config::default().keys;
                self.cfg.labels = Config::default().labels;
                self.selected = None;
                self.changed();
            }
            if ui.button("Open settings file").clicked() {
                self.save_now();
                let _ = std::process::Command::new("notepad.exe").arg(&self.path).spawn();
            }
            if ui.button("Reload settings file").clicked() {
                self.reload();
            }
            if ui.button("Open saved pictures").clicked() {
                let dir = config::pictures_dir();
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::process::Command::new("explorer.exe").arg(&dir).spawn();
            }
            let key = &self.cfg.picture_key;
            match &status.picture {
                Some(Ok(_)) => ui.label(RichText::new(format!("{} saved. {key} while playing saves another.", plural(status.pictures_saved))).small().color(LIVE)),
                Some(Err(why)) => ui.label(RichText::new(format!("Picture not saved: {why}.")).small().color(BAD)),
                None if key.is_empty() => ui.label(""),
                None => ui.label(RichText::new(format!("{key} while playing saves a picture of the game.")).small().color(MUTED)),
            };
        });
    }

    /// The controller the console receives: sticks, triggers and every button, lit while pressed.
    fn controller(&self, ui: &mut egui::Ui, pad: &PadState) {
        ui.label(RichText::new("WHAT THE CONSOLE RECEIVES").font(self.font(13.0)).color(MUTED));
        let width = ui.available_width().clamp(260.0, 380.0);
        let (response, painter) = ui.allocate_painter(Vec2::new(width, width * 0.5), Sense::hover());
        let r = response.rect;
        let unit = width / 360.0;
        let at = |x: f32, y: f32| Pos2::new(r.left() + x * unit, r.top() + y * unit);

        // Triggers fill by how far they are pulled; shoulder buttons sit under them.
        for (x, value, trig, shoulder) in [(34.0, pad.l2, Button::L2, Button::L1), (256.0, pad.r2, Button::R2, Button::R1)] {
            let color = tint(Action::Press(trig));
            let rect = Rect::from_min_size(at(x, 2.0), Vec2::new(70.0 * unit, 15.0 * unit));
            painter.rect_filled(rect, 4.0, KEY);
            let mut fill = rect;
            fill.set_width(rect.width() * value.clamp(0.0, 1.0));
            painter.rect_filled(fill, 4.0, color);
            painter.text(rect.center(), Align2::CENTER_CENTER, trig.label(), self.font(10.0 * unit.max(0.9)), if value > 0.5 { DARK_TEXT } else { color });
            let on = pad.pressed(shoulder);
            let rect = Rect::from_min_size(at(x, 21.0), Vec2::new(70.0 * unit, 13.0 * unit));
            painter.rect_filled(rect, 4.0, if on { color } else { KEY });
            painter.text(rect.center(), Align2::CENTER_CENTER, shoulder.label(), self.font(10.0 * unit.max(0.9)), if on { DARK_TEXT } else { color });
        }
        // D-pad, face buttons and the middle row, each with its own mark.
        for (button, x, y) in [
            (Button::DpadUp, 60.0, 58.0),
            (Button::DpadDown, 60.0, 100.0),
            (Button::DpadLeft, 39.0, 79.0),
            (Button::DpadRight, 81.0, 79.0),
            (Button::Triangle, 300.0, 58.0),
            (Button::Cross, 300.0, 100.0),
            (Button::Square, 279.0, 79.0),
            (Button::Circle, 321.0, 79.0),
            (Button::Create, 122.0, 50.0),
            (Button::Options, 238.0, 50.0),
            (Button::Touchpad, 180.0, 58.0),
            (Button::Ps, 180.0, 96.0),
        ] {
            let action = Action::Press(button);
            let on = pad.pressed(button);
            painter.circle_filled(at(x, y), 11.0 * unit, if on { tint(action) } else { KEY });
            self.glyph(&painter, at(x, y), 5.5 * unit, action, if on { DARK_TEXT } else { tint(action) });
        }
        // Sticks: the ring is the stick's range, the dot where it is; the ring lights when clicked.
        for (x, sx, sy, click) in [(125.0, pad.lx, pad.ly, Button::L3), (235.0, pad.rx, pad.ry, Button::R3)] {
            let color = tint(Action::Press(click));
            let centre = at(x, 142.0);
            let radius = 30.0 * unit;
            painter.circle_filled(centre, radius, if pad.pressed(click) { mix(KEY, color, 0.55) } else { KEY });
            painter.circle_stroke(centre, radius, Stroke::new(1.0, EDGE));
            let dot = Pos2::new(centre.x + sx.clamp(-1.0, 1.0) * radius, centre.y - sy.clamp(-1.0, 1.0) * radius);
            painter.line_segment([centre, dot], Stroke::new(1.5, color));
            painter.circle_filled(dot, 5.0 * unit, color);
        }
    }
}

fn plural(pictures: u32) -> String {
    if pictures == 1 { "1 picture".to_string() } else { format!("{pictures} pictures") }
}

/// Keycaps, headings and the state word use Bahnschrift, the DIN-style face Windows ships with:
/// it is the lettering of signs and real keycaps. Body text stays in egui's own face. If the
/// font file is missing the display face simply falls back to the body face.
fn install_fonts(ctx: &egui::Context) -> FontFamily {
    let display = FontFamily::Name("display".into());
    let mut fonts = egui::FontDefinitions::default();
    let mut stack = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let windows = std::env::var_os("WINDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    if let Ok(bytes) = std::fs::read(windows.join("Fonts").join("bahnschrift.ttf")) {
        fonts.font_data.insert("bahnschrift".to_owned(), Arc::new(egui::FontData::from_owned(bytes)));
        stack.insert(0, "bahnschrift".to_owned());
    }
    fonts.families.insert(display.clone(), stack);
    ctx.set_fonts(fonts);
    display
}

fn apply_theme(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = PANEL;
    v.extreme_bg_color = Color32::from_rgb(0x0C, 0x0E, 0x14);
    v.faint_bg_color = PANEL;
    v.hyperlink_color = LIVE;
    v.selection.bg_fill = mix(BG, LIVE, 0.55);
    v.selection.stroke = Stroke::new(1.0, TEXT);
    v.slider_trailing_fill = true;
    v.widgets.noninteractive.fg_stroke.color = TEXT;
    v.widgets.noninteractive.bg_stroke.color = EDGE;
    v.widgets.inactive.bg_fill = KEY;
    v.widgets.inactive.weak_bg_fill = KEY;
    v.widgets.inactive.fg_stroke.color = TEXT;
    v.widgets.hovered.bg_fill = mix(KEY, TEXT, 0.12);
    v.widgets.hovered.weak_bg_fill = mix(KEY, TEXT, 0.12);
    v.widgets.active.bg_fill = mix(KEY, LIVE, 0.5);
    v.widgets.active.weak_bg_fill = mix(KEY, LIVE, 0.5);
    ctx.set_visuals(v);
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let status = self.engine.status();
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                self.header(ui, &status);
                self.problems(ui, &status);
                ui.add_space(10.0);
                self.keyboard(ui, &status);
                ui.add_space(6.0);
                self.key_editor(ui, &status);
                ui.add_space(4.0);
                self.file_row(ui, &status);
                ui.add_space(8.0);
                ui.separator();
                ui.add_space(4.0);
                ui.columns(2, |columns| {
                    self.settings(&mut columns[0], &status);
                    let right = &mut columns[1];
                    self.controller(right, &status.pad);
                    if status.mouse_rate > 0.0 {
                        right.label(RichText::new(format!("Mouse: about {:.0} reports a second", status.mouse_rate)).small().color(MUTED));
                    }
                    right.add_space(8.0);
                    self.deadzone_finder(right, &status);
                });
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
