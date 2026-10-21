use std::collections::HashMap;
use std::time::Duration;

use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const OBEX: &str = "org.bluez.obex";

pub fn send(address: &str, file: &std::path::Path, progress: impl Fn(u64, u64)) -> Result<(), String> {
    let bus = zbus::blocking::Connection::session().map_err(|error| error.to_string())?;
    let mut args: HashMap<&str, Value> = HashMap::new();
    args.insert("Target", Value::from("opp"));
    let session: OwnedObjectPath = bus
        .call_method(Some(OBEX), "/org/bluez/obex", Some("org.bluez.obex.Client1"), "CreateSession", &(address, args))
        .and_then(|reply| reply.body().deserialize())
        .map_err(|error| error.to_string())?;
    let file_text = file.to_string_lossy().to_string();
    let sent = bus
        .call_method(Some(OBEX), session.as_str(), Some("org.bluez.obex.ObjectPush1"), "SendFile", &(file_text,))
        .and_then(|reply| reply.body().deserialize::<(OwnedObjectPath, HashMap<String, OwnedValue>)>())
        .map_err(|error| error.to_string());
    let outcome = sent.and_then(|(transfer, props)| {
        let size = props.get("Size").and_then(|value| value.downcast_ref::<u64>().ok()).unwrap_or(0);
        loop {
            let get = |key: &str| {
                bus.call_method(Some(OBEX), transfer.as_str(), Some("org.freedesktop.DBus.Properties"), "Get", &("org.bluez.obex.Transfer1", key))
                    .ok()
                    .and_then(|reply| reply.body().deserialize::<OwnedValue>().ok())
            };
            let status = get("Status").and_then(|value| value.downcast_ref::<String>().ok());
            if let Some(done) = get("Transferred").and_then(|value| value.downcast_ref::<u64>().ok()) {
                progress(done, size);
            }
            match status.as_deref() {
                Some("complete") | None => {
                    progress(size, size);
                    return Ok(());
                }
                Some("error") => return Err(drifter_core::lang::t("The phone said no, or it stopped answering.").to_string()),
                _ => std::thread::sleep(Duration::from_millis(250)),
            }
        }
    });
    let _ = bus.call_method(Some(OBEX), "/org/bluez/obex", Some("org.bluez.obex.Client1"), "RemoveSession", &(session,));
    outcome
}

pub fn tell(summary: &str, body: &str) {
    let Ok(bus) = zbus::blocking::Connection::session() else {
        return;
    };
    let hints: HashMap<&str, Value> = HashMap::new();
    let _ = bus.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &(drifter_core::lang::t("Bluetooth"), 0u32, "bluetooth", summary, body, Vec::<&str>::new(), hints, 6000i32),
    );
}

pub fn phones(blue: &drifter_core::blue::Blue) -> Vec<(String, String)> {
    blue.mine()
        .filter(|device| {
            device.sends || matches!(device.kind, drifter_core::blue::Kind::Phone | drifter_core::blue::Kind::Tablet | drifter_core::blue::Kind::Computer)
        })
        .map(|device| (device.address.clone(), device.shown_name()))
        .collect()
}
