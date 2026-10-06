use super::*;
use drifter_core::search::Found;

pub(super) struct Pad {
    pub query: String,
    pub all: Vec<Found>,
    pub shown: Vec<Found>,
    pub page: usize,
    pub base: drifter_core::Image,
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
        let mut all = drifter_core::search::apps(&drifter_core::desktop::installed());
        all.sort_by_key(|found| found.title.to_lowercase());
        let width = self.screen.x.max(1.0) as u32;
        let height = self.screen.y.max(1.0) as u32;
        let look = drifter_core::launchpad::Look::themed(&self.palette);
        let mut base = drifter_core::Image {
            width,
            height,
            rgba: vec![0; (width * height * 4) as usize],
        };
        let veil = self.veil(0.9) * 0.85;
        for pixel in base.rgba.chunks_mut(4) {
            for (channel, value) in look.veil.iter().enumerate() {
                pixel[channel] = (f64::from(*value) * veil).round() as u8;
            }
            pixel[3] = (veil * 255.0).round() as u8;
        }
        self.frosted(&mut base, Vec2::ZERO, veil);
        self.launchpad = Some(Pad {
            query: String::new(),
            shown: all.clone(),
            all,
            page: 0,
            base,
            art: None,
        });
        self.paint_launchpad();
    }

    fn pad_icon(&mut self, found: &Found, side: u32) -> Option<drifter_core::Image> {
        let name = found.icon.clone()?;
        let key = format!("{name}@{side}");
        if let Some(held) = self.search_icons.get(&key) {
            return held.clone();
        }
        let image = drifter_core::icon::find(&name, None, side)
            .and_then(|path| drifter_core::image::load_sized(&path, side).ok())
            .map(|image| image.scaled(side, side));
        self.search_icons.insert(key, image.clone());
        image
    }

    pub(super) fn paint_launchpad(&mut self) {
        if self.font.is_none() {
            return;
        }
        let look = drifter_core::launchpad::Look::themed(&self.palette);
        let screen = self.screen;
        let side = look.icon as u32;
        let Some(pad) = self.launchpad.as_ref() else {
            return;
        };
        let per = look.per_page(screen);
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
        let (Some(font), Some(pad)) = (self.font.as_ref(), self.launchpad.as_ref()) else {
            return;
        };
        let image = look.paint(&pad.base, screen, &pad.query, &apps, pad.page, font);
        let art = upload(&mut self.renderer, Some(&image), "apps").ok().flatten();
        if let Some(pad) = self.launchpad.as_mut() {
            pad.art = art;
        }
        self.kick_now();
    }

    pub(super) fn launchpad_key(&mut self, key: LockKey) {
        let look = drifter_core::launchpad::Look::themed(&self.palette);
        let screen = self.screen;
        let Some(pad) = self.launchpad.as_mut() else {
            return;
        };
        let pages = look.pages(screen, pad.shown.len());
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

    pub(super) fn press_launchpad(&mut self) -> bool {
        let look = drifter_core::launchpad::Look::themed(&self.palette);
        let screen = self.screen;
        let Some(pad) = self.launchpad.as_ref() else {
            return false;
        };
        let per = look.per_page(screen);
        let on_page = pad.shown.len().saturating_sub(pad.page * per);
        let chosen = look
            .slot_at(screen, self.pointer, on_page)
            .and_then(|slot| pad.shown.get(pad.page * per + slot).cloned());
        let in_field = look.field_rect(screen).contains(self.pointer);
        if in_field {
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
        let screen = self.screen;
        let Some(pad) = self.launchpad.as_mut() else {
            return false;
        };
        if steps.abs() < 0.5 {
            return true;
        }
        let pages = look.pages(screen, pad.shown.len());
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

    fn open_app(&mut self, found: &Found) {
        let launch = drifter_core::Launch::App(found.key.clone());
        crate::apps::run(&launch, &self.socket);
        self.bounces.retain(|(held, _)| held != &found.key);
        self.bounces.push((found.key.clone(), Instant::now()));
    }
}
