use super::*;

impl State {
    pub(super) fn on_screen(&self, world: Rect) -> Rect {
        let origin = self.camera.screen_from_world(world.origin, self.screen);
        Rect::new(origin, world.size * self.camera.zoom)
    }

    pub(super) fn in_world(&self, screen: Vec2) -> Vec2 {
        self.camera.world_from_screen(screen, self.screen)
    }

    pub(super) fn fly_to(&mut self, target: Camera) {
        let mut target = target;
        target.clamp_to_world(self.world, self.screen);
        self.flight = Some((
            Flight::new(self.camera, target, self.tunables.camera_anim),
            Instant::now(),
        ));
    }

    pub(super) fn steer(&mut self) -> bool {
        let Some((flight, since)) = self.flight else {
            return false;
        };
        let gone = Instant::now().duration_since(since);
        self.camera = flight.within(gone, self.world, self.screen);
        if flight.done(gone) {
            self.camera = flight.to;
            self.flight = None;
            return false;
        }
        true
    }

    pub(super) fn overview(&self) -> bool {
        self.camera.zoom < Camera::WORK_ZOOM - 1e-6
    }

    pub(super) fn watch_corner(&mut self) {
        if !self.tunables.hot_corner
            || self.modifier
            || self.lock.is_some()
            || self.held_by_fullscreen().is_some()
        {
            self.corner_since = None;
            return;
        }
        let size = self.tunables.hot_corner_size;
        let on_tray = self.tray_band().is_some_and(|band| {
            self.tray
                .icon_at(band, self.pointer, self.tray_room(band))
                .is_some()
        });
        let in_corner =
            self.pointer.x < size && self.pointer.y < size && !on_tray && self.popover.is_none();

        if !in_corner {
            self.corner_since = None;
            return;
        }
        if self.overview() || self.flight.is_some() {
            self.corner_since = None;
            return;
        }
        match self.corner_since {
            None => self.corner_since = Some(Instant::now()),
            Some(since) => {
                if Instant::now().duration_since(since) >= self.tunables.hot_corner_dwell {
                    self.corner_since = None;
                    self.fly_out();
                }
            }
        }
    }

    pub(super) fn fly_out(&mut self) {
        let framed: Vec<Rect> = self
            .windows
            .iter()
            .filter(|window| window.placed && !window.hidden)
            .map(|window| self.chrome.layout(window.content).frame)
            .collect();
        let target = Camera::showing(&framed, self.screen, self.chrome.title * 2.0, self.world);
        self.fly_to(target);
    }

    pub(super) fn three_tap(&mut self) {
        if self.flight.is_some() {
            return;
        }
        self.corner_since = None;
        if !self.press_overview() {
            self.fly_out();
        }
    }

    pub(super) fn press_overview(&mut self) -> bool {
        if !self.overview() {
            return false;
        }
        let target = match self.under_pointer() {
            Some((index, _)) => self.windows[index].content.center(),
            None => self.in_world(self.pointer),
        };
        self.fly_to(Camera::at_work_zoom(target));
        true
    }

    pub(super) fn zoom_by(&mut self, steps: f64) {
        if self.held_by_fullscreen().is_some() {
            return;
        }
        let least = Camera::fit_zoom(self.screen, self.world);
        let wanted = (self.camera.zoom * (1.0 + steps * 0.12)).clamp(least, 4.0);
        let mut camera = self.camera;
        camera.zoom_at(self.pointer, wanted, self.screen);
        camera.clamp_to_world(self.world, self.screen);
        self.camera = camera;
        self.flight = None;
    }
}
