use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::paint::{Painter, Scanout, Tex};

mod activity;
mod airdrop;
mod appname;
mod appwatch;
mod art;
mod bars;
mod bluetooth;
mod camera;
mod capture;
mod centers;
mod charges;
mod ctllist;
mod clients;
mod deskmenu;
mod dock;
mod docklogo;
mod fade;
mod follow;
mod genie;
mod ghost;
mod heads;
mod launchpad;
mod lit;
mod shoot;
mod spatial;
mod thumbs;
mod speech;
mod input;
mod lock;
mod overlays;
mod power;
mod reopen;
mod reveal;
mod search;
mod start;
mod welcome;
mod wifi;
mod worker;
#[cfg(test)]
mod tests;
mod wallpaper;
mod windows;

use art::*;
use follow::*;
use input::*;
use lock::*;
pub use start::{Seat, open};
pub use thumbs::send_shot;

use drifter_core::camera::{Camera, Flight};
use drifter_core::cursor::{self, Cursor};
use drifter_core::dock::{Dock, Slot};
use drifter_core::drawer::{self, Drawer, Edge, Panel};
use drifter_core::image::Image;
use drifter_core::lock::{Note, Pad};
use drifter_core::menu::{Item, Look};
use drifter_core::mode::{self, Mode};
use drifter_core::motion::{Motion, Phase};
use drifter_core::power::Action;
use drifter_core::snap::{Snap, Zone};
use drifter_core::text::Font;
use drifter_core::tray::Tray;
use drifter_core::window::{Button as ChromeButton, Chrome, Piece as ChromePiece};
use drifter_core::{Config, Rect, Tunables, Vec2};
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::compositor::DrmCompositor;
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::input::{
    AbsolutePositionEvent, ButtonState, Event as InputMeta, InputEvent, KeyState, KeyboardKeyEvent,
    PointerButtonEvent, PointerMotionEvent, Switch as HardSwitch, SwitchState, SwitchToggleEvent,
};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::Color32F;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::session::libseat::{LibSeatSession, LibSeatSessionNotifier};
use smithay::backend::session::{Event as SessionEvent, Session};
use smithay::output::OutputModeSource;
use smithay::reexports::calloop::channel::Event as ChannelEvent;
use smithay::reexports::calloop::signals::{Signal, Signals};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{EventLoop, LoopHandle, LoopSignal};
use smithay::reexports::drm::buffer::DrmFourcc;
use smithay::reexports::drm::control::{Device as ControlDevice, connector, crtc};
use smithay::reexports::input::Libinput;
use smithay::reexports::rustix::fs::OFlags;
use smithay::reexports::wayland_server::Display;
use smithay::utils::{DeviceFd, Scale, Transform};

const KEY_POWER: u32 = 124;
const KEY_UP: u32 = 111;
const KEY_LEFT: u32 = 113;
const KEY_RIGHT: u32 = 114;
const KEY_DOWN: u32 = 116;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoverCard {
    Battery,
}

const BATTERY_EVERY: Duration = Duration::from_secs(20);
const EBB_TICK: Duration = Duration::from_millis(50);
const LOCK_FADE: Duration = Duration::from_millis(450);
const BARS_AFTER_UNLOCK: Duration = Duration::from_millis(1600);
const LOCK_PATIENCE: Duration = Duration::from_secs(30);
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;
const CLOSE_PATIENCE: Duration = Duration::from_secs(2);
const PAPER_FADE: Duration = Duration::from_millis(1500);
const STYLE_FADE: Duration = Duration::from_millis(2000);
const LIVE_EVERY: Duration = Duration::from_millis(250);
const LIVE_STILL: Duration = Duration::from_secs(10);
const PANEL_ALPHA: u8 = 235;
const TICK: Duration = Duration::from_millis(8);
const CONFIG_WATCH: Duration = Duration::from_millis(400);
const HUD_TIME: Duration = Duration::from_millis(1500);
const SETTINGS_APP: &str = "drifter-settings";
const MARKUP_APP: &str = "drifter-markup";
const SECOND_TICK: Duration = Duration::from_millis(250);
const SETTLING: Duration = Duration::from_secs(2);
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const PUSH_BAND: f64 = 8.0;
const DETAIL_FADE: (f64, f64) = (0.72, 0.85);

use smithay::backend::renderer::{ImportAll, RendererSuper, element::render_elements};

use smithay::backend::renderer::element::utils::RescaleRenderElement;

render_elements! {
    pub Piece<R> where R: ImportAll;
    Texture=TextureRenderElement<<R as RendererSuper>::TextureId>,
    Client=RescaleRenderElement<WaylandSurfaceRenderElement<R>>,
    Clipped=smithay::backend::renderer::element::utils::CropRenderElement<RescaleRenderElement<WaylandSurfaceRenderElement<R>>>,
}

type Skin = Piece<Painter>;

type Shown = RescaleRenderElement<Skin>;

type ThemeCursor = (TextureBuffer<Tex>, Vec2, Vec2);

type Showing = (
    Rect,
    Rect,
    f32,
    Vec2,
    (f64, bool),
    f32,
    String,
    smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
);

pub struct Found {
    pub device: PathBuf,
    pub connector: String,
    pub width: u16,
    pub height: u16,
    pub refresh: u32,
    pub renderer: String,
    pub socket: String,
    pub wallpaper: Option<Rect>,
    pub camera: drifter_core::camera::Camera,
    pub drawer: Rect,
    pub corner: f64,
}

struct Head {
    name: String,
    connector: connector::Handle,
    crtc: crtc::Handle,
    compositor: Scanout,
    output: smithay::output::Output,
    global: smithay::reexports::wayland_server::backend::GlobalId,
    mode: smithay::output::Mode,
    pixels: Vec2,
    scale: f64,
    rotate: u32,
    flip: bool,
    vrr: bool,
    mirror: Option<String>,
    place: Vec2,
    camera: Camera,
    flight: Option<(Flight, Instant)>,
    modes: Vec<smithay::reexports::drm::control::Mode>,
    fit: Option<(u32, u32)>,
    drawers: [Drawer; 2],
    screen: Vec2,
    panels: [TextureBuffer<Tex>; 2],
    awaiting: bool,
    frames: u32,
    last: Instant,
    warmth: Option<(u32, Option<String>, Option<u32>)>,
    feedback: Option<smithay::desktop::utils::OutputPresentationFeedback>,
}

struct Backdrop {
    pyramid: std::sync::Arc<drifter_core::tiles::Pyramid>,
    resident: std::collections::HashMap<(usize, u32, u32), (TextureBuffer<Tex>, Instant)>,
    frost: std::sync::Arc<drifter_core::frost::Frost>,
    live: Option<wallpaper::Live>,
    shader: bool,
    pointer: Option<(Vec2, Instant)>,
}

impl Backdrop {
    fn new(pyramid: drifter_core::tiles::Pyramid, live: Option<wallpaper::Live>, shader: bool) -> Backdrop {
        let pyramid = std::sync::Arc::new(pyramid);
        let smallest = &pyramid.levels[pyramid.levels.len() - 1];
        let frost = std::sync::Arc::new(drifter_core::frost::Frost::build(smallest));
        Backdrop {
            pyramid,
            resident: std::collections::HashMap::new(),
            frost,
            live,
            shader,
            pointer: None,
        }
    }

    fn paper(&self, screen: Vec2) -> Vec2 {
        match self.shader {
            true => drifter_core::wallpaper::shader_paper(screen),
            false => self.pyramid.size(),
        }
    }

    fn laid(
        &self,
        mode: drifter_core::WallpaperMode,
        screen: Vec2,
        world: Vec2,
        camera: &Camera,
    ) -> drifter_core::wallpaper::Placement {
        let paper = self.paper(screen);
        drifter_core::wallpaper::place(mode, paper, screen, world, camera).onto(paper, self.pyramid.size())
    }
}

type Chore = Box<dyn FnOnce(&mut State) + Send>;

const TILE_KEEP: Duration = Duration::from_secs(5);

pub struct Screen {
    found: Found,
    events: EventLoop<'static, State>,
    state: State,
}

enum Target {
    Tray {
        service: String,
        trail: Vec<i32>,
        at: Vec2,
    },
    Window(u64),
    Dock(String),
    DockBar,
    PadApp(String),
    App(String),
    Desktop,
    Shooter,
    Shot(u64),
    Bluetooth,
    Wifi,
}

struct Switch {
    order: Vec<u64>,
    at: usize,
    ring: (Rect, Instant),
    layout: drifter_core::switcher::Layout,
    art: Option<TextureBuffer<Tex>>,
    icons: Vec<Option<TextureBuffer<Tex>>>,
    lane: worker::Lane,
}

enum Shortcut {
    Do(drifter_core::shortcut::Action, bool),
    Place(usize),
    SavePlace(usize),
    Media(crate::media::Key),
    Vt(i32),
    Swallow,
    Heard(u32),
    Thumb(thumbs::Chore),
    StopTimer,
    Blue(String),
}

struct Paper {
    set: Option<PathBuf>,
    shown: Option<PathBuf>,
    folder: Option<PathBuf>,
    every: u64,
    order: drifter_core::WallpaperOrder,
    next: Option<Instant>,
    hour: Option<u32>,
    drift: f64,
    clock: (f64, Instant),
}

impl Paper {
    fn from(config: &Config) -> Paper {
        Paper {
            set: config.wallpaper_path(),
            shown: config.wallpaper_path(),
            folder: config.wallpaper_folder(),
            every: config.workspace.wallpaper_every,
            order: config.workspace.wallpaper_order,
            next: None,
            hour: config
                .wallpaper_path()
                .filter(|path| drifter_core::shader::is_shader(path))
                .map(|_| drifter_core::night::hour_now()),
            drift: match config.access.reduce_motion {
                true => 0.0,
                false => f64::from(config.workspace.wallpaper_motion) / 100.0,
            },
            clock: (0.0, Instant::now()),
        }
    }
}

struct Hud {
    art: TextureBuffer<Tex>,
    rect: Rect,
    until: Instant,
    from: f64,
    to: f64,
    since: Instant,
    icon: Option<(TextureBuffer<Tex>, Instant)>,
}

struct Card {
    note: drifter_core::notify::Note,
    art: TextureBuffer<Tex>,
    tall: f64,
    until: Option<Instant>,
    born: Instant,
    from_y: f64,
    moved: Instant,
    frosted: f64,
}

struct Pending {
    asker: overlays::Asker,
    pad: Pad,
    error: Option<String>,
    busy: bool,
    shaken: Option<Instant>,
}


struct Popover {
    items: Vec<Item>,
    rect: Rect,
    anchor: Vec2,
    opened: Option<Instant>,
    art: Option<TextureBuffer<Tex>>,
    target: Target,
    lane: worker::Lane,
    keyed: Option<usize>,
    slide: Option<(TextureBuffer<Tex>, Rect, Instant, f64)>,
}

#[derive(Clone, Copy, Debug)]
struct OverviewHold {
    id: u64,
    from: Vec2,
    offset: Vec2,
    moved: bool,
}

const OVERVIEW_SLOP: f64 = 6.0;

enum Grab {
    None,
    Move {
        id: u64,
        offset: Vec2,
    },
    Resize {
        id: u64,
        grip: drifter_core::window::Grip,
        from: Vec2,
        start: Rect,
    },
}

pub struct ClientLock {
    locker: Option<smithay::wayland::session_lock::SessionLocker>,
    lock: wayland_protocols::ext::session_lock::v1::server::ext_session_lock_v1::ExtSessionLockV1,
    surfaces: Vec<(
        smithay::wayland::session_lock::LockSurface,
        smithay::reexports::wayland_server::protocol::wl_output::WlOutput,
    )>,
    refocus: Option<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface>,
}

struct Lock {
    since: Instant,
    pad: Pad,
    note: Note,
    checking: Option<Instant>,
    ticket: Option<u64>,
    art: Option<TextureBuffer<Tex>>,
    top: Option<(TextureBuffer<Tex>, Vec2)>,
    corner: Option<(TextureBuffer<Tex>, Vec2)>,
    veil: Option<(TextureBuffer<Tex>, [u8; 3])>,
    caret: Option<(TextureBuffer<Tex>, [u8; 3])>,
    typed: Instant,
    corner_lane: worker::Lane,
    corner_glow: Option<TextureBuffer<Tex>>,
    glow_lane: worker::Lane,
    clock: Option<(TextureBuffer<Tex>, Vec2)>,
    clock_lane: worker::Lane,
    shade: Option<TextureBuffer<Tex>>,
    shade_lane: worker::Lane,
    gear: Option<(TextureBuffer<Tex>, [u8; 3])>,
    shown: (String, String, usize, Note, [u8; 3]),
    lane: worker::Lane,
    top_lane: worker::Lane,
    name: String,
    user: std::ffi::CString,
    refocus: Option<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface>,
}

struct Hit {
    index: Option<usize>,
    layer: bool,
    surface: smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    origin: Vec2,
    popup: bool,
}

#[derive(Clone)]
pub struct Plug {
    pub name: String,
    pub modes: Vec<drifter_core::mode::Mode>,
    pub millimeters: (u32, u32),
    pub on: bool,
}

enum LockKey {
    Vt(i32),
    Letter(char),
    Enter,
    Back,
    Clear,
    Up,
    Down,
    Left,
    Right,
    Other,
}

pub struct State {
    pub output: smithay::output::Output,
    pub wayland: crate::shell::Wayland,
    pub clip: crate::clipboard::Keeper,
    pub windows: Vec<crate::shell::Window>,
    pub popups: smithay::desktop::PopupManager,
    display: Option<Display<State>>,
    chrome: Chrome,
    camera: Camera,
    world: Vec2,
    mode: drifter_core::WallpaperMode,
    flight: Option<(Flight, Instant)>,
    corner_since: Option<Instant>,
    grab: Grab,
    modifier: bool,
    fingers: u32,
    touch: drifter_core::Touchpad,
    touchpads: Vec<smithay::reexports::input::Device>,
    mouse: drifter_core::Mouse,
    mice: Vec<smithay::reexports::input::Device>,
    power_rules: drifter_core::Power,
    slept: bool,
    notify: drifter_core::Notify,
    sound: drifter_core::config::SoundConfig,
    spatial: Option<crate::spatial::Spatial>,
    noted: Vec<u32>,
    shortcuts: Vec<(
        drifter_core::shortcut::Action,
        drifter_core::shortcut::Combo,
    )>,
    switch_mods: drifter_core::shortcut::Mods,
    swallowed: Vec<u32>,
    appearance: drifter_core::Appearance,
    style: drifter_core::Style,
    gtk_layout: String,
    asset_paths: [Option<PathBuf>; 4],
    holds: drifter_core::Modifier,
    last_title: Option<(Vec2, Instant)>,
    held: Option<usize>,
    focus: Option<u64>,
    next_window: u64,
    eaten: Vec<u32>,
    frame_timer: bool,
    cursor_status: smithay::input::pointer::CursorImageStatus,
    client_cursor: smithay::input::pointer::CursorImageStatus,
    named_cursors: std::collections::HashMap<&'static str, Option<ThemeCursor>>,
    layer_focus: Option<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface>,
    skin: std::collections::HashMap<String, (TextureBuffer<Tex>, Vec2)>,
    socket: String,
    drm: Option<DrmDevice>,
    gbm: Option<GbmDevice<DrmDeviceFd>>,
    copier: Option<Result<GlesRenderer, String>>,
    render_node: Option<smithay::backend::drm::DrmNode>,
    render_formats: smithay::backend::allocator::format::FormatSet,
    virtual_size: (u32, u32),
    monitors: Vec<drifter_core::screens::MonitorConfig>,
    screens_wide: (u32, u32),
    heads: Vec<Head>,
    plugs: Vec<Plug>,
    at: usize,
    hand: usize,
    libinput: Option<Libinput>,
    renderer: Painter,
    wallpaper: Option<Backdrop>,
    panels: [TextureBuffer<Tex>; 2],
    shapes: [Panel; 2],
    arrow: TextureBuffer<Tex>,
    arrow_size: Vec2,
    arrow_hotspot: Vec2,
    font: Option<std::sync::Arc<Font>>,
    tray: Tray,
    dock: Dock,
    pinned: Pinned,
    pinned_from: (drifter_core::DockConfig, drifter_core::AppsConfig),
    apps_chosen: drifter_core::AppsConfig,
    dock_spare: Option<drifter_core::Image>,
    trash: Option<TextureBuffer<Tex>>,
    cog: Option<TextureBuffer<Tex>>,
    dot: Option<TextureBuffer<Tex>>,
    glow: Option<TextureBuffer<Tex>>,
    glow_key: (f64, [u8; 3]),
    rule: Option<TextureBuffer<Tex>>,
    app_icons: std::collections::HashMap<String, TextureBuffer<Tex>>,
    no_icon: std::collections::HashSet<String>,
    format: String,
    clock: Option<(String, TextureBuffer<Tex>, Rect)>,
    stats: Option<(TextureBuffer<Tex>, Vec2)>,
    sampler: crate::stats::Sampler,
    sampled: Option<Instant>,
    glance: drifter_core::glance::Style,
    glance_fields: Vec<(String, u8)>,
    cpu_history: Vec<u8>,
    stats_busy: bool,
    glance_card: Option<(TextureBuffer<Tex>, Rect)>,
    card_for: Option<HoverCard>,
    card_busy: bool,
    card_stale: bool,
    icons: Vec<(String, String, TextureBuffer<Tex>, bool)>,
    tray_raw: Vec<crate::tray::Icon>,
    frost_key: Option<[i64; 8]>,
    fading: Option<(Backdrop, Instant)>,
    control: Option<centers::Panel>,
    control_icon: Option<TextureBuffer<Tex>>,
    control_status: drifter_core::control::Status,
    radio: bluetooth::Radio,
    wifi: wifi::Wireless,
    control_list: Option<ctllist::List>,
    drop: airdrop::Drop,
    control_stale: bool,
    notes_panel: Option<(TextureBuffer<Tex>, Rect, usize)>,
    charges: Option<charges::ChargesPanel>,
    glance_panel: Option<activity::GlancePanel>,
    activity_launched: Option<Instant>,
    fn_key: drifter_core::fnkey::FnKey,
    search: Option<search::Search>,
    launchpad: Option<launchpad::Pad>,
    welcome: Option<welcome::Welcome>,
    dock_logo: docklogo::DockLogo,
    lock_logo: docklogo::DockLogo,
    battery_art: Option<(TextureBuffer<Tex>, Vec2)>,
    battery_seen: Option<(bool, u8)>,
    battery_at: Option<Instant>,
    battery_busy: bool,
    show_battery: bool,
    dock_fit: bool,
    pub drag_shot: Option<PathBuf>,
    pub drag_icon: Option<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface>,
    window_pace: f64,
    window_still: bool,
    dock_pad: f64,
    fit_busy: bool,
    battery_glow: Option<TextureBuffer<Tex>>,
    ebb_from: Instant,
    welcome_wanted: bool,
    login_lock: bool,
    search_font: Option<std::sync::Arc<Font>>,
    search_icons: std::collections::HashMap<String, Option<drifter_core::Image>>,
    app_label: Option<(String, TextureBuffer<Tex>, Vec2)>,
    window_shadow: Option<(TextureBuffer<Tex>, f64)>,
    shades: std::collections::HashMap<(u32, u32), Option<(TextureBuffer<Tex>, f64)>>,
    shades_wanted: std::cell::RefCell<Vec<(u32, u32)>>,
    bounces: Vec<dock::Bounce>,
    dock_press: Option<(usize, Vec2)>,
    dock_drag: Option<dock::DockDrag>,
    trash_full: bool,
    trash_seen: Option<Instant>,
    trash_full_art: Option<TextureBuffer<Tex>>,
    dock_art_dir: Option<PathBuf>,
    badges: std::collections::HashMap<String, dock::Badge>,
    badge_art: std::collections::HashMap<String, (TextureBuffer<Tex>, Vec2)>,
    drops: Vec<TextureBuffer<Tex>>,
    frost_at: Option<Instant>,
    frost_drift: Option<([i64; 3], Instant)>,
    frost_busy: bool,
    reused: std::collections::HashMap<&'static str, Tex>,
    installed: Option<(std::sync::Arc<Vec<drifter_core::desktop::App>>, Instant)>,
    app_names: std::cell::RefCell<std::collections::HashMap<String, String>>,
    resizing: Option<u64>,
    density: u32,
    palette: drifter_core::theme::Palette,
    popover: Option<Popover>,
    menu_titles: Vec<String>,
    menu_slide: Option<f64>,
    desk_menu: Option<deskmenu::DeskMenu>,
    ripple: Option<deskmenu::Ripple>,
    ripple_program: Option<Option<smithay::backend::renderer::gles::GlesTexProgram>>,
    ripple_on: bool,
    ripple_strength: f64,
    alert: Option<deskmenu::Alert>,
    reopening: Option<(drifter_core::reopen::Desk, Instant)>,
    switch: Option<Switch>,
    lock: Option<Lock>,
    greeter: Option<crate::greeter::Greeter>,
    greeted: bool,
    stepped: bool,
    handoff: bool,
    fly_in: Option<Instant>,
    fly_frost: Option<TextureBuffer<Tex>>,
    unlocking: Option<(Lock, Instant)>,
    client_lock: Option<ClientLock>,
    volume: Option<(f64, bool)>,
    volume_read: Option<Instant>,
    light: Option<f64>,
    light_read: Option<Instant>,
    lock_big: Option<std::sync::Arc<Font>>,
    lock_small: Option<std::sync::Arc<Font>>,
    lock_name: Option<std::sync::Arc<Font>>,
    card_font: Option<std::sync::Arc<Font>>,
    note_head: Option<std::sync::Arc<Font>>,
    note_body: Option<std::sync::Arc<Font>>,
    verify: smithay::reexports::calloop::channel::Sender<(u64, crate::auth::Verdict)>,
    tickets: u64,
    power: Option<TextureBuffer<Tex>>,
    look: Look,
    tip: Option<(TextureBuffer<Tex>, Rect)>,
    tip_of: Option<String>,
    tip_want: Option<String>,
    tip_lane: worker::Lane,
    notes_lane: worker::Lane,
    hud_lane: worker::Lane,
    hud_want: Option<drifter_core::hud::Reading>,
    prompt_lane: worker::Lane,
    haze_lane: worker::Lane,
    icon_jobs: worker::Icons,
    snap: Snap,
    config_seen: Option<(std::time::SystemTime, u64, u64)>,
    last_input: Instant,
    keys: drifter_core::Keyboard,
    night: drifter_core::Night,
    lock_after: u64,
    screen_off: u64,
    dark: bool,
    login: Vec<drifter_core::Launch>,
    autostart: bool,
    vt: Option<LibSeatSession>,
    paper: Paper,
    paper_tx: smithay::reexports::calloop::channel::Sender<(
        PathBuf,
        Option<drifter_core::tiles::Pyramid>,
        Option<[u8; 3]>,
    )>,
    paper_tint: Option<[u8; 3]>,
    told_accent: Option<drifter_core::Accent>,
    chores: smithay::reexports::calloop::channel::Sender<Chore>,
    config_path: PathBuf,
    zone: Option<Zone>,
    plain: Option<TextureBuffer<Tex>>,
    fades: fade::Fades,
    round: Option<TextureBuffer<Tex>>,
    accent_round: Option<TextureBuffer<Tex>>,
    snap_art: Option<TextureBuffer<Tex>>,
    black_round: Option<TextureBuffer<Tex>>,
    plain_round: Option<TextureBuffer<Tex>>,
    dock_hover: std::collections::HashMap<drifter_core::dock::Slot, (f64, f64, bool)>,
    dock_dots: std::collections::HashMap<String, (f64, f64)>,
    dock_hover_mode: drifter_core::DockHover,
    dock_magnify: f64,
    dock_lens: (f64, f64),
    dock_wound: Instant,
    dock_slides: std::collections::HashMap<String, dock::Slide>,
    dock_ghosts: Vec<(TextureBuffer<Tex>, Rect, Instant)>,
    dock_span: (f64, f64, Instant),
    lit: Option<lit::Lit>,
    ghosts: Vec<ghost::Ghost>,
    stows: std::collections::HashMap<u64, genie::Stow>,
    genie: Option<Option<smithay::backend::renderer::gles::GlesTexProgram>>,
    minimize_effect: drifter_core::Minimize,
    paper_reacts: bool,
    gl_name: String,
    kept: std::collections::HashMap<u64, (Tex, Vec2)>,
    zone_glide: Option<(Rect, Instant)>,
    titles: std::collections::HashMap<String, (TextureBuffer<Tex>, Vec2)>,
    hover_since: Option<(Vec2, Instant)>,
    tray_bus: Option<crate::tray::Tray>,
    media: Option<crate::media::Media>,
    notes_bus: Option<crate::notes::Notes>,
    copies: Vec<crate::screencopy::Copy>,
    prompts: Vec<Pending>,
    prompt_art: Option<(TextureBuffer<Tex>, Rect)>,
    prompt_haze: Option<TextureBuffer<Tex>>,
    checked: smithay::reexports::calloop::channel::Sender<(String, crate::polkit::Outcome)>,
    copied: std::collections::HashSet<smithay::reexports::wayland_server::backend::ClientId>,
    cursor_count: usize,
    shot_round: Option<shoot::Round>,
    shot_count: Option<shoot::Countdown>,
    shot_flash: Vec<(String, Rect, [f64; 4], Instant)>,
    restyle_due: Option<Box<Config>>,
    style_shots: Vec<(usize, TextureBuffer<Tex>)>,
    style_tex: Vec<(usize, smithay::backend::renderer::gles::GlesTexture)>,
    reveal: Option<Option<reveal::Reveal>>,
    reveal_from: Option<Vec2>,
    paper_reveal: Option<(smithay::backend::renderer::gles::GlesTexture, (i32, i32), Vec2, Instant)>,
    paper_sheet: Option<(wallpaper::Sheet, (i32, i32))>,
    paper_from: Option<Vec2>,
    transition: drifter_core::Transition,
    style_fade: Option<Instant>,
    shot_tool: drifter_core::grab::Tool,
    shot_clip: bool,
    shot_window: Option<u64>,
    shot_inks: Option<shoot::Inks>,
    shooter: Option<shoot::Shooter>,
    recording: Option<shoot::Recording>,
    thumbs: thumbs::Thumbs,
    sound_from: Option<String>,
    cursor_end: usize,
    cards: Vec<Card>,
    hud: Option<Hud>,
    hud_pill: Option<TextureBuffer<Tex>>,
    drawers: [Drawer; 2],
    tunables: Tunables,
    overview_hold: Option<OverviewHold>,
    places: Vec<(drifter_core::place::Place, Option<drifter_core::shortcut::Combo>)>,
    place_saves: Vec<(usize, drifter_core::shortcut::Combo)>,
    note_folders: std::collections::HashMap<u32, PathBuf>,
    note_links: std::collections::HashMap<u32, String>,
    access: drifter_core::Access,
    a11y: Option<crate::a11y::Monitor>,
    speech: Option<speech::Speech>,
    reading: bool,
    a11y_locks: Option<(bool, bool)>,
    sticky: drifter_core::sticky::Sticky,
    shots: drifter_core::Shots,
    screen: Vec2,
    pointer: Vec2,
    dwell: Option<(Edge, Instant)>,
    leave: Option<Instant>,
    last: Instant,
    clear: Color32F,
    active: bool,
    ticking: bool,
    handle: LoopHandle<'static, State>,
    signal: LoopSignal,
    failure: Option<String>,
}

impl State {
    fn animating(&self) -> bool {
        self.drawers.iter().any(|drawer| !drawer.settled())
            || self.flight.is_some()
            || self.windows.iter().any(|window| window.motion.is_some())
            || !self.bounces.is_empty()
            || self.fading.is_some()
            || self.style_fade.is_some()
            || self.lock_fading()
            || self.windows.iter().any(|window| window.closed_at.is_some())
            || self.dock_drag.is_some()
            || self.desk_menu_busy()
            || self.fades_busy()
            || self.popover_busy()
            || self.cards_moving()
            || self.hud_busy()
            || self.snap_gliding()
            || self.dock_winding()
            || self.dock_gliding()
            || self.lit_moving()
            || self.search_moving()
            || self.pad_moving()
            || self.switch_moving()
            || !self.ghosts.is_empty()
            || self.paper_stirring()
            || self.paper_reveal.is_some()
            || self.windows.iter().any(|window| window.lit != self.lit_target(window.id))
            || self.thumbs_moving()
            || self.shots_moving()
            || self.welcome_busy()
            || self.dock_logo.hot
            || self.blue_moving()
            || self.wifi_moving()
            || self.list_moving()
            || self.share_moving()
            || self.heads.iter().enumerate().any(|(index, head)| {
                index != self.at
                    && (head.flight.is_some() || head.drawers.iter().any(|d| !d.settled()))
            })
    }

    fn pending(&self) -> Option<Duration> {
        let rest = self.waiting();
        if self.dark {
            return rest;
        }
        let live = self
            .wallpaper
            .as_ref()
            .and_then(|paper| paper.live.as_ref())
            .map(|live| live.wait(self.live_every()));
        match (rest, live) {
            (Some(rest), Some(live)) => Some(rest.min(live)),
            (rest, live) => rest.or(live),
        }
    }

    fn waiting(&self) -> Option<Duration> {
        if self.dark {
            return None;
        }
        if self.lock.is_some() {
            let glowing = self.lock.as_ref().is_some_and(|lock| lock.corner_glow.is_some());
            if self.lock_fading() {
                return Some(TICK);
            }
            return Some(match (self.lock_logo.pace > 0.0, glowing) {
                (true, _) => TICK,
                (false, true) => EBB_TICK,
                (false, false) => drifter_core::lock::BLINK / 2,
            });
        }
        if self.animating() {
            return Some(TICK);
        }
        if self.alert.is_some() && self.window_pace > 0.0 {
            return Some(EBB_TICK);
        }
        if let Some(hud) = self.hud.as_ref() {
            return Some(hud.until.saturating_duration_since(Instant::now()).max(TICK));
        }
        if self.thumbs_waiting() {
            return Some(Duration::from_millis(200));
        }
        if !self.cards.is_empty() {
            return Some(SECOND_TICK);
        }
        if self.dwell.is_some()
            || self.leave.is_some()
            || self.animating()
            || self.corner_since.is_some()
            || self.modifier
        {
            return Some(TICK);
        }
        if self.hover_since.is_some() && self.tip.is_none() {
            return Some(SECOND_TICK);
        }
        if self.glows() && drifter_core::corner::heat(self.pointer, self.glow_key.0) > 0.0 {
            return Some(TICK);
        }
        if self.battery_glow.is_some() && self.drawers[Edge::Top.index()].visible() {
            return Some(EBB_TICK);
        }
        if self.drawers[Edge::Top.index()].visible() && self.font.is_some() {
            return Some(SECOND_TICK);
        }
        None
    }

    fn elements(&mut self) -> Vec<Skin> {
        let elements = self.gather_elements();
        match self.bake_shades() {
            true => self.gather_elements(),
            false => elements,
        }
    }

    fn gather_elements(&mut self) -> Vec<Skin> {
        let live = self.at == self.hand;
        if self.client_lock.is_some() {
            let mut elements = match live {
                true => self.cursor_elements(),
                false => Vec::new(),
            };
            self.cursor_count = elements.len();
            self.cursor_end = self.cursor_count;
            self.client_lock_elements(&mut elements);
            return elements;
        }
        let presence = self.lock_presence();
        if self.lock.is_some() && presence.is_some_and(|shown| shown >= 1.0) {
            let mut elements = match live {
                true => self.cursor_elements(),
                false => Vec::new(),
            };
            self.cursor_count = elements.len();
            self.cursor_end = self.cursor_count;
            if live {
                self.alert_elements(&mut elements);
                self.popover_elements(&mut elements);
            }
            elements.extend(self.lock_elements(1.0));
            return elements;
        }
        let layer = match presence {
            Some(shown) => self.lock_elements(shown),
            None => Vec::new(),
        };
        let mut elements: Vec<Skin> = Vec::new();
        if live {
            match self.shooter_cursor() {
                Some(cross) => elements.extend(cross),
                None => elements.extend(self.cursor_elements()),
            }
        }
        self.cursor_end = elements.len();
        if live {
            self.own_elements(&mut elements);
        }
        self.shot_flash_elements(&mut elements);
        self.cursor_count = elements.len();
        elements.extend(layer);
        if live {
            self.shooter_elements(&mut elements);
            self.pane_elements(fade::Pane::Pair, &mut elements);
            self.pane_elements(fade::Pane::Share, &mut elements);
            self.pane_elements(fade::Pane::Prompt, &mut elements);
            self.pane_elements(fade::Pane::Pill, &mut elements);
            self.pane_elements(fade::Pane::Hud, &mut elements);
            self.card_elements(&mut elements);
            self.switch_elements(&mut elements);
        }
        self.layer_elements(
            &[smithay::wayland::shell::wlr_layer::Layer::Overlay],
            &mut elements,
        );

        if let Some(buffer) = self.glow.as_ref().filter(|_| live && self.glows()) {
            let reach = self.glow_key.0;
            let warmth = drifter_core::corner::heat(self.pointer, reach);
            if warmth > 0.0 && !self.modifier && self.held_by_fullscreen().is_none() {
                let rect = Rect::new(Vec2::ZERO, Vec2::splat(reach));
                elements.push(Skin::Texture(element(buffer, rect, (warmth * 0.55) as f32)));
            }
        }

        if live {
            self.snap_elements(&mut elements);
        }

        if live {
            self.pane_elements(fade::Pane::Search, &mut elements);
            self.alert_elements(&mut elements);
            self.desk_menu_elements(&mut elements);
            self.pane_elements(fade::Pane::DeskMenu, &mut elements);
            if self.shooter.is_none() && !self.shot_menu_open() {
                self.popover_elements(&mut elements);
                self.pane_elements(fade::Pane::Menu, &mut elements);
            }
            for pane in [
                fade::Pane::Launchpad,
                fade::Pane::Control,
                fade::Pane::Notes,
                fade::Pane::Charges,
                fade::Pane::Glance,
                fade::Pane::GlanceCard,
                fade::Pane::Tip,
            ] {
                self.pane_elements(pane, &mut elements);
            }
            self.welcome_elements(&mut elements);
        }

        if let Some(band) = self.dock_band() {
            self.dock_elements(&mut elements, band);
        }

        if let (Some(rect), Some((_, buffer, _))) = (self.app_label_rect(), self.app_label.as_ref()) {
            elements.push(Skin::Texture(element(buffer, rect, 1.0)));
        }
        if let (Some(_), Some((_, buffer, rect))) = (self.tray_band(), self.clock.as_ref()) {
            elements.push(Skin::Texture(element(buffer, *rect, 1.0)));
        }
        if let (Some(rect), Some((buffer, _))) = (self.battery_rect(), self.battery_art.as_ref()) {
            if let Some(glow) = self.battery_glow.as_ref() {
                let ebb = drifter_core::lock::ebb(self.ebb_from.elapsed().as_secs_f64());
                elements.push(Skin::Texture(element(glow, rect, ebb)));
            }
            elements.push(Skin::Texture(element(buffer, rect, 1.0)));
        }
        if let (Some(rect), Some(buffer)) = (self.control_rect(), self.control_icon.as_ref()) {
            elements.push(Skin::Texture(element(buffer, rect, 1.0)));
        }
        if self.tray_band().is_some() {
            self.blue_tray_elements(&mut elements);
            self.wifi_tray_elements(&mut elements);
        }
        if let (Some(band), Some(buffer)) = (self.tray_band(), self.power.as_ref()) {
            elements.push(Skin::Texture(element(
                buffer,
                self.tray.power_rect(band),
                1.0,
            )));
        }
        if let Some(band) = self.tray_band() {
            let room = self.tray_layout(band);
            if let (Some((buffer, _)), Some(spot)) = (self.stats.as_ref(), self.stats_spot()) {
                elements.push(Skin::Texture(element(buffer, spot, 1.0)));
            }
            let net = self.net_slot();
            for slot in net.into_iter().chain(self.tray_row().into_iter().take(room)) {
                elements.push(Skin::Texture(element(
                    &self.icons[slot].2,
                    self.tray_icon_rect(band, slot),
                    1.0,
                )));
            }
        }
        self.frost_bars();
        for edge in Edge::ALL {
            let drawer = &self.drawers[edge.index()];
            if drawer.visible() {
                if edge == Edge::Bottom && self.dock_panel_elements(&mut elements) {
                    continue;
                }
                let shape = &self.shapes[edge.index()];
                let rect = drawer.rect(self.screen, shape);
                elements.push(Skin::Texture(element(
                    &self.panels[edge.index()],
                    rect,
                    1.0,
                )));
                if let Some(drop) = self.drops.get(edge.index())
                    && edge == Edge::Bottom
                    && shape.span > 0.0
                {
                    elements.push(Skin::Texture(element(drop, drifter_core::drawer::wrap_rect(rect, shape), 1.0)));
                } else if let Some(drop) = self.drops.get(edge.index()) {
                    let band = drawer.band(self.screen, shape);
                    let spot = drifter_core::drawer::shadow_rect(band, edge, shape);
                    let size = Vec2::new(spot.size.x.max(1.0).floor(), spot.size.y);
                    elements.push(Skin::Texture(cropped(drop, Rect::new(Vec2::ZERO, size), spot)));
                }
            }
        }

        self.layer_elements(
            &[smithay::wayland::shell::wlr_layer::Layer::Top],
            &mut elements,
        );

        let showing: Vec<Showing> = self
            .windows
            .iter()
            .rev()
            .filter(|window| window.placed && !window.hidden && window.closed_at.is_none() && !self.in_genie(window))
            .map(|window| self.showing_of(window))
            .collect();
        self.genie_elements(&mut elements);
        self.ghost_elements(&mut elements);
        for item in showing {
            self.show_window(item, true, &mut elements);
        }

        self.layer_elements(
            &[
                smithay::wayland::shell::wlr_layer::Layer::Bottom,
                smithay::wayland::shell::wlr_layer::Layer::Background,
            ],
            &mut elements,
        );

        let fading = self
            .fading
            .as_ref()
            .map(|(_, since)| drifter_core::motion::smooth(since.elapsed().as_secs_f64() / PAPER_FADE.as_secs_f64()));
        let view = (self.mode, self.screen, self.world, self.camera);
        if let Some(ripple) = self.ripple.as_ref() {
            elements.push(Skin::Texture(element(&ripple.sheet.buffer, Rect::new(Vec2::ZERO, self.screen), 1.0)));
        }
        let clock = self.paper_clock();
        let every = self.live_every();
        let revealing = self.paper_reveal_elements(&mut elements);
        if let Some(paper) = self.wallpaper.as_mut() {
            let alpha = if revealing { 1.0 } else { fading.unwrap_or(1.0) as f32 };
            paper_pieces(&mut self.renderer, paper, view, alpha, (clock, Some(every)), &mut elements);
        }
        if let (Some((old, _)), Some(t), false) = (self.fading.as_mut(), fading, revealing)
            && t < 1.0
        {
            paper_pieces(&mut self.renderer, old, view, 1.0, (clock, None), &mut elements);
        }
        elements
    }

    fn cursor_elements(&mut self) -> Vec<Skin> {
        let mut skins = self.pointer_elements();
        skins.extend(self.drag_icon_elements());
        skins
    }

    fn drag_icon_elements(&mut self) -> Vec<Skin> {
        use smithay::reexports::wayland_server::Resource;
        let Some(icon) = self.drag_icon.clone() else {
            return Vec::new();
        };
        if !icon.is_alive() {
            self.drag_icon = None;
            return Vec::new();
        }
        let anchor = smithay::utils::Point::<i32, smithay::utils::Physical>::from((
            self.pointer.x.round() as i32,
            self.pointer.y.round() as i32,
        ));
        render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<Painter>>(
            &mut self.renderer,
            &icon,
            anchor,
            1.0,
            0.9,
            Kind::Cursor,
        )
        .into_iter()
        .map(|piece| Skin::Client(RescaleRenderElement::from_element(piece, anchor, 1.0)))
        .collect()
    }

    fn pointer_elements(&mut self) -> Vec<Skin> {
        use smithay::input::pointer::{CursorIcon, CursorImageStatus, CursorImageSurfaceData};
        use smithay::reexports::wayland_server::Resource;

        let arrow = || {
            vec![Skin::Texture(element(
                &self.arrow,
                cursor::rect(self.pointer, self.arrow_size, self.arrow_hotspot),
                1.0,
            ))]
        };
        match self.cursor_status.clone() {
            CursorImageStatus::Hidden => Vec::new(),
            CursorImageStatus::Named(CursorIcon::Default) => arrow(),
            CursorImageStatus::Named(icon) => {
                let name = icon.name();
                if !self.named_cursors.contains_key(name) {
                    let mut names = vec![name];
                    names.extend_from_slice(icon.alt_names());
                    let theme = self.appearance.cursor_theme.as_deref();
                    let size = self.appearance.cursor_size;
                    let dense = self.density;
                    let art = crate::cursors::load(&names, theme, size * dense).and_then(|(image, hotspot)| {
                        let scale = f64::from(dense);
                        let size = image.size() / scale;
                        upload_at(&mut self.renderer, Some(&image), "cursor", dense as i32)
                            .ok()
                            .flatten()
                            .map(|buffer| (buffer, size, hotspot / scale))
                    });
                    self.named_cursors.insert(name, art);
                }
                match self.named_cursors.get(name) {
                    Some(Some((buffer, size, hotspot))) => vec![Skin::Texture(element(
                        buffer,
                        Rect::new(self.pointer - *hotspot, *size),
                        1.0,
                    ))],
                    _ => arrow(),
                }
            }
            CursorImageStatus::Surface(surface) => {
                if !surface.is_alive() {
                    self.cursor_status = CursorImageStatus::default_named();
                    return arrow();
                }
                let hotspot = smithay::wayland::compositor::with_states(&surface, |states| {
                    states
                        .data_map
                        .get::<CursorImageSurfaceData>()
                        .map(|data| data.lock().unwrap().hotspot)
                        .unwrap_or_default()
                });
                let at = self.pointer - Vec2::new(f64::from(hotspot.x), f64::from(hotspot.y));
                let anchor = smithay::utils::Point::<i32, smithay::utils::Physical>::from((
                    at.x.round() as i32,
                    at.y.round() as i32,
                ));
                render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<Painter>>(
                    &mut self.renderer,
                    &surface,
                    anchor,
                    1.0,
                    1.0,
                    Kind::Cursor,
                )
                .into_iter()
                .map(|piece| Skin::Client(RescaleRenderElement::from_element(piece, anchor, 1.0)))
                .collect()
            }
        }
    }

    pub fn set_cursor(&mut self, status: smithay::input::pointer::CursorImageStatus) {
        self.client_cursor = status.clone();
        if self.grip_under().is_none() && self.thumb_cursor().is_none() {
            self.cursor_status = status;
        }
    }

    fn draw(&mut self) -> Result<bool, String> {
        if !self.active || self.dark || self.stepped {
            return Ok(false);
        }

        let now = Instant::now();
        let dt = now.duration_since(self.last);
        self.last = now;
        self.step_windows(dt);
        self.remember_surfaces();

        if self.hud.as_ref().is_some_and(|hud| now >= hud.until) {
            self.hud = None;
        }
        self.watch_panes(now);
        self.expire_cards(now);
        self.settle_welcome();
        self.wind_titles();
        self.paint_lock();
        if self.greeted_waiting() {
            return Ok(false);
        }
        self.check_client_lock();
        self.settle_style_fade();

        let greeting = self.greeted;
        let mut drew = false;
        for index in 0..self.heads.len() {
            if self.heads[index].awaiting || self.mirror_source(&self.heads[index]).is_some() {
                continue;
            }
            match self.with_head(index, |state| state.draw_head(now)) {
                Ok(shown) => drew |= shown,
                Err(error) if self.heads.len() == 1 || self.heads[index].frames == 0 => {
                    if self.heads.iter().all(|head| head.frames == 0) {
                        return Err(error);
                    }
                    log!("drifter: {error}");
                }
                Err(error) => log!("drifter: {error}"),
            }
        }

        self.restyle(now);
        self.place_sound();
        self.send_resize();
        if drew {
            self.speak();
            self.confirm_client_lock();
            self.tell_clients();
            if greeting {
                self.greeted_unlock_now();
            }
        } else {
            self.frames_later();
        }
        Ok(drew)
    }

    fn draw_head(&mut self, now: Instant) -> Result<bool, String> {
        let at = self.at;
        let dt = now.duration_since(self.heads[at].last);
        self.heads[at].last = now;
        self.pin_bars();
        for drawer in &mut self.drawers {
            drawer.advance(dt, self.tunables.drawer_anim);
        }
        self.steer();
        self.wind_the_clock();
        self.wind_the_app_label();
        self.wind_the_stats();
        self.wind_the_battery();
        self.fit_dock();
        self.wind_the_volume();
        self.wind_the_light();
        self.wind_the_control();
        self.wind_the_ripple();
        self.wind_the_thumbs();
        self.wind_the_flash();
        self.wind_the_desk_menu();
        self.settle_bounces();
        self.wind_the_dock();
        self.settle_fade();

        let scale = self.heads[at].scale;
        let mut pieces = self.elements();
        self.paint_reveal();
        self.style_fade_elements(&mut pieces);
        let shown: Vec<Shown> = pieces
            .into_iter()
            .map(|piece| RescaleRenderElement::from_element(piece, (0, 0).into(), scale))
            .collect();
        let clear = self.clear;
        let tearing = self.wants_tearing();
        let frames = self.heads[at].frames;
        let State {
            heads, renderer, ..
        } = self;
        let head = &mut heads[at];
        let empty = head
            .compositor
            .render_frame(renderer, &shown, clear, tearing)
            .map_err(|e| {
                if frames == 0 {
                    format!(
                        "first render on {} failed: {e}\n\
                         drifter needs drm master, which needs a real tty.\n\
                         run it from tty1, not over ssh. smithay only warns when master is denied.",
                        head.name
                    )
                } else {
                    format!("render on {} failed after {frames} frames: {e}", head.name)
                }
            })?;

        self.draw_mirrors(at, clear);
        self.serve_copies(&shown, !empty);
        self.shoot_old_style(&shown);
        self.sync_wlr_toplevels();
        if self.shot_round.as_ref().is_some_and(|round| round.left.contains(&self.heads[at].name)) {
            self.screenshot(&shown);
        }

        if empty {
            return Ok(false);
        }

        let head = &mut self.heads[at];
        head.compositor.queue_frame().map_err(|e| {
            format!(
                "page flip on {} failed after {frames} frames: {e}",
                head.name
            )
        })?;
        head.frames += 1;
        head.awaiting = true;
        self.gather_feedback(at);
        let head = &mut self.heads[at];
        if head.compositor.is_offscreen() {
            let name = head.name.clone();
            let _ = self.handle.insert_source(
                Timer::from_duration(Duration::from_millis(16)),
                move |_, _, state: &mut State| {
                    if let Some(at) = state.heads.iter().position(|head| head.name == name) {
                        state.heads[at].awaiting = false;
                        state.presented(at, None);
                    }
                    if let Err(error) = state.draw() {
                        state.stop(error);
                    }
                    TimeoutAction::Drop
                },
            );
        }
        Ok(true)
    }

    pub(super) fn send_resize(&mut self) {
        let Some(id) = self.resizing.take() else {
            return;
        };
        if let Some(window) = self.windows.iter().find(|window| window.id == id) {
            window.toplevel.send_pending_configure();
        }
    }

    fn tell_clients(&mut self) {
        let now = self.clock_ms();
        for root in self.client_roots() {
            crate::shell::send_frame(&root, now);
        }
        if let Some(display) = self.display.as_mut() {
            let _ = display.flush_clients();
        }
    }

    pub fn dispatch_clients(&mut self) {
        let Some(mut display) = self.display.take() else {
            return;
        };
        if let Err(error) = display.dispatch_clients(self) {
            log!("drifter: wayland dispatch failed: {error}");
        }
        let _ = display.flush_clients();
        self.display = Some(display);
    }

    pub fn kick_now(&mut self) {
        self.kick();
    }

    fn background<F>(&self, name: &str, work: impl FnOnce() -> F + Send + 'static)
    where
        F: FnOnce(&mut State) + Send + 'static,
    {
        let chores = self.chores.clone();
        let spawned = std::thread::Builder::new().name(name.to_string()).spawn(move || {
            let done = work();
            let _ = chores.send(Box::new(done));
        });
        if let Err(error) = spawned {
            log!("drifter: cannot start {name}: {error}");
        }
    }

    fn kick(&mut self) {
        if self.heads.iter().any(|head| !head.awaiting)
            && let Err(error) = self.draw()
        {
            self.stop(error);
            return;
        }
        self.tick();
    }

    fn tick(&mut self) {
        if self.ticking {
            return;
        }
        let Some(wait) = self.pending() else {
            return;
        };
        let armed = self
            .handle
            .insert_source(Timer::from_duration(wait), |_, _, state: &mut State| {
                state.ticking = false;
                state.settle();
                match state.pending() {
                    Some(wait) => {
                        state.ticking = true;
                        TimeoutAction::ToDuration(wait)
                    }
                    None => TimeoutAction::Drop,
                }
            })
            .is_ok();
        self.ticking = armed;
    }

    fn hover(&mut self) {
        self.aim_shooter();
        self.watch_corner();
        self.hover_desk_menu();
        self.hover_welcome();
        self.hover_rows();
        self.stir_paper();
        if self.hover_since.is_none() {
            self.hover_since = Some((self.pointer, Instant::now()));
        } else if let Some((at, _)) = self.hover_since
            && (self.pointer - at).length() > 3.0
        {
            self.hover_since = Some((self.pointer, Instant::now()));
            self.tip = None;
            self.tip_of = None;
            self.tip_want = None;
        }
    }

    fn settle(&mut self) {
        let now = Instant::now();
        if self.modifier {
            self.dwell = None;
        }
        if let Some((edge, since)) = self.dwell
            && now.duration_since(since) >= self.tunables.drawer_dwell
        {
            self.dwell = None;
            self.set(edge, true);
        }
        if let Some(since) = self.leave
            && now.duration_since(since) >= self.tunables.drawer_hide_delay
        {
            self.leave = None;
            for edge in Edge::ALL {
                if self.tunables.bar_hides[edge.index()] {
                    self.set(edge, false);
                }
            }
        }
        self.settle_tip();
        self.watch_corner();
        if self.heads.iter().any(|head| !head.awaiting)
            && let Err(error) = self.draw()
        {
            self.stop(error);
        }
    }

    fn set(&mut self, edge: Edge, open: bool) {
        self.drawers[edge.index()].open = open;
    }

    fn hiding_open(&self) -> bool {
        Edge::ALL
            .iter()
            .any(|edge| self.tunables.bar_hides[edge.index()] && self.drawers[edge.index()].open)
    }

    fn pin_bars(&mut self) {
        let shown = self.lock.is_none() && self.held_by_fullscreen().is_none();
        for edge in Edge::ALL {
            if !self.tunables.bar_hides[edge.index()] {
                self.set(edge, shown);
            }
        }
    }

    fn toggle(&mut self, edge: Edge) {
        if self.lock.is_some() {
            return;
        }
        let open = self.drawers[edge.index()].open;
        self.dwell = None;
        self.leave = None;
        self.set(edge, !open);
    }

    fn wanted(&self) -> Option<Edge> {
        if self.held_by_fullscreen().is_some() {
            return None;
        }
        if self.dock_drag.is_some() {
            return Some(Edge::Bottom);
        }
        if self.control.is_some() || self.notes_panel.is_some() || self.glance_panel.is_some() || self.charges.is_some() {
            return Some(Edge::Top);
        }
        for edge in Edge::ALL {
            if self.drawers[edge.index()].holds(
                self.pointer,
                self.screen,
                &self.shapes[edge.index()],
            ) {
                return Some(edge);
            }
        }
        drawer::edge_at(
            self.pointer,
            self.screen,
            self.tunables.edge_band,
            self.tunables.hot_corner_size,
        )
    }

    fn stop(&mut self, error: String) {
        self.failure = Some(error);
        self.signal.stop();
    }
}

impl Screen {
    pub fn found(&self) -> &Found {
        &self.found
    }

    pub fn run(self) -> Result<(), String> {
        let Screen {
            mut events,
            mut state,
            ..
        } = self;

        if state.greeter.is_some() {
            if !state.lock_now() {
                return Err("the login screen cannot start".to_string());
            }
            state.draw()?;
            events
                .run(None, &mut state, |_| {})
                .map_err(|e| format!("event loop failed: {e}"))?;
            return state.failure.take().map_or(Ok(()), Err);
        }
        if start::greeted() {
            state.greeted = true;
            match state.lock_now() {
                true => {
                    if let Some(lock) = state.lock.as_mut() {
                        lock.since -= LOCK_FADE;
                        lock.note = Note::Checking;
                    }
                }
                false => state.greeted = false,
            }
        } else if lock_marker().is_some_and(|marker| marker.exists())
            || (state.login_lock && crate::logind::from_console())
        {
            state.lock_now();
        }
        state.draw()?;
        crate::mark("first frame");
        if state.welcome_wanted {
            state.open_welcome(true);
        }

        for item in state.login.clone() {
            crate::apps::run(&item, &state.socket);
        }
        state.reopen_desk();
        state.start_bluetooth();
        state.start_wifi();
        state.start_airdrop();
        if let Some(media) = state.media.as_ref() {
            media.read();
        }
        if state.autostart {
            let found = drifter_core::autostart::installed();
            for entry in state.wifi_autostart(drifter_core::autostart::to_start(&found, &state.login)) {
                match entry.argv() {
                    Some(argv) => crate::apps::launch(&argv, &state.socket),
                    None => log!("drifter: cannot read the autostart line {:?}", entry.exec),
                }
            }
        }
        if let (Some(error), Some(id)) = (
            crate::CONFIG_ERROR.get(),
            state.notes_bus.as_ref().map(|bus| bus.next_id()),
        ) {
            state.show_note(drifter_core::notify::Note {
                id,
                app: "Drifter".to_string(),
                summary: "The config has a mistake".to_string(),
                body: format!("{error}. Drifter uses its defaults until it is fixed."),
                actions: Vec::new(),
                expire: None,
                asked: 0,
                critical: true,
                icon: None,
            });
        }
        if std::env::var_os("DRIFTER_RECOVERED").is_some()
            && let Some(id) = state.notes_bus.as_ref().map(|bus| bus.next_id())
        {
            let log = Config::state_dir().join("crash.log");
            state.show_note(drifter_core::notify::Note {
                id,
                app: "Drifter".to_string(),
                summary: "Drifter restarted after a crash".to_string(),
                body: format!("What happened is in {}", log.display()),
                actions: Vec::new(),
                expire: Some(10_000),
                asked: 10_000,
                critical: false,
                icon: None,
            });
        }

        events
            .run(None, &mut state, |_| {})
            .map_err(|e| format!("event loop failed: {e}"))?;

        match state.failure {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

fn window_pace(config: &Config) -> f64 {
    config.motion.animation.pace()
}

fn dock_margin(config: &Config) -> f64 {
    match config.dock.fit {
        true => config.dock_padding() + drifter_core::drawer::Panel::default().radius,
        false => drifter_core::dock::Dock::themed(&drifter_core::theme::DARK).margin,
    }
}

fn dense(side: u32) -> u32 {
    drifter_core::paint::dense(f64::from(side))
}

fn upload(
    renderer: &mut Painter,
    image: Option<&Image>,
    what: &str,
) -> Result<Option<TextureBuffer<Tex>>, String> {
    let scale = match image {
        Some(image) if image.width > 1 || image.height > 1 => drifter_core::paint::density() as i32,
        _ => 1,
    };
    upload_at(renderer, image, what, scale)
}

fn upload_flat(
    renderer: &mut Painter,
    image: Option<&Image>,
    what: &str,
) -> Result<Option<TextureBuffer<Tex>>, String> {
    upload_at(renderer, image, what, 1)
}

fn upload_at(
    renderer: &mut Painter,
    image: Option<&Image>,
    what: &str,
    scale: i32,
) -> Result<Option<TextureBuffer<Tex>>, String> {
    match image {
        None => Ok(None),
        Some(image) => TextureBuffer::from_memory(
            renderer,
            &image.rgba,
            DrmFourcc::Abgr8888,
            (image.width as i32, image.height as i32),
            false,
            scale,
            Transform::Normal,
            None,
        )
        .map(Some)
        .map_err(|e| format!("cannot upload {what}: {e}")),
    }
}

const APPS_FRESH: Duration = Duration::from_secs(10);

impl State {
    fn installed_apps(&mut self) -> std::sync::Arc<Vec<drifter_core::desktop::App>> {
        let (apps, at) = self
            .installed
            .get_or_insert_with(|| (std::sync::Arc::new(drifter_core::desktop::installed()), Instant::now()))
            .clone();
        if at.elapsed() > APPS_FRESH {
            self.installed = Some((apps.clone(), Instant::now()));
            self.refresh_apps();
        }
        apps
    }

    fn refresh_apps(&self) {
        self.background("apps", || {
            let apps = std::sync::Arc::new(drifter_core::desktop::installed());
            move |state: &mut State| {
                state.installed = Some((apps, Instant::now()));
                state.kick_now();
            }
        });
    }

    fn apps_changed(&mut self) {
        self.background("apps-watch", || {
            let apps = std::sync::Arc::new(drifter_core::desktop::installed());
            let found = drifter_core::search::apps(&apps);
            let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
            let files = drifter_core::search::files(&home);
            move |state: &mut State| {
                state.installed = Some((apps, Instant::now()));
                state.relist_launchpad(&found);
                state.reindex_search(found, files);
                state.kick_now();
            }
        });
    }

    fn upload_again(&mut self, image: Option<&Image>, what: &'static str) -> Result<Option<TextureBuffer<Tex>>, String> {
        let Some(image) = image else {
            return Ok(None);
        };
        let size = (image.width as i32, image.height as i32);
        let kept = self.reused.get(what).filter(|tex| {
            let had = smithay::backend::renderer::Texture::size(*tex);
            (had.w, had.h) == size
        });
        if let Some(tex) = kept.cloned() {
            let region = smithay::utils::Rectangle::from_size(size.into());
            match smithay::backend::renderer::ImportMem::update_memory(&mut self.renderer, &tex, &image.rgba, region) {
                Ok(()) => {
                    let scale = drifter_core::paint::density() as i32;
                    return Ok(Some(TextureBuffer::from_texture(&self.renderer, tex, scale, Transform::Normal, None)));
                }
                Err(error) => log!("drifter: cannot update {what}: {error}"),
            }
        }
        let tex = smithay::backend::renderer::ImportMem::import_memory(
            &mut self.renderer,
            &image.rgba,
            DrmFourcc::Abgr8888,
            size.into(),
            false,
        )
        .map_err(|e| format!("cannot upload {what}: {e}"))?;
        self.reused.insert(what, tex.clone());
        let scale = drifter_core::paint::density() as i32;
        Ok(Some(TextureBuffer::from_texture(&self.renderer, tex, scale, Transform::Normal, None)))
    }
}

type Pinned = Vec<(drifter_core::Launch, Option<TextureBuffer<Tex>>)>;

fn paper_pieces(
    renderer: &mut Painter,
    paper: &mut Backdrop,
    (mode, screen, world, camera): (drifter_core::WallpaperMode, Vec2, Vec2, Camera),
    alpha: f32,
    (clock, every): (f64, Option<Duration>),
    elements: &mut Vec<Skin>,
) {
    let whole = paper.paper(screen);
    let near = wallpaper::live_pointer(paper, (mode, screen, world, camera), whole).filter(|_| every.is_some());
    if let Some((buffer, size)) = paper
        .live
        .as_mut()
        .and_then(|live| live.show(renderer, whole, clock, every, near))
    {
        let seen = drifter_core::wallpaper::place(mode, whole, screen, world, &camera)
            .seen(screen)
            .onto(whole, size);
        elements.push(Skin::Texture(faded(&buffer, seen.source, seen.target, alpha)));
        return;
    }
    let laid = paper.laid(mode, screen, world, &camera);
    let now = Instant::now();
    for piece in paper.pyramid.pieces(laid.source, laid.target) {
        if !paper.resident.contains_key(&piece.key) {
            let image = paper.pyramid.tile(piece.key);
            match upload_flat(renderer, Some(&image), "wallpaper tile") {
                Ok(Some(buffer)) => {
                    paper.resident.insert(piece.key, (buffer, now));
                }
                Ok(None) => continue,
                Err(error) => {
                    log!("drifter: {error}");
                    continue;
                }
            }
        }
        if let Some((buffer, used)) = paper.resident.get_mut(&piece.key) {
            *used = now;
            elements.push(Skin::Texture(faded(buffer, piece.source, piece.target, alpha)));
        }
    }
}

fn element(buffer: &TextureBuffer<Tex>, rect: Rect, alpha: f32) -> TextureRenderElement<Tex> {
    let origin = rect.origin.round();
    let far = (rect.origin + rect.size).round();
    let size = far - origin;
    TextureRenderElement::from_texture_buffer(
        (origin.x, origin.y),
        buffer,
        Some(alpha),
        None,
        Some((size.x as i32, size.y as i32).into()),
        Kind::Unspecified,
    )
}

fn fill(pixel: &TextureBuffer<Tex>, rect: Rect, alpha: f32) -> TextureRenderElement<Tex> {
    let origin = rect.origin.round();
    let size = (rect.origin + rect.size).round() - origin;
    TextureRenderElement::from_texture_buffer(
        (origin.x, origin.y),
        pixel,
        Some(alpha),
        Some(smithay::utils::Rectangle::from_size((1.0, 1.0).into())),
        Some((size.x as i32, size.y as i32).into()),
        Kind::Unspecified,
    )
}

const TILE_SPAN: u32 = 8192;

type SkinSet = std::collections::HashMap<String, (TextureBuffer<Tex>, Vec2)>;

type DockArt = (
    Option<TextureBuffer<Tex>>,
    Option<TextureBuffer<Tex>>,
    Option<drifter_core::Image>,
);

fn cropped(buffer: &TextureBuffer<Tex>, source: Rect, target: Rect) -> TextureRenderElement<Tex> {
    faded(buffer, source, target, 1.0)
}

fn faded(
    buffer: &TextureBuffer<Tex>,
    source: Rect,
    target: Rect,
    alpha: f32,
) -> TextureRenderElement<Tex> {
    let origin = target.origin.round();
    let far = (target.origin + target.size).round();
    let size = far - origin;
    TextureRenderElement::from_texture_buffer(
        (origin.x, origin.y),
        buffer,
        Some(alpha),
        Some(smithay::utils::Rectangle::new(
            (source.origin.x, source.origin.y).into(),
            (source.size.x, source.size.y).into(),
        )),
        Some((size.x as i32, size.y as i32).into()),
        Kind::Unspecified,
    )
}
