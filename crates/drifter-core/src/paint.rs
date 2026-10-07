use crate::geom::{Rect, Vec2};
use crate::image::Image;

pub struct Canvas {
    pub image: Image,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Canvas {
        let width = width.max(1);
        let height = height.max(1);
        Canvas {
            image: Image {
                width,
                height,
                rgba: vec![0; (width as usize) * (height as usize) * 4],
            },
        }
    }

    pub fn size(&self) -> Vec2 {
        self.image.size()
    }

    pub fn round_rect(&mut self, rect: Rect, radius: f64, color: [u8; 3], alpha: f64) {
        let radius = radius
            .min(rect.size.x / 2.0)
            .min(rect.size.y / 2.0)
            .max(0.0);
        self.shape(rect, color, alpha, |p| {
            let local = p - rect.origin;
            inside_round(local, rect.size, radius)
        });
    }

    pub fn circle(&mut self, center: Vec2, radius: f64, color: [u8; 3], alpha: f64) {
        let rect = Rect::from_center(center, Vec2::splat(radius * 2.0));
        self.shape(rect, color, alpha, |p| radius - (p - center).length());
    }

    pub fn triangle(&mut self, corners: [Vec2; 3], color: [u8; 3], alpha: f64) {
        let [a, b, c] = corners;
        let low = a.min(b).min(c);
        let high = a.max(b).max(c);
        let turn = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
        let sign = if turn < 0.0 { -1.0 } else { 1.0 };
        let edge = |p: Vec2, from: Vec2, to: Vec2| {
            let along = to - from;
            let length = along.length().max(1e-9);
            sign * (along.x * (p.y - from.y) - along.y * (p.x - from.x)) / length
        };
        self.shape(Rect::new(low, high - low), color, alpha, |p| {
            edge(p, a, b).min(edge(p, b, c)).min(edge(p, c, a))
        });
    }

    pub fn shadow(&mut self, rect: Rect, radius: f64, blur: f64, alpha: f64) {
        let grown = Rect::new(
            rect.origin - Vec2::splat(blur),
            rect.size + Vec2::splat(blur * 2.0),
        );
        let radius = radius
            .min(rect.size.x / 2.0)
            .min(rect.size.y / 2.0)
            .max(0.0);
        self.blend_each(grown, [0, 0, 0], |p| {
            let local = p - rect.origin;
            let edge = inside_round(local, rect.size, radius);
            let fade = ((edge + blur) / (blur * 2.0)).clamp(0.0, 1.0);
            fade * fade * alpha
        });
    }

    pub fn stamp(&mut self, piece: &Image, at: Vec2, alpha: f64) {
        let left = at.x.round() as i64;
        let top = at.y.round() as i64;
        for y in 0..piece.height {
            let ty = top + i64::from(y);
            if ty < 0 || ty >= i64::from(self.image.height) {
                continue;
            }
            for x in 0..piece.width {
                let tx = left + i64::from(x);
                if tx < 0 || tx >= i64::from(self.image.width) {
                    continue;
                }
                let from = ((y * piece.width + x) * 4) as usize;
                let a = f64::from(piece.rgba[from + 3]) / 255.0 * alpha;
                if a <= 0.0 {
                    continue;
                }
                let to = ((ty * i64::from(self.image.width) + tx) * 4) as usize;
                if a >= 1.0 {
                    self.image.rgba[to..to + 4].copy_from_slice(&piece.rgba[from..from + 4]);
                    continue;
                }
                for channel in 0..3 {
                    let src = f64::from(piece.rgba[from + channel]) * alpha;
                    let dst = f64::from(self.image.rgba[to + channel]);
                    self.image.rgba[to + channel] = byte(src + dst * (1.0 - a));
                }
                let dst = f64::from(self.image.rgba[to + 3]) / 255.0;
                self.image.rgba[to + 3] = byte((a + dst * (1.0 - a)) * 255.0);
            }
        }
    }

    fn shape(&mut self, rect: Rect, color: [u8; 3], alpha: f64, edge: impl Fn(Vec2) -> f64) {
        self.blend_each(rect, color, |p| (edge(p) + 0.5).clamp(0.0, 1.0) * alpha);
    }

    fn blend_each(&mut self, rect: Rect, color: [u8; 3], cover: impl Fn(Vec2) -> f64) {
        let from_x = rect.origin.x.floor().max(0.0) as u32;
        let from_y = rect.origin.y.floor().max(0.0) as u32;
        let to_x = (rect.max().x.ceil().max(0.0) as u32).min(self.image.width);
        let to_y = (rect.max().y.ceil().max(0.0) as u32).min(self.image.height);
        for y in from_y..to_y {
            for x in from_x..to_x {
                let a = cover(Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5));
                if a <= 0.0 {
                    continue;
                }
                let at = ((y * self.image.width + x) * 4) as usize;
                if a >= 1.0 {
                    self.image.rgba[at..at + 3].copy_from_slice(&color);
                    self.image.rgba[at + 3] = 255;
                    continue;
                }
                for (channel, value) in color.iter().enumerate() {
                    let dst = f64::from(self.image.rgba[at + channel]);
                    self.image.rgba[at + channel] = byte(f64::from(*value) * a + dst * (1.0 - a));
                }
                let dst = f64::from(self.image.rgba[at + 3]) / 255.0;
                self.image.rgba[at + 3] = byte((a + dst * (1.0 - a)) * 255.0);
            }
        }
    }
}

fn byte(value: f64) -> u8 {
    (value + 0.5) as u8
}

fn inside_round(p: Vec2, size: Vec2, radius: f64) -> f64 {
    let cx = p.x.clamp(radius, size.x - radius);
    let cy = p.y.clamp(radius, size.y - radius);
    let corner = p.x < radius || p.x > size.x - radius;
    let corner = corner && (p.y < radius || p.y > size.y - radius);
    if corner {
        return radius - (p - Vec2::new(cx, cy)).length();
    }
    p.x.min(size.x - p.x).min(p.y).min(size.y - p.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alpha(canvas: &Canvas, x: u32, y: u32) -> u8 {
        canvas.image.rgba[((y * canvas.image.width + x) * 4 + 3) as usize]
    }

    #[test]
    fn a_round_rect_fills_its_middle_and_cuts_its_corners() {
        let mut canvas = Canvas::new(40, 40);
        canvas.round_rect(
            Rect::new(Vec2::ZERO, Vec2::splat(40.0)),
            12.0,
            [255, 0, 0],
            1.0,
        );
        assert_eq!(alpha(&canvas, 20, 20), 255);
        assert_eq!(alpha(&canvas, 0, 0), 0);
        assert_eq!(alpha(&canvas, 20, 0), 255);
    }

    #[test]
    fn a_circle_is_round() {
        let mut canvas = Canvas::new(20, 20);
        canvas.circle(Vec2::splat(10.0), 8.0, [255, 255, 255], 1.0);
        assert_eq!(alpha(&canvas, 10, 10), 255);
        assert_eq!(alpha(&canvas, 1, 1), 0);
        assert!(alpha(&canvas, 10, 2) > 100);
    }

    #[test]
    fn half_alpha_over_an_opaque_ground_stays_opaque() {
        let mut canvas = Canvas::new(4, 4);
        let all = Rect::new(Vec2::ZERO, Vec2::splat(4.0));
        canvas.round_rect(all, 0.0, [0, 0, 0], 1.0);
        canvas.round_rect(all, 0.0, [200, 200, 200], 0.5);
        assert_eq!(alpha(&canvas, 2, 2), 255);
        assert_eq!(canvas.image.rgba[((2 * 4 + 2) * 4) as usize], 100);
    }

    #[test]
    fn a_triangle_fills_inside_and_leaves_outside() {
        let mut canvas = Canvas::new(20, 20);
        let up = [
            Vec2::new(10.0, 2.0),
            Vec2::new(18.0, 18.0),
            Vec2::new(2.0, 18.0),
        ];
        canvas.triangle(up, [255, 255, 255], 1.0);
        assert_eq!(alpha(&canvas, 10, 14), 255);
        assert_eq!(alpha(&canvas, 2, 3), 0);
        let mut flipped = Canvas::new(20, 20);
        flipped.triangle([up[0], up[2], up[1]], [255, 255, 255], 1.0);
        assert_eq!(flipped.image.rgba, canvas.image.rgba);
    }

    #[test]
    fn every_pixel_stays_premultiplied() {
        let mut canvas = Canvas::new(30, 30);
        canvas.shadow(
            Rect::new(Vec2::splat(8.0), Vec2::splat(14.0)),
            7.0,
            5.0,
            0.4,
        );
        canvas.circle(Vec2::splat(15.0), 7.0, [255, 200, 100], 0.7);
        for pixel in canvas.image.rgba.chunks(4) {
            assert!(pixel[0] <= pixel[3] && pixel[1] <= pixel[3] && pixel[2] <= pixel[3]);
        }
    }
}
