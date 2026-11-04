use std::os::fd::{AsFd, BorrowedFd, RawFd};
use std::time::{Duration, Instant, SystemTime};

use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::data_device_manager::data_device::{DataDevice, DataDeviceHandler};
use smithay_client_toolkit::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use smithay_client_toolkit::data_device_manager::data_source::{CopyPasteSource, DataSourceHandler};
use smithay_client_toolkit::data_device_manager::{DataDeviceManagerState, WritePipe};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::channel::{Channel, Event as Sent, Sender, channel};
use smithay_client_toolkit::reexports::calloop::generic::Generic;
use smithay_client_toolkit::reexports::calloop::{EventLoop, Interest, LoopHandle, Mode, PostAction, RegistrationToken};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym, KeyboardHandler, Modifiers};
use smithay_client_toolkit::seat::pointer::{BTN_LEFT, BTN_MIDDLE, BTN_RIGHT, PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::XdgShell;
use smithay_client_toolkit::shell::xdg::window::{Window, WindowConfigure, WindowDecorations, WindowHandler};
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_data_device, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry,
    delegate_seat, delegate_shm, delegate_xdg_shell, delegate_xdg_window, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::wl_data_device::WlDataDevice;
use wayland_client::protocol::wl_data_device_manager::DndAction;
use wayland_client::protocol::wl_data_source::WlDataSource;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

use drifter_core::glide::Glide;
use drifter_core::menu::Look;
use drifter_core::paint::Canvas;
use drifter_core::{Font, Image, Rect, Vec2};

use crate::config::Config;
use crate::pty;
use crate::render::Grid;
use crate::select::{Lines, Sel, Unit};
use crate::tabs;
use crate::ui::{self, Action, Menu};
use crate::vt::Vt;

const START: (u32, u32) = (960, 600);
const APP_ID: &str = "drifter-terminal";
const SETTINGS_APP: &str = "drifter-settings";
const RECOLOR: Duration = Duration::from_millis(1000);
const DOUBLE_CLICK: Duration = Duration::from_millis(400);
const LOOK_AGAIN: Duration = Duration::from_secs(1);
const RELOAD: Duration = Duration::from_secs(60);
const DRAG_START: f64 = 6.0;
const WHEEL_LINES: isize = 3;
const PASTE_MOST: u64 = 8 << 20;
const TEXT_KINDS: [&str; 5] = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING", "TEXT", "STRING"];

struct Fd(RawFd);
impl AsFd for Fd {
    fn as_fd(&self) -> BorrowedFd<'_> {
        unsafe { BorrowedFd::borrow_raw(self.0) }
    }
}

struct Pane {
    vt: Vt,
    fd: RawFd,
    child: libc::pid_t,
    token: RegistrationToken,
    title: String,
    name: Option<String>,
    sel: Option<Sel>,
    lines: Lines,
}

impl Pane {
    fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.title)
    }

    fn clear_selection(&mut self) {
        self.sel = None;
        self.lines.clear();
    }
}

struct TabHold {
    fd: RawFd,
    from: Vec2,
    moving: bool,
    group: Vec<RawFd>,
}

enum Hold {
    Tab(TabHold),
    Text,
}

struct Win {
    window: Window,
    size: (u32, u32),
    configured: bool,
    resized: bool,
    needs_draw: bool,
    panes: Vec<Pane>,
    active: usize,
    picked: Vec<RawFd>,
    menu: Option<Menu>,
    lit: Option<Glide>,
    blank_click: Option<Instant>,
    tab_click: Option<(RawFd, Instant)>,
    text_click: Option<(Instant, u8)>,
    hold: Option<Hold>,
    rename: Option<(RawFd, String)>,
    shown: Option<Image>,
    fade: Option<(Image, Instant)>,
    named: String,
}

impl Win {
    fn new(window: Window, panes: Vec<Pane>) -> Win {
        Win {
            window,
            size: START,
            configured: false,
            resized: true,
            needs_draw: true,
            panes,
            active: 0,
            picked: Vec::new(),
            menu: None,
            lit: None,
            blank_click: None,
            tab_click: None,
            text_click: None,
            hold: None,
            rename: None,
            shown: None,
            fade: None,
            named: String::new(),
        }
    }

    fn busy(&self) -> bool {
        self.lit.as_ref().is_some_and(Glide::busy) || self.fade.is_some()
    }

    fn at(&self, fd: RawFd) -> Option<usize> {
        self.panes.iter().position(|pane| pane.fd == fd)
    }

    fn active_fd(&self) -> Option<RawFd> {
        self.panes.get(self.active).map(|pane| pane.fd)
    }

    fn show_fd(&mut self, fd: RawFd) {
        if let Some(at) = self.at(fd) {
            self.active = at;
        }
    }
}

struct Term {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    compositor: CompositorState,
    shell: XdgShell,
    qh: QueueHandle<Term>,
    handle: LoopHandle<'static, Term>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    data: Option<DataDeviceManagerState>,
    device: Option<DataDevice>,
    clip: Option<(CopyPasteSource, String)>,
    pasted: Sender<(RawFd, String)>,
    serial: u32,
    mods: Modifiers,
    cfg: Config,
    zoom: f64,
    seen: Option<SystemTime>,
    looked: Instant,
    loaded: Instant,
    grid: Grid,
    ui_font: Font,
    menu_look: Look,
    accent: [u8; 3],
    on_accent: [u8; 3],
    bar: [u8; 3],
    dim: [u8; 3],
    wins: Vec<Win>,
    focus: Option<wl_surface::WlSurface>,
    pointer_pos: Vec2,
    wheel: f64,
}

fn mix(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    [0, 1, 2].map(|i| (f64::from(a[i]) + (f64::from(b[i]) - f64::from(a[i])) * t).round() as u8)
}

fn style(cfg: &Config) -> (Look, [u8; 3], [u8; 3]) {
    let mut look = Look::themed(&cfg.palette);
    look.accent = cfg.accent;
    let lift = if cfg.palette == drifter_core::theme::LIGHT { [0, 0, 0] } else { [255, 255, 255] };
    let bar = mix(cfg.bg, lift, 0.06);
    let dim = mix(cfg.fg, cfg.bg, 0.35);
    (look, bar, dim)
}

fn start(program: std::path::PathBuf, args: &[&str]) {
    match std::process::Command::new(&program).args(args).spawn() {
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
        }
        Err(error) => log!("drifter-terminal: cannot start {}: {error}", program.display()),
    }
}

fn pty_source(handle: &LoopHandle<'static, Term>, fd: RawFd) -> Result<RegistrationToken, String> {
    let source = Generic::new(Fd(fd), Interest::READ, Mode::Level);
    handle
        .insert_source(source, move |_, _, term: &mut Term| {
            term.drain_fd(fd);
            Ok(PostAction::Continue)
        })
        .map_err(|e| format!("cannot watch the shell: {e}"))
}

fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    let clean = text.replace("\x1b[201~", "");
    match bracketed {
        true => format!("\x1b[200~{clean}\x1b[201~").into_bytes(),
        false => clean.replace("\r\n", "\r").replace('\n', "\r").into_bytes(),
    }
}

pub fn run() -> Result<(), String> {
    let seen = Config::stamp();
    let cfg = Config::load();
    let grid = Grid::new(&cfg)?;
    let ui_font = crate::font::ui(13.0)?;
    let (menu_look, bar, dim) = style(&cfg);
    let accent = menu_look.accent;
    let on_accent = menu_look.on_accent;

    let connection = Connection::connect_to_env().map_err(|e| format!("no wayland: {e}"))?;
    let (globals, queue) = registry_queue_init(&connection).map_err(|e| format!("no wayland registry: {e}"))?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(|e| format!("no wl_compositor: {e}"))?;
    let shell = XdgShell::bind(&globals, &qh).map_err(|e| format!("no xdg shell: {e}"))?;
    let shm = Shm::bind(&globals, &qh).map_err(|e| format!("no wl_shm: {e}"))?;

    let pool = SlotPool::new((START.0 * START.1 * 4) as usize, &shm).map_err(|e| format!("no shared memory: {e}"))?;
    let mut events: EventLoop<'static, Term> = EventLoop::try_new().map_err(|e| format!("no event loop: {e}"))?;
    WaylandSource::new(connection, queue)
        .insert(events.handle())
        .map_err(|e| format!("cannot watch wayland: {e}"))?;
    let handle = events.handle();
    let (pasted, heard): (Sender<(RawFd, String)>, Channel<(RawFd, String)>) = channel();
    handle
        .insert_source(heard, |event, _, term: &mut Term| {
            if let Sent::Msg((fd, text)) = event {
                term.paste_into(fd, &text);
            }
        })
        .map_err(|e| format!("cannot watch the clipboard: {e}"))?;

    let mut term = Term {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        shm,
        pool,
        data: DataDeviceManagerState::bind(&globals, &qh).ok(),
        compositor,
        shell,
        qh: qh.clone(),
        handle,
        keyboard: None,
        pointer: None,
        device: None,
        clip: None,
        pasted,
        serial: 0,
        mods: Modifiers::default(),
        cfg,
        zoom: 0.0,
        seen,
        looked: Instant::now(),
        loaded: Instant::now(),
        grid,
        ui_font,
        menu_look,
        accent,
        on_accent,
        bar,
        dim,
        wins: Vec::new(),
        focus: None,
        pointer_pos: Vec2::ZERO,
        wheel: 0.0,
    };
    let first = term.make_pane(START, false)?;
    term.open_window(vec![first]);

    while !term.wins.is_empty() {
        let busy = term.wins.iter().any(Win::busy);
        let wait = if busy { Duration::from_millis(16) } else { Duration::from_millis(200) };
        events.dispatch(Some(wait), &mut term).map_err(|e| format!("event loop: {e}"))?;
        term.follow_config();
        for at in 0..term.wins.len() {
            if !term.wins[at].configured {
                continue;
            }
            if std::mem::take(&mut term.wins[at].resized) {
                term.fit_panes(at, false);
            }
            term.name_window(at);
            if std::mem::take(&mut term.wins[at].needs_draw) || term.wins[at].busy() {
                term.draw(at);
            }
        }
    }
    Ok(())
}

impl Term {
    fn tabs_shown(&self, at: usize) -> bool {
        self.wins[at].panes.len() > 1 || self.cfg.tab_always
    }

    fn content(&self, at: usize) -> Rect {
        ui::card_rect(self.cfg.tab_edge, self.wins[at].size, self.tabs_shown(at))
    }

    fn fit_panes(&mut self, at: usize, cells_changed: bool) {
        let card = self.content(at);
        let (cols, rows) = self.grid.fit(card.size.x, card.size.y);
        let (cw, ch) = self.grid.cell();
        let win = &mut self.wins[at];
        for pane in &mut win.panes {
            if cells_changed || (cols, rows) != (pane.vt.cols, pane.vt.rows) {
                pty::resize(pane.fd, cols, rows);
                pane.vt.resize(cols, rows, cw, ch);
                pane.clear_selection();
            }
        }
        win.needs_draw = true;
    }

    fn make_pane(&self, size: (u32, u32), tabs: bool) -> Result<Pane, String> {
        let card = ui::card_rect(self.cfg.tab_edge, size, tabs);
        let (cols, rows) = self.grid.fit(card.size.x, card.size.y);
        let p = pty::spawn(cols, rows)?;
        let mut vt = Vt::new(cols, rows, p.fd)?;
        vt.paint_with(&self.cfg.theme);
        let token = pty_source(&self.handle, p.fd)?;
        Ok(Pane { vt, fd: p.fd, child: p.child, token, title: "Terminal".into(), name: None, sel: None, lines: Lines::new() })
    }

    fn open_window(&mut self, panes: Vec<Pane>) {
        let surface = self.compositor.create_surface(&self.qh);
        let window = self.shell.create_window(surface, WindowDecorations::RequestServer, &self.qh);
        window.set_title("Terminal");
        window.set_app_id(APP_ID);
        window.set_min_size(Some((320, 200)));
        window.commit();
        self.wins.push(Win::new(window, panes));
    }

    fn win_of(&self, surface: &wl_surface::WlSurface) -> Option<usize> {
        self.wins.iter().position(|win| win.window.wl_surface() == surface)
    }

    fn focused(&self) -> Option<usize> {
        self.focus.as_ref().and_then(|surface| self.win_of(surface))
    }

    fn find_fd(&self, fd: RawFd) -> Option<(usize, usize)> {
        self.wins.iter().enumerate().find_map(|(w, win)| win.at(fd).map(|p| (w, p)))
    }

    fn new_tab(&mut self, at: usize) {
        let one_before = self.wins[at].panes.len() == 1;
        match self.make_pane(self.wins[at].size, true) {
            Ok(pane) => {
                let win = &mut self.wins[at];
                win.panes.push(pane);
                win.active = win.panes.len() - 1;
                win.picked.clear();
                if one_before {
                    self.fit_panes(at, false);
                }
                self.wins[at].needs_draw = true;
            }
            Err(error) => log!("drifter-terminal: {error}"),
        }
    }

    fn new_window(&mut self) {
        match self.make_pane(START, self.cfg.tab_always) {
            Ok(pane) => self.open_window(vec![pane]),
            Err(error) => log!("drifter-terminal: {error}"),
        }
    }

    fn open_settings(&self) {
        let beside = std::env::current_exe().ok().map(|exe| exe.with_file_name(SETTINGS_APP));
        let program = beside.filter(|path| path.is_file()).unwrap_or_else(|| SETTINGS_APP.into());
        start(program, &["terminal"]);
    }

    fn reap(&self) {
        loop {
            let mut status = 0;
            if unsafe { libc::waitpid(-1, &mut status, libc::WNOHANG) } <= 0 {
                break;
            }
        }
    }

    fn follow_config(&mut self) {
        if self.looked.elapsed() < LOOK_AGAIN {
            return;
        }
        self.looked = Instant::now();
        self.reap();
        let stamp = Config::stamp();
        if stamp == self.seen && self.loaded.elapsed() < RELOAD {
            return;
        }
        self.seen = stamp;
        self.loaded = Instant::now();
        let fresh = Config::load();
        if fresh != self.cfg {
            self.take_config(fresh);
        }
    }

    fn take_config(&mut self, fresh: Config) {
        let regrid = fresh.font != self.cfg.font || fresh.font_px != self.cfg.font_px || fresh.pad != self.cfg.pad;
        let recolor = fresh.theme != self.cfg.theme || fresh.palette != self.cfg.palette || fresh.cursor != self.cfg.cursor;
        for win in &mut self.wins {
            if recolor && !fresh.still {
                win.fade = win.shown.take().map(|image| (image, Instant::now()));
            }
            if fresh.theme != self.cfg.theme {
                for pane in &mut win.panes {
                    pane.vt.paint_with(&fresh.theme);
                }
            }
            win.menu = None;
            win.resized = true;
            win.needs_draw = true;
        }
        self.cfg = fresh;
        let (look, bar, dim) = style(&self.cfg);
        self.accent = look.accent;
        self.on_accent = look.on_accent;
        self.menu_look = look;
        self.bar = bar;
        self.dim = dim;
        let mut sized = self.cfg.clone();
        sized.font_px = (sized.font_px + self.zoom).clamp(6.0, 48.0);
        if let Ok(grid) = Grid::new(&sized) {
            self.grid = grid;
        }
        if regrid {
            for at in 0..self.wins.len() {
                self.fit_panes(at, true);
            }
        }
    }

    fn close_fd(&mut self, fd: RawFd) {
        let Some((w, i)) = self.find_fd(fd) else {
            return;
        };
        let pane = self.wins[w].panes.remove(i);
        self.handle.remove(pane.token);
        unsafe {
            libc::close(pane.fd);
            let mut status = 0;
            libc::waitpid(pane.child, &mut status, libc::WNOHANG);
        }
        let win = &mut self.wins[w];
        win.picked.retain(|each| *each != fd);
        if win.rename.as_ref().is_some_and(|(held, _)| *held == fd) {
            win.rename = None;
        }
        if win.panes.is_empty() {
            self.wins.remove(w);
            return;
        }
        if win.active >= win.panes.len() {
            win.active = win.panes.len() - 1;
        } else if win.active > i {
            win.active -= 1;
        }
        let one = win.panes.len() == 1;
        win.needs_draw = true;
        if one {
            self.fit_panes(w, false);
        }
    }

    fn close_window(&mut self, at: usize) {
        let fds: Vec<RawFd> = self.wins[at].panes.iter().map(|pane| pane.fd).collect();
        for fd in fds {
            self.close_fd(fd);
        }
    }

    fn switch(&mut self, at: usize, forward: bool) {
        let win = &mut self.wins[at];
        let count = win.panes.len();
        if count < 2 {
            return;
        }
        win.active = if forward { (win.active + 1) % count } else { (win.active + count - 1) % count };
        win.picked.clear();
        win.needs_draw = true;
    }

    fn drain_fd(&mut self, fd: RawFd) {
        let mut buf = [0u8; 16384];
        let mut closed = false;
        loop {
            let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len()) };
            if n > 0 {
                if let Some((w, p)) = self.find_fd(fd) {
                    let win = &mut self.wins[w];
                    let pane = &mut win.panes[p];
                    pane.vt.feed(&buf[..n as usize]);
                    let title = pane.vt.title();
                    let title = if title.is_empty() { "Terminal".to_string() } else { title };
                    if title != pane.title {
                        pane.title = title;
                    }
                    win.needs_draw = true;
                }
            } else if n == 0 {
                closed = true;
                break;
            } else {
                match std::io::Error::last_os_error().raw_os_error() {
                    Some(libc::EAGAIN) => break,
                    Some(libc::EINTR) => continue,
                    _ => {
                        closed = true;
                        break;
                    }
                }
            }
        }
        if closed {
            self.close_fd(fd);
        }
    }

    fn name_window(&mut self, at: usize) {
        let win = &mut self.wins[at];
        let Some(label) = win.panes.get(win.active).map(|pane| pane.label().to_string()) else {
            return;
        };
        if label != win.named {
            win.window.set_title(label.clone());
            win.named = label;
        }
    }

    fn to_pty(&mut self, at: usize, bytes: &[u8]) {
        let win = &mut self.wins[at];
        if let Some(pane) = win.panes.get_mut(win.active) {
            if pane.vt.scrolled_back() {
                pane.vt.scroll_bottom();
                win.needs_draw = true;
            }
            if pane.sel.is_some() {
                pane.clear_selection();
                win.needs_draw = true;
            }
            pty::write_all(pane.fd, bytes);
        }
    }

    fn rename_key(&mut self, at: usize, event: &KeyEvent) -> bool {
        let win = &mut self.wins[at];
        let Some((fd, text)) = win.rename.as_mut() else {
            return false;
        };
        let fd = *fd;
        match event.keysym {
            Keysym::Return | Keysym::KP_Enter => {
                let name = text.trim().to_string();
                win.rename = None;
                if let Some(p) = win.at(fd) {
                    win.panes[p].name = (!name.is_empty()).then_some(name);
                }
            }
            Keysym::Escape => win.rename = None,
            Keysym::BackSpace => {
                text.pop();
            }
            _ => {
                if let Some(typed) = event.utf8.as_ref().filter(|typed| !typed.chars().any(char::is_control)) {
                    text.push_str(typed);
                }
            }
        }
        win.needs_draw = true;
        true
    }

    fn key(&mut self, at: usize, event: &KeyEvent) {
        let ctrl = self.mods.ctrl;
        let shift = self.mods.shift;
        let alt = self.mods.alt;

        if self.rename_key(at, event) {
            return;
        }

        if ctrl && shift && !alt {
            match event.keysym {
                Keysym::c | Keysym::C => {
                    self.copy(at);
                    return;
                }
                Keysym::v | Keysym::V => {
                    self.paste(at);
                    return;
                }
                Keysym::t | Keysym::T => {
                    self.new_tab(at);
                    return;
                }
                Keysym::n | Keysym::N => {
                    self.new_window();
                    return;
                }
                Keysym::w | Keysym::W => {
                    if let Some(fd) = self.wins[at].active_fd() {
                        self.close_fd(fd);
                    }
                    return;
                }
                _ => {}
            }
        }

        if shift && !ctrl && !alt {
            match event.keysym {
                Keysym::Page_Up | Keysym::Page_Down => {
                    let win = &mut self.wins[at];
                    if let Some(pane) = win.panes.get_mut(win.active) {
                        let lines = pane.vt.rows.max(2) as isize - 1;
                        pane.vt.scroll(if event.keysym == Keysym::Page_Up { -lines } else { lines });
                        win.needs_draw = true;
                    }
                    return;
                }
                _ => {}
            }
        }

        if ctrl && !alt {
            match event.keysym {
                Keysym::t | Keysym::T => {
                    self.new_tab(at);
                    return;
                }
                Keysym::n | Keysym::N => {
                    self.new_window();
                    return;
                }
                Keysym::w | Keysym::W => {
                    if let Some(fd) = self.wins[at].active_fd() {
                        self.close_fd(fd);
                    }
                    return;
                }
                Keysym::Tab | Keysym::ISO_Left_Tab => {
                    self.switch(at, !shift);
                    return;
                }
                Keysym::equal | Keysym::plus | Keysym::KP_Add => {
                    self.resize_font(Some(1.0));
                    return;
                }
                Keysym::minus | Keysym::underscore | Keysym::KP_Subtract => {
                    self.resize_font(Some(-1.0));
                    return;
                }
                Keysym::_0 | Keysym::KP_0 => {
                    self.resize_font(None);
                    return;
                }
                _ => {}
            }
        }

        if self.wins[at].menu.is_some() && event.keysym == Keysym::Escape {
            let win = &mut self.wins[at];
            win.menu = None;
            Glide::aim(&mut win.lit, None, 1.0);
            win.needs_draw = true;
            return;
        }

        let mut out: Vec<u8> = Vec::new();
        match event.keysym {
            Keysym::Return | Keysym::KP_Enter => out.extend_from_slice(b"\r"),
            Keysym::BackSpace => out.push(0x7f),
            Keysym::Tab => out.push(b'\t'),
            Keysym::ISO_Left_Tab => out.extend_from_slice(b"\x1b[Z"),
            Keysym::Escape => out.push(0x1b),
            Keysym::Up => out.extend_from_slice(b"\x1b[A"),
            Keysym::Down => out.extend_from_slice(b"\x1b[B"),
            Keysym::Right => out.extend_from_slice(b"\x1b[C"),
            Keysym::Left => out.extend_from_slice(b"\x1b[D"),
            Keysym::Home => out.extend_from_slice(b"\x1b[H"),
            Keysym::End => out.extend_from_slice(b"\x1b[F"),
            Keysym::Page_Up => out.extend_from_slice(b"\x1b[5~"),
            Keysym::Page_Down => out.extend_from_slice(b"\x1b[6~"),
            Keysym::Delete => out.extend_from_slice(b"\x1b[3~"),
            _ => {
                if let Some(text) = &event.utf8 {
                    if ctrl && text.len() == 1 {
                        let c = text.as_bytes()[0];
                        let masked = match c {
                            b'a'..=b'z' => c - b'a' + 1,
                            b'A'..=b'Z' => c - b'A' + 1,
                            b' ' | b'@' => 0,
                            b'[' => 0x1b,
                            b'\\' => 0x1c,
                            b']' => 0x1d,
                            _ => c,
                        };
                        out.push(masked);
                    } else {
                        if alt {
                            out.push(0x1b);
                        }
                        out.extend_from_slice(text.as_bytes());
                    }
                }
            }
        }
        if !out.is_empty() {
            self.to_pty(at, &out);
        }
    }

    fn copy(&mut self, at: usize) {
        let win = &self.wins[at];
        let Some(text) = win.panes.get(win.active).and_then(|pane| pane.sel.map(|sel| sel.text(&pane.lines))) else {
            return;
        };
        if text.is_empty() {
            return;
        }
        let (Some(data), Some(device)) = (self.data.as_ref(), self.device.as_ref()) else {
            return;
        };
        let source = data.create_copy_paste_source(&self.qh, TEXT_KINDS);
        source.set_selection(device, self.serial);
        self.clip = Some((source, text));
    }

    fn paste(&mut self, at: usize) {
        let Some(fd) = self.wins[at].active_fd() else {
            return;
        };
        if let Some((_, text)) = self.clip.as_ref() {
            let text = text.clone();
            self.paste_into(fd, &text);
            return;
        }
        let Some(offer) = self.device.as_ref().and_then(|device| device.data().selection_offer()) else {
            return;
        };
        let Some(kind) = offer.with_mime_types(|kinds| TEXT_KINDS.iter().find(|kind| kinds.iter().any(|each| each == *kind)).map(|kind| kind.to_string())) else {
            return;
        };
        let Ok((reader, writer)) = std::io::pipe() else {
            return;
        };
        smithay_client_toolkit::data_device_manager::data_offer::receive_to_fd(offer.inner(), kind, writer.into());
        let pasted = self.pasted.clone();
        std::thread::spawn(move || {
            use std::io::Read;
            let mut bytes = Vec::new();
            if reader.take(PASTE_MOST).read_to_end(&mut bytes).is_ok() {
                let _ = pasted.send((fd, String::from_utf8_lossy(&bytes).into_owned()));
            }
        });
    }

    fn paste_into(&mut self, fd: RawFd, text: &str) {
        let Some((w, p)) = self.find_fd(fd) else {
            return;
        };
        let bracketed = self.wins[w].panes[p].vt.mode(2004);
        let bytes = paste_bytes(text, bracketed);
        let win = &mut self.wins[w];
        let pane = &mut win.panes[p];
        if pane.vt.scrolled_back() {
            pane.vt.scroll_bottom();
        }
        pane.clear_selection();
        win.needs_draw = true;
        pty::write_all(fd, &bytes);
    }

    fn open_menu(&mut self, at: usize) {
        let screen = Vec2::new(f64::from(self.wins[at].size.0), f64::from(self.wins[at].size.1));
        let tab = match self.tabs_shown(at) {
            true => ui::tab_at(self.cfg.tab_edge, self.wins[at].panes.len(), self.wins[at].size, self.pointer_pos),
            false => None,
        };
        let menu = match tab {
            Some(i) => {
                let win = &self.wins[at];
                ui::tab_menu(self.menu_look.clone(), self.pointer_pos, screen, &self.ui_font, win.panes[i].fd, win.panes.len() > 1)
            }
            None => {
                let win = &self.wins[at];
                let copy = win.panes.get(win.active).is_some_and(|pane| pane.sel.is_some_and(|sel| !sel.empty()));
                ui::context_menu(self.menu_look.clone(), self.pointer_pos, screen, &self.ui_font, copy)
            }
        };
        self.wins[at].menu = Some(menu);
        self.wins[at].needs_draw = true;
    }

    fn apply(&mut self, at: usize, action: Action) {
        match action {
            Action::NewTab => self.new_tab(at),
            Action::NewWindow => self.new_window(),
            Action::CloseTab => {
                if let Some(fd) = self.wins[at].active_fd() {
                    self.close_fd(fd);
                }
            }
            Action::Copy => self.copy(at),
            Action::Paste => self.paste(at),
            Action::Settings => self.open_settings(),
            Action::Rename(fd) => self.start_rename(at, fd),
            Action::MoveOut(fd) => {
                let group = self.group_of(at, fd);
                self.detach(at, &group);
            }
            Action::CloseOne(fd) => self.close_fd(fd),
        }
        if let Some(win) = self.wins.get_mut(at) {
            win.needs_draw = true;
        }
    }

    fn start_rename(&mut self, at: usize, fd: RawFd) {
        let win = &mut self.wins[at];
        let Some(p) = win.at(fd) else {
            return;
        };
        win.rename = Some((fd, win.panes[p].label().to_string()));
        win.active = p;
        win.needs_draw = true;
    }

    fn group_of(&self, at: usize, fd: RawFd) -> Vec<RawFd> {
        let win = &self.wins[at];
        match win.picked.contains(&fd) && win.picked.len() > 1 {
            true => win.panes.iter().map(|pane| pane.fd).filter(|each| win.picked.contains(each)).collect(),
            false => vec![fd],
        }
    }

    fn detach(&mut self, at: usize, group: &[RawFd]) {
        let win = &mut self.wins[at];
        let moving: Vec<usize> = group.iter().filter_map(|fd| win.at(*fd)).collect();
        if moving.is_empty() || moving.len() >= win.panes.len() {
            return;
        }
        let kept = win.active_fd().filter(|fd| !group.contains(fd));
        let taken = tabs::take(&mut win.panes, &moving);
        win.picked.clear();
        win.rename = None;
        win.active = kept.and_then(|fd| win.at(fd)).unwrap_or(0).min(win.panes.len() - 1);
        win.needs_draw = true;
        let one = win.panes.len() == 1;
        if one {
            self.fit_panes(at, false);
        }
        self.open_window(taken);
    }

    fn resize_font(&mut self, by: Option<f64>) {
        self.zoom = match by {
            Some(by) => (self.zoom + by).clamp(6.0 - self.cfg.font_px, 48.0 - self.cfg.font_px),
            None => 0.0,
        };
        let mut sized = self.cfg.clone();
        sized.font_px += self.zoom;
        if let Ok(grid) = Grid::new(&sized) {
            self.grid = grid;
            for at in 0..self.wins.len() {
                self.fit_panes(at, true);
            }
        }
    }

    fn hover_rect(&self, at: usize) -> Option<Rect> {
        let win = &self.wins[at];
        if let Some(menu) = &win.menu {
            if menu.rect.contains(self.pointer_pos) {
                let inside = self.pointer_pos - menu.rect.origin;
                let index = menu.look.item_at(&menu.items, menu.size, inside)?;
                menu.actions.get(index).copied().flatten()?;
                let row = menu.look.lit_rect(&menu.items, menu.size, index);
                return Some(Rect::new(menu.rect.origin + row.origin, row.size));
            }
            return None;
        }
        if win.hold.is_some() {
            return None;
        }
        if self.tabs_shown(at) {
            let rects = ui::tab_rects(self.cfg.tab_edge, win.panes.len(), win.size);
            return rects.into_iter().find(|rect| rect.contains(self.pointer_pos));
        }
        None
    }

    fn pointer_move(&mut self, at: usize) {
        match self.wins[at].hold.as_mut() {
            Some(Hold::Tab(hold)) => {
                if !hold.moving && (self.pointer_pos - hold.from).length() > DRAG_START {
                    hold.moving = true;
                }
            }
            Some(Hold::Text) => self.extend_selection(at),
            None => {}
        }
        let target = self.hover_rect(at);
        let win = &mut self.wins[at];
        Glide::aim(&mut win.lit, target, 1.0);
        win.needs_draw = true;
    }

    fn extend_selection(&mut self, at: usize) {
        let card = self.content(at);
        let edge = match self.pointer_pos.y {
            y if y < card.origin.y => -1,
            y if y > card.max().y => 1,
            _ => 0,
        };
        let win = &mut self.wins[at];
        let Some(pane) = win.panes.get_mut(win.active) else {
            return;
        };
        if edge != 0 {
            pane.vt.scroll(edge);
        }
        let top = pane.vt.scrollbar().offset;
        let head = self.grid.spot(card, top, pane.vt.cols, pane.vt.rows, self.pointer_pos);
        if let Some(sel) = pane.sel.as_mut() {
            sel.head = head;
        }
        win.needs_draw = true;
    }

    fn press_text(&mut self, at: usize) {
        let card = self.content(at);
        let win = &mut self.wins[at];
        let count = match win.text_click {
            Some((when, count)) if when.elapsed() < DOUBLE_CLICK => count % 3 + 1,
            _ => 1,
        };
        win.text_click = Some((Instant::now(), count));
        let Some(pane) = win.panes.get_mut(win.active) else {
            return;
        };
        let top = pane.vt.scrollbar().offset;
        let spot = self.grid.spot(card, top, pane.vt.cols, pane.vt.rows, self.pointer_pos);
        let unit = match count {
            1 => Unit::Cell,
            2 => Unit::Word,
            _ => Unit::Line,
        };
        if unit == Unit::Cell {
            pane.lines.clear();
        }
        pane.sel = Some(Sel::at(spot, unit));
        win.hold = Some(Hold::Text);
        win.needs_draw = true;
    }

    fn press_tab(&mut self, at: usize, i: usize) {
        let shift = self.mods.shift;
        let ctrl = self.mods.ctrl;
        let win = &mut self.wins[at];
        let fd = win.panes[i].fd;
        win.rename = win.rename.take().filter(|(held, _)| *held == fd);
        let double = win.tab_click.take().is_some_and(|(held, when)| held == fd && when.elapsed() < DOUBLE_CLICK);
        if double && !shift && !ctrl {
            self.start_rename(at, fd);
            return;
        }
        win.tab_click = Some((fd, Instant::now()));
        if shift {
            let from = win.active;
            win.picked = tabs::span(from, i).into_iter().map(|p| win.panes[p].fd).collect();
            win.needs_draw = true;
            return;
        }
        if ctrl {
            if win.picked.is_empty() {
                if let Some(now) = win.active_fd() {
                    win.picked.push(now);
                }
            }
            tabs::toggle(&mut win.picked, fd);
            win.needs_draw = true;
            return;
        }
        if !win.picked.contains(&fd) {
            win.picked.clear();
        }
        win.active = i;
        let group = match win.picked.contains(&fd) && win.picked.len() > 1 {
            true => win.panes.iter().map(|pane| pane.fd).filter(|each| win.picked.contains(each)).collect(),
            false => vec![fd],
        };
        win.hold = Some(Hold::Tab(TabHold { fd, from: self.pointer_pos, moving: false, group }));
        win.needs_draw = true;
    }

    fn release(&mut self, at: usize) {
        let Some(hold) = self.wins[at].hold.take() else {
            return;
        };
        self.wins[at].needs_draw = true;
        match hold {
            Hold::Text => {
                let win = &mut self.wins[at];
                if let Some(pane) = win.panes.get_mut(win.active)
                    && pane.sel.is_some_and(|sel| sel.empty())
                {
                    pane.clear_selection();
                }
            }
            Hold::Tab(hold) if !hold.moving => {
                let win = &mut self.wins[at];
                if win.picked.len() > 1 && win.picked.contains(&hold.fd) {
                    win.picked.clear();
                }
                win.show_fd(hold.fd);
            }
            Hold::Tab(hold) => {
                let size = self.wins[at].size;
                let outside = self.pointer_pos.x < 0.0
                    || self.pointer_pos.y < 0.0
                    || self.pointer_pos.x > f64::from(size.0)
                    || self.pointer_pos.y > f64::from(size.1);
                if outside {
                    self.detach(at, &hold.group);
                    return;
                }
                if ui::strip_rect(self.cfg.tab_edge, size).contains(self.pointer_pos) {
                    let edge = self.cfg.tab_edge;
                    let win = &mut self.wins[at];
                    let along = if edge.vertical() { self.pointer_pos.y } else { self.pointer_pos.x };
                    let slot = tabs::slot(&ui::tab_centers(edge, win.panes.len(), size), along);
                    let active = win.active_fd();
                    let moving: Vec<usize> = hold.group.iter().filter_map(|fd| win.at(*fd)).collect();
                    tabs::reorder(&mut win.panes, &moving, slot);
                    if let Some(fd) = active {
                        win.show_fd(fd);
                    }
                }
            }
        }
    }

    fn click(&mut self, at: usize, button: u32) {
        if button == BTN_LEFT {
            Glide::press(&mut self.wins[at].lit);
        }
        if let Some(menu) = self.wins[at].menu.take() {
            if menu.rect.contains(self.pointer_pos) && button == BTN_LEFT {
                let inside = self.pointer_pos - menu.rect.origin;
                if let Some(index) = menu.look.item_at(&menu.items, menu.size, inside)
                    && let Some(action) = menu.actions.get(index).copied().flatten()
                {
                    self.apply(at, action);
                }
            }
            if let Some(win) = self.wins.get_mut(at) {
                Glide::aim(&mut win.lit, None, 1.0);
                win.needs_draw = true;
            }
            return;
        }
        let tabs = self.tabs_shown(at);
        let strip = tabs && ui::strip_rect(self.cfg.tab_edge, self.wins[at].size).contains(self.pointer_pos);
        let tab = match tabs {
            true => ui::tab_at(self.cfg.tab_edge, self.wins[at].panes.len(), self.wins[at].size, self.pointer_pos),
            false => None,
        };
        match button {
            BTN_RIGHT => self.open_menu(at),
            BTN_LEFT => match tab {
                Some(i) => {
                    self.wins[at].blank_click = None;
                    self.press_tab(at, i);
                }
                None if strip => {
                    let win = &mut self.wins[at];
                    win.rename = None;
                    match win.blank_click.take() {
                        Some(first) if first.elapsed() < DOUBLE_CLICK => self.new_tab(at),
                        _ => win.blank_click = Some(Instant::now()),
                    }
                }
                None => {
                    self.wins[at].rename = None;
                    self.press_text(at);
                }
            },
            BTN_MIDDLE if !strip => self.paste(at),
            _ => {}
        }
    }

    fn wheel(&mut self, at: usize, discrete: i32, absolute: f64, wheel: bool) {
        let lines = match (discrete, wheel) {
            (0, true) => return,
            (0, false) => {
                self.wheel += absolute;
                let ch = f64::from(self.grid.cell().1.max(1));
                let whole = (self.wheel / ch).trunc();
                self.wheel -= whole * ch;
                whole as isize
            }
            (steps, _) => steps as isize * WHEEL_LINES,
        };
        if lines == 0 {
            return;
        }
        let card = self.content(at);
        let win = &mut self.wins[at];
        let Some(pane) = win.panes.get_mut(win.active) else {
            return;
        };
        if pane.vt.mouse_tracking() {
            let spot = self.grid.spot(card, 0, pane.vt.cols, pane.vt.rows, self.pointer_pos);
            let button = if lines < 0 { 64 } else { 65 };
            let one = match pane.vt.mode(1006) {
                true => format!("\x1b[<{button};{};{}M", spot.col + 1, spot.row + 1),
                false => {
                    let code = |n: u32| char::from_u32(32 + n).unwrap_or(' ');
                    format!("\x1b[M{}{}{}", code(button), code(u32::from(spot.col) + 1), code(spot.row as u32 + 1))
                }
            };
            for _ in 0..lines.unsigned_abs().div_ceil(WHEEL_LINES as usize) {
                pty::write_all(pane.fd, one.as_bytes());
            }
            return;
        }
        if pane.vt.alternate() {
            let key: &[u8] = match (lines < 0, pane.vt.mode(1)) {
                (true, true) => b"\x1bOA",
                (true, false) => b"\x1b[A",
                (false, true) => b"\x1bOB",
                (false, false) => b"\x1b[B",
            };
            for _ in 0..lines.unsigned_abs() {
                pty::write_all(pane.fd, key);
            }
            return;
        }
        pane.vt.scroll(lines);
        win.needs_draw = true;
    }

    fn draw(&mut self, at: usize) {
        let tabs = self.tabs_shown(at);
        let card = self.content(at);
        let edge = self.cfg.tab_edge;
        let frame = if tabs { self.bar } else { self.grid.bg() };
        let pointer = self.pointer_pos;
        let win = &mut self.wins[at];
        if win.panes.is_empty() {
            return;
        }
        let pane = &mut win.panes[win.active];
        let snap = pane.vt.snapshot();
        let picked = match pane.sel {
            Some(sel) if !sel.empty() => {
                for (y, row) in snap.grid.iter().enumerate() {
                    pane.lines.insert(snap.top + y as u64, row.iter().map(|cell| cell.text.clone()).collect());
                }
                Some(sel.bounds(&pane.lines))
            }
            _ => None,
        };
        let image = self.grid.base_image(win.size, frame);
        let mut canvas = Canvas::on(image);
        if tabs {
            canvas.round_rect(card, ui::CARD_RADIUS, self.grid.bg(), self.grid.opacity());
        }
        canvas.clip_to(Some(card));
        self.grid.paint_cells(&mut canvas, &snap, card, picked);
        canvas.clip_to(None);

        if tabs {
            let rects = ui::tab_rects(edge, win.panes.len(), win.size);
            let dragging: Vec<RawFd> = match &win.hold {
                Some(Hold::Tab(hold)) if hold.moving => hold.group.clone(),
                _ => Vec::new(),
            };
            for (i, rect) in rects.iter().enumerate() {
                let pane = &win.panes[i];
                let active = i == win.active;
                let picked = win.picked.contains(&pane.fd) && win.picked.len() > 1;
                let fade = if dragging.contains(&pane.fd) { 0.45 } else { 1.0 };
                if active {
                    canvas.round_rect(*rect, 7.0, self.accent, fade);
                } else if picked {
                    canvas.round_rect(*rect, 7.0, self.accent, 0.35 * fade);
                }
                let color = if active { self.on_accent } else { self.dim };
                let editing = win.rename.as_ref().filter(|(fd, _)| *fd == pane.fd).map(|(_, text)| text.clone());
                let label = match &editing {
                    Some(text) => text.clone(),
                    None => tab_label(i, pane.label()),
                };
                let room = rect.size.x - 16.0;
                let fit = match editing {
                    Some(_) => fit_tail(&self.ui_font, &label, room - 4.0),
                    None => fit_label(&self.ui_font, &label, room),
                };
                let glyph = self.ui_font.paint(&fit, color);
                let ty = rect.origin.y + (rect.size.y - self.ui_font.line_height()) / 2.0;
                canvas.stamp(&glyph, Vec2::new(rect.origin.x + 8.0, ty), fade);
                if win.rename.as_ref().is_some_and(|(fd, _)| *fd == pane.fd) {
                    let x = rect.origin.x + 9.0 + self.ui_font.width_of(&fit);
                    canvas.line(Vec2::new(x, ty + 1.0), Vec2::new(x, ty + self.ui_font.line_height() - 1.0), 1.5, color, 1.0);
                }
            }
            if !dragging.is_empty() && ui::strip_rect(edge, win.size).contains(pointer) {
                let along = if edge.vertical() { pointer.y } else { pointer.x };
                let slot = tabs::slot(&ui::tab_centers(edge, win.panes.len(), win.size), along);
                let mark = match (rects.get(slot), rects.last()) {
                    (Some(rect), _) if edge.vertical() => Rect::new(Vec2::new(rect.origin.x, rect.origin.y - 3.0), Vec2::new(rect.size.x, 2.0)),
                    (Some(rect), _) => Rect::new(Vec2::new(rect.origin.x - 3.0, rect.origin.y), Vec2::new(2.0, rect.size.y)),
                    (None, Some(rect)) if edge.vertical() => Rect::new(Vec2::new(rect.origin.x, rect.max().y + 1.0), Vec2::new(rect.size.x, 2.0)),
                    (None, Some(rect)) => Rect::new(Vec2::new(rect.max().x + 1.0, rect.origin.y), Vec2::new(2.0, rect.size.y)),
                    (None, None) => Rect::new(pointer, Vec2::ZERO),
                };
                canvas.round_rect(mark, 1.0, self.accent, 1.0);
            }
            if let Some(lit) = &win.lit {
                lit.draw(&mut canvas, self.cfg.palette.shade);
            }
        }

        if let Some(menu) = &win.menu {
            canvas.stamp(&menu.art, menu.rect.origin, 1.0);
            if let Some(lit) = &win.lit {
                lit.draw(&mut canvas, self.cfg.palette.shade);
            }
        }

        if let Some((old, since)) = &win.fade {
            let t = since.elapsed().as_secs_f64() / RECOLOR.as_secs_f64();
            if t < 1.0 && old.width == canvas.image.width && old.height == canvas.image.height {
                canvas.stamp(old, Vec2::ZERO, 1.0 - drifter_core::motion::smooth(t));
            } else {
                win.fade = None;
            }
        }
        let image = canvas.image;
        self.show(at, &image);
        self.wins[at].shown = Some(image);
    }

    fn show(&mut self, at: usize, image: &Image) {
        let (width, height) = (image.width, image.height);
        let stride = width as i32 * 4;
        let (buffer, canvas) = match self.pool.create_buffer(width as i32, height as i32, stride, wl_shm::Format::Argb8888) {
            Ok(made) => made,
            Err(error) => {
                log!("drifter-terminal: no buffer: {error}");
                return;
            }
        };
        let pixels = image.rgba.as_chunks::<4>().0;
        for (to, from) in canvas.as_chunks_mut::<4>().0.iter_mut().zip(pixels) {
            *to = [from[2], from[1], from[0], from[3]];
        }
        let window = &self.wins[at].window;
        let surface = window.wl_surface();
        surface.set_buffer_scale(1);
        surface.damage_buffer(0, 0, width as i32, height as i32);
        if let Err(error) = buffer.attach_to(surface) {
            log!("drifter-terminal: {error}");
            return;
        }
        window.commit();
    }
}

fn tab_label(i: usize, title: &str) -> String {
    format!("{}  {}", i + 1, title)
}

fn fit_label(font: &Font, label: &str, max: f64) -> String {
    if max <= 0.0 || font.width_of(label) <= max {
        return label.to_string();
    }
    let mut out = label.to_string();
    while !out.is_empty() && font.width_of(&format!("{out}\u{2026}")) > max {
        out.pop();
    }
    format!("{out}\u{2026}")
}

fn fit_tail(font: &Font, label: &str, max: f64) -> String {
    let mut out: String = label.to_string();
    while !out.is_empty() && font.width_of(&out) > max {
        out.remove(0);
    }
    out
}

impl CompositorHandler for Term {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {}
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for Term {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl WindowHandler for Term {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window) {
        if let Some(at) = self.win_of(window.wl_surface()) {
            self.close_window(at);
        }
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window, configure: WindowConfigure, _: u32) {
        let Some(at) = self.win_of(window.wl_surface()) else {
            return;
        };
        let win = &mut self.wins[at];
        let width = configure.new_size.0.map(|v| v.get()).unwrap_or(win.size.0);
        let height = configure.new_size.1.map(|v| v.get()).unwrap_or(win.size.1);
        win.size = (width.max(320), height.max(200));
        win.configured = true;
        win.resized = true;
        win.needs_draw = true;
    }
}

impl SeatHandler for Term {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            match self.seats.get_keyboard(qh, &seat, None) {
                Ok(keyboard) => self.keyboard = Some(keyboard),
                Err(error) => log!("drifter-terminal: no keyboard: {error}"),
            }
            if self.device.is_none() {
                self.device = self.data.as_ref().map(|data| data.get_data_device(qh, &seat));
            }
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            match self.seats.get_pointer(qh, &seat) {
                Ok(pointer) => self.pointer = Some(pointer),
                Err(error) => log!("drifter-terminal: no pointer: {error}"),
            }
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Keyboard
            && let Some(keyboard) = self.keyboard.take()
        {
            keyboard.release();
        }
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            pointer.release();
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Term {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, surface: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {
        self.focus = Some(surface.clone());
    }
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, surface: &wl_surface::WlSurface, _: u32) {
        if self.focus.as_ref() == Some(surface) {
            self.focus = None;
        }
    }
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, serial: u32, event: KeyEvent) {
        self.serial = serial;
        if let Some(at) = self.focused() {
            self.key(at, &event);
        }
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, modifiers: Modifiers, _: u32) {
        self.mods = modifiers;
    }
}

impl PointerHandler for Term {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for event in events {
            let Some(at) = self.win_of(&event.surface) else {
                continue;
            };
            self.pointer_pos = Vec2::new(event.position.0, event.position.1);
            match event.kind {
                PointerEventKind::Motion { .. } | PointerEventKind::Enter { .. } => self.pointer_move(at),
                PointerEventKind::Leave { .. } => {
                    let win = &mut self.wins[at];
                    Glide::aim(&mut win.lit, None, 1.0);
                    win.needs_draw = true;
                }
                PointerEventKind::Press { button, serial, .. } => {
                    self.serial = serial;
                    self.click(at, button);
                }
                PointerEventKind::Release { button, .. } if button == BTN_LEFT => {
                    if at < self.wins.len() {
                        self.release(at);
                    }
                }
                PointerEventKind::Axis { vertical, source, .. } => {
                    let wheel = source.is_none_or(|source| source == wl_pointer::AxisSource::Wheel);
                    self.wheel(at, vertical.discrete, vertical.absolute, wheel);
                }
                _ => {}
            }
        }
    }
}

impl DataDeviceHandler for Term {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64, _: &wl_surface::WlSurface) {}
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64) {}
    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
}

impl DataOfferHandler for Term {
    fn source_actions(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
    fn selected_action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
}

impl DataSourceHandler for Term {
    fn accept_mime(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: Option<String>) {}
    fn send_request(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource, _: String, fd: WritePipe) {
        let Some(text) = self.clip.as_ref().filter(|(ours, _)| ours.inner() == source).map(|(_, text)| text.clone()) else {
            return;
        };
        let mut file = std::fs::File::from(std::os::fd::OwnedFd::from(fd));
        std::thread::spawn(move || {
            use std::io::Write;
            let _ = file.write_all(text.as_bytes());
        });
    }
    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        if self.clip.as_ref().is_some_and(|(ours, _)| ours.inner() == source) {
            self.clip = None;
        } else {
            source.destroy();
        }
    }
    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}
    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

impl ShmHandler for Term {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Term {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(Term);
delegate_output!(Term);
delegate_shm!(Term);
delegate_seat!(Term);
delegate_keyboard!(Term);
delegate_pointer!(Term);
delegate_data_device!(Term);
delegate_xdg_shell!(Term);
delegate_xdg_window!(Term);
delegate_registry!(Term);

#[cfg(test)]
mod tests {
    use super::paste_bytes;

    #[test]
    fn a_paste_is_framed_only_when_the_program_asks() {
        assert_eq!(paste_bytes("a\nb", false), b"a\rb".to_vec());
        assert_eq!(paste_bytes("a\nb", true), b"\x1b[200~a\nb\x1b[201~".to_vec());
        assert_eq!(paste_bytes("x\x1b[201~y", true), b"\x1b[200~xy\x1b[201~".to_vec());
    }
}
