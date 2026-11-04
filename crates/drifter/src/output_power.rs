use smithay::output::Output;
use smithay::reexports::wayland_protocols_wlr::output_power_management::v1::server::{
    zwlr_output_power_manager_v1::{self, ZwlrOutputPowerManagerV1},
    zwlr_output_power_v1::{self, Mode, ZwlrOutputPowerV1},
};
use smithay::reexports::wayland_server::backend::GlobalId;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, WEnum,
};

use crate::render::State;

#[derive(Default)]
pub struct Powers {
    held: Vec<(ZwlrOutputPowerV1, Output)>,
}

impl Powers {
    pub fn tell(&mut self, output: &Output, on: bool) {
        self.held.retain(|(power, _)| power.is_alive());
        let mode = if on { Mode::On } else { Mode::Off };
        for (power, held) in &self.held {
            if held == output {
                power.mode(mode);
            }
        }
    }

    pub fn gone(&mut self, output: &Output) {
        self.held.retain(|(power, held)| {
            if held == output {
                power.failed();
                return false;
            }
            power.is_alive()
        });
    }
}

pub fn global(display: &DisplayHandle) -> GlobalId {
    display.create_global::<State, ZwlrOutputPowerManagerV1, ()>(1, ())
}

impl GlobalDispatch<ZwlrOutputPowerManagerV1, ()> for State {
    fn can_view(client: Client, _data: &()) -> bool {
        crate::protocols::trusted(&client)
    }

    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrOutputPowerManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZwlrOutputPowerManagerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _manager: &ZwlrOutputPowerManagerV1,
        request: zwlr_output_power_manager_v1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let zwlr_output_power_manager_v1::Request::GetOutputPower { id, output } = request {
            let found = Output::from_resource(&output).and_then(|output| state.output_lit(&output).map(|on| (output, on)));
            let power = data_init.init(id, ());
            match found {
                Some((output, on)) => {
                    power.mode(if on { Mode::On } else { Mode::Off });
                    state.wayland.extras.powers.held.push((power, output));
                }
                None => power.failed(),
            }
        }
    }
}

impl Dispatch<ZwlrOutputPowerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        power: &ZwlrOutputPowerV1,
        request: zwlr_output_power_v1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            zwlr_output_power_v1::Request::SetMode { mode } => {
                let on = match mode {
                    WEnum::Value(Mode::On) => true,
                    WEnum::Value(Mode::Off) => false,
                    _ => {
                        power.post_error(zwlr_output_power_v1::Error::InvalidMode, "there is no such power mode");
                        return;
                    }
                };
                let output = state
                    .wayland
                    .extras
                    .powers
                    .held
                    .iter()
                    .find(|(held, _)| held == power)
                    .map(|(_, output)| output.clone());
                let Some(output) = output else {
                    return;
                };
                if !state.set_output_power(&output, on) {
                    state.wayland.extras.powers.gone(&output);
                }
            }
            zwlr_output_power_v1::Request::Destroy => {
                state.wayland.extras.powers.held.retain(|(held, _)| held != power);
            }
            _ => {}
        }
    }

    fn destroyed(state: &mut Self, _client: smithay::reexports::wayland_server::backend::ClientId, power: &ZwlrOutputPowerV1, _data: &()) {
        state.wayland.extras.powers.held.retain(|(held, _)| held != power);
    }
}
