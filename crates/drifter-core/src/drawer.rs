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
            radius: 12.0,
            shadow: 16.0,
            slack: 16.0,
        }
    }
}

impl Panel {
    pub fn texture_size(&self, width: f64) -> Vec2 {
        Vec2::new(width, self.height + self.slack)
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
        ease(self.progress) * panel.height
    }

    pub fn holds(&self, pointer: Vec2, screen: Vec2, panel: &Panel) -> bool {
        self.open && self.rect(screen, panel).contains(pointer)
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
}

pub fn paint(edge: Edge, width: u32, panel: &Panel, color: [u8; 3], alpha: u8) -> Image {
    let height = (panel.height + panel.slack).round().max(1.0) as u32;
    let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
    let a = f64::from(alpha) / 255.0;
    let w = f64::from(width);
    let inner_edge = match edge {
        Edge::Top => panel.height,
        Edge::Bottom => panel.slack,
    };

    for y in 0..height {
        let py = f64::from(y) + 0.5;
        let from_inner = match edge {
            Edge::Top => inner_edge - py,
            Edge::Bottom => py - inner_edge,
        };
        for x in 0..width {
            let px = f64::from(x) + 0.5;
            let coverage = corner_coverage(px, py, w, panel, edge);
            if coverage <= 0.0 {
                continue;
            }

            let shade = shadow_shade(from_inner, panel.shadow);
            let pixel_alpha = a * coverage;
            let index = ((y as usize) * (width as usize) + x as usize) * 4;
            for (channel, value) in color.iter().enumerate() {
                let lit = f64::from(*value) * shade;
                rgba[index + channel] = (lit * pixel_alpha).round().clamp(0.0, 255.0) as u8;
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

fn shadow_shade(from_inner: f64, shadow: f64) -> f64 {
    if shadow <= 0.0 || from_inner < 0.0 || from_inner >= shadow {
        return 1.0;
    }
    let depth = 1.0 - from_inner / shadow;
    1.0 - 0.55 * depth * depth
}

fn corner_coverage(px: f64, py: f64, width: f64, panel: &Panel, edge: Edge) -> f64 {
    let radius = panel.radius;
    if radius <= 0.0 {
        return 1.0;
    }

    let inner_edge = match edge {
        Edge::Top => panel.height,
        Edge::Bottom => panel.slack,
    };
    let center_y = match edge {
        Edge::Top => inner_edge - radius,
        Edge::Bottom => inner_edge + radius,
    };

    let past_corner = match edge {
        Edge::Top => py > center_y,
        Edge::Bottom => py < center_y,
    };
    if !past_corner {
        return 1.0;
    }

    let center_x = if px < radius {
        radius
    } else if px > width - radius {
        width - radius
    } else {
        return 1.0;
    };

    let dx = px - center_x;
    let dy = py - center_y;
    let distance = (dx * dx + dy * dy).sqrt();
    (radius - distance + 0.5).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Vec2 = Vec2::new(1920.0, 1080.0);

    fn panel() -> Panel {
        Panel::default()
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
    fn an_open_drawer_sits_flush_against_its_edge() {
        let panel = panel();
        for edge in Edge::ALL {
            let mut drawer = Drawer::new(edge);
            drawer.open = true;
            drawer.advance(Duration::from_secs(1), Duration::from_millis(260));
            let rect = drawer.rect(SCREEN, &panel);
            match edge {
                Edge::Top => assert!((rect.max().y - panel.height).abs() < 1e-9),
                Edge::Bottom => {
                    assert!((rect.origin.y - (SCREEN.y - panel.height)).abs() < 1e-9);
                }
            }
        }
    }

    #[test]
    fn the_spring_never_opens_a_gap_at_the_edge() {
        let panel = panel();
        let mut drawer = Drawer::new(Edge::Top);
        drawer.open = true;
        for _ in 0..200 {
            drawer.advance(Duration::from_millis(4), Duration::from_millis(260));
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
        assert!((peak - 1.0) * panel.height <= panel.slack);
    }

    #[test]
    fn the_drawer_takes_the_animation_time_to_open() {
        let mut drawer = Drawer::new(Edge::Top);
        drawer.open = true;
        let anim = Duration::from_millis(260);
        for _ in 0..13 {
            drawer.advance(Duration::from_millis(20), anim);
        }
        assert!((drawer.progress() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_settled_drawer_reports_no_change() {
        let mut drawer = Drawer::new(Edge::Top);
        assert!(!drawer.advance(Duration::from_millis(20), Duration::from_millis(260)));
        drawer.open = true;
        assert!(drawer.advance(Duration::from_millis(20), Duration::from_millis(260)));
    }

    #[test]
    fn closing_returns_to_exactly_zero() {
        let mut drawer = Drawer::new(Edge::Bottom);
        drawer.open = true;
        drawer.advance(Duration::from_secs(1), Duration::from_millis(260));
        drawer.open = false;
        drawer.advance(Duration::from_secs(1), Duration::from_millis(260));
        assert_eq!(drawer.progress(), 0.0);
        assert!(!drawer.visible());
    }

    #[test]
    fn each_edge_has_its_own_slot() {
        assert_ne!(Edge::Top.index(), Edge::Bottom.index());
        assert!(Edge::ALL.iter().all(|edge| edge.index() < Edge::ALL.len()));
    }

    #[test]
    fn an_open_drawer_holds_the_pointer_that_rests_on_it() {
        let panel = panel();
        let mut drawer = Drawer::new(Edge::Top);
        drawer.open = true;
        drawer.advance(Duration::from_secs(1), Duration::from_millis(260));
        assert!(drawer.holds(Vec2::new(900.0, 30.0), SCREEN, &panel));
        assert!(!drawer.holds(Vec2::new(900.0, 500.0), SCREEN, &panel));
    }

    #[test]
    fn a_closed_drawer_holds_nothing() {
        let panel = panel();
        let drawer = Drawer::new(Edge::Top);
        assert!(!drawer.holds(Vec2::new(900.0, 0.0), SCREEN, &panel));
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
    fn the_inner_corners_are_transparent_and_the_outer_ones_are_not() {
        let panel = panel();
        let top = paint(Edge::Top, 200, &panel, [18, 20, 28], 235);
        let inner = (panel.height - 1.0) as u32;
        assert_eq!(pixel(&top, 0, inner)[3], 0);
        assert!(pixel(&top, 0, 0)[3] > 0);

        let bottom = paint(Edge::Bottom, 200, &panel, [18, 20, 28], 235);
        let inner = panel.slack as u32;
        assert_eq!(pixel(&bottom, 0, inner)[3], 0);
        assert!(pixel(&bottom, 0, bottom.height - 1)[3] > 0);
    }

    #[test]
    fn the_middle_of_the_panel_is_solid() {
        let panel = panel();
        let image = paint(Edge::Top, 200, &panel, [18, 20, 28], 255);
        assert_eq!(pixel(&image, 100, 40)[3], 255);
    }

    #[test]
    fn the_inset_shadow_is_darkest_at_the_inner_edge() {
        let panel = panel();
        let image = paint(Edge::Top, 200, &panel, [180, 180, 180], 255);
        let at_edge = pixel(&image, 100, (panel.height - 2.0) as u32)[0];
        let inside = pixel(&image, 100, (panel.height - panel.shadow - 2.0) as u32)[0];
        assert!(
            at_edge < inside,
            "the inner edge is not darker than the panel body"
        );
    }

    #[test]
    fn the_shadow_only_reaches_as_far_as_its_width() {
        let panel = panel();
        let image = paint(Edge::Top, 200, &panel, [180, 180, 180], 255);
        let outside = pixel(&image, 100, (panel.height - panel.shadow - 1.0) as u32)[0];
        let far = pixel(&image, 100, 4)[0];
        assert_eq!(outside, far);
    }

    #[test]
    fn the_shadow_sits_on_the_wallpaper_side_for_both_edges() {
        let panel = panel();
        let image = paint(Edge::Bottom, 200, &panel, [180, 180, 180], 255);
        let at_edge = pixel(&image, 100, (panel.slack + 1.0) as u32)[0];
        let inside = pixel(&image, 100, (panel.slack + panel.shadow + 2.0) as u32)[0];
        assert!(at_edge < inside);
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
    fn the_texture_is_the_panel_plus_the_slack() {
        let panel = panel();
        let image = paint(Edge::Top, 10, &panel, [0, 0, 0], 255);
        assert_eq!(f64::from(image.height), panel.height + panel.slack);
        assert_eq!(panel.texture_size(1920.0), Vec2::new(1920.0, 80.0));
    }
}
