use std::sync::atomic::{AtomicU64, Ordering};

use drifter_core::blue::{Ask, Kind, Question};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const PATH: &str = "/org/drifter/bluetooth/agent";

pub type Reply = async_channel::Sender<Option<String>>;

pub enum Event {
    Ask(Ask, Option<Reply>),
    Done(u64),
}

struct Agent {
    tell: Box<dyn Fn(Event) + Send + Sync>,
    next: AtomicU64,
    open: std::sync::Mutex<Vec<u64>>,
    shown: std::sync::Mutex<Option<(String, u64)>>,
}

async fn who(bus: &zbus::Connection, device: &OwnedObjectPath) -> (String, String, Kind) {
    let get = |key: &'static str| async move {
        bus.call_method(Some("org.bluez"), device.as_str(), Some("org.freedesktop.DBus.Properties"), "Get", &("org.bluez.Device1", key))
            .await
            .ok()
            .and_then(|reply| reply.body().deserialize::<OwnedValue>().ok())
    };
    let address = get("Address").await.and_then(|value| value.downcast_ref::<String>().ok()).unwrap_or_default();
    let name = get("Alias").await.and_then(|value| value.downcast_ref::<String>().ok()).unwrap_or_default();
    let icon = get("Icon").await.and_then(|value| value.downcast_ref::<String>().ok()).unwrap_or_default();
    let class = get("Class").await.and_then(|value| value.downcast_ref::<u32>().ok());
    let appearance = get("Appearance").await.and_then(|value| value.downcast_ref::<u16>().ok());
    let kind = Kind::guess(&icon, class, appearance, &name);
    let name = match drifter_core::blue::named(&name, &address) {
        true => name,
        false => kind.unnamed().to_string(),
    };
    (address.to_uppercase(), name, kind)
}

impl Agent {
    async fn ask(&self, bus: &zbus::Connection, device: &OwnedObjectPath, question: Question) -> zbus::fdo::Result<String> {
        let (address, name, kind) = who(bus, device).await;
        let id = self.next.fetch_add(1, Ordering::SeqCst);
        let (reply, answer) = async_channel::bounded(1);
        if let Ok(mut open) = self.open.lock() {
            open.push(id);
        }
        (self.tell)(Event::Ask(Ask { id, address, name, kind, question }, Some(reply)));
        let heard = answer.recv().await;
        if let Ok(mut open) = self.open.lock() {
            open.retain(|each| *each != id);
        }
        match heard {
            Ok(Some(text)) => Ok(text),
            _ => Err(zbus::fdo::Error::AccessDenied("Rejected".into())),
        }
    }

    async fn show(&self, bus: &zbus::Connection, device: &OwnedObjectPath, code: String, entered: u16) {
        let (address, name, kind) = who(bus, device).await;
        let id = {
            let mut shown = self.shown.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            match shown.as_ref() {
                Some((path, id)) if path == device.as_str() => *id,
                _ => {
                    let id = self.next.fetch_add(1, Ordering::SeqCst);
                    *shown = Some((device.to_string(), id));
                    id
                }
            }
        };
        (self.tell)(Event::Ask(
            Ask {
                id,
                address,
                name,
                kind,
                question: Question::Show(code, entered),
            },
            None,
        ));
    }

    fn close_all(&self) {
        let open: Vec<u64> = self.open.lock().map(|open| open.clone()).unwrap_or_default();
        for id in open {
            (self.tell)(Event::Done(id));
        }
        if let Some((_, id)) = self.shown.lock().ok().and_then(|mut shown| shown.take()) {
            (self.tell)(Event::Done(id));
        }
    }
}

fn digits(text: &str, most: usize) -> Option<String> {
    let text = text.trim();
    (!text.is_empty() && text.len() <= most && text.chars().all(|c| c.is_ascii_alphanumeric())).then(|| text.to_string())
}

#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    fn release(&self) {
        self.close_all();
    }

    async fn request_pin_code(&self, #[zbus(connection)] bus: &zbus::Connection, device: OwnedObjectPath) -> zbus::fdo::Result<String> {
        let pin = self.ask(bus, &device, Question::TypePin).await?;
        digits(&pin, 16).ok_or_else(|| zbus::fdo::Error::AccessDenied("Rejected".into()))
    }

    async fn display_pin_code(&self, #[zbus(connection)] bus: &zbus::Connection, device: OwnedObjectPath, pincode: String) {
        self.show(bus, &device, pincode, 0).await;
    }

    async fn request_passkey(&self, #[zbus(connection)] bus: &zbus::Connection, device: OwnedObjectPath) -> zbus::fdo::Result<u32> {
        let key = self.ask(bus, &device, Question::Type).await?;
        key.trim()
            .parse::<u32>()
            .ok()
            .filter(|key| *key <= 999_999)
            .ok_or_else(|| zbus::fdo::Error::AccessDenied("Rejected".into()))
    }

    async fn display_passkey(&self, #[zbus(connection)] bus: &zbus::Connection, device: OwnedObjectPath, passkey: u32, entered: u16) {
        self.show(bus, &device, format!("{passkey:06}"), entered).await;
    }

    async fn request_confirmation(&self, #[zbus(connection)] bus: &zbus::Connection, device: OwnedObjectPath, passkey: u32) -> zbus::fdo::Result<()> {
        self.ask(bus, &device, Question::Confirm(passkey)).await.map(drop)
    }

    async fn request_authorization(&self, #[zbus(connection)] bus: &zbus::Connection, device: OwnedObjectPath) -> zbus::fdo::Result<()> {
        self.ask(bus, &device, Question::Allow).await.map(drop)
    }

    async fn authorize_service(&self, #[zbus(connection)] bus: &zbus::Connection, device: OwnedObjectPath, _uuid: String) -> zbus::fdo::Result<()> {
        let paired = bus
            .call_method(Some("org.bluez"), device.as_str(), Some("org.freedesktop.DBus.Properties"), "Get", &("org.bluez.Device1", "Paired"))
            .await
            .ok()
            .and_then(|reply| reply.body().deserialize::<OwnedValue>().ok())
            .and_then(|value| value.downcast_ref::<bool>().ok())
            .unwrap_or(false);
        match paired {
            true => Ok(()),
            false => self.ask(bus, &device, Question::Allow).await.map(drop),
        }
    }

    fn cancel(&self) {
        self.close_all();
    }
}

pub struct Handle {
    _bus: zbus::blocking::Connection,
}

fn agent(tell: impl Fn(Event) + Send + Sync + 'static) -> Agent {
    Agent {
        tell: Box::new(tell),
        next: AtomicU64::new(1),
        open: std::sync::Mutex::new(Vec::new()),
        shown: std::sync::Mutex::new(None),
    }
}

pub fn serve(tell: impl Fn(Event) + Send + Sync + 'static) -> Result<Handle, String> {
    let agent = agent(tell);
    let bus = zbus::blocking::connection::Builder::system()
        .and_then(|builder| builder.serve_at(PATH, agent))
        .and_then(|builder| builder.build())
        .map_err(|error| drifter_core::lang::tf("Cannot reach Bluetooth: {}", &[&error]))?;
    let path = zbus::zvariant::ObjectPath::try_from(PATH).map_err(|error| error.to_string())?;
    bus.call_method(Some("org.bluez"), "/org/bluez", Some("org.bluez.AgentManager1"), "RegisterAgent", &(&path, "KeyboardDisplay"))
        .map_err(|error| drifter_core::lang::tf("Bluetooth would not take the pairing helper: {}", &[&error]))?;
    bus.call_method(Some("org.bluez"), "/org/bluez", Some("org.bluez.AgentManager1"), "RequestDefaultAgent", &(&path,))
        .map_err(|error| drifter_core::lang::tf("Bluetooth would not take the pairing helper: {}", &[&error]))?;
    Ok(Handle { _bus: bus })
}

pub fn answer(reply: &Reply, said: Option<String>) {
    let _ = reply.try_send(said);
}


#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn the_helper_becomes_the_default_agent() {
        let handle = super::serve(|_| {}).expect("BlueZ takes the pairing helper");
        drop(handle);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn a_question_waits_for_the_answer_on_the_screen() {
        use drifter_core::blue::Question;
        use std::io::BufRead;
        let mut daemon = std::process::Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("dbus-daemon runs");
        let mut address = String::new();
        std::io::BufReader::new(daemon.stdout.take().unwrap()).read_line(&mut address).unwrap();
        let address = address.trim().to_string();
        let (seen, heard) = std::sync::mpsc::channel::<(Question, bool)>();
        let seen = std::sync::Mutex::new(seen);
        let agent = super::agent(move |event| {
            if let super::Event::Ask(ask, Some(reply)) = event {
                let yes = matches!(ask.question, Question::Confirm(482_913));
                let _ = seen.lock().unwrap().send((ask.question.clone(), yes));
                super::answer(&reply, yes.then(String::new));
            }
        });
        let server = zbus::blocking::connection::Builder::address(address.as_str())
            .and_then(|builder| builder.serve_at(super::PATH, agent))
            .and_then(|builder| builder.build())
            .expect("the helper serves");
        let me = server.unique_name().unwrap().to_string();
        let client = zbus::blocking::connection::Builder::address(address.as_str()).unwrap().build().unwrap();
        let device = zbus::zvariant::ObjectPath::try_from("/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF").unwrap();
        let yes = client.call_method(Some(me.as_str()), super::PATH, Some("org.bluez.Agent1"), "RequestConfirmation", &(&device, 482_913u32));
        let no = client.call_method(Some(me.as_str()), super::PATH, Some("org.bluez.Agent1"), "RequestConfirmation", &(&device, 111_111u32));
        let asked: Vec<(Question, bool)> = heard.try_iter().collect();
        let _ = daemon.kill();
        assert!(yes.is_ok(), "a yes on screen pairs: {yes:?}");
        assert!(no.is_err_and(|error| error.to_string().contains("Rejected")), "a no on screen refuses");
        assert_eq!(asked, [(Question::Confirm(482_913), true), (Question::Confirm(111_111), false)]);
    }
}
