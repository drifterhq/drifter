use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use drifter_core::cursor::Cursor;
use drifter_core::drawer::{self, Drawer, Edge, Panel};
use drifter_core::image::Image;
use drifter_core::mode::{self, Mode};
use drifter_core::{Rect, Tunables, Vec2};
use smithay::backend::allocator::gbm::{GbmAllocator, GbmBufferFlags, GbmDevice};
use smithay::backend::drm::compositor::{DrmCompositor, FrameFlags};
use smithay::backend::drm::exporter::gbm::GbmFramebufferExporter;
use smithay::backend::drm::{DrmDevice, DrmDeviceFd, DrmEvent};
use smithay::backend::egl::{EGLContext, EGLDisplay};
use smithay::backend::input::{
    AbsolutePositionEvent, InputEvent, KeyState, KeyboardKeyEvent, PointerMotionEvent,
};
use smithay::backend::libinput::{LibinputInputBackend, LibinputSessionInterface};
use smithay::backend::renderer::Color32F;
use smithay::backend::renderer::element::Kind;
use smithay::backend::renderer::element::texture::{TextureBuffer, TextureRenderElement};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::gles::GlesTexture;
use smithay::backend::session::libseat::{LibSeatSession, LibSeatSessionNotifier};
use smithay::backend::session::{Event as SessionEvent, Session};
use smithay::output::OutputModeSource;
use smithay::reexports::calloop::signals::{Signal, Signals};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::{EventLoop, LoopHandle, LoopSignal};
use smithay::reexports::drm::buffer::DrmFourcc;
use smithay::reexports::drm::control::{Device as ControlDevice, connector, crtc};
use smithay::reexports::input::Libinput;
use smithay::reexports::rustix::fs::OFlags;
use smithay::utils::{DeviceFd, Scale, Transform};

const ESCAPE: u32 = 9;
const PANEL_COLOR: [u8; 3] = [16, 18, 26];
const PANEL_ALPHA: u8 = 235;
const TICK: Duration = Duration::from_millis(8);

type Compositor =
    DrmCompositor<GbmAllocator<DrmDeviceFd>, GbmFramebufferExporter<DrmDeviceFd>, (), DrmDeviceFd>;

pub struct Found {
    pub device: PathBuf,
    pub connector: String,
    pub width: u16,
    pub height: u16,
    pub refresh: u32,
    pub renderer: String,
    pub wallpaper: Option<Rect>,
}

pub struct Screen {
    found: Found,
    events: EventLoop<'static, State>,
    state: State,
}

struct State {
    drm: DrmDevice,
    compositor: Compositor,
    renderer: GlesRenderer,
    wallpaper: Option<(TextureBuffer<GlesTexture>, Rect)>,
    panels: [TextureBuffer<GlesTexture>; 2],
    panel: Panel,
    arrow: TextureBuffer<GlesTexture>,
    cursor: Cursor,
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

    fn pending(&self) -> bool {
        self.dwell.is_some() || self.leave.is_some() || self.animating()
    }

    fn elements(&self) -> Vec<TextureRenderElement<GlesTexture>> {
        let mut elements = vec![element(&self.arrow, self.cursor.rect(self.pointer), 1.0)];
        for edge in Edge::ALL {
            let drawer = &self.drawers[edge.index()];
            if drawer.visible() {
                let rect = drawer.rect(self.screen, &self.panel);
                elements.push(element(&self.panels[edge.index()], rect, 1.0));
            }
        }
        if let Some((buffer, rect)) = self.wallpaper.as_ref() {
            elements.push(element(buffer, *rect, 1.0));
        }
        elements
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

        let elements = self.elements();
        let result = self
            .compositor
            .render_frame(
                &mut self.renderer,
                &elements,
                self.clear,
                FrameFlags::DEFAULT,
            )
            .map_err(|e| {
                if self.frames == 0 {
                    format!(
                        "first render failed: {e}\n\
                         drifter needs drm master, which needs a real tty.\n\
                         run it from tty1, not over ssh. smithay only warns when master is denied."
                    )
                } else {
                    format!("render failed after {} frames: {e}", self.frames)
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
        Ok(true)
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
        if self.ticking || !self.pending() {
            return;
        }
        let armed = self
            .handle
            .insert_source(Timer::from_duration(TICK), |_, _, state: &mut State| {
                state.ticking = false;
                state.settle();
                if state.pending() {
                    state.ticking = true;
                    TimeoutAction::ToDuration(TICK)
                } else {
                    TimeoutAction::Drop
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
            if self.drawers[edge.index()].holds(self.pointer, self.screen, &self.panel) {
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
    let panel = Panel {
        height: tunables.drawer_height,
        ..Panel::default()
    };

    let texture = upload(&mut renderer, wallpaper, "wallpaper")?;
    let rect = wallpaper
        .zip(texture.as_ref())
        .map(|(image, _)| Rect::fill_centered(image.size(), screen));
    let wallpaper_element = texture.zip(rect);

    let cursor = Cursor::default();
    let arrow = upload(&mut renderer, Some(&cursor.paint()), "cursor")?
        .ok_or_else(|| "cannot build the cursor".to_string())?;

    let mut built = Vec::new();
    for edge in Edge::ALL {
        let what = match edge {
            Edge::Top => "top drawer",
            Edge::Bottom => "bottom drawer",
        };
        let image = drawer::paint(edge, u32::from(width), &panel, PANEL_COLOR, PANEL_ALPHA);
        let buffer = upload(&mut renderer, Some(&image), what)?
            .ok_or_else(|| format!("cannot build the {what}"))?;
        built.push(buffer);
    }
    let panels: [TextureBuffer<GlesTexture>; 2] = built
        .try_into()
        .map_err(|_| "the drawer textures went missing".to_string())?;

    let mut libinput = Libinput::new_with_udev(LibinputSessionInterface::from(session.clone()));
    let seat_name = session.seat();
    libinput
        .udev_assign_seat(&seat_name)
        .map_err(|_| format!("cannot take input on seat {seat_name}"))?;

    let state = State {
        drm,
        compositor,
        renderer,
        wallpaper: wallpaper_element,
        panels,
        panel,
        arrow,
        cursor,
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
            wallpaper: rect,
        },
        events,
        state,
    })
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
