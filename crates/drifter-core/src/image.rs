use std::path::Path;

use crate::geom::Vec2;

#[derive(Clone)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Image {
    pub fn size(&self) -> Vec2 {
        Vec2::new(f64::from(self.width), f64::from(self.height))
    }

    pub fn premultiply(&mut self) {
        for pixel in self.rgba.as_chunks_mut::<4>().0 {
            let a = u32::from(pixel[3]);
            let (color, _) = pixel.split_at_mut(3);
            for channel in color {
                *channel = ((u32::from(*channel) * a + 127) / 255) as u8;
            }
        }
    }

    pub fn straighten(&mut self) {
        for pixel in self.rgba.as_chunks_mut::<4>().0 {
            let a = u32::from(pixel[3]);
            if a == 0 || a == 255 {
                continue;
            }
            let (color, _) = pixel.split_at_mut(3);
            for channel in color {
                *channel = ((u32::from(*channel) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }

    pub fn tint(&mut self, ink: [u8; 3]) {
        for pixel in self.rgba.as_chunks_mut::<4>().0 {
            let a = u32::from(pixel[3]);
            for (channel, value) in pixel[..3].iter_mut().zip(ink) {
                *channel = ((u32::from(value) * a + 127) / 255) as u8;
            }
        }
    }

    pub fn tiled(&self, width: u32, height: u32) -> Image {
        let width = width.max(1);
        let height = height.max(1);
        if self.width == 0 || self.height == 0 {
            return Image {
                width,
                height,
                rgba: vec![0; (width as usize) * (height as usize) * 4],
            };
        }
        let mut rgba = Vec::with_capacity((width as usize) * (height as usize) * 4);
        for y in 0..height {
            let row = (y % self.height) as usize * self.width as usize;
            for x in 0..width {
                let at = (row + (x % self.width) as usize) * 4;
                rgba.extend_from_slice(&self.rgba[at..at + 4]);
            }
        }
        Image {
            width,
            height,
            rgba,
        }
    }

    pub fn scaled(&self, width: u32, height: u32) -> Image {
        let width = width.max(1);
        let height = height.max(1);
        if width == self.width && height == self.height {
            return Image {
                width,
                height,
                rgba: self.rgba.clone(),
            };
        }

        let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
        let across = f64::from(self.width) / f64::from(width);
        let down = f64::from(self.height) / f64::from(height);

        for y in 0..height {
            let from_y = ((f64::from(y) + 0.5) * down).floor() as u32;
            let from_y = from_y.min(self.height - 1);
            for x in 0..width {
                let from_x = ((f64::from(x) + 0.5) * across).floor() as u32;
                let from_x = from_x.min(self.width - 1);
                let source = ((from_y as usize) * (self.width as usize) + from_x as usize) * 4;
                let target = ((y as usize) * (width as usize) + x as usize) * 4;
                rgba[target..target + 4].copy_from_slice(&self.rgba[source..source + 4]);
            }
        }

        Image {
            width,
            height,
            rgba,
        }
    }
}

pub fn save_png(image: &Image, path: &Path) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut straight = image.clone();
    straight.straighten();
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&straight.rgba))
        .map_err(|e| format!("{}: {e}", path.display()))
}

pub fn save_jpeg(image: &Image, path: &Path) -> Result<(), String> {
    let width = u16::try_from(image.width).map_err(|_| "the picture is too wide for JPEG".to_string())?;
    let height = u16::try_from(image.height).map_err(|_| "the picture is too tall for JPEG".to_string())?;
    let rgb: Vec<u8> = image
        .rgba
        .chunks_exact(4)
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2]])
        .collect();
    let encoder = jpeg_encoder::Encoder::new_file(path, 92).map_err(|e| format!("{}: {e}", path.display()))?;
    encoder
        .encode(&rgb, width, height, jpeg_encoder::ColorType::Rgb)
        .map_err(|e| format!("{}: {e}", path.display()))
}

pub fn save_as(image: &Image, path: &Path, format: crate::config::ShotFormat) -> Result<(), String> {
    match format {
        crate::config::ShotFormat::Png => save_png(image, path),
        crate::config::ShotFormat::Jpeg => save_jpeg(image, path),
    }
}

pub const SVG_SIZE: u32 = 256;

pub fn load(path: &Path) -> Result<Image, String> {
    load_sized(path, SVG_SIZE)
}

pub fn load_sized(path: &Path, want: u32) -> Result<Image, String> {
    if crate::shader::is_shader(path) {
        return Ok(shader_still(path, want.max(1) * 3 / 2, want.max(1)));
    }
    load_with(path, |bytes| draw_svg(bytes, want))
}

pub fn load_covering(path: &Path, width: f64, height: f64) -> Result<Image, String> {
    if crate::shader::is_shader(path) {
        let wide = width.max(1.0).min(STILL_WIDE);
        let tall = (height.max(1.0) * wide / width.max(1.0)).round().max(1.0);
        return Ok(shader_still(path, wide as u32, tall as u32));
    }
    load_with(path, |bytes| cover_svg(bytes, width, height))
}

pub const STILL_WIDE: f64 = 960.0;

fn shader_still(path: &Path, width: u32, height: u32) -> Image {
    crate::shader::still(path, width, height, 0.0, crate::shader::hour_now())
}

fn load_with(
    path: &Path,
    svg: impl FnOnce(&[u8]) -> Result<Image, String>,
) -> Result<Image, String> {
    let bytes = match crate::livepaper::is_livepaper(path) {
        true => crate::livepaper::frame(path, crate::livepaper::hour_now())?,
        false => std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?,
    };
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return load_png(path, &bytes);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return load_jpeg(path, &bytes);
    }
    if is_svg(&bytes) {
        return svg(&bytes).map_err(|e| format!("{}: {e}", path.display()));
    }
    Err(format!("{}: not a png, a jpeg or an svg", path.display()))
}

fn is_svg(bytes: &[u8]) -> bool {
    let head = &bytes[..bytes.len().min(1024)];
    let text = String::from_utf8_lossy(head);
    let text = text.trim_start_matches('\u{feff}').trim_start();
    (text.starts_with("<?xml") || text.starts_with("<svg") || text.starts_with("<!--"))
        && text.contains("<svg")
}

pub fn draw_svg(bytes: &[u8], want: u32) -> Result<Image, String> {
    let tree = svg_tree(bytes)?;
    let size = tree.size();
    let longest = size.width().max(size.height());
    if longest <= 0.0 {
        return Err("the svg has no size".to_string());
    }
    render_svg(&tree, want.max(1) as f32 / longest)
}

pub const SVG_PIXELS: f64 = 5120.0 * 2880.0;

fn cover_svg(bytes: &[u8], width: f64, height: f64) -> Result<Image, String> {
    if width <= 0.0 || height <= 0.0 {
        return draw_svg(bytes, SVG_SIZE);
    }
    let tree = svg_tree(bytes)?;
    let size = tree.size();
    if size.width() <= 0.0 || size.height() <= 0.0 {
        return Err("the svg has no size".to_string());
    }
    let (w, h) = (f64::from(size.width()), f64::from(size.height()));
    let cover = (width / w).max(height / h);
    let most = (SVG_PIXELS / (w * h)).sqrt();
    render_svg(&tree, cover.min(most) as f32)
}

fn svg_tree(bytes: &[u8]) -> Result<resvg::usvg::Tree, String> {
    resvg::usvg::Tree::from_data(bytes, &resvg::usvg::Options::default()).map_err(|e| e.to_string())
}

fn render_svg(tree: &resvg::usvg::Tree, scale: f32) -> Result<Image, String> {
    use resvg::tiny_skia;

    let size = tree.size();
    let width = (size.width() * scale).round().max(1.0) as u32;
    let height = (size.height() * scale).round().max(1.0) as u32;
    let mut pixmap =
        tiny_skia::Pixmap::new(width, height).ok_or_else(|| "the svg is too big".to_string())?;
    resvg::render(
        tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(Image {
        width,
        height,
        rgba: pixmap.take(),
    })
}

fn load_jpeg(path: &Path, bytes: &[u8]) -> Result<Image, String> {
    let mut decoder = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
    let pixels = decoder
        .decode()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let info = decoder
        .info()
        .ok_or_else(|| format!("{}: the jpeg has no header", path.display()))?;

    let rgba: Vec<u8> = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        jpeg_decoder::PixelFormat::L8 => pixels.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        jpeg_decoder::PixelFormat::L16 => {
            let samples: Vec<u16> = pixels
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u16::from_ne_bytes(*pair))
                .collect();
            let top = samples.iter().copied().max().unwrap_or(0);
            let shift = (16 - top.leading_zeros()).saturating_sub(8);
            samples
                .iter()
                .flat_map(|sample| {
                    let g = (sample >> shift) as u8;
                    [g, g, g, 255]
                })
                .collect()
        }
        jpeg_decoder::PixelFormat::CMYK32 => pixels
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| {
                let white = 255 - u32::from(p[3]);
                let ink = |c: u8| ((255 - u32::from(c)) * white / 255) as u8;
                [ink(p[0]), ink(p[1]), ink(p[2]), 255]
            })
            .collect(),
    };

    let width = u32::from(info.width);
    let height = u32::from(info.height);
    fits(path, width, height, &rgba)?;
    let mut image = Image {
        width,
        height,
        rgba,
    };
    if let Some(turn) = decoder.exif_data().and_then(orientation) {
        image = orient(&image, turn);
    }
    image.premultiply();
    Ok(image)
}

fn fits(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    if rgba.len() != (width as usize) * (height as usize) * 4 {
        return Err(format!(
            "{}: decoded {} bytes for a {width} x {height} image",
            path.display(),
            rgba.len()
        ));
    }
    Ok(())
}

fn orientation(exif: &[u8]) -> Option<u16> {
    let big = match exif.get(0..2)? {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    let short = |at: usize| -> Option<u16> {
        let pair: [u8; 2] = exif.get(at..at + 2)?.try_into().ok()?;
        Some(if big {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        })
    };
    let long = |at: usize| -> Option<u32> {
        let four: [u8; 4] = exif.get(at..at + 4)?.try_into().ok()?;
        Some(if big {
            u32::from_be_bytes(four)
        } else {
            u32::from_le_bytes(four)
        })
    };
    if short(2)? != 42 {
        return None;
    }
    let table = long(4)? as usize;
    let count = short(table)? as usize;
    (0..count).find_map(|entry| {
        let at = table + 2 + entry * 12;
        (short(at)? == 0x0112 && short(at + 2)? == 3)
            .then(|| short(at + 8))
            .flatten()
    })
}

pub fn orient(image: &Image, turn: u16) -> Image {
    if !(2..=8).contains(&turn) {
        return image.clone();
    }
    let (w, h) = (image.width as usize, image.height as usize);
    let sideways = turn >= 5;
    let (out_w, out_h) = if sideways { (h, w) } else { (w, h) };
    let mut rgba = vec![0u8; out_w * out_h * 4];
    for y in 0..out_h {
        for x in 0..out_w {
            let (sx, sy) = match turn {
                2 => (w - 1 - x, y),
                3 => (w - 1 - x, h - 1 - y),
                4 => (x, h - 1 - y),
                5 => (y, x),
                6 => (y, h - 1 - x),
                7 => (w - 1 - y, h - 1 - x),
                _ => (w - 1 - y, x),
            };
            let from = (sy * w + sx) * 4;
            let to = (y * out_w + x) * 4;
            rgba[to..to + 4].copy_from_slice(&image.rgba[from..from + 4]);
        }
    }
    Image {
        width: out_w as u32,
        height: out_h as u32,
        rgba,
    }
}

fn load_png(path: &Path, bytes: &[u8]) -> Result<Image, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder
        .read_info()
        .map_err(|e| format!("{}: {e}", path.display()))?;

    let mut buffer = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut buffer)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    buffer.truncate(info.buffer_size());

    let rgba = match info.color_type {
        png::ColorType::Rgba => buffer,
        png::ColorType::Rgb => buffer
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => buffer.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buffer
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        other => {
            return Err(format!(
                "{}: unsupported png color type {other:?}",
                path.display()
            ));
        }
    };

    fits(path, info.width, info.height, &rgba)?;
    let mut image = Image {
        width: info.width,
        height: info.height,
        rgba,
    };
    image.premultiply();
    Ok(image)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_jpeg_screenshot_reads_back_close_to_what_was_saved() {
        let mut rgba = Vec::new();
        for y in 0..32u32 {
            for x in 0..48u32 {
                let red = if x < 24 { 220 } else { 30 };
                rgba.extend_from_slice(&[red, (y * 6) as u8, 90, 255]);
            }
        }
        let image = Image { width: 48, height: 32, rgba };
        let path = std::env::temp_dir().join(format!("drifter-shot-{}.jpg", std::process::id()));
        super::save_as(&image, &path, crate::config::ShotFormat::Jpeg).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..2], &[0xFF, 0xD8], "the file is a JPEG");
        let back = super::load(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!((back.width, back.height), (48, 32));
        let at = |x: u32, y: u32| back.rgba[((y * 48 + x) * 4) as usize];
        assert!(at(4, 10) > 180 && at(44, 10) < 70, "left is red, right is not");
    }

    use super::*;

    const SQUARE: &str = "<?xml version=\"1.0\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"16\" height=\"8\"><rect x=\"0\" y=\"0\" width=\"8\" height=\"8\" fill=\"#ff0000\"/></svg>";

    #[test]
    fn an_svg_is_drawn_at_the_size_asked_for() {
        let dir = std::env::temp_dir().join(format!("drifter-svg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("icon.svg");
        std::fs::write(&path, SQUARE).unwrap();
        let image = load_sized(&path, 32).unwrap();
        assert_eq!((image.width, image.height), (32, 16));
        assert_eq!(image.rgba[0..4], [255, 0, 0, 255]);
        let right = ((8 * 32 + 24) * 4) as usize;
        assert_eq!(image.rgba[right + 3], 0);
        assert_eq!(load(&path).unwrap().width, SVG_SIZE);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_covering_svg_fills_the_box_and_a_picture_keeps_its_size() {
        let dir = std::env::temp_dir().join(format!("drifter-cover-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wall.svg");
        std::fs::write(&path, SQUARE).unwrap();
        let wide = load_covering(&path, 2000.0, 500.0).unwrap();
        assert_eq!((wide.width, wide.height), (2000, 1000));
        let tall = load_covering(&path, 500.0, 1500.0).unwrap();
        assert_eq!((tall.width, tall.height), (3000, 1500));
        let world = load_covering(&path, 21600.0, 14400.0).unwrap();
        assert!(f64::from(world.width) * f64::from(world.height) <= SVG_PIXELS + 10_000.0);
        assert!(world.width.abs_diff(world.height * 2) <= 2, "a capped svg keeps its shape");
        assert_eq!(load_covering(&path, 0.0, 0.0).unwrap().width, SVG_SIZE);
        let png = dir.join("wall.png");
        write_png(&png, 3, 2, png::ColorType::Rgba, &[255; 24]);
        let kept = load_covering(&png, 4000.0, 1000.0).unwrap();
        assert_eq!((kept.width, kept.height), (3, 2));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_tint_keeps_the_alpha() {
        let mut image = Image {
            width: 2,
            height: 1,
            rgba: vec![10, 20, 30, 0, 40, 50, 60, 128],
        };
        image.tint([200, 201, 202]);
        assert_eq!(image.rgba, [0, 0, 0, 0, 100, 101, 101, 128]);
    }

    #[test]
    fn text_that_is_not_svg_is_refused() {
        assert!(!is_svg(b"hello"));
        assert!(is_svg(b"\n  <svg xmlns='x'></svg>"));
        assert!(draw_svg(b"<svg", 16).is_err());
    }

    fn write_png(path: &Path, width: u32, height: u32, color: png::ColorType, data: &[u8]) {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
        encoder.set_color(color);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(data)
            .unwrap();
    }

    fn scratch(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("drifter-image-tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn loads_an_rgba_png_unchanged() {
        let path = scratch("rgba.png");
        write_png(&path, 2, 1, png::ColorType::Rgba, &[1, 2, 3, 4, 5, 6, 7, 8]);
        let image = load(&path).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.rgba, vec![0, 0, 0, 4, 0, 0, 0, 8]);
    }

    #[test]
    fn expands_rgb_to_rgba_with_opaque_alpha() {
        let path = scratch("rgb.png");
        write_png(&path, 2, 1, png::ColorType::Rgb, &[9, 8, 7, 6, 5, 4]);
        let image = load(&path).unwrap();
        assert_eq!(image.rgba, vec![9, 8, 7, 255, 6, 5, 4, 255]);
    }

    fn write_png_deep(
        path: &Path,
        (width, height): (u32, u32),
        color: png::ColorType,
        depth: png::BitDepth,
        palette: Option<&[u8]>,
        data: &[u8],
    ) {
        let file = std::fs::File::create(path).unwrap();
        let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
        encoder.set_color(color);
        encoder.set_depth(depth);
        if let Some(palette) = palette {
            encoder.set_palette(palette.to_vec());
        }
        encoder
            .write_header()
            .unwrap()
            .write_image_data(data)
            .unwrap();
    }

    #[test]
    fn loads_a_palette_png() {
        let path = scratch("palette.png");
        let palette = [255, 0, 0, 0, 0, 255];
        write_png_deep(
            &path,
            (2, 1),
            png::ColorType::Indexed,
            png::BitDepth::Eight,
            Some(&palette),
            &[1, 0],
        );
        let image = load(&path).unwrap();
        assert_eq!(image.rgba, vec![0, 0, 255, 255, 255, 0, 0, 255]);
    }

    #[test]
    fn loads_a_sixteen_bit_png_as_eight_bit() {
        let path = scratch("deep.png");
        let pixel = [0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xff, 0xff];
        write_png_deep(
            &path,
            (1, 1),
            png::ColorType::Rgba,
            png::BitDepth::Sixteen,
            None,
            &pixel,
        );
        let image = load(&path).unwrap();
        assert_eq!(image.rgba, vec![0x12, 0x56, 0x9a, 0xff]);
    }

    #[test]
    fn loads_a_one_bit_gray_png_without_panicking() {
        let path = scratch("bits.png");
        write_png_deep(
            &path,
            (8, 1),
            png::ColorType::Grayscale,
            png::BitDepth::One,
            None,
            &[0b1010_0000],
        );
        let image = load(&path).unwrap();
        assert_eq!(image.rgba.len(), 8 * 4);
        assert_eq!(&image.rgba[0..8], &[255, 255, 255, 255, 0, 0, 0, 255]);
        let scaled = image.scaled(3, 3);
        assert_eq!(scaled.rgba.len(), 3 * 3 * 4);
    }

    fn exif(big: bool, turn: u16) -> Vec<u8> {
        let short = |v: u16| {
            if big {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let long = |v: u32| {
            if big {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        };
        let mut data = Vec::new();
        data.extend_from_slice(if big { b"MM" } else { b"II" });
        data.extend_from_slice(&short(42));
        data.extend_from_slice(&long(8));
        data.extend_from_slice(&short(2));
        data.extend_from_slice(&short(0x010f));
        data.extend_from_slice(&short(2));
        data.extend_from_slice(&long(4));
        data.extend_from_slice(b"ACME");
        data.extend_from_slice(&short(0x0112));
        data.extend_from_slice(&short(3));
        data.extend_from_slice(&long(1));
        data.extend_from_slice(&short(turn));
        data.extend_from_slice(&[0, 0]);
        data
    }

    #[test]
    fn reads_the_orientation_in_either_byte_order() {
        assert_eq!(orientation(&exif(true, 6)), Some(6));
        assert_eq!(orientation(&exif(false, 8)), Some(8));
        assert_eq!(orientation(b"junk"), None);
        assert_eq!(orientation(&exif(false, 6)[..12]), None);
    }

    fn strip() -> Image {
        Image {
            width: 3,
            height: 2,
            rgba: (0..6u8).flat_map(|n| [n, 0, 0, 255]).collect(),
        }
    }

    fn reds(image: &Image) -> Vec<u8> {
        image.rgba.as_chunks::<4>().0.iter().map(|p| p[0]).collect()
    }

    #[test]
    fn a_phone_photo_turned_right_stands_up() {
        let turned = orient(&strip(), 6);
        assert_eq!((turned.width, turned.height), (2, 3));
        assert_eq!(reds(&turned), vec![3, 0, 4, 1, 5, 2]);
    }

    #[test]
    fn every_orientation_keeps_every_pixel() {
        for turn in 1..=8 {
            let turned = orient(&strip(), turn);
            let mut seen = reds(&turned);
            seen.sort();
            assert_eq!(seen, vec![0, 1, 2, 3, 4, 5], "turn {turn}");
        }
        assert_eq!(reds(&orient(&strip(), 3)), vec![5, 4, 3, 2, 1, 0]);
        assert_eq!(reds(&orient(&strip(), 8)), vec![2, 5, 1, 4, 0, 3]);
    }

    #[test]
    fn tiling_repeats_the_piece_instead_of_stretching_it() {
        let piece = Image {
            width: 2,
            height: 1,
            rgba: vec![1, 0, 0, 255, 2, 0, 0, 255],
        };
        let long = piece.tiled(5, 2);
        assert_eq!((long.width, long.height), (5, 2));
        assert_eq!(reds(&long), vec![1, 2, 1, 2, 1, 1, 2, 1, 2, 1]);
    }

    #[test]
    fn expands_grayscale_to_rgba() {
        let path = scratch("gray.png");
        write_png(&path, 2, 1, png::ColorType::Grayscale, &[10, 20]);
        let image = load(&path).unwrap();
        assert_eq!(image.rgba, vec![10, 10, 10, 255, 20, 20, 20, 255]);
    }

    #[test]
    fn the_buffer_is_always_four_bytes_per_pixel() {
        let path = scratch("size.png");
        write_png(&path, 4, 3, png::ColorType::Rgb, &[0; 36]);
        let image = load(&path).unwrap();
        assert_eq!(image.rgba.len() as u32, image.width * image.height * 4);
    }

    #[test]
    fn size_matches_the_header() {
        let path = scratch("dims.png");
        write_png(&path, 7, 5, png::ColorType::Rgba, &[0; 140]);
        let image = load(&path).unwrap();
        assert_eq!(image.size(), Vec2::new(7.0, 5.0));
    }

    const TINY_JPEG: &str = "ffd8ffe000104a46494600010100000100010000ffdb0043000302020302020303030304030304050805050404050a070706080c0a0c0c0b0a0b0b0d0e12100d0e110e0b0b1016101113141515150c0f171816141812141514ffdb00430103040405040509050509140d0b0d1414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414141414ffc00011080002000403012200021101031101ffc4001f0000010501010101010100000000000000000102030405060708090a0bffc400b5100002010303020403050504040000017d01020300041105122131410613516107227114328191a1082342b1c11552d1f02433627282090a161718191a25262728292a3435363738393a434445464748494a535455565758595a636465666768696a737475767778797a838485868788898a92939495969798999aa2a3a4a5a6a7a8a9aab2b3b4b5b6b7b8b9bac2c3c4c5c6c7c8c9cad2d3d4d5d6d7d8d9dae1e2e3e4e5e6e7e8e9eaf1f2f3f4f5f6f7f8f9faffc4001f0100030101010101010101010000000000000102030405060708090a0bffc400b51100020102040403040705040400010277000102031104052131061241510761711322328108144291a1b1c109233352f0156272d10a162434e125f11718191a262728292a35363738393a434445464748494a535455565758595a636465666768696a737475767778797a82838485868788898a92939495969798999aa2a3a4a5a6a7a8a9aab2b3b4b5b6b7b8b9bac2c3c4c5c6c7c8c9cad2d3d4d5d6d7d8d9dae2e3e4e5e6e7e8e9eaf2f3f4f5f6f7f8f9faffda000c03010002110311003f00f04a28a2bf3f3fb80fffd9";

    fn hex(text: &str) -> Vec<u8> {
        text.as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    #[test]
    fn loads_a_jpeg_as_rgba() {
        let path = scratch("photo.jpeg");
        std::fs::write(&path, hex(TINY_JPEG)).unwrap();
        let image = load(&path).unwrap();
        assert_eq!((image.width, image.height), (4, 2));
        assert_eq!(image.rgba.len(), 4 * 2 * 4);
        assert!(image.rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
    }

    #[test]
    fn a_jpeg_is_read_by_its_bytes_not_by_its_name() {
        let path = scratch("liar.png");
        std::fs::write(&path, hex(TINY_JPEG)).unwrap();
        let image = load(&path).unwrap();
        assert_eq!((image.width, image.height), (4, 2));
    }

    #[test]
    fn straightening_undoes_premultiplying() {
        let mut image = Image {
            width: 2,
            height: 1,
            rgba: vec![200, 100, 50, 128, 255, 255, 255, 255],
        };
        let before = image.rgba.clone();
        image.premultiply();
        assert_ne!(image.rgba, before, "premultiply did nothing");
        image.straighten();
        for (now, then) in image.rgba.iter().zip(before.iter()) {
            assert!(
                now.abs_diff(*then) <= 1,
                "the round trip lost the color: {now} against {then}"
            );
        }
    }

    #[test]
    fn straightening_leaves_a_clear_pixel_alone() {
        let mut image = Image {
            width: 1,
            height: 1,
            rgba: vec![0, 0, 0, 0],
        };
        image.straighten();
        assert_eq!(image.rgba, vec![0, 0, 0, 0]);
    }

    #[test]
    fn loading_premultiplies_so_the_renderer_sees_what_it_expects() {
        let path = scratch("half.png");
        write_png(&path, 1, 1, png::ColorType::Rgba, &[255, 255, 255, 128]);
        let image = load(&path).unwrap();
        assert_eq!(image.rgba, vec![128, 128, 128, 128]);
    }

    #[test]
    fn a_fully_opaque_image_is_unchanged_by_premultiplying() {
        let path = scratch("opaque.png");
        write_png(&path, 2, 1, png::ColorType::Rgb, &[9, 8, 7, 6, 5, 4]);
        let image = load(&path).unwrap();
        assert_eq!(image.rgba, vec![9, 8, 7, 255, 6, 5, 4, 255]);
    }

    #[test]
    fn a_clear_pixel_loses_its_color() {
        let path = scratch("clear.png");
        write_png(&path, 1, 1, png::ColorType::Rgba, &[200, 100, 50, 0]);
        let image = load(&path).unwrap();
        assert_eq!(image.rgba, vec![0, 0, 0, 0]);
    }

    #[test]
    fn scaling_down_keeps_the_asked_for_size() {
        let path = scratch("big.png");
        write_png(&path, 8, 8, png::ColorType::Rgba, &[255; 8 * 8 * 4]);
        let image = load(&path).unwrap().scaled(24, 24);
        assert_eq!((image.width, image.height), (24, 24));
        assert_eq!(image.rgba.len(), 24 * 24 * 4);
    }

    #[test]
    fn scaling_to_the_same_size_changes_nothing() {
        let path = scratch("same.png");
        write_png(&path, 2, 2, png::ColorType::Rgba, &[7; 16]);
        let image = load(&path).unwrap();
        let same = image.scaled(2, 2);
        assert_eq!(same.rgba, image.rgba);
    }

    #[test]
    fn scaling_keeps_the_corners_where_they_belong() {
        let mut image = Image {
            width: 2,
            height: 2,
            rgba: vec![1, 1, 1, 255, 2, 2, 2, 255, 3, 3, 3, 255, 4, 4, 4, 255],
        };
        image.premultiply();
        let big = image.scaled(4, 4);
        assert_eq!(big.rgba[0], 1);
        assert_eq!(big.rgba[(3 * 4 + 3) * 4], 4);
    }

    #[test]
    fn a_saved_png_loads_back_the_same() {
        let path = std::env::temp_dir().join(format!("drifter-save-{}.png", std::process::id()));
        let image = Image {
            width: 2,
            height: 1,
            rgba: vec![10, 20, 30, 255, 0, 0, 0, 0],
        };
        save_png(&image, &path).unwrap();
        let back = load(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(back.rgba, image.rgba);
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        assert!(load(Path::new("/nonexistent/nope.png")).is_err());
    }

    #[test]
    fn a_file_that_is_not_a_png_is_an_error() {
        let path = scratch("notapng.png");
        std::fs::write(&path, b"this is not a png").unwrap();
        assert!(load(&path).is_err());
    }
}
