use crate::geom::{Rect, Vec2};
use crate::image::Image;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Pinned(usize),
    Minimized(usize),
    Trash,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dock {
    pub icon: f64,
    pub gap: f64,
    pub margin: f64,
    pub divider: f64,
    pub rule: [u8; 3],
}

impl Default for Dock {
    fn default() -> Self {
        Dock {
            icon: 44.0,
            gap: 10.0,
            margin: 16.0,
            divider: 18.0,
            rule: [120, 126, 140],
        }
    }
}

impl Dock {
    pub fn width(&self, pinned: usize, minimized: usize) -> f64 {
        let count = pinned + minimized;
        if count == 0 {
            return 0.0;
        }
        let icons = self.icon * count as f64;
        let gaps = self.gap * (count.saturating_sub(1)) as f64;
        let split = if pinned > 0 && minimized > 0 {
            self.divider
        } else {
            0.0
        };
        icons + gaps + split
    }

    pub fn start(&self, band: Rect, pinned: usize, minimized: usize) -> f64 {
        let width = self.width(pinned, minimized);
        let free = band.size.x - width;
        (band.origin.x + free / 2.0).max(band.origin.x + self.margin)
    }

    pub fn slot_rect(&self, band: Rect, pinned: usize, minimized: usize, slot: Slot) -> Rect {
        let step = self.icon + self.gap;
        let split = if pinned > 0 && minimized > 0 {
            self.divider
        } else {
            0.0
        };
        let y = band.origin.y + (band.size.y - self.icon) / 2.0;
        if slot == Slot::Trash {
            let x = band.max().x - self.margin - self.icon;
            return Rect::new(Vec2::new(x, y).round(), Vec2::splat(self.icon));
        }
        let place = match slot {
            Slot::Pinned(index) => step * index as f64,
            Slot::Minimized(index) => step * (pinned + index) as f64 + split,
            Slot::Trash => 0.0,
        };
        let x = self.start(band, pinned, minimized) + place;
        Rect::new(Vec2::new(x, y).round(), Vec2::splat(self.icon))
    }

    pub fn divider_rect(&self, band: Rect, pinned: usize, minimized: usize) -> Option<Rect> {
        if pinned == 0 || minimized == 0 {
            return None;
        }
        let before = self.slot_rect(band, pinned, minimized, Slot::Pinned(pinned - 1));
        let after = self.slot_rect(band, pinned, minimized, Slot::Minimized(0));
        let middle = (before.max().x + after.origin.x) / 2.0;
        let tall = self.icon * 0.6;
        Some(Rect::new(
            Vec2::new(middle - 0.5, band.origin.y + (band.size.y - tall) / 2.0).round(),
            Vec2::new(1.0, tall),
        ))
    }

    pub fn at(&self, band: Rect, pointer: Vec2, pinned: usize, minimized: usize) -> Option<Slot> {
        let slots = (0..pinned)
            .map(Slot::Pinned)
            .chain((0..minimized).map(Slot::Minimized))
            .chain(std::iter::once(Slot::Trash));
        slots.into_iter().find(|slot| {
            self.slot_rect(band, pinned, minimized, *slot)
                .contains(pointer)
        })
    }
}

pub fn paint_trash(side: u32, color: [u8; 3]) -> Image {
    let size = f64::from(side.max(8));
    let mut rgba = vec![0u8; (side as usize) * (side as usize) * 4];

    let unit = size / 24.0;
    let lid_top = 5.0 * unit;
    let lid_bottom = 7.5 * unit;
    let body_top = lid_bottom;
    let body_bottom = 21.0 * unit;
    let handle_top = 3.0 * unit;
    let thick = (unit * 1.6).max(1.0);

    for y in 0..side {
        let py = f64::from(y) + 0.5;
        for x in 0..side {
            let px = f64::from(x) + 0.5;
            let ink = trash_ink(
                px,
                py,
                size,
                unit,
                lid_top,
                lid_bottom,
                body_top,
                body_bottom,
                handle_top,
                thick,
            );
            if ink <= 0.0 {
                continue;
            }
            let index = ((y as usize) * (side as usize) + x as usize) * 4;
            for (channel, value) in color.iter().enumerate() {
                rgba[index + channel] = (f64::from(*value) * ink).round().clamp(0.0, 255.0) as u8;
            }
            rgba[index + 3] = (ink * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }

    Image {
        width: side,
        height: side,
        rgba,
    }
}

#[allow(clippy::too_many_arguments)]
fn trash_ink(
    px: f64,
    py: f64,
    size: f64,
    unit: f64,
    lid_top: f64,
    lid_bottom: f64,
    body_top: f64,
    body_bottom: f64,
    handle_top: f64,
    thick: f64,
) -> f64 {
    let middle = size / 2.0;
    let from_middle = (px - middle).abs();

    if py >= lid_top && py < lid_bottom && from_middle <= 9.5 * unit {
        return 1.0;
    }

    if py >= handle_top && py < lid_top {
        let inner = 2.2 * unit;
        let outer = 4.0 * unit;
        if from_middle <= outer && (from_middle >= inner || py >= lid_top - thick) {
            return 1.0;
        }
    }

    if py >= body_top && py < body_bottom {
        let down = (py - body_top) / (body_bottom - body_top);
        let half = 8.0 * unit - down * 1.4 * unit;
        if from_middle > half {
            return 0.0;
        }
        if from_middle >= half - thick || py >= body_bottom - thick {
            return 1.0;
        }
        for ridge in [-3.6, 0.0, 3.6] {
            if (px - middle - ridge * unit).abs() <= thick * 0.45
                && py >= body_top + 2.0 * unit
                && py < body_bottom - 2.0 * unit
            {
                return 1.0;
            }
        }
    }

    0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn band() -> Rect {
        Rect::new(Vec2::new(0.0, 1016.0), Vec2::new(1920.0, 64.0))
    }

    #[test]
    fn the_icons_sit_in_a_row_with_an_even_gap() {
        let dock = Dock::default();
        let first = dock.slot_rect(band(), 3, 0, Slot::Pinned(0));
        let second = dock.slot_rect(band(), 3, 0, Slot::Pinned(1));
        assert_eq!(second.origin.x - first.max().x, dock.gap);
        assert_eq!(first.size, Vec2::splat(dock.icon));
    }

    #[test]
    fn the_whole_row_is_centered_on_the_screen() {
        let dock = Dock::default();
        let band = band();
        let first = dock.slot_rect(band, 4, 0, Slot::Pinned(0));
        let last = dock.slot_rect(band, 4, 0, Slot::Pinned(3));
        let middle = (first.origin.x + last.max().x) / 2.0;
        assert!((middle - band.center().x).abs() < 1.0);
    }

    #[test]
    fn the_minimized_icons_come_after_the_pinned_ones() {
        let dock = Dock::default();
        let band = band();
        let last_pinned = dock.slot_rect(band, 2, 2, Slot::Pinned(1));
        let first_minimized = dock.slot_rect(band, 2, 2, Slot::Minimized(0));
        assert!(first_minimized.origin.x > last_pinned.max().x);
    }

    #[test]
    fn the_divider_sits_between_the_two_groups() {
        let dock = Dock::default();
        let band = band();
        let rule = dock.divider_rect(band, 2, 2).expect("there is a divider");
        let last_pinned = dock.slot_rect(band, 2, 2, Slot::Pinned(1));
        let first_minimized = dock.slot_rect(band, 2, 2, Slot::Minimized(0));
        assert!(rule.origin.x >= last_pinned.max().x);
        assert!(rule.max().x <= first_minimized.origin.x);
    }

    #[test]
    fn there_is_no_divider_when_one_side_is_empty() {
        let dock = Dock::default();
        assert_eq!(dock.divider_rect(band(), 3, 0), None);
        assert_eq!(dock.divider_rect(band(), 0, 3), None);
    }

    #[test]
    fn the_divider_takes_room_so_the_groups_never_touch() {
        let dock = Dock::default();
        assert_eq!(dock.width(2, 2) - dock.width(4, 0), dock.divider);
    }

    #[test]
    fn an_empty_dock_has_no_width() {
        assert_eq!(Dock::default().width(0, 0), 0.0);
    }

    #[test]
    fn a_click_finds_the_pinned_icon_under_it() {
        let dock = Dock::default();
        let band = band();
        let second = dock.slot_rect(band, 3, 2, Slot::Pinned(1));
        assert_eq!(dock.at(band, second.center(), 3, 2), Some(Slot::Pinned(1)));
    }

    #[test]
    fn a_click_finds_the_minimized_icon_under_it() {
        let dock = Dock::default();
        let band = band();
        let first = dock.slot_rect(band, 3, 2, Slot::Minimized(0));
        assert_eq!(
            dock.at(band, first.center(), 3, 2),
            Some(Slot::Minimized(0))
        );
    }

    #[test]
    fn a_click_in_a_gap_hits_nothing() {
        let dock = Dock::default();
        let band = band();
        let first = dock.slot_rect(band, 3, 2, Slot::Pinned(0));
        let gap = Vec2::new(first.max().x + dock.gap / 2.0, first.center().y);
        assert_eq!(dock.at(band, gap, 3, 2), None);
    }

    #[test]
    fn a_click_on_the_divider_hits_nothing() {
        let dock = Dock::default();
        let band = band();
        let rule = dock.divider_rect(band, 2, 2).unwrap();
        assert_eq!(dock.at(band, rule.center(), 2, 2), None);
    }

    #[test]
    fn the_icons_are_centered_in_the_band() {
        let dock = Dock::default();
        let band = band();
        let icon = dock.slot_rect(band, 2, 0, Slot::Pinned(0));
        assert_eq!(icon.center().y, band.center().y);
    }

    fn trash_pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let index = ((y as usize) * (image.width as usize) + x as usize) * 4;
        [
            image.rgba[index],
            image.rgba[index + 1],
            image.rgba[index + 2],
            image.rgba[index + 3],
        ]
    }

    #[test]
    fn the_trash_is_drawn_not_looked_up() {
        let image = paint_trash(44, [230, 233, 240]);
        assert_eq!((image.width, image.height), (44, 44));
        let ink = image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .count();
        assert!(ink > 100, "the trash can is nearly empty");
    }

    #[test]
    fn the_trash_has_a_lid_above_its_body() {
        let image = paint_trash(48, [255, 255, 255]);
        let middle = image.width / 2;
        let lid = trash_pixel(&image, middle, 12);
        let body_wall = trash_pixel(&image, 11, 30);
        assert!(lid[3] > 0, "no lid");
        assert!(body_wall[3] > 0, "no body");
    }

    #[test]
    fn the_trash_is_the_same_on_both_sides() {
        let image = paint_trash(48, [255, 255, 255]);
        for y in 0..image.height {
            for x in 0..(image.width / 2) {
                let left = trash_pixel(&image, x, y);
                let right = trash_pixel(&image, image.width - 1 - x, y);
                assert_eq!(left, right, "the trash is lopsided at {x} {y}");
            }
        }
    }

    #[test]
    fn the_corners_of_the_trash_are_empty() {
        let image = paint_trash(44, [255, 255, 255]);
        assert_eq!(trash_pixel(&image, 0, 0)[3], 0);
        assert_eq!(trash_pixel(&image, image.width - 1, image.height - 1)[3], 0);
    }

    #[test]
    fn every_trash_pixel_is_premultiplied() {
        let image = paint_trash(44, [200, 210, 255]);
        for pixel in image.rgba.as_chunks::<4>().0 {
            let a = pixel[3];
            assert!(pixel[0] <= a && pixel[1] <= a && pixel[2] <= a);
        }
    }

    #[test]
    fn the_trash_sits_at_the_far_right() {
        let dock = Dock::default();
        let band = band();
        let trash = dock.slot_rect(band, 3, 1, Slot::Trash);
        assert_eq!(trash.max().x, band.max().x - dock.margin);
        assert_eq!(trash.center().y, band.center().y);
    }

    #[test]
    fn the_trash_never_moves_when_icons_come_and_go() {
        let dock = Dock::default();
        let band = band();
        let empty = dock.slot_rect(band, 0, 0, Slot::Trash);
        let busy = dock.slot_rect(band, 6, 4, Slot::Trash);
        assert_eq!(empty, busy);
    }

    #[test]
    fn a_click_on_the_trash_finds_it() {
        let dock = Dock::default();
        let band = band();
        let trash = dock.slot_rect(band, 2, 1, Slot::Trash);
        assert_eq!(dock.at(band, trash.center(), 2, 1), Some(Slot::Trash));
    }

    #[test]
    fn a_full_dock_never_starts_before_the_margin() {
        let dock = Dock::default();
        let band = band();
        let first = dock.slot_rect(band, 60, 0, Slot::Pinned(0));
        assert_eq!(first.origin.x, band.origin.x + dock.margin);
    }
}
