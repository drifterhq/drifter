use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use drifter_core::config::ShotFormat;
use drifter_core::lang::{t, tf};
use drifter_core::paint::Canvas;
use drifter_core::{Config, Font, Image, Rect, Vec2};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::data_device_manager::data_device::{DataDevice, DataDeviceHandler};
use smithay_client_toolkit::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use smithay_client_toolkit::data_device_manager::data_source::{CopyPasteSource, DataSourceHandler};
use smithay_client_toolkit::data_device_manager::{DataDeviceManagerState, WritePipe};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1::{Shape as Cursor, WpCursorShapeDeviceV1};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers};
use smithay_client_toolkit::seat::pointer::cursor_shape::CursorShapeManager;
use smithay_client_toolkit::seat::pointer::{BTN_LEFT, PointerEvent, PointerEventKind, PointerHandler};
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

use crate::draw::{self, Clip};
use crate::look::{self, Answer, Bar, BAR_H, Fonts, Hit, Look, PAD};
use crate::markup::{self, BOTTOM, Grab, Handle, Kind, LEFT, MIDDLE, Map, Markup, RIGHT, Shape, TOP, Tool};

const APP: &str = "drifter-markup";
const SETTLE: Duration = Duration::from_millis(16);
const FRAME_LATE: Duration = Duration::from_millis(100);
const TOAST: Duration = Duration::from_millis(2200);
const SLOP: f64 = 6.0;
const PNG: &str = "image/png";

static SCALE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

fn scale() -> u32 {
    SCALE.load(std::sync::atomic::Ordering::Relaxed)
}

struct Faces {
    path: Option<PathBuf>,
    fonts: RefCell<HashMap<u64, Font>>,
}

impl Faces {
    fn with<T>(&self, px: f64, work: impl FnOnce(&Font) -> T) -> Option<T> {
        let key = (px * 4.0).round().max(8.0) as u64;
        let mut fonts = self.fonts.borrow_mut();
        if !fonts.contains_key(&key) {
            if fonts.len() >= 24 {
                fonts.clear();
            }
            let font = Font::find(self.path.clone(), key as f64 / 4.0).ok()?;
            fonts.insert(key, font);
        }
        fonts.get(&key).map(work)
    }

    fn glyph(&self, words: &str, px: f64, ink: [u8; 3]) -> Option<Image> {
        self.with(px, |font| drifter_core::paint::at_density(1, || font.paint_weight(words, ink, 560.0)))
    }

    fn measure(&self, words: &str, px: f64) -> Vec2 {
        self.with(px, |font| markup::measure_lines(words, px, &|line| font.width_of(line), font.line_height().ceil()))
            .unwrap_or(Vec2::new(0.0, px * 1.2))
    }
}

type SceneKey = (u32, u32, u64, i64, i64, Clip, [u8; 3]);

struct Editor {
    registry: RegistryState,
    seats: SeatState,
    outputs: OutputState,
    shm: Shm,
    pool: SlotPool,
    window: Window,
    qh: QueueHandle<Editor>,
    handle: smithay_client_toolkit::reexports::calloop::LoopHandle<'static, Editor>,
    pointer: Option<wl_pointer::WlPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    mods: Modifiers,
    data: Option<DataDeviceManagerState>,
    device: Option<DataDevice>,
    held: Option<(CopyPasteSource, Vec<u8>)>,
    shapes: Option<CursorShapeManager>,
    cursor: Option<WpCursorShapeDeviceV1>,
    cursor_serial: u32,
    cursor_now: Option<Cursor>,
    serial: u32,
    size: (u32, u32),
    configured: bool,
    exit: bool,
    resized: bool,
    stale: bool,
    shown: Option<Instant>,
    look: Look,
    fonts: (Font, Font, Font),
    faces: Faces,
    markup: Markup,
    path: PathBuf,
    format: ShotFormat,
    clear: bool,
    base: Option<((u64, bool), Image)>,
    scene: Option<(SceneKey, Vec<Shape>, Image)>,
    at: Vec2,
    pressing: bool,
    last_press: Option<(Instant, Vec2)>,
    hover: Option<Hit>,
    tip: Option<Hit>,
    tip_plan: u64,
    tip_seen: Option<Instant>,
    asking: bool,
    answer_hover: Option<Answer>,
    lit: Option<drifter_core::glide::Glide>,
    shown_base: Option<Image>,
    glow_due: bool,
    toast: Option<(String, Instant)>,
    blink: bool,
    appearance: drifter_core::config::Appearance,
    config_seen: Option<std::time::SystemTime>,
    watched_at: Instant,
}

fn modified(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

fn themed(look: &drifter_core::config::Appearance) -> Look {
    let palette = drifter_core::theme::of(drifter_core::theme::now(look.style));
    let accent = look.accent.map(|accent| accent.color()).unwrap_or([64, 128, 238]);
    Look::themed(&palette, accent)
}

fn format_of(path: &Path) -> ShotFormat {
    let ext = path.extension().map(|ext| ext.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => ShotFormat::Jpeg,
        _ => ShotFormat::Png,
    }
}

pub fn run(path: PathBuf) -> Result<(), String> {
    let config_path = Config::config_path();
    let config = Config::load_from(&config_path).unwrap_or_default();
    drifter_core::lang::use_config(config.appearance.language.as_deref());
    let find = |px| Font::find(config.font_path(), px);
    let fonts = (find(13.0)?, find(11.5)?, find(17.0)?);
    let source = drifter_core::image::load(&path)?;

    let connection = Connection::connect_to_env().map_err(|e| format!("no wayland: {e}"))?;
    let (globals, queue) = registry_queue_init(&connection).map_err(|e| format!("no wayland registry: {e}"))?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(|e| format!("no wl_compositor: {e}"))?;
    let shell = XdgShell::bind(&globals, &qh).map_err(|e| format!("no xdg shell: {e}"))?;
    let shm = Shm::bind(&globals, &qh).map_err(|e| format!("no wl_shm: {e}"))?;
    let outputs = OutputState::new(&globals, &qh);

    let surface = compositor.create_surface(&qh);
    let window = shell.create_window(surface, WindowDecorations::RequestServer, &qh);
    let title = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_else(|| path.display().to_string());
    window.set_title(title);
    window.set_app_id(APP);
    let look = themed(&config.appearance);
    let least = look.least();
    window.set_min_size(Some((least.x as u32, least.y as u32)));
    window.commit();

    let pool = SlotPool::new((least.x * least.y * 4.0) as usize, &shm).map_err(|e| format!("no shared memory: {e}"))?;
    let mut events: EventLoop<'static, Editor> = EventLoop::try_new().map_err(|e| format!("no event loop: {e}"))?;
    WaylandSource::new(connection, queue)
        .insert(events.handle())
        .map_err(|e| format!("cannot watch wayland: {e}"))?;
    events
        .handle()
        .insert_source(Timer::from_duration(Duration::from_millis(500)), |_, _, app: &mut Editor| {
            app.tick();
            TimeoutAction::ToDuration(Duration::from_millis(500))
        })
        .map_err(|e| format!("cannot start the timer: {e}"))?;

    let mut app = Editor {
        registry: RegistryState::new(&globals),
        seats: SeatState::new(&globals, &qh),
        outputs,
        shm,
        pool,
        window,
        qh: qh.clone(),
        handle: events.handle(),
        pointer: None,
        keyboard: None,
        mods: Modifiers::default(),
        data: DataDeviceManagerState::bind(&globals, &qh).ok(),
        device: None,
        held: None,
        shapes: CursorShapeManager::bind(&globals, &qh).ok(),
        cursor: None,
        cursor_serial: 0,
        cursor_now: None,
        serial: 0,
        size: (least.x as u32, least.y as u32),
        configured: false,
        exit: false,
        resized: false,
        stale: true,
        shown: None,
        look,
        fonts,
        faces: Faces { path: config.font_path(), fonts: RefCell::new(HashMap::new()) },
        clear: draw::has_alpha(&source),
        markup: Markup::new(source),
        format: format_of(&path),
        path,
        base: None,
        scene: None,
        at: Vec2::ZERO,
        pressing: false,
        last_press: None,
        hover: None,
        tip: None,
        tip_plan: 0,
        tip_seen: None,
        asking: false,
        answer_hover: None,
        lit: None,
        shown_base: None,
        glow_due: false,
        toast: None,
        blink: true,
        appearance: config.appearance.clone(),
        config_seen: modified(&config_path),
        watched_at: Instant::now(),
    };
    while !app.exit {
        events.dispatch((app.stale || app.glow_due).then_some(SETTLE), &mut app).map_err(|e| format!("event loop: {e}"))?;
        if app.configured && (std::mem::take(&mut app.resized) || (app.stale && app.frame_free())) {
            app.draw();
        } else if app.configured && app.glow_due && app.frame_free() {
            app.reshow();
        }
    }
    Ok(())
}

impl Editor {
    fn view(&self) -> Vec2 {
        Vec2::new(f64::from(self.size.0), f64::from(self.size.1))
    }

    fn fonts(&self) -> Fonts<'_> {
        Fonts { body: &self.fonts.0, small: &self.fonts.1, title: &self.fonts.2 }
    }

    fn frame_free(&self) -> bool {
        self.shown.is_none_or(|at| at.elapsed() >= FRAME_LATE)
    }

    fn map(&self) -> Map {
        look::view_map(&self.look, self.view(), self.markup.shown(), self.markup.unit)
    }

    fn image_at(&self) -> Vec2 {
        self.map().from(self.at)
    }

    fn slop(&self) -> f64 {
        SLOP / self.map().scale
    }

    fn on_canvas(&self) -> bool {
        self.at.y >= BAR_H
    }

    fn wanted(&self, room: Option<(u32, u32)>) -> (u32, u32) {
        let natural = self.markup.source.size() / self.markup.unit;
        let want = natural + Vec2::new(PAD * 2.0, PAD * 2.0 + BAR_H);
        let screen = self.outputs.outputs().filter_map(|output| self.outputs.info(&output)).find_map(|info| {
            info.logical_size.or_else(|| {
                let mode = info.modes.iter().find(|mode| mode.current)?;
                let dense = info.scale_factor.max(1);
                Some((mode.dimensions.0 / dense, mode.dimensions.1 / dense))
            })
        });
        let room = room.map(|(w, h)| Vec2::new(f64::from(w), f64::from(h))).or(screen.map(|(w, h)| Vec2::new(f64::from(w) * 0.92, f64::from(h) * 0.88)));
        let room = room.unwrap_or(Vec2::new(1600.0, 1000.0));
        let least = self.look.least();
        let size = want.min(room).max(least);
        (size.x.round() as u32, size.y.round() as u32)
    }

    fn live(&self) -> usize {
        let len = self.markup.doc.shapes.len();
        let grabbed = match &self.markup.grab {
            Some(Grab::Draw { index, .. } | Grab::Move { index, .. } | Grab::Handle { index, .. }) => Some(*index),
            _ => None,
        };
        grabbed.or(self.markup.editing).unwrap_or(len).min(len)
    }

    fn base_image(&mut self, deep: Map) -> &Image {
        let key = (deep.scale.to_bits(), self.look.dark);
        if self.base.as_ref().is_none_or(|(had, _)| *had != key) {
            let source = &self.markup.source;
            let size = source.size() * deep.scale;
            let (width, height) = (size.x.round().max(1.0) as u32, size.y.round().max(1.0) as u32);
            let scaled = source.scaled(width, height);
            let image = match self.clear {
                true => {
                    let (light, dark) = self.look.checker_cells();
                    let mut ground = draw::checker(width, height, 8 * scale(), light, dark);
                    draw::stamp(&mut ground, &scaled, (0, 0), draw::whole(&scaled));
                    ground
                }
                false => scaled,
            };
            self.base = Some((key, image));
        }
        &self.base.as_ref().expect("a base").1
    }

    fn draw(&mut self) {
        self.stale = false;
        let dense = scale();
        drifter_core::paint::set_density(dense);
        let d = f64::from(dense);
        let size = self.view();
        let map = self.map();
        let deep = Map { scale: map.scale * d, shift: map.shift * d };
        let (width, height) = (drifter_core::paint::dense(size.x), drifter_core::paint::dense(size.y));
        let shown = self.markup.shown();
        let clip = draw::clip_of(deep.rect(shown), [0, (BAR_H * d) as i64, i64::from(width), i64::from(height)]);
        let live = self.live();
        let key: SceneKey = (width, height, deep.scale.to_bits(), deep.shift.x as i64, deep.shift.y as i64, clip, self.look.back);
        let below = &self.markup.doc.shapes[..live];
        let fresh = self.scene.as_ref().is_none_or(|(had, shapes, _)| *had != key || shapes.as_slice() != below);
        if fresh {
            let mut canvas = Canvas::new(self.size.0, self.size.1);
            canvas.round_rect(Rect::new(Vec2::ZERO, size), 0.0, self.look.back, 1.0);
            canvas.shadow(map.rect(shown), 2.0, 14.0, self.look.drop * 0.9);
            let spot = deep.to(Vec2::ZERO);
            let origin = (spot.x.round() as i64, spot.y.round() as i64);
            let mut image = canvas.image;
            let base = self.base_image(deep).clone();
            draw::stamp(&mut image, &base, origin, clip);
            let glyphs = |words: &str, px: f64, ink: [u8; 3]| self.faces.glyph(words, px, ink);
            for shape in &self.markup.doc.shapes[..live] {
                markup::paint(&mut image, shape, self.markup.unit, &self.markup.source, deep, clip, &glyphs);
            }
            self.scene = Some((key, self.markup.doc.shapes[..live].to_vec(), image));
        }
        let mut image = self.scene.as_ref().expect("a scene").2.clone();
        {
            let glyphs = |words: &str, px: f64, ink: [u8; 3]| self.faces.glyph(words, px, ink);
            for shape in &self.markup.doc.shapes[live..] {
                markup::paint(&mut image, shape, self.markup.unit, &self.markup.source, deep, clip, &glyphs);
            }
        }
        let mut canvas = Canvas::on(image);
        let fonts = self.fonts();
        let unit = self.markup.unit;
        if self.markup.tool == Tool::Crop {
            let held = matches!(self.markup.grab, Some(Grab::Crop { .. }));
            self.look.paint_crop(&mut canvas, map, shown, self.markup.doc.crop, held, &fonts);
        }
        let drawing = matches!(self.markup.grab, Some(Grab::Draw { .. }));
        if let Some(shape) = self.markup.picked.and_then(|index| self.markup.doc.shapes.get(index))
            && !drawing
        {
            self.look.paint_picked(&mut canvas, map, shape, unit);
        }
        if let Some(index) = self.markup.editing
            && let Some(shape) = self.markup.doc.shapes.get(index)
        {
            let caret = self.blink.then(|| self.caret(shape, map)).flatten();
            self.look.paint_editing(&mut canvas, map, shape, unit, caret);
        }
        let bar = Bar {
            tool: self.markup.tool,
            ink: self.markup.ink,
            weight: self.markup.weight,
            text: self.markup.text_mode(),
            undo: self.markup.can_undo(),
            redo: self.markup.can_redo(),
            hover: self.hover.filter(|_| !self.asking),
        };
        self.look.paint_bar(&mut canvas, size, &fonts, &bar);
        if let Some(hit) = self.tip.filter(|_| !self.asking && !self.pressing) {
            self.look.paint_tip(&mut canvas, size, &fonts, hit, bar.text);
        }
        if let Some((words, _)) = &self.toast {
            self.look.paint_toast(&mut canvas, size, &fonts, words, 1.0);
        }
        if self.asking {
            self.look.paint_prompt(&mut canvas, size, &fonts, self.answer_hover);
        }
        let image = canvas.image;
        self.show(&image);
    }

    fn caret(&self, shape: &Shape, map: Map) -> Option<(Vec2, Vec2)> {
        let Kind::Text { at, words, .. } = &shape.kind else {
            return None;
        };
        let px = shape.text_px(self.markup.unit);
        let (wide, line) = self.faces.with(px, |font| {
            let last = words.rsplit('\n').next().unwrap_or("");
            (font.width_of(last), font.line_height().ceil())
        })?;
        let row = words.matches('\n').count() as f64;
        let top = *at + Vec2::new(wide + 1.0, row * line + line * 0.12);
        Some((map.to(top), map.to(top + Vec2::new(0.0, line * 0.8))))
    }

    fn aim_lit(&mut self) {
        let bar = Bar {
            tool: self.markup.tool,
            ink: self.markup.ink,
            weight: self.markup.weight,
            text: self.markup.text_mode(),
            undo: self.markup.can_undo(),
            redo: self.markup.can_redo(),
            hover: self.hover.filter(|_| !self.asking && !self.pressing),
        };
        let target = self.look.lit_target(self.view(), &self.fonts(), &bar);
        let before = self.lit.as_ref().map(|lit| (lit.rect(), lit.shade()));
        drifter_core::glide::Glide::aim(&mut self.lit, target, 1.0);
        if self.lit.as_ref().map(|lit| (lit.rect(), lit.shade())) != before || self.lit_busy() {
            self.glow_due = true;
        }
    }

    fn lit_busy(&self) -> bool {
        self.lit.as_ref().is_some_and(drifter_core::glide::Glide::busy)
    }

    fn reshow(&mut self) {
        self.glow_due = false;
        if let Some(base) = self.shown_base.take() {
            self.show(&base);
        }
    }

    fn show(&mut self, image: &Image) {
        self.glow_due = false;
        self.shown_base = Some(image.clone());
        let glowing;
        let image = match self.lit.as_ref().filter(|lit| lit.shade() > 0.0005) {
            Some(lit) => {
                drifter_core::paint::set_density(scale());
                let mut canvas = drifter_core::paint::Canvas::on(image.clone());
                lit.draw(&mut canvas, self.look.ink);
                glowing = canvas.image;
                &glowing
            }
            None => image,
        };
        if self.lit_busy() {
            self.glow_due = true;
        }
        let (width, height) = (image.width, image.height);
        let stride = width as i32 * 4;
        let (buffer, canvas) = match self.pool.create_buffer(width as i32, height as i32, stride, wl_shm::Format::Argb8888) {
            Ok(made) => made,
            Err(error) => {
                log!("drifter-markup: no buffer: {error}");
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
            log!("drifter-markup: {error}");
            return;
        }
        surface.frame(&self.qh, surface.clone());
        self.shown = Some(Instant::now());
        self.window.commit();
    }

    fn tick(&mut self) {
        self.watch();
        if self.markup.editing.is_some() {
            self.blink = !self.blink;
            self.stale = true;
        }
        if self.toast.as_ref().is_some_and(|(_, at)| at.elapsed() >= TOAST) {
            self.toast = None;
            self.stale = true;
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
                self.appearance = config.appearance.clone();
            }
        }
        let again = themed(&self.appearance);
        if again != self.look {
            self.look = again;
            self.stale = true;
        }
    }

    fn say(&mut self, words: String) {
        self.toast = Some((words, Instant::now()));
        self.stale = true;
    }

    fn render(&self) -> Image {
        let glyphs = |words: &str, px: f64, ink: [u8; 3]| self.faces.glyph(words, px, ink);
        self.markup.render(&glyphs)
    }

    fn copy(&mut self) {
        let (Some(data), Some(device)) = (self.data.as_ref(), self.device.as_ref()) else {
            self.say(t("The clipboard is not available.").to_string());
            return;
        };
        let image = {
            let glyphs = |words: &str, px: f64, ink: [u8; 3]| self.faces.glyph(words, px, ink);
            self.markup.render(&glyphs)
        };
        match markup::png_bytes(&image) {
            Ok(bytes) => {
                let source = data.create_copy_paste_source(&self.qh, [PNG]);
                source.set_selection(device, self.serial);
                self.held = Some((source, bytes));
                self.say(t("Copied").to_string());
            }
            Err(error) => self.say(tf("Could not copy: {}", &[&error])),
        }
    }

    fn save(&self) -> Result<(), String> {
        let image = self.render();
        let folder = self.path.parent().filter(|dir| !dir.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let name = self.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let temp = folder.join(format!(".{name}.markup-{}", std::process::id()));
        let written = drifter_core::image::save_as(&image, &temp, self.format).and_then(|()| {
            if let Ok(meta) = std::fs::metadata(&self.path) {
                let _ = std::fs::set_permissions(&temp, meta.permissions());
            }
            std::fs::rename(&temp, &self.path).map_err(|e| format!("{}: {e}", self.path.display()))
        });
        if written.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        written
    }

    fn done(&mut self) {
        self.markup.finish();
        if !self.markup.dirty() {
            self.exit = true;
            return;
        }
        match self.save() {
            Ok(()) => self.exit = true,
            Err(error) => {
                self.asking = false;
                self.say(tf("Could not save: {}", &[&error]));
            }
        }
    }

    fn cancel(&mut self) {
        self.markup.finish();
        match self.markup.dirty() {
            true => {
                self.asking = true;
                self.answer_hover = None;
                self.tip = None;
                self.stale = true;
            }
            false => self.exit = true,
        }
    }

    fn trash(&mut self) {
        match drifter_core::explorer::ops::trash(&self.path) {
            Ok(_) => self.exit = true,
            Err(error) => self.say(tf("Could not move it to the Trash: {}", &[&error])),
        }
    }

    fn answer(&mut self, answer: Answer) {
        self.asking = false;
        match answer {
            Answer::Save => self.done(),
            Answer::Discard => self.exit = true,
            Answer::Cancel => {}
        }
        self.stale = true;
    }

    fn act(&mut self, hit: Hit) {
        let faces = &self.faces;
        let measure = |words: &str, px: f64| faces.measure(words, px);
        match hit {
            Hit::Tool(tool) => self.markup.set_tool(tool),
            Hit::Ink(index) => self.markup.set_ink(markup::INKS[index.min(markup::INKS.len() - 1)].0),
            Hit::Weight(weight) => self.markup.set_weight(weight, &measure),
            Hit::Undo => self.markup.undo(),
            Hit::Redo => self.markup.redo(),
            Hit::Copy => self.copy(),
            Hit::Trash => self.trash(),
            Hit::Done => self.done(),
        }
        self.stale = true;
    }

    fn plan_tip(&mut self) {
        self.tip_plan += 1;
        let Some(hit) = self.hover else {
            return;
        };
        let plan = self.tip_plan;
        let quick = self.tip_seen.is_some_and(|at| at.elapsed() < Duration::from_millis(1500));
        let wait = Duration::from_millis(if quick { 60 } else { 650 });
        let _ = self.handle.insert_source(Timer::from_duration(wait), move |_, _, app: &mut Editor| {
            if app.tip_plan == plan && app.hover == Some(hit) && app.tip.is_none() && !app.pressing {
                app.tip = Some(hit);
                app.tip_seen = Some(Instant::now());
                app.stale = true;
            }
            TimeoutAction::Drop
        });
    }

    fn want_cursor(&self) -> Cursor {
        if self.asking || !self.on_canvas() {
            return Cursor::Default;
        }
        let at = self.image_at();
        let slop = self.slop();
        let edge_cursor = |edge: u8| match edge {
            MIDDLE => Cursor::Move,
            edge if edge == LEFT | TOP || edge == RIGHT | BOTTOM => Cursor::NwseResize,
            edge if edge == RIGHT | TOP || edge == LEFT | BOTTOM => Cursor::NeswResize,
            LEFT | RIGHT => Cursor::EwResize,
            _ => Cursor::NsResize,
        };
        match &self.markup.grab {
            Some(Grab::Crop { edge, .. }) => return edge_cursor(*edge),
            Some(Grab::Move { .. }) => return Cursor::Grabbing,
            Some(Grab::Handle { handle: Handle::Edge(edge), .. }) => return edge_cursor(*edge),
            Some(Grab::Handle { .. }) => return Cursor::Crosshair,
            Some(Grab::Draw { .. } | Grab::Maybe { .. }) => return Cursor::Crosshair,
            None => {}
        }
        if self.markup.tool == Tool::Crop {
            return self.markup.crop_edge(at, slop).map_or(Cursor::Default, edge_cursor);
        }
        match self.markup.handle_at(at, slop * 1.4) {
            Some(Handle::Edge(edge)) => return edge_cursor(edge),
            Some(Handle::Radius) => return Cursor::EwResize,
            Some(Handle::Zoom) => return Cursor::Pointer,
            Some(_) => return Cursor::Crosshair,
            None => {}
        }
        if let Some(index) = self.markup.editing
            && self.markup.doc.shapes.get(index).is_some_and(|shape| shape.near(at, slop, self.markup.unit))
        {
            return Cursor::Text;
        }
        let shown = self.markup.shown();
        if !Rect::new(shown.origin - Vec2::splat(slop * 4.0), shown.size + Vec2::splat(slop * 8.0)).contains(at) {
            return Cursor::Default;
        }
        match self.markup.hit(at, slop) {
            Some(index) => match (&self.markup.doc.shapes[index].kind, self.markup.tool) {
                (Kind::Text { .. }, Tool::Text) => Cursor::Text,
                _ if matches!(self.markup.tool, Tool::Pen | Tool::Highlight) && self.markup.picked != Some(index) => Cursor::Crosshair,
                _ => Cursor::Move,
            },
            None if self.markup.tool == Tool::Text => Cursor::Text,
            None => Cursor::Crosshair,
        }
    }

    fn set_cursor(&mut self) {
        let want = self.want_cursor();
        if self.cursor_now == Some(want) {
            return;
        }
        if let Some(device) = &self.cursor {
            device.set_shape(self.cursor_serial, want);
            self.cursor_now = Some(want);
        }
    }

    fn motion(&mut self) {
        if self.asking {
            let hover = self.look.prompt_buttons(self.view(), &self.fonts()).into_iter().find(|(_, area)| area.contains(self.at)).map(|(answer, _)| answer);
            if hover != self.answer_hover {
                self.answer_hover = hover;
                self.stale = true;
            }
            self.set_cursor();
            return;
        }
        if self.pressing {
            let (at, slop, shift) = (self.image_at(), self.slop(), self.mods.shift);
            self.markup.drag(at, slop, shift);
            self.stale = true;
            self.set_cursor();
            return;
        }
        let hover = self.look.hit(self.view(), &self.fonts(), self.at);
        if hover != self.hover {
            self.hover = hover;
            if self.tip.take().is_some() {
                self.tip_seen = Some(Instant::now());
            }
            self.plan_tip();
            self.stale = true;
        }
        self.set_cursor();
    }

    fn press(&mut self) {
        self.tip = None;
        self.tip_plan += 1;
        if self.asking {
            let fonts = self.fonts();
            let answer = self.look.prompt_buttons(self.view(), &fonts).into_iter().find(|(_, area)| area.contains(self.at)).map(|(answer, _)| answer);
            if let Some(answer) = answer {
                self.answer(answer);
            }
            return;
        }
        let hit = self.look.hit(self.view(), &self.fonts(), self.at);
        if let Some(hit) = hit {
            self.act(hit);
            return;
        }
        if !self.on_canvas() {
            return;
        }
        let at = self.image_at();
        let double = self.last_press.is_some_and(|(when, spot)| when.elapsed() < Duration::from_millis(400) && (spot - self.at).length() < 5.0);
        self.last_press = Some((Instant::now(), self.at));
        let slop = self.slop();
        let faces = &self.faces;
        let measure = |words: &str, px: f64| faces.measure(words, px);
        self.markup.press(at, slop, double, &measure);
        self.pressing = self.markup.grab.is_some();
        self.blink = true;
        self.stale = true;
        self.set_cursor();
    }

    fn let_go(&mut self) {
        if !std::mem::take(&mut self.pressing) {
            return;
        }
        let slop = self.slop();
        self.markup.release(slop);
        self.stale = true;
        self.set_cursor();
    }

    fn key(&mut self, event: &KeyEvent) {
        let (ctrl, shift) = (self.mods.ctrl, self.mods.shift);
        if self.asking {
            match event.keysym {
                Keysym::Return | Keysym::KP_Enter | Keysym::s | Keysym::S => self.answer(Answer::Save),
                Keysym::Escape | Keysym::c | Keysym::C => self.answer(Answer::Cancel),
                Keysym::d | Keysym::D | Keysym::n | Keysym::N => self.answer(Answer::Discard),
                _ => {}
            }
            return;
        }
        self.tip = None;
        self.stale = true;
        self.blink = true;
        let faces = &self.faces;
        let measure = |words: &str, px: f64| faces.measure(words, px);
        if self.markup.editing.is_some() {
            match event.keysym {
                Keysym::Escape => return self.markup.finish(),
                Keysym::Return | Keysym::KP_Enter if shift => return self.markup.type_text("\n", &measure),
                Keysym::Return | Keysym::KP_Enter => return self.markup.finish(),
                Keysym::BackSpace => return self.markup.erase_text(ctrl, &measure),
                _ if !ctrl && !self.mods.logo => {
                    if let Some(text) = event.utf8.as_deref().filter(|text| !text.is_empty() && !text.chars().any(char::is_control)) {
                        self.markup.type_text(text, &measure);
                    }
                    return;
                }
                _ => {}
            }
        }
        let step = if shift { 10.0 } else { 1.0 } / self.map().scale;
        match event.keysym {
            Keysym::z | Keysym::Z if ctrl && shift => self.markup.redo(),
            Keysym::z | Keysym::Z if ctrl => self.markup.undo(),
            Keysym::y | Keysym::Y if ctrl => self.markup.redo(),
            Keysym::c | Keysym::C if ctrl => self.copy(),
            Keysym::s | Keysym::S if ctrl => self.done(),
            Keysym::Return | Keysym::KP_Enter => self.done(),
            Keysym::Escape => self.cancel(),
            Keysym::Delete | Keysym::BackSpace => self.markup.remove(),
            Keysym::Left => self.markup.nudge(Vec2::new(-step, 0.0)),
            Keysym::Right => self.markup.nudge(Vec2::new(step, 0.0)),
            Keysym::Up => self.markup.nudge(Vec2::new(0.0, -step)),
            Keysym::Down => self.markup.nudge(Vec2::new(0.0, step)),
            _ if !ctrl && !self.mods.alt && !self.mods.logo => {
                let typed = event.utf8.as_deref().and_then(|text| text.chars().next()).map(|c| c.to_ascii_lowercase());
                if let Some(tool) = Tool::ALL.iter().find(|tool| Some(tool.key()) == typed) {
                    self.markup.set_tool(*tool);
                }
            }
            _ => {}
        }
    }
}

impl CompositorHandler for Editor {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, factor: i32) {
        let factor = factor.clamp(1, 4) as u32;
        if factor == scale() {
            return;
        }
        SCALE.store(factor, std::sync::atomic::Ordering::Relaxed);
        drifter_core::paint::set_density(factor);
        if !self.markup.dirty() && self.markup.past.is_empty() && self.markup.unit < f64::from(factor) {
            self.markup.unit = f64::from(factor);
        }
        self.base = None;
        self.scene = None;
        self.stale = true;
    }

    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}

    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        self.shown = None;
    }

    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}

    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for Editor {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }

    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}

    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl WindowHandler for Editor {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {
        match self.asking {
            true => self.exit = true,
            false => self.cancel(),
        }
    }

    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window, configure: WindowConfigure, _: u32) {
        if !self.configured {
            let dense = self.outputs.outputs().filter_map(|output| self.outputs.info(&output)).map(|info| info.scale_factor).max().unwrap_or(1);
            self.markup.unit = f64::from(dense.clamp(1, 4));
        }
        let least = self.look.least();
        let room = configure.suggested_bounds.filter(|room| room.0 > 0 && room.1 > 0);
        let free = match self.configured {
            true => self.size,
            false => self.wanted(room),
        };
        let width = configure.new_size.0.map(|v| v.get()).unwrap_or(free.0);
        let height = configure.new_size.1.map(|v| v.get()).unwrap_or(free.1);
        self.size = (width.max(least.x as u32), height.max(least.y as u32));
        self.configured = true;
        self.resized = true;
    }
}

impl SeatHandler for Editor {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }

    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}

    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer && self.pointer.is_none() {
            match self.seats.get_pointer(qh, &seat) {
                Ok(pointer) => {
                    self.cursor = self.shapes.as_ref().map(|shapes| shapes.get_shape_device(&pointer, qh));
                    self.pointer = Some(pointer);
                }
                Err(error) => log!("drifter-markup: no pointer: {error}"),
            }
        }
        if capability == Capability::Keyboard && self.device.is_none() {
            self.device = self.data.as_ref().map(|data| data.get_data_device(qh, &seat));
        }
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            let repeat = Box::new(|app: &mut Editor, _: &wl_keyboard::WlKeyboard, event: KeyEvent| app.key(&event));
            match self.seats.get_keyboard_with_repeat(qh, &seat, None, self.handle.clone(), repeat) {
                Ok(keyboard) => self.keyboard = Some(keyboard),
                Err(error) => log!("drifter-markup: no keyboard: {error}"),
            }
        }
    }

    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, capability: Capability) {
        if capability == Capability::Pointer
            && let Some(pointer) = self.pointer.take()
        {
            if let Some(cursor) = self.cursor.take() {
                cursor.destroy();
            }
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

impl KeyboardHandler for Editor {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, serial: u32, _: &[u32], _: &[Keysym]) {
        self.serial = serial;
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {}

    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, serial: u32, event: KeyEvent) {
        self.serial = serial;
        self.key(&event);
    }

    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}

    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, modifiers: Modifiers, _: u32) {
        self.mods = modifiers;
    }
}

impl PointerHandler for Editor {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for event in events {
            if &event.surface != self.window.wl_surface() {
                continue;
            }
            self.at = Vec2::new(event.position.0, event.position.1);
            match event.kind {
                PointerEventKind::Enter { serial } => {
                    self.cursor_serial = serial;
                    self.cursor_now = None;
                    self.motion();
                }
                PointerEventKind::Motion { .. } => {
                    self.motion();
                    self.aim_lit();
                }
                PointerEventKind::Press { button, serial, .. } if button == BTN_LEFT => {
                    self.serial = serial;
                    drifter_core::glide::Glide::press(&mut self.lit);
                    self.press();
                    self.aim_lit();
                }
                PointerEventKind::Release { button, .. } if button == BTN_LEFT => self.let_go(),
                PointerEventKind::Leave { .. } => {
                    self.let_go();
                    if self.hover.take().is_some() || self.tip.take().is_some() {
                        self.stale = true;
                    }
                    self.tip_plan += 1;
                    self.aim_lit();
                }
                _ => {}
            }
        }
    }
}

impl ShmHandler for Editor {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Editor {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }

    registry_handlers![OutputState, SeatState];
}

impl DataDeviceHandler for Editor {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64, _: &wl_surface::WlSurface) {}

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice, _: f64, _: f64) {}

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn drop_performed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}
}

impl DataOfferHandler for Editor {
    fn source_actions(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}

    fn selected_action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
}

impl DataSourceHandler for Editor {
    fn accept_mime(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: Option<String>) {}

    fn send_request(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource, mime: String, fd: WritePipe) {
        let bytes = self.held.as_ref().filter(|(ours, _)| ours.inner() == source && mime == PNG).map(|(_, bytes)| bytes.clone());
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
        match self.held.as_ref().is_some_and(|(ours, _)| ours.inner() == source) {
            true => self.held = None,
            false => source.destroy(),
        }
    }

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

delegate_compositor!(Editor);
delegate_output!(Editor);
delegate_shm!(Editor);
delegate_seat!(Editor);
delegate_pointer!(Editor);
delegate_keyboard!(Editor);
delegate_xdg_shell!(Editor);
delegate_xdg_window!(Editor);
delegate_registry!(Editor);
delegate_data_device!(Editor);
