use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub const EXTENSION: &str = "livepaper";

pub fn is_livepaper(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case(EXTENSION))
        || crate::shader::is_shader(path)
}

pub fn hour_now() -> u32 {
    use chrono::Timelike;
    chrono::Local::now().hour()
}

pub fn folders(data_home: &Path) -> Vec<PathBuf> {
    vec![
        data_home.join("drifter/livepapers"),
        PathBuf::from("/usr/local/share/drifter/livepapers"),
        PathBuf::from("/usr/share/drifter/livepapers"),
    ]
}

pub fn find(name: &str, folders: &[PathBuf]) -> Option<PathBuf> {
    folders
        .iter()
        .map(|folder| folder.join(name))
        .find(|path| path.is_file())
}

pub fn installed(folders: &[PathBuf]) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    for folder in folders {
        let Ok(entries) = std::fs::read_dir(folder) else {
            continue;
        };
        let mut here: Vec<PathBuf> = entries
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| is_livepaper(path) && path.is_file())
            .filter(|path| {
                !found
                    .iter()
                    .any(|seen| seen.file_name() == path.file_name())
            })
            .collect();
        here.sort();
        found.extend(here);
    }
    found
}

pub fn name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => stem,
    }
}

struct Entry {
    name: String,
    method: u16,
    packed: u64,
    size: u64,
    offset: u64,
}

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn entries(file: &mut File) -> Result<Vec<Entry>, String> {
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    let tail = length.min(65_557);
    file.seek(SeekFrom::Start(length - tail))
        .map_err(|e| e.to_string())?;
    let mut end = vec![0u8; tail as usize];
    file.read_exact(&mut end).map_err(|e| e.to_string())?;
    let at = (0..end.len().saturating_sub(21))
        .rev()
        .find(|at| end[*at..].starts_with(&[0x50, 0x4b, 0x05, 0x06]))
        .ok_or("not a zip file")?;
    let count = u16_at(&end, at + 10).ok_or("short zip end")? as usize;
    let dir_size = u32_at(&end, at + 12).ok_or("short zip end")? as usize;
    let dir_at = u32_at(&end, at + 16).ok_or("short zip end")? as u64;
    file.seek(SeekFrom::Start(dir_at))
        .map_err(|e| e.to_string())?;
    let mut dir = vec![0u8; dir_size];
    file.read_exact(&mut dir).map_err(|e| e.to_string())?;
    let mut out = Vec::with_capacity(count);
    let mut at = 0;
    while out.len() < count {
        if u32_at(&dir, at) != Some(0x0201_4b50) {
            return Err("broken zip directory".to_string());
        }
        let field = |offset| u16_at(&dir, at + offset).ok_or("short zip entry");
        let long = |offset| u32_at(&dir, at + offset).ok_or("short zip entry");
        let (name_len, extra_len, note_len) = (field(28)? as usize, field(30)? as usize, field(32)? as usize);
        let name = dir
            .get(at + 46..at + 46 + name_len)
            .map(|raw| String::from_utf8_lossy(raw).into_owned())
            .ok_or("short zip entry")?;
        out.push(Entry {
            name,
            method: field(10)?,
            packed: long(20)? as u64,
            size: long(24)? as u64,
            offset: long(42)? as u64,
        });
        at += 46 + name_len + extra_len + note_len;
    }
    Ok(out)
}

fn read_entry(file: &mut File, entry: &Entry) -> Result<Vec<u8>, String> {
    let mut head = [0u8; 30];
    file.seek(SeekFrom::Start(entry.offset))
        .map_err(|e| e.to_string())?;
    file.read_exact(&mut head).map_err(|e| e.to_string())?;
    if u32_at(&head, 0) != Some(0x0403_4b50) {
        return Err("broken zip entry".to_string());
    }
    let skip = u16_at(&head, 26).unwrap_or(0) as i64 + u16_at(&head, 28).unwrap_or(0) as i64;
    file.seek(SeekFrom::Current(skip))
        .map_err(|e| e.to_string())?;
    let mut packed = vec![0u8; entry.packed as usize];
    file.read_exact(&mut packed).map_err(|e| e.to_string())?;
    match entry.method {
        0 => Ok(packed),
        8 => miniz_oxide::inflate::decompress_to_vec_with_limit(&packed, entry.size as usize)
            .map_err(|e| format!("cannot inflate {}: {e:?}", entry.name)),
        method => Err(format!("{} uses zip method {method}", entry.name)),
    }
}

pub fn frame(path: &Path, hour: u32) -> Result<Vec<u8>, String> {
    let fail = |e: String| format!("{}: {e}", path.display());
    let mut file = File::open(path).map_err(|e| fail(e.to_string()))?;
    let list = entries(&mut file).map_err(fail)?;
    let wanted = format!("{:02}", hour % 24);
    let entry = list
        .iter()
        .find(|entry| {
            let base = entry.name.rsplit('/').next().unwrap_or(&entry.name);
            Path::new(base).file_stem().is_some_and(|stem| stem == wanted.as_str())
        })
        .ok_or_else(|| fail(format!("no frame for hour {wanted}")))?;
    read_entry(&mut file, entry).map_err(fail)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zip(files: &[(&str, &[u8], bool)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut dir = Vec::new();
        for (name, data, squeeze) in files {
            let packed = match squeeze {
                true => miniz_oxide::deflate::compress_to_vec(data, 6),
                false => data.to_vec(),
            };
            let method: u16 = if *squeeze { 8 } else { 0 };
            let offset = out.len() as u32;
            out.extend(0x0403_4b50u32.to_le_bytes());
            out.extend([20, 0, 0, 0]);
            out.extend(method.to_le_bytes());
            out.extend([0; 8]);
            out.extend((packed.len() as u32).to_le_bytes());
            out.extend((data.len() as u32).to_le_bytes());
            out.extend((name.len() as u16).to_le_bytes());
            out.extend(3u16.to_le_bytes());
            out.extend(name.as_bytes());
            out.extend([1, 2, 3]);
            out.extend(&packed);
            dir.extend(0x0201_4b50u32.to_le_bytes());
            dir.extend([20, 0, 20, 0, 0, 0]);
            dir.extend(method.to_le_bytes());
            dir.extend([0; 8]);
            dir.extend((packed.len() as u32).to_le_bytes());
            dir.extend((data.len() as u32).to_le_bytes());
            dir.extend((name.len() as u16).to_le_bytes());
            dir.extend([0; 12]);
            dir.extend(offset.to_le_bytes());
            dir.extend(name.as_bytes());
        }
        let dir_at = out.len() as u32;
        out.extend(&dir);
        out.extend(0x0605_4b50u32.to_le_bytes());
        out.extend([0; 4]);
        out.extend((files.len() as u16).to_le_bytes());
        out.extend((files.len() as u16).to_le_bytes());
        out.extend((dir.len() as u32).to_le_bytes());
        out.extend(dir_at.to_le_bytes());
        out.extend([0; 2]);
        out
    }

    fn saved(name: &str, bytes: &[u8]) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("drifter-livepaper-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn the_frame_for_the_hour_comes_out_stored_or_squeezed() {
        let night = vec![7u8; 5000];
        let noon: Vec<u8> = (0..5000u32).map(|n| (n % 251) as u8).collect();
        let path = saved(
            "both.livepaper",
            &zip(&[("00.png", &night, false), ("12.svg", &noon, true), ("13.jpg", b"x", false)]),
        );
        assert_eq!(frame(&path, 0).unwrap(), night);
        assert_eq!(frame(&path, 12).unwrap(), noon);
        assert_eq!(frame(&path, 36).unwrap(), noon, "hours wrap at 24");
        assert!(frame(&path, 5).unwrap_err().contains("no frame for hour 05"));
    }

    #[test]
    fn a_frame_in_a_folder_inside_the_zip_is_found() {
        let path = saved("nested.livepaper", &zip(&[("set/07.png", b"seven", false)]));
        assert_eq!(frame(&path, 7).unwrap(), b"seven");
    }

    #[test]
    fn a_livepaper_loads_as_a_picture_like_any_other() {
        let mut png = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut png, 3, 2);
            encoder.set_color(png::ColorType::Rgba);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[200u8; 3 * 2 * 4]).unwrap();
        }
        let names: Vec<String> = (0..24).map(|hour| format!("{hour:02}.png")).collect();
        let files: Vec<(&str, &[u8], bool)> = names.iter().map(|name| (name.as_str(), png.as_slice(), false)).collect();
        let path = saved("day.livepaper", &zip(&files));
        let image = crate::image::load(&path).unwrap();
        assert_eq!((image.width, image.height), (3, 2));
        assert!(crate::wallpaper::is_picture(&path));
    }

    #[test]
    fn junk_is_an_error_not_a_panic() {
        let path = saved("junk.livepaper", b"this is not a zip at all");
        assert!(frame(&path, 0).is_err());
        assert!(frame(Path::new("/no/such/file.livepaper"), 0).is_err());
    }

    #[test]
    fn livepapers_are_found_by_name_and_listed_once() {
        let root = std::env::temp_dir().join(format!("drifter-lp-dirs-{}", std::process::id()));
        let (mine, shared) = (root.join("mine"), root.join("shared"));
        std::fs::create_dir_all(&mine).unwrap();
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(mine.join("bloom.livepaper"), b"a").unwrap();
        std::fs::write(shared.join("bloom.livepaper"), b"b").unwrap();
        std::fs::write(shared.join("tiles.livepaper"), b"c").unwrap();
        std::fs::write(shared.join("note.txt"), b"d").unwrap();
        let folders = vec![mine.clone(), shared.clone()];
        assert_eq!(find("bloom.livepaper", &folders), Some(mine.join("bloom.livepaper")));
        assert_eq!(find("tiles.livepaper", &folders), Some(shared.join("tiles.livepaper")));
        assert_eq!(find("gone.livepaper", &folders), None);
        assert_eq!(
            installed(&folders),
            vec![mine.join("bloom.livepaper"), shared.join("tiles.livepaper")]
        );
        assert_eq!(name(&shared.join("tiles.livepaper")), "Tiles");
    }
}
