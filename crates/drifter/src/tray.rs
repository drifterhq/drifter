use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use drifter_core::image::Image;
use smithay::reexports::calloop::channel::{Channel, SyncSender, sync_channel};

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const ITEM: &str = "org.kde.StatusNotifierItem";
const PATIENCE: std::time::Duration = std::time::Duration::from_secs(1);

#[derive(Clone)]
pub struct Icon {
    pub service: String,
    pub title: String,
    pub image: Image,
    pub symbolic: bool,
}

pub enum Update {
    Icons(Vec<Icon>),
    Menu {
        service: String,
        trail: Vec<i32>,
        items: Vec<drifter_core::menu::Item>,
        at: (f64, f64),
    },
}

pub enum Click {
    Left,
    Middle,
}

enum Job {
    Look,
    Poke(String, Click, i32, i32),
    Menu(String, Vec<i32>, f64, f64),
    Choose(String, i32),
}

pub struct Tray {
    jobs: Sender<Job>,
}

impl Tray {
    pub fn click(&self, service: &str, click: Click, x: i32, y: i32) {
        let _ = self.jobs.send(Job::Poke(service.to_string(), click, x, y));
    }

    pub fn ask_menu(&self, service: &str, trail: Vec<i32>, x: f64, y: f64) {
        let _ = self.jobs.send(Job::Menu(service.to_string(), trail, x, y));
    }

    pub fn choose(&self, service: &str, id: i32) {
        let _ = self.jobs.send(Job::Choose(service.to_string(), id));
    }
}

pub fn start() -> Result<(Tray, Channel<Update>), String> {
    let (icon_tx, icon_rx) = sync_channel::<Update>(8);
    let (job_tx, job_rx) = channel();
    let items: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let serving = items.clone();
    let asking = job_tx.clone();
    let sender = icon_tx.clone();
    std::thread::Builder::new()
        .name("drifter-tray".to_string())
        .spawn(move || serve(serving, sender, asking, job_rx))
        .map_err(|e| format!("cannot start the tray thread: {e}"))?;

    Ok((Tray { jobs: job_tx }, icon_rx))
}

pub fn bus_address() -> Result<String, String> {
    if let Some(set) = std::env::var_os("DBUS_SESSION_BUS_ADDRESS")
        && !set.is_empty()
    {
        return set
            .into_string()
            .map_err(|_| "DBUS_SESSION_BUS_ADDRESS is not readable text".to_string());
    }

    let uid = smithay::reexports::rustix::process::getuid().as_raw();
    let path = format!("/run/user/{uid}/bus");
    if std::path::Path::new(&path).exists() {
        return Ok(format!("unix:path={path}"));
    }
    Err(format!("no session bus. {path} does not exist"))
}

struct Watcher {
    items: Arc<Mutex<Vec<String>>>,
    look: Sender<Job>,
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
        let _ = self.look.send(Job::Look);
    }
}

fn split(service: &str) -> (String, String) {
    match service.find('/') {
        Some(cut) => (service[..cut].to_string(), service[cut..].to_string()),
        None => (service.to_string(), "/StatusNotifierItem".to_string()),
    }
}

fn owned_by(service: &str, name: &str) -> bool {
    split(service).0 == name
}

fn read_icon(connection: &zbus::blocking::Connection, service: &str) -> Option<Icon> {
    let (name, path) = split(service);
    let proxy = zbus::blocking::Proxy::new(connection, name, path, ITEM).ok()?;

    let (image, symbolic) = pixmap(&proxy)
        .map(|image| (image, false))
        .or_else(|| named(&proxy))
        .unwrap_or_else(|| (placeholder(), false));
    let title = tip(&proxy).unwrap_or_else(|| service.to_string());

    Some(Icon {
        service: service.to_string(),
        title,
        image,
        symbolic,
    })
}

fn placeholder() -> Image {
    let mut canvas = drifter_core::paint::Canvas::new(24, 24);
    canvas.circle(
        drifter_core::Vec2::new(12.0, 12.0),
        9.0,
        [190, 196, 210],
        1.0,
    );
    canvas.circle(drifter_core::Vec2::new(12.0, 12.0), 4.0, [60, 64, 76], 1.0);
    canvas.image
}

fn tip(proxy: &zbus::blocking::Proxy<'_>) -> Option<String> {
    if let Ok(tooltip) =
        proxy.get_property::<(String, Vec<(i32, i32, Vec<u8>)>, String, String)>("ToolTip")
    {
        let (_, _, title, body) = tooltip;
        if !title.is_empty() {
            return Some(title);
        }
        if !body.is_empty() {
            return Some(body);
        }
    }
    for key in ["Title", "Id"] {
        if let Ok(text) = proxy.get_property::<String>(key)
            && !text.is_empty()
        {
            return Some(text);
        }
    }
    None
}

type Node = (
    i32,
    HashMap<String, zbus::zvariant::OwnedValue>,
    Vec<zbus::zvariant::OwnedValue>,
);

fn read_menu(
    connection: &zbus::blocking::Connection,
    service: &str,
    parent: i32,
) -> Option<Vec<drifter_core::menu::Item>> {
    use zbus::zvariant::{OwnedObjectPath, Value};

    let (name, path) = split(service);
    let item = zbus::blocking::Proxy::new(connection, name.clone(), path, ITEM).ok()?;
    let menu_path: OwnedObjectPath = item.get_property("Menu").ok()?;

    let menu =
        zbus::blocking::Proxy::new(connection, name, menu_path, "com.canonical.dbusmenu").ok()?;
    let _ = menu.call_method("AboutToShow", &(parent,));
    if parent != 0 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|gap| gap.as_secs() as u32)
            .unwrap_or(0);
        let data = Value::I32(0);
        let _ = menu.call_method("Event", &(parent, "opened", &data, now));
    }

    let layout: (u32, Node) = menu
        .call("GetLayout", &(parent, 1i32, Vec::<String>::new()))
        .ok()?;

    let mut items = Vec::new();
    for child in layout.1.2 {
        let value = Value::from(child);
        let Value::Structure(node) = value else {
            continue;
        };
        let fields = node.into_fields();
        if fields.len() < 2 {
            continue;
        }
        let id = match &fields[0] {
            Value::I32(id) => *id,
            _ => continue,
        };
        let Value::Dict(props) = &fields[1] else {
            continue;
        };

        let text = |key: &str| -> Option<String> {
            props
                .get::<&str, Value>(&key)
                .ok()
                .flatten()
                .and_then(|value| match value {
                    Value::Str(text) => Some(text.to_string()),
                    _ => None,
                })
        };
        let flag = |key: &str| -> Option<bool> {
            props
                .get::<&str, Value>(&key)
                .ok()
                .flatten()
                .and_then(|value| match value {
                    Value::Bool(on) => Some(on),
                    _ => None,
                })
        };

        if flag("visible") == Some(false) {
            continue;
        }
        if text("type").as_deref() == Some("separator") {
            items.push(drifter_core::menu::Item::rule(id));
            continue;
        }

        let label = text("label").unwrap_or_default();
        if label.is_empty() {
            continue;
        }
        let mut entry = drifter_core::menu::Item::line(id, &label);
        entry.enabled = flag("enabled").unwrap_or(true);
        entry.children = text("children-display").as_deref() == Some("submenu");
        items.push(entry);
    }

    Some(items)
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

fn named(proxy: &zbus::blocking::Proxy<'_>) -> Option<(Image, bool)> {
    let name: String = proxy.get_property("IconName").ok()?;
    if name.is_empty() {
        return None;
    }
    let theme: String = proxy.get_property("IconThemePath").unwrap_or_default();
    let own = (!theme.is_empty()).then(|| std::path::PathBuf::from(theme));
    // The symbolic twin is tinted to match the bar, so prefer it when the theme has one.
    let path = match name.ends_with("-symbolic") {
        true => None,
        false => drifter_core::icon::find(&format!("{name}-symbolic"), own.as_deref(), 24),
    }
    .or_else(|| drifter_core::icon::find(&name, own.as_deref(), 24))?;
    let image = drifter_core::image::load_sized(&path, 24).ok()?;
    let image = match (image.width, image.height) {
        (24, 24) => image,
        _ => image.scaled(24, 24),
    };
    Some((image, drifter_core::icon::is_symbolic(&path)))
}

fn read_icons(connection: &zbus::blocking::Connection, names: &[String]) -> Vec<Icon> {
    std::thread::scope(|scope| {
        let reading: Vec<_> = names
            .iter()
            .map(|service| scope.spawn(move || read_icon(connection, service)))
            .collect();
        reading
            .into_iter()
            .filter_map(|job| job.join().ok().flatten())
            .collect()
    })
}

fn choose(connection: &zbus::blocking::Connection, service: &str, id: i32) {
    let (name, path) = split(service);
    let Ok(item) = zbus::blocking::Proxy::new(connection, name.clone(), path, ITEM) else {
        return;
    };
    let Ok(menu_path) = item.get_property::<zbus::zvariant::OwnedObjectPath>("Menu") else {
        return;
    };
    let Ok(menu) =
        zbus::blocking::Proxy::new(connection, name, menu_path, "com.canonical.dbusmenu")
    else {
        return;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|gap| gap.as_secs() as u32)
        .unwrap_or(0);
    let data = zbus::zvariant::Value::I32(0);
    if let Err(error) = menu.call_method("Event", &(id, "clicked", &data, now)) {
        log!("drifter: tray menu click failed: {error}");
    }
}

fn poke(
    connection: &zbus::blocking::Connection,
    service: &str,
    click: Click,
    x: i32,
    y: i32,
) -> bool {
    let (name, path) = split(service);
    let Ok(proxy) = zbus::blocking::Proxy::new(connection, name, path, ITEM) else {
        return false;
    };
    let method = match click {
        Click::Left => {
            if proxy.get_property::<bool>("ItemIsMenu").unwrap_or(false) {
                return false;
            }
            "Activate"
        }
        Click::Middle => "SecondaryActivate",
    };
    match proxy.call_method(method, &(x, y)) {
        Ok(_) => true,
        Err(error) => {
            log!("drifter: tray {method} failed: {error}");
            false
        }
    }
}

fn send_menu(
    connection: &zbus::blocking::Connection,
    wake: &SyncSender<Update>,
    service: String,
    trail: Vec<i32>,
    at: (f64, f64),
) {
    let parent = trail.last().copied().unwrap_or(0);
    match read_menu(connection, &service, parent) {
        Some(items) if !items.is_empty() || parent != 0 => {
            let _ = wake.send(Update::Menu {
                service,
                trail,
                items,
                at,
            });
        }
        _ => log!("drifter: {service} offers no menu"),
    }
}

fn serve(
    items: Arc<Mutex<Vec<String>>>,
    wake: SyncSender<Update>,
    look: Sender<Job>,
    jobs: Receiver<Job>,
) {
    let address = match bus_address() {
        Ok(address) => address,
        Err(error) => {
            log!("drifter: no tray: {error}");
            return;
        }
    };

    let mut builder = match zbus::blocking::connection::Builder::address(address.as_str()) {
        Ok(builder) => builder.method_timeout(PATIENCE),
        Err(error) => {
            log!("drifter: no tray: {address}: {error}");
            return;
        }
    };

    let watcher = Watcher {
        items: items.clone(),
        look: look.clone(),
    };

    builder = match builder.name(WATCHER) {
        Ok(builder) => builder,
        Err(error) => {
            log!("drifter: no tray: {error}");
            return;
        }
    };
    builder = match builder.serve_at(WATCHER_PATH, watcher) {
        Ok(builder) => builder,
        Err(error) => {
            log!("drifter: no tray: {error}");
            return;
        }
    };

    let connection = match builder.build() {
        Ok(connection) => connection,
        Err(error) => {
            log!("drifter: no tray: {error}");
            log!("drifter: {address} may already have a tray owning {WATCHER}");
            return;
        }
    };

    out!("tray        {address}");
    out!("            owns {WATCHER}");

    let watching = connection.clone();
    let names = items.clone();
    let ping = wake.clone();
    std::thread::Builder::new()
        .name("drifter-tray-signals".to_string())
        .spawn(move || watch_signals(watching, names, ping))
        .ok();

    loop {
        let Ok(job) = jobs.recv() else {
            log!("drifter: the tray stopped listening");
            return;
        };

        match job {
            Job::Look => {
                let names = items.lock().unwrap().clone();
                let icons = read_icons(&connection, &names);
                out!("drifter: tray has {} icon(s)", icons.len());
                let _ = wake.send(Update::Icons(icons));
            }
            Job::Menu(service, trail, x, y) => {
                let connection = connection.clone();
                let wake = wake.clone();
                std::thread::spawn(move || send_menu(&connection, &wake, service, trail, (x, y)));
            }
            Job::Choose(service, id) => {
                let connection = connection.clone();
                std::thread::spawn(move || choose(&connection, &service, id));
            }
            Job::Poke(service, click, x, y) => {
                let connection = connection.clone();
                let wake = wake.clone();
                std::thread::spawn(move || {
                    let left = matches!(click, Click::Left);
                    if !poke(&connection, &service, click, x, y) && left {
                        let at = (f64::from(x), f64::from(y));
                        send_menu(&connection, &wake, service, Vec::new(), at);
                    }
                });
            }
        }
    }
}

fn watch_signals(
    connection: zbus::blocking::Connection,
    items: Arc<Mutex<Vec<String>>>,
    wake: SyncSender<Update>,
) {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .build();

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
            list.retain(|service| !owned_by(service, &name));
            changed = list.len() != before;
        }

        if !changed {
            continue;
        }

        let names = items.lock().unwrap().clone();
        let icons = read_icons(&connection, &names);
        let _ = wake.send(Update::Icons(icons));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_that_leaves_takes_only_its_own_items() {
        assert!(owned_by(":1.4/StatusNotifierItem", ":1.4"));
        assert!(owned_by(":1.4", ":1.4"));
        assert!(!owned_by(":1.40/StatusNotifierItem", ":1.4"));
        assert!(!owned_by(":1.41", ":1.4"));
        assert!(!owned_by("org.kde.StatusNotifierItem-12-1", "org.kde"));
    }
}
