use std::path::Path;

use crate::geom::Vec2;

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

pub fn load(path: &Path) -> Result<Image, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        return load_png(path, &bytes);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return load_jpeg(path, &bytes);
    }
    Err(format!("{}: not a png or a jpeg", path.display()))
}

fn load_jpeg(path: &Path, bytes: &[u8]) -> Result<Image, String> {
    let mut decoder = jpeg_decoder::Decoder::new(std::io::Cursor::new(bytes));
    let pixels = decoder
        .decode()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let info = decoder
        .info()
        .ok_or_else(|| format!("{}: the jpeg has no header", path.display()))?;

    let rgba = match info.pixel_format {
        jpeg_decoder::PixelFormat::RGB24 => pixels
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        jpeg_decoder::PixelFormat::L8 => pixels.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        other => {
            return Err(format!(
                "{}: unsupported jpeg pixel format {other:?}",
                path.display()
            ));
        }
    };

    let mut image = Image {
        width: u32::from(info.width),
        height: u32::from(info.height),
        rgba,
    };
    image.premultiply();
    Ok(image)
}

fn load_png(path: &Path, bytes: &[u8]) -> Result<Image, String> {
    let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
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
    use super::*;

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
