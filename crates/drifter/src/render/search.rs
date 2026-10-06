use super::*;
use drifter_core::search::{Found, Kind};

pub(super) struct Search {
    pub query: String,
    pub all: Vec<Found>,
    pub results: Vec<Found>,
    pub selected: usize,
    pub art: Option<(TextureBuffer<Tex>, Rect)>,
}

impl State {
    pub(super) fn toggle_search(&mut self) {
        if self.search.take().is_some() {
            self.kick_now();
            return;
        }
        self.popover = None;
        self.gauge_panel = None;
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let mut all = drifter_core::search::apps(&drifter_core::desktop::installed());
        all.extend(drifter_core::search::settings());
        all.extend(drifter_core::search::files(&home));
        self.search = Some(Search {
            query: String::new(),
            all,
            results: Vec::new(),
            selected: 0,
            art: None,
        });
        self.paint_search();
    }

    fn search_look(&self) -> drifter_core::search::Look {
        let plain = drifter_core::search::Look::themed(&self.palette, self.look.accent);
        drifter_core::search::Look {
            back_alpha: self.veil(plain.back_alpha),
            ..plain
        }
    }

    pub(super) fn paint_search(&mut self) {
        let Some(open) = self.search.as_ref() else {
            return;
        };
        let (Some(big), Some(small)) = (self.search_font.as_ref(), self.font.as_ref()) else {
            return;
        };
        let look = self.search_look();
        let side = look.icon as u32;
        let icons: Vec<Option<drifter_core::Image>> = open
            .results
            .iter()
            .map(|found| match (found.kind, found.icon.as_deref()) {
                (Kind::App, Some(name)) => self
                    .search_icons
                    .get(name)
                    .cloned()
                    .unwrap_or_else(|| {
                        drifter_core::icon::find(name, None, side)
                            .and_then(|path| drifter_core::image::load_sized(&path, side).ok())
                            .map(|image| image.scaled(side, side))
                    }),
                _ => None,
            })
            .collect();
        let image = look.paint(&open.query, &open.results, &icons, open.selected, big, small);
        let rect = look.rect(self.screen, open.results.len());
        let mut image = image;
        self.frosted(&mut image, rect.origin, look.back_alpha);
        for (found, icon) in open.results.iter().zip(&icons) {
            if let Some(name) = found.icon.clone() {
                self.search_icons.entry(name).or_insert_with(|| icon.clone());
            }
        }
        let art = upload(&mut self.renderer, Some(&image), "search")
            .ok()
            .flatten()
            .map(|buffer| (buffer, rect));
        if let Some(open) = self.search.as_mut() {
            open.art = art;
        }
        self.kick_now();
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
