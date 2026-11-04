use std::sync::Mutex;

use smithay::output::Output;
use smithay::reexports::wayland_protocols::ext::foreign_toplevel_list::v1::server::ext_foreign_toplevel_handle_v1::ExtForeignToplevelHandleV1;
use smithay::reexports::wayland_protocols::ext::image_capture_source::v1::server::{
    ext_foreign_toplevel_image_capture_source_manager_v1::{self as toplevel_sources, ExtForeignToplevelImageCaptureSourceManagerV1},
    ext_image_capture_source_v1::{self as source_object, ExtImageCaptureSourceV1},
    ext_output_image_capture_source_manager_v1::{self as output_sources, ExtOutputImageCaptureSourceManagerV1},
};
use smithay::reexports::wayland_protocols::ext::image_copy_capture::v1::server::{
    ext_image_copy_capture_cursor_session_v1::{self as cursor_session, ExtImageCopyCaptureCursorSessionV1},
    ext_image_copy_capture_frame_v1::{self as frame_object, ExtImageCopyCaptureFrameV1, FailureReason},
    ext_image_copy_capture_manager_v1::{self as manager, ExtImageCopyCaptureManagerV1, Options},
    ext_image_copy_capture_session_v1::{self as session_object, ExtImageCopyCaptureSessionV1},
};
use smithay::reexports::wayland_server::backend::{ClientId, GlobalId};
use smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer;
use smithay::reexports::wayland_server::{
    Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch, New, Resource, WEnum,
};
use smithay::wayland::foreign_toplevel_list::ForeignToplevelHandle;

use crate::render::State;

#[derive(Clone, PartialEq)]
pub enum Source {
    Output(Output),
    Window(String),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Picture { cursor: bool },
    Cursor,
}

pub struct Capture {
    pub key: u64,
    pub session: ExtImageCopyCaptureSessionV1,
    pub source: Option<Source>,
    pub kind: Kind,
    pub size: Option<(u32, u32)>,
    pub frame: Option<ExtImageCopyCaptureFrameV1>,
    pub waiting: Option<(ExtImageCopyCaptureFrameV1, WlBuffer)>,
    pub shown: bool,
    pub stamp: u64,
    pub spot: Option<drifter_core::Vec2>,
    pub dirty: bool,
    pub stopped: bool,
}

pub struct Pointer {
    pub key: u64,
    pub resource: ExtImageCopyCaptureCursorSessionV1,
    pub source: Option<Source>,
    pub made: bool,
    pub inside: bool,
    pub position: Option<(i32, i32)>,
    pub hotspot: Option<(i32, i32)>,
}

#[derive(Default)]
pub struct Copies {
    pub captures: Vec<Capture>,
    pub pointers: Vec<Pointer>,
    next: u64,
}

impl Copies {
    fn key(&mut self) -> u64 {
        self.next += 1;
        self.next
    }

    pub fn capture(&mut self, key: u64) -> Option<&mut Capture> {
        self.captures.iter_mut().find(|capture| capture.key == key)
    }
}

pub struct FrameData {
    pub key: u64,
    pub held: Mutex<FrameState>,
}

#[derive(Default)]
pub struct FrameState {
    pub buffer: Option<WlBuffer>,
    pub captured: bool,
}

pub struct Globals {
    _manager: GlobalId,
    _outputs: GlobalId,
    _toplevels: GlobalId,
}

pub fn globals(display: &DisplayHandle) -> Globals {
    Globals {
        _manager: display.create_global::<State, ExtImageCopyCaptureManagerV1, ()>(1, ()),
        _outputs: display.create_global::<State, ExtOutputImageCaptureSourceManagerV1, ()>(1, ()),
        _toplevels: display.create_global::<State, ExtForeignToplevelImageCaptureSourceManagerV1, ()>(1, ()),
    }
}

pub fn stop(capture: &mut Capture) {
    if capture.stopped {
        return;
    }
    capture.stopped = true;
    if let Some((frame, _)) = capture.waiting.take() {
        frame.failed(FailureReason::Stopped);
    }
    capture.session.stopped();
}

macro_rules! trusted_global {
    ($interface:ty) => {
        impl GlobalDispatch<$interface, ()> for State {
            fn can_view(client: Client, _data: &()) -> bool {
                crate::protocols::trusted(&client)
            }

            fn bind(
                _state: &mut Self,
                _handle: &DisplayHandle,
                _client: &Client,
                resource: New<$interface>,
                _data: &(),
                data_init: &mut DataInit<'_, Self>,
            ) {
                data_init.init(resource, ());
            }
        }
    };
}

trusted_global!(ExtImageCopyCaptureManagerV1);
trusted_global!(ExtOutputImageCaptureSourceManagerV1);
trusted_global!(ExtForeignToplevelImageCaptureSourceManagerV1);

impl Dispatch<ExtOutputImageCaptureSourceManagerV1, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _manager: &ExtOutputImageCaptureSourceManagerV1,
        request: output_sources::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let output_sources::Request::CreateSource { source, output } = request {
            data_init.init(source, Output::from_resource(&output).map(Source::Output));
        }
    }
}

impl Dispatch<ExtForeignToplevelImageCaptureSourceManagerV1, ()> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _manager: &ExtForeignToplevelImageCaptureSourceManagerV1,
        request: toplevel_sources::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        if let toplevel_sources::Request::CreateSource { source, toplevel_handle } = request {
            data_init.init(source, window_of(&toplevel_handle));
        }
    }
}

fn window_of(handle: &ExtForeignToplevelHandleV1) -> Option<Source> {
    let handle = ForeignToplevelHandle::from_resource(handle)?;
    (!handle.is_closed()).then(|| Source::Window(handle.identifier()))
}

impl Dispatch<ExtImageCaptureSourceV1, Option<Source>> for State {
    fn request(
        _state: &mut Self,
        _client: &Client,
        _source: &ExtImageCaptureSourceV1,
        _request: source_object::Request,
        _data: &Option<Source>,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
    }
}

impl Dispatch<ExtImageCopyCaptureManagerV1, ()> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        manager_object: &ExtImageCopyCaptureManagerV1,
        request: manager::Request,
        _data: &(),
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            manager::Request::CreateSession { session, source, options } => {
                let cursor = match options {
                    WEnum::Value(options) => options.contains(Options::PaintCursors),
                    WEnum::Unknown(_) => {
                        manager_object.post_error(manager::Error::InvalidOption, "unknown capture options");
                        return;
                    }
                };
                let source = source.data::<Option<Source>>().cloned().flatten();
                let key = state.wayland.extras.copies.key();
                let session = data_init.init(session, key);
                state.wayland.extras.copies.captures.push(Capture {
                    key,
                    session,
                    source,
                    kind: Kind::Picture { cursor },
                    size: None,
                    frame: None,
                    waiting: None,
                    shown: false,
                    stamp: 0,
                    spot: None,
                    dirty: false,
                    stopped: false,
                });
                state.advertise_copy(key);
            }
            manager::Request::CreatePointerCursorSession { session, source, pointer: _ } => {
                let source = source.data::<Option<Source>>().cloned().flatten();
                let key = state.wayland.extras.copies.key();
                let resource = data_init.init(session, key);
                state.wayland.extras.copies.pointers.push(Pointer {
                    key,
                    resource,
                    source,
                    made: false,
                    inside: false,
                    position: None,
                    hotspot: None,
                });
                state.kick_now();
            }
            _ => {}
        }
    }
}

impl Dispatch<ExtImageCopyCaptureCursorSessionV1, u64> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ExtImageCopyCaptureCursorSessionV1,
        request: cursor_session::Request,
        key: &u64,
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            cursor_session::Request::GetCaptureSession { session } => {
                let copies = &mut state.wayland.extras.copies;
                let pointer = copies.pointers.iter_mut().find(|pointer| pointer.key == *key);
                if pointer.as_ref().is_some_and(|pointer| pointer.made) {
                    resource.post_error(cursor_session::Error::DuplicateSession, "this cursor session already has a capture session");
                    return;
                }
                let source = match pointer {
                    Some(pointer) => {
                        pointer.made = true;
                        pointer.source.clone()
                    }
                    None => None,
                };
                let key = copies.key();
                let session = data_init.init(session, key);
                copies.captures.push(Capture {
                    key,
                    session,
                    source,
                    kind: Kind::Cursor,
                    size: None,
                    frame: None,
                    waiting: None,
                    shown: false,
                    stamp: 0,
                    spot: None,
                    dirty: false,
                    stopped: false,
                });
                state.advertise_copy(key);
            }
            cursor_session::Request::Destroy => {
                state.wayland.extras.copies.pointers.retain(|pointer| pointer.key != *key);
            }
            _ => {}
        }
    }

    fn destroyed(state: &mut Self, _client: ClientId, _resource: &ExtImageCopyCaptureCursorSessionV1, key: &u64) {
        state.wayland.extras.copies.pointers.retain(|pointer| pointer.key != *key);
    }
}

impl Dispatch<ExtImageCopyCaptureSessionV1, u64> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        session: &ExtImageCopyCaptureSessionV1,
        request: session_object::Request,
        key: &u64,
        _handle: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            session_object::Request::CreateFrame { frame } => {
                let held = state.wayland.extras.copies.capture(*key);
                if held.as_ref().is_some_and(|capture| capture.frame.as_ref().is_some_and(Resource::is_alive)) {
                    session.post_error(session_object::Error::DuplicateFrame, "the previous frame is not destroyed");
                    return;
                }
                let frame = data_init.init(frame, FrameData { key: *key, held: Mutex::new(FrameState::default()) });
                if let Some(capture) = held {
                    capture.frame = Some(frame);
                }
            }
            session_object::Request::Destroy => drop_capture(state, *key),
            _ => {}
        }
    }

    fn destroyed(state: &mut Self, _client: ClientId, _session: &ExtImageCopyCaptureSessionV1, key: &u64) {
        drop_capture(state, *key);
    }
}

impl Dispatch<ExtImageCopyCaptureFrameV1, FrameData> for State {
    fn request(
        state: &mut Self,
        _client: &Client,
        frame: &ExtImageCopyCaptureFrameV1,
        request: frame_object::Request,
        data: &FrameData,
        _handle: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        let mut held = data.held.lock().unwrap_or_else(|poison| poison.into_inner());
        match request {
            frame_object::Request::AttachBuffer { buffer } => {
                if held.captured {
                    frame.post_error(frame_object::Error::AlreadyCaptured, "the frame was already captured");
                    return;
                }
                held.buffer = Some(buffer);
            }
            frame_object::Request::DamageBuffer { x, y, width, height } => {
                if held.captured {
                    frame.post_error(frame_object::Error::AlreadyCaptured, "the frame was already captured");
                    return;
                }
                if x < 0 || y < 0 || width <= 0 || height <= 0 {
                    frame.post_error(frame_object::Error::InvalidBufferDamage, "damage must have a place and a size");
                }
            }
            frame_object::Request::Capture => {
                if held.captured {
                    frame.post_error(frame_object::Error::AlreadyCaptured, "the frame was already captured");
                    return;
                }
                let Some(buffer) = held.buffer.clone() else {
                    frame.post_error(frame_object::Error::NoBuffer, "capture needs a buffer first");
                    return;
                };
                held.captured = true;
                drop(held);
                match state.wayland.extras.copies.capture(data.key) {
                    Some(capture) if !capture.stopped => {
                        capture.waiting = Some((frame.clone(), buffer));
                        state.kick_now();
                    }
                    _ => frame.failed(FailureReason::Stopped),
                }
            }
            frame_object::Request::Destroy => {
                drop(held);
                forget_frame(state, frame, data.key);
            }
            _ => {}
        }
    }

    fn destroyed(state: &mut Self, _client: ClientId, frame: &ExtImageCopyCaptureFrameV1, data: &FrameData) {
        forget_frame(state, frame, data.key);
    }
}

fn drop_capture(state: &mut State, key: u64) {
    let copies = &mut state.wayland.extras.copies;
    if let Some(at) = copies.captures.iter().position(|capture| capture.key == key) {
        let capture = copies.captures.remove(at);
        if let Some((frame, _)) = capture.waiting {
            frame.failed(FailureReason::Stopped);
        }
    }
}

fn forget_frame(state: &mut State, frame: &ExtImageCopyCaptureFrameV1, key: u64) {
    if let Some(capture) = state.wayland.extras.copies.capture(key) {
        if capture.frame.as_ref() == Some(frame) {
            capture.frame = None;
        }
        if capture.waiting.as_ref().is_some_and(|(waiting, _)| waiting == frame) {
            capture.waiting = None;
        }
    }
}
