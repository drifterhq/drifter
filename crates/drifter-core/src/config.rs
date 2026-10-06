use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::geom::Vec2;
use crate::shortcut::{Action, Combo};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WallpaperMode {
    Fixed,
    Pan,
    #[default]
    Parallax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Modifier {
    #[default]
    Super,
    Alt,
    Ctrl,
}

impl Modifier {
    pub fn keys(self) -> [u32; 2] {
        match self {
            Modifier::Super => [133, 134],
            Modifier::Alt => [64, 108],
            Modifier::Ctrl => [37, 105],
        }
    }

    pub fn holds(self, key: u32) -> bool {
        self.keys().contains(&key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    #[serde(default = "default_screens")]
    pub width: u32,
    #[serde(default = "default_screens")]
    pub height: u32,
    #[serde(default)]
    pub wallpaper: Option<PathBuf>,
    #[serde(default)]
    pub wallpaper_mode: WallpaperMode,
    #[serde(default)]
    pub wallpaper_folder: Option<PathBuf>,
    #[serde(default)]
    pub wallpaper_every: u64,
    #[serde(default)]
    pub wallpaper_order: WallpaperOrder,
    #[serde(default = "default_motion")]
    pub wallpaper_motion: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WallpaperOrder {
    #[default]
    Random,
    Ordered,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Launch {
    App(String),
    Full {
        #[serde(default)]
        app: Option<String>,
        #[serde(default)]
        command: Option<String>,
        #[serde(default)]
        icon: Option<PathBuf>,
    },
}

impl Launch {
    pub fn app(&self) -> Option<&str> {
        match self {
            Launch::App(app) => Some(app),
            Launch::Full { app, .. } => app.as_deref(),
        }
    }

    pub fn command(&self) -> Option<&str> {
        match self {
            Launch::App(_) => None,
            Launch::Full { command, .. } => command.as_deref(),
        }
    }

    pub fn icon(&self) -> Option<&Path> {
        match self {
            Launch::App(_) => None,
            Launch::Full { icon, .. } => icon.as_deref(),
        }
    }

    pub fn name(&self) -> String {
        match (self.app(), self.command()) {
            (Some(app), _) => app.to_string(),
            (None, Some(crate::launchpad::DOCK_COMMAND)) => "Apps".to_string(),
            (None, Some(command)) if crate::place::from_dock(command).is_some() => {
                crate::place::from_dock(command).unwrap_or_default().to_string()
            }
            (None, Some(command)) => crate::exec::words(command)
                .and_then(|words| words.into_iter().next())
                .unwrap_or_default(),
            (None, None) => String::new(),
        }
    }

    fn check(&self) -> Result<(), String> {
        match (self.app(), self.command()) {
            (Some(app), None) if !app.trim().is_empty() => Ok(()),
            (None, Some(command)) if !command.trim().is_empty() => Ok(()),
            _ => Err("each item names one app or one command".to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    #[default]
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ButtonName {
    Close,
    Minimize,
    Maximize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WindowConfig {
    #[serde(default)]
    pub buttons: Side,
    #[serde(default = "default_order")]
    pub order: Vec<ButtonName>,
    #[serde(default = "yes")]
    pub title: bool,
}

fn default_order() -> Vec<ButtonName> {
    vec![
        ButtonName::Close,
        ButtonName::Minimize,
        ButtonName::Maximize,
    ]
}

fn yes() -> bool {
    true
}

impl Default for WindowConfig {
    fn default() -> Self {
        WindowConfig {
            buttons: Side::default(),
            order: default_order(),
            title: true,
        }
    }
}

impl ButtonName {
    pub fn word(self) -> &'static str {
        match self {
            ButtonName::Close => "close",
            ButtonName::Minimize => "minimize",
            ButtonName::Maximize => "maximize",
        }
    }
}

impl WindowConfig {
    pub fn gtk_layout(&self) -> String {
        let names: Vec<&str> = self.order.iter().map(|name| name.word()).collect();
        let names = names.join(",");
        match self.buttons {
            Side::Left => format!("{names}:"),
            Side::Right => format!(":{names}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LockConfig {
    #[serde(default)]
    pub after: u64,
    #[serde(default = "ten_minutes")]
    pub screen_off: u64,
}

fn ten_minutes() -> u64 {
    10
}

impl Default for LockConfig {
    fn default() -> Self {
        LockConfig {
            after: 0,
            screen_off: ten_minutes(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct AppsConfig {
    #[serde(default)]
    pub files: Option<Launch>,
    #[serde(default)]
    pub terminal: Option<Launch>,
    #[serde(default)]
    pub browser: Option<Launch>,
}

impl AppsConfig {
    pub fn get(&self, role: crate::desktop::Role) -> Option<&Launch> {
        match role {
            crate::desktop::Role::Files => self.files.as_ref(),
            crate::desktop::Role::Terminal => self.terminal.as_ref(),
            crate::desktop::Role::Browser => self.browser.as_ref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginConfig {
    #[serde(default)]
    pub start: Vec<Launch>,
    #[serde(default = "yes")]
    pub autostart: bool,
}

impl Default for LoginConfig {
    fn default() -> Self {
        LoginConfig {
            start: Vec::new(),
            autostart: true,
        }
    }
}

fn default_screens() -> u32 {
    9
}

fn default_motion() -> u32 {
    100
}

impl Default for Workspace {
    fn default() -> Self {
        Workspace {
            width: default_screens(),
            height: default_screens(),
            wallpaper: None,
            wallpaper_mode: WallpaperMode::default(),
            wallpaper_folder: None,
            wallpaper_every: 0,
            wallpaper_order: WallpaperOrder::default(),
            wallpaper_motion: default_motion(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Assets {
    #[serde(default)]
    pub cursor: Option<PathBuf>,
    #[serde(default)]
    pub font: Option<PathBuf>,
    #[serde(default)]
    pub window: Option<PathBuf>,
    #[serde(default)]
    pub dock: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DockConfig {
    #[serde(default)]
    pub pinned: Vec<Launch>,
    #[serde(default = "yes")]
    pub files: bool,
    #[serde(default = "yes")]
    pub terminal: bool,
    #[serde(default = "yes")]
    pub browser: bool,
    #[serde(default = "yes")]
    pub settings: bool,
    #[serde(default = "yes")]
    pub trash: bool,
    #[serde(default = "yes")]
    pub apps: bool,
}

impl Default for DockConfig {
    fn default() -> Self {
        DockConfig {
            pinned: Vec::new(),
            files: true,
            terminal: true,
            browser: true,
            settings: true,
            trash: true,
            apps: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drawers {
    #[serde(default = "default_top_height")]
    pub top: f64,
    #[serde(default = "default_bottom_height")]
    pub bottom: f64,
}

fn default_top_height() -> f64 {
    36.0
}

fn default_bottom_height() -> f64 {
    64.0
}

impl Default for Drawers {
    fn default() -> Self {
        Drawers {
            top: default_top_height(),
            bottom: default_bottom_height(),
        }
    }
}

impl Drawers {
    pub fn height(&self, edge: crate::drawer::Edge) -> f64 {
        match edge {
            crate::drawer::Edge::Top => self.top,
            crate::drawer::Edge::Bottom => self.bottom,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrayConfig {
    #[serde(default = "default_tray_format")]
    pub format: String,
    #[serde(default = "default_tray_size")]
    pub size: f64,
    #[serde(default = "default_stats")]
    pub stats: bool,
    #[serde(default = "default_metrics")]
    pub metrics: Vec<String>,
}

fn default_stats() -> bool {
    true
}

fn default_metrics() -> Vec<String> {
    ["cpu", "ram", "/", "battery"].map(String::from).to_vec()
}

fn default_tray_size() -> f64 {
    13.0
}

fn default_tray_format() -> String {
    crate::tray::DEFAULT_FORMAT.to_string()
}

impl Default for TrayConfig {
    fn default() -> Self {
        TrayConfig {
            format: default_tray_format(),
            size: default_tray_size(),
            stats: default_stats(),
            metrics: default_metrics(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Input {
    #[serde(default)]
    pub modifier: Modifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ThreeTap {
    #[default]
    Overview,
    Middle,
}

pub const FASTEST: u32 = 10;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Night {
    #[serde(default)]
    pub on: bool,
    #[serde(default = "night_warmth")]
    pub warmth: u32,
    #[serde(default = "night_start")]
    pub start: u32,
    #[serde(default = "night_end")]
    pub end: u32,
}

fn night_warmth() -> u32 {
    4000
}

fn night_start() -> u32 {
    21
}

fn night_end() -> u32 {
    7
}

impl Default for Night {
    fn default() -> Self {
        Night {
            on: false,
            warmth: night_warmth(),
            start: night_start(),
            end: night_end(),
        }
    }
}

pub const REPEAT_DELAY: (u32, u32) = (100, 1000);
pub const REPEAT_RATE: (u32, u32) = (5, 60);

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keyboard {
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub variant: String,
    #[serde(default)]
    pub options: String,
    #[serde(default = "repeat_delay")]
    pub repeat_delay: u32,
    #[serde(default = "repeat_rate")]
    pub repeat_rate: u32,
}

fn repeat_delay() -> u32 {
    200
}

fn repeat_rate() -> u32 {
    25
}

impl Default for Keyboard {
    fn default() -> Self {
        Keyboard {
            layout: String::new(),
            variant: String::new(),
            options: String::new(),
            repeat_delay: repeat_delay(),
            repeat_rate: repeat_rate(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Touchpad {
    #[serde(default = "yes")]
    pub tap: bool,
    #[serde(default)]
    pub three_tap: ThreeTap,
    #[serde(default = "yes")]
    pub swipe: bool,
    #[serde(default)]
    pub natural: bool,
    #[serde(default = "middle_speed")]
    pub speed: u32,
    #[serde(default = "yes")]
    pub accel: bool,
    #[serde(default = "yes")]
    pub ignore_while_typing: bool,
}

fn middle_speed() -> u32 {
    FASTEST / 2
}

pub fn libinput_speed(speed: u32) -> f64 {
    let half = f64::from(FASTEST) / 2.0;
    ((f64::from(speed.min(FASTEST)) - half) / half).clamp(-1.0, 1.0)
}

impl Touchpad {
    pub fn libinput_speed(&self) -> f64 {
        libinput_speed(self.speed)
    }
}

impl Default for Touchpad {
    fn default() -> Self {
        Touchpad {
            tap: true,
            three_tap: ThreeTap::default(),
            swipe: true,
            natural: false,
            speed: middle_speed(),
            accel: true,
            ignore_while_typing: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Mouse {
    #[serde(default = "middle_speed")]
    pub speed: u32,
    #[serde(default)]
    pub accel: bool,
    #[serde(default)]
    pub natural: bool,
    #[serde(default)]
    pub left_handed: bool,
}

impl Mouse {
    pub fn libinput_speed(&self) -> f64 {
        libinput_speed(self.speed)
    }
}

impl Default for Mouse {
    fn default() -> Self {
        Mouse {
            speed: middle_speed(),
            accel: false,
            natural: false,
            left_handed: false,
        }
    }
}

pub const CORNER_SIZE: (u32, u32) = (8, 160);
pub const PAN_SPEED: (u32, u32) = (25, 400);
pub const WAIT_MS: (u32, u32) = (0, 2000);
pub const SLIDE_MS: (u32, u32) = (40, 1500);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Motion {
    #[serde(default = "yes")]
    pub hot_corner: bool,
    #[serde(default = "corner_size")]
    pub corner_size: u32,
    #[serde(default = "corner_wait")]
    pub corner_wait: u32,
    #[serde(default = "pan_speed")]
    pub pan_speed: u32,
    #[serde(default = "bar_open")]
    pub bar_open: u32,
    #[serde(default = "bar_close")]
    pub bar_close: u32,
    #[serde(default = "bar_slide")]
    pub bar_slide: u32,
    #[serde(default = "yes")]
    pub hide_top: bool,
    #[serde(default = "yes")]
    pub hide_bottom: bool,
    #[serde(default = "flight")]
    pub flight: u32,
}

fn corner_size() -> u32 {
    48
}

fn corner_wait() -> u32 {
    300
}

fn pan_speed() -> u32 {
    100
}

fn bar_open() -> u32 {
    120
}

fn bar_close() -> u32 {
    400
}

fn bar_slide() -> u32 {
    300
}

fn flight() -> u32 {
    260
}

impl Default for Motion {
    fn default() -> Self {
        Motion {
            hot_corner: true,
            corner_size: corner_size(),
            corner_wait: corner_wait(),
            pan_speed: pan_speed(),
            bar_open: bar_open(),
            bar_close: bar_close(),
            bar_slide: bar_slide(),
            hide_top: true,
            hide_bottom: true,
            flight: flight(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    Light,
    Dark,
    Auto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Accent {
    Blue,
    Teal,
    Green,
    Yellow,
    Orange,
    Red,
    Pink,
    Purple,
    Slate,
}

impl Accent {
    pub const ALL: [Accent; 9] = [
        Accent::Blue,
        Accent::Teal,
        Accent::Green,
        Accent::Yellow,
        Accent::Orange,
        Accent::Red,
        Accent::Pink,
        Accent::Purple,
        Accent::Slate,
    ];

    pub fn word(self) -> &'static str {
        match self {
            Accent::Blue => "blue",
            Accent::Teal => "teal",
            Accent::Green => "green",
            Accent::Yellow => "yellow",
            Accent::Orange => "orange",
            Accent::Red => "red",
            Accent::Pink => "pink",
            Accent::Purple => "purple",
            Accent::Slate => "slate",
        }
    }

    pub fn color(self) -> [u8; 3] {
        match self {
            Accent::Blue => [53, 132, 228],
            Accent::Teal => [33, 144, 164],
            Accent::Green => [58, 148, 74],
            Accent::Yellow => [200, 136, 0],
            Accent::Orange => [237, 91, 0],
            Accent::Red => [230, 45, 66],
            Accent::Pink => [213, 97, 153],
            Accent::Purple => [145, 65, 172],
            Accent::Slate => [111, 131, 150],
        }
    }
}

pub const CURSOR_SIZES: [u32; 4] = [24, 32, 48, 64];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Appearance {
    #[serde(default)]
    pub style: Option<Style>,
    #[serde(default)]
    pub accent: Option<Accent>,
    #[serde(default)]
    pub icons: Option<String>,
    #[serde(default)]
    pub cursor_theme: Option<String>,
    #[serde(default = "cursor_size")]
    pub cursor_size: u32,
}

fn cursor_size() -> u32 {
    24
}

impl Default for Appearance {
    fn default() -> Self {
        Appearance {
            style: None,
            accent: None,
            icons: None,
            cursor_theme: None,
            cursor_size: cursor_size(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Lid {
    #[default]
    Suspend,
    Lock,
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum PowerKey {
    #[default]
    Menu,
    Suspend,
    Off,
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Power {
    #[serde(default)]
    pub sleep_after: u64,
    #[serde(default)]
    pub lid: Lid,
    #[serde(default)]
    pub button: PowerKey,
}

pub const LONGEST_NOTE: u32 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notify {
    #[serde(default)]
    pub quiet: bool,
    #[serde(default = "note_seconds")]
    pub seconds: u32,
    #[serde(default = "yes")]
    pub history: bool,
}

fn note_seconds() -> u32 {
    5
}

impl Default for Notify {
    fn default() -> Self {
        Notify {
            quiet: false,
            seconds: note_seconds(),
            history: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Keys {
    #[serde(default = "key_switch")]
    pub switch: String,
    #[serde(default = "key_close")]
    pub close: String,
    #[serde(default = "key_screenshot")]
    pub screenshot: String,
    #[serde(default = "key_lock")]
    pub lock: String,
    #[serde(default = "key_power")]
    pub power: String,
    #[serde(default)]
    pub overview: String,
    #[serde(default)]
    pub terminal: String,
    #[serde(default)]
    pub files: String,
    #[serde(default)]
    pub browser: String,
    #[serde(default)]
    pub settings: String,
    #[serde(default = "key_search")]
    pub search: String,
    #[serde(default = "key_shot_screen")]
    pub shot_screen: String,
    #[serde(default = "key_shot_part")]
    pub shot_part: String,
    #[serde(default = "key_shot_bar")]
    pub shot_bar: String,
}

fn key_shot_screen() -> String {
    Action::ShotScreen.default_combo().into()
}

fn key_shot_part() -> String {
    Action::ShotPart.default_combo().into()
}

fn key_shot_bar() -> String {
    Action::ShotBar.default_combo().into()
}

fn key_search() -> String {
    Action::Search.default_combo().into()
}

fn key_switch() -> String {
    Action::Switch.default_combo().into()
}

fn key_close() -> String {
    Action::Close.default_combo().into()
}

fn key_screenshot() -> String {
    Action::Screenshot.default_combo().into()
}

fn key_lock() -> String {
    Action::Lock.default_combo().into()
}

fn key_power() -> String {
    Action::Power.default_combo().into()
}

impl Default for Keys {
    fn default() -> Self {
        let mut keys = Keys {
            switch: String::new(),
            close: String::new(),
            screenshot: String::new(),
            lock: String::new(),
            power: String::new(),
            overview: String::new(),
            terminal: String::new(),
            files: String::new(),
            browser: String::new(),
            settings: String::new(),
            search: String::new(),
            shot_screen: String::new(),
            shot_part: String::new(),
            shot_bar: String::new(),
        };
        for action in Action::ALL {
            *keys.get_mut(action) = action.default_combo().to_string();
        }
        keys
    }
}

impl Keys {
    pub fn get(&self, action: Action) -> &str {
        match action {
            Action::Switch => &self.switch,
            Action::Close => &self.close,
            Action::Screenshot => &self.screenshot,
            Action::Lock => &self.lock,
            Action::Power => &self.power,
            Action::Overview => &self.overview,
            Action::Terminal => &self.terminal,
            Action::Files => &self.files,
            Action::Browser => &self.browser,
            Action::Settings => &self.settings,
            Action::Search => &self.search,
            Action::ShotScreen => &self.shot_screen,
            Action::ShotPart => &self.shot_part,
            Action::ShotBar => &self.shot_bar,
        }
    }

    pub fn get_mut(&mut self, action: Action) -> &mut String {
        match action {
            Action::Switch => &mut self.switch,
            Action::Close => &mut self.close,
            Action::Screenshot => &mut self.screenshot,
            Action::Lock => &mut self.lock,
            Action::Power => &mut self.power,
            Action::Overview => &mut self.overview,
            Action::Terminal => &mut self.terminal,
            Action::Files => &mut self.files,
            Action::Browser => &mut self.browser,
            Action::Settings => &mut self.settings,
            Action::Search => &mut self.search,
            Action::ShotScreen => &mut self.shot_screen,
            Action::ShotPart => &mut self.shot_part,
            Action::ShotBar => &mut self.shot_bar,
        }
    }

    pub fn combos(&self) -> Vec<(Action, Combo)> {
        Action::ALL
            .iter()
            .filter_map(|action| {
                Combo::parse(self.get(*action))
                    .ok()
                    .flatten()
                    .map(|combo| (*action, combo))
            })
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub workspace: Workspace,
    #[serde(default)]
    pub input: Input,
    #[serde(default)]
    pub touchpad: Touchpad,
    #[serde(default)]
    pub mouse: Mouse,
    #[serde(default)]
    pub motion: Motion,
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub power: Power,
    #[serde(default)]
    pub notify: Notify,
    #[serde(default)]
    pub keys: Keys,
    #[serde(default)]
    pub keyboard: Keyboard,
    #[serde(default)]
    pub night: Night,
    #[serde(default)]
    pub drawers: Drawers,
    #[serde(default)]
    pub tray: TrayConfig,
    #[serde(default)]
    pub dock: DockConfig,
    #[serde(default)]
    pub window: WindowConfig,
    #[serde(default)]
    pub lock: LockConfig,
    #[serde(default)]
    pub login: LoginConfig,
    #[serde(default)]
    pub apps: AppsConfig,
    #[serde(default)]
    pub assets: Assets,
    #[serde(default)]
    pub monitor: Vec<crate::screens::MonitorConfig>,
    #[serde(default)]
    pub place: Vec<crate::place::Place>,
    #[serde(default)]
    pub access: Access,
    #[serde(default)]
    pub screenshots: Shots,
    #[serde(skip)]
    base: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Access {
    #[serde(default)]
    pub large_text: bool,
    #[serde(default)]
    pub high_contrast: bool,
    #[serde(default)]
    pub reduce_motion: bool,
    #[serde(default)]
    pub sticky_keys: bool,
}

pub const LARGE_TEXT: f64 = 1.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ShotFormat {
    #[default]
    Png,
    Jpeg,
}

impl ShotFormat {
    pub fn extension(self) -> &'static str {
        match self {
            ShotFormat::Png => "png",
            ShotFormat::Jpeg => "jpg",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Shots {
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default)]
    pub format: ShotFormat,
}

impl Shots {
    pub fn folder_in(&self, home: &Path) -> PathBuf {
        match self.folder.as_deref().map(str::trim) {
            Some("") | None => home.join("Pictures/Screenshots"),
            Some(folder) => match folder.strip_prefix("~/") {
                Some(rest) => home.join(rest),
                None if folder == "~" => home.to_path_buf(),
                None => PathBuf::from(folder),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tunables {
    pub hot_corner: bool,
    pub drawer_height: f64,
    pub drawer_dwell: Duration,
    pub drawer_hide_delay: Duration,
    pub drawer_anim: Duration,
    pub bar_hides: [bool; 2],
    pub edge_band: f64,
    pub hot_corner_dwell: Duration,
    pub hot_corner_size: f64,
    pub pan_scale: f64,
    pub camera_anim: Duration,
}

impl Default for Tunables {
    fn default() -> Self {
        Tunables {
            hot_corner: true,
            drawer_height: 64.0,
            drawer_dwell: Duration::from_millis(120),
            drawer_hide_delay: Duration::from_millis(400),
            drawer_anim: Duration::from_millis(300),
            bar_hides: [true, true],
            edge_band: 2.0,
            hot_corner_dwell: Duration::from_millis(300),
            hot_corner_size: 48.0,
            pan_scale: 1.0,
            camera_anim: Duration::from_millis(260),
        }
    }
}

impl Tunables {
    pub fn still(self) -> Tunables {
        Tunables {
            drawer_anim: Duration::ZERO,
            camera_anim: Duration::ZERO,
            ..self
        }
    }

    pub fn from(motion: &Motion) -> Tunables {
        let ms = |n: u32| Duration::from_millis(u64::from(n));
        Tunables {
            hot_corner: motion.hot_corner,
            hot_corner_size: f64::from(motion.corner_size),
            hot_corner_dwell: ms(motion.corner_wait),
            pan_scale: f64::from(motion.pan_speed) / 100.0,
            drawer_dwell: ms(motion.bar_open),
            drawer_hide_delay: ms(motion.bar_close),
            drawer_anim: ms(motion.bar_slide),
            bar_hides: [motion.hide_top, motion.hide_bottom],
            camera_anim: ms(motion.flight),
            ..Tunables::default()
        }
    }
}

pub const DEFAULTS: &str = "default.toml";
pub const SHARED: &str = "/usr/share/drifter/conf";
pub const FALLBACK_WALLPAPER: &str = "aurora.shader";
pub const MOST_MOTION: u32 = 300;
pub const LONGEST_LOCK: u64 = 7 * 24 * 60;
pub const LONGEST_ROTATION: u64 = 366 * 24 * 3600;

pub fn choose_config(
    candidates: Vec<PathBuf>,
    xdg: PathBuf,
    exists: impl Fn(&Path) -> bool,
) -> PathBuf {
    candidates
        .into_iter()
        .find(|path| exists(path))
        .unwrap_or(xdg)
}

pub fn defaults_for(path: &Path, shared: &Path, exists: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    [path.with_file_name(DEFAULTS), shared.join(DEFAULTS)]
        .into_iter()
        .find(|candidate| exists(candidate))
}

pub fn resolve_picture(
    asked: &Path,
    livepapers: &[PathBuf],
    beside: impl Fn(&Path) -> PathBuf,
) -> PathBuf {
    if !crate::livepaper::is_livepaper(asked) || asked.components().count() != 1 {
        return beside(asked);
    }
    if let Some(found) = crate::livepaper::find(&asked.to_string_lossy(), livepapers) {
        return found;
    }
    let near = beside(asked);
    match near.exists() {
        true => near,
        false => crate::livepaper::find(FALLBACK_WALLPAPER, livepapers).unwrap_or(near),
    }
}

pub fn resolve_beside(
    base: &Path,
    shared: &Path,
    path: &Path,
    exists: impl Fn(&Path) -> bool,
) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    let near = base.join(path);
    if exists(&near) {
        return near;
    }
    let packaged = shared.join(path);
    if exists(&packaged) {
        return packaged;
    }
    near
}

pub fn repo_config(exe: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut at = exe.parent();
    for _ in 0..4 {
        let Some(dir) = at else {
            break;
        };
        candidates.push(dir.join("conf").join("drifter.toml"));
        at = dir.parent();
    }
    candidates
}

pub(crate) fn xdg_dir(var: &str, fallback: &str) -> PathBuf {
    resolve_xdg(std::env::var_os(var), std::env::var_os("HOME"), fallback)
}

fn resolve_xdg(
    xdg: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
    fallback: &str,
) -> PathBuf {
    let base = xdg
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home.map(|h| PathBuf::from(h).join(fallback)))
        .unwrap_or_else(|| PathBuf::from(fallback));
    base.join("drifter")
}

#[derive(Debug)]
pub enum ConfigError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        message: String,
    },
    Invalid(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Read { path, source } => {
                write!(f, "cannot read {}: {source}", path.display())
            }
            ConfigError::Parse { path, message } => {
                write!(f, "cannot parse {}: {message}", path.display())
            }
            ConfigError::Invalid(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn config_path() -> PathBuf {
        let running = std::env::current_exe().ok();
        let candidates = running.as_deref().map(repo_config).unwrap_or_default();
        let xdg = xdg_dir("XDG_CONFIG_HOME", ".config").join("drifter.toml");
        choose_config(candidates, xdg, |path| {
            path.is_file() || path.with_file_name(DEFAULTS).is_file()
        })
    }

    pub fn defaults(path: &Path) -> Option<PathBuf> {
        defaults_for(path, Path::new(SHARED), |candidate| candidate.is_file())
    }

    pub fn first_run(path: &Path) -> std::io::Result<bool> {
        Config::first_run_from(path, Path::new(SHARED))
    }

    pub fn first_run_from(path: &Path, shared: &Path) -> std::io::Result<bool> {
        if path.exists() {
            return Ok(false);
        }
        let Some(defaults) = defaults_for(path, shared, |candidate| candidate.is_file()) else {
            return Ok(false);
        };
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::copy(&defaults, path)?;
        Ok(true)
    }

    pub fn data_dir() -> PathBuf {
        xdg_dir("XDG_DATA_HOME", ".local/share")
    }

    pub fn beside(&self, path: &Path) -> PathBuf {
        resolve_beside(&self.base, Path::new(SHARED), path, |candidate| {
            candidate.exists()
        })
    }

    pub fn wallpaper_path(&self) -> Option<PathBuf> {
        let asked = self.workspace.wallpaper.as_deref()?;
        Some(resolve_picture(asked, &self.livepaper_folders(), |path| {
            self.beside(path)
        }))
    }

    pub fn livepaper_folders(&self) -> Vec<PathBuf> {
        let mut folders = crate::livepaper::folders(&Self::data_dir());
        folders.insert(1, self.beside(Path::new("../livepapers")));
        folders
    }

    pub fn state_dir() -> PathBuf {
        xdg_dir("XDG_STATE_HOME", ".local/state")
    }

    pub fn wallpaper_folder(&self) -> Option<PathBuf> {
        self.workspace
            .wallpaper_folder
            .as_deref()
            .map(|path| self.beside(path))
    }

    pub fn cursor_path(&self) -> Option<PathBuf> {
        self.assets.cursor.as_deref().map(|path| self.beside(path))
    }

    pub fn font_path(&self) -> Option<PathBuf> {
        self.assets.font.as_deref().map(|path| self.beside(path))
    }

    pub fn window_dir(&self) -> Option<PathBuf> {
        self.assets.window.as_deref().map(|path| self.beside(path))
    }

    pub fn dock_dir(&self) -> Option<PathBuf> {
        self.assets.dock.as_deref().map(|path| self.beside(path))
    }

    pub fn faces_dir(&self) -> PathBuf {
        self.beside(Path::new("../img/users"))
    }

    pub fn load() -> Result<Config, ConfigError> {
        Config::load_from(&Config::config_path())
    }

    pub fn load_from(path: &Path) -> Result<Config, ConfigError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Config::default());
            }
            Err(source) => {
                return Err(ConfigError::Read {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        let mut config = Config::parse(&text).map_err(|message| ConfigError::Parse {
            path: path.to_path_buf(),
            message,
        })?;
        config.base = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        config.validate()?;
        Ok(config)
    }

    pub fn parse(text: &str) -> Result<Config, String> {
        toml::from_str(text).map_err(|e| e.message().to_string())
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.workspace.width == 0 || self.workspace.height == 0 {
            return Err(ConfigError::Invalid(
                "workspace width and height are counted in screens and must be at least 1".into(),
            ));
        }
        if self.drawers.top < 16.0 || self.drawers.bottom < 16.0 {
            return Err(ConfigError::Invalid(
                "drawer heights are in pixels and must be at least 16".into(),
            ));
        }
        if self.tray.size < 6.0 {
            return Err(ConfigError::Invalid(
                "tray size is text height in pixels and must be at least 6".into(),
            ));
        }
        let (warm, plain) = (crate::night::WARMEST, crate::night::PLAIN);
        if !(warm..=plain).contains(&self.night.warmth) {
            return Err(ConfigError::Invalid(format!(
                "night warmth is kelvin from {warm} to {plain}"
            )));
        }
        if self.night.start > 23 || self.night.end > 23 {
            return Err(ConfigError::Invalid(
                "night start and end are hours from 0 to 23".into(),
            ));
        }
        let (least, most) = REPEAT_DELAY;
        if !(least..=most).contains(&self.keyboard.repeat_delay) {
            return Err(ConfigError::Invalid(format!(
                "keyboard repeat_delay is milliseconds from {least} to {most}"
            )));
        }
        let (least, most) = REPEAT_RATE;
        if !(least..=most).contains(&self.keyboard.repeat_rate) {
            return Err(ConfigError::Invalid(format!(
                "keyboard repeat_rate is keys a second from {least} to {most}"
            )));
        }
        if self.touchpad.speed > FASTEST {
            return Err(ConfigError::Invalid(format!(
                "touchpad speed runs from 0 to {FASTEST}"
            )));
        }
        if self.mouse.speed > FASTEST {
            return Err(ConfigError::Invalid(format!(
                "mouse speed runs from 0 to {FASTEST}"
            )));
        }
        let motion = &self.motion;
        let ranges = [
            ("corner_size", motion.corner_size, CORNER_SIZE, "pixels"),
            ("corner_wait", motion.corner_wait, WAIT_MS, "milliseconds"),
            ("pan_speed", motion.pan_speed, PAN_SPEED, "percent"),
            ("bar_open", motion.bar_open, WAIT_MS, "milliseconds"),
            ("bar_close", motion.bar_close, WAIT_MS, "milliseconds"),
            ("bar_slide", motion.bar_slide, SLIDE_MS, "milliseconds"),
            ("flight", motion.flight, SLIDE_MS, "milliseconds"),
        ];
        for (key, value, (least, most), unit) in ranges {
            if !(least..=most).contains(&value) {
                return Err(ConfigError::Invalid(format!(
                    "motion {key} is {unit} from {least} to {most}"
                )));
            }
        }
        if !(8..=256).contains(&self.appearance.cursor_size) {
            return Err(ConfigError::Invalid(
                "appearance cursor_size is pixels from 8 to 256".into(),
            ));
        }
        if self.power.sleep_after > LONGEST_LOCK {
            return Err(ConfigError::Invalid(format!(
                "power sleep_after is minutes and must be at most {LONGEST_LOCK}"
            )));
        }
        if self.notify.seconds > LONGEST_NOTE {
            return Err(ConfigError::Invalid(format!(
                "notify seconds must be at most {LONGEST_NOTE}"
            )));
        }
        for action in Action::ALL {
            Combo::parse(self.keys.get(action))
                .map_err(|error| ConfigError::Invalid(format!("keys {}: {error}", action.key())))?;
        }
        if self.lock.screen_off > LONGEST_LOCK {
            return Err(ConfigError::Invalid(format!(
                "lock screen_off is minutes and must be at most {LONGEST_LOCK}"
            )));
        }
        if self.lock.after > LONGEST_LOCK {
            return Err(ConfigError::Invalid(format!(
                "lock after is minutes and must be at most {LONGEST_LOCK}"
            )));
        }
        if self.workspace.wallpaper_motion > MOST_MOTION {
            return Err(ConfigError::Invalid(format!(
                "wallpaper_motion is a percent and must be at most {MOST_MOTION}"
            )));
        }
        if self.workspace.wallpaper_every > LONGEST_ROTATION {
            return Err(ConfigError::Invalid(format!(
                "wallpaper_every is seconds and must be at most {LONGEST_ROTATION}"
            )));
        }
        let mut order = self.window.order.clone();
        order.sort_by_key(|name| *name as u8);
        order.dedup();
        if order.len() != 3 || self.window.order.len() != 3 {
            return Err(ConfigError::Invalid(
                "window order names close, minimize and maximize, each once".into(),
            ));
        }
        let chosen = [&self.apps.files, &self.apps.terminal, &self.apps.browser];
        for item in self
            .dock
            .pinned
            .iter()
            .chain(&self.login.start)
            .chain(chosen.into_iter().flatten())
        {
            item.check().map_err(ConfigError::Invalid)?;
        }
        for metric in &self.tray.metrics {
            crate::stats::Metric::parse(metric).map_err(ConfigError::Invalid)?;
        }
        for place in &self.place {
            place.check().map_err(ConfigError::Invalid)?;
        }
        for (index, monitor) in self.monitor.iter().enumerate() {
            monitor.check().map_err(ConfigError::Invalid)?;
            if self.monitor[..index]
                .iter()
                .any(|before| before.name == monitor.name)
            {
                return Err(ConfigError::Invalid(format!(
                    "monitor {} is listed twice",
                    monitor.name
                )));
            }
        }
        Ok(())
    }

    pub fn world_size(&self, screen: Vec2) -> Vec2 {
        Vec2::new(
            screen.x * f64::from(self.workspace.width),
            screen.y * f64::from(self.workspace.height),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_is_looked_for_beside_the_binary_first() {
        let exe = Path::new("/home/dh/src/drifter/target/debug/drifter");
        let candidates = repo_config(exe);
        assert!(candidates.contains(&PathBuf::from("/home/dh/src/drifter/conf/drifter.toml")));
    }

    #[test]
    fn the_repo_config_wins_over_the_home_one() {
        let repo = PathBuf::from("/home/dh/src/drifter/conf/drifter.toml");
        let home = PathBuf::from("/home/dh/.config/drifter/drifter.toml");
        let chosen = choose_config(vec![repo.clone()], home, |path| path == repo);
        assert_eq!(chosen, repo);
    }

    #[test]
    fn the_home_config_is_used_when_there_is_no_repo_beside_the_binary() {
        let repo = PathBuf::from("/opt/drifter/conf/drifter.toml");
        let home = PathBuf::from("/home/dh/.config/drifter/drifter.toml");
        let chosen = choose_config(vec![repo], home.clone(), |_| false);
        assert_eq!(chosen, home);
    }

    #[test]
    fn the_first_run_copies_the_defaults_and_never_overwrites() {
        let folder = std::env::temp_dir().join(format!("drifter-first-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let path = folder.join("drifter.toml");
        std::fs::write(folder.join(DEFAULTS), "[lock]\nafter = 5\n").unwrap();
        assert!(Config::first_run(&path).unwrap());
        assert_eq!(Config::load_from(&path).unwrap().lock.after, 5);
        std::fs::write(&path, "[lock]\nafter = 9\n").unwrap();
        assert!(!Config::first_run(&path).unwrap());
        assert_eq!(Config::load_from(&path).unwrap().lock.after, 9);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_repo_search_gives_up_before_it_reaches_the_root() {
        let exe = Path::new("/home/dh/src/drifter/target/debug/drifter");
        assert!(!repo_config(exe).contains(&PathBuf::from("/conf/drifter.toml")));
    }

    #[test]
    fn a_relative_path_is_read_next_to_the_config() {
        let config = Config {
            base: PathBuf::from("/home/dh/src/drifter/conf"),
            ..Config::default()
        };
        assert_eq!(
            config.beside(Path::new("../img/cursor.png")),
            PathBuf::from("/home/dh/src/drifter/conf/../img/cursor.png")
        );
    }

    #[test]
    fn a_relative_path_missing_beside_the_config_is_read_from_the_package() {
        let home = Path::new("/home/dh/.config/drifter");
        let shared = Path::new("/usr/share/drifter/conf");
        let packaged = PathBuf::from("/usr/share/drifter/conf/../img/cursor.png");
        let chosen = resolve_beside(home, shared, Path::new("../img/cursor.png"), |p| {
            p == packaged
        });
        assert_eq!(chosen, packaged);
        let mine = home.join("../img/cursor.png");
        let chosen = resolve_beside(home, shared, Path::new("../img/cursor.png"), |p| {
            p == mine || p == packaged
        });
        assert_eq!(chosen, mine);
        let chosen = resolve_beside(home, shared, Path::new("../img/cursor.png"), |_| false);
        assert_eq!(chosen, mine);
    }

    #[test]
    fn the_first_run_falls_back_to_the_packaged_defaults() {
        let folder = std::env::temp_dir().join(format!("drifter-shared-{}", std::process::id()));
        let shared = folder.join("share");
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join(DEFAULTS), "[lock]\nafter = 7\n").unwrap();
        let path = folder.join("home").join("drifter.toml");
        assert!(Config::first_run_from(&path, &shared).unwrap());
        assert_eq!(Config::load_from(&path).unwrap().lock.after, 7);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_defaults_beside_the_config_win_over_the_packaged_ones() {
        let path = Path::new("/home/dh/src/drifter/conf/drifter.toml");
        let shared = Path::new("/usr/share/drifter/conf");
        let found = defaults_for(path, shared, |_| true);
        assert_eq!(found, Some(PathBuf::from("/home/dh/src/drifter/conf/default.toml")));
        let found = defaults_for(path, shared, |p| p.starts_with("/usr"));
        assert_eq!(found, Some(PathBuf::from("/usr/share/drifter/conf/default.toml")));
        assert_eq!(defaults_for(path, shared, |_| false), None);
    }

    #[test]
    fn access_is_off_until_the_config_turns_it_on() {
        let config = Config::parse("").unwrap();
        assert_eq!(config.access, Access::default());
        assert!(!config.access.sticky_keys);
        let config = Config::parse("[access]\nreduce_motion = true\nsticky_keys = true\n").unwrap();
        assert!(config.access.reduce_motion && config.access.sticky_keys);
        assert!(!config.access.large_text);
        assert!(Config::parse("[access]\nbig = true\n").is_err());
    }

    #[test]
    fn reduced_motion_makes_the_camera_and_bars_jump() {
        let still = Tunables::default().still();
        assert_eq!(still.camera_anim, Duration::ZERO);
        assert_eq!(still.drawer_anim, Duration::ZERO);
        assert_eq!(still.hot_corner_dwell, Tunables::default().hot_corner_dwell);
    }

    #[test]
    fn screenshots_go_to_pictures_unless_the_config_says_where() {
        let home = Path::new("/home/dh");
        let config = Config::parse("").unwrap();
        assert_eq!(config.screenshots.folder_in(home), PathBuf::from("/home/dh/Pictures/Screenshots"));
        assert_eq!(config.screenshots.format, ShotFormat::Png);
        let config = Config::parse("[screenshots]\nfolder = \"~/Desktop\"\nformat = \"jpeg\"\n").unwrap();
        assert_eq!(config.screenshots.folder_in(home), PathBuf::from("/home/dh/Desktop"));
        assert_eq!(config.screenshots.format.extension(), "jpg");
        let config = Config::parse("[screenshots]\nfolder = \"/srv/shots\"\n").unwrap();
        assert_eq!(config.screenshots.folder_in(home), PathBuf::from("/srv/shots"));
        assert!(Config::parse("[screenshots]\nformat = \"gif\"\n").is_err());
    }

    #[test]
    fn an_absolute_path_is_left_alone() {
        let config = Config {
            base: PathBuf::from("/home/dh/src/drifter/conf"),
            ..Config::default()
        };
        assert_eq!(
            config.beside(Path::new("/elsewhere/art.png")),
            PathBuf::from("/elsewhere/art.png")
        );
    }

    #[test]
    fn the_asset_paths_are_none_until_the_config_names_them() {
        let config = Config::default();
        assert_eq!(config.cursor_path(), None);
        assert_eq!(config.font_path(), None);
        assert_eq!(config.wallpaper_path(), None);
    }

    #[test]
    fn the_config_carries_the_cursor_and_the_font() {
        let config = Config::parse(
            "[assets]\ncursor = \"../img/cursor.png\"\nfont = \"../fonts/Inter-Regular.ttf\"\n",
        )
        .unwrap();
        assert_eq!(
            config.assets.cursor.as_deref(),
            Some(Path::new("../img/cursor.png"))
        );
        assert_eq!(
            config.assets.font.as_deref(),
            Some(Path::new("../fonts/Inter-Regular.ttf"))
        );
    }

    #[test]
    fn the_top_drawer_is_thinner_than_the_bottom_one_by_default() {
        let config = Config::default();
        assert_eq!(config.drawers.top, 36.0);
        assert_eq!(config.drawers.bottom, 64.0);
        assert_eq!(config.drawers.height(crate::drawer::Edge::Top), 36.0);
        assert_eq!(config.drawers.height(crate::drawer::Edge::Bottom), 64.0);
    }

    #[test]
    fn the_drawer_heights_come_from_the_config() {
        let config = Config::parse("[drawers]\ntop = 28\nbottom = 80\n").unwrap();
        assert_eq!(config.drawers.top, 28.0);
        assert_eq!(config.drawers.bottom, 80.0);
    }

    #[test]
    fn a_silly_drawer_height_is_refused() {
        let config = Config::parse("[drawers]\ntop = 2\n").unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn the_dock_is_empty_until_the_config_pins_something() {
        assert!(Config::default().dock.pinned.is_empty());
    }

    #[test]
    fn the_dock_keeps_the_order_the_config_gives() {
        let config = Config::parse("[dock]\npinned = [\"foot\", \"copyq\"]\n").unwrap();
        let names: Vec<String> = config.dock.pinned.iter().map(Launch::name).collect();
        assert_eq!(names, ["foot", "copyq"]);
    }

    #[test]
    fn the_tray_text_size_comes_from_the_config() {
        assert_eq!(Config::default().tray.size, 13.0);
        assert_eq!(
            Config::parse("[tray]\nsize = 20\n").unwrap().tray.size,
            20.0
        );
    }

    #[test]
    fn the_stats_show_cpu_ram_and_the_root_disk_by_default() {
        let config = Config::default();
        assert!(config.tray.stats);
        assert_eq!(config.tray.metrics, ["cpu", "ram", "/", "battery"]);
        assert_eq!(Config::parse("[tray]\n").unwrap().tray, config.tray);
    }

    #[test]
    fn the_stats_can_be_turned_off_and_chosen() {
        let config =
            Config::parse("[tray]\nstats = false\nmetrics = [\"ram\", \"/home\", \"sda3\"]\n")
                .unwrap();
        assert!(!config.tray.stats);
        assert_eq!(config.tray.metrics, ["ram", "/home", "sda3"]);
    }

    #[test]
    fn an_empty_metric_is_refused() {
        let config = Config::parse("[tray]\nmetrics = [\"cpu\", \"\"]\n").unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn the_new_sections_default_to_quiet() {
        let config = Config::default();
        assert_eq!(config.window.buttons, Side::Left);
        assert_eq!(config.window.order, default_order());
        assert!(config.window.title);
        assert_eq!(config.lock.after, 0);
        assert!(config.login.start.is_empty());
        assert_eq!(config.workspace.wallpaper_every, 0);
        assert_eq!(config.workspace.wallpaper_order, WallpaperOrder::Random);
    }

    #[test]
    fn an_old_pinned_list_of_names_still_reads() {
        let config = Config::parse("[dock]\npinned = [\"foot\", \"firefox\"]\n").unwrap();
        assert_eq!(config.dock.pinned[0], Launch::App("foot".into()));
        assert_eq!(config.dock.pinned[1].app(), Some("firefox"));
    }

    #[test]
    fn a_launch_can_be_a_command_with_an_icon() {
        let text = "[dock]\npinned = [{ command = \"foot -e htop\", icon = \"htop.png\" }]\n\
                    [login]\nstart = [\"firefox\", { command = \"syncthing\" }]\n";
        let config = Config::parse(text).unwrap();
        let item = &config.dock.pinned[0];
        assert_eq!(item.command(), Some("foot -e htop"));
        assert_eq!(item.icon(), Some(Path::new("htop.png")));
        assert_eq!(item.name(), "foot");
        assert_eq!(config.login.start.len(), 2);
        config.validate().unwrap();
    }

    #[test]
    fn autostart_is_on_unless_the_config_turns_it_off() {
        assert!(Config::default().login.autostart);
        assert!(
            Config::parse("[login]\nstart = []\n")
                .unwrap()
                .login
                .autostart
        );
        assert!(
            !Config::parse("[login]\nautostart = false\n")
                .unwrap()
                .login
                .autostart
        );
    }

    #[test]
    fn a_launch_with_both_or_neither_is_refused() {
        let both = Config::parse("[login]\nstart = [{ app = \"a\", command = \"b\" }]\n").unwrap();
        assert!(both.validate().is_err());
        let neither = Config::parse("[login]\nstart = [{ icon = \"x.png\" }]\n").unwrap();
        assert!(neither.validate().is_err());
    }

    #[test]
    fn the_window_section_reads_side_order_and_title() {
        let text = "[window]\nbuttons = \"right\"\norder = [\"minimize\", \"maximize\", \"close\"]\ntitle = false\n";
        let config = Config::parse(text).unwrap();
        assert_eq!(config.window.buttons, Side::Right);
        assert_eq!(config.window.order[2], ButtonName::Close);
        assert!(!config.window.title);
        config.validate().unwrap();
        assert_eq!(config.window.gtk_layout(), ":minimize,maximize,close");
    }

    #[test]
    fn gtk_apps_get_the_buttons_on_drifters_side_in_drifters_order() {
        assert_eq!(WindowConfig::default().gtk_layout(), "close,minimize,maximize:");
        let config = Config::parse("[appearance]\nstyle = \"auto\"\n").unwrap();
        assert_eq!(config.appearance.style, Some(Style::Auto));
    }

    #[test]
    fn a_window_order_must_name_each_button_once() {
        let twice =
            Config::parse("[window]\norder = [\"close\", \"close\", \"maximize\"]\n").unwrap();
        assert!(twice.validate().is_err());
        let short = Config::parse("[window]\norder = [\"close\"]\n").unwrap();
        assert!(short.validate().is_err());
    }

    #[test]
    fn a_wallpaper_folder_is_found_beside_the_config() {
        let text = "[workspace]\nwallpaper_folder = \"walls\"\nwallpaper_every = 600\nwallpaper_order = \"ordered\"\n";
        let config = Config::parse(text).unwrap();
        assert_eq!(config.workspace.wallpaper_every, 600);
        assert_eq!(config.workspace.wallpaper_order, WallpaperOrder::Ordered);
        assert!(config.wallpaper_folder().unwrap().ends_with("walls"));
    }

    #[test]
    fn a_silly_tray_size_is_refused() {
        assert!(
            Config::parse("[tray]\nsize = 1\n")
                .unwrap()
                .validate()
                .is_err()
        );
    }

    #[test]
    fn the_state_folder_is_named_drifter_once() {
        let dir = Config::state_dir();
        assert_eq!(dir.file_name().unwrap(), "drifter");
        assert_ne!(dir.parent().unwrap().file_name().unwrap(), "drifter");
    }

    #[test]
    fn huge_times_are_refused_rather_than_overflowing() {
        assert!(
            Config::parse("[lock]\nafter = 99999999999\n")
                .unwrap()
                .validate()
                .is_err()
        );
        assert!(
            Config::parse("[workspace]\nwallpaper_every = 99999999999999\n")
                .unwrap()
                .validate()
                .is_err()
        );
        Config::parse("[lock]\nafter = 60\n")
            .unwrap()
            .validate()
            .unwrap();
    }

    #[test]
    fn the_defaults_in_the_repo_parse() {
        let text = include_str!("../../../conf/default.toml");
        let config = Config::parse(text).expect("conf/default.toml does not parse");
        config.validate().expect("conf/default.toml is not valid");
    }

    #[test]
    fn every_modifier_has_a_left_and_a_right_key() {
        for modifier in [Modifier::Super, Modifier::Alt, Modifier::Ctrl] {
            let keys = modifier.keys();
            assert_ne!(keys[0], keys[1]);
            assert!(modifier.holds(keys[0]) && modifier.holds(keys[1]));
        }
    }

    #[test]
    fn the_modifiers_never_share_a_key() {
        let all = [Modifier::Super, Modifier::Alt, Modifier::Ctrl];
        let mut seen: Vec<u32> = all.iter().flat_map(|m| m.keys()).collect();
        let count = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), count);
    }

    #[test]
    fn a_plain_letter_is_never_the_modifier() {
        assert!(!Modifier::Ctrl.holds(38));
        assert!(!Modifier::Super.holds(9));
    }

    #[test]
    fn empty_config_is_all_defaults() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn a_livepaper_name_is_looked_up_and_falls_back_to_aurora() {
        let root = std::env::temp_dir().join(format!("drifter-resolve-{}", std::process::id()));
        let shelf = root.join("livepapers");
        std::fs::create_dir_all(&shelf).unwrap();
        std::fs::write(shelf.join("bloom.livepaper"), b"z").unwrap();
        std::fs::write(shelf.join(FALLBACK_WALLPAPER), b"vec3 wall(vec2 P){return vec3(0.);}").unwrap();
        let beside = |path: &Path| root.join("conf").join(path);
        let folders = vec![shelf.clone()];
        assert_eq!(
            resolve_picture(Path::new("bloom.livepaper"), &folders, beside),
            shelf.join("bloom.livepaper")
        );
        assert_eq!(
            resolve_picture(Path::new("gone.livepaper"), &folders, beside),
            shelf.join(FALLBACK_WALLPAPER)
        );
        assert_eq!(
            resolve_picture(Path::new("ink.shader"), &folders, beside),
            shelf.join(FALLBACK_WALLPAPER)
        );
        assert_eq!(
            resolve_picture(Path::new("../img/a.png"), &folders, beside),
            root.join("conf/../img/a.png")
        );
        let file = Config::parse(include_str!("../../../conf/default.toml")).unwrap();
        assert_eq!(file.workspace.wallpaper.as_deref(), Some(Path::new("aurora.shader")));
        assert_eq!(file.workspace.wallpaper_motion, 100);
    }

    #[test]
    fn defaults_are_nine_by_nine_and_parallax() {
        let config = Config::default();
        assert_eq!(config.workspace.width, 9);
        assert_eq!(config.workspace.height, 9);
        let file = Config::parse(include_str!("../../../conf/default.toml")).unwrap();
        assert_eq!((file.workspace.width, file.workspace.height), (9, 9));
        assert_eq!(config.workspace.wallpaper_mode, WallpaperMode::Parallax);
        assert_eq!(file.workspace.wallpaper_mode, WallpaperMode::Parallax);
        assert_eq!(config.input.modifier, Modifier::Super);
    }

    #[test]
    fn night_is_off_by_default_and_checked() {
        let night = Config::default().night;
        assert!(!night.on);
        assert_eq!((night.warmth, night.start, night.end), (4000, 21, 7));
        let set = Config::parse("[night]\non = true\nwarmth = 3000\n").unwrap();
        set.validate().unwrap();
        assert!(set.night.on && set.night.end == 7);
        assert!(
            Config::parse("[night]\nwarmth = 9000\n")
                .unwrap()
                .validate()
                .is_err()
        );
        assert!(
            Config::parse("[night]\nstart = 24\n")
                .unwrap()
                .validate()
                .is_err()
        );
    }

    #[test]
    fn the_keyboard_table_is_optional_and_checked() {
        let none = Config::default();
        assert_eq!(none.keyboard.layout, "");
        assert_eq!(
            (none.keyboard.repeat_delay, none.keyboard.repeat_rate),
            (200, 25)
        );
        let set = Config::parse(
            "[keyboard]\nlayout = \"de\"\nvariant = \"nodeadkeys\"\noptions = \"caps:escape\"\nrepeat_delay = 300\n",
        )
        .unwrap();
        set.validate().unwrap();
        assert_eq!(set.keyboard.layout, "de");
        assert_eq!(set.keyboard.repeat_rate, 25);
        let slow = Config::parse("[keyboard]\nrepeat_delay = 5000\n").unwrap();
        assert!(slow.validate().is_err());
        let fast = Config::parse("[keyboard]\nrepeat_rate = 0\n").unwrap();
        assert!(fast.validate().is_err());
    }

    #[test]
    fn the_screen_turns_off_after_ten_minutes_unless_told_otherwise() {
        assert_eq!(Config::default().lock.screen_off, 10);
        let other = Config::parse("[lock]\nafter = 5\n").unwrap();
        assert_eq!(other.lock.screen_off, 10);
        let never = Config::parse("[lock]\nscreen_off = 0\n").unwrap();
        assert_eq!(never.lock.screen_off, 0);
        let long = Config::parse(&format!("[lock]\nscreen_off = {}\n", LONGEST_LOCK + 1)).unwrap();
        assert!(long.validate().is_err());
    }

    #[test]
    fn touchpad_speed_maps_onto_the_libinput_range() {
        let at = |speed| {
            Touchpad {
                speed,
                ..Touchpad::default()
            }
            .libinput_speed()
        };
        assert_eq!(at(0), -1.0);
        assert_eq!(at(FASTEST / 2), 0.0);
        assert_eq!(at(FASTEST), 1.0);
        let fast = Config::parse("[touchpad]\nspeed = 11\n").unwrap();
        assert!(fast.validate().is_err());
        let word = Config::parse("[touchpad]\nthree_tap = \"middle\"\n").unwrap();
        assert_eq!(word.touchpad.three_tap, ThreeTap::Middle);
        assert!(word.touchpad.tap);
    }

    #[test]
    fn parses_the_documented_file() {
        let config = Config::parse(
            r#"
            [workspace]
            width = 4
            height = 6
            wallpaper = "/path/to/huge_wallpaper.png"
            wallpaper_mode = "pan"

            [input]
            modifier = "super"
            "#,
        )
        .unwrap();
        assert_eq!(config.workspace.width, 4);
        assert_eq!(config.workspace.height, 6);
        assert_eq!(
            config.workspace.wallpaper.as_deref(),
            Some(Path::new("/path/to/huge_wallpaper.png"))
        );
        assert_eq!(config.workspace.wallpaper_mode, WallpaperMode::Pan);
    }

    #[test]
    fn world_size_is_screens_times_resolution() {
        let config = Config::parse("[workspace]\nwidth = 4\nheight = 6\n").unwrap();
        let world = config.world_size(Vec2::new(3840.0, 2160.0));
        assert_eq!(world, Vec2::new(15360.0, 12960.0));
    }

    #[test]
    fn zero_screens_is_rejected() {
        let config = Config::parse("[workspace]\nwidth = 0\n").unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(Config::parse("[workspace]\nwidht = 4\n").is_err());
    }

    #[test]
    fn xdg_wins_when_it_is_absolute() {
        let p = resolve_xdg(Some("/xdg".into()), Some("/home/dh".into()), ".config");
        assert_eq!(p, Path::new("/xdg/drifter"));
    }

    #[test]
    fn a_relative_xdg_value_is_ignored_and_home_is_used() {
        let p = resolve_xdg(Some("relative".into()), Some("/home/dh".into()), ".config");
        assert_eq!(p, Path::new("/home/dh/.config/drifter"));
    }

    #[test]
    fn home_is_used_when_xdg_is_unset() {
        let p = resolve_xdg(None, Some("/home/dh".into()), ".local/share");
        assert_eq!(p, Path::new("/home/dh/.local/share/drifter"));
    }

    #[test]
    fn a_missing_file_is_defaults_not_an_error() {
        let path = Path::new("/nonexistent/drifter/drifter.toml");
        assert_eq!(Config::load_from(path).unwrap(), Config::default());
    }
}
