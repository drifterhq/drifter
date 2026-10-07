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
        let on_tray = self
            .tray_band()
            .is_some_and(|band| self.tray_icon_at(band).is_some());
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
        self.welcome_did(drifter_core::welcome::Lesson::Overview);
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

    pub(super) fn frame_under_pointer(&self) -> Option<usize> {
        let world = self.in_world(self.pointer);
        self.windows
            .iter()
            .enumerate()
            .rev()
            .find(|(_, window)| {
                window.placed
                    && !window.hidden
                    && !window.going
                    && self.chrome.layout(window.content).frame.contains(world)
            })
            .map(|(index, _)| index)
    }

    pub(super) fn press_overview_button(&mut self, code: u32) -> bool {
        if !self.overview() {
            return false;
        }
        if code == BTN_LEFT
            && self.flight.is_none()
            && let Some(index) = self.frame_under_pointer()
            && self.windows[index].full.is_none()
        {
            let window = &self.windows[index];
            self.overview_hold = Some(OverviewHold {
                id: window.id,
                from: self.pointer,
                offset: self.in_world(self.pointer) - window.content.origin,
                moved: false,
            });
            return true;
        }
        self.press_overview()
    }

    pub(super) fn drag_overview(&mut self) -> bool {
        let Some(mut hold) = self.overview_hold else {
            return false;
        };
        if !hold.moved && (self.pointer - hold.from).length() < OVERVIEW_SLOP {
            return true;
        }
        hold.moved = true;
        self.overview_hold = Some(hold);
        let world = self.world;
        let wanted = self.in_world(self.pointer) - hold.offset;
        if let Some(window) = self.windows.iter_mut().find(|window| window.id == hold.id) {
            let moved = Rect::new(wanted.round(), window.content.size).inside(world);
            window.content.origin = moved.origin;
            window.touched = true;
        }
        self.kick_now();
        true
    }

    pub(super) fn release_overview(&mut self) -> bool {
        let Some(hold) = self.overview_hold.take() else {
            return false;
        };
        let Some(index) = self.place_of(hold.id) else {
            return true;
        };
        match hold.moved {
            true => {
                self.raise(index);
            }
            false => {
                let center = self.windows[index].content.center();
                self.fly_to(Camera::at_work_zoom(center));
            }
        }
        self.kick_now();
        true
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
        self.welcome_did(drifter_core::welcome::Lesson::Zoom);
    }
}
