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

    /// The name used in the config file.
    pub fn name(self) -> &'static str {
        match self {
            Action::Press(b) => b.name(),
            Action::LsUp => "ls_up",
            Action::LsDown => "ls_down",
            Action::LsLeft => "ls_left",
            Action::LsRight => "ls_right",
            Action::ModeToggle => "mode_toggle",
            Action::ModeFoot => "mode_foot",
            Action::ModeDrive => "mode_drive",
        }
    }

    pub const ALL: [Action; 25] = [
        Action::Press(Button::Cross),
        Action::Press(Button::Circle),
        Action::Press(Button::Square),
        Action::Press(Button::Triangle),
        Action::Press(Button::L1),
        Action::Press(Button::R1),
        Action::Press(Button::L2),
        Action::Press(Button::R2),
        Action::Press(Button::L3),
        Action::Press(Button::R3),
        Action::Press(Button::DpadUp),
        Action::Press(Button::DpadDown),
        Action::Press(Button::DpadLeft),
        Action::Press(Button::DpadRight),
        Action::LsUp,
        Action::LsDown,
        Action::LsLeft,
        Action::LsRight,
        Action::Press(Button::Options),
        Action::Press(Button::Create),
        Action::Press(Button::Touchpad),
        Action::Press(Button::Ps),
        Action::ModeToggle,
        Action::ModeFoot,
        Action::ModeDrive,
    ];

    pub fn switches_layout(self) -> bool {
        matches!(self, Action::ModeToggle | Action::ModeFoot | Action::ModeDrive)
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

/// What each action does in the game, in the game's own words, per layout: `cross = "Jump"`.
/// A key with several actions can have its own entry: `"triangle+mode_toggle" = "Get in"`.
pub type Names = BTreeMap<String, String>;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Labels {
    pub foot: Names,
    pub drive: Names,
}

impl Keys {
    pub fn of(&self, mode: Mode) -> &Bindings {
        match mode {
            Mode::Foot => &self.foot,
            Mode::Drive => &self.drive,
        }
    }

    pub fn of_mut(&mut self, mode: Mode) -> &mut Bindings {
        match mode {
            Mode::Foot => &mut self.foot,
            Mode::Drive => &mut self.drive,
        }
    }
}

impl Labels {
    pub fn of(&self, mode: Mode) -> &Names {
        match mode {
            Mode::Foot => &self.foot,
            Mode::Drive => &self.drive,
        }
    }

    /// The game's word for what these actions do together, or for the first of them that has one.
    pub fn describe(&self, mode: Mode, actions: &[Action]) -> Option<&str> {
        let names = self.of(mode);
        let joined = actions.iter().map(|a| a.name()).collect::<Vec<_>>().join("+");
        names.get(&joined).or_else(|| actions.iter().find_map(|a| names.get(a.name()))).map(String::as_str)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Turns the keyboard and mouse takeover on and off.
    pub toggle_key: String,
    /// Saves a picture of the game while playing. Empty for none.
    pub picture_key: String,
    /// Programs that show the game. The takeover only happens while one of them is in front.
    pub targets: Vec<String>,
    /// Frames per second of the game on the console (30 for GTA 6 on PS5).
    pub game_fps: f32,
    /// Beep when the layout switches or the takeover turns on or off.
    pub sounds: bool,
    pub aim: AimConfig,
    pub steering: SteeringConfig,
    pub keys: Keys,
    pub labels: Labels,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            toggle_key: "F10".into(),
            picture_key: "F9".into(),
            targets: ["veyra_qml_ui.exe", "chiaki-ng.exe", "chiaki.exe", "remoteplay.exe"].map(String::from).to_vec(),
            game_fps: 30.0,
            sounds: true,
            aim: AimConfig::default(),
            steering: SteeringConfig::default(),
            keys: Keys { foot: gta6_foot(), drive: gta6_drive() },
            labels: Labels { foot: gta6_foot_names(), drive: gta6_drive_names() },
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

fn names(list: &[(&str, &str)]) -> Names {
    list.iter().map(|(k, v)| ((*k).to_string(), (*v).to_string())).collect()
}

/// What the buttons do in GTA 6 on foot (default control scheme, from the launch-day listings).
fn gta6_foot_names() -> Names {
    names(&[
        ("ls_up", "Forward"),
        ("ls_down", "Back"),
        ("ls_left", "Left"),
        ("ls_right", "Right"),
        ("cross", "Jump"),
        ("circle", "Crouch"),
        ("square", "Melee"),
        ("triangle", "Interact"),
        ("triangle+mode_toggle", "Get in"),
        ("l1", "Weapons"),
        ("r1", "Cover"),
        ("l2", "Aim"),
        ("r2", "Shoot"),
        ("l3", "Sprint"),
        ("r3", "Behind"),
        ("l3+r3", "Ability"),
        ("options", "Pause"),
        ("touchpad", "Map"),
        ("create", "Capture"),
        ("ps", "PS menu"),
        ("dpad_up", "Phone"),
        ("dpad_down", "Tasks"),
        ("dpad_left", "Radio"),
        ("dpad_right", "Detonate"),
        ("mode_toggle", "Switch"),
    ])
}

/// What the buttons do in GTA 6 in a vehicle.
fn gta6_drive_names() -> Names {
    names(&[
        ("r2", "Gas"),
        ("l2", "Brake"),
        ("ls_left", "Steer L"),
        ("ls_right", "Steer R"),
        ("ls_up", "Lean fwd"),
        ("ls_down", "Lean back"),
        ("cross", "Handbrake"),
        ("circle", "Lights"),
        ("square", "Melee"),
        ("triangle", "Get out"),
        ("triangle+mode_toggle", "Get out"),
        ("l1", "Weapons"),
        ("r1", "Shoot"),
        ("l3", "Horn"),
        ("r3", "Behind"),
        ("l3+r3", "Ability"),
        ("options", "Pause"),
        ("touchpad", "Map"),
        ("create", "Capture"),
        ("ps", "PS menu"),
        ("dpad_up", "Phone"),
        ("dpad_down", "Tasks"),
        ("dpad_left", "Radio"),
        ("dpad_right", "Indicator"),
        ("mode_toggle", "Switch"),
    ])
}

/// The key layouts in the form the engine uses: for every input, the actions it triggers.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    foot: Vec<Vec<Action>>,
    drive: Vec<Vec<Action>>,
    pub toggle: Input,
    pub picture: Option<Input>,
}

impl Layout {
    /// Builds the layout and lists every entry in the file that could not be understood.
    pub fn compile(cfg: &Config) -> (Layout, Vec<String>) {
        let mut warnings = Vec::new();
        let toggle = Input::from_name(&cfg.toggle_key).filter(|i| !i.is_mouse()).unwrap_or_else(|| {
            warnings.push(format!("toggle_key \"{}\" is not a key name; using F10", cfg.toggle_key));
            Input(0x79)
        });
        let picture = match cfg.picture_key.trim() {
            "" => None,
            name => match Input::from_name(name).filter(|i| !i.is_mouse() && *i != toggle && *i != Input::LALT) {
                Some(input) => Some(input),
                None => {
                    warnings.push(format!("picture_key \"{name}\" cannot be used; no picture key is set"));
                    None
                }
            },
        };
        let mut build = |name: &str, src: &Bindings| {
            let mut table = vec![Vec::new(); Input::COUNT];
            for (key, bound) in src {
                let Some(input) = Input::from_name(key) else {
                    warnings.push(format!("{name}: \"{key}\" is not a key or mouse button name"));
                    continue;
                };
                if input == toggle || input == Input::LALT || Some(input) == picture {
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
        (Layout { foot, drive, toggle, picture }, warnings)
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

/// Where pictures of the game are saved: a folder beside the config file.
pub fn pictures_dir() -> PathBuf {
    config_path().with_file_name("pictures")
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
    /// The actions a key has in a layout, as written in the file (unknown names are skipped).
    pub fn actions_of(&self, mode: Mode, input: Input) -> Vec<Action> {
        let found = self.keys.of(mode).iter().find(|(key, _)| Input::from_name(key) == Some(input));
        found.map(|(_, bound)| bound.names().into_iter().filter_map(Action::from_name).collect()).unwrap_or_default()
    }

    /// Sets what a key does in a layout. An empty list unbinds it.
    pub fn bind(&mut self, mode: Mode, input: Input, actions: &[Action]) {
        let table = self.keys.of_mut(mode);
        table.retain(|key, _| Input::from_name(key) != Some(input));
        match actions {
            [] => {}
            [one] => {
                table.insert(input.name(), Bound::One(one.name().to_string()));
            }
            many => {
                table.insert(input.name(), Bound::Many(many.iter().map(|a| a.name().to_string()).collect()));
            }
        }
    }

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
    fn every_action_has_a_unique_name_that_reads_back() {
        for a in Action::ALL {
            assert_eq!(Action::from_name(a.name()), Some(a));
        }
        let mut seen: Vec<&str> = Action::ALL.iter().map(|a| a.name()).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), Action::ALL.len());
    }

    #[test]
    fn default_keys_all_have_a_game_name() {
        let cfg = Config::default();
        for mode in [Mode::Foot, Mode::Drive] {
            for (key, bound) in cfg.keys.of(mode) {
                let actions: Vec<Action> = bound.names().into_iter().filter_map(Action::from_name).collect();
                assert!(cfg.labels.describe(mode, &actions).is_some(), "{mode:?} {key}");
            }
        }
        let f = Input::from_name("F").unwrap();
        assert_eq!(cfg.labels.describe(Mode::Foot, &cfg.actions_of(Mode::Foot, f)), Some("Get in"));
        assert_eq!(cfg.labels.describe(Mode::Foot, &[Action::Press(Button::Triangle)]), Some("Interact"));
    }

    #[test]
    fn binding_a_key_replaces_whatever_spelling_was_there() {
        let mut cfg: Config = toml::from_str("[keys.foot]\nw = \"ls_up\"\n").unwrap();
        let w = Input::from_name("W").unwrap();
        assert_eq!(cfg.actions_of(Mode::Foot, w), vec![Action::LsUp]);
        cfg.bind(Mode::Foot, w, &[Action::Press(Button::Cross), Action::ModeToggle]);
        assert_eq!(cfg.keys.foot.len(), 1, "the lower-case entry is gone");
        assert_eq!(cfg.actions_of(Mode::Foot, w), vec![Action::Press(Button::Cross), Action::ModeToggle]);
        cfg.bind(Mode::Foot, w, &[]);
        assert!(cfg.keys.foot.is_empty());
        let (_, warnings) = Layout::compile(&cfg);
        assert!(warnings.is_empty());
    }

    #[test]
    fn the_picture_key_is_reserved_and_can_be_turned_off() {
        let mut cfg = Config::default();
        let (layout, warnings) = Layout::compile(&cfg);
        assert!(warnings.is_empty());
        assert_eq!(layout.picture, Input::from_name("F9"));
        cfg.keys.foot.insert("F9".into(), Bound::One("cross".into()));
        let (layout, warnings) = Layout::compile(&cfg);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(!layout.is_bound(Input::from_name("F9").unwrap()));
        cfg.picture_key = String::new();
        let (layout, warnings) = Layout::compile(&cfg);
        assert!(warnings.is_empty() && layout.picture.is_none());
        assert!(layout.is_bound(Input::from_name("F9").unwrap()), "with no picture key, F9 is an ordinary key again");
        cfg.picture_key = "F10".into(); // same as the on/off key
        let (layout, warnings) = Layout::compile(&cfg);
        assert!(layout.picture.is_none() && warnings.len() == 1);
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
