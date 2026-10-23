use super::*;
use drifter_core::search::Found;

const PAD_OPEN: i32 = 1;
const PAD_PIN: i32 = 2;
const PAD_ZOOM: f64 = 0.25;
const PAD_TURN: f64 = 0.28;
const SWIPE_PAGE: f64 = 90.0;
const PAD_FROM_ZOOM: f64 = 1.1;

pub(super) struct Pad {
    pub query: String,
    pub all: Vec<Found>,
    pub shown: Vec<Found>,
    pub page: usize,
    pub rect: Rect,
    pub art: Option<TextureBuffer<Tex>>,
    pub lane: worker::Lane,
    pub opened: Option<Instant>,
    pub turn: Option<(TextureBuffer<Tex>, Instant, i8)>,
    pub turning: i8,
    pub swipe: f64,
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
        self.charges = None;
        self.glance_panel = None;
        let mut all = drifter_core::search::apps(&self.installed_apps());
        all.push(welcome_found());
        all.sort_by_key(|found| found.title.to_lowercase());
        let look = drifter_core::launchpad::Look::themed(&self.palette);
        let rect = look.rect(self.screen);
        self.launchpad = Some(Pad {
            query: String::new(),
            shown: all.clone(),
            all,
            page: 0,
            rect,
            art: None,
            lane: worker::Lane::default(),
            opened: None,
            turn: None,
            turning: 0,
            swipe: 0.0,
        });
        self.paint_launchpad();
    }

    fn pad_icon(&mut self, found: &Found, side: u32) -> Option<drifter_core::Image> {
        let name = found.icon.clone()?;
        self.icon_now(&name, side)
    }

    pub(super) fn paint_launchpad(&mut self) {
        let (Some(font), Some(big)) = (self.font.clone(), self.search_font.clone()) else {
            return;
        };
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
        let (query, page, origin) = (pad.query.clone(), pad.page, pad.rect.origin);
        let mut apps: Vec<(String, Option<drifter_core::Image>)> = vec![(String::new(), None); skipped];
        for found in &visible {
            let icon = self.pad_icon(found, side);
            apps.push((found.title.clone(), icon));
        }
        apps.resize(total, (String::new(), None));
        let veil = self.veil(0.97);
        let frosting = self.frosting();
        self.paint_in_lane(
            "apps",
            |state| state.launchpad.as_mut().map(|pad| &mut pad.lane),
            State::paint_launchpad,
            move || {
                let mut canvas = drifter_core::paint::Canvas::new(size.x as u32, size.y as u32);
                canvas.round_rect(Rect::new(Vec2::ZERO, size), look.radius, look.back, veil);
                let mut base = canvas.image;
                worker::frost(&frosting, &mut base, origin, veil);
                Some(look.paint(&base, &query, &apps, page, &font, &big))
            },
            |state, art| {
                if let Some(pad) = state.launchpad.as_mut() {
                    let old = pad.art.replace(art);
                    pad.opened.get_or_insert_with(Instant::now);
                    if let (Some(old), true) = (old, pad.turning != 0) {
                        pad.turn = Some((old, Instant::now(), pad.turning));
                    }
                    pad.turning = 0;
                }
            },
        );
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
            LockKey::Down | LockKey::Right => {
                let next = (pad.page + 1).min(pages - 1);
                pad.turning = if next != pad.page { 1 } else { pad.turning };
                pad.page = next;
            }
            LockKey::Up | LockKey::Left => {
                let back = pad.page.saturating_sub(1);
                pad.turning = if back != pad.page { -1 } else { pad.turning };
                pad.page = back;
            }
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
                    Item::line(PAD_OPEN, drifter_core::lang::t("Open")),
                    Item::rule(9),
                    Item::line(
                        PAD_PIN,
                        match pinned {
                            true => drifter_core::lang::t("Remove from Dock"),
                            false => drifter_core::lang::t("Keep in Dock"),
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
            pad.turning = if pad.page > before { 1 } else { -1 };
            self.paint_launchpad();
        }
        true
    }

    pub(super) fn swipe_launchpad(&mut self, moved: f64, done: bool) -> bool {
        let look = drifter_core::launchpad::Look::themed(&self.palette);
        let Some(pad) = self.launchpad.as_mut() else {
            return false;
        };
        let pages = look.pages(pad.rect.size, pad.shown.len());
        let before = pad.page;
        let (page, left) = drifter_core::launchpad::page_after_swipe(pad.page, pages, pad.swipe + moved, SWIPE_PAGE);
        pad.page = page;
        pad.swipe = if done { 0.0 } else { left };
        if pad.page != before {
            pad.turning = if pad.page > before { 1 } else { -1 };
            self.paint_launchpad();
        }
        true
    }

    pub(super) fn pad_moving(&self) -> bool {
        let Some(pad) = self.launchpad.as_ref() else {
            return false;
        };
        let pace = self.window_pace;
        pace > 0.0
            && (pad.opened.is_some_and(|at| at.elapsed().as_secs_f64() < PAD_ZOOM / pace)
                || pad.turn.as_ref().is_some_and(|(_, at, _)| at.elapsed().as_secs_f64() < PAD_TURN / pace))
    }

    pub(super) fn launchpad_layers(&self) -> Option<(Vec<super::fade::Layer>, Rect)> {
        use super::fade::{Layer, Shadow};
        let pad = self.launchpad.as_ref()?;
        let art = pad.art.as_ref()?;
        let look = drifter_core::launchpad::Look::themed(&self.palette);
        let size = pad.rect.size;
        let origin = pad.rect.origin;
        let band = look.grid_band(size);
        let pace = self.window_pace;
        let still = pace <= 0.0 || self.window_still;
        let crop = |art: &TextureBuffer<Tex>, source: Rect, target: Rect, alpha: f32| Layer {
            source: Some(source),
            alpha,
            ..Layer::new(art, Rect::new(origin + target.origin, target.size), Shadow::None)
        };
        let mut layers = Vec::new();
        let turn = pad
            .turn
            .as_ref()
            .filter(|_| pace > 0.0)
            .map(|(old, at, way)| (old, (at.elapsed().as_secs_f64() / (PAD_TURN / pace)).min(1.0), *way))
            .filter(|(_, t, _)| *t < 1.0);
        match turn {
            Some((old, t, way)) if !still => {
                let moved = band.size.x * drifter_core::motion::smooth(t);
                let [from, to] = drifter_core::launchpad::slide_crops(band.size.x, moved, way > 0);
                for (piece, (sx, dx, w)) in [(old, from), (art, to)] {
                    if w > 0.0 {
                        let source = Rect::new(Vec2::new(sx, band.origin.y), Vec2::new(w, band.size.y));
                        let target = Rect::new(Vec2::new(dx, band.origin.y), source.size);
                        layers.push(crop(piece, source, target, 1.0));
                    }
                }
            }
            Some((old, t, _)) => {
                let fade = drifter_core::motion::smooth(t) as f32;
                layers.push(crop(old, band, band, 1.0 - fade));
                layers.push(crop(art, band, band, 1.0));
            }
            None => {
                let t = match (still, pad.opened) {
                    (false, Some(at)) => at.elapsed().as_secs_f64() / (PAD_ZOOM / pace),
                    _ => 1.0,
                };
                let zoom = PAD_FROM_ZOOM - (PAD_FROM_ZOOM - 1.0) * drifter_core::motion::smooth(t);
                let source = Rect::from_center(band.center(), band.size / zoom);
                layers.push(crop(art, source, band, 1.0));
            }
        }
        let top = Rect::new(Vec2::ZERO, Vec2::new(size.x, band.origin.y));
        let bottom = Rect::new(Vec2::new(0.0, band.max().y), Vec2::new(size.x, size.y - band.max().y));
        for part in [top, bottom] {
            layers.push(crop(art, part, part, 1.0));
        }
        if let Some(first) = layers.first_mut() {
            first.shadow = Shadow::Modal;
            first.shade = Some(pad.rect);
        }
        Some((layers, pad.rect))
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
        self.bounce(&found.key);
    }
}

pub(super) fn welcome_found() -> Found {
    Found {
        kind: drifter_core::search::Kind::App,
        title: drifter_core::lang::t("Welcome").to_string(),
        detail: drifter_core::lang::t("Application").to_string(),
        key: super::welcome::WELCOME_KEY.to_string(),
        extra: "welcome tour start setup hello drifter".to_string(),
        icon: Some("drifter-welcome".to_string()),
    }
}
