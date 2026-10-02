use super::*;

impl State {
    pub(super) fn lock_elements(&self) -> Vec<Skin> {
        let Some(lock) = self.lock.as_ref() else {
            return Vec::new();
        };
        let rect = drifter_core::lock::Look::default().rect(self.screen);
        match lock.art.as_ref() {
            Some(art) => vec![Skin::Texture(element(art, rect, 1.0))],
            None => Vec::new(),
        }
    }

    pub(super) fn lock_now(&mut self) -> bool {
        use smithay::input::pointer::MotionEvent;
        use smithay::utils::SERIAL_COUNTER;

        if self.lock.is_some() {
            return true;
        }
        let user = match crate::auth::ready() {
            Ok(user) => user,
            Err(error) => {
                eprintln!("drifter: cannot lock: {error}");
                return false;
            }
        };

        self.dismiss_popups();
        self.popover = None;
        self.switch = None;
        self.tip = None;
        self.tip_of = None;
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

        self.lock = Some(Lock {
            pad: Pad::new(),
            note: Note::Ask,
            checking: None,
            ticket: None,
            art: None,
            rect: Rect::new(Vec2::ZERO, Vec2::ZERO),
            shown: (String::new(), String::new(), usize::MAX, Note::Ask),
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

    pub(super) fn paint_lock(&mut self) {
        let Some(lock) = self.lock.as_ref() else {
            return;
        };
        let clock = drifter_core::tray::now("%I:%M %p").unwrap_or_default();
        let date = drifter_core::tray::now("%A, %B %d").unwrap_or_default();
        let key = (clock, date, lock.pad.len(), lock.note);
        if lock.art.is_some() && lock.shown == key {
            return;
        }

        let look = drifter_core::lock::Look::default();
        let words = drifter_core::lock::Words {
            clock: &key.0,
            date: &key.1,
        };
        let image = drifter_core::lock::paint(
            &look,
            self.lock_big.as_ref(),
            self.lock_small.as_ref(),
            &words,
            key.2,
            key.3,
        );
        let rect = look.rect(self.screen);
        let art = match upload(&mut self.renderer, Some(&image), "lock") {
            Ok(art) => art,
            Err(error) => {
                eprintln!("drifter: {error}");
                None
            }
        };
        if let Some(lock) = self.lock.as_mut() {
            lock.art = art;
            lock.rect = rect;
            lock.shown = key;
        }
    }

    pub(super) fn locked_input(&mut self, event: InputEvent<LibinputInputBackend>) {
        let InputEvent::Keyboard { event } = event else {
            return;
        };
        let key = self.read_key(&event);
        if event.state() != KeyState::Pressed {
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
            LockKey::Other => {}
        }
        self.paint_lock();
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
                let key = match symbol.raw() {
                    keysyms::KEY_Return | keysyms::KEY_KP_Enter => LockKey::Enter,
                    keysyms::KEY_BackSpace => LockKey::Back,
                    keysyms::KEY_Escape => LockKey::Clear,
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
                eprintln!("drifter: the lock cannot check passwords: {error}");
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

pub(super) fn lock_marker() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR").map(|dir| PathBuf::from(dir).join("drifter-locked"))
}
