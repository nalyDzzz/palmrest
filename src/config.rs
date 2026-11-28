//! Settings and key layouts, stored as a TOML file the user can also edit by hand.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::keys::Input;
use crate::pad::Button;

/// The program's name: also the name of its folder for settings and saved pictures.
pub const APP_NAME: &str = "Palmrest";

/// The number of the starting keys as they are now. It goes up when a key is added to them, and
/// a settings file from before then is given that key when it is read (see `Config::catch_up`).
pub const KEYS_VERSION: u32 = 1;

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

/// The triggers a key eases: one of them, or both together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Triggers {
    L2,
    R2,
    Both,
}

impl Triggers {
    /// Whether this covers L2, and whether it covers R2.
    pub fn covers(self) -> [bool; 2] {
        [self != Triggers::R2, self != Triggers::L2]
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
    /// While held, the keys for these triggers pull them less and less, or (`true`) more and
    /// more: the same as the wheel as their dial, on keys.
    Ease(Triggers, bool),
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
            "l2_less" => Action::Ease(Triggers::L2, false),
            "l2_more" => Action::Ease(Triggers::L2, true),
            "r2_less" => Action::Ease(Triggers::R2, false),
            "r2_more" => Action::Ease(Triggers::R2, true),
            "triggers_less" => Action::Ease(Triggers::Both, false),
            "triggers_more" => Action::Ease(Triggers::Both, true),
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
            Action::Ease(Triggers::L2, false) => "l2_less",
            Action::Ease(Triggers::L2, true) => "l2_more",
            Action::Ease(Triggers::R2, false) => "r2_less",
            Action::Ease(Triggers::R2, true) => "r2_more",
            Action::Ease(Triggers::Both, false) => "triggers_less",
            Action::Ease(Triggers::Both, true) => "triggers_more",
            Action::ModeToggle => "mode_toggle",
            Action::ModeFoot => "mode_foot",
            Action::ModeDrive => "mode_drive",
        }
    }

    pub const ALL: [Action; 31] = [
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
        Action::Ease(Triggers::L2, false),
        Action::Ease(Triggers::L2, true),
        Action::Ease(Triggers::R2, false),
        Action::Ease(Triggers::R2, true),
        Action::Ease(Triggers::Both, false),
        Action::Ease(Triggers::Both, true),
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
            Action::Ease(Triggers::L2, false) => "L2 less",
            Action::Ease(Triggers::L2, true) => "L2 more",
            Action::Ease(Triggers::R2, false) => "R2 less",
            Action::Ease(Triggers::R2, true) => "R2 more",
            Action::Ease(Triggers::Both, false) => "L2 + R2 less",
            Action::Ease(Triggers::Both, true) => "L2 + R2 more",
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

/// How far the keys for L2 and R2 pull them. A key is all or nothing, but some things in the
/// game want a trigger held part of the way, or squeezed slowly. The pull can be set with the
/// mouse wheel (`wheel_dial`) and with keys given the jobs `l2_less`, `l2_more`, `r2_less`,
/// `r2_more`, `triggers_less` and `triggers_more`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TriggerConfig {
    /// The mouse wheel sets how far the trigger keys pull: down for less, up for more. It sets
    /// the triggers whose keys are held, and both when neither is.
    pub wheel_dial: bool,
    /// How much one notch of the wheel changes the pull (0.1: ten steps from nothing to all the way).
    pub step: f32,
    /// The pull goes back to all the way this many seconds after the trigger keys were let go.
    pub reset_after: f32,
    /// A held trigger that is lowered comes down no faster than this: the seconds it takes from
    /// all the way to nothing. However fast the wheel is turned, the trigger moves smoothly,
    /// which is what lowering a weight slowly needs, and a "less" key lowers at this speed
    /// while it is held. 0 for at once.
    pub lower_seconds: f32,
    /// A "more" key raises the pull at this speed while it is held: the seconds from nothing
    /// to all the way. 0 for at once. (The wheel raises at once.)
    pub raise_seconds: f32,
}

impl Default for TriggerConfig {
    fn default() -> Self {
        TriggerConfig { wheel_dial: false, step: 0.1, reset_after: 3.0, lower_seconds: 1.5, raise_seconds: 0.5 }
    }
}

/// How the triggers' pull is set, as the mapper needs it (see `TriggerConfig`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Easing {
    /// The mouse wheel is the triggers' dial.
    pub wheel: bool,
    pub step: f32,
    pub reset_after: f32,
    pub lower_seconds: f32,
    pub raise_seconds: f32,
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OverlayConfig {
    /// Show the layout in use in a small label on top of the game while playing.
    pub enabled: bool,
    /// Also show, in that label, the key for each button prompt the game has on screen.
    pub prompts: bool,
    /// When a prompt holds still, draw its key right on it instead of listing it in the label.
    pub on_prompt: bool,
    /// How many times a second the game picture is searched for prompts. More makes hints
    /// appear sooner and follow a moving prompt more closely, and costs more processor time.
    pub checks_per_second: f32,
}

impl Default for OverlayConfig {
    fn default() -> Self {
        OverlayConfig { enabled: true, prompts: true, on_prompt: true, checks_per_second: 4.0 }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ControllerConfig {
    /// Read a DualSense or DualShock 4 connected to this computer and add what it does to what
    /// the keys and the mouse do, so it can be picked up for the things a keyboard is bad at.
    pub pass_through: bool,
    /// Vibration and adaptive triggers on that controller: while it is in the hands, unplug our
    /// own virtual controller, so the Remote Play program reads the real one itself and can
    /// give it what the console sends back. Coming back to the keys then takes about a second.
    pub full_feedback: bool,
}

impl Default for ControllerConfig {
    fn default() -> Self {
        ControllerConfig { pass_through: true, full_feedback: false }
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
        // The same buttons in another order are the same job: R3 with L3 is L3 with R3.
        let reordered = || names.iter().find(|(name, _)| together(name).is_some_and(|set| set.len() == actions.len() && actions.iter().all(|a| set.contains(a)))).map(|(_, word)| word);
        let whole = names.get(&joined).or_else(|| if actions.len() > 1 { reordered() } else { None });
        whole.or_else(|| actions.iter().find_map(|a| names.get(a.name()))).map(String::as_str)
    }
}

/// The actions a name in the labels stands for when it names several done together: `"l3+r3"`.
fn together(name: &str) -> Option<Vec<Action>> {
    let actions: Option<Vec<Action>> = name.split('+').map(Action::from_name).collect();
    actions.filter(|a| a.len() > 1)
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
    /// Frames per second the game really runs at on the console: 30 for GTA 6 on PS5. The game
    /// reads the stick once per frame, and mouse movement is gathered over one frame. Frames
    /// made up afterwards on the PC (frame generation) do not count. Only in this file: there
    /// is nothing to choose while the game has one frame rate.
    pub game_fps: f32,
    /// Beep when the layout switches or the takeover turns on or off.
    pub sounds: bool,
    /// Switch to the driving layout when the vehicle's name card appears on screen, and not when
    /// the get-in key is pressed. Getting out still switches at once.
    pub switch_on_name_card: bool,
    /// Which starting keys this file has been given (see `KEYS_VERSION`). Palmrest keeps this
    /// itself. A file without it is from before the number existed, hence the 0 for "missing".
    #[serde(default)]
    pub keys_version: u32,
    pub aim: AimConfig,
    pub steering: SteeringConfig,
    pub triggers: TriggerConfig,
    pub overlay: OverlayConfig,
    pub controller: ControllerConfig,
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
            switch_on_name_card: false,
            keys_version: KEYS_VERSION,
            aim: AimConfig::default(),
            steering: SteeringConfig::default(),
            triggers: TriggerConfig::default(),
            overlay: OverlayConfig::default(),
            controller: ControllerConfig::default(),
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
        ("V", &["l3", "r3"]),                 // both stick clicks: the ability works in a vehicle too
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
    /// How the pull of the trigger keys is set.
    pub easing: Easing,
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
        let t = &cfg.triggers;
        let easing = Easing { wheel: t.wheel_dial, step: t.step, reset_after: t.reset_after, lower_seconds: t.lower_seconds, raise_seconds: t.raise_seconds };
        (Layout { foot, drive, toggle, picture, easing }, warnings)
    }

    pub fn actions(&self, mode: Mode, input: Input) -> &[Action] {
        let table = match mode {
            Mode::Foot => &self.foot,
            Mode::Drive => &self.drive,
        };
        table.get(input.index()).map(Vec::as_slice).unwrap_or(&[])
    }

    /// True if the input does something in either layout. Those are the inputs taken from Windows.
    /// Does this key, button or wheel direction do anything? (The wheel does when it is the
    /// dial for the triggers, bound or not.)
    pub fn is_bound(&self, input: Input) -> bool {
        (self.easing.wheel && input.is_wheel()) || !self.actions(Mode::Foot, input).is_empty() || !self.actions(Mode::Drive, input).is_empty()
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
        Ok(text) => match parse(&text) {
            Ok(cfg) => (cfg, None),
            Err(e) => (Config::default(), Some(format!("{} has a mistake, using defaults: {e}", path.display()))),
        },
    }
}

/// The text of a settings file as settings the engine can work with.
fn parse(text: &str) -> Result<Config, toml::de::Error> {
    let mut cfg: Config = toml::from_str(text)?;
    cfg.catch_up();
    cfg.sanitize();
    Ok(cfg)
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

    /// Where the keyboard hand rests: the middle of the keys that move the character on foot
    /// (W, A, S, D unless they were changed), as a place on the keyboard.
    pub fn hand_rest(&self) -> (f32, f32) {
        let places: Vec<(f32, f32)> = [Action::LsUp, Action::LsDown, Action::LsLeft, Action::LsRight]
            .into_iter()
            .flat_map(|action| self.bound_to(Mode::Foot, action))
            .filter_map(|(_, input)| input.span())
            .map(|(left, right, row)| ((left + right) / 2.0, row))
            .collect();
        if places.is_empty() {
            return (3.2, 2.75); // W, A, S, D
        }
        let n = places.len() as f32;
        (places.iter().map(|p| p.0).sum::<f32>() / n, places.iter().map(|p| p.1).sum::<f32>() / n)
    }

    /// The keys bound to this action in a layout, each with whether it also does something else.
    fn bound_to(&self, mode: Mode, action: Action) -> Vec<(bool, Input)> {
        let mut keys = Vec::new();
        for (name, bound) in self.keys.of(mode) {
            let Some(input) = Input::from_name(name) else { continue };
            let actions: Vec<Action> = bound.names().into_iter().filter_map(Action::from_name).collect();
            if actions.contains(&action) {
                keys.push((actions.len() > 1, input));
            }
        }
        keys
    }

    /// The keys that do this action in a layout, the one to show first. Keys that do only this
    /// come before keys that do it along with something else (such as also switching layout);
    /// among those, the key nearest the keyboard hand comes first: with the default keys the
    /// cross button is Space before Enter, whatever the order in the file.
    pub fn keys_for(&self, mode: Mode, action: Action) -> Vec<Input> {
        let home = self.hand_rest();
        let mut keys: Vec<(bool, i32, Input)> = self.bound_to(mode, action).into_iter().map(|(shared, input)| (shared, (input.reach(home) * 100.0) as i32, input)).collect();
        keys.sort();
        keys.into_iter().map(|(_, _, input)| input).collect()
    }

    /// The jobs a key can be given in the window: every button alone, and every set of buttons
    /// with a name of its own in this layout (L3 with R3 is the ability), each set right after
    /// the last of its buttons. Switching layout is not one of them: it can go with any job.
    pub fn jobs(&self, mode: Mode) -> Vec<Vec<Action>> {
        let place = |action: &Action| Action::ALL.iter().position(|a| a == action);
        let sets: Vec<Vec<Action>> = self.labels.of(mode).keys().filter_map(|name| together(name)).filter(|set| !set.iter().any(|a| a.switches_layout())).collect();
        let mut jobs = Vec::new();
        for action in Action::ALL.into_iter().filter(|a| !a.switches_layout()) {
            jobs.push(vec![action]);
            jobs.extend(sets.iter().filter(|set| set.iter().max_by_key(|a| place(a)) == Some(&action)).cloned());
        }
        jobs
    }

    /// Gives settings written by an older version the keys that have been added to the starting
    /// keys since. This happens once: the settings then say which starting keys they have seen,
    /// so a key that is cleared afterwards stays cleared.
    fn catch_up(&mut self) {
        if self.keys_version < 1 {
            // The ability (both stick clicks) can be used in a vehicle too.
            self.add_key(Mode::Drive, "V", &[Action::Press(Button::L3), Action::Press(Button::R3)]);
        }
        self.keys_version = KEYS_VERSION;
    }

    /// Gives a key a job in a layout if the key is free there and no other key there does that job.
    fn add_key(&mut self, mode: Mode, key: &str, job: &[Action]) {
        let Some(input) = Input::from_name(key) else { return };
        let reserved = [&self.toggle_key, &self.picture_key].into_iter().any(|name| Input::from_name(name) == Some(input));
        let taken = self.keys.of(mode).keys().any(|name| Input::from_name(name) == Some(input));
        let done_by_another = self.keys.of(mode).values().any(|bound| {
            let does: Vec<Action> = bound.names().into_iter().filter_map(Action::from_name).collect();
            job.iter().all(|a| does.contains(a))
        });
        if !reserved && !taken && !done_by_another {
            self.bind(mode, input, job);
        }
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
        self.overlay.checks_per_second = finite(self.overlay.checks_per_second, 4.0).clamp(1.0, 30.0);
        self.triggers.step = finite(self.triggers.step, 0.1).clamp(0.02, 0.5);
        self.triggers.reset_after = finite(self.triggers.reset_after, 3.0).clamp(0.5, 60.0);
        self.triggers.lower_seconds = finite(self.triggers.lower_seconds, 1.5).clamp(0.0, 10.0);
        self.triggers.raise_seconds = finite(self.triggers.raise_seconds, 0.5).clamp(0.0, 10.0);
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
        let mut with_dial = Config::default();
        with_dial.triggers.wheel_dial = true;
        assert!(Layout::compile(&with_dial).0.is_bound(Input::WHEEL_UP), "as the triggers' dial the wheel is taken, bound or not");
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
    fn keys_for_an_action_put_the_plain_binding_first() {
        let cfg = Config::default();
        let names = |mode, action| cfg.keys_for(mode, action).into_iter().map(Input::name).collect::<Vec<_>>();
        assert_eq!(names(Mode::Foot, Action::Press(Button::Triangle)), ["E", "F"], "E only interacts; F also switches layout");
        assert_eq!(names(Mode::Drive, Action::Press(Button::Triangle)), ["F"]);
        assert_eq!(names(Mode::Foot, Action::LsUp), ["W"]);
        assert!(names(Mode::Drive, Action::LsUp).contains(&"X".to_string()));
    }

    #[test]
    fn keys_for_an_action_put_the_key_nearest_the_hand_first() {
        let mut cfg = Config::default();
        let names = |cfg: &Config, mode, action| cfg.keys_for(mode, action).into_iter().map(Input::name).collect::<Vec<_>>();
        assert_eq!(names(&cfg, Mode::Foot, Action::Press(Button::Cross)), ["Space", "Enter"], "Space is under the thumb, Enter is across the keyboard");
        assert_eq!(names(&cfg, Mode::Drive, Action::Press(Button::Cross)), ["Space", "Enter"]);
        assert_eq!(names(&cfg, Mode::Foot, Action::Press(Button::Circle)), ["LCtrl", "Backspace"]);
        assert_eq!(names(&cfg, Mode::Foot, Action::Press(Button::L1)), ["Tab", "Mouse4"], "a key by the hand before a side button not every mouse has");
        assert_eq!(names(&cfg, Mode::Foot, Action::Press(Button::R3)), ["C", "MouseMiddle", "V"], "V clicks both sticks, so it comes last");
        assert_eq!(names(&cfg, Mode::Drive, Action::Press(Button::L1)), ["MouseRight", "Tab"], "the main mouse buttons are always at hand");
        // Someone who moves with the arrow keys has the hand at the other end: now Enter is the near one.
        for (old, new, action) in [("W", "Up", Action::LsUp), ("S", "Down", Action::LsDown), ("A", "Left", Action::LsLeft), ("D", "Right", Action::LsRight)] {
            cfg.bind(Mode::Foot, Input::from_name(old).unwrap(), &[]);
            cfg.bind(Mode::Foot, Input::from_name(new).unwrap(), &[action]);
        }
        assert_eq!(names(&cfg, Mode::Foot, Action::Press(Button::Cross)), ["Enter", "Space"]);
        assert_eq!(names(&cfg, Mode::Drive, Action::Press(Button::Cross)), ["Enter", "Space"], "the hand stays where it is when driving");
    }

    fn ability() -> Vec<Action> {
        vec![Action::Press(Button::L3), Action::Press(Button::R3)]
    }

    #[test]
    fn the_ability_has_a_key_in_both_layouts() {
        let cfg = Config::default();
        let v = Input::from_name("V").unwrap();
        for mode in [Mode::Foot, Mode::Drive] {
            assert_eq!(cfg.actions_of(mode, v), ability(), "{mode:?}");
            assert_eq!(cfg.labels.describe(mode, &cfg.actions_of(mode, v)), Some("Ability"), "{mode:?}");
        }
    }

    #[test]
    fn buttons_named_together_are_a_job_of_their_own() {
        let mut cfg = Config::default();
        for mode in [Mode::Foot, Mode::Drive] {
            let jobs = cfg.jobs(mode);
            let at = jobs.iter().position(|job| *job == ability()).expect("the ability is a job");
            assert_eq!(jobs[at - 1], [Action::Press(Button::R3)], "listed right after R3, the last of its buttons");
            assert_eq!(jobs.iter().filter(|job| job.len() == 1).count(), Action::ALL.len() - 3, "every button alone, without the three that switch layout");
            assert!(jobs.iter().flatten().all(|a| !a.switches_layout()), "getting in is a job with the switch added, not one to pick");
        }
        // Clicked together in the other order, it is still the ability.
        assert_eq!(cfg.labels.describe(Mode::Drive, &[Action::Press(Button::R3), Action::Press(Button::L3)]), Some("Ability"));
        assert_eq!(cfg.labels.describe(Mode::Drive, &[Action::Press(Button::L3)]), Some("Horn"));
        // A set someone names in the file is offered too.
        cfg.labels.drive.insert("l1+r1".into(), "Duck".into());
        assert!(cfg.jobs(Mode::Drive).contains(&vec![Action::Press(Button::L1), Action::Press(Button::R1)]));
        assert!(!cfg.jobs(Mode::Foot).contains(&vec![Action::Press(Button::L1), Action::Press(Button::R1)]));
    }

    #[test]
    fn older_settings_get_the_ability_key_for_driving_once_and_only_if_v_is_free() {
        let v = Input::from_name("V").unwrap();
        // As the first version wrote them: no key for the ability while driving, no version number.
        let as_written_then = |cfg: &Config| {
            let text = toml::to_string_pretty(cfg).unwrap().replace(&format!("keys_version = {KEYS_VERSION}\n"), "");
            assert!(!text.contains("keys_version"));
            text
        };
        let mut old = Config::default();
        old.bind(Mode::Drive, v, &[]);
        let cfg = parse(&as_written_then(&old)).unwrap();
        assert_eq!(cfg.actions_of(Mode::Drive, v), ability());
        assert_eq!(cfg.keys_version, KEYS_VERSION);
        assert!(Layout::compile(&cfg).1.is_empty());
        // Cleared after that, it stays cleared.
        let mut cleared = cfg.clone();
        cleared.bind(Mode::Drive, v, &[]);
        assert!(parse(&toml::to_string_pretty(&cleared).unwrap()).unwrap().actions_of(Mode::Drive, v).is_empty());
        // A V that was given another job keeps it...
        old.bind(Mode::Drive, v, &[Action::Press(Button::Circle)]);
        assert_eq!(parse(&as_written_then(&old)).unwrap().actions_of(Mode::Drive, v), [Action::Press(Button::Circle)]);
        // ...and nothing is added where another key is the ability already, or V is the on/off key.
        old.bind(Mode::Drive, v, &[]);
        old.bind(Mode::Drive, Input::from_name("B").unwrap(), &ability());
        assert!(parse(&as_written_then(&old)).unwrap().actions_of(Mode::Drive, v).is_empty());
        old.bind(Mode::Drive, Input::from_name("B").unwrap(), &[]);
        old.toggle_key = "V".into();
        assert!(parse(&as_written_then(&old)).unwrap().actions_of(Mode::Drive, v).is_empty());
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
        cfg.overlay.checks_per_second = 500.0;
        cfg.sanitize();
        assert_eq!(cfg.overlay.checks_per_second, 30.0);
    }
}
