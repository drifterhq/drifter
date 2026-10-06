use super::*;
use smithay::backend::renderer::gles::{GlesPixelProgram, GlesTexture, Uniform, UniformName, UniformType};
use smithay::backend::renderer::{Bind, Frame, Offscreen, Renderer};

struct Art {
    size: (i32, i32),
    texture: GlesTexture,
    buffer: TextureBuffer<Tex>,
    drawn: Instant,
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

    pub(super) fn draw(
        &mut self,
        renderer: &mut Painter,
        screen: Vec2,
        clock: f64,
        every: Option<Duration>,
    ) -> Option<TextureBuffer<Tex>> {
        let div = if self.half { 2.0 } else { 1.0 };
        let size = (
            ((screen.x / div).round() as i32).max(1),
            ((screen.y / div).round() as i32).max(1),
        );
        let at = self.arts.iter().position(|art| art.size == size);
        let stale = match at {
            None => true,
            Some(at) => every.is_some_and(|every| self.arts[at].drawn.elapsed() >= every),
        };
        if stale && let Err(error) = self.paint(renderer, size, clock, at) {
            log!("drifter: the live wallpaper did not draw: {error}");
        }
        self.arts
            .iter()
            .find(|art| art.size == size)
            .map(|art| art.buffer.clone())
    }

    fn paint(
        &mut self,
        renderer: &mut Painter,
        size: (i32, i32),
        clock: f64,
        at: Option<usize>,
    ) -> Result<(), String> {
        let Painter::Gles(gles) = renderer else {
            return Err("the live wallpaper needs the gles renderer".to_string());
        };
        let mut texture = match at {
            Some(at) => self.arts.remove(at).texture,
            None => Offscreen::<GlesTexture>::create_buffer(gles, DrmFourcc::Abgr8888, size.into())
                .map_err(|e| e.to_string())?,
        };
        let sky = drifter_core::shader::sky(drifter_core::shader::hour_now());
        let mut uniforms = vec![
            Uniform::new("T", clock as f32),
            Uniform::new("L", sky.light as f32),
        ];
        uniforms.extend(sky.uniforms().map(|(name, value)| Uniform::new(name, value)));
        {
            let mut target = gles.bind(&mut texture).map_err(|e| e.to_string())?;
            let mut frame = gles
                .render(&mut target, size.into(), Transform::Normal)
                .map_err(|e| e.to_string())?;
            frame
                .render_pixel_shader_to(
                    &self.program,
                    smithay::utils::Rectangle::from_size((f64::from(size.0), f64::from(size.1)).into()),
                    smithay::utils::Rectangle::from_size(size.into()),
                    size.into(),
                    None,
                    1.0,
                    &uniforms,
                )
                .map_err(|e| e.to_string())?;
            let _ = frame.finish().map_err(|e| e.to_string())?;
        }
        let buffer = TextureBuffer::from_texture(&*renderer, Tex::Gles(texture.clone()), 1, Transform::Normal, None);
        self.arts.push(Art {
            size,
            texture,
            buffer,
            drawn: Instant::now(),
        });
        Ok(())
    }
}

impl State {
    pub(super) fn watch_wallpaper(&mut self) {
        let hour = drifter_core::livepaper::hour_now();
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
            true => LIVE_EVERY,
            false => LIVE_STILL,
        }
    }

    pub(super) fn load_wallpaper(&self, path: PathBuf) {
        let reply = self.paper_tx.clone();
        let (world, screen) = (self.world, self.screen);
        let cover = match drifter_core::shader::is_shader(&path) {
            true => screen,
            false => world,
        };
        let spawned = std::thread::Builder::new()
            .name("drifter-wallpaper".to_string())
            .spawn(
                move || match drifter_core::image::load_covering(&path, cover.x, cover.y) {
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
        self.paper.hour = drifter_core::livepaper::is_livepaper(&path)
            .then(drifter_core::livepaper::hour_now);
        self.paper.shown = Some(path);
        self.kick_now();
    }

    pub(super) fn laid_paper(&self) -> Option<(drifter_core::wallpaper::Placement, Vec2)> {
        let paper = self.wallpaper.as_ref()?;
        let size = paper.pyramid.size();
        let mode = match paper.fixed {
            true => drifter_core::WallpaperMode::Fixed,
            false => self.mode,
        };
        let laid = drifter_core::wallpaper::place(mode, size, self.screen, self.world, &self.camera);
        Some((laid, size))
    }

    pub(super) fn frosted(&self, image: &mut drifter_core::Image, origin: Vec2, solid: f64) {
        let (Some(paper), Some((laid, size))) = (self.wallpaper.as_ref(), self.laid_paper()) else {
            return;
        };
        drifter_core::frost::underlay(image, solid, |point| {
            paper.frost.behind(&laid, size, origin + point)
        });
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
        let key = [
            q(laid.target.origin.x),
            q(laid.target.origin.y),
            q(laid.target.size.x),
            q(laid.target.size.y),
            q(self.screen.x),
            i64::from(self.palette.veil) * 1000 + i64::from(self.palette.bar[0]),
        ];
        if self.frost_key == Some(key)
            || self
                .frost_at
                .is_some_and(|at| at.elapsed() < Duration::from_millis(60))
        {
            return;
        }
        self.frost_key = Some(key);
        self.frost_at = Some(Instant::now());
        let veil = self.palette.veil;
        let mut built = Vec::new();
        for edge in Edge::ALL {
            let shape = self.shapes[edge.index()];
            let mut rest = Drawer::new(edge);
            rest.open = true;
            rest.advance(Duration::from_secs(1), Duration::from_millis(1));
            let origin = rest.rect(self.screen, &shape).origin;
            let mut image = drifter_core::drawer::paint_flat(
                edge,
                self.screen.x.max(1.0) as u32,
                &shape,
                self.palette.bar,
                veil,
            );
            self.frosted(&mut image, origin, f64::from(veil) / 255.0);
            drifter_core::drawer::rim(
                &mut image,
                edge,
                &shape,
                self.palette.rim,
                f64::from(self.palette.rim_alpha) / 255.0,
            );
            match upload(&mut self.renderer, Some(&image), "frosted drawer") {
                Ok(Some(buffer)) => built.push(buffer),
                _ => return,
            }
        }
        if let Ok(panels) = <[TextureBuffer<Tex>; 2]>::try_from(built) {
            if let Some(head) = self.heads.get_mut(self.at) {
                head.panels = panels.clone();
            }
            self.panels = panels;
        }
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
