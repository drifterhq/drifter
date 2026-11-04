use super::*;
use crate::image_copy::{Kind, Source, stop};
use smithay::backend::allocator::Buffer as _;
use smithay::backend::allocator::Fourcc;
use smithay::input::pointer::{CursorIcon, CursorImageStatus, CursorImageSurfaceData};
use smithay::reexports::wayland_protocols::ext::image_copy_capture::v1::server::ext_image_copy_capture_frame_v1::{
    ExtImageCopyCaptureFrameV1, FailureReason,
};
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::{wl_output, wl_shm};

const SHM_KINDS: [wl_shm::Format; 2] = [wl_shm::Format::Xrgb8888, wl_shm::Format::Argb8888];
const DMABUF_KINDS: [Fourcc; 2] = [Fourcc::Xrgb8888, Fourcc::Argb8888];

pub fn pixels_into(out: &mut [u8], stride: usize, width: usize, height: usize, rgba: &[u8], keep_alpha: bool) {
    for row in 0..height {
        for x in 0..width {
            let from = (row * width + x) * 4;
            let at = row * stride + x * 4;
            out[at] = rgba[from + 2];
            out[at + 1] = rgba[from + 1];
            out[at + 2] = rgba[from];
            out[at + 3] = if keep_alpha { rgba[from + 3] } else { 255 };
        }
    }
}

fn now_parts() -> (u32, u32, u32) {
    let mut now = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
    let seconds = now.tv_sec as u64;
    ((seconds >> 32) as u32, seconds as u32, now.tv_nsec as u32)
}

fn ready(frame: &ExtImageCopyCaptureFrameV1, size: (u32, u32)) {
    frame.transform(wl_output::Transform::Normal);
    frame.damage(0, 0, size.0 as i32, size.1 as i32);
    let (high, low, nanos) = now_parts();
    frame.presentation_time(high, low, nanos);
    frame.ready();
}

enum Fit {
    Shm,
    Dmabuf(smithay::backend::allocator::dmabuf::Dmabuf),
    Wrong,
}

fn fit_of(buffer: &WlBuffer, size: (u32, u32)) -> Fit {
    if let Ok(dmabuf) = smithay::wayland::dmabuf::get_dmabuf(buffer) {
        let shape = dmabuf.size();
        let right = (shape.w as u32, shape.h as u32) == size && DMABUF_KINDS.contains(&dmabuf.format().code);
        return match right {
            true => Fit::Dmabuf(dmabuf.clone()),
            false => Fit::Wrong,
        };
    }
    let right = smithay::wayland::shm::with_buffer_contents(buffer, |_, _, data| {
        (data.width as u32, data.height as u32) == size && SHM_KINDS.contains(&data.format)
    });
    match right {
        Ok(true) => Fit::Shm,
        _ => Fit::Wrong,
    }
}

fn write_shm(buffer: &WlBuffer, size: (u32, u32), rgba: &[u8]) -> Result<(), String> {
    let (width, height) = (size.0 as usize, size.1 as usize);
    smithay::wayland::shm::with_buffer_contents_mut(buffer, |ptr, len, data| {
        let stride = data.stride as usize;
        let offset = data.offset as usize;
        if stride < width * 4 || offset + stride * height > len || rgba.len() < width * height * 4 {
            return Err("the client's buffer is too small".to_string());
        }
        let out = unsafe { std::slice::from_raw_parts_mut(ptr.add(offset), stride * height) };
        pixels_into(out, stride, width, height, rgba, data.format == wl_shm::Format::Argb8888);
        Ok(())
    })
    .map_err(|error| format!("the client's buffer cannot be written: {error:?}"))?
}

impl State {
    fn copy_size(&mut self, source: &Source, kind: Kind) -> Option<(u32, u32)> {
        use super::capture::picture_pixels;
        match (source, kind) {
            (Source::Output(output), Kind::Picture { .. }) => {
                let head = &self.heads[self.capture_head(output)?];
                Some(picture_pixels(head.screen, head.scale))
            }
            (Source::Window(name), Kind::Picture { .. }) => {
                let index = self.window_named(name)?;
                Some(picture_pixels(self.window_frame(index).size.round(), self.capture_scale()))
            }
            (source, Kind::Cursor) => {
                let scale = self.cursor_scale(source)?;
                let (size, _) = self.cursor_shape().unwrap_or((Vec2::splat(1.0), Vec2::ZERO));
                Some(picture_pixels(size, scale))
            }
        }
    }

    fn cursor_scale(&self, source: &Source) -> Option<f64> {
        match source {
            Source::Output(output) => self.capture_head(output).map(|at| self.heads[at].scale),
            Source::Window(name) => self.window_named(name).map(|_| self.capture_scale()),
        }
    }

    fn cursor_key(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        match &self.cursor_status {
            CursorImageStatus::Hidden => 0u8.hash(&mut hash),
            CursorImageStatus::Named(icon) => {
                1u8.hash(&mut hash);
                icon.name().hash(&mut hash);
            }
            CursorImageStatus::Surface(surface) => {
                2u8.hash(&mut hash);
                surface.id().hash(&mut hash);
            }
        }
        self.cursor_stamp.hash(&mut hash);
        hash.finish()
    }

    fn window_named(&self, name: &str) -> Option<usize> {
        self.windows.iter().position(|window| {
            !window.going && window.listed.as_ref().is_some_and(|handle| handle.identifier() == name)
        })
    }

    pub(super) fn cursor_shape(&mut self) -> Option<(Vec2, Vec2)> {
        let _ = self.pointer_elements();
        match self.cursor_status.clone() {
            CursorImageStatus::Hidden => None,
            CursorImageStatus::Named(CursorIcon::Default) => Some((self.arrow_size, self.arrow_hotspot)),
            CursorImageStatus::Named(icon) => match self.named_cursors.get(icon.name()) {
                Some(Some((_, size, hotspot))) => Some((*size, *hotspot)),
                _ => Some((self.arrow_size, self.arrow_hotspot)),
            },
            CursorImageStatus::Surface(surface) => {
                let hotspot = smithay::wayland::compositor::with_states(&surface, |states| {
                    states
                        .data_map
                        .get::<CursorImageSurfaceData>()
                        .map(|data| data.lock().unwrap().hotspot)
                        .unwrap_or_default()
                });
                let bbox = smithay::desktop::utils::bbox_from_surface_tree(&surface, (0, 0));
                let size = Vec2::new(f64::from(bbox.size.w.max(1)), f64::from(bbox.size.h.max(1)));
                Some((size, Vec2::new(f64::from(hotspot.x), f64::from(hotspot.y))))
            }
        }
    }

    pub fn advertise_copy(&mut self, key: u64) {
        let Some(capture) = self.wayland.extras.copies.capture(key) else {
            return;
        };
        let (source, kind) = (capture.source.clone(), capture.kind);
        let size = source.as_ref().and_then(|source| self.copy_size(source, kind));
        let dmabuf = self.copy_dmabuf_kinds();
        let device = self.render_node.map(|node| node.dev_id());
        let Some(capture) = self.wayland.extras.copies.capture(key) else {
            return;
        };
        let Some(size) = size else {
            stop(capture);
            return;
        };
        for kind in SHM_KINDS {
            capture.session.shm_format(kind);
        }
        if let (Some(device), false) = (device, dmabuf.is_empty()) {
            capture.session.dmabuf_device(device.to_ne_bytes().to_vec());
            for (code, modifiers) in dmabuf {
                let words: Vec<u8> = modifiers.iter().flat_map(|modifier| modifier.to_ne_bytes()).collect();
                capture.session.dmabuf_format(code as u32, words);
            }
        }
        capture.session.buffer_size(size.0, size.1);
        capture.session.done();
        capture.size = Some(size);
        capture.shown = false;
        self.kick_now();
    }

    fn copy_dmabuf_kinds(&self) -> Vec<(Fourcc, Vec<u64>)> {
        if self.gbm.is_none() {
            return Vec::new();
        }
        DMABUF_KINDS
            .iter()
            .filter_map(|code| {
                let modifiers: Vec<u64> = self
                    .render_formats
                    .iter()
                    .filter(|format| format.code == *code)
                    .map(|format| u64::from(format.modifier))
                    .collect();
                (!modifiers.is_empty()).then_some((*code, modifiers))
            })
            .collect()
    }

    pub(super) fn check_copies(&mut self) {
        let keys: Vec<(u64, Option<Source>, Kind, Option<(u32, u32)>, bool)> = self
            .wayland
            .extras
            .copies
            .captures
            .iter()
            .map(|capture| (capture.key, capture.source.clone(), capture.kind, capture.size, capture.stopped))
            .collect();
        for (key, source, kind, size, stopped) in keys {
            if stopped || size.is_none() {
                continue;
            }
            let now = source.as_ref().and_then(|source| self.copy_size(source, kind));
            if now == size {
                continue;
            }
            let Some(capture) = self.wayland.extras.copies.capture(key) else {
                continue;
            };
            if now.is_none() {
                stop(capture);
                continue;
            }
            if let Some((frame, _)) = capture.waiting.take() {
                frame.failed(FailureReason::BufferConstraints);
            }
            self.advertise_copy(key);
        }
    }

    pub(super) fn serve_screen_copies(&mut self, elements: &[Shown], changed: bool) {
        if self.locked() {
            return;
        }
        let at = self.at;
        let mine: Vec<u64> = self
            .wayland
            .extras
            .copies
            .captures
            .iter()
            .filter(|capture| match &capture.source {
                Some(Source::Output(output)) => self.capture_head(output) == Some(at),
                _ => false,
            })
            .map(|capture| capture.key)
            .collect();
        let due: Vec<(u64, bool, (u32, u32), ExtImageCopyCaptureFrameV1, WlBuffer)> = self
            .wayland
            .extras
            .copies
            .captures
            .iter_mut()
            .filter(|capture| mine.contains(&capture.key))
            .filter_map(|capture| {
                capture.dirty |= changed;
                if capture.shown && !capture.dirty {
                    return None;
                }
                let Kind::Picture { cursor } = capture.kind else {
                    return None;
                };
                let size = capture.size?;
                let (frame, buffer) = capture.waiting.take()?;
                Some((capture.key, cursor, size, frame, buffer))
            })
            .collect();
        let mut shots: [Option<Vec<u8>>; 2] = [None, None];
        for (key, cursor, size, frame, buffer) in due {
            let done = match fit_of(&buffer, size) {
                Fit::Wrong => Err(FailureReason::BufferConstraints),
                Fit::Dmabuf(mut dmabuf) => self.copy_into(&mut dmabuf, elements, cursor).map_err(|error| {
                    log!("drifter: {error}");
                    FailureReason::Unknown
                }),
                Fit::Shm => {
                    let slot = usize::from(cursor);
                    if shots[slot].is_none() {
                        shots[slot] = self.capture(elements, cursor).inspect_err(|error| log!("drifter: {error}")).ok();
                    }
                    match shots[slot].as_ref() {
                        Some(bytes) => write_shm(&buffer, size, bytes).map_err(|error| {
                            log!("drifter: {error}");
                            FailureReason::Unknown
                        }),
                        None => Err(FailureReason::Unknown),
                    }
                }
            };
            self.finish_copy(key, &frame, size, done);
        }
    }

    fn finish_copy(&mut self, key: u64, frame: &ExtImageCopyCaptureFrameV1, size: (u32, u32), done: Result<(), FailureReason>) {
        if !frame.is_alive() {
            return;
        }
        match done {
            Ok(()) => {
                ready(frame, size);
                if let Some(capture) = self.wayland.extras.copies.capture(key) {
                    capture.shown = true;
                    capture.dirty = false;
                }
            }
            Err(reason) => frame.failed(reason),
        }
    }

    fn write_picture(&mut self, buffer: &WlBuffer, size: (u32, u32), image: &drifter_core::Image) -> Result<(), FailureReason> {
        if (image.width, image.height) != size {
            return Err(FailureReason::BufferConstraints);
        }
        let logged = |error: String| {
            log!("drifter: {error}");
            FailureReason::Unknown
        };
        match fit_of(buffer, size) {
            Fit::Wrong => Err(FailureReason::BufferConstraints),
            Fit::Shm => write_shm(buffer, size, &image.rgba).map_err(logged),
            Fit::Dmabuf(mut dmabuf) => {
                let modifier = dmabuf.format().modifier;
                if crate::screencopy::by_hand(modifier) {
                    return write_dmabuf_alpha(&dmabuf, size, &image.rgba).map_err(logged);
                }
                match self.copy_with_gpu(&mut dmabuf, &image.rgba) {
                    Ok(()) => Ok(()),
                    Err(_) if modifier == smithay::backend::allocator::Modifier::Invalid => {
                        write_dmabuf_alpha(&dmabuf, size, &image.rgba).map_err(logged)
                    }
                    Err(error) => Err(logged(error)),
                }
            }
        }
    }

    pub(super) fn serve_other_copies(&mut self) {
        if self.wayland.extras.copies.captures.is_empty() && self.wayland.extras.copies.pointers.is_empty() {
            return;
        }
        if self.locked() {
            self.hide_pointers();
            return;
        }
        self.tell_pointers();
        let cursor_now = self.cursor_key();
        let focus = self.focus;
        let pointer = self.pointer;
        let spots: Vec<(String, Option<Vec2>)> = self
            .windows
            .iter()
            .enumerate()
            .filter_map(|(index, window)| {
                let name = window.listed.as_ref()?.identifier();
                let seen = self.on_screen(self.window_frame(index));
                let over = seen.size.x > 0.0 && seen.size.y > 0.0 && seen.contains(pointer);
                Some((name, over.then(|| (pointer - seen.origin).round())))
            })
            .collect();
        let mut due = Vec::new();
        for capture in &mut self.wayland.extras.copies.captures {
            let Some(size) = capture.size else {
                continue;
            };
            if capture.waiting.is_none() || capture.stopped {
                continue;
            }
            match (&capture.source, capture.kind) {
                (Some(Source::Window(name)), Kind::Picture { cursor }) => {
                    let stamp = self
                        .windows
                        .iter()
                        .find(|window| window.listed.as_ref().is_some_and(|handle| handle.identifier() == *name))
                        .map_or(0, |window| window.stamp.wrapping_mul(2).wrapping_add(u64::from(focus == Some(window.id))));
                    let spot = match cursor {
                        true => spots.iter().find(|(held, _)| held == name).and_then(|(_, spot)| *spot),
                        false => None,
                    };
                    if capture.shown && capture.stamp == stamp && capture.spot == spot {
                        continue;
                    }
                    capture.stamp = stamp;
                    capture.spot = spot;
                    if let Some((frame, buffer)) = capture.waiting.take() {
                        due.push((capture.key, Ok(name.clone()), cursor, size, frame, buffer));
                    }
                }
                (Some(source), Kind::Cursor) => {
                    if capture.shown && capture.stamp == cursor_now {
                        continue;
                    }
                    capture.stamp = cursor_now;
                    if let Some((frame, buffer)) = capture.waiting.take() {
                        due.push((capture.key, Err(source.clone()), false, size, frame, buffer));
                    }
                }
                _ => {}
            }
        }
        let scale = self.capture_scale();
        for (key, window, cursor, size, frame, buffer) in due {
            let image = match window {
                Ok(name) => match self.window_named(&name).map(|index| self.windows[index].id) {
                    Some(id) => self.paint_window(id, false, cursor, scale),
                    None => Err("the window closed".to_string()),
                },
                Err(source) => match self.cursor_scale(&source) {
                    Some(own) => self.paint_cursor(own),
                    None => Err("the cursor's screen or window is gone".to_string()),
                },
            };
            let done = match image {
                Ok(image) => self.write_picture(&buffer, size, &image),
                Err(error) => {
                    log!("drifter: {error}");
                    Err(FailureReason::Unknown)
                }
            };
            self.finish_copy(key, &frame, size, done);
        }
    }

    fn paint_cursor(&mut self, scale: f64) -> Result<drifter_core::Image, String> {
        let Some((size, hotspot)) = self.cursor_shape() else {
            return self.render_skins(Vec::new(), Vec2::splat(1.0), scale);
        };
        let held = self.pointer;
        self.pointer = hotspot;
        let skins = self.pointer_elements();
        self.pointer = held;
        self.render_skins(skins, size, scale)
    }

    fn hide_pointers(&mut self) {
        for watch in &mut self.wayland.extras.copies.pointers {
            if watch.inside && watch.resource.is_alive() {
                watch.inside = false;
                watch.position = None;
                watch.hotspot = None;
                watch.resource.leave();
            }
        }
    }

    fn tell_pointers(&mut self) {
        if self.wayland.extras.copies.pointers.is_empty() {
            return;
        }
        let scale = self.capture_scale();
        let hand = self.hand;
        let on_hand: Vec<(smithay::output::Output, f64)> = self
            .heads
            .iter()
            .filter(|head| self.capture_head(&head.output) == Some(hand))
            .map(|head| (head.output.clone(), self.heads[hand].scale))
            .collect();
        let shape = self.cursor_shape();
        let pointer = self.pointer;
        let windows: Vec<(String, Rect, Rect)> = self
            .windows
            .iter()
            .enumerate()
            .filter(|(_, window)| !window.hidden && !window.going)
            .filter_map(|(index, window)| {
                let name = window.listed.as_ref()?.identifier();
                let frame = self.window_frame(index);
                Some((name, frame, self.on_screen(frame)))
            })
            .collect();
        for watch in &mut self.wayland.extras.copies.pointers {
            if !watch.resource.is_alive() {
                continue;
            }
            let place = match (&watch.source, shape) {
                (Some(Source::Output(output)), Some(_)) => on_hand
                    .iter()
                    .find(|(held, _)| held == output)
                    .map(|(_, own)| pointer * *own),
                (Some(Source::Window(name)), Some(_)) => windows
                    .iter()
                    .find(|(held, _, seen)| held == name && seen.size.x > 0.0 && seen.size.y > 0.0 && seen.contains(pointer))
                    .map(|(_, frame, seen)| {
                        let inside = pointer - seen.origin;
                        Vec2::new(inside.x * frame.size.x / seen.size.x, inside.y * frame.size.y / seen.size.y) * scale
                    }),
                _ => None,
            };
            let Some(place) = place else {
                if watch.inside {
                    watch.inside = false;
                    watch.position = None;
                    watch.hotspot = None;
                    watch.resource.leave();
                }
                continue;
            };
            if !watch.inside {
                watch.inside = true;
                watch.resource.enter();
            }
            let position = (place.x.round() as i32, place.y.round() as i32);
            if watch.position != Some(position) {
                watch.position = Some(position);
                watch.resource.position(position.0, position.1);
            }
            let own = match &watch.source {
                Some(Source::Output(output)) => on_hand.iter().find(|(held, _)| held == output).map_or(scale, |(_, own)| *own),
                _ => scale,
            };
            let hotspot = shape.map(|(_, hotspot)| ((hotspot.x * own).round() as i32, (hotspot.y * own).round() as i32));
            if let Some(hotspot) = hotspot
                && watch.hotspot != Some(hotspot)
            {
                watch.hotspot = Some(hotspot);
                watch.resource.hotspot(hotspot.0, hotspot.1);
            }
        }
        self.wayland.extras.copies.pointers.retain(|watch| watch.resource.is_alive());
    }

    pub fn bump_cursor(&mut self) {
        self.cursor_stamp = self.cursor_stamp.wrapping_add(1);
    }
}

fn write_dmabuf_alpha(dmabuf: &smithay::backend::allocator::dmabuf::Dmabuf, size: (u32, u32), rgba: &[u8]) -> Result<(), String> {
    use smithay::backend::allocator::dmabuf::{DmabufMappingMode, DmabufSyncFlags};

    if dmabuf.num_planes() != 1 {
        return Err("a capture buffer has more than one plane".to_string());
    }
    let (width, height) = (size.0 as usize, size.1 as usize);
    let stride = dmabuf.strides().next().unwrap_or(0) as usize;
    let mapping = dmabuf
        .map_plane(0, DmabufMappingMode::WRITE)
        .map_err(|error| format!("cannot map a capture buffer: {error}"))?;
    if stride < width * 4 || mapping.length() < stride * height {
        return Err("a capture buffer is too small".to_string());
    }
    let keep_alpha = dmabuf.format().code == Fourcc::Argb8888;
    let _ = dmabuf.sync_plane(0, DmabufSyncFlags::START | DmabufSyncFlags::WRITE);
    let out = unsafe { std::slice::from_raw_parts_mut(mapping.ptr().cast::<u8>(), stride * height) };
    pixels_into(out, stride, width, height, rgba, keep_alpha);
    let _ = dmabuf.sync_plane(0, DmabufSyncFlags::END | DmabufSyncFlags::WRITE);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::pixels_into;

    #[test]
    fn pixels_land_as_bgra_rows_with_padding() {
        let rgba = [10, 20, 30, 40, 50, 60, 70, 80];
        let mut out = [0u8; 12];
        pixels_into(&mut out, 12, 2, 1, &rgba, true);
        assert_eq!(&out[..8], &[30, 20, 10, 40, 70, 60, 50, 80]);
        assert_eq!(&out[8..], &[0, 0, 0, 0]);
        pixels_into(&mut out, 12, 2, 1, &rgba, false);
        assert_eq!(out[3], 255);
    }
}
