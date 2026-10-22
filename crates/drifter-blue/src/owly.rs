use std::process::{Command, Stdio};

use drifter_core::airdrop::{self, Report};

const NAME: &str = "io.github.t4t5.Owly";
const PATH: &str = "/io/github/t4t5/Owly";
const RADIO: &str = "io.github.t4t5.Owly.Radio1";
const CONFIG: &str = "/etc/owly/radio.toml";

pub fn report() -> Report {
    let owly = airdrop::owly_program().is_some();
    let probed = owly
        .then(|| {
            let bus = zbus::blocking::Connection::system().ok()?;
            let reply = bus.call_method(Some(NAME), PATH, Some(RADIO), "Probe", &()).ok()?;
            reply.body().deserialize::<String>().ok()
        })
        .flatten()
        .and_then(|json| Report::from_probe(&json, owly));
    let mut report = probed.unwrap_or_else(|| Report::from_cards(&airdrop::cards(std::path::Path::new("/sys/class/net")), owly));
    report.stale = report.owlyd && stale();
    report
}

fn stale() -> bool {
    let Some(built) = airdrop::owly_program().and_then(|owly| Some(owly.parent()?.join("owlyd"))) else {
        return false;
    };
    if std::path::Path::new("/usr/lib/owly/owlyd").exists() || !built.is_file() {
        return false;
    }
    match (std::fs::read(&built), std::fs::read(airdrop::INSTALLED)) {
        (Ok(built), Ok(installed)) => built != installed,
        (Ok(_), Err(_)) => true,
        _ => false,
    }
}

pub fn set_up() -> Result<(), String> {
    let script = airdrop::setup_script().ok_or_else(|| drifter_core::lang::t("Run scripts/install.sh in the Drifter folder to set up AirDrop.").to_string())?;
    let out = Command::new("pkexec").arg(&script).stdin(Stdio::null()).output().map_err(|error| error.to_string())?;
    match out.status.success() {
        true => Ok(()),
        false => Err(drifter_core::lang::t("AirDrop's radio service was not set up.").to_string()),
    }
}

pub fn allows_untested() -> bool {
    std::fs::read_to_string(CONFIG).is_ok_and(|text| {
        text.lines()
            .filter_map(|line| line.split('#').next())
            .any(|line| line.replace(' ', "") == "allow_untested=true")
    })
}

pub fn allow_untested() -> Result<(), String> {
    if allows_untested() {
        return Ok(());
    }
    let script = format!(
        "mkdir -p /etc/owly && touch {CONFIG} && grep -v '^ *allow_untested' {CONFIG} > {CONFIG}.drifter; echo 'allow_untested = true' >> {CONFIG}.drifter && mv {CONFIG}.drifter {CONFIG}"
    );
    let out = Command::new("pkexec")
        .args(["sh", "-c", &script])
        .stdin(Stdio::null())
        .output()
        .map_err(|error| error.to_string())?;
    match out.status.success() {
        true => Ok(()),
        false => Err(drifter_core::lang::t("AirDrop could not be allowed on this card.").to_string()),
    }
}

const DRIFTER: &str = "org.drifter.AirDrop";
const DRIFTER_PATH: &str = "/org/drifter/AirDrop";
const DRIFTER_FACE: &str = "org.drifter.AirDrop1";

pub fn drifter_state() -> Option<(bool, bool, bool)> {
    let bus = zbus::blocking::Connection::session().ok()?;
    let reply = bus.call_method(Some(DRIFTER), DRIFTER_PATH, Some(DRIFTER_FACE), "State", &()).ok()?;
    reply.body().deserialize().ok()
}

pub fn drifter_receive(on: bool) -> Result<(), String> {
    let bus = zbus::blocking::Connection::session().map_err(|error| error.to_string())?;
    bus.call_method(Some(DRIFTER), DRIFTER_PATH, Some(DRIFTER_FACE), "SetReceiving", &(on,))
        .map(drop)
        .map_err(|error| error.to_string())
}

pub fn drifter_refresh() {
    if let Ok(bus) = zbus::blocking::Connection::session() {
        let _ = bus.call_method(Some(DRIFTER), DRIFTER_PATH, Some(DRIFTER_FACE), "Refresh", &());
    }
}

pub fn drifter_send(files: &[std::path::PathBuf]) -> Result<(), String> {
    let bus = zbus::blocking::Connection::session().map_err(|error| error.to_string())?;
    let paths: Vec<String> = files.iter().map(|file| file.to_string_lossy().into_owned()).collect();
    bus.call_method(Some(DRIFTER), DRIFTER_PATH, Some(DRIFTER_FACE), "Send", &(paths,))
        .map(drop)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    #[ignore]
    fn this_computer_gets_a_verdict() {
        let report = super::report();
        println!("{report:?}");
        println!("{}", report.words());
    }
}
