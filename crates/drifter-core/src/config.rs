use std::fmt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;

use crate::geom::Vec2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WallpaperMode {
    Fixed,
    #[default]
    Pan,
    Parallax,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Modifier {
    #[default]
    Super,
    Alt,
    Ctrl,
}

impl Modifier {
    pub fn keys(self) -> [u32; 2] {
        match self {
            Modifier::Super => [133, 134],
            Modifier::Alt => [64, 108],
            Modifier::Ctrl => [37, 105],
        }
    }

    pub fn holds(self, key: u32) -> bool {
        self.keys().contains(&key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workspace {
    #[serde(default = "default_screens")]
    pub width: u32,
    #[serde(default = "default_screens")]
    pub height: u32,
    #[serde(default)]
    pub wallpaper: Option<PathBuf>,
    #[serde(default)]
    pub wallpaper_mode: WallpaperMode,
}

fn default_screens() -> u32 {
    3
}

impl Default for Workspace {
    fn default() -> Self {
        Workspace {
            width: default_screens(),
            height: default_screens(),
            wallpaper: None,
            wallpaper_mode: WallpaperMode::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Assets {
    #[serde(default)]
    pub cursor: Option<PathBuf>,
    #[serde(default)]
    pub font: Option<PathBuf>,
    #[serde(default)]
    pub window: Option<PathBuf>,
    #[serde(default)]
    pub dock: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct DockConfig {
    #[serde(default)]
    pub pinned: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drawers {
    #[serde(default = "default_top_height")]
    pub top: f64,
    #[serde(default = "default_bottom_height")]
    pub bottom: f64,
}

fn default_top_height() -> f64 {
    36.0
}

fn default_bottom_height() -> f64 {
    64.0
}

impl Default for Drawers {
    fn default() -> Self {
        Drawers {
            top: default_top_height(),
            bottom: default_bottom_height(),
        }
    }
}

impl Drawers {
    pub fn height(&self, edge: crate::drawer::Edge) -> f64 {
        match edge {
            crate::drawer::Edge::Top => self.top,
            crate::drawer::Edge::Bottom => self.bottom,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrayConfig {
    #[serde(default = "default_tray_format")]
    pub format: String,
    #[serde(default = "default_tray_size")]
    pub size: f64,
}

fn default_tray_size() -> f64 {
    13.0
}

fn default_tray_format() -> String {
    crate::tray::DEFAULT_FORMAT.to_string()
}

impl Default for TrayConfig {
    fn default() -> Self {
        TrayConfig {
            format: default_tray_format(),
            size: default_tray_size(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Input {
    #[serde(default)]
    pub modifier: Modifier,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub workspace: Workspace,
    #[serde(default)]
    pub input: Input,
    #[serde(default)]
    pub drawers: Drawers,
    #[serde(default)]
    pub tray: TrayConfig,
    #[serde(default)]
    pub dock: DockConfig,
    #[serde(default)]
    pub assets: Assets,
    #[serde(skip)]
    base: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tunables {
    pub drawer_height: f64,
    pub drawer_dwell: Duration,
    pub drawer_hide_delay: Duration,
    pub drawer_anim: Duration,
    pub edge_band: f64,
    pub hot_corner_dwell: Duration,
    pub hot_corner_size: f64,
    pub edge_pan_speed: f64,
    pub camera_anim: Duration,
}

impl Default for Tunables {
    fn default() -> Self {
        Tunables {
            drawer_height: 64.0,
            drawer_dwell: Duration::from_millis(120),
            drawer_hide_delay: Duration::from_millis(400),
            drawer_anim: Duration::from_millis(300),
            edge_band: 2.0,
            hot_corner_dwell: Duration::from_millis(300),
            hot_corner_size: 48.0,
            edge_pan_speed: 1400.0,
            camera_anim: Duration::from_millis(260),
        }
    }
}

pub fn choose_config(
    candidates: Vec<PathBuf>,
    xdg: PathBuf,
    exists: impl Fn(&Path) -> bool,
) -> PathBuf {
    candidates
        .into_iter()
        .find(|path| exists(path))
        .unwrap_or(xdg)
}

pub fn repo_config(exe: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut at = exe.parent();
    for _ in 0..4 {
        let Some(dir) = at else {
            break;
        };
        candidates.push(dir.join("conf").join("drifter.toml"));
        at = dir.parent();
    }
    candidates
}

fn xdg_dir(var: &str, fallback: &str) -> PathBuf {
    resolve_xdg(std::env::var_os(var), std::env::var_os("HOME"), fallback)
}

fn resolve_xdg(
    xdg: Option<std::ffi::OsString>,
    home: Option<std::ffi::OsString>,
    fallback: &str,
) -> PathBuf {
    let base = xdg
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| home.map(|h| PathBuf::from(h).join(fallback)))
        .unwrap_or_else(|| PathBuf::from(fallback));
    base.join("drifter")
}

#[derive(Debug)]
pub enum ConfigError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        message: String,
    },
    Invalid(String),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::Read { path, source } => {
                write!(f, "cannot read {}: {source}", path.display())
            }
            ConfigError::Parse { path, message } => {
                write!(f, "cannot parse {}: {message}", path.display())
            }
            ConfigError::Invalid(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl Config {
    pub fn config_path() -> PathBuf {
        let running = std::env::current_exe().ok();
        let candidates = running.as_deref().map(repo_config).unwrap_or_default();
        let xdg = xdg_dir("XDG_CONFIG_HOME", ".config").join("drifter.toml");
        choose_config(candidates, xdg, |path| path.is_file())
    }

    pub fn data_dir() -> PathBuf {
        xdg_dir("XDG_DATA_HOME", ".local/share")
    }

    pub fn beside(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            return path.to_path_buf();
        }
        self.base.join(path)
    }

    pub fn wallpaper_path(&self) -> Option<PathBuf> {
        self.workspace
            .wallpaper
            .as_deref()
            .map(|path| self.beside(path))
    }

    pub fn camera_path() -> PathBuf {
        xdg_dir("XDG_STATE_HOME", ".local/state").join("camera")
    }

    pub fn cursor_path(&self) -> Option<PathBuf> {
        self.assets.cursor.as_deref().map(|path| self.beside(path))
    }

    pub fn font_path(&self) -> Option<PathBuf> {
        self.assets.font.as_deref().map(|path| self.beside(path))
    }

    pub fn window_dir(&self) -> Option<PathBuf> {
        self.assets.window.as_deref().map(|path| self.beside(path))
    }

    pub fn dock_dir(&self) -> Option<PathBuf> {
        self.assets.dock.as_deref().map(|path| self.beside(path))
    }

    pub fn load() -> Result<Config, ConfigError> {
        Config::load_from(&Config::config_path())
    }

    pub fn load_from(path: &Path) -> Result<Config, ConfigError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Config::default());
            }
            Err(source) => {
                return Err(ConfigError::Read {
                    path: path.to_path_buf(),
                    source,
                });
            }
        };
        let mut config = Config::parse(&text).map_err(|message| ConfigError::Parse {
            path: path.to_path_buf(),
            message,
        })?;
        config.base = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        config.validate()?;
        Ok(config)
    }

    pub fn parse(text: &str) -> Result<Config, String> {
        toml::from_str(text).map_err(|e| e.message().to_string())
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.workspace.width == 0 || self.workspace.height == 0 {
            return Err(ConfigError::Invalid(
                "workspace width and height are counted in screens and must be at least 1".into(),
            ));
        }
        if self.drawers.top < 16.0 || self.drawers.bottom < 16.0 {
            return Err(ConfigError::Invalid(
                "drawer heights are in pixels and must be at least 16".into(),
            ));
        }
        if self.tray.size < 6.0 {
            return Err(ConfigError::Invalid(
                "tray size is text height in pixels and must be at least 6".into(),
            ));
        }
        Ok(())
    }

    pub fn world_size(&self, screen: Vec2) -> Vec2 {
        Vec2::new(
            screen.x * f64::from(self.workspace.width),
            screen.y * f64::from(self.workspace.height),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_config_is_looked_for_beside_the_binary_first() {
        let exe = Path::new("/home/dh/src/drifter/target/debug/drifter");
        let candidates = repo_config(exe);
        assert!(candidates.contains(&PathBuf::from("/home/dh/src/drifter/conf/drifter.toml")));
    }

    #[test]
    fn the_repo_config_wins_over_the_home_one() {
        let repo = PathBuf::from("/home/dh/src/drifter/conf/drifter.toml");
        let home = PathBuf::from("/home/dh/.config/drifter/drifter.toml");
        let chosen = choose_config(vec![repo.clone()], home, |path| path == repo);
        assert_eq!(chosen, repo);
    }

    #[test]
    fn the_home_config_is_used_when_there_is_no_repo_beside_the_binary() {
        let repo = PathBuf::from("/opt/drifter/conf/drifter.toml");
        let home = PathBuf::from("/home/dh/.config/drifter/drifter.toml");
        let chosen = choose_config(vec![repo], home.clone(), |_| false);
        assert_eq!(chosen, home);
    }

    #[test]
    fn the_repo_search_gives_up_before_it_reaches_the_root() {
        let exe = Path::new("/home/dh/src/drifter/target/debug/drifter");
        assert!(!repo_config(exe).contains(&PathBuf::from("/conf/drifter.toml")));
    }

    #[test]
    fn a_relative_path_is_read_next_to_the_config() {
        let config = Config {
            base: PathBuf::from("/home/dh/src/drifter/conf"),
            ..Config::default()
        };
        assert_eq!(
            config.beside(Path::new("../img/cursor.png")),
            PathBuf::from("/home/dh/src/drifter/conf/../img/cursor.png")
        );
    }

    #[test]
    fn an_absolute_path_is_left_alone() {
        let config = Config {
            base: PathBuf::from("/home/dh/src/drifter/conf"),
            ..Config::default()
        };
        assert_eq!(
            config.beside(Path::new("/elsewhere/art.png")),
            PathBuf::from("/elsewhere/art.png")
        );
    }

    #[test]
    fn the_asset_paths_are_none_until_the_config_names_them() {
        let config = Config::default();
        assert_eq!(config.cursor_path(), None);
        assert_eq!(config.font_path(), None);
        assert_eq!(config.wallpaper_path(), None);
    }

    #[test]
    fn the_config_carries_the_cursor_and_the_font() {
        let config = Config::parse(
            "[assets]\ncursor = \"../img/cursor.png\"\nfont = \"../fonts/Inter-Regular.ttf\"\n",
        )
        .unwrap();
        assert_eq!(
            config.assets.cursor.as_deref(),
            Some(Path::new("../img/cursor.png"))
        );
        assert_eq!(
            config.assets.font.as_deref(),
            Some(Path::new("../fonts/Inter-Regular.ttf"))
        );
    }

    #[test]
    fn the_top_drawer_is_thinner_than_the_bottom_one_by_default() {
        let config = Config::default();
        assert_eq!(config.drawers.top, 36.0);
        assert_eq!(config.drawers.bottom, 64.0);
        assert_eq!(config.drawers.height(crate::drawer::Edge::Top), 36.0);
        assert_eq!(config.drawers.height(crate::drawer::Edge::Bottom), 64.0);
    }

    #[test]
    fn the_drawer_heights_come_from_the_config() {
        let config = Config::parse("[drawers]\ntop = 28\nbottom = 80\n").unwrap();
        assert_eq!(config.drawers.top, 28.0);
        assert_eq!(config.drawers.bottom, 80.0);
    }

    #[test]
    fn a_silly_drawer_height_is_refused() {
        let config = Config::parse("[drawers]\ntop = 2\n").unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn the_dock_is_empty_until_the_config_pins_something() {
        assert!(Config::default().dock.pinned.is_empty());
    }

    #[test]
    fn the_dock_keeps_the_order_the_config_gives() {
        let config = Config::parse("[dock]\npinned = [\"foot\", \"copyq\"]\n").unwrap();
        assert_eq!(config.dock.pinned, vec!["foot", "copyq"]);
    }

    #[test]
    fn the_tray_text_size_comes_from_the_config() {
        assert_eq!(Config::default().tray.size, 13.0);
        assert_eq!(
            Config::parse("[tray]\nsize = 20\n").unwrap().tray.size,
            20.0
        );
    }

    #[test]
    fn a_silly_tray_size_is_refused() {
        assert!(
            Config::parse("[tray]\nsize = 1\n")
                .unwrap()
                .validate()
                .is_err()
        );
    }

    #[test]
    fn the_config_in_the_repo_parses() {
        let text = include_str!("../../../conf/drifter.toml");
        let config = Config::parse(text).expect("conf/drifter.toml does not parse");
        config.validate().expect("conf/drifter.toml is not valid");
    }

    #[test]
    fn every_modifier_has_a_left_and_a_right_key() {
        for modifier in [Modifier::Super, Modifier::Alt, Modifier::Ctrl] {
            let keys = modifier.keys();
            assert_ne!(keys[0], keys[1]);
            assert!(modifier.holds(keys[0]) && modifier.holds(keys[1]));
        }
    }

    #[test]
    fn the_modifiers_never_share_a_key() {
        let all = [Modifier::Super, Modifier::Alt, Modifier::Ctrl];
        let mut seen: Vec<u32> = all.iter().flat_map(|m| m.keys()).collect();
        let count = seen.len();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), count);
    }

    #[test]
    fn a_plain_letter_is_never_the_modifier() {
        assert!(!Modifier::Ctrl.holds(38));
        assert!(!Modifier::Super.holds(9));
    }

    #[test]
    fn empty_config_is_all_defaults() {
        assert_eq!(Config::parse("").unwrap(), Config::default());
    }

    #[test]
    fn defaults_are_three_by_three_and_pan() {
        let config = Config::default();
        assert_eq!(config.workspace.width, 3);
        assert_eq!(config.workspace.height, 3);
        assert_eq!(config.workspace.wallpaper_mode, WallpaperMode::Pan);
        assert_eq!(config.input.modifier, Modifier::Super);
    }

    #[test]
    fn parses_the_documented_file() {
        let config = Config::parse(
            r#"
            [workspace]
            width = 4
            height = 6
            wallpaper = "/path/to/huge_wallpaper.png"
            wallpaper_mode = "pan"

            [input]
            modifier = "super"
            "#,
        )
        .unwrap();
        assert_eq!(config.workspace.width, 4);
        assert_eq!(config.workspace.height, 6);
        assert_eq!(
            config.workspace.wallpaper.as_deref(),
            Some(Path::new("/path/to/huge_wallpaper.png"))
        );
        assert_eq!(config.workspace.wallpaper_mode, WallpaperMode::Pan);
    }

    #[test]
    fn world_size_is_screens_times_resolution() {
        let config = Config::parse("[workspace]\nwidth = 4\nheight = 6\n").unwrap();
        let world = config.world_size(Vec2::new(3840.0, 2160.0));
        assert_eq!(world, Vec2::new(15360.0, 12960.0));
    }

    #[test]
    fn zero_screens_is_rejected() {
        let config = Config::parse("[workspace]\nwidth = 0\n").unwrap();
        assert!(config.validate().is_err());
    }

    #[test]
    fn unknown_keys_are_rejected() {
        assert!(Config::parse("[workspace]\nwidht = 4\n").is_err());
    }

    #[test]
    fn xdg_wins_when_it_is_absolute() {
        let p = resolve_xdg(Some("/xdg".into()), Some("/home/dh".into()), ".config");
        assert_eq!(p, Path::new("/xdg/drifter"));
    }

    #[test]
    fn a_relative_xdg_value_is_ignored_and_home_is_used() {
        let p = resolve_xdg(Some("relative".into()), Some("/home/dh".into()), ".config");
        assert_eq!(p, Path::new("/home/dh/.config/drifter"));
    }

    #[test]
    fn home_is_used_when_xdg_is_unset() {
        let p = resolve_xdg(None, Some("/home/dh".into()), ".local/share");
        assert_eq!(p, Path::new("/home/dh/.local/share/drifter"));
    }

    #[test]
    fn a_missing_file_is_defaults_not_an_error() {
        let path = Path::new("/nonexistent/drifter/drifter.toml");
        assert_eq!(Config::load_from(path).unwrap(), Config::default());
    }
}
