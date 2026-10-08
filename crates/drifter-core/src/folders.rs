use std::path::{Path, PathBuf};

pub const SEARCHED: [(&str, &str); 6] = [
    ("DESKTOP", "Desktop"),
    ("DOCUMENTS", "Documents"),
    ("DOWNLOAD", "Downloads"),
    ("PICTURES", "Pictures"),
    ("MUSIC", "Music"),
    ("VIDEOS", "Videos"),
];

pub fn read(text: &str, key: &str, home: &Path) -> Option<PathBuf> {
    let wanted = format!("XDG_{key}_DIR");
    text.lines().find_map(|line| {
        let (name, value) = line.trim().split_once('=')?;
        if name.trim() != wanted {
            return None;
        }
        let value = value.trim().trim_matches('"');
        let path = match value.strip_prefix("$HOME") {
            Some(rest) => home.join(rest.trim_start_matches('/')),
            None if value.starts_with('/') => PathBuf::from(value),
            None => return None,
        };
        (path != home).then_some(path)
    })
}

fn listed(home: &Path) -> String {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    std::fs::read_to_string(config.join("user-dirs.dirs")).unwrap_or_default()
}

pub fn user_dir(home: &Path, key: &str, fallback: &str) -> PathBuf {
    read(&listed(home), key, home).unwrap_or_else(|| home.join(fallback))
}

pub fn searched(home: &Path) -> Vec<PathBuf> {
    let text = listed(home);
    let mut out: Vec<PathBuf> = Vec::new();
    for (key, fallback) in SEARCHED {
        let dir = read(&text, key, home).unwrap_or_else(|| home.join(fallback));
        if !out.contains(&dir) {
            out.push(dir);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const GERMAN: &str = "# written by xdg-user-dirs-update\nXDG_DESKTOP_DIR=\"$HOME/Schreibtisch\"\nXDG_PICTURES_DIR=\"$HOME/Bilder\"\nXDG_MUSIC_DIR=\"$HOME\"\nXDG_VIDEOS_DIR=\"/data/Videos\"\n";

    #[test]
    fn user_dirs_follow_the_language_the_system_named_them_in() {
        let home = Path::new("/home/anna");
        assert_eq!(read(GERMAN, "PICTURES", home), Some(PathBuf::from("/home/anna/Bilder")));
        assert_eq!(read(GERMAN, "DESKTOP", home), Some(PathBuf::from("/home/anna/Schreibtisch")));
        assert_eq!(read(GERMAN, "VIDEOS", home), Some(PathBuf::from("/data/Videos")));
    }

    #[test]
    fn a_dir_set_to_home_or_missing_is_not_used() {
        let home = Path::new("/home/anna");
        assert_eq!(read(GERMAN, "MUSIC", home), None);
        assert_eq!(read(GERMAN, "DOWNLOAD", home), None);
    }
}
