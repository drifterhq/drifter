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

    pub fn contains(&self, p: Vec2) -> bool {
        let m = self.max();
        p.x >= self.origin.x && p.y >= self.origin.y && p.x < m.x && p.y < m.y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn fit_centered_survives_a_zero_sized_image() {
        let r = Rect::fit_centered(Vec2::ZERO, Vec2::new(1000.0, 1000.0));
        assert_eq!(r.size, Vec2::ZERO);
    }
}
