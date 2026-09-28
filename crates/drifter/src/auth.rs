use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::mem::size_of;
use std::ptr;

use zeroize::Zeroize;

const SERVICE: &CStr = c"login";
const STACK: &str = "/etc/pam.d/login";

const PAM_SUCCESS: c_int = 0;
const PAM_CONV_ERR: c_int = 19;
const PAM_BUF_ERR: c_int = 5;
const PAM_NEW_AUTHTOK_REQD: c_int = 12;
const PAM_DISALLOW_NULL_AUTHTOK: c_int = 1;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_ERROR_MSG: c_int = 3;
const PAM_TEXT_INFO: c_int = 4;

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
    fn pam_start(
        service: *const c_char,
        user: *const c_char,
        conversation: *const Conversation,
        handle: *mut *mut Handle,
    ) -> c_int;
    fn pam_authenticate(handle: *mut Handle, flags: c_int) -> c_int;
    fn pam_acct_mgmt(handle: *mut Handle, flags: c_int) -> c_int;
    fn pam_end(handle: *mut Handle, status: c_int) -> c_int;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Granted,
    Denied,
    Broken(String),
}

pub fn user() -> Result<CString, String> {
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut found: *mut libc::passwd = ptr::null_mut();
    let mut buffer = vec![0u8; 4096];
    let uid = unsafe { libc::getuid() };
    let code = unsafe {
        libc::getpwuid_r(
            uid,
            &mut entry,
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut found,
        )
    };
    if code != 0 || found.is_null() {
        return Err(format!("no account for uid {uid}"));
    }
    let name = unsafe { CStr::from_ptr(entry.pw_name) };
    Ok(name.to_owned())
}

pub fn ready() -> Result<CString, String> {
    if !std::path::Path::new(STACK).is_file() {
        return Err(format!("{STACK} is missing, so no password can be checked"));
    }
    let user = user()?;
    let mut handle: *mut Handle = ptr::null_mut();
    let conversation = Conversation {
        talk: refuse,
        data: ptr::null_mut(),
    };
    let code = unsafe { pam_start(SERVICE.as_ptr(), user.as_ptr(), &conversation, &mut handle) };
    if code != PAM_SUCCESS || handle.is_null() {
        return Err(format!("pam_start failed with code {code}"));
    }
    unsafe { pam_end(handle, PAM_SUCCESS) };
    Ok(user)
}

pub fn verify(user: &CStr, password: Vec<u8>) -> Verdict {
    let mut secret = Secret { bytes: password };
    let conversation = Conversation {
        talk: answer,
        data: (&mut secret as *mut Secret).cast(),
    };
    let mut handle: *mut Handle = ptr::null_mut();
    let code = unsafe { pam_start(SERVICE.as_ptr(), user.as_ptr(), &conversation, &mut handle) };
    if code != PAM_SUCCESS || handle.is_null() {
        return Verdict::Broken(format!("pam_start failed with code {code}"));
    }

    let mut code = unsafe { pam_authenticate(handle, PAM_DISALLOW_NULL_AUTHTOK) };
    if code == PAM_SUCCESS {
        code = unsafe { pam_acct_mgmt(handle, 0) };
        if code == PAM_NEW_AUTHTOK_REQD {
            code = PAM_SUCCESS;
        }
    }
    unsafe { pam_end(handle, code) };

    match code {
        PAM_SUCCESS => Verdict::Granted,
        _ => Verdict::Denied,
    }
}

struct Secret {
    bytes: Vec<u8>,
}

impl Drop for Secret {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}

extern "C" fn refuse(
    _count: c_int,
    _messages: *mut *const Message,
    _replies: *mut *mut Response,
    _data: *mut c_void,
) -> c_int {
    PAM_CONV_ERR
}

extern "C" fn answer(
    count: c_int,
    messages: *mut *const Message,
    replies: *mut *mut Response,
    data: *mut c_void,
) -> c_int {
    if count <= 0 || messages.is_null() || replies.is_null() || data.is_null() {
        return PAM_CONV_ERR;
    }
    let count = count as usize;
    let secret = unsafe { &*(data as *const Secret) };

    for index in 0..count {
        let message = unsafe { &**messages.add(index) };
        if !matches!(
            message.style,
            PAM_PROMPT_ECHO_OFF | PAM_ERROR_MSG | PAM_TEXT_INFO
        ) {
            return PAM_CONV_ERR;
        }
    }

    let table = unsafe { libc::calloc(count, size_of::<Response>()) }.cast::<Response>();
    if table.is_null() {
        return PAM_BUF_ERR;
    }

    for index in 0..count {
        let message = unsafe { &**messages.add(index) };
        if message.style != PAM_PROMPT_ECHO_OFF {
            continue;
        }
        let copy = unsafe { libc::malloc(secret.bytes.len()) }.cast::<u8>();
        if copy.is_null() {
            wipe_table(table, count);
            return PAM_BUF_ERR;
        }
        unsafe {
            ptr::copy_nonoverlapping(secret.bytes.as_ptr(), copy, secret.bytes.len());
            (*table.add(index)).text = copy.cast();
        }
    }

    unsafe { *replies = table };
    PAM_SUCCESS
}

fn wipe_table(table: *mut Response, count: usize) {
    for index in 0..count {
        let text = unsafe { (*table.add(index)).text };
        if !text.is_null() {
            let length = unsafe { libc::strlen(text) };
            unsafe {
                ptr::write_bytes(text, 0, length);
                libc::free(text.cast());
            }
        }
    }
    unsafe { libc::free(table.cast()) };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed(text: &str) -> Vec<u8> {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0);
        bytes
    }

    #[test]
    fn the_login_stack_is_ready_for_the_current_user() {
        assert!(ready().is_ok());
    }

    #[test]
    fn a_wrong_password_is_denied() {
        let user = user().unwrap();
        assert_eq!(verify(&user, typed("definitely not it")), Verdict::Denied);
    }

    #[test]
    fn an_empty_password_is_denied() {
        let user = user().unwrap();
        assert_eq!(verify(&user, typed("")), Verdict::Denied);
    }

    #[test]
    fn a_password_with_a_trailing_space_is_not_the_password() {
        let Ok(real) = std::env::var("DRIFTER_TEST_PASSWORD") else {
            return;
        };
        let user = user().unwrap();
        assert_eq!(verify(&user, typed(&format!("{real} "))), Verdict::Denied);
    }

    #[test]
    fn the_right_password_is_granted() {
        let Ok(real) = std::env::var("DRIFTER_TEST_PASSWORD") else {
            return;
        };
        let user = user().unwrap();
        assert_eq!(verify(&user, typed(&real)), Verdict::Granted);
    }

    #[test]
    fn an_unknown_account_is_never_granted() {
        let nobody = CString::new("no-such-account-drifter").unwrap();
        assert_ne!(verify(&nobody, typed("anything")), Verdict::Granted);
    }
}
