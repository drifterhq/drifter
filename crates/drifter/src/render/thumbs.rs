use super::*;
use drifter_core::grab::{self, THUMB_BLUR};

const WIDE: f64 = 224.0;
const LINGER: Duration = Duration::from_secs(5);
const ARRIVE: f64 = 0.34;
const DEPART: f64 = 0.28;
const EDGE: f64 = 8.0;
const GAP: f64 = 12.0;
const MOST: usize = 5;
const LIFTED: f64 = 0.55;
const START_DRAG: f64 = 6.0;
const CLOSE: f64 = 22.0;
const SLOW_SAVE: Duration = Duration::from_millis(350);
const HOME: f64 = 0.26;
const CHIP: Duration = Duration::from_millis(1100);
const SWIPE_AWAY: f64 = 0.3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Kept {
    Saving,
    Saved,
    Failed,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Leave {
    Away,
    Used,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Chore {
    Copy,
    Edit,
    Open,
    Folder,
    Desktop,
    Documents,
    Keep,
    Trash,
    Close,
    Menu,
}

impl Chore {
    fn needs_file(self) -> bool {
        !matches!(self, Chore::Close | Chore::Menu)
    }

    fn id(self) -> i32 {
        match self {
            Chore::Copy => 1,
            Chore::Edit => 2,
            Chore::Open => 3,
            Chore::Folder => 4,
            Chore::Desktop => 5,
            Chore::Documents => 6,
            Chore::Keep => 7,
            Chore::Trash => 8,
            Chore::Close => 9,
            Chore::Menu => 10,
        }
    }

    fn from_id(id: i32) -> Option<Chore> {
        [
            Chore::Copy,
            Chore::Edit,
            Chore::Open,
            Chore::Folder,
            Chore::Desktop,
            Chore::Documents,
            Chore::Keep,
            Chore::Trash,
            Chore::Close,
        ]
        .into_iter()
        .find(|chore| chore.id() == id)
    }
}

pub(super) struct Thumb {
    pub id: u64,
    pub art: Option<TextureBuffer<Tex>>,
    pub size: Vec2,
    pub path: PathBuf,
    pub kept: Kept,
    pub stash: bool,
    pub hidden: bool,
    pub born: Instant,
    pub quiet: Instant,
    pub leaving: Option<(Instant, Leave)>,
    pub pressed: Option<Vec2>,
    pub swiping: bool,
    pub swipe: f64,
    pub swept: Option<Instant>,
    pub lift: f64,
    pub hover: f64,
    pub chores: Vec<Chore>,
    pub chip: Option<(TextureBuffer<Tex>, Vec2, Instant, bool)>,
}

pub(super) struct Thumbs {
    pub shown: Vec<Thumb>,
    pub next: u64,
    pub close: Option<TextureBuffer<Tex>>,
    pub wound: Instant,
    pub dragged: Option<u64>,
    pub home: Option<(u64, Vec2, Instant)>,
}

impl Default for Thumbs {
    fn default() -> Thumbs {
        Thumbs {
            shown: Vec::new(),
            next: 1,
            close: None,
            wound: Instant::now(),
            dragged: None,
            home: None,
        }
    }
}

fn card_of(rect: Rect) -> Rect {
    Rect::new(rect.origin + Vec2::splat(THUMB_BLUR), rect.size - Vec2::splat(THUMB_BLUR * 2.0))
}

fn ease(from: f64, to: f64, dt: f64, rate: f64) -> f64 {
    let next = from + (to - from) * (1.0 - (-dt * rate).exp());
    match (next - to).abs() < 0.25 {
        true => to,
        false => next,
    }
}

impl State {
    pub(super) fn show_thumb(&mut self, shot: &drifter_core::Image, path: PathBuf, stash: bool, hidden: bool) -> u64 {
        let image = grab::paint_thumb(shot, WIDE);
        let size = drifter_core::paint::logical(&image);
        let art = match hidden {
            true => None,
            false => upload(&mut self.renderer, Some(&image), "thumbnail").ok().flatten(),
        };
        let hidden = art.is_none();
        if self.thumbs.close.is_none() {
            let close = grab::paint_close(CLOSE);
            self.thumbs.close = upload(&mut self.renderer, Some(&close), "thumbnail close").ok().flatten();
        }
        let id = self.thumbs.next;
        self.thumbs.next += 1;
        let now = Instant::now();
        for thumb in &mut self.thumbs.shown {
            thumb.quiet = now;
        }
        self.thumbs.shown.push(Thumb {
            id,
            art,
            size,
            path,
            kept: Kept::Saving,
            stash,
            hidden,
            born: now,
            quiet: now,
            leaving: None,
            pressed: None,
            swiping: false,
            swipe: 0.0,
            swept: None,
            lift: 0.0,
            hover: 0.0,
            chores: Vec::new(),
            chip: None,
        });
        self.trim_thumbs();
        self.kick_now();
        id
    }

    fn thumb_floor(&self) -> f64 {
        let bottom = self.screen.y - EDGE;
        match self.dock_band() {
            Some(band) if band.max().x > self.screen.x - EDGE - WIDE - THUMB_BLUR * 2.0 => {
                bottom.min(band.origin.y - EDGE)
            }
            _ => bottom,
        }
    }

    fn thumb_ceiling(&self) -> f64 {
        self.tray_band().map_or(0.0, |band| band.max().y) + EDGE
    }

    fn thumb_targets(&self) -> Vec<f64> {
        let mut lifts = vec![0.0; self.thumbs.shown.len()];
        let mut lift = 0.0;
        for (at, thumb) in self.thumbs.shown.iter().enumerate().rev() {
            if thumb.leaving.is_some() || thumb.hidden {
                lifts[at] = thumb.lift;
                continue;
            }
            lifts[at] = lift;
            lift += thumb.size.y - THUMB_BLUR * 2.0 + GAP;
        }
        lifts
    }

    fn trim_thumbs(&mut self) {
        let room = self.thumb_floor() - self.thumb_ceiling();
        let now = Instant::now();
        let mut used = 0.0;
        let mut kept = 0;
        let dragged = self.thumbs.dragged;
        for thumb in self.thumbs.shown.iter_mut().rev() {
            if thumb.leaving.is_some() || thumb.hidden {
                continue;
            }
            used += thumb.size.y - THUMB_BLUR * 2.0 + GAP;
            kept += 1;
            let busy = thumb.pressed.is_some() || dragged == Some(thumb.id);
            if (kept > MOST || used > room + GAP) && kept > 1 && !busy {
                thumb.leaving = Some((now, Leave::Away));
            }
        }
    }

    pub(super) fn thumb_rest(&self, at: usize) -> Rect {
        let thumb = &self.thumbs.shown[at];
        let x = self.screen.x - EDGE - thumb.size.x + thumb.swipe;
        let y = self.thumb_floor() - thumb.lift - thumb.size.y;
        Rect::new(Vec2::new(x, y), thumb.size)
    }

    fn thumb_drawn(&self, at: usize) -> (Rect, f32) {
        let thumb = &self.thumbs.shown[at];
        let rest = self.thumb_rest(at);
        let arrived = drifter_core::motion::smooth((thumb.born.elapsed().as_secs_f64() / ARRIVE).min(1.0));
        let away = self.screen.x - rest.origin.x + 4.0;
        let mut origin = rest.origin + Vec2::new((1.0 - arrived) * away, 0.0);
        let mut alpha = 0.35 + 0.65 * arrived;
        if let Some((since, leave)) = thumb.leaving {
            let gone = drifter_core::motion::smooth((since.elapsed().as_secs_f64() / DEPART).min(1.0));
            match leave {
                Leave::Away => origin.x += gone * away,
                Leave::Used => alpha *= 1.0 - gone,
            }
        }
        if self.thumbs.dragged == Some(thumb.id) || self.thumbs.home.is_some_and(|(id, ..)| id == thumb.id) {
            alpha *= 0.35;
        }
        (Rect::new(origin, rest.size), alpha as f32)
    }

    pub(super) fn thumb_at(&self, point: Vec2) -> Option<usize> {
        (0..self.thumbs.shown.len()).rev().find(|&at| {
            let thumb = &self.thumbs.shown[at];
            thumb.leaving.is_none()
                && !thumb.hidden
                && self.thumbs.dragged != Some(thumb.id)
                && card_of(self.thumb_drawn(at).0).contains(point)
        })
    }

    fn close_spot(&self, at: usize) -> Vec2 {
        let card = card_of(self.thumb_drawn(at).0);
        card.origin + Vec2::splat(3.0)
    }

    fn on_close(&self, at: usize) -> bool {
        self.thumbs.shown[at].hover > 0.5 && (self.pointer - self.close_spot(at)).length() <= CLOSE / 2.0 + 2.0
    }

    pub(super) fn over_thumbs(&self) -> bool {
        self.thumbs.dragged.is_none()
            && (self.thumb_at(self.pointer).is_some()
                || self.thumbs.shown.iter().any(|thumb| thumb.pressed.is_some()))
    }

    pub(super) fn thumb_cursor(&self) -> Option<smithay::input::pointer::CursorIcon> {
        use smithay::input::pointer::CursorIcon;
        if self.thumbs.dragged.is_some() || self.thumbs.shown.iter().any(|thumb| thumb.swiping) {
            return Some(CursorIcon::Grabbing);
        }
        let at = self.thumb_at(self.pointer)?;
        Some(match self.on_close(at) {
            true => CursorIcon::Pointer,
            false => CursorIcon::Default,
        })
    }

    fn thumbs_held(&self) -> bool {
        self.thumb_at(self.pointer).is_some()
            || self.thumbs.dragged.is_some()
            || self.thumbs.home.is_some()
            || matches!(self.popover.as_ref().map(|open| &open.target), Some(Target::Shot(_)))
            || self.thumbs.shown.iter().any(|thumb| thumb.pressed.is_some() || thumb.swiping)
    }

    pub(super) fn wind_the_thumbs(&mut self) {
        let dt = self.thumbs.wound.elapsed().as_secs_f64().min(0.1);
        self.thumbs.wound = Instant::now();
        if self.thumbs.shown.is_empty() {
            return;
        }
        let dragged = self.thumbs.dragged;
        self.thumbs.shown.retain(|thumb| {
            let done = thumb.leaving.is_some_and(|(since, _)| since.elapsed().as_secs_f64() >= DEPART);
            !done || dragged == Some(thumb.id)
        });
        if let Some((_, _, since)) = self.thumbs.home
            && since.elapsed().as_secs_f64() >= HOME
        {
            self.thumbs.home = None;
        }
        self.trim_thumbs();
        let targets = self.thumb_targets();
        let hovered = self.thumb_at(self.pointer);
        let held = self.thumbs_held();
        let now = Instant::now();
        let mut ended = Vec::new();
        for (at, thumb) in self.thumbs.shown.iter_mut().enumerate() {
            thumb.lift = ease(thumb.lift, targets[at], dt, 14.0);
            let wanted = if hovered == Some(at) { 1.0 } else { 0.0 };
            thumb.hover += (wanted - thumb.hover) * (1.0 - (-dt * 16.0).exp());
            if (thumb.hover - wanted).abs() < 0.01 {
                thumb.hover = wanted;
            }
            let resting = !thumb.swiping && thumb.swept.is_none_or(|at| at.elapsed() > Duration::from_millis(140));
            if resting && thumb.leaving.is_none() {
                thumb.swipe = ease(thumb.swipe, 0.0, dt, 16.0);
                thumb.swept = None;
            }
            if let Some((_, _, since, leave)) = thumb.chip.as_ref()
                && since.elapsed() >= CHIP
            {
                if *leave && thumb.leaving.is_none() {
                    ended.push(thumb.id);
                }
                thumb.chip = None;
            }
            if held || thumb.hidden {
                thumb.quiet = now;
            } else if thumb.leaving.is_none() && thumb.quiet.elapsed() >= LINGER {
                thumb.leaving = Some((now, Leave::Away));
            }
        }
        for id in ended {
            self.thumb_leaves(id, Leave::Used);
        }
    }

    pub(super) fn thumbs_moving(&self) -> bool {
        let targets = self.thumb_targets();
        self.thumbs.home.is_some()
            || self.thumbs.dragged.is_some()
            || self.thumbs.shown.iter().enumerate().any(|(at, thumb)| {
                thumb.born.elapsed().as_secs_f64() < ARRIVE
                    || thumb.leaving.is_some()
                    || (thumb.lift - targets[at]).abs() > 0.0
                    || (thumb.hover != 0.0 && thumb.hover != 1.0)
                    || thumb.swipe != 0.0
                    || thumb.chip.is_some()
                    || (thumb.kept == Kept::Saving && (thumb.born.elapsed() >= SLOW_SAVE || !thumb.chores.is_empty()))
            })
    }

    pub(super) fn thumbs_waiting(&self) -> bool {
        !self.thumbs.shown.is_empty()
    }

    fn thumb_index(&self, id: u64) -> Option<usize> {
        self.thumbs.shown.iter().position(|thumb| thumb.id == id)
    }

    pub(super) fn thumb_leaves(&mut self, id: u64, leave: Leave) {
        if let Some(at) = self.thumb_index(id)
            && self.thumbs.shown[at].leaving.is_none()
        {
            self.thumbs.shown[at].leaving = Some((Instant::now(), leave));
            self.thumbs.shown[at].pressed = None;
        }
        self.kick_now();
    }

    pub(super) fn thumb_elements(&self, elements: &mut Vec<Skin>) {
        let lifted = |thumb: &Thumb, center: Vec2| {
            let size = thumb.size * LIFTED;
            Rect::new(center - size / 2.0, size)
        };
        if let Some(id) = self.thumbs.dragged
            && let Some(at) = self.thumb_index(id)
        {
            let thumb = &self.thumbs.shown[at];
            if let Some(art) = thumb.art.as_ref() {
                elements.push(Skin::Texture(element(art, lifted(thumb, self.pointer + Vec2::new(0.0, 6.0)), 0.92)));
            }
        }
        if let Some((id, from, since)) = self.thumbs.home
            && let Some(at) = self.thumb_index(id)
        {
            let thumb = &self.thumbs.shown[at];
            let t = drifter_core::motion::smooth((since.elapsed().as_secs_f64() / HOME).min(1.0));
            let rest = self.thumb_rest(at);
            let size = thumb.size * (LIFTED + (1.0 - LIFTED) * t);
            let center = from + (rest.center() - from) * t;
            if let Some(art) = thumb.art.as_ref() {
                elements.push(Skin::Texture(element(art, Rect::new(center - size / 2.0, size), 0.92)));
            }
        }
        for at in (0..self.thumbs.shown.len()).rev() {
            let thumb = &self.thumbs.shown[at];
            let Some(art) = thumb.art.as_ref() else {
                continue;
            };
            let (rect, alpha) = self.thumb_drawn(at);
            let card = card_of(rect);
            if thumb.hover > 0.0
                && thumb.leaving.is_none()
                && let Some(close) = self.thumbs.close.as_ref()
            {
                let side = Vec2::splat(CLOSE + 8.0);
                let spot = self.close_spot(at);
                elements.push(Skin::Texture(element(close, Rect::new(spot - side / 2.0, side), (thumb.hover as f32) * alpha)));
            }
            if let Some((chip, size, since, _)) = thumb.chip.as_ref() {
                let t = since.elapsed().as_secs_f64();
                let fade = (t / 0.12).min(1.0).min(((CHIP.as_secs_f64() - t) / 0.2).clamp(0.0, 1.0));
                let at = (card.center() - *size / 2.0).round();
                elements.push(Skin::Texture(element(chip, Rect::new(at, *size), fade as f32 * alpha)));
            }
            if thumb.kept == Kept::Saving
                && (thumb.born.elapsed() >= SLOW_SAVE || !thumb.chores.is_empty())
                && let Some((_, inks)) = self.shot_inks.as_ref()
            {
                let beat = (thumb.born.elapsed().as_secs_f64() * std::f64::consts::TAU / 1.2).sin() * 0.5 + 0.5;
                let dot = Rect::from_center(card.max() - Vec2::splat(12.0), Vec2::splat(8.0));
                elements.push(Skin::Texture(fill(&inks.accent, dot, (0.35 + 0.5 * beat) as f32 * alpha)));
            }
            elements.push(Skin::Texture(element(art, rect, alpha)));
        }
    }

    pub(super) fn press_thumb(&mut self, code: u32) -> bool {
        let Some(at) = self.thumb_at(self.pointer) else {
            return false;
        };
        let id = self.thumbs.shown[at].id;
        match code {
            BTN_LEFT if self.on_close(at) => self.thumb_leaves(id, Leave::Away),
            BTN_LEFT => self.thumbs.shown[at].pressed = Some(self.pointer),
            BTN_RIGHT => self.open_thumb_menu(id, self.pointer),
            _ => {}
        }
        self.kick_now();
        true
    }

    pub(super) fn thumb_pressed(&self) -> bool {
        self.thumbs.shown.iter().any(|thumb| thumb.pressed.is_some())
    }

    pub(super) fn move_thumb(&mut self) {
        let pointer = self.pointer;
        let Some(at) = self.thumbs.shown.iter().position(|thumb| thumb.pressed.is_some()) else {
            return;
        };
        let thumb = &mut self.thumbs.shown[at];
        let Some(from) = thumb.pressed else {
            return;
        };
        let moved = pointer - from;
        if thumb.swiping {
            thumb.swipe = match moved.x > 0.0 {
                true => moved.x,
                false => moved.x * 0.25,
            };
            self.kick_now();
            return;
        }
        if moved.length() < START_DRAG {
            return;
        }
        if moved.x > 0.0 && moved.y.abs() < moved.x * 0.6 {
            thumb.swiping = true;
            thumb.swipe = moved.x;
            self.kick_now();
            return;
        }
        let id = thumb.id;
        self.drag_thumb(id);
    }

    pub(super) fn release_thumb(&mut self) {
        let pointer = self.pointer;
        let Some(at) = self.thumbs.shown.iter().position(|thumb| thumb.pressed.is_some()) else {
            return;
        };
        let thumb = &mut self.thumbs.shown[at];
        thumb.pressed = None;
        let id = thumb.id;
        if thumb.swiping {
            thumb.swiping = false;
            if thumb.swipe > (thumb.size.x - THUMB_BLUR * 2.0) * SWIPE_AWAY {
                self.thumb_leaves(id, Leave::Away);
            }
            self.kick_now();
            return;
        }
        if card_of(self.thumb_drawn(at).0).contains(pointer) {
            self.thumb_chore(id, Chore::Edit);
        }
    }

    pub(super) fn scroll_thumb(&mut self, across: f64, stopped: bool) -> bool {
        let Some(at) = self.thumb_at(self.pointer).or_else(|| {
            self.thumbs.shown.iter().position(|thumb| thumb.swept.is_some() && thumb.leaving.is_none())
        }) else {
            return false;
        };
        let thumb = &mut self.thumbs.shown[at];
        let id = thumb.id;
        let wide = thumb.size.x - THUMB_BLUR * 2.0;
        if !stopped {
            thumb.swipe = (thumb.swipe + across).max(-12.0);
            thumb.swept = Some(Instant::now());
            if thumb.swipe > wide * 0.75 {
                self.thumb_leaves(id, Leave::Away);
            }
        } else if thumb.swipe > wide * SWIPE_AWAY {
            self.thumb_leaves(id, Leave::Away);
        } else {
            thumb.swept = None;
        }
        self.kick_now();
        true
    }

    fn drag_thumb(&mut self, id: u64) {
        use smithay::input::pointer::GrabStartData;
        use smithay::reexports::wayland_server::protocol::wl_data_device_manager::DndAction;
        use smithay::utils::SERIAL_COUNTER;
        use smithay::wayland::selection::data_device::{SourceMetadata, start_dnd};

        let Some(at) = self.thumb_index(id) else {
            return;
        };
        if self.thumbs.shown[at].kept == Kept::Failed {
            return;
        }
        self.thumbs.shown[at].pressed = None;
        let path = self.thumbs.shown[at].path.clone();
        self.thumbs.dragged = Some(id);
        self.thumbs.home = None;
        self.drag_shot = Some(path.clone());
        let world = self.in_world(self.pointer);
        let metadata = SourceMetadata {
            mime_types: shot_kinds(&path),
            dnd_action: DndAction::Copy | DndAction::Move,
        };
        let start = GrabStartData {
            focus: None,
            button: BTN_LEFT,
            location: (world.x, world.y).into(),
        };
        let dh = self.wayland.handle.clone();
        let seat = self.wayland._seat.clone();
        start_dnd(&dh, &seat, self, SERIAL_COUNTER.next_serial(), Some(start), None, metadata);
        self.tell_pointer(self.clock_ms());
        self.kick_now();
    }

    pub fn shot_dropped(&mut self, landed: bool) {
        self.drag_shot = None;
        if let Some(id) = self.thumbs.dragged.take() {
            match landed {
                true => self.thumb_leaves(id, Leave::Used),
                false => self.thumbs.home = Some((id, self.pointer + Vec2::new(0.0, 6.0), Instant::now())),
            }
        }
        self.tell_pointer(self.clock_ms());
        self.kick_now();
    }

    pub fn shot_drag_action(
        &self,
        available: smithay::reexports::wayland_server::protocol::wl_data_device_manager::DndAction,
    ) -> Option<smithay::reexports::wayland_server::protocol::wl_data_device_manager::DndAction> {
        use smithay::reexports::wayland_server::protocol::wl_data_device_manager::DndAction;
        self.drag_shot.as_ref()?;
        let copy = self.wayland.keyboard.modifier_state().ctrl;
        match (copy, available.contains(DndAction::Copy), available.contains(DndAction::Move)) {
            (true, true, _) => Some(DndAction::Copy),
            (_, _, true) => Some(DndAction::Move),
            (_, true, false) => Some(DndAction::Copy),
            _ => Some(DndAction::empty()),
        }
    }

    fn open_thumb_menu(&mut self, id: u64, at: Vec2) {
        use drifter_core::lang::t;
        use drifter_core::menu::Item;
        let Some(index) = self.thumb_index(id) else {
            return;
        };
        let thumb = &self.thumbs.shown[index];
        let usable = thumb.kept != Kept::Failed;
        let item = |chore: Chore, label: &str| Item {
            enabled: usable,
            ..Item::line(chore.id(), label)
        };
        let mut items = vec![
            item(Chore::Copy, t("Copy")),
            item(Chore::Edit, t("Markup")),
            Item::rule(90),
        ];
        if thumb.stash {
            items.push(item(Chore::Keep, t("Save to Screenshots")));
        }
        items.push(item(Chore::Desktop, t("Save to Desktop")));
        items.push(item(Chore::Documents, t("Save to Documents")));
        items.push(Item::rule(91));
        if !thumb.stash {
            items.push(item(Chore::Folder, t("Show in Folder")));
        }
        items.push(item(Chore::Open, t("Open")));
        items.push(Item::rule(92));
        items.push(Item::line(Chore::Close.id(), t("Close")));
        items.push(item(Chore::Trash, t("Move to Trash")));
        self.open_popover(items, at, Target::Shot(id));
    }

    pub(super) fn shot_menu_open(&self) -> bool {
        matches!(self.popover.as_ref().map(|open| &open.target), Some(Target::Shot(_)))
    }

    pub(super) fn press_shot_menu(&mut self) -> Option<bool> {
        if !self.shot_menu_open() {
            return None;
        }
        if self.popover.as_ref().is_some_and(|open| open.rect.contains(self.pointer)) {
            return Some(self.press_popover());
        }
        self.popover = None;
        self.kick_now();
        None
    }

    pub(super) fn shot_menu_pick(&mut self, id: u64, pick: i32) {
        self.popover = None;
        if let Some(chore) = Chore::from_id(pick) {
            self.thumb_chore(id, chore);
        }
        self.kick_now();
    }

    pub(super) fn thumb_key(&self, names: &[String], ctrl: bool) -> Option<Chore> {
        self.thumb_at(self.pointer)?;
        let named = |word: &str| names.iter().any(|name| name.eq_ignore_ascii_case(word));
        match ctrl {
            true if named("c") => Some(Chore::Copy),
            true => None,
            false if named("Escape") => Some(Chore::Close),
            false if named("Return") || named("KP_Enter") => Some(Chore::Edit),
            false if named("space") => Some(Chore::Open),
            false if named("Delete") || named("BackSpace") => Some(Chore::Trash),
            false if named("Menu") => Some(Chore::Menu),
            false => None,
        }
    }

    pub(super) fn thumb_key_chore(&mut self, chore: Chore) {
        let Some(at) = self.thumb_at(self.pointer) else {
            return;
        };
        let id = self.thumbs.shown[at].id;
        match chore {
            Chore::Menu => {
                let spot = card_of(self.thumb_drawn(at).0).center();
                self.open_thumb_menu(id, spot);
            }
            chore => self.thumb_chore(id, chore),
        }
    }

    pub(super) fn thumb_chore(&mut self, id: u64, chore: Chore) {
        let Some(at) = self.thumb_index(id) else {
            return;
        };
        match (chore.needs_file(), self.thumbs.shown[at].kept) {
            (true, Kept::Failed) => return,
            (true, Kept::Saving) => {
                let thumb = &mut self.thumbs.shown[at];
                if !thumb.chores.contains(&chore) {
                    thumb.chores.push(chore);
                }
                self.kick_now();
                return;
            }
            _ => {}
        }
        let path = self.thumbs.shown[at].path.clone();
        match chore {
            Chore::Copy => {
                self.copy_shot(&path);
                self.thumb_chip(id, drifter_core::lang::t("Copied"), false);
            }
            Chore::Edit => {
                self.edit_shot(&path);
                self.thumb_leaves(id, Leave::Used);
            }
            Chore::Open => {
                let role = drifter_core::desktop::Role::Images;
                crate::apps::open_as(&path.display().to_string(), role, self.apps_chosen.get(role), &self.socket);
                self.thumb_leaves(id, Leave::Used);
            }
            Chore::Folder => {
                self.show_in_files(crate::filemanager::Show::Items(vec![path.clone()]));
                self.thumb_leaves(id, Leave::Used);
            }
            Chore::Desktop => self.move_shot(id, &path, Some(("DESKTOP", "Desktop"))),
            Chore::Documents => self.move_shot(id, &path, Some(("DOCUMENTS", "Documents"))),
            Chore::Keep => self.move_shot(id, &path, None),
            Chore::Trash => {
                if let Err(error) = drifter_core::explorer::ops::trash(&path) {
                    self.shot_note(drifter_core::lang::t("Cannot move to Trash"), error.to_string(), None);
                }
                self.thumb_leaves(id, Leave::Used);
            }
            Chore::Close => self.thumb_leaves(id, Leave::Away),
            Chore::Menu => {}
        }
    }

    fn thumb_chip(&mut self, id: u64, words: &str, then_leave: bool) {
        let Some(font) = self.font.clone() else {
            if then_leave {
                self.thumb_leaves(id, Leave::Used);
            }
            return;
        };
        let image = grab::paint_chip(words, &font);
        let size = drifter_core::paint::logical(&image);
        let Ok(Some(art)) = upload(&mut self.renderer, Some(&image), "thumbnail chip") else {
            return;
        };
        if let Some(at) = self.thumb_index(id) {
            self.thumbs.shown[at].chip = Some((art, size, Instant::now(), then_leave));
        }
        self.kick_now();
    }

    fn copy_shot(&mut self, path: &std::path::Path) {
        let Ok(picture) = std::fs::read(path) else {
            return;
        };
        let files = vec![path.to_path_buf()];
        let held = drifter_core::explorer::clip::Held::Files(files.clone(), false);
        let mut kinds = vec![(shot_mime(path).to_string(), picture)];
        for kind in [drifter_core::explorer::clip::URIS, drifter_core::explorer::clip::FILES] {
            if let Some(bytes) = held.bytes(kind) {
                kinds.push((kind.to_string(), bytes));
            }
        }
        let mimes = kinds.iter().map(|(mime, _)| mime.clone()).collect();
        let dh = self.wayland.handle.clone();
        let seat = self.wayland._seat.clone();
        smithay::wayland::selection::data_device::set_data_device_selection(&dh, &seat, mimes, std::sync::Arc::new(kinds));
    }

    fn edit_shot(&mut self, path: &std::path::Path) {
        let markup = std::env::current_exe()
            .map(|exe| exe.with_file_name(MARKUP_APP))
            .ok()
            .filter(|exe| exe.is_file());
        let argv = match markup {
            Some(exe) => vec![exe.to_string_lossy().into_owned(), path.display().to_string()],
            None => vec!["xdg-open".to_string(), path.display().to_string()],
        };
        crate::apps::launch(&argv, &self.socket);
    }

    fn move_shot(&mut self, id: u64, path: &std::path::Path, to: Option<(&str, &str)>) {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        let folder = match to {
            Some((key, fallback)) => drifter_core::folders::user_dir(&home, key, fallback),
            None => self.shots.folder_in(&home),
        };
        let moved = std::fs::create_dir_all(&folder)
            .and_then(|()| drifter_core::explorer::ops::move_into(path, &folder))
            .map_err(|error| error.to_string());
        match moved {
            Ok(moved) => {
                let name = folder.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
                if let Some(at) = self.thumb_index(id) {
                    self.thumbs.shown[at].path = moved;
                    self.thumbs.shown[at].stash = false;
                }
                self.thumb_chip(id, &drifter_core::lang::tf("Saved to {}", &[&name]), true);
            }
            Err(error) => {
                self.shot_note(drifter_core::lang::t("No screenshot"), error, None);
                self.thumb_leaves(id, Leave::Used);
            }
        }
    }

    pub(super) fn shot_note(&mut self, summary: &str, body: String, folder: Option<PathBuf>) {
        let Some(id) = self.notes_bus.as_ref().map(|bus| bus.next_id()) else {
            return;
        };
        if let Some(folder) = folder {
            self.note_folders.insert(id, folder);
        }
        self.show_note(drifter_core::notify::Note {
            id,
            app: drifter_core::lang::t("Drifter").to_string(),
            summary: summary.to_string(),
            body,
            actions: Vec::new(),
            expire: Some(drifter_core::notify::DEFAULT_MS),
            asked: -1,
            critical: false,
            icon: None,
        });
    }

    pub(super) fn shot_saved(&mut self, id: u64, saved: Result<PathBuf, String>) {
        let Some(at) = self.thumb_index(id) else {
            if let Err(error) = saved {
                self.shot_note(drifter_core::lang::t("No screenshot"), error, None);
            }
            return;
        };
        match saved {
            Ok(path) => {
                self.thumbs.shown[at].kept = Kept::Saved;
                let chores = std::mem::take(&mut self.thumbs.shown[at].chores);
                for chore in chores {
                    self.thumb_chore(id, chore);
                }
                if let Some(at) = self.thumb_index(id)
                    && self.thumbs.shown[at].hidden
                {
                    let thumb = self.thumbs.shown.remove(at);
                    match thumb.stash {
                        true => self.shot_note(drifter_core::lang::t("Screenshot copied"), drifter_core::lang::t("Paste it anywhere").to_string(), None),
                        false => {
                            let folder = path.parent().map(PathBuf::from);
                            self.shot_note(drifter_core::lang::t("Screenshot saved"), path.display().to_string(), folder);
                        }
                    }
                }
            }
            Err(error) => {
                self.thumbs.shown[at].kept = Kept::Failed;
                self.thumbs.shown[at].chores.clear();
                self.thumb_leaves(id, Leave::Used);
                self.shot_note(drifter_core::lang::t("No screenshot"), error, None);
            }
        }
        self.kick_now();
    }
}

pub(super) fn shot_mime(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|ext| ext.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("jpg") | Some("jpeg") => "image/jpeg",
        _ => "image/png",
    }
}

fn shot_kinds(path: &std::path::Path) -> Vec<String> {
    vec![
        drifter_core::explorer::clip::URIS.to_string(),
        drifter_core::explorer::clip::FILES.to_string(),
        shot_mime(path).to_string(),
    ]
}

pub fn send_shot(path: &std::path::Path, mime: &str, fd: std::os::fd::OwnedFd) {
    use std::io::Write as _;
    let path = path.to_path_buf();
    let mime = mime.to_string();
    std::thread::spawn(move || {
        let waited = Instant::now();
        while !path.exists() && waited.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(25));
        }
        let held = drifter_core::explorer::clip::Held::Files(vec![path.clone()], false);
        let bytes = match mime.starts_with("image/") {
            true => std::fs::read(&path).unwrap_or_default(),
            false => held.bytes(&mime).unwrap_or_default(),
        };
        let _ = std::fs::File::from(fd).write_all(&bytes);
    });
}
