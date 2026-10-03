use super::*;

impl State {
    pub(super) fn watch_wallpaper(&mut self) {
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
        }
        self.paper.shown = Some(path);
        self.kick_now();
    }

    pub(super) fn evict_tiles(&mut self) {
        if let Some(paper) = self.wallpaper.as_mut() {
            paper
                .resident
                .retain(|_, (_, used)| used.elapsed() < TILE_KEEP);
        }
    }
}
