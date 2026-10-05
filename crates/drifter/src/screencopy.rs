use drifter_core::{Rect, Vec2};
use smithay::backend::allocator::dmabuf::{Dmabuf, DmabufMappingMode, DmabufSyncFlags};
use smithay::backend::allocator::{Buffer, Fourcc as DrmFourcc, Modifier as DrmModifier};
use smithay::reexports::wayland_protocols_wlr::screencopy::v1::server::zwlr_screencopy_frame_v1::{
    self, ZwlrScreencopyFrameV1,
};
use smithay::reexports::wayland_protocols_wlr::screencopy::v1::server::zwlr_screencopy_manager_v1::{
    self, ZwlrScreencopyManagerV1,
};
use smithay::reexports::wayland_server::backend::GlobalId;
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::protocol::wl_shm;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource,
};

use crate::render::State;

pub struct Asked {
    pub head: String,
    pub region: Rect,
    pub cursor: bool,
}

pub struct Copy {
    pub frame: ZwlrScreencopyFrameV1,
    pub buffer: WlBuffer,
    pub head: String,
    pub region: Rect,
    pub cursor: bool,
    pub damage: bool,
}

pub fn global(display: &DisplayHandle) -> GlobalId {
    display.create_global::<State, ZwlrScreencopyManagerV1, ()>(3, ())
}

impl GlobalDispatch<ZwlrScreencopyManagerV1, ()> for State {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwlrScreencopyManagerV1>,
        _data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        data_init.init(resource, ());
    }
}

impl Dispatch<ZwlrScreencopyManagerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        _manager: &ZwlrScreencopyManagerV1,
        request: zwlr_screencopy_manager_v1::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        let (frame, cursor, output, asked) = match request {
            zwlr_screencopy_manager_v1::Request::CaptureOutput {
                frame,
                overlay_cursor,
                output,
            } => (frame, overlay_cursor != 0, output, None),
            zwlr_screencopy_manager_v1::Request::CaptureOutputRegion {
                frame,
                overlay_cursor,
                output,
                x,
                y,
                width,
                height,
            } => {
                let asked = Rect::new(
                    Vec2::new(f64::from(x), f64::from(y)),
                    Vec2::new(f64::from(width), f64::from(height)),
                );
                (frame, overlay_cursor != 0, output, Some(asked))
            }
            _ => return,
        };
        let target = state.copy_target(&output);
        let (head, logical, scale) = target.clone().unwrap_or((String::new(), Vec2::ZERO, 1.0));
        let screen = Rect::new(Vec2::ZERO, (logical * scale).round());
        let region = match asked {
            None => screen,
            Some(asked) => clip(Rect::new(asked.origin * scale, asked.size * scale), screen),
        };
        let frame = data_init.init(
            frame,
            Asked {
                head,
                region,
                cursor,
            },
        );
        if target.is_none() || state.locked() || region.size.x < 1.0 || region.size.y < 1.0 {
            frame.failed();
            return;
        }
        let (width, height) = (region.size.x as u32, region.size.y as u32);
        frame.buffer(wl_shm::Format::Xrgb8888, width, height, width * 4);
        if frame.version() >= 3 {
            let whole = region == screen;
            if whole {
                frame.linux_dmabuf(
                    DrmFourcc::Xrgb8888 as u32,
                    width,
                    height,
                );
            }
            frame.buffer_done();
        }
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, Asked> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        frame: &ZwlrScreencopyFrameV1,
        request: zwlr_screencopy_frame_v1::Request,
        data: &Asked,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let (buffer, damage) = match request {
            zwlr_screencopy_frame_v1::Request::Copy { buffer } => (buffer, false),
            zwlr_screencopy_frame_v1::Request::CopyWithDamage { buffer } => (buffer, true),
            _ => return,
        };
        state.queue_copy(Copy {
            frame: frame.clone(),
            buffer,
            head: data.head.clone(),
            region: data.region,
            cursor: data.cursor,
            damage,
        });
    }
}

fn clip(asked: Rect, screen: Rect) -> Rect {
    let low = asked.origin.max(screen.origin).round();
    let high = asked.max().min(screen.max()).round();
    Rect::new(low, (high - low).max(Vec2::ZERO))
}

pub fn fill(copy: &Copy, screen: Vec2, rgba: &[u8]) -> Result<(), String> {
    let region = copy.region;
    let (width, height) = (region.size.x as usize, region.size.y as usize);
    let full_w = screen.x as usize;
    smithay::wayland::shm::with_buffer_contents_mut(&copy.buffer, |ptr, len, data| {
        let stride = data.stride as usize;
        let offset = data.offset as usize;
        let fits = data.width as usize == width
            && data.height as usize == height
            && stride >= width * 4
            && offset + stride * height <= len;
        let format_ok = matches!(
            data.format,
            wl_shm::Format::Xrgb8888 | wl_shm::Format::Argb8888
        );
        if !fits || !format_ok {
            return Err("the client's buffer does not match the frame".to_string());
        }
        let out = unsafe { std::slice::from_raw_parts_mut(ptr.add(offset), stride * height) };
        write_rows(out, stride, region, full_w, rgba);
        Ok(())
    })
    .map_err(|error| format!("the client's buffer cannot be written: {error:?}"))?
}

pub fn write_rows(out: &mut [u8], stride: usize, region: Rect, full_w: usize, rgba: &[u8]) {
    let (width, height) = (region.size.x as usize, region.size.y as usize);
    let left = region.origin.x as usize;
    let top = region.origin.y as usize;
    for row in 0..height {
        let from = ((top + row) * full_w + left) * 4;
        let to = row * stride;
        for x in 0..width {
            let pixel = &rgba[from + x * 4..from + x * 4 + 4];
            let at = to + x * 4;
            out[at] = pixel[2];
            out[at + 1] = pixel[1];
            out[at + 2] = pixel[0];
            out[at + 3] = 255;
        }
    }
}

pub fn by_hand(modifier: DrmModifier) -> bool {
    matches!(modifier, DrmModifier::Linear | DrmModifier::Invalid)
}

pub fn write_dmabuf(dmabuf: &Dmabuf, screen: Vec2, rgba: &[u8]) -> Result<(), String> {
    let format = dmabuf.format();
    if !by_hand(format.modifier) {
        return Err(format!(
            "a screen copy buffer is tiled ({:?}) and pixman cannot draw into it",
            format.modifier
        ));
    }
    if !matches!(format.code, DrmFourcc::Xrgb8888 | DrmFourcc::Argb8888) || dmabuf.num_planes() != 1 {
        return Err(format!("a screen copy buffer has the format {:?}", format.code));
    }
    let size = dmabuf.size();
    let (width, height) = (size.w as usize, size.h as usize);
    if (width as f64, height as f64) != (screen.x, screen.y) {
        return Err("a screen copy buffer is not the size of the screen".to_string());
    }
    let stride = dmabuf.strides().next().unwrap_or(0) as usize;
    let mapping = dmabuf
        .map_plane(0, DmabufMappingMode::WRITE)
        .map_err(|error| format!("cannot map a screen copy buffer: {error}"))?;
    if stride < width * 4 || mapping.length() < stride * height {
        return Err("a screen copy buffer is smaller than the screen".to_string());
    }
    let _ = dmabuf.sync_plane(0, DmabufSyncFlags::START | DmabufSyncFlags::WRITE);
    let out = unsafe { std::slice::from_raw_parts_mut(mapping.ptr().cast::<u8>(), stride * height) };
    let whole = Rect::new(Vec2::ZERO, screen);
    write_rows(out, stride, whole, width, rgba);
    let _ = dmabuf.sync_plane(0, DmabufSyncFlags::END | DmabufSyncFlags::WRITE);
    Ok(())
}

pub fn done(copy: &Copy) {
    if copy.damage {
        copy.frame
            .damage(0, 0, copy.region.size.x as u32, copy.region.size.y as u32);
    }
    copy.frame.flags(zwlr_screencopy_frame_v1::Flags::empty());
    let since = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = since.as_secs();
    copy.frame
        .ready((seconds >> 32) as u32, seconds as u32, since.subsec_nanos());
}
