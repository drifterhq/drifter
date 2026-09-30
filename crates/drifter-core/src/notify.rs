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

pub fn expire_ms(asked: i32, critical: bool) -> Option<u32> {
    match (asked, critical) {
        (_, true) => None,
        (0, _) => None,
        (ms, _) if ms > 0 => Some(ms as u32),
        _ => Some(DEFAULT_MS),
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

impl Default for Look {
    fn default() -> Self {
        Look {
            width: 360.0,
            pad: 14.0,
            icon: 40.0,
            gap: 10.0,
            margin: 16.0,
            radius: 14.0,
            back: [37, 40, 50],
            back_alpha: 0.96,
            ink: [236, 238, 244],
            dim: [146, 152, 166],
            blank: [72, 77, 94],
        }
    }
}

impl Look {
    fn text_x(&self) -> f64 {
        self.pad * 2.0 + self.icon
    }

    fn text_width(&self) -> f64 {
        self.width - self.text_x() - self.pad
    }

    pub fn height(&self, note: &Note, font: &Font) -> f64 {
        let lines = wrap(font, &note.body, self.text_width(), 3).len() as f64;
        let line = font.line_height().ceil();
        let text = line * (2.0 + lines) + self.pad * 2.0;
        text.max(self.icon + self.pad * 2.0).ceil()
    }

    pub fn stack(&self, screen: Vec2, heights: &[f64]) -> Vec<Rect> {
        let mut y = self.margin;
        heights
            .iter()
            .map(|tall| {
                let rect = Rect::new(
                    Vec2::new(screen.x - self.margin - self.width, y),
                    Vec2::new(self.width, *tall),
                );
                y += tall + self.gap;
                rect
            })
            .collect()
    }

    pub fn paint(&self, note: &Note, font: &Font) -> Image {
        let size = Vec2::new(self.width, self.height(note, font));
        let mut canvas = Canvas::new(size.x as u32, size.y as u32);
        canvas.round_rect(
            Rect::new(Vec2::ZERO, size),
            self.radius,
            self.back,
            self.back_alpha,
        );
        let spot = Rect::new(Vec2::splat(self.pad), Vec2::splat(self.icon));
        match &note.icon {
            Some(image) => {
                let fitted = image.scaled(self.icon as u32, self.icon as u32);
                canvas.stamp(&fitted, spot.origin, 1.0);
            }
            None => {
                canvas.round_rect(spot, self.icon * 0.25, self.blank, 1.0);
                let name = if note.app.is_empty() {
                    &note.summary
                } else {
                    &note.app
                };
                let first: String = name.chars().take(1).collect::<String>().to_uppercase();
                let letter = font.paint(&first, self.ink);
                canvas.stamp(&letter, spot.center() - letter.size() / 2.0, 1.0);
            }
        }
        let x = self.text_x();
        let line = font.line_height().ceil();
        let mut y = self.pad;
        for (text, ink) in [(&note.app, self.dim), (&note.summary, self.ink)] {
            let painted = font.paint(&cut(font, text, self.text_width()), ink);
            canvas.stamp(&painted, Vec2::new(x, y), 1.0);
            y += line;
        }
        for body in wrap(font, &note.body, self.text_width(), 3) {
            let painted = font.paint(&body, self.dim);
            canvas.stamp(&painted, Vec2::new(x, y), 1.0);
            y += line;
        }
        canvas.image
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
            icon: None,
        }
    }

    #[test]
    fn the_timeout_follows_the_spec() {
        assert_eq!(expire_ms(-1, false), Some(DEFAULT_MS));
        assert_eq!(expire_ms(0, false), None);
        assert_eq!(expire_ms(2500, false), Some(2500));
        assert_eq!(expire_ms(2500, true), None);
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
        let rects = look.stack(Vec2::new(1920.0, 1080.0), &[70.0, 90.0]);
        assert_eq!(rects[0].max().x, 1920.0 - look.margin);
        assert_eq!(rects[0].origin.y, look.margin);
        assert_eq!(rects[1].origin.y, rects[0].max().y + look.gap);
    }

    #[test]
    fn a_card_grows_with_its_body() {
        let look = Look::default();
        let font = font();
        let short = look.height(&note("hi"), &font);
        let long = look.height(&note(&"word ".repeat(80)), &font);
        assert!(long > short);
        let image = look.paint(&note(&"word ".repeat(80)), &font);
        assert_eq!(image.height as f64, long);
    }
}
