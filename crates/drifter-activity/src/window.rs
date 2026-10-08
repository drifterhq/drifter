use std::collections::HashMap;
use std::time::{Duration, Instant};

use drifter_core::activity::look::{self, Fonts, Hit, Look, Model, Toast, View};
use drifter_core::activity::{self, Gone, Group, Signal};
use drifter_core::{Config, Font, Image, Vec2};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop::channel::{Event as ChannelEvent, channel as loop_channel};
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers};
use smithay_client_toolkit::seat::pointer::{BTN_LEFT, PointerEvent, PointerEventKind, PointerHandler};
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

use crate::probe::{self, Fresh};

const APP: &str = activity::ACTIVITY_APP;
const START: (u32, u32) = (1180, 700);
const SETTLE: Duration = Duration::from_millis(16);
const TOAST: Duration = Duration::from_secs(6);
const FRAME_LATE: Duration = Duration::from_millis(100);

static SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

fn scale() -> u32 {
    SCALE.load(std::sync::atomic::Ordering::Relaxed)
}

struct Activity {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    window: Window,
    qh: QueueHandle<Activity>,
    handle: smithay_client_toolkit::reexports::calloop::LoopHandle<'static, Activity>,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    mods: Modifiers,
    size: (u32, u32),
    exit: bool,
    resized: bool,
    stale: bool,
    shown: Option<Instant>,
    last: Option<Image>,
    look: Look,
    fonts: (Font, Font, Font),
    model: Model,
    icons: HashMap<String, Image>,
    missing: std::collections::HashSet<String>,
    at: Vec2,
    over_list: bool,
    holding: Option<(Instant, String)>,
    scrubbing: bool,
    gone: Option<Gone>,
    toast_at: Option<Instant>,
    wanted: Option<String>,
    tall: Option<Image>,
    scrolled_only: bool,
    config_seen: Option<std::time::SystemTime>,
    show_seen: Option<std::time::SystemTime>,
    watched_at: Instant,
    theme_checks: u64,
}

fn modified(path: &std::path::Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

pub fn show_path() -> std::path::PathBuf {
    Config::state_dir().join("activity-show")
}

fn themed(config: &Config) -> Look {
    let palette = drifter_core::theme::of(drifter_core::theme::now(config.appearance.style));
    let accent = config.appearance.accent.map(|accent| accent.color()).unwrap_or([64, 128, 238]);
    Look::themed(&palette, accent)
}

pub fn run() -> Result<(), String> {
    let path = Config::config_path();
    let config = Config::load_from(&path).unwrap_or_default();
    drifter_core::lang::use_config(config.appearance.language.as_deref());
    let find = |px| Font::find(config.font_path(), px);
    let fonts = (find(13.0)?, find(11.5)?, find(22.0)?);
    drifter_core::icon::set_theme(config.appearance.icons.clone());

    let connection = Connection::connect_to_env().map_err(|e| format!("no wayland: {e}"))?;
    let (globals, queue) = registry_queue_init(&connection).map_err(|e| format!("no wayland registry: {e}"))?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(|e| format!("no wl_compositor: {e}"))?;
    let shell = XdgShell::bind(&globals, &qh).map_err(|e| format!("no xdg shell: {e}"))?;
    let shm = Shm::bind(&globals, &qh).map_err(|e| format!("no wl_shm: {e}"))?;

    let surface = compositor.create_surface(&qh);
    let window = shell.create_window(surface, WindowDecorations::RequestServer, &qh);
    window.set_title(drifter_core::lang::t("Activity"));
    window.set_app_id(APP);
    let look = themed(&config);
    let least = look.least();
    window.set_min_size(Some((least.x as u32, least.y as u32)));
    window.commit();

    let pool = SlotPool::new((START.0 * START.1 * 4) as usize, &shm).map_err(|e| format!("no shared memory: {e}"))?;
    let mut events: EventLoop<'static, Activity> = EventLoop::try_new().map_err(|e| format!("no event loop: {e}"))?;
    WaylandSource::new(connection, queue)
        .insert(events.handle())
        .map_err(|e| format!("cannot watch wayland: {e}"))?;
    let (fresh_tx, fresh_rx) = loop_channel::<Fresh>();
    events
        .handle()
        .insert_source(fresh_rx, |event, _, app: &mut Activity| {
            if let ChannelEvent::Msg(fresh) = event {
                app.take(fresh);
            }
        })
        .map_err(|e| format!("cannot watch the readings: {e}"))?;
    events
        .handle()
        .insert_source(Timer::from_duration(Duration::from_millis(33)), |_, _, app: &mut Activity| {
            app.tick();
            let busy = app.holding.is_some();
            TimeoutAction::ToDuration(Duration::from_millis(if busy { 33 } else { 500 }))
        })
        .map_err(|e| format!("cannot start the timer: {e}"))?;
    probe::start(fresh_tx);

    let mut model = Model::default();
    let wanted = std::env::args().nth(1).filter(|word| word.chars().all(|c| c.is_ascii_digit()));
    if wanted.is_some() {
        model.selected = wanted.clone();
    }
    if std::env::args().nth(1).as_deref() == Some("hour") {
        model.view = View::Hour;
    }
    model.hour = probe::read_hour();
    model.moments = activity::hour::moments(&model.hour);
    model.now = probe::now();

    let mut app = Activity {
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
        mods: Modifiers::default(),
        size: START,
        exit: false,
        resized: false,
        stale: true,
        shown: None,
        last: None,
        look,
        fonts,
        model,
        icons: HashMap::new(),
        missing: std::collections::HashSet::new(),
        at: Vec2::ZERO,
        over_list: false,
        holding: None,
        scrubbing: false,
        gone: None,
        toast_at: None,
        wanted,
        tall: None,
        scrolled_only: false,
        config_seen: modified(&path),
        show_seen: modified(&show_path()),
        watched_at: Instant::now(),
        theme_checks: 0,
    };
    while !app.exit {
        events
            .dispatch(app.stale.then_some(SETTLE), &mut app)
            .map_err(|e| format!("event loop: {e}"))?;
        if std::mem::take(&mut app.resized) || (app.stale && app.frame_free()) {
            app.draw();
        }
    }
    Ok(())
}

fn icon_image(name: &str, side: u32) -> Option<Image> {
    let path = match name.starts_with('/') {
        true => std::path::PathBuf::from(name),
        false => drifter_core::icon::find(name, None, side.max(32))?,
    };
    let px = side * scale();
    drifter_core::image::load_sized(&path, px).ok().map(|image| image.scaled(px, px))
}

impl Activity {
    fn view(&self) -> Vec2 {
        Vec2::new(f64::from(self.size.0), f64::from(self.size.1))
    }

    fn fonts(&self) -> Fonts<'_> {
        Fonts { body: &self.fonts.0, small: &self.fonts.1, title: &self.fonts.2 }
    }

    fn frame_free(&self) -> bool {
        self.shown.is_none_or(|at| at.elapsed() >= FRAME_LATE)
    }

    fn take(&mut self, fresh: Fresh) {
        self.model.groups = fresh.groups;
        self.model.memory = fresh.memory;
        self.model.cpu = fresh.cpu;
        self.model.cores = fresh.cores;
        self.model.pressure = fresh.pressure;
        self.model.now = fresh.now;
        if let Some(power) = fresh.power {
            self.model.power = power;
        }
        if let Some(disk) = fresh.disk {
            self.model.disk = disk;
        }
        if let Some(hour) = fresh.hour {
            self.model.moments = activity::hour::moments(&hour);
            self.model.hour = hour;
            let oldest = self.model.hour.first().map_or(0, |reading| reading.at);
            if self.model.cursor.is_some_and(|at| at < oldest) {
                self.model.cursor = None;
            }
        }
        self.model.remember();
        if !self.over_list || self.model.order.is_empty() {
            self.model.arrange();
        }
        if let Some(wanted) = self.wanted.take() {
            match self.model.groups.iter().find(|group| group.key == wanted) {
                Some(group) => {
                    self.model.view = if look::View::Apps.holds(group) { View::Apps } else { View::Background };
                    self.model.selected = Some(wanted);
                    self.model.arrange();
                }
                None => self.model.selected = None,
            }
        }
        let still = self.model.selected.as_ref().is_some_and(|key| self.model.groups.iter().any(|group| &group.key == key));
        if !still {
            self.model.selected = self.model.shown().first().map(|group| group.key.clone());
        }
        self.load_icons();
        self.clamp_scroll();
        self.stale = true;
        self.scrolled_only = false;
    }

    fn load_icons(&mut self) {
        let names: Vec<String> = self
            .model
            .groups
            .iter()
            .filter_map(|group| group.icon.clone())
            .chain(self.model.hour.iter().flat_map(|reading| reading.top.iter().filter_map(|share| share.icon.clone())))
            .collect();
        for name in names {
            if self.icons.contains_key(&name) || self.missing.contains(&name) {
                continue;
            }
            match icon_image(&name, look::ICON_SMALL) {
                Some(image) => {
                    self.icons.insert(name.clone(), image);
                    if let Some(big) = icon_image(&name, look::ICON_BIG) {
                        self.icons.insert(look::big_key(&name), big);
                    }
                }
                None => {
                    self.missing.insert(name);
                }
            }
        }
    }

    fn clamp_scroll(&mut self) {
        let most = self.look.most_scroll(self.view(), &self.model);
        self.model.scroll = self.model.scroll.clamp(0.0, most);
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
        let show = modified(&show_path());
        if show != self.show_seen {
            self.show_seen = show;
            let key = std::fs::read_to_string(show_path()).unwrap_or_default().trim().to_string();
            if !key.is_empty() {
                self.wanted = Some(key);
            }
        }
    }

    fn tick(&mut self) {
        self.watch();
        if let Some((started, key)) = self.holding.clone() {
            let hold = (started.elapsed().as_secs_f64() / look::hold_seconds()).min(1.0);
            self.model.hold = hold;
            if hold >= 1.0 {
                self.holding = None;
                self.model.hold = 0.0;
                if let Some(group) = self.model.groups.iter().find(|group| group.key == key).cloned() {
                    self.stop(&group, Signal::Force);
                }
                self.stale = true;
            } else {
                self.draw_hold();
            }
        }
        if self.toast_at.is_some_and(|at| at.elapsed() >= TOAST) {
            self.toast_at = None;
            self.model.toast = None;
            self.gone = None;
            self.stale = true;
        }
    }

    fn draw(&mut self) {
        self.stale = false;
        drifter_core::paint::set_density(scale());
        let scrolled = std::mem::take(&mut self.scrolled_only);
        if scrolled
            && self.model.view != View::Hour
            && let (Some(mut frame), Some(tall)) = (self.last.take(), self.tall.as_ref())
        {
            self.look.scroll_into(&mut frame, self.view(), &self.model, tall);
            self.show(&frame, None);
            self.last = Some(frame);
            return;
        }
        let (image, tall) = {
            let fonts = self.fonts();
            let tall = (self.model.view != View::Hour).then(|| self.look.body_layer(self.view(), &self.model, &fonts, &self.icons));
            (self.look.paint(self.view(), &self.model, &fonts, &self.icons), tall)
        };
        self.tall = tall;
        self.show(&image, None);
        self.last = Some(image);
    }

    fn draw_hold(&mut self) {
        if !self.frame_free() || self.stale {
            return;
        }
        drifter_core::paint::set_density(scale());
        let (rect, piece) = {
            let fonts = self.fonts();
            self.look.paint_hold_piece(self.view(), self.model.hold, &fonts)
        };
        let Some(mut image) = self.last.take() else {
            self.stale = true;
            return;
        };
        let mut canvas = drifter_core::paint::Canvas { image };
        canvas.stamp(&piece, rect.origin, 1.0);
        image = canvas.image;
        self.show(&image, Some(rect));
        self.last = Some(image);
    }

    fn show(&mut self, image: &Image, part: Option<drifter_core::Rect>) {
        let (width, height) = (image.width, image.height);
        let stride = width as i32 * 4;
        let (buffer, canvas) = match self.pool.create_buffer(width as i32, height as i32, stride, wl_shm::Format::Argb8888) {
            Ok(made) => made,
            Err(error) => {
                log!("drifter-activity: no buffer: {error}");
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
            Some(rect) => {
                let dense = f64::from(scale());
                surface.damage_buffer(
                    (rect.origin.x * dense).floor() as i32,
                    (rect.origin.y * dense).floor() as i32,
                    (rect.size.x * dense).ceil() as i32 + 1,
                    (rect.size.y * dense).ceil() as i32 + 1,
                );
            }
            None => surface.damage_buffer(0, 0, width as i32, height as i32),
        }
        if let Err(error) = buffer.attach_to(surface) {
            log!("drifter-activity: {error}");
            return;
        }
        surface.frame(&self.qh, surface.clone());
        self.shown = Some(Instant::now());
        self.window.commit();
    }

    fn chosen(&self) -> Option<Group> {
        self.model.chosen().cloned()
    }

    fn stop(&mut self, group: &Group, signal: Signal) {
        if !group.can_stop() {
            return;
        }
        if matches!(signal, Signal::Quit | Signal::Force) {
            self.gone = Some(Gone::of(group));
        }
        let result = activity::send(signal, group);
        let words = match (&result, signal) {
            (Err(_), signal) => signal.failed(&group.name),
            (Ok(()), Signal::Pause) => drifter_core::lang::tf("Paused {}.", &[&(group.name)]),
            (Ok(()), Signal::Resume) => drifter_core::lang::tf("{} runs again.", &[&(group.name)]),
            (Ok(()), Signal::Quit) => drifter_core::lang::tf("Asked {} to quit.", &[&(group.name)]),
            (Ok(()), Signal::Force) => drifter_core::lang::tf("Force quit {}.", &[&(group.name)]),
        };
        if let Ok(()) = result
            && let Some(held) = self.model.groups.iter_mut().find(|held| held.key == group.key)
        {
            match signal {
                Signal::Pause => {
                    held.paused = true;
                    held.detail = drifter_core::lang::t("Paused").to_string();
                }
                Signal::Resume => held.paused = false,
                _ => {}
            }
        }
        let reopen = result.is_ok() && matches!(signal, Signal::Quit | Signal::Force) && self.gone.as_ref().is_some_and(|gone| !gone.argv.is_empty());
        self.model.toast = Some(Toast { words, reopen });
        self.toast_at = Some(Instant::now());
        self.stale = true;
    }

    fn reopen(&mut self) {
        let Some(gone) = self.gone.take() else {
            return;
        };
        let Some((program, rest)) = gone.argv.split_first() else {
            return;
        };
        let mut command = std::process::Command::new(program);
        command
            .args(rest)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if let Some(dir) = gone.cwd.as_deref().filter(|dir| std::path::Path::new(dir).is_dir()) {
            command.current_dir(dir);
        }
        let words = match command.spawn() {
            Ok(mut child) => {
                std::thread::spawn(move || {
                    let _ = child.wait();
                });
                drifter_core::lang::tf("Opened {} again.", &[&(gone.name)])
            }
            Err(error) => drifter_core::lang::tf("{} did not open: {}.", &[&(gone.name), &(error)]),
        };
        self.model.toast = Some(Toast { words, reopen: false });
        self.toast_at = Some(Instant::now());
        self.stale = true;
    }

    fn press(&mut self) {
        self.scrolled_only = false;
        let hit = {
            let fonts = self.fonts();
            self.look.hit(self.view(), &self.model, &fonts, self.at)
        };
        let Some(hit) = hit else {
            return;
        };
        match hit {
            Hit::View(view) => {
                if self.model.view != view {
                    self.model.view = view;
                    self.model.scroll = 0.0;
                    self.model.arrange();
                    let fits = self.model.chosen().is_some_and(|group| view.holds(group));
                    if !fits {
                        self.model.selected = self.model.shown().first().map(|group| group.key.clone());
                    }
                }
            }
            Hit::Search => {}
            Hit::Sort(column) => {
                self.model.sort = column;
                self.model.arrange();
            }
            Hit::Caret(key) => {
                if !self.model.open.remove(&key) {
                    self.model.open.insert(key.clone());
                }
                self.model.selected = Some(key);
                self.clamp_scroll();
            }
            Hit::Row(key) => self.model.selected = Some(key),
            Hit::Pause => {
                if let Some(group) = self.chosen() {
                    let signal = if group.paused { Signal::Resume } else { Signal::Pause };
                    self.stop(&group, signal);
                }
            }
            Hit::Quit => {
                if let Some(group) = self.chosen() {
                    if group.paused {
                        let _ = activity::send(Signal::Resume, &group);
                    }
                    self.stop(&group, Signal::Quit);
                }
            }
            Hit::Force => {
                if let Some(group) = self.chosen() {
                    self.holding = Some((Instant::now(), group.key.clone()));
                    self.model.hold = 0.0;
                }
            }
            Hit::Reopen => self.reopen(),
            Hit::Measure(measure) => self.model.measure = measure,
            Hit::Chart(at) => {
                self.model.cursor = self.model.hour.get(at).map(|reading| reading.at);
                self.scrubbing = true;
            }
        }
        self.stale = true;
    }

    fn motion(&mut self) {
        let size = self.view();
        self.over_list = self.look.over_list(size, self.model.view, self.at);
        if self.scrubbing {
            let fonts = Fonts { body: &self.fonts.0, small: &self.fonts.1, title: &self.fonts.2 };
            if let Some(Hit::Chart(at)) = self.look.hit(size, &self.model, &fonts, self.at) {
                let when = self.model.hour.get(at).map(|reading| reading.at);
                if self.model.cursor != when {
                    self.model.cursor = when;
                    self.stale = true;
                }
            }
            return;
        }
        let hover = match self.over_list {
            true => {
                let fonts = Fonts { body: &self.fonts.0, small: &self.fonts.1, title: &self.fonts.2 };
                match self.look.hit(size, &self.model, &fonts, self.at) {
                    Some(Hit::Row(key)) | Some(Hit::Caret(key)) => Some(key),
                    _ => None,
                }
            }
            false => None,
        };
        self.model.hover = hover;
        if let Some((_, key)) = self.holding.as_ref() {
            let on = self.look.hold_rect(size).contains(self.at);
            if !on || self.model.selected.as_ref() != Some(key) {
                self.let_go();
            }
        }
    }

    fn let_go(&mut self) {
        self.scrubbing = false;
        if self.holding.take().is_some() {
            self.model.hold = 0.0;
            self.stale = true;
        }
    }

    fn scroll_by(&mut self, by: f64) {
        if self.model.view == View::Hour {
            return;
        }
        let before = self.model.scroll;
        self.model.scroll += by;
        self.clamp_scroll();
        if self.model.scroll != before {
            self.scrolled_only = !self.stale || self.scrolled_only;
            self.stale = true;
        }
    }

    fn move_selection(&mut self, step: i64) {
        let shown: Vec<String> = self.model.shown().iter().map(|group| group.key.clone()).collect();
        if shown.is_empty() {
            return;
        }
        let at = self.model.selected.as_ref().and_then(|key| shown.iter().position(|held| held == key));
        let next = match at {
            Some(at) => (at as i64 + step).clamp(0, shown.len() as i64 - 1) as usize,
            None => 0,
        };
        self.model.selected = Some(shown[next].clone());
        let rows = self.look.rows(&self.model);
        if let Some((top, tall, _)) = rows.iter().find(|(_, _, line)| matches!(line, look::Line::Group(key) if key == &shown[next])) {
            let body = self.look.table_rect(self.view(), self.model.view).size.y - 36.0;
            if *top < self.model.scroll {
                self.model.scroll = *top;
            } else if top + tall > self.model.scroll + body {
                self.model.scroll = top + tall - body;
            }
        }
        self.clamp_scroll();
        self.stale = true;
    }

    fn key(&mut self, event: &KeyEvent) {
        self.scrolled_only = false;
        match event.keysym {
            Keysym::Up => return self.move_selection(-1),
            Keysym::Down => return self.move_selection(1),
            Keysym::Return | Keysym::KP_Enter => {
                if let Some(key) = self.model.selected.clone()
                    && !self.model.open.remove(&key)
                {
                    self.model.open.insert(key);
                }
                self.clamp_scroll();
                self.stale = true;
                return;
            }
            _ => {}
        }
        let before = self.model.query.clone();
        match event.keysym {
            Keysym::Escape => self.model.query.clear(),
            Keysym::BackSpace => {
                self.model.query.pop();
            }
            _ => match event.utf8.as_deref() {
                Some(text) if !text.chars().any(char::is_control) && !self.mods.ctrl && !self.mods.logo => {
                    self.model.query.push_str(text);
                    if self.model.view == View::Hour {
                        self.model.view = View::Everything;
                    }
                }
                _ => return,
            },
        }
        if self.model.query != before {
            self.model.scroll = 0.0;
            self.model.arrange();
            if self.model.chosen().is_none_or(|group| !self.model.shown().iter().any(|shown| shown.key == group.key)) {
                self.model.selected = self.model.shown().first().map(|group| group.key.clone());
            }
            self.stale = true;
        }
    }
}

impl CompositorHandler for Activity {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, factor: i32) {
        let factor = factor.clamp(1, 4) as u32;
        if factor == scale() {
            return;
        }
        SCALE.store(factor, std::sync::atomic::Ordering::Relaxed);
        drifter_core::paint::set_density(factor);
        self.icons.clear();
        self.missing.clear();
        self.load_icons();
        self.stale = true;
    }

    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        self.shown = None;
    }

    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}

    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for Activity {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl WindowHandler for Activity {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.exit = true;
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window, configure: WindowConfigure, _: u32) {
        let least = self.look.least();
        let width = configure.new_size.0.map(|v| v.get()).unwrap_or(self.size.0);
        let height = configure.new_size.1.map(|v| v.get()).unwrap_or(self.size.1);
        self.size = (width.max(least.x as u32), height.max(least.y as u32));
        self.clamp_scroll();
        self.resized = true;
    }
}

impl SeatHandler for Activity {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            match self.seats.get_pointer(qh, &seat) {
                Ok(pointer) => self.pointer = Some(pointer),
                Err(error) => log!("drifter-activity: no pointer: {error}"),
            }
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let repeat = Box::new(|app: &mut Activity, _: &wl_keyboard::WlKeyboard, event: KeyEvent| app.key(&event));
            match self.seats.get_keyboard_with_repeat(qh, &seat, None, self.handle.clone(), repeat) {
                Ok(keyboard) => self.keyboard = Some(keyboard),
                Err(error) => log!("drifter-activity: no keyboard: {error}"),
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
            keyboard.release();
        }
    }

    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Activity {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {}

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {}

    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, event: KeyEvent) {
        self.key(&event);
    }

    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}

    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, modifiers: Modifiers, _: u32) {
        self.mods = modifiers;
    }
}

impl PointerHandler for Activity {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for event in events {
            if &event.surface != self.window.wl_surface() {
                continue;
            }
            self.at = Vec2::new(event.position.0, event.position.1);
            match event.kind {
                PointerEventKind::Motion { .. } => self.motion(),
                PointerEventKind::Press { button, .. } if button == BTN_LEFT => self.press(),
                PointerEventKind::Release { button, .. } if button == BTN_LEFT => self.let_go(),
                PointerEventKind::Axis { vertical, .. } => self.scroll_by(vertical.absolute),
                PointerEventKind::Leave { .. } => {
                    self.let_go();
                    self.over_list = false;
                    self.model.hover = None;
                }
                _ => {}
            }
        }
    }
}

impl ShmHandler for Activity {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Activity {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(Activity);
delegate_output!(Activity);
delegate_shm!(Activity);
delegate_seat!(Activity);
delegate_pointer!(Activity);
delegate_keyboard!(Activity);
delegate_xdg_shell!(Activity);
delegate_xdg_window!(Activity);
delegate_registry!(Activity);
