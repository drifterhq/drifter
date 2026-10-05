use std::collections::HashMap;

use drifter_core::disks::{Plugged, label};
use smithay::reexports::calloop::channel::{Channel, Sender, channel};
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, Value};

const SERVICE: &str = "org.freedesktop.UDisks2";
const ROOT: &str = "/org/freedesktop/UDisks2";
const BLOCK: &str = "org.freedesktop.UDisks2.Block";
const FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";

pub enum Event {
    Mounted { name: String, folder: String },
    Failed { name: String, error: String },
}

pub fn start() -> Result<Channel<Event>, String> {
    let (tx, rx) = channel::<Event>();
    let bus = Connection::system().map_err(|e| format!("no system bus: {e}"))?;
    std::thread::Builder::new()
        .name("drifter-disks".to_string())
        .spawn(move || {
            if let Err(error) = watch(&bus, &tx) {
                eprintln!("drifter: plugged in disks are not opened: {error}");
            }
        })
        .map_err(|e| format!("cannot start the disk thread: {e}"))?;
    Ok(rx)
}

fn watch(bus: &Connection, tx: &Sender<Event>) -> Result<(), String> {
    let manager = zbus::blocking::fdo::ObjectManagerProxy::builder(bus)
        .destination(SERVICE)
        .and_then(|builder| builder.path(ROOT))
        .and_then(|builder| builder.cache_properties(zbus::proxy::CacheProperties::No).build())
        .map_err(|e| format!("no udisks: {e}"))?;
    let added = manager
        .receive_interfaces_added()
        .map_err(|e| format!("udisks does not report disks: {e}"))?;
    for signal in added {
        let Ok(args) = signal.args() else {
            continue;
        };
        let fresh = args
            .interfaces_and_properties()
            .keys()
            .any(|name| name.as_str() == FILESYSTEM);
        if !fresh {
            continue;
        }
        let path = OwnedObjectPath::from(args.object_path().clone());
        if let Some(event) = mount(bus, &path)
            && tx.send(event).is_err()
        {
            break;
        }
    }
    Ok(())
}

fn mount(bus: &Connection, path: &OwnedObjectPath) -> Option<Event> {
    let plain = |interface: &'static str| {
        zbus::blocking::proxy::Builder::<Proxy>::new(bus)
            .destination(SERVICE)
            .and_then(|builder| builder.path(path.as_str()))
            .and_then(|builder| builder.interface(interface))
            .and_then(|builder| builder.cache_properties(zbus::proxy::CacheProperties::No).build())
    };
    let (block, filesystem) = match (plain(BLOCK), plain(FILESYSTEM)) {
        (Ok(block), Ok(filesystem)) => (block, filesystem),
        (Err(error), _) | (_, Err(error)) => {
            eprintln!("drifter: cannot read the disk {}: {error}", path.as_str());
            return None;
        }
    };
    let flag = |name: &str| block.get_property::<bool>(name).unwrap_or(false);
    let mounted = filesystem
        .get_property::<Vec<Vec<u8>>>("MountPoints")
        .map(|points| !points.is_empty())
        .unwrap_or(false);
    let plugged = Plugged {
        usage: block.get_property::<String>("IdUsage").unwrap_or_default(),
        auto: flag("HintAuto"),
        ignore: flag("HintIgnore"),
        system: flag("HintSystem"),
        mounted,
    };
    if !plugged.wants_mount() {
        return None;
    }
    let device = block
        .get_property::<Vec<u8>>("PreferredDevice")
        .map(|bytes| String::from_utf8_lossy(&bytes).trim_end_matches('\0').to_string())
        .unwrap_or_default();
    let name = label(
        &block.get_property::<String>("IdLabel").unwrap_or_default(),
        block.get_property::<u64>("Size").unwrap_or(0),
        &device,
    );
    let options: HashMap<&str, Value> = HashMap::from([("auth.no_user_interaction", Value::from(true))]);
    Some(match filesystem.call::<_, _, String>("Mount", &(options,)) {
        Ok(folder) => {
            println!("disk        {name} opened at {folder}");
            Event::Mounted { name, folder }
        }
        Err(error) => Event::Failed {
            name,
            error: error.to_string(),
        },
    })
}
