use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use smithay::reexports::calloop::channel::{Channel, Sender as LoopSender, channel as loop_channel};

const NAME: &str = "org.drifter.AirDrop";
const PATH: &str = "/org/drifter/AirDrop";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    Send(Vec<PathBuf>),
    Receive(bool),
    Refresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Shared {
    pub shown: bool,
    pub ready: bool,
    pub receiving: bool,
}

pub fn start(shared: Arc<Mutex<Shared>>) -> Result<Channel<Event>, String> {
    let (events, heard) = loop_channel::<Event>();
    std::thread::Builder::new()
        .name("drifter-airdrop".to_string())
        .spawn(move || serve(events, shared))
        .map_err(|e| format!("cannot start the AirDrop service: {e}"))?;
    Ok(heard)
}

fn serve(events: LoopSender<Event>, shared: Arc<Mutex<Shared>>) {
    let address = match crate::tray::bus_address() {
        Ok(address) => address,
        Err(error) => {
            log!("drifter: no AirDrop service: {error}");
            return;
        }
    };
    let built = zbus::blocking::connection::Builder::address(address.as_str())
        .and_then(|builder| builder.name(NAME))
        .and_then(|builder| builder.serve_at(PATH, Server { events, shared }))
        .and_then(|builder| builder.build());
    let _connection = match built {
        Ok(connection) => connection,
        Err(error) => {
            log!("drifter: no AirDrop service: {error}");
            return;
        }
    };
    loop {
        std::thread::park();
    }
}

struct Server {
    events: LoopSender<Event>,
    shared: Arc<Mutex<Shared>>,
}

impl Server {
    fn now(&self) -> Shared {
        self.shared.lock().map(|held| *held).unwrap_or_default()
    }
}

#[zbus::interface(name = "org.drifter.AirDrop1")]
impl Server {
    fn send(&self, uris: Vec<String>) -> zbus::fdo::Result<()> {
        let files: Vec<PathBuf> = uris
            .iter()
            .filter_map(|uri| match uri.starts_with("file://") {
                true => drifter_core::explorer::clip::file_path(uri),
                false => uri.starts_with('/').then(|| PathBuf::from(uri)),
            })
            .filter(|path| path.is_file())
            .collect();
        if files.is_empty() {
            return Err(zbus::fdo::Error::InvalidArgs("no files to send".into()));
        }
        if !self.now().shown {
            return Err(zbus::fdo::Error::NotSupported("AirDrop is not available here".into()));
        }
        let _ = self.events.send(Event::Send(files));
        Ok(())
    }

    fn set_receiving(&self, on: bool) {
        let _ = self.events.send(Event::Receive(on));
    }

    fn refresh(&self) {
        let _ = self.events.send(Event::Refresh);
    }

    fn state(&self) -> (bool, bool, bool) {
        let now = self.now();
        (now.shown, now.ready, now.receiving)
    }
}

