use std::path::PathBuf;
use std::time::{Duration, Instant};

use drifter_core::explorer::input::{self, Button, Key, Mods, Named, Ui};
use drifter_core::explorer::look::{Art, Fonts, Look, Paint, Want};
use drifter_core::explorer::look::Drop;
use drifter_core::explorer::model::{Act, Drag, Effect, Model};
use drifter_core::explorer::ops::{self, Done, Job};
use drifter_core::explorer::clip::{self, Held};
use drifter_core::explorer::prefs::Prefs;
use drifter_core::{Config, Font, Image, Vec2};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::data_device_manager::data_device::{DataDevice, DataDeviceHandler};
use smithay_client_toolkit::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use smithay_client_toolkit::data_device_manager::data_source::{CopyPasteSource, DataSourceHandler, DragSource};
use smithay_client_toolkit::data_device_manager::{DataDeviceManagerState, WritePipe};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::csd_frame::WindowState;
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
    delegate_compositor, delegate_data_device, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry, delegate_seat,
    delegate_shm, delegate_xdg_shell, delegate_xdg_window, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::wl_data_device::WlDataDevice;
use wayland_client::protocol::wl_data_device_manager::DndAction;
use wayland_client::protocol::wl_data_source::WlDataSource;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

use crate::art::{self, Painter};

const START: (u32, u32) = (1100, 700);
const SETTLE: Duration = Duration::from_millis(16);
const FRAME_LATE: Duration = Duration::from_millis(100);
const DOUBLE: Duration = Duration::from_millis(420);
const POP: Duration = Duration::from_millis(140);
const SWAP: Duration = Duration::from_millis(160);
const GLOW: Duration = Duration::from_millis(120);
const GLIDE: Duration = Duration::from_millis(150);
const WHEEL_STEP: f64 = 48.0;

static SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

fn scale() -> u32 {
    SCALE.load(std::sync::atomic::Ordering::Relaxed)
}

struct Glide {
    side: bool,
    from: f64,
    to: f64,
    shown: f64,
    since: Instant,
}

struct Fade {
    area: drifter_core::Rect,
    at: (u32, u32),
    old: Image,
    new: Image,
    since: Instant,
}

fn crop(image: &Image, area: drifter_core::Rect) -> Option<((u32, u32), Image)> {
    let dense = f64::from(scale());
    let x0 = ((area.origin.x * dense).floor().max(0.0) as u32).min(image.width);
    let y0 = ((area.origin.y * dense).floor().max(0.0) as u32).min(image.height);
    let x1 = ((area.max().x * dense).ceil().max(0.0) as u32).min(image.width);
    let y1 = ((area.max().y * dense).ceil().max(0.0) as u32).min(image.height);
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    let (width, height) = (x1 - x0, y1 - y0);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in y0..y1 {
        let from = ((y * image.width + x0) * 4) as usize;
        rgba.extend_from_slice(&image.rgba[from..from + (width * 4) as usize]);
    }
    Some(((x0, y0), Image { width, height, rgba }))
}

fn paste(image: &mut Image, at: (u32, u32), piece: &Image) {
    for y in 0..piece.height {
        let to = (((at.1 + y) * image.width + at.0) * 4) as usize;
        let from = (y * piece.width * 4) as usize;
        image.rgba[to..to + (piece.width * 4) as usize].copy_from_slice(&piece.rgba[from..from + (piece.width * 4) as usize]);
    }
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
    bound: bool,
    resized: bool,
    stale: bool,
    dirty: Option<drifter_core::Rect>,
    shifts: Vec<(f64, bool)>,
    last: Option<Image>,
    shown: Option<Instant>,
    look: Look,
    access: crate::access::Access,
    data: Option<DataDeviceManagerState>,
    device: Option<DataDevice>,
    held: Option<(CopyPasteSource, Held)>,
    serial: u32,
    clip_ask: u64,
    pasted: LoopSender<(u64, Option<Held>)>,
    press_serial: u32,
    drag_out: Option<(DragSource, Held, Option<(wl_surface::WlSurface, smithay_client_toolkit::shm::slot::Buffer)>)>,
    compositor: CompositorState,
    drop_offer: Option<DragOffer>,
    dropped: LoopSender<(Drop, Option<Held>, bool)>,
    sizes: LoopSender<(PathBuf, (u64, u64))>,
    seen: (bool, bool, drifter_core::explorer::prefs::Mode, PathBuf),
    pop_at: Option<Instant>,
    swap: Option<(Image, Instant, Duration)>,
    fade: Option<Fade>,
    glide: Option<Glide>,
    picks: usize,
    clicked: bool,
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
    tip_plan: u64,
    tip_seen: Option<Instant>,
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
    model.arrived();
    let surface = compositor.create_surface(&qh);
    let window = shell.create_window(surface, WindowDecorations::RequestServer, &qh);
    let title = drifter_core::explorer::entry::name_of(&model.dir);
    window.set_title(title.clone());
    window.set_app_id(drifter_core::explorer::APP);
    let look = themed(&config);
    let least = look.least();
    window.set_min_size(Some((least.x as u32, least.y as u32)));
    let size = model.prefs.window_size.map_or(START, |[width, height]| (width.max(least.x as u32), height.max(least.y as u32)));
    if model.prefs.maximized {
        window.set_maximized();
    }
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
    let (heard, heard_rx) = loop_channel::<crate::access::Heard>();
    events
        .handle()
        .insert_source(heard_rx, |event, _, app: &mut Explorer| {
            if let ChannelEvent::Msg(heard) = event {
                app.heard(heard);
            }
        })
        .map_err(|e| format!("cannot watch the screen reader: {e}"))?;
    let (pasted, pasted_rx) = loop_channel::<(u64, Option<Held>)>();
    events
        .handle()
        .insert_source(pasted_rx, |event, _, app: &mut Explorer| {
            if let ChannelEvent::Msg((asked, held)) = event
                && asked == app.clip_ask
            {
                app.model.heard_clip(held);
                app.stale = true;
            }
        })
        .map_err(|e| format!("cannot watch the clipboard: {e}"))?;
    let (dropped, dropped_rx) = loop_channel::<(Drop, Option<Held>, bool)>();
    events
        .handle()
        .insert_source(dropped_rx, |event, _, app: &mut Explorer| {
            if let ChannelEvent::Msg((target, held, copy)) = event {
                app.landed(target, held, copy);
            }
        })
        .map_err(|e| format!("cannot watch drops: {e}"))?;
    let (sizes, sizes_rx) = loop_channel::<(PathBuf, (u64, u64))>();
    events
        .handle()
        .insert_source(sizes_rx, |event, _, app: &mut Explorer| {
            if let ChannelEvent::Msg((path, total)) = event {
                app.model.measured(&path, total);
                app.stale = true;
            }
        })
        .map_err(|e| format!("cannot watch folder sizes: {e}"))?;
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
            let busy = app.model.jobs.iter().any(|job| job.shown()) || app.model.loading.is_some();
            TimeoutAction::ToDuration(Duration::from_millis(if busy { 250 } else { 500 }))
        })
        .map_err(|e| format!("cannot start the timer: {e}"))?;

    let mut app = Explorer {
        registry: RegistryState::new(&globals),
        compositor,
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
        size,
        exit: false,
        configured: false,
        bound: false,
        resized: false,
        stale: true,
        dirty: None,
        shifts: Vec::new(),
        last: None,
        shown: None,
        look,
        access: crate::access::Access::start(heard),
        data: DataDeviceManagerState::bind(&globals, &qh).ok(),
        device: None,
        held: None,
        serial: 0,
        clip_ask: 0,
        pasted,
        press_serial: 0,
        drag_out: None,
        drop_offer: None,
        dropped,
        sizes,
        seen: (false, false, drifter_core::explorer::prefs::Mode::List, PathBuf::new()),
        pop_at: None,
        swap: None,
        fade: None,
        glide: None,
        picks: 0,
        clicked: false,
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
        tip_plan: 0,
        tip_seen: None,
    };
    while !app.exit {
        events
            .dispatch(app.busy().then_some(SETTLE), &mut app)
            .map_err(|e| format!("event loop: {e}"))?;
        app.animate();
        if app.frame_free() {
            app.glide();
        }
        if app.configured && (std::mem::take(&mut app.resized) || (app.busy() && app.frame_free())) {
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
        if self.model.unsaved() && self.model.dragging.is_none() && self.model.sheet.is_none() {
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
                Effect::Job(id, job) => self.run_job(id, job),
                Effect::Launch(argv) => {
                    if let Err(error) = launch(&argv, &self.model.dir) {
                        self.model.say(error, true);
                    }
                }
                Effect::Hold(held) => self.offer(held),
                Effect::Measure(path) => {
                    let sizes = self.sizes.clone();
                    std::thread::spawn(move || {
                        let total = ops::folder_size(&path);
                        let _ = sizes.send((path, total));
                    });
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
        let title = drifter_core::explorer::model::title_of(&self.model.dir);
        if title != self.title {
            self.window.set_title(title.clone());
            self.title = title;
        }
        self.stale = true;
    }

    fn heard(&mut self, heard: crate::access::Heard) {
        use crate::access::{Heard, Order};
        let request = match heard {
            Heard::Wake => {
                self.access.update(&mut self.model);
                return;
            }
            Heard::Act(request) => request,
        };
        match self.access.aim(&request) {
            Some(Order::Act(act)) => self.model.act(act),
            Some(Order::Menu(act)) => {
                self.model.menu = None;
                self.model.act(act);
            }
            Some(Order::Pick(path)) => self.model.pick_only(path),
            Some(Order::Open(path)) => {
                self.model.pick_only(path);
                self.model.act(Act::Open);
            }
            None => return,
        }
        self.after();
    }

    fn run_job(&mut self, id: u64, job: Job) {
        let jobs = self.jobs.clone();
        let progress = self.model.progress_of(id);
        std::thread::spawn(move || {
            let mut done = ops::run(job, &progress);
            done.id = id;
            let _ = jobs.send(done);
        });
    }

    fn draw(&mut self) {
        use drifter_core::explorer::look::join;
        drifter_core::paint::set_density(scale());
        let lines = self.model.lines();
        let mut hover = self.dirty.take();
        let shifts = std::mem::take(&mut self.shifts);
        let mut before = match self.stale {
            true => None,
            false => self.last.take().filter(|image| image.width == self.size.0 * scale() && image.height == self.size.1 * scale()),
        };
        let fading = self.fade.take();
        if let (Some(fade), Some(image)) = (&fading, before.as_mut()) {
            paste(image, fade.at, &fade.new);
            if hover.is_none() && shifts.is_empty() {
                let mut canvas = drifter_core::paint::Canvas::on(before.take().unwrap_or_else(|| Image { width: 0, height: 0, rgba: Vec::new() }));
                let t = drifter_core::explorer::look::ease(fade.since.elapsed().as_secs_f64() / GLOW.as_secs_f64());
                let at = Vec2::new(f64::from(fade.at.0), f64::from(fade.at.1)) / f64::from(scale());
                canvas.stamp(&fade.old, at, 1.0 - t);
                let image = canvas.image;
                self.show(&image, Some(fade.area));
                self.last = Some(image);
                if let Some(fade) = fading.filter(|_| t < 1.0) {
                    self.fade = Some(fade);
                }
                return;
            }
            if shifts.is_empty() {
                hover = Some(hover.map_or(fade.area, |had| join(had, fade.area)));
            }
        }
        self.stale = false;
        let size = self.view();
        let fonts = Fonts { body: &self.fonts.0, small: &self.fonts.1, title: &self.fonts.2 };
        let job = Paint { model: &self.model, lines: &lines, fonts: &fonts, art: &self.art, clip: None };
        let frame = self.look.frame(size, &self.model);
        let mut moved: Vec<(f64, bool)> = Vec::new();
        for (by, side) in shifts {
            match moved.iter_mut().find(|(_, each)| *each == side) {
                Some(had) => had.0 += by,
                None => moved.push((by, side)),
            }
        }
        let mut part: Option<drifter_core::Rect> = None;
        for (by, side) in moved {
            let Some(image) = before.take() else {
                break;
            };
            if let Ok(image) = self.look.scrolled(image, size, &job, by, side) {
                let area = match side {
                    true => frame.side.unwrap_or(frame.body),
                    false => frame.body,
                };
                part = Some(part.map_or(area, |had| join(had, area)));
                before = Some(image);
            }
        }
        if let (Some(area), Some(image)) = (hover, before.take()) {
            let old = crop(&image, area).filter(|_| self.model.menu.is_none() && self.model.dragging.is_none() && part.is_none());
            let image = self.look.repaint(image, size, &Paint { clip: Some(area), ..job });
            if let (Some((at, old)), Some((_, new))) = (old, crop(&image, area)) {
                self.fade = Some(Fade { area, at, old, new, since: Instant::now() });
            }
            before = Some(image);
            part = Some(part.map_or(area, |had| join(had, area)));
        }
        let (image, part) = match (before, part) {
            (Some(image), Some(area)) => (image, Some(area)),
            _ => (self.look.paint(size, &job), None),
        };
        let image = match (&self.swap, part) {
            (Some((old, at, long)), None) if old.width == image.width && old.height == image.height => {
                let mut canvas = drifter_core::paint::Canvas::on(image);
                let fade = 1.0 - drifter_core::explorer::look::ease(at.elapsed().as_secs_f64() / long.as_secs_f64());
                canvas.stamp(old, Vec2::ZERO, fade);
                canvas.image
            }
            _ => image,
        };
        self.show(&image, part);
        self.last = Some(image);
        self.fetch();
    }

    fn animate(&mut self) {
        let now = (self.model.menu.is_some(), self.model.sheet.is_some(), self.model.view.mode, self.model.dir.clone());
        if (now.0 && !self.seen.0) || (now.1 && !self.seen.1) {
            self.pop_at = Some(Instant::now());
        }
        let moved = now.2 != self.seen.2 || now.3 != self.seen.3;
        let picks = self.picks_key();
        let picked = picks != self.picks && std::mem::take(&mut self.clicked);
        if (moved || picked) && !self.seen.3.as_os_str().is_empty()
            && let Some(last) = self.last.clone()
        {
            self.swap = Some((last, Instant::now(), if moved { SWAP } else { GLOW }));
        }
        self.picks = picks;
        self.seen = now;
        self.model.pop = match self.pop_at {
            Some(at) => drifter_core::explorer::look::ease(at.elapsed().as_secs_f64() / POP.as_secs_f64()),
            None => 1.0,
        };
        if self.pop_at.is_some() {
            self.stale = true;
            if self.model.pop >= 1.0 {
                self.pop_at = None;
            }
        }
        if let Some((_, at, long)) = &self.swap {
            self.stale = true;
            if at.elapsed() >= *long {
                self.swap = None;
            }
        }
    }

    fn busy(&self) -> bool {
        self.stale || self.dirty.is_some() || !self.shifts.is_empty() || self.fade.is_some() || self.glide.is_some()
    }

    fn glide(&mut self) {
        let Some(glide) = self.glide.as_mut() else {
            return;
        };
        let now = match glide.side {
            true => self.model.tree_scroll,
            false => self.model.scroll,
        };
        if now != glide.shown {
            self.glide = None;
            return;
        }
        let t = drifter_core::explorer::look::ease(glide.since.elapsed().as_secs_f64() / GLIDE.as_secs_f64());
        let dense = f64::from(scale());
        let next = ((glide.from + (glide.to - glide.from) * t) * dense).round() / dense;
        if next != now {
            match glide.side {
                true => self.model.tree_scroll = next,
                false => self.model.scroll = next,
            }
            self.shifts.push((next - now, glide.side));
            glide.shown = next;
        }
        if t >= 1.0 {
            self.glide = None;
        }
    }

    fn picks_key(&self) -> usize {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        let mut picked: Vec<&PathBuf> = self.model.picked.iter().collect();
        picked.sort();
        picked.hash(&mut hash);
        hash.finish() as usize
    }

    fn tip_damage(&mut self) -> Option<drifter_core::Rect> {
        self.model.tip.as_ref()?;
        let lines = self.model.lines();
        let fonts = Fonts { body: &self.fonts.0, small: &self.fonts.1, title: &self.fonts.2 };
        let (card, _, _) = self.look.tip(self.view(), &self.model, &lines, &fonts)?;
        Some(self.look.tip_area(card))
    }

    fn plan_tip(&mut self) {
        self.tip_plan += 1;
        let Some(hit) = self.model.hover.clone() else {
            return;
        };
        if self.look.tip_words(&hit, &self.model).is_none() {
            return;
        }
        let plan = self.tip_plan;
        let quick = self.tip_seen.is_some_and(|at| at.elapsed() < Duration::from_millis(1500));
        let wait = Duration::from_millis(if quick { 60 } else { 700 });
        let _ = self.handle.insert_source(Timer::from_duration(wait), move |_, _, app: &mut Explorer| {
            if app.tip_plan == plan && app.model.hover.as_ref() == Some(&hit) && app.model.tip.is_none() {
                app.model.tip = Some(hit.clone());
                drifter_core::paint::set_density(scale());
                match app.tip_damage() {
                    Some(area) => {
                        app.tip_seen = Some(Instant::now());
                        app.damage(Some(area));
                    }
                    None => app.model.tip = None,
                }
            }
            TimeoutAction::Drop
        });
    }

    fn damage(&mut self, area: Option<drifter_core::Rect>) {
        match area {
            Some(area) => self.dirty = Some(self.dirty.map_or(area, |had| drifter_core::explorer::look::join(had, area))),
            None => self.stale = true,
        }
    }

    fn hover_damage(&self, hover: &Option<drifter_core::explorer::look::Hit>, menu: &Option<drifter_core::explorer::model::Menu>, lines: &[drifter_core::explorer::model::Line]) -> Option<drifter_core::Rect> {
        let model = &self.model;
        if model.dragging.is_some() {
            return None;
        }
        let fonts = Fonts { body: &self.fonts.0, small: &self.fonts.1, title: &self.fonts.2 };
        let size = self.view();
        match (menu, &model.menu) {
            (Some(was), Some(now)) => {
                let same = was.items == now.items && was.at == now.at && was.bar == now.bar;
                return same.then(|| drifter_core::explorer::look::join(self.look.menu_area(size, was, &fonts), self.look.menu_area(size, now, &fonts)));
            }
            (None, None) => {}
            _ => return None,
        }
        let mut area: Option<drifter_core::Rect> = None;
        for hit in [hover, &model.hover].into_iter().flatten() {
            let found = self.look.hover_rect(size, model, lines, &fonts, hit)?;
            area = Some(area.map_or(found, |had| drifter_core::explorer::look::join(had, found)));
        }
        area
    }

    fn show(&mut self, image: &Image, part: Option<drifter_core::Rect>) {
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
        match part {
            Some(area) => {
                let dense = f64::from(scale());
                surface.damage_buffer(
                    (area.origin.x * dense).floor() as i32,
                    (area.origin.y * dense).floor() as i32,
                    (area.size.x * dense).ceil() as i32 + 2,
                    (area.size.y * dense).ceil() as i32 + 2,
                );
            }
            None => surface.damage_buffer(0, 0, width as i32, height as i32),
        }
        if let Err(error) = buffer.attach_to(surface) {
            log!("drifter-explorer: {error}");
            return;
        }
        surface.frame(&self.qh, surface.clone());
        self.shown = Some(Instant::now());
        self.window.commit();
        self.access.update(&mut self.model);
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
        self.model.act(drifter_core::explorer::model::Act::Close);
        self.after();
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window, configure: WindowConfigure, _: u32) {
        let least = self.look.least();
        let bound = configure.is_maximized() || configure.is_fullscreen() || configure.state.intersects(WindowState::TILED);
        let free = match self.model.prefs.window_size {
            Some([width, height]) if self.bound && !bound => (width, height),
            _ => self.size,
        };
        self.bound = bound;
        let width = configure.new_size.0.map(|v| v.get()).unwrap_or(free.0);
        let height = configure.new_size.1.map(|v| v.get()).unwrap_or(free.1);
        self.size = (width.max(least.x as u32), height.max(least.y as u32));
        self.remember(&configure);
        self.configured = true;
        self.resized = true;
    }
}

impl Explorer {
    fn remember(&mut self, configure: &WindowConfigure) {
        if configure.is_fullscreen() {
            return;
        }
        let prefs = &mut self.model.prefs;
        let free = !configure.is_maximized() && !configure.state.intersects(WindowState::TILED);
        let size = Some([self.size.0, self.size.1]);
        if free && prefs.window_size != size {
            prefs.window_size = size;
            self.model.saved = false;
        }
        if prefs.maximized != configure.is_maximized() {
            prefs.maximized = configure.is_maximized();
            self.model.saved = false;
        }
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
        if capability == Capability::Keyboard && self.device.is_none() {
            self.device = self.data.as_ref().map(|data| data.get_data_device(qh, &seat));
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
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {
        self.access.focused(true);
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {
        self.mods = Modifiers::default();
        self.access.focused(false);
    }

    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, serial: u32, event: KeyEvent) {
        self.serial = serial;
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
                    let (hover, menu) = (self.model.hover.clone(), self.model.menu.clone());
                    let tip = self.tip_damage();
                    let scrolls = (self.model.scroll, self.model.tree_scroll);
                    let rail = matches!(self.model.dragging, Some(drifter_core::explorer::model::Drag::Rail { .. }));
                    if self.with_ui(|model, ui, lines| input::motion(model, ui, lines, at)) {
                        self.lift();
                        if rail {
                            let (body, side) = (self.model.scroll - scrolls.0, self.model.tree_scroll - scrolls.1);
                            match (body != 0.0, side != 0.0) {
                                (true, false) => self.shifts.push((body, false)),
                                (false, true) => self.shifts.push((side, true)),
                                _ => self.stale = true,
                            }
                            continue;
                        }
                        let lines = self.model.lines();
                        let area = self.hover_damage(&hover, &menu, &lines);
                        self.damage(area);
                        if let Some(tip) = tip.filter(|_| self.model.tip.is_none()) {
                            self.tip_seen = Some(Instant::now());
                            self.damage(Some(tip));
                        }
                        if self.model.hover != hover {
                            self.plan_tip();
                        }
                    }
                }
                PointerEventKind::Press { button, serial, .. } if matches!(button, BTN_LEFT | BTN_RIGHT | BTN_MIDDLE) => {
                    self.serial = serial;
                    self.press_serial = serial;
                    self.clicked = true;
                    self.press(button)
                }
                PointerEventKind::Release { button, .. } if button == BTN_LEFT => {
                    let copy = self.mods.ctrl;
                    self.with_ui(|model, ui, lines| input::release(model, ui, lines, at, copy));
                    self.after();
                }
                PointerEventKind::Axis { horizontal, vertical, .. } => {
                    let ctrl = self.mods.ctrl;
                    let wheel = vertical.discrete != 0 || horizontal.discrete != 0;
                    let (dx, dy) = match wheel {
                        true => (f64::from(horizontal.discrete) * WHEEL_STEP, f64::from(vertical.discrete) * WHEEL_STEP),
                        false => (horizontal.absolute, vertical.absolute),
                    };
                    let shown = (self.model.scroll, self.model.tree_scroll);
                    if let Some(glide) = &self.glide {
                        match glide.side {
                            true => self.model.tree_scroll = glide.to,
                            false => self.model.scroll = glide.to,
                        }
                    }
                    let before = (self.model.scroll, self.model.tree_scroll, self.model.prefs.zoom, self.model.view.mode);
                    if self.model.tip.is_some() {
                        self.stale = true;
                    }
                    let moved = self.with_ui(|model, ui, lines| input::scroll(model, ui, lines, at, dx, dy, ctrl));
                    let (body, side) = (self.model.scroll - before.0, self.model.tree_scroll - before.1);
                    let plain = self.model.prefs.zoom == before.2 && self.model.view.mode == before.3;
                    let side_glide = self.glide.as_ref().map(|glide| glide.side);
                    match (moved, plain, body != 0.0, side != 0.0) {
                        (true, true, true, false) if wheel && side_glide != Some(true) => {
                            let to = self.model.scroll;
                            self.model.scroll = shown.0;
                            self.glide = Some(Glide { side: false, from: shown.0, to, shown: shown.0, since: Instant::now() });
                        }
                        (true, true, false, true) if wheel && side_glide != Some(false) => {
                            let to = self.model.tree_scroll;
                            self.model.tree_scroll = shown.1;
                            self.glide = Some(Glide { side: true, from: shown.1, to, shown: shown.1, since: Instant::now() });
                        }
                        _ => {
                            self.glide = None;
                            let (body, side) = (self.model.scroll - shown.0, self.model.tree_scroll - shown.1);
                            match (plain, body != 0.0, side != 0.0) {
                                (_, false, false) => {}
                                (true, true, false) => self.shifts.push((body, false)),
                                (true, false, true) => self.shifts.push((side, true)),
                                _ => self.stale = true,
                            }
                        }
                    }
                }
                PointerEventKind::Leave { .. } => {
                    let (hover, menu) = (self.model.hover.clone(), self.model.menu.clone());
                    let tip = self.tip_damage();
                    if input::leave(&mut self.model) {
                        let lines = self.model.lines();
                        let area = self.hover_damage(&hover, &menu, &lines);
                        self.damage(area);
                        if let Some(tip) = tip {
                            self.damage(Some(tip));
                        }
                    }
                    self.tip_plan += 1;
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
delegate_data_device!(Explorer);

impl Explorer {
    fn offer(&mut self, held: Held) {
        let (Some(data), Some(device)) = (self.data.as_ref(), self.device.as_ref()) else {
            return;
        };
        let source = data.create_copy_paste_source(&self.qh, held.kinds().iter().copied());
        source.set_selection(device, self.serial);
        self.held = Some((source, held));
    }
}

impl Explorer {
    fn lift(&mut self) {
        let Some(Drag::Files { paths, live: true, .. }) = self.model.dragging.clone() else {
            return;
        };
        if self.data.is_none() || self.device.is_none() || self.drag_out.is_some() {
            return;
        }
        let icon = self.drag_icon(&paths);
        let (Some(data), Some(device)) = (self.data.as_ref(), self.device.as_ref()) else {
            return;
        };
        let source = data.create_drag_and_drop_source(&self.qh, clip::FILE_KINDS.iter().copied(), DndAction::Copy | DndAction::Move);
        source.start_drag(device, self.window.wl_surface(), icon.as_ref().map(|(surface, _)| surface), self.press_serial);
        if let Some((surface, _)) = &icon {
            surface.commit();
        }
        self.model.dragging = None;
        self.stale = true;
        self.drag_out = Some((source, Held::Files(paths, false), icon));
    }

    fn drag_icon(&mut self, paths: &[PathBuf]) -> Option<(wl_surface::WlSurface, smithay_client_toolkit::shm::slot::Buffer)> {
        drifter_core::paint::set_density(scale());
        let lines = self.model.lines();
        let fonts = Fonts { body: &self.fonts.0, small: &self.fonts.1, title: &self.fonts.2 };
        let job = Paint { model: &self.model, lines: &lines, fonts: &fonts, art: &self.art, clip: None };
        let card = self.look.drag_card(&job, paths);
        let (width, height) = (card.width, card.height);
        let (buffer, canvas) = self.pool.create_buffer(width as i32, height as i32, width as i32 * 4, wl_shm::Format::Argb8888).ok()?;
        for (to, from) in canvas.chunks_exact_mut(4).zip(card.rgba.chunks_exact(4)) {
            to.copy_from_slice(&[from[2], from[1], from[0], from[3]]);
        }
        let surface = self.compositor.create_surface(&self.qh);
        surface.set_buffer_scale(scale() as i32);
        buffer.attach_to(&surface).ok()?;
        surface.damage_buffer(0, 0, width as i32, height as i32);
        Some((surface, buffer))
    }

    fn drop_drag_out(&mut self) {
        if let Some((_, _, Some((icon, _)))) = self.drag_out.take() {
            icon.destroy();
        }
    }

    fn landed(&mut self, target: Drop, held: Option<Held>, copy: bool) {
        if let Some(Held::Files(paths, _)) = held {
            self.model.drop_on(target, paths, copy);
            self.after();
        }
        if let Some(offer) = self.drop_offer.take() {
            offer.finish();
            offer.destroy();
        }
    }
}

impl DataDeviceHandler for Explorer {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, x: f64, y: f64, _: &wl_surface::WlSurface) {
        let Some(offer) = self.device.as_ref().and_then(|device| device.data().drag_offer()) else {
            return;
        };
        match offer.with_mime_types(clip::best_files) {
            Some(kind) => {
                offer.accept_mime_type(offer.serial, Some(kind.to_string()));
                let at = Vec2::new(x, y);
                let (preferred, paths) = match &self.drag_out {
                    Some((_, Held::Files(paths, _), _)) => (DndAction::Move, paths.clone()),
                    _ => (DndAction::Copy, Vec::new()),
                };
                let dragging = Drag::Foreign { at, paths };
                offer.set_actions(DndAction::Copy | DndAction::Move, preferred);
                self.model.dragging = Some(dragging);
                self.stale = true;
            }
            None => {
                offer.accept_mime_type(offer.serial, None);
                offer.set_actions(DndAction::empty(), DndAction::empty());
            }
        }
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        if matches!(self.model.dragging, Some(Drag::Foreign { .. } | Drag::Files { .. })) {
            self.model.dragging = None;
            self.stale = true;
        }
    }

    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, x: f64, y: f64) {
        match self.model.dragging.as_mut() {
            Some(Drag::Foreign { at, .. } | Drag::Files { at, .. }) => *at = Vec2::new(x, y),
            _ => return,
        }
        self.stale = true;
    }

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        if self.held.is_some() {
            return;
        }
        self.clip_ask += 1;
        let offer = self.device.as_ref().and_then(|device| device.data().selection_offer());
        let found = offer.as_ref().and_then(|offer| offer.with_mime_types(clip::best).map(|kind| (offer, kind)));
        let Some((offer, kind)) = found else {
            self.model.heard_clip(None);
            self.stale = true;
            return;
        };
        let Ok((reader, writer)) = std::io::pipe() else {
            return;
        };
        smithay_client_toolkit::data_device_manager::data_offer::receive_to_fd(offer.inner(), kind.to_string(), writer.into());
        let (asked, pasted) = (self.clip_ask, self.pasted.clone());
        std::thread::spawn(move || {
            use std::io::Read;
            let mut bytes = Vec::new();
            let held = match reader.take(clip::MOST as u64).read_to_end(&mut bytes) {
                Ok(_) => clip::read(kind, &bytes),
                Err(_) => None,
            };
            let _ = pasted.send((asked, held));
        });
    }

    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        self.model.dragging = None;
        self.stale = true;
        let Some(offer) = self.device.as_ref().and_then(|device| device.data().drag_offer()) else {
            return;
        };
        let at = Vec2::new(offer.x, offer.y);
        let ours = match &self.drag_out {
            Some((_, Held::Files(paths, _), _)) => paths.clone(),
            _ => Vec::new(),
        };
        let target = self.with_ui(|model, ui, lines| ui.look.drop_at(ui.size, model, lines, ui.fonts, at, &ours).map(|(target, _)| target));
        let kind = offer.with_mime_types(clip::best_files);
        let (Some(target), Some(kind)) = (target, kind) else {
            offer.destroy();
            return;
        };
        let copy = match ours.is_empty() {
            true => offer.selected_action != DndAction::Move,
            false => self.mods.ctrl,
        };
        if !ours.is_empty() {
            self.drop_offer = Some(offer);
            self.landed(target, Some(Held::Files(ours, false)), copy);
            return;
        }
        let Ok(pipe) = offer.receive(kind.to_string()) else {
            offer.destroy();
            return;
        };
        self.drop_offer = Some(offer);
        let file = std::fs::File::from(std::os::fd::OwnedFd::from(pipe));
        let dropped = self.dropped.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut bytes = Vec::new();
            let held = match file.take(clip::MOST as u64).read_to_end(&mut bytes) {
                Ok(_) => clip::read(kind, &bytes),
                Err(_) => None,
            };
            let _ = dropped.send((target, held, copy));
        });
    }
}

impl DataOfferHandler for Explorer {
    fn source_actions(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}

    fn selected_action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
}

impl DataSourceHandler for Explorer {
    fn accept_mime(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: Option<String>) {}

    fn send_request(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource, mime: String, fd: WritePipe) {
        let held = self.held.as_ref().filter(|(ours, _)| ours.inner() == source).map(|(_, held)| held);
        let dragged = self.drag_out.as_ref().filter(|(ours, _, _)| ours.inner() == source).map(|(_, held, _)| held);
        let bytes = held.or(dragged).and_then(|held| held.bytes(&mime));
        let Some(bytes) = bytes else {
            return;
        };
        let mut file = std::fs::File::from(std::os::fd::OwnedFd::from(fd));
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = file.write_all(&bytes);
        });
    }

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if self.held.as_ref().is_some_and(|(ours, _)| ours.inner() == source) {
            self.held = None;
        } else if self.drag_out.as_ref().is_some_and(|(ours, _, _)| ours.inner() == source) {
            self.drop_drag_out();
        } else {
            source.destroy();
        }
    }

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if self.drag_out.as_ref().is_some_and(|(ours, _, _)| ours.inner() == source) {
            self.drop_drag_out();
        }
    }

    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}
