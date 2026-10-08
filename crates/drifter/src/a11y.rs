use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zbus::message::Header;
use zbus::names::{BusName, UniqueName};

const NAME: &str = "org.freedesktop.a11y.Manager";
const PATH: &str = "/org/freedesktop/a11y/Manager";
const FACE: &str = "org.freedesktop.a11y.KeyboardMonitor";
const TRUSTED: [&str; 1] = ["org.gnome.Orca.KeyboardMonitor"];
const DOUBLE_TAP: Duration = Duration::from_millis(500);

#[derive(Default)]
struct Client {
    watch: bool,
    grab: bool,
    modifiers: Vec<u32>,
    keystrokes: Vec<(u32, u32)>,
}

#[derive(Default)]
struct Clients {
    held: HashMap<String, Client>,
    down: Vec<(String, u32)>,
    alone: Option<(u32, Instant, bool)>,
    passing: Vec<u32>,
}

pub struct Monitor {
    bus: zbus::blocking::Connection,
    clients: Arc<Mutex<Clients>>,
}

struct Face {
    clients: Arc<Mutex<Clients>>,
}

async fn trusted(bus: &zbus::Connection, sender: &UniqueName<'_>) -> bool {
    let Ok(proxy) = zbus::fdo::DBusProxy::new(bus).await else {
        return false;
    };
    for name in TRUSTED {
        let Ok(name) = BusName::try_from(name) else {
            continue;
        };
        if proxy
            .get_name_owner(name)
            .await
            .is_ok_and(|owner| owner.as_str() == sender.as_str())
        {
            return true;
        }
    }
    false
}

impl Face {
    async fn with(
        &self,
        bus: &zbus::Connection,
        header: &Header<'_>,
        change: impl FnOnce(&mut Client),
    ) -> zbus::fdo::Result<()> {
        let sender = header
            .sender()
            .ok_or_else(|| zbus::fdo::Error::AccessDenied("no sender".into()))?;
        if !trusted(bus, sender).await {
            return Err(zbus::fdo::Error::AccessDenied("only a screen reader may watch keys".into()));
        }
        let mut clients = self.clients.lock().map_err(|_| zbus::fdo::Error::Failed("poisoned".into()))?;
        change(clients.held.entry(sender.to_string()).or_default());
        Ok(())
    }
}

#[zbus::interface(name = "org.freedesktop.a11y.KeyboardMonitor")]
impl Face {
    async fn grab_keyboard(
        &self,
        #[zbus(connection)] bus: &zbus::Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        self.with(bus, &header, |client| client.grab = true).await
    }

    async fn ungrab_keyboard(
        &self,
        #[zbus(connection)] bus: &zbus::Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        self.with(bus, &header, |client| client.grab = false).await
    }

    async fn watch_keyboard(
        &self,
        #[zbus(connection)] bus: &zbus::Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        self.with(bus, &header, |client| client.watch = true).await
    }

    async fn unwatch_keyboard(
        &self,
        #[zbus(connection)] bus: &zbus::Connection,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        self.with(bus, &header, |client| client.watch = false).await
    }

    async fn set_key_grabs(
        &self,
        #[zbus(connection)] bus: &zbus::Connection,
        #[zbus(header)] header: Header<'_>,
        modifiers: Vec<u32>,
        keystrokes: Vec<(u32, u32)>,
    ) -> zbus::fdo::Result<()> {
        self.with(bus, &header, |client| {
            client.modifiers = modifiers;
            client.keystrokes = keystrokes;
        })
        .await
    }
}

pub struct Key {
    pub released: bool,
    pub state: u32,
    pub keysym: u32,
    pub unichar: u32,
    pub keycode: u16,
}

const LOCKS: u32 = 0x2 | 0x10;

impl Clients {
    fn route(&mut self, key: &Key, now: Instant) -> (Vec<String>, bool) {
        let mut told = Vec::new();
        let mut eaten = false;
        if key.released && let Some(at) = self.passing.iter().position(|sym| *sym == key.keysym) {
            self.passing.swap_remove(at);
            let watchers = self.held.iter().filter(|(_, client)| client.watch).map(|(name, _)| name.clone());
            return (watchers.collect(), false);
        }
        for (name, client) in &self.held {
            let is_modifier = client.modifiers.contains(&key.keysym);
            let under_modifier = self
                .down
                .iter()
                .any(|(owner, sym)| owner == name && *sym != key.keysym);
            let stroke = client
                .keystrokes
                .iter()
                .any(|(sym, mask)| *sym == key.keysym && (key.state & !LOCKS) == (*mask & !LOCKS));
            let grabbed = client.grab || is_modifier || under_modifier || stroke;
            if grabbed {
                eaten = true;
            }
            if grabbed || client.watch {
                told.push(name.clone());
            }
        }
        let modifier = self
            .held
            .iter()
            .find(|(_, client)| client.modifiers.contains(&key.keysym))
            .map(|(name, _)| name.clone());
        if let Some(owner) = modifier {
            match key.released {
                false => {
                    let again = self
                        .alone
                        .is_some_and(|(sym, at, alone)| sym == key.keysym && alone && now.duration_since(at) < DOUBLE_TAP);
                    if again {
                        self.alone = None;
                        self.passing.push(key.keysym);
                        let all_grab = self.held.values().any(|client| client.grab);
                        return (told, all_grab);
                    }
                    self.alone = Some((key.keysym, now, true));
                    if !self.down.iter().any(|(_, sym)| *sym == key.keysym) {
                        self.down.push((owner, key.keysym));
                    }
                }
                true => {
                    self.down.retain(|(_, sym)| *sym != key.keysym);
                    if let Some((sym, _, alone)) = self.alone.as_mut()
                        && *sym == key.keysym
                        && *alone
                    {
                        self.alone = Some((key.keysym, now, true));
                    }
                }
            }
        } else if !key.released {
            if let Some((_, _, alone)) = self.alone.as_mut() {
                *alone = false;
            }
        }
        (told, eaten)
    }
}

impl Monitor {
    pub fn start() -> Result<Monitor, String> {
        let clients = Arc::new(Mutex::new(Clients::default()));
        let bus = zbus::blocking::connection::Builder::session()
            .and_then(|builder| builder.name(NAME))
            .and_then(|builder| builder.serve_at(PATH, Face { clients: clients.clone() }))
            .and_then(|builder| builder.build())
            .map_err(|error| format!("cannot offer the keyboard to a screen reader: {error}"))?;
        let watched = clients.clone();
        let names = bus.clone();
        std::thread::Builder::new()
            .name("drifter-a11y".to_string())
            .spawn(move || {
                let Ok(proxy) = zbus::blocking::fdo::DBusProxy::new(&names) else {
                    return;
                };
                let Ok(changes) = proxy.receive_name_owner_changed() else {
                    return;
                };
                for change in changes {
                    let Ok(args) = change.args() else {
                        continue;
                    };
                    if args.new_owner().is_none()
                        && let Ok(mut clients) = watched.lock()
                    {
                        let gone = args.name().to_string();
                        clients.held.remove(&gone);
                        clients.down.retain(|(owner, _)| *owner != gone);
                    }
                }
            })
            .map_err(|error| error.to_string())?;
        Ok(Monitor { bus, clients })
    }

    pub fn forget(&self) {
        if let Ok(mut clients) = self.clients.lock() {
            *clients = Clients::default();
        }
    }

    pub fn key(&self, key: Key) -> bool {
        let Ok(mut clients) = self.clients.lock() else {
            return false;
        };
        if clients.held.is_empty() {
            return false;
        }
        let (told, eaten) = clients.route(&key, Instant::now());
        drop(clients);
        let body = (key.released, key.state, key.keysym, key.unichar, key.keycode);
        for name in told {
            if let Err(error) = self.bus.emit_signal(Some(name.as_str()), PATH, FACE, "KeyEvent", &body) {
                log!("drifter: a screen reader missed a key: {error}");
            }
        }
        eaten
    }
}

pub fn stop_orca() {
    let me = unsafe { libc::getuid() };
    let Ok(entries) = std::fs::read_dir("/proc") else {
        return;
    };
    for entry in entries.flatten() {
        let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse::<i32>().ok()) else {
            continue;
        };
        let path = entry.path();
        let named = std::fs::read_to_string(path.join("comm")).is_ok_and(|comm| comm.trim() == "orca");
        let mine = std::fs::metadata(&path).is_ok_and(|meta| std::os::unix::fs::MetadataExt::uid(&meta) == me);
        if named && mine {
            unsafe {
                libc::kill(pid, libc::SIGTERM);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INSERT: u32 = 0xff63;
    const A: u32 = 0x61;

    fn key(keysym: u32, released: bool) -> Key {
        Key {
            released,
            state: 0,
            keysym,
            unichar: 0,
            keycode: 0,
        }
    }

    fn orca() -> Clients {
        let mut clients = Clients::default();
        clients.held.insert(
            ":1.7".into(),
            Client {
                watch: true,
                modifiers: vec![INSERT],
                ..Client::default()
            },
        );
        clients
    }

    #[test]
    fn plain_keys_reach_the_app_and_the_reader_hears_them() {
        let mut clients = orca();
        let (told, eaten) = clients.route(&key(A, false), Instant::now());
        assert_eq!(told, [":1.7"]);
        assert!(!eaten);
    }

    #[test]
    fn the_reader_key_and_keys_under_it_are_eaten() {
        let mut clients = orca();
        let now = Instant::now();
        assert!(clients.route(&key(INSERT, false), now).1);
        assert!(clients.route(&key(A, false), now).1, "Insert+A goes to the reader only");
        assert!(clients.route(&key(A, true), now).1);
        assert!(clients.route(&key(INSERT, true), now).1);
        assert!(!clients.route(&key(A, false), now).1, "after Insert lets go, A is plain again");
    }

    #[test]
    fn a_quick_second_tap_of_the_reader_key_passes_through() {
        let mut clients = orca();
        let now = Instant::now();
        assert!(clients.route(&key(INSERT, false), now).1);
        assert!(clients.route(&key(INSERT, true), now).1);
        let soon = now + Duration::from_millis(200);
        assert!(!clients.route(&key(INSERT, false), soon).1, "the double tap reaches the app");
        assert!(!clients.route(&key(INSERT, true), soon).1);
    }

    #[test]
    fn a_grabbed_keystroke_is_eaten_with_its_modifiers_only() {
        let mut clients = orca();
        clients.held.get_mut(":1.7").unwrap().keystrokes = vec![(A, 0x4)];
        let mut ctrl_a = key(A, false);
        ctrl_a.state = 0x4;
        assert!(clients.route(&ctrl_a, Instant::now()).1);
        ctrl_a.state = 0x4 | 0x10;
        assert!(clients.route(&ctrl_a, Instant::now()).1, "Num Lock does not matter");
        assert!(!clients.route(&key(A, false), Instant::now()).1);
    }

    #[test]
    fn grab_keyboard_eats_everything() {
        let mut clients = orca();
        clients.held.get_mut(":1.7").unwrap().grab = true;
        assert!(clients.route(&key(A, false), Instant::now()).1);
    }
}
