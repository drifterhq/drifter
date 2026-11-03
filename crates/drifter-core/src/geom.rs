use std::ops::{Add, Div, Mul, Sub};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Vec2 { x, y }
    }

    pub const fn splat(v: f64) -> Self {
        Vec2 { x: v, y: v }
    }

    pub fn min(self, other: Vec2) -> Vec2 {
        Vec2::new(self.x.min(other.x), self.y.min(other.y))
    }

    pub fn max(self, other: Vec2) -> Vec2 {
        Vec2::new(self.x.max(other.x), self.y.max(other.y))
    }

    pub fn length(self) -> f64 {
        (self.x * self.x + self.y * self.y).sqrt()
    }

    pub fn round(self) -> Vec2 {
        Vec2::new((self.x + 0.5).floor(), (self.y + 0.5).floor())
    }

    pub fn clamp(self, lo: Vec2, hi: Vec2) -> Vec2 {
        self.max(lo).min(hi)
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, rhs: Vec2) -> Vec2 {
        Vec2::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, rhs: f64) -> Vec2 {
        Vec2::new(self.x * rhs, self.y * rhs)
    }
}

impl Div<f64> for Vec2 {
    type Output = Vec2;
    fn div(self, rhs: f64) -> Vec2 {
        Vec2::new(self.x / rhs, self.y / rhs)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub origin: Vec2,
    pub size: Vec2,
}

impl Rect {
    pub const fn new(origin: Vec2, size: Vec2) -> Self {
        Rect { origin, size }
    }

    pub fn from_center(center: Vec2, size: Vec2) -> Self {
        Rect::new(center - size / 2.0, size)
    }

    pub fn center(&self) -> Vec2 {
        self.origin + self.size / 2.0
    }

    pub fn max(&self) -> Vec2 {
        self.origin + self.size
    }

    pub fn fit_centered(content: Vec2, into: Vec2) -> Rect {
        if content.x <= 0.0 || content.y <= 0.0 {
            return Rect::new(into / 2.0, Vec2::ZERO);
        }
        let scale = (into.x / content.x).min(into.y / content.y);
        let size = content * scale;
        Rect::new((into - size) / 2.0, size)
    }

    pub fn fill_centered(content: Vec2, into: Vec2) -> Rect {
        if content.x <= 0.0 || content.y <= 0.0 {
            return Rect::new(into / 2.0, Vec2::ZERO);
        }
        let scale = (into.x / content.x).max(into.y / content.y);
        let size = content * scale;
        Rect::new((into - size) / 2.0, size)
    }

    pub fn round_out(&self) -> Rect {
        let origin = self.origin.round();
        Rect::new(origin, (self.origin + self.size).round() - origin)
    }

    pub fn inside(&self, area: Vec2) -> Rect {
        let far = (area - self.size).max(Vec2::ZERO);
        Rect::new(self.origin.clamp(Vec2::ZERO, far), self.size)
    }

    pub fn contains(&self, p: Vec2) -> bool {
        let m = self.max();
        p.x >= self.origin.x && p.y >= self.origin.y && p.x < m.x && p.y < m.y
    }

    pub fn contains_round(&self, p: Vec2, radius: f64) -> bool {
        self.contains_corners(p, [radius; 4])
    }

    pub fn contains_corners(&self, p: Vec2, corners: [f64; 4]) -> bool {
        if !self.contains(p) {
            return false;
        }
        let most = (self.size.x / 2.0).min(self.size.y / 2.0).max(0.0);
        let (o, m) = (self.origin, self.max());
        let [tl, tr, br, bl] = corners.map(|r| r.clamp(0.0, most));
        let centre = if p.x < o.x + tl && p.y < o.y + tl {
            Some((Vec2::new(o.x + tl, o.y + tl), tl))
        } else if p.x >= m.x - tr && p.y < o.y + tr {
            Some((Vec2::new(m.x - tr, o.y + tr), tr))
        } else if p.x >= m.x - br && p.y >= m.y - br {
            Some((Vec2::new(m.x - br, m.y - br), br))
        } else if p.x < o.x + bl && p.y >= m.y - bl {
            Some((Vec2::new(o.x + bl, m.y - bl), bl))
        } else {
            None
        };
        centre.is_none_or(|(at, radius)| (p - at).length() <= radius)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_rect_moves_back_inside_a_smaller_area() {
        let area = Vec2::new(100.0, 50.0);
        let far = Rect::new(Vec2::new(300.0, -20.0), Vec2::new(40.0, 20.0));
        assert_eq!(far.inside(area).origin, Vec2::new(60.0, 0.0));
        let fine = Rect::new(Vec2::new(10.0, 10.0), Vec2::new(40.0, 20.0));
        assert_eq!(fine.inside(area), fine);
        let huge = Rect::new(Vec2::new(30.0, 30.0), Vec2::new(400.0, 400.0));
        assert_eq!(huge.inside(area).origin, Vec2::ZERO);
    }

    use super::*;

    #[test]
    fn length_is_the_distance_from_zero() {
        assert_eq!(Vec2::new(3.0, 4.0).length(), 5.0);
        assert_eq!(Vec2::ZERO.length(), 0.0);
    }

    #[test]
    fn rounding_snaps_to_whole_pixels() {
        assert_eq!(Vec2::new(10.4, -3.6).round(), Vec2::new(10.0, -4.0));
        assert_eq!(Vec2::new(10.5, 2.5).round(), Vec2::new(11.0, 3.0));
    }

    #[test]
    fn fit_centered_fills_an_exact_match() {
        let screen = Vec2::new(1920.0, 1080.0);
        let r = Rect::fit_centered(screen, screen);
        assert_eq!(r.origin, Vec2::ZERO);
        assert_eq!(r.size, screen);
    }

    #[test]
    fn fit_centered_scales_up_and_centers_a_half_size_image() {
        let r = Rect::fit_centered(Vec2::new(960.0, 540.0), Vec2::new(1920.0, 1080.0));
        assert_eq!(r.size, Vec2::new(1920.0, 1080.0));
        assert_eq!(r.origin, Vec2::ZERO);
    }

    #[test]
    fn fit_centered_letterboxes_a_wider_image() {
        let r = Rect::fit_centered(Vec2::new(2000.0, 500.0), Vec2::new(1000.0, 1000.0));
        assert_eq!(r.size, Vec2::new(1000.0, 250.0));
        assert_eq!(r.origin, Vec2::new(0.0, 375.0));
    }

    #[test]
    fn fit_centered_pillarboxes_a_taller_image() {
        let r = Rect::fit_centered(Vec2::new(500.0, 2000.0), Vec2::new(1000.0, 1000.0));
        assert_eq!(r.size, Vec2::new(250.0, 1000.0));
        assert_eq!(r.origin, Vec2::new(375.0, 0.0));
    }

    #[test]
    fn fit_centered_never_distorts_the_aspect_ratio() {
        let content = Vec2::new(1920.0, 1080.0);
        let r = Rect::fit_centered(content, Vec2::new(3000.0, 1000.0));
        let before = content.x / content.y;
        let after = r.size.x / r.size.y;
        assert!((before - after).abs() < 1e-9);
    }

    #[test]
    fn fill_centered_covers_the_whole_box() {
        let r = Rect::fill_centered(Vec2::new(2000.0, 500.0), Vec2::new(1000.0, 1000.0));
        assert!(r.size.x >= 1000.0 && r.size.y >= 1000.0);
    }

    #[test]
    fn fill_centered_overflows_evenly_on_both_sides() {
        let r = Rect::fill_centered(Vec2::new(2000.0, 500.0), Vec2::new(1000.0, 1000.0));
        assert_eq!(r.size, Vec2::new(4000.0, 1000.0));
        assert_eq!(r.origin, Vec2::new(-1500.0, 0.0));
    }

    #[test]
    fn fill_centered_keeps_the_aspect_ratio() {
        let content = Vec2::new(1920.0, 1080.0);
        let r = Rect::fill_centered(content, Vec2::new(1000.0, 1000.0));
        assert!((content.x / content.y - r.size.x / r.size.y).abs() < 1e-9);
    }

    #[test]
    fn fill_and_fit_agree_on_an_exact_match() {
        let screen = Vec2::new(1920.0, 1080.0);
        assert_eq!(
            Rect::fill_centered(screen, screen),
            Rect::fit_centered(screen, screen)
        );
    }

    #[test]
    fn rounding_a_rect_keeps_both_edges_on_whole_pixels() {
        let rect = Rect::new(Vec2::new(10.4, 20.6), Vec2::new(100.3, 50.9));
        let snapped = rect.round_out();
        assert_eq!(snapped.origin, Vec2::new(10.0, 21.0));
        assert_eq!(snapped.max(), Vec2::new(111.0, 72.0));
    }

    #[test]
    fn a_round_rect_leaves_its_corners_out() {
        let rect = Rect::new(Vec2::new(10.0, 10.0), Vec2::new(100.0, 60.0));
        assert!(rect.contains_round(Vec2::new(60.0, 40.0), 14.0));
        assert!(rect.contains_round(Vec2::new(60.0, 10.5), 14.0), "the straight edge is in");
        assert!(!rect.contains_round(Vec2::new(11.0, 11.0), 14.0), "the top left corner is out");
        assert!(!rect.contains_round(Vec2::new(109.0, 69.0), 14.0), "the bottom right corner is out");
        assert!(rect.contains_round(Vec2::new(16.0, 16.0), 14.0), "just inside the curve is in");
        let square_bottom = [8.0, 8.0, 0.0, 0.0];
        assert!(rect.contains_corners(Vec2::new(10.5, 69.5), square_bottom), "a square corner keeps its tip");
        assert!(!rect.contains_corners(Vec2::new(10.5, 10.5), square_bottom));
    }

    #[test]
    fn fit_centered_survives_a_zero_sized_image() {
        let r = Rect::fit_centered(Vec2::ZERO, Vec2::new(1000.0, 1000.0));
        assert_eq!(r.size, Vec2::ZERO);
    }
}
