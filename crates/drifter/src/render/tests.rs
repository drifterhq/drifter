use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use drifter_core::camera::Camera;
use drifter_core::drawer::Edge;
use drifter_core::{Config, Image, Rect, Vec2, WallpaperMode};
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
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_compositor;
use wayland_client::protocol::{wl_output, wl_shm, wl_surface};
use wayland_client::protocol::{wl_registry, wl_seat};
use wayland_client::{Connection, QueueHandle};
use wayland_client::{Dispatch, Proxy, delegate_noop, event_created_child};
use wayland_protocols::ext::foreign_toplevel_list::v1::client::{
    ext_foreign_toplevel_handle_v1::{self, ExtForeignToplevelHandleV1},
    ext_foreign_toplevel_list_v1::{self, ExtForeignToplevelListV1},
};
use wayland_protocols::ext::session_lock::v1::client::{
    ext_session_lock_manager_v1::ExtSessionLockManagerV1,
    ext_session_lock_surface_v1::{self, ExtSessionLockSurfaceV1},
    ext_session_lock_v1::{self, ExtSessionLockV1},
};
use wayland_protocols::wp::keyboard_shortcuts_inhibit::zv1::client::{
    zwp_keyboard_shortcuts_inhibit_manager_v1::ZwpKeyboardShortcutsInhibitManagerV1,
    zwp_keyboard_shortcuts_inhibitor_v1::ZwpKeyboardShortcutsInhibitorV1,
};

use super::{Grab, RescaleRenderElement, Screen, Shown, State};

const SIZE: (u32, u32) = (800, 600);
const PAINT: (u32, u32) = (200, 100);
const RED: [u8; 4] = [255, 0, 0, 255];
const BACKGROUND: [f32; 4] = [0.0, 0.0, 0.2, 1.0];
const BLOCK: u32 = 16;

struct Client {
    registry: RegistryState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    window: Window,
    painted: bool,
    _inhibitor: Option<ZwpKeyboardShortcutsInhibitorV1>,
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
delegate_noop!(Client: ignore wl_seat::WlSeat);
delegate_noop!(Client: ZwpKeyboardShortcutsInhibitManagerV1);
delegate_noop!(Client: ignore ZwpKeyboardShortcutsInhibitorV1);

fn run_client_with(stop: Arc<AtomicBool>, inhibit: bool) {
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
    let inhibitor = inhibit.then(|| {
        let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=1, ()).expect("wl_seat");
        let manager: ZwpKeyboardShortcutsInhibitManagerV1 = globals
            .bind(&qh, 1..=1, ())
            .expect("the shortcut inhibit manager");
        manager.inhibit_shortcuts(window.wl_surface(), &seat, &qh, ())
    });
    let pool = SlotPool::new((PAINT.0 * PAINT.1 * 4) as usize, &shm).expect("shared memory");
    let mut client = Client {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        shm,
        pool,
        window,
        painted: false,
        _inhibitor: inhibitor,
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

static HEADLESS: Mutex<()> = Mutex::new(());

struct Run {
    screen: Screen,
    folder: PathBuf,
    _serial: MutexGuard<'static, ()>,
}

impl Run {
    fn shot(&mut self, name: &str) -> Image {
        let image = snapshot(&mut self.screen.state);
        let _ = drifter_core::image::save_png(&image, &self.folder.join(format!("{name}.png")));
        image
    }
}

fn start(name: &str, config: &Config, wallpaper: Option<Image>) -> Run {
    let serial = HEADLESS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let folder =
        std::env::temp_dir().join(format!("drifter-headless-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    unsafe { std::env::set_var("XDG_RUNTIME_DIR", &folder) };
    let screen = super::open(None, config, SIZE, BACKGROUND, wallpaper).expect("a headless screen");
    assert_eq!(screen.found.renderer, "pixman");
    unsafe { std::env::set_var("WAYLAND_DISPLAY", &screen.found.socket) };
    Run {
        screen,
        folder,
        _serial: serial,
    }
}

fn red_window(run: &mut Run) -> (Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    red_window_with(run, false)
}

fn red_window_with(run: &mut Run, inhibit: bool) -> (Arc<AtomicBool>, std::thread::JoinHandle<()>) {
    let stop = Arc::new(AtomicBool::new(false));
    let client = {
        let stop = stop.clone();
        std::thread::spawn(move || run_client_with(stop, inhibit))
    };
    pump(&mut run.screen, |state| {
        state
            .windows
            .iter()
            .any(|window| window.app == "drifter-test-red" && window.placed)
    });
    let started = Instant::now();
    pump(&mut run.screen, |_| {
        started.elapsed() > Duration::from_millis(500)
    });
    (stop, client)
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

fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * image.width + x) * 4) as usize;
    [
        image.rgba[at],
        image.rgba[at + 1],
        image.rgba[at + 2],
        image.rgba[at + 3],
    ]
}

fn is_background(color: [u8; 4]) -> bool {
    color[0] <= 1 && color[1] <= 1 && color[2].abs_diff(51) <= 1
}

fn alike(a: [u8; 4], b: [u8; 4]) -> bool {
    a.iter().zip(b.iter()).all(|(a, b)| a.abs_diff(*b) <= 12)
}

fn red_box(image: &Image) -> Option<(u32, u32, u32, u32)> {
    let mut found: Option<(u32, u32, u32, u32)> = None;
    for y in 0..image.height {
        for x in 0..image.width {
            if pixel(image, x, y) != RED {
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

fn pattern(width: u32, height: u32) -> Image {
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let block = (x / BLOCK).wrapping_mul(73_856_093) ^ (y / BLOCK).wrapping_mul(19_349_663);
            let mix = block.wrapping_mul(2_654_435_761);
            let part = |shift: u32| 64 + ((mix >> shift) % 192) as u8;
            rgba.extend_from_slice(&[part(8), part(16), part(24), 255]);
        }
    }
    Image {
        width,
        height,
        rgba,
    }
}

fn matching(before: &Image, after: &Image, shift: u32) -> f64 {
    let mut same = 0u32;
    let mut seen = 0u32;
    for y in (0..SIZE.1).step_by(3) {
        for x in (0..SIZE.0 - shift).step_by(3) {
            seen += 1;
            if alike(pixel(after, x, y), pixel(before, x + shift, y)) {
                same += 1;
            }
        }
    }
    f64::from(same) / f64::from(seen.max(1))
}

fn best_shift(before: &Image, after: &Image, most: u32) -> (u32, f64) {
    (0..=most)
        .map(|shift| (shift, matching(before, after, shift)))
        .fold((0, 0.0), |best, next| match next.1 > best.1 {
            true => next,
            false => best,
        })
}

fn background_count(image: &Image) -> usize {
    (0..SIZE.1)
        .step_by(2)
        .flat_map(|y| (0..SIZE.0).step_by(2).map(move |x| (x, y)))
        .filter(|&(x, y)| is_background(pixel(image, x, y)))
        .count()
}

fn look_at(state: &mut State, camera: Camera) {
    state.camera = camera;
    state.flight = None;
}

#[test]
fn a_window_is_drawn_at_its_size_and_halves_at_half_zoom() {
    let mut run = start("zoom", &Config::default(), None);
    let (stop, client) = red_window(&mut run);

    assert!(
        run.screen.state.heads[0].frames > 1,
        "the offscreen head draws frames of its own"
    );
    let near = run.shot("zoom-1");
    let (x, y, width, height) = red_box(&near).expect("the red window is on screen");
    assert_eq!((width, height), PAINT, "the window is drawn 1:1 at zoom 1");
    let middle = (x + width / 2, y + height / 2);
    assert!(
        middle.0.abs_diff(SIZE.0 / 2) <= 1,
        "centered across: {middle:?}"
    );

    let mut camera = run.screen.state.camera;
    camera.zoom = 0.5;
    look_at(&mut run.screen.state, camera);
    let far = run.shot("zoom-half");
    let (_, _, width, height) = red_box(&far).expect("the red window is still on screen");
    assert!(
        width.abs_diff(PAINT.0 / 2) <= 1 && height.abs_diff(PAINT.1 / 2) <= 1,
        "half zoom halves the window: {width} x {height}"
    );

    stop.store(true, Ordering::Relaxed);
    let _ = client.join();
}

#[test]
fn a_window_wears_a_frame_that_hugs_it_at_every_zoom() {
    let mut run = start("chrome", &Config::default(), None);
    let (stop, client) = red_window(&mut run);
    let content = {
        let window = run
            .screen
            .state
            .windows
            .iter()
            .find(|window| window.app == "drifter-test-red")
            .expect("the red window is placed");
        assert!(window.decorated, "the client asked for a server frame");
        window.content
    };

    let mut wrong = Vec::new();
    for zoom in [1.0, 0.5] {
        let mut camera = run.screen.state.camera;
        camera.zoom = zoom;
        look_at(&mut run.screen.state, camera);
        let image = run.shot(&format!("chrome-{zoom}"));
        let state = &run.screen.state;
        let inner = state.on_screen(content);
        let frame = state.on_screen(state.chrome.layout(content).frame);
        let title = state.chrome.title * zoom;
        let edge = state.chrome.edge * zoom;
        let radius = state.chrome.radius * zoom;

        let (red_x, red_y, _, _) = red_box(&image).expect("the red window is on screen");
        if (f64::from(red_x) - inner.origin.x).abs() > 1.0
            || (f64::from(red_y) - inner.origin.y).abs() > 1.0
        {
            wrong.push(format!(
                "zoom {zoom}: red at {red_x},{red_y}, content at {inner:?}"
            ));
        }

        let mut want = |x: f64, y: f64, framed: bool, part: &str| {
            let (x, y) = (x.round() as u32, y.round() as u32);
            if is_background(pixel(&image, x, y)) == framed {
                let said = if framed { "bare" } else { "covered" };
                wrong.push(format!("zoom {zoom}: {part} at {x},{y} is {said}"));
            }
        };
        let across = (frame.origin.x + radius) as i64..(frame.max().x - radius) as i64;
        for x in across.step_by(5) {
            let x = x as f64;
            want(x, inner.origin.y - title / 2.0, true, "title bar");
            want(x, frame.origin.y - 3.0, false, "above the frame");
            want(x, inner.max().y + edge / 2.0, true, "bottom edge");
            want(x, frame.max().y + 2.0, false, "below the frame");
        }
        let down = inner.origin.y as i64 + 2..inner.max().y as i64 - 2;
        for y in down.step_by(5) {
            let y = y as f64;
            want(inner.origin.x - edge / 2.0, y, true, "left edge");
            want(frame.origin.x - 3.0, y, false, "left of the frame");
            want(inner.max().x + edge / 2.0, y, true, "right edge");
            want(frame.max().x + 2.0, y, false, "right of the frame");
        }
    }

    stop.store(true, Ordering::Relaxed);
    let _ = client.join();
    assert!(wrong.is_empty(), "the frame is off:\n{}", wrong.join("\n"));
}

#[test]
fn zooming_all_the_way_out_shows_the_whole_world_and_no_further() {
    let mut run = start("world-out", &Config::default(), Some(pattern(1600, 1200)));
    let state = &mut run.screen.state;
    let fit = Camera::fit_zoom(state.screen, state.world);
    state.zoom_by(-1000.0);
    state.zoom_by(-5.0);
    assert!(
        (state.camera.zoom - fit).abs() < 1e-9,
        "zoom stops at {fit}, not {}",
        state.camera.zoom
    );

    let image = run.shot("world-out");
    let state = &run.screen.state;
    let world = state.on_screen(Rect::new(Vec2::ZERO, state.world));
    let mut wrong = Vec::new();
    for y in [5, 100, 500, SIZE.1 - 5] {
        for x in 0..SIZE.0 {
            let at = f64::from(x);
            let inside = at > world.origin.x + 2.0 && at < world.max().x - 2.0;
            let outside = at < world.origin.x - 2.0 || at > world.max().x + 2.0;
            let bare = is_background(pixel(&image, x, y));
            if (inside && bare) || (outside && !bare) {
                wrong.push((x, y));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "the world spans {world:?}, but {} pixels disagree, first {:?}",
        wrong.len(),
        wrong.first()
    );
}

#[test]
fn the_camera_stops_at_the_world_edge() {
    let mut run = start("world-edge", &Config::default(), Some(pattern(1600, 1200)));
    let state = &mut run.screen.state;
    let started = state.camera;
    state.modifier = true;
    for _ in 0..400 {
        let to = state.pointer - Vec2::splat(40.0);
        state.pointer_to(to);
    }
    state.modifier = false;
    let seen = state.camera.visible_rect(state.screen);
    assert_ne!(state.camera, started, "the push moved the camera");
    assert!(
        seen.origin.x.abs() < 1e-6 && seen.origin.y.abs() < 1e-6,
        "an edge push stops at the world corner, not {:?}",
        seen.origin
    );
    let pushed = run.shot("world-pushed");
    assert_eq!(
        background_count(&pushed),
        0,
        "nothing past the world shows after a push"
    );

    let state = &mut run.screen.state;
    state.fly_to(Camera::new(Vec2::splat(1.0e6), Camera::WORK_ZOOM));
    let (flight, _) = state.flight.expect("a flight");
    let seen = flight.to.visible_rect(state.screen);
    assert!(
        (seen.max() - state.world).length() < 1e-6,
        "a flight lands at the far world corner, not {:?}",
        seen.max()
    );
    look_at(state, flight.to);
    let flown = run.shot("world-flown");
    assert_eq!(
        background_count(&flown),
        0,
        "nothing past the world shows after a flight"
    );
}

#[test]
fn each_wallpaper_mode_moves_with_the_camera_as_it_should() {
    let mut run = start("wallpaper", &Config::default(), Some(pattern(1600, 1200)));
    let home = run.screen.state.camera;
    let away = Camera::new(home.center + Vec2::new(200.0, 0.0), home.zoom);

    let mut shots = |mode: WallpaperMode| {
        run.screen.state.mode = mode;
        look_at(&mut run.screen.state, home);
        let before = run.shot(&format!("{mode:?}-home"));
        look_at(&mut run.screen.state, away);
        let after = run.shot(&format!("{mode:?}-away"));
        (before, after)
    };

    let (before, after) = shots(WallpaperMode::Fixed);
    assert!(
        matching(&before, &after, 0) > 0.99,
        "a fixed wallpaper stays put when the camera moves"
    );

    let (before, after) = shots(WallpaperMode::Pan);
    let (shift, score) = best_shift(&before, &after, 260);
    assert!(
        shift.abs_diff(200) <= 1 && score > 0.95,
        "a panned wallpaper moves 200 px with the camera, not {shift} ({score})"
    );
    assert!(
        matching(&before, &after, 0) < 0.5,
        "a panned wallpaper does not stay put"
    );

    let (before, after) = shots(WallpaperMode::Parallax);
    let state = &run.screen.state;
    let image = state
        .wallpaper
        .as_ref()
        .expect("a wallpaper")
        .pyramid
        .size();
    let place = |camera: &Camera| {
        drifter_core::wallpaper::place(
            WallpaperMode::Parallax,
            image,
            state.screen,
            state.world,
            camera,
        )
    };
    let (from, to) = (place(&home), place(&away));
    let expected =
        (to.source.origin.x - from.source.origin.x) * from.target.size.x / from.source.size.x;
    let (shift, score) = best_shift(&before, &after, 260);
    assert!(
        expected > 5.0 && expected < 195.0,
        "parallax moves slower than the camera: {expected}"
    );
    assert!(
        (f64::from(shift) - expected).abs() <= 1.5 && score > 0.95,
        "a parallax wallpaper moves {expected:.1} px, not {shift} ({score})"
    );
}

#[test]
fn drawers_slide_in_at_their_edge_and_leave_no_trace() {
    let mut run = start("drawers", &Config::default(), None);
    let closed = run.shot("drawers-closed");
    let middle = Vec2::new(f64::from(SIZE.0) / 2.0, f64::from(SIZE.1) / 2.0);
    let mut wrong = Vec::new();

    for edge in Edge::ALL {
        let index = edge.index();
        let rim = match edge {
            Edge::Top => 0.0,
            Edge::Bottom => f64::from(SIZE.1) - 1.0,
        };
        run.screen.state.pointer_to(Vec2::new(middle.x, rim));
        run.screen.state.tick();
        pump(&mut run.screen, |state| {
            state.drawers[index].open && state.drawers[index].settled()
        });
        assert!(
            run.screen.state.drawers[index].open,
            "{edge:?} drawer opens when the pointer rests on its edge"
        );
        let opened = run.shot(&format!("drawer-{edge:?}"));
        let state = &run.screen.state;
        let rect = state.drawers[index].rect(state.screen, &state.shapes[index]);
        let rim_row = (rim as u32, rim + 0.5);
        let flush = match edge {
            Edge::Top => rect.origin.y <= 0.0 && rect.max().y > rim_row.1,
            Edge::Bottom => rect.origin.y < rim && rect.max().y >= state.screen.y,
        };
        let edge_covered = (0..SIZE.0)
            .step_by(4)
            .filter(|&x| pixel(&opened, x, rim_row.0) != pixel(&closed, x, rim_row.0))
            .count();
        if !flush || edge_covered < (SIZE.0 / 4 / 2) as usize {
            wrong.push(format!(
                "{edge:?} drawer sits at {rect:?} and covers {edge_covered} edge pixels"
            ));
        }

        let (mut inside, mut changed, mut stray) = (0u32, 0u32, 0u32);
        for y in (0..SIZE.1).step_by(2) {
            for x in (0..SIZE.0).step_by(2) {
                let at = Vec2::new(f64::from(x), f64::from(y));
                let same = pixel(&opened, x, y) == pixel(&closed, x, y);
                let near = at.x >= rect.origin.x - 4.0
                    && at.x <= rect.max().x + 4.0
                    && at.y >= rect.origin.y - 4.0
                    && at.y <= rect.max().y + 4.0;
                let within = at.x > rect.origin.x + 1.0
                    && at.x < rect.max().x - 1.0
                    && at.y > rect.origin.y + 1.0
                    && at.y < rect.max().y - 1.0;
                if within {
                    inside += 1;
                    changed += u32::from(!same);
                }
                if !near && !same {
                    stray += 1;
                }
            }
        }
        if inside == 0 || f64::from(changed) / f64::from(inside) < 0.5 {
            wrong.push(format!(
                "{edge:?} drawer covers {changed} of {inside} pixels in {rect:?}"
            ));
        }
        if stray > 0 {
            wrong.push(format!(
                "{edge:?} drawer changed {stray} pixels outside {rect:?}"
            ));
        }

        run.screen.state.pointer_to(middle);
        run.screen.state.tick();
        pump(&mut run.screen, |state| {
            !state.drawers[index].visible() && state.drawers[index].settled()
        });
        let shut = run.shot(&format!("drawer-{edge:?}-shut"));
        if shut.rgba != closed.rgba {
            wrong.push(format!("{edge:?} drawer left a trace after it closed"));
        }
    }
    assert!(wrong.is_empty(), "drawers are off:\n{}", wrong.join("\n"));
}

#[test]
fn a_window_grab_at_the_edge_does_not_open_a_bar() {
    let mut run = start("grab-edge", &Config::default(), None);
    let bottom = Edge::Bottom.index();
    let rim = Vec2::new(f64::from(SIZE.0) / 2.0, f64::from(SIZE.1) - 1.0);
    run.screen.state.grab = Grab::Move {
        id: u64::MAX,
        offset: Vec2::ZERO,
    };
    run.screen.state.pointer_to(rim);
    run.screen.state.tick();
    let waited = Instant::now() + run.screen.state.tunables.drawer_dwell * 4;
    pump(&mut run.screen, |_| Instant::now() >= waited);
    assert!(
        !run.screen.state.drawers[bottom].open,
        "the bottom bar opened while a window was held at the edge"
    );

    run.screen.state.grab = Grab::None;
    run.screen.state.pointer_to(rim);
    run.screen.state.tick();
    pump(&mut run.screen, |state| state.drawers[bottom].open);
    assert!(
        run.screen.state.drawers[bottom].open,
        "the bottom bar does not open after the window is let go"
    );
}

#[test]
fn a_bar_that_does_not_hide_stays_on_screen() {
    let config = Config::parse("[motion]\nhide_bottom = false\n").unwrap();
    let mut run = start("pinned-bar", &config, None);
    let (top, bottom) = (Edge::Top.index(), Edge::Bottom.index());
    let middle = Vec2::new(f64::from(SIZE.0) / 2.0, f64::from(SIZE.1) / 2.0);
    run.screen.state.pointer_to(middle);
    run.screen.state.kick_now();
    pump(&mut run.screen, |state| {
        state.drawers[bottom].open && state.drawers[bottom].settled()
    });
    assert!(
        run.screen.state.drawers[bottom].open,
        "the pinned bar is not shown"
    );

    run.screen.state.pointer_to(Vec2::new(middle.x, 0.0));
    run.screen.state.tick();
    pump(&mut run.screen, |state| state.drawers[top].open);
    run.screen.state.pointer_to(middle);
    run.screen.state.tick();
    pump(&mut run.screen, |state| {
        !state.drawers[top].open && state.drawers[top].settled()
    });
    assert!(
        !run.screen.state.drawers[top].open,
        "the top bar did not hide"
    );
    assert!(
        run.screen.state.drawers[bottom].open,
        "the pinned bar hid with the top bar"
    );
}

#[derive(Default)]
struct Listed {
    title: String,
    app: String,
    done: bool,
    closed: bool,
}

#[derive(Default)]
struct Lister {
    seen: Vec<(ExtForeignToplevelHandleV1, Listed)>,
}

impl Lister {
    fn entry(&mut self, handle: &ExtForeignToplevelHandleV1) -> &mut Listed {
        let at = match self.seen.iter().position(|(each, _)| each == handle) {
            Some(at) => at,
            None => {
                self.seen.push((handle.clone(), Listed::default()));
                self.seen.len() - 1
            }
        };
        &mut self.seen[at].1
    }
}

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Lister {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtForeignToplevelListV1, ()> for Lister {
    fn event(
        state: &mut Self,
        _: &ExtForeignToplevelListV1,
        event: ext_foreign_toplevel_list_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_foreign_toplevel_list_v1::Event::Toplevel { toplevel } = event {
            state.entry(&toplevel);
        }
    }

    event_created_child!(Lister, ExtForeignToplevelListV1, [
        ext_foreign_toplevel_list_v1::EVT_TOPLEVEL_OPCODE => (ExtForeignToplevelHandleV1, ()),
    ]);
}

impl Dispatch<ExtForeignToplevelHandleV1, ()> for Lister {
    fn event(
        state: &mut Self,
        handle: &ExtForeignToplevelHandleV1,
        event: ext_foreign_toplevel_handle_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let entry = state.entry(handle);
        match event {
            ext_foreign_toplevel_handle_v1::Event::Title { title } => entry.title = title,
            ext_foreign_toplevel_handle_v1::Event::AppId { app_id } => entry.app = app_id,
            ext_foreign_toplevel_handle_v1::Event::Done => entry.done = true,
            ext_foreign_toplevel_handle_v1::Event::Closed => entry.closed = true,
            _ => {}
        }
    }
}

fn list_windows(report: Arc<Mutex<Vec<(String, String, bool, bool)>>>, stop: Arc<AtomicBool>) {
    let connection = Connection::connect_to_env().expect("the lister connects");
    let (globals, mut queue) = registry_queue_init::<Lister>(&connection).expect("a registry");
    let qh = queue.handle();
    let _list: ExtForeignToplevelListV1 = globals
        .bind(&qh, 1..=1, ())
        .expect("ext_foreign_toplevel_list_v1");
    let mut lister = Lister::default();
    while !stop.load(Ordering::Relaxed) {
        let _ = connection.flush();
        if let Some(guard) = connection.prepare_read() {
            let _ = guard.read();
        }
        let _ = queue.dispatch_pending(&mut lister);
        *report.lock().unwrap() = lister
            .seen
            .iter()
            .map(|(handle, seen)| {
                let _ = handle.id();
                (seen.title.clone(), seen.app.clone(), seen.done, seen.closed)
            })
            .collect();
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn other_apps_see_each_window_come_and_go() {
    let mut run = start("toplevels", &Config::default(), None);
    let report = Arc::new(Mutex::new(Vec::new()));
    let watching = Arc::new(AtomicBool::new(false));
    let lister = {
        let (report, watching) = (report.clone(), watching.clone());
        std::thread::spawn(move || list_windows(report, watching))
    };
    let (stop, client) = red_window(&mut run);

    let seen =
        |report: &Arc<Mutex<Vec<(String, String, bool, bool)>>>| report.lock().unwrap().clone();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && !seen(&report).iter().any(|entry| entry.2) {
        run.screen
            .events
            .dispatch(Some(Duration::from_millis(10)), &mut run.screen.state)
            .unwrap();
    }
    assert_eq!(
        seen(&report),
        [(
            "red".to_string(),
            "drifter-test-red".to_string(),
            true,
            false
        )],
        "the list names the window by its title and app id"
    );

    stop.store(true, Ordering::Relaxed);
    let _ = client.join();
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && !seen(&report).iter().any(|entry| entry.3) {
        run.screen
            .events
            .dispatch(Some(Duration::from_millis(10)), &mut run.screen.state)
            .unwrap();
    }
    assert!(
        seen(&report).iter().all(|entry| entry.3),
        "a window that is gone is closed in the list: {:?}",
        seen(&report)
    );
    watching.store(true, Ordering::Relaxed);
    let _ = lister.join();
}

#[test]
fn a_window_that_inhibits_shortcuts_keeps_them_while_focused() {
    let mut run = start("inhibit", &Config::default(), None);
    let (stop, client) = red_window_with(&mut run, true);
    pump(&mut run.screen, |state| state.shortcuts_inhibited());
    assert!(
        run.screen.state.shortcuts_inhibited(),
        "the focused window holds the shortcuts"
    );
    stop.store(true, Ordering::Relaxed);
    let _ = client.join();
    pump(&mut run.screen, |state| !state.shortcuts_inhibited());
    assert!(!run.screen.state.shortcuts_inhibited());
    drop(run);

    let mut run = start("no-inhibit", &Config::default(), None);
    let (stop, client) = red_window(&mut run);
    assert!(!run.screen.state.shortcuts_inhibited());
    stop.store(true, Ordering::Relaxed);
    let _ = client.join();
}

struct Locker {
    shm: Shm,
    pool: SlotPool,
    surface: wl_surface::WlSurface,
    locked: Arc<AtomicBool>,
}

impl ShmHandler for Locker {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_shm!(Locker);
delegate_noop!(Locker: ignore wl_compositor::WlCompositor);
delegate_noop!(Locker: ignore wl_surface::WlSurface);
delegate_noop!(Locker: ignore wl_output::WlOutput);
delegate_noop!(Locker: ExtSessionLockManagerV1);

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Locker {
    fn event(
        _: &mut Self,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ExtSessionLockV1, ()> for Locker {
    fn event(
        state: &mut Self,
        _: &ExtSessionLockV1,
        event: ext_session_lock_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let ext_session_lock_v1::Event::Locked = event {
            state.locked.store(true, Ordering::Relaxed);
        }
    }
}

impl Dispatch<ExtSessionLockSurfaceV1, ()> for Locker {
    fn event(
        state: &mut Self,
        lock_surface: &ExtSessionLockSurfaceV1,
        event: ext_session_lock_surface_v1::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let ext_session_lock_surface_v1::Event::Configure {
            serial,
            width,
            height,
        } = event
        else {
            return;
        };
        lock_surface.ack_configure(serial);
        let Ok((buffer, canvas)) = state.pool.create_buffer(
            width as i32,
            height as i32,
            width as i32 * 4,
            wl_shm::Format::Argb8888,
        ) else {
            return;
        };
        for pixel in canvas.as_chunks_mut::<4>().0 {
            *pixel = [0, 255, 0, 255];
        }
        state
            .surface
            .damage_buffer(0, 0, width as i32, height as i32);
        if buffer.attach_to(&state.surface).is_ok() {
            state.surface.commit();
        }
    }
}

fn run_locker(locked: Arc<AtomicBool>, unlock: Arc<AtomicBool>, quit: Arc<AtomicBool>) {
    let connection = Connection::connect_to_env().expect("the locker connects");
    let (globals, mut queue) = registry_queue_init::<Locker>(&connection).expect("a registry");
    let qh = queue.handle();
    let compositor: wl_compositor::WlCompositor =
        globals.bind(&qh, 1..=4, ()).expect("wl_compositor");
    let output: wl_output::WlOutput = globals.bind(&qh, 1..=1, ()).expect("wl_output");
    let manager: ExtSessionLockManagerV1 = globals
        .bind(&qh, 1..=1, ())
        .expect("ext_session_lock_manager_v1");
    let shm = Shm::bind(&globals, &qh).expect("wl_shm");
    let pool = SlotPool::new((SIZE.0 * SIZE.1 * 4) as usize, &shm).expect("shared memory");
    let surface = compositor.create_surface(&qh, ());
    let lock = manager.lock(&qh, ());
    let _lock_surface = lock.get_lock_surface(&surface, &output, &qh, ());
    let mut locker = Locker {
        shm,
        pool,
        surface,
        locked,
    };
    let mut unlocked = false;
    while !quit.load(Ordering::Relaxed) {
        if !unlocked && unlock.load(Ordering::Relaxed) {
            lock.unlock_and_destroy();
            unlocked = true;
        }
        let _ = connection.flush();
        if let Some(guard) = connection.prepare_read() {
            let _ = guard.read();
        }
        let _ = queue.dispatch_pending(&mut locker);
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn green(image: &Image) -> bool {
    pixel(image, SIZE.0 / 4, SIZE.1 / 4) == [0, 255, 0, 255]
}

#[test]
fn a_screen_locker_covers_everything_until_it_unlocks() {
    let mut run = start("session-lock", &Config::default(), None);
    let (stop, client) = red_window(&mut run);
    let (locked, unlock, quit) = (
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    );
    let locker = {
        let (locked, unlock, quit) = (locked.clone(), unlock.clone(), quit.clone());
        std::thread::spawn(move || run_locker(locked, unlock, quit))
    };

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !locked.load(Ordering::Relaxed) {
        run.screen
            .events
            .dispatch(Some(Duration::from_millis(10)), &mut run.screen.state)
            .unwrap();
    }
    assert!(
        locked.load(Ordering::Relaxed),
        "the locker hears that the screen is locked"
    );
    let started = Instant::now();
    pump(&mut run.screen, |_| {
        started.elapsed() > Duration::from_millis(300)
    });
    let shot = run.shot("locked");
    assert!(green(&shot), "the lock surface fills the screen");
    assert_eq!(red_box(&shot), None, "no window shows through the lock");
    let focus = run.screen.state.wayland.keyboard.current_focus();
    let lock_surface = run
        .screen
        .state
        .client_lock
        .as_ref()
        .and_then(|held| held.surfaces.first())
        .map(|(surface, _)| surface.wl_surface().clone());
    assert!(
        focus.is_some() && focus == lock_surface,
        "keys go to the locker"
    );

    unlock.store(true, Ordering::Relaxed);
    pump(&mut run.screen, |state| state.client_lock.is_none());
    assert!(
        run.screen.state.client_lock.is_none(),
        "the locker unlocked"
    );
    let started = Instant::now();
    pump(&mut run.screen, |_| {
        started.elapsed() > Duration::from_millis(300)
    });
    let shot = run.shot("unlocked");
    assert!(!green(&shot), "the lock is gone");
    assert!(red_box(&shot).is_some(), "the window is back");
    assert!(
        run.screen.state.lock.is_none(),
        "a clean unlock leaves no lock behind"
    );

    quit.store(true, Ordering::Relaxed);
    let _ = locker.join();
    stop.store(true, Ordering::Relaxed);
    let _ = client.join();
}

#[test]
fn a_smaller_world_pulls_windows_back_inside() {
    let config = Config::parse("[workspace]\nwidth = 9\nheight = 9\n").unwrap();
    let mut run = start("shrink", &config, None);
    let (stop, client) = red_window(&mut run);
    let state = &mut run.screen.state;
    let far = Vec2::new(state.world.x - 300.0, state.world.y - 200.0);
    let at = state
        .windows
        .iter()
        .position(|window| window.app == "drifter-test-red")
        .expect("the red window is placed");
    state.windows[at].content.origin = far;

    state.screens_wide = (3, 3);
    state.relayout(false);
    let world = state.world;
    let content = state.windows[at].content;
    assert!(
        content.origin.x >= 0.0
            && content.origin.y >= 0.0
            && content.max().x <= world.x
            && content.max().y <= world.y,
        "the window {content:?} sits inside the world {world:?}"
    );
    stop.store(true, Ordering::Relaxed);
    let _ = client.join();
}

fn shared_dmabuf(
    width: u32,
    height: u32,
    modifier: smithay::backend::allocator::Modifier,
) -> (smithay::backend::allocator::dmabuf::Dmabuf, std::fs::File) {
    use smithay::backend::allocator::Fourcc;
    use smithay::backend::allocator::dmabuf::{Dmabuf, DmabufFlags};
    use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};

    #[repr(C)]
    struct UdmabufCreate {
        memfd: u32,
        flags: u32,
        offset: u64,
        size: u64,
    }
    const UDMABUF_CREATE: libc::c_ulong = 0x4018_7542;

    let size = (width * height * 4) as usize;
    let page = 4096;
    let size = size.div_ceil(page) * page;
    let memfd = unsafe { libc::memfd_create(c"drifter-copy".as_ptr(), libc::MFD_ALLOW_SEALING) };
    assert!(memfd >= 0, "memfd_create works");
    let memory = unsafe { std::fs::File::from_raw_fd(memfd) };
    memory.set_len(size as u64).unwrap();
    unsafe { libc::fcntl(memfd, libc::F_ADD_SEALS, libc::F_SEAL_SHRINK) };
    let device = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open("/dev/udmabuf")
        .expect("/dev/udmabuf opens; the test needs it to make a dmabuf");
    let mut create = UdmabufCreate {
        memfd: memfd as u32,
        flags: 1,
        offset: 0,
        size: size as u64,
    };
    let fd = unsafe { libc::ioctl(device.as_raw_fd(), UDMABUF_CREATE, &mut create) };
    assert!(fd >= 0, "udmabuf makes a dmabuf");
    let mut builder = Dmabuf::builder(
        (width as i32, height as i32),
        Fourcc::Xrgb8888,
        modifier,
        DmabufFlags::empty(),
    );
    builder.add_plane(unsafe { OwnedFd::from_raw_fd(fd) }, 0, 0, width * 4);
    (builder.build().expect("a dmabuf"), memory)
}

#[test]
fn pixman_copies_the_screen_into_a_dmabuf_it_cannot_bind() {
    use std::io::Read;

    let mut run = start("dmabuf", &Config::default(), None);
    let (stop, client) = red_window(&mut run);
    let expected = run.shot("dmabuf-expected");
    let state = &mut run.screen.state;
    let shown: Vec<Shown> = state
        .elements()
        .into_iter()
        .map(|piece| RescaleRenderElement::from_element(piece, (0, 0).into(), 1.0))
        .collect();

    let (mut dmabuf, mut memory) = shared_dmabuf(
        SIZE.0,
        SIZE.1,
        smithay::backend::allocator::Modifier::Invalid,
    );
    assert!(
        state.draw_into(&mut dmabuf, &shown, false).is_err(),
        "pixman refuses a dmabuf with an implicit modifier"
    );
    state
        .copy_into(&mut dmabuf, &shown, false)
        .expect("the copy falls back to writing the dmabuf by hand");
    let mut bytes = Vec::new();
    memory.read_to_end(&mut bytes).unwrap();
    let mut wrong = 0;
    for y in (0..SIZE.1).step_by(7) {
        for x in (0..SIZE.0).step_by(7) {
            let at = ((y * SIZE.0 + x) * 4) as usize;
            let got = [bytes[at + 2], bytes[at + 1], bytes[at], 255];
            if !alike(got, pixel(&expected, x, y)) {
                wrong += 1;
            }
        }
    }
    assert_eq!(wrong, 0, "the dmabuf holds the screen");
    assert!(red_box(&expected).is_some(), "the screen shows the red window");

    let (mut tiled, _memory) = shared_dmabuf(
        SIZE.0,
        SIZE.1,
        smithay::backend::allocator::Modifier::I915_x_tiled,
    );
    assert!(
        state.copy_into(&mut tiled, &shown, false).is_err(),
        "a tiled dmabuf is refused, not filled with a scrambled picture"
    );
    stop.store(true, Ordering::Relaxed);
    let _ = client.join();
}

mod wlr_taskbar {
    use super::*;
    use wayland_protocols_wlr::foreign_toplevel::v1::client::{
        zwlr_foreign_toplevel_handle_v1::{self, ZwlrForeignToplevelHandleV1},
        zwlr_foreign_toplevel_manager_v1::{self, ZwlrForeignToplevelManagerV1},
    };

    #[derive(Clone, Debug, Default, PartialEq)]
    pub struct Entry {
        pub title: String,
        pub app: String,
        pub states: Vec<u32>,
        pub done: bool,
        pub closed: bool,
    }

    #[derive(Clone, Copy, Debug)]
    pub enum Order {
        Minimize,
        Activate,
        Maximize,
        Close,
    }

    delegate_noop!(Bar: ignore wl_seat::WlSeat);

    #[derive(Default)]
    struct Bar {
        seen: Vec<(ZwlrForeignToplevelHandleV1, Entry)>,
    }

    impl Bar {
        fn entry(&mut self, handle: &ZwlrForeignToplevelHandleV1) -> &mut Entry {
            let at = match self.seen.iter().position(|(each, _)| each == handle) {
                Some(at) => at,
                None => {
                    self.seen.push((handle.clone(), Entry::default()));
                    self.seen.len() - 1
                }
            };
            &mut self.seen[at].1
        }
    }

    impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Bar {
        fn event(
            _: &mut Self,
            _: &wl_registry::WlRegistry,
            _: wl_registry::Event,
            _: &GlobalListContents,
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
        }
    }

    impl Dispatch<ZwlrForeignToplevelManagerV1, ()> for Bar {
        fn event(
            state: &mut Self,
            _: &ZwlrForeignToplevelManagerV1,
            event: zwlr_foreign_toplevel_manager_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            if let zwlr_foreign_toplevel_manager_v1::Event::Toplevel { toplevel } = event {
                state.entry(&toplevel);
            }
        }

        event_created_child!(Bar, ZwlrForeignToplevelManagerV1, [
            zwlr_foreign_toplevel_manager_v1::EVT_TOPLEVEL_OPCODE => (ZwlrForeignToplevelHandleV1, ()),
        ]);
    }

    impl Dispatch<ZwlrForeignToplevelHandleV1, ()> for Bar {
        fn event(
            state: &mut Self,
            handle: &ZwlrForeignToplevelHandleV1,
            event: zwlr_foreign_toplevel_handle_v1::Event,
            _: &(),
            _: &Connection,
            _: &QueueHandle<Self>,
        ) {
            let entry = state.entry(handle);
            match event {
                zwlr_foreign_toplevel_handle_v1::Event::Title { title } => entry.title = title,
                zwlr_foreign_toplevel_handle_v1::Event::AppId { app_id } => entry.app = app_id,
                zwlr_foreign_toplevel_handle_v1::Event::State { state } => {
                    entry.states = state
                        .chunks_exact(4)
                        .map(|bytes| u32::from_ne_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
                        .collect();
                }
                zwlr_foreign_toplevel_handle_v1::Event::Done => entry.done = true,
                zwlr_foreign_toplevel_handle_v1::Event::Closed => entry.closed = true,
                _ => {}
            }
        }
    }

    pub fn run(
        report: Arc<Mutex<Vec<Entry>>>,
        orders: Arc<Mutex<Vec<Order>>>,
        stop: Arc<AtomicBool>,
    ) {
        let connection = Connection::connect_to_env().expect("the bar connects");
        let (globals, mut queue) = registry_queue_init::<Bar>(&connection).expect("a registry");
        let qh = queue.handle();
        let seat: wl_seat::WlSeat = globals.bind(&qh, 1..=1, ()).expect("a seat");
        let _manager: ZwlrForeignToplevelManagerV1 = globals
            .bind(&qh, 3..=3, ())
            .expect("zwlr_foreign_toplevel_manager_v1");
        let mut bar = Bar::default();
        while !stop.load(Ordering::Relaxed) {
            for order in std::mem::take(&mut *orders.lock().unwrap()) {
                let Some((handle, _)) = bar.seen.first() else {
                    continue;
                };
                match order {
                    Order::Minimize => handle.set_minimized(),
                    Order::Activate => handle.activate(&seat),
                    Order::Maximize => handle.set_maximized(),
                    Order::Close => handle.close(),
                }
            }
            let _ = connection.flush();
            if let Some(guard) = connection.prepare_read() {
                let _ = guard.read();
            }
            let _ = queue.dispatch_pending(&mut bar);
            *report.lock().unwrap() = bar.seen.iter().map(|(_, entry)| entry.clone()).collect();
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}

#[test]
fn a_taskbar_sees_windows_and_can_minimize_raise_maximize_and_close_them() {
    use wlr_taskbar::{Entry, Order};
    const MAXIMIZED: u32 = 0;
    const MINIMIZED: u32 = 1;
    const ACTIVATED: u32 = 2;

    let mut run = start("taskbar", &Config::default(), None);
    let report: Arc<Mutex<Vec<Entry>>> = Arc::new(Mutex::new(Vec::new()));
    let orders = Arc::new(Mutex::new(Vec::new()));
    let quit = Arc::new(AtomicBool::new(false));
    let bar = {
        let (report, orders, quit) = (report.clone(), orders.clone(), quit.clone());
        std::thread::spawn(move || wlr_taskbar::run(report, orders, quit))
    };
    let (stop, client) = red_window(&mut run);
    let seen = |report: &Arc<Mutex<Vec<Entry>>>| report.lock().unwrap().first().cloned();
    let wait = |run: &mut Run, test: &dyn Fn(&State, &Entry) -> bool| {
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if let Some(entry) = seen(&report)
                && test(&run.screen.state, &entry)
            {
                return entry;
            }
            assert!(
                Instant::now() < deadline,
                "timed out; the bar sees {:?}",
                seen(&report)
            );
            run.screen
                .events
                .dispatch(Some(Duration::from_millis(10)), &mut run.screen.state)
                .unwrap();
            run.screen.state.sync_wlr_toplevels();
        }
    };
    let red = |state: &State| {
        state
            .windows
            .iter()
            .find(|window| window.app == "drifter-test-red")
            .map(|window| (window.id, window.hidden, window.restore.is_some(), window.going))
    };

    let entry = wait(&mut run, &|_, entry| entry.done && entry.title == "red");
    assert_eq!(entry.app, "drifter-test-red");
    assert!(entry.states.contains(&ACTIVATED), "the focused window is active: {entry:?}");

    orders.lock().unwrap().push(Order::Minimize);
    let entry = wait(&mut run, &|state, entry| {
        entry.states.contains(&MINIMIZED) && red(state).is_some_and(|w| w.1)
    });
    assert!(!entry.states.contains(&ACTIVATED), "a minimized window is not active");

    orders.lock().unwrap().push(Order::Activate);
    wait(&mut run, &|state, entry| {
        entry.states.contains(&ACTIVATED)
            && !entry.states.contains(&MINIMIZED)
            && red(state).is_some_and(|w| !w.1 && state.focus == Some(w.0))
    });

    orders.lock().unwrap().push(Order::Maximize);
    wait(&mut run, &|state, entry| {
        entry.states.contains(&MAXIMIZED) && red(state).is_some_and(|w| w.2)
    });

    orders.lock().unwrap().push(Order::Close);
    wait(&mut run, &|state, entry| {
        entry.closed && red(state).is_none_or(|w| w.3)
    });

    quit.store(true, Ordering::Relaxed);
    let _ = bar.join();
    stop.store(true, Ordering::Relaxed);
    let _ = client.join();
}

fn run_tool(run: &mut Run, program: &str, args: &[&str]) -> (bool, String) {
    let child = std::process::Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn();
    let mut child = child.unwrap_or_else(|error| panic!("{program} runs: {error}"));
    let deadline = Instant::now() + Duration::from_secs(5);
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "{program} finishes");
        run.screen
            .events
            .dispatch(Some(Duration::from_millis(10)), &mut run.screen.state)
            .unwrap();
    };
    let output = child.wait_with_output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout).to_string()
        + &String::from_utf8_lossy(&output.stderr);
    (status.success(), text)
}

#[test]
fn wlr_randr_lists_and_changes_the_screen() {
    let mut run = start("randr", &Config::default(), None);
    let name = run.screen.state.heads[0].name.clone();

    let (ok, listed) = run_tool(&mut run, "wlr-randr", &[]);
    assert!(ok, "wlr-randr lists the heads: {listed}");
    assert!(listed.contains(&name), "the head is named: {listed}");
    assert!(listed.contains("800x600"), "the mode is listed: {listed}");
    assert!(listed.contains("Enabled: yes"), "the head is on: {listed}");

    let (ok, said) = run_tool(&mut run, "wlr-randr", &["--output", &name, "--scale", "2", "--transform", "90"]);
    assert!(ok, "wlr-randr changes the head: {said}");
    let head = &run.screen.state.heads[0];
    assert_eq!((head.scale, head.rotate), (2.0, 90));
    assert_eq!(head.screen, Vec2::new(300.0, 400.0), "a turned head at scale 2 is 300 x 400");

    let (ok, listed) = run_tool(&mut run, "wlr-randr", &[]);
    assert!(ok && listed.contains("Scale: 2.0") && listed.contains("Transform: 90"), "{listed}");

    let (ok, said) = run_tool(&mut run, "wlr-randr", &["--output", &name, "--off"]);
    assert!(!ok, "the last head cannot be turned off: {said}");
    assert_eq!(run.screen.state.heads.len(), 1);

    let (ok, said) = run_tool(&mut run, "wlr-randr", &["--output", &name, "--transform", "flipped"]);
    assert!(!ok, "a flipped head is refused: {said}");
    assert_eq!(run.screen.state.heads[0].rotate, 90);
}

#[test]
fn sticky_keys_hold_a_tapped_ctrl_for_the_next_key() {
    const CTRL: u32 = 37;
    const C: u32 = 54;
    let mut run = start("sticky", &Config::default(), None);
    let state = &mut run.screen.state;
    let ctrl = |state: &State| state.wayland.keyboard.modifier_state().ctrl;

    state.key_in(CTRL, true, 1);
    state.key_in(CTRL, false, 2);
    state.key_in(C, true, 3);
    assert!(!ctrl(state), "without sticky keys a tap does nothing");
    state.key_in(C, false, 4);

    state.access.sticky_keys = true;
    state.key_in(CTRL, true, 5);
    state.key_in(CTRL, false, 6);
    assert!(!ctrl(state), "the tap itself lets go");
    state.key_in(C, true, 7);
    assert!(ctrl(state), "ctrl is down while c is down");
    state.key_in(C, false, 8);
    assert!(!ctrl(state), "ctrl lets go after c");
    state.key_in(C, true, 9);
    assert!(!ctrl(state), "only the next key gets ctrl");
    state.key_in(C, false, 10);
}

#[test]
fn a_place_is_saved_with_the_keys_and_flown_back_to() {
    const SUPER: u32 = 133;
    const SHIFT: u32 = 50;
    const THREE: u32 = 12;
    let mut run = start("places", &Config::default(), None);
    let folder = run.folder.clone();
    let state = &mut run.screen.state;
    state.config_path = folder.join("drifter.toml");
    std::fs::write(&state.config_path, "[lock]\nafter = 9\n").unwrap();

    let here = Camera::new(Vec2::new(5000.0, 3000.0), 0.5);
    look_at(state, here);
    for (code, pressed) in [(SUPER, true), (SHIFT, true), (THREE, true), (THREE, false), (SHIFT, false), (SUPER, false)] {
        state.key_in(code, pressed, 1);
    }
    let saved = Config::load_from(&state.config_path).unwrap();
    assert_eq!(saved.lock.after, 9, "the rest of the config stays");
    assert_eq!(saved.place.len(), 1);
    assert_eq!(saved.place[0].name, "Place 3");
    assert_eq!((saved.place[0].x, saved.place[0].y, saved.place[0].zoom), (5000.0, 3000.0, 0.5));

    look_at(state, Camera::new(Vec2::new(9000.0, 6000.0), 1.0));
    for (code, pressed) in [(SUPER, true), (THREE, true), (THREE, false), (SUPER, false)] {
        state.key_in(code, pressed, 2);
    }
    assert!(state.flight.is_some(), "the camera flies to the place");
    let deadline = Instant::now() + Duration::from_secs(3);
    while state.flight.is_some() && Instant::now() < deadline {
        state.steer();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        (state.camera.center - here.center).length() < 1.0 && (state.camera.zoom - 0.5).abs() < 1e-6,
        "the camera is back at the place: {:?}",
        state.camera
    );
}
