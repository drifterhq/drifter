use std::collections::HashMap;
use std::path::PathBuf;

use crate::config::{ButtonName, Config, Launch, Modifier, Side, WallpaperMode, WallpaperOrder};
use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::sheet::{For, Sheet};
use crate::stats::{self, Metric, Mount};
use crate::text::Font;

pub const WORLD_LEAST: u32 = 2;
pub const WORLD_MOST: u32 = 10;
pub const UNITS: [u64; 3] = [1, 60, 3600];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Desktop,
    Wallpaper,
    Window,
    Dock,
    TopBar,
    Input,
    Lock,
    Login,
}

impl Section {
    pub const ALL: [Section; 8] = [
        Section::Desktop,
        Section::Wallpaper,
        Section::Window,
        Section::Dock,
        Section::TopBar,
        Section::Input,
        Section::Lock,
        Section::Login,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Section::Desktop => "Desktop",
            Section::Wallpaper => "Wallpaper",
            Section::Window => "Windows",
            Section::Dock => "Dock",
            Section::TopBar => "Top Bar",
            Section::Input => "Keyboard & Mouse",
            Section::Lock => "Lock Screen",
            Section::Login => "Login Items",
        }
    }

    fn tint(self) -> [u8; 3] {
        match self {
            Section::Desktop => [52, 168, 140],
            Section::Wallpaper => [40, 170, 210],
            Section::Window => [88, 110, 230],
            Section::Dock => [120, 128, 146],
            Section::TopBar => [240, 140, 50],
            Section::Input => [150, 110, 220],
            Section::Lock => [96, 102, 120],
            Section::Login => [220, 80, 90],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    Pick(For),
    Files(For),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disk {
    pub label: String,
    pub point: String,
    pub on: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub world: u32,
    pub wallpaper_mode: WallpaperMode,
    pub modifier: Modifier,
    pub top: f64,
    pub bottom: f64,
    pub text_size: f64,
    pub day: bool,
    pub date: bool,
    pub seconds: bool,
    pub hours24: bool,
    pub stats: bool,
    pub cpu: bool,
    pub ram: bool,
    pub disks: Vec<Disk>,
    pub kept: Vec<String>,
    pub side: Side,
    pub order: [ButtonName; 3],
    pub title: bool,
    pub lock_after: u64,
    pub login: Vec<Launch>,
    pub dock: Vec<Launch>,
    pub wallpaper: Option<PathBuf>,
    pub folder: Option<PathBuf>,
    pub rotate: u64,
    pub unit: usize,
    pub rotation: WallpaperOrder,
    pub pictures: Vec<PathBuf>,
    pub names: HashMap<String, String>,
}

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
            top: config.drawers.top,
            bottom: config.drawers.bottom,
            text_size: config.tray.size,
            day: has(&["%a", "%A"]),
            date: has(&["%d", "%e", "%b", "%B"]),
            seconds: has(&["%S"]),
            hours24: has(&["%H", "%k"]) && !has(&["%I", "%l"]),
            stats: config.tray.stats,
            cpu,
            ram,
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
            lock_after: config.lock.after,
            login: config.login.start.clone(),
            dock: config.dock.pinned.clone(),
            wallpaper: config.wallpaper_path().map(clean),
            folder: config.wallpaper_folder().map(clean),
            rotate: every / UNITS[unit],
            unit,
            rotation: config.workspace.wallpaper_order,
            pictures: config
                .wallpaper_folder()
                .map(|folder| crate::wallpaper::pictures(&clean(folder)))
                .unwrap_or_default(),
            names: HashMap::new(),
        }
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

    pub fn icon_key(item: &Launch) -> String {
        match (item.icon(), item.app()) {
            (Some(path), _) => format!("file:{}", path.display()),
            (None, Some(app)) => format!("app:{app}"),
            (None, None) => format!("cmd:{}", item.name()),
        }
    }

    fn launch_row(&self, id: Id, item: &Launch, marks: Vec<Mark>) -> Row {
        let detail = match (item.command(), item.icon()) {
            (Some(_), Some(_)) => "Command line, own icon".to_string(),
            (Some(_), None) => "Command line".to_string(),
            (None, Some(_)) => format!("{}, own icon", item.name()),
            (None, None) => "Application".to_string(),
        };
        let mut row = Row::new(id, &self.name_of(item), Control::Buttons(marks)).detail(&detail);
        row.icon = Some(Model::icon_key(item));
        row
    }

    pub fn groups(&self, section: Section) -> Vec<Group> {
        match section {
            Section::Desktop => vec![Group {
                header: None,
                rows: vec![
                    Row::new(
                        Id::World,
                        "World size",
                        Control::Slider {
                            least: f64::from(WORLD_LEAST),
                            most: f64::from(WORLD_MOST),
                            value: f64::from(self.world),
                            unit: "",
                            zero: None,
                        },
                    )
                    .detail("Screens across and screens down"),
                ],
                footer: Some(
                    "Hold the camera key and push an edge to pan, or scroll to zoom out.".into(),
                ),
                thumbs: None,
            }],
            Section::Wallpaper => self.wallpaper_groups(),
            Section::Window => vec![
                Group {
                    header: None,
                    rows: vec![
                        Row::new(
                            Id::Side,
                            "Buttons",
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
                            "Show the window title",
                            Control::Switch(self.title),
                        ),
                    ],
                    footer: None,
                    thumbs: None,
                },
                Group {
                    header: Some("Button order"),
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
                    footer: Some("Left to right, as they sit in the title bar.".into()),
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
                            vec![Mark::Up, Mark::Down, Mark::Text("Icon"), Mark::Minus],
                        )
                    })
                    .collect();
                rows.push(
                    Row::new(
                        Id::DockAdd,
                        "Add an app or a command",
                        Control::Buttons(vec![Mark::Plus]),
                    )
                    .faint(),
                );
                vec![
                    Group {
                        header: None,
                        rows: vec![Row::new(
                            Id::Bottom,
                            "Size",
                            Control::Slider {
                                least: 16.0,
                                most: 160.0,
                                value: self.bottom,
                                unit: " px",
                                zero: None,
                            },
                        )],
                        footer: None,
                        thumbs: None,
                    },
                    Group {
                        header: Some("Dock items"),
                        rows,
                        footer: Some(
                            "With no items, the dock shows your browser, files and terminal."
                                .into(),
                        ),
                        thumbs: None,
                    },
                ]
            }
            Section::TopBar => {
                let mut info = vec![
                    Row::new(Id::Stats, "Show system info", Control::Switch(self.stats)),
                    Row::new(Id::Cpu, "CPU", Control::Switch(self.cpu))
                        .detail("All cores together")
                        .enabled(self.stats),
                    Row::new(Id::Ram, "Memory", Control::Switch(self.ram))
                        .detail("Memory in use")
                        .enabled(self.stats),
                ];
                for (index, disk) in self.disks.iter().enumerate() {
                    info.push(
                        Row::new(Id::Disk(index), &disk.label, Control::Switch(disk.on))
                            .detail(&format!("Disk mounted at {}", disk.point))
                            .enabled(self.stats),
                    );
                }
                vec![
                    Group {
                        header: None,
                        rows: vec![
                            Row::new(
                                Id::Top,
                                "Bar height",
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
                                "Text size",
                                Control::Slider {
                                    least: 8.0,
                                    most: 32.0,
                                    value: self.text_size,
                                    unit: " px",
                                    zero: None,
                                },
                            ),
                        ],
                        footer: None,
                        thumbs: None,
                    },
                    Group {
                        header: Some("Clock"),
                        rows: vec![
                            Row::new(
                                Id::Day,
                                "Show the day of the week",
                                Control::Switch(self.day),
                            ),
                            Row::new(Id::Date, "Show the date", Control::Switch(self.date)),
                            Row::new(Id::Seconds, "Show seconds", Control::Switch(self.seconds)),
                            Row::new(
                                Id::Hours24,
                                "Use a 24-hour clock",
                                Control::Switch(self.hours24),
                            ),
                        ],
                        footer: None,
                        thumbs: None,
                    },
                    Group {
                        header: Some("System info"),
                        rows: info,
                        footer: Some(
                            "Each one shows as a percentage, right of the tray icons.".into(),
                        ),
                        thumbs: None,
                    },
                ]
            }
            Section::Input => vec![Group {
                header: None,
                rows: vec![Row::new(
                    Id::Modifier,
                    "Camera key",
                    Control::Segments {
                        options: vec!["Super", "Alt", "Ctrl"],
                        chosen: match self.modifier {
                            Modifier::Super => 0,
                            Modifier::Alt => 1,
                            Modifier::Ctrl => 2,
                        },
                    },
                )],
                footer: Some(
                    "Hold it to zoom with the scroll wheel, and to pan by pushing an edge.".into(),
                ),
                thumbs: None,
            }],
            Section::Lock => vec![Group {
                header: None,
                rows: vec![
                    Row::new(
                        Id::LockAfter,
                        "Lock the screen after",
                        Control::Slider {
                            least: 0.0,
                            most: 60.0,
                            value: self.lock_after as f64,
                            unit: " min",
                            zero: Some("Never"),
                        },
                    )
                    .detail("Minutes with no key press or pointer move"),
                ],
                footer: Some("A video that is playing keeps the screen awake.".into()),
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
                        "Add an app or a command",
                        Control::Buttons(vec![Mark::Plus]),
                    )
                    .faint(),
                );
                vec![Group {
                    header: Some("Start when Drifter starts"),
                    rows,
                    footer: Some("They start in this order, and again after a crash.".into()),
                    thumbs: None,
                }]
            }
        }
    }

    fn wallpaper_groups(&self) -> Vec<Group> {
        let folder = self.folder.is_some();
        let turning = folder && self.rotate > 0;
        let folder_text = self
            .folder
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "None chosen".to_string());
        let mut groups = vec![
            Group {
                header: None,
                rows: vec![Row::new(
                    Id::WallpaperMode,
                    "Behavior",
                    Control::Segments {
                        options: vec!["Fixed", "Pan", "Parallax"],
                        chosen: match self.wallpaper_mode {
                            WallpaperMode::Fixed => 0,
                            WallpaperMode::Pan => 1,
                            WallpaperMode::Parallax => 2,
                        },
                    },
                )],
                footer: Some(
                    match self.wallpaper_mode {
                        WallpaperMode::Fixed => "The picture stays put while the camera moves.",
                        WallpaperMode::Pan => {
                            "The picture covers the whole world and moves with it."
                        }
                        WallpaperMode::Parallax => {
                            "The picture moves slower than the world, for depth."
                        }
                    }
                    .to_string(),
                ),
                thumbs: None,
            },
            Group {
                header: Some("Folder"),
                rows: vec![
                    Row::new(
                        Id::Folder,
                        "Pictures from",
                        Control::Buttons(vec![Mark::Text("Choose")]),
                    )
                    .detail(&folder_text),
                    Row::new(
                        Id::Rotate,
                        "Change the picture every",
                        Control::Slider {
                            least: 0.0,
                            most: 60.0,
                            value: self.rotate as f64,
                            unit: ["s", "min", "h"][self.unit.min(2)],
                            zero: Some("Never"),
                        },
                    )
                    .enabled(folder),
                    Row::new(
                        Id::Unit,
                        "Counted in",
                        Control::Segments {
                            options: vec!["Seconds", "Minutes", "Hours"],
                            chosen: self.unit.min(2),
                        },
                    )
                    .enabled(turning),
                    Row::new(
                        Id::Rotation,
                        "Order",
                        Control::Segments {
                            options: vec!["Random", "By name"],
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
        if !self.pictures.is_empty() {
            let chosen = self
                .wallpaper
                .as_ref()
                .and_then(|now| self.pictures.iter().position(|path| path == now));
            groups.push(Group {
                header: Some("Pictures"),
                rows: Vec::new(),
                footer: Some("Click a picture to use it now.".into()),
                thumbs: Some(Thumbs {
                    paths: self.pictures.clone(),
                    chosen,
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
            (Id::Day, Value::Switch(on)) => self.day = on,
            (Id::Date, Value::Switch(on)) => self.date = on,
            (Id::Seconds, Value::Switch(on)) => self.seconds = on,
            (Id::Hours24, Value::Switch(on)) => self.hours24 = on,
            (Id::Stats, Value::Switch(on)) => self.stats = on,
            (Id::Cpu, Value::Switch(on)) => self.cpu = on,
            (Id::Ram, Value::Switch(on)) => self.ram = on,
            (Id::Disk(index), Value::Switch(on)) => {
                if let Some(disk) = self.disks.get_mut(index) {
                    disk.on = on;
                }
            }
            _ => {}
        }
    }

    pub fn press(&mut self, id: Id, index: usize) -> Option<Ask> {
        match (id, index) {
            (Id::Order(slot), 0) if slot > 0 && slot < 3 => self.order.swap(slot, slot - 1),
            (Id::Order(slot), 1) if slot < 2 => self.order.swap(slot, slot + 1),
            (Id::Folder, _) => return Some(Ask::Files(For::Folder)),
            (Id::Login(at), 0) if at < self.login.len() => {
                self.login.remove(at);
            }
            (Id::LoginAdd, _) => return Some(Ask::Pick(For::Login)),
            (Id::Dock(at), 0) if at > 0 && at < self.dock.len() => self.dock.swap(at, at - 1),
            (Id::Dock(at), 1) if at + 1 < self.dock.len() => self.dock.swap(at, at + 1),
            (Id::Dock(at), 2) if at < self.dock.len() => {
                return Some(Ask::Files(For::DockIcon(at)));
            }
            (Id::Dock(at), 3) if at < self.dock.len() => {
                self.dock.remove(at);
            }
            (Id::DockAdd, _) => return Some(Ask::Pick(For::Dock)),
            _ => {}
        }
        None
    }

    pub fn add(&mut self, purpose: For, item: Launch) {
        match purpose {
            For::Login => self.login.push(item),
            For::Dock => self.dock.push(item),
            _ => {}
        }
    }

    pub fn set_icon(&mut self, at: usize, icon: Option<PathBuf>) {
        let Some(item) = self.dock.get_mut(at) else {
            return;
        };
        let app = item.app().map(str::to_string);
        let command = item.command().map(str::to_string);
        *item = match (app, command, icon) {
            (Some(app), None, None) => Launch::App(app),
            (app, command, icon) => Launch::Full { app, command, icon },
        };
    }

    pub fn choose_folder(&mut self, folder: PathBuf) {
        self.pictures = crate::wallpaper::pictures(&folder);
        self.folder = Some(folder);
    }

    pub fn choose_picture(&mut self, at: usize) {
        if let Some(path) = self.pictures.get(at) {
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
            put(
                &mut doc,
                "workspace",
                "wallpaper",
                word(&path.to_string_lossy()),
            );
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
        if self.top != before.top {
            put(&mut doc, "drawers", "top", int(self.top));
        }
        if self.bottom != before.bottom {
            put(&mut doc, "drawers", "bottom", int(self.bottom));
        }
        if self.text_size != before.text_size {
            put(&mut doc, "tray", "size", int(self.text_size));
        }
        if self.format() != before.format() {
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
        if self.lock_after != before.lock_after {
            put(&mut doc, "lock", "after", int(self.lock_after as f64));
        }
        if self.login != before.login {
            put(&mut doc, "login", "start", launches(&self.login));
        }
        if self.dock != before.dock {
            put(&mut doc, "dock", "pinned", launches(&self.dock));
        }
        Ok(doc.to_string())
    }
}

fn clean(path: PathBuf) -> PathBuf {
    std::fs::canonicalize(&path).unwrap_or(path)
}

fn button_label(name: ButtonName) -> &'static str {
    match name {
        ButtonName::Close => "Close",
        ButtonName::Minimize => "Minimize",
        ButtonName::Maximize => "Maximize",
    }
}

fn button_word(name: ButtonName) -> &'static str {
    match name {
        ButtonName::Close => "close",
        ButtonName::Minimize => "minimize",
        ButtonName::Maximize => "maximize",
    }
}

fn launches(items: &[Launch]) -> toml_edit::Value {
    let list: toml_edit::Array = items
        .iter()
        .map(|item| match item {
            Launch::App(app) => toml_edit::Value::from(app.as_str()),
            Launch::Full { app, command, icon } => {
                let mut table = toml_edit::InlineTable::new();
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
                toml_edit::Value::InlineTable(table)
            }
        })
        .collect();
    toml_edit::Value::Array(list)
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
    Disk(usize),
    Side,
    Title,
    Order(usize),
    LockAfter,
    Folder,
    Rotate,
    Unit,
    Rotation,
    Login(usize),
    LoginAdd,
    Dock(usize),
    DockAdd,
}

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
        }
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
    pub paths: Vec<PathBuf>,
    pub chosen: Option<usize>,
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
    Section(Section),
    Change(Id, Value),
    Press(Id, usize),
    Picture(usize),
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
}

impl Default for Look {
    fn default() -> Self {
        Look {
            sidebar: 220.0,
            item: 32.0,
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
            back: [30, 32, 40],
            side: [37, 40, 50],
            card: [42, 45, 57],
            rule: [58, 62, 78],
            ink: [236, 238, 244],
            dim: [146, 152, 166],
            accent: [64, 128, 238],
            off: [72, 77, 94],
            pick: [98, 105, 128],
        }
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

const THUMB_GAP: f64 = 10.0;
const THUMB_PAD: f64 = 14.0;

impl Look {
    pub fn least(&self) -> Vec2 {
        Vec2::new(self.sidebar + 440.0, 440.0)
    }

    pub fn item_rect(&self, index: usize) -> Rect {
        Rect::new(
            Vec2::new(10.0, 52.0 + index as f64 * (self.item + 2.0)),
            Vec2::new(self.sidebar - 20.0, self.item),
        )
    }

    fn column(&self, size: Vec2) -> (f64, f64) {
        let room = size.x - self.sidebar - self.pad * 2.0;
        let width = room.min(self.widest).max(200.0);
        (self.sidebar + self.pad + (room - width) / 2.0, width)
    }

    pub fn title_at(&self, size: Vec2, scroll: f64) -> Vec2 {
        Vec2::new(self.column(size).0, 22.0 - scroll)
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
        let whole = self.segment.x * count as f64;
        let left = row.max().x - 14.0 - whole;
        Rect::new(
            Vec2::new(
                left + self.segment.x * slot as f64,
                row.center().y - self.segment.y / 2.0,
            ),
            self.segment,
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

    pub fn hit(
        &self,
        size: Vec2,
        section: Section,
        model: &Model,
        scroll: f64,
        pointer: Vec2,
    ) -> Option<Hit> {
        if pointer.x < self.sidebar {
            return Section::ALL
                .iter()
                .enumerate()
                .find(|(index, _)| self.item_rect(*index).contains(pointer))
                .map(|(_, section)| Hit::Section(*section));
        }
        let groups = model.groups(section);
        let placed = self.place(size, section, &groups, scroll);
        for (group, placed) in groups.iter().zip(&placed) {
            if let Some(at) = placed.thumbs.iter().position(|rect| rect.contains(pointer)) {
                return Some(Hit::Picture(at));
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
                        .map(|at| Hit::Press(row.id, at)),
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
            canvas.round_rect(rect, self.radius, self.card, 1.0);
            match section {
                Section::Desktop => self.paint_world(&mut canvas, rect, model.world, fonts),
                Section::Window => self.paint_title_bar(&mut canvas, rect, model, fonts),
                _ => self.paint_wallpaper(&mut canvas, rect, art.wallpaper),
            }
        }

        for (group, placed) in groups
            .iter()
            .zip(self.place(size, section, &groups, scroll))
        {
            if let (Some(header), Some(at)) = (group.header, placed.header) {
                let text = fonts.body.paint(header, self.dim);
                canvas.stamp(&text, at, 1.0);
            }
            canvas.round_rect(placed.card, self.radius, self.card, 1.0);
            for (index, (row, rect)) in group.rows.iter().zip(&placed.rows).enumerate() {
                if index > 0 {
                    let line = Rect::new(
                        Vec2::new(rect.origin.x + 14.0, rect.origin.y),
                        Vec2::new(rect.size.x - 14.0, 1.0),
                    );
                    canvas.round_rect(line, 0.0, self.rule, 1.0);
                }
                self.paint_row(&mut canvas, row, *rect, fonts, art.icons);
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
            if let (Some(footer), Some(rect)) = (&group.footer, placed.footer) {
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
        let heading = fonts.small.paint("Settings", self.dim);
        canvas.stamp(&heading, Vec2::new(22.0, 24.0), 1.0);
        for (index, each) in Section::ALL.iter().enumerate() {
            let rect = self.item_rect(index);
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
                rect.center().y - f64::from(label.height) / 2.0,
            );
            canvas.stamp(&label, at, 1.0);
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
    ) {
        let fade = if row.enabled { 1.0 } else { 0.4 };
        let mut x = rect.origin.x + 14.0;
        if let Some(key) = &row.icon {
            let spot = Rect::new(Vec2::new(x, rect.center().y - 13.0), Vec2::splat(26.0));
            match icons.get(key) {
                Some(image) => canvas.stamp(image, spot.center() - image.size() / 2.0, fade),
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
            _ => rect.size.x * 0.55,
        };
        let ink = if row.faint { self.dim } else { self.ink };
        let label = fonts.body.paint(&fit(fonts.body, &row.label, room), ink);
        match &row.detail {
            Some(detail) => {
                let small = fonts.small.paint(&fit(fonts.small, detail, room), self.dim);
                let total = f64::from(label.height + small.height);
                let top = rect.center().y - total / 2.0;
                canvas.stamp(&label, Vec2::new(x, top), fade);
                canvas.stamp(&small, Vec2::new(x, top + f64::from(label.height)), fade);
            }
            None => {
                let at = Vec2::new(x, rect.center().y - f64::from(label.height) / 2.0);
                canvas.stamp(&label, at, fade);
            }
        }

        match &row.control {
            Control::Switch(on) => {
                let pill = self.switch_rect(rect);
                let color = if *on { self.accent } else { self.off };
                canvas.round_rect(pill, pill.size.y, color, fade);
                let knob = pill.size.y / 2.0 - 2.0;
                let cx = match on {
                    true => pill.max().x - 2.0 - knob,
                    false => pill.origin.x + 2.0 + knob,
                };
                let center = Vec2::new(cx, pill.center().y);
                canvas.circle(
                    center + Vec2::new(0.0, 1.0),
                    knob + 0.5,
                    [0, 0, 0],
                    0.25 * fade,
                );
                canvas.circle(center, knob, [255, 255, 255], fade);
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
                canvas.circle(knob + Vec2::new(0.0, 1.0), 9.5, [0, 0, 0], 0.3 * fade);
                canvas.circle(knob, 9.0, [255, 255, 255], fade);
                let words = match (zero, value.round() <= *least) {
                    (Some(zero), true) => zero.to_string(),
                    _ if unit.starts_with(' ') || unit.is_empty() => {
                        format!("{}{unit}", value.round() as i64)
                    }
                    _ => format!("{} {unit}", value.round() as i64),
                };
                let reading = fonts.body.paint(&words, self.ink);
                let at = Vec2::new(
                    rect.max().x - 14.0 - f64::from(reading.width),
                    rect.center().y - f64::from(reading.height) / 2.0,
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
                        - Vec2::new(f64::from(text.width), f64::from(text.height)) / 2.0;
                    canvas.stamp(&text, at, fade);
                }
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
                            canvas.stamp(&painted, c - painted.size() / 2.0, fade);
                        }
                    }
                }
            }
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
            &format!("{world} × {world} screens. The blue screen is what you see at zoom 1."),
            self.dim,
        );
        let at = Vec2::new(
            card.center().x - f64::from(caption.width) / 2.0,
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
            let text = fonts.small.paint("Window title", self.dim);
            let at = Vec2::new(
                window.center().x - f64::from(text.width) / 2.0,
                window.origin.y + 15.0 - f64::from(text.height) / 2.0,
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
                let piece = cover(
                    art,
                    shown.size.x.round() as u32,
                    shown.size.y.round() as u32,
                );
                canvas.stamp(&piece, shown.origin, 1.0);
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
        Section::Desktop | Section::Wallpaper | Section::Window
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
        Section::Input => {
            canvas.round_rect(r(3.0, 6.0, 16.0, 10.0), 2.0, white, 1.0);
            let key = section.tint();
            for x in [5.5, 9.0, 12.5] {
                canvas.round_rect(r(x, 8.0, 2.5, 2.0), 0.5, key, 1.0);
            }
            canvas.round_rect(r(6.0, 12.0, 10.0, 2.0), 0.5, key, 1.0);
        }
        Section::Lock => {
            canvas.circle(o + Vec2::new(11.0, 9.0), 4.5, white, 1.0);
            canvas.circle(o + Vec2::new(11.0, 9.0), 2.5, section.tint(), 1.0);
            canvas.round_rect(r(5.0, 10.0, 12.0, 8.0), 1.5, white, 1.0);
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

    fn model(config: &Config) -> Model {
        Model::read(config, &stats::mounts(MOUNTINFO), kernel)
    }

    fn config(text: &str) -> Config {
        Config::parse(text).unwrap()
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
        assert!(model.stats && model.cpu && model.ram);
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
    fn login_and_dock_items_round_trip_through_the_config() {
        let before = model(&Config::default());
        let mut after = before.clone();
        after.add(For::Login, Launch::App("firefox".into()));
        after.add(
            For::Login,
            Launch::Full {
                app: None,
                command: Some("foot -e htop".into()),
                icon: None,
            },
        );
        after.add(For::Dock, Launch::App("foot".into()));
        after.set_icon(0, Some(PathBuf::from("/icons/foot.png")));
        let back = Config::parse(&after.write(&before, "").unwrap()).unwrap();
        assert_eq!(back.login.start, after.login);
        assert_eq!(
            back.dock.pinned[0].icon(),
            Some(std::path::Path::new("/icons/foot.png"))
        );
        back.validate().unwrap();
        let mut plain = after.clone();
        plain.set_icon(0, None);
        assert_eq!(plain.dock[0], Launch::App("foot".into()));
    }

    #[test]
    fn the_add_rows_ask_for_the_picker_and_icon_asks_for_files() {
        let mut model = model(&Config::default());
        assert_eq!(model.press(Id::LoginAdd, 0), Some(Ask::Pick(For::Login)));
        assert_eq!(model.press(Id::DockAdd, 0), Some(Ask::Pick(For::Dock)));
        model.add(For::Dock, Launch::App("foot".into()));
        assert_eq!(
            model.press(Id::Dock(0), 2),
            Some(Ask::Files(For::DockIcon(0)))
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
        assert_eq!(hit, Some(Hit::Picture(1)));
        after.choose_picture(1);
        let back = Config::parse(&after.write(&before, "").unwrap()).unwrap();
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(back.workspace.wallpaper_folder, Some(folder.clone()));
        assert_eq!(back.workspace.wallpaper, Some(folder.join("two.png")));
    }

    #[test]
    fn rotation_rows_wait_for_a_folder() {
        let model = model(&Config::default());
        let groups = model.groups(Section::Wallpaper);
        let rows = &groups[1].rows;
        assert!(rows[0].enabled);
        assert!(!rows[1].enabled && !rows[2].enabled && !rows[3].enabled);
    }

    const SIZE: Vec2 = Vec2::new(860.0, 620.0);

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
    fn every_section_fits_in_the_sidebar_of_the_smallest_window() {
        let look = Look::default();
        let last = look.item_rect(Section::ALL.len() - 1);
        assert!(last.max().y < look.least().y);
    }

    #[test]
    fn clicking_a_switch_row_flips_it() {
        let look = Look::default();
        let model = model(&Config::default());
        let groups = model.groups(Section::TopBar);
        let placed = look.place(SIZE, Section::TopBar, &groups, 0.0);
        let seconds = placed[1].rows[2].center();
        assert_eq!(
            look.hit(SIZE, Section::TopBar, &model, 0.0, seconds),
            Some(Hit::Change(Id::Seconds, Value::Switch(false)))
        );
    }

    #[test]
    fn a_greyed_row_ignores_clicks() {
        let look = Look::default();
        let mut model = model(&Config::default());
        model.stats = false;
        let groups = model.groups(Section::TopBar);
        let placed = look.place(SIZE, Section::TopBar, &groups, 0.0);
        let cpu = placed[2].rows[1].center();
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
        let groups = model.groups(Section::Input);
        let row = look.place(SIZE, Section::Input, &groups, 0.0)[0].rows[0];
        let alt = look.segment_rect(row, 3, 1).center();
        assert_eq!(
            look.hit(SIZE, Section::Input, &model, 0.0, alt),
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
