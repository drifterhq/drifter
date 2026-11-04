use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::OsStrExt;

use smithay::reexports::calloop::{Interest, Mode, generic::Generic};

const EVENTS: u32 =
    libc::IN_CREATE | libc::IN_DELETE | libc::IN_MOVED_TO | libc::IN_MOVED_FROM | libc::IN_CLOSE_WRITE;

pub(super) fn watch() -> Option<Generic<OwnedFd>> {
    let fd = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
    if fd < 0 {
        return None;
    }
    let owned = unsafe { OwnedFd::from_raw_fd(fd) };
    let mut watched = 0;
    for dir in drifter_core::desktop::roots() {
        let Ok(path) = std::ffi::CString::new(dir.as_os_str().as_bytes()) else {
            continue;
        };
        if unsafe { libc::inotify_add_watch(fd, path.as_ptr(), EVENTS) } >= 0 {
            watched += 1;
        }
    }
    (watched > 0).then_some(Generic::new(owned, Interest::READ, Mode::Level))
}

pub(super) fn drain(fd: RawFd) {
    let mut buf = [0u8; 4096];
    loop {
        let read = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
        if read <= 0 {
            break;
        }
    }
}

pub(super) fn raw(fd: &OwnedFd) -> RawFd {
    fd.as_raw_fd()
}
