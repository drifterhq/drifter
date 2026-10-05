use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use drifter_core::desktop::App;
use drifter_core::settings::{Art, Ask, Fonts, Hit, Id, Look, Model, Section, Value};
use drifter_core::sheet::{Choice, For, Pick, Sheet};
use drifter_core::shortcut::{self, Combo, Mods};
use drifter_core::stats::{self, Mount};
use drifter_core::system::{self, Sound};
use drifter_core::{Config, Font, Image, Launch, Vec2};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop::channel::{
    Event as ChannelEvent, Sender as LoopSender, channel as loop_channel,
};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers};
use smithay_client_toolkit::seat::pointer::{
    BTN_LEFT, PointerEvent, PointerEventKind, PointerHandler,
};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::XdgShell;
use smithay_client_toolkit::shell::xdg::window::{
    Window, WindowConfigure, WindowDecorations, WindowHandler,
};
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm, delegate_xdg_shell, delegate_xdg_window, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

const APP: &str = "drifter-settings";
const START: (u32, u32) = (860, 640);
const ICON: u32 = 22;
const LOADERS: usize = 3;
const WATCH: std::time::Duration = std::time::Duration::from_secs(1);

pub struct Settings {
    pub heads: crate::heads::Heads,
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    window: Window,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    mods: Modifiers,
    chores: LoopSender<Option<String>>,
    net_done: LoopSender<(bool, Result<(), String>)>,
    searching: bool,
    net_ticks: u32,
    history_seen: Option<std::time::SystemTime>,
    size: (u32, u32),
    exit: bool,
    resized: bool,
    scrolled: bool,
    shown: Option<std::time::Instant>,
    qh: QueueHandle<Settings>,

    look: Look,
    fonts: (Font, Font, Font),
    wallpaper: Option<Image>,
    path: PathBuf,
    model: Model,
    saved: Model,
    section: Section,
    scroll: f64,
    at: Vec2,
    holding: Option<Id>,
    moving: Option<Moving>,
    sheet: Option<Sheet>,
    apps: Vec<App>,
    icons: HashMap<String, Image>,
    thumbs: HashMap<PathBuf, Image>,
    asked: HashSet<PathBuf>,
    loads: Sender<PathBuf>,
}

struct Moving {
    at: usize,
    zoom: f64,
    offset: Vec2,
    grab: Vec2,
}

pub fn run() -> Result<(), String> {
    let path = Config::config_path();
    Config::first_run(&path).map_err(|error| format!("cannot make {}: {error}", path.display()))?;
    let config = Config::load_from(&path).map_err(|error| error.to_string())?;
    let find = |px| Font::find(config.font_path(), px);
    let fonts = (find(13.0)?, find(11.5)?, find(22.0)?);
    let wallpaper = config
        .wallpaper_path()
        .and_then(|path| drifter_core::image::load(&path).ok())
        .map(small);
    let apps = drifter_core::desktop::installed();
    drifter_core::icon::set_theme(config.appearance.icons.clone());
    let mut model = read_model(&config);
    model.sound = read_sound();
    model.autostarts = drifter_core::autostart::installed();
    model.brightness = read_brightness();
    model.profile = read_profile();
    model.cells = read_cells();
    (model.zone, model.ntp) = read_time();
    model.about = system::about(std::env::var("DRIFTER_RENDERER").ok().as_deref(), &path);
    read_recent(&mut model);
    model.names = apps
        .iter()
        .map(|app| (app.id.clone(), app.name.clone()))
        .collect();

    let connection = Connection::connect_to_env().map_err(|e| format!("no wayland: {e}"))?;
    let (globals, queue) =
        registry_queue_init(&connection).map_err(|e| format!("no wayland registry: {e}"))?;
    let qh = queue.handle();
    let compositor =
        CompositorState::bind(&globals, &qh).map_err(|e| format!("no wl_compositor: {e}"))?;
    let shell = XdgShell::bind(&globals, &qh).map_err(|e| format!("no xdg shell: {e}"))?;
    let shm = Shm::bind(&globals, &qh).map_err(|e| format!("no wl_shm: {e}"))?;

    let surface = compositor.create_surface(&qh);
    let window = shell.create_window(surface, WindowDecorations::RequestServer, &qh);
    window.set_title("Settings");
    window.set_app_id(APP);
    let look = themed(&model, 0.0);
    let least = look.least();
    window.set_min_size(Some((least.x as u32, least.y as u32)));
    window.commit();

    let pool = SlotPool::new((START.0 * START.1 * 4) as usize, &shm)
        .map_err(|e| format!("no shared memory: {e}"))?;

    let mut events: EventLoop<Settings> =
        EventLoop::try_new().map_err(|e| format!("no event loop: {e}"))?;
    WaylandSource::new(connection, queue)
        .insert(events.handle())
        .map_err(|e| format!("cannot watch wayland: {e}"))?;
    let (done_tx, done_rx) = loop_channel::<(PathBuf, Option<Image>)>();
    events
        .handle()
        .insert_source(done_rx, |event, _, settings: &mut Settings| {
            if let ChannelEvent::Msg((path, image)) = event {
                if let Some(image) = image {
                    settings.thumbs.insert(path, image);
                }
                if settings.section == Section::Wallpaper && settings.sheet.is_none() {
                    settings.draw();
                }
            }
        })
        .map_err(|e| format!("cannot watch the thumbnails: {e}"))?;
    let loads = loaders(done_tx);
    let (net_done, net_back) = loop_channel::<(bool, Result<(), String>)>();
    events
        .handle()
        .insert_source(net_back, |event, _, settings: &mut Settings| {
            if let ChannelEvent::Msg((search, result)) = event {
                if search {
                    settings.searching = false;
                }
                settings.model.net_note = result.err();
                settings.read_net();
                let shown = matches!(settings.section, Section::Network | Section::Bluetooth);
                if shown && settings.sheet.is_none() {
                    settings.draw();
                }
            }
        })
        .map_err(|e| format!("cannot watch the network changes: {e}"))?;
    let (chores, chores_done) = loop_channel::<Option<String>>();
    events
        .handle()
        .insert_source(chores_done, |event, _, settings: &mut Settings| {
            if let ChannelEvent::Msg(error) = event {
                settings.model.time_error = error;
                let time = read_time();
                (settings.model.zone, settings.model.ntp) = time.clone();
                (settings.saved.zone, settings.saved.ntp) = time;
                if settings.section == Section::Time && settings.sheet.is_none() {
                    settings.draw();
                }
            }
        })
        .map_err(|e| format!("cannot watch the system changes: {e}"))?;
    events
        .handle()
        .insert_source(
            smithay_client_toolkit::reexports::calloop::timer::Timer::from_duration(WATCH),
            |_, _, settings: &mut Settings| {
                settings.watch();
                smithay_client_toolkit::reexports::calloop::timer::TimeoutAction::ToDuration(WATCH)
            },
        )
        .map_err(|e| format!("cannot start the settings timer: {e}"))?;

    let mut settings = Settings {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        heads: crate::heads::Heads::bind(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        shm,
        pool,
        window,
        pointer: None,
        keyboard: None,
        mods: Modifiers::default(),
        chores,
        net_done,
        searching: false,
        net_ticks: 0,
        history_seen: None,
        size: START,
        exit: false,
        resized: false,
        scrolled: false,
        shown: None,
        qh: qh.clone(),
        look,
        fonts,
        wallpaper,
        path,
        saved: model.clone(),
        model,
        section: std::env::args()
            .nth(1)
            .and_then(|word| Section::from_word(&word))
            .unwrap_or(Section::Desktop),
        scroll: 0.0,
        at: Vec2::ZERO,
        holding: None,
        moving: None,
        sheet: None,
        apps,
        icons: HashMap::new(),
        thumbs: HashMap::new(),
        asked: HashSet::new(),
        loads,
    };
    settings.load_row_icons();

    while !settings.exit {
        events
            .dispatch(
                settings.scrolled.then_some(std::time::Duration::from_millis(100)),
                &mut settings,
            )
            .map_err(|e| format!("event loop: {e}"))?;
        if std::mem::take(&mut settings.resized) || settings.scroll_due() {
            settings.draw();
        }
    }
    Ok(())
}

fn loaders(done: LoopSender<(PathBuf, Option<Image>)>) -> Sender<PathBuf> {
    let (tx, rx) = std::sync::mpsc::channel::<PathBuf>();
    let rx: Arc<Mutex<Receiver<PathBuf>>> = Arc::new(Mutex::new(rx));
    let look = Look::default();
    for _ in 0..LOADERS {
        let rx = rx.clone();
        let done = done.clone();
        let _ = std::thread::Builder::new()
            .name("settings-thumbs".to_string())
            .spawn(move || {
                loop {
                    let next = rx.lock().ok().and_then(|held| held.recv().ok());
                    let Some(path) = next else {
                        return;
                    };
                    let image = drifter_core::image::load(&path).ok().map(|image| {
                        drifter_core::settings::cover(
                            &image,
                            look.thumb.x as u32,
                            look.thumb.y as u32,
                        )
                    });
                    if done.send((path, image)).is_err() {
                        return;
                    }
                }
            });
    }
    tx
}

fn small(image: Image) -> Image {
    let width = image.width.min(720);
    let height =
        (f64::from(image.height) * f64::from(width) / f64::from(image.width.max(1))) as u32;
    image.scaled(width, height.max(1))
}

fn read_model(config: &Config) -> Model {
    let mounts = std::fs::read_to_string("/proc/self/mountinfo")
        .map(|text| stats::mounts(&text))
        .unwrap_or_default();
    Model::read(config, &mounts, kernel)
}

fn themed(model: &Model, side_scroll: f64) -> Look {
    Look {
        accent: accent(model),
        side_scroll,
        ..Look::themed(&drifter_core::theme::of(model.appearance.style))
    }
}

fn accent(model: &Model) -> [u8; 3] {
    model
        .appearance
        .accent
        .map(|accent| accent.color())
        .unwrap_or(Look::default().accent)
}

fn output(program: &str, args: &[&str]) -> Option<String> {
    std::process::Command::new(program)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn wpctl(args: &[&str]) {
    if let Err(error) = std::process::Command::new("wpctl").args(args).status() {
        eprintln!("drifter-settings: wpctl {args:?}: {error}");
    }
}

const SINK: &str = "@DEFAULT_AUDIO_SINK@";
const SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";

fn read_sound() -> Sound {
    let Some(status) = output("wpctl", &["status"]) else {
        return Sound::default();
    };
    let (outputs, inputs) = system::parse_status(&status);
    let level = |target: &str| {
        output("wpctl", &["get-volume", target])
            .and_then(|text| drifter_core::hud::parse_wpctl(&text))
            .unwrap_or((0.0, false))
    };
    let (volume, muted) = level(SINK);
    let (mic, mic_muted) = level(SOURCE);
    Sound {
        ready: true,
        outputs,
        inputs,
        volume: volume.min(1.0),
        muted,
        mic: mic.min(1.0),
        mic_muted,
    }
}

const PROFILE_BUSES: [(&str, &str); 2] = [
    ("org.freedesktop.UPower.PowerProfiles", "/org/freedesktop/UPower/PowerProfiles"),
    ("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles"),
];

fn read_profile() -> Option<drifter_core::settings::Profiles> {
    PROFILE_BUSES.iter().find_map(|(bus, path)| {
        let now = output("busctl", &["get-property", bus, path, bus, "ActiveProfile"])?;
        let now = now.trim().strip_prefix("s ")?.trim_matches('"').to_string();
        let listed = output("busctl", &["get-property", bus, path, bus, "Profiles"]).unwrap_or_default();
        let offered = drifter_core::settings::PROFILES
            .iter()
            .map(|(name, _)| name.to_string())
            .filter(|name| listed.contains(&format!("\"{name}\"")))
            .collect();
        Some(drifter_core::settings::Profiles { now, offered })
    })
}

fn set_profile(name: &str) {
    for (bus, path) in PROFILE_BUSES {
        let ran = std::process::Command::new("busctl")
            .args(["set-property", bus, path, bus, "ActiveProfile", "s", name])
            .stdin(std::process::Stdio::null())
            .output();
        match ran {
            Ok(out) if out.status.success() => return,
            Ok(_) => continue,
            Err(error) => {
                eprintln!("drifter-settings: the power mode needs busctl: {error}");
                return;
            }
        }
    }
    eprintln!("drifter-settings: power-profiles-daemon did not take {name}");
}

fn read_cells() -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir("/sys/class/power_supply") else {
        return Vec::new();
    };
    let mut folders: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    folders.sort();
    folders
        .iter()
        .map(|folder| {
            drifter_core::stats::battery_rows(|key| std::fs::read_to_string(folder.join(key)).ok())
        })
        .find(|rows| !rows.is_empty())
        .unwrap_or_default()
}

fn read_brightness() -> Option<f64> {
    system::backlight().map(|light| light.level())
}

fn set_brightness(level: f64) {
    let Some(light) = system::backlight() else {
        return;
    };
    let raw = light.raw(level).to_string();
    let path = "/org/freedesktop/login1/session/auto";
    let args = [
        "call",
        "org.freedesktop.login1",
        path,
        "org.freedesktop.login1.Session",
        "SetBrightness",
        "ssu",
        "backlight",
        &light.name,
        &raw,
    ];
    match std::process::Command::new("busctl")
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
    {
        Ok(out) if out.status.success() => {}
        Ok(out) => eprintln!(
            "drifter-settings: cannot set the brightness: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ),
        Err(error) => eprintln!("drifter-settings: brightness needs busctl: {error}"),
    }
}

fn read_time() -> (String, Option<bool>) {
    let shown = output("timedatectl", &["show", "-p", "Timezone", "-p", "NTP"]);
    let field = |key: &str| {
        shown.as_deref().and_then(|text| {
            text.lines()
                .find_map(|line| line.strip_prefix(key)?.strip_prefix('='))
                .map(str::to_string)
        })
    };
    let zone = field("Timezone")
        .filter(|zone| !zone.is_empty())
        .or_else(|| {
            std::fs::read_link("/etc/localtime")
                .ok()
                .and_then(|link| system::zone_from_link(&link))
        })
        .unwrap_or_default();
    (zone, field("NTP").map(|ntp| ntp == "yes"))
}

fn zones() -> Vec<Choice> {
    ["zone1970.tab", "zone.tab", "tzdata.zi"]
        .iter()
        .find_map(|name| std::fs::read_to_string(Path::new("/usr/share/zoneinfo").join(name)).ok())
        .map(|table| system::zone_choices(&table))
        .unwrap_or_default()
}

fn history_path() -> PathBuf {
    Config::state_dir().join("notes")
}

fn read_recent(model: &mut Model) {
    model.recent = std::fs::read_to_string(history_path())
        .map(|text| drifter_core::notify::recall(&text))
        .unwrap_or_default();
    model.now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.as_secs())
        .unwrap_or(0);
}

fn kernel(mount: &Mount) -> Option<String> {
    let link = std::fs::read_link(format!("/sys/dev/block/{}", mount.device)).ok()?;
    Some(link.file_name()?.to_string_lossy().into_owned())
}

fn mimeapps() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home().join(".config"))
        .join("mimeapps.list")
}

fn tell_the_system(change: impl Fn(&str) -> String) {
    let path = mimeapps();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            eprintln!(
                "drifter-settings: {} is left alone because it cannot be read: {error}",
                path.display()
            );
            return;
        }
    };
    if let Err(error) = write_whole(&path, &change(&text)) {
        eprintln!("drifter-settings: {error}");
    }
}

fn write_whole(path: &Path, text: &str) -> Result<(), String> {
    let folder = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let spare = folder.join(format!(".{name}.{}.new", std::process::id()));
    std::fs::write(&spare, text).map_err(|e| format!("{}: {e}", spare.display()))?;
    std::fs::rename(&spare, path).map_err(|e| {
        let _ = std::fs::remove_file(&spare);
        format!("{}: {e}", path.display())
    })
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn icon_file(name: &str) -> Option<PathBuf> {
    match name.starts_with('/') {
        true => Some(PathBuf::from(name)),
        false => drifter_core::icon::find(name, None, 32),
    }
}

fn icon_image(path: &Path) -> Option<Image> {
    drifter_core::image::load_sized(path, ICON)
        .ok()
        .map(|image| image.scaled(ICON, ICON))
}

impl Settings {
    fn view(&self) -> Vec2 {
        Vec2::new(f64::from(self.size.0), f64::from(self.size.1))
    }

    fn icon_for(&self, key: &str) -> Option<Image> {
        let path = if let Some(id) = key.strip_prefix("app:") {
            let app = self.apps.iter().find(|app| app.id == id)?;
            icon_file(app.icon.as_deref()?)?
        } else if let Some(file) = key.strip_prefix("file:") {
            PathBuf::from(file)
        } else {
            let name = key
                .strip_prefix("cmd:")
                .or_else(|| key.strip_prefix("icon:"))?;
            icon_file(name)?
        };
        icon_image(&path)
    }

    fn want_icon(&mut self, key: String) {
        if self.icons.contains_key(&key) {
            return;
        }
        if let Some(image) = self.icon_for(&key) {
            self.icons.insert(key, image);
        }
    }

    fn load_row_icons(&mut self) {
        let chosen: Vec<Launch> = (0..3).filter_map(|at| self.model.effective(at)).collect();
        let keys: Vec<String> = self
            .model
            .login
            .iter()
            .chain(&self.model.dock)
            .chain(&chosen)
            .map(Model::icon_key)
            .chain(
                self.model
                    .groups(Section::Login)
                    .into_iter()
                    .flat_map(|group| group.rows.into_iter().filter_map(|row| row.icon)),
            )
            .collect();
        for key in keys {
            self.want_icon(key);
        }
    }

    fn want_thumbs(&mut self) {
        if self.section != Section::Wallpaper {
            return;
        }
        let shown = [self.model.livepapers.clone(), self.model.pictures.clone()].concat();
        for path in shown {
            if self.asked.insert(path.clone()) {
                let _ = self.loads.send(path);
            }
        }
    }

    fn scroll_due(&self) -> bool {
        self.scrolled
            && self
                .shown
                .is_none_or(|shown| shown.elapsed() >= std::time::Duration::from_millis(100))
    }

    fn draw(&mut self) {
        self.scrolled = false;
        let (width, height) = self.size;
        let fonts = Fonts {
            body: &self.fonts.0,
            small: &self.fonts.1,
            title: &self.fonts.2,
        };
        let art = Art {
            wallpaper: self.wallpaper.as_ref(),
            icons: &self.icons,
            thumbs: &self.thumbs,
            sheet: self.sheet.as_ref(),
        };
        let image = self.look.paint(
            self.view(),
            self.section,
            &self.model,
            self.scroll,
            &fonts,
            &art,
        );
        let stride = width as i32 * 4;
        let (buffer, canvas) = match self.pool.create_buffer(
            width as i32,
            height as i32,
            stride,
            wl_shm::Format::Argb8888,
        ) {
            Ok(made) => made,
            Err(error) => {
                eprintln!("drifter-settings: no buffer: {error}");
                return;
            }
        };
        let pixels = image.rgba.as_chunks::<4>().0;
        for (to, from) in canvas.as_chunks_mut::<4>().0.iter_mut().zip(pixels) {
            to[0] = from[2];
            to[1] = from[1];
            to[2] = from[0];
            to[3] = from[3];
        }
        let surface = self.window.wl_surface();
        surface.damage_buffer(0, 0, width as i32, height as i32);
        if let Err(error) = buffer.attach_to(surface) {
            eprintln!("drifter-settings: {error}");
            return;
        }
        surface.frame(&self.qh, surface.clone());
        self.shown = Some(std::time::Instant::now());
        self.window.commit();
    }

    fn clamp_scroll(&mut self) {
        let groups = self.model.groups(self.section);
        let most = self.look.max_scroll(self.view(), self.section, &groups);
        self.scroll = self.scroll.clamp(0.0, most);
    }

    fn press(&mut self) {
        if self.sheet.is_some() {
            let pick = self
                .sheet
                .as_ref()
                .and_then(|sheet| sheet.hit(self.view(), self.at));
            if let Some(pick) = pick {
                self.take(pick);
            }
            self.draw();
            return;
        }
        let hit = self
            .look
            .hit(self.view(), self.section, &self.model, self.scroll, self.at);
        match hit {
            Some(Hit::Search) => {}
            Some(Hit::Section(section)) => self.go_to(section),
            Some(Hit::Change(id, value)) => {
                self.model.set(id, value);
                if let Value::Number(_) = value {
                    self.holding = Some(id);
                }
                self.save();
            }
            Some(Hit::Press(id, index)) => match self.model.press(id, index) {
                None if matches!(id, Id::Default(_)) && index == 1 => {
                    if let Id::Default(at) = id {
                        let role = drifter_core::desktop::Role::ALL[at.min(2)];
                        tell_the_system(|text| drifter_core::desktop::clear_default(text, role));
                        self.model.system[at.min(2)] = drifter_core::desktop::system_default(role);
                    }
                    self.save();
                }
                Some(Ask::Pick(purpose)) => self.open_picker(purpose),
                Some(Ask::Files(purpose)) => self.open_files(purpose),
                Some(Ask::Choose(purpose)) => self.open_choices(purpose),
                Some(Ask::Capture(at)) => {
                    let (title, now) = self.model.shortcut_name(at);
                    self.sheet = Some(Sheet::capture(at, &title, &now));
                }
                Some(Ask::Reset) => self.reset_all(),
                Some(Ask::Net(ask)) => self.net_ask(ask),
                Some(Ask::Forget) => {
                    if let Err(error) = std::fs::remove_file(history_path())
                        && error.kind() != std::io::ErrorKind::NotFound
                    {
                        eprintln!("drifter-settings: cannot clear the history: {error}");
                    }
                    read_recent(&mut self.model);
                }
                None => self.save(),
            },
            Some(Hit::Open(Id::Net(at))) => {
                if let Some(net) = self.model.wifi.as_ref().and_then(|wifi| wifi.nets.get(at)) {
                    let lines = crate::net::details(net);
                    self.sheet = Some(Sheet::info(For::NetInfo(at), &net.ssid, lines));
                }
            }
            Some(Hit::Open(_)) => return,
            Some(Hit::Monitor(at)) => {
                self.model.pick_monitor(at);
                let (zoom, offset) = self.look.monitor_map(self.view(), self.scroll, &self.model);
                if let Some(monitor) = self.model.monitors.get(at) {
                    self.moving = Some(Moving {
                        at,
                        zoom,
                        offset,
                        grab: (self.at - offset) / zoom - monitor.place,
                    });
                }
            }
            Some(Hit::Picture(shelf, at)) => {
                self.model.choose_picture(shelf, at);
                self.save();
                self.reload_wallpaper();
            }
            None => return,
        }
        self.draw();
    }

    fn open_picker(&mut self, purpose: For) {
        let keys: Vec<String> = self
            .apps
            .iter()
            .map(|app| format!("app:{}", app.id))
            .collect();
        for key in keys {
            self.want_icon(key);
        }
        self.sheet = Some(Sheet::picker(purpose, self.apps.clone()));
    }

    fn watch(&mut self) {
        if self.sheet.is_some() || self.holding.is_some() {
            return;
        }
        match self.section {
            Section::Notifications => {
                let stamp = std::fs::metadata(history_path())
                    .and_then(|meta| meta.modified())
                    .ok();
                if stamp != self.history_seen {
                    self.history_seen = stamp;
                    self.refresh(Section::Notifications);
                    self.draw();
                }
            }
            Section::Network | Section::Bluetooth => {
                self.net_ticks += 1;
                if self.net_ticks % 3 == 0 {
                    let (wifi, blue) = (self.model.wifi.clone(), self.model.blue.clone());
                    self.read_net();
                    if wifi != self.model.wifi || blue != self.model.blue {
                        self.draw();
                    }
                }
            }
            Section::Power => {
                let cells = read_cells();
                let profile = read_profile();
                if cells != self.model.cells || profile != self.saved.profile {
                    self.model.cells = cells.clone();
                    self.saved.cells = cells;
                    self.model.profile = profile.clone();
                    self.saved.profile = profile;
                    self.draw();
                }
            }
            Section::Sound => {
                let sound = read_sound();
                if sound != self.model.sound {
                    self.model.sound = sound.clone();
                    self.saved.sound = sound;
                    self.draw();
                }
            }
            Section::Displays => {
                let level = read_brightness();
                let near = match (level, self.model.brightness) {
                    (Some(now), Some(shown)) => (now - shown).abs() < 0.015,
                    (now, shown) => now.is_none() && shown.is_none(),
                };
                if !near {
                    self.model.brightness = level;
                    self.saved.brightness = level;
                    self.draw();
                }
            }
            _ => {}
        }
    }

    fn refresh(&mut self, section: Section) {
        match section {
            Section::Sound => {
                self.model.sound = read_sound();
                self.saved.sound = self.model.sound.clone();
            }
            Section::Notifications => {
                read_recent(&mut self.model);
                self.saved.recent = self.model.recent.clone();
            }
            Section::Displays => {
                self.model.brightness = read_brightness();
                self.saved.brightness = self.model.brightness;
            }
            Section::Time => {
                let time = read_time();
                (self.model.zone, self.model.ntp) = time.clone();
                (self.saved.zone, self.saved.ntp) = time;
            }
            Section::Network => {
                self.model.net_note = None;
                self.read_net();
                let done = self.net_done.clone();
                let _ = std::thread::Builder::new()
                    .name("settings-rescan".to_string())
                    .spawn(move || {
                        crate::net::rescan();
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        let _ = done.send((false, Ok(())));
                    });
            }
            Section::Bluetooth => {
                self.model.net_note = None;
                self.read_net();
            }
            _ => {}
        }
    }

    pub fn read_net(&mut self) {
        let wifi = crate::net::read_wifi();
        let blue = crate::net::read_blue(self.searching);
        self.model.wifi = wifi.clone();
        self.saved.wifi = wifi;
        self.model.blue = blue.clone();
        self.saved.blue = blue;
    }

    fn net_job(&mut self, job: crate::net::Job, note: String) {
        let search = matches!(job, crate::net::Job::Search);
        if search {
            self.searching = true;
            if let Some(blue) = self.model.blue.as_mut() {
                blue.searching = true;
            }
        }
        self.model.net_note = Some(note);
        let done = self.net_done.clone();
        let spawned = std::thread::Builder::new()
            .name("settings-net".to_string())
            .spawn(move || {
                let _ = done.send((search, crate::net::run(job)));
            });
        if let Err(error) = spawned {
            self.searching = false;
            self.model.net_note = Some(format!("Cannot start: {error}"));
        }
    }

    fn net_ask(&mut self, ask: drifter_core::settings::NetAsk) {
        use crate::net::Job;
        use drifter_core::settings::NetAsk;

        let net = |at: usize| self.model.wifi.as_ref().and_then(|wifi| wifi.nets.get(at).cloned());
        let gadget = |at: usize| self.model.blue.as_ref().and_then(|blue| blue.gadgets.get(at).cloned());
        match ask {
            NetAsk::Join(at) => {
                let Some(net) = net(at) else {
                    return;
                };
                if net.secure && !net.known {
                    self.sheet = Some(Sheet::secret(For::Wifi(at), &format!("Join {}", net.ssid)));
                    return;
                }
                let note = format!("Joining {}…", net.ssid);
                self.net_job(
                    Job::Join {
                        ssid: net.ssid,
                        password: None,
                        known: net.known,
                    },
                    note,
                );
            }
            NetAsk::Leave(at) => {
                if let Some(net) = net(at) {
                    let note = format!("Leaving {}…", net.ssid);
                    self.net_job(Job::Leave(net.ssid), note);
                }
            }
            NetAsk::Forget(at) => {
                if let Some(net) = net(at) {
                    let note = format!("Forgetting {}…", net.ssid);
                    self.net_job(Job::Forget(net.ssid), note);
                }
            }
            NetAsk::Search => self.net_job(Job::Search, "Searching for ten seconds…".to_string()),
            NetAsk::Pair(at) | NetAsk::Connect(at) | NetAsk::Disconnect(at) | NetAsk::Remove(at) => {
                let Some(gadget) = gadget(at) else {
                    return;
                };
                let (job, verb) = match ask {
                    NetAsk::Pair(_) => (Job::Pair(gadget.mac), "Pairing with"),
                    NetAsk::Connect(_) => (Job::Connect(gadget.mac), "Connecting to"),
                    NetAsk::Disconnect(_) => (Job::Disconnect(gadget.mac), "Disconnecting"),
                    _ => (Job::Remove(gadget.mac), "Removing"),
                };
                self.net_job(job, format!("{verb} {}…", gadget.name));
            }
        }
    }

    fn open_choices(&mut self, purpose: For) {
        let items = match purpose {
            For::Layout => self.model.layouts.clone(),
            For::Variant => self.model.variants.clone(),
            For::IconTheme => drifter_core::icon::icon_themes(&drifter_core::icon::roots()),
            For::CursorTheme => drifter_core::icon::cursor_themes(&drifter_core::icon::roots()),
            For::Output => system::choices(&self.model.sound.outputs),
            For::Input => system::choices(&self.model.sound.inputs),
            For::Zone => zones(),
            _ => Vec::new(),
        };
        self.sheet = Some(Sheet::choices(purpose, items));
    }

    fn open_files(&mut self, purpose: For) {
        let pictures = home().join("Pictures");
        let pictures = if pictures.is_dir() { pictures } else { home() };
        let near = |path: Option<&PathBuf>| {
            path.and_then(|path| path.parent())
                .map(Path::to_path_buf)
                .filter(|folder| folder.is_dir())
        };
        let start = match purpose {
            For::Folder => self
                .model
                .folder
                .clone()
                .filter(|folder| folder.is_dir())
                .unwrap_or(pictures),
            For::ShotFolder => self
                .model
                .shots
                .folder_in(&home())
                .ancestors()
                .find(|folder| folder.is_dir())
                .map(Path::to_path_buf)
                .unwrap_or(pictures),
            For::Picture => near(self.model.wallpaper.as_ref()).unwrap_or(pictures),
            For::Pointer => near(self.model.assets[0].as_ref()).unwrap_or_else(home),
            For::Font => near(self.model.assets[1].as_ref())
                .or_else(|| Some(PathBuf::from("/usr/share/fonts")).filter(|f| f.is_dir()))
                .unwrap_or_else(home),
            For::WindowArt => near(self.model.assets[2].as_ref()).unwrap_or_else(home),
            For::DockArt => near(self.model.assets[3].as_ref()).unwrap_or_else(home),
            _ => home(),
        };
        self.sheet = Some(Sheet::files(purpose, start));
    }

    fn take(&mut self, pick: Pick) {
        let Some(purpose) = self.sheet.as_ref().map(Sheet::purpose) else {
            return;
        };
        match pick {
            Pick::Cancel => self.sheet = None,
            Pick::Tab(tab) => {
                if let Some(Sheet::Picker { tab: held, .. }) = self.sheet.as_mut() {
                    *held = tab;
                }
            }
            Pick::App(at) => {
                let id = match self.sheet.as_ref() {
                    Some(Sheet::Picker { apps, .. }) => apps.get(at).map(|app| app.id.clone()),
                    _ => None,
                };
                if let Some(id) = id {
                    self.finish_add(purpose, Launch::App(id));
                }
            }
            Pick::Command(text) if matches!(purpose, For::Wifi(_)) => {
                self.sheet = None;
                let For::Wifi(at) = purpose else {
                    return;
                };
                if let Some(net) = self.model.wifi.as_ref().and_then(|wifi| wifi.nets.get(at).cloned()) {
                    let note = format!("Joining {}…", net.ssid);
                    self.net_job(
                        crate::net::Job::Join {
                            ssid: net.ssid,
                            password: Some(text),
                            known: false,
                        },
                        note,
                    );
                }
            }
            Pick::Command(text) if !text.is_empty() => {
                self.finish_add(
                    purpose,
                    Launch::Full {
                        app: None,
                        command: Some(text),
                        icon: None,
                    },
                );
            }
            Pick::Command(_) => {}
            Pick::Into(folder) => self.sheet = Some(Sheet::files(purpose, folder)),
            Pick::Folder(folder) if purpose == For::Export => {
                self.sheet = None;
                self.export(&folder);
            }
            Pick::File(file) if purpose == For::Import => {
                self.sheet = None;
                self.import(&file);
            }
            Pick::Folder(folder) => {
                match purpose {
                    For::Folder => self.model.choose_folder(folder),
                    For::ShotFolder => self.model.choose_shot_folder(folder),
                    _ => self.model.choose_file(purpose, folder),
                }
                self.sheet = None;
                self.save();
                self.want_thumbs();
            }
            Pick::File(file) => {
                match purpose {
                    For::DockIcon(at) => {
                        self.model.set_icon(at, Some(file));
                        self.load_row_icons();
                    }
                    _ => self.model.choose_file(purpose, file),
                }
                self.sheet = None;
                self.save();
                if purpose == For::Picture {
                    self.reload_wallpaper();
                }
            }
            Pick::Choice(code) => {
                self.model.choose(purpose, code);
                self.sheet = None;
                self.save();
            }
            Pick::Default => {
                match purpose {
                    For::DockIcon(at) => {
                        self.model.set_icon(at, None);
                        self.load_row_icons();
                    }
                    For::Shortcut(at) => {
                        let _ = self.model.set_shortcut(at, None);
                    }
                    _ => {}
                }
                self.sheet = None;
                self.save();
            }
        }
    }

    fn finish_add(&mut self, purpose: For, item: Launch) {
        if let (For::Default(role), Launch::App(id)) = (purpose, &item) {
            tell_the_system(|text| drifter_core::desktop::set_default(text, role, id));
        }
        self.model.add(purpose, item);
        self.sheet = None;
        self.load_row_icons();
        self.save();
    }

    fn reload_wallpaper(&mut self) {
        self.wallpaper = self
            .model
            .wallpaper
            .as_ref()
            .and_then(|path| drifter_core::image::load(path).ok())
            .map(small);
    }

    pub fn heads_changed(&mut self) {
        self.read_monitors(None);
    }

    fn read_monitors(&mut self, gone: Option<&wl_output::WlOutput>) {
        if let Some(found) = self.heads.found() {
            let named: Vec<(String, Vec2)> = found
                .iter()
                .map(|head| (head.name.clone(), head.pixels))
                .collect();
            for model in [&mut self.model, &mut self.saved] {
                model.see_monitors(&named);
                for head in &found {
                    model.see_modes(&head.name, &head.modes, head.current);
                }
            }
            if self.section == drifter_core::settings::Section::Displays && self.sheet.is_none() {
                self.draw();
            }
            return;
        }
        let found: Vec<(String, Vec2)> = self
            .outputs
            .outputs()
            .filter(|output| Some(output) != gone)
            .filter_map(|output| {
                let info = self.outputs.info(&output)?;
                let mode = info
                    .modes
                    .iter()
                    .find(|mode| mode.current)
                    .or_else(|| info.modes.first())?;
                let (w, h) = mode.dimensions;
                Some((info.name?, Vec2::new(f64::from(w), f64::from(h))))
            })
            .collect();
        self.model.see_monitors(&found);
        self.saved.see_monitors(&found);
        if self.section == drifter_core::settings::Section::Displays && self.sheet.is_none() {
            self.draw();
        }
    }

    fn move_monitor(&mut self) {
        let Some(moving) = self.moving.as_ref() else {
            return;
        };
        let wanted = (self.at - moving.offset) / moving.zoom - moving.grab;
        let before = self.model.monitors.clone();
        self.model.move_monitor(moving.at, wanted);
        if self.model.monitors != before {
            self.draw();
        }
    }

    fn drop_monitor(&mut self) {
        if self.moving.take().is_some() {
            self.save();
            self.draw();
        }
    }

    fn drag(&mut self) {
        if self.moving.is_some() {
            self.move_monitor();
            return;
        }
        let Some(id) = self.holding else {
            return;
        };
        let value = self.look.drag(
            self.view(),
            self.section,
            &self.model,
            self.scroll,
            id,
            self.at.x,
        );
        if let Some(value) = value {
            let before = self.model.clone();
            self.model.set(id, value);
            if self.model != before {
                self.save();
                self.draw();
            }
        }
    }

    fn scroll_by(&mut self, by: f64) {
        let size = self.view();
        if let Some(sheet) = self.sheet.as_mut() {
            sheet.scroll_by(size, by);
            self.scrolled = true;
            return;
        }
        if self.at.x <= self.look.sidebar {
            let most = self.look.side_most(size.y);
            let before = self.look.side_scroll;
            self.look.side_scroll = (before + by).clamp(0.0, most);
            if self.look.side_scroll != before {
                self.scrolled = true;
            }
            return;
        }
        let before = self.scroll;
        self.scroll += by;
        self.clamp_scroll();
        if self.scroll != before {
            self.scrolled = true;
        }
    }

    fn capture(&mut self, at: usize, event: &KeyEvent) {
        let name = event
            .keysym
            .name()
            .map(shortcut::keysym_word)
            .unwrap_or_default();
        if name.is_empty() || shortcut::is_modifier(&name) {
            return;
        }
        let mods = Mods {
            ctrl: self.mods.ctrl,
            alt: self.mods.alt,
            shift: self.mods.shift,
            logo: self.mods.logo,
        };
        if event.keysym == Keysym::Escape && mods == Mods::default() {
            self.sheet = None;
            self.draw();
            return;
        }
        let combo = Combo {
            mods,
            camera: false,
            key: name,
        };
        match self.model.set_shortcut(at, Some(combo)) {
            Ok(()) => {
                self.sheet = None;
                self.save();
            }
            Err(note) => {
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.tell(&note);
                }
            }
        }
        self.draw();
    }

    fn go_to(&mut self, section: Section) {
        self.section = section;
        self.scroll = 0.0;
        self.refresh(section);
        self.want_thumbs();
    }

    fn search_key(&mut self, event: &KeyEvent) {
        let before = self.model.query.clone();
        match event.keysym {
            Keysym::Escape => self.model.query.clear(),
            Keysym::BackSpace => {
                self.model.query.pop();
            }
            Keysym::Return | Keysym::KP_Enter => {
                if let Some(first) = self.model.shown().first().copied() {
                    self.go_to(first);
                }
                self.draw();
                return;
            }
            _ => match event.utf8.as_deref() {
                Some(text) if !text.chars().any(char::is_control) && !self.mods.ctrl => {
                    self.model.query.push_str(text)
                }
                _ => return,
            },
        }
        if self.model.query == before {
            return;
        }
        self.saved.query = self.model.query.clone();
        self.look.side_scroll = 0.0;
        let shown = self.model.shown();
        if !shown.contains(&self.section)
            && let Some(first) = shown.first().copied()
        {
            self.go_to(first);
        }
        self.draw();
    }

    fn reload(&mut self) {
        let config = match Config::load_from(&self.path) {
            Ok(config) => config,
            Err(error) => {
                self.model.file_note = Some(error.to_string());
                return;
            }
        };
        let old = std::mem::replace(&mut self.model, read_model(&config));
        let model = &mut self.model;
        model.sound = old.sound;
        model.brightness = old.brightness;
        model.zone = old.zone;
        model.ntp = old.ntp;
        model.about = old.about;
        model.recent = old.recent;
        model.autostarts = old.autostarts;
        model.names = old.names;
        model.query = old.query;
        model.monitors = old.monitors;
        model.picked = old.picked;
        self.saved = self.model.clone();
        self.look = themed(&self.model, self.look.side_scroll);
        self.load_row_icons();
        self.reload_wallpaper();
    }

    fn reset_all(&mut self) {
        let defaults = Config::defaults(&self.path)
            .unwrap_or_else(|| self.path.with_file_name(drifter_core::config::DEFAULTS));
        let note = match std::fs::read_to_string(&defaults) {
            Ok(text) => match drifter_core::settings::replace_config(&self.path, &text) {
                Ok(backup) => format!("Back to the defaults. The old file is {}", backup.display()),
                Err(error) => format!("Cannot reset: {error}"),
            },
            Err(error) => format!(
                "Cannot read the defaults in {}: {error}",
                defaults.display()
            ),
        };
        self.reload();
        self.model.file_note = Some(note);
    }

    fn import(&mut self, file: &Path) {
        let note = match std::fs::read_to_string(file) {
            Err(error) => format!("Cannot read {}: {error}", file.display()),
            Ok(text) => match drifter_core::settings::check_import(&text) {
                Err(error) => format!("Not imported: {error}"),
                Ok(()) => match drifter_core::settings::replace_config(&self.path, &text) {
                    Ok(backup) => format!("Imported. The old file is {}", backup.display()),
                    Err(error) => format!("Cannot import: {error}"),
                },
            },
        };
        self.reload();
        self.model.file_note = Some(note);
    }

    fn export(&mut self, folder: &Path) {
        let target = drifter_core::settings::export_path(folder, |path| path.exists());
        self.model.file_note = Some(match std::fs::copy(&self.path, &target) {
            Ok(_) => format!("Saved to {}", target.display()),
            Err(error) => format!("Cannot save to {}: {error}", target.display()),
        });
    }

    fn key(&mut self, event: &KeyEvent) {
        if let Some(Sheet::Capture {
            purpose: For::Shortcut(at),
            ..
        }) = self.sheet
        {
            self.capture(at, event);
            return;
        }
        let Some(sheet) = self.sheet.as_mut() else {
            self.search_key(event);
            return;
        };
        match event.keysym {
            Keysym::Escape => self.sheet = None,
            Keysym::BackSpace => sheet.backspace(),
            Keysym::Return | Keysym::KP_Enter => {
                if let Some(pick) = sheet.enter() {
                    self.take(pick);
                }
            }
            _ => {
                if let Some(text) = event.utf8.as_deref() {
                    sheet.type_text(text);
                }
            }
        }
        self.draw();
    }

    fn apply_live(&mut self) {
        let before = self.saved.clone();
        let wifi_on = self.model.wifi.as_ref().map(|wifi| wifi.on);
        if wifi_on.is_some() && wifi_on != before.wifi.as_ref().map(|wifi| wifi.on) {
            let on = wifi_on.unwrap_or(false);
            let note = match on {
                true => "Turning Wi-Fi on…",
                false => "Turning Wi-Fi off…",
            };
            self.net_job(crate::net::Job::WifiPower(on), note.to_string());
        }
        let blue_on = self.model.blue.as_ref().map(|blue| blue.on);
        if blue_on.is_some() && blue_on != before.blue.as_ref().map(|blue| blue.on) {
            let on = blue_on.unwrap_or(false);
            let note = match on {
                true => "Turning Bluetooth on…",
                false => "Turning Bluetooth off…",
            };
            self.net_job(crate::net::Job::BluePower(on), note.to_string());
        }
        if let Some(now) = self.model.profile.as_ref()
            && before.profile.as_ref().is_some_and(|was| was.now != now.now)
        {
            let wanted = now.now.clone();
            let spawned = std::thread::Builder::new()
                .name("settings-profile".to_string())
                .spawn(move || set_profile(&wanted));
            if let Err(error) = spawned {
                eprintln!("drifter-settings: cannot change the power mode: {error}");
            }
        }
        let (now, was) = (&self.model.sound, &before.sound);
        let mut fresh = false;
        let picked = |devices: Option<&system::Device>| devices.map(|device| device.id);
        for (now_id, was_id) in [
            (picked(now.output()), picked(was.output())),
            (picked(now.input()), picked(was.input())),
        ] {
            if let Some(id) = now_id.filter(|id| Some(*id) != was_id) {
                wpctl(&["set-default", &id.to_string()]);
                fresh = true;
            }
        }
        let levels = [
            (SINK, now.volume, was.volume, now.muted, was.muted),
            (SOURCE, now.mic, was.mic, now.mic_muted, was.mic_muted),
        ];
        for (target, level, was_level, muted, was_muted) in levels {
            if !fresh && (level - was_level).abs() > 1e-6 {
                wpctl(&["set-volume", target, &format!("{level:.2}")]);
            }
            if !fresh && muted != was_muted {
                wpctl(&["set-mute", target, if muted { "1" } else { "0" }]);
            }
        }
        if fresh {
            self.model.sound = read_sound();
        }

        if let Some(level) = self.model.brightness
            && self.model.brightness != before.brightness
        {
            set_brightness(level);
        }

        if self.model.autostarts != before.autostarts {
            let (user, system) = (
                drifter_core::autostart::user_dir(),
                drifter_core::autostart::system_dirs(),
            );
            for (now, was) in self.model.autostarts.iter().zip(&before.autostarts) {
                if now.on != was.on
                    && let Err(error) =
                        drifter_core::autostart::set_on(&user, &system, &now.id, now.on)
                {
                    eprintln!(
                        "drifter-settings: cannot change autostart {}: {error}",
                        now.id
                    );
                }
            }
            self.model.autostarts = drifter_core::autostart::installed();
        }

        let mut asks: Vec<Vec<String>> = Vec::new();
        if self.model.zone != before.zone && !self.model.zone.is_empty() {
            asks.push(vec!["set-timezone".into(), self.model.zone.clone()]);
        }
        if let Some(on) = self.model.ntp.filter(|on| Some(*on) != before.ntp) {
            asks.push(vec!["set-ntp".into(), on.to_string()]);
        }
        if !asks.is_empty() {
            let done = self.chores.clone();
            let spawned = std::thread::Builder::new()
                .name("settings-time".to_string())
                .spawn(move || {
                    let mut failed = None;
                    for args in asks {
                        let ran = std::process::Command::new("timedatectl")
                            .args(&args)
                            .stdin(std::process::Stdio::null())
                            .output();
                        match ran {
                            Ok(out) if out.status.success() => {}
                            Ok(out) => {
                                let said = String::from_utf8_lossy(&out.stderr);
                                let said = said.trim().trim_start_matches("Failed to ");
                                failed = Some(said.to_string());
                            }
                            Err(error) => {
                                failed = Some(format!("timedatectl did not run: {error}"))
                            }
                        }
                    }
                    let _ = done.send(failed);
                });
            if let Err(error) = spawned {
                eprintln!("drifter-settings: cannot change the time settings: {error}");
            }
        }

        if self.model.appearance.icons != before.appearance.icons {
            drifter_core::icon::set_theme(self.model.appearance.icons.clone());
            self.icons.clear();
            self.load_row_icons();
        }
        self.look = themed(&self.model, self.look.side_scroll);
    }

    fn save(&mut self) {
        self.apply_live();
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                eprintln!(
                    "drifter-settings: cannot read {}: {error}",
                    self.path.display()
                );
                return;
            }
        };
        let out = match self.model.write(&self.saved, &text) {
            Ok(out) => out,
            Err(error) => {
                eprintln!("drifter-settings: {error}");
                return;
            }
        };
        if out == text {
            self.saved = self.model.clone();
            return;
        }
        match write_whole(&self.path, &out) {
            Ok(()) => self.saved = self.model.clone(),
            Err(error) => eprintln!("drifter-settings: cannot save: {error}"),
        }
    }
}

impl CompositorHandler for Settings {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        self.shown = None;
    }

    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }

    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for Settings {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.read_monitors(None);
    }

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.read_monitors(None);
    }

    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        self.read_monitors(Some(&output));
    }
}

impl WindowHandler for Settings {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &Window,
        configure: WindowConfigure,
        _: u32,
    ) {
        let least = self.look.least();
        let width = configure.new_size.0.map(|v| v.get()).unwrap_or(self.size.0);
        let height = configure.new_size.1.map(|v| v.get()).unwrap_or(self.size.1);
        self.size = (width.max(least.x as u32), height.max(least.y as u32));
        self.clamp_scroll();
        let most = self.look.side_most(f64::from(self.size.1));
        self.look.side_scroll = self.look.side_scroll.min(most);
        self.resized = true;
    }
}

impl SeatHandler for Settings {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            match self.seats.get_pointer(qh, &seat) {
                Ok(pointer) => self.pointer = Some(pointer),
                Err(error) => eprintln!("drifter-settings: no pointer: {error}"),
            }
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            match self.seats.get_keyboard(qh, &seat, None) {
                Ok(keyboard) => self.keyboard = Some(keyboard),
                Err(error) => eprintln!("drifter-settings: no keyboard: {error}"),
            }
        }
    }

    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            pointer.release();
        }
        if capability == Capability::Keyboard
            && let Some(keyboard) = self.keyboard.take()
        {
            keyboard.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Settings {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }

    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        self.key(&event);
    }

    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }

    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: u32,
    ) {
        self.mods = modifiers;
    }
}

impl PointerHandler for Settings {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if &event.surface != self.window.wl_surface() {
                continue;
            }
            self.at = Vec2::new(event.position.0, event.position.1);
            match event.kind {
                PointerEventKind::Motion { .. } => self.drag(),
                PointerEventKind::Press { button, .. } if button == BTN_LEFT => self.press(),
                PointerEventKind::Release { button, .. } if button == BTN_LEFT => {
                    self.holding = None;
                    self.drop_monitor();
                }
                PointerEventKind::Axis { vertical, .. } => self.scroll_by(vertical.absolute),
                PointerEventKind::Leave { .. } => {
                    self.holding = None;
                    self.drop_monitor();
                }
                _ => {}
            }
        }
    }
}

impl ShmHandler for Settings {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Settings {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(Settings);
delegate_output!(Settings);
delegate_shm!(Settings);
delegate_seat!(Settings);
delegate_pointer!(Settings);
delegate_keyboard!(Settings);
delegate_xdg_shell!(Settings);
delegate_xdg_window!(Settings);
delegate_registry!(Settings);
