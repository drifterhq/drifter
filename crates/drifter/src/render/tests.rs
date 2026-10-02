use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use drifter_core::{Config, Image};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::xdg::XdgShell;
use smithay_client_toolkit::shell::xdg::window::{
    Window, WindowConfigure, WindowDecorations, WindowHandler,
};
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_output, delegate_registry, delegate_shm, delegate_xdg_shell,
    delegate_xdg_window, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

use super::{RescaleRenderElement, Screen, Shown, State};

const SIZE: (u32, u32) = (800, 600);
const PAINT: (u32, u32) = (200, 100);
const RED: [u8; 4] = [255, 0, 0, 255];

struct Client {
    registry: RegistryState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    window: Window,
    painted: bool,
}

impl Client {
    fn paint(&mut self) {
        let (width, height) = PAINT;
        let Ok((buffer, canvas)) = self.pool.create_buffer(
            width as i32,
            height as i32,
            width as i32 * 4,
            wl_shm::Format::Argb8888,
        ) else {
            return;
        };
        for pixel in canvas.as_chunks_mut::<4>().0 {
            *pixel = [0, 0, 255, 255];
        }
        let surface = self.window.wl_surface();
        surface.damage_buffer(0, 0, width as i32, height as i32);
        if buffer.attach_to(surface).is_ok() {
            self.window.commit();
            self.painted = true;
        }
    }
}

impl CompositorHandler for Client {
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

impl OutputHandler for Client {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl WindowHandler for Client {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {}
    fn configure(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &Window,
        _: WindowConfigure,
        _: u32,
    ) {
        self.paint();
    }
}

impl ShmHandler for Client {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Client {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState];
}

delegate_compositor!(Client);
delegate_output!(Client);
delegate_shm!(Client);
delegate_xdg_shell!(Client);
delegate_xdg_window!(Client);
delegate_registry!(Client);

fn run_client(stop: Arc<AtomicBool>) {
    let connection = Connection::connect_to_env().expect("the test client connects");
    let (globals, mut queue) = registry_queue_init(&connection).expect("a registry");
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).expect("wl_compositor");
    let shell = XdgShell::bind(&globals, &qh).expect("xdg shell");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let surface = compositor.create_surface(&qh);
    let window = shell.create_window(surface, WindowDecorations::RequestServer, &qh);
    window.set_title("red");
    window.set_app_id("drifter-test-red");
    window.commit();
    let pool = SlotPool::new((PAINT.0 * PAINT.1 * 4) as usize, &shm).expect("shared memory");
    let mut client = Client {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        shm,
        pool,
        window,
        painted: false,
    };
    while !stop.load(Ordering::Relaxed) {
        let _ = connection.flush();
        if let Some(guard) = connection.prepare_read() {
            let _ = guard.read();
        }
        let _ = queue.dispatch_pending(&mut client);
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn snapshot(state: &mut State) -> Image {
    let scale = state.heads[state.at].scale;
    let shown: Vec<Shown> = state
        .elements()
        .into_iter()
        .map(|piece| RescaleRenderElement::from_element(piece, (0, 0).into(), scale))
        .collect();
    let rgba = state.capture(&shown, false).expect("a frame");
    Image {
        width: SIZE.0,
        height: SIZE.1,
        rgba,
    }
}

fn red_box(image: &Image) -> Option<(u32, u32, u32, u32)> {
    let mut found: Option<(u32, u32, u32, u32)> = None;
    for y in 0..image.height {
        for x in 0..image.width {
            let at = ((y * image.width + x) * 4) as usize;
            if image.rgba[at..at + 4] != RED {
                continue;
            }
            found = Some(match found {
                None => (x, y, x, y),
                Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
            });
        }
    }
    found.map(|(x0, y0, x1, y1)| (x0, y0, x1 - x0 + 1, y1 - y0 + 1))
}

fn pump(screen: &mut Screen, until: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !until(&screen.state) && Instant::now() < deadline {
        screen
            .events
            .dispatch(Some(Duration::from_millis(10)), &mut screen.state)
            .expect("the event loop runs");
    }
}

#[test]
fn a_window_is_drawn_at_its_size_and_halves_at_half_zoom() {
    let runtime = std::env::temp_dir().join(format!("drifter-headless-{}", std::process::id()));
    std::fs::create_dir_all(&runtime).unwrap();
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", &runtime) };

    let config = Config::default();
    let mut screen =
        super::open(None, &config, SIZE, [0.0, 0.0, 0.2, 1.0], None).expect("a headless screen");
    assert_eq!(screen.found.renderer, "pixman");
    unsafe { std::env::set_var("WAYLAND_DISPLAY", &screen.found.socket) };

    let stop = Arc::new(AtomicBool::new(false));
    let client = {
        let stop = stop.clone();
        std::thread::spawn(move || run_client(stop))
    };

    pump(&mut screen, |state| {
        state
            .windows
            .iter()
            .any(|window| window.app == "drifter-test-red")
    });
    let started = Instant::now();
    pump(&mut screen, |_| {
        started.elapsed() > Duration::from_millis(500)
    });

    assert!(
        screen.state.heads[0].frames > 1,
        "the offscreen head draws frames of its own"
    );
    let near = snapshot(&mut screen.state);
    let _ = drifter_core::image::save_png(&near, &runtime.join("zoom-1.png"));
    let (x, y, width, height) = red_box(&near).expect("the red window is on screen");
    assert_eq!((width, height), PAINT, "the window is drawn 1:1 at zoom 1");
    let middle = (x + width / 2, y + height / 2);
    assert!(
        middle.0.abs_diff(SIZE.0 / 2) <= 1,
        "centered across: {middle:?}"
    );

    let mut camera = screen.state.camera;
    camera.zoom = 0.5;
    screen.state.camera = camera;
    screen.state.flight = None;
    let far = snapshot(&mut screen.state);
    let _ = drifter_core::image::save_png(&far, &runtime.join("zoom-half.png"));
    let (_, _, width, height) = red_box(&far).expect("the red window is still on screen");
    assert!(
        width.abs_diff(PAINT.0 / 2) <= 1 && height.abs_diff(PAINT.1 / 2) <= 1,
        "half zoom halves the window: {width} x {height}"
    );

    stop.store(true, Ordering::Relaxed);
    let _ = client.join();
    println!("frames in {}", runtime.display());
}
