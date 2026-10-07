use super::*;
use drifter_core::desktop::{Offer, Role};
use drifter_core::welcome::{Card, Choice, Device, Hit, Lesson, Motion, Paper, Tile, View};

const SLIDE: Duration = Duration::from_millis(320);
const SLIDE_STILL: Duration = Duration::from_millis(140);
const INTRO: Duration = Duration::from_millis(1400);
const CLOSING: Duration = Duration::from_millis(260);
const NUDGE: f64 = 28.0;
const DIM: f32 = 0.12;
const KEY_ESCAPE: u32 = 9;
const KEY_RETURN: u32 = 36;
const KEY_PAD_ENTER: u32 = 104;
pub(super) const WELCOME_KEY: &str = "drifter:welcome";

enum Pick {
    Have(String),
    Get(Offer),
}

pub(super) struct Welcome {
    view: View,
    fonts: drifter_core::welcome::Fonts,
    grow: f64,
    art: Option<TextureBuffer<Tex>>,
    leaving: Option<(TextureBuffer<Tex>, f64)>,
    moved: Instant,
    way: f64,
    opened: Instant,
    intro: bool,
    closing: Option<Instant>,
    papers: Vec<PathBuf>,
    days: Vec<Option<Vec<TextureBuffer<Tex>>>>,
    hovered: Option<(usize, Instant)>,
    picks: Vec<Pick>,
    tiles: Vec<Option<String>>,
    busy: Vec<Offer>,
    picked_device: bool,
    density: u32,
    black: Option<TextureBuffer<Tex>>,
    accent: Option<TextureBuffer<Tex>>,
    shadow: Option<(TextureBuffer<Tex>, f64)>,
    job: u64,
}

impl State {
    pub(super) fn open_welcome(&mut self, first: bool) {
        if self.welcome.as_ref().is_some_and(|open| open.closing.is_none()) {
            return;
        }
        let config = match Config::load_from(&self.config_path) {
            Ok(config) => config,
            Err(error) => {
                log!("drifter: the welcome waits, the config did not load: {error}");
                return;
            }
        };
        self.launchpad = None;
        self.search = None;
        self.popover = None;
        let grow = grow_for(config.access.large_text);
        let fonts = match drifter_core::welcome::Fonts::load(config.font_path(), grow) {
            Ok(fonts) => fonts,
            Err(error) => {
                log!("drifter: no welcome without a font: {error}");
                return;
            }
        };
        let mut view = View::new(Card::Hello);
        view.greeting = drifter_core::welcome::greeting(
            drifter_core::night::hour_now(),
            &drifter_core::welcome::first_name(&own_gecos()),
        );
        let papers: Vec<PathBuf> = drifter_core::shader::installed(&config.shader_folders())
            .into_iter()
            .take(drifter_core::welcome::SHOWN_PAPERS)
            .collect();
        let shown = config.wallpaper_path();
        view.papers = papers
            .iter()
            .map(|path| Paper {
                name: title_case(&drifter_core::shader::stem(path)),
                still: None,
                chosen: shown.as_ref().is_some_and(|shown| shown == path),
            })
            .collect();
        view.device = match self.touchpads.is_empty() {
            true => Device::Mouse,
            false => Device::Touchpad,
        };
        view.quiet = true;
        let black = upload(
            &mut self.renderer,
            Some(&drifter_core::Image { width: 1, height: 1, rgba: vec![0, 0, 0, 255] }),
            "welcome dim",
        )
        .ok()
        .flatten();
        let accent = self.look.accent;
        let tint = upload(
            &mut self.renderer,
            Some(&drifter_core::Image {
                width: 1,
                height: 1,
                rgba: vec![accent[0], accent[1], accent[2], 255],
            }),
            "welcome accent",
        )
        .ok()
        .flatten();
        let patch = drifter_core::window::shadow_patch(drifter_core::welcome::RADIUS, 44.0);
        let shadow = upload_flat(&mut self.renderer, Some(&patch), "welcome shadow")
            .ok()
            .flatten()
            .map(|buffer| (buffer, f64::from(patch.width - 1) / 2.0));
        let count = papers.len();
        self.welcome = Some(Welcome {
            view,
            fonts,
            grow,
            art: None,
            leaving: None,
            moved: Instant::now(),
            way: 1.0,
            opened: Instant::now(),
            intro: first,
            closing: None,
            papers,
            days: vec![None; count],
            hovered: None,
            picks: Vec::new(),
            tiles: Vec::new(),
            busy: Vec::new(),
            picked_device: false,
            density: self.density,
            black,
            accent: tint,
            shadow,
            job: 0,
        });
        self.welcome_read(&config);
        self.welcome_tiles(&config);
        self.welcome_pictures();
        self.paint_welcome();
    }

    fn welcome_read(&mut self, config: &Config) {
        let scales = self.welcome_scales();
        let search: Vec<String> = config
            .keys
            .search
            .split('+')
            .map(|word| word.trim().to_lowercase())
            .filter(|word| !word.is_empty())
            .collect();
        let modifier = config.input.modifier;
        let style = config.appearance.style.unwrap_or(drifter_core::Style::Dark);
        let motion = Motion::of(config.access.reduce_motion, config.workspace.wallpaper_motion);
        let large = config.access.large_text;
        let shown = config.wallpaper_path();
        let Some(open) = self.welcome.as_mut() else {
            return;
        };
        let view = &mut open.view;
        view.style = style;
        view.motion = motion;
        view.large = large;
        view.modifier = modifier;
        view.search = search;
        (view.scales, view.scale, view.suggested, view.screen) = scales;
        for (paper, path) in view.papers.iter_mut().zip(&open.papers) {
            paper.chosen = shown.as_ref().is_some_and(|shown| shown == path);
        }
    }

    fn welcome_scales(&self) -> (Vec<f64>, f64, Option<f64>, String) {
        let Some(head) = self.heads.get(self.hand) else {
            return (vec![1.0], 1.0, None, String::new());
        };
        let pixels = Vec2::new(f64::from(head.mode.size.w), f64::from(head.mode.size.h));
        let pixels = match head.rotate % 180 {
            90 => Vec2::new(pixels.y, pixels.x),
            _ => pixels,
        };
        let mut scales = drifter_core::welcome::scales_for(pixels);
        if !scales.iter().any(|scale| (scale - head.scale).abs() < 0.01) {
            scales.push(head.scale);
            scales.sort_by(f64::total_cmp);
        }
        let millimeters = self
            .plugs
            .iter()
            .find(|plug| plug.name == head.name)
            .map(|plug| f64::from(plug.millimeters.0))
            .unwrap_or(0.0);
        let suggested = drifter_core::welcome::suggest(pixels.x, millimeters, &scales);
        let place = match head.name.starts_with("eDP") || head.name.starts_with("LVDS") || head.name.starts_with("DSI") {
            true => "Built-in screen".to_string(),
            false => head.name.clone(),
        };
        let words = format!("{place}, {} by {}", pixels.x as u32, pixels.y as u32);
        (scales, head.scale, suggested, words)
    }

    fn welcome_tiles(&mut self, config: &Config) {
        let roles = Role::ALL;
        let mut tiles = Vec::new();
        let mut keys = Vec::new();
        for role in roles {
            let item = config
                .apps
                .get(role)
                .cloned()
                .or_else(|| drifter_core::desktop::system_default(role).map(drifter_core::Launch::App));
            let id = item.as_ref().and_then(|item| item.app().map(str::to_string));
            let entry = id.as_deref().and_then(crate::apps::find);
            let name = item.as_ref().map(|item| {
                entry
                    .as_ref()
                    .and_then(|entry| entry.name.clone())
                    .unwrap_or_else(|| item.name())
            });
            let icon = id.as_deref().and_then(|id| self.welcome_icon(id, 68));
            let busy = self
                .welcome
                .as_ref()
                .is_some_and(|open| open.busy.iter().any(|offer| offer.role == role));
            tiles.push(Tile { role, name, icon, busy });
            keys.push(id);
        }
        if let Some(open) = self.welcome.as_mut() {
            open.view.tiles = tiles;
            open.tiles = keys;
        }
    }

    fn welcome_icon(&mut self, id: &str, side: u32) -> Option<drifter_core::Image> {
        let name = self
            .installed_apps()
            .iter()
            .find(|app| app.id == id)
            .and_then(|app| app.icon.clone())?;
        let key = format!("{name}@welcome{side}");
        if let Some(held) = self.search_icons.get(&key) {
            return held.clone();
        }
        let image = drifter_core::icon::find(&name, None, dense(side))
            .and_then(|path| drifter_core::image::load_sized(&path, dense(side)).ok())
            .map(|image| image.scaled(dense(side), dense(side)));
        self.search_icons.insert(key, image.clone());
        image
    }

    fn welcome_pictures(&mut self) {
        let Some(open) = self.welcome.as_mut() else {
            return;
        };
        open.job += 1;
        let job = open.job;
        let papers = open.papers.clone();
        let density = self.density;
        let chores = self.chores.clone();
        let spawned = std::thread::Builder::new()
            .name("drifter-welcome".to_string())
            .spawn(move || {
                drifter_core::paint::set_density(density);
                let hour = drifter_core::shader::hour_now();
                let stills: Vec<drifter_core::Image> = papers
                    .iter()
                    .map(|path| drifter_core::welcome::thumb(path, hour, 30.0))
                    .collect();
                let _ = chores.send(Box::new(move |state: &mut State| {
                    let Some(open) = state.welcome.as_mut().filter(|open| open.job == job) else {
                        return;
                    };
                    for (paper, still) in open.view.papers.iter_mut().zip(stills) {
                        paper.still = Some(still);
                    }
                    state.paint_welcome();
                }));
                for (at, path) in papers.iter().enumerate() {
                    let frames = drifter_core::welcome::day(path);
                    let sent = chores.send(Box::new(move |state: &mut State| {
                        let uploaded: Vec<TextureBuffer<Tex>> = frames
                            .iter()
                            .filter_map(|frame| upload(&mut state.renderer, Some(frame), "welcome day").ok().flatten())
                            .collect();
                        let Some(open) = state.welcome.as_mut().filter(|open| open.job == job) else {
                            return;
                        };
                        if uploaded.len() == drifter_core::welcome::FRAMES
                            && let Some(slot) = open.days.get_mut(at)
                        {
                            *slot = Some(uploaded);
                        }
                    }));
                    if sent.is_err() {
                        return;
                    }
                }
            });
        if let Err(error) = spawned {
            log!("drifter: the welcome pictures did not start: {error}");
        }
    }

    pub(super) fn welcome_rect(&self) -> Option<Rect> {
        self.welcome.as_ref()?;
        let look = drifter_core::welcome::Look::themed(&self.palette, self.look.accent);
        Some(look.rect(self.screen))
    }

    pub(super) fn paint_welcome(&mut self) {
        if self.welcome.as_ref().is_some_and(|open| open.density != self.density) {
            let card = self.welcome.as_ref().map(|open| open.view.card);
            self.welcome_rebuild(card);
            return;
        }
        let Some(rect) = self.welcome_rect() else {
            return;
        };
        let look = drifter_core::welcome::Look::themed(&self.palette, self.look.accent);
        let veil = self.veil(0.97);
        let Some(open) = self.welcome.as_ref() else {
            return;
        };
        let mut image = look.paint(&open.view, &open.fonts, veil);
        self.frosted(&mut image, rect.origin, veil);
        let art = match upload(&mut self.renderer, Some(&image), "welcome") {
            Ok(art) => art,
            Err(error) => {
                log!("drifter: {error}");
                None
            }
        };
        if let Some(open) = self.welcome.as_mut() {
            open.art = art;
        }
        self.kick_now();
    }

    fn welcome_rebuild(&mut self, card: Option<Card>) {
        let held = self.welcome.take();
        self.open_welcome(false);
        if let (Some(open), Some(was)) = (self.welcome.as_mut(), held) {
            open.view.card = card.unwrap_or(Card::Hello);
            open.view.done = was.view.done;
            open.view.quiet = was.view.quiet;
            open.view.device = was.view.device;
            open.picked_device = was.picked_device;
            open.busy = was.busy;
            open.intro = false;
        }
        self.paint_welcome();
    }

    pub(super) fn welcome_elements(&mut self, elements: &mut Vec<Skin>) {
        let Some(rect) = self.welcome_rect() else {
            return;
        };
        let reduce = self.access.reduce_motion;
        let screen = self.screen;
        let Some(open) = self.welcome.as_ref() else {
            return;
        };
        let now = Instant::now();
        let closing = open
            .closing
            .map(|since| (now.duration_since(since).as_secs_f64() / CLOSING.as_secs_f64()).min(1.0));
        let intro = match open.intro {
            true => (now.duration_since(open.opened).as_secs_f64() / INTRO.as_secs_f64()).min(1.0),
            false => 1.0,
        };
        let slide = match reduce || open.view.motion == Motion::Still {
            true => SLIDE_STILL,
            false => SLIDE,
        };
        let t = drifter_core::motion::smooth((now.duration_since(open.moved).as_secs_f64() / slide.as_secs_f64()).min(1.0));
        let shift = match reduce || open.view.motion == Motion::Still {
            true => 0.0,
            false => NUDGE,
        };
        let appear = match open.intro {
            true => drifter_core::motion::smooth(((intro - 0.35) / 0.65).clamp(0.0, 1.0)),
            false => 1.0,
        };
        let fade = closing.map(|c| 1.0 - drifter_core::motion::smooth(c)).unwrap_or(1.0) * appear;
        let rise = Vec2::new(0.0, (1.0 - appear) * 18.0 + closing.unwrap_or(0.0) * 10.0);
        let here = Rect::new(rect.origin + Vec2::new(open.way * shift * (1.0 - t), 0.0) + rise, rect.size);
        let look = drifter_core::welcome::Look::themed(&self.palette, self.look.accent);
        if open.view.card == Card::Look && open.closing.is_none() {
            if let Some((at, since)) = open.hovered
                && let Some(Some(frames)) = open.days.get(at)
            {
                let thumb = look.paper_rect(at);
                let target = Rect::new(here.origin + thumb.origin, thumb.size);
                let (one, two, blend) = drifter_core::welcome::frame_at(now.duration_since(since).as_secs_f64());
                let alpha = (fade * t) as f32;
                elements.push(Skin::Texture(element(&frames[two], target, alpha * blend as f32)));
                elements.push(Skin::Texture(element(&frames[one], target, alpha)));
                if let Some(white) = self.plain.as_ref() {
                    let day = (now.duration_since(since).as_secs_f64() / drifter_core::welcome::CYCLE).fract();
                    let track = Rect::new(
                        Vec2::new(target.origin.x + 12.0, target.max().y - 9.0),
                        Vec2::new(target.size.x - 24.0, 2.0),
                    );
                    let gone = Rect::new(track.origin, Vec2::new(track.size.x * day, 2.0));
                    let dot = Rect::from_center(Vec2::new(gone.max().x, track.center().y), Vec2::splat(6.0));
                    elements.push(Skin::Texture(fill(white, dot, alpha)));
                    elements.push(Skin::Texture(fill(white, gone, alpha * 0.9)));
                    elements.push(Skin::Texture(fill(white, track, alpha * 0.3)));
                }
            }
        }
        if let Some(tint) = open.accent.as_ref().filter(|_| open.closing.is_none()) {
            let bars: Vec<Rect> = match (&open.view.choosing, open.view.card) {
                (Some((_, choices)), Card::Apps) => choices
                    .iter()
                    .take(10)
                    .enumerate()
                    .filter(|(_, choice)| choice.busy)
                    .map(|(at, _)| look.choice_bar(at, choices.len().min(10)))
                    .collect(),
                (None, Card::Apps) => open
                    .view
                    .tiles
                    .iter()
                    .enumerate()
                    .filter(|(_, tile)| tile.busy)
                    .map(|(at, _)| look.tile_bar(at))
                    .collect(),
                _ => Vec::new(),
            };
            let phase = now.duration_since(open.opened).as_secs_f64() / 1.1;
            for bar in bars {
                let bar = Rect::new(here.origin + bar.origin, bar.size);
                let wide = bar.size.x * 0.32;
                let x = bar.origin.x + (bar.size.x + wide) * phase.fract() - wide;
                let from = x.max(bar.origin.x);
                let to = (x + wide).min(bar.max().x);
                if to > from {
                    let run = Rect::new(Vec2::new(from, bar.origin.y), Vec2::new(to - from, bar.size.y));
                    elements.push(Skin::Texture(fill(tint, run, fade as f32)));
                }
                if let Some(white) = self.plain.as_ref() {
                    elements.push(Skin::Texture(fill(white, bar, fade as f32 * 0.18)));
                }
            }
        }
        if let Some(art) = open.art.as_ref() {
            elements.push(Skin::Texture(element(art, here, (fade * t) as f32)));
        }
        if let Some((art, way)) = open.leaving.as_ref()
            && t < 1.0
        {
            let gone = Rect::new(rect.origin - Vec2::new(way * shift * t, 0.0) + rise, rect.size);
            elements.push(Skin::Texture(element(art, gone, (fade * (1.0 - t)) as f32)));
        }
        if let Some((buffer, corner)) = open.shadow.as_ref() {
            for (source, target) in drifter_core::window::shadow_slices(here, *corner, 1.0, 10.0) {
                elements.push(Skin::Texture(faded(buffer, source, target, 0.2 * fade as f32)));
            }
        }
        if let Some(black) = open.black.as_ref() {
            let dim = DIM * fade as f32;
            elements.push(Skin::Texture(fill(black, Rect::new(Vec2::ZERO, screen), dim)));
            if open.intro && intro < 1.0 {
                let dark = 1.0 - drifter_core::motion::smooth((intro / 0.7).min(1.0));
                elements.push(Skin::Texture(fill(black, Rect::new(Vec2::ZERO, screen), dark as f32)));
            }
        }
    }

    pub(super) fn welcome_busy(&self) -> bool {
        let Some(open) = self.welcome.as_ref() else {
            return false;
        };
        let now = Instant::now();
        open.closing.is_some()
            || (open.intro && now.duration_since(open.opened) < INTRO)
            || now.duration_since(open.moved) < SLIDE
            || (open.view.card == Card::Look && open.hovered.is_some())
            || (open.view.card == Card::Apps && !open.busy.is_empty())
    }

    pub(super) fn settle_welcome(&mut self) {
        let Some(open) = self.welcome.as_mut() else {
            return;
        };
        if let Some(since) = open.closing
            && since.elapsed() >= CLOSING
        {
            self.welcome = None;
            return;
        }
        if open.leaving.is_some() && open.moved.elapsed() >= SLIDE {
            open.leaving = None;
        }
        if open.intro && open.opened.elapsed() >= INTRO {
            open.intro = false;
        }
    }

    pub(super) fn hover_welcome(&mut self) {
        let Some(rect) = self.welcome_rect() else {
            return;
        };
        let inside = self.pointer - rect.origin;
        let look = drifter_core::welcome::Look::themed(&self.palette, self.look.accent);
        let Some(open) = self.welcome.as_mut().filter(|open| open.closing.is_none()) else {
            return;
        };
        let hit = match rect.contains(self.pointer) {
            true => look.hit(&open.view, &open.fonts, inside),
            false => None,
        };
        let paper = match hit {
            Some(Hit::Paper(at)) => Some(at),
            _ => None,
        };
        if open.hovered.map(|(at, _)| at) != paper {
            open.hovered = paper.map(|at| (at, Instant::now()));
        }
        if open.view.hover != hit {
            open.view.hover = hit;
            self.paint_welcome();
        }
    }

    pub(super) fn press_welcome(&mut self, code: u32) -> bool {
        let Some(rect) = self.welcome_rect() else {
            return false;
        };
        if self.welcome.as_ref().is_some_and(|open| open.closing.is_some()) || !rect.contains(self.pointer) {
            return false;
        }
        if code != BTN_LEFT {
            return true;
        }
        let look = drifter_core::welcome::Look::themed(&self.palette, self.look.accent);
        let hit = self
            .welcome
            .as_ref()
            .and_then(|open| look.hit(&open.view, &open.fonts, self.pointer - rect.origin));
        if let Some(hit) = hit {
            self.welcome_do(hit);
        }
        true
    }

    pub(super) fn welcome_key(&mut self, code: u32) -> bool {
        let Some(open) = self.welcome.as_ref().filter(|open| open.closing.is_none()) else {
            return false;
        };
        if self.wayland.keyboard.current_focus().is_some() {
            return false;
        }
        let card = open.view.card;
        let choosing = open.view.choosing.is_some();
        match code {
            KEY_RETURN | KEY_PAD_ENTER => self.welcome_do(Hit::Next),
            KEY_ESCAPE if choosing => self.welcome_do(Hit::Leave),
            KEY_ESCAPE if card == Card::Done => self.welcome_do(Hit::Next),
            KEY_ESCAPE => self.close_welcome(),
            KEY_RIGHT => self.welcome_do(Hit::Next),
            KEY_LEFT if card != Card::Hello => self.welcome_do(Hit::Back),
            KEY_LEFT => {}
            _ => return false,
        }
        true
    }

    pub(super) fn welcome_eats(&self, code: u32) -> bool {
        self.welcome.as_ref().is_some_and(|open| open.closing.is_none())
            && self.wayland.keyboard.current_focus().is_none()
            && matches!(code, KEY_ESCAPE | KEY_RETURN | KEY_PAD_ENTER | KEY_LEFT | KEY_RIGHT)
    }

    fn welcome_go(&mut self, card: Card) {
        let Some(open) = self.welcome.as_mut() else {
            return;
        };
        if open.view.card == card {
            return;
        }
        open.way = match card.index() > open.view.card.index() {
            true => 1.0,
            false => -1.0,
        };
        open.leaving = open.art.take().map(|art| (art, open.way));
        open.moved = Instant::now();
        open.view.card = card;
        open.view.choosing = None;
        open.view.hover = None;
        open.hovered = None;
        if card == Card::Keys {
            open.view.focus = Lesson::ALL
                .into_iter()
                .find(|lesson| !open.view.done[lesson.index()])
                .unwrap_or(Lesson::Search);
        }
        let refresh = matches!(card, Card::Apps | Card::Display);
        if refresh && let Ok(config) = Config::load_from(&self.config_path) {
            self.welcome_read(&config);
            self.welcome_tiles(&config);
        }
        self.paint_welcome();
    }

    fn welcome_do(&mut self, hit: Hit) {
        let Some(open) = self.welcome.as_ref() else {
            return;
        };
        let card = open.view.card;
        match hit {
            Hit::Next => match card.next() {
                Some(next) => self.welcome_go(next),
                None => {
                    let quiet = open.view.quiet;
                    self.close_welcome();
                    self.write_config(|text| drifter_core::settings::write_flag(text, "welcome", "show", !quiet));
                }
            },
            Hit::Back => {
                if let Some(back) = card.back() {
                    self.welcome_go(back);
                }
            }
            Hit::Dot(at) => self.welcome_go(Card::ALL[at.min(Card::ALL.len() - 1)]),
            Hit::Style(style) => {
                let word = match style {
                    drifter_core::Style::Light => "light",
                    drifter_core::Style::Dark => "dark",
                    drifter_core::Style::Auto => "auto",
                };
                self.write_config(|text| drifter_core::settings::write_word(text, "appearance", "style", word));
                self.welcome_reload();
            }
            Hit::Paper(at) => {
                let Some(path) = open.papers.get(at).cloned() else {
                    return;
                };
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                self.write_config(|text| drifter_core::settings::write_word(text, "workspace", "wallpaper", &name));
                self.welcome_reload();
            }
            Hit::Motion(motion) => {
                self.write_config(|text| {
                    let text = drifter_core::settings::write_flag(text, "access", "reduce_motion", motion.reduce())?;
                    drifter_core::settings::write_number(&text, "workspace", "wallpaper_motion", i64::from(motion.percent()))
                });
                self.welcome_reload();
            }
            Hit::Text(large) => {
                self.write_config(|text| drifter_core::settings::write_flag(text, "access", "large_text", large));
                if let Some(open) = self.welcome.as_mut()
                    && open.grow != grow_for(large)
                    && let Ok(config) = Config::load_from(&self.config_path)
                    && let Ok(fonts) = drifter_core::welcome::Fonts::load(config.font_path(), grow_for(large))
                {
                    open.fonts = fonts;
                    open.grow = grow_for(large);
                }
                self.welcome_reload();
            }
            Hit::Scale(at) => {
                let Some(scale) = open.view.scales.get(at).copied() else {
                    return;
                };
                let Some(name) = self.heads.get(self.hand).map(|head| head.name.clone()) else {
                    return;
                };
                self.write_config(|text| drifter_core::settings::write_scale(text, &name, scale));
                self.welcome_reload();
            }
            Hit::Device(device) => {
                if let Some(open) = self.welcome.as_mut() {
                    open.view.device = device;
                    open.picked_device = true;
                }
                self.paint_welcome();
            }
            Hit::Lesson(at) => {
                if let Some(open) = self.welcome.as_mut() {
                    open.view.focus = Lesson::ALL[at.min(Lesson::ALL.len() - 1)];
                }
                self.paint_welcome();
            }
            Hit::Tile(at) => {
                if let Some(role) = open.view.tiles.get(at).map(|tile| tile.role) {
                    self.welcome_choose(role);
                }
            }
            Hit::Choice(at) => self.welcome_pick(at),
            Hit::Leave => {
                if let Some(open) = self.welcome.as_mut() {
                    open.view.choosing = None;
                    open.view.note = None;
                    open.view.hover = None;
                }
                if let Ok(config) = Config::load_from(&self.config_path) {
                    self.welcome_tiles(&config);
                }
                self.paint_welcome();
            }
            Hit::Quiet => {
                if let Some(open) = self.welcome.as_mut() {
                    open.view.quiet = !open.view.quiet;
                }
                self.paint_welcome();
            }
        }
    }

    pub(super) fn close_welcome(&mut self) {
        let Some(open) = self.welcome.as_mut() else {
            return;
        };
        if open.closing.is_none() {
            open.closing = Some(Instant::now());
        }
        self.kick_now();
    }

    fn write_config(&mut self, change: impl FnOnce(&str) -> Result<String, String>) {
        let path = self.config_path.clone();
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        match change(&text) {
            Ok(out) if out != text => {
                if let Err(error) = std::fs::write(&path, out) {
                    log!("drifter: the welcome cannot save: {error}");
                }
            }
            Ok(_) => {}
            Err(error) => log!("drifter: the welcome cannot save: {error}"),
        }
    }

    fn welcome_reload(&mut self) {
        self.watch_config();
        if let Ok(config) = Config::load_from(&self.config_path) {
            self.welcome_read(&config);
        }
        self.paint_welcome();
    }

    fn welcome_choose(&mut self, role: Role) {
        let have = drifter_core::desktop::serving(role);
        let now = self
            .welcome
            .as_ref()
            .and_then(|open| open.tiles.get(Role::ALL.iter().position(|each| *each == role)?).cloned())
            .flatten();
        let busy: Vec<Offer> = self.welcome.as_ref().map(|open| open.busy.clone()).unwrap_or_default();
        let mut choices = Vec::new();
        let mut picks = Vec::new();
        for app in have.iter().take(10) {
            choices.push(Choice {
                name: app.name.clone(),
                icon: self.welcome_icon(&app.id, 56),
                chosen: now.as_deref() == Some(app.id.as_str()),
                installed: true,
                busy: false,
            });
            picks.push(Pick::Have(app.id.clone()));
        }
        for offer in drifter_core::desktop::offers(role, &have) {
            if choices.len() >= 10 {
                break;
            }
            choices.push(Choice {
                name: offer.name.to_string(),
                icon: None,
                chosen: false,
                installed: false,
                busy: busy.contains(&offer),
            });
            picks.push(Pick::Get(offer));
        }
        let note = match installer() {
            None if picks.iter().any(|pick| matches!(pick, Pick::Get(_))) => {
                Some("Getting new apps needs pkexec and apt or dnf.".to_string())
            }
            _ => None,
        };
        if let Some(open) = self.welcome.as_mut() {
            open.view.choosing = Some((role, choices));
            open.view.note = note;
            open.view.hover = None;
            open.picks = picks;
        }
        self.paint_welcome();
    }

    fn welcome_pick(&mut self, at: usize) {
        let Some(open) = self.welcome.as_ref() else {
            return;
        };
        let Some((role, _)) = open.view.choosing.as_ref() else {
            return;
        };
        let role = *role;
        match open.picks.get(at) {
            Some(Pick::Have(id)) => {
                let id = id.clone();
                self.welcome_default(role, &id);
                self.welcome_do(Hit::Leave);
            }
            Some(Pick::Get(offer)) => {
                let offer = *offer;
                if open.busy.contains(&offer) {
                    return;
                }
                self.welcome_install(offer);
            }
            None => {}
        }
    }

    fn welcome_default(&mut self, role: Role, id: &str) {
        let key = match role {
            Role::Files => "files",
            Role::Terminal => "terminal",
            Role::Browser => "browser",
            Role::Editor => "editor",
        };
        let launch = drifter_core::Launch::App(id.to_string());
        self.write_config(|text| drifter_core::settings::write_app(text, key, &launch));
        let list = mimeapps();
        let text = std::fs::read_to_string(&list).unwrap_or_default();
        let out = drifter_core::desktop::set_default(&text, role, id);
        if let Some(folder) = list.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        if let Err(error) = std::fs::write(&list, out) {
            log!("drifter: {} is left alone: {error}", list.display());
        }
        self.watch_config();
    }

    fn welcome_install(&mut self, offer: Offer) {
        let Some((program, package)) = installer().map(|(tool, apt)| (tool, if apt { offer.apt } else { offer.dnf })) else {
            if let Some(open) = self.welcome.as_mut() {
                open.view.note = Some("Getting new apps needs pkexec and apt or dnf.".to_string());
            }
            self.paint_welcome();
            return;
        };
        if let Some(open) = self.welcome.as_mut() {
            open.busy.push(offer);
            if let Some((_, choices)) = open.view.choosing.as_mut() {
                for (choice, pick) in choices.iter_mut().zip(&open.picks) {
                    if matches!(pick, Pick::Get(held) if *held == offer) {
                        choice.busy = true;
                    }
                }
            }
            open.view.note = None;
        }
        self.paint_welcome();
        self.background("welcome-install", move || {
            let mut command = std::process::Command::new("pkexec");
            match program {
                "apt-get" => command.args(["env", "DEBIAN_FRONTEND=noninteractive", "apt-get", "install", "-y", package]),
                _ => command.args([program, "install", "-y", package]),
            };
            let done = command
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .map(|status| status.success())
                .unwrap_or(false);
            move |state: &mut State| state.welcome_installed(offer, done)
        });
    }

    fn welcome_installed(&mut self, offer: Offer, done: bool) {
        self.installed = None;
        let Some(open) = self.welcome.as_mut() else {
            return;
        };
        open.busy.retain(|held| *held != offer);
        let found = drifter_core::desktop::serving(offer.role)
            .into_iter()
            .find(|app| offer.ids.iter().any(|id| id.eq_ignore_ascii_case(&app.id)));
        match (done, found) {
            (true, Some(app)) => {
                self.welcome_default(offer.role, &app.id);
            }
            _ => {
                if let Some(open) = self.welcome.as_mut() {
                    open.view.note = Some(format!("{} did not install.", offer.name));
                }
            }
        }
        let choosing = self
            .welcome
            .as_ref()
            .and_then(|open| open.view.choosing.as_ref().map(|(role, _)| *role));
        if let Ok(config) = Config::load_from(&self.config_path) {
            self.welcome_tiles(&config);
        }
        let note = self.welcome.as_ref().and_then(|open| open.view.note.clone());
        if let Some(role) = choosing {
            self.welcome_choose(role);
            if let (Some(open), Some(note)) = (self.welcome.as_mut(), note) {
                open.view.note = Some(note);
            }
        }
        self.paint_welcome();
    }

    pub(super) fn welcome_did(&mut self, lesson: Lesson) {
        let Some(open) = self.welcome.as_mut().filter(|open| open.view.card == Card::Keys && open.closing.is_none()) else {
            return;
        };
        let at = lesson.index();
        if open.view.done[at] {
            return;
        }
        open.view.done[at] = true;
        if open.view.focus == lesson {
            open.view.focus = Lesson::ALL
                .into_iter()
                .find(|each| !open.view.done[each.index()])
                .unwrap_or(lesson);
        }
        self.paint_welcome();
    }

    pub(super) fn welcome_device(&mut self, touchpad: bool) {
        let Some(open) = self.welcome.as_mut().filter(|open| !open.picked_device) else {
            return;
        };
        let device = match touchpad {
            true => Device::Touchpad,
            false => Device::Mouse,
        };
        if open.view.device != device {
            open.view.device = device;
            if open.view.card == Card::Keys {
                self.paint_welcome();
            }
        }
    }

    pub(super) fn welcome_held(&mut self, held: bool) {
        let Some(open) = self.welcome.as_mut().filter(|open| open.view.card == Card::Keys) else {
            return;
        };
        if open.view.held != held {
            open.view.held = held;
            self.paint_welcome();
        }
    }

    pub(super) fn welcome_restyle(&mut self) {
        if self.welcome.is_some() {
            self.paint_welcome();
        }
    }
}

fn grow_for(large: bool) -> f64 {
    match large {
        true => 1.12,
        false => 1.0,
    }
}

fn title_case(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

fn own_gecos() -> String {
    let uid = unsafe { libc::getuid() };
    let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
    passwd
        .lines()
        .map(|line| line.split(':').collect::<Vec<_>>())
        .find(|fields| fields.len() >= 5 && fields[2].parse::<u32>().ok() == Some(uid))
        .map(|fields| fields[4].to_string())
        .unwrap_or_default()
}

fn mimeapps() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .unwrap_or_else(|| PathBuf::from(".config"))
        .join("mimeapps.list")
}

fn installer() -> Option<(&'static str, bool)> {
    let on = |path: &str| Path::new(path).is_file();
    if !(on("/usr/bin/pkexec") || on("/bin/pkexec")) {
        return None;
    }
    match (on("/usr/bin/apt-get"), on("/usr/bin/dnf")) {
        (true, _) => Some(("apt-get", true)),
        (_, true) => Some(("dnf", false)),
        _ => None,
    }
}
