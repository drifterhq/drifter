use std::path::PathBuf;

use drifter_core::settings::{Fonts, Hit, Id, Look, Model, Section, Value};
use drifter_core::stats::{self, Mount};
use drifter_core::{Config, Font, Image, Vec2};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
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
    delegate_compositor, delegate_output, delegate_pointer, delegate_registry, delegate_seat,
    delegate_shm, delegate_xdg_shell, delegate_xdg_window, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

const APP: &str = "drifter-settings";
const START: (u32, u32) = (860, 620);

struct Settings {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    window: Window,
    pointer: Option<wl_pointer::WlPointer>,
    size: (u32, u32),
    configured: bool,
    exit: bool,

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
        .map(|art| {
            let width = art.width.min(720);
            let height =
                (f64::from(art.height) * f64::from(width) / f64::from(art.width.max(1))) as u32;
            art.scaled(width, height.max(1))
        });
    let model = read_model(&config);

    let connection = Connection::connect_to_env().map_err(|e| format!("no wayland: {e}"))?;
    let (globals, mut queue) =
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
    let look = Look::default();
    let least = look.least();
    window.set_min_size(Some((least.x as u32, least.y as u32)));
    window.commit();

    let pool = SlotPool::new((START.0 * START.1 * 4) as usize, &shm)
        .map_err(|e| format!("no shared memory: {e}"))?;

    let mut settings = Settings {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        shm,
        pool,
        window,
        pointer: None,
        size: START,
        configured: false,
        exit: false,
        look,
        fonts,
        wallpaper,
        path,
        saved: model.clone(),
        model,
        section: Section::Desktop,
        scroll: 0.0,
        at: Vec2::ZERO,
        holding: None,
    };

    while !settings.exit {
        queue
            .blocking_dispatch(&mut settings)
            .map_err(|e| format!("wayland: {e}"))?;
    }
    Ok(())
}

fn read_model(config: &Config) -> Model {
    let mounts = std::fs::read_to_string("/proc/self/mountinfo")
        .map(|text| stats::mounts(&text))
        .unwrap_or_default();
    Model::read(config, &mounts, kernel)
}

fn kernel(mount: &Mount) -> Option<String> {
    let link = std::fs::read_link(format!("/sys/dev/block/{}", mount.device)).ok()?;
    Some(link.file_name()?.to_string_lossy().into_owned())
}

impl Settings {
    fn view(&self) -> Vec2 {
        Vec2::new(f64::from(self.size.0), f64::from(self.size.1))
    }

    fn draw(&mut self) {
        let (width, height) = self.size;
        let fonts = Fonts {
            body: &self.fonts.0,
            small: &self.fonts.1,
            title: &self.fonts.2,
        };
        let image = self.look.paint(
            self.view(),
            self.section,
            &self.model,
            self.scroll,
            &fonts,
            self.wallpaper.as_ref(),
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
        self.window.commit();
    }

    fn clamp_scroll(&mut self) {
        let groups = self.model.groups(self.section);
        let most = self.look.max_scroll(self.view(), self.section, &groups);
        self.scroll = self.scroll.clamp(0.0, most);
    }

    fn press(&mut self) {
        let hit = self
            .look
            .hit(self.view(), self.section, &self.model, self.scroll, self.at);
        match hit {
            Some(Hit::Section(section)) => {
                self.section = section;
                self.scroll = 0.0;
            }
            Some(Hit::Change(id, value)) => {
                self.model.set(id, value);
                match value {
                    Value::Number(_) => self.holding = Some(id),
                    _ => self.save(),
                }
            }
            None => return,
        }
        self.draw();
    }

    fn drag(&mut self) {
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
                self.draw();
            }
        }
    }

    fn release(&mut self) {
        if self.holding.take().is_some() {
            self.save();
        }
    }

    fn save(&mut self) {
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
        if let Some(folder) = self.path.parent() {
            let _ = std::fs::create_dir_all(folder);
        }
        match std::fs::write(&self.path, out) {
            Ok(()) => self.saved = self.model.clone(),
            Err(error) => eprintln!(
                "drifter-settings: cannot save {}: {error}",
                self.path.display()
            ),
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

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}

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

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
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
        self.configured = true;
        self.clamp_scroll();
        self.draw();
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
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
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
                PointerEventKind::Release { button, .. } if button == BTN_LEFT => self.release(),
                PointerEventKind::Axis { vertical, .. } if self.at.x > self.look.sidebar => {
                    let before = self.scroll;
                    self.scroll += vertical.absolute;
                    self.clamp_scroll();
                    if self.scroll != before {
                        self.draw();
                    }
                }
                PointerEventKind::Leave { .. } => self.release(),
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
delegate_xdg_shell!(Settings);
delegate_xdg_window!(Settings);
delegate_registry!(Settings);
