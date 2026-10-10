use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use drifter_core::Image;
use drifter_core::explorer::entry::{self, Family};
use drifter_core::explorer::look::Want;
use drifter_core::explorer::thumbs;
use drifter_core::explorer::prefs::on_path;
use smithay_client_toolkit::reexports::calloop::channel::Sender as LoopSender;

const MOST_BYTES: u64 = 120 << 20;
const WORKERS: usize = 2;

pub struct Painter {
    asks: Sender<Want>,
}

pub fn fit(image: &Image, side: u32) -> Image {
    let (w, h) = (image.width.max(1), image.height.max(1));
    if w <= side && h <= side {
        return image.clone();
    }
    let (to_w, to_h) = match w >= h {
        true => (side, (u64::from(h) * u64::from(side) / u64::from(w)).max(1) as u32),
        false => ((u64::from(w) * u64::from(side) / u64::from(h)).max(1) as u32, side),
    };
    if (to_w, to_h) == (w, h) {
        return image.clone();
    }
    image.scaled(to_w, to_h)
}

pub fn icon(name: &str, side: u32) -> Option<Image> {
    let path = match name.starts_with('/') {
        true => PathBuf::from(name),
        false => drifter_core::icon::find(name, None, side.max(16))?,
    };
    drifter_core::image::load_sized(&path, side).ok().map(|image| fit(&image, side))
}

fn spare(path: &Path, side: u32, tag: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    side.hash(&mut hasher);
    std::env::temp_dir().join(format!("drifter-explorer-{}-{:x}-{tag}", std::process::id(), hasher.finish()))
}

fn quiet(command: &mut std::process::Command) -> bool {
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

fn picture(path: &Path, side: u32) -> Option<Image> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MOST_BYTES {
        return None;
    }
    let root = thumbs::root();
    if !thumbs::worth_caching(path, &root) {
        return make(path, side);
    }
    if let Some(image) = thumbs::cached_in(&root, path, side) {
        return Some(fit(&image, side));
    }
    if thumbs::failed_in(&root, path) {
        return None;
    }
    let (size, _) = thumbs::bucket(side);
    match make(path, size) {
        Some(image) => {
            let _ = thumbs::store_in(&root, path, size, &image);
            Some(fit(&image, side))
        }
        None => {
            if can_make(path) {
                let _ = thumbs::store_failed_in(&root, path);
            }
            None
        }
    }
}

fn can_make(path: &Path) -> bool {
    let mime = entry::mime_of(&entry::name_of(path), false);
    match entry::family(mime, false) {
        Family::Image => true,
        Family::Video => on_path("ffmpegthumbnailer"),
        Family::Pdf => on_path("pdftoppm"),
        _ => false,
    }
}

fn make(path: &Path, side: u32) -> Option<Image> {
    let name = entry::name_of(path);
    let mime = entry::mime_of(&name, false);
    match entry::family(mime, false) {
        Family::Image => drifter_core::image::load_sized(path, side).ok().map(|image| fit(&image, side)),
        Family::Video if on_path("ffmpegthumbnailer") => {
            let out = spare(path, side, "video.png");
            let made = quiet(std::process::Command::new("ffmpegthumbnailer").arg("-i").arg(path).arg("-o").arg(&out).arg("-s").arg(side.to_string()).arg("-f"));
            let image = made.then(|| drifter_core::image::load(&out).ok()).flatten();
            let _ = std::fs::remove_file(&out);
            image.map(|image| fit(&image, side))
        }
        Family::Pdf if on_path("pdftoppm") => {
            let base = spare(path, side, "page");
            let made = quiet(std::process::Command::new("pdftoppm").args(["-png", "-singlefile", "-f", "1", "-l", "1", "-scale-to"]).arg(side.to_string()).arg(path).arg(&base));
            let out = PathBuf::from(format!("{}.png", base.display()));
            let image = made.then(|| drifter_core::image::load(&out).ok()).flatten();
            let _ = std::fs::remove_file(&out);
            image.map(|image| fit(&image, side))
        }
        _ => None,
    }
}

impl Painter {
    pub fn start(done: LoopSender<(Want, Option<Image>)>) -> Painter {
        let (asks, heard) = channel::<Want>();
        let heard: Arc<Mutex<Receiver<Want>>> = Arc::new(Mutex::new(heard));
        for _ in 0..WORKERS {
            let heard = heard.clone();
            let done = done.clone();
            std::thread::spawn(move || {
                loop {
                    let want = match heard.lock() {
                        Ok(heard) => match heard.recv() {
                            Ok(want) => want,
                            Err(_) => return,
                        },
                        Err(_) => return,
                    };
                    let image = match &want {
                        Want::Thumb(path, side) => picture(path, *side),
                        Want::Icon(name, side) => icon(name, *side),
                    };
                    if done.send((want, image)).is_err() {
                        return;
                    }
                }
            });
        }
        Painter { asks }
    }

    pub fn ask(&self, want: Want) {
        let _ = self.asks.send(want);
    }
}
