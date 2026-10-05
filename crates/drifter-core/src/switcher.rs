use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub tile: f64,
    pub icon: f64,
    pub pad: f64,
    pub gap: f64,
    pub title: f64,
    pub radius: f64,
    pub back: [u8; 3],
    pub back_alpha: f64,
    pub pick: [u8; 3],
    pub pick_alpha: f64,
    pub blank: [u8; 3],
    pub ink: [u8; 3],
}

impl Look {
    pub fn themed(p: &crate::theme::Palette) -> Look {
        Look {
            tile: 112.0,
            icon: 72.0,
            pad: 16.0,
            gap: 8.0,
            title: 36.0,
            radius: 18.0,
            back: p.back,
            back_alpha: 0.92,
            pick: p.shade,
            pick_alpha: 0.16,
            blank: p.off,
            ink: p.ink,
        }
    }
}

impl Default for Look {
    fn default() -> Self {
        Look::themed(&crate::theme::DARK)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub panel: Rect,
    pub tile: f64,
    pub icon: f64,
    count: usize,
    look: Look,
}

impl Look {
    pub fn layout(&self, screen: Vec2, count: usize) -> Layout {
        let count = count.max(1);
        let gaps = self.gap * (count - 1) as f64;
        let room = screen.x * 0.9 - self.pad * 2.0 - gaps;
        let tile = self.tile.min(room / count as f64).max(24.0);
        let icon = self.icon * tile / self.tile;
        let size = Vec2::new(
            self.pad * 2.0 + tile * count as f64 + gaps,
            self.pad * 2.0 + tile + self.title,
        );
        Layout {
            panel: Rect::new(((screen - size) / 2.0).round(), size),
            tile,
            icon,
            count,
            look: *self,
        }
    }
}

impl Layout {
    pub fn tile_rect(&self, index: usize) -> Rect {
        let look = &self.look;
        let x = look.pad + (self.tile + look.gap) * index.min(self.count - 1) as f64;
        Rect::new(Vec2::new(x, look.pad), Vec2::splat(self.tile))
    }

    pub fn icon_rect(&self, index: usize) -> Rect {
        Rect::from_center(self.tile_rect(index).center(), Vec2::splat(self.icon))
    }

    pub fn paint(
        &self,
        chosen: usize,
        title: &str,
        letters: &[Option<char>],
        font: Option<&Font>,
    ) -> Image {
        let look = &self.look;
        let size = self.panel.size;
        let mut canvas = Canvas::new(size.x.round() as u32, size.y.round() as u32);
        let all = Rect::new(Vec2::ZERO, size);
        canvas.round_rect(all, look.radius, look.back, look.back_alpha);
        canvas.round_rect(self.tile_rect(chosen), 12.0, look.pick, look.pick_alpha);

        for (index, letter) in letters.iter().enumerate() {
            let Some(letter) = letter else {
                continue;
            };
            let icon = self.icon_rect(index);
            canvas.round_rect(icon, self.icon * 0.22, look.blank, 1.0);
            if let Some(font) = font {
                let text = font.paint(&letter.to_uppercase().to_string(), look.ink);
                let at = icon.center() - text.size() / 2.0;
                canvas.stamp(&text, at, 1.0);
            }
        }

        if let Some(font) = font {
            let room = size.x - look.pad * 2.0;
            let text = font.paint(&fit(font, title, room), look.ink);
            let row = look.pad + self.tile;
            let at = Vec2::new(
                (size.x - f64::from(text.width)) / 2.0,
                row + (look.title - f64::from(text.height)) / 2.0 + look.pad / 2.0,
            );
            canvas.stamp(&text, at, 1.0);
        }
        canvas.image
    }
}

pub fn step(at: usize, count: usize, back: bool) -> usize {
    match (count, back) {
        (0, _) => 0,
        (_, false) => (at + 1) % count,
        (_, true) => (at + count - 1) % count,
    }
}

fn fit(font: &Font, text: &str, width: f64) -> String {
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
    format!("{}…", out.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Vec2 = Vec2::new(1920.0, 1080.0);

    #[test]
    fn the_panel_is_centered_on_the_screen() {
        let layout = Look::default().layout(SCREEN, 4);
        let middle = layout.panel.center();
        assert!((middle.x - 960.0).abs() <= 1.0 && (middle.y - 540.0).abs() <= 1.0);
    }

    #[test]
    fn tiles_sit_side_by_side_inside_the_panel() {
        let layout = Look::default().layout(SCREEN, 3);
        let first = layout.tile_rect(0);
        let second = layout.tile_rect(1);
        assert!(second.origin.x > first.max().x);
        assert!(layout.tile_rect(2).max().x <= layout.panel.size.x);
    }

    #[test]
    fn many_windows_shrink_the_tiles_to_fit_the_screen() {
        let layout = Look::default().layout(SCREEN, 40);
        assert!(layout.tile < Look::default().tile);
        assert!(layout.panel.size.x <= SCREEN.x * 0.9 + 1.0);
        assert!(layout.icon < layout.tile);
    }

    #[test]
    fn the_icon_sits_in_the_middle_of_its_tile() {
        let layout = Look::default().layout(SCREEN, 2);
        assert_eq!(layout.icon_rect(1).center(), layout.tile_rect(1).center());
    }

    #[test]
    fn stepping_wraps_both_ways() {
        assert_eq!(step(2, 3, false), 0);
        assert_eq!(step(0, 3, true), 2);
        assert_eq!(step(0, 0, false), 0);
    }

    #[test]
    fn the_panel_paints_at_its_own_size_with_round_corners() {
        let layout = Look::default().layout(SCREEN, 3);
        let image = layout.paint(1, "a window", &[None, Some('f'), None], None);
        assert_eq!(image.size(), layout.panel.size.round());
        assert_eq!(image.rgba[3], 0, "the corner is cut");
        let middle = ((image.height / 2 * image.width + image.width / 2) * 4 + 3) as usize;
        assert!(image.rgba[middle] > 200);
    }
}
