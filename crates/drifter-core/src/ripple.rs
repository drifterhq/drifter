use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::tiles::Pyramid;
use crate::wallpaper::Placement;

pub const REACH: f64 = 300.0;
pub const SECONDS: f64 = 0.8;

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

const TRAINS: [(f64, f64, f64); 4] = [
    (0.055, 1.0, 1.0),
    (0.085, 0.75, 0.82),
    (0.125, 0.55, 0.66),
    (0.18, 0.35, 0.52),
];

const DISPERSION: [f64; 3] = [0.93, 1.0, 1.08];
const WATER_R0: f64 = 0.02;
const ABSORB: [f64; 3] = [0.012, 0.005, 0.002];
const SKY: [f64; 3] = [232.0, 240.0, 255.0];

pub fn height(r: f64, t: f64) -> f64 {
    if !(0.0..1.0).contains(&t) {
        return 0.0;
    }
    let edge = (1.0 - (r / REACH).powi(6)).max(0.0);
    if edge == 0.0 {
        return 0.0;
    }
    let fade = (1.0 - t).powf(1.6);
    let mut sum = 0.0;
    for (wavenumber, strength, speed) in TRAINS {
        let front = REACH * 0.95 * speed * (1.0 - (1.0 - t).powi(2));
        let gap = r - front;
        let width = 26.0 + 60.0 * t * speed;
        let packet = (-(gap * gap) / (2.0 * width * width)).exp();
        let spread = 1.0 / (1.0 + r / 90.0).sqrt();
        sum += strength * packet * spread * (wavenumber * gap).cos();
    }
    let splash = -2.2 * (-(r * r) / (2.0 * 14.0 * 14.0)).exp() * (1.0 - t * 4.0).max(0.0);
    (sum * 3.2 + splash) * fade * edge
}

pub fn wave(r: f64, t: f64) -> f64 {
    height(r, t)
}

pub fn frame(source: &Image, t: f64) -> Image {
    let width = source.width;
    let height_px = source.height;
    let center = Vec2::new(f64::from(width) / 2.0, f64::from(height_px) / 2.0);
    let reach = (REACH as usize) + 2;
    let profile: Vec<f64> = (0..reach * 2)
        .map(|half| height(half as f64 * 0.5, t))
        .collect();
    let at_r = |r: f64| {
        let x = (r * 2.0).clamp(0.0, (profile.len() - 2) as f64);
        let i = x.floor() as usize;
        let f = x - i as f64;
        profile[i] * (1.0 - f) + profile[i + 1] * f
    };
    let light = {
        let raw = [-0.45f64, -0.55, 0.70];
        let len = (raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2]).sqrt();
        raw.map(|v| v / len)
    };
    let depth = 26.0;
    let mut rgba = vec![0u8; (width * height_px * 4) as usize];
    let threads = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(8);
    let band = (height_px as usize).div_ceil(threads).max(1);
    let row = (width * 4) as usize;
    let paint = |first: u32, out: &mut [u8]| {
        for y in first..first + (out.len() / row) as u32 {
            for x in 0..width {
                let p = Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
                let away = p - center;
                let r = away.length();
                let at = ((y * width + x) * 4) as usize;
                let here = (((y - first) * width + x) * 4) as usize;
                if r >= REACH - 1.0 {
                    out[here..here + 4].copy_from_slice(&source.rgba[at..at + 4]);
                    continue;
                }
                let slope = (at_r(r + 0.5) - at_r((r - 0.5).max(0.0))) / 1.0;
                if slope.abs() < 0.0005 && at_r(r).abs() < 0.01 {
                    out[here..here + 4].copy_from_slice(&source.rgba[at..at + 4]);
                    continue;
                }
                let dir = if r > 0.01 { away / r } else { Vec2::ZERO };
                let grad = dir * slope;
                let mut pixel = [0.0f64; 3];
                for (channel, bend) in DISPERSION.into_iter().enumerate() {
                    let from = p - grad * (depth * bend);
                    pixel[channel] = bilinear(source, from.x - 0.5, from.y - 0.5)[channel];
                }
                let normal = {
                    let raw = [-grad.x, -grad.y, 1.0];
                    let len = (raw[0] * raw[0] + raw[1] * raw[1] + 1.0).sqrt();
                    raw.map(|v| v / len)
                };
                let facing = normal[0] * light[0] + normal[1] * light[1] + normal[2] * light[2];
                let shade = 1.0 + (facing - light[2]) * 0.55;
                let cosine = normal[2].clamp(0.0, 1.0);
                let fresnel = WATER_R0 + (1.0 - WATER_R0) * (1.0 - cosine).powi(5);
                let mirror = ((fresnel - WATER_R0) * 6.0).clamp(0.0, 0.15);
                let thickness = -at_r(r) * 2.0;
                let half = {
                    let raw = [light[0], light[1], light[2] + 1.0];
                    let len = (raw[0] * raw[0] + raw[1] * raw[1] + raw[2] * raw[2]).sqrt();
                    raw.map(|v| v / len)
                };
                let gloss =
                    (normal[0] * half[0] + normal[1] * half[1] + normal[2] * half[2]).max(0.0);
                let base_gloss = half[2];
                let spark = ((gloss - base_gloss).max(0.0) / (1.0 - base_gloss)).powf(2.0) * 38.0;
                for channel in 0..3 {
                    let through = pixel[channel]
                        * shade
                        * (-ABSORB[channel] * thickness).exp().clamp(0.85, 1.15);
                    pixel[channel] = through * (1.0 - mirror) + SKY[channel] * mirror + spark;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wave_spreads_out_fades_and_leaves_the_edge_still() {
        let peak = |t: f64| {
            (0..300)
                .map(|r| wave(f64::from(r), t).abs())
                .fold(0.0, f64::max)
        };
        let where_at = |t: f64| {
            (0..300)
                .max_by(|a, b| {
                    wave(f64::from(*a), t)
                        .abs()
                        .total_cmp(&wave(f64::from(*b), t).abs())
                })
                .unwrap()
        };
        assert!(where_at(0.6) > where_at(0.2), "the ring moves out");
        assert!(peak(0.8) < peak(0.2), "and fades");
        assert_eq!(wave(REACH, 0.5), 0.0, "the edge of the patch never moves");
        assert_eq!(wave(100.0, 1.0), 0.0, "done at the end");
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
        let source = Image {
            width: side,
            height: side,
            rgba,
        };
        let out = frame(&source, 0.25);
        assert_eq!(
            &out.rgba[0..4],
            &source.rgba[0..4],
            "the corner is untouched"
        );
        assert_ne!(out.rgba, source.rgba, "something moved");
        assert!(
            out.rgba.chunks(4).all(|p| p[3] == 255),
            "the patch stays solid"
        );
    }
}
