use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use crate::bus::{Setup, text};

const PATH: &str = "/org/freedesktop/NetworkManager/SecretAgent";
const NM: &str = "org.freedesktop.NetworkManager";
const AGENTS: &str = "/org/freedesktop/NetworkManager/AgentManager";
const AGENT_MANAGER: &str = "org.freedesktop.NetworkManager.AgentManager";
const IDENTITY: &str = "org.drifter.network";
const ALLOW_INTERACTION: u32 = 0x1;
const REQUEST_NEW: u32 = 0x2;

pub type Reply = async_channel::Sender<Option<String>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Want {
    Password,
    Wep,
    Enterprise,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub id: u64,
    pub ssid: String,
    pub name: String,
    pub want: Want,
    pub again: bool,
}

pub enum Event {
    Ask(Ask, Reply),
    Done(u64),
    Settings(String),
    Vpn(String),
    Lost,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "org.freedesktop.NetworkManager.SecretAgent")]
pub enum Refusal {
    #[zbus(error)]
    ZBus(zbus::Error),
    UserCanceled(String),
    NoSecrets(String),
}

struct Agent {
    tell: std::sync::Arc<dyn Fn(Event) + Send + Sync>,
    next: AtomicU64,
    open: Mutex<Vec<(String, String, u64, Reply)>>,
}

fn part<'a>(setup: &'a Setup, name: &str) -> Option<&'a HashMap<String, OwnedValue>> {
    setup.get(name)
}

fn flags_of(setup: &Setup, name: &str, key: &str) -> u32 {
    part(setup, name)
        .and_then(|props| props.get(&format!("{key}-flags")))
        .and_then(|value| value.downcast_ref::<u32>().ok())
        .unwrap_or(0)
}

fn owned(text: &str) -> OwnedValue {
    OwnedValue::try_from(Value::from(text.to_string())).unwrap_or_else(|_| OwnedValue::from(0u32))
}

fn answer_of(name: &str, values: HashMap<String, String>) -> Setup {
    let mut out: Setup = HashMap::new();
    let props = out.entry(name.to_string()).or_default();
    for (key, value) in values {
        props.insert(key, owned(&value));
    }
    out
}

async fn off_thread<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    let (done, wait) = async_channel::bounded(1);
    std::thread::Builder::new()
        .name("drifter-net-keyring".into())
        .spawn(move || {
            let _ = done.try_send(work());
        })
        .ok()?;
    wait.recv().await.ok()
}

impl Agent {
    async fn ask(&self, path: &str, setting: &str, ask: Ask) -> Option<String> {
        let (reply, heard) = async_channel::bounded(1);
        let id = ask.id;
        if let Ok(mut open) = self.open.lock() {
            open.push((path.to_string(), setting.to_string(), id, reply.clone()));
        }
        (self.tell)(Event::Ask(ask, reply));
        let said = heard.recv().await.ok().flatten();
        if let Ok(mut open) = self.open.lock() {
            open.retain(|(_, _, each, _)| *each != id);
        }
        (self.tell)(Event::Done(id));
        said
    }
}

#[zbus::interface(name = "org.freedesktop.NetworkManager.SecretAgent")]
impl Agent {
    async fn get_secrets(
        &self,
        connection: Setup,
        connection_path: OwnedObjectPath,
        setting_name: String,
        _hints: Vec<String>,
        flags: u32,
    ) -> Result<Setup, Refusal> {
        let empty = HashMap::new();
        let about = part(&connection, "connection").unwrap_or(&empty);
        let uuid = text(about, "uuid");
        let id = text(about, "id");
        let ssid = part(&connection, "802-11-wireless")
            .and_then(|props| props.get("ssid"))
            .and_then(|value| value.try_clone().ok())
            .and_then(|value| Vec::<u8>::try_from(value).ok())
            .map(|bytes| drifter_core::net::ssid_text(&bytes))
            .filter(|ssid| !ssid.is_empty())
            .unwrap_or_else(|| id.clone());
        let key_mgmt = part(&connection, "802-11-wireless-security").map(|props| text(props, "key-mgmt")).unwrap_or_default();
        let (want, key) = match (setting_name.as_str(), key_mgmt.as_str()) {
            ("802-1x", _) | ("802-11-wireless-security", "wpa-eap" | "ieee8021x") => (Some(Want::Enterprise), "password"),
            ("802-11-wireless-security", "none") => (Some(Want::Wep), "wep-key0"),
            ("802-11-wireless-security", _) => (Some(Want::Password), "psk"),
            _ => (None, ""),
        };
        let (look_uuid, look_part) = (uuid.clone(), setting_name.clone());
        let kept = off_thread(move || crate::keyring::lookup(&look_uuid, &look_part)).await.unwrap_or_default();
        if flags & REQUEST_NEW == 0 && !kept.is_empty() && (want.is_none() || kept.contains_key(key)) {
            return Ok(answer_of(&setting_name, kept));
        }
        let Some(want) = want else {
            if flags & ALLOW_INTERACTION != 0 {
                (self.tell)(Event::Vpn(id.clone()));
            }
            return Err(Refusal::NoSecrets(format!("Drifter cannot ask for {setting_name} secrets")));
        };
        if flags & ALLOW_INTERACTION == 0 {
            return Err(Refusal::NoSecrets("No secrets kept and no asking allowed".into()));
        }
        let part_name = match want {
            Want::Enterprise => "802-1x",
            _ => "802-11-wireless-security",
        };
        if want == Want::Enterprise {
            let identity = part(&connection, "802-1x").map(|props| text(props, "identity")).unwrap_or_default();
            if identity.trim().is_empty() {
                (self.tell)(Event::Settings(ssid));
                return Err(Refusal::UserCanceled("The sign-in opens in Settings".into()));
            }
        }
        let ask = Ask {
            id: self.next.fetch_add(1, Ordering::SeqCst),
            ssid,
            name: id,
            want,
            again: flags & REQUEST_NEW != 0,
        };
        match self.ask(connection_path.as_str(), &setting_name, ask).await {
            Some(secret) => {
                let mut values = HashMap::new();
                values.insert(key.to_string(), secret);
                Ok(answer_of(part_name, values))
            }
            None => Err(Refusal::UserCanceled("Canceled".into())),
        }
    }

    fn cancel_get_secrets(&self, connection_path: OwnedObjectPath, setting_name: String) {
        let found: Vec<(u64, Reply)> = self
            .open
            .lock()
            .map(|open| {
                open.iter()
                    .filter(|(path, setting, _, _)| path == connection_path.as_str() && *setting == setting_name)
                    .map(|(_, _, id, reply)| (*id, reply.clone()))
                    .collect()
            })
            .unwrap_or_default();
        for (id, reply) in found {
            let _ = reply.try_send(None);
            (self.tell)(Event::Done(id));
        }
    }

    async fn save_secrets(&self, connection: Setup, _connection_path: OwnedObjectPath) {
        let empty = HashMap::new();
        let about = part(&connection, "connection").unwrap_or(&empty);
        let uuid = text(about, "uuid");
        let id = text(about, "id");
        let mut keep: Vec<(String, String, String)> = Vec::new();
        for (name, key) in [
            ("802-11-wireless-security", "psk"),
            ("802-11-wireless-security", "wep-key0"),
            ("802-1x", "password"),
            ("wireguard", "private-key"),
        ] {
            let value = part(&connection, name).map(|props| text(props, key)).unwrap_or_default();
            if !value.is_empty() && flags_of(&connection, name, key) & 0x1 != 0 {
                keep.push((name.to_string(), key.to_string(), value));
            }
        }
        if let Some(secrets) = part(&connection, "vpn")
            .and_then(|props| props.get("secrets"))
            .and_then(|value| value.try_clone().ok())
            .and_then(|value| HashMap::<String, String>::try_from(value).ok())
        {
            for (key, value) in secrets {
                keep.push(("vpn".to_string(), key, value));
            }
        }
        if keep.is_empty() {
            return;
        }
        off_thread(move || {
            for (name, key, value) in keep {
                crate::keyring::store(&uuid, &id, &name, &key, &value);
            }
        })
        .await;
    }

    async fn delete_secrets(&self, connection: Setup, _connection_path: OwnedObjectPath) {
        let empty = HashMap::new();
        let uuid = text(part(&connection, "connection").unwrap_or(&empty), "uuid");
        if !uuid.is_empty() {
            off_thread(move || crate::keyring::forget(&uuid)).await;
        }
    }
}

pub struct Handle {
    _bus: zbus::blocking::Connection,
}

fn register(bus: &zbus::blocking::Connection) -> Result<(), String> {
    bus.call_method(Some(NM), AGENTS, Some(AGENT_MANAGER), "RegisterWithCapabilities", &(IDENTITY, 0u32))
        .map(drop)
        .map_err(|error| drifter_core::lang::tf("NetworkManager would not take the password helper: {}", &[&error]))
}

pub fn serve(tell: impl Fn(Event) + Send + Sync + 'static) -> Result<Handle, String> {
    let tell: std::sync::Arc<dyn Fn(Event) + Send + Sync> = std::sync::Arc::new(tell);
    let lost = tell.clone();
    let agent = Agent {
        tell,
        next: AtomicU64::new(1),
        open: Mutex::new(Vec::new()),
    };
    let bus = zbus::blocking::connection::Builder::system()
        .and_then(|builder| builder.serve_at(PATH, agent))
        .and_then(|builder| builder.build())
        .map_err(|error| drifter_core::lang::tf("Cannot reach NetworkManager: {}", &[&error]))?;
    register(&bus)?;
    let rule = format!("type='signal',sender='org.freedesktop.DBus',member='NameOwnerChanged',arg0='{NM}'");
    if let Ok(rule) = zbus::MatchRule::try_from(rule.as_str())
        && let Ok(messages) = zbus::blocking::MessageIterator::for_match_rule(rule, &bus, Some(8))
    {
        let again = bus.clone();
        let _ = std::thread::Builder::new().name("drifter-net-agent".into()).spawn(move || {
            for message in messages {
                let Ok(message) = message else {
                    break;
                };
                let back = message
                    .body()
                    .deserialize::<(String, String, String)>()
                    .is_ok_and(|(_, _, owner)| !owner.is_empty());
                if back {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let _ = register(&again);
                }
            }
            lost(Event::Lost);
        });
    }
    Ok(Handle { _bus: bus })
}

pub fn answer(reply: &Reply, said: Option<String>) {
    let _ = reply.try_send(said);
}
