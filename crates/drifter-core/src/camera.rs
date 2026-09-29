use crate::geom::{Rect, Vec2};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub center: Vec2,
    pub zoom: f64,
}

impl Camera {
    pub const WORK_ZOOM: f64 = 1.0;

    pub fn new(center: Vec2, zoom: f64) -> Self {
        Camera { center, zoom }
    }

    pub fn at_work_zoom(center: Vec2) -> Self {
        Camera::new(center, Camera::WORK_ZOOM)
    }

    pub fn visible_size(&self, viewport: Vec2) -> Vec2 {
        viewport / self.zoom
    }

    pub fn visible_rect(&self, viewport: Vec2) -> Rect {
        Rect::from_center(self.center, self.visible_size(viewport))
    }

    pub fn world_from_screen(&self, screen: Vec2, viewport: Vec2) -> Vec2 {
        (screen - viewport / 2.0) / self.zoom + self.center
    }

    pub fn screen_from_world(&self, world: Vec2, viewport: Vec2) -> Vec2 {
        (world - self.center) * self.zoom + viewport / 2.0
    }

    pub fn fit_zoom(viewport: Vec2, world: Vec2) -> f64 {
        (viewport.x / world.x).min(viewport.y / world.y)
    }

    pub fn fitting_world(viewport: Vec2, world: Vec2) -> Self {
        Camera::new(world / 2.0, Camera::fit_zoom(viewport, world))
    }

    pub fn showing(things: &[Rect], viewport: Vec2, margin: f64, world: Vec2) -> Self {
        let Some(first) = things.first() else {
            return Camera::fitting_world(viewport, world);
        };

        let mut low = first.origin;
        let mut high = first.max();
        for thing in things.iter().skip(1) {
            low = low.min(thing.origin);
            high = high.max(thing.max());
        }

        let span = (high - low) + Vec2::splat(margin * 2.0);
        let least = Camera::fit_zoom(viewport, world);
        let zoom = Camera::fit_zoom(viewport, span).clamp(least, Camera::WORK_ZOOM);

        let mut camera = Camera::new((low + high) / 2.0, zoom);
        camera.clamp_to_world(world, viewport);
        camera
    }

    pub fn zoom_at(&mut self, screen: Vec2, zoom: f64, viewport: Vec2) {
        let anchor = self.world_from_screen(screen, viewport);
        self.zoom = zoom;
        self.center = anchor - (screen - viewport / 2.0) / zoom;
    }

    pub fn pan_by_screen(&mut self, delta: Vec2) {
        self.center = self.center + delta / self.zoom;
    }

    pub fn center_on(&mut self, target: Rect) {
        self.center = target.center();
    }

    pub fn clamp_to_world(&mut self, world: Vec2, viewport: Vec2) {
        let visible = self.visible_size(viewport);
        self.center = Vec2::new(
            clamp_axis(self.center.x, visible.x, world.x),
            clamp_axis(self.center.y, visible.y, world.y),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flight {
    pub from: Camera,
    pub to: Camera,
    pub span: std::time::Duration,
}

impl Flight {
    pub fn new(from: Camera, to: Camera, span: std::time::Duration) -> Self {
        Flight { from, to, span }
    }

    pub fn progress(&self, gone: std::time::Duration) -> f64 {
        if self.span.is_zero() {
            return 1.0;
        }
        (gone.as_secs_f64() / self.span.as_secs_f64()).clamp(0.0, 1.0)
    }

    pub fn done(&self, gone: std::time::Duration) -> bool {
        self.progress(gone) >= 1.0
    }

    pub fn at(&self, gone: std::time::Duration) -> Camera {
        let t = crate::drawer::ease(self.progress(gone));
        let center = self.from.center + (self.to.center - self.from.center) * t;
        let zoom = glide(self.from.zoom, self.to.zoom, t.clamp(0.0, 1.0));
        Camera::new(center, zoom)
    }

    pub fn within(&self, gone: std::time::Duration, world: Vec2, viewport: Vec2) -> Camera {
        let mut camera = self.at(gone);
        camera.clamp_to_world(world, viewport);
        camera
    }
}

fn glide(from: f64, to: f64, t: f64) -> f64 {
    if from <= 0.0 || to <= 0.0 {
        return from + (to - from) * t;
    }
    (from.ln() + (to.ln() - from.ln()) * t).exp()
}

fn clamp_axis(center: f64, visible: f64, world: f64) -> f64 {
    if visible >= world {
        world / 2.0
    } else {
        let half = visible / 2.0;
        center.clamp(half, world - half)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const VIEWPORT: Vec2 = Vec2::new(1920.0, 1080.0);

    #[test]
    fn a_flight_in_never_zooms_past_its_target() {
        let from = Camera::fitting_world(VIEWPORT, WORLD);
        let to = Camera::at_work_zoom(WORLD / 2.0);
        let flight = Flight::new(from, to, Duration::from_millis(400));
        for step in 0..=400 {
            let camera = flight.at(Duration::from_millis(step));
            assert!(
                camera.zoom <= to.zoom + 1e-9,
                "step {step} zoom {}",
                camera.zoom
            );
        }
    }

    #[test]
    fn a_flight_out_never_zooms_past_the_world() {
        let from = Camera::at_work_zoom(WORLD / 2.0);
        let to = Camera::fitting_world(VIEWPORT, WORLD);
        let flight = Flight::new(from, to, Duration::from_millis(400));
        for step in 0..=400 {
            let camera = flight.at(Duration::from_millis(step));
            assert!(
                camera.zoom >= to.zoom - 1e-9,
                "step {step} zoom {}",
                camera.zoom
            );
        }
    }

    #[test]
    fn a_flight_to_the_world_edge_never_shows_outside_it() {
        let from = Camera::at_work_zoom(WORLD / 2.0);
        let mut to = Camera::at_work_zoom(Vec2::ZERO);
        to.clamp_to_world(WORLD, VIEWPORT);
        let flight = Flight::new(from, to, Duration::from_millis(400));
        for step in 0..=400 {
            let seen = flight
                .within(Duration::from_millis(step), WORLD, VIEWPORT)
                .visible_rect(VIEWPORT);
            assert!(
                seen.origin.x >= -1e-9 && seen.origin.y >= -1e-9,
                "step {step}"
            );
            assert!(seen.max().x <= WORLD.x + 1e-9 && seen.max().y <= WORLD.y + 1e-9);
        }
    }

    #[test]
    fn showing_nothing_falls_back_to_the_whole_world() {
        let camera = Camera::showing(&[], VIEWPORT, 40.0, WORLD);
        assert_eq!(camera, Camera::fitting_world(VIEWPORT, WORLD));
    }

    #[test]
    fn showing_one_small_window_never_zooms_past_the_work_zoom() {
        let window = Rect::new(Vec2::new(2800.0, 1600.0), Vec2::new(200.0, 150.0));
        let camera = Camera::showing(&[window], VIEWPORT, 40.0, WORLD);
        assert_eq!(camera.zoom, Camera::WORK_ZOOM);
    }

    #[test]
    fn showing_centers_on_the_windows_not_on_the_world() {
        let one = Rect::new(Vec2::new(400.0, 300.0), Vec2::new(600.0, 400.0));
        let two = Rect::new(Vec2::new(1200.0, 900.0), Vec2::new(600.0, 400.0));
        let camera = Camera::showing(&[one, two], VIEWPORT, 0.0, WORLD);
        let middle = (one.origin + two.max()) / 2.0;
        assert_eq!(camera.center, middle);
    }

    #[test]
    fn showing_zooms_out_far_enough_to_hold_every_window() {
        let one = Rect::new(Vec2::new(200.0, 200.0), Vec2::new(800.0, 600.0));
        let two = Rect::new(Vec2::new(3600.0, 2200.0), Vec2::new(800.0, 600.0));
        let camera = Camera::showing(&[one, two], VIEWPORT, 0.0, WORLD);
        let seen = camera.visible_rect(VIEWPORT);
        assert!(seen.origin.x <= one.origin.x + 1.0);
        assert!(seen.origin.y <= one.origin.y + 1.0);
        assert!(seen.max().x >= two.max().x - 1.0);
        assert!(seen.max().y >= two.max().y - 1.0);
    }

    #[test]
    fn showing_never_zooms_out_past_the_world() {
        let huge = Rect::new(Vec2::new(-9000.0, -9000.0), Vec2::new(30000.0, 30000.0));
        let camera = Camera::showing(&[huge], VIEWPORT, 0.0, WORLD);
        assert_eq!(camera.zoom, Camera::fit_zoom(VIEWPORT, WORLD));
    }

    #[test]
    fn the_margin_pushes_the_windows_off_the_very_edge() {
        let window = Rect::new(Vec2::new(0.0, 0.0), Vec2::new(3840.0, 2160.0));
        let tight = Camera::showing(&[window], VIEWPORT, 0.0, WORLD);
        let roomy = Camera::showing(&[window], VIEWPORT, 200.0, WORLD);
        assert!(roomy.zoom < tight.zoom);
    }

    #[test]
    fn a_flight_starts_where_it_was_and_lands_where_it_was_sent() {
        let from = Camera::at_work_zoom(Vec2::new(500.0, 400.0));
        let to = Camera::new(Vec2::new(2000.0, 1500.0), 0.3333);
        let flight = Flight::new(from, to, Duration::from_millis(260));

        let start = flight.at(Duration::ZERO);
        assert!((start.center - from.center).length() < 1e-9);
        assert!((start.zoom - from.zoom).abs() < 1e-9);

        let end = flight.at(Duration::from_millis(260));
        assert!((end.center - to.center).length() < 1e-9);
        assert!((end.zoom - to.zoom).abs() < 1e-9);
    }

    #[test]
    fn a_flight_never_passes_the_time_it_was_given() {
        let flight = Flight::new(
            Camera::at_work_zoom(Vec2::ZERO),
            Camera::at_work_zoom(Vec2::new(100.0, 0.0)),
            Duration::from_millis(200),
        );
        assert!(flight.done(Duration::from_millis(200)));
        assert!(!flight.done(Duration::from_millis(199)));
        let late = flight.at(Duration::from_secs(9));
        assert_eq!(late.center, Vec2::new(100.0, 0.0));
    }

    #[test]
    fn zoom_glides_evenly_rather_than_in_a_straight_line() {
        let middle = glide(0.25, 1.0, 0.5);
        let straight = (0.25 + 1.0) / 2.0;
        assert!(
            middle < straight,
            "zoom is mixed in a straight line, so half way out looks nearly all the way out"
        );
        assert!(
            (middle - 0.5).abs() < 1e-9,
            "half way should double the zoom"
        );
    }

    #[test]
    fn the_camera_springs_rather_than_easing_flat() {
        let flight = Flight::new(
            Camera::at_work_zoom(Vec2::ZERO),
            Camera::at_work_zoom(Vec2::new(100.0, 0.0)),
            Duration::from_millis(200),
        );
        let peak = (0..=100)
            .map(|step| flight.at(Duration::from_millis(step * 2)).center.x)
            .fold(0.0f64, f64::max);
        assert!(
            peak > 100.0,
            "the camera never overshoots, so it is not elastic"
        );
    }

    #[test]
    fn a_flight_of_no_time_is_already_over() {
        let flight = Flight::new(
            Camera::at_work_zoom(Vec2::ZERO),
            Camera::at_work_zoom(Vec2::new(5.0, 5.0)),
            Duration::ZERO,
        );
        assert!(flight.done(Duration::ZERO));
        assert_eq!(flight.at(Duration::ZERO).center, Vec2::new(5.0, 5.0));
    }
    const WORLD: Vec2 = Vec2::new(7680.0, 6480.0);

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn screen_and_world_round_trip() {
        let cam = Camera::new(Vec2::new(3000.0, 2000.0), 0.37);
        let world = Vec2::new(1234.5, 6789.0);
        let back = cam.world_from_screen(cam.screen_from_world(world, VIEWPORT), VIEWPORT);
        assert!(close(back.x, world.x) && close(back.y, world.y));
    }

    #[test]
    fn work_zoom_maps_one_to_one() {
        let cam = Camera::at_work_zoom(VIEWPORT / 2.0);
        assert_eq!(cam.screen_from_world(Vec2::ZERO, VIEWPORT), Vec2::ZERO);
        assert_eq!(cam.visible_size(VIEWPORT), VIEWPORT);
    }

    #[test]
    fn fit_zoom_shows_the_whole_world() {
        let cam = Camera::fitting_world(VIEWPORT, WORLD);
        let visible = cam.visible_size(VIEWPORT);
        assert!(visible.x >= WORLD.x - 1e-9);
        assert!(visible.y >= WORLD.y - 1e-9);
    }

    #[test]
    fn zoom_at_holds_the_point_under_the_cursor() {
        let mut cam = Camera::at_work_zoom(Vec2::new(3000.0, 2000.0));
        let cursor = Vec2::new(400.0, 900.0);
        let before = cam.world_from_screen(cursor, VIEWPORT);
        cam.zoom_at(cursor, 0.25, VIEWPORT);
        let after = cam.world_from_screen(cursor, VIEWPORT);
        assert!(close(before.x, after.x) && close(before.y, after.y));
    }

    #[test]
    fn clamp_keeps_the_visible_rect_inside_the_world() {
        let mut cam = Camera::at_work_zoom(Vec2::new(-5000.0, 99999.0));
        cam.clamp_to_world(WORLD, VIEWPORT);
        let rect = cam.visible_rect(VIEWPORT);
        assert!(rect.origin.x >= -1e-9 && rect.origin.y >= -1e-9);
        assert!(rect.max().x <= WORLD.x + 1e-9);
        assert!(rect.max().y <= WORLD.y + 1e-9);
    }

    #[test]
    fn clamp_centers_the_world_when_zoomed_out_past_fit() {
        let mut cam = Camera::new(Vec2::ZERO, Camera::fit_zoom(VIEWPORT, WORLD) / 2.0);
        cam.clamp_to_world(WORLD, VIEWPORT);
        assert_eq!(cam.center, WORLD / 2.0);
    }

    #[test]
    fn pan_by_screen_scales_with_zoom() {
        let mut cam = Camera::new(Vec2::ZERO, 0.5);
        cam.pan_by_screen(Vec2::new(100.0, 0.0));
        assert_eq!(cam.center, Vec2::new(200.0, 0.0));
    }

    #[test]
    fn center_on_puts_a_window_in_the_middle() {
        let mut cam = Camera::at_work_zoom(Vec2::ZERO);
        let window = Rect::new(Vec2::new(5000.0, 4000.0), Vec2::new(800.0, 600.0));
        cam.center_on(window);
        let on_screen = cam.screen_from_world(window.center(), VIEWPORT);
        assert_eq!(on_screen, VIEWPORT / 2.0);
    }
}
