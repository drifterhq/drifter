use std::sync::{Arc, Mutex};

use drifter_core::mode::Mode;
use smithay::reexports::wayland_protocols_wlr::output_management::v1::server::{
    zwlr_output_configuration_head_v1::{self, ZwlrOutputConfigurationHeadV1},
    zwlr_output_configuration_v1::{self, ZwlrOutputConfigurationV1},
    zwlr_output_head_v1::{self, ZwlrOutputHeadV1},
    zwlr_output_manager_v1::{self, ZwlrOutputManagerV1},
    zwlr_output_mode_v1::{self, ZwlrOutputModeV1},
};
use smithay::reexports::wayland_server::backend::GlobalId;
use smithay::reexports::wayland_server::protocol::wl_output::Transform;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, WEnum,
};

use crate::render::State;

#[derive(Clone, Debug, PartialEq)]
pub struct Seen {
    pub name: String,
    pub millimeters: (i32, i32),
    pub modes: Vec<Mode>,
    pub current: Option<Mode>,
    pub enabled: bool,
    pub position: (i32, i32),
    pub rotate: u32,
    pub scale: f64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Want {
    pub mode: Option<Mode>,
    pub position: Option<(i32, i32)>,
    pub rotate: Option<u32>,
    pub scale: Option<f64>,
    pub refused: bool,
}

pub type Change = (String, Option<Arc<Mutex<Want>>>);

pub struct Pending {
    serial: u32,
    changes: Vec<Change>,
    used: bool,
}

struct Sent {
    manager: ZwlrOutputManagerV1,
    head: ZwlrOutputHeadV1,
    modes: Vec<ZwlrOutputModeV1>,
    seen: Seen,
}

#[derive(Default)]
pub struct Outputs {
    managers: Vec<ZwlrOutputManagerV1>,
    sent: Vec<Sent>,
    last: Vec<Seen>,
    pub serial: u32,
}

pub fn global(display: &DisplayHandle) -> GlobalId {
    display.create_global::<State, ZwlrOutputManagerV1, ()>(3, ())
}

fn transform(rotate: u32) -> Transform {
    match rotate {
        90 => Transform::_90,
        180 => Transform::_180,
        270 => Transform::_270,
        _ => Transform::Normal,
    }
}

fn rotate_of(transform: Transform) -> Option<u32> {
    match transform {
        Transform::Normal => Some(0),
        Transform::_90 => Some(90),
        Transform::_180 => Some(180),
        Transform::_270 => Some(270),
        _ => None,
    }
}

impl Outputs {
    pub fn sync(&mut self, display: &DisplayHandle, seen: Vec<Seen>, force: bool) {
        self.managers.retain(|manager| manager.is_alive());
        self.sent.retain(|sent| sent.manager.is_alive() && sent.head.is_alive());
        let changed = seen != self.last;
        if changed {
            self.serial = self.serial.wrapping_add(1);
            self.last = seen;
        }
        if !changed && !force {
            return;
        }
        let serial = self.serial;
        let managers = self.managers.clone();
        for manager in managers {
            let mut touched = false;
            let mut index = self.sent.len();
            while index > 0 {
                index -= 1;
                let sent = &self.sent[index];
                if sent.manager != manager {
                    continue;
                }
                let now = self.last.iter().find(|each| each.name == sent.seen.name);
                if now.is_some_and(|now| now.modes == sent.seen.modes) {
                    continue;
                }
                let sent = self.sent.remove(index);
                for mode in &sent.modes {
                    mode.finished();
                }
                sent.head.finished();
                touched = true;
            }
            for now in self.last.clone() {
                let known = self
                    .sent
                    .iter()
                    .position(|sent| sent.manager == manager && sent.seen.name == now.name);
                match known {
                    Some(at) => touched |= update(&mut self.sent[at], &now),
                    None => {
                        if let Some(sent) = announce(display, &manager, &now) {
                            self.sent.push(sent);
                            touched = true;
                        }
                    }
                }
            }
            if touched || force {
                manager.done(serial);
            }
        }
    }
}

fn announce(display: &DisplayHandle, manager: &ZwlrOutputManagerV1, seen: &Seen) -> Option<Sent> {
    let client = manager.client()?;
    let version = manager.version();
    let head = client
        .create_resource::<ZwlrOutputHeadV1, String, State>(display, version, seen.name.clone())
        .ok()?;
    manager.head(&head);
    head.name(seen.name.clone());
    head.description(format!("Drifter {}", seen.name));
    if seen.millimeters.0 > 0 && seen.millimeters.1 > 0 {
        head.physical_size(seen.millimeters.0, seen.millimeters.1);
    }
    let mut modes = Vec::new();
    for mode in &seen.modes {
        let Ok(made) =
            client.create_resource::<ZwlrOutputModeV1, Mode, State>(display, version, *mode)
        else {
            continue;
        };
        head.mode(&made);
        made.size(mode.width as i32, mode.height as i32);
        made.refresh(mode.refresh_mhz as i32);
        if mode.preferred {
            made.preferred();
        }
        modes.push(made);
    }
    if version >= 2 {
        head.make("Drifter".to_string());
        head.model(seen.name.clone());
    }
    let mut sent = Sent {
        manager: manager.clone(),
        head,
        modes,
        seen: Seen {
            enabled: !seen.enabled,
            ..seen.clone()
        },
    };
    update(&mut sent, seen);
    Some(sent)
}

fn update(sent: &mut Sent, now: &Seen) -> bool {
    let before = sent.seen.clone();
    let mut touched = false;
    if before.enabled != now.enabled {
        sent.head.enabled(i32::from(now.enabled));
        touched = true;
    }
    if now.enabled {
        let fresh = touched;
        if (fresh || before.current != now.current)
            && let Some(current) = now.current
            && let Some(at) = now.modes.iter().position(|mode| *mode == current)
        {
            sent.head.current_mode(&sent.modes[at]);
            touched = true;
        }
        if fresh || before.position != now.position {
            sent.head.position(now.position.0, now.position.1);
            touched = true;
        }
        if fresh || before.rotate != now.rotate {
            sent.head.transform(transform(now.rotate));
            touched = true;
        }
        if fresh || (before.scale - now.scale).abs() > 1e-6 {
            sent.head.scale(now.scale);
            touched = true;
        }
    }
    sent.seen = now.clone();
    touched
}

impl GlobalDispatch<ZwlrOutputManagerV1, ()> for State {
    fn can_view(client: Client, _data: &()) -> bool {
        crate::protocols::trusted(&client)
    }

    fn bind(
        state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrOutputManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        let manager = data_init.init(resource, ());
        state.wayland.extras.wlr_outputs.managers.push(manager);
        state.sync_wlr_outputs(true);
    }
}

impl Dispatch<ZwlrOutputManagerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        manager: &ZwlrOutputManagerV1,
        request: zwlr_output_manager_v1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            zwlr_output_manager_v1::Request::CreateConfiguration { id, serial } => {
                data_init.init(
                    id,
                    Mutex::new(Pending {
                        serial,
                        changes: Vec::new(),
                        used: false,
                    }),
                );
            }
            zwlr_output_manager_v1::Request::Stop => {
                let outputs = &mut state.wayland.extras.wlr_outputs;
                outputs.managers.retain(|kept| kept != manager);
                outputs.sent.retain(|sent| &sent.manager != manager);
                manager.finished();
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrOutputHeadV1, String> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        head: &ZwlrOutputHeadV1,
        request: zwlr_output_head_v1::Request,
        _data: &String,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        if let zwlr_output_head_v1::Request::Release = request {
            state
                .wayland
                .extras
                .wlr_outputs
                .sent
                .retain(|sent| &sent.head != head);
        }
    }
}

impl Dispatch<ZwlrOutputModeV1, Mode> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _mode: &ZwlrOutputModeV1,
        _request: zwlr_output_mode_v1::Request,
        _data: &Mode,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

impl Dispatch<ZwlrOutputConfigurationV1, Mutex<Pending>> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        config: &ZwlrOutputConfigurationV1,
        request: zwlr_output_configuration_v1::Request,
        data: &Mutex<Pending>,
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        use zwlr_output_configuration_v1::{Error, Request};

        let mut pending = data.lock().unwrap();
        let named = |head: &ZwlrOutputHeadV1| head.data::<String>().cloned().unwrap_or_default();
        match request {
            Request::EnableHead { id, head } => {
                let name = named(&head);
                let want = Arc::new(Mutex::new(Want::default()));
                data_init.init(id, want.clone());
                if pending.changes.iter().any(|(each, _)| *each == name) {
                    config.post_error(Error::AlreadyConfiguredHead, "head configured twice");
                    return;
                }
                pending.changes.push((name, Some(want)));
            }
            Request::DisableHead { head } => {
                let name = named(&head);
                if pending.changes.iter().any(|(each, _)| *each == name) {
                    config.post_error(Error::AlreadyConfiguredHead, "head configured twice");
                    return;
                }
                pending.changes.push((name, None));
            }
            Request::Apply | Request::Test => {
                if pending.used {
                    config.post_error(Error::AlreadyUsed, "configuration used twice");
                    return;
                }
                pending.used = true;
                let only_test = matches!(request, Request::Test);
                let changes = pending.changes.clone();
                let serial = pending.serial;
                drop(pending);
                match state.configure_outputs(serial, &changes, only_test) {
                    Answer::Done => config.succeeded(),
                    Answer::Refused => config.failed(),
                    Answer::Stale => config.cancelled(),
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<ZwlrOutputConfigurationHeadV1, Arc<Mutex<Want>>> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _head: &ZwlrOutputConfigurationHeadV1,
        request: zwlr_output_configuration_head_v1::Request,
        data: &Arc<Mutex<Want>>,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        use zwlr_output_configuration_head_v1::Request;

        let mut want = data.lock().unwrap();
        match request {
            Request::SetMode { mode } => want.mode = mode.data::<Mode>().copied(),
            Request::SetCustomMode {
                width,
                height,
                refresh,
            } => {
                want.mode = Some(Mode::new(
                    width.max(0) as u32,
                    height.max(0) as u32,
                    refresh.max(0) as u32,
                    false,
                ));
            }
            Request::SetPosition { x, y } => want.position = Some((x, y)),
            Request::SetTransform { transform } => match transform {
                WEnum::Value(turned) => match rotate_of(turned) {
                    Some(rotate) => want.rotate = Some(rotate),
                    None => want.refused = true,
                },
                WEnum::Unknown(_) => want.refused = true,
            },
            Request::SetScale { scale } => want.scale = Some(scale),
            _ => {}
        }
    }
}

pub enum Answer {
    Done,
    Refused,
    Stale,
}

pub fn plan(
    monitors: &[drifter_core::screens::MonitorConfig],
    seen: &[Seen],
    changes: &[(String, Option<Want>)],
) -> Result<Vec<drifter_core::screens::MonitorConfig>, String> {
    use drifter_core::screens::MonitorConfig;

    let mut out = monitors.to_vec();
    for (name, want) in changes {
        let Some(head) = seen.iter().find(|each| each.name == *name) else {
            return Err(format!("no head named {name}"));
        };
        let at = match out.iter().position(|each| each.name == *name) {
            Some(at) => at,
            None => {
                let mut fresh = MonitorConfig::named(name);
                fresh.scale = head.scale;
                fresh.rotate = head.rotate;
                out.push(fresh);
                out.len() - 1
            }
        };
        let entry = &mut out[at];
        let Some(want) = want else {
            entry.enabled = false;
            continue;
        };
        if want.refused {
            return Err(format!("{name} cannot be flipped"));
        }
        entry.enabled = true;
        if let Some(mode) = want.mode {
            let close = head.modes.iter().any(|each| {
                each.width == mode.width
                    && each.height == mode.height
                    && each.refresh_mhz.abs_diff(mode.refresh_mhz) <= 1000
            });
            if !close {
                return Err(format!("{name} has no mode {}", mode.text()));
            }
            entry.mode = Some(mode.text());
        }
        if let Some((x, y)) = want.position {
            entry.x = Some(i64::from(x));
            entry.y = Some(i64::from(y));
        }
        if let Some(rotate) = want.rotate {
            entry.rotate = rotate;
        }
        if let Some(scale) = want.scale {
            entry.scale = scale;
        }
        entry.check()?;
    }
    let on = |name: &str| {
        out.iter()
            .find(|each| each.name == name)
            .is_none_or(|each| each.enabled)
    };
    if !seen.iter().any(|head| on(&head.name)) {
        return Err("at least one monitor stays on".to_string());
    }
    Ok(out)
}
