use smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_handle_v1::{
    self, ZwlrForeignToplevelHandleV1,
};
use smithay::reexports::wayland_protocols_wlr::foreign_toplevel::v1::server::zwlr_foreign_toplevel_manager_v1::{
    self, ZwlrForeignToplevelManagerV1,
};
use smithay::reexports::wayland_server::backend::GlobalId;
use smithay::reexports::wayland_server::protocol::wl_output::WlOutput;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

use crate::render::State;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ask {
    Maximize(bool),
    Minimize(bool),
    Fullscreen(bool),
    Activate,
    Close,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Shown {
    pub title: String,
    pub app: String,
    pub maximized: bool,
    pub minimized: bool,
    pub activated: bool,
    pub fullscreen: bool,
}

impl Shown {
    fn states(&self) -> Vec<u8> {
        use zwlr_foreign_toplevel_handle_v1::State as S;
        [
            (self.maximized, S::Maximized),
            (self.minimized, S::Minimized),
            (self.activated, S::Activated),
            (self.fullscreen, S::Fullscreen),
        ]
        .into_iter()
        .filter(|(on, _)| *on)
        .flat_map(|(_, state)| (state as u32).to_ne_bytes())
        .collect()
    }
}

struct Held {
    window: u64,
    handle: ZwlrForeignToplevelHandleV1,
    sent: Option<Shown>,
    outputs: Vec<WlOutput>,
}

#[derive(Default)]
pub struct Toplevels {
    managers: Vec<ZwlrForeignToplevelManagerV1>,
    held: Vec<Held>,
    asks: Vec<(u64, Ask)>,
}

pub fn global(display: &DisplayHandle) -> GlobalId {
    display.create_global::<State, ZwlrForeignToplevelManagerV1, ()>(3, ())
}

impl Toplevels {
    pub fn take_asks(&mut self) -> Vec<(u64, Ask)> {
        std::mem::take(&mut self.asks)
    }

    pub fn sync(
        &mut self,
        display: &DisplayHandle,
        windows: &[(u64, Shown)],
        output: &smithay::output::Output,
    ) {
        self.managers.retain(|manager| manager.is_alive());
        self.held.retain(|held| {
            let alive = held.handle.is_alive();
            let live = windows.iter().any(|(id, _)| *id == held.window);
            if alive && !live {
                held.handle.closed();
            }
            alive && live
        });
        for manager in &self.managers {
            let Some(client) = manager.client() else {
                continue;
            };
            for (id, _) in windows {
                let known = self
                    .held
                    .iter()
                    .any(|held| held.window == *id && held.handle.client().as_ref() == Some(&client));
                if known {
                    continue;
                }
                let Ok(handle) = client.create_resource::<ZwlrForeignToplevelHandleV1, u64, State>(
                    display,
                    manager.version(),
                    *id,
                ) else {
                    continue;
                };
                manager.toplevel(&handle);
                self.held.push(Held {
                    window: *id,
                    handle,
                    sent: None,
                    outputs: Vec::new(),
                });
            }
        }
        for held in &mut self.held {
            let Some((_, now)) = windows.iter().find(|(id, _)| *id == held.window) else {
                continue;
            };
            let outputs: Vec<WlOutput> = held
                .handle
                .client()
                .map(|client| output.client_outputs(&client).collect())
                .unwrap_or_default();
            let mut changed = false;
            for gone in held.outputs.iter().filter(|o| !outputs.contains(o)) {
                held.handle.output_leave(gone);
                changed = true;
            }
            for new in outputs.iter().filter(|o| !held.outputs.contains(o)) {
                held.handle.output_enter(new);
                changed = true;
            }
            held.outputs = outputs;
            let before = held.sent.clone().unwrap_or_default();
            let first = held.sent.is_none();
            if first || before.title != now.title {
                held.handle.title(now.title.clone());
                changed = true;
            }
            if first || before.app != now.app {
                held.handle.app_id(now.app.clone());
                changed = true;
            }
            if first || before.states() != now.states() {
                held.handle.state(now.states());
                changed = true;
            }
            if changed {
                held.handle.done();
            }
            held.sent = Some(now.clone());
        }
    }
}

impl GlobalDispatch<ZwlrForeignToplevelManagerV1, ()> for State {
    fn can_view(client: Client, _data: &()) -> bool {
        crate::protocols::trusted(&client)
    }

    fn bind(
        state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrForeignToplevelManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        let manager = data_init.init(resource, ());
        state.wayland.extras.wlr_toplevels.managers.push(manager);
        state.sync_wlr_toplevels();
    }
}

impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        manager: &ZwlrForeignToplevelManagerV1,
        request: zwlr_foreign_toplevel_manager_v1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let zwlr_foreign_toplevel_manager_v1::Request::Stop = request {
            state
                .wayland
                .extras
                .wlr_toplevels
                .managers
                .retain(|kept| kept != manager);
            manager.finished();
        }
    }
}

impl Dispatch<ZwlrForeignToplevelHandleV1, u64> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        handle: &ZwlrForeignToplevelHandleV1,
        request: zwlr_foreign_toplevel_handle_v1::Request,
        window: &u64,
        _display: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        use zwlr_foreign_toplevel_handle_v1::Request as R;

        let ask = match request {
            R::SetMaximized => Ask::Maximize(true),
            R::UnsetMaximized => Ask::Maximize(false),
            R::SetMinimized => Ask::Minimize(true),
            R::UnsetMinimized => Ask::Minimize(false),
            R::SetFullscreen { .. } => Ask::Fullscreen(true),
            R::UnsetFullscreen => Ask::Fullscreen(false),
            R::Activate { .. } => Ask::Activate,
            R::Close => Ask::Close,
            R::Destroy => {
                state
                    .wayland
                    .extras
                    .wlr_toplevels
                    .held
                    .retain(|held| &held.handle != handle);
                return;
            }
            _ => return,
        };
        state.wayland.extras.wlr_toplevels.asks.push((*window, ask));
        state.answer_wlr_toplevels();
    }
}
