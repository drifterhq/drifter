use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gauge {
    Volume,
    Microphone,
    Brightness,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    pub gauge: Gauge,
    pub level: f64,
    pub muted: bool,
}

impl Reading {
    pub fn shown(&self) -> f64 {
        match self.muted {
            true => 0.0,
            false => self.level.clamp(0.0, 1.0),
        }
    }
}

pub const EASE: f64 = 0.18;

pub fn eased_level(from: f64, to: f64, t: f64) -> f64 {
    from + (to - from) * crate::motion::smooth(t)
}

pub fn parse_wpctl(text: &str) -> Option<(f64, bool)> {
    let rest = text.trim().strip_prefix("Volume:")?;
    let level: f64 = rest.split_whitespace().next()?.parse().ok()?;
    Some((level, rest.contains("[MUTED]")))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub size: Vec2,
    pub lift: f64,
    pub back: [u8; 3],
    pub back_alpha: f64,
    pub track: [u8; 3],
    pub fill: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub inset: f64,
}

pub const NAMED: f64 = 56.0;

impl Look {
    pub fn named(self) -> Look {
        Look {
            size: Vec2::new(self.size.x + NAMED, self.size.y),
            inset: NAMED,
            ..self
        }
    }

    pub fn themed(p: &crate::theme::Palette) -> Look {
        Look {
            size: Vec2::new(320.0, 56.0),
            lift: 120.0,
            back: p.back,
            back_alpha: 0.94,
            track: p.off,
            fill: p.ink,
            ink: p.ink,
            dim: p.dim,
            inset: 0.0,
        }
    }
}

impl Default for Look {
    fn default() -> Self {
        Look::themed(&crate::theme::DARK)
    }
}

impl Look {
    pub fn rect(&self, screen: Vec2) -> Rect {
        Rect::new(
            Vec2::new(
                ((screen.x - self.size.x) / 2.0).round(),
                screen.y - self.lift - self.size.y,
            ),
            self.size,
        )
    }

    pub fn bar(&self) -> Rect {
        Rect::new(
            Vec2::new(120.0 + self.inset, self.size.y / 2.0 - 3.0),
            Vec2::new(130.0, 6.0),
        )
    }

    pub fn paint(&self, reading: Reading, font: Option<&Font>) -> Image {
        let mut canvas = Canvas::on(self.paint_frame(reading, font));
        let bar = self.bar();
        let filled = Rect::new(bar.origin, Vec2::new(bar.size.x * reading.shown(), bar.size.y));
        canvas.round_rect(filled, 3.0, self.fill, 1.0);
        canvas.image
    }

    pub fn icon_rect(&self) -> Rect {
        Rect::new(Vec2::new(18.0, (self.size.y - 26.0) / 2.0), Vec2::splat(26.0))
    }

    pub fn paint_named(&self, reading: Reading, name: &str, font: &Font) -> Image {
        let mut canvas = Canvas::new(self.size.x as u32, self.size.y as u32);
        canvas.round_rect(Rect::new(Vec2::ZERO, self.size), self.size.y / 2.0, self.back, self.back_alpha);
        let bar = self.bar();
        canvas.round_rect(bar, 3.0, self.track, 1.0);
        let x = self.icon_rect().max().x + 8.0;
        let label = font.paint(&crate::control::fit(font, name, bar.origin.x - x - 8.0), self.ink);
        canvas.stamp(&label, Vec2::new(x, (self.size.y - crate::paint::logical(&label).y) / 2.0), 1.0);
        let words = match reading.muted {
            true => crate::lang::t("Muted").to_string(),
            false => format!("{}%", (reading.level * 100.0).round() as i64),
        };
        let value = font.paint(&words, if reading.muted { self.dim } else { self.ink });
        let at = Vec2::new(self.size.x - 24.0 - crate::paint::logical(&value).x, (self.size.y - crate::paint::logical(&value).y) / 2.0);
        canvas.stamp(&value, at, 1.0);
        canvas.image
    }

    pub fn pill(&self) -> Image {
        let bar = self.bar();
        let mut canvas = Canvas::new(bar.size.x as u32, bar.size.y as u32);
        canvas.round_rect(Rect::new(Vec2::ZERO, bar.size), 3.0, self.fill, 1.0);
        canvas.image
    }

    pub fn paint_frame(&self, reading: Reading, font: Option<&Font>) -> Image {
        let mut canvas = Canvas::new(self.size.x as u32, self.size.y as u32);
        let all = Rect::new(Vec2::ZERO, self.size);
        canvas.round_rect(all, self.size.y / 2.0, self.back, self.back_alpha);

        let bar = self.bar();
        canvas.round_rect(bar, 3.0, self.track, 1.0);

        if let Some(font) = font {
            let name = match reading.gauge {
                Gauge::Volume => crate::lang::t("Volume"),
                Gauge::Microphone => crate::lang::t("Microphone"),
                Gauge::Brightness => crate::lang::t("Brightness"),
            };
            let label = font.paint(name, self.ink);
            let at = Vec2::new(24.0, (self.size.y - crate::paint::logical(&label).y) / 2.0);
            canvas.stamp(&label, at, 1.0);

            let words = match reading.muted {
                true => crate::lang::t("Muted").to_string(),
                false => format!("{}%", (reading.level * 100.0).round() as i64),
            };
            let value = font.paint(&words, if reading.muted { self.dim } else { self.ink });
            let at = Vec2::new(
                self.size.x - 24.0 - crate::paint::logical(&value).x,
                (self.size.y - crate::paint::logical(&value).y) / 2.0,
            );
            canvas.stamp(&value, at, 1.0);
        }
        canvas.image
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wpctl_reads_as_a_level_and_a_mute() {
        assert_eq!(parse_wpctl("Volume: 0.40\n"), Some((0.40, false)));
        assert_eq!(parse_wpctl("Volume: 0.55 [MUTED]\n"), Some((0.55, true)));
        assert_eq!(parse_wpctl("Volume: 1.20"), Some((1.20, false)));
    }

    #[test]
    fn the_frame_has_an_empty_track() {
        let look = Look::default();
        let bar = look.bar();
        let reading = Reading {
            gauge: Gauge::Volume,
            level: 1.0,
            muted: false,
        };
        let frame = look.paint_frame(reading, None);
        let full = look.paint(reading, None);
        let y = bar.center().y as u32;
        let at = |image: &Image, x: f64| image.rgba[((y * image.width + x as u32) * 4) as usize];
        assert!(at(&frame, bar.center().x) < 120, "the frame leaves the track empty");
        assert!(at(&full, bar.center().x) > 200, "a full paint fills it");
        let pill = look.pill();
        assert_eq!((pill.width, pill.height), (bar.size.x as u32, bar.size.y as u32));
    }

    #[test]
    fn the_level_eases_without_overshoot() {
        assert_eq!(eased_level(0.2, 0.8, 0.0), 0.2);
        assert_eq!(eased_level(0.2, 0.8, 1.0), 0.8);
        let mut last = 0.2;
        for step in 1..=20 {
            let now = eased_level(0.2, 0.8, step as f64 / 20.0);
            assert!(now >= last && now <= 0.8, "{now} after {last}");
            last = now;
        }
        assert!((eased_level(0.8, 0.2, 2.0) - 0.2).abs() < 1e-9, "it clamps past the end");
    }

    #[test]
    fn junk_from_wpctl_gives_nothing() {
        assert_eq!(parse_wpctl(""), None);
        assert_eq!(parse_wpctl("Volume: loud"), None);
        assert_eq!(parse_wpctl("Error: no sink"), None);
    }

    #[test]
    fn the_hud_at_density_two_matches_density_one() {
        let look = Look::default();
        let font = crate::text::Font::find(None, 13.0).ok();
        let reading = Reading {
            gauge: Gauge::Brightness,
            level: 0.6,
            muted: false,
        };
        crate::paint::same_at_two("hud", || look.paint(reading, font.as_ref()));
    }

    #[test]
    fn the_panel_sits_bottom_middle() {
        let look = Look::default();
        let rect = look.rect(Vec2::new(1920.0, 1080.0));
        assert_eq!(rect.center().x, 960.0);
        assert_eq!(rect.max().y, 1080.0 - look.lift);
    }

    #[test]
    fn the_bar_fills_with_the_level_and_empties_when_muted() {
        let look = Look::default();
        let bar = look.bar();
        let at = |image: &Image, x: f64| {
            let y = bar.center().y as u32;
            image.rgba[((y * image.width + x as u32) * 4) as usize]
        };
        let reading = Reading {
            gauge: Gauge::Volume,
            level: 0.5,
            muted: false,
        };
        let half = look.paint(reading, None);
        assert!(at(&half, bar.origin.x + bar.size.x * 0.25) > 200);
        assert!(at(&half, bar.origin.x + bar.size.x * 0.75) < 120);
        let muted = look.paint(
            Reading {
                muted: true,
                ..reading
            },
            None,
        );
        assert!(at(&muted, bar.origin.x + bar.size.x * 0.25) < 120);
    }

    #[test]
    fn a_named_hud_leaves_room_for_the_device_icon() {
        let look = Look::default().named();
        let font = Font::find(None, 13.0).unwrap();
        let icon = look.icon_rect();
        assert!(look.bar().origin.x - icon.max().x > 100.0, "room for a name");
        assert!(icon.max().x < look.bar().origin.x);
        crate::paint::same_at_two("named hud", || {
            look.paint_named(Reading { gauge: Gauge::Volume, level: 0.4, muted: false }, "AirPods Pro", &font)
        });
    }
}
