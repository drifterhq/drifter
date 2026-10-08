use super::*;

impl State {
    pub(super) fn open_settings(&mut self) {
        self.open_settings_at("");
    }

    pub(super) fn open_settings_at(&mut self, page: &str) {
        self.open_settings_with(&[page]);
    }

    pub(super) fn open_settings_with(&mut self, words: &[&str]) {
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
        argv.extend(words.iter().filter(|word| !word.is_empty()).map(|word| word.to_string()));
        crate::apps::launch(&argv, &self.socket);
    }

    pub(super) fn set_keys(&mut self, keys: &drifter_core::Keyboard) {
        let keyboard = self.wayland.keyboard.clone();
        if keys.layout != self.keys.layout
            || keys.variant != self.keys.variant
            || keys.options != self.keys.options
        {
            if let Err(error) = keyboard.set_xkb_config(self, crate::shell::xkb(keys)) {
                log!(
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
                (format!("{name} saved"), format!("{} + {slot} comes back here.", self.holds.name()))
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
            Action::Screenshot | Action::ShotScreen => self.take_still(None),
            Action::ShotPart => {
                use drifter_core::grab::{Pick, Take, Tool};
                self.open_shooter(Tool::new(Take::Still, Pick::Portion), false);
            }
            Action::ShotBar => match self.recording.is_some() {
                true => self.stop_recording(),
                false => self.open_shooter(self.shot_tool, true),
            },
            Action::Lock => {
                self.lock_now();
            }
            Action::Search => {
                self.toggle_search();
                self.welcome_did(drifter_core::welcome::Lesson::Search);
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
        let style = drifter_core::theme::now(look.style);
        let restyled = style != self.style;
        self.style = style;
        if was != look || restyled {
            tell_gnome(Some(&was), &look, restyled.then_some(style));
            cursor_env(&look);
        }
        let layout = config.window.gtk_layout();
        if layout != self.gtk_layout {
            self.gtk_layout = layout.clone();
            tell_buttons(layout);
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
        let palette = drifter_core::theme::of(style);
        if palette != self.palette {
            self.recolor(config, palette);
        }
        self.look.accent = look
            .accent
            .map(|accent| accent.color())
            .unwrap_or(drifter_core::menu::ACCENT);
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
                Err(error) => log!("drifter: {error}"),
            }
        }
        if paths[1] != self.asset_paths[1] {
            match Font::find(config.font_path(), config.tray.size) {
                Ok(font) => self.font = Some(std::sync::Arc::new(font)),
                Err(error) => log!("drifter: no clock: {error}"),
            }
            self.lock_big = Font::find(config.font_path(), 180.0).ok().map(std::sync::Arc::new);
            self.lock_small = Font::find(config.font_path(), 17.0).ok().map(std::sync::Arc::new);
            self.lock_name = Font::find(config.font_path(), 26.0).ok().map(std::sync::Arc::new);
            self.card_font = Font::find(config.font_path(), 11.5).ok().map(std::sync::Arc::new);
            self.note_head = Font::find(config.font_path(), 13.0).ok().map(std::sync::Arc::new);
            self.note_body = Font::find(config.font_path(), 12.0).ok().map(std::sync::Arc::new);
            self.clock = None;
            self.stats = None;
            self.battery_seen = None;
            self.battery_at = None;
            self.titles.clear();
        }
        if paths[2] != self.asset_paths[2] {
            let (skin, chrome) = load_skin(&mut self.renderer, config, &self.palette);
            self.skin = skin;
            self.chrome = chrome;
            self.chrome.follow(&config.window);
        }
        if paths[3] != self.asset_paths[3] {
            let side = self.dock.icon as u32;
            let (trash, cog, spare) = load_dock_art(&mut self.renderer, config, side, &self.palette);
            self.trash_full_art = None;
            self.dock_art_dir = config.dock_dir();
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
                Err(error) => log!("drifter: {error}"),
            }
        }
        self.asset_paths = paths;
    }

    pub(super) fn recolor(&mut self, config: &Config, palette: drifter_core::theme::Palette) {
        self.palette = palette;
        self.look = Look::themed(&palette);
        self.popover = None;
        self.tip = None;
        self.tip_of = None;
        self.tip_want = None;
        self.tray.ink = palette.ink;
        self.battery_seen = None;
        self.battery_at = None;
        self.show_battery = config.tray.battery;
        self.clock = None;
        self.stats = None;
        self.app_label = None;
        self.search = None;
        self.control = None;
        self.notes_panel = None;
        self.glance_panel = None;
        let button = drifter_core::control::paint_icon(self.tray.button as u32, palette.ink);
        if let Ok(Some(buffer)) = upload(&mut self.renderer, Some(&button), "control center button") {
            self.control_icon = Some(buffer);
        }
        self.titles.clear();
        let power = drifter_core::tray::paint_power(self.tray.button as u32, palette.ink);
        match upload(&mut self.renderer, Some(&power), "power") {
            Ok(Some(buffer)) => self.power = Some(buffer),
            Ok(None) => {}
            Err(error) => log!("drifter: {error}"),
        }
        let (skin, chrome) = load_skin(&mut self.renderer, config, &palette);
        self.skin = skin;
        self.chrome = chrome;
        self.chrome.follow(&config.window);
        let side = self.dock.icon as u32;
        let (trash, cog, spare) = load_dock_art(&mut self.renderer, config, side, &palette);
        self.trash_full_art = None;
        self.badge_art.clear();
        self.trash = trash;
        self.cog = cog;
        self.dock_spare = spare;
        let dock = drifter_core::dock::Dock::themed(&palette).with_icon(self.dock.icon);
        let dot = drifter_core::dock::paint_dot(dock.dot_side() as u32, palette.ink);
        if let Ok(Some(buffer)) = upload(&mut self.renderer, Some(&dot), "running dot") {
            self.dot = Some(buffer);
        }
        let rule = drifter_core::Image {
            width: 1,
            height: 1,
            rgba: vec![dock.rule[0] / 2, dock.rule[1] / 2, dock.rule[2] / 2, 128],
        };
        if let Ok(Some(buffer)) = upload(&mut self.renderer, Some(&rule), "dock divider") {
            self.rule = Some(buffer);
        }
        self.frost_key = None;
        self.drops = paint_drops(&mut self.renderer, self.screen.x, &self.shapes, &palette);
        self.take_icons(self.tray_raw.clone());
        self.relayout(true);
        self.welcome_restyle();
        self.kick_now();
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
            log!("drifter: cannot keep the note history: {error}");
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
                self.welcome_config_changed();
                self.kick_now();
            }
            Err(error) => log!("drifter: the config did not load: {error}"),
        }
    }

    pub(super) fn watch_style(&mut self) {
        if self.restyle_due.is_some() || drifter_core::theme::now(self.appearance.style) == self.style {
            return;
        }
        match Config::load_from(&self.config_path) {
            Ok(config) if config.access.reduce_motion || self.renderer.is_pixman() => {
                self.follow_looks(&config);
                self.kick_now();
            }
            Ok(config) => {
                self.restyle_due = Some(Box::new(config));
                self.style_shots.clear();
                self.style_fade = None;
                self.kick_now();
            }
            Err(error) => log!("drifter: the look waits, the config did not load: {error}"),
        }
    }

    pub(super) fn shoot_old_style(&mut self, shown: &[Shown]) {
        if self.restyle_due.is_none() {
            return;
        }
        let bytes = match self.capture(shown, false) {
            Ok(bytes) => bytes,
            Err(error) => {
                log!("drifter: the look changes without a fade: {error}");
                return;
            }
        };
        let whole = self.physical();
        let mut rgba = Vec::with_capacity(bytes.len());
        for pixel in bytes.as_chunks::<4>().0 {
            rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }
        let image = Image {
            width: whole.x as u32,
            height: whole.y as u32,
            rgba,
        };
        match upload_flat(&mut self.renderer, Some(&image), "the old look") {
            Ok(Some(shot)) => self.style_shots.push((self.at, shot)),
            Ok(None) => {}
            Err(error) => log!("drifter: {error}"),
        }
    }

    pub(super) fn restyle(&mut self, now: Instant) {
        let Some(config) = self.restyle_due.take() else {
            return;
        };
        self.follow_looks(&config);
        if !self.style_shots.is_empty() {
            self.style_fade = Some(now);
        }
    }

    pub(super) fn settle_style_fade(&mut self) {
        if self.style_fade.is_some_and(|since| since.elapsed() >= STYLE_FADE) {
            self.style_fade = None;
        }
        if self.style_fade.is_none() && self.restyle_due.is_none() {
            self.style_shots.clear();
        }
    }

    pub(super) fn style_fade_elements(&self, elements: &mut Vec<Skin>) {
        let Some(since) = self.style_fade else {
            return;
        };
        let Some((_, shot)) = self.style_shots.iter().find(|(head, _)| *head == self.at) else {
            return;
        };
        let t = (since.elapsed().as_secs_f64() / STYLE_FADE.as_secs_f64()).min(1.0);
        let alpha = 1.0 - t * t * (3.0 - 2.0 * t);
        let at = self.cursor_count.min(elements.len());
        let whole = Rect::new(Vec2::ZERO, self.screen);
        elements.insert(at, Skin::Texture(element(shot, whole, alpha as f32)));
    }

    pub(super) fn apply(&mut self, config: &Config) {
        let laid = (config.workspace.width, config.workspace.height) != self.screens_wide
            || config.monitor != self.monitors;
        self.screens_wide = (config.workspace.width, config.workspace.height);
        if config.tray.battery && !self.show_battery {
            self.battery_at = None;
            self.battery_seen = None;
        }
        self.show_battery = config.tray.battery;
        self.window_pace = window_pace(config);
        let reduce = config.access.reduce_motion;
        self.dock_logo.pace = drifter_core::welcome::logo_pace(
            drifter_core::welcome::Motion::of(reduce, config.workspace.wallpaper_motion),
            reduce,
        );
        self.lock_logo.pace = self.dock_logo.pace;
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

        let heights = [config.drawers.top, config.dock_height()];
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
            false if laid || repaint => self.relayout(repaint),
            false => {}
        }

        if self.tray.size != config.tray.size {
            self.tray.size = config.tray.size;
            match Font::find(config.font_path(), config.tray.size) {
                Ok(font) => self.font = Some(std::sync::Arc::new(font)),
                Err(error) => log!("drifter: no clock: {error}"),
            }
            self.clock = None;
            self.stats = None;
            self.battery_seen = None;
            self.battery_at = None;
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
        self.paper.drift = paper.drift;
        self.dock.settings = config.dock.settings;
        self.dock.trash = config.dock.trash;
        self.apps_chosen = config.apps.clone();
        let wanted = (config.dock.clone(), config.apps.clone());
        self.dock_fit = config.dock.fit;
        self.dock_pad = config.dock_padding();
        self.dock.margin = dock_margin(config);
        let resized = self.dock.with_icon(config.dock_icon());
        self.dock.gap = config.dock_spacing();
        let regrown = resized.icon != self.dock.icon;
        if regrown {
            self.dock = resized.spaced(config.dock_spacing());
            let side = self.dock.icon as u32;
            let (trash, cog, spare) = load_dock_art(&mut self.renderer, config, side, &self.palette);
            self.trash = trash;
            self.cog = cog;
            self.dock_spare = spare;
            self.trash_full_art = None;
            self.app_icons.clear();
            self.no_icon.clear();
            let dot = drifter_core::dock::paint_dot(self.dock.dot_side() as u32, self.palette.ink);
            if let Ok(Some(buffer)) = upload(&mut self.renderer, Some(&dot), "running dot") {
                self.dot = Some(buffer);
            }
        }
        if self.pinned_from != wanted || regrown {
            let side = self.dock.icon as u32;
            match dock_items(&mut self.renderer, config, side, self.dock_spare.as_ref()) {
                Ok(pinned) => {
                    self.pinned = pinned;
                    self.pinned_from = wanted;
                }
                Err(error) => log!("drifter: {error}"),
            }
        }
        self.sampler.follow(&config.tray);
        self.glance = drifter_core::glance::Style::parse(&config.tray.style).unwrap_or(drifter_core::glance::Style::Calm);
        self.sampled = None;
        self.glance_card = None;
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

const FACE: &str = "org.gnome.desktop.interface";

pub(super) fn tell_access(was: Option<&drifter_core::Access>, now: &drifter_core::Access) {
    let mut sets: Vec<(&'static str, &'static str, String)> = Vec::new();
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
    gsettings(sets);
}

pub(super) fn tell_gnome(
    was: Option<&drifter_core::Appearance>,
    look: &drifter_core::Appearance,
    style: Option<drifter_core::Style>,
) {
    let mut sets: Vec<(&'static str, &'static str, String)> = Vec::new();
    let changed = |pick: fn(&drifter_core::Appearance) -> Option<String>| {
        let now = pick(look);
        now.clone()
            .filter(|_| was.is_none_or(|was| pick(was) != now))
    };
    if let Some(style) = style {
        let (scheme, theme) = match style {
            drifter_core::Style::Light => ("default", "Adwaita"),
            _ => ("prefer-dark", "Adwaita-dark"),
        };
        sets.push((FACE, "color-scheme", scheme.to_string()));
        sets.push((FACE, "gtk-theme", theme.to_string()));
    }
    if let Some(accent) = changed(|look| look.accent.map(|a| a.word().to_string())) {
        sets.push((FACE, "accent-color", accent));
    }
    if let Some(icons) = changed(|look| look.icons.clone()) {
        sets.push((FACE, "icon-theme", icons));
    }
    if let Some(cursor) = changed(|look| look.cursor_theme.clone()) {
        sets.push((FACE, "cursor-theme", cursor));
    }
    let size_moved = match was {
        Some(was) => was.cursor_size != look.cursor_size,
        None => look.cursor_size != drifter_core::Appearance::default().cursor_size,
    };
    if size_moved {
        sets.push((FACE, "cursor-size", look.cursor_size.to_string()));
    }
    gsettings(sets);
}

pub(super) fn tell_buttons(layout: String) {
    gsettings(vec![("org.gnome.desktop.wm.preferences", "button-layout", layout)]);
}

pub(super) fn dress_gtk4() {
    let dir = drifter_core::theme::gtk4_dir();
    if let Err(error) = drifter_core::theme::install_gtk4_css(&dir) {
        log!(
            "drifter: gtk 4 apps keep their own colors, {} is not writable: {error}",
            dir.display()
        );
    }
}

fn gsettings(sets: Vec<(&'static str, &'static str, String)>) {
    if sets.is_empty() {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("drifter-gsettings".to_string())
        .spawn(move || {
            for (schema, key, value) in sets {
                if key == "gtk-theme" {
                    let now = std::process::Command::new("gsettings")
                        .args(["get", schema, key])
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
                }
                let ran = std::process::Command::new("gsettings")
                    .args(["set", schema, key, &value])
                    .stderr(std::process::Stdio::null())
                    .status();
                match ran {
                    Ok(status) if status.success() => {}
                    Ok(_) => log!("drifter: gsettings has no {schema} {key} here"),
                    Err(error) => {
                        log!("drifter: apps keep their settings, gsettings did not run: {error}");
                        return;
                    }
                }
            }
        });
    if let Err(error) = spawned {
        log!("drifter: cannot tell apps about the look: {error}");
    }
}
