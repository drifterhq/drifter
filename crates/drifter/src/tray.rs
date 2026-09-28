use std::collections::HashMap;
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

pub enum Update {
    Icons(Vec<Icon>),
    Menu {
        service: String,
        items: Vec<drifter_core::menu::Item>,
        at: (f64, f64),
    },
}

pub enum Click {
    Left,
    Right,
    Middle,
}

enum Job {
    Look,
    Poke(String, Click, i32, i32),
    Menu(String, f64, f64),
    Choose(String, i32),
}

pub struct Tray {
    jobs: Sender<Job>,
}

impl Tray {
    pub fn click(&self, service: &str, click: Click, x: i32, y: i32) {
        let _ = self.jobs.send(Job::Poke(service.to_string(), click, x, y));
    }

    pub fn ask_menu(&self, service: &str, x: f64, y: f64) {
        let _ = self.jobs.send(Job::Menu(service.to_string(), x, y));
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

fn bus_address() -> Result<String, String> {
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

fn read_icon(connection: &zbus::blocking::Connection, service: &str) -> Option<Icon> {
    let (name, path) = split(service);
    let proxy = zbus::blocking::Proxy::new(connection, name, path, ITEM).ok()?;

    let image = pixmap(&proxy).or_else(|| named(&proxy))?;
    let title = tip(&proxy).unwrap_or_else(|| service.to_string());

    Some(Icon {
        service: service.to_string(),
        title,
        image,
    })
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

fn read_menu(
    connection: &zbus::blocking::Connection,
    service: &str,
) -> Option<Vec<drifter_core::menu::Item>> {
    use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

    let (name, path) = split(service);
    let item = zbus::blocking::Proxy::new(connection, name.clone(), path, ITEM).ok()?;
    let menu_path: OwnedObjectPath = item.get_property("Menu").ok()?;

    let menu =
        zbus::blocking::Proxy::new(connection, name, menu_path, "com.canonical.dbusmenu").ok()?;
    let _ = menu.call_method("AboutToShow", &(0i32));

    let layout: (u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)) = menu
        .call("GetLayout", &(0i32, 1i32, Vec::<String>::new()))
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
    wake: SyncSender<Update>,
    look: Sender<Job>,
    jobs: Receiver<Job>,
) {
    let address = match bus_address() {
        Ok(address) => address,
        Err(error) => {
            eprintln!("drifter: no tray: {error}");
            return;
        }
    };

    let mut builder = match zbus::blocking::connection::Builder::address(address.as_str()) {
        Ok(builder) => builder,
        Err(error) => {
            eprintln!("drifter: no tray: {address}: {error}");
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
            eprintln!("drifter: {address} may already have a tray owning {WATCHER}");
            return;
        }
    };

    println!("tray        {address}");
    println!("            owns {WATCHER}");

    let watching = connection.clone();
    let names = items.clone();
    let ping = wake.clone();
    std::thread::Builder::new()
        .name("drifter-tray-signals".to_string())
        .spawn(move || watch_signals(watching, names, ping))
        .ok();

    loop {
        let Ok(job) = jobs.recv() else {
            eprintln!("drifter: the tray stopped listening");
            return;
        };

        let (service, click, x, y) = match job {
            Job::Look => {
                let names = items.lock().unwrap().clone();
                let icons = names
                    .iter()
                    .filter_map(|service| read_icon(&connection, service))
                    .collect::<Vec<_>>();
                println!("drifter: tray has {} icon(s)", icons.len());
                let _ = wake.send(Update::Icons(icons));
                continue;
            }
            Job::Menu(service, x, y) => {
                match read_menu(&connection, &service) {
                    Some(items) if !items.is_empty() => {
                        let _ = wake.send(Update::Menu {
                            service,
                            items,
                            at: (x, y),
                        });
                    }
                    _ => eprintln!("drifter: {service} offers no menu"),
                }
                continue;
            }
            Job::Choose(service, id) => {
                let (name, path) = split(&service);
                let Ok(item) = zbus::blocking::Proxy::new(&connection, name.clone(), path, ITEM)
                else {
                    continue;
                };
                let Ok(menu_path) = item.get_property::<zbus::zvariant::OwnedObjectPath>("Menu")
                else {
                    continue;
                };
                let Ok(menu) = zbus::blocking::Proxy::new(
                    &connection,
                    name,
                    menu_path,
                    "com.canonical.dbusmenu",
                ) else {
                    continue;
                };
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|gap| gap.as_secs() as u32)
                    .unwrap_or(0);
                let data = zbus::zvariant::Value::I32(0);
                if let Err(error) = menu.call_method("Event", &(id, "clicked", &data, now)) {
                    eprintln!("drifter: tray menu click failed: {error}");
                }
                continue;
            }
            Job::Poke(service, click, x, y) => (service, click, x, y),
        };

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
            .collect::<Vec<_>>();
        let _ = wake.send(Update::Icons(icons));
    }
}
