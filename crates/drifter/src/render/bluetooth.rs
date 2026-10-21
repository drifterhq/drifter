use std::collections::HashMap;

use super::*;
use drifter_blue::agent::Reply;
use drifter_core::blue::{self, Ask, Blue, Change, Kind, Prefs, When};
use drifter_core::control::{Item, Tile, What};

const LIST_OPEN: f64 = 0.38;
const LIST_CLOSE: f64 = 0.2;
const FILL_IN: f64 = 0.34;
const FILL_OUT: f64 = 0.18;
const ROW_IN: f64 = 0.36;
const ROW_STAGGER: f64 = 0.03;
const ROW_RISE: f64 = 10.0;
const ROW_MOVE: f64 = 0.44;
const ROW_OUT: f64 = 0.22;
const SHAKE: f64 = 0.5;
const SHAKE_REACH: f64 = 9.0;
const POP: f64 = 0.42;
const POP_FROM: f64 = 0.55;
const TURN: f64 = 0.9;
const BREATH: f64 = 1.6;
const RING_FRAMES: usize = 24;
const FAILED_FOR: Duration = Duration::from_secs(4);
const PILL_FOR: Duration = Duration::from_millis(2600);
const BADGE_FOR: Duration = Duration::from_secs(2);
const PILL_IN: f64 = 0.4;
const PILL_OUT: f64 = 0.2;
const PILL_FROM: f64 = 0.86;
const PAIR_IN: f64 = 0.34;
const PAIR_FROM: f64 = 0.96;
const DIGIT_IN: f64 = 0.34;
const DIGIT_STAGGER: f64 = 0.055;
const DIGIT_FROM: f64 = 0.5;
const MORPH: f64 = 0.32;
const MORPH_FROM: f64 = 0.6;
const LEVEL: f64 = 0.36;
const HUD_ICON_IN: f64 = 0.4;
const HUD_ICON_FROM: f64 = 0.4;

pub(super) struct Row {
    pub key: String,
    pub item: Item,
    pub art: Option<TextureBuffer<Tex>>,
    pub painted: Option<Item>,
    pub from: f64,
    pub to: f64,
    pub moved: Instant,
    pub bounce: bool,
    pub born: Instant,
    pub gone: Option<Instant>,
    pub level: (f64, f64, Instant),
    pub signal: (f64, f64, Instant),
}

pub(super) struct List {
    pub items: Vec<Item>,
    pub rows: Vec<Row>,
    pub lane: worker::Lane,
    pub closing: bool,
    pub epoch: Instant,
}

pub(super) struct Pill {
    pub art: Option<TextureBuffer<Tex>>,
    pub rect: Rect,
    pub born: Instant,
    pub until: Instant,
    pub gone: Option<Instant>,
}

#[derive(Default)]
pub(super) struct Art {
    key: Option<(drifter_core::theme::Palette, [u8; 3], u32)>,
    busy: bool,
    discs: HashMap<(Kind, bool), TextureBuffer<Tex>>,
    ring: Vec<TextureBuffer<Tex>>,
    pub symbols: HashMap<u8, TextureBuffer<Tex>>,
    pub accent_disc: Option<TextureBuffer<Tex>>,
    tray: Vec<TextureBuffer<Tex>>,
    dot: Option<TextureBuffer<Tex>>,
    fills: Vec<TextureBuffer<Tex>>,
    bars: Option<TextureBuffer<Tex>>,
    pub glyphs: HashMap<Kind, TextureBuffer<Tex>>,
}

pub(super) struct Radio {
    pub bus: Option<drifter_blue::Bus>,
    agent: Option<drifter_blue::agent::Handle>,
    agent_tried: Option<Instant>,
    pub now: Option<Blue>,
    pub missing: Option<blue::Missing>,
    pub seen: bool,
    pub prefs: Prefs,
    pub busy: Vec<String>,
    pub failed: Vec<(String, Instant)>,
    pub pops: HashMap<String, Instant>,
    pub shakes: HashMap<String, Instant>,
    pub asks: Vec<(Ask, Option<Reply>)>,
    pub typed: String,
    pub typed_at: Vec<Instant>,
    pub asked_at: Instant,
    pub pair_art: Option<(TextureBuffer<Tex>, Rect)>,
    pub pair_haze: Option<TextureBuffer<Tex>>,
    pub pair_lane: worker::Lane,
    pub digits: HashMap<(char, bool), TextureBuffer<Tex>>,
    pub searching: bool,
    pub pill: Option<Pill>,
    pub badge: bool,
    pub art: Art,
    pub tray_state: (usize, usize, Instant),
    pub list: Option<List>,
    pub prefs_seen: Option<std::time::SystemTime>,
    pub mic_asked: Vec<String>,
    pub mic_wanted: Vec<String>,
    pub next_ask: u64,
}

impl Default for Radio {
    fn default() -> Radio {
        let long_ago = Instant::now() - Duration::from_secs(60);
        Radio {
            bus: None,
            agent: None,
            agent_tried: None,
            now: None,
            missing: None,
            seen: false,
            prefs: Prefs::default(),
            busy: Vec::new(),
            failed: Vec::new(),
            pops: HashMap::new(),
            shakes: HashMap::new(),
            asks: Vec::new(),
            typed: String::new(),
            typed_at: Vec::new(),
            asked_at: long_ago,
            pair_art: None,
            pair_haze: None,
            pair_lane: worker::Lane::default(),
            digits: HashMap::new(),
            searching: false,
            pill: None,
            badge: false,
            art: Art::default(),
            tray_state: (0, 0, long_ago),
            list: None,
            prefs_seen: None,
            mic_asked: Vec::new(),
            mic_wanted: Vec::new(),
            next_ask: 1 << 40,
        }
    }
}

fn tile_index(tile: Tile) -> u8 {
    Tile::ALL.iter().position(|each| *each == tile).unwrap_or(0) as u8
}

fn chore(chores: &smithay::reexports::calloop::channel::Sender<Chore>, work: impl FnOnce(&mut State) + Send + 'static) {
    let _ = chores.send(Box::new(work));
}

impl State {
    pub(super) fn start_bluetooth(&mut self) {
        self.blue_prefs_fresh();
        let bus = match drifter_blue::Bus::system() {
            Ok(bus) => bus,
            Err(error) => {
                log!("drifter: no Bluetooth: {error}");
                return;
            }
        };
        let chores = std::sync::Mutex::new(self.chores.clone());
        let watched = bus.watch(move |now| {
            if let Ok(chores) = chores.lock() {
                chore(&chores, move |state| state.blue_now(now));
            }
        });
        if let Err(error) = watched {
            log!("drifter: cannot watch Bluetooth: {error}");
        }
        let chores = std::sync::Mutex::new(self.chores.clone());
        drifter_blue::sound::watch_streams(move |stream| {
            if let Ok(chores) = chores.lock() {
                chore(&chores, move |state| state.blue_stream(stream));
            }
        });
        self.radio.bus = Some(bus);
    }

    fn blue_agent_up(&mut self) {
        let chores = std::sync::Mutex::new(self.chores.clone());
        self.radio.agent = None;
        self.background("bluetooth agent", move || {
            let served = drifter_blue::agent::serve(move |event| {
                if let Ok(chores) = chores.lock() {
                    chore(&chores, move |state| state.blue_agent(event));
                }
            });
            move |state: &mut State| match served {
                Ok(agent) => state.radio.agent = Some(agent),
                Err(error) => log!("drifter: no Bluetooth pairing helper: {error}"),
            }
        });
    }

    pub(super) fn blue_now(&mut self, seen: blue::Seen) {
        let lost = self.radio.now.is_none() && seen.is_ok();
        let missing = self.radio.agent.is_none() && self.radio.agent_tried.is_none_or(|at| at.elapsed() > Duration::from_secs(10));
        if seen.is_ok() && (lost || missing) && self.radio.bus.is_some() {
            self.radio.agent_tried = Some(Instant::now());
            self.blue_agent_up();
        }
        self.radio.missing = seen.as_ref().err().copied();
        let now = seen.ok();
        let before = self.radio.now.take().unwrap_or_default();
        let first = !self.radio.seen;
        self.radio.seen = true;
        if let Some(now) = now.as_ref() {
            for change in blue::changes(&before, now) {
                match change {
                    Change::Connected(address) if !first => self.blue_connected(now, &address),
                    Change::Disconnected(address) => {
                        self.radio.mic_asked.retain(|each| *each != address);
                        self.radio.mic_wanted.retain(|each| *each != address);
                    }
                    Change::Low(address, line) => self.blue_low(now, &address, line),
                    _ => {}
                }
            }
            self.radio
                .asks
                .retain(|(ask, reply)| !(reply.is_none() && now.find(&ask.address).is_some_and(|device| device.paired)));
        }
        self.radio.now = now;
        if first {
            self.blue_login();
        }
        let was = (self.control_status.bluetooth, self.control_status.blue_note.clone());
        self.control_status.bluetooth = self.radio.now.as_ref().map(|blue| blue.on);
        self.control_status.blue_note = match (self.radio.now.as_ref(), self.radio.missing) {
            (Some(blue), _) => blue.note(),
            (None, Some(blue::Missing::Service)) => drifter_core::lang::t("Service off").to_string(),
            (None, _) => String::new(),
        };
        let changed = was != (self.control_status.bluetooth, self.control_status.blue_note.clone());
        self.blue_tray_state();
        self.blue_search_as_needed();
        if self.control.is_some() {
            self.blue_fill(Tile::Bluetooth);
            self.blue_list_refresh();
            if changed {
                self.paint_control();
            }
        }
        self.paint_pairing();
        self.kick_now();
    }

    fn blue_prefs_fresh(&mut self) {
        let seen = std::fs::metadata(Prefs::path()).and_then(|meta| meta.modified()).ok();
        if seen != self.radio.prefs_seen {
            self.radio.prefs_seen = seen;
            self.radio.prefs = Prefs::load();
        }
    }

    pub(super) fn blue_key(&mut self, held: drifter_core::shortcut::Mods, names: &[String]) -> Option<String> {
        self.radio.bus.as_ref()?;
        self.blue_prefs_fresh();
        self.radio
            .prefs
            .keys
            .iter()
            .find(|(_, text)| {
                drifter_core::shortcut::Combo::parse(text)
                    .ok()
                    .flatten()
                    .is_some_and(|combo| combo.matches(held, names, false))
            })
            .map(|(address, _)| address.clone())
    }

    fn blue_login(&mut self) {
        let Some(blue) = self.radio.now.clone() else {
            return;
        };
        let untrusted: Vec<String> = blue
            .mine()
            .filter(|device| !device.trusted && device.kind == Kind::Controller)
            .map(|device| device.address.clone())
            .collect();
        if let (false, Some(bus)) = (untrusted.is_empty(), self.radio.bus.clone()) {
            self.background("bluetooth trust", move || {
                for address in &untrusted {
                    let _ = bus.trust(address, true);
                }
                |_: &mut State| {}
            });
        }
        for device in blue.mine().filter(|device| !device.connected) {
            if self.radio.prefs.when(&device.address) == When::Login {
                self.blue_connect(&device.address);
            }
        }
    }

    fn blue_stream(&mut self, stream: drifter_blue::sound::Stream) {
        self.blue_prefs_fresh();
        let Some(blue) = self.radio.now.clone() else {
            return;
        };
        if !blue.on {
            return;
        }
        let want = match stream {
            drifter_blue::sound::Stream::Sound => When::Sound,
            drifter_blue::sound::Stream::Mic => When::Mic,
        };
        for device in blue.mine().filter(|device| !device.connected) {
            if self.radio.prefs.when(&device.address) == want && !self.radio.busy.contains(&device.address) {
                if want == When::Mic {
                    self.radio.mic_wanted.push(device.address.clone());
                }
                self.blue_connect(&device.address);
            }
        }
        if want != When::Mic {
            return;
        }
        let audio: Vec<blue::Device> = blue
            .connected()
            .filter(|device| device.audio || device.kind.audio())
            .filter(|device| !self.radio.mic_asked.contains(&device.address))
            .cloned()
            .collect();
        for device in audio {
            self.radio.mic_asked.push(device.address.clone());
            let auto = self.radio.prefs.when(&device.address) == When::Mic;
            let sound = self.radio.prefs.sound(&device.address);
            let address = device.address.clone();
            self.background("bluetooth mic check", move || {
                let card = drifter_blue::sound::card(&address);
                let offers = card.as_ref().is_some_and(|card| !card.headset() && card.headset_profile().is_some());
                let switched = (offers && auto).then(|| drifter_blue::sound::mic(&address, true, sound));
                move |state: &mut State| {
                    if let Some(Err(error)) = switched {
                        log!("drifter: cannot turn on the mic of {address}: {error}");
                    }
                    if offers && !auto {
                        state.blue_ask_mic(&device);
                    }
                }
            });
        }
    }

    fn blue_ask_mic(&mut self, device: &blue::Device) {
        let id = self.radio.next_ask;
        self.radio.next_ask += 1;
        let ask = Ask {
            id,
            address: device.address.clone(),
            name: device.shown_name(),
            kind: device.kind,
            question: blue::Question::Mic,
        };
        self.blue_queue_ask(ask);
    }

    pub(super) fn blue_queue_ask(&mut self, ask: Ask) {
        if self.radio.asks.is_empty() {
            self.radio.asked_at = Instant::now();
            self.radio.typed.clear();
            self.radio.typed_at.clear();
        }
        self.radio.asks.push((ask, None));
        self.paint_pairing();
        self.kick_now();
    }

    fn blue_connected(&mut self, blue: &Blue, address: &str) {
        self.blue_prefs_fresh();
        self.radio.pops.insert(address.to_string(), Instant::now());
        self.radio.failed.retain(|(each, _)| each != address);
        let Some(device) = blue.find(address).cloned() else {
            return;
        };
        if device.kind.audio() || device.audio {
            let sound = self.radio.prefs.sound(address);
            let mic = self.radio.mic_wanted.iter().any(|each| each == address);
            let address = address.to_string();
            self.background("bluetooth sound", move || {
                let switched = drifter_blue::sound::make_default(&address, sound);
                if mic && matches!(switched, Ok(true)) {
                    let _ = drifter_blue::sound::mic(&address, true, sound);
                }
                move |state: &mut State| match switched {
                    Ok(true) => state.blue_output_hud(&address),
                    Ok(false) => {}
                    Err(error) => log!("drifter: cannot switch sound to {address}: {error}"),
                }
            });
        }
        let list_open = self.radio.list.as_ref().is_some_and(|list| !list.closing) && self.control.is_some();
        if !list_open {
            self.blue_show_pill(&device);
        }
    }

    fn blue_low(&mut self, blue: &Blue, address: &str, line: u8) {
        let Some(device) = blue.find(address) else {
            return;
        };
        let Some(id) = self.notes_bus.as_ref().map(|bus| bus.next_id()) else {
            return;
        };
        let name = device.shown_name();
        let gaming = self.held_by_fullscreen().is_some();
        if gaming && device.kind == Kind::Controller {
            self.blue_show_pill(device);
        }
        let icon = Some(drifter_core::blue::glyph(device.kind, 64, self.palette.ink));
        self.show_note(drifter_core::notify::Note {
            id,
            app: drifter_core::lang::t("Bluetooth").to_string(),
            summary: drifter_core::lang::tf("{} battery is low", &[&name]),
            body: drifter_core::lang::tf("{}% left. Charge it soon.", &[&line]),
            actions: Vec::new(),
            expire: None,
            asked: 8000,
            critical: line <= 10,
            icon,
        });
    }

    pub(super) fn blue_connect(&mut self, address: &str) {
        let Some(bus) = self.radio.bus.clone() else {
            return;
        };
        let Some(device) = self.radio.now.as_ref().and_then(|blue| blue.find(address).cloned()) else {
            return;
        };
        if self.radio.busy.iter().any(|each| each == address) {
            return;
        }
        self.radio.busy.push(address.to_string());
        self.radio.failed.retain(|(each, _)| each != address);
        let address = address.to_string();
        let (paired, connected) = (device.paired, device.connected);
        let xbox = blue::xbox(&device);
        self.background("bluetooth connect", move || {
            let done = match (paired, connected) {
                (_, true) => bus.disconnect(&address),
                (true, false) => bus.connect(&address),
                (false, _) => bus.pair(&address),
            };
            move |state: &mut State| state.blue_done(&address, done.err(), xbox)
        });
        self.blue_list_refresh();
        self.kick_now();
    }

    fn blue_done(&mut self, address: &str, error: Option<String>, xbox: bool) {
        self.radio.busy.retain(|each| each != address);
        if let Some(error) = error {
            let device = self.radio.now.as_ref().and_then(|blue| blue.find(address).cloned());
            let (name, kind) = device
                .as_ref()
                .map_or((drifter_core::lang::t("The device").to_string(), Kind::Other), |device| (device.shown_name(), device.kind));
            log!("drifter: Bluetooth {address}: {error}");
            let quiet = error.contains("AuthenticationCanceled") || error.contains("Rejected");
            if !quiet {
                self.radio.failed.push((address.to_string(), Instant::now()));
                self.radio.shakes.insert(address.to_string(), Instant::now());
                let mut body = blue::plain_error(&error, &name, kind);
                if xbox && drifter_blue::xbox::ertm_on() {
                    body = drifter_core::lang::t("Older Xbox controllers need a Bluetooth setting changed. Open Bluetooth settings and press Fix It.").to_string();
                }
                if let Some(id) = self.notes_bus.as_ref().map(|bus| bus.next_id()) {
                    self.show_note(drifter_core::notify::Note {
                        id,
                        app: drifter_core::lang::t("Bluetooth").to_string(),
                        summary: drifter_core::lang::tf("{} did not connect", &[&name]),
                        body,
                        actions: Vec::new(),
                        expire: None,
                        asked: 9000,
                        critical: false,
                        icon: Some(drifter_core::blue::glyph(kind, 64, self.palette.ink)),
                    });
                }
            }
        }
        self.blue_list_refresh();
        self.kick_now();
    }

    pub(super) fn hud_icon_look(since: Instant, pace: f64, still: bool) -> (f64, f64) {
        if pace <= 0.0 {
            return (1.0, 1.0);
        }
        let t = since.elapsed().as_secs_f64() / (HUD_ICON_IN / pace);
        let scale = match still {
            true => 1.0,
            false => HUD_ICON_FROM + (1.0 - HUD_ICON_FROM) * drifter_core::drawer::ease(t),
        };
        (drifter_core::motion::smooth(t), scale)
    }

    pub(super) fn blue_output_hud(&mut self, address: &str) {
        let Some(device) = self.radio.now.as_ref().and_then(|blue| blue.find(address).cloned()) else {
            return;
        };
        let Some(font) = self.font.clone() else {
            return;
        };
        let icon = self.radio.art.glyphs.get(&device.kind).cloned();
        let (level, muted) = self.volume.unwrap_or((0.5, false));
        let reading = drifter_core::hud::Reading {
            gauge: drifter_core::hud::Gauge::Volume,
            level,
            muted,
        };
        let look = drifter_core::hud::Look::themed(&self.palette).named();
        let rect = look.rect(self.screen);
        let name = device.shown_name();
        self.paint_on_worker(
            "output hud",
            move || Some(look.paint_named(reading, &name, &font)),
            move |state, art| {
                let Some(art) = art else {
                    return;
                };
                let now = Instant::now();
                let to = reading.shown();
                state.hud = Some(super::Hud {
                    art,
                    rect,
                    until: now + HUD_TIME,
                    from: to,
                    to,
                    since: now,
                    icon: icon.map(|icon| (icon, now)),
                });
                state.kick_now();
            },
        );
    }

    pub(super) fn blue_power(&mut self, on: bool) {
        let Some(bus) = self.radio.bus.clone() else {
            return;
        };
        self.background("bluetooth power", move || {
            let done = bus.power(on);
            move |state: &mut State| {
                if let Err(error) = done {
                    log!("drifter: cannot turn Bluetooth {}: {error}", if on { "on" } else { "off" });
                    state.control_status.bluetooth = state.radio.now.as_ref().map(|blue| blue.on);
                    state.blue_fill(Tile::Bluetooth);
                    state.paint_control();
                }
            }
        });
    }

    pub(super) fn blue_search_as_needed(&mut self) {
        let want = self.control.is_some()
            && self.radio.list.as_ref().is_some_and(|list| !list.closing)
            && self.radio.now.as_ref().is_some_and(|blue| blue.on);
        if want == self.radio.searching {
            return;
        }
        self.radio.searching = want;
        let Some(bus) = self.radio.bus.clone() else {
            return;
        };
        self.background("bluetooth search", move || {
            let done = bus.search(want);
            move |_: &mut State| {
                if let Err(error) = done
                    && want
                {
                    log!("drifter: Bluetooth search did not start: {error}");
                }
            }
        });
    }

    pub(super) fn blue_items(&self) -> Vec<Item> {
        let failed: Vec<String> = self
            .radio
            .failed
            .iter()
            .filter(|(_, at)| at.elapsed() < FAILED_FOR)
            .map(|(address, _)| address.clone())
            .collect();
        let seen = match (self.radio.now.as_ref(), self.radio.missing) {
            (Some(blue), _) => Ok(blue),
            (None, missing) => Err(missing.unwrap_or(blue::Missing::Adapter)),
        };
        drifter_core::control::items(seen, &self.radio.busy, &failed)
    }

    pub(super) fn shown_items(&self) -> &[Item] {
        match self.radio.list.as_ref() {
            Some(list) if !list.closing => &list.items,
            _ => &[],
        }
    }

    pub(super) fn painted_items(&self) -> Vec<Item> {
        match self.radio.list.as_ref() {
            Some(list) if !list.closing => list.items.clone(),
            Some(list) if self.control.as_ref().is_some_and(|open| open.drawn_now(self) > self.control_look().size().y + 0.5) => list.items.clone(),
            _ => Vec::new(),
        }
    }

    pub(super) fn blue_toggle_list(&mut self) {
        let open = self.radio.list.as_ref().is_some_and(|list| !list.closing);
        match open {
            true => {
                if let Some(list) = self.radio.list.as_mut() {
                    list.closing = true;
                }
                let base = self.control_look().size().y;
                self.control_retarget(base);
            }
            false => {
                self.blue_art();
                let items = self.blue_items();
                let now = Instant::now();
                let look = self.control_look();
                let rows = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        let y = look.item_rect(&items, index).origin.y;
                        let born = now + Duration::from_secs_f64(ROW_STAGGER * index as f64 / self.window_pace.max(0.01));
                        let (level, signal) = item_numbers(item);
                        Row {
                            key: item.key.clone(),
                            item: item.clone(),
                            art: None,
                            painted: None,
                            from: y,
                            to: y,
                            moved: now,
                            bounce: false,
                            born,
                            gone: None,
                            level: (0.0, level, born),
                            signal: (0.0, signal, born),
                        }
                    })
                    .collect();
                self.radio.list = Some(List {
                    items,
                    rows,
                    lane: worker::Lane::default(),
                    closing: false,
                    epoch: now,
                });
                self.paint_rows();
                let size = self.control_look().size_with(self.shown_items());
                self.control_retarget(size.y);
            }
        }
        self.blue_search_as_needed();
        self.paint_control();
    }

    pub(super) fn blue_list_refresh(&mut self) {
        if self.control.is_none() || self.radio.list.as_ref().is_none_or(|list| list.closing) {
            return;
        }
        let items = self.blue_items();
        let look = self.control_look();
        let now = Instant::now();
        let pace = self.window_pace.max(0.01);
        let Some(list) = self.radio.list.as_mut() else {
            return;
        };
        if list.items == items {
            return;
        }
        let shown_y = |row: &Row| row_y(row, now, pace);
        let mut fresh = 0;
        for (index, item) in items.iter().enumerate() {
            let y = look.item_rect(&items, index).origin.y;
            match list.rows.iter_mut().find(|row| row.key == item.key) {
                Some(row) => {
                    let jumped = matches!(
                        (&row.item.what, &item.what),
                        (What::Device { connected: false, .. }, What::Device { connected: true, .. })
                    );
                    if row.gone.take().is_some() {
                        row.born = now;
                    }
                    if (row.to - y).abs() > 0.5 {
                        row.from = shown_y(row);
                        row.to = y;
                        row.moved = now;
                        row.bounce = jumped;
                    }
                    let (level, signal) = item_numbers(item);
                    if (row.level.1 - level).abs() > 0.01 {
                        row.level = (eased(row.level, pace, false), level, now);
                    }
                    if (row.signal.1 - signal).abs() > 0.01 {
                        row.signal = (eased(row.signal, pace, false), signal, now);
                    }
                    row.item = item.clone();
                }
                None => {
                    let born = now + Duration::from_secs_f64(ROW_STAGGER * fresh as f64 / pace);
                    let (level, signal) = item_numbers(item);
                    list.rows.push(Row {
                        key: item.key.clone(),
                        item: item.clone(),
                        art: None,
                        painted: None,
                        from: y,
                        to: y,
                        moved: now,
                        bounce: false,
                        born,
                        gone: None,
                        level: (0.0, level, born),
                        signal: (0.0, signal, born),
                    });
                    fresh += 1;
                }
            }
        }
        for row in list.rows.iter_mut() {
            if row.gone.is_none() && !items.iter().any(|item| item.key == row.key) {
                row.gone = Some(now);
            }
        }
        let resized = list.items.iter().map(Item::height).sum::<f64>() != items.iter().map(Item::height).sum::<f64>();
        list.items = items;
        self.paint_rows();
        if resized {
            let size = look.size_with(self.shown_items());
            self.control_retarget(size.y);
            self.paint_control();
        }
    }

    pub(super) fn blue_list_wind(&mut self) {
        let now = Instant::now();
        let pace = self.window_pace.max(0.01);
        self.radio.failed.retain(|(_, at)| at.elapsed() < FAILED_FOR);
        self.radio.pops.retain(|_, at| at.elapsed().as_secs_f64() < POP / pace);
        self.radio.shakes.retain(|_, at| at.elapsed().as_secs_f64() < SHAKE / pace);
        if let Some(list) = self.radio.list.as_mut() {
            list.rows
                .retain(|row| row.gone.is_none_or(|gone| now.duration_since(gone).as_secs_f64() < ROW_OUT / pace));
        }
        let closed = self.radio.list.as_ref().is_some_and(|list| list.closing)
            && self.control.as_ref().is_none_or(|open| !open.growing(self));
        if closed {
            self.radio.list = None;
            self.paint_control();
        }
        if let Some(pill) = self.radio.pill.as_mut() {
            if pill.gone.is_none() && now >= pill.until {
                pill.gone = Some(now);
            }
        }
        if self
            .radio
            .pill
            .as_ref()
            .is_some_and(|pill| pill.gone.is_some_and(|gone| now.duration_since(gone).as_secs_f64() >= PILL_OUT / pace))
        {
            self.radio.pill = None;
        }
        let stale = self.blue_items();
        if self.radio.list.as_ref().is_some_and(|list| !list.closing && list.items != stale) {
            self.blue_list_refresh();
        }
    }

    pub(super) fn blue_moving(&self) -> bool {
        let pace = self.window_pace;
        if pace <= 0.0 {
            return false;
        }
        let now = Instant::now();
        let open = self.control.is_some();
        let list = self.radio.list.as_ref().filter(|_| open);
        let rows = list.is_some_and(|list| {
            !list.closing
                && (self.radio.now.as_ref().is_some_and(|blue| blue.searching)
                    || list.rows.iter().any(|row| {
                        row.gone.is_some()
                            || now < row.born
                            || now.duration_since(row.born).as_secs_f64() < ROW_IN / pace
                            || now.duration_since(row.moved).as_secs_f64() < ROW_MOVE / pace
                            || row.level.2.elapsed().as_secs_f64() < LEVEL / pace
                            || row.signal.2.elapsed().as_secs_f64() < LEVEL / pace
                    }))
        });
        let busy = open && list.is_some() && !self.radio.busy.is_empty();
        let popping = !self.radio.pops.is_empty() || !self.radio.shakes.is_empty();
        let panel = self.control.as_ref().is_some_and(|panel| panel.growing(self) || panel.filling(pace));
        let pill = self.radio.pill.is_some();
        let pairing = !self.radio.asks.is_empty()
            && (self.radio.asked_at.elapsed().as_secs_f64() < (DIGIT_IN + DIGIT_STAGGER * 16.0) / pace
                || self.radio.typed_at.iter().any(|at| at.elapsed().as_secs_f64() < DIGIT_IN / pace));
        let morph = self.radio.tray_state.2.elapsed().as_secs_f64() < MORPH / pace;
        let hud = self.hud.as_ref().is_some_and(|hud| hud.icon.as_ref().is_some_and(|(_, at)| at.elapsed().as_secs_f64() < HUD_ICON_IN / pace));
        rows || busy || (open && popping) || panel || pill || pairing || morph || hud || list.is_some_and(|list| list.closing)
    }

    pub(super) fn blue_art(&mut self) {
        let key = (self.palette, self.look.accent, self.density);
        if self.radio.art.key == Some(key) || self.radio.art.busy {
            return;
        }
        self.radio.art.busy = true;
        let look = self.control_look();
        let ink = self.tray.ink;
        let accent = self.look.accent;
        self.on_worker(
            "bluetooth art",
            move || {
                let kinds = [
                    Kind::Headphones, Kind::Earbuds, Kind::Speaker, Kind::Mouse, Kind::Keyboard, Kind::Trackpad, Kind::Controller,
                    Kind::Phone, Kind::Tablet, Kind::Computer, Kind::Watch, Kind::Display, Kind::Printer, Kind::Other,
                ];
                let discs: Vec<((Kind, bool), drifter_core::Image)> = kinds
                    .iter()
                    .flat_map(|kind| [false, true].map(|lit| ((*kind, lit), look.disc(*kind, lit))))
                    .collect();
                let ring: Vec<drifter_core::Image> = (0..RING_FRAMES).map(|frame| look.ring_frame(frame, RING_FRAMES)).collect();
                let symbols: Vec<(u8, drifter_core::Image)> =
                    Tile::ALL.iter().map(|tile| (tile_index(*tile), look.tile_symbol(*tile, [255, 255, 255]))).collect();
                let mut disc = drifter_core::paint::Canvas::new(32, 32);
                disc.circle(Vec2::splat(16.0), 15.0, accent, 1.0);
                let tray = tray_glyphs(ink, accent);
                let mut dot = drifter_core::paint::Canvas::new(10, 10);
                dot.circle(Vec2::splat(5.0), 4.0, look.ink, 1.0);
                let fills: Vec<drifter_core::Image> = [5u8, 15, 100].iter().map(|level| look.battery_fill(*level)).collect();
                let bars = look.bars_lit();
                let glyphs: Vec<(Kind, drifter_core::Image)> = kinds
                    .iter()
                    .map(|kind| (*kind, drifter_core::blue::glyph(*kind, drifter_core::paint::dense(26.0), look.ink)))
                    .collect();
                (discs, ring, symbols, disc.image, tray, dot.image, fills, bars, glyphs)
            },
            move |state, (discs, ring, symbols, disc, tray, dot, fills, bars, glyphs)| {
                let renderer = &mut state.renderer;
                let mut art = Art {
                    key: Some(key),
                    ..Art::default()
                };
                for (key, image) in discs {
                    if let Ok(Some(tex)) = upload(renderer, Some(&image), "bluetooth disc") {
                        art.discs.insert(key, tex);
                    }
                }
                art.ring = ring
                    .iter()
                    .filter_map(|image| upload(renderer, Some(image), "bluetooth ring").ok().flatten())
                    .collect();
                for (index, image) in symbols {
                    if let Ok(Some(tex)) = upload(renderer, Some(&image), "control symbol") {
                        art.symbols.insert(index, tex);
                    }
                }
                art.accent_disc = upload(renderer, Some(&disc), "control fill").ok().flatten();
                art.tray = tray
                    .iter()
                    .filter_map(|image| upload(renderer, Some(image), "bluetooth tray").ok().flatten())
                    .collect();
                art.dot = upload(renderer, Some(&dot), "pin dot").ok().flatten();
                art.fills = fills
                    .iter()
                    .filter_map(|image| upload(renderer, Some(image), "battery fill").ok().flatten())
                    .collect();
                art.bars = upload(renderer, Some(&bars), "signal bars").ok().flatten();
                for (kind, image) in glyphs {
                    if let Ok(Some(tex)) = upload(renderer, Some(&image), "device glyph") {
                        art.glyphs.insert(kind, tex);
                    }
                }
                state.radio.art = art;
                state.tray.blue = state.radio.seen;
                state.kick_now();
            },
        );
    }

    fn paint_rows(&mut self) {
        let (Some(font), Some(small)) = (self.note_body.clone(), self.card_font.clone()) else {
            return;
        };
        let Some(list) = self.radio.list.as_mut() else {
            return;
        };
        let wanted: Vec<Item> = list
            .rows
            .iter()
            .filter(|row| row.painted.as_ref() != Some(&row.item))
            .map(|row| row.item.clone())
            .collect();
        if wanted.is_empty() || !list.lane.claim() {
            return;
        }
        let id = list.lane.id();
        let look = self.control_look();
        self.on_worker(
            "bluetooth rows",
            move || wanted.into_iter().map(|item| { let image = look.paint_item_with(&item, true, &font, &small); (item, image) }).collect::<Vec<_>>(),
            move |state, painted| {
                let Some(list) = state.radio.list.as_mut().filter(|list| list.lane.id() == id) else {
                    return;
                };
                let again = list.lane.release();
                for (item, image) in painted {
                    let Ok(Some(tex)) = upload(&mut state.renderer, Some(&image), "bluetooth row") else {
                        continue;
                    };
                    if let Some(row) = state.radio.list.as_mut().and_then(|list| list.rows.iter_mut().find(|row| row.key == item.key)) {
                        row.art = Some(tex);
                        row.painted = Some(item);
                    }
                }
                if again || state.radio.list.as_ref().is_some_and(|list| list.rows.iter().any(|row| row.painted.as_ref() != Some(&row.item))) {
                    state.paint_rows();
                }
                state.kick_now();
            },
        );
    }

    pub(super) fn blue_fill(&mut self, tile: Tile) {
        let on = self.control_status.on(tile);
        if let Some(open) = self.control.as_mut() {
            let slot = &mut open.fills[tile_index(tile) as usize];
            if slot.0 != on {
                *slot = (on, Instant::now());
            }
        }
    }

    pub(super) fn control_layers(&self) -> Option<(Vec<fade::Layer>, Rect)> {
        let open = self.control.as_ref()?;
        let art = open.art.as_ref()?;
        let look = self.control_look();
        let now = Instant::now();
        let pace = self.window_pace;
        let still = pace <= 0.0 || self.window_still;
        let drawn = open.drawn_now(self).max(1.0);
        let origin = open.rect.origin;
        let frame = Rect::new(origin, Vec2::new(open.rect.size.x, drawn));
        let mut layers: Vec<fade::Layer> = Vec::new();
        let width = open.rect.size.x;
        let art_h = open.art_height;
        let cap = look.cap().min(art_h / 2.0);
        let visible = drawn - look.pad - 2.0;

        if let Some(list) = self.radio.list.as_ref() {
            for row in &list.rows {
                let Some(row_art) = row.art.as_ref() else {
                    continue;
                };
                let height = row.item.height();
                let mut y = row_y(row, now, pace.max(0.01));
                let mut alpha = 1.0;
                let mut squash = 1.0;
                if pace > 0.0 {
                    if now < row.born {
                        continue;
                    }
                    let t = now.duration_since(row.born).as_secs_f64() / (ROW_IN / pace);
                    if t < 1.0 {
                        alpha *= drifter_core::motion::smooth(t);
                        if !still {
                            y += (1.0 - drifter_core::drawer::ease(t)) * ROW_RISE;
                        }
                    }
                    if let Some(gone) = row.gone {
                        let t = now.duration_since(gone).as_secs_f64() / (ROW_OUT / pace);
                        let left = 1.0 - drifter_core::motion::smooth(t);
                        alpha *= left;
                        if !still {
                            squash = left.max(0.0);
                        }
                    }
                    if row.key == "heading:nearby" && self.radio.now.as_ref().is_some_and(|blue| blue.searching) {
                        let wave = (list.epoch.elapsed().as_secs_f64() / BREATH * std::f64::consts::TAU).cos();
                        alpha *= 0.62 + 0.38 * (0.5 + 0.5 * wave);
                    }
                } else if row.gone.is_some() {
                    continue;
                }
                let shake = row
                    .item
                    .address()
                    .and_then(|address| self.radio.shakes.get(address))
                    .filter(|_| !still)
                    .map_or(0.0, |at| {
                        let t = (at.elapsed().as_secs_f64() / (SHAKE / pace)).clamp(0.0, 1.0);
                        (t * std::f64::consts::TAU * 3.0).sin() * SHAKE_REACH * (1.0 - t) * (1.0 - t)
                    });
                let top = y;
                if top >= visible || alpha <= 0.01 {
                    continue;
                }
                let shown_h = (visible - top).min(height);
                let squashed = height * squash;
                let left = origin + Vec2::new(look.pad + shake, top + (height - squashed) / 2.0);
                let rect = Rect::new(left, Vec2::new(width - look.pad * 2.0, squashed.min(shown_h)));
                let source = Rect::new(Vec2::ZERO, Vec2::new(width - look.pad * 2.0, height.min(shown_h / squash.max(0.01))));
                if let What::Device { kind, connected, battery, bars, .. } = &row.item.what {
                    let address = row.item.address().unwrap_or_default();
                    let fits = |part: Rect| part.max().y - origin.y <= visible && squash > 0.98;
                    if battery.is_some() {
                        let inner = look.battery_inner(&row.item);
                        let level = eased(row.level, pace, still).clamp(0.0, 100.0);
                        let place = Rect::new(left + inner.origin, Vec2::new(inner.size.x * level / 100.0, inner.size.y));
                        if let Some(fill) = self.radio.art.fills.get(fill_class(row.level.1))
                            && place.size.x >= 0.5
                            && fits(place)
                        {
                            layers.push(fade::Layer {
                                source: Some(Rect::new(Vec2::ZERO, place.size)),
                                alpha: alpha as f32,
                                ..fade::Layer::new(fill, place, fade::Shadow::None)
                            });
                        }
                    } else if bars.is_some()
                        && let Some(lit) = self.radio.art.bars.as_ref()
                    {
                        let area = look.bars_rect(&row.item);
                        let level = eased(row.signal, pace, still);
                        for step in 0..4 {
                            let shown = (level - step as f64).clamp(0.0, 1.0);
                            let bar = drifter_core::control::bar_source(step);
                            let place = Rect::new(left + area.origin + bar.origin, bar.size);
                            if shown > 0.01 && fits(place) {
                                layers.push(fade::Layer {
                                    source: Some(bar),
                                    alpha: (alpha * shown * 0.9) as f32,
                                    ..fade::Layer::new(lit, place, fade::Shadow::None)
                                });
                            }
                        }
                    }
                    let centre = origin + look.disc_at(Rect::new(Vec2::new(look.pad + shake, top), Vec2::new(width, height)));
                    if centre.y - origin.y + 16.0 <= visible {
                        if self.radio.busy.iter().any(|each| each == address)
                            && let Some(frame_art) = (!self.radio.art.ring.is_empty())
                                .then(|| {
                                    let turn = match still {
                                        true => 0.0,
                                        false => list.epoch.elapsed().as_secs_f64() / TURN,
                                    };
                                    let at = (turn * RING_FRAMES as f64) as usize % self.radio.art.ring.len();
                                    &self.radio.art.ring[at]
                                })
                        {
                            layers.push(fade::Layer {
                                alpha: alpha as f32,
                                ..fade::Layer::new(frame_art, Rect::from_center(centre, Vec2::splat(40.0)), fade::Shadow::None)
                            });
                        }
                        if let Some(disc) = self.radio.art.discs.get(&(*kind, *connected)) {
                            let pop = self.radio.pops.get(address).filter(|_| !still).map_or(1.0, |at| {
                                let t = at.elapsed().as_secs_f64() / (POP / pace);
                                POP_FROM + (1.0 - POP_FROM) * drifter_core::drawer::ease(t)
                            });
                            let side = 32.0 * pop * squash.max(0.01).sqrt();
                            layers.push(fade::Layer {
                                alpha: alpha as f32,
                                ..fade::Layer::new(disc, Rect::from_center(centre, Vec2::splat(side)), fade::Shadow::None)
                            });
                        }
                    }
                }
                layers.push(fade::Layer {
                    source: Some(source),
                    alpha: alpha as f32,
                    ..fade::Layer::new(row_art, rect, fade::Shadow::None)
                });
            }
        }

        for tile in Tile::ALL {
            let (on, since) = open.fills[tile_index(tile) as usize];
            let (shown, scale) = fill_level(on, since, pace, still);
            if shown <= 0.001 {
                continue;
            }
            let centre = origin + look.dot(tile);
            if let Some(symbol) = self.radio.art.symbols.get(&tile_index(tile)) {
                layers.push(fade::Layer {
                    alpha: shown as f32,
                    ..fade::Layer::new(symbol, Rect::from_center(centre, Vec2::splat(30.0)), fade::Shadow::None)
                });
            }
            if let Some(disc) = self.radio.art.accent_disc.as_ref() {
                layers.push(fade::Layer {
                    alpha: shown.sqrt() as f32,
                    ..fade::Layer::new(disc, Rect::from_center(centre, Vec2::splat(32.0 * scale)), fade::Shadow::None)
                });
            }
        }

        let body = (drawn.min(art_h) - cap).max(0.0);
        layers.push(fade::Layer {
            source: Some(Rect::new(Vec2::new(0.0, art_h - cap), Vec2::new(width, cap))),
            ..fade::Layer::new(art, Rect::new(origin + Vec2::new(0.0, drawn - cap), Vec2::new(width, cap)), fade::Shadow::None)
        });
        if drawn > art_h {
            let slice = (art_h - cap - 2.0).max(0.0);
            layers.push(fade::Layer {
                source: Some(Rect::new(Vec2::new(0.0, slice), Vec2::new(width, 1.0))),
                ..fade::Layer::new(art, Rect::new(origin + Vec2::new(0.0, body), Vec2::new(width, drawn - art_h)), fade::Shadow::None)
            });
        }
        layers.push(fade::Layer {
            source: Some(Rect::new(Vec2::ZERO, Vec2::new(width, body))),
            shade: Some(frame),
            ..fade::Layer::new(art, Rect::new(origin, Vec2::new(width, body)), fade::Shadow::Menu)
        });
        Some((layers, frame))
    }

    pub(super) fn press_control_item(&mut self, index: usize) {
        let Some(item) = self.shown_items().get(index).cloned() else {
            return;
        };
        if item.key == drifter_core::control::SERVICE_KEY {
            self.background("bluetooth service", || {
                let done = drifter_blue::Bus::start_service();
                move |state: &mut State| {
                    if let Err(error) = done
                        && let Some(id) = state.notes_bus.as_ref().map(|bus| bus.next_id())
                    {
                        state.show_note(drifter_core::notify::Note {
                            id,
                            app: drifter_core::lang::t("Bluetooth").to_string(),
                            summary: drifter_core::lang::t("Bluetooth is not running").to_string(),
                            body: error,
                            actions: Vec::new(),
                            expire: None,
                            asked: 9000,
                            critical: false,
                            icon: None,
                        });
                    }
                }
            });
            return;
        }
        if item.key == drifter_core::control::SETTINGS_KEY {
            self.control = None;
            self.radio.list = None;
            self.blue_search_as_needed();
            self.open_settings_at("bluetooth");
            self.kick_now();
            return;
        }
        if let Some(address) = item.address() {
            let address = address.to_string();
            self.blue_connect(&address);
        }
    }

    pub(super) fn blue_show_pill(&mut self, device: &blue::Device) {
        let (Some(font), Some(small)) = (self.note_body.clone(), self.card_font.clone()) else {
            return;
        };
        let look = blue::PillLook::themed(&self.palette, self.look.accent);
        let badge = self.held_by_fullscreen().is_some();
        let top = self.tray_band().map_or(0.0, |band| band.max().y);
        let rect = match badge {
            true => look.badge_rect(self.screen),
            false => look.rect(self.screen, top),
        };
        let now = Instant::now();
        self.radio.pill = Some(Pill {
            art: None,
            rect,
            born: now,
            until: now + if badge { BADGE_FOR } else { PILL_FOR },
            gone: None,
        });
        self.radio.badge = badge;
        let (name, kind, battery) = (device.shown_name(), device.kind, device.battery);
        let words = drifter_core::lang::t("Connected").to_string();
        self.paint_on_worker(
            "bluetooth pill",
            move || {
                Some(match badge {
                    true => look.paint_badge(kind, battery),
                    false => look.paint(&name, kind, battery, &words, &font, &small),
                })
            },
            move |state, art| {
                if let Some(pill) = state.radio.pill.as_mut().filter(|pill| pill.born == now) {
                    pill.art = art;
                    pill.born = Instant::now();
                }
                state.kick_now();
            },
        );
        self.kick_now();
    }

    pub(super) fn pill_layers(&self) -> Option<(Vec<fade::Layer>, Rect)> {
        let pill = self.radio.pill.as_ref()?;
        let art = pill.art.as_ref()?;
        let pace = self.window_pace;
        let still = self.window_still;
        let (alpha, scale) = match (pace <= 0.0, pill.gone) {
            (true, None) => (1.0, 1.0),
            (true, Some(_)) => (0.0, 1.0),
            (false, Some(gone)) => {
                let t = gone.elapsed().as_secs_f64() / (PILL_OUT / pace);
                let shrink = if still { 0.0 } else { 0.06 };
                (1.0 - drifter_core::motion::smooth(t), 1.0 - shrink * drifter_core::motion::smooth(t))
            }
            (false, None) => {
                let t = pill.born.elapsed().as_secs_f64() / (PILL_IN / pace);
                let scale = match still {
                    true => 1.0,
                    false => PILL_FROM + (1.0 - PILL_FROM) * drifter_core::drawer::ease(t),
                };
                (drifter_core::motion::smooth(t), scale)
            }
        };
        if alpha <= 0.0 {
            return None;
        }
        let rect = Rect::from_center(pill.rect.center(), pill.rect.size * scale);
        Some((
            vec![fade::Layer {
                alpha: alpha as f32,
                scales: false,
                ..fade::Layer::new(art, rect, fade::Shadow::Menu)
            }],
            rect,
        ))
    }

    pub(super) fn blue_agent(&mut self, event: drifter_blue::agent::Event) {
        match event {
            drifter_blue::agent::Event::Ask(ask, reply) => {
                if let Some(held) = self.radio.asks.iter_mut().find(|(held, _)| held.id == ask.id) {
                    held.0 = ask;
                } else {
                    if self.radio.asks.is_empty() {
                        self.radio.asked_at = Instant::now();
                        self.radio.typed.clear();
                        self.radio.typed_at.clear();
                    }
                    self.radio.asks.push((ask, reply));
                }
            }
            drifter_blue::agent::Event::Done(id) => {
                self.radio.asks.retain(|(ask, _)| ask.id != id);
                self.radio.asked_at = Instant::now();
            }
        }
        self.paint_pairing();
        self.kick_now();
    }

    fn pairing_fonts(&self) -> Option<(std::sync::Arc<Font>, std::sync::Arc<Font>, std::sync::Arc<Font>)> {
        Some((self.search_font.clone()?, self.font.clone()?, self.lock_small.clone().or_else(|| self.search_font.clone())?))
    }

    pub(super) fn paint_pairing(&mut self) {
        let Some((ask, _)) = self.radio.asks.first().cloned() else {
            self.radio.pair_art = None;
            self.radio.pair_haze = None;
            return;
        };
        let Some((title, font, big)) = self.pairing_fonts() else {
            return;
        };
        self.blue_art();
        if self.radio.pair_haze.is_none() {
            self.paint_on_worker("pairing dim", move || Some(worker::dim(0.28)), |state, art| {
                if !state.radio.asks.is_empty() {
                    state.radio.pair_haze = art;
                }
            });
        }
        let look = drifter_core::pairing::Look::themed(&self.palette, self.look.accent);
        let rect = look.rect(self.screen, &ask, &title, &font);
        let typed = self.radio.typed.chars().count();
        let digits: Vec<(char, bool)> = drifter_core::pairing::shown(&ask, &self.radio.typed)
            .into_iter()
            .filter(|key| !self.radio.digits.contains_key(key))
            .collect();
        self.paint_in_lane(
            "pairing prompt",
            |state| Some(&mut state.radio.pair_lane),
            State::paint_pairing,
            move || Some(look.paint(&ask, typed, &title, &font)),
            move |state, art| {
                if !state.radio.asks.is_empty() {
                    state.radio.pair_art = Some((art, rect));
                }
            },
        );
        if !digits.is_empty() {
            self.on_worker(
                "pairing digits",
                move || digits.into_iter().map(|(c, lit)| ((c, lit), look.digit(c, lit, &big))).collect::<Vec<_>>(),
                |state, painted| {
                    for (key, image) in painted {
                        if let Ok(Some(tex)) = upload(&mut state.renderer, Some(&image), "pairing digit") {
                            state.radio.digits.insert(key, tex);
                        }
                    }
                    state.kick_now();
                },
            );
        }
    }

    pub(super) fn pairing_layers(&self) -> Option<(Vec<fade::Layer>, Rect)> {
        let (ask, _) = self.radio.asks.first()?;
        let (art, rect) = self.radio.pair_art.as_ref()?;
        let (title, font, _) = self.pairing_fonts()?;
        let look = drifter_core::pairing::Look::themed(&self.palette, self.look.accent);
        let pace = self.window_pace;
        let still = pace <= 0.0 || self.window_still;
        let mut layers = Vec::new();
        let slots = look.digit_rects(ask, &title, &font);
        for (at, (key, slot)) in drifter_core::pairing::shown(ask, &self.radio.typed).into_iter().zip(slots).enumerate() {
            let Some(tex) = self.radio.digits.get(&key) else {
                continue;
            };
            let since = match ask.wants_typing() {
                true => self.radio.typed_at.get(at).copied().unwrap_or(self.radio.asked_at),
                false => self.radio.asked_at + Duration::from_secs_f64(DIGIT_STAGGER * at as f64 / pace.max(0.01)),
            };
            let (alpha, scale) = match pace <= 0.0 {
                true => (1.0, 1.0),
                false => {
                    let now = Instant::now();
                    if now < since {
                        continue;
                    }
                    let t = now.duration_since(since).as_secs_f64() / (DIGIT_IN / pace);
                    let scale = match still {
                        true => 1.0,
                        false => DIGIT_FROM + (1.0 - DIGIT_FROM) * drifter_core::drawer::ease(t),
                    };
                    (drifter_core::motion::smooth(t), scale)
                }
            };
            let place = Rect::new(rect.origin + slot.origin, slot.size);
            layers.push(fade::Layer {
                alpha: alpha as f32,
                ..fade::Layer::new(tex, Rect::from_center(place.center(), look.digit * scale), fade::Shadow::None)
            });
        }
        if let Some(dot) = self.radio.art.dot.as_ref() {
            for (at, centre) in look.pin_dots(ask, self.radio.typed.chars().count(), &title, &font).into_iter().enumerate() {
                let t = self
                    .radio
                    .typed_at
                    .get(at)
                    .filter(|_| !still)
                    .map_or(1.0, |since| since.elapsed().as_secs_f64() / (DIGIT_IN / pace));
                let scale = 0.3 + 0.7 * drifter_core::drawer::ease(t);
                layers.push(fade::Layer::new(dot, Rect::from_center(rect.origin + centre, Vec2::splat(10.0 * scale)), fade::Shadow::None));
            }
        }
        layers.push(fade::Layer::new(art, *rect, fade::Shadow::Modal));
        if let Some(haze) = self.radio.pair_haze.as_ref() {
            layers.push(fade::Layer {
                scales: false,
                ..fade::Layer::new(haze, Rect::new(Vec2::ZERO, self.screen), fade::Shadow::None)
            });
        }
        Some((layers, *rect))
    }

    pub(super) fn pairing_look(t: f64, still: bool) -> (f64, f64) {
        let grow = drifter_core::drawer::ease(t);
        let scale = match still {
            true => 1.0,
            false => PAIR_FROM + (1.0 - PAIR_FROM) * grow,
        };
        (drifter_core::motion::smooth(t), scale)
    }

    pub(super) fn pairing_in() -> f64 {
        PAIR_IN
    }

    pub(super) fn pairing_open(&self) -> bool {
        self.radio.pair_art.is_some() && !self.radio.asks.is_empty()
    }

    pub(super) fn pair_key(&mut self, key: LockKey) {
        let Some((ask, _)) = self.radio.asks.first().cloned() else {
            return;
        };
        match key {
            LockKey::Letter(c) if drifter_core::pairing::takes(&ask, &self.radio.typed, c) => {
                self.radio.typed.push(c);
                self.radio.typed_at.push(Instant::now());
            }
            LockKey::Back => {
                self.radio.typed.pop();
                self.radio.typed_at.pop();
            }
            LockKey::Enter => {
                self.pair_answer(true);
                return;
            }
            LockKey::Clear => {
                self.pair_answer(false);
                return;
            }
            LockKey::Vt(vt) => {
                self.change_vt(vt);
                return;
            }
            _ => return,
        }
        self.paint_pairing();
        self.kick_now();
    }

    pub(super) fn pair_press(&mut self) -> bool {
        let Some((ask, _)) = self.radio.asks.first().cloned() else {
            return false;
        };
        let Some((_, rect)) = self.radio.pair_art.as_ref() else {
            return true;
        };
        let rect = *rect;
        let look = drifter_core::pairing::Look::themed(&self.palette, self.look.accent);
        match look.hit(rect.size, &ask, self.pointer - rect.origin) {
            Some(drifter_core::pairing::Press::Go) => self.pair_answer(true),
            Some(drifter_core::pairing::Press::Cancel) => self.pair_answer(false),
            None => {}
        }
        true
    }

    fn pair_answer(&mut self, yes: bool) {
        if self.radio.asks.is_empty() {
            return;
        }
        let ask = self.radio.asks[0].0.clone();
        if yes && !drifter_core::pairing::ready(&ask, &self.radio.typed) {
            return;
        }
        let (ask, reply) = self.radio.asks.remove(0);
        if let blue::Question::Incoming(_) = ask.question {
            self.airdrop_answer(yes);
            self.radio.asked_at = Instant::now();
            self.paint_pairing();
            self.kick_now();
            return;
        }
        if ask.question == blue::Question::Mic {
            if yes {
                let sound = self.radio.prefs.sound(&ask.address);
                let address = ask.address.clone();
                self.background("bluetooth mic", move || {
                    let done = drifter_blue::sound::mic(&address, true, sound);
                    move |_: &mut State| {
                        if let Err(error) = done {
                            log!("drifter: cannot turn on the mic of {address}: {error}");
                        }
                    }
                });
            }
            self.radio.asked_at = Instant::now();
            self.paint_pairing();
            self.kick_now();
            return;
        }
        let said = match (yes, &ask.question) {
            (false, _) => None,
            (true, blue::Question::Type | blue::Question::TypePin) => Some(self.radio.typed.clone()),
            (true, _) => Some(String::new()),
        };
        match reply {
            Some(reply) => drifter_blue::agent::answer(&reply, said),
            None if !yes => {
                if let Some(bus) = self.radio.bus.clone() {
                    let address = ask.address.clone();
                    self.background("bluetooth cancel", move || {
                        let _ = bus.cancel_pairing(&address);
                        |_: &mut State| {}
                    });
                }
            }
            None => {}
        }
        self.radio.typed.clear();
        self.radio.typed_at.clear();
        self.radio.asked_at = Instant::now();
        self.paint_pairing();
        self.kick_now();
    }

    fn blue_tray_state(&mut self) {
        let state = match self.radio.now.as_ref() {
            None => 0,
            Some(blue) if !blue.on => 0,
            Some(blue) if blue.connected().next().is_some() => 2,
            Some(_) => 1,
        };
        let (now, _, _) = self.radio.tray_state;
        if now != state {
            self.radio.tray_state = (state, now, Instant::now());
        }
        if !self.tray.blue {
            self.tray.blue = true;
            self.blue_art();
        }
    }

    pub(super) fn blue_tray_rect(&self) -> Option<Rect> {
        if !self.tray.blue || self.radio.art.tray.is_empty() {
            return None;
        }
        Some(self.tray.blue_rect(self.tray_band()?))
    }

    pub(super) fn blue_tray_elements(&self, elements: &mut Vec<Skin>) {
        let Some(rect) = self.blue_tray_rect() else {
            return;
        };
        let (now, was, since) = self.radio.tray_state;
        let pace = self.window_pace;
        let t = match pace <= 0.0 {
            true => 1.0,
            false => since.elapsed().as_secs_f64() / (MORPH / pace),
        };
        let still = self.window_still;
        let art = &self.radio.art.tray;
        if t < 1.0
            && let Some(old) = art.get(was)
        {
            let alpha = 1.0 - drifter_core::motion::smooth(t);
            elements.push(Skin::Texture(element(old, rect, alpha as f32)));
        }
        if let Some(new) = art.get(now) {
            let scale = MORPH_FROM + (1.0 - MORPH_FROM) * drifter_core::drawer::ease(t.clamp(0.0, 1.0));
            let scale = if t >= 1.0 || still { 1.0 } else { scale };
            let alpha = drifter_core::motion::smooth(t);
            elements.push(Skin::Texture(element(new, Rect::from_center(rect.center(), rect.size * scale), alpha as f32)));
        }
    }

    pub(super) fn open_blue_menu(&mut self) {
        let items = match self.radio.now.as_ref() {
            Some(blue) => blue::menu(blue),
            None => vec![drifter_core::menu::Item::line(blue::MENU_SETTINGS, drifter_core::lang::t("Bluetooth Settings…"))],
        };
        let at = self.blue_tray_rect().map_or(self.pointer, |rect| Vec2::new(rect.origin.x, rect.max().y + 6.0));
        self.close_panels();
        self.open_popover(items, at, Target::Bluetooth);
    }

    pub(super) fn blue_menu_pick(&mut self, id: i32) {
        match id {
            blue::MENU_POWER => {
                let on = !self.radio.now.as_ref().is_some_and(|blue| blue.on);
                self.control_status.bluetooth = Some(on);
                self.blue_power(on);
            }
            blue::MENU_SETTINGS => self.open_settings_at("bluetooth"),
            at => {
                let address = self
                    .radio
                    .now
                    .as_ref()
                    .and_then(|blue| blue.devices.get(at.max(0) as usize))
                    .map(|device| device.address.clone());
                if let Some(address) = address {
                    self.blue_connect(&address);
                }
            }
        }
        self.kick_now();
    }

    pub(super) fn press_blue_tray(&mut self) -> bool {
        if !self.blue_tray_rect().is_some_and(|rect| rect.contains(self.pointer)) {
            return false;
        }
        if self.control.is_none() {
            self.toggle_control();
        }
        if self.radio.list.as_ref().is_none_or(|list| list.closing) {
            self.blue_toggle_list();
        }
        true
    }
}

fn item_numbers(item: &Item) -> (f64, f64) {
    match item.what {
        What::Device { battery, bars, .. } => (battery.map_or(0.0, f64::from), bars.map_or(0.0, f64::from)),
        _ => (0.0, 0.0),
    }
}

fn eased((from, to, since): (f64, f64, Instant), pace: f64, still: bool) -> f64 {
    match still || pace <= 0.0 {
        true => to,
        false => drifter_core::hud::eased_level(from, to, since.elapsed().as_secs_f64() / (LEVEL / pace)),
    }
}

fn fill_class(level: f64) -> usize {
    match level.round() as u8 {
        0..=10 => 0,
        11..=20 => 1,
        _ => 2,
    }
}

fn row_y(row: &Row, now: Instant, pace: f64) -> f64 {
    let t = now.duration_since(row.moved).as_secs_f64() / (ROW_MOVE / pace);
    drifter_core::dock::glide(row.from, row.to, t, row.bounce)
}

fn fill_level(on: bool, since: Instant, pace: f64, still: bool) -> (f64, f64) {
    if pace <= 0.0 {
        return if on { (1.0, 1.0) } else { (0.0, 1.0) };
    }
    let span = if on { FILL_IN } else { FILL_OUT };
    let t = since.elapsed().as_secs_f64() / (span / pace);
    let fade = drifter_core::motion::smooth(t);
    let shown = if on { fade } else { 1.0 - fade };
    let scale = match (still, on) {
        (true, _) => 1.0,
        (false, true) => drifter_core::drawer::ease(t),
        (false, false) => 1.0 - fade,
    };
    (shown.clamp(0.0, 1.0), scale.max(0.0))
}

fn tray_glyphs(ink: [u8; 3], accent: [u8; 3]) -> Vec<drifter_core::Image> {
    let side = drifter_core::paint::dense(20.0);
    let off = {
        let mut image = blue::svg_glyph(blue::rune(), side, ink);
        for pixel in image.rgba.chunks_mut(4) {
            for channel in pixel.iter_mut() {
                *channel = (f64::from(*channel) * 0.45).round() as u8;
            }
        }
        image
    };
    let on = blue::svg_glyph(blue::rune(), side, ink);
    let mut linked = drifter_core::paint::Canvas::new(20, 20);
    linked.stamp(&on, Vec2::ZERO, 1.0);
    linked.circle(Vec2::new(3.0, 10.0), 1.6, accent, 1.0);
    linked.circle(Vec2::new(17.0, 10.0), 1.6, accent, 1.0);
    vec![off, on, linked.image]
}

impl centers::Panel {
    pub(super) fn drawn_now(&self, state: &State) -> f64 {
        let (from, to, since) = self.drawn;
        let pace = state.window_pace;
        if pace <= 0.0 || state.window_still {
            return to;
        }
        let growing = to > from;
        let span = if growing { LIST_OPEN } else { LIST_CLOSE };
        let t = since.elapsed().as_secs_f64() / (span / pace);
        if t >= 1.0 {
            return to;
        }
        let shape = match growing {
            true => drifter_core::drawer::ease(t),
            false => drifter_core::motion::smooth(t),
        };
        from + (to - from) * shape
    }

    pub(super) fn growing(&self, state: &State) -> bool {
        let (from, to, since) = self.drawn;
        let pace = state.window_pace;
        if pace <= 0.0 || state.window_still || from == to {
            return false;
        }
        let span = if to > from { LIST_OPEN } else { LIST_CLOSE };
        since.elapsed().as_secs_f64() < span / pace
    }

    pub(super) fn filling(&self, pace: f64) -> bool {
        pace > 0.0 && self.fills.iter().any(|(on, since)| since.elapsed().as_secs_f64() < if *on { FILL_IN } else { FILL_OUT } / pace)
    }
}
