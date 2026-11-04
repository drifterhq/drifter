use std::path::PathBuf;

use smithay::backend::session::Session;
use smithay::backend::session::libseat::{LibSeatSession, LibSeatSessionNotifier};
use smithay::backend::udev::UdevBackend;

pub struct Seat {
    pub session: LibSeatSession,
    pub notifier: LibSeatSessionNotifier,
    pub name: String,
    pub active: bool,
    pub devices: Vec<PathBuf>,
}

pub fn open() -> Result<Seat, String> {
    let (session, notifier) = LibSeatSession::new().map_err(|e| e.to_string())?;
    let name = session.seat();
    let mut active = session.is_active();
    if !active && std::env::var_os("DRIFTER_GREETED").is_some() {
        active = crate::logind::activate();
    }

    let udev = UdevBackend::new(&name).map_err(|e| e.to_string())?;
    let mut devices: Vec<PathBuf> = udev
        .device_list()
        .map(|(_, path)| path.to_path_buf())
        .collect();
    devices.sort();

    Ok(Seat {
        session,
        notifier,
        name,
        active,
        devices,
    })
}
