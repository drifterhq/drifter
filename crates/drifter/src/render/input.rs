use super::*;

impl State {
    pub(super) fn pointer_to(&mut self, position: Vec2) {
        let edge = self.screen - Vec2::splat(1.0);
        let inside = position.clamp(Vec2::ZERO, edge);
        if !self.modifier
            && inside != position
            && let Some(there) = self.cross(position)
        {
            let edge = self.screen - Vec2::splat(1.0);
            self.pointer = there.clamp(Vec2::ZERO, edge);
            return self.pointer_to(self.pointer);
        }
        let was = self.pointer;

        if self.modifier {
            let band = PUSH_BAND;
            let moved = position - was;
            let mut pan = position - inside;
            if inside.x <= band && moved.x < 0.0 {
                pan.x = moved.x;
            }
            if inside.x >= edge.x - band && moved.x > 0.0 {
                pan.x = moved.x;
            }
            if inside.y <= band && moved.y < 0.0 {
                pan.y = moved.y;
            }
            if inside.y >= edge.y - band && moved.y > 0.0 {
                pan.y = moved.y;
            }

            if pan != Vec2::ZERO && self.held_by_fullscreen().is_none() {
                let mut camera = self.camera;
                camera.pan_by_screen(pan * self.tunables.pan_scale);
                camera.clamp_to_world(self.world, self.screen);
                self.camera = camera;
                self.flight = None;
            }
            self.pointer = inside;
            self.dwell = None;
            return;
        }

        self.pointer = inside;

        if !matches!(self.grab, Grab::None) {
            self.dwell = None;
            return;
        }

        match self.wanted() {
            Some(edge) => {
                self.leave = None;
                if !self.drawers[edge.index()].open
                    && self.dwell.map(|(held, _)| held != edge).unwrap_or(true)
                {
                    self.dwell = Some((edge, Instant::now()));
                }
            }
            None => {
                self.dwell = None;
                if self.hiding_open() && self.leave.is_none() {
                    self.leave = Some(Instant::now());
                }
            }
        }
    }

    pub(super) fn under_pointer(&self) -> Option<(usize, Vec2)> {
        let world = self.in_world(self.pointer);
        self.windows
            .iter()
            .enumerate()
            .rev()
            .find(|(_, window)| window.placed && !window.hidden && window.content.contains(world))
            .map(|(index, window)| (index, world - window.content.origin))
    }

    pub(super) fn layer_hit(
        &self,
        kinds: &[smithay::wayland::shell::wlr_layer::Layer],
    ) -> Option<Hit> {
        use smithay::desktop::WindowSurfaceType;

        let map = smithay::desktop::layer_map_for_output(&self.output);
        let world = self.in_world(self.pointer);
        for kind in kinds {
            let Some(layer) = map.layer_under(*kind, (self.pointer.x, self.pointer.y)) else {
                continue;
            };
            let Some(place) = map.layer_geometry(layer) else {
                continue;
            };
            let at = Vec2::new(f64::from(place.loc.x), f64::from(place.loc.y));
            let local = self.pointer - at;
            if let Some((surface, inside)) =
                layer.surface_under((local.x, local.y), WindowSurfaceType::ALL)
            {
                let origin = at + Vec2::new(f64::from(inside.x), f64::from(inside.y));
                return Some(Hit {
                    index: None,
                    layer: true,
                    surface,
                    origin: world - (self.pointer - origin),
                    popup: false,
                });
            }
        }
        None
    }

    pub(super) fn surface_under(&self) -> Option<Hit> {
        use smithay::wayland::shell::wlr_layer::Layer;

        if self.client_lock.is_some() {
            return self.client_lock_hit();
        }
        if let Some(hit) = self.layer_hit(&[Layer::Overlay, Layer::Top]) {
            return Some(hit);
        }
        self.window_hit()
            .or_else(|| self.layer_hit(&[Layer::Bottom, Layer::Background]))
    }

    pub(super) fn window_hit(&self) -> Option<Hit> {
        let world = self.in_world(self.pointer);
        for (index, window) in self.windows.iter().enumerate().rev() {
            if !window.placed || window.hidden {
                continue;
            }
            let root = window.toplevel.wl_surface();
            for (origin, area, surface) in Self::popup_places(root, window.content) {
                if area.contains(world) {
                    return Some(Hit {
                        index: Some(index),
                        layer: false,
                        surface,
                        origin,
                        popup: true,
                    });
                }
            }
            if window.content.contains(world) {
                return Some(Hit {
                    index: Some(index),
                    layer: false,
                    surface: root.clone(),
                    origin: window.content.origin - window.inset,
                    popup: false,
                });
            }
        }
        None
    }

    pub(super) fn tell_pointer(&mut self, time: u32) {
        use smithay::input::pointer::MotionEvent;
        use smithay::utils::SERIAL_COUNTER;

        let focus = self.pointer_focus();
        if focus.is_none() || self.grip_under().is_some() {
            self.cursor_status = smithay::input::pointer::CursorImageStatus::default_named();
        }
        let pointer = self.wayland.pointer.clone();
        let world = self.in_world(self.pointer);
        let at = smithay::utils::Point::from((world.x, world.y));
        pointer.motion(
            self,
            focus,
            &MotionEvent {
                location: at,
                serial: SERIAL_COUNTER.next_serial(),
                time,
            },
        );
        pointer.frame(self);
        if let Some(surface) = pointer.current_focus() {
            smithay::wayland::pointer_constraints::with_pointer_constraint(
                &surface,
                &pointer,
                |constraint| {
                    if let Some(constraint) = constraint
                        && !constraint.is_active()
                    {
                        constraint.activate();
                    }
                },
            );
        }
    }

    pub(super) fn pointer_focus(
        &self,
    ) -> Option<(
        smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        smithay::utils::Point<f64, smithay::utils::Logical>,
    )> {
        if self.over_own_parts() {
            return None;
        }
        self.surface_under().map(|hit| {
            (
                hit.surface,
                smithay::utils::Point::from((hit.origin.x, hit.origin.y)),
            )
        })
    }

    pub(super) fn tell_relative(&mut self, delta: Vec2, bare: Vec2, utime: u64) {
        use smithay::input::pointer::RelativeMotionEvent;

        let focus = self.pointer_focus();
        let pointer = self.wayland.pointer.clone();
        pointer.relative_motion(
            self,
            focus,
            &RelativeMotionEvent {
                delta: (delta.x, delta.y).into(),
                delta_unaccel: (bare.x, bare.y).into(),
                utime,
            },
        );
        pointer.frame(self);
    }

    pub(super) fn held_pointer(&self) -> Option<Option<Rect>> {
        use smithay::wayland::pointer_constraints::{PointerConstraint, with_pointer_constraint};

        if self.modifier {
            return None;
        }
        let pointer = self.wayland.pointer.clone();
        let surface = pointer.current_focus()?;
        let locked = with_pointer_constraint(&surface, &pointer, |constraint| {
            let constraint = constraint.filter(|constraint| constraint.is_active())?;
            Some(matches!(&*constraint, PointerConstraint::Locked(_)))
        })?;
        if locked {
            return Some(None);
        }
        let window = self
            .windows
            .iter()
            .find(|window| window.toplevel.wl_surface() == &surface)?;
        Some(Some(self.on_screen(window.content)))
    }

    pub(super) fn grip_under(&self) -> Option<(usize, drifter_core::window::Grip)> {
        self.windows
            .iter()
            .enumerate()
            .rev()
            .filter(|(_, window)| {
                window.placed && !window.hidden && window.decorated && window.full.is_none()
            })
            .find_map(|(index, window)| {
                self.chrome
                    .grip_at(window.content, self.in_world(self.pointer), 3)
                    .map(|grip| (index, grip))
            })
    }

    pub(super) fn take_grab(&mut self) -> bool {
        use drifter_core::window::Grip;

        let Some((index, grip)) = self.grip_under() else {
            return false;
        };
        let index = self.focus_window(index);
        let window = &self.windows[index];
        let id = window.id;
        let content = window.content;

        match grip {
            Grip::Button(slot) => {
                self.held = Some(slot);
                self.press_button(index, self.chrome.order[slot]);
            }
            Grip::Title => {
                let now = Instant::now();
                let again = self
                    .last_title
                    .map(|(at, when)| {
                        (self.pointer - at).length() < 6.0
                            && now.duration_since(when) < DOUBLE_CLICK
                    })
                    .unwrap_or(false);
                if again {
                    self.last_title = None;
                    self.press_button(index, ChromeButton::Maximize);
                } else {
                    self.last_title = Some((self.pointer, now));
                    self.grab = Grab::Move {
                        id,
                        offset: self.in_world(self.pointer) - content.origin,
                    };
                }
            }
            other => {
                self.grab = Grab::Resize {
                    id,
                    grip: other,
                    from: self.in_world(self.pointer),
                    start: content,
                };
            }
        }
        true
    }

    pub fn start_move(&mut self, index: usize) {
        if self.lock.is_some() || self.windows[index].full.is_some() {
            return;
        }
        let index = self.raise(index);
        let window = &self.windows[index];
        self.grab = Grab::Move {
            id: window.id,
            offset: self.in_world(self.pointer) - window.content.origin,
        };
    }

    pub fn start_resize(
        &mut self,
        index: usize,
        edges: smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge,
    ) {
        if self.lock.is_some() || self.windows[index].full.is_some() {
            return;
        }
        use drifter_core::window::Grip;
        use smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::ResizeEdge as Side;

        let grip = match edges {
            Side::Left => Grip::Left,
            Side::Right => Grip::Right,
            Side::Top => Grip::Top,
            Side::TopLeft => Grip::TopLeft,
            Side::TopRight => Grip::TopRight,
            Side::Bottom => Grip::Bottom,
            Side::BottomLeft => Grip::BottomLeft,
            Side::BottomRight => Grip::BottomRight,
            _ => return,
        };
        let index = self.raise(index);
        let window = &self.windows[index];
        let start = window.content;
        self.grab = Grab::Resize {
            id: window.id,
            grip,
            from: self.in_world(self.pointer),
            start,
        };
    }

    pub(super) fn land(&mut self, index: usize, zone: Zone) {
        let view = self.camera.visible_rect(self.screen);
        let frame = self.snap.rect(zone, view);
        let chrome = self.chrome;
        let want = Rect::new(
            Vec2::new(frame.origin.x + chrome.edge, frame.origin.y + chrome.title),
            Vec2::new(
                (frame.size.x - chrome.edge * 2.0).max(80.0),
                (frame.size.y - chrome.title - chrome.edge).max(60.0),
            ),
        )
        .round_out();
        let Some(window) = self.windows.get_mut(index) else {
            return;
        };
        if window.restore.is_none() {
            window.restore = Some(window.content);
        }
        window.content = want;
        window.touched = true;
        window.toplevel.with_pending_state(|state| {
            state.size = Some((want.size.x as i32, want.size.y as i32).into());
        });
        window.toplevel.send_pending_configure();
    }

    pub(super) fn press_button(&mut self, index: usize, button: ChromeButton) {
        use wayland_protocols::xdg::shell::server::xdg_toplevel;

        match button {
            ChromeButton::Close => {
                let seat = self.dock_slot(0);
                let window = &mut self.windows[index];
                if !window.going {
                    window.going = true;
                    window.motion = Some(Motion::new(Phase::Closing, seat));
                }
            }
            ChromeButton::Minimize => {
                let seat = self.dock_slot(self.minimized().len());
                self.windows[index].motion = Some(Motion::new(Phase::Minimizing, seat));
                let id = self.windows[index].id;
                self.focus_next(Some(id));
            }
            ChromeButton::Maximize | ChromeButton::Restore => {
                let chrome = self.chrome;
                let zone =
                    smithay::desktop::layer_map_for_output(&self.output).non_exclusive_zone();
                let zone = Rect::new(
                    Vec2::new(f64::from(zone.loc.x), f64::from(zone.loc.y)),
                    Vec2::new(f64::from(zone.size.w), f64::from(zone.size.h)),
                );
                let view = Rect::new(self.in_world(zone.origin), zone.size / self.camera.zoom);
                let room = view.size.min(zone.size);
                let full = Rect::new(view.center() - room / 2.0, room);
                let window = &mut self.windows[index];
                let want = match window.restore.take() {
                    Some(back) => back,
                    None => {
                        window.restore = Some(window.content);
                        Rect::new(
                            Vec2::new(full.origin.x + chrome.edge, full.origin.y + chrome.title),
                            Vec2::new(
                                full.size.x - chrome.edge * 2.0,
                                full.size.y - chrome.title - chrome.edge,
                            ),
                        )
                        .round_out()
                    }
                };
                window.content = want;
                window.touched = true;
                let full = window.restore.is_some();
                window.toplevel.with_pending_state(|state| {
                    state.size = Some((want.size.x as i32, want.size.y as i32).into());
                    if full {
                        state.states.set(xdg_toplevel::State::Maximized);
                    } else {
                        state.states.unset(xdg_toplevel::State::Maximized);
                    }
                });
                window.toplevel.send_pending_configure();
            }
        }
    }

    pub(super) fn drag(&mut self) {
        if self.drag_gauge_panel() {
            return;
        }
        if self.drag_overview() {
            return;
        }
        match self.grab {
            Grab::None => {}
            Grab::Move { id, offset } => {
                let world = self.in_world(self.pointer);
                if let Some(window) = self.windows.iter_mut().find(|window| window.id == id) {
                    window.content.origin = (world - offset).round();
                    window.touched = true;
                }
                self.zone = match self.modifier {
                    true => None,
                    false => {
                        let view = self.camera.visible_rect(self.screen);
                        self.snap.zone_at(self.in_world(self.pointer), view)
                    }
                };
            }
            Grab::Resize {
                id,
                grip,
                from,
                start,
            } => {
                let chrome = self.chrome;
                let moved = self.in_world(self.pointer) - from;
                let least = Vec2::new(160.0, 80.0);
                let wanted = chrome.resize(start, grip, moved, least);
                if let Some(window) = self.windows.iter_mut().find(|window| window.id == id) {
                    window.touched = true;
                    window.toplevel.with_pending_state(|state| {
                        state.size = Some((wanted.size.x as i32, wanted.size.y as i32).into());
                    });
                    window.toplevel.send_pending_configure();
                }
            }
        }
    }

    pub(super) fn tell_button(&mut self, code: u32, state: ButtonState, time: u32) {
        use smithay::input::pointer::ButtonEvent;
        use smithay::utils::SERIAL_COUNTER;

        let serial = SERIAL_COUNTER.next_serial();
        if state == ButtonState::Released {
            self.release_gauge_panel();
            self.release_overview();
            if let (Grab::Move { id, .. }, Some(zone)) = (&self.grab, self.zone)
                && let Some(index) = self.place_of(*id)
            {
                self.land(index, zone);
            }
            self.zone = None;
            self.grab = Grab::None;
            self.held = None;
            if let Some(at) = self.eaten.iter().position(|eaten| *eaten == code) {
                self.eaten.swap_remove(at);
                self.kick();
                return;
            }
        }
        if state == ButtonState::Pressed {
            if self.press_own(code) {
                self.eaten.push(code);
                self.dismiss_popups();
                self.kick();
                return;
            }
            match self.surface_under() {
                Some(Hit {
                    index: Some(index),
                    popup: false,
                    ..
                }) => {
                    self.focus_window(index);
                }
                Some(Hit {
                    layer: true,
                    surface,
                    ..
                }) => self.focus_layer(&surface),
                _ => {}
            }
        }

        let pointer = self.wayland.pointer.clone();
        pointer.button(
            self,
            &ButtonEvent {
                button: code,
                state,
                serial,
                time,
            },
        );
        pointer.frame(self);
    }

    pub(super) fn press_own(&mut self, code: u32) -> bool {
        if let Some((_, rect)) = self.prompt_art.as_ref() {
            let rect = *rect;
            if code == BTN_LEFT {
                let look = drifter_core::prompt::Look::themed(&self.palette);
                match look.hit(rect.size, self.pointer - rect.origin) {
                    Some(drifter_core::prompt::Press::Go) => self.prompt_submit(),
                    Some(drifter_core::prompt::Press::Cancel) => self.prompt_cancel(),
                    None => {}
                }
            }
            return true;
        }
        if self.press_card(code) {
            return true;
        }
        if self.press_overview_button(code) {
            return true;
        }
        if self.press_gauge_panel(code) {
            return true;
        }
        let had_menu = self.popover.is_some();
        if self.press_popover() {
            return true;
        }
        if self.press_tray(code, had_menu) {
            return true;
        }
        if code == BTN_RIGHT && self.open_context() {
            return true;
        }
        if self.press_dock() {
            return true;
        }
        if self.over_drawer() {
            return true;
        }
        self.take_grab()
    }

    pub(super) fn tell_axis(
        &mut self,
        event: &impl smithay::backend::input::PointerAxisEvent<LibinputInputBackend>,
    ) {
        use smithay::backend::input::{Axis, AxisSource};
        use smithay::input::pointer::{AxisFrame, PointerHandle};

        let source = event.source();
        let mut frame = AxisFrame::new(event.time_msec()).source(source);
        for axis in [Axis::Horizontal, Axis::Vertical] {
            if let Some(amount) = event.amount(axis) {
                frame = frame.value(axis, amount);
                if amount == 0.0 && source == AxisSource::Finger {
                    frame = frame.stop(axis);
                }
            }
            if let Some(ticks) = event.amount_v120(axis) {
                frame = frame.v120(axis, ticks as i32);
            }
        }

        let pointer: PointerHandle<State> = self.wayland.pointer.clone();
        pointer.axis(self, frame);
        pointer.frame(self);
    }

    pub(super) fn tell_keyboard(&mut self, event: &impl KeyboardKeyEvent<LibinputInputBackend>) {
        let time = event.time_msec();
        let code = event.key_code().raw();
        let pressed = event.state() == KeyState::Pressed;
        self.key_in(code, pressed, time);
    }

    pub(super) fn key_in(&mut self, code: u32, pressed: bool, time: u32) {
        if !self.access.sticky_keys {
            self.feed_key(code, pressed, time);
            return;
        }
        let modifier = drifter_core::shortcut::is_modifier_code(code);
        for (code, pressed) in self.sticky.key(code, pressed, modifier) {
            self.feed_key(code, pressed, time);
        }
    }

    pub(super) fn clock_ms(&self) -> u32 {
        self.started.elapsed().as_millis() as u32
    }

    pub(super) fn feed_key(&mut self, code: u32, pressed: bool, time: u32) {
        use smithay::input::keyboard::FilterResult;
        use smithay::utils::SERIAL_COUNTER;

        let keyboard = self.wayland.keyboard.clone();
        let state = match pressed {
            true => KeyState::Pressed,
            false => KeyState::Released,
        };
        let shortcut = keyboard.input::<Shortcut, _>(
            self,
            smithay::input::keyboard::Keycode::new(code),
            state,
            SERIAL_COUNTER.next_serial(),
            time,
            |state, held, keysym| {
                use drifter_core::shortcut::{Action, Mods, keysym_word};
                if let Some(vt) = drifter_core::shortcut::vt_number(keysym.modified_sym().raw())
                    && pressed
                {
                    state.swallowed.push(code);
                    return FilterResult::Intercept(Shortcut::Vt(vt));
                }
                if let Some(key) = crate::media::key(code) {
                    return FilterResult::Intercept(match pressed {
                        true => Shortcut::Media(key),
                        false => Shortcut::Swallow,
                    });
                }
                if !pressed {
                    return match state.swallowed.iter().position(|held| *held == code) {
                        Some(at) => {
                            state.swallowed.swap_remove(at);
                            FilterResult::Intercept(Shortcut::Swallow)
                        }
                        None => FilterResult::Forward,
                    };
                }
                if state.shortcuts_inhibited() || state.client_lock.is_some() {
                    return FilterResult::Forward;
                }
                let mut names: Vec<String> = keysym
                    .modified_syms()
                    .iter()
                    .copied()
                    .chain(keysym.raw_syms())
                    .filter_map(|sym| sym.name())
                    .map(keysym_word)
                    .collect();
                if let Some(latin) = keysym.raw_latin_sym_or_raw_current_sym()
                    && let Some(name) = latin.name()
                {
                    names.push(keysym_word(name));
                }
                let mods = Mods {
                    ctrl: held.ctrl,
                    alt: held.alt,
                    shift: held.shift,
                    logo: held.logo,
                };
                let found = state.shortcuts.iter().find(|(action, combo)| {
                    let free_shift = *action == Action::Switch && !combo.mods.shift;
                    combo.matches(mods, &names, free_shift)
                });
                if let Some((action, _)) = found {
                    state.swallowed.push(code);
                    return FilterResult::Intercept(Shortcut::Do(*action, held.shift));
                }
                if let Some((slot, _)) = state
                    .place_saves
                    .iter()
                    .find(|(_, combo)| combo.matches(mods, &names, false))
                {
                    state.swallowed.push(code);
                    return FilterResult::Intercept(Shortcut::SavePlace(*slot));
                }
                let place = state.places.iter().position(|(_, combo)| {
                    combo
                        .as_ref()
                        .is_some_and(|combo| combo.matches(mods, &names, false))
                });
                match place {
                    Some(at) => {
                        state.swallowed.push(code);
                        FilterResult::Intercept(Shortcut::Place(at))
                    }
                    None => FilterResult::Forward,
                }
            },
        );
        match shortcut {
            Some(Shortcut::Do(action, shift)) => self.run_shortcut(action, shift),
            Some(Shortcut::Place(at)) if self.lock.is_none() => self.go_to_place(at),
            Some(Shortcut::SavePlace(slot)) if self.lock.is_none() => self.save_place(slot),
            Some(Shortcut::Place(_) | Shortcut::SavePlace(_)) => {}
            Some(Shortcut::Vt(vt)) => self.change_vt(vt),
            Some(Shortcut::Media(key)) => {
                if let Some(media) = self.media.as_ref() {
                    media.press(key);
                }
            }
            Some(Shortcut::Swallow) | None => {}
        }
        let held = keyboard.modifier_state();
        let wanted = self.switch_mods;
        let still =
            (wanted.alt && held.alt) || (wanted.ctrl && held.ctrl) || (wanted.logo && held.logo);
        if self.switch.is_some() && !still {
            self.switch_commit();
        }
    }

    pub(super) fn shortcuts_inhibited(&self) -> bool {
        use smithay::wayland::keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitorSeat;
        self.wayland
            .keyboard
            .current_focus()
            .and_then(|surface| {
                self.wayland
                    ._seat
                    .keyboard_shortcuts_inhibitor_for_surface(&surface)
            })
            .is_some_and(|inhibitor| inhibitor.is_active())
    }

    pub(super) fn input(&mut self, event: InputEvent<LibinputInputBackend>) {
        self.last_input = Instant::now();
        let seat = self.wayland._seat.clone();
        self.wayland.extras.idle.notify_activity(&seat);
        self.slept = false;
        self.wake();
        match &event {
            InputEvent::SwitchToggle { event }
                if SwitchToggleEvent::switch(event) == Some(HardSwitch::Lid) =>
            {
                if SwitchToggleEvent::state(event) == SwitchState::On {
                    self.lid_closed();
                }
                return;
            }
            InputEvent::Keyboard { event } if event.key_code().raw() == KEY_POWER => {
                if event.state() == KeyState::Pressed {
                    self.power_pressed();
                }
                return;
            }
            _ => {}
        }
        if self.client_lock.is_some() {
            self.client_locked_input(event);
            return;
        }
        if self.lock.is_none()
            && !self.prompts.is_empty()
            && let InputEvent::Keyboard { event } = &event
            && event.state() == KeyState::Pressed
        {
            let key = self.read_key(event);
            self.prompt_key(key);
            return;
        }
        if self.lock.is_some()
            && !matches!(
                event,
                InputEvent::DeviceAdded { .. } | InputEvent::DeviceRemoved { .. }
            )
        {
            self.locked_input(event);
            return;
        }
        match event {
            InputEvent::PointerMotion { event } => {
                let delta = Vec2::new(event.delta_x(), event.delta_y());
                let bare = Vec2::new(event.delta_x_unaccel(), event.delta_y_unaccel());
                match self.held_pointer() {
                    Some(None) => {}
                    Some(Some(area)) => {
                        let far = area.max() - Vec2::splat(1.0);
                        self.pointer =
                            (self.pointer + delta).clamp(area.origin, far.max(area.origin));
                        self.tell_pointer(event.time_msec());
                    }
                    None => {
                        self.pointer_to(self.pointer + delta);
                        self.drag();
                        self.hover();
                        self.tell_pointer(event.time_msec());
                    }
                }
                self.tell_relative(delta, bare, event.time());
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let rects = self.head_rects();
                let whole = drifter_core::screens::bounds(&rects);
                let global = whole.origin
                    + Vec2::new(
                        event.x_transformed(whole.size.x as i32),
                        event.y_transformed(whole.size.y as i32),
                    );
                if let Some(index) = drifter_core::screens::find(&rects, global) {
                    self.switch_head(index);
                }
                let local = global - self.heads[self.hand].place;
                let edge = self.screen - Vec2::splat(1.0);
                self.pointer_to(local.clamp(Vec2::ZERO, edge));
                self.drag();
                self.hover();
                self.tell_pointer(event.time_msec());
            }
            InputEvent::PointerButton { event } => {
                use smithay::backend::input::Event;
                let code = event.button_code();
                if code == BTN_MIDDLE
                    && self.touch.three_tap == drifter_core::ThreeTap::Overview
                    && self.held_by_fullscreen().is_none()
                    && is_touchpad(&event.device())
                {
                    if event.state() == ButtonState::Pressed {
                        self.three_tap();
                    }
                    return;
                }
                self.tell_button(code, event.state(), event.time_msec());
            }
            InputEvent::PointerAxis { event } => {
                use smithay::backend::input::{Axis, PointerAxisEvent};
                if self.modifier {
                    let steps = event
                        .amount_v120(Axis::Vertical)
                        .map(|raw| -raw / 120.0)
                        .or_else(|| event.amount(Axis::Vertical).map(|raw| -raw / 10.0))
                        .unwrap_or(0.0);
                    if steps != 0.0 {
                        self.zoom_by(steps);
                    }
                    return;
                }
                let steps = event
                    .amount_v120(Axis::Vertical)
                    .map(|raw| -raw / 120.0)
                    .or_else(|| event.amount(Axis::Vertical).map(|raw| -raw / 10.0))
                    .unwrap_or(0.0);
                if self.scroll_gauges(steps) {
                    return;
                }
                self.tell_axis(&event);
            }
            InputEvent::DeviceAdded { mut device } => {
                use smithay::reexports::input::DeviceCapability;
                if is_touchpad(&device) {
                    set_up_touchpad(&mut device, &self.touch, self.mouse.left_handed);
                    self.touchpads.push(device.clone());
                } else if device.has_capability(DeviceCapability::Pointer) {
                    set_up_mouse(&mut device, &self.mouse);
                    self.mice.push(device.clone());
                }
                println!("drifter: input {}", device.name());
            }
            InputEvent::DeviceRemoved { device } => {
                self.touchpads.retain(|pad| *pad != device);
                self.mice.retain(|mouse| *mouse != device);
            }
            InputEvent::GestureSwipeBegin { event } => {
                use smithay::backend::input::GestureBeginEvent;
                self.fingers = event.fingers();
            }
            InputEvent::GestureSwipeUpdate { event } => {
                use smithay::backend::input::GestureSwipeUpdateEvent;
                if self.fingers < 3 || !self.touch.swipe || self.held_by_fullscreen().is_some() {
                    return;
                }
                let moved = Vec2::new(event.delta_x(), event.delta_y());
                let mut camera = self.camera;
                camera.pan_by_screen(moved * -1.0);
                camera.clamp_to_world(self.world, self.screen);
                self.camera = camera;
                self.flight = None;
            }
            InputEvent::GestureSwipeEnd { .. } => self.fingers = 0,
            InputEvent::Keyboard { event } => {
                let code = event.key_code().raw();
                if self.holds.holds(code) && !self.shortcuts_inhibited() {
                    self.modifier = event.state() == KeyState::Pressed;
                    if self.modifier {
                        self.corner_since = None;
                    }
                }
                self.tell_keyboard(&event);
            }
            _ => {}
        }
    }

    fn client_locked_input(&mut self, event: InputEvent<LibinputInputBackend>) {
        use smithay::backend::input::Event;
        use smithay::input::pointer::ButtonEvent;
        use smithay::utils::SERIAL_COUNTER;

        self.focus_client_lock();
        let edge = self.screen - Vec2::splat(1.0);
        match event {
            InputEvent::PointerMotion { event } => {
                let delta = Vec2::new(event.delta_x(), event.delta_y());
                self.pointer = (self.pointer + delta).clamp(Vec2::ZERO, edge);
                self.tell_pointer(event.time_msec());
            }
            InputEvent::PointerMotionAbsolute { event } => {
                let x = event.x_transformed(self.screen.x as i32);
                let y = event.y_transformed(self.screen.y as i32);
                self.pointer = Vec2::new(x, y).clamp(Vec2::ZERO, edge);
                self.tell_pointer(event.time_msec());
            }
            InputEvent::PointerButton { event } => {
                let pointer = self.wayland.pointer.clone();
                pointer.button(
                    self,
                    &ButtonEvent {
                        button: event.button_code(),
                        state: event.state(),
                        serial: SERIAL_COUNTER.next_serial(),
                        time: event.time_msec(),
                    },
                );
                pointer.frame(self);
            }
            InputEvent::PointerAxis { event } => self.tell_axis(&event),
            InputEvent::Keyboard { event } => self.tell_keyboard(&event),
            InputEvent::DeviceAdded { mut device } => {
                use smithay::reexports::input::DeviceCapability;
                if is_touchpad(&device) {
                    set_up_touchpad(&mut device, &self.touch, self.mouse.left_handed);
                    self.touchpads.push(device.clone());
                } else if device.has_capability(DeviceCapability::Pointer) {
                    set_up_mouse(&mut device, &self.mouse);
                    self.mice.push(device.clone());
                }
            }
            InputEvent::DeviceRemoved { device } => {
                self.touchpads.retain(|pad| *pad != device);
                self.mice.retain(|mouse| *mouse != device);
            }
            _ => {}
        }
    }

    pub(super) fn drop_held_input(&mut self) {
        self.modifier = false;
        self.fingers = 0;
        self.grab = Grab::None;
        self.zone = None;
        self.held = None;
        self.dwell = None;
        self.corner_since = None;
    }
}

pub(super) fn set_up_mouse(
    device: &mut smithay::reexports::input::Device,
    mouse: &drifter_core::Mouse,
) {
    use smithay::reexports::input::AccelProfile;
    if device.config_accel_is_available() {
        let profile = match mouse.accel {
            true => AccelProfile::Adaptive,
            false => AccelProfile::Flat,
        };
        let _ = device.config_accel_set_profile(profile);
        let _ = device.config_accel_set_speed(mouse.libinput_speed());
    }
    if device.config_scroll_has_natural_scroll() {
        let _ = device.config_scroll_set_natural_scroll_enabled(mouse.natural);
    }
    if device.config_left_handed_is_available() {
        let _ = device.config_left_handed_set(mouse.left_handed);
    }
}

pub(super) fn is_touchpad(device: &smithay::reexports::input::Device) -> bool {
    device.config_tap_finger_count() > 0
}

pub(super) fn set_up_touchpad(
    device: &mut smithay::reexports::input::Device,
    pad: &drifter_core::Touchpad,
    left_handed: bool,
) {
    use smithay::reexports::input::{AccelProfile, ScrollMethod, TapButtonMap};
    if device.config_left_handed_is_available() {
        let _ = device.config_left_handed_set(left_handed);
    }
    let _ = device.config_tap_set_enabled(pad.tap);
    let _ = device.config_tap_set_button_map(TapButtonMap::LeftRightMiddle);
    if device
        .config_scroll_methods()
        .contains(&ScrollMethod::TwoFinger)
    {
        let _ = device.config_scroll_set_method(ScrollMethod::TwoFinger);
    }
    if device.config_scroll_has_natural_scroll() {
        let _ = device.config_scroll_set_natural_scroll_enabled(pad.natural);
    }
    if device.config_dwt_is_available() {
        let _ = device.config_dwt_set_enabled(pad.ignore_while_typing);
    }
    if device.config_accel_is_available() {
        let profile = match pad.accel {
            true => AccelProfile::Adaptive,
            false => AccelProfile::Flat,
        };
        let _ = device.config_accel_set_profile(profile);
        let _ = device.config_accel_set_speed(pad.libinput_speed());
    }
}
