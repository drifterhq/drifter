use std::time::Duration;

use crate::geom::{Rect, Vec2};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Loading,
    Fading,
    Ready,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Startup {
    pub fade: Duration,
}

impl Default for Startup {
    fn default() -> Self {
        Startup {
            fade: Duration::from_secs(1),
        }
    }
}

impl Startup {
    pub const SPLASH_SIZE: Vec2 = Vec2::new(960.0, 540.0);

    pub fn splash_rect(screen: Vec2) -> Rect {
        Rect::new((screen - Startup::SPLASH_SIZE) / 2.0, Startup::SPLASH_SIZE)
    }

    pub fn wallpaper_rect(image: Vec2, screen: Vec2) -> Rect {
        Rect::fill_centered(image, screen)
    }

    pub fn progress(&self, since_loaded: Option<Duration>) -> f32 {
        match since_loaded {
            None => 0.0,
            Some(_) if self.fade.is_zero() => 1.0,
            Some(elapsed) => (elapsed.as_secs_f32() / self.fade.as_secs_f32()).clamp(0.0, 1.0),
        }
    }

    pub fn alphas(&self, since_loaded: Option<Duration>) -> (f32, f32) {
        let t = self.progress(since_loaded);
        (1.0 - t, t)
    }

    pub fn phase(&self, since_loaded: Option<Duration>) -> Phase {
        match since_loaded {
            None => Phase::Loading,
            Some(_) if self.progress(since_loaded) >= 1.0 => Phase::Ready,
            Some(_) => Phase::Fading,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HD: Vec2 = Vec2::new(1920.0, 1080.0);
    const UHD: Vec2 = Vec2::new(3840.0, 2160.0);

    #[test]
    fn the_splash_is_960_by_540_centered_on_1080p() {
        let r = Startup::splash_rect(HD);
        assert_eq!(r.origin, Vec2::new(480.0, 270.0));
        assert_eq!(r.size, Vec2::new(960.0, 540.0));
    }

    #[test]
    fn the_splash_stays_960_by_540_on_a_4k_screen() {
        let r = Startup::splash_rect(UHD);
        assert_eq!(r.size, Vec2::new(960.0, 540.0));
        assert_eq!(r.origin, Vec2::new(1440.0, 810.0));
    }

    #[test]
    fn the_splash_is_never_scaled_to_the_screen() {
        for screen in [HD, UHD, Vec2::new(1280.0, 800.0)] {
            assert_eq!(Startup::splash_rect(screen).size, Startup::SPLASH_SIZE);
        }
    }

    #[test]
    fn the_wallpaper_covers_the_screen() {
        let r = Startup::wallpaper_rect(Vec2::new(1600.0, 1200.0), HD);
        assert!(r.size.x >= HD.x && r.size.y >= HD.y);
    }

    #[test]
    fn loading_shows_only_the_splash() {
        assert_eq!(Startup::default().alphas(None), (1.0, 0.0));
    }

    #[test]
    fn the_fade_starts_on_the_splash_alone() {
        let s = Startup::default();
        assert_eq!(s.alphas(Some(Duration::ZERO)), (1.0, 0.0));
    }

    #[test]
    fn the_fade_ends_on_the_wallpaper_alone() {
        let s = Startup::default();
        assert_eq!(s.alphas(Some(Duration::from_secs(1))), (0.0, 1.0));
    }

    #[test]
    fn halfway_through_both_are_half_visible() {
        let s = Startup::default();
        let (splash, wallpaper) = s.alphas(Some(Duration::from_millis(500)));
        assert!((splash - 0.5).abs() < 1e-6);
        assert!((wallpaper - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_two_alphas_always_sum_to_one() {
        let s = Startup::default();
        for ms in [0, 1, 250, 500, 750, 999, 1000, 5000] {
            let (a, b) = s.alphas(Some(Duration::from_millis(ms)));
            assert!((a + b - 1.0).abs() < 1e-6, "at {ms}ms");
        }
    }

    #[test]
    fn the_fade_clamps_past_its_end() {
        let s = Startup::default();
        assert_eq!(s.alphas(Some(Duration::from_secs(60))), (0.0, 1.0));
    }

    #[test]
    fn a_zero_fade_switches_instantly() {
        let s = Startup {
            fade: Duration::ZERO,
        };
        assert_eq!(s.alphas(Some(Duration::ZERO)), (0.0, 1.0));
    }

    #[test]
    fn the_phases_run_loading_then_fading_then_ready() {
        let s = Startup::default();
        assert_eq!(s.phase(None), Phase::Loading);
        assert_eq!(s.phase(Some(Duration::from_millis(500))), Phase::Fading);
        assert_eq!(s.phase(Some(Duration::from_secs(1))), Phase::Ready);
    }
}
