use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

pub fn paint_icon(side: u32, ink: [u8; 3]) -> Image {
    let size = f64::from(side.max(8));
    let mut canvas = Canvas::new(side, side);
    let pill = |canvas: &mut Canvas, y: f64, knob_left: bool| {
        let rect = Rect::new(Vec2::new(size * 0.1, y), Vec2::new(size * 0.8, size * 0.3));
        canvas.round_rect(rect, size * 0.15, ink, 1.0);
        let x = if knob_left { rect.origin.x + size * 0.15 } else { rect.max().x - size * 0.15 };
        canvas.circle(Vec2::new(x, rect.center().y), size * 0.1, [0, 0, 0], 1.0);
    };
    pill(&mut canvas, size * 0.14, false);
    pill(&mut canvas, size * 0.56, true);
    let mut image = canvas.image;
    for pixel in image.rgba.chunks_mut(4) {
        if pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 0 && pixel[3] == 255 {
            pixel[3] = 0;
        }
    }
    image
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile {
    Wifi,
    Bluetooth,
    Quiet,
    Dark,
}

impl Tile {
    pub const ALL: [Tile; 4] = [Tile::Wifi, Tile::Bluetooth, Tile::Quiet, Tile::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Tile::Wifi => "Wi-Fi",
            Tile::Bluetooth => "Bluetooth",
            Tile::Quiet => "Do Not Disturb",
            Tile::Dark => "Dark Mode",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Status {
    pub wifi: Option<(bool, String)>,
    pub bluetooth: Option<bool>,
    pub quiet: bool,
    pub dark: bool,
    pub light: Option<f64>,
    pub volume: Option<(f64, bool)>,
}

impl Status {
    pub fn on(&self, tile: Tile) -> bool {
        match tile {
            Tile::Wifi => self.wifi.as_ref().is_some_and(|(on, _)| *on),
            Tile::Bluetooth => self.bluetooth.unwrap_or(false),
            Tile::Quiet => self.quiet,
            Tile::Dark => self.dark,
        }
    }

    pub fn note(&self, tile: Tile) -> String {
        match tile {
            Tile::Wifi => match &self.wifi {
                None => "Not available".to_string(),
                Some((false, _)) => "Off".to_string(),
                Some((true, name)) if name.is_empty() => "Not connected".to_string(),
                Some((true, name)) => name.clone(),
            },
            Tile::Bluetooth => match self.bluetooth {
                None => "Not available".to_string(),
                Some(true) => "On".to_string(),
                Some(false) => "Off".to_string(),
            },
            Tile::Quiet | Tile::Dark => match self.on(tile) {
                true => "On".to_string(),
                false => "Off".to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Tile(Tile),
    Light(f64),
    Volume(f64),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub width: f64,
    pub pad: f64,
    pub tile: f64,
    pub gap: f64,
    pub slider: f64,
    pub radius: f64,
    pub back: [u8; 3],
    pub back_alpha: f64,
    pub card: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub off: [u8; 3],
    pub accent: [u8; 3],
}

impl Look {
    pub fn themed(p: &crate::theme::Palette, accent: [u8; 3]) -> Look {
        Look {
            width: 344.0,
            pad: 12.0,
            tile: 60.0,
            gap: 10.0,
            slider: 58.0,
            radius: 14.0,
            back: p.back,
            back_alpha: 0.96,
            card: p.card,
            ink: p.ink,
            dim: p.dim,
            off: p.off,
            accent,
        }
    }

    pub fn size(&self) -> Vec2 {
        let tiles = self.tile * 2.0 + self.gap;
        Vec2::new(self.width, self.pad * 2.0 + tiles + (self.gap + self.slider) * 2.0)
    }

    pub fn rect(&self, under: Rect, screen: Vec2) -> Rect {
        let size = self.size();
        let x = (under.max().x - size.x).min(screen.x - size.x - 6.0).max(6.0);
        Rect::new(Vec2::new(x, under.max().y + 8.0).round(), size)
    }

    pub fn tile_rect(&self, tile: Tile) -> Rect {
        let index = Tile::ALL.iter().position(|held| *held == tile).unwrap_or(0);
        let wide = (self.width - self.pad * 2.0 - self.gap) / 2.0;
        let (col, row) = (index % 2, index / 2);
        Rect::new(
            Vec2::new(
                self.pad + (wide + self.gap) * col as f64,
                self.pad + (self.tile + self.gap) * row as f64,
            ),
            Vec2::new(wide, self.tile),
        )
    }

    pub fn slider_rect(&self, which: usize) -> Rect {
        let top = self.pad + self.tile * 2.0 + self.gap * 2.0;
        Rect::new(
            Vec2::new(self.pad, top + (self.slider + self.gap) * which as f64),
            Vec2::new(self.width - self.pad * 2.0, self.slider),
        )
    }

    fn track(&self, which: usize) -> Rect {
        let card = self.slider_rect(which);
        Rect::new(
            Vec2::new(card.origin.x + 12.0, card.max().y - 22.0),
            Vec2::new(card.size.x - 24.0, 8.0),
        )
    }

    pub fn level_at(&self, which: usize, x: f64) -> f64 {
        let track = self.track(which);
        ((x - track.origin.x) / track.size.x.max(1.0)).clamp(0.0, 1.0)
    }

    pub fn hit(&self, inside: Vec2) -> Option<Hit> {
        if let Some(tile) = Tile::ALL.into_iter().find(|tile| self.tile_rect(*tile).contains(inside)) {
            return Some(Hit::Tile(tile));
        }
        if self.slider_rect(0).contains(inside) {
            return Some(Hit::Light(self.level_at(0, inside.x)));
        }
        if self.slider_rect(1).contains(inside) {
            return Some(Hit::Volume(self.level_at(1, inside.x)));
        }
        None
    }

    pub fn paint(&self, status: &Status, font: &Font) -> Image {
        let size = self.size();
        let mut canvas = Canvas::new(size.x as u32, size.y as u32);
        canvas.round_rect(Rect::new(Vec2::ZERO, size), self.radius, self.back, self.back_alpha);
        for tile in Tile::ALL {
            let rect = self.tile_rect(tile);
            canvas.round_rect(rect, 12.0, self.card, 0.85);
            let on = status.on(tile);
            let dot = Vec2::new(rect.origin.x + 26.0, rect.center().y);
            canvas.circle(dot, 15.0, if on { self.accent } else { self.off }, 1.0);
            symbol(&mut canvas, tile, dot, if on { [255, 255, 255] } else { self.ink });
            let x = rect.origin.x + 50.0;
            let room = (rect.max().x - x - 8.0).max(1.0);
            let label = font.paint(&fit(font, tile.label(), room), self.ink);
            let note = font.paint(&fit(font, &status.note(tile), room), self.dim);
            canvas.stamp(&label, Vec2::new(x, rect.center().y - crate::paint::logical(&label).y - 1.0), 1.0);
            canvas.stamp(&note, Vec2::new(x, rect.center().y + 1.0), 1.0);
        }
        for (which, (name, level)) in [
            ("Display", status.light),
            ("Sound", status.volume.map(|(level, muted)| if muted { 0.0 } else { level })),
        ]
        .into_iter()
        .enumerate()
        {
            let card = self.slider_rect(which);
            canvas.round_rect(card, 12.0, self.card, 0.85);
            let title = font.paint(name, self.ink);
            canvas.stamp(&title, card.origin + Vec2::new(12.0, 8.0), 1.0);
            let track = self.track(which);
            canvas.round_rect(track, 4.0, self.off, 1.0);
            if let Some(level) = level {
                let filled = Rect::new(track.origin, Vec2::new(track.size.x * level.clamp(0.0, 1.0), track.size.y));
                canvas.round_rect(filled, 4.0, self.ink, 1.0);
                let knob = Vec2::new(track.origin.x + track.size.x * level.clamp(0.0, 1.0), track.center().y);
                canvas.circle(knob + Vec2::new(0.0, 1.0), 9.0, [0, 0, 0], 0.25);
                canvas.circle(knob, 8.5, [255, 255, 255], 1.0);
            }
        }
        canvas.image
    }
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

fn shape(canvas: &mut Canvas, at: Vec2, ink: [u8; 3], inside: impl Fn(f64, Vec2) -> f64) {
    let dense = crate::paint::density();
    let (at, reach) = (at * dense, 10.0 * dense);
    let image = &mut canvas.image;
    for y in (at.y - reach).max(0.0) as u32..((at.y + reach) as u32).min(image.height) {
        for x in (at.x - reach).max(0.0) as u32..((at.x + reach) as u32).min(image.width) {
            let p = (Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5) - at) / dense;
            let cover = (inside(p.length(), p) * dense + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let index = ((y * image.width + x) * 4) as usize;
            for (channel, value) in ink.iter().enumerate() {
                let was = f64::from(image.rgba[index + channel]);
                image.rgba[index + channel] = (f64::from(*value) * cover + was * (1.0 - cover)).round() as u8;
            }
            let was = f64::from(image.rgba[index + 3]);
            image.rgba[index + 3] = (255.0 * cover + was * (1.0 - cover)).round() as u8;
        }
    }
}

fn symbol(canvas: &mut Canvas, tile: Tile, at: Vec2, ink: [u8; 3]) {
    match tile {
        Tile::Wifi => {
            for step in 0..3 {
                let tall = 4.0 + f64::from(step) * 4.0;
                let bar = Rect::new(
                    at + Vec2::new(-7.5 + f64::from(step) * 5.5, 6.0 - tall),
                    Vec2::new(3.5, tall),
                );
                canvas.round_rect(bar, 1.0, ink, 1.0);
            }
        }
        Tile::Bluetooth => {
            canvas.round_rect(Rect::new(at + Vec2::new(-1.0, -8.0), Vec2::new(2.0, 16.0)), 1.0, ink, 1.0);
            canvas.triangle([at + Vec2::new(1.0, -8.0), at + Vec2::new(6.0, -3.5), at + Vec2::new(1.0, 0.5)], ink, 1.0);
            canvas.triangle([at + Vec2::new(1.0, -0.5), at + Vec2::new(6.0, 3.5), at + Vec2::new(1.0, 8.0)], ink, 1.0);
        }
        Tile::Quiet => shape(canvas, at, ink, |d, p| {
            let cut = (p - Vec2::new(4.0, -3.5)).length();
            (7.5 - d).min(cut - 6.0)
        }),
        Tile::Dark => shape(canvas, at, ink, |d, p| {
            let ring = 1.6 - (d - 6.8).abs();
            let half = (7.0 - d).min(p.x);
            ring.max(half)
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_center_at_density_two_matches_density_one() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        let font = Font::find(None, 13.0).unwrap();
        let status = Status {
            wifi: Some((true, "Home".to_string())),
            bluetooth: Some(false),
            light: Some(0.6),
            volume: Some((0.4, false)),
            ..Status::default()
        };
        crate::paint::same_at_two("control", || look.paint(&status, &font));
        crate::paint::same_at_two("control icon", || paint_icon(20, [255, 255, 255]));
    }

    #[test]
    fn tiles_and_sliders_are_found_by_where_the_pointer_is() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        for tile in Tile::ALL {
            assert_eq!(look.hit(look.tile_rect(tile).center()), Some(Hit::Tile(tile)));
        }
        let display = look.slider_rect(0);
        match look.hit(Vec2::new(display.max().x - 1.0, display.center().y)) {
            Some(Hit::Light(level)) => assert!(level > 0.95),
            other => panic!("{other:?}"),
        }
        let sound = look.slider_rect(1);
        assert!(matches!(look.hit(Vec2::new(sound.origin.x + 1.0, sound.center().y)), Some(Hit::Volume(_))));
        assert_eq!(look.hit(Vec2::new(1.0, 1.0)), None);
        let size = look.size();
        assert!(look.slider_rect(1).max().y <= size.y);
    }

    #[test]
    fn each_tile_says_what_it_is_doing() {
        let status = Status {
            wifi: Some((true, "Home".into())),
            bluetooth: Some(false),
            quiet: true,
            ..Status::default()
        };
        assert_eq!(status.note(Tile::Wifi), "Home");
        assert!(status.on(Tile::Wifi));
        assert_eq!(status.note(Tile::Bluetooth), "Off");
        assert!(status.on(Tile::Quiet));
        assert!(!status.on(Tile::Dark));
        assert_eq!(Status::default().note(Tile::Wifi), "Not available");
    }
}
