use chrono::format::StrftimeItems;
use chrono::{DateTime, Local, TimeZone};

use crate::geom::{Rect, Vec2};
use crate::image::Image;

pub const DEFAULT_FORMAT: &str = "%a %b %d %I:%M:%S %p";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tray {
    pub margin: f64,
    pub size: f64,
    pub ink: [u8; 3],
    pub icon: f64,
    pub gap: f64,
    pub button: f64,
    pub button_gap: f64,
}

impl Default for Tray {
    fn default() -> Self {
        Tray {
            margin: 12.0,
            size: 13.0,
            ink: [236, 238, 244],
            icon: 18.0,
            gap: 14.0,
            button: 20.0,
            button_gap: 14.0,
        }
    }
}

impl Tray {
    pub fn clock_rect(&self, band: Rect, text: Vec2) -> Rect {
        let x = self.power_rect(band).origin.x - self.button_gap - text.x;
        Rect::new(Vec2::new(x, self.centered(band, text.y)), text)
    }

    pub fn power_rect(&self, band: Rect) -> Rect {
        let size = Vec2::splat(self.button);
        let x = band.max().x - self.margin - size.x;
        Rect::new(Vec2::new(x, self.centered(band, size.y)), size)
    }

    pub fn speaker_rect(&self, band: Rect, clock: Option<Rect>) -> Rect {
        let size = Vec2::splat(self.button);
        let right = clock
            .map(|clock| clock.origin.x)
            .unwrap_or_else(|| self.power_rect(band).origin.x);
        let x = right - self.button_gap - size.x;
        Rect::new(Vec2::new(x, self.centered(band, size.y)), size)
    }

    pub fn left_of(&self, band: Rect, right: f64) -> Rect {
        let size = Vec2::splat(self.button);
        let x = right - self.button_gap - size.x;
        Rect::new(Vec2::new(x, self.centered(band, size.y)), size)
    }

    pub fn on_power(&self, band: Rect, pointer: Vec2) -> bool {
        self.power_rect(band).contains(pointer)
    }

    pub fn icon_rect(&self, band: Rect, slot: usize) -> Rect {
        let step = self.icon + self.gap;
        let x = band.origin.x + self.margin + step * slot as f64;
        let size = Vec2::splat(self.icon);
        Rect::new(Vec2::new(x, self.centered(band, size.y)), size)
    }

    pub fn room(&self, band: Rect, before: f64) -> usize {
        (0..)
            .take_while(|slot| self.icon_rect(band, *slot).max().x + self.gap <= before)
            .count()
    }

    pub fn stats_rect(&self, band: Rect, icons: usize, text: Vec2) -> Rect {
        let x = match icons {
            0 => band.origin.x + self.margin,
            shown => self.icon_rect(band, shown - 1).max().x + self.gap,
        };
        Rect::new(Vec2::new(x, self.centered(band, text.y)), text)
    }

    pub fn layout(
        &self,
        band: Rect,
        before: f64,
        icons: usize,
        stats: Option<f64>,
    ) -> (usize, bool) {
        if let Some(width) = stats {
            let shown = icons.min(self.room(band, before - width - self.gap));
            let rect = self.stats_rect(band, shown, Vec2::new(width, 0.0));
            if rect.max().x + self.gap <= before {
                return (shown, true);
            }
        }
        (icons.min(self.room(band, before)), false)
    }

    pub fn icon_at(&self, band: Rect, pointer: Vec2, count: usize) -> Option<usize> {
        (0..count).find(|slot| self.icon_rect(band, *slot).contains(pointer))
    }

    fn centered(&self, band: Rect, height: f64) -> f64 {
        band.origin.y + (band.size.y - height) / 2.0
    }
}

pub fn paint_power(side: u32, color: [u8; 3]) -> Image {
    let side = side.max(8);
    let size = f64::from(side);
    let middle = size / 2.0;
    let ring = size * 0.36;
    let thick = (size * 0.11).max(1.5);
    let stem_top = size * 0.10;
    let stem_bottom = middle - size * 0.04;
    let gap = 0.62;
    let mut rgba = vec![0u8; (side as usize) * (side as usize) * 4];

    for y in 0..side {
        let dy = f64::from(y) + 0.5 - middle;
        for x in 0..side {
            let dx = f64::from(x) + 0.5 - middle;
            let away = (dx * dx + dy * dy).sqrt();
            let from_top = dx.atan2(-dy).abs();
            let round = if from_top > gap {
                (thick / 2.0 - (away - ring).abs() + 0.5).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let along = (dy - (stem_top - middle)).min((stem_bottom - middle) - dy);
            let across = thick / 2.0 - dx.abs();
            let stem = (along + 0.5).clamp(0.0, 1.0) * (across + 0.5).clamp(0.0, 1.0);
            let ink = round.max(stem);
            if ink <= 0.0 {
                continue;
            }
            let at = ((y as usize) * (side as usize) + x as usize) * 4;
            for (channel, value) in color.iter().enumerate() {
                rgba[at + channel] = (f64::from(*value) * ink).round().clamp(0.0, 255.0) as u8;
            }
            rgba[at + 3] = (ink * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }

    Image {
        width: side,
        height: side,
        rgba,
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

    #[test]
    fn icons_stop_short_of_the_clock() {
        let tray = Tray::default();
        let band = Rect::new(Vec2::ZERO, Vec2::new(400.0, 36.0));
        let clock = tray.clock_rect(band, Vec2::new(200.0, 14.0));
        let room = tray.room(band, clock.origin.x);
        assert!(room > 0);
        let last = tray.icon_rect(band, room - 1);
        assert!(last.max().x + tray.gap <= clock.origin.x);
        let next = tray.icon_rect(band, room);
        assert!(next.max().x + tray.gap > clock.origin.x);
    }

    #[test]
    fn the_speaker_sits_left_of_the_clock_or_the_power_button() {
        let tray = Tray::default();
        let band = band();
        let clock = tray.clock_rect(band, Vec2::new(180.0, 14.0));
        let speaker = tray.speaker_rect(band, Some(clock));
        assert_eq!(speaker.max().x + tray.button_gap, clock.origin.x);
        assert_eq!(speaker.center().y, band.center().y);
        let alone = tray.speaker_rect(band, None);
        assert_eq!(
            alone.max().x + tray.button_gap,
            tray.power_rect(band).origin.x
        );
    }

    #[test]
    fn the_stats_sit_at_the_margin_with_no_icons() {
        let tray = Tray::default();
        let rect = tray.stats_rect(band(), 0, Vec2::new(200.0, 14.0));
        assert_eq!(rect.origin.x, tray.margin);
        assert_eq!(rect.center().y, band().center().y);
    }

    #[test]
    fn the_stats_sit_one_gap_right_of_the_last_icon() {
        let tray = Tray::default();
        let rect = tray.stats_rect(band(), 3, Vec2::new(200.0, 14.0));
        assert_eq!(rect.origin.x, tray.icon_rect(band(), 2).max().x + tray.gap);
    }

    #[test]
    fn with_room_every_icon_and_the_stats_show() {
        let tray = Tray::default();
        assert_eq!(tray.layout(band(), 1500.0, 4, Some(200.0)), (4, true));
        assert_eq!(tray.layout(band(), 1500.0, 4, None), (4, false));
    }

    #[test]
    fn icons_give_up_room_to_the_stats() {
        let tray = Tray::default();
        let before = 400.0;
        let alone = tray.layout(band(), before, 50, None).0;
        let (shown, stats) = tray.layout(band(), before, 50, Some(200.0));
        assert!(stats);
        assert!(shown < alone);
        let rect = tray.stats_rect(band(), shown, Vec2::new(200.0, 14.0));
        assert!(rect.max().x + tray.gap <= before);
    }

    #[test]
    fn the_stats_hide_before_they_reach_the_clock() {
        let tray = Tray::default();
        let (shown, stats) = tray.layout(band(), 150.0, 2, Some(200.0));
        assert!(!stats);
        assert_eq!(shown, tray.room(band(), 150.0).min(2));
    }

    #[test]
    fn no_room_means_no_icons() {
        let tray = Tray::default();
        let band = Rect::new(Vec2::ZERO, Vec2::new(400.0, 36.0));
        assert_eq!(tray.room(band, 10.0), 0);
    }
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
    fn the_power_button_sits_against_the_right_edge() {
        let tray = Tray::default();
        let rect = tray.power_rect(band());
        assert_eq!(rect.max().x, 1920.0 - tray.margin);
        assert_eq!(rect.size, Vec2::splat(tray.button));
    }

    #[test]
    fn the_clock_sits_left_of_the_power_button() {
        let tray = Tray::default();
        let text = Vec2::new(240.0, 26.0);
        let rect = tray.clock_rect(band(), text);
        let button = tray.power_rect(band());
        assert_eq!(button.origin.x - rect.max().x, tray.button_gap);
    }

    #[test]
    fn the_power_button_is_centered_in_the_band() {
        let tray = Tray::default();
        assert_eq!(tray.power_rect(band()).center().y, band().center().y);
    }

    #[test]
    fn a_click_finds_the_power_button_and_nothing_beside_it() {
        let tray = Tray::default();
        let button = tray.power_rect(band());
        assert!(tray.on_power(band(), button.center()));
        let beside = Vec2::new(button.origin.x - 4.0, button.center().y);
        assert!(!tray.on_power(band(), beside));
    }

    #[test]
    fn the_power_glyph_is_a_ring_with_a_stem_and_a_gap_at_the_top() {
        let image = paint_power(40, [255, 255, 255]);
        let alpha = |x: u32, y: u32| image.rgba[((y * image.width + x) * 4 + 3) as usize];
        assert!(alpha(20, 8) > 200, "the stem");
        assert!(alpha(20, 20) == 0, "the hollow middle");
        assert!(alpha(20, 5) > 0, "the stem reaches the top");
        assert!(alpha(6, 20) > 200, "the left of the ring");
        assert!(alpha(33, 20) > 200, "the right of the ring");
        assert!(alpha(23, 6) == 0, "the gap beside the stem");
        assert!(alpha(20, 34) > 200, "the bottom of the ring");
    }

    #[test]
    fn every_power_pixel_is_premultiplied() {
        let image = paint_power(20, [200, 100, 50]);
        for pixel in image.rgba.chunks(4) {
            assert!(pixel[0] <= pixel[3] && pixel[1] <= pixel[3] && pixel[2] <= pixel[3]);
        }
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
