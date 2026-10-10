use std::path::PathBuf;

use super::ops::{decode, encode};

pub const FILES: &str = "x-special/gnome-copied-files";
pub const URIS: &str = "text/uri-list";
pub const TEXT: &str = "text/plain;charset=utf-8";
pub const PLAIN: &str = "text/plain";
pub const UTF8: &str = "UTF8_STRING";

pub const FILE_KINDS: [&str; 5] = [FILES, URIS, TEXT, PLAIN, UTF8];
pub const TEXT_KINDS: [&str; 3] = [TEXT, PLAIN, UTF8];

pub const MOST: usize = 4 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Held {
    Files(Vec<PathBuf>, bool),
    Text(String),
}

impl Held {
    pub fn kinds(&self) -> &'static [&'static str] {
        match self {
            Held::Files(..) => &FILE_KINDS,
            Held::Text(_) => &TEXT_KINDS,
        }
    }

    pub fn text(&self) -> String {
        match self {
            Held::Files(paths, _) => paths.iter().map(|path| path.to_string_lossy()).collect::<Vec<_>>().join("\n"),
            Held::Text(text) => text.clone(),
        }
    }

    pub fn bytes(&self, kind: &str) -> Option<Vec<u8>> {
        match (self, kind) {
            (Held::Files(paths, cut), FILES) => {
                let mut out = String::from(if *cut { "cut" } else { "copy" });
                for path in paths {
                    out.push_str("\nfile://");
                    out.push_str(&encode(path));
                }
                Some(out.into_bytes())
            }
            (Held::Files(paths, _), URIS) => Some(paths.iter().map(|path| format!("file://{}\r\n", encode(path))).collect::<String>().into_bytes()),
            (_, TEXT | PLAIN | UTF8) => Some(self.text().into_bytes()),
            _ => None,
        }
    }
}

pub fn best(offered: &[String]) -> Option<&'static str> {
    FILE_KINDS.into_iter().find(|kind| offered.iter().any(|each| each == kind))
}

pub fn read(kind: &str, bytes: &[u8]) -> Option<Held> {
    let text = String::from_utf8_lossy(bytes);
    match kind {
        FILES => {
            let mut lines = text.lines();
            let cut = lines.next()?.trim() == "cut";
            let paths: Vec<PathBuf> = lines.filter_map(file_path).collect();
            (!paths.is_empty()).then_some(Held::Files(paths, cut))
        }
        URIS => {
            let paths: Vec<PathBuf> = text.lines().filter(|line| !line.starts_with('#')).filter_map(file_path).collect();
            (!paths.is_empty()).then_some(Held::Files(paths, false))
        }
        _ => {
            let text = text.trim_end_matches('\0').to_string();
            (!text.is_empty()).then_some(Held::Text(text))
        }
    }
}

fn file_path(line: &str) -> Option<PathBuf> {
    let rest = line.trim().strip_prefix("file://")?;
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    rest.starts_with('/').then(|| PathBuf::from(decode(rest)))
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_files_go_out_in_every_kind_and_come_back() {
        let held = Held::Files(vec![PathBuf::from("/home/me/My Notes.txt"), PathBuf::from("/home/me/ü.png")], true);
        let gnome = held.bytes(FILES).unwrap();
        assert_eq!(String::from_utf8(gnome.clone()).unwrap(), "cut\nfile:///home/me/My%20Notes.txt\nfile:///home/me/%C3%BC.png");
        assert_eq!(read(FILES, &gnome), Some(held.clone()));
        let uris = held.bytes(URIS).unwrap();
        assert_eq!(read(URIS, &uris), Some(Held::Files(vec![PathBuf::from("/home/me/My Notes.txt"), PathBuf::from("/home/me/ü.png")], false)));
        assert_eq!(held.bytes(TEXT).unwrap(), b"/home/me/My Notes.txt\n/home/me/\xc3\xbc.png");
        assert_eq!(held.bytes("image/png"), None);
    }

    #[test]
    fn other_apps_lists_are_read_with_care() {
        let list = b"# from a browser\r\nfile://localhost/tmp/a%20b\r\nhttps://example.com/x\r\nfile:///tmp/c\r\n";
        assert_eq!(read(URIS, list), Some(Held::Files(vec![PathBuf::from("/tmp/a b"), PathBuf::from("/tmp/c")], false)));
        assert_eq!(read(URIS, b"https://example.com/only"), None);
        assert_eq!(read(FILES, b"copy\n"), None);
        assert_eq!(read(TEXT, b"hello\0"), Some(Held::Text("hello".to_string())));
        assert_eq!(read(TEXT, b""), None);
    }

    #[test]
    fn files_are_preferred_over_text() {
        let offered = vec![TEXT.to_string(), URIS.to_string(), "image/png".to_string()];
        assert_eq!(best(&offered), Some(URIS));
        assert_eq!(best(&[PLAIN.to_string()]), Some(PLAIN));
        assert_eq!(best(&["image/png".to_string()]), None);
    }
}
