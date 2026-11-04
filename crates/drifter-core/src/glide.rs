use std::time::Instant;

use crate::geom::Rect;
use crate::motion::{between, smooth};
use crate::paint::Canvas;

const SLIDE: f64 = 0.12;
const FADE: f64 = 0.10;
pub const ALPHA: f64 = 0.07;
pub const RADIUS: f64 = 9.0;
const PRESS_ALPHA: f64 = 0.16;
const PRESS_HOLD: f64 = 0.08;
const PRESS_FADE: f64 = 0.12;

#[derive(Debug, Clone)]
pub struct Glide {
    from: Rect,
    to: Rect,
    since: Instant,
    alpha: (f64, f64, Instant),
    pressed: Option<Instant>,
    pace: f64,
}

impl Glide {
    pub fn aim(slot: &mut Option<Glide>, target: Option<Rect>, pace: f64) {
        let now = Instant::now();
        match (target, slot.as_mut()) {
            (Some(rect), Some(glide)) if glide.shown() > 0.001 || glide.alpha.1 > 0.0 => {
                glide.pace = pace;
                if glide.to != rect {
                    glide.from = glide.current();
                    glide.to = rect;
                    glide.since = now;
                }
                if glide.alpha.1 < 1.0 {
                    glide.alpha = (glide.shown(), 1.0, now);
                }
            }
            (Some(rect), _) => {
                *slot = Some(Glide { from: rect, to: rect, since: now, alpha: (0.0, 1.0, now), pressed: None, pace });
            }
            (None, Some(glide)) => {
                if glide.alpha.1 > 0.0 {
                    glide.alpha = (glide.shown(), 0.0, now);
                }
            }
            (None, None) => {}
        }
    }

    pub fn press(slot: &mut Option<Glide>) {
        if let Some(glide) = slot.as_mut() {
            glide.pressed = Some(Instant::now());
        }
    }

    fn scaled(&self, seconds: f64) -> f64 {
        match self.pace > 0.0 {
            true => seconds / self.pace,
            false => 0.0,
        }
    }

    fn shown(&self) -> f64 {
        let span = self.scaled(FADE);
        let t = match span > 0.0 {
            true => (self.alpha.2.elapsed().as_secs_f64() / span).min(1.0),
            false => 1.0,
        };
        self.alpha.0 + (self.alpha.1 - self.alpha.0) * smooth(t)
    }

    fn current(&self) -> Rect {
        let span = self.scaled(SLIDE);
        let t = match span > 0.0 {
            true => (self.since.elapsed().as_secs_f64() / span).min(1.0),
            false => 1.0,
        };
        between(self.from, self.to, smooth(t))
    }

    fn boost(&self) -> Option<f64> {
        let at = self.pressed?;
        let t = at.elapsed().as_secs_f64() * self.pace.max(1.0);
        match t {
            t if t < PRESS_HOLD => Some(PRESS_ALPHA / ALPHA),
            t if t < PRESS_HOLD + PRESS_FADE => Some(PRESS_ALPHA / ALPHA * (1.0 - smooth((t - PRESS_HOLD) / PRESS_FADE))),
            _ => None,
        }
    }

    pub fn shade(&self) -> f64 {
        let alpha = self.shown();
        match self.boost() {
            Some(boost) => ALPHA * boost.max(alpha),
            None => alpha * ALPHA,
        }
    }

    pub fn rect(&self) -> Rect {
        self.current()
    }

    pub fn draw(&self, canvas: &mut Canvas, ink: [u8; 3]) {
        let shade = self.shade();
        if shade <= 0.0005 {
            return;
        }
        canvas.round_rect(self.current(), RADIUS, ink, shade);
    }

    pub fn busy(&self) -> bool {
        self.since.elapsed().as_secs_f64() < self.scaled(SLIDE)
            || self.alpha.2.elapsed().as_secs_f64() < self.scaled(FADE)
            || self.boost().is_some()
    }

    pub fn gone(&self) -> bool {
        self.alpha.1 <= 0.0 && !self.busy()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geom::Vec2;

    fn row(y: f64) -> Rect {
        Rect::new(Vec2::new(0.0, y), Vec2::new(100.0, 30.0))
    }

    #[test]
    fn it_lands_on_the_target_and_fades_out_when_aimed_at_nothing() {
        let mut slot = None;
        Glide::aim(&mut slot, Some(row(0.0)), 0.0);
        Glide::aim(&mut slot, Some(row(40.0)), 0.0);
        let glide = slot.as_ref().unwrap();
        assert_eq!(glide.rect(), row(40.0));
        assert!((glide.shade() - ALPHA).abs() < 1e-9);
        Glide::aim(&mut slot, None, 0.0);
        assert!(slot.as_ref().unwrap().gone());
    }

    #[test]
    fn a_press_flashes_brighter() {
        let mut slot = None;
        Glide::aim(&mut slot, Some(row(0.0)), 0.0);
        Glide::press(&mut slot);
        assert!(slot.as_ref().unwrap().shade() > ALPHA);
    }
}
