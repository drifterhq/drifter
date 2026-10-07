use crate::geom::{Rect, Vec2};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub const WAIT: std::time::Duration = std::time::Duration::from_secs(90);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Start {
    pub argv: Vec<String>,
    #[serde(default)]
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Spot {
    pub app: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Spot {
    pub fn new(app: &str, frame: Rect) -> Spot {
        Spot {
            app: app.to_string(),
            x: frame.origin.x.round(),
            y: frame.origin.y.round(),
            width: frame.size.x.round(),
            height: frame.size.y.round(),
        }
    }

    pub fn rect(&self) -> Rect {
        Rect::new(Vec2::new(self.x, self.y), Vec2::new(self.width, self.height))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Desk {
    #[serde(default)]
    pub camera: Option<[f64; 3]>,
    #[serde(default)]
    pub start: Vec<Start>,
    #[serde(default)]
    pub spot: Vec<Spot>,
}

impl Desk {
    pub fn text(&self) -> String {
        toml::to_string(self).unwrap_or_default()
    }

    pub fn read(text: &str) -> Option<Desk> {
        toml::from_str(text).ok()
    }

    pub fn claim(&mut self, app: &str) -> Option<Rect> {
        let at = self.spot.iter().position(|spot| spot.app == app)?;
        Some(self.spot.remove(at).rect())
    }

    pub fn camera(&self) -> Option<crate::Camera> {
        let [x, y, zoom] = self.camera?;
        (x.is_finite() && y.is_finite() && zoom.is_finite() && zoom > 0.0)
            .then(|| crate::Camera::new(Vec2::new(x, y), zoom))
    }
}

pub fn file() -> PathBuf {
    crate::Config::state_dir().join("reopen.toml")
}

fn off_file() -> PathBuf {
    crate::Config::state_dir().join("reopen-off")
}

pub fn wanted() -> bool {
    !off_file().exists()
}

pub fn want(on: bool) {
    let path = off_file();
    let _ = match on {
        true => std::fs::remove_file(path),
        false => std::fs::create_dir_all(crate::Config::state_dir()).and_then(|_| std::fs::write(path, "")),
    };
}

pub fn save(desk: &Desk) -> std::io::Result<()> {
    std::fs::create_dir_all(crate::Config::state_dir())?;
    std::fs::write(file(), desk.text())
}

pub fn take() -> Option<Desk> {
    let path = file();
    let text = std::fs::read_to_string(&path).ok()?;
    let _ = std::fs::remove_file(&path);
    Desk::read(&text)
}

pub fn command_line(raw: &[u8]) -> Vec<String> {
    raw.split(|byte| *byte == 0)
        .filter(|word| !word.is_empty())
        .map(|word| String::from_utf8_lossy(word).into_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desk() -> Desk {
        Desk {
            camera: Some([5000.0, 4000.0, 0.8]),
            start: vec![Start {
                argv: vec!["foot".into()],
                cwd: Some("/home/me/code".into()),
            }],
            spot: vec![
                Spot::new("foot", Rect::new(Vec2::new(10.4, 20.0), Vec2::new(800.0, 600.0))),
                Spot::new("foot", Rect::new(Vec2::new(900.0, 20.0), Vec2::new(640.0, 480.0))),
            ],
        }
    }

    #[test]
    fn a_saved_desk_reads_back_the_same() {
        let desk = desk();
        assert_eq!(Desk::read(&desk.text()), Some(desk));
    }

    #[test]
    fn each_window_of_an_app_takes_the_next_spot_once() {
        let mut desk = desk();
        assert_eq!(desk.claim("foot").map(|rect| rect.origin.x), Some(10.0));
        assert_eq!(desk.claim("foot").map(|rect| rect.origin.x), Some(900.0));
        assert_eq!(desk.claim("foot"), None);
        assert_eq!(desk.claim("firefox"), None);
    }

    #[test]
    fn a_broken_camera_is_ignored() {
        let mut desk = desk();
        assert!(desk.camera().is_some());
        desk.camera = Some([f64::NAN, 0.0, 1.0]);
        assert!(desk.camera().is_none());
        desk.camera = Some([0.0, 0.0, 0.0]);
        assert!(desk.camera().is_none());
    }

    #[test]
    fn the_command_line_splits_on_nul() {
        assert_eq!(command_line(b"foot\0--app-id\0x\0"), ["foot", "--app-id", "x"]);
        assert!(command_line(b"").is_empty());
    }
}
