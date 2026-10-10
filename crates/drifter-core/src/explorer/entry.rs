use std::cmp::Ordering;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub folder: bool,
    pub link: bool,
    pub size: u64,
    pub modified: Option<SystemTime>,
    pub mime: &'static str,
    pub runs: bool,
    pub trashed: Option<(PathBuf, Option<SystemTime>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    Name,
    Modified,
    Size,
    Kind,
    Origin,
    Deleted,
}

impl Sort {
    pub const ALL: [Sort; 4] = [Sort::Name, Sort::Modified, Sort::Size, Sort::Kind];

    pub fn name(self) -> &'static str {
        match self {
            Sort::Name => crate::lang::t("Name"),
            Sort::Modified => crate::lang::t("Modified"),
            Sort::Size => crate::lang::t("Size"),
            Sort::Kind => crate::lang::t("Kind"),
            Sort::Origin => crate::lang::t("Original Location"),
            Sort::Deleted => crate::lang::t("Date Deleted"),
        }
    }
}

const KINDS: [(&str, &str); 64] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("svg", "image/svg+xml"),
    ("bmp", "image/bmp"),
    ("tif", "image/tiff"),
    ("tiff", "image/tiff"),
    ("heic", "image/heic"),
    ("avif", "image/avif"),
    ("ico", "image/vnd.microsoft.icon"),
    ("mp4", "video/mp4"),
    ("m4v", "video/mp4"),
    ("mkv", "video/x-matroska"),
    ("webm", "video/webm"),
    ("mov", "video/quicktime"),
    ("avi", "video/x-msvideo"),
    ("mp3", "audio/mpeg"),
    ("flac", "audio/flac"),
    ("ogg", "audio/ogg"),
    ("opus", "audio/ogg"),
    ("wav", "audio/x-wav"),
    ("m4a", "audio/mp4"),
    ("aac", "audio/aac"),
    ("pdf", "application/pdf"),
    ("txt", "text/plain"),
    ("md", "text/markdown"),
    ("log", "text/plain"),
    ("csv", "text/csv"),
    ("json", "application/json"),
    ("toml", "application/toml"),
    ("yaml", "application/x-yaml"),
    ("yml", "application/x-yaml"),
    ("xml", "application/xml"),
    ("html", "text/html"),
    ("htm", "text/html"),
    ("css", "text/css"),
    ("js", "text/javascript"),
    ("ts", "text/x-typescript"),
    ("rs", "text/x-rust"),
    ("go", "text/x-go"),
    ("py", "text/x-python"),
    ("c", "text/x-csrc"),
    ("h", "text/x-chdr"),
    ("cpp", "text/x-c++src"),
    ("java", "text/x-java"),
    ("sh", "application/x-shellscript"),
    ("glsl", "text/plain"),
    ("shader", "text/plain"),
    ("conf", "text/plain"),
    ("ini", "text/plain"),
    ("zip", "application/zip"),
    ("tar", "application/x-tar"),
    ("gz", "application/gzip"),
    ("xz", "application/x-xz"),
    ("zst", "application/zstd"),
    ("7z", "application/x-7z-compressed"),
    ("deb", "application/vnd.debian.binary-package"),
    ("rpm", "application/x-rpm"),
    ("odt", "application/vnd.oasis.opendocument.text"),
    ("docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
    ("xlsx", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"),
    ("iso", "application/x-cd-image"),
];

pub const FOLDER: &str = "inode/directory";
pub const UNKNOWN: &str = "application/octet-stream";

pub fn mime_of(name: &str, folder: bool) -> &'static str {
    if folder {
        return FOLDER;
    }
    let Some((_, ext)) = name.rsplit_once('.') else {
        return UNKNOWN;
    };
    let ext = ext.to_ascii_lowercase();
    KINDS.iter().find(|(each, _)| *each == ext).map_or(UNKNOWN, |(_, mime)| mime)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Folder,
    Image,
    Video,
    Audio,
    Text,
    Pdf,
    Archive,
    Document,
    Program,
    Other,
}

pub fn family(mime: &str, runs: bool) -> Family {
    match mime {
        FOLDER => Family::Folder,
        "application/pdf" => Family::Pdf,
        "application/json" | "application/toml" | "application/x-yaml" | "application/xml" | "application/x-shellscript" => Family::Text,
        "application/zip" | "application/x-tar" | "application/gzip" | "application/x-xz" | "application/zstd" | "application/x-7z-compressed"
        | "application/vnd.debian.binary-package" | "application/x-rpm" | "application/x-cd-image" => Family::Archive,
        _ if mime.starts_with("image/") => Family::Image,
        _ if mime.starts_with("video/") => Family::Video,
        _ if mime.starts_with("audio/") => Family::Audio,
        _ if mime.starts_with("text/") => Family::Text,
        _ if mime.contains("opendocument") || mime.contains("officedocument") => Family::Document,
        _ if runs => Family::Program,
        _ => Family::Other,
    }
}

impl Family {
    pub fn icon(self) -> &'static str {
        match self {
            Family::Folder => "folder",
            Family::Image => "image-x-generic",
            Family::Video => "video-x-generic",
            Family::Audio => "audio-x-generic",
            Family::Text => "text-x-generic",
            Family::Pdf => "application-pdf",
            Family::Archive => "package-x-generic",
            Family::Document => "x-office-document",
            Family::Program => "application-x-executable",
            Family::Other => "text-x-generic",
        }
    }

    pub fn words(self) -> &'static str {
        match self {
            Family::Folder => crate::lang::t("Folder"),
            Family::Image => crate::lang::t("Image"),
            Family::Video => crate::lang::t("Video"),
            Family::Audio => crate::lang::t("Audio"),
            Family::Text => crate::lang::t("Text"),
            Family::Pdf => crate::lang::t("PDF document"),
            Family::Archive => crate::lang::t("Archive"),
            Family::Document => crate::lang::t("Document"),
            Family::Program => crate::lang::t("Program"),
            Family::Other => crate::lang::t("File"),
        }
    }

    pub fn tint(self) -> [u8; 3] {
        match self {
            Family::Folder => [86, 156, 238],
            Family::Image => [52, 178, 120],
            Family::Video => [226, 86, 98],
            Family::Audio => [176, 110, 230],
            Family::Text => [140, 150, 168],
            Family::Pdf => [222, 72, 62],
            Family::Archive => [210, 150, 60],
            Family::Document => [64, 128, 238],
            Family::Program => [96, 108, 130],
            Family::Other => [140, 150, 168],
        }
    }

    pub fn shows_picture(self) -> bool {
        matches!(self, Family::Image | Family::Video | Family::Pdf)
    }
}

impl Entry {
    pub fn family(&self) -> Family {
        family(self.mime, self.runs)
    }

    pub fn hidden(&self) -> bool {
        self.name.starts_with('.') || self.name.ends_with('~')
    }

    pub fn kind_words(&self) -> String {
        let family = self.family();
        match (family, self.name.rsplit_once('.')) {
            (Family::Folder | Family::Program, _) => family.words().to_string(),
            (_, Some((stem, ext))) if !stem.is_empty() && ext.len() <= 5 && !family.words().to_lowercase().contains(&ext.to_lowercase()) => format!("{} {}", ext.to_ascii_uppercase(), family.words().to_lowercase()),
            _ => family.words().to_string(),
        }
    }

    pub fn at(path: &Path) -> Option<Entry> {
        let name = path.file_name()?.to_string_lossy().into_owned();
        let link = std::fs::symlink_metadata(path).ok()?;
        let meta = std::fs::metadata(path).unwrap_or_else(|_| link.clone());
        let folder = meta.is_dir();
        let runs = !folder && {
            use std::os::unix::fs::PermissionsExt;
            meta.permissions().mode() & 0o111 != 0
        };
        Some(Entry {
            mime: mime_of(&name, folder),
            name,
            path: path.to_path_buf(),
            folder,
            link: link.file_type().is_symlink(),
            size: if folder { 0 } else { meta.len() },
            modified: meta.modified().ok(),
            runs,
            trashed: None,
        })
    }
}

pub fn read(dir: &Path, hidden: bool) -> std::io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    for item in std::fs::read_dir(dir)? {
        let Ok(item) = item else {
            continue;
        };
        if let Some(entry) = Entry::at(&item.path())
            && (hidden || !entry.hidden())
        {
            out.push(entry);
        }
    }
    Ok(out)
}

pub fn folders_in(dir: &Path, hidden: bool) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|item| item.path().is_dir())
        .filter(|item| hidden || !item.file_name().to_string_lossy().starts_with('.'))
        .map(|item| item.path())
        .collect();
    out.sort_by(|a, b| natural(&name_of(a), &name_of(b)));
    out
}

pub fn short_path(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

pub fn name_of(path: &Path) -> String {
    path.file_name().map_or_else(|| path.to_string_lossy().into_owned(), |name| name.to_string_lossy().into_owned())
}

pub fn natural(a: &str, b: &str) -> Ordering {
    let (mut a, mut b) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let take = |chars: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut digits = String::new();
                    while let Some(c) = chars.peek().copied().filter(char::is_ascii_digit) {
                        digits.push(c);
                        chars.next();
                    }
                    digits
                };
                let (x, y) = (take(&mut a), take(&mut b));
                let (tx, ty) = (x.trim_start_matches('0'), y.trim_start_matches('0'));
                let order = tx.len().cmp(&ty.len()).then_with(|| tx.cmp(ty));
                if order != Ordering::Equal {
                    return order;
                }
            }
            (Some(x), Some(y)) => {
                let order = x.to_lowercase().cmp(y.to_lowercase());
                if order != Ordering::Equal {
                    return order;
                }
                a.next();
                b.next();
            }
        }
    }
}

pub fn sort(entries: &mut [Entry], by: Sort, down: bool, folders_first: bool) {
    entries.sort_by(|a, b| {
        let grouped = match folders_first {
            true => b.folder.cmp(&a.folder),
            false => Ordering::Equal,
        };
        let order = match by {
            Sort::Name => natural(&a.name, &b.name),
            Sort::Modified => a.modified.cmp(&b.modified).reverse(),
            Sort::Size => a.size.cmp(&b.size).reverse(),
            Sort::Kind => a.kind_words().cmp(&b.kind_words()),
            Sort::Origin => a.trashed.as_ref().map(|(from, _)| from).cmp(&b.trashed.as_ref().map(|(from, _)| from)),
            Sort::Deleted => a.trashed.as_ref().and_then(|(_, when)| *when).cmp(&b.trashed.as_ref().and_then(|(_, when)| *when)).reverse(),
        }
        .then_with(|| natural(&a.name, &b.name));
        grouped.then(if down { order.reverse() } else { order })
    });
}

pub fn items(count: usize) -> String {
    match count {
        1 => crate::lang::t("1 item").to_string(),
        _ => crate::lang::tf("{} items", &[&count]),
    }
}

pub fn size_words(bytes: u64) -> String {
    match bytes {
        0 => crate::lang::t("Empty").to_string(),
        1 => crate::lang::t("1 byte").to_string(),
        2..1000 => crate::lang::tf("{} bytes", &[&bytes]),
        _ => crate::disks::size_words(bytes),
    }
}

pub fn date_words(when: Option<SystemTime>, now: SystemTime, full: bool) -> String {
    use chrono::{DateTime, Local};
    let Some(when) = when else {
        return "--".to_string();
    };
    let when: DateTime<Local> = when.into();
    let now: DateTime<Local> = now.into();
    if full {
        return when.format("%Y-%m-%d %H:%M").to_string();
    }
    let days = (now.date_naive() - when.date_naive()).num_days();
    match days {
        0 => crate::lang::tf("Today {}", &[&when.format("%H:%M")]),
        1 => crate::lang::tf("Yesterday {}", &[&when.format("%H:%M")]),
        2..7 => when.format("%A %H:%M").to_string(),
        _ if when.format("%Y").to_string() == now.format("%Y").to_string() => when.format("%b %-d").to_string(),
        _ => when.format("%b %-d, %Y").to_string(),
    }
}

pub fn free_space(dir: &Path) -> Option<(u64, u64)> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    let block = stat.f_frsize as u64;
    Some((stat.f_bavail as u64 * block, stat.f_blocks as u64 * block))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, folder: bool, size: u64) -> Entry {
        Entry {
            name: name.to_string(),
            path: PathBuf::from("/x").join(name),
            folder,
            link: false,
            size,
            modified: None,
            mime: mime_of(name, folder),
            runs: false,
            trashed: None,
        }
    }

    #[test]
    fn trash_columns_sort_by_place_and_date() {
        let at = |secs| Some(SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(secs));
        let mut list = vec![entry("a", false, 1), entry("b", false, 1), entry("c", false, 1)];
        list[0].trashed = Some((PathBuf::from("/z/a"), at(10)));
        list[1].trashed = Some((PathBuf::from("/m/b"), at(30)));
        list[2].trashed = Some((PathBuf::from("/a/c"), at(20)));
        sort(&mut list, Sort::Deleted, false, false);
        assert_eq!(list.iter().map(|each| each.name.as_str()).collect::<Vec<_>>(), ["b", "c", "a"]);
        sort(&mut list, Sort::Origin, false, false);
        assert_eq!(list.iter().map(|each| each.name.as_str()).collect::<Vec<_>>(), ["c", "b", "a"]);
        assert_eq!(short_path(Path::new("/home/me/Music"), Path::new("/home/me")), "~/Music");
        assert_eq!(short_path(Path::new("/media/stick"), Path::new("/home/me")), "/media/stick");
    }

    #[test]
    fn names_sort_like_people_count() {
        let mut names = vec!["file10", "File2", "file1", "file02b"];
        names.sort_by(|a, b| natural(a, b));
        assert_eq!(names, ["file1", "File2", "file02b", "file10"]);
    }

    #[test]
    fn folders_come_first_when_asked() {
        let mut all = vec![entry("b.txt", false, 5), entry("a", true, 0), entry("c.png", false, 50)];
        sort(&mut all, Sort::Name, false, true);
        assert_eq!(all.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["a", "b.txt", "c.png"]);
        sort(&mut all, Sort::Size, false, false);
        assert_eq!(all[0].name, "c.png");
        sort(&mut all, Sort::Name, true, true);
        assert_eq!(all.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["a", "c.png", "b.txt"]);
    }

    #[test]
    fn kinds_come_from_the_name() {
        assert_eq!(mime_of("Photo.JPG", false), "image/jpeg");
        assert_eq!(family(mime_of("clip.mkv", false), false), Family::Video);
        assert_eq!(family(mime_of("notes", false), true), Family::Program);
        assert_eq!(family(mime_of("x", true), false), Family::Folder);
        assert_eq!(entry("song.flac", false, 1).kind_words(), "FLAC audio");
    }

    #[test]
    fn hidden_files_start_with_a_dot() {
        assert!(entry(".bashrc", false, 1).hidden());
        assert!(entry("draft~", false, 1).hidden());
        assert!(!entry("a.txt", false, 1).hidden());
    }

    #[test]
    fn a_folder_reads_back_its_files() {
        let dir = std::env::temp_dir().join(format!("drifter-entry-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("inner")).unwrap();
        std::fs::write(dir.join("a.txt"), "hello").unwrap();
        std::fs::write(dir.join(".secret"), "x").unwrap();
        let shown = read(&dir, false).unwrap();
        assert_eq!(shown.len(), 2);
        assert_eq!(read(&dir, true).unwrap().len(), 3);
        let file = shown.iter().find(|e| e.name == "a.txt").unwrap();
        assert_eq!(file.size, 5);
        assert_eq!(folders_in(&dir, false), vec![dir.join("inner")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn sizes_read_plainly() {
        assert_eq!(size_words(1), "1 byte");
        assert_eq!(size_words(512), "512 bytes");
        assert_eq!(size_words(4_200_000), "4.2 MB");
    }
}
