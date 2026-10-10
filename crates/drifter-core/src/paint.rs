use crate::geom::{Rect, Vec2};
use crate::image::Image;

thread_local! {
    static DENSITY: std::cell::Cell<u32> = const { std::cell::Cell::new(1) };
}

pub fn density() -> f64 {
    f64::from(DENSITY.with(|density| density.get()))
}

pub fn set_density(density: u32) {
    DENSITY.with(|cell| cell.set(density.clamp(1, 4)));
}

pub fn at_density<T>(density: u32, work: impl FnOnce() -> T) -> T {
    let was = DENSITY.with(|cell| cell.get());
    set_density(density);
    let out = work();
    set_density(was);
    out
}

pub fn dense(logical: f64) -> u32 {
    (logical * density()).round().max(1.0) as u32
}

pub fn logical(image: &Image) -> Vec2 {
    image.size() / density()
}

fn up(rect: Rect) -> Rect {
    let d = density();
    Rect::new(rect.origin * d, rect.size * d)
}

pub struct Canvas {
    pub image: Image,
    pub clip: Option<[u32; 4]>,
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Canvas {
        let width = dense(f64::from(width.max(1)));
        let height = dense(f64::from(height.max(1)));
        Canvas {
            image: Image {
                width,
                height,
                rgba: vec![0; (width as usize) * (height as usize) * 4],
            },
            clip: None,
        }
    }

    pub fn on(image: Image) -> Canvas {
        Canvas { image, clip: None }
    }

    pub fn clip_to(&mut self, area: Option<Rect>) {
        self.clip = area.map(|area| {
            let area = up(area);
            let (w, h) = (self.image.width, self.image.height);
            [
                (area.origin.x.floor().max(0.0) as u32).min(w),
                (area.origin.y.floor().max(0.0) as u32).min(h),
                (area.max().x.ceil().max(0.0) as u32).min(w),
                (area.max().y.ceil().max(0.0) as u32).min(h),
            ]
        });
    }

    fn bounds(&self, from_x: u32, from_y: u32, to_x: u32, to_y: u32) -> (u32, u32, u32, u32) {
        match self.clip {
            Some([x0, y0, x1, y1]) => (from_x.max(x0), from_y.max(y0), to_x.min(x1), to_y.min(y1)),
            None => (from_x, from_y, to_x, to_y),
        }
    }

    pub fn size(&self) -> Vec2 {
        logical(&self.image)
    }

    pub fn round_rect(&mut self, rect: Rect, radius: f64, color: [u8; 3], alpha: f64) {
        let (rect, radius) = (up(rect), radius * density());
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
        let (center, radius) = (center * density(), radius * density());
        let rect = Rect::from_center(center, Vec2::splat(radius * 2.0));
        self.shape(rect, color, alpha, |p| radius - (p - center).length());
    }

    pub fn triangle(&mut self, corners: [Vec2; 3], color: [u8; 3], alpha: f64) {
        let [a, b, c] = corners.map(|corner| corner * density());
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
        let (rect, radius, blur) = (up(rect), radius * density(), blur * density());
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
        let at = at * density();
        let left = (at.x + 0.5).floor() as i64;
        let top = (at.y + 0.5).floor() as i64;
        let (low_x, low_y, high_x, high_y) = self.bounds(0, 0, self.image.width, self.image.height);
        let (low_x, low_y, high_x, high_y) = (i64::from(low_x), i64::from(low_y), i64::from(high_x), i64::from(high_y));
        let first_y = (low_y - top).clamp(0, i64::from(piece.height)) as u32;
        let last_y = (high_y - top).clamp(0, i64::from(piece.height)) as u32;
        let first_x = (low_x - left).clamp(0, i64::from(piece.width)) as u32;
        let last_x = (high_x - left).clamp(0, i64::from(piece.width)) as u32;
        for y in first_y..last_y {
            let ty = top + i64::from(y);
            for x in first_x..last_x {
                let tx = left + i64::from(x);
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

    pub fn line(&mut self, from: Vec2, to: Vec2, width: f64, color: [u8; 3], alpha: f64) {
        let (a, b, half) = (from * density(), to * density(), width * density() / 2.0);
        let low = a.min(b) - Vec2::splat(half + 1.0);
        let high = a.max(b) + Vec2::splat(half + 1.0);
        let along = b - a;
        let span = (along.x * along.x + along.y * along.y).max(1e-9);
        self.shape(Rect::new(low, high - low), color, alpha, |p| {
            let rel = p - a;
            let t = ((rel.x * along.x + rel.y * along.y) / span).clamp(0.0, 1.0);
            half - (rel - along * t).length()
        });
    }

    pub fn ring(&mut self, rect: Rect, radius: f64, width: f64, color: [u8; 3], alpha: f64) {
        let (rect, radius, width) = (up(rect), radius * density(), width * density());
        let radius = radius.min(rect.size.x / 2.0).min(rect.size.y / 2.0).max(0.0);
        self.shape(rect, color, alpha, |p| {
            let edge = inside_round(p - rect.origin, rect.size, radius);
            edge.min(width - edge)
        });
    }

    pub fn arc(&mut self, center: Vec2, radius: f64, width: f64, share: f64, color: [u8; 3], alpha: f64) {
        let share = share.clamp(0.0, 1.0);
        if share <= 0.0 {
            return;
        }
        let (center, radius, half) = (center * density(), radius * density(), width * density() / 2.0);
        let sweep = share * std::f64::consts::TAU;
        let at = |turn: f64| center + Vec2::new(turn.sin(), -turn.cos()) * radius;
        let (start, end) = (at(0.0), at(sweep));
        let rect = Rect::from_center(center, Vec2::splat((radius + half + 1.0) * 2.0));
        self.shape(rect, color, alpha, |p| {
            let rel = p - center;
            let turn = rel.x.atan2(-rel.y).rem_euclid(std::f64::consts::TAU);
            let band = half - (rel.length() - radius).abs();
            match share >= 1.0 || turn <= sweep {
                true => band,
                false => (half - (p - start).length()).max(half - (p - end).length()),
            }
        });
    }

    pub fn glow(&mut self, center: Vec2, radius: f64, color: [u8; 3], alpha: f64) {
        let (center, radius) = (center * density(), radius * density());
        let rect = Rect::from_center(center, Vec2::splat(radius * 2.0));
        self.blend_each(rect, color, |p| {
            let t = (1.0 - (p - center).length() / radius).clamp(0.0, 1.0);
            t * t * alpha
        });
    }

    pub fn shade(&mut self, rect: Rect, radius: f64, top: [u8; 3], bottom: [u8; 3], alpha: f64) {
        let (rect, radius) = (up(rect), radius * density());
        let radius = radius.min(rect.size.x / 2.0).min(rect.size.y / 2.0).max(0.0);
        let from_y = rect.origin.y.floor().max(0.0) as u32;
        let to_y = (rect.max().y.ceil().max(0.0) as u32).min(self.image.height);
        for y in from_y..to_y {
            let t = ((f64::from(y) + 0.5 - rect.origin.y) / rect.size.y.max(1.0)).clamp(0.0, 1.0);
            let color = [0, 1, 2].map(|c| (f64::from(top[c]) + (f64::from(bottom[c]) - f64::from(top[c])) * t).round() as u8);
            let row = Rect::new(Vec2::new(rect.origin.x, f64::from(y)), Vec2::new(rect.size.x, 1.0));
            self.shape(row, color, alpha, |p| inside_round(p - rect.origin, rect.size, radius));
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
        let (from_x, from_y, to_x, to_y) = self.bounds(from_x, from_y, to_x, to_y);
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
    fn a_clip_keeps_paint_inside_it() {
        let mut canvas = Canvas::new(20, 20);
        canvas.clip_to(Some(Rect::new(Vec2::new(5.0, 5.0), Vec2::splat(10.0))));
        canvas.round_rect(Rect::new(Vec2::ZERO, Vec2::splat(20.0)), 0.0, [255, 255, 255], 1.0);
        let mut piece = Canvas::new(20, 20);
        piece.round_rect(Rect::new(Vec2::ZERO, Vec2::splat(20.0)), 0.0, [9, 9, 9], 1.0);
        canvas.stamp(&piece.image, Vec2::new(-3.0, 2.0), 1.0);
        assert_eq!(alpha(&canvas, 2, 2), 0);
        assert_eq!(alpha(&canvas, 17, 17), 0);
        assert_eq!(alpha(&canvas, 6, 6), 255);
        assert_eq!(canvas.image.rgba[((10 * 20 + 10) * 4) as usize], 9);
        assert_eq!(alpha(&canvas, 16, 10), 0);
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

#[cfg(test)]
pub fn same_at_two(what: &str, paint: impl Fn() -> Image) {
    let one = at_density(1, &paint);
    let two = at_density(2, &paint);
    assert_eq!(
        (two.width, two.height),
        (one.width * 2, one.height * 2),
        "{what}: density 2 is twice the size"
    );
    let mut total = 0u64;
    let mut far = 0usize;
    for y in 0..one.height {
        for x in 0..one.width {
            for channel in 0..4 {
                let mut sum = 0u32;
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let at = (((y * 2 + dy) * two.width + x * 2 + dx) * 4) as usize + channel;
                    sum += u32::from(two.rgba[at]);
                }
                let shrunk = (sum + 2) / 4;
                let was = u32::from(one.rgba[((y * one.width + x) * 4) as usize + channel]);
                let gap = shrunk.abs_diff(was);
                total += u64::from(gap);
                if gap > 96 {
                    far += 1;
                }
            }
        }
    }
    let count = (one.width * one.height * 4) as f64;
    let mean = total as f64 / count;
    let wild = far as f64 / count;
    if mean >= 8.0 || wild >= 0.01 {
        let dir = std::env::temp_dir();
        let _ = crate::image::save_png(&one, &dir.join(format!("density-{what}-1.png")));
        let _ = crate::image::save_png(&two, &dir.join(format!("density-{what}-2.png")));
    }
    assert!(
        mean < 8.0 && wild < 0.01,
        "{what}: density 2 does not match density 1 (mean {mean:.2}, wild {:.2}%)",
        wild * 100.0
    );
}
