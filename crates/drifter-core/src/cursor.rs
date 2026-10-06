use crate::geom::{Rect, Vec2};
use crate::image::Image;

const OUTLINE: f64 = 1.0;
const SHADOW_ROOM: f64 = 3.0;
const SHADOW_DROP: Vec2 = Vec2::new(0.3, 1.1);
const SHADOW_SOFT: f64 = 1.1;
const SHADOW_DARK: f64 = 0.38;
const SHAPE: [(f64, f64); 7] = [
    (0.0, 0.0),
    (0.0, 17.0),
    (4.1, 13.0),
    (6.9, 19.9),
    (10.5, 18.4),
    (7.7, 11.9),
    (12.2, 11.8),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cursor {
    pub scale: f64,
    pub fill: [u8; 3],
    pub edge: [u8; 3],
}

impl Default for Cursor {
    fn default() -> Self {
        Cursor {
            scale: 1.6,
            fill: [0, 0, 0],
            edge: [255, 255, 255],
        }
    }
}

impl Cursor {
    pub fn size(&self) -> Vec2 {
        let reach = SHAPE.iter().fold(Vec2::ZERO, |most, (x, y)| {
            Vec2::new(most.x.max(*x), most.y.max(*y))
        });
        (reach + Vec2::splat(OUTLINE * 2.0 + SHADOW_ROOM)) * self.scale
    }

    pub fn hotspot(&self) -> Vec2 {
        Vec2::splat(OUTLINE * self.scale)
    }

    pub fn rect(&self, pointer: Vec2) -> Rect {
        rect(pointer, self.size(), self.hotspot())
    }

    pub fn paint(&self) -> Image {
        let size = self.size();
        let width = size.x.ceil().max(1.0) as u32;
        let height = size.y.ceil().max(1.0) as u32;
        let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];

        for y in 0..height {
            for x in 0..width {
                let point = Vec2::new(
                    (f64::from(x) + 0.5) / self.scale - OUTLINE,
                    (f64::from(y) + 0.5) / self.scale - OUTLINE,
                );
                let inside = inside(point);
                let distance = distance_to_edge(point);

                let (color, coverage) = if inside && distance >= OUTLINE {
                    (self.fill, 1.0)
                } else if distance <= OUTLINE {
                    let reach = if inside { OUTLINE } else { OUTLINE - distance };
                    (self.edge, (reach / OUTLINE).clamp(0.0, 1.0))
                } else {
                    (self.edge, 0.0)
                };

                let below = point - SHADOW_DROP;
                let gap = match crate::cursor::inside(below) {
                    true => 0.0,
                    false => (distance_to_edge(below) - OUTLINE).max(0.0),
                };
                let shade = SHADOW_DARK * (-(gap * gap) / (2.0 * SHADOW_SOFT * SHADOW_SOFT)).exp();

                let alpha = coverage + shade * (1.0 - coverage);
                if alpha <= 0.0 {
                    continue;
                }
                let index = ((y as usize) * (width as usize) + x as usize) * 4;
                for (channel, value) in color.iter().enumerate() {
                    rgba[index + channel] =
                        (f64::from(*value) * coverage).round().clamp(0.0, 255.0) as u8;
                }
                rgba[index + 3] = (alpha * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }

        Image {
            width,
            height,
            rgba,
        }
    }
}

pub fn rect(pointer: Vec2, size: Vec2, hotspot: Vec2) -> Rect {
    Rect::new(pointer - hotspot, size)
}

fn inside(point: Vec2) -> bool {
    let mut crossings = 0;
    for index in 0..SHAPE.len() {
        let (ax, ay) = SHAPE[index];
        let (bx, by) = SHAPE[(index + 1) % SHAPE.len()];
        if (ay > point.y) != (by > point.y) {
            let cut = ax + (point.y - ay) / (by - ay) * (bx - ax);
            if point.x < cut {
                crossings += 1;
            }
        }
    }
    crossings % 2 == 1
}

fn distance_to_edge(point: Vec2) -> f64 {
    let mut closest = f64::MAX;
    for index in 0..SHAPE.len() {
        let a = Vec2::new(SHAPE[index].0, SHAPE[index].1);
        let next = SHAPE[(index + 1) % SHAPE.len()];
        let b = Vec2::new(next.0, next.1);
        closest = closest.min(distance_to_segment(point, a, b));
    }
    closest
}

fn distance_to_segment(point: Vec2, a: Vec2, b: Vec2) -> f64 {
    let along = b - a;
    let length = along.x * along.x + along.y * along.y;
    if length <= 0.0 {
        let away = point - a;
        return (away.x * away.x + away.y * away.y).sqrt();
    }
    let offset = point - a;
    let t = ((offset.x * along.x + offset.y * along.y) / length).clamp(0.0, 1.0);
    let near = a + along * t;
    let away = point - near;
    (away.x * away.x + away.y * away.y).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let index = ((y as usize) * (image.width as usize) + x as usize) * 4;
        [
            image.rgba[index],
            image.rgba[index + 1],
            image.rgba[index + 2],
            image.rgba[index + 3],
        ]
    }

    #[test]
    fn the_hotspot_is_the_tip_of_the_arrow() {
        let cursor = Cursor::default();
        let rect = cursor.rect(Vec2::new(500.0, 300.0));
        let image = cursor.paint();
        let tip_x = (500.0 - rect.origin.x) as u32;
        let tip_y = (300.0 - rect.origin.y) as u32;
        assert!(
            pixel(&image, tip_x, tip_y)[3] > 0,
            "the pointer position is not on the arrow"
        );
    }

    #[test]
    fn the_arrow_has_a_light_outline_around_a_dark_body() {
        let cursor = Cursor::default();
        let image = cursor.paint();
        let body = pixel(
            &image,
            (4.0 * cursor.scale) as u32,
            (7.0 * cursor.scale) as u32,
        );
        assert_eq!(body[3], 255);
        assert!(body[0] < 40, "the arrow body is not dark");

        let rim = pixel(&image, 1, (7.0 * cursor.scale) as u32);
        assert!(rim[3] > 0, "the arrow has no outline");
        assert!(rim[0] > 150, "the outline is not light");
    }

    #[test]
    fn the_far_corner_is_empty() {
        let image = Cursor::default().paint();
        let corner = pixel(&image, image.width - 1, 0);
        assert_eq!(corner[3], 0);
    }

    #[test]
    fn the_notch_under_the_tail_is_not_filled() {
        let cursor = Cursor::default();
        let image = cursor.paint();
        let notch = pixel(
            &image,
            (10.0 * cursor.scale) as u32,
            (6.0 * cursor.scale) as u32,
        );
        assert!(notch[3] < 60, "the arrow is a solid triangle, not an arrow: {notch:?}");
        assert!(notch[0] < 10, "only the soft shadow reaches the notch");
    }

    #[test]
    fn every_pixel_is_premultiplied() {
        let image = Cursor::default().paint();
        for pixel in image.rgba.as_chunks::<4>().0 {
            let a = pixel[3];
            assert!(pixel[0] <= a && pixel[1] <= a && pixel[2] <= a);
        }
    }

    #[test]
    fn a_supplied_cursor_hangs_its_top_left_pixel_on_the_pointer() {
        let where_it_goes = rect(Vec2::new(100.0, 200.0), Vec2::new(32.0, 32.0), Vec2::ZERO);
        assert_eq!(where_it_goes.origin, Vec2::new(100.0, 200.0));
        assert_eq!(where_it_goes.size, Vec2::new(32.0, 32.0));
    }

    #[test]
    fn the_cursor_grows_with_its_scale() {
        let small = Cursor {
            scale: 1.0,
            ..Cursor::default()
        };
        let big = Cursor {
            scale: 2.0,
            ..Cursor::default()
        };
        assert!((big.size().x - small.size().x * 2.0).abs() < 1e-9);
    }
}
