use drifter_core::{Image, Rect, Vec2};

pub type Clip = [i64; 4];

pub fn whole(image: &Image) -> Clip {
    [0, 0, i64::from(image.width), i64::from(image.height)]
}

pub fn clip_of(area: Rect, inside: Clip) -> Clip {
    [
        (area.origin.x.round() as i64).max(inside[0]),
        (area.origin.y.round() as i64).max(inside[1]),
        (area.max().x.round() as i64).min(inside[2]),
        (area.max().y.round() as i64).min(inside[3]),
    ]
}

pub struct Mask {
    pub x: i64,
    pub y: i64,
    pub w: usize,
    pub h: usize,
    pub cov: Vec<f32>,
}

fn dot(a: Vec2, b: Vec2) -> f64 {
    a.x * b.x + a.y * b.y
}

pub fn to_segment(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let along = b - a;
    let span = dot(along, along);
    if span <= 1e-12 {
        return (p - a).length();
    }
    let t = (dot(p - a, along) / span).clamp(0.0, 1.0);
    (p - (a + along * t)).length()
}

pub fn inside_polygon(p: Vec2, points: &[Vec2]) -> bool {
    let mut inside = false;
    let mut j = points.len().wrapping_sub(1);
    for i in 0..points.len() {
        let (a, b) = (points[i], points[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

pub fn oval_distance(p: Vec2, rect: Rect) -> f64 {
    let c = rect.center();
    let (rx, ry) = ((rect.size.x / 2.0).max(0.5), (rect.size.y / 2.0).max(0.5));
    let q = p - c;
    let f = (q.x / rx).powi(2) + (q.y / ry).powi(2) - 1.0;
    let g = 2.0 * ((q.x / (rx * rx)).powi(2) + (q.y / (ry * ry)).powi(2)).sqrt();
    if g <= 1e-9 {
        return -rx.min(ry);
    }
    f / g
}

pub fn ordered(a: Vec2, b: Vec2) -> Rect {
    let low = a.min(b);
    Rect::new(low, a.max(b) - low)
}

impl Mask {
    pub fn new(area: Rect, clip: Clip) -> Mask {
        let x0 = (area.origin.x.floor() as i64).max(clip[0]);
        let y0 = (area.origin.y.floor() as i64).max(clip[1]);
        let x1 = (area.max().x.ceil() as i64).min(clip[2]);
        let y1 = (area.max().y.ceil() as i64).min(clip[3]);
        let (w, h) = ((x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize);
        Mask { x: x0, y: y0, w, h, cov: vec![0.0; w * h] }
    }

    pub fn each(&mut self, area: Rect, cover: impl Fn(Vec2) -> f64) {
        let x0 = (area.origin.x.floor() as i64).max(self.x);
        let y0 = (area.origin.y.floor() as i64).max(self.y);
        let x1 = (area.max().x.ceil() as i64).min(self.x + self.w as i64);
        let y1 = (area.max().y.ceil() as i64).min(self.y + self.h as i64);
        for y in y0..y1 {
            let row = (y - self.y) as usize * self.w;
            for x in x0..x1 {
                let c = cover(Vec2::new(x as f64 + 0.5, y as f64 + 0.5)).clamp(0.0, 1.0) as f32;
                let at = row + (x - self.x) as usize;
                if c > self.cov[at] {
                    self.cov[at] = c;
                }
            }
        }
    }

    pub fn segment(&mut self, a: Vec2, b: Vec2, half: f64) {
        let grow = Vec2::splat(half + 1.0);
        let area = Rect::new(a.min(b) - grow, a.max(b) - a.min(b) + grow * 2.0);
        self.each(area, |p| half - to_segment(p, a, b) + 0.5);
    }

    pub fn line(&mut self, points: &[Vec2], half: f64) {
        match points {
            [] => {}
            [one] => self.segment(*one, *one, half),
            _ => {
                for pair in points.windows(2) {
                    self.segment(pair[0], pair[1], half);
                }
            }
        }
    }

    pub fn polygon(&mut self, points: &[Vec2]) {
        let Some(first) = points.first() else {
            return;
        };
        let (low, high) = points.iter().fold((*first, *first), |(low, high), p| (low.min(*p), high.max(*p)));
        let area = Rect::new(low - Vec2::splat(1.0), high - low + Vec2::splat(2.0));
        self.each(area, |p| {
            let mut near = f64::MAX;
            let mut j = points.len() - 1;
            for i in 0..points.len() {
                near = near.min(to_segment(p, points[j], points[i]));
                j = i;
            }
            match inside_polygon(p, points) {
                true => 0.5 + near,
                false => 0.5 - near,
            }
        });
    }

    pub fn frame(&mut self, rect: Rect, half: f64) {
        let outer = Rect::new(rect.origin - Vec2::splat(half), rect.size + Vec2::splat(half * 2.0));
        let inner = Rect::new(rect.origin + Vec2::splat(half), rect.size - Vec2::splat(half * 2.0));
        let hollow = inner.size.x > 0.0 && inner.size.y > 0.0;
        self.each(Rect::new(outer.origin - Vec2::splat(1.0), outer.size + Vec2::splat(2.0)), |p| {
            let out = 0.5 - round_box(p, outer, half);
            match hollow {
                true => out.min(0.5 + round_box(p, inner, 0.0)),
                false => out,
            }
        });
    }

    pub fn oval(&mut self, rect: Rect, half: f64) {
        let grow = Vec2::splat(half + 1.5);
        self.each(Rect::new(rect.origin - grow, rect.size + grow * 2.0), |p| half - oval_distance(p, rect).abs() + 0.5);
    }

    pub fn from_alpha(piece: &Image, at: (i64, i64), grow: usize, boost: f32, clip: Clip) -> Mask {
        let (w, h) = (piece.width as usize + grow * 2, piece.height as usize + grow * 2);
        let mut alpha = vec![0f32; w * h];
        for y in 0..piece.height as usize {
            for x in 0..piece.width as usize {
                alpha[(y + grow) * w + x + grow] = f32::from(piece.rgba[(y * piece.width as usize + x) * 4 + 3]) / 255.0;
            }
        }
        if grow > 0 {
            blur(&mut alpha, w, h, grow);
            blur(&mut alpha, w, h, grow);
        }
        let area = Rect::new(Vec2::new((at.0 - grow as i64) as f64, (at.1 - grow as i64) as f64), Vec2::new(w as f64, h as f64));
        let mut mask = Mask::new(area, clip);
        let (left, top) = (at.0 - grow as i64, at.1 - grow as i64);
        for y in 0..mask.h {
            for x in 0..mask.w {
                let (sx, sy) = ((mask.x + x as i64 - left) as usize, (mask.y + y as i64 - top) as usize);
                mask.cov[y * mask.w + x] = (alpha[sy * w + sx] * boost).min(1.0);
            }
        }
        mask
    }

    fn spots(&self, target: &Image) -> impl Iterator<Item = (usize, f32)> + '_ {
        let (tw, th) = (i64::from(target.width), i64::from(target.height));
        (0..self.h).flat_map(move |y| {
            (0..self.w).filter_map(move |x| {
                let (tx, ty) = (self.x + x as i64, self.y + y as i64);
                let c = self.cov[y * self.w + x];
                (c > 0.0 && tx >= 0 && ty >= 0 && tx < tw && ty < th).then(|| (((ty * tw + tx) * 4) as usize, c))
            })
        })
    }
}

fn round_box(p: Vec2, rect: Rect, radius: f64) -> f64 {
    let c = rect.center();
    let half = rect.size / 2.0 - Vec2::splat(radius);
    let q = Vec2::new((p.x - c.x).abs() - half.x, (p.y - c.y).abs() - half.y);
    let outside = q.max(Vec2::ZERO).length();
    outside + q.x.max(q.y).min(0.0) - radius
}

fn blur(values: &mut [f32], w: usize, h: usize, r: usize) {
    let mut line = vec![0f32; w.max(h)];
    let span = (r * 2 + 1) as f32;
    for y in 0..h {
        let row = &mut values[y * w..(y + 1) * w];
        line[..w].copy_from_slice(row);
        let mut sum: f32 = line[..r.min(w)].iter().sum();
        for x in 0..w {
            if x + r < w {
                sum += line[x + r];
            }
            if x > r {
                sum -= line[x - r - 1];
            }
            row[x] = sum / span;
        }
    }
    for x in 0..w {
        for y in 0..h {
            line[y] = values[y * w + x];
        }
        let mut sum: f32 = line[..r.min(h)].iter().sum();
        for y in 0..h {
            if y + r < h {
                sum += line[y + r];
            }
            if y > r {
                sum -= line[y - r - 1];
            }
            values[y * w + x] = sum / span;
        }
    }
}

fn byte(value: f32) -> u8 {
    (value + 0.5).clamp(0.0, 255.0) as u8
}

pub fn over(target: &mut Image, mask: &Mask, color: [u8; 3], alpha: f32) {
    let spots: Vec<(usize, f32)> = mask.spots(target).collect();
    for (at, c) in spots {
        let a = c * alpha;
        let px = &mut target.rgba[at..at + 4];
        for channel in 0..3 {
            px[channel] = byte(f32::from(color[channel]) * a + f32::from(px[channel]) * (1.0 - a));
        }
        px[3] = byte(255.0 * a + f32::from(px[3]) * (1.0 - a));
    }
}

pub fn multiply(target: &mut Image, mask: &Mask, color: [u8; 3], alpha: f32) {
    let spots: Vec<(usize, f32)> = mask.spots(target).collect();
    for (at, c) in spots {
        let a = c * alpha;
        let px = &mut target.rgba[at..at + 4];
        for channel in 0..3 {
            let keep = 1.0 - a + a * f32::from(color[channel]) / 255.0;
            px[channel] = byte(f32::from(px[channel]) * keep);
        }
    }
}

pub fn stamp(target: &mut Image, piece: &Image, at: (i64, i64), clip: Clip) {
    let x0 = at.0.max(clip[0]).max(0);
    let y0 = at.1.max(clip[1]).max(0);
    let x1 = (at.0 + i64::from(piece.width)).min(clip[2]).min(i64::from(target.width));
    let y1 = (at.1 + i64::from(piece.height)).min(clip[3]).min(i64::from(target.height));
    for y in y0..y1 {
        for x in x0..x1 {
            let from = (((y - at.1) * i64::from(piece.width) + x - at.0) * 4) as usize;
            let a = u32::from(piece.rgba[from + 3]);
            if a == 0 {
                continue;
            }
            let to = ((y * i64::from(target.width) + x) * 4) as usize;
            for channel in 0..4 {
                let under = u32::from(target.rgba[to + channel]);
                target.rgba[to + channel] = (u32::from(piece.rgba[from + channel]) + (under * (255 - a) + 127) / 255).min(255) as u8;
            }
        }
    }
}

pub fn sample(source: &Image, at: Vec2) -> [f32; 4] {
    let (w, h) = (source.width as i64, source.height as i64);
    let (fx, fy) = (at.x - 0.5, at.y - 0.5);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = ((fx - x0) as f32, (fy - y0) as f32);
    let pick = |x: i64, y: i64| {
        let at = ((y.clamp(0, h - 1) * w + x.clamp(0, w - 1)) * 4) as usize;
        [0, 1, 2, 3].map(|c| f32::from(source.rgba[at + c]))
    };
    let (x0, y0) = (x0 as i64, y0 as i64);
    let (a, b, c, d) = (pick(x0, y0), pick(x0 + 1, y0), pick(x0, y0 + 1), pick(x0 + 1, y0 + 1));
    [0, 1, 2, 3].map(|i| (a[i] * (1.0 - tx) + b[i] * tx) * (1.0 - ty) + (c[i] * (1.0 - tx) + d[i] * tx) * ty)
}

pub fn checker(width: u32, height: u32, cell: u32, light: [u8; 3], dark: [u8; 3]) -> Image {
    let cell = cell.max(1);
    let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
    for y in 0..height {
        for x in 0..width {
            let color = if (x / cell + y / cell).is_multiple_of(2) { light } else { dark };
            rgba.extend_from_slice(&[color[0], color[1], color[2], 255]);
        }
    }
    Image { width, height, rgba }
}

pub fn has_alpha(image: &Image) -> bool {
    image.rgba.as_chunks::<4>().0.iter().any(|px| px[3] < 255)
}

pub fn cut(image: &Image, area: Clip) -> Image {
    let x0 = area[0].clamp(0, i64::from(image.width)) as usize;
    let y0 = area[1].clamp(0, i64::from(image.height)) as usize;
    let x1 = (area[2].clamp(0, i64::from(image.width)) as usize).max(x0 + 1).min(image.width as usize);
    let y1 = (area[3].clamp(0, i64::from(image.height)) as usize).max(y0 + 1).min(image.height as usize);
    let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    let mut rgba = Vec::with_capacity(w * h * 4);
    for y in y0..y0 + h {
        let row = (y * image.width as usize + x0) * 4;
        rgba.extend_from_slice(&image.rgba[row..row + w * 4]);
    }
    Image { width: w as u32, height: h as u32, rgba }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blank(w: u32, h: u32) -> Image {
        Image { width: w, height: h, rgba: vec![255; (w * h * 4) as usize] }
    }

    #[test]
    fn a_thick_segment_covers_its_middle_and_not_far_away() {
        let mut mask = Mask::new(Rect::new(Vec2::ZERO, Vec2::splat(40.0)), [0, 0, 40, 40]);
        mask.segment(Vec2::new(5.0, 20.0), Vec2::new(35.0, 20.0), 2.7);
        assert_eq!(mask.cov[20 * 40 + 20], 1.0);
        assert_eq!(mask.cov[5 * 40 + 20], 0.0);
        assert!(mask.cov[22 * 40 + 20] > 0.0 && mask.cov[22 * 40 + 20] < 1.0);
    }

    #[test]
    fn a_polygon_fills_inside_with_soft_edges() {
        let mut mask = Mask::new(Rect::new(Vec2::ZERO, Vec2::splat(20.0)), [0, 0, 20, 20]);
        mask.polygon(&[Vec2::new(10.0, 1.0), Vec2::new(19.0, 19.0), Vec2::new(1.0, 19.0)]);
        assert_eq!(mask.cov[15 * 20 + 10], 1.0);
        assert_eq!(mask.cov[2 * 20 + 2], 0.0);
    }

    #[test]
    fn a_frame_is_hollow() {
        let mut mask = Mask::new(Rect::new(Vec2::ZERO, Vec2::splat(40.0)), [0, 0, 40, 40]);
        mask.frame(Rect::new(Vec2::splat(5.0), Vec2::splat(30.0)), 2.0);
        assert_eq!(mask.cov[20 * 40 + 20], 0.0);
        assert_eq!(mask.cov[20 * 40 + 5], 1.0);
    }

    #[test]
    fn an_oval_ring_follows_the_edge() {
        let mut mask = Mask::new(Rect::new(Vec2::ZERO, Vec2::new(60.0, 40.0)), [0, 0, 60, 40]);
        mask.oval(Rect::new(Vec2::splat(10.0), Vec2::new(40.0, 20.0)), 1.5);
        assert_eq!(mask.cov[20 * 60 + 30], 0.0);
        assert!(mask.cov[20 * 60 + 10] > 0.5);
        assert!(mask.cov[10 * 60 + 30] > 0.5);
    }

    #[test]
    fn multiply_keeps_dark_ink_dark() {
        let mut image = blank(4, 1);
        image.rgba[0..4].copy_from_slice(&[0, 0, 0, 255]);
        let mut mask = Mask::new(Rect::new(Vec2::ZERO, Vec2::new(4.0, 1.0)), whole(&image));
        mask.cov.fill(1.0);
        multiply(&mut image, &mask, [255, 220, 0], 1.0);
        assert_eq!(&image.rgba[0..4], &[0, 0, 0, 255]);
        assert_eq!(&image.rgba[4..8], &[255, 220, 0, 255]);
    }

    #[test]
    fn cut_takes_the_asked_part() {
        let mut image = blank(4, 4);
        image.rgba[(4 + 1) * 4] = 7;
        let part = cut(&image, [1, 1, 3, 4]);
        assert_eq!((part.width, part.height), (2, 3));
        assert_eq!(part.rgba[0], 7);
    }
}
