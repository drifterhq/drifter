use std::collections::HashMap;

use drifter_core::settings::Update;
use zbus::blocking::{Connection, MessageIterator};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const KIT: &str = "org.freedesktop.PackageKit";
const ROOT: &str = "/org/freedesktop/PackageKit";
const TRANSACTION: &str = "org.freedesktop.PackageKit.Transaction";
pub const FILTER_NONE: u64 = 1 << 1;
pub const ONLY_TRUSTED: u64 = 1 << 1;
const PERCENT_UNKNOWN: u32 = 101;
const EXIT_SUCCESS: u32 = 1;
const EXIT_CANCELLED: u32 = 3;
const ERROR_NO_NETWORK: u32 = 2;
const ERROR_CANCELLED: u32 = 17;
const ERROR_NOT_AUTHORIZED: u32 = 48;
const HINTS: [&str; 1] = ["interactive=true"];

pub enum Kit {
    Missing,
    Percent(Option<u32>),
    Found(Vec<Update>, Option<String>),
    Installed(bool),
    Failed(String),
}

#[derive(Debug, PartialEq, Eq)]
enum Stop {
    Missing,
    Failed(String),
}

#[derive(Default)]
struct Heard {
    packages: Vec<Update>,
    restart: bool,
    error: Option<(u32, String)>,
    exit: u32,
}

pub fn check(tell: impl Fn(Kit)) {
    tell(match look() {
        Ok((found, note)) => Kit::Found(found, note),
        Err(stop) => stopped(stop),
    });
}

pub fn install(ids: Vec<String>, tell: impl Fn(Kit)) {
    let done = system().and_then(|bus| {
        let start = |path: &str| call(&bus, path, "UpdatePackages", &(ONLY_TRUSTED, ids.clone()));
        run(&bus, &start, &|percent| tell(Kit::Percent(percent)))
    });
    tell(match done {
        Ok(heard) => Kit::Installed(heard.restart),
        Err(stop) => stopped(stop),
    });
}

fn stopped(stop: Stop) -> Kit {
    match stop {
        Stop::Missing => Kit::Missing,
        Stop::Failed(error) => Kit::Failed(error),
    }
}

fn look() -> Result<(Vec<Update>, Option<String>), Stop> {
    let bus = system()?;
    let note = match run(&bus, &|path| call(&bus, path, "RefreshCache", &(false,)), &|_| {}) {
        Ok(_) => None,
        Err(Stop::Missing) => return Err(Stop::Missing),
        Err(Stop::Failed(error)) => Some(error),
    };
    let heard = run(&bus, &|path| call(&bus, path, "GetUpdates", &(FILTER_NONE,)), &|_| {})?;
    Ok((heard.packages, note))
}

fn system() -> Result<Connection, Stop> {
    Connection::system().map_err(|error| Stop::Failed(error.to_string()))
}

fn failed(error: impl std::fmt::Display) -> Stop {
    Stop::Failed(error.to_string())
}

fn stop(error: zbus::Error) -> Stop {
    let name = match &error {
        zbus::Error::MethodError(name, _, _) => name.to_string(),
        other => other.to_string(),
    };
    match missing(&name) {
        true => Stop::Missing,
        false => Stop::Failed(error.to_string()),
    }
}

fn missing(said: &str) -> bool {
    ["ServiceUnknown", "NameHasNoOwner", "not provided by any .service"]
        .iter()
        .any(|mark| said.contains(mark))
}

fn shown(raw: u32) -> Option<u32> {
    (raw < PERCENT_UNKNOWN).then_some(raw)
}

fn restarts(kind: u32) -> bool {
    matches!(kind, 3..=6)
}

fn failure(exit: u32, error: Option<&(u32, String)>) -> Option<String> {
    if exit == EXIT_SUCCESS {
        return None;
    }
    let said = match error {
        Some((ERROR_NOT_AUTHORIZED, _)) => drifter_core::lang::t("Not done. The admin password was not given."),
        Some((ERROR_NO_NETWORK, _)) => drifter_core::lang::t("No network. Connect to the internet, then try again."),
        Some((ERROR_CANCELLED, _)) => drifter_core::lang::t("Stopped before the end."),
        Some((_, details)) => match details.lines().map(str::trim).find(|line| !line.is_empty()) {
            Some(line) => return Some(line.to_string()),
            None => drifter_core::lang::t("It did not work."),
        },
        None if exit == EXIT_CANCELLED => drifter_core::lang::t("Stopped before the end."),
        None => drifter_core::lang::t("It did not work."),
    };
    Some(said.to_string())
}

fn call<B>(bus: &Connection, path: &str, method: &str, body: &B) -> zbus::Result<zbus::Message>
where
    B: zbus::export::serde::Serialize + zbus::zvariant::DynamicType,
{
    bus.call_method(Some(KIT), path, Some(TRANSACTION), method, body)
}

fn run(
    bus: &Connection,
    start: &dyn Fn(&str) -> zbus::Result<zbus::Message>,
    percent: &dyn Fn(Option<u32>),
) -> Result<Heard, Stop> {
    let made = bus
        .call_method(Some(KIT), ROOT, Some(KIT), "CreateTransaction", &())
        .map_err(stop)?;
    let path: OwnedObjectPath = made.body().deserialize().map_err(failed)?;
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(KIT)
        .map_err(failed)?
        .path(path.as_str())
        .map_err(failed)?
        .build();
    let messages = MessageIterator::for_match_rule(rule, bus, Some(1024)).map_err(failed)?;
    let _ = call(bus, path.as_str(), "SetHints", &(HINTS.to_vec(),));
    start(path.as_str()).map_err(stop)?;
    let mut heard = Heard::default();
    for message in messages {
        let Ok(message) = message else {
            break;
        };
        let header = message.header();
        let member = header.member().map(|member| member.as_str()).unwrap_or("");
        let body = message.body();
        match member {
            "Package" => {
                if let Ok((info, id, summary)) = body.deserialize::<(u32, String, String)>()
                    && !heard.packages.iter().any(|each| each.id == id)
                {
                    heard.packages.push(Update { id, info, summary });
                }
            }
            "ErrorCode" => heard.error = body.deserialize::<(u32, String)>().ok().or(heard.error),
            "RequireRestart" => {
                if let Ok((kind, _)) = body.deserialize::<(u32, String)>() {
                    heard.restart |= restarts(kind);
                }
            }
            "PropertiesChanged" => {
                if let Ok((_, changed, _)) = body.deserialize::<(String, HashMap<String, OwnedValue>, Vec<String>)>()
                    && let Some(raw) = changed.get("Percentage").and_then(|value| value.downcast_ref::<u32>().ok())
                {
                    percent(shown(raw));
                }
            }
            "Finished" => {
                heard.exit = body.deserialize::<(u32, u32)>().map_or(0, |(exit, _)| exit);
                break;
            }
            "Destroy" => break,
            _ => {}
        }
    }
    match failure(heard.exit, heard.error.as_ref()) {
        Some(error) => Err(Stop::Failed(error)),
        None => Ok(heard),
    }
}
