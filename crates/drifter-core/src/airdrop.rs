use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Works,
    Untested,
    Broken,
    NoCard,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card {
    pub iface: String,
    pub driver: String,
    pub device: Option<String>,
    pub name: Option<String>,
}

const PCI_IDS: [&str; 3] = ["/usr/share/hwdata/pci.ids", "/usr/share/misc/pci.ids", "/usr/share/pci.ids"];
const USB_IDS: [&str; 3] = ["/usr/share/hwdata/usb.ids", "/usr/share/misc/usb.ids", "/var/lib/usbutils/usb.ids"];

pub fn ids_name(ids: &str, device: &str, subsystem: Option<&str>) -> Option<String> {
    let (vendor, product) = device.split_once(':')?;
    let mut lines = ids.lines().skip_while(|line| !line.starts_with(&format!("{vendor}  ")));
    let maker = lines.next()?[vendor.len()..].trim().to_string();
    let mut lines = lines.take_while(|line| line.starts_with('\t') || line.starts_with('#') || line.is_empty());
    let model = lines.find_map(|line| line.strip_prefix(&format!("\t{product}  ")))?.trim().to_string();
    let sub = subsystem.and_then(|sub| {
        let (sub_vendor, sub_device) = sub.split_once(':')?;
        let want = format!("\t\t{sub_vendor} {sub_device}  ");
        lines.take_while(|line| line.starts_with("\t\t")).find_map(|line| line.strip_prefix(&want)).map(|name| name.trim().to_string())
    });
    let model = sub.unwrap_or(model);
    let model = match model.find(" [") {
        Some(at) => model[..at].to_string(),
        None => model,
    };
    let brand = maker.split([' ', ',']).next().unwrap_or_default();
    match model.to_lowercase().starts_with(&brand.to_lowercase()) {
        true => Some(model),
        false => Some(format!("{brand} {model}")),
    }
}

fn card_name(device: &str, subsystem: Option<&str>, usb: bool) -> Option<String> {
    let paths = if usb { USB_IDS } else { PCI_IDS };
    let ids = paths.iter().find_map(|path| std::fs::read_to_string(path).ok())?;
    ids_name(&ids, device, subsystem)
}

const TESTED: [&str; 4] = ["mt7925e", "mt76x2u", "iwlwifi", "rtw88_8822be"];
const BROKEN_DRIVERS: [&str; 3] = ["rtw88_8821ce", "ath11k_pci", "ath12k_wifi7_pci"];
const BROKEN_DEVICES: [&str; 3] = ["14c3:7927", "8086:095a", "8086:24f3"];

pub fn judge(card: &Card) -> Verdict {
    if card.device.as_deref().is_some_and(|device| BROKEN_DEVICES.contains(&device)) {
        return Verdict::Broken;
    }
    if BROKEN_DRIVERS.contains(&card.driver.as_str()) {
        return Verdict::Broken;
    }
    match TESTED.contains(&card.driver.as_str()) {
        true => Verdict::Works,
        false => Verdict::Untested,
    }
}

fn read_trim(path: &Path) -> Option<String> {
    std::fs::read_to_string(path).ok().map(|text| text.trim().to_string())
}

pub fn cards(net: &Path) -> Vec<Card> {
    let Ok(entries) = std::fs::read_dir(net) else {
        return Vec::new();
    };
    let mut out: Vec<Card> = entries
        .flatten()
        .filter(|entry| entry.path().join("wireless").exists() || entry.path().join("phy80211").exists())
        .filter_map(|entry| {
            let iface = entry.file_name().to_string_lossy().into_owned();
            if iface.starts_with("awdl") || iface.starts_with("owly") || iface.starts_with("mon") {
                return None;
            }
            let device = entry.path().join("device");
            let driver = std::fs::read_link(device.join("driver"))
                .ok()
                .and_then(|link| link.file_name().map(|name| name.to_string_lossy().into_owned()))?;
            let pci = read_trim(&device.join("vendor"))
                .zip(read_trim(&device.join("device")))
                .map(|(vendor, id)| format!("{}:{}", vendor.trim_start_matches("0x"), id.trim_start_matches("0x")));
            let usb = read_trim(&device.join("../idVendor")).zip(read_trim(&device.join("../idProduct"))).map(|(v, p)| format!("{v}:{p}"));
            let subsystem = read_trim(&device.join("subsystem_vendor"))
                .zip(read_trim(&device.join("subsystem_device")))
                .map(|(vendor, id)| format!("{}:{}", vendor.trim_start_matches("0x"), id.trim_start_matches("0x")));
            let name = match (&pci, &usb) {
                (Some(pci), _) => card_name(pci, subsystem.as_deref(), false),
                (None, Some(usb)) => card_name(usb, None, true),
                _ => None,
            };
            Some(Card {
                iface,
                driver,
                device: pci.or(usb),
                name,
            })
        })
        .collect();
    out.sort_by(|a, b| a.iface.cmp(&b.iface));
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Shared,
    Exclusive,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub verdict: Verdict,
    pub card: Option<Card>,
    pub mode: Option<Mode>,
    pub missing: Vec<String>,
    pub owly: bool,
    pub owlyd: bool,
    pub stale: bool,
}

impl Report {
    pub fn from_cards(cards: &[Card], owly: bool) -> Report {
        let best = cards
            .iter()
            .min_by_key(|card| match judge(card) {
                Verdict::Works => 0,
                Verdict::Untested => 1,
                Verdict::Broken => 2,
                Verdict::NoCard => 3,
            })
            .cloned();
        Report {
            verdict: best.as_ref().map_or(Verdict::NoCard, judge),
            mode: best.as_ref().map(|card| match card.driver.as_str() {
                "mt7925e" => Mode::Shared,
                _ => Mode::Exclusive,
            }),
            card: best,
            missing: Vec::new(),
            owly,
            owlyd: false,
            stale: false,
        }
    }

    pub fn from_probe(json: &str, owly: bool) -> Option<Report> {
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        let text = |key: &str| value.get(key).and_then(|item| item.as_str()).map(str::to_string);
        let verdict = match text("status").as_deref() {
            Some("tested") => Verdict::Works,
            Some("broken") => Verdict::Broken,
            Some(_) => Verdict::Untested,
            None if text("station").is_none() => Verdict::NoCard,
            None => Verdict::Untested,
        };
        let mode = match text("mode").as_deref() {
            Some("shared") => Some(Mode::Shared),
            Some(_) => Some(Mode::Exclusive),
            None => None,
        };
        let missing = value
            .get("missing")
            .and_then(|list| list.as_array())
            .map(|list| list.iter().filter_map(|item| item.get("say")?.as_str().map(|say| say.replace("Owly", "AirDrop"))).collect())
            .unwrap_or_default();
        let card = text("station").map(|iface| Card {
            iface,
            driver: text("driver").unwrap_or_default(),
            device: text("device"),
            name: None,
        });
        Some(Report {
            verdict,
            card,
            mode,
            missing,
            owly,
            owlyd: true,
            stale: false,
        })
    }

    pub fn merged(probed: Option<Report>, seen: Report) -> Report {
        match probed {
            Some(probed) if probed.verdict == Verdict::NoCard && seen.verdict != Verdict::NoCard => Report {
                missing: probed.missing,
                owlyd: true,
                ..seen
            },
            Some(mut probed) => {
                if let Some(card) = probed.card.as_mut()
                    && let Some(seen) = seen.card.filter(|seen| seen.iface == card.iface)
                {
                    card.name = seen.name;
                }
                probed
            }
            None => seen,
        }
    }

    pub fn shown(&self, force: bool) -> bool {
        force || self.verdict == Verdict::Works
    }

    pub fn ready(&self) -> bool {
        self.owly && self.owlyd && self.missing.is_empty()
    }

    pub fn words(&self) -> String {
        let card = self.card.as_ref().map(|card| match &card.device {
            Some(device) => format!("{}, {device}", card.driver),
            None => card.driver.clone(),
        });
        match (self.verdict, card) {
            (Verdict::NoCard, _) | (_, None) => crate::lang::t("This computer has no Wi-Fi card for AirDrop.").to_string(),
            (Verdict::Works, Some(card)) => crate::lang::tf("This Wi-Fi card works with AirDrop ({}).", &[&card]),
            (Verdict::Untested, Some(card)) => crate::lang::tf("Nobody has tested AirDrop on this Wi-Fi card yet ({}).", &[&card]),
            (Verdict::Broken, Some(card)) => crate::lang::tf("This Wi-Fi card is known not to work with AirDrop ({}).", &[&card]),
        }
    }

    pub fn mode_words(&self) -> Option<(&'static str, &'static str)> {
        match self.mode? {
            Mode::Shared => Some((crate::lang::t("Shared mode"), crate::lang::t("This card can use Wi-Fi and AirDrop at the same time. Wi-Fi stays connected while AirDrop is on."))),
            Mode::Exclusive => Some((
                crate::lang::t("Exclusive mode"),
                crate::lang::t("This card can't use Wi-Fi and AirDrop at the same time. While AirDrop is on, Wi-Fi turns off. Wi-Fi comes back when AirDrop stops, a few seconds after a transfer ends."),
            )),
        }
    }
}

pub fn owly_program() -> Option<PathBuf> {
    let beside = std::env::current_exe().ok().and_then(|exe| Some(exe.parent()?.join("owly")));
    beside.filter(|path| path.is_file()).or_else(|| {
        let path = std::env::var_os("PATH")?;
        std::env::split_paths(&path).map(|dir| dir.join("owly")).find(|path| path.is_file())
    })
}

pub fn setup_script() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let script = exe.parent()?.parent()?.parent()?.join("scripts/airdrop-service.sh");
    script.is_file().then_some(script)
}

pub fn glyph(side: u32, ink: [u8; 3]) -> crate::image::Image {
    crate::paint::at_density(1, || {
        let size = f64::from(side.max(8));
        let middle = crate::geom::Vec2::splat(size / 2.0);
        let mut canvas = crate::paint::Canvas::new(side.max(8), side.max(8));
        canvas.arc_from(middle, size * 0.42, size * 0.085, 0.58, 0.84, ink, 1.0);
        canvas.arc_from(middle, size * 0.25, size * 0.085, 0.6, 0.8, ink, 1.0);
        canvas.circle(middle, size * 0.09, ink, 1.0);
        canvas.image
    })
}

pub const RECEIVE_FOR: u64 = 600;
pub const INSTALLED: &str = "/usr/local/bin/drifter-owlyd";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Heard {
    Asking(String),
    Offered(String, Option<u64>),
    AskNow,
    Withdrawn,
    TimedOut,
    Failed(String),
}

pub fn unquote(text: &str) -> String {
    let text = text.trim();
    let Some(inner) = text.strip_prefix('"').and_then(|rest| rest.strip_suffix('"')) else {
        return text.to_string();
    };
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        match (c, c == '\\') {
            (_, true) => match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other),
                None => {}
            },
            (c, false) => out.push(c),
        }
    }
    out
}

fn quoted_first(text: &str) -> Option<(String, &str)> {
    let rest = text.trim_start().strip_prefix('"')?;
    let mut escaped = false;
    for (at, c) in rest.char_indices() {
        match (c, escaped) {
            ('\\', false) => escaped = true,
            ('"', false) => return Some((unquote(&format!("\"{}\"", &rest[..at])), &rest[at + 1..])),
            _ => escaped = false,
        }
    }
    None
}

pub fn read_receiver_line(line: &str) -> Option<Heard> {
    let line = line.trim_end_matches(['\r', '\n']);
    if line.trim() == "accept? [y/N]" || line.ends_with("accept? [y/N] ") {
        return Some(Heard::AskNow);
    }
    if line.trim() == "timed out" {
        return Some(Heard::TimedOut);
    }
    if line.trim() == "the request was withdrawn" {
        return Some(Heard::Withdrawn);
    }
    if let Some(reason) = line.strip_prefix("transfer failed: ") {
        return Some(Heard::Failed(reason.to_string()));
    }
    if line.ends_with("wants to send:") {
        let (name, _) = quoted_first(line)?;
        return Some(Heard::Asking(name));
    }
    if line.starts_with("  \"") {
        let (name, rest) = quoted_first(line)?;
        let size = rest.split_whitespace().rev().nth(1).and_then(|number| number.parse().ok());
        let size = size.filter(|_| rest.trim_end().ends_with("bytes"));
        return Some(Heard::Offered(name, size));
    }
    None
}

pub fn read_browse_line(line: &str) -> Option<(String, String, Option<String>)> {
    let line = line.trim();
    let (instance, rest) = line.split_once(char::is_whitespace)?;
    let (name, rest) = quoted_first(rest)?;
    let mac = rest
        .trim()
        .strip_prefix('(')
        .and_then(|inner| inner.rsplit_once(':'))
        .and_then(|(addr, _)| crate::share::mac_from_link_local(addr));
    (!instance.is_empty() && !name.is_empty()).then(|| (instance.to_string(), name, mac))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sent {
    Progress(u64, u64),
    Accepted,
    Delivered,
    Declined,
    Failed(String),
}

pub fn read_sender_text(text: &str) -> Vec<Sent> {
    let mut out = Vec::new();
    for piece in text.split(['\r', '\n']) {
        let piece = piece.trim();
        if piece.is_empty() {
            continue;
        }
        if let Some((sent, total)) = piece.strip_suffix(" bytes").and_then(|rest| rest.split_once(" / ")) {
            if let (Ok(sent), Ok(total)) = (sent.trim().parse(), total.trim().parse()) {
                out.push(Sent::Progress(sent, total));
            }
            continue;
        }
        match piece {
            "accepted" => out.push(Sent::Accepted),
            "delivered" => out.push(Sent::Delivered),
            "declined" => out.push(Sent::Declined),
            _ => {
                if let Some(reason) = piece.strip_prefix("failed: ") {
                    out.push(Sent::Failed(reason.to_string()));
                } else if let Some(reason) = piece.strip_prefix("owly: ") {
                    out.push(Sent::Failed(reason.to_string()));
                }
            }
        }
    }
    out
}

pub fn plain_failure(reason: &str) -> String {
    let has = |words: &[&str]| words.iter().any(|word| reason.contains(word));
    if has(&["NotAuthorized", "not authorized", "polkit"]) {
        return crate::lang::t("Drifter was not allowed to use the Wi-Fi radio for AirDrop.").to_string();
    }
    if has(&["Busy", "busy"]) {
        return crate::lang::t("Another app is using AirDrop right now.").to_string();
    }
    if has(&["ServiceUnknown", "can't be reached", "owlyd"]) {
        return crate::lang::t("AirDrop's radio service is not set up or did not start.").to_string();
    }
    if has(&["didn't open its port", "no AWDL peer", "no such receiver", "TimedOut", "timed out"]) {
        return crate::lang::t("The device did not answer. Unlock it, open AirDrop, set it to Everyone for 10 minutes, then try again.").to_string();
    }
    if has(&["declined"]) {
        return crate::lang::t("They said no.").to_string();
    }
    crate::lang::t("AirDrop did not finish. Try again.").to_string()
}

pub fn received_folder(folder: Option<&str>, home: &Path) -> PathBuf {
    match folder.map(str::trim) {
        Some("") | None => crate::folders::user_dir(home, "DOWNLOAD", "Downloads"),
        Some(folder) => match folder.strip_prefix("~/") {
            Some(rest) => home.join(rest),
            None if folder == "~" => home.to_path_buf(),
            None => PathBuf::from(folder),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(driver: &str, device: Option<&str>) -> Card {
        Card {
            name: None,
            iface: "wlan0".into(),
            driver: driver.into(),
            device: device.map(str::to_string),
        }
    }

    #[test]
    fn known_cards_are_judged_like_owly_does() {
        assert_eq!(judge(&card("iwlwifi", Some("8086:2526"))), Verdict::Works);
        assert_eq!(judge(&card("iwlwifi", Some("8086:095a"))), Verdict::Broken);
        assert_eq!(judge(&card("mt7925e", Some("14c3:0717"))), Verdict::Works);
        assert_eq!(judge(&card("mt7925e", Some("14c3:7927"))), Verdict::Broken);
        assert_eq!(judge(&card("ath11k_pci", None)), Verdict::Broken);
        assert_eq!(judge(&card("rtw89_8852be", None)), Verdict::Untested);
        assert_eq!(judge(&card("brcmfmac", None)), Verdict::Untested);
    }

    #[test]
    fn a_compatible_card_shows_airdrop_and_force_shows_it_anyway() {
        let works = Report::from_cards(&[card("ath11k_pci", None), card("iwlwifi", Some("8086:a370"))], false);
        assert_eq!(works.verdict, Verdict::Works, "the best card decides");
        assert_eq!(works.card.as_ref().unwrap().driver, "iwlwifi");
        assert_eq!(works.mode, Some(Mode::Exclusive));
        assert!(works.shown(false));
        assert!(!works.ready(), "the radio service is still needed to use it");
        let none = Report::from_cards(&[], false);
        assert_eq!(none.verdict, Verdict::NoCard);
        assert!(!none.shown(false));
        assert!(none.shown(true), "the force switch shows it");
        let broken = Report::from_cards(&[card("ath12k_wifi7_pci", None)], true);
        assert!(!broken.shown(false));
    }

    #[test]
    fn the_cards_are_read_from_sys() {
        let root = std::env::temp_dir().join(format!("drifter-airdrop-sys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let net = root.join("class/net");
        let device = root.join("devices/pci/0000:03:00.0");
        let drivers = root.join("bus/pci/drivers/iwlwifi");
        std::fs::create_dir_all(net.join("wlan0/wireless")).unwrap();
        std::fs::create_dir_all(net.join("eth0")).unwrap();
        std::fs::create_dir_all(&device).unwrap();
        std::fs::create_dir_all(&drivers).unwrap();
        std::fs::write(device.join("vendor"), "0x8086\n").unwrap();
        std::fs::write(device.join("device"), "0x2526\n").unwrap();
        std::os::unix::fs::symlink(&device, net.join("wlan0/device")).unwrap();
        std::os::unix::fs::symlink(&drivers, device.join("driver")).unwrap();
        let found = cards(&net);
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(found, vec![card("iwlwifi", Some("8086:2526"))]);
    }

    #[test]
    fn a_card_is_named_from_the_hardware_list() {
        let ids = "# list\n10ec  Realtek Semiconductor Co., Ltd.\n\t8822  RTL8822BE\n8086  Intel Corporation\n\t2526  Wi-Fi 5(802.11ac) Wireless-AC 9x6x [Thunder Peak]\n\t\t8086 0014  Dual Band Wi-Fi 5 Wireless-AC 9260 160MHz 2x2\n\t2530  Host Bridge\n";
        assert_eq!(ids_name(ids, "8086:2526", Some("8086:0014")).as_deref(), Some("Intel Dual Band Wi-Fi 5 Wireless-AC 9260 160MHz 2x2"));
        assert_eq!(ids_name(ids, "8086:2526", Some("1028:0001")).as_deref(), Some("Intel Wi-Fi 5(802.11ac) Wireless-AC 9x6x"));
        assert_eq!(ids_name(ids, "10ec:8822", None).as_deref(), Some("Realtek RTL8822BE"));
        assert_eq!(ids_name(ids, "8086:9999", None), None);
    }

    #[test]
    fn a_service_that_misses_the_card_does_not_hide_it() {
        let blind = Report::from_probe(r#"{"station":null,"missing":[{"id":"hardware","say":"no Wi-Fi interface"}]}"#, true).unwrap();
        assert_eq!(blind.verdict, Verdict::NoCard);
        let seen = Report::from_cards(&[card("iwlwifi", Some("8086:2526"))], true);
        let merged = Report::merged(Some(blind.clone()), seen.clone());
        assert_eq!(merged.verdict, Verdict::Works);
        assert!(merged.owlyd && !merged.ready() && merged.shown(false));
        let none = Report::from_cards(&[], true);
        assert_eq!(Report::merged(Some(blind), none.clone()).verdict, Verdict::NoCard);
        assert_eq!(Report::merged(None, seen.clone()), seen);
    }

    #[test]
    fn owlys_probe_report_is_read() {
        let json = r#"{"station":"wlp3s0","driver":"iwlwifi","device":"8086:2526","status":"tested","mode":"exclusive","channel":6,"missing":[],"issues":["Intel cards start no ad-hoc cell on 5 GHz, so they run on channel 6."]}"#;
        let report = Report::from_probe(json, true).unwrap();
        assert_eq!(report.verdict, Verdict::Works);
        assert_eq!(report.mode, Some(Mode::Exclusive));
        assert!(report.ready());
        let blocked = r#"{"station":"wlan0","driver":"mt7921e","status":"untested","mode":"active-monitor","missing":[{"id":"untested","say":"This card is untested."}]}"#;
        let report = Report::from_probe(blocked, true).unwrap();
        assert_eq!(report.verdict, Verdict::Untested);
        assert_eq!(report.missing, ["This card is untested."]);
        assert!(!report.ready());
        assert_eq!(Report::from_probe("not json", true), None);
    }

    #[test]
    fn the_receiver_talk_is_read() {
        assert_eq!(read_receiver_line("\"Jane's iPhone\" (\"iPhone\") wants to send:"), Some(Heard::Asking("Jane's iPhone".into())));
        assert_eq!(read_receiver_line("  \"IMG_0001.HEIC\" public.heic 2400000 bytes"), Some(Heard::Offered("IMG_0001.HEIC".into(), Some(2_400_000))));
        assert_eq!(read_receiver_line("  \"say \\\"hi\\\".txt\" public.plain-text "), Some(Heard::Offered("say \"hi\".txt".into(), None)));
        assert_eq!(read_receiver_line("accept? [y/N] "), Some(Heard::AskNow));
        assert_eq!(read_receiver_line("the request was withdrawn"), Some(Heard::Withdrawn));
        assert_eq!(read_receiver_line("transfer failed: reset"), Some(Heard::Failed("reset".into())));
        assert_eq!(read_receiver_line("seen by fe80::1"), None);
        assert_eq!(read_browse_line("a1b2c3d4e5f6   \"Studio Mac\" ([fe80::1%5]:8770)"), Some(("a1b2c3d4e5f6".into(), "Studio Mac".into(), None)));
        assert_eq!(
            read_browse_line("a1b2c3d4e5f6   \"Studio Mac\" ([fe80::a8bb:ccff:fedd:ee01%5]:8770)").and_then(|(_, _, mac)| mac).as_deref(),
            Some("aa:bb:cc:dd:ee:01")
        );
    }

    #[test]
    fn the_sender_talk_is_read() {
        let heard = read_sender_text("accepted\n\r1000 / 5000 bytes\r5000 / 5000 bytes\ndelivered\n");
        assert_eq!(heard, [Sent::Accepted, Sent::Progress(1000, 5000), Sent::Progress(5000, 5000), Sent::Delivered]);
        assert_eq!(read_sender_text("failed: reset\n"), [Sent::Failed("reset".into())]);
        assert!(plain_failure("the receiver didn't open its port within 30s").contains("Unlock it"));
        assert!(!plain_failure("weird").contains("owly"));
    }
}
