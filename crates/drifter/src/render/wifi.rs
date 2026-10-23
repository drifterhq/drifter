use std::collections::HashMap;

use super::*;
use super::ctllist::{LEVEL, Owner, POP, SHAKE};
use drifter_core::control::{Item, Tile, What};
use drifter_core::net::{self, Badge, Face, Glyph, Online, Shape};

const MORPH: f64 = 0.32;
const MORPH_FROM: f64 = 0.6;
const BADGE_POP: f64 = 0.42;
const BADGE_FROM: f64 = 0.55;
const BADGE_OUT: f64 = 0.2;
const FAILED_FOR: Duration = Duration::from_secs(4);
const SCAN_EVERY: Duration = Duration::from_secs(10);
const SCAN_SETTLE: Duration = Duration::from_secs(6);
const UNPLUGGED_FOR: Duration = Duration::from_secs(60);
const JOIN_WAIT: Duration = Duration::from_secs(150);
const STILL_ARC: f64 = 0.6;
const PILL_FOR: Duration = Duration::from_millis(2600);

#[derive(Default)]
pub(super) struct TrayTex {
    dot: Option<TextureBuffer<Tex>>,
    arcs: Vec<TextureBuffer<Tex>>,
    hollow: Option<TextureBuffer<Tex>>,
    wired: Option<TextureBuffer<Tex>>,
    alert: Option<TextureBuffer<Tex>>,
    portal: Option<TextureBuffer<Tex>>,
    lock: Option<TextureBuffer<Tex>>,
}

#[derive(Default)]
pub(super) struct Art {
    key: Option<(drifter_core::theme::Palette, [u8; 3], u32, [u8; 3])>,
    busy: bool,
    tray: Option<TrayTex>,
    pub discs: HashMap<(Glyph, bool), TextureBuffer<Tex>>,
}

pub(super) struct Wireless {
    pub bus: Option<drifter_net::Bus>,
    agent: Option<drifter_net::agent::Handle>,
    pub now: Option<net::Wifi>,
    pub missing: Option<net::Missing>,
    pub seen: bool,
    pub busy: Vec<String>,
    pub failed: Vec<(String, String, Instant)>,
    pub shakes: HashMap<String, Instant>,
    pub pops: HashMap<String, Instant>,
    picked: Vec<String>,
    pub art: Art,
    face: (Face, Face, Instant),
    arcs: [(f64, f64, Instant); 3],
    badge: (Badge, Badge, Instant),
    lock: (bool, Instant),
    wave_from: Instant,
    pub scanning: bool,
    scan_at: Option<Instant>,
    portal_told: Option<String>,
    unplugged: Option<Instant>,
    settings_told: Option<Instant>,
    airdrop: bool,
    menu_nets: Vec<String>,
    menu_vpns: Vec<String>,
}

impl Default for Wireless {
    fn default() -> Wireless {
        let long_ago = Instant::now() - Duration::from_secs(60);
        let blank = Face {
            shape: Shape::Off,
            arcs: 0,
            badge: Badge::None,
            lock: false,
        };
        Wireless {
            bus: None,
            agent: None,
            now: None,
            missing: None,
            seen: false,
            busy: Vec::new(),
            failed: Vec::new(),
            shakes: HashMap::new(),
            pops: HashMap::new(),
            picked: Vec::new(),
            art: Art::default(),
            face: (blank, blank, long_ago),
            arcs: [(net::BASE, net::BASE, long_ago); 3],
            badge: (Badge::None, Badge::None, long_ago),
            lock: (false, long_ago),
            wave_from: long_ago,
            scanning: false,
            scan_at: None,
            portal_told: None,
            unplugged: None,
            settings_told: None,
            airdrop: false,
            menu_nets: Vec::new(),
            menu_vpns: Vec::new(),
        }
    }
}

fn chore(chores: &smithay::reexports::calloop::channel::Sender<Chore>, work: impl FnOnce(&mut State) + Send + 'static) {
    let _ = chores.send(Box::new(work));
}

impl State {
    pub(super) fn start_wifi(&mut self) {
        let bus = match drifter_net::Bus::system() {
            Ok(bus) => bus,
            Err(error) => {
                log!("drifter: no network: {error}");
                return;
            }
        };
        if bus.running() {
            self.wifi_agent_up();
        }
        let chores = std::sync::Mutex::new(self.chores.clone());
        let watched = bus.watch(move |seen| {
            if let Ok(chores) = chores.lock() {
                chore(&chores, move |state| state.wifi_now(seen));
            }
        });
        if let Err(error) = watched {
            log!("drifter: cannot watch the network: {error}");
        }
        self.wifi.bus = Some(bus);
    }

    fn wifi_agent_up(&mut self) {
        let chores = std::sync::Mutex::new(self.chores.clone());
        match drifter_net::agent::serve(move |event| {
            if let Ok(chores) = chores.lock() {
                chore(&chores, move |state| state.wifi_agent(event));
            }
        }) {
            Ok(agent) => self.wifi.agent = Some(agent),
            Err(error) => log!("drifter: no Wi-Fi password helper: {error}"),
        }
    }

    pub(super) fn owns_network(&self) -> bool {
        self.wifi.agent.is_some()
    }

    pub(super) fn wifi_owns_icon(&self) -> bool {
        self.wifi.art.tray.is_some()
            && self.wifi.now.as_ref().is_some_and(|wifi| wifi.managed || wifi.wired.is_some() || !self.icons.iter().any(|icon| icon.3))
    }

    pub(super) fn wifi_now(&mut self, seen: net::Seen) {
        if seen.is_ok() && self.wifi.agent.is_none() && self.wifi.bus.is_some() {
            self.wifi_agent_up();
        }
        self.wifi.missing = seen.as_ref().err().copied();
        let mut now = seen.ok();
        let before = self.wifi.now.take();
        let first = !self.wifi.seen;
        self.wifi.seen = true;
        if let Some(now) = now.as_mut() {
            let had_wire = before.as_ref().and_then(|was| was.wired.as_ref()).is_some_and(|wired| wired.connected());
            let has_wire = now.wired.as_ref().is_some_and(|wired| wired.connected() || wired.carrier);
            if had_wire && !has_wire {
                self.wifi.unplugged = Some(Instant::now());
            }
            if has_wire {
                self.wifi.unplugged = None;
            }
            let joined_before = before.as_ref().and_then(|was| was.joined.clone());
            if let Some(joined) = now.joined.clone()
                && joined_before.as_deref() != Some(joined.as_str())
                && !first
            {
                self.wifi.failed.retain(|(ssid, _, _)| *ssid != joined);
                self.wifi.pops.insert(format!("net:{joined}"), Instant::now());
                if let Some(at) = self.wifi.picked.iter().position(|ssid| *ssid == joined) {
                    self.wifi.picked.remove(at);
                    self.wifi_show_pill(&joined);
                }
            }
            if now.online == Online::Portal
                && let Some(joined) = now.joined.clone()
                && self.wifi.portal_told.as_deref() != Some(joined.as_str())
            {
                self.wifi.portal_told = Some(joined.clone());
                self.wifi_portal_note(&joined);
            }
            if now.online != Online::Portal {
                self.wifi.portal_told = None;
            }
        }
        self.wifi.now = now;
        if let Some(wifi) = self.wifi.now.as_ref() {
            self.wifi.busy.retain(|ssid| ssid.starts_with("vpn:") || wifi.joined.as_deref() != Some(ssid.as_str()));
        }
        self.wifi_face();
        self.wifi_art();
        let was = self.control_status.wifi.clone();
        self.control_status.wifi = self.wifi.now.as_ref().filter(|wifi| wifi.card.is_some()).map(|wifi| {
            let name = match (wifi.joined.clone(), wifi.joining.clone()) {
                (Some(name), _) => name,
                (None, Some(name)) => drifter_core::lang::tf("Joining {}…", &[&name]),
                (None, None) => String::new(),
            };
            (wifi.on, name)
        });
        let changed = was != self.control_status.wifi;
        self.tray.net = self.net_slot().is_some() || self.wifi_owns_icon();
        self.wifi_scan_as_needed();
        if self.control.is_some() {
            self.fill_tile(Tile::Wifi);
            self.list_refresh();
            if changed {
                self.paint_control();
            }
        }
        self.kick_now();
    }

    fn wifi_face(&mut self) {
        let airdrop = self.airdrop_holds_wifi();
        let Some(wifi) = self.wifi.now.as_ref() else {
            return;
        };
        let face = net::face(wifi, airdrop);
        let now = Instant::now();
        let (held, _, _) = self.wifi.face;
        if held.shape != face.shape {
            self.wifi.face = (face, held, now);
            if face.shape == Shape::Busy {
                self.wifi.wave_from = now;
            }
        } else {
            self.wifi.face.0 = face;
        }
        let pace = self.window_pace.max(0.01);
        for (at, slot) in self.wifi.arcs.iter_mut().enumerate() {
            let want = match face.shape {
                Shape::Joined if (at as u8) < face.arcs => 1.0,
                _ => net::BASE,
            };
            if (slot.1 - want).abs() > 0.001 {
                let shown = super::ctllist::eased(*slot, pace, false);
                *slot = (shown, want, now);
            }
        }
        if self.wifi.badge.0 != face.badge {
            self.wifi.badge = (face.badge, self.wifi.badge.0, now);
        }
        if self.wifi.lock.0 != face.lock {
            self.wifi.lock = (face.lock, now);
        }
    }

    pub(super) fn wifi_art(&mut self) {
        let key = (self.palette, self.look.accent, self.density, self.tray.ink);
        if self.wifi.art.key == Some(key) || self.wifi.art.busy {
            return;
        }
        self.wifi.art.busy = true;
        let look = self.control_look();
        let ink = self.tray.ink;
        let accent = self.look.accent;
        self.on_worker(
            "wifi art",
            move || {
                let tray = net::tray_art(drifter_core::paint::dense(20.0), ink, accent);
                let discs: Vec<((Glyph, bool), drifter_core::Image)> = [Glyph::Wifi, Glyph::Wired, Glyph::Vpn]
                    .iter()
                    .flat_map(|glyph| [false, true].map(|lit| ((*glyph, lit), look.net_disc(*glyph, lit))))
                    .collect();
                (tray, discs)
            },
            move |state, (tray, discs)| {
                let renderer = &mut state.renderer;
                let mut up = |image: &drifter_core::Image| upload(renderer, Some(image), "wifi tray").ok().flatten();
                let baked = TrayTex {
                    dot: up(&tray.dot),
                    arcs: tray.arcs.iter().filter_map(&mut up).collect(),
                    hollow: up(&tray.hollow),
                    wired: up(&tray.wired),
                    alert: up(&tray.alert),
                    portal: up(&tray.portal),
                    lock: up(&tray.lock),
                };
                let mut art = Art {
                    key: Some(key),
                    busy: false,
                    tray: Some(baked),
                    discs: HashMap::new(),
                };
                for (key, image) in discs {
                    if let Ok(Some(tex)) = upload(&mut state.renderer, Some(&image), "wifi disc") {
                        art.discs.insert(key, tex);
                    }
                }
                state.wifi.art = art;
                state.tray.net = state.net_slot().is_some() || state.wifi_owns_icon();
                state.kick_now();
            },
        );
    }

    pub(super) fn wifi_tray_rect(&self) -> Option<Rect> {
        if !self.wifi_owns_icon() {
            return None;
        }
        Some(self.tray.net_rect(self.tray_band()?))
    }

    fn face_layers(&self, face: Face, rect: Rect, alpha: f64, elements: &mut Vec<Skin>) {
        let Some(tex) = self.wifi.art.tray.as_ref() else {
            return;
        };
        let pace = self.window_pace;
        let still = pace <= 0.0 || self.window_still;
        let push = |elements: &mut Vec<Skin>, art: &Option<TextureBuffer<Tex>>, place: Rect, shown: f64| {
            if let Some(art) = art.as_ref()
                && shown > 0.004
            {
                elements.push(Skin::Texture(element(art, place, shown as f32)));
            }
        };
        let corner = |x: f64, y: f64, scale: f64| {
            let pivot = rect.origin + Vec2::new(rect.size.x * x / 20.0, rect.size.y * y / 20.0);
            Rect::new(pivot + (rect.origin - pivot) * scale, rect.size * scale)
        };
        let (badge_now, badge_was, badge_since) = self.wifi.badge;
        let badge_t = match pace <= 0.0 {
            true => 1.0,
            false => badge_since.elapsed().as_secs_f64() / (BADGE_POP / pace),
        };
        let badge_art = |badge: Badge| match badge {
            Badge::None => &None,
            Badge::Alert => &tex.alert,
            Badge::Portal => &tex.portal,
        };
        if face.badge != Badge::None {
            let fresh = face.badge == badge_now && badge_t < 1.0;
            let scale = match (fresh, still) {
                (true, false) => BADGE_FROM + (1.0 - BADGE_FROM) * drifter_core::drawer::ease(badge_t),
                _ => 1.0,
            };
            let shown = if fresh { drifter_core::motion::smooth(badge_t) } else { 1.0 };
            push(elements, badge_art(face.badge), corner(18.4, 15.6, scale), alpha * shown);
        } else if badge_was != Badge::None && pace > 0.0 {
            let t = badge_since.elapsed().as_secs_f64() / (BADGE_OUT / pace);
            if t < 1.0 {
                push(elements, badge_art(badge_was), rect, alpha * (1.0 - drifter_core::motion::smooth(t)));
            }
        }
        let (lock_on, lock_since) = self.wifi.lock;
        if face.lock {
            let t = match pace <= 0.0 {
                true => 1.0,
                false => lock_since.elapsed().as_secs_f64() / (BADGE_POP / pace),
            };
            let fresh = lock_on && t < 1.0;
            let scale = match (fresh, still) {
                (true, false) => BADGE_FROM + (1.0 - BADGE_FROM) * drifter_core::drawer::ease(t),
                _ => 1.0,
            };
            let shown = if fresh { drifter_core::motion::smooth(t) } else { 1.0 };
            push(elements, &tex.lock, corner(3.1, 17.2, scale), alpha * shown);
        }
        match face.shape {
            Shape::Off => push(elements, &tex.hollow, rect, alpha * net::OFF),
            Shape::Wired => push(elements, &tex.wired, rect, alpha),
            Shape::Idle => {
                for arc in &tex.arcs {
                    elements.push(Skin::Texture(element(arc, rect, (alpha * net::BASE) as f32)));
                }
                push(elements, &tex.dot, rect, alpha * net::BASE);
            }
            Shape::Busy => {
                let seconds = self.wifi.wave_from.elapsed().as_secs_f64() * pace.max(0.01);
                for (at, arc) in tex.arcs.iter().enumerate() {
                    let level = match still {
                        true => STILL_ARC,
                        false => net::wave(seconds, at),
                    };
                    elements.push(Skin::Texture(element(arc, rect, (alpha * level) as f32)));
                }
                push(elements, &tex.dot, rect, alpha);
            }
            Shape::Joined => {
                for (at, arc) in tex.arcs.iter().enumerate() {
                    let target = match (at as u8) < face.arcs {
                        true => 1.0,
                        false => net::BASE,
                    };
                    let level = match face == self.wifi.face.0 {
                        true => super::ctllist::eased(self.wifi.arcs[at], pace, still),
                        false => target,
                    };
                    elements.push(Skin::Texture(element(arc, rect, (alpha * level) as f32)));
                }
                push(elements, &tex.dot, rect, alpha);
            }
        }
    }

    pub(super) fn wifi_tray_elements(&self, elements: &mut Vec<Skin>) {
        let Some(rect) = self.wifi_tray_rect() else {
            return;
        };
        let (now, was, since) = self.wifi.face;
        let pace = self.window_pace;
        let t = match pace <= 0.0 {
            true => 1.0,
            false => since.elapsed().as_secs_f64() / (MORPH / pace),
        };
        if t >= 1.0 {
            self.face_layers(now, rect, 1.0, elements);
            return;
        }
        let shown = drifter_core::motion::smooth(t);
        let scale = match self.window_still {
            true => 1.0,
            false => MORPH_FROM + (1.0 - MORPH_FROM) * drifter_core::drawer::ease(t.clamp(0.0, 1.0)),
        };
        self.face_layers(now, Rect::from_center(rect.center(), rect.size * scale), shown, elements);
        self.face_layers(was, rect, 1.0 - shown, elements);
    }

    pub(super) fn wifi_moving(&self) -> bool {
        let pace = self.window_pace;
        if pace <= 0.0 {
            return false;
        }
        let tray = self.wifi_tray_rect().is_some()
            && (self.wifi.face.2.elapsed().as_secs_f64() < MORPH / pace
                || self.wifi.arcs.iter().any(|arc| arc.2.elapsed().as_secs_f64() < LEVEL / pace)
                || self.wifi.badge.2.elapsed().as_secs_f64() < BADGE_POP.max(BADGE_OUT) / pace
                || self.wifi.lock.1.elapsed().as_secs_f64() < BADGE_POP / pace
                || (self.wifi.face.0.shape == Shape::Busy && !self.window_still));
        let popping = self.control.is_some() && (!self.wifi.pops.is_empty() || !self.wifi.shakes.is_empty());
        let shaking = self
            .prompts
            .first()
            .and_then(|pending| pending.shaken)
            .is_some_and(|at| at.elapsed().as_secs_f64() < SHAKE / pace);
        tray || popping || shaking
    }

    pub(super) fn wifi_tip(&self) -> Option<(String, Rect)> {
        let rect = self.wifi_tray_rect()?;
        if !rect.contains(self.pointer) {
            return None;
        }
        let wifi = self.wifi.now.as_ref()?;
        Some((net::tip(wifi, self.airdrop_holds_wifi()), rect))
    }

    pub(super) fn wifi_items(&self) -> Vec<Item> {
        let failed: Vec<(String, String)> = self
            .wifi
            .failed
            .iter()
            .filter(|(_, _, at)| at.elapsed() < FAILED_FOR)
            .map(|(ssid, words, _)| (ssid.clone(), words.clone()))
            .collect();
        let empty = net::Wifi::default();
        let wifi = match (self.wifi.now.as_ref(), self.wifi.missing) {
            (Some(wifi), _) if wifi.card.is_some() || wifi.wired.is_some() => Ok(wifi),
            (Some(_), _) => Err(net::Missing::Card),
            (None, missing) => match self.wifi.bus.is_some() {
                true => Err(missing.unwrap_or(net::Missing::Service)),
                false => Ok(&empty),
            },
        };
        drifter_core::control::wifi_items(drifter_core::control::Seen {
            wifi,
            airdrop: self.airdrop_holds_wifi(),
            busy: &self.wifi.busy,
            failed: &failed,
            scanning: self.wifi.scanning,
            unplugged: self.wifi.unplugged.is_some_and(|at| at.elapsed() < UNPLUGGED_FOR),
        })
    }

    pub(super) fn wifi_scan_as_needed(&mut self) {
        let looking = (self.control.is_some() && self.listing() == Some(Owner::Wifi))
            || self.popover.as_ref().is_some_and(|open| matches!(open.target, Target::Wifi));
        let on = self.wifi.now.as_ref().is_some_and(|wifi| wifi.on && wifi.managed) && !self.airdrop_holds_wifi();
        if !looking || !on {
            if self.wifi.scanning {
                self.wifi.scanning = false;
                self.list_refresh();
            }
            self.wifi.scan_at = None;
            return;
        }
        let due = self.wifi.scan_at.is_none_or(|at| at.elapsed() >= SCAN_EVERY);
        if !due {
            return;
        }
        self.wifi.scan_at = Some(Instant::now());
        self.wifi.scanning = true;
        let Some(bus) = self.wifi.bus.clone() else {
            return;
        };
        let since = self.wifi.now.as_ref().and_then(|wifi| wifi.card.as_ref()).map_or(0, |card| card.scanned);
        self.background("wifi scan", move || {
            let asked = bus.scan();
            if asked.is_ok() {
                bus.wait(|wifi| wifi.card.as_ref().is_some_and(|card| card.scanned > since), SCAN_SETTLE);
            }
            move |state: &mut State| {
                if let Err(error) = asked {
                    log!("drifter: Wi-Fi scan did not start: {error}");
                }
                state.wifi.scanning = false;
                state.list_refresh();
                state.kick_now();
            }
        });
        self.list_refresh();
    }

    pub(super) fn wifi_wind(&mut self) {
        let pace = self.window_pace.max(0.01);
        self.wifi.failed.retain(|(_, _, at)| at.elapsed() < FAILED_FOR);
        self.wifi.pops.retain(|_, at| at.elapsed().as_secs_f64() < POP / pace);
        self.wifi.shakes.retain(|_, at| at.elapsed().as_secs_f64() < SHAKE / pace);
        if self.wifi.unplugged.is_some_and(|at| at.elapsed() >= UNPLUGGED_FOR) {
            self.wifi.unplugged = None;
            self.list_refresh();
        }
        let airdrop = self.airdrop_holds_wifi();
        if airdrop != self.wifi.airdrop {
            self.wifi.airdrop = airdrop;
            self.wifi_face();
            self.list_refresh();
            self.kick_now();
        }
        self.wifi_scan_as_needed();
    }

    pub(super) fn wifi_power(&mut self, on: bool) {
        let Some(bus) = self.wifi.bus.clone() else {
            return;
        };
        self.background("wifi power", move || {
            let done = bus.power(on);
            move |state: &mut State| {
                if let Err(error) = done {
                    log!("drifter: cannot turn Wi-Fi {}: {error}", if on { "on" } else { "off" });
                    state.control_status.wifi = state.wifi.now.as_ref().map(|wifi| (wifi.on, wifi.joined.clone().unwrap_or_default()));
                    state.fill_tile(Tile::Wifi);
                    state.paint_control();
                    state.wifi_note(drifter_core::lang::t("Wi-Fi did not change"), &error, Glyph::Wifi);
                }
            }
        });
    }

    fn wifi_note(&mut self, summary: &str, body: &str, glyph: Glyph) -> Option<u32> {
        let id = self.notes_bus.as_ref().map(|bus| bus.next_id())?;
        let icon = Some(net::glyph(glyph, 64, self.palette.ink));
        self.show_note(drifter_core::notify::Note {
            id,
            app: drifter_core::lang::t("Wi-Fi").to_string(),
            summary: summary.to_string(),
            body: body.to_string(),
            actions: Vec::new(),
            expire: None,
            asked: 9000,
            critical: false,
            icon,
        });
        Some(id)
    }

    fn wifi_portal_note(&mut self, ssid: &str) {
        let summary = drifter_core::lang::tf("Sign in to {}", &[&ssid]);
        let body = drifter_core::lang::t("This network wants you to sign in before you use the internet. Click to open the sign-in page.");
        if let Some(id) = self.wifi_note(&summary, body, Glyph::Wifi) {
            self.note_links.insert(id, net::PORTAL_PAGE.to_string());
        }
    }

    pub(super) fn open_link(&self, url: &str) {
        let role = drifter_core::desktop::Role::Browser;
        crate::apps::open_as(url, role, self.apps_chosen.get(role), &self.socket);
    }

    pub(super) fn wifi_join(&mut self, ssid: &str) {
        let Some(bus) = self.wifi.bus.clone() else {
            return;
        };
        let Some(net) = self.wifi.now.as_ref().and_then(|wifi| wifi.find(ssid).cloned()) else {
            return;
        };
        if net.enterprise && !net.known {
            self.wifi_settings_join(ssid);
            return;
        }
        if self.wifi.busy.iter().any(|each| each == ssid) {
            return;
        }
        self.wifi.busy.push(ssid.to_string());
        self.wifi.failed.retain(|(each, _, _)| each != ssid);
        if !self.wifi.picked.iter().any(|each| each == ssid) {
            self.wifi.picked.push(ssid.to_string());
        }
        let ssid = ssid.to_string();
        self.background("wifi join", move || {
            let outcome = match bus.join(&ssid) {
                Ok(active) => Ok(bus.settle(&active, JOIN_WAIT)),
                Err(error) => Err(error),
            };
            move |state: &mut State| state.wifi_done(&ssid, outcome)
        });
        self.list_refresh();
        self.kick_now();
    }

    fn wifi_done(&mut self, ssid: &str, outcome: Result<drifter_net::bus::Joined, String>) {
        use drifter_net::bus::Joined;
        self.wifi.busy.retain(|each| each != ssid);
        let key = format!("net:{ssid}");
        match outcome {
            Ok(Joined::Done) => {
                self.wifi.pops.insert(key, Instant::now());
            }
            Ok(Joined::Canceled) => {
                self.wifi.picked.retain(|each| each != ssid);
            }
            Ok(outcome) => {
                self.wifi.picked.retain(|each| each != ssid);
                let (short, long) = match outcome {
                    Joined::Failed(reason) => (net::plain_error(reason, ssid), net::plain_failure(reason, ssid)),
                    _ => (
                        drifter_core::lang::t("Did not join").to_string(),
                        drifter_core::lang::tf("{} took too long to answer. Try again.", &[&ssid]),
                    ),
                };
                log!("drifter: Wi-Fi {ssid}: {long}");
                self.wifi.failed.push((ssid.to_string(), short, Instant::now()));
                self.wifi.shakes.insert(key, Instant::now());
                if self.listing() != Some(Owner::Wifi) {
                    self.wifi_note(&drifter_core::lang::tf("{} did not join", &[&ssid]), &long, Glyph::Wifi);
                }
            }
            Err(error) => {
                self.wifi.picked.retain(|each| each != ssid);
                log!("drifter: Wi-Fi {ssid}: {error}");
                self.wifi.failed.push((ssid.to_string(), drifter_core::lang::t("Did not join").to_string(), Instant::now()));
                self.wifi.shakes.insert(key, Instant::now());
                self.wifi_note(&drifter_core::lang::tf("{} did not join", &[&ssid]), &error, Glyph::Wifi);
            }
        }
        self.list_refresh();
        self.kick_now();
    }

    pub(super) fn wifi_leave(&mut self, ssid: &str) {
        let Some(bus) = self.wifi.bus.clone() else {
            return;
        };
        if self.wifi.busy.iter().any(|each| each == ssid) {
            return;
        }
        self.wifi.busy.push(ssid.to_string());
        let ssid = ssid.to_string();
        self.background("wifi leave", move || {
            let done = bus.leave(&ssid);
            move |state: &mut State| {
                state.wifi.busy.retain(|each| *each != ssid);
                if let Err(error) = done {
                    log!("drifter: cannot leave {ssid}: {error}");
                    state.wifi.shakes.insert(format!("net:{ssid}"), Instant::now());
                }
                state.list_refresh();
                state.kick_now();
            }
        });
        self.list_refresh();
    }

    fn wifi_vpn(&mut self, uuid: &str) {
        let Some(bus) = self.wifi.bus.clone() else {
            return;
        };
        let Some(vpn) = self.wifi.now.as_ref().and_then(|wifi| wifi.vpns.iter().find(|vpn| vpn.uuid == uuid).cloned()) else {
            return;
        };
        let key = format!("vpn:{uuid}");
        if self.wifi.busy.contains(&key) {
            return;
        }
        self.wifi.busy.push(key.clone());
        let on = !vpn.active;
        let uuid = uuid.to_string();
        self.background("vpn switch", move || {
            let done = bus.vpn(&uuid, on);
            let settled = match (&done, on) {
                (Ok(()), true) => bus.wait(|wifi| wifi.vpns.iter().any(|each| each.uuid == uuid && !each.busy), Duration::from_secs(30)),
                _ => true,
            };
            move |state: &mut State| {
                state.wifi.busy.retain(|each| *each != key);
                let ok = done.is_ok() && settled && state.wifi.now.as_ref().is_some_and(|wifi| wifi.vpns.iter().any(|each| each.uuid == uuid && each.active == on));
                match ok {
                    true => {
                        state.wifi.pops.insert(key, Instant::now());
                    }
                    false => {
                        state.wifi.shakes.insert(key, Instant::now());
                        let words = done.err().unwrap_or_else(|| drifter_core::lang::t("It did not connect. Check the VPN in Wi-Fi Settings.").to_string());
                        state.wifi_note(&drifter_core::lang::tf("{} did not change", &[&vpn.name]), &words, Glyph::Vpn);
                    }
                }
                state.list_refresh();
                state.kick_now();
            }
        });
        self.list_refresh();
    }

    fn wifi_settings_join(&mut self, ssid: &str) {
        if let Err(error) = drifter_core::settings::ask_wifi(Some(ssid)) {
            log!("drifter: cannot ask Settings to join {ssid}: {error}");
        }
        self.close_wifi_panels();
        self.open_settings_at("wifi");
    }

    fn wifi_settings_hidden(&mut self) {
        if let Err(error) = drifter_core::settings::ask_wifi(None) {
            log!("drifter: cannot ask Settings for a hidden network: {error}");
        }
        self.close_wifi_panels();
        self.open_settings_at("wifi");
    }

    fn close_wifi_panels(&mut self) {
        if self.control.is_some() {
            self.control = None;
            self.close_list();
        }
        self.popover = None;
        self.kick_now();
    }

    fn wifi_service(&mut self) {
        self.background("network service", || {
            let done = drifter_net::Bus::start_service();
            move |state: &mut State| {
                if let Err(error) = done {
                    state.wifi_note(drifter_core::lang::t("NetworkManager is not running"), &error, Glyph::Wifi);
                }
            }
        });
    }

    pub(super) fn press_wifi_item(&mut self, item: &Item) {
        match item.key.as_str() {
            drifter_core::control::WIFI_SETTINGS_KEY => {
                self.close_wifi_panels();
                self.open_settings_at("wifi");
            }
            drifter_core::control::OTHER_KEY => self.wifi_settings_hidden(),
            drifter_core::control::STOP_AIRDROP_KEY => {
                self.airdrop_receive(false);
                self.list_refresh();
            }
            drifter_core::control::NET_SERVICE_KEY => self.wifi_service(),
            drifter_core::control::WIRED_KEY => {
                self.wifi.pops.insert(item.key.clone(), Instant::now());
            }
            _ => {
                if let Some(uuid) = item.vpn() {
                    let uuid = uuid.to_string();
                    self.wifi_vpn(&uuid);
                    return;
                }
                let Some(ssid) = item.ssid().map(str::to_string) else {
                    return;
                };
                match matches!(item.what, What::Network { joined: true, .. }) {
                    true => self.wifi_leave(&ssid),
                    false => self.wifi_join(&ssid),
                }
            }
        }
    }

    pub(super) fn open_wifi_menu(&mut self, details: bool) {
        let at = self.wifi_tray_rect().map_or(self.pointer, |rect| Vec2::new(rect.origin.x, rect.max().y + 6.0));
        let airdrop = self.airdrop_holds_wifi();
        if !details {
            let items = match self.wifi.now.as_ref() {
                Some(wifi) => net::menu(wifi, airdrop),
                None => vec![drifter_core::menu::Item::line(net::MENU_SETTINGS, drifter_core::lang::t("Wi-Fi Settings…"))],
            };
            self.wifi.menu_nets = self.wifi.now.as_ref().map(|wifi| wifi.nets.iter().map(|net| net.ssid.clone()).collect()).unwrap_or_default();
            self.wifi.menu_vpns = self.wifi.now.as_ref().map(|wifi| wifi.vpns.iter().map(|vpn| vpn.uuid.clone()).collect()).unwrap_or_default();
            self.close_panels();
            self.open_popover(items, at, Target::Wifi);
            self.wifi_scan_as_needed();
            return;
        }
        let Some(bus) = self.wifi.bus.clone() else {
            return;
        };
        let Some(wifi) = self.wifi.now.clone() else {
            return;
        };
        self.background("wifi details", move || {
            let lines = match wifi.joined_net().cloned() {
                Some(joined) => {
                    let ap = bus.points(&joined.ssid).into_iter().next();
                    let link = bus.link();
                    net::quick_lines(&wifi, &joined, ap.as_ref(), link.as_ref())
                }
                None => vec![(drifter_core::lang::t("Wi-Fi").to_string(), net::tip(&wifi, false))],
            };
            move |state: &mut State| {
                state.close_panels();
                state.open_popover(net::detail_menu(&lines), at, Target::Wifi);
            }
        });
    }

    pub(super) fn wifi_menu_pick(&mut self, id: i32) {
        match id {
            net::MENU_POWER => {
                let on = !self.wifi.now.as_ref().is_some_and(|wifi| wifi.on);
                self.wifi_power(on);
            }
            net::MENU_SETTINGS => self.open_settings_at("wifi"),
            net::MENU_OTHER => self.wifi_settings_hidden(),
            net::MENU_STOP_AIRDROP => self.airdrop_receive(false),
            at if at >= net::MENU_VPN => {
                let uuid = self.wifi.menu_vpns.get((at - net::MENU_VPN) as usize).cloned();
                if let Some(uuid) = uuid {
                    self.wifi_vpn(&uuid);
                }
            }
            at if at >= 0 => {
                let found = self
                    .wifi
                    .menu_nets
                    .get(at as usize)
                    .and_then(|ssid| self.wifi.now.as_ref().and_then(|wifi| wifi.find(ssid)))
                    .map(|net| (net.ssid.clone(), net.active));
                match found {
                    Some((ssid, true)) => self.wifi_leave(&ssid),
                    Some((ssid, false)) => self.wifi_join(&ssid),
                    None => {}
                }
            }
            _ => {}
        }
        self.kick_now();
    }

    pub(super) fn alt_held(&self) -> bool {
        self.wayland.keyboard.modifier_state().alt
    }

    pub(super) fn press_wifi_tray(&mut self, had_menu: bool, details: bool) -> bool {
        if !self.wifi_tray_rect().is_some_and(|rect| rect.contains(self.pointer)) {
            return false;
        }
        if !had_menu {
            self.open_wifi_menu(details);
        }
        true
    }

    fn wifi_show_pill(&mut self, ssid: &str) {
        let (Some(font), Some(small)) = (self.note_body.clone(), self.card_font.clone()) else {
            return;
        };
        if self.listing() == Some(Owner::Wifi) && self.control.is_some() {
            return;
        }
        let look = drifter_core::blue::PillLook::themed(&self.palette, self.look.accent);
        let top = self.tray_band().map_or(0.0, |band| band.max().y);
        let rect = look.rect(self.screen, top);
        let name = ssid.to_string();
        let words = match self.wifi.now.as_ref().map(|wifi| wifi.online) {
            Some(Online::Portal) => drifter_core::lang::t("Joined, sign in to use the internet").to_string(),
            Some(Online::None | Online::Limited) => drifter_core::lang::t("Joined, no internet yet").to_string(),
            _ => drifter_core::lang::t("Joined").to_string(),
        };
        self.show_pill(rect, PILL_FOR, move || {
            let mark = net::glyph(Glyph::Wifi, drifter_core::paint::dense(26.0), [255, 255, 255]);
            Some(look.paint_mark(&name, &mark, None, &words, &font, &small))
        });
    }

    pub(super) fn wifi_agent(&mut self, event: drifter_net::agent::Event) {
        use drifter_net::agent::Event;
        match event {
            Event::Ask(ask, reply) => {
                if ask.again {
                    self.wifi.shakes.insert(format!("net:{}", ask.ssid), Instant::now());
                }
                self.wifi_prompt(ask, reply);
            }
            Event::Done(id) => self.wifi_prompt_done(id),
            Event::Settings(ssid) => {
                let recent = self.wifi.settings_told.is_some_and(|at| at.elapsed() < Duration::from_secs(5));
                if !recent {
                    self.wifi.settings_told = Some(Instant::now());
                    self.wifi_settings_join(&ssid);
                }
            }
            Event::Lost => {
                log!("drifter: the Wi-Fi password helper lost its bus, starting it again");
                self.wifi.agent = None;
                if self.wifi.bus.as_ref().is_some_and(drifter_net::Bus::running) {
                    self.wifi_agent_up();
                }
            }
            Event::Vpn(name) => {
                let body = drifter_core::lang::t("Drifter cannot ask for this VPN's password yet. Set the password in the VPN's own settings, then try again.");
                self.wifi_note(&drifter_core::lang::tf("{} needs a password", &[&name]), body, Glyph::Vpn);
            }
        }
        self.kick_now();
    }
}
