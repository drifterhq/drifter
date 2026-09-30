use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Command, Stdio};

use drifter_core::prompt::{self, Helper};
use smithay::reexports::calloop::channel::{
    Channel, Sender as LoopSender, channel as loop_channel,
};
use zbus::zvariant::{OwnedValue, Value};
use zeroize::Zeroizing;

const PATH: &str = "/org/drifter/PolicyKit1/AuthenticationAgent";
const HELPER: &str = "/usr/lib/polkit-1/polkit-agent-helper-1";

pub struct Request {
    pub cookie: String,
    pub message: String,
    pub user: String,
    pub reply: async_channel::Sender<bool>,
}

pub enum Event {
    Begin(Request),
    Cancel(String),
}

pub fn start() -> Result<Channel<Event>, String> {
    let (tx, rx) = loop_channel::<Event>();
    std::thread::Builder::new()
        .name("drifter-polkit".to_string())
        .spawn(move || {
            if let Err(error) = serve(tx) {
                eprintln!("drifter: no password prompt: {error}");
            }
        })
        .map_err(|e| format!("cannot start the polkit thread: {e}"))?;
    Ok(rx)
}

fn session_id(bus: &zbus::blocking::Connection) -> Result<String, String> {
    if let Ok(id) = std::env::var("XDG_SESSION_ID")
        && !id.is_empty()
    {
        return Ok(id);
    }
    let path: zbus::zvariant::OwnedObjectPath = bus
        .call_method(
            Some("org.freedesktop.login1"),
            "/org/freedesktop/login1",
            Some("org.freedesktop.login1.Manager"),
            "GetSessionByPID",
            &(std::process::id(),),
        )
        .and_then(|reply| reply.body().deserialize())
        .map_err(|e| format!("no login session: {e}"))?;
    let value: OwnedValue = bus
        .call_method(
            Some("org.freedesktop.login1"),
            path.as_str(),
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.freedesktop.login1.Session", "Id"),
        )
        .and_then(|reply| reply.body().deserialize())
        .map_err(|e| format!("no login session: {e}"))?;
    String::try_from(value).map_err(|e| format!("no login session: {e}"))
}

fn serve(events: LoopSender<Event>) -> Result<(), String> {
    let bus = zbus::blocking::connection::Builder::system()
        .and_then(|builder| builder.serve_at(PATH, Agent { events }))
        .and_then(|builder| builder.build())
        .map_err(|e| e.to_string())?;
    let session = session_id(&bus)?;
    let mut details: HashMap<&str, Value> = HashMap::new();
    details.insert("session-id", Value::from(session.as_str()));
    let locale = std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into());
    bus.call_method(
        Some("org.freedesktop.PolicyKit1"),
        "/org/freedesktop/PolicyKit1/Authority",
        Some("org.freedesktop.PolicyKit1.Authority"),
        "RegisterAuthenticationAgent",
        &(("unix-session", details), locale.as_str(), PATH),
    )
    .map_err(|e| format!("polkit refused the agent for session {session}: {e}"))?;
    println!("polkit      prompts for session {session}");
    loop {
        std::thread::park();
    }
}

struct Agent {
    events: LoopSender<Event>,
}

#[zbus::interface(name = "org.freedesktop.PolicyKit1.AuthenticationAgent")]
impl Agent {
    async fn begin_authentication(
        &self,
        _action_id: String,
        message: String,
        _icon_name: String,
        _details: HashMap<String, String>,
        cookie: String,
        identities: Vec<(String, HashMap<String, OwnedValue>)>,
    ) -> zbus::fdo::Result<()> {
        let me = unsafe { libc::getuid() };
        let uids: Vec<u32> = identities
            .iter()
            .filter(|(kind, _)| kind == "unix-user")
            .filter_map(|(_, fields)| fields.get("uid")?.downcast_ref::<u32>().ok())
            .collect();
        let uid = uids
            .iter()
            .copied()
            .find(|uid| *uid == me)
            .or_else(|| uids.first().copied())
            .ok_or_else(|| zbus::fdo::Error::Failed("no user to ask for".into()))?;
        let user = crate::auth::user_of(uid)
            .map_err(zbus::fdo::Error::Failed)?
            .to_string_lossy()
            .into_owned();
        let (reply, answer) = async_channel::bounded(1);
        self.events
            .send(Event::Begin(Request {
                cookie,
                message,
                user,
                reply,
            }))
            .map_err(|_| zbus::fdo::Error::Failed("drifter is gone".into()))?;
        match answer.recv().await {
            Ok(true) => Ok(()),
            _ => Err(zbus::fdo::Error::Failed("cancelled".into())),
        }
    }

    fn cancel_authentication(&self, cookie: String) {
        let _ = self.events.send(Event::Cancel(cookie));
    }
}

pub enum Outcome {
    Granted,
    Refused(String),
}

pub fn check(user: &str, cookie: &str, password: Zeroizing<Vec<u8>>) -> Outcome {
    let spawned = Command::new(HELPER)
        .arg(user)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(error) => return Outcome::Refused(format!("cannot run {HELPER}: {error}")),
    };
    let (Some(mut input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
        return Outcome::Refused("the polkit helper has no pipes".into());
    };
    if writeln!(input, "{cookie}").is_err() {
        return Outcome::Refused("the polkit helper did not take the cookie".into());
    }
    let mut said = false;
    let mut last_error = None;
    let mut outcome = Outcome::Refused("Wrong password. Try again.".into());
    for line in BufReader::new(output).lines() {
        let Ok(line) = line else {
            break;
        };
        match prompt::read_helper_line(&line) {
            Helper::Secret(_) | Helper::Plain(_) if !said => {
                said = true;
                let mut secret: Zeroizing<Vec<u8>> =
                    Zeroizing::new(password.iter().copied().take_while(|b| *b != 0).collect());
                secret.push(b'\n');
                if input.write_all(&secret).is_err() {
                    break;
                }
            }
            Helper::Secret(_) | Helper::Plain(_) => break,
            Helper::Error(message) => last_error = Some(message),
            Helper::Info(_) => {}
            Helper::Success => {
                outcome = Outcome::Granted;
                break;
            }
            Helper::Failure => {
                if let Some(message) = last_error.take()
                    && !said
                {
                    outcome = Outcome::Refused(message);
                }
                break;
            }
        }
    }
    drop(input);
    let _ = child.wait();
    outcome
}
