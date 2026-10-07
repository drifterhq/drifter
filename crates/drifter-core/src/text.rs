use std::path::{Path, PathBuf};

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use swash::scale::{Render, ScaleContext, Source, image::Content};
use swash::shape::ShapeContext;
use swash::zeno::{Format, Vector};
use swash::{FontRef, NormalizedCoord};

use crate::geom::Vec2;
use crate::image::Image;

const FALLBACKS: [&str; 8] = [
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/TTF/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
    "/usr/share/fonts/liberation-sans-fonts/LiberationSans-Regular.ttf",
    "/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/Library/Fonts/Arial.ttf",
];

pub fn font_path(chosen: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(chosen) = chosen
        && chosen.is_file()
    {
        return Some(chosen);
    }
    FALLBACKS
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

const STEPS: f32 = 4.0;
const SHAPES_KEPT: usize = 4096;
const REGULAR: f32 = 400.0;
const BOLD: f32 = 650.0;

#[derive(Clone, Copy)]
struct Placed {
    id: u16,
    x: f32,
    advance: f32,
}

struct Raster {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    coverage: Vec<u8>,
}

struct Kit {
    shape: ShapeContext,
    scale: ScaleContext,
    shapes: HashMap<(String, u32), Arc<Vec<Placed>>>,
    glyphs: HashMap<(u16, u8, u32, u32), Arc<Raster>>,
}

pub struct Font {
    data: Vec<u8>,
    px: f32,
    darken: f32,
    variable: bool,
    ascent: f64,
    descent: f64,
    kit: Mutex<Kit>,
}

impl Font {
    pub fn load(path: &Path, px: f64) -> Result<Font, String> {
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let font = FontRef::from_index(&data, 0)
            .ok_or_else(|| format!("{}: not a font", path.display()))?;
        let variable = font
            .variations()
            .any(|axis| axis.tag() == swash::tag_from_bytes(b"wght"));
        let metrics = font.metrics(&[]).scale(px as f32);
        let (ascent, descent) = (f64::from(metrics.ascent), f64::from(metrics.descent));
        Ok(Font {
            data,
            px: px as f32,
            darken: darken_for(px as f32),
            variable,
            ascent,
            descent,
            kit: Mutex::new(Kit {
                shape: ShapeContext::new(),
                scale: ScaleContext::new(),
                shapes: HashMap::new(),
                glyphs: HashMap::new(),
            }),
        })
    }

    pub fn find(chosen: Option<PathBuf>, px: f64) -> Result<Font, String> {
        let asked = chosen.clone();
        let path = font_path(chosen).ok_or_else(|| match asked {
            Some(path) => format!("no font found. put one at {}", path.display()),
            None => "no font found".to_string(),
        })?;
        Font::load(&path, px)
    }

    pub fn ascent(&self) -> f64 {
        self.ascent
    }

    pub fn line_height(&self) -> f64 {
        self.ascent + self.descent
    }

    pub fn width_of(&self, text: &str) -> f64 {
        self.shaped(text, REGULAR)
            .last()
            .map_or(0.0, |last| f64::from(last.x + last.advance))
    }

    pub fn measure(&self, text: &str) -> Vec2 {
        Vec2::new(self.width_of(text), self.line_height())
    }

    pub fn paint(&self, text: &str, color: [u8; 3]) -> Image {
        let placed = self.shaped(text, REGULAR);
        self.lay(&placed, REGULAR, 0.0, color)
    }

    pub fn paint_bold(&self, text: &str, color: [u8; 3]) -> Image {
        match self.variable {
            true => {
                let placed = self.shaped(text, BOLD);
                self.lay(&placed, BOLD, 0.0, color)
            }
            false => {
                let placed = self.shaped(text, REGULAR);
                self.lay(&placed, REGULAR, self.px * 0.03, color)
            }
        }
    }

    pub fn paint_even_digits(&self, text: &str, color: [u8; 3]) -> Image {
        let cell = ('0'..='9')
            .map(|digit| self.width_of(&digit.to_string()))
            .fold(0.0, f64::max) as f32;
        let mut placed = Vec::new();
        let mut pen = 0.0f32;
        for glyph in text.chars() {
            let one = self.shaped(&glyph.to_string(), REGULAR);
            let natural = one.last().map_or(0.0, |last| last.x + last.advance);
            let room = if glyph.is_ascii_digit() {
                cell
            } else {
                natural
            };
            let centering = (room - natural) / 2.0;
            placed.extend(one.iter().map(|each| Placed {
                x: pen + centering + each.x,
                ..*each
            }));
            if let Some(last) = placed.last_mut() {
                last.advance = pen + room - last.x;
            }
            pen += room;
        }
        self.lay(&placed, REGULAR, 0.0, color)
    }

    fn font(&self) -> FontRef<'_> {
        FontRef::from_index(&self.data, 0).expect("the font read when it loaded")
    }

    fn coords(&self, weight: f32) -> Vec<NormalizedCoord> {
        match self.variable {
            true => self
                .font()
                .variations()
                .normalized_coords([("wght", weight)])
                .collect(),
            false => Vec::new(),
        }
    }

    fn shaped(&self, text: &str, weight: f32) -> Arc<Vec<Placed>> {
        let key = (text.to_string(), weight.to_bits());
        let mut kit = self
            .kit
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(found) = kit.shapes.get(&key) {
            return found.clone();
        }
        let font = self.font();
        let coords = self.coords(weight);
        let mut out = Vec::new();
        let mut pen = 0.0f32;
        {
            let mut shaper = kit
                .shape
                .builder(font)
                .size(self.px)
                .normalized_coords(&coords)
                .build();
            shaper.add_str(text);
            shaper.shape_with(|cluster| {
                for glyph in cluster.glyphs {
                    out.push(Placed {
                        id: glyph.id,
                        x: pen + glyph.x,
                        advance: glyph.advance,
                    });
                    pen += glyph.advance;
                }
            });
        }
        if let Some(last) = out.last_mut() {
            last.advance = pen - last.x;
        }
        if kit.shapes.len() >= SHAPES_KEPT {
            kit.shapes.clear();
        }
        let out = Arc::new(out);
        kit.shapes.insert(key, out.clone());
        out
    }

    fn raster(&self, id: u16, step: u8, weight: f32, darken: f32, density: f32) -> Arc<Raster> {
        let key = (
            id,
            step,
            weight.to_bits() ^ darken.to_bits().rotate_left(16),
            density.to_bits(),
        );
        let mut kit = self
            .kit
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(found) = kit.glyphs.get(&key) {
            return found.clone();
        }
        let font = self.font();
        let coords = self.coords(weight);
        let mut scaler = kit
            .scale
            .builder(font)
            .size(self.px * density)
            .hint(false)
            .normalized_coords(&coords)
            .build();
        let mut render = Render::new(&[Source::Outline]);
        render
            .format(Format::Alpha)
            .offset(Vector::new(f32::from(step) / STEPS, 0.0));
        if darken > 0.0 {
            render.embolden(darken * density);
        }
        let raster = match render.render(&mut scaler, id) {
            Some(image) if image.content == Content::Mask => Raster {
                left: image.placement.left,
                top: image.placement.top,
                width: image.placement.width,
                height: image.placement.height,
                coverage: image.data,
            },
            _ => Raster {
                left: 0,
                top: 0,
                width: 0,
                height: 0,
                coverage: Vec::new(),
            },
        };
        let raster = Arc::new(raster);
        kit.glyphs.insert(key, raster.clone());
        raster
    }

    fn lay(&self, placed: &[Placed], weight: f32, extra: f32, color: [u8; 3]) -> Image {
        let wide = placed
            .last()
            .map_or(0.0, |last| f64::from(last.x + last.advance));
        let density = crate::paint::density();
        let width = ((wide + f64::from(extra) * 2.0).ceil().max(1.0) * density) as u32;
        let height = (self.line_height().ceil().max(1.0) * density) as u32;
        let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
        let baseline = (self.ascent.round() * density) as i64;
        let density = density as f32;
        for glyph in placed {
            let x = (glyph.x + extra) * density;
            let whole = x.floor();
            let step = ((x - whole) * STEPS).round() as i64;
            let (whole, step) = match step >= STEPS as i64 {
                true => (whole as i64 + 1, 0u8),
                false => (whole as i64, step as u8),
            };
            let raster = self.raster(glyph.id, step, weight, self.darken + extra, density);
            for row in 0..raster.height as i64 {
                let y = baseline - i64::from(raster.top) + row;
                if y < 0 || y >= i64::from(height) {
                    continue;
                }
                for column in 0..raster.width as i64 {
                    let x = whole + i64::from(raster.left) + column;
                    if x < 0 || x >= i64::from(width) {
                        continue;
                    }
                    let ink = u32::from(
                        raster.coverage[(row * i64::from(raster.width) + column) as usize],
                    );
                    if ink == 0 {
                        continue;
                    }
                    let index = ((y as usize) * (width as usize) + x as usize) * 4;
                    let now = (u32::from(rgba[index + 3]) + ink).min(255);
                    for (channel, value) in color.iter().enumerate() {
                        rgba[index + channel] = ((u32::from(*value) * now + 127) / 255) as u8;
                    }
                    rgba[index + 3] = now as u8;
                }
            }
        }
        Image {
            width,
            height,
            rgba,
        }
    }
}

fn darken_for(px: f32) -> f32 {
    (px * 0.012).clamp(0.0, 0.35)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Rect;

    fn font() -> Font {
        Font::find(None, 22.0).expect("this machine has no usable font")
    }

    #[test]
    fn even_digits_keep_a_clock_the_same_width() {
        let work_sans = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts/WorkSans.ttf");
        let font = Font::load(&work_sans, 22.0).expect("the repo ships Work Sans");
        let narrow = font.paint_even_digits("11:11", [255, 255, 255]);
        let wide = font.paint_even_digits("10:08", [255, 255, 255]);
        assert_eq!(narrow.width, wide.width);
        assert_ne!(
            font.paint("11:11", [255, 255, 255]).width,
            font.paint("10:08", [255, 255, 255]).width,
            "this font has uneven digits, so the test means something"
        );
        assert!(opaque(&narrow) > 0);
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
    fn text_at_density_two_matches_density_one() {
        let font = font();
        crate::paint::same_at_two("text", || font.paint("Mon Sep 28 12:35:26 AM AVATAR", [240, 240, 240]));
        crate::paint::same_at_two("bold", || font.paint_bold("Terminal Settings", [20, 20, 20]));
        crate::paint::same_at_two("digits", || font.paint_even_digits("10:08:59", [255, 255, 255]));
    }

    #[test]
    fn a_canvas_at_density_two_matches_density_one() {
        crate::paint::same_at_two("canvas", || {
            let mut canvas = crate::paint::Canvas::new(120, 60);
            canvas.round_rect(Rect::new(Vec2::new(4.0, 4.0), Vec2::new(100.0, 40.0)), 9.0, [40, 90, 200], 1.0);
            canvas.circle(Vec2::new(30.0, 30.0), 10.0, [250, 250, 250], 0.8);
            canvas.shadow(Rect::new(Vec2::new(60.0, 10.0), Vec2::new(40.0, 30.0)), 6.0, 5.0, 0.5);
            let label = font().paint("Hi", [0, 0, 0]);
            canvas.stamp(&label, Vec2::new(50.0, 20.0), 1.0);
            canvas.image
        });
    }

    #[test]
    fn a_font_is_found_on_this_machine() {
        let path = font_path(None).expect("no font found");
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
        let small = Font::find(None, 12.0).unwrap();
        let big = Font::find(None, 36.0).unwrap();
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
