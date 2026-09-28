pub mod camera;
pub mod config;
pub mod corner;
pub mod cursor;
pub mod dock;
pub mod drawer;
pub mod geom;
pub mod image;
pub mod menu;
pub mod mode;
pub mod motion;
pub mod snap;
pub mod text;
pub mod tray;
pub mod wallpaper;
pub mod window;

pub use camera::Camera;
pub use config::{Assets, Config, DockConfig, Modifier, TrayConfig, Tunables, WallpaperMode};
pub use cursor::Cursor;
pub use drawer::{Drawer, Edge, Panel};
pub use geom::{Rect, Vec2};
pub use image::{Image, load};
pub use mode::Mode;
pub use text::Font;
pub use tray::Tray;
pub use wallpaper::parallax_factor;

#[cfg(test)]
mod wallpaper_tests {
    use crate::config::WallpaperMode;
    use crate::geom::Vec2;
    use crate::wallpaper;

    const VIEWPORT: Vec2 = Vec2::new(1920.0, 1080.0);
    const WORLD: Vec2 = Vec2::new(3840.0, 2160.0);
    const TWO_K: Vec2 = Vec2::new(2560.0, 1440.0);

    #[test]
    fn fixed_never_moves() {
        let f = wallpaper::factor(WallpaperMode::Fixed, VIEWPORT, VIEWPORT, WORLD);
        assert_eq!(f, Vec2::ZERO);
    }

    #[test]
    fn pan_moves_with_the_world() {
        let f = wallpaper::factor(WallpaperMode::Pan, WORLD, VIEWPORT, WORLD);
        assert_eq!(f, Vec2::splat(1.0));
    }

    #[test]
    fn parallax_factor_is_derived_from_the_three_sizes() {
        let f = wallpaper::factor(WallpaperMode::Parallax, TWO_K, VIEWPORT, WORLD);
        assert!((f.x - 1.0 / 3.0).abs() < 1e-9);
        assert!((f.y - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn parallax_edges_land_on_the_world_edges() {
        let mode = WallpaperMode::Parallax;
        let far = WORLD - VIEWPORT;
        let origin = wallpaper::origin(mode, far, TWO_K, VIEWPORT, WORLD);
        assert!((origin.x + VIEWPORT.x - TWO_K.x).abs() < 1e-9);
        assert!((origin.y + VIEWPORT.y - TWO_K.y).abs() < 1e-9);
    }

    #[test]
    fn a_world_sized_wallpaper_in_parallax_behaves_like_pan() {
        let f = wallpaper::factor(WallpaperMode::Parallax, WORLD, VIEWPORT, WORLD);
        assert_eq!(f, Vec2::splat(1.0));
    }

    #[test]
    fn a_viewport_sized_wallpaper_in_parallax_behaves_like_fixed() {
        let f = wallpaper::factor(WallpaperMode::Parallax, VIEWPORT, VIEWPORT, WORLD);
        assert_eq!(f, Vec2::ZERO);
    }

    #[test]
    fn required_size_matches_the_mode() {
        assert_eq!(
            wallpaper::required_size(WallpaperMode::Fixed, VIEWPORT, WORLD),
            VIEWPORT
        );
        assert_eq!(
            wallpaper::required_size(WallpaperMode::Pan, VIEWPORT, WORLD),
            WORLD
        );
    }
}
