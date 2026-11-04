use std::path::{Path, PathBuf};

use serde::Deserialize;

const BUILT: &str = include_str!("../assets/terminal-themes.toml");
pub const DARK: &str = "Drifter Dark";
pub const LIGHT: &str = "Drifter Light";

#[derive(Deserialize)]
struct File {
    #[serde(default)]
    theme: Vec<Raw>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    name: String,
    light: bool,
    background: String,
    foreground: String,
    #[serde(default)]
    cursor: Option<String>,
    #[serde(default)]
    selection: Option<String>,
    palette: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: String,
    pub light: bool,
    pub background: [u8; 3],
    pub foreground: [u8; 3],
    pub cursor: Option<[u8; 3]>,
    pub selection: [u8; 3],
    pub palette: [[u8; 3]; 16],
}

pub fn hex(value: &str) -> Option<[u8; 3]> {
    let v = value.trim().trim_start_matches('#');
    if v.len() != 6 || !v.is_ascii() {
        return None;
    }
    let part = |at: usize| u8::from_str_radix(&v[at..at + 2], 16).ok();
    Some([part(0)?, part(2)?, part(4)?])
}

fn mix(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    [0, 1, 2].map(|i| (f64::from(a[i]) + (f64::from(b[i]) - f64::from(a[i])) * t).round() as u8)
}

impl Raw {
    fn theme(self) -> Option<Theme> {
        let colors: Vec<[u8; 3]> = self.palette.iter().map(|each| hex(each)).collect::<Option<_>>()?;
        let palette: [[u8; 3]; 16] = colors.try_into().ok()?;
        let background = hex(&self.background)?;
        let foreground = hex(&self.foreground)?;
        let cursor = match self.cursor {
            Some(text) => Some(hex(&text)?),
            None => None,
        };
        let selection = match self.selection {
            Some(text) => hex(&text)?,
            None => mix(background, foreground, 0.25),
        };
        Some(Theme { name: self.name, light: self.light, background, foreground, cursor, selection, palette })
    }
}

pub fn parse(text: &str) -> Vec<Theme> {
    toml::from_str::<File>(text)
        .map(|file| file.theme.into_iter().filter_map(Raw::theme).collect())
        .unwrap_or_default()
}

pub fn folder() -> PathBuf {
    crate::config::xdg_dir("XDG_CONFIG_HOME", ".config").join("drifter/terminal-themes")
}

fn own(folder: &Path) -> Vec<Theme> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "toml"))
        .collect();
    paths.sort();
    paths
        .iter()
        .filter_map(|path| std::fs::read_to_string(path).ok())
        .flat_map(|text| parse(&text))
        .collect()
}

pub fn all() -> Vec<Theme> {
    let mut themes = own(&folder());
    for theme in parse(BUILT) {
        if !themes.iter().any(|mine| mine.name == theme.name) {
            themes.push(theme);
        }
    }
    themes
}

pub fn find(name: &str, light: bool) -> Theme {
    let themes = all();
    let fallback = if light { LIGHT } else { DARK };
    themes
        .iter()
        .find(|theme| theme.name == name)
        .or_else(|| themes.iter().find(|theme| theme.name == fallback))
        .cloned()
        .unwrap_or_else(|| parse(BUILT).into_iter().find(|theme| theme.name == fallback).expect("built-in theme"))
}

pub fn full_palette(theme: &Theme) -> [[u8; 3]; 256] {
    let mut out = [[0u8; 3]; 256];
    out[..16].copy_from_slice(&theme.palette);
    let level = |n: usize| if n == 0 { 0 } else { (55 + n * 40) as u8 };
    for at in 16..232 {
        let n = at - 16;
        out[at] = [level(n / 36), level((n / 6) % 6), level(n % 6)];
    }
    for at in 232..256 {
        let gray = (8 + (at - 232) * 10) as u8;
        out[at] = [gray; 3];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_theme_reads() {
        let themes = parse(BUILT);
        let count = BUILT.matches("[[theme]]").count();
        assert_eq!(themes.len(), count);
        assert!(themes.iter().filter(|theme| theme.light).count() >= 15);
        assert!(themes.iter().filter(|theme| !theme.light).count() >= 15);
        for name in [DARK, LIGHT, "Solarized Dark", "Solarized Light", "Catppuccin Latte", "Nord", "Tango Dark", "rxvt Light", "Detroit Future", "Detroit Future Light"] {
            assert!(themes.iter().any(|theme| theme.name == name), "{name}");
        }
    }

    #[test]
    fn a_missing_theme_falls_back_to_drifter() {
        assert_eq!(find("No Such Theme", true).name, LIGHT);
        assert_eq!(find("No Such Theme", false).name, DARK);
    }

    #[test]
    fn the_cube_and_grays_follow_xterm() {
        let palette = full_palette(&find(DARK, false));
        assert_eq!(palette[16], [0, 0, 0]);
        assert_eq!(palette[21], [0, 0, 255]);
        assert_eq!(palette[231], [255, 255, 255]);
        assert_eq!(palette[232], [8, 8, 8]);
        assert_eq!(palette[255], [238, 238, 238]);
    }
}
