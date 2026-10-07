use std::path::Path;

use crate::config::{Modifier, Style};
use crate::desktop::Role;
use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::{Canvas, dense, logical};
use crate::text::Font;

pub const SIZE: Vec2 = Vec2::new(720.0, 500.0);
pub const LIVEPAPER: &str =
    "A livepaper is a wallpaper that moves gently and changes color with the time of day.";
pub const THUMB: Vec2 = Vec2::new(150.0, 94.0);
pub const FRAMES: usize = 40;
pub const CYCLE: f64 = 2.6;
pub const SHOWN_PAPERS: usize = 8;
pub const CHOICES: usize = 6;
const PAD: f64 = 36.0;
pub const RADIUS: f64 = 26.0;
const FOOT: f64 = 58.0;
const WHITE: [u8; 3] = [255, 255, 255];
const BLACK: [u8; 3] = [0, 0, 0];
const GREEN: [u8; 3] = [52, 199, 89];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Card {
    Hello,
    Look,
    Comfort,
    Display,
    Keys,
    Apps,
    Done,
}

impl Card {
    pub const ALL: [Card; 7] = [
        Card::Hello,
        Card::Look,
        Card::Comfort,
        Card::Display,
        Card::Keys,
        Card::Apps,
        Card::Done,
    ];

    pub fn index(self) -> usize {
        Card::ALL.iter().position(|card| *card == self).unwrap_or(0)
    }

    pub fn next(self) -> Option<Card> {
        Card::ALL.get(self.index() + 1).copied()
    }

    pub fn back(self) -> Option<Card> {
        self.index().checked_sub(1).map(|at| Card::ALL[at])
    }

    fn go(self) -> &'static str {
        match self {
            Card::Hello => "Begin",
            Card::Done => "Start",
            _ => "Continue",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Still,
    Calm,
    Full,
}

impl Motion {
    pub const ALL: [Motion; 3] = [Motion::Still, Motion::Calm, Motion::Full];

    pub fn of(reduce: bool, percent: u32) -> Motion {
        match (reduce, percent) {
            (true, _) | (_, 0) => Motion::Still,
            (_, 1..=60) => Motion::Calm,
            _ => Motion::Full,
        }
    }

    pub fn percent(self) -> u32 {
        match self {
            Motion::Still => 0,
            Motion::Calm => 40,
            Motion::Full => 100,
        }
    }

    pub fn reduce(self) -> bool {
        self == Motion::Still
    }

    fn label(self) -> &'static str {
        match self {
            Motion::Still => "Still",
            Motion::Calm => "Calm",
            Motion::Full => "Full",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Device {
    Touchpad,
    Mouse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lesson {
    Search,
    Glide,
    Overview,
    Zoom,
}

impl Lesson {
    pub const ALL: [Lesson; 4] = [Lesson::Search, Lesson::Glide, Lesson::Overview, Lesson::Zoom];

    pub fn index(self) -> usize {
        Lesson::ALL.iter().position(|each| *each == self).unwrap_or(0)
    }

    fn title(self) -> &'static str {
        match self {
            Lesson::Search => "Find anything",
            Lesson::Glide => "Glide around",
            Lesson::Overview => "See everything",
            Lesson::Zoom => "Zoom in and out",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Bit {
    Word(String),
    Key(String),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Next,
    Back,
    Dot(usize),
    Style(Style),
    Paper(usize),
    Motion(Motion),
    Text(bool),
    Scale(usize),
    Device(Device),
    Lesson(usize),
    Tile(usize),
    Choice(usize),
    Leave,
    Quiet,
}

#[derive(Clone)]
pub struct Paper {
    pub name: String,
    pub still: Option<Image>,
    pub chosen: bool,
}

#[derive(Clone)]
pub struct Tile {
    pub role: Role,
    pub name: Option<String>,
    pub icon: Option<Image>,
    pub busy: bool,
}

#[derive(Clone)]
pub struct Choice {
    pub name: String,
    pub icon: Option<Image>,
    pub chosen: bool,
    pub installed: bool,
    pub busy: bool,
}

#[derive(Clone)]
pub struct View {
    pub card: Card,
    pub greeting: String,
    pub style: Style,
    pub papers: Vec<Paper>,
    pub motion: Motion,
    pub large: bool,
    pub scales: Vec<f64>,
    pub scale: f64,
    pub suggested: Option<f64>,
    pub screen: String,
    pub device: Device,
    pub done: [bool; 4],
    pub focus: Lesson,
    pub modifier: Modifier,
    pub held: bool,
    pub search: Vec<String>,
    pub tiles: Vec<Tile>,
    pub choosing: Option<(Role, Vec<Choice>)>,
    pub note: Option<String>,
    pub quiet: bool,
    pub hover: Option<Hit>,
}

impl View {
    pub fn new(card: Card) -> View {
        View {
            card,
            greeting: String::new(),
            style: Style::Dark,
            papers: Vec::new(),
            motion: Motion::Full,
            large: false,
            scales: vec![1.0],
            scale: 1.0,
            suggested: None,
            screen: String::new(),
            device: Device::Touchpad,
            done: [false; 4],
            focus: Lesson::Search,
            modifier: Modifier::Super,
            held: false,
            search: vec!["super".into(), "space".into()],
            tiles: Vec::new(),
            choosing: None,
            note: None,
            quiet: true,
            hover: None,
        }
    }

    fn hovered(&self, hit: Hit) -> bool {
        self.hover == Some(hit)
    }
}

pub struct Fonts {
    pub hero: Font,
    pub title: Font,
    pub body: Font,
    pub small: Font,
    pub key: Font,
    pub note_title: Font,
    pub note_body: Font,
}

impl Fonts {
    pub fn load(path: Option<std::path::PathBuf>, large: bool) -> Result<Fonts, String> {
        let grow = if large { 1.08 } else { 1.0 };
        let apps = if large { crate::config::LARGE_TEXT } else { 1.0 };
        let font = |px: f64| Font::find(path.clone(), px * grow);
        let note = |px: f64| Font::find(path.clone(), px * apps);
        Ok(Fonts {
            hero: font(34.0)?,
            title: font(25.0)?,
            body: font(14.5)?,
            small: font(12.5)?,
            key: Font::find(path.clone(), 11.0)?,
            note_title: note(22.0)?,
            note_body: note(14.0)?,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub back: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub well: [u8; 3],
    pub raised: [u8; 3],
    pub hover: [u8; 3],
    pub rule: [u8; 3],
    pub accent: [u8; 3],
    pub light: bool,
}

impl Look {
    pub fn themed(p: &crate::theme::Palette, accent: [u8; 3]) -> Look {
        Look {
            back: p.back,
            ink: p.ink,
            dim: p.dim,
            well: p.well,
            raised: p.raised,
            hover: p.hover,
            rule: p.rule,
            accent,
            light: *p == crate::theme::LIGHT,
        }
    }

    pub fn rect(&self, screen: Vec2) -> Rect {
        Rect::new(((screen - SIZE) / 2.0).round(), SIZE)
    }

    pub fn radius(&self) -> f64 {
        RADIUS
    }

    pub fn paper_rect(&self, at: usize) -> Rect {
        let column = (at % 4) as f64;
        let row = (at / 4) as f64;
        Rect::new(
            Vec2::new(PAD + column * (THUMB.x + 16.0), 100.0 + row * (THUMB.y + 38.0)),
            THUMB,
        )
    }

    pub fn tile_rect(&self, at: usize) -> Rect {
        let wide = 150.0;
        let gap = 16.0;
        let left = (SIZE.x - (wide * 4.0 + gap * 3.0)) / 2.0;
        Rect::new(Vec2::new(left + at as f64 * (wide + gap), 110.0), Vec2::new(wide, 164.0))
    }

    pub fn tile_bar(&self, at: usize) -> Rect {
        let tile = self.tile_rect(at);
        Rect::new(tile.origin + Vec2::new(28.0, tile.size.y - 18.0), Vec2::new(tile.size.x - 56.0, 3.0))
    }

    pub fn tray_rect(&self) -> Rect {
        Rect::new(Vec2::new(PAD, 290.0), Vec2::new(SIZE.x - PAD * 2.0, 140.0))
    }

    pub fn choice_rect(&self, at: usize, count: usize) -> Rect {
        let wide = 108.0;
        let count = count.clamp(1, CHOICES);
        let tray = self.tray_rect();
        let left = tray.center().x - wide * count as f64 / 2.0;
        Rect::new(Vec2::new(left + at as f64 * wide, tray.origin.y + 22.0), Vec2::new(wide, 100.0))
    }

    pub fn choice_bar(&self, at: usize, count: usize) -> Rect {
        let cell = self.choice_rect(at, count);
        Rect::new(cell.origin + Vec2::new(30.0, cell.size.y - 4.0), Vec2::new(cell.size.x - 60.0, 3.0))
    }

    fn next_rect(&self) -> Rect {
        Rect::new(Vec2::new(SIZE.x - PAD - 124.0, SIZE.y - FOOT + 6.0), Vec2::new(124.0, 34.0))
    }

    fn hero_next_rect(&self) -> Rect {
        Rect::from_center(Vec2::new(SIZE.x / 2.0, 372.0), Vec2::new(150.0, 40.0))
    }

    fn back_rect(&self) -> Rect {
        Rect::new(Vec2::new(PAD - 10.0, SIZE.y - FOOT + 6.0), Vec2::new(76.0, 34.0))
    }

    fn dot_rect(&self, at: usize) -> Rect {
        let count = Card::ALL.len() as f64;
        let gap = 16.0;
        let left = SIZE.x / 2.0 - gap * (count - 1.0) / 2.0;
        Rect::from_center(Vec2::new(left + gap * at as f64, SIZE.y - FOOT + 23.0), Vec2::new(14.0, 18.0))
    }

    fn segments_rect(&self, count: usize, top: f64) -> Rect {
        let wide = 92.0 * count as f64;
        Rect::new(Vec2::new(SIZE.x - PAD - wide, top), Vec2::new(wide, 30.0))
    }

    fn segment(&self, whole: Rect, count: usize, at: usize) -> Rect {
        let wide = whole.size.x / count as f64;
        Rect::new(whole.origin + Vec2::new(wide * at as f64, 0.0), Vec2::new(wide, whole.size.y))
    }

    fn scale_rect(&self, count: usize, at: usize) -> Rect {
        let wide = 112.0;
        let left = (SIZE.x - wide * count as f64) / 2.0;
        Rect::new(Vec2::new(left + wide * at as f64, 140.0), Vec2::new(wide, 150.0))
    }

    fn well_rect(&self) -> Rect {
        Rect::new(Vec2::new(PAD, 92.0), Vec2::new(272.0, 318.0))
    }

    fn lesson_rect(&self, at: usize) -> Rect {
        let left = PAD + 272.0 + 20.0;
        Rect::new(Vec2::new(left, 92.0 + at as f64 * 80.0), Vec2::new(SIZE.x - PAD - left, 74.0))
    }

    fn quiet_rect(&self, fonts: &Fonts) -> Rect {
        let wide = 26.0 + fonts.body.width_of(QUIET);
        Rect::from_center(Vec2::new(SIZE.x / 2.0, 340.0), Vec2::new(wide + 16.0, 30.0))
    }

    pub fn spots(&self, view: &View, fonts: &Fonts) -> Vec<(Hit, Rect)> {
        let mut spots = Vec::new();
        for at in 0..Card::ALL.len() {
            spots.push((Hit::Dot(at), self.dot_rect(at)));
        }
        match view.card {
            Card::Hello => spots.push((Hit::Next, self.hero_next_rect())),
            _ => spots.push((Hit::Next, self.next_rect())),
        }
        if view.card != Card::Hello {
            spots.push((Hit::Back, self.back_rect()));
        }
        match view.card {
            Card::Hello => {}
            Card::Look => {
                let whole = self.segments_rect(3, 34.0);
                for (at, style) in [Style::Light, Style::Dark, Style::Auto].into_iter().enumerate() {
                    spots.push((Hit::Style(style), self.segment(whole, 3, at)));
                }
                for at in 0..view.papers.len().min(SHOWN_PAPERS) {
                    spots.push((Hit::Paper(at), self.paper_rect(at)));
                }
            }
            Card::Comfort => {
                let whole = self.segments_rect(3, 112.0);
                for (at, motion) in Motion::ALL.into_iter().enumerate() {
                    spots.push((Hit::Motion(motion), self.segment(whole, 3, at)));
                }
                let whole = self.segments_rect(2, 186.0);
                for (at, large) in [false, true].into_iter().enumerate() {
                    spots.push((Hit::Text(large), self.segment(whole, 2, at)));
                }
            }
            Card::Display => {
                for at in 0..view.scales.len() {
                    spots.push((Hit::Scale(at), self.scale_rect(view.scales.len(), at)));
                }
            }
            Card::Keys => {
                let whole = self.segments_rect(2, 34.0);
                for (at, device) in [Device::Touchpad, Device::Mouse].into_iter().enumerate() {
                    spots.push((Hit::Device(device), self.segment(whole, 2, at)));
                }
                for at in 0..Lesson::ALL.len() {
                    spots.push((Hit::Lesson(at), self.lesson_rect(at)));
                }
            }
            Card::Apps => {
                for at in 0..view.tiles.len().min(4) {
                    spots.push((Hit::Tile(at), self.tile_rect(at)));
                }
                if let Some((_, choices)) = &view.choosing {
                    let count = choices.len().min(CHOICES);
                    for at in 0..count {
                        spots.push((Hit::Choice(at), self.choice_rect(at, count)));
                    }
                }
            }
            Card::Done => spots.push((Hit::Quiet, self.quiet_rect(fonts))),
        }
        spots
    }

    pub fn hit(&self, view: &View, fonts: &Fonts, point: Vec2) -> Option<Hit> {
        self.spots(view, fonts)
            .into_iter()
            .rev()
            .find(|(_, rect)| rect.contains(point))
            .map(|(hit, _)| hit)
    }

    pub fn paint(&self, view: &View, fonts: &Fonts, alpha: f64) -> Image {
        let mut canvas = Canvas::new(SIZE.x as u32, SIZE.y as u32);
        let whole = Rect::new(Vec2::ZERO, SIZE);
        canvas.round_rect(whole, RADIUS, self.back, alpha);
        canvas.ring(whole, RADIUS, 1.0, WHITE, if self.light { 0.5 } else { 0.10 });
        match view.card {
            Card::Hello => self.paint_hello(&mut canvas, view, fonts),
            Card::Look => self.paint_look(&mut canvas, view, fonts),
            Card::Comfort => self.paint_comfort(&mut canvas, view, fonts),
            Card::Display => self.paint_display(&mut canvas, view, fonts),
            Card::Keys => self.paint_keys(&mut canvas, view, fonts),
            Card::Apps => self.paint_apps(&mut canvas, view, fonts),
            Card::Done => self.paint_done(&mut canvas, view, fonts),
        }
        self.paint_foot(&mut canvas, view, fonts);
        canvas.image
    }

    fn title(&self, canvas: &mut Canvas, fonts: &Fonts, text: &str, left: f64) {
        let words = fonts.title.paint_bold(text, self.ink);
        canvas.stamp(&words, Vec2::new(left, 34.0 + (30.0 - logical(&words).y) / 2.0).round(), 1.0);
    }

    fn line_center(&self, canvas: &mut Canvas, font: &Font, text: &str, y: f64, color: [u8; 3]) {
        let words = font.paint(text, color);
        let size = logical(&words);
        canvas.stamp(&words, Vec2::new((SIZE.x - size.x) / 2.0, y).round(), 1.0);
    }

    fn paint_foot(&self, canvas: &mut Canvas, view: &View, fonts: &Fonts) {
        let now = view.card.index();
        for at in 0..Card::ALL.len() {
            let center = self.dot_rect(at).center();
            match at == now {
                true => canvas.round_rect(Rect::from_center(center, Vec2::new(18.0, 6.0)), 3.0, self.ink, 0.9),
                false => {
                    let strong = view.hovered(Hit::Dot(at));
                    canvas.circle(center, 3.0, self.ink, if strong { 0.6 } else { 0.25 });
                }
            }
        }
        if view.card != Card::Hello {
            let rect = self.back_rect();
            let ink = match view.hovered(Hit::Back) {
                true => self.ink,
                false => self.dim,
            };
            let words = fonts.body.paint("Back", ink);
            let size = logical(&words);
            canvas.stamp(&words, Vec2::new(rect.origin.x + 10.0, rect.center().y - size.y / 2.0).round(), 1.0);
        }
        let rect = match view.card {
            Card::Hello => self.hero_next_rect(),
            _ => self.next_rect(),
        };
        self.button(canvas, fonts, rect, view.card.go(), view.hovered(Hit::Next));
    }

    fn button(&self, canvas: &mut Canvas, fonts: &Fonts, rect: Rect, label: &str, hover: bool) {
        canvas.shadow(rect, rect.size.y / 2.0, 8.0, 0.18);
        let top = lift(self.accent, if hover { 0.22 } else { 0.12 });
        let bottom = lift(self.accent, if hover { 0.06 } else { -0.04 });
        canvas.shade(rect, rect.size.y / 2.0, top, bottom, 1.0);
        let words = fonts.body.paint_bold(label, WHITE);
        let size = logical(&words);
        canvas.stamp(&words, (rect.center() - size / 2.0).round(), 1.0);
    }

    fn segments(&self, canvas: &mut Canvas, fonts: &Fonts, whole: Rect, labels: &[&str], chosen: usize, hover: Option<usize>) {
        canvas.round_rect(whole, 9.0, self.well, 1.0);
        canvas.ring(whole, 9.0, 1.0, self.rule, 0.6);
        let count = labels.len();
        for (at, label) in labels.iter().enumerate() {
            let rect = self.segment(whole, count, at);
            let inner = Rect::new(rect.origin + Vec2::splat(2.5), rect.size - Vec2::splat(5.0));
            if at == chosen {
                canvas.shadow(inner, 7.0, 4.0, 0.25);
                let face = match self.light {
                    true => WHITE,
                    false => self.hover,
                };
                canvas.round_rect(inner, 7.0, face, 1.0);
            } else if hover == Some(at) {
                canvas.round_rect(inner, 7.0, self.raised, 0.6);
            }
            let ink = match at == chosen {
                true => self.ink,
                false => self.dim,
            };
            let words = match at == chosen {
                true => fonts.small.paint_bold(label, ink),
                false => fonts.small.paint(label, ink),
            };
            let size = logical(&words);
            canvas.stamp(&words, (rect.center() - size / 2.0).round(), 1.0);
        }
    }

    fn paint_hello(&self, canvas: &mut Canvas, view: &View, fonts: &Fonts) {
        let center = Vec2::new(SIZE.x / 2.0, 168.0);
        mark(canvas, center, 46.0, self.accent);
        let hero = fonts.hero.paint_bold("Welcome to Drifter", self.ink);
        let size = logical(&hero);
        canvas.stamp(&hero, Vec2::new((SIZE.x - size.x) / 2.0, 240.0).round(), 1.0);
        self.line_center(canvas, &fonts.body, &view.greeting, 294.0, self.dim);
    }

    fn paint_look(&self, canvas: &mut Canvas, view: &View, fonts: &Fonts) {
        self.title(canvas, fonts, "Pick your look", PAD);
        let styles = [Style::Light, Style::Dark, Style::Auto];
        let chosen = styles.iter().position(|style| *style == view.style).unwrap_or(1);
        let hover = styles.iter().position(|style| view.hovered(Hit::Style(*style)));
        self.segments(canvas, fonts, self.segments_rect(3, 34.0), &["Light", "Dark", "Auto"], chosen, hover);
        for (at, paper) in view.papers.iter().take(SHOWN_PAPERS).enumerate() {
            let rect = self.paper_rect(at);
            let hover = view.hovered(Hit::Paper(at));
            canvas.shadow(rect, 12.0, if hover { 12.0 } else { 7.0 }, if hover { 0.4 } else { 0.25 });
            match &paper.still {
                Some(still) => canvas.stamp(still, rect.origin, 1.0),
                None => canvas.round_rect(rect, 12.0, self.raised, 1.0),
            }
            canvas.ring(rect, 12.0, 1.0, WHITE, 0.14);
            if paper.chosen {
                let around = Rect::new(rect.origin - Vec2::splat(4.0), rect.size + Vec2::splat(8.0));
                canvas.ring(around, 15.0, 2.5, self.accent, 1.0);
            }
            let ink = match paper.chosen || hover {
                true => self.ink,
                false => self.dim,
            };
            let name = fonts.small.paint(&paper.name, ink);
            let size = logical(&name);
            canvas.stamp(&name, Vec2::new(rect.center().x - size.x / 2.0, rect.max().y + 9.0).round(), 1.0);
        }
        let rows = view.papers.len().min(SHOWN_PAPERS).div_ceil(4).max(1);
        let y = self.paper_rect((rows - 1) * 4).max().y + 44.0;
        self.line_center(canvas, &fonts.small, LIVEPAPER, y, self.dim);
    }

    fn row_label(&self, canvas: &mut Canvas, fonts: &Fonts, top: f64, label: &str, detail: &str) {
        let words = fonts.body.paint_bold(label, self.ink);
        canvas.stamp(&words, Vec2::new(PAD, top - 2.0).round(), 1.0);
        let words = fonts.small.paint(detail, self.dim);
        canvas.stamp(&words, Vec2::new(PAD, top + 18.0).round(), 1.0);
    }

    fn paint_comfort(&self, canvas: &mut Canvas, view: &View, fonts: &Fonts) {
        self.title(canvas, fonts, "Motion and text", PAD);
        self.row_label(canvas, fonts, 112.0, "Motion", "How much the desktop moves");
        let chosen = Motion::ALL.iter().position(|motion| *motion == view.motion).unwrap_or(2);
        let hover = Motion::ALL.iter().position(|motion| view.hovered(Hit::Motion(*motion)));
        let labels: Vec<&str> = Motion::ALL.iter().map(|motion| motion.label()).collect();
        self.segments(canvas, fonts, self.segments_rect(3, 112.0), &labels, chosen, hover);
        canvas.round_rect(Rect::new(Vec2::new(PAD, 160.0), Vec2::new(SIZE.x - PAD * 2.0, 1.0)), 0.0, self.rule, 0.8);
        self.row_label(canvas, fonts, 186.0, "Text size", "The size your apps use");
        let hover = [false, true].iter().position(|large| view.hovered(Hit::Text(*large)));
        self.segments(canvas, fonts, self.segments_rect(2, 186.0), &["Regular", "Large"], usize::from(view.large), hover);
        let sheet = Rect::new(Vec2::new(PAD, 244.0), Vec2::new(SIZE.x - PAD * 2.0, 164.0));
        canvas.shadow(sheet, 14.0, 10.0, 0.22);
        let paper = match self.light {
            true => WHITE,
            false => self.well,
        };
        canvas.round_rect(sheet, 14.0, paper, 1.0);
        canvas.ring(sheet, 14.0, 1.0, self.rule, 0.7);
        for (at, color) in [[255, 95, 87], [254, 188, 46], [40, 200, 64]].into_iter().enumerate() {
            canvas.circle(sheet.origin + Vec2::new(20.0 + at as f64 * 18.0, 18.0), 5.5, color, 1.0);
        }
        let heading = fonts.note_title.paint_bold("Notes for Tuesday", self.ink);
        canvas.stamp(&heading, sheet.origin + Vec2::new(22.0, 40.0), 1.0);
        let lines = [
            "Walk before lunch. Water the fern.",
            "Call about the bike, then rest a little.",
        ];
        let mut y = sheet.origin.y + 40.0 + logical(&heading).y + 12.0;
        for line in lines {
            let words = fonts.note_body.paint(line, self.dim);
            canvas.stamp(&words, Vec2::new(sheet.origin.x + 22.0, y).round(), 1.0);
            y += logical(&words).y + 8.0;
        }
    }

    fn paint_display(&self, canvas: &mut Canvas, view: &View, fonts: &Fonts) {
        self.title(canvas, fonts, "Choose a size", PAD);
        let words = fonts.body.paint("Pick the one that looks sharp and easy to read.", self.dim);
        canvas.stamp(&words, Vec2::new(PAD, 76.0), 1.0);
        let count = view.scales.len();
        let most = view.scales.iter().copied().fold(1.0, f64::max);
        for (at, scale) in view.scales.iter().enumerate() {
            let cell = self.scale_rect(count, at);
            let chosen = (scale - view.scale).abs() < 0.01;
            let hover = view.hovered(Hit::Scale(at));
            let screen = Rect::from_center(Vec2::new(cell.center().x, cell.origin.y + 44.0), Vec2::new(92.0, 62.0));
            canvas.shadow(screen, 8.0, 6.0, 0.25);
            canvas.round_rect(screen, 8.0, [22, 24, 30], 1.0);
            let glass = Rect::new(screen.origin + Vec2::splat(4.0), screen.size - Vec2::splat(8.0));
            canvas.shade(glass, 5.0, lift(self.accent, 0.25), lift(self.accent, -0.25), 1.0);
            let grow = 1.0 + 0.55 * (scale - 1.0) / (most - 1.0).max(0.25);
            let bar = Rect::new(glass.origin, Vec2::new(glass.size.x, 3.0 * grow));
            canvas.round_rect(bar, 1.5, WHITE, 0.4);
            let window = Rect::new(glass.origin + Vec2::new(7.0, 5.0 + 3.0 * grow), Vec2::new(36.0, 30.0) * grow);
            let window = Rect::new(window.origin, window.size.min(glass.max() - window.origin - Vec2::splat(2.0)));
            canvas.round_rect(window, 2.5 * grow, WHITE, 0.92);
            canvas.round_rect(Rect::new(window.origin, Vec2::new(window.size.x, 4.0 * grow)), 2.0, [214, 218, 228], 1.0);
            for line in 0..3 {
                let y = window.origin.y + (8.0 + line as f64 * 5.0) * grow;
                if y + 2.0 * grow > window.max().y - 2.0 {
                    break;
                }
                let wide = (window.size.x - 8.0 * grow) * [1.0, 0.8, 0.55][line];
                canvas.round_rect(Rect::new(Vec2::new(window.origin.x + 4.0 * grow, y), Vec2::new(wide, 1.6 * grow)), 0.8, [120, 126, 140], 1.0);
            }
            canvas.round_rect(Rect::new(Vec2::new(screen.center().x - 10.0, screen.max().y), Vec2::new(20.0, 7.0)), 1.0, [60, 64, 76], 1.0);
            canvas.round_rect(Rect::new(Vec2::new(screen.center().x - 20.0, screen.max().y + 7.0), Vec2::new(40.0, 3.0)), 1.5, [60, 64, 76], 1.0);
            if chosen {
                let around = Rect::new(screen.origin - Vec2::splat(5.0), screen.size + Vec2::splat(10.0));
                canvas.ring(around, 12.0, 2.5, self.accent, 1.0);
            } else if hover {
                let around = Rect::new(screen.origin - Vec2::splat(5.0), screen.size + Vec2::splat(10.0));
                canvas.ring(around, 12.0, 1.5, self.ink, 0.35);
            }
            let label = format!("{}%", (scale * 100.0).round());
            let words = match chosen {
                true => fonts.body.paint_bold(&label, self.ink),
                false => fonts.body.paint(&label, self.dim),
            };
            let size = logical(&words);
            canvas.stamp(&words, Vec2::new(cell.center().x - size.x / 2.0, cell.origin.y + 96.0).round(), 1.0);
            if view.suggested.is_some_and(|best| (best - scale).abs() < 0.01) {
                let words = fonts.small.paint("Suggested", self.accent);
                let size = logical(&words);
                canvas.stamp(&words, Vec2::new(cell.center().x - size.x / 2.0, cell.origin.y + 118.0).round(), 1.0);
            }
        }
        if count > 1 {
            let first = self.scale_rect(count, 0);
            let last = self.scale_rect(count, count - 1);
            let y = first.max().y + 22.0;
            let left = fonts.small.paint("More space", self.dim);
            canvas.stamp(&left, Vec2::new(first.center().x - logical(&left).x / 2.0, y).round(), 1.0);
            let right = fonts.small.paint("Larger", self.dim);
            canvas.stamp(&right, Vec2::new(last.center().x - logical(&right).x / 2.0, y).round(), 1.0);
            let from = Vec2::new(first.center().x + logical(&left).x / 2.0 + 12.0, y + 8.0);
            let to = Vec2::new(last.center().x - logical(&right).x / 2.0 - 12.0, y + 8.0);
            canvas.line(from, to, 1.0, self.dim, 0.4);
        }
        if !view.screen.is_empty() {
            self.line_center(canvas, &fonts.small, &view.screen, 360.0, self.dim);
        }
    }

    fn lessons(&self, view: &View) -> [(Lesson, Vec<Bit>); 4] {
        let held = match view.modifier {
            Modifier::Super => "super",
            Modifier::Alt => "alt",
            Modifier::Ctrl => "ctrl",
        };
        let key = |word: &str| Bit::Key(word.to_string());
        let word = |text: &str| Bit::Word(text.to_string());
        let search: Vec<Bit> = match view.search.is_empty() {
            true => vec![word("Click the search in the top bar")],
            false => {
                let mut bits = vec![word("Press")];
                bits.extend(view.search.iter().map(|name| key(name)));
                bits
            }
        };
        match view.device {
            Device::Touchpad => [
                (Lesson::Search, search),
                (Lesson::Glide, vec![word("Swipe with three fingers")]),
                (Lesson::Overview, vec![word("Tap with three fingers")]),
                (Lesson::Zoom, vec![word("Hold"), key(held), word("and scroll with two fingers")]),
            ],
            Device::Mouse => [
                (Lesson::Search, search),
                (Lesson::Glide, vec![word("Hold"), key(held), word("and push into an edge")]),
                (Lesson::Overview, vec![word("Rest the pointer in the top left corner")]),
                (Lesson::Zoom, vec![word("Hold"), key(held), word("and turn the wheel")]),
            ],
        }
    }

    fn paint_keys(&self, canvas: &mut Canvas, view: &View, fonts: &Fonts) {
        let all = view.done.iter().all(|done| *done);
        self.title(canvas, fonts, if all { "You know the moves" } else { "Four moves to know" }, PAD);
        let device = [Device::Touchpad, Device::Mouse];
        let chosen = device.iter().position(|each| *each == view.device).unwrap_or(0);
        let hover = device.iter().position(|each| view.hovered(Hit::Device(*each)));
        self.segments(canvas, fonts, self.segments_rect(2, 34.0), &["Touchpad", "Mouse"], chosen, hover);
        let well = self.well_rect();
        let (high, low) = match self.light {
            true => ([246, 247, 250], [230, 232, 238]),
            false => ([38, 41, 52], [22, 24, 31]),
        };
        canvas.shade(well, 18.0, high, low, 1.0);
        canvas.glow(Vec2::new(well.center().x, well.origin.y + 90.0), 150.0, self.accent, 0.07);
        canvas.ring(well, 18.0, 1.0, self.rule, 0.6);
        self.illustrate(canvas, view, fonts, well);
        for (at, (lesson, bits)) in self.lessons(view).into_iter().enumerate() {
            let rect = self.lesson_rect(at);
            let focus = view.focus == lesson;
            if focus {
                canvas.round_rect(rect, 14.0, self.raised, if self.light { 0.6 } else { 0.55 });
            } else if view.hovered(Hit::Lesson(at)) {
                canvas.round_rect(rect, 14.0, self.raised, 0.3);
            }
            let badge = Vec2::new(rect.origin.x + 26.0, rect.origin.y + 26.0);
            match view.done[at] {
                true => {
                    canvas.circle(badge, 12.0, GREEN, 1.0);
                    tick(canvas, badge, 5.0, WHITE);
                }
                false => {
                    canvas.ring(Rect::from_center(badge, Vec2::splat(24.0)), 12.0, 1.5, self.dim, 0.7);
                    let number = fonts.small.paint_bold(&(at + 1).to_string(), self.dim);
                    canvas.stamp(&number, (badge - logical(&number) / 2.0).round(), 1.0);
                }
            }
            let left = rect.origin.x + 50.0;
            let words = fonts.body.paint_bold(lesson.title(), self.ink);
            canvas.stamp(&words, Vec2::new(left, rect.origin.y + 14.0).round(), 1.0);
            self.bits(canvas, fonts, &bits, Vec2::new(left, rect.origin.y + 38.0), view.held);
        }
    }

    fn bits(&self, canvas: &mut Canvas, fonts: &Fonts, bits: &[Bit], at: Vec2, held: bool) {
        let mut x = at.x;
        for bit in bits {
            match bit {
                Bit::Word(text) => {
                    let words = fonts.small.paint(text, self.dim);
                    canvas.stamp(&words, Vec2::new(x, at.y + (22.0 - logical(&words).y) / 2.0).round(), 1.0);
                    x += logical(&words).x + 6.0;
                }
                Bit::Key(name) => {
                    let wide = cap_width(&fonts.key, name);
                    let rect = Rect::new(Vec2::new(x, at.y), Vec2::new(wide, 22.0));
                    let lit = held && is_modifier(name);
                    self.keycap(canvas, fonts, rect, name, lit);
                    x += wide + 6.0;
                }
            }
        }
    }

    pub fn keycap(&self, canvas: &mut Canvas, fonts: &Fonts, rect: Rect, name: &str, lit: bool) {
        let radius = (rect.size.y * 0.24).min(9.0);
        let depth = (rect.size.y * 0.07).max(1.5);
        canvas.shadow(Rect::new(rect.origin + Vec2::new(0.0, depth), rect.size), radius, depth * 2.0, 0.35);
        let (rim, top, bottom) = match (lit, self.light) {
            (true, _) => (lift(self.accent, -0.3), lift(self.accent, 0.18), self.accent),
            (false, false) => ([20, 21, 27], [86, 90, 106], [66, 70, 84]),
            (false, true) => ([178, 182, 192], [255, 255, 255], [240, 241, 245]),
        };
        canvas.round_rect(Rect::new(rect.origin + Vec2::new(0.0, depth * 0.6), rect.size), radius, rim, 1.0);
        let face = Rect::new(rect.origin, rect.size - Vec2::new(0.0, depth * 0.4));
        canvas.shade(face, radius, top, bottom, 1.0);
        canvas.ring(face, radius, 1.0, WHITE, if lit { 0.3 } else if self.light { 0.9 } else { 0.08 });
        let ink = match (lit, self.light) {
            (true, _) => WHITE,
            (false, false) => [232, 234, 240],
            (false, true) => [60, 62, 70],
        };
        let font = &fonts.key;
        let big = rect.size.y > 40.0;
        let words = match big {
            true => fonts.body.paint(name, ink),
            false => font.paint(name, ink),
        };
        let size = logical(&words);
        let unit = rect.size.y / 22.0;
        match is_modifier(name) || name == "space" {
            true => {
                let at = Vec2::new(rect.origin.x + 6.0 * unit, face.max().y - size.y - 2.5 * unit);
                canvas.stamp(&words, at.round(), 1.0);
                if name == "super" {
                    let corner = Vec2::new(face.max().x - 8.0 * unit, face.origin.y + 7.0 * unit);
                    let cell = 2.6 * unit;
                    for (dx, dy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                        let center = corner + Vec2::new(dx, dy) * (cell * 0.62);
                        canvas.round_rect(Rect::from_center(center, Vec2::splat(cell)), cell * 0.3, ink, 0.9);
                    }
                }
            }
            false => canvas.stamp(&words, (face.center() - size / 2.0).round(), 1.0),
        }
    }

    fn illustrate(&self, canvas: &mut Canvas, view: &View, fonts: &Fonts, well: Rect) {
        let center = well.center();
        let held = match view.modifier {
            Modifier::Super => "super",
            Modifier::Alt => "alt",
            Modifier::Ctrl => "ctrl",
        };
        let caption = |canvas: &mut Canvas, text: &str| {
            let words = fonts.small.paint(text, self.dim);
            let size = logical(&words);
            canvas.stamp(&words, Vec2::new(center.x - size.x / 2.0, well.max().y - 36.0).round(), 1.0);
        };
        match (view.focus, view.device) {
            (Lesson::Search, _) => {
                let keys: Vec<String> = match view.search.is_empty() {
                    true => vec!["super".into(), "space".into()],
                    false => view.search.clone(),
                };
                let tall = 56.0;
                let widths: Vec<f64> = keys
                    .iter()
                    .map(|name| match name.as_str() {
                        "space" => 112.0,
                        name if is_modifier(name) => 76.0,
                        _ => 56.0,
                    })
                    .collect();
                let plus = 22.0;
                let total: f64 = widths.iter().sum::<f64>() + plus * (keys.len().saturating_sub(1)) as f64;
                let mut x = center.x - total / 2.0;
                for (at, (name, wide)) in keys.iter().zip(&widths).enumerate() {
                    if at > 0 {
                        let mark = fonts.title.paint("+", self.dim);
                        canvas.stamp(&mark, Vec2::new(x + (plus - logical(&mark).x) / 2.0, center.y - 20.0 - logical(&mark).y / 2.0).round(), 1.0);
                        x += plus;
                    }
                    let rect = Rect::new(Vec2::new(x, center.y - 20.0 - tall / 2.0), Vec2::new(*wide, tall));
                    self.keycap(canvas, fonts, rect, name, view.held && is_modifier(name));
                    x += wide;
                }
                caption(canvas, "Then type a few letters");
            }
            (lesson, Device::Touchpad) => {
                let pad = Rect::from_center(center + Vec2::new(0.0, 6.0), Vec2::new(200.0, 132.0));
                self.trackpad(canvas, pad);
                let fingers = |count: usize, at: Vec2| -> Vec<Vec2> {
                    (0..count)
                        .map(|i| {
                            let offset = i as f64 - (count as f64 - 1.0) / 2.0;
                            at + Vec2::new(offset * 24.0, (offset * offset) * 3.0 - 4.0)
                        })
                        .collect()
                };
                match lesson {
                    Lesson::Glide => {
                        for (step, fade) in [(-46.0, 0.12), (-26.0, 0.25)] {
                            for dot in fingers(3, pad.center() + Vec2::new(step, 0.0)) {
                                canvas.circle(dot, 9.0, WHITE, fade);
                            }
                        }
                        for dot in fingers(3, pad.center()) {
                            self.finger(canvas, dot);
                        }
                        arrow(canvas, pad.center() + Vec2::new(44.0, 26.0), pad.center() + Vec2::new(78.0, 26.0), self.accent);
                        caption(canvas, "The world slides with your fingers");
                    }
                    Lesson::Overview => {
                        for dot in fingers(3, pad.center()) {
                            canvas.ring(Rect::from_center(dot, Vec2::splat(34.0)), 17.0, 1.5, self.accent, 0.35);
                            canvas.ring(Rect::from_center(dot, Vec2::splat(48.0)), 24.0, 1.0, self.accent, 0.15);
                            self.finger(canvas, dot);
                        }
                        caption(canvas, "Tap again to fly back");
                    }
                    _ => {
                        let cap = Rect::from_center(Vec2::new(center.x, pad.origin.y - 32.0), Vec2::new(76.0, 40.0));
                        self.keycap(canvas, fonts, cap, held, view.held);
                        for dot in fingers(2, pad.center()) {
                            self.finger(canvas, dot);
                        }
                        arrow(canvas, pad.center() + Vec2::new(46.0, 12.0), pad.center() + Vec2::new(46.0, -26.0), self.accent);
                        arrow(canvas, pad.center() + Vec2::new(-46.0, -12.0), pad.center() + Vec2::new(-46.0, 26.0), self.accent);
                        caption(canvas, "Up to zoom in, down to zoom out");
                    }
                }
            }
            (lesson, Device::Mouse) => match lesson {
                Lesson::Glide | Lesson::Overview => {
                    let screen = Rect::from_center(center + Vec2::new(0.0, 14.0), Vec2::new(204.0, 128.0));
                    canvas.shadow(screen, 10.0, 8.0, 0.3);
                    canvas.round_rect(screen, 10.0, [22, 24, 30], 1.0);
                    let glass = Rect::new(screen.origin + Vec2::splat(5.0), screen.size - Vec2::splat(10.0));
                    canvas.shade(glass, 6.0, lift(self.accent, 0.1), lift(self.accent, -0.45), 1.0);
                    for (spot, size) in [(Vec2::new(24.0, 26.0), Vec2::new(64.0, 44.0)), (Vec2::new(100.0, 44.0), Vec2::new(52.0, 38.0))] {
                        let window = Rect::new(glass.origin + spot, size);
                        canvas.shadow(window, 4.0, 4.0, 0.25);
                        canvas.round_rect(window, 4.0, WHITE, 0.9);
                        canvas.round_rect(Rect::new(window.origin, Vec2::new(size.x, 7.0)), 3.0, [214, 218, 228], 1.0);
                    }
                    match lesson {
                        Lesson::Glide => {
                            let cap = Rect::from_center(Vec2::new(center.x, screen.origin.y - 34.0), Vec2::new(76.0, 40.0));
                            self.keycap(canvas, fonts, cap, held, view.held);
                            let tip = Vec2::new(glass.max().x - 14.0, glass.center().y - 6.0);
                            arrow(canvas, tip - Vec2::new(56.0, -12.0), tip - Vec2::new(18.0, -12.0), WHITE);
                            pointer(canvas, tip);
                            canvas.round_rect(Rect::new(Vec2::new(glass.max().x - 3.0, glass.origin.y + 8.0), Vec2::new(3.0, glass.size.y - 16.0)), 1.5, WHITE, 0.8);
                            caption(canvas, "The pointer pulls the world along");
                        }
                        _ => {
                            canvas.glow(glass.origin + Vec2::splat(4.0), 46.0, WHITE, 0.55);
                            pointer(canvas, glass.origin + Vec2::new(10.0, 8.0));
                            caption(canvas, "Click a window to fly to it");
                        }
                    }
                }
                _ => {
                    let cap = Rect::from_center(Vec2::new(center.x - 54.0, center.y - 4.0), Vec2::new(76.0, 40.0));
                    self.keycap(canvas, fonts, cap, held, view.held);
                    let plus = fonts.title.paint("+", self.dim);
                    canvas.stamp(&plus, (Vec2::new(center.x + 2.0, center.y - 4.0) - logical(&plus) / 2.0).round(), 1.0);
                    let body = Rect::from_center(Vec2::new(center.x + 58.0, center.y - 4.0), Vec2::new(62.0, 100.0));
                    self.mouse(canvas, body, true);
                    arrow(canvas, Vec2::new(body.max().x + 16.0, body.center().y + 18.0), Vec2::new(body.max().x + 16.0, body.center().y - 22.0), self.accent);
                    caption(canvas, "Up to zoom in, down to zoom out");
                }
            },
        }
    }

    fn trackpad(&self, canvas: &mut Canvas, pad: Rect) {
        canvas.shadow(pad, 16.0, 10.0, 0.35);
        let (top, bottom) = match self.light {
            true => ([250, 251, 253], [226, 229, 236]),
            false => ([66, 70, 84], [46, 49, 60]),
        };
        canvas.shade(pad, 16.0, top, bottom, 1.0);
        canvas.ring(pad, 16.0, 1.0, WHITE, if self.light { 0.9 } else { 0.1 });
    }

    fn finger(&self, canvas: &mut Canvas, at: Vec2) {
        canvas.shadow(Rect::from_center(at + Vec2::new(0.0, 2.0), Vec2::splat(18.0)), 9.0, 4.0, 0.35);
        canvas.circle(at, 9.0, WHITE, 0.96);
        canvas.circle(at - Vec2::new(2.0, 2.5), 3.0, WHITE, 1.0);
    }

    fn mouse(&self, canvas: &mut Canvas, body: Rect, wheel: bool) {
        canvas.shadow(body, body.size.x / 2.0, 10.0, 0.35);
        let (top, bottom) = match self.light {
            true => ([252, 252, 254], [222, 225, 232]),
            false => ([78, 82, 98], [48, 51, 62]),
        };
        canvas.shade(body, body.size.x / 2.0, top, bottom, 1.0);
        canvas.ring(body, body.size.x / 2.0, 1.0, WHITE, if self.light { 0.9 } else { 0.1 });
        let split = body.origin.y + body.size.y * 0.4;
        canvas.line(Vec2::new(body.center().x, body.origin.y + 4.0), Vec2::new(body.center().x, split), 1.0, BLACK, 0.3);
        canvas.line(Vec2::new(body.origin.x + 6.0, split), Vec2::new(body.max().x - 6.0, split), 1.0, BLACK, 0.18);
        let roll = Rect::from_center(Vec2::new(body.center().x, body.origin.y + body.size.y * 0.2), Vec2::new(7.0, 16.0));
        let color = match wheel {
            true => self.accent,
            false => self.dim,
        };
        canvas.round_rect(roll, 3.5, color, 1.0);
    }

    fn paint_apps(&self, canvas: &mut Canvas, view: &View, fonts: &Fonts) {
        self.title(canvas, fonts, "Your apps", PAD);
        let words = fonts.body.paint("These open your links, files and notes. Click one to change it.", self.dim);
        canvas.stamp(&words, Vec2::new(PAD, 76.0), 1.0);
        let open = view.choosing.as_ref().map(|(role, _)| *role);
        for (at, tile) in view.tiles.iter().take(4).enumerate() {
            let rect = self.tile_rect(at);
            let hover = view.hovered(Hit::Tile(at));
            let active = open == Some(tile.role);
            let dim = open.is_some() && !active;
            canvas.shadow(rect, 20.0, if hover || active { 14.0 } else { 8.0 }, if hover || active { 0.3 } else { 0.16 });
            let face = match self.light {
                true => WHITE,
                false => self.card_face(hover || active),
            };
            canvas.round_rect(rect, 20.0, face, 1.0);
            match active {
                true => canvas.ring(rect, 20.0, 2.5, self.accent, 1.0),
                false => canvas.ring(rect, 20.0, 1.0, WHITE, if self.light { 0.9 } else { 0.07 }),
            }
            let fade = if dim { 0.55 } else { 1.0 };
            let icon = Rect::from_center(Vec2::new(rect.center().x, rect.origin.y + 54.0), Vec2::splat(64.0));
            match &tile.name {
                Some(name) => self.app_icon(canvas, fonts, icon, tile.icon.as_ref(), name, fade),
                None => {
                    canvas.ring(icon, 19.0, 1.5, self.dim, 0.6 * fade);
                    canvas.line(icon.center() - Vec2::new(10.0, 0.0), icon.center() + Vec2::new(10.0, 0.0), 2.0, self.dim, 0.9 * fade);
                    canvas.line(icon.center() - Vec2::new(0.0, 10.0), icon.center() + Vec2::new(0.0, 10.0), 2.0, self.dim, 0.9 * fade);
                }
            }
            let name = tile.name.clone().unwrap_or_else(|| "Choose one".to_string());
            let name = fit(&fonts.body, &name, rect.size.x - 20.0);
            let words = fonts.body.paint_bold(&name, self.ink);
            canvas.stamp(&words, Vec2::new(rect.center().x - logical(&words).x / 2.0, rect.origin.y + 98.0).round(), fade);
            let (label, ink) = match tile.busy {
                true => ("Getting it", self.accent),
                false => (tile.role.label(), self.dim),
            };
            let words = fonts.small.paint(label, ink);
            canvas.stamp(&words, Vec2::new(rect.center().x - logical(&words).x / 2.0, rect.origin.y + 120.0).round(), fade);
        }
        let Some((role, choices)) = &view.choosing else {
            return;
        };
        let tray = self.tray_rect();
        let floor = match self.light {
            true => [236, 238, 243],
            false => self.well,
        };
        if let Some(at) = view.tiles.iter().position(|tile| tile.role == *role) {
            let x = self.tile_rect(at).center().x;
            canvas.triangle(
                [Vec2::new(x, tray.origin.y - 9.0), Vec2::new(x + 11.0, tray.origin.y + 1.0), Vec2::new(x - 11.0, tray.origin.y + 1.0)],
                floor,
                1.0,
            );
        }
        canvas.round_rect(tray, 18.0, floor, 1.0);
        let count = choices.len().min(CHOICES);
        for (at, choice) in choices.iter().take(CHOICES).enumerate() {
            let cell = self.choice_rect(at, count);
            let hover = view.hovered(Hit::Choice(at));
            let icon = Rect::from_center(Vec2::new(cell.center().x, cell.origin.y + 32.0), Vec2::splat(52.0));
            if hover {
                canvas.round_rect(Rect::new(cell.origin + Vec2::new(6.0, -4.0), cell.size - Vec2::new(12.0, 0.0)), 14.0, self.raised, 0.5);
            }
            let fade = match choice.installed {
                true => 1.0,
                false => 0.55,
            };
            self.app_icon(canvas, fonts, icon, choice.icon.as_ref(), &choice.name, fade);
            if choice.chosen {
                let around = Rect::new(icon.origin - Vec2::splat(5.0), icon.size + Vec2::splat(10.0));
                canvas.ring(around, 16.0, 2.5, self.accent, 1.0);
                let badge = Vec2::new(icon.max().x + 2.0, icon.origin.y + 2.0);
                canvas.circle(badge, 9.0, self.accent, 1.0);
                tick(canvas, badge, 4.0, WHITE);
            }
            if !choice.installed && !choice.busy {
                let badge = Vec2::new(icon.max().x + 2.0, icon.max().y - 2.0);
                canvas.circle(badge, 9.0, self.accent, 1.0);
                canvas.line(badge - Vec2::new(0.0, 4.0), badge + Vec2::new(0.0, 2.5), 1.7, WHITE, 1.0);
                canvas.line(badge + Vec2::new(-3.2, 0.0), badge + Vec2::new(0.0, 3.2), 1.7, WHITE, 1.0);
                canvas.line(badge + Vec2::new(3.2, 0.0), badge + Vec2::new(0.0, 3.2), 1.7, WHITE, 1.0);
            }
            let name = fit(&fonts.small, &choice.name, cell.size.x - 12.0);
            let ink = match choice.chosen || hover {
                true => self.ink,
                false => self.dim,
            };
            let words = fonts.small.paint(&name, ink);
            canvas.stamp(&words, Vec2::new(cell.center().x - logical(&words).x / 2.0, cell.origin.y + 64.0).round(), 1.0);
            let under = match (choice.busy, choice.installed) {
                (true, _) => Some("Getting it"),
                (false, false) => Some("Get"),
                _ => None,
            };
            if let Some(under) = under {
                let words = fonts.small.paint(under, self.accent);
                canvas.stamp(&words, Vec2::new(cell.center().x - logical(&words).x / 2.0, cell.origin.y + 80.0).round(), 1.0);
            }
        }
        if let Some(note) = &view.note {
            self.line_center(canvas, &fonts.small, note, tray.max().y - 22.0, self.dim);
        }
    }

    fn card_face(&self, hover: bool) -> [u8; 3] {
        match hover {
            true => self.hover,
            false => self.raised,
        }
    }

    fn app_icon(&self, canvas: &mut Canvas, fonts: &Fonts, rect: Rect, icon: Option<&Image>, name: &str, fade: f64) {
        match icon {
            Some(image) => {
                let size = logical(image);
                canvas.stamp(image, (rect.center() - size / 2.0).round(), fade);
            }
            None => {
                let hue = name.bytes().fold(7u32, |sum, byte| sum.wrapping_mul(31).wrapping_add(u32::from(byte)));
                let tint = [[94, 92, 230], [255, 149, 0], [52, 199, 89], [0, 122, 255], [255, 55, 95], [90, 200, 250]]
                    [(hue % 6) as usize];
                canvas.shadow(rect, rect.size.x * 0.23, 5.0, 0.25 * fade);
                canvas.shade(rect, rect.size.x * 0.23, lift(tint, 0.2), lift(tint, -0.15), fade);
                let letter: String = name.chars().next().map(|c| c.to_uppercase().collect()).unwrap_or_default();
                let words = fonts.hero.paint_bold(&letter, WHITE);
                canvas.stamp(&words, (rect.center() - logical(&words) / 2.0).round(), fade);
            }
        }
    }

    fn paint_done(&self, canvas: &mut Canvas, view: &View, fonts: &Fonts) {
        let center = Vec2::new(SIZE.x / 2.0, 158.0);
        canvas.glow(center, 90.0, GREEN, 0.25);
        canvas.shadow(Rect::from_center(center + Vec2::new(0.0, 4.0), Vec2::splat(84.0)), 42.0, 10.0, 0.3);
        canvas.shade(Rect::from_center(center, Vec2::splat(84.0)), 42.0, lift(GREEN, 0.2), lift(GREEN, -0.15), 1.0);
        tick(canvas, center, 17.0, WHITE);
        let hero = fonts.hero.paint_bold("You're all set", self.ink);
        canvas.stamp(&hero, Vec2::new((SIZE.x - logical(&hero).x) / 2.0, 226.0).round(), 1.0);
        self.line_center(canvas, &fonts.body, "Open this again any time from Programs.", 278.0, self.dim);
        let rect = self.quiet_rect(fonts);
        if view.hovered(Hit::Quiet) {
            canvas.round_rect(rect, 8.0, self.raised, 0.4);
        }
        let box_rect = Rect::new(Vec2::new(rect.origin.x + 8.0, rect.center().y - 9.0), Vec2::splat(18.0));
        match view.quiet {
            true => {
                canvas.round_rect(box_rect, 5.0, self.accent, 1.0);
                tick(canvas, box_rect.center(), 4.5, WHITE);
            }
            false => canvas.ring(box_rect, 5.0, 1.5, self.dim, 0.9),
        }
        let words = fonts.body.paint(QUIET, self.ink);
        canvas.stamp(&words, Vec2::new(box_rect.max().x + 10.0, rect.center().y - logical(&words).y / 2.0).round(), 1.0);
    }
}

const QUIET: &str = "Don't show this at startup";

fn is_modifier(name: &str) -> bool {
    matches!(name, "super" | "alt" | "ctrl" | "shift")
}

fn cap_width(font: &Font, name: &str) -> f64 {
    let words = font.width_of(name) + 14.0;
    match name {
        "space" => words.max(66.0),
        "super" => words.max(56.0) + 8.0,
        name if is_modifier(name) => words.max(46.0),
        _ => words.max(24.0),
    }
}

fn lift(color: [u8; 3], amount: f64) -> [u8; 3] {
    color.map(|c| {
        let c = f64::from(c);
        let out = match amount >= 0.0 {
            true => c + (255.0 - c) * amount,
            false => c * (1.0 + amount),
        };
        out.round().clamp(0.0, 255.0) as u8
    })
}

fn tick(canvas: &mut Canvas, center: Vec2, size: f64, color: [u8; 3]) {
    let a = center + Vec2::new(-size, 0.0);
    let b = center + Vec2::new(-size * 0.3, size * 0.7);
    let c = center + Vec2::new(size, -size * 0.75);
    let width = (size * 0.38).max(1.8);
    canvas.line(a, b, width, color, 1.0);
    canvas.line(b, c, width, color, 1.0);
}

fn arrow(canvas: &mut Canvas, from: Vec2, to: Vec2, color: [u8; 3]) {
    let along = to - from;
    let length = along.length().max(1e-6);
    let unit = along / length;
    let side = Vec2::new(-unit.y, unit.x);
    canvas.line(from, to - unit * 4.0, 2.5, color, 1.0);
    canvas.triangle([to + unit * 3.0, to - unit * 7.0 + side * 6.0, to - unit * 7.0 - side * 6.0], color, 1.0);
}

fn pointer(canvas: &mut Canvas, tip: Vec2) {
    let shape = |grow: f64| {
        let p = |x: f64, y: f64| tip + Vec2::new(x, y) * grow;
        [
            [p(0.0, 0.0), p(0.0, 17.0), p(4.6, 12.6)],
            [p(0.0, 0.0), p(4.6, 12.6), p(12.0, 12.0)],
        ]
    };
    for corners in shape(1.18) {
        canvas.triangle(corners.map(|c| c - Vec2::new(0.9, 1.4)), BLACK, 0.85);
    }
    for corners in shape(1.0) {
        canvas.triangle(corners, WHITE, 1.0);
    }
}

fn mark(canvas: &mut Canvas, center: Vec2, radius: f64, accent: [u8; 3]) {
    canvas.glow(center, radius * 2.4, accent, 0.35);
    canvas.shadow(Rect::from_center(center + Vec2::new(0.0, 6.0), Vec2::splat(radius * 2.0)), radius, 14.0, 0.35);
    let violet = [175, 82, 222];
    canvas.shade(Rect::from_center(center, Vec2::splat(radius * 2.0)), radius, lift(accent, 0.35), violet, 1.0);
    for (at, alpha) in [(-0.32, 0.95), (0.0, 0.7), (0.32, 0.45)] {
        let middle = center + Vec2::new(radius * at, radius * at * -0.25);
        let card = Rect::from_center(middle, Vec2::new(radius * 0.62, radius * 0.46));
        canvas.shadow(card, 5.0, 4.0, 0.2);
        canvas.round_rect(card, radius * 0.1, WHITE, alpha);
    }
    canvas.glow(center - Vec2::new(radius * 0.35, radius * 0.45), radius * 0.7, WHITE, 0.35);
}

fn fit(font: &Font, text: &str, room: f64) -> String {
    if font.width_of(text) <= room {
        return text.to_string();
    }
    let mut out = String::new();
    for c in text.chars() {
        if font.width_of(&format!("{out}{c}…")) > room {
            break;
        }
        out.push(c);
    }
    format!("{}…", out.trim_end())
}

pub fn icon(side: u32) -> Image {
    let size = f64::from(side.max(16));
    let mut canvas = Canvas::new(side.max(16), side.max(16));
    mark(&mut canvas, Vec2::splat(size / 2.0), size * 0.42, [53, 132, 228]);
    canvas.image
}

pub fn rounded(image: &mut Image, radius: f64) {
    let (w, h) = (f64::from(image.width), f64::from(image.height));
    for y in 0..image.height {
        for x in 0..image.width {
            let p = Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            let cx = p.x.clamp(radius, w - radius);
            let cy = p.y.clamp(radius, h - radius);
            let corner = (p.x < radius || p.x > w - radius) && (p.y < radius || p.y > h - radius);
            if !corner {
                continue;
            }
            let cover = (radius - (p - Vec2::new(cx, cy)).length() + 0.5).clamp(0.0, 1.0);
            let at = ((y * image.width + x) * 4) as usize;
            for c in 0..4 {
                image.rgba[at + c] = (f64::from(image.rgba[at + c]) * cover).round() as u8;
            }
        }
    }
}

pub fn thumb(path: &Path, hour: f64, motion: f64) -> Image {
    let (w, h) = (dense(THUMB.x), dense(THUMB.y));
    let mut image = crate::shader::still(path, w, h, motion, hour);
    rounded(&mut image, 12.0 * crate::paint::density());
    image
}

pub fn day(path: &Path) -> Vec<Image> {
    (0..FRAMES)
        .map(|at| {
            let hour = at as f64 * 24.0 / FRAMES as f64;
            thumb(path, hour, 30.0 + at as f64 * 0.6)
        })
        .collect()
}

pub fn frame_at(seconds: f64) -> (usize, usize, f64) {
    let spot = (seconds / CYCLE).fract() * FRAMES as f64;
    let at = spot.floor() as usize % FRAMES;
    (at, (at + 1) % FRAMES, spot.fract())
}

pub fn suggest(pixels: f64, millimeters: f64, scales: &[f64]) -> Option<f64> {
    if millimeters <= 0.0 || scales.is_empty() {
        return None;
    }
    let dpi = pixels / (millimeters / 25.4);
    let wanted = dpi / 110.0;
    scales
        .iter()
        .copied()
        .min_by(|a, b| (a - wanted).abs().total_cmp(&(b - wanted).abs()))
}

pub fn scales_for(pixels: Vec2) -> Vec<f64> {
    [1.0, 1.25, 1.5, 1.75, 2.0]
        .into_iter()
        .filter(|scale| *scale == 1.0 || (pixels.x / scale >= 1024.0 && pixels.y / scale >= 640.0))
        .collect()
}

pub fn greeting(hour: u32, name: &str) -> String {
    let part = match hour {
        5..=11 => "Good morning",
        12..=17 => "Good afternoon",
        _ => "Good evening",
    };
    match name.trim().is_empty() {
        true => format!("{part}. Let's make it yours."),
        false => format!("{part}, {}. Let's make it yours.", name.trim()),
    }
}

pub fn first_name(gecos: &str) -> String {
    gecos
        .split(',')
        .next()
        .unwrap_or("")
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fonts() -> Fonts {
        Fonts::load(None, false).unwrap()
    }

    fn full(card: Card) -> View {
        let mut view = View::new(card);
        view.greeting = greeting(20, "Dana");
        view.papers = (0..8)
            .map(|at| Paper {
                name: format!("Paper {at}"),
                still: None,
                chosen: at == 2,
            })
            .collect();
        view.scales = vec![1.0, 1.25, 1.5, 2.0];
        view.scale = 1.25;
        view.suggested = Some(2.0);
        view.tiles = Role::ALL
            .iter()
            .map(|role| Tile {
                role: *role,
                name: Some("Thing".into()),
                icon: None,
                busy: false,
            })
            .collect();
        view
    }

    #[test]
    fn every_card_paints_the_same_at_density_two() {
        let look = Look::themed(&crate::theme::DARK, [53, 132, 228]);
        let fonts = fonts();
        for card in Card::ALL {
            let view = full(card);
            crate::paint::same_at_two(&format!("welcome {card:?}"), || look.paint(&view, &fonts, 0.8));
        }
    }

    #[test]
    fn every_spot_sits_inside_the_card_and_none_overlap() {
        let look = Look::themed(&crate::theme::DARK, [53, 132, 228]);
        let fonts = fonts();
        let whole = Rect::new(Vec2::ZERO, SIZE);
        for card in Card::ALL {
            let mut views = vec![full(card)];
            if card == Card::Apps {
                let mut choosing = full(card);
                let choices = (0..7)
                    .map(|at| Choice {
                        name: format!("App {at}"),
                        icon: None,
                        chosen: at == 0,
                        installed: at < 5,
                        busy: false,
                    })
                    .collect();
                choosing.choosing = Some((Role::Browser, choices));
                views.push(choosing);
            }
            for view in views {
                let spots = look.spots(&view, &fonts);
                for (hit, rect) in &spots {
                    assert!(
                        rect.origin.x >= 0.0 && rect.origin.y >= 0.0 && rect.max().x <= whole.size.x && rect.max().y <= whole.size.y,
                        "{card:?} {hit:?} {rect:?} leaves the card"
                    );
                }
                for (at, (one, a)) in spots.iter().enumerate() {
                    for (two, b) in spots.iter().skip(at + 1) {
                        let apart = a.max().x <= b.origin.x
                            || b.max().x <= a.origin.x
                            || a.max().y <= b.origin.y
                            || b.max().y <= a.origin.y;
                        assert!(apart, "{card:?}: {one:?} {a:?} overlaps {two:?} {b:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn the_pointer_finds_what_it_is_over() {
        let look = Look::themed(&crate::theme::DARK, [53, 132, 228]);
        let fonts = fonts();
        let view = full(Card::Look);
        let paper = look.paper_rect(5).center();
        assert_eq!(look.hit(&view, &fonts, paper), Some(Hit::Paper(5)));
        assert_eq!(look.hit(&view, &fonts, look.next_rect().center()), Some(Hit::Next));
        assert_eq!(look.hit(&view, &fonts, Vec2::new(5.0, 5.0)), None);
        let hello = full(Card::Hello);
        assert_eq!(look.hit(&hello, &fonts, look.back_rect().center()), None, "the first card has no way back");
    }

    #[test]
    fn the_livepaper_sentence_is_one_plain_sentence_that_fits() {
        assert_eq!(LIVEPAPER.matches('.').count(), 1);
        assert!(!LIVEPAPER.contains('\u{2014}') && !LIVEPAPER.contains('\u{2013}'));
        let fonts = fonts();
        assert!(fonts.small.width_of(LIVEPAPER) < SIZE.x - PAD * 2.0);
    }

    #[test]
    fn a_whole_day_plays_in_under_three_seconds() {
        assert!(CYCLE < 3.0);
        assert_eq!(frame_at(0.0).0, 0);
        assert_eq!(frame_at(CYCLE / 2.0).0, FRAMES / 2);
        assert_eq!(frame_at(CYCLE).0, 0, "the day loops");
        let (at, next, blend) = frame_at(CYCLE * (FRAMES as f64 - 0.5) / FRAMES as f64);
        assert_eq!((at, next), (FRAMES - 1, 0));
        assert!((blend - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_suggested_size_follows_the_pixel_density() {
        let scales = scales_for(Vec2::new(2400.0, 1600.0));
        assert_eq!(scales, [1.0, 1.25, 1.5, 1.75, 2.0]);
        assert_eq!(suggest(2400.0, 259.0, &scales), Some(2.0), "a Pixelbook");
        assert_eq!(suggest(1920.0, 527.0, &scales), Some(1.0), "a 24 inch 1080p screen");
        assert_eq!(scales_for(Vec2::new(1366.0, 768.0)), [1.0]);
        assert_eq!(suggest(1920.0, 0.0, &scales), None);
    }

    #[test]
    fn motion_maps_both_ways() {
        for motion in Motion::ALL {
            assert_eq!(Motion::of(motion.reduce(), motion.percent()), motion);
        }
        assert_eq!(Motion::of(false, 250), Motion::Full);
    }

    #[test]
    fn the_greeting_uses_the_first_name_and_the_time() {
        assert_eq!(first_name("Dana Ruiz,,,"), "Dana");
        assert_eq!(greeting(9, "Dana"), "Good morning, Dana. Let's make it yours.");
        assert_eq!(greeting(23, ""), "Good evening. Let's make it yours.");
    }

    #[test]
    fn the_cards_run_in_order() {
        assert_eq!(Card::Hello.back(), None);
        assert_eq!(Card::Hello.next(), Some(Card::Look));
        assert_eq!(Card::Done.next(), None);
        assert_eq!(Card::Apps.back(), Some(Card::Keys));
    }
}
