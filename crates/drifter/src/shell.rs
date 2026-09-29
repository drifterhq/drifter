use std::os::unix::io::OwnedFd;
use std::sync::Arc;

use drifter_core::{Rect, Vec2};
use smithay::desktop::PopupKind;
use smithay::input::{Seat, SeatHandler, SeatState};
use smithay::reexports::wayland_server::backend::{ClientData, ClientId, DisconnectReason};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_seat::WlSeat;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::reexports::wayland_server::{Client, Display, DisplayHandle};
use smithay::utils::Serial;
use smithay::wayland::buffer::BufferHandler;
use smithay::wayland::compositor::{
    CompositorClientState, CompositorHandler, CompositorState, get_parent, is_sync_subsurface,
};
use smithay::wayland::selection::SelectionHandler;
use smithay::wayland::selection::data_device::{
    ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
};
use smithay::wayland::shell::xdg::{
    PopupSurface, PositionerState, ToplevelSurface, XdgShellHandler, XdgShellState,
};
use smithay::wayland::shm::{ShmHandler, ShmState};
use smithay::{
    delegate_compositor, delegate_data_device, delegate_output, delegate_seat, delegate_shm,
    delegate_xdg_decoration, delegate_xdg_shell,
};
use wayland_protocols::xdg::decoration::zv1::server::zxdg_toplevel_decoration_v1::Mode as DecorationMode;

use crate::render::State;

pub struct Wayland {
    pub handle: DisplayHandle,
    pub compositor: CompositorState,
    pub xdg: XdgShellState,
    pub shm: ShmState,
    pub seats: SeatState<State>,
    pub data_device: DataDeviceState,
    pub _seat: Seat<State>,
    pub keyboard: smithay::input::keyboard::KeyboardHandle<State>,
    pub pointer: smithay::input::pointer::PointerHandle<State>,
}

pub struct Window {
    pub id: u64,
    pub toplevel: ToplevelSurface,
    pub content: Rect,
    pub inset: Vec2,
    pub placed: bool,
    pub touched: bool,
    pub born: std::time::Instant,
    pub hidden: bool,
    pub decorated: bool,
    pub app: String,
    pub title: String,
    pub restore: Option<Rect>,
    pub motion: Option<drifter_core::motion::Motion>,
    pub going: bool,
}

#[derive(Default)]
pub struct Greeting {
    pub compositor_state: CompositorClientState,
}

impl ClientData for Greeting {
    fn initialized(&self, _id: ClientId) {}
    fn disconnected(&self, _id: ClientId, _reason: DisconnectReason) {}
}

impl BufferHandler for State {
    fn buffer_destroyed(&mut self, _buffer: &WlBuffer) {}
}

impl CompositorHandler for State {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.wayland.compositor
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<Greeting>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        use smithay::backend::renderer::utils::on_commit_buffer_handler;
        on_commit_buffer_handler::<Self>(surface);

        self.popups.commit(surface);
        if is_sync_subsurface(surface) {
            return;
        }
        let mut root = surface.clone();
        while let Some(parent) = get_parent(&root) {
            root = parent;
        }
        self.settle_window(&root);
        self.kick_now();
    }
}

impl ShmHandler for State {
    fn shm_state(&self) -> &ShmState {
        &self.wayland.shm
    }
}

impl SeatHandler for State {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.wayland.seats
    }

    fn focus_changed(&mut self, seat: &Seat<Self>, focused: Option<&WlSurface>) {
        use smithay::reexports::wayland_server::Resource;
        use smithay::wayland::selection::data_device::set_data_device_focus;

        let client = focused.and_then(|surface| self.wayland.handle.get_client(surface.id()).ok());
        set_data_device_focus(&self.wayland.handle, seat, client);
    }

    fn cursor_image(
        &mut self,
        _seat: &Seat<Self>,
        _image: smithay::input::pointer::CursorImageStatus,
    ) {
    }
}

impl SelectionHandler for State {
    type SelectionUserData = ();
}

impl DataDeviceHandler for State {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.wayland.data_device
    }
}

impl ClientDndGrabHandler for State {}
impl ServerDndGrabHandler for State {
    fn send(&mut self, _mime: String, _fd: OwnedFd, _seat: Seat<Self>) {}
}

impl smithay::wayland::output::OutputHandler for State {}

impl XdgShellHandler for State {
    fn xdg_shell_state(&mut self) -> &mut XdgShellState {
        &mut self.wayland.xdg
    }

    fn new_toplevel(&mut self, surface: ToplevelSurface) {
        surface.send_configure();
        let start = self.opening_motion();
        let id = self.next_window_id();
        self.windows.push(Window {
            id,
            toplevel: surface,
            content: Rect::new(Vec2::ZERO, Vec2::ZERO),
            inset: Vec2::ZERO,
            placed: false,
            touched: false,
            born: std::time::Instant::now(),
            hidden: false,
            decorated: true,
            app: String::new(),
            title: String::new(),
            restore: None,
            motion: Some(start),
            going: false,
        });
    }

    fn toplevel_destroyed(&mut self, surface: ToplevelSurface) {
        self.window_gone(&surface);
        self.kick_now();
    }

    fn move_request(&mut self, surface: ToplevelSurface, _seat: WlSeat, _serial: Serial) {
        if let Some(index) = self
            .windows
            .iter()
            .position(|window| window.toplevel == surface)
        {
            self.start_move(index);
        }
    }

    fn resize_request(
        &mut self,
        surface: ToplevelSurface,
        _seat: WlSeat,
        _serial: Serial,
        edges: smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge,
    ) {
        if let Some(index) = self
            .windows
            .iter()
            .position(|window| window.toplevel == surface)
        {
            self.start_resize(index, edges);
        }
    }

    fn new_popup(&mut self, surface: PopupSurface, positioner: PositionerState) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        if let Err(error) = self.popups.track_popup(PopupKind::Xdg(surface.clone())) {
            eprintln!("drifter: popup lost its parent: {error}");
            return;
        }
        if let Err(error) = surface.send_configure() {
            eprintln!("drifter: popup refused: {error}");
            return;
        }
        self.kick_now();
    }

    fn popup_destroyed(&mut self, _surface: PopupSurface) {
        self.popups.cleanup();
        self.kick_now();
    }

    fn grab(&mut self, surface: PopupSurface, seat: WlSeat, serial: Serial) {
        use smithay::desktop::{
            PopupKeyboardGrab, PopupPointerGrab, PopupUngrabStrategy, find_popup_root_surface,
        };
        use smithay::input::pointer::Focus;

        if self.locked() {
            surface.send_popup_done();
            return;
        }
        let Some(seat) = Seat::<State>::from_resource(&seat) else {
            return;
        };
        let kind = PopupKind::Xdg(surface);
        let Ok(root) = find_popup_root_surface(&kind) else {
            return;
        };
        let Ok(mut grab) = self.popups.grab_popup(root, kind, &seat, serial) else {
            return;
        };

        if let Some(keyboard) = seat.get_keyboard() {
            let earlier = grab.previous_serial().unwrap_or(serial);
            if keyboard.is_grabbed() && !(keyboard.has_grab(serial) || keyboard.has_grab(earlier)) {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            keyboard.set_focus(self, grab.current_grab(), serial);
            keyboard.set_grab(self, PopupKeyboardGrab::new(&grab), serial);
        }
        if let Some(pointer) = seat.get_pointer() {
            let earlier = grab.previous_serial().unwrap_or_else(|| grab.serial());
            if pointer.is_grabbed() && !(pointer.has_grab(serial) || pointer.has_grab(earlier)) {
                grab.ungrab(PopupUngrabStrategy::All);
                return;
            }
            pointer.set_grab(self, PopupPointerGrab::new(&grab), serial, Focus::Keep);
        }
    }

    fn reposition_request(
        &mut self,
        surface: PopupSurface,
        positioner: PositionerState,
        token: u32,
    ) {
        surface.with_pending_state(|state| {
            state.geometry = positioner.get_geometry();
            state.positioner = positioner;
        });
        surface.send_repositioned(token);
        self.kick_now();
    }
}

impl State {
    fn set_frame(&mut self, toplevel: &ToplevelSurface, mode: DecorationMode) {
        toplevel.with_pending_state(|state| {
            state.decoration_mode = Some(mode);
        });
        toplevel.send_configure();
        if let Some(window) = self
            .windows
            .iter_mut()
            .find(|window| &window.toplevel == toplevel)
        {
            window.decorated = mode == DecorationMode::ServerSide;
        }
        self.kick_now();
    }
}

impl smithay::wayland::shell::xdg::decoration::XdgDecorationHandler for State {
    fn new_decoration(&mut self, toplevel: ToplevelSurface) {
        self.set_frame(&toplevel, DecorationMode::ServerSide);
    }

    fn request_mode(&mut self, toplevel: ToplevelSurface, mode: DecorationMode) {
        self.set_frame(&toplevel, mode);
    }

    fn unset_mode(&mut self, toplevel: ToplevelSurface) {
        self.set_frame(&toplevel, DecorationMode::ServerSide);
    }
}

delegate_compositor!(State);
delegate_shm!(State);
delegate_seat!(State);
delegate_data_device!(State);
delegate_output!(State);
delegate_xdg_shell!(State);
delegate_xdg_decoration!(State);

pub fn start(display: &Display<State>) -> Result<Wayland, String> {
    let handle = display.handle();
    let compositor = CompositorState::new::<State>(&handle);
    let xdg = XdgShellState::new::<State>(&handle);
    let shm = ShmState::new::<State>(&handle, Vec::new());
    let mut seats = SeatState::new();
    let mut seat = seats.new_wl_seat(&handle, "drifter");
    let keyboard = seat
        .add_keyboard(Default::default(), 200, 25)
        .map_err(|e| format!("cannot make a keyboard: {e}"))?;
    let pointer = seat.add_pointer();
    let data_device = DataDeviceState::new::<State>(&handle);
    smithay::wayland::shell::xdg::decoration::XdgDecorationState::new::<State>(&handle);

    Ok(Wayland {
        handle,
        compositor,
        xdg,
        shm,
        seats,
        data_device,
        _seat: seat,
        keyboard,
        pointer,
    })
}

pub fn skin_names() -> Vec<String> {
    use drifter_core::window::{Button, Look, Piece};
    let mut names = Vec::new();
    for piece in Piece::ALL {
        names.push(piece.file_name(true));
        names.push(piece.file_name(false));
    }
    for button in Button::ALL {
        for look in Look::ALL {
            names.push(button.file_name(look));
        }
    }
    names
}

pub fn send_frame(surface: &WlSurface, time: u32) {
    use smithay::wayland::compositor::{
        SurfaceAttributes, TraversalAction, with_surface_tree_downward,
    };
    with_surface_tree_downward(
        surface,
        (),
        |_, _, &()| TraversalAction::DoChildren(()),
        |_, states, &()| {
            for callback in states
                .cached_state
                .get::<SurfaceAttributes>()
                .current()
                .frame_callbacks
                .drain(..)
            {
                callback.done(time);
            }
        },
        |_, _, &()| true,
    );
}

pub fn client_data() -> Arc<Greeting> {
    Arc::new(Greeting::default())
}
