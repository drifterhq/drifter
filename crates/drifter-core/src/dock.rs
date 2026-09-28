use crate::geom::{Rect, Vec2};
use crate::image::Image;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Pinned(usize),
    Minimized(usize),
    Settings,
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
    pub fn rule_ink(&self) -> [u8; 3] {
        [
            self.rule[0].saturating_add(80),
            self.rule[1].saturating_add(80),
            self.rule[2].saturating_add(80),
        ]
    }

    fn splits(&self, pinned: usize, minimized: usize) -> usize {
        let groups = [pinned, minimized, 2];
        let filled: Vec<usize> = groups.into_iter().filter(|count| *count > 0).collect();
        filled.len().saturating_sub(1)
    }

    pub fn width(&self, pinned: usize, minimized: usize) -> f64 {
        let count = pinned + minimized + 2;
        let icons = self.icon * count as f64;
        let gaps = self.gap * (count - 1) as f64;
        icons + gaps + self.divider * self.splits(pinned, minimized) as f64
    }

    pub fn start(&self, band: Rect, pinned: usize, minimized: usize) -> f64 {
        let width = self.width(pinned, minimized);
        let free = band.size.x - width;
        (band.origin.x + free / 2.0).max(band.origin.x + self.margin)
    }

    pub fn slot_rect(&self, band: Rect, pinned: usize, minimized: usize, slot: Slot) -> Rect {
        let step = self.icon + self.gap;
        let y = band.origin.y + (band.size.y - self.icon) / 2.0;
        let before_minimized = if pinned > 0 { self.divider } else { 0.0 };
        let tail = self.divider * self.splits(pinned, minimized) as f64;
        let place = match slot {
            Slot::Pinned(index) => step * index as f64,
            Slot::Minimized(index) => step * (pinned + index) as f64 + before_minimized,
            Slot::Settings => step * (pinned + minimized) as f64 + tail,
            Slot::Trash => step * (pinned + minimized + 1) as f64 + tail,
        };
        let x = self.start(band, pinned, minimized) + place;
        Rect::new(Vec2::new(x, y).round(), Vec2::splat(self.icon))
    }

    pub fn dividers(&self, band: Rect, pinned: usize, minimized: usize) -> Vec<Rect> {
        let mut rules = Vec::new();
        let tall = self.icon * 0.6;
        let y = band.origin.y + (band.size.y - tall) / 2.0;
        let between = |before: Rect, after: Rect, rules: &mut Vec<Rect>| {
            let middle = (before.max().x + after.origin.x) / 2.0;
            rules.push(Rect::new(
                Vec2::new(middle - 0.5, y).round(),
                Vec2::new(1.0, tall),
            ));
        };

        if pinned > 0 && minimized > 0 {
            between(
                self.slot_rect(band, pinned, minimized, Slot::Pinned(pinned - 1)),
                self.slot_rect(band, pinned, minimized, Slot::Minimized(0)),
                &mut rules,
            );
        }
        let last = match (minimized > 0, pinned > 0) {
            (true, _) => {
                Some(self.slot_rect(band, pinned, minimized, Slot::Minimized(minimized - 1)))
            }
            (false, true) => {
                Some(self.slot_rect(band, pinned, minimized, Slot::Pinned(pinned - 1)))
            }
            _ => None,
        };
        if let Some(last) = last {
            between(
                last,
                self.slot_rect(band, pinned, minimized, Slot::Settings),
                &mut rules,
            );
        }
        rules
    }

    pub fn dot_rect(&self, band: Rect, pinned: usize, minimized: usize, slot: Slot) -> Rect {
        let icon = self.slot_rect(band, pinned, minimized, slot);
        let side = (self.icon / 9.0).max(3.0).round();
        Rect::new(
            Vec2::new(
                icon.center().x - side / 2.0,
                (icon.max().y + side).min(band.max().y - side - 1.0),
            )
            .round(),
            Vec2::splat(side),
        )
    }

    pub fn at(&self, band: Rect, pointer: Vec2, pinned: usize, minimized: usize) -> Option<Slot> {
        let slots = (0..pinned)
            .map(Slot::Pinned)
            .chain((0..minimized).map(Slot::Minimized))
            .chain([Slot::Settings, Slot::Trash]);
        slots.into_iter().find(|slot| {
            self.slot_rect(band, pinned, minimized, *slot)
                .contains(pointer)
        })
    }
}

pub fn paint_cog(side: u32, color: [u8; 3]) -> Image {
    let side = side.max(8);
    let size = f64::from(side);
    let middle = size / 2.0;
    let outer = size * 0.44;
    let root = size * 0.34;
    let hole = size * 0.15;
    let mut rgba = vec![0u8; (side as usize) * (side as usize) * 4];

    for y in 0..side {
        let dy = f64::from(y) + 0.5 - middle;
        for x in 0..side {
            let dx = f64::from(x) + 0.5 - middle;
            let away = (dx * dx + dy * dy).sqrt();
            let wave = (dy.atan2(dx) * 8.0).cos();
            let edge = root + (outer - root) * (wave * 0.5 + 0.5).powf(6.0);
            let ink = (edge - away + 0.5).clamp(0.0, 1.0);
            let cut = (hole - away + 0.5).clamp(0.0, 1.0);
            let ink = (ink - cut).clamp(0.0, 1.0);
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

pub fn paint_dot(side: u32, color: [u8; 3]) -> Image {
    let side = side.max(2);
    let size = f64::from(side);
    let middle = size / 2.0;
    let radius = middle - 0.5;
    let mut rgba = vec![0u8; (side as usize) * (side as usize) * 4];

    for y in 0..side {
        let dy = f64::from(y) + 0.5 - middle;
        for x in 0..side {
            let dx = f64::from(x) + 0.5 - middle;
            let ink = (radius - (dx * dx + dy * dy).sqrt() + 0.5).clamp(0.0, 1.0);
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

pub fn paint_trash(side: u32, color: [u8; 3]) -> Image {
    let size = f64::from(side.max(8));
    let mut rgba = vec![0u8; (side as usize) * (side as usize) * 4];
    let unit = size / 24.0;

    for y in 0..side {
        let py = f64::from(y) + 0.5;
        for x in 0..side {
            let px = f64::from(x) + 0.5;
            let ink = can_ink(px, py, size, unit);
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

fn can_ink(px: f64, py: f64, size: f64, unit: f64) -> f64 {
    let middle = size / 2.0;
    let dx = px - middle;

    let handle = bar(dx, py, 2.6 * unit, 4.4 * unit, 6.0 * unit, 0.9 * unit);
    let lid = bar(dx, py, 8.4 * unit, 6.6 * unit, 8.6 * unit, 0.9 * unit);
    let body = can_body(dx, py, unit);
    let slots = slot_cut(dx, py, unit);

    let solid = handle.max(lid).max(body);
    (solid - slots).clamp(0.0, 1.0)
}

fn bar(dx: f64, py: f64, half: f64, top: f64, bottom: f64, radius: f64) -> f64 {
    let inside_x = half - dx.abs();
    let inside_y = (py - top).min(bottom - py);
    rounded(inside_x, inside_y, half, (bottom - top) / 2.0, radius)
}

fn can_body(dx: f64, py: f64, unit: f64) -> f64 {
    let top = 9.0 * unit;
    let bottom = 20.5 * unit;
    if py < top - 1.0 || py > bottom + 1.0 {
        return 0.0;
    }
    let down = ((py - top) / (bottom - top)).clamp(0.0, 1.0);
    let half = 7.2 * unit - down * 1.1 * unit;
    let inside_x = half - dx.abs();
    let inside_y = (py - top).min(bottom - py);
    rounded(inside_x, inside_y, half, (bottom - top) / 2.0, 1.6 * unit)
}

fn rounded(inside_x: f64, inside_y: f64, half_x: f64, half_y: f64, radius: f64) -> f64 {
    if inside_x <= -1.0 || inside_y <= -1.0 {
        return 0.0;
    }
    let corner_x = half_x - radius;
    let corner_y = half_y - radius;
    let over_x = (half_x - inside_x).abs() - corner_x;
    let over_y = (half_y - inside_y).abs() - corner_y;
    let distance = if over_x > 0.0 && over_y > 0.0 {
        radius - (over_x * over_x + over_y * over_y).sqrt()
    } else {
        inside_x.min(inside_y)
    };
    (distance + 0.5).clamp(0.0, 1.0)
}

fn slot_cut(dx: f64, py: f64, unit: f64) -> f64 {
    let top = 11.5 * unit;
    let bottom = 18.0 * unit;
    if py < top || py > bottom {
        return 0.0;
    }
    let mut cut: f64 = 0.0;
    for place in [-2.6, 0.0, 2.6] {
        let inside_x = 0.3 * unit - (dx - place * unit).abs();
        let inside_y = (py - top).min(bottom - py);
        cut = cut.max((inside_x.min(inside_y) + 0.5).clamp(0.0, 1.0));
    }
    cut
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
        let trash = dock.slot_rect(band, 4, 0, Slot::Trash);
        let middle = (first.origin.x + trash.max().x) / 2.0;
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
    fn a_divider_sits_between_each_pair_of_groups() {
        let dock = Dock::default();
        let band = band();
        let rules = dock.dividers(band, 2, 2);
        assert_eq!(rules.len(), 2, "pinned, minimized and trash need two rules");

        let last_pinned = dock.slot_rect(band, 2, 2, Slot::Pinned(1));
        let first_minimized = dock.slot_rect(band, 2, 2, Slot::Minimized(0));
        assert!(rules[0].origin.x >= last_pinned.max().x);
        assert!(rules[0].max().x <= first_minimized.origin.x);

        let last_minimized = dock.slot_rect(band, 2, 2, Slot::Minimized(1));
        let trash = dock.slot_rect(band, 2, 2, Slot::Trash);
        assert!(rules[1].origin.x >= last_minimized.max().x);
        assert!(rules[1].max().x <= trash.origin.x);
    }

    #[test]
    fn nothing_minimized_gives_one_divider_not_two() {
        let dock = Dock::default();
        assert_eq!(dock.dividers(band(), 3, 0).len(), 1);
    }

    #[test]
    fn an_empty_dock_still_has_no_divider_before_the_trash() {
        let dock = Dock::default();
        assert!(dock.dividers(band(), 0, 0).is_empty());
    }

    #[test]
    fn the_trash_comes_after_the_minimized_icons() {
        let dock = Dock::default();
        let band = band();
        let last = dock.slot_rect(band, 1, 2, Slot::Minimized(1));
        let trash = dock.slot_rect(band, 1, 2, Slot::Trash);
        assert!(trash.origin.x > last.max().x);
    }

    #[test]
    fn every_divider_takes_room_in_the_width() {
        let dock = Dock::default();
        let one = dock.width(3, 0);
        let two = dock.width(2, 1);
        assert_eq!(two - one, dock.divider);
    }

    #[test]
    fn an_empty_dock_still_holds_the_cog_and_the_trash() {
        let dock = Dock::default();
        assert_eq!(dock.width(0, 0), dock.icon * 2.0 + dock.gap);
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
    fn a_click_on_a_divider_hits_nothing() {
        let dock = Dock::default();
        let band = band();
        for rule in dock.dividers(band, 2, 2) {
            assert_eq!(dock.at(band, rule.center(), 2, 2), None);
        }
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
    fn the_trash_ink_is_lighter_than_the_divider() {
        let dock = Dock::default();
        assert!(dock.rule_ink()[0] > dock.rule[0]);
    }

    #[test]
    fn the_cog_sits_directly_left_of_the_trash() {
        let dock = Dock::default();
        let band = band();
        let cog = dock.slot_rect(band, 2, 1, Slot::Settings);
        let trash = dock.slot_rect(band, 2, 1, Slot::Trash);
        assert_eq!(trash.origin.x - cog.max().x, dock.gap);
        assert_eq!(cog.center().y, trash.center().y);
    }

    #[test]
    fn no_divider_ever_splits_the_cog_from_the_trash() {
        let dock = Dock::default();
        let band = band();
        let cog = dock.slot_rect(band, 2, 2, Slot::Settings);
        let trash = dock.slot_rect(band, 2, 2, Slot::Trash);
        for rule in dock.dividers(band, 2, 2) {
            assert!(rule.max().x <= cog.origin.x || rule.origin.x >= trash.max().x);
        }
    }

    #[test]
    fn a_click_on_the_cog_finds_it() {
        let dock = Dock::default();
        let band = band();
        let cog = dock.slot_rect(band, 1, 0, Slot::Settings);
        assert_eq!(dock.at(band, cog.center(), 1, 0), Some(Slot::Settings));
    }

    #[test]
    fn the_drawn_cog_is_round_with_a_hole() {
        let image = paint_cog(44, [230, 233, 240]);
        let middle = ((22usize) * 44 + 22) * 4;
        assert_eq!(image.rgba[middle + 3], 0, "the cog has no hole");
        let body = ((22usize) * 44 + 12) * 4;
        assert!(image.rgba[body + 3] > 0, "the cog has no body");
        assert_eq!(image.rgba[3], 0, "the cog fills its corner");
    }

    #[test]
    fn the_running_dot_sits_under_its_icon() {
        let dock = Dock::default();
        let band = band();
        let icon = dock.slot_rect(band, 2, 0, Slot::Pinned(1));
        let dot = dock.dot_rect(band, 2, 0, Slot::Pinned(1));
        assert!((dot.center().x - icon.center().x).abs() <= 0.5);
        assert!(dot.origin.y >= icon.max().y);
        assert!(dot.max().y <= band.max().y);
    }

    #[test]
    fn the_running_dot_is_small() {
        let dock = Dock::default();
        let dot = dock.dot_rect(band(), 1, 0, Slot::Pinned(0));
        assert!(dot.size.x >= 3.0);
        assert!(dot.size.x < dock.icon / 4.0);
        assert_eq!(dot.size.x, dot.size.y);
    }

    #[test]
    fn the_dot_is_round_and_solid_in_the_middle() {
        let image = paint_dot(6, [230, 233, 240]);
        assert_eq!((image.width, image.height), (6, 6));
        let middle = ((3usize) * 6 + 3) * 4;
        assert_eq!(image.rgba[middle + 3], 255);
        assert_eq!(image.rgba[3], 0);
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

    fn filled_rows(image: &Image) -> Vec<u32> {
        (0..image.height)
            .filter(|y| (0..image.width).any(|x| trash_pixel(image, x, *y)[3] > 128))
            .collect()
    }

    #[test]
    fn the_trash_is_a_handle_then_a_lid_then_a_body() {
        let image = paint_trash(48, [255, 255, 255]);
        let rows = filled_rows(&image);
        let widths: Vec<usize> = rows
            .iter()
            .map(|y| {
                (0..image.width)
                    .filter(|x| trash_pixel(&image, *x, *y)[3] > 128)
                    .count()
            })
            .collect();

        let widest = widths.iter().copied().max().expect("the can is empty");
        let lid = widths.iter().position(|w| *w == widest).unwrap();
        assert!(
            lid > 0,
            "the widest part is the very top, so there is no handle"
        );
        assert!(
            widths[..lid].iter().all(|w| *w < widest),
            "the handle is as wide as the lid"
        );
        assert!(
            widths[lid + 1..].iter().any(|w| *w > 2),
            "there is no body under the lid"
        );
    }

    #[test]
    fn the_trash_has_slots_cut_into_the_body() {
        let image = paint_trash(48, [255, 255, 255]);
        let deep = image.height * 2 / 3;
        let mut runs = 0;
        let mut inside = false;
        for x in 0..image.width {
            let solid = trash_pixel(&image, x, deep)[3] > 128;
            if solid && !inside {
                runs += 1;
            }
            inside = solid;
        }
        assert!(runs >= 3, "the body has no slots cut into it");
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
    fn the_trash_is_the_last_thing_in_the_row() {
        let dock = Dock::default();
        let band = band();
        let trash = dock.slot_rect(band, 3, 1, Slot::Trash);
        for slot in [Slot::Pinned(2), Slot::Minimized(0)] {
            assert!(dock.slot_rect(band, 3, 1, slot).max().x <= trash.origin.x);
        }
        assert_eq!(trash.center().y, band.center().y);
    }

    #[test]
    fn the_whole_row_including_the_trash_stays_centered() {
        let dock = Dock::default();
        let band = band();
        let first = dock.slot_rect(band, 2, 1, Slot::Pinned(0));
        let trash = dock.slot_rect(band, 2, 1, Slot::Trash);
        let middle = (first.origin.x + trash.max().x) / 2.0;
        assert!((middle - band.center().x).abs() < 1.0);
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
