use crate::config::WallpaperMode;
use crate::geom::Vec2;

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
