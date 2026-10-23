use std::sync::OnceLock;
use std::time::Duration;

use drifter_core::net::{self, Private, Wifi};
use drifter_net::Bus;
use drifter_net::bus::Joined;

pub enum Job {
    WifiPower(bool),
    Join { ssid: String, password: Option<String> },
    JoinHidden { ssid: String, password: Option<String> },
    JoinEnterprise { ssid: String, user: String, password: String, domain: Option<String> },
    Leave(String),
    Forget(String),
    Autojoin(String, bool),
    Private(String, Private),
    Metered(String, bool),
    Raise(String, bool),
    StartService,
}

const WAIT: Duration = Duration::from_secs(150);

fn bus() -> Option<&'static Bus> {
    static BUS: OnceLock<Option<Bus>> = OnceLock::new();
    BUS.get_or_init(|| Bus::system().ok()).as_ref()
}

pub fn read_wifi() -> Option<Wifi> {
    let wifi = bus()?.snapshot()?;
    match wifi.card.is_some() {
        true => Some(wifi),
        false => None,
    }
}

pub fn running() -> bool {
    bus().is_some_and(Bus::running)
}

pub fn details(net: &net::Net) -> Vec<(String, String)> {
    let Some(bus) = bus() else {
        return Vec::new();
    };
    let aps = bus.points(&net.ssid);
    let link = net.active.then(|| bus.link()).flatten();
    let card = bus.snapshot().and_then(|wifi| wifi.card).map(|card| card.iface).unwrap_or_default();
    let dbm = match card.is_empty() {
        true => Vec::new(),
        false => bus.dbm(&card),
    };
    net::details(net, &aps, &dbm, link.as_ref())
}

pub fn rescan() {
    if let Some(bus) = bus() {
        let _ = bus.scan();
    }
}

fn settled(bus: &Bus, active: Result<String, String>, ssid: &str) -> Result<(), String> {
    let active = active?;
    match bus.settle(&active, WAIT) {
        Joined::Done => Ok(()),
        outcome => Err(bus.outcome_words(outcome, ssid)),
    }
}

fn uuid_of(bus: &Bus, ssid: &str) -> Result<String, String> {
    bus.snapshot()
        .and_then(|wifi| wifi.saved_for(ssid).first().map(|saved| saved.uuid.clone()))
        .ok_or_else(|| drifter_core::lang::t("That network is not saved.").to_string())
}

pub fn run(job: Job) -> Result<(), String> {
    if let Job::StartService = job {
        return Bus::start_service();
    }
    let bus = bus().ok_or_else(|| net::Missing::Service.words().to_string())?;
    match job {
        Job::StartService => Ok(()),
        Job::WifiPower(on) => bus.power(on),
        Job::Join { ssid, password: None, .. } => settled(bus, bus.join(&ssid), &ssid),
        Job::Join { ssid, password: Some(password), .. } => settled(bus, bus.join_with(&ssid, Some(&password), false), &ssid),
        Job::JoinHidden { ssid, password } => settled(bus, bus.join_with(&ssid, password.as_deref(), true), &ssid),
        Job::JoinEnterprise { ssid, user, password, domain } => bus.join_enterprise(&ssid, &user, &password, domain.as_deref()).map(drop),
        Job::Leave(ssid) => bus.leave(&ssid),
        Job::Forget(ssid) => bus.forget(&ssid),
        Job::Autojoin(ssid, on) => bus.set_autojoin(&uuid_of(bus, &ssid)?, on),
        Job::Private(ssid, private) => bus.set_private(&uuid_of(bus, &ssid)?, private),
        Job::Metered(ssid, on) => bus.set_metered(&uuid_of(bus, &ssid)?, on),
        Job::Raise(ssid, up) => {
            let wifi = bus.snapshot().ok_or_else(|| net::Missing::Service.words().to_string())?;
            let mut known: Vec<&net::Saved> = wifi.saved.iter().filter(|each| each.kind == net::Kind::Wifi).collect();
            known.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.id.to_lowercase().cmp(&b.id.to_lowercase())));
            let at = known.iter().position(|each| each.ssid == ssid).ok_or_else(|| drifter_core::lang::t("That network is not saved.").to_string())?;
            let to = match up {
                true => at.checked_sub(1),
                false => (at + 1 < known.len()).then_some(at + 1),
            };
            let Some(to) = to else {
                return Ok(());
            };
            let mut order: Vec<&net::Saved> = known.clone();
            order.swap(at, to);
            let top = order.len() as i32;
            for (place, each) in order.iter().enumerate() {
                let want = (top - place as i32) * 10;
                if each.priority != want {
                    bus.set_priority(&each.uuid, want)?;
                }
            }
            Ok(())
        }
    }
}
