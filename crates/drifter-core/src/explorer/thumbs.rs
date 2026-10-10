use std::io::Write;
use std::path::{Path, PathBuf};

use crate::Image;

pub const SIZES: [(u32, &str); 4] = [(128, "normal"), (256, "large"), (512, "x-large"), (1024, "xx-large")];
const FAILED: &str = "fail/drifter-explorer";

pub fn md5(data: &[u8]) -> [u8; 16] {
    const SHIFT: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let table: Vec<u32> = (0..64).map(|i| ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32).collect();
    let mut message = data.to_vec();
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&((data.len() as u64).wrapping_mul(8)).to_le_bytes());
    let mut state: [u32; 4] = [0x67452301, 0xefcdab89, 0x98badcfe, 0x10325476];
    for block in message.chunks_exact(64) {
        let words: Vec<u32> = block.chunks_exact(4).map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]])).collect();
        let [mut a, mut b, mut c, mut d] = state;
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let turned = a.wrapping_add(f).wrapping_add(table[i]).wrapping_add(words[g]).rotate_left(SHIFT[i]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(turned);
        }
        state = [state[0].wrapping_add(a), state[1].wrapping_add(b), state[2].wrapping_add(c), state[3].wrapping_add(d)];
    }
    let mut out = [0u8; 16];
    for (at, word) in state.iter().enumerate() {
        out[at * 4..at * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    out
}

pub fn uri(path: &Path) -> String {
    let mut out = String::from("file://");
    for byte in path.as_os_str().as_encoded_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b'-' | b'.' | b'/' | b':' | b'=' | b'@' | b'_' | b'~' => out.push(*byte as char),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

pub fn root() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| super::model::home().join(".cache"))
        .join("thumbnails")
}

pub fn bucket(side: u32) -> (u32, &'static str) {
    SIZES.iter().copied().find(|(size, _)| *size >= side).unwrap_or(SIZES[SIZES.len() - 1])
}

fn file_for(root: &Path, folder: &str, uri: &str) -> PathBuf {
    let sum: String = md5(uri.as_bytes()).iter().map(|byte| format!("{byte:02x}")).collect();
    root.join(folder).join(format!("{sum}.png"))
}

pub fn stamp(path: &Path) -> Option<u64> {
    let meta = std::fs::metadata(path).ok()?;
    meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok().map(|since| since.as_secs())
}

fn read(file: &Path, uri: &str, mtime: u64) -> Option<Image> {
    let mut decoder = png::Decoder::new(std::io::BufReader::new(std::fs::File::open(file).ok()?));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().ok()?;
    let info = reader.info();
    let text = |key: &str| info.uncompressed_latin1_text.iter().find(|chunk| chunk.keyword == key).map(|chunk| chunk.text.clone());
    if text("Thumb::URI")? != uri || text("Thumb::MTime")?.trim().parse::<u64>().ok()? != mtime {
        return None;
    }
    let mut buffer = vec![0u8; reader.output_buffer_size()?];
    let frame = reader.next_frame(&mut buffer).ok()?;
    let pixels = &buffer[..frame.buffer_size()];
    let rgba: Vec<u8> = match frame.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels.chunks_exact(3).flat_map(|p| [p[0], p[1], p[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => pixels.chunks_exact(2).flat_map(|p| [p[0], p[0], p[0], p[1]]).collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|g| [*g, *g, *g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    let mut image = Image { width: frame.width, height: frame.height, rgba };
    image.premultiply();
    Some(image)
}

pub fn cached_in(root: &Path, path: &Path, side: u32) -> Option<Image> {
    let mtime = stamp(path)?;
    let uri = uri(path);
    SIZES.iter().filter(|(size, _)| *size >= side).find_map(|(_, folder)| read(&file_for(root, folder, &uri), &uri, mtime))
}

pub fn failed_in(root: &Path, path: &Path) -> bool {
    let (Some(mtime), uri) = (stamp(path), uri(path)) else {
        return false;
    };
    read(&file_for(root, FAILED, &uri), &uri, mtime).is_some()
}

fn write(root: &Path, folder: &str, path: &Path, image: &Image) -> Result<(), String> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
    let mtime = stamp(path).ok_or("no time")?;
    let uri = uri(path);
    let target = file_for(root, folder, &uri);
    let dir = target.parent().ok_or("no folder")?;
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir).map_err(|e| e.to_string())?;
    let spare = dir.join(format!(".{}-{}.png", std::process::id(), target.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default()));
    let file = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&spare).map_err(|e| e.to_string())?;
    let mut straight = image.clone();
    straight.straighten();
    let written = (|| -> Result<(), png::EncodingError> {
        let mut out = std::io::BufWriter::new(file);
        {
            let mut encoder = png::Encoder::new(&mut out, straight.width, straight.height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder.add_text_chunk("Thumb::URI".to_string(), uri.clone())?;
            encoder.add_text_chunk("Thumb::MTime".to_string(), mtime.to_string())?;
            encoder.add_text_chunk("Software".to_string(), "Drifter Explorer".to_string())?;
            encoder.write_header()?.write_image_data(&straight.rgba)?;
        }
        out.flush()?;
        Ok(())
    })();
    if let Err(error) = written {
        let _ = std::fs::remove_file(&spare);
        return Err(error.to_string());
    }
    std::fs::rename(&spare, &target).map_err(|e| e.to_string())
}

pub fn store_in(root: &Path, path: &Path, side: u32, image: &Image) -> Result<(), String> {
    write(root, bucket(side).1, path, image)
}

pub fn store_failed_in(root: &Path, path: &Path) -> Result<(), String> {
    write(root, FAILED, path, &Image { width: 1, height: 1, rgba: vec![0; 4] })
}

pub fn worth_caching(path: &Path, root: &Path) -> bool {
    !path.starts_with(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(sum: [u8; 16]) -> String {
        sum.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn md5_matches_known_sums() {
        assert_eq!(hex(md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(hex(md5(b"The quick brown fox jumps over the lazy dog")), "9e107d9d372bb6826bd81d3542a419d6");
        assert_eq!(hex(md5(&[b'a'; 200])), hex(md5(&vec![b'a'; 200])));
    }

    #[test]
    fn names_follow_the_thumbnail_spec() {
        assert_eq!(uri(Path::new("/home/jens/photos/me.png")), "file:///home/jens/photos/me.png");
        assert_eq!(hex(md5(uri(Path::new("/home/jens/photos/me.png")).as_bytes())), "c6ee772d9e49320e97ec29a7eb5b1697");
        assert_eq!(uri(Path::new("/a b/100%#x.png")), "file:///a%20b/100%25%23x.png");
        assert_eq!(bucket(72), (128, "normal"));
        assert_eq!(bucket(200), (256, "large"));
        assert_eq!(bucket(4000), (1024, "xx-large"));
    }

    #[test]
    fn a_stored_preview_comes_back_until_the_file_changes() {
        let dir = std::env::temp_dir().join(format!("drifter-thumbs-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let root = dir.join("cache");
        let file = dir.join("photo.png");
        std::fs::write(&file, "x").unwrap();
        let mut image = Image { width: 2, height: 1, rgba: vec![255, 0, 0, 255, 0, 0, 255, 128] };
        image.premultiply();
        assert!(cached_in(&root, &file, 100).is_none());
        store_in(&root, &file, 100, &image).unwrap();
        let back = cached_in(&root, &file, 100).unwrap();
        assert_eq!((back.width, back.height), (2, 1));
        assert_eq!(back.rgba[..4], [255, 0, 0, 255]);
        assert!(cached_in(&root, &file, 300).is_none());
        std::fs::File::options().write(true).open(&file).unwrap().set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(5)).unwrap();
        assert!(cached_in(&root, &file, 100).is_none());
        assert!(!failed_in(&root, &file));
        store_failed_in(&root, &file).unwrap();
        assert!(failed_in(&root, &file));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
