use std::path::PathBuf;
use std::time::{Duration, Instant};

use drifter_core::explorer::input::{self, Button, Key, Mods, Named, Ui};
use drifter_core::explorer::look::{Art, Fonts, Look, Paint, Want};
use drifter_core::explorer::model::{Effect, Model};
use drifter_core::explorer::ops::{self, Done, Job};
use drifter_core::explorer::prefs::{Prefs, on_path};
use drifter_core::{Config, Font, Image, Vec2};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop::channel::{Event as ChannelEvent, Sender as LoopSender, channel as loop_channel};
use smithay_client_toolkit::reexports::calloop::ping::make_ping;
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers};
use smithay_client_toolkit::seat::pointer::{BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::XdgShell;
use smithay_client_toolkit::shell::xdg::window::{Window, WindowConfigure, WindowDecorations, WindowHandler};
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry, delegate_seat,
    delegate_shm, delegate_xdg_shell, delegate_xdg_window, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

use crate::art::{self, Painter};

const START: (u32, u32) = (1100, 700);
const SETTLE: Duration = Duration::from_millis(16);
const FRAME_LATE: Duration = Duration::from_millis(100);
const DOUBLE: Duration = Duration::from_millis(420);

static SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

fn scale() -> u32 {
    SCALE.load(std::sync::atomic::Ordering::Relaxed)
}

struct Explorer {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    window: Window,
    qh: QueueHandle<Explorer>,
    handle: smithay_client_toolkit::reexports::calloop::LoopHandle<'static, Explorer>,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    gone_keyboards: Vec<wl_keyboard::WlKeyboard>,
    mods: Modifiers,
    size: (u32, u32),
    exit: bool,
    configured: bool,
    resized: bool,
    stale: bool,
    shown: Option<Instant>,
    look: Look,
    fonts: (Font, Font, Font),
    model: Model,
    art: Art,
    painter: Painter,
    asked: std::collections::HashSet<Want>,
    jobs: LoopSender<Done>,
    at: Vec2,
    last_press: Option<(Instant, Vec2, u32, u32)>,
    title: String,
    config_seen: Option<std::time::SystemTime>,
    watched_at: Instant,
    theme_checks: u64,
}

fn modified(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

fn themed(config: &Config) -> Look {
    let palette = drifter_core::theme::of(drifter_core::theme::now(config.appearance.style));
    let accent = config.appearance.accent.map(|accent| accent.color()).unwrap_or([64, 128, 238]);
    Look::themed(&palette, accent)
}

fn asked_path() -> Option<PathBuf> {
    let word = std::env::args_os().nth(1)?;
    let text = word.to_string_lossy();
    let text = text.strip_prefix("file://").unwrap_or(&text);
    let path = PathBuf::from(text);
    match path.is_dir() {
        true => Some(path),
        false => path.parent().map(PathBuf::from),
    }
}

pub fn run() -> Result<(), String> {
    let path = Config::config_path();
    let config = Config::load_from(&path).unwrap_or_default();
    drifter_core::lang::use_config(config.appearance.language.as_deref());
    let find = |px| Font::find(config.font_path(), px);
    let fonts = (find(13.0)?, find(11.5)?, find(17.0)?);
    drifter_core::icon::set_theme(config.appearance.icons.clone());

    let connection = Connection::connect_to_env().map_err(|e| format!("no wayland: {e}"))?;
    let (globals, queue) = registry_queue_init(&connection).map_err(|e| format!("no wayland registry: {e}"))?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(|e| format!("no wl_compositor: {e}"))?;
    let shell = XdgShell::bind(&globals, &qh).map_err(|e| format!("no xdg shell: {e}"))?;
    let shm = Shm::bind(&globals, &qh).map_err(|e| format!("no wl_shm: {e}"))?;

    let (ping, pinged) = make_ping().map_err(|e| format!("no wake-up pipe: {e}"))?;
    let mut model = Model::new(Prefs::path(), asked_path());
    model.reader.set_wake(std::sync::Arc::new(move || ping.ping()));
    let surface = compositor.create_surface(&qh);
    let window = shell.create_window(surface, WindowDecorations::RequestServer, &qh);
    let title = drifter_core::explorer::entry::name_of(&model.dir);
    window.set_title(title.clone());
    window.set_app_id(drifter_core::explorer::APP);
    let look = themed(&config);
    let least = look.least();
    window.set_min_size(Some((least.x as u32, least.y as u32)));
    window.commit();

    let pool = SlotPool::new((START.0 * START.1 * 4) as usize, &shm).map_err(|e| format!("no shared memory: {e}"))?;
    let mut events: EventLoop<'static, Explorer> = EventLoop::try_new().map_err(|e| format!("no event loop: {e}"))?;
    WaylandSource::new(connection, queue)
        .insert(events.handle())
        .map_err(|e| format!("cannot watch wayland: {e}"))?;
    let (painted, painted_rx) = loop_channel::<(Want, Option<Image>)>();
    events
        .handle()
        .insert_source(painted_rx, |event, _, app: &mut Explorer| {
            if let ChannelEvent::Msg((want, image)) = event {
                app.painted(want, image);
            }
        })
        .map_err(|e| format!("cannot watch the previews: {e}"))?;
    events
        .handle()
        .insert_source(pinged, |_, _, app: &mut Explorer| {
            if app.model.arrived() {
                app.stale = true;
            }
        })
        .map_err(|e| format!("cannot watch folder reads: {e}"))?;
    let (jobs, jobs_rx) = loop_channel::<Done>();
    events
        .handle()
        .insert_source(jobs_rx, |event, _, app: &mut Explorer| {
            if let ChannelEvent::Msg(done) = event {
                for made in &done.made {
                    app.art.forget(made);
                }
                app.model.finished(done);
                app.after();
            }
        })
        .map_err(|e| format!("cannot watch file work: {e}"))?;
    events
        .handle()
        .insert_source(Timer::from_duration(Duration::from_millis(500)), |_, _, app: &mut Explorer| {
            app.tick();
            TimeoutAction::ToDuration(Duration::from_millis(500))
        })
        .map_err(|e| format!("cannot start the timer: {e}"))?;

    let mut app = Explorer {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        shm,
        pool,
        window,
        qh: qh.clone(),
        handle: events.handle(),
        pointer: None,
        keyboard: None,
        gone_keyboards: Vec::new(),
        mods: Modifiers::default(),
        size: START,
        exit: false,
        configured: false,
        resized: false,
        stale: true,
        shown: None,
        look,
        fonts,
        model,
        art: Art::default(),
        painter: Painter::start(painted),
        asked: std::collections::HashSet::new(),
        jobs,
        at: Vec2::ZERO,
        last_press: None,
        title,
        config_seen: modified(&path),
        watched_at: Instant::now(),
        theme_checks: 0,
    };
    while !app.exit {
        events
            .dispatch(app.stale.then_some(SETTLE), &mut app)
            .map_err(|e| format!("event loop: {e}"))?;
        if app.configured && (std::mem::take(&mut app.resized) || (app.stale && app.frame_free())) {
            app.draw();
        }
    }
    app.model.save();
    Ok(())
}

fn launch(argv: &[String], dir: &std::path::Path) -> Result<(), String> {
    let Some((program, rest)) = argv.split_first() else {
        return Err("nothing to run".to_string());
    };
    let mut command = std::process::Command::new(program);
    command
        .args(rest)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut child = command.spawn().map_err(|error| format!("{program}: {error}"))?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn clip(text: &str) -> bool {
    use std::io::Write;
    if !on_path("wl-copy") {
        return false;
    }
    let child = std::process::Command::new("wl-copy")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return false;
    };
    let wrote = child.stdin.take().is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok());
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    wrote
}

fn named(keysym: Keysym) -> Named {
    match keysym {
        Keysym::Up | Keysym::KP_Up => Named::Up,
        Keysym::Down | Keysym::KP_Down => Named::Down,
        Keysym::Left | Keysym::KP_Left => Named::Left,
        Keysym::Right | Keysym::KP_Right => Named::Right,
        Keysym::Home | Keysym::KP_Home => Named::Home,
        Keysym::End | Keysym::KP_End => Named::End,
        Keysym::Page_Up | Keysym::KP_Page_Up => Named::PageUp,
        Keysym::Page_Down | Keysym::KP_Page_Down => Named::PageDown,
        Keysym::Return | Keysym::KP_Enter => Named::Enter,
        Keysym::Escape => Named::Escape,
        Keysym::BackSpace => Named::Back,
        Keysym::Delete | Keysym::KP_Delete => Named::Delete,
        Keysym::Tab | Keysym::ISO_Left_Tab => Named::Tab,
        Keysym::F2 => Named::F2,
        Keysym::F5 => Named::F5,
        Keysym::F9 => Named::F9,
        Keysym::Menu => Named::Menu,
        Keysym::KP_Add => Named::Letter('+'),
        Keysym::KP_Subtract => Named::Letter('-'),
        _ => match keysym.key_char() {
            Some(letter) if !letter.is_control() => Named::Letter(letter.to_ascii_lowercase()),
            _ => Named::Other,
        },
    }
}

impl Explorer {
    fn view(&self) -> Vec2 {
        Vec2::new(f64::from(self.size.0), f64::from(self.size.1))
    }

    fn frame_free(&self) -> bool {
        self.shown.is_none_or(|at| at.elapsed() >= FRAME_LATE)
    }

    fn painted(&mut self, want: Want, image: Option<Image>) {
        self.asked.remove(&want);
        match (want, image) {
            (Want::Thumb(path, side), Some(image)) => {
                self.art.thumbs.insert((path, side), image);
            }
            (Want::Icon(name, side), Some(image)) => {
                self.art.icons.insert((name, side), image);
            }
            (want, None) => {
                self.art.missing.insert(want);
            }
        }
        self.stale = true;
    }

    fn fetch(&mut self) {
        for want in self.art.take_wants() {
            if self.asked.contains(&want) {
                continue;
            }
            match &want {
                Want::Icon(name, side) => match art::icon(name, *side) {
                    Some(image) => {
                        self.art.icons.insert((name.clone(), *side), image);
                        self.stale = true;
                    }
                    None => {
                        self.art.missing.insert(want);
                    }
                },
                Want::Thumb(..) => {
                    self.asked.insert(want.clone());
                    self.painter.ask(want);
                }
            }
        }
        if self.art.thumbs.len() > 600 {
            let dir = self.model.dir.clone();
            self.art.thumbs.retain(|(path, _), _| path.parent() == Some(dir.as_path()));
        }
    }

    fn tick(&mut self) {
        self.watch();
        if self.model.tick() {
            self.after();
        }
        if !self.model.saved && self.model.dragging.is_none() && self.model.sheet.is_none() {
            self.model.save();
        }
    }

    fn watch(&mut self) {
        if self.watched_at.elapsed() < Duration::from_millis(450) {
            return;
        }
        self.watched_at = Instant::now();
        let path = Config::config_path();
        let seen = modified(&path);
        if seen != self.config_seen {
            self.config_seen = seen;
            if let Ok(config) = Config::load_from(&path) {
                drifter_core::icon::set_theme(config.appearance.icons.clone());
                self.look = themed(&config);
                self.art.icons.clear();
                self.art.missing.retain(|want| matches!(want, Want::Thumb(..)));
                self.stale = true;
            }
        }
        self.theme_checks += 1;
        if self.theme_checks % 120 == 0 {
            let again = themed(&Config::load_from(&path).unwrap_or_default());
            if again != self.look {
                self.look = again;
                self.stale = true;
            }
        }
    }

    fn after(&mut self) {
        for effect in std::mem::take(&mut self.model.effects) {
            match effect {
                Effect::Job(job) => self.run_job(job),
                Effect::Launch(argv) => {
                    if let Err(error) = launch(&argv, &self.model.dir) {
                        self.model.say(error, true);
                    }
                }
                Effect::Clip(text) => {
                    if !clip(&text) {
                        log!("drifter-explorer: install wl-clipboard to copy to other apps");
                    }
                }
                Effect::NewWindow(path) => {
                    let me = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("drifter-explorer"));
                    let argv = vec![me.to_string_lossy().into_owned(), path.to_string_lossy().into_owned()];
                    if let Err(error) = launch(&argv, &self.model.dir) {
                        self.model.say(error, true);
                    }
                }
                Effect::Close => self.exit = true,
            }
        }
        let title = match ops::in_trash(&self.model.dir) {
            true => drifter_core::lang::t("Trash").to_string(),
            false => drifter_core::explorer::entry::name_of(&self.model.dir),
        };
        if title != self.title {
            self.window.set_title(title.clone());
            self.title = title;
        }
        self.stale = true;
    }

    fn run_job(&mut self, job: Job) {
        let jobs = self.jobs.clone();
        std::thread::spawn(move || {
            let done = ops::run(job);
            let _ = jobs.send(done);
        });
    }

    fn draw(&mut self) {
        self.stale = false;
        drifter_core::paint::set_density(scale());
        let lines = self.model.lines();
        let image = {
            let fonts = Fonts { body: &self.fonts.0, small: &self.fonts.1, title: &self.fonts.2 };
            let job = Paint { model: &self.model, lines: &lines, fonts: &fonts, art: &self.art };
            self.look.paint(self.view(), &job)
        };
        self.show(&image);
        self.fetch();
    }

    fn show(&mut self, image: &Image) {
        let (width, height) = (image.width, image.height);
        let stride = width as i32 * 4;
        let (buffer, canvas) = match self.pool.create_buffer(width as i32, height as i32, stride, wl_shm::Format::Argb8888) {
            Ok(made) => made,
            Err(error) => {
                log!("drifter-explorer: no buffer: {error}");
                return;
            }
        };
        let pixels = image.rgba.as_chunks::<4>().0;
        for (to, from) in canvas.as_chunks_mut::<4>().0.iter_mut().zip(pixels) {
            *to = [from[2], from[1], from[0], from[3]];
        }
        let surface = self.window.wl_surface();
        surface.set_buffer_scale(scale() as i32);
        surface.damage_buffer(0, 0, width as i32, height as i32);
        if let Err(error) = buffer.attach_to(surface) {
            log!("drifter-explorer: {error}");
            return;
        }
        surface.frame(&self.qh, surface.clone());
        self.shown = Some(Instant::now());
        self.window.commit();
    }

    fn with_ui<T>(&mut self, work: impl FnOnce(&mut Model, &Ui, &[drifter_core::explorer::model::Line]) -> T) -> T {
        drifter_core::paint::set_density(scale());
        let lines = self.model.lines();
        let fonts = Fonts { body: &self.fonts.0, small: &self.fonts.1, title: &self.fonts.2 };
        let ui = Ui { look: &self.look, fonts: &fonts, size: Vec2::new(f64::from(self.size.0), f64::from(self.size.1)) };
        work(&mut self.model, &ui, &lines)
    }

    fn press(&mut self, button: u32) {
        let now = Instant::now();
        let clicks = match self.last_press {
            Some((when, at, last, count)) if last == button && now.duration_since(when) < DOUBLE && (at - self.at).length() < 5.0 => count + 1,
            _ => 1,
        };
        self.last_press = Some((now, self.at, button, clicks));
        let button = match button {
            BTN_RIGHT => Button::Right,
            BTN_MIDDLE => Button::Middle,
            _ => Button::Left,
        };
        let mods = Mods { ctrl: self.mods.ctrl, shift: self.mods.shift };
        let at = self.at;
        self.with_ui(|model, ui, lines| input::press(model, ui, lines, at, button, clicks, mods));
        self.after();
    }

    fn key(&mut self, event: &KeyEvent) {
        let key = Key { named: named(event.keysym), text: event.utf8.clone(), ctrl: self.mods.ctrl, shift: self.mods.shift, alt: self.mods.alt };
        self.with_ui(|model, ui, _| input::key(model, ui, &key));
        self.after();
    }
}

impl CompositorHandler for Explorer {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, factor: i32) {
        let factor = factor.clamp(1, 4) as u32;
        if factor == scale() {
            return;
        }
        SCALE.store(factor, std::sync::atomic::Ordering::Relaxed);
        drifter_core::paint::set_density(factor);
        self.art.icons.clear();
        self.art.thumbs.clear();
        self.art.missing.clear();
        self.stale = true;
    }

    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        self.shown = None;
    }

    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}

    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for Explorer {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl WindowHandler for Explorer {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.exit = true;
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window, configure: WindowConfigure, _: u32) {
        let least = self.look.least();
        let width = configure.new_size.0.map(|v| v.get()).unwrap_or(self.size.0);
        let height = configure.new_size.1.map(|v| v.get()).unwrap_or(self.size.1);
        self.size = (width.max(least.x as u32), height.max(least.y as u32));
        self.configured = true;
        self.resized = true;
    }
}

impl SeatHandler for Explorer {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            match self.seats.get_pointer(qh, &seat) {
                Ok(pointer) => self.pointer = Some(pointer),
                Err(error) => log!("drifter-explorer: no pointer: {error}"),
            }
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let repeat = Box::new(|app: &mut Explorer, _: &wl_keyboard::WlKeyboard, event: KeyEvent| app.key(&event));
            match self.seats.get_keyboard_with_repeat(qh, &seat, None, self.handle.clone(), repeat) {
                Ok(keyboard) => self.keyboard = Some(keyboard),
                Err(error) => log!("drifter-explorer: no keyboard: {error}"),
            }
        }
    }

    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            pointer.release();
        }
        if capability == Capability::Keyboard
            && let Some(keyboard) = self.keyboard.take()
        {
            self.gone_keyboards.push(keyboard);
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Explorer {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {}

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {
        self.mods = Modifiers::default();
    }

    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        self.key(&event);
    }

    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}

    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, modifiers: Modifiers, _: u32) {
        self.mods = modifiers;
    }
}

impl PointerHandler for Explorer {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for event in events {
            if &event.surface != self.window.wl_surface() {
                continue;
            }
            self.at = Vec2::new(event.position.0, event.position.1);
            let at = self.at;
            match event.kind {
                PointerEventKind::Motion { .. } | PointerEventKind::Enter { .. } => {
                    if self.with_ui(|model, ui, lines| input::motion(model, ui, lines, at)) {
                        self.stale = true;
                    }
                }
                PointerEventKind::Press { button, .. } if matches!(button, BTN_LEFT | BTN_RIGHT | BTN_MIDDLE) => self.press(button),
                PointerEventKind::Release { button, .. } if button == BTN_LEFT => {
                    let copy = self.mods.ctrl;
                    self.with_ui(|model, ui, lines| input::release(model, ui, lines, at, copy));
                    self.after();
                }
                PointerEventKind::Axis { horizontal, vertical, .. } => {
                    let ctrl = self.mods.ctrl;
                    let (dx, dy) = (horizontal.absolute, vertical.absolute);
                    if self.with_ui(|model, ui, lines| input::scroll(model, ui, lines, at, dx, dy, ctrl)) {
                        self.stale = true;
                    }
                }
                PointerEventKind::Leave { .. } => {
                    if input::leave(&mut self.model) {
                        self.stale = true;
                    }
                }
                _ => {}
            }
        }
    }
}

impl ShmHandler for Explorer {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Explorer {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(Explorer);
delegate_output!(Explorer);
delegate_shm!(Explorer);
delegate_seat!(Explorer);
delegate_pointer!(Explorer);
delegate_keyboard!(Explorer);
delegate_xdg_shell!(Explorer);
delegate_xdg_window!(Explorer);
delegate_registry!(Explorer);
