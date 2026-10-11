use super::*;
use drifter_core::dock::same_app;

const DRAG_START: f64 = 6.0;
const BAR_HIDING: i32 = 1;
const BAR_SIZE: i32 = 2;
const BAR_COG: i32 = 3;
const BAR_TRASH: i32 = 4;
const BAR_SETTINGS: i32 = 5;
const BAR_SIZE_BASE: i32 = 1000;
const DOCK_SIZES: [(i64, &str); 3] = [(52, "Small"), (64, "Medium"), (84, "Large")];
const HOP: f64 = 0.42;

pub(super) struct DockApp {
    pub launch: drifter_core::Launch,
    pub key: String,
    pub pinned: bool,
    pub place: Option<String>,
}

pub(super) struct DockDrag {
    pub from: usize,
    pub key: String,
    pub grab: Vec2,
}

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) struct Badge {
    pub count: Option<i64>,
    pub progress: Option<f64>,
    pub urgent: bool,
}

impl State {
    pub(super) fn dock_apps(&self) -> Vec<DockApp> {
        let mut apps: Vec<DockApp> = self
            .pinned
            .iter()
            .map(|(launch, _)| DockApp {
                key: launch
                    .app()
                    .map(str::to_string)
                    .unwrap_or_else(|| launch.name()),
                place: launch
                    .command()
                    .and_then(drifter_core::place::from_dock)
                    .map(str::to_string),
                launch: launch.clone(),
                pinned: true,
            })
            .collect();
        for window in self
            .windows
            .iter()
            .filter(|window| window.placed && !window.going && !window.app.is_empty())
            .filter(|window| window.app != SETTINGS_APP)
        {
            if apps
                .iter()
                .any(|app| app.place.is_none() && same_app(&app.key, &window.app))
            {
                continue;
            }
            apps.push(DockApp {
                launch: drifter_core::Launch::App(window.app.clone()),
                key: window.app.clone(),
                pinned: false,
                place: None,
            });
        }
        apps
    }

    pub(super) fn windows_of(&self, key: &str) -> Vec<usize> {
        self.windows
            .iter()
            .enumerate()
            .filter(|(_, window)| window.placed && !window.going && same_app(key, &window.app))
            .map(|(at, _)| at)
            .collect()
    }

    pub(super) fn dock_order(&self, count: usize) -> Vec<Option<usize>> {
        let Some(drag) = self.dock_drag.as_ref() else {
            return (0..count).map(Some).collect();
        };
        let mut order: Vec<Option<usize>> = (0..count).filter(|at| *at != drag.from).map(Some).collect();
        if let Some(target) = self.drag_target(count) {
            order.insert(target.min(order.len()), None);
        }
        order
    }

    fn drag_target(&self, count: usize) -> Option<usize> {
        let band = self.dock_band()?;
        if self.pointer.y < band.origin.y - self.dock.icon * 1.5 {
            return None;
        }
        let minimized = self.minimized().len();
        let shown = count;
        (0..shown).min_by(|a, b| {
            let center = |at: usize| {
                self.dock
                    .slot_rect(band, shown, minimized, Slot::Pinned(at))
                    .center()
                    .x
            };
            (center(*a) - self.pointer.x)
                .abs()
                .total_cmp(&(center(*b) - self.pointer.x).abs())
        })
    }

    pub(super) fn press_dock(&mut self, code: u32) -> bool {
        let Some(band) = self.dock_band() else {
            return false;
        };
        let hidden = self.minimized();
        let apps = self.dock_apps();
        let Some(slot) = self.dock.at(band, self.pointer, apps.len(), hidden.len()) else {
            return false;
        };
        match slot {
            Slot::Pinned(index) if code == BTN_MIDDLE => {
                if let Some(app) = apps.get(index) {
                    self.launch_from_dock(&app.launch, &app.key);
                }
            }
            Slot::Pinned(index) => self.dock_press = Some((index, self.pointer)),
            Slot::Settings => self.open_settings(),
            Slot::Trash => {
                let trash = trash_files();
                let _ = std::fs::create_dir_all(&trash);
                crate::apps::open_folder(
                    &trash.to_string_lossy(),
                    self.apps_chosen.files.as_ref(),
                    &self.socket,
                );
            }
            Slot::Minimized(index) => {
                let Some(place) = self
                    .windows
                    .iter()
                    .enumerate()
                    .filter(|(_, window)| window.hidden)
                    .map(|(at, _)| at)
                    .nth(index)
                else {
                    return true;
                };
                let seat = self.dock_slot(index);
                self.windows[place].hidden = false;
                self.windows[place].motion = Some(Motion::new(Phase::Restoring, seat));
                self.focus_window(place);
            }
        }
        true
    }

    pub(super) fn drag_dock(&mut self) -> bool {
        if let Some((index, start)) = self.dock_press
            && self.dock_drag.is_none()
            && (self.pointer - start).length() > DRAG_START
        {
            let apps = self.dock_apps();
            if let (Some(app), Some(band)) = (apps.get(index), self.dock_band()) {
                let rect = self.dock.slot_rect(
                    band,
                    apps.len(),
                    self.minimized().len(),
                    Slot::Pinned(index),
                );
                self.dock_drag = Some(DockDrag {
                    from: index,
                    key: app.key.clone(),
                    grab: start - rect.origin,
                });
                self.tip = None;
                self.tip_of = None;
                self.tip_want = None;
            }
        }
        if self.dock_drag.is_some() {
            self.kick_now();
            return true;
        }
        false
    }

    pub(super) fn release_dock(&mut self) -> bool {
        let pressed = self.dock_press.take();
        if self.dock_drag.is_some() {
            let target = self.drag_target(self.dock_apps().len());
            if let Some(drag) = self.dock_drag.take() {
                self.drop_dock(drag, target);
            }
            self.kick_now();
            return true;
        }
        let Some((index, _)) = pressed else {
            return false;
        };
        let apps = self.dock_apps();
        let Some(app) = apps.get(index) else {
            return true;
        };
        if drifter_core::launchpad::is_button(&app.launch) {
            self.toggle_launchpad();
            return true;
        }
        if let Some(name) = app.place.clone() {
            if !self.go_to_named_place(&name) {
                log!("drifter: the dock names a place that is gone: {name}");
            }
            return true;
        }
        let (launch, key) = (app.launch.clone(), app.key.clone());
        self.open_from_dock(&launch, &key);
        true
    }

    fn open_from_dock(&mut self, launch: &drifter_core::Launch, key: &str) {
        let windows = self.windows_of(key);
        let Some(&top) = windows.last() else {
            self.launch_from_dock(launch, key);
            return;
        };
        let shown: Vec<usize> = windows
            .iter()
            .copied()
            .filter(|at| !self.windows[*at].hidden)
            .collect();
        let focused = self
            .focus
            .is_some_and(|id| windows.iter().any(|at| self.windows[*at].id == id));
        let next = match (focused, shown.first(), shown.last()) {
            (true, Some(&first), _) if shown.len() > 1 => first,
            (_, _, Some(&last)) => last,
            _ => top,
        };
        self.bring(next);
    }

    fn launch_from_dock(&mut self, launch: &drifter_core::Launch, key: &str) {
        crate::apps::run(launch, &self.socket);
        self.bounces.retain(|(held, _)| held != key);
        self.bounces.push((key.to_string(), Instant::now()));
        self.kick_now();
    }

    pub(super) fn settle_bounces(&mut self) {
        let done = HOP * f64::from(drifter_core::dock::BOUNCES);
        let windows: Vec<String> = self
            .windows
            .iter()
            .filter(|window| window.placed)
            .map(|window| window.app.clone())
            .collect();
        self.bounces.retain(|(key, since)| {
            let elapsed = since.elapsed().as_secs_f64();
            let arrived = elapsed > HOP && windows.iter().any(|app| same_app(key, app));
            elapsed < done && !arrived
        });
    }

    fn bounce_of(&self, key: &str) -> f64 {
        self.bounces
            .iter()
            .find(|(held, _)| held == key)
            .map(|(_, since)| drifter_core::dock::bounce(since.elapsed().as_secs_f64(), self.dock.icon))
            .unwrap_or(0.0)
    }

    fn drop_dock(&mut self, drag: DockDrag, target: Option<usize>) {
        let apps = self.dock_apps();
        let Some(moved) = apps.get(drag.from) else {
            return;
        };
        if moved.key != drag.key {
            return;
        }
        let mut off: Vec<&str> = Vec::new();
        let pinned_count = apps.iter().filter(|app| app.pinned).count();
        let order: Vec<(drifter_core::Launch, bool)> = match target {
            None if !moved.pinned => return,
            None => {
                off.extend(self.builtin_switch(&moved.launch));
                apps.iter()
                    .enumerate()
                    .filter(|(at, _)| *at != drag.from)
                    .map(|(_, app)| (app.launch.clone(), app.pinned))
                    .collect()
            }
            Some(target) => {
                let mut rest: Vec<(drifter_core::Launch, bool)> = apps
                    .iter()
                    .enumerate()
                    .filter(|(at, _)| *at != drag.from)
                    .map(|(_, app)| (app.launch.clone(), app.pinned))
                    .collect();
                let pin = moved.pinned || target < pinned_count.max(1);
                rest.insert(target.min(rest.len()), (moved.launch.clone(), pin));
                rest
            }
        };
        let pinned: Vec<drifter_core::Launch> = order
            .into_iter()
            .filter(|(_, pinned)| *pinned)
            .map(|(launch, _)| launch)
            .collect();
        self.save_dock(&pinned, &off);
    }

    fn builtin_switch(&self, launch: &drifter_core::Launch) -> Option<&'static str> {
        use drifter_core::desktop::Role;
        if drifter_core::launchpad::is_button(launch) {
            return Some("apps");
        }
        let (dock, apps) = &self.pinned_from;
        [("files", Role::Files), ("terminal", Role::Terminal), ("browser", Role::Browser)]
            .into_iter()
            .zip([dock.files, dock.terminal, dock.browser])
            .filter(|(_, on)| *on)
            .find(|((_, role), _)| {
                let item = apps
                    .get(*role)
                    .cloned()
                    .or_else(|| drifter_core::desktop::system_default(*role).map(drifter_core::Launch::App));
                item.as_ref() == Some(launch)
            })
            .map(|((key, _), _)| key)
    }

    pub(super) fn save_dock(&mut self, pinned: &[drifter_core::Launch], off: &[&str]) {
        let path = self.config_path.clone();
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        match drifter_core::settings::write_dock(&text, pinned, off) {
            Ok(out) => {
                if let Err(error) = std::fs::write(&path, out) {
                    log!("drifter: cannot save the dock: {error}");
                }
            }
            Err(error) => log!("drifter: cannot save the dock: {error}"),
        }
    }

    pub(super) fn dock_menu(&mut self) -> bool {
        let Some(band) = self.dock_band() else {
            return false;
        };
        let hidden = self.minimized();
        let apps = self.dock_apps();
        match self.dock.at(band, self.pointer, apps.len(), hidden.len()) {
            Some(Slot::Pinned(index)) => {
                let Some(app) = apps.get(index) else {
                    return true;
                };
                let mut items = Vec::new();
                for (n, at) in self.windows_of(&app.key).into_iter().enumerate() {
                    let title = match self.windows[at].title.trim() {
                        "" => app.launch.name(),
                        title => title.to_string(),
                    };
                    items.push(Item::line(100 + n as i32, &title));
                }
                if !items.is_empty() {
                    items.push(Item::rule(99));
                }
                let button = drifter_core::launchpad::is_button(&app.launch);
                if app.place.is_none() && !button {
                    items.push(Item::line(1, drifter_core::lang::t("New Window")));
                }
                items.push(Item::line(
                    2,
                    match app.pinned {
                        true => drifter_core::lang::t("Remove from Dock"),
                        false => drifter_core::lang::t("Keep in Dock"),
                    },
                ));
                items.push(Item::line(4, drifter_core::lang::t("Settings…")));
                if !self.windows_of(&app.key).is_empty() {
                    items.push(Item::rule(98));
                    items.push(Item::line(3, drifter_core::lang::t("Quit")));
                }
                let at = self.pointer;
                self.open_popover(items, at, Target::Dock(app.key.clone()));
                true
            }
            Some(Slot::Trash) => {
                let items = vec![
                    Item::line(10, drifter_core::lang::t("Open")),
                    Item {
                        enabled: self.trash_full,
                        ..Item::line(11, drifter_core::lang::t("Empty Trash"))
                    },
                ];
                let at = self.pointer;
                self.open_popover(items, at, Target::Dock(String::new()));
                true
            }
            _ => false,
        }
    }

    pub(super) fn dock_bar_menu(&mut self) -> bool {
        let Some(band) = self.dock_band() else {
            return false;
        };
        if !band.contains(self.pointer) {
            return false;
        }
        let at = self.pointer;
        let items = self.dock_bar_items();
        self.open_popover(items, at, Target::DockBar);
        true
    }

    fn dock_bar_items(&self) -> Vec<Item> {
        let hides = self.tunables.bar_hides[Edge::Bottom.index()];
        vec![
            Item::line(
                BAR_HIDING,
                match hides {
                    true => drifter_core::lang::t("Turn Hiding Off"),
                    false => drifter_core::lang::t("Turn Hiding On"),
                },
            ),
            Item::more(BAR_SIZE, drifter_core::lang::t("Dock Size")),
            Item::rule(90),
            Item::line(
                BAR_COG,
                match self.dock.settings {
                    true => drifter_core::lang::t("Hide Settings Button"),
                    false => drifter_core::lang::t("Show Settings Button"),
                },
            ),
            Item::line(
                BAR_TRASH,
                match self.dock.trash {
                    true => drifter_core::lang::t("Hide Trash"),
                    false => drifter_core::lang::t("Show Trash"),
                },
            ),
            Item::rule(91),
            Item::line(BAR_SETTINGS, drifter_core::lang::t("Dock Settings…")),
        ]
    }

    pub(super) fn dock_bar_pick(&mut self, id: i32) {
        let at = self.popover.as_ref().map(|open| open.rect.origin).unwrap_or(self.pointer);
        match id {
            BAR_SIZE => {
                let now = self.shapes[Edge::Bottom.index()].height.round() as i64;
                let sizes = DOCK_SIZES
                    .iter()
                    .map(|(height, name)| Item {
                        enabled: *height != now,
                        ..Item::line(BAR_SIZE_BASE + *height as i32, name)
                    })
                    .collect();
                self.open_popover(drifter_core::menu::with_back(sizes), at, Target::DockBar);
                return;
            }
            drifter_core::menu::BACK => {
                let items = self.dock_bar_items();
                self.open_popover(items, at, Target::DockBar);
                return;
            }
            BAR_HIDING => {
                let hide = !self.tunables.bar_hides[Edge::Bottom.index()];
                self.save_key(|text| drifter_core::settings::write_flag(text, "motion", "hide_bottom", hide));
            }
            BAR_COG => {
                let show = !self.dock.settings;
                self.save_key(|text| drifter_core::settings::write_flag(text, "dock", "settings", show));
            }
            BAR_TRASH => {
                let show = !self.dock.trash;
                self.save_key(|text| drifter_core::settings::write_flag(text, "dock", "trash", show));
            }
            BAR_SETTINGS => self.open_settings_at("dock"),
            size if size > BAR_SIZE_BASE => {
                let height = f64::from(size - BAR_SIZE_BASE);
                let icon = (height * 0.82).round();
                let padding = ((height - icon) / 2.0).round();
                self.save_key(|text| {
                    drifter_core::settings::write_number(text, "dock", "icon", icon as i64)
                        .and_then(|text| drifter_core::settings::write_number(&text, "dock", "padding", padding as i64))
                });
            }
            _ => {}
        }
        self.popover = None;
    }

    pub(super) fn dock_menu_pick(&mut self, key: &str, id: i32) {
        if key.is_empty() {
            match id {
                10 => {
                    let trash = trash_files();
                    let _ = std::fs::create_dir_all(&trash);
                    crate::apps::open_folder(
                        &trash.to_string_lossy(),
                        self.apps_chosen.files.as_ref(),
                        &self.socket,
                    );
                }
                11 => {
                    empty_trash();
                    self.trash_seen = None;
                }
                _ => {}
            }
            return;
        }
        let apps = self.dock_apps();
        let Some(index) = apps.iter().position(|app| app.key == key) else {
            return;
        };
        let app = &apps[index];
        let windows = self.windows_of(key);
        match id {
            1 => {
                let launch = app.launch.clone();
                self.launch_from_dock(&launch, key);
            }
            2 => {
                let mut pinned: Vec<drifter_core::Launch> = apps
                    .iter()
                    .filter(|other| other.pinned && other.key != key)
                    .map(|other| other.launch.clone())
                    .collect();
                let mut off = Vec::new();
                match app.pinned {
                    true => off.extend(self.builtin_switch(&app.launch)),
                    false => pinned.push(app.launch.clone()),
                }
                self.save_dock(&pinned, &off);
            }
            3 => {
                for at in windows {
                    self.windows[at].toplevel.send_close();
                }
            }
            4 => self.edit_dock_item(&apps, index),
            n if n >= 100 => {
                if let Some(&at) = windows.get((n - 100) as usize) {
                    self.bring(at);
                }
            }
            _ => {}
        }
    }

    fn edit_dock_item(&mut self, apps: &[DockApp], index: usize) {
        let Some(app) = apps.get(index) else {
            return;
        };
        let mut pinned: Vec<drifter_core::Launch> = apps
            .iter()
            .filter(|other| other.pinned)
            .map(|other| other.launch.clone())
            .collect();
        let at = match app.pinned {
            true => index,
            false => {
                pinned.push(app.launch.clone());
                pinned.len() - 1
            }
        };
        let off: Vec<&str> = self
            .builtin_switch(&app.launch)
            .filter(|key| *key != "apps")
            .into_iter()
            .collect();
        self.save_dock(&pinned, &off);
        if let Err(error) = drifter_core::settings::ask_dock_item(at) {
            log!("drifter: cannot ask Settings for the dock item: {error}");
        }
        self.open_settings_at("dock");
    }

    pub(super) fn watch_trash(&mut self) {
        if self
            .trash_seen
            .is_some_and(|seen| seen.elapsed() < Duration::from_secs(2))
        {
            return;
        }
        self.trash_seen = Some(Instant::now());
        let full = std::fs::read_dir(trash_files())
            .map(|mut entries| entries.next().is_some())
            .unwrap_or(false);
        if full != self.trash_full {
            self.trash_full = full;
            self.kick_now();
        }
    }

    pub(super) fn take_badge(&mut self, app: String, badge: Badge) {
        self.badge_art.remove(&app);
        match badge == Badge::default() {
            true => {
                self.badges.remove(&app);
            }
            false => {
                self.badges.insert(app, badge);
            }
        }
        self.kick_now();
    }

    fn badge_for(&self, key: &str) -> Option<(String, Badge)> {
        self.badges
            .iter()
            .find(|(app, _)| same_app(app, key))
            .map(|(app, badge)| (app.clone(), *badge))
    }

    fn badge_texture(&mut self, app: &str, badge: Badge) -> Option<(TextureBuffer<Tex>, Vec2)> {
        if let Some(held) = self.badge_art.get(app) {
            return Some(held.clone());
        }
        let text = badge.count.map(drifter_core::dock::badge_text).unwrap_or_default();
        if text.is_empty() {
            return None;
        }
        let font = self.font.as_deref()?;
        let words = font.paint(&text, [255, 255, 255]);
        let size = drifter_core::dock::badge_rect(
            Rect::new(Vec2::ZERO, Vec2::splat(self.dock.icon)),
            drifter_core::paint::logical(&words).x,
        )
        .size;
        let mut canvas = drifter_core::paint::Canvas::new(size.x as u32, size.y as u32);
        canvas.round_rect(
            Rect::new(Vec2::ZERO, size),
            size.y / 2.0,
            [235, 64, 52],
            1.0,
        );
        canvas.stamp(&words, (size - drifter_core::paint::logical(&words)) / 2.0, 1.0);
        let buffer = upload(&mut self.renderer, Some(&canvas.image), "dock badge").ok()??;
        self.badge_art.insert(app.to_string(), (buffer.clone(), size));
        Some((buffer, size))
    }

    pub(super) fn dock_elements(&mut self, elements: &mut Vec<Skin>, band: Rect) {
        let missing: Vec<usize> = self
            .windows
            .iter()
            .enumerate()
            .filter(|(_, window)| {
                window.placed
                    && !window.app.is_empty()
                    && !self.app_icons.contains_key(&window.app)
                    && !self.no_icon.contains(&window.app)
            })
            .map(|(at, _)| at)
            .collect();
        for at in missing {
            self.remember_icon(at);
        }
        let hidden = self.minimized();
        let apps = self.dock_apps();
        let count = apps.len();
        let order = self.dock_order(count);
        let slot_of = |index: usize| order.iter().position(|held| *held == Some(index));

        if let Some(drag) = self.dock_drag.as_ref()
            && let Some(app) = apps.get(drag.from)
            && let Some(buffer) = self.dock_icon(app)
        {
            let at = self.pointer - drag.grab;
            let rect = Rect::new(at, Vec2::splat(self.dock.icon));
            let removing = self.drag_target(count).is_none();
            elements.push(Skin::Texture(element(&buffer, rect, if removing { 0.55 } else { 0.95 })));
        }

        let mut badges = Vec::new();
        for (index, app) in apps.iter().enumerate() {
            let Some(slot) = slot_of(index) else {
                continue;
            };
            let lift = self.bounce_of(&app.key);
            let rect = self.dock.slot_rect(band, count, hidden.len(), Slot::Pinned(slot));
            let rect = Rect::new(rect.origin + Vec2::new(0.0, lift), rect.size);
            if let Some((name, badge)) = self.badge_for(&app.key) {
                badges.push((name, badge, rect));
            }
        }
        for (name, badge, rect) in badges {
            if let Some((buffer, size)) = self.badge_texture(&name, badge) {
                let spot = drifter_core::dock::badge_rect(rect, size.x - size.y * 0.55);
                elements.push(Skin::Texture(element(&buffer, Rect::new(spot.origin, size), 1.0)));
            }
            if let (Some(progress), Some(plain)) = (badge.progress, self.plain.as_ref()) {
                let bar = drifter_core::dock::progress_rect(rect);
                let filled = Rect::new(bar.origin, Vec2::new(bar.size.x * progress.clamp(0.0, 1.0), bar.size.y));
                elements.push(Skin::Texture(fill(plain, filled, 0.95)));
                elements.push(Skin::Texture(fill(plain, bar, 0.3)));
            }
        }

        for (index, app) in apps.iter().enumerate() {
            let Some(slot) = slot_of(index) else {
                continue;
            };
            let lift = self.bounce_of(&app.key);
            let rect = self.dock.slot_rect(band, count, hidden.len(), Slot::Pinned(slot));
            let rect = Rect::new(rect.origin + Vec2::new(0.0, lift), rect.size);
            if drifter_core::launchpad::is_button(&app.launch) {
                let hot = self.dock.at(band, self.pointer, count, hidden.len()) == Some(Slot::Pinned(slot));
                if let Some((buffer, shown)) = self.dock_logo_frame(rect, hot) {
                    elements.push(Skin::Texture(element(&buffer, shown, 1.0)));
                    continue;
                }
            }
            if let Some(buffer) = self.dock_icon(app) {
                elements.push(Skin::Texture(element(&buffer, rect, 1.0)));
            }
        }
        for (index, window) in hidden.iter().enumerate() {
            if let Some(buffer) = self.app_icons.get(window.as_str()) {
                let rect = self
                    .dock
                    .slot_rect(band, count, hidden.len(), Slot::Minimized(index));
                elements.push(Skin::Texture(element(buffer, rect, 1.0)));
            }
        }
        if let Some(buffer) = self.dot.as_ref() {
            for (index, app) in apps.iter().enumerate() {
                let Some(slot) = slot_of(index) else {
                    continue;
                };
                if app.place.is_none() && !self.windows_of(&app.key).is_empty() {
                    let rect = self
                        .dock
                        .dot_rect(band, count, hidden.len(), Slot::Pinned(slot));
                    elements.push(Skin::Texture(element(buffer, rect, 1.0)));
                }
            }
        }
        if let (true, Some(buffer)) = (self.dock.settings, self.cog.as_ref()) {
            let rect = self.dock.slot_rect(band, count, hidden.len(), Slot::Settings);
            elements.push(Skin::Texture(element(buffer, rect, 1.0)));
            let open = self
                .windows
                .iter()
                .any(|window| window.placed && !window.going && window.app == SETTINGS_APP);
            if let (true, Some(dot)) = (open, self.dot.as_ref()) {
                let spot = self.dock.dot_rect(band, count, hidden.len(), Slot::Settings);
                elements.push(Skin::Texture(element(dot, spot, 1.0)));
            }
        }
        if self.dock.trash {
            let art = match self.trash_full {
                true => self.full_trash(),
                false => self.trash.clone(),
            };
            if let Some(buffer) = art {
                let rect = self.dock.slot_rect(band, count, hidden.len(), Slot::Trash);
                elements.push(Skin::Texture(element(&buffer, rect, 1.0)));
            }
        }
        if let Some(buffer) = self.rule.as_ref() {
            for rule in self.dock.dividers(band, count, hidden.len()) {
                elements.push(Skin::Texture(fill(buffer, rule, 1.0)));
            }
        }
    }

    fn dock_icon(&self, app: &DockApp) -> Option<TextureBuffer<Tex>> {
        if app.pinned
            && let Some((_, Some(buffer))) = self
                .pinned
                .iter()
                .find(|(launch, _)| *launch == app.launch)
        {
            return Some(buffer.clone());
        }
        self.app_icons.get(&app.key).cloned()
    }

    fn full_trash(&mut self) -> Option<TextureBuffer<Tex>> {
        if self.trash_full_art.is_none() {
            let side = self.dock.icon as u32;
            let themed = drifter_core::icon::find("user-trash-full", None, dense(side))
                .and_then(|path| drifter_core::image::load_sized(&path, dense(side)).ok())
                .map(|image| image.scaled(dense(side), dense(side)));
            let from_art = themed.or_else(|| {
                self.dock_art_dir
                    .as_ref()
                    .and_then(|dir| drifter_core::image::load(&dir.join("trash-full.png")).ok())
                    .map(|image| image.scaled(dense(side), dense(side)))
            });
            let dock = drifter_core::dock::Dock::themed(&self.palette);
            let image = from_art
                .unwrap_or_else(|| drifter_core::dock::paint_trash_full(side, dock.rule_ink()));
            self.trash_full_art = upload(&mut self.renderer, Some(&image), "full trash").ok().flatten();
        }
        self.trash_full_art.clone()
    }

    pub(super) fn dock_tip(&self) -> Option<(String, Rect)> {
        let band = self.dock_band()?;
        if self.dock_drag.is_some() {
            return None;
        }
        let hidden = self.minimized();
        let apps = self.dock_apps();
        let slot = self.dock.at(band, self.pointer, apps.len(), hidden.len())?;
        let rect = self.dock.slot_rect(band, apps.len(), hidden.len(), slot);
        let text = match slot {
            Slot::Pinned(index) => {
                let launch = &apps.get(index)?.launch;
                launch
                    .title()
                    .map(str::to_string)
                    .or_else(|| launch.app().and_then(crate::apps::find).and_then(|entry| entry.name))
                    .unwrap_or_else(|| launch.name())
            }
            Slot::Minimized(index) => hidden.get(index)?.clone(),
            Slot::Settings => drifter_core::lang::t("Settings").to_string(),
            Slot::Trash => drifter_core::lang::t("Trash").to_string(),
        };
        Some((text, rect))
    }
}

pub(super) fn trash_files() -> PathBuf {
    Config::data_dir().join("Trash/files")
}

fn empty_trash() {
    let base = Config::data_dir().join("Trash");
    for part in ["files", "info"] {
        let folder = base.join(part);
        let Ok(entries) = std::fs::read_dir(&folder) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let gone = match path.is_dir() && !path.is_symlink() {
                true => std::fs::remove_dir_all(&path),
                false => std::fs::remove_file(&path),
            };
            if let Err(error) = gone {
                log!("drifter: cannot empty the trash: {}: {error}", path.display());
            }
        }
    }
}
