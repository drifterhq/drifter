use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use drifter_core::net::{Ap, Card, Guard, Kind, Link, Missing, Net, Online, Primary, Private, Saved, Seen, Vpn, WireGuard, Wifi, Wired};
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const NM: &str = "org.freedesktop.NetworkManager";
const ROOT: &str = "/org/freedesktop/NetworkManager";
const SETTINGS: &str = "/org/freedesktop/NetworkManager/Settings";
const MANAGER: &str = "org.freedesktop.NetworkManager";
const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
const WIRED: &str = "org.freedesktop.NetworkManager.Device.Wired";
const POINT: &str = "org.freedesktop.NetworkManager.AccessPoint";
const ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
const IP4: &str = "org.freedesktop.NetworkManager.IP4Config";
const IP6: &str = "org.freedesktop.NetworkManager.IP6Config";
const SETTINGS_FACE: &str = "org.freedesktop.NetworkManager.Settings";
const CONNECTION: &str = "org.freedesktop.NetworkManager.Settings.Connection";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const SETTLE: Duration = Duration::from_millis(60);
const CALM: Duration = Duration::from_millis(150);
const EVERY: Duration = Duration::from_secs(30);

pub type Props = HashMap<String, OwnedValue>;
pub type Faces = HashMap<String, Props>;
pub type Setup = HashMap<String, HashMap<String, OwnedValue>>;

#[derive(Clone)]
pub struct Bus {
    system: Connection,
    saved: Arc<Mutex<Option<Vec<(Saved, Setup)>>>>,
    stale: Arc<AtomicBool>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Joined {
    Done,
    Failed(u32),
    Canceled,
    Waited,
}

pub fn text(props: &Props, key: &str) -> String {
    props.get(key).and_then(|value| value.downcast_ref::<String>().ok()).unwrap_or_default()
}

fn flag(props: &Props, key: &str) -> bool {
    props.get(key).and_then(|value| value.downcast_ref::<bool>().ok()).unwrap_or(false)
}

fn number(props: &Props, key: &str) -> u32 {
    props.get(key).and_then(|value| value.downcast_ref::<u32>().ok()).unwrap_or(0)
}

fn path(props: &Props, key: &str) -> String {
    props
        .get(key)
        .and_then(|value| value.downcast_ref::<zbus::zvariant::ObjectPath>().ok())
        .map(|path| path.to_string())
        .filter(|path| path != "/")
        .unwrap_or_default()
}

fn paths(props: &Props, key: &str) -> Vec<String> {
    props
        .get(key)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| Vec::<OwnedObjectPath>::try_from(value).ok())
        .map(|list| list.into_iter().map(|path| path.to_string()).collect())
        .unwrap_or_default()
}

fn bytes(props: &Props, key: &str) -> Vec<u8> {
    props
        .get(key)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| Vec::<u8>::try_from(value).ok())
        .unwrap_or_default()
}

fn reason(props: &Props) -> u32 {
    props
        .get("StateReason")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| <(u32, u32)>::try_from(value).ok())
        .map_or(0, |(_, why)| why)
}

fn dicts(props: &Props, key: &str) -> Vec<Props> {
    props
        .get(key)
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| Vec::<HashMap<String, OwnedValue>>::try_from(value).ok())
        .unwrap_or_default()
}

fn addresses(props: &Props) -> Vec<String> {
    dicts(props, "AddressData")
        .iter()
        .map(|data| {
            let prefix = data.get("prefix").and_then(|value| value.downcast_ref::<u32>().ok()).unwrap_or(0);
            format!("{}/{prefix}", text(data, "address"))
        })
        .filter(|each| !each.starts_with('/'))
        .collect()
}

fn servers(props: &Props) -> Vec<String> {
    let data: Vec<String> = dicts(props, "NameserverData").iter().map(|data| text(data, "address")).filter(|each| !each.is_empty()).collect();
    if !data.is_empty() {
        return data;
    }
    props
        .get("Nameservers")
        .and_then(|value| value.try_clone().ok())
        .and_then(|value| Vec::<String>::try_from(value).ok())
        .unwrap_or_default()
}

fn setting<'a>(setup: &'a Setup, part: &str) -> Option<&'a HashMap<String, OwnedValue>> {
    setup.get(part)
}

pub fn read_saved(path: &str, setup: &Setup) -> Saved {
    let empty = HashMap::new();
    let connection = setting(setup, "connection").unwrap_or(&empty);
    let wireless = setting(setup, "802-11-wireless").unwrap_or(&empty);
    let kind = Kind::read(&text(connection, "type"));
    let autoconnect = connection.get("autoconnect").and_then(|value| value.downcast_ref::<bool>().ok()).unwrap_or(true);
    let priority = connection.get("autoconnect-priority").and_then(|value| value.downcast_ref::<i32>().ok()).unwrap_or(0);
    let metered = connection.get("metered").and_then(|value| value.downcast_ref::<i32>().ok()).unwrap_or(0) == 1;
    let mac = text(wireless, "cloned-mac-address");
    let mac = match mac.is_empty() {
        true => text(wireless, "assigned-mac-address"),
        false => mac,
    };
    let key_mgmt = setting(setup, "802-11-wireless-security").map(|security| text(security, "key-mgmt")).unwrap_or_default();
    Saved {
        path: path.to_string(),
        uuid: text(connection, "uuid"),
        id: text(connection, "id"),
        kind,
        ssid: drifter_core::net::ssid_text(&bytes(wireless, "ssid")),
        autoconnect,
        priority,
        metered,
        private: Private::read(&mac),
        enterprise: setup.contains_key("802-1x") || key_mgmt == "wpa-eap" || key_mgmt == "ieee8021x",
        hidden: flag(wireless, "hidden"),
        powersave: number(wireless, "powersave"),
    }
}

pub fn build(objects: &HashMap<String, Faces>, saved: &[Saved]) -> Wifi {
    let empty = HashMap::new();
    let root = objects.get(ROOT).and_then(|faces| faces.get(MANAGER)).unwrap_or(&empty);
    let active: HashMap<&str, &Props> = objects
        .iter()
        .filter_map(|(path, faces)| faces.get(ACTIVE).map(|props| (path.as_str(), props)))
        .collect();
    let devices: Vec<(&String, &Faces)> = paths(root, "Devices")
        .iter()
        .filter_map(|path| objects.get_key_value(path))
        .collect();
    let mut devices = match devices.is_empty() {
        true => objects.iter().filter(|(_, faces)| faces.contains_key(DEVICE)).collect(),
        false => devices,
    };
    devices.sort_by(|a, b| a.0.cmp(b.0));
    let card = devices
        .iter()
        .find(|(_, faces)| faces.contains_key(WIRELESS) && faces.get(DEVICE).is_some_and(|device| flag(device, "Managed")))
        .or_else(|| devices.iter().find(|(_, faces)| faces.contains_key(WIRELESS)))
        .copied();
    let wired = devices
        .iter()
        .find(|(_, faces)| faces.contains_key(WIRED) && faces.get(DEVICE).is_some_and(|device| flag(device, "Managed") && number(device, "DeviceType") == 1))
        .copied();
    let ip_of = |config: &str, face: &str| -> Vec<String> {
        objects.get(config).and_then(|faces| faces.get(face)).map(addresses).unwrap_or_default()
    };
    let ssid_of_active = |props: &Props| -> String {
        let connection = path(props, "Connection");
        saved
            .iter()
            .find(|each| each.path == connection)
            .map(|each| each.ssid.clone())
            .filter(|ssid| !ssid.is_empty())
            .unwrap_or_else(|| {
                let point = path(props, "SpecificObject");
                objects
                    .get(&point)
                    .and_then(|faces| faces.get(POINT))
                    .map(|point| drifter_core::net::ssid_text(&bytes(point, "Ssid")))
                    .unwrap_or_else(|| text(props, "Id"))
            })
    };
    let mut wifi = Wifi {
        running: true,
        on: flag(root, "WirelessEnabled"),
        hardware: root.get("WirelessHardwareEnabled").and_then(|value| value.downcast_ref::<bool>().ok()).unwrap_or(true),
        online: Online::read(number(root, "Connectivity")),
        primary: match text(root, "PrimaryConnectionType").as_str() {
            "" => Primary::None,
            "802-11-wireless" => Primary::Wifi,
            "802-3-ethernet" => Primary::Wired,
            "vpn" | "wireguard" => Primary::Vpn,
            _ => Primary::Other,
        },
        saved: saved.to_vec(),
        ..Wifi::default()
    };
    if let Some((card_path, faces)) = card {
        let device = faces.get(DEVICE).unwrap_or(&empty);
        let wireless = faces.get(WIRELESS).unwrap_or(&empty);
        wifi.managed = flag(device, "Managed");
        let hw = text(device, "HwAddress");
        wifi.card = Some(Card {
            path: card_path.clone(),
            iface: text(device, "Interface"),
            state: number(device, "State"),
            reason: reason(device),
            hw: match hw.is_empty() {
                true => text(wireless, "HwAddress"),
                false => hw,
            },
            scanned: wireless.get("LastScan").and_then(|value| value.downcast_ref::<i64>().ok()).unwrap_or(0),
            ip4: ip_of(&path(device, "Ip4Config"), IP4).into_iter().next().unwrap_or_default(),
            ip6: ip_of(&path(device, "Ip6Config"), IP6).into_iter().next().unwrap_or_default(),
        });
        let on_card = active
            .values()
            .filter(|props| paths(props, "Devices").iter().any(|each| each == card_path))
            .copied()
            .collect::<Vec<_>>();
        for props in on_card {
            match number(props, "State") {
                2 => wifi.joined = Some(ssid_of_active(props)),
                1 => wifi.joining = Some(ssid_of_active(props)),
                _ => {}
            }
        }
        if wifi.on {
            let mut nets: Vec<Net> = Vec::new();
            for point_path in paths(wireless, "AccessPoints") {
                let Some(point) = objects.get(&point_path).and_then(|faces| faces.get(POINT)) else {
                    continue;
                };
                let ssid = drifter_core::net::ssid_text(&bytes(point, "Ssid"));
                if ssid.is_empty() {
                    continue;
                }
                let guard = Guard {
                    flags: number(point, "Flags"),
                    wpa: number(point, "WpaFlags"),
                    rsn: number(point, "RsnFlags"),
                };
                let signal = point.get("Strength").and_then(|value| value.downcast_ref::<u8>().ok()).unwrap_or(0).min(100);
                let net = Net {
                    known: saved.iter().any(|each| each.kind == Kind::Wifi && each.ssid == ssid),
                    active: wifi.joined.as_deref() == Some(ssid.as_str()),
                    busy: wifi.joining.as_deref() == Some(ssid.as_str()),
                    secure: guard.secure(),
                    enterprise: guard.enterprise(),
                    sae: guard.sae(),
                    wep: guard.wep(),
                    mhz: number(point, "Frequency"),
                    point: point_path.clone(),
                    signal,
                    ssid,
                };
                match nets.iter_mut().find(|kept| kept.ssid == net.ssid) {
                    Some(kept) => {
                        if net.signal > kept.signal {
                            kept.signal = net.signal;
                            kept.mhz = net.mhz;
                            kept.point = net.point;
                        }
                        kept.secure |= net.secure;
                        kept.enterprise |= net.enterprise;
                        kept.sae &= net.sae;
                    }
                    None => nets.push(net),
                }
            }
            wifi.nets = nets;
        }
        if let Some(joined) = wifi.joined.clone()
            && wifi.on
            && wifi.find(&joined).is_none()
        {
            wifi.nets.push(Net {
                ssid: joined,
                active: true,
                known: true,
                secure: true,
                signal: 0,
                ..Net::default()
            });
        }
        if let Some(joining) = wifi.joining.clone()
            && wifi.on
            && wifi.find(&joining).is_none()
        {
            wifi.nets.push(Net {
                ssid: joining,
                busy: true,
                known: true,
                secure: true,
                ..Net::default()
            });
        }
        wifi.sort();
    }
    if let Some((wired_path, faces)) = wired {
        let device = faces.get(DEVICE).unwrap_or(&empty);
        wifi.wired = Some(Wired {
            path: wired_path.clone(),
            iface: text(device, "Interface"),
            state: number(device, "State"),
            carrier: faces.get(WIRED).is_some_and(|wired| flag(wired, "Carrier")),
        });
    }
    for each in saved.iter().filter(|each| matches!(each.kind, Kind::WireGuard | Kind::Vpn)) {
        let state = active
            .values()
            .find(|props| text(props, "Uuid") == each.uuid)
            .map(|props| number(props, "State"));
        wifi.vpns.push(Vpn {
            uuid: each.uuid.clone(),
            name: each.id.clone(),
            wireguard: each.kind == Kind::WireGuard,
            active: state == Some(2),
            busy: state == Some(1) || state == Some(3),
        });
    }
    wifi.vpns.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    wifi
}

fn ssid_value(ssid: &str) -> Value<'static> {
    Value::from(ssid.as_bytes().to_vec())
}

type Part = HashMap<&'static str, Value<'static>>;

fn security(guard_sae: bool, wep: bool, password: &str) -> Part {
    let mut part: Part = HashMap::new();
    match (wep, guard_sae) {
        (true, _) => {
            part.insert("key-mgmt", Value::from("none"));
            part.insert("wep-key0", Value::from(password.to_string()));
            let hexish = matches!(password.len(), 10 | 26) && password.chars().all(|c| c.is_ascii_hexdigit());
            let raw = matches!(password.len(), 5 | 13);
            part.insert("wep-key-type", Value::from(if hexish || raw { 1u32 } else { 2u32 }));
        }
        (false, true) => {
            part.insert("key-mgmt", Value::from("sae"));
            part.insert("psk", Value::from(password.to_string()));
        }
        (false, false) => {
            part.insert("key-mgmt", Value::from("wpa-psk"));
            part.insert("psk", Value::from(password.to_string()));
        }
    }
    part
}

impl Bus {
    pub fn system() -> Result<Bus, String> {
        Connection::system()
            .map(|system| Bus {
                system,
                saved: Arc::new(Mutex::new(None)),
                stale: Arc::new(AtomicBool::new(true)),
            })
            .map_err(|error| drifter_core::lang::tf("Cannot reach NetworkManager: {}", &[&error]))
    }

    pub fn running(&self) -> bool {
        zbus::blocking::fdo::DBusProxy::new(&self.system)
            .and_then(|proxy| proxy.name_has_owner(NM.try_into().map_err(zbus::Error::from)?).map_err(zbus::Error::from))
            .unwrap_or(false)
    }

    fn objects(&self) -> Option<HashMap<String, Faces>> {
        let manager = zbus::blocking::fdo::ObjectManagerProxy::builder(&self.system)
            .destination(NM)
            .and_then(|builder| builder.path("/org/freedesktop"))
            .and_then(|builder| builder.build())
            .ok()?;
        let objects = manager.get_managed_objects().ok()?;
        let mut out: HashMap<String, Faces> = objects
            .into_iter()
            .map(|(path, faces)| (path.to_string(), faces.into_iter().map(|(name, props)| (name.to_string(), props)).collect()))
            .collect();
        if !out.get(ROOT).is_some_and(|faces| faces.contains_key(MANAGER))
            && let Ok(props) = self.get_all(ROOT, MANAGER)
        {
            out.entry(ROOT.to_string()).or_default().insert(MANAGER.to_string(), props);
        }
        Some(out)
    }

    fn get_all(&self, at: &str, face: &str) -> Result<Props, String> {
        self.system
            .call_method(Some(NM), at, Some(PROPERTIES), "GetAll", &(face,))
            .map_err(|error| error.to_string())?
            .body()
            .deserialize::<Props>()
            .map_err(|error| error.to_string())
    }

    pub fn setup(&self, at: &str) -> Result<Setup, String> {
        self.system
            .call_method(Some(NM), at, Some(CONNECTION), "GetSettings", &())
            .map_err(|error| error.to_string())?
            .body()
            .deserialize::<Setup>()
            .map_err(|error| error.to_string())
    }

    fn secrets(&self, at: &str, part: &str) -> Option<Setup> {
        self.system
            .call_method(Some(NM), at, Some(CONNECTION), "GetSecrets", &(part,))
            .ok()?
            .body()
            .deserialize::<Setup>()
            .ok()
    }

    fn saved_all(&self, objects: &HashMap<String, Faces>) -> Vec<(Saved, Setup)> {
        let listed = objects
            .get(SETTINGS)
            .and_then(|faces| faces.get(SETTINGS_FACE))
            .map(|props| paths(props, "Connections"))
            .filter(|list| !list.is_empty())
            .unwrap_or_else(|| {
                let mut found: Vec<String> = objects.iter().filter(|(_, faces)| faces.contains_key(CONNECTION)).map(|(path, _)| path.clone()).collect();
                found.sort();
                found
            });
        let mut cache = self.saved.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let fresh = !self.stale.swap(false, Ordering::SeqCst)
            && cache.as_ref().is_some_and(|kept| kept.len() == listed.len() && kept.iter().zip(&listed).all(|((saved, _), path)| saved.path == *path));
        if !fresh {
            let read: Vec<(Saved, Setup)> = listed
                .iter()
                .filter_map(|path| self.setup(path).ok().map(|setup| (read_saved(path, &setup), setup)))
                .collect();
            *cache = Some(read);
        }
        cache.clone().unwrap_or_default()
    }

    pub fn snapshot(&self) -> Option<Wifi> {
        if !self.running() {
            return None;
        }
        let objects = self.objects()?;
        let saved: Vec<Saved> = self.saved_all(&objects).into_iter().map(|(saved, _)| saved).collect();
        Some(build(&objects, &saved))
    }

    pub fn seen(&self) -> Seen {
        match self.snapshot() {
            Some(wifi) => Ok(wifi),
            None => Err(Missing::Service),
        }
    }

    pub fn watch(&self, tell: impl Fn(Seen) + Send + 'static) -> Result<(), String> {
        let rules = [
            format!("type='signal',sender='{NM}'"),
            format!("type='signal',sender='org.freedesktop.DBus',member='NameOwnerChanged',arg0='{NM}'"),
        ];
        let (poke, poked) = channel::<()>();
        for rule in rules {
            let rule = zbus::MatchRule::try_from(rule.as_str()).map_err(|error| error.to_string())?;
            let messages = zbus::blocking::MessageIterator::for_match_rule(rule, &self.system, Some(256))
                .map_err(|error| drifter_core::lang::tf("Cannot watch NetworkManager: {}", &[&error]))?;
            let poke = poke.clone();
            let stale = self.stale.clone();
            std::thread::Builder::new()
                .name("drifter-net-signals".into())
                .spawn(move || {
                    for message in messages {
                        let Ok(message) = message else {
                            break;
                        };
                        let header = message.header();
                        let face = header.interface().map(|face| face.to_string()).unwrap_or_default();
                        let member = header.member().map(|member| member.to_string()).unwrap_or_default();
                        let touches_saved = face.starts_with(SETTINGS_FACE)
                            || member == "NameOwnerChanged"
                            || (face == PROPERTIES
                                && message
                                    .body()
                                    .deserialize::<(String, Props, Vec<String>)>()
                                    .is_ok_and(|(changed, _, _)| changed.starts_with(SETTINGS_FACE)));
                        if touches_saved {
                            stale.store(true, Ordering::SeqCst);
                        }
                        if poke.send(()).is_err() {
                            break;
                        }
                    }
                })
                .map_err(|error| error.to_string())?;
        }
        let bus = self.clone();
        std::thread::Builder::new()
            .name("drifter-net".into())
            .spawn(move || {
                let mut last = bus.seen();
                tell(last.clone());
                loop {
                    match poked.recv_timeout(EVERY) {
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
                    std::thread::sleep(CALM);
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn call(&self, at: &str, face: &str, method: &str, body: &(impl serde::Serialize + zbus::zvariant::DynamicType)) -> Result<zbus::Message, String> {
        self.system.call_method(Some(NM), at, Some(face), method, body).map_err(|error| plain_bus(&error.to_string()))
    }

    fn card(&self) -> Result<Card, String> {
        self.snapshot()
            .and_then(|wifi| wifi.card)
            .ok_or_else(|| drifter_core::lang::t("No Wi-Fi card found.").to_string())
    }

    pub fn power(&self, on: bool) -> Result<(), String> {
        self.system
            .call_method(Some(NM), ROOT, Some(PROPERTIES), "Set", &(MANAGER, "WirelessEnabled", Value::from(on)))
            .map(drop)
            .map_err(|error| plain_bus(&error.to_string()))
    }

    pub fn scan(&self) -> Result<(), String> {
        let card = self.card()?;
        let options: HashMap<&str, Value> = HashMap::new();
        match self.call(&card.path, WIRELESS, "RequestScan", &(options,)) {
            Err(error) if error.contains("not allowed") || error.contains("Scanning not allowed") || error.contains("already") => Ok(()),
            done => done.map(drop),
        }
    }

    pub fn start_service() -> Result<(), String> {
        let out = std::process::Command::new("pkexec")
            .args(["systemctl", "enable", "--now", "NetworkManager.service"])
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|error| error.to_string())?;
        match out.status.success() {
            true => Ok(()),
            false => Err(drifter_core::lang::t("NetworkManager did not start. Install it, then try again.").to_string()),
        }
    }

    fn object(text: &str) -> Result<zbus::zvariant::ObjectPath<'_>, String> {
        zbus::zvariant::ObjectPath::try_from(if text.is_empty() { "/" } else { text }).map_err(|error| error.to_string())
    }

    fn activate(&self, connection: &str, device: &str, point: &str) -> Result<String, String> {
        let reply = self.call(ROOT, MANAGER, "ActivateConnection", &(Self::object(connection)?, Self::object(device)?, Self::object(point)?))?;
        reply.body().deserialize::<OwnedObjectPath>().map(|path| path.to_string()).map_err(|error| error.to_string())
    }

    fn add_and_activate(&self, setup: HashMap<&str, Part>, device: &str, point: &str) -> Result<(String, String), String> {
        let reply = self.call(ROOT, MANAGER, "AddAndActivateConnection", &(setup, Self::object(device)?, Self::object(point)?))?;
        reply
            .body()
            .deserialize::<(OwnedObjectPath, OwnedObjectPath)>()
            .map(|(made, active)| (made.to_string(), active.to_string()))
            .map_err(|error| error.to_string())
    }

    pub fn join(&self, ssid: &str) -> Result<String, String> {
        let wifi = self.snapshot().ok_or_else(|| Missing::Service.words().to_string())?;
        let card = wifi.card.clone().ok_or_else(|| Missing::Card.words().to_string())?;
        let point = wifi.find(ssid).map(|net| net.point.clone()).unwrap_or_default();
        if let Some(saved) = wifi.saved_for(ssid).first() {
            return self.activate(&saved.path, &card.path, &point);
        }
        let mut wireless: Part = HashMap::new();
        wireless.insert("ssid", ssid_value(ssid));
        let mut setup: HashMap<&str, Part> = HashMap::new();
        setup.insert("802-11-wireless", wireless);
        if let Some(net) = wifi.find(ssid)
            && net.secure
            && !net.enterprise
        {
            let mut part: Part = HashMap::new();
            part.insert(
                "key-mgmt",
                Value::from(match (net.wep, net.sae) {
                    (true, _) => "none",
                    (false, true) => "sae",
                    (false, false) => "wpa-psk",
                }),
            );
            setup.insert("802-11-wireless-security", part);
        }
        self.add_and_activate(setup, &card.path, &point).map(|(_, active)| active)
    }

    pub fn join_with(&self, ssid: &str, password: Option<&str>, hidden: bool) -> Result<String, String> {
        let wifi = self.snapshot().ok_or_else(|| Missing::Service.words().to_string())?;
        let card = wifi.card.clone().ok_or_else(|| Missing::Card.words().to_string())?;
        let found = wifi.find(ssid).cloned();
        let point = found.as_ref().map(|net| net.point.clone()).filter(|_| !hidden).unwrap_or_default();
        if password.is_some() {
            for saved in wifi.saved_for(ssid) {
                let _ = self.call(&saved.path, CONNECTION, "Delete", &());
            }
        } else if let Some(saved) = wifi.saved_for(ssid).first() {
            return self.activate(&saved.path, &card.path, &point);
        }
        let mut wireless: Part = HashMap::new();
        wireless.insert("ssid", ssid_value(ssid));
        if hidden {
            wireless.insert("hidden", Value::from(true));
        }
        let mut connection: Part = HashMap::new();
        connection.insert("id", Value::from(ssid.to_string()));
        connection.insert("type", Value::from("802-11-wireless"));
        let mut setup: HashMap<&str, Part> = HashMap::new();
        setup.insert("connection", connection);
        setup.insert("802-11-wireless", wireless);
        if let Some(password) = password {
            let sae = found.as_ref().is_some_and(|net| net.sae);
            let wep = found.as_ref().is_some_and(|net| net.wep);
            setup.insert("802-11-wireless-security", security(sae, wep, password));
        }
        self.add_and_activate(setup, &card.path, &point).map(|(_, active)| active)
    }

    pub fn join_enterprise(&self, ssid: &str, user: &str, password: &str, domain: Option<&str>) -> Result<String, String> {
        let wifi = self.snapshot().ok_or_else(|| Missing::Service.words().to_string())?;
        let card = wifi.card.clone().ok_or_else(|| Missing::Card.words().to_string())?;
        for saved in wifi.saved_for(ssid) {
            let _ = self.call(&saved.path, CONNECTION, "Delete", &());
        }
        let point = wifi.find(ssid).map(|net| net.point.clone()).unwrap_or_default();
        let mut connection: Part = HashMap::new();
        connection.insert("id", Value::from(ssid.to_string()));
        connection.insert("type", Value::from("802-11-wireless"));
        let mut wireless: Part = HashMap::new();
        wireless.insert("ssid", ssid_value(ssid));
        let mut guard: Part = HashMap::new();
        guard.insert("key-mgmt", Value::from("wpa-eap"));
        let mut eap: Part = HashMap::new();
        eap.insert("eap", Value::from(vec!["peap".to_string()]));
        eap.insert("phase2-auth", Value::from("mschapv2"));
        eap.insert("identity", Value::from(user.to_string()));
        eap.insert("password", Value::from(password.to_string()));
        eap.insert("password-flags", Value::from(0u32));
        if let Some(domain) = domain.filter(|domain| !domain.trim().is_empty()) {
            eap.insert("system-ca-certs", Value::from(true));
            eap.insert("domain-suffix-match", Value::from(domain.trim().to_string()));
        }
        let mut setup: HashMap<&str, Part> = HashMap::new();
        setup.insert("connection", connection);
        setup.insert("802-11-wireless", wireless);
        setup.insert("802-11-wireless-security", guard);
        setup.insert("802-1x", eap);
        let (made, active) = self.add_and_activate(setup, &card.path, &point)?;
        match self.settle(&active, Duration::from_secs(60)) {
            Joined::Done => Ok(active),
            outcome => {
                let _ = self.call(&made, CONNECTION, "Delete", &());
                Err(self.outcome_words(outcome, ssid))
            }
        }
    }

    pub fn outcome_words(&self, outcome: Joined, ssid: &str) -> String {
        match outcome {
            Joined::Done => String::new(),
            Joined::Failed(reason) => drifter_core::net::plain_failure(reason, ssid),
            Joined::Canceled => drifter_core::lang::t("Joining was stopped.").to_string(),
            Joined::Waited => drifter_core::lang::tf("{} took too long to answer. Try again.", &[&ssid]),
        }
    }

    pub fn settle(&self, active: &str, most: Duration) -> Joined {
        let started = Instant::now();
        let mut last_reason = 0;
        while started.elapsed() < most {
            if let Some(card) = self.snapshot().and_then(|wifi| wifi.card) {
                if card.reason != 0 {
                    last_reason = card.reason;
                }
            }
            match self.get_all(active, ACTIVE) {
                Ok(props) => match number(&props, "State") {
                    2 => return Joined::Done,
                    4 => {
                        return match last_reason {
                            0 => Joined::Failed(number(&props, "StateReason")),
                            reason => Joined::Failed(reason),
                        };
                    }
                    _ => {}
                },
                Err(_) => {
                    return match last_reason {
                        0 | 39 => Joined::Canceled,
                        reason => Joined::Failed(reason),
                    };
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        Joined::Waited
    }

    pub fn leave(&self, ssid: &str) -> Result<(), String> {
        let objects = self.objects().ok_or_else(|| Missing::Service.words().to_string())?;
        let saved = self.saved_all(&objects);
        let card = build(&objects, &saved.iter().map(|(each, _)| each.clone()).collect::<Vec<_>>()).card.ok_or_else(|| Missing::Card.words().to_string())?;
        let mine: Vec<String> = saved.iter().filter(|(each, _)| each.ssid == ssid).map(|(each, _)| each.path.clone()).collect();
        let target = objects
            .iter()
            .filter_map(|(at, faces)| faces.get(ACTIVE).map(|props| (at, props)))
            .find(|(_, props)| paths(props, "Devices").contains(&card.path) || mine.contains(&path(props, "Connection")))
            .map(|(at, _)| at.clone())
            .ok_or_else(|| drifter_core::lang::tf("{} is not joined.", &[&ssid]))?;
        self.call(ROOT, MANAGER, "DeactivateConnection", &(Self::object(&target)?,)).map(drop)
    }

    pub fn forget(&self, ssid: &str) -> Result<(), String> {
        let wifi = self.snapshot().ok_or_else(|| Missing::Service.words().to_string())?;
        let saved = wifi.saved_for(ssid);
        if saved.is_empty() {
            return Err(drifter_core::lang::tf("{} is not saved.", &[&ssid]));
        }
        for each in saved {
            self.call(&each.path, CONNECTION, "Delete", &())?;
        }
        Ok(())
    }

    pub fn change(&self, uuid: &str, apply: impl FnOnce(&mut Setup)) -> Result<(), String> {
        let objects = self.objects().ok_or_else(|| Missing::Service.words().to_string())?;
        let (saved, mut setup) = self
            .saved_all(&objects)
            .into_iter()
            .find(|(each, _)| each.uuid == uuid)
            .ok_or_else(|| drifter_core::lang::t("That network is not saved.").to_string())?;
        for part in ["802-11-wireless-security", "802-1x", "wireguard", "vpn"] {
            if !setup.contains_key(part) {
                continue;
            }
            if let Some(secrets) = self.secrets(&saved.path, part) {
                for (name, values) in secrets {
                    let into = setup.entry(name).or_default();
                    for (key, value) in values {
                        into.insert(key, value);
                    }
                }
            }
        }
        apply(&mut setup);
        self.stale.store(true, Ordering::SeqCst);
        self.call(&saved.path, CONNECTION, "Update", &(setup,)).map(drop)
    }

    pub fn set_autojoin(&self, uuid: &str, on: bool) -> Result<(), String> {
        self.change(uuid, |setup| {
            setup.entry("connection".into()).or_default().insert("autoconnect".into(), OwnedValue::from(on));
        })
    }

    pub fn set_priority(&self, uuid: &str, priority: i32) -> Result<(), String> {
        self.change(uuid, |setup| {
            setup.entry("connection".into()).or_default().insert("autoconnect-priority".into(), OwnedValue::from(priority));
        })
    }

    pub fn set_metered(&self, uuid: &str, on: bool) -> Result<(), String> {
        self.change(uuid, |setup| {
            setup.entry("connection".into()).or_default().insert("metered".into(), OwnedValue::from(if on { 1i32 } else { 2i32 }));
        })
    }

    pub fn set_private(&self, uuid: &str, private: Private) -> Result<(), String> {
        self.change(uuid, |setup| {
            let wireless = setup.entry("802-11-wireless".into()).or_default();
            wireless.remove("cloned-mac-address");
            if let Ok(value) = OwnedValue::try_from(Value::from(private.word())) {
                wireless.insert("assigned-mac-address".into(), value);
            }
        })
    }

    pub fn set_powersave(&self, uuid: &str, off: bool) -> Result<(), String> {
        self.change(uuid, |setup| {
            setup.entry("802-11-wireless".into()).or_default().insert("powersave".into(), OwnedValue::from(if off { 2u32 } else { 0u32 }));
        })
    }

    pub fn vpn(&self, uuid: &str, on: bool) -> Result<(), String> {
        let objects = self.objects().ok_or_else(|| Missing::Service.words().to_string())?;
        let saved = self.saved_all(&objects);
        let (each, _) = saved
            .iter()
            .find(|(each, _)| each.uuid == uuid)
            .ok_or_else(|| drifter_core::lang::t("That VPN is not saved.").to_string())?;
        if on {
            return self.activate(&each.path, "/", "/").map(drop);
        }
        let active = objects
            .iter()
            .filter_map(|(at, faces)| faces.get(ACTIVE).map(|props| (at, props)))
            .find(|(_, props)| text(props, "Uuid") == uuid)
            .map(|(at, _)| at.clone())
            .ok_or_else(|| drifter_core::lang::tf("{} is not on.", &[&each.id]))?;
        self.call(ROOT, MANAGER, "DeactivateConnection", &(Self::object(&active)?,)).map(drop)
    }

    pub fn add_wireguard(&self, setup: &WireGuard) -> Result<(), String> {
        let iface: String = setup
            .name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .take(15)
            .collect();
        let iface = match iface.is_empty() {
            true => "wg0".to_string(),
            false => iface,
        };
        let mut connection: Part = HashMap::new();
        connection.insert("id", Value::from(setup.name.clone()));
        connection.insert("type", Value::from("wireguard"));
        connection.insert("interface-name", Value::from(iface));
        connection.insert("autoconnect", Value::from(false));
        let peers: Vec<HashMap<&str, Value>> = setup
            .peers
            .iter()
            .filter(|peer| !peer.public_key.is_empty())
            .map(|peer| {
                let mut out: HashMap<&str, Value> = HashMap::new();
                out.insert("public-key", Value::from(peer.public_key.clone()));
                out.insert("allowed-ips", Value::from(peer.allowed.clone()));
                if !peer.endpoint.is_empty() {
                    out.insert("endpoint", Value::from(peer.endpoint.clone()));
                }
                if !peer.preshared_key.is_empty() {
                    out.insert("preshared-key", Value::from(peer.preshared_key.clone()));
                    out.insert("preshared-key-flags", Value::from(0u32));
                }
                if peer.keepalive > 0 {
                    out.insert("persistent-keepalive", Value::from(peer.keepalive));
                }
                out
            })
            .collect();
        let mut guard: Part = HashMap::new();
        guard.insert("private-key", Value::from(setup.private_key.clone()));
        guard.insert("private-key-flags", Value::from(0u32));
        if setup.port > 0 {
            guard.insert("listen-port", Value::from(setup.port));
        }
        if setup.mtu > 0 {
            guard.insert("mtu", Value::from(setup.mtu));
        }
        guard.insert("peers", Value::from(peers));
        let split = |v6: bool| -> Vec<HashMap<&'static str, Value<'static>>> {
            setup
                .addresses
                .iter()
                .filter(|each| each.contains(':') == v6)
                .filter_map(|each| {
                    let (address, prefix) = each.split_once('/').unwrap_or((each.as_str(), if v6 { "128" } else { "32" }));
                    let prefix: u32 = prefix.parse().ok()?;
                    let mut out: HashMap<&str, Value> = HashMap::new();
                    out.insert("address", Value::from(address.to_string()));
                    out.insert("prefix", Value::from(prefix));
                    Some(out)
                })
                .collect()
        };
        let mut ip4: Part = HashMap::new();
        let v4 = split(false);
        match v4.is_empty() {
            true => {
                ip4.insert("method", Value::from("disabled"));
            }
            false => {
                ip4.insert("method", Value::from("manual"));
                ip4.insert("address-data", Value::from(v4));
                let dns: Vec<u32> = setup
                    .dns
                    .iter()
                    .filter_map(|each| each.parse::<std::net::Ipv4Addr>().ok())
                    .map(|ip| u32::from_ne_bytes(ip.octets()))
                    .collect();
                if !dns.is_empty() {
                    ip4.insert("dns", Value::from(dns));
                    ip4.insert("dns-priority", Value::from(-50i32));
                }
            }
        }
        let mut ip6: Part = HashMap::new();
        let v6 = split(true);
        match v6.is_empty() {
            true => {
                ip6.insert("method", Value::from("disabled"));
            }
            false => {
                ip6.insert("method", Value::from("manual"));
                ip6.insert("address-data", Value::from(v6));
                let dns: Vec<Vec<u8>> = setup
                    .dns
                    .iter()
                    .filter_map(|each| each.parse::<std::net::Ipv6Addr>().ok())
                    .map(|ip| ip.octets().to_vec())
                    .collect();
                if !dns.is_empty() {
                    ip6.insert("dns", Value::from(dns));
                }
            }
        }
        let mut whole: HashMap<&str, Part> = HashMap::new();
        whole.insert("connection", connection);
        whole.insert("wireguard", guard);
        whole.insert("ipv4", ip4);
        whole.insert("ipv6", ip6);
        self.stale.store(true, Ordering::SeqCst);
        self.call(SETTINGS, SETTINGS_FACE, "AddConnection", &(whole,)).map(drop)
    }

    pub fn points(&self, ssid: &str) -> Vec<Ap> {
        let Some(objects) = self.objects() else {
            return Vec::new();
        };
        let saved: Vec<Saved> = self.saved_all(&objects).into_iter().map(|(each, _)| each).collect();
        let wifi = build(&objects, &saved);
        let Some(card) = wifi.card.as_ref() else {
            return Vec::new();
        };
        let Some(wireless) = objects.get(&card.path).and_then(|faces| faces.get(WIRELESS)) else {
            return Vec::new();
        };
        let in_use = path(wireless, "ActiveAccessPoint");
        let mut aps: Vec<Ap> = paths(wireless, "AccessPoints")
            .iter()
            .filter_map(|at| objects.get(at).and_then(|faces| faces.get(POINT)).map(|props| (at, props)))
            .filter(|(_, props)| drifter_core::net::ssid_text(&bytes(props, "Ssid")) == ssid)
            .map(|(at, props)| {
                let guard = Guard {
                    flags: number(props, "Flags"),
                    wpa: number(props, "WpaFlags"),
                    rsn: number(props, "RsnFlags"),
                };
                let mhz = number(props, "Frequency");
                let rate = number(props, "MaxBitrate");
                let width = number(props, "Bandwidth");
                Ap {
                    bssid: text(props, "HwAddress").to_uppercase(),
                    in_use: *at == in_use,
                    channel: drifter_core::net::channel(mhz),
                    mhz,
                    rate: match rate {
                        0 => String::new(),
                        kbit => format!("{} Mbit/s", kbit / 1000),
                    },
                    width: match width {
                        0 => String::new(),
                        mhz => format!("{mhz} MHz"),
                    },
                    signal: props.get("Strength").and_then(|value| value.downcast_ref::<u8>().ok()).unwrap_or(0).min(100),
                    security: guard.words(),
                    flags: guard.flag_words(),
                }
            })
            .collect();
        aps.sort_by(|a, b| b.in_use.cmp(&a.in_use).then(b.signal.cmp(&a.signal)));
        aps
    }

    pub fn link(&self) -> Option<Link> {
        let objects = self.objects()?;
        let saved: Vec<Saved> = self.saved_all(&objects).into_iter().map(|(each, _)| each).collect();
        let card = build(&objects, &saved).card?;
        let device = objects.get(&card.path).and_then(|faces| faces.get(DEVICE))?;
        let mut fields: Vec<(String, String)> = Vec::new();
        if !card.hw.is_empty() {
            fields.push(("GENERAL.HWADDR".into(), card.hw.clone()));
        }
        let mut add = |prefix: &str, config: String, face: &str| {
            let Some(props) = objects.get(&config).and_then(|faces| faces.get(face)) else {
                return;
            };
            for (at, address) in addresses(props).into_iter().enumerate() {
                fields.push((format!("{prefix}.ADDRESS[{}]", at + 1), address));
            }
            let gateway = text(props, "Gateway");
            if !gateway.is_empty() {
                fields.push((format!("{prefix}.GATEWAY"), gateway));
            }
            for (at, server) in servers(props).into_iter().enumerate() {
                fields.push((format!("{prefix}.DNS[{}]", at + 1), server));
            }
        };
        add("IP4", path(device, "Ip4Config"), IP4);
        add("IP6", path(device, "Ip6Config"), IP6);
        let iw = iw();
        Some(Link {
            dbm: std::fs::read_to_string("/proc/net/wireless").ok().and_then(|text| drifter_core::net::link_dbm(&text, &card.iface)),
            bitrate: read(iw, &["dev", &card.iface, "link"]).and_then(|text| {
                text.lines()
                    .find_map(|line| line.trim().strip_prefix("tx bitrate:"))
                    .map(|rate| rate.split_whitespace().take(2).collect::<Vec<_>>().join(" "))
                    .filter(|rate| !rate.is_empty())
            }),
            device: fields,
            card: card.iface,
        })
    }

    pub fn dbm(&self, iface: &str) -> Vec<(String, f64)> {
        read(iw(), &["dev", iface, "scan", "dump"]).map(|dump| drifter_core::net::scan_dbm(&dump)).unwrap_or_default()
    }

    pub fn check_online(&self) -> Online {
        self.call(ROOT, MANAGER, "CheckConnectivity", &())
            .ok()
            .and_then(|reply| reply.body().deserialize::<u32>().ok())
            .map_or(Online::Unknown, Online::read)
    }

    pub fn wait(&self, until: impl Fn(&Wifi) -> bool, most: Duration) -> bool {
        let started = Instant::now();
        while started.elapsed() < most {
            if self.snapshot().is_some_and(|wifi| until(&wifi)) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        false
    }
}

fn iw() -> &'static str {
    ["/usr/sbin/iw", "/sbin/iw", "/usr/bin/iw"]
        .into_iter()
        .find(|path| std::path::Path::new(path).is_file())
        .unwrap_or("iw")
}

fn read(program: &str, args: &[&str]) -> Option<String> {
    std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn plain_bus(text: &str) -> String {
    let has = |words: &[&str]| words.iter().any(|word| text.contains(word));
    if has(&["PermissionDenied", "Not authorized", "not authorized"]) {
        return drifter_core::lang::t("This account is not allowed to change the network.").to_string();
    }
    if has(&["ServiceUnknown", "was not provided by any"]) {
        return Missing::Service.words().to_string();
    }
    if has(&["UnknownConnection", "UnknownDevice", "UnknownObject"]) {
        return drifter_core::lang::t("The network went away. Look again.").to_string();
    }
    if has(&["key-mgmt", "802-11-wireless-security"]) {
        return drifter_core::lang::t("The network asked for a kind of password Drifter cannot send. Join it from Wi-Fi Settings.").to_string();
    }
    if has(&["NoSecrets", "no secrets"]) {
        return drifter_core::lang::t("No password was given.").to_string();
    }
    text.rsplit(": ").next().unwrap_or(text).trim().to_string()
}

pub fn tell(summary: &str, body: &str) {
    let Ok(bus) = Connection::session() else {
        return;
    };
    let hints: HashMap<&str, Value> = HashMap::new();
    let _ = bus.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &(drifter_core::lang::t("Wi-Fi"), 0u32, "", summary, body, Vec::<&str>::new(), hints, 6000i32),
    );
}
