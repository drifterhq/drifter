use serde::Deserialize;

use crate::geom::{Rect, Vec2};

pub const SCALES: [f64; 8] = [1.0, 1.25, 1.5, 1.75, 2.0, 2.25, 2.5, 3.0];
pub const TURNS: [u32; 4] = [0, 90, 180, 270];
pub const LEAST_SCALE: f64 = 0.5;
pub const MOST_SCALE: f64 = 4.0;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MonitorConfig {
    pub name: String,
    #[serde(default = "one")]
    pub scale: f64,
    #[serde(default)]
    pub rotate: u32,
    #[serde(default)]
    pub x: Option<i64>,
    #[serde(default)]
    pub y: Option<i64>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default)]
    pub flip: bool,
    #[serde(default)]
    pub vrr: bool,
    #[serde(default)]
    pub vrr_games: bool,
    #[serde(default)]
    pub mirror: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub hdr: bool,
    #[serde(default)]
    pub hdr_white: Option<u32>,
}

fn one() -> f64 {
    1.0
}

fn yes() -> bool {
    true
}

impl MonitorConfig {
    pub fn named(name: &str) -> MonitorConfig {
        MonitorConfig {
            name: name.to_string(),
            scale: 1.0,
            rotate: 0,
            x: None,
            y: None,
            mode: None,
            enabled: true,
            flip: false,
            vrr: false,
            vrr_games: false,
            mirror: None,
            profile: None,
            hdr: false,
            hdr_white: None,
        }
    }

    pub fn mirrors(&self) -> Option<&str> {
        self.mirror.as_deref().filter(|source| *source != self.name)
    }
}

impl MonitorConfig {
    pub fn check(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("a monitor needs a name, like HDMI-A-1".into());
        }
        if !(LEAST_SCALE..=MOST_SCALE).contains(&self.scale) {
            return Err(format!(
                "monitor {} scale runs from {LEAST_SCALE} to {MOST_SCALE}",
                self.name
            ));
        }
        if !TURNS.contains(&self.rotate) {
            return Err(format!("monitor {} rotate is 0, 90, 180 or 270", self.name));
        }
        if let Some(white) = self.hdr_white
            && !(crate::hdr::LEAST_WHITE..=crate::hdr::MOST_WHITE).contains(&white)
        {
            return Err(format!(
                "monitor {} hdr_white runs from {} to {} nits",
                self.name,
                crate::hdr::LEAST_WHITE,
                crate::hdr::MOST_WHITE
            ));
        }
        if self.mirror.as_deref() == Some(self.name.as_str()) {
            return Err(format!("monitor {} cannot mirror itself", self.name));
        }
        if let Some(mode) = &self.mode
            && crate::mode::parse(mode).is_none()
        {
            return Err(format!(
                "monitor {} mode {mode:?} should look like 1920x1080@60",
                self.name
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Head {
    pub name: String,
    pub pixels: Vec2,
    pub scale: f64,
    pub rotate: u32,
}

impl Head {
    pub fn new(name: &str, pixels: Vec2, config: &[MonitorConfig]) -> Head {
        let own = config.iter().find(|each| each.name == name);
        Head {
            name: name.to_string(),
            pixels,
            scale: own.map(|each| each.scale).unwrap_or(1.0),
            rotate: own.map(|each| each.rotate).unwrap_or(0),
        }
    }

    pub fn turned(&self) -> Vec2 {
        match self.rotate {
            90 | 270 => Vec2::new(self.pixels.y, self.pixels.x),
            _ => self.pixels,
        }
    }

    pub fn size(&self) -> Vec2 {
        (self.turned() / self.scale.max(LEAST_SCALE)).round()
    }
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.origin.x < b.max().x - 0.5
        && b.origin.x < a.max().x - 0.5
        && a.origin.y < b.max().y - 0.5
        && b.origin.y < a.max().y - 0.5
}

fn touches(a: Rect, b: Rect) -> bool {
    let side = |a0: f64, a1: f64, b0: f64, b1: f64| a0 < b1 - 0.5 && b0 < a1 - 0.5;
    let near = |p: f64, q: f64| (p - q).abs() < 0.5;
    let across = (near(a.max().x, b.origin.x) || near(b.max().x, a.origin.x))
        && side(a.origin.y, a.max().y, b.origin.y, b.max().y);
    let down = (near(a.max().y, b.origin.y) || near(b.max().y, a.origin.y))
        && side(a.origin.x, a.max().x, b.origin.x, b.max().x);
    across || down
}

pub fn arrange(heads: &[Head], config: &[MonitorConfig]) -> Vec<Rect> {
    let mut out: Vec<Rect> = Vec::new();
    let mut loose = Vec::new();
    for (index, head) in heads.iter().enumerate() {
        let own = config.iter().find(|each| each.name == head.name);
        let set = own.and_then(|each| each.x.zip(each.y));
        let rect = Rect::new(
            set.map(|(x, y)| Vec2::new(x as f64, y as f64))
                .unwrap_or(Vec2::ZERO),
            head.size(),
        );
        if set.is_none() || out.iter().any(|placed| overlaps(*placed, rect)) {
            loose.push(index);
        }
        out.push(rect);
    }
    for index in loose {
        let right = out
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .map(|(_, rect)| rect.max().x)
            .fold(0.0, f64::max);
        let top = out
            .iter()
            .enumerate()
            .filter(|(other, _)| *other != index)
            .map(|(_, rect)| rect.origin.y)
            .fold(f64::INFINITY, f64::min);
        let top = if top.is_finite() { top } else { 0.0 };
        out[index].origin = Vec2::new(right, top);
    }
    normalize(&mut out);
    out
}

pub fn normalize(rects: &mut [Rect]) {
    let low = rects
        .iter()
        .map(|rect| rect.origin)
        .fold(Vec2::splat(f64::INFINITY), Vec2::min);
    if !low.x.is_finite() {
        return;
    }
    for rect in rects.iter_mut() {
        rect.origin = (rect.origin - low).round();
    }
}

pub fn find(rects: &[Rect], point: Vec2) -> Option<usize> {
    rects.iter().position(|rect| rect.contains(point))
}

pub fn bounds(rects: &[Rect]) -> Rect {
    let low = rects
        .iter()
        .map(|rect| rect.origin)
        .fold(Vec2::splat(f64::INFINITY), Vec2::min);
    let high = rects
        .iter()
        .map(|rect| rect.max())
        .fold(Vec2::splat(f64::NEG_INFINITY), Vec2::max);
    if !low.x.is_finite() {
        return Rect::new(Vec2::ZERO, Vec2::ZERO);
    }
    Rect::new(low, high - low)
}

pub fn snap(rects: &[Rect], moving: usize, wanted: Vec2) -> Vec2 {
    let Some(me) = rects.get(moving).copied() else {
        return wanted;
    };
    let others: Vec<Rect> = rects
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != moving)
        .map(|(_, rect)| *rect)
        .collect();
    if others.is_empty() {
        return Vec2::ZERO;
    }
    let size = me.size;
    let mut spots = Vec::new();
    for other in &others {
        let xs = [
            other.origin.x - size.x,
            other.max().x,
            other.origin.x,
            other.max().x - size.x,
            wanted.x.clamp(other.origin.x - size.x, other.max().x),
        ];
        let ys = [
            other.origin.y - size.y,
            other.max().y,
            other.origin.y,
            other.max().y - size.y,
            wanted.y.clamp(other.origin.y - size.y, other.max().y),
        ];
        for x in [other.origin.x - size.x, other.max().x] {
            for y in ys {
                spots.push(Vec2::new(x, y));
            }
        }
        for y in [other.origin.y - size.y, other.max().y] {
            for x in xs {
                spots.push(Vec2::new(x, y));
            }
        }
    }
    let catch = 24.0;
    let pull = |spot: Vec2| -> Vec2 {
        let mut out = spot;
        for other in &others {
            for (edge, mine) in [
                (other.origin.x, spot.x),
                (other.max().x, spot.x),
                (other.origin.x, spot.x + size.x),
                (other.max().x, spot.x + size.x),
            ] {
                if (edge - mine).abs() < catch && (edge - mine).abs() > 0.0 {
                    out.x = spot.x + (edge - mine);
                }
            }
            for (edge, mine) in [
                (other.origin.y, spot.y),
                (other.max().y, spot.y),
                (other.origin.y, spot.y + size.y),
                (other.max().y, spot.y + size.y),
            ] {
                if (edge - mine).abs() < catch && (edge - mine).abs() > 0.0 {
                    out.y = spot.y + (edge - mine);
                }
            }
        }
        out
    };
    spots
        .into_iter()
        .map(pull)
        .map(Vec2::round)
        .filter(|spot| {
            let rect = Rect::new(*spot, size);
            !others.iter().any(|other| overlaps(*other, rect))
                && others.iter().any(|other| touches(*other, rect))
        })
        .min_by(|a, b| {
            let da = (*a - wanted).length();
            let db = (*b - wanted).length();
            da.total_cmp(&db)
        })
        .unwrap_or(me.origin)
}

pub fn joined(rects: &[Rect]) -> bool {
    if rects.len() < 2 {
        return true;
    }
    let mut reached = vec![false; rects.len()];
    reached[0] = true;
    let mut grew = true;
    while grew {
        grew = false;
        for a in 0..rects.len() {
            if !reached[a] {
                continue;
            }
            for b in 0..rects.len() {
                if !reached[b] && touches(rects[a], rects[b]) {
                    reached[b] = true;
                    grew = true;
                }
            }
        }
    }
    reached.into_iter().all(|on| on)
}

pub fn scale_word(scale: f64) -> String {
    format!("{}%", (scale * 100.0).round() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn head(name: &str, w: f64, h: f64) -> Head {
        Head {
            name: name.into(),
            pixels: Vec2::new(w, h),
            scale: 1.0,
            rotate: 0,
        }
    }

    fn set(name: &str, scale: f64, rotate: u32, at: Option<(i64, i64)>) -> MonitorConfig {
        MonitorConfig {
            name: name.into(),
            scale,
            rotate,
            x: at.map(|(x, _)| x),
            y: at.map(|(_, y)| y),
            mode: None,
            enabled: true,
            flip: false,
            vrr: false,
            vrr_games: false,
            mirror: None,
            profile: None,
            hdr: false,
            hdr_white: None,
        }
    }

    #[test]
    fn scale_and_turn_shape_the_logical_size() {
        let config = [set("DP-1", 2.0, 90, None)];
        let head = Head::new("DP-1", Vec2::new(3840.0, 2160.0), &config);
        assert_eq!(head.size(), Vec2::new(1080.0, 1920.0));
        let plain = Head::new("HDMI-A-1", Vec2::new(1920.0, 1080.0), &config);
        assert_eq!(plain.size(), Vec2::new(1920.0, 1080.0));
    }

    #[test]
    fn unplaced_monitors_line_up_left_to_right() {
        let heads = [head("A", 1920.0, 1080.0), head("B", 1280.0, 1024.0)];
        let rects = arrange(&heads, &[]);
        assert_eq!(rects[0].origin, Vec2::ZERO);
        assert_eq!(rects[1].origin, Vec2::new(1920.0, 0.0));
    }

    #[test]
    fn placed_monitors_keep_their_place_and_start_at_zero() {
        let heads = [head("A", 1920.0, 1080.0), head("B", 1920.0, 1080.0)];
        let config = [
            set("A", 1.0, 0, Some((1920, 500))),
            set("B", 1.0, 0, Some((0, 0))),
        ];
        let rects = arrange(&heads, &config);
        assert_eq!(rects[0].origin, Vec2::new(1920.0, 500.0));
        assert_eq!(rects[1].origin, Vec2::ZERO);
    }

    #[test]
    fn an_overlapping_place_is_moved_aside() {
        let heads = [head("A", 1920.0, 1080.0), head("B", 1920.0, 1080.0)];
        let config = [
            set("A", 1.0, 0, Some((0, 0))),
            set("B", 1.0, 0, Some((100, 100))),
        ];
        let rects = arrange(&heads, &config);
        assert!(!overlaps(rects[0], rects[1]));
        assert!(joined(&rects));
    }

    #[test]
    fn a_dragged_monitor_snaps_against_its_neighbor() {
        let rects = [
            Rect::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0)),
            Rect::new(Vec2::new(1920.0, 0.0), Vec2::new(1280.0, 1024.0)),
        ];
        let spot = snap(&rects, 1, Vec2::new(1950.0, 200.0));
        assert_eq!(spot, Vec2::new(1920.0, 200.0));
        let below = snap(&rects, 1, Vec2::new(300.0, 1100.0));
        assert_eq!(below, Vec2::new(300.0, 1080.0));
        let corner = snap(&rects, 1, Vec2::new(1925.0, 10.0));
        assert_eq!(corner, Vec2::new(1920.0, 0.0));
    }

    #[test]
    fn a_snapped_monitor_never_overlaps() {
        let rects = [
            Rect::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0)),
            Rect::new(Vec2::new(1920.0, 0.0), Vec2::new(1920.0, 1080.0)),
        ];
        let spot = snap(&rects, 1, Vec2::new(900.0, 400.0));
        let rect = Rect::new(spot, rects[1].size);
        assert!(!overlaps(rects[0], rect));
        assert!(touches(rects[0], rect));
    }

    #[test]
    fn the_monitor_under_a_point_is_found() {
        let rects = [
            Rect::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0)),
            Rect::new(Vec2::new(1920.0, 0.0), Vec2::new(1280.0, 1024.0)),
        ];
        assert_eq!(find(&rects, Vec2::new(10.0, 10.0)), Some(0));
        assert_eq!(find(&rects, Vec2::new(1925.0, 10.0)), Some(1));
        assert_eq!(find(&rects, Vec2::new(1925.0, 1050.0)), None);
    }

    #[test]
    fn bad_monitor_settings_are_refused() {
        assert!(set("A", 1.5, 90, None).check().is_ok());
        assert!(set("A", 9.0, 0, None).check().is_err());
        assert!(set("A", 1.0, 45, None).check().is_err());
        assert!(set(" ", 1.0, 0, None).check().is_err());
    }
}
