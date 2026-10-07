use super::*;
use drifter_core::search::Found;

const PAD_OPEN: i32 = 1;
const PAD_PIN: i32 = 2;

pub(super) struct Pad {
    pub query: String,
    pub all: Vec<Found>,
    pub shown: Vec<Found>,
    pub page: usize,
    pub base: drifter_core::Image,
    pub rect: Rect,
    pub art: Option<TextureBuffer<Tex>>,
}

impl State {
    pub(super) fn toggle_launchpad(&mut self) {
        if self.launchpad.take().is_some() {
            self.kick_now();
            return;
        }
        self.popover = None;
        self.search = None;
        self.control = None;
        self.notes_panel = None;
        let mut all = drifter_core::search::apps(&self.installed_apps());
        all.push(welcome_found());
        all.sort_by_key(|found| found.title.to_lowercase());
        let look = drifter_core::launchpad::Look::themed(&self.palette);
        let rect = look.rect(self.screen);
        let veil = self.veil(0.97);
        let mut canvas = drifter_core::paint::Canvas::new(rect.size.x as u32, rect.size.y as u32);
        canvas.round_rect(Rect::new(Vec2::ZERO, rect.size), look.radius, look.back, veil);
        let mut base = canvas.image;
        self.frosted(&mut base, rect.origin, veil);
        self.launchpad = Some(Pad {
            query: String::new(),
            shown: all.clone(),
            all,
            page: 0,
            base,
            rect,
            art: None,
        });
        self.paint_launchpad();
    }

    fn pad_icon(&mut self, found: &Found, side: u32) -> Option<drifter_core::Image> {
        if found.key == super::welcome::WELCOME_KEY {
            return Some(drifter_core::welcome::icon(side));
        }
        let name = found.icon.clone()?;
        let key = format!("{name}@{side}");
        if let Some(held) = self.search_icons.get(&key) {
            return held.clone();
        }
        let image = drifter_core::icon::find(&name, None, dense(side))
            .and_then(|path| drifter_core::image::load_sized(&path, dense(side)).ok())
            .map(|image| image.scaled(dense(side), dense(side)));
        self.search_icons.insert(key, image.clone());
        image
    }

    pub(super) fn paint_launchpad(&mut self) {
        if self.font.is_none() {
            return;
        }
        let look = drifter_core::launchpad::Look::themed(&self.palette);
        let side = look.icon as u32;
        let Some(pad) = self.launchpad.as_ref() else {
            return;
        };
        let size = pad.rect.size;
        let per = look.per_page(size);
        let visible: Vec<Found> = pad.shown.iter().skip(pad.page * per).take(per).cloned().collect();
        let skipped = pad.page * per;
        let total = pad.shown.len();
        let mut apps: Vec<(String, Option<drifter_core::Image>)> =
            vec![(String::new(), None); skipped];
        for found in &visible {
            let icon = self.pad_icon(found, side);
            apps.push((found.title.clone(), icon));
        }
        apps.resize(total, (String::new(), None));
        let (Some(font), Some(big), Some(pad)) = (
            self.font.as_ref(),
            self.search_font.as_ref(),
            self.launchpad.as_ref(),
        ) else {
            return;
        };
        let image = look.paint(&pad.base, &pad.query, &apps, pad.page, font, big);
        let art = self.upload_again(Some(&image), "apps").ok().flatten();
        if let Some(pad) = self.launchpad.as_mut() {
            pad.art = art;
        }
        self.kick_now();
    }

    pub(super) fn launchpad_key(&mut self, key: LockKey) {
        let look = drifter_core::launchpad::Look::themed(&self.palette);
        let Some(pad) = self.launchpad.as_mut() else {
            return;
        };
        let pages = look.pages(pad.rect.size, pad.shown.len());
        match key {
            LockKey::Letter(letter) => {
                pad.query.push(letter);
                pad.shown = drifter_core::search::rank_all(&pad.query, &pad.all);
                pad.page = 0;
            }
            LockKey::Back => {
                pad.query.pop();
                pad.shown = match pad.query.is_empty() {
                    true => pad.all.clone(),
                    false => drifter_core::search::rank_all(&pad.query, &pad.all),
                };
                pad.page = 0;
            }
            LockKey::Down | LockKey::Right => pad.page = (pad.page + 1).min(pages - 1),
            LockKey::Up | LockKey::Left => pad.page = pad.page.saturating_sub(1),
            LockKey::Enter => {
                let first = pad.shown.first().cloned();
                self.launchpad = None;
                if let Some(found) = first {
                    self.open_app(&found);
                }
                self.kick_now();
                return;
            }
            LockKey::Clear => {
                self.launchpad = None;
                self.kick_now();
                return;
            }
            LockKey::Vt(vt) => {
                self.change_vt(vt);
                return;
            }
            LockKey::Other => return,
        }
        self.paint_launchpad();
    }

    pub(super) fn press_launchpad(&mut self, code: u32) -> bool {
        let look = drifter_core::launchpad::Look::themed(&self.palette);
        let Some(pad) = self.launchpad.as_ref() else {
            return false;
        };
        let size = pad.rect.size;
        let inside = self.pointer - pad.rect.origin;
        let per = look.per_page(size);
        let on_page = pad.shown.len().saturating_sub(pad.page * per);
        let chosen = look
            .slot_at(size, inside, on_page)
            .and_then(|slot| pad.shown.get(pad.page * per + slot).cloned());
        if pad.rect.contains(self.pointer) && chosen.is_none() {
            return true;
        }
        if code == BTN_RIGHT {
            if let Some(found) = chosen {
                let pinned = self.pinned_app(&found.key);
                let items = vec![
                    Item::line(PAD_OPEN, "Open"),
                    Item::rule(9),
                    Item::line(
                        PAD_PIN,
                        match pinned {
                            true => "Remove from Dock",
                            false => "Keep in Dock",
                        },
                    ),
                ];
                let at = self.pointer;
                self.open_popover(items, at, Target::PadApp(found.key));
            }
            return true;
        }
        self.launchpad = None;
        if let Some(found) = chosen {
            self.open_app(&found);
        }
        self.kick_now();
        true
    }

    pub(super) fn scroll_launchpad(&mut self, steps: f64) -> bool {
        let look = drifter_core::launchpad::Look::themed(&self.palette);
        let Some(pad) = self.launchpad.as_mut() else {
            return false;
        };
        if steps.abs() < 0.5 {
            return true;
        }
        let pages = look.pages(pad.rect.size, pad.shown.len());
        let before = pad.page;
        pad.page = match steps > 0.0 {
            true => (pad.page + 1).min(pages - 1),
            false => pad.page.saturating_sub(1),
        };
        if pad.page != before {
            self.paint_launchpad();
        }
        true
    }

    fn pinned_app(&self, key: &str) -> bool {
        self.dock_apps()
            .iter()
            .any(|app| app.pinned && app.place.is_none() && drifter_core::dock::same_app(&app.key, key))
    }

    pub(super) fn pad_menu_pick(&mut self, key: &str, id: i32) {
        match id {
            PAD_OPEN => {
                let found = self
                    .launchpad
                    .as_ref()
                    .and_then(|pad| pad.all.iter().find(|found| found.key == key).cloned());
                self.launchpad = None;
                if let Some(found) = found {
                    self.open_app(&found);
                }
                self.kick_now();
            }
            PAD_PIN => {
                let apps = self.dock_apps();
                match self.pinned_app(key) {
                    true => {
                        if let Some(app) = apps
                            .iter()
                            .find(|app| app.pinned && drifter_core::dock::same_app(&app.key, key))
                        {
                            let held = app.key.clone();
                            self.dock_menu_pick(&held, 2);
                        }
                    }
                    false => {
                        let mut pinned: Vec<drifter_core::Launch> = apps
                            .iter()
                            .filter(|app| app.pinned)
                            .map(|app| app.launch.clone())
                            .collect();
                        pinned.push(drifter_core::Launch::App(key.to_string()));
                        self.save_dock(&pinned, &[]);
                    }
                }
            }
            _ => {}
        }
    }

    fn open_app(&mut self, found: &Found) {
        if found.key == super::welcome::WELCOME_KEY {
            self.open_welcome(false);
            return;
        }
        let launch = drifter_core::Launch::App(found.key.clone());
        crate::apps::run(&launch, &self.socket);
        self.bounces.retain(|(held, _)| held != &found.key);
        self.bounces.push((found.key.clone(), Instant::now()));
    }
}

pub(super) fn welcome_found() -> Found {
    Found {
        kind: drifter_core::search::Kind::App,
        title: "Welcome".to_string(),
        detail: "Application".to_string(),
        key: super::welcome::WELCOME_KEY.to_string(),
        extra: "welcome tour start setup hello drifter".to_string(),
        icon: None,
    }
}
