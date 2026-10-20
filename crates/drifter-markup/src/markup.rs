use drifter_core::{Image, Rect, Vec2};

use crate::draw::{self, Clip, Mask, ordered};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tool {
    Crop,
    Pen,
    Highlight,
    Arrow,
    Box,
    Oval,
    Text,
    Loupe,
    Redact,
}

impl Tool {
    pub const ALL: [Tool; 9] = [Tool::Crop, Tool::Pen, Tool::Highlight, Tool::Arrow, Tool::Box, Tool::Oval, Tool::Text, Tool::Loupe, Tool::Redact];

    pub fn name(self) -> &'static str {
        match self {
            Tool::Crop => "Crop",
            Tool::Pen => "Pen",
            Tool::Highlight => "Highlighter",
            Tool::Arrow => "Arrow",
            Tool::Box => "Rectangle",
            Tool::Oval => "Ellipse",
            Tool::Text => "Text",
            Tool::Loupe => "Magnifier",
            Tool::Redact => "Pixelate",
        }
    }

    pub fn key(self) -> char {
        match self {
            Tool::Crop => 'c',
            Tool::Pen => 'p',
            Tool::Highlight => 'h',
            Tool::Arrow => 'a',
            Tool::Box => 'r',
            Tool::Oval => 'o',
            Tool::Text => 't',
            Tool::Loupe => 'm',
            Tool::Redact => 'x',
        }
    }

    fn freehand(self) -> bool {
        matches!(self, Tool::Pen | Tool::Highlight)
    }
}

pub const INKS: [([u8; 3], &str); 7] = [
    ([255, 59, 48], "Red"),
    ([255, 149, 0], "Orange"),
    ([255, 204, 0], "Yellow"),
    ([52, 199, 89], "Green"),
    ([10, 122, 255], "Blue"),
    ([22, 22, 24], "Black"),
    ([255, 255, 255], "White"),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Weight {
    Thin,
    Medium,
    Thick,
}

impl Weight {
    pub const ALL: [Weight; 3] = [Weight::Thin, Weight::Medium, Weight::Thick];

    pub fn line(self) -> f64 {
        match self {
            Weight::Thin => 2.0,
            Weight::Medium => 4.0,
            Weight::Thick => 8.0,
        }
    }

    pub fn text(self) -> f64 {
        match self {
            Weight::Thin => 15.0,
            Weight::Medium => 24.0,
            Weight::Thick => 38.0,
        }
    }

    pub fn block(self) -> f64 {
        match self {
            Weight::Thin => 6.0,
            Weight::Medium => 11.0,
            Weight::Thick => 20.0,
        }
    }

    pub fn name(self, text: bool) -> &'static str {
        match (self, text) {
            (Weight::Thin, false) => "Thin",
            (Weight::Medium, false) => "Medium",
            (Weight::Thick, false) => "Thick",
            (Weight::Thin, true) => "Small",
            (Weight::Medium, true) => "Medium",
            (Weight::Thick, true) => "Large",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Pen(Vec<Vec2>),
    Highlight(Vec<Vec2>),
    Arrow(Vec2, Vec2),
    Box(Vec2, Vec2),
    Oval(Vec2, Vec2),
    Text { at: Vec2, words: String, size: Vec2 },
    Loupe { center: Vec2, radius: f64, zoom: f64 },
    Redact(Vec2, Vec2),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    pub kind: Kind,
    pub ink: [u8; 3],
    pub weight: Weight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    Edge(u8),
    Start,
    End,
    Radius,
    Zoom,
}

pub const LEFT: u8 = 1;
pub const TOP: u8 = 2;
pub const RIGHT: u8 = 4;
pub const BOTTOM: u8 = 8;
pub const MIDDLE: u8 = 15;

const EDGES: [u8; 8] = [LEFT | TOP, TOP, RIGHT | TOP, RIGHT, RIGHT | BOTTOM, BOTTOM, LEFT | BOTTOM, LEFT];

pub fn edge_point(rect: Rect, edge: u8) -> Vec2 {
    let x = match (edge & LEFT != 0, edge & RIGHT != 0) {
        (true, _) => rect.origin.x,
        (_, true) => rect.max().x,
        _ => rect.center().x,
    };
    let y = match (edge & TOP != 0, edge & BOTTOM != 0) {
        (true, _) => rect.origin.y,
        (_, true) => rect.max().y,
        _ => rect.center().y,
    };
    Vec2::new(x, y)
}

fn pull(rect: Rect, edge: u8, to: Vec2) -> (Vec2, Vec2) {
    let (mut low, mut high) = (rect.origin, rect.max());
    if edge & LEFT != 0 {
        low.x = to.x;
    }
    if edge & RIGHT != 0 {
        high.x = to.x;
    }
    if edge & TOP != 0 {
        low.y = to.y;
    }
    if edge & BOTTOM != 0 {
        high.y = to.y;
    }
    (low, high)
}

pub fn snap_angle(from: Vec2, to: Vec2) -> Vec2 {
    let along = to - from;
    let length = along.length();
    let step = std::f64::consts::FRAC_PI_4;
    let turn = (along.y.atan2(along.x) / step).round() * step;
    from + Vec2::new(turn.cos(), turn.sin()) * length
}

pub fn snap_square(from: Vec2, to: Vec2) -> Vec2 {
    let along = to - from;
    let side = along.x.abs().max(along.y.abs());
    from + Vec2::new(side.copysign(along.x), side.copysign(along.y))
}

pub fn smooth(points: &[Vec2]) -> Vec<Vec2> {
    let mut now = points.to_vec();
    for _ in 0..2 {
        if now.len() < 3 {
            return now;
        }
        let mut next = Vec::with_capacity(now.len() * 2);
        next.push(now[0]);
        for pair in now.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            next.push(a * 0.75 + b * 0.25);
            next.push(a * 0.25 + b * 0.75);
        }
        next.push(now[now.len() - 1]);
        now = next;
    }
    now
}

fn shifted(points: &[Vec2], by: Vec2) -> Vec<Vec2> {
    points.iter().map(|p| *p + by).collect()
}

pub fn head(a: Vec2, b: Vec2, width: f64, unit: f64) -> Option<([Vec2; 4], Vec2)> {
    let along = b - a;
    let length = along.length();
    if length < 1e-6 {
        return None;
    }
    let dir = along / length;
    let side = Vec2::new(-dir.y, dir.x);
    let long = (width * 3.0 + 7.0 * unit).min(length * 0.8);
    let wide = long * 0.62;
    let base = b - dir * long;
    let notch = b - dir * long * 0.78;
    Some(([b, base + side * wide, notch, base - side * wide], notch + dir * width.min(long * 0.3)))
}

impl Shape {
    pub fn stroke(&self, unit: f64) -> f64 {
        match self.kind {
            Kind::Highlight(_) => self.weight.line() * unit * 4.0 + 6.0 * unit,
            _ => self.weight.line() * unit,
        }
    }

    pub fn text_px(&self, unit: f64) -> f64 {
        self.weight.text() * unit
    }

    pub fn bounds(&self, unit: f64) -> Rect {
        let half = self.stroke(unit) / 2.0;
        let spread = |points: &[Vec2]| {
            let first = points.first().copied().unwrap_or(Vec2::ZERO);
            let (low, high) = points.iter().fold((first, first), |(low, high), p| (low.min(*p), high.max(*p)));
            Rect::new(low - Vec2::splat(half), high - low + Vec2::splat(half * 2.0))
        };
        match &self.kind {
            Kind::Pen(points) | Kind::Highlight(points) => spread(points),
            Kind::Arrow(a, b) => {
                let mut points = vec![*a, *b];
                if let Some((tip, _)) = head(*a, *b, self.stroke(unit), unit) {
                    points.extend(tip);
                }
                spread(&points)
            }
            Kind::Box(a, b) | Kind::Oval(a, b) => {
                let rect = ordered(*a, *b);
                Rect::new(rect.origin - Vec2::splat(half), rect.size + Vec2::splat(half * 2.0))
            }
            Kind::Redact(a, b) => ordered(*a, *b),
            Kind::Text { at, size, .. } => Rect::new(*at, *size),
            Kind::Loupe { center, radius, .. } => Rect::from_center(*center, Vec2::splat(radius * 2.0)),
        }
    }

    pub fn near(&self, p: Vec2, slop: f64, unit: f64) -> bool {
        let half = self.stroke(unit) / 2.0;
        match &self.kind {
            Kind::Pen(points) | Kind::Highlight(points) => match points.as_slice() {
                [one] => (p - *one).length() <= half + slop,
                _ => points.windows(2).any(|pair| draw::to_segment(p, pair[0], pair[1]) <= half + slop),
            },
            Kind::Arrow(a, b) => {
                draw::to_segment(p, *a, *b) <= half + slop
                    || head(*a, *b, self.stroke(unit), unit).is_some_and(|(tip, _)| draw::inside_polygon(p, &tip))
            }
            Kind::Box(a, b) => {
                let rect = ordered(*a, *b);
                let outer = Rect::new(rect.origin - Vec2::splat(half + slop), rect.size + Vec2::splat((half + slop) * 2.0));
                let inner = Rect::new(rect.origin + Vec2::splat(half + slop), rect.size - Vec2::splat((half + slop) * 2.0));
                outer.contains(p) && !inner.contains(p)
            }
            Kind::Oval(a, b) => draw::oval_distance(p, ordered(*a, *b)).abs() <= half + slop,
            Kind::Redact(a, b) => ordered(*a, *b).contains(p),
            Kind::Text { at, size, .. } => Rect::new(*at - Vec2::splat(slop), Vec2::new(size.x.max(slop * 2.0), size.y) + Vec2::splat(slop * 2.0)).contains(p),
            Kind::Loupe { center, radius, .. } => (p - *center).length() <= radius + slop,
        }
    }

    pub fn moved(&self, by: Vec2) -> Shape {
        let kind = match &self.kind {
            Kind::Pen(points) => Kind::Pen(shifted(points, by)),
            Kind::Highlight(points) => Kind::Highlight(shifted(points, by)),
            Kind::Arrow(a, b) => Kind::Arrow(*a + by, *b + by),
            Kind::Box(a, b) => Kind::Box(*a + by, *b + by),
            Kind::Oval(a, b) => Kind::Oval(*a + by, *b + by),
            Kind::Redact(a, b) => Kind::Redact(*a + by, *b + by),
            Kind::Text { at, words, size } => Kind::Text { at: *at + by, words: words.clone(), size: *size },
            Kind::Loupe { center, radius, zoom } => Kind::Loupe { center: *center + by, radius: *radius, zoom: *zoom },
        };
        Shape { kind, ..self.clone() }
    }

    pub fn handles(&self) -> Vec<(Handle, Vec2)> {
        match &self.kind {
            Kind::Box(a, b) | Kind::Oval(a, b) | Kind::Redact(a, b) => {
                let rect = ordered(*a, *b);
                EDGES.iter().map(|edge| (Handle::Edge(*edge), edge_point(rect, *edge))).collect()
            }
            Kind::Arrow(a, b) => vec![(Handle::Start, *a), (Handle::End, *b)],
            Kind::Loupe { center, radius, zoom } => {
                let turn = -std::f64::consts::FRAC_PI_4 * (0.4 + (zoom - 1.0) / 4.0).min(1.6);
                vec![
                    (Handle::Radius, *center + Vec2::new(*radius, 0.0)),
                    (Handle::Zoom, *center + Vec2::new(turn.cos(), turn.sin()) * *radius),
                ]
            }
            _ => Vec::new(),
        }
    }

    pub fn reshaped(&self, handle: Handle, to: Vec2, square: bool) -> Shape {
        let rect_of = |a: Vec2, b: Vec2, edge: u8| {
            let (low, high) = pull(ordered(a, b), edge, to);
            match square && edge.count_ones() == 2 {
                true => {
                    let fixed = edge_point(ordered(a, b), MIDDLE ^ edge);
                    let moving = edge_point(ordered(low, high), edge);
                    (fixed, snap_square(fixed, moving))
                }
                false => (low, high),
            }
        };
        let kind = match (&self.kind, handle) {
            (Kind::Box(a, b), Handle::Edge(edge)) => {
                let (a, b) = rect_of(*a, *b, edge);
                Kind::Box(a, b)
            }
            (Kind::Oval(a, b), Handle::Edge(edge)) => {
                let (a, b) = rect_of(*a, *b, edge);
                Kind::Oval(a, b)
            }
            (Kind::Redact(a, b), Handle::Edge(edge)) => {
                let (a, b) = rect_of(*a, *b, edge);
                Kind::Redact(a, b)
            }
            (Kind::Arrow(_, b), Handle::Start) => Kind::Arrow(if square { snap_angle(*b, to) } else { to }, *b),
            (Kind::Arrow(a, _), Handle::End) => Kind::Arrow(*a, if square { snap_angle(*a, to) } else { to }),
            (Kind::Loupe { center, zoom, .. }, Handle::Radius) => Kind::Loupe { center: *center, radius: (to - *center).length().max(8.0), zoom: *zoom },
            (Kind::Loupe { center, radius, .. }, Handle::Zoom) => {
                let along = to - *center;
                let turn = (-along.y.atan2(along.x)).clamp(0.0, std::f64::consts::PI * 0.4);
                let zoom = 1.0 + (turn / std::f64::consts::FRAC_PI_4 - 0.4) * 4.0;
                Kind::Loupe { center: *center, radius: *radius, zoom: zoom.clamp(1.25, 5.0) }
            }
            (kind, _) => kind.clone(),
        };
        Shape { kind, ..self.clone() }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Doc {
    pub shapes: Vec<Shape>,
    pub crop: Rect,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Grab {
    Draw { index: usize, from: Vec2 },
    Maybe { index: usize, from: Vec2 },
    Move { index: usize, from: Vec2, was: Shape },
    Handle { index: usize, handle: Handle, was: Shape },
    Crop { edge: u8, from: Vec2, was: Rect },
}

pub struct Markup {
    pub source: Image,
    pub doc: Doc,
    pub past: Vec<Doc>,
    pub ahead: Vec<Doc>,
    first: Doc,
    pending: Option<Doc>,
    pub unit: f64,
    pub tool: Tool,
    pub ink: [u8; 3],
    pub weight: Weight,
    pub picked: Option<usize>,
    pub editing: Option<usize>,
    pub grab: Option<Grab>,
}

pub type Measure<'a> = &'a dyn Fn(&str, f64) -> Vec2;

pub fn measure_lines(words: &str, px: f64, width_of: &dyn Fn(&str) -> f64, line: f64) -> Vec2 {
    let lines: Vec<&str> = words.split('\n').collect();
    let widest = lines.iter().map(|each| width_of(each)).fold(0.0, f64::max);
    Vec2::new(widest + px * 0.1, line * lines.len() as f64)
}

impl Markup {
    pub fn new(source: Image) -> Markup {
        let doc = Doc { shapes: Vec::new(), crop: Rect::new(Vec2::ZERO, source.size()) };
        Markup {
            source,
            first: doc.clone(),
            doc,
            past: Vec::new(),
            ahead: Vec::new(),
            pending: None,
            unit: 1.0,
            tool: Tool::Pen,
            ink: INKS[0].0,
            weight: Weight::Medium,
            picked: None,
            editing: None,
            grab: None,
        }
    }

    pub fn dirty(&self) -> bool {
        match self.editing.filter(|at| self.text_of(*at).is_some_and(|words| words.trim().is_empty())) {
            Some(at) => {
                let mut doc = self.doc.clone();
                doc.shapes.remove(at);
                doc != self.first
            }
            None => self.doc != self.first,
        }
    }

    pub fn text_of(&self, index: usize) -> Option<&str> {
        match &self.doc.shapes.get(index)?.kind {
            Kind::Text { words, .. } => Some(words),
            _ => None,
        }
    }

    pub fn begin(&mut self) {
        if self.pending.is_none() {
            self.pending = Some(self.doc.clone());
        }
    }

    pub fn commit(&mut self) {
        if let Some(before) = self.pending.take()
            && before != self.doc
        {
            self.past.push(before);
            self.ahead.clear();
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty() || self.pending.as_ref().is_some_and(|before| *before != self.doc)
    }

    pub fn can_redo(&self) -> bool {
        !self.ahead.is_empty()
    }

    fn settle(&mut self) {
        self.grab = None;
        self.end_text();
        self.commit();
    }

    pub fn undo(&mut self) {
        self.settle();
        if let Some(before) = self.past.pop() {
            self.ahead.push(std::mem::replace(&mut self.doc, before));
            self.picked = None;
        }
    }

    pub fn redo(&mut self) {
        self.settle();
        if let Some(after) = self.ahead.pop() {
            self.past.push(std::mem::replace(&mut self.doc, after));
            self.picked = None;
        }
    }

    pub fn shown(&self) -> Rect {
        match self.tool {
            Tool::Crop => Rect::new(Vec2::ZERO, self.source.size()),
            _ => self.doc.crop,
        }
    }

    pub fn set_tool(&mut self, tool: Tool) {
        self.settle();
        if tool != self.tool {
            self.picked = None;
        }
        self.tool = tool;
    }

    fn restyle(&mut self, change: impl Fn(&mut Shape)) {
        let Some(index) = self.editing.or(self.picked) else {
            return;
        };
        if self.editing.is_none() {
            self.begin();
        }
        if let Some(shape) = self.doc.shapes.get_mut(index) {
            change(shape);
        }
        if self.editing.is_none() {
            self.commit();
        }
    }

    pub fn set_ink(&mut self, ink: [u8; 3]) {
        self.ink = ink;
        self.restyle(|shape| shape.ink = ink);
    }

    pub fn set_weight(&mut self, weight: Weight, measure: Measure) {
        self.weight = weight;
        self.restyle(|shape| shape.weight = weight);
        if let Some(index) = self.editing.or(self.picked) {
            self.remeasure(index, measure);
        }
    }

    pub fn text_mode(&self) -> bool {
        let shape = self.editing.or(self.picked).and_then(|index| self.doc.shapes.get(index));
        match shape {
            Some(shape) => matches!(shape.kind, Kind::Text { .. }),
            None => self.tool == Tool::Text,
        }
    }

    pub fn remove(&mut self) {
        let Some(index) = self.picked.take() else {
            return;
        };
        if index < self.doc.shapes.len() {
            self.begin();
            self.doc.shapes.remove(index);
            self.commit();
        }
    }

    pub fn nudge(&mut self, by: Vec2) {
        let Some(index) = self.picked else {
            return;
        };
        if let Some(shape) = self.doc.shapes.get(index).cloned() {
            self.begin();
            self.doc.shapes[index] = shape.moved(by);
            self.commit();
        }
    }

    pub fn hit(&self, at: Vec2, slop: f64) -> Option<usize> {
        (0..self.doc.shapes.len()).rev().find(|index| self.doc.shapes[*index].near(at, slop, self.unit))
    }

    pub fn handle_at(&self, at: Vec2, slop: f64) -> Option<Handle> {
        let shape = self.doc.shapes.get(self.picked?)?;
        shape.handles().into_iter().filter(|(_, spot)| (*spot - at).length() <= slop).min_by(|a, b| (a.1 - at).length().total_cmp(&(b.1 - at).length())).map(|(handle, _)| handle)
    }

    pub fn crop_edge(&self, at: Vec2, slop: f64) -> Option<u8> {
        let crop = self.doc.crop;
        let near = EDGES.iter().copied().filter(|edge| (edge_point(crop, *edge) - at).length() <= slop * 1.6).min_by(|a, b| (edge_point(crop, *a) - at).length().total_cmp(&(edge_point(crop, *b) - at).length()));
        if let Some(edge) = near {
            return Some(edge);
        }
        let grown = Rect::new(crop.origin - Vec2::splat(slop), crop.size + Vec2::splat(slop * 2.0));
        if !grown.contains(at) {
            return None;
        }
        let mut edge = 0;
        if (at.x - crop.origin.x).abs() <= slop {
            edge |= LEFT;
        }
        if (at.x - crop.max().x).abs() <= slop {
            edge |= RIGHT;
        }
        if (at.y - crop.origin.y).abs() <= slop {
            edge |= TOP;
        }
        if (at.y - crop.max().y).abs() <= slop {
            edge |= BOTTOM;
        }
        Some(if edge == 0 { MIDDLE } else { edge })
    }

    fn fresh(&self, kind: Kind) -> Shape {
        Shape { kind, ink: self.ink, weight: self.weight }
    }

    pub fn press(&mut self, at: Vec2, slop: f64, double: bool, measure: Measure) {
        if let Some(index) = self.editing {
            if self.doc.shapes.get(index).is_some_and(|shape| shape.near(at, slop, self.unit)) {
                return;
            }
            self.end_text();
            self.commit();
            if self.tool == Tool::Text {
                return;
            }
        }
        self.commit();
        if self.tool == Tool::Crop {
            if let Some(edge) = self.crop_edge(at, slop) {
                self.begin();
                self.grab = Some(Grab::Crop { edge, from: at, was: self.doc.crop });
            }
            return;
        }
        if let (Some(index), Some(handle)) = (self.picked, self.handle_at(at, slop * 1.4)) {
            self.begin();
            self.grab = Some(Grab::Handle { index, handle, was: self.doc.shapes[index].clone() });
            return;
        }
        if let Some(index) = self.hit(at, slop) {
            let text = matches!(self.doc.shapes[index].kind, Kind::Text { .. });
            if text && (double || self.tool == Tool::Text) {
                self.edit_text(index);
                return;
            }
            self.begin();
            if self.tool.freehand() && self.picked != Some(index) {
                self.grab = Some(Grab::Maybe { index, from: at });
                return;
            }
            self.picked = Some(index);
            self.grab = Some(Grab::Move { index, from: at, was: self.doc.shapes[index].clone() });
            return;
        }
        self.picked = None;
        self.begin();
        let kind = match self.tool {
            Tool::Pen => Kind::Pen(vec![at]),
            Tool::Highlight => Kind::Highlight(vec![at]),
            Tool::Arrow => Kind::Arrow(at, at),
            Tool::Box => Kind::Box(at, at),
            Tool::Oval => Kind::Oval(at, at),
            Tool::Redact => Kind::Redact(at, at),
            Tool::Loupe => Kind::Loupe { center: at, radius: 0.0, zoom: 2.0 },
            Tool::Text => {
                let px = self.weight.text() * self.unit;
                let size = measure("", px);
                Kind::Text { at: at - Vec2::new(0.0, size.y / 2.0), words: String::new(), size }
            }
            Tool::Crop => return,
        };
        let shape = self.fresh(kind);
        self.doc.shapes.push(shape);
        let index = self.doc.shapes.len() - 1;
        match self.tool {
            Tool::Text => self.editing = Some(index),
            _ => self.grab = Some(Grab::Draw { index, from: at }),
        }
    }

    pub fn drag(&mut self, at: Vec2, slop: f64, shift: bool) {
        let Some(grab) = self.grab.clone() else {
            return;
        };
        match grab {
            Grab::Maybe { from, .. } => {
                if (at - from).length() > slop {
                    let kind = match self.tool {
                        Tool::Highlight => Kind::Highlight(vec![from, at]),
                        _ => Kind::Pen(vec![from, at]),
                    };
                    let shape = self.fresh(kind);
                    self.doc.shapes.push(shape);
                    self.grab = Some(Grab::Draw { index: self.doc.shapes.len() - 1, from });
                }
            }
            Grab::Draw { index, from } => {
                let Some(shape) = self.doc.shapes.get_mut(index) else {
                    return;
                };
                match &mut shape.kind {
                    Kind::Pen(points) | Kind::Highlight(points) => {
                        let last = points.last().copied().unwrap_or(from);
                        let gap = (at - last).length();
                        if gap >= slop * 0.25 {
                            if shift && !points.is_empty() {
                                points.truncate(1);
                                points.push(snap_angle(points[0], at));
                            } else {
                                let eased = last + (at - last) * 0.6_f64.max((gap / (slop * 2.0)).min(1.0));
                                points.push(eased);
                            }
                        }
                    }
                    Kind::Arrow(_, b) => *b = if shift { snap_angle(from, at) } else { at },
                    Kind::Box(_, b) | Kind::Oval(_, b) | Kind::Redact(_, b) => *b = if shift { snap_square(from, at) } else { at },
                    Kind::Loupe { radius, .. } => *radius = (at - from).length(),
                    Kind::Text { .. } => {}
                }
            }
            Grab::Move { index, from, was } => {
                let mut by = at - from;
                if shift {
                    by = if by.x.abs() > by.y.abs() { Vec2::new(by.x, 0.0) } else { Vec2::new(0.0, by.y) };
                }
                if let Some(shape) = self.doc.shapes.get_mut(index) {
                    *shape = was.moved(by);
                }
            }
            Grab::Handle { index, handle, was } => {
                if let Some(shape) = self.doc.shapes.get_mut(index) {
                    *shape = was.reshaped(handle, at, shift);
                }
            }
            Grab::Crop { edge, from, was } => self.doc.crop = self.cropped(was, edge, from, at),
        }
    }

    fn cropped(&self, was: Rect, edge: u8, from: Vec2, at: Vec2) -> Rect {
        let size = self.source.size();
        if edge == MIDDLE {
            let by = at - from;
            let origin = (was.origin + by).clamp(Vec2::ZERO, (size - was.size).max(Vec2::ZERO));
            return Rect::new(origin, was.size);
        }
        let least = (24.0 * self.unit).min(size.x).min(size.y);
        let to = at.clamp(Vec2::ZERO, size);
        let (mut low, mut high) = pull(was, edge, to);
        if edge & LEFT != 0 {
            low.x = low.x.min(high.x - least);
        }
        if edge & RIGHT != 0 {
            high.x = high.x.max(low.x + least);
        }
        if edge & TOP != 0 {
            low.y = low.y.min(high.y - least);
        }
        if edge & BOTTOM != 0 {
            high.y = high.y.max(low.y + least);
        }
        let low = low.clamp(Vec2::ZERO, size);
        let high = high.clamp(Vec2::ZERO, size);
        Rect::new(low.round(), (high.round() - low.round()).max(Vec2::splat(1.0)))
    }

    pub fn release(&mut self, slop: f64) {
        let Some(grab) = self.grab.take() else {
            return;
        };
        let unit = self.unit;
        match grab {
            Grab::Maybe { index, .. } => self.picked = Some(index),
            Grab::Draw { index, from } => {
                let Some(shape) = self.doc.shapes.get_mut(index) else {
                    return;
                };
                let small = shape.bounds(unit).size.x.max(shape.bounds(unit).size.y) < slop * 2.0 + shape.stroke(unit);
                match &mut shape.kind {
                    Kind::Pen(points) | Kind::Highlight(points) => *points = smooth(points),
                    Kind::Arrow(a, b) if (*b - *a).length() < slop * 2.0 => {
                        *a = from + Vec2::new(-90.0, 60.0) * unit;
                        *b = from;
                    }
                    Kind::Box(a, b) | Kind::Oval(a, b) | Kind::Redact(a, b) if small => {
                        *a = from - Vec2::new(80.0, 50.0) * unit;
                        *b = from + Vec2::new(80.0, 50.0) * unit;
                    }
                    Kind::Loupe { radius, .. } if *radius < slop * 2.0 => *radius = 70.0 * unit,
                    _ => {}
                }
                if !matches!(shape.kind, Kind::Pen(_) | Kind::Highlight(_)) {
                    self.picked = Some(index);
                }
            }
            _ => {}
        }
        self.commit();
    }

    pub fn edit_text(&mut self, index: usize) {
        self.begin();
        self.picked = None;
        self.editing = Some(index);
        if let Some(shape) = self.doc.shapes.get(index) {
            self.ink = shape.ink;
            self.weight = shape.weight;
        }
    }

    fn remeasure(&mut self, index: usize, measure: Measure) {
        let unit = self.unit;
        if let Some(shape) = self.doc.shapes.get_mut(index) {
            let px = shape.text_px(unit);
            if let Kind::Text { words, size, .. } = &mut shape.kind {
                *size = measure(words, px);
            }
        }
    }

    pub fn type_text(&mut self, typed: &str, measure: Measure) {
        let Some(index) = self.editing else {
            return;
        };
        if let Some(Shape { kind: Kind::Text { words, .. }, .. }) = self.doc.shapes.get_mut(index) {
            words.push_str(typed);
        }
        self.remeasure(index, measure);
    }

    pub fn erase_text(&mut self, word: bool, measure: Measure) {
        let Some(index) = self.editing else {
            return;
        };
        if let Some(Shape { kind: Kind::Text { words, .. }, .. }) = self.doc.shapes.get_mut(index) {
            match word {
                true => {
                    let kept = words.trim_end_matches(|c: char| c.is_whitespace()).len();
                    words.truncate(kept);
                    let cut = words.rfind(|c: char| c.is_whitespace()).map_or(0, |at| at + 1);
                    words.truncate(cut);
                }
                false => {
                    words.pop();
                }
            }
        }
        self.remeasure(index, measure);
    }

    pub fn end_text(&mut self) {
        let Some(index) = self.editing.take() else {
            return;
        };
        let empty = self.text_of(index).is_some_and(|words| words.trim().is_empty());
        match empty {
            true => {
                self.doc.shapes.remove(index);
            }
            false => self.picked = Some(index),
        }
    }

    pub fn finish(&mut self) {
        self.settle();
    }

    pub fn render(&self, glyphs: Glyphs) -> Image {
        let mut image = self.source.clone();
        let map = Map { scale: 1.0, shift: Vec2::ZERO };
        let clip = draw::whole(&image);
        for shape in &self.doc.shapes {
            paint(&mut image, shape, self.unit, &self.source, map, clip, glyphs);
        }
        let crop = self.doc.crop;
        draw::cut(&image, [crop.origin.x.round() as i64, crop.origin.y.round() as i64, crop.max().x.round() as i64, crop.max().y.round() as i64])
    }
}

pub fn png_bytes(image: &Image) -> Result<Vec<u8>, String> {
    let mut straight = image.clone();
    straight.straighten();
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| {
            writer.write_image_data(&straight.rgba)?;
            writer.finish()
        })
        .map_err(|e| e.to_string())?;
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Map {
    pub scale: f64,
    pub shift: Vec2,
}

impl Map {
    pub fn to(&self, p: Vec2) -> Vec2 {
        p * self.scale + self.shift
    }

    pub fn from(&self, q: Vec2) -> Vec2 {
        (q - self.shift) / self.scale
    }

    pub fn rect(&self, rect: Rect) -> Rect {
        Rect::new(self.to(rect.origin), rect.size * self.scale)
    }
}

pub fn fit(shown: Rect, area: Rect, pad: f64, most: f64) -> Map {
    let room = Vec2::new((area.size.x - pad * 2.0).max(1.0), (area.size.y - pad * 2.0).max(1.0));
    let scale = (room.x / shown.size.x.max(1.0)).min(room.y / shown.size.y.max(1.0)).min(most);
    let shift = area.center() - shown.center() * scale;
    Map { scale, shift: shift.round() }
}

pub type Glyphs<'a> = &'a dyn Fn(&str, f64, [u8; 3]) -> Option<Image>;

pub fn halo_of(ink: [u8; 3]) -> ([u8; 3], f32) {
    let light = 0.2126 * f64::from(ink[0]) + 0.7152 * f64::from(ink[1]) + 0.0722 * f64::from(ink[2]);
    match light > 150.0 {
        true => ([0, 0, 0], 0.55),
        false => ([255, 255, 255], 0.9),
    }
}

pub fn paint(target: &mut Image, shape: &Shape, unit: f64, source: &Image, map: Map, clip: Clip, glyphs: Glyphs) {
    let scale = map.scale;
    let half = shape.stroke(unit) * scale / 2.0;
    let bounds = map.rect(shape.bounds(unit));
    let area = Rect::new(bounds.origin - Vec2::splat(4.0), bounds.size + Vec2::splat(8.0));
    match &shape.kind {
        Kind::Pen(points) => {
            let mut mask = Mask::new(area, clip);
            mask.line(&points.iter().map(|p| map.to(*p)).collect::<Vec<_>>(), half);
            draw::over(target, &mask, shape.ink, 1.0);
        }
        Kind::Highlight(points) => {
            let mut mask = Mask::new(area, clip);
            mask.line(&points.iter().map(|p| map.to(*p)).collect::<Vec<_>>(), half);
            draw::multiply(target, &mask, shape.ink, 0.9);
        }
        Kind::Arrow(a, b) => {
            let mut mask = Mask::new(area, clip);
            let (a, b) = (map.to(*a), map.to(*b));
            match head(a, b, half * 2.0, unit * scale) {
                Some((tip, end)) => {
                    mask.segment(a, end, half);
                    mask.polygon(&tip);
                }
                None => mask.segment(a, b, half),
            }
            draw::over(target, &mask, shape.ink, 1.0);
        }
        Kind::Box(a, b) => {
            let mut mask = Mask::new(area, clip);
            mask.frame(map.rect(ordered(*a, *b)), half);
            draw::over(target, &mask, shape.ink, 1.0);
        }
        Kind::Oval(a, b) => {
            let mut mask = Mask::new(area, clip);
            mask.oval(map.rect(ordered(*a, *b)), half);
            draw::over(target, &mask, shape.ink, 1.0);
        }
        Kind::Redact(a, b) => redact(target, source, ordered(*a, *b), shape.weight.block() * unit, map, clip),
        Kind::Loupe { center, radius, zoom } => loupe(target, source, *center, *radius, *zoom, shape, unit, map, clip),
        Kind::Text { at, words, .. } => {
            let px = shape.text_px(unit) * scale;
            let (halo, strength) = halo_of(shape.ink);
            let grow = (px * 0.07).ceil().max(1.0) as usize;
            let mut y = map.to(*at).y;
            let x = map.to(*at).x;
            for line in words.split('\n') {
                let Some(piece) = glyphs(line, px, shape.ink) else {
                    continue;
                };
                let spot = (x.round() as i64, y.round() as i64);
                let soft = Mask::from_alpha(&piece, (spot.0, spot.1 + (px * 0.03).round() as i64), grow, 2.6, clip);
                draw::over(target, &soft, halo, strength);
                draw::stamp(target, &piece, spot, clip);
                y += f64::from(piece.height);
            }
        }
    }
}

fn redact(target: &mut Image, source: &Image, rect: Rect, block: f64, map: Map, clip: Clip) {
    let size = source.size();
    let low = rect.origin.clamp(Vec2::ZERO, size);
    let high = rect.max().clamp(Vec2::ZERO, size);
    let block = block.max(2.0);
    let (across, down) = (((high.x - low.x) / block).ceil().max(1.0) as usize, ((high.y - low.y) / block).ceil().max(1.0) as usize);
    if high.x - low.x < 1.0 || high.y - low.y < 1.0 {
        return;
    }
    let mut sums = vec![[0f64; 5]; across * down];
    let width = source.width as usize;
    for y in low.y.floor() as usize..(high.y.ceil() as usize).min(source.height as usize) {
        let row = (((y as f64 + 0.5 - low.y) / block).floor().max(0.0) as usize).min(down - 1);
        for x in low.x.floor() as usize..(high.x.ceil() as usize).min(width) {
            let column = (((x as f64 + 0.5 - low.x) / block).floor().max(0.0) as usize).min(across - 1);
            let at = (y * width + x) * 4;
            let sum = &mut sums[row * across + column];
            for (total, value) in sum.iter_mut().zip(&source.rgba[at..at + 4]) {
                *total += f64::from(*value);
            }
            sum[4] += 1.0;
        }
    }
    let colors: Vec<[u8; 4]> = sums.iter().map(|sum| [0, 1, 2, 3].map(|c| (sum[c] / sum[4].max(1.0)).round() as u8)).collect();
    let shown = draw::clip_of(map.rect(Rect::new(low, high - low)), clip);
    let tw = i64::from(target.width);
    for y in shown[1].max(0)..shown[3].min(i64::from(target.height)) {
        for x in shown[0].max(0)..shown[2].min(tw) {
            let p = map.from(Vec2::new(x as f64 + 0.5, y as f64 + 0.5));
            let column = (((p.x - low.x) / block).floor().max(0.0) as usize).min(across - 1);
            let row = (((p.y - low.y) / block).floor().max(0.0) as usize).min(down - 1);
            let at = ((y * tw + x) * 4) as usize;
            target.rgba[at..at + 4].copy_from_slice(&colors[row * across + column]);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn loupe(target: &mut Image, source: &Image, center: Vec2, radius: f64, zoom: f64, shape: &Shape, unit: f64, map: Map, clip: Clip) {
    let (c, r) = (map.to(center), radius * map.scale);
    if r < 1.0 {
        return;
    }
    let ring = (shape.weight.line() * 0.75 + 1.5) * unit * map.scale;
    let shade = 10.0 * unit * map.scale;
    let outer = r + ring;
    let mut glow = Mask::new(Rect::from_center(c + Vec2::new(0.0, shade * 0.3), Vec2::splat((outer + shade) * 2.0 + 2.0)), clip);
    let lift = c + Vec2::new(0.0, shade * 0.3);
    glow.each(Rect::from_center(lift, Vec2::splat((outer + shade) * 2.0 + 2.0)), |p| {
        let d = (p - lift).length() - outer;
        let t = (1.0 - d / shade).clamp(0.0, 1.0);
        t * t * 0.45
    });
    draw::over(target, &glow, [0, 0, 0], 1.0);
    let area = draw::clip_of(Rect::from_center(c, Vec2::splat(r * 2.0 + 2.0)), clip);
    let tw = i64::from(target.width);
    for y in area[1].max(0)..area[3].min(i64::from(target.height)) {
        for x in area[0].max(0)..area[2].min(tw) {
            let q = Vec2::new(x as f64 + 0.5, y as f64 + 0.5);
            let cover = (r - (q - c).length() + 0.5).clamp(0.0, 1.0) as f32;
            if cover <= 0.0 {
                continue;
            }
            let p = map.from(q);
            let from = center + (p - center) / zoom;
            let got = draw::sample(source, from);
            let at = ((y * tw + x) * 4) as usize;
            for channel in 0..4 {
                let under = f32::from(target.rgba[at + channel]);
                let value = if channel == 3 { 255.0 } else { got[channel] + (255.0 - got[3]) };
                target.rgba[at + channel] = (value * cover + under * (1.0 - cover)).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
    let mut edge = Mask::new(Rect::from_center(c, Vec2::splat(outer * 2.0 + 4.0)), clip);
    edge.each(Rect::from_center(c, Vec2::splat(outer * 2.0 + 4.0)), |p| {
        let d = (p - c).length();
        (outer - d + 0.5).min(d - r + 0.5)
    });
    draw::over(target, &edge, shape.ink, 1.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(w: u32, h: u32, color: [u8; 4]) -> Image {
        Image { width: w, height: h, rgba: color.iter().copied().cycle().take((w * h * 4) as usize).collect() }
    }

    fn measure(words: &str, px: f64) -> Vec2 {
        Vec2::new(words.chars().count() as f64 * px * 0.5, px * 1.2)
    }

    fn glyphs(words: &str, px: f64, ink: [u8; 3]) -> Option<Image> {
        let (w, h) = ((words.chars().count() as f64 * px * 0.5).ceil().max(1.0) as u32, (px * 1.2).ceil() as u32);
        Some(picture(w, h, [ink[0], ink[1], ink[2], 255]))
    }

    fn at(x: f64, y: f64) -> Vec2 {
        Vec2::new(x, y)
    }

    fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let at = ((y * image.width + x) * 4) as usize;
        [image.rgba[at], image.rgba[at + 1], image.rgba[at + 2], image.rgba[at + 3]]
    }

    fn drawn(markup: &mut Markup, tool: Tool, from: Vec2, to: Vec2) {
        markup.set_tool(tool);
        markup.press(from, 4.0, false, &measure);
        markup.drag((from + to) / 2.0, 4.0, false);
        markup.drag(to, 4.0, false);
        markup.release(4.0);
    }

    #[test]
    fn drawing_a_box_picks_it_and_undo_takes_it_away() {
        let mut markup = Markup::new(picture(200, 100, [255, 255, 255, 255]));
        assert!(!markup.dirty());
        drawn(&mut markup, Tool::Box, at(20.0, 20.0), at(120.0, 70.0));
        assert_eq!(markup.doc.shapes.len(), 1);
        assert_eq!(markup.picked, Some(0));
        assert!(markup.dirty());
        markup.undo();
        assert!(markup.doc.shapes.is_empty());
        assert!(!markup.dirty());
        markup.redo();
        assert_eq!(markup.doc.shapes.len(), 1);
    }

    #[test]
    fn a_shape_moves_when_dragged_and_resizes_from_a_handle() {
        let mut markup = Markup::new(picture(200, 100, [255, 255, 255, 255]));
        drawn(&mut markup, Tool::Box, at(20.0, 20.0), at(120.0, 70.0));
        markup.press(at(20.0, 45.0), 4.0, false, &measure);
        assert!(matches!(markup.grab, Some(Grab::Handle { handle: Handle::Edge(LEFT), .. })));
        markup.drag(at(10.0, 45.0), 4.0, false);
        markup.release(4.0);
        assert_eq!(markup.doc.shapes[0].kind, Kind::Box(at(10.0, 20.0), at(120.0, 70.0)));
        markup.press(at(70.0, 20.0), 4.0, false, &measure);
        assert!(matches!(markup.grab, Some(Grab::Handle { .. })));
        markup.release(4.0);
        markup.press(at(120.0, 30.0), 4.0, false, &measure);
        assert!(matches!(markup.grab, Some(Grab::Move { .. })));
        markup.drag(at(130.0, 40.0), 4.0, false);
        markup.release(4.0);
        assert_eq!(markup.doc.shapes[0].kind, Kind::Box(at(20.0, 30.0), at(130.0, 80.0)));
        assert_eq!(markup.past.len(), 3);
    }

    #[test]
    fn a_click_with_a_shape_tool_makes_a_default_shape() {
        let mut markup = Markup::new(picture(400, 300, [255, 255, 255, 255]));
        markup.set_tool(Tool::Oval);
        markup.press(at(200.0, 150.0), 4.0, false, &measure);
        markup.release(4.0);
        let bounds = ordered(at(120.0, 100.0), at(280.0, 200.0));
        assert!(matches!(markup.doc.shapes[0].kind, Kind::Oval(a, b) if ordered(a, b) == bounds));
    }

    #[test]
    fn the_pen_draws_over_shapes_but_a_click_picks_them() {
        let mut markup = Markup::new(picture(200, 100, [255, 255, 255, 255]));
        drawn(&mut markup, Tool::Box, at(20.0, 20.0), at(120.0, 70.0));
        markup.set_tool(Tool::Pen);
        markup.press(at(20.0, 40.0), 4.0, false, &measure);
        markup.release(4.0);
        assert_eq!(markup.picked, Some(0));
        assert_eq!(markup.doc.shapes.len(), 1);
        markup.picked = None;
        drawn(&mut markup, Tool::Pen, at(20.0, 30.0), at(90.0, 30.0));
        assert_eq!(markup.doc.shapes.len(), 2);
        assert!(matches!(&markup.doc.shapes[1].kind, Kind::Pen(points) if points.len() > 2));
    }

    #[test]
    fn delete_removes_the_picked_shape_and_undo_brings_it_back() {
        let mut markup = Markup::new(picture(200, 100, [255, 255, 255, 255]));
        drawn(&mut markup, Tool::Arrow, at(20.0, 20.0), at(120.0, 70.0));
        markup.remove();
        assert!(markup.doc.shapes.is_empty());
        markup.undo();
        assert_eq!(markup.doc.shapes.len(), 1);
    }

    #[test]
    fn text_is_typed_and_an_empty_box_leaves_nothing() {
        let mut markup = Markup::new(picture(300, 100, [255, 255, 255, 255]));
        markup.set_tool(Tool::Text);
        markup.press(at(10.0, 50.0), 4.0, false, &measure);
        assert_eq!(markup.editing, Some(0));
        markup.type_text("Hi there", &measure);
        markup.erase_text(true, &measure);
        markup.type_text("you", &measure);
        assert_eq!(markup.text_of(0), Some("Hi you"));
        markup.end_text();
        markup.commit();
        assert_eq!(markup.past.len(), 1);
        markup.press(at(200.0, 20.0), 4.0, false, &measure);
        markup.end_text();
        markup.commit();
        assert_eq!(markup.doc.shapes.len(), 1);
        assert_eq!(markup.past.len(), 1);
    }

    #[test]
    fn changing_the_ink_restyles_the_picked_shape_with_undo() {
        let mut markup = Markup::new(picture(200, 100, [255, 255, 255, 255]));
        drawn(&mut markup, Tool::Box, at(20.0, 20.0), at(120.0, 70.0));
        markup.set_ink(INKS[4].0);
        assert_eq!(markup.doc.shapes[0].ink, INKS[4].0);
        markup.undo();
        assert_eq!(markup.doc.shapes[0].ink, INKS[0].0);
    }

    #[test]
    fn crop_handles_stay_inside_the_picture() {
        let mut markup = Markup::new(picture(200, 100, [255, 255, 255, 255]));
        markup.set_tool(Tool::Crop);
        markup.press(at(0.0, 0.0), 4.0, false, &measure);
        markup.drag(at(-50.0, 30.0), 4.0, false);
        markup.release(4.0);
        assert_eq!(markup.doc.crop, Rect::new(at(0.0, 30.0), at(200.0, 70.0)));
        markup.press(at(100.0, 60.0), 4.0, false, &measure);
        markup.drag(at(150.0, 40.0), 4.0, false);
        markup.release(4.0);
        assert_eq!(markup.doc.crop.origin, at(0.0, 10.0));
        markup.press(at(200.0, 80.0), 4.0, false, &measure);
        markup.drag(at(5.0, 31.0), 4.0, false);
        markup.release(4.0);
        assert_eq!(markup.doc.crop.size, at(24.0, 24.0));
    }

    #[test]
    fn render_crops_at_full_size_and_paints_shapes() {
        let mut markup = Markup::new(picture(200, 100, [255, 255, 255, 255]));
        drawn(&mut markup, Tool::Pen, at(10.0, 50.0), at(190.0, 50.0));
        markup.doc.crop = Rect::new(at(0.0, 25.0), at(200.0, 50.0));
        let out = markup.render(&glyphs);
        assert_eq!((out.width, out.height), (200, 50));
        assert_eq!(pixel(&out, 100, 25), [255, 59, 48, 255]);
        assert_eq!(pixel(&out, 100, 5), [255, 255, 255, 255]);
    }

    #[test]
    fn the_highlighter_keeps_black_text_black() {
        let mut source = picture(100, 40, [255, 255, 255, 255]);
        for x in 0..100 {
            let at = ((20 * 100 + x) * 4) as usize;
            source.rgba[at..at + 3].copy_from_slice(&[0, 0, 0]);
        }
        let mut markup = Markup::new(source);
        markup.ink = INKS[2].0;
        drawn(&mut markup, Tool::Highlight, at(5.0, 20.0), at(95.0, 20.0));
        let out = markup.render(&glyphs);
        assert_eq!(pixel(&out, 50, 20), [0, 0, 0, 255]);
        let lit = pixel(&out, 50, 17);
        assert!(lit[2] < 100 && lit[0] > 240);
    }

    #[test]
    fn redact_hides_detail_and_the_loupe_magnifies() {
        let mut source = picture(100, 100, [255, 255, 255, 255]);
        for y in 0..100u32 {
            for x in 0..100u32 {
                if (x + y) % 2 == 0 {
                    let at = ((y * 100 + x) * 4) as usize;
                    source.rgba[at..at + 3].copy_from_slice(&[0, 0, 0]);
                }
            }
        }
        let mut markup = Markup::new(source);
        drawn(&mut markup, Tool::Redact, at(0.0, 0.0), at(44.0, 44.0));
        let out = markup.render(&glyphs);
        let (a, b) = (pixel(&out, 3, 3), pixel(&out, 4, 3));
        assert_eq!(a, b);
        assert!(a[0] > 100 && a[0] < 160);
        let mut markup = Markup::new(picture(100, 100, [255, 255, 255, 255]));
        markup.source.rgba[(50 * 100 + 50) * 4..(50 * 100 + 50) * 4 + 3].copy_from_slice(&[0, 0, 0]);
        markup.set_tool(Tool::Loupe);
        markup.press(at(50.5, 50.5), 4.0, false, &measure);
        markup.drag(at(80.5, 50.5), 4.0, false);
        markup.release(4.0);
        let out = markup.render(&glyphs);
        assert!(pixel(&out, 51, 50)[0] < 200);
        assert_eq!(pixel(&out, 54, 50)[0], 255);
    }

    #[test]
    fn the_view_map_fits_and_never_blows_up_past_natural_size() {
        let map = fit(Rect::new(Vec2::ZERO, at(400.0, 200.0)), Rect::new(at(0.0, 50.0), at(1000.0, 650.0)), 24.0, 0.5);
        assert_eq!(map.scale, 0.5);
        let map = fit(Rect::new(Vec2::ZERO, at(4000.0, 2000.0)), Rect::new(at(0.0, 50.0), at(1000.0, 650.0)), 24.0, 1.0);
        assert!((map.scale - 952.0 / 4000.0).abs() < 1e-9);
        let back = map.from(map.to(at(123.0, 456.0)));
        assert!((back - at(123.0, 456.0)).length() < 1e-9);
    }

    #[test]
    fn the_clipboard_copy_is_a_png() {
        let bytes = png_bytes(&picture(3, 2, [10, 20, 30, 255])).unwrap();
        assert!(bytes.starts_with(&[0x89, b'P', b'N', b'G']));
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let reader = decoder.read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (3, 2));
    }

    #[test]
    fn smoothing_keeps_the_ends() {
        let points = [at(0.0, 0.0), at(10.0, 0.0), at(10.0, 10.0)];
        let smooth = smooth(&points);
        assert_eq!(smooth.first(), Some(&points[0]));
        assert_eq!(smooth.last(), Some(&points[2]));
        assert!(smooth.len() > points.len());
    }

    #[test]
    fn the_loupe_zoom_handle_turns_the_zoom() {
        let shape = Shape { kind: Kind::Loupe { center: at(0.0, 0.0), radius: 50.0, zoom: 2.0 }, ink: INKS[0].0, weight: Weight::Medium };
        let zoom = shape.handles()[1].1;
        let again = shape.reshaped(Handle::Zoom, zoom, false);
        assert!(matches!(again.kind, Kind::Loupe { zoom, .. } if (zoom - 2.0).abs() < 1e-6));
        let more = shape.reshaped(Handle::Zoom, at(0.0, -50.0), false);
        assert!(matches!(more.kind, Kind::Loupe { zoom, .. } if zoom > 2.0));
    }
}
