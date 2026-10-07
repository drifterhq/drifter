use std::collections::HashMap;

use super::{Group, Kind, memory_words, percent_words};
use crate::geom::{Rect, Vec2};
use crate::glance::Card;
use crate::image::Image;
use crate::paint::{Canvas, logical};
use crate::text::Font;

pub const WIDTH: f64 = 320.0;
pub const SHOWN: usize = 4;
pub const ICON: u32 = 28;
const PAD: f64 = 14.0;
const HEAD: f64 = 26.0;
const ROW: f64 = 44.0;
const ACTS: f64 = 36.0;
const FOOT: f64 = 52.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Top {
    pub key: String,
    pub name: String,
    pub detail: String,
    pub icon: Option<String>,
    pub cpu: f64,
    pub mem_kb: u64,
    pub paused: bool,
    pub can_stop: bool,
    pub pids: Vec<u32>,
    pub root: u32,
    pub flatpak: bool,
}

impl Top {
    pub fn of(group: &Group) -> Top {
        Top {
            key: group.key.clone(),
            name: group.name.clone(),
            detail: group.detail.clone(),
            icon: group.icon.clone(),
            cpu: group.cpu,
            mem_kb: group.mem_kb,
            paused: group.paused,
            can_stop: group.can_stop(),
            pids: group.pids(),
            root: group.root,
            flatpak: group.flatpak,
        }
    }
}

pub fn pick(groups: &[Group], total_kb: u64) -> Vec<Top> {
    let mut apps: Vec<&Group> = groups.iter().filter(|group| group.kind == Kind::App).collect();
    apps.sort_by(|a, b| b.score(total_kb).total_cmp(&a.score(total_kb)));
    apps.into_iter().take(SHOWN).map(Top::of).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hit {
    Row(usize),
    Pause(usize),
    Quit(usize),
    Details(usize),
    Open,
    Reopen,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub card: Card,
    pub rows: usize,
    pub spark: bool,
}

impl Layout {
    fn system(&self) -> f64 {
        self.card.height(self.rows, self.spark) - 10.0
    }

    pub fn height(&self, tops: usize, open: Option<usize>) -> f64 {
        let opened = open.filter(|at| *at < tops).map_or(0.0, |_| ACTS);
        self.system() + HEAD + ROW * tops.max(1) as f64 + opened + FOOT
    }

    fn row_rect(&self, at: usize, open: Option<usize>) -> Rect {
        let mut y = self.system() + HEAD + ROW * at as f64;
        if let Some(open) = open
            && open < at
        {
            y += ACTS;
        }
        let tall = if open == Some(at) { ROW + ACTS } else { ROW };
        Rect::new(Vec2::new(PAD - 6.0, y), Vec2::new(self.card.width - PAD * 2.0 + 12.0, tall))
    }

    fn act_rects(&self, at: usize, open: Option<usize>) -> [Rect; 3] {
        let row = self.row_rect(at, open);
        let left = row.origin.x + 6.0 + 28.0 + 10.0;
        let width = (row.max().x - 6.0 - left - 12.0) / 3.0;
        let y = row.origin.y + ROW - 2.0;
        [0, 1, 2].map(|slot| Rect::new(Vec2::new(left + slot as f64 * (width + 6.0), y), Vec2::new(width, 26.0)))
    }

    fn open_rect(&self, tops: usize, open: Option<usize>, font: &Font) -> Rect {
        let height = self.height(tops, open);
        let width = font.width_of("Open Activity") + 28.0;
        Rect::new(Vec2::new(PAD, height - FOOT + 12.0), Vec2::new(width, 28.0))
    }

    fn reopen_rect(&self, tops: usize, open: Option<usize>, font: &Font, name: &str) -> Rect {
        let button = self.open_rect(tops, open, font);
        let width = font.width_of(&reopen_words(name)) + 16.0;
        Rect::new(Vec2::new(self.card.width - PAD - width, button.origin.y), Vec2::new(width, button.size.y))
    }

    pub fn hit(&self, tops: usize, open: Option<usize>, font: &Font, undo: Option<&str>, at: Vec2) -> Option<Hit> {
        if self.open_rect(tops, open, font).contains(at) {
            return Some(Hit::Open);
        }
        if let Some(name) = undo
            && self.reopen_rect(tops, open, font, name).contains(at)
        {
            return Some(Hit::Reopen);
        }
        for row in 0..tops {
            if open == Some(row) {
                let [pause, quit, details] = self.act_rects(row, open);
                if pause.contains(at) {
                    return Some(Hit::Pause(row));
                }
                if quit.contains(at) {
                    return Some(Hit::Quit(row));
                }
                if details.contains(at) {
                    return Some(Hit::Details(row));
                }
            }
            if self.row_rect(row, open).contains(at) {
                return Some(Hit::Row(row));
            }
        }
        None
    }
}

pub struct Paint<'a> {
    pub font: &'a Font,
    pub bold: &'a Font,
    pub fields: &'a [(String, u8)],
    pub history: &'a [u8],
    pub tops: &'a [Top],
    pub open: Option<usize>,
    pub hover: Option<usize>,
    pub icons: &'a HashMap<String, Image>,
    pub counts: (usize, usize),
    pub accent: [u8; 3],
    pub undo: Option<&'a str>,
}

fn reopen_words(name: &str) -> String {
    let short: String = match name.chars().count() > 16 {
        true => name.chars().take(15).collect::<String>() + "…",
        false => name.to_string(),
    };
    format!("Reopen {short}")
}

pub fn paint(layout: &Layout, what: &Paint) -> Image {
    let card = &layout.card;
    let size = Vec2::new(card.width, layout.height(what.tops.len(), what.open));
    let mut canvas = Canvas::new(size.x as u32, size.y.ceil() as u32);
    canvas.round_rect(Rect::new(Vec2::ZERO, size), 14.0, card.back, card.back_alpha);
    crate::glance::paint_system(&mut canvas, card, what.font, what.bold, what.fields, what.history);
    let head = what.font.paint_weight("Using the most", card.dim, 600.0);
    canvas.stamp(&head, Vec2::new(PAD, layout.system() + HEAD - 6.0 - logical(&head).y).round(), 1.0);
    if what.tops.is_empty() {
        let note = what.font.paint("No apps are open.", card.dim);
        canvas.stamp(&note, Vec2::new(PAD, layout.system() + HEAD + ROW / 2.0 - logical(&note).y / 2.0).round(), 1.0);
    }
    for (at, top) in what.tops.iter().enumerate() {
        let row = layout.row_rect(at, what.open);
        if what.open == Some(at) {
            canvas.round_rect(row, 9.0, card.ink, 0.08);
        } else if what.hover == Some(at) {
            canvas.round_rect(row, 9.0, card.ink, 0.06);
        }
        let mid = row.origin.y + ROW / 2.0;
        let spot = Rect::new(Vec2::new(row.origin.x + 6.0, mid - 14.0), Vec2::splat(28.0));
        let fade = if top.paused { 0.6 } else { 1.0 };
        match top.icon.as_ref().and_then(|name| what.icons.get(name)) {
            Some(image) => canvas.stamp(image, (spot.center() - logical(image) / 2.0).round(), fade),
            None => {
                canvas.round_rect(spot, 7.5, card.ink, 0.16);
                let first: String = top.name.chars().take(1).collect::<String>().to_uppercase();
                let letter = what.bold.paint_bold(&first, card.ink);
                canvas.stamp(&letter, (spot.center() - logical(&letter) / 2.0).round(), fade);
            }
        }
        if top.flatpak {
            super::look::flatpak_badge(&mut canvas, spot, card.back, fade);
        }
        let cpu_words = match top.paused {
            true => "Paused".to_string(),
            false => percent_words(top.cpu),
        };
        let cpu_ink = match top.paused {
            true => crate::glance::COOL,
            false => super::tone(card.ink, top.cpu),
        };
        let cpu = what.font.paint_even_digits(&cpu_words, cpu_ink);
        let mem = what.font.paint_even_digits(&memory_words(top.mem_kb), card.dim);
        let right = row.max().x - 8.0;
        let number_room = logical(&cpu).x.max(logical(&mem).x);
        let left = spot.max().x + 10.0;
        let room = right - number_room - 12.0 - left;
        let name = what.bold.paint_bold(&crate::glance::fit(what.bold, &top.name, room), card.ink);
        let detail = what.font.paint(&crate::glance::fit(what.font, &top.detail, room), card.dim);
        let both = logical(&name).y + logical(&detail).y;
        canvas.stamp(&name, Vec2::new(left, mid - both / 2.0).round(), fade);
        canvas.stamp(&detail, Vec2::new(left, mid - both / 2.0 + logical(&name).y).round(), fade);
        let pair = logical(&cpu).y + logical(&mem).y;
        canvas.stamp(&cpu, Vec2::new(right - logical(&cpu).x, mid - pair / 2.0).round(), 1.0);
        canvas.stamp(&mem, Vec2::new(right - logical(&mem).x, mid - pair / 2.0 + logical(&cpu).y).round(), 1.0);
        if what.open == Some(at) {
            let words = [if top.paused { "Resume" } else { "Pause" }, "Quit", "Details"];
            for (slot, rect) in layout.act_rects(at, what.open).iter().enumerate() {
                let enabled = slot == 2 || top.can_stop;
                canvas.round_rect(*rect, 7.0, card.ink, if enabled { 0.12 } else { 0.05 });
                let label = what.font.paint(words[slot], card.ink);
                canvas.stamp(&label, (rect.center() - logical(&label) / 2.0).round(), if enabled { 1.0 } else { 0.4 });
            }
        }
    }
    let foot = size.y - FOOT;
    canvas.round_rect(Rect::new(Vec2::new(PAD, foot), Vec2::new(card.width - PAD * 2.0, 1.0)), 0.0, card.ink, 0.1);
    let button = layout.open_rect(what.tops.len(), what.open, what.font);
    canvas.round_rect(button, 7.0, what.accent, 1.0);
    let label = what.bold.paint_bold("Open Activity", [255, 255, 255]);
    canvas.stamp(&label, (button.center() - logical(&label) / 2.0).round(), 1.0);
    match what.undo {
        Some(name) => {
            let rect = layout.reopen_rect(what.tops.len(), what.open, what.font, name);
            canvas.round_rect(rect, 7.0, card.ink, 0.1);
            let words = what.font.paint(&reopen_words(name), what.accent);
            canvas.stamp(&words, (rect.center() - logical(&words) / 2.0).round(), 1.0);
        }
        None => {
            let (apps, background) = what.counts;
            let count = what.font.paint(&format!("{apps} apps · {background} in background"), card.dim);
            canvas.stamp(&count, Vec2::new(size.x - PAD - logical(&count).x, button.center().y - logical(&count).y / 2.0).round(), 1.0);
        }
    }
    canvas.image
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout() -> Layout {
        Layout {
            card: Card { width: WIDTH, ..Card::themed(&crate::theme::DARK) },
            rows: 3,
            spark: true,
        }
    }

    #[test]
    fn an_open_row_pushes_the_rows_under_it_down() {
        let layout = layout();
        let shut = layout.row_rect(2, None);
        let pushed = layout.row_rect(2, Some(0));
        assert_eq!(pushed.origin.y - shut.origin.y, ACTS);
        assert_eq!(layout.height(4, Some(1)) - layout.height(4, None), ACTS);
    }

    #[test]
    fn the_buttons_of_an_open_row_hit_before_the_row() {
        let layout = layout();
        let [pause, quit, details] = layout.act_rects(1, Some(1));
        let font = Font::find(None, 12.0);
        let Ok(font) = font else {
            return;
        };
        assert_eq!(layout.hit(4, Some(1), &font, None, pause.center()), Some(Hit::Pause(1)));
        assert_eq!(layout.hit(4, Some(1), &font, None, quit.center()), Some(Hit::Quit(1)));
        assert_eq!(layout.hit(4, Some(1), &font, None, details.center()), Some(Hit::Details(1)));
        assert_eq!(layout.hit(4, Some(1), &font, None, layout.row_rect(3, Some(1)).center()), Some(Hit::Row(3)));
    }
}
