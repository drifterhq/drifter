use std::path::{Path, PathBuf};

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};

use swash::scale::{Render, ScaleContext, Source, StrikeWith, image::Content};
use swash::shape::{Direction, ShapeContext};
use swash::text::{Codepoint, Script};
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
    face: u16,
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
    color: bool,
}

struct Kit {
    shape: ShapeContext,
    scale: ScaleContext,
    shapes: HashMap<(String, u32), Arc<Vec<Placed>>>,
    glyphs: HashMap<(u16, u16, u8, u32, u32), Arc<Raster>>,
}

struct Face {
    data: Arc<Vec<u8>>,
    index: usize,
    variable: bool,
}

#[derive(Default)]
struct Faces {
    list: Vec<Face>,
    by_file: HashMap<(PathBuf, usize), Option<usize>>,
    by_char: HashMap<char, Option<usize>>,
}

static FACES: LazyLock<Mutex<Faces>> = LazyLock::new(|| Mutex::new(Faces::default()));

fn faces() -> std::sync::MutexGuard<'static, Faces> {
    FACES.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn covers(data: &[u8], index: usize, c: char) -> bool {
    FontRef::from_index(data, index).is_some_and(|font| font.charmap().map(c) != 0)
}

fn rides_along(c: char) -> bool {
    c.is_whitespace()
        || matches!(c, '\u{0300}'..='\u{036F}' | '\u{200C}'..='\u{200F}' | '\u{FE00}'..='\u{FE0F}' | '\u{E0100}'..='\u{E01EF}')
}

fn right_to_left(script: Script) -> bool {
    matches!(script, Script::Arabic | Script::Hebrew | Script::Syriac | Script::Thaana | Script::Nko)
}

fn ask_fontconfig(c: char) -> Option<(PathBuf, usize)> {
    let out = std::process::Command::new("fc-match")
        .args(["-f", "%{file}\n%{index}", &format!(":charset={:x}", u32::from(c))])
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut lines = text.lines();
    let file = PathBuf::from(lines.next()?.trim());
    let index = lines.next().and_then(|line| line.trim().parse::<usize>().ok()).unwrap_or(0) & 0xFFFF;
    file.is_file().then_some((file, index))
}

fn fallback_for(c: char) -> Option<usize> {
    let mut faces = faces();
    if let Some(found) = faces.by_char.get(&c) {
        return *found;
    }
    let mut found = faces.list.iter().position(|face| covers(&face.data, face.index, c));
    if found.is_none()
        && let Some(file) = ask_fontconfig(c)
    {
        let known = faces.by_file.get(&file).copied();
        let loaded = match known {
            Some(loaded) => loaded,
            None => {
                let face = std::fs::read(&file.0).ok().and_then(|data| {
                    let font = FontRef::from_index(&data, file.1)?;
                    let variable = font.variations().any(|axis| axis.tag() == swash::tag_from_bytes(b"wght"));
                    Some(Face { data: Arc::new(data), index: file.1, variable })
                });
                let at = face.map(|face| {
                    faces.list.push(face);
                    faces.list.len() - 1
                });
                faces.by_file.insert(file, at);
                at
            }
        };
        found = loaded.filter(|at| covers(&faces.list[*at].data, faces.list[*at].index, c));
    }
    faces.by_char.insert(c, found);
    found
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
        Font::from_bytes(data, px).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn from_bytes(data: Vec<u8>, px: f64) -> Result<Font, String> {
        let font = FontRef::from_index(&data, 0).ok_or_else(|| "not a font".to_string())?;
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

    pub fn paint_weight(&self, text: &str, color: [u8; 3], weight: f32) -> Image {
        match self.variable {
            true => {
                let placed = self.shaped(text, weight);
                self.lay(&placed, weight, 0.0, color)
            }
            false => {
                let placed = self.shaped(text, REGULAR);
                let extra = ((weight - REGULAR) / (BOLD - REGULAR)).clamp(0.0, 1.0);
                self.lay(&placed, REGULAR, self.px * 0.03 * extra, color)
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
        let mut out = Vec::new();
        let mut pen = 0.0f32;
        for (face, script, run) in self.runs(text) {
            let extra = (face > 0).then(|| faces().list.get(usize::from(face) - 1).map(|one| (one.data.clone(), one.index, one.variable))).flatten();
            let font = match &extra {
                Some((data, index, _)) => match FontRef::from_index(data, *index) {
                    Some(font) => font,
                    None => continue,
                },
                None => self.font(),
            };
            let coords: Vec<NormalizedCoord> = match &extra {
                Some((_, _, true)) => font.variations().normalized_coords([("wght", weight)]).collect(),
                Some(_) => Vec::new(),
                None => self.coords(weight),
            };
            let flip = right_to_left(script);
            let mut shaper = kit
                .shape
                .builder(font)
                .size(self.px)
                .script(script)
                .direction(if flip { Direction::RightToLeft } else { Direction::LeftToRight })
                .normalized_coords(&coords)
                .build();
            shaper.add_str(&text[run]);
            let mut glyphs = Vec::new();
            shaper.shape_with(|cluster| glyphs.extend(cluster.glyphs.iter().map(|glyph| (glyph.id, glyph.x, glyph.advance))));
            for (id, x, advance) in glyphs {
                out.push(Placed { face, id, x: pen + x, advance });
                pen += advance;
            }
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

    fn runs(&self, text: &str) -> Vec<(u16, Script, std::ops::Range<usize>)> {
        let primary = self.font().charmap();
        let mut runs: Vec<(u16, Script, std::ops::Range<usize>)> = Vec::new();
        for (at, c) in text.char_indices() {
            let end = at + c.len_utf8();
            let script = c.script();
            let plain = matches!(script, Script::Common | Script::Inherited | Script::Unknown);
            if let Some(last) = runs.last_mut() {
                let has = |face: u16| match face {
                    0 => primary.map(c) != 0,
                    _ => faces().list.get(usize::from(face) - 1).is_some_and(|one| covers(&one.data, one.index, c)),
                };
                let joins = match c.is_whitespace() {
                    true => last.0 == 0 || right_to_left(last.1),
                    false => rides_along(c) || (plain && !right_to_left(last.1) && has(last.0)),
                };
                if joins {
                    last.2.end = end;
                    continue;
                }
            }
            let face = match primary.map(c) != 0 || c.is_control() {
                true => 0,
                false => fallback_for(c).map_or(0, |at| at as u16 + 1),
            };
            match runs.last_mut() {
                Some(last) if last.0 == face && (plain || last.1 == script || matches!(last.1, Script::Common | Script::Inherited | Script::Unknown)) => {
                    if !plain {
                        last.1 = script;
                    }
                    last.2.end = end;
                }
                _ => runs.push((face, script, at..end)),
            }
        }
        for run in &mut runs {
            if matches!(run.1, Script::Common | Script::Inherited | Script::Unknown) {
                run.1 = Script::Latin;
            }
        }
        runs
    }

    fn raster(&self, face: u16, id: u16, step: u8, weight: f32, darken: f32, density: f32) -> Arc<Raster> {
        let key = (
            face,
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
        let extra = (face > 0).then(|| faces().list.get(usize::from(face) - 1).map(|one| (one.data.clone(), one.index, one.variable))).flatten();
        let font = match &extra {
            Some((data, index, _)) => match FontRef::from_index(data, *index) {
                Some(font) => font,
                None => self.font(),
            },
            None => self.font(),
        };
        let coords: Vec<NormalizedCoord> = match &extra {
            Some((_, _, true)) => font.variations().normalized_coords([("wght", weight)]).collect(),
            Some(_) => Vec::new(),
            None => self.coords(weight),
        };
        let mut scaler = kit
            .scale
            .builder(font)
            .size(self.px * density)
            .hint(false)
            .normalized_coords(&coords)
            .build();
        let mut render = Render::new(&[Source::ColorOutline(0), Source::ColorBitmap(StrikeWith::BestFit), Source::Outline]);
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
                color: false,
            },
            Some(image) if image.content == Content::Color => Raster {
                left: image.placement.left,
                top: image.placement.top,
                width: image.placement.width,
                height: image.placement.height,
                coverage: premultiplied(image.data),
                color: true,
            },
            _ => Raster {
                left: 0,
                top: 0,
                width: 0,
                height: 0,
                coverage: Vec::new(),
                color: false,
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
            let raster = self.raster(glyph.face, glyph.id, step, weight, self.darken + extra, density);
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
                    if raster.color {
                        let from = ((row * i64::from(raster.width) + column) * 4) as usize;
                        let src = &raster.coverage[from..from + 4];
                        if src[3] == 0 {
                            continue;
                        }
                        let index = ((y as usize) * (width as usize) + x as usize) * 4;
                        let keep = 255 - u32::from(src[3]);
                        for channel in 0..4 {
                            let under = u32::from(rgba[index + channel]);
                            rgba[index + channel] = (u32::from(src[channel]) + (under * keep + 127) / 255).min(255) as u8;
                        }
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

fn premultiplied(mut rgba: Vec<u8>) -> Vec<u8> {
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        for channel in &mut pixel[..3] {
            *channel = ((u32::from(*channel) * alpha + 127) / 255) as u8;
        }
    }
    rgba
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
    fn missing_letters_come_from_another_font() {
        let font = font();
        let latin = font.runs("plain name.txt");
        assert_eq!(latin.len(), 1);
        assert_eq!(latin[0].0, 0);
        if ask_fontconfig('日').is_none() {
            return;
        }
        let mixed = font.runs("日本語 file.txt");
        assert!(mixed[0].0 > 0, "Japanese comes from a fallback font: {:?}", mixed.iter().map(|run| (run.0, run.2.clone())).collect::<Vec<_>>());
        assert_eq!(mixed.last().map(|run| run.0), Some(0), "the Latin part stays in the main font");
        let painted = font.paint("日本語", [255, 255, 255]);
        let boxes = Font::find(None, 22.0).unwrap();
        assert!(opaque(&painted) > 0);
        assert!(boxes.width_of("日本語") > 0.0);
        let mixed = font.runs("🎉 party 🎈.txt");
        assert!(mixed.len() >= 3 && mixed[0].0 > 0 && mixed.last().unwrap().0 == 0, "{:?}", mixed.iter().map(|run| (run.0, run.2.clone())).collect::<Vec<_>>());
        assert!(mixed.iter().any(|run| run.0 == mixed[0].0 && run.2.start > 0), "the second emoji finds the emoji font too");
        assert_eq!(&"🎉 party 🎈.txt"[mixed[1].2.clone()].chars().next(), &Some(' '), "the space stays in the main font");
        let party = font.paint("🎉", [255, 255, 255]);
        let colored = party.rgba.chunks_exact(4).any(|pixel| pixel[3] > 0 && (pixel[0] != pixel[1] || pixel[1] != pixel[2]));
        let inked = opaque(&party) > 0;
        if ask_fontconfig('🎉').is_some_and(|(file, _)| file.to_string_lossy().contains("Color")) {
            assert!(colored || inked, "an emoji paints something");
        }
        let arabic = font.runs("مرحبا.txt");
        assert_eq!(arabic[0].1, Script::Arabic);
        assert!(arabic[0].2.end <= "مرحبا".len(), "the dot and the extension are their own run");
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
