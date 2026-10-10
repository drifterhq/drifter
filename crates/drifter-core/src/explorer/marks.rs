use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub path: PathBuf,
    pub name: String,
}

pub fn file() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| super::model::home().join(".config"));
    config.join("gtk-3.0/bookmarks")
}

pub fn parse(text: &str) -> Vec<Mark> {
    let mut out: Vec<Mark> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let (uri, name) = match line.split_once(' ') {
            Some((uri, name)) => (uri, Some(name.trim())),
            None => (line, None),
        };
        let Some(rest) = uri.strip_prefix("file://") else {
            continue;
        };
        let path = PathBuf::from(super::ops::decode(rest));
        if out.iter().any(|mark| mark.path == path) {
            continue;
        }
        let name = name.filter(|name| !name.is_empty()).map_or_else(|| super::entry::name_of(&path), str::to_string);
        out.push(Mark { path, name });
    }
    out
}

pub fn render(marks: &[Mark]) -> String {
    marks
        .iter()
        .map(|mark| match mark.name == super::entry::name_of(&mark.path) {
            true => format!("file://{}\n", super::ops::encode(&mark.path)),
            false => format!("file://{} {}\n", super::ops::encode(&mark.path), mark.name),
        })
        .collect()
}

pub fn load_from(path: &Path) -> Vec<Mark> {
    parse(&std::fs::read_to_string(path).unwrap_or_default())
}

pub fn save_to(path: &Path, marks: &[Mark]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let spare = path.with_extension("new");
    std::fs::write(&spare, render(marks))?;
    std::fs::rename(&spare, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gtk_bookmarks_read_and_write_back() {
        let text = "file:///home/me/Projects\nfile:///home/me/My%20Music Tunes\nsftp://server/x\nfile:///home/me/Projects\n";
        let marks = parse(text);
        assert_eq!(marks.len(), 2);
        assert_eq!(marks[0].name, "Projects");
        assert_eq!(marks[1].path, PathBuf::from("/home/me/My Music"));
        assert_eq!(marks[1].name, "Tunes");
        assert_eq!(render(&marks), "file:///home/me/Projects\nfile:///home/me/My%20Music Tunes\n");
    }
}
