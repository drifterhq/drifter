use super::*;

const CARD_IN: f64 = 0.28;
const CARD_GLIDE: f64 = 0.22;
const RING_SLIDE: f64 = 0.14;

impl State {
    pub(super) fn switch_step(&mut self, back: bool) {
        if self.lock.is_some() {
            return;
        }
        let at = match self.switch.as_ref() {
            Some(open) => drifter_core::switcher::step(open.at, open.order.len(), back),
            None => {
                let order: Vec<u64> = self
                    .windows
                    .iter()
                    .rev()
                    .filter(|window| window.placed && !window.going)
                    .map(|window| window.id)
                    .collect();
                if order.is_empty() {
                    return;
                }
                let at = match order.len() {
                    1 => 0,
                    count => drifter_core::switcher::step(0, count, back),
                };
                let layout =
                    drifter_core::switcher::Look::themed(&self.palette).layout(self.screen, order.len());
                let tile = layout.tile_rect(at);
                self.switch = Some(Switch {
                    icons: vec![None; order.len()],
                    order,
                    at,
                    ring: (Rect::new(layout.panel.origin + tile.origin, tile.size), Instant::now() - Duration::from_secs(1)),
                    layout,
                    art: None,
                    lane: worker::Lane::default(),
                });
                self.refresh_switch_icons();
                at
            }
        };
        let drawn = self.switch_ring().map(|(rect, _)| rect);
        if let Some(open) = self.switch.as_mut() {
            if at != open.at {
                let tile = open.layout.tile_rect(at);
                let to = Rect::new(open.layout.panel.origin + tile.origin, tile.size);
                let jump = (at as isize - open.at as isize).abs() > 1;
                open.ring = (if jump { to } else { drawn.unwrap_or(to) }, Instant::now());
            }
            open.at = at;
        }
        self.paint_switch();
    }

    pub(super) fn switch_ring(&self) -> Option<(Rect, f32)> {
        let open = self.switch.as_ref()?;
        let tile = open.layout.tile_rect(open.at);
        let to = Rect::new(open.layout.panel.origin + tile.origin, tile.size);
        let pace = self.window_pace;
        let (from, since) = open.ring;
        if pace <= 0.0 {
            return Some((to, 1.0));
        }
        let t = drifter_core::motion::smooth(since.elapsed().as_secs_f64() / (RING_SLIDE / pace));
        match self.window_still {
            true => Some((to, t as f32)),
            false => Some((drifter_core::motion::between(from, to, t), 1.0)),
        }
    }

    pub(super) fn switch_moving(&self) -> bool {
        let pace = self.window_pace;
        pace > 0.0
            && self
                .switch
                .as_ref()
                .is_some_and(|open| open.ring.1.elapsed().as_secs_f64() < RING_SLIDE / pace)
    }

    pub(super) fn refresh_switch_icons(&mut self) {
        let Some(open) = self.switch.as_ref() else {
            return;
        };
        let side = open.layout.icon.round() as u32;
        let apps: Vec<String> = open
            .order
            .iter()
            .map(|id| {
                self.windows
                    .iter()
                    .find(|window| window.id == *id)
                    .map(|window| window.app.clone())
                    .unwrap_or_default()
            })
            .collect();
        let icons: Vec<Option<TextureBuffer<Tex>>> = apps.iter().map(|app| self.app_icon_now(app, side)).collect();
        let changed = self
            .switch
            .as_ref()
            .is_some_and(|open| open.icons.iter().zip(&icons).any(|(a, b)| a.is_some() != b.is_some()));
        if let Some(open) = self.switch.as_mut() {
            open.icons = icons;
        }
        if changed {
            self.paint_switch();
        }
    }

    pub(super) fn paint_switch(&mut self) {
        let Some(open) = self.switch.as_ref() else {
            return;
        };
        let windows: Vec<&crate::shell::Window> = open
            .order
            .iter()
            .filter_map(|id| self.windows.iter().find(|window| window.id == *id))
            .collect();
        let letters: Vec<Option<char>> = windows
            .iter()
            .zip(&open.icons)
            .map(|(window, icon)| match icon.is_some() {
                true => None,
                false => window
                    .app
                    .rsplit('.')
                    .next()
                    .and_then(|name| name.chars().next())
                    .or_else(|| window.title.chars().next())
                    .or(Some('?')),
            })
            .collect();
        let title = windows
            .get(open.at)
            .map(|window| match window.title.is_empty() {
                true => window.app.clone(),
                false => window.title.clone(),
            })
            .unwrap_or_default();
        let layout = open.layout.clone();
        let font = self.font.clone();
        self.paint_in_lane(
            "switcher",
            |state| state.switch.as_mut().map(|open| &mut open.lane),
            State::paint_switch,
            move || Some(layout.paint(None, &title, &letters, font.as_deref())),
            |state, art| {
                if let Some(open) = state.switch.as_mut() {
                    open.art = Some(art);
                }
            },
        );
    }

    pub(super) fn show_hud(&mut self, reading: drifter_core::hud::Reading) {
        self.hud_want = Some(reading);
        let look = drifter_core::hud::Look::themed(&self.palette);
        let font = self.font.clone();
        let rect = look.rect(self.screen);
        self.paint_in_lane(
            "hud",
            |state| Some(&mut state.hud_lane),
            |state| {
                if let Some(reading) = state.hud_want {
                    state.show_hud(reading);
                }
            },
            move || Some(look.paint_frame(reading, font.as_deref())),
            move |state, art| {
                let now = Instant::now();
                let to = reading.shown();
                let from = state.hud_level().unwrap_or(to);
                state.hud = Some(super::Hud {
                    art,
                    rect,
                    until: now + HUD_TIME,
                    from,
                    to,
                    since: now,
                    icon: None,
                });
            },
        );
    }

    pub(super) fn prompt_begin(&mut self, request: crate::polkit::Request) {
        if self.font.is_none() {
            log!("drifter: no font, so no password prompt can be shown");
            let _ = request.reply.try_send(false);
            return;
        }
        self.prompts.push(Pending {
            asker: Asker::Polkit(request),
            pad: Pad::new(),
            error: None,
            busy: false,
            shaken: None,
        });
        self.paint_prompt();
    }

    pub(super) fn wifi_prompt(&mut self, ask: drifter_net::agent::Ask, reply: drifter_net::agent::Reply) {
        if self.font.is_none() || self.lock.is_some() {
            drifter_net::agent::answer(&reply, None);
            return;
        }
        let again = ask.again;
        let pending = Pending {
            asker: Asker::Wifi(ask.clone(), reply),
            pad: Pad::new(),
            error: again.then(|| drifter_core::lang::t("Wrong password. Try again.").to_string()),
            busy: false,
            shaken: again.then(Instant::now),
        };
        let same = self
            .prompts
            .iter()
            .position(|held| matches!(&held.asker, Asker::Wifi(held, _) if held.ssid == ask.ssid));
        match same {
            Some(at) => {
                if let Asker::Wifi(_, old) = &self.prompts[at].asker {
                    drifter_net::agent::answer(old, None);
                }
                self.prompts[at] = pending;
            }
            None => self.prompts.push(pending),
        }
        self.paint_prompt();
        self.kick_now();
    }

    pub(super) fn wifi_prompt_done(&mut self, id: u64) {
        let cookie = format!("wifi:{id}");
        if self.prompts.iter().any(|pending| pending.cookie() == cookie) {
            self.prompt_end(&cookie, false);
        }
    }

    pub(super) fn prompt_end(&mut self, cookie: &str, granted: bool) {
        if let Some(at) = self.prompts.iter().position(|pending| pending.cookie() == cookie) {
            let mut pending = self.prompts.remove(at);
            pending.pad.wipe();
            match &pending.asker {
                Asker::Polkit(request) => {
                    let _ = request.reply.try_send(granted);
                }
                Asker::Wifi(_, reply) => drifter_net::agent::answer(reply, None),
            }
        }
        self.paint_prompt();
    }

    pub(super) fn prompt_cancel(&mut self) {
        if self.prompts.first().is_some_and(|pending| pending.busy) {
            return;
        }
        if let Some(cookie) = self.prompts.first().map(Pending::cookie) {
            self.prompt_end(&cookie, false);
        }
    }

    pub(super) fn prompt_submit(&mut self) {
        let Some(pending) = self.prompts.first_mut() else {
            return;
        };
        if pending.busy || pending.pad.is_empty() {
            return;
        }
        if let Asker::Wifi(_, reply) = &pending.asker {
            let typed = pending.pad.take();
            let end = typed.iter().position(|byte| *byte == 0).unwrap_or(typed.len());
            let secret = String::from_utf8_lossy(&typed[..end]).into_owned();
            drifter_net::agent::answer(reply, Some(secret));
            pending.pad.wipe();
            self.prompts.remove(0);
            self.list_refresh();
            self.paint_prompt();
            return;
        }
        pending.busy = true;
        pending.error = None;
        let password = pending.pad.take();
        let Asker::Polkit(request) = &pending.asker else {
            return;
        };
        let user = request.user.clone();
        let cookie = request.cookie.clone();
        let reply = self.checked.clone();
        let spawned = std::thread::Builder::new()
            .name("drifter-polkit-check".to_string())
            .spawn(move || {
                let outcome = crate::polkit::check(&user, &cookie, password);
                let _ = reply.send((cookie, outcome));
            });
        if spawned.is_err() {
            pending.busy = false;
            pending.error = Some(drifter_core::lang::t("The password cannot be checked right now.").into());
        }
        self.paint_prompt();
    }

    pub(super) fn prompt_checked(&mut self, cookie: &str, outcome: crate::polkit::Outcome) {
        match outcome {
            crate::polkit::Outcome::Granted => self.prompt_end(cookie, true),
            crate::polkit::Outcome::Refused(message) => {
                if let Some(pending) = self.prompts.iter_mut().find(|pending| pending.cookie() == cookie) {
                    pending.busy = false;
                    pending.error = Some(message);
                    pending.shaken = Some(Instant::now());
                    pending.pad.wipe();
                }
                self.paint_prompt();
            }
        }
    }

    pub(super) fn prompt_key(&mut self, key: LockKey) {
        let Some(pending) = self.prompts.first_mut() else {
            return;
        };
        if pending.busy {
            return;
        }
        match key {
            LockKey::Letter(letter) => {
                pending.pad.push(letter);
            }
            LockKey::Back => pending.pad.backspace(),
            LockKey::Enter => {
                self.prompt_submit();
                return;
            }
            LockKey::Clear => {
                self.prompt_cancel();
                return;
            }
            LockKey::Vt(vt) => {
                self.change_vt(vt);
                return;
            }
            LockKey::Other | LockKey::Up | LockKey::Down | LockKey::Left | LockKey::Right => return,
        }
        self.paint_prompt();
    }

    pub(super) fn paint_prompt(&mut self) {
        let (Some(pending), Some(font)) = (self.prompts.first(), self.font.clone()) else {
            self.prompt_art = None;
            self.prompt_haze = None;
            self.kick_now();
            return;
        };
        let (message, user) = match &pending.asker {
            Asker::Polkit(request) => (request.message.clone(), request.user.clone()),
            Asker::Wifi(ask, _) => (drifter_core::lang::tf("Enter the password for “{}”.", &[&ask.ssid]), String::new()),
        };
        let ask = drifter_core::prompt::Ask {
            message,
            user,
            typed: pending.pad.len(),
            error: pending.error.clone(),
            busy: pending.busy,
            join: pending.joins(),
        };
        if self.prompt_haze.is_none() {
            self.paint_in_lane(
                "password dim",
                |state| Some(&mut state.haze_lane),
                |_| {},
                move || Some(worker::dim(0.28)),
                |state, art| {
                    if !state.prompts.is_empty() {
                        state.prompt_haze = Some(art);
                    }
                },
            );
        }
        let look = drifter_core::prompt::Look::themed(&self.palette);
        let rect = look.rect(self.screen, &ask, &font);
        self.paint_in_lane(
            "password prompt",
            |state| Some(&mut state.prompt_lane),
            State::paint_prompt,
            move || Some(look.paint(&ask, &font)),
            move |state, art| {
                if !state.prompts.is_empty() {
                    state.prompt_art = Some((art, rect));
                }
            },
        );
    }

    pub(super) fn show_note(&mut self, mut note: drifter_core::notify::Note) {
        let usual = (self.notify.seconds > 0).then(|| self.notify.seconds.saturating_mul(1000));
        note.expire = drifter_core::notify::expire_ms(note.asked, note.critical, usual);
        let update = self.cards.iter().any(|card| card.note.id == note.id);
        if !update {
            self.remember_note(&note);
        }
        if self.notify.quiet && !note.critical && !update {
            self.tell_closed(note.id, 1);
            return;
        }
        self.paint_note(note, true);
    }

    pub(super) fn repaint_notes(&mut self) {
        let notes: Vec<drifter_core::notify::Note> = self.cards.iter().map(|card| card.note.clone()).collect();
        for note in notes {
            self.paint_note(note, false);
        }
    }

    fn paint_note(&mut self, note: drifter_core::notify::Note, restart: bool) {
        let (Some(head), Some(body), Some(meta)) = (self.note_head.clone(), self.note_body.clone(), self.card_font.clone()) else {
            self.tell_closed(note.id, 3);
            return;
        };
        let plain = drifter_core::notify::Look::themed(&self.palette);
        let look = drifter_core::notify::Look {
            back_alpha: self.veil(plain.back_alpha),
            ..plain
        };
        let tall = look.height(&note, &drifter_core::notify::Type { head: &head, body: &body, meta: &meta });
        let mut heights: Vec<f64> = self.cards.iter().map(|card| card.tall).collect();
        let slot = match self.cards.iter().position(|held| held.note.id == note.id) {
            Some(at) => {
                heights[at] = tall;
                at
            }
            None => {
                heights.insert(0, tall);
                0
            }
        };
        let spot = self.card_stack(&heights).get(slot).copied();
        let frosted = spot.map_or(f64::NAN, |spot| spot.origin.y);
        let frosting = self.frosting();
        let painted = note.clone();
        self.paint_on_worker(
            "notification",
            move || {
                let kinds = drifter_core::notify::Type { head: &head, body: &body, meta: &meta };
                let mut image = look.paint(&painted, &kinds);
                if let Some(spot) = spot {
                    worker::frost(&frosting, &mut image, spot.origin, look.back_alpha);
                }
                Some(image)
            },
            move |state, art| state.place_note(note, art, tall, restart, frosted),
        );
    }

    fn place_note(
        &mut self,
        note: drifter_core::notify::Note,
        art: Option<TextureBuffer<Tex>>,
        tall: f64,
        restart: bool,
        frosted: f64,
    ) {
        let Some(art) = art else {
            self.tell_closed(note.id, 3);
            return;
        };
        let now = Instant::now();
        let held = self.cards.iter().find(|held| held.note.id == note.id);
        let until = match (restart, held.map(|held| held.until)) {
            (false, Some(until)) => until,
            _ => note.expire.map(|ms| now + Duration::from_millis(u64::from(ms))),
        };
        let card = Card {
            born: held.map_or(now, |held| held.born),
            from_y: held.map_or(0.0, |held| held.from_y),
            moved: held.map_or(now, |held| held.moved),
            note,
            art,
            tall,
            until,
            frosted,
        };
        let mut dropped = Vec::new();
        self.restack_cards(|state| {
            match state.cards.iter().position(|held| held.note.id == card.note.id) {
                Some(at) => state.cards[at] = card,
                None => state.cards.insert(0, card),
            }
            while state.cards.len() > drifter_core::notify::MOST {
                if let Some(old) = state.cards.pop() {
                    dropped.push(old.note.id);
                }
            }
        });
        for id in dropped {
            self.tell_closed(id, 1);
        }
        self.kick_now();
    }

    fn restack_cards(&mut self, change: impl FnOnce(&mut State)) {
        let was: std::collections::HashMap<u32, (f64, f64)> = self
            .cards
            .iter()
            .zip(self.card_rects().into_iter().zip(self.card_shown()))
            .map(|(card, (target, shown))| (card.note.id, (target.origin.y, shown.origin.y)))
            .collect();
        change(self);
        let now = Instant::now();
        let rects = self.card_rects();
        for (card, rect) in self.cards.iter_mut().zip(rects) {
            match was.get(&card.note.id) {
                Some((target, shown)) if (target - rect.origin.y).abs() > 0.5 => {
                    card.from_y = *shown;
                    card.moved = now;
                }
                Some(_) => {}
                None => card.from_y = rect.origin.y,
            }
        }
    }

    fn card_stack(&self, heights: &[f64]) -> Vec<Rect> {
        let bottom = Edge::Bottom.index();
        let floor = match self.tunables.bar_hides[bottom] {
            true => 0.0,
            false => self.screen.y - self.drawers[bottom].open_band(self.screen, &self.shapes[bottom]).origin.y,
        };
        drifter_core::notify::Look::themed(&self.palette).stack(self.screen, heights, self.notify.position, floor)
    }

    fn cards_still(&self) -> bool {
        self.window_pace <= 0.0 || self.window_still
    }

    pub(super) fn card_shown(&self) -> Vec<Rect> {
        let rects = self.card_rects();
        if self.cards_still() {
            return rects;
        }
        let pace = self.window_pace;
        let look = drifter_core::notify::Look::themed(&self.palette);
        self.cards
            .iter()
            .zip(rects)
            .map(|(card, rect)| {
                let entered = drifter_core::motion::smooth(card.born.elapsed().as_secs_f64() / (CARD_IN / pace));
                let dx = (1.0 - entered) * (look.width + look.margin);
                let glide = drifter_core::motion::smooth(card.moved.elapsed().as_secs_f64() / (CARD_GLIDE / pace));
                let y = card.from_y + (rect.origin.y - card.from_y) * glide;
                Rect::new(Vec2::new(rect.origin.x + dx, y), rect.size)
            })
            .collect()
    }

    pub(super) fn cards_moving(&self) -> bool {
        if self.cards_still() {
            return false;
        }
        let pace = self.window_pace;
        self.cards.iter().any(|card| {
            card.born.elapsed().as_secs_f64() < CARD_IN / pace || card.moved.elapsed().as_secs_f64() < CARD_GLIDE / pace
        })
    }

    pub(super) fn close_note(&mut self, id: u32, reason: u32) {
        let before = self.cards.len();
        self.restack_cards(|state| state.cards.retain(|card| card.note.id != id));
        if self.cards.len() != before {
            self.tell_closed(id, reason);
            self.kick_now();
        }
    }

    pub(super) fn tell_closed(&self, id: u32, reason: u32) {
        if let Some(bus) = self.notes_bus.as_ref() {
            bus.closed(id, reason);
        }
    }

    pub(super) fn card_rects(&self) -> Vec<Rect> {
        let heights: Vec<f64> = self.cards.iter().map(|card| card.tall).collect();
        self.card_stack(&heights)
    }

    pub(super) fn expire_cards(&mut self, now: Instant) {
        let rects = self.card_rects();
        let resting = !self.cards_moving();
        let mut refrost = Vec::new();
        for (card, rect) in self.cards.iter_mut().zip(&rects) {
            if resting && (card.frosted - rect.origin.y).abs() > 0.5 {
                card.frosted = rect.origin.y;
                refrost.push(card.note.clone());
            }
        }
        for note in refrost {
            self.paint_note(note, false);
        }
        let over = rects.iter().position(|rect| rect.contains(self.pointer));
        let mut gone = Vec::new();
        for (at, card) in self.cards.iter_mut().enumerate() {
            if Some(at) == over {
                if let Some(until) = card.until.as_mut() {
                    *until = (*until).max(now + Duration::from_secs(2));
                }
                continue;
            }
            if card.until.is_some_and(|until| now >= until) {
                gone.push(card.note.id);
            }
        }
        for id in gone {
            self.close_note(id, 1);
        }
    }

    pub(super) fn press_card(&mut self, code: u32) -> bool {
        let Some(at) = self
            .card_rects()
            .iter()
            .position(|rect| rect.contains(self.pointer))
        else {
            return false;
        };
        let id = self.cards[at].note.id;
        let link = self.note_links.remove(&id);
        if let Some(url) = link.as_deref()
            && code == BTN_LEFT
        {
            self.open_link(url);
        } else if let Some(folder) = self.note_folders.remove(&id)
            && code == BTN_LEFT
        {
            crate::apps::open_folder(
                &folder.to_string_lossy(),
                self.apps_chosen.files.as_ref(),
                &self.socket,
            );
        } else if code == BTN_LEFT
            && let (Some(key), Some(bus)) = (
                self.cards[at].note.default_action(),
                self.notes_bus.as_ref(),
            )
        {
            bus.action(id, key);
        }
        self.close_note(id, 2);
        true
    }

    pub(super) fn switch_commit(&mut self) {
        let Some(open) = self.switch.take() else {
            return;
        };
        if let Some(place) = open.order.get(open.at).and_then(|id| self.place_of(*id)) {
            self.bring(place);
        }
        self.kick_now();
    }

    pub(super) fn switch_elements(&self, elements: &mut Vec<Skin>) {
        self.pane_elements(super::fade::Pane::Switch, elements);
    }

}
