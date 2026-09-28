use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use drifter_core::image::Image;
use smithay::reexports::calloop::channel::{Channel, SyncSender, sync_channel};

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const ITEM: &str = "org.kde.StatusNotifierItem";

pub struct Icon {
    pub service: String,
    pub title: String,
    pub image: Image,
}

pub enum Click {
    Left,
    Right,
    Middle,
}

pub struct Tray {
    clicks: Sender<(String, Click, i32, i32)>,
}

impl Tray {
    pub fn click(&self, service: &str, click: Click, x: i32, y: i32) {
        let _ = self.clicks.send((service.to_string(), click, x, y));
    }
}

pub fn start() -> Result<(Tray, Channel<Vec<Icon>>), String> {
    let (icon_tx, icon_rx) = sync_channel::<Vec<Icon>>(4);
    let (click_tx, click_rx) = channel();
    let items: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let serving = items.clone();
    let sender = icon_tx.clone();
    std::thread::Builder::new()
        .name("drifter-tray".to_string())
        .spawn(move || serve(serving, sender, click_rx))
        .map_err(|e| format!("cannot start the tray thread: {e}"))?;

    Ok((Tray { clicks: click_tx }, icon_rx))
}

fn bus_address() -> Option<String> {
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some() {
        return None;
    }
    let uid = smithay::reexports::rustix::process::getuid().as_raw();
    let path = format!("/run/user/{uid}/bus");
    std::path::Path::new(&path)
        .exists()
        .then(|| format!("unix:path={path}"))
}

struct Watcher {
    items: Arc<Mutex<Vec<String>>>,
    wake: SyncSender<Vec<Icon>>,
    connection: Option<zbus::blocking::Connection>,
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    fn register_status_notifier_item(
        &mut self,
        service: &str,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) {
        let sender = header.sender().map(|name| name.to_string());
        let full = match (service.starts_with('/'), sender) {
            (true, Some(owner)) => format!("{owner}{service}"),
            _ => service.to_string(),
        };

        let mut items = self.items.lock().unwrap();
        if !items.contains(&full) {
            items.push(full);
        }
        drop(items);
        self.refresh();
    }

    fn register_status_notifier_host(&mut self, _service: &str) {}

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.lock().unwrap().clone()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }
}

impl Watcher {
    fn refresh(&self) {
        let Some(connection) = self.connection.as_ref() else {
            return;
        };
        let names = self.items.lock().unwrap().clone();
        let icons = names
            .iter()
            .filter_map(|service| read_icon(connection, service))
            .collect();
        let _ = self.wake.send(icons);
    }
}

fn split(service: &str) -> (String, String) {
    match service.find('/') {
        Some(cut) => (service[..cut].to_string(), service[cut..].to_string()),
        None => (service.to_string(), "/StatusNotifierItem".to_string()),
    }
}

fn read_icon(connection: &zbus::blocking::Connection, service: &str) -> Option<Icon> {
    let (name, path) = split(service);
    let proxy = zbus::blocking::Proxy::new(connection, name, path, ITEM).ok()?;

    let title: String = proxy
        .get_property("Title")
        .or_else(|_| proxy.get_property("Id"))
        .unwrap_or_else(|_| service.to_string());

    let image = pixmap(&proxy).or_else(|| named(&proxy))?;

    Some(Icon {
        service: service.to_string(),
        title,
        image,
    })
}

fn pixmap(proxy: &zbus::blocking::Proxy<'_>) -> Option<Image> {
    let pixmaps: Vec<(i32, i32, Vec<u8>)> = proxy.get_property("IconPixmap").ok()?;
    let (width, height, argb) = pixmaps
        .into_iter()
        .filter(|(w, h, bytes)| {
            *w > 0 && *h > 0 && bytes.len() >= (*w as usize) * (*h as usize) * 4
        })
        .min_by_key(|(w, _, _)| (*w - 24).abs())?;

    let mut rgba = Vec::with_capacity(argb.len());
    for pixel in argb.as_chunks::<4>().0 {
        rgba.extend_from_slice(&[pixel[1], pixel[2], pixel[3], pixel[0]]);
    }

    let mut image = Image {
        width: width as u32,
        height: height as u32,
        rgba,
    };
    image.premultiply();
    Some(image)
}

fn named(proxy: &zbus::blocking::Proxy<'_>) -> Option<Image> {
    let name: String = proxy.get_property("IconName").ok()?;
    if name.is_empty() {
        return None;
    }
    let theme: String = proxy.get_property("IconThemePath").unwrap_or_default();
    let path = find_icon(&name, &theme)?;
    drifter_core::image::load(&path).ok()
}

fn find_icon(name: &str, theme: &str) -> Option<std::path::PathBuf> {
    let mut roots: Vec<std::path::PathBuf> = Vec::new();
    if !theme.is_empty() {
        roots.push(std::path::PathBuf::from(theme));
    }
    for size in ["32x32", "24x24", "48x48", "scalable"] {
        roots.push(
            std::path::PathBuf::from("/usr/share/icons/hicolor")
                .join(size)
                .join("apps"),
        );
    }
    roots.push(std::path::PathBuf::from("/usr/share/pixmaps"));

    roots
        .into_iter()
        .map(|root| root.join(format!("{name}.png")))
        .find(|candidate| candidate.is_file())
}

fn serve(
    items: Arc<Mutex<Vec<String>>>,
    wake: SyncSender<Vec<Icon>>,
    clicks: Receiver<(String, Click, i32, i32)>,
) {
    let mut builder = match bus_address() {
        Some(address) => match zbus::blocking::connection::Builder::address(address.as_str()) {
            Ok(builder) => builder,
            Err(error) => {
                eprintln!("drifter: no tray: {error}");
                return;
            }
        },
        None => match zbus::blocking::connection::Builder::session() {
            Ok(builder) => builder,
            Err(error) => {
                eprintln!("drifter: no tray: {error}");
                return;
            }
        },
    };

    let watcher = Watcher {
        items: items.clone(),
        wake: wake.clone(),
        connection: None,
    };

    builder = match builder.name(WATCHER) {
        Ok(builder) => builder,
        Err(error) => {
            eprintln!("drifter: no tray: {error}");
            return;
        }
    };
    builder = match builder.serve_at(WATCHER_PATH, watcher) {
        Ok(builder) => builder,
        Err(error) => {
            eprintln!("drifter: no tray: {error}");
            return;
        }
    };

    let connection = match builder.build() {
        Ok(connection) => connection,
        Err(error) => {
            eprintln!("drifter: no tray: {error}");
            eprintln!("drifter: another tray already owns {WATCHER}");
            return;
        }
    };

    if let Ok(mut interface) = connection
        .object_server()
        .interface::<_, Watcher>(WATCHER_PATH)
    {
        interface.get_mut().connection = Some(connection.clone());
    }

    let watching = connection.clone();
    let names = items.clone();
    let ping = wake.clone();
    std::thread::Builder::new()
        .name("drifter-tray-signals".to_string())
        .spawn(move || watch_signals(watching, names, ping))
        .ok();

    while let Ok((service, click, x, y)) = clicks.recv() {
        let (name, path) = split(&service);
        let Ok(proxy) = zbus::blocking::Proxy::new(&connection, name, path, ITEM) else {
            continue;
        };
        let method = match click {
            Click::Left => "Activate",
            Click::Right => "ContextMenu",
            Click::Middle => "SecondaryActivate",
        };
        if let Err(error) = proxy.call_method(method, &(x, y)) {
            eprintln!("drifter: tray {method} failed: {error}");
        }
    }
}

fn watch_signals(
    connection: zbus::blocking::Connection,
    items: Arc<Mutex<Vec<String>>>,
    wake: SyncSender<Vec<Icon>>,
) {
    let Ok(rule) = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .build()
        .try_into()
        .map(|rule: zbus::MatchRule<'static>| rule)
    else {
        return;
    };

    let Ok(messages) = zbus::blocking::MessageIterator::for_match_rule(rule, &connection, None)
    else {
        return;
    };

    for message in messages.flatten() {
        let header = message.header();
        let Some(interface) = header.interface() else {
            continue;
        };
        let member = header.member().map(|m| m.as_str().to_string());

        let mut changed = false;
        if interface.as_str() == ITEM {
            changed = true;
        }
        if interface.as_str() == "org.freedesktop.DBus"
            && member.as_deref() == Some("NameOwnerChanged")
            && let Ok((name, _old, new)) = message.body().deserialize::<(String, String, String)>()
            && new.is_empty()
        {
            let mut list = items.lock().unwrap();
            let before = list.len();
            list.retain(|service| !service.starts_with(&name));
            changed = list.len() != before;
        }

        if !changed {
            continue;
        }

        let names = items.lock().unwrap().clone();
        let icons = names
            .iter()
            .filter_map(|service| read_icon(&connection, service))
            .collect();
        let _ = wake.send(icons);
    }
}
