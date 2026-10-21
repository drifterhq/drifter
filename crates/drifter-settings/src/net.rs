use std::io::Write;
use std::process::{Command, Stdio};

use drifter_core::net::{self, Wifi};

pub enum Job {
    WifiPower(bool),
    Join { ssid: String, password: Option<String>, known: bool },
    JoinHidden { ssid: String, password: Option<String> },
    JoinEnterprise { ssid: String, user: String, password: String, domain: Option<String> },
    Leave(String),
    Forget(String),
}

fn read(program: &str, args: &[&str]) -> Option<String> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
}

pub fn read_wifi() -> Option<Wifi> {
    let devices = read("nmcli", &["-t", "-f", "TYPE,STATE", "device"])?;
    if !net::wifi_managed(&devices)? {
        return Some(Wifi::default());
    }
    let on = read("nmcli", &["-t", "radio", "wifi"])?.trim() == "enabled";
    let known = read("nmcli", &["-t", "-f", "NAME,TYPE", "connection", "show"])
        .map(|text| net::wifi_known(&text))
        .unwrap_or_default();
    let nets = match on {
        true => read(
            "nmcli",
            &["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", "no"],
        )
        .map(|text| net::wifi_nets(&text, &known))
        .unwrap_or_default(),
        false => Vec::new(),
    };
    Some(Wifi {
        managed: true,
        on,
        nets,
    })
}

fn iw() -> &'static str {
    ["/usr/sbin/iw", "/sbin/iw"]
        .into_iter()
        .find(|path| std::path::Path::new(path).is_file())
        .unwrap_or("iw")
}

pub fn details(net: &net::Net) -> Vec<(String, String)> {
    let list = read(
        "nmcli",
        &["-t", "-f", net::AP_FIELDS, "device", "wifi", "list", "--rescan", "no"],
    )
    .unwrap_or_default();
    let aps = net::access_points(&list, &net.ssid);
    let card = read("nmcli", &["-t", "-f", "DEVICE,TYPE", "device"])
        .and_then(|text| {
            text.lines()
                .map(net::terse)
                .find(|fields| fields.get(1).is_some_and(|kind| kind == "wifi"))
                .and_then(|fields| fields.into_iter().next())
        })
        .unwrap_or_default();
    let dbm = match card.is_empty() {
        true => Vec::new(),
        false => read(iw(), &["dev", &card, "scan", "dump"])
            .map(|dump| net::scan_dbm(&dump))
            .unwrap_or_default(),
    };
    let link = (net.active && !card.is_empty()).then(|| net::Link {
        device: read(
            "nmcli",
            &[
                "-t",
                "-f",
                "GENERAL.HWADDR,IP4.ADDRESS,IP4.GATEWAY,IP4.DNS,IP6.ADDRESS,IP6.GATEWAY,IP6.DNS",
                "device",
                "show",
                &card,
            ],
        )
        .map(|text| net::shown(&text))
        .unwrap_or_default(),
        dbm: std::fs::read_to_string("/proc/net/wireless")
            .ok()
            .and_then(|text| net::link_dbm(&text, &card)),
        bitrate: read(iw(), &["dev", &card, "link"]).and_then(|text| {
            text.lines()
                .find_map(|line| line.trim().strip_prefix("tx bitrate:"))
                .map(|rate| rate.split_whitespace().take(2).collect::<Vec<_>>().join(" "))
                .filter(|rate| !rate.is_empty())
        }),
        card: card.clone(),
    });
    net::details(net, &aps, &dbm, link.as_ref())
}

pub fn rescan() {
    let _ = Command::new("nmcli")
        .args(["device", "wifi", "rescan"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn said(out: &std::process::Output) -> String {
    let text = String::from_utf8_lossy(&out.stderr).to_string()
        + &String::from_utf8_lossy(&out.stdout);
    let line = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .find(|line| line.starts_with("Error") || line.starts_with("Failed"))
        .or_else(|| text.lines().map(str::trim).rfind(|line| !line.is_empty()))
        .unwrap_or("it did not work")
        .to_string();
    line.trim_start_matches("Error: ").to_string()
}

fn run_one(program: &str, args: &[&str], typed: Option<&str>) -> Result<String, String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(match typed {
            Some(_) => Stdio::piped(),
            None => Stdio::null(),
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("{program} did not run: {error}"))?;
    if let (Some(typed), Some(mut stdin)) = (typed, child.stdin.take()) {
        let _ = writeln!(stdin, "{typed}");
    }
    let out = child
        .wait_with_output()
        .map_err(|error| format!("{program} did not finish: {error}"))?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let failed = text.lines().any(|line| line.trim_start().starts_with("Failed"));
    match out.status.success() && !failed {
        true => Ok(text),
        false => Err(said(&out)),
    }
}

pub fn run(job: Job) -> Result<(), String> {
    let switch = |on: bool| if on { "on" } else { "off" };
    match job {
        Job::WifiPower(on) => run_one("nmcli", &["radio", "wifi", switch(on)], None).map(drop),
        Job::Join {
            ssid,
            known: true,
            password: None,
        } => run_one("nmcli", &["connection", "up", "id", &ssid], None).map(drop),
        Job::Join { ssid, password, .. } => match password {
            Some(password) => run_one(
                "nmcli",
                &["--ask", "device", "wifi", "connect", &ssid],
                Some(&password),
            )
            .map(drop),
            None => run_one("nmcli", &["device", "wifi", "connect", &ssid], None).map(drop),
        },
        Job::JoinHidden { ssid, password } => {
            let _ = run_one("nmcli", &["device", "wifi", "rescan", "ssid", &ssid], None);
            std::thread::sleep(std::time::Duration::from_secs(4));
            join_hidden(&ssid, password)
        }
        Job::JoinEnterprise {
            ssid,
            user,
            password,
            domain,
        } => {
            let _ = run_one("nmcli", &["connection", "delete", "id", &ssid], None);
            let mut args = vec![
                "connection", "add", "type", "wifi", "con-name", &ssid, "ssid", &ssid,
                "connection.autoconnect", "no", "wifi-sec.key-mgmt", "wpa-eap", "802-1x.eap", "peap",
                "802-1x.phase2-auth", "mschapv2", "802-1x.identity", &user, "802-1x.password-flags", "0",
            ];
            if let Some(domain) = domain.as_deref() {
                args.extend(["802-1x.system-ca-certs", "yes", "802-1x.domain-suffix-match", domain]);
            }
            run_one("nmcli", &args, None)?;
            let typed = format!("set 802-1x.password {password}\nsave\nquit");
            let joined = run_one("nmcli", &["connection", "edit", "id", &ssid], Some(&typed))
                .and_then(|_| run_one("nmcli", &["connection", "up", "id", &ssid], None));
            match joined {
                Ok(_) => run_one("nmcli", &["connection", "modify", "id", &ssid, "connection.autoconnect", "yes"], None).map(drop),
                Err(error) => {
                    let _ = run_one("nmcli", &["connection", "delete", "id", &ssid], None);
                    Err(error)
                }
            }
        }
        Job::Leave(ssid) => run_one("nmcli", &["connection", "down", "id", &ssid], None).map(drop),
        Job::Forget(ssid) => {
            run_one("nmcli", &["connection", "delete", "id", &ssid], None).map(drop)
        }
    }
}

fn join_hidden(ssid: &str, password: Option<String>) -> Result<(), String> {
    match password {
        Some(password) => run_one(
            "nmcli",
            &["--ask", "device", "wifi", "connect", ssid, "hidden", "yes"],
            Some(&password),
        )
        .map(drop),
        None => run_one("nmcli", &["device", "wifi", "connect", ssid, "hidden", "yes"], None).map(drop),
    }
}
