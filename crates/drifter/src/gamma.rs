use std::os::fd::OwnedFd;

use smithay::output::Output;
use smithay::reexports::wayland_protocols_wlr::gamma_control::v1::server::{
    zwlr_gamma_control_manager_v1::{self, ZwlrGammaControlManagerV1},
    zwlr_gamma_control_v1::{self, ZwlrGammaControlV1},
};
use smithay::reexports::wayland_server::backend::{ClientId, GlobalId};
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

use crate::render::State;

pub struct Gamma {
    pub control: ZwlrGammaControlV1,
    pub output: Output,
    pub size: usize,
    pub table: Option<Vec<u16>>,
    pub applied: bool,
}

#[derive(Default)]
pub struct Gammas {
    pub held: Vec<Gamma>,
}

impl Gammas {
    pub fn owns(&self, output: &Output) -> bool {
        self.held.iter().any(|gamma| &gamma.output == output && gamma.control.is_alive())
    }

    pub fn unapply(&mut self) {
        for gamma in &mut self.held {
            gamma.applied = false;
        }
    }

    pub fn gone(&mut self, output: &Output) {
        self.held.retain(|gamma| {
            if &gamma.output == output {
                gamma.control.failed();
                return false;
            }
            true
        });
    }

    fn drop_control(&mut self, control: &ZwlrGammaControlV1) -> Option<Output> {
        let at = self.held.iter().position(|gamma| &gamma.control == control)?;
        Some(self.held.remove(at).output)
    }
}

pub fn global(display: &DisplayHandle) -> GlobalId {
    display.create_global::<State, ZwlrGammaControlManagerV1, ()>(1, ())
}

pub fn read_table(fd: OwnedFd, size: usize) -> Result<Vec<u16>, String> {
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::FileExt;

    let want = size * 3 * 2;
    unsafe {
        let flags = libc::fcntl(fd.as_raw_fd(), libc::F_GETFL);
        if flags >= 0 {
            libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }
    let file = std::fs::File::from(fd);
    let mut bytes = vec![0u8; want];
    let mut got = 0;
    while got < want {
        match file.read_at(&mut bytes[got..], got as u64) {
            Ok(0) => break,
            Ok(n) => got += n,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(format!("cannot read the gamma table: {error}")),
        }
    }
    if got != want {
        return Err(format!("the gamma table holds {got} bytes, not {want}"));
    }
    Ok(bytes.as_chunks::<2>().0.iter().map(|pair| u16::from_ne_bytes(*pair)).collect())
}

impl GlobalDispatch<ZwlrGammaControlManagerV1, ()> for State {
    fn can_view(client: Client, _data: &()) -> bool {
        crate::protocols::trusted(&client)
    }

    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrGammaControlManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZwlrGammaControlManagerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _manager: &ZwlrGammaControlManagerV1,
        request: zwlr_gamma_control_manager_v1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let zwlr_gamma_control_manager_v1::Request::GetGammaControl { id, output } = request {
            let control = data_init.init(id, ());
            let Some(output) = Output::from_resource(&output) else {
                control.failed();
                return;
            };
            if state.wayland.extras.gammas.owns(&output) {
                control.failed();
                return;
            }
            let Some(size) = state.gamma_size(&output) else {
                control.failed();
                return;
            };
            control.gamma_size(size as u32);
            state.wayland.extras.gammas.held.push(Gamma { control, output, size, table: None, applied: false });
        }
    }
}

impl Dispatch<ZwlrGammaControlV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        control: &ZwlrGammaControlV1,
        request: zwlr_gamma_control_v1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            zwlr_gamma_control_v1::Request::SetGamma { fd } => {
                let Some(at) = state.wayland.extras.gammas.held.iter().position(|gamma| &gamma.control == control) else {
                    return;
                };
                let size = state.wayland.extras.gammas.held[at].size;
                match read_table(fd, size) {
                    Ok(table) => {
                        let gamma = &mut state.wayland.extras.gammas.held[at];
                        gamma.table = Some(table);
                        gamma.applied = false;
                        state.apply_gammas();
                    }
                    Err(error) => {
                        control.post_error(zwlr_gamma_control_v1::Error::InvalidGamma, error);
                        state.release_gamma(control);
                    }
                }
            }
            zwlr_gamma_control_v1::Request::Destroy => state.release_gamma(control),
            _ => {}
        }
    }

    fn destroyed(state: &mut Self, _client: ClientId, control: &ZwlrGammaControlV1, _data: &()) {
        state.release_gamma(control);
    }
}

impl State {
    pub fn release_gamma(&mut self, control: &ZwlrGammaControlV1) {
        if let Some(output) = self.wayland.extras.gammas.drop_control(control) {
            self.restore_colors(&output);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::read_table;
    use std::io::Write;

    fn table_file(bytes: &[u8]) -> std::os::fd::OwnedFd {
        let mut file = tempfile();
        file.write_all(bytes).unwrap();
        std::os::fd::OwnedFd::from(file)
    }

    fn tempfile() -> std::fs::File {
        let path = std::env::temp_dir().join(format!("drifter-gamma-{}-{}", std::process::id(), rand_suffix()));
        let file = std::fs::OpenOptions::new().read(true).write(true).create_new(true).open(&path).unwrap();
        let _ = std::fs::remove_file(&path);
        file
    }

    fn rand_suffix() -> u128 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    }

    #[test]
    fn a_table_must_hold_three_full_ramps() {
        let ramp: Vec<u8> = (0u16..4).flat_map(|v| (v * 1000).to_ne_bytes()).collect();
        let whole: Vec<u8> = ramp.iter().chain(&ramp).chain(&ramp).copied().collect();
        let table = read_table(table_file(&whole), 4).unwrap();
        assert_eq!(table.len(), 12);
        assert_eq!(table[5], 1000);
        assert!(read_table(table_file(&whole[..20]), 4).is_err());
    }

    #[test]
    fn a_pipe_is_refused_instead_of_blocking() {
        let (reader, mut writer) = std::io::pipe().unwrap();
        writer.write_all(&[7u8; 24]).unwrap();
        assert!(read_table(std::os::fd::OwnedFd::from(reader), 4).is_err());
    }
}
