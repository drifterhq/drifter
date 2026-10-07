use super::*;
use drifter_core::cascade::{Cascade, Entry, Key, Kind, Outcome};
use drifter_core::power::Action;

const OPEN_DELAY: Duration = Duration::from_millis(140);
const PANEL_FADE: f64 = 0.12;

#[derive(Clone)]
pub(super) enum Pick {
    App(String),
    Wallpaper,
    Settings,
    Power(Action),
}

pub(super) struct DeskMenu {
    pub menu: Cascade,
    pub picks: Vec<Pick>,
    pub arts: Vec<(TextureBuffer<Tex>, Rect)>,
    pub keys: Vec<LevelKey>,
    pub opened: Vec<Instant>,
    pub pending: Option<(usize, usize, Instant)>,
    pub lane: worker::Lane,
}

type LevelKey = (usize, usize, Option<usize>, u64, [i64; 4]);

pub(super) struct Ripple {
    pub fresh: std::sync::Arc<std::sync::Mutex<Option<drifter_core::Image>>>,
    pub area: Rect,
    pub since: Instant,
    pub art: Option<TextureBuffer<Tex>>,
}

pub(super) struct Alert {
    pub action: Action,
    pub ticked: bool,
    pub since: Instant,
    pub shown: u64,
    pub art: Option<TextureBuffer<Tex>>,
    pub rect: Rect,
    pub lane: worker::Lane,
}

impl State {
    fn cascade_look(&self) -> drifter_core::cascade::Look {
        let plain = drifter_core::cascade::Look::themed(&self.palette, self.look.accent);
        drifter_core::cascade::Look {
            back_alpha: self.veil(plain.back_alpha),
            ..plain
        }
    }

    fn small_icon(&mut self, name: &str) -> Option<drifter_core::Image> {
        self.icon_now(name, 18)
    }

    pub(super) fn refresh_desk_icons(&mut self) {
        if self.desk_menu.is_none() {
            return;
        }
        let mut picks = Vec::new();
        let entries = self.desk_entries(&mut picks);
        if let Some(open) = self.desk_menu.as_mut() {
            open.menu.root = entries;
            open.picks = picks;
            open.keys.clear();
        }
        self.paint_desk_menu();
    }

    fn desk_entries(&mut self, picks: &mut Vec<Pick>) -> Vec<Entry> {
        let pick = |picks: &mut Vec<Pick>, what: Pick| {
            picks.push(what);
            (picks.len() - 1) as u32
        };
        let installed = self.installed_apps();
        let mut groups = Vec::new();
        for (name, members) in drifter_core::desktop::grouped(&installed) {
            let mut apps = Vec::new();
            for app in members {
                let icon = app.icon.clone().and_then(|icon| self.small_icon(&icon));
                let id = pick(picks, Pick::App(app.id.clone()));
                apps.push(Entry::action(id, &app.name).with_icon(icon));
            }
            groups.push(Entry::submenu(name, apps));
        }
        vec![
            Entry::submenu("Applications", groups),
            Entry::action(pick(picks, Pick::Wallpaper), "Change Wallpaper…"),
            Entry::rule(),
            Entry::action(pick(picks, Pick::Settings), "Settings…"),
            Entry::rule(),
            Entry::action(pick(picks, Pick::Power(Action::Lock)), "Lock Screen"),
            Entry::action(pick(picks, Pick::Power(Action::Logout)), "Log Out…"),
            Entry::rule(),
            Entry::action(pick(picks, Pick::Power(Action::Suspend)), "Sleep"),
            Entry::action(pick(picks, Pick::Power(Action::Reboot)), "Restart…"),
            Entry::action(pick(picks, Pick::Power(Action::Shutdown)), "Shut Down…"),
        ]
    }

    pub(super) fn open_desk_menu(&mut self, at: Vec2) {
        self.popover = None;
        self.control = None;
        self.notes_panel = None;
        self.start_ripple(at);
        let mut picks = Vec::new();
        let entries = self.desk_entries(&mut picks);
        self.desk_menu = Some(DeskMenu {
            menu: Cascade::new(entries, at + Vec2::new(4.0, 4.0)),
            picks,
            arts: Vec::new(),
            keys: Vec::new(),
            opened: vec![Instant::now()],
            pending: None,
            lane: worker::Lane::default(),
        });
        self.paint_desk_menu();
    }

    fn start_ripple(&mut self, at: Vec2) {
        let Some(paper) = self.wallpaper.as_ref() else {
            return;
        };
        let Some((laid, _)) = self.laid_paper() else {
            return;
        };
        let area = drifter_core::ripple::region(at);
        let pyramid = paper.pyramid.clone();
        let since = Instant::now();
        let fresh = std::sync::Arc::new(std::sync::Mutex::new(None));
        let out = std::sync::Arc::downgrade(&fresh);
        let spawned = std::thread::Builder::new().name("ripple".into()).spawn(move || {
            let patch = drifter_core::ripple::capture(&pyramid, &laid, area);
            let mut water = drifter_core::ripple::Water::new();
            loop {
                let t = since.elapsed().as_secs_f64() / drifter_core::ripple::SECONDS;
                if t >= 1.0 {
                    return;
                }
                water.advance(t);
                let image = water.render(&patch, t);
                let Some(slot) = out.upgrade() else {
                    return;
                };
                if let Ok(mut slot) = slot.lock() {
                    *slot = Some(image);
                }
            }
        });
        if spawned.is_ok() {
            self.ripple = Some(Ripple {
                fresh,
                area,
                since,
                art: None,
            });
        }
    }

    pub(super) fn wind_the_ripple(&mut self) {
        let Some(ripple) = self.ripple.as_mut() else {
            return;
        };
        let t = ripple.since.elapsed().as_secs_f64() / drifter_core::ripple::SECONDS;
        if t >= 1.0 {
            self.ripple = None;
            return;
        }
        let Some(image) = ripple.fresh.lock().ok().and_then(|mut slot| slot.take()) else {
            return;
        };
        let art = upload_flat(&mut self.renderer, Some(&image), "ripple").ok().flatten();
        if let Some(ripple) = self.ripple.as_mut()
            && art.is_some()
        {
            ripple.art = art;
        }
    }

    pub(super) fn paint_desk_menu(&mut self) {
        let Some(font) = self.font.clone() else {
            return;
        };
        let look = self.cascade_look();
        let screen = self.screen;
        let frosting = self.frosting();
        let Some(open) = self.desk_menu.as_mut() else {
            return;
        };
        if !open.lane.claim() {
            return;
        }
        let id = open.lane.id();
        let menu = open.menu.clone();
        let had = open.keys.clone();
        self.on_worker(
            "desktop menu",
            move || {
                let rects = menu.panels(&look, &font, screen);
                rects
                    .iter()
                    .enumerate()
                    .map(|(level, rect)| {
                        let items = menu.items(level);
                        let key: LevelKey = (
                            level,
                            items.len(),
                            menu.hover[level],
                            menu.scroll[level].to_bits(),
                            [rect.origin.x, rect.origin.y, rect.size.x, rect.size.y].map(|v| v.round() as i64),
                        );
                        let image = (had.get(level) != Some(&key)).then(|| {
                            let mut image = look.paint(items, rect.size, menu.hover[level], menu.scroll[level], &font);
                            worker::frost(&frosting, &mut image, rect.origin, look.back_alpha);
                            image
                        });
                        (key, image, *rect)
                    })
                    .collect::<Vec<_>>()
            },
            move |state, levels| {
                let Some(open) = state.desk_menu.as_mut().filter(|open| open.lane.id() == id) else {
                    return;
                };
                let rerun = open.lane.release();
                let held: Vec<(LevelKey, (TextureBuffer<Tex>, Rect))> =
                    open.keys.iter().cloned().zip(open.arts.iter().cloned()).collect();
                let mut arts = Vec::new();
                let mut keys = Vec::new();
                for (key, image, rect) in levels {
                    let art = match image {
                        Some(image) => upload(&mut state.renderer, Some(&image), "desktop menu")
                            .ok()
                            .flatten()
                            .map(|art| (art, rect)),
                        None => held.iter().find(|(had, _)| *had == key).map(|(_, art)| art.clone()),
                    };
                    if let Some(art) = art {
                        arts.push(art);
                        keys.push(key);
                    }
                }
                if let Some(open) = state.desk_menu.as_mut() {
                    open.keys = keys;
                    let levels = open.menu.levels();
                    open.opened.truncate(levels);
                    while open.opened.len() < levels {
                        open.opened.push(Instant::now());
                    }
                    open.arts = arts;
                }
                if rerun {
                    state.paint_desk_menu();
                }
                state.kick_now();
            },
        );
    }

    pub(super) fn desk_menu_elements(&self, elements: &mut Vec<Skin>) {
        let Some(open) = self.desk_menu.as_ref() else {
            return;
        };
        for (level, (art, rect)) in open.arts.iter().enumerate().rev() {
            let shown = open
                .opened
                .get(level)
                .map(|at| (at.elapsed().as_secs_f64() / PANEL_FADE).min(1.0))
                .unwrap_or(1.0);
            let drop = Rect::new(rect.origin + Vec2::new(0.0, 6.0 * (1.0 - shown)), rect.size);
            elements.push(Skin::Texture(element(art, drop, shown as f32)));
            if let Some((buffer, corner)) = self.window_shadow.as_ref() {
                for (source, target) in drifter_core::window::shadow_slices(drop, *corner, 0.6, 6.0) {
                    elements.push(Skin::Texture(faded(buffer, source, target, 0.3 * shown as f32)));
                }
            }
        }
    }

    pub(super) fn desk_menu_busy(&self) -> bool {
        self.ripple.is_some()
            || self.desk_menu.as_ref().is_some_and(|open| {
                open.pending.is_some()
                    || open
                        .opened
                        .iter()
                        .any(|at| at.elapsed().as_secs_f64() < PANEL_FADE)
            })
    }

    pub(super) fn hover_desk_menu(&mut self) {
        let Some(font) = self.font.as_deref() else {
            return;
        };
        let look = self.cascade_look();
        let screen = self.screen;
        let pointer = self.pointer;
        let Some(open) = self.desk_menu.as_mut() else {
            return;
        };
        let Some((level, index)) = open.menu.hit(&look, font, screen, pointer) else {
            return;
        };
        let mut changed = open.menu.point(level, index);
        if level + 1 < open.menu.levels() && open.menu.path.get(level).copied() == index {
            open.pending = None;
        } else {
            match index {
                Some(at) => {
                    let opens = matches!(open.menu.items(level)[at].kind, Kind::Submenu(_));
                    let deeper = level + 1 < open.menu.levels();
                    if opens || deeper {
                        if open.pending.map(|(l, i, _)| (l, i)) != Some((level, at)) {
                            open.pending = Some((level, at, Instant::now()));
                        }
                    } else {
                        open.pending = None;
                    }
                }
                None => open.pending = None,
            }
        }
        if level + 1 < open.menu.levels() {
            changed |= open.menu.point(level + 1, open.menu.hover[level + 1]);
        }
        if changed {
            self.paint_desk_menu();
        }
    }

    pub(super) fn wind_the_desk_menu(&mut self) {
        let due = self
            .desk_menu
            .as_ref()
            .and_then(|open| open.pending)
            .filter(|(_, _, since)| since.elapsed() >= OPEN_DELAY);
        if let Some((level, index, _)) = due
            && let Some(open) = self.desk_menu.as_mut()
        {
            open.pending = None;
            if open.menu.hover.get(level).copied().flatten() == Some(index) {
                open.menu.open(level, index);
                self.paint_desk_menu();
            }
        }
    }

    pub(super) fn press_desk_menu(&mut self) -> bool {
        if self.alert.is_some() {
            return self.press_alert();
        }
        let Some(font) = self.font.as_deref() else {
            return false;
        };
        let look = self.cascade_look();
        let screen = self.screen;
        let pointer = self.pointer;
        let Some(open) = self.desk_menu.as_mut() else {
            return false;
        };
        match open.menu.hit(&look, font, screen, pointer) {
            None => {
                self.desk_menu = None;
                self.kick_now();
                false
            }
            Some((_, None)) => true,
            Some((level, Some(at))) => {
                match open.menu.items(level)[at].kind.clone() {
                    Kind::Submenu(_) => {
                        open.pending = None;
                        open.menu.open(level, at);
                        self.paint_desk_menu();
                    }
                    Kind::Action(id) => self.activate_desk(id),
                    _ => {}
                }
                true
            }
        }
    }

    pub(super) fn scroll_desk_menu(&mut self, steps: f64) -> bool {
        let Some(font) = self.font.as_deref() else {
            return false;
        };
        let look = self.cascade_look();
        let screen = self.screen;
        let pointer = self.pointer;
        let Some(open) = self.desk_menu.as_mut() else {
            return false;
        };
        let Some((level, _)) = open.menu.hit(&look, font, screen, pointer) else {
            return true;
        };
        if open.menu.scroll_by(&look, font, screen, level, steps * look.item * 2.0) {
            self.paint_desk_menu();
        }
        true
    }

    pub(super) fn desk_menu_key(&mut self, key: LockKey) {
        if self.alert.is_some() {
            match key {
                LockKey::Enter => self.confirm_alert(),
                LockKey::Clear => {
                    self.alert = None;
                    self.kick_now();
                }
                _ => {}
            }
            return;
        }
        let key = match key {
            LockKey::Up => Key::Up,
            LockKey::Down => Key::Down,
            LockKey::Left => Key::Left,
            LockKey::Right => Key::Right,
            LockKey::Enter => Key::Enter,
            LockKey::Clear => Key::Escape,
            LockKey::Letter(letter) => Key::Letter(letter),
            LockKey::Vt(vt) => {
                self.change_vt(vt);
                return;
            }
            LockKey::Back | LockKey::Other => return,
        };
        let Some(open) = self.desk_menu.as_mut() else {
            return;
        };
        open.pending = None;
        match open.menu.key(key) {
            Outcome::Nothing => {}
            Outcome::Changed => self.paint_desk_menu(),
            Outcome::Activate(id) => self.activate_desk(id),
            Outcome::Close => {
                self.desk_menu = None;
                self.kick_now();
            }
        }
    }

    fn activate_desk(&mut self, id: u32) {
        let Some(pick) = self
            .desk_menu
            .take()
            .and_then(|open| open.picks.get(id as usize).cloned())
        else {
            return;
        };
        self.kick_now();
        match pick {
            Pick::App(app) => {
                crate::apps::run(&drifter_core::Launch::App(app.clone()), &self.socket);
                self.bounces.retain(|(held, _)| *held != app);
                self.bounces.push((app, Instant::now()));
            }
            Pick::Wallpaper => self.open_settings_at("wallpaper"),
            Pick::Settings => self.open_settings(),
            Pick::Power(Action::Lock) => {
                self.lock_now();
            }
            Pick::Power(Action::Suspend) => {
                if self.lock_now() {
                    crate::power::run(Action::Suspend);
                }
            }
            Pick::Power(action) => self.ask_alert(action),
        }
    }

    fn alert_look(&self) -> drifter_core::alert::Look {
        let plain = drifter_core::alert::Look::themed(&self.palette, self.look.accent).with_tick();
        drifter_core::alert::Look {
            back_alpha: self.veil(plain.back_alpha),
            ..plain
        }
    }

    pub(super) fn ask_alert(&mut self, action: Action) {
        let look = self.alert_look();
        let since = Instant::now();
        self.alert = Some(Alert {
            action,
            ticked: drifter_core::reopen::wanted(),
            since,
            shown: drifter_core::power::COUNTDOWN,
            art: None,
            rect: look.rect(self.screen),
            lane: worker::Lane::default(),
        });
        self.paint_alert();
        let _ = self.handle.insert_source(
            Timer::from_duration(Duration::from_millis(250)),
            move |_, _, state: &mut State| match state.tick_alert(since) {
                true => TimeoutAction::ToDuration(Duration::from_millis(250)),
                false => TimeoutAction::Drop,
            },
        );
    }

    fn tick_alert(&mut self, since: Instant) -> bool {
        let Some(open) = self.alert.as_mut().filter(|open| open.since == since) else {
            return false;
        };
        let left = drifter_core::power::COUNTDOWN.saturating_sub(since.elapsed().as_secs());
        if left == 0 {
            self.confirm_alert();
            return false;
        }
        if left != open.shown {
            open.shown = left;
            self.paint_alert();
            self.kick_now();
        }
        true
    }

    fn paint_alert(&mut self) {
        let (Some(font), Some(bold)) = (self.font.clone(), self.search_font.clone()) else {
            self.alert = None;
            return;
        };
        let Some(open) = self.alert.as_ref() else {
            return;
        };
        let (title, verb) = match open.action {
            Action::Logout => ("Log out now?", "Log Out"),
            Action::Reboot => ("Restart now?", "Restart"),
            _ => ("Shut down now?", "Shut Down"),
        };
        let message = open.action.countdown(open.shown);
        let ticked = open.ticked;
        let rect = open.rect;
        let look = self.alert_look();
        let frosting = self.frosting();
        self.paint_in_lane(
            "alert",
            |state| state.alert.as_mut().map(|open| &mut open.lane),
            |_| {},
            move || {
                let mut image = look.paint(title, &message, verb, ticked, &font, &bold);
                worker::frost(&frosting, &mut image, rect.origin, look.back_alpha);
                Some(image)
            },
            |state, art| {
                if let Some(open) = state.alert.as_mut() {
                    open.art = Some(art);
                }
            },
        );
    }

    fn press_alert(&mut self) -> bool {
        let Some(open) = self.alert.as_ref() else {
            return false;
        };
        let look = self.alert_look();
        match look.hit(self.pointer - open.rect.origin) {
            Some(drifter_core::alert::Press::Confirm) => self.confirm_alert(),
            Some(drifter_core::alert::Press::Tick) => {
                if let Some(open) = self.alert.as_mut() {
                    open.ticked = !open.ticked;
                }
                self.paint_alert();
                self.kick_now();
            }
            Some(drifter_core::alert::Press::Cancel) => {
                self.alert = None;
                self.kick_now();
            }
            None => {}
        }
        true
    }

    fn confirm_alert(&mut self) {
        let Some(open) = self.alert.take() else {
            return;
        };
        self.kick_now();
        drifter_core::reopen::want(open.ticked);
        if open.ticked {
            self.save_desk();
        }
        match open.action {
            Action::Logout => self.signal.stop(),
            action => crate::power::run(action),
        }
    }

    pub(super) fn alert_elements(&self, elements: &mut Vec<Skin>) {
        if let Some(open) = self.alert.as_ref()
            && let Some(art) = open.art.as_ref()
        {
            elements.push(Skin::Texture(element(art, open.rect, 1.0)));
            self.modal_shadow(open.rect, elements);
        }
    }
}
