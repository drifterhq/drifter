use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

#[derive(Clone)]
pub struct Note {
    pub id: u32,
    pub app: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<(String, String)>,
    pub expire: Option<u32>,
    pub asked: i32,
    pub critical: bool,
    pub icon: Option<Image>,
}

impl Note {
    pub fn default_action(&self) -> Option<&str> {
        self.actions
            .iter()
            .find(|(key, _)| key == "default")
            .map(|(key, _)| key.as_str())
    }
}

pub const DEFAULT_MS: u32 = 5000;
pub const MOST: usize = 5;

pub fn expire_ms(asked: i32, critical: bool, usual: Option<u32>) -> Option<u32> {
    match (asked, critical) {
        (_, true) => None,
        (0, _) => None,
        (ms, _) if ms > 0 => Some(ms as u32),
        _ => usual,
    }
}

pub const KEPT: usize = 50;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recent {
    pub when: u64,
    pub app: String,
    pub summary: String,
    pub body: String,
}

fn flat(text: &str) -> String {
    text.chars()
        .map(|c| {
            if c == '\t' || c == '\n' || c == '\r' {
                ' '
            } else {
                c
            }
        })
        .collect()
}

pub fn remember(history: &str, note: &Recent) -> String {
    let line = format!(
        "{}\t{}\t{}\t{}",
        note.when,
        flat(&note.app),
        flat(&note.summary),
        flat(&note.body)
    );
    let mut lines: Vec<&str> = history.lines().filter(|line| !line.is_empty()).collect();
    lines.push(&line);
    let skip = lines.len().saturating_sub(KEPT);
    let mut out = lines[skip..].join("\n");
    out.push('\n');
    out
}

pub fn recall(history: &str) -> Vec<Recent> {
    history
        .lines()
        .rev()
        .filter_map(|line| {
            let mut parts = line.splitn(4, '\t');
            Some(Recent {
                when: parts.next()?.parse().ok()?,
                app: parts.next()?.to_string(),
                summary: parts.next()?.to_string(),
                body: parts.next().unwrap_or("").to_string(),
            })
        })
        .collect()
}

pub fn ago(when: u64, now: u64) -> String {
    let gone = now.saturating_sub(when);
    match gone {
        0..60 => crate::lang::t("Just now").to_string(),
        60..3600 => crate::lang::tf("{} min ago", &[&(gone / 60)]),
        3600..86400 => format!("{} h ago", gone / 3600),
        _ => crate::lang::tf("{} days ago", &[&(gone / 86400)]),
    }
}

pub fn actions(flat: &[String]) -> Vec<(String, String)> {
    flat.as_chunks::<2>()
        .0
        .iter()
        .map(|[key, label]| (key.clone(), label.clone()))
        .collect()
}

pub fn from_pixels(
    width: i32,
    height: i32,
    stride: i32,
    alpha: bool,
    channels: i32,
    data: &[u8],
) -> Option<Image> {
    if width <= 0 || height <= 0 || !(3..=4).contains(&channels) {
        return None;
    }
    let (width, height, stride, channels) = (
        width as usize,
        height as usize,
        stride as usize,
        channels as usize,
    );
    if stride < width * channels || data.len() < stride * (height - 1) + width * channels {
        return None;
    }
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let at = y * stride + x * channels;
            let a = if alpha && channels == 4 {
                data[at + 3]
            } else {
                255
            };
            rgba.extend_from_slice(&[data[at], data[at + 1], data[at + 2], a]);
        }
    }
    let mut image = Image {
        width: width as u32,
        height: height as u32,
        rgba,
    };
    image.premultiply();
    Some(image)
}

#[derive(Clone, Copy)]
pub struct Type<'a> {
    pub head: &'a Font,
    pub body: &'a Font,
    pub meta: &'a Font,
}

impl<'a> Type<'a> {
    pub fn one(font: &'a Font) -> Type<'a> {
        Type { head: font, body: font, meta: font }
    }
}

pub fn wrap(font: &Font, text: &str, width: f64, lines: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for paragraph in text.lines() {
        let mut line = String::new();
        for word in paragraph
            .split_whitespace()
            .flat_map(|word| pieces(font, word, width))
        {
            let tried = match line.is_empty() {
                true => word.to_string(),
                false => format!("{line} {word}"),
            };
            if font.width_of(&tried) <= width || line.is_empty() {
                line = tried;
            } else {
                out.push(std::mem::take(&mut line));
                line = word.to_string();
            }
        }
        if !line.is_empty() {
            out.push(line);
        }
    }
    if out.len() > lines {
        out.truncate(lines);
        if let Some(last) = out.last_mut() {
            last.push('…');
        }
    }
    out.into_iter()
        .map(|line| cut(font, &line, width))
        .collect()
}

fn pieces(font: &Font, word: &str, width: f64) -> Vec<String> {
    let mut out = Vec::new();
    let mut piece = String::new();
    for glyph in word.chars() {
        piece.push(glyph);
        if font.width_of(&piece) > width && piece.chars().count() > 1 {
            piece.pop();
            out.push(std::mem::take(&mut piece));
            piece.push(glyph);
        }
    }
    if !piece.is_empty() {
        out.push(piece);
    }
    out
}

fn cut(font: &Font, text: &str, width: f64) -> String {
    if font.width_of(text) <= width {
        return text.to_string();
    }
    let mut out = String::new();
    for glyph in text.chars() {
        if font.width_of(&format!("{out}{glyph}…")) > width {
            break;
        }
        out.push(glyph);
    }
    format!("{out}…")
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub width: f64,
    pub pad: f64,
    pub icon: f64,
    pub gap: f64,
    pub margin: f64,
    pub radius: f64,
    pub back: [u8; 3],
    pub back_alpha: f64,
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub blank: [u8; 3],
}

impl Look {
    pub fn themed(p: &crate::theme::Palette) -> Look {
        Look {
            width: 360.0,
            pad: 14.0,
            icon: 40.0,
            gap: 10.0,
            margin: 16.0,
            radius: 14.0,
            back: p.surface,
            back_alpha: 0.96,
            ink: p.ink,
            dim: p.dim,
            blank: p.off,
        }
    }
}

impl Default for Look {
    fn default() -> Self {
        Look::themed(&crate::theme::DARK)
    }
}

impl Look {
    fn text_x(&self) -> f64 {
        self.pad * 2.0 + self.icon
    }

    fn text_width(&self) -> f64 {
        self.width - self.text_x() - self.pad
    }

    pub fn height(&self, note: &Note, fonts: &Type) -> f64 {
        let lines = wrap(fonts.body, &note.body, self.text_width(), 3).len() as f64;
        let text = fonts.meta.line_height().ceil()
            + 3.0
            + fonts.head.line_height().ceil()
            + 2.0
            + fonts.body.line_height().ceil() * lines
            + self.pad * 2.0;
        text.max(self.icon + self.pad * 2.0).ceil()
    }

    pub fn stack(&self, screen: Vec2, heights: &[f64], position: crate::config::NotePosition, floor: f64) -> Vec<Rect> {
        let x = screen.x - self.margin - self.width;
        match position {
            crate::config::NotePosition::TopRight => {
                let mut y = self.margin;
                heights
                    .iter()
                    .map(|tall| {
                        let rect = Rect::new(Vec2::new(x, y), Vec2::new(self.width, *tall));
                        y += tall + self.gap;
                        rect
                    })
                    .collect()
            }
            crate::config::NotePosition::BottomRight => {
                let mut y = screen.y - self.margin - floor;
                heights
                    .iter()
                    .map(|tall| {
                        y -= tall;
                        let rect = Rect::new(Vec2::new(x, y), Vec2::new(self.width, *tall));
                        y -= self.gap;
                        rect
                    })
                    .collect()
            }
        }
    }

    pub fn paint(&self, note: &Note, fonts: &Type) -> Image {
        let size = Vec2::new(self.width, self.height(note, fonts));
        let mut canvas = Canvas::new(size.x as u32, size.y as u32);
        canvas.round_rect(Rect::new(Vec2::ZERO, size), self.radius, self.back, self.back_alpha);
        let spot = Rect::new(Vec2::splat(self.pad), Vec2::splat(self.icon));
        badge(&mut canvas, note.icon.as_ref(), &note.app, &note.summary, spot, self.blank, self.ink, fonts.head);
        let x = self.text_x();
        let room = self.text_width();
        let mut y = self.pad;
        let app = fonts.meta.paint(&cut(fonts.meta, &note.app, room), self.dim);
        canvas.stamp(&app, Vec2::new(x, y), 0.9);
        y += fonts.meta.line_height().ceil() + 3.0;
        let head = fonts.head.paint_bold(&cut(fonts.head, &note.summary, room), self.ink);
        canvas.stamp(&head, Vec2::new(x, y), 1.0);
        y += fonts.head.line_height().ceil() + 2.0;
        for body in wrap(fonts.body, &note.body, room, 3) {
            let painted = fonts.body.paint(&body, self.dim);
            canvas.stamp(&painted, Vec2::new(x, y), 1.0);
            y += fonts.body.line_height().ceil();
        }
        canvas.image
    }
}

#[allow(clippy::too_many_arguments)]
pub fn badge(canvas: &mut Canvas, icon: Option<&Image>, app: &str, summary: &str, spot: Rect, blank: [u8; 3], ink: [u8; 3], font: &Font) {
    match icon {
        Some(image) => {
            let side = crate::paint::dense(spot.size.x);
            canvas.stamp(&image.scaled(side, side), spot.origin, 1.0);
        }
        None => {
            canvas.round_rect(spot, spot.size.x * 0.25, blank, 1.0);
            let name = if app.is_empty() { summary } else { app };
            let first: String = name.chars().take(1).collect::<String>().to_uppercase();
            let letter = font.paint_bold(&first, ink);
            canvas.stamp(&letter, (spot.center() - crate::paint::logical(&letter) / 2.0).round(), 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Font {
        Font::find(None, 13.0).expect("a font")
    }

    fn note(body: &str) -> Note {
        Note {
            id: 1,
            app: "Mail".into(),
            summary: "New message".into(),
            body: body.into(),
            actions: Vec::new(),
            expire: Some(DEFAULT_MS),
            asked: -1,
            critical: false,
            icon: None,
        }
    }

    #[test]
    fn a_note_at_density_two_matches_density_one() {
        let font = font();
        let look = Look::default();
        let note = note("A longer body that wraps onto a second line for the check");
        crate::paint::same_at_two("note", || look.paint(&note, &Type::one(&font)));
    }

    #[test]
    fn the_timeout_follows_the_spec() {
        let usual = Some(DEFAULT_MS);
        assert_eq!(expire_ms(-1, false, usual), usual);
        assert_eq!(expire_ms(-1, false, None), None);
        assert_eq!(expire_ms(0, false, usual), None);
        assert_eq!(expire_ms(2500, false, usual), Some(2500));
        assert_eq!(expire_ms(2500, true, usual), None);
    }

    #[test]
    fn the_history_keeps_the_newest_fifty_on_one_line_each() {
        let mut text = String::new();
        for at in 0..60 {
            let note = Recent {
                when: at,
                app: "Mail".into(),
                summary: format!("note {at}"),
                body: "two\nlines\tand a tab".into(),
            };
            text = remember(&text, &note);
        }
        let back = recall(&text);
        assert_eq!(back.len(), KEPT);
        assert_eq!(back[0].summary, "note 59");
        assert_eq!(back[0].body, "two lines and a tab");
        assert_eq!(back.last().unwrap().when, 10);
        assert_eq!(ago(100, 100), "Just now");
        assert_eq!(ago(0, 7200), "2 h ago");
    }

    #[test]
    fn actions_come_in_key_and_label_pairs() {
        let flat: Vec<String> = ["default", "Open", "reply", "Reply", "odd"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let pairs = actions(&flat);
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[1], ("reply".into(), "Reply".into()));
        let mut with = note("");
        with.actions = pairs;
        assert_eq!(with.default_action(), Some("default"));
    }

    #[test]
    fn raw_pixels_become_a_premultiplied_image() {
        let data = [255, 0, 0, 128, 0, 255, 0, 255, 0, 0, 0, 0];
        let image = from_pixels(3, 1, 12, true, 4, &data).unwrap();
        assert_eq!(image.width, 3);
        assert_eq!(&image.rgba[0..4], &[128, 0, 0, 128]);
        let rgb = from_pixels(1, 1, 3, false, 3, &[10, 20, 30]).unwrap();
        assert_eq!(rgb.rgba, [10, 20, 30, 255]);
        assert!(from_pixels(4, 4, 4, true, 4, &data).is_none());
    }

    #[test]
    fn a_long_body_wraps_to_three_lines_at_most() {
        let font = font();
        let lines = wrap(&font, &"word ".repeat(200), 200.0, 3);
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|line| font.width_of(line) <= 200.0));
        assert!(lines[2].ends_with('…'));
        assert_eq!(wrap(&font, "short", 200.0, 3), ["short"]);
    }

    #[test]
    fn a_word_too_long_for_a_line_breaks_by_letter() {
        let font = font();
        let path = "/home/someone/Pictures/Screenshots/Screenshot-2026-09-29-16.22.57.png";
        let lines = wrap(&font, path, 180.0, 3);
        assert!(lines.len() > 1);
        assert_eq!(
            lines.concat().replace('…', ""),
            path[..lines.concat().replace('…', "").len()]
        );
        assert!(lines.iter().all(|line| font.width_of(line) <= 180.0));
    }

    #[test]
    fn cards_stack_down_from_the_top_right() {
        let look = Look::default();
        let rects = look.stack(Vec2::new(1920.0, 1080.0), &[70.0, 90.0], NotePosition::TopRight, 64.0);
        assert_eq!(rects[0].max().x, 1920.0 - look.margin);
        assert_eq!(rects[0].origin.y, look.margin);
        assert_eq!(rects[1].origin.y, rects[0].max().y + look.gap);
    }

    #[test]
    fn cards_stack_up_from_the_bottom_right() {
        let look = Look::default();
        let rects = look.stack(Vec2::new(1920.0, 1080.0), &[70.0, 90.0], NotePosition::BottomRight, 64.0);
        assert_eq!(rects[0].max().x, 1920.0 - look.margin);
        assert_eq!(rects[0].max().y, 1080.0 - look.margin - 64.0, "the newest sits above the dock");
        assert_eq!(rects[1].max().y, rects[0].origin.y - look.gap, "older notes stack up");
        assert_eq!(rects[1].size.y, 90.0);
    }

    use crate::config::NotePosition;

    #[test]
    fn a_card_grows_with_its_body() {
        let look = Look::default();
        let font = font();
        let short = look.height(&note("hi"), &Type::one(&font));
        let long = look.height(&note(&"word ".repeat(80)), &Type::one(&font));
        assert!(long > short);
        let image = look.paint(&note(&"word ".repeat(80)), &Type::one(&font));
        assert_eq!(image.height as f64, long);
    }
}
