use crate::geom::Vec2;
use crate::image::Image;
use crate::wallpaper::Placement;

const WIDE: u32 = 160;
const RADIUS: usize = 5;
const PASSES: usize = 3;

#[derive(Debug, Clone, PartialEq)]
pub struct Frost {
    width: usize,
    height: usize,
    rgb: Vec<[f64; 3]>,
}

impl Frost {
    pub fn build(source: &Image) -> Frost {
        let width = WIDE.min(source.width.max(1)) as usize;
        let height = ((f64::from(source.height) * width as f64 / f64::from(source.width.max(1))).round() as usize).max(1);
        let mut rgb = vec![[0.0; 3]; width * height];
        let mut count = vec![0.0f64; width * height];
        for y in 0..source.height as usize {
            let ty = (y * height / source.height as usize).min(height - 1);
            for x in 0..source.width as usize {
                let tx = (x * width / source.width as usize).min(width - 1);
                let at = (y * source.width as usize + x) * 4;
                let cell = ty * width + tx;
                for c in 0..3 {
                    rgb[cell][c] += f64::from(source.rgba[at + c]);
                }
                count[cell] += 1.0;
            }
        }
        for (cell, n) in rgb.iter_mut().zip(&count) {
            if *n > 0.0 {
                for value in cell.iter_mut() {
                    *value /= n;
                }
            }
        }
        let mut frost = Frost { width, height, rgb };
        for _ in 0..PASSES {
            frost.blur();
        }
        frost
    }

    fn blur(&mut self) {
        let (w, h) = (self.width, self.height);
        let mut across = vec![[0.0; 3]; w * h];
        for y in 0..h {
            for x in 0..w {
                let (from, to) = (x.saturating_sub(RADIUS), (x + RADIUS).min(w - 1));
                let mut sum = [0.0; 3];
                for i in from..=to {
                    for c in 0..3 {
                        sum[c] += self.rgb[y * w + i][c];
                    }
                }
                let n = (to - from + 1) as f64;
                across[y * w + x] = sum.map(|v| v / n);
            }
        }
        for y in 0..h {
            for x in 0..w {
                let (from, to) = (y.saturating_sub(RADIUS), (y + RADIUS).min(h - 1));
                let mut sum = [0.0; 3];
                for i in from..=to {
                    for c in 0..3 {
                        sum[c] += across[i * w + x][c];
                    }
                }
                let n = (to - from + 1) as f64;
                self.rgb[y * w + x] = sum.map(|v| v / n);
            }
        }
    }

    pub fn at(&self, u: f64, v: f64) -> [f64; 3] {
        let x = (u * self.width as f64 - 0.5).clamp(0.0, (self.width - 1) as f64);
        let y = (v * self.height as f64 - 0.5).clamp(0.0, (self.height - 1) as f64);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = (x - x0 as f64, y - y0 as f64);
        let get = |x: usize, y: usize| self.rgb[y * self.width + x];
        let mut out = [0.0; 3];
        for (c, value) in out.iter_mut().enumerate() {
            let top = get(x0, y0)[c] * (1.0 - fx) + get(x1, y0)[c] * fx;
            let bottom = get(x0, y1)[c] * (1.0 - fx) + get(x1, y1)[c] * fx;
            *value = top * (1.0 - fy) + bottom * fy;
        }
        out
    }

    pub fn behind(&self, laid: &Placement, picture: Vec2, point: Vec2) -> [f64; 3] {
        let target = laid.target;
        let source = laid.source;
        let sx = source.origin.x + (point.x - target.origin.x) * source.size.x / target.size.x.max(1.0);
        let sy = source.origin.y + (point.y - target.origin.y) * source.size.y / target.size.y.max(1.0);
        self.at(sx / picture.x.max(1.0), sy / picture.y.max(1.0))
    }
}

pub fn underlay(image: &mut Image, solid: f64, under: impl Fn(Vec2) -> [f64; 3]) {
    let solid = solid.clamp(0.01, 1.0);
    for y in 0..image.height {
        for x in 0..image.width {
            let at = ((y * image.width + x) * 4) as usize;
            let a = f64::from(image.rgba[at + 3]) / 255.0;
            if a <= 0.0 {
                continue;
            }
            let cover = (a / solid).min(1.0);
            let gap = cover - a;
            if gap <= 0.0 {
                continue;
            }
            let back = under(Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5));
            for c in 0..3 {
                let v = f64::from(image.rgba[at + c]) + back[c] * gap;
                image.rgba[at + c] = v.round().clamp(0.0, 255.0) as u8;
            }
            image.rgba[at + 3] = (cover * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;

    fn split(width: u32, height: u32) -> Image {
        let mut rgba = Vec::new();
        for _ in 0..height {
            for x in 0..width {
                let red = if x < width / 2 { 255 } else { 0 };
                rgba.extend([red, 0, 255 - red, 255]);
            }
        }
        Image { width, height, rgba }
    }

    #[test]
    fn the_frost_is_soft_but_keeps_the_colors_of_each_side() {
        let frost = Frost::build(&split(1600, 900));
        let left = frost.at(0.02, 0.5);
        let right = frost.at(0.98, 0.5);
        let middle = frost.at(0.5, 0.5);
        assert!(left[0] > 200.0 && left[2] < 55.0, "{left:?}");
        assert!(right[2] > 200.0 && right[0] < 55.0, "{right:?}");
        assert!(middle[0] > 60.0 && middle[2] > 60.0, "the edge is blurred: {middle:?}");
    }

    #[test]
    fn the_frost_follows_where_the_wallpaper_sits_on_screen() {
        let frost = Frost::build(&split(1600, 900));
        let laid = Placement {
            source: Rect::new(Vec2::ZERO, Vec2::new(1600.0, 900.0)),
            target: Rect::new(Vec2::new(-800.0, 0.0), Vec2::new(1600.0, 900.0)),
        };
        let seen = frost.behind(&laid, Vec2::new(1600.0, 900.0), Vec2::new(600.0, 450.0));
        assert!(seen[2] > 200.0, "a point on screen shows the blue half that is moved under it: {seen:?}");
    }

    #[test]
    fn underlay_fills_a_see_through_panel_and_leaves_the_rest() {
        let mut panel = Image { width: 2, height: 1, rgba: vec![50, 50, 50, 128, 0, 0, 0, 0] };
        underlay(&mut panel, 128.0 / 255.0, |_| [200.0, 100.0, 0.0]);
        assert_eq!(&panel.rgba[0..4], &[150, 100, 50, 255]);
        assert_eq!(&panel.rgba[4..8], &[0, 0, 0, 0], "outside the shape stays clear");
    }
}
