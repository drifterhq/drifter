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
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_output, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

use super::{RescaleRenderElement, Screen, Shown, State};

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
    let stop = Arc::new(AtomicBool::new(false));
    let client = {
        let stop = stop.clone();
        std::thread::spawn(move || run_client(stop))
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
