use super::*;
use drifter_core::reopen::{Desk, Spot, Start};
use smithay::reexports::wayland_server::Resource;

impl State {
    pub(super) fn save_desk(&mut self) {
        let Some(handle) = self.display.as_ref().map(|display| display.handle()) else {
            return;
        };
        let mut desk = Desk {
            camera: Some([self.camera.center.x, self.camera.center.y, self.camera.zoom]),
            ..Desk::default()
        };
        let mut seen = Vec::new();
        for window in &self.windows {
            if window.going || window.app.is_empty() || !window.placed {
                continue;
            }
            let Some(pid) = window
                .toplevel
                .wl_surface()
                .client()
                .and_then(|client| client.get_credentials(&handle).ok())
                .map(|credentials| credentials.pid)
            else {
                continue;
            };
            if !seen.contains(&pid) {
                let Some(start) = start_of(&window.app, pid) else {
                    continue;
                };
                seen.push(pid);
                desk.start.push(start);
            }
            let frame = window
                .full
                .map(|(before, _)| before)
                .or(window.restore)
                .unwrap_or(window.content);
            desk.spot.push(Spot::new(&window.app, frame));
        }
        match drifter_core::reopen::save(&desk) {
            Ok(()) => out!("reopen      saved {} windows", desk.spot.len()),
            Err(error) => log!("drifter: cannot save the windows to reopen: {error}"),
        }
    }

    pub(super) fn reopen_desk(&mut self) {
        let Some(desk) = drifter_core::reopen::take() else {
            return;
        };
        if let Some(camera) = desk.camera() {
            self.camera = camera;
            self.camera.clamp_to_world(self.world, self.screen);
        }
        for start in &desk.start {
            crate::apps::launch_in(&start.argv, start.cwd.as_deref(), &self.socket);
        }
        self.reopening = Some((desk, Instant::now()));
        self.kick_now();
    }

    pub(super) fn reopen_spot(&mut self, app: &str) -> Option<Rect> {
        let (desk, since) = self.reopening.as_mut()?;
        if since.elapsed() > drifter_core::reopen::WAIT {
            self.reopening = None;
            return None;
        }
        let spot = desk.claim(app);
        if desk.spot.is_empty() {
            self.reopening = None;
        }
        spot
    }
}

fn start_of(app: &str, pid: i32) -> Option<Start> {
    let argv = crate::apps::find(app)
        .and_then(|entry| entry.exec)
        .and_then(|exec| drifter_core::exec::command(&exec, &[]))
        .filter(|argv| !argv.is_empty())
        .or_else(|| {
            let raw = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
            Some(drifter_core::reopen::command_line(&raw)).filter(|argv| !argv.is_empty())
        })?;
    let child = std::fs::read_to_string(format!("/proc/{pid}/task/{pid}/children"))
        .ok()
        .and_then(|text| text.split_whitespace().next().map(str::to_string));
    let cwd = child
        .and_then(|child| std::fs::read_link(format!("/proc/{child}/cwd")).ok())
        .or_else(|| std::fs::read_link(format!("/proc/{pid}/cwd")).ok())
        .map(|path| path.to_string_lossy().into_owned());
    Some(Start { argv, cwd })
}
