use std::sync::atomic::{AtomicBool, Ordering};

use smithay::reexports::wayland_protocols::wp::tearing_control::v1::server::{
    wp_tearing_control_manager_v1::{self, WpTearingControlManagerV1},
    wp_tearing_control_v1::{self, PresentationHint, WpTearingControlV1},
};
use smithay::reexports::wayland_server::backend::GlobalId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, WEnum, Weak,
};
use smithay::wayland::compositor::with_states;

use crate::render::State;

#[derive(Default)]
struct Hint {
    held: AtomicBool,
    eager: AtomicBool,
}

pub fn global(display: &DisplayHandle) -> GlobalId {
    display.create_global::<State, WpTearingControlManagerV1, ()>(1, ())
}

pub fn wants_tearing(surface: &WlSurface) -> bool {
    with_states(surface, |states| {
        states
            .data_map
            .get::<Hint>()
            .is_some_and(|hint| hint.eager.load(Ordering::Relaxed))
    })
}

fn hint_of(surface: &WlSurface, held: Option<bool>, eager: Option<bool>) -> bool {
    with_states(surface, |states| {
        states.data_map.insert_if_missing_threadsafe(Hint::default);
        let hint = states.data_map.get::<Hint>().unwrap();
        let was = hint.held.load(Ordering::Relaxed);
        if let Some(held) = held {
            hint.held.store(held, Ordering::Relaxed);
        }
        if let Some(eager) = eager {
            hint.eager.store(eager, Ordering::Relaxed);
        }
        was
    })
}

impl GlobalDispatch<WpTearingControlManagerV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<WpTearingControlManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<WpTearingControlManagerV1, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        manager: &WpTearingControlManagerV1,
        request: wp_tearing_control_manager_v1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let wp_tearing_control_manager_v1::Request::GetTearingControl { id, surface } = request {
            if hint_of(&surface, Some(true), None) {
                manager.post_error(
                    wp_tearing_control_manager_v1::Error::TearingControlExists,
                    "this surface already has a tearing control",
                );
                return;
            }
            data_init.init(id, surface.downgrade());
        }
    }
}

impl Dispatch<WpTearingControlV1, Weak<WlSurface>> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _control: &WpTearingControlV1,
        request: wp_tearing_control_v1::Request,
        surface: &Weak<WlSurface>,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let Ok(surface) = surface.upgrade() else {
            return;
        };
        match request {
            wp_tearing_control_v1::Request::SetPresentationHint { hint } => {
                let eager = matches!(hint, WEnum::Value(PresentationHint::Async));
                hint_of(&surface, None, Some(eager));
                state.kick_now();
            }
            wp_tearing_control_v1::Request::Destroy => {
                hint_of(&surface, Some(false), Some(false));
            }
            _ => {}
        }
    }
}
