use crate::image::Image;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    Headphones,
    Earbuds,
    Speaker,
    Mouse,
    Keyboard,
    Trackpad,
    Controller,
    Phone,
    Tablet,
    Computer,
    Watch,
    Display,
    Printer,
    Other,
}

impl Kind {
    pub fn guess(icon: &str, class: Option<u32>, appearance: Option<u16>, name: &str) -> Kind {
        let lower = name.to_lowercase();
        let named = |words: &[&str]| words.iter().any(|word| lower.contains(word));
        if named(&["airpods", "buds", "earbuds", "galaxy buds", "pixel buds", "beats fit", "powerbeats"]) {
            return Kind::Earbuds;
        }
        if named(&["xbox", "dualsense", "dualshock", "controller", "gamepad", "joy-con", "8bitdo", "pro controller"]) {
            return Kind::Controller;
        }
        if named(&["trackpad", "touchpad"]) {
            return Kind::Trackpad;
        }
        if named(&["monitor", "webos", " tv", "television", "bravia"]) {
            return Kind::Display;
        }
        match icon {
            "audio-headset" | "audio-headphones" => return Kind::Headphones,
            "audio-card" | "audio-speakers" => return Kind::Speaker,
            "input-mouse" => return Kind::Mouse,
            "input-keyboard" => return Kind::Keyboard,
            "input-tablet" => return Kind::Trackpad,
            "input-gaming" => return Kind::Controller,
            "phone" => return Kind::Phone,
            "computer" => return Kind::Computer,
            "video-display" | "camera-video" => return Kind::Display,
            "printer" | "scanner" => return Kind::Printer,
            _ => {}
        }
        if let Some(appearance) = appearance {
            match appearance >> 6 {
                1 => return Kind::Phone,
                2 => return Kind::Computer,
                3 => return Kind::Watch,
                15 => match appearance & 0x3f {
                    1 => return Kind::Keyboard,
                    2 => return Kind::Mouse,
                    3 | 4 => return Kind::Controller,
                    5 => return Kind::Trackpad,
                    _ => {}
                },
                37 => return Kind::Earbuds,
                33 | 34 => return Kind::Speaker,
                _ => {}
            }
        }
        if let Some(class) = class {
            let (major, minor) = ((class >> 8) & 0x1f, (class >> 2) & 0x3f);
            match major {
                1 => return Kind::Computer,
                2 => return Kind::Phone,
                4 => {
                    return match minor {
                        1 | 2 | 6 => Kind::Headphones,
                        5 | 7 | 10 => Kind::Speaker,
                        _ => Kind::Headphones,
                    };
                }
                5 => {
                    return match (minor >> 4, minor & 0x0f) {
                        (1, _) => Kind::Keyboard,
                        (2, _) => Kind::Mouse,
                        (_, 1 | 2) => Kind::Controller,
                        (_, 5) => Kind::Trackpad,
                        _ => Kind::Mouse,
                    };
                }
                6 => return Kind::Printer,
                7 => return Kind::Watch,
                _ => {}
            }
        }
        if named(&["headphone", "wh-1000", "wf-1000", "bose", "sony wh", "jabra", "headset", "beats"]) {
            return Kind::Headphones;
        }
        if named(&["speaker", "soundlink", "jbl", "boom", "sonos", "homepod"]) {
            return Kind::Speaker;
        }
        if named(&["keyboard", "keys"]) {
            return Kind::Keyboard;
        }
        if named(&["mouse", "mx ", "magic mouse"]) {
            return Kind::Mouse;
        }
        if named(&["iphone", "pixel", "galaxy", "phone"]) {
            return Kind::Phone;
        }
        if named(&["ipad", "tablet"]) {
            return Kind::Tablet;
        }
        if named(&["watch"]) {
            return Kind::Watch;
        }
        Kind::Other
    }

    pub fn label(self) -> &'static str {
        match self {
            Kind::Headphones => crate::lang::t("Headphones"),
            Kind::Earbuds => crate::lang::t("Earbuds"),
            Kind::Speaker => crate::lang::t("Speaker"),
            Kind::Mouse => crate::lang::t("Mouse"),
            Kind::Keyboard => crate::lang::t("Keyboard"),
            Kind::Trackpad => crate::lang::t("Trackpad"),
            Kind::Controller => crate::lang::t("Game controller"),
            Kind::Phone => crate::lang::t("Phone"),
            Kind::Tablet => crate::lang::t("Tablet"),
            Kind::Computer => crate::lang::t("Computer"),
            Kind::Watch => crate::lang::t("Watch"),
            Kind::Display => crate::lang::t("Display"),
            Kind::Printer => crate::lang::t("Printer"),
            Kind::Other => crate::lang::t("Device"),
        }
    }

    pub fn unnamed(self) -> &'static str {
        match self {
            Kind::Headphones => crate::lang::t("Unnamed headphones"),
            Kind::Earbuds => crate::lang::t("Unnamed earbuds"),
            Kind::Speaker => crate::lang::t("Unnamed speaker"),
            Kind::Mouse => crate::lang::t("Unnamed mouse"),
            Kind::Keyboard => crate::lang::t("Unnamed keyboard"),
            Kind::Trackpad => crate::lang::t("Unnamed trackpad"),
            Kind::Controller => crate::lang::t("Unnamed controller"),
            Kind::Phone => crate::lang::t("Unnamed phone"),
            Kind::Tablet => crate::lang::t("Unnamed tablet"),
            Kind::Computer => crate::lang::t("Unnamed computer"),
            Kind::Watch => crate::lang::t("Unnamed watch"),
            Kind::Display => crate::lang::t("Unnamed display"),
            Kind::Printer => crate::lang::t("Unnamed printer"),
            Kind::Other => crate::lang::t("Unnamed device"),
        }
    }

    pub fn audio(self) -> bool {
        matches!(self, Kind::Headphones | Kind::Earbuds | Kind::Speaker)
    }

    pub fn svg(self) -> &'static str {
        match self {
            Kind::Headphones => HEADPHONES,
            Kind::Earbuds => EARBUDS,
            Kind::Speaker => SPEAKER,
            Kind::Mouse => MOUSE,
            Kind::Keyboard => KEYBOARD,
            Kind::Trackpad => TRACKPAD,
            Kind::Controller => CONTROLLER,
            Kind::Phone => PHONE,
            Kind::Tablet => TABLET,
            Kind::Computer => COMPUTER,
            Kind::Watch => WATCH,
            Kind::Display => DISPLAY,
            Kind::Printer => PRINTER,
            Kind::Other => RUNE,
        }
    }
}

const HEADPHONES: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M4 15v-3a8 8 0 0 1 16 0v3" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round"/><rect x="3" y="13.5" width="5" height="7.5" rx="2.2"/><rect x="16" y="13.5" width="5" height="7.5" rx="2.2"/></svg>"##;
const EARBUDS: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><circle cx="7" cy="7.5" r="4"/><rect x="5.6" y="9" width="2.8" height="11" rx="1.4"/><circle cx="17" cy="7.5" r="4"/><rect x="15.6" y="9" width="2.8" height="11" rx="1.4"/></svg>"##;
const SPEAKER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill-rule="evenodd" d="M8 2h8a3 3 0 0 1 3 3v14a3 3 0 0 1-3 3H8a3 3 0 0 1-3-3V5a3 3 0 0 1 3-3zm4 9a4 4 0 1 0 0 8a4 4 0 1 0 0-8zm0-6.5a1.8 1.8 0 1 0 0 3.6a1.8 1.8 0 1 0 0-3.6z"/></svg>"##;
const MOUSE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="6" y="2.5" width="12" height="19" rx="6" fill="none" stroke="#000" stroke-width="2"/><rect x="11" y="6" width="2" height="4.5" rx="1"/></svg>"##;
const KEYBOARD: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="1.5" y="6" width="21" height="12" rx="2.5" fill="none" stroke="#000" stroke-width="1.8"/><g><rect x="4.5" y="9" width="2" height="2" rx=".5"/><rect x="8" y="9" width="2" height="2" rx=".5"/><rect x="11.5" y="9" width="2" height="2" rx=".5"/><rect x="15" y="9" width="2" height="2" rx=".5"/><rect x="18" y="9" width="2" height="2" rx=".5"/><rect x="7" y="13" width="10" height="2" rx=".8"/></g></svg>"##;
const TRACKPAD: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="2.5" y="4" width="19" height="16" rx="3" fill="none" stroke="#000" stroke-width="2"/><rect x="5.5" y="15.5" width="13" height="1.6" rx=".8"/></svg>"##;
const CONTROLLER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill-rule="evenodd" d="M7.2 6h9.6c2.6 0 4.2 1.6 4.8 4.4l1.2 6.2c.4 2.2-1.6 3.6-3.4 2.3L16.6 17H7.4l-2.8 1.9C2.8 20.2.8 18.8 1.2 16.6l1.2-6.2C3 7.6 4.6 6 7.2 6zM6.2 9.2v1.6H4.6v1.8h1.6v1.6H8v-1.6h1.6v-1.8H8V9.2zm10.6.2a1.1 1.1 0 1 0 0 2.2a1.1 1.1 0 1 0 0-2.2zm-2.2 2.2a1.1 1.1 0 1 0 0 2.2a1.1 1.1 0 1 0 0-2.2z"/></svg>"##;
const PHONE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="6" y="1.5" width="12" height="21" rx="3" fill="none" stroke="#000" stroke-width="2"/><rect x="9.5" y="4" width="5" height="1.6" rx=".8"/></svg>"##;
const TABLET: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="3.5" y="2" width="17" height="20" rx="2.5" fill="none" stroke="#000" stroke-width="2"/><circle cx="12" cy="18.6" r="1"/></svg>"##;
const COMPUTER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="4" y="4" width="16" height="11" rx="1.6" fill="none" stroke="#000" stroke-width="2"/><path d="M1.5 18.5h21a1 1 0 0 1-1 1.5h-19a1 1 0 0 1-1-1.5z"/></svg>"##;
const WATCH: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M8.5 1.5h7l.6 4h-8.2zM8.5 22.5h7l.6-4h-8.2z"/><rect x="5.5" y="5" width="13" height="14" rx="3.5" fill="none" stroke="#000" stroke-width="2"/></svg>"##;
const DISPLAY: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><rect x="2" y="3.5" width="20" height="13" rx="2" fill="none" stroke="#000" stroke-width="2"/><rect x="9" y="19" width="6" height="2" rx="1"/></svg>"##;
const PRINTER: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path fill-rule="evenodd" d="M7 2h10v5H7zM4 8h16a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2h-2v-3H6v3H4a2 2 0 0 1-2-2v-6a2 2 0 0 1 2-2z"/><rect x="7" y="16" width="10" height="6" rx=".8" fill="none" stroke="#000" stroke-width="1.6"/></svg>"##;
const RUNE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M6.5 7.5l11 9-5.5 4.5V3l5.5 4.5-11 9" fill="none" stroke="#000" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;

pub fn kind_word(kind: Kind) -> &'static str {
    KINDS.iter().find(|(each, _)| *each == kind).map_or("other", |(_, word)| word)
}

pub fn kind_from_word(word: &str) -> Kind {
    KINDS.iter().find(|(_, each)| *each == word).map_or(Kind::Other, |(kind, _)| *kind)
}

const KINDS: [(Kind, &str); 14] = [
    (Kind::Headphones, "headphones"),
    (Kind::Earbuds, "earbuds"),
    (Kind::Speaker, "speaker"),
    (Kind::Mouse, "mouse"),
    (Kind::Keyboard, "keyboard"),
    (Kind::Trackpad, "trackpad"),
    (Kind::Controller, "controller"),
    (Kind::Phone, "phone"),
    (Kind::Tablet, "tablet"),
    (Kind::Computer, "computer"),
    (Kind::Watch, "watch"),
    (Kind::Display, "display"),
    (Kind::Printer, "printer"),
    (Kind::Other, "other"),
];

pub fn disc_image(kind: Kind, lit: bool, side: u32, accent: [u8; 3], off: [u8; 3], ink: [u8; 3]) -> Image {
    crate::paint::at_density(1, || {
        let size = f64::from(side.max(8));
        let mut canvas = crate::paint::Canvas::new(side.max(8), side.max(8));
        canvas.circle(crate::geom::Vec2::splat(size / 2.0), size / 2.0, if lit { accent } else { off }, 1.0);
        let inner = (size * 0.58).round() as u32;
        let mark = glyph(kind, inner, if lit { [255, 255, 255] } else { ink });
        canvas.stamp(&mark, crate::geom::Vec2::splat(((size - f64::from(inner)) / 2.0).round()), 1.0);
        canvas.image
    })
}

pub fn rune() -> &'static str {
    RUNE
}

pub fn glyph(kind: Kind, side: u32, ink: [u8; 3]) -> Image {
    svg_glyph(kind.svg(), side, ink)
}

pub fn svg_glyph(svg: &str, side: u32, ink: [u8; 3]) -> Image {
    let side = side.max(4);
    let mut image = crate::image::draw_svg(svg.as_bytes(), side).unwrap_or(Image {
        width: side,
        height: side,
        rgba: vec![0; (side * side * 4) as usize],
    });
    image.tint(ink);
    image
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub path: String,
    pub address: String,
    pub name: String,
    pub named: bool,
    pub kind: Kind,
    pub paired: bool,
    pub connected: bool,
    pub trusted: bool,
    pub battery: Option<u8>,
    pub rssi: Option<i16>,
    pub audio: bool,
    pub sends: bool,
    pub classic: bool,
}

impl Device {
    pub fn shown_name(&self) -> String {
        match self.named {
            true => self.name.clone(),
            false => self.kind.unnamed().to_string(),
        }
    }

    pub fn state(&self) -> &'static str {
        match (self.paired, self.connected) {
            (_, true) => crate::lang::t("Connected"),
            (true, false) => crate::lang::t("Not connected"),
            (false, false) => crate::lang::t("Not set up"),
        }
    }

    pub fn note(&self) -> String {
        match (self.connected, self.battery) {
            (true, Some(level)) => format!("{}, {level}%", self.state()),
            _ => self.state().to_string(),
        }
    }

    pub fn bars(&self) -> Option<u8> {
        self.rssi.map(bars)
    }
}

pub fn named(name: &str, address: &str) -> bool {
    let name = name.trim();
    !name.is_empty() && name.replace('-', ":").to_uppercase() != address.to_uppercase()
}

pub fn bars(rssi: i16) -> u8 {
    match rssi {
        i16::MIN..=-90 => 1,
        -89..=-78 => 2,
        -77..=-65 => 3,
        _ => 4,
    }
}

pub fn closer(rssi: Option<i16>) -> Option<&'static str> {
    rssi.filter(|rssi| *rssi <= -80).map(|_| crate::lang::t("Move closer"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Missing {
    Service,
    Adapter,
}

impl Missing {
    pub fn words(self) -> &'static str {
        match self {
            Missing::Service => crate::lang::t("The Bluetooth service is off."),
            Missing::Adapter => crate::lang::t("No Bluetooth adapter found."),
        }
    }
}

pub type Seen = Result<Blue, Missing>;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Blue {
    pub adapter: String,
    pub on: bool,
    pub searching: bool,
    pub devices: Vec<Device>,
}

impl Blue {
    pub fn mine(&self) -> impl Iterator<Item = &Device> {
        self.devices.iter().filter(|device| device.paired)
    }

    pub fn nearby(&self) -> impl Iterator<Item = &Device> {
        self.devices.iter().filter(|device| !device.paired && (device.named || device.rssi.is_some()))
    }

    pub fn find(&self, address: &str) -> Option<&Device> {
        self.devices.iter().find(|device| device.address.eq_ignore_ascii_case(address))
    }

    pub fn connected(&self) -> impl Iterator<Item = &Device> {
        self.devices.iter().filter(|device| device.connected)
    }

    pub fn sort(&mut self) {
        self.devices.sort_by(|a, b| {
            b.paired
                .cmp(&a.paired)
                .then(b.connected.cmp(&a.connected))
                .then(b.named.cmp(&a.named))
                .then(match a.paired {
                    true => std::cmp::Ordering::Equal,
                    false => b.rssi.unwrap_or(i16::MIN).cmp(&a.rssi.unwrap_or(i16::MIN)),
                })
                .then(a.shown_name().to_lowercase().cmp(&b.shown_name().to_lowercase()))
                .then(a.address.cmp(&b.address))
        });
    }

    pub fn note(&self) -> String {
        if !self.on {
            return crate::lang::t("Off").to_string();
        }
        let on: Vec<String> = self.connected().map(Device::shown_name).collect();
        match on.as_slice() {
            [] => crate::lang::t("On").to_string(),
            [one] => one.clone(),
            [one, rest @ ..] => crate::lang::tf("{} and {} more", &[one, &rest.len()]),
        }
    }
}

pub const MENU_POWER: i32 = 1000;
pub const MENU_SETTINGS: i32 = 1001;

pub fn menu(blue: &Blue) -> Vec<crate::menu::Item> {
    use crate::menu::Item;
    let mut items = Vec::new();
    if blue.on {
        for (at, device) in blue.devices.iter().enumerate().filter(|(_, device)| device.paired) {
            let state = match (device.connected, device.battery) {
                (true, Some(level)) => format!("{level}%"),
                (true, None) => crate::lang::t("Connected").to_string(),
                (false, _) => crate::lang::t("Connect").to_string(),
            };
            items.push(Item::line(at as i32, &format!("{}, {state}", device.shown_name())));
        }
        if !items.is_empty() {
            items.push(Item::rule(-3));
        }
    }
    items.push(Item::line(
        MENU_POWER,
        match blue.on {
            true => crate::lang::t("Turn Bluetooth Off"),
            false => crate::lang::t("Turn Bluetooth On"),
        },
    ));
    items.push(Item::line(MENU_SETTINGS, crate::lang::t("Bluetooth Settings…")));
    items
}

pub const LOW: [u8; 2] = [20, 10];

pub fn dropped_to(before: Option<u8>, now: Option<u8>) -> Option<u8> {
    let (before, now) = (before?, now?);
    LOW.iter().copied().filter(|line| before > *line && now <= *line).min()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Connected(String),
    Disconnected(String),
    Low(String, u8),
    Appeared(String),
    Gone(String),
}

pub fn changes(before: &Blue, now: &Blue) -> Vec<Change> {
    let mut out = Vec::new();
    for device in &now.devices {
        match before.find(&device.address) {
            None => out.push(Change::Appeared(device.address.clone())),
            Some(was) => {
                if device.connected && !was.connected {
                    out.push(Change::Connected(device.address.clone()));
                }
                if !device.connected && was.connected {
                    out.push(Change::Disconnected(device.address.clone()));
                }
                if let Some(line) = dropped_to(was.battery, device.battery) {
                    out.push(Change::Low(device.address.clone(), line));
                }
            }
        }
    }
    for was in &before.devices {
        if now.find(&was.address).is_none() {
            out.push(Change::Gone(was.address.clone()));
        }
    }
    out
}

pub fn plain_error(text: &str, device: &str, kind: Kind) -> String {
    let has = |words: &[&str]| words.iter().any(|word| text.contains(word));
    if has(&["AuthenticationFailed", "AuthenticationRejected"]) {
        return crate::lang::t("The code did not match. Try again.").to_string();
    }
    if has(&["AuthenticationCanceled", "Canceled", "Cancelled"]) {
        return crate::lang::t("Pairing was stopped.").to_string();
    }
    if has(&["AuthenticationTimeout"]) {
        return crate::lang::tf("{} waited too long. Try again and answer on the device.", &[&device]);
    }
    if has(&["AlreadyExists", "Already Exists"]) {
        return crate::lang::t("It is already set up.").to_string();
    }
    if has(&["NotReady", "Resource Not Ready"]) {
        return crate::lang::t("Bluetooth is still starting. Try again in a moment.").to_string();
    }
    if has(&["InProgress", "In Progress"]) {
        return crate::lang::t("Drifter is already doing that. Give it a moment.").to_string();
    }
    if has(&["br-connection-profile-unavailable", "le-connection-profile-unavailable", "NotAvailable", "profile-unavailable"]) {
        return match kind.audio() {
            true => crate::lang::tf("{} is linked to another device. Turn off Bluetooth on your phone, then try again.", &[&device]),
            false => crate::lang::tf("{} does not offer anything Drifter can use.", &[&device]),
        };
    }
    if has(&["page-timeout", "PageTimeout", "ConnectionAttemptFailed", "le-connection-abort-by-local", "Host is down", "connect-failed"]) {
        return match kind.audio() {
            true => crate::lang::tf("{} did not answer. It may be linked to your phone. Turn off Bluetooth on the phone, or put {} in its case and take it out, then try again.", &[&device, &device]),
            false => crate::lang::tf("{} did not answer. Turn it on, bring it closer, then try again.", &[&device]),
        };
    }
    if has(&["DoesNotExist", "UnknownObject", "No such"]) {
        return crate::lang::t("The device went away. Search again.").to_string();
    }
    if has(&["NotPowered", "not powered", "Powered off"]) {
        return crate::lang::t("Bluetooth is off. Turn it on, then try again.").to_string();
    }
    if has(&["Blocked", "rfkill"]) {
        return crate::lang::t("Bluetooth is blocked by a switch or airplane mode.").to_string();
    }
    if has(&["NoReply", "Timeout", "timed out"]) {
        return crate::lang::tf("{} took too long to answer. Try again.", &[&device]);
    }
    crate::lang::tf("{} did not connect. Try Fix It in Bluetooth settings.", &[&device])
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    Sbc,
    SbcXq,
    Aac,
    Aptx,
    AptxHd,
    AptxLl,
    Ldac,
    Lc3,
    Opus,
    Call,
}

impl Codec {
    pub fn read(name: &str) -> Option<Codec> {
        let name = name.trim().trim_matches('"').to_lowercase();
        Some(match name.as_str() {
            "sbc" => Codec::Sbc,
            "sbc_xq" => Codec::SbcXq,
            "aac" | "aac_eld" => Codec::Aac,
            "aptx" => Codec::Aptx,
            "aptx_hd" => Codec::AptxHd,
            "aptx_ll" | "aptx_ll_duplex" => Codec::AptxLl,
            "ldac" => Codec::Ldac,
            "lc3" | "lc3plus_h3" => Codec::Lc3,
            "opus_05" | "opus_g" | "opus_05_51" | "opus_05_71" | "opus_05_duplex" | "opus_05_pro" => Codec::Opus,
            "msbc" | "cvsd" | "lc3_swb" => Codec::Call,
            _ => return None,
        })
    }

    pub fn words(self) -> String {
        let (quality, short) = match self {
            Codec::Sbc => (crate::lang::t("Standard quality"), "SBC"),
            Codec::SbcXq => (crate::lang::t("Good quality"), "SBC XQ"),
            Codec::Aac => (crate::lang::t("High quality"), "AAC"),
            Codec::Aptx => (crate::lang::t("High quality"), "aptX"),
            Codec::AptxHd => (crate::lang::t("Very high quality"), "aptX HD"),
            Codec::AptxLl => (crate::lang::t("Low delay"), "aptX LL"),
            Codec::Ldac => (crate::lang::t("High quality"), "LDAC"),
            Codec::Lc3 => (crate::lang::t("High quality"), "LC3"),
            Codec::Opus => (crate::lang::t("High quality"), "Opus"),
            Codec::Call => (crate::lang::t("Call quality, mic on"), "HFP"),
        };
        format!("{quality} ({short})")
    }

    pub fn rank(self) -> u8 {
        match self {
            Codec::Call => 0,
            Codec::Sbc => 1,
            Codec::AptxLl => 2,
            Codec::SbcXq => 3,
            Codec::Aac => 4,
            Codec::Aptx => 5,
            Codec::Opus => 6,
            Codec::Lc3 => 6,
            Codec::AptxHd => 7,
            Codec::Ldac => 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    Auto,
    Best,
    Steady,
}

impl Sound {
    pub const ALL: [Sound; 3] = [Sound::Auto, Sound::Best, Sound::Steady];

    pub fn label(self) -> &'static str {
        match self {
            Sound::Auto => crate::lang::t("Automatic"),
            Sound::Best => crate::lang::t("Best quality"),
            Sound::Steady => crate::lang::t("Steady connection"),
        }
    }

    pub fn short(self) -> &'static str {
        match self {
            Sound::Auto => crate::lang::t("Auto"),
            Sound::Best => crate::lang::t("Best"),
            Sound::Steady => crate::lang::t("Steady"),
        }
    }

    pub fn word(self) -> &'static str {
        match self {
            Sound::Auto => "auto",
            Sound::Best => "best",
            Sound::Steady => "steady",
        }
    }

    pub fn read(word: &str) -> Sound {
        Sound::ALL.into_iter().find(|each| each.word() == word).unwrap_or(Sound::Auto)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Card {
    pub name: String,
    pub active: String,
    pub profiles: Vec<(String, bool)>,
    pub codec: Option<Codec>,
}

impl Card {
    pub fn headset(&self) -> bool {
        self.active.starts_with("headset")
    }

    fn a2dp(&self) -> Vec<(&String, Option<Codec>)> {
        self.profiles
            .iter()
            .filter(|(name, ready)| *ready && plain_profile(name).starts_with("a2dp-sink"))
            .map(|(name, _)| (name, plain_profile(name).strip_prefix("a2dp-sink-").and_then(Codec::read)))
            .collect()
    }

    pub fn profile_for(&self, sound: Sound) -> Option<String> {
        let a2dp = self.a2dp();
        match sound {
            Sound::Auto => a2dp.iter().find(|(name, _)| plain_profile(name) == "a2dp-sink").map(|(name, _)| name.to_string()),
            Sound::Best => a2dp
                .iter()
                .filter_map(|(name, codec)| codec.map(|codec| (name, codec)))
                .filter(|(_, codec)| *codec != Codec::AptxLl)
                .max_by_key(|(_, codec)| codec.rank())
                .map(|(name, _)| name.to_string()),
            Sound::Steady => a2dp
                .iter()
                .find(|(_, codec)| *codec == Some(Codec::Sbc))
                .map(|(name, _)| name.to_string()),
        }
        .or_else(|| a2dp.first().map(|(name, _)| name.to_string()))
    }

    pub fn headset_profile(&self) -> Option<String> {
        self.profiles
            .iter()
            .filter(|(name, ready)| *ready && plain_profile(name).starts_with("headset-head-unit"))
            .map(|(name, _)| name.clone())
            .min_by_key(|name| name.len())
    }
}

fn plain_profile(name: &str) -> String {
    name.replace('_', "-")
}

pub fn card_name(address: &str) -> String {
    format!("bluez_card.{}", address.replace(':', "_"))
}

pub fn sink_prefixes(address: &str) -> [String; 2] {
    let tail = address.replace(':', "_");
    [format!("bluez_output.{tail}"), format!("bluez_sink.{tail}")]
}

pub fn source_prefixes(address: &str) -> [String; 2] {
    let tail = address.replace(':', "_");
    [format!("bluez_input.{tail}"), format!("bluez_source.{tail}")]
}

pub fn read_card(list: &str, address: &str) -> Option<Card> {
    let want = card_name(address);
    let mut card: Option<Card> = None;
    let mut in_profiles = false;
    for line in list.lines() {
        let trimmed = line.trim();
        if line.starts_with("Card #") {
            if card.as_ref().is_some_and(|card| card.name == want) {
                break;
            }
            card = None;
            in_profiles = false;
            continue;
        }
        if let Some(name) = trimmed.strip_prefix("Name:") {
            let name = name.trim();
            card = (name == want).then(|| Card {
                name: name.to_string(),
                ..Card::default()
            });
            continue;
        }
        let Some(card) = card.as_mut() else {
            continue;
        };
        if let Some(active) = trimmed.strip_prefix("Active Profile:") {
            card.active = active.trim().to_string();
            in_profiles = false;
            continue;
        }
        if trimmed == "Profiles:" {
            in_profiles = true;
            continue;
        }
        if trimmed.starts_with("Ports:") || trimmed.starts_with("Properties:") {
            in_profiles = false;
            continue;
        }
        if (trimmed.starts_with("api.bluez5.codec") || trimmed.starts_with("bluetooth.codec")) && card.codec.is_none() {
            card.codec = trimmed.split_once('=').and_then(|(_, value)| Codec::read(value));
            continue;
        }
        if in_profiles && let Some((name, rest)) = trimmed.split_once(": ") {
            let ready = !rest.contains("available: no");
            if name != "off" {
                card.profiles.push((name.to_string(), ready));
            }
        }
    }
    card.filter(|card| card.name == want)
}

pub fn sink_codec(sinks: &str, address: &str) -> Option<Codec> {
    let prefixes = sink_prefixes(address);
    let mut inside = false;
    for line in sinks.lines() {
        let trimmed = line.trim();
        if line.starts_with("Sink #") {
            inside = false;
            continue;
        }
        if let Some(name) = trimmed.strip_prefix("Name:") {
            inside = prefixes.iter().any(|prefix| name.trim().starts_with(prefix.as_str()));
            continue;
        }
        if inside && (trimmed.starts_with("api.bluez5.codec") || trimmed.starts_with("bluetooth.codec")) {
            return trimmed.split_once('=').and_then(|(_, value)| Codec::read(value));
        }
    }
    None
}

pub fn short_named(short: &str, prefixes: &[String]) -> Option<String> {
    short
        .lines()
        .filter_map(|line| line.split('\t').nth(1))
        .find(|name| prefixes.iter().any(|prefix| name.starts_with(prefix.as_str())))
        .map(str::to_string)
}

pub fn wpctl_node(status: &str, prefixes: &[String]) -> Option<u32> {
    status.lines().find_map(|line| {
        let rest = line.trim_start_matches(|c: char| !c.is_ascii_digit());
        let (id, name) = rest.split_once('.')?;
        let name = name.trim().split_whitespace().next()?;
        prefixes
            .iter()
            .any(|prefix| name.starts_with(prefix.as_str()))
            .then(|| id.trim().parse().ok())
            .flatten()
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    Never,
    Login,
    Sound,
    Mic,
}

impl When {
    pub const ALL: [When; 4] = [When::Never, When::Login, When::Sound, When::Mic];

    pub fn word(self) -> &'static str {
        match self {
            When::Never => "never",
            When::Login => "login",
            When::Sound => "sound",
            When::Mic => "mic",
        }
    }

    pub fn read(word: &str) -> When {
        When::ALL.into_iter().find(|each| each.word() == word).unwrap_or(When::Never)
    }

    pub fn short(self) -> &'static str {
        match self {
            When::Never => crate::lang::t("No"),
            When::Login => crate::lang::t("Log in"),
            When::Sound => crate::lang::t("Sound"),
            When::Mic => crate::lang::t("Calls"),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            When::Never => crate::lang::t("Only when I ask"),
            When::Login => crate::lang::t("When I log in"),
            When::Sound => crate::lang::t("When sound plays"),
            When::Mic => crate::lang::t("When I start a call"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Prefs {
    pub when: Vec<(String, When)>,
    pub sound: Vec<(String, Sound)>,
    pub keys: Vec<(String, String)>,
}

impl Prefs {
    pub fn path() -> std::path::PathBuf {
        crate::config::Config::state_dir().join("bluetooth.toml")
    }

    pub fn load() -> Prefs {
        std::fs::read_to_string(Prefs::path())
            .map(|text| Prefs::parse(&text))
            .unwrap_or_default()
    }

    pub fn parse(text: &str) -> Prefs {
        let Ok(doc) = text.parse::<toml::Table>() else {
            return Prefs::default();
        };
        let table = |name: &str| -> Vec<(String, String)> {
            doc.get(name)
                .and_then(|value| value.as_table())
                .map(|table| {
                    table
                        .iter()
                        .filter_map(|(key, value)| Some((key.to_uppercase(), value.as_str()?.to_string())))
                        .collect()
                })
                .unwrap_or_default()
        };
        Prefs {
            when: table("connect").into_iter().map(|(key, word)| (key, When::read(&word))).collect(),
            sound: table("sound").into_iter().map(|(key, word)| (key, Sound::read(&word))).collect(),
            keys: table("keys"),
        }
    }

    pub fn text(&self) -> String {
        let mut doc = toml::Table::new();
        let mut put = |name: &str, pairs: Vec<(String, String)>| {
            let table: toml::Table = pairs.into_iter().map(|(key, value)| (key, toml::Value::String(value))).collect();
            if !table.is_empty() {
                doc.insert(name.to_string(), toml::Value::Table(table));
            }
        };
        put(
            "connect",
            self.when
                .iter()
                .filter(|(_, when)| *when != When::Never)
                .map(|(key, when)| (key.clone(), when.word().to_string()))
                .collect(),
        );
        put(
            "sound",
            self.sound
                .iter()
                .filter(|(_, sound)| *sound != Sound::Auto)
                .map(|(key, sound)| (key.clone(), sound.word().to_string()))
                .collect(),
        );
        put("keys", self.keys.iter().filter(|(_, combo)| !combo.is_empty()).cloned().collect());
        toml::to_string(&doc).unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let path = Prefs::path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
        }
        std::fs::write(&path, self.text()).map_err(|error| error.to_string())
    }

    pub fn when(&self, address: &str) -> When {
        self.when
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(address))
            .map_or(When::Never, |(_, when)| *when)
    }

    pub fn sound(&self, address: &str) -> Sound {
        self.sound
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(address))
            .map_or(Sound::Auto, |(_, sound)| *sound)
    }

    pub fn key(&self, address: &str) -> Option<&str> {
        self.keys
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(address))
            .map(|(_, combo)| combo.as_str())
            .filter(|combo| !combo.is_empty())
    }

    pub fn set_when(&mut self, address: &str, when: When) {
        self.when.retain(|(key, _)| !key.eq_ignore_ascii_case(address));
        self.when.push((address.to_uppercase(), when));
    }

    pub fn set_sound(&mut self, address: &str, sound: Sound) {
        self.sound.retain(|(key, _)| !key.eq_ignore_ascii_case(address));
        self.sound.push((address.to_uppercase(), sound));
    }

    pub fn set_key(&mut self, address: &str, combo: &str) {
        self.keys.retain(|(key, _)| !key.eq_ignore_ascii_case(address));
        self.keys.push((address.to_uppercase(), combo.to_string()));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Wait,
    Doing,
    Popping,
    Done,
    Failed,
}

impl Step {
    pub fn word(self) -> &'static str {
        match self {
            Step::Wait => "wait",
            Step::Doing => "wait",
            Step::Popping => "blank",
            Step::Done => "done",
            Step::Failed => "failed",
        }
    }
}

pub fn step_icon(word: &str, side: u32, accent: [u8; 3], off: [u8; 3]) -> Image {
    use crate::geom::{Rect, Vec2};
    crate::paint::at_density(1, || {
        let size = f64::from(side.max(8));
        let middle = Vec2::splat(size / 2.0);
        let mut canvas = crate::paint::Canvas::new(side.max(8), side.max(8));
        let white = [255, 255, 255];
        match word {
            "wait" => canvas.ring(Rect::new(Vec2::splat(size * 0.12), Vec2::splat(size * 0.76)), size * 0.38, size * 0.07, off, 1.0),
            "doing" => {
                canvas.ring(Rect::new(Vec2::splat(size * 0.12), Vec2::splat(size * 0.76)), size * 0.38, size * 0.07, off, 1.0);
                canvas.arc(middle, size * 0.345, size * 0.08, 0.3, accent, 1.0);
            }
            "done" => {
                canvas.circle(middle, size * 0.42, accent, 1.0);
                canvas.line(Vec2::new(size * 0.30, size * 0.52), Vec2::new(size * 0.44, size * 0.65), size * 0.09, white, 1.0);
                canvas.line(Vec2::new(size * 0.44, size * 0.65), Vec2::new(size * 0.70, size * 0.37), size * 0.09, white, 1.0);
            }
            "failed" => {
                canvas.circle(middle, size * 0.42, [255, 69, 58], 1.0);
                canvas.round_rect(Rect::new(Vec2::new(size * 0.46, size * 0.26), Vec2::new(size * 0.08, size * 0.30)), size * 0.04, white, 1.0);
                canvas.circle(Vec2::new(size * 0.5, size * 0.68), size * 0.05, white, 1.0);
            }
            _ => {}
        }
        canvas.image
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Question {
    Confirm(u32),
    Type,
    TypePin,
    Show(String, u16),
    Allow,
    Mic,
    Incoming(Vec<(String, Option<u64>)>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub id: u64,
    pub address: String,
    pub name: String,
    pub kind: Kind,
    pub question: Question,
}

impl Ask {
    pub fn code(&self) -> Option<String> {
        match &self.question {
            Question::Confirm(key) => Some(format!("{key:06}")),
            Question::Show(code, _) => Some(code.clone()),
            _ => None,
        }
    }

    pub fn title(&self) -> String {
        match &self.question {
            Question::Mic => crate::lang::tf("Use the {} microphone?", &[&self.name]),
            Question::Incoming(_) => crate::lang::tf("{} wants to AirDrop", &[&self.name]),
            _ => crate::lang::tf("Set up {}?", &[&self.name]),
        }
    }

    pub fn words(&self) -> String {
        match &self.question {
            Question::Confirm(_) => crate::lang::tf("Check that {} shows this code.", &[&self.name]),
            Question::Type => crate::lang::tf("Type the code that {} shows.", &[&self.name]),
            Question::TypePin => crate::lang::tf("Type the PIN for {}. Many devices use 0000.", &[&self.name]),
            Question::Show(_, _) => crate::lang::tf("Type this code on {}, then press Enter.", &[&self.name]),
            Question::Allow => crate::lang::tf("{} wants to connect to this computer.", &[&self.name]),
            Question::Mic => crate::lang::t("An app wants a microphone. Mic on, sound quality will drop until the app stops.").to_string(),
            Question::Incoming(files) => incoming_words(files),
        }
    }

    pub fn wants_typing(&self) -> bool {
        matches!(self.question, Question::Type | Question::TypePin)
    }

    pub fn typed_digits(&self) -> usize {
        match self.question {
            Question::Show(_, entered) => usize::from(entered),
            _ => 0,
        }
    }
}

fn size_words(bytes: u64) -> String {
    match bytes {
        0..1_000 => crate::lang::tf("{} bytes", &[&bytes]),
        1_000..1_000_000 => crate::lang::tf("{} KB", &[&(bytes / 1_000)]),
        1_000_000..1_000_000_000 => format!("{:.1} MB", bytes as f64 / 1e6),
        _ => format!("{:.1} GB", bytes as f64 / 1e9),
    }
}

pub fn incoming_words(files: &[(String, Option<u64>)]) -> String {
    let total: u64 = files.iter().filter_map(|(_, size)| *size).sum();
    let what = match files {
        [] => crate::lang::t("Something").to_string(),
        [(name, _)] => name.clone(),
        [(name, _), rest @ ..] => crate::lang::tf("{} and {} more", &[name, &rest.len()]),
    };
    match total {
        0 => what,
        total => format!("{what}, {}", size_words(total)),
    }
}

pub fn power_supply_battery(address: &str, root: &std::path::Path) -> Option<u8> {
    let want = address.to_lowercase();
    let entries = std::fs::read_dir(root).ok()?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_lowercase();
        if !name.contains(&want) {
            continue;
        }
        let level = std::fs::read_to_string(entry.path().join("capacity"))
            .ok()
            .and_then(|text| text.trim().parse::<u8>().ok());
        if let Some(level) = level {
            return Some(level.min(100));
        }
    }
    None
}

pub fn xbox(device: &Device) -> bool {
    device.kind == Kind::Controller && device.name.to_lowercase().contains("xbox")
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PillLook {
    pub size: crate::geom::Vec2,
    pub badge: f64,
    pub back: [u8; 3],
    pub back_alpha: f64,
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub accent: [u8; 3],
}

impl PillLook {
    pub fn themed(p: &crate::theme::Palette, accent: [u8; 3]) -> PillLook {
        PillLook {
            size: crate::geom::Vec2::new(290.0, 64.0),
            badge: 64.0,
            back: p.surface,
            back_alpha: 0.97,
            ink: p.ink,
            dim: p.dim,
            accent,
        }
    }

    pub fn rect(&self, screen: crate::geom::Vec2, top: f64) -> crate::geom::Rect {
        crate::geom::Rect::new(
            crate::geom::Vec2::new(((screen.x - self.size.x) / 2.0).round(), top + 14.0),
            self.size,
        )
    }

    pub fn badge_rect(&self, screen: crate::geom::Vec2) -> crate::geom::Rect {
        crate::geom::Rect::new(
            crate::geom::Vec2::new(screen.x - self.badge - 24.0, 24.0),
            crate::geom::Vec2::splat(self.badge),
        )
    }

    pub fn paint(&self, name: &str, kind: Kind, battery: Option<u8>, words: &str, font: &crate::text::Font, small: &crate::text::Font) -> Image {
        let mark = glyph(kind, crate::paint::dense(26.0), [255, 255, 255]);
        self.paint_mark(name, &mark, battery, words, font, small)
    }

    pub fn paint_mark(&self, name: &str, mark: &Image, battery: Option<u8>, words: &str, font: &crate::text::Font, small: &crate::text::Font) -> Image {
        use crate::geom::{Rect, Vec2};
        let size = self.size;
        let mut canvas = crate::paint::Canvas::new(size.x as u32, size.y as u32);
        canvas.round_rect(Rect::new(Vec2::ZERO, size), size.y / 2.0, self.back, self.back_alpha);
        let disc = Vec2::new(size.y / 2.0, size.y / 2.0);
        canvas.circle(disc, 22.0, self.accent, 1.0);
        canvas.stamp(mark, disc - Vec2::splat(13.0), 1.0);
        let x = size.y + 2.0;
        let mut right = size.x - 22.0;
        if let Some(level) = battery {
            let cell = Rect::new(Vec2::new(right - 26.0, size.y / 2.0 - 7.0), Vec2::new(26.0, 14.0));
            crate::control::battery_glyph(&mut canvas, cell, level, self.dim, self.accent);
            right = cell.origin.x - 8.0;
        }
        let room = (right - x).max(1.0);
        let title = font.paint_weight(&crate::control::fit(font, name, room), self.ink, 600.0);
        let mut note = words.to_string();
        if let Some(level) = battery {
            note = format!("{note}, {level}%");
        }
        let note = small.paint(&crate::control::fit(small, &note, room), self.dim);
        canvas.stamp(&title, Vec2::new(x, size.y / 2.0 - crate::paint::logical(&title).y), 1.0);
        canvas.stamp(&note, Vec2::new(x, size.y / 2.0 + 1.0), 1.0);
        canvas.image
    }

    pub fn paint_badge(&self, kind: Kind, battery: Option<u8>) -> Image {
        use crate::geom::Vec2;
        let side = self.badge;
        let mut canvas = crate::paint::Canvas::new(side as u32, side as u32);
        let middle = Vec2::splat(side / 2.0);
        canvas.circle(middle, side / 2.0, self.back, self.back_alpha);
        canvas.arc(middle, side / 2.0 - 5.0, 3.5, 1.0, self.dim, 0.35);
        if let Some(level) = battery {
            let ink = match level {
                0..=10 => [255, 69, 58],
                11..=20 => [255, 159, 10],
                _ => [52, 199, 89],
            };
            canvas.arc(middle, side / 2.0 - 5.0, 3.5, f64::from(level.min(100)) / 100.0, ink, 1.0);
        }
        let glyph = glyph(kind, crate::paint::dense(28.0), self.ink);
        canvas.stamp(&glyph, middle - Vec2::splat(14.0), 1.0);
        canvas.image
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, paired: bool, connected: bool) -> Device {
        Device {
            path: format!("/org/bluez/hci0/dev_{name}"),
            address: format!("AA:BB:CC:DD:EE:{:02X}", name.len()),
            name: name.to_string(),
            named: !name.is_empty(),
            kind: Kind::Other,
            paired,
            connected,
            trusted: paired,
            battery: None,
            rssi: None,
            audio: false,
            sends: false,
            classic: true,
        }
    }

    #[test]
    fn a_kind_comes_from_the_icon_the_class_or_the_name() {
        assert_eq!(Kind::guess("input-gaming", None, None, "Xbox Wireless Controller"), Kind::Controller);
        assert_eq!(Kind::guess("audio-headset", None, None, "AirPods Pro"), Kind::Earbuds);
        assert_eq!(Kind::guess("audio-headset", None, None, "WH-1000XM5"), Kind::Headphones);
        assert_eq!(Kind::guess("", Some(1428), None, "Magic Trackpad"), Kind::Trackpad);
        assert_eq!(Kind::guess("input-mouse", None, Some(962), "MX Anywhere 3"), Kind::Mouse);
        assert_eq!(Kind::guess("", None, Some(961), "K380"), Kind::Keyboard);
        assert_eq!(Kind::guess("", Some(0x5a020c), None, "Pixel 8"), Kind::Phone);
        assert_eq!(Kind::guess("", Some(0x240418), None, "Bose QC"), Kind::Headphones);
        assert_eq!(Kind::guess("", None, None, "JBL Flip 6"), Kind::Speaker);
        assert_eq!(Kind::guess("", None, None, "Thing"), Kind::Other);
        assert_eq!(Kind::guess("audio-card", Some(0x240414), None, "[LG] webOS Monitor SQ700S"), Kind::Display);
    }

    #[test]
    fn a_device_never_shows_its_address_as_its_name() {
        assert!(!named("AA-BB-CC-DD-EE-FF", "AA:BB:CC:DD:EE:FF"));
        assert!(!named("  ", "AA:BB:CC:DD:EE:FF"));
        assert!(named("Buds", "AA:BB:CC:DD:EE:FF"));
        let mut buds = device("", false, false);
        buds.kind = Kind::Earbuds;
        assert_eq!(buds.shown_name(), "Unnamed earbuds");
    }

    #[test]
    fn the_words_are_plain() {
        let mut mouse = device("Mouse", true, true);
        assert_eq!(mouse.note(), "Connected");
        mouse.battery = Some(80);
        assert_eq!(mouse.note(), "Connected, 80%");
        mouse.connected = false;
        assert_eq!(mouse.note(), "Not connected");
        assert_eq!(device("Pad", false, false).state(), "Not set up");
        for word in ["Trusted", "Paired", "Bonded"] {
            assert!(!mouse.note().contains(word));
        }
    }

    #[test]
    fn connected_come_first_then_mine_then_the_strongest_nearby() {
        let mut far = device("Far", false, false);
        far.rssi = Some(-90);
        let mut near = device("Near", false, false);
        near.rssi = Some(-50);
        let mut blue = Blue {
            on: true,
            devices: vec![far, device("Mine", true, false), near, device("On", true, true)],
            ..Blue::default()
        };
        blue.sort();
        let names: Vec<&str> = blue.devices.iter().map(|device| device.name.as_str()).collect();
        assert_eq!(names, ["On", "Mine", "Near", "Far"]);
    }

    #[test]
    fn the_tile_says_what_is_connected() {
        let mut blue = Blue {
            on: true,
            devices: vec![device("Buds", true, true)],
            ..Blue::default()
        };
        assert_eq!(blue.note(), "Buds");
        blue.devices.push(device("Mouse", true, true));
        assert_eq!(blue.note(), "Buds and 1 more");
        blue.on = false;
        assert_eq!(blue.note(), "Off");
    }

    #[test]
    fn low_battery_warns_once_at_each_line() {
        assert_eq!(dropped_to(Some(21), Some(20)), Some(20));
        assert_eq!(dropped_to(Some(20), Some(19)), None);
        assert_eq!(dropped_to(Some(11), Some(10)), Some(10));
        assert_eq!(dropped_to(Some(30), Some(5)), Some(10));
        assert_eq!(dropped_to(None, Some(5)), None);
        let mut before = Blue::default();
        before.devices.push(Device {
            battery: Some(25),
            ..device("Pad", true, true)
        });
        let mut now = before.clone();
        now.devices[0].battery = Some(18);
        now.devices[0].connected = false;
        let address = now.devices[0].address.clone();
        assert_eq!(changes(&before, &now), [Change::Disconnected(address.clone()), Change::Low(address, 20)]);
    }

    #[test]
    fn signal_turns_into_bars_and_a_hint() {
        assert_eq!(bars(-40), 4);
        assert_eq!(bars(-70), 3);
        assert_eq!(bars(-85), 2);
        assert_eq!(bars(-95), 1);
        assert_eq!(closer(Some(-85)), Some("Move closer"));
        assert_eq!(closer(Some(-60)), None);
    }

    #[test]
    fn errors_are_plain_words() {
        let said = plain_error("org.bluez.Error.Failed: br-connection-profile-unavailable", "Buds", Kind::Earbuds);
        assert!(said.contains("Turn off Bluetooth on your phone"), "{said}");
        assert!(!said.contains("org.bluez"));
        let said = plain_error("org.bluez.Error.Failed: Page Timeout page-timeout", "Mouse", Kind::Mouse);
        assert!(said.starts_with("Mouse did not answer"), "{said}");
        assert_eq!(plain_error("org.bluez.Error.AuthenticationFailed", "Pad", Kind::Other), "The code did not match. Try again.");
        assert!(!plain_error("org.bluez.Error.Failed", "Pad", Kind::Other).contains("org.bluez"));
    }

    const CARDS: &str = "Card #52
\tName: bluez_card.AA_BB_CC_DD_EE_FF
\tDriver: module-bluez5-device.c
\tProperties:
\t\tdevice.description = \"Buds\"
\t\tapi.bluez5.codec = \"ldac\"
\tProfiles:
\t\toff: Off (sinks: 0, sources: 0, priority: 0, available: yes)
\t\ta2dp-sink-sbc: High Fidelity Playback (A2DP Sink, codec SBC) (sinks: 1, sources: 0, priority: 18, available: yes)
\t\ta2dp-sink-aac: High Fidelity Playback (A2DP Sink, codec AAC) (sinks: 1, sources: 0, priority: 19, available: yes)
\t\ta2dp-sink-ldac: High Fidelity Playback (A2DP Sink, codec LDAC) (sinks: 1, sources: 0, priority: 20, available: yes)
\t\ta2dp-sink: High Fidelity Playback (A2DP Sink) (sinks: 1, sources: 0, priority: 21, available: yes)
\t\theadset-head-unit-cvsd: Headset Head Unit (HSP/HFP, codec CVSD) (sinks: 1, sources: 1, priority: 1, available: yes)
\t\theadset-head-unit: Headset Head Unit (HSP/HFP) (sinks: 1, sources: 1, priority: 2, available: yes)
\tActive Profile: a2dp-sink
\tPorts:
\t\theadset-output: Headset (type: Headset, priority: 0, latency offset: 0 usec, availability unknown)
";

    #[test]
    fn a_pulseaudio_card_reads_like_a_pipewire_one() {
        let cards = "Card #4\n\tName: bluez_card.AA_BB_CC_DD_EE_FF\n\tProperties:\n\t\tbluetooth.codec = \"aac\"\n\tProfiles:\n\t\ta2dp_sink_sbc: High Fidelity Playback (A2DP Sink: SBC) (sinks: 1, sources: 0, priority: 40, available: yes)\n\t\ta2dp_sink_aac: High Fidelity Playback (A2DP Sink: AAC) (sinks: 1, sources: 0, priority: 40, available: yes)\n\t\theadset_head_unit: Headset Head Unit (HSP/HFP) (sinks: 1, sources: 1, priority: 30, available: yes)\n\t\toff: Off (sinks: 0, sources: 0, priority: 0, available: yes)\n\tActive Profile: a2dp_sink_aac\n";
        let card = read_card(cards, "AA:BB:CC:DD:EE:FF").expect("the card");
        assert_eq!(card.codec, Some(Codec::Aac));
        assert!(!card.headset());
        assert_eq!(card.profile_for(Sound::Best).as_deref(), Some("a2dp_sink_aac"));
        assert_eq!(card.profile_for(Sound::Steady).as_deref(), Some("a2dp_sink_sbc"));
        assert_eq!(card.headset_profile().as_deref(), Some("headset_head_unit"));
    }

    #[test]
    fn a_card_lists_its_profiles_and_codec() {
        let card = read_card(CARDS, "AA:BB:CC:DD:EE:FF").expect("the card");
        assert_eq!(card.active, "a2dp-sink");
        assert!(!card.headset());
        assert_eq!(card.codec, Some(Codec::Ldac));
        assert_eq!(card.profile_for(Sound::Best).as_deref(), Some("a2dp-sink-ldac"));
        assert_eq!(card.profile_for(Sound::Steady).as_deref(), Some("a2dp-sink-sbc"));
        assert_eq!(card.profile_for(Sound::Auto).as_deref(), Some("a2dp-sink"));
        assert_eq!(card.headset_profile().as_deref(), Some("headset-head-unit"));
        assert_eq!(read_card(CARDS, "11:22:33:44:55:66"), None);
        assert_eq!(Codec::Ldac.words(), "High quality (LDAC)");
    }

    #[test]
    fn the_sink_is_found_by_its_address() {
        let short = "61\talsa_output.pci.analog-stereo\tPipeWire\ts32le 2ch 48000Hz\tSUSPENDED\n77\tbluez_output.AA_BB_CC_DD_EE_FF.1\tPipeWire\ts16le 2ch 48000Hz\tRUNNING\n";
        assert_eq!(short_named(short, &sink_prefixes("AA:BB:CC:DD:EE:FF")).as_deref(), Some("bluez_output.AA_BB_CC_DD_EE_FF.1"));
        assert_eq!(short_named(short, &sink_prefixes("11:22:33:44:55:66")), None);
        let pulse = "3\tbluez_sink.AA_BB_CC_DD_EE_FF.a2dp_sink\tmodule-bluez5-device.c\ts16le 2ch 44100Hz\tRUNNING\n";
        assert_eq!(short_named(pulse, &sink_prefixes("AA:BB:CC:DD:EE:FF")).as_deref(), Some("bluez_sink.AA_BB_CC_DD_EE_FF.a2dp_sink"), "PulseAudio names work too");
        let status = " │  *   61. Built-in Audio Analog Stereo [vol: 0.40]\n │      77. bluez_output.AA_BB_CC_DD_EE_FF.1 [vol: 0.80]\n";
        assert_eq!(wpctl_node(status, &sink_prefixes("AA:BB:CC:DD:EE:FF")), Some(77));
        let sinks = "Sink #77\n\tName: bluez_output.AA_BB_CC_DD_EE_FF.1\n\tProperties:\n\t\tapi.bluez5.codec = \"aac\"\n";
        assert_eq!(sink_codec(sinks, "AA:BB:CC:DD:EE:FF"), Some(Codec::Aac));
    }

    #[test]
    fn prefs_round_trip_and_skip_defaults() {
        let mut prefs = Prefs::default();
        prefs.set_when("aa:bb:cc:dd:ee:ff", When::Sound);
        prefs.set_sound("AA:BB:CC:DD:EE:FF", Sound::Best);
        prefs.set_key("AA:BB:CC:DD:EE:FF", "Super+Shift+B");
        prefs.set_when("11:22:33:44:55:66", When::Never);
        let back = Prefs::parse(&prefs.text());
        assert_eq!(back.when("AA:BB:CC:DD:EE:FF"), When::Sound);
        assert_eq!(back.sound("aa:bb:cc:dd:ee:ff"), Sound::Best);
        assert_eq!(back.key("AA:BB:CC:DD:EE:FF"), Some("Super+Shift+B"));
        assert_eq!(back.when("11:22:33:44:55:66"), When::Never);
        assert!(!prefs.text().contains("never"));
    }

    #[test]
    fn a_battery_is_read_from_the_power_supply_folder() {
        let root = std::env::temp_dir().join(format!("drifter-blue-power-{}", std::process::id()));
        let folder = root.join("hid-d0:c0:50:b8:f5:c8-battery-144");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("capacity"), "67\n").unwrap();
        assert_eq!(power_supply_battery("D0:C0:50:B8:F5:C8", &root), Some(67));
        assert_eq!(power_supply_battery("11:22:33:44:55:66", &root), None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_pairing_question_reads_plainly() {
        let ask = Ask {
            id: 1,
            address: "AA:BB:CC:DD:EE:FF".into(),
            name: "Buds".into(),
            kind: Kind::Earbuds,
            question: Question::Confirm(4321),
        };
        assert_eq!(ask.code().as_deref(), Some("004321"));
        assert_eq!(ask.title(), "Set up Buds?");
        let show = Ask {
            question: Question::Show("123456".into(), 3),
            ..ask
        };
        assert_eq!(show.typed_digits(), 3);
    }

    #[test]
    fn every_kind_draws_a_glyph() {
        for kind in [
            Kind::Headphones, Kind::Earbuds, Kind::Speaker, Kind::Mouse, Kind::Keyboard, Kind::Trackpad, Kind::Controller,
            Kind::Phone, Kind::Tablet, Kind::Computer, Kind::Watch, Kind::Display, Kind::Printer, Kind::Other,
        ] {
            let image = glyph(kind, 32, [255, 255, 255]);
            assert_eq!((image.width, image.height), (32, 32));
            let ink = image.rgba.chunks(4).filter(|pixel| pixel[3] > 128).count();
            assert!(ink > 40, "{kind:?} has ink {ink}");
        }
    }

    #[test]
    fn the_pill_and_badge_paint_at_both_densities() {
        let look = PillLook::themed(&crate::theme::DARK, [10, 122, 255]);
        let font = crate::text::Font::find(None, 13.0).unwrap();
        crate::paint::same_at_two("pill", || look.paint("AirPods Pro", Kind::Earbuds, Some(80), "Connected", &font, &font));
        crate::paint::same_at_two("badge", || look.paint_badge(Kind::Controller, Some(15)));
        let screen = crate::geom::Vec2::new(1920.0, 1080.0);
        assert!(look.badge_rect(screen).max().x < screen.x);
        assert!((look.rect(screen, 30.0).center().x - 960.0).abs() < 1.0);
    }

    #[test]
    fn every_fix_step_has_a_mark() {
        for word in ["wait", "doing", "done", "failed"] {
            let image = step_icon(word, 26, [10, 122, 255], [70, 74, 90]);
            assert!(image.rgba.chunks(4).any(|pixel| pixel[3] > 200), "{word} draws something");
        }
        assert!(step_icon("blank", 26, [0, 0, 0], [0, 0, 0]).rgba.chunks(4).all(|pixel| pixel[3] == 0));
    }

    #[test]
    fn the_tray_menu_lists_mine_with_battery() {
        let mut mouse = device("Mouse", true, true);
        mouse.battery = Some(90);
        let blue = Blue {
            on: true,
            devices: vec![mouse, device("Pad", true, false), device("Other", false, false)],
            ..Blue::default()
        };
        let labels: Vec<String> = menu(&blue).into_iter().map(|item| item.label).collect();
        assert_eq!(labels, ["Mouse, 90%", "Pad, Connect", "", "Turn Bluetooth Off", "Bluetooth Settings…"]);
        let off = menu(&Blue::default());
        assert_eq!(off[0].label, "Turn Bluetooth On");
    }

    #[test]
    fn an_airdrop_offer_reads_plainly() {
        let ask = Ask {
            id: 1,
            address: String::new(),
            name: "Jane's iPhone".into(),
            kind: Kind::Phone,
            question: Question::Incoming(vec![("IMG_1.HEIC".into(), Some(2_400_000)), ("IMG_2.HEIC".into(), Some(600_000))]),
        };
        assert_eq!(ask.title(), "Jane's iPhone wants to AirDrop");
        assert_eq!(ask.words(), "IMG_1.HEIC and 1 more, 3.0 MB");
        assert_eq!(incoming_words(&[("a.txt".into(), Some(12))]), "a.txt, 12 bytes");
    }
}
