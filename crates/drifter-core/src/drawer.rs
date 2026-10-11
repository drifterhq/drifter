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
    pub span: f64,
}

impl Default for Panel {
    fn default() -> Self {
        Panel {
            height: 36.0,
            radius: 10.0,
            shadow: 28.0,
            slack: 16.0,
            span: 0.0,
        }
    }
}

impl Panel {
    pub fn width(&self, screen: f64) -> f64 {
        match self.span > 0.0 {
            true => self.span.min(screen),
            false => screen,
        }
    }

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

pub fn edge_at(pointer: Vec2, screen: Vec2, band: f64, corner: f64) -> Option<Edge> {
    if pointer.x < corner && pointer.y < corner {
        return None;
    }
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
        let size = panel.texture_size(panel.width(screen.x));
        let reveal = self.reveal(panel);
        let y = match self.edge {
            Edge::Top => reveal - size.y,
            Edge::Bottom => screen.y - reveal,
        };
        Rect::new(Vec2::new(((screen.x - size.x) / 2.0).round(), y), size)
    }

    pub fn open_band(&self, screen: Vec2, panel: &Panel) -> Rect {
        Drawer {
            progress: 1.0,
            ..*self
        }
        .band(screen, panel)
    }

    pub fn band(&self, screen: Vec2, panel: &Panel) -> Rect {
        let rect = self.rect(screen, panel);
        let straight = rect.origin.y + panel.inner_row(self.edge);
        let y = match self.edge {
            Edge::Top => straight - panel.height,
            Edge::Bottom => straight,
        };
        Rect::new(Vec2::new(rect.origin.x, y), Vec2::new(rect.size.x, panel.height))
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
        let cy = inner - side * radius;
        if side * (py - cy) > 0.0 {
            let dx = px - cx;
            let dy = py - cy;
            return radius - (dx * dx + dy * dy).sqrt();
        }
    }

    side * (inner - py)
}

pub fn paint(edge: Edge, width: u32, panel: &Panel, color: [u8; 3], alpha: u8) -> Image {
    paint_flat(edge, width, panel, color, alpha)
}

pub fn rim(image: &mut Image, edge: Edge, panel: &Panel, color: [u8; 3], strength: f64) {
    let dense = crate::paint::density();
    let w = f64::from(image.width) / dense;
    for y in 0..image.height {
        let py = (f64::from(y) + 0.5) / dense;
        for x in 0..image.width {
            let d = depth(edge, (f64::from(x) + 0.5) / dense, py, w, panel);
            let weight = strength * (1.0 - (d - 0.5).abs()).clamp(0.0, 1.0);
            if weight <= 0.0 {
                continue;
            }
            let index = ((y * image.width + x) * 4) as usize;
            let a = f64::from(image.rgba[index + 3]) / 255.0;
            for (channel, value) in color.iter().enumerate() {
                let was = f64::from(image.rgba[index + channel]);
                let now = was + (f64::from(*value) * a - was) * weight;
                image.rgba[index + channel] = now.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

pub fn drop_shadow(edge: Edge, width: u32, panel: &Panel, strength: f64) -> Image {
    let dense = crate::paint::density();
    let reach = panel.shadow.max(1.0).round();
    let radius = panel.radius.max(0.0).round();
    let w = f64::from(width);
    let height = crate::paint::dense(reach + radius);
    let width = crate::paint::dense(w);
    let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
    let sigma = reach / 2.6;
    for y in 0..height {
        let from_edge = match edge {
            Edge::Top => (f64::from(y) + 0.5) / dense - radius,
            Edge::Bottom => (f64::from(height - y) - 0.5) / dense - radius,
        };
        for x in 0..width {
            let px = (f64::from(x) + 0.5) / dense;
            let corner = if px < radius {
                Some(radius)
            } else if px > w - radius {
                Some(w - radius)
            } else {
                None
            };
            let away = match corner {
                Some(cx) => {
                    let dx = px - cx;
                    let dy = from_edge + radius;
                    (dx * dx + dy * dy).sqrt() - radius
                }
                None => from_edge,
            };
            if away <= 0.0 {
                continue;
            }
            let falloff = (-(away * away) / (2.0 * sigma * sigma)).exp();
            let fade_in = (away * dense).min(1.0);
            let alpha = (strength * falloff * fade_in * 255.0).round().clamp(0.0, 255.0) as u8;
            rgba[(y as usize * width as usize + x as usize) * 4 + 3] = alpha;
        }
    }
    Image {
        width,
        height,
        rgba,
    }
}

pub fn wrap_shadow(width: u32, panel: &Panel, strength: f64) -> Image {
    let dense = crate::paint::density();
    let reach = panel.shadow.max(1.0).round();
    let w = f64::from(width);
    let size = Vec2::new(w + reach * 2.0, reach + panel.texture_height().round());
    let (pw, ph) = (crate::paint::dense(size.x), crate::paint::dense(size.y));
    let mut rgba = vec![0u8; (pw as usize) * (ph as usize) * 4];
    let sigma = reach / 2.6;
    for y in 0..ph {
        let py = (f64::from(y) + 0.5) / dense - reach;
        for x in 0..pw {
            let px = (f64::from(x) + 0.5) / dense - reach;
            let beside = py > panel.inner_row(Edge::Bottom) + panel.radius;
            let away = match (px < 0.0, px > w) {
                (true, _) if beside => -px,
                (_, true) if beside => px - w,
                _ => -depth(Edge::Bottom, px, py, w, panel),
            };
            if away <= 0.0 {
                continue;
            }
            let falloff = (-(away * away) / (2.0 * sigma * sigma)).exp();
            let fade_in = (away * dense).min(1.0);
            rgba[(y as usize * pw as usize + x as usize) * 4 + 3] =
                (strength * falloff * fade_in * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    Image { width: pw, height: ph, rgba }
}

pub fn panel_slices(art: Vec2, shown: Rect, cap: f64) -> [(Rect, Rect); 3] {
    let cap = cap.min(art.x / 2.0).min(shown.size.x / 2.0).max(0.0);
    let tall = art.y;
    [
        (Rect::new(Vec2::ZERO, Vec2::new(cap, tall)), Rect::new(shown.origin, Vec2::new(cap, tall))),
        (
            Rect::new(Vec2::new(cap, 0.0), Vec2::new((art.x - cap * 2.0).max(1.0), tall)),
            Rect::new(shown.origin + Vec2::new(cap, 0.0), Vec2::new((shown.size.x - cap * 2.0).max(0.0), tall)),
        ),
        (
            Rect::new(Vec2::new(art.x - cap, 0.0), Vec2::new(cap, tall)),
            Rect::new(shown.origin + Vec2::new(shown.size.x - cap, 0.0), Vec2::new(cap, tall)),
        ),
    ]
}

pub fn wrap_rect(rect: Rect, panel: &Panel) -> Rect {
    let reach = panel.shadow.max(1.0).round();
    Rect::new(
        rect.origin - Vec2::splat(reach),
        Vec2::new(rect.size.x + reach * 2.0, rect.size.y + reach),
    )
}

pub fn shadow_rect(band: Rect, edge: Edge, panel: &Panel) -> Rect {
    let reach = panel.shadow.max(1.0).round();
    let radius = panel.radius.max(0.0).round();
    let y = match edge {
        Edge::Top => band.max().y - radius,
        Edge::Bottom => band.origin.y - reach,
    };
    Rect::new(Vec2::new(band.origin.x, y), Vec2::new(band.size.x, reach + radius))
}

pub fn paint_flat(edge: Edge, width: u32, panel: &Panel, color: [u8; 3], alpha: u8) -> Image {
    let dense = crate::paint::density();
    let height = crate::paint::dense(panel.texture_height().round().max(1.0));
    let a = f64::from(alpha) / 255.0;
    let w = f64::from(width);
    let width = crate::paint::dense(w);
    let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];

    for y in 0..height {
        let py = (f64::from(y) + 0.5) / dense;
        for x in 0..width {
            let px = (f64::from(x) + 0.5) / dense;
            let into_panel = depth(edge, px, py, w, panel);
            let coverage = (into_panel * dense + 0.5).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }

            let pixel_alpha = a * coverage;
            let index = ((y as usize) * (width as usize) + x as usize) * 4;
            for (channel, value) in color.iter().enumerate() {
                rgba[index + channel] = (f64::from(*value) * pixel_alpha)
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
    #[test]
    fn panel_slices_cover_the_width_and_keep_the_caps() {
        use super::panel_slices;
        use crate::geom::{Rect, Vec2};
        let art = Vec2::new(400.0, 50.0);
        for width in [300.0, 400.0, 520.0] {
            let shown = Rect::new(Vec2::new(100.0, 900.0), Vec2::new(width, 50.0));
            let [left, middle, right] = panel_slices(art, shown, 12.0);
            assert_eq!(left.1.size.x + middle.1.size.x + right.1.size.x, width, "they cover {width}");
            assert_eq!(middle.1.origin.x, left.1.max().x);
            assert_eq!(right.1.origin.x, middle.1.max().x);
            assert_eq!(left.0.size, left.1.size, "the caps never stretch");
            assert_eq!(right.0.size, right.1.size);
            assert_eq!(right.0.max().x, art.x);
        }
    }

    #[test]
    fn a_narrow_dock_casts_shadow_on_its_sides_but_not_inside() {
        let panel = Panel { height: 64.0, span: 300.0, ..Panel::default() };
        let image = wrap_shadow(300, &panel, 0.5);
        let reach = panel.shadow.round() as u32;
        let alpha = |x: u32, y: u32| image.rgba[((y * image.width + x) * 4 + 3) as usize];
        let mid_y = reach + panel.texture_height() as u32 / 2;
        assert!(alpha(reach - 4, mid_y) > 0, "the left side has shadow");
        assert!(alpha(image.width - reach + 3, mid_y) > 0, "the right side has shadow");
        assert!(alpha(image.width / 2, reach - 4) > 0, "the top has shadow");
        assert_eq!(alpha(image.width / 2, mid_y), 0, "nothing under the dock itself");
    }

    #[test]
    fn a_narrow_panel_sits_in_the_middle_and_its_band_matches() {
        let panel = Panel { span: 400.0, ..Panel::default() };
        let mut drawer = Drawer::new(Edge::Bottom);
        drawer.open = true;
        drawer.advance(std::time::Duration::from_secs(1), std::time::Duration::from_millis(1));
        let screen = Vec2::new(1600.0, 1000.0);
        let rect = drawer.rect(screen, &panel);
        let band = drawer.band(screen, &panel);
        assert_eq!(rect.origin.x, 600.0);
        assert_eq!(rect.size.x, 400.0);
        assert_eq!((band.origin.x, band.size.x), (600.0, 400.0));
        let full = Panel::default();
        assert_eq!(drawer.band(screen, &full).size.x, 1600.0);
        assert_eq!(Panel { span: 4000.0, ..full }.width(1600.0), 1600.0);
    }

    use super::*;

    const SCREEN: Vec2 = Vec2::new(1920.0, 1080.0);
    const WIDTH: u32 = 400;

    fn panel() -> Panel {
        Panel::default()
    }

    #[test]
    fn bars_at_density_two_match_density_one() {
        let panel = panel();
        for edge in Edge::ALL {
            crate::paint::same_at_two("bar", || paint_flat(edge, 300, &panel, [30, 30, 34], 220));
            crate::paint::same_at_two("rim", || {
                let mut image = paint_flat(edge, 300, &panel, [30, 30, 34], 220);
                rim(&mut image, edge, &panel, [255, 255, 255], 0.4);
                image
            });
            crate::paint::same_at_two("shadow", || drop_shadow(edge, 300, &panel, 0.5));
        }
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
        assert_eq!(
            edge_at(Vec2::new(900.0, 0.0), SCREEN, 2.0, 12.0),
            Some(Edge::Top)
        );
        assert_eq!(
            edge_at(Vec2::new(900.0, 1079.5), SCREEN, 2.0, 12.0),
            Some(Edge::Bottom)
        );
        assert_eq!(edge_at(Vec2::new(900.0, 540.0), SCREEN, 2.0, 12.0), None);
    }

    #[test]
    fn the_hot_corner_is_not_the_top_drawer() {
        assert_eq!(edge_at(Vec2::new(0.5, 0.5), SCREEN, 2.0, 12.0), None);
        assert_eq!(edge_at(Vec2::new(11.0, 1.0), SCREEN, 2.0, 12.0), None);
    }

    #[test]
    fn the_top_drawer_starts_where_the_hot_corner_ends() {
        assert_eq!(
            edge_at(Vec2::new(12.5, 1.0), SCREEN, 2.0, 12.0),
            Some(Edge::Top)
        );
    }

    #[test]
    fn the_bottom_left_corner_is_still_the_dock() {
        assert_eq!(
            edge_at(Vec2::new(1.0, 1079.5), SCREEN, 2.0, 12.0),
            Some(Edge::Bottom)
        );
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
    fn the_drawer_has_its_own_rounded_corners_on_the_inner_edge() {
        let panel = panel();
        let screen = Vec2::new(f64::from(WIDTH), SCREEN.y);
        for edge in Edge::ALL {
            let image = paint(edge, WIDTH, &panel, [16, 18, 26], 255);
            let rect = opened(edge).rect(screen, &panel);
            let just_inside = match edge {
                Edge::Top => panel.height - 0.5,
                Edge::Bottom => screen.y - panel.height + 0.5,
            };
            let cut = on_screen(&image, rect, Vec2::new(0.5, just_inside));
            assert_eq!(cut[3], 0, "{edge:?}: the corner is cut away");
            let middle = on_screen(&image, rect, Vec2::new(f64::from(WIDTH) / 2.0, just_inside));
            assert_eq!(middle[3], 255, "{edge:?}: the straight edge is solid");
            let deep = match edge {
                Edge::Top => panel.height - panel.radius - 1.0,
                Edge::Bottom => screen.y - panel.height + panel.radius + 1.0,
            };
            assert_eq!(on_screen(&image, rect, Vec2::new(0.5, deep))[3], 255, "{edge:?}");
            let past = match edge {
                Edge::Top => panel.height + 2.0,
                Edge::Bottom => screen.y - panel.height - 2.0,
            };
            assert_eq!(on_screen(&image, rect, Vec2::new(0.5, past))[3], 0, "{edge:?}: nothing past the edge");
        }
    }

    #[test]
    fn the_outer_shadow_is_darkest_by_the_drawer_and_fades_away() {
        let panel = panel();
        for edge in Edge::ALL {
            let image = drop_shadow(edge, 400, &panel, 0.4);
            let alpha = |y: u32| image.rgba[((y * image.width + 200) * 4 + 3) as usize];
            let r = panel.radius as u32;
            let (near, far) = match edge {
                Edge::Top => (r + 2, image.height - 1),
                Edge::Bottom => (image.height - 3 - r, 0),
            };
            assert!(alpha(near) > 90, "{edge:?}: {}", alpha(near));
            assert!(alpha(far) < 5, "{edge:?}: {}", alpha(far));
            assert!(image.rgba.chunks(4).all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0));
        }
    }

    #[test]
    fn the_shadow_strip_sits_just_past_the_drawer_on_the_wallpaper() {
        let panel = panel();
        let screen = Vec2::new(f64::from(WIDTH), SCREEN.y);
        let top = opened(Edge::Top).band(screen, &panel);
        let below = shadow_rect(top, Edge::Top, &panel);
        assert_eq!(below.origin.y + panel.radius, top.max().y);
        let bottom = opened(Edge::Bottom).band(screen, &panel);
        let above = shadow_rect(bottom, Edge::Bottom, &panel);
        assert_eq!(above.max().y - panel.radius, bottom.origin.y);
        let image = drop_shadow(Edge::Top, 400, &panel, 0.4);
        let in_the_cut = image.rgba[(((panel.radius as u32 - 1) * 400) * 4 + 3) as usize];
        assert!(in_the_cut > 0, "the shadow follows the corner into the cut");
    }

    #[test]
    fn the_rim_is_a_thin_line_on_the_edge_only() {
        let panel = panel();
        let screen = Vec2::new(f64::from(WIDTH), SCREEN.y);
        let mut image = paint(Edge::Top, WIDTH, &panel, [100, 100, 100], 255);
        rim(&mut image, Edge::Top, &panel, [255, 255, 255], 0.5);
        let rect = opened(Edge::Top).rect(screen, &panel);
        let middle = f64::from(WIDTH) / 2.0;
        let edge = on_screen(&image, rect, Vec2::new(middle, panel.height - 0.5))[0];
        let inside = on_screen(&image, rect, Vec2::new(middle, panel.height - 4.0))[0];
        assert!(edge > 150, "{edge}");
        assert_eq!(inside, 100, "the body keeps its color");
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
