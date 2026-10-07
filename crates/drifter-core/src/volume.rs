use crate::geom::{Rect, Vec2};
use crate::hud::Gauge;
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

pub const STEP: f64 = 0.05;

pub fn waves(level: f64, muted: bool) -> usize {
    match (muted, level) {
        (true, _) => 0,
        (_, level) if level <= 0.005 => 0,
        (_, level) if level < 0.34 => 1,
        (_, level) if level < 0.67 => 2,
        _ => 3,
    }
}

pub fn paint_speaker(side: u32, level: f64, muted: bool, color: [u8; 3]) -> Image {
    let side = side.max(8);
    let size = f64::from(side);
    let mut canvas = Canvas::new(side, side);
    let middle = size / 2.0;
    let body = Rect::new(
        Vec2::new(size * 0.08, middle - size * 0.14),
        Vec2::new(size * 0.18, size * 0.28),
    );
    canvas.round_rect(body, size * 0.03, color, 1.0);
    canvas.triangle(
        [
            Vec2::new(size * 0.14, middle),
            Vec2::new(size * 0.46, middle - size * 0.34),
            Vec2::new(size * 0.46, middle + size * 0.34),
        ],
        color,
        1.0,
    );

    let thick = (size * 0.085).max(1.2);
    if muted {
        let (cx, cy, reach) = (size * 0.74, middle, size * 0.16);
        cross(&mut canvas.image, Vec2::new(cx, cy), reach, thick, color);
        return canvas.image;
    }
    let center = Vec2::new(size * 0.40, middle);
    let rings = [size * 0.22, size * 0.37, size * 0.52];
    let shown = waves(level, muted);
    for (index, ring) in rings.iter().enumerate() {
        let alpha = if index < shown { 1.0 } else { 0.0 };
        if alpha > 0.0 {
            arc(&mut canvas.image, center, *ring, thick, color);
        }
    }
    canvas.image
}

pub fn paint_sun(side: u32, color: [u8; 3]) -> Image {
    let side = side.max(8);
    let size = f64::from(side);
    let mut canvas = Canvas::new(side, side);
    let center = Vec2::splat(size / 2.0);
    canvas.circle(center, size * 0.2, color, 1.0);
    let thick = (size * 0.085).max(1.2);
    for ray in 0..8 {
        let angle = f64::from(ray) * std::f64::consts::FRAC_PI_4;
        let way = Vec2::new(angle.cos(), angle.sin());
        segment(
            &mut canvas.image,
            center + way * (size * 0.32),
            center + way * (size * 0.45),
            thick,
            color,
        );
    }
    canvas.image
}

fn segment(image: &mut Image, from: Vec2, to: Vec2, thick: f64, color: [u8; 3]) {
    let along = to - from;
    let long = along.length().max(0.001);
    for y in 0..image.height {
        for x in 0..image.width {
            let p = spot(x, y) - from;
            let t = ((p.x * along.x + p.y * along.y) / (long * long)).clamp(0.0, 1.0);
            let away = (p - along * t).length();
            let ink = ((thick / 2.0 - away) * crate::paint::density() + 0.5).clamp(0.0, 1.0);
            if ink > 0.0 {
                put(image, x, y, color, ink);
            }
        }
    }
}

fn spot(x: u32, y: u32) -> Vec2 {
    Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5) / crate::paint::density()
}

fn put(image: &mut Image, x: u32, y: u32, color: [u8; 3], ink: f64) {
    let at = ((y as usize) * (image.width as usize) + x as usize) * 4;
    let had = f64::from(image.rgba[at + 3]) / 255.0;
    let ink = ink.max(had);
    for (channel, value) in color.iter().enumerate() {
        image.rgba[at + channel] = (f64::from(*value) * ink).round().clamp(0.0, 255.0) as u8;
    }
    image.rgba[at + 3] = (ink * 255.0).round().clamp(0.0, 255.0) as u8;
}

fn arc(image: &mut Image, center: Vec2, radius: f64, thick: f64, color: [u8; 3]) {
    let spread = 0.78_f64;
    for y in 0..image.height {
        for x in 0..image.width {
            let d = spot(x, y) - center;
            if d.x <= 0.0 || d.y.atan2(d.x).abs() > spread {
                continue;
            }
            let ink = ((thick / 2.0 - (d.length() - radius).abs()) * crate::paint::density() + 0.5).clamp(0.0, 1.0);
            if ink > 0.0 {
                put(image, x, y, color, ink);
            }
        }
    }
}

fn cross(image: &mut Image, center: Vec2, reach: f64, thick: f64, color: [u8; 3]) {
    for y in 0..image.height {
        for x in 0..image.width {
            let d = spot(x, y) - center;
            if d.x.abs() > reach + thick || d.y.abs() > reach + thick {
                continue;
            }
            let dense = crate::paint::density();
            let line = |a: f64, b: f64| (a - b).abs() / std::f64::consts::SQRT_2;
            let away = line(d.x, d.y).min(line(d.x, -d.y));
            let inside = ((reach - d.x.abs().max(d.y.abs())) * dense + 0.5).clamp(0.0, 1.0);
            let ink = ((thick / 2.0 - away) * dense + 0.5).clamp(0.0, 1.0) * inside;
            if ink > 0.0 {
                put(image, x, y, color, ink);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Level(f64),
    Mute,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Panel {
    pub size: Vec2,
    pub pad: f64,
    pub row: f64,
    pub radius: f64,
    pub back: [u8; 3],
    pub back_alpha: f64,
    pub track: [u8; 3],
    pub fill: [u8; 3],
    pub button: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
}

impl Panel {
    pub fn themed(p: &crate::theme::Palette) -> Panel {
        Panel {
            size: Vec2::new(260.0, 96.0),
            pad: 14.0,
            row: 30.0,
            radius: 10.0,
            back: p.back,
            back_alpha: 0.96,
            track: p.off,
            fill: p.ink,
            button: p.raised,
            ink: p.ink,
            dim: p.dim,
        }
    }
}

impl Default for Panel {
    fn default() -> Self {
        Panel::themed(&crate::theme::DARK)
    }
}

impl Panel {
    pub fn rect(&self, under: Rect, screen: Vec2) -> Rect {
        let x = (under.max().x - self.size.x)
            .min(screen.x - self.size.x - 6.0)
            .max(6.0);
        let y = under.max().y + 8.0;
        Rect::new(Vec2::new(x, y).round(), self.size)
    }

    pub fn icon(&self) -> Rect {
        Rect::new(Vec2::new(self.pad, self.pad + 4.0), Vec2::splat(22.0))
    }

    pub fn track(&self) -> Rect {
        let left = self.icon().max().x + 12.0;
        let right = self.size.x - self.pad - 44.0;
        Rect::new(
            Vec2::new(left, self.pad + self.row / 2.0 - 3.0),
            Vec2::new(right - left, 6.0),
        )
    }

    pub fn buttons(&self, gauge: Gauge) -> Vec<(Rect, Hit)> {
        let top = self.pad + self.row + 10.0;
        let gap = 8.0;
        let tall = self.size.y - top - self.pad;
        let whole = self.size.x - self.pad * 2.0;
        match gauge {
            Gauge::Volume => {
                let width = (whole - gap) / 2.0;
                vec![
                    (Rect::new(Vec2::new(self.pad, top), Vec2::new(width, tall)), Hit::Mute),
                    (
                        Rect::new(Vec2::new(self.pad + width + gap, top), Vec2::new(width, tall)),
                        Hit::Settings,
                    ),
                ]
            }
            _ => vec![(Rect::new(Vec2::new(self.pad, top), Vec2::new(whole, tall)), Hit::Settings)],
        }
    }

    pub fn level_at(&self, x: f64) -> f64 {
        let track = self.track();
        let along = (x - track.origin.x) / track.size.x.max(1.0);
        (along.clamp(0.0, 1.0) * 100.0).round() / 100.0
    }

    pub fn hit(&self, gauge: Gauge, inside: Vec2) -> Option<Hit> {
        let track = self.track();
        let grab = Rect::new(
            track.origin - Vec2::new(10.0, 12.0),
            track.size + Vec2::new(20.0, 24.0),
        );
        if grab.contains(inside) {
            return Some(Hit::Level(self.level_at(inside.x)));
        }
        self.buttons(gauge)
            .into_iter()
            .find(|(rect, _)| rect.contains(inside))
            .map(|(_, hit)| hit)
    }

    pub fn paint(&self, gauge: Gauge, level: f64, muted: bool, font: Option<&Font>) -> Image {
        let mut canvas = Canvas::new(self.size.x as u32, self.size.y as u32);
        let all = Rect::new(Vec2::ZERO, self.size);
        canvas.round_rect(all, self.radius, self.back, self.back_alpha);

        let icon = self.icon();
        let glyph = match gauge {
            Gauge::Brightness => paint_sun(icon.size.x as u32, self.ink),
            _ => paint_speaker(icon.size.x as u32, level, muted, self.ink),
        };
        canvas.stamp(&glyph, icon.origin, 1.0);

        let track = self.track();
        canvas.round_rect(track, 3.0, self.track, 1.0);
        let shown = level.clamp(0.0, 1.0);
        let filled = Rect::new(track.origin, Vec2::new(track.size.x * shown, track.size.y));
        let fill = if muted { self.dim } else { self.fill };
        canvas.round_rect(filled, 3.0, fill, 1.0);
        let knob = Vec2::new(track.origin.x + track.size.x * shown, track.center().y);
        canvas.circle(knob + Vec2::new(0.0, 1.0), 8.5, [0, 0, 0], 0.3);
        canvas.circle(knob, 8.0, [255, 255, 255], 1.0);

        let buttons = self.buttons(gauge);
        for (rect, _) in &buttons {
            canvas.round_rect(*rect, 7.0, self.button, 1.0);
        }

        if let Some(font) = font {
            let words = format!("{}%", (level * 100.0).round() as i64);
            let reading = font.paint(&words, if muted { self.dim } else { self.ink });
            let at = Vec2::new(
                self.size.x - self.pad - crate::paint::logical(&reading).x,
                track.center().y - crate::paint::logical(&reading).y / 2.0,
            );
            canvas.stamp(&reading, at, 1.0);
            for (rect, hit) in buttons {
                let label = match (hit, gauge) {
                    (Hit::Mute, _) if muted => "Unmute",
                    (Hit::Mute, _) => "Mute",
                    (_, Gauge::Brightness) => "Display Settings",
                    _ => "Sound Settings",
                };
                let text = font.paint(label, self.ink);
                canvas.stamp(&text, rect.center() - crate::paint::logical(&text) / 2.0, 1.0);
            }
        }
        canvas.image
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inked(image: &Image) -> usize {
        image.rgba.chunks(4).filter(|pixel| pixel[3] > 128).count()
    }

    #[test]
    fn gauges_at_density_two_match_density_one() {
        let font = crate::text::Font::find(None, 13.0).ok();
        let panel = Panel::themed(&crate::theme::DARK);
        crate::paint::same_at_two("speaker", || paint_speaker(20, 0.7, false, [255, 255, 255]));
        crate::paint::same_at_two("muted", || paint_speaker(20, 0.7, true, [255, 255, 255]));
        crate::paint::same_at_two("sun", || paint_sun(20, [255, 255, 255]));
        crate::paint::same_at_two("panel", || panel.paint(crate::hud::Gauge::Volume, 0.4, false, font.as_ref()));
    }

    #[test]
    fn louder_shows_more_waves_and_mute_shows_none() {
        assert_eq!(waves(0.0, false), 0);
        assert_eq!(waves(0.2, false), 1);
        assert_eq!(waves(0.5, false), 2);
        assert_eq!(waves(0.9, false), 3);
        assert_eq!(waves(0.9, true), 0);
    }

    #[test]
    fn the_speaker_grows_with_its_waves() {
        let quiet = inked(&paint_speaker(36, 0.0, false, [255; 3]));
        let some = inked(&paint_speaker(36, 0.5, false, [255; 3]));
        let loud = inked(&paint_speaker(36, 1.0, false, [255; 3]));
        let muted = inked(&paint_speaker(36, 1.0, true, [255; 3]));
        assert!(quiet < some && some < loud, "{quiet} {some} {loud}");
        assert!(
            muted > quiet && muted < loud,
            "the mute cross is drawn: {muted}"
        );
    }

    #[test]
    fn the_panel_hangs_under_its_button_and_stays_on_screen() {
        let panel = Panel::default();
        let screen = Vec2::new(1280.0, 720.0);
        let button = Rect::new(Vec2::new(1100.0, 8.0), Vec2::splat(20.0));
        let rect = panel.rect(button, screen);
        assert_eq!(rect.max().x, button.max().x);
        assert!(rect.origin.y > button.max().y);
        let edge = Rect::new(Vec2::new(1260.0, 8.0), Vec2::splat(20.0));
        assert!(panel.rect(edge, screen).max().x <= screen.x);
        let left = Rect::new(Vec2::new(4.0, 8.0), Vec2::splat(20.0));
        assert!(panel.rect(left, screen).origin.x >= 0.0);
    }

    #[test]
    fn the_track_sets_the_level_from_end_to_end() {
        let panel = Panel::default();
        let track = panel.track();
        let at = |x: f64| panel.hit(Gauge::Volume, Vec2::new(x, track.center().y));
        assert_eq!(at(track.origin.x - 5.0), Some(Hit::Level(0.0)));
        assert_eq!(at(track.max().x + 5.0), Some(Hit::Level(1.0)));
        assert_eq!(at(track.center().x), Some(Hit::Level(0.5)));
        let buttons = panel.buttons(Gauge::Volume);
        assert_eq!(panel.hit(Gauge::Volume, buttons[0].0.center()), Some(Hit::Mute));
        assert_eq!(panel.hit(Gauge::Volume, buttons[1].0.center()), Some(Hit::Settings));
        assert_eq!(panel.hit(Gauge::Volume, Vec2::new(2.0, 2.0)), None);
        let light = panel.buttons(Gauge::Brightness);
        assert_eq!(light.len(), 1, "brightness has no mute");
        assert_eq!(panel.hit(Gauge::Brightness, buttons[0].0.center()), Some(Hit::Settings));
        assert_eq!(
            panel.hit(Gauge::Brightness, Vec2::new(track.center().x, track.center().y)),
            Some(Hit::Level(0.5))
        );
    }

    #[test]
    fn the_sun_has_a_middle_and_rays() {
        let sun = paint_sun(36, [255; 3]);
        let at = |x: u32, y: u32| sun.rgba[((y * sun.width + x) * 4 + 3) as usize];
        assert!(at(18, 18) > 200, "the middle is solid");
        assert!(at(18, 2) > 100, "a ray reaches up");
        assert!(at(18, 9) < 50, "a gap between the middle and the ray");
    }

    #[test]
    fn the_buttons_sit_inside_the_panel_below_the_track() {
        let panel = Panel::default();
        let whole = Rect::new(Vec2::ZERO, panel.size);
        for (button, _) in panel.buttons(Gauge::Volume).into_iter().chain(panel.buttons(Gauge::Brightness)) {
            assert!(button.origin.y > panel.track().max().y + 8.0);
            assert!(button.max().x <= whole.max().x && button.max().y <= whole.max().y);
            assert!(button.size.y >= 24.0, "a button is big enough to hit");
        }
    }
}
