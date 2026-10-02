use super::*;

impl State {
    pub(super) fn dock_band(&self) -> Option<Rect> {
        let drawer = self.drawers[Edge::Bottom.index()];
        drawer
            .visible()
            .then(|| drawer.band(self.screen, &self.shapes[Edge::Bottom.index()]))
    }

    pub(super) fn minimized(&self) -> Vec<String> {
        self.windows
            .iter()
            .filter(|window| window.hidden)
            .map(|window| window.app.clone())
            .collect()
    }

    pub(super) fn press_dock(&mut self) -> bool {
        let Some(band) = self.dock_band() else {
            return false;
        };
        let hidden = self.minimized();
        let pinned = self.pinned.len();
        let Some(slot) = self.dock.at(band, self.pointer, pinned, hidden.len()) else {
            return false;
        };

        match slot {
            Slot::Pinned(index) => {
                crate::apps::run(&self.pinned[index].0, &self.socket);
            }
            Slot::Settings => self.open_settings(),
            Slot::Trash => {
                let home = std::env::var("HOME").unwrap_or_default();
                let trash = format!("{home}/.local/share/Trash/files");
                let _ = std::fs::create_dir_all(&trash);
                crate::apps::open_folder(&trash, self.apps_chosen.files.as_ref(), &self.socket);
            }
            Slot::Minimized(index) => {
                let Some(place) = self
                    .windows
                    .iter()
                    .enumerate()
                    .filter(|(_, window)| window.hidden)
                    .map(|(at, _)| at)
                    .nth(index)
                else {
                    return true;
                };
                let seat = self.dock_slot(index);
                self.windows[place].hidden = false;
                self.windows[place].motion = Some(Motion::new(Phase::Restoring, seat));
                self.focus_window(place);
            }
        }
        true
    }

    pub(super) fn tray_band(&self) -> Option<Rect> {
        let drawer = self.drawers[Edge::Top.index()];
        drawer
            .visible()
            .then(|| drawer.band(self.screen, &self.shapes[Edge::Top.index()]))
    }

    pub(super) fn take_icons(&mut self, icons: Vec<crate::tray::Icon>) {
        let wanted = self.tray.icon as u32;
        self.icons.clear();
        for icon in icons {
            let image = icon.image.scaled(wanted, wanted);
            match upload(&mut self.renderer, Some(&image), "tray icon") {
                Ok(Some(buffer)) => self.icons.push((icon.service, icon.title, buffer)),
                Ok(None) => {}
                Err(error) => eprintln!("drifter: {error}"),
            }
        }
    }

    pub(super) fn open_popover(&mut self, items: Vec<Item>, at: Vec2, target: Target) {
        if items.is_empty() || self.lock.is_some() {
            return;
        }
        let look = self.look;
        let size = look.size(&items, self.font.as_ref());
        let image = look.paint(&items, size, None, self.font.as_ref());
        let rect = look.rect(at, size, self.screen);
        match upload(&mut self.renderer, Some(&image), "menu") {
            Ok(Some(art)) => {
                self.popover = Some(Popover {
                    items,
                    rect,
                    art,
                    target,
                    hover: None,
                })
            }
            Ok(None) => {}
            Err(error) => eprintln!("drifter: {error}"),
        }
    }

    pub(super) fn repaint_popover(&mut self) {
        let Some(open) = self.popover.as_ref() else {
            return;
        };
        let inside = self.pointer - open.rect.origin;
        let hover = self.look.item_at(&open.items, open.rect.size, inside);
        if hover == open.hover {
            return;
        }
        let items = open.items.clone();
        let size = open.rect.size;
        let image = self.look.paint(&items, size, hover, self.font.as_ref());
        match upload(&mut self.renderer, Some(&image), "menu") {
            Ok(Some(art)) => {
                if let Some(open) = self.popover.as_mut() {
                    open.art = art;
                    open.hover = hover;
                }
            }
            Ok(None) => {}
            Err(error) => eprintln!("drifter: {error}"),
        }
    }

    pub(super) fn press_popover(&mut self) -> bool {
        let Some(open) = self.popover.as_ref() else {
            return false;
        };
        if !open.rect.contains(self.pointer) {
            self.popover = None;
            return false;
        }

        let inside = self.pointer - open.rect.origin;
        let Some(index) = self.look.item_at(&open.items, open.rect.size, inside) else {
            return true;
        };
        let chosen = open.items[index].clone();
        if !chosen.enabled {
            return true;
        }

        match &open.target {
            Target::Tray { service, trail, at } => {
                let (service, mut trail, at) = (service.clone(), trail.clone(), *at);
                let deeper = chosen.id == drifter_core::menu::BACK || chosen.children;
                if let Some(bus) = self.tray_bus.as_ref() {
                    match deeper {
                        true => {
                            match chosen.id == drifter_core::menu::BACK {
                                true => trail.pop(),
                                false => {
                                    trail.push(chosen.id);
                                    None
                                }
                            };
                            bus.ask_menu(&service, trail, at.x, at.y);
                            return true;
                        }
                        false => bus.choose(&service, chosen.id),
                    }
                }
            }
            Target::Desktop => {
                self.popover = None;
                match Action::from_id(chosen.id) {
                    Some(Action::Lock) => {
                        self.lock_now();
                    }
                    Some(Action::Logout) => self.signal.stop(),
                    Some(Action::Suspend) => {
                        if self.lock_now() {
                            crate::power::run(Action::Suspend);
                        }
                    }
                    Some(action) => crate::power::run(action),
                    None => self.open_settings(),
                }
                return true;
            }
            Target::Window(id) => {
                let id = *id;
                self.popover = None;
                let Some(index) = self.place_of(id) else {
                    return true;
                };
                match chosen.id {
                    0 => {
                        let window = &mut self.windows[index];
                        window.hidden = !window.hidden;
                        if window.hidden {
                            self.focus_next(Some(id));
                        } else {
                            self.focus_window(index);
                        }
                    }
                    _ => self.windows[index].toplevel.send_close(),
                }
                return true;
            }
        }
        self.popover = None;
        true
    }

    pub(super) fn show_tip(&mut self, text: &str, under: Rect, below: bool) {
        if self.tip_of.as_deref() == Some(text) {
            return;
        }
        let items = vec![Item::line(0, text)];
        let look = Look {
            least_width: 0.0,
            ..self.look
        };
        let size = look.size(&items, self.font.as_ref());
        let image = look.paint(&items, size, None, self.font.as_ref());
        let at = Vec2::new(
            under.center().x - size.x / 2.0,
            if below {
                under.max().y + 6.0
            } else {
                under.origin.y - size.y - 6.0
            },
        );
        let rect = look.rect(at, size, self.screen);
        match upload(&mut self.renderer, Some(&image), "hover text") {
            Ok(Some(art)) => {
                self.tip = Some((art, rect));
                self.tip_of = Some(text.to_string());
            }
            Ok(None) => {}
            Err(error) => eprintln!("drifter: {error}"),
        }
    }

    pub(super) fn settle_tip(&mut self) {
        let Some((at, since)) = self.hover_since else {
            self.tip = None;
            self.tip_of = None;
            return;
        };
        if (self.pointer - at).length() > 3.0 {
            self.hover_since = Some((self.pointer, Instant::now()));
            self.tip = None;
            self.tip_of = None;
            return;
        }
        if Instant::now().duration_since(since) < self.tunables.drawer_dwell * 4 {
            return;
        }

        if let Some(band) = self.tray_band()
            && self.tray.on_power(band, self.pointer)
        {
            let rect = self.tray.power_rect(band);
            self.show_tip("Power", rect, true);
            return;
        }

        if let Some(band) = self.tray_band()
            && let Some(slot) = self.tray.icon_at(band, self.pointer, self.tray_room(band))
        {
            let rect = self.tray.icon_rect(band, slot);
            let text = self.icons[slot].1.clone();
            self.show_tip(&text, rect, true);
            return;
        }

        if let Some(band) = self.dock_band() {
            let hidden = self.minimized();
            let pinned = self.pinned.len();
            if let Some(slot) = self.dock.at(band, self.pointer, pinned, hidden.len()) {
                let rect = self.dock.slot_rect(band, pinned, hidden.len(), slot);
                let text = match slot {
                    Slot::Pinned(index) => self.pinned[index].0.name(),
                    Slot::Minimized(index) => hidden[index].clone(),
                    Slot::Settings => "Settings".to_string(),
                    Slot::Trash => "Trash".to_string(),
                };
                self.show_tip(&text, rect, false);
                return;
            }
        }

        self.tip = None;
        self.tip_of = None;
    }

    pub(super) fn open_context(&mut self) -> bool {
        if let Some(band) = self.tray_band()
            && let Some(slot) = self.tray.icon_at(band, self.pointer, self.tray_room(band))
        {
            let service = self.icons[slot].0.clone();
            if let Some(bus) = self.tray_bus.as_ref() {
                bus.ask_menu(&service, Vec::new(), self.pointer.x, self.pointer.y);
            }
            return true;
        }

        if !self.over_drawer() && self.surface_under().is_none() && self.grip_under().is_none() {
            let at = self.pointer;
            let mut items = vec![Item::line(0, "Settings"), Item::rule(1)];
            items.extend(drifter_core::power::items());
            self.open_popover(items, at, Target::Desktop);
            return true;
        }

        if let Some(band) = self.dock_band() {
            let hidden = self.minimized();
            let pinned = self.pinned.len();
            if let Some(Slot::Minimized(index)) =
                self.dock.at(band, self.pointer, pinned, hidden.len())
            {
                let Some(place) = self
                    .windows
                    .iter()
                    .enumerate()
                    .filter(|(_, window)| window.hidden)
                    .map(|(at, _)| at)
                    .nth(index)
                else {
                    return true;
                };
                let items = vec![Item::line(0, "Show"), Item::line(1, "Exit")];
                let at = self.pointer;
                let id = self.windows[place].id;
                self.open_popover(items, at, Target::Window(id));
                return true;
            }
        }

        false
    }

    pub(super) fn press_tray(&mut self, code: u32, had_menu: bool) -> bool {
        let Some(band) = self.tray_band() else {
            return false;
        };
        if self.tray.on_power(band, self.pointer) {
            if code == BTN_LEFT && !had_menu {
                let rect = self.tray.power_rect(band);
                let at = Vec2::new(rect.max().x, rect.max().y + 6.0);
                self.open_popover(drifter_core::power::items(), at, Target::Desktop);
            }
            return true;
        }
        let Some(slot) = self.tray.icon_at(band, self.pointer, self.tray_room(band)) else {
            return false;
        };
        let click = match code {
            BTN_LEFT => crate::tray::Click::Left,
            BTN_MIDDLE => crate::tray::Click::Middle,
            BTN_RIGHT => return false,
            _ => return true,
        };
        if let Some(bus) = self.tray_bus.as_ref() {
            let service = &self.icons[slot].0;
            bus.click(service, click, self.pointer.x as i32, self.pointer.y as i32);
        }
        true
    }

    pub(super) fn over_own_parts(&self) -> bool {
        self.prompt_art.is_some()
            || self
                .card_rects()
                .iter()
                .any(|rect| rect.contains(self.pointer))
            || self.over_drawer()
            || self
                .popover
                .as_ref()
                .is_some_and(|open| open.rect.contains(self.pointer))
    }

    pub(super) fn tray_room(&self, band: Rect) -> usize {
        self.tray_layout(band).0
    }

    pub(super) fn tray_layout(&self, band: Rect) -> (usize, bool) {
        let stop = match self.clock.as_ref() {
            Some((_, _, rect)) => rect.origin.x,
            None => self.tray.power_rect(band).origin.x,
        };
        let stats = self.stats.as_ref().map(|(_, size)| size.x);
        self.tray.layout(band, stop, self.icons.len(), stats)
    }

    pub(super) fn over_drawer(&self) -> bool {
        Edge::ALL.into_iter().any(|edge| {
            let drawer = &self.drawers[edge.index()];
            drawer.visible()
                && drawer
                    .rect(self.screen, &self.shapes[edge.index()])
                    .contains(self.pointer)
        })
    }

    pub(super) fn wind_the_clock(&mut self) {
        let drawer = self.drawers[Edge::Top.index()];
        if !drawer.visible() || self.font.is_none() {
            return;
        }

        let reading = match drifter_core::tray::now(&self.format) {
            Ok(reading) => reading,
            Err(error) => {
                eprintln!("drifter: {error}");
                self.format = drifter_core::tray::DEFAULT_FORMAT.to_string();
                return;
            }
        };

        let tray = self.tray;
        let band = drawer.band(self.screen, &self.shapes[Edge::Top.index()]);

        if let Some((shown, _, rect)) = self.clock.as_mut()
            && *shown == reading
        {
            *rect = tray.clock_rect(band, rect.size);
            return;
        }

        let image = self
            .font
            .as_ref()
            .map(|font| font.paint(&reading, tray.ink))
            .expect("the font was there a moment ago");
        let rect = tray.clock_rect(band, image.size());
        match upload(&mut self.renderer, Some(&image), "clock") {
            Ok(Some(buffer)) => self.clock = Some((reading, buffer, rect)),
            Ok(None) => {}
            Err(error) => eprintln!("drifter: {error}"),
        }
    }

    pub(super) fn wind_the_stats(&mut self) {
        if !self.drawers[Edge::Top.index()].visible() || self.font.is_none() {
            return;
        }
        if self.stats.is_some()
            && self
                .sampled
                .is_some_and(|at| at.elapsed() < Duration::from_secs(1))
        {
            return;
        }
        self.sampled = Some(Instant::now());
        let fields = self.sampler.read();
        if fields.is_empty() {
            self.stats = None;
            return;
        }
        let fields: Vec<(&str, u8)> = fields
            .iter()
            .map(|(label, value)| (label.as_str(), *value))
            .collect();
        let image = self
            .font
            .as_ref()
            .map(|font| drifter_core::stats::paint(font, &fields, self.tray.ink))
            .expect("the font was there a moment ago");
        let size = image.size();
        match upload(&mut self.renderer, Some(&image), "system info") {
            Ok(Some(buffer)) => self.stats = Some((buffer, size)),
            Ok(None) => {}
            Err(error) => eprintln!("drifter: {error}"),
        }
    }
}
