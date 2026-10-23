use super::fade::Pane;
use super::*;

const LIT_SLIDE: f64 = 0.12;
const LIT_FADE: f64 = 0.10;
const LIT_ALPHA: f64 = 0.07;
const LIT_RADIUS: f64 = 9.0;
const PRESS_ALPHA: f64 = 0.16;
const PRESS_HOLD: f64 = 0.08;
const PRESS_FADE: f64 = 0.12;

pub(super) struct Lit {
    pub pane: Pane,
    from: Rect,
    pub to: Rect,
    since: Instant,
    alpha: (f64, f64, Instant),
    pressed: Option<Instant>,
}

impl State {
    fn row_under(&self) -> Option<(Pane, Rect)> {
        let pointer = self.pointer;
        if let Some(open) = self.popover.as_ref().filter(|open| open.art.is_some() && open.rect.contains(pointer)) {
            let look = self.menu_look();
            let at = look.item_at(&open.items, open.rect.size, pointer - open.rect.origin)?;
            let row = look.lit_rect(&open.items, open.rect.size, at);
            return Some((Pane::Menu, Rect::new(open.rect.origin + row.origin, row.size)));
        }
        if let Some(pad) = self.launchpad.as_ref().filter(|pad| pad.art.is_some() && pad.rect.contains(pointer)) {
            let look = drifter_core::launchpad::Look::themed(&self.palette);
            let size = pad.rect.size;
            let per = look.per_page(size);
            let on_page = pad.shown.len().saturating_sub(pad.page * per);
            let slot = look.slot_at(size, pointer - pad.rect.origin, on_page)?;
            let cell = look.cell_rect(size, slot);
            return Some((Pane::Launchpad, Rect::new(pad.rect.origin + cell.origin, cell.size)));
        }
        if let Some((open, rect)) = self.search.as_ref().and_then(|open| Some((open, open.art.as_ref()?.1))) {
            let look = self.search_look();
            let at = look.row_at(pointer - rect.origin, open.results.len()).filter(|at| *at != open.selected)?;
            let row = look.row_rect(at);
            return Some((Pane::Search, Rect::new(rect.origin + row.origin, row.size)));
        }
        if let Some(open) = self.control.as_ref().filter(|open| open.art.is_some() && open.rect.contains(pointer)) {
            let hit = self.control_look().hit_rect(pointer - open.rect.origin, self.shown_items())?;
            return Some((Pane::Control, Rect::new(open.rect.origin + hit.origin, hit.size)));
        }
        if let Some(panel) = self.glance_panel.as_ref().filter(|panel| panel.art.is_some()) {
            let at = panel.hover.filter(|at| panel.open != Some(*at))?;
            let row = self.glance_layout().row_rect(at, panel.open);
            return Some((Pane::Glance, Rect::new(panel.rect.origin + row.origin, row.size)));
        }
        self.cards
            .iter()
            .zip(self.card_shown())
            .find(|(_, rect)| rect.contains(pointer))
            .map(|(card, rect)| (Pane::Card(card.note.id), rect))
    }

    pub(super) fn light_row(&mut self, pane: Pane, rect: Rect) {
        let now = Instant::now();
        let drawn = self.lit_rect();
        match self.lit.as_mut() {
            Some(lit) if lit.pane == pane && drawn.is_some() => {
                lit.from = drawn.map_or(rect, |(_, at, _)| at);
                lit.to = rect;
                lit.since = now;
                if lit.alpha.1 < 1.0 {
                    lit.alpha = (shown(lit.alpha, self.window_pace), 1.0, now);
                }
            }
            _ => {
                self.lit = Some(Lit {
                    pane,
                    from: rect,
                    to: rect,
                    since: now,
                    alpha: (0.0, 1.0, now),
                    pressed: None,
                });
            }
        }
    }

    pub(super) fn hover_rows(&mut self) {
        let now = Instant::now();
        let target = self.row_under();
        let drawn = self.lit_rect();
        match (target, self.lit.as_mut()) {
            (Some((pane, rect)), Some(lit)) if lit.pane == pane && drawn.is_some() => {
                if lit.to != rect {
                    lit.from = drawn.map_or(rect, |(_, at, _)| at);
                    lit.to = rect;
                    lit.since = now;
                }
                if lit.alpha.1 < 1.0 {
                    lit.alpha = (shown(lit.alpha, self.window_pace), 1.0, now);
                }
            }
            (Some((pane, rect)), _) => {
                self.lit = Some(Lit {
                    pane,
                    from: rect,
                    to: rect,
                    since: now,
                    alpha: (0.0, 1.0, now),
                    pressed: None,
                });
            }
            (None, Some(lit)) if lit.alpha.1 > 0.0 => {
                lit.alpha = (shown(lit.alpha, self.window_pace), 0.0, now);
            }
            (None, _) => {}
        }
    }

    pub(super) fn press_lit(&mut self) {
        if let Some(lit) = self.lit.as_mut() {
            lit.pressed = Some(Instant::now());
        }
        self.kick_now();
    }

    fn pressed_boost(&self, lit: &Lit) -> Option<f64> {
        let pace = self.window_pace;
        let at = lit.pressed?;
        if pace <= 0.0 {
            return None;
        }
        let t = at.elapsed().as_secs_f64() * pace;
        match t {
            t if t < PRESS_HOLD => Some(PRESS_ALPHA / LIT_ALPHA),
            t if t < PRESS_HOLD + PRESS_FADE => {
                let fade = drifter_core::motion::smooth((t - PRESS_HOLD) / PRESS_FADE);
                Some(PRESS_ALPHA / LIT_ALPHA * (1.0 - fade))
            }
            _ => None,
        }
    }

    pub(super) fn lit_rect(&self) -> Option<(Pane, Rect, f32)> {
        let lit = self.lit.as_ref()?;
        let boost = self.pressed_boost(lit);
        let open = match lit.pane {
            Pane::Launchpad => self.launchpad.is_some(),
            Pane::Search => self.search.is_some(),
            Pane::Control => self.control.is_some(),
            Pane::Menu => self.popover.is_some() || boost.is_some(),
            Pane::Glance => self.glance_panel.is_some(),
            Pane::Card(id) => self.cards.iter().any(|card| card.note.id == id),
            _ => false,
        };
        let alpha = shown(lit.alpha, self.window_pace);
        if !open || (alpha <= 0.0 && boost.is_none()) {
            return None;
        }
        let pace = self.window_pace;
        let rect = match pace > 0.0 && !self.window_still {
            true => {
                let t = lit.since.elapsed().as_secs_f64() / (LIT_SLIDE / pace);
                drifter_core::motion::between(lit.from, lit.to, drifter_core::motion::smooth(t))
            }
            false => lit.to,
        };
        let shade = match boost {
            Some(boost) => LIT_ALPHA * boost.max(alpha),
            None => alpha * LIT_ALPHA,
        };
        Some((lit.pane, rect, shade as f32))
    }

    pub(super) fn lit_moving(&self) -> bool {
        let Some(lit) = self.lit.as_ref() else {
            return false;
        };
        let pace = self.window_pace;
        if pace <= 0.0 || self.lit_rect().is_none() {
            return false;
        }
        lit.since.elapsed().as_secs_f64() < LIT_SLIDE / pace
            || lit.alpha.2.elapsed().as_secs_f64() < LIT_FADE / pace
            || self.pressed_boost(lit).is_some()
    }

    pub(super) fn lit_elements(&self, pane: Pane, centre: Vec2, scale: f64, lift: f64, alpha: f32, elements: &mut Vec<Skin>) {
        let (Some((lit, rect, shade)), Some(patch)) = (self.lit_rect(), self.round.as_ref()) else {
            return;
        };
        if lit != pane {
            return;
        }
        let drawn = Rect::new(centre + (rect.origin - centre) * scale - Vec2::new(0.0, lift), rect.size * scale);
        self.round_fill(patch, LIT_RADIUS * scale, drawn, shade * alpha, elements);
    }
}

fn shown((from, to, since): (f64, f64, Instant), pace: f64) -> f64 {
    if pace <= 0.0 {
        return to;
    }
    let t = since.elapsed().as_secs_f64() / (LIT_FADE / pace);
    from + (to - from) * drifter_core::motion::smooth(t)
}
