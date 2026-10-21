use std::collections::HashMap;

use super::*;
use drifter_core::motion::{appear, vanish};

pub(super) const ROUND_PATCH: f64 = 12.0;
const FADE_IN: f64 = 0.16;
const FADE_OUT: f64 = 0.12;
const ALERT_IN: f64 = 0.22;
const ALERT_FROM_SCALE: f64 = 0.90;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum Pane {
    Menu,
    DeskMenu,
    Tip,
    Search,
    Launchpad,
    Control,
    Notes,
    Charges,
    Glance,
    GlanceCard,
    Hud,
    Card(u32),
    Switch,
    Alert,
    Prompt,
    Pair,
    Pill,
    Share,
}

impl Pane {
    const FIXED: [Pane; 17] = [
        Pane::Menu,
        Pane::DeskMenu,
        Pane::Tip,
        Pane::Search,
        Pane::Launchpad,
        Pane::Control,
        Pane::Notes,
        Pane::Charges,
        Pane::Glance,
        Pane::GlanceCard,
        Pane::Hud,
        Pane::Switch,
        Pane::Alert,
        Pane::Prompt,
        Pane::Pair,
        Pane::Pill,
        Pane::Share,
    ];

    fn grows_itself(self) -> bool {
        matches!(self, Pane::Menu | Pane::DeskMenu)
    }
}

#[derive(Clone)]
pub(super) struct Layer {
    pub art: TextureBuffer<Tex>,
    pub rect: Rect,
    pub source: Option<Rect>,
    pub scales: bool,
    pub shadow: Shadow,
    pub shade: Option<Rect>,
    pub alpha: f32,
}

impl Layer {
    pub(super) fn new(art: &TextureBuffer<Tex>, rect: Rect, shadow: Shadow) -> Layer {
        Layer {
            art: art.clone(),
            rect,
            source: None,
            scales: true,
            shadow,
            shade: None,
            alpha: 1.0,
        }
    }
}

struct Fader {
    born: Instant,
    last: Vec<Layer>,
    frame: Rect,
    gone: Option<Instant>,
}

#[derive(Default)]
pub(super) struct Fades {
    panes: HashMap<Pane, Fader>,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Shadow {
    None,
    Tip,
    Menu,
    Modal,
}

impl Shadow {
    fn cast(self) -> Option<(f64, f64, f32)> {
        match self {
            Shadow::None => None,
            Shadow::Tip => Some((0.35, 3.0, 0.22)),
            Shadow::Menu => Some((0.6, 6.0, 0.30)),
            Shadow::Modal => Some((1.0, 8.0, 0.38)),
        }
    }
}

pub(super) fn round_art(renderer: &mut Painter, color: [u8; 3]) -> Option<TextureBuffer<Tex>> {
    let patch = drifter_core::paint::round_patch(ROUND_PATCH, color);
    upload_flat(renderer, Some(&patch), "round fill").ok().flatten()
}

impl State {
    fn pane_layers(&self, pane: Pane) -> Option<(Vec<Layer>, Rect)> {
        let one = |art: &TextureBuffer<Tex>, rect: Rect, shadow: Shadow| Some((vec![Layer::new(art, rect, shadow)], rect));
        match pane {
            Pane::Menu => self.popover.as_ref().and_then(|open| Some((open.art.as_ref()?, open.rect))).and_then(|(art, rect)| one(art, rect, Shadow::Menu)),
            Pane::DeskMenu => {
                let open = self.desk_menu.as_ref()?;
                let frame = open.arts.iter().map(|(_, rect)| *rect).reduce(|a, b| {
                    let origin = a.origin.min(b.origin);
                    Rect::new(origin, a.max().max(b.max()) - origin)
                })?;
                let layers = open.arts.iter().rev().map(|(art, rect)| Layer::new(art, *rect, Shadow::Menu)).collect();
                Some((layers, frame))
            }
            Pane::Tip => self.tip.as_ref().and_then(|(art, rect)| one(art, *rect, Shadow::Tip)),
            Pane::Search => {
                let (parts, frame) = self.search_layers()?;
                let mut layers: Vec<Layer> = parts
                    .into_iter()
                    .map(|(art, source, rect)| Layer {
                        source: Some(source),
                        ..Layer::new(&art, rect, Shadow::None)
                    })
                    .collect();
                if let Some(first) = layers.first_mut() {
                    first.shadow = Shadow::Modal;
                    first.shade = Some(frame);
                }
                Some((layers, frame))
            }
            Pane::Launchpad => self.launchpad_layers(),
            Pane::Control => self.control_layers(),
            Pane::Pair => self.pairing_layers(),
            Pane::Pill => self.pill_layers(),
            Pane::Share => self.share_layers(),
            Pane::Notes => self.notes_panel.as_ref().and_then(|(art, rect, _)| one(art, *rect, Shadow::Menu)),
            Pane::Charges => self.charges.as_ref().and_then(|panel| one(panel.art.as_ref()?, panel.rect, Shadow::Menu)),
            Pane::Glance => self.glance_panel.as_ref().and_then(|panel| one(panel.art.as_ref()?, panel.rect, Shadow::Menu)),
            Pane::GlanceCard => self.glance_card.as_ref().and_then(|(art, rect)| one(art, *rect, Shadow::Modal)),
            Pane::Hud => self.hud_layers(),
            Pane::Card(id) => {
                let (card, rect) = self.cards.iter().zip(self.card_shown()).find(|(card, _)| card.note.id == id)?;
                one(&card.art, rect, Shadow::Menu)
            }
            Pane::Switch => {
                let open = self.switch.as_ref()?;
                let art = open.art.as_ref()?;
                let panel = open.layout.panel;
                let mut layers = Vec::new();
                for (index, icon) in open.icons.iter().enumerate() {
                    if let Some(icon) = icon {
                        let rect = open.layout.icon_rect(index);
                        layers.push(Layer::new(icon, Rect::new(panel.origin + rect.origin, rect.size), Shadow::None));
                    }
                }
                if let (Some((ring, shown)), Some(patch)) = (self.switch_ring(), self.round.as_ref()) {
                    let (_, alpha) = open.layout.pick_style();
                    for (source, target) in drifter_core::window::round_slices(ring, ROUND_PATCH, 12.0) {
                        layers.push(Layer {
                            source: Some(source),
                            alpha: alpha as f32 * shown,
                            ..Layer::new(patch, target, Shadow::None)
                        });
                    }
                }
                layers.push(Layer::new(art, panel, Shadow::Modal));
                Some((layers, panel))
            }
            Pane::Alert => {
                let open = self.alert.as_ref()?;
                let art = open.art.as_ref()?;
                let mut layers = Vec::new();
                if let (Some(pill), Some(left)) = (open.pill.as_ref(), self.alert_left()) {
                    let drain = self.alert_look().drain();
                    let width = drain.size.x * left;
                    if width >= 0.5 {
                        let source = Rect::new(Vec2::ZERO, Vec2::new(width, drain.size.y));
                        layers.push(Layer {
                            source: Some(source),
                            ..Layer::new(pill, Rect::new(open.rect.origin + drain.origin, source.size), Shadow::None)
                        });
                    }
                }
                layers.push(Layer::new(art, open.rect, Shadow::Modal));
                Some((layers, open.rect))
            }
            Pane::Prompt => {
                let (art, rect) = self.prompt_art.as_ref()?;
                let mut layers = vec![Layer::new(art, *rect, Shadow::Modal)];
                if let Some(haze) = self.prompt_haze.as_ref() {
                    layers.push(Layer {
                        scales: false,
                        ..Layer::new(haze, Rect::new(Vec2::ZERO, self.screen), Shadow::None)
                    });
                }
                Some((layers, *rect))
            }
        }
    }

    fn hud_layers(&self) -> Option<(Vec<Layer>, Rect)> {
        let hud = self.hud.as_ref()?;
        let mut layers = Vec::new();
        if let (Some(pill), Some(level)) = (self.hud_pill.as_ref(), self.hud_level()) {
            let plain = drifter_core::hud::Look::themed(&self.palette);
            let bar = match hud.icon.is_some() {
                true => plain.named().bar(),
                false => plain.bar(),
            };
            let at = hud.rect.origin + bar.origin;
            let width = bar.size.x * level;
            let cap = 3.0_f64.min(width);
            if width >= 0.5 {
                let part = |source: Rect, x: f64| Layer {
                    source: Some(source),
                    ..Layer::new(pill, Rect::new(at + Vec2::new(x, 0.0), source.size), Shadow::None)
                };
                if width > cap {
                    layers.push(part(Rect::new(Vec2::ZERO, Vec2::new(width - cap, bar.size.y)), 0.0));
                }
                layers.push(part(
                    Rect::new(Vec2::new(bar.size.x - cap, 0.0), Vec2::new(cap, bar.size.y)),
                    width - cap,
                ));
            }
        }
        if let Some((icon, since)) = hud.icon.as_ref() {
            let look = drifter_core::hud::Look::themed(&self.palette).named();
            let spot = look.icon_rect();
            let (alpha, scale) = State::hud_icon_look(*since, self.window_pace, self.window_still);
            let centre = hud.rect.origin + spot.center();
            layers.push(Layer {
                alpha: alpha as f32,
                ..Layer::new(icon, Rect::from_center(centre, spot.size * scale), Shadow::None)
            });
        }
        layers.push(Layer::new(&hud.art, hud.rect, Shadow::Menu));
        Some((layers, hud.rect))
    }

    pub(super) fn hud_level(&self) -> Option<f64> {
        let hud = self.hud.as_ref()?;
        let pace = self.window_pace;
        if pace <= 0.0 || self.window_still {
            return Some(hud.to);
        }
        let t = hud.since.elapsed().as_secs_f64() / (drifter_core::hud::EASE / pace);
        Some(drifter_core::hud::eased_level(hud.from, hud.to, t))
    }

    pub(super) fn hud_busy(&self) -> bool {
        let pace = self.window_pace;
        pace > 0.0
            && !self.window_still
            && self
                .hud
                .as_ref()
                .is_some_and(|hud| hud.from != hud.to && hud.since.elapsed().as_secs_f64() < drifter_core::hud::EASE / pace)
    }

    pub(super) fn watch_panes(&mut self, now: Instant) {
        let mut seen: Vec<Pane> = Pane::FIXED.to_vec();
        seen.extend(self.cards.iter().map(|card| Pane::Card(card.note.id)));
        let pace = self.window_pace;
        for pane in seen.iter().copied() {
            match self.pane_layers(pane) {
                Some((layers, frame)) => {
                    let look = self.fades.panes.get(&pane).map(|fader| self.pane_look(pane, fader, now).0);
                    let fader = self.fades.panes.entry(pane).or_insert_with(|| Fader {
                        born: now,
                        last: Vec::new(),
                        frame,
                        gone: None,
                    });
                    if fader.gone.take().is_some() {
                        let shown = f64::from(look.unwrap_or(0.0));
                        fader.born = now - Duration::from_secs_f64(shown * FADE_IN / pace.max(0.01));
                    }
                    fader.last = layers;
                    fader.frame = frame;
                }
                None => {
                    if let Some(fader) = self.fades.panes.get_mut(&pane)
                        && fader.gone.is_none()
                    {
                        fader.gone = Some(now);
                    }
                }
            }
        }
        let cards: Vec<u32> = self.cards.iter().map(|card| card.note.id).collect();
        for (pane, fader) in self.fades.panes.iter_mut() {
            if let Pane::Card(id) = pane
                && !cards.contains(id)
                && fader.gone.is_none()
            {
                fader.gone = Some(now);
            }
        }
        self.fades.panes.retain(|_, fader| match fader.gone {
            None => true,
            Some(gone) => pace > 0.0 && now.duration_since(gone).as_secs_f64() < FADE_OUT / pace,
        });
    }

    fn pane_look(&self, pane: Pane, fader: &Fader, now: Instant) -> (f32, f64) {
        let pace = self.window_pace;
        if pace <= 0.0 {
            return (1.0, 1.0);
        }
        let (alpha, scale) = match fader.gone {
            Some(gone) => vanish(now.duration_since(gone).as_secs_f64() / (FADE_OUT / pace), self.window_still),
            None if pane.grows_itself() || pane == Pane::Pill => (1.0, 1.0),
            None if pane == Pane::Share => {
                let t = (now.duration_since(fader.born).as_secs_f64() / (State::share_in() / pace)).clamp(0.0, 1.0);
                State::share_look_in(t, self.window_still)
            }
            None if pane == Pane::Pair => {
                let t = (now.duration_since(fader.born).as_secs_f64() / (State::pairing_in() / pace)).clamp(0.0, 1.0);
                State::pairing_look(t, self.window_still)
            }
            None if pane == Pane::Alert => {
                let t = (now.duration_since(fader.born).as_secs_f64() / (ALERT_IN / pace)).clamp(0.0, 1.0);
                let grow = match pace >= 1.0 {
                    true => drifter_core::drawer::ease(t),
                    false => drifter_core::motion::smooth(t),
                };
                let scale = match self.window_still {
                    true => 1.0,
                    false => ALERT_FROM_SCALE + (1.0 - ALERT_FROM_SCALE) * grow,
                };
                (drifter_core::motion::smooth(t), scale)
            }
            None => appear(now.duration_since(fader.born).as_secs_f64() / (FADE_IN / pace), self.window_still),
        };
        (alpha as f32, scale)
    }

    pub(super) fn drop_ghosts(&mut self) {
        self.watch_panes(Instant::now());
        self.fades.panes.retain(|_, fader| fader.gone.is_none());
    }

    pub(super) fn fades_busy(&self) -> bool {
        let now = Instant::now();
        self.fades.panes.iter().any(|(pane, fader)| {
            fader.gone.is_some() || (!pane.grows_itself() && self.pane_look(*pane, fader, now).0 < 1.0)
        })
    }

    pub(super) fn pane_elements(&self, pane: Pane, elements: &mut Vec<Skin>) {
        let now = Instant::now();
        let fader = self.fades.panes.get(&pane);
        let live = match pane.grows_itself() {
            true => None,
            false => self.pane_layers(pane),
        };
        let (layers, frame) = match (live, fader) {
            (Some(live), _) => live,
            (None, Some(fader)) if fader.gone.is_some() => (fader.last.clone(), fader.frame),
            _ => return,
        };
        let (alpha, scale) = fader.map_or((1.0, 1.0), |fader| self.pane_look(pane, fader, now));
        if alpha <= 0.0 {
            return;
        }
        let centre = frame.center();
        self.lit_elements(pane, centre, scale, alpha, elements);
        let place = |rect: Rect, scales: bool| match scales && scale != 1.0 {
            true => Rect::new(centre + (rect.origin - centre) * scale, rect.size * scale),
            false => rect,
        };
        let mut shadows = Vec::new();
        for layer in &layers {
            let drawn = place(layer.rect, layer.scales);
            let shown = alpha * layer.alpha;
            if shown <= 0.0 || drawn.size.x <= 0.0 || drawn.size.y <= 0.0 {
                continue;
            }
            match layer.source {
                Some(source) => elements.push(Skin::Texture(faded(&layer.art, source, drawn, shown))),
                None => elements.push(Skin::Texture(element(&layer.art, drawn, shown))),
            }
            shadows.push((layer.shade.map_or(drawn, |rect| place(rect, layer.scales)), layer.shadow));
        }
        for (shade, shadow) in shadows {
            self.panel_shadow(shade, shadow, alpha, elements);
        }
    }

    pub(super) fn card_elements(&self, elements: &mut Vec<Skin>) {
        for card in &self.cards {
            self.pane_elements(Pane::Card(card.note.id), elements);
        }
        for pane in self.fades.panes.keys() {
            if let Pane::Card(id) = pane
                && !self.cards.iter().any(|card| card.note.id == *id)
            {
                self.pane_elements(*pane, elements);
            }
        }
    }

    pub(super) fn round_fill(
        &self,
        patch: &TextureBuffer<Tex>,
        radius: f64,
        rect: Rect,
        alpha: f32,
        elements: &mut Vec<Skin>,
    ) {
        if alpha <= 0.0 {
            return;
        }
        for (source, target) in drifter_core::window::round_slices(rect, ROUND_PATCH, radius) {
            elements.push(Skin::Texture(faded(patch, source, target, alpha)));
        }
    }

    pub(super) fn panel_shadow(&self, rect: Rect, kind: Shadow, alpha: f32, elements: &mut Vec<Skin>) {
        let (Some((zoom, drop, dark)), Some((buffer, corner))) = (kind.cast(), self.window_shadow.as_ref()) else {
            return;
        };
        if alpha <= 0.0 {
            return;
        }
        for (source, target) in drifter_core::window::shadow_slices(rect, *corner, zoom, drop) {
            elements.push(Skin::Texture(faded(buffer, source, target, dark * alpha)));
        }
    }

    pub(super) fn rebake_rounds(&mut self) {
        let pill = drifter_core::hud::Look::themed(&self.palette).pill();
        self.hud_pill = upload(&mut self.renderer, Some(&pill), "hud bar").ok().flatten();
        self.round = round_art(&mut self.renderer, self.palette.shade);
        self.accent_round = round_art(&mut self.renderer, self.look.accent);
    }
}
