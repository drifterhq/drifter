use crate::geom::{Rect, Vec2};
use crate::image::Image;

pub const GLIDE: f64 = 0.18;
pub const RADIUS: f64 = 12.0;

pub fn glide(from: Rect, to: Rect, t: f64, still: bool) -> (Rect, f32) {
    let eased = crate::motion::smooth(t);
    let alpha = eased.max(0.35) as f32;
    match still {
        true => (to, alpha),
        false => (crate::motion::between(from, to, eased), alpha),
    }
}

pub fn preview_patch(accent: [u8; 3]) -> Image {
    crate::paint::at_density(1, || {
        let side = 2 * RADIUS as u32 + 1;
        let full = Rect::new(Vec2::ZERO, Vec2::splat(f64::from(side)));
        let mut canvas = crate::paint::Canvas::new(side, side);
        canvas.round_rect(full, RADIUS, accent, 0.22);
        canvas.ring(full, RADIUS, 2.0, accent, 0.9);
        canvas.image
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Left,
    Right,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snap {
    pub edge: f64,
    pub corner: f64,
}

impl Default for Snap {
    fn default() -> Self {
        Snap {
            edge: 12.0,
            corner: 160.0,
        }
    }
}

impl Snap {
    pub fn zone_at(&self, pointer: Vec2, view: Rect) -> Option<Zone> {
        let left = pointer.x <= view.origin.x + self.edge;
        let right = pointer.x >= view.max().x - self.edge;
        let top = pointer.y <= view.origin.y + self.edge;

        let high = pointer.y <= view.origin.y + self.corner;
        let low = pointer.y >= view.max().y - self.corner;

        if top && !left && !right {
            return Some(Zone::Full);
        }
        if left && high {
            return Some(Zone::TopLeft);
        }
        if left && low {
            return Some(Zone::BottomLeft);
        }
        if left {
            return Some(Zone::Left);
        }
        if right && high {
            return Some(Zone::TopRight);
        }
        if right && low {
            return Some(Zone::BottomRight);
        }
        if right {
            return Some(Zone::Right);
        }
        None
    }

    pub fn rect(&self, zone: Zone, view: Rect) -> Rect {
        let half = Vec2::new(view.size.x / 2.0, view.size.y / 2.0);
        let (origin, size) = match zone {
            Zone::Full => (view.origin, view.size),
            Zone::Left => (view.origin, Vec2::new(half.x, view.size.y)),
            Zone::Right => (
                Vec2::new(view.origin.x + half.x, view.origin.y),
                Vec2::new(half.x, view.size.y),
            ),
            Zone::TopLeft => (view.origin, half),
            Zone::TopRight => (Vec2::new(view.origin.x + half.x, view.origin.y), half),
            Zone::BottomLeft => (Vec2::new(view.origin.x, view.origin.y + half.y), half),
            Zone::BottomRight => (view.origin + half, half),
        };
        Rect::new(origin.round(), size.round())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_preview_glides_from_the_window_into_the_zone() {
        let window = Rect::new(Vec2::new(600.0, 300.0), Vec2::new(500.0, 400.0));
        let zone = Rect::new(Vec2::ZERO, Vec2::new(960.0, 1040.0));
        let (start, faint) = glide(window, zone, 0.0, false);
        assert_eq!(start, window, "it starts on the window");
        assert_eq!(faint, 0.35, "and is never invisible");
        assert_eq!(glide(window, zone, 1.0, false), (zone, 1.0));
        let (mid, _) = glide(window, zone, 0.5, false);
        assert!(mid.origin.x < window.origin.x && mid.size.x > window.size.x);
        let (still, alpha) = glide(window, zone, 0.3, true);
        assert_eq!(still, zone, "reduce motion shows the zone at once");
        assert!(alpha < 1.0, "but still fades in");
        let patch = preview_patch([53, 132, 228]);
        let alpha_at = |x: u32, y: u32| patch.rgba[((y * patch.width + x) * 4 + 3) as usize];
        assert!(alpha_at(12, 0) > alpha_at(12, 12), "the ring is stronger than the fill");
    }

    fn view() -> Rect {
        Rect::new(Vec2::new(1000.0, 500.0), Vec2::new(1920.0, 1080.0))
    }

    #[test]
    fn the_left_edge_snaps_to_the_left_half() {
        let snap = Snap::default();
        let view = view();
        let at = Vec2::new(view.origin.x + 2.0, view.center().y);
        assert_eq!(snap.zone_at(at, view), Some(Zone::Left));

        let rect = snap.rect(Zone::Left, view);
        assert_eq!(rect.origin, view.origin);
        assert_eq!(rect.size, Vec2::new(960.0, 1080.0));
    }

    #[test]
    fn the_right_edge_snaps_to_the_right_half() {
        let snap = Snap::default();
        let view = view();
        let at = Vec2::new(view.max().x - 1.0, view.center().y);
        assert_eq!(snap.zone_at(at, view), Some(Zone::Right));

        let rect = snap.rect(Zone::Right, view);
        assert_eq!(rect.max().x, view.max().x);
        assert_eq!(rect.size, Vec2::new(960.0, 1080.0));
    }

    #[test]
    fn the_corners_win_over_the_sides() {
        let snap = Snap::default();
        let view = view();
        let top_left = Vec2::new(view.origin.x + 1.0, view.origin.y + 40.0);
        let bottom_right = Vec2::new(view.max().x - 1.0, view.max().y - 40.0);
        assert_eq!(snap.zone_at(top_left, view), Some(Zone::TopLeft));
        assert_eq!(snap.zone_at(bottom_right, view), Some(Zone::BottomRight));
    }

    #[test]
    fn the_top_edge_snaps_to_the_whole_viewport() {
        let snap = Snap::default();
        let view = view();
        let at = Vec2::new(view.center().x, view.origin.y + 1.0);
        assert_eq!(snap.zone_at(at, view), Some(Zone::Full));
        assert_eq!(snap.rect(Zone::Full, view), view);
    }

    #[test]
    fn the_middle_of_the_screen_snaps_to_nothing() {
        let snap = Snap::default();
        let view = view();
        assert_eq!(snap.zone_at(view.center(), view), None);
    }

    #[test]
    fn the_four_quadrants_tile_the_viewport_with_no_gap() {
        let snap = Snap::default();
        let view = view();
        let quads = [
            Zone::TopLeft,
            Zone::TopRight,
            Zone::BottomLeft,
            Zone::BottomRight,
        ];
        let area: f64 = quads
            .iter()
            .map(|zone| {
                let rect = snap.rect(*zone, view);
                rect.size.x * rect.size.y
            })
            .sum();
        assert_eq!(area, view.size.x * view.size.y);

        let top_left = snap.rect(Zone::TopLeft, view);
        let bottom_right = snap.rect(Zone::BottomRight, view);
        assert_eq!(top_left.max(), bottom_right.origin);
    }

    #[test]
    fn the_two_halves_tile_the_viewport_with_no_gap() {
        let snap = Snap::default();
        let view = view();
        let left = snap.rect(Zone::Left, view);
        let right = snap.rect(Zone::Right, view);
        assert_eq!(left.max().x, right.origin.x);
        assert_eq!(left.origin, view.origin);
        assert_eq!(right.max(), view.max());
    }

    #[test]
    fn a_snap_zone_follows_the_camera_rather_than_the_screen() {
        let snap = Snap::default();
        let here = Rect::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        let there = Rect::new(Vec2::new(5000.0, 3000.0), Vec2::new(1920.0, 1080.0));
        assert_eq!(snap.rect(Zone::Left, there).origin, there.origin);
        assert_ne!(
            snap.rect(Zone::Left, here).origin,
            snap.rect(Zone::Left, there).origin
        );
    }
}
