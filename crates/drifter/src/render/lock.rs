use super::*;

impl State {
    pub(super) fn lock_elements(&mut self) -> Vec<Skin> {
        let look = drifter_core::lock::Look::themed(&self.palette);
        let logo = look.logo_rect(self.screen);
        let mut elements = Vec::new();
        let Some(lock) = self.lock.as_ref() else {
            return elements;
        };
        if let Some(art) = lock.art.as_ref() {
            elements.push(Skin::Texture(element(art, look.rect(self.screen), 1.0)));
        }
        let top = lock
            .top
            .as_ref()
            .map(|(art, size)| (art.clone(), look.top_rect(self.screen, *size)));
        let veil = lock.veil.as_ref().map(|(art, _)| art.clone());
        if let Some(frame) = self.lock_logo_frame(logo) {
            elements.push(Skin::Texture(element(&frame, logo, 1.0)));
        }
        if let Some((art, rect)) = top {
            elements.push(Skin::Texture(element(&art, rect, 1.0)));
        }
        if let Some(veil) = veil {
            elements.push(Skin::Texture(fill(&veil, Rect::new(Vec2::ZERO, self.screen), look.veil as f32)));
        }
        let view = (self.mode, self.screen, self.world, self.camera);
        let clock = self.paper_clock();
        let every = self.live_every();
        if let Some(paper) = self.wallpaper.as_mut() {
            paper_pieces(&mut self.renderer, paper, view, 1.0, (clock, Some(every)), &mut elements);
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
        self.lock = Some(Lock {
            pad: Pad::new(),
            note: Note::Ask,
            checking: None,
            ticket: None,
            art: None,
            top: None,
            veil: None,
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
        self.gauge_panel = None;
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
                time: self.started.elapsed().as_millis() as u32,
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
        let key = (clock, date, lock.pad.len(), lock.note, look.ink);
        let shown = lock.shown.clone();
        let fresh = lock.art.is_some() && lock.top.is_some();
        if lock.veil.as_ref().is_none_or(|(_, color)| *color != look.back) {
            let pixel = drifter_core::Image {
                width: 1,
                height: 1,
                rgba: vec![look.back[0], look.back[1], look.back[2], 255],
            };
            if let Ok(Some(art)) = upload(&mut self.renderer, Some(&pixel), "lock veil")
                && let Some(lock) = self.lock.as_mut()
            {
                lock.veil = Some((art, look.back));
            }
        }
        if fresh && shown == key {
            return;
        }
        let (big, small) = (self.lock_big.clone(), self.lock_small.clone());
        if !fresh || (shown.0, shown.1, shown.4) != (key.0.clone(), key.1.clone(), key.4) {
            let words = (key.0.clone(), key.1.clone());
            let (big, small) = (big.clone(), small.clone());
            self.paint_in_lane_sized(
                "lock clock",
                |state| state.lock.as_mut().map(|lock| &mut lock.top_lane),
                State::paint_lock,
                move || {
                    let words = drifter_core::lock::Words {
                        clock: &words.0,
                        date: &words.1,
                    };
                    Some(drifter_core::lock::paint_top(&look, big.as_deref(), small.as_deref(), &words))
                },
                |state, art, size| {
                    if let Some(lock) = state.lock.as_mut() {
                        lock.top = Some((art, size));
                    }
                },
            );
        }
        let name = lock_name(self);
        let named = self.lock_name.clone();
        let rect = look.rect(self.screen);
        let frosting = self.frosting();
        let painted = key.clone();
        self.paint_in_lane(
            "lock",
            |state| state.lock.as_mut().map(|lock| &mut lock.lane),
            State::paint_lock,
            move || {
                let mut image = drifter_core::lock::paint(&look, small.as_deref(), named.as_deref(), &name, painted.2, painted.3);
                worker::frost(&frosting, &mut image, rect.origin, look.pill);
                Some(image)
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
        let InputEvent::Keyboard { event } = event else {
            return;
        };
        let key = self.read_key(&event);
        if event.state() != KeyState::Pressed {
            return;
        }
        if let LockKey::Vt(vt) = key {
            self.change_vt(vt);
            return;
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
