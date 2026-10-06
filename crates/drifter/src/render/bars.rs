use super::*;
use drifter_core::hud::Gauge;

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

    pub(super) fn tray_band(&self) -> Option<Rect> {
        let drawer = self.drawers[Edge::Top.index()];
        drawer
            .visible()
            .then(|| drawer.band(self.screen, &self.shapes[Edge::Top.index()]))
    }

    pub(super) fn take_icons(&mut self, icons: Vec<crate::tray::Icon>) {
        let wanted = self.tray.icon as u32;
        self.icons.clear();
        self.tray_raw = icons.clone();
        for icon in icons {
            let mut image = icon.image.scaled(wanted, wanted);
            if icon.symbolic {
                image.tint(self.palette.glyph);
            }
            match upload(&mut self.renderer, Some(&image), "tray icon") {
                Ok(Some(buffer)) => self.icons.push((icon.service, icon.title, buffer)),
                Ok(None) => {}
                Err(error) => log!("drifter: {error}"),
            }
        }
    }

    pub(super) fn open_popover(&mut self, items: Vec<Item>, at: Vec2, target: Target) {
        if items.is_empty() || self.lock.is_some() {
            return;
        }
        let look = self.menu_look();
        let size = look.size(&items, self.font.as_ref());
        let mut image = look.paint(&items, size, None, self.font.as_ref());
        let rect = look.rect(at, size, self.screen);
        self.frosted(&mut image, rect.origin, f64::from(look.back_alpha) / 255.0);
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
            Err(error) => log!("drifter: {error}"),
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
        let origin = open.rect.origin;
        let look = self.menu_look();
        let mut image = look.paint(&items, size, hover, self.font.as_ref());
        self.frosted(&mut image, origin, f64::from(look.back_alpha) / 255.0);
        match upload(&mut self.renderer, Some(&image), "menu") {
            Ok(Some(art)) => {
                if let Some(open) = self.popover.as_mut() {
                    open.art = art;
                    open.hover = hover;
                }
            }
            Ok(None) => {}
            Err(error) => log!("drifter: {error}"),
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
            Target::Dock(key) => {
                let key = key.clone();
                self.popover = None;
                self.dock_menu_pick(&key, chosen.id);
                return true;
            }
            Target::PadApp(key) => {
                let key = key.clone();
                self.popover = None;
                self.pad_menu_pick(&key, chosen.id);
                return true;
            }
            Target::Shooter => {
                self.shooter_menu_pick(chosen.id);
                return true;
            }
            Target::DockBar => {
                self.dock_bar_pick(chosen.id);
                return true;
            }
            Target::App(app) => {
                let app = app.clone();
                self.popover = None;
                self.app_menu_pick(&app, chosen.id);
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
            ..self.menu_look()
        };
        let size = look.size(&items, self.font.as_ref());
        let mut image = look.paint(&items, size, None, self.font.as_ref());
        let at = Vec2::new(
            under.center().x - size.x / 2.0,
            if below {
                under.max().y + 6.0
            } else {
                under.origin.y - size.y - 6.0
            },
        );
        let rect = look.rect(at, size, self.screen);
        self.frosted(&mut image, rect.origin, f64::from(look.back_alpha) / 255.0);
        match upload(&mut self.renderer, Some(&image), "hover text") {
            Ok(Some(art)) => {
                self.tip = Some((art, rect));
                self.tip_of = Some(text.to_string());
            }
            Ok(None) => {}
            Err(error) => log!("drifter: {error}"),
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

        if let Some(rect) = self
            .speaker_rect()
            .filter(|rect| rect.contains(self.pointer))
            && self.gauge_panel.is_none()
        {
            let words = match self.volume {
                Some((_, true)) => "Volume: muted".to_string(),
                Some((level, false)) => format!("Volume: {}%", (level * 100.0).round() as i64),
                None => "Volume".to_string(),
            };
            self.show_tip(&words, rect, true);
            return;
        }

        if let Some(rect) = self.sun_rect().filter(|rect| rect.contains(self.pointer))
            && self.gauge_panel.is_none()
        {
            let words = match self.light {
                Some(level) => format!("Brightness: {}%", (level * 100.0).round() as i64),
                None => "Brightness".to_string(),
            };
            self.show_tip(&words, rect, true);
            return;
        }

        if let Some(band) = self.tray_band()
            && let Some(slot) = self.tray_icon_at(band)
        {
            let rect = self.tray_icon_rect(band, slot);
            let text = self.icons[slot].1.clone();
            self.show_tip(&text, rect, true);
            return;
        }

        if let Some((text, rect)) = self.dock_tip() {
            self.show_tip(&text, rect, false);
            return;
        }

        self.tip = None;
        self.tip_of = None;
    }

    pub(super) fn open_context(&mut self) -> bool {
        if let Some(band) = self.tray_band()
            && let Some(slot) = self.tray_icon_at(band)
        {
            let service = self.icons[slot].0.clone();
            if let Some(bus) = self.tray_bus.as_ref() {
                bus.ask_menu(&service, Vec::new(), self.pointer.x, self.pointer.y);
            }
            return true;
        }

        if !self.over_drawer() && self.surface_under().is_none() && self.grip_under().is_none() {
            self.open_desk_menu(self.pointer);
            return true;
        }

        if self.dock_menu() {
            return true;
        }
        let minimized_slot = self.dock_band().is_some_and(|band| {
            matches!(
                self.dock.at(band, self.pointer, self.dock_apps().len(), self.minimized().len()),
                Some(Slot::Minimized(_))
            )
        });
        if !minimized_slot && self.dock_bar_menu() {
            return true;
        }

        if let Some(band) = self.dock_band() {
            let hidden = self.minimized();
            let pinned = self.dock_apps().len();
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
        if code == BTN_LEFT && !had_menu && self.press_app_label() {
            return true;
        }
        if code == BTN_LEFT && self.control_rect().is_some_and(|rect| rect.contains(self.pointer)) {
            self.toggle_control();
            return true;
        }
        if code == BTN_LEFT
            && self
                .clock
                .as_ref()
                .is_some_and(|(_, _, rect)| rect.contains(self.pointer))
        {
            self.toggle_notes();
            return true;
        }
        if self
            .speaker_rect()
            .is_some_and(|rect| rect.contains(self.pointer))
        {
            match code {
                BTN_MIDDLE => {
                    if let Some(media) = self.media.as_ref() {
                        media.toggle_mute();
                    }
                }
                _ => self.toggle_gauge_panel(Gauge::Volume),
            }
            return true;
        }
        if self.sun_rect().is_some_and(|rect| rect.contains(self.pointer)) {
            self.toggle_gauge_panel(Gauge::Brightness);
            return true;
        }
        if self.tray.on_power(band, self.pointer) {
            if code == BTN_LEFT && !had_menu {
                let rect = self.tray.power_rect(band);
                let at = Vec2::new(rect.max().x, rect.max().y + 6.0);
                self.open_popover(drifter_core::power::items(), at, Target::Desktop);
            }
            return true;
        }
        let Some(slot) = self.tray_icon_at(band) else {
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

    pub(super) fn hear(&mut self, heard: crate::media::Heard) {
        let reading = heard.reading;
        match reading.gauge {
            Gauge::Volume => self.take_volume(reading.level.min(1.0), reading.muted),
            Gauge::Brightness => self.take_light(reading.level.min(1.0)),
            Gauge::Microphone => {}
        }
        if heard.show {
            self.show_hud(reading);
        } else {
            self.kick_now();
        }
    }

    pub(super) fn take_volume(&mut self, level: f64, muted: bool) {
        self.volume = Some((level, muted));
        let key = (drifter_core::volume::waves(level, muted), muted);
        if self.speaker.as_ref().map(|(_, held)| *held) != Some(key) {
            let side = self.tray.button as u32;
            let image = drifter_core::volume::paint_speaker(side, level, muted, self.tray.ink);
            match upload(&mut self.renderer, Some(&image), "speaker") {
                Ok(Some(buffer)) => self.speaker = Some((buffer, key)),
                Ok(None) => {}
                Err(error) => log!("drifter: {error}"),
            }
        }
        if self.gauge_panel.as_ref().is_some_and(|panel| panel.gauge == Gauge::Volume) {
            self.paint_gauge_panel(Gauge::Volume);
        }
    }

    pub(super) fn take_light(&mut self, level: f64) {
        self.light = Some(level);
        if self.sun.is_none() {
            let image = drifter_core::volume::paint_sun(self.tray.button as u32, self.tray.ink);
            match upload(&mut self.renderer, Some(&image), "sun") {
                Ok(Some(buffer)) => self.sun = Some(buffer),
                Ok(None) => {}
                Err(error) => log!("drifter: {error}"),
            }
        }
        if self.gauge_panel.as_ref().is_some_and(|panel| panel.gauge == Gauge::Brightness) {
            self.paint_gauge_panel(Gauge::Brightness);
        }
    }

    pub(super) fn wind_the_light(&mut self) {
        if !self.drawers[Edge::Top.index()].visible() {
            return;
        }
        if self
            .light_read
            .is_some_and(|at| at.elapsed() < Duration::from_secs(3))
        {
            return;
        }
        self.light_read = Some(Instant::now());
        match drifter_core::system::backlight() {
            Some(light) => {
                if self.light != Some(light.level()) {
                    self.take_light(light.level());
                }
            }
            None => {
                self.light = None;
                self.sun = None;
            }
        }
    }

    pub(super) fn sun_rect(&self) -> Option<Rect> {
        None
    }

    fn gauge_rect(&self, gauge: Gauge) -> Option<Rect> {
        match gauge {
            Gauge::Brightness => self.sun_rect(),
            _ => self.speaker_rect(),
        }
    }

    fn gauge_level(&self, gauge: Gauge) -> Option<(f64, bool)> {
        match gauge {
            Gauge::Brightness => self.light.map(|level| (level, false)),
            _ => self.volume,
        }
    }

    pub(super) fn wind_the_volume(&mut self) {
        if !self.drawers[Edge::Top.index()].visible() {
            return;
        }
        let Some(media) = self.media.as_ref() else {
            return;
        };
        if self
            .volume_read
            .is_some_and(|at| at.elapsed() < Duration::from_secs(3))
        {
            return;
        }
        self.volume_read = Some(Instant::now());
        media.read();
    }

    pub(super) fn speaker_rect(&self) -> Option<Rect> {
        None
    }

    pub(super) fn menu_look(&self) -> Look {
        Look {
            back_alpha: (self.veil(f64::from(self.look.back_alpha) / 255.0) * 255.0).round() as u8,
            ..self.look
        }
    }

    fn paint_gauge_panel(&mut self, gauge: Gauge) {
        let Some((level, muted)) = self.gauge_level(gauge) else {
            return;
        };
        let plain = drifter_core::volume::Panel::themed(&self.palette);
        let look = drifter_core::volume::Panel {
            back_alpha: self.veil(plain.back_alpha),
            ..plain
        };
        let shown = ((level * 100.0).round() as i64, muted);
        if self
            .gauge_panel
            .as_ref()
            .is_some_and(|panel| panel.shown == shown && panel.gauge == gauge)
        {
            return;
        }
        let mut image = look.paint(gauge, level, muted, self.font.as_ref());
        let spot = match self.gauge_panel.as_ref().filter(|panel| panel.gauge == gauge) {
            Some(panel) => Some(panel.rect),
            None => self.gauge_rect(gauge).map(|under| look.rect(under, self.screen)),
        };
        if let Some(spot) = spot {
            self.frosted(&mut image, spot.origin, look.back_alpha);
        }
        let art = match upload(&mut self.renderer, Some(&image), "sound panel") {
            Ok(Some(art)) => art,
            Ok(None) => return,
            Err(error) => {
                log!("drifter: {error}");
                return;
            }
        };
        match self.gauge_panel.as_mut().filter(|panel| panel.gauge == gauge) {
            Some(panel) => {
                panel.art = art;
                panel.shown = shown;
            }
            None => {
                let Some(under) = self.gauge_rect(gauge) else {
                    return;
                };
                self.gauge_panel = Some(GaugePanel {
                    gauge,
                    rect: look.rect(under, self.screen),
                    art,
                    shown,
                    held: false,
                });
            }
        }
        self.kick_now();
    }

    pub(super) fn toggle_gauge_panel(&mut self, gauge: Gauge) {
        match self.gauge_panel.take() {
            Some(panel) if panel.gauge == gauge => self.kick_now(),
            _ => {
                self.popover = None;
                self.paint_gauge_panel(gauge);
                if gauge == Gauge::Volume
                    && let Some(media) = self.media.as_ref()
                {
                    media.read();
                }
            }
        }
    }

    pub(super) fn press_gauge_panel(&mut self, code: u32) -> bool {
        use drifter_core::volume::Hit;

        let Some(panel) = self.gauge_panel.as_ref() else {
            return false;
        };
        let gauge = panel.gauge;
        if !panel.rect.contains(self.pointer) {
            let on_button = self
                .gauge_rect(gauge)
                .is_some_and(|rect| rect.contains(self.pointer));
            self.gauge_panel = None;
            self.kick_now();
            return on_button;
        }
        if code != BTN_LEFT {
            return true;
        }
        let inside = self.pointer - panel.rect.origin;
        let look = drifter_core::volume::Panel::themed(&self.palette);
        match look.hit(gauge, inside) {
            Some(Hit::Level(level)) => {
                if let Some(panel) = self.gauge_panel.as_mut() {
                    panel.held = true;
                }
                self.set_gauge(gauge, level);
            }
            Some(Hit::Mute) => {
                if let Some(media) = self.media.as_ref() {
                    media.toggle_mute();
                }
            }
            Some(Hit::Settings) => {
                self.gauge_panel = None;
                self.open_settings_at(match gauge {
                    Gauge::Brightness => "displays",
                    _ => "sound",
                });
            }
            None => {}
        }
        true
    }

    pub(super) fn drag_gauge_panel(&mut self) -> bool {
        let Some(panel) = self.gauge_panel.as_ref().filter(|panel| panel.held) else {
            return false;
        };
        let gauge = panel.gauge;
        let inside = self.pointer - panel.rect.origin;
        let level = drifter_core::volume::Panel::themed(&self.palette).level_at(inside.x);
        self.set_gauge(gauge, level);
        true
    }

    pub(super) fn release_gauge_panel(&mut self) {
        if let Some(panel) = self.gauge_panel.as_mut() {
            panel.held = false;
        }
    }

    pub(super) fn set_gauge(&mut self, gauge: Gauge, level: f64) {
        match gauge {
            Gauge::Brightness => {
                let level = level.max(0.05);
                if let Some(media) = self.media.as_ref() {
                    media.set_brightness(level);
                }
                self.take_light(level);
            }
            _ => self.set_volume(level),
        }
    }

    fn set_volume(&mut self, level: f64) {
        if let Some(media) = self.media.as_ref() {
            media.set_volume(level);
        }
        self.take_volume(level, false);
    }

    pub(super) fn scroll_gauges(&mut self, steps: f64) -> bool {
        let on = |rect: Option<Rect>| rect.is_some_and(|rect| rect.contains(self.pointer));
        let gauge = match self.gauge_panel.as_ref() {
            Some(panel) if panel.rect.contains(self.pointer) => panel.gauge,
            _ if on(self.speaker_rect()) => Gauge::Volume,
            _ if on(self.sun_rect()) => Gauge::Brightness,
            _ => return false,
        };
        if steps == 0.0 {
            return true;
        }
        let Some((level, _)) = self.gauge_level(gauge) else {
            return true;
        };
        let step = drifter_core::volume::STEP * steps.signum();
        let wanted = ((level + step) * 20.0).round() / 20.0;
        self.set_gauge(gauge, wanted.clamp(0.0, 1.0));
        true
    }

    pub(super) fn over_own_parts(&self) -> bool {
        self.gauge_panel
            .as_ref()
            .is_some_and(|panel| panel.rect.contains(self.pointer))
            || self.prompt_art.is_some()
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

    pub(super) fn tray_layout(&self, band: Rect) -> (usize, bool) {
        let stats = self.stats.as_ref().map(|(_, size)| size.x);
        self.tray.layout(
            band,
            self.tray_lead(),
            self.tray_right(band),
            self.icons.len(),
            stats,
        )
    }

    fn tray_right(&self, band: Rect) -> f64 {
        self.clock
            .as_ref()
            .map(|(_, _, rect)| rect.origin.x)
            .or_else(|| self.control_rect().map(|rect| rect.origin.x))
            .unwrap_or_else(|| self.tray.power_rect(band).origin.x)
    }

    pub(super) fn tray_icon_rect(&self, band: Rect, slot: usize) -> Rect {
        let shown = self.tray_layout(band).0;
        self.tray
            .icon_rect(band, self.tray_right(band), shown, slot)
    }

    pub(super) fn tray_icon_at(&self, band: Rect) -> Option<usize> {
        let shown = self.tray_layout(band).0;
        self.tray
            .icon_at(band, self.tray_right(band), self.pointer, shown)
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
                log!("drifter: {error}");
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
            .map(|font| font.paint_even_digits(&reading, tray.ink))
            .expect("the font was there a moment ago");
        let rect = tray.clock_rect(band, image.size());
        match upload(&mut self.renderer, Some(&image), "clock") {
            Ok(Some(buffer)) => self.clock = Some((reading, buffer, rect)),
            Ok(None) => {}
            Err(error) => log!("drifter: {error}"),
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
            Err(error) => log!("drifter: {error}"),
        }
    }
}
