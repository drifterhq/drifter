//! The small panel that shows the volume or the brightness for a moment
//! after a media key.

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
    /// 0 to 1. Volume can go past 1 in PipeWire, but the bar stops at full.
    pub level: f64,
    pub muted: bool,
}

/// Reads wpctl get-volume, which prints "Volume: 0.40" and adds
/// " [MUTED]" when muted.
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
}

impl Default for Look {
    fn default() -> Self {
        Look {
            size: Vec2::new(320.0, 56.0),
            lift: 120.0,
            back: [28, 31, 40],
            back_alpha: 0.94,
            track: [72, 77, 94],
            fill: [236, 238, 244],
            ink: [236, 238, 244],
            dim: [146, 152, 166],
        }
    }
}

impl Look {
    /// Bottom middle of the screen, clear of the dock.
    pub fn rect(&self, screen: Vec2) -> Rect {
        Rect::new(
            Vec2::new(
                ((screen.x - self.size.x) / 2.0).round(),
                screen.y - self.lift - self.size.y,
            ),
            self.size,
        )
    }

    fn bar(&self) -> Rect {
        Rect::new(
            Vec2::new(120.0, self.size.y / 2.0 - 3.0),
            Vec2::new(130.0, 6.0),
        )
    }

    pub fn paint(&self, reading: Reading, font: Option<&Font>) -> Image {
        let mut canvas = Canvas::new(self.size.x as u32, self.size.y as u32);
        let all = Rect::new(Vec2::ZERO, self.size);
        canvas.round_rect(all, self.size.y / 2.0, self.back, self.back_alpha);

        let bar = self.bar();
        canvas.round_rect(bar, 3.0, self.track, 1.0);
        let level = if reading.muted {
            0.0
        } else {
            reading.level.clamp(0.0, 1.0)
        };
        let filled = Rect::new(bar.origin, Vec2::new(bar.size.x * level, bar.size.y));
        canvas.round_rect(filled, 3.0, self.fill, 1.0);

        if let Some(font) = font {
            let name = match reading.gauge {
                Gauge::Volume => "Volume",
                Gauge::Microphone => "Microphone",
                Gauge::Brightness => "Brightness",
            };
            let label = font.paint(name, self.ink);
            let at = Vec2::new(24.0, (self.size.y - f64::from(label.height)) / 2.0);
            canvas.stamp(&label, at, 1.0);

            let words = match reading.muted {
                true => "Muted".to_string(),
                false => format!("{}%", (reading.level * 100.0).round() as i64),
            };
            let value = font.paint(&words, if reading.muted { self.dim } else { self.ink });
            let at = Vec2::new(
                self.size.x - 24.0 - f64::from(value.width),
                (self.size.y - f64::from(value.height)) / 2.0,
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
    fn junk_from_wpctl_gives_nothing() {
        assert_eq!(parse_wpctl(""), None);
        assert_eq!(parse_wpctl("Volume: loud"), None);
        assert_eq!(parse_wpctl("Error: no sink"), None);
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
}
