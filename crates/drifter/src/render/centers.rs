use super::*;
use drifter_core::control::{Hit as ControlHit, Status, Tile};

pub(super) struct Panel {
    pub rect: Rect,
    pub art: Option<TextureBuffer<Tex>>,
    pub held: Option<usize>,
    pub last: usize,
    pub lane: worker::Lane,
    pub art_height: f64,
    pub drawn: (f64, f64, Instant),
    pub pending: Option<f64>,
    pub fills: [(bool, Instant); 4],
}

impl Panel {
    pub(super) fn new(rect: Rect, status: &Status) -> Panel {
        let long_ago = Instant::now() - Duration::from_secs(60);
        Panel {
            rect,
            art: None,
            held: None,
            last: 1,
            lane: worker::Lane::default(),
            art_height: rect.size.y,
            drawn: (rect.size.y, rect.size.y, long_ago),
            pending: None,
            fills: Tile::ALL.map(|tile| (status.on(tile), long_ago)),
        }
    }
}

impl State {
    pub(super) fn control_rect(&self) -> Option<Rect> {
        self.control_icon.as_ref()?;
        Some(self.tray.control_rect(self.tray_band()?))
    }

    fn control_status(&self) -> Status {
        Status {
            wifi: self.control_status.wifi.clone(),
            bluetooth: self.radio.now.as_ref().map(|blue| blue.on).or(self.control_status.bluetooth),
            blue_note: self.radio.now.as_ref().map(drifter_core::blue::Blue::note).unwrap_or_default(),
            quiet: self.notify.quiet,
            dark: self.palette == drifter_core::theme::DARK,
            light: self.light,
            volume: self.volume,
        }
    }

    fn refresh_radios(&self) {
        self.background("control-status", || {
            let wifi = radios();
            move |state: &mut State| {
                state.control_status.wifi = wifi;
                if state.control.is_some() {
                    state.paint_control();
                }
            }
        });
    }

    pub(super) fn control_look(&self) -> drifter_core::control::Look {
        let plain = drifter_core::control::Look::themed(&self.palette, self.look.accent);
        drifter_core::control::Look {
            back_alpha: self.veil(plain.back_alpha),
            ..plain
        }
    }

    pub(super) fn toggle_control(&mut self) {
        if self.control.take().is_some() {
            self.radio.list = None;
            self.blue_search_as_needed();
            self.kick_now();
            return;
        }
        self.close_panels();
        self.control_status = self.control_status();
        self.blue_art();
        self.paint_control();
        self.refresh_radios();
    }

    pub(super) fn control_retarget(&mut self, height: f64) {
        let drawn = self.control.as_ref().map(|open| open.drawn_now(self));
        let Some(open) = self.control.as_mut() else {
            return;
        };
        let now = drawn.unwrap_or(height);
        match height < open.art_height - 0.5 && self.radio.list.as_ref().is_some_and(|list| list.closing) {
            true => {
                open.drawn = (now, height, Instant::now());
                open.pending = None;
            }
            false => open.pending = Some(height),
        }
    }

    pub(super) fn paint_control(&mut self) {
        self.paint_control_art();
        self.kick_now();
    }

    fn paint_control_art(&mut self) {
        let (Some(font), Some(small)) = (self.note_body.clone(), self.card_font.clone()) else {
            return;
        };
        let look = self.control_look();
        let items = self.painted_items();
        let size = look.size_with(&items);
        let rect = match (self.control.as_ref(), self.control_rect()) {
            (Some(open), _) => Rect::new(open.rect.origin, size),
            (None, Some(under)) => look.rect(under, self.screen),
            (None, None) => return,
        };
        if self.control.is_none() {
            let status = self.control_status.clone();
            self.control = Some(Panel::new(rect, &status));
        }
        let status = self.control_status.clone();
        let bare = !self.radio.art.symbols.is_empty() && self.radio.art.accent_disc.is_some();
        let frosting = self.frosting();
        self.paint_in_lane(
            "control center",
            |state| state.control.as_mut().map(|open| &mut open.lane),
            State::paint_control_art,
            move || {
                let mut image = look.paint_with(&status, &items, bare, &font, &small);
                worker::frost(&frosting, &mut image, rect.origin, look.back_alpha);
                Some(image)
            },
            move |state, art| {
                let drawn = state.control.as_ref().map(|open| open.drawn_now(state));
                if let Some(open) = state.control.as_mut() {
                    open.art = Some(art);
                    open.art_height = size.y;
                    open.rect = rect;
                    if open.pending.is_some_and(|want| (want - size.y).abs() < 0.5) {
                        open.pending = None;
                        open.drawn = (drawn.unwrap_or(size.y), size.y, Instant::now());
                    } else if open.pending.is_none() && !state.radio.list.as_ref().is_some_and(|list| list.closing) {
                        let (_, to, _) = open.drawn;
                        if (to - size.y).abs() > 0.5 {
                            open.drawn = (drawn.unwrap_or(size.y), size.y, Instant::now());
                        }
                    }
                }
            },
        );
    }

    pub(super) fn close_panels(&mut self) {
        self.popover = None;
        self.control = None;
        self.radio.list = None;
        self.notes_panel = None;
        self.glance_panel = None;
        self.charges = None;
    }

    pub(super) fn press_centers(&mut self, code: u32) -> bool {
        if let Some(open) = self.control.as_ref() {
            let rect = Rect::new(open.rect.origin, Vec2::new(open.rect.size.x, open.drawn_now(self)));
            if !rect.contains(self.pointer) {
                let on_button = self.control_rect().is_some_and(|button| button.contains(self.pointer))
                    || self.blue_tray_rect().is_some_and(|button| button.contains(self.pointer));
                self.control = None;
                self.radio.list = None;
                self.blue_search_as_needed();
                self.kick_now();
                return on_button;
            }
            if code == BTN_LEFT {
                let look = self.control_look();
                let items = self.shown_items().to_vec();
                match look.hit(self.pointer - rect.origin, &items) {
                    Some(ControlHit::Tile(tile)) => self.flip_tile(tile),
                    Some(ControlHit::More(_)) => self.blue_toggle_list(),
                    Some(ControlHit::Item(index)) => self.press_control_item(index),
                    Some(ControlHit::Light(level)) => {
                        if let Some(open) = self.control.as_mut() {
                            open.held = Some(0);
                            open.last = 0;
                        }
                        self.slide_control(0, level);
                    }
                    Some(ControlHit::Volume(level)) => {
                        if let Some(open) = self.control.as_mut() {
                            open.held = Some(1);
                            open.last = 1;
                        }
                        self.slide_control(1, level);
                    }
                    None => {}
                }
            }
            return true;
        }
        if let Some((_, rect, rows)) = self.notes_panel.as_ref().map(|(art, rect, rows)| (art.clone(), *rect, *rows)) {
            if !rect.contains(self.pointer) {
                let on_clock = self.clock.as_ref().is_some_and(|(_, _, clock)| clock.contains(self.pointer));
                self.notes_panel = None;
                self.kick_now();
                return on_clock;
            }
            let look = self.notes_look();
            if code == BTN_LEFT && look.clear_rect(rows).contains(self.pointer - rect.origin) {
                let _ = std::fs::write(Config::state_dir().join("notes"), "");
                self.paint_notes();
            }
            return true;
        }
        false
    }

    pub(super) fn drag_control(&mut self) -> bool {
        let Some((which, rect)) = self
            .control
            .as_ref()
            .and_then(|open| open.held.map(|which| (which, open.rect)))
        else {
            return false;
        };
        let level = self.control_look().level_at(which, self.pointer.x - rect.origin.x);
        self.slide_control(which, level);
        true
    }

    pub(super) fn nudge_panels(&mut self, steps: f64) {
        let step = steps * 0.01;
        if let Some(which) = self.control.as_ref().map(|open| open.last) {
            let now = match which {
                0 => self.light,
                _ => self.volume.map(|(level, _)| level),
            };
            if let Some(now) = now {
                self.slide_control(which, (now + step).clamp(0.0, 1.0));
            }
            return;
        }
    }

    pub(super) fn release_control(&mut self) {
        if let Some(open) = self.control.as_mut() {
            open.held = None;
        }
    }

    fn slide_control(&mut self, which: usize, level: f64) {
        match which {
            0 => {
                self.set_gauge(drifter_core::hud::Gauge::Brightness, level);
                self.control_status.light = self.light;
            }
            _ => {
                self.set_gauge(drifter_core::hud::Gauge::Volume, level);
                self.control_status.volume = self.volume;
            }
        }
        self.control_stale = true;
        self.kick_now();
    }

    pub(super) fn wind_the_control(&mut self) {
        if std::mem::take(&mut self.control_stale) && self.control.is_some() {
            self.paint_control_art();
        }
        self.blue_list_wind();
        self.blue_search_as_needed();
    }

    fn flip_tile(&mut self, tile: Tile) {
        let on = !self.control_status.on(tile);
        let spawn = |program: &str, args: &[&str]| {
            let _ = std::process::Command::new(program)
                .args(args)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn();
        };
        let word = if on { "on" } else { "off" };
        match tile {
            Tile::Wifi => {
                if let Some((held, _)) = self.control_status.wifi.as_mut() {
                    *held = on;
                    spawn("nmcli", &["radio", "wifi", word]);
                }
            }
            Tile::Bluetooth => {
                if let Some(held) = self.control_status.bluetooth.as_mut() {
                    *held = on;
                    self.blue_power(on);
                }
            }
            Tile::Quiet => {
                self.control_status.quiet = on;
                self.save_key(|text| drifter_core::settings::write_flag(text, "notify", "quiet", on));
            }
            Tile::Dark => {
                self.control_status.dark = on;
                self.reveal_from = Some(self.pointer);
                let style = if on { "dark" } else { "light" };
                self.save_key(|text| drifter_core::settings::write_word(text, "appearance", "style", style));
            }
        }
        self.blue_fill(tile);
        self.paint_control();
    }

    pub(super) fn save_key(&self, change: impl Fn(&str) -> Result<String, String>) {
        let text = std::fs::read_to_string(&self.config_path).unwrap_or_default();
        match change(&text) {
            Ok(out) => {
                if let Err(error) = std::fs::write(&self.config_path, out) {
                    log!("drifter: cannot save the setting: {error}");
                }
            }
            Err(error) => log!("drifter: cannot save the setting: {error}"),
        }
    }

    fn notes_look(&self) -> drifter_core::calendar::Look {
        let plain = drifter_core::calendar::Look::themed(&self.palette, self.look.accent);
        drifter_core::calendar::Look {
            back_alpha: self.veil(plain.back_alpha),
            ..plain
        }
    }

    pub(super) fn toggle_notes(&mut self) {
        if self.notes_panel.take().is_some() {
            self.kick_now();
            return;
        }
        self.close_panels();
        self.paint_notes();
    }

    fn paint_notes(&mut self) {
        let (Some(font), Some(title)) = (self.font.clone(), self.search_font.clone()) else {
            return;
        };
        let (Some(head), Some(body), Some(meta)) = (self.note_head.clone(), self.note_body.clone(), self.card_font.clone()) else {
            return;
        };
        let Some(under) = self.clock.as_ref().map(|(_, _, rect)| *rect) else {
            return;
        };
        let look = self.notes_look();
        let screen = self.screen;
        let frosting = self.frosting();
        if !self.notes_lane.claim() {
            return;
        }
        let id = self.notes_lane.id();
        self.on_worker(
            "notification center",
            move || {
                let notes = std::fs::read_to_string(Config::state_dir().join("notes"))
                    .map(|text| drifter_core::notify::recall(&text))
                    .unwrap_or_default();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|since| since.as_secs())
                    .unwrap_or(0);
                let today = drifter_core::calendar::today();
                let rows = drifter_core::calendar::rows_now();
                let kinds = drifter_core::notify::Type { head: &head, body: &body, meta: &meta };
                let mut image = look.paint(today, &notes, now, &font, &title, &kinds);
                let rect = look.rect(under, screen, rows, notes.len());
                worker::frost(&frosting, &mut image, rect.origin, look.back_alpha);
                (image, rect, rows)
            },
            move |state, (image, rect, rows)| {
                if state.notes_lane.id() != id {
                    return;
                }
                let rerun = state.notes_lane.release();
                if let Ok(Some(art)) = upload(&mut state.renderer, Some(&image), "notification center") {
                    state.notes_panel = Some((art, rect, rows));
                }
                if rerun {
                    state.paint_notes();
                }
                state.kick_now();
            },
        );
    }

}

fn radios() -> Option<(bool, String)> {
    let read = |program: &str, args: &[&str]| {
        std::process::Command::new(program)
            .args(args)
            .stderr(std::process::Stdio::null())
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let wifi = read("nmcli", &["-t", "radio", "wifi"]).map(|radio| {
        let on = radio.trim() == "enabled";
        let name = read("nmcli", &["-t", "-f", "ACTIVE,SSID", "device", "wifi", "list", "--rescan", "no"])
            .and_then(|list| {
                list.lines()
                    .map(drifter_core::net::terse)
                    .find(|fields| fields.first().is_some_and(|active| active == "yes"))
                    .and_then(|fields| fields.get(1).cloned())
            })
            .unwrap_or_default();
        (on, name)
    });
    wifi
}
