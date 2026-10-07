use crate::config::IconShape;
use crate::image::Image;

const SQUIRCLE: f64 = 5.0;
const INSET: f64 = 0.78;
const EDGE: f64 = 0.06;
const SOLID: u8 = 200;

/// Takes and gives premultiplied art, like `image::load`.
pub fn dress(image: &Image, shape: IconShape, background: bool) -> Image {
    let side = image.width.min(image.height).max(1);
    let mut image = image.scaled(side, side);
    image.straighten();
    let backed = background && clear_part(&image) > 0.02;
    let art = match backed && clear_edge(&image) {
        true => inset(&image, INSET),
        false => image.clone(),
    };
    let behind = backed.then(|| backing(&image));
    let mut out = art.clone();
    for y in 0..side {
        for x in 0..side {
            let at = ((y * side + x) * 4) as usize;
            let pixel = &mut out.rgba[at..at + 4];
            if let Some(color) = behind {
                let a = f64::from(pixel[3]) / 255.0;
                for (channel, under) in pixel[..3].iter_mut().zip(color) {
                    *channel = (f64::from(*channel) * a + f64::from(under) * (1.0 - a)).round() as u8;
                }
                pixel[3] = 255;
            }
            let keep = cover(shape, side, x, y);
            pixel[3] = (f64::from(pixel[3]) * keep).round() as u8;
        }
    }
    out.premultiply();
    out
}

fn cover(shape: IconShape, side: u32, x: u32, y: u32) -> f64 {
    let r = f64::from(side) / 2.0;
    let dx = (f64::from(x) + 0.5 - r).abs();
    let dy = (f64::from(y) + 0.5 - r).abs();
    let inside = match shape {
        IconShape::Square => return 1.0,
        IconShape::Circle => r - (dx * dx + dy * dy).sqrt(),
        IconShape::Squircle => {
            let reach = ((dx / r).powf(SQUIRCLE) + (dy / r).powf(SQUIRCLE)).powf(1.0 / SQUIRCLE);
            (1.0 - reach) * r
        }
    };
    (inside + 0.5).clamp(0.0, 1.0)
}

fn clear_part(image: &Image) -> f64 {
    let pixels = image.rgba.as_chunks::<4>().0;
    let clear = pixels.iter().filter(|pixel| pixel[3] < SOLID).count();
    clear as f64 / pixels.len().max(1) as f64
}

fn clear_edge(image: &Image) -> bool {
    let side = image.width;
    let band = ((f64::from(side) * EDGE).ceil() as u32).max(1);
    let (mut sum, mut count) = (0u64, 0u64);
    for y in 0..side {
        for x in 0..side {
            let near = x < band || y < band || x >= side - band || y >= side - band;
            if near {
                sum += u64::from(image.rgba[((y * side + x) * 4 + 3) as usize]);
                count += 1;
            }
        }
    }
    sum < count.max(1) * 128
}

fn backing(image: &Image) -> [u8; 3] {
    let mut sum = [0.0f64; 3];
    let mut weight = 0.0;
    for pixel in image.rgba.as_chunks::<4>().0 {
        let a = f64::from(pixel[3]) / 255.0;
        for (total, channel) in sum.iter_mut().zip(&pixel[..3]) {
            *total += f64::from(*channel) * a;
        }
        weight += a;
    }
    if weight < 1.0 {
        return [236, 238, 242];
    }
    let mean = sum.map(|total| total / weight);
    let light = (0.2126 * mean[0] + 0.7152 * mean[1] + 0.0722 * mean[2]) / 255.0;
    let (toward, keep) = match light > 0.7 {
        true => ([30.0, 32.0, 38.0], 0.25),
        false => ([255.0, 255.0, 255.0], 0.18),
    };
    [0, 1, 2].map(|at| (mean[at] * keep + toward[at] * (1.0 - keep)).round() as u8)
}

fn inset(image: &Image, part: f64) -> Image {
    let side = image.width;
    let small = ((f64::from(side) * part).round() as u32).max(1);
    let shrunk = shrink(image, small);
    let mut out = Image {
        width: side,
        height: side,
        rgba: vec![0; (side * side * 4) as usize],
    };
    let off = (side - small) / 2;
    for y in 0..small {
        let from = (y * small * 4) as usize;
        let to = (((y + off) * side + off) * 4) as usize;
        out.rgba[to..to + (small * 4) as usize].copy_from_slice(&shrunk.rgba[from..from + (small * 4) as usize]);
    }
    out
}

fn shrink(image: &Image, small: u32) -> Image {
    let side = image.width;
    let step = f64::from(side) / f64::from(small);
    let mut rgba = vec![0u8; (small * small * 4) as usize];
    for y in 0..small {
        for x in 0..small {
            let (y0, y1) = span(y, step, side);
            let (x0, x1) = span(x, step, side);
            let mut total = [0.0f64; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let at = ((sy * side + sx) * 4) as usize;
                    let a = f64::from(image.rgba[at + 3]);
                    for (sum, channel) in total.iter_mut().zip(&image.rgba[at..at + 3]) {
                        *sum += f64::from(*channel) * a;
                    }
                    total[3] += a;
                }
            }
            let count = f64::from((y1 - y0) * (x1 - x0));
            let at = ((y * small + x) * 4) as usize;
            if total[3] > 0.0 {
                for c in 0..3 {
                    rgba[at + c] = (total[c] / total[3]).round() as u8;
                }
            }
            rgba[at + 3] = (total[3] / count).round() as u8;
        }
    }
    Image {
        width: small,
        height: small,
        rgba,
    }
}

fn span(at: u32, step: f64, side: u32) -> (u32, u32) {
    let from = (f64::from(at) * step).floor() as u32;
    let to = ((f64::from(at + 1) * step).ceil() as u32).min(side);
    (from.min(side - 1), to.max(from + 1).min(side))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filled(side: u32, pixel: [u8; 4]) -> Image {
        Image {
            width: side,
            height: side,
            rgba: pixel.repeat((side * side) as usize),
        }
    }

    fn alpha(image: &Image, x: u32, y: u32) -> u8 {
        image.rgba[((y * image.width + x) * 4 + 3) as usize]
    }

    #[test]
    fn each_shape_cuts_its_own_corners() {
        let solid = filled(64, [200, 40, 40, 255]);
        let square = dress(&solid, IconShape::Square, true);
        let circle = dress(&solid, IconShape::Circle, true);
        let squircle = dress(&solid, IconShape::Squircle, true);
        assert_eq!(alpha(&square, 0, 0), 255);
        assert_eq!(alpha(&circle, 0, 0), 0);
        assert_eq!(alpha(&squircle, 0, 0), 0);
        assert_eq!(alpha(&circle, 32, 32), 255);
        assert_eq!(alpha(&circle, 6, 6), 0, "a circle cuts deep into the corner");
        assert_eq!(alpha(&squircle, 6, 6), 255, "a squircle keeps more of the corner");
    }

    #[test]
    fn a_clear_icon_gets_a_backing_color_unless_switched_off() {
        let mut icon = filled(64, [0, 0, 0, 0]);
        for y in 24..40 {
            for x in 24..40 {
                let at = ((y * 64 + x) * 4) as usize;
                icon.rgba[at..at + 4].copy_from_slice(&[20, 60, 200, 255]);
            }
        }
        let backed = dress(&icon, IconShape::Square, true);
        assert_eq!(alpha(&backed, 2, 2), 255);
        let back = &backed.rgba[(2 * 64 + 2) * 4..(2 * 64 + 2) * 4 + 3];
        assert!(back.iter().all(|channel| *channel > 180), "a dark icon sits on a light tint: {back:?}");
        assert!(back[2] > back[0], "the tint leans to the icon's own color: {back:?}");
        let bare = dress(&icon, IconShape::Square, false);
        assert_eq!(alpha(&bare, 2, 2), 0);
    }

    #[test]
    fn a_light_icon_sits_on_a_dark_backing() {
        let mut icon = filled(32, [0, 0, 0, 0]);
        for at in (0..32 * 32).filter(|at| at % 3 == 0) {
            icon.rgba[at * 4..at * 4 + 4].copy_from_slice(&[250, 250, 250, 255]);
        }
        let color = backing(&icon);
        assert!(color.iter().all(|channel| *channel < 100), "{color:?}");
    }
}
