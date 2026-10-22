pub mod activity;
pub mod airdrop;
pub mod alert;
pub mod autostart;
pub mod blue;
pub mod calendar;
pub mod camera;
pub mod cascade;
pub mod charges;
pub mod color;
pub mod config;
pub mod control;
pub mod corner;
pub mod cursor;
pub mod desktop;
pub mod disks;
pub mod dress;
pub mod dock;
pub mod drawer;
pub mod exec;
pub mod explorer;
pub mod fnkey;
pub mod folders;
pub mod frost;
pub mod geom;
pub mod genie;
pub mod glance;
pub mod grab;
pub mod hdr;
pub mod hud;
pub mod icon;
pub mod image;
pub mod keyboard;
pub mod lang;
pub mod launchpad;
pub mod lock;
pub mod menu;
pub mod mode;
pub mod motion;
pub mod net;
pub mod night;
pub mod notify;
pub mod pairing;
pub mod paint;
pub mod place;
pub mod power;
pub mod prompt;
pub mod reopen;
pub mod ripple;
pub mod screens;
pub mod search;
pub mod settings;
pub mod shader;
pub mod share;
pub mod shutter;
pub mod sheet;
pub mod shortcut;
pub mod snap;
pub mod spatial;
pub mod stats;
pub mod sticky;
pub mod switcher;
pub mod system;
pub mod text;
pub mod theme;
pub mod tiles;
pub mod tray;
pub mod tween;
pub mod users;
pub mod wallpaper;
pub mod welcome;
pub mod window;

#[cfg(test)]
mod density_tests;

pub use camera::Camera;
pub use config::{
    Accent, Animation, Access, DockHover, Inactive, Appearance, AppsConfig, Assets, ButtonName, Config, DockConfig, Keyboard, Keys,
    Launch, Lid, Minimize, Modifier, Motion, Mouse, Night, NotePosition, Notify, Power, PowerKey, ShotFormat, Shots, Side,
    Style, ThreeTap, Transition, Touchpad, TrayConfig, Tunables, WallpaperMode, WallpaperOrder,
};
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

    fn aspect(rect: crate::geom::Rect) -> f64 {
        rect.size.x / rect.size.y
    }

    const PHOTO: Vec2 = Vec2::new(4000.0, 3000.0);

    #[test]
    fn fixed_crops_the_image_to_the_screen_shape() {
        let camera = crate::Camera::at_work_zoom(WORLD / 2.0);
        let laid = wallpaper::place(WallpaperMode::Fixed, PHOTO, VIEWPORT, WORLD, &camera);
        assert_eq!(laid.target.size, VIEWPORT);
        assert!((aspect(laid.source) - aspect(laid.target)).abs() < 1e-9);
        assert!(laid.source.origin.x >= 0.0 && laid.source.origin.y >= 0.0);
        assert!(laid.source.max().x <= PHOTO.x + 1e-9 && laid.source.max().y <= PHOTO.y + 1e-9);
    }

    #[test]
    fn pan_keeps_the_picture_in_shape_on_a_tall_world() {
        let tall = Vec2::new(4.0 * 1920.0, 6.0 * 1080.0);
        let camera = crate::Camera::at_work_zoom(tall / 2.0);
        let laid = wallpaper::place(WallpaperMode::Pan, PHOTO, VIEWPORT, tall, &camera);
        assert!((aspect(laid.source) - aspect(laid.target)).abs() < 1e-9);
        assert_eq!(laid.target.size, tall);
    }

    #[test]
    fn pan_moves_the_picture_with_the_world() {
        let one = crate::Camera::at_work_zoom(WORLD / 2.0);
        let mut two = one;
        two.center.x += 100.0;
        let a = wallpaper::place(WallpaperMode::Pan, PHOTO, VIEWPORT, WORLD, &one);
        let b = wallpaper::place(WallpaperMode::Pan, PHOTO, VIEWPORT, WORLD, &two);
        assert!((a.target.origin.x - b.target.origin.x - 100.0).abs() < 1e-9);
    }

    fn parallax_at(center: Vec2, zoom: f64) -> wallpaper::Placement {
        let camera = crate::Camera::new(center, zoom);
        wallpaper::place(WallpaperMode::Parallax, TWO_K, VIEWPORT, WORLD, &camera)
    }

    #[test]
    fn parallax_moves_a_third_as_fast_as_the_world_with_a_2k_picture() {
        let a = parallax_at(Vec2::new(1920.0, 1080.0), 1.0);
        let b = parallax_at(Vec2::new(1920.0 + 300.0, 1080.0), 1.0);
        assert!((b.source.origin.x - a.source.origin.x - 100.0).abs() < 1e-9);
        assert_eq!(a.source.size, VIEWPORT);
        assert_eq!(a.target.size, VIEWPORT);
    }

    #[test]
    fn parallax_reaches_the_picture_edges_at_the_world_edges() {
        let left = parallax_at(VIEWPORT / 2.0, 1.0);
        let right = parallax_at(WORLD - VIEWPORT / 2.0, 1.0);
        assert_eq!(left.source.origin, Vec2::ZERO);
        assert!((right.source.max().x - TWO_K.x).abs() < 1e-9);
        assert!((right.source.max().y - TWO_K.y).abs() < 1e-9);
    }

    #[test]
    fn parallax_never_reads_outside_the_picture_at_any_zoom() {
        for zoom in [0.5, 0.75, 1.0, 2.0, 4.0] {
            for x in [0.0, 900.0, 1920.0, 3000.0, 3840.0] {
                let laid = parallax_at(Vec2::new(x, 1000.0), zoom);
                assert!(laid.source.origin.x >= -1e-9 && laid.source.origin.y >= -1e-9);
                assert!(laid.source.max().x <= TWO_K.x + 1e-9, "zoom {zoom} x {x}");
                assert!(laid.source.max().y <= TWO_K.y + 1e-9, "zoom {zoom} x {x}");
                assert!((aspect(laid.source) - aspect(laid.target)).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn a_small_picture_is_grown_to_cover_the_screen() {
        let paper = wallpaper::paper_size(Vec2::new(800.0, 600.0), VIEWPORT, WORLD);
        assert!(paper.x >= VIEWPORT.x && paper.y >= VIEWPORT.y);
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
