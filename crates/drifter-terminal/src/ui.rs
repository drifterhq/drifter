use drifter_core::menu::{Item, Look};
use drifter_core::{Font, Image, Rect, Vec2};

use crate::config::Edge;

pub const STRIP: f64 = 36.0;
pub const CARD_MARGIN: f64 = 0.0;
pub const CARD_RADIUS: f64 = 10.0;
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
    Rename(i32),
    MoveOut(i32),
    CloseOne(i32),
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

pub fn window_for(edge: Edge, tabs: bool, text: (f64, f64), most: (u32, u32)) -> (u32, u32) {
    let (mut width, mut height) = text;
    if tabs {
        match edge.vertical() {
            true => width += STRIP * 4.0,
            false => height += STRIP,
        }
    }
    let width = (width.ceil() as u32).clamp(320, most.0.max(320));
    let height = (height.ceil() as u32).clamp(200, most.1.max(200));
    (width, height)
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

pub fn tab_centers(edge: Edge, count: usize, size: (u32, u32)) -> Vec<f64> {
    tab_rects(edge, count, size)
        .into_iter()
        .map(|rect| match edge.vertical() {
            true => rect.center().y,
            false => rect.center().x,
        })
        .collect()
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

pub fn context_menu(look: Look, at: Vec2, screen: Vec2, font: &Font, copy: bool) -> Menu {
    let copy_row = match copy {
        true => (Item::line(4, "Copy"), Some(Action::Copy)),
        false => (Item::line(4, "Copy").off(), None),
    };
    build(
        look,
        at,
        screen,
        font,
        vec![
            (Item::line(1, "New Tab"), Some(Action::NewTab)),
            (Item::line(2, "New Window"), Some(Action::NewWindow)),
            (Item::line(3, "Close Tab"), Some(Action::CloseTab)),
            (Item::rule(90), None),
            copy_row,
            (Item::line(5, "Paste"), Some(Action::Paste)),
            (Item::rule(91), None),
            (Item::line(6, "Settings"), Some(Action::Settings)),
        ],
    )
}

pub fn tab_menu(look: Look, at: Vec2, screen: Vec2, font: &Font, fd: i32, many: bool) -> Menu {
    let out = match many {
        true => (Item::line(12, "Move to New Window"), Some(Action::MoveOut(fd))),
        false => (Item::line(12, "Move to New Window").off(), None),
    };
    build(
        look,
        at,
        screen,
        font,
        vec![
            (Item::line(11, "Rename Tab"), Some(Action::Rename(fd))),
            out,
            (Item::rule(92), None),
            (Item::line(13, "Close Tab"), Some(Action::CloseOne(fd))),
        ],
    )
}

fn build(look: Look, at: Vec2, screen: Vec2, font: &Font, rows: Vec<(Item, Option<Action>)>) -> Menu {
    let items: Vec<Item> = rows.iter().map(|(item, _)| item.clone()).collect();
    let actions: Vec<Option<Action>> = rows.iter().map(|(_, a)| *a).collect();
    let size = look.size(&items, Some(font), Some(font));
    let rect = look.rect(at, size, screen);
    let art = look.paint(&items, size, None, Some(font), Some(font));
    Menu { items, actions, look, rect, size, art }
}
