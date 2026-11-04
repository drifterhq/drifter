use std::collections::{HashMap, HashSet};

use super::hour::{Moment, Reading};
use super::{Column, Group, Impact, Kind, Memory, memory_words, percent_words};
use crate::geom::{Rect, Vec2};
use crate::glance::{COOL, HOT, WARM};
use crate::image::Image;
use crate::paint::{Canvas, logical};
use crate::text::Font;

pub const HISTORY: usize = 60;
const BODY_TRIM: f64 = 10.0;
const LAYER_MOST: f64 = 6000.0;
const SIDE_TOP: f64 = 86.0;
const SIDE_HEAD: f64 = 30.0;
const PAD: f64 = 20.0;
const TILE_H: f64 = 92.0;
const HEAD_H: f64 = 30.0;
const ROW_H: f64 = 42.0;
const MEMBER_H: f64 = 26.0;
const MEMBERS_SHOWN: usize = 12;
const HOLD_SECONDS: f64 = 1.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum View {
    Apps,
    Background,
    Everything,
    Hour,
}

impl View {
    pub const ALL: [View; 4] = [View::Apps, View::Background, View::Everything, View::Hour];

    pub fn name(self) -> &'static str {
        match self {
            View::Apps => crate::lang::t("Apps"),
            View::Background => crate::lang::t("Background"),
            View::Everything => crate::lang::t("Everything"),
            View::Hour => crate::lang::t("Last hour"),
        }
    }

    fn group(self) -> &'static str {
        match self {
            View::Hour => crate::lang::t("Look back"),
            _ => crate::lang::t("Running"),
        }
    }

    fn tint(self) -> [u8; 3] {
        match self {
            View::Apps => [64, 128, 238],
            View::Background => [120, 128, 146],
            View::Everything => [94, 102, 128],
            View::Hour => [255, 149, 0],
        }
    }

    pub fn holds(self, group: &Group) -> bool {
        match self {
            View::Apps => matches!(group.kind, Kind::App | Kind::Drifter),
            View::Background => matches!(group.kind, Kind::Background | Kind::System),
            View::Everything => true,
            View::Hour => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Measure {
    Cpu,
    Memory,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Power {
    pub watts: Option<f64>,
    pub left_hours: Option<f64>,
    pub charging: bool,
    pub percent: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Disk {
    pub label: String,
    pub used: u8,
    pub size_gb: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Toast {
    pub words: String,
    pub reopen: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    pub view: View,
    pub groups: Vec<Group>,
    pub order: Vec<String>,
    pub sort: Column,
    pub query: String,
    pub selected: Option<String>,
    pub open: HashSet<String>,
    pub scroll: f64,
    pub history: HashMap<String, Vec<f64>>,
    pub machine: Vec<f64>,
    pub cpu: f64,
    pub cores: usize,
    pub memory: Memory,
    pub pressure: Option<f64>,
    pub power: Option<Power>,
    pub disk: Option<Disk>,
    pub hold: f64,
    pub toast: Option<Toast>,
    pub hour: Vec<Reading>,
    pub moments: Vec<Moment>,
    pub cursor: Option<u64>,
    pub measure: Measure,
    pub now: u64,
    pub hover: Option<String>,
}

impl Default for Model {
    fn default() -> Self {
        Model {
            view: View::Apps,
            groups: Vec::new(),
            order: Vec::new(),
            sort: Column::Impact,
            query: String::new(),
            selected: None,
            open: HashSet::new(),
            scroll: 0.0,
            history: HashMap::new(),
            machine: Vec::new(),
            cpu: 0.0,
            cores: 1,
            memory: Memory::default(),
            pressure: None,
            power: None,
            disk: None,
            hold: 0.0,
            toast: None,
            hour: Vec::new(),
            moments: Vec::new(),
            cursor: None,
            measure: Measure::Cpu,
            now: 0,
            hover: None,
        }
    }
}

impl Model {
    pub fn count(&self, view: View) -> usize {
        self.groups.iter().filter(|group| view.holds(group)).count()
    }

    fn matches(&self, group: &Group) -> bool {
        let query = self.query.trim().to_lowercase();
        query.is_empty()
            || group.name.to_lowercase().contains(&query)
            || group.detail.to_lowercase().contains(&query)
            || group.members.iter().any(|member| member.name.to_lowercase().contains(&query) || member.pid.to_string() == query)
    }

    pub fn arrange(&mut self) {
        let mut shown: Vec<Group> = self.groups.iter().filter(|group| self.view.holds(group) && self.matches(group)).cloned().collect();
        super::sorted(&mut shown, self.sort, self.memory.total_kb);
        self.order = shown.into_iter().map(|group| group.key).collect();
    }

    pub fn shown(&self) -> Vec<&Group> {
        let mut out: Vec<&Group> = self
            .order
            .iter()
            .filter_map(|key| self.groups.iter().find(|group| &group.key == key))
            .filter(|group| self.view.holds(group) && self.matches(group))
            .collect();
        for group in &self.groups {
            if self.view.holds(group) && self.matches(group) && !self.order.contains(&group.key) {
                out.push(group);
            }
        }
        out
    }

    pub fn chosen(&self) -> Option<&Group> {
        let key = self.selected.as_ref()?;
        self.groups.iter().find(|group| &group.key == key)
    }

    pub fn remember(&mut self) {
        let keys: HashSet<&String> = self.groups.iter().map(|group| &group.key).collect();
        self.history.retain(|key, _| keys.contains(key));
        for group in &self.groups {
            let line = self.history.entry(group.key.clone()).or_insert_with(|| vec![0.0; HISTORY]);
            line.push(group.cpu);
            let extra = line.len().saturating_sub(HISTORY);
            line.drain(..extra);
        }
        self.machine.push(self.cpu);
        let extra = self.machine.len().saturating_sub(HISTORY);
        self.machine.drain(..extra);
    }
}

pub struct Fonts<'a> {
    pub body: &'a Font,
    pub small: &'a Font,
    pub title: &'a Font,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub sidebar: f64,
    pub item: f64,
    pub inspector: f64,
    pub radius: f64,
    pub back: [u8; 3],
    pub side: [u8; 3],
    pub card: [u8; 3],
    pub rule: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub accent: [u8; 3],
    pub off: [u8; 3],
    pub alert: [u8; 3],
    pub side_scroll: f64,
}

impl Look {
    pub fn themed(p: &crate::theme::Palette, accent: [u8; 3]) -> Look {
        Look {
            sidebar: 220.0,
            item: 28.0,
            inspector: 264.0,
            radius: 10.0,
            back: p.well,
            side: p.surface,
            card: p.card,
            rule: p.rule,
            ink: p.ink,
            dim: p.dim,
            accent,
            off: p.off,
            alert: p.alert,
            side_scroll: 0.0,
        }
    }

    pub fn least(&self) -> Vec2 {
        Vec2::new(self.sidebar + 540.0 + self.inspector, 540.0)
    }

    pub fn item_rect(&self, index: usize) -> Rect {
        let heads = View::ALL[..=index.min(View::ALL.len() - 1)]
            .iter()
            .enumerate()
            .filter(|(at, view)| *at == 0 || View::ALL[at - 1].group() != view.group())
            .count();
        Rect::new(
            Vec2::new(10.0, SIDE_TOP + heads as f64 * SIDE_HEAD + index as f64 * (self.item + 2.0)),
            Vec2::new(self.sidebar - 20.0, self.item),
        )
    }

    pub fn search_rect(&self) -> Rect {
        Rect::new(Vec2::new(12.0, 44.0), Vec2::new(self.sidebar - 24.0, 28.0))
    }

    fn main(&self, size: Vec2, view: View) -> Rect {
        let right = match view {
            View::Hour => size.x,
            _ => size.x - self.inspector,
        };
        Rect::new(Vec2::new(self.sidebar, 0.0), Vec2::new(right - self.sidebar, size.y))
    }

    fn tiles(&self, size: Vec2, view: View) -> Vec<Rect> {
        let main = self.main(size, view);
        let width = (main.size.x - PAD * 2.0 - 10.0 * 3.0) / 4.0;
        (0..4)
            .map(|at| Rect::new(Vec2::new(main.origin.x + PAD + at as f64 * (width + 10.0), PAD), Vec2::new(width, TILE_H)))
            .collect()
    }

    pub fn table_rect(&self, size: Vec2, view: View) -> Rect {
        let main = self.main(size, view);
        let top = PAD + TILE_H + 14.0;
        Rect::new(Vec2::new(main.origin.x + PAD, top), Vec2::new(main.size.x - PAD * 2.0, size.y - top - PAD))
    }

    fn body_rect(&self, size: Vec2, view: View) -> Rect {
        let table = self.table_rect(size, view);
        Rect::new(table.origin + Vec2::new(0.0, HEAD_H), table.size - Vec2::new(0.0, HEAD_H))
    }

    fn columns(&self, table: Rect) -> [(Column, f64, f64, bool); 4] {
        let right = table.max().x - 12.0;
        let spark = right - 72.0;
        let mem = spark - 14.0;
        let cpu = mem - 84.0;
        let impact = cpu - 70.0 - 84.0;
        [
            (Column::Name, table.origin.x + 70.0, impact - table.origin.x - 80.0, false),
            (Column::Impact, impact, 84.0, false),
            (Column::Cpu, cpu, 0.0, true),
            (Column::Memory, mem, 0.0, true),
        ]
    }

    fn spark_x(&self, table: Rect) -> f64 {
        table.max().x - 12.0 - 72.0
    }

    pub fn rows(&self, model: &Model) -> Vec<(f64, f64, Line)> {
        let mut out = Vec::new();
        let mut y = 0.0;
        for group in model.shown() {
            out.push((y, ROW_H, Line::Group(group.key.clone())));
            y += ROW_H;
            if model.open.contains(&group.key) && group.members.len() > 1 {
                for (at, _) in group.members.iter().enumerate().take(MEMBERS_SHOWN) {
                    out.push((y, MEMBER_H, Line::Member(group.key.clone(), at)));
                    y += MEMBER_H;
                }
                if group.members.len() > MEMBERS_SHOWN {
                    out.push((y, MEMBER_H, Line::More(group.key.clone(), group.members.len() - MEMBERS_SHOWN)));
                    y += MEMBER_H;
                }
            }
        }
        out
    }

    pub fn most_scroll(&self, size: Vec2, model: &Model) -> f64 {
        let whole = self.rows(model).last().map_or(0.0, |(y, h, _)| y + h);
        (whole.min(LAYER_MOST) - (self.body_rect(size, model.view).size.y - BODY_TRIM)).max(0.0)
    }

    pub fn over_list(&self, size: Vec2, view: View, at: Vec2) -> bool {
        view != View::Hour && self.table_rect(size, view).contains(at)
    }

    fn inspector_rect(&self, size: Vec2) -> Rect {
        Rect::new(Vec2::new(size.x - self.inspector, 0.0), Vec2::new(self.inspector, size.y))
    }

    fn action_rects(&self, size: Vec2) -> (Rect, Rect, Rect) {
        let panel = self.inspector_rect(size);
        let pad = 16.0;
        let width = panel.size.x - pad * 2.0;
        let force = Rect::new(Vec2::new(panel.origin.x + pad, size.y - pad - 30.0), Vec2::new(width, 30.0));
        let half = (width - 8.0) / 2.0;
        let pause = Rect::new(Vec2::new(force.origin.x, force.origin.y - 38.0), Vec2::new(half, 30.0));
        let quit = Rect::new(Vec2::new(force.origin.x + half + 8.0, pause.origin.y), Vec2::new(half, 30.0));
        (pause, quit, force)
    }

    pub fn hold_rect(&self, size: Vec2) -> Rect {
        self.action_rects(size).2
    }

    fn toast_rect(&self, size: Vec2, model: &Model, fonts: &Fonts) -> Option<(Rect, Option<Rect>)> {
        let toast = model.toast.as_ref()?;
        let main = self.main(size, model.view);
        let words = fonts.body.width_of(&toast.words);
        let button = if toast.reopen { fonts.body.width_of(crate::lang::t("Reopen")) + 24.0 } else { 0.0 };
        let width = words + 32.0 + button;
        let rect = Rect::new(Vec2::new(main.center().x - width / 2.0, size.y - PAD - 44.0), Vec2::new(width, 36.0));
        let reopen = toast.reopen.then(|| Rect::new(Vec2::new(rect.max().x - button - 6.0, rect.origin.y + 5.0), Vec2::new(button, 26.0)));
        Some((rect, reopen))
    }

    fn segment_rects(&self, size: Vec2) -> [(Measure, Rect); 2] {
        let main = self.main(size, View::Hour);
        let width = 84.0;
        let right = main.max().x - PAD;
        let y = PAD + 2.0;
        [
            (Measure::Cpu, Rect::new(Vec2::new(right - width * 2.0 - 4.0, y), Vec2::new(width, 26.0))),
            (Measure::Memory, Rect::new(Vec2::new(right - width - 2.0, y), Vec2::new(width, 26.0))),
        ]
    }

    fn chart_card(&self, size: Vec2) -> Rect {
        let main = self.main(size, View::Hour);
        let top = PAD + 60.0;
        Rect::new(Vec2::new(main.origin.x + PAD, top), Vec2::new(main.size.x - PAD * 2.0, size.y - top - PAD))
    }

    fn chart_area(&self, size: Vec2) -> Rect {
        let card = self.chart_card(size);
        let read = 250.0;
        Rect::new(card.origin + Vec2::new(18.0, 22.0), Vec2::new(card.size.x - read - 18.0 - 50.0, card.size.y - 22.0 - 76.0))
    }

    pub fn lit_target(&self, size: Vec2, model: &Model, fonts: &Fonts, at: Vec2) -> Option<Rect> {
        for (index, view) in View::ALL.iter().enumerate() {
            let rect = self.item_rect(index);
            if rect.contains(at) {
                return (*view != model.view).then_some(rect);
            }
        }
        if let Some((_, Some(reopen))) = self.toast_rect(size, model, fonts)
            && reopen.contains(at)
        {
            return Some(reopen);
        }
        if model.view == View::Hour {
            return self
                .segment_rects(size)
                .into_iter()
                .find(|(measure, rect)| rect.contains(at) && *measure != model.measure)
                .map(|(_, rect)| rect);
        }
        if self.inspector_rect(size).contains(at) {
            if !model.chosen()?.can_stop() {
                return None;
            }
            let (pause, quit, force) = self.action_rects(size);
            return [pause, quit, force].into_iter().find(|rect| rect.contains(at));
        }
        let table = self.table_rect(size, model.view);
        if !table.contains(at) {
            return None;
        }
        if at.y < table.origin.y + HEAD_H {
            return self.columns(table).into_iter().find_map(|(_, x, width, right)| {
                let (from, to) = match right {
                    true => (x - 70.0, x),
                    false => (x, x + width),
                };
                (at.x >= from && at.x <= to)
                    .then(|| Rect::new(Vec2::new(from - 6.0, table.origin.y + 3.0), Vec2::new(to - from + 12.0, HEAD_H - 6.0)))
            });
        }
        let body = self.body_rect(size, model.view);
        let y = at.y - body.origin.y + model.scroll;
        let (top, tall, line) = self.rows(model).into_iter().find(|(top, tall, _)| y >= *top && y < top + tall)?;
        if matches!(&line, Line::Group(key) if model.selected.as_ref() == Some(key)) {
            return None;
        }
        let row = Rect::new(Vec2::new(table.origin.x, body.origin.y + top - model.scroll), Vec2::new(table.size.x, tall));
        Some(crate::rows::lit_rect(row))
    }

    pub fn hit(&self, size: Vec2, model: &Model, fonts: &Fonts, at: Vec2) -> Option<Hit> {
        if self.search_rect().contains(at) {
            return Some(Hit::Search);
        }
        for (index, view) in View::ALL.iter().enumerate() {
            if self.item_rect(index).contains(at) {
                return Some(Hit::View(*view));
            }
        }
        if let Some((_, Some(reopen))) = self.toast_rect(size, model, fonts)
            && reopen.contains(at)
        {
            return Some(Hit::Reopen);
        }
        if model.view == View::Hour {
            for (measure, rect) in self.segment_rects(size) {
                if rect.contains(at) {
                    return Some(Hit::Measure(measure));
                }
            }
            let area = self.chart_area(size);
            let grown = Rect::new(area.origin - Vec2::new(4.0, 16.0), area.size + Vec2::new(8.0, 32.0));
            if grown.contains(at) {
                return self.reading_at(area, model, at.x).map(Hit::Chart);
            }
            return None;
        }
        if self.inspector_rect(size).contains(at) {
            let group = model.chosen()?;
            if !group.can_stop() {
                return None;
            }
            let (pause, quit, force) = self.action_rects(size);
            return match () {
                _ if pause.contains(at) => Some(Hit::Pause),
                _ if quit.contains(at) => Some(Hit::Quit),
                _ if force.contains(at) => Some(Hit::Force),
                _ => None,
            };
        }
        let table = self.table_rect(size, model.view);
        if !table.contains(at) {
            return None;
        }
        if at.y < table.origin.y + HEAD_H {
            let columns = self.columns(table);
            for (column, x, width, right) in columns.iter() {
                let (from, to) = match right {
                    true => (x - 70.0, *x),
                    false => (*x, x + width),
                };
                if at.x >= from && at.x <= to {
                    return Some(Hit::Sort(*column));
                }
            }
            return None;
        }
        let body = self.body_rect(size, model.view);
        let y = at.y - body.origin.y + model.scroll;
        let (_, _, line) = self.rows(model).into_iter().find(|(top, tall, _)| y >= *top && y < top + tall)?;
        match line {
            Line::Group(key) => match at.x < table.origin.x + 30.0 {
                true => Some(Hit::Caret(key)),
                false => Some(Hit::Row(key)),
            },
            Line::Member(key, _) | Line::More(key, _) => Some(Hit::Row(key)),
        }
    }

    fn reading_at(&self, area: Rect, model: &Model, x: f64) -> Option<usize> {
        if model.hour.is_empty() {
            return None;
        }
        let when = |reading: &Reading| self.time_x(area, model.now, reading.at);
        model
            .hour
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (when(a) - x).abs().total_cmp(&(when(b) - x).abs()))
            .map(|(at, _)| at)
    }

    fn time_x(&self, area: Rect, now: u64, at: u64) -> f64 {
        let span = (super::hour::EVERY_SECONDS * super::hour::KEPT as u64) as f64;
        let back = now.saturating_sub(at) as f64;
        area.max().x - (back / span).clamp(0.0, 1.0) * area.size.x
    }

    pub fn paint(&self, size: Vec2, model: &Model, fonts: &Fonts, icons: &HashMap<String, Image>) -> Image {
        let mut canvas = Canvas::new(size.x.round() as u32, size.y.round() as u32);
        canvas.round_rect(Rect::new(Vec2::ZERO, size), 0.0, self.back, 1.0);
        match model.view {
            View::Hour => self.paint_hour(&mut canvas, size, model, fonts, icons),
            _ => {
                self.paint_tiles(&mut canvas, size, model, fonts);
                self.paint_table(&mut canvas, size, model, fonts, icons);
                self.paint_inspector(&mut canvas, size, model, fonts, icons);
            }
        }
        self.paint_side(&mut canvas, size, model, fonts);
        if let Some((rect, reopen)) = self.toast_rect(size, model, fonts)
            && let Some(toast) = model.toast.as_ref()
        {
            canvas.shadow(rect, 12.0, 18.0, 0.35);
            canvas.round_rect(rect, 12.0, self.card, 1.0);
            canvas.ring(rect, 12.0, 1.0, self.rule, 1.0);
            let words = fonts.body.paint(&toast.words, self.ink);
            canvas.stamp(&words, Vec2::new(rect.origin.x + 16.0, rect.center().y - logical(&words).y / 2.0).round(), 1.0);
            if let Some(button) = reopen {
                canvas.round_rect(button, 7.0, self.accent, 1.0);
                let label = fonts.body.paint_weight(crate::lang::t("Reopen"), [255, 255, 255], 600.0);
                canvas.stamp(&label, (button.center() - logical(&label) / 2.0).round(), 1.0);
            }
        }
        canvas.image
    }

    fn paint_side(&self, canvas: &mut Canvas, size: Vec2, model: &Model, fonts: &Fonts) {
        let side = Rect::new(Vec2::ZERO, Vec2::new(self.sidebar, size.y));
        canvas.round_rect(side, 0.0, self.side, 1.0);
        canvas.round_rect(Rect::new(Vec2::new(self.sidebar - 1.0, 0.0), Vec2::new(1.0, size.y)), 0.0, self.rule, 1.0);
        let heading = fonts.small.paint(crate::lang::t("Activity"), self.dim);
        canvas.stamp(&heading, Vec2::new(22.0, 18.0), 1.0);
        let field = self.search_rect();
        canvas.round_rect(field, 7.0, self.back, 1.0);
        let (words, ink) = match model.query.is_empty() {
            true => (crate::lang::t("Search").to_string(), self.dim),
            false => (format!("{}|", model.query), self.ink),
        };
        let text = fonts.body.paint(&crate::glance::fit(fonts.body, &words, field.size.x - 20.0), ink);
        canvas.stamp(&text, Vec2::new(field.origin.x + 10.0, field.center().y - logical(&text).y / 2.0).round(), 1.0);
        for (index, view) in View::ALL.iter().enumerate() {
            let rect = self.item_rect(index);
            if index == 0 || View::ALL[index - 1].group() != view.group() {
                let words = fonts.small.paint_weight(view.group(), self.dim, 600.0);
                let at = Vec2::new(22.0, rect.origin.y - 8.0 - logical(&words).y);
                canvas.stamp(&words, at.round(), 0.85);
            }
            if *view == model.view {
                canvas.round_rect(rect, 7.0, self.accent, 1.0);
            }
            let icon = Rect::new(Vec2::new(rect.origin.x + 8.0, rect.center().y - 11.0), Vec2::splat(22.0));
            canvas.round_rect(icon, 6.0, view.tint(), 1.0);
            view_glyph(canvas, *view, icon);
            let label = fonts.body.paint(view.name(), self.ink);
            canvas.stamp(&label, Vec2::new(icon.max().x + 10.0, rect.center().y - logical(&label).y / 2.0).round(), 1.0);
            if *view != View::Hour {
                let (ink, alpha) = match *view == model.view {
                    true => ([255, 255, 255], 0.8),
                    false => (self.dim, 0.9),
                };
                let count = fonts.small.paint_even_digits(&model.count(*view).to_string(), ink);
                canvas.stamp(&count, Vec2::new(rect.max().x - 10.0 - logical(&count).x, rect.center().y - logical(&count).y / 2.0).round(), alpha);
            }
        }
    }

    fn tile_frame(&self, canvas: &mut Canvas, rect: Rect, fonts: &Fonts, head: &str, big: &str, unit: &str) -> f64 {
        canvas.round_rect(rect, self.radius, self.card, 1.0);
        let head = fonts.small.paint(head, self.dim);
        canvas.stamp(&head, rect.origin + Vec2::new(12.0, 10.0), 1.0);
        let big = fonts.title.paint_weight(big, self.ink, 600.0);
        let at = rect.origin + Vec2::new(12.0, 26.0);
        canvas.stamp(&big, at.round(), 1.0);
        if !unit.is_empty() {
            let unit = fonts.small.paint(unit, self.dim);
            let base = at.y + logical(&big).y - logical(&unit).y - 3.0;
            canvas.stamp(&unit, Vec2::new(at.x + logical(&big).x + 4.0, base).round(), 1.0);
        }
        at.y + logical(&big).y
    }

    fn paint_tiles(&self, canvas: &mut Canvas, size: Vec2, model: &Model, fonts: &Fonts) {
        let tiles = self.tiles(size, model.view);
        let cores = crate::lang::tf("CPU · {} cores", &[&(model.cores)]);
        let below = self.tile_frame(canvas, tiles[0], fonts, &cores, &percent_words(model.cpu), "");
        let spark = Rect::new(Vec2::new(tiles[0].origin.x + 12.0, below + 2.0), Vec2::new(tiles[0].size.x - 24.0, tiles[0].max().y - below - 10.0));
        if model.machine.len() > 1 {
            paint_spark(canvas, spark, &model.machine, 100.0, false);
        }

        let used = model.memory.used_kb();
        let gb = |kb: u64| kb as f64 / 1024.0 / 1024.0;
        let below = self.tile_frame(canvas, tiles[1], fonts, crate::lang::t("Memory"), &format!("{:.1} GB", gb(used)), "");
        let track = Rect::new(Vec2::new(tiles[1].origin.x + 12.0, below + 6.0), Vec2::new(tiles[1].size.x - 24.0, 4.0));
        meter(canvas, track, model.memory.share(), self.ink, crate::activity::tone(self.ink, model.memory.share() * 100.0));
        let swap = model.memory.swap_used_kb();
        let of = format!("of {:.0} GB", gb(model.memory.total_kb).round());
        let note = match (model.pressure, swap) {
            (Some(p), _) if p >= 10.0 => format!("{of} · tight"),
            (_, 0..1024) => of,
            (_, kb) => format!("{of} · swap {}", memory_words(kb)),
        };
        let note = fonts.small.paint(&crate::glance::fit(fonts.small, &note, tiles[1].size.x - 24.0), self.dim);
        canvas.stamp(&note, Vec2::new(track.origin.x, track.max().y + 6.0).round(), 1.0);

        match model.power.as_ref() {
            Some(power) => {
                let (big, unit) = match power.watts {
                    Some(watts) => (format!("{watts:.1}"), "W"),
                    None => (format!("{}%", power.percent), ""),
                };
                let head = if power.charging { crate::lang::t("Charging") } else { crate::lang::t("Power draw") };
                let below = self.tile_frame(canvas, tiles[2], fonts, head, &big, unit);
                let words = match (power.charging, power.left_hours) {
                    (true, _) => crate::lang::tf("Battery {}%", &[&(power.percent)]),
                    (false, Some(hours)) => format!("{} left", hours_words(hours)),
                    (false, None) => crate::lang::tf("Battery {}%", &[&(power.percent)]),
                };
                let note = fonts.small.paint(&crate::glance::fit(fonts.small, &words, tiles[2].size.x - 24.0), self.dim);
                canvas.stamp(&note, Vec2::new(tiles[2].origin.x + 12.0, below + 8.0).round(), 1.0);
            }
            None => {
                let running = model.groups.iter().map(|group| group.members.len()).sum::<usize>();
                let below = self.tile_frame(canvas, tiles[2], fonts, crate::lang::t("Running"), &running.to_string(), "processes");
                let apps = model.count(View::Apps);
                let note = fonts.small.paint(&crate::lang::tf("{} apps · on wall power", &[&(apps)]), self.dim);
                canvas.stamp(&note, Vec2::new(tiles[2].origin.x + 12.0, below + 8.0).round(), 1.0);
            }
        }

        match model.disk.as_ref() {
            Some(disk) => {
                let below = self.tile_frame(canvas, tiles[3], fonts, &disk.label, &format!("{}%", disk.used), "");
                let track = Rect::new(Vec2::new(tiles[3].origin.x + 12.0, below + 6.0), Vec2::new(tiles[3].size.x - 24.0, 4.0));
                meter(canvas, track, f64::from(disk.used) / 100.0, self.ink, crate::activity::tone(self.ink, f64::from(disk.used)));
                let free = disk.size_gb * (100.0 - f64::from(disk.used)) / 100.0;
                let note = fonts.small.paint(&crate::glance::fit(fonts.small, &format!("{} free", size_words(free)), tiles[3].size.x - 24.0), self.dim);
                canvas.stamp(&note, Vec2::new(track.origin.x, track.max().y + 6.0).round(), 1.0);
            }
            None => {
                self.tile_frame(canvas, tiles[3], fonts, crate::lang::t("Disk"), "—", "");
            }
        }
    }

    fn paint_table(&self, canvas: &mut Canvas, size: Vec2, model: &Model, fonts: &Fonts, icons: &HashMap<String, Image>) {
        let table = self.table_rect(size, model.view);
        canvas.round_rect(table, self.radius, self.card, 1.0);
        let columns = self.columns(table);
        let names = [(Column::Name, crate::lang::t("Name")), (Column::Impact, crate::lang::t("Impact")), (Column::Cpu, "CPU"), (Column::Memory, crate::lang::t("Memory"))];
        for ((column, x, _, right), (_, word)) in columns.iter().zip(names) {
            let on = model.sort == *column;
            let painted = match on {
                true => fonts.small.paint_weight(word, self.ink, 600.0),
                false => fonts.small.paint(word, self.dim),
            };
            let left = if *right { x - logical(&painted).x } else { *x };
            canvas.stamp(&painted, Vec2::new(left, table.origin.y + HEAD_H / 2.0 - logical(&painted).y / 2.0).round(), 1.0);
        }
        let last = fonts.small.paint(crate::lang::t("Last minute"), self.dim);
        canvas.stamp(&last, Vec2::new(table.max().x - 12.0 - logical(&last).x, table.origin.y + HEAD_H / 2.0 - logical(&last).y / 2.0).round(), 1.0);
        canvas.round_rect(Rect::new(Vec2::new(table.origin.x, table.origin.y + HEAD_H - 1.0), Vec2::new(table.size.x, 1.0)), 0.0, self.rule, 1.0);

        let tall = self.body_layer(size, model, fonts, icons);
        let shown = self.visible_body(size, model.view);
        stamp_rows(canvas, &tall, shown, model.scroll);
    }

    fn visible_body(&self, size: Vec2, view: View) -> Rect {
        let body = self.body_rect(size, view);
        Rect::new(body.origin, Vec2::new(body.size.x, (body.size.y - BODY_TRIM).max(1.0)))
    }

    pub fn scroll_into(&self, frame: &mut Image, size: Vec2, model: &Model, tall: &Image) {
        let shown = self.visible_body(size, model.view);
        let mut canvas = Canvas::on(std::mem::replace(frame, Image { width: 0, height: 0, rgba: Vec::new() }));
        canvas.round_rect(shown, 0.0, self.card, 1.0);
        stamp_rows(&mut canvas, tall, shown, model.scroll);
        *frame = canvas.image;
    }

    pub fn body_layer(&self, size: Vec2, model: &Model, fonts: &Fonts, icons: &HashMap<String, Image>) -> Image {
        let table = self.table_rect(size, model.view);
        let columns = self.columns(table);
        let body = self.body_rect(size, model.view);
        let rows = self.rows(model);
        let whole = rows.last().map_or(0.0, |(y, h, _)| y + h);
        let tall = whole.max(body.size.y - BODY_TRIM).min(LAYER_MOST);
        let mut layer = Canvas::new(body.size.x.round() as u32, tall.max(1.0).round() as u32);
        let shown = model.shown();
        let rows = rows;
        if shown.is_empty() {
            let words = match model.query.is_empty() {
                true => crate::lang::t("Nothing runs here right now.").to_string(),
                false => crate::lang::tf("Nothing matches “{}”.", &[&(model.query.trim())]),
            };
            let note = fonts.body.paint(&words, self.dim);
            layer.stamp(&note, Vec2::new(body.size.x / 2.0 - logical(&note).x / 2.0, 40.0).round(), 1.0);
        }
        let total = model.memory.total_kb;
        for (top, tall, line) in rows {
            let y = top;
            if y > LAYER_MOST {
                break;
            }
            let row = Rect::new(Vec2::new(0.0, y), Vec2::new(body.size.x, tall));
            match line {
                Line::Group(key) => {
                    let Some(group) = model.groups.iter().find(|group| group.key == key) else {
                        continue;
                    };
                    let selected = model.selected.as_ref() == Some(&key);
                    if selected {
                        layer.round_rect(row, 0.0, self.accent, 0.16);
                    }
                    if top > 0.0 {
                        layer.round_rect(Rect::new(Vec2::new(12.0, y), Vec2::new(body.size.x - 24.0, 1.0)), 0.0, self.rule, 0.6);
                    }
                    let fade = if group.paused { 0.6 } else { 1.0 };
                    if group.members.len() > 1 {
                        let open = model.open.contains(&key);
                        caret(&mut layer, Vec2::new(17.0, row.center().y), open, self.dim);
                    }
                    let spot = Rect::new(Vec2::new(36.0, row.center().y - 12.0), Vec2::splat(24.0));
                    self.paint_icon(&mut layer, group, spot, fonts, icons, fade);
                    let name_x = columns[0].1 - table.origin.x;
                    let room = columns[1].1 - columns[0].1 - 12.0;
                    let name = fonts.body.paint(&crate::glance::fit(fonts.body, &group.name, room), self.ink);
                    let detail = fonts.small.paint(&crate::glance::fit(fonts.small, &group.detail, room), self.dim);
                    let both = logical(&name).y + logical(&detail).y;
                    let ty = row.center().y - both / 2.0;
                    layer.stamp(&name, Vec2::new(name_x, ty).round(), fade);
                    layer.stamp(&detail, Vec2::new(name_x, ty + logical(&name).y).round(), fade);
                    let impact = group.impact(total);
                    pill_left(&mut layer, fonts, Vec2::new(columns[1].1 - table.origin.x, row.center().y), impact, self.ink, self.dim);
                    let cpu_text = match group.paused {
                        true => "—".to_string(),
                        false => percent_words(group.cpu),
                    };
                    let cpu_ink = if group.paused { self.dim } else { crate::activity::tone(self.ink, group.cpu) };
                    let cpu = fonts.body.paint_even_digits(&cpu_text, cpu_ink);
                    layer.stamp(&cpu, Vec2::new(columns[2].1 - table.origin.x - logical(&cpu).x, row.center().y - logical(&cpu).y / 2.0).round(), fade);
                    let mem = fonts.body.paint_even_digits(&memory_words(group.mem_kb), self.ink);
                    layer.stamp(&mem, Vec2::new(columns[3].1 - table.origin.x - logical(&mem).x, row.center().y - logical(&mem).y / 2.0).round(), fade);
                    if let Some(line) = model.history.get(&key) {
                        let most = line.iter().copied().fold(20.0, f64::max) * 1.1;
                        let spot = Rect::new(Vec2::new(self.spark_x(table) - table.origin.x, row.center().y - 9.0), Vec2::new(72.0, 18.0));
                        paint_spark(&mut layer, spot, line, most, false);
                    }
                }
                Line::Member(key, at) => {
                    let Some(member) = model.groups.iter().find(|group| group.key == key).and_then(|group| group.members.get(at)) else {
                        continue;
                    };
                    let x = columns[0].1 - table.origin.x;
                    let words = format!("{}  ·  {}", member.name, member.pid);
                    let room = columns[1].1 - columns[0].1 - 12.0;
                    let name = fonts.small.paint(&crate::glance::fit(fonts.small, &words, room), self.dim);
                    layer.stamp(&name, Vec2::new(x, row.center().y - logical(&name).y / 2.0).round(), 1.0);
                    let cpu = fonts.small.paint_even_digits(&percent_words(member.cpu), self.dim);
                    layer.stamp(&cpu, Vec2::new(columns[2].1 - table.origin.x - logical(&cpu).x, row.center().y - logical(&cpu).y / 2.0).round(), 1.0);
                    let mem = fonts.small.paint_even_digits(&memory_words(member.mem_kb), self.dim);
                    layer.stamp(&mem, Vec2::new(columns[3].1 - table.origin.x - logical(&mem).x, row.center().y - logical(&mem).y / 2.0).round(), 1.0);
                }
                Line::More(_, more) => {
                    let x = columns[0].1 - table.origin.x;
                    let words = fonts.small.paint(&format!("and {more} more"), self.dim);
                    layer.stamp(&words, Vec2::new(x, row.center().y - logical(&words).y / 2.0).round(), 0.8);
                }
            }
        }
        layer.image
    }

    fn paint_icon(&self, canvas: &mut Canvas, group: &Group, spot: Rect, fonts: &Fonts, icons: &HashMap<String, Image>, fade: f64) {
        self.paint_icon_plain(canvas, group, spot, fonts, icons, fade);
        if group.flatpak {
            flatpak_badge(canvas, spot, self.side, fade);
        }
    }

    fn paint_icon_plain(&self, canvas: &mut Canvas, group: &Group, spot: Rect, fonts: &Fonts, icons: &HashMap<String, Image>, fade: f64) {
        let big = spot.size.x > 30.0;
        let found = group.icon.as_ref().and_then(|name| match big {
            true => icons.get(&big_key(name)).or_else(|| icons.get(name)),
            false => icons.get(name),
        });
        if let Some(image) = found {
            canvas.stamp(image, (spot.center() - logical(image) / 2.0).round(), fade);
            return;
        }
        match group.kind {
            Kind::Drifter => {
                canvas.round_rect(spot, spot.size.x * 0.27, [77, 82, 214], fade);
                let mid = spot.center();
                let s = spot.size.x / 24.0;
                for (from, to) in [((-6.0, 2.0), (-2.5, -1.5)), ((-2.5, -1.5), (1.0, 2.0)), ((1.0, 2.0), (6.0, -2.0))] {
                    canvas.line(mid + Vec2::new(from.0 * s, from.1 * s), mid + Vec2::new(to.0 * s, to.1 * s), 2.0 * s, [255, 255, 255], fade);
                }
            }
            Kind::App => {
                canvas.round_rect(spot, spot.size.x * 0.27, self.off, fade);
                let first: String = group.name.chars().take(1).collect::<String>().to_uppercase();
                let text = fonts.small.paint_weight(&first, self.ink, 600.0);
                canvas.stamp(&text, (spot.center() - logical(&text) / 2.0).round(), fade);
            }
            Kind::Background | Kind::System => {
                let tint = if group.kind == Kind::System { [94, 102, 128] } else { [120, 128, 146] };
                canvas.round_rect(spot, spot.size.x * 0.27, tint, fade);
                let mid = spot.center();
                let r = spot.size.x * 0.2;
                canvas.ring(Rect::from_center(mid, Vec2::splat(r * 2.0)), r, 1.8, [255, 255, 255], fade);
                for step in 0..6 {
                    let angle = step as f64 * std::f64::consts::TAU / 6.0;
                    let dir = Vec2::new(angle.cos(), angle.sin());
                    canvas.line(mid + dir * (r + 1.2), mid + dir * (r + 3.4), 2.0, [255, 255, 255], fade);
                }
            }
        }
    }

    fn paint_inspector(&self, canvas: &mut Canvas, size: Vec2, model: &Model, fonts: &Fonts, icons: &HashMap<String, Image>) {
        let panel = self.inspector_rect(size);
        canvas.round_rect(panel, 0.0, self.side, 1.0);
        canvas.round_rect(Rect::new(panel.origin, Vec2::new(1.0, size.y)), 0.0, self.rule, 1.0);
        let pad = 16.0;
        let x = panel.origin.x + pad;
        let width = panel.size.x - pad * 2.0;
        let Some(group) = model.chosen() else {
            let words = fonts.body.paint(crate::lang::t("Pick an app to see it here."), self.dim);
            canvas.stamp(&words, Vec2::new(panel.center().x - logical(&words).x / 2.0, size.y / 2.0 - 10.0).round(), 1.0);
            return;
        };
        let spot = Rect::new(Vec2::new(x, 20.0), Vec2::splat(40.0));
        self.paint_icon(canvas, group, spot, fonts, icons, 1.0);
        let name = fonts.body.paint_weight(&crate::glance::fit(fonts.body, &group.name, width - 52.0), self.ink, 650.0);
        canvas.stamp(&name, Vec2::new(spot.max().x + 12.0, spot.origin.y + 1.0).round(), 1.0);
        pill_left(canvas, fonts, Vec2::new(spot.max().x + 12.0, spot.origin.y + 29.0), group.impact(model.memory.total_kb), self.ink, self.dim);

        let mut y = spot.max().y + 18.0;
        if let Some(line) = model.history.get(&group.key) {
            let area = Rect::new(Vec2::new(x, y), Vec2::new(width, 48.0));
            canvas.round_rect(Rect::new(Vec2::new(x, y + 24.0), Vec2::new(width, 1.0)), 0.0, self.ink, 0.07);
            canvas.round_rect(Rect::new(Vec2::new(x, y + 47.0), Vec2::new(width, 1.0)), 0.0, self.ink, 0.1);
            let most = line.iter().copied().fold(20.0, f64::max) * 1.1;
            paint_spark(canvas, area, line, most, true);
            let label = fonts.small.paint(crate::lang::t("CPU · last minute"), self.dim);
            canvas.stamp(&label, Vec2::new(x, y + 52.0).round(), 0.8);
            y += 76.0;
        }
        let program = group.argv.first().map(|first| first.rsplit('/').next().unwrap_or(first).to_string()).unwrap_or_default();
        let mut pairs: Vec<(&str, String)> = vec![
            (crate::lang::t("Doing"), group.detail.clone()),
            ("CPU", if group.paused { crate::lang::t("Paused").to_string() } else { percent_words(group.cpu) }),
            (crate::lang::t("Memory"), memory_words(group.mem_kb)),
            (crate::lang::t("Processes"), group.members.len().to_string()),
        ];
        if !program.is_empty() {
            pairs.push((crate::lang::t("Program"), program));
        }
        for (label, value) in pairs {
            let left = fonts.small.paint(label, self.dim);
            canvas.stamp(&left, Vec2::new(x, y).round(), 1.0);
            let shown = fonts.small.paint_even_digits(&crate::glance::fit(fonts.small, &value, width - 84.0), self.ink);
            canvas.stamp(&shown, Vec2::new(x + width - logical(&shown).x, y).round(), 1.0);
            y += 21.0;
        }
        y += 8.0;
        let note = match group.kind {
            Kind::Drifter => crate::lang::t("Drifter draws your screen. It cannot pause or quit itself."),
            Kind::System => crate::lang::t("This runs for the whole computer. Only an administrator can stop it."),
            _ if group.paused => crate::lang::t("Paused. Its windows stay open and its work stays in memory. It uses no CPU until you resume it."),
            Kind::Background => crate::lang::t("This runs in the background for your session. Pause stops it for now. Quit asks it to close."),
            Kind::App => crate::lang::t("Pause keeps the work open and stops the CPU. Quit asks the app to save and close."),
        };
        let (pause, quit, force) = self.action_rects(size);
        let limit = if group.can_stop() { pause.origin.y - 10.0 } else { size.y - 16.0 };
        for line in wrap(fonts.small, note, width) {
            if y + 16.0 > limit {
                break;
            }
            let painted = fonts.small.paint(&line, self.dim);
            canvas.stamp(&painted, Vec2::new(x, y).round(), 1.0);
            y += 16.0;
        }
        y += 10.0;
        if group.members.len() > 1 && y + 60.0 < limit {
            let head = fonts.small.paint_weight(crate::lang::t("Busiest processes"), self.dim, 600.0);
            canvas.stamp(&head, Vec2::new(x, y).round(), 0.9);
            y += 22.0;
            let mut busiest: Vec<&super::Member> = group.members.iter().collect();
            busiest.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.mem_kb.cmp(&a.mem_kb)));
            for member in busiest {
                if y + 20.0 > limit {
                    break;
                }
                let value = fonts.small.paint_even_digits(&percent_words(member.cpu), crate::activity::tone(self.dim, member.cpu));
                let name = fonts.small.paint(&crate::glance::fit(fonts.small, &member.name, width - logical(&value).x - 12.0), self.ink);
                canvas.stamp(&name, Vec2::new(x, y).round(), 1.0);
                canvas.stamp(&value, Vec2::new(x + width - logical(&value).x, y).round(), 1.0);
                y += 20.0;
            }
        }
        if !group.can_stop() {
            return;
        }
        let button = |canvas: &mut Canvas, rect: Rect, word: &str| {
            canvas.round_rect(rect, 7.0, self.ink, 0.1);
            let label = fonts.body.paint_weight(word, self.ink, 550.0);
            canvas.stamp(&label, (rect.center() - logical(&label) / 2.0).round(), 1.0);
        };
        button(canvas, pause, if group.paused { crate::lang::t("Resume") } else { "Pause" });
        button(canvas, quit, crate::lang::t("Quit"));
        self.paint_hold(canvas, force, model.hold, fonts);
    }

    pub fn paint_hold(&self, canvas: &mut Canvas, rect: Rect, hold: f64, fonts: &Fonts) {
        canvas.round_rect(rect, 7.0, self.side, 1.0);
        canvas.round_rect(rect, 7.0, self.alert, 0.16);
        if hold > 0.0 {
            let filled = Rect::new(rect.origin, Vec2::new(rect.size.x * hold.clamp(0.0, 1.0), rect.size.y));
            canvas.round_rect(filled, 7.0, self.alert, 0.32);
        }
        let word = if hold > 0.0 { crate::lang::t("Keep holding…") } else { crate::lang::t("Hold to force quit") };
        let label = fonts.body.paint_weight(word, self.alert, 550.0);
        canvas.stamp(&label, (rect.center() - logical(&label) / 2.0).round(), 1.0);
    }

    pub fn paint_hold_piece(&self, size: Vec2, hold: f64, fonts: &Fonts) -> (Rect, Image) {
        let rect = self.hold_rect(size);
        let mut canvas = Canvas::new(rect.size.x.round() as u32, rect.size.y.round() as u32);
        self.paint_hold(&mut canvas, Rect::new(Vec2::ZERO, rect.size), hold, fonts);
        (rect, canvas.image)
    }

    fn paint_hour(&self, canvas: &mut Canvas, size: Vec2, model: &Model, fonts: &Fonts, icons: &HashMap<String, Image>) {
        let main = self.main(size, View::Hour);
        let title = fonts.title.paint(crate::lang::t("Last hour"), self.ink);
        canvas.stamp(&title, Vec2::new(main.origin.x + PAD, PAD).round(), 1.0);
        let note = fonts.small.paint(crate::lang::t("Drifter takes a reading every 30 seconds. It stays on this computer."), self.dim);
        canvas.stamp(&note, Vec2::new(main.origin.x + PAD, PAD + logical(&title).y + 4.0).round(), 1.0);
        for (measure, rect) in self.segment_rects(size) {
            let on = model.measure == measure;
            canvas.round_rect(rect, 7.0, if on { self.accent } else { self.card }, 1.0);
            let word = match measure {
                Measure::Cpu => "CPU",
                Measure::Memory => crate::lang::t("Memory"),
            };
            let label = match on {
                true => fonts.body.paint_weight(word, [255, 255, 255], 600.0),
                false => fonts.body.paint(word, self.ink),
            };
            canvas.stamp(&label, (rect.center() - logical(&label) / 2.0).round(), 1.0);
        }
        let card = self.chart_card(size);
        canvas.round_rect(card, self.radius, self.card, 1.0);
        let area = self.chart_area(size);
        for step in 0..=4 {
            let value = step as f64 * 25.0;
            let y = area.max().y - value / 100.0 * area.size.y;
            canvas.round_rect(Rect::new(Vec2::new(area.origin.x, y.round()), Vec2::new(area.size.x, 1.0)), 0.0, self.ink, if step == 0 { 0.16 } else { 0.06 });
            if step % 2 == 0 {
                let label = fonts.small.paint(&format!("{}%", value as u64), self.dim);
                canvas.stamp(&label, Vec2::new(area.max().x + 8.0, y - logical(&label).y / 2.0).round(), 1.0);
            }
        }
        for minutes in [60u64, 45, 30, 15, 0] {
            let x = self.time_x(area, model.now, model.now.saturating_sub(minutes * 60));
            let words = match minutes {
                0 => crate::lang::t("Now").to_string(),
                m => crate::lang::tf("{} min ago", &[&(m)]),
            };
            let label = fonts.small.paint(&words, self.dim);
            let left = match minutes {
                60 => x,
                0 => x - logical(&label).x,
                _ => x - logical(&label).x / 2.0,
            };
            canvas.stamp(&label, Vec2::new(left, area.max().y + 8.0).round(), 1.0);
        }
        let read = Rect::new(Vec2::new(card.max().x - 250.0, card.origin.y + 20.0), Vec2::new(232.0, card.size.y - 40.0));
        if model.hour.len() < 2 {
            let words = fonts.body.paint(crate::lang::t("The chart fills in as time passes."), self.dim);
            canvas.stamp(&words, Vec2::new(area.center().x - logical(&words).x / 2.0, area.center().y - 10.0).round(), 1.0);
            let more = fonts.small.paint(crate::lang::t("Come back in a few minutes."), self.dim);
            canvas.stamp(&more, Vec2::new(area.center().x - logical(&more).x / 2.0, area.center().y + 10.0).round(), 1.0);
            return;
        }
        let series = series(&model.hour, model.measure);
        let colors = SERIES;
        let total_mb = (model.memory.total_kb / 1024).max(1) as f64;
        let value_of = |reading: &Reading, name: &str| -> f64 {
            match model.measure {
                Measure::Cpu => reading.top.iter().find(|share| share.name == name).map_or(0.0, |share| share.cpu),
                Measure::Memory => reading.top.iter().find(|share| share.name == name).map_or(0.0, |share| share.mem_mb as f64 / total_mb * 100.0),
            }
        };
        let whole_of = |reading: &Reading| match model.measure {
            Measure::Cpu => reading.cpu,
            Measure::Memory => reading.mem,
        };
        let xs: Vec<f64> = model.hour.iter().map(|reading| self.time_x(area, model.now, reading.at)).collect();
        let first = xs.first().copied().unwrap_or(area.origin.x);
        let columns = area.size.x.floor() as usize;
        for column in 0..columns {
            let x = area.origin.x + column as f64 + 0.5;
            if x < first {
                continue;
            }
            let last = xs[xs.len() - 1];
            if x > last + 1.0 {
                continue;
            }
            let next = xs.iter().position(|at| *at >= x).unwrap_or(xs.len() - 1);
            let prev = next.saturating_sub(1);
            if model.hour[next].at.saturating_sub(model.hour[prev].at) > super::hour::EVERY_SECONDS * 2 + 10 {
                continue;
            }
            let t = match xs[next] - xs[prev] {
                gap if gap > 0.5 => ((x - xs[prev]) / gap).clamp(0.0, 1.0),
                _ => 1.0,
            };
            let blend = |f: &dyn Fn(&Reading) -> f64| f(&model.hour[prev]) * (1.0 - t) + f(&model.hour[next]) * t;
            let mut floor = 0.0;
            for (at, name) in series.iter().enumerate() {
                let value = blend(&|reading: &Reading| value_of(reading, name));
                let top = (floor + value).min(100.0);
                let y0 = area.max().y - floor / 100.0 * area.size.y;
                let y1 = area.max().y - top / 100.0 * area.size.y;
                if y0 - y1 > 0.05 {
                    canvas.round_rect(Rect::new(Vec2::new(x - 0.5, y1), Vec2::new(1.0, y0 - y1)), 0.0, colors[at % colors.len()], 0.82);
                }
                floor = top;
            }
            let whole = blend(&whole_of).min(100.0);
            if whole > floor {
                let y0 = area.max().y - floor / 100.0 * area.size.y;
                let y1 = area.max().y - whole / 100.0 * area.size.y;
                canvas.round_rect(Rect::new(Vec2::new(x - 0.5, y1), Vec2::new(1.0, y0 - y1)), 0.0, self.dim, 0.45);
            }
        }
        let mut pinned: Vec<f64> = Vec::new();
        for moment in &model.moments {
            let Some(x) = xs.get(moment.at).copied() else {
                continue;
            };
            if pinned.iter().any(|held| (held - x).abs() < 12.0) {
                continue;
            }
            pinned.push(x);
            let mut y = area.origin.y + 10.0;
            while y < area.max().y {
                canvas.round_rect(Rect::new(Vec2::new(x.round(), y), Vec2::new(1.0, 2.0)), 0.0, self.ink, 0.28);
                y += 5.0;
            }
            canvas.circle(Vec2::new(x, area.origin.y + 4.0), 5.0, self.ink, 1.0);
            canvas.circle(Vec2::new(x, area.origin.y + 4.0), 3.6, self.card, 1.0);
        }
        let cursor = model
            .cursor
            .and_then(|at| model.hour.iter().enumerate().min_by_key(|(_, reading)| reading.at.abs_diff(at)).map(|(index, _)| index))
            .unwrap_or(model.hour.len() - 1);
        let reading = &model.hour[cursor];
        let cx = xs[cursor];
        canvas.round_rect(Rect::new(Vec2::new(cx.round() - 0.75, area.origin.y), Vec2::new(1.5, area.size.y)), 0.0, self.ink, 0.9);
        let top = whole_of(reading).min(100.0);
        canvas.circle(Vec2::new(cx, area.max().y - top / 100.0 * area.size.y), 4.5, self.ink, 1.0);

        let mut y = read.origin.y;
        let time = clock_words(reading.at);
        let big = fonts.title.paint_weight(&time, self.ink, 600.0);
        canvas.stamp(&big, Vec2::new(read.origin.x, y).round(), 1.0);
        y += logical(&big).y + 2.0;
        let measure = match model.measure {
            Measure::Cpu => format!("CPU {}", percent_words(reading.cpu)),
            Measure::Memory => crate::lang::tf("Memory {}%", &[&(reading.mem.round() as u64)]),
        };
        let sub = fonts.small.paint(&format!("{} · {}", super::hour::ago(model.now.saturating_sub(reading.at)), measure), self.dim);
        canvas.stamp(&sub, Vec2::new(read.origin.x, y).round(), 1.0);
        y += 26.0;
        let near = model.moments.iter().filter(|moment| moment.at.abs_diff(cursor) <= 3).min_by_key(|moment| moment.at.abs_diff(cursor));
        let (head, words) = match near {
            Some(moment) => (moment.head.clone(), moment.words.clone()),
            None => (crate::lang::t("Quiet").to_string(), crate::lang::t("Nothing stood out at this moment.").to_string()),
        };
        let lines = wrap(fonts.small, &words, read.size.x - 24.0);
        let box_h = 34.0 + lines.len() as f64 * 16.0;
        let event = Rect::new(Vec2::new(read.origin.x, y), Vec2::new(read.size.x, box_h));
        canvas.round_rect(event, 10.0, self.ink, 0.07);
        let head = fonts.body.paint_weight(&crate::glance::fit(fonts.body, &head, read.size.x - 24.0), self.ink, 600.0);
        canvas.stamp(&head, Vec2::new(event.origin.x + 12.0, event.origin.y + 9.0).round(), 1.0);
        for (at, line) in lines.iter().enumerate() {
            let painted = fonts.small.paint(line, self.dim);
            canvas.stamp(&painted, Vec2::new(event.origin.x + 12.0, event.origin.y + 28.0 + at as f64 * 16.0).round(), 1.0);
        }
        y = event.max().y + 14.0;
        let mut shares = reading.top.clone();
        match model.measure {
            Measure::Cpu => shares.sort_by(|a, b| b.cpu.total_cmp(&a.cpu)),
            Measure::Memory => shares.sort_by(|a, b| b.mem_mb.cmp(&a.mem_mb)),
        }
        for share in shares.iter().take(4) {
            if y + 24.0 > read.max().y {
                break;
            }
            let spot = Rect::new(Vec2::new(read.origin.x, y), Vec2::splat(22.0));
            let stand_in = Group {
                key: String::new(),
                name: share.name.clone(),
                detail: String::new(),
                icon: share.icon.clone(),
                kind: if share.app { Kind::App } else { Kind::Background },
                root: 0,
                argv: Vec::new(),
                members: Vec::new(),
                cpu: 0.0,
                mem_kb: 0,
                paused: false,
                protected: false,
                flatpak: false,
            };
            self.paint_icon(canvas, &stand_in, spot, fonts, icons, 1.0);
            let value = match model.measure {
                Measure::Cpu => percent_words(share.cpu),
                Measure::Memory => memory_words(share.mem_mb * 1024),
            };
            let number = fonts.small.paint_even_digits(&value, self.dim);
            let name = fonts.small.paint(&crate::glance::fit(fonts.small, &share.name, read.size.x - 40.0 - logical(&number).x), self.ink);
            canvas.stamp(&name, Vec2::new(spot.max().x + 8.0, spot.center().y - logical(&name).y / 2.0).round(), 1.0);
            canvas.stamp(&number, Vec2::new(read.max().x - logical(&number).x, spot.center().y - logical(&number).y / 2.0).round(), 1.0);
            y += 28.0;
        }

        let mut x = area.origin.x;
        let key_y = card.max().y - 26.0;
        for (at, name) in series.iter().enumerate() {
            let swatch = Rect::new(Vec2::new(x, key_y + 2.0), Vec2::splat(10.0));
            canvas.round_rect(swatch, 3.0, colors[at % colors.len()], 1.0);
            let label = fonts.small.paint(name, self.dim);
            canvas.stamp(&label, Vec2::new(x + 15.0, key_y + 7.0 - logical(&label).y / 2.0).round(), 1.0);
            x += 15.0 + logical(&label).x + 16.0;
            if x > read.origin.x - 120.0 {
                break;
            }
        }
        let swatch = Rect::new(Vec2::new(x, key_y + 2.0), Vec2::splat(10.0));
        if x <= read.origin.x - 120.0 {
            canvas.round_rect(swatch, 3.0, self.dim, 0.45);
            let label = fonts.small.paint(crate::lang::t("Everything else"), self.dim);
            canvas.stamp(&label, Vec2::new(x + 15.0, key_y + 7.0 - logical(&label).y / 2.0).round(), 1.0);
        }
    }
}

pub const SERIES: [[u8; 3]; 5] = [[64, 128, 238], WARM, [181, 140, 255], [79, 209, 197], [255, 122, 168]];

pub fn series(hour: &[Reading], measure: Measure) -> Vec<String> {
    let mut sums: Vec<(String, f64)> = Vec::new();
    for reading in hour {
        for share in &reading.top {
            let value = match measure {
                Measure::Cpu => share.cpu,
                Measure::Memory => share.mem_mb as f64,
            };
            match sums.iter_mut().find(|(name, _)| *name == share.name) {
                Some((_, sum)) => *sum += value,
                None => sums.push((share.name.clone(), value)),
            }
        }
    }
    sums.sort_by(|a, b| b.1.total_cmp(&a.1));
    sums.into_iter().filter(|(_, sum)| *sum > 0.0).take(SERIES.len()).map(|(name, _)| name).collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Group(String),
    Member(String, usize),
    More(String, usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    View(View),
    Search,
    Sort(Column),
    Caret(String),
    Row(String),
    Pause,
    Quit,
    Force,
    Reopen,
    Measure(Measure),
    Chart(usize),
}

pub fn big_key(name: &str) -> String {
    format!("{name}@big")
}

pub const ICON_SMALL: u32 = 24;
pub const ICON_BIG: u32 = 40;

pub fn hold_seconds() -> f64 {
    HOLD_SECONDS
}

pub fn hours_words(hours: f64) -> String {
    let minutes = (hours * 60.0).round() as u64;
    match minutes / 60 {
        0 => format!("{} min", minutes % 60),
        h => format!("{h} h {:02} min", minutes % 60),
    }
}

pub fn size_words(gb: f64) -> String {
    match gb >= 1000.0 {
        true => format!("{:.1} TB", gb / 1000.0),
        false => format!("{} GB", gb.round() as u64),
    }
}

pub fn clock_words(at: u64) -> String {
    chrono::DateTime::from_timestamp(at as i64, 0)
        .map(|time| time.with_timezone(&chrono::Local).format("%-I:%M %p").to_string())
        .unwrap_or_default()
}

pub fn wrap(font: &Font, text: &str, width: f64) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let tried = match line.is_empty() {
            true => word.to_string(),
            false => format!("{line} {word}"),
        };
        if font.width_of(&tried) > width && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            line = word.to_string();
        } else {
            line = tried;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn stamp_rows(canvas: &mut Canvas, tall: &Image, shown: Rect, scroll: f64) {
    let dense = crate::paint::density();
    let from = (scroll * dense).round().max(0.0) as u32;
    let rows = ((shown.size.y * dense).round() as u32).min(tall.height.saturating_sub(from));
    if rows == 0 || tall.width == 0 {
        return;
    }
    let start = (from * tall.width * 4) as usize;
    let end = start + (rows * tall.width * 4) as usize;
    let piece = Image { width: tall.width, height: rows, rgba: tall.rgba[start..end].to_vec() };
    canvas.stamp(&piece, shown.origin, 1.0);
}

fn meter(canvas: &mut Canvas, track: Rect, share: f64, ink: [u8; 3], color: [u8; 3]) {
    canvas.round_rect(track, track.size.y / 2.0, ink, 0.14);
    if share > 0.0 {
        let filled = Rect::new(track.origin, Vec2::new((track.size.x * share.clamp(0.0, 1.0)).max(track.size.y), track.size.y));
        canvas.round_rect(filled, track.size.y / 2.0, color, 0.8);
    }
}

pub fn paint_spark(canvas: &mut Canvas, area: Rect, line: &[f64], most: f64, dot: bool) {
    if line.len() < 2 {
        return;
    }
    let most = most.max(1.0);
    let last = (line.len() - 1) as f64;
    let y_of = |value: f64| area.max().y - 1.0 - (value / most).clamp(0.0, 1.0) * (area.size.y - 4.0);
    let value_at = |along: f64| {
        let low = along.floor().max(0.0) as usize;
        let high = (low + 1).min(line.len() - 1);
        let mix = along - low as f64;
        line[low] * (1.0 - mix) + line[high] * mix
    };
    let recent = line.iter().rev().take(6).copied().fold(0.0, f64::max);
    let color = crate::glance::heat((recent / 100.0).max(recent / most * 0.5));
    let columns = area.size.x.floor() as usize;
    for column in 0..=columns {
        let x = area.origin.x + column as f64;
        let top = y_of(value_at(column as f64 / area.size.x.max(1.0) * last));
        canvas.round_rect(Rect::new(Vec2::new(x, top), Vec2::new(1.0, (area.max().y - top).max(0.0))), 0.0, color, 0.13);
    }
    let point = |at: usize| Vec2::new(area.origin.x + area.size.x * at as f64 / last, y_of(line[at]));
    for at in 1..line.len() {
        canvas.line(point(at - 1), point(at), 1.4, color, 0.95);
    }
    if dot {
        canvas.circle(point(line.len() - 1), 2.6, color, 1.0);
    }
}

fn impact_color(impact: Impact, dim: [u8; 3]) -> [u8; 3] {
    match impact {
        Impact::Calm => dim,
        Impact::Busy => WARM,
        Impact::Heavy => HOT,
        Impact::Paused => COOL,
    }
}

fn pill_left(canvas: &mut Canvas, fonts: &Fonts, at: Vec2, impact: Impact, ink: [u8; 3], dim: [u8; 3]) {
    let color = impact_color(impact, dim);
    let word = fonts.small.paint_weight(impact.word(), color, 600.0);
    let size = Vec2::new(logical(&word).x + 26.0, 20.0);
    let rect = Rect::new(Vec2::new(at.x, at.y - size.y / 2.0), size);
    canvas.round_rect(rect, 10.0, ink, 0.08);
    canvas.circle(Vec2::new(rect.origin.x + 10.0, rect.center().y), 3.0, color, 1.0);
    canvas.stamp(&word, Vec2::new(rect.origin.x + 17.0, rect.center().y - logical(&word).y / 2.0).round(), 1.0);
}

fn caret(canvas: &mut Canvas, mid: Vec2, open: bool, ink: [u8; 3]) {
    let (a, b, c) = match open {
        true => (Vec2::new(-4.0, -2.0), Vec2::new(0.0, 2.5), Vec2::new(4.0, -2.0)),
        false => (Vec2::new(-2.0, -4.0), Vec2::new(2.5, 0.0), Vec2::new(-2.0, 4.0)),
    };
    canvas.line(mid + a, mid + b, 1.6, ink, 1.0);
    canvas.line(mid + b, mid + c, 1.6, ink, 1.0);
}

pub const FLATPAK: [u8; 3] = [74, 144, 217];

pub fn flatpak_badge(canvas: &mut Canvas, spot: Rect, edge: [u8; 3], fade: f64) {
    let side = (spot.size.x * 0.5).clamp(11.0, 18.0);
    let badge = Rect::new(spot.max() - Vec2::splat(side * 0.8), Vec2::splat(side));
    canvas.circle(badge.center(), side / 2.0 + 1.2, edge, fade);
    canvas.circle(badge.center(), side / 2.0, FLATPAK, fade);
    let mid = badge.center();
    let s = side / 14.0;
    let white = [255, 255, 255];
    let top = mid + Vec2::new(0.0, -4.2 * s);
    let left = mid + Vec2::new(-3.8 * s, -2.0 * s);
    let right = mid + Vec2::new(3.8 * s, -2.0 * s);
    let center = mid + Vec2::new(0.0, 0.2 * s);
    let bottom = mid + Vec2::new(0.0, 4.6 * s);
    let w = 1.2 * s.max(0.9);
    for (from, to) in [(top, left), (top, right), (left, center), (right, center), (center, bottom), (left, left + Vec2::new(0.0, 4.4 * s)), (right, right + Vec2::new(0.0, 4.4 * s)), (left + Vec2::new(0.0, 4.4 * s), bottom), (right + Vec2::new(0.0, 4.4 * s), bottom)] {
        canvas.line(from, to, w, white, fade);
    }
}

fn view_glyph(canvas: &mut Canvas, view: View, spot: Rect) {
    let white = [255, 255, 255];
    let mid = spot.center();
    match view {
        View::Apps => {
            for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let cell = Rect::from_center(mid + Vec2::new(dx * 3.2, dy * 3.2), Vec2::splat(5.0));
                canvas.round_rect(cell, 1.4, white, 1.0);
            }
        }
        View::Background => {
            canvas.ring(Rect::from_center(mid, Vec2::splat(8.0)), 4.0, 1.7, white, 1.0);
            for step in 0..6 {
                let angle = step as f64 * std::f64::consts::TAU / 6.0;
                let dir = Vec2::new(angle.cos(), angle.sin());
                canvas.line(mid + dir * 5.0, mid + dir * 7.0, 1.8, white, 1.0);
            }
        }
        View::Everything => {
            for row in [-4.0, 0.0, 4.0] {
                canvas.circle(mid + Vec2::new(-5.0, row), 1.1, white, 1.0);
                canvas.line(mid + Vec2::new(-2.5, row), mid + Vec2::new(6.0, row), 1.6, white, 1.0);
            }
        }
        View::Hour => {
            canvas.ring(Rect::from_center(mid, Vec2::splat(14.0)), 7.0, 1.6, white, 1.0);
            canvas.line(mid, mid + Vec2::new(0.0, -4.5), 1.6, white, 1.0);
            canvas.line(mid, mid + Vec2::new(3.2, 1.6), 1.6, white, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(key: &str, name: &str, kind: Kind, cpu: f64, mem_kb: u64, members: usize) -> Group {
        Group {
            key: key.to_string(),
            name: name.to_string(),
            detail: "1 process".to_string(),
            icon: None,
            kind,
            root: key.parse().unwrap_or(1),
            argv: vec![format!("/usr/bin/{name}")],
            members: (0..members)
                .map(|at| super::super::Member { pid: at as u32 + 10, ppid: 1, start: 0, name: format!("p{at}"), cpu: 0.0, mem_kb: 1 })
                .collect(),
            cpu,
            mem_kb,
            paused: false,
            protected: false,
            flatpak: false,
        }
    }

    fn model() -> Model {
        let mut model = Model {
            groups: vec![
                group("1", "Firefox", Kind::App, 9.0, 2_000_000, 6),
                group("2", "Code", Kind::App, 30.0, 1_000_000, 3),
                group("3", "Sound", Kind::Background, 1.0, 40_000, 2),
                group("4", "Network", Kind::System, 0.1, 20_000, 1),
            ],
            memory: Memory { total_kb: 16 * 1024 * 1024, available_kb: 8 * 1024 * 1024, ..Memory::default() },
            ..Model::default()
        };
        model.arrange();
        model
    }

    #[test]
    fn each_view_holds_its_own_kind() {
        let model = model();
        assert_eq!(model.count(View::Apps), 2);
        assert_eq!(model.count(View::Background), 2);
        assert_eq!(model.count(View::Everything), 4);
        assert_eq!(model.shown().iter().map(|g| g.name.as_str()).collect::<Vec<_>>(), vec!["Code", "Firefox"]);
    }

    #[test]
    fn rows_keep_their_place_until_asked_to_resort() {
        let mut model = model();
        model.groups[0].cpu = 90.0;
        assert_eq!(model.shown()[0].name, "Code", "the list holds still");
        model.arrange();
        assert_eq!(model.shown()[0].name, "Firefox");
    }

    #[test]
    fn a_new_app_shows_at_the_end_before_a_resort() {
        let mut model = model();
        model.groups.push(group("9", "Blender", Kind::App, 50.0, 3_000_000, 1));
        assert_eq!(model.shown().last().unwrap().name, "Blender");
    }

    #[test]
    fn search_finds_apps_by_name_and_process() {
        let mut model = model();
        model.query = "p2".into();
        model.arrange();
        assert_eq!(model.shown().len(), 2, "Firefox and Code both have a p2 process");
        model.query = "fire".into();
        model.arrange();
        assert_eq!(model.shown().len(), 1);
    }

    #[test]
    fn an_open_app_lists_its_processes_under_it() {
        let mut model = model();
        model.open.insert("1".into());
        let look = Look::themed(&crate::theme::DARK, [64, 128, 238]);
        let rows = look.rows(&model);
        assert_eq!(rows.len(), 2 + 6);
        assert!(matches!(rows[2].2, Line::Member(ref key, 0) if key == "1"));
    }

    #[test]
    fn the_glide_finds_other_views_and_rows_but_not_the_chosen_ones() {
        let model = model();
        let look = Look::themed(&crate::theme::DARK, [64, 128, 238]);
        let font = crate::text::Font::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts/WorkSans.ttf"), 13.0).unwrap();
        let fonts = Fonts { body: &font, small: &font, title: &font };
        let size = Vec2::new(1000.0, 700.0);
        let here = View::ALL.iter().position(|view| *view == model.view).unwrap();
        let there = (here + 1) % View::ALL.len();
        assert_eq!(look.lit_target(size, &model, &fonts, look.item_rect(here).center()), None);
        assert_eq!(look.lit_target(size, &model, &fonts, look.item_rect(there).center()), Some(look.item_rect(there)));
        let body = look.body_rect(size, model.view);
        let (top, tall, _) = look.rows(&model)[0];
        let point = Vec2::new(body.center().x, body.origin.y + top + tall / 2.0);
        let lit = look.lit_target(size, &model, &fonts, point).expect("a row lights");
        assert!(lit.contains(point));
    }

    #[test]
    fn the_history_keeps_one_minute_per_app_and_forgets_gone_apps() {
        let mut model = model();
        for _ in 0..70 {
            model.remember();
        }
        assert_eq!(model.history["1"].len(), HISTORY);
        model.groups.remove(0);
        model.remember();
        assert!(!model.history.contains_key("1"));
    }

    #[test]
    fn hours_and_sizes_read_plainly() {
        assert_eq!(hours_words(3.17), "3 h 10 min");
        assert_eq!(hours_words(0.5), "30 min");
        assert_eq!(size_words(512.0), "512 GB");
        assert_eq!(size_words(2000.0), "2.0 TB");
    }

    #[test]
    fn series_pick_the_biggest_users_of_the_hour() {
        let reading = |cpu: f64| Reading {
            at: 0,
            cpu,
            mem: 0.0,
            swap_mb: 0,
            top: vec![
                super::super::hour::Share { name: "cargo".into(), icon: None, app: true, cpu, mem_mb: 10 },
                super::super::hour::Share { name: "Code".into(), icon: None, app: true, cpu: 2.0, mem_mb: 900 },
            ],
        };
        let hour = vec![reading(80.0), reading(60.0)];
        assert_eq!(series(&hour, Measure::Cpu), vec!["cargo", "Code"]);
        assert_eq!(series(&hour, Measure::Memory), vec!["Code", "cargo"]);
    }
}
