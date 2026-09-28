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

    const VIEWPORT: Vec2 = Vec2::new(1920.0, 1080.0);
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
