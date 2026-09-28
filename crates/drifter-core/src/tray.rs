use chrono::format::StrftimeItems;
use chrono::{DateTime, Local, TimeZone};

use crate::geom::{Rect, Vec2};

pub const DEFAULT_FORMAT: &str = "%a %b %d %I:%M:%S %p";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tray {
    pub margin: f64,
    pub size: f64,
    pub ink: [u8; 3],
    pub icon: f64,
    pub gap: f64,
}

impl Default for Tray {
    fn default() -> Self {
        Tray {
            margin: 12.0,
            size: 13.0,
            ink: [236, 238, 244],
            icon: 18.0,
            gap: 14.0,
        }
    }
}

impl Tray {
    pub fn clock_rect(&self, band: Rect, text: Vec2) -> Rect {
        let x = band.max().x - self.margin - text.x;
        Rect::new(Vec2::new(x, self.centered(band, text.y)), text)
    }

    pub fn icon_rect(&self, band: Rect, slot: usize) -> Rect {
        let step = self.icon + self.gap;
        let x = band.origin.x + self.margin + step * slot as f64;
        let size = Vec2::splat(self.icon);
        Rect::new(Vec2::new(x, self.centered(band, size.y)), size)
    }

    pub fn icon_at(&self, band: Rect, pointer: Vec2, count: usize) -> Option<usize> {
        (0..count).find(|slot| self.icon_rect(band, *slot).contains(pointer))
    }

    fn centered(&self, band: Rect, height: f64) -> f64 {
        band.origin.y + (band.size.y - height) / 2.0
    }
}

pub fn now(format: &str) -> Result<String, String> {
    stamp(&Local::now(), format)
}

pub fn stamp<Tz: TimeZone>(when: &DateTime<Tz>, format: &str) -> Result<String, String>
where
    Tz::Offset: std::fmt::Display,
{
    let items = StrftimeItems::new(format)
        .parse()
        .map_err(|_| format!("tray format {format:?} is not a time format"))?;
    Ok(when.format_with_items(items.iter()).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{FixedOffset, TimeZone};

    fn at(hour: u32, minute: u32, second: u32) -> DateTime<FixedOffset> {
        FixedOffset::west_opt(5 * 3600)
            .unwrap()
            .with_ymd_and_hms(2026, 9, 28, hour, minute, second)
            .unwrap()
    }

    fn band() -> Rect {
        Rect::new(Vec2::ZERO, Vec2::new(1920.0, 64.0))
    }

    #[test]
    fn the_default_format_reads_the_way_the_user_writes_it() {
        assert_eq!(
            stamp(&at(0, 35, 26), DEFAULT_FORMAT).unwrap(),
            "Mon Sep 28 12:35:26 AM"
        );
    }

    #[test]
    fn midnight_and_noon_are_both_twelve() {
        assert!(
            stamp(&at(0, 0, 0), DEFAULT_FORMAT)
                .unwrap()
                .contains("12:00:00 AM")
        );
        assert!(
            stamp(&at(12, 0, 0), DEFAULT_FORMAT)
                .unwrap()
                .contains("12:00:00 PM")
        );
    }

    #[test]
    fn the_afternoon_is_not_written_in_twenty_four_hour_time() {
        let text = stamp(&at(13, 5, 0), DEFAULT_FORMAT).unwrap();
        assert!(text.contains("01:05:00 PM"), "{text}");
    }

    #[test]
    fn a_custom_format_is_obeyed() {
        assert_eq!(stamp(&at(13, 5, 9), "%H:%M").unwrap(), "13:05");
        assert_eq!(stamp(&at(13, 5, 9), "%Y-%m-%d").unwrap(), "2026-09-28");
    }

    #[test]
    fn a_broken_format_is_an_error_not_a_panic() {
        assert!(stamp(&at(1, 2, 3), "%").is_err());
        assert!(stamp(&at(1, 2, 3), "%Q").is_err());
    }

    #[test]
    fn plain_text_in_the_format_is_kept() {
        assert_eq!(stamp(&at(9, 0, 0), "it is %I%p").unwrap(), "it is 09AM");
    }

    #[test]
    fn the_icons_start_at_the_left_edge() {
        let tray = Tray::default();
        let first = tray.icon_rect(band(), 0);
        assert_eq!(first.origin.x, tray.margin);
        assert_eq!(first.size, Vec2::splat(tray.icon));
    }

    #[test]
    fn the_icons_grow_to_the_right_with_an_even_gap() {
        let tray = Tray::default();
        let first = tray.icon_rect(band(), 0);
        let second = tray.icon_rect(band(), 1);
        let third = tray.icon_rect(band(), 2);
        assert_eq!(second.origin.x - first.max().x, tray.gap);
        assert_eq!(third.origin.x - second.max().x, tray.gap);
    }

    #[test]
    fn the_icons_are_centered_in_the_band_like_the_clock() {
        let tray = Tray::default();
        let icon = tray.icon_rect(band(), 3);
        assert_eq!(icon.center().y, band().center().y);
    }

    #[test]
    fn a_click_finds_the_icon_under_it() {
        let tray = Tray::default();
        let second = tray.icon_rect(band(), 1);
        assert_eq!(tray.icon_at(band(), second.center(), 3), Some(1));
    }

    #[test]
    fn a_click_in_the_gap_hits_nothing() {
        let tray = Tray::default();
        let gap = Vec2::new(tray.icon_rect(band(), 0).max().x + 2.0, band().center().y);
        assert_eq!(tray.icon_at(band(), gap, 3), None);
    }

    #[test]
    fn a_click_past_the_last_icon_hits_nothing() {
        let tray = Tray::default();
        let beyond = tray.icon_rect(band(), 5).center();
        assert_eq!(tray.icon_at(band(), beyond, 3), None);
    }

    #[test]
    fn the_icons_never_reach_the_clock() {
        let tray = Tray::default();
        let clock = tray.clock_rect(band(), Vec2::new(240.0, 20.0));
        assert!(tray.icon_rect(band(), 0).max().x < clock.origin.x);
    }

    #[test]
    fn the_clock_sits_against_the_right_edge() {
        let tray = Tray::default();
        let text = Vec2::new(240.0, 26.0);
        let rect = tray.clock_rect(band(), text);
        assert_eq!(rect.max().x, 1920.0 - tray.margin);
    }

    #[test]
    fn the_clock_is_centered_in_the_drawer_band() {
        let tray = Tray::default();
        let text = Vec2::new(240.0, 26.0);
        let rect = tray.clock_rect(band(), text);
        assert_eq!(rect.center().y, band().center().y);
    }

    #[test]
    fn a_longer_clock_only_grows_to_the_left() {
        let tray = Tray::default();
        let short = tray.clock_rect(band(), Vec2::new(200.0, 26.0));
        let long = tray.clock_rect(band(), Vec2::new(300.0, 26.0));
        assert_eq!(short.max().x, long.max().x);
        assert!(long.origin.x < short.origin.x);
    }

    #[test]
    fn the_clock_follows_the_band_down_the_screen() {
        let tray = Tray::default();
        let low = Rect::new(Vec2::new(0.0, 1016.0), Vec2::new(1920.0, 64.0));
        let rect = tray.clock_rect(low, Vec2::new(200.0, 26.0));
        assert_eq!(rect.center().y, low.center().y);
    }
}
