use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;

use zeroize::Zeroize;

const PAM_SUCCESS: c_int = 0;
const PAM_BUF_ERR: c_int = 5;
const PAM_CONV_ERR: c_int = 19;
const PAM_NEW_AUTHTOK_REQD: c_int = 12;
const PAM_TTY: c_int = 3;
const PAM_ESTABLISH_CRED: c_int = 0x2;
const PAM_DELETE_CRED: c_int = 0x4;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_ERROR_MSG: c_int = 3;

#[repr(C)]
struct Handle {
    _private: [u8; 0],
}

#[repr(C)]
struct Message {
    style: c_int,
    text: *const c_char,
}

#[repr(C)]
struct Response {
    text: *mut c_char,
    code: c_int,
}

#[repr(C)]
struct Conversation {
    talk: extern "C" fn(c_int, *mut *const Message, *mut *mut Response, *mut c_void) -> c_int,
    data: *mut c_void,
}

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(service: *const c_char, user: *const c_char, conversation: *const Conversation, handle: *mut *mut Handle) -> c_int;
    fn pam_authenticate(handle: *mut Handle, flags: c_int) -> c_int;
    fn pam_acct_mgmt(handle: *mut Handle, flags: c_int) -> c_int;
    fn pam_setcred(handle: *mut Handle, flags: c_int) -> c_int;
    fn pam_open_session(handle: *mut Handle, flags: c_int) -> c_int;
    fn pam_close_session(handle: *mut Handle, flags: c_int) -> c_int;
    fn pam_putenv(handle: *mut Handle, pair: *const c_char) -> c_int;
    fn pam_getenvlist(handle: *mut Handle) -> *mut *mut c_char;
    fn pam_set_item(handle: *mut Handle, kind: c_int, item: *const c_void) -> c_int;
    fn pam_strerror(handle: *mut Handle, code: c_int) -> *const c_char;
    fn pam_end(handle: *mut Handle, status: c_int) -> c_int;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Secret,
    Visible,
    Info,
    Error,
}

impl Style {
    pub fn word(self) -> &'static str {
        match self {
            Style::Secret => "secret",
            Style::Visible => "visible",
            Style::Info => "info",
            Style::Error => "error",
        }
    }
}

pub type Ask = dyn FnMut(Style, &str) -> Option<String>;

struct Talker {
    ask: Box<Ask>,
}

pub struct Pam {
    handle: *mut Handle,
    talker: *mut Talker,
    status: c_int,
    open: bool,
    cred: bool,
}

impl Pam {
    pub fn start(service: &str, user: &str, ask: Box<Ask>) -> Result<Pam, String> {
        let service = CString::new(service).map_err(|_| "bad service name".to_string())?;
        let user = CString::new(user).map_err(|_| "bad user name".to_string())?;
        let talker = Box::into_raw(Box::new(Talker { ask }));
        let conversation = Conversation {
            talk,
            data: talker.cast(),
        };
        let mut handle = ptr::null_mut();
        let code = unsafe { pam_start(service.as_ptr(), user.as_ptr(), &conversation, &mut handle) };
        if code != PAM_SUCCESS || handle.is_null() {
            drop(unsafe { Box::from_raw(talker) });
            return Err(format!("pam_start failed with code {code}"));
        }
        Ok(Pam {
            handle,
            talker,
            status: PAM_SUCCESS,
            open: false,
            cred: false,
        })
    }

    fn check(&mut self, code: c_int) -> Result<(), String> {
        self.status = code;
        if code == PAM_SUCCESS {
            return Ok(());
        }
        let text = unsafe { pam_strerror(self.handle, code) };
        let text = match text.is_null() {
            true => format!("code {code}"),
            false => unsafe { CStr::from_ptr(text) }.to_string_lossy().into_owned(),
        };
        Err(text)
    }

    pub fn authenticate(&mut self) -> Result<(), String> {
        let code = unsafe { pam_authenticate(self.handle, 0) };
        self.check(code)?;
        let code = match unsafe { pam_acct_mgmt(self.handle, 0) } {
            PAM_NEW_AUTHTOK_REQD => PAM_SUCCESS,
            other => other,
        };
        self.check(code)
    }

    pub fn account(&mut self) -> Result<(), String> {
        let code = unsafe { pam_acct_mgmt(self.handle, 0) };
        self.check(code)
    }

    pub fn tty(&mut self, tty: &str) -> Result<(), String> {
        let tty = CString::new(tty).map_err(|_| "bad tty".to_string())?;
        let code = unsafe { pam_set_item(self.handle, PAM_TTY, tty.as_ptr().cast()) };
        self.check(code)
    }

    pub fn put(&mut self, pair: &str) -> Result<(), String> {
        let pair = CString::new(pair).map_err(|_| "bad environment entry".to_string())?;
        let code = unsafe { pam_putenv(self.handle, pair.as_ptr()) };
        self.check(code)
    }

    pub fn open(&mut self) -> Result<(), String> {
        let code = unsafe { pam_setcred(self.handle, PAM_ESTABLISH_CRED) };
        self.check(code)?;
        self.cred = true;
        let code = unsafe { pam_open_session(self.handle, 0) };
        self.check(code)?;
        self.open = true;
        Ok(())
    }

    pub fn environment(&self) -> Vec<String> {
        let list = unsafe { pam_getenvlist(self.handle) };
        if list.is_null() {
            return Vec::new();
        }
        let mut out = Vec::new();
        let mut at = 0;
        loop {
            let entry = unsafe { *list.add(at) };
            if entry.is_null() {
                break;
            }
            out.push(unsafe { CStr::from_ptr(entry) }.to_string_lossy().into_owned());
            unsafe { libc::free(entry.cast()) };
            at += 1;
        }
        unsafe { libc::free(list.cast()) };
        out
    }
}

impl Drop for Pam {
    fn drop(&mut self) {
        unsafe {
            if self.open {
                pam_close_session(self.handle, 0);
            }
            if self.cred {
                pam_setcred(self.handle, PAM_DELETE_CRED);
            }
            pam_end(self.handle, self.status);
            drop(Box::from_raw(self.talker));
        }
    }
}

extern "C" fn talk(count: c_int, messages: *mut *const Message, replies: *mut *mut Response, data: *mut c_void) -> c_int {
    if count <= 0 || messages.is_null() || replies.is_null() || data.is_null() {
        return PAM_CONV_ERR;
    }
    let count = count as usize;
    let talker = unsafe { &mut *(data as *mut Talker) };
    let table = unsafe { libc::calloc(count, std::mem::size_of::<Response>()) }.cast::<Response>();
    if table.is_null() {
        return PAM_BUF_ERR;
    }
    for index in 0..count {
        let message = unsafe { &**messages.add(index) };
        let text = match message.text.is_null() {
            true => String::new(),
            false => unsafe { CStr::from_ptr(message.text) }.to_string_lossy().into_owned(),
        };
        let style = match message.style {
            PAM_PROMPT_ECHO_OFF => Style::Secret,
            PAM_PROMPT_ECHO_ON => Style::Visible,
            PAM_ERROR_MSG => Style::Error,
            _ => Style::Info,
        };
        let answer = (talker.ask)(style, &text);
        if matches!(style, Style::Secret | Style::Visible) {
            let Some(mut answer) = answer else {
                wipe(table, count);
                return PAM_CONV_ERR;
            };
            let length = answer.len();
            let copy = unsafe { libc::calloc(length + 1, 1) }.cast::<u8>();
            if copy.is_null() {
                answer.zeroize();
                wipe(table, count);
                return PAM_BUF_ERR;
            }
            unsafe {
                ptr::copy_nonoverlapping(answer.as_ptr(), copy, length);
                (*table.add(index)).text = copy.cast();
            }
            answer.zeroize();
        }
    }
    unsafe { *replies = table };
    PAM_SUCCESS
}

fn wipe(table: *mut Response, count: usize) {
    for index in 0..count {
        let text = unsafe { (*table.add(index)).text };
        if !text.is_null() {
            unsafe {
                ptr::write_bytes(text, 0, libc::strlen(text));
                libc::free(text.cast());
            }
        }
    }
    unsafe { libc::free(table.cast()) };
}
