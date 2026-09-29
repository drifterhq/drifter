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

pub fn pictures(folder: &std::path::Path) -> Vec<std::path::PathBuf> {
    let Ok(entries) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<std::path::PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| is_picture(path))
        .collect();
    found.sort();
    found
}

pub fn is_picture(path: &std::path::Path) -> bool {
    path.is_file()
        && path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_lowercase())
            .is_some_and(|ext| matches!(ext.as_str(), "png" | "jpg" | "jpeg"))
}

pub fn next_picture(
    pictures: &[std::path::PathBuf],
    current: Option<&std::path::Path>,
    order: crate::config::WallpaperOrder,
    seed: u64,
) -> Option<std::path::PathBuf> {
    let at = current.and_then(|now| pictures.iter().position(|path| path == now));
    let count = pictures.len();
    let index = match (order, at, count) {
        (_, _, 0) => return None,
        (_, Some(_), 1) => return None,
        (crate::config::WallpaperOrder::Ordered, Some(at), _) => (at + 1) % count,
        (crate::config::WallpaperOrder::Ordered, None, _) => 0,
        (crate::config::WallpaperOrder::Random, Some(at), _) => {
            let step = 1 + (seed % (count as u64 - 1)) as usize;
            (at + step) % count
        }
        (crate::config::WallpaperOrder::Random, None, _) => (seed % count as u64) as usize,
    };
    pictures.get(index).cloned()
}

#[cfg(test)]
mod folder_tests {
    use super::*;
    use crate::config::WallpaperOrder;
    use std::path::PathBuf;

    fn list() -> Vec<PathBuf> {
        ["a.jpg", "b.png", "c.jpeg"]
            .iter()
            .map(PathBuf::from)
            .collect()
    }

    #[test]
    fn in_order_goes_to_the_next_and_wraps() {
        let all = list();
        let next = |now: &str| next_picture(&all, Some(now.as_ref()), WallpaperOrder::Ordered, 0);
        assert_eq!(next("a.jpg"), Some(PathBuf::from("b.png")));
        assert_eq!(next("c.jpeg"), Some(PathBuf::from("a.jpg")));
        assert_eq!(
            next_picture(&all, None, WallpaperOrder::Ordered, 9),
            Some(PathBuf::from("a.jpg"))
        );
    }

    #[test]
    fn at_random_never_shows_the_same_picture_twice_in_a_row() {
        let all = list();
        for seed in 0..50 {
            let next = next_picture(&all, Some("b.png".as_ref()), WallpaperOrder::Random, seed);
            assert_ne!(next, Some(PathBuf::from("b.png")));
            assert!(next.is_some());
        }
    }

    #[test]
    fn one_or_no_pictures_never_rotate() {
        let one = vec![PathBuf::from("a.jpg")];
        assert_eq!(
            next_picture(&one, Some("a.jpg".as_ref()), WallpaperOrder::Random, 3),
            None
        );
        assert_eq!(next_picture(&[], None, WallpaperOrder::Ordered, 0), None);
    }

    #[test]
    fn only_pictures_are_listed_in_name_order() {
        let folder = std::env::temp_dir().join(format!("drifter-walls-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        for name in ["b.PNG", "a.jpg", "notes.txt", "c.jpeg"] {
            std::fs::write(folder.join(name), b"x").unwrap();
        }
        let names: Vec<String> = pictures(&folder)
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        std::fs::remove_dir_all(&folder).unwrap();
        assert_eq!(names, ["a.jpg", "b.PNG", "c.jpeg"]);
    }
}
