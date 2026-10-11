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

impl Placement {
    pub fn seen(&self, screen: Vec2) -> Placement {
        let target = self.target;
        let lo = target.origin.max(Vec2::ZERO);
        let hi = (target.origin + target.size).min(screen);
        let size = (hi - lo).max(Vec2::ZERO);
        let scale = |v: Vec2| {
            Vec2::new(
                v.x * self.source.size.x / target.size.x.max(f64::MIN_POSITIVE),
                v.y * self.source.size.y / target.size.y.max(f64::MIN_POSITIVE),
            )
        };
        Placement {
            source: Rect::new(self.source.origin + scale(lo - target.origin), scale(size)),
            target: Rect::new(lo, size),
        }
    }

    pub fn onto(&self, from: Vec2, to: Vec2) -> Placement {
        let scale = |v: Vec2| {
            Vec2::new(
                v.x * to.x / from.x.max(f64::MIN_POSITIVE),
                v.y * to.y / from.y.max(f64::MIN_POSITIVE),
            )
        };
        Placement {
            source: Rect::new(scale(self.source.origin), scale(self.source.size)),
            target: self.target,
        }
    }
}

pub const SHADER_ROOM: f64 = 4.0 / 3.0;

pub fn shader_paper(screen: Vec2) -> Vec2 {
    screen * SHADER_ROOM
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

pub fn pointer_in_paper(source: Rect, target: Rect, whole: Vec2, pointer: Vec2) -> Vec2 {
    let x = source.origin.x + (pointer.x - target.origin.x) * source.size.x / target.size.x.max(f64::MIN_POSITIVE);
    let y = source.origin.y + (pointer.y - target.origin.y) * source.size.y / target.size.y.max(f64::MIN_POSITIVE);
    Vec2::new(x, whole.y - y)
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
            .is_some_and(|ext| matches!(ext.as_str(), "png" | "jpg" | "jpeg" | "svg" | "shader"))
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
mod shader_tests {
    use super::*;

    #[test]
    fn the_pointer_maps_into_shader_space_with_y_up() {
        let whole = Vec2::new(2560.0, 1440.0);
        let source = Rect::new(Vec2::new(320.0, 180.0), Vec2::new(1920.0, 1080.0));
        let target = Rect::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        assert_eq!(pointer_in_paper(source, target, whole, Vec2::ZERO), Vec2::new(320.0, 1440.0 - 180.0));
        let low = pointer_in_paper(source, target, whole, Vec2::new(0.0, 1080.0));
        assert_eq!(low.y, 1440.0 - 1260.0, "lower on screen is lower in P");
    }

    #[test]
    fn a_shader_paper_leaves_parallax_room_past_the_screen() {
        let screen = Vec2::new(1920.0, 1080.0);
        let world = Vec2::new(8000.0, 6000.0);
        let paper = shader_paper(screen);
        assert_eq!(paper, Vec2::new(2560.0, 1440.0));
        let home = Camera::new(Vec2::new(1000.0, 1000.0), 1.0);
        let away = Camera::new(Vec2::new(5000.0, 1000.0), 1.0);
        let from = place(WallpaperMode::Parallax, paper, screen, world, &home);
        let to = place(WallpaperMode::Parallax, paper, screen, world, &away);
        assert_eq!(from.source.size, screen);
        assert!(to.source.origin.x > from.source.origin.x);
        assert!(to.source.origin.x + to.source.size.x <= paper.x + 1e-9);
    }

    #[test]
    fn a_placement_moves_onto_a_smaller_still() {
        let laid = Placement {
            source: Rect::new(Vec2::new(320.0, 180.0), Vec2::new(1920.0, 1080.0)),
            target: Rect::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0)),
        };
        let small = laid.onto(Vec2::new(2560.0, 1440.0), Vec2::new(960.0, 540.0));
        assert_eq!(small.source, Rect::new(Vec2::new(120.0, 67.5), Vec2::new(720.0, 405.0)));
        assert_eq!(small.target, laid.target);
    }

    #[test]
    fn only_the_part_on_the_screen_is_seen() {
        let screen = Vec2::new(100.0, 100.0);
        let laid = Placement {
            source: Rect::new(Vec2::ZERO, Vec2::new(400.0, 400.0)),
            target: Rect::new(Vec2::new(-100.0, -50.0), Vec2::new(800.0, 800.0)),
        };
        let seen = laid.seen(screen);
        assert_eq!(seen.target, Rect::new(Vec2::ZERO, screen));
        assert_eq!(seen.source, Rect::new(Vec2::new(50.0, 25.0), Vec2::new(50.0, 50.0)));
        let whole = Placement {
            source: Rect::new(Vec2::new(5.0, 5.0), Vec2::new(10.0, 10.0)),
            target: Rect::new(Vec2::ZERO, screen),
        };
        assert_eq!(whole.seen(screen), whole);
    }
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
    fn svg_files_are_pictures_like_png_and_jpeg() {
        let dir = std::env::temp_dir().join(format!("drifter-pictures-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["a.svg", "b.PNG", "c.jpg", "d.SVG", "e.txt", "f.webp"] {
            std::fs::write(dir.join(name), b"").unwrap();
        }
        let found: Vec<String> = pictures(&dir)
            .iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(found, ["a.svg", "b.PNG", "c.jpg", "d.SVG"]);
        std::fs::remove_dir_all(&dir).unwrap();
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
