use super::*;
use crate::spatial::{Scene, Seat};
use smithay::reexports::wayland_server::Resource;

impl State {
    pub(super) fn place_sound(&mut self) {
        if self.spatial.is_none() {
            return;
        }
        if !self.sound.spatial {
            if let Some(spatial) = self.spatial.as_mut() {
                spatial.tell(Scene {
                    on: false,
                    mix: 0.0,
                    ignore: Vec::new(),
                    seats: Vec::new(),
                });
            }
            return;
        }
        let Some(handle) = self.display.as_ref().map(|display| display.handle()) else {
            return;
        };
        let width = self.screen.x;
        let seats = self
            .windows
            .iter()
            .filter(|window| window.placed && !window.hidden && !window.going)
            .filter_map(|window| {
                let pid = window
                    .toplevel
                    .wl_surface()
                    .client()
                    .and_then(|client| client.get_credentials(&handle).ok())
                    .map(|credentials| credentials.pid)?;
                let center = self.camera.screen_from_world(window.content.center(), self.screen).x;
                let pan = (drifter_core::spatial::pan(center, width) * 100.0).round() / 100.0;
                Some(Seat {
                    pid: u32::try_from(pid).ok()?,
                    app: window.app.clone(),
                    pan,
                })
            })
            .collect();
        let scene = Scene {
            on: true,
            mix: f64::from(self.sound.spatial_mix) / 100.0,
            ignore: self.sound.spatial_ignore.clone(),
            seats,
        };
        if let Some(spatial) = self.spatial.as_mut() {
            spatial.tell(scene);
        }
    }
}
