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

    pub fn contains(&self, p: Vec2) -> bool {
        let m = self.max();
        p.x >= self.origin.x && p.y >= self.origin.y && p.x < m.x && p.y < m.y
    }
}
