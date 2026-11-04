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
                make: drifter_core::lang::t("Drifter").to_string(),
                model: name.clone(),
            },
        );
        let global = output.create_global::<State>(&self.wayland.handle);
        output.set_preferred(mode);
        let panels = paint_panels(&mut self.renderer, pixels.x, &self.shapes, &self.palette)?;
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
            flip: false,
            vrr: false,
            vrr_wanted: (false, false),
            vrr_refused: false,
            vrr_idle: None,
            vrr_modeset: None,
            copy_stamp: 0,
            asleep: false,
            vblank: None,
            on: Vec::new(),
            latched: Vec::new(),
            mirror: None,
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
            feedback: None,
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
        let head = &self.heads[self.capture_head(&output)?];
        Some((head.name.clone(), head.screen, head.scale))
    }

    pub(super) fn capture_head(&self, output: &smithay::output::Output) -> Option<usize> {
        let at = self.heads.iter().position(|head| &head.output == output)?;
        Some(self.mirror_source(&self.heads[at]).unwrap_or(at))
    }

    pub(super) fn capture_scale(&self) -> f64 {
        self.heads
            .iter()
            .filter(|head| !head.asleep)
            .map(|head| head.scale)
            .fold(None, |most: Option<f64>, scale| Some(most.map_or(scale, |most| most.max(scale))))
            .unwrap_or(1.0)
    }

    pub(super) fn copy_unseen_head(&mut self, index: usize) {
        if !self.heads[index].asleep || self.mirror_source(&self.heads[index]).is_some() {
            return;
        }
        let name = self.heads[index].name.clone();
        let changed = self.heads[index].copy_stamp != self.commit_stamp;
        let wlr = self.copies.iter().any(|copy| copy.head == name && (!copy.damage || changed));
        let ext = self.wayland.extras.copies.captures.iter().any(|capture| {
            capture.waiting.is_some()
                && (!capture.shown || capture.dirty || changed)
                && matches!(&capture.source, Some(crate::image_copy::Source::Output(output)) if self.capture_head(output) == Some(index))
        });
        if !wlr && !ext {
            return;
        }
        self.heads[index].copy_stamp = self.commit_stamp;
        self.with_head(index, |state| {
            let scale = state.heads[state.at].scale;
            let mut pieces = state.elements();
            state.style_fade_elements(&mut pieces);
            let shown: Vec<Shown> = pieces
                .into_iter()
                .map(|piece| RescaleRenderElement::from_element(piece, (0, 0).into(), scale))
                .collect();
            state.serve_copies(&shown, changed);
            state.serve_screen_copies(&shown, changed);
        });
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

    pub(super) fn capture_texture(&mut self, elements: &[Shown]) -> Result<smithay::backend::renderer::gles::GlesTexture, String> {
        use smithay::backend::renderer::Bind;
        use smithay::backend::renderer::damage::OutputDamageTracker;
        use smithay::utils::{Buffer as BufferCoord, Physical, Size};

        let whole = self.physical();
        let size = Size::<i32, Physical>::from((whole.x as i32, whole.y as i32));
        let buffer_size = Size::<i32, BufferCoord>::from((size.w, size.h));
        let mut canvas = self
            .renderer
            .canvas(DrmFourcc::Abgr8888, buffer_size)
            .map_err(|e| format!("no offscreen buffer: {e}"))?;
        {
            let mut target = self
                .renderer
                .bind(&mut canvas)
                .map_err(|e| format!("cannot draw offscreen: {e}"))?;
            let shown = &elements[self.cursor_count.min(elements.len())..];
            let mut tracker = OutputDamageTracker::new(size, 1.0, Transform::Normal);
            tracker
                .render_output(&mut self.renderer, &mut target, 0, shown, self.clear)
                .map_err(|e| format!("cannot draw offscreen: {e:?}"))?;
        }
        match canvas {
            crate::paint::Canvas::Gles(texture) => Ok(texture),
            _ => Err("a reveal needs the gles renderer".to_string()),
        }
    }

    fn copied_elements<'a>(&self, elements: &'a [Shown], cursor: bool) -> Vec<&'a Shown> {
        let scene = self.cursor_count.min(elements.len());
        let mut shown: Vec<&Shown> = match cursor {
            true => elements[..self.cursor_end.min(scene)].iter().collect(),
            false => Vec::new(),
        };
        shown.extend(&elements[scene..]);
        shown
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
        let shown = self.copied_elements(elements, cursor);
        let mut tracker = OutputDamageTracker::new(size, 1.0, Transform::Normal);
        tracker
            .render_output(&mut self.renderer, &mut target, 0, &shown, self.clear)
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
        let shown = self.copied_elements(elements, cursor);
        let clear = self.clear;
        let mut target = self
            .renderer
            .bind(dmabuf)
            .map_err(|e| format!("cannot draw into a screen copy buffer: {e}"))?;
        let mut tracker = OutputDamageTracker::new(size, 1.0, Transform::Normal);
        let result = tracker
            .render_output(&mut self.renderer, &mut target, 0, &shown, clear)
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
        let bytes = self.capture(elements, cursor)?;
        let modifier = dmabuf.format().modifier;
        if crate::screencopy::by_hand(modifier) {
            return crate::screencopy::write_dmabuf(dmabuf, self.physical(), &bytes);
        }
        match self.copy_with_gpu(dmabuf, &bytes) {
            Ok(()) => Ok(()),
            Err(_) if modifier == smithay::backend::allocator::Modifier::Invalid => {
                crate::screencopy::write_dmabuf(dmabuf, self.physical(), &bytes)
            }
            Err(error) => Err(drifter_core::lang::tf("{}; the GPU copy failed too: {}", &[&(refused), &(error)])),
        }
    }

    pub(super) fn copy_with_gpu(
        &mut self,
        dmabuf: &mut smithay::backend::allocator::dmabuf::Dmabuf,
        bytes: &[u8],
    ) -> Result<(), String> {
        use smithay::backend::allocator::Buffer;
        use smithay::backend::renderer::{Bind, Frame, ImportMem, Renderer};
        use smithay::utils::{Buffer as BufferCoord, Physical, Rectangle, Size};

        if self.copier.is_none() {
            let made = match self.gbm.as_ref() {
                Some(gbm) => super::start::gles(gbm).map(|(renderer, _)| renderer),
                None => Err("there is no GPU to copy with".to_string()),
            };
            if let Err(error) = &made {
                log!("drifter: tiled screen copies cannot work: {error}");
            }
            self.copier = Some(made);
        }
        let Some(Ok(gles)) = self.copier.as_mut() else {
            return Err("there is no GPU to copy with".to_string());
        };
        let size = dmabuf.size();
        let (width, height) = (size.w, size.h);
        let texture = gles
            .import_memory(bytes, DrmFourcc::Abgr8888, (width, height).into(), false)
            .map_err(|e| format!("cannot upload the screen: {e}"))?;
        let mut target = gles
            .bind(dmabuf)
            .map_err(|e| format!("cannot draw into the buffer: {e}"))?;
        let whole = Rectangle::<i32, Physical>::from_size(Size::from((width, height)));
        let mut frame = gles
            .render(&mut target, whole.size, Transform::Normal)
            .map_err(|e| format!("cannot start the copy: {e}"))?;
        frame
            .clear(Color32F::new(0.0, 0.0, 0.0, 0.0), &[whole])
            .map_err(|e| format!("cannot start the copy: {e}"))?;
        Frame::render_texture_from_to(
                &mut frame,
                &texture,
                Rectangle::<f64, BufferCoord>::from_size(Size::from((f64::from(width), f64::from(height)))),
                whole,
                &[whole],
                &[],
                Transform::Normal,
                1.0,
            )
            .map_err(|e| format!("cannot copy the screen: {e}"))?;
        frame
            .finish()
            .map_err(|e| format!("cannot finish the copy: {e}"))?
            .wait()
            .map_err(|_| "the copy was interrupted".to_string())
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
                        log!("drifter: {error}");
                        copy.frame.failed();
                    }
                }
                continue;
            }
            let slot = usize::from(copy.cursor);
            if shots[slot].is_none() {
                match self.capture(elements, copy.cursor) {
                    Ok(shot) => shots[slot] = Some(shot),
                    Err(error) => log!("drifter: {error}"),
                }
            }
            let written = shots[slot]
                .as_ref()
                .map(|bytes| crate::screencopy::fill(&copy, self.physical(), bytes));
            match written {
                Some(Ok(())) => crate::screencopy::done(&copy),
                Some(Err(error)) => {
                    log!("drifter: {error}");
                    copy.frame.failed();
                }
                None => copy.frame.failed(),
            }
        }
    }

    pub(super) fn screenshot(&mut self, elements: &[Shown]) {
        let name = self.heads[self.at].name.clone();
        let Some(round) = self.shot_round.as_mut() else {
            return;
        };
        round.left.retain(|left| *left != name);
        let (crop, window) = (round.crop, round.window);
        let alone = window.and_then(|id| match self.capture_window(id) {
            Ok(image) => Some(image),
            Err(error) => {
                log!("drifter: the window shot falls back to the screen: {error}");
                None
            }
        });
        let whole = self.physical();
        let scale = whole.x / self.screen.x.max(1.0);
        let flash = match (window.and_then(|id| self.place_of(id)), crop) {
            (Some(index), _) => self.shape_on_screen(index),
            (None, Some(rect)) => (rect, [0.0; 4]),
            (None, None) => (Rect::new(Vec2::ZERO, self.screen), [0.0; 4]),
        };
        let image = match alone {
            Some(image) => Some(image),
            None => match self.capture(elements, self.shots.pointer && self.at == self.hand) {
                Ok(bytes) => {
                    let mut rgba = Vec::with_capacity(bytes.len());
                    for pixel in bytes.as_chunks::<4>().0 {
                        rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
                    }
                    let screen = drifter_core::Image {
                        width: whole.x as u32,
                        height: whole.y as u32,
                        rgba,
                    };
                    Some(match crop {
                        Some(rect) => drifter_core::grab::crop(&screen, Rect::new(rect.origin * scale, rect.size * scale)),
                        None => screen,
                    })
                }
                Err(error) => {
                    log!("drifter: no screenshot: {error}");
                    None
                }
            },
        };
        if let Some(image) = image {
            self.shot_flash.push((name, flash.0, flash.1, Instant::now()));
            let place = self.heads[self.at].place;
            let screen = self.screen;
            if let Some(round) = self.shot_round.as_mut() {
                round.got.push((place, screen, scale, image));
            }
        }
        if self.shot_round.as_ref().is_some_and(|round| round.left.is_empty()) {
            self.finish_round();
        }
    }

    pub(super) fn finish_round(&mut self) {
        let Some(mut round) = self.shot_round.take() else {
            return;
        };
        if round.got.is_empty() {
            self.shot_note(drifter_core::lang::t("No screenshot"), drifter_core::lang::t("The screen could not be read").to_string(), None);
            return;
        }
        self.play(drifter_core::shutter::Sound::Shutter);
        round.got.sort_by(|a, b| a.0.x.total_cmp(&b.0.x).then(a.0.y.total_cmp(&b.0.y)));
        let joined = round.got.len() > 1 && (self.shots.joined || round.stash);
        let pictures: Vec<drifter_core::Image> = match joined {
            true => vec![drifter_core::grab::stitch(&round.got)],
            false => round.got.into_iter().map(|(.., image)| image).collect(),
        };
        let count = pictures.len();
        for (at, image) in pictures.into_iter().enumerate() {
            self.keep_shot(image, round.stash, (count > 1).then_some(at + 1));
        }
        self.kick_now();
    }

    fn keep_shot(&mut self, image: drifter_core::Image, stash: bool, number: Option<usize>) {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let folder = match stash {
            true => drifter_core::grab::stash_folder(&home),
            false => self.shots.folder_in(&home),
        };
        let format = self.shots.format;
        let stamp = drifter_core::tray::now("%Y-%m-%d at %H.%M.%S").unwrap_or_default();
        let mut name = drifter_core::grab::file_name(drifter_core::grab::Take::Still, &stamp, format.extension());
        if let Some(number) = number {
            let (stem, ext) = drifter_core::explorer::ops::split_name(&name);
            name = format!("{stem} ({number}){ext}");
        }
        let path = self.fresh_shot_path(&folder, &name);
        let id = self.show_thumb(&image, path.clone(), stash, !self.shots.thumbnail);
        if stash {
            self.thumb_chore(id, thumbs::Chore::Copy);
        }
        self.background("drifter-screenshot", move || {
            if stash {
                drifter_core::grab::prune_stash(&folder, Duration::from_secs(24 * 60 * 60));
            }
            let saved = drifter_core::grab::save_shot(&image, &path, format).map(|()| path);
            move |state: &mut State| state.shot_saved(id, saved)
        });
    }

    fn capture_window(&mut self, id: u64) -> Result<drifter_core::Image, String> {
        let shadow = self.shots.shadow;
        let scale = self.heads.get(self.at).map_or(1.0, |head| head.scale);
        let image = self.paint_window(id, shadow, false, scale)?;
        Ok(drifter_core::grab::trim_clear(&image))
    }

    pub(super) fn window_frame(&self, index: usize) -> Rect {
        let window = &self.windows[index];
        match window.decorated && window.full.is_none() {
            true => self.chrome.layout(window.content).frame,
            false => window.content,
        }
    }

    pub(super) fn paint_window(&mut self, id: u64, shadow_wanted: bool, cursor: bool, scale: f64) -> Result<drifter_core::Image, String> {
        let index = self.place_of(id).ok_or_else(|| "the window closed".to_string())?;
        let frame = self.window_frame(index);
        let mut item = self.showing_of(&self.windows[index]);
        item.1 = item.0;
        item.2 = 1.0;
        item.4.0 = 1.0;
        let shadow = shadow_wanted && !item.4.1;
        let margin = if shadow { 96.0 } else { 0.0 };
        let size = (frame.size + Vec2::splat(margin * 2.0)).round();
        let seen = self.on_screen(frame);
        let over = cursor && seen.size.x > 0.0 && seen.size.y > 0.0 && seen.contains(self.pointer);
        let saved = (self.camera, self.screen);
        self.camera = Camera {
            center: frame.origin - Vec2::splat(margin) + size / 2.0,
            zoom: 1.0,
        };
        self.screen = size;
        let mut skins = Vec::new();
        if over {
            let held = self.pointer;
            let inside = held - seen.origin;
            self.pointer = Vec2::new(inside.x * frame.size.x / seen.size.x, inside.y * frame.size.y / seen.size.y) + Vec2::splat(margin);
            skins.extend(self.pointer_elements());
            self.pointer = held;
        }
        self.show_window(item, shadow, &mut skins);
        (self.camera, self.screen) = saved;
        self.render_skins(skins, size, scale)
    }

    pub(super) fn render_skins(&mut self, skins: Vec<Skin>, size: Vec2, scale: f64) -> Result<drifter_core::Image, String> {
        use smithay::backend::renderer::damage::OutputDamageTracker;
        use smithay::backend::renderer::{Bind, ExportMem};
        use smithay::utils::{Buffer as BufferCoord, Physical, Rectangle, Size};

        let shown: Vec<Shown> = skins
            .into_iter()
            .map(|piece| RescaleRenderElement::from_element(piece, (0, 0).into(), scale))
            .collect();
        let (width, height) = picture_pixels(size, scale);
        let (width, height) = (width as i32, height as i32);
        let physical = Size::<i32, Physical>::from((width, height));
        let buffer_size = Size::<i32, BufferCoord>::from((width, height));
        let mut texture = self
            .renderer
            .canvas(DrmFourcc::Abgr8888, buffer_size)
            .map_err(|e| format!("no offscreen buffer: {e}"))?;
        let mut target = self
            .renderer
            .bind(&mut texture)
            .map_err(|e| format!("cannot draw offscreen: {e}"))?;
        let mut tracker = OutputDamageTracker::new(physical, 1.0, Transform::Normal);
        tracker
            .render_output(&mut self.renderer, &mut target, 0, &shown, Color32F::new(0.0, 0.0, 0.0, 0.0))
            .map_err(|e| format!("cannot draw offscreen: {e:?}"))?;
        let mapping = self
            .renderer
            .copy_framebuffer(&target, Rectangle::from_size(buffer_size), DrmFourcc::Abgr8888)
            .map_err(|e| format!("cannot read the picture back: {e}"))?;
        let rgba = self
            .renderer
            .map_texture(&mapping)
            .map_err(|e| format!("cannot read the picture back: {e}"))?
            .to_vec();
        Ok(drifter_core::Image {
            width: width as u32,
            height: height as u32,
            rgba,
        })
    }

    fn fresh_shot_path(&self, folder: &std::path::Path, name: &str) -> PathBuf {
        let taken = |path: &std::path::Path| {
            std::fs::symlink_metadata(path).is_ok() || self.thumbs.shown.iter().any(|thumb| thumb.path == path)
        };
        let first = folder.join(name);
        if !taken(&first) {
            return first;
        }
        let (stem, ext) = drifter_core::explorer::ops::split_name(name);
        (2..)
            .map(|n| folder.join(format!("{stem} {n}{ext}")))
            .find(|path| !taken(path))
            .unwrap_or(first)
    }

    pub(super) fn disk_event(&mut self, event: crate::disks::Event) {
        let Some(id) = self.notes_bus.as_ref().map(|bus| bus.next_id()) else {
            return;
        };
        let (summary, body) = match event {
            crate::disks::Event::Mounted { name, folder } => {
                self.note_folders.insert(id, PathBuf::from(&folder));
                (format!("{name} is ready"), drifter_core::lang::tf("Click to open {}", &[&(folder)]))
            }
            crate::disks::Event::Failed { name, error } => {
                (drifter_core::lang::tf("{} did not open", &[&(name)]), error)
            }
        };
        self.show_note(drifter_core::notify::Note {
            id,
            app: drifter_core::lang::t("Drifter").to_string(),
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

pub(super) fn picture_pixels(size: Vec2, scale: f64) -> (u32, u32) {
    let whole = (size * scale).round();
    ((whole.x.max(1.0)) as u32, (whole.y.max(1.0)) as u32)
}
