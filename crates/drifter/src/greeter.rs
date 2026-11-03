
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use zeroize::Zeroizing;

use crate::auth::Verdict;

const SOCKET: &str = "GREETD_SOCK";
const WAYLAND_SESSIONS: &str = "/usr/share/wayland-sessions";
const OURS: &str = "drifter";
const LONGEST_REPLY: usize = 1 << 20;
pub const FIRST_SESSION_ID: i32 = 300;

pub fn active() -> bool {
    std::env::var_os(SOCKET).is_some_and(|socket| !socket.is_empty())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub exec: Vec<String>,
    pub desktops: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub login: String,
    pub shown: String,
}

pub struct Greeter {
    pub people: Vec<Person>,
    pub person: usize,
    pub sessions: Vec<Session>,
    pub session: usize,
}

impl Greeter {
    pub fn start() -> Greeter {
        let passwd = std::fs::read_to_string("/etc/passwd").unwrap_or_default();
        let group = std::fs::read_to_string("/etc/group").unwrap_or_default();
        let people = people(&passwd, &group);
        let sessions = sessions_in(Path::new(WAYLAND_SESSIONS));
        let (last_person, last_session) = remembered();
        let person = last_person
            .and_then(|login| people.iter().position(|each| each.login == login))
            .unwrap_or(0);
        let session = last_session
            .and_then(|id| sessions.iter().position(|each| each.id == id))
            .or_else(|| sessions.iter().position(|each| each.id == OURS))
            .unwrap_or(0);
        out!("greeter     {} people, {} sessions", people.len(), sessions.len());
        Greeter {
            people,
            person,
            sessions,
            session,
        }
    }

    pub fn person(&self) -> Option<&Person> {
        self.people.get(self.person)
    }

    pub fn session(&self) -> Option<&Session> {
        self.sessions.get(self.session)
    }

    pub fn step(&mut self, forward: bool) -> bool {
        let count = self.people.len();
        if count < 2 {
            return false;
        }
        self.person = match forward {
            true => (self.person + 1) % count,
            false => (self.person + count - 1) % count,
        };
        true
    }

    pub fn title(&self) -> String {
        let name = self.person().map_or("", |person| person.shown.as_str());
        let mut line = match self.people.len() > 1 {
            true => format!("‹  {name}  ›"),
            false => name.to_string(),
        };
        if let Some(session) = self.session().filter(|session| session.id != OURS) {
            line.push_str(&format!("  ·  {}", session.name));
        }
        line
    }

    pub fn menu(&self) -> Vec<drifter_core::menu::Item> {
        use drifter_core::menu::Item;
        if self.sessions.len() < 2 {
            return Vec::new();
        }
        let mut items = vec![Item::section(drifter_core::lang::t("Session"))];
        for (at, session) in self.sessions.iter().enumerate() {
            let mut item = Item::line(FIRST_SESSION_ID + at as i32, &session.name);
            item.trail = drifter_core::rows::Trail::Check(at == self.session);
            items.push(item);
        }
        items.push(Item::rule(FIRST_SESSION_ID - 1));
        items
    }

    pub fn pick(&mut self, id: i32) -> bool {
        let Ok(at) = usize::try_from(id - FIRST_SESSION_ID) else {
            return false;
        };
        if at >= self.sessions.len() {
            return false;
        }
        self.session = at;
        true
    }

    pub fn remember(&self) {
        let (Some(person), Some(session)) = (self.person(), self.session()) else {
            return;
        };
        let file = memory_file();
        let text = format!("{}\n{}\n", person.login, session.id);
        let written = file
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|()| std::fs::write(&file, text));
        if let Err(error) = written {
            log!("drifter: the login screen cannot remember who logged in: {error}");
        }
    }
}

pub fn people(passwd: &str, group: &str) -> Vec<Person> {
    let mut found: Vec<Person> = drifter_core::users::people(passwd, group)
        .into_iter()
        .map(|user| Person {
            shown: user.shown().to_string(),
            login: user.login,
        })
        .collect();
    found.sort_by_key(|person| person.shown.to_lowercase());
    found
}

pub fn sessions_in(folder: &Path) -> Vec<Session> {
    let mut found: Vec<Session> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "desktop"))
        .filter_map(|path| {
            let id = path.file_stem()?.to_string_lossy().into_owned();
            let text = std::fs::read_to_string(&path).ok()?;
            session(&id, &text)
        })
        .collect();
    found.sort_by(|a, b| (a.id != OURS, a.name.to_lowercase()).cmp(&(b.id != OURS, b.name.to_lowercase())));
    found.dedup_by(|a, b| a.name == b.name);
    found
}

pub fn session(id: &str, text: &str) -> Option<Session> {
    let mut inside = false;
    let (mut name, mut exec, mut desktops) = (None, None, None);
    let mut hidden = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            inside = line == "[Desktop Entry]";
            continue;
        }
        let Some((key, value)) = line.split_once('=').filter(|_| inside) else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim() {
            "Name" => name = Some(value),
            "Exec" => exec = Some(value),
            "DesktopNames" => desktops = Some(value),
            "Hidden" | "NoDisplay" if value == "true" => hidden = true,
            _ => {}
        }
    }
    if hidden {
        return None;
    }
    let exec = drifter_core::exec::command(&exec?, &[]).filter(|argv| !argv.is_empty())?;
    Some(Session {
        id: id.to_string(),
        name: name.unwrap_or_else(|| id.to_string()),
        exec,
        desktops: desktops
            .unwrap_or_else(|| id.to_string())
            .split(';')
            .filter(|each| !each.is_empty())
            .collect::<Vec<_>>()
            .join(":"),
    })
}

fn memory_file() -> PathBuf {
    drifter_core::Config::state_dir().join("greeter")
}

fn remembered() -> (Option<String>, Option<String>) {
    let text = std::fs::read_to_string(memory_file()).unwrap_or_default();
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty()).map(str::to_string);
    (lines.next(), lines.next())
}

pub fn environment(session: &Session) -> Vec<String> {
    vec![
        "XDG_SESSION_TYPE=wayland".to_string(),
        format!("XDG_SESSION_DESKTOP={}", session.id),
        format!("XDG_CURRENT_DESKTOP={}", session.desktops),
    ]
}

pub fn log_in(login: &str, secret: Vec<u8>, session: &Session) -> Verdict {
    let mut secret = Zeroizing::new(secret);
    while secret.last() == Some(&0) {
        secret.pop();
    }
    let Ok(password) = std::str::from_utf8(&secret).map(|text| Zeroizing::new(text.to_string())) else {
        return Verdict::Denied;
    };
    let Some(path) = std::env::var_os(SOCKET) else {
        return Verdict::Broken(format!("{SOCKET} is not set"));
    };
    let mut talk = match UnixStream::connect(&path) {
        Ok(stream) => Talk { stream },
        Err(error) => return Verdict::Broken(format!("cannot reach greetd: {error}")),
    };
    let verdict = talk.log_in(login, &password, session);
    if verdict != Verdict::Granted {
        let _ = talk.ask(&json!({ "type": "cancel_session" }));
    }
    verdict
}

struct Talk {
    stream: UnixStream,
}

impl Talk {
    fn log_in(&mut self, login: &str, password: &str, session: &Session) -> Verdict {
        let mut reply = match self.ask(&json!({ "type": "create_session", "username": login })) {
            Ok(reply) => reply,
            Err(error) => return Verdict::Broken(error),
        };
        let mut answered = false;
        loop {
            match Reply::read(&reply) {
                Reply::Success => break,
                Reply::Denied => return Verdict::Denied,
                Reply::Error(error) => return Verdict::Broken(error),
                Reply::Ask(secret) => {
                    if answered {
                        return Verdict::Broken(format!(
                            "the login asks for more than a password ({secret}); Drifter's login screen cannot answer it"
                        ));
                    }
                    answered = true;
                    let mut message = Zeroizing::new(
                        json!({ "type": "post_auth_message_response", "response": password }).to_string(),
                    );
                    reply = match self.ask_text(&message) {
                        Ok(reply) => reply,
                        Err(error) => return Verdict::Broken(error),
                    };
                    message.clear();
                }
                Reply::Told(text) => {
                    log!("drifter: the login says: {text}");
                    reply = match self.ask(&json!({ "type": "post_auth_message_response" })) {
                        Ok(reply) => reply,
                        Err(error) => return Verdict::Broken(error),
                    };
                }
            }
        }
        let started = self.ask(&json!({
            "type": "start_session",
            "cmd": session.exec,
            "env": environment(session),
        }));
        match started.map(|reply| Reply::read(&reply)) {
            Ok(Reply::Success) => Verdict::Granted,
            Ok(Reply::Error(error)) => Verdict::Broken(format!("greetd cannot start {}: {error}", session.name)),
            Ok(_) => Verdict::Broken(format!("greetd did not start {}", session.name)),
            Err(error) => Verdict::Broken(error),
        }
    }

    fn ask(&mut self, message: &Value) -> Result<Value, String> {
        self.ask_text(&message.to_string())
    }

    fn ask_text(&mut self, message: &str) -> Result<Value, String> {
        let length = u32::try_from(message.len()).map_err(|_| "the message is too long".to_string())?;
        self.stream
            .write_all(&length.to_ne_bytes())
            .and_then(|()| self.stream.write_all(message.as_bytes()))
            .map_err(|error| format!("cannot talk to greetd: {error}"))?;
        let mut head = [0u8; 4];
        self.stream
            .read_exact(&mut head)
            .map_err(|error| format!("greetd did not answer: {error}"))?;
        let length = u32::from_ne_bytes(head) as usize;
        if length > LONGEST_REPLY {
            return Err(format!("greetd sent {length} bytes"));
        }
        let mut body = vec![0u8; length];
        self.stream
            .read_exact(&mut body)
            .map_err(|error| format!("greetd did not finish its answer: {error}"))?;
        serde_json::from_slice(&body).map_err(|error| format!("greetd sent something unclear: {error}"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Reply {
    Success,
    Denied,
    Error(String),
    Ask(String),
    Told(String),
}

impl Reply {
    fn read(reply: &Value) -> Reply {
        let text = |key: &str| reply.get(key).and_then(Value::as_str).unwrap_or("").to_string();
        match reply.get("type").and_then(Value::as_str) {
            Some("success") => Reply::Success,
            Some("error") if text("error_type") == "auth_error" => Reply::Denied,
            Some("error") => Reply::Error(text("description")),
            Some("auth_message") => match text("auth_message_type").as_str() {
                "secret" | "visible" => Reply::Ask(text("auth_message")),
                _ => Reply::Told(text("auth_message")),
            },
            _ => Reply::Error(format!("greetd sent {reply}")),
        }
    }
}

