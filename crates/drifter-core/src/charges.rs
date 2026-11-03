use crate::geom::{Rect, Vec2};
use crate::glance::{Card, fit};
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

pub const GOOD: [u8; 3] = crate::lock::FULL;
pub const FAIR: [u8; 3] = [255, 204, 0];
pub const POOR: [u8; 3] = crate::glance::HOT;

const PAD: f64 = 14.0;
const HEAD: f64 = 42.0;
const ROW: f64 = 46.0;
const RING: f64 = 15.0;
const STROKE: f64 = 3.5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gadget {
    Computer,
    Mouse,
    Keyboard,
    Trackpad,
    Other,
}

impl Gadget {
    pub fn guess(name: &str) -> Gadget {
        let name = name.to_lowercase();
        let has = |words: &[&str]| words.iter().any(|word| name.contains(word));
        match () {
            _ if has(&["trackpad", "touchpad"]) => Gadget::Trackpad,
            _ if has(&["keyboard", "keys", "mx mechanical"]) => Gadget::Keyboard,
            _ if has(&["mouse", "mx anywhere", "mx master", "mx vertical", "mx ergo", "trackball"]) => Gadget::Mouse,
            _ => Gadget::Other,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Charge {
    pub name: String,
    pub gadget: Gadget,
    pub percent: u8,
    pub charging: bool,
}

pub fn color(percent: u8) -> [u8; 3] {
    match percent {
        50.. => GOOD,
        20..=49 => FAIR,
        _ => POOR,
    }
}

pub fn charge_of(read: impl Fn(&str) -> Option<String>) -> Option<Charge> {
    if read("type").as_deref().map(str::trim) != Some("Battery") {
        return None;
    }
    if read("present").is_some_and(|present| present.trim() == "0") {
        return None;
    }
    let number = |key: &str| read(key).and_then(|text| text.trim().parse::<f64>().ok());
    let pair = |energy: &str, charge: &str| number(energy).or_else(|| number(charge));
    let percent = match (pair("energy_now", "charge_now"), pair("energy_full", "charge_full")) {
        (Some(now), Some(full)) if full > 0.0 => now / full * 100.0,
        _ => number("capacity")?,
    };
    let device = read("scope").is_some_and(|scope| scope.trim() == "Device");
    let name = read("model_name").map(|name| name.trim().to_string()).filter(|name| !name.is_empty());
    let (name, gadget) = match (device, name) {
        (false, _) => (crate::lang::t("This computer").to_string(), Gadget::Computer),
        (true, Some(name)) => {
            let gadget = Gadget::guess(&name);
            (name, gadget)
        }
        (true, None) => (crate::lang::t("Battery").to_string(), Gadget::Other),
    };
    Some(Charge {
        name,
        gadget,
        percent: percent.round().clamp(0.0, 100.0) as u8,
        charging: read("status").is_some_and(|status| matches!(status.trim(), "Charging" | "Full")),
    })
}

pub fn read_all() -> Vec<Charge> {
    let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") else {
        return Vec::new();
    };
    let mut folders: Vec<std::path::PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    folders.sort();
    let mut found: Vec<Charge> = folders
        .iter()
        .filter_map(|folder| charge_of(|key| std::fs::read_to_string(folder.join(key)).ok()))
        .collect();
    found.sort_by_key(|charge| charge.gadget != Gadget::Computer);
    found
}

pub fn height(rows: usize) -> f64 {
    HEAD + ROW * rows.max(1) as f64 + 6.0
}

pub fn paint(card: &Card, font: &Font, bold: &Font, charges: &[Charge]) -> Image {
    let size = Vec2::new(card.width, height(charges.len()));
    let mut canvas = Canvas::new(size.x as u32, size.y.ceil() as u32);
    canvas.round_rect(Rect::new(Vec2::ZERO, size), crate::glance::RADIUS, card.back, card.back_alpha);
    let title = bold.paint_bold(crate::lang::t("Batteries"), card.ink);
    canvas.stamp(&title, Vec2::new(PAD, 14.0), 1.0);
    if charges.is_empty() {
        let none = font.paint(crate::lang::t("No batteries found"), card.dim);
        let mid = HEAD + ROW / 2.0;
        let shown = crate::paint::logical(&none);
        canvas.stamp(&none, Vec2::new((size.x - shown.x) / 2.0, mid - shown.y / 2.0).round(), 1.0);
        return canvas.image;
    }
    for (at, charge) in charges.iter().enumerate() {
        let mid = HEAD + ROW * at as f64 + ROW / 2.0;
        let center = Vec2::new(PAD + RING, mid);
        canvas.arc(center, RING, STROKE, 1.0, card.ink, 0.16);
        canvas.arc(center, RING, STROKE, f64::from(charge.percent) / 100.0, color(charge.percent), 1.0);
        glyph(&mut canvas, charge.gadget, center, card.ink);
        let shown = font.paint_even_digits(&format!("{}%", charge.percent), card.ink);
        let shown_size = crate::paint::logical(&shown);
        canvas.stamp(&shown, Vec2::new(size.x - PAD - shown_size.x, mid - shown_size.y / 2.0).round(), 1.0);
        let left = PAD + RING * 2.0 + 12.0;
        let room = size.x - PAD - shown_size.x - 10.0 - left;
        let name = font.paint(&fit(font, &charge.name, room.max(0.0)), card.ink);
        let name_size = crate::paint::logical(&name);
        match charge.charging {
            true => {
                let note = font.paint(&fit(font, crate::lang::t("Charging"), room.max(0.0)), card.dim);
                let note_size = crate::paint::logical(&note);
                let top = mid - (name_size.y + note_size.y) / 2.0;
                canvas.stamp(&name, Vec2::new(left, top).round(), 1.0);
                canvas.stamp(&note, Vec2::new(left, top + name_size.y).round(), 1.0);
            }
            false => canvas.stamp(&name, Vec2::new(left, mid - name_size.y / 2.0).round(), 1.0),
        }
    }
    canvas.image
}

fn glyph(canvas: &mut Canvas, gadget: Gadget, center: Vec2, ink: [u8; 3]) {
    let at = |x: f64, y: f64, w: f64, h: f64| Rect::new(center + Vec2::new(x, y), Vec2::new(w, h));
    match gadget {
        Gadget::Computer => desk(canvas, center, ink, 0.9),
        Gadget::Mouse => {
            canvas.ring(at(-4.5, -7.0, 9.0, 14.0), 4.5, 1.4, ink, 0.9);
            canvas.line(center + Vec2::new(0.0, -6.0), center + Vec2::new(0.0, -2.5), 1.4, ink, 0.9);
        }
        Gadget::Keyboard => {
            canvas.ring(at(-8.0, -5.0, 16.0, 10.0), 2.0, 1.4, ink, 0.9);
            for row in 0..2 {
                for col in 0..4 {
                    let spot = center + Vec2::new(-4.5 + 3.0 * f64::from(col), -1.5 + 3.0 * f64::from(row));
                    canvas.circle(spot, 0.8, ink, 0.9);
                }
            }
        }
        Gadget::Trackpad => canvas.ring(at(-8.0, -5.5, 16.0, 11.0), 2.5, 1.4, ink, 0.9),
        Gadget::Other => {
            canvas.ring(at(-7.0, -3.5, 13.0, 7.0), 1.8, 1.3, ink, 0.9);
            canvas.round_rect(at(6.5, -1.5, 1.6, 3.0), 0.8, ink, 0.9);
        }
    }
}

fn desk(canvas: &mut Canvas, center: Vec2, ink: [u8; 3], alpha: f64) {
    let screen = Rect::new(center + Vec2::new(-8.0, -7.0), Vec2::new(16.0, 11.0));
    canvas.ring(screen, 2.0, 1.4, ink, alpha);
    canvas.line(center + Vec2::new(0.0, 4.0), center + Vec2::new(0.0, 6.5), 1.4, ink, alpha);
    canvas.line(center + Vec2::new(-4.0, 7.0), center + Vec2::new(4.0, 7.0), 1.4, ink, alpha);
}

pub fn paint_desktop(ink: [u8; 3], tall: f64) -> Image {
    let mut canvas = Canvas::new(22, tall as u32);
    desk(&mut canvas, Vec2::new(11.0, tall / 2.0), ink, 0.9);
    canvas.image
}

#[cfg(test)]
mod tests {
    use super::*;

    fn supply(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |key| {
            pairs
                .iter()
                .find(|(each, _)| *each == key)
                .map(|(_, value)| format!("{value}\n"))
        }
    }

    #[test]
    fn a_mouse_reads_by_its_name_and_the_laptop_as_this_computer() {
        let mouse = charge_of(supply(&[
            ("type", "Battery"),
            ("scope", "Device"),
            ("model_name", "MX Anywhere 3"),
            ("capacity", "60"),
            ("status", "Discharging"),
        ]))
        .unwrap();
        assert_eq!((mouse.name.as_str(), mouse.gadget, mouse.percent, mouse.charging), ("MX Anywhere 3", Gadget::Mouse, 60, false));
        let laptop = charge_of(supply(&[
            ("type", "Battery"),
            ("energy_now", "30000000"),
            ("energy_full", "40000000"),
            ("status", "Charging"),
        ]))
        .unwrap();
        assert_eq!((laptop.gadget, laptop.percent, laptop.charging), (Gadget::Computer, 75, true));
    }

    #[test]
    fn mains_power_and_an_empty_bay_are_not_batteries() {
        assert!(charge_of(supply(&[("type", "Mains"), ("online", "1")])).is_none());
        assert!(charge_of(supply(&[("type", "Battery"), ("present", "0"), ("capacity", "0")])).is_none());
    }

    #[test]
    fn names_tell_the_gadget() {
        assert_eq!(Gadget::guess("Magic Trackpad"), Gadget::Trackpad);
        assert_eq!(Gadget::guess("Magic Keyboard"), Gadget::Keyboard);
        assert_eq!(Gadget::guess("Logitech MX Master 3S"), Gadget::Mouse);
        assert_eq!(Gadget::guess("Xbox Wireless Controller"), Gadget::Other);
    }

    #[test]
    fn the_ring_turns_green_yellow_red() {
        assert_eq!(color(100), GOOD);
        assert_eq!(color(50), GOOD);
        assert_eq!(color(31), FAIR);
        assert_eq!(color(19), POOR);
    }

    #[test]
    fn an_arc_fills_its_share_from_the_top() {
        let mut canvas = Canvas::new(40, 40);
        canvas.arc(Vec2::new(20.0, 20.0), 15.0, 4.0, 0.25, [255, 255, 255], 1.0);
        let lit = |x: f64, y: f64| {
            let d = crate::paint::density();
            let (x, y) = ((x * d) as u32, (y * d) as u32);
            canvas.image.rgba[((y * canvas.image.width + x) * 4 + 3) as usize] > 128
        };
        assert!(lit(28.0, 7.5));
        assert!(lit(35.0, 20.0) || lit(34.5, 18.0));
        assert!(!lit(20.0, 35.0));
        assert!(!lit(5.0, 20.0));
    }
}
