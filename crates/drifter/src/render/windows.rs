use super::*;

impl State {
    pub fn opening_motion(&self) -> Motion {
        Motion::new(Phase::Opening, self.dock_slot(0))
    }

    pub(super) fn dock_slot(&self, index: usize) -> Rect {
        let panel = self.shapes[Edge::Bottom.index()];
        let mut open = drifter_core::Drawer::new(Edge::Bottom);
        open.open = true;
        open.advance(Duration::from_secs(1), Duration::from_millis(1));
        let band = open.band(self.screen, &panel);
        let hidden = self.minimized().len();
        let seat = self.dock.slot_rect(
            band,
            self.dock_apps().len(),
            hidden.max(1),
            Slot::Minimized(index),
        );
        Rect::new(self.in_world(seat.origin), seat.size / self.camera.zoom)
    }

    pub(super) fn showing_of(&self, window: &crate::shell::Window) -> Showing {
        let (shown, fade) = window
            .motion
            .map(|motion| motion.shape_for(window.content, self.window_still))
            .unwrap_or((window.content, 1.0));
        (
            window.content,
            shown,
            fade,
            window.inset,
            (window.lit, window.restore.is_some()),
            match window.decorated {
                false => 0.0,
                true => match (window.full.is_none(), window.motion) {
                    (_, Some(motion)) if matches!(motion.phase, Phase::Filling | Phase::Unfilling) => motion.chrome() as f32,
                    (true, _) => 1.0,
                    (false, _) => 0.0,
                },
            },
            window.title.clone(),
            window.toplevel.wl_surface().clone(),
        )
    }

    pub(super) fn show_window(&mut self, item: Showing, shadow: bool, elements: &mut Vec<Skin>) {
        let (content, shown, fade, inset, (lit, maximized), chrome, title, surface) = item;
        let dressed = chrome > 0.0;
        let zoom = self.camera.zoom;
        let stretch = match content.size.x > 0.0 {
            true => shown.size.x / content.size.x,
            false => 1.0,
        };
        let at = self
            .camera
            .screen_from_world(shown.origin - inset * stretch, self.screen);
        let anchor = smithay::utils::Point::<i32, smithay::utils::Physical>::from((
            at.x.round() as i32,
            at.y.round() as i32,
        ));
        let client: Vec<WaylandSurfaceRenderElement<Painter>> =
            render_elements_from_surface_tree(
                &mut self.renderer,
                &surface,
                anchor,
                1.0,
                fade,
                Kind::Unspecified,
            );
        if shown == content {
            for (origin, popup) in self.popups_of(&surface, content) {
                let at = self.camera.screen_from_world(origin, self.screen);
                let anchor = smithay::utils::Point::<i32, smithay::utils::Physical>::from((
                    at.x.round() as i32,
                    at.y.round() as i32,
                ));
                let bare: Vec<WaylandSurfaceRenderElement<Painter>> =
                    render_elements_from_surface_tree(
                        &mut self.renderer,
                        &popup,
                        anchor,
                        1.0,
                        fade,
                        Kind::Unspecified,
                    );
                elements.extend(bare.into_iter().map(|piece| {
                    Skin::Client(RescaleRenderElement::from_element(piece, anchor, zoom))
                }));
            }
        }
        if dressed {
            self.dress(shown, (lit, maximized), &title, fade * chrome, elements);
        }
        let edge = self.on_screen(shown);
        let corners = self
            .windows
            .iter()
            .find(|window| window.toplevel.wl_surface() == &surface)
            .map_or([self.chrome.radius; 4], |window| self.corners_of(window));
        let washed = match dressed {
            true => [0.0; 4],
            false => corners.map(|round| round * zoom * stretch),
        };
        self.idle_wash(edge, washed, lit, fade, elements);
        let crop = smithay::utils::Rectangle::<i32, smithay::utils::Physical>::new(
            (edge.origin.x.round() as i32, edge.origin.y.round() as i32).into(),
            (edge.size.x.round() as i32, edge.size.y.round() as i32).into(),
        );
        elements.extend(client.into_iter().filter_map(|piece| {
            let scaled = RescaleRenderElement::from_element(piece, anchor, zoom * stretch);
            smithay::backend::renderer::element::utils::CropRenderElement::from_element(
                scaled,
                smithay::utils::Scale::from(1.0),
                crop,
            )
            .map(Skin::Clipped)
        }));
        if shadow && !maximized {
            self.cast_shadow(shown, dressed, corners, lit, fade, elements);
        }
    }

    pub(super) fn lit_target(&self, id: u64) -> f64 {
        match self.focus == Some(id) {
            true => 1.0,
            false => 0.0,
        }
    }

    pub(super) fn idle_wash(&self, edge: Rect, corners: [f64; 4], lit: f64, fade: f32, elements: &mut Vec<Skin>) {
        let wash = self.chrome.idle_wash * (1.0 - drifter_core::motion::smooth(lit));
        if let Some(black) = self.black_round.as_ref().filter(|_| wash > 0.0) {
            self.round_fill_each(black, corners, edge, wash as f32 * fade, elements);
        }
    }

    pub(super) fn corners_of(&self, window: &crate::shell::Window) -> [f64; 4] {
        match (window.full.is_some(), window.decorated) {
            (true, _) => [0.0; 4],
            (false, true) => [self.chrome.radius; 4],
            (false, false) => window.corners.unwrap_or([self.chrome.radius; 4]),
        }
    }

    pub(super) fn step_windows(&mut self, dt: Duration) {
        let dt = dt.min(Duration::from_millis(50));
        self.step_ghosts(dt);
        let focus = self.focus;
        for window in self.windows.iter_mut() {
            let target = if focus == Some(window.id) { 1.0 } else { 0.0 };
            window.lit = drifter_core::window::step_lit(window.lit, target, dt.as_secs_f64(), self.window_pace);
        }
        let mut finished: Vec<usize> = Vec::new();
        for (index, window) in self.windows.iter_mut().enumerate() {
            let Some(motion) = window.motion.as_mut() else {
                continue;
            };
            motion.advance(match self.window_pace {
                pace if pace <= 0.0 => Duration::from_secs(60),
                pace => dt.mul_f64(pace),
            });
            if motion.done() {
                finished.push(index);
            }
        }

        for index in finished.into_iter().rev() {
            let Some(window) = self.windows.get_mut(index) else {
                continue;
            };
            let phase = window.motion.take().map(|motion| motion.phase);
            match phase {
                Some(Phase::Minimizing) => {
                    window.hidden = true;
                    self.remember_icon(index);
                    self.wake_windows();
                }
                Some(Phase::Closing) => {
                    window.closed_at = Some(Instant::now());
                }
                _ => {}
            }
        }
        let mut back = false;
        for window in self.windows.iter_mut() {
            if window
                .closed_at
                .is_some_and(|at| at.elapsed() >= CLOSE_PATIENCE)
            {
                window.closed_at = None;
                window.going = false;
                window.motion = Some(Motion::new(Phase::Opening, window.content));
                back = true;
            }
        }
        if back {
            self.wake_windows();
        }
    }

    pub(super) fn popup_places(
        root: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        content: Rect,
    ) -> Vec<(
        Vec2,
        Rect,
        smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    )> {
        smithay::desktop::PopupManager::popups_for_surface(root)
            .map(|(popup, place)| {
                let geometry = popup.geometry();
                let at = content.origin + Vec2::new(f64::from(place.x), f64::from(place.y));
                let origin = at - Vec2::new(f64::from(geometry.loc.x), f64::from(geometry.loc.y));
                let size = match geometry.size.w > 0 && geometry.size.h > 0 {
                    true => Vec2::new(f64::from(geometry.size.w), f64::from(geometry.size.h)),
                    false => smithay::backend::renderer::utils::with_renderer_surface_state(
                        popup.wl_surface(),
                        |state| state.surface_size(),
                    )
                    .flatten()
                    .map(|size| Vec2::new(f64::from(size.w), f64::from(size.h)))
                    .unwrap_or(Vec2::ZERO),
                };
                (origin, Rect::new(at, size), popup.wl_surface().clone())
            })
            .collect()
    }

    pub(super) fn popups_of(
        &self,
        root: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
        content: Rect,
    ) -> Vec<(
        Vec2,
        smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    )> {
        Self::popup_places(root, content)
            .into_iter()
            .map(|(origin, _, surface)| (origin, surface))
            .collect()
    }

    pub(super) fn dismiss_popups(&mut self) {
        use smithay::desktop::PopupManager;
        use smithay::utils::SERIAL_COUNTER;

        for window in &self.windows {
            let root = window.toplevel.wl_surface();
            let open: Vec<_> = PopupManager::popups_for_surface(root)
                .map(|(popup, _)| popup)
                .collect();
            for popup in open {
                let _ = PopupManager::dismiss_popup(root, &popup);
            }
        }
        let time = self.clock_ms();
        let pointer = self.wayland.pointer.clone();
        if pointer.is_grabbed() {
            pointer.unset_grab(self, SERIAL_COUNTER.next_serial(), time);
        }
        let keyboard = self.wayland.keyboard.clone();
        if keyboard.is_grabbed() {
            keyboard.unset_grab(self);
        }
    }

    pub(super) fn layer_elements(
        &mut self,
        kinds: &[smithay::wayland::shell::wlr_layer::Layer],
        elements: &mut Vec<Skin>,
    ) {
        let placed: Vec<(
            smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
            Vec2,
        )> = {
            let map = smithay::desktop::layer_map_for_output(&self.output);
            kinds
                .iter()
                .flat_map(|kind| map.layers_on(*kind).rev().collect::<Vec<_>>())
                .filter_map(|layer| {
                    let place = map.layer_geometry(layer)?;
                    let at = Vec2::new(f64::from(place.loc.x), f64::from(place.loc.y));
                    Some((layer.wl_surface().clone(), at))
                })
                .collect()
        };
        for (surface, at) in placed {
            let mut surfaces = Vec::new();
            for (popup, place) in smithay::desktop::PopupManager::popups_for_surface(&surface) {
                let geometry = popup.geometry();
                let origin = at
                    + Vec2::new(
                        f64::from(place.x - geometry.loc.x),
                        f64::from(place.y - geometry.loc.y),
                    );
                surfaces.push((popup.wl_surface().clone(), origin));
            }
            surfaces.push((surface, at));
            for (surface, origin) in surfaces {
                let anchor = smithay::utils::Point::<i32, smithay::utils::Physical>::from((
                    origin.x.round() as i32,
                    origin.y.round() as i32,
                ));
                let bare: Vec<WaylandSurfaceRenderElement<Painter>> =
                    render_elements_from_surface_tree(
                        &mut self.renderer,
                        &surface,
                        anchor,
                        1.0,
                        1.0,
                        Kind::Unspecified,
                    );
                elements.extend(bare.into_iter().map(|piece| {
                    Skin::Client(RescaleRenderElement::from_element(piece, anchor, 1.0))
                }));
            }
        }
    }

    pub(super) fn focus_layer(
        &mut self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        use smithay::desktop::WindowSurfaceType;
        use smithay::utils::SERIAL_COUNTER;

        if self.lock.is_some() {
            return;
        }
        let Some(output) = self.layer_output(surface) else {
            return;
        };
        let wants = smithay::desktop::layer_map_for_output(&output)
            .layer_for_surface(surface, WindowSurfaceType::ALL)
            .map(|layer| {
                (
                    layer.can_receive_keyboard_focus(),
                    layer.wl_surface().clone(),
                )
            });
        let Some((true, root)) = wants else {
            return;
        };
        self.layer_focus = Some(root.clone());
        let keyboard = self.wayland.keyboard.clone();
        keyboard.set_focus(self, Some(root), SERIAL_COUNTER.next_serial());
    }

    pub fn layer_mapped(
        &mut self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        use smithay::desktop::WindowSurfaceType;
        use smithay::wayland::shell::wlr_layer::KeyboardInteractivity;

        if self.layer_focus.as_ref() == Some(surface) {
            return;
        }
        let Some(output) = self.layer_output(surface) else {
            return;
        };
        let exclusive = smithay::desktop::layer_map_for_output(&output)
            .layer_for_surface(surface, WindowSurfaceType::TOPLEVEL)
            .map(|layer| {
                layer.cached_state().keyboard_interactivity == KeyboardInteractivity::Exclusive
            })
            .unwrap_or(false);
        if exclusive {
            self.focus_layer(surface);
        }
    }

    pub fn layer_gone(
        &mut self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        if self.layer_focus.as_ref() == Some(surface) {
            self.layer_focus = None;
            self.focus_next(None);
        }
        self.kick_now();
    }

    pub fn import_dmabuf(&mut self, dmabuf: &smithay::backend::allocator::dmabuf::Dmabuf) -> bool {
        use smithay::backend::renderer::ImportDma;
        self.renderer.import_dmabuf(dmabuf, None).is_ok()
    }

    pub fn activate(
        &mut self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        if self.lock.is_some() {
            return;
        }
        let Some(index) = self
            .windows
            .iter()
            .position(|window| window.toplevel.wl_surface() == surface)
        else {
            return;
        };
        if self.windows[index].hidden {
            self.windows[index].hidden = false;
        }
        self.focus_window(index);
        self.kick_now();
    }

    pub fn window_geometry(
        &self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) -> Option<smithay::utils::Rectangle<i32, smithay::utils::Logical>> {
        let window = self
            .windows
            .iter()
            .find(|window| window.toplevel.wl_surface() == surface)?;
        Some(smithay::utils::Rectangle::new(
            (window.inset.x as i32, window.inset.y as i32).into(),
            (window.content.size.x as i32, window.content.size.y as i32).into(),
        ))
    }

    pub fn locked(&self) -> bool {
        self.lock.is_some()
    }

    pub(super) fn dress(
        &self,
        content: Rect,
        (lit, maximized): (f64, bool),
        title: &str,
        fade: f32,
        elements: &mut Vec<Skin>,
    ) {
        let layout = self.chrome.layout(content);
        let show = |rect: Rect| self.on_screen(rect);
        let bar = self.chrome.title_bar(content);
        let detail = drifter_core::window::detail_alpha(self.camera.zoom, DETAIL_FADE);
        let close_enough = detail > 0.0;
        let over = self
            .chrome
            .button_at(content, self.in_world(self.pointer), 3);
        let grow = if maximized { "restore" } else { "maximize" };
        for (slot, kind) in self
            .chrome
            .order
            .iter()
            .enumerate()
            .filter(|_| close_enough)
        {
            let name = match kind {
                ChromeButton::Close => "close",
                ChromeButton::Minimize => "minimize",
                ChromeButton::Maximize | ChromeButton::Restore => grow,
            };
            let look = match (over == Some(slot), self.held == Some(slot)) {
                (true, true) => "-press",
                (true, false) => "-hover",
                _ => "",
            };
            let file = format!("button-{name}{look}.png");
            if let Some((buffer, size)) = self.skin.get(&file) {
                let place = self.chrome.button_rect(slot, bar.size.x);
                let rect = Rect::new(bar.origin + place.origin, *size);
                elements.push(Skin::Texture(element(buffer, show(rect), fade * detail)));
            }
        }
        if let Some((buffer, size)) = self
            .titles
            .get(title)
            .filter(|_| close_enough && self.chrome.show_title)
        {
            let room = bar.size.x - self.chrome.buttons_width(3) * 2.0;
            let width = size.x.min(room.max(0.0));
            if width > 4.0 {
                let at = Vec2::new(
                    bar.center().x - width / 2.0,
                    bar.origin.y + (bar.size.y - size.y) / 2.0,
                );
                let dim = fade * detail * drifter_core::window::focus_look(lit, self.chrome.idle_title).0;
                for (source, offset, alpha) in drifter_core::window::title_cut(*size, room) {
                    let rect = Rect::new(at.round() + Vec2::new(offset, 0.0), source.size);
                    elements.push(Skin::Texture(faded(buffer, source, show(rect), dim * alpha)));
                }
            }
        }

        let pieces = [
            (ChromePiece::TitleLeft, layout.title_left),
            (ChromePiece::TitleMid, layout.title_mid),
            (ChromePiece::TitleRight, layout.title_right),
            (ChromePiece::EdgeLeft, layout.edge_left),
            (ChromePiece::EdgeRight, layout.edge_right),
            (ChromePiece::EdgeBottom, layout.edge_bottom),
            (ChromePiece::CornerBottomLeft, layout.corner_left),
            (ChromePiece::CornerBottomRight, layout.corner_right),
        ];
        for (piece, rect) in pieces {
            if rect.size.x <= 0.0 || rect.size.y <= 0.0 {
                continue;
            }
            let e = drifter_core::motion::smooth(lit) as f32;
            for (awake, alpha) in [(true, fade * e), (false, if e < 1.0 { fade } else { 0.0 })] {
                if alpha <= 0.0 {
                    continue;
                }
                let Some((buffer, art)) = self.skin.get(&piece.file_name(awake)) else {
                    continue;
                };
                match tiles(piece) {
                    true => {
                        let source = Rect::new(Vec2::ZERO, rect.size.min(*art));
                        elements.push(Skin::Texture(faded(buffer, source, show(rect), alpha)));
                    }
                    false => elements.push(Skin::Texture(element(buffer, show(rect), alpha))),
                }
            }
        }
    }

    pub(super) fn snap_target(&self, zone: drifter_core::snap::Zone) -> Rect {
        self.on_screen(self.snap.rect(zone, self.snap_view()))
    }

    pub(super) fn snap_drawn(&self) -> Option<(Rect, f32)> {
        let target = self.snap_target(self.zone?);
        let pace = self.window_pace;
        let Some((from, since)) = self.zone_glide.filter(|_| pace > 0.0) else {
            return Some((target, 1.0));
        };
        let t = since.elapsed().as_secs_f64() / (drifter_core::snap::GLIDE / pace);
        Some(drifter_core::snap::glide(from, target, t, self.window_still))
    }

    pub(super) fn snap_gliding(&self) -> bool {
        let pace = self.window_pace;
        self.zone.is_some()
            && pace > 0.0
            && self
                .zone_glide
                .is_some_and(|(_, since)| since.elapsed().as_secs_f64() < drifter_core::snap::GLIDE / pace)
    }

    pub(super) fn snap_elements(&self, elements: &mut Vec<Skin>) {
        let (Some((rect, alpha)), Some(art)) = (self.snap_drawn(), self.snap_art.as_ref()) else {
            return;
        };
        let radius = drifter_core::snap::RADIUS;
        for (source, target) in drifter_core::window::round_slices(rect, radius, radius) {
            elements.push(Skin::Texture(faded(art, source, target, alpha)));
        }
        self.panel_shadow(rect, fade::Shadow::Modal(radius), alpha, elements);
    }

    pub(super) fn modal_shadow(&self, rect: Rect, round: f64, elements: &mut Vec<Skin>) {
        self.panel_shadow(rect, fade::Shadow::Modal(round), 1.0, elements);
    }

    pub(super) fn cast_shadow(
        &self,
        content: Rect,
        dressed: bool,
        corners: [f64; 4],
        lit: f64,
        fade: f32,
        elements: &mut Vec<Skin>,
    ) {
        let Some(framed) = self.window_shadow.as_ref() else {
            return;
        };
        let round = corners.into_iter().fold(0.0, f64::max);
        let Some((buffer, corner)) = (match (round - self.chrome.radius).abs() < 0.5 {
            true => Some(framed),
            false => self.shade(drifter_core::window::SHADOW_BLUR, round),
        }) else {
            return;
        };
        let strength = fade * drifter_core::window::focus_look(lit, self.chrome.idle_title).1;
        let edge = match dressed {
            true => self.chrome.layout(content).frame,
            false => content,
        };
        let frame = self.on_screen(edge);
        for (source, target) in
            drifter_core::window::shadow_slices(frame, *corner, drifter_core::window::SHADOW_BLUR, self.camera.zoom, 10.0)
        {
            elements.push(Skin::Texture(faded(buffer, source, target, strength)));
        }
    }

    pub fn settle_window(
        &mut self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        if !self
            .windows
            .iter()
            .any(|window| window.toplevel.wl_surface() == surface)
        {
            return;
        }

        let whole =
            smithay::backend::renderer::utils::with_renderer_surface_state(surface, |state| {
                state.surface_size()
            })
            .flatten();
        let Some(whole) = whole else {
            return;
        };

        let geometry = smithay::wayland::compositor::with_states(surface, |states| {
            states
                .cached_state
                .get::<smithay::wayland::shell::xdg::SurfaceCachedState>()
                .current()
                .geometry
        });

        let (size, inset) = match geometry {
            Some(area) if area.size.w > 0 && area.size.h > 0 => (
                Vec2::new(f64::from(area.size.w), f64::from(area.size.h)),
                Vec2::new(f64::from(area.loc.x), f64::from(area.loc.y)),
            ),
            _ => (
                Vec2::new(f64::from(whole.w), f64::from(whole.h)),
                Vec2::ZERO,
            ),
        };
        if size.x <= 0.0 || size.y <= 0.0 {
            return;
        }
        let corners = smithay::wayland::compositor::with_states(surface, |states| {
            let opaque = states
                .cached_state
                .get::<smithay::wayland::compositor::SurfaceAttributes>()
                .current()
                .opaque_region
                .clone()?;
            drifter_core::window::app_corners(Rect::new(inset, size), |p| {
                opaque.contains((p.x.floor() as i32, p.y.floor() as i32))
            })
        });

        let (app, title) = smithay::wayland::compositor::with_states(surface, |states| {
            let held = states
                .data_map
                .get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>()
                .and_then(|data| {
                    data.lock()
                        .ok()
                        .map(|data| (data.app_id.clone(), data.title.clone()))
                });
            held.unwrap_or((None, None))
        });
        let app = app.unwrap_or_default();

        let fresh = self
            .windows
            .iter()
            .find(|window| window.toplevel.wl_surface() == surface)
            .map(|window| !window.placed)
            .unwrap_or(false);
        if fresh {
            self.enter_heads(surface);
            self.tell_scale(surface);
        }

        let view = self.camera.visible_rect(self.screen);
        let chrome = self.chrome;
        let taken: Vec<Vec2> = self
            .windows
            .iter()
            .filter(|window| window.placed && window.toplevel.wl_surface() != surface)
            .map(|window| window.content.origin)
            .collect();
        let Some(window) = self
            .windows
            .iter_mut()
            .find(|window| window.toplevel.wl_surface() == surface)
        else {
            return;
        };

        window.inset = inset;
        window.corners = corners;
        let grew = window.content.size != size;
        window.content.size = size;
        if let Some((_, screen)) = window.full {
            window.content.origin = (screen.center() - size / 2.0).round();
        }
        if let Grab::Resize {
            id, grip, start, ..
        } = self.grab
            && id == window.id
        {
            if grip.moves_left() {
                window.content.origin.x = start.max().x - size.x;
            }
            if grip.moves_up() {
                window.content.origin.y = start.max().y - size.y;
            }
        }
        if !app.is_empty() {
            window.app = app;
        }
        if let Some(title) = title {
            window.title = title;
        }
        crate::protocols::list_toplevel(
            &mut self.wayland.extras.toplevels,
            &mut window.listed,
            &window.title,
            &window.app,
        );

        if window.placed && (window.touched || !grew || window.born.elapsed() > SETTLING) {
            return;
        }
        if window.reopened {
            return;
        }

        if let Some(index) = self.windows.iter().position(|window| window.toplevel.wl_surface() == surface)
            && self.center_on_parent(index)
        {
            self.windows[index].placed = true;
            return;
        }
        let Some(window) = self
            .windows
            .iter_mut()
            .find(|window| window.toplevel.wl_surface() == surface)
        else {
            return;
        };
        let seen = view.origin;
        let middle = seen + (view.size - size) / 2.0;
        let top = middle.y.max(seen.y + chrome.title + chrome.edge);
        let wanted = Vec2::new(middle.x, top);
        window.content.origin = drifter_core::window::cascade(wanted, &taken, 34.0, view);
        window.placed = true;
        let app = window.app.clone();
        let toplevel = window.toplevel.clone();
        if !window.touched
            && let Some(spot) = self.reopen_spot(&app)
            && let Some(window) = self
                .windows
                .iter_mut()
                .find(|window| window.toplevel.wl_surface() == surface)
        {
            window.content.origin = spot.origin;
            window.reopened = true;
            if spot.size != size {
                toplevel.with_pending_state(|state| {
                    state.size = Some((spot.size.x as i32, spot.size.y as i32).into());
                });
                toplevel.send_pending_configure();
            }
        }
        let Some(window) = self
            .windows
            .iter_mut()
            .find(|window| window.toplevel.wl_surface() == surface)
        else {
            return;
        };
        let wants_full = std::mem::take(&mut window.wants_full);

        if fresh
            && self.lock.is_none()
            && let Some(index) = self
                .windows
                .iter()
                .position(|window| window.toplevel.wl_surface() == surface)
        {
            self.focus_window(index);
        }
        if wants_full
            && let Some(index) = self
                .windows
                .iter()
                .position(|window| window.toplevel.wl_surface() == surface)
        {
            self.enter_fullscreen(index);
        }
    }

    pub(super) fn full_rect(&self, near: Vec2) -> Rect {
        let half = self.screen / 2.0;
        let center = near.clamp(half, (self.world - half).max(half));
        Rect::from_center(center, self.screen).round_out()
    }

    pub fn enter_fullscreen(&mut self, index: usize) {
        use wayland_protocols::xdg::shell::server::xdg_toplevel;

        if self.windows[index].full.is_some() {
            return;
        }
        if !self.windows[index].placed {
            let window = &mut self.windows[index];
            window.wants_full = true;
            let size = (self.screen.x as i32, self.screen.y as i32);
            window.toplevel.with_pending_state(|state| {
                state.size = Some(size.into());
                state.states.set(xdg_toplevel::State::Fullscreen);
            });
            window.toplevel.send_pending_configure();
            return;
        }
        let screen = self.full_rect(self.windows[index].content.center());
        let window = &mut self.windows[index];
        let was = window.content;
        window.full = Some((window.content, screen));
        window.content = Rect::new(
            screen.center() - window.content.size / 2.0,
            window.content.size,
        );
        if window.motion.is_none() {
            window.motion = Some(Motion::new(Phase::Filling, was));
        }
        window.touched = true;
        window.toplevel.with_pending_state(|state| {
            state.size = Some((screen.size.x as i32, screen.size.y as i32).into());
            state.states.set(xdg_toplevel::State::Fullscreen);
        });
        window.toplevel.send_pending_configure();
        self.popover = None;
        self.dwell = None;
        self.leave = None;
        for edge in Edge::ALL {
            self.set(edge, false);
        }
        self.focus_window(index);
        self.kick_now();
    }

    pub fn leave_fullscreen(&mut self, index: usize) {
        use wayland_protocols::xdg::shell::server::xdg_toplevel;

        let window = &mut self.windows[index];
        window.wants_full = false;
        let Some((back, screen)) = window.full.take() else {
            window.toplevel.with_pending_state(|state| {
                state.states.unset(xdg_toplevel::State::Fullscreen);
            });
            window.toplevel.send_pending_configure();
            return;
        };
        window.content = back;
        if window.motion.is_none() {
            window.motion = Some(Motion::new(Phase::Unfilling, screen));
        }
        window.toplevel.with_pending_state(|state| {
            state.size = Some((back.size.x as i32, back.size.y as i32).into());
            state.states.unset(xdg_toplevel::State::Fullscreen);
        });
        window.toplevel.send_pending_configure();
        self.kick_now();
    }

    pub(super) fn held_by_fullscreen(&self) -> Option<usize> {
        let focus = self.focus?;
        self.windows
            .iter()
            .position(|window| window.id == focus && window.full.is_some() && !window.hidden)
    }

    pub(super) fn wind_titles(&mut self) {
        let Some(font) = self.font.as_deref() else {
            return;
        };
        let wanted: Vec<String> = self
            .windows
            .iter()
            .filter(|window| window.decorated && !window.title.is_empty())
            .map(|window| window.title.clone())
            .collect();

        for title in wanted {
            if self.titles.contains_key(&title) {
                continue;
            }
            let Some(font) = self.font.as_deref() else {
                return;
            };
            let image = font.paint(&title, self.tray.ink);
            let size = drifter_core::paint::logical(&image);
            match upload(&mut self.renderer, Some(&image), "window title") {
                Ok(Some(buffer)) => {
                    self.titles.insert(title, (buffer, size));
                }
                Ok(None) => {}
                Err(error) => log!("drifter: {error}"),
            }
        }
        let _ = font;

        if self.titles.len() > 64 {
            let live: std::collections::HashSet<&str> = self
                .windows
                .iter()
                .map(|window| window.title.as_str())
                .collect();
            self.titles.retain(|title, _| live.contains(title.as_str()));
        }
    }

    pub(super) fn raise(&mut self, index: usize) -> usize {
        let window = self.windows.remove(index);
        self.focus = Some(window.id);
        self.windows.push(window);
        self.wake_windows();
        self.windows.len() - 1
    }

    pub(super) fn focus_window(&mut self, index: usize) -> usize {
        use smithay::utils::SERIAL_COUNTER;

        let index = self.modal_of(index).unwrap_or(index);
        let index = self.raise(index);
        self.layer_focus = None;
        if let Some((_, screen)) = self.windows[index].full {
            self.fly_to(Camera::at_work_zoom(screen.center()));
        }
        if self.lock.is_none() {
            let surface = self.windows[index].toplevel.wl_surface().clone();
            let keyboard = self.wayland.keyboard.clone();
            keyboard.set_focus(self, Some(surface), SERIAL_COUNTER.next_serial());
        }
        index
    }

    pub(super) fn focus_next(&mut self, skip: Option<u64>) {
        use smithay::utils::SERIAL_COUNTER;

        let next = self.windows.iter().rposition(|window| {
            Some(window.id) != skip
                && window.placed
                && !window.hidden
                && !window.going
                && !window
                    .motion
                    .is_some_and(|motion| motion.phase == Phase::Minimizing)
        });
        match next {
            Some(index) => {
                self.focus_window(index);
            }
            None => {
                self.focus = None;
                self.wake_windows();
                if self.lock.is_none() {
                    let keyboard = self.wayland.keyboard.clone();
                    keyboard.set_focus(self, None, SERIAL_COUNTER.next_serial());
                }
            }
        }
    }

    pub(super) fn place_of(&self, id: u64) -> Option<usize> {
        self.windows.iter().position(|window| window.id == id)
    }

    pub fn next_window_id(&mut self) -> u64 {
        self.next_window += 1;
        self.next_window
    }

    pub fn window_gone(&mut self, surface: &smithay::wayland::shell::xdg::ToplevelSurface) {
        let Some(index) = self
            .windows
            .iter()
            .position(|window| &window.toplevel == surface)
        else {
            return;
        };
        self.keep_ghost(index);
        let window = self.windows.remove(index);
        if let Some(listed) = window.listed.as_ref() {
            self.wayland.extras.toplevels.remove_toplevel(listed);
        }
        let id = window.id;
        match self.grab {
            Grab::Move { id: held, .. } | Grab::Resize { id: held, .. } if held == id => {
                self.grab = Grab::None;
                self.zone = None;
                self.held = None;
            }
            _ => {}
        }
        if matches!(
            self.popover.as_ref().map(|open| &open.target),
            Some(Target::Window(target)) if *target == id
        ) {
            self.popover = None;
        }
        if self.focus == Some(id) {
            self.focus_next(None);
        }
    }

    pub(super) fn wake_windows(&mut self) {
        use wayland_protocols::xdg::shell::server::xdg_toplevel;
        for window in self.windows.iter() {
            let awake = self.focus == Some(window.id);
            window.toplevel.with_pending_state(|state| {
                if awake {
                    state.states.set(xdg_toplevel::State::Activated);
                } else {
                    state.states.unset(xdg_toplevel::State::Activated);
                }
            });
            window.toplevel.send_pending_configure();
        }
    }

    pub(super) fn remember_icon(&mut self, index: usize) {
        let name = self.windows[index].app.clone();
        if name.is_empty() || self.app_icons.contains_key(&name) || self.no_icon.contains(&name) {
            return;
        }
        let wanted = self.dock_side();
        let art = crate::apps::find(&name)
            .and_then(|entry| entry.icon)
            .and_then(|path| drifter_core::image::load_sized(&path, dense(wanted)).ok())
            .or_else(|| self.dock_spare.clone())
            .map(|image| image.scaled(dense(wanted), dense(wanted)));
        let Some(image) = art else {
            log!("drifter: no dock icon for {name}");
            self.no_icon.insert(name);
            return;
        };
        match upload(&mut self.renderer, Some(&image), "dock icon") {
            Ok(Some(buffer)) => {
                self.app_icons.insert(name, buffer);
            }
            Ok(None) => {}
            Err(error) => log!("drifter: {error}"),
        }
    }

    pub(super) fn bring(&mut self, place: usize) {
        if self.windows[place].hidden {
            let index = self.windows[..place]
                .iter()
                .filter(|window| window.hidden)
                .count();
            let seat = self.dock_slot(index);
            self.windows[place].hidden = false;
            self.windows[place].motion = Some(Motion::new(Phase::Restoring, seat));
            self.begin_stow(place);
        }
        let full = self.windows[place].full.is_some();
        let center = self.windows[place].content.center();
        let index = self.focus_window(place);
        if !full {
            self.fly_to(Camera::at_work_zoom(center));
        }
        let _ = index;
    }

    pub fn sync_wlr_toplevels(&mut self) {
        let shown: Vec<(u64, crate::wlr_toplevel::Shown)> = self
            .windows
            .iter()
            .filter(|window| window.placed && !window.going)
            .map(|window| {
                let minimizing = window
                    .motion
                    .is_some_and(|motion| motion.phase == Phase::Minimizing);
                (
                    window.id,
                    crate::wlr_toplevel::Shown {
                        title: window.title.clone(),
                        app: window.app.clone(),
                        maximized: window.restore.is_some(),
                        minimized: window.hidden || minimizing,
                        activated: self.focus == Some(window.id) && !window.hidden,
                        fullscreen: window.full.is_some(),
                    },
                )
            })
            .collect();
        let display = self.wayland.handle.clone();
        let output = self.output.clone();
        self.wayland
            .extras
            .wlr_toplevels
            .sync(&display, &shown, &output);
    }

    pub fn answer_wlr_toplevels(&mut self) {
        let asks = self.wayland.extras.wlr_toplevels.take_asks();
        if self.lock.is_some() {
            return;
        }
        for (id, ask) in asks {
            if let Some(place) = self.place_of(id) {
                self.answer(place, ask);
            }
        }
        self.sync_wlr_toplevels();
        self.kick_now();
    }

    pub fn app_asks(
        &mut self,
        surface: &smithay::wayland::shell::xdg::ToplevelSurface,
        ask: crate::wlr_toplevel::Ask,
    ) {
        let place = self
            .windows
            .iter()
            .position(|window| &window.toplevel == surface);
        match place {
            Some(place) if self.lock.is_none() => self.answer(place, ask),
            _ => {
                surface.send_configure();
            }
        }
        self.sync_wlr_toplevels();
        self.kick_now();
    }

    fn answer(&mut self, place: usize, ask: crate::wlr_toplevel::Ask) {
        use crate::wlr_toplevel::Ask;

        let window = &self.windows[place];
        let minimizing = window
            .motion
            .is_some_and(|motion| motion.phase == Phase::Minimizing);
        match ask {
            Ask::Activate => self.bring(place),
            Ask::Close => self.press_button(place, ChromeButton::Close),
            Ask::Minimize(true) if !window.hidden && !minimizing => {
                self.press_button(place, ChromeButton::Minimize)
            }
            Ask::Minimize(false) if window.hidden => self.bring(place),
            Ask::Maximize(true) if window.restore.is_none() => {
                self.press_button(place, ChromeButton::Maximize)
            }
            Ask::Maximize(false) if window.restore.is_some() => {
                self.press_button(place, ChromeButton::Restore)
            }
            Ask::Maximize(_) => {
                window.toplevel.send_configure();
            }
            Ask::Fullscreen(true) => self.enter_fullscreen(place),
            Ask::Fullscreen(false) => self.leave_fullscreen(place),
            _ => {}
        }
    }

    pub(super) fn close_focused(&mut self) {
        if self.lock.is_some() {
            return;
        }
        if let Some(place) = self.focus.and_then(|id| self.place_of(id)) {
            self.press_button(place, ChromeButton::Close);
        }
    }
}
