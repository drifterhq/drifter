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
}

pub fn load_png(path: &Path) -> Result<Image, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let decoder = png::Decoder::new(std::io::BufReader::new(file));
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
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => buffer.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buffer
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        other => {
            return Err(format!(
                "{}: unsupported png color type {other:?}",
                path.display()
            ));
        }
    };

    Ok(Image {
        width: info.width,
        height: info.height,
        rgba,
    })
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
        let image = load_png(&path).unwrap();
        assert_eq!((image.width, image.height), (2, 1));
        assert_eq!(image.rgba, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn expands_rgb_to_rgba_with_opaque_alpha() {
        let path = scratch("rgb.png");
        write_png(&path, 2, 1, png::ColorType::Rgb, &[9, 8, 7, 6, 5, 4]);
        let image = load_png(&path).unwrap();
        assert_eq!(image.rgba, vec![9, 8, 7, 255, 6, 5, 4, 255]);
    }

    #[test]
    fn expands_grayscale_to_rgba() {
        let path = scratch("gray.png");
        write_png(&path, 2, 1, png::ColorType::Grayscale, &[10, 20]);
        let image = load_png(&path).unwrap();
        assert_eq!(image.rgba, vec![10, 10, 10, 255, 20, 20, 20, 255]);
    }

    #[test]
    fn the_buffer_is_always_four_bytes_per_pixel() {
        let path = scratch("size.png");
        write_png(&path, 4, 3, png::ColorType::Rgb, &[0; 36]);
        let image = load_png(&path).unwrap();
        assert_eq!(image.rgba.len() as u32, image.width * image.height * 4);
    }

    #[test]
    fn size_matches_the_header() {
        let path = scratch("dims.png");
        write_png(&path, 7, 5, png::ColorType::Rgba, &[0; 140]);
        let image = load_png(&path).unwrap();
        assert_eq!(image.size(), Vec2::new(7.0, 5.0));
    }

    #[test]
    fn a_missing_file_is_an_error_not_a_panic() {
        assert!(load_png(Path::new("/nonexistent/nope.png")).is_err());
    }

    #[test]
    fn a_file_that_is_not_a_png_is_an_error() {
        let path = scratch("notapng.png");
        std::fs::write(&path, b"this is not a png").unwrap();
        assert!(load_png(&path).is_err());
    }
}
