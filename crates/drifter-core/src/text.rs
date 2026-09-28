use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::geom::Vec2;
use crate::image::Image;

const FALLBACKS: [&str; 7] = [
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/Library/Fonts/Arial.ttf",
];

pub fn font_path() -> Option<PathBuf> {
    let chosen = Config::data_dir().join("font.ttf");
    if chosen.is_file() {
        return Some(chosen);
    }
    FALLBACKS
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

pub struct Font {
    inner: fontdue::Font,
    px: f32,
}

impl Font {
    pub fn load(path: &Path, px: f64) -> Result<Font, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let inner = fontdue::Font::from_bytes(bytes, fontdue::FontSettings::default())
            .map_err(|e| format!("{}: {e}", path.display()))?;
        Ok(Font {
            inner,
            px: px as f32,
        })
    }

    pub fn find(px: f64) -> Result<Font, String> {
        let path = font_path().ok_or_else(|| {
            format!(
                "no font found. put one at {}",
                Config::data_dir().join("font.ttf").display()
            )
        })?;
        Font::load(&path, px)
    }

    pub fn ascent(&self) -> f64 {
        self.line()
            .map(|line| f64::from(line.ascent))
            .unwrap_or(0.0)
    }

    pub fn line_height(&self) -> f64 {
        self.line()
            .map(|line| f64::from(line.ascent - line.descent))
            .unwrap_or(0.0)
    }

    pub fn width_of(&self, text: &str) -> f64 {
        text.chars()
            .map(|glyph| f64::from(self.inner.metrics(glyph, self.px).advance_width))
            .sum()
    }

    pub fn measure(&self, text: &str) -> Vec2 {
        Vec2::new(self.width_of(text), self.line_height())
    }

    pub fn paint(&self, text: &str, color: [u8; 3]) -> Image {
        let size = self.measure(text);
        let width = size.x.ceil().max(1.0) as u32;
        let height = size.y.ceil().max(1.0) as u32;
        let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
        let baseline = self.ascent();
        let mut pen = 0.0f64;

        for glyph in text.chars() {
            let (metrics, coverage) = self.inner.rasterize(glyph, self.px);
            let left = pen + f64::from(metrics.xmin);
            let top = baseline - f64::from(metrics.height as i32 + metrics.ymin);

            for row in 0..metrics.height {
                let y = top + row as f64;
                if y < 0.0 || y >= f64::from(height) {
                    continue;
                }
                for column in 0..metrics.width {
                    let x = left + column as f64;
                    if x < 0.0 || x >= f64::from(width) {
                        continue;
                    }
                    let ink = f64::from(coverage[row * metrics.width + column]) / 255.0;
                    if ink <= 0.0 {
                        continue;
                    }
                    let index = ((y as usize) * (width as usize) + x as usize) * 4;
                    let was = f64::from(rgba[index + 3]) / 255.0;
                    let now = (was + ink).min(1.0);
                    for (channel, value) in color.iter().enumerate() {
                        rgba[index + channel] =
                            (f64::from(*value) * now).round().clamp(0.0, 255.0) as u8;
                    }
                    rgba[index + 3] = (now * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }

            pen += f64::from(metrics.advance_width);
        }

        Image {
            width,
            height,
            rgba,
        }
    }

    fn line(&self) -> Option<fontdue::LineMetrics> {
        self.inner.horizontal_line_metrics(self.px)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Font {
        Font::find(22.0).expect("this machine has no usable font")
    }

    fn opaque(image: &Image) -> usize {
        image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| pixel[3] > 0)
            .count()
    }

    #[test]
    fn a_font_is_found_on_this_machine() {
        let path = font_path().expect("no font found");
        assert!(path.is_file());
    }

    #[test]
    fn painting_text_puts_ink_on_the_image() {
        let image = font().paint("28", [255, 255, 255]);
        assert!(opaque(&image) > 0, "the text painted nothing");
    }

    #[test]
    fn more_characters_are_wider() {
        let font = font();
        assert!(font.width_of("8888") > font.width_of("88"));
    }

    #[test]
    fn a_space_takes_room_but_paints_nothing() {
        let font = font();
        assert!(font.width_of(" ") > 0.0);
        assert_eq!(opaque(&font.paint(" ", [255, 255, 255])), 0);
    }

    #[test]
    fn the_image_is_as_tall_as_a_line() {
        let font = font();
        let image = font.paint("Mon Sep 28", [255, 255, 255]);
        assert_eq!(image.height, font.line_height().ceil() as u32);
    }

    #[test]
    fn the_image_is_as_wide_as_the_text_measures() {
        let font = font();
        let text = "12:35:26 AM";
        let image = font.paint(text, [255, 255, 255]);
        assert_eq!(image.width, font.width_of(text).ceil() as u32);
    }

    #[test]
    fn a_bigger_size_makes_bigger_text() {
        let small = Font::find(12.0).unwrap();
        let big = Font::find(36.0).unwrap();
        assert!(big.width_of("28") > small.width_of("28"));
        assert!(big.line_height() > small.line_height());
    }

    #[test]
    fn every_pixel_is_premultiplied() {
        let image = font().paint("Mon Sep 28 12:35:26 AM", [200, 210, 255]);
        for pixel in image.rgba.as_chunks::<4>().0 {
            let a = pixel[3];
            assert!(pixel[0] <= a && pixel[1] <= a && pixel[2] <= a);
        }
    }

    #[test]
    fn the_text_sits_inside_the_image() {
        let font = font();
        let image = font.paint("Mgjq|", [255, 255, 255]);
        let rows = image.rgba.as_chunks::<4>().0.len() / image.width as usize;
        assert_eq!(rows, image.height as usize);
        assert!(opaque(&image) > 0);
    }

    #[test]
    fn an_empty_string_is_still_a_usable_image() {
        let image = font().paint("", [255, 255, 255]);
        assert!(image.width >= 1 && image.height >= 1);
        assert_eq!(opaque(&image), 0);
    }
}
