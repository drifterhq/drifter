use drifter_core::lang::t;
use drifter_core::paint::{Canvas, density, logical};
use drifter_core::text::Font;
use drifter_core::{Rect, Vec2};

use crate::draw::{self, Mask};
use crate::markup::{self, Handle, INKS, Kind, Map, Shape, Tool, Weight};

pub const BAR_H: f64 = 50.0;
pub const PAD: f64 = 28.0;
const BUTTON: f64 = 30.0;
const TOOL_W: f64 = 32.0;
const INK_W: f64 = 22.0;
const WEIGHT_W: f64 = 30.0;
const ZOOM: [u8; 3] = [52, 199, 89];

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Tool(Tool),
    Ink(usize),
    Weight(Weight),
    Undo,
    Redo,
    Copy,
    Trash,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Save,
    Discard,
    Cancel,
}

pub struct Fonts<'a> {
    pub body: &'a Font,
    pub small: &'a Font,
    pub title: &'a Font,
}

pub struct Bar {
    pub tool: Tool,
    pub ink: [u8; 3],
    pub weight: Weight,
    pub text: bool,
    pub undo: bool,
    pub redo: bool,
    pub hover: Option<Hit>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub bar: [u8; 3],
    pub back: [u8; 3],
    pub card: [u8; 3],
    pub raised: [u8; 3],
    pub hover: [u8; 3],
    pub rule: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub accent: [u8; 3],
    pub on_accent: [u8; 3],
    pub drop: f64,
    pub veil: f64,
    pub dark: bool,
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(Vec2::new(x, y), Vec2::new(w.max(0.0), h.max(0.0)))
}

fn mix(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    let at = |i: usize| (f64::from(a[i]) + (f64::from(b[i]) - f64::from(a[i])) * t).round().clamp(0.0, 255.0) as u8;
    [at(0), at(1), at(2)]
}

fn lift(color: [u8; 3], amount: f64) -> [u8; 3] {
    match amount >= 0.0 {
        true => mix(color, [255, 255, 255], amount),
        false => mix(color, [0, 0, 0], -amount),
    }
}

fn dense_rect(area: Rect) -> Rect {
    let d = density();
    Rect::new(area.origin * d, area.size * d)
}

fn curve(canvas: &mut Canvas, center: Vec2, radius: f64, from: f64, to: f64, width: f64, color: [u8; 3]) {
    let d = density();
    let (c, r, half) = (center * d, radius * d, width * d / 2.0);
    let ends = [from, to].map(|turn| c + Vec2::new(turn.cos(), turn.sin()) * r);
    let area = Rect::from_center(c, Vec2::splat((r + half + 1.0) * 2.0));
    let mut mask = Mask::new(area, draw::whole(&canvas.image));
    mask.each(area, |p| {
        let rel = p - c;
        let turn = (rel.y.atan2(rel.x) - from).rem_euclid(std::f64::consts::TAU);
        match turn <= to - from {
            true => half - (rel.length() - r).abs() + 0.5,
            false => (half - (p - ends[0]).length()).max(half - (p - ends[1]).length()) + 0.5,
        }
    });
    draw::over(&mut canvas.image, &mask, color, 1.0);
}

fn oval_ring(canvas: &mut Canvas, area: Rect, width: f64, color: [u8; 3], alpha: f32) {
    let area = dense_rect(area);
    let mut mask = Mask::new(Rect::new(area.origin - Vec2::splat(4.0), area.size + Vec2::splat(8.0)), draw::whole(&canvas.image));
    mask.oval(area, width * density() / 2.0);
    draw::over(&mut canvas.image, &mask, color, alpha);
}

impl Look {
    pub fn themed(p: &drifter_core::theme::Palette, accent: [u8; 3]) -> Look {
        let dark = p.ink[0] > 128;
        Look {
            bar: p.bar,
            back: if dark { lift(p.bar, -0.15) } else { p.well },
            card: p.card,
            raised: p.raised,
            hover: p.hover,
            rule: p.rule,
            ink: p.ink,
            dim: p.dim,
            accent,
            on_accent: [255, 255, 255],
            drop: f64::from(p.drop) / 255.0,
            veil: f64::from(p.veil) / 255.0,
            dark,
        }
    }

    pub fn least(&self) -> Vec2 {
        Vec2::new(860.0, 420.0)
    }

    pub fn canvas_area(&self, size: Vec2) -> Rect {
        rect(0.0, BAR_H, size.x, size.y - BAR_H)
    }

    pub fn buttons(&self, size: Vec2, fonts: &Fonts) -> Vec<(Hit, Rect)> {
        let y = (BAR_H - BUTTON) / 2.0;
        let mut out = vec![(Hit::Tool(Tool::Crop), rect(12.0, y, TOOL_W + 2.0, BUTTON))];
        let mut x = 12.0 + TOOL_W + 2.0 + 10.0;
        for tool in &Tool::ALL[1..] {
            out.push((Hit::Tool(*tool), rect(x, y, TOOL_W, BUTTON)));
            x += TOOL_W;
        }
        x += 14.0;
        for index in 0..INKS.len() {
            out.push((Hit::Ink(index), rect(x, y, INK_W, BUTTON)));
            x += INK_W;
        }
        x += 12.0;
        for weight in Weight::ALL {
            out.push((Hit::Weight(weight), rect(x, y, WEIGHT_W, BUTTON)));
            x += WEIGHT_W;
        }
        x += 14.0;
        out.push((Hit::Undo, rect(x, y, TOOL_W, BUTTON)));
        out.push((Hit::Redo, rect(x + TOOL_W + 4.0, y, TOOL_W, BUTTON)));
        let done_w = (fonts.body.width_of(t("Done")) + 30.0).max(68.0).ceil();
        let right = size.x - 12.0;
        out.push((Hit::Done, rect(right - done_w, y, done_w, BUTTON)));
        let trash = right - done_w - 10.0 - TOOL_W;
        out.push((Hit::Trash, rect(trash, y, TOOL_W, BUTTON)));
        out.push((Hit::Copy, rect(trash - 4.0 - TOOL_W, y, TOOL_W, BUTTON)));
        out
    }

    pub fn hit(&self, size: Vec2, fonts: &Fonts, at: Vec2) -> Option<Hit> {
        self.buttons(size, fonts).into_iter().find(|(_, area)| area.contains(at)).map(|(hit, _)| hit)
    }

    pub fn tip_words(&self, hit: Hit, text: bool) -> Option<(String, String)> {
        let keys = drifter_core::explorer::look::key_words;
        Some(match hit {
            Hit::Tool(tool) => (t(tool.name()).to_string(), tool.key().to_ascii_uppercase().to_string()),
            Hit::Ink(index) => (t(INKS.get(index)?.1).to_string(), String::new()),
            Hit::Weight(weight) => (t(weight.name(text)).to_string(), String::new()),
            Hit::Undo => (t("Undo").to_string(), keys("Ctrl+Z")),
            Hit::Redo => (t("Redo").to_string(), keys("Ctrl+Shift+Z")),
            Hit::Copy => (t("Copy to Clipboard").to_string(), keys("Ctrl+C")),
            Hit::Trash => (t("Move to Trash").to_string(), String::new()),
            Hit::Done => (t("Save and Close").to_string(), keys("Ctrl+S")),
        })
    }

    pub fn tip(&self, size: Vec2, fonts: &Fonts, hit: Hit, text: bool) -> Option<(Rect, String, String)> {
        let (words, keys) = self.tip_words(hit, text)?;
        let anchor = self.buttons(size, fonts).into_iter().find(|(each, _)| *each == hit)?.1;
        let keys_w = if keys.is_empty() { 0.0 } else { fonts.small.width_of(&keys) + 14.0 };
        let width = (fonts.small.width_of(&words) + keys_w + 18.0).ceil();
        let x = (anchor.center().x - width / 2.0).round().clamp(6.0, (size.x - width - 6.0).max(6.0));
        Some((rect(x, anchor.max().y + 6.0, width, 24.0), words, keys))
    }

    pub fn paint_tip(&self, canvas: &mut Canvas, size: Vec2, fonts: &Fonts, hit: Hit, text: bool) {
        let Some((card, words, keys)) = self.tip(size, fonts, hit, text) else {
            return;
        };
        canvas.shadow(card, 6.0, 8.0, self.drop * 0.8);
        canvas.round_rect(card, 6.0, if self.dark { self.raised } else { [255, 255, 255] }, 1.0);
        canvas.ring(card, 6.0, 1.0, self.rule, 1.0);
        let middle = card.center().y;
        let piece = fonts.small.paint(&words, self.ink);
        canvas.stamp(&piece, Vec2::new(card.origin.x + 9.0, middle - logical(&piece).y / 2.0).round(), 1.0);
        if !keys.is_empty() {
            let piece = fonts.small.paint(&keys, self.dim);
            let shown = logical(&piece);
            canvas.stamp(&piece, Vec2::new(card.max().x - 9.0 - shown.x, middle - shown.y / 2.0).round(), 1.0);
        }
    }

    fn glossy(&self, canvas: &mut Canvas, area: Rect, radius: f64, hot: bool) {
        let base = if hot { lift(self.accent, 0.08) } else { self.accent };
        canvas.shadow(rect(area.origin.x, area.origin.y + 1.0, area.size.x, area.size.y), radius, 3.0, if self.dark { 0.35 } else { 0.18 });
        canvas.shade(area, radius, lift(base, 0.14), lift(base, -0.10), 1.0);
        canvas.ring(area, radius, 1.0, [0, 0, 0], 0.2);
        let y = area.origin.y + 1.0;
        canvas.line(Vec2::new(area.origin.x + radius, y), Vec2::new(area.max().x - radius, y), 1.0, [255, 255, 255], 0.22);
    }

    fn raised(&self, canvas: &mut Canvas, area: Rect, radius: f64, hot: bool) {
        canvas.shadow(rect(area.origin.x, area.origin.y + 1.0, area.size.x, area.size.y), radius, 3.0, if self.dark { 0.35 } else { 0.16 });
        let base = if hot { self.hover } else { self.raised };
        let (top, bottom) = match self.dark {
            true => (lift(base, 0.16), lift(base, -0.06)),
            false => (lift(base, 0.7), lift(base, -0.02)),
        };
        canvas.shade(area, radius, top, bottom, 1.0);
        canvas.ring(area, radius, 1.0, [0, 0, 0], if self.dark { 0.5 } else { 0.16 });
        let y = area.origin.y + 1.5;
        canvas.line(Vec2::new(area.origin.x + radius, y), Vec2::new(area.max().x - radius, y), 1.0, [255, 255, 255], if self.dark { 0.16 } else { 0.9 });
    }

    fn pressed(&self, canvas: &mut Canvas, area: Rect, radius: f64) {
        canvas.shade(area, radius, lift(self.accent, -0.24), lift(self.accent, 0.06), 1.0);
        canvas.ring(area, radius, 1.0, [0, 0, 0], 0.3);
        let y = area.origin.y + 1.5;
        canvas.line(Vec2::new(area.origin.x + radius, y), Vec2::new(area.max().x - radius, y), 1.0, [0, 0, 0], 0.25);
    }

    fn group(&self, canvas: &mut Canvas, areas: &[Rect], on: Option<usize>, _: Option<usize>) {
        let (Some(first), Some(last)) = (areas.first(), areas.last()) else {
            return;
        };
        self.raised(canvas, rect(first.origin.x, first.origin.y, last.max().x - first.origin.x, first.size.y), 7.0, false);
        for (at, area) in areas.iter().enumerate() {
            if at > 0 && on != Some(at) && on != Some(at - 1) {
                canvas.line(Vec2::new(area.origin.x, area.origin.y + 7.0), Vec2::new(area.origin.x, area.max().y - 7.0), 1.0, self.rule, 1.0);
            }
            let face = rect(area.origin.x + 1.0, area.origin.y + 1.0, area.size.x - 2.0, area.size.y - 2.0);
            if on == Some(at) {
                self.pressed(canvas, face, 6.0);
            }
        }
    }

    pub fn lit_target(&self, size: Vec2, fonts: &Fonts, bar: &Bar) -> Option<Rect> {
        let hit = bar.hover?;
        let wanted = match hit {
            Hit::Tool(tool) => tool != bar.tool,
            Hit::Weight(weight) => weight != bar.weight,
            Hit::Undo => bar.undo,
            Hit::Redo => bar.redo,
            Hit::Copy | Hit::Trash => true,
            _ => false,
        };
        if !wanted {
            return None;
        }
        let area = self.buttons(size, fonts).into_iter().find(|(each, _)| *each == hit)?.1;
        Some(match hit {
            Hit::Tool(Tool::Crop) => area,
            Hit::Tool(_) | Hit::Weight(_) => rect(area.origin.x + 1.0, area.origin.y + 1.0, area.size.x - 2.0, area.size.y - 2.0),
            _ => area,
        })
    }

    pub fn paint_bar(&self, canvas: &mut Canvas, size: Vec2, fonts: &Fonts, bar: &Bar) {
        let strip = rect(0.0, 0.0, size.x, BAR_H);
        canvas.shade(strip, 0.0, lift(self.bar, if self.dark { 0.05 } else { 0.5 }), lift(self.bar, -0.03), 1.0);
        canvas.round_rect(rect(0.0, BAR_H - 1.0, size.x, 1.0), 0.0, self.rule, 1.0);
        let buttons = self.buttons(size, fonts);
        let find = |hit: Hit| buttons.iter().find(|(each, _)| *each == hit).map(|(_, area)| *area).unwrap_or(strip);
        let crop = find(Hit::Tool(Tool::Crop));
        match bar.tool == Tool::Crop {
            true => self.pressed(canvas, crop, 7.0),
            false => self.raised(canvas, crop, 7.0, false),
        }
        self.tool_glyph(canvas, Tool::Crop, crop.center(), if bar.tool == Tool::Crop { self.on_accent } else { self.ink }, fonts);
        let tools = &Tool::ALL[1..];
        let areas: Vec<Rect> = tools.iter().map(|tool| find(Hit::Tool(*tool))).collect();
        let on = tools.iter().position(|tool| *tool == bar.tool);
        let hot = tools.iter().position(|tool| bar.hover == Some(Hit::Tool(*tool)));
        self.group(canvas, &areas, on, hot);
        for (at, (tool, area)) in tools.iter().zip(&areas).enumerate() {
            let ink = if on == Some(at) { self.on_accent } else { self.ink };
            self.tool_glyph(canvas, *tool, area.center(), ink, fonts);
        }
        for (index, (ink, _)) in INKS.iter().enumerate() {
            let c = find(Hit::Ink(index)).center();
            if bar.ink == *ink {
                canvas.ring(Rect::from_center(c, Vec2::splat(22.0)), 11.0, 2.0, self.ink, 1.0);
            } else if bar.hover == Some(Hit::Ink(index)) {
                canvas.ring(Rect::from_center(c, Vec2::splat(22.0)), 11.0, 1.5, self.dim, 0.7);
            }
            canvas.circle(c + Vec2::new(0.0, 0.5), 8.0, [0, 0, 0], if self.dark { 0.35 } else { 0.12 });
            canvas.circle(c, 8.0, *ink, 1.0);
            match self.dark {
                true => canvas.ring(Rect::from_center(c, Vec2::splat(16.0)), 8.0, 1.0, [255, 255, 255], 0.3),
                false => canvas.ring(Rect::from_center(c, Vec2::splat(16.0)), 8.0, 1.0, [0, 0, 0], 0.18),
            }
        }
        let areas: Vec<Rect> = Weight::ALL.iter().map(|weight| find(Hit::Weight(*weight))).collect();
        let on = Weight::ALL.iter().position(|weight| *weight == bar.weight);
        let hot = Weight::ALL.iter().position(|weight| bar.hover == Some(Hit::Weight(*weight)));
        self.group(canvas, &areas, on, hot);
        for (at, area) in areas.iter().enumerate() {
            let c = area.center();
            let lit = on == Some(at);
            match bar.text {
                true => {
                    let font = [fonts.small, fonts.body, fonts.title][at];
                    let piece = font.paint_bold("A", if lit { self.on_accent } else { self.ink });
                    canvas.stamp(&piece, (c - logical(&piece) / 2.0).round(), 1.0);
                }
                false => {
                    let thick = [1.5, 3.0, 5.5][at];
                    let (a, b) = (c - Vec2::new(7.0, 0.0), c + Vec2::new(7.0, 0.0));
                    canvas.line(a, b, thick + 1.5, if lit { lift(self.accent, -0.4) } else { self.rule }, 0.9);
                    canvas.line(a, b, thick, bar.ink, 1.0);
                }
            }
        }
        for (hit, able, way) in [(Hit::Undo, bar.undo, -1.0), (Hit::Redo, bar.redo, 1.0)] {
            let area = find(hit);
            self.raised(canvas, area, 7.0, false);
            let ink = if able { self.ink } else { mix(self.bar, self.dim, 0.5) };
            self.turn_glyph(canvas, area.center(), way, ink);
        }
        let copy = find(Hit::Copy);
        self.raised(canvas, copy, 7.0, false);
        self.copy_glyph(canvas, copy.center(), self.ink);
        let trash = find(Hit::Trash);
        self.raised(canvas, trash, 7.0, false);
        self.trash_glyph(canvas, trash.center(), self.ink);
        let done = find(Hit::Done);
        self.glossy(canvas, done, 7.0, bar.hover == Some(Hit::Done));
        let piece = fonts.body.paint_bold(t("Done"), self.on_accent);
        canvas.stamp(&piece, (done.center() - logical(&piece) / 2.0).round(), 1.0);
    }

    fn tool_glyph(&self, canvas: &mut Canvas, tool: Tool, c: Vec2, ink: [u8; 3], fonts: &Fonts) {
        let v = |x: f64, y: f64| c + Vec2::new(x, y);
        match tool {
            Tool::Crop => {
                canvas.line(v(-4.0, -9.0), v(-4.0, 4.0), 1.8, ink, 1.0);
                canvas.line(v(-4.0, 4.0), v(9.0, 4.0), 1.8, ink, 1.0);
                canvas.line(v(-9.0, -4.0), v(4.0, -4.0), 1.8, ink, 1.0);
                canvas.line(v(4.0, -4.0), v(4.0, 9.0), 1.8, ink, 1.0);
            }
            Tool::Pen => {
                canvas.line(v(-3.0, 3.0), v(6.0, -6.0), 4.0, ink, 1.0);
                canvas.triangle([v(-2.6, 5.4), v(-5.4, 2.6), v(-7.6, 7.6)], ink, 1.0);
            }
            Tool::Highlight => {
                canvas.line(v(-9.0, 9.0), v(9.0, 9.0), 2.4, ink, 0.45);
                canvas.line(v(-1.5, 1.5), v(5.5, -5.5), 6.0, ink, 1.0);
                canvas.triangle([v(-0.4, 4.6), v(-4.6, 0.4), v(-6.0, 6.0)], ink, 1.0);
            }
            Tool::Arrow => {
                canvas.line(v(-7.0, 7.0), v(3.0, -3.0), 2.0, ink, 1.0);
                canvas.triangle([v(8.0, -8.0), v(-0.5, -5.5), v(5.5, 0.5)], ink, 1.0);
            }
            Tool::Box => canvas.ring(Rect::from_center(c, Vec2::new(17.0, 13.0)), 2.0, 1.8, ink, 1.0),
            Tool::Oval => oval_ring(canvas, Rect::from_center(c, Vec2::new(18.0, 13.0)), 1.8, ink, 1.0),
            Tool::Text => {
                let piece = fonts.title.paint_bold("A", ink);
                canvas.stamp(&piece, (c - logical(&piece) / 2.0).round(), 1.0);
            }
            Tool::Loupe => {
                canvas.ring(Rect::from_center(v(-2.0, -2.0), Vec2::splat(13.0)), 6.5, 1.8, ink, 1.0);
                canvas.line(v(2.8, 2.8), v(8.0, 8.0), 2.8, ink, 1.0);
                canvas.line(v(-4.6, -2.0), v(0.6, -2.0), 1.4, ink, 1.0);
                canvas.line(v(-2.0, -4.6), v(-2.0, 0.6), 1.4, ink, 1.0);
            }
            Tool::Redact => {
                for row in 0..3 {
                    for column in 0..3 {
                        let alpha = if (row + column) % 2 == 0 { 1.0 } else { 0.4 };
                        let at = v(-7.5 + f64::from(column) * 5.2, -7.5 + f64::from(row) * 5.2);
                        canvas.round_rect(Rect::new(at, Vec2::splat(4.6)), 1.0, ink, alpha);
                    }
                }
            }
        }
    }

    fn turn_glyph(&self, canvas: &mut Canvas, c: Vec2, way: f64, ink: [u8; 3]) {
        let pi = std::f64::consts::PI;
        let center = c + Vec2::new(0.0, 1.5);
        match way < 0.0 {
            true => curve(canvas, center, 6.0, pi, 2.0 * pi + pi / 5.0, 1.8, ink),
            false => curve(canvas, center, 6.0, 4.0 * pi / 5.0, 2.0 * pi, 1.8, ink),
        }
        let end = center + Vec2::new(6.0 * way, 0.0);
        canvas.triangle([end + Vec2::new(0.0, 4.6), end + Vec2::new(-3.8, -0.6), end + Vec2::new(3.8, -0.6)], ink, 1.0);
    }

    fn copy_glyph(&self, canvas: &mut Canvas, c: Vec2, ink: [u8; 3]) {
        let back = Rect::from_center(c + Vec2::new(-2.0, -2.0), Vec2::new(10.0, 12.0));
        let front = Rect::from_center(c + Vec2::new(2.0, 2.0), Vec2::new(10.0, 12.0));
        for clip in [rect(back.origin.x - 2.0, back.origin.y - 2.0, back.size.x + 4.0, front.origin.y - back.origin.y + 1.0), rect(back.origin.x - 2.0, back.origin.y - 2.0, front.origin.x - back.origin.x + 1.0, back.size.y + 4.0)] {
            canvas.clip_to(Some(clip));
            canvas.ring(back, 2.5, 1.5, ink, 0.8);
        }
        canvas.clip_to(None);
        canvas.ring(front, 2.5, 1.6, ink, 1.0);
    }

    fn trash_glyph(&self, canvas: &mut Canvas, c: Vec2, ink: [u8; 3]) {
        let v = |x: f64, y: f64| c + Vec2::new(x, y);
        canvas.line(v(-6.5, -5.0), v(6.5, -5.0), 1.7, ink, 1.0);
        canvas.ring(Rect::new(v(-2.6, -8.0), Vec2::new(5.2, 3.6)), 1.4, 1.4, ink, 1.0);
        canvas.line(v(-4.8, -3.0), v(-3.8, 7.5), 1.6, ink, 1.0);
        canvas.line(v(4.8, -3.0), v(3.8, 7.5), 1.6, ink, 1.0);
        canvas.line(v(-3.8, 7.5), v(3.8, 7.5), 1.6, ink, 1.0);
        canvas.line(v(-1.4, -1.0), v(-1.2, 5.0), 1.2, ink, 1.0);
        canvas.line(v(1.4, -1.0), v(1.2, 5.0), 1.2, ink, 1.0);
    }

    pub fn paint_crop(&self, canvas: &mut Canvas, map: Map, whole: Rect, crop: Rect, held: bool, fonts: &Fonts) {
        let (image, crop) = (map.rect(whole), map.rect(crop));
        let veil = [
            rect(image.origin.x, image.origin.y, image.size.x, crop.origin.y - image.origin.y),
            rect(image.origin.x, crop.max().y, image.size.x, image.max().y - crop.max().y),
            rect(image.origin.x, crop.origin.y, crop.origin.x - image.origin.x, crop.size.y),
            rect(crop.max().x, crop.origin.y, image.max().x - crop.max().x, crop.size.y),
        ];
        for part in veil {
            canvas.round_rect(part, 0.0, [0, 0, 0], 0.55);
        }
        if held {
            for third in [1.0, 2.0] {
                let x = crop.origin.x + crop.size.x * third / 3.0;
                let y = crop.origin.y + crop.size.y * third / 3.0;
                canvas.line(Vec2::new(x, crop.origin.y), Vec2::new(x, crop.max().y), 1.0, [255, 255, 255], 0.55);
                canvas.line(Vec2::new(crop.origin.x, y), Vec2::new(crop.max().x, y), 1.0, [255, 255, 255], 0.55);
            }
        }
        canvas.ring(Rect::new(crop.origin - Vec2::splat(1.0), crop.size + Vec2::splat(2.0)), 0.0, 1.0, [0, 0, 0], 0.35);
        canvas.ring(crop, 0.0, 1.0, [255, 255, 255], 0.95);
        let arm = (crop.size.x.min(crop.size.y) / 3.0).min(20.0);
        let (low, high) = (crop.origin - Vec2::splat(2.0), crop.max() + Vec2::splat(2.0));
        for (corner, sx, sy) in [(low, 1.0, 1.0), (Vec2::new(high.x, low.y), -1.0, 1.0), (high, -1.0, -1.0), (Vec2::new(low.x, high.y), 1.0, -1.0)] {
            for (dx, dy) in [(sx * arm, 0.0), (0.0, sy * arm)] {
                canvas.line(corner + Vec2::new(0.0, 0.8), corner + Vec2::new(dx, dy + 0.8), 4.0, [0, 0, 0], 0.25);
                canvas.line(corner, corner + Vec2::new(dx, dy), 4.0, [255, 255, 255], 1.0);
            }
        }
        let c = crop.center();
        for (a, b) in [
            (Vec2::new(c.x - arm / 2.0, low.y), Vec2::new(c.x + arm / 2.0, low.y)),
            (Vec2::new(c.x - arm / 2.0, high.y), Vec2::new(c.x + arm / 2.0, high.y)),
            (Vec2::new(low.x, c.y - arm / 2.0), Vec2::new(low.x, c.y + arm / 2.0)),
            (Vec2::new(high.x, c.y - arm / 2.0), Vec2::new(high.x, c.y + arm / 2.0)),
        ] {
            canvas.line(a, b, 4.0, [255, 255, 255], 1.0);
        }
        let words = format!("{} × {}", (crop.size.x / map.scale).round(), (crop.size.y / map.scale).round());
        let piece = fonts.small.paint(&words, [255, 255, 255]);
        let shown = logical(&piece);
        let pill = Vec2::new(shown.x + 16.0, 22.0);
        let below = crop.max().y + 10.0;
        let y = match below + pill.y < image.max().y + PAD - 4.0 {
            true => below,
            false => crop.max().y - pill.y - 10.0,
        };
        let card = rect((c.x - pill.x / 2.0).round(), y.round(), pill.x, pill.y);
        canvas.round_rect(card, 11.0, [0, 0, 0], 0.62);
        canvas.stamp(&piece, (card.center() - shown / 2.0).round(), 1.0);
    }

    pub fn paint_picked(&self, canvas: &mut Canvas, map: Map, shape: &Shape, unit: f64) {
        let handles = shape.handles();
        if handles.is_empty() || matches!(shape.kind, Kind::Redact(..)) {
            let bounds = map.rect(shape.bounds(unit));
            let area = Rect::new(bounds.origin - Vec2::splat(4.0), bounds.size + Vec2::splat(8.0));
            canvas.ring(area, 4.0, 1.0, self.accent, 0.9);
        }
        for (handle, spot) in handles {
            let at = map.to(spot);
            canvas.shadow(Rect::from_center(at + Vec2::new(0.0, 1.0), Vec2::splat(10.0)), 5.0, 3.0, 0.35);
            let fill = if handle == Handle::Zoom { ZOOM } else { [255, 255, 255] };
            canvas.circle(at, 5.5, fill, 1.0);
            canvas.ring(Rect::from_center(at, Vec2::splat(11.0)), 5.5, 1.5, self.accent, 1.0);
        }
    }

    pub fn paint_editing(&self, canvas: &mut Canvas, map: Map, shape: &Shape, unit: f64, caret: Option<(Vec2, Vec2)>) {
        let bounds = map.rect(shape.bounds(unit));
        let area = Rect::new(bounds.origin - Vec2::new(6.0, 3.0), Vec2::new(bounds.size.x.max(8.0), bounds.size.y) + Vec2::new(12.0, 6.0));
        canvas.ring(area, 4.0, 1.0, self.accent, 0.9);
        if let Some((top, bottom)) = caret {
            canvas.line(top, bottom, 2.0, self.accent, 1.0);
        }
    }

    pub fn prompt_card(&self, size: Vec2) -> Rect {
        Rect::from_center(size / 2.0 + Vec2::new(0.0, BAR_H / 2.0), Vec2::new(400.0, 136.0)).round_out()
    }

    pub fn prompt_buttons(&self, size: Vec2, fonts: &Fonts) -> Vec<(Answer, Rect)> {
        let card = self.prompt_card(size);
        let y = card.max().y - 20.0 - 28.0;
        let width = |words: &str| (fonts.body.width_of(t(words)) + 28.0).max(84.0).ceil();
        let save = width("Save");
        let cancel = width("Cancel");
        let discard = width("Don't Save");
        let right = card.max().x - 20.0;
        vec![
            (Answer::Save, rect(right - save, y, save, 28.0)),
            (Answer::Cancel, rect(right - save - 10.0 - cancel, y, cancel, 28.0)),
            (Answer::Discard, rect(card.origin.x + 20.0, y, discard, 28.0)),
        ]
    }

    pub fn paint_prompt(&self, canvas: &mut Canvas, size: Vec2, fonts: &Fonts, hover: Option<Answer>) {
        canvas.round_rect(rect(0.0, 0.0, size.x, size.y), 0.0, [0, 0, 0], self.veil * 0.45);
        let card = self.prompt_card(size);
        canvas.shadow(card, 12.0, 24.0, self.drop * 1.4);
        canvas.round_rect(card, 12.0, self.card, 1.0);
        canvas.ring(card, 12.0, 1.0, self.rule, 1.0);
        let title = fonts.body.paint_bold(t("Save changes?"), self.ink);
        canvas.stamp(&title, Vec2::new(card.origin.x + 20.0, card.origin.y + 22.0), 1.0);
        let words = fonts.small.paint(t("Your markup is lost if you don't save it."), self.dim);
        canvas.stamp(&words, Vec2::new(card.origin.x + 20.0, card.origin.y + 22.0 + logical(&title).y + 6.0), 1.0);
        for (answer, area) in self.prompt_buttons(size, fonts) {
            let hot = hover == Some(answer);
            let (words, ink) = match answer {
                Answer::Save => {
                    self.glossy(canvas, area, 7.0, hot);
                    ("Save", self.on_accent)
                }
                Answer::Cancel => {
                    self.raised(canvas, area, 7.0, hot);
                    ("Cancel", self.ink)
                }
                Answer::Discard => {
                    self.raised(canvas, area, 7.0, hot);
                    ("Don't Save", self.ink)
                }
            };
            let piece = match answer {
                Answer::Save => fonts.body.paint_bold(t(words), ink),
                _ => fonts.body.paint(t(words), ink),
            };
            canvas.stamp(&piece, (area.center() - logical(&piece) / 2.0).round(), 1.0);
        }
    }

    pub fn paint_toast(&self, canvas: &mut Canvas, size: Vec2, fonts: &Fonts, words: &str, alpha: f64) {
        let piece = fonts.body.paint(words, self.ink);
        let shown = logical(&piece);
        let card = rect(((size.x - shown.x - 32.0) / 2.0).round(), (size.y - 56.0).round(), shown.x + 32.0, 32.0);
        canvas.shadow(card, 16.0, 12.0, self.drop * alpha);
        canvas.round_rect(card, 16.0, if self.dark { self.raised } else { [255, 255, 255] }, alpha);
        canvas.ring(card, 16.0, 1.0, self.rule, alpha);
        canvas.stamp(&piece, (card.center() - shown / 2.0).round(), alpha);
    }

    pub fn checker_cells(&self) -> ([u8; 3], [u8; 3]) {
        match self.dark {
            true => ([70, 72, 80], [52, 54, 62]),
            false => ([255, 255, 255], [222, 224, 230]),
        }
    }
}

pub fn view_map(look: &Look, size: Vec2, shown: Rect, unit: f64) -> Map {
    markup::fit(shown, look.canvas_area(size), PAD, 1.0 / unit.max(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_toolbar_holds_every_button_without_overlap_at_the_least_size() {
        let Some(path) = drifter_core::text::font_path(None) else {
            return;
        };
        let font = Font::load(&path, 13.0).unwrap();
        let fonts = Fonts { body: &font, small: &font, title: &font };
        let look = Look::themed(&drifter_core::theme::DARK, [64, 128, 238]);
        let buttons = look.buttons(look.least(), &fonts);
        assert_eq!(buttons.len(), 1 + 8 + INKS.len() + 3 + 2 + 3);
        for (at, (_, a)) in buttons.iter().enumerate() {
            for (_, b) in &buttons[at + 1..] {
                let apart = a.max().x <= b.origin.x + 0.01 || b.max().x <= a.origin.x + 0.01;
                assert!(apart, "{a:?} overlaps {b:?}");
            }
        }
        assert_eq!(look.hit(look.least(), &fonts, buttons[3].1.center()), Some(buttons[3].0));
    }

    #[test]
    fn a_tooltip_names_every_button() {
        let look = Look::themed(&drifter_core::theme::LIGHT, [64, 128, 238]);
        for tool in Tool::ALL {
            assert!(look.tip_words(Hit::Tool(tool), false).is_some());
        }
        assert_eq!(look.tip_words(Hit::Weight(Weight::Thick), true).map(|(words, _)| words), Some("Large".to_string()));
    }
}
