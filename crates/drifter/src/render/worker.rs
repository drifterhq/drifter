use super::*;

static LANES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub(super) struct Lane {
    id: u64,
    busy: bool,
    again: bool,
}

impl Default for Lane {
    fn default() -> Lane {
        Lane {
            id: LANES.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            busy: false,
            again: false,
        }
    }
}

impl Lane {
    pub(super) fn id(&self) -> u64 {
        self.id
    }

    pub(super) fn claim(&mut self) -> bool {
        match self.busy {
            true => {
                self.again = true;
                false
            }
            false => {
                self.busy = true;
                true
            }
        }
    }

    pub(super) fn release(&mut self) -> bool {
        self.busy = false;
        std::mem::take(&mut self.again)
    }
}

#[derive(Clone)]
pub(super) struct Frosting {
    frost: std::sync::Arc<drifter_core::frost::Frost>,
    laid: drifter_core::wallpaper::Placement,
    size: Vec2,
}

impl Frosting {
    pub(super) fn average(&self, rect: Rect) -> [f64; 3] {
        let mut sum = [0.0; 3];
        let steps = 6;
        for y in 0..steps {
            for x in 0..steps {
                let at = rect.origin
                    + Vec2::new(
                        rect.size.x * (f64::from(x) + 0.5) / f64::from(steps),
                        rect.size.y * (f64::from(y) + 0.5) / f64::from(steps),
                    );
                let seen = self.frost.behind(&self.laid, self.size, at);
                for c in 0..3 {
                    sum[c] += seen[c];
                }
            }
        }
        sum.map(|v| v / f64::from(steps * steps))
    }

    pub(super) fn under(&self, image: &mut drifter_core::Image, origin: Vec2, solid: f64) {
        let dense = drifter_core::paint::density();
        drifter_core::frost::underlay(image, solid, |point| {
            self.frost.behind(&self.laid, self.size, origin + point / dense)
        });
    }
}

impl Frosting {
    pub(super) fn screen(&self, screen: Vec2, width: u32) -> drifter_core::Image {
        let height = ((f64::from(width) * screen.y / screen.x.max(1.0)).round() as u32).max(1);
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                let point = Vec2::new(
                    (f64::from(x) + 0.5) * screen.x / f64::from(width),
                    (f64::from(y) + 0.5) * screen.y / f64::from(height),
                );
                let seen = self.frost.behind(&self.laid, self.size, point);
                rgba.extend(seen.map(|c| c.round().clamp(0.0, 255.0) as u8));
                rgba.push(255);
            }
        }
        drifter_core::Image { width, height, rgba }
    }
}

pub(super) fn contrast(frosting: &Option<Frosting>, look: drifter_core::lock::Look, rect: Rect) -> drifter_core::lock::Look {
    match frosting {
        Some(frosting) => look.over(frosting.average(rect)),
        None => look,
    }
}

pub(super) fn dim(cover: f64) -> drifter_core::Image {
    let dark = (cover * 255.0).round() as u8;
    drifter_core::Image { width: 1, height: 1, rgba: vec![0, 0, 0, dark] }
}

pub(super) fn frost(frosting: &Option<Frosting>, image: &mut drifter_core::Image, origin: Vec2, solid: f64) {
    if let Some(frosting) = frosting {
        frosting.under(image, origin, solid);
    }
}

pub(super) type Lanes = fn(&mut State) -> Option<&mut Lane>;

#[derive(Default)]
pub(super) struct Icons {
    asked: Vec<(String, u32)>,
    apps: Vec<(String, u32)>,
    lane: Lane,
}

impl State {
    pub(super) fn frosting(&self) -> Option<Frosting> {
        let paper = self.wallpaper.as_ref()?;
        let (laid, size) = self.laid_paper()?;
        Some(Frosting {
            frost: paper.frost.clone(),
            laid,
            size,
        })
    }

    pub(super) fn on_worker<T, W, D>(&self, name: &str, work: W, done: D)
    where
        T: Send + 'static,
        W: FnOnce() -> T + Send + 'static,
        D: FnOnce(&mut State, T) + Send + 'static,
    {
        let density = self.density;
        self.background(name, move || {
            drifter_core::paint::set_density(density);
            let out = work();
            move |state: &mut State| {
                if state.density == density {
                    done(state, out);
                }
            }
        });
    }

    pub(super) fn paint_on_worker<P, D>(&self, name: &str, paint: P, done: D)
    where
        P: FnOnce() -> Option<drifter_core::Image> + Send + 'static,
        D: FnOnce(&mut State, Option<TextureBuffer<Tex>>) + Send + 'static,
    {
        self.paint_sized(name, paint, move |state, art| done(state, art.map(|(art, _)| art)));
    }

    fn paint_sized<P, D>(&self, name: &str, paint: P, done: D)
    where
        P: FnOnce() -> Option<drifter_core::Image> + Send + 'static,
        D: FnOnce(&mut State, Option<(TextureBuffer<Tex>, Vec2)>) + Send + 'static,
    {
        let what = name.to_string();
        self.on_worker(name, paint, move |state, image| {
            let art = image.and_then(|image| {
                let size = drifter_core::paint::logical(&image);
                match upload(&mut state.renderer, Some(&image), &what) {
                    Ok(art) => art.map(|art| (art, size)),
                    Err(error) => {
                        log!("drifter: {error}");
                        None
                    }
                }
            });
            done(state, art);
        });
    }

    pub(super) fn paint_in_lane<P, D>(&mut self, name: &str, lane: Lanes, again: fn(&mut State), paint: P, done: D)
    where
        P: FnOnce() -> Option<drifter_core::Image> + Send + 'static,
        D: FnOnce(&mut State, TextureBuffer<Tex>) + Send + 'static,
    {
        self.paint_in_lane_sized(name, lane, again, paint, move |state, art, _| done(state, art));
    }

    pub(super) fn paint_in_lane_sized<P, D>(&mut self, name: &str, lane: Lanes, again: fn(&mut State), paint: P, done: D)
    where
        P: FnOnce() -> Option<drifter_core::Image> + Send + 'static,
        D: FnOnce(&mut State, TextureBuffer<Tex>, Vec2) + Send + 'static,
    {
        let Some(held) = lane(self) else {
            return;
        };
        if !held.claim() {
            return;
        }
        let id = held.id();
        self.paint_sized(name, paint, move |state, art| {
            let mine = lane(state).filter(|held| held.id() == id);
            let Some(held) = mine else {
                return;
            };
            let rerun = held.release();
            if let Some((art, size)) = art {
                done(state, art, size);
            }
            if rerun {
                again(state);
            }
            state.kick_now();
        });
    }

    pub(super) fn icon_now(&mut self, name: &str, side: u32) -> Option<drifter_core::Image> {
        let key = format!("{name}@{side}");
        if let Some(held) = self.search_icons.get(&key) {
            return held.clone();
        }
        if !self.icon_jobs.asked.iter().any(|(held, at)| held == name && *at == side) {
            self.icon_jobs.asked.push((name.to_string(), side));
        }
        self.fetch_icons();
        None
    }

    pub(super) fn app_icon_now(&mut self, app: &str, side: u32) -> Option<TextureBuffer<Tex>> {
        let key = format!("{app}@{side}");
        if let Some(held) = self.app_icons.get(&key) {
            return Some(held.clone());
        }
        if app.is_empty() || self.no_icon.contains(app) {
            return None;
        }
        if !self.icon_jobs.apps.iter().any(|(held, at)| held == app && *at == side) {
            self.icon_jobs.apps.push((app.to_string(), side));
        }
        self.fetch_icons();
        None
    }

    fn fetch_icons(&mut self) {
        if self.icon_jobs.asked.is_empty() && self.icon_jobs.apps.is_empty() {
            return;
        }
        if !self.icon_jobs.lane.claim() {
            return;
        }
        let id = self.icon_jobs.lane.id();
        let named = std::mem::take(&mut self.icon_jobs.asked);
        let apps = std::mem::take(&mut self.icon_jobs.apps);
        self.on_worker(
            "icons",
            move || {
                let load = |path: &std::path::Path, side: u32| {
                    drifter_core::image::load_sized(path, dense(side))
                        .ok()
                        .map(|image| image.scaled(dense(side), dense(side)))
                };
                let named: Vec<(String, Option<drifter_core::Image>)> = named
                    .into_iter()
                    .map(|(name, side)| {
                        let image = drifter_core::icon::find(&name, None, dense(side)).and_then(|path| load(&path, side));
                        (format!("{name}@{side}"), image)
                    })
                    .collect();
                let apps: Vec<(String, String, Option<drifter_core::Image>)> = apps
                    .into_iter()
                    .map(|(app, side)| {
                        let image = crate::apps::find(&app)
                            .and_then(|entry| entry.icon)
                            .and_then(|path| load(&path, side));
                        (app.clone(), format!("{app}@{side}"), image)
                    })
                    .collect();
                (named, apps)
            },
            move |state, (named, apps)| {
                if state.icon_jobs.lane.id() != id {
                    return;
                }
                let rerun = state.icon_jobs.lane.release();
                for (key, image) in named {
                    state.search_icons.insert(key, image);
                }
                for (app, key, image) in apps {
                    match image.and_then(|image| upload(&mut state.renderer, Some(&image), "app icon").ok().flatten()) {
                        Some(buffer) => {
                            state.app_icons.insert(key, buffer);
                        }
                        None => {
                            state.no_icon.insert(app);
                        }
                    }
                }
                state.icons_landed();
                if rerun {
                    state.fetch_icons();
                }
            },
        );
    }

    fn icons_landed(&mut self) {
        if self.glance_panel.is_some() {
            self.paint_glance();
        }
        if self.launchpad.is_some() {
            self.paint_launchpad();
        }
        if self.search.is_some() {
            self.paint_search();
        }
        if self.desk_menu.is_some() {
            self.refresh_desk_icons();
        }
        if self.switch.is_some() {
            self.refresh_switch_icons();
        }
        self.kick_now();
    }
}
