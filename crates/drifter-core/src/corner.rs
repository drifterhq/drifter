use crate::geom::Vec2;
use crate::image::Image;

pub const REACH: f64 = 128.0;

pub fn heat(pointer: Vec2, reach: f64) -> f64 {
    if reach <= 0.0 {
        return 0.0;
    }
    let away = (pointer.x * pointer.x + pointer.y * pointer.y).sqrt();
    let near = 1.0 - (away / reach).clamp(0.0, 1.0);
    near * near
}

pub fn paint_glow(side: u32, color: [u8; 3]) -> Image {
    let side = side.max(2);
    let reach = f64::from(side);
    let mut rgba = vec![0u8; (side as usize) * (side as usize) * 4];

    for y in 0..side {
        let py = f64::from(y) + 0.5;
        for x in 0..side {
            let px = f64::from(x) + 0.5;
            let ink = heat(Vec2::new(px, py), reach);
            if ink <= 0.0 {
                continue;
            }
            let index = ((y as usize) * (side as usize) + x as usize) * 4;
            for (channel, value) in color.iter().enumerate() {
                rgba[index + channel] = (f64::from(*value) * ink).round().clamp(0.0, 255.0) as u8;
            }
            rgba[index + 3] = (ink * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }

    Image {
        width: side,
        height: side,
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_corner_itself_is_the_hottest_point() {
        assert_eq!(heat(Vec2::ZERO, REACH), 1.0);
    }

    #[test]
    fn heat_falls_off_with_distance() {
        let near = heat(Vec2::new(10.0, 10.0), REACH);
        let far = heat(Vec2::new(90.0, 90.0), REACH);
        assert!(near > far);
        assert!(far > 0.0);
    }

    #[test]
    fn heat_is_gone_past_the_reach() {
        assert_eq!(heat(Vec2::new(REACH, 0.0), REACH), 0.0);
        assert_eq!(heat(Vec2::new(200.0, 200.0), REACH), 0.0);
    }

    #[test]
    fn heat_is_round_rather_than_square() {
        let side = heat(Vec2::new(100.0, 0.0), REACH);
        let corner = heat(Vec2::new(100.0, 100.0), REACH);
        assert!(
            side > corner,
            "the falloff follows the box, not the distance"
        );
    }

    #[test]
    fn a_reach_of_nothing_is_never_warm() {
        assert_eq!(heat(Vec2::ZERO, 0.0), 0.0);
    }

    #[test]
    fn the_glow_is_brightest_in_its_own_corner() {
        let image = paint_glow(64, [255, 220, 170]);
        let first = image.rgba[3];
        let last = image.rgba[((63 * 64 + 63) * 4) + 3];
        assert!(first > 200);
        assert_eq!(last, 0);
    }

    #[test]
    fn every_glow_pixel_is_premultiplied() {
        let image = paint_glow(32, [255, 220, 170]);
        for pixel in image.rgba.as_chunks::<4>().0 {
            let a = pixel[3];
            assert!(pixel[0] <= a && pixel[1] <= a && pixel[2] <= a);
        }
    }
}
