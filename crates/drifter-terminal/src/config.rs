#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

impl Edge {
    pub fn from_str(s: &str) -> Edge {
        match s.trim().to_ascii_lowercase().as_str() {
            "bottom" => Edge::Bottom,
            "left" => Edge::Left,
            "right" => Edge::Right,
            _ => Edge::Top,
        }
    }
}

pub struct Config {
    pub font_px: f64,
    pub pad: f64,
    pub scrollback: u32,
    pub tab_edge: Edge,
}

impl Default for Config {
    fn default() -> Config {
        Config { font_px: 15.0, pad: 8.0, scrollback: 10_000, tab_edge: Edge::Top }
    }
}

impl Config {
    /// Reads ~/.config/drifter/terminal.conf if it exists. Lines are `key = value`.
    pub fn load() -> Config {
        let mut cfg = Config::default();
        let Some(home) = std::env::var_os("HOME") else {
            return cfg;
        };
        let path = std::path::Path::new(&home).join(".config/drifter/terminal.conf");
        let Ok(text) = std::fs::read_to_string(path) else {
            return cfg;
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let (key, value) = (key.trim(), value.trim());
                match key {
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
                    "tabs" => cfg.tab_edge = Edge::from_str(value),
                    _ => {}
                }
            }
        }
        cfg
    }
}
