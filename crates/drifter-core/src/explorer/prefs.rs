use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::entry::Sort;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    List,
    Icons,
    Columns,
    Gallery,
}

impl Mode {
    pub const ALL: [Mode; 4] = [Mode::List, Mode::Icons, Mode::Columns, Mode::Gallery];

    pub fn name(self) -> &'static str {
        match self {
            Mode::List => crate::lang::t("List"),
            Mode::Icons => crate::lang::t("Icons"),
            Mode::Columns => crate::lang::t("Columns"),
            Mode::Gallery => crate::lang::t("Gallery"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Start {
    Home,
    Last,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Click {
    Double,
    Single,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Root {
    Home,
    Computer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Dates {
    Friendly,
    Exact,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub start: Start,
    pub last: Option<PathBuf>,
    pub click: Click,
    #[serde(rename = "ask_before_trash")]
    pub confirm_trash: bool,
    pub hidden: bool,
    pub mode: Mode,
    pub sort: Sort,
    pub down: bool,
    pub folders_first: bool,
    pub zoom: u32,
    pub thumbnails: bool,
    pub show_size: bool,
    pub show_kind: bool,
    pub show_modified: bool,
    pub dates: Dates,
    pub tree: bool,
    pub tree_root: Root,
    pub places: bool,
    pub drives: bool,
    pub status: bool,
    pub tree_width: u32,
    pub terminal: Option<String>,
}

pub const LEAST_ZOOM: u32 = 32;
pub const MOST_ZOOM: u32 = 256;
pub const LEAST_TREE: u32 = 150;
pub const MOST_TREE: u32 = 420;

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            start: Start::Home,
            last: None,
            click: Click::Double,
            confirm_trash: true,
            hidden: false,
            mode: Mode::List,
            sort: Sort::Name,
            down: false,
            folders_first: true,
            zoom: 72,
            thumbnails: true,
            show_size: true,
            show_kind: true,
            show_modified: true,
            dates: Dates::Friendly,
            tree: true,
            tree_root: Root::Home,
            places: true,
            drives: true,
            status: true,
            tree_width: 230,
            terminal: None,
        }
    }
}

impl Prefs {
    pub fn path() -> PathBuf {
        crate::Config::config_path().with_file_name("explorer.toml")
    }

    pub fn load_from(path: &Path) -> Prefs {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| toml::from_str::<Prefs>(&text).ok())
            .unwrap_or_default()
            .tidy()
    }

    pub fn save_to(&self, path: &Path) -> Result<(), String> {
        let text = toml::to_string(self).map_err(|error| error.to_string())?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        }
        let spare = path.with_extension("toml.new");
        std::fs::write(&spare, text).map_err(|error| error.to_string())?;
        std::fs::rename(&spare, path).map_err(|error| error.to_string())
    }

    pub fn tidy(mut self) -> Prefs {
        self.zoom = self.zoom.clamp(LEAST_ZOOM, MOST_ZOOM);
        self.tree_width = self.tree_width.clamp(LEAST_TREE, MOST_TREE);
        self
    }
}

pub const TERMINALS: [(&str, &str); 10] = [
    ("ptyxis", "Ptyxis"),
    ("gnome-terminal", "GNOME Terminal"),
    ("kgx", "Console"),
    ("konsole", "Konsole"),
    ("kitty", "kitty"),
    ("alacritty", "Alacritty"),
    ("foot", "foot"),
    ("wezterm", "WezTerm"),
    ("xfce4-terminal", "Xfce Terminal"),
    ("xterm", "XTerm"),
];

pub fn on_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

pub fn terminals() -> Vec<(&'static str, &'static str)> {
    TERMINALS.iter().copied().filter(|(program, _)| on_path(program)).collect()
}

pub fn terminal_argv(program: &str, dir: &Path) -> Vec<String> {
    let dir = dir.to_string_lossy().into_owned();
    match program {
        "gnome-terminal" | "ptyxis" | "kgx" | "xfce4-terminal" => vec![program.to_string(), format!("--working-directory={dir}")],
        "konsole" => vec![program.to_string(), "--workdir".into(), dir],
        "kitty" => vec![program.to_string(), "--directory".into(), dir],
        "alacritty" => vec![program.to_string(), "--working-directory".into(), dir],
        "foot" => vec![program.to_string(), format!("--working-directory={dir}")],
        "wezterm" => vec![program.to_string(), "start".into(), "--cwd".into(), dir],
        _ => vec![program.to_string()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefs_round_trip_and_fill_gaps() {
        let dir = std::env::temp_dir().join(format!("drifter-prefs-{}", std::process::id()));
        let path = dir.join("explorer.toml");
        let mut prefs = Prefs { mode: Mode::Columns, zoom: 128, hidden: true, ..Prefs::default() };
        prefs.last = Some(PathBuf::from("/tmp"));
        prefs.save_to(&path).unwrap();
        assert_eq!(Prefs::load_from(&path), prefs);
        std::fs::write(&path, "mode = \"icons\"\nzoom = 9000\n").unwrap();
        let read = Prefs::load_from(&path);
        assert_eq!(read.mode, Mode::Icons);
        assert_eq!(read.zoom, MOST_ZOOM);
        assert!(read.folders_first);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn moving_to_the_trash_asks_unless_turned_off() {
        assert!(Prefs::default().confirm_trash);
        let old: Prefs = toml::from_str("confirm_trash = false").unwrap();
        assert!(old.confirm_trash);
        let off: Prefs = toml::from_str("ask_before_trash = false").unwrap();
        assert!(!off.confirm_trash);
    }

    #[test]
    fn broken_prefs_fall_back() {
        let dir = std::env::temp_dir().join(format!("drifter-prefs-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("explorer.toml");
        std::fs::write(&path, "mode = 7").unwrap();
        assert_eq!(Prefs::load_from(&path), Prefs::default());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
