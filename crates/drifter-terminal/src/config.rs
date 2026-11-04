use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

impl Edge {
    fn parse(s: &str) -> Edge {
        match s.trim().to_ascii_lowercase().as_str() {
            "bottom" => Edge::Bottom,
            "left" => Edge::Left,
            "right" => Edge::Right,
            _ => Edge::Top,
        }
    }

    pub fn vertical(self) -> bool {
        matches!(self, Edge::Left | Edge::Right)
    }
}

#[derive(Clone)]
pub struct Config {
    pub font_path: Option<PathBuf>,
    pub font_px: f64,
    pub pad: f64,
    pub scrollback: u32,
    pub tab_edge: Edge,
    pub tab_always: bool,
    pub opacity: f64,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
    pub cursor: [u8; 3],
    pub selection: [u8; 3],
    pub cursor_block: bool,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            font_path: None,
            font_px: 15.0,
            pad: 8.0,
            scrollback: 10_000,
            tab_edge: Edge::Top,
            tab_always: false,
            opacity: 1.0,
            fg: [0xd4, 0xd4, 0xd4],
            bg: [0x10, 0x12, 0x16],
            cursor: [0x0a, 0x7a, 0xff],
            selection: [0x34, 0x45, 0x5a],
            cursor_block: true,
        }
    }
}

fn hex(value: &str) -> Option<[u8; 3]> {
    let v = value.trim().trim_start_matches('#');
    if v.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&v[0..2], 16).ok()?;
    let g = u8::from_str_radix(&v[2..4], 16).ok()?;
    let b = u8::from_str_radix(&v[4..6], 16).ok()?;
    Some([r, g, b])
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join(".config/drifter/terminal.conf"))
    }

    pub fn load() -> Config {
        let mut cfg = Config::default();
        let Some(path) = Config::path() else {
            return cfg;
        };
        let Ok(text) = std::fs::read_to_string(path) else {
            return cfg;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            match key {
                "font" => cfg.font_path = Some(PathBuf::from(value)),
                "font_size" => {
                    if let Ok(v) = value.parse() {
                        cfg.font_px = v;
                    }
                }
                "padding" => {
                    if let Ok(v) = value.parse() {
                        cfg.pad = v;
                    }
                }
                "scrollback" => {
                    if let Ok(v) = value.parse() {
                        cfg.scrollback = v;
                    }
                }
                "tab_bar" => cfg.tab_edge = Edge::parse(value),
                "tab_always_show" => cfg.tab_always = value == "true",
                "opacity" => {
                    if let Ok(v) = value.parse::<f64>() {
                        cfg.opacity = v.clamp(0.2, 1.0);
                    }
                }
                "foreground" => {
                    if let Some(c) = hex(value) {
                        cfg.fg = c;
                    }
                }
                "background" => {
                    if let Some(c) = hex(value) {
                        cfg.bg = c;
                    }
                }
                "cursor_color" => {
                    if let Some(c) = hex(value) {
                        cfg.cursor = c;
                    }
                }
                "selection" => {
                    if let Some(c) = hex(value) {
                        cfg.selection = c;
                    }
                }
                "cursor_style" => cfg.cursor_block = value != "beam" && value != "bar",
                _ => {}
            }
        }
        cfg
    }
}
