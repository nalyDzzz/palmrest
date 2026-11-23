//! Settings and key layouts, stored as a TOML file the user can also edit by hand.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::keys::Input;
use crate::pad::Button;

pub const APP_NAME: &str = "ps5-remote-sim";

/// Which key layout is in use.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Foot,
    Drive,
}

impl Mode {
    pub fn other(self) -> Mode {
        match self {
            Mode::Foot => Mode::Drive,
            Mode::Drive => Mode::Foot,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Foot => "On foot",
            Mode::Drive => "Driving",
        }
    }
}

/// What a key does when it is held.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Press(Button),
    LsUp,
    LsDown,
    LsLeft,
    LsRight,
    ModeToggle,
    ModeFoot,
    ModeDrive,
}

impl Action {
    pub fn from_name(name: &str) -> Option<Action> {
        let n = name.trim().to_ascii_lowercase();
        Some(match n.as_str() {
            "ls_up" => Action::LsUp,
            "ls_down" => Action::LsDown,
            "ls_left" => Action::LsLeft,
            "ls_right" => Action::LsRight,
            "mode_toggle" => Action::ModeToggle,
            "mode_foot" => Action::ModeFoot,
            "mode_drive" => Action::ModeDrive,
            other => Action::Press(Button::from_name(other)?),
        })
    }

    /// How the action reads in the window.
    pub fn label(self) -> &'static str {
        match self {
            Action::Press(b) => b.label(),
            Action::LsUp => "Left stick up",
            Action::LsDown => "Left stick down",
            Action::LsLeft => "Left stick left",
            Action::LsRight => "Left stick right",
            Action::ModeToggle => "Switch layout",
            Action::ModeFoot => "On-foot layout",
            Action::ModeDrive => "Driving layout",
        }
    }
}

/// One key's entry in the file: `W = "ls_up"` or `F = ["triangle", "mode_toggle"]`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Bound {
    One(String),
    Many(Vec<String>),
}

impl Bound {
    pub fn names(&self) -> Vec<&str> {
        match self {
            Bound::One(s) => vec![s.as_str()],
            Bound::Many(v) => v.iter().map(String::as_str).collect(),
        }
    }
}

pub type Bindings = BTreeMap<String, Bound>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AimConfig {
    /// Mouse speed. Full stick tilt is reached at 6000 / sensitivity mouse counts per second.
    pub sensitivity: f32,
    /// Vertical speed relative to horizontal.
    pub vertical: f32,
    /// Speed while the aim button (L2) is held, relative to normal.
    pub aim_in: f32,
    /// The game's stick dead zone (0..1). Stick output starts just past it.
    pub deadzone: f32,
    /// The game's stick response exponent, undone so camera speed follows mouse speed. 1 = leave as is.
    pub curve: f32,
    /// 1 = default smoothing, 0 = none, 2 = double.
    pub smoothing: f32,
    pub invert_y: bool,
}

impl Default for AimConfig {
    fn default() -> Self {
        AimConfig {
            sensitivity: 3.0,
            vertical: 1.0,
            aim_in: 1.0,
            deadzone: 0.30,
            curve: 1.8,
            smoothing: 1.0,
            invert_y: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SteeringConfig {
    /// Ease A / D into the stick while driving, so a key steers like a thumb and not a slam.
    pub smooth: bool,
}

impl Default for SteeringConfig {
    fn default() -> Self {
        SteeringConfig { smooth: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Keys {
    pub foot: Bindings,
    pub drive: Bindings,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Turns the keyboard and mouse takeover on and off.
    pub toggle_key: String,
    /// Programs that show the game. The takeover only happens while one of them is in front.
    pub targets: Vec<String>,
    /// Frames per second of the game on the console (30 for GTA 6 on PS5).
    pub game_fps: f32,
    /// Beep when the layout switches or the takeover turns on or off.
    pub sounds: bool,
    pub aim: AimConfig,
    pub steering: SteeringConfig,
    pub keys: Keys,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            toggle_key: "F10".into(),
            targets: ["veyra_qml_ui.exe", "chiaki-ng.exe", "chiaki.exe", "remoteplay.exe"].map(String::from).to_vec(),
            game_fps: 30.0,
            sounds: true,
            aim: AimConfig::default(),
            steering: SteeringConfig::default(),
            keys: Keys { foot: gta6_foot(), drive: gta6_drive() },
        }
    }
}

fn bindings(list: &[(&str, &[&str])]) -> Bindings {
    list.iter()
        .map(|(key, acts)| {
            let bound = match acts {
                [one] => Bound::One((*one).to_string()),
                many => Bound::Many(many.iter().map(|s| (*s).to_string()).collect()),
            };
            ((*key).to_string(), bound)
        })
        .collect()
}

/// GTA 6 on foot, laid out like GTA V on PC. The PS5 button jobs are from the launch-day
/// control listings and have not been checked in the game here: fix any that are off.
fn gta6_foot() -> Bindings {
    bindings(&[
        ("W", &["ls_up"]),
        ("A", &["ls_left"]),
        ("S", &["ls_down"]),
        ("D", &["ls_right"]),
        ("Space", &["cross"]),                // jump / climb
        ("LShift", &["l3"]),                  // run, again to sprint
        ("LCtrl", &["circle"]),               // crouch
        ("F", &["triangle", "mode_toggle"]),  // get in a vehicle -> driving layout
        ("E", &["triangle"]),                 // interact
        ("R", &["square"]),                   // melee
        ("Q", &["r1"]),                       // cover
        ("G", &["dpad_right"]),               // detonate
        ("Tab", &["l1"]),                     // weapon menu
        ("C", &["r3"]),                       // look behind
        ("V", &["l3", "r3"]),                 // both stick clicks
        ("M", &["touchpad"]),                 // map
        ("P", &["options"]),                  // pause
        ("Esc", &["options"]),
        ("Enter", &["cross"]),                // menus: confirm
        ("Backspace", &["circle"]),           // menus: back
        ("Up", &["dpad_up"]),                 // phone
        ("Down", &["dpad_down"]),
        ("Left", &["dpad_left"]),
        ("Right", &["dpad_right"]),
        ("Z", &["dpad_down"]),                // objectives / minimap
        ("CapsLock", &["mode_toggle"]),
        ("F1", &["ps"]),
        ("F2", &["create"]),
        ("MouseLeft", &["r2"]),               // shoot
        ("MouseRight", &["l2"]),              // aim
        ("MouseMiddle", &["r3"]),
        ("Mouse4", &["l1"]),
        ("Mouse5", &["r1"]),
    ])
}

/// GTA 6 in a vehicle.
fn gta6_drive() -> Bindings {
    bindings(&[
        ("W", &["r2"]),                       // accelerate
        ("S", &["l2"]),                       // brake / reverse
        ("A", &["ls_left"]),
        ("D", &["ls_right"]),
        ("LCtrl", &["ls_down"]),              // lean back (bikes)
        ("X", &["ls_up"]),                    // lean forward (bikes)
        ("Space", &["cross"]),                // handbrake
        ("F", &["triangle", "mode_toggle"]),  // get out -> on-foot layout
        ("E", &["l3"]),                       // horn
        ("LShift", &["l3"]),
        ("H", &["circle"]),                   // headlights
        ("C", &["r3"]),                       // look behind
        ("R", &["square"]),
        ("Q", &["dpad_left"]),                // radio
        ("Tab", &["l1"]),
        ("M", &["touchpad"]),
        ("P", &["options"]),
        ("Esc", &["options"]),
        ("Enter", &["cross"]),
        ("Backspace", &["circle"]),
        ("Up", &["dpad_up"]),
        ("Down", &["dpad_down"]),
        ("Left", &["dpad_left"]),
        ("Right", &["dpad_right"]),
        ("Z", &["dpad_down"]),
        ("CapsLock", &["mode_toggle"]),
        ("F1", &["ps"]),
        ("F2", &["create"]),
        ("MouseLeft", &["r1"]),               // drive-by
        ("MouseRight", &["l1"]),
    ])
}

/// The key layouts in the form the engine uses: for every input, the actions it triggers.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    foot: Vec<Vec<Action>>,
    drive: Vec<Vec<Action>>,
    pub toggle: Input,
}

impl Layout {
    /// Builds the layout and lists every entry in the file that could not be understood.
    pub fn compile(cfg: &Config) -> (Layout, Vec<String>) {
        let mut warnings = Vec::new();
        let toggle = Input::from_name(&cfg.toggle_key).filter(|i| !i.is_mouse()).unwrap_or_else(|| {
            warnings.push(format!("toggle_key \"{}\" is not a key name; using F10", cfg.toggle_key));
            Input(0x79)
        });
        let mut build = |name: &str, src: &Bindings| {
            let mut table = vec![Vec::new(); Input::COUNT];
            for (key, bound) in src {
                let Some(input) = Input::from_name(key) else {
                    warnings.push(format!("{name}: \"{key}\" is not a key or mouse button name"));
                    continue;
                };
                if input == toggle || input == Input::LALT {
                    warnings.push(format!("{name}: {key} is reserved and cannot be bound"));
                    continue;
                }
                for act in bound.names() {
                    match Action::from_name(act) {
                        Some(a) => table[input.index()].push(a),
                        None => warnings.push(format!("{name}: {key} = \"{act}\" is not an action")),
                    }
                }
            }
            table
        };
        let foot = build("on foot", &cfg.keys.foot);
        let drive = build("driving", &cfg.keys.drive);
        (Layout { foot, drive, toggle }, warnings)
    }

    pub fn actions(&self, mode: Mode, input: Input) -> &[Action] {
        let table = match mode {
            Mode::Foot => &self.foot,
            Mode::Drive => &self.drive,
        };
        table.get(input.index()).map(Vec::as_slice).unwrap_or(&[])
    }

    /// True if the input does something in either layout. Those are the inputs taken from Windows.
    pub fn is_bound(&self, input: Input) -> bool {
        !self.actions(Mode::Foot, input).is_empty() || !self.actions(Mode::Drive, input).is_empty()
    }
}

/// `config.toml` next to the program if there is one (portable use), otherwise in AppData.
pub fn config_path() -> PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let beside = exe.with_file_name("config.toml");
        if beside.exists() {
            return beside;
        }
    }
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join(APP_NAME).join("config.toml")
}

/// Reads the config. A missing file gives the defaults. A file that cannot be read gives the
/// defaults plus the error, and the file is left alone until `save` backs it up.
pub fn load(path: &std::path::Path) -> (Config, Option<String>) {
    match std::fs::read_to_string(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Config::default(), None),
        Err(e) => (Config::default(), Some(format!("Could not read {}: {e}", path.display()))),
        Ok(text) => match toml::from_str::<Config>(&text) {
            Ok(mut cfg) => {
                cfg.sanitize();
                (cfg, None)
            }
            Err(e) => (Config::default(), Some(format!("{} has a mistake, using defaults: {e}", path.display()))),
        },
    }
}

/// Writes the config. If the existing file was unreadable it is kept as `config.broken.toml`.
pub fn save(path: &std::path::Path, cfg: &Config, keep_broken: bool) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    if keep_broken && path.exists() {
        let _ = std::fs::copy(path, path.with_file_name("config.broken.toml"));
    }
    let text = toml::to_string_pretty(cfg).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| e.to_string())
}

impl Config {
    /// Pulls hand-edited values back into the ranges the engine can work with.
    pub fn sanitize(&mut self) {
        let a = &mut self.aim;
        a.sensitivity = finite(a.sensitivity, 3.0).clamp(0.1, 30.0);
        a.vertical = finite(a.vertical, 1.0).clamp(0.1, 4.0);
        a.aim_in = finite(a.aim_in, 1.0).clamp(0.1, 4.0);
        a.deadzone = finite(a.deadzone, 0.30).clamp(0.0, 0.9);
        a.curve = finite(a.curve, 1.8).clamp(0.5, 4.0);
        a.smoothing = finite(a.smoothing, 1.0).clamp(0.0, 4.0);
        self.game_fps = finite(self.game_fps, 30.0).clamp(20.0, 240.0);
        for t in &mut self.targets {
            *t = t.trim().to_ascii_lowercase();
        }
        self.targets.retain(|t| !t.is_empty());
    }
}

fn finite(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_compile_without_warnings() {
        let (layout, warnings) = Layout::compile(&Config::default());
        assert!(warnings.is_empty(), "{warnings:?}");
        let w = Input::from_name("W").unwrap();
        assert_eq!(layout.actions(Mode::Foot, w), &[Action::LsUp]);
        assert_eq!(layout.actions(Mode::Drive, w), &[Action::Press(Button::R2)]);
        let f = Input::from_name("F").unwrap();
        assert_eq!(layout.actions(Mode::Foot, f), &[Action::Press(Button::Triangle), Action::ModeToggle]);
        assert!(layout.is_bound(Input::MOUSE_LEFT));
        assert!(!layout.is_bound(Input::WHEEL_UP));
    }

    #[test]
    fn defaults_survive_a_round_trip_through_toml() {
        let cfg = Config::default();
        let text = toml::to_string_pretty(&cfg).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(cfg, back);
    }

    #[test]
    fn a_partial_file_keeps_the_other_defaults() {
        let cfg: Config = toml::from_str("[aim]\nsensitivity = 5.5\n").unwrap();
        assert_eq!(cfg.aim.sensitivity, 5.5);
        assert_eq!(cfg.aim.deadzone, AimConfig::default().deadzone);
        assert_eq!(cfg.keys.foot, gta6_foot());
    }

    #[test]
    fn bad_entries_are_reported_and_skipped() {
        let mut cfg = Config::default();
        cfg.keys.foot.insert("NotAKey".into(), Bound::One("cross".into()));
        cfg.keys.foot.insert("K".into(), Bound::One("not_an_action".into()));
        cfg.keys.foot.insert("F10".into(), Bound::One("cross".into()));
        cfg.keys.foot.insert("LAlt".into(), Bound::One("cross".into()));
        let (layout, warnings) = Layout::compile(&cfg);
        assert_eq!(warnings.len(), 4, "{warnings:?}");
        assert!(!layout.is_bound(Input::from_name("K").unwrap()));
        assert!(!layout.is_bound(Input::LALT));
    }

    #[test]
    fn sanitize_fixes_out_of_range_values() {
        let mut cfg = Config::default();
        cfg.aim.sensitivity = f32::NAN;
        cfg.aim.deadzone = 3.0;
        cfg.game_fps = 0.0;
        cfg.targets = vec!["  Veyra_QML_UI.exe ".into(), "".into()];
        cfg.sanitize();
        assert_eq!(cfg.aim.sensitivity, 3.0);
        assert_eq!(cfg.aim.deadzone, 0.9);
        assert_eq!(cfg.game_fps, 20.0);
        assert_eq!(cfg.targets, vec!["veyra_qml_ui.exe".to_string()]);
    }
}
