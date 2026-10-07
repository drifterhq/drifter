use std::path::PathBuf;

use super::{
    Control, Group, Id, Mark, Model, NetAsk, PROFILES, Row, Value, drop_key, put,
};
use crate::config::{
    ShotFormat,
    Accent, CORNER_SIZE, CURSOR_SIZES, FASTEST, LONGEST_NOTE, Lid, Modifier, PAN_SPEED, PowerKey,
    REPEAT_DELAY, REPEAT_RATE, SLIDE_MS, Style, WAIT_MS,
};
use crate::keyboard;
use crate::settings::Ask;
use crate::sheet::For;
use crate::shortcut::{Action, Combo};

pub const ASSETS: [(&str, &str, &str); 4] = [
    ("cursor", "Pointer picture", "From the cursor theme"),
    ("font", "Font", "A system font"),
    ("window", "Window art", "Drawn by Drifter"),
    ("dock", "Dock art", "Drawn by Drifter"),
];

pub fn place_button(name: &str) -> crate::config::Launch {
    crate::config::Launch::Full {
        app: None,
        command: Some(crate::place::dock_command(name)),
        icon: None,
    }
}

pub const ASSET_FOR: [For; 4] = [For::Pointer, For::Font, For::WindowArt, For::DockArt];

const SLEEP_MOST: f64 = 120.0;
const NOTE_MOST: f64 = 30.0;
const SHOWN_NOTES: usize = 8;

fn slider(id: Id, label: &str, range: (u32, u32), value: u32, unit: &'static str) -> Row {
    Row::new(
        id,
        label,
        Control::Slider {
            least: f64::from(range.0),
            most: f64::from(range.1),
            value: f64::from(value),
            unit,
            zero: None,
        },
    )
}

fn number(n: f64, range: (u32, u32)) -> u32 {
    n.round().clamp(f64::from(range.0), f64::from(range.1)) as u32
}

fn choose_and_reset(set: bool) -> Control {
    Control::Buttons(match set {
        true => vec![Mark::Text("Choose"), Mark::Text("Reset")],
        false => vec![Mark::Text("Choose")],
    })
}

pub fn lid_word(lid: Lid) -> &'static str {
    match lid {
        Lid::Suspend => "suspend",
        Lid::Lock => "lock",
        Lid::Nothing => "nothing",
    }
}

pub fn button_word(key: PowerKey) -> &'static str {
    match key {
        PowerKey::Menu => "menu",
        PowerKey::Suspend => "suspend",
        PowerKey::Off => "off",
        PowerKey::Nothing => "nothing",
    }
}

impl Model {
    pub(super) fn motion_groups(&self) -> Vec<super::Group> {
        let motion = &self.motion;
        vec![
            Group {
                header: Some("Hot corner"),
                rows: vec![
                    Row::new(
                        Id::HotCorner,
                        "Hot corner",
                        Control::Switch(motion.hot_corner),
                    )
                    .detail("Rest the pointer in the top left corner to zoom out"),
                    slider(
                        Id::CornerSize,
                        "Size",
                        CORNER_SIZE,
                        motion.corner_size,
                        " px",
                    )
                    .enabled(motion.hot_corner),
                    slider(Id::CornerWait, "Wait", WAIT_MS, motion.corner_wait, " ms")
                        .detail("How long the pointer rests before it fires")
                        .enabled(motion.hot_corner),
                ],
                footer: None,
                thumbs: None,
            },
            Group {
                header: Some("Moving around"),
                rows: vec![
                    slider(
                        Id::PanSpeed,
                        "Edge pan speed",
                        PAN_SPEED,
                        motion.pan_speed,
                        "%",
                    )
                    .detail("How far the world moves when you push an edge"),
                    slider(Id::Flight, "Camera flight", SLIDE_MS, motion.flight, " ms")
                        .detail("How long the camera takes to fly to a window"),
                ],
                footer: None,
                thumbs: None,
            },
            Group {
                header: Some("Bars"),
                rows: vec![
                    Row::new(
                        Id::HideTop,
                        "Auto hide top bar",
                        Control::Switch(motion.hide_top),
                    )
                    .detail("Off keeps the top bar on screen, and maximized windows leave room for it"),
                    Row::new(
                        Id::HideBottom,
                        "Auto hide bottom bar",
                        Control::Switch(motion.hide_bottom),
                    )
                    .detail("Off keeps the dock on screen, and maximized windows leave room for it"),
                    slider(Id::BarOpen, "Open after", WAIT_MS, motion.bar_open, " ms")
                        .detail("How long the pointer rests at the edge"),
                    slider(
                        Id::BarClose,
                        "Close after",
                        WAIT_MS,
                        motion.bar_close,
                        " ms",
                    )
                    .detail("How long after the pointer leaves"),
                    slider(
                        Id::BarSlide,
                        "Slide time",
                        SLIDE_MS,
                        motion.bar_slide,
                        " ms",
                    ),
                ],
                footer: None,
                thumbs: None,
            },
        ]
    }

    pub(super) fn keyboard_group(&self) -> Group {
        let keys = &self.keyboard;
        let layout = keyboard::name_of(&self.layouts, &keys.layout);
        let variant = match keys.variant.as_str() {
            "" => "Plain".to_string(),
            code => self
                .variants
                .iter()
                .find(|variant| variant.code == code)
                .map(|variant| variant.name.clone())
                .unwrap_or_else(|| code.to_string()),
        };
        let mut rows = vec![
            Row::new(
                Id::Layout,
                "Layout",
                Control::Buttons(vec![Mark::Text("Choose")]),
            )
            .detail(&layout),
            Row::new(
                Id::Variant,
                "Variant",
                Control::Buttons(vec![Mark::Text("Choose")]),
            )
            .detail(&variant),
            Row::new(
                Id::Caps,
                "Caps Lock acts as",
                Control::Segments {
                    options: keyboard::CAPS.iter().map(|(_, name)| *name).collect(),
                    chosen: keyboard::caps(&keys.options),
                },
            ),
            Row::new(
                Id::Compose,
                "Compose key",
                Control::Segments {
                    options: keyboard::COMPOSE.iter().map(|(_, name)| *name).collect(),
                    chosen: keyboard::compose(&keys.options),
                },
            )
            .detail("Hold it, then type two keys for é, ñ or ©"),
        ];
        let others = keyboard::others(&keys.options);
        if !others.is_empty() {
            rows.push(Row::new(
                Id::Options,
                "Other xkb options",
                Control::Note(others),
            ));
        }
        let (delay_least, delay_most) = REPEAT_DELAY;
        let (rate_least, rate_most) = REPEAT_RATE;
        rows.push(
            slider(
                Id::RepeatDelay,
                "Repeat after",
                (delay_least, delay_most),
                keys.repeat_delay,
                " ms",
            )
            .detail("How long a key is held before it repeats"),
        );
        rows.push(
            slider(
                Id::RepeatRate,
                "Repeat speed",
                (rate_least, rate_most),
                keys.repeat_rate,
                "",
            )
            .detail("Repeats a second while a key is held"),
        );
        Group {
            header: None,
            rows,
            footer: Some("Other xkb options can be added to the config file.".into()),
            thumbs: None,
        }
    }

    pub(super) fn mouse_groups(&self) -> Vec<Group> {
        let mouse = &self.mouse;
        vec![
            Group {
                header: None,
                rows: vec![Row::new(
                    Id::Modifier,
                    "Hold to move the camera",
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
            },
            Group {
                header: Some("Mouse"),
                rows: vec![
                    slider(
                        Id::MouseSpeed,
                        "Pointer speed",
                        (0, FASTEST),
                        mouse.speed,
                        "",
                    ),
                    Row::new(Id::MouseAccel, "Acceleration", Control::Switch(mouse.accel))
                        .detail("A fast move takes the pointer farther"),
                    Row::new(
                        Id::MouseNatural,
                        "Natural scrolling",
                        Control::Switch(mouse.natural),
                    )
                    .detail("The page follows the wheel"),
                    Row::new(
                        Id::LeftHanded,
                        "Primary button",
                        Control::Segments {
                            options: vec!["Left", "Right"],
                            chosen: usize::from(mouse.left_handed),
                        },
                    )
                    .detail("Right swaps the buttons, also on the touchpad"),
                ],
                footer: None,
                thumbs: None,
            },
        ]
    }

    pub fn rename_place(&mut self, at: usize, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Type a name for this place.".to_string());
        }
        if self.places.iter().enumerate().any(|(other, place)| other != at && place.name == name) {
            return Err(format!("Another place is already called {name}."));
        }
        let Some(place) = self.places.get_mut(at) else {
            return Err("That place is gone.".to_string());
        };
        let (old, new) = (place_button(&place.name), place_button(name));
        place.name = name.to_string();
        for each in &mut self.dock {
            if *each == old {
                *each = new.clone();
            }
        }
        Ok(())
    }

    pub(super) fn appearance_groups(&self) -> Vec<Group> {
        let look = &self.appearance;
        let named = |name: &Option<String>, none: &str| name.clone().unwrap_or(none.to_string());
        vec![
            Group {
                header: Some("Style"),
                rows: vec![
                    Row::new(
                        Id::Style,
                        "Light or dark",
                        Control::Segments {
                            options: vec!["Light", "Dark", "Auto"],
                            chosen: match look.style {
                                Some(Style::Light) => 0,
                                None | Some(Style::Dark) => 1,
                                Some(Style::Auto) => 2,
                            },
                        },
                    ),
                    Row::new(
                        Id::AccentColor,
                        "Accent color",
                        Control::Swatches {
                            colors: Accent::ALL.iter().map(|accent| accent.color()).collect(),
                            chosen: look
                                .accent
                                .and_then(|held| Accent::ALL.iter().position(|a| *a == held)),
                        },
                    ),
                ],
                footer: Some(
                    "Auto is dark from the night light start hour to its end hour, under Displays. GTK, libadwaita and Qt apps switch with Drifter.".into(),
                ),
                thumbs: None,
            },
            Group {
                header: Some("Icons and pointer"),
                rows: vec![
                    Row::new(
                        Id::IconTheme,
                        "Icon theme",
                        choose_and_reset(look.icons.is_some()),
                    )
                    .detail(&named(&look.icons, "The system choice")),
                    Row::new(
                        Id::CursorTheme,
                        "Cursor theme",
                        choose_and_reset(look.cursor_theme.is_some()),
                    )
                    .detail(&named(&look.cursor_theme, "The system choice")),
                    Row::new(
                        Id::CursorSize,
                        "Cursor size",
                        Control::Segments {
                            options: vec!["24", "32", "48", "64"],
                            chosen: CURSOR_SIZES
                                .iter()
                                .position(|size| *size == look.cursor_size)
                                .unwrap_or(0),
                        },
                    ),
                ],
                footer: Some("The pointer changes at once. Apps opened after a change use it too. Window edges show resize arrows from the cursor theme.".into()),
                thumbs: None,
            },
        ]
    }

    pub(super) fn notify_groups(&self) -> Vec<Group> {
        let notify = &self.notify;
        let mut groups = vec![Group {
            header: None,
            rows: vec![
                Row::new(Id::Quiet, "Do not disturb", Control::Switch(notify.quiet))
                    .detail("Only urgent notes show. The rest go to the history"),
                Row::new(
                    Id::NoteSeconds,
                    "Show each note for",
                    Control::Slider {
                        least: 0.0,
                        most: NOTE_MOST,
                        value: f64::from(notify.seconds),
                        unit: " s",
                        zero: Some("Until clicked"),
                    },
                )
                .detail("Apps can ask for their own time"),
                Row::new(
                    Id::History,
                    "Keep a history",
                    Control::Switch(notify.history),
                ),
            ],
            footer: None,
            thumbs: None,
        }];
        if notify.history {
            let mut rows: Vec<Row> = self
                .recent
                .iter()
                .take(SHOWN_NOTES)
                .enumerate()
                .map(|(at, note)| {
                    let label = match note.summary.is_empty() {
                        true => &note.app,
                        false => &note.summary,
                    };
                    let mut detail = crate::notify::ago(note.when, self.now);
                    if !note.app.is_empty() {
                        detail = format!("{}, {detail}", note.app);
                    }
                    if !note.body.is_empty() {
                        detail = format!("{detail}: {}", note.body);
                    }
                    Row::new(Id::Recent(at), label, Control::Buttons(Vec::new())).detail(&detail)
                })
                .collect();
            let empty = rows.is_empty();
            if !empty {
                rows.push(
                    Row::new(
                        Id::Forget,
                        "Clear the history",
                        Control::Buttons(vec![Mark::Text("Clear")]),
                    )
                    .faint(),
                );
            }
            groups.push(Group {
                header: Some("Recent"),
                rows,
                footer: empty.then(|| "No notes yet.".to_string()),
                thumbs: None,
            });
        }
        groups
    }

    pub(super) fn sound_groups(&self) -> Vec<Group> {
        let sound = &self.sound;
        if !sound.ready {
            return vec![Group {
                header: None,
                rows: Vec::new(),
                footer: Some("Sound needs PipeWire and its wpctl tool.".into()),
                thumbs: None,
            }];
        }
        let name = |device: Option<&crate::system::Device>| {
            device
                .map(|device| device.name.clone())
                .unwrap_or_else(|| "None found".to_string())
        };
        let level = |id, label: &str, value: f64, on: bool| {
            Row::new(
                id,
                label,
                Control::Slider {
                    least: 0.0,
                    most: 100.0,
                    value: (value * 100.0).round(),
                    unit: "%",
                    zero: None,
                },
            )
            .enabled(on)
        };
        let has_output = sound.output().is_some();
        let has_input = sound.input().is_some();
        vec![
            Group {
                header: Some("Output"),
                rows: vec![
                    Row::new(
                        Id::Output,
                        "Play sound on",
                        Control::Buttons(vec![Mark::Text("Choose")]),
                    )
                    .detail(&name(sound.output())),
                    level(Id::Volume, "Volume", sound.volume, has_output),
                    Row::new(Id::Mute, "Mute", Control::Switch(sound.muted)).enabled(has_output),
                ],
                footer: None,
                thumbs: None,
            },
            Group {
                header: Some("Input"),
                rows: vec![
                    Row::new(
                        Id::Input,
                        "Listen with",
                        Control::Buttons(vec![Mark::Text("Choose")]),
                    )
                    .detail(&name(sound.input())),
                    level(Id::Mic, "Input level", sound.mic, has_input),
                    Row::new(Id::MicMute, "Mute", Control::Switch(sound.mic_muted))
                        .enabled(has_input),
                ],
                footer: Some("The volume keys work everywhere.".into()),
                thumbs: None,
            },
        ]
    }

    pub(super) fn shortcut_groups(&self) -> Vec<Group> {
        let rows = Action::ALL
            .iter()
            .enumerate()
            .map(|(at, action)| {
                let shown = match Combo::parse(self.keys.get(*action)) {
                    Ok(Some(combo)) => combo.shown(self.modifier),
                    Ok(None) => "None".to_string(),
                    Err(_) => format!("{} (not understood)", self.keys.get(*action)),
                };
                let changed = self.keys.get(*action) != action.default_combo();
                let marks = match changed {
                    true => vec![Mark::Text("Change"), Mark::Text("Reset")],
                    false => vec![Mark::Text("Change")],
                };
                Row::new(Id::Shortcut(at), action.label(), Control::Buttons(marks)).detail(&shown)
            })
            .collect();
        vec![
            Group {
                header: None,
                rows,
                footer: Some(
                    "Hold Shift with the window switch to go back. Media keys always work.".into(),
                ),
                thumbs: None,
            },
            self.shot_group(),
        ]
    }

    pub(super) fn place_group(&self) -> Group {
        let rows = self
            .places
            .iter()
            .enumerate()
            .map(|(at, place)| {
                let key = place
                    .key
                    .as_deref()
                    .and_then(|key| Combo::parse(key).ok().flatten())
                    .map(|combo| combo.shown(self.modifier))
                    .unwrap_or_else(|| "No key".to_string());
                let detail = format!("{key}, zoom {}%", (place.zoom * 100.0).round());
                let docked = self.dock.contains(&place_button(&place.name));
                let marks = match docked {
                    true => vec![Mark::Text("Rename"), Mark::Text("Undock"), Mark::Minus],
                    false => vec![Mark::Text("Rename"), Mark::Text("Dock"), Mark::Minus],
                };
                Row::new(Id::Place(at), &place.name, Control::Buttons(marks)).detail(&detail)
            })
            .collect();
        Group {
            header: Some("Places"),
            rows,
            footer: Some({
                let key = self.modifier.name();
                match self.modifier {
                    crate::config::Modifier::Ctrl => format!("{key} + Shift + 1 to 9 saves this view. {key} + 1 to 9 flies back."),
                    _ => format!("{key} + Ctrl + 1 to 9 saves this view. {key} + 1 to 9 flies back."),
                }
            }),
            thumbs: None,
        }
    }

    pub(super) fn wifi_groups(&self) -> Vec<Group> {
        let Some(wifi) = &self.wifi else {
            return vec![Group {
                header: None,
                rows: Vec::new(),
                footer: Some(
                    "Settings could not find a Wi-Fi card through NetworkManager. \
                     If you use another Wi-Fi tool, keep using it. Your Wi-Fi works the same."
                        .into(),
                ),
                thumbs: None,
            }];
        };
        if !wifi.managed {
            return vec![Group {
                header: None,
                rows: Vec::new(),
                footer: Some(
                    "Another tool runs this Wi-Fi card, so Settings cannot show its networks. \
                     Your Wi-Fi still works. To manage it here, let NetworkManager run the card."
                        .into(),
                ),
                thumbs: None,
            }];
        }
        let mut groups = vec![Group {
            header: None,
            rows: vec![Row::new(Id::WifiOn, "Wi-Fi", Control::Switch(wifi.on))],
            footer: self.net_note.clone(),
            thumbs: None,
        }];
        if wifi.on {
            let rows: Vec<Row> = wifi
                .nets
                .iter()
                .enumerate()
                .map(|(at, net)| {
                    let detail = match net.active {
                        true => "Connected".to_string(),
                        false => {
                            let mut words = Vec::new();
                            if net.known {
                                words.push("Saved".to_string());
                            }
                            words.push(match net.secure {
                                true => "secured".to_string(),
                                false => "open".to_string(),
                            });
                            words.push(format!("{} signal", crate::net::bars(net.signal)));
                            let mut detail = words.join(", ");
                            if let Some(first) = detail.get_mut(0..1) {
                                first.make_ascii_uppercase();
                            }
                            detail
                        }
                    };
                    let marks = match (net.active, net.known) {
                        (true, _) => vec![Mark::Text("Leave")],
                        (false, true) => vec![Mark::Text("Join"), Mark::Text("Forget")],
                        (false, false) => vec![Mark::Text("Join")],
                    };
                    Row::new(Id::Net(at), &net.ssid, Control::Buttons(marks))
                        .detail(&detail)
                        .opens()
                })
                .collect();
            groups.push(Group {
                header: Some("Networks"),
                footer: Some(
                    match rows.is_empty() {
                        true => "No networks in range.",
                        false => "Click a network name to see its details.",
                    }
                    .to_string(),
                ),
                rows,
                thumbs: None,
            });
        }
        groups
    }

    pub(super) fn blue_groups(&self) -> Vec<Group> {
        let Some(blue) = &self.blue else {
            return vec![Group {
                header: None,
                rows: Vec::new(),
                footer: Some("No Bluetooth adapter found.".into()),
                thumbs: None,
            }];
        };
        let mut groups = vec![Group {
            header: None,
            rows: vec![Row::new(Id::BlueOn, "Bluetooth", Control::Switch(blue.on))],
            footer: self.net_note.clone(),
            thumbs: None,
        }];
        if !blue.on {
            return groups;
        }
        let row = |at: usize, gadget: &crate::net::Gadget| {
            let (marks, detail) = match (gadget.paired, gadget.connected) {
                (true, true) => (vec![Mark::Text("Disconnect")], "Connected"),
                (true, false) => (vec![Mark::Text("Connect"), Mark::Text("Remove")], "Not connected"),
                (false, _) => (vec![Mark::Text("Pair")], "Not paired"),
            };
            Row::new(Id::Gadget(at), &gadget.name, Control::Buttons(marks)).detail(detail)
        };
        let mine: Vec<Row> = blue
            .gadgets
            .iter()
            .enumerate()
            .filter(|(_, gadget)| gadget.paired)
            .map(|(at, gadget)| row(at, gadget))
            .collect();
        if !mine.is_empty() {
            groups.push(Group {
                header: Some("My devices"),
                rows: mine,
                footer: None,
                thumbs: None,
            });
        }
        let search = match blue.searching {
            true => Row::new(Id::BlueSearch, "Searching", Control::Buttons(Vec::new()))
                .detail("Put the device in pairing mode"),
            false => Row::new(
                Id::BlueSearch,
                "Find devices",
                Control::Buttons(vec![Mark::Text("Search")]),
            )
            .detail("Looks for ten seconds"),
        };
        let mut nearby = vec![search];
        nearby.extend(
            blue.gadgets
                .iter()
                .enumerate()
                .filter(|(_, gadget)| !gadget.paired)
                .map(|(at, gadget)| row(at, gadget)),
        );
        groups.push(Group {
            header: Some("Nearby"),
            rows: nearby,
            footer: None,
            thumbs: None,
        });
        groups
    }

    pub(super) fn access_groups(&self) -> Vec<Group> {
        let access = &self.access;
        vec![
            Group {
                header: Some("Seeing"),
                rows: vec![
                    Row::new(Id::LargeText, "Large text", Control::Switch(access.large_text))
                        .detail("Text in apps is a quarter bigger"),
                    Row::new(Id::HighContrast, "High contrast", Control::Switch(access.high_contrast))
                        .detail("Apps use stronger edges and colors"),
                ],
                footer: None,
                thumbs: None,
            },
            Group {
                header: Some("Moving"),
                rows: vec![
                    Row::new(Id::ReduceMotion, "Reduce motion", Control::Switch(access.reduce_motion))
                        .detail("The camera, bars and windows jump instead of sliding"),
                ],
                footer: None,
                thumbs: None,
            },
            Group {
                header: Some("Typing"),
                rows: vec![
                    Row::new(Id::StickyKeys, "Sticky keys", Control::Switch(access.sticky_keys))
                        .detail("Tap Shift, Ctrl, Alt or Super, then a key"),
                ],
                footer: Some(
                    "With sticky keys, tap a key like Ctrl twice to keep it held. Tap it again to let go."
                        .into(),
                ),
                thumbs: None,
            },
        ]
    }

    fn shot_group(&self) -> Group {
        let folder = match self.shots.folder.as_deref() {
            Some(folder) if !folder.trim().is_empty() => folder.to_string(),
            _ => "Pictures/Screenshots in your home folder".to_string(),
        };
        let marks = match self.shots.folder.is_some() {
            true => vec![Mark::Text("Choose"), Mark::Text("Reset")],
            false => vec![Mark::Text("Choose")],
        };
        Group {
            header: Some("Screenshots"),
            rows: vec![
                Row::new(Id::ShotFolder, "Save to", Control::Buttons(marks)).detail(&folder),
                Row::new(
                    Id::ShotFormat,
                    "Format",
                    Control::Segments {
                        options: vec!["PNG", "JPEG"],
                        chosen: match self.shots.format {
                            ShotFormat::Png => 0,
                            ShotFormat::Jpeg => 1,
                        },
                    },
                )
                .detail("PNG keeps every pixel. JPEG files are smaller"),
            ],
            footer: None,
            thumbs: None,
        }
    }

    fn power_extras(&self) -> Vec<Group> {
        let mut groups = Vec::new();
        if let Some(profiles) = &self.profile {
            let offered: Vec<(&str, &'static str)> = PROFILES
                .iter()
                .filter(|(name, _)| profiles.offered.iter().any(|each| each == name))
                .copied()
                .collect();
            if offered.len() > 1 {
                groups.push(Group {
                    header: Some("Performance"),
                    rows: vec![Row::new(
                        Id::Profile,
                        "Power mode",
                        Control::Segments {
                            options: offered.iter().map(|(_, label)| *label).collect(),
                            chosen: offered
                                .iter()
                                .position(|(name, _)| *name == profiles.now)
                                .unwrap_or(0),
                        },
                    )
                    .detail("Saver runs cooler and longer on battery")],
                    footer: None,
                    thumbs: None,
                });
            }
        }
        if !self.cells.is_empty() {
            groups.push(Group {
                header: Some("Battery"),
                rows: self
                    .cells
                    .iter()
                    .map(|(key, value)| Row::new(Id::Battery, key, Control::Note(value.clone())))
                    .collect(),
                footer: None,
                thumbs: None,
            });
        }
        groups
    }

    pub(super) fn power_groups(&self) -> Vec<Group> {
        let mut groups = self.power_groups_base();
        groups.extend(self.power_extras());
        groups
    }

    fn power_groups_base(&self) -> Vec<Group> {
        let power = &self.power;
        vec![Group {
            header: None,
            rows: vec![
                Row::new(
                    Id::SleepAfter,
                    "Sleep after",
                    Control::Slider {
                        least: 0.0,
                        most: SLEEP_MOST,
                        value: power.sleep_after as f64,
                        unit: " min",
                        zero: Some("Never"),
                    },
                )
                .detail("Minutes with no key press or pointer move"),
                Row::new(
                    Id::Lid,
                    "When the lid closes",
                    Control::Segments {
                        options: vec!["Sleep", "Lock", "Nothing"],
                        chosen: match power.lid {
                            Lid::Suspend => 0,
                            Lid::Lock => 1,
                            Lid::Nothing => 2,
                        },
                    },
                )
                .detail("With another monitor plugged in, it only locks"),
                Row::new(
                    Id::PowerButton,
                    "Power button",
                    Control::Segments {
                        options: vec!["Menu", "Sleep", "Off", "Nothing"],
                        chosen: match power.button {
                            PowerKey::Menu => 0,
                            PowerKey::Suspend => 1,
                            PowerKey::Off => 2,
                            PowerKey::Nothing => 3,
                        },
                    },
                ),
            ],
            footer: Some("A video that is playing keeps the computer awake.".into()),
            thumbs: None,
        }]
    }

    pub(super) fn time_groups(&self) -> Vec<Group> {
        let zone = match self.zone.as_str() {
            "" => "Not known".to_string(),
            zone => zone.replace('_', " "),
        };
        vec![
            Group {
                header: None,
                rows: vec![
                    Row::new(
                        Id::Zone,
                        "Time zone",
                        Control::Buttons(vec![Mark::Text("Choose")]),
                    )
                    .detail(&zone),
                    Row::new(
                        Id::AutoTime,
                        "Set the time automatically",
                        Control::Switch(self.ntp.unwrap_or(false)),
                    )
                    .detail("From time servers on the internet")
                    .enabled(self.ntp.is_some()),
                ],
                footer: Some(match &self.time_error {
                    Some(error) => format!("Not changed: {error}"),
                    None => "The system may ask for your password.".into(),
                }),
                thumbs: None,
            },
            Group {
                header: Some("Clock"),
                rows: self.clock_rows(),
                footer: Some("The clock sits in the top bar.".into()),
                thumbs: None,
            },
        ]
    }

    pub(super) fn about_groups(&self) -> Vec<Group> {
        let reset = match self.confirm_reset {
            true => "Click to confirm",
            false => "Reset",
        };
        vec![
            Group {
                header: None,
                rows: self
                    .about
                    .iter()
                    .enumerate()
                    .map(|(at, (key, value))| {
                        Row::new(Id::Info(at), key, Control::Note(value.clone()))
                    })
                    .collect(),
                footer: None,
                thumbs: None,
            },
            Group {
                header: Some("Settings file"),
                rows: vec![
                    Row::new(
                        Id::Export,
                        "Export settings",
                        Control::Buttons(vec![Mark::Text("Export")]),
                    )
                    .detail("Save a copy of your settings to a folder"),
                    Row::new(
                        Id::Import,
                        "Import settings",
                        Control::Buttons(vec![Mark::Text("Import")]),
                    )
                    .detail("Use a settings file you saved before"),
                    Row::new(
                        Id::ResetAll,
                        "Reset all settings",
                        Control::Buttons(vec![Mark::Text(reset)]),
                    )
                    .detail("Go back to the defaults. The old file is kept as a backup"),
                ],
                footer: Some(self.file_note.clone().unwrap_or_else(|| {
                    "Import and reset keep the old file as drifter.toml.bak.".into()
                })),
                thumbs: None,
            },
        ]
    }

    pub(super) fn set_more(&mut self, id: Id, value: Value) {
        match (id, value) {
            (Id::WifiOn, Value::Switch(on)) => {
                if let Some(wifi) = self.wifi.as_mut() {
                    wifi.on = on;
                }
            }
            (Id::BlueOn, Value::Switch(on)) => {
                if let Some(blue) = self.blue.as_mut() {
                    blue.on = on;
                }
            }
            (Id::LargeText, Value::Switch(on)) => self.access.large_text = on,
            (Id::HighContrast, Value::Switch(on)) => self.access.high_contrast = on,
            (Id::ReduceMotion, Value::Switch(on)) => self.access.reduce_motion = on,
            (Id::StickyKeys, Value::Switch(on)) => self.access.sticky_keys = on,
            (Id::ShotFormat, Value::Choice(n)) => {
                self.shots.format = match n {
                    0 => ShotFormat::Png,
                    _ => ShotFormat::Jpeg,
                }
            }
            (Id::Profile, Value::Choice(n)) => {
                if let Some(profiles) = self.profile.as_mut() {
                    let offered: Vec<&str> = PROFILES
                        .iter()
                        .map(|(name, _)| *name)
                        .filter(|name| profiles.offered.iter().any(|each| each == name))
                        .collect();
                    if let Some(name) = offered.get(n) {
                        profiles.now = name.to_string();
                    }
                }
            }
            (Id::Caps, Value::Choice(n)) => {
                self.keyboard.options = keyboard::set_caps(&self.keyboard.options, n)
            }
            (Id::Compose, Value::Choice(n)) => {
                self.keyboard.options = keyboard::set_compose(&self.keyboard.options, n)
            }
            (Id::MouseSpeed, Value::Number(n)) => self.mouse.speed = number(n, (0, FASTEST)),
            (Id::MouseAccel, Value::Switch(on)) => self.mouse.accel = on,
            (Id::MouseNatural, Value::Switch(on)) => self.mouse.natural = on,
            (Id::LeftHanded, Value::Choice(n)) => self.mouse.left_handed = n == 1,
            (Id::HotCorner, Value::Switch(on)) => self.motion.hot_corner = on,
            (Id::CornerSize, Value::Number(n)) => self.motion.corner_size = number(n, CORNER_SIZE),
            (Id::CornerWait, Value::Number(n)) => {
                self.motion.corner_wait = number(n / 10.0, (0, 200)) * 10
            }
            (Id::PanSpeed, Value::Number(n)) => {
                self.motion.pan_speed = number(n / 5.0, (5, 80)) * 5
            }
            (Id::Flight, Value::Number(n)) => self.motion.flight = number(n / 10.0, (4, 150)) * 10,
            (Id::HideTop, Value::Switch(on)) => self.motion.hide_top = on,
            (Id::HideBottom, Value::Switch(on)) => self.motion.hide_bottom = on,
            (Id::BarOpen, Value::Number(n)) => {
                self.motion.bar_open = number(n / 10.0, (0, 200)) * 10
            }
            (Id::BarClose, Value::Number(n)) => {
                self.motion.bar_close = number(n / 10.0, (0, 200)) * 10
            }
            (Id::BarSlide, Value::Number(n)) => {
                self.motion.bar_slide = number(n / 10.0, (4, 150)) * 10
            }
            (Id::Style, Value::Choice(n)) => {
                self.appearance.style = match n {
                    0 => Some(Style::Light),
                    1 => Some(Style::Dark),
                    _ => Some(Style::Auto),
                }
            }
            (Id::AccentColor, Value::Choice(n)) => {
                self.appearance.accent = Accent::ALL.get(n).copied()
            }
            (Id::CursorSize, Value::Choice(n)) => {
                self.appearance.cursor_size = CURSOR_SIZES[n.min(CURSOR_SIZES.len() - 1)]
            }
            (Id::SleepAfter, Value::Number(n)) => {
                self.power.sleep_after = n.round().clamp(0.0, SLEEP_MOST) as u64
            }
            (Id::Lid, Value::Choice(n)) => {
                self.power.lid = match n {
                    0 => Lid::Suspend,
                    1 => Lid::Lock,
                    _ => Lid::Nothing,
                }
            }
            (Id::PowerButton, Value::Choice(n)) => {
                self.power.button = match n {
                    0 => PowerKey::Menu,
                    1 => PowerKey::Suspend,
                    2 => PowerKey::Off,
                    _ => PowerKey::Nothing,
                }
            }
            (Id::Quiet, Value::Switch(on)) => self.notify.quiet = on,
            (Id::NoteSeconds, Value::Number(n)) => {
                self.notify.seconds =
                    n.round().clamp(0.0, NOTE_MOST.min(f64::from(LONGEST_NOTE))) as u32
            }
            (Id::History, Value::Switch(on)) => self.notify.history = on,
            (Id::Volume, Value::Number(n)) => self.sound.volume = (n / 100.0).clamp(0.0, 1.0),
            (Id::Mute, Value::Switch(on)) => self.sound.muted = on,
            (Id::Mic, Value::Number(n)) => self.sound.mic = (n / 100.0).clamp(0.0, 1.0),
            (Id::MicMute, Value::Switch(on)) => self.sound.mic_muted = on,
            (Id::AutoTime, Value::Switch(on)) if self.ntp.is_some() => self.ntp = Some(on),
            _ => {}
        }
    }

    pub(super) fn press_more(&mut self, id: Id, index: usize) -> Option<Ask> {
        match (id, index) {
            (Id::Net(at), index) => {
                let net = self.wifi.as_ref()?.nets.get(at)?;
                return Some(Ask::Net(match (net.active, index) {
                    (true, _) => NetAsk::Leave(at),
                    (false, 0) => NetAsk::Join(at),
                    (false, _) => NetAsk::Forget(at),
                }));
            }
            (Id::BlueSearch, _) => {
                let blue = self.blue.as_ref()?;
                return (!blue.searching).then_some(Ask::Net(NetAsk::Search));
            }
            (Id::Gadget(at), index) => {
                let gadget = self.blue.as_ref()?.gadgets.get(at)?;
                return Some(Ask::Net(match (gadget.paired, gadget.connected, index) {
                    (false, _, _) => NetAsk::Pair(at),
                    (true, true, _) => NetAsk::Disconnect(at),
                    (true, false, 0) => NetAsk::Connect(at),
                    (true, false, _) => NetAsk::Remove(at),
                }));
            }
            (Id::Place(at), 0) if at < self.places.len() => return Some(Ask::RenamePlace(at)),
            (Id::Place(at), 1) if at < self.places.len() => {
                let button = place_button(&self.places[at].name);
                match self.dock.iter().position(|each| *each == button) {
                    Some(found) => {
                        self.dock.remove(found);
                    }
                    None => self.dock.push(button),
                }
            }
            (Id::Place(at), _) if at < self.places.len() => {
                let gone = self.places.remove(at);
                let button = place_button(&gone.name);
                self.dock.retain(|each| *each != button);
            }
            (Id::ShotFolder, 0) => return Some(Ask::Files(For::ShotFolder)),
            (Id::ShotFolder, _) => self.shots.folder = None,
            (Id::Variant, _) => return Some(Ask::Choose(For::Variant)),
            (Id::IconTheme, 0) => return Some(Ask::Choose(For::IconTheme)),
            (Id::IconTheme, _) => self.appearance.icons = None,
            (Id::CursorTheme, 0) => return Some(Ask::Choose(For::CursorTheme)),
            (Id::CursorTheme, _) => self.appearance.cursor_theme = None,
            (Id::Asset(at), 0) if at < 4 => return Some(Ask::Files(ASSET_FOR[at])),
            (Id::Asset(at), _) if at < 4 => self.assets[at] = None,
            (Id::Forget, _) => return Some(Ask::Forget),
            (Id::Shortcut(at), 0) if at < Action::ALL.len() => return Some(Ask::Capture(at)),
            (Id::Shortcut(at), _) if at < Action::ALL.len() => {
                let action = Action::ALL[at];
                *self.keys.get_mut(action) = action.default_combo().to_string();
            }
            (Id::Output, _) => return Some(Ask::Choose(For::Output)),
            (Id::Input, _) => return Some(Ask::Choose(For::Input)),
            (Id::Zone, _) => return Some(Ask::Choose(For::Zone)),
            (Id::Export, _) => return Some(Ask::Files(For::Export)),
            (Id::Import, _) => return Some(Ask::Files(For::Import)),
            (Id::ResetAll, _) if self.confirm_reset => {
                self.confirm_reset = false;
                return Some(Ask::Reset);
            }
            (Id::ResetAll, _) => self.confirm_reset = true,
            _ => {}
        }
        None
    }

    pub fn choose(&mut self, purpose: For, code: String) {
        match purpose {
            For::Layout => self.choose_layout(code),
            For::Variant => self.keyboard.variant = code,
            For::IconTheme => self.appearance.icons = Some(code),
            For::CursorTheme => self.appearance.cursor_theme = Some(code),
            For::Zone => self.zone = code,
            For::Output | For::Input => {
                let Ok(id) = code.parse::<u32>() else {
                    return;
                };
                let list = match purpose {
                    For::Output => &mut self.sound.outputs,
                    _ => &mut self.sound.inputs,
                };
                for device in list {
                    device.default = device.id == id;
                }
            }
            _ => {}
        }
    }

    pub fn choose_shot_folder(&mut self, folder: PathBuf) {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let shown = match home.as_ref().and_then(|home| folder.strip_prefix(home).ok()) {
            Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
            Some(rest) => format!("~/{}", rest.display()),
            None => folder.display().to_string(),
        };
        self.shots.folder = Some(shown);
    }

    pub fn choose_file(&mut self, purpose: For, path: PathBuf) {
        if purpose == For::Picture {
            self.wallpaper = Some(path);
            return;
        }
        if let Some(at) = ASSET_FOR.iter().position(|each| *each == purpose) {
            self.assets[at] = Some(path);
        }
    }

    pub fn shortcut_name(&self, at: usize) -> (String, String) {
        let action = Action::ALL[at.min(Action::ALL.len() - 1)];
        let now = Combo::parse(self.keys.get(action))
            .ok()
            .flatten()
            .map(|combo| combo.shown(self.modifier))
            .unwrap_or_default();
        (action.label().to_string(), now)
    }

    pub fn set_shortcut(&mut self, at: usize, combo: Option<Combo>) -> Result<(), String> {
        let Some(action) = Action::ALL.get(at).copied() else {
            return Ok(());
        };
        let Some(combo) = combo else {
            self.keys.get_mut(action).clear();
            return Ok(());
        };
        let plain = combo.mods == crate::shortcut::Mods::default() && !combo.camera;
        if plain && !crate::shortcut::may_stand_alone(&combo.key) {
            return Err("Add Ctrl, Alt or Super, so typing still works.".into());
        }
        let wanted = combo.resolve(self.modifier);
        for other in Action::ALL {
            if other == action {
                continue;
            }
            let taken = Combo::parse(self.keys.get(other))
                .ok()
                .flatten()
                .is_some_and(|held| {
                    let held = held.resolve(self.modifier);
                    held.mods == wanted.mods && held.key.eq_ignore_ascii_case(&wanted.key)
                });
            if taken {
                self.keys.get_mut(other).clear();
            }
        }
        *self.keys.get_mut(action) = combo.word();
        Ok(())
    }

    pub(super) fn write_more(&self, before: &Model, doc: &mut toml_edit::DocumentMut) {
        let int = |n: u64| toml_edit::Value::from(n as i64);
        let flag = toml_edit::Value::from;
        let word = |w: &str| toml_edit::Value::from(w);

        let (mouse, was) = (&self.mouse, &before.mouse);
        if mouse.speed != was.speed {
            put(doc, "mouse", "speed", int(u64::from(mouse.speed)));
        }
        for (key, now, then) in [
            ("accel", mouse.accel, was.accel),
            ("natural", mouse.natural, was.natural),
            ("left_handed", mouse.left_handed, was.left_handed),
        ] {
            if now != then {
                put(doc, "mouse", key, flag(now));
            }
        }

        let (motion, was) = (&self.motion, &before.motion);
        for (key, now, then) in [
            ("hot_corner", motion.hot_corner, was.hot_corner),
            ("hide_top", motion.hide_top, was.hide_top),
            ("hide_bottom", motion.hide_bottom, was.hide_bottom),
        ] {
            if now != then {
                put(doc, "motion", key, flag(now));
            }
        }
        for (key, now, then) in [
            ("corner_size", motion.corner_size, was.corner_size),
            ("corner_wait", motion.corner_wait, was.corner_wait),
            ("pan_speed", motion.pan_speed, was.pan_speed),
            ("bar_open", motion.bar_open, was.bar_open),
            ("bar_close", motion.bar_close, was.bar_close),
            ("bar_slide", motion.bar_slide, was.bar_slide),
            ("flight", motion.flight, was.flight),
        ] {
            if now != then {
                put(doc, "motion", key, int(u64::from(now)));
            }
        }

        let (look, was) = (&self.appearance, &before.appearance);
        let optional =
            |doc: &mut toml_edit::DocumentMut, key: &str, value: Option<&str>| match value {
                Some(value) => put(doc, "appearance", key, word(value)),
                None => drop_key(doc, "appearance", key),
            };
        if look.style != was.style {
            let style = look.style.map(|style| match style {
                Style::Light => "light",
                Style::Dark => "dark",
                Style::Auto => "auto",
            });
            optional(doc, "style", style);
        }
        if look.accent != was.accent {
            optional(doc, "accent", look.accent.map(Accent::word));
        }
        if look.icons != was.icons {
            optional(doc, "icons", look.icons.as_deref());
        }
        if look.cursor_theme != was.cursor_theme {
            optional(doc, "cursor_theme", look.cursor_theme.as_deref());
        }
        if look.cursor_size != was.cursor_size {
            put(
                doc,
                "appearance",
                "cursor_size",
                int(u64::from(look.cursor_size)),
            );
        }

        for (at, (key, _, _)) in ASSETS.iter().enumerate() {
            if self.assets[at] == before.assets[at] {
                continue;
            }
            match &self.assets[at] {
                Some(path) => put(doc, "assets", key, word(&path.to_string_lossy())),
                None => drop_key(doc, "assets", key),
            }
        }

        let (power, was) = (&self.power, &before.power);
        if power.sleep_after != was.sleep_after {
            put(doc, "power", "sleep_after", int(power.sleep_after));
        }
        if power.lid != was.lid {
            put(doc, "power", "lid", word(lid_word(power.lid)));
        }
        if power.button != was.button {
            put(doc, "power", "button", word(button_word(power.button)));
        }

        let (access, was) = (&self.access, &before.access);
        for (key, now, then) in [
            ("large_text", access.large_text, was.large_text),
            ("high_contrast", access.high_contrast, was.high_contrast),
            ("reduce_motion", access.reduce_motion, was.reduce_motion),
            ("sticky_keys", access.sticky_keys, was.sticky_keys),
        ] {
            if now != then {
                put(doc, "access", key, flag(now));
            }
        }

        let (shots, was) = (&self.shots, &before.shots);
        if shots.folder != was.folder {
            match &shots.folder {
                Some(folder) => put(doc, "screenshots", "folder", word(folder)),
                None => drop_key(doc, "screenshots", "folder"),
            }
        }
        if shots.format != was.format {
            let format = match shots.format {
                ShotFormat::Png => "png",
                ShotFormat::Jpeg => "jpeg",
            };
            put(doc, "screenshots", "format", word(format));
        }

        let (notify, was) = (&self.notify, &before.notify);
        if notify.quiet != was.quiet {
            put(doc, "notify", "quiet", flag(notify.quiet));
        }
        if notify.seconds != was.seconds {
            put(doc, "notify", "seconds", int(u64::from(notify.seconds)));
        }
        if notify.history != was.history {
            put(doc, "notify", "history", flag(notify.history));
        }

        for action in Action::ALL {
            let now = self.keys.get(action);
            if now != before.keys.get(action) {
                put(doc, "keys", action.key(), word(now));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::geom::Vec2;
    use crate::settings::{Hit, Look, Section};

    fn model(text: &str) -> Model {
        Model::read(&Config::parse(text).unwrap(), &[], |_| None)
    }

    fn round_trip(text: &str, change: impl Fn(&mut Model)) -> Config {
        let before = model(text);
        let mut after = before.clone();
        change(&mut after);
        let out = after.write(&before, text).unwrap();
        let back = Config::parse(&out).unwrap();
        back.validate().unwrap();
        back
    }

    #[test]
    fn the_bar_auto_hide_switches_write_the_motion_table() {
        let back = round_trip("", |m| {
            m.set(Id::HideBottom, Value::Switch(false));
        });
        assert!(back.motion.hide_top);
        assert!(!back.motion.hide_bottom);
        let tunables = crate::config::Tunables::from(&back.motion);
        assert_eq!(tunables.bar_hides, [true, false]);
    }

    #[test]
    fn mouse_and_motion_write_their_own_tables() {
        let back = round_trip("", |m| {
            m.set(Id::MouseSpeed, Value::Number(8.0));
            m.set(Id::LeftHanded, Value::Choice(1));
            m.set(Id::HotCorner, Value::Switch(false));
            m.set(Id::CornerWait, Value::Number(433.0));
            m.set(Id::PanSpeed, Value::Number(151.0));
        });
        assert_eq!(back.mouse.speed, 8);
        assert!(back.mouse.left_handed && !back.mouse.accel);
        assert!(!back.motion.hot_corner);
        assert_eq!(back.motion.corner_wait, 430);
        assert_eq!(back.motion.pan_speed, 150);
        assert_eq!(back.motion.bar_slide, Motion::default().bar_slide);
    }

    use crate::config::Motion;

    #[test]
    fn appearance_choices_are_written_and_reset_removes_them() {
        let text = "[appearance]\nicons = \"Papirus\"\n[assets]\nfont = \"/f.ttf\"\n";
        let back = round_trip(text, |m| {
            m.set(Id::Style, Value::Choice(2));
            m.set(Id::AccentColor, Value::Choice(5));
            m.set(Id::CursorSize, Value::Choice(2));
            m.press(Id::IconTheme, 1);
            m.press(Id::Asset(1), 1);
            m.choose(For::CursorTheme, "Bibata".into());
            m.choose_file(For::Pointer, "/p.png".into());
        });
        assert_eq!(back.appearance.style, Some(Style::Auto));
        assert_eq!(back.appearance.accent, Some(Accent::Red));
        assert_eq!(back.appearance.cursor_size, 48);
        assert_eq!(back.appearance.icons, None);
        assert_eq!(back.appearance.cursor_theme.as_deref(), Some("Bibata"));
        assert_eq!(back.assets.font, None);
        assert_eq!(back.assets.cursor, Some("/p.png".into()));
    }

    #[test]
    fn power_notify_and_keyboard_options_round_trip() {
        let back = round_trip("[keyboard]\noptions = \"grp:alt_shift_toggle\"\n", |m| {
            m.set(Id::SleepAfter, Value::Number(30.0));
            m.set(Id::Lid, Value::Choice(1));
            m.set(Id::PowerButton, Value::Choice(2));
            m.set(Id::Quiet, Value::Switch(true));
            m.set(Id::NoteSeconds, Value::Number(0.0));
            m.set(Id::Caps, Value::Choice(1));
        });
        assert_eq!(back.power.sleep_after, 30);
        assert_eq!(back.power.lid, Lid::Lock);
        assert_eq!(back.power.button, PowerKey::Off);
        assert!(back.notify.quiet && back.notify.history);
        assert_eq!(back.notify.seconds, 0);
        assert_eq!(back.keyboard.options, "grp:alt_shift_toggle,caps:escape");
    }

    #[test]
    fn a_new_shortcut_takes_its_keys_from_any_other() {
        let mut m = model("[input]\nmodifier = \"ctrl\"\n");
        let lock = Action::ALL.iter().position(|a| *a == Action::Lock).unwrap();
        let term = Action::ALL
            .iter()
            .position(|a| *a == Action::Terminal)
            .unwrap();
        let ctrl_l = Combo::parse("ctrl+L").unwrap();
        m.set_shortcut(term, ctrl_l).unwrap();
        assert_eq!(m.keys.terminal, "ctrl+L");
        assert_eq!(m.keys.lock, "", "camera+l is ctrl+l here, so it moved");
        assert!(m.set_shortcut(term, Combo::parse("a").unwrap()).is_err());
        assert!(m.set_shortcut(term, Combo::parse("F9").unwrap()).is_ok());
        assert_eq!(m.press(Id::Shortcut(lock), 1), None);
        assert_eq!(m.keys.lock, "camera+l");
        assert_eq!(m.press(Id::Shortcut(lock), 0), Some(Ask::Capture(lock)));
    }

    #[test]
    fn a_swatch_click_picks_its_color() {
        let look = Look::default();
        let size = Vec2::new(860.0, 620.0);
        let m = model("");
        let groups = m.groups(Section::Appearance);
        let row = look.place(size, Section::Appearance, &groups, 0.0)[0].rows[1];
        let third = look.swatch_rects(row, Accent::ALL.len())[2].center();
        assert_eq!(
            look.hit(size, Section::Appearance, &m, 0.0, third),
            Some(Hit::Change(Id::AccentColor, Value::Choice(2)))
        );
    }

    #[test]
    fn live_pages_say_what_they_need() {
        let m = model("");
        let sound = m.groups(Section::Sound);
        assert!(sound[0].rows.is_empty() && sound[0].footer.is_some());
        let time = m.groups(Section::Time);
        assert!(!time[0].rows[1].enabled, "no timedatectl answer yet");
        for section in Section::ALL {
            assert!(!m.groups(section).is_empty(), "{section:?}");
        }
    }
}
