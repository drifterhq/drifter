use std::os::fd::{AsFd, BorrowedFd, RawFd};
use std::time::Duration;

use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
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
    delegate_compositor, delegate_keyboard, delegate_output, delegate_pointer, delegate_registry, delegate_seat, delegate_shm,
    delegate_xdg_shell, delegate_xdg_window, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

use drifter_core::menu::Look;
use drifter_core::paint::Canvas;
use drifter_core::{Font, Image, Rect, Vec2};

use crate::config::Config;
use crate::render::Grid;
use crate::ui::{self, Action, Lit, Menu};
use crate::vt::Vt;
use crate::pty;

const START: (u32, u32) = (960, 600);
const APP_ID: &str = "drifter-terminal";

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
}

struct Term {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    window: Window,
    qh: QueueHandle<Term>,
    handle: LoopHandle<'static, Term>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    mods: Modifiers,
    size: (u32, u32),
    exit: bool,
    configured: bool,
    resized: bool,
    needs_draw: bool,
    cfg: Config,
    grid: Grid,
    panes: Vec<Pane>,
    active: usize,
    closed: u32,
    ui_font: Font,
    menu_look: Look,
    accent: [u8; 3],
    on_accent: [u8; 3],
    bar: [u8; 3],
    dim: [u8; 3],
    pointer_pos: Vec2,
    lit: Option<Lit>,
    menu: Option<Menu>,
}

fn mix(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    [0, 1, 2].map(|i| (f64::from(a[i]) + (f64::from(b[i]) - f64::from(a[i])) * t).round() as u8)
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

pub fn run() -> Result<(), String> {
    let cfg = Config::load();
    let grid = Grid::new(&cfg)?;
    let ui_font = crate::font::ui(13.0)?;
    let menu_look = Look::themed(&drifter_core::theme::DARK);
    let accent = menu_look.accent;
    let on_accent = menu_look.on_accent;
    let bar = mix(cfg.bg, [255, 255, 255], 0.06);
    let dim = mix(cfg.fg, cfg.bg, 0.35);

    let connection = Connection::connect_to_env().map_err(|e| format!("no wayland: {e}"))?;
    let (globals, queue) = registry_queue_init(&connection).map_err(|e| format!("no wayland registry: {e}"))?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(|e| format!("no wl_compositor: {e}"))?;
    let shell = XdgShell::bind(&globals, &qh).map_err(|e| format!("no xdg shell: {e}"))?;
    let shm = Shm::bind(&globals, &qh).map_err(|e| format!("no wl_shm: {e}"))?;

    let surface = compositor.create_surface(&qh);
    let window = shell.create_window(surface, WindowDecorations::RequestServer, &qh);
    window.set_title("Terminal");
    window.set_app_id(APP_ID);
    window.set_min_size(Some((320, 200)));
    window.commit();

    let pool = SlotPool::new((START.0 * START.1 * 4) as usize, &shm).map_err(|e| format!("no shared memory: {e}"))?;
    let mut events: EventLoop<'static, Term> = EventLoop::try_new().map_err(|e| format!("no event loop: {e}"))?;
    WaylandSource::new(connection, queue)
        .insert(events.handle())
        .map_err(|e| format!("cannot watch wayland: {e}"))?;
    let handle = events.handle();

    let content = ui::content_rect(cfg.tab_edge, START, false);
    let (cols, rows) = grid.fit(content.size.x, content.size.y);
    let p = pty::spawn(cols, rows)?;
    let vt = Vt::new(cols, rows)?;
    let token = pty_source(&handle, p.fd)?;
    let first = Pane { vt, fd: p.fd, child: p.child, token, title: "Terminal".into() };

    let mut term = Term {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        shm,
        pool,
        window,
        qh: qh.clone(),
        handle,
        keyboard: None,
        pointer: None,
        mods: Modifiers::default(),
        size: START,
        exit: false,
        configured: false,
        resized: false,
        needs_draw: true,
        cfg,
        grid,
        panes: vec![first],
        active: 0,
        closed: 0,
        ui_font,
        menu_look,
        accent,
        on_accent,
        bar,
        dim,
        pointer_pos: Vec2::ZERO,
        lit: None,
        menu: None,
    };

    while !term.exit {
        let busy = term.lit.as_ref().is_some_and(Lit::busy);
        let wait = if busy { Duration::from_millis(16) } else { Duration::from_millis(200) };
        events.dispatch(Some(wait), &mut term).map_err(|e| format!("event loop: {e}"))?;
        if term.configured {
            if std::mem::take(&mut term.resized) {
                term.resize_panes();
            }
            let busy = term.lit.as_ref().is_some_and(Lit::busy);
            if std::mem::take(&mut term.needs_draw) || busy {
                term.draw();
            }
        }
    }
    Ok(())
}

impl Term {
    fn tabs_shown(&self) -> bool {
        self.panes.len() > 1 || self.cfg.tab_always
    }

    fn content(&self) -> Rect {
        ui::content_rect(self.cfg.tab_edge, self.size, self.tabs_shown())
    }

    fn content_fit(&self) -> (u16, u16) {
        let c = self.content();
        self.grid.fit(c.size.x, c.size.y)
    }

    fn resize_panes(&mut self) {
        let (cols, rows) = self.content_fit();
        let (cw, ch) = self.grid.cell();
        for p in &mut self.panes {
            if (cols, rows) != (p.vt.cols, p.vt.rows) {
                pty::resize(p.fd, cols, rows);
                p.vt.resize(cols, rows, cw, ch);
            }
        }
        self.needs_draw = true;
    }

    fn make_pane(&self) -> Result<Pane, String> {
        let (cols, rows) = self.content_fit();
        let p = pty::spawn(cols, rows)?;
        let vt = Vt::new(cols, rows)?;
        let token = pty_source(&self.handle, p.fd)?;
        Ok(Pane { vt, fd: p.fd, child: p.child, token, title: "Terminal".into() })
    }

    fn new_tab(&mut self) {
        let one_before = self.panes.len() == 1;
        match self.make_pane() {
            Ok(pane) => {
                self.panes.push(pane);
                self.active = self.panes.len() - 1;
                if one_before {
                    self.resize_panes();
                }
                self.needs_draw = true;
            }
            Err(error) => log!("drifter-terminal: {error}"),
        }
    }

    fn new_window(&self) {
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::process::Command::new(exe).spawn();
        }
    }

    fn close_tab(&mut self) {
        if !self.panes.is_empty() {
            let fd = self.panes[self.active].fd;
            self.close_fd(fd);
        }
    }

    fn close_fd(&mut self, fd: RawFd) {
        let Some(i) = self.panes.iter().position(|p| p.fd == fd) else {
            return;
        };
        let pane = self.panes.remove(i);
        self.handle.remove(pane.token);
        unsafe {
            libc::close(pane.fd);
            let mut status = 0;
            libc::waitpid(pane.child, &mut status, libc::WNOHANG);
        }
        self.closed = self.closed.saturating_add(1);
        if self.panes.is_empty() {
            self.exit = true;
            return;
        }
        if self.active >= self.panes.len() {
            self.active = self.panes.len() - 1;
        } else if self.active > i {
            self.active -= 1;
        }
        if self.panes.len() == 1 {
            self.resize_panes();
        }
        self.needs_draw = true;
    }

    fn switch(&mut self, forward: bool) {
        if self.panes.len() < 2 {
            return;
        }
        self.active = if forward {
            (self.active + 1) % self.panes.len()
        } else {
            (self.active + self.panes.len() - 1) % self.panes.len()
        };
        self.needs_draw = true;
    }

    fn drain_fd(&mut self, fd: RawFd) {
        let mut buf = [0u8; 16384];
        let mut closed = false;
        loop {
            let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len()) };
            if n > 0 {
                if let Some(pane) = self.panes.iter_mut().find(|p| p.fd == fd) {
                    pane.vt.feed(&buf[..n as usize]);
                }
                self.needs_draw = true;
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

    fn to_pty(&self, bytes: &[u8]) {
        if let Some(pane) = self.panes.get(self.active) {
            pty::write_all(pane.fd, bytes);
        }
    }

    fn key(&mut self, event: &KeyEvent) {
        let ctrl = self.mods.ctrl;
        let shift = self.mods.shift;
        let alt = self.mods.alt;

        if ctrl && !alt {
            match event.keysym {
                Keysym::t | Keysym::T if shift => {
                    self.new_tab();
                    return;
                }
                Keysym::t | Keysym::T => {
                    self.new_tab();
                    return;
                }
                Keysym::n | Keysym::N => {
                    self.new_window();
                    return;
                }
                Keysym::w | Keysym::W => {
                    self.close_tab();
                    return;
                }
                Keysym::Tab | Keysym::ISO_Left_Tab => {
                    self.switch(!shift);
                    return;
                }
                _ => {}
            }
        }

        if self.menu.is_some() && event.keysym == Keysym::Escape {
            self.menu = None;
            Lit::aim(&mut self.lit, None);
            self.needs_draw = true;
            return;
        }

        let mut out: Vec<u8> = Vec::new();
        match event.keysym {
            Keysym::Return | Keysym::KP_Enter => out.extend_from_slice(b"\r"),
            Keysym::BackSpace => out.push(0x7f),
            Keysym::Tab => out.push(b'\t'),
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
            self.to_pty(&out);
        }
    }

    fn open_menu(&mut self) {
        let menu = ui::context_menu(
            self.menu_look.clone(),
            self.pointer_pos,
            Vec2::new(f64::from(self.size.0), f64::from(self.size.1)),
            &self.ui_font,
            self.cfg.tab_edge,
        );
        self.menu = Some(menu);
        self.needs_draw = true;
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::NewTab => self.new_tab(),
            Action::NewWindow => self.new_window(),
            Action::CloseTab => self.close_tab(),
            Action::Copy => {}
            Action::Paste => {}
            Action::Tabs(edge) => {
                self.cfg.tab_edge = edge;
                self.resize_panes();
            }
            Action::FontUp => self.resize_font(1.0),
            Action::FontDown => self.resize_font(-1.0),
            Action::SelectTab(i) => {
                if i < self.panes.len() {
                    self.active = i;
                }
            }
        }
        self.needs_draw = true;
    }

    fn resize_font(&mut self, by: f64) {
        self.cfg.font_px = (self.cfg.font_px + by).clamp(8.0, 40.0);
        if let Ok(grid) = Grid::new(&self.cfg) {
            self.grid = grid;
            self.resize_panes();
        }
    }

    fn pointer_move(&mut self) {
        let target = self.hover_rect();
        Lit::aim(&mut self.lit, target);
        self.needs_draw = true;
    }

    fn hover_rect(&self) -> Option<Rect> {
        if let Some(menu) = &self.menu {
            if menu.rect.contains(self.pointer_pos) {
                let inside = self.pointer_pos - menu.rect.origin;
                let at = menu.look.item_at(&menu.items, menu.size, inside)?;
                menu.actions.get(at).copied().flatten()?;
                let row = menu.look.lit_rect(&menu.items, menu.size, at);
                return Some(Rect::new(menu.rect.origin + row.origin, row.size));
            }
            return None;
        }
        if self.tabs_shown() {
            let rects = ui::tab_rects(self.cfg.tab_edge, self.panes.len(), self.size);
            return rects.into_iter().find(|rect| rect.contains(self.pointer_pos));
        }
        None
    }

    fn click(&mut self, button: u32) {
        if let Some(menu) = self.menu.take() {
            if menu.rect.contains(self.pointer_pos) && button == BTN_LEFT {
                let inside = self.pointer_pos - menu.rect.origin;
                if let Some(at) = menu.look.item_at(&menu.items, menu.size, inside)
                    && let Some(action) = menu.actions.get(at).copied().flatten()
                {
                    self.apply(action);
                }
            }
            Lit::aim(&mut self.lit, None);
            self.needs_draw = true;
            return;
        }
        match button {
            BTN_RIGHT => self.open_menu(),
            BTN_LEFT if self.tabs_shown() => {
                if let Some(i) = ui::tab_at(self.cfg.tab_edge, self.panes.len(), self.size, self.pointer_pos) {
                    self.active = i;
                    self.needs_draw = true;
                }
            }
            BTN_MIDDLE => {}
            _ => {}
        }
    }

    fn draw(&mut self) {
        let snap = self.panes[self.active].vt.snapshot();
        let content = self.content();
        let image = self.grid.base_image(self.size);
        let mut canvas = Canvas::on(image);
        self.grid.paint_cells(&mut canvas, &snap, content);

        if self.tabs_shown() {
            let strip = ui::strip_rect(self.cfg.tab_edge, self.size);
            canvas.round_rect(strip, 0.0, self.bar, 1.0);
            let rects = ui::tab_rects(self.cfg.tab_edge, self.panes.len(), self.size);
            for (i, rect) in rects.iter().enumerate() {
                let active = i == self.active;
                if active {
                    canvas.round_rect(*rect, 7.0, self.accent, 1.0);
                }
                let color = if active { self.on_accent } else { self.dim };
                let label = tab_label(i, &self.panes[i].title);
                let fit = fit_label(&self.ui_font, &label, rect.size.x - 16.0);
                let glyph = self.ui_font.paint(&fit, color);
                let ty = rect.origin.y + (rect.size.y - self.ui_font.line_height()) / 2.0;
                canvas.stamp(&glyph, Vec2::new(rect.origin.x + 8.0, ty), 1.0);
            }
            if let Some(lit) = &self.lit {
                lit.draw(&mut canvas);
            }
        }

        if let Some(menu) = &self.menu {
            canvas.stamp(&menu.art, menu.rect.origin, 1.0);
            if let Some(lit) = &self.lit {
                lit.draw(&mut canvas);
            }
        }

        let image = canvas.image;
        self.show(&image);
    }

    fn show(&mut self, image: &Image) {
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
        let surface = self.window.wl_surface();
        surface.set_buffer_scale(1);
        surface.damage_buffer(0, 0, width as i32, height as i32);
        if let Err(error) = buffer.attach_to(surface) {
            log!("drifter-terminal: {error}");
            return;
        }
        self.window.commit();
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
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        self.exit = true;
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window, configure: WindowConfigure, _: u32) {
        let width = configure.new_size.0.map(|v| v.get()).unwrap_or(self.size.0);
        let height = configure.new_size.1.map(|v| v.get()).unwrap_or(self.size.1);
        self.size = (width.max(320), height.max(200));
        self.configured = true;
        self.resized = true;
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

impl PointerHandler for Term {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for event in events {
            self.pointer_pos = Vec2::new(event.position.0, event.position.1);
            match event.kind {
                PointerEventKind::Motion { .. } | PointerEventKind::Enter { .. } => self.pointer_move(),
                PointerEventKind::Leave { .. } => {
                    Lit::aim(&mut self.lit, None);
                    self.needs_draw = true;
                }
                PointerEventKind::Press { button, .. } => self.click(button),
                _ => {}
            }
        }
    }
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
delegate_xdg_shell!(Term);
delegate_xdg_window!(Term);
delegate_registry!(Term);
