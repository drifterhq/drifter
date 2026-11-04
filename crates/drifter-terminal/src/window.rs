use std::os::fd::{AsFd, BorrowedFd, RawFd};
use std::time::Duration;

use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::generic::Generic;
use smithay_client_toolkit::reexports::calloop::{EventLoop, Interest, Mode, PostAction};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, Keysym, KeyboardHandler, Modifiers};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::XdgShell;
use smithay_client_toolkit::shell::xdg::window::{Window, WindowConfigure, WindowDecorations, WindowHandler};
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_keyboard, delegate_output, delegate_registry, delegate_seat, delegate_shm, delegate_xdg_shell,
    delegate_xdg_window, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

use drifter_core::Image;

use crate::config::Config;
use crate::render::Grid;
use crate::vt::Vt;
use crate::{log, pty};

const START: (u32, u32) = (900, 560);
const APP_ID: &str = "drifter-terminal";

struct Fd(RawFd);
impl AsFd for Fd {
    fn as_fd(&self) -> BorrowedFd<'_> {
        unsafe { BorrowedFd::borrow_raw(self.0) }
    }
}

struct Term {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    window: Window,
    qh: QueueHandle<Term>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    mods: Modifiers,
    size: (u32, u32),
    exit: bool,
    configured: bool,
    resized: bool,
    needs_draw: bool,
    grid: Grid,
    vt: Vt,
    pty_fd: RawFd,
}

pub fn run() -> Result<(), String> {
    let config = Config::load();
    let grid = Grid::new(config.font_px, config.pad)?;
    let (cols, rows) = grid.fit(START.0, START.1);

    let p = pty::spawn(cols, rows)?;
    let vt = Vt::new(cols, rows)?;

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
    window.set_min_size(Some((240, 160)));
    window.commit();

    let pool = SlotPool::new((START.0 * START.1 * 4) as usize, &shm).map_err(|e| format!("no shared memory: {e}"))?;
    let mut events: EventLoop<'static, Term> = EventLoop::try_new().map_err(|e| format!("no event loop: {e}"))?;
    WaylandSource::new(connection, queue)
        .insert(events.handle())
        .map_err(|e| format!("cannot watch wayland: {e}"))?;

    let fd_source = Generic::new(Fd(p.fd), Interest::READ, Mode::Level);
    events
        .handle()
        .insert_source(fd_source, |_, _, term: &mut Term| {
            term.drain_pty();
            Ok(PostAction::Continue)
        })
        .map_err(|e| format!("cannot watch the shell: {e}"))?;

    let mut term = Term {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs: OutputState::new(&globals, &qh),
        shm,
        pool,
        window,
        qh: qh.clone(),
        keyboard: None,
        mods: Modifiers::default(),
        size: START,
        exit: false,
        configured: false,
        resized: false,
        needs_draw: true,
        grid,
        vt,
        pty_fd: p.fd,
    };

    while !term.exit {
        events.dispatch(Some(Duration::from_millis(200)), &mut term).map_err(|e| format!("event loop: {e}"))?;
        if term.configured {
            if std::mem::take(&mut term.resized) {
                term.apply_size();
            }
            if std::mem::take(&mut term.needs_draw) {
                term.draw();
            }
        }
    }
    Ok(())
}

impl Term {
    fn drain_pty(&mut self) {
        let mut buf = [0u8; 16384];
        loop {
            let n = unsafe { libc::read(self.pty_fd, buf.as_mut_ptr() as *mut _, buf.len()) };
            if n > 0 {
                self.vt.feed(&buf[..n as usize]);
                self.needs_draw = true;
            } else if n == 0 {
                self.exit = true;
                break;
            } else {
                let err = std::io::Error::last_os_error();
                match err.raw_os_error() {
                    Some(libc::EAGAIN) => break,
                    Some(libc::EINTR) => continue,
                    _ => {
                        self.exit = true;
                        break;
                    }
                }
            }
        }
    }

    fn apply_size(&mut self) {
        let (cols, rows) = self.grid.fit(self.size.0, self.size.1);
        if (cols, rows) != (self.vt.cols, self.vt.rows) {
            pty::resize(self.pty_fd, cols, rows);
            self.vt.resize(cols, rows, self.grid.cell_w, self.grid.cell_h);
        }
        self.needs_draw = true;
    }

    fn draw(&mut self) {
        let snap = self.vt.snapshot();
        let image = self.grid.paint(&snap, self.size.0, self.size.1);
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

    fn key(&mut self, event: &KeyEvent) {
        let ctrl = self.mods.ctrl;
        let alt = self.mods.alt;
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
            pty::write_all(self.pty_fd, &out);
        }
    }
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
        self.size = (width.max(240), height.max(160));
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
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Keyboard
            && let Some(keyboard) = self.keyboard.take()
        {
            keyboard.release();
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
delegate_xdg_shell!(Term);
delegate_xdg_window!(Term);
delegate_registry!(Term);

use wayland_client::protocol::wl_output;
