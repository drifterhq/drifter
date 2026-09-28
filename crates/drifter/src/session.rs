use std::time::{Duration, Instant};

use smithay::backend::session::Session;
use smithay::backend::session::libseat::LibSeatSession;
use smithay::backend::udev::UdevBackend;
use smithay::reexports::calloop::EventLoop;

pub struct Bootstrap {
    pub seat: String,
    pub active: bool,
    pub devices: usize,
    pub events: u32,
}

struct State {
    events: u32,
}

pub fn bootstrap(watch: Duration) -> Result<Bootstrap, String> {
    let (session, notifier) = LibSeatSession::new().map_err(|e| e.to_string())?;
    let seat = session.seat();
    let active = session.is_active();

    let udev = UdevBackend::new(&seat).map_err(|e| e.to_string())?;
    let devices = udev.device_list().count();

    let mut event_loop = EventLoop::<State>::try_new().map_err(|e| e.to_string())?;
    event_loop
        .handle()
        .insert_source(notifier, |_event, _, state: &mut State| {
            state.events += 1;
        })
        .map_err(|e| e.to_string())?;

    let mut state = State { events: 0 };
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
        events: state.events,
    })
}
