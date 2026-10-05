#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Net {
    pub ssid: String,
    pub signal: u8,
    pub secure: bool,
    pub active: bool,
    pub known: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Wifi {
    pub managed: bool,
    pub on: bool,
    pub nets: Vec<Net>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gadget {
    pub mac: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Blue {
    pub on: bool,
    pub gadgets: Vec<Gadget>,
    pub searching: bool,
}

pub fn terse(line: &str) -> Vec<String> {
    let mut fields = vec![String::new()];
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if let Some(next) = chars.next()
                    && let Some(field) = fields.last_mut()
                {
                    field.push(next);
                }
            }
            ':' => fields.push(String::new()),
            c => {
                if let Some(field) = fields.last_mut() {
                    field.push(c);
                }
            }
        }
    }
    fields
}

pub fn wifi_nets(list: &str, known: &[String]) -> Vec<Net> {
    let mut nets: Vec<Net> = Vec::new();
    for line in list.lines() {
        let fields = terse(line);
        let [in_use, ssid, signal, security] = fields.as_slice() else {
            continue;
        };
        if ssid.is_empty() {
            continue;
        }
        let net = Net {
            ssid: ssid.clone(),
            signal: signal.trim().parse().unwrap_or(0),
            secure: !security.trim().is_empty() && security.trim() != "--",
            active: in_use.trim() == "*",
            known: known.iter().any(|each| each == ssid),
        };
        match nets.iter_mut().find(|kept| kept.ssid == net.ssid) {
            Some(kept) => {
                kept.active |= net.active;
                kept.signal = kept.signal.max(net.signal);
            }
            None => nets.push(net),
        }
    }
    nets.sort_by(|a, b| {
        b.active
            .cmp(&a.active)
            .then(b.known.cmp(&a.known))
            .then(b.signal.cmp(&a.signal))
            .then(a.ssid.cmp(&b.ssid))
    });
    nets
}

pub fn wifi_known(connections: &str) -> Vec<String> {
    connections
        .lines()
        .map(terse)
        .filter(|fields| fields.get(1).is_some_and(|kind| kind == "802-11-wireless"))
        .filter_map(|fields| fields.into_iter().next())
        .collect()
}

pub fn wifi_managed(devices: &str) -> Option<bool> {
    let states: Vec<String> = devices
        .lines()
        .map(terse)
        .filter(|fields| fields.first().is_some_and(|kind| kind == "wifi"))
        .map(|fields| fields.get(1).cloned().unwrap_or_default())
        .collect();
    match states.is_empty() {
        true => None,
        false => Some(states.iter().any(|state| state != "unmanaged")),
    }
}

pub fn devices(listed: &str) -> Vec<(String, String)> {
    listed
        .lines()
        .filter_map(|line| line.strip_prefix("Device "))
        .filter_map(|rest| {
            let (mac, name) = rest.split_once(' ').unwrap_or((rest, ""));
            let mac = mac.trim();
            (mac.len() == 17).then(|| {
                let name = name.trim();
                let name = match name.is_empty() || name.replace('-', ":") == mac {
                    true => mac.to_string(),
                    false => name.to_string(),
                };
                (mac.to_string(), name)
            })
        })
        .collect()
}

pub fn gadgets(all: &str, paired: &str, connected: &str) -> Vec<Gadget> {
    let paired: Vec<String> = devices(paired).into_iter().map(|(mac, _)| mac).collect();
    let connected: Vec<String> = devices(connected).into_iter().map(|(mac, _)| mac).collect();
    let mut out: Vec<Gadget> = devices(all)
        .into_iter()
        .map(|(mac, name)| Gadget {
            paired: paired.contains(&mac),
            connected: connected.contains(&mac),
            mac,
            name,
        })
        .collect();
    out.sort_by(|a, b| {
        b.paired
            .cmp(&a.paired)
            .then(b.connected.cmp(&a.connected))
            .then(a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    out
}

pub fn powered(show: &str) -> Option<bool> {
    show.lines()
        .find_map(|line| line.trim().strip_prefix("Powered:"))
        .map(|value| value.trim() == "yes")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ap {
    pub bssid: String,
    pub in_use: bool,
    pub channel: u32,
    pub mhz: u32,
    pub rate: String,
    pub width: String,
    pub signal: u8,
    pub security: String,
    pub flags: String,
}

pub const AP_FIELDS: &str = "IN-USE,SSID,BSSID,CHAN,FREQ,RATE,BANDWIDTH,SIGNAL,SECURITY,RSN-FLAGS,WPA-FLAGS";

pub fn access_points(list: &str, ssid: &str) -> Vec<Ap> {
    let number = |text: &str| {
        text.split_whitespace()
            .next()
            .and_then(|word| word.parse().ok())
            .unwrap_or(0)
    };
    let mut aps: Vec<Ap> = list
        .lines()
        .map(terse)
        .filter(|fields| fields.len() >= 11 && fields[1] == ssid)
        .map(|fields| {
            let flags = [&fields[9], &fields[10]]
                .into_iter()
                .filter(|flags| !flags.is_empty() && flags.as_str() != "(none)")
                .cloned()
                .collect::<Vec<_>>()
                .join(" ");
            Ap {
                in_use: fields[0].trim() == "*",
                bssid: fields[2].to_uppercase(),
                channel: number(&fields[3]),
                mhz: number(&fields[4]),
                rate: fields[5].clone(),
                width: fields[6].clone(),
                signal: number(&fields[7]).min(100) as u8,
                security: fields[8].clone(),
                flags,
            }
        })
        .collect();
    aps.sort_by(|a, b| b.in_use.cmp(&a.in_use).then(b.signal.cmp(&a.signal)));
    aps
}

pub fn scan_dbm(dump: &str) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    let mut bssid = None;
    for line in dump.lines() {
        if let Some(rest) = line.strip_prefix("BSS ") {
            bssid = rest.get(..17).map(str::to_uppercase);
        } else if let Some(value) = line.trim().strip_prefix("signal:")
            && let Some(bssid) = bssid.take()
            && let Some(dbm) = value.split_whitespace().next().and_then(|v| v.parse().ok())
        {
            out.push((bssid, dbm));
        }
    }
    out
}

pub fn link_dbm(wireless: &str, card: &str) -> Option<i32> {
    let line = wireless
        .lines()
        .find(|line| line.trim_start().starts_with(&format!("{card}:")))?;
    let level = line.split_whitespace().nth(3)?.trim_end_matches('.');
    level.parse().ok()
}

pub fn shown(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.to_string(), value.trim().to_string()))
        .filter(|(_, value)| !value.is_empty() && value != "--")
        .collect()
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Link {
    pub card: String,
    pub device: Vec<(String, String)>,
    pub dbm: Option<i32>,
    pub bitrate: Option<String>,
}

pub fn band(mhz: u32) -> &'static str {
    match mhz {
        0 => "unknown band",
        1..=3000 => "2.4 GHz",
        3001..=5924 => "5 GHz",
        _ => "6 GHz",
    }
}

fn keys(flags: &str) -> String {
    let words: Vec<&str> = flags.split_whitespace().collect();
    let mut out = Vec::new();
    if words.contains(&"psk") {
        out.push("password (PSK)");
    }
    if words.contains(&"sae") {
        out.push("password (SAE)");
    }
    if words.contains(&"802.1X") {
        out.push("enterprise (802.1X)");
    }
    if words.contains(&"owe") {
        out.push("enhanced open (OWE)");
    }
    if words.iter().any(|w| w.ends_with("ccmp")) {
        out.push("AES (CCMP)");
    }
    if words.iter().any(|w| w.ends_with("tkip")) {
        out.push("TKIP");
    }
    out.dedup();
    out.join(", ")
}

pub fn details(net: &Net, aps: &[Ap], dbm: &[(String, f64)], link: Option<&Link>) -> Vec<(String, String)> {
    let mut lines = vec![("Network".to_string(), String::new())];
    let mut add = |key: &str, value: String| lines.push((key.to_string(), value));
    add("Name", net.ssid.clone());
    add(
        "Status",
        match (net.active, net.known) {
            (true, _) => "Connected",
            (false, true) => "Saved, not connected",
            (false, false) => "Not saved",
        }
        .to_string(),
    );
    let security = aps
        .first()
        .map(|ap| ap.security.trim().to_string())
        .filter(|words| !words.is_empty() && words != "--")
        .unwrap_or_else(|| "None, open network".to_string());
    add("Security", security);
    if let Some(ap) = aps.first()
        && !ap.flags.is_empty()
    {
        add("Keys", keys(&ap.flags));
    }
    let strongest = aps.iter().map(|ap| ap.signal).max().unwrap_or(net.signal);
    add("Signal", format!("{strongest}%, {}", bars(strongest)));
    if let Some(link) = link {
        lines.push(("This connection".to_string(), String::new()));
        let mut add = |key: &str, value: String| lines.push((key.to_string(), value));
        if let Some(dbm) = link.dbm {
            add("Signal", format!("{dbm} dBm"));
        }
        if let Some(rate) = &link.bitrate {
            add("Speed now", rate.clone());
        }
        let named = [
            ("IP4.ADDRESS", "IP address"),
            ("IP4.GATEWAY", "Gateway"),
            ("IP4.DNS", "DNS"),
            ("IP6.ADDRESS", "IPv6 address"),
            ("IP6.GATEWAY", "IPv6 gateway"),
            ("IP6.DNS", "IPv6 DNS"),
            ("GENERAL.HWADDR", "Your MAC address"),
        ];
        for (key, label) in named {
            for (_, value) in link
                .device
                .iter()
                .filter(|(field, _)| field == key || field.starts_with(&format!("{key}[")))
            {
                add(label, value.clone());
            }
        }
        add("Card", link.card.clone());
    }
    if !aps.is_empty() {
        let heading = match aps.len() {
            1 => "Access point".to_string(),
            count => format!("Access points ({count})"),
        };
        lines.push((heading, String::new()));
        for ap in aps {
            let mut words = vec![
                band(ap.mhz).to_string(),
                format!("channel {}", ap.channel),
            ];
            if !ap.width.is_empty() {
                words.push(ap.width.clone());
            }
            if !ap.rate.is_empty() {
                words.push(format!("up to {}", ap.rate));
            }
            let found = dbm.iter().find(|(bssid, _)| *bssid == ap.bssid);
            words.push(match found {
                Some((_, dbm)) => format!("{}%, {dbm:.0} dBm", ap.signal),
                None => format!("{}%", ap.signal),
            });
            if ap.in_use {
                words.push("in use".to_string());
            }
            lines.push((ap.bssid.clone(), words.join(", ")));
        }
    }
    lines
}

pub fn bars(signal: u8) -> &'static str {
    match signal {
        0..=24 => "weak",
        25..=49 => "fair",
        50..=74 => "good",
        _ => "strong",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terse_fields_keep_escaped_colons() {
        assert_eq!(terse(r"*:Cafe\:Guest:80:WPA2"), ["*", "Cafe:Guest", "80", "WPA2"]);
        assert_eq!(terse(r" :a\\b::"), [" ", r"a\b", "", ""]);
    }

    #[test]
    fn networks_merge_by_name_and_the_joined_one_leads() {
        let list = "\
 :DrifterOpen:40:
 :DrifterNet:70:WPA2
*:Home:55:WPA2
 :Home:90:WPA2
 ::60:WPA2
 :Cafe\\:Guest:20:--
";
        let nets = wifi_nets(list, &["DrifterNet".to_string()]);
        let names: Vec<&str> = nets.iter().map(|net| net.ssid.as_str()).collect();
        assert_eq!(names, ["Home", "DrifterNet", "DrifterOpen", "Cafe:Guest"]);
        assert!(nets[0].active && nets[0].secure);
        assert_eq!(nets[0].signal, 90, "the strongest access point counts");
        assert!(nets[1].known && !nets[1].active);
        assert!(!nets[2].secure && !nets[3].secure, "blank and -- are open");
    }

    #[test]
    fn only_wifi_connections_count_as_known() {
        let shown = "Home:802-11-wireless\nWired connection 1:802-3-ethernet\nCafe\\:Guest:802-11-wireless\n";
        assert_eq!(wifi_known(shown), ["Home", "Cafe:Guest"]);
    }

    #[test]
    fn a_wifi_card_counts_only_when_networkmanager_runs_it() {
        assert_eq!(wifi_managed("loopback:connected (externally)\nethernet:connected\n"), None);
        assert_eq!(wifi_managed("wifi:unmanaged\nwifi-p2p:disconnected\n"), Some(false));
        assert_eq!(wifi_managed("wifi:unmanaged\nwifi:disconnected\n"), Some(true));
        assert_eq!(wifi_managed("wifi:connected\n"), Some(true));
    }

    const APS: &str = "\
 :Home:62\\:BA\\:BD\\:90\\:A7\\:95:10:2457 MHz:270 Mbit/s:20 MHz:70:WPA2:pair_ccmp group_ccmp psk:(none)
*:Home:62\\:ba\\:bd\\:90\\:a7\\:94:44:5220 MHz:540 Mbit/s:160 MHz:60:WPA2:pair_ccmp group_ccmp psk:(none)
 :Other:00\\:11\\:22\\:33\\:44\\:55:1:2412 MHz:54 Mbit/s:20 MHz:90:--:(none):(none)
";

    #[test]
    fn access_points_of_one_network_put_the_one_in_use_first() {
        let aps = access_points(APS, "Home");
        assert_eq!(aps.len(), 2, "only the asked network");
        assert!(aps[0].in_use);
        assert_eq!(aps[0].bssid, "62:BA:BD:90:A7:94");
        assert_eq!((aps[0].channel, aps[0].mhz, aps[0].signal), (44, 5220, 60));
        assert_eq!(aps[0].flags, "pair_ccmp group_ccmp psk");
        assert_eq!(band(aps[0].mhz), "5 GHz");
        assert_eq!(band(aps[1].mhz), "2.4 GHz");
    }

    #[test]
    fn dbm_comes_from_iw_and_proc() {
        let dump = "BSS 62:ba:bd:90:a7:94(on wlp1s0) -- associated\n\tfreq: 5220\n\tsignal: -47.00 dBm\nBSS 62:ba:bd:90:a7:95(on wlp1s0)\n\tsignal: -61.00 dBm\n";
        assert_eq!(
            scan_dbm(dump),
            [("62:BA:BD:90:A7:94".to_string(), -47.0), ("62:BA:BD:90:A7:95".to_string(), -61.0)]
        );
        let wireless = "Inter-| sta-|   Quality        |   Discarded packets\n face | tus | link level noise |  nwid  crypt\nwlp1s0: 0000   70.  -40.  -256        0      0\n";
        assert_eq!(link_dbm(wireless, "wlp1s0"), Some(-40));
        assert_eq!(link_dbm(wireless, "wlan0"), None);
    }

    #[test]
    fn details_show_the_ip_setup_only_for_the_joined_network() {
        let net = Net {
            ssid: "Home".into(),
            signal: 70,
            secure: true,
            active: true,
            known: true,
        };
        let aps = access_points(APS, "Home");
        let dbm = scan_dbm("BSS 62:ba:bd:90:a7:94(on wlp1s0)\n\tsignal: -47.00 dBm\n");
        let link = Link {
            card: "wlp1s0".into(),
            device: shown("GENERAL.HWADDR:38:DE:AD:4D:2A:4C\nIP4.ADDRESS[1]:192.168.1.219/24\nIP4.GATEWAY:192.168.1.1\nIP4.DNS[1]:192.168.1.1\nIP6.GATEWAY:\n"),
            dbm: Some(-40),
            bitrate: None,
        };
        let lines = details(&net, &aps, &dbm, Some(&link));
        let get = |key: &str| lines.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
        assert_eq!(get("IP address"), Some("192.168.1.219/24"));
        assert_eq!(get("Your MAC address"), Some("38:DE:AD:4D:2A:4C"));
        assert_eq!(get("IPv6 gateway"), None, "empty fields stay out");
        assert_eq!(get("Keys"), Some("password (PSK), AES (CCMP)"));
        assert_eq!(get("62:BA:BD:90:A7:94"), Some("5 GHz, channel 44, 160 MHz, up to 540 Mbit/s, 60%, -47 dBm, in use"));
        assert_eq!(get("62:BA:BD:90:A7:95"), Some("2.4 GHz, channel 10, 20 MHz, up to 270 Mbit/s, 70%"));
        assert!(lines.contains(&("Access points (2)".to_string(), String::new())));

        let away = Net { active: false, ..net };
        let lines = details(&away, &aps, &dbm, None);
        assert!(lines.iter().all(|(k, _)| k != "IP address" && k != "This connection"));
        assert!(lines.contains(&("Status".to_string(), "Saved, not connected".to_string())));
    }

    #[test]
    fn bluetooth_devices_sort_paired_first() {
        let all = "Device 00:AA:01:00:00:00 trixie\nDevice 11:22:33:44:55:66 Headphones\nDevice 77:88:99:AA:BB:CC 77-88-99-AA-BB-CC\nnot a device\n";
        let paired = "Device 11:22:33:44:55:66 Headphones\n";
        let connected = "Device 11:22:33:44:55:66 Headphones\n";
        let found = gadgets(all, paired, connected);
        assert_eq!(found[0].name, "Headphones");
        assert!(found[0].paired && found[0].connected);
        assert_eq!(found[1].name, "77:88:99:AA:BB:CC", "a nameless device shows its address");
        assert_eq!(found[2].name, "trixie");
        assert!(!found[2].paired);
    }

    #[test]
    fn the_adapter_power_reads_from_show() {
        assert_eq!(powered("Controller 00:AA\n\tName: x\n\tPowered: yes\n"), Some(true));
        assert_eq!(powered("\tPowered: no\n"), Some(false));
        assert_eq!(powered("No default controller available\n"), None);
    }
}
