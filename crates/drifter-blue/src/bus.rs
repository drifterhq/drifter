use std::collections::HashMap;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use drifter_core::blue::{Blue, Device, Kind, Missing, Seen};
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const BLUEZ: &str = "org.bluez";
const ADAPTER: &str = "org.bluez.Adapter1";
const DEVICE: &str = "org.bluez.Device1";
const BATTERY: &str = "org.bluez.Battery1";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const SETTLE: Duration = Duration::from_millis(40);
const BATTERY_EVERY: Duration = Duration::from_secs(30);
const AUDIO: [&str; 4] = ["0000110b", "0000111e", "00001108", "0000184e"];
const PUSH: &str = "00001105";

type Faces = HashMap<String, HashMap<String, OwnedValue>>;

#[derive(Clone)]
pub struct Bus {
    system: Connection,
}

fn text(props: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    props.get(key).and_then(|value| value.downcast_ref::<String>().ok())
}

fn flag(props: &HashMap<String, OwnedValue>, key: &str) -> bool {
    props.get(key).and_then(|value| value.downcast_ref::<bool>().ok()).unwrap_or(false)
}

fn uuids(props: &HashMap<String, OwnedValue>) -> Vec<String> {
    props
        .get("UUIDs")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| Vec::<String>::try_from(value).ok())
        .unwrap_or_default()
}

pub fn build(objects: &HashMap<OwnedObjectPath, Faces>, power: &std::path::Path) -> Option<Blue> {
    let mut adapters: Vec<(&OwnedObjectPath, &HashMap<String, OwnedValue>)> = objects
        .iter()
        .filter_map(|(path, faces)| faces.get(ADAPTER).map(|props| (path, props)))
        .collect();
    adapters.sort_by(|a, b| a.0.as_str().cmp(b.0.as_str()));
    let (adapter, props) = adapters.first()?;
    let under = format!("{}/", adapter.as_str());
    let mut devices: Vec<Device> = objects
        .iter()
        .filter(|(path, _)| path.as_str().starts_with(&under))
        .filter_map(|(path, faces)| {
            let props = faces.get(DEVICE)?;
            let address = text(props, "Address")?.to_uppercase();
            let name = text(props, "Alias").or_else(|| text(props, "Name")).unwrap_or_default();
            let real_name = text(props, "Name").is_some() || drifter_core::blue::named(&name, &address);
            let icon = text(props, "Icon").unwrap_or_default();
            let class = props.get("Class").and_then(|value| value.downcast_ref::<u32>().ok());
            let appearance = props.get("Appearance").and_then(|value| value.downcast_ref::<u16>().ok());
            let services = uuids(props);
            let connected = flag(props, "Connected");
            let battery = faces
                .get(BATTERY)
                .and_then(|battery| battery.get("Percentage"))
                .and_then(|value| value.downcast_ref::<u8>().ok())
                .or_else(|| match connected {
                    true => drifter_core::blue::power_supply_battery(&address, power),
                    false => None,
                });
            Some(Device {
                path: path.to_string(),
                kind: Kind::guess(&icon, class, appearance, &name),
                named: real_name && drifter_core::blue::named(&name, &address),
                name,
                paired: flag(props, "Paired") || flag(props, "Bonded"),
                connected,
                trusted: flag(props, "Trusted"),
                battery,
                rssi: props.get("RSSI").and_then(|value| value.downcast_ref::<i16>().ok()),
                audio: services.iter().any(|uuid| AUDIO.iter().any(|want| uuid.starts_with(want))),
                sends: services.iter().any(|uuid| uuid.starts_with(PUSH)),
                classic: class.is_some() || faces.contains_key("org.bluez.Bearer.BREDR1"),
                address,
            })
        })
        .collect();
    devices.retain(|device| !device.path.is_empty());
    let mut blue = Blue {
        adapter: adapter.to_string(),
        on: flag(props, "Powered"),
        searching: flag(props, "Discovering"),
        devices,
    };
    blue.sort();
    Some(blue)
}

impl Bus {
    pub fn system() -> Result<Bus, String> {
        Connection::system()
            .map(|system| Bus { system })
            .map_err(|error| drifter_core::lang::tf("Cannot reach Bluetooth: {}", &[&error]))
    }

    pub fn snapshot(&self) -> Option<Blue> {
        if !self.running() {
            return None;
        }
        let manager = zbus::blocking::fdo::ObjectManagerProxy::builder(&self.system)
            .destination(BLUEZ)
            .and_then(|builder| builder.path("/"))
            .and_then(|builder| builder.build())
            .ok()?;
        let objects = manager.get_managed_objects().ok()?;
        let objects: HashMap<OwnedObjectPath, Faces> = objects
            .into_iter()
            .map(|(path, faces)| (path, faces.into_iter().map(|(name, props)| (name.to_string(), props)).collect()))
            .collect();
        build(&objects, std::path::Path::new("/sys/class/power_supply"))
    }

    pub fn running(&self) -> bool {
        zbus::blocking::fdo::DBusProxy::new(&self.system)
            .and_then(|proxy| proxy.name_has_owner(BLUEZ.try_into().map_err(zbus::Error::from)?).map_err(zbus::Error::from))
            .unwrap_or(false)
    }

    pub fn seen(&self) -> Seen {
        if !radios() {
            return Err(Missing::Adapter);
        }
        if !self.running() {
            return Err(Missing::Service);
        }
        self.snapshot().ok_or(Missing::Adapter)
    }

    pub fn watch(&self, tell: impl Fn(Seen) + Send + 'static) -> Result<(), String> {
        let rules = [
            "type='signal',interface='org.freedesktop.DBus.ObjectManager',path='/'".to_string(),
            format!("type='signal',interface='{PROPERTIES}',member='PropertiesChanged',path_namespace='/org/bluez'"),
            format!("type='signal',sender='org.freedesktop.DBus',member='NameOwnerChanged',arg0='{BLUEZ}'"),
        ];
        let (poke, poked) = channel::<()>();
        for rule in rules {
            let rule = zbus::MatchRule::try_from(rule.as_str()).map_err(|error| error.to_string())?;
            let messages = zbus::blocking::MessageIterator::for_match_rule(rule, &self.system, Some(64))
                .map_err(|error| drifter_core::lang::tf("Cannot watch Bluetooth: {}", &[&error]))?;
            let poke = poke.clone();
            std::thread::Builder::new()
                .name("drifter-blue-signals".into())
                .spawn(move || {
                    for message in messages {
                        if message.is_err() || poke.send(()).is_err() {
                            break;
                        }
                    }
                })
                .map_err(|error| error.to_string())?;
        }
        let bus = self.clone();
        std::thread::Builder::new()
            .name("drifter-blue".into())
            .spawn(move || {
                let mut last = bus.seen();
                tell(last.clone());
                loop {
                    match poked.recv_timeout(BATTERY_EVERY) {
                        Ok(()) => {
                            std::thread::sleep(SETTLE);
                            while poked.try_recv().is_ok() {}
                        }
                        Err(RecvTimeoutError::Timeout) => {}
                        Err(RecvTimeoutError::Disconnected) => break,
                    }
                    let now = bus.seen();
                    if now != last {
                        tell(now.clone());
                        last = now;
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn device(&self, address: &str) -> Result<(String, String), String> {
        let blue = self.snapshot().ok_or_else(|| drifter_core::lang::t("No Bluetooth adapter found.").to_string())?;
        blue.find(address)
            .map(|device| (blue.adapter.clone(), device.path.clone()))
            .ok_or_else(|| drifter_core::lang::t("The device went away. Search again.").to_string())
    }

    fn adapter(&self) -> Result<String, String> {
        self.snapshot()
            .map(|blue| blue.adapter)
            .ok_or_else(|| drifter_core::lang::t("No Bluetooth adapter found.").to_string())
    }

    fn call(&self, path: &str, face: &str, method: &str) -> Result<(), String> {
        self.system
            .call_method(Some(BLUEZ), path, Some(face), method, &())
            .map(drop)
            .map_err(|error| error.to_string())
    }

    fn set(&self, path: &str, face: &str, key: &str, value: Value<'_>) -> Result<(), String> {
        self.system
            .call_method(Some(BLUEZ), path, Some(PROPERTIES), "Set", &(face, key, value))
            .map(drop)
            .map_err(|error| error.to_string())
    }

    pub fn power(&self, on: bool) -> Result<(), String> {
        let adapter = self.adapter()?;
        match self.set(&adapter, ADAPTER, "Powered", Value::from(on)) {
            Err(error) if on && (error.contains("Blocked") || error.contains("rfkill") || error.contains("NotReady")) => {
                unblock();
                self.set(&adapter, ADAPTER, "Powered", Value::from(true))
            }
            done => done,
        }
    }

    pub fn start_service() -> Result<(), String> {
        let out = std::process::Command::new("pkexec")
            .args(["systemctl", "enable", "--now", "bluetooth.service"])
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|error| error.to_string())?;
        match out.status.success() {
            true => Ok(()),
            false => Err(drifter_core::lang::t("The Bluetooth service did not start. Install BlueZ, then try again.").to_string()),
        }
    }

    pub fn search(&self, on: bool) -> Result<(), String> {
        let adapter = self.adapter()?;
        if on {
            let mut filter: HashMap<&str, Value> = HashMap::new();
            filter.insert("Transport", Value::from("auto"));
            filter.insert("DuplicateData", Value::from(false));
            let _ = self
                .system
                .call_method(Some(BLUEZ), adapter.as_str(), Some(ADAPTER), "SetDiscoveryFilter", &(filter,));
        }
        self.call(&adapter, ADAPTER, if on { "StartDiscovery" } else { "StopDiscovery" })
    }

    pub fn connect(&self, address: &str) -> Result<(), String> {
        let (_, path) = self.device(address)?;
        let _ = self.set(&path, DEVICE, "Trusted", Value::from(true));
        self.call(&path, DEVICE, "Connect")
    }

    pub fn disconnect(&self, address: &str) -> Result<(), String> {
        let (_, path) = self.device(address)?;
        self.call(&path, DEVICE, "Disconnect")
    }

    pub fn set_up(&self, address: &str) -> Result<(), String> {
        let (_, path) = self.device(address)?;
        let paired = self.snapshot().and_then(|blue| blue.find(address).map(|device| device.paired)).unwrap_or(false);
        if !paired {
            self.call(&path, DEVICE, "Pair")?;
        }
        self.set(&path, DEVICE, "Trusted", Value::from(true))
    }

    pub fn pair(&self, address: &str) -> Result<(), String> {
        self.set_up(address)?;
        let (_, path) = self.device(address)?;
        self.call(&path, DEVICE, "Connect")
    }

    pub fn cancel_pairing(&self, address: &str) -> Result<(), String> {
        let (_, path) = self.device(address)?;
        self.call(&path, DEVICE, "CancelPairing")
    }

    pub fn forget(&self, address: &str) -> Result<(), String> {
        let (adapter, path) = self.device(address)?;
        let path = zbus::zvariant::ObjectPath::try_from(path.as_str()).map_err(|error| error.to_string())?;
        self.system
            .call_method(Some(BLUEZ), adapter.as_str(), Some(ADAPTER), "RemoveDevice", &(path,))
            .map(drop)
            .map_err(|error| error.to_string())
    }

    pub fn trust(&self, address: &str, on: bool) -> Result<(), String> {
        let (_, path) = self.device(address)?;
        self.set(&path, DEVICE, "Trusted", Value::from(on))
    }

    pub fn rename(&self, address: &str, name: &str) -> Result<(), String> {
        let (_, path) = self.device(address)?;
        self.set(&path, DEVICE, "Alias", Value::from(name.trim()))
    }

    pub fn wait(&self, until: impl Fn(&Blue) -> bool, most: Duration) -> bool {
        let started = std::time::Instant::now();
        while started.elapsed() < most {
            if self.snapshot().is_some_and(|blue| until(&blue)) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        false
    }
}


fn radios() -> bool {
    std::fs::read_dir("/sys/class/bluetooth").is_ok_and(|mut entries| entries.next().is_some())
}

fn unblock() {
    let quiet = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    };
    if !quiet("rfkill", &["unblock", "bluetooth"]) {
        quiet("pkexec", &["rfkill", "unblock", "bluetooth"]);
    }
    std::thread::sleep(Duration::from_millis(400));
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn reads_the_live_adapter() {
        let bus = super::Bus::system().expect("the system bus");
        assert!(bus.running(), "bluetoothd is on the bus");
        let blue = bus.seen().expect("an adapter");
        for device in &blue.devices {
            println!("{} {:?} {} paired={} connected={} battery={:?} audio={}", device.shown_name(), device.kind, device.note(), device.paired, device.connected, device.battery, device.audio);
        }
        assert!(!blue.adapter.is_empty());
    }

    #[test]
    #[ignore]
    fn a_change_arrives_within_a_second() {
        let bus = super::Bus::system().expect("the system bus");
        let (tell, heard) = std::sync::mpsc::channel();
        bus.watch(move |seen| {
            let _ = tell.send(seen.map(|blue| blue.searching).unwrap_or(false));
        })
        .unwrap();
        let first = heard.recv_timeout(std::time::Duration::from_secs(5)).expect("the first reading");
        if first {
            println!("another app is already searching, so there is no change to see");
            return;
        }
        let started = std::time::Instant::now();
        bus.search(true).expect("a search starts");
        let mut searching = false;
        while let Ok(now) = heard.recv_timeout(std::time::Duration::from_secs(3)) {
            if now {
                searching = true;
                break;
            }
        }
        let took = started.elapsed();
        let _ = bus.search(false);
        assert!(searching, "the search showed up");
        assert!(took < std::time::Duration::from_secs(1), "it showed up in {took:?}, not on the slow fallback");
    }
}
