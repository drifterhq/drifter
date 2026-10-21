use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::{
    Appearance, ButtonName, Config, FASTEST, Keyboard, Keys, Launch, Modifier, Motion, Mouse,
    Night, Notify, Power, REPEAT_DELAY, REPEAT_RATE, Side, ThreeTap, Touchpad, WallpaperMode,
    WallpaperOrder,
};
use crate::desktop::Role;
use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::keyboard::Layout;
use crate::notify::Recent;
use crate::paint::Canvas;
use crate::screens::{self, Head, MonitorConfig};
use crate::sheet::{For, Sheet};
use crate::stats::{self, Metric, Mount};
use crate::system::Sound;
use crate::text::Font;

pub const WORLD_LEAST: u32 = 2;
pub const WORLD_MOST: u32 = 10;
pub const UNITS: [u64; 3] = [1, 60, 3600];
pub const BRIGHTNESS_LEAST: f64 = 5.0;

mod keys;
mod pages;
mod people;

pub use keys::{Move, Press, Spot};

pub use people::{BIG_FACE, ROW_FACE, UserAsk};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Desktop,
    Displays,
    Wallpaper,
    Appearance,
    Window,
    Dock,
    TopBar,
    Notifications,
    Network,
    Bluetooth,
    Sound,
    Keyboard,
    Mouse,
    Touchpad,
    Shortcuts,
    Access,
    Power,
    Lock,
    Users,
    Time,
    Login,
    Apps,
    About,
}

impl Section {
    pub const ALL: [Section; 23] = [
        Section::Desktop,
        Section::Wallpaper,
        Section::Appearance,
        Section::Window,
        Section::TopBar,
        Section::Dock,
        Section::Notifications,
        Section::Network,
        Section::Bluetooth,
        Section::Displays,
        Section::Sound,
        Section::Power,
        Section::Keyboard,
        Section::Mouse,
        Section::Touchpad,
        Section::Shortcuts,
        Section::Users,
        Section::Lock,
        Section::Access,
        Section::Time,
        Section::Login,
        Section::Apps,
        Section::About,
    ];

    pub fn group(self) -> &'static str {
        match self {
            Section::Desktop
            | Section::Wallpaper
            | Section::Appearance
            | Section::Window
            | Section::Dock
            | Section::TopBar
            | Section::Notifications => crate::lang::t("Desktop"),
            Section::Network | Section::Bluetooth => crate::lang::t("Network"),
            Section::Displays | Section::Sound | Section::Power => crate::lang::t("Hardware"),
            Section::Keyboard | Section::Mouse | Section::Touchpad | Section::Shortcuts => crate::lang::t("Input"),
            Section::Users | Section::Lock | Section::Access => crate::lang::t("Users & Security"),
            Section::Time | Section::Login | Section::Apps | Section::About => crate::lang::t("System"),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Section::Desktop => crate::lang::t("Desktop"),
            Section::Displays => crate::lang::t("Displays"),
            Section::Wallpaper => crate::lang::t("Wallpaper"),
            Section::Appearance => crate::lang::t("Appearance"),
            Section::Window => crate::lang::t("Windows"),
            Section::Dock => crate::lang::t("Dock"),
            Section::TopBar => crate::lang::t("Top Bar"),
            Section::Notifications => crate::lang::t("Notifications"),
            Section::Network => crate::lang::t("Wi-Fi"),
            Section::Bluetooth => crate::lang::t("Bluetooth"),
            Section::Sound => crate::lang::t("Sound"),
            Section::Keyboard => crate::lang::t("Keyboard"),
            Section::Mouse => crate::lang::t("Mouse"),
            Section::Touchpad => crate::lang::t("Touchpad"),
            Section::Shortcuts => crate::lang::t("Shortcuts"),
            Section::Access => crate::lang::t("Accessibility"),
            Section::Power => crate::lang::t("Power"),
            Section::Lock => crate::lang::t("Lock Screen"),
            Section::Users => crate::lang::t("Users"),
            Section::Time => crate::lang::t("Date & Time"),
            Section::Login => crate::lang::t("Login Items"),
            Section::Apps => crate::lang::t("Default Apps"),
            Section::About => crate::lang::t("About"),
        }
    }

    pub fn words(self) -> &'static str {
        match self {
            Section::Desktop => "world workspace zoom pan hot corner camera animation",
            Section::Displays => "monitor screen scale rotation flip resolution refresh rate hz mode adaptive sync vrr freesync gsync hdr high dynamic range mirror duplicate brightness night light warm",
            Section::Wallpaper => "background picture image photo",
            Section::Appearance => "theme dark light accent color icons cursor font",
            Section::Window => "title bar buttons close minimize maximize",
            Section::Dock => "apps pinned trash bottom bar hide always visible",
            Section::TopBar => "clock tray cpu memory battery disk hide always visible",
            Section::Notifications => "alerts do not disturb history",
            Section::Network => "wifi wireless network internet password join",
            Section::Bluetooth => "bluetooth pair headphones speaker mouse keyboard devices",
            Section::Sound => "audio volume speaker headphones microphone mute",
            Section::Keyboard => "layout typing repeat caps compose",
            Section::Mouse => "pointer speed scroll buttons",
            Section::Touchpad => "trackpad tap swipe scroll gestures",
            Section::Shortcuts => "keys hotkeys keybindings screenshot folder format png jpeg",
            Section::Access => "accessibility large text bigger high contrast reduce motion animation sticky keys a11y screen reader orca speech blind",
            Section::Power => "sleep suspend lid battery shutdown profile performance saver balanced screen monitor display off blank idle inactive timeout",
            Section::Lock => "password idle screen off login automatic monitor display sleep blank inactive timeout",
            Section::Users => "user account people password picture photo avatar face admin administrator name add remove login",
            Section::Time => "clock zone ntp date language español deutsch français translation",
            Section::Login => "startup autostart apps",
            Section::Apps => "browser terminal file manager",
            Section::About => "system version computer reset import export",
        }
    }

    pub fn from_word(word: &str) -> Option<Section> {
        let wanted: String = word
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
            .to_lowercase();
        Section::ALL.into_iter().find(|section| {
            let name: String = section
                .name()
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect();
            name.to_lowercase() == wanted
        })
    }

    fn tint(self) -> [u8; 3] {
        match self {
            Section::Desktop => [52, 168, 140],
            Section::Displays => [70, 120, 210],
            Section::Wallpaper => [40, 170, 210],
            Section::Appearance => [70, 76, 96],
            Section::Window => [88, 110, 230],
            Section::Dock => [120, 128, 146],
            Section::TopBar => [240, 140, 50],
            Section::Notifications => [226, 64, 72],
            Section::Network => [40, 130, 230],
            Section::Bluetooth => [30, 100, 210],
            Section::Sound => [230, 70, 130],
            Section::Keyboard => [150, 110, 220],
            Section::Mouse => [110, 120, 200],
            Section::Touchpad => [60, 150, 110],
            Section::Shortcuts => [130, 100, 200],
            Section::Access => [40, 120, 220],
            Section::Power => [70, 170, 80],
            Section::Lock => [96, 102, 120],
            Section::Users => [70, 130, 220],
            Section::Time => [60, 130, 220],
            Section::Login => [220, 80, 90],
            Section::Apps => [214, 160, 40],
            Section::About => [128, 134, 150],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    Net(NetAsk),
    Pick(For),
    Files(For),
    Choose(For),
    Capture(usize),
    Forget,
    Reset,
    User(UserAsk),
    RenamePlace(usize),
    EditDock(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    pub label: String,
    pub point: String,
    pub on: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Monitor {
    pub name: String,
    pub pixels: Vec2,
    pub scale: f64,
    pub rotate: u32,
    pub place: Vec2,
    pub mode: Option<String>,
    pub enabled: bool,
    pub modes: Vec<crate::mode::Mode>,
    pub current: Option<crate::mode::Mode>,
    pub flip: bool,
    pub vrr: bool,
    pub mirror: Option<String>,
    pub profile: Option<String>,
    pub hdr: bool,
    pub hdr_white: u32,
    pub hdr_able: bool,
}

impl Monitor {
    pub fn mirrors(&self) -> Option<&str> {
        self.mirror.as_deref().filter(|source| *source != self.name)
    }

    fn head(&self) -> Head {
        Head {
            name: self.name.clone(),
            pixels: self.pixels,
            scale: self.scale,
            rotate: self.rotate,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub world: u32,
    pub wallpaper_mode: WallpaperMode,
    pub modifier: Modifier,
    pub touchpad: Touchpad,
    pub keyboard: Keyboard,
    pub layouts: Vec<Layout>,
    pub night: Night,
    pub top: f64,
    pub bottom: f64,
    pub text_size: f64,
    pub day: bool,
    pub date: bool,
    pub seconds: bool,
    pub hours24: bool,
    pub custom: Option<String>,
    pub stats: bool,
    pub cpu: bool,
    pub ram: bool,
    pub battery: bool,
    pub disks: Vec<Disk>,
    pub kept: Vec<String>,
    pub side: Side,
    pub order: [ButtonName; 3],
    pub title: bool,
    pub inactive: crate::config::Inactive,
    pub lock_after: u64,
    pub screen_off: u64,
    pub login: Vec<Launch>,
    pub autostart: bool,
    pub automatic: bool,
    pub autostarts: Vec<crate::autostart::Entry>,
    pub dock: Vec<Launch>,
    pub dock_icon: f64,
    pub dock_pad: f64,
    pub dock_fit: bool,
    pub dock_hover: crate::config::DockHover,
    pub dock_magnify: u32,
    pub dock_gap: f64,
    pub glance: usize,
    pub shows: [bool; 6],
    pub defaults: [Option<Launch>; 8],
    pub system: [Option<String>; 8],
    pub wallpaper: Option<PathBuf>,
    pub folder: Option<PathBuf>,
    pub rotate: u64,
    pub unit: usize,
    pub rotation: WallpaperOrder,
    pub drift: u32,
    pub ripple: bool,
    pub ripple_strength: u32,
    pub wallpaper_pointer: Option<bool>,
    pub pictures: Vec<PathBuf>,
    pub shaders: Vec<PathBuf>,
    pub names: HashMap<String, String>,
    pub monitors: Vec<Monitor>,
    pub picked: usize,
    pub monitor_config: Vec<MonitorConfig>,
    pub mouse: Mouse,
    pub motion: Motion,
    pub appearance: Appearance,
    pub assets: [Option<PathBuf>; 4],
    pub power: Power,
    pub notify: Notify,
    pub keys: Keys,
    pub variants: Vec<Layout>,
    pub recent: Vec<Recent>,
    pub now: u64,
    pub sound: Sound,
    pub brightness: Option<f64>,
    pub zone: String,
    pub ntp: Option<bool>,
    pub about: Vec<(String, String)>,
    pub live_logo: bool,
    pub time_error: Option<String>,
    pub query: String,
    pub focus: Option<Id>,
    pub confirm_reset: bool,
    pub file_note: Option<String>,
    pub access: crate::config::Access,
    pub shots: crate::config::Shots,
    pub profile: Option<Profiles>,
    pub cells: Vec<(String, String)>,
    pub places: Vec<crate::place::Place>,
    pub wifi: Option<crate::net::Wifi>,
    pub blue: Option<crate::blue::Blue>,
    pub blue_missing: Option<crate::blue::Missing>,
    pub blue_open: Option<String>,
    pub blue_card: Option<crate::blue::Card>,
    pub blue_mic: bool,
    pub blue_prefs: crate::blue::Prefs,
    pub blue_address: bool,
    pub blue_naming: Option<String>,
    pub blue_steps: Vec<(String, crate::blue::Step)>,
    pub net_note: Option<String>,
    pub display_note: Option<String>,
    pub users: Option<crate::users::Users>,
    pub user_note: Option<String>,
    pub faces: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetAsk {
    Join(usize),
    Hidden,
    Leave(usize),
    Forget(usize),
    Pair(usize),
    Connect(usize),
    Disconnect(usize),
    Remove(usize),
    Options(usize),
    Rename,
    SaveName,
    StopNaming,
    BlueKey,
    Fix,
    Reset,
    Service,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profiles {
    pub now: String,
    pub offered: Vec<String>,
}

pub const PROFILES: [(&str, &str); 3] = [
    ("power-saver", "Saver"),
    ("balanced", "Balanced"),
    ("performance", "Performance"),
];

impl Model {
    pub fn read(
        config: &Config,
        mounts: &[Mount],
        kernel: impl Fn(&Mount) -> Option<String>,
    ) -> Model {
        let format = &config.tray.format;
        let has = |codes: &[&str]| codes.iter().any(|code| format.contains(code));

        let mut disks: Vec<(Disk, String)> = Vec::new();
        for mount in mounts {
            let real = mount.source.starts_with("/dev/") && !mount.source.starts_with("/dev/loop");
            if !real || disks.iter().any(|(_, device)| *device == mount.device) {
                continue;
            }
            disks.push((
                Disk {
                    label: stats::disk_label(kernel(mount).as_deref(), &mount.source),
                    point: mount.point.clone(),
                    on: false,
                },
                mount.device.clone(),
            ));
        }

        let (mut cpu, mut ram, mut kept) = (false, false, Vec::new());
        for text in &config.tray.metrics {
            match Metric::parse(text) {
                Ok(Metric::Cpu) => cpu = true,
                Ok(Metric::Ram) => ram = true,
                Ok(Metric::Battery) => kept.push(text.clone()),
                Ok(Metric::Disk(name)) => {
                    let found = stats::find_disk(mounts, &name, &kernel).and_then(|mount| {
                        disks.iter_mut().find(|(_, device)| *device == mount.device)
                    });
                    match found {
                        Some((disk, _)) => disk.on = true,
                        None => kept.push(text.clone()),
                    }
                }
                Err(_) => {}
            }
        }

        let every = config.workspace.wallpaper_every;
        let unit = match every {
            0 => 1,
            _ if every.is_multiple_of(3600) => 2,
            _ if every.is_multiple_of(60) => 1,
            _ => 0,
        };

        Model {
            world: config
                .workspace
                .width
                .max(config.workspace.height)
                .clamp(WORLD_LEAST, WORLD_MOST),
            wallpaper_mode: config.workspace.wallpaper_mode,
            modifier: config.input.modifier,
            touchpad: config.touchpad,
            keyboard: config.keyboard.clone(),
            night: config.night,
            layouts: crate::keyboard::installed(),
            top: config.drawers.top,
            bottom: config.drawers.bottom,
            text_size: config.tray.size,
            day: has(&["%a", "%A"]),
            date: has(&["%d", "%e", "%b", "%B"]),
            seconds: has(&["%S"]),
            hours24: has(&["%H", "%k", "%R", "%T"]) && !has(&["%I", "%l"]),
            custom: None,
            stats: config.tray.stats,
            cpu,
            ram,
            battery: config.tray.battery,
            disks: disks.into_iter().map(|(disk, _)| disk).collect(),
            kept,
            side: config.window.buttons,
            order: [0, 1, 2].map(|slot| {
                config
                    .window
                    .order
                    .get(slot)
                    .copied()
                    .unwrap_or(ButtonName::Close)
            }),
            title: config.window.title,
            inactive: config.window.inactive,
            lock_after: config.lock.after,
            screen_off: config.lock.screen_off,
            login: config.login.start.clone(),
            autostart: config.login.autostart,
            automatic: config.login.automatic,
            autostarts: Vec::new(),
            dock: config.dock.pinned.clone(),
            dock_icon: config.dock_icon(),
            dock_pad: config.dock_padding(),
            dock_fit: config.dock.fit,
            dock_hover: config.dock.hover,
            dock_magnify: config.dock.magnify,
            dock_gap: config.dock_spacing(),
            glance: crate::glance::Style::parse(&config.tray.style)
                .ok()
                .and_then(|style| crate::glance::Style::ALL.iter().position(|each| *each == style))
                .unwrap_or(0),
            shows: [
                config.dock.files,
                config.dock.terminal,
                config.dock.browser,
                config.dock.settings,
                config.dock.trash,
                config.dock.apps,
            ],
            defaults: Role::ALL.map(|role| config.apps.get(role).cloned()),
            system: Role::ALL.map(crate::desktop::system_default),
            wallpaper: config.wallpaper_path().map(clean),
            folder: config.wallpaper_folder().map(clean),
            rotate: every / UNITS[unit],
            unit,
            rotation: config.workspace.wallpaper_order,
            drift: config.workspace.wallpaper_motion,
            ripple: config.workspace.ripple,
            ripple_strength: config.workspace.ripple_strength,
            wallpaper_pointer: config.workspace.wallpaper_pointer,
            pictures: config
                .wallpaper_folder()
                .map(|folder| crate::wallpaper::pictures(&clean(folder)))
                .unwrap_or_default(),
            shaders: crate::shader::installed(&config.shader_folders())
                .into_iter()
                .map(clean)
                .collect(),
            names: HashMap::new(),
            monitors: Vec::new(),
            picked: 0,
            monitor_config: config.monitor.clone(),
            mouse: config.mouse,
            motion: config.motion,
            appearance: config.appearance.clone(),
            assets: [
                config.cursor_path(),
                config.font_path(),
                config.window_dir(),
                config.dock_dir(),
            ]
            .map(|path| path.map(clean)),
            power: config.power,
            notify: config.notify,
            access: config.access,
            shots: config.screenshots.clone(),
            profile: None,
            cells: Vec::new(),
            places: config.place.clone(),
            wifi: None,
            blue: None,
            blue_missing: None,
            blue_open: None,
            blue_card: None,
            blue_mic: false,
            blue_prefs: crate::blue::Prefs::default(),
            blue_address: false,
            blue_naming: None,
            blue_steps: Vec::new(),
            net_note: None,
            display_note: None,
            users: None,
            user_note: None,
            faces: crate::users::stock(&config.faces_dir()),
            keys: config.keys.clone(),
            variants: crate::keyboard::variants(&config.keyboard.layout),
            recent: Vec::new(),
            now: 0,
            sound: Sound::default(),
            brightness: None,
            zone: String::new(),
            ntp: None,
            about: Vec::new(),
            live_logo: false,
            time_error: None,
            query: String::new(),
            focus: None,
            confirm_reset: false,
            file_note: None,
        }
        .mark_custom(&config.tray.format)
    }

    fn mark_custom(mut self, format: &str) -> Model {
        if self.format() != format {
            self.custom = Some(format.to_string());
        }
        self
    }

    pub fn shown(&self) -> Vec<Section> {
        let query = self.query.trim().to_lowercase();
        if query.is_empty() {
            return Section::ALL.to_vec();
        }
        Section::ALL
            .into_iter()
            .filter(|section| {
                let mut said = vec![section.name().to_string(), section.words().to_string()];
                for group in self.groups(*section) {
                    said.extend(group.header.map(str::to_string));
                    said.extend(group.footer);
                    said.extend(group.rows.into_iter().map(|row| row.label));
                }
                query
                    .split_whitespace()
                    .all(|word| said.iter().any(|text| text.to_lowercase().contains(word)))
            })
            .collect()
    }

    pub fn see_monitors(&mut self, found: &[(String, Vec2)]) {
        let config = self.monitor_entries();
        let heads: Vec<Head> = found
            .iter()
            .map(|(name, pixels)| Head::new(name, *pixels, &config))
            .collect();
        let rects = screens::arrange(&heads, &config);
        let old = std::mem::take(&mut self.monitors);
        self.monitors = heads
            .into_iter()
            .zip(rects)
            .map(|(head, rect)| {
                let own = config.iter().find(|each| each.name == head.name);
                let kept = old.iter().find(|each| each.name == head.name);
                Monitor {
                    mode: own.and_then(|each| each.mode.clone()),
                    enabled: own.is_none_or(|each| each.enabled),
                    flip: own.is_some_and(|each| each.flip),
                    vrr: own.is_some_and(|each| each.vrr),
                    mirror: own.and_then(|each| each.mirror.clone()),
                    profile: own.and_then(|each| each.profile.clone()),
                    hdr: own.is_some_and(|each| each.hdr),
                    hdr_white: own.and_then(|each| each.hdr_white).unwrap_or(crate::hdr::WHITE),
                    hdr_able: kept.map_or_else(|| crate::hdr::screen(&head.name).is_some(), |each| each.hdr_able),
                    modes: kept.map(|each| each.modes.clone()).unwrap_or_default(),
                    current: kept.and_then(|each| each.current),
                    name: head.name,
                    pixels: head.pixels,
                    scale: head.scale,
                    rotate: head.rotate,
                    place: rect.origin,
                }
            })
            .collect();
        self.picked = self.picked.min(self.monitors.len().saturating_sub(1));
    }

    pub fn see_modes(&mut self, name: &str, modes: &[crate::mode::Mode], current: Option<crate::mode::Mode>) {
        if let Some(monitor) = self.monitors.iter_mut().find(|each| each.name == name) {
            monitor.modes = crate::mode::choices(modes);
            monitor.current = current;
        }
    }

    fn shown_mode(monitor: &Monitor) -> Option<crate::mode::Mode> {
        let fallback = monitor.current.map(|mode| (mode.width, mode.height));
        match &monitor.mode {
            Some(_) => crate::mode::pick(&monitor.modes, monitor.mode.as_deref(), fallback),
            None => monitor
                .current
                .or_else(|| crate::mode::select(&monitor.modes, None)),
        }
    }

    fn step_mode(&mut self, size: bool, up: bool) {
        let Some(monitor) = self.monitors.get_mut(self.picked) else {
            return;
        };
        let Some(now) = Model::shown_mode(monitor) else {
            return;
        };
        let next = match size {
            true => {
                let mut sizes: Vec<(u32, u32)> = Vec::new();
                for mode in &monitor.modes {
                    if !sizes.contains(&(mode.width, mode.height)) {
                        sizes.push((mode.width, mode.height));
                    }
                }
                let at = sizes.iter().position(|each| *each == (now.width, now.height));
                let to = match (at, up) {
                    (Some(at), true) => at.checked_sub(1),
                    (Some(at), false) => Some(at + 1).filter(|to| *to < sizes.len()),
                    (None, _) => None,
                };
                to.and_then(|to| {
                    let (w, h) = sizes[to];
                    monitor
                        .modes
                        .iter()
                        .filter(|mode| (mode.width, mode.height) == (w, h))
                        .min_by_key(|mode| mode.refresh_mhz.abs_diff(now.refresh_mhz))
                        .copied()
                })
            }
            false => {
                let mut rates: Vec<crate::mode::Mode> = monitor
                    .modes
                    .iter()
                    .filter(|mode| (mode.width, mode.height) == (now.width, now.height))
                    .copied()
                    .collect();
                rates.sort_by_key(|mode| mode.refresh_mhz);
                let at = rates.iter().position(|mode| mode.refresh_mhz == now.refresh_mhz);
                let to = match (at, up) {
                    (Some(at), true) => Some(at + 1).filter(|to| *to < rates.len()),
                    (Some(at), false) => at.checked_sub(1),
                    (None, _) => None,
                };
                to.map(|to| rates[to])
            }
        };
        if let Some(next) = next {
            monitor.mode = Some(next.text());
            monitor.pixels = Vec2::new(f64::from(next.width), f64::from(next.height));
            self.tidy_monitor();
        }
    }

    pub fn monitor_rects(&self) -> Vec<Rect> {
        self.monitors
            .iter()
            .map(|monitor| Rect::new(monitor.place, monitor.head().size()))
            .collect()
    }

    pub fn monitor_entries(&self) -> Vec<MonitorConfig> {
        let mut rects = self.monitor_rects();
        screens::normalize(&mut rects);
        let mut out: Vec<MonitorConfig> = self
            .monitors
            .iter()
            .zip(rects)
            .map(|(monitor, rect)| MonitorConfig {
                name: monitor.name.clone(),
                scale: monitor.scale,
                rotate: monitor.rotate,
                x: Some(rect.origin.x as i64),
                y: Some(rect.origin.y as i64),
                mode: monitor.mode.clone(),
                enabled: monitor.enabled,
                flip: monitor.flip,
                vrr: monitor.vrr,
                mirror: monitor.mirror.clone(),
                profile: monitor.profile.clone(),
                hdr: monitor.hdr,
                hdr_white: (monitor.hdr_white != crate::hdr::WHITE).then_some(monitor.hdr_white),
            })
            .collect();
        for kept in &self.monitor_config {
            if !out.iter().any(|each| each.name == kept.name) {
                out.push(kept.clone());
            }
        }
        out
    }

    pub fn pick_monitor(&mut self, at: usize) {
        if at < self.monitors.len() {
            self.picked = at;
        }
    }

    pub fn move_monitor(&mut self, at: usize, wanted: Vec2) {
        if at >= self.monitors.len() {
            return;
        }
        let rects = self.monitor_rects();
        self.monitors[at].place = screens::snap(&rects, at, wanted.round());
    }

    pub fn choose_profile(&mut self, file: PathBuf) {
        if let Some(monitor) = self.monitors.get_mut(self.picked) {
            monitor.profile = Some(file.display().to_string());
        }
    }

    fn step_mirror(&mut self, up: bool) {
        let picked = self.picked;
        let Some(name) = self.monitors.get(picked).map(|monitor| monitor.name.clone()) else {
            return;
        };
        let mut choices: Vec<Option<String>> = vec![None];
        choices.extend(
            self.monitors
                .iter()
                .filter(|each| each.name != name && each.enabled && each.mirrors().is_none())
                .map(|each| Some(each.name.clone())),
        );
        let now = self.monitors[picked].mirrors().map(str::to_string);
        let at = choices.iter().position(|each| *each == now).unwrap_or(0);
        let next = match up {
            true => (at + 1) % choices.len(),
            false => (at + choices.len() - 1) % choices.len(),
        };
        self.monitors[picked].mirror = choices[next].clone();
        if let Some(source) = choices[next].as_deref() {
            for each in &mut self.monitors {
                if each.mirrors() == Some(name.as_str()) {
                    each.mirror = Some(source.to_string());
                }
            }
        }
    }

    fn tidy_monitor(&mut self) {
        let at = self.picked;
        if at < self.monitors.len() {
            let here = self.monitors[at].place;
            self.move_monitor(at, here);
        }
    }

    fn mode_rows(&self, monitor: &Monitor) -> Vec<Row> {
        let mut rows = Vec::new();
        if let Some(now) = Model::shown_mode(monitor) {
            let marks = vec![Mark::Minus, Mark::Plus];
            let mut sizes: Vec<(u32, u32)> = monitor.modes.iter().map(|mode| (mode.width, mode.height)).collect();
            sizes.dedup();
            sizes.sort_unstable();
            sizes.dedup();
            let rates = monitor
                .modes
                .iter()
                .filter(|mode| (mode.width, mode.height) == (now.width, now.height))
                .count();
            let stepped = |id: Id, label: &str, many: bool, shown: String| match many {
                true => Row::new(id, label, Control::Buttons(marks.clone())).detail(&shown),
                false => Row::new(id, label, Control::Note(shown)),
            };
            rows.push(stepped(
                Id::Resolution,
                crate::lang::t("Resolution"),
                sizes.len() > 1,
                format!("{} × {}", now.width, now.height),
            ));
            let hertz = f64::from(now.refresh_mhz) / 1000.0;
            let rate = match now.refresh_mhz % 1000 {
                0 => format!("{} Hz", now.refresh_mhz / 1000),
                _ => format!("{hertz:.2} Hz"),
            };
            rows.push(stepped(Id::Refresh, crate::lang::t("Refresh rate"), rates > 1, rate));
        }
        let (marks, shown) = match monitor.profile.as_deref() {
            Some(path) => (
                vec![Mark::Text(crate::lang::t("Choose")), Mark::Text(crate::lang::t("Reset"))],
                std::path::Path::new(path)
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.to_string()),
            ),
            None => (
                vec![Mark::Text(crate::lang::t("Choose"))],
                crate::lang::t("None. Colors are shown as sRGB").to_string(),
            ),
        };
        rows.push(Row::new(Id::ColorProfile, crate::lang::t("Color profile"), Control::Buttons(marks)).detail(&shown));
        if monitor.hdr_able {
            rows.push(
                Row::new(Id::Hdr, "HDR", Control::Switch(monitor.hdr))
                    .detail(crate::lang::t("Brighter highlights and richer color on this HDR screen")),
            );
            if monitor.hdr {
                rows.push(
                    Row::new(
                        Id::HdrWhite,
                        crate::lang::t("Desktop brightness"),
                        Control::Slider {
                            least: f64::from(crate::hdr::LEAST_WHITE),
                            most: f64::from(crate::hdr::MOST_WHITE),
                            value: f64::from(monitor.hdr_white),
                            unit: " nits",
                            zero: None,
                        },
                    )
                    .detail(crate::lang::t("How bright windows and the desktop look while HDR is on")),
                );
            }
        }
        rows.push(
            Row::new(Id::Vrr, crate::lang::t("Adaptive sync"), Control::Switch(monitor.vrr))
                .detail(crate::lang::t("Smoother games and video on FreeSync and G-Sync monitors")),
        );
        if self.monitors.len() > 1 {
            let shown = match monitor.mirrors() {
                Some(source) => crate::lang::tf("Shows the same picture as {}", &[&(source)]),
                None => crate::lang::t("Off. This monitor has its own desktop").to_string(),
            };
            rows.push(Row::new(Id::Mirror, crate::lang::t("Mirror"), Control::Buttons(vec![Mark::Minus, Mark::Plus])).detail(&shown));
        }
        if self.monitors.len() > 1 {
            let others_on = self
                .monitors
                .iter()
                .any(|each| each.name != monitor.name && each.enabled);
            let mut row = Row::new(Id::MonitorOn, crate::lang::t("Use this monitor"), Control::Switch(monitor.enabled));
            if !others_on {
                row = row.detail(crate::lang::t("The last monitor in use stays on"));
                row.enabled = false;
            }
            rows.push(row);
        }
        rows
    }

    fn step_scale(&mut self, up: bool) {
        let Some(monitor) = self.monitors.get_mut(self.picked) else {
            return;
        };
        let now = monitor.scale;
        let next = match up {
            true => screens::SCALES.iter().copied().find(|s| *s > now + 1e-6),
            false => screens::SCALES
                .iter()
                .rev()
                .copied()
                .find(|s| *s < now - 1e-6),
        };
        if let Some(next) = next {
            monitor.scale = next;
            self.tidy_monitor();
        }
    }

    fn display_groups(&self) -> Vec<Group> {
        let Some(monitor) = self.monitors.get(self.picked) else {
            return vec![Group {
                header: None,
                rows: Vec::new(),
                footer: Some(crate::lang::t("Drifter has not shown any monitors yet.").into()),
                thumbs: None,
            }];
        };
        let size = monitor.head().size();
        let detail = crate::lang::tf("{} × {} pixels, seen as {} × {}", &[&(monitor.pixels.x), &(monitor.pixels.y), &(size.x), &(size.y)]);
        let marks = vec![Mark::Minus, Mark::Plus];
        vec![Group {
            header: None,
            rows: vec![
                Row::new(Id::MonitorName, &monitor.name, Control::Buttons(Vec::new()))
                    .detail(&detail),
                Row::new(Id::Scale, crate::lang::t("Scale"), Control::Buttons(marks)).detail(&crate::lang::tf("{}. Larger makes text and windows bigger", &[&(screens::scale_word(monitor.scale))])),
                Row::new(
                    Id::Turn,
                    crate::lang::t("Rotation"),
                    Control::Segments {
                        options: vec![crate::lang::t("None"), "90°", "180°", "270°"],
                        chosen: screens::TURNS
                            .iter()
                            .position(|turn| *turn == monitor.rotate)
                            .unwrap_or(0),
                    },
                ),
                Row::new(Id::Flip, crate::lang::t("Flip left to right"), Control::Switch(monitor.flip))
                    .detail(crate::lang::t("For a projector shown from behind, or a mirror rig")),
            ]
            .into_iter()
            .chain(self.mode_rows(monitor))
            .collect(),
            footer: Some(match self.monitors.len() {
                _ if self.display_note.is_some() => self.display_note.clone().unwrap_or_default(),
                1 => crate::lang::t("Plug in another monitor to arrange them side by side.").into(),
                _ => {
                    crate::lang::t("Click a monitor to choose it. Drag it to move it. Edges snap together.").into()
                }
            }),
            thumbs: None,
        }]
    }

    pub fn format(&self) -> String {
        let mut parts = Vec::new();
        if self.day {
            parts.push("%a".to_string());
        }
        if self.date {
            parts.push("%b %d".to_string());
        }
        let hours = if self.hours24 { "%H:%M" } else { "%I:%M" };
        let seconds = if self.seconds { ":%S" } else { "" };
        let half = if self.hours24 { "" } else { " %p" };
        parts.push(format!("{hours}{seconds}{half}"));
        parts.join(" ")
    }

    pub fn metrics(&self) -> Vec<String> {
        let mut out = Vec::new();
        if self.cpu {
            out.push("cpu".to_string());
        }
        if self.ram {
            out.push("ram".to_string());
        }
        out.extend(
            self.disks
                .iter()
                .filter(|disk| disk.on)
                .map(|disk| disk.point.clone()),
        );
        out.extend(self.kept.iter().cloned());
        out
    }

    pub fn every(&self) -> u64 {
        self.rotate * UNITS[self.unit.min(2)]
    }

    pub fn name_of(&self, item: &Launch) -> String {
        if let Some(title) = item.title() {
            return title.to_string();
        }
        match (item.app(), item.command()) {
            (Some(app), _) => self
                .names
                .get(app)
                .cloned()
                .unwrap_or_else(|| app.to_string()),
            (None, Some(command)) => command.to_string(),
            _ => String::new(),
        }
    }

    pub fn effective(&self, at: usize) -> Option<Launch> {
        self.defaults[at]
            .clone()
            .or_else(|| self.system[at].clone().map(Launch::App))
    }

    fn default_name(&self, at: usize) -> String {
        match (&self.defaults[at], self.effective(at)) {
            (Some(item), _) => self.name_of(item),
            (None, Some(item)) => crate::lang::tf("{} (system default)", &[&(self.name_of(&item))]),
            (None, None) => crate::lang::t("None found").to_string(),
        }
    }

    pub fn icon_key(item: &Launch) -> String {
        match (item.icon(), item.app()) {
            (Some(path), _) => format!("file:{}", path.display()),
            (None, Some(app)) => format!("app:{app}"),
            (None, None) => format!("cmd:{}", item.name()),
        }
    }

    fn launch_row(&self, id: Id, item: &Launch, marks: Vec<Mark>) -> Row {
        let detail = match (item.command(), item.icon()) {
            (Some(_), Some(_)) => crate::lang::t("Command line, own icon").to_string(),
            (Some(_), None) => crate::lang::t("Command line").to_string(),
            (None, Some(_)) => crate::lang::tf("{}, own icon", &[&(item.name())]),
            (None, None) => crate::lang::t("Application").to_string(),
        };
        let mut row = Row::new(id, &self.name_of(item), Control::Buttons(marks)).detail(&detail);
        row.icon = Some(Model::icon_key(item));
        row
    }

    pub(super) fn screen_off_row(&self) -> Row {
        Row::new(
            Id::ScreenOff,
            crate::lang::t("Turn the screen off after"),
            Control::Slider {
                least: 0.0,
                most: 60.0,
                value: self.screen_off as f64,
                unit: " min",
                zero: Some(crate::lang::t("Never")),
            },
        )
        .detail(crate::lang::t("Any key or pointer move turns it back on"))
    }

    pub fn nudge(&self, section: Section, id: Id, steps: f64) -> Option<Value> {
        self.groups(section)
            .into_iter()
            .flat_map(|group| group.rows)
            .filter(|row| row.id == id && row.enabled)
            .find_map(|row| match row.control {
                Control::Slider { least, most, value, .. } => {
                    let span = most - least;
                    let step = match span > 1000.0 {
                        true => (span / 100.0).round(),
                        false => 1.0,
                    };
                    Some(Value::Number((value.round() + step * steps).clamp(least, most)))
                }
                _ => None,
            })
    }

    pub fn groups(&self, section: Section) -> Vec<Group> {
        match section {
            Section::Desktop => {
                let mut groups = vec![Group {
                    header: None,
                    rows: vec![
                        Row::new(
                            Id::World,
                            crate::lang::t("World size"),
                            Control::Slider {
                                least: f64::from(WORLD_LEAST),
                                most: f64::from(WORLD_MOST),
                                value: f64::from(self.world),
                                unit: "",
                                zero: None,
                            },
                        )
                        .detail(crate::lang::t("Screens across and screens down")),
                    ],
                    footer: Some(crate::lang::tf("Hold {} and push an edge to pan, or scroll to zoom out.", &[&(self.modifier.name())])),
                    thumbs: None,
                }];
                groups.push(self.place_group());
                groups.extend(self.motion_groups());
                groups
            }
            Section::Wallpaper => self.wallpaper_groups(),
            Section::Window => vec![
                Group {
                    header: None,
                    rows: vec![
                        Row::new(
                            Id::Side,
                            crate::lang::t("Buttons"),
                            Control::Segments {
                                options: vec!["Left", "Right"],
                                chosen: match self.side {
                                    Side::Left => 0,
                                    Side::Right => 1,
                                },
                            },
                        ),
                        Row::new(
                            Id::Title,
                            crate::lang::t("Show the window title"),
                            Control::Switch(self.title),
                        ),
                    ],
                    footer: None,
                    thumbs: None,
                },
                Group {
                    header: Some(crate::lang::t("Button order")),
                    rows: self
                        .order
                        .iter()
                        .enumerate()
                        .map(|(slot, name)| {
                            Row::new(
                                Id::Order(slot),
                                button_label(*name),
                                Control::Buttons(vec![Mark::Up, Mark::Down]),
                            )
                        })
                        .collect(),
                    footer: Some(crate::lang::t("Left to right, as they sit in the title bar.").into()),
                    thumbs: None,
                },
            ],
            Section::Dock => {
                let mut rows: Vec<Row> = self
                    .dock
                    .iter()
                    .enumerate()
                    .map(|(at, item)| {
                        self.launch_row(
                            Id::Dock(at),
                            item,
                            vec![Mark::Up, Mark::Down, Mark::Text(crate::lang::t("Edit")), Mark::Minus],
                        )
                    })
                    .collect();
                rows.push(
                    Row::new(
                        Id::DockAdd,
                        crate::lang::t("Add an app or a command"),
                        Control::Buttons(vec![Mark::Plus]),
                    )
                    .faint(),
                );
                for (at, label) in SHOWS.iter().enumerate() {
                    let detail = match at {
                        0..=2 => [Role::Files, Role::Terminal, Role::Browser]
                            .get(at)
                            .and_then(|role| Role::ALL.iter().position(|each| each == role))
                            .and_then(|index| self.effective(index))
                            .map(|item| self.name_of(&item))
                            .unwrap_or_else(|| crate::lang::t("None found").to_string()),
                        3 => crate::lang::t("This window").to_string(),
                        4 => crate::lang::t("Opens the trash folder").to_string(),
                        _ => crate::lang::t("A grid of all your apps").to_string(),
                    };
                    rows.push(
                        Row::new(Id::DockShow(at), crate::lang::t(label), Control::Switch(self.shows[at]))
                            .detail(&detail),
                    );
                }
                vec![
                    Group {
                        header: None,
                        rows: vec![Row::new(Id::HideBottom, crate::lang::t("Auto hide"), Control::Switch(self.motion.hide_bottom))
                            .detail(crate::lang::t("Off keeps the dock on screen, and maximized windows leave room for it")),
                        Row::new(
                            Id::DockIcon,
                            crate::lang::t("Icon size"),
                            Control::Slider {
                                least: 16.0,
                                most: 128.0,
                                value: self.dock_icon,
                                unit: " px",
                                zero: None,
                            },
                        ),
                        Row::new(
                            Id::DockPadding,
                            crate::lang::t("Padding"),
                            Control::Slider {
                                least: 0.0,
                                most: 40.0,
                                value: self.dock_pad,
                                unit: " px",
                                zero: None,
                            },
                        )
                        .detail(crate::lang::t("Room around the icons. The dock is the icon size plus this above and below")),
                        Row::new(
                            Id::DockSpacing,
                            crate::lang::t("Icon spacing"),
                            Control::Slider {
                                least: 0.0,
                                most: 48.0,
                                value: self.dock_gap,
                                unit: " px",
                                zero: None,
                            },
                        )
                        .detail(crate::lang::t("Room between one icon and the next")),
                        Row::new(Id::DockFit, crate::lang::t("Fit to icons"), Control::Switch(self.dock_fit))
                            .detail(crate::lang::t("Only as wide as its icons. Off spans the whole screen")),
                        Row::new(
                            Id::DockHover,
                            crate::lang::t("Hover"),
                            Control::Segments {
                                options: vec![crate::lang::t("Off"), crate::lang::t("Lift"), crate::lang::t("Magnify")],
                                chosen: crate::config::DockHover::ALL
                                    .iter()
                                    .position(|h| *h == self.dock_hover)
                                    .unwrap_or(1),
                            },
                        )
                        .detail(crate::lang::t("What an icon does under the pointer")),
                        Row::new(
                            Id::DockMagnify,
                            crate::lang::t("Magnify"),
                            Control::Slider {
                                least: f64::from(crate::config::MAGNIFY.0),
                                most: f64::from(crate::config::MAGNIFY.1),
                                value: f64::from(self.dock_magnify),
                                unit: "%",
                                zero: None,
                            },
                        )
                        .enabled(self.dock_hover == crate::config::DockHover::Magnify),
                        ],
                        footer: None,
                        thumbs: None,
                    },
                    Group {
                        header: Some(crate::lang::t("Dock items")),
                        rows,
                        footer: Some(
                            crate::lang::t("The file manager, terminal and browser are your system defaults.")
                                .into(),
                        ),
                        thumbs: None,
                    },
                ]
            }
            Section::TopBar => {
                let mut info = vec![
                    Row::new(Id::Stats, crate::lang::t("Show system info"), Control::Switch(self.stats)),
                    Row::new(
                        Id::Glance,
                        crate::lang::t("Style"),
                        Control::Segments {
                            options: crate::glance::Style::ALL.iter().map(|style| style.label()).collect(),
                            chosen: self.glance,
                        },
                    )
                    .enabled(self.stats),
                    Row::new(Id::Cpu, "CPU", Control::Switch(self.cpu))
                        .detail(crate::lang::t("All cores together"))
                        .enabled(self.stats),
                    Row::new(Id::Ram, crate::lang::t("Memory"), Control::Switch(self.ram))
                        .detail(crate::lang::t("Memory in use"))
                        .enabled(self.stats),
                ];
                for (index, disk) in self.disks.iter().enumerate() {
                    info.push(
                        Row::new(Id::Disk(index), &disk.label, Control::Switch(disk.on))
                            .detail(&crate::lang::tf("Disk mounted at {}", &[&(disk.point)]))
                            .enabled(self.stats),
                    );
                }
                vec![
                    Group {
                        header: None,
                        rows: vec![
                            Row::new(Id::HideTop, crate::lang::t("Auto hide"), Control::Switch(self.motion.hide_top))
                                .detail(crate::lang::t("Off keeps the top bar on screen, and maximized windows leave room for it")),
                            Row::new(
                                Id::Top,
                                crate::lang::t("Bar height"),
                                Control::Slider {
                                    least: 16.0,
                                    most: 96.0,
                                    value: self.top,
                                    unit: " px",
                                    zero: None,
                                },
                            ),
                            Row::new(
                                Id::TextSize,
                                crate::lang::t("Text size"),
                                Control::Slider {
                                    least: 8.0,
                                    most: 32.0,
                                    value: self.text_size,
                                    unit: " px",
                                    zero: None,
                                },
                            ),
                            Row::new(Id::Battery, crate::lang::t("Battery"), Control::Switch(self.battery))
                                .detail(crate::lang::t("Charge and charging, left of the power button")),
                        ],
                        footer: Some(crate::lang::t("A computer with no battery never shows it.").into()),
                        thumbs: None,
                    },
                    Group {
                        header: Some(crate::lang::t("System info")),
                        rows: info,
                        footer: Some(
                            crate::lang::t("Point at the system info in the top bar for a card with every number.").into(),
                        ),
                        thumbs: None,
                    },
                ]
            }
            Section::Keyboard => vec![self.keyboard_group()],
            Section::Mouse => self.mouse_groups(),
            Section::Appearance => self.appearance_groups(),
            Section::Notifications => self.notify_groups(),
            Section::Sound => self.sound_groups(),
            Section::Shortcuts => self.shortcut_groups(),
            Section::Access => self.access_groups(),
            Section::Network => self.wifi_groups(),
            Section::Bluetooth => self.blue_groups(),
            Section::Power => self.power_groups(),
            Section::Time => self.time_groups(),
            Section::About => self.about_groups(),
            Section::Users => self.user_groups(),
            Section::Touchpad => self.touchpad_groups(),
            Section::Displays => {
                let mut groups = self.display_groups();
                groups.extend(self.brightness_group());
                groups.extend(self.night_groups());
                groups
            }
            Section::Lock => vec![Group {
                header: None,
                rows: vec![
                    Row::new(
                        Id::LockAfter,
                        crate::lang::t("Lock the screen after"),
                        Control::Slider {
                            least: 0.0,
                            most: 60.0,
                            value: self.lock_after as f64,
                            unit: " min",
                            zero: Some(crate::lang::t("Never")),
                        },
                    )
                    .detail(crate::lang::t("Minutes with no key press or pointer move")),
                    self.screen_off_row(),
                ],
                footer: Some(crate::lang::t("A video that is playing keeps the screen awake.").into()),
                thumbs: None,
            }, Group {
                header: Some(crate::lang::t("Login")),
                rows: vec![
                    Row::new(Id::AutoLogin, crate::lang::t("Log in automatically"), Control::Switch(self.automatic))
                        .detail(crate::lang::t("Skip the password when Drifter starts")),
                ],
                footer: Some(crate::lang::t("This counts when Drifter starts from a text console. A login screen such as GDM or SDDM asks for the password itself.").into()),
                thumbs: None,
            }],
            Section::Apps => vec![Group {
                header: None,
                rows: Role::ALL
                    .iter()
                    .enumerate()
                    .map(|(at, role)| {
                        let marks = match self.defaults[at].is_some() {
                            true => vec![Mark::Text(crate::lang::t("Choose")), Mark::Text(crate::lang::t("Reset"))],
                            false => vec![Mark::Text(crate::lang::t("Choose"))],
                        };
                        let mut row = Row::new(Id::Default(at), role.label(), Control::Buttons(marks))
                            .detail(&self.default_name(at));
                        row.icon = self.effective(at).map(|item| Model::icon_key(&item));
                        row
                    })
                    .collect(),
                footer: Some(
                    crate::lang::t("The dock, the trash and other apps open these. Reset goes back to the system's choice.")
                        .into(),
                ),
                thumbs: None,
            }],
            Section::Login => {
                let mut rows: Vec<Row> = self
                    .login
                    .iter()
                    .enumerate()
                    .map(|(at, item)| self.launch_row(Id::Login(at), item, vec![Mark::Minus]))
                    .collect();
                rows.push(
                    Row::new(
                        Id::LoginAdd,
                        crate::lang::t("Add an app or a command"),
                        Control::Buttons(vec![Mark::Plus]),
                    )
                    .faint(),
                );
                let mut found = vec![
                    Row::new(
                        Id::Autostart,
                        crate::lang::t("Start autostart apps"),
                        Control::Switch(self.autostart),
                    )
                    .detail(crate::lang::t("Apps that ask to start with every desktop")),
                ];
                for (at, entry) in self.autostarts.iter().enumerate() {
                    let mut row = Row::new(Id::AutoEntry(at), &entry.name, Control::Switch(entry.on))
                        .detail(&entry.exec)
                        .enabled(self.autostart);
                    row.icon = Some(format!("icon:{}", entry.icon.as_deref().unwrap_or(&entry.id)));
                    found.push(row);
                }
                vec![
                    Group {
                        header: Some(crate::lang::t("Start when Drifter starts")),
                        rows,
                        footer: Some(crate::lang::t("They start in this order, and again after a crash.").into()),
                        thumbs: None,
                    },
                    Group {
                        header: Some(crate::lang::t("Autostart")),
                        rows: found,
                        footer: Some(match self.autostarts.is_empty() {
                            true => "No app asks to start. Apps put a file in /etc/xdg/autostart to ask.".into(),
                            false => "From ~/.config/autostart and /etc/xdg/autostart. A login item above wins over the same app here.".into(),
                        }),
                        thumbs: None,
                    },
                ]
            }
        }
    }

    fn brightness_group(&self) -> Option<Group> {
        let level = self.brightness?;
        Some(Group {
            header: Some(crate::lang::t("Brightness")),
            rows: vec![
                Row::new(
                    Id::Brightness,
                    crate::lang::t("Brightness"),
                    Control::Slider {
                        least: BRIGHTNESS_LEAST,
                        most: 100.0,
                        value: (level * 100.0).round().max(BRIGHTNESS_LEAST),
                        unit: "%",
                        zero: None,
                    },
                )
                .detail(crate::lang::t("The built-in screen")),
            ],
            footer: Some(crate::lang::t("The brightness keys work everywhere.").into()),
            thumbs: None,
        })
    }

    fn night_groups(&self) -> Vec<Group> {
        let night = &self.night;
        let hour = |id, label: &str, value: u32| {
            Row::new(
                id,
                label,
                Control::Slider {
                    least: 0.0,
                    most: 23.0,
                    value: f64::from(value),
                    unit: ":00",
                    zero: None,
                },
            )
            .enabled(night.on)
        };
        vec![Group {
            header: Some(crate::lang::t("Night light")),
            rows: vec![
                Row::new(Id::NightOn, crate::lang::t("Night light"), Control::Switch(night.on))
                    .detail(crate::lang::t("Warmer colors at night")),
                Row::new(
                    Id::Warmth,
                    crate::lang::t("Warmth"),
                    Control::Slider {
                        least: f64::from(crate::night::WARMEST),
                        most: f64::from(crate::night::PLAIN),
                        value: f64::from(night.warmth),
                        unit: " K",
                        zero: None,
                    },
                )
                .detail(crate::lang::t("Lower is warmer"))
                .enabled(night.on),
                hour(Id::NightStart, crate::lang::t("Starts at"), night.start),
                hour(Id::NightEnd, crate::lang::t("Ends at"), night.end),
            ],
            footer: Some(
                crate::lang::t("Night runs from the start hour to the end hour, through midnight.").into(),
            ),
            thumbs: None,
        }]
    }

    pub fn choose_layout(&mut self, code: String) {
        if code != self.keyboard.layout {
            self.keyboard.variant.clear();
            self.variants = crate::keyboard::variants(&code);
        }
        self.keyboard.layout = code;
    }

    fn touchpad_groups(&self) -> Vec<Group> {
        let pad = &self.touchpad;
        vec![
            Group {
                header: None,
                rows: vec![
                    Row::new(
                        Id::Speed,
                        crate::lang::t("Pointer speed"),
                        Control::Slider {
                            least: 0.0,
                            most: f64::from(FASTEST),
                            value: f64::from(pad.speed),
                            unit: "",
                            zero: None,
                        },
                    ),
                    Row::new(Id::Accel, crate::lang::t("Acceleration"), Control::Switch(pad.accel))
                        .detail(crate::lang::t("A fast finger moves the pointer farther")),
                    Row::new(
                        Id::Natural,
                        crate::lang::t("Natural scrolling"),
                        Control::Switch(pad.natural),
                    )
                    .detail(crate::lang::t("The page follows your fingers")),
                    Row::new(
                        Id::Typing,
                        crate::lang::t("Ignore while typing"),
                        Control::Switch(pad.ignore_while_typing),
                    ),
                ],
                footer: None,
                thumbs: None,
            },
            Group {
                header: Some(crate::lang::t("Taps and swipes")),
                rows: vec![
                    Row::new(Id::Tap, crate::lang::t("Tap to click"), Control::Switch(pad.tap))
                        .detail(crate::lang::t("One finger is left, two is right")),
                    Row::new(
                        Id::ThreeTap,
                        crate::lang::t("Three finger tap"),
                        Control::Segments {
                            options: vec![crate::lang::t("Overview"), crate::lang::t("Middle")],
                            chosen: match pad.three_tap {
                                ThreeTap::Overview => 0,
                                ThreeTap::Middle => 1,
                            },
                        },
                    )
                    .enabled(pad.tap),
                    Row::new(Id::Swipe, crate::lang::t("Three finger swipe"), Control::Switch(pad.swipe))
                        .detail(crate::lang::t("Pans the camera")),
                ],
                footer: Some(
                    crate::lang::t("Overview zooms out like the hot corner. Tap again to fly back in.").into(),
                ),
                thumbs: None,
            },
        ]
    }

    fn clock_rows(&self) -> Vec<Row> {
        let free = self.custom.is_none();
        let mut rows = Vec::new();
        if let Some(format) = &self.custom {
            rows.push(
                Row::new(
                    Id::Custom,
                    crate::lang::t("Your own clock format"),
                    Control::Buttons(vec![Mark::Text(crate::lang::t("Use switches"))]),
                )
                .detail(format),
            );
        }
        rows.extend([
            Row::new(
                Id::Day,
                crate::lang::t("Show the day of the week"),
                Control::Switch(self.day),
            )
            .enabled(free),
            Row::new(Id::Date, crate::lang::t("Show the date"), Control::Switch(self.date)).enabled(free),
            Row::new(Id::Seconds, crate::lang::t("Show seconds"), Control::Switch(self.seconds)).enabled(free),
            Row::new(
                Id::Hours24,
                crate::lang::t("Use a 24-hour clock"),
                Control::Switch(self.hours24),
            )
            .enabled(free),
        ]);
        rows
    }

    fn wallpaper_groups(&self) -> Vec<Group> {
        let folder = self.folder.is_some();
        let turning = folder && self.rotate > 0;
        let folder_text = self
            .folder
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| crate::lang::t("None chosen").to_string());
        let mut groups = vec![
            Group {
                header: None,
                rows: vec![
                    Row::new(
                        Id::Picture,
                        crate::lang::t("Picture"),
                        Control::Buttons(vec![Mark::Text(crate::lang::t("Choose"))]),
                    )
                    .detail(&file_name(self.wallpaper.as_deref(), crate::lang::t("None chosen"))),
                    Row::new(
                        Id::WallpaperMode,
                        crate::lang::t("Behavior"),
                        Control::Segments {
                            options: vec![crate::lang::t("Fixed"), crate::lang::t("Pan"), crate::lang::t("Parallax")],
                            chosen: match self.wallpaper_mode {
                                WallpaperMode::Fixed => 0,
                                WallpaperMode::Pan => 1,
                                WallpaperMode::Parallax => 2,
                            },
                        },
                    ),
                    Row::new(Id::Ripple, crate::lang::t("Ripple"), Control::Switch(self.ripple))
                        .detail(crate::lang::t("A right click on the desktop sends a ripple across the wallpaper")),
                    Row::new(
                        Id::RippleStrength,
                        crate::lang::t("Ripple strength"),
                        Control::Slider {
                            least: f64::from(crate::ripple::STRENGTH.0),
                            most: f64::from(crate::ripple::STRENGTH.1),
                            value: f64::from(self.ripple_strength),
                            unit: "%",
                            zero: None,
                        },
                    )
                    .enabled(self.ripple),
                ],
                footer: Some(
                    match self.wallpaper_mode {
                        WallpaperMode::Fixed => crate::lang::t("The picture stays put while the camera moves."),
                        WallpaperMode::Pan => {
                            crate::lang::t("The picture covers the whole world and moves with it.")
                        }
                        WallpaperMode::Parallax => {
                            crate::lang::t("The picture moves slower than the world, for depth.")
                        }
                    }
                    .to_string(),
                ),
                thumbs: None,
            },
            Group {
                header: Some(crate::lang::t("Folder")),
                rows: vec![
                    Row::new(
                        Id::Folder,
                        crate::lang::t("Pictures from"),
                        Control::Buttons(vec![Mark::Text(crate::lang::t("Choose"))]),
                    )
                    .detail(&folder_text),
                    Row::new(
                        Id::Rotate,
                        crate::lang::t("Change the picture every"),
                        Control::Slider {
                            least: 0.0,
                            most: 60.0,
                            value: self.rotate as f64,
                            unit: ["s", "min", "h"][self.unit.min(2)],
                            zero: Some(crate::lang::t("Never")),
                        },
                    )
                    .enabled(folder),
                    Row::new(
                        Id::Unit,
                        crate::lang::t("Counted in"),
                        Control::Segments {
                            options: vec![crate::lang::t("Seconds"), crate::lang::t("Minutes"), crate::lang::t("Hours")],
                            chosen: self.unit.min(2),
                        },
                    )
                    .enabled(turning),
                    Row::new(
                        Id::Rotation,
                        crate::lang::t("Order"),
                        Control::Segments {
                            options: vec![crate::lang::t("Random"), crate::lang::t("By name")],
                            chosen: match self.rotation {
                                WallpaperOrder::Random => 0,
                                WallpaperOrder::Ordered => 1,
                            },
                        },
                    )
                    .enabled(turning),
                ],
                footer: None,
                thumbs: None,
            },
        ];
        let chosen_in = |list: &[PathBuf]| {
            self.wallpaper
                .as_ref()
                .and_then(|now| list.iter().position(|path| path == now))
        };
        groups.insert(
            1,
            Group {
                header: Some(crate::lang::t("Shaders")),
                rows: vec![
                    Row::new(
                        Id::Drift,
                        crate::lang::t("Motion"),
                        Control::Slider {
                            least: 0.0,
                            most: f64::from(crate::config::MOST_MOTION),
                            value: f64::from(self.drift),
                            unit: "%",
                            zero: Some(crate::lang::t("Still")),
                        },
                    ),
                    Row::new(
                        Id::WallpaperPointer,
                        crate::lang::t("Reacts to pointer"),
                        Control::Switch(self.wallpaper_pointer.unwrap_or_else(|| {
                            crate::shader::strong_gpu(&std::env::var("DRIFTER_RENDERER").unwrap_or_default())
                        })),
                    )
                    .detail(crate::lang::t("The shader stirs where the pointer moves on the bare desktop"))
                    .enabled(self.drift > 0),
                ],
                footer: Some(
                    match self.shaders.is_empty() {
                        true => "No shaders yet. Put .shader files in ~/.local/share/drifter/shaders.",
                        false => crate::lang::t("A shader wallpaper follows the time of day. Click one to use it. Motion sets how fast it drifts."),
                    }
                    .into(),
                ),
                thumbs: (!self.shaders.is_empty()).then(|| Thumbs {
                    shelf: Shelf::Shaders,
                    paths: self.shaders.clone(),
                    chosen: chosen_in(&self.shaders),
                }),
            },
        );
        if !self.pictures.is_empty() {
            groups.push(Group {
                header: Some(crate::lang::t("Pictures")),
                rows: Vec::new(),
                footer: Some(crate::lang::t("Click a picture to use it now.").into()),
                thumbs: Some(Thumbs {
                    shelf: Shelf::Pictures,
                    paths: self.pictures.clone(),
                    chosen: chosen_in(&self.pictures),
                }),
            });
        }
        groups
    }

    pub fn set(&mut self, id: Id, value: Value) {
        match (id, value) {
            (Id::World, Value::Number(n)) => {
                self.world = (n.round() as u32).clamp(WORLD_LEAST, WORLD_MOST);
            }
            (Id::Top, Value::Number(n)) => self.top = n.round().clamp(16.0, 96.0),
            (Id::Bottom, Value::Number(n)) => self.bottom = n.round().clamp(16.0, 160.0),
            (Id::TextSize, Value::Number(n)) => self.text_size = n.round().clamp(8.0, 32.0),
            (Id::LockAfter, Value::Number(n)) => {
                self.lock_after = n.round().clamp(0.0, 60.0) as u64
            }
            (Id::ScreenOff, Value::Number(n)) => {
                self.screen_off = n.round().clamp(0.0, 60.0) as u64
            }
            (Id::NightOn, Value::Switch(on)) => self.night.on = on,
            (Id::Brightness, Value::Number(n)) => {
                if self.brightness.is_some() {
                    self.brightness = Some(n.round().clamp(BRIGHTNESS_LEAST, 100.0) / 100.0);
                }
            }
            (Id::Warmth, Value::Number(n)) => {
                let (least, most) = (crate::night::WARMEST, crate::night::PLAIN);
                self.night.warmth =
                    ((n / 100.0).round() * 100.0).clamp(f64::from(least), f64::from(most)) as u32
            }
            (Id::NightStart, Value::Number(n)) => {
                self.night.start = n.round().clamp(0.0, 23.0) as u32
            }
            (Id::NightEnd, Value::Number(n)) => self.night.end = n.round().clamp(0.0, 23.0) as u32,
            (Id::RepeatDelay, Value::Number(n)) => {
                let (least, most) = REPEAT_DELAY;
                self.keyboard.repeat_delay =
                    n.round().clamp(f64::from(least), f64::from(most)) as u32
            }
            (Id::RepeatRate, Value::Number(n)) => {
                let (least, most) = REPEAT_RATE;
                self.keyboard.repeat_rate =
                    n.round().clamp(f64::from(least), f64::from(most)) as u32
            }
            (Id::Ripple, Value::Switch(on)) => self.ripple = on,
            (Id::WallpaperPointer, Value::Switch(on)) => self.wallpaper_pointer = Some(on),
            (Id::RippleStrength, Value::Number(n)) => {
                let (weak, strong) = crate::ripple::STRENGTH;
                self.ripple_strength = n.round().clamp(f64::from(weak), f64::from(strong)) as u32
            }
            (Id::Drift, Value::Number(n)) => {
                self.drift = n.round().clamp(0.0, f64::from(crate::config::MOST_MOTION)) as u32
            }
            (Id::Rotate, Value::Number(n)) => self.rotate = n.round().clamp(0.0, 60.0) as u64,
            (Id::Unit, Value::Choice(n)) => self.unit = n.min(2),
            (Id::Rotation, Value::Choice(n)) => {
                self.rotation = match n {
                    0 => WallpaperOrder::Random,
                    _ => WallpaperOrder::Ordered,
                }
            }
            (Id::Side, Value::Choice(n)) => {
                self.side = match n {
                    0 => Side::Left,
                    _ => Side::Right,
                }
            }
            (Id::Title, Value::Switch(on)) => self.title = on,
            (Id::DockShow(at), Value::Switch(on)) if at < 6 => self.shows[at] = on,
            (Id::WallpaperMode, Value::Choice(n)) => {
                self.wallpaper_mode = match n {
                    0 => WallpaperMode::Fixed,
                    1 => WallpaperMode::Pan,
                    _ => WallpaperMode::Parallax,
                }
            }
            (Id::Modifier, Value::Choice(n)) => {
                self.modifier = match n {
                    0 => Modifier::Super,
                    1 => Modifier::Alt,
                    _ => Modifier::Ctrl,
                }
            }
            (Id::Speed, Value::Number(n)) => {
                self.touchpad.speed = n.round().clamp(0.0, f64::from(FASTEST)) as u32
            }
            (Id::Accel, Value::Switch(on)) => self.touchpad.accel = on,
            (Id::Natural, Value::Switch(on)) => self.touchpad.natural = on,
            (Id::Typing, Value::Switch(on)) => self.touchpad.ignore_while_typing = on,
            (Id::Tap, Value::Switch(on)) => self.touchpad.tap = on,
            (Id::ThreeTap, Value::Choice(n)) => {
                self.touchpad.three_tap = match n {
                    0 => ThreeTap::Overview,
                    _ => ThreeTap::Middle,
                }
            }
            (Id::Swipe, Value::Switch(on)) => self.touchpad.swipe = on,
            (Id::MonitorOn, Value::Switch(on)) => {
                let picked = self.picked;
                let others_on = self
                    .monitors
                    .iter()
                    .enumerate()
                    .any(|(at, each)| at != picked && each.enabled);
                if let Some(monitor) = self.monitors.get_mut(picked)
                    && (on || others_on)
                {
                    monitor.enabled = on;
                }
            }
            (Id::Flip, Value::Switch(on)) => {
                if let Some(monitor) = self.monitors.get_mut(self.picked) {
                    monitor.flip = on;
                }
            }
            (Id::Vrr, Value::Switch(on)) => {
                if let Some(monitor) = self.monitors.get_mut(self.picked) {
                    monitor.vrr = on;
                }
            }
            (Id::Hdr, Value::Switch(on)) => {
                if let Some(monitor) = self.monitors.get_mut(self.picked) {
                    monitor.hdr = on;
                }
            }
            (Id::HdrWhite, Value::Number(n)) => {
                if let Some(monitor) = self.monitors.get_mut(self.picked) {
                    monitor.hdr_white = (n.round() as u32).clamp(crate::hdr::LEAST_WHITE, crate::hdr::MOST_WHITE);
                }
            }
            (Id::Turn, Value::Choice(n)) => {
                if let Some(monitor) = self.monitors.get_mut(self.picked) {
                    monitor.rotate = screens::TURNS[n.min(3)];
                    self.tidy_monitor();
                }
            }
            (Id::Day, Value::Switch(on)) => self.day = on,
            (Id::Date, Value::Switch(on)) => self.date = on,
            (Id::Seconds, Value::Switch(on)) => self.seconds = on,
            (Id::Hours24, Value::Switch(on)) => self.hours24 = on,
            (Id::Stats, Value::Switch(on)) => self.stats = on,
            (Id::Cpu, Value::Switch(on)) => self.cpu = on,
            (Id::Ram, Value::Switch(on)) => self.ram = on,
            (Id::Battery, Value::Switch(on)) => self.battery = on,
            (Id::Autostart, Value::Switch(on)) => self.autostart = on,
            (Id::AutoLogin, Value::Switch(on)) => self.automatic = on,
            (Id::DockIcon, Value::Number(n)) => self.dock_icon = n.round().clamp(16.0, 128.0),
            (Id::DockPadding, Value::Number(n)) => self.dock_pad = n.round().clamp(0.0, 40.0),
            (Id::DockFit, Value::Switch(on)) => self.dock_fit = on,
            (Id::DockHover, Value::Choice(n)) => self.dock_hover = crate::config::DockHover::ALL[n.min(2)],
            (Id::DockMagnify, Value::Number(n)) => {
                let (least, most) = crate::config::MAGNIFY;
                self.dock_magnify = n.round().clamp(f64::from(least), f64::from(most)) as u32
            }
            (Id::DockSpacing, Value::Number(n)) => self.dock_gap = n.round().clamp(0.0, 48.0),
            (Id::Glance, Value::Choice(at)) => self.glance = at.min(crate::glance::Style::ALL.len() - 1),
            (Id::AutoEntry(at), Value::Switch(on)) => {
                if let Some(entry) = self.autostarts.get_mut(at) {
                    entry.on = on;
                }
            }
            (Id::Disk(index), Value::Switch(on)) => {
                if let Some(disk) = self.disks.get_mut(index) {
                    disk.on = on;
                }
            }
            (id, value) => self.set_more(id, value),
        }
    }

    pub fn press(&mut self, id: Id, index: usize) -> Option<Ask> {
        if id != Id::ResetAll {
            self.confirm_reset = false;
        }
        match (id, index) {
            (Id::Order(slot), 0) if slot > 0 && slot < 3 => self.order.swap(slot, slot - 1),
            (Id::Order(slot), 1) if slot < 2 => self.order.swap(slot, slot + 1),
            (Id::Folder, _) => return Some(Ask::Files(For::Folder)),
            (Id::Layout, _) => return Some(Ask::Choose(For::Layout)),
            (Id::Picture, _) => return Some(Ask::Files(For::Picture)),
            (Id::Login(at), 0) if at < self.login.len() => {
                self.login.remove(at);
            }
            (Id::LoginAdd, _) => return Some(Ask::Pick(For::Login)),
            (Id::Dock(at), 0) if at > 0 && at < self.dock.len() => self.dock.swap(at, at - 1),
            (Id::Dock(at), 1) if at + 1 < self.dock.len() => self.dock.swap(at, at + 1),
            (Id::Dock(at), 2) if at < self.dock.len() => {
                return Some(Ask::EditDock(at));
            }
            (Id::Dock(at), 3) if at < self.dock.len() => {
                self.dock.remove(at);
            }
            (Id::DockAdd, _) => return Some(Ask::Pick(For::Dock)),
            (Id::Default(at), 0) if at < Role::ALL.len() => return Some(Ask::Pick(For::Default(Role::ALL[at]))),
            (Id::Default(at), 1) if at < Role::ALL.len() => self.defaults[at] = None,
            (Id::Custom, _) => self.custom = None,
            (Id::Scale, 0) => self.step_scale(false),
            (Id::Scale, 1) => self.step_scale(true),
            (Id::Resolution, at) => self.step_mode(true, at == 1),
            (Id::Refresh, at) => self.step_mode(false, at == 1),
            (Id::Mirror, at) => self.step_mirror(at == 1),
            (Id::ColorProfile, 0) => return Some(Ask::Files(For::ColorProfile)),
            (Id::ColorProfile, _) => {
                if let Some(monitor) = self.monitors.get_mut(self.picked) {
                    monitor.profile = None;
                }
            }
            (Id::MyName | Id::MyPicture | Id::MyPassword | Id::User(_) | Id::AddUser, index) => {
                return self.press_user(id, index);
            }
            (id, index) => return self.press_more(id, index),
        }
        None
    }

    pub fn add(&mut self, purpose: For, item: Launch) {
        match purpose {
            For::Login => self.login.push(item),
            For::Dock => self.dock.push(item),
            For::Default(role) => {
                if let Some(at) = Role::ALL.iter().position(|each| *each == role) {
                    self.defaults[at] = Some(item);
                }
            }
            _ => {}
        }
    }

    pub fn edit_dock(&mut self, at: usize, edit: DockEdit) -> Result<(), String> {
        let target = edit.target.trim().to_string();
        if target.is_empty() {
            return Err(crate::lang::t("Type an app or a command.").to_string());
        }
        let (app, command) = match edit.app {
            true => (Some(target), None),
            false => (None, Some(target)),
        };
        let plain = Launch::dressed(app.clone(), command.clone(), None, None, None, None);
        let name = edit.name.trim();
        let name = (!name.is_empty() && name != self.name_of(&plain)).then(|| name.to_string());
        let background = (edit.shape.is_some() && !edit.background).then_some(false);
        let Some(item) = self.dock.get_mut(at) else {
            return Err(crate::lang::t("That dock item is gone.").to_string());
        };
        *item = Launch::dressed(app, command, edit.icon, name, edit.shape, background);
        Ok(())
    }

    pub fn choose_folder(&mut self, folder: PathBuf) {
        self.pictures = crate::wallpaper::pictures(&folder);
        self.folder = Some(folder);
    }

    pub fn choose_picture(&mut self, shelf: Shelf, at: usize) {
        let list = match shelf {
            Shelf::Shaders => &self.shaders,
            Shelf::Pictures => &self.pictures,
        };
        if let Some(path) = list.get(at) {
            self.wallpaper = Some(path.clone());
        }
    }

    pub fn write(&self, before: &Model, text: &str) -> Result<String, String> {
        let mut doc = text
            .parse::<toml_edit::DocumentMut>()
            .map_err(|error| format!("the config no longer parses: {error}"))?;
        let int = |n: f64| toml_edit::Value::from(n.round() as i64);
        let word = |w: &str| toml_edit::Value::from(w);

        if self.world != before.world {
            put(&mut doc, "workspace", "width", int(f64::from(self.world)));
            put(&mut doc, "workspace", "height", int(f64::from(self.world)));
        }
        if self.wallpaper_mode != before.wallpaper_mode {
            let mode = match self.wallpaper_mode {
                WallpaperMode::Fixed => "fixed",
                WallpaperMode::Pan => "pan",
                WallpaperMode::Parallax => "parallax",
            };
            put(&mut doc, "workspace", "wallpaper_mode", word(mode));
        }
        if self.wallpaper != before.wallpaper
            && let Some(path) = &self.wallpaper
        {
            let shared = self.shaders.contains(path);
            let written = match (shared, path.file_name()) {
                (true, Some(name)) => name.to_string_lossy().into_owned(),
                _ => path.to_string_lossy().into_owned(),
            };
            put(&mut doc, "workspace", "wallpaper", word(&written));
        }
        if self.folder != before.folder
            && let Some(path) = &self.folder
        {
            put(
                &mut doc,
                "workspace",
                "wallpaper_folder",
                word(&path.to_string_lossy()),
            );
        }
        if self.every() != before.every() {
            put(
                &mut doc,
                "workspace",
                "wallpaper_every",
                int(self.every() as f64),
            );
        }
        if self.wallpaper_pointer != before.wallpaper_pointer
            && let Some(on) = self.wallpaper_pointer
        {
            put(&mut doc, "workspace", "wallpaper_pointer", toml_edit::Value::from(on));
        }
        if self.ripple != before.ripple {
            put(&mut doc, "workspace", "ripple", toml_edit::Value::from(self.ripple));
        }
        if self.ripple_strength != before.ripple_strength {
            put(&mut doc, "workspace", "ripple_strength", int(f64::from(self.ripple_strength)));
        }
        if self.drift != before.drift {
            put(&mut doc, "workspace", "wallpaper_motion", int(f64::from(self.drift)));
        }
        if self.rotation != before.rotation {
            let order = match self.rotation {
                WallpaperOrder::Random => "random",
                WallpaperOrder::Ordered => "ordered",
            };
            put(&mut doc, "workspace", "wallpaper_order", word(order));
        }
        if self.modifier != before.modifier {
            let key = match self.modifier {
                Modifier::Super => "super",
                Modifier::Alt => "alt",
                Modifier::Ctrl => "ctrl",
            };
            put(&mut doc, "input", "modifier", word(key));
        }
        let (pad, was) = (&self.touchpad, &before.touchpad);
        let switches = [
            ("tap", pad.tap, was.tap),
            ("swipe", pad.swipe, was.swipe),
            ("natural", pad.natural, was.natural),
            ("accel", pad.accel, was.accel),
            (
                "ignore_while_typing",
                pad.ignore_while_typing,
                was.ignore_while_typing,
            ),
        ];
        for (key, now, then) in switches {
            if now != then {
                put(&mut doc, "touchpad", key, toml_edit::Value::from(now));
            }
        }
        if pad.three_tap != was.three_tap {
            let tap = match pad.three_tap {
                ThreeTap::Overview => "overview",
                ThreeTap::Middle => "middle",
            };
            put(&mut doc, "touchpad", "three_tap", word(tap));
        }
        if pad.speed != was.speed {
            put(&mut doc, "touchpad", "speed", int(f64::from(pad.speed)));
        }
        let (night, was) = (&self.night, &before.night);
        if night.on != was.on {
            put(&mut doc, "night", "on", toml_edit::Value::from(night.on));
        }
        let numbers = [
            ("warmth", night.warmth, was.warmth),
            ("start", night.start, was.start),
            ("end", night.end, was.end),
        ];
        for (key, now, then) in numbers {
            if now != then {
                put(&mut doc, "night", key, int(f64::from(now)));
            }
        }
        let (keys, was) = (&self.keyboard, &before.keyboard);
        if keys.layout != was.layout {
            put(&mut doc, "keyboard", "layout", word(&keys.layout));
        }
        if keys.variant != was.variant {
            put(&mut doc, "keyboard", "variant", word(&keys.variant));
        }
        if keys.options != was.options {
            put(&mut doc, "keyboard", "options", word(&keys.options));
        }
        if keys.repeat_delay != was.repeat_delay {
            put(
                &mut doc,
                "keyboard",
                "repeat_delay",
                int(f64::from(keys.repeat_delay)),
            );
        }
        if keys.repeat_rate != was.repeat_rate {
            put(
                &mut doc,
                "keyboard",
                "repeat_rate",
                int(f64::from(keys.repeat_rate)),
            );
        }
        if self.top != before.top {
            put(&mut doc, "drawers", "top", int(self.top));
        }
        if self.bottom != before.bottom {
            put(&mut doc, "drawers", "bottom", int(self.bottom));
        }
        if self.text_size != before.text_size {
            put(&mut doc, "tray", "size", int(self.text_size));
        }
        if self.custom.is_none() && (self.format() != before.format() || before.custom.is_some()) {
            put(&mut doc, "tray", "format", word(&self.format()));
        }
        if self.stats != before.stats {
            put(
                &mut doc,
                "tray",
                "stats",
                toml_edit::Value::from(self.stats),
            );
        }
        if self.battery != before.battery {
            put(&mut doc, "tray", "battery", toml_edit::Value::from(self.battery));
        }
        if self.metrics() != before.metrics() {
            let list: toml_edit::Array = self.metrics().iter().map(String::as_str).collect();
            put(&mut doc, "tray", "metrics", toml_edit::Value::Array(list));
        }
        if self.side != before.side {
            let side = match self.side {
                Side::Left => "left",
                Side::Right => "right",
            };
            put(&mut doc, "window", "buttons", word(side));
        }
        if self.order != before.order {
            let list: toml_edit::Array = self.order.iter().map(|name| button_word(*name)).collect();
            put(&mut doc, "window", "order", toml_edit::Value::Array(list));
        }
        if self.title != before.title {
            put(
                &mut doc,
                "window",
                "title",
                toml_edit::Value::from(self.title),
            );
        }
        if self.inactive != before.inactive {
            put(&mut doc, "window", "inactive", word(self.inactive.word()));
        }
        if self.lock_after != before.lock_after {
            put(&mut doc, "lock", "after", int(self.lock_after as f64));
        }
        if self.screen_off != before.screen_off {
            put(&mut doc, "lock", "screen_off", int(self.screen_off as f64));
        }
        if self.login != before.login {
            put(&mut doc, "login", "start", launches(&self.login));
        }
        if self.autostart != before.autostart {
            put(
                &mut doc,
                "login",
                "autostart",
                toml_edit::Value::from(self.autostart),
            );
        }
        if self.glance != before.glance {
            let name = crate::glance::Style::ALL[self.glance.min(crate::glance::Style::ALL.len() - 1)].name();
            put(&mut doc, "tray", "style", toml_edit::Value::from(name));
        }
        if self.dock_icon != before.dock_icon {
            put(&mut doc, "dock", "icon", int(self.dock_icon));
        }
        if self.dock_pad != before.dock_pad {
            put(&mut doc, "dock", "padding", int(self.dock_pad));
        }
        if self.dock_gap != before.dock_gap {
            put(&mut doc, "dock", "spacing", int(self.dock_gap));
        }
        if self.dock_fit != before.dock_fit {
            put(&mut doc, "dock", "fit", toml_edit::Value::from(self.dock_fit));
        }
        if self.dock_hover != before.dock_hover {
            put(&mut doc, "dock", "hover", word(self.dock_hover.word()));
        }
        if self.dock_magnify != before.dock_magnify {
            put(&mut doc, "dock", "magnify", int(f64::from(self.dock_magnify)));
        }
        if self.automatic != before.automatic {
            put(
                &mut doc,
                "login",
                "automatic",
                toml_edit::Value::from(self.automatic),
            );
        }
        if self.dock != before.dock {
            put(&mut doc, "dock", "pinned", launches(&self.dock));
        }
        for (at, key) in Role::ALL.iter().map(|role| role.key()).enumerate() {
            if self.defaults[at] != before.defaults[at] {
                match &self.defaults[at] {
                    Some(item) => put(&mut doc, "apps", key, launch(item)),
                    None => drop_key(&mut doc, "apps", key),
                }
            }
        }
        if self.places != before.places {
            let mut list = toml_edit::ArrayOfTables::new();
            for place in &self.places {
                list.push(crate::place::table(place));
            }
            doc.remove("place");
            if !list.is_empty() {
                doc.insert("place", toml_edit::Item::ArrayOfTables(list));
            }
        }
        let monitors = self.monitor_entries();
        if monitors != before.monitor_entries() {
            put_monitors(&mut doc, &monitors);
        }
        for (at, key) in ["files", "terminal", "browser", "settings", "trash", "apps"]
            .iter()
            .enumerate()
        {
            if self.shows[at] != before.shows[at] {
                put(
                    &mut doc,
                    "dock",
                    key,
                    toml_edit::Value::from(self.shows[at]),
                );
            }
        }
        self.write_more(before, &mut doc);
        Ok(doc.to_string())
    }
}

pub fn export_path(folder: &std::path::Path, taken: impl Fn(&std::path::Path) -> bool) -> PathBuf {
    let first = folder.join("drifter.toml");
    if !taken(&first) {
        return first;
    }
    (2..)
        .map(|n| folder.join(format!("drifter-{n}.toml")))
        .find(|path| !taken(path))
        .unwrap_or(first)
}

pub fn check_import(text: &str) -> Result<(), String> {
    let config =
        Config::parse(text).map_err(|error| format!("not a Drifter settings file: {error}"))?;
    config
        .validate()
        .map_err(|error| format!("the file has a mistake: {error}"))
}

pub fn replace_config(path: &std::path::Path, text: &str) -> std::io::Result<PathBuf> {
    let backup = path.with_extension("toml.bak");
    if path.exists() {
        std::fs::copy(path, &backup)?;
    }
    let fresh = path.with_extension("toml.new");
    std::fs::write(&fresh, text)?;
    std::fs::rename(&fresh, path)?;
    Ok(backup)
}

fn file_name(path: Option<&std::path::Path>, none: &str) -> String {
    path.and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| none.to_string())
}

fn drop_key(doc: &mut toml_edit::DocumentMut, table: &str, key: &str) {
    if let Some(table) = doc.get_mut(table).and_then(|t| t.as_table_like_mut()) {
        table.remove(key);
    }
}

fn clean(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

fn button_label(name: ButtonName) -> &'static str {
    match name {
        ButtonName::Close => crate::lang::t("Close"),
        ButtonName::Minimize => crate::lang::t("Minimize"),
        ButtonName::Maximize => crate::lang::t("Maximize"),
    }
}

fn button_word(name: ButtonName) -> &'static str {
    match name {
        ButtonName::Close => "close",
        ButtonName::Minimize => "minimize",
        ButtonName::Maximize => "maximize",
    }
}

pub fn write_word(text: &str, table: &str, key: &str, word: &str) -> Result<String, String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("the config no longer parses: {error}"))?;
    put(&mut doc, table, key, toml_edit::Value::from(word));
    Ok(doc.to_string())
}

pub fn write_flag(text: &str, table: &str, key: &str, on: bool) -> Result<String, String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("the config no longer parses: {error}"))?;
    put(&mut doc, table, key, toml_edit::Value::from(on));
    Ok(doc.to_string())
}

pub fn write_number(text: &str, table: &str, key: &str, value: i64) -> Result<String, String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("the config no longer parses: {error}"))?;
    put(&mut doc, table, key, toml_edit::Value::from(value));
    Ok(doc.to_string())
}

pub fn write_app(text: &str, key: &str, item: &Launch) -> Result<String, String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("the config no longer parses: {error}"))?;
    put(&mut doc, "apps", key, launch(item));
    Ok(doc.to_string())
}

fn put_monitors(doc: &mut toml_edit::DocumentMut, monitors: &[MonitorConfig]) {
    let mut list = toml_edit::ArrayOfTables::new();
    for monitor in monitors {
        let mut table = toml_edit::Table::new();
        table.insert("name", toml_edit::value(monitor.name.as_str()));
        table.insert("scale", toml_edit::value(monitor.scale));
        table.insert("rotate", toml_edit::value(i64::from(monitor.rotate)));
        if let (Some(x), Some(y)) = (monitor.x, monitor.y) {
            table.insert("x", toml_edit::value(x));
            table.insert("y", toml_edit::value(y));
        }
        if let Some(mode) = &monitor.mode {
            table.insert("mode", toml_edit::value(mode.as_str()));
        }
        if !monitor.enabled {
            table.insert("enabled", toml_edit::value(false));
        }
        if monitor.flip {
            table.insert("flip", toml_edit::value(true));
        }
        if monitor.vrr {
            table.insert("vrr", toml_edit::value(true));
        }
        if let Some(source) = monitor.mirrors() {
            table.insert("mirror", toml_edit::value(source));
        }
        if let Some(profile) = monitor.profile.as_deref().filter(|path| !path.trim().is_empty()) {
            table.insert("profile", toml_edit::value(profile));
        }
        if monitor.hdr {
            table.insert("hdr", toml_edit::value(true));
        }
        if let Some(white) = monitor.hdr_white {
            table.insert("hdr_white", toml_edit::value(i64::from(white)));
        }
        list.push(table);
    }
    doc.remove("monitor");
    if !list.is_empty() {
        doc.insert("monitor", toml_edit::Item::ArrayOfTables(list));
    }
}

pub fn write_monitors(text: &str, monitors: &[MonitorConfig]) -> Result<String, String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("the config no longer parses: {error}"))?;
    put_monitors(&mut doc, monitors);
    Ok(doc.to_string())
}

pub fn write_scale(text: &str, name: &str, scale: f64) -> Result<String, String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("the config no longer parses: {error}"))?;
    let item = doc
        .entry("monitor")
        .or_insert(toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()));
    let Some(list) = item.as_array_of_tables_mut() else {
        return Err("monitor in the config is not a list of [[monitor]] tables".into());
    };
    let scale = (scale * 100.0).round() / 100.0;
    let held = list
        .iter_mut()
        .find(|table| table.get("name").and_then(|name| name.as_str()) == Some(name));
    if let Some(table) = held {
        table.insert("scale", toml_edit::value(scale));
        return Ok(doc.to_string());
    }
    let mut table = toml_edit::Table::new();
    table.insert("name", toml_edit::value(name));
    table.insert("scale", toml_edit::value(scale));
    list.push(table);
    Ok(doc.to_string())
}

const ASK_FRESH: std::time::Duration = std::time::Duration::from_secs(15);
const ASK_DOCK: &str = "dock-item";

fn ask_path() -> PathBuf {
    Config::state_dir().join("settings-ask")
}

pub fn ask_dock_item(at: usize) -> Result<(), String> {
    let path = ask_path();
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(|error| error.to_string())?;
    }
    std::fs::write(&path, format!("{ASK_DOCK} {at}\n")).map_err(|error| error.to_string())
}

pub fn take_dock_ask() -> Option<usize> {
    let path = ask_path();
    let fresh = std::fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|stamp| stamp.elapsed().ok())
        .is_some_and(|age| age < ASK_FRESH);
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    if !fresh {
        return None;
    }
    read_dock_ask(&text)
}

fn read_dock_ask(text: &str) -> Option<usize> {
    text.trim().strip_prefix(ASK_DOCK)?.trim().parse().ok()
}

#[derive(Debug, Clone, PartialEq)]
pub struct DockEdit {
    pub name: String,
    pub target: String,
    pub app: bool,
    pub icon: Option<PathBuf>,
    pub shape: Option<crate::config::IconShape>,
    pub background: bool,
}

pub fn write_dock(text: &str, pinned: &[Launch], off: &[&str]) -> Result<String, String> {
    let mut doc = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|error| format!("the config no longer parses: {error}"))?;
    put(&mut doc, "dock", "pinned", launches(pinned));
    for key in off {
        put(&mut doc, "dock", key, toml_edit::Value::from(false));
    }
    Ok(doc.to_string())
}

fn launches(items: &[Launch]) -> toml_edit::Value {
    let list: toml_edit::Array = items.iter().map(launch).collect();
    toml_edit::Value::Array(list)
}

fn launch(item: &Launch) -> toml_edit::Value {
    match item {
        Launch::App(app) => toml_edit::Value::from(app.as_str()),
        Launch::Full {
            app,
            command,
            icon,
            name,
            shape,
            background,
        } => {
            let mut table = toml_edit::InlineTable::new();
            if let Some(name) = name {
                table.insert("name", toml_edit::Value::from(name.as_str()));
            }
            if let Some(app) = app {
                table.insert("app", toml_edit::Value::from(app.as_str()));
            }
            if let Some(command) = command {
                table.insert("command", toml_edit::Value::from(command.as_str()));
            }
            if let Some(icon) = icon {
                table.insert(
                    "icon",
                    toml_edit::Value::from(icon.to_string_lossy().as_ref()),
                );
            }
            if let Some(shape) = shape {
                table.insert("shape", toml_edit::Value::from(shape.word()));
            }
            if let Some(background) = background {
                table.insert("background", toml_edit::Value::from(*background));
            }
            toml_edit::Value::InlineTable(table)
        }
    }
}

fn put(doc: &mut toml_edit::DocumentMut, table: &str, key: &str, fresh: toml_edit::Value) {
    let table = doc
        .entry(table)
        .or_insert(toml_edit::Item::Table(toml_edit::Table::new()));
    let Some(table) = table.as_table_like_mut() else {
        return;
    };
    match table.get_mut(key).and_then(|held| held.as_value_mut()) {
        Some(held) => {
            let decor = held.decor().clone();
            *held = fresh;
            *held.decor_mut() = decor;
        }
        None => {
            table.insert(key, toml_edit::Item::Value(fresh));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Id {
    World,
    Drift,
    Ripple,
    RippleStrength,
    WallpaperPointer,
    WallpaperMode,
    Modifier,
    Top,
    Bottom,
    TextSize,
    Day,
    Date,
    Seconds,
    Hours24,
    Stats,
    Cpu,
    Ram,
    Battery,
    Disk(usize),
    Side,
    Title,
    Order(usize),
    LockAfter,
    ScreenOff,
    Layout,
    RepeatDelay,
    RepeatRate,
    NightOn,
    Warmth,
    NightStart,
    NightEnd,
    Folder,
    Rotate,
    Unit,
    Rotation,
    Login(usize),
    LoginAdd,
    Autostart,
    AutoEntry(usize),
    AutoLogin,
    DockIcon,
    DockPadding,
    DockFit,
    DockHover,
    DockMagnify,
    DockSpacing,
    Glance,
    Dock(usize),
    DockAdd,
    DockShow(usize),
    Default(usize),
    Custom,
    Speed,
    Accel,
    Natural,
    Typing,
    Tap,
    ThreeTap,
    Swipe,
    Place(usize),
    ScreenReader,
    Language,
    WifiOn,
    Net(usize),
    WifiHidden,
    BlueOn,
    Gadget(usize),
    BlueName,
    BlueBattery,
    BlueWhen,
    BlueSound,
    BlueMic,
    BlueKey,
    BlueAddress,
    BlueFix,
    BlueForget,
    BlueReset,
    BlueStep(usize),
    BlueService,
    LargeText,
    HighContrast,
    ReduceMotion,
    StickyKeys,
    ShotFolder,
    ShotFormat,
    ShotClipboard,
    ShotTimer,
    ShotShadow,
    ShotPointer,
    ShotSound,
    ShotThumbnail,
    ShotJoined,
    Profile,
    MonitorName,
    Scale,
    Turn,
    Flip,
    Resolution,
    Refresh,
    MonitorOn,
    Vrr,
    Hdr,
    HdrWhite,
    Mirror,
    ColorProfile,
    Brightness,
    Picture,
    Variant,
    Caps,
    Compose,
    Options,
    MouseSpeed,
    MouseAccel,
    MouseNatural,
    SmoothScroll,
    LeftHanded,
    Animation,
    MinimizeEffect,
    HotCorner,
    CornerGlow,
    CornerSize,
    CornerWait,
    PanSpeed,
    Flight,
    HideTop,
    HideBottom,
    BarOpen,
    BarClose,
    BarSlide,
    Style,
    Inactive,
    ThemeChange,
    AccentColor,
    IconTheme,
    CursorTheme,
    CursorSize,
    Asset(usize),
    SleepAfter,
    Countdown,
    Lid,
    PowerButton,
    Quiet,
    NoteSeconds,
    NotePosition,
    History,
    Recent(usize),
    Forget,
    Shortcut(usize),
    Output,
    Volume,
    Mute,
    Input,
    Mic,
    MicMute,
    Zone,
    AutoTime,
    Info(usize),
    Export,
    Import,
    ResetAll,
    MyName,
    MyPicture,
    MyPassword,
    User(usize),
    AddUser,
}

const SHOWS: [&str; 6] = [
    "File manager",
    "Terminal",
    "Web browser",
    "Settings",
    "Trash",
    "Apps",
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    Switch(bool),
    Number(f64),
    Choice(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    Up,
    Down,
    Plus,
    Minus,
    Text(&'static str),
}

impl Mark {
    fn width(self) -> f64 {
        match self {
            Mark::Text(text) => 7.5 * text.chars().count() as f64 + 26.0,
            _ => 30.0,
        }
    }
}

const SEGMENTS_WIDE: f64 = 300.0;

#[derive(Debug, Clone, PartialEq)]
pub enum Control {
    Switch(bool),
    Slider {
        least: f64,
        most: f64,
        value: f64,
        unit: &'static str,
        zero: Option<&'static str>,
    },
    Segments {
        options: Vec<&'static str>,
        chosen: usize,
    },
    Buttons(Vec<Mark>),
    Swatches {
        colors: Vec<[u8; 3]>,
        chosen: Option<usize>,
    },
    Note(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub id: Id,
    pub label: String,
    pub detail: Option<String>,
    pub control: Control,
    pub enabled: bool,
    pub icon: Option<String>,
    pub faint: bool,
    pub opens: bool,
    pub editing: bool,
}

impl Row {
    fn new(id: Id, label: &str, control: Control) -> Row {
        Row {
            id,
            label: label.to_string(),
            detail: None,
            control,
            enabled: true,
            icon: None,
            faint: false,
            opens: false,
            editing: false,
        }
    }

    fn opens(mut self) -> Row {
        self.opens = true;
        self
    }

    fn detail(mut self, detail: &str) -> Row {
        self.detail = Some(detail.to_string());
        self
    }

    fn enabled(mut self, enabled: bool) -> Row {
        self.enabled = enabled;
        self
    }

    fn faint(mut self) -> Row {
        self.faint = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Thumbs {
    pub shelf: Shelf,
    pub paths: Vec<PathBuf>,
    pub chosen: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shelf {
    Shaders,
    Pictures,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub header: Option<&'static str>,
    pub rows: Vec<Row>,
    pub footer: Option<String>,
    pub thumbs: Option<Thumbs>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Search,
    Section(Section),
    Change(Id, Value),
    Press(Id, usize),
    Open(Id),
    Picture(Shelf, usize),
    Monitor(usize),
}

pub struct Fonts<'a> {
    pub body: &'a Font,
    pub small: &'a Font,
    pub title: &'a Font,
}

pub struct Art<'a> {
    pub wallpaper: Option<&'a Image>,
    pub icons: &'a HashMap<String, Image>,
    pub thumbs: &'a HashMap<PathBuf, Image>,
    pub sheet: Option<&'a Sheet>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub sidebar: f64,
    pub item: f64,
    pub pad: f64,
    pub row: f64,
    pub radius: f64,
    pub gap: f64,
    pub widest: f64,
    pub picture: f64,
    pub switch: Vec2,
    pub track: f64,
    pub reading: f64,
    pub segment: Vec2,
    pub thumb: Vec2,
    pub back: [u8; 3],
    pub side: [u8; 3],
    pub card: [u8; 3],
    pub rule: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub accent: [u8; 3],
    pub off: [u8; 3],
    pub pick: [u8; 3],
    pub side_scroll: f64,
}

impl Look {
    pub fn themed(p: &crate::theme::Palette) -> Look {
        Look {
            sidebar: 220.0,
            item: 28.0,
            pad: 28.0,
            row: 46.0,
            radius: 10.0,
            gap: 26.0,
            widest: 580.0,
            picture: 180.0,
            switch: Vec2::new(40.0, 24.0),
            track: 190.0,
            reading: 56.0,
            segment: Vec2::new(84.0, 26.0),
            thumb: Vec2::new(128.0, 72.0),
            back: p.well,
            side: p.surface,
            card: p.card,
            rule: p.rule,
            ink: p.ink,
            dim: p.dim,
            accent: [64, 128, 238],
            off: p.off,
            pick: p.pick,
            side_scroll: 0.0,
        }
    }
}

impl Default for Look {
    fn default() -> Self {
        Look::themed(&crate::theme::DARK)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub header: Option<Vec2>,
    pub card: Rect,
    pub rows: Vec<Rect>,
    pub thumbs: Vec<Rect>,
    pub footer: Option<Rect>,
}

const SIDE_TOP: f64 = 86.0;
const SIDE_HEAD: f64 = 30.0;
const THUMB_GAP: f64 = 10.0;

fn heads_through(shown: &[Section], index: usize) -> usize {
    let upto = index.min(shown.len().saturating_sub(1));
    (0..=upto)
        .filter(|at| shown.get(*at).is_some() && (*at == 0 || shown[*at].group() != shown[*at - 1].group()))
        .count()
}
const THUMB_PAD: f64 = 14.0;

impl Look {
    pub fn least(&self) -> Vec2 {
        Vec2::new(self.sidebar + 440.0, 440.0)
    }

    pub fn item_rect(&self, index: usize) -> Rect {
        self.item_rect_in(&Section::ALL, index)
    }

    pub fn item_rect_in(&self, shown: &[Section], index: usize) -> Rect {
        let heads = heads_through(shown, index);
        Rect::new(
            Vec2::new(
                10.0,
                SIDE_TOP + heads as f64 * SIDE_HEAD + index as f64 * (self.item + 2.0) - self.side_scroll,
            ),
            Vec2::new(self.sidebar - 20.0, self.item),
        )
    }

    pub fn head_rect_in(&self, shown: &[Section], index: usize) -> Rect {
        let item = self.item_rect_in(shown, index);
        Rect::new(Vec2::new(22.0, item.origin.y - SIDE_HEAD + 8.0), Vec2::new(self.sidebar - 34.0, SIDE_HEAD - 10.0))
    }

    pub fn search_rect(&self) -> Rect {
        Rect::new(Vec2::new(12.0, 44.0), Vec2::new(self.sidebar - 24.0, 28.0))
    }

    pub fn side_most(&self, height: f64) -> f64 {
        let count = Section::ALL.len();
        let whole = SIDE_TOP + heads_through(&Section::ALL, count) as f64 * SIDE_HEAD + count as f64 * (self.item + 2.0) + 10.0;
        (whole - height).max(0.0)
    }

    pub fn swatch_rects(&self, row: Rect, count: usize) -> Vec<Rect> {
        let (size, gap) = (22.0, 8.0);
        let whole = count as f64 * (size + gap) - gap;
        let left = row.max().x - 14.0 - whole;
        (0..count)
            .map(|slot| {
                Rect::new(
                    Vec2::new(
                        left + slot as f64 * (size + gap),
                        row.center().y - size / 2.0,
                    ),
                    Vec2::splat(size),
                )
            })
            .collect()
    }

    fn column(&self, size: Vec2) -> (f64, f64) {
        let room = size.x - self.sidebar - self.pad * 2.0;
        let width = room.min(self.widest).max(200.0);
        (self.sidebar + self.pad + (room - width) / 2.0, width)
    }

    pub fn title_at(&self, size: Vec2, scroll: f64) -> Vec2 {
        Vec2::new(self.column(size).0, 22.0 - scroll)
    }

    pub fn preview_size(&self) -> (u32, u32) {
        let tall = self.picture - 24.0;
        (crate::paint::dense(tall * 16.0 / 9.0), crate::paint::dense(tall))
    }

    pub fn about_logo_rect(&self, size: Vec2, scroll: f64) -> Rect {
        let card = self.picture_rect(size, scroll);
        Rect::from_center(card.center(), Vec2::splat(card.size.y))
    }

    fn paint_about(&self, canvas: &mut Canvas, size: Vec2, scroll: f64, model: &Model) {
        let logo = self.about_logo_rect(size, scroll);
        let drop = logo.center() + Vec2::new(0.0, logo.size.y * 0.24);
        canvas.shadow(Rect::from_center(drop, Vec2::new(logo.size.y * 0.42, 14.0)), 7.0, 16.0, 0.2);
        if !model.live_logo {
            canvas.circle(logo.center(), logo.size.y * 0.27, self.accent, 1.0);
            canvas.circle(logo.center() - Vec2::new(logo.size.y * 0.08, logo.size.y * 0.09), logo.size.y * 0.08, [255, 255, 255], 0.35);
        }
    }

    pub fn picture_rect(&self, size: Vec2, scroll: f64) -> Rect {
        let (x, width) = self.column(size);
        Rect::new(Vec2::new(x, 72.0 - scroll), Vec2::new(width, self.picture))
    }

    fn thumb_grid(&self, width: f64, count: usize) -> (usize, usize) {
        let room = width - THUMB_PAD * 2.0 + THUMB_GAP;
        let across = ((room / (self.thumb.x + THUMB_GAP)).floor() as usize).max(1);
        (across, count.div_ceil(across))
    }

    pub fn place(
        &self,
        size: Vec2,
        section: Section,
        groups: &[Group],
        scroll: f64,
    ) -> Vec<Placed> {
        let (x, width) = self.column(size);
        let mut y = 72.0 - scroll;
        if has_picture(section) {
            y += self.picture + self.gap;
        }
        let mut out = Vec::new();
        for group in groups {
            let header = group.header.map(|_| {
                let at = Vec2::new(x + 4.0, y);
                y += 24.0;
                at
            });
            let rows: Vec<Rect> = (0..group.rows.len())
                .map(|index| {
                    Rect::new(
                        Vec2::new(x, y + self.row * index as f64),
                        Vec2::new(width, self.row),
                    )
                })
                .collect();
            let mut tall = self.row * group.rows.len() as f64;
            let mut thumbs = Vec::new();
            if let Some(set) = &group.thumbs {
                let (across, down) = self.thumb_grid(width, set.paths.len());
                let used = across as f64 * (self.thumb.x + THUMB_GAP) - THUMB_GAP;
                let left = x + (width - used) / 2.0;
                for index in 0..set.paths.len() {
                    let (col, line) = (index % across, index / across);
                    thumbs.push(Rect::new(
                        Vec2::new(
                            left + col as f64 * (self.thumb.x + THUMB_GAP),
                            y + tall + THUMB_PAD + line as f64 * (self.thumb.y + THUMB_GAP),
                        ),
                        self.thumb,
                    ));
                }
                tall += THUMB_PAD * 2.0 + down as f64 * (self.thumb.y + THUMB_GAP) - THUMB_GAP;
            }
            let card = Rect::new(Vec2::new(x, y), Vec2::new(width, tall));
            y = card.max().y;
            let footer = group.footer.as_ref().map(|_| {
                let rect = Rect::new(Vec2::new(x + 4.0, y + 8.0), Vec2::new(width - 8.0, 20.0));
                y += 30.0;
                rect
            });
            y += self.gap;
            out.push(Placed {
                header,
                card,
                rows,
                thumbs,
                footer,
            });
        }
        out
    }

    pub fn content_height(&self, size: Vec2, section: Section, groups: &[Group]) -> f64 {
        self.place(size, section, groups, 0.0)
            .last()
            .map(|placed| {
                placed
                    .footer
                    .map(|f| f.max().y)
                    .unwrap_or(placed.card.max().y)
                    + self.pad
            })
            .unwrap_or(0.0)
    }

    pub fn max_scroll(&self, size: Vec2, section: Section, groups: &[Group]) -> f64 {
        (self.content_height(size, section, groups) - size.y).max(0.0)
    }

    pub fn paint_switch(&self, canvas: &mut Canvas, pill: Rect, t: f64, fade: f64) {
        let t = t.clamp(0.0, 1.0);
        let color = [0, 1, 2].map(|c| (f64::from(self.off[c]) + (f64::from(self.accent[c]) - f64::from(self.off[c])) * t).round() as u8);
        canvas.round_rect(pill, pill.size.y, color, fade);
        let knob = pill.size.y / 2.0 - 2.0;
        let left = pill.origin.x + 2.0 + knob;
        let right = pill.max().x - 2.0 - knob;
        let center = Vec2::new(left + (right - left) * t, pill.center().y);
        canvas.circle(center + Vec2::new(0.0, 1.0), knob + 0.5, [0, 0, 0], 0.25 * fade);
        canvas.circle(center, knob, [255, 255, 255], fade);
    }

    pub fn switch_rect(&self, row: Rect) -> Rect {
        Rect::new(
            Vec2::new(
                row.max().x - 14.0 - self.switch.x,
                row.center().y - self.switch.y / 2.0,
            ),
            self.switch,
        )
    }

    pub fn track_rect(&self, row: Rect) -> Rect {
        let right = row.max().x - 14.0 - self.reading;
        Rect::new(
            Vec2::new(right - self.track, row.center().y - 2.0),
            Vec2::new(self.track, 4.0),
        )
    }

    pub fn segment_rect(&self, row: Rect, count: usize, slot: usize) -> Rect {
        let each = self.segment.x.min(SEGMENTS_WIDE / count.max(1) as f64).floor();
        let whole = each * count as f64;
        let left = row.max().x - 14.0 - whole;
        Rect::new(
            Vec2::new(left + each * slot as f64, row.center().y - self.segment.y / 2.0),
            Vec2::new(each, self.segment.y),
        )
    }

    pub fn mark_rects(&self, row: Rect, marks: &[Mark]) -> Vec<Rect> {
        let mut right = row.max().x - 14.0;
        let mut out: Vec<Rect> = marks
            .iter()
            .rev()
            .map(|mark| {
                let width = mark.width();
                right -= width;
                let rect = Rect::new(
                    Vec2::new(right, row.center().y - 13.0),
                    Vec2::new(width, 26.0),
                );
                right -= 6.0;
                rect
            })
            .collect();
        out.reverse();
        out
    }

    fn slide(&self, row: Rect, least: f64, most: f64, x: f64) -> f64 {
        let track = self.track_rect(row);
        let along = ((x - track.origin.x) / track.size.x).clamp(0.0, 1.0);
        (least + (most - least) * along).round()
    }

    pub fn monitor_map(&self, size: Vec2, scroll: f64, model: &Model) -> (f64, Vec2) {
        let card = self.picture_rect(size, scroll);
        let area = Rect::new(
            card.origin + Vec2::new(24.0, 20.0),
            card.size - Vec2::new(48.0, 40.0),
        );
        let rects = model.monitor_rects();
        let whole = screens::bounds(&rects);
        let fit = (area.size.x / whole.size.x.max(1.0))
            .min(area.size.y / whole.size.y.max(1.0))
            .max(1e-3);
        let offset = area.center() - (whole.origin + whole.size / 2.0) * fit;
        (fit, offset)
    }

    pub fn monitor_under(
        &self,
        size: Vec2,
        scroll: f64,
        model: &Model,
        pointer: Vec2,
    ) -> Option<usize> {
        let (fit, offset) = self.monitor_map(size, scroll, model);
        model.monitor_rects().iter().position(|rect| {
            Rect::new(offset + rect.origin * fit, rect.size * fit).contains(pointer)
        })
    }

    pub fn hit(
        &self,
        size: Vec2,
        section: Section,
        model: &Model,
        scroll: f64,
        pointer: Vec2,
    ) -> Option<Hit> {
        if pointer.x < self.sidebar {
            if self.search_rect().contains(pointer) {
                return Some(Hit::Search);
            }
            if pointer.y < SIDE_TOP - 6.0 {
                return None;
            }
            return model
                .shown()
                .iter()
                .enumerate()
                .find(|(index, _)| self.item_rect_in(&model.shown(), *index).contains(pointer))
                .map(|(_, section)| Hit::Section(*section));
        }
        if section == Section::Displays
            && let Some(at) = self.monitor_under(size, scroll, model, pointer)
        {
            return Some(Hit::Monitor(at));
        }
        if section == Section::Users
            && model.me().is_some()
            && self
                .face_spot(self.picture_rect(size, scroll))
                .contains(pointer)
        {
            return Some(Hit::Press(Id::MyPicture, 0));
        }
        let groups = model.groups(section);
        let placed = self.place(size, section, &groups, scroll);
        for (group, placed) in groups.iter().zip(&placed) {
            if let (Some(set), Some(at)) = (
                group.thumbs.as_ref(),
                placed.thumbs.iter().position(|rect| rect.contains(pointer)),
            ) {
                return Some(Hit::Picture(set.shelf, at));
            }
            for (row, rect) in group.rows.iter().zip(&placed.rows) {
                if !rect.contains(pointer) || !row.enabled {
                    continue;
                }
                return match &row.control {
                    Control::Switch(on) => Some(Hit::Change(row.id, Value::Switch(!on))),
                    Control::Slider { least, most, .. } => {
                        let track = self.track_rect(*rect);
                        let near =
                            pointer.x >= track.origin.x - 12.0 && pointer.x <= track.max().x + 12.0;
                        near.then(|| {
                            Hit::Change(
                                row.id,
                                Value::Number(self.slide(*rect, *least, *most, pointer.x)),
                            )
                        })
                    }
                    Control::Segments { options, .. } => (0..options.len())
                        .find(|slot| {
                            self.segment_rect(*rect, options.len(), *slot)
                                .contains(pointer)
                        })
                        .map(|slot| Hit::Change(row.id, Value::Choice(slot))),
                    Control::Buttons(marks) => self
                        .mark_rects(*rect, marks)
                        .iter()
                        .position(|mark| mark.contains(pointer))
                        .map(|at| Hit::Press(row.id, at))
                        .or_else(|| row.opens.then_some(Hit::Open(row.id))),
                    Control::Swatches { colors, .. } => self
                        .swatch_rects(*rect, colors.len())
                        .iter()
                        .position(|spot| {
                            Rect::new(spot.origin - Vec2::splat(3.0), spot.size + Vec2::splat(6.0))
                                .contains(pointer)
                        })
                        .map(|at| Hit::Change(row.id, Value::Choice(at))),
                    Control::Note(_) => None,
                };
            }
        }
        None
    }

    pub fn drag(
        &self,
        size: Vec2,
        section: Section,
        model: &Model,
        scroll: f64,
        id: Id,
        x: f64,
    ) -> Option<Value> {
        let groups = model.groups(section);
        let placed = self.place(size, section, &groups, scroll);
        for (group, placed) in groups.iter().zip(&placed) {
            for (row, rect) in group.rows.iter().zip(&placed.rows) {
                if row.id != id {
                    continue;
                }
                if let Control::Slider { least, most, .. } = row.control {
                    return Some(Value::Number(self.slide(*rect, least, most, x)));
                }
            }
        }
        None
    }

    pub fn paint(
        &self,
        size: Vec2,
        section: Section,
        model: &Model,
        scroll: f64,
        fonts: &Fonts,
        art: &Art,
    ) -> Image {
        let mut canvas = Canvas::new(size.x.round() as u32, size.y.round() as u32);
        let all = Rect::new(Vec2::ZERO, size);
        canvas.round_rect(all, 0.0, self.back, 1.0);

        let groups = model.groups(section);
        let title = fonts.title.paint(section.name(), self.ink);
        canvas.stamp(&title, self.title_at(size, scroll), 1.0);

        if has_picture(section) {
            let rect = self.picture_rect(size, scroll);
            if section != Section::About {
                canvas.round_rect(rect, self.radius, self.card, 1.0);
            }
            match section {
                Section::Desktop => self.paint_world(&mut canvas, rect, model.world, fonts),
                Section::Window => self.paint_title_bar(&mut canvas, rect, model, fonts),
                Section::Displays => self.paint_monitors(&mut canvas, size, scroll, model, fonts),
                Section::Users => self.paint_me(&mut canvas, rect, model, fonts, art.icons),
                Section::About => self.paint_about(&mut canvas, size, scroll, model),
                _ => self.paint_wallpaper(&mut canvas, rect, art.wallpaper),
            }
        }

        for (group, placed) in groups
            .iter()
            .zip(self.place(size, section, &groups, scroll))
        {
            if let (Some(header), Some(at)) = (group.header, placed.header)
                && at.y < size.y
                && at.y > -40.0
            {
                let text = fonts.body.paint(header, self.dim);
                canvas.stamp(&text, at, 1.0);
            }
            canvas.round_rect(placed.card, self.radius, self.card, 1.0);
            for (index, (row, rect)) in group.rows.iter().zip(&placed.rows).enumerate() {
                if rect.max().y < 0.0 || rect.origin.y > size.y {
                    continue;
                }
                if index > 0 {
                    let line = Rect::new(
                        Vec2::new(rect.origin.x + 14.0, rect.origin.y),
                        Vec2::new(rect.size.x - 14.0, 1.0),
                    );
                    canvas.round_rect(line, 0.0, self.rule, 1.0);
                }
                self.paint_row(&mut canvas, row, *rect, fonts, art.icons, model.focus == Some(row.id));
            }
            if let Some(set) = &group.thumbs {
                for (index, rect) in placed.thumbs.iter().enumerate() {
                    if rect.max().y < 0.0 || rect.origin.y > size.y {
                        continue;
                    }
                    if set.chosen == Some(index) {
                        let ring =
                            Rect::new(rect.origin - Vec2::splat(3.0), rect.size + Vec2::splat(6.0));
                        canvas.round_rect(ring, 7.0, self.accent, 1.0);
                    }
                    match set.paths.get(index).and_then(|path| art.thumbs.get(path)) {
                        Some(image) => canvas.stamp(image, rect.origin, 1.0),
                        None => canvas.round_rect(*rect, 4.0, [22, 24, 31], 1.0),
                    }
                }
            }
            if let (Some(footer), Some(rect)) = (&group.footer, placed.footer)
                && rect.origin.y < size.y
                && rect.max().y > -40.0
            {
                let text = fonts
                    .small
                    .paint(&fit(fonts.small, footer, rect.size.x), self.dim);
                canvas.stamp(&text, rect.origin, 1.0);
            }
        }

        let side = Rect::new(Vec2::ZERO, Vec2::new(self.sidebar, size.y));
        canvas.round_rect(side, 0.0, self.side, 1.0);
        let edge = Rect::new(Vec2::new(self.sidebar - 1.0, 0.0), Vec2::new(1.0, size.y));
        canvas.round_rect(edge, 0.0, self.rule, 1.0);
        let shown = model.shown();
        for (index, each) in shown.iter().enumerate() {
            let rect = self.item_rect_in(&shown, index);
            if rect.max().y < SIDE_TOP - 6.0 - SIDE_HEAD || rect.origin.y > size.y {
                continue;
            }
            if index == 0 || shown[index - 1].group() != each.group() {
                let head = self.head_rect_in(&shown, index);
                let words = fonts.small.paint_weight(each.group(), self.dim, 600.0);
                let at = Vec2::new(head.origin.x, head.max().y - crate::paint::logical(&words).y);
                canvas.stamp(&words, at.round(), 0.85);
            }
            if *each == section {
                canvas.round_rect(rect, 7.0, self.accent, 1.0);
            }
            let icon = Rect::new(
                Vec2::new(rect.origin.x + 8.0, rect.center().y - 11.0),
                Vec2::splat(22.0),
            );
            canvas.round_rect(icon, 6.0, each.tint(), 1.0);
            paint_glyph(&mut canvas, *each, icon);
            let label = fonts.body.paint(each.name(), self.ink);
            let at = Vec2::new(
                icon.max().x + 10.0,
                rect.center().y - crate::paint::logical(&label).y / 2.0,
            );
            canvas.stamp(&label, at, 1.0);
        }
        let top = Rect::new(Vec2::ZERO, Vec2::new(self.sidebar - 1.0, SIDE_TOP - 6.0));
        canvas.round_rect(top, 0.0, self.side, 1.0);
        let heading = fonts.small.paint(crate::lang::t("Settings"), self.dim);
        canvas.stamp(&heading, Vec2::new(22.0, 18.0), 1.0);
        let field = self.search_rect();
        canvas.round_rect(field, 7.0, self.back, 1.0);
        let (words, ink) = match model.query.is_empty() {
            true => (crate::lang::t("Search").to_string(), self.dim),
            false => (format!("{}|", model.query), self.ink),
        };
        let text = fonts
            .body
            .paint(&fit(fonts.body, &words, field.size.x - 20.0), ink);
        let at = Vec2::new(
            field.origin.x + 10.0,
            field.center().y - crate::paint::logical(&text).y / 2.0,
        );
        canvas.stamp(&text, at, 1.0);
        if shown.is_empty() {
            let none = fonts.small.paint(crate::lang::t("Nothing matches"), self.dim);
            canvas.stamp(&none, Vec2::new(22.0, SIDE_TOP + 6.0), 1.0);
        }

        if let Some(sheet) = art.sheet {
            sheet.paint(&mut canvas, self, fonts, art.icons);
        }

        canvas.image
    }

    fn paint_row(
        &self,
        canvas: &mut Canvas,
        row: &Row,
        rect: Rect,
        fonts: &Fonts,
        icons: &HashMap<String, Image>,
        focused: bool,
    ) {
        let fade = if row.enabled { 1.0 } else { 0.4 };
        let mut x = rect.origin.x + 14.0;
        if let Some(key) = &row.icon {
            let spot = Rect::new(Vec2::new(x, rect.center().y - 13.0), Vec2::splat(26.0));
            match icons.get(key) {
                Some(image) => canvas.stamp(image, spot.center() - crate::paint::logical(&image) / 2.0, fade),
                None => crate::sheet::letter(canvas, self, fonts, spot, &row.label),
            }
            x = spot.max().x + 12.0;
        }
        let room = match &row.control {
            Control::Buttons(marks) => {
                self.mark_rects(rect, marks)
                    .first()
                    .map(|first| first.origin.x)
                    .unwrap_or(rect.max().x)
                    - x
                    - 10.0
            }
            Control::Note(text) if text.is_empty() => rect.max().x - x - 14.0,
            Control::Note(_) => rect.size.x * 0.32,
            Control::Swatches { colors, .. } => {
                self.swatch_rects(rect, colors.len())
                    .first()
                    .map(|first| first.origin.x)
                    .unwrap_or(rect.max().x)
                    - x
                    - 10.0
            }
            _ => rect.size.x * 0.55,
        };
        let ink = if row.faint { self.dim } else { self.ink };
        let label = fonts.body.paint(&fit(fonts.body, &row.label, room), ink);
        if row.editing {
            let well = Rect::new(Vec2::new(x - 8.0, rect.center().y - 15.0), Vec2::new(room + 8.0, 30.0));
            canvas.round_rect(well, 7.0, self.accent, fade);
            canvas.round_rect(Rect::new(well.origin + Vec2::splat(1.5), well.size - Vec2::splat(3.0)), 6.0, self.back, fade);
            let at = Vec2::new(x, rect.center().y - crate::paint::logical(&label).y / 2.0);
            canvas.stamp(&label, at, fade);
            let caret = (x + crate::paint::logical(&label).x + 2.0).min(well.max().x - 4.0);
            canvas.round_rect(Rect::new(Vec2::new(caret, rect.center().y - 8.0), Vec2::new(1.5, 16.0)), 0.7, self.accent, fade);
        } else {
        match &row.detail {
            Some(detail) => {
                let small = fonts.small.paint(&fit(fonts.small, detail, room), self.dim);
                let total = crate::paint::logical(&label).y + crate::paint::logical(&small).y;
                let top = rect.center().y - total / 2.0;
                canvas.stamp(&label, Vec2::new(x, top), fade);
                canvas.stamp(&small, Vec2::new(x, top + crate::paint::logical(&label).y), fade);
            }
            None => {
                let at = Vec2::new(x, rect.center().y - crate::paint::logical(&label).y / 2.0);
                canvas.stamp(&label, at, fade);
            }
        }
        }

        match &row.control {
            Control::Switch(on) => {
                let t = if *on { 1.0 } else { 0.0 };
                self.paint_switch(canvas, self.switch_rect(rect), t, fade);
            }
            Control::Slider {
                least,
                most,
                value,
                unit,
                zero,
            } => {
                let track = self.track_rect(rect);
                canvas.round_rect(track, 2.0, self.off, fade);
                let along = ((value - least) / (most - least).max(1e-9)).clamp(0.0, 1.0);
                let knob = Vec2::new(track.origin.x + track.size.x * along, track.center().y);
                let filled = Rect::new(
                    track.origin,
                    Vec2::new(knob.x - track.origin.x, track.size.y),
                );
                canvas.round_rect(filled, 2.0, self.accent, fade);
                if focused {
                    canvas.circle(knob, 12.5, self.accent, 0.45 * fade);
                }
                canvas.circle(knob + Vec2::new(0.0, 1.0), 9.5, [0, 0, 0], 0.3 * fade);
                canvas.circle(knob, 9.0, [255, 255, 255], fade);
                let words = match (zero, value.round() <= *least) {
                    (Some(zero), true) => zero.to_string(),
                    _ if unit.starts_with([' ', ':']) || unit.is_empty() => {
                        format!("{}{unit}", value.round() as i64)
                    }
                    _ => format!("{} {unit}", value.round() as i64),
                };
                let reading = fonts.body.paint(&words, self.ink);
                let at = Vec2::new(
                    rect.max().x - 14.0 - crate::paint::logical(&reading).x,
                    rect.center().y - crate::paint::logical(&reading).y / 2.0,
                );
                canvas.stamp(&reading, at, fade);
            }
            Control::Segments { options, chosen } => {
                let first = self.segment_rect(rect, options.len(), 0);
                let last = self.segment_rect(rect, options.len(), options.len() - 1);
                let whole = Rect::new(
                    first.origin,
                    Vec2::new(last.max().x - first.origin.x, first.size.y),
                );
                canvas.round_rect(whole, 7.0, self.off, fade);
                for (slot, name) in options.iter().enumerate() {
                    let cell = self.segment_rect(rect, options.len(), slot);
                    let picked = slot == *chosen;
                    if picked {
                        let inner =
                            Rect::new(cell.origin + Vec2::splat(2.0), cell.size - Vec2::splat(4.0));
                        canvas.round_rect(inner, 5.0, self.pick, fade);
                    }
                    let text = fonts
                        .body
                        .paint(name, if picked { self.ink } else { self.dim });
                    let at = cell.center()
                        - Vec2::new(crate::paint::logical(&text).x, crate::paint::logical(&text).y) / 2.0;
                    canvas.stamp(&text, at, fade);
                }
            }
            Control::Swatches { colors, chosen } => {
                for (slot, (color, spot)) in colors
                    .iter()
                    .zip(self.swatch_rects(rect, colors.len()))
                    .enumerate()
                {
                    if *chosen == Some(slot) {
                        canvas.circle(spot.center(), spot.size.x / 2.0 + 3.0, self.ink, fade);
                        canvas.circle(spot.center(), spot.size.x / 2.0 + 1.5, self.card, fade);
                    }
                    canvas.circle(spot.center(), spot.size.x / 2.0, *color, fade);
                }
            }
            Control::Note(text) => {
                let room = rect.size.x * 0.62 - 14.0;
                let painted = fonts.body.paint(&fit(fonts.body, text, room), self.dim);
                let at = Vec2::new(
                    rect.max().x - 14.0 - crate::paint::logical(&painted).x,
                    rect.center().y - crate::paint::logical(&painted).y / 2.0,
                );
                canvas.stamp(&painted, at, fade);
            }
            Control::Buttons(marks) => {
                for (mark, spot) in marks.iter().zip(self.mark_rects(rect, marks)) {
                    canvas.round_rect(spot, 7.0, self.off, fade);
                    let c = spot.center();
                    let white = self.ink;
                    match mark {
                        Mark::Up => canvas.triangle(
                            [
                                c + Vec2::new(0.0, -5.0),
                                c + Vec2::new(6.0, 4.0),
                                c + Vec2::new(-6.0, 4.0),
                            ],
                            white,
                            fade,
                        ),
                        Mark::Down => canvas.triangle(
                            [
                                c + Vec2::new(-6.0, -4.0),
                                c + Vec2::new(6.0, -4.0),
                                c + Vec2::new(0.0, 5.0),
                            ],
                            white,
                            fade,
                        ),
                        Mark::Plus => {
                            canvas.round_rect(
                                Rect::from_center(c, Vec2::new(12.0, 2.0)),
                                1.0,
                                white,
                                fade,
                            );
                            canvas.round_rect(
                                Rect::from_center(c, Vec2::new(2.0, 12.0)),
                                1.0,
                                white,
                                fade,
                            );
                        }
                        Mark::Minus => {
                            canvas.round_rect(
                                Rect::from_center(c, Vec2::new(12.0, 2.0)),
                                1.0,
                                white,
                                fade,
                            );
                        }
                        Mark::Text(text) => {
                            let painted = fonts.body.paint(text, white);
                            canvas.stamp(&painted, c - crate::paint::logical(&painted) / 2.0, fade);
                        }
                    }
                }
            }
        }
    }

    fn paint_monitors(
        &self,
        canvas: &mut Canvas,
        size: Vec2,
        scroll: f64,
        model: &Model,
        fonts: &Fonts,
    ) {
        let (zoom, offset) = self.monitor_map(size, scroll, model);
        for (index, (monitor, rect)) in model.monitors.iter().zip(model.monitor_rects()).enumerate()
        {
            let shown = Rect::new(offset + rect.origin * zoom, rect.size * zoom);
            let inner = Rect::new(
                shown.origin + Vec2::splat(1.5),
                shown.size - Vec2::splat(3.0),
            );
            let picked = index == model.picked;
            canvas.round_rect(inner, 4.0, [22, 24, 31], 1.0);
            let bar = Rect::new(
                inner.origin,
                Vec2::new(inner.size.x, 4.0_f64.min(inner.size.y)),
            );
            canvas.round_rect(bar, 2.0, self.rule, 1.0);
            outline(
                canvas,
                inner,
                2.0,
                if picked { self.accent } else { self.rule },
            );
            let name = fonts.small.paint(
                &fit(fonts.small, &monitor.name, inner.size.x - 8.0),
                self.ink,
            );
            let scale = fonts
                .small
                .paint(&screens::scale_word(monitor.scale), self.dim);
            let tall = crate::paint::logical(&name).y + crate::paint::logical(&scale).y;
            let top = inner.center().y - tall / 2.0;
            canvas.stamp(
                &name,
                Vec2::new(inner.center().x - crate::paint::logical(&name).x / 2.0, top),
                1.0,
            );
            canvas.stamp(
                &scale,
                Vec2::new(
                    inner.center().x - crate::paint::logical(&scale).x / 2.0,
                    top + crate::paint::logical(&name).y,
                ),
                1.0,
            );
        }
    }

    fn paint_world(&self, canvas: &mut Canvas, card: Rect, world: u32, fonts: &Fonts) {
        let tall = card.size.y - 48.0;
        let size = Vec2::new(tall * 16.0 / 9.0, tall);
        let area = Rect::new(
            Vec2::new(card.center().x - size.x / 2.0, card.origin.y + 16.0),
            size,
        );
        canvas.round_rect(area, 4.0, [22, 24, 31], 1.0);
        let n = f64::from(world.max(1));
        let cell = Vec2::new(area.size.x / n, area.size.y / n);
        for step in 1..world {
            let x = area.origin.x + cell.x * f64::from(step);
            let y = area.origin.y + cell.y * f64::from(step);
            canvas.round_rect(
                Rect::new(Vec2::new(x, area.origin.y), Vec2::new(1.0, area.size.y)),
                0.0,
                self.rule,
                1.0,
            );
            canvas.round_rect(
                Rect::new(Vec2::new(area.origin.x, y), Vec2::new(area.size.x, 1.0)),
                0.0,
                self.rule,
                1.0,
            );
        }
        let view = Rect::from_center(area.center(), cell);
        canvas.round_rect(view, 2.0, self.accent, 0.35);
        outline(canvas, view, 2.0, self.accent);
        let caption = fonts.small.paint(
            &crate::lang::tf("{} × {} screens. The colored screen is what you see at zoom 1.", &[&(world), &(world)]),
            self.dim,
        );
        let at = Vec2::new(
            card.center().x - crate::paint::logical(&caption).x / 2.0,
            area.max().y + 8.0,
        );
        canvas.stamp(&caption, at, 1.0);
    }

    fn paint_title_bar(&self, canvas: &mut Canvas, card: Rect, model: &Model, fonts: &Fonts) {
        let window = Rect::from_center(card.center(), Vec2::new(card.size.x * 0.7, 120.0));
        canvas.shadow(window, 10.0, 10.0, 0.4);
        canvas.round_rect(window, 10.0, [30, 33, 42], 1.0);
        let content = Rect::new(
            window.origin + Vec2::new(4.0, 30.0),
            window.size - Vec2::new(8.0, 34.0),
        );
        canvas.round_rect(content, 4.0, [22, 24, 31], 1.0);
        let dots = 3.0 * 12.0 + 2.0 * 8.0;
        let start = match model.side {
            Side::Left => window.origin.x + 11.0,
            Side::Right => window.max().x - 11.0 - dots,
        };
        for (slot, name) in model.order.iter().enumerate() {
            let color = match name {
                ButtonName::Close => [255, 95, 87],
                ButtonName::Minimize => [254, 188, 46],
                ButtonName::Maximize => [40, 200, 64],
            };
            let center = Vec2::new(start + 6.0 + slot as f64 * 20.0, window.origin.y + 15.0);
            canvas.circle(center, 6.0, color, 1.0);
        }
        if model.title {
            let text = fonts.small.paint(crate::lang::t("Window title"), self.dim);
            let at = Vec2::new(
                window.center().x - crate::paint::logical(&text).x / 2.0,
                window.origin.y + 15.0 - crate::paint::logical(&text).y / 2.0,
            );
            canvas.stamp(&text, at, 1.0);
        }
    }

    fn paint_wallpaper(&self, canvas: &mut Canvas, card: Rect, wallpaper: Option<&Image>) {
        let inner = Rect::new(
            card.origin + Vec2::splat(12.0),
            card.size - Vec2::splat(24.0),
        );
        let size = Vec2::new(inner.size.y * 16.0 / 9.0, inner.size.y);
        let shown = Rect::new(
            Vec2::new(inner.center().x - size.x / 2.0, inner.origin.y),
            size,
        );
        match wallpaper {
            Some(art) => {
                let (wide, tall) = (crate::paint::dense(shown.size.x), crate::paint::dense(shown.size.y));
                match (art.width, art.height) == (wide, tall) {
                    true => canvas.stamp(art, shown.origin, 1.0),
                    false => canvas.stamp(&cover(art, wide, tall), shown.origin, 1.0),
                }
            }
            None => canvas.round_rect(shown, 6.0, [22, 24, 31], 1.0),
        }
    }
}

fn outline(canvas: &mut Canvas, rect: Rect, thick: f64, color: [u8; 3]) {
    for side in [
        Rect::new(rect.origin, Vec2::new(rect.size.x, thick)),
        Rect::new(
            Vec2::new(rect.origin.x, rect.max().y - thick),
            Vec2::new(rect.size.x, thick),
        ),
        Rect::new(rect.origin, Vec2::new(thick, rect.size.y)),
        Rect::new(
            Vec2::new(rect.max().x - thick, rect.origin.y),
            Vec2::new(thick, rect.size.y),
        ),
    ] {
        canvas.round_rect(side, 0.0, color, 1.0);
    }
}

fn has_picture(section: Section) -> bool {
    matches!(
        section,
        Section::Desktop
            | Section::Displays
            | Section::Wallpaper
            | Section::Window
            | Section::Users
            | Section::About
    )
}

pub fn cover(art: &Image, width: u32, height: u32) -> Image {
    let want = f64::from(width) / f64::from(height.max(1));
    let have = f64::from(art.width) / f64::from(art.height.max(1));
    let (cw, ch) = if have > want {
        ((f64::from(art.height) * want) as u32, art.height)
    } else {
        (art.width, (f64::from(art.width) / want) as u32)
    };
    let (cw, ch) = (
        cw.clamp(1, art.width.max(1)),
        ch.clamp(1, art.height.max(1)),
    );
    let (left, top) = ((art.width - cw) / 2, (art.height - ch) / 2);
    let mut rgba = Vec::with_capacity((cw * ch * 4) as usize);
    for y in top..top + ch {
        let from = ((y * art.width + left) * 4) as usize;
        rgba.extend_from_slice(&art.rgba[from..from + (cw * 4) as usize]);
    }
    Image {
        width: cw,
        height: ch,
        rgba,
    }
    .scaled(width, height)
}

fn fit(font: &Font, text: &str, width: f64) -> String {
    if font.width_of(text) <= width {
        return text.to_string();
    }
    let mut out = String::new();
    for glyph in text.chars() {
        if font.width_of(&format!("{out}{glyph}…")) > width {
            break;
        }
        out.push(glyph);
    }
    format!("{}…", out.trim_end())
}

fn paint_glyph(canvas: &mut Canvas, section: Section, icon: Rect) {
    let white = [255, 255, 255];
    let o = icon.origin;
    let r = |x: f64, y: f64, w: f64, h: f64| Rect::new(o + Vec2::new(x, y), Vec2::new(w, h));
    match section {
        Section::Desktop => {
            for (x, y) in [(5.0, 5.0), (12.0, 5.0), (5.0, 12.0), (12.0, 12.0)] {
                canvas.round_rect(r(x, y, 5.0, 5.0), 1.0, white, 1.0);
            }
        }
        Section::Wallpaper => {
            canvas.circle(o + Vec2::new(15.0, 7.0), 2.5, white, 1.0);
            canvas.triangle(
                [
                    o + Vec2::new(4.0, 17.0),
                    o + Vec2::new(10.0, 8.0),
                    o + Vec2::new(16.0, 17.0),
                ],
                white,
                1.0,
            );
        }
        Section::Window => {
            canvas.round_rect(r(4.0, 5.0, 14.0, 12.0), 2.0, white, 1.0);
            canvas.round_rect(r(5.5, 9.0, 11.0, 6.5), 1.0, section.tint(), 1.0);
        }
        Section::Dock => {
            canvas.round_rect(r(4.0, 13.0, 14.0, 5.0), 2.0, white, 1.0);
            canvas.round_rect(r(4.0, 5.0, 14.0, 6.0), 1.5, white, 0.45);
        }
        Section::TopBar => {
            canvas.round_rect(r(4.0, 4.0, 14.0, 5.0), 2.0, white, 1.0);
            canvas.round_rect(r(4.0, 11.0, 14.0, 7.0), 1.5, white, 0.45);
        }
        Section::Appearance => {
            canvas.circle(o + Vec2::new(11.0, 11.0), 7.0, white, 1.0);
            canvas.round_rect(r(11.0, 4.0, 7.0, 14.0), 0.0, section.tint(), 1.0);
            canvas.circle(o + Vec2::new(11.0, 11.0), 1.0, white, 1.0);
        }
        Section::Notifications => {
            canvas.circle(o + Vec2::new(11.0, 10.0), 5.5, white, 1.0);
            canvas.round_rect(r(5.5, 10.0, 11.0, 5.0), 1.0, white, 1.0);
            canvas.round_rect(r(4.0, 14.5, 14.0, 1.5), 0.5, white, 1.0);
            canvas.circle(o + Vec2::new(11.0, 17.5), 1.6, white, 1.0);
        }
        Section::Sound => {
            canvas.round_rect(r(4.0, 8.5, 4.0, 5.0), 0.5, white, 1.0);
            canvas.triangle(
                [
                    o + Vec2::new(6.0, 11.0),
                    o + Vec2::new(12.0, 5.0),
                    o + Vec2::new(12.0, 17.0),
                ],
                white,
                1.0,
            );
            canvas.round_rect(r(14.0, 9.0, 1.5, 4.0), 0.7, white, 1.0);
            canvas.round_rect(r(17.0, 7.0, 1.5, 8.0), 0.7, white, 1.0);
        }
        Section::Keyboard => {
            canvas.round_rect(r(3.0, 6.0, 16.0, 10.0), 2.0, white, 1.0);
            let key = section.tint();
            for x in [5.5, 9.0, 12.5] {
                canvas.round_rect(r(x, 8.0, 2.5, 2.0), 0.5, key, 1.0);
            }
            canvas.round_rect(r(6.0, 12.0, 10.0, 2.0), 0.5, key, 1.0);
        }
        Section::Mouse => {
            canvas.round_rect(r(6.5, 3.0, 9.0, 16.0), 4.5, white, 1.0);
            canvas.round_rect(r(10.5, 5.0, 1.0, 4.0), 0.5, section.tint(), 1.0);
        }
        Section::Shortcuts => {
            canvas.round_rect(r(3.0, 7.0, 7.0, 8.0), 1.5, white, 1.0);
            canvas.round_rect(r(12.0, 7.0, 7.0, 8.0), 1.5, white, 1.0);
            canvas.round_rect(r(10.0, 10.5, 2.0, 1.0), 0.5, white, 1.0);
        }
        Section::Power => {
            canvas.circle(o + Vec2::new(11.0, 12.0), 6.5, white, 1.0);
            canvas.circle(o + Vec2::new(11.0, 12.0), 4.5, section.tint(), 1.0);
            canvas.round_rect(r(9.0, 3.0, 4.0, 7.0), 1.0, section.tint(), 1.0);
            canvas.round_rect(r(10.0, 3.5, 2.0, 8.0), 1.0, white, 1.0);
        }
        Section::Time => {
            canvas.circle(o + Vec2::new(11.0, 11.0), 7.5, white, 1.0);
            canvas.round_rect(r(10.2, 6.0, 1.6, 5.8), 0.8, section.tint(), 1.0);
            canvas.round_rect(r(10.2, 10.2, 4.8, 1.6), 0.8, section.tint(), 1.0);
        }
        Section::About => {
            canvas.circle(o + Vec2::new(11.0, 11.0), 7.5, white, 1.0);
            canvas.circle(o + Vec2::new(11.0, 7.5), 1.3, section.tint(), 1.0);
            canvas.round_rect(r(10.0, 10.0, 2.0, 6.0), 1.0, section.tint(), 1.0);
        }
        Section::Displays => {
            canvas.round_rect(r(3.0, 4.0, 16.0, 11.0), 2.0, white, 1.0);
            canvas.round_rect(r(5.0, 6.0, 12.0, 7.0), 1.0, section.tint(), 1.0);
            canvas.round_rect(r(9.0, 15.0, 4.0, 2.5), 0.5, white, 1.0);
            canvas.round_rect(r(7.0, 17.0, 8.0, 1.5), 0.5, white, 1.0);
        }
        Section::Touchpad => {
            canvas.round_rect(r(3.0, 4.0, 16.0, 14.0), 2.5, white, 1.0);
            let dot = section.tint();
            for x in [7.0, 11.0, 15.0] {
                canvas.circle(o + Vec2::new(x, 11.0), 1.5, dot, 1.0);
            }
        }
        Section::Network => {
            for (at, tall) in [(0.0, 4.0), (1.0, 8.0), (2.0, 12.0)] {
                canvas.round_rect(r(5.0 + at * 4.5, 17.0 - tall, 3.0, tall), 1.0, white, 1.0);
            }
        }
        Section::Bluetooth => {
            canvas.round_rect(r(10.0, 3.0, 2.0, 16.0), 1.0, white, 1.0);
            canvas.triangle(
                [o + Vec2::new(11.0, 3.0), o + Vec2::new(16.0, 7.5), o + Vec2::new(11.0, 11.0)],
                white,
                1.0,
            );
            canvas.triangle(
                [o + Vec2::new(11.0, 11.0), o + Vec2::new(16.0, 14.5), o + Vec2::new(11.0, 19.0)],
                white,
                1.0,
            );
            canvas.triangle(
                [o + Vec2::new(11.0, 4.5), o + Vec2::new(14.0, 7.5), o + Vec2::new(11.0, 9.5)],
                section.tint(),
                1.0,
            );
            canvas.triangle(
                [o + Vec2::new(11.0, 12.5), o + Vec2::new(14.0, 14.5), o + Vec2::new(11.0, 17.5)],
                section.tint(),
                1.0,
            );
            canvas.round_rect(r(6.0, 6.5, 5.5, 1.6), 0.8, white, 1.0);
            canvas.round_rect(r(6.0, 14.0, 5.5, 1.6), 0.8, white, 1.0);
        }
        Section::Access => {
            canvas.circle(o + Vec2::new(11.0, 5.0), 2.0, white, 1.0);
            canvas.round_rect(r(4.0, 8.0, 14.0, 2.0), 1.0, white, 1.0);
            canvas.round_rect(r(9.5, 8.0, 3.0, 6.0), 1.0, white, 1.0);
            canvas.round_rect(r(8.0, 13.0, 2.0, 6.0), 1.0, white, 1.0);
            canvas.round_rect(r(12.0, 13.0, 2.0, 6.0), 1.0, white, 1.0);
        }
        Section::Lock => {
            canvas.circle(o + Vec2::new(11.0, 9.0), 4.5, white, 1.0);
            canvas.circle(o + Vec2::new(11.0, 9.0), 2.5, section.tint(), 1.0);
            canvas.round_rect(r(5.0, 10.0, 12.0, 8.0), 1.5, white, 1.0);
        }
        Section::Apps => {
            for (x, y) in [(6.5, 6.5), (15.5, 6.5), (6.5, 15.5), (15.5, 15.5)] {
                canvas.circle(o + Vec2::new(x, y), 3.0, white, 1.0);
            }
        }
        Section::Users => {
            canvas.circle(o + Vec2::new(11.0, 8.0), 3.5, white, 1.0);
            canvas.round_rect(r(5.0, 13.0, 12.0, 6.0), 3.0, white, 1.0);
        }
        Section::Login => {
            canvas.round_rect(r(12.0, 4.0, 6.0, 14.0), 1.5, white, 1.0);
            canvas.round_rect(r(3.0, 10.0, 7.0, 2.0), 1.0, white, 1.0);
            canvas.triangle(
                [
                    o + Vec2::new(9.0, 7.0),
                    o + Vec2::new(13.0, 11.0),
                    o + Vec2::new(9.0, 15.0),
                ],
                white,
                1.0,
            );
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_scale_lands_on_its_own_monitor_and_keeps_the_rest() {
        let text = "[[monitor]]\nname = \"eDP-1\"\nscale = 1.0\nrotate = 0\n\n[[monitor]]\nname = \"HDMI-A-1\"\nscale = 1.0\n";
        let out = write_scale(text, "eDP-1", 2.0).unwrap();
        let config = crate::Config::parse(&out).unwrap();
        assert_eq!(config.monitor[0].scale, 2.0);
        assert_eq!(config.monitor[1].scale, 1.0);
        let fresh = write_scale("", "DP-2", 1.25).unwrap();
        let config = crate::Config::parse(&fresh).unwrap();
        assert_eq!((config.monitor[0].name.as_str(), config.monitor[0].scale), ("DP-2", 1.25));
    }

    #[test]
    fn an_app_default_is_written_under_apps() {
        let out = write_app("[apps]\n# keep\n", "editor", &Launch::App("org.xfce.mousepad".into())).unwrap();
        assert!(out.contains("# keep"), "{out}");
        let config = crate::Config::parse(&out).unwrap();
        assert_eq!(config.apps.editor, Some(Launch::App("org.xfce.mousepad".into())));
    }
    use super::*;

    const MOUNTINFO: &str = "29 1 254:1 / / rw - ext4 /dev/vda1 rw\n\
        30 29 0:25 / /dev rw - devtmpfs udev rw\n\
        31 29 8:3 / /home rw - ext4 /dev/sda3 rw\n\
        32 29 7:0 / /snap/core rw - squashfs /dev/loop0 ro\n\
        33 29 8:3 /bind /srv rw - ext4 /dev/sda3 rw\n";

    fn kernel(mount: &Mount) -> Option<String> {
        match mount.device.as_str() {
            "254:1" => Some("vda1".into()),
            "8:3" => Some("sda3".into()),
            _ => None,
        }
    }

    #[test]
    fn every_page_at_density_two_matches_density_one() {
        let body = Font::find(None, 13.0).unwrap();
        let small = Font::find(None, 11.5).unwrap();
        let title = Font::find(None, 22.0).unwrap();
        let fonts = Fonts {
            body: &body,
            small: &small,
            title: &title,
        };
        let (icons, thumbs) = (HashMap::new(), HashMap::new());
        let art = Art {
            wallpaper: None,
            icons: &icons,
            thumbs: &thumbs,
            sheet: None,
        };
        let look = Look::default();
        let model = model(&Config::default());
        for section in Section::ALL {
            crate::paint::same_at_two(section.name(), || {
                look.paint(Vec2::new(860.0, 620.0), section, &model, 0.0, &fonts, &art)
            });
        }
    }

    pub(super) fn model(config: &Config) -> Model {
        Model::read(config, &stats::mounts(MOUNTINFO), kernel)
    }

    fn config(text: &str) -> Config {
        Config::parse(text).unwrap()
    }

    #[test]
    fn search_finds_pages_by_name_rows_and_plain_words() {
        let mut model = model(&Config::default());
        assert_eq!(model.shown().len(), Section::ALL.len());
        model.query = "volume".into();
        assert!(model.shown().contains(&Section::Sound));
        model.query = "BRIGHTNESS".into();
        assert_eq!(model.shown(), [Section::Displays]);
        model.query = "tap to click".into();
        assert_eq!(model.shown(), [Section::Touchpad]);
        model.query = "zzzz".into();
        assert!(model.shown().is_empty());
    }

    #[test]
    fn the_screen_off_time_is_found_where_people_look() {
        let mut model = model(&Config::default());
        for query in ["monitor off", "display sleep", "screen off", "turn the screen off"] {
            model.query = query.into();
            let shown = model.shown();
            assert!(shown.contains(&Section::Power) && shown.contains(&Section::Lock), "{query}: {shown:?}");
        }
        for section in [Section::Power, Section::Lock] {
            assert_eq!(model.nudge(section, Id::ScreenOff, 1.0), Some(Value::Number(model.screen_off as f64 + 1.0)));
        }
    }

    #[test]
    fn the_sidebar_lists_only_the_matches_in_order() {
        let mut model = model(&Config::default());
        model.query = "wallpaper".into();
        let look = Look::default();
        let size = look.least();
        let first = look.item_rect_in(&model.shown(), 0).center();
        assert_eq!(
            look.hit(size, Section::Desktop, &model, 0.0, first),
            Some(Hit::Section(model.shown()[0]))
        );
        let search = look.search_rect().center();
        assert_eq!(
            look.hit(size, Section::Desktop, &model, 0.0, search),
            Some(Hit::Search)
        );
        let shown = model.shown();
        let below = look.item_rect_in(&shown, shown.len() - 1);
        let past = below.center() + Vec2::new(0.0, look.item + 2.0);
        assert_eq!(look.hit(size, Section::Desktop, &model, 0.0, past), None);
    }

    #[test]
    fn reset_asks_twice_and_any_other_press_cancels() {
        let mut model = model(&Config::default());
        assert_eq!(model.press(Id::ResetAll, 0), None);
        assert!(model.confirm_reset);
        model.press(Id::Custom, 0);
        assert!(!model.confirm_reset);
        model.press(Id::ResetAll, 0);
        assert_eq!(model.press(Id::ResetAll, 0), Some(Ask::Reset));
        assert!(!model.confirm_reset);
        assert_eq!(model.press(Id::Export, 0), Some(Ask::Files(For::Export)));
        assert_eq!(model.press(Id::Import, 0), Some(Ask::Files(For::Import)));
    }

    #[test]
    fn export_never_writes_over_a_file_and_import_checks_first() {
        let folder = std::path::Path::new("/x");
        assert_eq!(export_path(folder, |_| false), folder.join("drifter.toml"));
        let taken = |path: &std::path::Path| !path.ends_with("drifter-3.toml");
        assert_eq!(export_path(folder, taken), folder.join("drifter-3.toml"));
        assert!(check_import(include_str!("../../../conf/default.toml")).is_ok());
        assert!(check_import("[workspace]\nwidth = \"wide\"\n").is_err());
        assert!(check_import("[login]\nstart = [{ icon = \"x\" }]\n").is_err());
    }

    #[test]
    fn replacing_the_config_keeps_a_backup() {
        let dir = std::env::temp_dir().join(format!("drifter-replace-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("drifter.toml");
        std::fs::write(&path, "old").unwrap();
        let backup = replace_config(&path, "new").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "old");
        assert_eq!(backup, dir.join("drifter.toml.bak"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_page_is_found_by_its_name_in_any_case() {
        assert_eq!(Section::from_word("sound"), Some(Section::Sound));
        assert_eq!(Section::from_word("Date & Time"), Some(Section::Time));
        assert_eq!(Section::from_word("lockscreen"), Some(Section::Lock));
        assert_eq!(Section::from_word("nope"), None);
    }

    #[test]
    fn only_real_disks_are_offered_and_each_only_once() {
        let model = model(&Config::default());
        let labels: Vec<&str> = model.disks.iter().map(|d| d.label.as_str()).collect();
        assert_eq!(labels, ["VDA1", "SDA3"]);
    }

    #[test]
    fn the_default_config_shows_cpu_ram_and_the_root_disk() {
        let model = model(&Config::default());
        assert!(model.stats && model.cpu && model.ram && model.battery);
        assert!(model.disks[0].on);
        assert!(!model.disks[1].on);
        assert_eq!(model.metrics(), ["cpu", "ram", "/"]);
    }

    #[test]
    fn a_disk_named_by_device_is_found_and_written_by_mount_point() {
        let model = model(&config("[tray]\nmetrics = [\"sda3\"]\n"));
        assert!(model.disks[1].on);
        assert_eq!(model.metrics(), ["/home"]);
    }

    #[test]
    fn a_metric_for_a_disk_that_is_not_here_is_kept() {
        let model = model(&config("[tray]\nmetrics = [\"cpu\", \"sdz9\"]\n"));
        assert_eq!(model.kept, ["sdz9"]);
        assert_eq!(model.metrics(), ["cpu", "sdz9"]);
    }

    #[test]
    fn the_default_clock_format_round_trips() {
        let model = model(&Config::default());
        assert!(model.day && model.date && model.seconds && !model.hours24);
        assert_eq!(model.format(), crate::tray::DEFAULT_FORMAT);
    }

    #[test]
    fn the_clock_switches_build_the_format() {
        let mut model = model(&Config::default());
        model.set(Id::Day, Value::Switch(false));
        model.set(Id::Seconds, Value::Switch(false));
        model.set(Id::Hours24, Value::Switch(true));
        assert_eq!(model.format(), "%b %d %H:%M");
        model.set(Id::Date, Value::Switch(false));
        assert_eq!(model.format(), "%H:%M");
    }

    #[test]
    fn a_hand_written_clock_format_is_kept_until_the_user_lets_go_of_it() {
        let text = "[tray]\nformat = \"%Y-%m-%d %R\"\n";
        let before = model(&config(text));
        assert_eq!(before.custom.as_deref(), Some("%Y-%m-%d %R"));
        assert!(before.hours24);
        let rows = before.clock_rows();
        assert!(rows[1..].iter().all(|row| !row.enabled));
        let mut after = before.clone();
        after.set(Id::Seconds, Value::Switch(true));
        assert!(!after.write(&before, text).unwrap().contains("%b"));
        after.press(Id::Custom, 0);
        let out = after.write(&before, text).unwrap();
        let back = Config::parse(&out).unwrap();
        assert_eq!(back.tray.format, after.format());
        assert!(model(&Config::default()).custom.is_none());
    }

    #[test]
    fn the_world_is_one_number_for_both_sides() {
        let model = model(&config("[workspace]\nwidth = 4\nheight = 6\n"));
        assert_eq!(model.world, 6);
        let small = self::model(&config("[workspace]\nwidth = 1\nheight = 1\n"));
        assert_eq!(small.world, WORLD_LEAST);
    }

    #[test]
    fn writing_changes_only_what_changed_and_keeps_comments() {
        let text = "# mine\n[workspace]\n# across\nwidth = 3\nheight = 3\n\n[tray]\nsize = 13\n";
        let before = model(&config(text));
        let mut after = before.clone();
        after.set(Id::World, Value::Number(5.0));
        let out = after.write(&before, text).unwrap();
        assert!(out.contains("# mine") && out.contains("# across"), "{out}");
        assert!(
            out.contains("width = 5") && out.contains("height = 5"),
            "{out}"
        );
        for untouched in [
            "format", "metrics", "[window]", "[lock]", "[login]", "pinned",
        ] {
            assert!(!out.contains(untouched), "{untouched} was written: {out}");
        }
        let back = Config::parse(&out).unwrap();
        assert_eq!(back.workspace.width, 5);
    }

    #[test]
    fn writing_the_stats_gives_a_list_the_config_reads() {
        let text = "[tray]\n";
        let before = model(&config(text));
        let mut after = before.clone();
        after.set(Id::Ram, Value::Switch(false));
        after.set(Id::Disk(1), Value::Switch(true));
        after.set(Id::Stats, Value::Switch(false));
        let out = after.write(&before, text).unwrap();
        let back = Config::parse(&out).unwrap();
        assert!(!back.tray.stats);
        assert_eq!(back.tray.metrics, ["cpu", "/", "/home"]);
        back.validate().unwrap();
    }

    #[test]
    fn writing_the_camera_key_and_wallpaper_mode_uses_the_config_words() {
        let text = "";
        let before = model(&config(text));
        let mut after = before.clone();
        after.set(Id::Modifier, Value::Choice(0));
        after.set(Id::WallpaperMode, Value::Choice(2));
        let back = Config::parse(&after.write(&before, text).unwrap()).unwrap();
        assert_eq!(back.input.modifier, Modifier::Super);
        assert_eq!(back.workspace.wallpaper_mode, WallpaperMode::Parallax);
    }

    #[test]
    fn arrow_steps_move_a_slider_finely_and_stop_at_its_ends() {
        let model = model(&config(""));
        assert_eq!(model.nudge(Section::Wallpaper, Id::Drift, 1.0), Some(Value::Number(101.0)));
        assert_eq!(model.nudge(Section::Wallpaper, Id::Drift, -10.0), Some(Value::Number(90.0)));
        let mut low = model.clone();
        low.drift = 0;
        assert_eq!(low.nudge(Section::Wallpaper, Id::Drift, -1.0), Some(Value::Number(0.0)));
        assert_eq!(model.nudge(Section::Wallpaper, Id::WallpaperMode, 1.0), None);
        assert_eq!(model.nudge(Section::Desktop, Id::Drift, 1.0), None);
    }

    #[test]
    fn the_motion_slider_writes_a_percent_and_stops_at_the_most() {
        let text = "";
        let before = model(&config(text));
        assert_eq!(before.drift, 100);
        let mut after = before.clone();
        after.set(Id::Drift, Value::Number(0.0));
        let back = Config::parse(&after.write(&before, text).unwrap()).unwrap();
        assert_eq!(back.workspace.wallpaper_motion, 0);
        after.set(Id::Drift, Value::Number(9999.0));
        let back = Config::parse(&after.write(&before, text).unwrap()).unwrap();
        assert_eq!(back.workspace.wallpaper_motion, crate::config::MOST_MOTION);
        assert!(Config::parse("[workspace]\nwallpaper_motion = 301\n").unwrap().validate().is_err());
    }

    #[test]
    fn the_ripple_switch_and_strength_write_the_workspace_table() {
        let text = "";
        let before = model(&config(text));
        assert!(before.ripple && before.ripple_strength == 100);
        let mut after = before.clone();
        after.set(Id::Ripple, Value::Switch(false));
        after.set(Id::RippleStrength, Value::Number(5.0));
        after.set(Id::WallpaperPointer, Value::Switch(true));
        let back = Config::parse(&after.write(&before, text).unwrap()).unwrap();
        assert_eq!(back.workspace.wallpaper_pointer, Some(true));
        assert!(!back.workspace.ripple);
        assert_eq!(back.workspace.ripple_strength, 25, "the strength clamps");
        back.validate().unwrap();
    }

    #[test]
    fn night_changes_write_the_night_table() {
        let text = "";
        let before = model(&config(text));
        let mut after = before.clone();
        after.set(Id::NightOn, Value::Switch(true));
        after.set(Id::Warmth, Value::Number(3449.0));
        after.set(Id::NightStart, Value::Number(22.4));
        let out = after.write(&before, text).unwrap();
        let back = Config::parse(&out).unwrap();
        assert!(back.night.on);
        assert_eq!(
            (back.night.warmth, back.night.start, back.night.end),
            (3400, 22, 7)
        );
        assert!(!out.contains("end"));
        back.validate().unwrap();
    }

    #[test]
    fn keyboard_changes_write_the_keyboard_table() {
        let text = "[keyboard]\nlayout = \"us\"\nvariant = \"dvorak\"\n";
        let before = model(&config(text));
        assert_eq!(
            before.clone().press(Id::Layout, 0),
            Some(Ask::Choose(For::Layout))
        );
        let mut after = before.clone();
        after.choose_layout("de".into());
        after.set(Id::RepeatDelay, Value::Number(5000.0));
        after.set(Id::RepeatRate, Value::Number(33.4));
        let out = after.write(&before, text).unwrap();
        let back = Config::parse(&out).unwrap();
        assert_eq!(back.keyboard.layout, "de");
        assert_eq!(back.keyboard.variant, "");
        assert_eq!(back.keyboard.repeat_delay, REPEAT_DELAY.1);
        assert_eq!(back.keyboard.repeat_rate, 33);
        back.validate().unwrap();
    }

    #[test]
    fn screen_off_writes_the_lock_table() {
        let text = "[lock]\nafter = 5\n";
        let before = model(&config(text));
        assert_eq!(before.screen_off, 10);
        let mut after = before.clone();
        after.set(Id::ScreenOff, Value::Number(0.2));
        let out = after.write(&before, text).unwrap();
        let back = Config::parse(&out).unwrap();
        assert_eq!(back.lock.screen_off, 0);
        assert_eq!(back.lock.after, 5);
    }

    #[test]
    fn touchpad_changes_write_the_touchpad_table() {
        let text = "[input]\nmodifier = \"ctrl\"\n";
        let before = model(&config(text));
        let mut after = before.clone();
        after.set(Id::ThreeTap, Value::Choice(1));
        after.set(Id::Natural, Value::Switch(true));
        after.set(Id::Speed, Value::Number(7.4));
        after.set(Id::Tap, Value::Switch(false));
        let out = after.write(&before, text).unwrap();
        let back = Config::parse(&out).unwrap();
        assert_eq!(back.touchpad.three_tap, ThreeTap::Middle);
        assert!(back.touchpad.natural);
        assert!(!back.touchpad.tap);
        assert_eq!(back.touchpad.speed, 7);
        assert!(back.touchpad.swipe && back.touchpad.accel);
        assert!(!out.contains("swipe"));
        back.validate().unwrap();
    }

    #[test]
    fn the_displays_page_writes_scale_rotation_and_place() {
        let text = "[input]\nmodifier = \"ctrl\"\n\n[[monitor]]\nname = \"DP-9\"\nscale = 2.0\n";
        let mut before = model(&config(text));
        let found = [
            ("eDP-1".to_string(), Vec2::new(2880.0, 1800.0)),
            ("HDMI-A-1".to_string(), Vec2::new(1920.0, 1080.0)),
        ];
        before.see_monitors(&found);
        assert_eq!(before.write(&before, text).unwrap(), text);
        let mut after = before.clone();
        after.press(Id::Scale, 1);
        after.press(Id::Scale, 1);
        assert_eq!(after.monitors[0].scale, 1.5);
        after.pick_monitor(1);
        after.set(Id::Turn, Value::Choice(1));
        after.move_monitor(1, Vec2::new(-1100.0, 30.0));
        let out = after.write(&before, text).unwrap();
        let back = Config::parse(&out).unwrap();
        back.validate().unwrap();
        assert_eq!(back.monitor.len(), 3);
        let laptop = &back.monitor[0];
        assert_eq!((laptop.name.as_str(), laptop.scale), ("eDP-1", 1.5));
        let side = &back.monitor[1];
        assert_eq!((side.name.as_str(), side.rotate), ("HDMI-A-1", 90));
        assert_eq!((side.x, side.y), (Some(0), Some(30)));
        assert_eq!((laptop.x, laptop.y), (Some(1080), Some(0)));
        assert_eq!(back.monitor[2].name, "DP-9");
        assert_eq!(back.monitor[2].scale, 2.0);
        assert!(out.contains("modifier = \"ctrl\""));
    }

    #[test]
    fn the_displays_page_writes_the_mode_and_turns_a_monitor_off() {
        use crate::mode::Mode;
        let text = "[[monitor]]\nname = \"eDP-1\"\nmode = \"1920x1080@60\"\n";
        let mut before = model(&config(text));
        let found = [
            ("eDP-1".to_string(), Vec2::new(1920.0, 1080.0)),
            ("HDMI-A-1".to_string(), Vec2::new(1920.0, 1080.0)),
        ];
        before.see_monitors(&found);
        let modes = [
            Mode::new(1920, 1080, 60_000, true),
            Mode::new(1920, 1080, 144_000, false),
            Mode::new(1280, 720, 60_000, false),
        ];
        before.see_modes("eDP-1", &modes, Some(modes[0]));
        before.see_modes("HDMI-A-1", &modes, Some(modes[0]));
        assert_eq!(before.write(&before, text).unwrap(), text);

        let mut after = before.clone();
        after.press(Id::Refresh, 1);
        assert_eq!(after.monitors[0].mode.as_deref(), Some("1920x1080@144"));
        after.press(Id::Refresh, 1);
        assert_eq!(after.monitors[0].mode.as_deref(), Some("1920x1080@144"), "the top rate stops");
        after.press(Id::Resolution, 0);
        assert_eq!(after.monitors[0].mode.as_deref(), Some("1280x720@60"));
        assert_eq!(after.monitors[0].pixels, Vec2::new(1280.0, 720.0));
        after.press(Id::Resolution, 1);
        assert_eq!(after.monitors[0].mode.as_deref(), Some("1920x1080@60"));
        after.press(Id::Refresh, 1);
        after.pick_monitor(1);
        after.set(Id::MonitorOn, Value::Switch(false));
        after.pick_monitor(0);
        after.set(Id::MonitorOn, Value::Switch(false));
        assert!(after.monitors[0].enabled, "the last monitor in use stays on");

        let out = after.write(&before, text).unwrap();
        let back = Config::parse(&out).unwrap();
        back.validate().unwrap();
        let laptop = back.monitor.iter().find(|m| m.name == "eDP-1").unwrap();
        assert_eq!(laptop.mode.as_deref(), Some("1920x1080@144"));
        assert!(laptop.enabled);
        let side = back.monitor.iter().find(|m| m.name == "HDMI-A-1").unwrap();
        assert!(!side.enabled);
        assert!(!out.contains("enabled = true"));
    }

    #[test]
    fn the_access_and_screenshot_rows_write_their_keys() {
        let text = "[notify]\nquiet = true\n";
        let before = model(&config(text));
        let mut after = before.clone();
        after.set(Id::ReduceMotion, Value::Switch(true));
        after.set(Id::StickyKeys, Value::Switch(true));
        after.set(Id::ShotFormat, Value::Choice(1));
        after.shots.folder = Some("~/Desktop".to_string());
        let out = after.write(&before, text).unwrap();
        let back = Config::parse(&out).unwrap();
        back.validate().unwrap();
        assert!(back.access.reduce_motion && back.access.sticky_keys);
        assert!(!back.access.large_text);
        assert_eq!(back.screenshots.format, crate::config::ShotFormat::Jpeg);
        assert_eq!(back.screenshots.folder.as_deref(), Some("~/Desktop"));
        assert!(back.notify.quiet);

        let mut reset = model(&back);
        assert_eq!(reset.press(Id::ShotFolder, 0), Some(Ask::Files(For::ShotFolder)));
        reset.press(Id::ShotFolder, 1);
        let out = reset.write(&model(&back), &out).unwrap();
        assert_eq!(Config::parse(&out).unwrap().screenshots.folder, None);
    }

    #[test]
    fn the_power_mode_shows_only_what_the_daemon_offers() {
        let mut model = model(&Config::default());
        let rows = |model: &Model| -> Vec<Id> {
            model
                .groups(Section::Power)
                .iter()
                .flat_map(|group| group.rows.iter().map(|row| row.id))
                .collect()
        };
        assert!(!rows(&model).contains(&Id::Profile), "no daemon, no row");
        model.profile = Some(Profiles {
            now: "balanced".into(),
            offered: vec!["power-saver".into(), "balanced".into()],
        });
        assert!(rows(&model).contains(&Id::Profile));
        let group = model.groups(Section::Power);
        let row = group
            .iter()
            .flat_map(|group| group.rows.iter())
            .find(|row| row.id == Id::Profile)
            .unwrap();
        assert_eq!(
            row.control,
            Control::Segments { options: vec!["Saver", "Balanced"], chosen: 1 }
        );
        model.set(Id::Profile, Value::Choice(0));
        assert_eq!(model.profile.as_ref().unwrap().now, "power-saver");
        model.cells = vec![("Charge".into(), "80%, charging".into())];
        assert!(rows(&model).contains(&Id::Battery));
    }

    #[test]
    fn the_network_rows_ask_for_the_right_action() {
        use crate::net::{Net, Wifi};
        let mut model = model(&Config::default());
        assert_eq!(model.press(Id::Net(0), 0), None, "no wifi, nothing to press");
        let net = |ssid: &str, active: bool, known: bool| Net {
            ssid: ssid.into(),
            signal: 80,
            secure: true,
            enterprise: false,
            active,
            known,
        };
        model.wifi = Some(Wifi {
            managed: true,
            on: true,
            nets: vec![net("Home", true, true), net("Work", false, true), net("Cafe", false, false)],
        });
        assert_eq!(model.press(Id::Net(0), 0), Some(Ask::Net(NetAsk::Leave(0))));
        assert_eq!(model.press(Id::Net(1), 0), Some(Ask::Net(NetAsk::Join(1))));
        assert_eq!(model.press(Id::Net(1), 1), Some(Ask::Net(NetAsk::Forget(1))));
        assert_eq!(model.press(Id::Net(2), 0), Some(Ask::Net(NetAsk::Join(2))));
        let rows: Vec<Row> = model.groups(Section::Network).into_iter().flat_map(|g| g.rows).collect();
        assert_eq!(rows.len(), 5);
        assert_eq!(rows[4].id, Id::WifiHidden);
        assert_eq!(model.press(Id::WifiHidden, 0), Some(Ask::Net(NetAsk::Hidden)));
        let look = Look::default();
        let groups = model.groups(Section::Network);
        let row = look.place(SIZE, Section::Network, &groups, 0.0)[1].rows[1];
        let name = Vec2::new(row.origin.x + 30.0, row.center().y);
        assert_eq!(look.hit(SIZE, Section::Network, &model, 0.0, name), Some(Hit::Open(Id::Net(1))));
        let join = look.mark_rects(row, &[Mark::Text("Join"), Mark::Text("Forget")])[0].center();
        assert_eq!(look.hit(SIZE, Section::Network, &model, 0.0, join), Some(Hit::Press(Id::Net(1), 0)));
        let switch = look.place(SIZE, Section::Network, &groups, 0.0)[0].rows[0];
        assert_ne!(look.hit(SIZE, Section::Network, &model, 0.0, switch.center()), Some(Hit::Open(Id::WifiOn)));
        assert_eq!(rows[1].detail.as_deref(), Some("Connected"));
        assert_eq!(rows[3].detail.as_deref(), Some("Secured, strong signal"));
        model.set(Id::WifiOn, Value::Switch(false));
        assert_eq!(model.groups(Section::Network).into_iter().flat_map(|g| g.rows).count(), 1);
        model.wifi.as_mut().unwrap().managed = false;
        let groups = model.groups(Section::Network);
        assert_eq!(groups.iter().flat_map(|g| &g.rows).count(), 0, "no switch for a card we do not run");
        assert!(groups[0].footer.as_deref().is_some_and(|words| words.contains("Another tool")));

    }

    #[test]
    fn the_bluetooth_page_speaks_plainly_and_opens_options() {
        use crate::blue::{Blue, Device, Kind, Sound, When};
        let mut model = model(&Config::default());
        let device = |name: &str, address: &str, kind: Kind, paired: bool, connected: bool| Device {
            path: String::new(),
            address: address.into(),
            name: name.into(),
            named: true,
            kind,
            paired,
            connected,
            trusted: paired,
            battery: connected.then_some(70),
            rssi: (!paired).then_some(-85),
            audio: kind.audio(),
            sends: false,
            classic: true,
        };
        model.blue = Some(Blue {
            on: true,
            searching: true,
            devices: vec![
                device("Buds", "AA:00:00:00:00:01", Kind::Earbuds, true, true),
                device("Mouse", "AA:00:00:00:00:02", Kind::Mouse, true, false),
                device("Pad", "AA:00:00:00:00:03", Kind::Trackpad, false, false),
            ],
            ..Blue::default()
        });
        assert_eq!(model.press(Id::Gadget(0), 0), Some(Ask::Net(NetAsk::Disconnect(0))));
        assert_eq!(model.press(Id::Gadget(1), 0), Some(Ask::Net(NetAsk::Connect(1))));
        assert_eq!(model.press(Id::Gadget(1), 1), Some(Ask::Net(NetAsk::Options(1))));
        assert_eq!(model.press(Id::Gadget(2), 0), Some(Ask::Net(NetAsk::Pair(2))));
        let groups = model.groups(Section::Bluetooth);
        assert_eq!(groups[1].header, Some("My devices"));
        assert_eq!(groups[1].rows[0].detail.as_deref(), Some("Earbuds, Connected, 70%"));
        assert_eq!(groups[2].header, Some("Nearby"));
        assert_eq!(groups[2].rows[0].detail.as_deref(), Some("Trackpad, move closer"));
        let words: String = groups.iter().flat_map(|group| &group.rows).map(|row| format!("{} {:?}", row.label, row.detail)).collect();
        for jargon in ["Trusted", "Paired", "Bonded", "AA:00"] {
            assert!(!words.contains(jargon), "{jargon} shows: {words}");
        }

        model.blue_open = Some("AA:00:00:00:00:01".into());
        let groups = model.groups(Section::Bluetooth);
        let options = groups.iter().find(|group| group.header == Some("Device options")).expect("the options show");
        let ids: Vec<Id> = options.rows.iter().map(|row| row.id).collect();
        assert!(ids.contains(&Id::BlueSound) && ids.contains(&Id::BlueMic) && ids.contains(&Id::BlueKey));
        let address = options.rows.iter().find(|row| row.id == Id::BlueAddress).unwrap();
        assert_eq!(address.detail.as_deref(), Some("Hidden"));
        assert_eq!(model.press(Id::BlueAddress, 0), None);
        let groups = model.groups(Section::Bluetooth);
        let shown = groups.iter().flat_map(|group| &group.rows).find(|row| row.id == Id::BlueAddress).unwrap();
        assert_eq!(shown.detail.as_deref(), Some("AA:00:00:00:00:01"), "the address shows once asked");
        model.set(Id::BlueWhen, Value::Choice(2));
        model.set(Id::BlueSound, Value::Choice(1));
        assert_eq!(model.blue_prefs.when("AA:00:00:00:00:01"), When::Sound);
        assert_eq!(model.blue_prefs.sound("AA:00:00:00:00:01"), Sound::Best);
        assert_eq!(model.press(Id::BlueForget, 0), Some(Ask::Net(NetAsk::Remove(0))));
        assert_eq!(model.press(Id::BlueName, 0), Some(Ask::Net(NetAsk::Rename)));
        model.blue_naming = Some("Pods".into());
        let name = model.groups(Section::Bluetooth).into_iter().flat_map(|group| group.rows).find(|row| row.id == Id::BlueName).unwrap();
        assert!(name.editing, "the name turns into a box in place");
        assert_eq!(name.label, "Pods");
        assert_eq!(model.press(Id::BlueName, 1), Some(Ask::Net(NetAsk::SaveName)));
        assert_eq!(model.press(Id::BlueName, 0), Some(Ask::Net(NetAsk::StopNaming)));

        model.blue_steps = vec![("Turn Bluetooth off and on".into(), crate::blue::Step::Done), ("Forget Buds".into(), crate::blue::Step::Doing)];
        let groups = model.groups(Section::Bluetooth);
        let fix = groups.iter().find(|group| group.header == Some("Fix It")).expect("the steps show");
        let icons: Vec<Option<String>> = fix.rows.iter().map(|row| row.icon.clone()).collect();
        assert_eq!(icons, [Some("step:done".to_string()), Some("step:wait".to_string())], "a step in progress turns on top of an empty ring");
    }

    #[test]
    fn a_place_can_go_in_the_dock_and_be_removed() {
        let text = "[[place]]\nname = \"Place 1\"\nx = 10.0\ny = 20.0\nzoom = 0.5\nkey = \"camera+1\"\n\n[[place]]\nname = \"Place 2\"\nx = 30.0\ny = 40.0\nzoom = 1.0\n";
        let before = model(&config(text));
        assert_eq!(before.places.len(), 2);
        let row = before
            .groups(Section::Desktop)
            .into_iter()
            .flat_map(|group| group.rows)
            .find(|row| row.id == Id::Place(0))
            .unwrap();
        assert_eq!(row.detail.as_deref(), Some("Super + 1, zoom 50%"));

        let mut after = before.clone();
        assert_eq!(after.press(Id::Place(0), 0), Some(Ask::RenamePlace(0)));
        after.press(Id::Place(0), 1);
        let out = after.write(&before, text).unwrap();
        let back = Config::parse(&out).unwrap();
        back.validate().unwrap();
        assert_eq!(back.dock.pinned.len(), 1);
        assert_eq!(back.dock.pinned[0].command(), Some("drifter:place Place 1"));

        let mut later = model(&back);
        later.rename_place(0, "Studio").unwrap();
        assert!(later.rename_place(1, "Studio").is_err());
        assert_eq!(later.dock.len(), 1);
        later.press(Id::Place(0), 2);
        let out = later.write(&model(&back), &out).unwrap();
        let back = Config::parse(&out).unwrap();
        assert_eq!(back.place.len(), 1);
        assert_eq!(back.place[0].name, "Place 2");
        assert!(back.dock.pinned.is_empty(), "removing a place takes its dock button too");
    }

    #[test]
    fn a_broken_config_is_not_overwritten() {
        let before = model(&Config::default());
        assert!(before.write(&before, "this = = broken").is_err());
    }

    #[test]
    fn the_button_order_moves_one_step_and_stops_at_the_ends() {
        let mut model = model(&Config::default());
        model.press(Id::Order(0), 0);
        assert_eq!(model.order[0], ButtonName::Close);
        model.press(Id::Order(0), 1);
        assert_eq!(
            model.order,
            [
                ButtonName::Minimize,
                ButtonName::Close,
                ButtonName::Maximize
            ]
        );
        model.press(Id::Order(2), 1);
        assert_eq!(model.order[2], ButtonName::Maximize);
    }

    #[test]
    fn the_window_page_writes_side_order_and_title() {
        let before = model(&Config::default());
        let mut after = before.clone();
        after.set(Id::Side, Value::Choice(1));
        after.set(Id::Title, Value::Switch(false));
        after.press(Id::Order(2), 0);
        let back = Config::parse(&after.write(&before, "").unwrap()).unwrap();
        assert_eq!(back.window.buttons, Side::Right);
        assert!(!back.window.title);
        assert_eq!(
            back.window.order,
            [
                ButtonName::Close,
                ButtonName::Maximize,
                ButtonName::Minimize
            ]
        );
        back.validate().unwrap();
    }

    #[test]
    fn brightness_shows_only_with_a_backlight_and_stays_off_the_config() {
        let mut model = model(&Config::default());
        let has = |model: &Model| {
            model
                .groups(Section::Displays)
                .iter()
                .any(|group| group.rows.iter().any(|row| row.id == Id::Brightness))
        };
        assert!(!has(&model));
        model.set(Id::Brightness, Value::Number(50.0));
        assert_eq!(model.brightness, None);

        model.brightness = Some(0.4);
        assert!(has(&model));
        let before = model.clone();
        model.set(Id::Brightness, Value::Number(0.0));
        assert_eq!(model.brightness, Some(0.05));
        model.set(Id::Brightness, Value::Number(72.4));
        assert_eq!(model.brightness, Some(0.72));
        assert_eq!(model.write(&before, "").unwrap(), "");
    }

    #[test]
    fn the_dock_icon_padding_and_fit_write_the_dock_table() {
        let text = "[dock]\n";
        let before = model(&config(text));
        assert_eq!((before.dock_icon, before.dock_pad, before.dock_fit), (52.0, 6.0, false));
        let mut after = before.clone();
        after.set(Id::DockIcon, Value::Number(40.0));
        after.set(Id::DockPadding, Value::Number(12.0));
        after.set(Id::DockFit, Value::Switch(true));
        after.set(Id::DockHover, Value::Choice(2));
        after.set(Id::DockMagnify, Value::Number(400.0));
        let back = Config::parse(&after.write(&before, text).unwrap()).unwrap();
        assert_eq!(back.dock_height(), 64.0);
        assert!(back.dock.fit);
        assert_eq!(back.dock.hover, crate::config::DockHover::Magnify);
        assert_eq!(back.dock.magnify, 200, "the amount clamps");
        back.validate().unwrap();
    }

    #[test]
    fn autostart_switches_flip_entries_and_write_the_login_table() {
        let mut before = model(&Config::default());
        before.autostarts = vec![crate::autostart::Entry {
            id: "nm-applet".into(),
            name: "Network".into(),
            exec: "nm-applet".into(),
            icon: Some("nm-device-wireless".into()),
            on: true,
        }];
        let rows = &before.groups(Section::Login)[1].rows;
        assert_eq!(rows[0].control, Control::Switch(true));
        assert_eq!(rows[1].icon.as_deref(), Some("icon:nm-device-wireless"));

        let mut after = before.clone();
        after.set(Id::AutoEntry(0), Value::Switch(false));
        assert!(!after.autostarts[0].on);
        assert_eq!(after.write(&before, "").unwrap(), "");

        after.set(Id::Autostart, Value::Switch(false));
        assert!(!after.groups(Section::Login)[1].rows[1].enabled);
        let back = Config::parse(&after.write(&before, "").unwrap()).unwrap();
        assert!(!back.login.autostart);
    }

    #[test]
    fn login_and_dock_items_round_trip_through_the_config() {
        let before = model(&Config::default());
        let mut after = before.clone();
        after.add(For::Login, Launch::App("firefox".into()));
        after.add(
            For::Login,
            Launch::command_line("foot -e htop"),
        );
        after.add(For::Dock, Launch::App("foot".into()));
        let edit = |icon: Option<&str>| DockEdit {
            name: String::new(),
            target: "foot".into(),
            app: true,
            icon: icon.map(PathBuf::from),
            shape: None,
            background: true,
        };
        after.edit_dock(0, edit(Some("/icons/foot.png"))).unwrap();
        let back = Config::parse(&after.write(&before, "").unwrap()).unwrap();
        assert_eq!(back.login.start, after.login);
        assert_eq!(
            back.dock.pinned[0].icon(),
            Some(std::path::Path::new("/icons/foot.png"))
        );
        back.validate().unwrap();
        let mut plain = after.clone();
        plain.edit_dock(0, edit(None)).unwrap();
        assert_eq!(plain.dock[0], Launch::App("foot".into()));
    }

    #[test]
    fn the_dock_ask_names_one_item() {
        assert_eq!(read_dock_ask("dock-item 3\n"), Some(3));
        assert_eq!(read_dock_ask("dock-item x"), None);
        assert_eq!(read_dock_ask("other 3"), None);
    }

    #[test]
    fn a_dock_item_keeps_its_own_name_target_shape_and_background() {
        let before = model(&Config::default());
        let mut after = before.clone();
        after.names.insert("foot".into(), "Foot".into());
        after.add(For::Dock, Launch::App("foot".into()));
        let edit = DockEdit {
            name: "Shell".into(),
            target: "foot -e htop".into(),
            app: false,
            icon: None,
            shape: Some(crate::config::IconShape::Squircle),
            background: false,
        };
        after.edit_dock(0, edit.clone()).unwrap();
        let back = Config::parse(&after.write(&before, "").unwrap()).unwrap();
        back.validate().unwrap();
        let item = &back.dock.pinned[0];
        assert_eq!(item.command(), Some("foot -e htop"));
        assert_eq!(item.title(), Some("Shell"));
        assert_eq!(after.name_of(item), "Shell");
        assert_eq!(item.shape(), Some(crate::config::IconShape::Squircle));
        assert!(!item.background());

        let same = DockEdit {
            name: "Foot".into(),
            target: "foot".into(),
            app: true,
            shape: None,
            background: false,
            ..edit.clone()
        };
        after.edit_dock(0, same).unwrap();
        assert_eq!(after.dock[0], Launch::App("foot".into()), "the app's own name is not stored");
        let blank = DockEdit {
            target: "  ".into(),
            ..edit
        };
        assert!(after.edit_dock(0, blank).is_err());
    }

    #[test]
    fn single_keys_are_written_and_comments_survive() {
        let text = "[notify]\n# hush\nquiet = false\n";
        let out = write_flag(text, "notify", "quiet", true).unwrap();
        assert!(Config::parse(&out).unwrap().notify.quiet);
        assert!(out.contains("# hush"));
        let out = write_word(&out, "appearance", "style", "dark").unwrap();
        let out = write_number(&out, "drawers", "bottom", 80).unwrap();
        assert_eq!(Config::parse(&out).unwrap().drawers.bottom, 80.0);
        assert_eq!(Config::parse(&out).unwrap().appearance.style, Some(crate::config::Style::Dark));
    }

    #[test]
    fn the_dock_list_is_rewritten_and_a_removed_builtin_is_switched_off() {
        let text = "[dock]\n# keep me\npinned = [\"foot\"]\nfiles = true\n";
        let out = write_dock(text, &[Launch::App("firefox".into()), Launch::App("foot".into())], &["files"]).unwrap();
        let back = Config::parse(&out).unwrap();
        assert_eq!(back.dock.pinned, vec![Launch::App("firefox".into()), Launch::App("foot".into())]);
        assert!(!back.dock.files);
        assert!(out.contains("# keep me"), "comments survive");
    }

    #[test]
    fn the_dock_switches_write_their_own_keys() {
        let before = model(&Config::default());
        assert_eq!(before.shows, [true; 6]);
        let mut after = before.clone();
        after.set(Id::DockShow(1), Value::Switch(false));
        after.set(Id::DockShow(4), Value::Switch(false));
        let out = after.write(&before, "").unwrap();
        let back = Config::parse(&out).unwrap();
        assert!(!back.dock.terminal && !back.dock.trash);
        assert!(back.dock.files && back.dock.browser && back.dock.settings);
        assert!(!out.contains("files"), "{out}");
        let look = Look::default();
        let groups = after.groups(Section::Dock);
        let rows = look.place(SIZE, Section::Dock, &groups, 0.0)[1]
            .rows
            .clone();
        let terminal = rows[rows.len() - 5];
        assert_eq!(
            look.hit(SIZE, Section::Dock, &after, 0.0, terminal.center()),
            Some(Hit::Change(Id::DockShow(1), Value::Switch(true)))
        );
    }

    #[test]
    fn a_default_app_is_chosen_written_and_reset() {
        let before = model(&Config::default());
        let mut after = before.clone();
        let browser = Role::ALL.iter().position(|role| *role == Role::Browser).unwrap();
        assert_eq!(
            after.press(Id::Default(browser), 0),
            Some(Ask::Pick(For::Default(Role::Browser)))
        );
        after.add(
            For::Default(Role::Browser),
            Launch::App("firefox-esr".into()),
        );
        let out = after.write(&before, "").unwrap();
        let back = Config::parse(&out).unwrap();
        assert_eq!(back.apps.browser, Some(Launch::App("firefox-esr".into())));
        assert_eq!(after.effective(browser), Some(Launch::App("firefox-esr".into())));
        let chosen = after.clone();
        after.press(Id::Default(browser), 1);
        let reset = after.write(&chosen, &out).unwrap();
        assert!(!reset.contains("firefox-esr"), "{reset}");
        Config::parse(&reset).unwrap().validate().unwrap();
    }

    #[test]
    fn the_dock_switches_name_the_app_they_show() {
        let mut model = model(&Config::default());
        model.system = [None, None, Some("pcmanfm".into()), None, None, None, None, None];
        model
            .names
            .insert("pcmanfm".into(), "File Manager PCManFM".into());
        let groups = model.groups(Section::Dock);
        let rows = &groups[1].rows;
        let files = &rows[rows.len() - 6];
        assert_eq!(files.detail.as_deref(), Some("File Manager PCManFM"));
        assert_eq!(rows[rows.len() - 5].detail.as_deref(), Some("None found"));
        assert_eq!(rows[rows.len() - 1].label, "Apps");
    }

    #[test]
    fn the_add_rows_ask_for_the_picker_and_edit_opens_the_item() {
        let mut model = model(&Config::default());
        assert_eq!(model.press(Id::LoginAdd, 0), Some(Ask::Pick(For::Login)));
        assert_eq!(model.press(Id::DockAdd, 0), Some(Ask::Pick(For::Dock)));
        model.add(For::Dock, Launch::App("foot".into()));
        assert_eq!(
            model.press(Id::Dock(0), 2),
            Some(Ask::EditDock(0))
        );
        assert_eq!(model.press(Id::Folder, 0), Some(Ask::Files(For::Folder)));
        assert_eq!(model.press(Id::Dock(0), 3), None);
        assert!(model.dock.is_empty());
    }

    #[test]
    fn the_rotation_splits_into_a_count_and_a_unit() {
        let hours = model(&config("[workspace]\nwallpaper_every = 7200\n"));
        assert_eq!((hours.rotate, hours.unit, hours.every()), (2, 2, 7200));
        let seconds = model(&config("[workspace]\nwallpaper_every = 45\n"));
        assert_eq!((seconds.rotate, seconds.unit), (45, 0));
        let never = model(&Config::default());
        assert_eq!((never.rotate, never.every()), (0, 0));
        let mut changed = never.clone();
        changed.set(Id::Rotate, Value::Number(10.0));
        changed.set(Id::Unit, Value::Choice(1));
        let back = Config::parse(&changed.write(&never, "").unwrap()).unwrap();
        assert_eq!(back.workspace.wallpaper_every, 600);
    }

    #[test]
    fn a_shader_is_clicked_and_saved_by_name() {
        let shelf = std::env::temp_dir().join(format!("drifter-shelf-{}", std::process::id()));
        std::fs::create_dir_all(&shelf).unwrap();
        let before = model(&Config::default());
        let mut after = before.clone();
        after.shaders = vec![shelf.join("ink.shader"), shelf.join("silk.shader")];
        let look = Look::default();
        let groups = after.groups(Section::Wallpaper);
        assert_eq!(groups[1].header, Some("Shaders"));
        let placed = look.place(SIZE, Section::Wallpaper, &groups, 0.0);
        let tiles = placed[1].thumbs[1].center();
        let hit = look.hit(SIZE, Section::Wallpaper, &after, 0.0, tiles);
        assert_eq!(hit, Some(Hit::Picture(Shelf::Shaders, 1)));
        after.choose_picture(Shelf::Shaders, 1);
        assert_eq!(after.groups(Section::Wallpaper)[1].thumbs.as_ref().unwrap().chosen, Some(1));
        let back = Config::parse(&after.write(&before, "").unwrap()).unwrap();
        assert_eq!(back.workspace.wallpaper.as_deref(), Some(std::path::Path::new("silk.shader")));
    }

    #[test]
    fn a_wallpaper_folder_offers_its_pictures_to_click() {
        let folder = std::env::temp_dir().join(format!("drifter-pick-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        for name in ["one.jpg", "two.png"] {
            std::fs::write(folder.join(name), b"x").unwrap();
        }
        let before = model(&Config::default());
        let mut after = before.clone();
        after.choose_folder(folder.clone());
        let look = Look::default();
        let groups = after.groups(Section::Wallpaper);
        let placed = look.place(SIZE, Section::Wallpaper, &groups, 0.0);
        let second = placed.last().unwrap().thumbs[1].center();
        let hit = look.hit(SIZE, Section::Wallpaper, &after, 0.0, second);
        assert_eq!(hit, Some(Hit::Picture(Shelf::Pictures, 1)));
        after.choose_picture(Shelf::Pictures, 1);
        let back = Config::parse(&after.write(&before, "").unwrap()).unwrap();
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(back.workspace.wallpaper_folder, Some(folder.clone()));
        assert_eq!(back.workspace.wallpaper, Some(folder.join("two.png")));
    }

    #[test]
    fn rotation_rows_wait_for_a_folder() {
        let model = model(&Config::default());
        let groups = model.groups(Section::Wallpaper);
        let rows = &groups.iter().find(|group| group.header == Some("Folder")).unwrap().rows;
        assert!(rows[0].enabled);
        assert!(!rows[1].enabled && !rows[2].enabled && !rows[3].enabled);
    }

    const SIZE: Vec2 = Vec2::new(860.0, 620.0);

    #[test]
    fn the_sidebar_has_six_groups_with_a_heading_each() {
        let groups: Vec<&str> = Section::ALL
            .iter()
            .enumerate()
            .filter(|(at, section)| *at == 0 || Section::ALL[at - 1].group() != section.group())
            .map(|(_, section)| section.group())
            .collect();
        assert_eq!(groups, ["Desktop", "Network", "Hardware", "Input", "Users & Security", "System"]);
        let look = Look::default();
        let wifi = Section::ALL.iter().position(|s| *s == Section::Network).unwrap();
        let above = look.item_rect(wifi - 1);
        let head = look.head_rect_in(&Section::ALL, wifi);
        assert!(head.origin.y >= above.max().y && head.max().y <= look.item_rect(wifi).origin.y);
    }

    #[test]
    fn clicking_a_sidebar_item_picks_its_section() {
        let look = Look::default();
        let model = model(&Config::default());
        let at = Section::ALL
            .iter()
            .position(|s| *s == Section::TopBar)
            .unwrap();
        let item = look.item_rect(at).center();
        assert_eq!(
            look.hit(SIZE, Section::Desktop, &model, 0.0, item),
            Some(Hit::Section(Section::TopBar))
        );
    }

    #[test]
    fn the_sidebar_scrolls_to_its_last_section_in_the_smallest_window() {
        let mut look = Look::default();
        let least = look.least();
        look.side_scroll = look.side_most(least.y);
        let last = look.item_rect(Section::ALL.len() - 1);
        assert!(last.max().y < least.y);
        let model = model(&Config::default());
        assert_eq!(
            look.hit(least, Section::Desktop, &model, 0.0, last.center()),
            Some(Hit::Section(Section::About))
        );
        let hidden = Vec2::new(40.0, 20.0);
        assert_eq!(look.hit(least, Section::Desktop, &model, 0.0, hidden), None);
    }

    #[test]
    fn clicking_a_switch_row_flips_it() {
        let look = Look::default();
        let model = model(&Config::default());
        let groups = model.groups(Section::Time);
        let placed = look.place(SIZE, Section::Time, &groups, 0.0);
        let seconds = placed[1].rows[2].center();
        assert_eq!(
            look.hit(SIZE, Section::Time, &model, 0.0, seconds),
            Some(Hit::Change(Id::Seconds, Value::Switch(false)))
        );
    }

    #[test]
    fn five_segments_stay_clear_of_the_row_label() {
        let look = Look::default();
        let row = Rect::new(Vec2::new(0.0, 0.0), Vec2::new(560.0, 44.0));
        let first = look.segment_rect(row, 5, 0);
        let last = look.segment_rect(row, 5, 4);
        assert!(last.max().x - first.origin.x <= SEGMENTS_WIDE);
        assert!(first.origin.x > 200.0, "room is left for the label");
        assert_eq!(look.segment_rect(row, 2, 0).size.x, look.segment.x);
    }

    #[test]
    fn a_greyed_row_ignores_clicks() {
        let look = Look::default();
        let mut model = model(&Config::default());
        model.stats = false;
        let groups = model.groups(Section::TopBar);
        let placed = look.place(SIZE, Section::TopBar, &groups, 0.0);
        let cpu = placed[1].rows[1].center();
        assert_eq!(look.hit(SIZE, Section::TopBar, &model, 0.0, cpu), None);
    }

    #[test]
    fn a_row_button_is_found_by_its_place() {
        let look = Look::default();
        let mut model = model(&Config::default());
        model.add(For::Dock, Launch::App("foot".into()));
        let groups = model.groups(Section::Dock);
        let row = look.place(SIZE, Section::Dock, &groups, 0.0)[1].rows[0];
        let marks = [Mark::Up, Mark::Down, Mark::Text("Icon"), Mark::Minus];
        let spots = look.mark_rects(row, &marks);
        assert!(
            spots
                .windows(2)
                .all(|pair| pair[0].max().x < pair[1].origin.x)
        );
        assert_eq!(spots[3].max().x, row.max().x - 14.0);
        assert_eq!(
            look.hit(SIZE, Section::Dock, &model, 0.0, spots[2].center()),
            Some(Hit::Press(Id::Dock(0), 2))
        );
    }

    #[test]
    fn the_world_slider_runs_from_two_to_ten() {
        let look = Look::default();
        let model = model(&Config::default());
        let groups = model.groups(Section::Desktop);
        let row = look.place(SIZE, Section::Desktop, &groups, 0.0)[0].rows[0];
        let track = look.track_rect(row);
        let left = Vec2::new(track.origin.x, row.center().y);
        let right = Vec2::new(track.max().x, row.center().y);
        assert_eq!(
            look.hit(SIZE, Section::Desktop, &model, 0.0, left),
            Some(Hit::Change(Id::World, Value::Number(2.0)))
        );
        assert_eq!(
            look.hit(SIZE, Section::Desktop, &model, 0.0, right),
            Some(Hit::Change(Id::World, Value::Number(10.0)))
        );
        assert_eq!(
            look.drag(SIZE, Section::Desktop, &model, 0.0, Id::World, -500.0),
            Some(Value::Number(2.0))
        );
    }

    #[test]
    fn clicking_a_segment_picks_it() {
        let look = Look::default();
        let model = model(&Config::default());
        let groups = model.groups(Section::Mouse);
        let row = look.place(SIZE, Section::Mouse, &groups, 0.0)[0].rows[0];
        let alt = look.segment_rect(row, 3, 1).center();
        assert_eq!(
            look.hit(SIZE, Section::Mouse, &model, 0.0, alt),
            Some(Hit::Change(Id::Modifier, Value::Choice(1)))
        );
    }

    #[test]
    fn scrolling_moves_the_rows_up() {
        let look = Look::default();
        let model = model(&Config::default());
        let groups = model.groups(Section::TopBar);
        let still = look.place(SIZE, Section::TopBar, &groups, 0.0);
        let moved = look.place(SIZE, Section::TopBar, &groups, 40.0);
        assert_eq!(moved[0].card.origin.y, still[0].card.origin.y - 40.0);
    }

    #[test]
    fn a_short_window_can_scroll_to_the_last_row() {
        let look = Look::default();
        let model = model(&Config::default());
        let groups = model.groups(Section::TopBar);
        let short = Vec2::new(860.0, 420.0);
        let most = look.max_scroll(short, Section::TopBar, &groups);
        assert!(most > 0.0);
        let placed = look.place(short, Section::TopBar, &groups, most);
        let last = placed.last().unwrap().rows.last().unwrap();
        assert!(last.max().y <= short.y);
    }

    #[test]
    fn a_long_footer_is_cut_to_fit() {
        let font = Font::find(None, 12.0).expect("this machine has no usable font");
        let text = fit(
            &font,
            "a very long line that will never fit in this width",
            80.0,
        );
        assert!(text.ends_with('…'));
        assert!(font.width_of(&text) <= 80.0);
    }
}
