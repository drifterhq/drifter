use std::collections::BTreeMap;
use std::ffi::{CStr, CString};
use std::io::{BufRead, BufReader, Write};
use std::os::fd::FromRawFd;
use std::os::unix::net::UnixStream;

use serde_json::{Value, json};

use crate::pam::{Pam, Style};

pub const GREETER_SERVICE: &str = "drifter-greeter";
pub const LOGIN_SERVICE: &str = "drifter-login";
const PATH: &str = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin";

pub struct Account {
    pub name: String,
    pub uid: libc::uid_t,
    pub gid: libc::gid_t,
    pub home: String,
    pub shell: String,
}

pub fn account(name: &str) -> Option<Account> {
    let wanted = CString::new(name).ok()?;
    let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
    let mut found: *mut libc::passwd = std::ptr::null_mut();
    let mut buffer = vec![0u8; 16384];
    let code = unsafe { libc::getpwnam_r(wanted.as_ptr(), &mut entry, buffer.as_mut_ptr().cast(), buffer.len(), &mut found) };
    if code != 0 || found.is_null() {
        return None;
    }
    let text = |field: *const libc::c_char| match field.is_null() {
        true => String::new(),
        false => unsafe { CStr::from_ptr(field) }.to_string_lossy().into_owned(),
    };
    let shell = text(entry.pw_shell);
    Some(Account {
        name: text(entry.pw_name),
        uid: entry.pw_uid,
        gid: entry.pw_gid,
        home: text(entry.pw_dir),
        shell: match shell.is_empty() {
            true => "/bin/sh".to_string(),
            false => shell,
        },
    })
}

struct Line {
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Line {
    fn send(&mut self, value: Value) -> Result<(), String> {
        let mut text = value.to_string();
        text.push('\n');
        self.writer.write_all(text.as_bytes()).map_err(|e| format!("the login service went away: {e}"))
    }

    fn read(&mut self) -> Result<Value, String> {
        let mut text = String::new();
        let read = self.reader.read_line(&mut text).map_err(|e| e.to_string())?;
        if read == 0 {
            return Err("the login service went away".to_string());
        }
        let value = serde_json::from_str(&text).map_err(|e| e.to_string());
        zeroize::Zeroize::zeroize(&mut text);
        value
    }
}

pub fn run(fd: i32) -> std::process::ExitCode {
    let stream = unsafe { UnixStream::from_raw_fd(fd) };
    let Ok(writer) = stream.try_clone() else {
        return std::process::ExitCode::FAILURE;
    };
    let line = Line {
        reader: BufReader::new(stream),
        writer,
    };
    let line = std::rc::Rc::new(std::cell::RefCell::new(line));
    match work(&line) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("drifter-login worker: {error}");
            let _ = line.borrow_mut().send(json!({ "error": error }));
            std::process::ExitCode::FAILURE
        }
    }
}

fn work(line: &std::rc::Rc<std::cell::RefCell<Line>>) -> Result<(), String> {
    let job = line.borrow_mut().read()?;
    if let Some(greeter) = job.get("greeter") {
        let user = text(greeter, "user");
        let vt = greeter.get("vt").and_then(Value::as_u64).unwrap_or(1) as u32;
        let who = account(&user).ok_or_else(|| format!("no account {user}"))?;
        let mut pam = Pam::start(GREETER_SERVICE, &user, Box::new(|_, _| None))?;
        pam.account()?;
        return launch(line, &mut pam, &who, vt, "greeter", list(greeter, "cmd"), list(greeter, "env"));
    }
    let Some(login) = job.get("login") else {
        return Err("no job".to_string());
    };
    let user = text(login, "user");
    let vt = login.get("vt").and_then(Value::as_u64).unwrap_or(2) as u32;
    let who = account(&user).ok_or_else(|| format!("no account {user}"))?;
    let asking = line.clone();
    let ask = Box::new(move |style: Style, said: &str| -> Option<String> {
        let mut line = asking.borrow_mut();
        line.send(json!({ "ask": { "style": style.word(), "text": said } })).ok()?;
        let reply = line.read().ok()?;
        reply.get("answer").and_then(Value::as_str).map(str::to_string)
    });
    let mut pam = Pam::start(LOGIN_SERVICE, &user, ask)?;
    match pam.authenticate() {
        Ok(()) => line.borrow_mut().send(json!({ "auth": "ok" }))?,
        Err(error) => {
            line.borrow_mut().send(json!({ "auth": "denied", "text": error }))?;
            return Ok(());
        }
    }
    let order = line.borrow_mut().read()?;
    let Some(start) = order.get("start") else {
        return Ok(());
    };
    launch(line, &mut pam, &who, vt, "user", list(start, "cmd"), list(start, "env"))
}

fn text(value: &Value, key: &str) -> String {
    value.get(key).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn list(value: &Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

fn launch(
    line: &std::rc::Rc<std::cell::RefCell<Line>>,
    pam: &mut Pam,
    who: &Account,
    vt: u32,
    class: &str,
    cmd: Vec<String>,
    asked: Vec<String>,
) -> Result<(), String> {
    if cmd.is_empty() {
        return Err("no command to start".to_string());
    }
    let tty = format!("/dev/tty{vt}");
    pam.tty(&tty)?;
    let mut env: BTreeMap<String, String> = BTreeMap::new();
    let mut set = |pair: &str| {
        if let Some((key, value)) = pair.split_once('=') {
            env.insert(key.to_string(), value.to_string());
        }
    };
    for pair in [
        "XDG_SESSION_TYPE=wayland".to_string(),
        format!("XDG_SESSION_CLASS={class}"),
        "XDG_SEAT=seat0".to_string(),
        format!("XDG_VTNR={vt}"),
    ] {
        set(&pair);
    }
    for pair in &asked {
        set(pair);
    }
    for key in ["XDG_SESSION_TYPE", "XDG_SESSION_CLASS", "XDG_SEAT", "XDG_VTNR", "XDG_SESSION_DESKTOP"] {
        if let Some(value) = env.get(key) {
            pam.put(&format!("{key}={value}"))?;
        }
    }
    pam.open()?;
    for pair in pam.environment() {
        set(&pair);
    }
    for pair in [
        format!("HOME={}", who.home),
        format!("USER={}", who.name),
        format!("LOGNAME={}", who.name),
        format!("SHELL={}", who.shell),
        format!("PATH={PATH}"),
    ] {
        set(&pair);
    }
    let mut argv = vec![who.shell.clone(), "-l".to_string(), "-c".to_string(), "exec \"$@\"".to_string(), "drifter-login".to_string()];
    argv.extend(cmd);
    let pid = spawn(who, &argv, &env)?;
    line.borrow_mut().send(json!({ "started": pid }))?;
    let mut status = 0;
    loop {
        let done = unsafe { libc::waitpid(pid, &mut status, 0) };
        if done == pid || (done < 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR)) {
            break;
        }
    }
    Ok(())
}

fn spawn(who: &Account, argv: &[String], env: &BTreeMap<String, String>) -> Result<libc::pid_t, String> {
    let cstrings = |items: Vec<String>| items.into_iter().filter_map(|item| CString::new(item).ok()).collect::<Vec<_>>();
    let program = CString::new(who.shell.clone()).map_err(|_| "bad shell".to_string())?;
    let args = cstrings(argv.to_vec());
    let pairs = cstrings(env.iter().map(|(key, value)| format!("{key}={value}")).collect());
    let mut arg_ptrs: Vec<*const libc::c_char> = args.iter().map(|arg| arg.as_ptr()).collect();
    arg_ptrs.push(std::ptr::null());
    let mut env_ptrs: Vec<*const libc::c_char> = pairs.iter().map(|pair| pair.as_ptr()).collect();
    env_ptrs.push(std::ptr::null());
    let home = CString::new(who.home.clone()).unwrap_or_else(|_| c"/".to_owned());
    let name = CString::new(who.name.clone()).map_err(|_| "bad user name".to_string())?;
    let mut groups = vec![0 as libc::gid_t; 256];
    let mut count = groups.len() as libc::c_int;
    if unsafe { libc::getgrouplist(name.as_ptr(), who.gid, groups.as_mut_ptr(), &mut count) } < 0 {
        groups.resize(count.max(1) as usize, 0);
        unsafe { libc::getgrouplist(name.as_ptr(), who.gid, groups.as_mut_ptr(), &mut count) };
    }
    groups.truncate(count.max(0) as usize);
    let null = c"/dev/null";
    let pid = unsafe { libc::fork() };
    if pid < 0 {
        return Err(format!("cannot fork: {}", std::io::Error::last_os_error()));
    }
    if pid == 0 {
        unsafe {
            libc::setsid();
            if libc::setgroups(groups.len(), groups.as_ptr()) != 0 || libc::setgid(who.gid) != 0 || libc::setuid(who.uid) != 0 {
                libc::_exit(126);
            }
            if libc::chdir(home.as_ptr()) != 0 {
                libc::chdir(c"/".as_ptr());
            }
            let quiet = libc::open(null.as_ptr(), libc::O_RDWR);
            if quiet >= 0 {
                libc::dup2(quiet, 0);
            }
            libc::execve(program.as_ptr(), arg_ptrs.as_ptr(), env_ptrs.as_ptr());
            libc::_exit(127);
        }
    }
    Ok(pid)
}
