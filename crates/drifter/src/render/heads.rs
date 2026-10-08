use super::*;

impl State {
    pub(super) fn head_rects(&self) -> Vec<Rect> {
        self.heads
            .iter()
            .map(|head| match self.mirror_source(head) {
                Some(_) => Rect::new(head.place, Vec2::ZERO),
                None => Rect::new(head.place, head.screen),
            })
            .collect()
    }

    pub(super) fn own_monitor(&self, name: &str) -> drifter_core::screens::MonitorConfig {
        self.monitors
            .iter()
            .find(|each| each.name == name)
            .cloned()
            .unwrap_or_else(|| drifter_core::screens::MonitorConfig::named(name))
    }

    pub(super) fn mirror_source(&self, head: &Head) -> Option<usize> {
        let source = head.mirror.as_deref()?;
        self.heads
            .iter()
            .position(|each| each.name == source && each.mirror.is_none())
    }

    pub(super) fn stash(&mut self) {
        let Some(head) = self.heads.get_mut(self.at) else {
            return;
        };
        head.camera = self.camera;
        head.flight = self.flight;
        head.drawers = self.drawers;
    }

    pub(super) fn load(&mut self, index: usize) {
        let Some(head) = self.heads.get(index) else {
            return;
        };
        self.at = index;
        self.camera = head.camera;
        self.flight = head.flight;
        self.drawers = head.drawers;
        self.screen = head.screen;
        self.panels = head.panels.clone();
        self.output = head.output.clone();
    }

    pub(super) fn with_head<R>(&mut self, index: usize, work: impl FnOnce(&mut State) -> R) -> R {
        if index != self.at {
            self.stash();
            self.load(index);
        }
        let out = work(self);
        if self.at != self.hand {
            self.stash();
            self.load(self.hand);
        }
        out
    }

    pub(super) fn switch_head(&mut self, index: usize) {
        if index == self.hand || index >= self.heads.len() {
            return;
        }
        self.stash();
        let hides = self.tunables.bar_hides;
        for drawer in &mut self.heads[self.at].drawers {
            drawer.open &= !hides[drawer.edge.index()];
        }
        self.popover = None;
        self.tip = None;
        self.tip_of = None;
        self.tip_want = None;
        self.dwell = None;
        self.leave = None;
        self.corner_since = None;
        self.hover_since = None;
        self.zone = None;
        self.hand = index;
        self.load(index);
    }

    pub(super) fn cross(&mut self, local: Vec2) -> Option<Vec2> {
        if self.heads.len() < 2 {
            return None;
        }
        let global = self.heads[self.hand].place + local;
        let next = drifter_core::screens::find(&self.head_rects(), global)?;
        if next == self.hand {
            return None;
        }
        self.switch_head(next);
        Some(global - self.heads[next].place)
    }

    pub(super) fn beside(&self, index: usize) -> Camera {
        let (Some(base), Some(head)) = (self.heads.first(), self.heads.get(index)) else {
            return Camera::at_work_zoom(self.world / 2.0);
        };
        let middle = |one: &Head| one.place + one.screen / 2.0;
        let mut camera = Camera::at_work_zoom(
            base.camera.center + (middle(head) - middle(base)) / base.camera.zoom,
        );
        camera.zoom = base
            .camera
            .zoom
            .max(Camera::fit_zoom(head.screen, self.world));
        camera.clamp_to_world(self.world, head.screen);
        camera
    }

    pub fn top_scale(&self) -> f64 {
        self.heads.iter().map(|head| head.scale).fold(1.0, f64::max)
    }

    pub fn tell_scale(
        &self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        let scale = self.top_scale();
        smithay::wayland::compositor::with_states(surface, |states| {
            smithay::wayland::fractional_scale::with_fractional_scale(states, |fractional| {
                fractional.set_preferred_scale(scale);
            });
            smithay::wayland::compositor::send_surface_state(
                surface,
                states,
                scale.ceil() as i32,
                Transform::Normal,
            );
        });
    }

    pub(super) fn tell_scales(&self) {
        for window in &self.windows {
            self.tell_scale(window.toplevel.wl_surface());
        }
        for head in &self.heads {
            let layers: Vec<_> = smithay::desktop::layer_map_for_output(&head.output)
                .layers()
                .map(|layer| layer.wl_surface().clone())
                .collect();
            for layer in layers {
                self.tell_scale(&layer);
            }
        }
    }

    pub fn enter_heads(
        &self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) {
        for head in &self.heads {
            head.output.enter(surface);
        }
    }

    pub fn layer_output(
        &self,
        surface: &smithay::reexports::wayland_server::protocol::wl_surface::WlSurface,
    ) -> Option<smithay::output::Output> {
        use smithay::desktop::WindowSurfaceType;
        self.heads
            .iter()
            .find(|head| {
                smithay::desktop::layer_map_for_output(&head.output)
                    .layer_for_surface(surface, WindowSurfaceType::ALL)
                    .is_some()
            })
            .map(|head| head.output.clone())
    }

    pub(super) fn add_head(
        &mut self,
        handle: connector::Handle,
        info: &connector::Info,
    ) -> Result<(), String> {
        let name = format!("{}-{}", info.interface().as_str(), info.interface_id());
        let modes: Vec<Mode> = info
            .modes()
            .iter()
            .map(|m| {
                let (w, h) = m.size();
                Mode::new(
                    u32::from(w),
                    u32::from(h),
                    m.vrefresh() * 1000,
                    m.mode_type()
                        .contains(smithay::reexports::drm::control::ModeTypeFlags::PREFERRED),
                )
            })
            .collect();
        let want = (info.interface() == connector::Interface::Virtual).then_some(self.virtual_size);
        let chosen = self
            .monitors
            .iter()
            .find(|monitor| monitor.name == name)
            .and_then(|monitor| monitor.mode.clone());
        let picked = mode::pick(&modes, chosen.as_deref(), want)
            .ok_or_else(|| format!("connector {name} offered no usable mode"))?;
        let drm_mode = *info
            .modes()
            .iter()
            .find(|m| {
                let (w, h) = m.size();
                u32::from(w) == picked.width
                    && u32::from(h) == picked.height
                    && m.vrefresh() * 1000 == picked.refresh_mhz
            })
            .ok_or_else(|| "selected mode vanished from the connector".to_string())?;

        let drm = self
            .drm
            .as_mut()
            .ok_or_else(|| "there is no drm device".to_string())?;
        let resources = drm
            .resource_handles()
            .map_err(|e| format!("cannot read drm resources: {e}"))?;
        let used: Vec<crtc::Handle> = self.heads.iter().map(|head| head.crtc).collect();
        let crtc = pick_crtc(drm, &resources, info, &used)
            .ok_or_else(|| format!("connector {name} has no free crtc"))?;
        let fd = drm.device_fd().clone();
        let surface = drm
            .create_surface(crtc, drm_mode, &[handle])
            .map_err(|e| format!("cannot create a drm surface for {name}: {e}"))?;

        let (width, height) = drm_mode.size();
        let pixels = Vec2::new(f64::from(width), f64::from(height));
        let wanted = drifter_core::screens::Head::new(&name, pixels, &self.monitors);
        let own = self.own_monitor(&name);
        let transform = turn(wanted.rotate, own.flip);
        let source = OutputModeSource::Static {
            size: (i32::from(width), i32::from(height)).into(),
            scale: Scale::from(1.0),
            transform,
        };
        let colors = [DrmFourcc::Xrgb8888, DrmFourcc::Argb8888];
        let compositor = match (&self.gbm, self.renderer.is_pixman()) {
            (Some(gbm), false) => DrmCompositor::new(
                source,
                surface,
                None,
                GbmAllocator::new(
                    gbm.clone(),
                    GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
                ),
                GbmFramebufferExporter::new(gbm.clone(), self.render_node),
                colors,
                self.render_formats.clone(),
                (64, 64).into(),
                Some(gbm.clone()),
            )
            .map(Scanout::Gbm)
            .map_err(|e| e.to_string()),
            _ => DrmCompositor::new(
                source,
                surface,
                None,
                smithay::backend::allocator::dumb::DumbAllocator::new(fd.clone()),
                fd,
                colors,
                self.render_formats.clone(),
                (64, 64).into(),
                None::<GbmDevice<DrmDeviceFd>>,
            )
            .map(Scanout::Dumb)
            .map_err(|e| e.to_string()),
        }
        .map_err(|e| format!("drm compositor for {name}: {e}"))?;

        let (mm_w, mm_h) = info.size().unwrap_or((0, 0));
        let output = smithay::output::Output::new(
            name.clone(),
            smithay::output::PhysicalProperties {
                size: (mm_w as i32, mm_h as i32).into(),
                subpixel: smithay::output::Subpixel::Unknown,
                make: "Drifter".to_string(),
                model: name.clone(),
            },
        );
        let global = output.create_global::<State>(&self.wayland.handle);
        let mode = smithay::output::Mode {
            size: (i32::from(width), i32::from(height)).into(),
            refresh: (drm_mode.vrefresh() * 1000) as i32,
        };
        output.set_preferred(mode);
        let screen = wanted.size();
        let panels = paint_panels(&mut self.renderer, screen.x, &self.shapes, &self.palette)?;
        for window in &self.windows {
            output.enter(window.toplevel.wl_surface());
        }
        out!(
            "screen      {name} {width} x {height} @ {}, scale {}, rotate {}",
            drm_mode.vrefresh(),
            wanted.scale,
            wanted.rotate
        );
        self.heads.push(Head {
            name,
            connector: handle,
            crtc,
            compositor,
            output,
            global,
            mode,
            pixels,
            scale: wanted.scale,
            rotate: wanted.rotate,
            flip: own.flip,
            vrr: false,
            mirror: own.mirrors().map(str::to_string),
            place: Vec2::ZERO,
            camera: Camera::at_work_zoom(self.world / 2.0),
            flight: None,
            modes: info.modes().to_vec(),
            fit: want,
            drawers: [Drawer::new(Edge::Top), Drawer::new(Edge::Bottom)],
            screen,
            panels,
            awaiting: false,
            warmth: None,
            feedback: None,
            frames: 0,
            last: Instant::now(),
        });
        Ok(())
    }

    pub(super) fn scan_heads(&mut self) {
        let Some(drm) = self.drm.as_ref() else {
            return;
        };
        let Ok(resources) = drm.resource_handles() else {
            log!("drifter: cannot read drm resources");
            return;
        };
        let connected: Vec<(connector::Handle, connector::Info)> = resources
            .connectors()
            .iter()
            .filter_map(|handle| {
                drm.get_connector(*handle, true)
                    .ok()
                    .map(|info| (*handle, info))
            })
            .filter(|(_, info)| {
                info.state() == connector::State::Connected && !info.modes().is_empty()
            })
            .collect();
        let named = |info: &connector::Info| {
            format!("{}-{}", info.interface().as_str(), info.interface_id())
        };
        let wanted_on = |name: &str| {
            self.monitors
                .iter()
                .find(|monitor| monitor.name == name)
                .is_none_or(|monitor| monitor.enabled)
        };
        let any_on = connected.iter().any(|(_, info)| wanted_on(&named(info)));
        self.plugs = connected
            .iter()
            .map(|(_, info)| {
                let name = named(info);
                let (mm_w, mm_h) = info.size().unwrap_or((0, 0));
                Plug {
                    on: !any_on || wanted_on(&name),
                    modes: core_modes(info.modes()),
                    millimeters: (mm_w, mm_h),
                    name,
                }
            })
            .collect();
        let connected: Vec<(connector::Handle, connector::Info)> = connected
            .into_iter()
            .filter(|(_, info)| !any_on || wanted_on(&named(info)))
            .collect();

        self.stash();
        let hand = self.heads.get(self.hand).map(|head| head.name.clone());
        if !connected.is_empty() {
            let mut index = self.heads.len();
            while index > 0 {
                index -= 1;
                let alive = connected
                    .iter()
                    .any(|(handle, _)| *handle == self.heads[index].connector);
                if alive {
                    continue;
                }
                let head = self.heads.remove(index);
                out!("screen      {} unplugged", head.name);
                let layers: Vec<_> = smithay::desktop::layer_map_for_output(&head.output)
                    .layers()
                    .cloned()
                    .collect();
                for layer in layers {
                    layer.layer_surface().send_close();
                }
                self.wayland
                    .handle
                    .remove_global::<State>(head.global.clone());
            }
        }
        let known = self.heads.len();
        for (handle, info) in &connected {
            if self.heads.iter().any(|head| head.connector == *handle) {
                continue;
            }
            if let Err(error) = self.add_head(*handle, info) {
                log!("drifter: {error}");
            }
        }
        let fresh = known..self.heads.len();
        if self.heads.is_empty() {
            return;
        }
        let found = hand.and_then(|name| self.heads.iter().position(|head| head.name == name));
        self.hand = found.unwrap_or(0);
        self.at = self.hand;
        self.load(self.hand);
        if found.is_none() {
            self.pointer = self.screen / 2.0;
            self.popover = None;
            self.tip = None;
        }
        self.relayout(false);
        if known > 0 && !fresh.is_empty() {
            self.stash();
            for index in fresh {
                self.heads[index].camera = self.beside(index);
            }
            self.load(self.hand);
        }
    }

    pub(super) fn wants_rescan(&self) -> bool {
        self.plugs.iter().any(|plug| {
            let on = self
                .monitors
                .iter()
                .find(|monitor| monitor.name == plug.name)
                .is_none_or(|monitor| monitor.enabled);
            on != plug.on
        })
    }

    fn follow_modes(&mut self) {
        for head in &mut self.heads {
            if head.modes.is_empty() {
                continue;
            }
            let chosen = self
                .monitors
                .iter()
                .find(|monitor| monitor.name == head.name)
                .and_then(|monitor| monitor.mode.clone());
            let Some(picked) = mode::pick(&core_modes(&head.modes), chosen.as_deref(), head.fit)
            else {
                continue;
            };
            let now = (head.mode.size.w as u32, head.mode.size.h as u32, head.mode.refresh as u32);
            if (picked.width, picked.height, picked.refresh_mhz) == now {
                continue;
            }
            let Some(drm_mode) = head.modes.iter().copied().find(|m| {
                let (w, h) = m.size();
                (u32::from(w), u32::from(h), m.vrefresh() * 1000)
                    == (picked.width, picked.height, picked.refresh_mhz)
            }) else {
                continue;
            };
            if let Err(error) = head.compositor.use_mode(drm_mode) {
                log!("drifter: {} cannot use {}: {error}", head.name, picked.text());
                continue;
            }
            let (width, height) = drm_mode.size();
            head.mode = smithay::output::Mode {
                size: (i32::from(width), i32::from(height)).into(),
                refresh: (drm_mode.vrefresh() * 1000) as i32,
            };
            head.pixels = Vec2::new(f64::from(width), f64::from(height));
            head.compositor.set_output_mode_source(OutputModeSource::Static {
                size: head.mode.size,
                scale: Scale::from(1.0),
                transform: turn(head.rotate, head.flip),
            });
            head.compositor.reset_buffers();
            out!("screen      {} now {}", head.name, picked.text());
        }
    }

    pub(super) fn relayout(&mut self, repaint: bool) {
        if self.heads.is_empty() {
            return;
        }
        self.follow_modes();
        self.stash();
        let wanted: Vec<drifter_core::screens::Head> = self
            .heads
            .iter()
            .map(|head| drifter_core::screens::Head::new(&head.name, head.pixels, &self.monitors))
            .collect();
        let owns: Vec<drifter_core::screens::MonitorConfig> =
            self.heads.iter().map(|head| self.own_monitor(&head.name)).collect();
        for (head, own) in self.heads.iter_mut().zip(&owns) {
            head.mirror = own.mirrors().map(str::to_string);
        }
        let mirrored: Vec<Option<usize>> = self.heads.iter().map(|head| self.mirror_source(head)).collect();
        let free: Vec<drifter_core::screens::Head> = wanted
            .iter()
            .zip(&mirrored)
            .filter(|(_, source)| source.is_none())
            .map(|(want, _)| want.clone())
            .collect();
        let mut placed = drifter_core::screens::arrange(&free, &self.monitors).into_iter();
        let mut rects: Vec<Rect> = mirrored
            .iter()
            .map(|source| match source {
                None => placed.next().unwrap_or(Rect::new(Vec2::ZERO, Vec2::ZERO)),
                Some(_) => Rect::new(Vec2::ZERO, Vec2::ZERO),
            })
            .collect();
        for (index, source) in mirrored.iter().enumerate() {
            if let Some(source) = source {
                rects[index] = rects[*source];
            }
        }
        for (index, (want, rect)) in wanted.iter().zip(&rects).enumerate() {
            let flip = owns[index].flip;
            let transform = turn(want.rotate, flip);
            let vrr = owns[index].vrr;
            let head = &mut self.heads[index];
            let resized = head.screen != rect.size;
            if head.vrr != vrr {
                match head.compositor.use_vrr(vrr) {
                    Ok(()) => head.vrr = vrr,
                    Err(error) => {
                        log!("drifter: {} cannot use adaptive sync: {error}", head.name);
                        head.vrr = false;
                    }
                }
            }
            if head.rotate != want.rotate || head.flip != flip {
                head.compositor
                    .set_output_mode_source(OutputModeSource::Static {
                        size: head.mode.size,
                        scale: Scale::from(1.0),
                        transform,
                    });
                head.compositor.reset_buffers();
            }
            head.scale = want.scale;
            head.rotate = want.rotate;
            head.flip = flip;
            head.place = rect.origin;
            head.screen = rect.size;
            head.output.change_current_state(
                Some(head.mode),
                Some(transform),
                Some(smithay::output::Scale::Fractional(want.scale)),
                Some((rect.origin.x as i32, rect.origin.y as i32).into()),
            );
            if resized || repaint {
                self.frost_key = None;
                self.drops = paint_drops(&mut self.renderer, rect.size.x, &self.shapes, &self.palette);
                match paint_panels(&mut self.renderer, rect.size.x, &self.shapes, &self.palette) {
                    Ok(panels) => self.heads[index].panels = panels,
                    Err(error) => log!("drifter: {error}"),
                }
            }
            let output = self.heads[index].output.clone();
            smithay::desktop::layer_map_for_output(&output).arrange();
        }

        let base = self.heads[0].screen;
        self.world = Vec2::new(
            base.x * f64::from(self.screens_wide.0),
            base.y * f64::from(self.screens_wide.1),
        );
        let world = self.world;
        for window in &mut self.windows {
            window.content = window.content.inside(world);
            window.restore = window.restore.map(|rect| rect.inside(world));
        }
        for head in &mut self.heads {
            let least = Camera::fit_zoom(head.screen, world);
            head.camera.zoom = head.camera.zoom.clamp(least, 4.0);
            head.camera.clamp_to_world(world, head.screen);
            head.flight = None;
        }
        self.load(self.hand);
        let edge = (self.screen - Vec2::splat(1.0)).max(Vec2::ZERO);
        self.pointer = self.pointer.clamp(Vec2::ZERO, edge);
        self.tell_scales();
        self.follow_density();
        self.sync_wlr_outputs(false);
    }

    pub(super) fn follow_density(&mut self) {
        let density = self.top_scale().ceil().clamp(1.0, 4.0) as u32;
        self.use_density(density);
    }

    pub(super) fn use_density(&mut self, density: u32) {
        if density == self.density {
            return;
        }
        self.density = density;
        drifter_core::paint::set_density(density);
        self.app_icons.clear();
        self.no_icon.clear();
        self.search_icons.clear();
        self.reused.clear();
        self.named_cursors.clear();
        self.frost_key = None;
        match Config::load_from(&self.config_path) {
            Ok(config) => {
                self.recolor(&config, self.palette);
                match load_arrow(&mut self.renderer, &config) {
                    Ok((arrow, size, hotspot)) => {
                        self.arrow = arrow;
                        self.arrow_size = size;
                        self.arrow_hotspot = hotspot;
                    }
                    Err(error) => log!("drifter: {error}"),
                }
                let side = self.dock.icon as u32;
                match dock_items(&mut self.renderer, &config, side, self.dock_spare.as_ref()) {
                    Ok(pinned) => self.pinned = pinned,
                    Err(error) => log!("drifter: {error}"),
                }
            }
            Err(error) => log!("drifter: the screen scale waits, the config did not load: {error}"),
        }
    }

    pub(super) fn seen_outputs(&self) -> Vec<crate::wlr_output::Seen> {
        let mode_of = |head: &Head| {
            Mode::new(
                head.mode.size.w as u32,
                head.mode.size.h as u32,
                head.mode.refresh as u32,
                false,
            )
        };
        let from_head = |head: &Head, modes: Vec<Mode>, millimeters: (u32, u32)| {
            let now = mode_of(head);
            let current = modes
                .iter()
                .copied()
                .find(|mode| (mode.width, mode.height, mode.refresh_mhz) == (now.width, now.height, now.refresh_mhz));
            crate::wlr_output::Seen {
                name: head.name.clone(),
                millimeters: (millimeters.0 as i32, millimeters.1 as i32),
                current,
                modes,
                enabled: true,
                position: (head.place.x as i32, head.place.y as i32),
                rotate: head.rotate,
                flip: head.flip,
                vrr: head.vrr,
                scale: head.scale,
            }
        };
        let mut seen = Vec::new();
        for plug in &self.plugs {
            match self.heads.iter().find(|head| head.name == plug.name) {
                Some(head) => seen.push(from_head(head, plug.modes.clone(), plug.millimeters)),
                None => {
                    let own = self.monitors.iter().find(|each| each.name == plug.name);
                    seen.push(crate::wlr_output::Seen {
                        name: plug.name.clone(),
                        millimeters: (plug.millimeters.0 as i32, plug.millimeters.1 as i32),
                        modes: plug.modes.clone(),
                        current: None,
                        enabled: false,
                        position: (0, 0),
                        rotate: own.map(|each| each.rotate).unwrap_or(0),
                        flip: own.is_some_and(|each| each.flip),
                        vrr: own.is_some_and(|each| each.vrr),
                        scale: own.map(|each| each.scale).unwrap_or(1.0),
                    });
                }
            }
        }
        for head in &self.heads {
            if !self.plugs.iter().any(|plug| plug.name == head.name) {
                let mut only = mode_of(head);
                only.preferred = true;
                seen.push(from_head(head, vec![only], (0, 0)));
            }
        }
        seen
    }

    pub fn sync_wlr_outputs(&mut self, force: bool) {
        let seen = self.seen_outputs();
        let display = self.wayland.handle.clone();
        self.wayland.extras.wlr_outputs.sync(&display, seen, force);
    }

    pub fn configure_outputs(
        &mut self,
        serial: u32,
        changes: &[crate::wlr_output::Change],
        only_test: bool,
    ) -> crate::wlr_output::Answer {
        use crate::wlr_output::Answer;

        if serial != self.wayland.extras.wlr_outputs.serial {
            return Answer::Stale;
        }
        let changes: Vec<(String, Option<crate::wlr_output::Want>)> = changes
            .iter()
            .map(|(name, want)| (name.clone(), want.as_ref().map(|want| want.lock().unwrap().clone())))
            .collect();
        let planned = match crate::wlr_output::plan(&self.monitors, &self.seen_outputs(), &changes) {
            Ok(planned) => planned,
            Err(error) => {
                log!("drifter: an output change was refused: {error}");
                return Answer::Refused;
            }
        };
        if only_test {
            return Answer::Done;
        }
        self.save_key(|text| drifter_core::settings::write_monitors(text, &planned));
        self.monitors = planned;
        match self.wants_rescan() {
            true => self.scan_heads(),
            false => self.relayout(false),
        }
        self.kick_now();
        Answer::Done
    }
}

pub(super) fn pick_crtc(
    device: &DrmDevice,
    resources: &smithay::reexports::drm::control::ResourceHandles,
    conn: &connector::Info,
    used: &[crtc::Handle],
) -> Option<crtc::Handle> {
    if let Some(encoder) = conn.current_encoder()
        && let Ok(info) = device.get_encoder(encoder)
        && let Some(crtc) = info.crtc()
        && !used.contains(&crtc)
    {
        return Some(crtc);
    }
    for encoder in conn.encoders() {
        if let Ok(info) = device.get_encoder(*encoder)
            && let Some(crtc) = resources
                .filter_crtcs(info.possible_crtcs())
                .into_iter()
                .find(|crtc| !used.contains(crtc))
        {
            return Some(crtc);
        }
    }
    None
}

pub(super) fn core_modes(modes: &[smithay::reexports::drm::control::Mode]) -> Vec<Mode> {
    modes
        .iter()
        .map(|m| {
            let (w, h) = m.size();
            Mode::new(
                u32::from(w),
                u32::from(h),
                m.vrefresh() * 1000,
                m.mode_type()
                    .contains(smithay::reexports::drm::control::ModeTypeFlags::PREFERRED),
            )
        })
        .collect()
}

pub(super) fn turn(rotate: u32, flip: bool) -> Transform {
    match (rotate, flip) {
        (90, false) => Transform::_90,
        (180, false) => Transform::_180,
        (270, false) => Transform::_270,
        (_, false) => Transform::Normal,
        (90, true) => Transform::Flipped90,
        (180, true) => Transform::Flipped180,
        (270, true) => Transform::Flipped270,
        (_, true) => Transform::Flipped,
    }
}
