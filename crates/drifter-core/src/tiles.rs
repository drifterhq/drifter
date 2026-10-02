use crate::geom::{Rect, Vec2};
use crate::image::Image;

pub const TILE: u32 = 2048;

#[derive(Clone)]
pub struct Pyramid {
    pub levels: Vec<Image>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Piece {
    pub key: (usize, u32, u32),
    pub source: Rect,
    pub target: Rect,
}

impl Pyramid {
    pub fn build(image: Image, most: Vec2, least: Vec2) -> Pyramid {
        let image = cap(image, most);
        let mut levels = vec![image];
        loop {
            let last = &levels[levels.len() - 1];
            let fits = f64::from(last.width) <= least.x.max(1.0)
                && f64::from(last.height) <= least.y.max(1.0);
            if fits || last.width <= 1 || last.height <= 1 {
                break;
            }
            let half = halved(last);
            levels.push(half);
        }
        Pyramid { levels }
    }

    pub fn size(&self) -> Vec2 {
        self.levels[0].size()
    }

    pub fn level_for(&self, source: Rect, target: Rect) -> usize {
        if source.size.x <= 0.0 || target.size.x <= 0.0 {
            return 0;
        }
        let shrink = source.size.x / target.size.x;
        if shrink <= 1.0 {
            return 0;
        }
        (shrink.log2().floor() as usize).min(self.levels.len() - 1)
    }

    pub fn pieces(&self, source: Rect, target: Rect) -> Vec<Piece> {
        let level = self.level_for(source, target);
        let image = &self.levels[level];
        let down = Vec2::new(
            image.size().x / self.size().x,
            image.size().y / self.size().y,
        );
        let seen = Rect::new(
            Vec2::new(source.origin.x * down.x, source.origin.y * down.y),
            Vec2::new(source.size.x * down.x, source.size.y * down.y),
        );
        let per = Vec2::new(target.size.x / seen.size.x, target.size.y / seen.size.y);
        let span = f64::from(TILE);
        let columns = image.width.div_ceil(TILE);
        let rows = image.height.div_ceil(TILE);
        let first_col = (seen.origin.x / span).floor().max(0.0) as u32;
        let first_row = (seen.origin.y / span).floor().max(0.0) as u32;
        let last_col = ((seen.max().x / span).ceil() as u32).min(columns);
        let last_row = ((seen.max().y / span).ceil() as u32).min(rows);
        let mut out = Vec::new();
        for row in first_row..last_row {
            for col in first_col..last_col {
                let tile = Rect::new(
                    Vec2::new(f64::from(col) * span, f64::from(row) * span),
                    Vec2::new(
                        f64::from((image.width - col * TILE).min(TILE)),
                        f64::from((image.height - row * TILE).min(TILE)),
                    ),
                );
                let low = tile.origin.max(seen.origin);
                let high = tile.max().min(seen.max());
                if high.x <= low.x || high.y <= low.y {
                    continue;
                }
                let on_screen = |at: Vec2| {
                    target.origin
                        + Vec2::new(
                            (at.x - seen.origin.x) * per.x,
                            (at.y - seen.origin.y) * per.y,
                        )
                };
                out.push(Piece {
                    key: (level, col, row),
                    source: Rect::new(low - tile.origin, high - low),
                    target: Rect::new(on_screen(low), on_screen(high) - on_screen(low)),
                });
            }
        }
        out
    }

    pub fn tile(&self, key: (usize, u32, u32)) -> Image {
        let (level, col, row) = key;
        let image = &self.levels[level.min(self.levels.len() - 1)];
        crop(image, col * TILE, row * TILE, TILE, TILE)
    }
}

fn cap(image: Image, most: Vec2) -> Image {
    if most.x <= 0.0 || most.y <= 0.0 {
        return image;
    }
    let size = image.size();
    let cover = (most.x / size.x).max(most.y / size.y);
    if cover >= 1.0 {
        return image;
    }
    let mut out = image;
    while f64::from(out.width) / 2.0 >= size.x * cover
        && f64::from(out.height) / 2.0 >= size.y * cover
    {
        out = halved(&out);
    }
    out
}

pub fn halved(image: &Image) -> Image {
    let width = image.width.div_ceil(2).max(1);
    let height = image.height.div_ceil(2).max(1);
    let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
    for y in 0..height {
        for x in 0..width {
            let mut sum = [0u32; 4];
            let mut count = 0;
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let (sx, sy) = (x * 2 + dx, y * 2 + dy);
                if sx >= image.width || sy >= image.height {
                    continue;
                }
                let at = ((sy as usize) * (image.width as usize) + sx as usize) * 4;
                for channel in 0..4 {
                    sum[channel] += u32::from(image.rgba[at + channel]);
                }
                count += 1;
            }
            let to = ((y as usize) * (width as usize) + x as usize) * 4;
            for channel in 0..4 {
                rgba[to + channel] = ((sum[channel] + count / 2) / count) as u8;
            }
        }
    }
    Image {
        width,
        height,
        rgba,
    }
}

fn crop(image: &Image, left: u32, top: u32, width: u32, height: u32) -> Image {
    let width = width.min(image.width.saturating_sub(left)).max(1);
    let height = height.min(image.height.saturating_sub(top)).max(1);
    let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
    for y in top..top + height {
        let start = ((y as usize) * (image.width as usize) + left as usize) * 4;
        rgba.extend_from_slice(&image.rgba[start..start + (width as usize) * 4]);
    }
    Image {
        width,
        height,
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(width: u32, height: u32) -> Image {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for y in 0..height {
            for x in 0..width {
                rgba.extend_from_slice(&[(x % 256) as u8, (y % 256) as u8, 7, 255]);
            }
        }
        Image {
            width,
            height,
            rgba,
        }
    }

    #[test]
    fn levels_halve_until_they_fit_the_screen() {
        let pyramid = Pyramid::build(picture(5000, 3000), Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        let sizes: Vec<(u32, u32)> = pyramid.levels.iter().map(|l| (l.width, l.height)).collect();
        assert_eq!(sizes, [(5000, 3000), (2500, 1500), (1250, 750)]);
    }

    #[test]
    fn a_picture_larger_than_the_world_is_halved_down_to_cover_it() {
        let pyramid = Pyramid::build(
            picture(9000, 4000),
            Vec2::new(2000.0, 1000.0),
            Vec2::new(1000.0, 500.0),
        );
        assert_eq!(
            (pyramid.levels[0].width, pyramid.levels[0].height),
            (2250, 1000)
        );
    }

    #[test]
    fn halving_averages_four_pixels() {
        let image = Image {
            width: 2,
            height: 1,
            rgba: vec![0, 0, 0, 255, 200, 100, 50, 255],
        };
        assert_eq!(halved(&image).rgba, [100, 50, 25, 255]);
    }

    #[test]
    fn the_level_follows_how_far_the_picture_is_shrunk() {
        let pyramid = Pyramid::build(picture(4096, 4096), Vec2::ZERO, Vec2::new(256.0, 256.0));
        let screen = Rect::new(Vec2::ZERO, Vec2::new(1000.0, 1000.0));
        let whole = |size| Rect::new(Vec2::ZERO, Vec2::splat(size));
        assert_eq!(pyramid.level_for(whole(1000.0), screen), 0);
        assert_eq!(pyramid.level_for(whole(1999.0), screen), 0);
        assert_eq!(pyramid.level_for(whole(2000.0), screen), 1);
        assert_eq!(pyramid.level_for(whole(4096.0), screen), 2);
        assert_eq!(
            pyramid.level_for(whole(1e9), screen),
            pyramid.levels.len() - 1
        );
    }

    #[test]
    fn pieces_cover_the_target_with_no_gap() {
        let pyramid = Pyramid::build(picture(5000, 3000), Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        let source = Rect::new(Vec2::new(1500.0, 1000.0), Vec2::new(1920.0, 1080.0));
        let target = Rect::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        let pieces = pyramid.pieces(source, target);
        let keys: Vec<_> = pieces.iter().map(|piece| piece.key).collect();
        assert_eq!(keys, [(0, 0, 0), (0, 1, 0), (0, 0, 1), (0, 1, 1)]);
        let area: f64 = pieces
            .iter()
            .map(|p| p.target.size.x * p.target.size.y)
            .sum();
        assert!((area - 1920.0 * 1080.0).abs() < 1e-6);
        assert_eq!(pieces[0].target.origin, Vec2::ZERO);
        assert_eq!(pieces[1].target.origin.x, pieces[0].target.max().x);
    }

    #[test]
    fn a_tile_at_the_edge_is_cut_to_the_picture() {
        let pyramid = Pyramid::build(picture(5000, 3000), Vec2::ZERO, Vec2::new(5000.0, 3000.0));
        let edge = pyramid.tile((0, 2, 1));
        assert_eq!((edge.width, edge.height), (5000 - 4096, 3000 - 2048));
        assert_eq!(edge.rgba[0..2], [(4096 % 256) as u8, (2048 % 256) as u8]);
    }
}
