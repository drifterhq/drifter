#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Net {
    pub ssid: String,
    pub signal: u8,
    pub secure: bool,
    pub enterprise: bool,
    pub active: bool,
    pub known: bool,
    pub sae: bool,
    pub wep: bool,
    pub mhz: u32,
    pub point: String,
    pub busy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Online {
    #[default]
    Unknown,
    None,
    Portal,
    Limited,
    Full,
}

impl Online {
    pub fn read(value: u32) -> Online {
        match value {
            1 => Online::None,
            2 => Online::Portal,
            3 => Online::Limited,
            4 => Online::Full,
            _ => Online::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Primary {
    #[default]
    None,
    Wifi,
    Wired,
    Vpn,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Wifi,
    Wired,
    WireGuard,
    Vpn,
    Other,
}

impl Kind {
    pub fn read(word: &str) -> Kind {
        match word {
            "802-11-wireless" => Kind::Wifi,
            "802-3-ethernet" => Kind::Wired,
            "wireguard" => Kind::WireGuard,
            "vpn" => Kind::Vpn,
            _ => Kind::Other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Private {
    Off,
    #[default]
    Fixed,
    Changes,
}

impl Private {
    pub const ALL: [Private; 3] = [Private::Off, Private::Fixed, Private::Changes];

    pub fn read(word: &str) -> Private {
        match word {
            "stable" => Private::Fixed,
            "random" => Private::Changes,
            _ => Private::Off,
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Private::Off => "permanent",
            Private::Fixed => "stable",
            Private::Changes => "random",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Private::Off => crate::lang::t("Off"),
            Private::Fixed => crate::lang::t("Fixed"),
            Private::Changes => crate::lang::t("Changes"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Saved {
    pub path: String,
    pub uuid: String,
    pub id: String,
    pub kind: Kind,
    pub ssid: String,
    pub autoconnect: bool,
    pub priority: i32,
    pub metered: bool,
    pub private: Private,
    pub enterprise: bool,
    pub hidden: bool,
    pub powersave: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Card {
    pub path: String,
    pub iface: String,
    pub state: u32,
    pub reason: u32,
    pub hw: String,
    pub scanned: i64,
    pub ip4: String,
    pub ip6: String,
}

impl Card {
    pub fn busy(&self) -> bool {
        (40..=90).contains(&self.state)
    }

    pub fn joined(&self) -> bool {
        self.state == 100
    }

}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Wired {
    pub path: String,
    pub iface: String,
    pub state: u32,
    pub carrier: bool,
}

impl Wired {
    pub fn connected(&self) -> bool {
        self.state == 100
    }

    pub fn busy(&self) -> bool {
        (40..=90).contains(&self.state)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Vpn {
    pub uuid: String,
    pub name: String,
    pub wireguard: bool,
    pub active: bool,
    pub busy: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Wifi {
    pub managed: bool,
    pub on: bool,
    pub nets: Vec<Net>,
    pub hardware: bool,
    pub card: Option<Card>,
    pub online: Online,
    pub primary: Primary,
    pub wired: Option<Wired>,
    pub vpns: Vec<Vpn>,
    pub saved: Vec<Saved>,
    pub joined: Option<String>,
    pub joining: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    Service,
    Card,
}

impl Missing {
    pub fn words(self) -> &'static str {
        match self {
            Missing::Service => crate::lang::t("NetworkManager is not running."),
            Missing::Card => crate::lang::t("No Wi-Fi card found."),
        }
    }
}

pub type Seen = Result<Wifi, Missing>;

impl Wifi {
    pub fn find(&self, ssid: &str) -> Option<&Net> {
        self.nets.iter().find(|net| net.ssid == ssid)
    }

    pub fn saved_for(&self, ssid: &str) -> Vec<&Saved> {
        let mut saved: Vec<&Saved> = self.saved.iter().filter(|each| each.kind == Kind::Wifi && each.ssid == ssid).collect();
        saved.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.id.cmp(&b.id)));
        saved
    }

    pub fn known(&self) -> impl Iterator<Item = &Net> {
        self.nets.iter().filter(|net| net.known || net.active)
    }

    pub fn others(&self) -> impl Iterator<Item = &Net> {
        self.nets.iter().filter(|net| !net.known && !net.active)
    }

    pub fn joined_net(&self) -> Option<&Net> {
        self.nets.iter().find(|net| net.active)
    }

    pub fn vpn_on(&self) -> bool {
        self.vpns.iter().any(|vpn| vpn.active)
    }

    pub fn sort(&mut self) {
        self.nets.sort_by(|a, b| {
            b.active
                .cmp(&a.active)
                .then(b.known.cmp(&a.known))
                .then(b.signal.cmp(&a.signal))
                .then(a.ssid.to_lowercase().cmp(&b.ssid.to_lowercase()))
        });
    }
}

pub const PRIVACY: u32 = 0x1;
pub const PAIR_TKIP: u32 = 0x4;
pub const PAIR_CCMP: u32 = 0x8;
pub const GROUP_TKIP: u32 = 0x40;
pub const GROUP_CCMP: u32 = 0x80;
pub const KEY_PSK: u32 = 0x100;
pub const KEY_EAP: u32 = 0x200;
pub const KEY_SAE: u32 = 0x400;
pub const KEY_OWE: u32 = 0x800;
pub const KEY_OWE_TM: u32 = 0x1000;
pub const KEY_SUITE_B: u32 = 0x2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Guard {
    pub flags: u32,
    pub wpa: u32,
    pub rsn: u32,
}

impl Guard {
    pub fn keys(self) -> u32 {
        self.wpa | self.rsn
    }

    pub fn enterprise(self) -> bool {
        self.keys() & (KEY_EAP | KEY_SUITE_B) != 0
    }

    pub fn sae(self) -> bool {
        self.rsn & KEY_SAE != 0 && self.keys() & KEY_PSK == 0
    }

    pub fn wep(self) -> bool {
        self.flags & PRIVACY != 0 && self.wpa == 0 && self.rsn == 0
    }

    pub fn owe(self) -> bool {
        self.rsn & (KEY_OWE | KEY_OWE_TM) != 0 && self.keys() & (KEY_PSK | KEY_SAE | KEY_EAP) == 0
    }

    pub fn secure(self) -> bool {
        self.wep() || self.keys() & (KEY_PSK | KEY_SAE | KEY_EAP | KEY_SUITE_B) != 0
    }

    pub fn words(self) -> String {
        if self.wep() {
            return "WEP".to_string();
        }
        if self.owe() {
            return "OWE".to_string();
        }
        let mut out: Vec<&str> = Vec::new();
        if self.wpa != 0 {
            out.push("WPA1");
        }
        if self.rsn & (KEY_PSK | KEY_EAP) != 0 {
            out.push("WPA2");
        }
        if self.rsn & (KEY_SAE | KEY_SUITE_B) != 0 {
            out.push("WPA3");
        }
        if self.enterprise() {
            out.push("802.1X");
        }
        out.join(" ")
    }

    pub fn flag_words(self) -> String {
        let keys = self.keys();
        let mut out: Vec<&str> = Vec::new();
        if keys & PAIR_TKIP != 0 {
            out.push("pair_tkip");
        }
        if keys & PAIR_CCMP != 0 {
            out.push("pair_ccmp");
        }
        if keys & GROUP_TKIP != 0 {
            out.push("group_tkip");
        }
        if keys & GROUP_CCMP != 0 {
            out.push("group_ccmp");
        }
        if keys & KEY_PSK != 0 {
            out.push("psk");
        }
        if keys & KEY_SAE != 0 {
            out.push("sae");
        }
        if keys & (KEY_EAP | KEY_SUITE_B) != 0 {
            out.push("802.1X");
        }
        if keys & (KEY_OWE | KEY_OWE_TM) != 0 {
            out.push("owe");
        }
        out.join(" ")
    }
}

pub fn channel(mhz: u32) -> u32 {
    match mhz {
        2484 => 14,
        2412..=2472 => (mhz - 2407) / 5,
        5955..=7115 => (mhz - 5950) / 5,
        5000..=5900 => (mhz - 5000) / 5,
        _ => 0,
    }
}

pub fn ssid_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim_end_matches('\0').to_string()
}

pub fn arcs(signal: u8) -> u8 {
    match signal {
        0 => 0,
        1..=30 => 1,
        31..=60 => 2,
        _ => 3,
    }
}

pub fn steps(signal: u8) -> u8 {
    match signal {
        0..=20 => 1,
        21..=45 => 2,
        46..=70 => 3,
        _ => 4,
    }
}

pub const WEAK: u8 = 30;

pub const PORTAL_PAGE: &str = "http://neverssl.com/";

pub fn plain_error(reason: u32, name: &str) -> String {
    match reason {
        7 | 8 | 9 | 10 | 11 => crate::lang::t("Wrong password").to_string(),
        5 | 6 | 15 | 16 | 17 => crate::lang::t("No address from the network").to_string(),
        53 | 70 => crate::lang::t("Out of range").to_string(),
        36 | 26 => crate::lang::t("The Wi-Fi card went away").to_string(),
        2 | 3 | 4 | 12 | 13 => crate::lang::t("Did not join").to_string(),
        _ => crate::lang::tf("Could not join {}", &[&name]),
    }
}

pub fn plain_failure(reason: u32, name: &str) -> String {
    match reason {
        7 | 8 | 9 | 10 | 11 => crate::lang::tf("The password for {} was not right.", &[&name]),
        5 | 6 | 15 | 16 | 17 => crate::lang::tf("{} did not give this computer an address. Try again, or restart the router.", &[&name]),
        53 | 70 => crate::lang::tf("{} is out of range. Move closer, then try again.", &[&name]),
        36 | 26 => crate::lang::t("The Wi-Fi card went away. Check that it is still plugged in.").to_string(),
        _ => crate::lang::tf("Could not join {}. Try Fix It in Wi-Fi settings.", &[&name]),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Off,
    Idle,
    Busy,
    Joined,
    Wired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Badge {
    #[default]
    None,
    Alert,
    Portal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Face {
    pub shape: Shape,
    pub arcs: u8,
    pub badge: Badge,
    pub lock: bool,
}

pub fn face(wifi: &Wifi, airdrop: bool) -> Face {
    let lock = wifi.vpn_on();
    let badge = match wifi.online {
        Online::Portal => Badge::Portal,
        Online::None | Online::Limited => Badge::Alert,
        _ => Badge::None,
    };
    let wired_lead = wifi.primary == Primary::Wired || (wifi.wired.as_ref().is_some_and(Wired::connected) && wifi.joined.is_none());
    if wired_lead {
        return Face { shape: Shape::Wired, arcs: 0, badge, lock };
    }
    if airdrop || !wifi.on || !wifi.managed {
        return Face { shape: Shape::Off, arcs: 0, badge: Badge::None, lock };
    }
    if wifi.card.as_ref().is_some_and(Card::busy) {
        return Face { shape: Shape::Busy, arcs: 0, badge: Badge::None, lock };
    }
    match wifi.joined_net() {
        Some(net) => Face {
            shape: Shape::Joined,
            arcs: arcs(net.signal),
            badge,
            lock,
        },
        None if wifi.card.as_ref().is_some_and(Card::joined) => Face {
            shape: Shape::Joined,
            arcs: 3,
            badge,
            lock,
        },
        None => Face { shape: Shape::Idle, arcs: 0, badge: Badge::None, lock },
    }
}

pub fn tip(wifi: &Wifi, airdrop: bool) -> String {
    if airdrop {
        return crate::lang::t("AirDrop is using Wi-Fi.").to_string();
    }
    if wifi.primary == Primary::Wired {
        return crate::lang::t("Ethernet, Connected").to_string();
    }
    if !wifi.on {
        return crate::lang::t("Wi-Fi is off.").to_string();
    }
    if let Some(name) = wifi.joining.as_deref() {
        return crate::lang::tf("Joining {}…", &[&name]);
    }
    match (wifi.joined.as_deref(), wifi.online) {
        (Some(name), Online::Portal) => crate::lang::tf("{}: sign in to use the internet", &[&name]),
        (Some(name), Online::None | Online::Limited) => crate::lang::tf("{}: no internet", &[&name]),
        (Some(name), _) => name.to_string(),
        (None, _) => crate::lang::t("Not connected").to_string(),
    }
}

pub const MENU_POWER: i32 = -1;
pub const MENU_SETTINGS: i32 = -2;
pub const MENU_OTHER: i32 = -4;
pub const MENU_STOP_AIRDROP: i32 = -5;
pub const MENU_WIRED: i32 = -6;
pub const MENU_VPN: i32 = 1 << 20;
pub const MENU_MOST_OTHERS: usize = 6;

pub fn menu(wifi: &Wifi, airdrop: bool) -> Vec<crate::menu::Item> {
    use crate::menu::Item;
    let note = |text: &str| Item {
        enabled: false,
        ..Item::line(-8, text)
    };
    let mut items = Vec::new();
    if airdrop {
        items.push(note(crate::lang::t("AirDrop is using Wi-Fi.")));
        items.push(Item::line(MENU_STOP_AIRDROP, crate::lang::t("Stop AirDrop")));
        return items;
    }
    if !wifi.managed {
        items.push(note(crate::lang::t("Another tool runs this Wi-Fi card.")));
        return items;
    }
    if !wifi.on {
        items.push(note(crate::lang::t("Wi-Fi is off.")));
        items.push(Item::line(MENU_POWER, crate::lang::t("Turn Wi-Fi On")));
        return items;
    }
    let line = |at: usize, net: &Net| {
        let state = match (net.active, net.busy, net.signal < WEAK) {
            (true, _, _) => crate::lang::tf("{}, joined", &[&net.ssid]),
            (false, true, _) => crate::lang::tf("{}, joining…", &[&net.ssid]),
            (false, false, true) => crate::lang::tf("{}, weak", &[&net.ssid]),
            (false, false, false) => net.ssid.clone(),
        };
        Item::line(at as i32, &state)
    };
    let known: Vec<Item> = wifi.nets.iter().enumerate().filter(|(_, net)| net.known || net.active).map(|(at, net)| line(at, net)).collect();
    let others: Vec<Item> = wifi
        .nets
        .iter()
        .enumerate()
        .filter(|(_, net)| !net.known && !net.active)
        .take(MENU_MOST_OTHERS)
        .map(|(at, net)| line(at, net))
        .collect();
    let had = !known.is_empty();
    items.extend(known);
    if !others.is_empty() {
        if had {
            items.push(Item::rule(-10));
        }
        items.extend(others);
    }
    if items.is_empty() {
        items.push(note(crate::lang::t("No networks in range")));
    }
    items.push(Item::line(MENU_OTHER, crate::lang::t("Other…")));
    if !wifi.vpns.is_empty() {
        items.push(Item::rule(-12));
        for (at, vpn) in wifi.vpns.iter().enumerate() {
            let words = match (vpn.active, vpn.busy) {
                (_, true) => crate::lang::tf("{}, connecting…", &[&vpn.name]),
                (true, false) => crate::lang::tf("{}, on", &[&vpn.name]),
                (false, false) => vpn.name.clone(),
            };
            items.push(Item::line(MENU_VPN + at as i32, &words));
        }
    }
    items
}

pub fn status_lines(wifi: &Wifi, airdrop: bool, ap: Option<&Ap>, link: Option<&Link>) -> Vec<String> {
    if airdrop {
        return vec![crate::lang::t("AirDrop is using Wi-Fi.").to_string()];
    }
    let mut lines = Vec::new();
    if let Some(wired) = wifi.wired.as_ref().filter(|wired| wired.connected()) {
        lines.push(crate::lang::tf("Ethernet ({}), connected", &[&wired.iface]));
    }
    if !wifi.on {
        lines.push(crate::lang::t("Wi-Fi is off.").to_string());
        return lines;
    }
    let Some(net) = wifi.joined_net() else {
        lines.push(crate::lang::t("Wi-Fi: not connected").to_string());
        return lines;
    };
    lines.push(match wifi.online {
        Online::Portal => crate::lang::tf("{}: sign in to use the internet", &[&net.ssid]),
        Online::None | Online::Limited => crate::lang::tf("{}: no internet", &[&net.ssid]),
        _ => crate::lang::tf("{}, joined", &[&net.ssid]),
    });
    let field = |key: &str| {
        link.and_then(|link| link.device.iter().find(|(field, _)| field == key || field.starts_with(&format!("{key}["))).map(|(_, value)| value.clone()))
    };
    if let Some(address) = field("IP4.ADDRESS") {
        lines.push(crate::lang::tf("IP address: {}", &[&address]));
    }
    if let Some(router) = field("IP4.GATEWAY") {
        lines.push(crate::lang::tf("Router: {}", &[&router]));
    }
    if let Some(ap) = ap {
        lines.push(crate::lang::tf("{}, channel {}", &[&band(ap.mhz), &ap.channel]));
        lines.push(match ap.security.is_empty() {
            true => crate::lang::t("Security: none").to_string(),
            false => crate::lang::tf("Security: {}", &[&ap.security]),
        });
    }
    lines.push(match link.and_then(|link| link.dbm) {
        Some(dbm) => crate::lang::tf("Signal: {}%, {} dBm", &[&net.signal, &dbm]),
        None => crate::lang::tf("Signal: {}%", &[&net.signal]),
    });
    if let Some(rate) = link.and_then(|link| link.bitrate.clone()) {
        lines.push(crate::lang::tf("Speed now: {}", &[&rate]));
    }
    lines
}

pub fn status_menu(wifi: &Wifi, lines: &[String]) -> Vec<crate::menu::Item> {
    use crate::menu::Item;
    let mut items: Vec<Item> = lines
        .iter()
        .map(|text| Item {
            enabled: false,
            ..Item::line(-20, text)
        })
        .collect();
    items.push(Item::rule(-21));
    if wifi.managed {
        items.push(Item::line(
            MENU_POWER,
            match wifi.on {
                true => crate::lang::t("Turn Wi-Fi Off"),
                false => crate::lang::t("Turn Wi-Fi On"),
            },
        ));
    }
    items.push(Item::line(MENU_SETTINGS, crate::lang::t("Wi-Fi Settings…")));
    items
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Peer {
    pub public_key: String,
    pub preshared_key: String,
    pub allowed: Vec<String>,
    pub endpoint: String,
    pub keepalive: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WireGuard {
    pub name: String,
    pub private_key: String,
    pub addresses: Vec<String>,
    pub dns: Vec<String>,
    pub port: u32,
    pub mtu: u32,
    pub peers: Vec<Peer>,
}

pub fn wireguard(name: &str, text: &str) -> Result<WireGuard, String> {
    let mut out = WireGuard {
        name: name.to_string(),
        ..WireGuard::default()
    };
    let mut part = "";
    let list = |value: &str| value.split(',').map(|each| each.trim().to_string()).filter(|each| !each.is_empty()).collect::<Vec<_>>();
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') {
            part = match line.to_ascii_lowercase().as_str() {
                "[interface]" => "interface",
                "[peer]" => {
                    out.peers.push(Peer::default());
                    "peer"
                }
                _ => "",
            };
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim().to_ascii_lowercase(), value.trim());
        match (part, key.as_str()) {
            ("interface", "privatekey") => out.private_key = value.to_string(),
            ("interface", "address") => out.addresses.extend(list(value)),
            ("interface", "dns") => out.dns.extend(list(value)),
            ("interface", "listenport") => out.port = value.parse().unwrap_or(0),
            ("interface", "mtu") => out.mtu = value.parse().unwrap_or(0),
            ("peer", "publickey") => {
                if let Some(peer) = out.peers.last_mut() {
                    peer.public_key = value.to_string();
                }
            }
            ("peer", "presharedkey") => {
                if let Some(peer) = out.peers.last_mut() {
                    peer.preshared_key = value.to_string();
                }
            }
            ("peer", "allowedips") => {
                if let Some(peer) = out.peers.last_mut() {
                    peer.allowed.extend(list(value));
                }
            }
            ("peer", "endpoint") => {
                if let Some(peer) = out.peers.last_mut() {
                    peer.endpoint = value.to_string();
                }
            }
            ("peer", "persistentkeepalive") => {
                if let Some(peer) = out.peers.last_mut() {
                    peer.keepalive = value.parse().unwrap_or(0);
                }
            }
            _ => {}
        }
    }
    if out.private_key.is_empty() {
        return Err(crate::lang::t("This file has no private key, so it is not a WireGuard setup.").to_string());
    }
    if out.peers.iter().all(|peer| peer.public_key.is_empty()) {
        return Err(crate::lang::t("This file names no server to connect to.").to_string());
    }
    Ok(out)
}

pub fn is_wireguard(path: &std::path::Path) -> bool {
    path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("conf"))
        && std::fs::read_to_string(path).is_ok_and(|text| {
            let lower = text.to_ascii_lowercase();
            lower.contains("[interface]") && lower.contains("privatekey") && lower.contains("[peer]")
        })
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
        1..=3000 => crate::lang::t("2.4 GHz"),
        3001..=5924 => crate::lang::t("5 GHz"),
        _ => crate::lang::t("6 GHz"),
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
    let mut lines = vec![(crate::lang::t("Network").to_string(), String::new())];
    let mut add = |key: &str, value: String| lines.push((key.to_string(), value));
    add(crate::lang::t("Name"), net.ssid.clone());
    add(
        crate::lang::t("Status"),
        match (net.active, net.known) {
            (true, _) => crate::lang::t("Connected"),
            (false, true) => crate::lang::t("Saved, not connected"),
            (false, false) => crate::lang::t("Not saved"),
        }
        .to_string(),
    );
    let security = aps
        .first()
        .map(|ap| ap.security.trim().to_string())
        .filter(|words| !words.is_empty() && words != "--")
        .unwrap_or_else(|| crate::lang::t("None, open network").to_string());
    add(crate::lang::t("Security"), security);
    if let Some(ap) = aps.first()
        && !ap.flags.is_empty()
    {
        add(crate::lang::t("Keys"), keys(&ap.flags));
    }
    let strongest = aps.iter().map(|ap| ap.signal).max().unwrap_or(net.signal);
    add(crate::lang::t("Signal"), format!("{strongest}%, {}", bars(strongest)));
    if let Some(link) = link {
        lines.push((crate::lang::t("This connection").to_string(), String::new()));
        let mut add = |key: &str, value: String| lines.push((key.to_string(), value));
        if let Some(dbm) = link.dbm {
            add(crate::lang::t("Signal"), format!("{dbm} dBm"));
        }
        if let Some(rate) = &link.bitrate {
            add(crate::lang::t("Speed now"), rate.clone());
        }
        let named = [
            ("IP4.ADDRESS", crate::lang::t("IP address")),
            ("IP4.GATEWAY", crate::lang::t("Gateway")),
            ("IP4.DNS", "DNS"),
            ("IP6.ADDRESS", crate::lang::t("IPv6 address")),
            ("IP6.GATEWAY", crate::lang::t("IPv6 gateway")),
            ("IP6.DNS", crate::lang::t("IPv6 DNS")),
            ("GENERAL.HWADDR", crate::lang::t("Your MAC address")),
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
        add(crate::lang::t("Card"), link.card.clone());
    }
    if !aps.is_empty() {
        let heading = match aps.len() {
            1 => crate::lang::t("Access point").to_string(),
            count => crate::lang::tf("Access points ({})", &[&(count)]),
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

const ARCS: [&str; 3] = [
    "M6.535 12.735A4.9 4.9 0 0 1 13.465 12.735",
    "M3.919 10.119A8.6 8.6 0 0 1 16.081 10.119",
    "M1.303 7.503A12.3 12.3 0 0 1 18.697 7.503",
];

fn arc_svg(at: usize) -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="{}" fill="none" stroke="#000" stroke-width="2.3" stroke-linecap="round"/></svg>"##,
        ARCS[at.min(2)]
    )
}

const DOT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><circle cx="10" cy="16.2" r="1.9"/></svg>"##;

fn fan_svg() -> String {
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><g fill="none" stroke="#000" stroke-width="2.3" stroke-linecap="round"><path d="{}"/><path d="{}"/><path d="{}"/></g><circle cx="10" cy="16.2" r="1.9"/></svg>"##,
        ARCS[0], ARCS[1], ARCS[2]
    )
}

fn hollow_svg() -> String {
    let paths = ARCS.iter().map(|d| format!(r#"<path d="{d}"/>"#)).collect::<String>();
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><mask id="m"><rect width="20" height="20" fill="#fff"/><g fill="none" stroke="#000" stroke-width="0.85" stroke-linecap="round">{paths}</g><circle cx="10" cy="16.2" r="0.75" fill="#000"/></mask><g mask="url(#m)"><g fill="none" stroke="#000" stroke-width="2.6" stroke-linecap="round">{paths}</g><circle cx="10" cy="16.2" r="2.05"/></g></svg>"##
    )
}

const WIRED: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M4.5 4.5h11a1.5 1.5 0 0 1 1.5 1.5v6.5a1.5 1.5 0 0 1-1.5 1.5H13v2.2H7V14H4.5A1.5 1.5 0 0 1 3 12.5V6a1.5 1.5 0 0 1 1.5-1.5z" fill="none" stroke="#000" stroke-width="1.8" stroke-linejoin="round"/><path d="M7 7.4v2.2M10 7.4v2.2M13 7.4v2.2" stroke="#000" stroke-width="1.4" stroke-linecap="round"/></svg>"##;

const ALERT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M18.5 12.2v3.6" stroke="#000" stroke-width="2.2" stroke-linecap="round"/><circle cx="18.5" cy="18.6" r="1.25"/></svg>"##;

const LOCK_BADGE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M1.75 15.6v-1.5a1.35 1.35 0 0 1 2.7 0v1.5" fill="none" stroke="#000" stroke-width="1.1"/><rect x="0.7" y="15.2" width="4.8" height="4.1" rx="0.9"/></svg>"##;

const LOCK: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 12 12"><path d="M3.6 5.4V4a2.4 2.4 0 0 1 4.8 0v1.4" fill="none" stroke="#000" stroke-width="1.3"/><rect x="2.2" y="5.2" width="7.6" height="5.8" rx="1.3"/></svg>"##;

const SHIELD: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20"><path d="M10 2.2l6.3 2.4v4.6c0 4.1-2.7 7.3-6.3 8.6-3.6-1.3-6.3-4.5-6.3-8.6V4.6z" fill="none" stroke="#000" stroke-width="1.8" stroke-linejoin="round"/><path d="M7.4 9.9l1.9 1.9 3.5-3.6" fill="none" stroke="#000" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Glyph {
    Wifi,
    Wired,
    Vpn,
}

pub fn glyph(glyph: Glyph, side: u32, ink: [u8; 3]) -> crate::image::Image {
    match glyph {
        Glyph::Wifi => crate::blue::svg_glyph(&fan_svg(), side, ink),
        Glyph::Wired => crate::blue::svg_glyph(WIRED, side, ink),
        Glyph::Vpn => crate::blue::svg_glyph(SHIELD, side, ink),
    }
}

pub fn lock_glyph(side: u32, ink: [u8; 3]) -> crate::image::Image {
    crate::blue::svg_glyph(LOCK, side, ink)
}

pub struct TrayArt {
    pub dot: crate::image::Image,
    pub arcs: [crate::image::Image; 3],
    pub hollow: crate::image::Image,
    pub wired: crate::image::Image,
    pub alert: crate::image::Image,
    pub portal: crate::image::Image,
    pub lock: crate::image::Image,
}

pub fn tray_art(side: u32, ink: [u8; 3], accent: [u8; 3]) -> TrayArt {
    TrayArt {
        dot: crate::blue::svg_glyph(DOT, side, ink),
        arcs: [0, 1, 2].map(|at| crate::blue::svg_glyph(&arc_svg(at), side, ink)),
        hollow: crate::blue::svg_glyph(&hollow_svg(), side, ink),
        wired: crate::blue::svg_glyph(WIRED, side, ink),
        alert: crate::blue::svg_glyph(ALERT, side, ink),
        portal: crate::blue::svg_glyph(ALERT, side, accent),
        lock: crate::blue::svg_glyph(LOCK_BADGE, side, ink),
    }
}

pub const BASE: f64 = 0.3;
pub const OFF: f64 = 0.45;

pub fn wave(seconds: f64, arc: usize) -> f64 {
    let phase = (seconds - 0.15 * arc as f64) / 1.2;
    let lift = 0.5 - 0.5 * (phase * std::f64::consts::TAU).cos();
    BASE + (1.0 - BASE) * lift
}
