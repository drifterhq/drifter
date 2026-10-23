use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::blue::{Blue, Kind};
use crate::text::Font;

pub fn paint_icon(side: u32, ink: [u8; 3]) -> Image {
    let size = f64::from(side.max(8));
    let mut canvas = Canvas::new(side, side);
    let pill = |canvas: &mut Canvas, y: f64, knob_left: bool| {
        let rect = Rect::new(Vec2::new(size * 0.1, y), Vec2::new(size * 0.8, size * 0.3));
        canvas.round_rect(rect, size * 0.15, ink, 1.0);
        let x = if knob_left { rect.origin.x + size * 0.15 } else { rect.max().x - size * 0.15 };
        canvas.circle(Vec2::new(x, rect.center().y), size * 0.1, [0, 0, 0], 1.0);
    };
    pill(&mut canvas, size * 0.14, false);
    pill(&mut canvas, size * 0.56, true);
    let mut image = canvas.image;
    for pixel in image.rgba.chunks_mut(4) {
        if pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 0 && pixel[3] == 255 {
            pixel[3] = 0;
        }
    }
    image
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tile {
    Wifi,
    Bluetooth,
    AirDrop,
    Quiet,
    Dark,
}

impl Tile {
    pub const ALL: [Tile; 5] = [Tile::Wifi, Tile::Bluetooth, Tile::AirDrop, Tile::Quiet, Tile::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Tile::Wifi => crate::lang::t("Wi-Fi"),
            Tile::Bluetooth => crate::lang::t("Bluetooth"),
            Tile::AirDrop => "AirDrop",
            Tile::Quiet => crate::lang::t("Do Not Disturb"),
            Tile::Dark => crate::lang::t("Dark Mode"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Status {
    pub wifi: Option<(bool, String)>,
    pub bluetooth: Option<bool>,
    pub blue_note: String,
    pub airdrop: Option<(bool, String)>,
    pub quiet: bool,
    pub dark: bool,
    pub light: Option<f64>,
    pub volume: Option<(f64, bool)>,
}

impl Status {
    pub fn on(&self, tile: Tile) -> bool {
        match tile {
            Tile::Wifi => self.wifi.as_ref().is_some_and(|(on, _)| *on),
            Tile::Bluetooth => self.bluetooth.unwrap_or(false),
            Tile::AirDrop => self.airdrop.as_ref().is_some_and(|(on, _)| *on),
            Tile::Quiet => self.quiet,
            Tile::Dark => self.dark,
        }
    }

    pub fn note(&self, tile: Tile) -> String {
        match tile {
            Tile::Wifi => match &self.wifi {
                None => crate::lang::t("Not available").to_string(),
                Some((false, _)) => crate::lang::t("Off").to_string(),
                Some((true, name)) if name.is_empty() => crate::lang::t("Not connected").to_string(),
                Some((true, name)) => name.clone(),
            },
            Tile::Bluetooth => match self.bluetooth {
                None if !self.blue_note.is_empty() => self.blue_note.clone(),
                None => crate::lang::t("Not available").to_string(),
                Some(true) if self.blue_note.is_empty() => crate::lang::t("On").to_string(),
                Some(true) => self.blue_note.clone(),
                Some(false) => crate::lang::t("Off").to_string(),
            },
            Tile::AirDrop => match &self.airdrop {
                None => crate::lang::t("Not available").to_string(),
                Some((_, note)) => note.clone(),
            },
            Tile::Quiet | Tile::Dark => match self.on(tile) {
                true => crate::lang::t("On").to_string(),
                false => crate::lang::t("Off").to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    Tile(Tile),
    More(Tile),
    Light(f64),
    Volume(f64),
    Item(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub enum What {
    Device {
        name: String,
        note: String,
        kind: Kind,
        connected: bool,
        battery: Option<u8>,
        bars: Option<u8>,
    },
    Network {
        name: String,
        note: String,
        glyph: crate::net::Glyph,
        joined: bool,
        lock: bool,
        bars: Option<u8>,
        switch: Option<bool>,
        busy: bool,
    },
    Heading(String),
    Link(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub key: String,
    pub what: What,
}

impl Item {
    pub fn height(&self) -> f64 {
        match self.what {
            What::Device { .. } | What::Network { .. } => 46.0,
            What::Heading(_) => 28.0,
            What::Link(_) => 36.0,
        }
    }

    pub fn picks(&self) -> bool {
        !matches!(self.what, What::Heading(_))
    }

    pub fn address(&self) -> Option<&str> {
        self.key.strip_prefix("device:")
    }

    pub fn ssid(&self) -> Option<&str> {
        self.key.strip_prefix("net:")
    }

    pub fn vpn(&self) -> Option<&str> {
        self.key.strip_prefix("vpn:")
    }
}

pub const OTHER_KEY: &str = "link:wifi-other";
pub const WIFI_SETTINGS_KEY: &str = "link:wifi-settings";
pub const STOP_AIRDROP_KEY: &str = "link:stop-airdrop";
pub const NET_SERVICE_KEY: &str = "link:net-service";
pub const WIRED_KEY: &str = "wired:";
const MOST_OTHERS: usize = 8;

pub struct Seen<'a> {
    pub wifi: Result<&'a crate::net::Wifi, crate::net::Missing>,
    pub airdrop: bool,
    pub busy: &'a [String],
    pub failed: &'a [(String, String)],
    pub scanning: bool,
    pub unplugged: bool,
}

pub fn wifi_items(seen: Seen) -> Vec<Item> {
    let mut out = Vec::new();
    let heading = |key: &str, words: &str| Item {
        key: key.to_string(),
        what: What::Heading(words.to_string()),
    };
    let link = |key: &str, words: &str| Item {
        key: key.to_string(),
        what: What::Link(words.to_string()),
    };
    let wifi = match seen.wifi {
        Err(missing) => {
            out.push(heading("heading:none", missing.words()));
            if missing == crate::net::Missing::Service {
                out.push(link(NET_SERVICE_KEY, crate::lang::t("Turn On NetworkManager")));
            }
            out.push(link(WIFI_SETTINGS_KEY, crate::lang::t("Wi-Fi Settings…")));
            return out;
        }
        Ok(wifi) => wifi,
    };
    if let Some(wired) = wifi.wired.as_ref().filter(|wired| wired.connected() || wired.busy() || wired.carrier || seen.unplugged) {
        let note = match (wired.connected(), wired.busy(), wired.carrier) {
            (true, _, _) => match wifi.primary {
                crate::net::Primary::Wired => match wifi.online {
                    crate::net::Online::None | crate::net::Online::Limited => crate::lang::t("Connected, no internet"),
                    _ => crate::lang::t("Connected"),
                },
                _ => crate::lang::t("Connected"),
            },
            (false, true, _) => crate::lang::t("Connecting…"),
            (false, false, false) => crate::lang::t("Cable unplugged"),
            (false, false, true) => crate::lang::t("Not connected"),
        };
        out.push(Item {
            key: WIRED_KEY.to_string(),
            what: What::Network {
                name: crate::lang::t("Ethernet").to_string(),
                note: note.to_string(),
                glyph: crate::net::Glyph::Wired,
                joined: wired.connected(),
                lock: false,
                bars: None,
                switch: None,
                busy: wired.busy(),
            },
        });
    }
    if seen.airdrop {
        out.push(heading("heading:airdrop", crate::lang::t("AirDrop is using Wi-Fi.")));
        out.push(link(STOP_AIRDROP_KEY, crate::lang::t("Stop AirDrop")));
    } else if !wifi.managed {
        out.push(heading("heading:unmanaged", crate::lang::t("Another tool runs this Wi-Fi card.")));
    } else if !wifi.hardware {
        out.push(heading("heading:blocked", crate::lang::t("A switch or airplane mode blocks Wi-Fi.")));
    } else if !wifi.on {
        out.push(heading("heading:off", crate::lang::t("Wi-Fi is off.")));
    } else {
        let row = |net: &crate::net::Net| {
            let busy = net.busy || seen.busy.iter().any(|each| *each == net.ssid);
            let failed = seen.failed.iter().find(|(ssid, _)| *ssid == net.ssid).map(|(_, words)| words.clone());
            let note = match (busy, failed, net.active) {
                (true, _, _) => crate::lang::t("Connecting…").to_string(),
                (false, Some(words), _) => words,
                (false, None, true) => match wifi.online {
                    crate::net::Online::Portal => crate::lang::t("Sign in to use the internet").to_string(),
                    crate::net::Online::None | crate::net::Online::Limited => crate::lang::t("No internet").to_string(),
                    _ => crate::lang::t("Joined").to_string(),
                },
                (false, None, false) if net.signal < crate::net::WEAK => crate::lang::t("Weak signal").to_string(),
                (false, None, false) if net.known => crate::lang::t("Saved").to_string(),
                (false, None, false) if net.enterprise => crate::lang::t("Work or school sign-in").to_string(),
                (false, None, false) if net.secure => crate::lang::t("Password").to_string(),
                (false, None, false) => crate::lang::t("Open").to_string(),
            };
            Item {
                key: format!("net:{}", net.ssid),
                what: What::Network {
                    name: net.ssid.clone(),
                    note,
                    glyph: crate::net::Glyph::Wifi,
                    joined: net.active,
                    lock: net.secure,
                    bars: Some(crate::net::steps(net.signal)),
                    switch: None,
                    busy,
                },
            }
        };
        let known: Vec<Item> = wifi.known().map(row).collect();
        if !known.is_empty() {
            out.push(heading("heading:known", crate::lang::t("Known")));
            out.extend(known);
        }
        let others: Vec<Item> = wifi.others().take(MOST_OTHERS).map(row).collect();
        out.push(heading(
            "heading:others",
            match (others.is_empty(), seen.scanning) {
                (true, true) => crate::lang::t("Looking for networks…"),
                (true, false) => crate::lang::t("No other networks"),
                (false, _) => crate::lang::t("Other networks"),
            },
        ));
        out.extend(others);
        out.push(link(OTHER_KEY, crate::lang::t("Other…")));
    }
    if !wifi.vpns.is_empty() {
        out.push(heading("heading:vpn", "VPN"));
        for vpn in &wifi.vpns {
            out.push(Item {
                key: format!("vpn:{}", vpn.uuid),
                what: What::Network {
                    name: vpn.name.clone(),
                    note: match (vpn.busy, vpn.active) {
                        (true, _) => crate::lang::t("Connecting…").to_string(),
                        (false, true) => crate::lang::t("On").to_string(),
                        (false, false) => crate::lang::t("Off").to_string(),
                    },
                    glyph: crate::net::Glyph::Vpn,
                    joined: vpn.active,
                    lock: false,
                    bars: None,
                    switch: Some(vpn.active),
                    busy: vpn.busy || seen.busy.iter().any(|each| *each == format!("vpn:{}", vpn.uuid)),
                },
            });
        }
    }
    out.push(link(WIFI_SETTINGS_KEY, crate::lang::t("Wi-Fi Settings…")));
    out
}

pub const SETTINGS_KEY: &str = "link:settings";
pub const SERVICE_KEY: &str = "link:service";
const MOST_NEARBY: usize = 6;

pub fn items(seen: Result<&Blue, crate::blue::Missing>, busy: &[String], failed: &[String]) -> Vec<Item> {
    let mut out = Vec::new();
    let device = |device: &crate::blue::Device| {
        let busy = busy.iter().any(|each| each.eq_ignore_ascii_case(&device.address));
        let failed = failed.iter().any(|each| each.eq_ignore_ascii_case(&device.address));
        let note = match (busy, failed, device.paired, device.connected) {
            (true, _, false, _) => crate::lang::t("Setting up…").to_string(),
            (true, _, true, true) => crate::lang::t("Disconnecting…").to_string(),
            (true, _, _, _) => crate::lang::t("Connecting…").to_string(),
            (false, true, _, _) => crate::lang::t("Did not connect").to_string(),
            (false, false, true, _) => device.state().to_string(),
            (false, false, false, _) => match crate::blue::closer(device.rssi) {
                Some(hint) => format!("{}, {hint}", device.kind.label()),
                None => device.kind.label().to_string(),
            },
        };
        Item {
            key: format!("device:{}", device.address),
            what: What::Device {
                name: device.shown_name(),
                note,
                kind: device.kind,
                connected: device.connected,
                battery: device.battery.filter(|_| device.connected),
                bars: device.bars().filter(|_| !device.paired),
            },
        }
    };
    match seen {
        Err(missing) => {
            out.push(Item {
                key: "heading:none".into(),
                what: What::Heading(missing.words().into()),
            });
            if missing == crate::blue::Missing::Service {
                out.push(Item {
                    key: SERVICE_KEY.into(),
                    what: What::Link(crate::lang::t("Turn On the Bluetooth Service").into()),
                });
            }
        }
        Ok(blue) if !blue.on => out.push(Item {
            key: "heading:off".into(),
            what: What::Heading(crate::lang::t("Bluetooth is off.").into()),
        }),
        Ok(blue) => {
            let mine: Vec<Item> = blue.mine().map(device).collect();
            if !mine.is_empty() {
                out.push(Item {
                    key: "heading:mine".into(),
                    what: What::Heading(crate::lang::t("My devices").into()),
                });
                out.extend(mine);
            }
            let nearby: Vec<Item> = blue.nearby().filter(|device| device.named).take(MOST_NEARBY).map(device).collect();
            out.push(Item {
                key: "heading:nearby".into(),
                what: What::Heading(match (nearby.is_empty(), blue.searching) {
                    (true, true) => crate::lang::t("Looking for devices…").into(),
                    (true, false) => crate::lang::t("Nothing nearby").into(),
                    (false, _) => crate::lang::t("Nearby").into(),
                }),
            });
            out.extend(nearby);
        }
    }
    out.push(Item {
        key: SETTINGS_KEY.into(),
        what: What::Link(crate::lang::t("Bluetooth Settings…").into()),
    });
    out
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub width: f64,
    pub pad: f64,
    pub tile: f64,
    pub gap: f64,
    pub slider: f64,
    pub radius: f64,
    pub back: [u8; 3],
    pub back_alpha: f64,
    pub card: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub off: [u8; 3],
    pub accent: [u8; 3],
    pub airdrop: bool,
    pub listing: Option<Tile>,
}

impl Look {
    pub fn tiles(&self) -> Vec<Tile> {
        Tile::ALL.into_iter().filter(|tile| *tile != Tile::AirDrop || self.airdrop).collect()
    }

    fn tile_rows(&self) -> f64 {
        self.tiles().len().div_ceil(2) as f64
    }

    pub fn themed(p: &crate::theme::Palette, accent: [u8; 3]) -> Look {
        Look {
            width: 344.0,
            pad: 12.0,
            tile: 60.0,
            gap: 10.0,
            slider: 58.0,
            radius: 14.0,
            back: p.back,
            back_alpha: 0.96,
            card: p.card,
            ink: p.ink,
            dim: p.dim,
            off: p.off,
            accent,
            airdrop: false,
            listing: None,
        }
    }

    pub fn size(&self) -> Vec2 {
        let rows = self.tile_rows();
        let tiles = self.tile * rows + self.gap * (rows - 1.0);
        Vec2::new(self.width, self.pad * 2.0 + tiles + (self.gap + self.slider) * 2.0)
    }

    pub fn list_top(&self) -> f64 {
        self.size().y - self.pad + self.gap
    }

    pub fn size_with(&self, items: &[Item]) -> Vec2 {
        match items.is_empty() {
            true => self.size(),
            false => Vec2::new(self.width, self.list_top() + items.iter().map(Item::height).sum::<f64>() + self.pad + 8.0),
        }
    }

    pub fn list_rect(&self, items: &[Item]) -> Rect {
        let top = self.list_top();
        Rect::new(
            Vec2::new(self.pad, top),
            Vec2::new(self.width - self.pad * 2.0, self.size_with(items).y - self.pad - top),
        )
    }

    pub fn item_rect(&self, items: &[Item], index: usize) -> Rect {
        let above: f64 = items.iter().take(index).map(Item::height).sum();
        let height = items.get(index).map_or(0.0, Item::height);
        Rect::new(
            Vec2::new(self.pad, self.list_top() + 4.0 + above),
            Vec2::new(self.width - self.pad * 2.0, height),
        )
    }

    pub fn cap(&self) -> f64 {
        self.pad + self.radius
    }

    pub fn dot(&self, tile: Tile) -> Vec2 {
        let rect = self.tile_rect(tile);
        Vec2::new(rect.origin.x + 26.0, rect.center().y)
    }

    pub fn disc_at(&self, item: Rect) -> Vec2 {
        Vec2::new(item.origin.x + 25.0, item.center().y)
    }

    pub fn rect(&self, under: Rect, screen: Vec2) -> Rect {
        let size = self.size();
        let x = (under.max().x - size.x).min(screen.x - size.x - 6.0).max(6.0);
        Rect::new(Vec2::new(x, under.max().y + 8.0).round(), size)
    }

    pub fn tile_rect(&self, tile: Tile) -> Rect {
        let tiles = self.tiles();
        let index = tiles.iter().position(|held| *held == tile).unwrap_or(0);
        let wide = (self.width - self.pad * 2.0 - self.gap) / 2.0;
        let (col, row) = (index % 2, index / 2);
        let alone = index + 1 == tiles.len() && tiles.len() % 2 == 1;
        Rect::new(
            Vec2::new(
                self.pad + (wide + self.gap) * col as f64,
                self.pad + (self.tile + self.gap) * row as f64,
            ),
            Vec2::new(if alone { wide * 2.0 + self.gap } else { wide }, self.tile),
        )
    }

    pub fn slider_rect(&self, which: usize) -> Rect {
        let rows = self.tile_rows();
        let top = self.pad + self.tile * rows + self.gap * rows;
        Rect::new(
            Vec2::new(self.pad, top + (self.slider + self.gap) * which as f64),
            Vec2::new(self.width - self.pad * 2.0, self.slider),
        )
    }

    fn track(&self, which: usize) -> Rect {
        let card = self.slider_rect(which);
        Rect::new(
            Vec2::new(card.origin.x + 12.0, card.max().y - 22.0),
            Vec2::new(card.size.x - 24.0, 8.0),
        )
    }

    pub fn level_at(&self, which: usize, x: f64) -> f64 {
        let track = self.track(which);
        ((x - track.origin.x) / track.size.x.max(1.0)).clamp(0.0, 1.0)
    }

    pub fn hit_rect(&self, inside: Vec2, items: &[Item]) -> Option<Rect> {
        self.tiles()
            .into_iter()
            .map(|tile| self.tile_rect(tile))
            .chain([self.slider_rect(0), self.slider_rect(1)])
            .chain(
                items
                    .iter()
                    .enumerate()
                    .filter(|(_, item)| item.picks())
                    .map(|(index, _)| self.item_rect(items, index)),
            )
            .find(|rect| rect.contains(inside))
    }

    pub fn hit(&self, inside: Vec2, items: &[Item]) -> Option<Hit> {
        if let Some(tile) = self.tiles().into_iter().find(|tile| self.tile_rect(*tile).contains(inside)) {
            let on_dot = (inside - self.dot(tile)).length() <= 19.0;
            return Some(match (tile, on_dot) {
                (Tile::Bluetooth | Tile::Wifi, false) => Hit::More(tile),
                _ => Hit::Tile(tile),
            });
        }
        if self.slider_rect(0).contains(inside) {
            return Some(Hit::Light(self.level_at(0, inside.x)));
        }
        if self.slider_rect(1).contains(inside) {
            return Some(Hit::Volume(self.level_at(1, inside.x)));
        }
        (0..items.len())
            .find(|index| items[*index].picks() && self.item_rect(items, *index).contains(inside))
            .map(Hit::Item)
    }

    pub fn paint(&self, status: &Status, font: &Font, small: &Font) -> Image {
        self.paint_with(status, &[], false, font, small)
    }

    pub fn paint_with(&self, status: &Status, items: &[Item], bare_dots: bool, font: &Font, small: &Font) -> Image {
        let size = self.size_with(items);
        let mut canvas = Canvas::new(size.x as u32, size.y as u32);
        canvas.round_rect(Rect::new(Vec2::ZERO, size), self.radius, self.back, self.back_alpha);
        if !items.is_empty() {
            canvas.round_rect(self.list_rect(items), 12.0, self.card, 0.85);
        }
        for tile in self.tiles() {
            let rect = self.tile_rect(tile);
            canvas.round_rect(rect, 12.0, self.card, 0.85);
            let on = status.on(tile) && !bare_dots;
            let dot = self.dot(tile);
            canvas.circle(dot, 15.0, if on { self.accent } else { self.off }, 1.0);
            symbol(&mut canvas, tile, dot, if on { [255, 255, 255] } else { self.ink });
            let lists = matches!(tile, Tile::Bluetooth | Tile::Wifi);
            if lists {
                let open = !items.is_empty() && self.listing.is_none_or(|listing| listing == tile);
                chevron(&mut canvas, Vec2::new(rect.max().x - 14.0, rect.center().y), open, self.dim);
            }
            let x = rect.origin.x + 50.0;
            let spare = if lists { 16.0 } else { 0.0 };
            let room = (rect.max().x - x - 8.0 - spare).max(1.0);
            let label = font.paint_weight(&fit(font, tile.label(), room), self.ink, 600.0);
            let note = small.paint(&fit(small, &status.note(tile), room), self.dim);
            canvas.stamp(&label, Vec2::new(x, rect.center().y - crate::paint::logical(&label).y - 1.0), 1.0);
            canvas.stamp(&note, Vec2::new(x, rect.center().y + 1.0), 1.0);
        }
        for (which, (name, level)) in [
            (crate::lang::t("Display"), status.light),
            (crate::lang::t("Sound"), status.volume.map(|(level, muted)| if muted { 0.0 } else { level })),
        ]
        .into_iter()
        .enumerate()
        {
            let card = self.slider_rect(which);
            canvas.round_rect(card, 12.0, self.card, 0.85);
            let title = font.paint_weight(name, self.ink, 600.0);
            canvas.stamp(&title, card.origin + Vec2::new(12.0, 8.0), 1.0);
            let track = self.track(which);
            canvas.round_rect(track, 4.0, self.off, 1.0);
            if let Some(level) = level {
                let filled = Rect::new(track.origin, Vec2::new(track.size.x * level.clamp(0.0, 1.0), track.size.y));
                canvas.round_rect(filled, 4.0, self.ink, 1.0);
                let knob = Vec2::new(track.origin.x + track.size.x * level.clamp(0.0, 1.0), track.center().y);
                canvas.circle(knob + Vec2::new(0.0, 1.0), 9.0, [0, 0, 0], 0.25);
                canvas.circle(knob, 8.5, [255, 255, 255], 1.0);
            }
        }
        canvas.image
    }

    pub fn tile_symbol(&self, tile: Tile, ink: [u8; 3]) -> Image {
        let mut canvas = Canvas::new(30, 30);
        symbol(&mut canvas, tile, Vec2::splat(15.0), ink);
        canvas.image
    }

    pub fn disc(&self, kind: Kind, lit: bool) -> Image {
        let mut canvas = Canvas::new(32, 32);
        canvas.circle(Vec2::splat(16.0), 15.0, if lit { self.accent } else { self.off }, 1.0);
        let glyph = crate::blue::glyph(kind, crate::paint::dense(18.0), if lit { [255, 255, 255] } else { self.ink });
        canvas.stamp(&glyph, Vec2::splat(7.0), 1.0);
        canvas.image
    }

    pub fn net_disc(&self, glyph: crate::net::Glyph, lit: bool) -> Image {
        let mut canvas = Canvas::new(32, 32);
        canvas.circle(Vec2::splat(16.0), 15.0, if lit { self.accent } else { self.off }, 1.0);
        let mark = crate::net::glyph(glyph, crate::paint::dense(18.0), if lit { [255, 255, 255] } else { self.ink });
        canvas.stamp(&mark, Vec2::splat(7.0), 1.0);
        canvas.image
    }

    pub fn switch_rect(&self, item: &Item) -> Rect {
        let size = Vec2::new(self.width - self.pad * 2.0, item.height());
        Rect::new(Vec2::new(size.x - 46.0, size.y / 2.0 - 10.0), Vec2::new(34.0, 20.0))
    }

    pub fn ring_frame(&self, frame: usize, frames: usize) -> Image {
        let mut canvas = Canvas::new(40, 40);
        let from = frame as f64 / frames.max(1) as f64;
        for step in 0..8 {
            let share = 0.04;
            let at = from - f64::from(step) * share;
            canvas.arc_from(Vec2::splat(20.0), 18.0, 2.4, at, share + 0.01, self.accent, 1.0 - f64::from(step) / 8.0);
        }
        canvas.image
    }

    pub fn battery_cell(&self, item: &Item) -> Rect {
        let size = Vec2::new(self.width - self.pad * 2.0, item.height());
        Rect::new(Vec2::new(size.x - 32.0, size.y / 2.0 - 5.5), Vec2::new(20.0, 11.0))
    }

    pub fn battery_inner(&self, item: &Item) -> Rect {
        let body = self.battery_cell(item);
        let body = Rect::new(body.origin, Vec2::new(body.size.x - 2.0, body.size.y));
        Rect::new(body.origin + Vec2::splat(2.2), Vec2::new(body.size.x - 4.4, body.size.y - 4.4))
    }

    pub fn bars_rect(&self, item: &Item) -> Rect {
        let size = Vec2::new(self.width - self.pad * 2.0, item.height());
        Rect::new(Vec2::new(size.x - 30.0, size.y / 2.0 - 6.0), Vec2::new(18.0, 13.0))
    }

    pub fn battery_fill(&self, level: u8) -> Image {
        let size = self.battery_inner(&Item { key: String::new(), what: What::Heading(String::new()) }).size;
        let mut canvas = Canvas::new(size.x.ceil() as u32, size.y.ceil() as u32);
        canvas.round_rect(Rect::new(Vec2::ZERO, size), 1.4, battery_color(level, self.accent), 1.0);
        canvas.image
    }

    pub fn bars_lit(&self) -> Image {
        let mut canvas = Canvas::new(18, 13);
        bars(&mut canvas, Rect::new(Vec2::ZERO, Vec2::new(18.0, 13.0)), 4, self.ink, self.ink);
        canvas.image
    }

    pub fn paint_item(&self, item: &Item, font: &Font, small: &Font) -> Image {
        self.paint_item_with(item, false, font, small)
    }

    pub fn paint_item_with(&self, item: &Item, bare: bool, font: &Font, small: &Font) -> Image {
        let size = Vec2::new(self.width - self.pad * 2.0, item.height());
        let mut canvas = Canvas::new(size.x as u32, size.y as u32);
        match &item.what {
            What::Heading(text) => {
                let label = small.paint_weight(&fit(small, text, size.x - 24.0), self.dim, 600.0);
                canvas.stamp(&label, Vec2::new(12.0, size.y - crate::paint::logical(&label).y - 5.0), 1.0);
            }
            What::Link(text) => {
                let label = font.paint(&fit(font, text, size.x - 24.0), self.ink);
                canvas.stamp(&label, Vec2::new(12.0, (size.y - crate::paint::logical(&label).y) / 2.0), 1.0);
            }
            What::Network { name, note, lock, bars: signal, switch, .. } => {
                let mut right = size.x - 10.0;
                if let Some(on) = switch {
                    let rect = self.switch_rect(item);
                    canvas.round_rect(rect, rect.size.y / 2.0, if *on { self.accent } else { self.off }, 1.0);
                    let knob = Vec2::new(if *on { rect.max().x - 10.0 } else { rect.origin.x + 10.0 }, rect.center().y);
                    canvas.circle(knob + Vec2::new(0.0, 0.6), 8.6, [0, 0, 0], 0.18);
                    canvas.circle(knob, 8.0, [255, 255, 255], 1.0);
                    right = rect.origin.x - 8.0;
                } else if let Some(lit) = signal {
                    bars(&mut canvas, self.bars_rect(item), if bare { 0 } else { *lit }, self.ink, self.off);
                    right -= 28.0;
                }
                if *lock {
                    let mark = crate::net::lock_glyph(crate::paint::dense(12.0), self.dim);
                    let at = Vec2::new(right - 14.0, (size.y - 12.0) / 2.0);
                    canvas.stamp(&mark, at.round(), 0.9);
                    right -= 18.0;
                }
                let x = 50.0;
                let room = (right - x).max(1.0);
                let label = font.paint_weight(&fit(font, name, room), self.ink, 600.0);
                let words = small.paint(&fit(small, note, room), self.dim);
                canvas.stamp(&label, Vec2::new(x, size.y / 2.0 - crate::paint::logical(&label).y), 1.0);
                canvas.stamp(&words, Vec2::new(x, size.y / 2.0 + 1.0), 1.0);
            }
            What::Device { name, note, battery, bars: signal, .. } => {
                let mut right = size.x - 10.0;
                if let Some(level) = battery {
                    let words = small.paint(&format!("{level}%"), self.dim);
                    let width = crate::paint::logical(&words).x;
                    let cell = self.battery_cell(item);
                    battery_glyph(&mut canvas, cell, if bare { 0 } else { *level }, self.dim, self.accent);
                    canvas.stamp(&words, Vec2::new(cell.origin.x - width - 5.0, (size.y - crate::paint::logical(&words).y) / 2.0), 1.0);
                    right = cell.origin.x - width - 10.0;
                } else if let Some(lit) = signal {
                    bars(&mut canvas, self.bars_rect(item), if bare { 0 } else { *lit }, self.ink, self.off);
                    right -= 28.0;
                }
                let x = 50.0;
                let room = (right - x).max(1.0);
                let label = font.paint_weight(&fit(font, name, room), self.ink, 600.0);
                let words = small.paint(&fit(small, note, room), self.dim);
                canvas.stamp(&label, Vec2::new(x, size.y / 2.0 - crate::paint::logical(&label).y), 1.0);
                canvas.stamp(&words, Vec2::new(x, size.y / 2.0 + 1.0), 1.0);
            }
        }
        canvas.image
    }
}

fn bars(canvas: &mut Canvas, area: Rect, lit: u8, on: [u8; 3], off: [u8; 3]) {
    for step in 0..4u8 {
        let tall = 4.0 + f64::from(step) * 3.0;
        let bar = Rect::new(area.origin + Vec2::new(f64::from(step) * 5.0, area.size.y - tall), Vec2::new(3.0, tall));
        let shown = step < lit;
        canvas.round_rect(bar, 1.0, if shown { on } else { off }, if shown { 0.9 } else { 1.0 });
    }
}

pub fn bar_source(step: usize) -> Rect {
    Rect::new(Vec2::new(step as f64 * 5.0, 0.0), Vec2::new(3.0, 13.0))
}

fn battery_color(level: u8, good: [u8; 3]) -> [u8; 3] {
    match level {
        0..=10 => [255, 69, 58],
        11..=20 => [255, 159, 10],
        _ => good,
    }
}

fn chevron(canvas: &mut Canvas, at: Vec2, open: bool, ink: [u8; 3]) {
    let (a, b, c) = match open {
        false => (Vec2::new(-2.5, -5.0), Vec2::new(2.5, 0.0), Vec2::new(-2.5, 5.0)),
        true => (Vec2::new(-5.0, -2.5), Vec2::new(0.0, 2.5), Vec2::new(5.0, -2.5)),
    };
    canvas.line(at + a, at + b, 1.8, ink, 1.0);
    canvas.line(at + b, at + c, 1.8, ink, 1.0);
}

pub fn battery_glyph(canvas: &mut Canvas, cell: Rect, level: u8, ink: [u8; 3], good: [u8; 3]) {
    let body = Rect::new(cell.origin, Vec2::new(cell.size.x - 2.0, cell.size.y));
    canvas.ring(body, 3.0, 1.2, ink, 0.9);
    canvas.round_rect(
        Rect::new(Vec2::new(body.max().x + 0.5, cell.center().y - 2.0), Vec2::new(1.5, 4.0)),
        0.7,
        ink,
        0.9,
    );
    let share = f64::from(level.min(100)) / 100.0;
    let inner = Rect::new(body.origin + Vec2::splat(2.2), Vec2::new((body.size.x - 4.4) * share, body.size.y - 4.4));
    if inner.size.x > 0.3 {
        canvas.round_rect(inner, 1.4, battery_color(level, good), 1.0);
    }
}

pub fn fit(font: &Font, text: &str, room: f64) -> String {
    if font.width_of(text) <= room {
        return text.to_string();
    }
    let mut out = String::new();
    for c in text.chars() {
        if font.width_of(&format!("{out}{c}…")) > room {
            break;
        }
        out.push(c);
    }
    format!("{}…", out.trim_end())
}

fn shape(canvas: &mut Canvas, at: Vec2, ink: [u8; 3], inside: impl Fn(f64, Vec2) -> f64) {
    let dense = crate::paint::density();
    let (at, reach) = (at * dense, 10.0 * dense);
    let image = &mut canvas.image;
    for y in (at.y - reach).max(0.0) as u32..((at.y + reach) as u32).min(image.height) {
        for x in (at.x - reach).max(0.0) as u32..((at.x + reach) as u32).min(image.width) {
            let p = (Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5) - at) / dense;
            let cover = (inside(p.length(), p) * dense + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let index = ((y * image.width + x) * 4) as usize;
            for (channel, value) in ink.iter().enumerate() {
                let was = f64::from(image.rgba[index + channel]);
                image.rgba[index + channel] = (f64::from(*value) * cover + was * (1.0 - cover)).round() as u8;
            }
            let was = f64::from(image.rgba[index + 3]);
            image.rgba[index + 3] = (255.0 * cover + was * (1.0 - cover)).round() as u8;
        }
    }
}

fn symbol(canvas: &mut Canvas, tile: Tile, at: Vec2, ink: [u8; 3]) {
    match tile {
        Tile::Wifi => {
            let base = at + Vec2::new(0.0, 5.5);
            for radius in [4.6, 8.1, 11.6] {
                canvas.arc_from(base, radius, 2.1, -0.125, 0.25, ink, 1.0);
            }
            canvas.circle(base, 1.8, ink, 1.0);
        }
        Tile::Bluetooth => {
            canvas.round_rect(Rect::new(at + Vec2::new(-1.0, -8.0), Vec2::new(2.0, 16.0)), 1.0, ink, 1.0);
            canvas.triangle([at + Vec2::new(1.0, -8.0), at + Vec2::new(6.0, -3.5), at + Vec2::new(1.0, 0.5)], ink, 1.0);
            canvas.triangle([at + Vec2::new(1.0, -0.5), at + Vec2::new(6.0, 3.5), at + Vec2::new(1.0, 8.0)], ink, 1.0);
        }
        Tile::AirDrop => {
            canvas.arc_from(at, 8.5, 1.7, 0.58, 0.84, ink, 1.0);
            canvas.arc_from(at, 5.0, 1.7, 0.6, 0.8, ink, 1.0);
            canvas.circle(at, 1.9, ink, 1.0);
        }
        Tile::Quiet => shape(canvas, at, ink, |d, p| {
            let cut = (p - Vec2::new(4.0, -3.5)).length();
            (7.5 - d).min(cut - 6.0)
        }),
        Tile::Dark => shape(canvas, at, ink, |d, p| {
            let ring = 1.6 - (d - 6.8).abs();
            let half = (7.0 - d).min(p.x);
            ring.max(half)
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_rect_finds_the_tile_or_slider_under_the_pointer() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        for tile in look.tiles() {
            let rect = look.tile_rect(tile);
            assert_eq!(look.hit_rect(rect.center(), &[]), Some(rect));
        }
        let slider = look.slider_rect(1);
        assert_eq!(look.hit_rect(slider.center(), &[]), Some(slider));
        assert_eq!(look.hit_rect(Vec2::new(1.0, 1.0), &[]), None, "the padding is nothing");
    }

    #[test]
    fn control_center_at_density_two_matches_density_one() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        let font = Font::find(None, 13.0).unwrap();
        let status = Status {
            wifi: Some((true, "Home".to_string())),
            bluetooth: Some(false),
            light: Some(0.6),
            volume: Some((0.4, false)),
            ..Status::default()
        };
        crate::paint::same_at_two("control", || look.paint(&status, &font, &font));
        crate::paint::same_at_two("control icon", || paint_icon(20, [255, 255, 255]));
    }

    #[test]
    fn tiles_and_sliders_are_found_by_where_the_pointer_is() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        for tile in look.tiles() {
            assert_eq!(look.hit(look.dot(tile), &[]), Some(Hit::Tile(tile)));
        }
        let display = look.slider_rect(0);
        match look.hit(Vec2::new(display.max().x - 1.0, display.center().y), &[]) {
            Some(Hit::Light(level)) => assert!(level > 0.95),
            other => panic!("{other:?}"),
        }
        let sound = look.slider_rect(1);
        assert!(matches!(look.hit(Vec2::new(sound.origin.x + 1.0, sound.center().y), &[]), Some(Hit::Volume(_))));
        assert_eq!(look.hit(Vec2::new(1.0, 1.0), &[]), None);
        assert_eq!(look.hit(look.tile_rect(Tile::Bluetooth).center(), &[]), Some(Hit::More(Tile::Bluetooth)));
        assert_eq!(look.hit(look.tile_rect(Tile::Wifi).center(), &[]), Some(Hit::Tile(Tile::Wifi)));
        let size = look.size();
        assert!(look.slider_rect(1).max().y <= size.y);
    }

    #[test]
    fn each_tile_says_what_it_is_doing() {
        let status = Status {
            wifi: Some((true, "Home".into())),
            bluetooth: Some(false),
            quiet: true,
            ..Status::default()
        };
        assert_eq!(status.note(Tile::Wifi), "Home");
        assert!(status.on(Tile::Wifi));
        assert_eq!(status.note(Tile::Bluetooth), "Off");
        assert!(status.on(Tile::Quiet));
        assert!(!status.on(Tile::Dark));
        assert_eq!(Status::default().note(Tile::Wifi), "Not available");
    }

    fn gadget(name: &str, paired: bool, connected: bool) -> crate::blue::Device {
        crate::blue::Device {
            path: String::new(),
            address: format!("AA:BB:CC:DD:EE:{:02X}", name.len()),
            name: name.into(),
            named: true,
            kind: Kind::Mouse,
            paired,
            connected,
            trusted: paired,
            battery: Some(55),
            rssi: Some(-85),
            audio: false,
            sends: false,
            classic: false,
        }
    }

    #[test]
    fn the_list_holds_mine_then_nearby_then_settings() {
        let blue = Blue {
            on: true,
            searching: true,
            devices: vec![gadget("Mouse", true, true), gadget("Gadget", false, false)],
            ..Blue::default()
        };
        let list = items(Ok(&blue), &[], &[]);
        let keys: Vec<&str> = list.iter().map(|item| item.key.as_str()).collect();
        assert_eq!(keys, ["heading:mine", "device:AA:BB:CC:DD:EE:05", "heading:nearby", "device:AA:BB:CC:DD:EE:06", SETTINGS_KEY]);
        match &list[1].what {
            What::Device { note, battery, bars, .. } => {
                assert_eq!(note, "Connected");
                assert_eq!(*battery, Some(55));
                assert_eq!(*bars, None);
            }
            other => panic!("{other:?}"),
        }
        match &list[3].what {
            What::Device { note, bars, battery, .. } => {
                assert_eq!(note, "Mouse, Move closer");
                assert_eq!(*bars, Some(2));
                assert_eq!(*battery, None);
            }
            other => panic!("{other:?}"),
        }
        let busy = items(Ok(&blue), &["AA:BB:CC:DD:EE:06".into()], &[]);
        assert!(matches!(&busy[3].what, What::Device { note, .. } if note == "Setting up…"));
        let off = items(Ok(&Blue::default()), &[], &[]);
        assert_eq!(off.len(), 2);
        assert_eq!(items(Err(crate::blue::Missing::Adapter), &[], &[])[0].key, "heading:none");
        let service = items(Err(crate::blue::Missing::Service), &[], &[]);
        assert_eq!(service[1].key, SERVICE_KEY, "a stopped service can be started from the list");
    }

    #[test]
    fn the_list_grows_the_panel_and_its_rows_are_hit() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        let blue = Blue {
            on: true,
            devices: vec![gadget("Mouse", true, false)],
            ..Blue::default()
        };
        let list = items(Ok(&blue), &[], &[]);
        assert!(look.size_with(&list).y > look.size().y + 100.0);
        assert_eq!(look.size_with(&[]), look.size());
        let row = look.item_rect(&list, 1);
        assert_eq!(look.hit(row.center(), &list), Some(Hit::Item(1)));
        assert_eq!(look.hit(look.item_rect(&list, 0).center(), &list), None, "a heading is not a button");
        assert_eq!(look.hit_rect(row.center(), &list), Some(row));
        let last = look.item_rect(&list, list.len() - 1);
        assert!(last.max().y <= look.size_with(&list).y - look.pad);
        crate::paint::same_at_two("bluetooth row", || {
            let font = Font::find(None, 13.0).unwrap();
            look.paint_item(&list[1], &font, &font)
        });
    }

    #[test]
    fn the_airdrop_tile_only_shows_when_asked_and_the_odd_tile_spans_the_row() {
        let plain = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        assert_eq!(plain.tiles().len(), 4);
        let with = Look { airdrop: true, ..plain };
        assert_eq!(with.tiles(), Tile::ALL.to_vec());
        assert!(with.size().y > plain.size().y, "a third row of tiles");
        let last = with.tile_rect(Tile::Dark);
        assert!((last.size.x - (with.width - with.pad * 2.0)).abs() < 0.5, "the odd tile spans the row");
        assert!(with.slider_rect(0).origin.y > last.max().y);
        let status = Status { airdrop: Some((true, "Everyone, 10 min".into())), ..Status::default() };
        assert!(status.on(Tile::AirDrop));
        assert_eq!(status.note(Tile::AirDrop), "Everyone, 10 min");
        crate::paint::same_at_two("airdrop tile", || {
            let font = Font::find(None, 13.0).unwrap();
            with.paint(&status, &font, &font)
        });
    }
}
