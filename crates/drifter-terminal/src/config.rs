use std::path::PathBuf;
use std::time::SystemTime;

use drifter_core::config::{Style, TermColors, TermCursor, TermTabs};
use drifter_core::termtheme::Theme;
use drifter_core::theme::Palette;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

impl Edge {
    pub fn vertical(self) -> bool {
        matches!(self, Edge::Left | Edge::Right)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cursor {
    Block,
    Bar,
    Underline,
}

#[derive(Clone, PartialEq)]
pub struct Config {
    pub font: Option<String>,
    pub font_px: f64,
    pub pad: f64,
    pub tab_edge: Edge,
    pub tab_always: bool,
    pub opacity: f64,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
    pub cursor: [u8; 3],
    pub cursor_shape: Cursor,
    pub accent: [u8; 3],
    pub palette: Palette,
    pub theme: Theme,
}

const ACCENT: [u8; 3] = [0x0a, 0x7a, 0xff];

impl Config {
    pub fn path() -> PathBuf {
        drifter_core::Config::config_path()
    }

    pub fn stamp() -> Option<SystemTime> {
        std::fs::metadata(Config::path()).and_then(|meta| meta.modified()).ok()
    }

    pub fn load() -> Config {
        let whole = drifter_core::Config::load().unwrap_or_else(|error| {
            log!("drifter-terminal: {error}");
            drifter_core::Config::default()
        });
        let t = &whole.terminal;
        let style = match t.colors {
            TermColors::Dark => Style::Dark,
            TermColors::Light => Style::Light,
            TermColors::Auto => drifter_core::theme::now(whole.appearance.style),
        };
        let theme = match style {
            Style::Light => drifter_core::termtheme::find(&t.light_theme, true),
            _ => drifter_core::termtheme::find(&t.dark_theme, false),
        };
        let accent = whole.appearance.accent.map(|accent| accent.color()).unwrap_or(ACCENT);
        Config {
            font: t.font.clone(),
            font_px: f64::from(t.text_size),
            pad: f64::from(t.padding),
            tab_edge: match t.tabs {
                TermTabs::Top => Edge::Top,
                TermTabs::Bottom => Edge::Bottom,
                TermTabs::Left => Edge::Left,
                TermTabs::Right => Edge::Right,
            },
            tab_always: t.always_show_tabs,
            opacity: f64::from(t.opacity) / 100.0,
            fg: theme.foreground,
            bg: theme.background,
            cursor: theme.cursor.unwrap_or(accent),
            cursor_shape: match t.cursor {
                TermCursor::Block => Cursor::Block,
                TermCursor::Bar => Cursor::Bar,
                TermCursor::Underline => Cursor::Underline,
            },
            accent,
            palette: drifter_core::theme::of(style),
            theme,
        }
    }
}
