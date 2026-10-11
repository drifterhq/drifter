use std::time::{Duration, Instant};

use crate::geom::Rect;
use crate::image::Image;

pub fn ease(t: f64) -> f64 {
    crate::motion::smooth(t)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glide {
    pub from: f64,
    pub to: f64,
    pub shown: f64,
    pub since: Instant,
    pub span: Duration,
}

impl Glide {
    pub fn new(from: f64, to: f64, span: Duration) -> Glide {
        Glide {
            from,
            to,
            shown: from,
            since: Instant::now(),
            span,
        }
    }

    pub fn retarget(&mut self, to: f64, now: Instant) {
        self.from = self.shown;
        self.to = to;
        self.since = now;
    }

    pub fn progress(&self, now: Instant) -> f64 {
        match self.span.is_zero() {
            true => 1.0,
            false => now.duration_since(self.since).as_secs_f64() / self.span.as_secs_f64(),
        }
    }

    pub fn step(&mut self, now: Instant, dense: f64) -> Option<f64> {
        let t = ease(self.progress(now));
        let dense = dense.max(1.0);
        let next = ((self.from + (self.to - self.from) * t) * dense).round() / dense;
        let next = match self.progress(now) >= 1.0 {
            true => self.to,
            false => next,
        };
        if next == self.shown {
            return None;
        }
        self.shown = next;
        Some(next)
    }

    pub fn done(&self, now: Instant) -> bool {
        self.progress(now) >= 1.0 && self.shown == self.to
    }
}

#[derive(Clone)]
pub struct Fade {
    pub area: Rect,
    pub at: (u32, u32),
    pub old: Image,
    pub new: Image,
    pub since: Instant,
    pub span: Duration,
}

impl Fade {
    pub fn new(area: Rect, at: (u32, u32), old: Image, new: Image, span: Duration) -> Fade {
        Fade {
            area,
            at,
            old,
            new,
            since: Instant::now(),
            span,
        }
    }

    pub fn old_alpha(&self, now: Instant) -> f64 {
        let t = match self.span.is_zero() {
            true => 1.0,
            false => now.duration_since(self.since).as_secs_f64() / self.span.as_secs_f64(),
        };
        1.0 - ease(t)
    }

    pub fn done(&self, now: Instant) -> bool {
        self.old_alpha(now) <= 0.0
    }
}

pub fn crop(image: &Image, area: Rect, dense: f64) -> Option<((u32, u32), Image)> {
    let x0 = ((area.origin.x * dense).floor().max(0.0) as u32).min(image.width);
    let y0 = ((area.origin.y * dense).floor().max(0.0) as u32).min(image.height);
    let x1 = ((area.max().x * dense).ceil().max(0.0) as u32).min(image.width);
    let y1 = ((area.max().y * dense).ceil().max(0.0) as u32).min(image.height);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (width, height) = (x1 - x0, y1 - y0);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in y0..y1 {
        let from = ((y * image.width + x0) * 4) as usize;
        rgba.extend_from_slice(&image.rgba[from..from + (width * 4) as usize]);
    }
    Some(((x0, y0), Image { width, height, rgba }))
}

pub fn paste(image: &mut Image, at: (u32, u32), piece: &Image) {
    for y in 0..piece.height {
        if at.1 + y >= image.height {
            break;
        }
        let wide = piece.width.min(image.width.saturating_sub(at.0));
        let to = (((at.1 + y) * image.width + at.0) * 4) as usize;
        let from = (y * piece.width * 4) as usize;
        image.rgba[to..to + (wide * 4) as usize].copy_from_slice(&piece.rgba[from..from + (wide * 4) as usize]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Vec2;

    #[test]
    fn a_glide_lands_on_its_target_and_snaps_to_pixels() {
        let mut glide = Glide::new(0.0, 100.0, Duration::from_millis(150));
        let start = glide.since;
        let mid = glide.step(start + Duration::from_millis(40), 2.0).expect("it moves");
        assert_eq!(mid * 2.0, (mid * 2.0).round(), "half pixels at density 2");
        assert!(mid > 0.0 && mid < 100.0);
        assert_eq!(glide.step(start + Duration::from_millis(400), 1.0), Some(100.0));
        assert!(glide.done(start + Duration::from_millis(400)));
        assert_eq!(glide.step(start + Duration::from_millis(500), 1.0), None, "nothing more to do");
    }

    #[test]
    fn retarget_starts_from_what_is_shown() {
        let mut glide = Glide::new(0.0, 100.0, Duration::from_millis(150));
        let start = glide.since;
        let shown = glide.step(start + Duration::from_millis(50), 1.0).unwrap();
        glide.retarget(300.0, start + Duration::from_millis(50));
        assert_eq!((glide.from, glide.to), (shown, 300.0));
        assert_eq!(glide.step(start + Duration::from_millis(50), 1.0), None, "no jump at the turn");
    }

    #[test]
    fn a_fade_runs_old_alpha_from_1_to_0() {
        let blank = Image { width: 1, height: 1, rgba: vec![0; 4] };
        let fade = Fade::new(Rect::new(Vec2::ZERO, Vec2::splat(1.0)), (0, 0), blank.clone(), blank, Duration::from_millis(120));
        assert_eq!(fade.old_alpha(fade.since), 1.0);
        let half = fade.old_alpha(fade.since + Duration::from_millis(60));
        assert!(half > 0.0 && half < 1.0);
        assert_eq!(fade.old_alpha(fade.since + Duration::from_millis(500)), 0.0);
        assert!(fade.done(fade.since + Duration::from_millis(500)));
    }

    #[test]
    fn crop_then_paste_gives_back_the_same_pixels() {
        let mut image = Image { width: 4, height: 3, rgba: (0..48).collect() };
        let (at, piece) = crop(&image, Rect::new(Vec2::new(1.0, 1.0), Vec2::new(2.0, 2.0)), 1.0).unwrap();
        assert_eq!((at, piece.width, piece.height), ((1, 1), 2, 2));
        let before = image.rgba.clone();
        paste(&mut image, at, &piece);
        assert_eq!(image.rgba, before);
    }
}
