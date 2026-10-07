use super::*;

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
                let apps: Vec<String> = order
                    .iter()
                    .filter_map(|id| self.windows.iter().find(|window| window.id == *id))
                    .map(|window| window.app.clone())
                    .collect();
                let icons = apps
                    .iter()
                    .map(|app| self.switch_icon(app, layout.icon.round() as u32))
                    .collect();
                self.switch = Some(Switch {
                    order,
                    at,
                    layout,
                    art: None,
                    icons,
                });
                at
            }
        };
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
            .get(at)
            .map(|window| match window.title.is_empty() {
                true => window.app.clone(),
                false => window.title.clone(),
            })
            .unwrap_or_default();
        let image = open.layout.paint(at, &title, &letters, self.font.as_ref());
        let art = match self.upload_again(Some(&image), "switcher") {
            Ok(art) => art,
            Err(error) => {
                log!("drifter: {error}");
                None
            }
        };
        if let Some(open) = self.switch.as_mut() {
            open.at = at;
            open.art = art;
        }
        self.kick_now();
    }

    pub(super) fn show_hud(&mut self, reading: drifter_core::hud::Reading) {
        let look = drifter_core::hud::Look::themed(&self.palette);
        let image = look.paint(reading, self.font.as_ref());
        match upload(&mut self.renderer, Some(&image), "hud") {
            Ok(Some(art)) => {
                self.hud = Some((art, look.rect(self.screen), Instant::now() + HUD_TIME));
            }
            Ok(None) => {}
            Err(error) => log!("drifter: {error}"),
        }
        self.kick_now();
    }

    pub(super) fn prompt_begin(&mut self, request: crate::polkit::Request) {
        if self.font.is_none() {
            log!("drifter: no font, so no password prompt can be shown");
            let _ = request.reply.try_send(false);
            return;
        }
        self.prompts.push(Pending {
            request,
            pad: Pad::new(),
            error: None,
            busy: false,
        });
        self.paint_prompt();
    }

    pub(super) fn prompt_end(&mut self, cookie: &str, granted: bool) {
        if let Some(at) = self
            .prompts
            .iter()
            .position(|pending| pending.request.cookie == cookie)
        {
            let pending = self.prompts.remove(at);
            let _ = pending.request.reply.try_send(granted);
        }
        self.paint_prompt();
    }

    pub(super) fn prompt_cancel(&mut self) {
        if self.prompts.first().is_some_and(|pending| pending.busy) {
            return;
        }
        if let Some(cookie) = self.prompts.first().map(|p| p.request.cookie.clone()) {
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
        pending.busy = true;
        pending.error = None;
        let password = pending.pad.take();
        let user = pending.request.user.clone();
        let cookie = pending.request.cookie.clone();
        let reply = self.checked.clone();
        let spawned = std::thread::Builder::new()
            .name("drifter-polkit-check".to_string())
            .spawn(move || {
                let outcome = crate::polkit::check(&user, &cookie, password);
                let _ = reply.send((cookie, outcome));
            });
        if spawned.is_err() {
            pending.busy = false;
            pending.error = Some("The password cannot be checked right now.".into());
        }
        self.paint_prompt();
    }

    pub(super) fn prompt_checked(&mut self, cookie: &str, outcome: crate::polkit::Outcome) {
        match outcome {
            crate::polkit::Outcome::Granted => self.prompt_end(cookie, true),
            crate::polkit::Outcome::Refused(message) => {
                if let Some(pending) = self
                    .prompts
                    .iter_mut()
                    .find(|pending| pending.request.cookie == cookie)
                {
                    pending.busy = false;
                    pending.error = Some(message);
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
        let (Some(pending), Some(font)) = (self.prompts.first(), self.font.as_ref()) else {
            self.prompt_art = None;
            self.kick_now();
            return;
        };
        let ask = drifter_core::prompt::Ask {
            message: pending.request.message.clone(),
            user: pending.request.user.clone(),
            typed: pending.pad.len(),
            error: pending.error.clone(),
            busy: pending.busy,
        };
        let look = drifter_core::prompt::Look::themed(&self.palette);
        let image = look.paint(&ask, font);
        let rect = look.rect(self.screen, &ask, font);
        match upload(&mut self.renderer, Some(&image), "password prompt") {
            Ok(Some(art)) => self.prompt_art = Some((art, rect)),
            Ok(None) => {}
            Err(error) => log!("drifter: {error}"),
        }
        self.kick_now();
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
        let Some(font) = self.font.as_ref() else {
            self.tell_closed(note.id, 3);
            return;
        };
        let plain = drifter_core::notify::Look::themed(&self.palette);
        let look = drifter_core::notify::Look {
            back_alpha: self.veil(plain.back_alpha),
            ..plain
        };
        let tall = look.height(&note, font);
        let mut image = look.paint(&note, font);
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
        if let Some(spot) = look.stack(self.screen, &heights).get(slot) {
            self.frosted(&mut image, spot.origin, look.back_alpha);
        }
        let art = match upload(&mut self.renderer, Some(&image), "notification") {
            Ok(Some(art)) => art,
            Ok(None) => {
                self.tell_closed(note.id, 3);
                return;
            }
            Err(error) => {
                log!("drifter: {error}");
                self.tell_closed(note.id, 3);
                return;
            }
        };
        let until = note
            .expire
            .map(|ms| Instant::now() + Duration::from_millis(u64::from(ms)));
        let card = Card {
            note,
            art,
            tall,
            until,
        };
        match self
            .cards
            .iter()
            .position(|held| held.note.id == card.note.id)
        {
            Some(at) => self.cards[at] = card,
            None => self.cards.insert(0, card),
        }
        while self.cards.len() > drifter_core::notify::MOST {
            if let Some(old) = self.cards.pop() {
                self.tell_closed(old.note.id, 1);
            }
        }
        self.kick_now();
    }

    pub(super) fn close_note(&mut self, id: u32, reason: u32) {
        let before = self.cards.len();
        self.cards.retain(|card| card.note.id != id);
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
        drifter_core::notify::Look::themed(&self.palette).stack(self.screen, &heights)
    }

    pub(super) fn expire_cards(&mut self, now: Instant) {
        let rects = self.card_rects();
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
        if let Some(folder) = self.note_folders.remove(&id)
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
        let Some(open) = self.switch.as_ref() else {
            return;
        };
        let Some(art) = open.art.as_ref() else {
            return;
        };
        let panel = open.layout.panel;
        for (index, icon) in open.icons.iter().enumerate() {
            if let Some(icon) = icon {
                let rect = open.layout.icon_rect(index);
                let rect = Rect::new(panel.origin + rect.origin, rect.size);
                elements.push(Skin::Texture(element(icon, rect, 1.0)));
            }
        }
        elements.push(Skin::Texture(element(art, panel, 1.0)));
    }

    pub(super) fn switch_icon(&mut self, app: &str, side: u32) -> Option<TextureBuffer<Tex>> {
        let key = format!("{app}@{side}");
        if let Some(held) = self.app_icons.get(&key) {
            return Some(held.clone());
        }
        if app.is_empty() || self.no_icon.contains(app) {
            return None;
        }
        let image = crate::apps::find(app)
            .and_then(|entry| entry.icon)
            .and_then(|path| drifter_core::image::load_sized(&path, side).ok())
            .map(|image| image.scaled(side, side))?;
        let buffer = upload(&mut self.renderer, Some(&image), "switcher icon")
            .ok()
            .flatten()?;
        self.app_icons.insert(key, buffer.clone());
        Some(buffer)
    }
}
