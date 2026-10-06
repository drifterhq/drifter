use super::*;
use drifter_core::cascade::{Cascade, Entry, Key, Kind, Outcome};
use drifter_core::power::Action;

const OPEN_DELAY: Duration = Duration::from_millis(140);
const PANEL_FADE: f64 = 0.12;

#[derive(Clone)]
pub(super) enum Pick {
    App(String),
    Wallpaper(String),
    MoreWallpapers,
    Settings,
    Power(Action),
}

pub(super) struct DeskMenu {
    pub menu: Cascade,
    pub picks: Vec<Pick>,
    pub arts: Vec<(TextureBuffer<Tex>, Rect)>,
    pub opened: Vec<Instant>,
    pub pending: Option<(usize, usize, Instant)>,
}

pub(super) struct Ripple {
    pub patch: drifter_core::Image,
    pub area: Rect,
    pub since: Instant,
    pub art: Option<TextureBuffer<Tex>>,
}

pub(super) struct Alert {
    pub action: Action,
    pub art: TextureBuffer<Tex>,
    pub rect: Rect,
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
        let key = format!("{name}@18");
        if let Some(held) = self.search_icons.get(&key) {
            return held.clone();
        }
        let image = drifter_core::icon::find(name, None, 18)
            .and_then(|path| drifter_core::image::load_sized(&path, 18).ok())
            .map(|image| image.scaled(18, 18));
        self.search_icons.insert(key, image.clone());
        image
    }

    fn desk_entries(&mut self, picks: &mut Vec<Pick>) -> Vec<Entry> {
        let pick = |picks: &mut Vec<Pick>, what: Pick| {
            picks.push(what);
            (picks.len() - 1) as u32
        };
        let installed = drifter_core::desktop::installed();
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
        let config = Config::load_from(&self.config_path).unwrap_or_default();
        let mut papers = Vec::new();
        for path in drifter_core::livepaper::installed(&config.livepaper_folders()) {
            let Some(file) = path.file_name().map(|name| name.to_string_lossy().into_owned()) else {
                continue;
            };
            let id = pick(picks, Pick::Wallpaper(file));
            papers.push(Entry::action(id, &drifter_core::livepaper::name(&path)));
        }
        if !papers.is_empty() {
            papers.push(Entry::rule());
        }
        papers.push(Entry::action(pick(picks, Pick::MoreWallpapers), "More Wallpapers…"));
        vec![
            Entry::submenu("Applications", groups),
            Entry::submenu("Change Wallpaper", papers),
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
            opened: vec![Instant::now()],
            pending: None,
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
        let patch = drifter_core::ripple::capture(&paper.pyramid, &laid, area);
        self.ripple = Some(Ripple {
            patch,
            area,
            since: Instant::now(),
            art: None,
        });
    }

    pub(super) fn wind_the_ripple(&mut self) {
        let Some(ripple) = self.ripple.as_ref() else {
            return;
        };
        let t = ripple.since.elapsed().as_secs_f64() / drifter_core::ripple::SECONDS;
        if t >= 1.0 {
            self.ripple = None;
            return;
        }
        let image = drifter_core::ripple::frame(&ripple.patch, t);
        let art = upload(&mut self.renderer, Some(&image), "ripple").ok().flatten();
        if let Some(ripple) = self.ripple.as_mut() {
            ripple.art = art;
        }
    }

    pub(super) fn paint_desk_menu(&mut self) {
        let Some(font) = self.font.as_ref() else {
            return;
        };
        let look = self.cascade_look();
        let Some(open) = self.desk_menu.as_ref() else {
            return;
        };
        let rects = open.menu.panels(&look, font, self.screen);
        let mut images = Vec::new();
        for (level, rect) in rects.iter().enumerate() {
            let items = open.menu.items(level);
            let image = look.paint(items, rect.size, open.menu.hover[level], open.menu.scroll[level], font);
            images.push((image, *rect));
        }
        let mut arts = Vec::new();
        for (mut image, rect) in images {
            self.frosted(&mut image, rect.origin, look.back_alpha);
            if let Ok(Some(art)) = upload(&mut self.renderer, Some(&image), "desktop menu") {
                arts.push((art, rect));
            }
        }
        if let Some(open) = self.desk_menu.as_mut() {
            let levels = open.menu.levels();
            open.opened.truncate(levels);
            while open.opened.len() < levels {
                open.opened.push(Instant::now());
            }
            open.arts = arts;
        }
        self.kick_now();
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
        let Some(font) = self.font.as_ref() else {
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
        let Some(font) = self.font.as_ref() else {
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
        let Some(font) = self.font.as_ref() else {
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
            Pick::Wallpaper(file) => {
                self.save_key(|text| drifter_core::settings::write_word(text, "workspace", "wallpaper", &file));
            }
            Pick::MoreWallpapers => self.open_settings_at("wallpaper"),
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
        let plain = drifter_core::alert::Look::themed(&self.palette, self.look.accent);
        drifter_core::alert::Look {
            back_alpha: self.veil(plain.back_alpha),
            ..plain
        }
    }

    fn ask_alert(&mut self, action: Action) {
        let (Some(font), Some(bold)) = (self.font.as_ref(), self.search_font.as_ref()) else {
            return;
        };
        let (title, message, verb) = match action {
            Action::Logout => ("Log out now?", "Your apps will close.", "Log Out"),
            Action::Reboot => ("Restart now?", "Your apps will close.", "Restart"),
            _ => ("Shut down now?", "Your apps will close.", "Shut Down"),
        };
        let look = self.alert_look();
        let mut image = look.paint(title, message, verb, font, bold);
        let rect = look.rect(self.screen);
        self.frosted(&mut image, rect.origin, look.back_alpha);
        if let Ok(Some(art)) = upload(&mut self.renderer, Some(&image), "alert") {
            self.alert = Some(Alert { action, art, rect });
        }
        self.kick_now();
    }

    fn press_alert(&mut self) -> bool {
        let Some(open) = self.alert.as_ref() else {
            return false;
        };
        let look = self.alert_look();
        match look.hit(self.pointer - open.rect.origin) {
            Some(drifter_core::alert::Press::Confirm) => self.confirm_alert(),
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
        match open.action {
            Action::Logout => self.signal.stop(),
            action => crate::power::run(action),
        }
    }

    pub(super) fn alert_elements(&self, elements: &mut Vec<Skin>) {
        if let Some(open) = self.alert.as_ref() {
            elements.push(Skin::Texture(element(&open.art, open.rect, 1.0)));
            self.modal_shadow(open.rect, elements);
        }
    }
}
