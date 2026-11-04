use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use zeroize::Zeroize;

use crate::worker::account;

const CONFIG: &str = "/etc/drifter/login.conf";
const RUN: &str = "/run/drifter-login";
const SOCKET: &str = "/run/drifter-login/greeter.sock";
const VT_OPENQRY: libc::c_ulong = 0x5600;
const VT_ACTIVATE: libc::c_ulong = 0x5606;
const HANDOFF: Duration = Duration::from_secs(30);
const LONGEST: usize = 1 << 20;

struct Config {
    greeter: Vec<String>,
    user: String,
    vt: u32,
}

fn config() -> Config {
    let mut config = Config {
        greeter: vec!["/usr/bin/drifter".to_string()],
        user: "drifter-greeter".to_string(),
        vt: 1,
    };
    let text = std::fs::read_to_string(CONFIG).unwrap_or_default();
    for line in text.lines().map(str::trim).filter(|line| !line.starts_with('#')) {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"');
        match key.trim() {
            "greeter" => config.greeter = value.split_whitespace().map(str::to_string).collect(),
            "user" => config.user = value.to_string(),
            "vt" => config.vt = value.parse().unwrap_or(1),
            _ => {}
        }
    }
    config
}

enum Event {
    Greeter(i32),
    GreeterGone,
    Session(u32, Worker),
    Handoff,
    SessionGone(u32),
}

struct Worker {
    child: Child,
    reader: BufReader<UnixStream>,
    writer: UnixStream,
}

impl Worker {
    fn spawn() -> Result<Worker, String> {
        let (ours, theirs) = UnixStream::pair().map_err(|e| e.to_string())?;
        let fd = theirs.as_raw_fd();
        let me = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut command = Command::new(me);
        command.env("DRIFTER_LOGIN_WORKER", "3").stdin(Stdio::null());
        unsafe {
            command.pre_exec(move || {
                let moved = match fd == 3 {
                    true => libc::fcntl(3, libc::F_SETFD, 0),
                    false => libc::dup2(fd, 3),
                };
                if moved < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = command.spawn().map_err(|e| format!("cannot start a login worker: {e}"))?;
        drop(theirs);
        let writer = ours.try_clone().map_err(|e| e.to_string())?;
        Ok(Worker {
            child,
            reader: BufReader::new(ours),
            writer,
        })
    }

    fn send(&mut self, value: Value) -> Result<(), String> {
        let mut text = value.to_string();
        text.push('\n');
        let sent = self.writer.write_all(text.as_bytes()).map_err(|e| e.to_string());
        text.zeroize();
        sent
    }

    fn read(&mut self) -> Result<Value, String> {
        let mut text = String::new();
        let read = self.reader.read_line(&mut text).map_err(|e| e.to_string())?;
        if read == 0 {
            return Err("the login worker stopped".to_string());
        }
        serde_json::from_str(&text).map_err(|e| e.to_string())
    }

    fn wait_gone(mut self, tx: Sender<Event>, gone: Event) {
        std::thread::spawn(move || {
            let mut sink = String::new();
            while self.reader.read_line(&mut sink).is_ok_and(|read| read > 0) {
                sink.clear();
            }
            let _ = self.child.wait();
            let _ = tx.send(gone);
        });
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let _ = self.child.try_wait();
    }
}

fn console() -> Option<std::fs::File> {
    std::fs::OpenOptions::new().read(true).write(true).open("/dev/tty0").ok()
}

fn free_vt(not: u32) -> u32 {
    let mut found: libc::c_int = 0;
    if let Some(tty) = console()
        && unsafe { libc::ioctl(tty.as_raw_fd(), VT_OPENQRY, &mut found) } == 0
        && found > 0
        && found as u32 != not
    {
        return found as u32;
    }
    (2..64).find(|vt| *vt != not).unwrap_or(2)
}

fn activate(vt: u32) {
    if let Some(tty) = console()
        && unsafe { libc::ioctl(tty.as_raw_fd(), VT_ACTIVATE, vt as libc::c_int) } != 0
    {
        eprintln!("drifter-login: cannot switch to terminal {vt}: {}", std::io::Error::last_os_error());
    }
}

fn active_vt() -> Option<u32> {
    std::fs::read_to_string("/sys/class/tty/tty0/active").ok()?.trim().strip_prefix("tty")?.parse().ok()
}

fn start_greeter(config: &Config, tx: &Sender<Event>) -> Result<(), String> {
    let mut worker = Worker::spawn()?;
    worker.send(json!({
        "greeter": {
            "user": config.user,
            "vt": config.vt,
            "cmd": config.greeter,
            "env": [format!("GREETD_SOCK={SOCKET}"), "DRIFTER_LOGIN=1"],
        }
    }))?;
    let reply = worker.read()?;
    let Some(pid) = reply.get("started").and_then(Value::as_i64) else {
        return Err(format!("the login screen did not start: {reply}"));
    };
    let _ = tx.send(Event::Greeter(pid as i32));
    worker.wait_gone(tx.clone(), Event::GreeterGone);
    Ok(())
}

fn listen(config: &Config) -> Result<UnixListener, String> {
    std::fs::create_dir_all(RUN).map_err(|e| e.to_string())?;
    std::fs::set_permissions(RUN, std::fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(SOCKET);
    let listener = UnixListener::bind(SOCKET).map_err(|e| format!("cannot listen on {SOCKET}: {e}"))?;
    let who = account(&config.user).ok_or_else(|| format!("there is no {} account", config.user))?;
    let path = std::ffi::CString::new(SOCKET).map_err(|e| e.to_string())?;
    unsafe {
        libc::chown(path.as_ptr(), who.uid, who.gid);
    }
    std::fs::set_permissions(SOCKET, std::fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    Ok(listener)
}

fn read_message(stream: &mut UnixStream) -> Result<Value, String> {
    let mut head = [0u8; 4];
    stream.read_exact(&mut head).map_err(|e| e.to_string())?;
    let length = u32::from_ne_bytes(head) as usize;
    if length > LONGEST {
        return Err("too long".to_string());
    }
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body).map_err(|e| e.to_string())?;
    let value = serde_json::from_slice(&body).map_err(|e| e.to_string());
    body.zeroize();
    value
}

fn write_message(stream: &mut UnixStream, value: Value) -> Result<(), String> {
    let text = value.to_string();
    stream
        .write_all(&(text.len() as u32).to_ne_bytes())
        .and_then(|()| stream.write_all(text.as_bytes()))
        .map_err(|e| e.to_string())
}

fn failed(kind: &str, text: &str) -> Value {
    json!({ "type": "error", "error_type": kind, "description": text })
}

fn pump(worker: &mut Option<(Worker, u32)>) -> Value {
    let Some((current, _)) = worker.as_mut() else {
        return failed("error", "no session is being made");
    };
    let reply = match current.read() {
        Ok(reply) => reply,
        Err(error) => {
            *worker = None;
            return failed("error", &error);
        }
    };
    if let Some(ask) = reply.get("ask") {
        let style = ask.get("style").and_then(Value::as_str).unwrap_or("info");
        let text = ask.get("text").and_then(Value::as_str).unwrap_or("");
        return json!({ "type": "auth_message", "auth_message_type": style, "auth_message": text });
    }
    match reply.get("auth").and_then(Value::as_str) {
        Some("ok") => json!({ "type": "success" }),
        Some(_) => {
            *worker = None;
            failed("auth_error", reply.get("text").and_then(Value::as_str).unwrap_or("not allowed"))
        }
        None => {
            *worker = None;
            failed("error", reply.get("error").and_then(Value::as_str).unwrap_or("the login worker failed"))
        }
    }
}

fn started(worker: &mut Worker) -> Result<Value, String> {
    loop {
        let reply = worker.read()?;
        let Some(ask) = reply.get("ask") else {
            return Ok(reply);
        };
        match ask.get("style").and_then(Value::as_str) {
            Some("secret" | "visible") => worker.send(json!({ "answer": Value::Null }))?,
            _ => worker.send(json!({ "answer": "" }))?,
        }
    }
}

fn serve(mut stream: UnixStream, config_vt: u32, tx: &Sender<Event>) {
    let mut making: Option<(Worker, u32)> = None;
    while let Ok(mut request) = read_message(&mut stream) {
        let kind = request.get("type").and_then(Value::as_str).unwrap_or("").to_string();
        let reply = match kind.as_str() {
            "create_session" => {
                let user = request.get("username").and_then(Value::as_str).unwrap_or("").to_string();
                match (making.is_some(), account(&user)) {
                    (true, _) => failed("error", "a session is already being made"),
                    (false, None) => failed("auth_error", "no such account"),
                    (false, Some(_)) => {
                        let vt = free_vt(config_vt);
                        match Worker::spawn().and_then(|mut worker| {
                            worker.send(json!({ "login": { "user": user, "vt": vt } }))?;
                            Ok(worker)
                        }) {
                            Ok(worker) => {
                                making = Some((worker, vt));
                                pump(&mut making)
                            }
                            Err(error) => failed("error", &error),
                        }
                    }
                }
            }
            "post_auth_message_response" => {
                let answer = request.get("response").cloned().unwrap_or(Value::Null);
                let sent = making.as_mut().map(|(worker, _)| worker.send(json!({ "answer": answer })));
                if let Some(Value::String(secret)) = request.get_mut("response") {
                    secret.zeroize();
                }
                match sent {
                    Some(Ok(())) => pump(&mut making),
                    Some(Err(error)) => {
                        making = None;
                        failed("error", &error)
                    }
                    None => failed("error", "no session is being made"),
                }
            }
            "start_session" => match making.take() {
                Some((mut worker, vt)) => {
                    let cmd = request.get("cmd").cloned().unwrap_or(json!([]));
                    let env = request.get("env").cloned().unwrap_or(json!([]));
                    match worker.send(json!({ "start": { "cmd": cmd, "env": env } })).and_then(|()| started(&mut worker)) {
                        Ok(reply) if reply.get("started").is_some() => {
                            let _ = tx.send(Event::Session(vt, worker));
                            json!({ "type": "success" })
                        }
                        Ok(reply) => failed("error", reply.get("error").and_then(Value::as_str).unwrap_or("the session did not start")),
                        Err(error) => failed("error", &error),
                    }
                }
                None => failed("error", "no session is ready to start"),
            },
            "cancel_session" => {
                making = None;
                json!({ "type": "success" })
            }
            _ => failed("error", "unknown request"),
        };
        if write_message(&mut stream, reply).is_err() {
            break;
        }
    }
}

pub fn run() -> std::process::ExitCode {
    let config = config();
    let listener = match listen(&config) {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("drifter-login: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let (tx, rx) = channel::<Event>();
    let serving = tx.clone();
    let greeter_vt = config.vt;
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            serve(stream, greeter_vt, &serving);
        }
    });
    activate(config.vt);
    let mut greeter: Option<i32> = None;
    let mut sessions: BTreeSet<u32> = BTreeSet::new();
    let mut starts: Vec<Instant> = Vec::new();
    let mut want_greeter = true;
    loop {
        if want_greeter && greeter.is_none() {
            want_greeter = false;
            starts.retain(|when| when.elapsed() < Duration::from_secs(30));
            if starts.len() >= 5 {
                eprintln!("drifter-login: the login screen keeps failing, waiting before trying again");
                std::thread::sleep(Duration::from_secs(10));
                starts.clear();
            }
            starts.push(Instant::now());
            activate(config.vt);
            if let Err(error) = start_greeter(&config, &tx) {
                eprintln!("drifter-login: {error}");
                std::thread::sleep(Duration::from_secs(1));
                want_greeter = true;
                continue;
            }
        }
        let Ok(event) = rx.recv() else {
            break;
        };
        match event {
            Event::Greeter(pid) => greeter = Some(pid),
            Event::GreeterGone => {
                greeter = None;
                want_greeter = sessions.is_empty();
            }
            Event::Session(vt, worker) => {
                sessions.insert(vt);
                worker.wait_gone(tx.clone(), Event::SessionGone(vt));
                let handing = tx.clone();
                std::thread::spawn(move || {
                    let since = Instant::now();
                    while active_vt() != Some(vt) && since.elapsed() < HANDOFF {
                        std::thread::sleep(Duration::from_millis(20));
                    }
                    if active_vt() != Some(vt) {
                        activate(vt);
                    }
                    let _ = handing.send(Event::Handoff);
                });
            }
            Event::Handoff => {
                if let Some(pid) = greeter {
                    unsafe { libc::kill(pid, libc::SIGTERM) };
                }
            }
            Event::SessionGone(vt) => {
                sessions.remove(&vt);
                if sessions.is_empty() {
                    want_greeter = true;
                    if greeter.is_some() {
                        activate(config.vt);
                    }
                }
            }
        }
    }
    std::process::ExitCode::SUCCESS
}
