use super::*;
use smithay::backend::renderer::gles::{GlesPixelProgram, GlesTexture, Uniform, UniformName, UniformType};
use smithay::backend::renderer::{Bind, Frame, Offscreen, Renderer};

const STRIPS: i32 = 15;
const GPU_STILL: u32 = 240;
const FROST_SETTLE: Duration = Duration::from_millis(150);
const ART_KEEP: Duration = Duration::from_secs(5);

pub(super) struct Sheet {
    pub texture: GlesTexture,
    pub buffer: TextureBuffer<Tex>,
}

struct Art {
    size: (i32, i32),
    front: Sheet,
    back: Option<Sheet>,
    strip: i32,
    cycle: Option<(f64, f64)>,
    started: Instant,
    used: Instant,
}

pub(super) struct Live {
    program: GlesPixelProgram,
    half: bool,
    arts: Vec<Art>,
}

impl Live {
    pub(super) fn compile(renderer: &mut Painter, path: &Path) -> Option<Live> {
        let Painter::Gles(gles) = renderer else {
            return None;
        };
        let body = match std::fs::read_to_string(path) {
            Ok(body) => body,
            Err(error) => {
                log!("drifter: cannot read {}: {error}", path.display());
                return None;
            }
        };
        let mut names = vec![
            UniformName::new("T", UniformType::_1f),
            UniformName::new("L", UniformType::_1f),
            UniformName::new("W", UniformType::_2f),
        ];
        names.extend(["S0", "S1", "A", "B", "C"].map(|name| UniformName::new(name, UniformType::_3f)));
        match gles.compile_custom_pixel_shader(drifter_core::shader::source(&body), &names) {
            Ok(program) => Some(Live {
                program,
                half: drifter_core::shader::half(path),
                arts: Vec::new(),
            }),
            Err(error) => {
                log!("drifter: {} does not compile: {error}", path.display());
                None
            }
        }
    }

    pub(super) fn show(
        &mut self,
        renderer: &mut Painter,
        paper: Vec2,
        clock: f64,
        every: Option<Duration>,
    ) -> Option<(TextureBuffer<Tex>, Vec2)> {
        let div = if self.half { 2.0 } else { 1.0 };
        let size = (
            ((paper.x / div).round() as i32).max(1),
            ((paper.y / div).round() as i32).max(1),
        );
        let result = match self.arts.iter().position(|art| art.size == size) {
            Some(at) => match every {
                Some(every) => self.advance(renderer, at, paper, clock, every),
                None => Ok(()),
            },
            None => self.first(renderer, size, paper, clock),
        };
        if let Err(error) = result {
            log!("drifter: the live wallpaper did not draw: {error}");
        }
        let art = self.arts.iter_mut().find(|art| art.size == size)?;
        art.used = Instant::now();
        Some((art.front.buffer.clone(), Vec2::new(f64::from(size.0), f64::from(size.1))))
    }

    pub(super) fn wait(&self, every: Duration) -> Duration {
        if self.arts.is_empty() || self.arts.iter().any(|art| art.cycle.is_some()) {
            return TICK;
        }
        self.arts
            .iter()
            .map(|art| every.saturating_sub(art.started.elapsed()))
            .min()
            .unwrap_or(every)
            .max(TICK)
    }

    fn first(&mut self, renderer: &mut Painter, size: (i32, i32), paper: Vec2, clock: f64) -> Result<(), String> {
        self.arts.retain(|art| art.used.elapsed() < ART_KEEP);
        let mut front = sheet(renderer, size)?;
        let moment = (clock, drifter_core::shader::hour_now());
        paint(&self.program, renderer, &mut front, size, paper, moment, None)?;
        self.arts.push(Art {
            size,
            front,
            back: None,
            strip: 0,
            cycle: None,
            started: Instant::now(),
            used: Instant::now(),
        });
        Ok(())
    }

    fn advance(
        &mut self,
        renderer: &mut Painter,
        at: usize,
        paper: Vec2,
        clock: f64,
        every: Duration,
    ) -> Result<(), String> {
        let art = &mut self.arts[at];
        if art.cycle.is_none() {
            if art.started.elapsed() < every {
                return Ok(());
            }
            art.cycle = Some((clock, drifter_core::shader::hour_now()));
            art.strip = 0;
            art.started = Instant::now();
        }
        let Some(moment) = art.cycle else {
            return Ok(());
        };
        let mut back = match art.back.take() {
            Some(back) => back,
            None => sheet(renderer, art.size)?,
        };
        let top = art.size.1 * art.strip / STRIPS;
        let bottom = art.size.1 * (art.strip + 1) / STRIPS;
        let painted = paint(&self.program, renderer, &mut back, art.size, paper, moment, Some((top, bottom)));
        art.back = Some(back);
        painted?;
        art.strip += 1;
        if art.strip >= STRIPS {
            if let Some(back) = art.back.as_mut() {
                std::mem::swap(&mut art.front, back);
            }
            art.cycle = None;
        }
        Ok(())
    }
}

pub(super) fn sheet(renderer: &mut Painter, size: (i32, i32)) -> Result<Sheet, String> {
    let Painter::Gles(gles) = renderer else {
        return Err("the live wallpaper needs the gles renderer".to_string());
    };
    let texture = Offscreen::<GlesTexture>::create_buffer(gles, DrmFourcc::Abgr8888, size.into())
        .map_err(|e| e.to_string())?;
    let buffer = TextureBuffer::from_texture(&*renderer, Tex::Gles(texture.clone()), 1, Transform::Normal, None);
    Ok(Sheet { texture, buffer })
}

fn paint(
    program: &GlesPixelProgram,
    renderer: &mut Painter,
    sheet: &mut Sheet,
    size: (i32, i32),
    paper: Vec2,
    (clock, hour): (f64, f64),
    rows: Option<(i32, i32)>,
) -> Result<(), String> {
    let Painter::Gles(gles) = renderer else {
        return Err("the live wallpaper needs the gles renderer".to_string());
    };
    let sky = drifter_core::shader::sky(hour);
    let mut uniforms = vec![
        Uniform::new("T", clock as f32),
        Uniform::new("L", sky.light as f32),
        Uniform::new("W", (paper.x as f32, paper.y as f32)),
    ];
    uniforms.extend(sky.uniforms().map(|(name, value)| Uniform::new(name, value)));
    let whole = smithay::utils::Rectangle::from_size(size.into());
    let band = rows.map(|(top, bottom)| {
        [smithay::utils::Rectangle::new((0, top).into(), (size.0, (bottom - top).max(0)).into())]
    });
    let mut target = gles.bind(&mut sheet.texture).map_err(|e| e.to_string())?;
    let mut frame = gles
        .render(&mut target, size.into(), Transform::Normal)
        .map_err(|e| e.to_string())?;
    frame
        .render_pixel_shader_to(
            program,
            smithay::utils::Rectangle::from_size((f64::from(size.0), f64::from(size.1)).into()),
            whole,
            size.into(),
            band.as_ref().map(|band| band.as_slice()),
            1.0,
            &uniforms,
        )
        .map_err(|e| e.to_string())?;
    let _ = frame.finish().map_err(|e| e.to_string())?;
    Ok(())
}

pub(super) fn logo_program(renderer: &mut Painter) -> Option<GlesPixelProgram> {
    overlay_program(renderer, drifter_core::welcome::logo_source(), false)
}

pub(super) fn squircle_logo_program(renderer: &mut Painter) -> Option<GlesPixelProgram> {
    overlay_program(renderer, drifter_core::welcome::squircle_logo_source(), true)
}

pub(super) fn desk_program(renderer: &mut Painter) -> Option<GlesPixelProgram> {
    overlay_program(renderer, drifter_core::welcome::desk_source(), true)
}

fn overlay_program(renderer: &mut Painter, source: String, extra: bool) -> Option<GlesPixelProgram> {
    let Painter::Gles(gles) = renderer else {
        return None;
    };
    let mut names = vec![
        UniformName::new("T", UniformType::_1f),
        UniformName::new("L", UniformType::_1f),
        UniformName::new("W", UniformType::_2f),
    ];
    names.extend(["S0", "S1", "A", "B", "C"].map(|name| UniformName::new(name, UniformType::_3f)));
    if extra {
        names.push(UniformName::new("U", UniformType::_4f));
    }
    match gles.compile_custom_pixel_shader(source, &names) {
        Ok(program) => Some(program),
        Err(error) => {
            log!("drifter: a welcome shader does not compile: {error}");
            None
        }
    }
}

pub(super) fn paint_logo(
    program: &GlesPixelProgram,
    renderer: &mut Painter,
    sheet: &mut Sheet,
    size: (i32, i32),
    clock: f64,
    hour: f64,
    extra: Option<[f32; 4]>,
) -> Result<(), String> {
    let Painter::Gles(gles) = renderer else {
        return Err("the welcome logo needs the gles renderer".to_string());
    };
    let sky = drifter_core::shader::sky(hour);
    let mut uniforms = vec![
        Uniform::new("T", clock as f32),
        Uniform::new("L", sky.light as f32),
        Uniform::new("W", (size.0 as f32, size.1 as f32)),
    ];
    uniforms.extend(sky.uniforms().map(|(name, value)| Uniform::new(name, value)));
    if let Some(u) = extra {
        uniforms.push(Uniform::new("U", (u[0], u[1], u[2], u[3])));
    }
    let whole = smithay::utils::Rectangle::from_size(size.into());
    let mut target = gles.bind(&mut sheet.texture).map_err(|e| e.to_string())?;
    let mut frame = gles
        .render(&mut target, size.into(), Transform::Normal)
        .map_err(|e| e.to_string())?;
    frame.clear(Color32F::TRANSPARENT, &[whole]).map_err(|e| e.to_string())?;
    frame
        .render_pixel_shader_to(
            program,
            smithay::utils::Rectangle::from_size((f64::from(size.0), f64::from(size.1)).into()),
            whole,
            size.into(),
            None,
            1.0,
            &uniforms,
        )
        .map_err(|e| e.to_string())?;
    let _ = frame.finish().map_err(|e| e.to_string())?;
    Ok(())
}

impl State {
    pub(super) fn watch_wallpaper(&mut self) {
        let hour = drifter_core::night::hour_now();
        if let (Some(shown), Some(was)) = (self.paper.shown.clone(), self.paper.hour)
            && was != hour
        {
            self.paper.hour = Some(hour);
            self.load_wallpaper(shown);
        }
        let Some(folder) = self.paper.folder.clone() else {
            return;
        };
        if self.paper.every == 0 {
            self.paper.next = None;
            return;
        }
        let now = Instant::now();
        let Some(later) = now.checked_add(Duration::from_secs(self.paper.every)) else {
            return;
        };
        let due = *self.paper.next.get_or_insert(later);
        if now < due {
            return;
        }
        self.paper.next = Some(later);
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos() as u64)
            .unwrap_or(0);
        let pictures = drifter_core::wallpaper::pictures(&folder);
        if let Some(next) = drifter_core::wallpaper::next_picture(
            &pictures,
            self.paper.shown.as_deref(),
            self.paper.order,
            seed,
        ) {
            self.load_wallpaper(next);
        }
    }

    pub(super) fn paper_clock(&mut self) -> f64 {
        let now = Instant::now();
        let (then, at) = self.paper.clock;
        let clock = then + now.duration_since(at).as_secs_f64() * self.paper.drift;
        self.paper.clock = (clock, now);
        clock
    }

    pub(super) fn live_every(&self) -> Duration {
        match self.paper.drift > 0.0 {
            true => LIVE_EVERY.div_f64(self.paper.drift.min(1.0)).min(LIVE_STILL),
            false => LIVE_STILL,
        }
    }

    pub(super) fn load_wallpaper(&self, path: PathBuf) {
        let reply = self.paper_tx.clone();
        let (world, screen) = (self.world, self.screen);
        let shader = drifter_core::shader::is_shader(&path);
        let cover = match shader {
            true => drifter_core::wallpaper::shader_paper(screen),
            false => world,
        };
        let gpu = matches!(self.renderer, Painter::Gles(_));
        let spawned = std::thread::Builder::new()
            .name("drifter-wallpaper".to_string())
            .spawn(
                move || match match shader && gpu {
                    true => Ok(drifter_core::shader::still(
                        &path,
                        GPU_STILL,
                        ((f64::from(GPU_STILL) * cover.y / cover.x.max(1.0)).round() as u32).max(1),
                        0.0,
                        drifter_core::shader::hour_now(),
                    )),
                    false => drifter_core::image::load_covering(&path, cover.x, cover.y),
                } {
                    Ok(image) => {
                        let pyramid = drifter_core::tiles::Pyramid::build(image, world, screen);
                        let _ = reply.send((path, Some(pyramid)));
                    }
                    Err(error) => {
                        log!("drifter: no wallpaper {}: {error}", path.display());
                        let _ = reply.send((path, None));
                    }
                },
            );
        if let Err(error) = spawned {
            log!("drifter: cannot load the wallpaper: {error}");
        }
    }

    pub(super) fn take_wallpaper(
        &mut self,
        path: PathBuf,
        pyramid: Option<drifter_core::tiles::Pyramid>,
    ) {
        if let Some(pyramid) = pyramid {
            let shader = drifter_core::shader::is_shader(&path);
            let live = match shader {
                true => Live::compile(&mut self.renderer, &path),
                false => None,
            };
            let old = self.wallpaper.replace(Backdrop::new(pyramid, live, shader));
            self.fading = old.map(|old| (old, Instant::now()));
            self.frost_key = None;
        }
        self.paper.hour = drifter_core::shader::is_shader(&path)
            .then(drifter_core::night::hour_now);
        self.paper.shown = Some(path);
        self.kick_now();
    }

    pub(super) fn laid_paper(&self) -> Option<(drifter_core::wallpaper::Placement, Vec2)> {
        let paper = self.wallpaper.as_ref()?;
        let laid = paper.laid(self.mode, self.screen, self.world, &self.camera);
        Some((laid, paper.pyramid.size()))
    }

    pub(super) fn veil(&self, plain: f64) -> f64 {
        match self.wallpaper.is_some() {
            true => f64::from(self.palette.veil) / 255.0,
            false => plain,
        }
    }

    pub(super) fn frost_bars(&mut self) {
        let Some((laid, _)) = self.laid_paper() else {
            return;
        };
        let q = |v: f64| (v / 6.0).round() as i64;
        let zoom = laid.target.size.x / laid.source.size.x.max(f64::MIN_POSITIVE);
        let drift = [q(laid.source.origin.x * zoom), q(laid.source.origin.y * zoom)];
        let key = [
            q(laid.target.origin.x),
            q(laid.target.origin.y),
            q(laid.target.size.x),
            q(laid.target.size.y),
            q(self.screen.x),
            i64::from(self.palette.veil) * 1000 + i64::from(self.palette.bar[0]),
            drift[0],
            drift[1],
        ];
        let aim = [
            self.camera.center.x.round() as i64,
            self.camera.center.y.round() as i64,
            (self.camera.zoom * 1000.0).round() as i64,
        ];
        if self.frost_drift.is_none_or(|(was, _)| was != aim) {
            self.frost_drift = Some((aim, Instant::now()));
        }
        let drifting = self
            .frost_key
            .is_some_and(|built| built[..6] == key[..6])
            && self
                .frost_drift
                .is_some_and(|(_, since)| since.elapsed() < FROST_SETTLE);
        if self.frost_key == Some(key)
            || drifting
            || self
                .frost_at
                .is_some_and(|at| at.elapsed() < Duration::from_millis(60))
        {
            return;
        }
        if self.frost_busy {
            return;
        }
        let Some(frost) = self.wallpaper.as_ref().map(|paper| paper.frost.clone()) else {
            return;
        };
        self.frost_key = Some(key);
        self.frost_at = Some(Instant::now());
        self.frost_busy = true;
        let (shapes, screen, palette, at) = (self.shapes, self.screen, self.palette, self.at);
        let picture = self.wallpaper.as_ref().map(|paper| paper.pyramid.size()).unwrap_or(screen);
        let density = self.density;
        self.background("frost", move || {
            drifter_core::paint::set_density(density);
            let dense = f64::from(density);
            let veil = palette.veil;
            let images = Edge::ALL.map(|edge| {
                let shape = shapes[edge.index()];
                let mut rest = Drawer::new(edge);
                rest.open = true;
                rest.advance(Duration::from_secs(1), Duration::from_millis(1));
                let origin = rest.rect(screen, &shape).origin;
                let mut image =
                    drifter_core::drawer::paint_flat(edge, shape.width(screen.x).max(1.0) as u32, &shape, palette.bar, veil);
                drifter_core::frost::underlay(&mut image, f64::from(veil) / 255.0, |point| {
                    frost.behind(&laid, picture, origin + point / dense)
                });
                drifter_core::drawer::rim(&mut image, edge, &shape, palette.rim, f64::from(palette.rim_alpha) / 255.0);
                image
            });
            move |state: &mut State| {
                state.frost_busy = false;
                let mut built = Vec::new();
                for image in &images {
                    match upload(&mut state.renderer, Some(image), "frosted drawer") {
                        Ok(Some(buffer)) => built.push(buffer),
                        _ => return,
                    }
                }
                if let Ok(panels) = <[TextureBuffer<Tex>; 2]>::try_from(built) {
                    if let Some(head) = state.heads.get_mut(at) {
                        head.panels = panels.clone();
                    }
                    if state.at == at {
                        state.panels = panels;
                    }
                }
                state.kick_now();
            }
        });
    }

    pub(super) fn settle_fade(&mut self) {
        if self
            .fading
            .as_ref()
            .is_some_and(|(_, since)| since.elapsed() >= PAPER_FADE)
        {
            self.fading = None;
        }
    }

    pub(super) fn evict_tiles(&mut self) {
        if let Some(paper) = self.wallpaper.as_mut() {
            paper
                .resident
                .retain(|_, (_, used)| used.elapsed() < TILE_KEEP);
        }
    }
}
