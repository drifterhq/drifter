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
