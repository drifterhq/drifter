use std::time::Duration;

use crate::geom::{Rect, Vec2};
use crate::image::Image;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Top,
    Bottom,
}

impl Edge {
    pub const ALL: [Edge; 2] = [Edge::Top, Edge::Bottom];

    pub fn index(self) -> usize {
        match self {
            Edge::Top => 0,
            Edge::Bottom => 1,
        }
    }

    fn deeper(self) -> f64 {
        match self {
            Edge::Top => 1.0,
            Edge::Bottom => -1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Panel {
    pub height: f64,
    pub radius: f64,
    pub shadow: f64,
    pub slack: f64,
}

impl Default for Panel {
    fn default() -> Self {
        Panel {
            height: 64.0,
            radius: 10.0,
            shadow: 16.0,
            slack: 16.0,
        }
    }
}

impl Panel {
    pub fn texture_height(&self) -> f64 {
        self.slack + self.height + self.radius
    }

    pub fn texture_size(&self, width: f64) -> Vec2 {
        Vec2::new(width, self.texture_height())
    }

    pub fn inner_row(&self, edge: Edge) -> f64 {
        match edge {
            Edge::Top => self.slack + self.height,
            Edge::Bottom => self.radius,
        }
    }

    pub fn travel(&self) -> f64 {
        self.height + self.radius
    }
}

const OVERSHOOT: f64 = 1.2;

pub fn ease(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    let c1 = OVERSHOOT;
    let c3 = c1 + 1.0;
    let back = t - 1.0;
    1.0 + c3 * back * back * back + c1 * back * back
}

pub fn edge_at(pointer: Vec2, screen: Vec2, band: f64) -> Option<Edge> {
    if pointer.y < band {
        Some(Edge::Top)
    } else if pointer.y > screen.y - band {
        Some(Edge::Bottom)
    } else {
        None
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Drawer {
    pub edge: Edge,
    pub open: bool,
    progress: f64,
}

impl Drawer {
    pub fn new(edge: Edge) -> Self {
        Drawer {
            edge,
            open: false,
            progress: 0.0,
        }
    }

    pub fn progress(&self) -> f64 {
        self.progress
    }

    pub fn visible(&self) -> bool {
        self.progress > 0.0
    }

    pub fn settled(&self) -> bool {
        self.progress == if self.open { 1.0 } else { 0.0 }
    }

    pub fn advance(&mut self, dt: Duration, anim: Duration) -> bool {
        if self.settled() {
            return false;
        }
        let step = if anim.is_zero() {
            1.0
        } else {
            dt.as_secs_f64() / anim.as_secs_f64()
        };
        self.progress = if self.open {
            (self.progress + step).min(1.0)
        } else {
            (self.progress - step).max(0.0)
        };
        true
    }

    pub fn reveal(&self, panel: &Panel) -> f64 {
        ease(self.progress) * panel.travel()
    }

    pub fn rect(&self, screen: Vec2, panel: &Panel) -> Rect {
        let size = panel.texture_size(screen.x);
        let reveal = self.reveal(panel);
        let y = match self.edge {
            Edge::Top => reveal - size.y,
            Edge::Bottom => screen.y - reveal,
        };
        Rect::new(Vec2::new(0.0, y), size)
    }

    pub fn band(&self, screen: Vec2, panel: &Panel) -> Rect {
        let rect = self.rect(screen, panel);
        let straight = rect.origin.y + panel.inner_row(self.edge);
        let y = match self.edge {
            Edge::Top => straight - panel.height,
            Edge::Bottom => straight,
        };
        Rect::new(Vec2::new(0.0, y), Vec2::new(screen.x, panel.height))
    }

    pub fn holds(&self, pointer: Vec2, screen: Vec2, panel: &Panel) -> bool {
        self.open && self.rect(screen, panel).contains(pointer)
    }
}

fn depth(edge: Edge, px: f64, py: f64, width: f64, panel: &Panel) -> f64 {
    let inner = panel.inner_row(edge);
    let side = edge.deeper();
    let radius = panel.radius;

    if radius > 0.0 && (px < radius || px > width - radius) {
        let cx = if px < radius { radius } else { width - radius };
        let cy = inner + side * radius;
        if side * (py - cy) <= 0.0 {
            let dx = px - cx;
            let dy = py - cy;
            return (dx * dx + dy * dy).sqrt() - radius;
        }
    }

    side * (inner - py)
}

fn shade(depth: f64, shadow: f64) -> f64 {
    if shadow <= 0.0 || depth >= shadow {
        return 1.0;
    }
    let sink = 1.0 - (depth.max(0.0) / shadow);
    1.0 - 0.55 * sink * sink
}

pub fn paint(edge: Edge, width: u32, panel: &Panel, color: [u8; 3], alpha: u8) -> Image {
    let height = panel.texture_height().round().max(1.0) as u32;
    let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
    let a = f64::from(alpha) / 255.0;
    let w = f64::from(width);

    for y in 0..height {
        let py = f64::from(y) + 0.5;
        for x in 0..width {
            let px = f64::from(x) + 0.5;
            let into_panel = depth(edge, px, py, w, panel);
            let coverage = (into_panel + 0.5).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }

            let lit = shade(into_panel, panel.shadow);
            let pixel_alpha = a * coverage;
            let index = ((y as usize) * (width as usize) + x as usize) * 4;
            for (channel, value) in color.iter().enumerate() {
                rgba[index + channel] = (f64::from(*value) * lit * pixel_alpha)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
            rgba[index + 3] = (pixel_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }

    Image {
        width,
        height,
        rgba,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Vec2 = Vec2::new(1920.0, 1080.0);
    const WIDTH: u32 = 400;

    fn panel() -> Panel {
        Panel::default()
    }

    fn opened(edge: Edge) -> Drawer {
        let mut drawer = Drawer::new(edge);
        drawer.open = true;
        drawer.advance(Duration::from_secs(1), Duration::from_millis(300));
        drawer
    }

    fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let index = ((y as usize) * (image.width as usize) + x as usize) * 4;
        [
            image.rgba[index],
            image.rgba[index + 1],
            image.rgba[index + 2],
            image.rgba[index + 3],
        ]
    }

    fn on_screen(image: &Image, rect: Rect, point: Vec2) -> [u8; 4] {
        let x = (point.x - rect.origin.x).floor();
        let y = (point.y - rect.origin.y).floor();
        assert!(
            x >= 0.0 && y >= 0.0 && x < f64::from(image.width) && y < f64::from(image.height),
            "{point:?} is outside the drawer texture"
        );
        pixel(image, x as u32, y as u32)
    }

    #[test]
    fn ease_starts_at_zero_and_ends_at_one() {
        assert!(ease(0.0).abs() < 1e-9);
        assert!((ease(1.0) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn ease_overshoots_so_the_drawer_springs() {
        let peak = (0..=100)
            .map(|i| ease(f64::from(i) / 100.0))
            .fold(0.0f64, f64::max);
        assert!(peak > 1.0, "ease never goes past 1, so there is no spring");
    }

    #[test]
    fn each_edge_has_its_own_slot() {
        assert_ne!(Edge::Top.index(), Edge::Bottom.index());
        assert!(Edge::ALL.iter().all(|edge| edge.index() < Edge::ALL.len()));
    }

    #[test]
    fn a_closed_drawer_is_completely_off_screen() {
        for edge in Edge::ALL {
            let rect = Drawer::new(edge).rect(SCREEN, &panel());
            match edge {
                Edge::Top => assert!(rect.max().y <= 0.0),
                Edge::Bottom => assert!(rect.origin.y >= SCREEN.y),
            }
        }
    }

    #[test]
    fn an_open_drawer_puts_its_straight_edge_flush_against_the_screen_edge() {
        let panel = panel();
        for edge in Edge::ALL {
            let rect = opened(edge).rect(SCREEN, &panel);
            let straight = rect.origin.y + panel.inner_row(edge);
            match edge {
                Edge::Top => assert!((straight - panel.height).abs() < 1e-9),
                Edge::Bottom => assert!((straight - (SCREEN.y - panel.height)).abs() < 1e-9),
            }
        }
    }

    #[test]
    fn the_spring_never_opens_a_gap_at_the_edge() {
        let panel = panel();
        let mut drawer = Drawer::new(Edge::Top);
        drawer.open = true;
        for _ in 0..200 {
            drawer.advance(Duration::from_millis(4), Duration::from_millis(300));
            let rect = drawer.rect(SCREEN, &panel);
            assert!(
                rect.origin.y <= 0.0,
                "wallpaper showed above the top drawer at progress {}",
                drawer.progress()
            );
        }
    }

    #[test]
    fn the_slack_covers_the_whole_overshoot() {
        let panel = panel();
        let peak = (0..=1000)
            .map(|i| ease(f64::from(i) / 1000.0))
            .fold(0.0f64, f64::max);
        assert!((peak - 1.0) * panel.travel() <= panel.slack);
    }

    #[test]
    fn the_drawer_takes_the_animation_time_to_open() {
        let mut drawer = Drawer::new(Edge::Top);
        drawer.open = true;
        let anim = Duration::from_millis(300);
        for _ in 0..15 {
            drawer.advance(Duration::from_millis(20), anim);
        }
        assert!((drawer.progress() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_settled_drawer_reports_no_change() {
        let mut drawer = Drawer::new(Edge::Top);
        assert!(!drawer.advance(Duration::from_millis(20), Duration::from_millis(300)));
        drawer.open = true;
        assert!(drawer.advance(Duration::from_millis(20), Duration::from_millis(300)));
    }

    #[test]
    fn closing_returns_to_exactly_zero() {
        let mut drawer = opened(Edge::Bottom);
        drawer.open = false;
        drawer.advance(Duration::from_secs(1), Duration::from_millis(300));
        assert_eq!(drawer.progress(), 0.0);
        assert!(!drawer.visible());
    }

    #[test]
    fn the_pointer_finds_the_top_and_bottom_edges() {
        assert_eq!(edge_at(Vec2::new(900.0, 0.0), SCREEN, 2.0), Some(Edge::Top));
        assert_eq!(
            edge_at(Vec2::new(900.0, 1079.5), SCREEN, 2.0),
            Some(Edge::Bottom)
        );
        assert_eq!(edge_at(Vec2::new(900.0, 540.0), SCREEN, 2.0), None);
    }

    #[test]
    fn an_open_drawer_holds_the_pointer_that_rests_on_it() {
        let panel = panel();
        let drawer = opened(Edge::Top);
        assert!(drawer.holds(Vec2::new(900.0, 30.0), SCREEN, &panel));
        assert!(!drawer.holds(Vec2::new(900.0, 500.0), SCREEN, &panel));
    }

    #[test]
    fn a_closed_drawer_holds_nothing() {
        let drawer = Drawer::new(Edge::Top);
        assert!(!drawer.holds(Vec2::new(900.0, 0.0), SCREEN, &panel()));
    }

    #[test]
    fn the_band_of_an_open_drawer_is_the_strip_it_covers() {
        let panel = panel();
        let top = opened(Edge::Top).band(SCREEN, &panel);
        assert_eq!(top.origin, Vec2::ZERO);
        assert_eq!(top.size, Vec2::new(SCREEN.x, panel.height));

        let bottom = opened(Edge::Bottom).band(SCREEN, &panel);
        assert_eq!(bottom.origin, Vec2::new(0.0, SCREEN.y - panel.height));
        assert_eq!(bottom.size, Vec2::new(SCREEN.x, panel.height));
    }

    #[test]
    fn the_band_rides_in_with_the_drawer() {
        let panel = panel();
        let mut drawer = Drawer::new(Edge::Top);
        drawer.open = true;
        drawer.advance(Duration::from_millis(60), Duration::from_millis(300));
        let band = drawer.band(SCREEN, &panel);
        assert!(
            band.max().y < panel.height,
            "the band did not follow the slide"
        );
        assert!(band.max().y > 0.0);
    }

    #[test]
    fn the_panel_covers_its_whole_band_on_screen() {
        let panel = panel();
        let screen = Vec2::new(f64::from(WIDTH), SCREEN.y);
        for edge in Edge::ALL {
            let image = paint(edge, WIDTH, &panel, [16, 18, 26], 255);
            let rect = opened(edge).rect(screen, &panel);
            let middle = f64::from(WIDTH) / 2.0;
            let band = match edge {
                Edge::Top => [1.0, panel.height / 2.0, panel.height - 1.0],
                Edge::Bottom => [
                    screen.y - panel.height + 1.0,
                    screen.y - panel.height / 2.0,
                    screen.y - 1.0,
                ],
            };
            for y in band {
                assert_eq!(
                    on_screen(&image, rect, Vec2::new(middle, y))[3],
                    255,
                    "{edge:?} drawer has a hole at y {y}"
                );
            }
        }
    }

    #[test]
    fn nothing_is_drawn_past_the_drawer_except_the_rounded_corners() {
        let panel = panel();
        let screen = Vec2::new(f64::from(WIDTH), SCREEN.y);
        let image = paint(Edge::Top, WIDTH, &panel, [16, 18, 26], 255);
        let rect = opened(Edge::Top).rect(screen, &panel);
        let past = panel.height + panel.radius / 2.0;
        assert_eq!(
            on_screen(&image, rect, Vec2::new(f64::from(WIDTH) / 2.0, past))[3],
            0,
            "there is an extra box under the drawer"
        );
    }

    #[test]
    fn the_rounded_corner_belongs_to_the_viewport_not_the_drawer() {
        let panel = panel();
        let screen = Vec2::new(f64::from(WIDTH), SCREEN.y);
        for edge in Edge::ALL {
            let image = paint(edge, WIDTH, &panel, [16, 18, 26], 255);
            let rect = opened(edge).rect(screen, &panel);
            let just_past = match edge {
                Edge::Top => panel.height + 1.0,
                Edge::Bottom => screen.y - panel.height - 1.0,
            };

            let corner = on_screen(&image, rect, Vec2::new(0.5, just_past));
            assert_eq!(
                corner[3], 255,
                "{edge:?}: the screen corner is not filled, so the drawer is rounded \
                 instead of the viewport"
            );

            let inward = on_screen(&image, rect, Vec2::new(panel.radius * 2.0, just_past));
            assert_eq!(
                inward[3], 0,
                "{edge:?}: the drawer bleeds across the whole width past its edge"
            );

            let far_corner = on_screen(&image, rect, Vec2::new(f64::from(WIDTH) - 0.5, just_past));
            assert_eq!(far_corner[3], 255, "{edge:?}: only one corner is filled");
        }
    }

    #[test]
    fn the_inset_shadow_is_darkest_at_the_straight_edge() {
        let panel = panel();
        let screen = Vec2::new(f64::from(WIDTH), SCREEN.y);
        let image = paint(Edge::Top, WIDTH, &panel, [180, 180, 180], 255);
        let rect = opened(Edge::Top).rect(screen, &panel);
        let middle = f64::from(WIDTH) / 2.0;
        let at_edge = on_screen(&image, rect, Vec2::new(middle, panel.height - 2.0))[0];
        let inside = on_screen(
            &image,
            rect,
            Vec2::new(middle, panel.height - panel.shadow - 4.0),
        )[0];
        assert!(
            at_edge < inside,
            "the inner edge is not darker than the panel body"
        );
    }

    #[test]
    fn the_shadow_sits_on_the_wallpaper_side_for_both_edges() {
        let panel = panel();
        let screen = Vec2::new(f64::from(WIDTH), SCREEN.y);
        let image = paint(Edge::Bottom, WIDTH, &panel, [180, 180, 180], 255);
        let rect = opened(Edge::Bottom).rect(screen, &panel);
        let middle = f64::from(WIDTH) / 2.0;
        let at_edge = on_screen(
            &image,
            rect,
            Vec2::new(middle, screen.y - panel.height + 2.0),
        )[0];
        let inside = on_screen(
            &image,
            rect,
            Vec2::new(middle, screen.y - panel.height + panel.shadow + 4.0),
        )[0];
        assert!(at_edge < inside);
    }

    #[test]
    fn the_shadow_only_reaches_as_far_as_its_width() {
        let panel = panel();
        let screen = Vec2::new(f64::from(WIDTH), SCREEN.y);
        let image = paint(Edge::Top, WIDTH, &panel, [180, 180, 180], 255);
        let rect = opened(Edge::Top).rect(screen, &panel);
        let middle = f64::from(WIDTH) / 2.0;
        let outside = on_screen(
            &image,
            rect,
            Vec2::new(middle, panel.height - panel.shadow - 2.0),
        )[0];
        let far = on_screen(&image, rect, Vec2::new(middle, 2.0))[0];
        assert_eq!(outside, far);
    }

    #[test]
    fn every_pixel_is_premultiplied() {
        let image = paint(Edge::Top, 64, &Panel::default(), [255, 255, 255], 128);
        for pixel in image.rgba.as_chunks::<4>().0 {
            let a = pixel[3];
            assert!(pixel[0] <= a && pixel[1] <= a && pixel[2] <= a);
        }
    }

    #[test]
    fn the_texture_holds_the_panel_the_slack_and_the_corners() {
        let panel = panel();
        let image = paint(Edge::Top, 10, &panel, [0, 0, 0], 255);
        assert_eq!(
            f64::from(image.height),
            panel.height + panel.slack + panel.radius
        );
        assert_eq!(panel.texture_size(1920.0).x, 1920.0);
    }
}
