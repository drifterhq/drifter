use std::sync::Mutex;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use smithay_client_toolkit::reexports::calloop::channel::Sender as LoopSender;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const PATH: &str = "/org/drifter/agent";
const PATIENCE: Duration = Duration::from_secs(90);

pub enum Question {
    Pin(String),
    Passkey(String),
    Confirm(String, u32),
    Show(String),
}

pub type LoopAsks = LoopSender<Asked>;

pub struct Asked {
    pub question: Question,
    pub reply: Option<Sender<Option<String>>>,
}

struct Agent {
    name: String,
    asks: LoopSender<Asked>,
}

impl Agent {
    fn ask(&self, question: Question) -> zbus::fdo::Result<String> {
        let (reply, answer): (Sender<Option<String>>, Receiver<Option<String>>) = channel();
        self.asks
            .send(Asked {
                question,
                reply: Some(reply),
            })
            .map_err(|_| zbus::fdo::Error::Failed("Settings closed".into()))?;
        match answer.recv_timeout(PATIENCE) {
            Ok(Some(text)) => Ok(text),
            _ => Err(zbus::fdo::Error::AccessDenied("not paired".into())),
        }
    }

    fn show(&self, words: String) {
        let _ = self.asks.send(Asked {
            question: Question::Show(words),
            reply: None,
        });
    }
}

#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    fn release(&self) {}

    fn request_pin_code(&self, _device: OwnedObjectPath) -> zbus::fdo::Result<String> {
        let pin = self.ask(Question::Pin(self.name.clone()))?;
        match pin.trim() {
            "" => Err(zbus::fdo::Error::AccessDenied("no pin".into())),
            pin => Ok(pin.to_string()),
        }
    }

    fn display_pin_code(&self, _device: OwnedObjectPath, pincode: String) {
        self.show(format!("Type {pincode} on {}, then press Enter.", self.name));
    }

    fn request_passkey(&self, _device: OwnedObjectPath) -> zbus::fdo::Result<u32> {
        let key = self.ask(Question::Passkey(self.name.clone()))?;
        key.trim()
            .parse::<u32>()
            .ok()
            .filter(|key| *key <= 999_999)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("a passkey is up to six digits".into()))
    }

    fn display_passkey(&self, _device: OwnedObjectPath, passkey: u32, _entered: u16) {
        self.show(format!("Type {passkey:06} on {}, then press Enter.", self.name));
    }

    fn request_confirmation(&self, _device: OwnedObjectPath, passkey: u32) -> zbus::fdo::Result<()> {
        match passkey {
            0 => Ok(()),
            _ => self.ask(Question::Confirm(self.name.clone(), passkey)).map(drop),
        }
    }

    fn request_authorization(&self, _device: OwnedObjectPath) {}

    fn authorize_service(&self, _device: OwnedObjectPath, _uuid: String) {}

    fn cancel(&self) {}
}

fn device_path(bus: &zbus::blocking::Connection, mac: &str) -> Result<OwnedObjectPath, String> {
    let objects = zbus::blocking::fdo::ObjectManagerProxy::builder(bus)
        .destination("org.bluez")
        .and_then(|builder| builder.path("/"))
        .and_then(|builder| builder.build())
        .and_then(|manager| manager.get_managed_objects().map_err(zbus::Error::from))
        .map_err(|error| format!("Bluetooth did not answer: {error}"))?;
    objects
        .into_iter()
        .find(|(_, faces)| {
            faces
                .get("org.bluez.Device1")
                .and_then(|device| device.get("Address"))
                .and_then(|address: &OwnedValue| String::try_from(address.clone()).ok())
                .is_some_and(|address| address.eq_ignore_ascii_case(mac))
        })
        .map(|(path, _)| path)
        .ok_or_else(|| "The device went away. Search again.".to_string())
}

static PAIRING: Mutex<()> = Mutex::new(());

pub fn pair(mac: &str, name: &str, asks: LoopSender<Asked>) -> Result<(), String> {
    let _one = PAIRING.lock().map_err(|_| "another pairing broke".to_string())?;
    let agent = Agent {
        name: name.to_string(),
        asks,
    };
    let bus = zbus::blocking::connection::Builder::system()
        .and_then(|builder| builder.serve_at(PATH, agent))
        .and_then(|builder| builder.build())
        .map_err(|error| format!("Cannot reach Bluetooth: {error}"))?;
    let path = zbus::zvariant::ObjectPath::try_from(PATH).map_err(|error| error.to_string())?;
    let manager = zbus::blocking::Proxy::new(&bus, "org.bluez", "/org/bluez", "org.bluez.AgentManager1")
        .map_err(|error| error.to_string())?;
    manager
        .call_method("RegisterAgent", &(&path, "KeyboardDisplay"))
        .map_err(|error| format!("Bluetooth would not take the pairing helper: {error}"))?;
    let device = device_path(&bus, mac)?;
    let paired = zbus::blocking::Proxy::new(&bus, "org.bluez", device.as_ref(), "org.bluez.Device1")
        .map_err(|error| error.to_string())
        .and_then(|proxy| {
            proxy
                .call_method("Pair", &())
                .map(drop)
                .map_err(|error| said(&error))
        });
    let _ = manager.call_method("UnregisterAgent", &(&path,));
    paired
}

fn said(error: &zbus::Error) -> String {
    let text = error.to_string();
    if text.contains("AuthenticationFailed") || text.contains("AuthenticationRejected") {
        return "The code did not match. Try again.".to_string();
    }
    if text.contains("AuthenticationCanceled") {
        return "Pairing was stopped.".to_string();
    }
    if text.contains("AlreadyExists") {
        return "It is already paired.".to_string();
    }
    if text.contains("ConnectionAttemptFailed") || text.contains("PageTimeout") {
        return "It did not answer. Put it in pairing mode and try again.".to_string();
    }
    format!("It did not pair: {text}")
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn pairs_the_fake_buds_through_the_agent() {
        use smithay_client_toolkit::reexports::calloop::EventLoop;
        use smithay_client_toolkit::reexports::calloop::channel::{Event, channel};

        let mac = std::env::var("DRIFTER_TEST_BLUE").expect("set DRIFTER_TEST_BLUE to a device address");
        let (asks, heard) = channel::<super::Asked>();
        let pairing = std::thread::spawn(move || super::pair(&mac, "test device", asks));
        let mut events: EventLoop<Vec<String>> = EventLoop::try_new().unwrap();
        events
            .handle()
            .insert_source(heard, |event, _, seen: &mut Vec<String>| {
                if let Event::Msg(asked) = event {
                    let (said, answer) = match asked.question {
                        super::Question::Pin(_) => ("pin".to_string(), "0000".to_string()),
                        super::Question::Passkey(_) => ("passkey".to_string(), "0".to_string()),
                        super::Question::Confirm(_, key) => (format!("confirm {key:06}"), "yes".to_string()),
                        super::Question::Show(words) => (words, String::new()),
                    };
                    seen.push(said);
                    if let Some(reply) = asked.reply {
                        let _ = reply.send(Some(answer));
                    }
                }
            })
            .unwrap();
        let mut seen = Vec::new();
        while !pairing.is_finished() {
            events.dispatch(Some(std::time::Duration::from_millis(50)), &mut seen).unwrap();
        }
        println!("the agent was asked: {seen:?}");
        pairing.join().unwrap().expect("it pairs");
    }
}
