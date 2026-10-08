use super::*;

impl State {
    pub(super) fn lock_presence(&mut self) -> Option<f64> {
        let span = LOCK_FADE.as_secs_f64();
        if let Some(lock) = self.lock.as_ref() {
            return Some(drifter_core::motion::smooth((lock.since.elapsed().as_secs_f64() / span).min(1.0)));
        }
        let (_, since) = self.unlocking.as_ref()?;
        let gone = since.elapsed().as_secs_f64() / span;
        if gone >= 1.0 {
            self.unlocking = None;
            return None;
        }
        Some(1.0 - drifter_core::motion::smooth(gone))
    }

    pub(super) fn lock_fading(&self) -> bool {
        self.unlocking.is_some() || self.lock.as_ref().is_some_and(|lock| lock.since.elapsed() < LOCK_FADE)
    }

    pub(super) fn lock_elements(&mut self, shown: f64) -> Vec<Skin> {
        let look = drifter_core::lock::Look::themed(&self.palette);
        let alpha = shown as f32;
        let drift = match self.lock_logo.pace > 0.0 {
            true => (1.0 - shown) * 18.0,
            false => 0.0,
        };
        let up = Vec2::new(0.0, -drift);
        let down = Vec2::new(0.0, drift);
        let moved = |rect: Rect, by: Vec2| Rect::new((rect.origin + by).round(), rect.size);
        let logo = moved(look.logo_rect(self.screen), down);
        let mut elements = Vec::new();
        let Some(lock) = self.lock.as_ref().or(self.unlocking.as_ref().map(|(lock, _)| lock)) else {
            return elements;
        };
        let card = moved(look.rect(self.screen), down);
        let blink = lock.typed.elapsed().as_millis() / drifter_core::lock::BLINK.as_millis();
        if blink % 2 == 0
            && lock.checking.is_none()
            && self.lock.is_some()
            && let Some((caret, _)) = lock.caret.as_ref()
        {
            let at = look.caret_rect(lock.pad.len());
            elements.push(Skin::Texture(fill(caret, Rect::new(card.origin + at.origin, at.size), alpha)));
        }
        if let Some(art) = lock.art.as_ref() {
            elements.push(Skin::Texture(element(art, card, alpha)));
        }
        if let Some((art, size)) = lock.corner.as_ref() {
            let spot = self.tray.status_rect(self.top_rest_band(), *size);
            if let Some(glow) = lock.corner_glow.as_ref() {
                let ebb = drifter_core::lock::ebb(self.ebb_from.elapsed().as_secs_f64());
                elements.push(Skin::Texture(element(glow, spot, ebb * alpha)));
            }
            elements.push(Skin::Texture(element(art, spot, alpha)));
        }
        let top = lock
            .top
            .as_ref()
            .map(|(art, size)| (art.clone(), moved(look.top_rect(self.screen, *size), up)));
        let clock = lock
            .clock
            .as_ref()
            .map(|(art, size)| (art.clone(), moved(look.clock_rect(self.screen, *size), up)));
        let shade = lock.shade.clone();
        let veil = lock.veil.as_ref().map(|(art, _)| art.clone());
        if let Some(frame) = self.lock_logo_frame(logo) {
            elements.push(Skin::Texture(element(&frame, logo, alpha)));
        }
        if let Some(shade) = shade {
            let spot = moved(look.logo_shadow_rect(self.screen), down);
            elements.push(Skin::Texture(element(&shade, spot, alpha)));
        }
        if let Some((art, rect)) = clock {
            elements.push(Skin::Texture(element(&art, rect, alpha)));
        }
        if let Some((art, rect)) = top {
            elements.push(Skin::Texture(element(&art, rect, alpha)));
        }
        if let Some(veil) = veil {
            elements.push(Skin::Texture(fill(&veil, Rect::new(Vec2::ZERO, self.screen), look.veil as f32 * alpha)));
        }
        let view = (self.mode, self.screen, self.world, self.camera);
        let clock = self.paper_clock();
        let every = self.live_every();
        if let Some(paper) = self.wallpaper.as_mut() {
            paper_pieces(&mut self.renderer, paper, view, alpha, (clock, Some(every)), &mut elements);
        }
        elements
    }

    pub(super) fn lock_now(&mut self) -> bool {
        if self.lock.is_some() {
            return true;
        }
        let user = match crate::auth::ready() {
            Ok(user) => user,
            Err(error) => {
                log!("drifter: cannot lock: {error}");
                return false;
            }
        };

        let refocus = self.hush();
        self.unlocking = None;
        self.lock = Some(Lock {
            since: Instant::now(),
            pad: Pad::new(),
            note: Note::Ask,
            checking: None,
            ticket: None,
            art: None,
            top: None,
            corner: None,
            veil: None,
            caret: None,
            typed: Instant::now(),
            corner_lane: worker::Lane::default(),
            corner_glow: None,
            glow_lane: worker::Lane::default(),
            clock: None,
            clock_lane: worker::Lane::default(),
            shade: None,
            shade_lane: worker::Lane::default(),
            shown: (String::new(), String::new(), usize::MAX, Note::Ask, [0; 3]),
            lane: worker::Lane::default(),
            top_lane: worker::Lane::default(),
            name: full_name(&user),
            user,
            refocus,
        });
        if let Some(marker) = lock_marker() {
            let _ = std::fs::write(marker, b"");
        }
        self.paint_lock();
        self.kick();
        true
    }

    fn hush(
        &mut self,
    ) -> Option<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface> {
        use smithay::input::pointer::MotionEvent;
        use smithay::utils::SERIAL_COUNTER;

        self.dismiss_popups();
        self.popover = None;
        self.switch = None;
        self.tip = None;
        self.tip_of = None;
        self.tip_want = None;
        self.zone = None;
        self.grab = Grab::None;
        self.held = None;
        self.dwell = None;
        self.leave = None;
        self.corner_since = None;
        self.hover_since = None;
        self.flight = None;
        self.modifier = false;
        self.fingers = 0;
        for edge in Edge::ALL {
            self.set(edge, false);
        }

        let keyboard = self.wayland.keyboard.clone();
        let refocus = self
            .focus
            .and_then(|id| self.place_of(id))
            .map(|index| self.windows[index].toplevel.wl_surface().clone())
            .or_else(|| keyboard.current_focus());
        keyboard.set_focus(self, None, SERIAL_COUNTER.next_serial());

        let pointer = self.wayland.pointer.clone();
        let world = self.in_world(self.pointer);
        pointer.motion(
            self,
            None,
            &MotionEvent {
                location: smithay::utils::Point::from((world.x, world.y)),
                serial: SERIAL_COUNTER.next_serial(),
                time: self.clock_ms(),
            },
        );
        pointer.frame(self);
        refocus
    }

    pub fn client_locks(&mut self, locker: smithay::wayland::session_lock::SessionLocker) {
        use smithay::reexports::wayland_server::Resource;

        if self
            .client_lock
            .as_ref()
            .is_some_and(|held| held.lock.is_alive())
        {
            return;
        }
        let refocus = match self.client_lock.take() {
            Some(old) => old.refocus,
            None => self.hush(),
        };
        let lock = locker.ext_session_lock().clone();
        self.client_lock = Some(ClientLock {
            locker: Some(locker),
            lock,
            surfaces: Vec::new(),
            refocus,
        });
        self.kick();
    }

    pub fn client_lock_surface(
        &mut self,
        surface: smithay::wayland::session_lock::LockSurface,
        output: smithay::reexports::wayland_server::protocol::wl_output::WlOutput,
    ) {
        let Some(held) = self.client_lock.as_mut() else {
            return;
        };
        let size = self
            .heads
            .iter()
            .enumerate()
            .find(|(_, head)| head.output.owns(&output))
            .map(|(index, head)| match index == self.at {
                true => self.screen,
                false => head.screen,
            })
            .unwrap_or(self.screen);
        surface.with_pending_state(|state| {
            state.size = Some((size.x.round() as u32, size.y.round() as u32).into());
        });
        surface.send_configure();
        held.surfaces.push((surface, output));
        self.focus_client_lock();
        self.kick();
    }

    pub fn client_unlocks(&mut self) {
        use smithay::reexports::wayland_server::Resource;
        use smithay::utils::SERIAL_COUNTER;

        let Some(held) = self.client_lock.take() else {
            return;
        };
        if self.lock.is_none()
            && let Some(marker) = lock_marker()
        {
            let _ = std::fs::remove_file(marker);
        }
        let keyboard = self.wayland.keyboard.clone();
        let back = match self.lock.as_mut() {
            Some(own) => {
                own.refocus = own.refocus.take().or(held.refocus);
                None
            }
            None => held.refocus.filter(|surface| surface.is_alive()),
        };
        keyboard.set_focus(self, back, SERIAL_COUNTER.next_serial());
        self.last = Instant::now();
        self.kick();
    }

    pub(super) fn check_client_lock(&mut self) {
        use smithay::reexports::wayland_server::Resource;

        let Some(held) = self.client_lock.as_ref() else {
            return;
        };
        if held.lock.is_alive() {
            return;
        }
        log!("drifter: the screen locker quit without unlocking, so Drifter locks instead");
        let refocus = self.client_lock.take().and_then(|held| held.refocus);
        self.wayland.extras.session_lock =
            smithay::wayland::session_lock::SessionLockManagerState::new::<State, _>(
                &self.wayland.handle,
                |_| false,
            );
        self.lock_now();
        if let Some(own) = self.lock.as_mut() {
            own.refocus = own.refocus.take().or(refocus);
        }
    }

    pub(super) fn confirm_client_lock(&mut self) {
        let Some(locker) = self
            .client_lock
            .as_mut()
            .and_then(|held| held.locker.take())
        else {
            return;
        };
        locker.lock();
        if let Some(marker) = lock_marker() {
            let _ = std::fs::write(marker, b"");
        }
    }

    pub(super) fn focus_client_lock(&mut self) {
        use smithay::utils::SERIAL_COUNTER;

        let Some(surface) = self.client_lock_here(self.hand) else {
            return;
        };
        let keyboard = self.wayland.keyboard.clone();
        if keyboard.current_focus().as_ref() != Some(&surface) {
            keyboard.set_focus(self, Some(surface), SERIAL_COUNTER.next_serial());
        }
    }

    fn client_lock_here(
        &self,
        index: usize,
    ) -> Option<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface> {
        let held = self.client_lock.as_ref()?;
        let output = match index == self.at {
            true => &self.output,
            false => &self.heads.get(index)?.output,
        };
        let alive = || held.surfaces.iter().filter(|(surface, _)| surface.alive());
        let (mine, _) = alive()
            .find(|(_, wl)| output.owns(wl))
            .or_else(|| alive().next())?;
        Some(mine.wl_surface().clone())
    }

    pub(super) fn client_lock_hit(&self) -> Option<Hit> {
        let surface = self.client_lock_here(self.at)?;
        let world = self.in_world(self.pointer);
        Some(Hit {
            index: None,
            layer: true,
            surface,
            origin: world - self.pointer,
            popup: false,
        })
    }

    pub(super) fn client_lock_elements(&mut self, elements: &mut Vec<Skin>) {
        let Some(surface) = self.client_lock_here(self.at) else {
            return;
        };
        let anchor = smithay::utils::Point::<i32, smithay::utils::Physical>::from((0, 0));
        let bare: Vec<WaylandSurfaceRenderElement<Painter>> = render_elements_from_surface_tree(
            &mut self.renderer,
            &surface,
            anchor,
            1.0,
            1.0,
            Kind::Unspecified,
        );
        elements.extend(
            bare.into_iter()
                .map(|piece| Skin::Client(RescaleRenderElement::from_element(piece, anchor, 1.0))),
        );
    }

    pub(super) fn paint_lock(&mut self) {
        let Some(lock) = self.lock.as_ref() else {
            return;
        };
        let clock = drifter_core::tray::now("%-I:%M").unwrap_or_default();
        let date = drifter_core::tray::now("%A, %B %-d").unwrap_or_default();
        let look = drifter_core::lock::Look::themed(&self.palette);
        let frosting = self.frosting();
        let rect = look.rect(self.screen);
        let login = worker::contrast(&frosting, look, rect);
        let key = (clock, date, lock.pad.len(), lock.note, login.ink);
        let shown = lock.shown.clone();
        let fresh = lock.art.is_some() && lock.top.is_some() && lock.corner.is_some() && lock.clock.is_some();
        let shaded = lock.shade.is_some();
        let pixel = |color: [u8; 3]| drifter_core::Image {
            width: 1,
            height: 1,
            rgba: vec![color[0], color[1], color[2], 255],
        };
        if lock.veil.as_ref().is_none_or(|(_, color)| *color != look.back)
            && let Ok(Some(art)) = upload(&mut self.renderer, Some(&pixel(look.back)), "lock veil")
            && let Some(lock) = self.lock.as_mut()
        {
            lock.veil = Some((art, look.back));
        }
        if self.lock.as_ref().is_some_and(|lock| lock.caret.as_ref().is_none_or(|(_, color)| *color != login.ink))
            && let Ok(Some(art)) = upload(&mut self.renderer, Some(&pixel(login.ink)), "lock caret")
            && let Some(lock) = self.lock.as_mut()
        {
            lock.caret = Some((art, login.ink));
        }
        if fresh && shown == key {
            return;
        }
        let (big, small) = (self.lock_big.clone(), self.lock_small.clone());
        let clock_moved = !fresh || shown.0 != key.0 || shown.4 != key.4;
        if !fresh || (shown.0, shown.1, shown.4) != (key.0.clone(), key.1.clone(), key.4) {
            let words = (key.0.clone(), key.1.clone());
            let date = self.lock_name.clone();
            let corner_font = small.clone();
            let screen = self.screen;
            let tray = self.tray;
            let show = self.show_battery;
            let corner_look = worker::contrast(&frosting, look, tray.status_rect(self.top_rest_band(), Vec2::new(160.0, 28.0)));
            let date_look = worker::contrast(&frosting, look, look.top_rect(screen, Vec2::new(screen.x * 0.25, 40.0)));
            self.paint_in_lane_sized(
                "lock corner",
                |state| state.lock.as_mut().map(|lock| &mut lock.corner_lane),
                State::paint_lock,
                move || {
                    let battery = crate::stats::battery()
                        .map(|(label, percent)| (label == "CHG", percent))
                        .filter(|_| show);
                    Some(drifter_core::lock::paint_corner(&corner_look, corner_font.as_deref(), battery, &tray))
                },
                |state, art, size| {
                    if let Some(lock) = state.lock.as_mut() {
                        lock.corner = Some((art, size));
                    }
                },
            );
            if let Some(lock) = self.lock.as_mut() {
                lock.corner_glow = None;
            }
            let glow_font = small.clone();
            self.paint_in_lane(
                "lock corner glow",
                |state| state.lock.as_mut().map(|lock| &mut lock.glow_lane),
                |_| {},
                move || {
                    let battery = crate::stats::battery()
                        .map(|(label, percent)| (label == "CHG", percent))
                        .filter(|(charging, _)| show && *charging)?;
                    Some(drifter_core::lock::paint_corner_charge(&corner_look, glow_font.as_deref(), Some(battery), &tray))
                },
                |state, art| {
                    if let Some(lock) = state.lock.as_mut() {
                        lock.corner_glow = Some(art);
                    }
                },
            );
            self.paint_in_lane_sized(
                "lock clock",
                |state| state.lock.as_mut().map(|lock| &mut lock.top_lane),
                State::paint_lock,
                move || {
                    Some(drifter_core::lock::paint_top(&date_look, date.as_deref(), &words.1))
                },
                |state, art, size| {
                    if let Some(lock) = state.lock.as_mut() {
                        lock.top = Some((art, size));
                    }
                },
            );
        }
        if clock_moved {
            self.paint_lock_clock(key.0.clone(), big.clone(), look);
        }
        if !shaded {
            self.paint_in_lane(
                "lock shade",
                |state| state.lock.as_mut().map(|lock| &mut lock.shade_lane),
                |_| {},
                move || Some(drifter_core::lock::paint_logo_shadow(&look)),
                |state, art| {
                    if let Some(lock) = state.lock.as_mut() {
                        lock.shade = Some(art);
                    }
                },
            );
        }
        let name = lock_name(self);
        let named = self.lock_name.clone();
        let painted = key.clone();
        let look = login;
        self.paint_in_lane(
            "lock",
            |state| state.lock.as_mut().map(|lock| &mut lock.lane),
            State::paint_lock,
            move || {
                let mut image = drifter_core::lock::paint(&look, small.as_deref(), named.as_deref(), &name, painted.2, painted.3);
                worker::frost(&frosting, &mut image, rect.origin, look.pill);
                Some(drifter_core::lock::with_shadow(&look, &image))
            },
            move |state, art| {
                if let Some(lock) = state.lock.as_mut() {
                    lock.art = Some(art);
                    lock.shown = key;
                }
            },
        );
    }

    pub(super) fn locked_input(&mut self, event: InputEvent<LibinputInputBackend>) {
        use smithay::backend::input::{ButtonState, Event, PointerButtonEvent};
        let edge = self.screen - Vec2::splat(1.0);
        let event = match event {
            InputEvent::PointerMotion { event } => {
                let delta = Vec2::new(event.delta_x(), event.delta_y());
                self.pointer = (self.pointer + delta).clamp(Vec2::ZERO, edge);
                self.repaint_popover();
                self.kick();
                return;
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let x = event.x_transformed(self.screen.x as i32);
                let y = event.y_transformed(self.screen.y as i32);
                self.pointer = Vec2::new(x, y).clamp(Vec2::ZERO, edge);
                self.repaint_popover();
                self.kick();
                return;
            }
            InputEvent::PointerButton { event } => {
                let _ = event.time_msec();
                if event.state() == ButtonState::Pressed {
                    self.locked_press();
                }
                return;
            }
            InputEvent::Keyboard { event } => event,
            _ => return,
        };
        let key = self.read_key(&event);
        if event.state() != KeyState::Pressed {
            return;
        }
        if let LockKey::Vt(vt) = key {
            self.change_vt(vt);
            return;
        }
        if self.alert.is_some() || self.popover.is_some() {
            match key {
                LockKey::Enter if self.alert.is_some() => self.desk_menu_key(LockKey::Enter),
                LockKey::Clear => {
                    self.alert = None;
                    self.popover = None;
                    self.kick_now();
                }
                _ => {}
            }
            return;
        }
        if let Some(lock) = self.lock.as_mut() {
            lock.typed = Instant::now();
        }
        let Some(lock) = self.lock.as_mut() else {
            return;
        };
        if let Some(since) = lock.checking
            && since.elapsed() < LOCK_PATIENCE
        {
            return;
        }

        match key {
            LockKey::Letter(letter) => {
                if lock.pad.push(letter) {
                    lock.note = Note::Ask;
                }
            }
            LockKey::Back => {
                lock.pad.backspace();
                lock.note = Note::Ask;
            }
            LockKey::Clear => {
                lock.pad.wipe();
                lock.note = Note::Ask;
            }
            LockKey::Enter => {
                if lock.pad.is_empty() {
                    return;
                }
                let mut typed = lock.pad.take();
                let secret = std::mem::take(&mut *typed);
                self.tickets += 1;
                let ticket = self.tickets;
                lock.checking = Some(Instant::now());
                lock.ticket = Some(ticket);
                lock.note = Note::Checking;
                let user = lock.user.clone();
                let sender = self.verify.clone();
                std::thread::spawn(move || {
                    let verdict = crate::auth::verify(&user, secret);
                    let _ = sender.send((ticket, verdict));
                });
            }
            LockKey::Other
            | LockKey::Up
            | LockKey::Down
            | LockKey::Left
            | LockKey::Right
            | LockKey::Vt(_) => {}
        }
        self.paint_lock();
    }

    pub(super) fn change_vt(&mut self, vt: i32) {
        let Some(session) = self.vt.as_mut() else {
            return;
        };
        if let Err(error) = session.change_vt(vt) {
            log!("drifter: cannot switch to terminal {vt}: {error}");
        }
    }

    pub(super) fn read_key(
        &mut self,
        event: &impl KeyboardKeyEvent<LibinputInputBackend>,
    ) -> LockKey {
        use smithay::input::keyboard::{FilterResult, keysyms};
        use smithay::utils::SERIAL_COUNTER;

        let keyboard = self.wayland.keyboard.clone();
        let read = keyboard.input::<LockKey, _>(
            self,
            event.key_code(),
            event.state(),
            SERIAL_COUNTER.next_serial(),
            event.time_msec(),
            |_, _, handle| {
                let symbol = handle.modified_sym();
                if let Some(vt) = drifter_core::shortcut::vt_number(symbol.raw()) {
                    return FilterResult::Intercept(LockKey::Vt(vt));
                }
                let key = match symbol.raw() {
                    keysyms::KEY_Return | keysyms::KEY_KP_Enter => LockKey::Enter,
                    keysyms::KEY_BackSpace => LockKey::Back,
                    keysyms::KEY_Escape => LockKey::Clear,
                    keysyms::KEY_Up => LockKey::Up,
                    keysyms::KEY_Down => LockKey::Down,
                    keysyms::KEY_Left => LockKey::Left,
                    keysyms::KEY_Right => LockKey::Right,
                    _ => match symbol.key_char() {
                        Some(letter) => LockKey::Letter(letter),
                        None => LockKey::Other,
                    },
                };
                FilterResult::Intercept(key)
            },
        );
        read.unwrap_or(LockKey::Other)
    }

    pub(super) fn verdict(&mut self, ticket: u64, verdict: crate::auth::Verdict) {
        use crate::auth::Verdict;

        let Some(lock) = self.lock.as_mut() else {
            return;
        };
        if lock.ticket != Some(ticket) {
            return;
        }
        lock.checking = None;
        lock.ticket = None;
        match verdict {
            Verdict::Granted => {
                self.unlock();
                return;
            }
            Verdict::Denied => lock.note = Note::Wrong,
            Verdict::Broken(error) => {
                log!("drifter: the lock cannot check passwords: {error}");
                lock.note = Note::Wrong;
            }
        }
        self.paint_lock();
    }

    pub(super) fn unlock(&mut self) {
        use smithay::reexports::wayland_server::Resource;
        use smithay::utils::SERIAL_COUNTER;

        let Some(mut lock) = self.lock.take() else {
            return;
        };
        if let Some(marker) = lock_marker() {
            let _ = std::fs::remove_file(marker);
        }
        lock.pad.wipe();
        if let Some(surface) = lock.refocus.take()
            && surface.is_alive()
        {
            let keyboard = self.wayland.keyboard.clone();
            keyboard.set_focus(self, Some(surface), SERIAL_COUNTER.next_serial());
        }
        self.last = Instant::now();
        self.unlocking = Some((lock, Instant::now()));
        for edge in Edge::ALL {
            self.drawers[edge.index()].open = true;
        }
        self.dwell = None;
        self.leave = Some(Instant::now() + BARS_AFTER_UNLOCK);
        self.kick_now();
    }
}

impl State {
    fn paint_lock_clock(&mut self, clock: String, font: Option<std::sync::Arc<Font>>, look: drifter_core::lock::Look) {
        let Some(font) = font else {
            return;
        };
        let screen = self.screen;
        let frosting = self.frosting();
        let near = look.clock_rect(screen, Vec2::new(screen.x * 0.3, screen.y * 0.2));
        let look = worker::contrast(&frosting, look, near);
        self.paint_in_lane_sized(
            "lock clock",
            |state| state.lock.as_mut().map(|lock| &mut lock.clock_lane),
            |_| {},
            move || {
                let mut frosted = drifter_core::lock::frosted_clock(&font, &clock, look.dark_ink());
                let rect = look.clock_rect(screen, drifter_core::paint::logical(&frosted.body));
                worker::frost(&frosting, &mut frosted.body, rect.origin, drifter_core::lock::TINT);
                Some(drifter_core::lock::finish_clock(&frosted))
            },
            |state, art, size| {
                if let Some(lock) = state.lock.as_mut() {
                    lock.clock = Some((art, size));
                }
            },
        );
    }

    fn locked_press(&mut self) {
        if self.alert.is_some() {
            self.press_desk_menu();
            return;
        }
        if self.popover.is_some() {
            self.press_popover();
            self.kick_now();
            return;
        }
        let look = drifter_core::lock::Look::themed(&self.palette);
        let Some(size) = self.lock.as_ref().and_then(|lock| lock.corner.as_ref()).map(|(_, size)| *size) else {
            return;
        };
        let _ = (look, size);
        let power = self.tray.power_rect(self.top_rest_band());
        if power.contains(self.pointer) {
            use drifter_core::menu::Item;
            use drifter_core::power::Action;
            let items = vec![
                Item::line(Action::Suspend.id(), drifter_core::lang::t("Sleep")),
                Item::line(Action::Reboot.id(), drifter_core::lang::t("Restart…")),
                Item::line(Action::Shutdown.id(), drifter_core::lang::t("Shut Down…")),
            ];
            let at = Vec2::new(power.max().x, power.max().y + 6.0);
            self.open_popover(items, at, Target::Desktop);
            self.kick_now();
        }
    }
}

fn lock_name(state: &State) -> String {
    state.lock.as_ref().map(|lock| lock.name.clone()).unwrap_or_default()
}

fn full_name(login: &std::ffi::CStr) -> String {
    let login = login.to_string_lossy().into_owned();
    std::fs::read_to_string("/etc/passwd")
        .ok()
        .and_then(|passwd| {
            passwd.lines().find_map(|line| {
                let fields: Vec<&str> = line.split(':').collect();
                (fields.first() == Some(&login.as_str()))
                    .then(|| fields.get(4).and_then(|gecos| gecos.split(',').next()).unwrap_or("").trim().to_string())
            })
        })
        .filter(|full| !full.is_empty())
        .unwrap_or(login)
}

pub(super) fn lock_marker() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|dir| PathBuf::from(dir).join("drifter-locked"))
}
