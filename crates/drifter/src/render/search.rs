use super::*;
use drifter_core::search::{Found, Kind};

pub(super) struct Search {
    pub query: String,
    pub all: Vec<Found>,
    pub results: Vec<Found>,
    pub selected: usize,
    pub art: Option<(TextureBuffer<Tex>, Rect)>,
    pub lane: worker::Lane,
    pub height: (f64, Instant),
    pub prior: Option<(TextureBuffer<Tex>, Rect)>,
}

const SEARCH_EASE: f64 = 0.16;
const SEARCH_RADIUS: f64 = 16.0;

impl State {
    pub(super) fn toggle_search(&mut self) {
        if self.search.take().is_some() {
            self.kick_now();
            return;
        }
        self.popover = None;
        self.search = Some(Search {
            query: String::new(),
            all: drifter_core::search::settings(),
            results: Vec::new(),
            selected: 0,
            art: None,
            lane: worker::Lane::default(),
            height: (0.0, Instant::now()),
            prior: None,
        });
        self.paint_search();
        let apps = self.installed_apps();
        self.background("search-index", move || {
            let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
            let mut found = drifter_core::search::apps(&apps);
            found.push(super::launchpad::welcome_found());
            found.extend(drifter_core::search::files(&home));
            move |state: &mut State| {
                let Some(open) = state.search.as_mut() else {
                    return;
                };
                found.extend(std::mem::take(&mut open.all));
                open.all = found;
                if !open.query.is_empty() {
                    open.results = drifter_core::search::rank(&open.query, &open.all);
                    open.selected = open.selected.min(open.results.len().saturating_sub(1));
                    state.paint_search();
                }
            }
        });
    }

    pub(super) fn search_look(&self) -> drifter_core::search::Look {
        let plain = drifter_core::search::Look::themed(&self.palette, self.look.accent);
        drifter_core::search::Look {
            back_alpha: self.veil(plain.back_alpha),
            ..plain
        }
    }

    pub(super) fn paint_search(&mut self) {
        let (Some(big), Some(small)) = (self.search_font.clone(), self.font.clone()) else {
            return;
        };
        let look = self.search_look();
        let side = look.icon as u32;
        let Some(results) = self.search.as_ref().map(|open| open.results.clone()) else {
            return;
        };
        let icons: Vec<Option<drifter_core::Image>> = results
            .iter()
            .map(|found| match (found.kind, found.icon.as_deref()) {
                (Kind::App, Some(name)) => self.icon_now(name, side),
                _ => None,
            })
            .collect();
        let Some(open) = self.search.as_ref() else {
            return;
        };
        let (query, selected) = (open.query.clone(), open.selected);
        let rect = look.rect(self.screen, results.len());
        let frosting = self.frosting();
        self.paint_in_lane(
            "search",
            |state| state.search.as_mut().map(|open| &mut open.lane),
            State::paint_search,
            move || {
                let mut image = look.paint(&query, &results, &icons, selected, &big, &small);
                worker::frost(&frosting, &mut image, rect.origin, look.back_alpha);
                Some(image)
            },
            move |state, art| {
                let shown = state.search_height();
                if let Some(open) = state.search.as_mut() {
                    let old = open.art.replace((art, rect));
                    match shown {
                        Some(shown) if (shown - rect.size.y).abs() > 0.5 => {
                            open.height = (shown, Instant::now());
                            open.prior = old.filter(|(_, was)| was.size.y > rect.size.y);
                        }
                        Some(_) => {}
                        None => open.height = (rect.size.y, Instant::now()),
                    }
                }
            },
        );
    }

    pub(super) fn search_height(&self) -> Option<f64> {
        let open = self.search.as_ref()?;
        let (_, rect) = open.art.as_ref()?;
        let pace = self.window_pace;
        if pace <= 0.0 {
            return Some(rect.size.y);
        }
        let (from, since) = open.height;
        let t = since.elapsed().as_secs_f64() / (SEARCH_EASE / pace);
        Some(from + (rect.size.y - from) * drifter_core::motion::smooth(t))
    }

    pub(super) fn search_moving(&self) -> bool {
        let Some(open) = self.search.as_ref() else {
            return false;
        };
        self.window_pace > 0.0
            && open.art.is_some()
            && open.height.1.elapsed().as_secs_f64() < SEARCH_EASE / self.window_pace
    }

    pub(super) fn search_layers(&self) -> Option<(Vec<(TextureBuffer<Tex>, Rect, Rect)>, Rect)> {
        let open = self.search.as_ref()?;
        let (art, rect) = open.art.as_ref()?;
        let height = self.search_height()?;
        let (art, size) = match open.prior.as_ref().filter(|_| height > rect.size.y + 0.5) {
            Some((prior, was)) => (prior, was.size),
            None => (art, rect.size),
        };
        let frame = Rect::new(rect.origin, Vec2::new(rect.size.x, height));
        let parts = drifter_core::search::Look::slices(size, height, SEARCH_RADIUS)
            .into_iter()
            .filter(|(source, _)| source.size.y > 0.0)
            .map(|(source, target)| (art.clone(), source, Rect::new(rect.origin + target.origin, target.size)))
            .collect();
        Some((parts, frame))
    }

    pub(super) fn search_key(&mut self, key: LockKey) {
        let Some(open) = self.search.as_mut() else {
            return;
        };
        match key {
            LockKey::Letter(letter) => {
                open.query.push(letter);
                open.results = drifter_core::search::rank(&open.query, &open.all);
                open.selected = 0;
            }
            LockKey::Back => {
                open.query.pop();
                open.results = drifter_core::search::rank(&open.query, &open.all);
                open.selected = 0;
            }
            LockKey::Down => {
                if !open.results.is_empty() {
                    open.selected = (open.selected + 1) % open.results.len();
                }
            }
            LockKey::Up => {
                if !open.results.is_empty() {
                    open.selected = (open.selected + open.results.len() - 1) % open.results.len();
                }
            }
            LockKey::Enter => {
                let chosen = open.results.get(open.selected).cloned();
                self.search = None;
                if let Some(found) = chosen {
                    self.open_found(&found);
                }
                self.kick_now();
                return;
            }
            LockKey::Clear => {
                self.search = None;
                self.kick_now();
                return;
            }
            LockKey::Vt(vt) => {
                self.change_vt(vt);
                return;
            }
            LockKey::Other | LockKey::Left | LockKey::Right => return,
        }
        self.paint_search();
    }

    pub(super) fn press_search(&mut self, code: u32) -> bool {
        let Some(open) = self.search.as_ref() else {
            return false;
        };
        let Some((_, rect)) = open.art.as_ref() else {
            return false;
        };
        let rect = *rect;
        if !rect.contains(self.pointer) {
            self.search = None;
            self.kick_now();
            return true;
        }
        if code != BTN_LEFT {
            return true;
        }
        let look = self.search_look();
        if let Some(at) = look.row_at(self.pointer - rect.origin, open.results.len()) {
            let found = open.results[at].clone();
            self.search = None;
            self.open_found(&found);
            self.kick_now();
        }
        true
    }

    fn open_found(&mut self, found: &Found) {
        match found.kind {
            Kind::App if found.key == super::welcome::WELCOME_KEY => self.open_welcome(false),
            Kind::App => {
                let launch = drifter_core::Launch::App(found.key.clone());
                crate::apps::run(&launch, &self.socket);
            }
            Kind::Setting => self.open_settings_at(&found.key),
            Kind::File => {
                let path = std::path::Path::new(&found.key);
                match path.is_dir() {
                    true => crate::apps::open_folder(&found.key, self.apps_chosen.files.as_ref(), &self.socket),
                    false => crate::apps::start("xdg-open %f", &[found.key.as_str()], &self.socket),
                }
            }
        }
    }
}
