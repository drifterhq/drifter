use super::*;

impl State {
    pub(super) fn add_offscreen_head(&mut self, size: (u32, u32)) -> Result<(), String> {
        let name = "HEADLESS-1".to_string();
        let pixels = Vec2::new(f64::from(size.0), f64::from(size.1));
        let mode = smithay::output::Mode {
            size: (size.0 as i32, size.1 as i32).into(),
            refresh: 60_000,
        };
        let source = OutputModeSource::Static {
            size: mode.size,
            scale: Scale::from(1.0),
            transform: Transform::Normal,
        };
        let compositor = Scanout::offscreen(&mut self.renderer, source)?;
        let output = smithay::output::Output::new(
            name.clone(),
            smithay::output::PhysicalProperties {
                size: (0, 0).into(),
                subpixel: smithay::output::Subpixel::Unknown,
                make: "Drifter".to_string(),
                model: name.clone(),
            },
        );
        let global = output.create_global::<State>(&self.wayland.handle);
        output.set_preferred(mode);
        let panels = paint_panels(&mut self.renderer, pixels.x, &self.shapes)?;
        let placeholder = std::num::NonZeroU32::MIN;
        self.heads.push(Head {
            name,
            connector: connector::Handle::from(placeholder),
            crtc: crtc::Handle::from(placeholder),
            compositor,
            output,
            global,
            mode,
            pixels,
            scale: 1.0,
            rotate: 0,
            place: Vec2::ZERO,
            camera: Camera::at_work_zoom(self.world / 2.0),
            flight: None,
            modes: Vec::new(),
            fit: None,
            drawers: [Drawer::new(Edge::Top), Drawer::new(Edge::Bottom)],
            screen: pixels,
            panels,
            awaiting: false,
            warmth: None,
            frames: 0,
            last: Instant::now(),
        });
        self.hand = 0;
        self.at = 0;
        self.load(0);
        self.relayout(false);
        Ok(())
    }

    pub(super) fn physical(&self) -> Vec2 {
        let scale = self
            .heads
            .get(self.at)
            .map(|head| head.scale)
            .unwrap_or(1.0);
        (self.screen * scale).round()
    }

    pub fn copy_target(
        &self,
        output: &smithay::reexports::wayland_server::protocol::wl_output::WlOutput,
    ) -> Option<(String, Vec2, f64)> {
        let output = smithay::output::Output::from_resource(output)?;
        let head = self.heads.iter().find(|head| head.output == output)?;
        Some((head.name.clone(), head.screen, head.scale))
    }

    pub fn queue_copy(&mut self, mut copy: crate::screencopy::Copy) {
        use smithay::reexports::wayland_server::Resource;

        let first = copy
            .frame
            .client()
            .is_some_and(|client| self.copied.insert(client.id()));
        if first {
            copy.damage = false;
        }
        let damage = copy.damage;
        self.copies.push(copy);
        if !damage {
            self.kick_now();
        }
    }

    pub(super) fn capture(&mut self, elements: &[Shown], cursor: bool) -> Result<Vec<u8>, String> {
        use smithay::backend::renderer::damage::OutputDamageTracker;
        use smithay::backend::renderer::{Bind, ExportMem};
        use smithay::utils::{Buffer as BufferCoord, Physical, Rectangle, Size};

        let whole = self.physical();
        let size = Size::<i32, Physical>::from((whole.x as i32, whole.y as i32));
        let buffer_size = Size::<i32, BufferCoord>::from((size.w, size.h));
        let mut texture = self
            .renderer
            .canvas(DrmFourcc::Abgr8888, buffer_size)
            .map_err(|e| format!("no offscreen buffer: {e}"))?;
        let mut target = self
            .renderer
            .bind(&mut texture)
            .map_err(|e| format!("cannot draw offscreen: {e}"))?;
        let shown = match cursor {
            true => elements,
            false => &elements[self.cursor_count.min(elements.len())..],
        };
        let mut tracker = OutputDamageTracker::new(size, 1.0, Transform::Normal);
        tracker
            .render_output(&mut self.renderer, &mut target, 0, shown, self.clear)
            .map_err(|e| format!("cannot draw offscreen: {e:?}"))?;
        let mapping = self
            .renderer
            .copy_framebuffer(
                &target,
                Rectangle::from_size(buffer_size),
                DrmFourcc::Abgr8888,
            )
            .map_err(|e| format!("cannot read the screen back: {e}"))?;
        let bytes = self
            .renderer
            .map_texture(&mapping)
            .map_err(|e| format!("cannot read the screen back: {e}"))?
            .to_vec();
        Ok(bytes)
    }

    pub(super) fn draw_into(
        &mut self,
        dmabuf: &mut smithay::backend::allocator::dmabuf::Dmabuf,
        elements: &[Shown],
        cursor: bool,
    ) -> Result<(), String> {
        use smithay::backend::allocator::Buffer;
        use smithay::backend::renderer::Bind;
        use smithay::backend::renderer::damage::OutputDamageTracker;
        use smithay::utils::{Physical, Size};

        let size = dmabuf.size();
        let whole = self.physical();
        if (f64::from(size.w), f64::from(size.h)) != (whole.x, whole.y) {
            return Err("a screen copy buffer is not the size of the screen".to_string());
        }
        let size = Size::<i32, Physical>::from((size.w, size.h));
        let shown = match cursor {
            true => elements,
            false => &elements[self.cursor_count.min(elements.len())..],
        };
        let clear = self.clear;
        let mut target = self
            .renderer
            .bind(dmabuf)
            .map_err(|e| format!("cannot draw into a screen copy buffer: {e}"))?;
        let mut tracker = OutputDamageTracker::new(size, 1.0, Transform::Normal);
        let result = tracker
            .render_output(&mut self.renderer, &mut target, 0, shown, clear)
            .map_err(|e| format!("cannot draw into a screen copy buffer: {e:?}"))?;
        result
            .sync
            .wait()
            .map_err(|_| "a screen copy was interrupted".to_string())
    }

    pub(super) fn copy_into(
        &mut self,
        dmabuf: &mut smithay::backend::allocator::dmabuf::Dmabuf,
        elements: &[Shown],
        cursor: bool,
    ) -> Result<(), String> {
        use smithay::backend::allocator::Buffer;

        let refused = match self.draw_into(dmabuf, elements, cursor) {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        if !crate::screencopy::by_hand(dmabuf.format().modifier) {
            return Err(refused);
        }
        let bytes = self.capture(elements, cursor)?;
        crate::screencopy::write_dmabuf(dmabuf, self.physical(), &bytes)
    }

    pub(super) fn serve_copies(&mut self, elements: &[Shown], changed: bool) {
        if self.copies.is_empty() {
            return;
        }
        let name = self.heads[self.at].name.clone();
        let (ready, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut self.copies)
            .into_iter()
            .partition(|copy| copy.head == name && (!copy.damage || changed));
        self.copies = waiting;
        if ready.is_empty() {
            return;
        }
        if self.lock.is_some() {
            for copy in ready {
                copy.frame.failed();
            }
            return;
        }
        let mut shots: [Option<Vec<u8>>; 2] = [None, None];
        for copy in ready {
            if let Ok(dmabuf) = smithay::wayland::dmabuf::get_dmabuf(&copy.buffer) {
                let mut dmabuf = dmabuf.clone();
                match self.copy_into(&mut dmabuf, elements, copy.cursor) {
                    Ok(()) => crate::screencopy::done(&copy),
                    Err(error) => {
                        eprintln!("drifter: {error}");
                        copy.frame.failed();
                    }
                }
                continue;
            }
            let slot = usize::from(copy.cursor);
            if shots[slot].is_none() {
                match self.capture(elements, copy.cursor) {
                    Ok(shot) => shots[slot] = Some(shot),
                    Err(error) => eprintln!("drifter: {error}"),
                }
            }
            let written = shots[slot]
                .as_ref()
                .map(|bytes| crate::screencopy::fill(&copy, self.physical(), bytes));
            match written {
                Some(Ok(())) => crate::screencopy::done(&copy),
                Some(Err(error)) => {
                    eprintln!("drifter: {error}");
                    copy.frame.failed();
                }
                None => copy.frame.failed(),
            }
        }
    }

    pub(super) fn screenshot(&mut self, elements: &[Shown]) {
        let bytes = match self.capture(elements, false) {
            Ok(shot) => shot,
            Err(error) => {
                eprintln!("drifter: no screenshot: {error}");
                return;
            }
        };
        let whole = self.physical();
        let (width, height) = (whole.x as u32, whole.y as u32);
        let mut rgba = Vec::with_capacity(bytes.len());
        for pixel in bytes.as_chunks::<4>().0 {
            rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
        }
        let image = drifter_core::Image {
            width,
            height,
            rgba,
        };
        let folder = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("Pictures/Screenshots");
        let stamp = drifter_core::tray::now("%Y-%m-%d %H.%M.%S").unwrap_or_default();
        let path = folder.join(format!("Screenshot {stamp}.png"));
        let reply = self.shot_tx.clone();
        let spawned = std::thread::Builder::new()
            .name("drifter-screenshot".to_string())
            .spawn(move || {
                let saved = std::fs::create_dir_all(&folder)
                    .map_err(|e| e.to_string())
                    .and_then(|()| drifter_core::image::save_png(&image, &path))
                    .map(|()| path);
                let _ = reply.send(saved);
            });
        if let Err(error) = spawned {
            self.shot_saved(Err(error.to_string()));
        }
    }

    pub(super) fn shot_saved(&mut self, saved: Result<PathBuf, String>) {
        let Some(id) = self.notes_bus.as_ref().map(|bus| bus.next_id()) else {
            return;
        };
        let (summary, body) = match saved {
            Ok(path) => ("Screenshot saved".to_string(), path.display().to_string()),
            Err(error) => ("No screenshot".to_string(), error),
        };
        self.show_note(drifter_core::notify::Note {
            id,
            app: "Drifter".to_string(),
            summary,
            body,
            actions: Vec::new(),
            expire: Some(drifter_core::notify::DEFAULT_MS),
            asked: -1,
            critical: false,
            icon: None,
        });
    }

    pub(super) fn frames_later(&mut self) {
        if self.frame_timer {
            return;
        }
        let wait = Duration::from_millis(16);
        self.frame_timer = self
            .handle
            .insert_source(Timer::from_duration(wait), |_, _, state: &mut State| {
                state.frame_timer = false;
                state.tell_clients();
                TimeoutAction::Drop
            })
            .is_ok();
    }
}
