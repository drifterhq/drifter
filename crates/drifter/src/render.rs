use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use drifter_core::camera::{Camera, Flight};
use drifter_core::cursor::{self, Cursor};
use drifter_core::dock::{Dock, Slot};
use drifter_core::drawer::{self, Drawer, Edge, Panel};
use drifter_core::image::Image;
use drifter_core::menu::{Item, Look};
use drifter_core::mode::{self, Mode};
use drifter_core::text::Font;
use drifter_core::tray::Tray;
use drifter_core::window::{Chrome, Piece as ChromePiece};
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
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;
const SUPER_LEFT: u32 = 133;
const SUPER_RIGHT: u32 = 134;
const PANEL_COLOR: [u8; 3] = [16, 18, 26];
const PANEL_ALPHA: u8 = 235;
const TICK: Duration = Duration::from_millis(8);
const SECOND_TICK: Duration = Duration::from_millis(250);
const SETTLING: Duration = Duration::from_secs(2);

use smithay::backend::renderer::{ImportAll, RendererSuper, element::render_elements};

render_elements! {
    pub Piece<R> where R: ImportAll;
    Texture=TextureRenderElement<<R as RendererSuper>::TextureId>,
    Client=WaylandSurfaceRenderElement<R>,
}

type Skin = Piece<GlesRenderer>;

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
    Window(usize),
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
        index: usize,
        offset: Vec2,
    },
    Resize {
        index: usize,
        grip: drifter_core::window::Grip,
        from: Vec2,
        start: Rect,
    },
}

pub struct State {
    pub output: smithay::output::Output,
    pub wayland: crate::shell::Wayland,
    pub windows: Vec<crate::shell::Window>,
    pub popups: Vec<smithay::wayland::shell::xdg::PopupSurface>,
    display: Option<Display<State>>,
    chrome: Chrome,
    camera: Camera,
    world: Vec2,
    mode: drifter_core::WallpaperMode,
    flight: Option<(Flight, Instant)>,
    corner_since: Option<Instant>,
    grab: Grab,
    modifier: bool,
    held: Option<usize>,
    pub focus: Option<usize>,
    skin: std::collections::HashMap<String, (TextureBuffer<GlesTexture>, Vec2)>,
    started: Instant,
    socket: String,
    drm: DrmDevice,
    compositor: Compositor,
    renderer: GlesRenderer,
    wallpaper: Option<(TextureBuffer<GlesTexture>, Rect)>,
    panels: [TextureBuffer<GlesTexture>; 2],
    shapes: [Panel; 2],
    arrow: TextureBuffer<GlesTexture>,
    arrow_size: Vec2,
    arrow_hotspot: Vec2,
    font: Option<Font>,
    tray: Tray,
    dock: Dock,
    pinned: Vec<(String, Option<TextureBuffer<GlesTexture>>)>,
    trash: Option<TextureBuffer<GlesTexture>>,
    rule: Option<TextureBuffer<GlesTexture>>,
    app_icons: std::collections::HashMap<String, TextureBuffer<GlesTexture>>,
    format: String,
    clock: Option<(String, TextureBuffer<GlesTexture>, Rect)>,
    icons: Vec<(String, String, TextureBuffer<GlesTexture>)>,
    popover: Option<Popover>,
    look: Look,
    tip: Option<(TextureBuffer<GlesTexture>, Rect)>,
    tip_of: Option<String>,
    hover_since: Option<(Vec2, Instant)>,
    tray_bus: Option<crate::tray::Tray>,
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
        self.camera = flight.at(gone);
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
        self.drawers.iter().any(|drawer| !drawer.settled()) || self.flight.is_some()
    }

    fn pending(&self) -> Option<Duration> {
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
        if self.drawers[Edge::Top.index()].visible() && self.font.is_some() {
            return Some(SECOND_TICK);
        }
        None
    }

    fn elements(&mut self) -> Vec<Skin> {
        let mut elements: Vec<Skin> = vec![Skin::Texture(element(
            &self.arrow,
            cursor::rect(self.pointer, self.arrow_size, self.arrow_hotspot),
            1.0,
        ))];
        if let Some(open) = self.popover.as_ref() {
            elements.push(Skin::Texture(element(&open.art, open.rect, 1.0)));
        }
        if let Some((art, rect)) = self.tip.as_ref() {
            elements.push(Skin::Texture(element(art, *rect, 1.0)));
        }

        let popups: Vec<(
            Vec2,
            smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        )> = self
            .popups
            .iter()
            .rev()
            .filter_map(|popup| self.popup_at(popup))
            .collect();
        for (at, surface) in popups {
            let bare: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                render_elements_from_surface_tree(
                    &mut self.renderer,
                    &surface,
                    (at.x as i32, at.y as i32),
                    1.0,
                    1.0,
                    Kind::Unspecified,
                );
            elements.extend(bare.into_iter().map(Skin::Client));
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
            if let Some(buffer) = self.trash.as_ref() {
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
        if let Some(band) = self.tray_band() {
            for (slot, (_, _, buffer)) in self.icons.iter().enumerate() {
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

        let windows: Vec<(
            Rect,
            Vec2,
            bool,
            smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        )> = self
            .windows
            .iter()
            .rev()
            .filter(|window| window.placed && !window.hidden)
            .map(|window| {
                (
                    window.content,
                    window.inset,
                    self.focus
                        .and_then(|index| self.windows.get(index))
                        .map(|awake| awake.toplevel == window.toplevel)
                        .unwrap_or(false),
                    window.toplevel.wl_surface().clone(),
                )
            })
            .collect();
        let zoom = self.camera.zoom;
        for (content, inset, awake, surface) in windows {
            let at = self
                .camera
                .screen_from_world(content.origin - inset, self.screen);
            let client: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                render_elements_from_surface_tree(
                    &mut self.renderer,
                    &surface,
                    (at.x.round() as i32, at.y.round() as i32),
                    zoom,
                    1.0,
                    Kind::Unspecified,
                );
            elements.extend(client.into_iter().map(Skin::Client));
            self.dress(content, awake, &mut elements);
        }

        if let Some((buffer, rect)) = self.wallpaper.as_ref() {
            elements.push(Skin::Texture(element(
                buffer,
                self.wallpaper_rect(*rect),
                1.0,
            )));
        }
        elements
    }

    fn wallpaper_rect(&self, fitted: Rect) -> Rect {
        use drifter_core::WallpaperMode;
        match self.mode {
            WallpaperMode::Fixed => fitted,
            WallpaperMode::Pan => self.on_screen(Rect::new(Vec2::ZERO, self.world)),
            WallpaperMode::Parallax => {
                let view = self.camera.visible_rect(self.screen);
                let paper = fitted.size;
                let origin = drifter_core::wallpaper::origin(
                    WallpaperMode::Parallax,
                    view.origin,
                    paper,
                    view.size,
                    self.world,
                );
                Rect::new(
                    self.camera.screen_from_world(origin, self.screen),
                    paper * self.camera.zoom,
                )
            }
        }
    }

    fn popup_at(
        &self,
        popup: &smithay::wayland::shell::xdg::PopupSurface,
    ) -> Option<(
        Vec2,
        smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    )> {
        if !popup.alive() {
            return None;
        }
        let parent = popup.get_parent_surface()?;
        let anchor = self
            .windows
            .iter()
            .find(|window| window.toplevel.wl_surface() == &parent)
            .map(|window| window.content.origin - window.inset)
            .or_else(|| {
                self.popups
                    .iter()
                    .find(|other| other.wl_surface() == &parent)
                    .and_then(|other| self.popup_at(other))
                    .map(|(at, _)| at)
            })?;

        let place = popup.with_pending_state(|state| state.geometry.loc);
        let at = anchor + Vec2::new(f64::from(place.x), f64::from(place.y));
        Some((at.round(), popup.wl_surface().clone()))
    }

    fn dress(&self, content: Rect, awake: bool, elements: &mut Vec<Skin>) {
        let layout = self.chrome.layout(content);
        let show = |rect: Rect| self.on_screen(rect);
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
            if let Some((buffer, _)) = self.skin.get(&piece.file_name(awake)) {
                elements.push(Skin::Texture(element(buffer, show(rect), 1.0)));
            }
        }

        let bar = self.chrome.title_bar(content);
        let over = self
            .chrome
            .button_at(content, self.in_world(self.pointer), 3);
        for (slot, name) in ["close", "minimize", "maximize"].iter().enumerate() {
            let look = match (over == Some(slot), self.held == Some(slot)) {
                (true, true) => "-press",
                (true, false) => "-hover",
                _ => "",
            };
            let file = format!("button-{name}{look}.png");
            if let Some((buffer, size)) = self.skin.get(&file) {
                let place = self.chrome.button_rect(slot);
                let rect = Rect::new(bar.origin + place.origin, *size);
                elements.push(Skin::Texture(element(buffer, show(rect), 1.0)));
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

        let app = smithay::wayland::compositor::with_states(surface, |states| {
            states
                .data_map
                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                .and_then(|data| data.lock().ok().and_then(|data| data.app_id.clone()))
        })
        .unwrap_or_default();

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
        if !app.is_empty() {
            window.app = app;
        }

        if window.placed && (window.touched || !grew || window.born.elapsed() > SETTLING) {
            return;
        }

        let seen = view.origin;
        let middle = seen + (view.size - size) / 2.0;
        let top = middle.y.max(seen.y + chrome.title + chrome.edge);
        window.content.origin = Vec2::new(middle.x, top).round();
        window.placed = true;
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
        self.push_edge(dt);

        self.wind_the_clock();

        let elements = self.elements();
        let clear = self.clear;
        let frames = self.frames;
        let State {
            compositor,
            renderer,
            ..
        } = self;
        let result = compositor
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
            })?;

        if result.is_empty {
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

    fn tell_clients(&mut self) {
        let now = self.started.elapsed().as_millis() as u32;
        for window in &self.windows {
            crate::shell::send_frame(window.toplevel.wl_surface(), now);
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
                let name = self.pinned[index].0.clone();
                if let Some(exec) = crate::apps::find(&name).and_then(|entry| entry.exec) {
                    crate::apps::launch(&exec, &self.socket);
                }
            }
            Slot::Trash => {
                let trash = std::env::var("HOME").unwrap_or_default() + "/.local/share/Trash/files";
                if let Some(files) = crate::apps::defaults().get(1).cloned()
                    && let Some(exec) = crate::apps::find(&files).and_then(|entry| entry.exec)
                {
                    crate::apps::launch(&format!("{exec} {trash}"), &self.socket);
                }
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
                self.windows[place].hidden = false;
                self.raise(place);
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
        if items.is_empty() {
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
            Target::Window(index) => {
                let index = *index;
                match chosen.id {
                    0 => {
                        if let Some(window) = self.windows.get_mut(index) {
                            window.hidden = !window.hidden;
                        }
                        if self.windows.get(index).map(|w| w.hidden) == Some(false) {
                            self.raise(index);
                        }
                    }
                    _ => {
                        if let Some(window) = self.windows.get(index) {
                            window.toplevel.send_close();
                        }
                    }
                }
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
            && let Some(slot) = self.tray.icon_at(band, self.pointer, self.icons.len())
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
                    Slot::Pinned(index) => self.pinned[index].0.clone(),
                    Slot::Minimized(index) => hidden[index].clone(),
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
            && let Some(slot) = self.tray.icon_at(band, self.pointer, self.icons.len())
        {
            let service = self.icons[slot].0.clone();
            if let Some(bus) = self.tray_bus.as_ref() {
                bus.ask_menu(&service, self.pointer.x, self.pointer.y);
            }
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
                self.open_popover(items, at, Target::Window(place));
                return true;
            }
        }

        false
    }

    fn watch_corner(&mut self) {
        let size = self.tunables.hot_corner_size;
        let in_corner = self.pointer.x < size && self.pointer.y < size;

        if !in_corner {
            self.corner_since = None;
            return;
        }
        if self.overview() || self.flight.is_some() {
            return;
        }
        match self.corner_since {
            None => self.corner_since = Some(Instant::now()),
            Some(since) => {
                if Instant::now().duration_since(since) >= self.tunables.hot_corner_dwell {
                    self.corner_since = None;
                    let whole = Camera::fitting_world(self.screen, self.world);
                    self.fly_to(whole);
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
        let least = Camera::fit_zoom(self.screen, self.world);
        let wanted = (self.camera.zoom * (1.0 + steps * 0.12)).clamp(least, 4.0);
        let mut camera = self.camera;
        camera.zoom_at(self.pointer, wanted, self.screen);
        camera.clamp_to_world(self.world, self.screen);
        self.camera = camera;
        self.flight = None;
    }

    fn push_edge(&mut self, dt: Duration) {
        if !self.modifier {
            return;
        }
        let band = self.tunables.edge_band.max(2.0);
        let speed = self.tunables.edge_pan_speed * dt.as_secs_f64();
        let mut push = Vec2::ZERO;
        if self.pointer.x < band {
            push.x = -speed;
        }
        if self.pointer.x > self.screen.x - band {
            push.x = speed;
        }
        if self.pointer.y < band {
            push.y = -speed;
        }
        if self.pointer.y > self.screen.y - band {
            push.y = speed;
        }
        if push == Vec2::ZERO {
            return;
        }
        let mut camera = self.camera;
        camera.pan_by_screen(push);
        camera.clamp_to_world(self.world, self.screen);
        self.camera = camera;
    }

    fn press(&mut self, code: u32) {
        let Some(band) = self.tray_band() else {
            return;
        };
        let Some(slot) = self.tray.icon_at(band, self.pointer, self.icons.len()) else {
            return;
        };
        let click = match code {
            BTN_LEFT => crate::tray::Click::Left,
            BTN_MIDDLE => crate::tray::Click::Middle,
            _ => return,
        };
        if let Some(bus) = self.tray_bus.as_ref() {
            let service = &self.icons[slot].0;
            bus.click(service, click, self.pointer.x as i32, self.pointer.y as i32);
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
        let open = self.drawers[edge.index()].open;
        self.dwell = None;
        self.leave = None;
        self.set(edge, !open);
    }

    fn wanted(&self) -> Option<Edge> {
        for edge in Edge::ALL {
            if self.drawers[edge.index()].holds(
                self.pointer,
                self.screen,
                &self.shapes[edge.index()],
            ) {
                return Some(edge);
            }
        }
        drawer::edge_at(self.pointer, self.screen, self.tunables.edge_band)
    }

    fn pointer_to(&mut self, position: Vec2) {
        self.pointer = position.clamp(Vec2::ZERO, self.screen - Vec2::splat(1.0));
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

    fn tell_pointer(&mut self, time: u32) {
        use smithay::input::pointer::MotionEvent;
        use smithay::utils::SERIAL_COUNTER;

        let focus = self.under_pointer().map(|(index, _)| {
            let window = &self.windows[index];
            let at = window.content.origin - window.inset;
            (
                window.toplevel.wl_surface().clone(),
                smithay::utils::Point::from((at.x, at.y)),
            )
        });
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
    }

    fn raise(&mut self, index: usize) -> usize {
        let window = self.windows.remove(index);
        self.windows.push(window);
        let top = self.windows.len() - 1;
        self.focus = Some(top);
        self.wake_windows();
        top
    }

    fn wake_windows(&mut self) {
        use wayland_protocols::xdg::shell::server::xdg_toplevel;
        for (index, window) in self.windows.iter().enumerate() {
            let awake = self.focus == Some(index);
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
            .filter(|(_, window)| window.placed && !window.hidden)
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
        let index = self.raise(index);
        let window = &self.windows[index];
        let surface = window.toplevel.wl_surface().clone();
        let content = window.content;

        match grip {
            Grip::Button(slot) => {
                self.held = Some(slot);
                self.press_button(index, slot);
            }
            Grip::Title => {
                self.grab = Grab::Move {
                    index,
                    offset: self.in_world(self.pointer) - content.origin,
                };
            }
            other => {
                self.grab = Grab::Resize {
                    index,
                    grip: other,
                    from: self.in_world(self.pointer),
                    start: content,
                };
            }
        }

        let serial = smithay::utils::SERIAL_COUNTER.next_serial();
        let keyboard = self.wayland.keyboard.clone();
        keyboard.set_focus(self, Some(surface), serial);
        true
    }

    fn press_button(&mut self, index: usize, slot: usize) {
        use wayland_protocols::xdg::shell::server::xdg_toplevel;

        match slot {
            0 => self.windows[index].toplevel.send_close(),
            1 => {
                self.windows[index].hidden = true;
                self.focus = None;
                self.remember_icon(index);
                self.wake_windows();
            }
            _ => {
                let screen = self.screen;
                let chrome = self.chrome;
                let window = &mut self.windows[index];
                let want = match window.restore.take() {
                    Some(back) => back,
                    None => {
                        window.restore = Some(window.content);
                        Rect::new(
                            Vec2::new(chrome.edge, chrome.title),
                            Vec2::new(
                                screen.x - chrome.edge * 2.0,
                                screen.y - chrome.title - chrome.edge,
                            ),
                        )
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
        if name.is_empty() || self.app_icons.contains_key(&name) {
            return;
        }
        let wanted = self.dock.icon as u32;
        let art = crate::apps::find(&name)
            .and_then(|entry| entry.icon)
            .and_then(|path| drifter_core::image::load(&path).ok())
            .map(|image| image.scaled(wanted, wanted));
        let Some(image) = art else {
            eprintln!("drifter: no dock icon for {name}");
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
            Grab::Move { index, offset } => {
                let world = self.in_world(self.pointer);
                if let Some(window) = self.windows.get_mut(index) {
                    window.content.origin = (world - offset).round();
                    window.touched = true;
                }
            }
            Grab::Resize {
                index,
                grip,
                from,
                start,
            } => {
                let chrome = self.chrome;
                let moved = self.in_world(self.pointer) - from;
                let least = Vec2::new(160.0, 80.0);
                let wanted = chrome.resize(start, grip, moved, least);
                if let Some(window) = self.windows.get_mut(index) {
                    window.content.origin = wanted.origin.round();
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
            self.grab = Grab::None;
            self.held = None;
        }
        if state == ButtonState::Pressed {
            if self.press_overview() {
                self.kick();
                return;
            }
            if self.press_popover() {
                self.kick();
                return;
            }
            if code == BTN_RIGHT && self.open_context() {
                self.kick();
                return;
            }
            if self.press_dock() {
                self.kick();
                return;
            }
            if self.take_grab() {
                return;
            }
            if let Some((index, _)) = self.under_pointer() {
                let index = self.raise(index);
                let surface = self.windows[index].toplevel.wl_surface().clone();
                let keyboard = self.wayland.keyboard.clone();
                keyboard.set_focus(self, Some(surface), serial);
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

    fn tell_keyboard(&mut self, event: &impl KeyboardKeyEvent<LibinputInputBackend>) {
        use smithay::input::keyboard::FilterResult;
        use smithay::utils::SERIAL_COUNTER;

        let keyboard = self.wayland.keyboard.clone();
        let time = event.time_msec();
        keyboard.input::<(), _>(
            self,
            event.key_code(),
            event.state(),
            SERIAL_COUNTER.next_serial(),
            time,
            |_, _, _| FilterResult::Forward,
        );
    }

    fn input(&mut self, event: InputEvent<LibinputInputBackend>) {
        match event {
            InputEvent::PointerMotion { event } => {
                let delta = Vec2::new(event.delta_x(), event.delta_y());
                self.pointer_to(self.pointer + delta);
                self.drag();
                self.hover();
                self.tell_pointer(event.time_msec());
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
                if event.state() == ButtonState::Pressed {
                    self.press(event.button_code());
                }
                self.tell_button(event.button_code(), event.state(), event.time_msec());
            }
            InputEvent::Keyboard { event }
                if event.state() == KeyState::Pressed && event.key_code().raw() == ESCAPE =>
            {
                self.signal.stop();
            }
            InputEvent::PointerAxis { event } => {
                use smithay::backend::input::{Axis, PointerAxisEvent};
                if !self.modifier {
                    return;
                }
                let steps = event
                    .amount_v120(Axis::Vertical)
                    .map(|raw| -raw / 120.0)
                    .or_else(|| event.amount(Axis::Vertical).map(|raw| -raw / 10.0))
                    .unwrap_or(0.0);
                if steps != 0.0 {
                    self.zoom_by(steps);
                }
            }
            InputEvent::Keyboard { event } => {
                let code = event.key_code().raw();
                if matches!(code, SUPER_LEFT | SUPER_RIGHT) {
                    self.modifier = event.state() == KeyState::Pressed;
                }
                self.tell_keyboard(&event);
            }
            _ => {}
        }
    }

    pub fn remember(&self) {
        let path = Config::camera_path();
        if let Some(folder) = path.parent()
            && let Err(error) = std::fs::create_dir_all(folder)
        {
            eprintln!("drifter: cannot keep the camera: {error}");
            return;
        }
        if let Err(error) = std::fs::write(&path, self.camera.write()) {
            eprintln!("drifter: cannot keep the camera: {error}");
        }
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

        state.draw()?;

        events
            .run(None, &mut state, |_| {})
            .map_err(|e| format!("event loop failed: {e}"))?;

        state.remember();

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
    want: (u32, u32),
    background: [f32; 4],
    wallpaper: Option<&Image>,
) -> Result<Screen, String> {
    let events: EventLoop<'static, State> =
        EventLoop::try_new().map_err(|e| format!("cannot create the event loop: {e}"))?;

    let fd = session
        .open(
            path,
            OFlags::RDWR | OFlags::CLOEXEC | OFlags::NOCTTY | OFlags::NONBLOCK,
        )
        .map_err(|e| format!("cannot open {}: {e}", path.display()))?;

    let drm_fd = DrmDeviceFd::new(DeviceFd::from(fd));
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

    let picked = mode::select(&modes, Some(want))
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
        GbmFramebufferExporter::new(gbm.clone(), None),
        [DrmFourcc::Xrgb8888, DrmFourcc::Argb8888],
        renderer_formats,
        (64, 64).into(),
        Some(gbm.clone()),
    )
    .map_err(|e| format!("drm compositor: {e}"))?;

    let gl = describe(&mut renderer);
    let screen = Vec2::new(f64::from(width), f64::from(height));
    let world = config.world_size(screen);
    let remembered = std::fs::read_to_string(Config::camera_path())
        .ok()
        .and_then(|text| Camera::read(&text))
        .map(|mut camera| {
            let least = Camera::fit_zoom(screen, world);
            camera.zoom = camera.zoom.clamp(least, 4.0);
            camera.clamp_to_world(world, screen);
            camera
        })
        .unwrap_or_else(|| Camera::at_work_zoom(world / 2.0));
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
    let rect = wallpaper
        .zip(texture.as_ref())
        .map(|(image, _)| Rect::fill_centered(image.size(), screen));
    let wallpaper_element = texture.zip(rect);

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
    let chrome = match skin.get("title-mid.png").zip(skin.get("edge-left.png")) {
        Some(((_, title), (_, edge))) => Chrome {
            title: title.y,
            edge: edge.x,
            ..chrome
        },
        None => chrome,
    };

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
    let wayland = crate::shell::start(&display)?;
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

    let signals = Signals::new(&[Signal::SIGUSR1, Signal::SIGUSR2])
        .map_err(|e| format!("cannot watch signals: {e}"))?;

    let (tray_bus, tray_icons) = match crate::tray::start() {
        Ok((bus, icons)) => (Some(bus), Some(icons)),
        Err(error) => {
            eprintln!("drifter: no tray: {error}");
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

    let wanted = match config.dock.pinned.is_empty() {
        true => crate::apps::defaults(),
        false => config.dock.pinned.clone(),
    };

    let mut pinned = Vec::new();
    for name in &wanted {
        let art = crate::apps::find(name)
            .and_then(|entry| entry.icon)
            .and_then(|path| match drifter_core::image::load(&path) {
                Ok(image) => Some(image),
                Err(error) => {
                    eprintln!("drifter: no dock icon for {name}: {error}");
                    None
                }
            })
            .map(|image| image.scaled(Dock::default().icon as u32, Dock::default().icon as u32));
        let buffer = match art {
            Some(image) => upload(&mut renderer, Some(&image), "dock icon")?,
            None => None,
        };
        pinned.push((name.clone(), buffer));
    }

    let trash_look = Dock::default();
    let trash_image =
        drifter_core::dock::paint_trash(trash_look.icon as u32, trash_look.rule_ink());
    let trash = upload(&mut renderer, Some(&trash_image), "trash")?;

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

    let mut libinput = Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
    let seat_name = session.seat();
    libinput
        .udev_assign_seat(&seat_name)
        .map_err(|_| format!("cannot take input on seat {seat_name}"))?;

    let state = State {
        output,
        wayland,
        windows: Vec::new(),
        popups: Vec::new(),
        display: Some(display),
        chrome,
        camera: remembered,
        world,
        mode: config.workspace.wallpaper_mode,
        flight: None,
        corner_since: None,
        grab: Grab::None,
        modifier: false,
        held: None,
        focus: None,
        skin,
        started: Instant::now(),
        socket: String::new(),
        drm,
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
        dock: Dock::default(),
        pinned,
        trash,
        rule,
        app_icons: std::collections::HashMap::new(),
        format: config.tray.format.clone(),
        clock: None,
        icons: Vec::new(),
        popover: None,
        look: Look::default(),
        tip: None,
        tip_of: None,
        hover_since: None,
        tray_bus,
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
                state.drm.pause();
            }
            SessionEvent::ActivateSession => {
                state.active = true;
                state.awaiting = false;
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
    let poll = state_poll_fd(&mut state);
    handle
        .insert_source(poll, |_, _, state: &mut State| {
            state.dispatch_clients();
            state.kick_now();
            Ok(smithay::reexports::calloop::PostAction::Continue)
        })
        .map_err(|e| format!("cannot watch wayland clients: {e}"))?;

    handle
        .insert_source(signals, |event, _, state: &mut State| {
            let edge = if event.signal() == Signal::SIGUSR1 {
                Edge::Top
            } else {
                Edge::Bottom
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
            camera: remembered,
            drawer: open_band,
            corner: shapes[Edge::Top.index()].radius,
        },
        events,
        state,
    })
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
