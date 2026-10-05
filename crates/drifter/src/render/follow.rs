use super::*;

impl State {
    pub(super) fn open_settings(&mut self) {
        self.open_settings_at("");
    }

    pub(super) fn open_settings_at(&mut self, page: &str) {
        if self.lock.is_some() {
            return;
        }
        if let Some(place) = self
            .windows
            .iter()
            .position(|window| window.app == SETTINGS_APP && !window.going)
        {
            self.bring(place);
            return;
        }
        let program = std::env::current_exe()
            .map(|path| path.with_file_name(SETTINGS_APP))
            .unwrap_or_else(|_| std::path::PathBuf::from(SETTINGS_APP));
        let mut argv = vec![program.to_string_lossy().into_owned()];
        if !page.is_empty() {
            argv.push(page.to_string());
        }
        crate::apps::launch(&argv, &self.socket);
    }

    pub(super) fn set_keys(&mut self, keys: &drifter_core::Keyboard) {
        let keyboard = self.wayland.keyboard.clone();
        if keys.layout != self.keys.layout
            || keys.variant != self.keys.variant
            || keys.options != self.keys.options
        {
            if let Err(error) = keyboard.set_xkb_config(self, crate::shell::xkb(keys)) {
                eprintln!(
                    "drifter: keyboard layout {:?} did not load: {error:?}",
                    keys.layout
                );
            }
        }
        keyboard.change_repeat_info(keys.repeat_rate as i32, keys.repeat_delay as i32);
        self.keys = keys.clone();
    }

    pub(super) fn set_shortcuts(
        &mut self,
        keys: &drifter_core::Keys,
        camera: drifter_core::Modifier,
    ) {
        use drifter_core::shortcut::Action;
        self.shortcuts = keys
            .combos()
            .into_iter()
            .map(|(action, combo)| (action, combo.resolve(camera)))
            .collect();
        self.switch_mods = self
            .shortcuts
            .iter()
            .find(|(action, _)| *action == Action::Switch)
            .map(|(_, combo)| combo.mods)
            .unwrap_or_default();
    }

    pub(super) fn set_places(
        &mut self,
        places: &[drifter_core::place::Place],
        camera: drifter_core::Modifier,
    ) {
        use drifter_core::place;
        use drifter_core::shortcut::Combo;
        let resolve = |text: &str| Combo::parse(text).ok().flatten().map(|combo| combo.resolve(camera));
        self.places = places
            .iter()
            .map(|each| (each.clone(), each.key.as_deref().and_then(resolve)))
            .collect();
        self.place_saves = (1..=place::SLOTS)
            .filter_map(|slot| resolve(&place::save_key(slot)).map(|combo| (slot, combo)))
            .collect();
    }

    pub(super) fn go_to_place(&mut self, at: usize) {
        let Some((place, _)) = self.places.get(at) else {
            return;
        };
        let target = Camera::new(Vec2::new(place.x, place.y), place.zoom);
        self.fly_to(target);
        self.kick_now();
    }

    pub(super) fn go_to_named_place(&mut self, name: &str) -> bool {
        match self.places.iter().position(|(place, _)| place.name == name) {
            Some(at) => {
                self.go_to_place(at);
                true
            }
            None => false,
        }
    }

    pub(super) fn save_place(&mut self, slot: usize) {
        use drifter_core::place::{self, Place};
        let name = place::slot_name(slot);
        let key = self
            .places
            .iter()
            .find(|(each, _)| each.name == name)
            .map(|(each, _)| each.key.clone())
            .unwrap_or_else(|| Some(place::go_key(slot)));
        let saved = Place {
            name: name.clone(),
            x: self.camera.center.x,
            y: self.camera.center.y,
            zoom: self.camera.zoom,
            key,
        };
        let path = self.config_path.clone();
        let written = std::fs::read_to_string(&path)
            .or_else(|error| match error.kind() {
                std::io::ErrorKind::NotFound => Ok(String::new()),
                _ => Err(error),
            })
            .map_err(|error| format!("cannot read {}: {error}", path.display()))
            .and_then(|text| place::store(&text, &saved))
            .and_then(|text| {
                let fresh = path.with_extension("toml.new");
                std::fs::write(&fresh, text)
                    .and_then(|()| std::fs::rename(&fresh, &path))
                    .map_err(|error| format!("cannot write {}: {error}", path.display()))
            });
        let (summary, body) = match written {
            Ok(()) => {
                match self.places.iter_mut().find(|(each, _)| each.name == name) {
                    Some((each, _)) => *each = saved,
                    None => {
                        let combo = drifter_core::shortcut::Combo::parse(&place::go_key(slot))
                            .ok()
                            .flatten()
                            .map(|combo| combo.resolve(self.holds));
                        self.places.push((saved, combo));
                    }
                }
                (format!("{name} saved"), format!("The camera key and {slot} come back here."))
            }
            Err(error) => (format!("{name} not saved"), error),
        };
        if let Some(id) = self.notes_bus.as_ref().map(|bus| bus.next_id()) {
            self.show_note(drifter_core::notify::Note {
                id,
                app: "Drifter".to_string(),
                summary,
                body,
                actions: Vec::new(),
                expire: Some(drifter_core::notify::DEFAULT_MS),
                asked: -1,
                critical: false,
                icon: None,
            });
        }
    }

    pub(super) fn run_shortcut(&mut self, action: drifter_core::shortcut::Action, shift: bool) {
        use drifter_core::desktop::Role;
        use drifter_core::shortcut::Action;
        if self.lock.is_some() {
            return;
        }
        match action {
            Action::Switch => self.switch_step(shift),
            Action::Close => self.close_focused(),
            Action::Screenshot => {
                self.shot_wanted = true;
                self.kick_now();
            }
            Action::Lock => {
                self.lock_now();
            }
            Action::Power => {
                let middle = self.screen * 0.5;
                self.open_popover(drifter_core::power::items(), middle, Target::Desktop);
            }
            Action::Overview => {
                if self.flight.is_none() && !self.press_overview() {
                    self.fly_out();
                }
            }
            Action::Terminal => {
                crate::apps::open_role(Role::Terminal, &self.apps_chosen, &self.socket)
            }
            Action::Files => crate::apps::open_role(Role::Files, &self.apps_chosen, &self.socket),
            Action::Browser => {
                crate::apps::open_role(Role::Browser, &self.apps_chosen, &self.socket)
            }
            Action::Settings => self.open_settings(),
        }
    }

    pub(super) fn follow_looks(&mut self, config: &Config) {
        let look = config.appearance.clone();
        let paths = asset_paths(config);
        let was = std::mem::replace(&mut self.appearance, look.clone());
        if was != look {
            tell_gnome(Some(&was), &look);
            cursor_env(&look);
        }
        if was.icons != look.icons {
            drifter_core::icon::set_theme(look.icons.clone());
            self.app_icons.clear();
            self.no_icon.clear();
        }
        let cursor_moved =
            (&was.cursor_theme, was.cursor_size) != (&look.cursor_theme, look.cursor_size);
        if cursor_moved {
            self.named_cursors.clear();
        }
        if paths == self.asset_paths && !cursor_moved && was.icons == look.icons {
            return;
        }
        if paths[0] != self.asset_paths[0] || cursor_moved {
            match load_arrow(&mut self.renderer, config) {
                Ok((arrow, size, hotspot)) => {
                    self.arrow = arrow;
                    self.arrow_size = size;
                    self.arrow_hotspot = hotspot;
                }
                Err(error) => eprintln!("drifter: {error}"),
            }
        }
        if paths[1] != self.asset_paths[1] {
            match Font::find(config.font_path(), config.tray.size) {
                Ok(font) => self.font = Some(font),
                Err(error) => eprintln!("drifter: no clock: {error}"),
            }
            self.lock_big = Font::find(config.font_path(), 56.0).ok();
            self.lock_small = Font::find(config.font_path(), 15.0).ok();
            self.clock = None;
            self.stats = None;
            self.titles.clear();
        }
        if paths[2] != self.asset_paths[2] {
            let (skin, chrome) = load_skin(&mut self.renderer, config);
            self.skin = skin;
            self.chrome = chrome;
            self.chrome.follow(&config.window);
        }
        if paths[3] != self.asset_paths[3] {
            let side = self.dock.icon as u32;
            let (trash, cog, spare) = load_dock_art(&mut self.renderer, config, side);
            self.trash = trash;
            self.cog = cog;
            self.dock_spare = spare;
        }
        if was.icons != look.icons || paths[3] != self.asset_paths[3] {
            let side = self.dock.icon as u32;
            match dock_items(&mut self.renderer, config, side, self.dock_spare.as_ref()) {
                Ok(pinned) => {
                    self.pinned = pinned;
                    self.pinned_from = (config.dock.clone(), config.apps.clone());
                }
                Err(error) => eprintln!("drifter: {error}"),
            }
        }
        self.asset_paths = paths;
    }

    pub(super) fn remember_note(&mut self, note: &drifter_core::notify::Note) {
        if self.noted.contains(&note.id) {
            return;
        }
        self.noted.push(note.id);
        if self.noted.len() > 64 {
            self.noted.remove(0);
        }
        if !self.notify.history {
            return;
        }
        let path = Config::state_dir().join("notes");
        let when = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_secs())
            .unwrap_or(0);
        let recent = drifter_core::notify::Recent {
            when,
            app: note.app.clone(),
            summary: note.summary.clone(),
            body: note.body.clone(),
        };
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        let out = drifter_core::notify::remember(&text, &recent);
        if let Some(folder) = path.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        if let Err(error) = std::fs::write(&path, out) {
            eprintln!("drifter: cannot keep the note history: {error}");
        }
    }

    pub(super) fn watch_config(&mut self) {
        let seen = config_stamp(&self.config_path);
        if seen == self.config_seen {
            return;
        }
        self.config_seen = seen;
        if seen.is_none() {
            return;
        }
        match Config::load_from(&self.config_path) {
            Ok(config) => {
                self.apply(&config);
                self.kick_now();
            }
            Err(error) => eprintln!("drifter: the config did not load: {error}"),
        }
    }

    pub(super) fn apply(&mut self, config: &Config) {
        self.screens_wide = (config.workspace.width, config.workspace.height);
        self.monitors = config.monitor.clone();
        self.mode = config.workspace.wallpaper_mode;
        self.holds = config.input.modifier;
        self.modifier = false;
        self.night = config.night;
        if self.keys != config.keyboard {
            self.set_keys(&config.keyboard);
        }
        if self.touch != config.touchpad || self.mouse != config.mouse {
            self.touch = config.touchpad;
            self.mouse = config.mouse;
            for pad in &mut self.touchpads {
                set_up_touchpad(pad, &self.touch, self.mouse.left_handed);
            }
            for mouse in &mut self.mice {
                set_up_mouse(mouse, &self.mouse);
            }
        }
        let pinned = self.tunables.bar_hides;
        self.tunables = match config.access.reduce_motion {
            true => Tunables::from(&config.motion).still(),
            false => Tunables::from(&config.motion),
        };
        if self.access.sticky_keys && !config.access.sticky_keys {
            let time = self.clock_ms();
            for (code, pressed) in self.sticky.clear() {
                self.feed_key(code, pressed, time);
            }
        }
        if self.access != config.access {
            tell_access(Some(&self.access), &config.access);
        }
        self.access = config.access;
        self.shots = config.screenshots.clone();
        for edge in Edge::ALL {
            if !pinned[edge.index()] && self.tunables.bar_hides[edge.index()] {
                self.set(edge, false);
            }
        }
        self.power_rules = config.power;
        self.notify = config.notify;
        self.set_shortcuts(&config.keys, config.input.modifier);
        self.set_places(&config.place, config.input.modifier);
        self.follow_looks(config);

        let heights = [config.drawers.top, config.drawers.bottom];
        let mut repaint = false;
        for edge in Edge::ALL {
            let shape = &mut self.shapes[edge.index()];
            if shape.height != heights[edge.index()] {
                shape.height = heights[edge.index()];
                repaint = true;
            }
        }
        match self.wants_rescan() {
            true => self.scan_heads(),
            false => self.relayout(repaint),
        }

        if self.tray.size != config.tray.size {
            self.tray.size = config.tray.size;
            match Font::find(config.font_path(), config.tray.size) {
                Ok(font) => self.font = Some(font),
                Err(error) => eprintln!("drifter: no clock: {error}"),
            }
            self.clock = None;
            self.stats = None;
            self.titles.clear();
        }
        self.format = config.tray.format.clone();
        self.chrome.follow(&config.window);
        self.lock_after = config.lock.after;
        self.screen_off = config.lock.screen_off;
        self.login = config.login.start.clone();
        self.autostart = config.login.autostart;
        let paper = Paper::from(config);
        if paper.set != self.paper.set {
            match paper.set.clone() {
                Some(path) => self.load_wallpaper(path),
                None => self.wallpaper = None,
            }
        }
        if (paper.folder.clone(), paper.every) != (self.paper.folder.clone(), self.paper.every) {
            self.paper.next = None;
        }
        self.paper.set = paper.set;
        self.paper.folder = paper.folder;
        self.paper.every = paper.every;
        self.paper.order = paper.order;
        self.dock.settings = config.dock.settings;
        self.dock.trash = config.dock.trash;
        self.apps_chosen = config.apps.clone();
        let wanted = (config.dock.clone(), config.apps.clone());
        if self.pinned_from != wanted {
            let side = self.dock.icon as u32;
            match dock_items(&mut self.renderer, config, side, self.dock_spare.as_ref()) {
                Ok(pinned) => {
                    self.pinned = pinned;
                    self.pinned_from = wanted;
                }
                Err(error) => eprintln!("drifter: {error}"),
            }
        }
        self.sampler.follow(&config.tray);
        self.stats = None;
    }
}

pub(super) fn config_stamp(path: &Path) -> Option<(std::time::SystemTime, u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len(), meta.ino()))
}

pub(super) fn cursor_env(look: &drifter_core::Appearance) {
    crate::apps::set_env("XCURSOR_THEME", look.cursor_theme.clone());
    crate::apps::set_env("XCURSOR_SIZE", Some(look.cursor_size.to_string()));
}

pub(super) fn tell_access(was: Option<&drifter_core::Access>, now: &drifter_core::Access) {
    let mut sets: Vec<(&'static str, &'static str, String)> = Vec::new();
    const FACE: &str = "org.gnome.desktop.interface";
    const A11Y: &str = "org.gnome.desktop.a11y.interface";
    let moved = |pick: fn(&drifter_core::Access) -> bool| match was {
        Some(was) => pick(was) != pick(now),
        None => pick(now),
    };
    if moved(|access| access.large_text) {
        let factor = match now.large_text {
            true => drifter_core::config::LARGE_TEXT,
            false => 1.0,
        };
        sets.push((FACE, "text-scaling-factor", format!("{factor}")));
    }
    if moved(|access| access.reduce_motion) {
        sets.push((FACE, "enable-animations", (!now.reduce_motion).to_string()));
    }
    if moved(|access| access.high_contrast) {
        sets.push((A11Y, "high-contrast", now.high_contrast.to_string()));
    }
    if sets.is_empty() {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("drifter-access".to_string())
        .spawn(move || {
            for (schema, key, value) in sets {
                let ran = std::process::Command::new("gsettings")
                    .args(["set", schema, key, &value])
                    .stderr(std::process::Stdio::null())
                    .status();
                match ran {
                    Ok(status) if status.success() => {}
                    Ok(_) => eprintln!("drifter: gsettings has no {schema} {key} here"),
                    Err(error) => {
                        eprintln!("drifter: apps keep their settings, gsettings did not run: {error}");
                        return;
                    }
                }
            }
        });
    if let Err(error) = spawned {
        eprintln!("drifter: cannot tell apps about accessibility: {error}");
    }
}

pub(super) fn tell_gnome(was: Option<&drifter_core::Appearance>, look: &drifter_core::Appearance) {
    use drifter_core::Style;
    let mut sets: Vec<(&'static str, String)> = Vec::new();
    let changed = |pick: fn(&drifter_core::Appearance) -> Option<String>| {
        let now = pick(look);
        now.clone()
            .filter(|_| was.is_none_or(|was| pick(was) != now))
    };
    if let Some(style) = changed(|look| look.style.map(|s| format!("{s:?}"))) {
        let scheme = if style == format!("{:?}", Style::Dark) {
            "prefer-dark"
        } else {
            "default"
        };
        sets.push(("color-scheme", scheme.to_string()));
        sets.push(("gtk-theme", style));
    }
    if let Some(accent) = changed(|look| look.accent.map(|a| a.word().to_string())) {
        sets.push(("accent-color", accent));
    }
    if let Some(icons) = changed(|look| look.icons.clone()) {
        sets.push(("icon-theme", icons));
    }
    if let Some(cursor) = changed(|look| look.cursor_theme.clone()) {
        sets.push(("cursor-theme", cursor));
    }
    let size_moved = match was {
        Some(was) => was.cursor_size != look.cursor_size,
        None => look.cursor_size != drifter_core::Appearance::default().cursor_size,
    };
    if size_moved {
        sets.push(("cursor-size", look.cursor_size.to_string()));
    }
    if sets.is_empty() {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("drifter-gsettings".to_string())
        .spawn(move || {
            const SCHEMA: &str = "org.gnome.desktop.interface";
            for (key, value) in sets {
                let value = match key {
                    "gtk-theme" => {
                        let now = std::process::Command::new("gsettings")
                            .args(["get", SCHEMA, "gtk-theme"])
                            .output()
                            .map(|out| {
                                String::from_utf8_lossy(&out.stdout)
                                    .trim()
                                    .trim_matches('\'')
                                    .to_string()
                            })
                            .unwrap_or_default();
                        if now != "Adwaita" && now != "Adwaita-dark" {
                            continue;
                        }
                        match value.as_str() {
                            "Dark" => "Adwaita-dark".to_string(),
                            _ => "Adwaita".to_string(),
                        }
                    }
                    _ => value,
                };
                let ran = std::process::Command::new("gsettings")
                    .args(["set", SCHEMA, key, &value])
                    .stderr(std::process::Stdio::null())
                    .status();
                match ran {
                    Ok(status) if status.success() => {}
                    Ok(_) => eprintln!("drifter: gsettings has no {key} here"),
                    Err(error) => {
                        eprintln!("drifter: apps keep their look, gsettings did not run: {error}");
                        return;
                    }
                }
            }
        });
    if let Err(error) = spawned {
        eprintln!("drifter: cannot tell apps about the look: {error}");
    }
}
