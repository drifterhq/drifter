use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use drifter_core::cursor::{self, Cursor};
use drifter_core::drawer::{self, Drawer, Edge, Panel};
use drifter_core::image::Image;
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
    AbsolutePositionEvent, ButtonState, InputEvent, KeyState, KeyboardKeyEvent, PointerButtonEvent,
    PointerMotionEvent,
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
use smithay::utils::SERIAL_COUNTER;
use smithay::utils::{DeviceFd, Scale, Transform};

const ESCAPE: u32 = 9;
const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;
const PANEL_COLOR: [u8; 3] = [16, 18, 26];
const PANEL_ALPHA: u8 = 235;
const TICK: Duration = Duration::from_millis(8);
const SECOND_TICK: Duration = Duration::from_millis(250);

smithay::backend::renderer::element::render_elements! {
    pub Piece;
    Texture=TextureRenderElement<GlesTexture>,
    Client=WaylandSurfaceRenderElement<GlesRenderer>,
}

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
    pub drawer: Rect,
    pub corner: f64,
}

pub struct Screen {
    found: Found,
    events: EventLoop<'static, State>,
    state: State,
}

pub struct State {
    pub wayland: crate::shell::Wayland,
    pub windows: Vec<crate::shell::Window>,
    display: Option<Display<State>>,
    chrome: Chrome,
    skin: std::collections::HashMap<String, (TextureBuffer<GlesTexture>, Vec2)>,
    started: Instant,
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
    format: String,
    clock: Option<(String, TextureBuffer<GlesTexture>, Rect)>,
    icons: Vec<(String, TextureBuffer<GlesTexture>)>,
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
    fn animating(&self) -> bool {
        self.drawers.iter().any(|drawer| !drawer.settled())
    }

    fn pending(&self) -> Option<Duration> {
        if self.dwell.is_some() || self.leave.is_some() || self.animating() {
            return Some(TICK);
        }
        if self.drawers[Edge::Top.index()].visible() && self.font.is_some() {
            return Some(SECOND_TICK);
        }
        None
    }

    fn elements(&mut self) -> Vec<Piece> {
        let mut elements: Vec<Piece> = vec![Piece::Texture(element(
            &self.arrow,
            cursor::rect(self.pointer, self.arrow_size, self.arrow_hotspot),
            1.0,
        ))];
        if let Some((_, buffer, rect)) = self.clock.as_ref() {
            elements.push(Piece::Texture(element(buffer, *rect, 1.0)));
        }
        if let Some(band) = self.tray_band() {
            for (slot, (_, buffer)) in self.icons.iter().enumerate() {
                elements.push(Piece::Texture(element(
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
                elements.push(Piece::Texture(element(
                    &self.panels[edge.index()],
                    rect,
                    1.0,
                )));
            }
        }

        let windows: Vec<(
            Rect,
            smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        )> = self
            .windows
            .iter()
            .rev()
            .filter(|window| window.placed)
            .map(|window| (window.content, window.toplevel.wl_surface().clone()))
            .collect();
        for (content, surface) in windows {
            let client: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                render_elements_from_surface_tree(
                    &mut self.renderer,
                    &surface,
                    (content.origin.x as i32, content.origin.y as i32),
                    1.0,
                    1.0,
                    Kind::Unspecified,
                );
            elements.extend(client.into_iter().map(Piece::Client));
            self.dress(content, &mut elements);
        }

        if let Some((buffer, rect)) = self.wallpaper.as_ref() {
            elements.push(Piece::Texture(element(buffer, *rect, 1.0)));
        }
        elements
    }

    fn dress(&self, content: Rect, elements: &mut Vec<Piece>) {
        let layout = self.chrome.layout(content);
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
            if let Some((buffer, _)) = self.skin.get(&piece.file_name(true)) {
                elements.push(Piece::Texture(element(buffer, rect, 1.0)));
            }
        }

        let bar = self.chrome.title_bar(content);
        for (slot, name) in [
            "button-close.png",
            "button-minimize.png",
            "button-maximize.png",
        ]
        .iter()
        .enumerate()
        {
            if let Some((buffer, size)) = self.skin.get(*name) {
                let place = self.chrome.button_rect(slot);
                let rect = Rect::new(bar.origin + place.origin, *size);
                elements.push(Piece::Texture(element(buffer, rect, 1.0)));
            }
        }
    }

    pub fn settle_window(
        &mut self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        let screen = self.screen;
        let chrome = self.chrome;
        let Some(window) = self
            .windows
            .iter_mut()
            .find(|window| window.toplevel.wl_surface() == surface)
        else {
            return;
        };

        let size =
            smithay::backend::renderer::utils::with_renderer_surface_state(surface, |state| {
                state.surface_size()
            })
            .flatten();
        let Some(size) = size else {
            return;
        };
        let size = Vec2::new(f64::from(size.w), f64::from(size.h));
        if size.x <= 0.0 || size.y <= 0.0 {
            return;
        }

        if !window.placed {
            let middle = (screen - size) / 2.0;
            let top = middle.y.max(chrome.title + chrome.edge);
            window.content = Rect::new(Vec2::new(middle.x.max(chrome.edge), top), size);
            window.placed = true;
        } else {
            window.content.size = size;
        }
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
                Ok(Some(buffer)) => self.icons.push((icon.service, buffer)),
                Ok(None) => {}
                Err(error) => eprintln!("drifter: {error}"),
            }
        }
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
            BTN_RIGHT => crate::tray::Click::Right,
            BTN_MIDDLE => crate::tray::Click::Middle,
            _ => return,
        };
        if let Some(bus) = self.tray_bus.as_ref() {
            let (service, _) = &self.icons[slot];
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

    fn input(&mut self, event: InputEvent<LibinputInputBackend>) {
        match event {
            InputEvent::PointerMotion { event } => {
                let delta = Vec2::new(event.delta_x(), event.delta_y());
                self.pointer_to(self.pointer + delta);
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let x = event.x_transformed(self.screen.x as i32);
                let y = event.y_transformed(self.screen.y as i32);
                self.pointer_to(Vec2::new(x, y));
            }
            InputEvent::PointerButton { event } => {
                if event.state() == ButtonState::Pressed {
                    self.press(event.button_code());
                }
            }
            InputEvent::Keyboard { event }
                if event.state() == KeyState::Pressed && event.key_code().raw() == ESCAPE =>
            {
                self.signal.stop();
            }
            _ => {}
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
    let output = OutputModeSource::Static {
        size: (i32::from(width), i32::from(height)).into(),
        scale: Scale::from(1.0),
        transform: Transform::Normal,
    };

    let compositor: Compositor = DrmCompositor::new(
        output,
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

    let mut libinput = Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
    let seat_name = session.seat();
    libinput
        .udev_assign_seat(&seat_name)
        .map_err(|_| format!("cannot take input on seat {seat_name}"))?;

    let state = State {
        wayland,
        windows: Vec::new(),
        display: Some(display),
        chrome,
        skin,
        started: Instant::now(),
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
        format: config.tray.format.clone(),
        clock: None,
        icons: Vec::new(),
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
                if let ChannelEvent::Msg(icons) = event {
                    state.take_icons(icons);
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

    let poll = state_poll_fd(&state);
    handle
        .insert_source(poll, |_, _, state: &mut State| {
            state.dispatch_clients();
            state.kick_now();
            Ok(smithay::reexports::calloop::PostAction::Continue)
        })
        .map_err(|e| format!("cannot watch wayland clients: {e}"))?;

    let signals = Signals::new(&[Signal::SIGUSR1, Signal::SIGUSR2])
        .map_err(|e| format!("cannot watch signals: {e}"))?;
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
            drawer: open_band,
            corner: shapes[Edge::Top.index()].radius,
        },
        events,
        state,
    })
}

fn state_poll_fd(
    state: &State,
) -> smithay::reexports::calloop::generic::Generic<
    smithay::reexports::calloop::generic::FdWrapper<
        smithay::reexports::wayland_server::backend::WaylandSocket,
    >,
> {
    use smithay::reexports::calloop::{Interest, Mode as PollMode, generic::Generic};
    let fd = state
        .display
        .as_ref()
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
    TextureRenderElement::from_texture_buffer(
        (rect.origin.x, rect.origin.y),
        buffer,
        Some(alpha),
        None,
        Some((rect.size.x as i32, rect.size.y as i32).into()),
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
