use crate::geom::{Rect, Vec2};

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
