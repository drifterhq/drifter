use std::time::Duration;

use crate::drawer::ease;
use crate::geom::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Opening,
    Closing,
    Minimizing,
    Restoring,
    Resizing,
    Filling,
    Unfilling,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Motion {
    pub phase: Phase,
    pub span: Duration,
    pub gone: Duration,
    pub dock: Rect,
}

pub const OPEN: Duration = Duration::from_millis(190);
pub const CLOSE: Duration = Duration::from_millis(150);
pub const STOW: Duration = Duration::from_millis(260);
pub const RESIZE: Duration = Duration::from_millis(230);

impl Motion {
    pub fn new(phase: Phase, dock: Rect) -> Self {
        let span = match phase {
            Phase::Opening => OPEN,
            Phase::Closing => CLOSE,
            Phase::Minimizing | Phase::Restoring => STOW,
            Phase::Resizing | Phase::Filling | Phase::Unfilling => RESIZE,
        };
        Motion {
            phase,
            span,
            gone: Duration::ZERO,
            dock,
        }
    }

    pub fn advance(&mut self, dt: Duration) {
        self.gone = (self.gone + dt).min(self.span);
    }

    pub fn progress(&self) -> f64 {
        if self.span.is_zero() {
            return 1.0;
        }
        (self.gone.as_secs_f64() / self.span.as_secs_f64()).clamp(0.0, 1.0)
    }

    pub fn done(&self) -> bool {
        self.progress() >= 1.0
    }

    pub fn shape_for(&self, window: Rect, still: bool) -> (Rect, f32) {
        if !still {
            return self.shape(window);
        }
        let t = self.progress().min(1.0) as f32;
        match self.phase {
            Phase::Opening | Phase::Restoring => (window, t),
            Phase::Closing | Phase::Minimizing => (window, 1.0 - t),
            Phase::Resizing | Phase::Filling | Phase::Unfilling => (window, 1.0),
        }
    }

    pub fn chrome(&self) -> f64 {
        let t = smooth(self.progress());
        match self.phase {
            Phase::Filling => 1.0 - t,
            Phase::Unfilling => t,
            _ => 1.0,
        }
    }

    pub fn shape(&self, window: Rect) -> (Rect, f32) {
        let t = self.progress();
        match self.phase {
            Phase::Opening => (grow(window, 0.90 + 0.10 * ease(t)), t.min(1.0) as f32),
            Phase::Closing => {
                let fade = 1.0 - t;
                (grow(window, 1.0 - 0.10 * smooth(t)), fade as f32)
            }
            Phase::Minimizing => {
                let t = smooth(t);
                (between(window, self.dock, t), (1.0 - t * t) as f32)
            }
            Phase::Restoring => {
                let t = smooth(t);
                (between(self.dock, window, t), (t * t) as f32)
            }
            Phase::Resizing | Phase::Filling | Phase::Unfilling => (between(self.dock, window, smooth(t)), 1.0),
        }
    }
}

pub const FROM_SCALE: f64 = 0.96;

pub fn appear(t: f64, reduce: bool) -> (f64, f64) {
    let shown = smooth(t);
    let scale = match reduce {
        true => 1.0,
        false => FROM_SCALE + (1.0 - FROM_SCALE) * shown,
    };
    (shown, scale)
}

pub fn vanish(t: f64, reduce: bool) -> (f64, f64) {
    appear(1.0 - t.clamp(0.0, 1.0), reduce)
}

pub fn smooth(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t) * (1.0 - t) * (1.0 - t)
}

pub fn grow(rect: Rect, factor: f64) -> Rect {
    let size = rect.size * factor;
    Rect::new(rect.center() - size / 2.0, size)
}

pub fn between(from: Rect, to: Rect, t: f64) -> Rect {
    let t = t.clamp(0.0, 1.0);
    Rect::new(
        from.origin + (to.origin - from.origin) * t,
        from.size + (to.size - from.size) * t,
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_resize_glides_from_the_old_frame_to_the_new_one_and_stays_solid() {
        let old = dock();
        let mut motion = Motion::new(Phase::Resizing, old);
        assert_eq!(motion.shape(window()), (old, 1.0));
        motion.advance(RESIZE / 2);
        let (mid, fade) = motion.shape(window());
        assert_eq!(fade, 1.0);
        assert!(mid.size.x > old.size.x && mid.size.x < window().size.x);
        motion.advance(RESIZE);
        assert_eq!(motion.shape(window()).0, window());
    }

    use super::*;
    use crate::geom::Vec2;

    #[test]
    fn appear_starts_clear_and_small_and_ends_solid_and_whole() {
        assert_eq!(appear(0.0, false), (0.0, FROM_SCALE));
        assert_eq!(appear(1.0, false), (1.0, 1.0));
        assert_eq!(appear(0.0, true), (0.0, 1.0), "reduce motion keeps the size");
        let (alpha, scale) = appear(0.5, false);
        assert!(alpha > 0.5 && alpha < 1.0 && scale > FROM_SCALE && scale < 1.0);
        assert_eq!(vanish(0.0, false), (1.0, 1.0));
        assert_eq!(vanish(1.0, false), (0.0, FROM_SCALE));
        assert_eq!(vanish(2.0, true), (0.0, 1.0));
    }

    #[test]
    fn filling_glides_like_a_resize_and_fades_the_frame() {
        let old = window();
        let screen = Rect::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        let mut filling = Motion::new(Phase::Filling, old);
        assert_eq!(filling.shape(screen), (old, 1.0));
        assert_eq!(filling.chrome(), 1.0, "the frame starts whole");
        filling.advance(RESIZE / 2);
        let (mid, fade) = filling.shape(screen);
        assert_eq!(fade, 1.0);
        assert!(mid.size.x > old.size.x && mid.size.x < screen.size.x);
        assert!(filling.chrome() > 0.0 && filling.chrome() < 1.0);
        filling.advance(RESIZE);
        assert_eq!(filling.shape(screen).0, screen);
        assert_eq!(filling.chrome(), 0.0, "and is gone at the end");
        let mut leaving = Motion::new(Phase::Unfilling, screen);
        assert_eq!(leaving.chrome(), 0.0);
        leaving.advance(RESIZE);
        assert_eq!((leaving.shape(old).0, leaving.chrome()), (old, 1.0));
    }

    #[test]
    fn reduce_motion_keeps_the_fade_but_not_the_move() {
        for phase in [Phase::Opening, Phase::Closing, Phase::Minimizing, Phase::Restoring, Phase::Resizing] {
            let mut motion = Motion::new(phase, dock());
            motion.advance(motion.span / 2);
            let (rect, fade) = motion.shape_for(window(), true);
            assert_eq!(rect, window(), "{phase:?} stays in place");
            let want = match phase {
                Phase::Resizing => 1.0,
                _ => 0.5,
            };
            assert!((fade - want).abs() < 0.01, "{phase:?} fades to {fade}");
            assert_eq!(motion.shape_for(window(), false), motion.shape(window()));
        }
    }

    fn window() -> Rect {
        Rect::new(Vec2::new(400.0, 300.0), Vec2::new(800.0, 600.0))
    }

    fn dock() -> Rect {
        Rect::new(Vec2::new(900.0, 1020.0), Vec2::new(44.0, 44.0))
    }

    #[test]
    fn growing_keeps_the_center_still() {
        let rect = window();
        let bigger = grow(rect, 1.5);
        let smaller = grow(rect, 0.5);
        assert_eq!(bigger.center(), rect.center());
        assert_eq!(smaller.center(), rect.center());
        assert_eq!(smaller.size, rect.size * 0.5);
    }

    #[test]
    fn between_lands_on_both_ends() {
        let from = window();
        let to = dock();
        assert_eq!(between(from, to, 0.0), from);
        assert_eq!(between(from, to, 1.0), to);
    }

    #[test]
    fn a_window_opens_from_slightly_small_and_clear() {
        let motion = Motion::new(Phase::Opening, dock());
        let (rect, alpha) = motion.shape(window());
        assert!(rect.size.x < window().size.x);
        assert_eq!(alpha, 0.0);
        assert_eq!(rect.center(), window().center());
    }

    #[test]
    fn a_window_finishes_opening_at_its_real_size() {
        let mut motion = Motion::new(Phase::Opening, dock());
        motion.advance(OPEN);
        let (rect, alpha) = motion.shape(window());
        assert!((rect.size.x - window().size.x).abs() < 1e-9);
        assert_eq!(alpha, 1.0);
        assert!(motion.done());
    }

    #[test]
    fn an_opening_window_springs_past_its_size_on_the_way() {
        let mut motion = Motion::new(Phase::Opening, dock());
        let mut peak = 0.0f64;
        for _ in 0..40 {
            motion.advance(Duration::from_millis(5));
            peak = peak.max(motion.shape(window()).0.size.x);
        }
        assert!(peak > window().size.x, "the open is flat, not elastic");
    }

    #[test]
    fn a_closing_window_shrinks_and_fades_out() {
        let mut motion = Motion::new(Phase::Closing, dock());
        motion.advance(CLOSE);
        let (rect, alpha) = motion.shape(window());
        assert!(rect.size.x < window().size.x);
        assert_eq!(alpha, 0.0);
    }

    #[test]
    fn a_minimizing_window_ends_on_its_dock_icon() {
        let mut motion = Motion::new(Phase::Minimizing, dock());
        motion.advance(STOW);
        let (rect, alpha) = motion.shape(window());
        assert_eq!(rect, dock());
        assert_eq!(alpha, 0.0);
    }

    #[test]
    fn a_minimizing_window_starts_where_it_was() {
        let motion = Motion::new(Phase::Minimizing, dock());
        let (rect, alpha) = motion.shape(window());
        assert_eq!(rect, window());
        assert_eq!(alpha, 1.0);
    }

    #[test]
    fn restoring_is_the_way_back() {
        let motion = Motion::new(Phase::Restoring, dock());
        assert_eq!(motion.shape(window()).0, dock());

        let mut done = Motion::new(Phase::Restoring, dock());
        done.advance(STOW);
        assert_eq!(done.shape(window()).0, window());
        assert_eq!(done.shape(window()).1, 1.0);
    }

    #[test]
    fn a_motion_never_runs_past_its_span() {
        let mut motion = Motion::new(Phase::Opening, dock());
        motion.advance(Duration::from_secs(9));
        assert_eq!(motion.progress(), 1.0);
        assert!(motion.done());
    }

    #[test]
    fn closing_is_quicker_than_opening_and_stowing_is_slowest() {
        assert!(CLOSE < OPEN);
        assert!(OPEN < STOW);
    }

    #[test]
    fn smooth_starts_fast_and_lands_exactly() {
        assert_eq!(smooth(0.0), 0.0);
        assert_eq!(smooth(1.0), 1.0);
        assert!(smooth(0.5) > 0.5, "the ease does not front load");
    }
}
