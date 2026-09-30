use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use drifter_core::Image;
use drifter_core::notify::{self, Note};
use smithay::reexports::calloop::channel::{
    Channel, Sender as LoopSender, channel as loop_channel,
};
use zbus::zvariant::{OwnedValue, Structure, Value};

const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";
const ICON: u32 = 40;
const LONGEST_SUMMARY: usize = 200;
const LONGEST_BODY: usize = 1000;

pub enum Event {
    Show(Note),
    Close(u32),
}

pub enum Reply {
    Closed(u32, u32),
    Action(u32, String),
}

pub struct Notes {
    replies: Sender<Reply>,
    next: Arc<AtomicU32>,
}

impl Notes {
    pub fn closed(&self, id: u32, reason: u32) {
        let _ = self.replies.send(Reply::Closed(id, reason));
    }

    pub fn action(&self, id: u32, key: &str) {
        let _ = self.replies.send(Reply::Action(id, key.to_string()));
    }

    pub fn next_id(&self) -> u32 {
        self.next.fetch_add(1, Ordering::SeqCst)
    }
}

pub fn start() -> Result<(Notes, Channel<Event>), String> {
    let (event_tx, event_rx) = loop_channel::<Event>();
    let (reply_tx, reply_rx) = channel::<Reply>();
    let next = Arc::new(AtomicU32::new(1));
    let counter = next.clone();
    std::thread::Builder::new()
        .name("drifter-notes".to_string())
        .spawn(move || serve(counter, event_tx, reply_rx))
        .map_err(|e| format!("cannot start the notification thread: {e}"))?;
    Ok((
        Notes {
            replies: reply_tx,
            next,
        },
        event_rx,
    ))
}

fn serve(next: Arc<AtomicU32>, events: LoopSender<Event>, replies: Receiver<Reply>) {
    let address = match crate::tray::bus_address() {
        Ok(address) => address,
        Err(error) => {
            eprintln!("drifter: no notifications: {error}");
            return;
        }
    };
    let server = Server { next, events };
    let built = zbus::blocking::connection::Builder::address(address.as_str())
        .and_then(|builder| builder.name(NAME))
        .and_then(|builder| builder.serve_at(PATH, server))
        .and_then(|builder| builder.build());
    let connection = match built {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("drifter: no notifications: {error}");
            return;
        }
    };
    for reply in replies {
        let sent = match reply {
            Reply::Closed(id, reason) => connection.emit_signal(
                None::<&str>,
                PATH,
                NAME,
                "NotificationClosed",
                &(id, reason),
            ),
            Reply::Action(id, key) => connection.emit_signal(
                None::<&str>,
                PATH,
                NAME,
                "ActionInvoked",
                &(id, key.as_str()),
            ),
        };
        if let Err(error) = sent {
            eprintln!("drifter: a notification signal failed: {error}");
        }
    }
}

struct Server {
    next: Arc<AtomicU32>,
    events: LoopSender<Event>,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Server {
    fn get_capabilities(&self) -> Vec<String> {
        ["body", "actions", "icon-static"]
            .map(String::from)
            .to_vec()
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(
        &mut self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
    ) -> u32 {
        let id = match replaces_id {
            0 => self.next.fetch_add(1, Ordering::SeqCst),
            held => {
                self.next
                    .fetch_max(held.saturating_add(1), Ordering::SeqCst);
                held
            }
        };
        let summary: String = summary.chars().take(LONGEST_SUMMARY).collect();
        let body: String = body.chars().take(LONGEST_BODY).collect();
        let critical = hints
            .get("urgency")
            .and_then(|value| value.downcast_ref::<u8>().ok())
            == Some(2);
        let icon = ["image-data", "image_data", "icon_data"]
            .iter()
            .find_map(|key| hints.get(*key).and_then(pixels))
            .or_else(|| {
                ["image-path", "image_path"].iter().find_map(|key| {
                    hints
                        .get(*key)
                        .and_then(|value| value.downcast_ref::<String>().ok())
                        .and_then(|name| named(&name))
                })
            })
            .or_else(|| named(&app_icon));
        let note = Note {
            id,
            app: app_name,
            summary,
            body,
            actions: notify::actions(&actions),
            expire: notify::expire_ms(expire_timeout, critical),
            icon,
        };
        let _ = self.events.send(Event::Show(note));
        id
    }

    fn close_notification(&self, id: u32) {
        let _ = self.events.send(Event::Close(id));
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "Drifter".to_string(),
            "drifter".to_string(),
            env!("CARGO_PKG_VERSION").to_string(),
            "1.2".to_string(),
        )
    }
}

fn pixels(value: &OwnedValue) -> Option<Image> {
    let structure = value.downcast_ref::<Structure>().ok()?;
    let fields = structure.fields();
    let int = |at: usize| fields.get(at)?.downcast_ref::<i32>().ok();
    let alpha = fields.get(3)?.downcast_ref::<bool>().ok()?;
    let data: Vec<u8> = match fields.get(6)? {
        Value::Array(array) => array
            .iter()
            .filter_map(|byte| byte.downcast_ref::<u8>().ok())
            .collect(),
        _ => return None,
    };
    notify::from_pixels(int(0)?, int(1)?, int(2)?, alpha, int(5)?, &data)
        .map(|image| image.scaled(ICON, ICON))
}

fn named(name: &str) -> Option<Image> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let path = match name.strip_prefix("file://") {
        Some(path) => std::path::PathBuf::from(path),
        None if name.starts_with('/') => std::path::PathBuf::from(name),
        None => drifter_core::icon::find(name, None, 48)?,
    };
    drifter_core::image::load(&path)
        .ok()
        .map(|image| image.scaled(ICON, ICON))
}
