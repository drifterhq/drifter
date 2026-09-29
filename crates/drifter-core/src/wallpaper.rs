use crate::camera::Camera;
use crate::config::WallpaperMode;
use crate::geom::{Rect, Vec2};

pub fn parallax_factor(wallpaper: f64, visible: f64, world: f64) -> f64 {
    let span = world - visible;
    if span <= 0.0 {
        return 0.0;
    }
    ((wallpaper - visible) / span).clamp(0.0, 1.0)
}

pub fn factor(mode: WallpaperMode, wallpaper: Vec2, visible: Vec2, world: Vec2) -> Vec2 {
    match mode {
        WallpaperMode::Fixed => Vec2::ZERO,
        WallpaperMode::Pan => Vec2::splat(1.0),
        WallpaperMode::Parallax => Vec2::new(
            parallax_factor(wallpaper.x, visible.x, world.x),
            parallax_factor(wallpaper.y, visible.y, world.y),
        ),
    }
}

pub fn origin(
    mode: WallpaperMode,
    visible_origin: Vec2,
    wallpaper: Vec2,
    visible: Vec2,
    world: Vec2,
) -> Vec2 {
    let f = factor(mode, wallpaper, visible, world);
    Vec2::new(visible_origin.x * f.x, visible_origin.y * f.y)
}

pub fn required_size(mode: WallpaperMode, visible: Vec2, world: Vec2) -> Vec2 {
    match mode {
        WallpaperMode::Fixed => visible,
        WallpaperMode::Pan => world,
        WallpaperMode::Parallax => visible,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    pub source: Rect,
    pub target: Rect,
}

pub fn cover(image: Vec2, area: Vec2) -> Rect {
    if image.x <= 0.0 || image.y <= 0.0 || area.x <= 0.0 || area.y <= 0.0 {
        return Rect::new(Vec2::ZERO, image);
    }
    let scale = (area.x / image.x).max(area.y / image.y);
    let size = area / scale;
    Rect::new((image - size) / 2.0, size)
}

pub fn paper_size(image: Vec2, screen: Vec2, world: Vec2) -> Vec2 {
    if image.x <= 0.0 || image.y <= 0.0 {
        return screen;
    }
    let grow = (screen.x / image.x).max(screen.y / image.y).max(1.0);
    (image * grow).min(world.max(screen))
}

pub fn place(
    mode: WallpaperMode,
    image: Vec2,
    screen: Vec2,
    world: Vec2,
    camera: &Camera,
) -> Placement {
    let whole = Rect::new(Vec2::ZERO, screen);
    match mode {
        WallpaperMode::Fixed => Placement {
            source: cover(image, screen),
            target: whole,
        },
        WallpaperMode::Pan => Placement {
            source: cover(image, world),
            target: Rect::new(
                camera.screen_from_world(Vec2::ZERO, screen),
                world * camera.zoom,
            ),
        },
        WallpaperMode::Parallax => {
            let paper = paper_size(image, screen, world);
            let grow = (screen.x / image.x).max(screen.y / image.y).max(1.0);
            let laid = image * grow;
            let paper_origin = (laid - paper) / 2.0;

            let most = (paper.x / screen.x)
                .min(paper.y / screen.y)
                .max(f64::MIN_POSITIVE);
            let seen = screen * (1.0 / camera.zoom).min(most);
            let visible = camera.visible_rect(screen);
            let along = |at: f64, size: f64, world: f64| match world - size > 0.0 {
                true => (at / (world - size)).clamp(0.0, 1.0),
                false => 0.5,
            };
            let t = Vec2::new(
                along(visible.origin.x, visible.size.x, world.x),
                along(visible.origin.y, visible.size.y, world.y),
            );
            let room = paper - seen;
            let corner = paper_origin + Vec2::new(room.x * t.x, room.y * t.y);
            Placement {
                source: Rect::new(corner / grow, seen / grow),
                target: whole,
            }
        }
    }
}
