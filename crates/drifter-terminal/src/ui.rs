use std::time::Instant;

use drifter_core::menu::{Item, Look};
use drifter_core::motion::{between, smooth};
use drifter_core::{Font, Image, Rect, Vec2};

use crate::config::Edge;

pub const STRIP: f64 = 36.0;
pub const CARD_MARGIN: f64 = 0.0;
pub const CARD_RADIUS: f64 = 10.0;
const SLIDE: f64 = 0.12;
const FADE: f64 = 0.10;
const LIT_ALPHA: f64 = 0.08;
const LIT_RADIUS: f64 = 8.0;
const TAB_RADIUS: f64 = 7.0;
const TAB_GAP: f64 = 4.0;
const TAB_MIN: f64 = 96.0;
const TAB_MAX: f64 = 220.0;

#[derive(Clone, Copy, PartialEq)]
pub enum Action {
    NewTab,
    NewWindow,
    CloseTab,
    Copy,
    Paste,
    Settings,
    SelectTab(usize),
}

pub struct Lit {
    from: Rect,
    to: Rect,
    since: Instant,
    alpha: (f64, f64, Instant),
}

impl Lit {
    pub fn aim(slot: &mut Option<Lit>, target: Option<Rect>) {
        let now = Instant::now();
        match (target, slot.as_mut()) {
            (Some(rect), Some(lit)) => {
                if lit.to != rect {
                    lit.from = lit.current();
                    lit.to = rect;
                    lit.since = now;
                }
                if lit.alpha.1 < 1.0 {
                    lit.alpha = (lit.shown(), 1.0, now);
                }
            }
            (Some(rect), None) => {
                *slot = Some(Lit { from: rect, to: rect, since: now, alpha: (0.0, 1.0, now) });
            }
            (None, Some(lit)) => {
                if lit.alpha.1 > 0.0 {
                    lit.alpha = (lit.shown(), 0.0, now);
                }
            }
            (None, None) => {}
        }
    }

    fn shown(&self) -> f64 {
        let t = (self.alpha.2.elapsed().as_secs_f64() / FADE).min(1.0);
        self.alpha.0 + (self.alpha.1 - self.alpha.0) * smooth(t)
    }

    fn current(&self) -> Rect {
        let t = (self.since.elapsed().as_secs_f64() / SLIDE).min(1.0);
        between(self.from, self.to, smooth(t))
    }

    pub fn draw(&self, canvas: &mut drifter_core::paint::Canvas, ink: [u8; 3]) {
        let a = self.shown();
        if a <= 0.001 {
            return;
        }
        canvas.round_rect(self.current(), LIT_RADIUS, ink, a * LIT_ALPHA);
    }

    pub fn busy(&self) -> bool {
        self.since.elapsed().as_secs_f64() < SLIDE
            || self.alpha.2.elapsed().as_secs_f64() < FADE
            || self.shown() > 0.001 && self.alpha.1 > 0.0
    }
}

pub fn strip_rect(edge: Edge, size: (u32, u32)) -> Rect {
    let (w, h) = (f64::from(size.0), f64::from(size.1));
    match edge {
        Edge::Top => Rect::new(Vec2::new(0.0, 0.0), Vec2::new(w, STRIP)),
        Edge::Bottom => Rect::new(Vec2::new(0.0, h - STRIP), Vec2::new(w, STRIP)),
        Edge::Left => Rect::new(Vec2::new(0.0, 0.0), Vec2::new(STRIP * 4.0, h)),
        Edge::Right => Rect::new(Vec2::new(w - STRIP * 4.0, 0.0), Vec2::new(STRIP * 4.0, h)),
    }
}

pub fn content_rect(edge: Edge, size: (u32, u32), tabs: bool) -> Rect {
    let (w, h) = (f64::from(size.0), f64::from(size.1));
    if !tabs {
        return Rect::new(Vec2::ZERO, Vec2::new(w, h));
    }
    match edge {
        Edge::Top => Rect::new(Vec2::new(0.0, STRIP), Vec2::new(w, (h - STRIP).max(1.0))),
        Edge::Bottom => Rect::new(Vec2::ZERO, Vec2::new(w, (h - STRIP).max(1.0))),
        Edge::Left => Rect::new(Vec2::new(STRIP * 4.0, 0.0), Vec2::new((w - STRIP * 4.0).max(1.0), h)),
        Edge::Right => Rect::new(Vec2::ZERO, Vec2::new((w - STRIP * 4.0).max(1.0), h)),
    }
}

pub fn card_rect(edge: Edge, size: (u32, u32), tabs: bool) -> Rect {
    let content = content_rect(edge, size, tabs);
    if !tabs {
        return content;
    }
    Rect::new(
        content.origin + Vec2::splat(CARD_MARGIN),
        Vec2::new((content.size.x - CARD_MARGIN * 2.0).max(1.0), (content.size.y - CARD_MARGIN * 2.0).max(1.0)),
    )
}

pub fn tab_rects(edge: Edge, count: usize, size: (u32, u32)) -> Vec<Rect> {
    if count == 0 {
        return Vec::new();
    }
    let strip = strip_rect(edge, size);
    let pad = 5.0;
    let mut rects = Vec::with_capacity(count);
    if edge.vertical() {
        let th = (STRIP - TAB_GAP).max(22.0);
        for i in 0..count {
            let y = strip.origin.y + pad + i as f64 * (th + TAB_GAP);
            rects.push(Rect::new(Vec2::new(strip.origin.x + pad, y), Vec2::new(strip.size.x - pad * 2.0, th)));
        }
    } else {
        let avail = strip.size.x - pad * 2.0;
        let each = ((avail - (count as f64 - 1.0) * TAB_GAP) / count as f64).clamp(TAB_MIN.min(avail), TAB_MAX);
        let th = STRIP - pad * 2.0;
        for i in 0..count {
            let x = strip.origin.x + pad + i as f64 * (each + TAB_GAP);
            rects.push(Rect::new(Vec2::new(x, strip.origin.y + pad), Vec2::new(each, th)));
        }
    }
    rects
}

pub fn tab_at(edge: Edge, count: usize, size: (u32, u32), point: Vec2) -> Option<usize> {
    tab_rects(edge, count, size).into_iter().position(|rect| rect.contains(point))
}

pub struct Menu {
    pub items: Vec<Item>,
    pub actions: Vec<Option<Action>>,
    pub look: Look,
    pub rect: Rect,
    pub size: Vec2,
    pub art: Image,
}

pub fn context_menu(look: Look, at: Vec2, screen: Vec2, font: &Font) -> Menu {
    let rows: Vec<(Item, Option<Action>)> = vec![
        (Item::line(1, "New Tab"), Some(Action::NewTab)),
        (Item::line(2, "New Window"), Some(Action::NewWindow)),
        (Item::line(3, "Close Tab"), Some(Action::CloseTab)),
        (Item::rule(90), None),
        (Item::line(4, "Copy"), Some(Action::Copy)),
        (Item::line(5, "Paste"), Some(Action::Paste)),
        (Item::rule(91), None),
        (Item::line(6, "Settings"), Some(Action::Settings)),
    ];
    let items: Vec<Item> = rows.iter().map(|(item, _)| item.clone()).collect();
    let actions: Vec<Option<Action>> = rows.iter().map(|(_, a)| *a).collect();
    let size = look.size(&items, Some(font), Some(font));
    let rect = look.rect(at, size, screen);
    let art = look.paint(&items, size, None, Some(font), Some(font));
    Menu { items, actions, look, rect, size, art }
}
