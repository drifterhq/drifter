use std::path::PathBuf;
use std::time::{Duration, Instant};

use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::session::{Event as SessionEvent, Session};
use smithay::backend::udev::UdevBackend;
use smithay::reexports::calloop::EventLoop;

pub struct Bootstrap {
    pub seat: String,
    pub active: bool,
    pub devices: Vec<PathBuf>,
    pub paused: u32,
    pub activated: u32,
}

struct State {
    paused: u32,
    activated: u32,
}

pub fn bootstrap(watch: Duration) -> Result<Bootstrap, String> {
    let (session, notifier) = LibSeatSession::new().map_err(|e| e.to_string())?;
    let seat = session.seat();
    let active = session.is_active();

    let udev = UdevBackend::new(&seat).map_err(|e| e.to_string())?;
    let mut devices: Vec<PathBuf> = udev
        .device_list()
        .map(|(_, path)| path.to_path_buf())
        .collect();
    devices.sort();

    let mut event_loop = EventLoop::<State>::try_new().map_err(|e| e.to_string())?;
    event_loop
        .handle()
        .insert_source(notifier, |event, _, state: &mut State| match event {
            SessionEvent::PauseSession => state.paused += 1,
            SessionEvent::ActivateSession => state.activated += 1,
        })
        .map_err(|e| e.to_string())?;

    let mut state = State {
        paused: 0,
        activated: 0,
    };
    let deadline = Instant::now() + watch;
    while Instant::now() < deadline {
        event_loop
            .dispatch(Some(Duration::from_millis(100)), &mut state)
            .map_err(|e| e.to_string())?;
    }

    Ok(Bootstrap {
        seat,
        active,
        devices,
        paused: state.paused,
        activated: state.activated,
    })
}
