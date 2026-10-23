use std::sync::Arc;

use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

pub const RADIUS: f64 = 14.0;
pub const PAD: f64 = 6.0;
pub const BACK_ALPHA: f64 = 0.96;
pub const LIT_RADIUS: f64 = 9.0;
pub const INSET: f64 = 12.0;
pub const DISC: f64 = 30.0;
pub const GLYPH: f64 = 16.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Action,
    Header,
    Section,
    Rich,
    Info,
    Message,
    Rule,
}

impl Kind {
    pub fn height(self) -> f64 {
        match self {
            Kind::Action => 28.0,
            Kind::Header => 40.0,
            Kind::Section => 26.0,
            Kind::Rich => 44.0,
            Kind::Info => 24.0,
            Kind::Message => 44.0,
            Kind::Rule => 11.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Glyph {
    Lock,
    Sleep,
    Restart,
    Power,
    LogOut,
    Hide,
    HideOthers,
    ShowAll,
    Quit,
    Settings,
    About,
    More,
    Wifi,
    Bluetooth,
    Plus,
    Back,
    AirDrop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disc {
    Net(crate::net::Glyph),
    Device(crate::blue::Kind),
}

#[derive(Clone, Default)]
pub enum Lead {
    #[default]
    None,
    Glyph(Glyph),
    Icon(Arc<Image>),
    Disc(Disc, bool),
}

impl PartialEq for Lead {
    fn eq(&self, other: &Lead) -> bool {
        match (self, other) {
            (Lead::None, Lead::None) => true,
            (Lead::Glyph(a), Lead::Glyph(b)) => a == b,
            (Lead::Icon(a), Lead::Icon(b)) => Arc::ptr_eq(a, b),
            (Lead::Disc(a, x), Lead::Disc(b, y)) => a == b && x == y,
            _ => false,
        }
    }
}

impl Eq for Lead {}

impl std::fmt::Debug for Lead {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Lead::None => write!(out, "None"),
            Lead::Glyph(glyph) => write!(out, "Glyph({glyph:?})"),
            Lead::Icon(_) => write!(out, "Icon"),
            Lead::Disc(disc, lit) => write!(out, "Disc({disc:?}, {lit})"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Trail {
    #[default]
    None,
    Shortcut(String),
    Check(bool),
    Radio(bool),
    Chevron,
    Bars(u8),
    Battery(u8),
    Lock,
    LockBars(u8),
    Switch(bool),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub back: [u8; 3],
    pub back_alpha: f64,
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub off: [u8; 3],
    pub accent: [u8; 3],
    pub rule: [u8; 3],
    pub shade: [u8; 3],
}

impl Style {
    pub fn themed(p: &crate::theme::Palette, accent: [u8; 3]) -> Style {
        Style {
            back: p.back,
            back_alpha: BACK_ALPHA,
            ink: p.ink,
            dim: p.dim,
            off: p.off,
            accent,
            rule: p.ink,
            shade: p.shade,
        }
    }
}

pub struct Row<'a> {
    pub kind: Kind,
    pub label: &'a str,
    pub note: &'a str,
    pub value: &'a str,
    pub lead: &'a Lead,
    pub trail: &'a Trail,
    pub enabled: bool,
    pub lead_room: bool,
}

pub struct Fonts<'a> {
    pub body: &'a Font,
    pub small: &'a Font,
}

const LOCK_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M6.5 9V6.8a3.5 3.5 0 0 1 7 0V9" fill="none" stroke="#000" stroke-width="1.8"/><rect x="4.5" y="8.6" width="11" height="8.4" rx="2.2"/></svg>"##;
const SLEEP_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M15.8 12.4A6.6 6.6 0 0 1 7.6 4.2a6.6 6.6 0 1 0 8.2 8.2z" fill="none" stroke="#000" stroke-width="1.8" stroke-linejoin="round"/></svg>"##;
const RESTART_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M15.6 10.6A5.7 5.7 0 1 1 13.9 5.9" fill="none" stroke="#000" stroke-width="1.8" stroke-linecap="round"/><path d="M15.4 3.2v3.6h-3.6" fill="none" stroke="#000" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const POWER_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M6.4 5.6a6 6 0 1 0 7.2 0" fill="none" stroke="#000" stroke-width="1.8" stroke-linecap="round"/><path d="M10 2.8v6.4" stroke="#000" stroke-width="1.8" stroke-linecap="round"/></svg>"##;
const LOGOUT_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M8.2 3.6H5.2a1.6 1.6 0 0 0-1.6 1.6v9.6a1.6 1.6 0 0 0 1.6 1.6h3" fill="none" stroke="#000" stroke-width="1.8" stroke-linecap="round"/><path d="M9 10h7.4M13.4 6.8l3.2 3.2-3.2 3.2" fill="none" stroke="#000" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const HIDE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M2.6 10s2.8-5 7.4-5 7.4 5 7.4 5-2.8 5-7.4 5-7.4-5-7.4-5z" fill="none" stroke="#000" stroke-width="1.7" stroke-linejoin="round"/><circle cx="10" cy="10" r="2.2"/><path d="M4 16L16 4" stroke="#000" stroke-width="1.8" stroke-linecap="round"/></svg>"##;
const HIDE_OTHERS_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><rect x="2.8" y="5.6" width="10" height="9" rx="1.8" fill="none" stroke="#000" stroke-width="1.6" stroke-dasharray="2 1.8"/><rect x="7.2" y="3" width="10" height="9" rx="1.8" fill="none" stroke="#000" stroke-width="1.8"/></svg>"##;
const SHOW_ALL_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><rect x="3" y="3" width="6" height="6" rx="1.4"/><rect x="11" y="3" width="6" height="6" rx="1.4"/><rect x="3" y="11" width="6" height="6" rx="1.4"/><rect x="11" y="11" width="6" height="6" rx="1.4"/></svg>"##;
const QUIT_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><circle cx="10" cy="10" r="7" fill="none" stroke="#000" stroke-width="1.8"/><path d="M7.4 7.4l5.2 5.2M12.6 7.4l-5.2 5.2" stroke="#000" stroke-width="1.8" stroke-linecap="round"/></svg>"##;
const SETTINGS_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M3.5 6h13M3.5 14h13" stroke="#000" stroke-width="1.7" stroke-linecap="round"/><circle cx="7.4" cy="6" r="2.4"/><circle cx="12.6" cy="14" r="2.4"/></svg>"##;
const ABOUT_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><circle cx="10" cy="10" r="7" fill="none" stroke="#000" stroke-width="1.8"/><path d="M10 9v4.6" stroke="#000" stroke-width="1.9" stroke-linecap="round"/><circle cx="10" cy="6.4" r="1.1"/></svg>"##;
const MORE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><circle cx="5" cy="10" r="1.5"/><circle cx="10" cy="10" r="1.5"/><circle cx="15" cy="10" r="1.5"/></svg>"##;
const BACK_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M12.4 4.4L6.8 10l5.6 5.6" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;
const AIRDROP_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M5.1 15.4A7.2 7.2 0 1 1 14.9 15.4" fill="none" stroke="#000" stroke-width="1.8" stroke-linecap="round"/><path d="M7.5 12.8A3.8 3.8 0 1 1 12.5 12.8" fill="none" stroke="#000" stroke-width="1.8" stroke-linecap="round"/><circle cx="10" cy="10" r="1.6"/></svg>"##;
const PLUS_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M10 4v12M4 10h12" stroke="#000" stroke-width="1.9" stroke-linecap="round"/></svg>"##;

pub fn glyph_svg(glyph: Glyph) -> &'static str {
    match glyph {
        Glyph::Lock => LOCK_SVG,
        Glyph::Sleep => SLEEP_SVG,
        Glyph::Restart => RESTART_SVG,
        Glyph::Power => POWER_SVG,
        Glyph::LogOut => LOGOUT_SVG,
        Glyph::Hide => HIDE_SVG,
        Glyph::HideOthers => HIDE_OTHERS_SVG,
        Glyph::ShowAll => SHOW_ALL_SVG,
        Glyph::Quit => QUIT_SVG,
        Glyph::Settings => SETTINGS_SVG,
        Glyph::About => ABOUT_SVG,
        Glyph::More => MORE_SVG,
        Glyph::Wifi => "",
        Glyph::Bluetooth => crate::blue::rune(),
        Glyph::Plus => PLUS_SVG,
        Glyph::Back => BACK_SVG,
        Glyph::AirDrop => AIRDROP_SVG,
    }
}

pub fn glyph_image(glyph: Glyph, side: f64, ink: [u8; 3]) -> Image {
    let pixels = crate::paint::dense(side);
    match glyph {
        Glyph::Wifi => crate::net::glyph(crate::net::Glyph::Wifi, pixels, ink),
        other => crate::blue::svg_glyph(glyph_svg(other), pixels, ink),
    }
}

pub fn disc_image(disc: Disc, lit: bool, style: &Style) -> Image {
    let side = DISC;
    let mut canvas = Canvas::new(side as u32, side as u32);
    canvas.circle(Vec2::splat(side / 2.0), side / 2.0 - 0.5, if lit { style.accent } else { style.off }, 1.0);
    let ink = if lit { [255, 255, 255] } else { style.ink };
    let inner = 17.0;
    let mark = match disc {
        Disc::Net(glyph) => crate::net::glyph(glyph, crate::paint::dense(inner), ink),
        Disc::Device(kind) => crate::blue::glyph(kind, crate::paint::dense(inner), ink),
    };
    canvas.stamp(&mark, Vec2::splat((side - inner) / 2.0), 1.0);
    canvas.image
}

fn trail_width(trail: &Trail, small: &Font) -> f64 {
    match trail {
        Trail::None => 0.0,
        Trail::Shortcut(text) => small.width_of(text) + 4.0,
        Trail::Check(_) | Trail::Radio(_) | Trail::Chevron => 14.0,
        Trail::Bars(_) => 18.0,
        Trail::Battery(_) => 52.0,
        Trail::Lock => 12.0,
        Trail::LockBars(_) => 36.0,
        Trail::Switch(_) => 34.0,
    }
}

pub fn width_of(row: &Row, fonts: &Fonts) -> f64 {
    let lead = lead_width(row.lead, row.lead_room, row.kind);
    let trail = trail_width(row.trail, fonts.small);
    let text = match row.kind {
        Kind::Header => fonts.body.width_of(row.label) + 4.0,
        Kind::Section => fonts.small.width_of(row.label),
        Kind::Rich => fonts.body.width_of(row.label).max(fonts.small.width_of(row.note)),
        Kind::Info => fonts.small.width_of(row.label) + 16.0 + fonts.small.width_of(row.value),
        Kind::Message => fonts.small.width_of(row.label),
        Kind::Rule => 0.0,
        Kind::Action => fonts.body.width_of(row.label),
    };
    let gap = if trail > 0.0 { 18.0 } else { 0.0 };
    INSET + lead + text + gap + trail + INSET
}

fn lead_width(lead: &Lead, room: bool, kind: Kind) -> f64 {
    match (lead, kind) {
        (Lead::Disc(..), _) | (_, Kind::Rich) => DISC + 10.0,
        (Lead::None, Kind::Action) if !room => 0.0,
        (Lead::None, _) if kind != Kind::Action => 0.0,
        _ => GLYPH + 10.0,
    }
}

pub fn back(canvas: &mut Canvas, size: Vec2, radius: f64, style: &Style) {
    canvas.round_rect(Rect::new(Vec2::ZERO, size), radius, style.back, style.back_alpha);
}

pub fn highlight(canvas: &mut Canvas, rect: Rect, style: &Style, alpha: f64) {
    let inner = Rect::new(rect.origin + Vec2::new(PAD, 1.0), Vec2::new(rect.size.x - PAD * 2.0, rect.size.y - 2.0));
    canvas.round_rect(inner, LIT_RADIUS, style.ink, alpha);
}

pub fn lit_rect(rect: Rect) -> Rect {
    Rect::new(rect.origin + Vec2::new(PAD, 1.0), Vec2::new(rect.size.x - PAD * 2.0, rect.size.y - 2.0))
}

fn centred(canvas: &mut Canvas, image: &Image, x: f64, middle: f64, alpha: f64) {
    let at = Vec2::new(x, middle - crate::paint::logical(image).y / 2.0).round();
    canvas.stamp(image, at, alpha);
}

pub fn paint_row(canvas: &mut Canvas, rect: Rect, row: &Row, style: &Style, fonts: &Fonts) {
    let middle = rect.center().y;
    let left = rect.origin.x + INSET;
    let right = rect.max().x - INSET;
    let fade = if row.enabled { 1.0 } else { 0.45 };
    match row.kind {
        Kind::Rule => {
            let line = Rect::new(Vec2::new(rect.origin.x + INSET, middle.round()), Vec2::new(rect.size.x - INSET * 2.0, 1.0));
            canvas.round_rect(line, 0.0, style.rule, 0.1);
            return;
        }
        Kind::Section => {
            let text = fonts.small.paint_weight(row.label, style.dim, 600.0);
            canvas.stamp(&text, Vec2::new(left, rect.max().y - crate::paint::logical(&text).y - 5.0).round(), 1.0);
            return;
        }
        Kind::Message => {
            let text = fonts.small.paint(row.label, style.dim);
            let width = crate::paint::logical(&text).x;
            centred(canvas, &text, rect.origin.x + (rect.size.x - width) / 2.0, middle, 1.0);
            return;
        }
        Kind::Info => {
            let key = fonts.small.paint(row.label, style.dim);
            centred(canvas, &key, left, middle, 1.0);
            let value = fonts.small.paint(row.value, style.ink);
            centred(canvas, &value, right - crate::paint::logical(&value).x, middle, 1.0);
            return;
        }
        _ => {}
    }
    let mut x = left;
    match row.lead {
        Lead::Disc(disc, lit) => {
            let image = disc_image(*disc, *lit, style);
            centred(canvas, &image, x, middle, fade);
            x += DISC + 10.0;
        }
        Lead::Glyph(glyph) => {
            let image = glyph_image(*glyph, GLYPH, style.ink);
            centred(canvas, &image, x, middle, fade * 0.92);
            x += GLYPH + 10.0;
        }
        Lead::Icon(image) => {
            let side = if row.kind == Kind::Rich { DISC } else { GLYPH };
            let pixels = crate::paint::dense(side);
            let scaled = image.scaled(pixels, pixels);
            centred(canvas, &scaled, x, middle, fade);
            x += side + 10.0;
        }
        Lead::None => {
            if row.kind == Kind::Rich {
                x += DISC + 10.0;
            } else if row.lead_room && row.kind == Kind::Action {
                x += GLYPH + 10.0;
            }
        }
    }
    let trail_left = paint_trail(canvas, row.trail, right, middle, style, fonts, fade);
    let room = (trail_left - 12.0 - x).max(1.0);
    match row.kind {
        Kind::Header => {
            let text = fonts.body.paint_weight(&crate::control::fit(fonts.body, row.label, room), style.ink, 600.0);
            centred(canvas, &text, x, middle, 1.0);
        }
        Kind::Rich => {
            let name = fonts.body.paint_weight(&crate::control::fit(fonts.body, row.label, room), style.ink, 600.0);
            let note = fonts.small.paint(&crate::control::fit(fonts.small, row.note, room), style.dim);
            match row.note.is_empty() {
                true => centred(canvas, &name, x, middle, fade),
                false => {
                    canvas.stamp(&name, Vec2::new(x, middle - crate::paint::logical(&name).y).round(), fade);
                    canvas.stamp(&note, Vec2::new(x, middle + 1.0).round(), fade);
                }
            }
        }
        _ => {
            let text = fonts.body.paint(&crate::control::fit(fonts.body, row.label, room), style.ink);
            centred(canvas, &text, x, middle, fade);
        }
    }
}

fn paint_trail(canvas: &mut Canvas, trail: &Trail, right: f64, middle: f64, style: &Style, fonts: &Fonts, fade: f64) -> f64 {
    match trail {
        Trail::None => right,
        Trail::Shortcut(text) => {
            let image = fonts.small.paint(text, style.dim);
            let width = crate::paint::logical(&image).x;
            centred(canvas, &image, right - width, middle, fade);
            right - width
        }
        Trail::Check(on) => {
            if *on {
                let a = Vec2::new(right - 12.0, middle);
                let b = Vec2::new(right - 8.0, middle + 4.0);
                let c = Vec2::new(right, middle - 5.0);
                canvas.line(a, b, 1.9, style.accent, fade);
                canvas.line(b, c, 1.9, style.accent, fade);
            }
            right - 14.0
        }
        Trail::Radio(on) => {
            let centre = Vec2::new(right - 6.0, middle);
            canvas.ring(Rect::from_center(centre, Vec2::splat(12.0)), 6.0, 1.4, if *on { style.accent } else { style.dim }, fade);
            if *on {
                canvas.circle(centre, 3.0, style.accent, fade);
            }
            right - 14.0
        }
        Trail::Chevron => {
            let at = Vec2::new(right - 4.0, middle);
            canvas.line(at + Vec2::new(-4.0, -4.5), at, 1.7, style.dim, fade);
            canvas.line(at, at + Vec2::new(-4.0, 4.5), 1.7, style.dim, fade);
            right - 14.0
        }
        Trail::Bars(lit) => {
            bars(canvas, Rect::new(Vec2::new(right - 18.0, middle - 6.5), Vec2::new(18.0, 13.0)), *lit, style, fade);
            right - 18.0
        }
        Trail::Lock => {
            let image = crate::net::lock_glyph(crate::paint::dense(12.0), style.dim);
            centred(canvas, &image, right - 12.0, middle, fade * 0.9);
            right - 12.0
        }
        Trail::LockBars(lit) => {
            bars(canvas, Rect::new(Vec2::new(right - 18.0, middle - 6.5), Vec2::new(18.0, 13.0)), *lit, style, fade);
            let image = crate::net::lock_glyph(crate::paint::dense(12.0), style.dim);
            centred(canvas, &image, right - 18.0 - 6.0 - 12.0, middle, fade * 0.9);
            right - 36.0
        }
        Trail::Battery(level) => {
            let cell = Rect::new(Vec2::new(right - 22.0, middle - 5.5), Vec2::new(22.0, 11.0));
            crate::control::battery_glyph(canvas, cell, *level, style.dim, style.accent);
            let words = fonts.small.paint(&format!("{level}%"), style.dim);
            let width = crate::paint::logical(&words).x;
            centred(canvas, &words, cell.origin.x - 6.0 - width, middle, fade);
            cell.origin.x - 6.0 - width
        }
        Trail::Switch(on) => {
            let track = Rect::new(Vec2::new(right - 34.0, middle - 10.0), Vec2::new(34.0, 20.0));
            canvas.round_rect(track, 10.0, if *on { style.accent } else { style.off }, 1.0);
            let knob = Vec2::new(if *on { track.max().x - 10.0 } else { track.origin.x + 10.0 }, middle);
            canvas.circle(knob + Vec2::new(0.0, 0.6), 8.6, [0, 0, 0], 0.18);
            canvas.circle(knob, 8.0, [255, 255, 255], 1.0);
            track.origin.x
        }
    }
}

fn bars(canvas: &mut Canvas, area: Rect, lit: u8, style: &Style, fade: f64) {
    for step in 0..4u8 {
        let tall = 4.0 + f64::from(step) * 3.0;
        let bar = Rect::new(area.origin + Vec2::new(f64::from(step) * 5.0, area.size.y - tall), Vec2::new(3.0, tall));
        let shown = step < lit;
        canvas.round_rect(bar, 1.0, if shown { style.ink } else { style.off }, if shown { 0.9 * fade } else { fade });
    }
}

pub fn switch_rect(row: Rect) -> Rect {
    let right = row.max().x - INSET;
    Rect::new(Vec2::new(right - 34.0, row.center().y - 10.0), Vec2::new(34.0, 20.0))
}
