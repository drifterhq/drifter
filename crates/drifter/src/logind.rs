use std::time::Duration;

use smithay::reexports::calloop::channel::{Channel, Sender, channel};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedFd, OwnedObjectPath};

const SERVICE: &str = "org.freedesktop.login1";
const PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";
const SESSION: &str = "org.freedesktop.login1.Session";
const LOCK_TIME: Duration = Duration::from_millis(400);

pub enum Event {
    Lock,
    Sleep,
    Woke,
}

pub fn start() -> Result<Channel<Event>, String> {
    let (tx, rx) = channel::<Event>();
    let bus = Connection::system().map_err(|e| format!("no system bus: {e}"))?;
    let path = session_path(&bus)?;

    let lock_bus = bus.clone();
    let lock_tx = tx.clone();
    std::thread::Builder::new()
        .name("drifter-logind-lock".to_string())
        .spawn(move || {
            if let Err(error) = watch_lock(&lock_bus, path, lock_tx) {
                log!("drifter: logind lock requests are not seen: {error}");
            }
        })
        .map_err(|e| format!("cannot start the logind thread: {e}"))?;

    std::thread::Builder::new()
        .name("drifter-logind-sleep".to_string())
        .spawn(move || {
            if let Err(error) = watch_sleep(&bus, tx) {
                log!("drifter: the screen does not lock before sleep: {error}");
            }
        })
        .map_err(|e| format!("cannot start the logind thread: {e}"))?;
    Ok(rx)
}

fn session_path(bus: &Connection) -> Result<OwnedObjectPath, String> {
    let manager = Proxy::new(bus, SERVICE, PATH, MANAGER).map_err(|e| e.to_string())?;
    let found = match std::env::var("XDG_SESSION_ID") {
        Ok(id) if !id.is_empty() => manager.call("GetSession", &(id,)),
        _ => manager.call("GetSessionByPID", &(std::process::id(),)),
    };
    found.map_err(|e| format!("no login session: {e}"))
}

fn watch_lock(bus: &Connection, path: OwnedObjectPath, tx: Sender<Event>) -> Result<(), String> {
    let session = Proxy::new(bus, SERVICE, path, SESSION).map_err(|e| e.to_string())?;
    let signals = session.receive_signal("Lock").map_err(|e| e.to_string())?;
    for _ in signals {
        if tx.send(Event::Lock).is_err() {
            break;
        }
    }
    Ok(())
}

fn watch_sleep(bus: &Connection, tx: Sender<Event>) -> Result<(), String> {
    let manager = Proxy::new(bus, SERVICE, PATH, MANAGER).map_err(|e| e.to_string())?;
    let signals = manager
        .receive_signal("PrepareForSleep")
        .map_err(|e| e.to_string())?;
    let mut held = inhibit(&manager);
    let _keys = manager
        .call::<_, _, OwnedFd>(
            "Inhibit",
            &(
                "handle-power-key:handle-lid-switch",
                "Drifter",
                "Drifter follows Settings > Power for the lid and the power button",
                "block",
            ),
        )
        .inspect_err(|e| log!("drifter: logind keeps the lid and power button: {e}"))
        .ok();
    for message in signals {
        let going: bool = message.body().deserialize().unwrap_or(false);
        let event = match going {
            true => Event::Sleep,
            false => Event::Woke,
        };
        if tx.send(event).is_err() {
            break;
        }
        if going {
            std::thread::sleep(LOCK_TIME);
            held = None;
        } else if held.is_none() {
            held = inhibit(&manager);
        }
    }
    drop(held);
    Ok(())
}

fn inhibit(manager: &Proxy) -> Option<OwnedFd> {
    manager
        .call(
            "Inhibit",
            &("sleep", "Drifter", "Lock the screen before sleep", "delay"),
        )
        .inspect_err(|e| log!("drifter: cannot delay sleep to lock first: {e}"))
        .ok()
}
