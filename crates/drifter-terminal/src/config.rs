use std::path::PathBuf;
use std::time::SystemTime;

use drifter_core::config::{Style, TermColors, TermCursor, TermTabs};
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
    pub font_path: Option<PathBuf>,
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
}

const DARK_INK: ([u8; 3], [u8; 3]) = ([0xd4, 0xd4, 0xd4], [0x10, 0x12, 0x16]);
const LIGHT_INK: ([u8; 3], [u8; 3]) = ([0x1f, 0x23, 0x28], [0xfa, 0xfa, 0xfb]);
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
        let (fg, bg) = match style {
            Style::Light => LIGHT_INK,
            _ => DARK_INK,
        };
        let accent = whole.appearance.accent.map(|accent| accent.color()).unwrap_or(ACCENT);
        Config {
            font_path: t.font.as_ref().map(PathBuf::from),
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
            fg,
            bg,
            cursor: accent,
            cursor_shape: match t.cursor {
                TermCursor::Block => Cursor::Block,
                TermCursor::Bar => Cursor::Bar,
                TermCursor::Underline => Cursor::Underline,
            },
            accent,
            palette: drifter_core::theme::of(style),
        }
    }
}
