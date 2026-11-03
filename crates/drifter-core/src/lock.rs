use zeroize::{Zeroize, Zeroizing};

use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::text::Font;

pub const MAX_LEN: usize = 256;
pub const TINT: f64 = 0.45;
const DARK_FILL: f32 = 0.8;
pub const BLINK: std::time::Duration = std::time::Duration::from_millis(530);
const INDENT: f64 = 26.0;
const SHADE: f64 = 28.0;
const LIFT: f64 = 6.0;
pub const FULL: [u8; 3] = [52, 199, 89];
const CORNER: f64 = 28.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Note {
    Ask,
    Checking,
    Wrong,
}

impl Note {
    pub fn text(self) -> &'static str {
        match self {
            Note::Ask => crate::lang::t("Enter Password"),
            Note::Checking => crate::lang::t("Checking..."),
            Note::Wrong => crate::lang::t("Wrong password"),
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
            size: Vec2::new(320.0, 128.0),
            back: p.back,
            field: p.raised,
            ink: p.ink,
            dim: p.dim,
            alert: p.alert,
            pill: 0.55,
            veil: 0.32,
            logo: 200.0,
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
    pub fn over(self, behind: [f64; 3]) -> Look {
        let mixed: Vec<f64> = (0..3)
            .map(|c| behind[c] * (1.0 - self.veil) + f64::from(self.back[c]) * self.veil)
            .collect();
        let light = 0.2126 * mixed[0] + 0.7152 * mixed[1] + 0.0722 * mixed[2];
        match light > 150.0 {
            true => Look {
                ink: [22, 22, 26],
                dim: [70, 70, 76],
                back: [246, 246, 248],
                ..self
            },
            false => Look {
                ink: [255, 255, 255],
                dim: [228, 228, 234],
                back: [36, 36, 42],
                ..self
            },
        }
    }

    pub fn dark_ink(&self) -> bool {
        self.ink.iter().map(|c| u32::from(*c)).sum::<u32>() < 384
    }

    pub fn field_rect(&self) -> Rect {
        let side = 40.0;
        Rect::new(Vec2::new(side, 54.0), Vec2::new(self.size.x - side * 2.0, 36.0))
    }

    pub fn shown(&self) -> usize {
        ((self.field_rect().size.x - 32.0) / self.step).floor().max(1.0) as usize
    }

    pub fn dot_center(&self, slot: usize, _count: usize) -> Vec2 {
        let field = self.field_rect();
        Vec2::new(field.origin.x + INDENT + self.step * slot as f64 + self.step / 2.0, field.center().y)
    }

    pub fn caret_rect(&self, dots: usize) -> Rect {
        let field = self.field_rect();
        let count = dots.min(self.shown());
        let x = field.origin.x + INDENT + self.step * count as f64 + 1.0;
        Rect::new(Vec2::new(x, field.center().y - 9.0), Vec2::new(2.0, 18.0))
    }

    pub fn logo_rect(&self, screen: Vec2) -> Rect {
        let room = screen.y - self.logo / 2.0 - 6.0 - self.size.y - 24.0;
        let center = Vec2::new(screen.x / 2.0, (screen.y * 0.78).min(room).max(self.logo));
        Rect::new((center - Vec2::splat(self.logo / 2.0)).round(), Vec2::splat(self.logo))
    }

    pub fn rect(&self, screen: Vec2) -> Rect {
        let logo = self.logo_rect(screen);
        Rect::new(
            Vec2::new((screen.x - self.size.x) / 2.0, logo.max().y + 6.0).round(),
            self.size,
        )
    }

    pub fn logo_shadow_rect(&self, screen: Vec2) -> Rect {
        let logo = self.logo_rect(screen);
        Rect::new(logo.origin - Vec2::splat(SHADE), logo.size + Vec2::splat(SHADE * 2.0))
    }

    pub fn clock_rect(&self, screen: Vec2, clock: Vec2) -> Rect {
        Rect::new(
            Vec2::new((screen.x - clock.x) / 2.0, screen.y * 0.09 + 36.0 - clock.y * 0.12).round(),
            clock,
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

pub fn paint_top(look: &Look, font: Option<&Font>, date: &str) -> Image {
    match font {
        Some(font) => soft_shadow(&font.paint_weight(date, look.ink, 540.0), look.dark_ink()),
        None => crate::paint::Canvas::new(1, 1).image,
    }
}

pub fn soft_shadow(piece: &Image, dark_ink: bool) -> Image {
    let pad = crate::paint::dense(LIFT) as usize;
    let drop = crate::paint::dense(1.5) as usize;
    let reach = crate::paint::dense(2.5).max(1) as usize;
    let (pw, ph) = (piece.width as usize, piece.height as usize);
    let (width, height) = (pw + pad * 2, ph + pad * 2);
    let mut cover = vec![0.0f32; width * height];
    for y in 0..ph {
        for x in 0..pw {
            let to = (y + pad + drop).min(height - 1) * width + x + pad;
            cover[to] = f32::from(piece.rgba[(y * pw + x) * 4 + 3]) / 255.0;
        }
    }
    let shade = blurred(&cover, width, height, reach);
    let (tone, deep) = match dark_ink {
        true => (255.0, 0.45),
        false => (0.0, 0.40),
    };
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let a = shade[y * width + x] * deep;
            let mut px = [tone * a, tone * a, tone * a, a * 255.0];
            if (pad..pad + pw).contains(&x) && (pad..pad + ph).contains(&y) {
                let at = ((y - pad) * pw + x - pad) * 4;
                let over = f32::from(piece.rgba[at + 3]) / 255.0;
                for c in 0..4 {
                    px[c] = f32::from(piece.rgba[at + c]) + px[c] * (1.0 - over);
                }
            }
            rgba.extend(px.map(|v| v.round().clamp(0.0, 255.0) as u8));
        }
    }
    Image {
        width: width as u32,
        height: height as u32,
        rgba,
    }
}

pub struct Frosted {
    pub body: Image,
    pub shadow: Image,
}

pub fn frosted_clock(font: &Font, clock: &str, dark: bool) -> Frosted {
    let text = font.paint_bold(clock, [255, 255, 255]);
    let size = crate::paint::logical(&text);
    let pad = (size.y * 0.22).round();
    let mut canvas = crate::paint::Canvas::new((size.x + pad * 2.0) as u32, (size.y + pad * 2.0) as u32);
    canvas.stamp(&text, Vec2::splat(pad), 1.0);
    let (width, height) = (canvas.image.width as usize, canvas.image.height as usize);
    let cover: Vec<f32> = canvas.image.rgba.chunks(4).map(|px| f32::from(px[3]) / 255.0).collect();
    let reach = (text.height as f64 * 0.03).max(1.0) as usize;
    let inner = blurred(&cover, width, height, reach);
    let shade = blurred(&cover, width, height, reach * 4);
    let drop = (text.height as f64 * 0.035).round() as usize;
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let mut body = Vec::with_capacity(width * height * 4);
    let mut shadow = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let at = y * width + x;
            let rim = cover[at] * (1.0 - (inner[at] * 1.6 - 0.6).clamp(0.0, 1.0));
            let fill = match dark {
                true => DARK_FILL,
                false => TINT as f32,
            };
            let a = cover[at] * fill + rim * (1.0 - fill) * 0.85;
            let v = byte(a);
            let tone = match dark {
                true => 0,
                false => v,
            };
            body.extend_from_slice(&[tone, tone, tone, v]);
            let above = y.checked_sub(drop).map_or(0.0, |up| shade[up * width + x]);
            let deep = match dark {
                true => 0.12,
                false => 0.30,
            };
            shadow.extend_from_slice(&[0, 0, 0, byte(above * deep)]);
        }
    }
    let image = |rgba| Image {
        width: width as u32,
        height: height as u32,
        rgba,
    };
    Frosted {
        body: image(body),
        shadow: image(shadow),
    }
}

pub fn finish_clock(frosted: &Frosted) -> Image {
    let size = crate::paint::logical(&frosted.body);
    let mut canvas = crate::paint::Canvas::new(size.x.round() as u32, size.y.round() as u32);
    canvas.stamp(&frosted.shadow, Vec2::ZERO, 1.0);
    canvas.stamp(&frosted.body, Vec2::ZERO, 1.0);
    canvas.image
}

fn blurred(values: &[f32], width: usize, height: usize, reach: usize) -> Vec<f32> {
    let mut out = values.to_vec();
    let mut scratch = vec![0.0f32; values.len()];
    for _ in 0..3 {
        for y in 0..height {
            let row = &out[y * width..(y + 1) * width];
            let mut sum: f32 = row[..reach.min(width)].iter().sum();
            for x in 0..width {
                if x + reach < width {
                    sum += row[x + reach];
                }
                if x > reach {
                    sum -= row[x - reach - 1];
                }
                let span = (x + reach).min(width - 1) - x.saturating_sub(reach) + 1;
                scratch[y * width + x] = sum / span as f32;
            }
        }
        for x in 0..width {
            let mut sum: f32 = (0..reach.min(height)).map(|y| scratch[y * width + x]).sum();
            for y in 0..height {
                if y + reach < height {
                    sum += scratch[(y + reach) * width + x];
                }
                if y > reach {
                    sum -= scratch[(y - reach - 1) * width + x];
                }
                let span = (y + reach).min(height - 1) - y.saturating_sub(reach) + 1;
                out[y * width + x] = sum / span as f32;
            }
        }
    }
    out
}

pub fn with_shadow(look: &Look, image: &Image) -> Image {
    let mut canvas = crate::paint::Canvas::new(look.size.x.round() as u32, look.size.y.round() as u32);
    let field = look.field_rect();
    canvas.shadow(
        Rect::new(field.origin + Vec2::new(0.0, 3.0), field.size),
        field.size.y / 2.0,
        12.0,
        0.22,
    );
    canvas.stamp(image, Vec2::ZERO, 1.0);
    canvas.image
}

pub fn paint_logo_shadow(look: &Look) -> Image {
    let side = look.logo + SHADE * 2.0;
    let mut canvas = crate::paint::Canvas::new(side as u32, side as u32);
    let body = look.logo * 0.62;
    let center = Vec2::new(side / 2.0, side / 2.0 + look.logo * 0.06);
    canvas.shadow(Rect::new(center - Vec2::splat(body / 2.0), Vec2::splat(body)), body / 2.0, SHADE, 0.30);
    canvas.image
}

pub const EBB: f64 = 3.0;

pub fn ebb(seconds: f64) -> f32 {
    (0.5 - 0.5 * (seconds * std::f64::consts::TAU / EBB).cos()) as f32
}

pub fn paint_battery(look: &Look, font: Option<&Font>, battery: (bool, u8)) -> Image {
    battery_layer(look, font, battery, false)
}

pub fn paint_charge(look: &Look, font: Option<&Font>, battery: (bool, u8)) -> Image {
    battery_layer(look, font, battery, true)
}

fn battery_layer(look: &Look, font: Option<&Font>, (charging, percent): (bool, u8), glow: bool) -> Image {
    let words = font.map(|font| font.paint(&format!("{percent}%"), look.ink));
    let cell = Vec2::new(27.0, 13.0);
    let gap = 8.0;
    let text = words.as_ref().map(crate::paint::logical);
    let width = text.map_or(0.0, |size| size.x + gap) + cell.x + 3.0;
    let mut canvas = crate::paint::Canvas::new(width.ceil() as u32, CORNER as u32);
    let mut x = 0.0;
    if let (Some(words), Some(size)) = (&words, text) {
        if !glow {
            canvas.stamp(words, Vec2::new(x, (CORNER - size.y) / 2.0).round(), 1.0);
        }
        x += size.x + gap;
    }
    let body = Rect::new(Vec2::new(x, (CORNER - cell.y) / 2.0), cell);
    if !glow {
        canvas.ring(body, 3.5, 1.2, look.ink, 0.9);
    }
    let inner = Rect::new(body.origin + Vec2::splat(2.5), body.size - Vec2::splat(5.0));
    let full = inner.size.x * f64::from(percent.min(100)) / 100.0;
    let color = match (charging, percent) {
        (_, 100..) => FULL,
        (false, 0..=10) => look.alert,
        _ => look.ink,
    };
    let strength = match (charging, glow) {
        (true, false) => 0.5,
        _ => 1.0,
    };
    if full >= 1.0 {
        canvas.round_rect(Rect::new(inner.origin, Vec2::new(full, inner.size.y)), 1.5, color, strength);
    }
    if !glow {
        canvas.round_rect(
            Rect::new(Vec2::new(body.max().x + 1.0, body.center().y - 2.5), Vec2::new(2.0, 5.0)),
            1.0,
            look.ink,
            0.6,
        );
    }
    if charging {
        let c = body.center();
        canvas.triangle([c + Vec2::new(1.5, -6.0), c + Vec2::new(-3.5, 1.0), c + Vec2::new(0.5, 1.0)], look.back, 1.0);
        canvas.triangle([c + Vec2::new(-1.5, 6.0), c + Vec2::new(3.5, -1.0), c + Vec2::new(-0.5, -1.0)], look.back, 1.0);
    }
    canvas.image
}

pub fn paint_corner(look: &Look, font: Option<&Font>, battery: Option<(bool, u8)>, tray: &crate::tray::Tray) -> Image {
    corner_layer(look, font, battery, tray, false)
}

pub fn paint_corner_charge(look: &Look, font: Option<&Font>, battery: Option<(bool, u8)>, tray: &crate::tray::Tray) -> Image {
    corner_layer(look, font, battery, tray, true)
}

fn corner_layer(look: &Look, font: Option<&Font>, battery: Option<(bool, u8)>, tray: &crate::tray::Tray, glow: bool) -> Image {
    let power = crate::tray::paint_power(tray.button as u32, look.ink);
    let part = battery.map(|battery| battery_layer(look, font, battery, glow));
    let lead = part.as_ref().map_or(0.0, |part| crate::paint::logical(part).x + tray.button_gap);
    let width = lead + tray.button;
    let mut canvas = crate::paint::Canvas::new(width.ceil() as u32, CORNER as u32);
    if let Some(part) = &part {
        canvas.stamp(part, Vec2::ZERO, 1.0);
    }
    if !glow {
        let side = crate::paint::logical(&power);
        canvas.stamp(&power, Vec2::new(lead, (CORNER - side.y) / 2.0).round(), 1.0);
    }
    canvas.image
}

pub fn paint(look: &Look, small: Option<&Font>, named: Option<&Font>, name: &str, dots: usize, note: Note) -> Image {
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
    let named = soft_shadow(&named.unwrap_or(font).paint_bold(name, look.ink), look.dark_ink());
    canvas.stamp(&named, centered(&named, -LIFT), 1.0);
    if count == 0 && note == Note::Ask {
        let hint = font.paint(note.text(), look.dim);
        let y = field.center().y - crate::paint::logical(&hint).y / 2.0;
        canvas.stamp(&hint, Vec2::new(field.origin.x + INDENT + 8.0, y).round(), 1.0);
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
        paint(&Look::default(), None, None, "Ada", dots, note)
    }

    #[test]
    fn the_lock_at_density_two_matches_density_one() {
        let look = Look::default();
        let big = Font::find(None, 96.0).ok();
        let small = Font::find(None, 17.0).ok();
        crate::paint::same_at_two("lock", || paint(&look, small.as_ref(), big.as_ref(), "Ada Lovelace", 5, Note::Wrong));
        crate::paint::same_at_two("lock top", || paint_top(&look, small.as_ref(), words().date));
        let _ = big;
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
    fn the_caret_follows_the_last_dot_inside_the_field() {
        let look = Look::default();
        let field = look.field_rect();
        let empty = look.caret_rect(0);
        let three = look.caret_rect(3);
        assert!(field.contains(empty.center()) && field.contains(three.center()));
        assert!(three.origin.x > look.dot_center(2, 3).x + look.dot / 2.0);
        assert!(look.caret_rect(MAX_LEN).max().x < field.max().x);
    }

    #[test]
    fn the_corner_puts_battery_and_power_where_the_top_bar_does() {
        let look = Look::default();
        let font = Font::find(None, 17.0).ok();
        let tray = crate::tray::Tray::default();
        let band = Rect::new(Vec2::ZERO, Vec2::new(1600.0, 32.0));
        let battery = paint_battery(&look, font.as_ref(), (true, 87));
        let corner = paint_corner(&look, font.as_ref(), Some((true, 87)), &tray);
        let size = crate::paint::logical(&corner);
        let spot = tray.status_rect(band, size);
        assert_eq!(spot.max().x, tray.power_rect(band).max().x);
        assert!(spot.origin.x < tray.power_rect(band).origin.x - crate::paint::logical(&battery).x);
        assert!(paint_corner(&look, font.as_ref(), None, &tray).width < corner.width);
        crate::paint::same_at_two("lock corner", || paint_corner(&look, font.as_ref(), Some((false, 5)), &tray));
    }

    #[test]
    fn a_charging_battery_ebbs_on_a_slow_glow_layer_of_the_same_size() {
        let look = Look::default();
        let font = Font::find(None, 17.0).ok();
        let base = paint_battery(&look, font.as_ref(), (true, 60));
        let glow = paint_charge(&look, font.as_ref(), (true, 60));
        assert_eq!((base.width, base.height), (glow.width, glow.height));
        assert!(glow.rgba.chunks(4).filter(|px| px[3] > 0).count() < base.rgba.chunks(4).filter(|px| px[3] > 0).count());
        assert_eq!(ebb(0.0), 0.0);
        assert!((ebb(EBB / 2.0) - 1.0).abs() < 1e-6);
        assert!(ebb(EBB / 4.0) > 0.4 && ebb(EBB / 4.0) < 0.6);
    }

    #[test]
    fn a_full_battery_fills_green_inside_its_own_border() {
        let look = Look::default();
        let full = paint_battery(&look, None, (false, 100));
        let half = paint_battery(&look, None, (false, 50));
        let green = |image: &Image| image.rgba.chunks(4).any(|px| px[3] == 255 && px[..3] == FULL);
        assert!(green(&full));
        assert!(!green(&half));
        assert!(full.rgba.chunks(4).any(|px| px[3] > 0 && px[0] > 200 && px[1] > 200 && px[2] > 200), "the border stays light");
    }

    #[test]
    fn the_battery_sits_left_of_the_control_button_and_the_clock_makes_room() {
        let band = Rect::new(Vec2::ZERO, Vec2::new(1600.0, 32.0));
        let mut tray = crate::tray::Tray::default();
        let text = Vec2::new(80.0, 16.0);
        let bare = tray.clock_rect(band, text);
        tray.battery = 60.0;
        let control = tray.control_rect(band);
        let battery = tray.battery_rect(band, Vec2::new(60.0, 28.0));
        assert!(battery.max().x <= control.origin.x);
        assert!(control.max().x <= tray.power_rect(band).origin.x);
        let clock = tray.clock_rect(band, text);
        assert_eq!(bare.origin.x - clock.origin.x, 60.0 + tray.button_gap);
        assert!(clock.max().x <= battery.origin.x);
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
        let plain = paint(&look, Some(&font), None, "Ada", 0, Note::Ask);
        let wrong = paint(&look, Some(&font), None, "Ada", 0, Note::Wrong);
        assert_ne!(plain.rgba, wrong.rgba);
    }

    #[test]
    fn the_frosted_clock_is_see_through_white_with_a_bright_edge_and_a_shadow() {
        let Ok(font) = Font::find(None, 64.0) else { return };
        let frosted = frosted_clock(&font, "9:41", false);
        let alphas: Vec<u8> = frosted.body.rgba.chunks(4).map(|px| px[3]).collect();
        let most = *alphas.iter().max().unwrap();
        let tint = (TINT * 255.0).round() as u8;
        assert!(alphas.iter().any(|a| *a == tint), "the inside is a light white tint");
        assert!(most > tint + 40, "the edge is brighter than the inside");
        assert!(frosted.body.rgba.chunks(4).all(|px| px[0] == px[3]), "white, premultiplied");
        assert!(frosted.shadow.rgba.chunks(4).any(|px| px[3] > 0));
        crate::paint::same_at_two("frosted clock", || finish_clock(&frosted_clock(&font, "9:41", true)));
    }

    #[test]
    fn the_clock_sits_under_the_date_and_above_the_logo() {
        let look = Look::default();
        let screen = Vec2::new(1600.0, 1067.0);
        let date = look.top_rect(screen, Vec2::new(300.0, 34.0));
        let clock = look.clock_rect(screen, Vec2::new(500.0, 230.0));
        assert!(clock.origin.y > date.origin.y);
        assert!(clock.max().y < look.logo_shadow_rect(screen).origin.y);
        assert_eq!(clock.center().x, 800.0);
    }

    #[test]
    fn words_turn_black_over_a_bright_wallpaper_and_white_over_a_dark_one() {
        for palette in [crate::theme::DARK, crate::theme::LIGHT] {
            let look = Look::themed(&palette);
            assert!(look.over([250.0, 250.0, 240.0]).dark_ink());
            assert!(!look.over([20.0, 30.0, 60.0]).dark_ink());
        }
    }

    #[test]
    fn a_soft_shadow_pads_the_words_and_keeps_them_on_top() {
        let Ok(font) = Font::find(None, 24.0) else { return };
        let words = font.paint("Ada", [255, 255, 255]);
        let shadowed = soft_shadow(&words, false);
        let pad = crate::paint::dense(LIFT);
        assert_eq!(shadowed.width, words.width + pad * 2);
        let solid = words.rgba.chunks(4).position(|px| px[3] == 255).unwrap();
        let (x, y) = (solid as u32 % words.width + pad, solid as u32 / words.width + pad);
        assert_eq!(pixel(&shadowed, x, y), [255, 255, 255, 255]);
        let below = pixel(&shadowed, x, words.height + pad + 1);
        assert!(below[3] > 0 && below[0] == 0, "a dark shade falls under white words");
        let glow = soft_shadow(&font.paint("Ada", [20, 20, 20]), true);
        assert!(glow.rgba.chunks(4).any(|px| px[3] > 0 && px[3] < 200 && px[0] == px[3]));
    }

    #[test]
    fn the_light_palette_writes_dark_words() {
        let dark = Look::themed(&crate::theme::DARK);
        let light = Look::themed(&crate::theme::LIGHT);
        assert_ne!(dark.ink, light.ink);
        assert_ne!(dark.back, light.back);
    }
}
