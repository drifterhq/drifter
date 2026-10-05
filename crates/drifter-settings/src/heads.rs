use drifter_core::mode::Mode;
use drifter_core::Vec2;
use wayland_client::globals::GlobalList;
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
use wayland_protocols_wlr::output_management::v1::client::{
    zwlr_output_head_v1::{self, ZwlrOutputHeadV1},
    zwlr_output_manager_v1::{self, ZwlrOutputManagerV1},
    zwlr_output_mode_v1::{self, ZwlrOutputModeV1},
};

use crate::window::Settings;

#[derive(Default, Clone)]
struct Head {
    name: String,
    enabled: bool,
    current: Option<ZwlrOutputModeV1>,
    modes: Vec<ZwlrOutputModeV1>,
}

#[derive(Default)]
pub struct Heads {
    _manager: Option<ZwlrOutputManagerV1>,
    heads: Vec<(ZwlrOutputHeadV1, Head)>,
    modes: Vec<(ZwlrOutputModeV1, Mode)>,
    pub ready: bool,
}

pub struct Found {
    pub name: String,
    pub pixels: Vec2,
    pub modes: Vec<Mode>,
    pub current: Option<Mode>,
}

impl Heads {
    pub fn bind(globals: &GlobalList, qh: &QueueHandle<Settings>) -> Heads {
        Heads {
            _manager: globals.bind(qh, 1..=3, ()).ok(),
            ..Heads::default()
        }
    }

    fn head(&mut self, head: &ZwlrOutputHeadV1) -> &mut Head {
        let at = match self.heads.iter().position(|(each, _)| each == head) {
            Some(at) => at,
            None => {
                self.heads.push((head.clone(), Head::default()));
                self.heads.len() - 1
            }
        };
        &mut self.heads[at].1
    }

    fn mode(&mut self, mode: &ZwlrOutputModeV1) -> &mut Mode {
        let at = match self.modes.iter().position(|(each, _)| each == mode) {
            Some(at) => at,
            None => {
                self.modes.push((mode.clone(), Mode::new(0, 0, 0, false)));
                self.modes.len() - 1
            }
        };
        &mut self.modes[at].1
    }

    pub fn found(&self) -> Option<Vec<Found>> {
        if !self.ready || self.heads.is_empty() {
            return None;
        }
        let read = |proxy: &ZwlrOutputModeV1| {
            self.modes
                .iter()
                .find(|(each, _)| each == proxy)
                .map(|(_, mode)| *mode)
        };
        Some(
            self.heads
                .iter()
                .map(|(_, head)| {
                    let modes: Vec<Mode> = head.modes.iter().filter_map(read).collect();
                    let current = head.current.as_ref().and_then(read).filter(|_| head.enabled);
                    let shown = current
                        .or_else(|| modes.iter().copied().find(|mode| mode.preferred))
                        .or_else(|| modes.first().copied());
                    Found {
                        name: head.name.clone(),
                        pixels: shown
                            .map(|mode| Vec2::new(f64::from(mode.width), f64::from(mode.height)))
                            .unwrap_or(Vec2::ZERO),
                        modes,
                        current,
                    }
                })
                .collect(),
        )
    }
}

impl Dispatch<ZwlrOutputManagerV1, ()> for Settings {
    fn event(
        settings: &mut Self,
        _: &ZwlrOutputManagerV1,
        event: zwlr_output_manager_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_output_manager_v1::Event::Head { head } => {
                settings.heads.head(&head);
            }
            zwlr_output_manager_v1::Event::Done { .. } => {
                settings.heads.ready = true;
                settings.heads_changed();
            }
            zwlr_output_manager_v1::Event::Finished => {
                settings.heads = Heads::default();
            }
            _ => {}
        }
    }

    event_created_child!(Settings, ZwlrOutputManagerV1, [
        zwlr_output_manager_v1::EVT_HEAD_OPCODE => (ZwlrOutputHeadV1, ()),
    ]);
}

impl Dispatch<ZwlrOutputHeadV1, ()> for Settings {
    fn event(
        settings: &mut Self,
        proxy: &ZwlrOutputHeadV1,
        event: zwlr_output_head_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let heads = &mut settings.heads;
        match event {
            zwlr_output_head_v1::Event::Name { name } => heads.head(proxy).name = name,
            zwlr_output_head_v1::Event::Enabled { enabled } => heads.head(proxy).enabled = enabled != 0,
            zwlr_output_head_v1::Event::Mode { mode } => {
                heads.mode(&mode);
                heads.head(proxy).modes.push(mode);
            }
            zwlr_output_head_v1::Event::CurrentMode { mode } => heads.head(proxy).current = Some(mode),
            zwlr_output_head_v1::Event::Finished => {
                heads.heads.retain(|(each, _)| each != proxy);
                if proxy.version() >= 3 {
                    proxy.release();
                }
            }
            _ => {}
        }
    }

    event_created_child!(Settings, ZwlrOutputHeadV1, [
        zwlr_output_head_v1::EVT_MODE_OPCODE => (ZwlrOutputModeV1, ()),
    ]);
}

impl Dispatch<ZwlrOutputModeV1, ()> for Settings {
    fn event(
        settings: &mut Self,
        proxy: &ZwlrOutputModeV1,
        event: zwlr_output_mode_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let heads = &mut settings.heads;
        match event {
            zwlr_output_mode_v1::Event::Size { width, height } => {
                let mode = heads.mode(proxy);
                mode.width = width.max(0) as u32;
                mode.height = height.max(0) as u32;
            }
            zwlr_output_mode_v1::Event::Refresh { refresh } => {
                heads.mode(proxy).refresh_mhz = refresh.max(0) as u32
            }
            zwlr_output_mode_v1::Event::Preferred => heads.mode(proxy).preferred = true,
            zwlr_output_mode_v1::Event::Finished => {
                heads.modes.retain(|(each, _)| each != proxy);
                if proxy.version() >= 3 {
                    proxy.release();
                }
            }
            _ => {}
        }
    }
}
