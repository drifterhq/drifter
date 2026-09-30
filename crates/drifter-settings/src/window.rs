use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use drifter_core::desktop::App;
use drifter_core::settings::{Art, Ask, Fonts, Hit, Id, Look, Model, Section, Value};
use drifter_core::sheet::{For, Pick, Sheet};
use drifter_core::stats::{self, Mount};
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

struct Settings {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    window: Window,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    size: (u32, u32),
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
    sheet: Option<Sheet>,
    apps: Vec<App>,
    icons: HashMap<String, Image>,
    thumbs: HashMap<PathBuf, Image>,
    asked: HashSet<PathBuf>,
    loads: Sender<PathBuf>,
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
    let mut model = read_model(&config);
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
    let look = Look::default();
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

    let mut settings = Settings {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        shm,
        pool,
        window,
        pointer: None,
        keyboard: None,
        size: START,
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
            .dispatch(None, &mut settings)
            .map_err(|e| format!("event loop: {e}"))?;
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

fn kernel(mount: &Mount) -> Option<String> {
    let link = std::fs::read_link(format!("/sys/dev/block/{}", mount.device)).ok()?;
    Some(link.file_name()?.to_string_lossy().into_owned())
}

fn tell_the_system(role: drifter_core::desktop::Role, id: &str) {
    let path = home().join(".config/mimeapps.list");
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let out = drifter_core::desktop::set_default(&text, role, id);
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    if let Err(error) = std::fs::write(&path, out) {
        eprintln!("drifter-settings: cannot save {}: {error}", path.display());
    }
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
    drifter_core::image::load(path)
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
            icon_file(key.strip_prefix("cmd:")?)?
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
            .collect();
        for key in keys {
            self.want_icon(key);
        }
    }

    fn want_thumbs(&mut self) {
        if self.section != Section::Wallpaper {
            return;
        }
        for path in self.model.pictures.clone() {
            if self.asked.insert(path.clone()) {
                let _ = self.loads.send(path);
            }
        }
    }

    fn draw(&mut self) {
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
            Some(Hit::Section(section)) => {
                self.section = section;
                self.scroll = 0.0;
                self.want_thumbs();
            }
            Some(Hit::Change(id, value)) => {
                self.model.set(id, value);
                if let Value::Number(_) = value {
                    self.holding = Some(id);
                }
                self.save();
            }
            Some(Hit::Press(id, index)) => match self.model.press(id, index) {
                Some(Ask::Pick(purpose)) => self.open_picker(purpose),
                Some(Ask::Files(purpose)) => self.open_files(purpose),
                None => self.save(),
            },
            Some(Hit::Picture(at)) => {
                self.model.choose_picture(at);
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

    fn open_files(&mut self, purpose: For) {
        let pictures = home().join("Pictures");
        let start = match purpose {
            For::Folder => self
                .model
                .folder
                .clone()
                .filter(|folder| folder.is_dir())
                .unwrap_or(if pictures.is_dir() { pictures } else { home() }),
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
            Pick::Folder(folder) => {
                self.model.choose_folder(folder);
                self.sheet = None;
                self.save();
                self.want_thumbs();
            }
            Pick::File(file) => {
                if let For::DockIcon(at) = purpose {
                    self.model.set_icon(at, Some(file));
                    self.load_row_icons();
                    self.save();
                }
                self.sheet = None;
            }
            Pick::Default => {
                if let For::DockIcon(at) = purpose {
                    self.model.set_icon(at, None);
                    self.load_row_icons();
                    self.save();
                }
                self.sheet = None;
            }
        }
    }

    fn finish_add(&mut self, purpose: For, item: Launch) {
        if let (For::Default(role), Launch::App(id)) = (purpose, &item) {
            tell_the_system(role, id);
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
                self.save();
                self.draw();
            }
        }
    }

    fn scroll_by(&mut self, by: f64) {
        let size = self.view();
        if let Some(sheet) = self.sheet.as_mut() {
            sheet.scroll_by(size, by);
            self.draw();
            return;
        }
        if self.at.x <= self.look.sidebar {
            return;
        }
        let before = self.scroll;
        self.scroll += by;
        self.clamp_scroll();
        if self.scroll != before {
            self.draw();
        }
    }

    fn key(&mut self, event: &KeyEvent) {
        let Some(sheet) = self.sheet.as_mut() else {
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
        _: Modifiers,
        _: u32,
    ) {
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
                }
                PointerEventKind::Axis { vertical, .. } => self.scroll_by(vertical.absolute),
                PointerEventKind::Leave { .. } => self.holding = None,
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
