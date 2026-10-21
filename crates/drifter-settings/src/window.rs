use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use drifter_core::desktop::App;
use drifter_core::settings::{Art, Ask, Fonts, Hit, Id, Look, Model, Move, Press, Section, Spot, Value};
use drifter_core::settings::UserAsk;
use drifter_core::sheet::{Choice, Field, For, Pick, Sheet};
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
use smithay_client_toolkit::subcompositor::SubcompositorState;
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
const SAVE_EVERY: std::time::Duration = std::time::Duration::from_millis(50);
const SETTLE: std::time::Duration = std::time::Duration::from_millis(16);
const QUIET: std::time::Duration = std::time::Duration::from_millis(1500);

enum Fresh {
    Sound(Sound),
    Power(Vec<(String, String)>, Option<drifter_core::settings::Profiles>),
    Time(String, Option<bool>),
    Net(Option<drifter_core::net::Wifi>),
    Users(drifter_core::users::Users),
}

enum BlueNews {
    Now(drifter_core::blue::Seen),
    Card(String, Option<drifter_core::blue::Card>),
    Note(String),
    Step(usize, drifter_core::blue::Step),
    Air(drifter_core::airdrop::Report, &'static str, Option<bool>),
    Shake(String),
    Done(Option<String>),
}

enum Hand {
    Volume(&'static str, f64),
    Mute(&'static str, bool),
    Light(f64),
}

impl Hand {
    fn replaces(&self, older: &Hand) -> bool {
        match (self, older) {
            (Hand::Volume(now, _), Hand::Volume(was, _)) => now == was,
            (Hand::Light(_), Hand::Light(_)) => true,
            _ => false,
        }
    }

    fn run(&self) {
        match self {
            Hand::Volume(target, level) => wpctl(&["set-volume", target, &format!("{level:.2}")]),
            Hand::Mute(target, muted) => wpctl(&["set-mute", target, if *muted { "1" } else { "0" }]),
            Hand::Light(level) => set_brightness(*level),
        }
    }
}

fn hands() -> Sender<Hand> {
    let (tx, rx) = std::sync::mpsc::channel::<Hand>();
    let spawned = std::thread::Builder::new()
        .name("settings-hands".to_string())
        .spawn(move || {
            while let Ok(first) = rx.recv() {
                let batch: Vec<Hand> = std::iter::once(first).chain(rx.try_iter()).collect();
                for (at, hand) in batch.iter().enumerate() {
                    if !batch[at + 1..].iter().any(|later| later.replaces(hand)) {
                        hand.run();
                    }
                }
            }
        });
    if let Err(error) = spawned {
        log!("drifter-settings: cannot start the volume and brightness thread: {error}");
    }
    tx
}

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
    blue_bus: Option<drifter_blue::Bus>,
    blue_back: LoopSender<BlueNews>,
    blue_searching: bool,
    step_pops: Vec<(usize, std::time::Instant)>,
    shake: Option<(String, std::time::Instant)>,
    started: std::time::Instant,
    fresh: LoopSender<Fresh>,
    hands: Sender<Hand>,
    fetching: bool,
    unsaved: bool,
    saved_at: Option<std::time::Instant>,
    user_done: LoopSender<(For, Result<String, String>)>,
    parked: Option<Sheet>,
    net_ticks: u32,
    history_seen: Option<std::time::SystemTime>,
    size: (u32, u32),
    exit: bool,
    resized: bool,
    stale: bool,
    shown: Option<std::time::Instant>,
    painted: Option<(u64, f64, f64)>,
    last: Option<(u64, drifter_core::Image)>,
    glide: Option<drifter_core::tween::Glide>,
    page_fade: Option<(drifter_core::Image, Option<drifter_core::tween::Fade>)>,
    flip: Option<(Id, bool, std::time::Instant)>,
    side_pages: Vec<Option<drifter_core::Image>>,
    pages: Vec<Option<drifter_core::Image>>,
    sheet_pages: (u64, Vec<Option<drifter_core::Image>>),
    under: Option<(u64, f64, drifter_core::Image)>,
    qh: QueueHandle<Settings>,
    handle: smithay_client_toolkit::reexports::calloop::LoopHandle<'static, Settings>,

    look: Look,
    fonts: std::sync::Arc<(Font, Font, Font)>,
    wallpaper: Option<Image>,
    path: PathBuf,
    model: Model,
    saved: Model,
    section: Section,
    scroll: f64,
    at: Vec2,
    holding: Option<Id>,
    spot: Option<Spot>,
    access: crate::access::Access,
    dragged: Option<(Id, Value)>,
    moving: Option<Moving>,
    sheet: Option<Sheet>,
    apps: Vec<App>,
    icons: HashMap<String, Image>,
    thumbs: HashMap<PathBuf, Image>,
    asked: HashSet<PathBuf>,
    loads: Sender<PathBuf>,
    connection: Connection,
    subcompositor: Option<SubcompositorState>,
    logo: Option<crate::logo::Logo>,
    logo_failed: bool,
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
    drifter_core::lang::use_config(config.appearance.language.as_deref());
    let find = |px| Font::find(config.font_path(), px);
    let fonts = (find(13.0)?, find(11.5)?, find(22.0)?);
    let wallpaper = config
        .wallpaper_path()
        .and_then(|path| drifter_core::image::load(&path).ok())
        .map(small);
    let apps = drifter_core::desktop::installed();
    drifter_core::icon::follow(&config.appearance);
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
    let subcompositor = SubcompositorState::bind(compositor.wl_compositor().clone(), &globals, &qh).ok();
    let held_connection = connection.clone();

    let surface = compositor.create_surface(&qh);
    let window = shell.create_window(surface, WindowDecorations::RequestServer, &qh);
    window.set_title(drifter_core::lang::t("Settings"));
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
                    settings.stale = true;
                }
            }
        })
        .map_err(|e| format!("cannot watch the thumbnails: {e}"))?;
    let loads = loaders(done_tx);
    let (net_done, net_back) = loop_channel::<(bool, Result<(), String>)>();
    events
        .handle()
        .insert_source(net_back, |event, _, settings: &mut Settings| {
            if let ChannelEvent::Msg((_, result)) = event {
                settings.model.net_note = result.err();
                settings.fetch(Section::Network);
                if settings.section == Section::Network && settings.sheet.is_none() {
                    settings.stale = true;
                }
            }
        })
        .map_err(|e| format!("cannot watch the network changes: {e}"))?;
    let (heard, heard_back) = loop_channel::<crate::access::Heard>();
    events
        .handle()
        .insert_source(heard_back, |event, _, settings: &mut Settings| {
            if let ChannelEvent::Msg(heard) = event {
                settings.heard(heard);
            }
        })
        .map_err(|e| format!("cannot watch the screen reader: {e}"))?;
    let (blue_back, blue_news) = loop_channel::<BlueNews>();
    events
        .handle()
        .insert_source(blue_news, |event, _, settings: &mut Settings| {
            if let ChannelEvent::Msg(news) = event {
                settings.blue_news(news);
            }
        })
        .map_err(|e| format!("cannot watch Bluetooth: {e}"))?;
    let (fresh, fresh_back) = loop_channel::<Fresh>();
    events
        .handle()
        .insert_source(fresh_back, |event, _, settings: &mut Settings| {
            if let ChannelEvent::Msg(fresh) = event {
                settings.take_fresh(fresh);
            }
        })
        .map_err(|e| format!("cannot watch the system readings: {e}"))?;
    let (user_done, user_back) = loop_channel::<(For, Result<String, String>)>();
    events
        .handle()
        .insert_source(user_back, |event, _, settings: &mut Settings| {
            if let ChannelEvent::Msg((purpose, result)) = event {
                settings.user_finished(purpose, result);
            }
        })
        .map_err(|e| format!("cannot watch the user changes: {e}"))?;
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
                    settings.stale = true;
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
        blue_bus: None,
        blue_back,
        blue_searching: false,
        step_pops: Vec::new(),
        shake: None,
        started: std::time::Instant::now(),
        fresh,
        hands: hands(),
        fetching: false,
        unsaved: false,
        saved_at: None,
        user_done,
        parked: None,
        net_ticks: 0,
        history_seen: None,
        size: START,
        exit: false,
        resized: false,
        stale: false,
        shown: None,
        painted: None,
        last: None,
        glide: None,
        page_fade: None,
        flip: None,
        side_pages: Vec::new(),
        pages: Vec::new(),
        sheet_pages: (0, Vec::new()),
        under: None,
        qh: qh.clone(),
        handle: events.handle(),
        look,
        fonts: std::sync::Arc::new(fonts),
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
        spot: None,
        access: crate::access::Access::start(heard),
        dragged: None,
        moving: None,
        sheet: None,
        apps,
        icons: HashMap::new(),
        thumbs: HashMap::new(),
        asked: HashSet::new(),
        loads,
        connection: held_connection,
        subcompositor,
        logo: None,
        logo_failed: false,
    };
    settings.load_row_icons();
    settings.start_blue();
    settings.refresh(settings.section);
    settings.take_ask(false);
    if let Some(role) = std::env::args().nth(2).and_then(|word| drifter_core::desktop::Role::from_key(&word)) {
        settings.open_picker(For::Default(role));
    }

    while !settings.exit {
        events
            .dispatch(
                (settings.stale || settings.unsaved || settings.moving()).then_some(SETTLE),
                &mut settings,
            )
            .map_err(|e| format!("event loop: {e}"))?;
        settings.step_glide();
        if settings.save_due() {
            settings.save();
        }
        if std::mem::take(&mut settings.resized) || settings.draw_due() {
            settings.draw();
        }
    }
    Ok(())
}

const GLIDE: std::time::Duration = std::time::Duration::from_millis(150);
const PAGE_FADE: std::time::Duration = std::time::Duration::from_millis(160);
const SWITCH_FLIP: std::time::Duration = std::time::Duration::from_millis(150);
const STEP_POP: f64 = 0.38;
const ROW_ICON: u32 = 26;
const STEP_FROM: f64 = 0.35;

static SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

fn scale() -> u32 {
    SCALE.load(std::sync::atomic::Ordering::Relaxed)
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
                            look.thumb.x as u32 * scale(),
                            look.thumb.y as u32 * scale(),
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
        ..Look::themed(&drifter_core::theme::of(drifter_core::theme::now(model.appearance.style)))
    }
}

impl Settings {
    fn follow_look(&mut self) {
        if drifter_core::icon::follow(&self.model.appearance) {
            self.icons.clear();
            self.load_row_icons();
            self.stale = true;
        }
        let again = themed(&self.model, self.look.side_scroll);
        if again != self.look {
            self.look = again;
            self.stale = true;
        }
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
        log!("drifter-settings: wpctl {args:?}: {error}");
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
                log!("drifter-settings: the power mode needs busctl: {error}");
                return;
            }
        }
    }
    log!("drifter-settings: power-profiles-daemon did not take {name}");
}

fn read_cells() -> Vec<(String, String)> {
    drifter_core::stats::read_battery_rows()
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
        Ok(out) => log!(
            "drifter-settings: cannot set the brightness: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ),
        Err(error) => log!("drifter-settings: brightness needs busctl: {error}"),
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
    drifter_core::desktop::user_mime_list()
}

fn tell_the_system(change: impl Fn(&str) -> String) {
    let path = mimeapps();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            log!(
                "drifter-settings: {} is left alone because it cannot be read: {error}",
                path.display()
            );
            return;
        }
    };
    if let Err(error) = write_whole(&path, &change(&text)) {
        log!("drifter-settings: {error}");
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

pub(crate) fn home() -> PathBuf {
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
    let side = ICON * scale();
    drifter_core::image::load_sized(path, side)
        .ok()
        .map(|image| image.scaled(side, side))
}

impl Settings {
    fn view(&self) -> Vec2 {
        Vec2::new(f64::from(self.size.0), f64::from(self.size.1))
    }

    fn icon_for(&self, key: &str) -> Option<Image> {
        if let Some(word) = key.strip_prefix("step:") {
            let side = ROW_ICON * scale();
            return Some(drifter_core::blue::step_icon(word, side, self.look.accent, self.look.off));
        }
        if let Some(rest) = key.strip_prefix("blue:") {
            let (kind, lit) = rest.split_once(':')?;
            let side = ROW_ICON * scale();
            return Some(drifter_core::blue::disc_image(drifter_core::blue::kind_from_word(kind), lit == "true", side, self.look.accent, self.look.off, self.look.ink));
        }
        if let Some((px, path)) = drifter_core::sheet::face_of(key) {
            let image = drifter_core::image::load(&path).ok()?;
            let square = drifter_core::users::square(&image, px * scale());
            return Some(drifter_core::users::round(&square));
        }
        if key.starts_with("letter:") {
            return None;
        }
        if let Some((shape, background, inner)) = drifter_core::sheet::art_of(key) {
            let side = drifter_core::sheet::FACE_CELL as u32 * scale();
            let image = drifter_core::image::load_sized(&self.icon_path(inner)?, side)
                .ok()?
                .scaled(side, side);
            return Some(match shape.checked_sub(1).and_then(|at| drifter_core::config::IconShape::ALL.get(at)) {
                Some(shape) => drifter_core::dress::dress(&image, *shape, background),
                None => image,
            });
        }
        icon_image(&self.icon_path(key)?)
    }

    fn icon_path(&self, key: &str) -> Option<PathBuf> {
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
        Some(path)
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
                [Section::Login, Section::Bluetooth]
                    .into_iter()
                    .flat_map(|section| self.model.groups(section))
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
        let shown = [self.model.shaders.clone(), self.model.pictures.clone()].concat();
        for path in shown {
            if self.asked.insert(path.clone()) {
                let _ = self.loads.send(path);
            }
        }
    }

    fn save_due(&self) -> bool {
        self.unsaved
            && (self.holding.is_none()
                || self.saved_at.is_none_or(|at| at.elapsed() >= SAVE_EVERY))
    }

    fn draw_due(&self) -> bool {
        self.stale
            && self
                .shown
                .is_none_or(|shown| shown.elapsed() >= std::time::Duration::from_millis(100))
    }

    fn sync_logo(&mut self) {
        let wanted = self.section == Section::About && self.sheet.is_none();
        if !wanted {
            self.logo = None;
            self.model.live_logo = false;
            return;
        }
        if self.logo.is_none() && !self.logo_failed {
            let Some(sub) = self.subcompositor.as_ref() else {
                self.logo_failed = true;
                return;
            };
            match crate::logo::Logo::new(&self.connection, sub, self.window.wl_surface(), &self.qh) {
                Ok(logo) => self.logo = Some(logo),
                Err(error) => {
                    log!("drifter-settings: no live logo: {error}");
                    self.logo_failed = true;
                }
            }
        }
        let rect = self.look.about_logo_rect(self.view(), self.scroll);
        let motion = drifter_core::welcome::Motion::of(self.model.access.reduce_motion, self.model.drift);
        let pace = drifter_core::welcome::logo_pace(motion, self.model.access.reduce_motion);
        let mut broken = false;
        if let Some(logo) = self.logo.as_mut() {
            logo.set_pace(pace);
            if let Err(error) = logo.place(rect.origin.x.round() as i32, rect.origin.y.round() as i32, rect.size.x.round() as i32, scale() as i32) {
                log!("drifter-settings: no live logo: {error}");
                broken = true;
            }
        }
        if broken {
            self.logo = None;
            self.logo_failed = true;
        }
        self.model.live_logo = self.logo.is_some();
    }

    fn draw_logo(&mut self) {
        let Some(logo) = self.logo.as_mut() else {
            return;
        };
        if let Err(error) = logo.draw(&self.qh) {
            log!("drifter-settings: the logo did not draw: {error}");
            self.logo = None;
            self.logo_failed = true;
            self.stale = true;
        }
    }

    fn draw(&mut self) {
        self.stale = false;
        self.sync_logo();
        let held = self.fonts.clone();
        let fonts = Fonts {
            body: &held.0,
            small: &held.1,
            title: &held.2,
        };
        let key = self.fingerprint();
        let sided = |this: &Self, scroll: f64, side: f64| {
            let art = Art {
                wallpaper: this.wallpaper.as_ref(),
                icons: &this.icons,
                thumbs: &this.thumbs,
                sheet: None,
            };
            let look = Look {
                side_scroll: side,
                ..this.look.clone()
            };
            look.paint(this.view(), this.section, &this.model, scroll, &fonts, &art)
        };
        let plain = |this: &Self, scroll: f64| sided(this, scroll, this.look.side_scroll);
        let image = match self.sheet.as_ref() {
            Some(sheet) => {
                let fresh = self.under.as_ref().is_some_and(|(held, at, _)| *held == key && *at == self.scroll);
                if !fresh {
                    let base = plain(self, self.scroll);
                    self.under = Some((key, self.scroll, base));
                }
                let lay = |this: &Self, sheet: &Sheet| {
                    let base = match this.under.as_ref() {
                        Some((_, _, base)) => base.clone(),
                        None => plain(this, this.scroll),
                    };
                    let mut canvas = drifter_core::paint::Canvas::on(base);
                    sheet.paint(&mut canvas, &this.look, &fonts, &this.icons);
                    canvas.image
                };
                match sheet.scroll_list(self.view()) {
                    Some((list, scroll)) if list.size.y > 1.0 => {
                        let sheet = sheet.clone();
                        let still = {
                            use std::hash::{Hash, Hasher};
                            let mut hash = std::collections::hash_map::DefaultHasher::new();
                            key.hash(&mut hash);
                            format!("{:?}", sheet.scrolled_to(0.0)).hash(&mut hash);
                            hash.finish()
                        };
                        if self.sheet_pages.0 != still {
                            self.sheet_pages = (still, Vec::new());
                        }
                        let page = (scroll / list.size.y).floor().max(0.0) as usize;
                        for at in [page, page + 1] {
                            if self.sheet_pages.1.len() <= at {
                                self.sheet_pages.1.resize(at + 1, None);
                            }
                            if self.sheet_pages.1[at].is_none() {
                                let image = lay(self, &sheet.scrolled_to(at as f64 * list.size.y));
                                self.sheet_pages.1[at] = Some(image);
                            }
                        }
                        let offset = scroll - page as f64 * list.size.y;
                        match (&self.sheet_pages.1[page], &self.sheet_pages.1[page + 1]) {
                            (Some(top), Some(next)) => {
                                let mut image = top.clone();
                                match splice(&mut image, top, next, self.view(), list, offset) {
                                    true => image,
                                    false => lay(self, &sheet),
                                }
                            }
                            _ => lay(self, &sheet),
                        }
                    }
                    _ => {
                        self.sheet_pages = (0, Vec::new());
                        lay(self, sheet)
                    }
                }
            }
            None => {
                self.under = None;
                self.sheet_pages = (0, Vec::new());
                let side = self.look.side_scroll;
                let (was_scroll, was_side) = self
                    .painted
                    .filter(|(held, _, _)| *held == key)
                    .map_or((f64::NAN, f64::NAN), |(_, at, side)| (at, side));
                let moved = was_scroll != self.scroll || was_side != side;
                let fading = self.page_fade.as_ref().is_some_and(|(_, fade)| fade.is_some()) || self.flip.is_some();
                let last = self.last.take().filter(|(held, _)| *held == key && (moved || fading));
                let image = match last {
                    Some((_, mut image)) => {
                        let mut fine = true;
                        if was_scroll != self.scroll {
                            fine &= self.compose_pages(&mut image, &plain);
                        }
                        if fine && was_side != side {
                            fine &= self.compose_side(&mut image, &sided);
                        }
                        match fine {
                            true => image,
                            false => plain(self, self.scroll),
                        }
                    }
                    None => {
                        if self.painted.is_none_or(|(held, _, _)| held != key) || !moved {
                            self.pages.clear();
                            self.side_pages.clear();
                        }
                        plain(self, self.scroll)
                    }
                };
                self.painted = Some((key, self.scroll, side));
                let shown = self.crossfade(&image);
                let shown = self.flip_switch(shown.as_ref().unwrap_or(&image)).or(shown);
                let shown = self.pop_steps(shown.as_ref().unwrap_or(&image)).or(shown);
                let shown = self.shake_row(shown.as_ref().unwrap_or(&image)).or(shown);
                self.show_ringed(shown.as_ref().unwrap_or(&image));
                self.last = Some((key, image));
                return;
            }
        };
        self.painted = Some((key, self.scroll, self.look.side_scroll));
        self.show(&image);
    }

    fn pop_steps(&mut self, image: &drifter_core::Image) -> Option<drifter_core::Image> {
        if (self.step_pops.is_empty() && !self.step_turning()) || self.section != Section::Bluetooth {
            return None;
        }
        let side = ROW_ICON * scale();
        let done = drifter_core::blue::step_icon("done", side, self.look.accent, self.look.off);
        let mut canvas = drifter_core::paint::Canvas::on(image.clone());
        let mut settled = Vec::new();
        for (at, since) in self.step_pops.clone() {
            let t = match self.animates() {
                true => since.elapsed().as_secs_f64() / STEP_POP,
                false => 1.0,
            };
            if t >= 1.0 {
                settled.push(at);
                continue;
            }
            let spot = drifter_core::settings::Spot::Row(Id::BlueStep(at), 0);
            let Some(row) = self.look.spot_rect(self.view(), self.section, &self.model, self.scroll, spot) else {
                settled.push(at);
                continue;
            };
            let cell = drifter_core::Rect::new(Vec2::new(row.origin.x + 14.0, row.center().y - 13.0), Vec2::splat(26.0));
            let grow = STEP_FROM + (1.0 - STEP_FROM) * drifter_core::drawer::ease(t);
            let size = (f64::from(side) * grow).round().max(1.0) as u32;
            let scaled = done.scaled(size, size);
            let alpha = drifter_core::motion::smooth(t * 1.6);
            canvas.stamp(&scaled, cell.center() - drifter_core::paint::logical(&scaled) / 2.0, alpha);
        }
        let turn = match self.animates() {
            true => self.started.elapsed().as_secs_f64() / 0.9,
            false => 0.0,
        };
        for (at, (_, step)) in self.model.blue_steps.iter().enumerate() {
            if *step != drifter_core::blue::Step::Doing {
                continue;
            }
            let spot = drifter_core::settings::Spot::Row(Id::BlueStep(at), 0);
            if let Some(row) = self.look.spot_rect(self.view(), self.section, &self.model, self.scroll, spot) {
                let middle = Vec2::new(row.origin.x + 27.0, row.center().y);
                canvas.arc_from(middle, 26.0 * 0.345, 26.0 * 0.08, turn, 0.3, self.look.accent, 1.0);
            }
        }
        if !settled.is_empty() {
            self.step_pops.retain(|(at, _)| !settled.contains(at));
            for at in settled {
                if let Some(step) = self.model.blue_steps.get_mut(at) {
                    step.1 = drifter_core::blue::Step::Done;
                }
            }
            self.want_icon("step:done".into());
        }
        self.stale = true;
        Some(canvas.image)
    }

    fn flip_switch(&mut self, image: &drifter_core::Image) -> Option<drifter_core::Image> {
        let (id, on, since) = self.flip?;
        let t = since.elapsed().as_secs_f64() / SWITCH_FLIP.as_secs_f64();
        if t >= 1.0 {
            self.flip = None;
            return None;
        }
        let spot = drifter_core::settings::Spot::Row(id, 0);
        let Some(pill) = self.look.spot_rect(self.view(), self.section, &self.model, self.scroll, spot) else {
            self.flip = None;
            return None;
        };
        let eased = drifter_core::tween::ease(t);
        let mut canvas = drifter_core::paint::Canvas::on(image.clone());
        self.look.paint_switch(&mut canvas, pill, if on { eased } else { 1.0 - eased }, 1.0);
        self.stale = true;
        Some(canvas.image)
    }

    fn crossfade(&mut self, image: &drifter_core::Image) -> Option<drifter_core::Image> {
        let (old, fade) = self.page_fade.take()?;
        let view = self.view();
        let area = drifter_core::Rect::new(
            Vec2::new(self.look.sidebar, 0.0),
            Vec2::new((view.x - self.look.sidebar).max(0.0), view.y),
        );
        let dense = f64::from(scale());
        let fade = match fade {
            Some(fade) => fade,
            None => {
                let (at, before) = drifter_core::tween::crop(&old, area, dense)?;
                let (_, after) = drifter_core::tween::crop(image, area, dense)?;
                drifter_core::tween::Fade::new(area, at, before, after, PAGE_FADE)
            }
        };
        let left = fade.old_alpha(std::time::Instant::now());
        if left <= 0.0 {
            return None;
        }
        let mut canvas = drifter_core::paint::Canvas::on(image.clone());
        let at = Vec2::new(f64::from(fade.at.0), f64::from(fade.at.1)) / dense;
        canvas.stamp(&fade.old, at, left);
        self.page_fade = Some((old, Some(fade)));
        self.stale = true;
        Some(canvas.image)
    }

    fn show_ringed(&mut self, image: &drifter_core::Image) {
        let ring = self
            .spot
            .and_then(|spot| self.look.spot_rect(self.view(), self.section, &self.model, self.scroll, spot));
        let Some(ring) = ring else {
            self.show(image);
            return;
        };
        let mut canvas = drifter_core::paint::Canvas::on(image.clone());
        let outer = drifter_core::Rect::new(ring.origin - Vec2::splat(3.0), ring.size + Vec2::splat(6.0));
        canvas.ring(outer, 9.0, 2.5, self.look.accent, 0.95);
        self.show(&canvas.image);
    }

    fn focus_key(&mut self, event: &KeyEvent) -> bool {
        let press = match event.keysym {
            Keysym::Tab if self.mods.shift => Press::Back,
            Keysym::ISO_Left_Tab => Press::Back,
            Keysym::Tab => Press::Next,
            _ if self.spot.is_none() => return false,
            Keysym::Up => Press::Up,
            Keysym::Down => Press::Down,
            Keysym::Left => Press::Left,
            Keysym::Right => Press::Right,
            Keysym::space | Keysym::Return | Keysym::KP_Enter => Press::Go,
            Keysym::Escape => {
                self.spot = None;
                self.stale = true;
                return true;
            }
            _ => return false,
        };
        let spot = self.spot.or(match press {
            Press::Next | Press::Back => None,
            _ => Some(Spot::Sidebar),
        });
        match self.model.key_move(self.section, spot, press) {
            Move::Stay => {
                self.spot = spot.or(Some(Spot::Sidebar));
            }
            Move::Focus(next) => {
                self.spot = Some(next);
                self.reveal(next);
            }
            Move::Section(section) => {
                self.go_to(section);
                self.spot = Some(Spot::Sidebar);
                self.reveal(Spot::Sidebar);
            }
            Move::Act(hit) => {
                self.spot = spot;
                self.act(Some(hit));
            }
        }
        self.stale = true;
        true
    }

    fn reveal(&mut self, spot: Spot) {
        let view = self.view();
        let Some(rect) = self.look.spot_rect(view, self.section, &self.model, self.scroll, spot) else {
            return;
        };
        match spot {
            Spot::Sidebar => {
                let low = self.look.search_rect().max().y + 8.0;
                if rect.origin.y < low {
                    self.look.side_scroll = (self.look.side_scroll - (low - rect.origin.y)).max(0.0);
                } else if rect.max().y > view.y - 8.0 {
                    self.look.side_scroll += rect.max().y - (view.y - 8.0);
                }
            }
            Spot::Row(..) => {
                if rect.origin.y < 64.0 {
                    self.scroll = (self.scroll - (64.0 - rect.origin.y)).max(0.0);
                } else if rect.max().y > view.y - 16.0 {
                    self.scroll += rect.max().y - (view.y - 16.0);
                }
                self.clamp_scroll();
            }
        }
    }

    fn speak(&mut self) {
        let view = crate::access::View {
            sheet: self.sheet.as_ref(),
            model: &self.model,
            look: &self.look,
            section: self.section,
            spot: self.spot,
            scroll: self.scroll,
            size: self.view(),
        };
        self.access.update(&view);
    }

    fn heard(&mut self, heard: crate::access::Heard) {
        use crate::access::{Heard, Order};
        let request = match heard {
            Heard::Wake => {
                self.speak();
                return;
            }
            Heard::Act(request) => request,
        };
        let Some(order) = self.access.aim(&request) else {
            return;
        };
        match order {
            Order::Pick(pick) => {
                if self.sheet.is_some() {
                    self.take(pick);
                }
            }
            Order::Section(section) => {
                if section != self.section {
                    self.go_to(section);
                }
                self.spot = Some(Spot::Sidebar);
            }
            Order::Focus(spot) => {
                self.spot = Some(spot);
                self.reveal(spot);
            }
            Order::Act(spot, hit) => {
                self.spot = Some(spot);
                self.act(Some(hit));
            }
            Order::Nudge(spot, id, step) => {
                self.spot = Some(spot);
                if let Some(value) = self.model.nudge(self.section, id, step) {
                    self.act(Some(Hit::Change(id, value)));
                }
            }
        }
        self.stale = true;
    }

    fn show(&mut self, image: &drifter_core::Image) {
        let (width, height) = (image.width, image.height);
        let stride = width as i32 * 4;
        let (buffer, canvas) = match self.pool.create_buffer(
            width as i32,
            height as i32,
            stride,
            wl_shm::Format::Argb8888,
        ) {
            Ok(made) => made,
            Err(error) => {
                log!("drifter-settings: no buffer: {error}");
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
        surface.set_buffer_scale(scale() as i32);
        surface.damage_buffer(0, 0, width as i32, height as i32);
        if let Err(error) = buffer.attach_to(surface) {
            log!("drifter-settings: {error}");
            return;
        }
        surface.frame(&self.qh, surface.clone());
        self.shown = Some(std::time::Instant::now());
        self.window.commit();
        self.draw_logo();
        self.speak();
    }

    fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        format!(
            "{:?}|{:?}|{:?}|{:?}|{}|{}|{}|{}",
            self.section,
            self.model.groups(self.section),
            Look {
                side_scroll: 0.0,
                ..self.look.clone()
            },
            self.size,
            self.model.query,
            self.icons.len(),
            self.thumbs.len(),
            self.wallpaper.is_some(),
        )
        .hash(&mut hash);
        self.model.live_logo.hash(&mut hash);
        hash.finish()
    }

    fn compose_side(
        &mut self,
        base: &mut drifter_core::Image,
        sided: &dyn Fn(&Self, f64, f64) -> drifter_core::Image,
    ) -> bool {
        let view = self.view();
        let head = self.look.search_rect().max().y + 8.0;
        let tall = view.y - head;
        if tall <= 0.0 {
            return false;
        }
        let side = self.look.side_scroll;
        let page = (side / tall).floor().max(0.0) as usize;
        for at in [page, page + 1] {
            if self.side_pages.len() <= at {
                self.side_pages.resize(at + 1, None);
            }
            if self.side_pages[at].is_none() {
                let image = sided(self, self.scroll, at as f64 * tall);
                self.side_pages[at] = Some(image);
            }
        }
        let (Some(top), Some(next)) = (&self.side_pages[page], &self.side_pages[page + 1]) else {
            return false;
        };
        let region = drifter_core::Rect::new(
            drifter_core::Vec2::new(0.0, head),
            drifter_core::Vec2::new(self.look.sidebar.min(view.x), tall),
        );
        splice(base, top, next, view, region, side - page as f64 * tall)
    }

    fn compose_pages(
        &mut self,
        base: &mut drifter_core::Image,
        plain: &dyn Fn(&Self, f64) -> drifter_core::Image,
    ) -> bool {
        let tall = self.view().y;
        if tall <= 0.0 {
            return false;
        }
        let page = (self.scroll / tall).floor().max(0.0) as usize;
        for at in [page, page + 1] {
            if self.pages.len() <= at {
                self.pages.resize(at + 1, None);
            }
            if self.pages[at].is_none() {
                let image = plain(self, at as f64 * tall);
                self.pages[at] = Some(image);
            }
        }
        let (Some(top), Some(next)) = (&self.pages[page], &self.pages[page + 1]) else {
            return false;
        };
        let view = self.view();
        let region = drifter_core::Rect::new(
            drifter_core::Vec2::new(self.look.sidebar, 0.0),
            drifter_core::Vec2::new((view.x - self.look.sidebar).max(0.0), view.y),
        );
        splice(base, top, next, view, region, self.scroll - page as f64 * tall)
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
            self.stale = true;
            return;
        }
        let hit = self
            .look
            .hit(self.view(), self.section, &self.model, self.scroll, self.at);
        if self.spot.take().is_some() {
            self.stale = true;
        }
        self.act(hit);
    }

    fn act(&mut self, hit: Option<Hit>) {
        self.model.focus = match hit {
            Some(Hit::Change(id, Value::Number(_))) => Some(id),
            _ => None,
        };
        match hit {
            Some(Hit::Search) => {}
            Some(Hit::Section(section)) => self.go_to(section),
            Some(Hit::Change(id, value)) => {
                if let Value::Switch(on) = value
                    && self.animates()
                {
                    self.flip = Some((id, on, std::time::Instant::now()));
                }
                self.model.set(id, value);
                if let Value::Number(_) = value {
                    self.holding = Some(id);
                    self.dragged = None;
                }
                self.save();
            }
            Some(Hit::Press(id, index)) => match self.model.press(id, index) {
                None if matches!(id, Id::Default(_)) && index == 1 => {
                    if let Id::Default(at) = id {
                        let role = drifter_core::desktop::Role::ALL[at.min(drifter_core::desktop::Role::ALL.len() - 1)];
                        tell_the_system(|text| drifter_core::desktop::clear_default(text, role));
                        self.model.system[at.min(drifter_core::desktop::Role::ALL.len() - 1)] = drifter_core::desktop::system_default(role);
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
                Some(Ask::User(ask)) => self.user_ask(ask),
                Some(Ask::EditDock(at)) => self.edit_dock(at),
                Some(Ask::RenamePlace(at)) => {
                    let name = self.model.places.get(at).map(|place| place.name.clone()).unwrap_or_default();
                    self.sheet = Some(Sheet::form(
                        For::PlaceName(at),
                        drifter_core::lang::t("Rename this place"),
                        vec![Field::plain(drifter_core::lang::t("Name"), &name)],
                        drifter_core::lang::t("Save"),
                    ));
                }
                Some(Ask::Forget) => {
                    if let Err(error) = std::fs::remove_file(history_path())
                        && error.kind() != std::io::ErrorKind::NotFound
                    {
                        log!("drifter-settings: cannot clear the history: {error}");
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
            Some(Hit::Open(Id::Gadget(at))) => self.blue_ask(drifter_core::settings::NetAsk::Options(at)),
            Some(Hit::Open(Id::BlueName)) => self.blue_ask(drifter_core::settings::NetAsk::Rename),
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
        self.stale = true;
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
        self.blue_search();
        self.take_ask(true);
        self.follow_look();
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
                    self.stale = true;
                }
            }
            Section::AirDrop => {
                self.net_ticks += 1;
                if self.net_ticks % 3 == 0 {
                    self.air_fetch();
                }
            }
            Section::Network => {
                self.net_ticks += 1;
                if self.net_ticks % 3 == 0 && !self.fetching {
                    self.fetch(Section::Network);
                }
            }
            Section::Power | Section::Sound if !self.fetching => self.fetch(self.section),
            Section::Displays => {
                let level = read_brightness();
                let near = match (level, self.model.brightness) {
                    (Some(now), Some(shown)) => (now - shown).abs() < 0.015,
                    (now, shown) => now.is_none() && shown.is_none(),
                };
                if !near && !self.lately_saved() {
                    self.model.brightness = level;
                    self.saved.brightness = level;
                    self.stale = true;
                }
            }
            _ => {}
        }
    }

    fn fetch(&mut self, section: Section) {
        let job: Box<dyn FnOnce() -> Fresh + Send> = match section {
            Section::Sound => Box::new(|| Fresh::Sound(read_sound())),
            Section::Power => Box::new(|| Fresh::Power(read_cells(), read_profile())),
            Section::Time => Box::new(|| {
                let (zone, ntp) = read_time();
                Fresh::Time(zone, ntp)
            }),
            Section::Network => Box::new(move || Fresh::Net(crate::net::read_wifi())),
            Section::Bluetooth => return,
            Section::Users => Box::new(|| Fresh::Users(crate::users::read())),
            _ => return,
        };
        let back = self.fresh.clone();
        let spawned = std::thread::Builder::new()
            .name("settings-read".to_string())
            .spawn(move || {
                let _ = back.send(job());
            });
        match spawned {
            Ok(_) => self.fetching = true,
            Err(error) => log!("drifter-settings: cannot read the system: {error}"),
        }
    }

    fn lately_saved(&self) -> bool {
        self.unsaved || self.saved_at.is_some_and(|at| at.elapsed() < QUIET)
    }

    fn take_fresh(&mut self, fresh: Fresh) {
        self.fetching = false;
        if self.holding.is_some() {
            return;
        }
        let shown = match fresh {
            Fresh::Sound(sound) => {
                if sound == self.model.sound || self.lately_saved() {
                    return;
                }
                self.model.sound = sound.clone();
                self.saved.sound = sound;
                Section::Sound
            }
            Fresh::Power(cells, profile) => {
                if cells == self.model.cells && profile == self.saved.profile {
                    return;
                }
                self.model.cells = cells.clone();
                self.saved.cells = cells;
                self.model.profile = profile.clone();
                self.saved.profile = profile;
                Section::Power
            }
            Fresh::Time(zone, ntp) => {
                (self.model.zone, self.model.ntp) = (zone.clone(), ntp);
                (self.saved.zone, self.saved.ntp) = (zone, ntp);
                Section::Time
            }
            Fresh::Net(wifi) => {
                if wifi == self.model.wifi {
                    return;
                }
                self.model.wifi = wifi.clone();
                self.saved.wifi = wifi;
                Section::Network
            }
            Fresh::Users(users) => {
                self.take_users(users);
                Section::Users
            }
        };
        if self.section == shown && self.sheet.is_none() {
            self.stale = true;
        }
    }

    fn refresh(&mut self, section: Section) {
        match section {
            Section::Sound | Section::Power | Section::Time => self.fetch(section),
            Section::Notifications => {
                read_recent(&mut self.model);
                self.saved.recent = self.model.recent.clone();
            }
            Section::Displays => {
                self.model.brightness = read_brightness();
                self.saved.brightness = self.model.brightness;
            }
            Section::Network => {
                self.model.net_note = None;
                self.fetch(section);
                let done = self.net_done.clone();
                let _ = std::thread::Builder::new()
                    .name("settings-rescan".to_string())
                    .spawn(move || {
                        crate::net::rescan();
                        std::thread::sleep(std::time::Duration::from_secs(3));
                        let _ = done.send((false, Ok(())));
                    });
            }
            Section::AirDrop => {
                self.model.net_note = None;
                self.air_fetch();
            }
            Section::Bluetooth => {
                self.model.net_note = None;
                self.model.blue_address = false;
                self.model.blue_naming = None;
                if self.step_pops.is_empty() && !self.model.blue_steps.iter().any(|(_, step)| *step == drifter_core::blue::Step::Doing) {
                    self.model.blue_steps.clear();
                }
                self.blue_search();
            }
            Section::Users => {
                self.model.user_note = None;
                self.fetch(section);
            }
            _ => {}
        }
    }

    fn take_users(&mut self, users: drifter_core::users::Users) {
        self.model.users = Some(users.clone());
        self.saved.users = Some(users);
        let stock = self.model.faces.clone();
        self.icons.retain(|key, _| match drifter_core::sheet::face_of(key) {
            Some((_, path)) => stock.contains(&path),
            None => true,
        });
        for key in self.model.face_keys() {
            self.want_icon(key);
        }
    }

    fn want_faces(&mut self, wanted: &[PathBuf]) {
        for path in wanted {
            self.want_icon(drifter_core::sheet::face_key(path, drifter_core::sheet::FACE_CELL as u32));
        }
    }

    fn open_faces(&mut self, purpose: For, chosen: Option<PathBuf>) {
        let stock = self.model.faces.clone();
        self.want_faces(&stock);
        self.sheet = Some(Sheet::faces(purpose, stock, chosen));
    }

    fn user_ask(&mut self, ask: UserAsk) {
        match ask {
            UserAsk::Rename => {
                let full = self.model.me().map(|me| me.full.clone()).unwrap_or_default();
                self.sheet = Some(Sheet::form(
                    For::MyName,
                    drifter_core::lang::t("Change your full name"),
                    vec![Field::plain(drifter_core::lang::t("Full name"), &full)],
                    drifter_core::lang::t("Save"),
                ));
            }
            UserAsk::Picture => self.open_faces(For::MyFace, None),
            UserAsk::Password => {
                self.sheet = Some(Sheet::form(
                    For::MyPassword,
                    drifter_core::lang::t("Change your password"),
                    vec![
                        Field::secret(drifter_core::lang::t("Current password")),
                        Field::secret(drifter_core::lang::t("New password")),
                        Field::secret(drifter_core::lang::t("Repeat the new password")),
                    ],
                    drifter_core::lang::t("Change Password"),
                ));
            }
            UserAsk::Add => {
                let seed = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|since| since.subsec_nanos() as u64)
                    .unwrap_or(0);
                let face = drifter_core::users::pick_stock(&self.model.faces, seed);
                if let Some(face) = &face {
                    self.want_faces(std::slice::from_ref(face));
                }
                self.sheet = Some(
                    Sheet::form(
                        For::AddUser,
                        drifter_core::lang::t("Add a user"),
                        vec![
                            Field::plain(drifter_core::lang::t("Full name"), ""),
                            Field::following(drifter_core::lang::t("Account name")),
                            Field::secret(drifter_core::lang::t("Password")),
                            Field::secret(drifter_core::lang::t("Repeat the password")),
                        ],
                        drifter_core::lang::t("Add User"),
                    )
                    .with_switch(drifter_core::lang::t("Allow this user to administer the computer"), false)
                    .with_face(face),
                );
            }
            UserAsk::Admin(at, admin) => {
                let Some(user) = self.model.other(at).cloned() else {
                    return;
                };
                self.model.user_note = Some(match admin {
                    true => drifter_core::lang::tf("Making {} an admin…", &[&(user.shown())]),
                    false => drifter_core::lang::tf("Making {} a standard user…", &[&(user.shown())]),
                });
                self.user_job(
                    For::MakeAdmin(at),
                    crate::users::Job::Admin {
                        login: user.login,
                        admin,
                    },
                );
            }
            UserAsk::Remove(at) => {
                let Some(user) = self.model.other(at).cloned() else {
                    return;
                };
                self.sheet = Some(
                    Sheet::form(For::RemoveUser(at), &drifter_core::lang::tf("Remove {}?", &[&(user.shown())]), Vec::new(), drifter_core::lang::t("Remove"))
                        .with_switch(drifter_core::lang::t("Delete the home folder too"), false)
                        .with_note(&drifter_core::lang::tf("If you keep it, {} stays on this computer.", &[&(user.home.display())])),
                );
            }
        }
    }

    fn edit_dock(&mut self, at: usize) {
        let Some(item) = self.model.dock.get(at).cloned() else {
            return;
        };
        let target = item.command().or(item.app()).unwrap_or_default().to_string();
        let plain = drifter_core::Launch::dressed(
            item.app().map(str::to_string),
            item.command().map(str::to_string),
            None,
            None,
            None,
            None,
        );
        let shape = item
            .shape()
            .and_then(|shape| drifter_core::config::IconShape::ALL.iter().position(|each| *each == shape))
            .map_or(0, |at| at + 1);
        self.sheet = Some(
            Sheet::form(
                For::DockItem(at),
                drifter_core::lang::t("Dock item"),
                vec![
                    Field::plain(drifter_core::lang::t("Name"), &self.model.name_of(&item)),
                    Field::plain(drifter_core::lang::t("Target: an app id or a command"), &target),
                ],
                drifter_core::lang::t("Save"),
            )
            .with_art(item.icon().map(Path::to_path_buf), &Model::icon_key(&plain), shape)
            .with_switch(drifter_core::lang::t("Fill clear parts with a color"), item.background()),
        );
        self.want_art();
    }

    fn want_art(&mut self) {
        if let Some(key) = self.sheet.as_ref().and_then(Sheet::art_key) {
            self.want_icon(key);
        }
    }

    fn take_ask(&mut self, running: bool) {
        let Some(at) = drifter_core::settings::take_dock_ask() else {
            return;
        };
        if running {
            if self.unsaved {
                self.save();
            }
            self.reload();
        }
        self.parked = None;
        self.go_to(Section::Dock);
        self.edit_dock(at);
        self.stale = true;
    }

    fn user_job(&mut self, purpose: For, job: crate::users::Job) {
        let done = self.user_done.clone();
        let spawned = std::thread::Builder::new()
            .name("settings-users".to_string())
            .spawn(move || {
                let _ = done.send((purpose, crate::users::run(job)));
            });
        if let Err(error) = spawned {
            self.user_finished(purpose, Err(drifter_core::lang::tf("Cannot start: {}", &[&(error)])));
        }
    }

    fn user_finished(&mut self, purpose: For, result: Result<String, String>) {
        let open = self
            .sheet
            .as_ref()
            .is_some_and(|sheet| sheet.purpose() == purpose && matches!(sheet, Sheet::Form { .. }));
        match (open, result) {
            (true, Err(error)) => {
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.work(Some(&error), false);
                }
            }
            (true, Ok(done)) => {
                self.sheet = None;
                self.model.user_note = Some(done);
            }
            (false, result) => {
                self.model.user_note = Some(result.unwrap_or_else(|error| error));
            }
        }
        self.fetch(Section::Users);
        if self.section == Section::Users {
            self.stale = true;
        }
    }

    fn submit_form(&mut self, purpose: For, texts: Vec<String>, on: bool) {
        use drifter_core::users;
        use crate::users::Job;

        let text = |at: usize| texts.get(at).cloned().unwrap_or_default();
        if let For::DockItem(at) = purpose {
            let target = text(1).trim().to_string();
            let was_app = self.model.dock.get(at).and_then(|item| item.app()) == Some(target.as_str());
            let shape = self.sheet.as_ref().and_then(Sheet::shape).unwrap_or(0);
            let edit = drifter_core::settings::DockEdit {
                name: text(0),
                app: was_app || self.apps.iter().any(|app| app.id == target),
                target,
                icon: self.sheet.as_ref().and_then(Sheet::face),
                shape: shape.checked_sub(1).and_then(|at| drifter_core::config::IconShape::ALL.get(at).copied()),
                background: on,
            };
            match self.model.edit_dock(at, edit) {
                Ok(()) => {
                    self.sheet = None;
                    self.load_row_icons();
                    self.save();
                    self.stale = true;
                }
                Err(error) => {
                    if let Some(sheet) = self.sheet.as_mut() {
                        sheet.work(Some(&error), false);
                    }
                }
            }
            return;
        }
        if let For::WifiHidden = purpose {
            let ssid = text(0).trim().to_string();
            if ssid.is_empty() {
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.work(Some(drifter_core::lang::t("Type the network name.")), false);
                }
                return;
            }
            self.sheet = None;
            let password = Some(text(1)).filter(|password| !password.is_empty());
            let note = drifter_core::lang::tf("Joining {}…", &[&(ssid)]);
            self.net_job(crate::net::Job::JoinHidden { ssid, password }, note);
            return;
        }
        if let For::WifiEnterprise(at) = purpose {
            let Some(net) = self.model.wifi.as_ref().and_then(|wifi| wifi.nets.get(at).cloned()) else {
                self.sheet = None;
                return;
            };
            let (user, password) = (text(0).trim().to_string(), text(1));
            if user.is_empty() || password.is_empty() {
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.work(Some(drifter_core::lang::t("Type your username and password.")), false);
                }
                return;
            }
            self.sheet = None;
            let domain = Some(text(2).trim().to_string()).filter(|domain| !domain.is_empty());
            let note = drifter_core::lang::tf("Joining {}…", &[&(net.ssid)]);
            self.net_job(
                crate::net::Job::JoinEnterprise {
                    ssid: net.ssid,
                    user,
                    password,
                    domain,
                },
                note,
            );
            return;
        }
        if let For::PlaceName(at) = purpose {
            match self.model.rename_place(at, &text(0)) {
                Ok(()) => {
                    self.sheet = None;
                    self.unsaved = true;
                    self.stale = true;
                }
                Err(error) => {
                    if let Some(sheet) = self.sheet.as_mut() {
                        sheet.work(Some(&error), false);
                    }
                }
            }
            return;
        }
        let checked: Result<(Job, String), String> = match purpose {
            For::MyName => users::check_full(&text(0))
                .map(|full| (Job::Rename(full), drifter_core::lang::t("Saving…").to_string())),
            For::MyPassword => match text(0).is_empty() {
                true => Err(drifter_core::lang::t("Type your current password.").to_string()),
                false => users::check_new_password(&text(1), &text(2)).map(|()| {
                    (
                        Job::Password {
                            old: text(0),
                            new: text(1),
                        },
                        drifter_core::lang::t("Changing your password…").to_string(),
                    )
                }),
            },
            For::AddUser => {
                let taken = self
                    .model
                    .users
                    .as_ref()
                    .map(|users| users.taken.clone())
                    .unwrap_or_default();
                users::check_full(&text(0)).and_then(|full| {
                    users::check_login(&text(1), &taken)?;
                    users::check_new_password(&text(2), &text(3))?;
                    let note = drifter_core::lang::tf("Adding {}. Type an admin password if asked.", &[&(full)]);
                    Ok((
                        Job::Add {
                            full,
                            login: text(1),
                            password: text(2),
                            admin: on,
                            picture: self.sheet.as_ref().and_then(Sheet::face),
                        },
                        note,
                    ))
                })
            }
            For::RemoveUser(at) => match self.model.other(at) {
                Some(user) => Ok((
                    Job::Remove {
                        uid: user.uid,
                        files: on,
                    },
                    drifter_core::lang::tf("Removing {}. Type an admin password if asked.", &[&(user.shown())]),
                )),
                None => Err(drifter_core::lang::t("That user is gone.").to_string()),
            },
            _ => return,
        };
        match checked {
            Ok((job, note)) => {
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.work(Some(&note), true);
                }
                self.user_job(purpose, job);
            }
            Err(error) => {
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.work(Some(&error), false);
                }
            }
        }
    }

    fn net_job(&mut self, job: crate::net::Job, note: String) {
        self.model.net_note = Some(note);
        let done = self.net_done.clone();
        let spawned = std::thread::Builder::new()
            .name("settings-net".to_string())
            .spawn(move || {
                let _ = done.send((false, crate::net::run(job)));
            });
        if let Err(error) = spawned {
            self.model.net_note = Some(drifter_core::lang::tf("Cannot start: {}", &[&(error)]));
        }
    }

    fn net_ask(&mut self, ask: drifter_core::settings::NetAsk) {
        use crate::net::Job;
        use drifter_core::settings::NetAsk;

        let net = |at: usize| self.model.wifi.as_ref().and_then(|wifi| wifi.nets.get(at).cloned());
        match ask {
            NetAsk::Join(at) => {
                let Some(net) = net(at) else {
                    return;
                };
                if net.enterprise && !net.known {
                    self.sheet = Some(Sheet::form(
                        For::WifiEnterprise(at),
                        &drifter_core::lang::tf("Join {}", &[&(net.ssid)]),
                        vec![
                            Field::plain(drifter_core::lang::t("Username"), ""),
                            Field::secret(drifter_core::lang::t("Password")),
                            Field::plain(drifter_core::lang::t("Server domain, if your school or work gave one"), ""),
                        ],
                        drifter_core::lang::t("Join"),
                    ));
                    return;
                }
                if net.secure && !net.known {
                    self.sheet = Some(Sheet::secret(For::Wifi(at), &drifter_core::lang::tf("Join {}", &[&(net.ssid)])));
                    return;
                }
                let note = drifter_core::lang::tf("Joining {}…", &[&(net.ssid)]);
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
                    let note = drifter_core::lang::tf("Leaving {}…", &[&(net.ssid)]);
                    self.net_job(Job::Leave(net.ssid), note);
                }
            }
            NetAsk::Forget(at) => {
                if let Some(net) = net(at) {
                    let note = drifter_core::lang::tf("Forgetting {}…", &[&(net.ssid)]);
                    self.net_job(Job::Forget(net.ssid), note);
                }
            }
            NetAsk::Hidden => {
                self.sheet = Some(Sheet::form(
                    For::WifiHidden,
                    drifter_core::lang::t("Join a hidden network"),
                    vec![
                        Field::plain(drifter_core::lang::t("Network name"), ""),
                        Field::secret(drifter_core::lang::t("Password, or leave it empty for an open network")),
                    ],
                    drifter_core::lang::t("Join"),
                ));
            }
            ask => self.blue_ask(ask),
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
        let pictures = drifter_core::folders::user_dir(&home(), "PICTURES", "Pictures");
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
            For::AirDropFolder => drifter_core::airdrop::received_folder(self.model.airdrop.folder.as_deref(), &home())
                .ancestors()
                .find(|folder| folder.is_dir())
                .map(Path::to_path_buf)
                .unwrap_or_else(home),
            For::Picture => near(self.model.wallpaper.as_ref()).unwrap_or(pictures),
            For::Pointer => near(self.model.assets[0].as_ref()).unwrap_or_else(home),
            For::Font => near(self.model.assets[1].as_ref())
                .or_else(|| Some(PathBuf::from("/usr/share/fonts")).filter(|f| f.is_dir()))
                .unwrap_or_else(home),
            For::WindowArt => near(self.model.assets[2].as_ref()).unwrap_or_else(home),
            For::ColorProfile => [home().join(".local/share/icc"), PathBuf::from("/usr/share/color/icc")]
                .into_iter()
                .find(|folder| folder.is_dir())
                .unwrap_or_else(home),
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
            Pick::Cancel if matches!(purpose, For::NewFace | For::DockIcon(_)) && self.parked.is_some() => {
                self.sheet = self.parked.take();
            }
            Pick::Cancel => {
                self.sheet = None;
                self.parked = None;
            }
            Pick::Focus(at) => {
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.focus(at);
                }
            }
            Pick::Toggle => {
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.toggle();
                }
                self.want_art();
            }
            Pick::Shape(at) => {
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.set_shape(at);
                }
                self.want_art();
            }
            Pick::Face if matches!(purpose, For::DockItem(_)) => {
                let For::DockItem(at) = purpose else {
                    return;
                };
                self.parked = self.sheet.take();
                self.open_files(For::DockIcon(at));
            }
            Pick::Face => {
                let chosen = self.sheet.as_ref().and_then(Sheet::face);
                self.parked = self.sheet.take();
                self.open_faces(For::NewFace, chosen);
            }
            Pick::Browse => {
                let pictures = drifter_core::folders::user_dir(&home(), "PICTURES", "Pictures");
                let start = if pictures.is_dir() { pictures } else { home() };
                self.sheet = Some(Sheet::files(purpose, start));
            }
            Pick::Submit { texts, on } => self.submit_form(purpose, texts, on),
            Pick::File(file) if purpose == For::MyFace => {
                self.sheet = None;
                self.model.user_note = Some(drifter_core::lang::t("Changing your picture…").to_string());
                self.user_job(For::MyFace, crate::users::Job::Face(file));
            }
            Pick::File(file) if matches!(purpose, For::DockIcon(_)) && self.parked.is_some() => {
                self.sheet = self.parked.take();
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.put_face(file);
                }
                self.want_art();
            }
            Pick::Default if matches!(purpose, For::DockIcon(_)) && self.parked.is_some() => {
                self.sheet = self.parked.take();
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.clear_face();
                }
                self.want_art();
            }
            Pick::File(file) if purpose == For::NewFace => {
                self.sheet = self.parked.take();
                self.want_faces(std::slice::from_ref(&file));
                if let Some(sheet) = self.sheet.as_mut() {
                    sheet.put_face(file);
                }
            }
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
                    let note = drifter_core::lang::tf("Joining {}…", &[&(net.ssid)]);
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
                    Launch::command_line(&text),
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
            Pick::File(file) if purpose == For::ColorProfile => {
                self.sheet = None;
                match drifter_core::color::load(&file) {
                    Ok(_) => {
                        self.model.display_note = None;
                        self.model.choose_profile(file);
                        self.unsaved = true;
                    }
                    Err(error) => self.model.display_note = Some(error),
                }
                self.stale = true;
            }
            Pick::Folder(folder) => {
                match purpose {
                    For::Folder => self.model.choose_folder(folder),
                    For::ShotFolder => self.model.choose_shot_folder(folder),
                    For::AirDropFolder => self.model.choose_airdrop_folder(folder),
                    _ => self.model.choose_file(purpose, folder),
                }
                self.sheet = None;
                self.save();
                self.want_thumbs();
            }
            Pick::File(file) => {
                self.model.choose_file(purpose, file);
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
                if let For::Shortcut(at) = purpose {
                    let _ = self.model.set_shortcut(at, None);
                }
                if purpose == For::BlueKey
                    && let Some(address) = self.model.blue_open.clone()
                {
                    self.model.blue_prefs.set_key(&address, "");
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
            .map(small)
            .map(|art| {
                let (wide, tall) = self.look.preview_size();
                drifter_core::settings::cover(&art, wide, tall)
            });
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
                self.stale = true;
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
            self.stale = true;
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
            self.stale = true;
        }
    }

    fn drop_monitor(&mut self) {
        if self.moving.take().is_some() {
            self.save();
            self.stale = true;
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
            if self.dragged == Some((id, value)) {
                return;
            }
            self.dragged = Some((id, value));
            let before = self.model.clone();
            self.model.set(id, value);
            if self.model != before {
                self.unsaved = true;
                self.stale = true;
            }
        }
    }

    fn animates(&self) -> bool {
        !self.model.access.reduce_motion && self.model.motion.animation != drifter_core::Animation::Off
    }

    fn smooth(&self) -> bool {
        let model = &self.model;
        model.mouse.smooth_scroll && !model.access.reduce_motion && model.motion.animation != drifter_core::Animation::Off
    }

    fn moving(&self) -> bool {
        self.glide.is_some() || self.page_fade.is_some() || self.flip.is_some() || !self.step_pops.is_empty() || self.step_turning() || self.shake.is_some()
    }

    fn shake_row(&mut self, image: &drifter_core::Image) -> Option<drifter_core::Image> {
        let (address, since) = self.shake.clone()?;
        let t = since.elapsed().as_secs_f64() / 0.5;
        if t >= 1.0 || self.section != Section::Bluetooth {
            self.shake = None;
            return None;
        }
        let at = self.model.blue.as_ref()?.devices.iter().position(|device| device.address == address)?;
        let groups = self.model.groups(self.section);
        let placed = self.look.place(self.view(), self.section, &groups, self.scroll);
        let row = groups
            .iter()
            .zip(&placed)
            .flat_map(|(group, spots)| group.rows.iter().zip(&spots.rows))
            .find(|(row, _)| row.id == Id::Gadget(at))
            .map(|(_, rect)| *rect)?;
        let dense = f64::from(scale());
        let (_, piece) = drifter_core::tween::crop(image, row, dense)?;
        let offset = (t * std::f64::consts::TAU * 3.0).sin() * 9.0 * (1.0 - t) * (1.0 - t);
        let mut canvas = drifter_core::paint::Canvas::on(image.clone());
        canvas.round_rect(row, 0.0, self.look.card, 1.0);
        canvas.stamp(&piece, row.origin + Vec2::new(offset, 0.0), 1.0);
        self.stale = true;
        Some(canvas.image)
    }

    fn step_turning(&self) -> bool {
        self.section == Section::Bluetooth
            && self.sheet.is_none()
            && self.model.blue_steps.iter().any(|(_, step)| *step == drifter_core::blue::Step::Doing)
    }

    fn wheel(&mut self, by: f64, click: bool) {
        let page = self.sheet.is_none() && self.at.x > self.look.sidebar;
        if !(click && page && self.smooth()) {
            if let Some(glide) = self.glide.take() {
                self.scroll = glide.to;
                self.stale = true;
            }
            self.scroll_by(by);
            return;
        }
        let groups = self.model.groups(self.section);
        let most = self.look.max_scroll(self.view(), self.section, &groups);
        let from = self.glide.map_or(self.scroll, |glide| glide.to);
        let to = (from + by).clamp(0.0, most);
        if to == self.scroll && self.glide.is_none() {
            return;
        }
        match self.glide.as_mut() {
            Some(glide) => glide.retarget(to, std::time::Instant::now()),
            None => self.glide = Some(drifter_core::tween::Glide::new(self.scroll, to, GLIDE)),
        }
        self.stale = true;
    }

    fn step_glide(&mut self) {
        let Some(glide) = self.glide.as_mut() else {
            return;
        };
        let now = std::time::Instant::now();
        if let Some(next) = glide.step(now, f64::from(scale())) {
            self.scroll = next;
            self.stale = true;
        }
        if glide.done(now) {
            self.glide = None;
        }
    }

    fn scroll_by(&mut self, by: f64) {
        let size = self.view();
        if let Some(sheet) = self.sheet.as_mut() {
            sheet.scroll_by(size, by);
            self.stale = true;
            return;
        }
        if self.at.x <= self.look.sidebar {
            let most = self.look.side_most(size.y);
            let before = self.look.side_scroll;
            self.look.side_scroll = (before + by).clamp(0.0, most);
            if self.look.side_scroll != before {
                self.stale = true;
            }
            return;
        }
        let before = self.scroll;
        self.scroll += by;
        self.clamp_scroll();
        if self.scroll != before {
            self.stale = true;
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
            self.stale = true;
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
        self.stale = true;
    }

    fn go_to(&mut self, section: Section) {
        self.glide = None;
        self.page_fade = match (self.smooth(), self.last.as_ref(), section != self.section) {
            (true, Some((_, image)), true) => Some((image.clone(), None)),
            _ => None,
        };
        self.section = section;
        self.model.focus = None;
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
                if self.model.query.trim().is_empty() {
                    return;
                }
                if let Some(first) = self.model.shown().first().copied() {
                    self.go_to(first);
                }
                self.stale = true;
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
        self.stale = true;
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
        model.users = old.users;
        model.user_note = old.user_note;
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
                Ok(backup) => drifter_core::lang::tf("Back to the defaults. The old file is {}", &[&(backup.display())]),
                Err(error) => drifter_core::lang::tf("Cannot reset: {}", &[&(error)]),
            },
            Err(error) => drifter_core::lang::tf("Cannot read the defaults in {}: {}", &[&(defaults.display()), &(error)]),
        };
        self.reload();
        self.model.file_note = Some(note);
    }

    fn import(&mut self, file: &Path) {
        let note = match std::fs::read_to_string(file) {
            Err(error) => drifter_core::lang::tf("Cannot read {}: {}", &[&(file.display()), &(error)]),
            Ok(text) => match drifter_core::settings::check_import(&text) {
                Err(error) => drifter_core::lang::tf("Not imported: {}", &[&(error)]),
                Ok(()) => match drifter_core::settings::replace_config(&self.path, &text) {
                    Ok(backup) => drifter_core::lang::tf("Imported. The old file is {}", &[&(backup.display())]),
                    Err(error) => drifter_core::lang::tf("Cannot import: {}", &[&(error)]),
                },
            },
        };
        self.reload();
        self.model.file_note = Some(note);
    }

    fn export(&mut self, folder: &Path) {
        let target = drifter_core::settings::export_path(folder, |path| path.exists());
        self.model.file_note = Some(match std::fs::copy(&self.path, &target) {
            Ok(_) => drifter_core::lang::tf("Saved to {}", &[&(target.display())]),
            Err(error) => drifter_core::lang::tf("Cannot save to {}: {}", &[&(target.display()), &(error)]),
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
        if let Some(Sheet::Capture {
            purpose: For::BlueKey,
            ..
        }) = self.sheet
        {
            self.capture_blue(event);
            return;
        }
        let Some(sheet) = self.sheet.as_mut() else {
            if self.naming_key(event) {
                return;
            }
            if !self.focus_key(event) && !self.nudge(event) {
                self.search_key(event);
            }
            return;
        };
        match event.keysym {
            Keysym::Escape => self.take(Pick::Cancel),
            Keysym::Tab => sheet.next_field(),
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
        self.stale = true;
    }

    fn nudge(&mut self, event: &KeyEvent) -> bool {
        let Some(id) = self.model.focus else {
            return false;
        };
        let way = match event.keysym {
            Keysym::Left | Keysym::Down => -1.0,
            Keysym::Right | Keysym::Up => 1.0,
            _ => return false,
        };
        let steps = if self.mods.shift { way * 10.0 } else { way };
        if let Some(value) = self.model.nudge(self.section, id, steps) {
            self.model.set(id, value);
            self.unsaved = true;
            self.stale = true;
        }
        true
    }

    fn apply_live(&mut self) {
        let before = self.saved.clone();
        let wifi_on = self.model.wifi.as_ref().map(|wifi| wifi.on);
        if wifi_on.is_some() && wifi_on != before.wifi.as_ref().map(|wifi| wifi.on) {
            let on = wifi_on.unwrap_or(false);
            let note = match on {
                true => drifter_core::lang::t("Turning Wi-Fi on…"),
                false => drifter_core::lang::t("Turning Wi-Fi off…"),
            };
            self.net_job(crate::net::Job::WifiPower(on), note.to_string());
        }
        let blue_on = self.model.blue.as_ref().map(|blue| blue.on);
        if blue_on.is_some() && blue_on != before.blue.as_ref().map(|blue| blue.on) {
            let on = blue_on.unwrap_or(false);
            let note = match on {
                true => drifter_core::lang::t("Turning Bluetooth on…"),
                false => drifter_core::lang::t("Turning Bluetooth off…"),
            };
            self.blue_job(note.to_string(), move |bus| bus.power(on));
        }
        self.apply_blue(&before);
        self.apply_air(&before);
        if let Some(now) = self.model.profile.as_ref()
            && before.profile.as_ref().is_some_and(|was| was.now != now.now)
        {
            let wanted = now.now.clone();
            let spawned = std::thread::Builder::new()
                .name("settings-profile".to_string())
                .spawn(move || set_profile(&wanted));
            if let Err(error) = spawned {
                log!("drifter-settings: cannot change the power mode: {error}");
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
                let _ = self.hands.send(Hand::Volume(target, level));
            }
            if !fresh && muted != was_muted {
                let _ = self.hands.send(Hand::Mute(target, muted));
            }
        }
        if fresh {
            self.model.sound = read_sound();
        }

        if let Some(level) = self.model.brightness
            && self.model.brightness != before.brightness
        {
            let _ = self.hands.send(Hand::Light(level));
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
                    log!(
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
                log!("drifter-settings: cannot change the time settings: {error}");
            }
        }

        if drifter_core::icon::follow(&self.model.appearance) {
            self.icons.clear();
            self.load_row_icons();
        }
        self.look = themed(&self.model, self.look.side_scroll);
    }

    fn save(&mut self) {
        self.unsaved = false;
        self.saved_at = Some(std::time::Instant::now());
        self.apply_live();
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                log!(
                    "drifter-settings: cannot read {}: {error}",
                    self.path.display()
                );
                return;
            }
        };
        let out = match self.model.write(&self.saved, &text) {
            Ok(out) => out,
            Err(error) => {
                log!("drifter-settings: {error}");
                return;
            }
        };
        if out == text {
            self.saved = self.model.clone();
            return;
        }
        match write_whole(&self.path, &out) {
            Ok(()) => self.saved = self.model.clone(),
            Err(error) => log!("drifter-settings: cannot save: {error}"),
        }
    }
}

impl CompositorHandler for Settings {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        factor: i32,
    ) {
        let factor = factor.clamp(1, 4) as u32;
        if factor == scale() {
            return;
        }
        SCALE.store(factor, std::sync::atomic::Ordering::Relaxed);
        drifter_core::paint::set_density(factor);
        self.icons.clear();
        self.thumbs.clear();
        self.asked.clear();
        self.load_row_icons();
        self.reload_wallpaper();
        self.want_thumbs();
        self.stale = true;
    }

    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, _: u32) {
        if self.logo.as_ref().is_some_and(|logo| &logo.surface == surface) {
            if self.logo.as_ref().is_some_and(|logo| logo.moving()) {
                self.draw_logo();
            }
            return;
        }
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
                Err(error) => log!("drifter-settings: no pointer: {error}"),
            }
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let repeat = Box::new(|settings: &mut Settings, _: &wl_keyboard::WlKeyboard, event: KeyEvent| {
                settings.key(&event)
            });
            match self.seats.get_keyboard_with_repeat(qh, &seat, None, self.handle.clone(), repeat) {
                Ok(keyboard) => self.keyboard = Some(keyboard),
                Err(error) => log!("drifter-settings: no keyboard: {error}"),
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
        self.access.focused(true);
    }

    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        self.access.focused(false);
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
                PointerEventKind::Axis { vertical, source, .. } => {
                    self.wheel(vertical.absolute, vertical.discrete != 0 || source == Some(wl_pointer::AxisSource::Wheel))
                }
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
smithay_client_toolkit::delegate_subcompositor!(Settings);
delegate_output!(Settings);
delegate_shm!(Settings);
delegate_seat!(Settings);
delegate_pointer!(Settings);
delegate_keyboard!(Settings);
delegate_xdg_shell!(Settings);
delegate_xdg_window!(Settings);
delegate_registry!(Settings);

fn splice(
    out: &mut drifter_core::Image,
    top: &drifter_core::Image,
    next: &drifter_core::Image,
    view: drifter_core::Vec2,
    region: drifter_core::Rect,
    offset: f64,
) -> bool {
    let (width, height) = (top.width as usize, top.height as usize);
    let same = |image: &drifter_core::Image| image.width as usize == width && image.height as usize == height;
    if !same(top) || !same(next) || !same(out) || view.y <= 0.0 {
        return false;
    }
    let dense = height as f64 / view.y;
    let px = |v: f64, most: usize| ((v * dense).round().max(0.0) as usize).min(most);
    let (x0, x1) = (px(region.origin.x, width), px(region.max().x, width));
    let (y0, y1) = (px(region.origin.y, height), px(region.max().y, height));
    let tall = y1.saturating_sub(y0);
    let shift = px(offset, tall);
    let row = width * 4;
    for y in 0..tall {
        let source = shift + y;
        let (from, line) = match source < tall {
            true => (top, y0 + source),
            false => (next, y0 + source - tall),
        };
        let into = (y0 + y) * row;
        let back = line * row;
        out.rgba[into + x0 * 4..into + x1 * 4].copy_from_slice(&from.rgba[back + x0 * 4..back + x1 * 4]);
    }
    true
}

impl Settings {
    fn start_blue(&mut self) {
        self.model.blue_prefs = drifter_core::blue::Prefs::load();
        self.saved.blue_prefs = self.model.blue_prefs.clone();
        let bus = match drifter_blue::Bus::system() {
            Ok(bus) => bus,
            Err(error) => {
                log!("drifter-settings: {error}");
                return;
            }
        };
        let back = self.blue_back.clone();
        if let Err(error) = bus.watch(move |now| {
            let _ = back.send(BlueNews::Now(now));
        }) {
            log!("drifter-settings: {error}");
        }
        self.blue_bus = Some(bus);
    }

    fn blue_news(&mut self, news: BlueNews) {
        match news {
            BlueNews::Now(seen) => {
                let had: Vec<String> = self.model.blue.iter().flat_map(|blue| blue.connected().map(|device| device.address.clone())).collect();
                self.model.blue_missing = seen.as_ref().err().copied();
                self.saved.blue_missing = self.model.blue_missing;
                let now = seen.ok();
                self.model.blue = now.clone();
                self.saved.blue = now;
                let open_gone = self.model.blue_open.as_deref().is_some_and(|address| {
                    !self.model.blue.as_ref().is_some_and(|blue| blue.find(address).is_some_and(|device| device.paired))
                });
                if open_gone {
                    self.model.blue_open = None;
                    self.model.blue_card = None;
                }
                let joined = self.model.blue_open.as_deref().is_some_and(|address| {
                    !had.iter().any(|each| each == address)
                        && self.model.blue.as_ref().is_some_and(|blue| blue.find(address).is_some_and(|device| device.connected))
                });
                if joined {
                    self.blue_card();
                }
                self.load_row_icons();
            }
            BlueNews::Card(address, card) => {
                if self.model.blue_open.as_deref() == Some(address.as_str()) {
                    self.model.blue_mic = card.as_ref().is_some_and(drifter_core::blue::Card::headset);
                    self.saved.blue_mic = self.model.blue_mic;
                    self.model.blue_card = card;
                }
            }
            BlueNews::Note(note) => self.model.net_note = Some(note),
            BlueNews::Air(report, hint, receiving) => {
                self.model.airdrop_report = Some(report.clone());
                self.saved.airdrop_report = Some(report);
                self.model.airdrop_hint = hint;
                self.saved.airdrop_hint = hint;
                self.model.airdrop_receiving = receiving;
                self.saved.airdrop_receiving = receiving;
                if self.section == Section::AirDrop && self.sheet.is_none() {
                    self.stale = true;
                }
            }
            BlueNews::Shake(address) => {
                if self.animates() {
                    self.shake = Some((address, std::time::Instant::now()));
                }
            }
            BlueNews::Step(at, step) => {
                if let Some(held) = self.model.blue_steps.get_mut(at) {
                    held.1 = match step {
                        drifter_core::blue::Step::Done => {
                            self.step_pops.push((at, std::time::Instant::now()));
                            drifter_core::blue::Step::Popping
                        }
                        other => other,
                    };
                }
                self.load_row_icons();
            }
            BlueNews::Done(error) => {
                if error.is_none() && !self.model.blue_steps.is_empty() {
                    self.model.net_note = Some(drifter_core::lang::t("Fixed. It is set up and connected.").to_string());
                } else {
                    self.model.net_note = error;
                }
                self.blue_card();
            }
        }
        if self.section == Section::Bluetooth && self.sheet.is_none() {
            self.stale = true;
        }
    }

    fn blue_search(&mut self) {
        let want = self.section == Section::Bluetooth && self.model.blue.as_ref().is_some_and(|blue| blue.on);
        if want == self.blue_searching {
            return;
        }
        self.blue_searching = want;
        let Some(bus) = self.blue_bus.clone() else {
            return;
        };
        let _ = std::thread::Builder::new().name("settings-blue-search".into()).spawn(move || {
            if let Err(error) = bus.search(want)
                && want
            {
                log!("drifter-settings: Bluetooth search did not start: {error}");
            }
        });
    }

    fn blue_card(&mut self) {
        let Some(address) = self.model.blue_open.clone() else {
            return;
        };
        let audio = self
            .model
            .blue
            .as_ref()
            .and_then(|blue| blue.find(&address))
            .is_some_and(|device| device.connected && (device.audio || device.kind.audio()));
        if !audio {
            self.model.blue_card = None;
            return;
        }
        let back = self.blue_back.clone();
        let _ = std::thread::Builder::new().name("settings-blue-card".into()).spawn(move || {
            let card = drifter_blue::sound::card(&address);
            let _ = back.send(BlueNews::Card(address, card));
        });
    }

    fn blue_device(&self, at: usize) -> Option<drifter_core::blue::Device> {
        self.model.blue.as_ref()?.devices.get(at).cloned()
    }

    fn blue_open_device(&self) -> Option<drifter_core::blue::Device> {
        let address = self.model.blue_open.as_deref()?;
        self.model.blue.as_ref()?.find(address).cloned()
    }

    fn blue_job(&mut self, note: String, work: impl FnOnce(&drifter_blue::Bus) -> Result<(), String> + Send + 'static) {
        let Some(bus) = self.blue_bus.clone().or_else(|| drifter_blue::Bus::system().ok()) else {
            self.model.net_note = Some(drifter_core::lang::t("Cannot reach Bluetooth.").to_string());
            return;
        };
        self.model.net_note = Some(note);
        self.stale = true;
        let back = self.blue_back.clone();
        let spawned = std::thread::Builder::new().name("settings-blue".into()).spawn(move || {
            let _ = back.send(BlueNews::Done(work(&bus).err()));
        });
        if let Err(error) = spawned {
            self.model.net_note = Some(drifter_core::lang::tf("Cannot start: {}", &[&(error)]));
        }
    }

    fn blue_device_job(&mut self, device: drifter_core::blue::Device, note: String, work: impl FnOnce(&drifter_blue::Bus, &str) -> Result<(), String> + Send + 'static) {
        let name = device.shown_name();
        let kind = device.kind;
        let address = device.address.clone();
        let back = self.blue_back.clone();
        self.blue_job(note, move |bus| {
            work(bus, &address).map_err(|error| match error.contains("Rejected") || error.contains("Canceled") {
                true => drifter_core::lang::t("Setting it up was stopped.").to_string(),
                false => {
                    let _ = back.send(BlueNews::Shake(address.clone()));
                    drifter_core::blue::plain_error(&error, &name, kind)
                }
            })
        });
    }

    fn blue_ask(&mut self, ask: drifter_core::settings::NetAsk) {
        use drifter_core::settings::NetAsk;

        match ask {
            NetAsk::Pair(at) => {
                if let Some(device) = self.blue_device(at) {
                    let note = drifter_core::lang::tf("Setting up {}. Answer on the screen if it asks.", &[&device.shown_name()]);
                    self.blue_device_job(device, note, |bus, address| bus.pair(address));
                }
            }
            NetAsk::Connect(at) => {
                if let Some(device) = self.blue_device(at) {
                    let note = drifter_core::lang::tf("Connecting to {}…", &[&device.shown_name()]);
                    self.blue_device_job(device, note, |bus, address| bus.connect(address));
                }
            }
            NetAsk::Disconnect(at) => {
                if let Some(device) = self.blue_device(at) {
                    let note = drifter_core::lang::tf("Disconnecting {}…", &[&device.shown_name()]);
                    self.blue_device_job(device, note, |bus, address| bus.disconnect(address));
                }
            }
            NetAsk::Remove(at) => {
                if let Some(device) = self.blue_device(at) {
                    let note = drifter_core::lang::tf("Forgetting {}…", &[&device.shown_name()]);
                    self.model.blue_open = None;
                    self.model.blue_card = None;
                    self.blue_device_job(device, note, |bus, address| bus.forget(address));
                }
            }
            NetAsk::Options(at) => {
                let Some(device) = self.blue_device(at).filter(|device| device.paired) else {
                    return;
                };
                self.model.blue_address = false;
                self.model.blue_card = None;
                self.model.blue_open = match self.model.blue_open.as_deref() == Some(device.address.as_str()) {
                    true => None,
                    false => Some(device.address),
                };
                self.blue_card();
                self.stale = true;
            }
            NetAsk::Rename => {
                if let Some(device) = self.blue_open_device() {
                    self.fade_page();
                    self.model.blue_naming = Some(device.shown_name());
                    self.stale = true;
                }
            }
            NetAsk::StopNaming => {
                self.fade_page();
                self.model.blue_naming = None;
                self.stale = true;
            }
            NetAsk::SaveName => {
                let name = self.model.blue_naming.take().unwrap_or_default().trim().to_string();
                self.fade_page();
                self.stale = true;
                if let Some(address) = self.model.blue_open.clone().filter(|_| !name.is_empty()) {
                    let note = drifter_core::lang::tf("Renaming it to {}…", &[&name]);
                    self.blue_job(note, move |bus| bus.rename(&address, &name));
                }
            }
            NetAsk::BlueKey => {
                if let Some(device) = self.blue_open_device() {
                    let now = self
                        .model
                        .blue_prefs
                        .key(&device.address)
                        .and_then(|text| drifter_core::shortcut::Combo::parse(text).ok().flatten())
                        .map_or_else(|| drifter_core::lang::t("No shortcut").to_string(), |combo| combo.shown(drifter_core::config::Modifier::Super));
                    let title = drifter_core::lang::tf("Shortcut to connect {}", &[&device.shown_name()]);
                    self.sheet = Some(Sheet::capture_for(For::BlueKey, &title, &now));
                    self.stale = true;
                }
            }
            NetAsk::Fix => {
                if let Some(device) = self.blue_open_device() {
                    self.blue_fix(device);
                }
            }
            NetAsk::OwlyPage => {
                let _ = std::process::Command::new("xdg-open")
                    .arg(drifter_core::airdrop::OWLY_PAGE)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null())
                    .spawn();
            }
            NetAsk::Service => {
                let note = drifter_core::lang::t("Starting the Bluetooth service. Type your password if asked.").to_string();
                self.blue_job(note, |_| drifter_blue::Bus::start_service());
            }
            NetAsk::Reset => {
                let note = drifter_core::lang::t("Turning Bluetooth off and on…").to_string();
                let back = self.blue_back.clone();
                self.blue_job(note, move |bus| {
                    bus.power(false)?;
                    bus.wait(|blue| !blue.on, std::time::Duration::from_secs(5));
                    std::thread::sleep(std::time::Duration::from_millis(600));
                    bus.power(true)?;
                    match bus.wait(|blue| blue.on, std::time::Duration::from_secs(8)) {
                        true => {
                            let _ = back.send(BlueNews::Note(drifter_core::lang::t("Bluetooth is back on.").to_string()));
                            Ok(())
                        }
                        false => Err(drifter_core::lang::t("Bluetooth did not come back on. Restart the computer.").to_string()),
                    }
                });
            }
            _ => {}
        }
    }

    fn blue_fix(&mut self, device: drifter_core::blue::Device) {
        use drifter_core::blue::Step;
        let back = self.blue_back.clone();
        let name = device.shown_name();
        let kind = device.kind;
        let ertm = drifter_core::blue::xbox(&device) && drifter_blue::xbox::ertm_on();
        let address = device.address.clone();
        self.model.blue_open = None;
        self.model.blue_card = None;
        self.model.blue_naming = None;
        self.step_pops.clear();
        let mut steps = Vec::new();
        if ertm {
            steps.push(drifter_core::lang::t("Change a setting older Xbox controllers need").to_string());
        }
        steps.push(drifter_core::lang::t("Turn Bluetooth off and on").to_string());
        steps.push(drifter_core::lang::tf("Forget {}", &[&name]));
        steps.push(drifter_core::lang::tf("Find {}. Put it in pairing mode now.", &[&name]));
        steps.push(drifter_core::lang::tf("Set up {}", &[&name]));
        self.model.blue_steps = steps.into_iter().map(|words| (words, Step::Wait)).collect();
        self.load_row_icons();
        let first = usize::from(ertm);
        let step = move |at: usize, state: Step| {
            let _ = back.send(BlueNews::Step(at, state));
        };
        self.blue_job(drifter_core::lang::tf("Fixing {}…", &[&name]), move |bus| {
            let run = |at: usize, work: &mut dyn FnMut() -> Result<(), String>| {
                step(at, Step::Doing);
                let done = work();
                step(at, if done.is_ok() { Step::Done } else { Step::Failed });
                done
            };
            if ertm {
                run(0, &mut || drifter_blue::xbox::fix_ertm())?;
            }
            run(first, &mut || {
                bus.power(false)?;
                bus.wait(|blue| !blue.on, std::time::Duration::from_secs(5));
                std::thread::sleep(std::time::Duration::from_millis(600));
                bus.power(true)?;
                match bus.wait(|blue| blue.on, std::time::Duration::from_secs(8)) {
                    true => Ok(()),
                    false => Err(drifter_core::lang::t("Bluetooth did not come back on. Restart the computer.").to_string()),
                }
            })?;
            run(first + 1, &mut || {
                let _ = bus.forget(&address);
                Ok(())
            })?;
            run(first + 2, &mut || {
                bus.search(true)?;
                match bus.wait(|blue| blue.find(&address).is_some(), std::time::Duration::from_secs(30)) {
                    true => Ok(()),
                    false => {
                        let _ = bus.search(false);
                        Err(drifter_core::lang::tf("Drifter could not find {}. Put it in pairing mode, then press Set Up under Nearby.", &[&name]))
                    }
                }
            })?;
            run(first + 3, &mut || {
                let paired = bus.set_up(&address);
                let _ = bus.search(false);
                paired.map_err(|error| drifter_core::blue::plain_error(&error, &name, kind))
            })?;
            bus.connect(&address).map_err(|error| drifter_core::blue::plain_error(&error, &name, kind))
        });
    }

    fn air_fetch(&mut self) {
        let back = self.blue_back.clone();
        let _ = std::thread::Builder::new().name("settings-airdrop".into()).spawn(move || {
            let report = drifter_blue::owly::report();
            let receiving = drifter_blue::owly::drifter_state().filter(|(_, ready, _)| *ready).map(|(_, _, on)| on);
            let _ = back.send(BlueNews::Air(report, drifter_blue::owly::hint(), receiving));
        });
    }

    fn apply_air(&mut self, before: &drifter_core::settings::Model) {
        if let Some(on) = self.model.airdrop_receiving
            && before.airdrop_receiving.is_some()
            && before.airdrop_receiving != Some(on)
        {
            let back = self.blue_back.clone();
            let _ = std::thread::Builder::new().name("settings-airdrop".into()).spawn(move || {
                if let Err(error) = drifter_blue::owly::drifter_receive(on) {
                    let _ = back.send(BlueNews::Note(drifter_core::lang::tf("AirDrop did not answer: {}", &[&error])));
                }
                std::thread::sleep(std::time::Duration::from_millis(600));
                let report = drifter_blue::owly::report();
                let receiving = drifter_blue::owly::drifter_state().filter(|(_, ready, _)| *ready).map(|(_, _, on)| on);
                let _ = back.send(BlueNews::Air(report, drifter_blue::owly::hint(), receiving));
            });
        }
        let untested = self
            .model
            .airdrop_report
            .as_ref()
            .is_some_and(|report| report.owly && report.verdict != drifter_core::airdrop::Verdict::Works);
        if self.model.airdrop.force && !before.airdrop.force && untested && !drifter_blue::owly::allows_untested() {
            let back = self.blue_back.clone();
            let _ = std::thread::Builder::new().name("settings-airdrop".into()).spawn(move || {
                let said = match drifter_blue::owly::allow_untested() {
                    Ok(()) => drifter_core::lang::t("Owly will now try this card.").to_string(),
                    Err(error) => error,
                };
                let _ = back.send(BlueNews::Note(said));
            });
        }
    }

    fn apply_blue(&mut self, before: &drifter_core::settings::Model) {
        if self.model.blue_prefs != before.blue_prefs {
            if let Err(error) = self.model.blue_prefs.save() {
                log!("drifter-settings: cannot save Bluetooth choices: {error}");
            }
        }
        let Some(device) = self.blue_open_device() else {
            return;
        };
        let sound = self.model.blue_prefs.sound(&device.address);
        if sound != before.blue_prefs.sound(&device.address) && device.connected {
            let address = device.address.clone();
            let note = drifter_core::lang::tf("Sound: {}…", &[&sound.label()]);
            self.blue_job(note, move |_| drifter_blue::sound::choose(&address, sound));
        }
        if self.model.blue_mic != before.blue_mic && device.connected {
            let (address, on) = (device.address.clone(), self.model.blue_mic);
            let note = match on {
                true => drifter_core::lang::t("Turning the microphone on. Sound quality drops while it is on.").to_string(),
                false => drifter_core::lang::t("Turning the microphone off…").to_string(),
            };
            self.blue_job(note, move |_| drifter_blue::sound::mic(&address, on, sound));
        }
    }

    fn fade_page(&mut self) {
        if self.smooth()
            && let Some((_, image)) = self.last.as_ref()
        {
            self.page_fade = Some((image.clone(), None));
        }
    }

    fn naming_key(&mut self, event: &KeyEvent) -> bool {
        use drifter_core::settings::NetAsk;
        let Some(text) = self.model.blue_naming.as_mut() else {
            return false;
        };
        match event.keysym {
            Keysym::Escape => self.blue_ask(NetAsk::StopNaming),
            Keysym::Return | Keysym::KP_Enter => self.blue_ask(NetAsk::SaveName),
            Keysym::BackSpace => {
                text.pop();
            }
            _ => {
                if let Some(typed) = event.utf8.as_deref().filter(|typed| typed.chars().all(|c| !c.is_control())) {
                    if text.chars().count() + typed.chars().count() <= 48 {
                        text.push_str(typed);
                    }
                }
            }
        }
        self.stale = true;
        true
    }

    fn capture_blue(&mut self, event: &KeyEvent) {
        let name = event.keysym.name().map(shortcut::keysym_word).unwrap_or_default();
        if name.is_empty() || shortcut::is_modifier(&name) {
            return;
        }
        let mods = Mods {
            ctrl: self.mods.ctrl,
            alt: self.mods.alt,
            shift: self.mods.shift,
            logo: self.mods.logo,
        };
        self.sheet = None;
        self.stale = true;
        if event.keysym == Keysym::Escape && mods == Mods::default() {
            return;
        }
        if mods == Mods::default() {
            self.model.net_note = Some(drifter_core::lang::t("A shortcut needs Ctrl, Alt or Super with the key.").to_string());
            return;
        }
        let combo = Combo { mods, camera: false, key: name };
        if let Some(address) = self.model.blue_open.clone() {
            self.model.blue_prefs.set_key(&address, &combo.word());
            self.save();
        }
    }
}
