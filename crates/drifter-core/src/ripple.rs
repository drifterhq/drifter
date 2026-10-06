use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::tiles::Pyramid;
use crate::wallpaper::Placement;

pub const REACH: f64 = 300.0;
pub const SECONDS: f64 = 1.4;

pub fn region(center: Vec2) -> Rect {
    Rect::new(
        (center - Vec2::splat(REACH)).round(),
        Vec2::splat(REACH * 2.0),
    )
}

pub fn capture(pyramid: &Pyramid, laid: &Placement, area: Rect) -> Image {
    let level = pyramid
        .level_for(laid.source, laid.target)
        .min(pyramid.levels.len() - 1);
    let picture = &pyramid.levels[level];
    let shrink = f64::from(1u32 << level);
    let width = area.size.x.max(1.0) as u32;
    let height = area.size.y.max(1.0) as u32;
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    let across = laid.source.size.x / laid.target.size.x.max(1.0);
    let down = laid.source.size.y / laid.target.size.y.max(1.0);
    for y in 0..height {
        for x in 0..width {
            let screen = area.origin + Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            let sx = (laid.source.origin.x + (screen.x - laid.target.origin.x) * across) / shrink;
            let sy = (laid.source.origin.y + (screen.y - laid.target.origin.y) * down) / shrink;
            let pixel = bilinear(picture, sx - 0.5, sy - 0.5);
            let at = ((y * width + x) * 4) as usize;
            rgba[at..at + 3].copy_from_slice(&pixel.map(|v| v.round().clamp(0.0, 255.0) as u8));
            rgba[at + 3] = 255;
        }
    }
    Image {
        width,
        height,
        rgba,
    }
}

fn bilinear(image: &Image, x: f64, y: f64) -> [f64; 3] {
    let w = image.width.max(1) as i64;
    let h = image.height.max(1) as i64;
    let x0 = x.floor() as i64;
    let y0 = y.floor() as i64;
    let (fx, fy) = (x - x0 as f64, y - y0 as f64);
    let get = |px: i64, py: i64| {
        let px = px.clamp(0, w - 1) as u32;
        let py = py.clamp(0, h - 1) as u32;
        let at = ((py * image.width + px) * 4) as usize;
        let a = f64::from(image.rgba[at + 3]).max(1.0) / 255.0;
        [0, 1, 2].map(|c| f64::from(image.rgba[at + c]) / a)
    };
    let (a, b, c, d) = (
        get(x0, y0),
        get(x0 + 1, y0),
        get(x0, y0 + 1),
        get(x0 + 1, y0 + 1),
    );
    [0, 1, 2].map(|k| {
        let top = a[k] * (1.0 - fx) + b[k] * fx;
        let bottom = c[k] * (1.0 - fx) + d[k] * fx;
        top * (1.0 - fy) + bottom * fy
    })
}

pub const CELL: f64 = 2.0;
const STEPS_PER_SECOND: f64 = 200.0;
const DROP: f64 = 6.0;
const SPONGE: f64 = 30.0;
const BEND: f64 = 34.0;
const DISPERSION: [f64; 3] = [0.94, 1.0, 1.07];

pub struct Water {
    side: usize,
    height: Vec<f64>,
    speed: Vec<f64>,
    hold: Vec<f64>,
    steps: usize,
}

impl Water {
    pub fn new() -> Water {
        let side = (REACH * 2.0 / CELL) as usize;
        let middle = side as f64 / 2.0;
        let edge = middle - 1.0;
        let mut height = vec![0.0; side * side];
        let mut hold = vec![0.0; side * side];
        for y in 0..side {
            for x in 0..side {
                let dx = x as f64 + 0.5 - middle;
                let dy = y as f64 + 0.5 - middle;
                let r = (dx * dx + dy * dy).sqrt();
                let i = y * side + x;
                height[i] = 9.0 * (-(r * r) / (2.0 * DROP * DROP)).exp();
                let inside = ((edge - r) / SPONGE).clamp(0.0, 1.0);
                hold[i] = 1.0 - 0.12 * (1.0 - inside).powi(2);
                if r >= edge {
                    hold[i] = 0.0;
                }
            }
        }
        Water {
            side,
            height,
            speed: vec![0.0; side * side],
            hold,
            steps: 0,
        }
    }

    fn step(&mut self) {
        let n = self.side;
        let h = &self.height;
        for y in 1..n - 1 {
            for x in 1..n - 1 {
                let i = y * n + x;
                let around = h[i - 1] + h[i + 1] + h[i - n] + h[i + n] - 4.0 * h[i];
                let mut v = self.speed[i] + around / 4.0;
                v -= 0.005 * h[i];
                v *= 0.998;
                self.speed[i] = v * self.hold[i];
            }
        }
        for (i, value) in self.height.iter_mut().enumerate() {
            *value = (*value + self.speed[i]) * 0.999 * self.hold[i];
        }
        self.steps += 1;
    }

    pub fn advance(&mut self, t: f64) {
        let wanted = (t.clamp(0.0, 1.0) * SECONDS * STEPS_PER_SECOND) as usize;
        while self.steps < wanted {
            self.step();
        }
    }

    fn sample(&self, x: f64, y: f64) -> f64 {
        let n = self.side as i64;
        let gx = x / CELL - 0.5;
        let gy = y / CELL - 0.5;
        let x0 = gx.floor() as i64;
        let y0 = gy.floor() as i64;
        let (fx, fy) = (gx - x0 as f64, gy - y0 as f64);
        let get = |px: i64, py: i64| {
            self.height[(py.clamp(0, n - 1) * n + px.clamp(0, n - 1)) as usize]
        };
        let top = get(x0, y0) * (1.0 - fx) + get(x0 + 1, y0) * fx;
        let bottom = get(x0, y0 + 1) * (1.0 - fx) + get(x0 + 1, y0 + 1) * fx;
        top * (1.0 - fy) + bottom * fy
    }

    pub fn render(&self, source: &Image, t: f64) -> Image {
        let width = source.width;
        let height_px = source.height;
        let center = Vec2::new(f64::from(width) / 2.0, f64::from(height_px) / 2.0);
        let fade = (1.0 - t.clamp(0.0, 1.0)).powf(1.3);
        let light = {
            let raw = [-0.3f64, -0.4, 1.0];
            let len = (raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2]).sqrt();
            raw.map(|v| v / len)
        };
        let mut rgba = vec![0u8; (width * height_px * 4) as usize];
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(8);
        let band = (height_px as usize).div_ceil(threads).max(1);
        let row = (width * 4) as usize;
        let paint = |first: u32, out: &mut [u8]| {
            for y in first..first + (out.len() / row) as u32 {
                for x in 0..width {
                    let p = Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
                    let r = (p - center).length();
                    let at = ((y * width + x) * 4) as usize;
                    let here = (((y - first) * width + x) * 4) as usize;
                    let edge = (1.0 - (r / REACH).powi(6)).max(0.0) * fade;
                    if edge <= 0.0 {
                        out[here..here + 4].copy_from_slice(&source.rgba[at..at + 4]);
                        continue;
                    }
                    let grad = Vec2::new(
                        self.sample(p.x + CELL, p.y) - self.sample(p.x - CELL, p.y),
                        self.sample(p.x, p.y + CELL) - self.sample(p.x, p.y - CELL),
                    ) * (0.5 * edge);
                    if grad.length() < 0.0004 {
                        out[here..here + 4].copy_from_slice(&source.rgba[at..at + 4]);
                        continue;
                    }
                    let mut pixel = [0.0f64; 3];
                    for (channel, bend) in DISPERSION.into_iter().enumerate() {
                        let from = p + grad * (BEND * bend);
                        pixel[channel] = bilinear(source, from.x - 0.5, from.y - 0.5)[channel];
                    }
                    let normal = {
                        let raw = [-grad.x * 4.0, -grad.y * 4.0, 1.0];
                        let len = (raw[0] * raw[0] + raw[1] * raw[1] + 1.0).sqrt();
                        raw.map(|v| v / len)
                    };
                    let facing = normal[0] * light[0] + normal[1] * light[1] + normal[2] * light[2];
                    let shade = 1.0 + (facing - light[2]) * 0.35;
                    let glint = (facing.max(0.0).powf(24.0) - light[2].powf(24.0)).max(0.0) * 70.0;
                    for value in &mut pixel {
                        *value = *value * shade + glint;
                    }
                    out[here..here + 3]
                        .copy_from_slice(&pixel.map(|v| v.round().clamp(0.0, 255.0) as u8));
                    out[here + 3] = 255;
                }
            }
        };
        std::thread::scope(|scope| {
            for (n, out) in rgba.chunks_mut(band * row).enumerate() {
                let paint = &paint;
                scope.spawn(move || paint((n * band) as u32, out));
            }
        });
        Image {
            width,
            height: height_px,
            rgba,
        }
    }
}

impl Default for Water {
    fn default() -> Self {
        Water::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wave_spreads_out_and_leaves_the_edge_still() {
        let mut water = Water::new();
        let ring = |water: &Water| {
            (0..water.side / 2)
                .max_by(|a, b| {
                    let at = |r: usize| water.height[(water.side / 2) * water.side + water.side / 2 + r].abs();
                    at(*a).total_cmp(&at(*b))
                })
                .unwrap()
        };
        water.advance(0.15);
        let early = ring(&water);
        water.advance(0.5);
        assert!(ring(&water) > early, "the ring moves out");
        assert_eq!(water.height[0], 0.0, "the corner never moves");
    }

    #[test]
    fn a_frame_keeps_the_patch_edges_and_moves_the_middle() {
        let mut rgba = Vec::new();
        let side = (REACH * 2.0) as u32;
        for y in 0..side {
            for x in 0..side {
                rgba.extend([(x % 256) as u8, (y % 256) as u8, 128, 255]);
            }
        }
        let source = Image { width: side, height: side, rgba };
        let mut water = Water::new();
        water.advance(0.25);
        let out = water.render(&source, 0.25);
        assert_eq!(&out.rgba[0..4], &source.rgba[0..4], "the corner is untouched");
        assert_ne!(out.rgba, source.rgba, "something moved");
        assert!(out.rgba.chunks(4).all(|p| p[3] == 255), "the patch stays solid");
        assert_eq!(water.render(&source, 1.0).rgba, source.rgba, "still water at the end");
    }
}
