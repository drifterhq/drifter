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
    pub dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TrayConfig {
    #[serde(default = "default_tray_format")]
    pub format: String,
}

fn default_tray_format() -> String {
    crate::tray::DEFAULT_FORMAT.to_string()
}

impl Default for TrayConfig {
    fn default() -> Self {
        TrayConfig {
            format: default_tray_format(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Input {
    #[serde(default)]
    pub modifier: Modifier,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub workspace: Workspace,
    #[serde(default)]
    pub input: Input,
    #[serde(default)]
    pub tray: TrayConfig,
    #[serde(default)]
    pub assets: Assets,
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
            hot_corner_size: 4.0,
            edge_pan_speed: 1400.0,
            camera_anim: Duration::from_millis(260),
        }
    }
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
        xdg_dir("XDG_CONFIG_HOME", ".config").join("drifter.toml")
    }

    pub fn data_dir() -> PathBuf {
        xdg_dir("XDG_DATA_HOME", ".local/share")
    }

    pub fn assets_dir(&self) -> PathBuf {
        self.assets.dir.clone().unwrap_or_else(Config::data_dir)
    }

    pub fn cursor_path(&self) -> PathBuf {
        self.assets_dir().join("cursor.png")
    }

    pub fn font_path(&self) -> PathBuf {
        self.assets_dir().join("font.ttf")
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
        let config = Config::parse(&text).map_err(|message| ConfigError::Parse {
            path: path.to_path_buf(),
            message,
        })?;
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
