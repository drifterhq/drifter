use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

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
use smithay::backend::drm::compositor::{DrmCompositor, FrameFlags};
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::input::{
    AbsolutePositionEvent, ButtonState, Event as InputMeta, InputEvent, KeyState, KeyboardKeyEvent,
    PointerButtonEvent, PointerMotionEvent,
};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::Color32F;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::surface::{
    WaylandSurfaceRenderElement, render_elements_from_surface_tree,
};
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::gles::GlesTexture;
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

const ESCAPE: u32 = 9;
const KEY_L: u32 = 46;
const KEY_TAB: u32 = 23;
const KEY_F4: u32 = 70;
const KEY_PRINT: u32 = 107;
const LOCK_PATIENCE: Duration = Duration::from_secs(30);
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;
const PANEL_COLOR: [u8; 3] = [16, 18, 26];
const PANEL_ALPHA: u8 = 235;
const TICK: Duration = Duration::from_millis(8);
const CONFIG_WATCH: Duration = Duration::from_millis(400);
const HUD_TIME: Duration = Duration::from_millis(1500);
const SETTINGS_APP: &str = "drifter-settings";
const SECOND_TICK: Duration = Duration::from_millis(250);
const SETTLING: Duration = Duration::from_secs(2);
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const PUSH_BAND: f64 = 8.0;
const DETAIL_ZOOM: f64 = 0.75;

use smithay::backend::renderer::{ImportAll, RendererSuper, element::render_elements};

use smithay::backend::renderer::element::utils::RescaleRenderElement;

render_elements! {
    pub Piece<R> where R: ImportAll;
    Texture=TextureRenderElement<<R as RendererSuper>::TextureId>,
    Client=RescaleRenderElement<WaylandSurfaceRenderElement<R>>,
}

type Skin = Piece<GlesRenderer>;

type ThemeCursor = (TextureBuffer<GlesTexture>, Vec2, Vec2);

type Showing = (
    Rect,
    Rect,
    f32,
    Vec2,
    (bool, bool),
    bool,
    String,
    smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
);

type Compositor =
    DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;

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

pub struct Screen {
    found: Found,
    events: EventLoop<'static, State>,
    state: State,
}

enum Target {
    Tray(String),
    Window(u64),
    Desktop,
}

struct Switch {
    order: Vec<u64>,
    at: usize,
    layout: drifter_core::switcher::Layout,
    art: Option<TextureBuffer<GlesTexture>>,
    icons: Vec<Option<TextureBuffer<GlesTexture>>>,
}

enum Shortcut {
    Switch { back: bool },
    Close,
    Media(crate::media::Key),
    Shot,
    Swallow,
}

struct Paper {
    set: Option<PathBuf>,
    shown: Option<PathBuf>,
    folder: Option<PathBuf>,
    every: u64,
    order: drifter_core::WallpaperOrder,
    next: Option<Instant>,
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
        }
    }
}

struct Card {
    note: drifter_core::notify::Note,
    art: TextureBuffer<GlesTexture>,
    tall: f64,
    until: Option<Instant>,
}

struct Pending {
    request: crate::polkit::Request,
    pad: Pad,
    error: Option<String>,
    busy: bool,
}

struct Popover {
    items: Vec<Item>,
    rect: Rect,
    art: TextureBuffer<GlesTexture>,
    target: Target,
    hover: Option<usize>,
}

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

struct Lock {
    pad: Pad,
    note: Note,
    checking: Option<Instant>,
    ticket: Option<u64>,
    art: Option<TextureBuffer<GlesTexture>>,
    rect: Rect,
    shown: (String, String, usize, Note),
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

enum LockKey {
    Letter(char),
    Enter,
    Back,
    Clear,
    Other,
}

pub struct State {
    pub output: smithay::output::Output,
    pub wayland: crate::shell::Wayland,
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
    holds: drifter_core::Modifier,
    last_title: Option<(Vec2, Instant)>,
    held: Option<usize>,
    focus: Option<u64>,
    next_window: u64,
    eaten: Vec<u32>,
    frame_timer: bool,
    cursor_status: smithay::input::pointer::CursorImageStatus,
    named_cursors: std::collections::HashMap<&'static str, Option<ThemeCursor>>,
    layer_focus: Option<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface>,
    skin: std::collections::HashMap<String, (TextureBuffer<GlesTexture>, Vec2)>,
    started: Instant,
    socket: String,
    drm: DrmDevice,
    libinput: Libinput,
    compositor: Compositor,
    renderer: GlesRenderer,
    wallpaper: Option<(TextureBuffer<GlesTexture>, Vec2)>,
    panels: [TextureBuffer<GlesTexture>; 2],
    shapes: [Panel; 2],
    arrow: TextureBuffer<GlesTexture>,
    arrow_size: Vec2,
    arrow_hotspot: Vec2,
    font: Option<Font>,
    tray: Tray,
    dock: Dock,
    pinned: Pinned,
    pinned_from: (drifter_core::DockConfig, drifter_core::AppsConfig),
    apps_chosen: drifter_core::AppsConfig,
    dock_spare: Option<drifter_core::Image>,
    trash: Option<TextureBuffer<GlesTexture>>,
    cog: Option<TextureBuffer<GlesTexture>>,
    dot: Option<TextureBuffer<GlesTexture>>,
    glow: Option<TextureBuffer<GlesTexture>>,
    rule: Option<TextureBuffer<GlesTexture>>,
    app_icons: std::collections::HashMap<String, TextureBuffer<GlesTexture>>,
    no_icon: std::collections::HashSet<String>,
    format: String,
    clock: Option<(String, TextureBuffer<GlesTexture>, Rect)>,
    stats: Option<(TextureBuffer<GlesTexture>, Vec2)>,
    sampler: crate::stats::Sampler,
    sampled: Option<Instant>,
    icons: Vec<(String, String, TextureBuffer<GlesTexture>)>,
    popover: Option<Popover>,
    switch: Option<Switch>,
    lock: Option<Lock>,
    lock_big: Option<Font>,
    lock_small: Option<Font>,
    verify: smithay::reexports::calloop::channel::Sender<(u64, crate::auth::Verdict)>,
    tickets: u64,
    power: Option<TextureBuffer<GlesTexture>>,
    look: Look,
    tip: Option<(TextureBuffer<GlesTexture>, Rect)>,
    tip_of: Option<String>,
    snap: Snap,
    config_seen: Option<std::time::SystemTime>,
    last_input: Instant,
    lock_after: u64,
    login: Vec<drifter_core::Launch>,
    paper: Paper,
    paper_tx: smithay::reexports::calloop::channel::Sender<(PathBuf, drifter_core::Image)>,
    config_path: PathBuf,
    zone: Option<Zone>,
    plain: Option<TextureBuffer<GlesTexture>>,
    titles: std::collections::HashMap<String, (TextureBuffer<GlesTexture>, Vec2)>,
    hover_since: Option<(Vec2, Instant)>,
    tray_bus: Option<crate::tray::Tray>,
    media: Option<crate::media::Media>,
    notes_bus: Option<crate::notes::Notes>,
    copies: Vec<crate::screencopy::Copy>,
    prompts: Vec<Pending>,
    prompt_art: Option<(TextureBuffer<GlesTexture>, Rect)>,
    checked: smithay::reexports::calloop::channel::Sender<(String, crate::polkit::Outcome)>,
    copied: std::collections::HashSet<smithay::reexports::wayland_server::backend::ClientId>,
    cursor_count: usize,
    shot_wanted: bool,
    cards: Vec<Card>,
    hud: Option<(TextureBuffer<GlesTexture>, Rect, Instant)>,
    drawers: [Drawer; 2],
    tunables: Tunables,
    screen: Vec2,
    pointer: Vec2,
    dwell: Option<(Edge, Instant)>,
    leave: Option<Instant>,
    last: Instant,
    clear: Color32F,
    active: bool,
    awaiting: bool,
    ticking: bool,
    handle: LoopHandle<'static, State>,
    signal: LoopSignal,
    failure: Option<String>,
    frames: u32,
}

impl State {
    fn on_screen(&self, world: Rect) -> Rect {
        let origin = self.camera.screen_from_world(world.origin, self.screen);
        Rect::new(origin, world.size * self.camera.zoom)
    }

    fn in_world(&self, screen: Vec2) -> Vec2 {
        self.camera.world_from_screen(screen, self.screen)
    }

    fn fly_to(&mut self, target: Camera) {
        let mut target = target;
        target.clamp_to_world(self.world, self.screen);
        self.flight = Some((
            Flight::new(self.camera, target, self.tunables.camera_anim),
            Instant::now(),
        ));
    }

    fn steer(&mut self) -> bool {
        let Some((flight, since)) = self.flight else {
            return false;
        };
        let gone = Instant::now().duration_since(since);
        self.camera = flight.within(gone, self.world, self.screen);
        if flight.done(gone) {
            self.camera = flight.to;
            self.flight = None;
            return false;
        }
        true
    }

    fn overview(&self) -> bool {
        self.camera.zoom < Camera::WORK_ZOOM - 1e-6
    }

    fn animating(&self) -> bool {
        self.drawers.iter().any(|drawer| !drawer.settled())
            || self.flight.is_some()
            || self.windows.iter().any(|window| window.motion.is_some())
    }

    pub fn opening_motion(&self) -> Motion {
        Motion::new(Phase::Opening, self.dock_slot(0))
    }

    fn dock_slot(&self, index: usize) -> Rect {
        let panel = self.shapes[Edge::Bottom.index()];
        let mut open = drifter_core::Drawer::new(Edge::Bottom);
        open.open = true;
        open.advance(Duration::from_secs(1), Duration::from_millis(1));
        let band = open.band(self.screen, &panel);
        let hidden = self.minimized().len();
        let seat = self.dock.slot_rect(
            band,
            self.pinned.len(),
            hidden.max(1),
            Slot::Minimized(index),
        );
        Rect::new(self.in_world(seat.origin), seat.size / self.camera.zoom)
    }

    fn step_windows(&mut self, dt: Duration) {
        let mut finished: Vec<usize> = Vec::new();
        for (index, window) in self.windows.iter_mut().enumerate() {
            let Some(motion) = window.motion.as_mut() else {
                continue;
            };
            motion.advance(dt);
            if motion.done() {
                finished.push(index);
            }
        }

        for index in finished.into_iter().rev() {
            let Some(window) = self.windows.get_mut(index) else {
                continue;
            };
            let phase = window.motion.take().map(|motion| motion.phase);
            match phase {
                Some(Phase::Minimizing) => {
                    window.hidden = true;
                    self.remember_icon(index);
                    self.wake_windows();
                }
                Some(Phase::Closing) => {
                    window.toplevel.send_close();
                    window.going = false;
                }
                _ => {}
            }
        }
    }

    fn pending(&self) -> Option<Duration> {
        if self.lock.is_some() {
            return Some(SECOND_TICK);
        }
        if let Some((_, _, until)) = self.hud.as_ref() {
            return Some(until.saturating_duration_since(Instant::now()).max(TICK));
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
        if drifter_core::corner::heat(self.pointer, drifter_core::corner::REACH) > 0.0 {
            return Some(TICK);
        }
        if self.drawers[Edge::Top.index()].visible() && self.font.is_some() {
            return Some(SECOND_TICK);
        }
        None
    }

    fn elements(&mut self) -> Vec<Skin> {
        if self.lock.is_some() {
            return self.lock_elements();
        }
        let mut elements: Vec<Skin> = self.cursor_elements();
        self.cursor_count = elements.len();
        if let Some((art, rect)) = self.prompt_art.as_ref() {
            elements.push(Skin::Texture(element(art, *rect, 1.0)));
        }
        if let Some((art, rect, _)) = self.hud.as_ref() {
            elements.push(Skin::Texture(element(art, *rect, 1.0)));
        }
        for (card, rect) in self.cards.iter().zip(self.card_rects()) {
            elements.push(Skin::Texture(element(&card.art, rect, 1.0)));
        }
        self.switch_elements(&mut elements);
        self.layer_elements(
            &[smithay::wayland::shell::wlr_layer::Layer::Overlay],
            &mut elements,
        );

        if let Some(buffer) = self.glow.as_ref() {
            let reach = drifter_core::corner::REACH;
            let warmth = drifter_core::corner::heat(self.pointer, reach);
            if warmth > 0.0 && !self.modifier && self.held_by_fullscreen().is_none() {
                let rect = Rect::new(Vec2::ZERO, Vec2::splat(reach));
                elements.push(Skin::Texture(element(buffer, rect, (warmth * 0.55) as f32)));
            }
        }

        if let Some(zone) = self.zone
            && let Some(buffer) = self.plain.as_ref()
        {
            let view = self.camera.visible_rect(self.screen);
            let target = self.on_screen(self.snap.rect(zone, view));
            let thick = 3.0;
            let bars = [
                Rect::new(target.origin, Vec2::new(target.size.x, thick)),
                Rect::new(
                    Vec2::new(target.origin.x, target.max().y - thick),
                    Vec2::new(target.size.x, thick),
                ),
                Rect::new(target.origin, Vec2::new(thick, target.size.y)),
                Rect::new(
                    Vec2::new(target.max().x - thick, target.origin.y),
                    Vec2::new(thick, target.size.y),
                ),
            ];
            for bar in bars {
                elements.push(Skin::Texture(element(buffer, bar, 0.85)));
            }
            elements.push(Skin::Texture(element(buffer, target, 0.12)));
        }

        if let Some(open) = self.popover.as_ref() {
            elements.push(Skin::Texture(element(&open.art, open.rect, 1.0)));
        }
        if let Some((art, rect)) = self.tip.as_ref() {
            elements.push(Skin::Texture(element(art, *rect, 1.0)));
        }

        if let Some(band) = self.dock_band() {
            let hidden = self.minimized();
            let pinned = self.pinned.len();
            for (index, (_, buffer)) in self.pinned.iter().enumerate() {
                if let Some(buffer) = buffer {
                    let rect = self
                        .dock
                        .slot_rect(band, pinned, hidden.len(), Slot::Pinned(index));
                    elements.push(Skin::Texture(element(buffer, rect, 1.0)));
                }
            }
            for (index, window) in hidden.iter().enumerate() {
                if let Some(buffer) = self.app_icons.get(window.as_str()) {
                    let rect =
                        self.dock
                            .slot_rect(band, pinned, hidden.len(), Slot::Minimized(index));
                    elements.push(Skin::Texture(element(buffer, rect, 1.0)));
                }
            }
            if let Some(buffer) = self.dot.as_ref() {
                for (index, (item, _)) in self.pinned.iter().enumerate() {
                    let name = item.name();
                    let running = self
                        .windows
                        .iter()
                        .any(|window| window.placed && window.app.as_str() == name.as_str());
                    if running {
                        let rect =
                            self.dock
                                .dot_rect(band, pinned, hidden.len(), Slot::Pinned(index));
                        elements.push(Skin::Texture(element(buffer, rect, 0.9)));
                    }
                }
            }
            if let (true, Some(buffer)) = (self.dock.settings, self.cog.as_ref()) {
                let rect = self
                    .dock
                    .slot_rect(band, pinned, hidden.len(), Slot::Settings);
                elements.push(Skin::Texture(element(buffer, rect, 1.0)));
            }
            if let (true, Some(buffer)) = (self.dock.trash, self.trash.as_ref()) {
                let rect = self.dock.slot_rect(band, pinned, hidden.len(), Slot::Trash);
                elements.push(Skin::Texture(element(buffer, rect, 1.0)));
            }
            if let Some(buffer) = self.rule.as_ref() {
                for rule in self.dock.dividers(band, pinned, hidden.len()) {
                    elements.push(Skin::Texture(element(buffer, rule, 1.0)));
                }
            }
        }

        if let Some((_, buffer, rect)) = self.clock.as_ref() {
            elements.push(Skin::Texture(element(buffer, *rect, 1.0)));
        }
        if let (Some(band), Some(buffer)) = (self.tray_band(), self.power.as_ref()) {
            elements.push(Skin::Texture(element(
                buffer,
                self.tray.power_rect(band),
                1.0,
            )));
        }
        if let Some(band) = self.tray_band() {
            let (room, stats) = self.tray_layout(band);
            if let (true, Some((buffer, size))) = (stats, self.stats.as_ref()) {
                elements.push(Skin::Texture(element(
                    buffer,
                    self.tray.stats_rect(band, room, *size),
                    1.0,
                )));
            }
            for (slot, (_, _, buffer)) in self.icons.iter().take(room).enumerate() {
                elements.push(Skin::Texture(element(
                    buffer,
                    self.tray.icon_rect(band, slot),
                    1.0,
                )));
            }
        }
        for edge in Edge::ALL {
            let drawer = &self.drawers[edge.index()];
            if drawer.visible() {
                let rect = drawer.rect(self.screen, &self.shapes[edge.index()]);
                elements.push(Skin::Texture(element(
                    &self.panels[edge.index()],
                    rect,
                    1.0,
                )));
            }
        }

        self.layer_elements(
            &[smithay::wayland::shell::wlr_layer::Layer::Top],
            &mut elements,
        );

        let zoom = self.camera.zoom;
        let showing: Vec<Showing> = self
            .windows
            .iter()
            .rev()
            .filter(|window| window.placed && !window.hidden)
            .map(|window| {
                let (shown, fade) = window
                    .motion
                    .map(|motion| motion.shape(window.content))
                    .unwrap_or((window.content, 1.0));
                (
                    window.content,
                    shown,
                    fade,
                    window.inset,
                    (self.focus == Some(window.id), window.restore.is_some()),
                    window.decorated && window.full.is_none(),
                    window.title.clone(),
                    window.toplevel.wl_surface().clone(),
                )
            })
            .collect();

        for (content, shown, fade, inset, (awake, maximized), dressed, title, surface) in showing {
            let stretch = match content.size.x > 0.0 {
                true => shown.size.x / content.size.x,
                false => 1.0,
            };
            let at = self
                .camera
                .screen_from_world(shown.origin - inset * stretch, self.screen);
            let anchor = smithay::utils::Point::<i32, smithay::utils::Physical>::from((
                at.x.round() as i32,
                at.y.round() as i32,
            ));
            let client: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                render_elements_from_surface_tree(
                    &mut self.renderer,
                    &surface,
                    anchor,
                    1.0,
                    fade,
                    Kind::Unspecified,
                );
            if shown == content {
                for (origin, popup) in self.popups_of(&surface, content) {
                    let at = self.camera.screen_from_world(origin, self.screen);
                    let anchor = smithay::utils::Point::<i32, smithay::utils::Physical>::from((
                        at.x.round() as i32,
                        at.y.round() as i32,
                    ));
                    let bare: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                        render_elements_from_surface_tree(
                            &mut self.renderer,
                            &popup,
                            anchor,
                            1.0,
                            fade,
                            Kind::Unspecified,
                        );
                    elements.extend(bare.into_iter().map(|piece| {
                        Skin::Client(RescaleRenderElement::from_element(piece, anchor, zoom))
                    }));
                }
            }
            if dressed {
                self.dress(shown, (awake, maximized), &title, fade, &mut elements);
            }
            elements.extend(client.into_iter().map(|piece| {
                Skin::Client(RescaleRenderElement::from_element(
                    piece,
                    anchor,
                    zoom * stretch,
                ))
            }));
        }

        self.layer_elements(
            &[
                smithay::wayland::shell::wlr_layer::Layer::Bottom,
                smithay::wayland::shell::wlr_layer::Layer::Background,
            ],
            &mut elements,
        );

        if let Some((buffer, size)) = self.wallpaper.as_ref() {
            let laid = drifter_core::wallpaper::place(
                self.mode,
                *size,
                self.screen,
                self.world,
                &self.camera,
            );
            elements.push(Skin::Texture(cropped(buffer, laid.source, laid.target)));
        }
        elements
    }

    fn popup_places(
        root: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        content: Rect,
    ) -> Vec<(
        Vec2,
        Rect,
        smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    )> {
        smithay::desktop::PopupManager::popups_for_surface(root)
            .map(|(popup, place)| {
                let geometry = popup.geometry();
                let at = content.origin + Vec2::new(f64::from(place.x), f64::from(place.y));
                let origin = at - Vec2::new(f64::from(geometry.loc.x), f64::from(geometry.loc.y));
                let size = match geometry.size.w > 0 && geometry.size.h > 0 {
                    true => Vec2::new(f64::from(geometry.size.w), f64::from(geometry.size.h)),
                    false => smithay::backend::renderer::utils::with_renderer_surface_state(
                        popup.wl_surface(),
                        |state| state.surface_size(),
                    )
                    .flatten()
                    .map(|size| Vec2::new(f64::from(size.w), f64::from(size.h)))
                    .unwrap_or(Vec2::ZERO),
                };
                (origin, Rect::new(at, size), popup.wl_surface().clone())
            })
            .collect()
    }

    fn popups_of(
        &self,
        root: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        content: Rect,
    ) -> Vec<(
        Vec2,
        smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    )> {
        Self::popup_places(root, content)
            .into_iter()
            .map(|(origin, _, surface)| (origin, surface))
            .collect()
    }

    fn dismiss_popups(&mut self) {
        use smithay::desktop::PopupManager;
        use smithay::utils::SERIAL_COUNTER;

        for window in &self.windows {
            let root = window.toplevel.wl_surface();
            let open: Vec<_> = PopupManager::popups_for_surface(root)
                .map(|(popup, _)| popup)
                .collect();
            for popup in open {
                let _ = PopupManager::dismiss_popup(root, &popup);
            }
        }
        let time = self.started.elapsed().as_millis() as u32;
        let pointer = self.wayland.pointer.clone();
        if pointer.is_grabbed() {
            pointer.unset_grab(self, SERIAL_COUNTER.next_serial(), time);
        }
        let keyboard = self.wayland.keyboard.clone();
        if keyboard.is_grabbed() {
            keyboard.unset_grab(self);
        }
    }

    fn cursor_elements(&mut self) -> Vec<Skin> {
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
                    let art = crate::cursors::load(&names).and_then(|(image, hotspot)| {
                        let size = image.size();
                        upload(&mut self.renderer, Some(&image), "cursor")
                            .ok()
                            .flatten()
                            .map(|buffer| (buffer, size, hotspot))
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
                render_elements_from_surface_tree::<_, WaylandSurfaceRenderElement<GlesRenderer>>(
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
        self.cursor_status = status;
    }

    fn layer_elements(
        &mut self,
        kinds: &[smithay::wayland::shell::wlr_layer::Layer],
        elements: &mut Vec<Skin>,
    ) {
        let placed: Vec<(
            smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
            Vec2,
        )> = {
            let map = smithay::desktop::layer_map_for_output(&self.output);
            kinds
                .iter()
                .flat_map(|kind| map.layers_on(*kind).rev().collect::<Vec<_>>())
                .filter_map(|layer| {
                    let place = map.layer_geometry(layer)?;
                    let at = Vec2::new(f64::from(place.loc.x), f64::from(place.loc.y));
                    Some((layer.wl_surface().clone(), at))
                })
                .collect()
        };
        for (surface, at) in placed {
            let mut surfaces = Vec::new();
            for (popup, place) in smithay::desktop::PopupManager::popups_for_surface(&surface) {
                let geometry = popup.geometry();
                let origin = at
                    + Vec2::new(
                        f64::from(place.x - geometry.loc.x),
                        f64::from(place.y - geometry.loc.y),
                    );
                surfaces.push((popup.wl_surface().clone(), origin));
            }
            surfaces.push((surface, at));
            for (surface, origin) in surfaces {
                let anchor = smithay::utils::Point::<i32, smithay::utils::Physical>::from((
                    origin.x.round() as i32,
                    origin.y.round() as i32,
                ));
                let bare: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                    render_elements_from_surface_tree(
                        &mut self.renderer,
                        &surface,
                        anchor,
                        1.0,
                        1.0,
                        Kind::Unspecified,
                    );
                elements.extend(bare.into_iter().map(|piece| {
                    Skin::Client(RescaleRenderElement::from_element(piece, anchor, 1.0))
                }));
            }
        }
    }

    fn focus_layer(
        &mut self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        use smithay::desktop::WindowSurfaceType;
        use smithay::utils::SERIAL_COUNTER;

        if self.lock.is_some() {
            return;
        }
        let wants = smithay::desktop::layer_map_for_output(&self.output)
            .layer_for_surface(surface, WindowSurfaceType::ALL)
            .map(|layer| {
                (
                    layer.can_receive_keyboard_focus(),
                    layer.wl_surface().clone(),
                )
            });
        let Some((true, root)) = wants else {
            return;
        };
        self.layer_focus = Some(root.clone());
        let keyboard = self.wayland.keyboard.clone();
        keyboard.set_focus(self, Some(root), SERIAL_COUNTER.next_serial());
    }

    pub fn layer_mapped(
        &mut self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        use smithay::desktop::WindowSurfaceType;
        use smithay::wayland::shell::wlr_layer::KeyboardInteractivity;

        if self.layer_focus.as_ref() == Some(surface) {
            return;
        }
        let exclusive = smithay::desktop::layer_map_for_output(&self.output)
            .layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
            .map(|layer| {
                layer.cached_state().keyboard_interactivity == KeyboardInteractivity::Exclusive
            })
            .unwrap_or(false);
        if exclusive {
            self.focus_layer(surface);
        }
    }

    pub fn layer_gone(
        &mut self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        if self.layer_focus.as_ref() == Some(surface) {
            self.layer_focus = None;
            self.focus_next(None);
        }
        self.kick_now();
    }

    pub fn import_dmabuf(&mut self, dmabuf: &smithay::backend::allocator::dmabuf::Dmabuf) -> bool {
        use smithay::backend::renderer::ImportDma;
        self.renderer.import_dmabuf(dmabuf, None).is_ok()
    }

    pub fn activate(
        &mut self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        if self.lock.is_some() {
            return;
        }
        let Some(index) = self
            .windows
            .iter()
            .position(|window| window.toplevel.wl_surface() == surface)
        else {
            return;
        };
        if self.windows[index].hidden {
            self.windows[index].hidden = false;
        }
        self.focus_window(index);
        self.kick_now();
    }

    pub fn window_geometry(
        &self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) -> Option<smithay::utils::Rectangle<i32, smithay::utils::Logical>> {
        let window = self
            .windows
            .iter()
            .find(|window| window.toplevel.wl_surface() == surface)?;
        Some(smithay::utils::Rectangle::new(
            (window.inset.x as i32, window.inset.y as i32).into(),
            (window.content.size.x as i32, window.content.size.y as i32).into(),
        ))
    }

    pub fn locked(&self) -> bool {
        self.lock.is_some()
    }

    fn dress(
        &self,
        content: Rect,
        (awake, maximized): (bool, bool),
        title: &str,
        fade: f32,
        elements: &mut Vec<Skin>,
    ) {
        let layout = self.chrome.layout(content);
        let show = |rect: Rect| self.on_screen(rect);
        let bar = self.chrome.title_bar(content);
        let close_enough = self.camera.zoom >= DETAIL_ZOOM;
        let over = self
            .chrome
            .button_at(content, self.in_world(self.pointer), 3);
        let grow = if maximized { "restore" } else { "maximize" };
        for (slot, kind) in self
            .chrome
            .order
            .iter()
            .enumerate()
            .filter(|_| close_enough)
        {
            let name = match kind {
                ChromeButton::Close => "close",
                ChromeButton::Minimize => "minimize",
                ChromeButton::Maximize | ChromeButton::Restore => grow,
            };
            let look = match (over == Some(slot), self.held == Some(slot)) {
                (true, true) => "-press",
                (true, false) => "-hover",
                _ => "",
            };
            let file = format!("button-{name}{look}.png");
            if let Some((buffer, size)) = self.skin.get(&file) {
                let place = self.chrome.button_rect(slot, bar.size.x);
                let rect = Rect::new(bar.origin + place.origin, *size);
                elements.push(Skin::Texture(element(buffer, show(rect), fade)));
            }
        }
        if let Some((buffer, size)) = self
            .titles
            .get(title)
            .filter(|_| close_enough && self.chrome.show_title)
        {
            let room = bar.size.x - self.chrome.buttons_width(3) * 2.0;
            let width = size.x.min(room.max(0.0));
            if width > 4.0 {
                let at = Vec2::new(
                    bar.center().x - width / 2.0,
                    bar.origin.y + (bar.size.y - size.y) / 2.0,
                );
                let rect = Rect::new(at.round(), Vec2::new(width, size.y));
                let dim = fade * if awake { 1.0 } else { 0.55 };
                elements.push(Skin::Texture(element(buffer, show(rect), dim)));
            }
        }

        let pieces = [
            (ChromePiece::TitleLeft, layout.title_left),
            (ChromePiece::TitleMid, layout.title_mid),
            (ChromePiece::TitleRight, layout.title_right),
            (ChromePiece::EdgeLeft, layout.edge_left),
            (ChromePiece::EdgeRight, layout.edge_right),
            (ChromePiece::EdgeBottom, layout.edge_bottom),
            (ChromePiece::CornerBottomLeft, layout.corner_left),
            (ChromePiece::CornerBottomRight, layout.corner_right),
        ];
        for (piece, rect) in pieces {
            if rect.size.x <= 0.0 || rect.size.y <= 0.0 {
                continue;
            }
            let Some((buffer, art)) = self.skin.get(&piece.file_name(awake)) else {
                continue;
            };
            match tiles(piece) {
                true => {
                    let source = Rect::new(Vec2::ZERO, rect.size.min(*art));
                    elements.push(Skin::Texture(faded(buffer, source, show(rect), fade)));
                }
                false => elements.push(Skin::Texture(element(buffer, show(rect), fade))),
            }
        }
    }

    pub fn settle_window(
        &mut self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        if !self
            .windows
            .iter()
            .any(|window| window.toplevel.wl_surface() == surface)
        {
            return;
        }

        let whole =
            smithay::backend::renderer::utils::with_renderer_surface_state(surface, |state| {
                state.surface_size()
            })
            .flatten();
        let Some(whole) = whole else {
            return;
        };

        let geometry = smithay::wayland::compositor::with_states(surface, |states| {
            states
                .cached_state
                .get::<smithay::wayland::shell::xdg::SurfaceCachedState>()
                .current()
                .geometry
        });

        let (size, inset) = match geometry {
            Some(area) if area.size.w > 0 && area.size.h > 0 => (
                Vec2::new(f64::from(area.size.w), f64::from(area.size.h)),
                Vec2::new(f64::from(area.loc.x), f64::from(area.loc.y)),
            ),
            _ => (
                Vec2::new(f64::from(whole.w), f64::from(whole.h)),
                Vec2::ZERO,
            ),
        };
        if size.x <= 0.0 || size.y <= 0.0 {
            return;
        }

        let (app, title) = smithay::wayland::compositor::with_states(surface, |states| {
            let held = states
                .data_map
                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                .and_then(|data| {
                    data.lock()
                        .ok()
                        .map(|data| (data.app_id.clone(), data.title.clone()))
                });
            held.unwrap_or((None, None))
        });
        let app = app.unwrap_or_default();

        let fresh = self
            .windows
            .iter()
            .find(|window| window.toplevel.wl_surface() == surface)
            .map(|window| !window.placed)
            .unwrap_or(false);
        if fresh {
            self.output.enter(surface);
        }

        let view = self.camera.visible_rect(self.screen);
        let chrome = self.chrome;
        let taken: Vec<Vec2> = self
            .windows
            .iter()
            .filter(|window| window.placed && window.toplevel.wl_surface() != surface)
            .map(|window| window.content.origin)
            .collect();
        let Some(window) = self
            .windows
            .iter_mut()
            .find(|window| window.toplevel.wl_surface() == surface)
        else {
            return;
        };

        window.inset = inset;
        let grew = window.content.size != size;
        window.content.size = size;
        if let Some((_, screen)) = window.full {
            window.content.origin = (screen.center() - size / 2.0).round();
        }
        if let Grab::Resize {
            id, grip, start, ..
        } = self.grab
            && id == window.id
        {
            if grip.moves_left() {
                window.content.origin.x = start.max().x - size.x;
            }
            if grip.moves_up() {
                window.content.origin.y = start.max().y - size.y;
            }
        }
        if !app.is_empty() {
            window.app = app;
        }
        if let Some(title) = title {
            window.title = title;
        }

        if window.placed && (window.touched || !grew || window.born.elapsed() > SETTLING) {
            return;
        }

        let seen = view.origin;
        let middle = seen + (view.size - size) / 2.0;
        let top = middle.y.max(seen.y + chrome.title + chrome.edge);
        let wanted = Vec2::new(middle.x, top);
        window.content.origin = drifter_core::window::cascade(wanted, &taken, 34.0, view);
        window.placed = true;
        let wants_full = std::mem::take(&mut window.wants_full);

        if fresh
            && self.lock.is_none()
            && let Some(index) = self
                .windows
                .iter()
                .position(|window| window.toplevel.wl_surface() == surface)
        {
            self.focus_window(index);
        }
        if wants_full
            && let Some(index) = self
                .windows
                .iter()
                .position(|window| window.toplevel.wl_surface() == surface)
        {
            self.enter_fullscreen(index);
        }
    }

    fn full_rect(&self, near: Vec2) -> Rect {
        let half = self.screen / 2.0;
        let center = near.clamp(half, (self.world - half).max(half));
        Rect::from_center(center, self.screen).round_out()
    }

    pub fn enter_fullscreen(&mut self, index: usize) {
        use wayland_protocols::xdg::shell::server::xdg_toplevel;

        if self.windows[index].full.is_some() {
            return;
        }
        if !self.windows[index].placed {
            let window = &mut self.windows[index];
            window.wants_full = true;
            let size = (self.screen.x as i32, self.screen.y as i32);
            window.toplevel.with_pending_state(|state| {
                state.size = Some(size.into());
                state.states.set(xdg_toplevel::State::Fullscreen);
            });
            window.toplevel.send_pending_configure();
            return;
        }
        let screen = self.full_rect(self.windows[index].content.center());
        let window = &mut self.windows[index];
        window.full = Some((window.content, screen));
        window.content = Rect::new(
            screen.center() - window.content.size / 2.0,
            window.content.size,
        );
        window.touched = true;
        window.toplevel.with_pending_state(|state| {
            state.size = Some((screen.size.x as i32, screen.size.y as i32).into());
            state.states.set(xdg_toplevel::State::Fullscreen);
        });
        window.toplevel.send_pending_configure();
        self.popover = None;
        self.dwell = None;
        self.leave = None;
        for edge in Edge::ALL {
            self.set(edge, false);
        }
        self.focus_window(index);
        self.kick_now();
    }

    pub fn leave_fullscreen(&mut self, index: usize) {
        use wayland_protocols::xdg::shell::server::xdg_toplevel;

        let window = &mut self.windows[index];
        window.wants_full = false;
        let Some((back, _)) = window.full.take() else {
            window.toplevel.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Fullscreen);
            });
            window.toplevel.send_pending_configure();
            return;
        };
        window.content = back;
        window.toplevel.with_pending_state(|state| {
            state.size = Some((back.size.x as i32, back.size.y as i32).into());
            state.states.unset(xdg_toplevel::State::Fullscreen);
        });
        window.toplevel.send_pending_configure();
        self.kick_now();
    }

    fn held_by_fullscreen(&self) -> Option<usize> {
        let focus = self.focus?;
        self.windows
            .iter()
            .position(|window| window.id == focus && window.full.is_some() && !window.hidden)
    }

    fn draw(&mut self) -> Result<bool, String> {
        if !self.active {
            return Ok(false);
        }

        let now = Instant::now();
        let dt = now.duration_since(self.last);
        self.last = now;
        for drawer in &mut self.drawers {
            drawer.advance(dt, self.tunables.drawer_anim);
        }
        self.steer();
        self.step_windows(dt);

        if self.hud.as_ref().is_some_and(|(_, _, until)| now >= *until) {
            self.hud = None;
        }
        self.expire_cards(now);
        self.wind_the_clock();
        self.wind_the_stats();
        self.wind_titles();

        self.paint_lock();
        let elements = self.elements();
        let clear = self.clear;
        let frames = self.frames;
        let State {
            compositor,
            renderer,
            ..
        } = self;
        let empty = compositor
            .render_frame(renderer, &elements, clear, FrameFlags::DEFAULT)
            .map_err(|e| {
                if frames == 0 {
                    format!(
                        "first render failed: {e}\n\
                         drifter needs drm master, which needs a real tty.\n\
                         run it from tty1, not over ssh. smithay only warns when master is denied."
                    )
                } else {
                    format!("render failed after {frames} frames: {e}")
                }
            })?
            .is_empty;

        self.serve_copies(&elements, !empty);
        if self.shot_wanted {
            self.shot_wanted = false;
            self.screenshot(&elements);
        }

        if empty {
            self.frames_later();
            return Ok(false);
        }

        self.compositor
            .queue_frame(())
            .map_err(|e| format!("page flip failed after {} frames: {e}", self.frames))?;
        self.frames += 1;
        self.awaiting = true;
        self.tell_clients();
        Ok(true)
    }

    pub fn screen_size(&self) -> Vec2 {
        self.screen
    }

    pub fn queue_copy(&mut self, mut copy: crate::screencopy::Copy) {
        use smithay::reexports::wayland_server::Resource;

        let first = copy
            .frame
            .client()
            .is_some_and(|client| self.copied.insert(client.id()));
        if first {
            copy.damage = false;
        }
        let damage = copy.damage;
        self.copies.push(copy);
        if !damage {
            self.kick_now();
        }
    }

    fn capture(&mut self, elements: &[Skin], cursor: bool) -> Result<(Vec<u8>, bool), String> {
        use smithay::backend::renderer::damage::OutputDamageTracker;
        use smithay::backend::renderer::{Bind, ExportMem, Offscreen};
        use smithay::utils::{Buffer as BufferCoord, Physical, Rectangle, Size};

        let size = Size::<i32, Physical>::from((self.screen.x as i32, self.screen.y as i32));
        let buffer_size = Size::<i32, BufferCoord>::from((size.w, size.h));
        let mut texture: GlesTexture = Offscreen::<GlesTexture>::create_buffer(
            &mut self.renderer,
            DrmFourcc::Abgr8888,
            buffer_size,
        )
        .map_err(|e| format!("no offscreen buffer: {e}"))?;
        let mut target = self
            .renderer
            .bind(&mut texture)
            .map_err(|e| format!("cannot draw offscreen: {e}"))?;
        let shown = match cursor {
            true => elements,
            false => &elements[self.cursor_count.min(elements.len())..],
        };
        let mut tracker = OutputDamageTracker::new(size, 1.0, Transform::Normal);
        tracker
            .render_output(&mut self.renderer, &mut target, 0, shown, self.clear)
            .map_err(|e| format!("cannot draw offscreen: {e:?}"))?;
        let mapping = self
            .renderer
            .copy_framebuffer(
                &target,
                Rectangle::from_size(buffer_size),
                DrmFourcc::Abgr8888,
            )
            .map_err(|e| format!("cannot read the screen back: {e}"))?;
        let flipped = !smithay::backend::renderer::TextureMapping::flipped(&mapping);
        let bytes = self
            .renderer
            .map_texture(&mapping)
            .map_err(|e| format!("cannot read the screen back: {e}"))?
            .to_vec();
        Ok((bytes, flipped))
    }

    fn draw_into(
        &mut self,
        dmabuf: &mut smithay::backend::allocator::dmabuf::Dmabuf,
        elements: &[Skin],
        cursor: bool,
    ) -> Result<(), String> {
        use smithay::backend::allocator::Buffer;
        use smithay::backend::renderer::Bind;
        use smithay::backend::renderer::damage::OutputDamageTracker;
        use smithay::utils::{Physical, Size};

        let size = dmabuf.size();
        if (f64::from(size.w), f64::from(size.h)) != (self.screen.x, self.screen.y) {
            return Err("a screen copy buffer is not the size of the screen".to_string());
        }
        let size = Size::<i32, Physical>::from((size.w, size.h));
        let shown = match cursor {
            true => elements,
            false => &elements[self.cursor_count.min(elements.len())..],
        };
        let clear = self.clear;
        let mut target = self
            .renderer
            .bind(dmabuf)
            .map_err(|e| format!("cannot draw into a screen copy buffer: {e}"))?;
        let mut tracker = OutputDamageTracker::new(size, 1.0, Transform::Normal);
        let result = tracker
            .render_output(&mut self.renderer, &mut target, 0, shown, clear)
            .map_err(|e| format!("cannot draw into a screen copy buffer: {e:?}"))?;
        result
            .sync
            .wait()
            .map_err(|_| "a screen copy was interrupted".to_string())
    }

    fn serve_copies(&mut self, elements: &[Skin], changed: bool) {
        if self.copies.is_empty() {
            return;
        }
        let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.copies)
            .into_iter()
            .partition(|copy| !copy.damage || changed);
        self.copies = waiting;
        if ready.is_empty() {
            return;
        }
        if self.lock.is_some() {
            for copy in ready {
                copy.frame.failed();
            }
            return;
        }
        let mut shots: [Option<(Vec<u8>, bool)>; 2] = [None, None];
        for copy in ready {
            if let Ok(dmabuf) = smithay::wayland::dmabuf::get_dmabuf(&copy.buffer) {
                let mut dmabuf = dmabuf.clone();
                match self.draw_into(&mut dmabuf, elements, copy.cursor) {
                    Ok(()) => crate::screencopy::done(&copy),
                    Err(error) => {
                        eprintln!("drifter: {error}");
                        copy.frame.failed();
                    }
                }
                continue;
            }
            let slot = usize::from(copy.cursor);
            if shots[slot].is_none() {
                match self.capture(elements, copy.cursor) {
                    Ok(shot) => shots[slot] = Some(shot),
                    Err(error) => eprintln!("drifter: {error}"),
                }
            }
            let written = shots[slot].as_ref().map(|(bytes, flipped)| {
                crate::screencopy::fill(&copy, self.screen, bytes, *flipped)
            });
            match written {
                Some(Ok(())) => crate::screencopy::done(&copy),
                Some(Err(error)) => {
                    eprintln!("drifter: {error}");
                    copy.frame.failed();
                }
                None => copy.frame.failed(),
            }
        }
    }

    fn screenshot(&mut self, elements: &[Skin]) {
        let (bytes, flipped) = match self.capture(elements, false) {
            Ok(shot) => shot,
            Err(error) => {
                eprintln!("drifter: no screenshot: {error}");
                return;
            }
        };
        let (width, height) = (self.screen.x as u32, self.screen.y as u32);
        let mut rgba = Vec::with_capacity(bytes.len());
        for row in 0..height as usize {
            let source = if flipped {
                height as usize - 1 - row
            } else {
                row
            };
            let line = &bytes[source * width as usize * 4..(source + 1) * width as usize * 4];
            for pixel in line.as_chunks::<4>().0 {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
        }
        let image = drifter_core::Image {
            width,
            height,
            rgba,
        };
        let folder = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("Pictures/Screenshots");
        let stamp = drifter_core::tray::now("%Y-%m-%d %H.%M.%S").unwrap_or_default();
        let path = folder.join(format!("Screenshot {stamp}.png"));
        let saved = std::fs::create_dir_all(&folder)
            .map_err(|e| e.to_string())
            .and_then(|()| drifter_core::image::save_png(&image, &path));
        let Some(id) = self.notes_bus.as_ref().map(|bus| bus.next_id()) else {
            return;
        };
        let (summary, body) = match saved {
            Ok(()) => ("Screenshot saved".to_string(), path.display().to_string()),
            Err(error) => ("No screenshot".to_string(), error),
        };
        self.show_note(drifter_core::notify::Note {
            id,
            app: "Drifter".to_string(),
            summary,
            body,
            actions: Vec::new(),
            expire: Some(drifter_core::notify::DEFAULT_MS),
            icon: None,
        });
    }

    fn frames_later(&mut self) {
        if self.frame_timer {
            return;
        }
        let wait = Duration::from_millis(16);
        self.frame_timer = self
            .handle
            .insert_source(Timer::from_duration(wait), |_, _, state: &mut State| {
                state.frame_timer = false;
                state.tell_clients();
                TimeoutAction::Drop
            })
            .is_ok();
    }

    fn tell_clients(&mut self) {
        let now = self.started.elapsed().as_millis() as u32;
        for window in &self.windows {
            let root = window.toplevel.wl_surface();
            crate::shell::send_frame(root, now);
            for (popup, _) in smithay::desktop::PopupManager::popups_for_surface(root) {
                crate::shell::send_frame(popup.wl_surface(), now);
            }
        }
        let layers: Vec<_> = smithay::desktop::layer_map_for_output(&self.output)
            .layers()
            .map(|layer| layer.wl_surface().clone())
            .collect();
        for root in layers {
            crate::shell::send_frame(&root, now);
            for (popup, _) in smithay::desktop::PopupManager::popups_for_surface(&root) {
                crate::shell::send_frame(popup.wl_surface(), now);
            }
        }
        if let smithay::input::pointer::CursorImageStatus::Surface(surface) = &self.cursor_status {
            crate::shell::send_frame(surface, now);
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
            eprintln!("drifter: wayland dispatch failed: {error}");
        }
        let _ = display.flush_clients();
        self.display = Some(display);
    }

    pub fn kick_now(&mut self) {
        self.kick();
    }

    fn dock_band(&self) -> Option<Rect> {
        let drawer = self.drawers[Edge::Bottom.index()];
        drawer
            .visible()
            .then(|| drawer.band(self.screen, &self.shapes[Edge::Bottom.index()]))
    }

    fn minimized(&self) -> Vec<String> {
        self.windows
            .iter()
            .filter(|window| window.hidden)
            .map(|window| window.app.clone())
            .collect()
    }

    fn press_dock(&mut self) -> bool {
        let Some(band) = self.dock_band() else {
            return false;
        };
        let hidden = self.minimized();
        let pinned = self.pinned.len();
        let Some(slot) = self.dock.at(band, self.pointer, pinned, hidden.len()) else {
            return false;
        };

        match slot {
            Slot::Pinned(index) => {
                crate::apps::run(&self.pinned[index].0, &self.socket);
            }
            Slot::Settings => self.open_settings(),
            Slot::Trash => {
                let home = std::env::var("HOME").unwrap_or_default();
                let trash = format!("{home}/.local/share/Trash/files");
                let _ = std::fs::create_dir_all(&trash);
                crate::apps::open_folder(&trash, self.apps_chosen.files.as_ref(), &self.socket);
            }
            Slot::Minimized(index) => {
                let Some(place) = self
                    .windows
                    .iter()
                    .enumerate()
                    .filter(|(_, window)| window.hidden)
                    .map(|(at, _)| at)
                    .nth(index)
                else {
                    return true;
                };
                let seat = self.dock_slot(index);
                self.windows[place].hidden = false;
                self.windows[place].motion = Some(Motion::new(Phase::Restoring, seat));
                self.focus_window(place);
            }
        }
        true
    }

    fn tray_band(&self) -> Option<Rect> {
        let drawer = self.drawers[Edge::Top.index()];
        drawer
            .visible()
            .then(|| drawer.band(self.screen, &self.shapes[Edge::Top.index()]))
    }

    fn take_icons(&mut self, icons: Vec<crate::tray::Icon>) {
        let wanted = self.tray.icon as u32;
        self.icons.clear();
        for icon in icons {
            let image = icon.image.scaled(wanted, wanted);
            match upload(&mut self.renderer, Some(&image), "tray icon") {
                Ok(Some(buffer)) => self.icons.push((icon.service, icon.title, buffer)),
                Ok(None) => {}
                Err(error) => eprintln!("drifter: {error}"),
            }
        }
    }

    fn open_popover(&mut self, items: Vec<Item>, at: Vec2, target: Target) {
        if items.is_empty() || self.lock.is_some() {
            return;
        }
        let look = self.look;
        let size = look.size(&items, self.font.as_ref());
        let image = look.paint(&items, size, None, self.font.as_ref());
        let rect = look.rect(at, size, self.screen);
        match upload(&mut self.renderer, Some(&image), "menu") {
            Ok(Some(art)) => {
                self.popover = Some(Popover {
                    items,
                    rect,
                    art,
                    target,
                    hover: None,
                })
            }
            Ok(None) => {}
            Err(error) => eprintln!("drifter: {error}"),
        }
    }

    fn repaint_popover(&mut self) {
        let Some(open) = self.popover.as_ref() else {
            return;
        };
        let inside = self.pointer - open.rect.origin;
        let hover = self.look.item_at(&open.items, open.rect.size, inside);
        if hover == open.hover {
            return;
        }
        let items = open.items.clone();
        let size = open.rect.size;
        let image = self.look.paint(&items, size, hover, self.font.as_ref());
        match upload(&mut self.renderer, Some(&image), "menu") {
            Ok(Some(art)) => {
                if let Some(open) = self.popover.as_mut() {
                    open.art = art;
                    open.hover = hover;
                }
            }
            Ok(None) => {}
            Err(error) => eprintln!("drifter: {error}"),
        }
    }

    fn press_popover(&mut self) -> bool {
        let Some(open) = self.popover.as_ref() else {
            return false;
        };
        if !open.rect.contains(self.pointer) {
            self.popover = None;
            return false;
        }

        let inside = self.pointer - open.rect.origin;
        let Some(index) = self.look.item_at(&open.items, open.rect.size, inside) else {
            return true;
        };
        let chosen = open.items[index].clone();
        if !chosen.enabled {
            return true;
        }

        match &open.target {
            Target::Tray(service) => {
                let service = service.clone();
                if let Some(bus) = self.tray_bus.as_ref() {
                    bus.choose(&service, chosen.id);
                }
            }
            Target::Desktop => {
                self.popover = None;
                match Action::from_id(chosen.id) {
                    Some(Action::Lock) => {
                        self.lock_now();
                    }
                    Some(Action::Logout) => self.signal.stop(),
                    Some(Action::Suspend) => {
                        if self.lock_now() {
                            crate::power::run(Action::Suspend);
                        }
                    }
                    Some(action) => crate::power::run(action),
                    None => self.open_settings(),
                }
                return true;
            }
            Target::Window(id) => {
                let id = *id;
                self.popover = None;
                let Some(index) = self.place_of(id) else {
                    return true;
                };
                match chosen.id {
                    0 => {
                        let window = &mut self.windows[index];
                        window.hidden = !window.hidden;
                        if window.hidden {
                            self.focus_next(Some(id));
                        } else {
                            self.focus_window(index);
                        }
                    }
                    _ => self.windows[index].toplevel.send_close(),
                }
                return true;
            }
        }
        self.popover = None;
        true
    }

    fn show_tip(&mut self, text: &str, under: Rect, below: bool) {
        if self.tip_of.as_deref() == Some(text) {
            return;
        }
        let items = vec![Item::line(0, text)];
        let look = Look {
            least_width: 0.0,
            ..self.look
        };
        let size = look.size(&items, self.font.as_ref());
        let image = look.paint(&items, size, None, self.font.as_ref());
        let at = Vec2::new(
            under.center().x - size.x / 2.0,
            if below {
                under.max().y + 6.0
            } else {
                under.origin.y - size.y - 6.0
            },
        );
        let rect = look.rect(at, size, self.screen);
        match upload(&mut self.renderer, Some(&image), "hover text") {
            Ok(Some(art)) => {
                self.tip = Some((art, rect));
                self.tip_of = Some(text.to_string());
            }
            Ok(None) => {}
            Err(error) => eprintln!("drifter: {error}"),
        }
    }

    fn settle_tip(&mut self) {
        let Some((at, since)) = self.hover_since else {
            self.tip = None;
            self.tip_of = None;
            return;
        };
        if (self.pointer - at).length() > 3.0 {
            self.hover_since = Some((self.pointer, Instant::now()));
            self.tip = None;
            self.tip_of = None;
            return;
        }
        if Instant::now().duration_since(since) < self.tunables.drawer_dwell * 4 {
            return;
        }

        if let Some(band) = self.tray_band()
            && self.tray.on_power(band, self.pointer)
        {
            let rect = self.tray.power_rect(band);
            self.show_tip("Power", rect, true);
            return;
        }

        if let Some(band) = self.tray_band()
            && let Some(slot) = self.tray.icon_at(band, self.pointer, self.tray_room(band))
        {
            let rect = self.tray.icon_rect(band, slot);
            let text = self.icons[slot].1.clone();
            self.show_tip(&text, rect, true);
            return;
        }

        if let Some(band) = self.dock_band() {
            let hidden = self.minimized();
            let pinned = self.pinned.len();
            if let Some(slot) = self.dock.at(band, self.pointer, pinned, hidden.len()) {
                let rect = self.dock.slot_rect(band, pinned, hidden.len(), slot);
                let text = match slot {
                    Slot::Pinned(index) => self.pinned[index].0.name(),
                    Slot::Minimized(index) => hidden[index].clone(),
                    Slot::Settings => "Settings".to_string(),
                    Slot::Trash => "Trash".to_string(),
                };
                self.show_tip(&text, rect, false);
                return;
            }
        }

        self.tip = None;
        self.tip_of = None;
    }

    fn open_context(&mut self) -> bool {
        if let Some(band) = self.tray_band()
            && let Some(slot) = self.tray.icon_at(band, self.pointer, self.tray_room(band))
        {
            let service = self.icons[slot].0.clone();
            if let Some(bus) = self.tray_bus.as_ref() {
                bus.ask_menu(&service, self.pointer.x, self.pointer.y);
            }
            return true;
        }

        if !self.over_drawer() && self.surface_under().is_none() && self.grip_under().is_none() {
            let at = self.pointer;
            let mut items = vec![Item::line(0, "Settings"), Item::rule(1)];
            items.extend(drifter_core::power::items());
            self.open_popover(items, at, Target::Desktop);
            return true;
        }

        if let Some(band) = self.dock_band() {
            let hidden = self.minimized();
            let pinned = self.pinned.len();
            if let Some(Slot::Minimized(index)) =
                self.dock.at(band, self.pointer, pinned, hidden.len())
            {
                let Some(place) = self
                    .windows
                    .iter()
                    .enumerate()
                    .filter(|(_, window)| window.hidden)
                    .map(|(at, _)| at)
                    .nth(index)
                else {
                    return true;
                };
                let items = vec![Item::line(0, "Show"), Item::line(1, "Exit")];
                let at = self.pointer;
                let id = self.windows[place].id;
                self.open_popover(items, at, Target::Window(id));
                return true;
            }
        }

        false
    }

    fn watch_corner(&mut self) {
        if self.modifier || self.lock.is_some() || self.held_by_fullscreen().is_some() {
            self.corner_since = None;
            return;
        }
        let size = self.tunables.hot_corner_size;
        let in_corner = self.pointer.x < size && self.pointer.y < size;

        if !in_corner {
            self.corner_since = None;
            return;
        }
        if self.overview() || self.flight.is_some() {
            self.corner_since = None;
            return;
        }
        match self.corner_since {
            None => self.corner_since = Some(Instant::now()),
            Some(since) => {
                if Instant::now().duration_since(since) >= self.tunables.hot_corner_dwell {
                    self.corner_since = None;
                    let framed: Vec<Rect> = self
                        .windows
                        .iter()
                        .filter(|window| window.placed && !window.hidden)
                        .map(|window| self.chrome.layout(window.content).frame)
                        .collect();
                    let target =
                        Camera::showing(&framed, self.screen, self.chrome.title * 2.0, self.world);
                    self.fly_to(target);
                }
            }
        }
    }

    fn press_overview(&mut self) -> bool {
        if !self.overview() {
            return false;
        }
        let target = match self.under_pointer() {
            Some((index, _)) => self.windows[index].content.center(),
            None => self.in_world(self.pointer),
        };
        self.fly_to(Camera::at_work_zoom(target));
        true
    }

    fn zoom_by(&mut self, steps: f64) {
        if self.held_by_fullscreen().is_some() {
            return;
        }
        let least = Camera::fit_zoom(self.screen, self.world);
        let wanted = (self.camera.zoom * (1.0 + steps * 0.12)).clamp(least, 4.0);
        let mut camera = self.camera;
        camera.zoom_at(self.pointer, wanted, self.screen);
        camera.clamp_to_world(self.world, self.screen);
        self.camera = camera;
        self.flight = None;
    }

    fn open_settings(&mut self) {
        if self.lock.is_some() {
            return;
        }
        if let Some(place) = self
            .windows
            .iter()
            .position(|window| window.app == SETTINGS_APP && !window.going)
        {
            self.bring(place);
            return;
        }
        let program = std::env::current_exe()
            .map(|path| path.with_file_name(SETTINGS_APP))
            .unwrap_or_else(|_| std::path::PathBuf::from(SETTINGS_APP));
        crate::apps::launch(&[program.to_string_lossy().into_owned()], &self.socket);
    }

    fn watch_idle(&mut self) {
        if self.lock_after == 0 || self.lock.is_some() {
            return;
        }
        if !self.wayland.extras.inhibitors.is_empty() {
            self.last_input = Instant::now();
            return;
        }
        if self.last_input.elapsed() >= Duration::from_secs(self.lock_after * 60) {
            self.lock_now();
        }
    }

    fn watch_wallpaper(&mut self) {
        let Some(folder) = self.paper.folder.clone() else {
            return;
        };
        if self.paper.every == 0 {
            self.paper.next = None;
            return;
        }
        let now = Instant::now();
        let every = Duration::from_secs(self.paper.every);
        let due = *self.paper.next.get_or_insert(now + every);
        if now < due {
            return;
        }
        self.paper.next = Some(now + every);
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos() as u64)
            .unwrap_or(0);
        let pictures = drifter_core::wallpaper::pictures(&folder);
        if let Some(next) = drifter_core::wallpaper::next_picture(
            &pictures,
            self.paper.shown.as_deref(),
            self.paper.order,
            seed,
        ) {
            self.load_wallpaper(next);
        }
    }

    fn load_wallpaper(&self, path: PathBuf) {
        let reply = self.paper_tx.clone();
        let spawned = std::thread::Builder::new()
            .name("drifter-wallpaper".to_string())
            .spawn(move || match drifter_core::image::load(&path) {
                Ok(image) => {
                    let _ = reply.send((path, image));
                }
                Err(error) => eprintln!("drifter: no wallpaper {}: {error}", path.display()),
            });
        if let Err(error) = spawned {
            eprintln!("drifter: cannot load the wallpaper: {error}");
        }
    }

    fn take_wallpaper(&mut self, path: PathBuf, image: drifter_core::Image) {
        match upload(&mut self.renderer, Some(&image), "wallpaper") {
            Ok(Some(buffer)) => {
                self.wallpaper = Some((buffer, image.size()));
                self.paper.shown = Some(path);
            }
            Ok(None) => {}
            Err(error) => eprintln!("drifter: {error}"),
        }
        self.kick_now();
    }

    fn watch_config(&mut self) {
        let seen = std::fs::metadata(&self.config_path)
            .and_then(|meta| meta.modified())
            .ok();
        if seen == self.config_seen {
            return;
        }
        self.config_seen = seen;
        if seen.is_none() {
            return;
        }
        match Config::load_from(&self.config_path) {
            Ok(config) => {
                self.apply(&config);
                self.kick_now();
            }
            Err(error) => eprintln!("drifter: the config did not load: {error}"),
        }
    }

    fn apply(&mut self, config: &Config) {
        self.world = config.world_size(self.screen);
        let least = Camera::fit_zoom(self.screen, self.world);
        self.camera.zoom = self.camera.zoom.clamp(least, 4.0);
        self.camera.clamp_to_world(self.world, self.screen);
        self.flight = None;
        self.mode = config.workspace.wallpaper_mode;
        self.holds = config.input.modifier;
        self.modifier = false;

        let heights = [config.drawers.top, config.drawers.bottom];
        for edge in Edge::ALL {
            let shape = &mut self.shapes[edge.index()];
            if shape.height == heights[edge.index()] {
                continue;
            }
            shape.height = heights[edge.index()];
            let image = drawer::paint(edge, self.screen.x as u32, shape, PANEL_COLOR, PANEL_ALPHA);
            match upload(&mut self.renderer, Some(&image), "drawer") {
                Ok(Some(buffer)) => self.panels[edge.index()] = buffer,
                Ok(None) => {}
                Err(error) => eprintln!("drifter: {error}"),
            }
        }

        if self.tray.size != config.tray.size {
            self.tray.size = config.tray.size;
            match Font::find(config.font_path(), config.tray.size) {
                Ok(font) => self.font = Some(font),
                Err(error) => eprintln!("drifter: no clock: {error}"),
            }
            self.clock = None;
            self.stats = None;
            self.titles.clear();
        }
        self.format = config.tray.format.clone();
        self.chrome.follow(&config.window);
        self.lock_after = config.lock.after;
        self.login = config.login.start.clone();
        let paper = Paper::from(config);
        if paper.set != self.paper.set {
            match paper.set.clone() {
                Some(path) => self.load_wallpaper(path),
                None => self.wallpaper = None,
            }
        }
        if (paper.folder.clone(), paper.every) != (self.paper.folder.clone(), self.paper.every) {
            self.paper.next = None;
        }
        self.paper.set = paper.set;
        self.paper.folder = paper.folder;
        self.paper.every = paper.every;
        self.paper.order = paper.order;
        self.dock.settings = config.dock.settings;
        self.dock.trash = config.dock.trash;
        self.apps_chosen = config.apps.clone();
        let wanted = (config.dock.clone(), config.apps.clone());
        if self.pinned_from != wanted {
            let side = self.dock.icon as u32;
            match dock_items(&mut self.renderer, config, side, self.dock_spare.as_ref()) {
                Ok(pinned) => {
                    self.pinned = pinned;
                    self.pinned_from = wanted;
                }
                Err(error) => eprintln!("drifter: {error}"),
            }
        }
        self.sampler = crate::stats::Sampler::new(&config.tray);
        self.stats = None;
    }

    fn press_tray(&mut self, code: u32, had_menu: bool) -> bool {
        let Some(band) = self.tray_band() else {
            return false;
        };
        if self.tray.on_power(band, self.pointer) {
            if code == BTN_LEFT && !had_menu {
                let rect = self.tray.power_rect(band);
                let at = Vec2::new(rect.max().x, rect.max().y + 6.0);
                self.open_popover(drifter_core::power::items(), at, Target::Desktop);
            }
            return true;
        }
        let Some(slot) = self.tray.icon_at(band, self.pointer, self.tray_room(band)) else {
            return false;
        };
        let click = match code {
            BTN_LEFT => crate::tray::Click::Left,
            BTN_MIDDLE => crate::tray::Click::Middle,
            _ => return true,
        };
        if let Some(bus) = self.tray_bus.as_ref() {
            let service = &self.icons[slot].0;
            bus.click(service, click, self.pointer.x as i32, self.pointer.y as i32);
        }
        true
    }

    fn over_own_parts(&self) -> bool {
        self.prompt_art.is_some()
            || self
                .card_rects()
                .iter()
                .any(|rect| rect.contains(self.pointer))
            || self.over_drawer()
            || self
                .popover
                .as_ref()
                .is_some_and(|open| open.rect.contains(self.pointer))
    }

    fn tray_room(&self, band: Rect) -> usize {
        self.tray_layout(band).0
    }

    fn tray_layout(&self, band: Rect) -> (usize, bool) {
        let stop = match self.clock.as_ref() {
            Some((_, _, rect)) => rect.origin.x,
            None => self.tray.power_rect(band).origin.x,
        };
        let stats = self.stats.as_ref().map(|(_, size)| size.x);
        self.tray.layout(band, stop, self.icons.len(), stats)
    }

    fn over_drawer(&self) -> bool {
        Edge::ALL.into_iter().any(|edge| {
            let drawer = &self.drawers[edge.index()];
            drawer.visible()
                && drawer
                    .rect(self.screen, &self.shapes[edge.index()])
                    .contains(self.pointer)
        })
    }

    fn wind_titles(&mut self) {
        let Some(font) = self.font.as_ref() else {
            return;
        };
        let wanted: Vec<String> = self
            .windows
            .iter()
            .filter(|window| window.decorated && !window.title.is_empty())
            .map(|window| window.title.clone())
            .collect();

        for title in wanted {
            if self.titles.contains_key(&title) {
                continue;
            }
            let Some(font) = self.font.as_ref() else {
                return;
            };
            let image = font.paint(&title, self.tray.ink);
            let size = image.size();
            match upload(&mut self.renderer, Some(&image), "window title") {
                Ok(Some(buffer)) => {
                    self.titles.insert(title, (buffer, size));
                }
                Ok(None) => {}
                Err(error) => eprintln!("drifter: {error}"),
            }
        }
        let _ = font;

        if self.titles.len() > 64 {
            let live: std::collections::HashSet<&str> = self
                .windows
                .iter()
                .map(|window| window.title.as_str())
                .collect();
            self.titles.retain(|title, _| live.contains(title.as_str()));
        }
    }

    fn wind_the_clock(&mut self) {
        let drawer = self.drawers[Edge::Top.index()];
        if !drawer.visible() || self.font.is_none() {
            self.clock = None;
            return;
        }

        let reading = match drifter_core::tray::now(&self.format) {
            Ok(reading) => reading,
            Err(error) => {
                eprintln!("drifter: {error}");
                self.format = drifter_core::tray::DEFAULT_FORMAT.to_string();
                return;
            }
        };

        let tray = self.tray;
        let band = drawer.band(self.screen, &self.shapes[Edge::Top.index()]);

        if let Some((shown, _, rect)) = self.clock.as_mut()
            && *shown == reading
        {
            *rect = tray.clock_rect(band, rect.size);
            return;
        }

        let image = self
            .font
            .as_ref()
            .map(|font| font.paint(&reading, tray.ink))
            .expect("the font was there a moment ago");
        let rect = tray.clock_rect(band, image.size());
        match upload(&mut self.renderer, Some(&image), "clock") {
            Ok(Some(buffer)) => self.clock = Some((reading, buffer, rect)),
            Ok(None) => {}
            Err(error) => eprintln!("drifter: {error}"),
        }
    }

    fn wind_the_stats(&mut self) {
        if !self.drawers[Edge::Top.index()].visible() || self.font.is_none() {
            self.stats = None;
            return;
        }
        if self.stats.is_some()
            && self
                .sampled
                .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.sampled = Some(Instant::now());
        let fields = self.sampler.read();
        if fields.is_empty() {
            self.stats = None;
            return;
        }
        let fields: Vec<(&str, u8)> = fields
            .iter()
            .map(|(label, value)| (label.as_str(), *value))
            .collect();
        let image = self
            .font
            .as_ref()
            .map(|font| drifter_core::stats::paint(font, &fields, self.tray.ink))
            .expect("the font was there a moment ago");
        let size = image.size();
        match upload(&mut self.renderer, Some(&image), "system info") {
            Ok(Some(buffer)) => self.stats = Some((buffer, size)),
            Ok(None) => {}
            Err(error) => eprintln!("drifter: {error}"),
        }
    }

    fn kick(&mut self) {
        if !self.awaiting
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
        self.watch_corner();
        self.repaint_popover();
        if self.hover_since.is_none() {
            self.hover_since = Some((self.pointer, Instant::now()));
        } else if let Some((at, _)) = self.hover_since
            && (self.pointer - at).length() > 3.0
        {
            self.hover_since = Some((self.pointer, Instant::now()));
            self.tip = None;
            self.tip_of = None;
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
                self.set(edge, false);
            }
        }
        self.settle_tip();
        self.watch_corner();
        if !self.awaiting
            && let Err(error) = self.draw()
        {
            self.stop(error);
        }
    }

    fn set(&mut self, edge: Edge, open: bool) {
        self.drawers[edge.index()].open = open;
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

    fn pointer_to(&mut self, position: Vec2) {
        let edge = self.screen - Vec2::splat(1.0);
        let inside = position.clamp(Vec2::ZERO, edge);
        let was = self.pointer;

        if self.modifier {
            let band = PUSH_BAND;
            let moved = position - was;
            let mut pan = position - inside;
            if inside.x <= band && moved.x < 0.0 {
                pan.x = moved.x;
            }
            if inside.x >= edge.x - band && moved.x > 0.0 {
                pan.x = moved.x;
            }
            if inside.y <= band && moved.y < 0.0 {
                pan.y = moved.y;
            }
            if inside.y >= edge.y - band && moved.y > 0.0 {
                pan.y = moved.y;
            }

            if pan != Vec2::ZERO && self.held_by_fullscreen().is_none() {
                let mut camera = self.camera;
                camera.pan_by_screen(pan);
                camera.clamp_to_world(self.world, self.screen);
                self.camera = camera;
                self.flight = None;
            }
            self.pointer = inside;
            self.dwell = None;
            return;
        }

        self.pointer = inside;

        match self.wanted() {
            Some(edge) => {
                self.leave = None;
                if !self.drawers[edge.index()].open
                    && self.dwell.map(|(held, _)| held != edge).unwrap_or(true)
                {
                    self.dwell = Some((edge, Instant::now()));
                }
            }
            None => {
                self.dwell = None;
                if self.drawers.iter().any(|drawer| drawer.open) && self.leave.is_none() {
                    self.leave = Some(Instant::now());
                }
            }
        }
    }

    fn under_pointer(&self) -> Option<(usize, Vec2)> {
        let world = self.in_world(self.pointer);
        self.windows
            .iter()
            .enumerate()
            .rev()
            .find(|(_, window)| window.placed && !window.hidden && window.content.contains(world))
            .map(|(index, window)| (index, world - window.content.origin))
    }

    fn layer_hit(&self, kinds: &[smithay::wayland::shell::wlr_layer::Layer]) -> Option<Hit> {
        use smithay::desktop::WindowSurfaceType;

        let map = smithay::desktop::layer_map_for_output(&self.output);
        let world = self.in_world(self.pointer);
        for kind in kinds {
            let Some(layer) = map.layer_under(*kind, (self.pointer.x, self.pointer.y)) else {
                continue;
            };
            let Some(place) = map.layer_geometry(layer) else {
                continue;
            };
            let at = Vec2::new(f64::from(place.loc.x), f64::from(place.loc.y));
            let local = self.pointer - at;
            if let Some((surface, inside)) =
                layer.surface_under((local.x, local.y), WindowSurfaceType::ALL)
            {
                let origin = at + Vec2::new(f64::from(inside.x), f64::from(inside.y));
                return Some(Hit {
                    index: None,
                    layer: true,
                    surface,
                    origin: world - (self.pointer - origin),
                    popup: false,
                });
            }
        }
        None
    }

    fn surface_under(&self) -> Option<Hit> {
        use smithay::wayland::shell::wlr_layer::Layer;

        if let Some(hit) = self.layer_hit(&[Layer::Overlay, Layer::Top]) {
            return Some(hit);
        }
        self.window_hit()
            .or_else(|| self.layer_hit(&[Layer::Bottom, Layer::Background]))
    }

    fn window_hit(&self) -> Option<Hit> {
        let world = self.in_world(self.pointer);
        for (index, window) in self.windows.iter().enumerate().rev() {
            if !window.placed || window.hidden {
                continue;
            }
            let root = window.toplevel.wl_surface();
            for (origin, area, surface) in Self::popup_places(root, window.content) {
                if area.contains(world) {
                    return Some(Hit {
                        index: Some(index),
                        layer: false,
                        surface,
                        origin,
                        popup: true,
                    });
                }
            }
            if window.content.contains(world) {
                return Some(Hit {
                    index: Some(index),
                    layer: false,
                    surface: root.clone(),
                    origin: window.content.origin - window.inset,
                    popup: false,
                });
            }
        }
        None
    }

    fn tell_pointer(&mut self, time: u32) {
        use smithay::input::pointer::MotionEvent;
        use smithay::utils::SERIAL_COUNTER;

        let focus = self.pointer_focus();
        if focus.is_none() || self.grip_under().is_some() {
            self.cursor_status = smithay::input::pointer::CursorImageStatus::default_named();
        }
        let pointer = self.wayland.pointer.clone();
        let world = self.in_world(self.pointer);
        let at = smithay::utils::Point::from((world.x, world.y));
        pointer.motion(
            self,
            focus,
            &MotionEvent {
                location: at,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        pointer.frame(self);
        if let Some(surface) = pointer.current_focus() {
            smithay::wayland::pointer_constraints::with_pointer_constraint(
                &surface,
                &pointer,
                |constraint| {
                    if let Some(constraint) = constraint
                        && !constraint.is_active()
                    {
                        constraint.activate();
                    }
                },
            );
        }
    }

    fn pointer_focus(
        &self,
    ) -> Option<(
        smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        smithay::utils::Point<f64, smithay::utils::Logical>,
    )> {
        if self.over_own_parts() {
            return None;
        }
        self.surface_under().map(|hit| {
            (
                hit.surface,
                smithay::utils::Point::from((hit.origin.x, hit.origin.y)),
            )
        })
    }

    fn tell_relative(&mut self, delta: Vec2, bare: Vec2, utime: u64) {
        use smithay::input::pointer::RelativeMotionEvent;

        let focus = self.pointer_focus();
        let pointer = self.wayland.pointer.clone();
        pointer.relative_motion(
            self,
            focus,
            &RelativeMotionEvent {
                delta: (delta.x, delta.y).into(),
                delta_unaccel: (bare.x, bare.y).into(),
                utime,
            },
        );
        pointer.frame(self);
    }

    fn held_pointer(&self) -> Option<Option<Rect>> {
        use smithay::wayland::pointer_constraints::{PointerConstraint, with_pointer_constraint};

        if self.modifier {
            return None;
        }
        let pointer = self.wayland.pointer.clone();
        let surface = pointer.current_focus()?;
        let locked = with_pointer_constraint(&surface, &pointer, |constraint| {
            let constraint = constraint.filter(|constraint| constraint.is_active())?;
            Some(matches!(&*constraint, PointerConstraint::Locked(_)))
        })?;
        if locked {
            return Some(None);
        }
        let window = self
            .windows
            .iter()
            .find(|window| window.toplevel.wl_surface() == &surface)?;
        Some(Some(self.on_screen(window.content)))
    }

    fn raise(&mut self, index: usize) -> usize {
        let window = self.windows.remove(index);
        self.focus = Some(window.id);
        self.windows.push(window);
        self.wake_windows();
        self.windows.len() - 1
    }

    fn focus_window(&mut self, index: usize) -> usize {
        use smithay::utils::SERIAL_COUNTER;

        let index = self.raise(index);
        self.layer_focus = None;
        if let Some((_, screen)) = self.windows[index].full {
            self.fly_to(Camera::at_work_zoom(screen.center()));
        }
        if self.lock.is_none() {
            let surface = self.windows[index].toplevel.wl_surface().clone();
            let keyboard = self.wayland.keyboard.clone();
            keyboard.set_focus(self, Some(surface), SERIAL_COUNTER.next_serial());
        }
        index
    }

    fn focus_next(&mut self, skip: Option<u64>) {
        use smithay::utils::SERIAL_COUNTER;

        let next = self.windows.iter().rposition(|window| {
            Some(window.id) != skip
                && window.placed
                && !window.hidden
                && !window.going
                && !window
                    .motion
                    .is_some_and(|motion| motion.phase == Phase::Minimizing)
        });
        match next {
            Some(index) => {
                self.focus_window(index);
            }
            None => {
                self.focus = None;
                self.wake_windows();
                if self.lock.is_none() {
                    let keyboard = self.wayland.keyboard.clone();
                    keyboard.set_focus(self, None, SERIAL_COUNTER.next_serial());
                }
            }
        }
    }

    fn place_of(&self, id: u64) -> Option<usize> {
        self.windows.iter().position(|window| window.id == id)
    }

    pub fn next_window_id(&mut self) -> u64 {
        self.next_window += 1;
        self.next_window
    }

    pub fn window_gone(&mut self, surface: &smithay::wayland::shell::xdg::ToplevelSurface) {
        let Some(index) = self
            .windows
            .iter()
            .position(|window| &window.toplevel == surface)
        else {
            return;
        };
        let window = self.windows.remove(index);
        let id = window.id;
        match self.grab {
            Grab::Move { id: held, .. } | Grab::Resize { id: held, .. } if held == id => {
                self.grab = Grab::None;
                self.zone = None;
                self.held = None;
            }
            _ => {}
        }
        if matches!(
            self.popover.as_ref().map(|open| &open.target),
            Some(Target::Window(target)) if *target == id
        ) {
            self.popover = None;
        }
        if self.focus == Some(id) {
            self.focus_next(None);
        }
    }

    fn wake_windows(&mut self) {
        use wayland_protocols::xdg::shell::server::xdg_toplevel;
        for window in self.windows.iter() {
            let awake = self.focus == Some(window.id);
            window.toplevel.with_pending_state(|state| {
                if awake {
                    state.states.set(xdg_toplevel::State::Activated);
                } else {
                    state.states.unset(xdg_toplevel::State::Activated);
                }
            });
            window.toplevel.send_pending_configure();
        }
    }

    fn grip_under(&self) -> Option<(usize, drifter_core::window::Grip)> {
        self.windows
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, window)| {
                window.placed && !window.hidden && window.decorated && window.full.is_none()
            })
            .find_map(|(index, window)| {
                self.chrome
                    .grip_at(window.content, self.in_world(self.pointer), 3)
                    .map(|grip| (index, grip))
            })
    }

    fn take_grab(&mut self) -> bool {
        use drifter_core::window::Grip;

        let Some((index, grip)) = self.grip_under() else {
            return false;
        };
        let index = self.focus_window(index);
        let window = &self.windows[index];
        let id = window.id;
        let content = window.content;

        match grip {
            Grip::Button(slot) => {
                self.held = Some(slot);
                self.press_button(index, self.chrome.order[slot]);
            }
            Grip::Title => {
                let now = Instant::now();
                let again = self
                    .last_title
                    .map(|(at, when)| {
                        (self.pointer - at).length() < 6.0
                            && now.duration_since(when) < DOUBLE_CLICK
                    })
                    .unwrap_or(false);
                if again {
                    self.last_title = None;
                    self.press_button(index, ChromeButton::Maximize);
                } else {
                    self.last_title = Some((self.pointer, now));
                    self.grab = Grab::Move {
                        id,
                        offset: self.in_world(self.pointer) - content.origin,
                    };
                }
            }
            other => {
                self.grab = Grab::Resize {
                    id,
                    grip: other,
                    from: self.in_world(self.pointer),
                    start: content,
                };
            }
        }
        true
    }

    pub fn start_move(&mut self, index: usize) {
        if self.lock.is_some() || self.windows[index].full.is_some() {
            return;
        }
        let index = self.raise(index);
        let window = &self.windows[index];
        self.grab = Grab::Move {
            id: window.id,
            offset: self.in_world(self.pointer) - window.content.origin,
        };
    }

    pub fn start_resize(
        &mut self,
        index: usize,
        edges: smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge,
    ) {
        if self.lock.is_some() || self.windows[index].full.is_some() {
            return;
        }
        use drifter_core::window::Grip;
        use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge as Side;

        let grip = match edges {
            Side::Left => Grip::Left,
            Side::Right => Grip::Right,
            Side::Top => Grip::Top,
            Side::TopLeft => Grip::TopLeft,
            Side::TopRight => Grip::TopRight,
            Side::Bottom => Grip::Bottom,
            Side::BottomLeft => Grip::BottomLeft,
            Side::BottomRight => Grip::BottomRight,
            _ => return,
        };
        let index = self.raise(index);
        let window = &self.windows[index];
        let start = window.content;
        self.grab = Grab::Resize {
            id: window.id,
            grip,
            from: self.in_world(self.pointer),
            start,
        };
    }

    fn land(&mut self, index: usize, zone: Zone) {
        let view = self.camera.visible_rect(self.screen);
        let frame = self.snap.rect(zone, view);
        let chrome = self.chrome;
        let want = Rect::new(
            Vec2::new(frame.origin.x + chrome.edge, frame.origin.y + chrome.title),
            Vec2::new(
                (frame.size.x - chrome.edge * 2.0).max(80.0),
                (frame.size.y - chrome.title - chrome.edge).max(60.0),
            ),
        )
        .round_out();
        let Some(window) = self.windows.get_mut(index) else {
            return;
        };
        if window.restore.is_none() {
            window.restore = Some(window.content);
        }
        window.content = want;
        window.touched = true;
        window.toplevel.with_pending_state(|state| {
            state.size = Some((want.size.x as i32, want.size.y as i32).into());
        });
        window.toplevel.send_pending_configure();
    }

    fn press_button(&mut self, index: usize, button: ChromeButton) {
        use wayland_protocols::xdg::shell::server::xdg_toplevel;

        match button {
            ChromeButton::Close => {
                let seat = self.dock_slot(0);
                let window = &mut self.windows[index];
                if !window.going {
                    window.going = true;
                    window.motion = Some(Motion::new(Phase::Closing, seat));
                }
            }
            ChromeButton::Minimize => {
                let seat = self.dock_slot(self.minimized().len());
                self.windows[index].motion = Some(Motion::new(Phase::Minimizing, seat));
                let id = self.windows[index].id;
                self.focus_next(Some(id));
            }
            ChromeButton::Maximize | ChromeButton::Restore => {
                let chrome = self.chrome;
                let zone =
                    smithay::desktop::layer_map_for_output(&self.output).non_exclusive_zone();
                let zone = Rect::new(
                    Vec2::new(f64::from(zone.loc.x), f64::from(zone.loc.y)),
                    Vec2::new(f64::from(zone.size.w), f64::from(zone.size.h)),
                );
                let view = Rect::new(self.in_world(zone.origin), zone.size / self.camera.zoom);
                let room = view.size.min(zone.size);
                let full = Rect::new(view.center() - room / 2.0, room);
                let window = &mut self.windows[index];
                let want = match window.restore.take() {
                    Some(back) => back,
                    None => {
                        window.restore = Some(window.content);
                        Rect::new(
                            Vec2::new(full.origin.x + chrome.edge, full.origin.y + chrome.title),
                            Vec2::new(
                                full.size.x - chrome.edge * 2.0,
                                full.size.y - chrome.title - chrome.edge,
                            ),
                        )
                        .round_out()
                    }
                };
                window.content = want;
                window.touched = true;
                let full = window.restore.is_some();
                window.toplevel.with_pending_state(|state| {
                    state.size = Some((want.size.x as i32, want.size.y as i32).into());
                    if full {
                        state.states.set(xdg_toplevel::State::Maximized);
                    } else {
                        state.states.unset(xdg_toplevel::State::Maximized);
                    }
                });
                window.toplevel.send_pending_configure();
            }
        }
    }

    fn remember_icon(&mut self, index: usize) {
        let name = self.windows[index].app.clone();
        if name.is_empty() || self.app_icons.contains_key(&name) || self.no_icon.contains(&name) {
            return;
        }
        let wanted = self.dock.icon as u32;
        let art = crate::apps::find(&name)
            .and_then(|entry| entry.icon)
            .and_then(|path| drifter_core::image::load(&path).ok())
            .map(|image| image.scaled(wanted, wanted));
        let Some(image) = art else {
            eprintln!("drifter: no dock icon for {name}");
            self.no_icon.insert(name);
            return;
        };
        match upload(&mut self.renderer, Some(&image), "dock icon") {
            Ok(Some(buffer)) => {
                self.app_icons.insert(name, buffer);
            }
            Ok(None) => {}
            Err(error) => eprintln!("drifter: {error}"),
        }
    }

    fn drag(&mut self) {
        match self.grab {
            Grab::None => {}
            Grab::Move { id, offset } => {
                let world = self.in_world(self.pointer);
                if let Some(window) = self.windows.iter_mut().find(|window| window.id == id) {
                    window.content.origin = (world - offset).round();
                    window.touched = true;
                }
                self.zone = match self.modifier {
                    true => None,
                    false => {
                        let view = self.camera.visible_rect(self.screen);
                        self.snap.zone_at(self.in_world(self.pointer), view)
                    }
                };
            }
            Grab::Resize {
                id,
                grip,
                from,
                start,
            } => {
                let chrome = self.chrome;
                let moved = self.in_world(self.pointer) - from;
                let least = Vec2::new(160.0, 80.0);
                let wanted = chrome.resize(start, grip, moved, least);
                if let Some(window) = self.windows.iter_mut().find(|window| window.id == id) {
                    window.touched = true;
                    window.toplevel.with_pending_state(|state| {
                        state.size = Some((wanted.size.x as i32, wanted.size.y as i32).into());
                    });
                    window.toplevel.send_pending_configure();
                }
            }
        }
    }

    fn tell_button(&mut self, code: u32, state: ButtonState, time: u32) {
        use smithay::input::pointer::ButtonEvent;
        use smithay::utils::SERIAL_COUNTER;

        let serial = SERIAL_COUNTER.next_serial();
        if state == ButtonState::Released {
            if let (Grab::Move { id, .. }, Some(zone)) = (&self.grab, self.zone)
                && let Some(index) = self.place_of(*id)
            {
                self.land(index, zone);
            }
            self.zone = None;
            self.grab = Grab::None;
            self.held = None;
            if let Some(at) = self.eaten.iter().position(|eaten| *eaten == code) {
                self.eaten.swap_remove(at);
                self.kick();
                return;
            }
        }
        if state == ButtonState::Pressed {
            if self.press_own(code) {
                self.eaten.push(code);
                self.dismiss_popups();
                self.kick();
                return;
            }
            match self.surface_under() {
                Some(Hit {
                    index: Some(index),
                    popup: false,
                    ..
                }) => {
                    self.focus_window(index);
                }
                Some(Hit {
                    layer: true,
                    surface,
                    ..
                }) => self.focus_layer(&surface),
                _ => {}
            }
        }

        let pointer = self.wayland.pointer.clone();
        pointer.button(
            self,
            &ButtonEvent {
                button: code,
                state,
                serial,
                time,
            },
        );
        pointer.frame(self);
    }

    fn press_own(&mut self, code: u32) -> bool {
        if let Some((_, rect)) = self.prompt_art.as_ref() {
            let rect = *rect;
            if code == BTN_LEFT {
                let look = drifter_core::prompt::Look::default();
                match look.hit(rect.size, self.pointer - rect.origin) {
                    Some(drifter_core::prompt::Press::Go) => self.prompt_submit(),
                    Some(drifter_core::prompt::Press::Cancel) => self.prompt_cancel(),
                    None => {}
                }
            }
            return true;
        }
        if self.press_card(code) {
            return true;
        }
        if self.press_overview() {
            return true;
        }
        let had_menu = self.popover.is_some();
        if self.press_popover() {
            return true;
        }
        if self.press_tray(code, had_menu) {
            return true;
        }
        if code == BTN_RIGHT && self.open_context() {
            return true;
        }
        if self.press_dock() {
            return true;
        }
        if self.over_drawer() {
            return true;
        }
        self.take_grab()
    }

    fn tell_axis(
        &mut self,
        event: &impl smithay::backend::input::PointerAxisEvent<LibinputInputBackend>,
    ) {
        use smithay::backend::input::{Axis, AxisSource};
        use smithay::input::pointer::{AxisFrame, PointerHandle};

        let source = event.source();
        let mut frame = AxisFrame::new(event.time_msec()).source(source);
        for axis in [Axis::Horizontal, Axis::Vertical] {
            if let Some(amount) = event.amount(axis) {
                frame = frame.value(axis, amount);
                if amount == 0.0 && source == AxisSource::Finger {
                    frame = frame.stop(axis);
                }
            }
            if let Some(ticks) = event.amount_v120(axis) {
                frame = frame.v120(axis, ticks as i32);
            }
        }

        let pointer: PointerHandle<State> = self.wayland.pointer.clone();
        pointer.axis(self, frame);
        pointer.frame(self);
    }

    fn tell_keyboard(&mut self, event: &impl KeyboardKeyEvent<LibinputInputBackend>) {
        use smithay::input::keyboard::FilterResult;
        use smithay::utils::SERIAL_COUNTER;

        let keyboard = self.wayland.keyboard.clone();
        let time = event.time_msec();
        let code = event.key_code().raw();
        let pressed = event.state() == KeyState::Pressed;
        let shortcut = keyboard.input::<Shortcut, _>(
            self,
            event.key_code(),
            event.state(),
            SERIAL_COUNTER.next_serial(),
            time,
            |_, held, _| {
                if let Some(key) = crate::media::key(code) {
                    return FilterResult::Intercept(match pressed {
                        true => Shortcut::Media(key),
                        false => Shortcut::Swallow,
                    });
                }
                if code == KEY_PRINT {
                    return FilterResult::Intercept(match pressed {
                        true => Shortcut::Shot,
                        false => Shortcut::Swallow,
                    });
                }
                match (pressed && held.alt, code) {
                    (true, KEY_TAB) => {
                        FilterResult::Intercept(Shortcut::Switch { back: held.shift })
                    }
                    (true, KEY_F4) => FilterResult::Intercept(Shortcut::Close),
                    _ => FilterResult::Forward,
                }
            },
        );
        match shortcut {
            Some(Shortcut::Switch { back }) => self.switch_step(back),
            Some(Shortcut::Close) => self.close_focused(),
            Some(Shortcut::Media(key)) => {
                if let Some(media) = self.media.as_ref() {
                    media.press(key);
                }
            }
            Some(Shortcut::Shot) => {
                if self.lock.is_none() {
                    self.shot_wanted = true;
                    self.kick_now();
                }
            }
            Some(Shortcut::Swallow) | None => {}
        }
        if self.switch.is_some() && !keyboard.modifier_state().alt {
            self.switch_commit();
        }
    }

    fn switch_step(&mut self, back: bool) {
        if self.lock.is_some() {
            return;
        }
        let at = match self.switch.as_ref() {
            Some(open) => drifter_core::switcher::step(open.at, open.order.len(), back),
            None => {
                let order: Vec<u64> = self
                    .windows
                    .iter()
                    .rev()
                    .filter(|window| window.placed && !window.going)
                    .map(|window| window.id)
                    .collect();
                if order.is_empty() {
                    return;
                }
                let at = match order.len() {
                    1 => 0,
                    count => drifter_core::switcher::step(0, count, back),
                };
                let layout =
                    drifter_core::switcher::Look::default().layout(self.screen, order.len());
                let apps: Vec<String> = order
                    .iter()
                    .filter_map(|id| self.windows.iter().find(|window| window.id == *id))
                    .map(|window| window.app.clone())
                    .collect();
                let icons = apps
                    .iter()
                    .map(|app| self.switch_icon(app, layout.icon.round() as u32))
                    .collect();
                self.switch = Some(Switch {
                    order,
                    at,
                    layout,
                    art: None,
                    icons,
                });
                at
            }
        };
        let Some(open) = self.switch.as_ref() else {
            return;
        };
        let windows: Vec<&crate::shell::Window> = open
            .order
            .iter()
            .filter_map(|id| self.windows.iter().find(|window| window.id == *id))
            .collect();
        let letters: Vec<Option<char>> = windows
            .iter()
            .zip(&open.icons)
            .map(|(window, icon)| match icon.is_some() {
                true => None,
                false => window
                    .app
                    .rsplit('.')
                    .next()
                    .and_then(|name| name.chars().next())
                    .or_else(|| window.title.chars().next())
                    .or(Some('?')),
            })
            .collect();
        let title = windows
            .get(at)
            .map(|window| match window.title.is_empty() {
                true => window.app.clone(),
                false => window.title.clone(),
            })
            .unwrap_or_default();
        let image = open.layout.paint(at, &title, &letters, self.font.as_ref());
        let art = match upload(&mut self.renderer, Some(&image), "switcher") {
            Ok(art) => art,
            Err(error) => {
                eprintln!("drifter: {error}");
                None
            }
        };
        if let Some(open) = self.switch.as_mut() {
            open.at = at;
            open.art = art;
        }
        self.kick_now();
    }

    fn show_hud(&mut self, reading: drifter_core::hud::Reading) {
        let look = drifter_core::hud::Look::default();
        let image = look.paint(reading, self.font.as_ref());
        match upload(&mut self.renderer, Some(&image), "hud") {
            Ok(Some(art)) => {
                self.hud = Some((art, look.rect(self.screen), Instant::now() + HUD_TIME));
            }
            Ok(None) => {}
            Err(error) => eprintln!("drifter: {error}"),
        }
        self.kick_now();
    }

    fn prompt_begin(&mut self, request: crate::polkit::Request) {
        self.prompts.push(Pending {
            request,
            pad: Pad::new(),
            error: None,
            busy: false,
        });
        self.paint_prompt();
    }

    fn prompt_end(&mut self, cookie: &str, granted: bool) {
        if let Some(at) = self
            .prompts
            .iter()
            .position(|pending| pending.request.cookie == cookie)
        {
            let pending = self.prompts.remove(at);
            let _ = pending.request.reply.try_send(granted);
        }
        self.paint_prompt();
    }

    fn prompt_cancel(&mut self) {
        if let Some(cookie) = self.prompts.first().map(|p| p.request.cookie.clone()) {
            self.prompt_end(&cookie, false);
        }
    }

    fn prompt_submit(&mut self) {
        let Some(pending) = self.prompts.first_mut() else {
            return;
        };
        if pending.busy || pending.pad.is_empty() {
            return;
        }
        pending.busy = true;
        pending.error = None;
        let password = pending.pad.take();
        let user = pending.request.user.clone();
        let cookie = pending.request.cookie.clone();
        let reply = self.checked.clone();
        let spawned = std::thread::Builder::new()
            .name("drifter-polkit-check".to_string())
            .spawn(move || {
                let outcome = crate::polkit::check(&user, &cookie, password);
                let _ = reply.send((cookie, outcome));
            });
        if spawned.is_err() {
            pending.busy = false;
            pending.error = Some("The password cannot be checked right now.".into());
        }
        self.paint_prompt();
    }

    fn prompt_checked(&mut self, cookie: &str, outcome: crate::polkit::Outcome) {
        match outcome {
            crate::polkit::Outcome::Granted => self.prompt_end(cookie, true),
            crate::polkit::Outcome::Refused(message) => {
                if let Some(pending) = self
                    .prompts
                    .iter_mut()
                    .find(|pending| pending.request.cookie == cookie)
                {
                    pending.busy = false;
                    pending.error = Some(message);
                    pending.pad.wipe();
                }
                self.paint_prompt();
            }
        }
    }

    fn prompt_key(&mut self, key: LockKey) {
        let Some(pending) = self.prompts.first_mut() else {
            return;
        };
        if pending.busy {
            return;
        }
        match key {
            LockKey::Letter(letter) => {
                pending.pad.push(letter);
            }
            LockKey::Back => pending.pad.backspace(),
            LockKey::Enter => {
                self.prompt_submit();
                return;
            }
            LockKey::Clear => {
                self.prompt_cancel();
                return;
            }
            LockKey::Other => return,
        }
        self.paint_prompt();
    }

    fn paint_prompt(&mut self) {
        let (Some(pending), Some(font)) = (self.prompts.first(), self.font.as_ref()) else {
            self.prompt_art = None;
            self.kick_now();
            return;
        };
        let ask = drifter_core::prompt::Ask {
            message: pending.request.message.clone(),
            user: pending.request.user.clone(),
            typed: pending.pad.len(),
            error: pending.error.clone(),
            busy: pending.busy,
        };
        let look = drifter_core::prompt::Look::default();
        let image = look.paint(&ask, font);
        let rect = look.rect(self.screen, &ask, font);
        match upload(&mut self.renderer, Some(&image), "password prompt") {
            Ok(Some(art)) => self.prompt_art = Some((art, rect)),
            Ok(None) => {}
            Err(error) => eprintln!("drifter: {error}"),
        }
        self.kick_now();
    }

    fn show_note(&mut self, note: drifter_core::notify::Note) {
        let Some(font) = self.font.as_ref() else {
            return;
        };
        let look = drifter_core::notify::Look::default();
        let tall = look.height(&note, font);
        let image = look.paint(&note, font);
        let art = match upload(&mut self.renderer, Some(&image), "notification") {
            Ok(Some(art)) => art,
            Ok(None) => return,
            Err(error) => {
                eprintln!("drifter: {error}");
                return;
            }
        };
        let until = note
            .expire
            .map(|ms| Instant::now() + Duration::from_millis(u64::from(ms)));
        let card = Card {
            note,
            art,
            tall,
            until,
        };
        match self
            .cards
            .iter()
            .position(|held| held.note.id == card.note.id)
        {
            Some(at) => self.cards[at] = card,
            None => self.cards.insert(0, card),
        }
        while self.cards.len() > drifter_core::notify::MOST {
            if let Some(old) = self.cards.pop() {
                self.tell_closed(old.note.id, 1);
            }
        }
        self.kick_now();
    }

    fn close_note(&mut self, id: u32, reason: u32) {
        let before = self.cards.len();
        self.cards.retain(|card| card.note.id != id);
        if self.cards.len() != before {
            self.tell_closed(id, reason);
            self.kick_now();
        }
    }

    fn tell_closed(&self, id: u32, reason: u32) {
        if let Some(bus) = self.notes_bus.as_ref() {
            bus.closed(id, reason);
        }
    }

    fn card_rects(&self) -> Vec<Rect> {
        let heights: Vec<f64> = self.cards.iter().map(|card| card.tall).collect();
        drifter_core::notify::Look::default().stack(self.screen, &heights)
    }

    fn expire_cards(&mut self, now: Instant) {
        let rects = self.card_rects();
        let over = rects.iter().position(|rect| rect.contains(self.pointer));
        let mut gone = Vec::new();
        for (at, card) in self.cards.iter_mut().enumerate() {
            if Some(at) == over {
                if let Some(until) = card.until.as_mut() {
                    *until = (*until).max(now + Duration::from_secs(2));
                }
                continue;
            }
            if card.until.is_some_and(|until| now >= until) {
                gone.push(card.note.id);
            }
        }
        for id in gone {
            self.close_note(id, 1);
        }
    }

    fn press_card(&mut self, code: u32) -> bool {
        let Some(at) = self
            .card_rects()
            .iter()
            .position(|rect| rect.contains(self.pointer))
        else {
            return false;
        };
        let id = self.cards[at].note.id;
        if code == BTN_LEFT
            && let (Some(key), Some(bus)) = (
                self.cards[at].note.default_action(),
                self.notes_bus.as_ref(),
            )
        {
            bus.action(id, key);
        }
        self.close_note(id, 2);
        true
    }

    fn switch_commit(&mut self) {
        let Some(open) = self.switch.take() else {
            return;
        };
        if let Some(place) = open.order.get(open.at).and_then(|id| self.place_of(*id)) {
            self.bring(place);
        }
        self.kick_now();
    }

    fn switch_elements(&self, elements: &mut Vec<Skin>) {
        let Some(open) = self.switch.as_ref() else {
            return;
        };
        let Some(art) = open.art.as_ref() else {
            return;
        };
        let panel = open.layout.panel;
        for (index, icon) in open.icons.iter().enumerate() {
            if let Some(icon) = icon {
                let rect = open.layout.icon_rect(index);
                let rect = Rect::new(panel.origin + rect.origin, rect.size);
                elements.push(Skin::Texture(element(icon, rect, 1.0)));
            }
        }
        elements.push(Skin::Texture(element(art, panel, 1.0)));
    }

    fn switch_icon(&mut self, app: &str, side: u32) -> Option<TextureBuffer<GlesTexture>> {
        let key = format!("{app}@{side}");
        if let Some(held) = self.app_icons.get(&key) {
            return Some(held.clone());
        }
        if app.is_empty() || self.no_icon.contains(app) {
            return None;
        }
        let image = crate::apps::find(app)
            .and_then(|entry| entry.icon)
            .and_then(|path| drifter_core::image::load(&path).ok())
            .map(|image| image.scaled(side, side))?;
        let buffer = upload(&mut self.renderer, Some(&image), "switcher icon")
            .ok()
            .flatten()?;
        self.app_icons.insert(key, buffer.clone());
        Some(buffer)
    }

    fn bring(&mut self, place: usize) {
        if self.windows[place].hidden {
            let index = self.windows[..place]
                .iter()
                .filter(|window| window.hidden)
                .count();
            let seat = self.dock_slot(index);
            self.windows[place].hidden = false;
            self.windows[place].motion = Some(Motion::new(Phase::Restoring, seat));
        }
        let full = self.windows[place].full.is_some();
        let center = self.windows[place].content.center();
        let index = self.focus_window(place);
        if !full {
            self.fly_to(Camera::at_work_zoom(center));
        }
        let _ = index;
    }

    fn close_focused(&mut self) {
        if self.lock.is_some() {
            return;
        }
        if let Some(place) = self.focus.and_then(|id| self.place_of(id)) {
            self.press_button(place, ChromeButton::Close);
        }
    }

    fn input(&mut self, event: InputEvent<LibinputInputBackend>) {
        self.last_input = Instant::now();
        if self.lock.is_none()
            && !self.prompts.is_empty()
            && let InputEvent::Keyboard { event } = &event
        {
            let key = self.read_key(event);
            self.prompt_key(key);
            return;
        }
        if self.lock.is_some() && !matches!(event, InputEvent::DeviceAdded { .. }) {
            self.locked_input(event);
            return;
        }
        match event {
            InputEvent::PointerMotion { event } => {
                let delta = Vec2::new(event.delta_x(), event.delta_y());
                let bare = Vec2::new(event.delta_x_unaccel(), event.delta_y_unaccel());
                match self.held_pointer() {
                    Some(None) => {}
                    Some(Some(area)) => {
                        let far = area.max() - Vec2::splat(1.0);
                        self.pointer =
                            (self.pointer + delta).clamp(area.origin, far.max(area.origin));
                        self.tell_pointer(event.time_msec());
                    }
                    None => {
                        self.pointer_to(self.pointer + delta);
                        self.drag();
                        self.hover();
                        self.tell_pointer(event.time_msec());
                    }
                }
                self.tell_relative(delta, bare, event.time());
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let x = event.x_transformed(self.screen.x as i32);
                let y = event.y_transformed(self.screen.y as i32);
                self.pointer_to(Vec2::new(x, y));
                self.drag();
                self.hover();
                self.tell_pointer(event.time_msec());
            }
            InputEvent::PointerButton { event } => {
                self.tell_button(event.button_code(), event.state(), event.time_msec());
            }
            InputEvent::Keyboard { event }
                if event.state() == KeyState::Pressed
                    && event.key_code().raw() == ESCAPE
                    && self.modifier =>
            {
                let middle = self.screen * 0.5;
                self.open_popover(drifter_core::power::items(), middle, Target::Desktop);
            }
            InputEvent::Keyboard { event }
                if event.state() == KeyState::Pressed
                    && event.key_code().raw() == KEY_L
                    && self.modifier =>
            {
                self.lock_now();
            }
            InputEvent::PointerAxis { event } => {
                use smithay::backend::input::{Axis, PointerAxisEvent};
                if self.modifier {
                    let steps = event
                        .amount_v120(Axis::Vertical)
                        .map(|raw| -raw / 120.0)
                        .or_else(|| event.amount(Axis::Vertical).map(|raw| -raw / 10.0))
                        .unwrap_or(0.0);
                    if steps != 0.0 {
                        self.zoom_by(steps);
                    }
                    return;
                }
                self.tell_axis(&event);
            }
            InputEvent::DeviceAdded { mut device } => {
                use smithay::reexports::input::AccelProfile;
                if device.config_accel_is_available() {
                    let _ = device.config_accel_set_profile(AccelProfile::Flat);
                    let _ = device.config_accel_set_speed(0.0);
                }
                if device.config_tap_finger_count() > 0 {
                    let _ = device.config_tap_set_enabled(true);
                }
                if device
                    .config_scroll_methods()
                    .contains(&smithay::reexports::input::ScrollMethod::TwoFinger)
                {
                    let _ = device.config_scroll_set_method(
                        smithay::reexports::input::ScrollMethod::TwoFinger,
                    );
                }
                println!("drifter: input {}", device.name());
            }
            InputEvent::GestureSwipeBegin { event } => {
                use smithay::backend::input::GestureBeginEvent;
                self.fingers = event.fingers();
            }
            InputEvent::GestureSwipeUpdate { event } => {
                use smithay::backend::input::GestureSwipeUpdateEvent;
                if self.fingers < 3 {
                    return;
                }
                let moved = Vec2::new(event.delta_x(), event.delta_y());
                let mut camera = self.camera;
                camera.pan_by_screen(moved * -1.0);
                camera.clamp_to_world(self.world, self.screen);
                self.camera = camera;
                self.flight = None;
            }
            InputEvent::GestureSwipeEnd { .. } => self.fingers = 0,
            InputEvent::Keyboard { event } => {
                let code = event.key_code().raw();
                if self.holds.holds(code) {
                    self.modifier = event.state() == KeyState::Pressed;
                    if self.modifier {
                        self.corner_since = None;
                    }
                }
                self.tell_keyboard(&event);
            }
            _ => {}
        }
    }

    fn drop_held_input(&mut self) {
        self.modifier = false;
        self.fingers = 0;
        self.grab = Grab::None;
        self.zone = None;
        self.held = None;
        self.dwell = None;
        self.corner_since = None;
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

        if lock_marker().is_some_and(|marker| marker.exists()) {
            state.lock_now();
        }
        state.draw()?;

        for item in state.login.clone() {
            crate::apps::run(&item, &state.socket);
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

pub fn open(
    session: &mut LibSeatSession,
    seat_events: LibSeatSessionNotifier,
    config: &Config,
    path: &Path,
    virtual_size: (u32, u32),
    background: [f32; 4],
    wallpaper: Option<&Image>,
) -> Result<Screen, String> {
    let events: EventLoop<'static, State> =
        EventLoop::try_new().map_err(|e| format!("cannot create the event loop: {e}"))?;
    let signals = Signals::new(&[
        Signal::SIGUSR1,
        Signal::SIGUSR2,
        Signal::SIGTERM,
        Signal::SIGHUP,
        Signal::SIGINT,
    ])
    .map_err(|e| format!("cannot watch signals: {e}"))?;

    let fd = session
        .open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;

    let drm_fd = DrmDeviceFd::new(DeviceFd::from(fd));
    let render_node = smithay::backend::drm::DrmNode::from_file(drm_fd.clone())
        .ok()
        .map(|node| {
            node.node_with_type(smithay::backend::drm::NodeType::Render)
                .and_then(Result::ok)
                .unwrap_or(node)
        });
    let gbm = GbmDevice::new(drm_fd.clone()).map_err(|e| format!("gbm device: {e}"))?;
    let (drm, drm_events) =
        DrmDevice::new(drm_fd, true).map_err(|e| format!("cannot open drm device: {e}"))?;
    let mut drm = drm;

    let resources = drm
        .resource_handles()
        .map_err(|e| format!("cannot read drm resources: {e}"))?;

    let (conn_handle, conn) = resources
        .connectors()
        .iter()
        .filter_map(|handle| drm.get_connector(*handle, false).ok().map(|c| (*handle, c)))
        .find(|(_, c)| c.state() == connector::State::Connected && !c.modes().is_empty())
        .ok_or_else(|| "no connected connector with modes".to_string())?;

    let name = format!("{}-{}", conn.interface().as_str(), conn.interface_id());

    let modes: Vec<Mode> = conn
        .modes()
        .iter()
        .map(|m| {
            let (w, h) = m.size();
            Mode::new(
                u32::from(w),
                u32::from(h),
                m.vrefresh() * 1000,
                m.mode_type()
                    .contains(smithay::reexports::drm::control::ModeTypeFlags::PREFERRED),
            )
        })
        .collect();

    let want = (conn.interface() == connector::Interface::Virtual).then_some(virtual_size);
    let picked = mode::select(&modes, want)
        .ok_or_else(|| format!("connector {name} offered no usable mode"))?;

    let drm_mode = *conn
        .modes()
        .iter()
        .find(|m| {
            let (w, h) = m.size();
            u32::from(w) == picked.width
                && u32::from(h) == picked.height
                && m.vrefresh() * 1000 == picked.refresh_mhz
        })
        .ok_or_else(|| "selected mode vanished from the connector".to_string())?;

    let crtc = pick_crtc(&drm, &resources, &conn)
        .ok_or_else(|| format!("connector {name} has no usable crtc"))?;

    let surface = drm
        .create_surface(crtc, drm_mode, &[conn_handle])
        .map_err(|e| format!("cannot create drm surface: {e}"))?;

    let egl_display =
        unsafe { EGLDisplay::new(gbm.clone()) }.map_err(|e| format!("egl display: {e}"))?;
    let context = EGLContext::new(&egl_display).map_err(|e| format!("egl context: {e}"))?;
    let renderer_formats = context.dmabuf_render_formats().clone();
    let mut renderer =
        unsafe { GlesRenderer::new(context) }.map_err(|e| format!("gles renderer: {e}"))?;

    let allocator = GbmAllocator::new(
        gbm.clone(),
        GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
    );

    let (width, height) = drm_mode.size();
    let mode_source = OutputModeSource::Static {
        size: (i32::from(width), i32::from(height)).into(),
        scale: Scale::from(1.0),
        transform: Transform::Normal,
    };

    let compositor: Compositor = DrmCompositor::new(
        mode_source,
        surface,
        None,
        allocator,
        GbmFramebufferExporter::new(gbm.clone(), render_node),
        [DrmFourcc::Xrgb8888, DrmFourcc::Argb8888],
        renderer_formats,
        (64, 64).into(),
        Some(gbm.clone()),
    )
    .map_err(|e| format!("drm compositor: {e}"))?;

    let gl = describe(&mut renderer);
    let screen = Vec2::new(f64::from(width), f64::from(height));
    let world = config.world_size(screen);
    let start = Camera::at_work_zoom(world / 2.0);
    let screen_mode = smithay::output::Mode {
        size: (i32::from(width), i32::from(height)).into(),
        refresh: (drm_mode.vrefresh() * 1000) as i32,
    };
    let tunables = Tunables::default();
    let shapes = [
        Panel {
            height: config.drawers.top,
            ..Panel::default()
        },
        Panel {
            height: config.drawers.bottom,
            ..Panel::default()
        },
    ];

    let texture = upload(&mut renderer, wallpaper, "wallpaper")?;
    let wallpaper_element = texture.zip(wallpaper.map(|image| image.size()));
    let rect = wallpaper.map(|image| {
        drifter_core::wallpaper::place(
            config.workspace.wallpaper_mode,
            image.size(),
            screen,
            world,
            &start,
        )
        .target
    });

    let supplied = config
        .cursor_path()
        .and_then(|path| match drifter_core::image::load(&path) {
            Ok(image) => Some(image),
            Err(error) => {
                eprintln!("drifter: no cursor: {error}");
                None
            }
        });
    let (arrow_image, arrow_hotspot) = match supplied {
        Some(image) => (image, Vec2::ZERO),
        None => {
            let drawn = Cursor::default();
            (drawn.paint(), drawn.hotspot())
        }
    };
    let arrow_size = arrow_image.size();
    let arrow = upload(&mut renderer, Some(&arrow_image), "cursor")?
        .ok_or_else(|| "cannot build the cursor".to_string())?;

    let open_band = {
        let mut probe = Drawer::new(Edge::Top);
        probe.open = true;
        probe.advance(Duration::from_secs(1), Duration::from_millis(1));
        probe.band(screen, &shapes[Edge::Top.index()])
    };

    let chrome = Chrome::default();
    let mut skin = std::collections::HashMap::new();
    if let Some(dir) = config.window_dir() {
        for name in crate::shell::skin_names() {
            let path = dir.join(&name);
            match drifter_core::image::load(&path) {
                Ok(image) => {
                    let image = tile_skin(&name, image);
                    let size = image.size();
                    match upload(&mut renderer, Some(&image), &name) {
                        Ok(Some(buffer)) => {
                            skin.insert(name, (buffer, size));
                        }
                        Ok(None) => {}
                        Err(error) => eprintln!("drifter: {error}"),
                    }
                }
                Err(error) => eprintln!("drifter: no window art: {error}"),
            }
        }
    }
    let mut chrome = match skin.get("title-mid.png").zip(skin.get("edge-left.png")) {
        Some(((_, title), (_, edge))) => Chrome {
            title: title.y,
            edge: edge.x,
            ..chrome
        },
        None => chrome,
    };
    chrome.follow(&config.window);

    let mut built = Vec::new();
    for edge in Edge::ALL {
        let what = match edge {
            Edge::Top => "top drawer",
            Edge::Bottom => "bottom drawer",
        };
        let image = drawer::paint(
            edge,
            u32::from(width),
            &shapes[edge.index()],
            PANEL_COLOR,
            PANEL_ALPHA,
        );
        let buffer = upload(&mut renderer, Some(&image), what)?
            .ok_or_else(|| format!("cannot build the {what}"))?;
        built.push(buffer);
    }
    let panels: [TextureBuffer<GlesTexture>; 2] = built
        .try_into()
        .map_err(|_| "the drawer textures went missing".to_string())?;

    let tray = Tray {
        size: config.tray.size,
        ..Tray::default()
    };
    let display: Display<State> =
        Display::new().map_err(|e| format!("cannot start the wayland display: {e}"))?;
    let mut wayland = crate::shell::start(&display)?;
    {
        use smithay::backend::renderer::ImportDma;
        use smithay::wayland::dmabuf::DmabufFeedbackBuilder;

        let formats: Vec<_> = renderer.dmabuf_formats().into_iter().collect();
        let feedback = render_node.and_then(|node| {
            DmabufFeedbackBuilder::new(node.dev_id(), formats.clone())
                .build()
                .ok()
        });
        let handle = display.handle();
        let dmabuf = &mut wayland.extras.dmabuf;
        let global = match feedback {
            Some(feedback) => {
                dmabuf.create_global_with_default_feedback::<State>(&handle, &feedback)
            }
            None => dmabuf.create_global::<State>(&handle, formats),
        };
        wayland.extras.dmabuf_global = Some(global);
    }
    let output = smithay::output::Output::new(
        "drifter".to_string(),
        smithay::output::PhysicalProperties {
            size: (0, 0).into(),
            subpixel: smithay::output::Subpixel::Unknown,
            make: "Drifter".to_string(),
            model: "Drifter".to_string(),
        },
    );
    output.create_global::<State>(&wayland.handle);
    output.change_current_state(
        Some(screen_mode),
        Some(Transform::Normal),
        Some(smithay::output::Scale::Integer(1)),
        Some((0, 0).into()),
    );
    output.set_preferred(screen_mode);

    let (tray_bus, tray_icons) = match crate::tray::start() {
        Ok((bus, icons)) => (Some(bus), Some(icons)),
        Err(error) => {
            eprintln!("drifter: no tray: {error}");
            (None, None)
        }
    };
    let (notes_bus, note_events) = match crate::notes::start() {
        Ok((bus, events)) => (Some(bus), Some(events)),
        Err(error) => {
            eprintln!("drifter: no notifications: {error}");
            (None, None)
        }
    };
    let (media, media_readings) = match crate::media::start() {
        Ok((media, readings)) => (Some(media), Some(readings)),
        Err(error) => {
            eprintln!("drifter: no media keys: {error}");
            (None, None)
        }
    };
    let font = match Font::find(config.font_path(), tray.size) {
        Ok(font) => Some(font),
        Err(error) => {
            eprintln!("drifter: no clock: {error}");
            None
        }
    };

    let trash_look = Dock::default();
    let art = config.dock_dir();
    let side = trash_look.icon as u32;
    let from_art = |name: &str| -> Option<drifter_core::Image> {
        let dir = art.as_ref()?;
        match drifter_core::image::load(&dir.join(format!("{name}.png"))) {
            Ok(image) => Some(image.scaled(side, side)),
            Err(error) => {
                eprintln!("drifter: no dock art for {name}: {error}");
                None
            }
        }
    };

    let trash_image = from_art("trash")
        .unwrap_or_else(|| drifter_core::dock::paint_trash(side, trash_look.rule_ink()));
    let trash = upload(&mut renderer, Some(&trash_image), "trash")?;
    let cog_image = from_art("settings")
        .unwrap_or_else(|| drifter_core::dock::paint_cog(side, trash_look.rule_ink()));
    let cog = upload(&mut renderer, Some(&cog_image), "settings")?;
    let power_image = drifter_core::tray::paint_power(tray.button as u32, tray.ink);
    let power = upload(&mut renderer, Some(&power_image), "power")?;
    let lock_big = Font::find(config.font_path(), 56.0).ok();
    let lock_small = Font::find(config.font_path(), 15.0).ok();
    let (checked_tx, checked_rx) =
        smithay::reexports::calloop::channel::channel::<(String, crate::polkit::Outcome)>();
    let (paper_tx, paper_rx) =
        smithay::reexports::calloop::channel::channel::<(PathBuf, drifter_core::Image)>();
    let (verify_tx, verify_rx) =
        smithay::reexports::calloop::channel::channel::<(u64, crate::auth::Verdict)>();
    let spare = from_art("app");

    let pinned = dock_items(&mut renderer, config, side, spare.as_ref())?;

    let dot_image = drifter_core::dock::paint_dot(
        (trash_look.icon / 9.0).max(3.0) as u32,
        trash_look.rule_ink(),
    );
    let dot = upload(&mut renderer, Some(&dot_image), "running dot")?;
    let glow_image =
        drifter_core::corner::paint_glow(drifter_core::corner::REACH as u32, [255, 214, 160]);
    let glow = upload(&mut renderer, Some(&glow_image), "hot corner")?;

    let dock_look = Dock::default();
    let rule_image = drifter_core::Image {
        width: 1,
        height: 1,
        rgba: vec![
            dock_look.rule[0] / 2,
            dock_look.rule[1] / 2,
            dock_look.rule[2] / 2,
            128,
        ],
    };
    let rule = upload(&mut renderer, Some(&rule_image), "dock divider")?;
    let plain_image = drifter_core::Image {
        width: 1,
        height: 1,
        rgba: vec![255, 255, 255, 255],
    };
    let plain = upload(&mut renderer, Some(&plain_image), "outline")?;

    let mut libinput = Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
    let seat_name = session.seat();
    libinput
        .udev_assign_seat(&seat_name)
        .map_err(|_| format!("cannot take input on seat {seat_name}"))?;

    let state = State {
        output,
        wayland,
        windows: Vec::new(),
        popups: smithay::desktop::PopupManager::default(),
        display: Some(display),
        chrome,
        camera: start,
        world,
        mode: config.workspace.wallpaper_mode,
        flight: None,
        corner_since: None,
        grab: Grab::None,
        modifier: false,
        fingers: 0,
        holds: config.input.modifier,
        last_title: None,
        held: None,
        focus: None,
        next_window: 0,
        eaten: Vec::new(),
        frame_timer: false,
        cursor_status: smithay::input::pointer::CursorImageStatus::default_named(),
        named_cursors: std::collections::HashMap::new(),
        layer_focus: None,
        skin,
        started: Instant::now(),
        socket: String::new(),
        drm,
        libinput: libinput.clone(),
        compositor,
        renderer,
        wallpaper: wallpaper_element,
        panels,
        shapes,
        arrow,
        arrow_size,
        arrow_hotspot,
        font,
        tray,
        dock: Dock {
            settings: config.dock.settings,
            trash: config.dock.trash,
            ..Dock::default()
        },
        pinned,
        pinned_from: (config.dock.clone(), config.apps.clone()),
        apps_chosen: config.apps.clone(),
        dock_spare: spare,
        trash,
        cog,
        dot,
        glow,
        rule,
        app_icons: std::collections::HashMap::new(),
        no_icon: std::collections::HashSet::new(),
        format: config.tray.format.clone(),
        clock: None,
        stats: None,
        sampler: crate::stats::Sampler::new(&config.tray),
        sampled: None,
        icons: Vec::new(),
        popover: None,
        switch: None,
        lock: None,
        lock_big,
        lock_small,
        verify: verify_tx,
        tickets: 0,
        power,
        look: Look::default(),
        tip: None,
        tip_of: None,
        snap: Snap::default(),
        config_seen: None,
        last_input: Instant::now(),
        lock_after: config.lock.after,
        login: config.login.start.clone(),
        paper: Paper::from(config),
        paper_tx,
        config_path: Config::config_path(),
        zone: None,
        plain,
        titles: std::collections::HashMap::new(),
        hover_since: None,
        tray_bus,
        media,
        hud: None,
        notes_bus,
        cards: Vec::new(),
        copies: Vec::new(),
        prompts: Vec::new(),
        prompt_art: None,
        checked: checked_tx,
        copied: std::collections::HashSet::new(),
        cursor_count: 0,
        shot_wanted: false,
        drawers: [Drawer::new(Edge::Top), Drawer::new(Edge::Bottom)],
        tunables,
        screen,
        pointer: screen / 2.0,
        dwell: None,
        leave: None,
        last: Instant::now(),
        clear: Color32F::from(background),
        active: true,
        awaiting: false,
        ticking: false,
        handle: events.handle(),
        signal: events.get_signal(),
        failure: None,
        frames: 0,
    };

    let handle = events.handle();

    handle
        .insert_source(drm_events, |event, _, state: &mut State| match event {
            DrmEvent::VBlank(_) => {
                state.awaiting = false;
                if let Err(error) = state.compositor.frame_submitted() {
                    state.stop(format!("frame submit failed: {error}"));
                    return;
                }
                if let Err(error) = state.draw() {
                    state.stop(error);
                }
            }
            DrmEvent::Error(error) => eprintln!("drifter: drm event: {error}"),
        })
        .map_err(|e| format!("cannot watch drm events: {e}"))?;

    handle
        .insert_source(seat_events, |event, _, state: &mut State| match event {
            SessionEvent::PauseSession => {
                state.active = false;
                state.libinput.suspend();
                state.drm.pause();
                state.drop_held_input();
            }
            SessionEvent::ActivateSession => {
                state.active = true;
                state.awaiting = false;
                if state.libinput.resume().is_err() {
                    eprintln!("drifter: cannot take the input devices back");
                }
                state.drop_held_input();
                if let Err(error) = state.drm.activate(true) {
                    eprintln!("drifter: cannot take the screen back: {error}");
                }
                if let Err(error) = state.compositor.reset_state() {
                    eprintln!("drifter: cannot reset the compositor: {error}");
                }
                state.compositor.reset_buffers();
                state.last = Instant::now();
                state.kick();
            }
        })
        .map_err(|e| format!("cannot watch seat events: {e}"))?;

    handle
        .insert_source(
            LibinputInputBackend::new(libinput),
            |event, _, state: &mut State| {
                state.input(event);
                state.kick();
            },
        )
        .map_err(|e| format!("cannot watch input events: {e}"))?;

    if let Some(icons) = tray_icons {
        handle
            .insert_source(icons, |event, _, state: &mut State| {
                if let ChannelEvent::Msg(update) = event {
                    match update {
                        crate::tray::Update::Icons(icons) => state.take_icons(icons),
                        crate::tray::Update::Menu { service, items, at } => {
                            let at = Vec2::new(at.0, at.1);
                            state.open_popover(items, at, Target::Tray(service));
                        }
                    }
                    state.kick();
                }
            })
            .map_err(|e| format!("cannot watch the tray: {e}"))?;
    }

    if let Some(events) = note_events {
        handle
            .insert_source(events, |event, _, state: &mut State| {
                if let ChannelEvent::Msg(event) = event {
                    match event {
                        crate::notes::Event::Show(note) => state.show_note(note),
                        crate::notes::Event::Close(id) => state.close_note(id, 3),
                    }
                }
            })
            .map_err(|e| format!("cannot watch the notifications: {e}"))?;
    }

    if let Some(readings) = media_readings {
        handle
            .insert_source(readings, |event, _, state: &mut State| {
                if let ChannelEvent::Msg(reading) = event {
                    state.show_hud(reading);
                }
            })
            .map_err(|e| format!("cannot watch the media keys: {e}"))?;
    }

    handle
        .insert_source(checked_rx, |event, _, state: &mut State| {
            if let ChannelEvent::Msg((cookie, outcome)) = event {
                state.prompt_checked(&cookie, outcome);
            }
        })
        .map_err(|e| format!("cannot watch the password checks: {e}"))?;
    match crate::polkit::start() {
        Ok(requests) => {
            handle
                .insert_source(requests, |event, _, state: &mut State| {
                    if let ChannelEvent::Msg(event) = event {
                        match event {
                            crate::polkit::Event::Begin(request) => state.prompt_begin(request),
                            crate::polkit::Event::Cancel(cookie) => {
                                state.prompt_end(&cookie, false)
                            }
                        }
                    }
                })
                .map_err(|e| format!("cannot watch polkit: {e}"))?;
        }
        Err(error) => eprintln!("drifter: no password prompt: {error}"),
    }

    handle
        .insert_source(paper_rx, |event, _, state: &mut State| {
            if let ChannelEvent::Msg((path, image)) = event {
                state.take_wallpaper(path, image);
            }
        })
        .map_err(|e| format!("cannot watch the wallpaper: {e}"))?;

    handle
        .insert_source(verify_rx, |event, _, state: &mut State| {
            if let ChannelEvent::Msg((ticket, verdict)) = event {
                state.verdict(ticket, verdict);
                state.kick();
            }
        })
        .map_err(|e| format!("cannot watch the lock: {e}"))?;

    let socket = smithay::wayland::socket::ListeningSocketSource::new_auto()
        .map_err(|e| format!("cannot open a wayland socket: {e}"))?;
    let socket_name = socket.socket_name().to_string_lossy().into_owned();
    handle
        .insert_source(socket, |stream, _, state: &mut State| {
            if let Err(error) = state
                .wayland
                .handle
                .insert_client(stream, crate::shell::client_data())
            {
                eprintln!("drifter: cannot greet a client: {error}");
            }
        })
        .map_err(|e| format!("cannot watch the wayland socket: {e}"))?;

    let mut state = state;
    state.socket = socket_name.clone();
    crate::apps::start_x11(&socket_name);
    crate::portal::announce(&socket_name);
    let poll = state_poll_fd(&mut state);
    handle
        .insert_source(poll, |_, _, state: &mut State| {
            state.dispatch_clients();
            state.kick_now();
            Ok(smithay::reexports::calloop::PostAction::Continue)
        })
        .map_err(|e| format!("cannot watch wayland clients: {e}"))?;

    state.config_seen = std::fs::metadata(&state.config_path)
        .and_then(|meta| meta.modified())
        .ok();
    handle
        .insert_source(
            Timer::from_duration(CONFIG_WATCH),
            |_, _, state: &mut State| {
                state.watch_config();
                state.watch_idle();
                state.watch_wallpaper();
                TimeoutAction::ToDuration(CONFIG_WATCH)
            },
        )
        .map_err(|e| format!("cannot watch the config: {e}"))?;

    handle
        .insert_source(signals, |event, _, state: &mut State| {
            let edge = match event.signal() {
                Signal::SIGUSR1 => Edge::Top,
                Signal::SIGUSR2 => Edge::Bottom,
                _ => {
                    state.signal.stop();
                    return;
                }
            };
            state.toggle(edge);
            state.kick();
        })
        .map_err(|e| format!("cannot watch signals: {e}"))?;

    Ok(Screen {
        found: Found {
            device: path.to_path_buf(),
            connector: name,
            width,
            height,
            refresh: drm_mode.vrefresh(),
            renderer: gl,
            socket: socket_name.clone(),
            wallpaper: rect,
            camera: start,
            drawer: open_band,
            corner: shapes[Edge::Top.index()].radius,
        },
        events,
        state,
    })
}

impl State {
    fn lock_elements(&self) -> Vec<Skin> {
        let Some(lock) = self.lock.as_ref() else {
            return Vec::new();
        };
        match lock.art.as_ref() {
            Some(art) => vec![Skin::Texture(element(art, lock.rect, 1.0))],
            None => Vec::new(),
        }
    }

    fn lock_now(&mut self) -> bool {
        use smithay::input::pointer::MotionEvent;
        use smithay::utils::SERIAL_COUNTER;

        if self.lock.is_some() {
            return true;
        }
        let user = match crate::auth::ready() {
            Ok(user) => user,
            Err(error) => {
                eprintln!("drifter: cannot lock: {error}");
                return false;
            }
        };

        self.dismiss_popups();
        self.popover = None;
        self.switch = None;
        self.tip = None;
        self.tip_of = None;
        self.zone = None;
        self.grab = Grab::None;
        self.held = None;
        self.dwell = None;
        self.leave = None;
        self.corner_since = None;
        self.hover_since = None;
        self.flight = None;
        self.modifier = false;
        self.fingers = 0;
        for edge in Edge::ALL {
            self.set(edge, false);
        }

        let keyboard = self.wayland.keyboard.clone();
        let refocus = self
            .focus
            .and_then(|id| self.place_of(id))
            .map(|index| self.windows[index].toplevel.wl_surface().clone())
            .or_else(|| keyboard.current_focus());
        keyboard.set_focus(self, None, SERIAL_COUNTER.next_serial());

        let pointer = self.wayland.pointer.clone();
        let world = self.in_world(self.pointer);
        pointer.motion(
            self,
            None,
            &MotionEvent {
                location: smithay::utils::Point::from((world.x, world.y)),
                serial: SERIAL_COUNTER.next_serial(),
                time: self.started.elapsed().as_millis() as u32,
            },
        );
        pointer.frame(self);

        self.lock = Some(Lock {
            pad: Pad::new(),
            note: Note::Ask,
            checking: None,
            ticket: None,
            art: None,
            rect: Rect::new(Vec2::ZERO, Vec2::ZERO),
            shown: (String::new(), String::new(), usize::MAX, Note::Ask),
            user,
            refocus,
        });
        if let Some(marker) = lock_marker() {
            let _ = std::fs::write(marker, b"");
        }
        self.paint_lock();
        self.kick();
        true
    }

    fn paint_lock(&mut self) {
        let Some(lock) = self.lock.as_ref() else {
            return;
        };
        let clock = drifter_core::tray::now("%I:%M %p").unwrap_or_default();
        let date = drifter_core::tray::now("%A, %B %d").unwrap_or_default();
        let key = (clock, date, lock.pad.len(), lock.note);
        if lock.art.is_some() && lock.shown == key {
            return;
        }

        let look = drifter_core::lock::Look::default();
        let words = drifter_core::lock::Words {
            clock: &key.0,
            date: &key.1,
        };
        let image = drifter_core::lock::paint(
            &look,
            self.lock_big.as_ref(),
            self.lock_small.as_ref(),
            &words,
            key.2,
            key.3,
        );
        let rect = look.rect(self.screen);
        let art = match upload(&mut self.renderer, Some(&image), "lock") {
            Ok(art) => art,
            Err(error) => {
                eprintln!("drifter: {error}");
                None
            }
        };
        if let Some(lock) = self.lock.as_mut() {
            lock.art = art;
            lock.rect = rect;
            lock.shown = key;
        }
    }

    fn locked_input(&mut self, event: InputEvent<LibinputInputBackend>) {
        let InputEvent::Keyboard { event } = event else {
            return;
        };
        let key = self.read_key(&event);
        if event.state() != KeyState::Pressed {
            return;
        }
        let Some(lock) = self.lock.as_mut() else {
            return;
        };
        if let Some(since) = lock.checking
            && since.elapsed() < LOCK_PATIENCE
        {
            return;
        }

        match key {
            LockKey::Letter(letter) => {
                if lock.pad.push(letter) {
                    lock.note = Note::Ask;
                }
            }
            LockKey::Back => {
                lock.pad.backspace();
                lock.note = Note::Ask;
            }
            LockKey::Clear => {
                lock.pad.wipe();
                lock.note = Note::Ask;
            }
            LockKey::Enter => {
                if lock.pad.is_empty() {
                    return;
                }
                let mut typed = lock.pad.take();
                let secret = std::mem::take(&mut *typed);
                self.tickets += 1;
                let ticket = self.tickets;
                lock.checking = Some(Instant::now());
                lock.ticket = Some(ticket);
                lock.note = Note::Checking;
                let user = lock.user.clone();
                let sender = self.verify.clone();
                std::thread::spawn(move || {
                    let verdict = crate::auth::verify(&user, secret);
                    let _ = sender.send((ticket, verdict));
                });
            }
            LockKey::Other => {}
        }
        self.paint_lock();
    }

    fn read_key(&mut self, event: &impl KeyboardKeyEvent<LibinputInputBackend>) -> LockKey {
        use smithay::input::keyboard::{FilterResult, keysyms};
        use smithay::utils::SERIAL_COUNTER;

        let keyboard = self.wayland.keyboard.clone();
        let read = keyboard.input::<LockKey, _>(
            self,
            event.key_code(),
            event.state(),
            SERIAL_COUNTER.next_serial(),
            event.time_msec(),
            |_, _, handle| {
                let symbol = handle.modified_sym();
                let key = match symbol.raw() {
                    keysyms::KEY_Return | keysyms::KEY_KP_Enter => LockKey::Enter,
                    keysyms::KEY_BackSpace => LockKey::Back,
                    keysyms::KEY_Escape => LockKey::Clear,
                    _ => match symbol.key_char() {
                        Some(letter) => LockKey::Letter(letter),
                        None => LockKey::Other,
                    },
                };
                FilterResult::Intercept(key)
            },
        );
        read.unwrap_or(LockKey::Other)
    }

    fn verdict(&mut self, ticket: u64, verdict: crate::auth::Verdict) {
        use crate::auth::Verdict;

        let Some(lock) = self.lock.as_mut() else {
            return;
        };
        if lock.ticket != Some(ticket) {
            return;
        }
        lock.checking = None;
        lock.ticket = None;
        match verdict {
            Verdict::Granted => {
                self.unlock();
                return;
            }
            Verdict::Denied => lock.note = Note::Wrong,
            Verdict::Broken(error) => {
                eprintln!("drifter: the lock cannot check passwords: {error}");
                lock.note = Note::Wrong;
            }
        }
        self.paint_lock();
    }

    fn unlock(&mut self) {
        use smithay::reexports::wayland_server::Resource;
        use smithay::utils::SERIAL_COUNTER;

        let Some(mut lock) = self.lock.take() else {
            return;
        };
        if let Some(marker) = lock_marker() {
            let _ = std::fs::remove_file(marker);
        }
        lock.pad.wipe();
        if let Some(surface) = lock.refocus.take()
            && surface.is_alive()
        {
            let keyboard = self.wayland.keyboard.clone();
            keyboard.set_focus(self, Some(surface), SERIAL_COUNTER.next_serial());
        }
        self.last = Instant::now();
    }
}

fn state_poll_fd(
    state: &mut State,
) -> smithay::reexports::calloop::generic::Generic<std::os::fd::OwnedFd> {
    use smithay::reexports::calloop::{Interest, Mode as PollMode, generic::Generic};
    let fd = state
        .display
        .as_mut()
        .expect("the display was just made")
        .backend()
        .poll_fd()
        .try_clone_to_owned()
        .expect("cannot clone the wayland poll fd");
    Generic::new(fd, Interest::READ, PollMode::Level)
}

fn upload(
    renderer: &mut GlesRenderer,
    image: Option<&Image>,
    what: &str,
) -> Result<Option<TextureBuffer<GlesTexture>>, String> {
    match image {
        None => Ok(None),
        Some(image) => TextureBuffer::from_memory(
            renderer,
            &image.rgba,
            DrmFourcc::Abgr8888,
            (image.width as i32, image.height as i32),
            false,
            1,
            Transform::Normal,
            None,
        )
        .map(Some)
        .map_err(|e| format!("cannot upload {what}: {e}")),
    }
}

type Pinned = Vec<(drifter_core::Launch, Option<TextureBuffer<GlesTexture>>)>;

fn dock_items(
    renderer: &mut GlesRenderer,
    config: &Config,
    side: u32,
    spare: Option<&drifter_core::Image>,
) -> Result<Pinned, String> {
    let dock = &config.dock;
    let mut wanted: Vec<drifter_core::Launch> = crate::apps::defaults(dock, &config.apps)
        .into_iter()
        .filter(|item| {
            !dock
                .pinned
                .iter()
                .any(|held| held == item || (held.app().is_some() && held.app() == item.app()))
        })
        .collect();
    wanted.extend(dock.pinned.iter().cloned());
    let mut pinned = Vec::new();
    for item in wanted {
        let path = item
            .icon()
            .map(|path| path.to_path_buf())
            .or_else(|| item.app().and_then(crate::apps::find).and_then(|e| e.icon))
            .or_else(|| crate::apps::icon_file(&item.name()));
        let art = path
            .and_then(|path| match drifter_core::image::load(&path) {
                Ok(image) => Some(image),
                Err(error) => {
                    eprintln!("drifter: no dock icon for {}: {error}", item.name());
                    None
                }
            })
            .map(|image| image.scaled(side, side))
            .or_else(|| spare.cloned());
        let buffer = match art {
            Some(image) => upload(renderer, Some(&image), "dock icon")?,
            None => None,
        };
        pinned.push((item, buffer));
    }
    Ok(pinned)
}

fn lock_marker() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|dir| PathBuf::from(dir).join("drifter-locked"))
}

fn element(
    buffer: &TextureBuffer<GlesTexture>,
    rect: Rect,
    alpha: f32,
) -> TextureRenderElement<GlesTexture> {
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

const TILE_SPAN: u32 = 8192;

fn tile_skin(name: &str, image: Image) -> Image {
    let piece = ChromePiece::ALL.into_iter().find(|piece| {
        tiles(*piece) && (piece.file_name(true) == name || piece.file_name(false) == name)
    });
    match piece {
        Some(ChromePiece::TitleMid | ChromePiece::EdgeBottom) => {
            image.tiled(TILE_SPAN, image.height)
        }
        Some(_) => image.tiled(image.width, TILE_SPAN),
        None => image,
    }
}

fn tiles(piece: ChromePiece) -> bool {
    matches!(
        piece,
        ChromePiece::TitleMid
            | ChromePiece::EdgeLeft
            | ChromePiece::EdgeRight
            | ChromePiece::EdgeBottom
    )
}

fn cropped(
    buffer: &TextureBuffer<GlesTexture>,
    source: Rect,
    target: Rect,
) -> TextureRenderElement<GlesTexture> {
    faded(buffer, source, target, 1.0)
}

fn faded(
    buffer: &TextureBuffer<GlesTexture>,
    source: Rect,
    target: Rect,
    alpha: f32,
) -> TextureRenderElement<GlesTexture> {
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

fn describe(renderer: &mut GlesRenderer) -> String {
    use smithay::backend::renderer::gles::ffi;
    renderer
        .with_context(|gl| unsafe {
            let raw = gl.GetString(ffi::RENDERER);
            if raw.is_null() {
                "unknown".to_string()
            } else {
                std::ffi::CStr::from_ptr(raw as *const std::ffi::c_char)
                    .to_string_lossy()
                    .into_owned()
            }
        })
        .unwrap_or_else(|_| "unknown".to_string())
}

fn pick_crtc(
    device: &DrmDevice,
    resources: &smithay::reexports::drm::control::ResourceHandles,
    conn: &connector::Info,
) -> Option<crtc::Handle> {
    if let Some(encoder) = conn.current_encoder()
        && let Ok(info) = device.get_encoder(encoder)
        && let Some(crtc) = info.crtc()
    {
        return Some(crtc);
    }
    for encoder in conn.encoders() {
        if let Ok(info) = device.get_encoder(*encoder)
            && let Some(crtc) = resources.filter_crtcs(info.possible_crtcs()).first()
        {
            return Some(*crtc);
        }
    }
    None
}
