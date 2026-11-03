use super::*;
use drifter_core::cascade::{Cascade, Entry, Key, Kind, Outcome};
use drifter_core::power::Action;

const OPEN_DELAY: Duration = Duration::from_millis(140);
const PANEL_FADE: f64 = 0.12;
use drifter_core::menu::{GROW, GROW_WAIT, grown};

#[derive(Clone)]
pub(super) enum Pick {
    App(String),
    Wallpaper,
    Settings,
    Power(Action),
}

pub(super) struct DeskMenu {
    pub menu: Cascade,
    pub picks: Vec<Pick>,
    pub arts: Vec<(TextureBuffer<Tex>, Rect)>,
    pub keys: Vec<LevelKey>,
    pub opened: Vec<Instant>,
    pub anchor: Vec2,
    pub pending: Option<(usize, usize, Instant)>,
    pub lane: worker::Lane,
}

type LevelKey = (usize, usize, Option<usize>, u64, [i64; 4]);

pub(super) struct Ripple {
    pub snapshot: smithay::backend::renderer::gles::GlesTexture,
    pub sheet: super::wallpaper::Sheet,
    pub size: (i32, i32),
    pub drops: Vec<(Vec2, Instant)>,
}

pub(super) struct Alert {
    pub action: Action,
    pub ticked: bool,
    pub since: Instant,
    pub art: Option<TextureBuffer<Tex>>,
    pub pill: Option<TextureBuffer<Tex>>,
    pub rect: Rect,
    pub lane: worker::Lane,
}

impl State {
    pub(super) fn cascade_look(&self) -> drifter_core::cascade::Look {
        let plain = drifter_core::cascade::Look::themed(&self.palette, self.look.accent);
        drifter_core::cascade::Look {
            back_alpha: self.veil(plain.back_alpha),
            ..plain
        }
    }

    fn small_icon(&mut self, name: &str) -> Option<drifter_core::Image> {
        self.icon_now(name, 18)
    }

    pub(super) fn refresh_desk_icons(&mut self) {
        if self.desk_menu.is_none() {
            return;
        }
        let mut picks = Vec::new();
        let entries = self.desk_entries(&mut picks);
        if let Some(open) = self.desk_menu.as_mut() {
            open.menu.root = entries;
            open.picks = picks;
            open.keys.clear();
        }
        self.paint_desk_menu();
    }

    fn desk_entries(&mut self, picks: &mut Vec<Pick>) -> Vec<Entry> {
        let pick = |picks: &mut Vec<Pick>, what: Pick| {
            picks.push(what);
            (picks.len() - 1) as u32
        };
        let installed = self.installed_apps();
        let mut groups = Vec::new();
        for (name, members) in drifter_core::desktop::grouped(&installed) {
            let mut apps = Vec::new();
            for app in members {
                let icon = app.icon.clone().and_then(|icon| self.small_icon(&icon));
                let id = pick(picks, Pick::App(app.id.clone()));
                apps.push(Entry::action(id, &app.name).with_icon(icon));
            }
            groups.push(Entry::submenu(name, apps));
        }
        vec![
            Entry::submenu(drifter_core::lang::t("Applications"), groups),
            Entry::action(pick(picks, Pick::Wallpaper), drifter_core::lang::t("Change Wallpaper…")),
            Entry::rule(),
            Entry::action(pick(picks, Pick::Settings), drifter_core::lang::t("Settings…")),
            Entry::rule(),
            Entry::action(pick(picks, Pick::Power(Action::Lock)), drifter_core::lang::t("Lock Screen")),
            Entry::action(pick(picks, Pick::Power(Action::Logout)), drifter_core::lang::t("Log Out…")),
            Entry::rule(),
            Entry::action(pick(picks, Pick::Power(Action::Suspend)), drifter_core::lang::t("Sleep")),
            Entry::action(pick(picks, Pick::Power(Action::Reboot)), drifter_core::lang::t("Restart…")),
            Entry::action(pick(picks, Pick::Power(Action::Shutdown)), drifter_core::lang::t("Shut Down…")),
        ]
    }

    pub(super) fn open_desk_menu(&mut self, at: Vec2) {
        self.popover = None;
        self.control = None;
        self.notes_panel = None;
        self.charges = None;
        self.glance_panel = None;
        self.start_ripple(at);
        let mut picks = Vec::new();
        let entries = self.desk_entries(&mut picks);
        self.desk_menu = Some(DeskMenu {
            menu: Cascade::new(entries, at + Vec2::new(4.0, 4.0)),
            picks,
            arts: Vec::new(),
            keys: Vec::new(),
            opened: Vec::new(),
            anchor: at,
            pending: None,
            lane: worker::Lane::default(),
        });
        self.paint_desk_menu();
    }

    fn wallpaper_snapshot(&mut self) -> Option<(smithay::backend::renderer::gles::GlesTexture, (i32, i32))> {
        let mut paper = self.wallpaper.take()?;
        let shot = self.shoot_paper(&mut paper);
        self.wallpaper = Some(paper);
        shot
    }

    pub(super) fn shoot_paper(&mut self, paper: &mut Backdrop) -> Option<(smithay::backend::renderer::gles::GlesTexture, (i32, i32))> {
        use smithay::backend::renderer::Bind;
        use smithay::backend::renderer::damage::OutputDamageTracker;
        use smithay::utils::{Buffer as BufferCoord, Physical, Size};

        let scale = self.heads.get(self.at).map_or(1.0, |head| head.scale);
        let view = (self.mode, self.screen, self.world, self.camera);
        let clock = self.paper_clock();
        let mut pieces = Vec::new();
        paper_pieces(&mut self.renderer, paper, view, 1.0, (clock, None), &mut pieces);
        let shown: Vec<Shown> = pieces
            .into_iter()
            .map(|piece| RescaleRenderElement::from_element(piece, (0, 0).into(), scale))
            .collect();
        let whole = self.physical();
        let size = Size::<i32, Physical>::from((whole.x as i32, whole.y as i32));
        let buffer_size = Size::<i32, BufferCoord>::from((size.w, size.h));
        let mut canvas = self.renderer.canvas(DrmFourcc::Abgr8888, buffer_size).ok()?;
        {
            let mut target = self.renderer.bind(&mut canvas).ok()?;
            let mut tracker = OutputDamageTracker::new(size, 1.0, Transform::Normal);
            tracker.render_output(&mut self.renderer, &mut target, 0, &shown, self.clear).ok()?;
        }
        match canvas {
            crate::paint::Canvas::Gles(texture) => Some((texture, (size.w, size.h))),
            _ => None,
        }
    }

    fn start_ripple(&mut self, at: Vec2) {
        let wanted = self.ripple_on && !self.window_still && self.window_pace > 0.0 && !self.renderer.is_pixman();
        if !wanted {
            return;
        }
        if let Some(ripple) = self.ripple.as_mut() {
            if ripple.drops.len() >= drifter_core::ripple::DROPS {
                ripple.drops.remove(0);
            }
            ripple.drops.push((at, Instant::now()));
            return;
        }
        if self.ripple_program.is_none() {
            let program = super::wallpaper::ripple_program(&mut self.renderer);
            self.ripple_program = Some(program);
        }
        if !matches!(self.ripple_program, Some(Some(_))) {
            return;
        }
        let Some((snapshot, size)) = self.wallpaper_snapshot() else {
            return;
        };
        let Ok(sheet) = super::wallpaper::sheet(&mut self.renderer, size) else {
            return;
        };
        self.ripple = Some(Ripple {
            snapshot,
            sheet,
            size,
            drops: vec![(at, Instant::now())],
        });
    }

    pub(super) fn wind_the_ripple(&mut self) {
        let Some(ripple) = self.ripple.as_mut() else {
            return;
        };
        ripple.drops.retain(|(_, since)| since.elapsed().as_secs_f64() < drifter_core::ripple::SECONDS);
        if ripple.drops.is_empty() {
            self.ripple = None;
            return;
        }
        let Some(Some(program)) = self.ripple_program.as_ref() else {
            self.ripple = None;
            return;
        };
        let look = (self.screen, self.ripple_strength as f32);
        if let Err(error) = super::wallpaper::paint_ripple(program, &mut self.renderer, ripple, look) {
            log!("drifter: no ripple: {error}");
            self.ripple = None;
        }
    }

    pub(super) fn paint_desk_menu(&mut self) {
        let Some(font) = self.note_head.clone() else {
            return;
        };
        let look = self.cascade_look();
        let screen = self.screen;
        let frosting = self.frosting();
        let Some(open) = self.desk_menu.as_mut() else {
            return;
        };
        if !open.lane.claim() {
            return;
        }
        let id = open.lane.id();
        let menu = open.menu.clone();
        let had = open.keys.clone();
        self.on_worker(
            "desktop menu",
            move || {
                let rects = menu.panels(&look, &font, screen);
                rects
                    .iter()
                    .enumerate()
                    .map(|(level, rect)| {
                        let items = menu.items(level);
                        let key: LevelKey = (
                            level,
                            items.len(),
                            menu.hover[level],
                            menu.scroll[level].to_bits(),
                            [rect.origin.x, rect.origin.y, rect.size.x, rect.size.y].map(|v| v.round() as i64),
                        );
                        let image = (had.get(level) != Some(&key)).then(|| {
                            let mut image = look.paint(items, rect.size, menu.hover[level], menu.scroll[level], &font);
                            worker::frost(&frosting, &mut image, rect.origin, look.back_alpha);
                            image
                        });
                        (key, image, *rect)
                    })
                    .collect::<Vec<_>>()
            },
            move |state, levels| {
                let Some(open) = state.desk_menu.as_mut().filter(|open| open.lane.id() == id) else {
                    return;
                };
                let rerun = open.lane.release();
                let held: Vec<(LevelKey, (TextureBuffer<Tex>, Rect))> =
                    open.keys.iter().cloned().zip(open.arts.iter().cloned()).collect();
                let mut arts = Vec::new();
                let mut keys = Vec::new();
                for (key, image, rect) in levels {
                    let art = match image {
                        Some(image) => upload(&mut state.renderer, Some(&image), "desktop menu")
                            .ok()
                            .flatten()
                            .map(|art| (art, rect)),
                        None => held.iter().find(|(had, _)| *had == key).map(|(_, art)| art.clone()),
                    };
                    if let Some(art) = art {
                        arts.push(art);
                        keys.push(key);
                    }
                }
                if let Some(open) = state.desk_menu.as_mut() {
                    open.keys = keys;
                    let levels = open.menu.levels();
                    open.opened.truncate(levels);
                    while open.opened.len() < levels {
                        open.opened.push(Instant::now());
                    }
                    open.arts = arts;
                }
                if rerun {
                    state.paint_desk_menu();
                }
                state.kick_now();
            },
        );
    }

    pub(super) fn desk_menu_elements(&self, elements: &mut Vec<Skin>) {
        let Some(open) = self.desk_menu.as_ref() else {
            return;
        };
        let instant = self.window_pace <= 0.0;
        let fade_only = self.window_still;
        let round = self.cascade_look().radius;
        for (level, (art, rect)) in open.arts.iter().enumerate().rev() {
            let gone = open.opened.get(level).map_or(f64::MAX, |at| at.elapsed().as_secs_f64());
            let (drop, shown) = match (level, instant) {
                (_, true) => (*rect, 1.0),
                (0, false) => grown(*rect, open.anchor, gone, false, fade_only),
                (_, false) if fade_only => (*rect, (gone / PANEL_FADE).min(1.0)),
                _ => {
                    let shown = (gone / PANEL_FADE).min(1.0);
                    (Rect::new(rect.origin + Vec2::new(0.0, 6.0 * (1.0 - shown)), rect.size), shown)
                }
            };
            if shown <= 0.0 {
                continue;
            }
            self.menu_skin(art, drop, round, shown as f32, elements);
        }
    }

    pub(super) fn menu_skin(&self, art: &TextureBuffer<Tex>, drop: Rect, round: f64, shown: f32, elements: &mut Vec<Skin>) {
        elements.push(Skin::Texture(element(art, drop, shown)));
        self.panel_shadow(drop, fade::Shadow::Menu(round), shown, elements);
    }

    pub(super) fn desk_menu_busy(&self) -> bool {
        self.ripple.is_some()
            || self.desk_menu.as_ref().is_some_and(|open| {
                open.pending.is_some()
                    || open
                        .opened
                        .iter()
                        .any(|at| at.elapsed().as_secs_f64() < GROW_WAIT + GROW)
            })
    }

    pub(super) fn hover_desk_menu(&mut self) {
        let Some(font) = self.note_head.as_deref() else {
            return;
        };
        let look = self.cascade_look();
        let screen = self.screen;
        let pointer = self.pointer;
        let Some(open) = self.desk_menu.as_mut() else {
            return;
        };
        let Some((level, index)) = open.menu.hit(&look, font, screen, pointer) else {
            return;
        };
        let mut changed = open.menu.point(level, index);
        if level + 1 < open.menu.levels() && open.menu.path.get(level).copied() == index {
            open.pending = None;
        } else {
            match index {
                Some(at) => {
                    let opens = matches!(open.menu.items(level)[at].kind, Kind::Submenu(_));
                    let deeper = level + 1 < open.menu.levels();
                    if opens || deeper {
                        if open.pending.map(|(l, i, _)| (l, i)) != Some((level, at)) {
                            open.pending = Some((level, at, Instant::now()));
                        }
                    } else {
                        open.pending = None;
                    }
                }
                None => open.pending = None,
            }
        }
        if level + 1 < open.menu.levels() {
            changed |= open.menu.point(level + 1, open.menu.hover[level + 1]);
        }
        if changed {
            self.paint_desk_menu();
        }
    }

    pub(super) fn wind_the_desk_menu(&mut self) {
        let due = self
            .desk_menu
            .as_ref()
            .and_then(|open| open.pending)
            .filter(|(_, _, since)| since.elapsed() >= OPEN_DELAY);
        if let Some((level, index, _)) = due
            && let Some(open) = self.desk_menu.as_mut()
        {
            open.pending = None;
            if open.menu.hover.get(level).copied().flatten() == Some(index) {
                open.menu.open(level, index);
                self.paint_desk_menu();
            }
        }
    }

    pub(super) fn press_desk_menu(&mut self) -> bool {
        if self.alert.is_some() {
            return self.press_alert();
        }
        let Some(font) = self.note_head.as_deref() else {
            return false;
        };
        let look = self.cascade_look();
        let screen = self.screen;
        let pointer = self.pointer;
        let Some(open) = self.desk_menu.as_mut() else {
            return false;
        };
        match open.menu.hit(&look, font, screen, pointer) {
            None => {
                self.desk_menu = None;
                self.kick_now();
                false
            }
            Some((_, None)) => true,
            Some((level, Some(at))) => {
                match open.menu.items(level)[at].kind.clone() {
                    Kind::Submenu(_) => {
                        open.pending = None;
                        open.menu.open(level, at);
                        self.paint_desk_menu();
                    }
                    Kind::Action(id) => self.activate_desk(id),
                    _ => {}
                }
                true
            }
        }
    }

    pub(super) fn scroll_desk_menu(&mut self, steps: f64) -> bool {
        let Some(font) = self.note_head.as_deref() else {
            return false;
        };
        let look = self.cascade_look();
        let screen = self.screen;
        let pointer = self.pointer;
        let Some(open) = self.desk_menu.as_mut() else {
            return false;
        };
        let Some((level, _)) = open.menu.hit(&look, font, screen, pointer) else {
            return true;
        };
        if open.menu.scroll_by(&look, font, screen, level, steps * look.item * 2.0) {
            self.paint_desk_menu();
        }
        true
    }

    pub(super) fn desk_menu_key(&mut self, key: LockKey) {
        if self.alert.is_some() {
            match key {
                LockKey::Enter => self.confirm_alert(),
                LockKey::Clear => {
                    self.alert = None;
                    self.kick_now();
                }
                _ => {}
            }
            return;
        }
        let key = match key {
            LockKey::Up => Key::Up,
            LockKey::Down => Key::Down,
            LockKey::Left => Key::Left,
            LockKey::Right => Key::Right,
            LockKey::Enter => Key::Enter,
            LockKey::Clear => Key::Escape,
            LockKey::Letter(letter) => Key::Letter(letter),
            LockKey::Vt(vt) => {
                self.change_vt(vt);
                return;
            }
            LockKey::Back | LockKey::Other => return,
        };
        let Some(open) = self.desk_menu.as_mut() else {
            return;
        };
        open.pending = None;
        match open.menu.key(key) {
            Outcome::Nothing => {}
            Outcome::Changed => self.paint_desk_menu(),
            Outcome::Activate(id) => self.activate_desk(id),
            Outcome::Close => {
                self.desk_menu = None;
                self.kick_now();
            }
        }
    }

    fn activate_desk(&mut self, id: u32) {
        let Some(pick) = self
            .desk_menu
            .take()
            .and_then(|open| open.picks.get(id as usize).cloned())
        else {
            return;
        };
        self.kick_now();
        match pick {
            Pick::App(app) => {
                crate::apps::run(&drifter_core::Launch::App(app.clone()), &self.socket);
                self.bounce(&app);
            }
            Pick::Wallpaper => self.open_settings_at("wallpaper"),
            Pick::Settings => self.open_settings(),
            Pick::Power(Action::Lock) => {
                self.lock_now();
            }
            Pick::Power(Action::Suspend) => {
                if self.lock_now() {
                    crate::power::run(Action::Suspend);
                }
            }
            Pick::Power(action) => self.ask_alert(action),
        }
    }

    pub(super) fn alert_look(&self) -> drifter_core::alert::Look {
        let plain = drifter_core::alert::Look::themed(&self.palette, self.look.accent).with_tick();
        drifter_core::alert::Look {
            back_alpha: self.veil(plain.back_alpha),
            ..plain
        }
    }

    pub(super) fn ask_alert(&mut self, action: Action) {
        let look = self.alert_look();
        let since = Instant::now();
        let pill = upload(&mut self.renderer, Some(&look.pill()), "alert bar").ok().flatten();
        self.alert = Some(Alert {
            action,
            ticked: drifter_core::reopen::wanted(),
            since,
            art: None,
            pill,
            rect: look.rect(self.screen),
            lane: worker::Lane::default(),
        });
        self.paint_alert();
        let _ = self.handle.insert_source(
            Timer::from_duration(Duration::from_secs(self.power_rules.countdown)),
            move |_, _, state: &mut State| {
                if state.alert.as_ref().is_some_and(|open| open.since == since) {
                    state.confirm_alert();
                }
                TimeoutAction::Drop
            },
        );
    }

    pub(super) fn alert_left(&self) -> Option<f64> {
        let open = self.alert.as_ref()?;
        if self.window_pace <= 0.0 {
            return Some(1.0);
        }
        let countdown = self.power_rules.countdown.max(1) as f64;
        Some((1.0 - open.since.elapsed().as_secs_f64() / countdown).clamp(0.0, 1.0))
    }

    pub(super) fn paint_alert(&mut self) {
        let (Some(font), Some(bold)) = (self.note_head.clone(), self.note_head.clone()) else {
            self.alert = None;
            return;
        };
        let Some(open) = self.alert.as_ref() else {
            return;
        };
        let (title, verb) = match open.action {
            Action::Logout => (drifter_core::lang::t("Log out now?"), drifter_core::lang::t("Log Out")),
            Action::Reboot => (drifter_core::lang::t("Restart now?"), drifter_core::lang::t("Restart")),
            _ => (drifter_core::lang::t("Shut down now?"), drifter_core::lang::t("Shut Down")),
        };
        let message = open.action.notice();
        let ticked = open.ticked;
        let rect = open.rect;
        let look = self.alert_look();
        let frosting = self.frosting();
        self.paint_in_lane(
            "alert",
            |state| state.alert.as_mut().map(|open| &mut open.lane),
            |_| {},
            move || {
                let mut image = look.paint(title, &message, verb, ticked, &font, &bold);
                worker::frost(&frosting, &mut image, rect.origin, look.back_alpha);
                Some(image)
            },
            |state, art| {
                if let Some(open) = state.alert.as_mut() {
                    open.art = Some(art);
                }
            },
        );
    }

    fn press_alert(&mut self) -> bool {
        let Some(open) = self.alert.as_ref() else {
            return false;
        };
        let look = self.alert_look();
        match look.hit(self.pointer - open.rect.origin) {
            Some(drifter_core::alert::Press::Confirm) => self.confirm_alert(),
            Some(drifter_core::alert::Press::Tick) => {
                if let Some(open) = self.alert.as_mut() {
                    open.ticked = !open.ticked;
                }
                self.paint_alert();
                self.kick_now();
            }
            Some(drifter_core::alert::Press::Cancel) => {
                self.alert = None;
                self.kick_now();
            }
            None => {}
        }
        true
    }

    fn confirm_alert(&mut self) {
        let Some(open) = self.alert.take() else {
            return;
        };
        self.kick_now();
        drifter_core::reopen::want(open.ticked);
        if open.ticked {
            self.save_desk();
        }
        match open.action {
            Action::Logout => self.signal.stop(),
            action => crate::power::run(action),
        }
    }

    pub(super) fn alert_elements(&self, elements: &mut Vec<Skin>) {
        self.pane_elements(fade::Pane::Alert, elements);
    }
}
