use super::*;

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

    pub(super) fn load_wallpaper(&self, path: PathBuf) {
        let reply = self.paper_tx.clone();
        let (world, screen) = (self.world, self.screen);
        let spawned = std::thread::Builder::new()
            .name("drifter-wallpaper".to_string())
            .spawn(
                move || match drifter_core::image::load_covering(&path, world.x, world.y) {
                    Ok(image) => {
                        let pyramid = drifter_core::tiles::Pyramid::build(image, world, screen);
                        let _ = reply.send((path, Some(pyramid)));
                    }
                    Err(error) => {
                        eprintln!("drifter: no wallpaper {}: {error}", path.display());
                        let _ = reply.send((path, None));
                    }
                },
            );
        if let Err(error) = spawned {
            eprintln!("drifter: cannot load the wallpaper: {error}");
        }
    }

    pub(super) fn take_wallpaper(
        &mut self,
        path: PathBuf,
        pyramid: Option<drifter_core::tiles::Pyramid>,
    ) {
        if let Some(pyramid) = pyramid {
            self.wallpaper = Some(Backdrop::new(pyramid));
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
        let laid = drifter_core::wallpaper::place(self.mode, size, self.screen, self.world, &self.camera);
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
            let mut image = drifter_core::drawer::paint(
                edge,
                self.screen.x.max(1.0) as u32,
                &shape,
                self.palette.bar,
                veil,
            );
            self.frosted(&mut image, origin, f64::from(veil) / 255.0);
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

    pub(super) fn evict_tiles(&mut self) {
        if let Some(paper) = self.wallpaper.as_mut() {
            paper
                .resident
                .retain(|_, (_, used)| used.elapsed() < TILE_KEEP);
        }
    }
}
