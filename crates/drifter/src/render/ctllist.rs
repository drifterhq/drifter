use super::*;
use drifter_core::control::{Item, Tile, What};

pub(super) const LIST_OPEN: f64 = 0.38;
pub(super) const LIST_CLOSE: f64 = 0.2;
const FILL_IN: f64 = 0.34;
const FILL_OUT: f64 = 0.18;
const ROW_IN: f64 = 0.36;
const ROW_STAGGER: f64 = 0.03;
const ROW_RISE: f64 = 10.0;
const ROW_MOVE: f64 = 0.44;
pub(super) const ROW_OUT: f64 = 0.22;
pub(super) const SHAKE: f64 = 0.5;
const SHAKE_REACH: f64 = 9.0;
pub(super) const POP: f64 = 0.42;
const POP_FROM: f64 = 0.55;
const TURN: f64 = 0.9;
const BREATH: f64 = 1.6;
pub(super) const RING_FRAMES: usize = 24;
pub(super) const LEVEL: f64 = 0.36;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Owner {
    Wifi,
    Bluetooth,
}

impl Owner {
    pub(super) fn of(tile: Tile) -> Option<Owner> {
        match tile {
            Tile::Wifi => Some(Owner::Wifi),
            Tile::Bluetooth => Some(Owner::Bluetooth),
            _ => None,
        }
    }

    pub(super) fn tile(self) -> Tile {
        match self {
            Owner::Wifi => Tile::Wifi,
            Owner::Bluetooth => Tile::Bluetooth,
        }
    }
}

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
    pub owner: Owner,
    pub items: Vec<Item>,
    pub rows: Vec<Row>,
    pub lane: worker::Lane,
    pub closing: bool,
    pub epoch: Instant,
}

fn item_numbers(item: &Item) -> (f64, f64) {
    match item.what {
        What::Device { battery, bars, .. } => (battery.map_or(0.0, f64::from), bars.map_or(0.0, f64::from)),
        What::Network { bars, .. } => (0.0, bars.map_or(0.0, f64::from)),
        _ => (0.0, 0.0),
    }
}

fn joined(item: &Item) -> bool {
    match item.what {
        What::Device { connected, .. } => connected,
        What::Network { joined, .. } => joined,
        _ => false,
    }
}

pub(super) fn eased((from, to, since): (f64, f64, Instant), pace: f64, still: bool) -> f64 {
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

pub(super) fn tile_index(tile: Tile) -> u8 {
    Tile::ALL.iter().position(|each| *each == tile).unwrap_or(0) as u8
}

fn fresh_row(item: &Item, y: f64, born: Instant, now: Instant) -> Row {
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
}

impl State {
    pub(super) fn owner_items(&self, owner: Owner) -> Vec<Item> {
        match owner {
            Owner::Bluetooth => self.blue_items(),
            Owner::Wifi => self.wifi_items(),
        }
    }

    pub(super) fn listing(&self) -> Option<Owner> {
        self.control_list.as_ref().filter(|list| !list.closing).map(|list| list.owner)
    }

    pub(super) fn shown_items(&self) -> &[Item] {
        match self.control_list.as_ref() {
            Some(list) if !list.closing => &list.items,
            _ => &[],
        }
    }

    pub(super) fn painted_items(&self) -> Vec<Item> {
        match self.control_list.as_ref() {
            Some(list) if !list.closing => list.items.clone(),
            Some(list) if self.control.as_ref().is_some_and(|open| open.drawn_now(self) > self.control_look().size().y + 0.5) => list.items.clone(),
            _ => Vec::new(),
        }
    }

    pub(super) fn list_toggle(&mut self, owner: Owner) {
        let pace = self.window_pace.max(0.01);
        let now = Instant::now();
        let open = self.listing();
        match open {
            Some(was) if was == owner => {
                if let Some(list) = self.control_list.as_mut() {
                    list.closing = true;
                }
                let base = self.control_look().size().y;
                self.control_retarget(base);
            }
            Some(_) => {
                self.blue_art();
                self.wifi_art();
                let items = self.owner_items(owner);
                let look = self.control_look_for(Some(owner));
                let delay = ROW_OUT * 0.6;
                if let Some(list) = self.control_list.as_mut() {
                    for row in list.rows.iter_mut().filter(|row| row.gone.is_none()) {
                        row.gone = Some(now);
                    }
                    for (index, item) in items.iter().enumerate() {
                        let y = look.item_rect(&items, index).origin.y;
                        let born = now + Duration::from_secs_f64((delay + ROW_STAGGER * index as f64) / pace);
                        list.rows.retain(|row| row.key != item.key);
                        list.rows.push(fresh_row(item, y, born, now));
                    }
                    list.owner = owner;
                    list.items = items;
                    list.epoch = now;
                }
                self.paint_rows();
                let size = look.size_with(self.shown_items());
                self.control_retarget(size.y);
            }
            None => {
                self.blue_art();
                self.wifi_art();
                let items = self.owner_items(owner);
                let look = self.control_look_for(Some(owner));
                let rows = items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        let y = look.item_rect(&items, index).origin.y;
                        let born = now + Duration::from_secs_f64(ROW_STAGGER * index as f64 / pace);
                        fresh_row(item, y, born, now)
                    })
                    .collect();
                self.control_list = Some(List {
                    owner,
                    items,
                    rows,
                    lane: worker::Lane::default(),
                    closing: false,
                    epoch: now,
                });
                self.paint_rows();
                let size = look.size_with(self.shown_items());
                self.control_retarget(size.y);
            }
        }
        self.blue_search_as_needed();
        self.wifi_scan_as_needed();
        self.paint_control();
    }

    pub(super) fn close_list(&mut self) {
        self.control_list = None;
        self.blue_search_as_needed();
        self.wifi_scan_as_needed();
    }

    pub(super) fn blue_list_refresh(&mut self) {
        self.list_refresh();
    }

    pub(super) fn list_refresh(&mut self) {
        if self.control.is_none() {
            return;
        }
        let Some(owner) = self.listing() else {
            return;
        };
        let items = self.owner_items(owner);
        let look = self.control_look();
        let now = Instant::now();
        let pace = self.window_pace.max(0.01);
        let Some(list) = self.control_list.as_mut() else {
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
                    let jumped = !joined(&row.item) && joined(item);
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
                    list.rows.push(fresh_row(item, y, born, now));
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
        self.hover_rows();
        if resized {
            let size = look.size_with(self.shown_items());
            self.control_retarget(size.y);
            self.paint_control();
        }
    }

    pub(super) fn list_wind(&mut self) {
        let now = Instant::now();
        let pace = self.window_pace.max(0.01);
        if let Some(list) = self.control_list.as_mut() {
            list.rows
                .retain(|row| row.gone.is_none_or(|gone| now.duration_since(gone).as_secs_f64() < ROW_OUT / pace));
        }
        let closed = self.control_list.as_ref().is_some_and(|list| list.closing)
            && self.control.as_ref().is_none_or(|open| !open.growing(self));
        if closed {
            self.control_list = None;
            self.paint_control();
        }
        if let Some(owner) = self.listing() {
            let stale = self.owner_items(owner);
            if self.control_list.as_ref().is_some_and(|list| list.items != stale) {
                self.list_refresh();
            }
        }
    }

    fn searching_now(&self, owner: Owner) -> bool {
        match owner {
            Owner::Bluetooth => self.radio.now.as_ref().is_some_and(|blue| blue.searching),
            Owner::Wifi => self.wifi.scanning,
        }
    }

    pub(super) fn list_moving(&self) -> bool {
        let pace = self.window_pace;
        if pace <= 0.0 || self.control.is_none() {
            return false;
        }
        let now = Instant::now();
        let Some(list) = self.control_list.as_ref() else {
            return false;
        };
        if list.closing {
            return true;
        }
        self.searching_now(list.owner)
            || list.rows.iter().any(|row| {
                row.gone.is_some()
                    || now < row.born
                    || now.duration_since(row.born).as_secs_f64() < ROW_IN / pace
                    || now.duration_since(row.moved).as_secs_f64() < ROW_MOVE / pace
                    || row.level.2.elapsed().as_secs_f64() < LEVEL / pace
                    || row.signal.2.elapsed().as_secs_f64() < LEVEL / pace
            })
            || self.list_busy()
    }

    fn list_busy(&self) -> bool {
        self.shown_items().iter().any(|item| self.row_busy(item))
    }

    fn row_busy(&self, item: &Item) -> bool {
        match &item.what {
            What::Device { .. } => item.address().is_some_and(|address| self.radio.busy.iter().any(|each| each == address)),
            What::Network { busy, .. } => *busy,
            _ => false,
        }
    }

    pub(super) fn paint_rows(&mut self) {
        let (Some(font), Some(small)) = (self.note_body.clone(), self.card_font.clone()) else {
            return;
        };
        let owner = self.control_list.as_ref().map(|list| list.owner);
        let look = self.control_look_for(owner);
        let Some(list) = self.control_list.as_mut() else {
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
        self.on_worker(
            "control rows",
            move || {
                wanted
                    .into_iter()
                    .map(|item| {
                        let image = look.paint_item_with(&item, true, &font, &small);
                        (item, image)
                    })
                    .collect::<Vec<_>>()
            },
            move |state, painted| {
                let Some(list) = state.control_list.as_mut().filter(|list| list.lane.id() == id) else {
                    return;
                };
                let again = list.lane.release();
                for (item, image) in painted {
                    let Ok(Some(tex)) = upload(&mut state.renderer, Some(&image), "control row") else {
                        continue;
                    };
                    if let Some(row) = state.control_list.as_mut().and_then(|list| list.rows.iter_mut().find(|row| row.key == item.key)) {
                        row.art = Some(tex);
                        row.painted = Some(item);
                    }
                }
                if again || state.control_list.as_ref().is_some_and(|list| list.rows.iter().any(|row| row.painted.as_ref() != Some(&row.item))) {
                    state.paint_rows();
                }
                state.kick_now();
            },
        );
    }

    pub(super) fn fill_tile(&mut self, tile: Tile) {
        let on = self.control_status.on(tile);
        if let Some(open) = self.control.as_mut() {
            let slot = &mut open.fills[tile_index(tile) as usize];
            if slot.0 != on {
                *slot = (on, Instant::now());
            }
        }
    }

    fn row_marks(&self, row: &Row) -> (Option<&TextureBuffer<Tex>>, bool, Option<Instant>, Option<Instant>, Option<u8>, Option<u8>) {
        match &row.item.what {
            What::Device { kind, connected, battery, bars, .. } => {
                let address = row.item.address().unwrap_or_default();
                (
                    self.radio.art.discs.get(&(*kind, *connected)),
                    self.radio.busy.iter().any(|each| each == address),
                    self.radio.shakes.get(address).copied(),
                    self.radio.pops.get(address).copied(),
                    *battery,
                    *bars,
                )
            }
            What::Network { glyph, joined, bars, .. } => (
                self.wifi.art.discs.get(&(*glyph, *joined)),
                self.row_busy(&row.item),
                self.wifi.shakes.get(&row.key).copied(),
                self.wifi.pops.get(&row.key).copied(),
                None,
                *bars,
            ),
            _ => (None, false, None, None, None, None),
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

        if let Some(list) = self.control_list.as_ref() {
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
                    let breathing = matches!(row.key.as_str(), "heading:nearby" | "heading:others") && row.gone.is_none() && self.searching_now(list.owner);
                    if breathing {
                        let wave = (list.epoch.elapsed().as_secs_f64() / BREATH * std::f64::consts::TAU).cos();
                        alpha *= 0.62 + 0.38 * (0.5 + 0.5 * wave);
                    }
                } else if row.gone.is_some() {
                    continue;
                }
                let (disc, busy, shake_at, pop_at, battery, bars) = self.row_marks(row);
                let shake = shake_at.filter(|_| !still).map_or(0.0, |at| {
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
                if matches!(row.item.what, What::Device { .. } | What::Network { .. }) {
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
                        if busy
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
                        if let Some(disc) = disc {
                            let pop = pop_at.filter(|_| !still).map_or(1.0, |at| {
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
        match self.listing() {
            Some(Owner::Wifi) => self.press_wifi_item(&item),
            _ => self.press_blue_item(&item),
        }
    }
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
