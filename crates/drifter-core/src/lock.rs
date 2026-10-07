use zeroize::{Zeroize, Zeroizing};

use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::text::Font;

pub const MAX_LEN: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Note {
    Ask,
    Checking,
    Wrong,
}

impl Note {
    pub fn text(self) -> &'static str {
        match self {
            Note::Ask => "Enter Password",
            Note::Checking => "Checking...",
            Note::Wrong => "Wrong password",
        }
    }
}

pub struct Pad {
    chars: Vec<char>,
}

impl Default for Pad {
    fn default() -> Self {
        Pad::new()
    }
}

impl Pad {
    pub fn new() -> Pad {
        Pad {
            chars: Vec::with_capacity(MAX_LEN),
        }
    }

    pub fn push(&mut self, letter: char) -> bool {
        if letter.is_control() || self.chars.len() >= MAX_LEN {
            return false;
        }
        self.chars.push(letter);
        true
    }

    pub fn backspace(&mut self) {
        if let Some(last) = self.chars.last_mut() {
            *last = '\0';
        }
        self.chars.pop();
    }

    pub fn wipe(&mut self) {
        self.chars.zeroize();
    }

    pub fn len(&self) -> usize {
        self.chars.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chars.is_empty()
    }

    pub fn take(&mut self) -> Zeroizing<Vec<u8>> {
        let mut bytes = Zeroizing::new(Vec::with_capacity(MAX_LEN * 4 + 1));
        let mut scratch = [0u8; 4];
        for letter in &self.chars {
            bytes.extend_from_slice(letter.encode_utf8(&mut scratch).as_bytes());
        }
        scratch.zeroize();
        bytes.push(0);
        self.wipe();
        bytes
    }
}

impl Drop for Pad {
    fn drop(&mut self) {
        self.wipe();
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub size: Vec2,
    pub back: [u8; 3],
    pub field: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub alert: [u8; 3],
    pub pill: f64,
    pub veil: f64,
    pub logo: f64,
    pub dot: f64,
    pub step: f64,
}

impl Look {
    pub fn themed(p: &crate::theme::Palette) -> Look {
        Look {
            size: Vec2::new(300.0, 108.0),
            back: p.back,
            field: p.raised,
            ink: p.ink,
            dim: p.dim,
            alert: p.alert,
            pill: 0.55,
            veil: 0.32,
            logo: 104.0,
            dot: 10.0,
            step: 16.0,
        }
    }
}

impl Default for Look {
    fn default() -> Self {
        Look::themed(&crate::theme::DARK)
    }
}

impl Look {
    pub fn field_rect(&self) -> Rect {
        let side = 30.0;
        Rect::new(Vec2::new(side, 34.0), Vec2::new(self.size.x - side * 2.0, 36.0))
    }

    pub fn shown(&self) -> usize {
        ((self.field_rect().size.x - 32.0) / self.step).floor().max(1.0) as usize
    }

    pub fn dot_center(&self, slot: usize, count: usize) -> Vec2 {
        let field = self.field_rect();
        let width = self.step * count as f64;
        let left = field.origin.x + (field.size.x - width) / 2.0;
        Vec2::new(left + self.step * slot as f64 + self.step / 2.0, field.center().y)
    }

    pub fn logo_rect(&self, screen: Vec2) -> Rect {
        let center = Vec2::new(screen.x / 2.0, (screen.y * 0.60).max(self.logo));
        Rect::new((center - Vec2::splat(self.logo / 2.0)).round(), Vec2::splat(self.logo))
    }

    pub fn rect(&self, screen: Vec2) -> Rect {
        let logo = self.logo_rect(screen);
        Rect::new(
            Vec2::new((screen.x - self.size.x) / 2.0, logo.max().y + 18.0).round(),
            self.size,
        )
    }

    pub fn top_rect(&self, screen: Vec2, size: Vec2) -> Rect {
        Rect::new(
            Vec2::new((screen.x - size.x) / 2.0, screen.y * 0.09).round(),
            size,
        )
    }
}

pub struct Words<'a> {
    pub clock: &'a str,
    pub date: &'a str,
}

pub fn paint_top(look: &Look, big: Option<&Font>, small: Option<&Font>, words: &Words) -> Image {
    let date = small.map(|font| font.paint_bold(words.date, look.ink));
    let clock = big.map(|font| font.paint_bold(words.clock, look.ink));
    let sizes: Vec<Vec2> = [&date, &clock]
        .into_iter()
        .flatten()
        .map(crate::paint::logical)
        .collect();
    let width = sizes.iter().map(|size| size.x).fold(1.0, f64::max).ceil() + 8.0;
    let height = sizes.iter().map(|size| size.y).sum::<f64>().ceil() + 4.0;
    let mut canvas = crate::paint::Canvas::new(width as u32, height as u32);
    let mut y = 0.0;
    for piece in [date, clock].into_iter().flatten() {
        let size = crate::paint::logical(&piece);
        canvas.stamp(&piece, Vec2::new((width - size.x) / 2.0, y).round(), 1.0);
        y += size.y;
    }
    canvas.image
}

pub fn paint(look: &Look, small: Option<&Font>, name: &str, dots: usize, note: Note) -> Image {
    let mut canvas = crate::paint::Canvas::new(look.size.x.round() as u32, look.size.y.round() as u32);
    let field = look.field_rect();
    canvas.round_rect(field, field.size.y / 2.0, look.back, look.pill);

    let count = dots.min(look.shown());
    for slot in 0..count {
        canvas.circle(look.dot_center(slot, count), look.dot / 2.0, look.ink, 1.0);
    }

    let Some(font) = small else {
        return canvas.image;
    };
    let centered = |piece: &Image, y: f64| Vec2::new((look.size.x - crate::paint::logical(piece).x) / 2.0, y).round();
    let named = font.paint_bold(name, look.ink);
    canvas.stamp(&named, centered(&named, 4.0), 1.0);
    if count == 0 && note == Note::Ask {
        let hint = font.paint(note.text(), look.dim);
        let y = field.center().y - crate::paint::logical(&hint).y / 2.0;
        canvas.stamp(&hint, centered(&hint, y), 1.0);
    }
    if note != Note::Ask {
        let color = match note {
            Note::Wrong => look.alert,
            _ => look.dim,
        };
        let line = font.paint(note.text(), color);
        canvas.stamp(&line, centered(&line, field.max().y + 10.0), 1.0);
    }
    canvas.image
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let at = ((y as usize) * (image.width as usize) + x as usize) * 4;
        [image.rgba[at], image.rgba[at + 1], image.rgba[at + 2], image.rgba[at + 3]]
    }

    fn words() -> Words<'static> {
        Words {
            clock: "12:00",
            date: "Monday, September 28",
        }
    }

    fn panel(dots: usize, note: Note) -> Image {
        paint(&Look::default(), None, "Ada", dots, note)
    }

    #[test]
    fn the_lock_at_density_two_matches_density_one() {
        let look = Look::default();
        let big = Font::find(None, 96.0).ok();
        let small = Font::find(None, 17.0).ok();
        crate::paint::same_at_two("lock", || paint(&look, small.as_ref(), "Ada Lovelace", 5, Note::Wrong));
        crate::paint::same_at_two("lock top", || paint_top(&look, big.as_ref(), small.as_ref(), &words()));
    }

    #[test]
    fn a_typed_letter_is_kept_and_a_control_key_is_refused() {
        let mut pad = Pad::new();
        assert!(pad.push('a'));
        assert!(!pad.push('\n'));
        assert!(!pad.push('\0'));
        assert!(!pad.push('\u{7f}'));
        assert_eq!(pad.len(), 1);
    }

    #[test]
    fn backspace_drops_one_letter_and_never_underflows() {
        let mut pad = Pad::new();
        pad.push('a');
        pad.push('b');
        pad.backspace();
        assert_eq!(pad.len(), 1);
        pad.backspace();
        pad.backspace();
        assert!(pad.is_empty());
    }

    #[test]
    fn the_pad_stops_taking_letters_at_the_limit() {
        let mut pad = Pad::new();
        for _ in 0..MAX_LEN {
            assert!(pad.push('x'));
        }
        assert!(!pad.push('x'));
        assert_eq!(pad.len(), MAX_LEN);
    }

    #[test]
    fn take_gives_utf8_with_one_nul_and_empties_the_pad() {
        let mut pad = Pad::new();
        for letter in "pässwörd".chars() {
            pad.push(letter);
        }
        let bytes = pad.take();
        assert_eq!(&bytes[..], "pässwörd\0".as_bytes());
        assert!(pad.is_empty());
    }

    #[test]
    fn a_wiped_pad_keeps_its_room_so_nothing_is_copied_on_growth() {
        let mut pad = Pad::new();
        let room = pad.chars.capacity();
        for _ in 0..MAX_LEN {
            pad.push('x');
        }
        assert_eq!(pad.chars.capacity(), room);
        pad.wipe();
        assert_eq!(pad.chars.capacity(), room);
    }

    #[test]
    fn the_field_is_see_through_so_the_wallpaper_shows() {
        let look = Look::default();
        let image = panel(0, Note::Ask);
        let middle = look.field_rect().center();
        let alpha = pixel(&image, middle.x as u32, middle.y as u32)[3];
        assert!(alpha > 0 && alpha < 255);
        assert_eq!(pixel(&image, 0, 0)[3], 0);
    }

    #[test]
    fn each_typed_letter_paints_one_dot_in_the_field() {
        let look = Look::default();
        for count in [1usize, 3, 7] {
            let image = panel(count, Note::Ask);
            for slot in 0..count {
                let at = look.dot_center(slot, count);
                let seen = pixel(&image, at.x as u32, at.y as u32);
                assert_eq!(&seen[..3], &look.ink, "dot {slot} of {count}");
            }
        }
    }

    #[test]
    fn the_dots_are_centered_in_the_field() {
        let look = Look::default();
        let first = look.dot_center(0, 4);
        let last = look.dot_center(3, 4);
        let middle = (first.x + last.x) / 2.0;
        assert!((middle - look.field_rect().center().x).abs() < 0.01);
    }

    #[test]
    fn a_very_long_password_never_paints_past_the_field() {
        let look = Look::default();
        let shown = look.shown();
        let last = look.dot_center(shown - 1, shown);
        assert!(last.x + look.dot / 2.0 < look.field_rect().max().x);
    }

    #[test]
    fn the_logo_sits_above_the_field_and_the_clock_near_the_top() {
        let look = Look::default();
        let screen = Vec2::new(1920.0, 1080.0);
        let logo = look.logo_rect(screen);
        let card = look.rect(screen);
        let top = look.top_rect(screen, Vec2::new(400.0, 140.0));
        assert_eq!(logo.center().x, 960.0);
        assert_eq!(card.center().x, 960.0);
        assert!(logo.max().y < card.origin.y);
        assert!(top.max().y < logo.origin.y);
        assert!(card.max().y < screen.y);
    }

    #[test]
    fn the_layout_fits_a_small_screen() {
        let look = Look::default();
        let screen = Vec2::new(1024.0, 600.0);
        assert!(look.rect(screen).max().y < screen.y);
    }

    #[test]
    fn every_note_has_its_own_words() {
        assert_ne!(Note::Ask.text(), Note::Wrong.text());
        assert_ne!(Note::Ask.text(), Note::Checking.text());
        assert_ne!(Note::Checking.text(), Note::Wrong.text());
    }

    #[test]
    fn a_wrong_password_is_written_in_the_alert_color() {
        let Some(font) = Font::find(None, 17.0).ok() else { return };
        let look = Look::default();
        let plain = paint(&look, Some(&font), "Ada", 0, Note::Ask);
        let wrong = paint(&look, Some(&font), "Ada", 0, Note::Wrong);
        assert_ne!(plain.rgba, wrong.rgba);
    }

    #[test]
    fn the_light_palette_writes_dark_words() {
        let dark = Look::themed(&crate::theme::DARK);
        let light = Look::themed(&crate::theme::LIGHT);
        assert_ne!(dark.ink, light.ink);
        assert_ne!(dark.back, light.back);
    }
}
