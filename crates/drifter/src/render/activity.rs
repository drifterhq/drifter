use super::*;
use drifter_core::activity::glance::{self, Hit as GlanceHit, Layout, Top};
use drifter_core::activity::{Gone, Signal};

pub(super) struct GlancePanel {
    pub rect: Rect,
    pub art: Option<TextureBuffer<Tex>>,
    pub tops: Vec<Top>,
    pub counts: (usize, usize),
    pub open: Option<usize>,
    pub hover: Option<usize>,
    pub undo: Option<Gone>,
    pub lane: worker::Lane,
    pub pinned: bool,
    pub ready: bool,
    pub over_open: bool,
}

impl State {
    fn glance_layout(&self) -> Layout {
        let plain = drifter_core::glance::Card::themed(&self.palette);
        let card = drifter_core::glance::Card {
            width: glance::WIDTH,
            back_alpha: self.veil(0.62),
            ..plain
        };
        Layout {
            card,
            rows: self.glance_fields.len(),
            spark: self.cpu_history.len() > 1,
        }
    }

    pub(super) fn toggle_glance(&mut self) {
        if let Some(panel) = self.glance_panel.as_mut() {
            if !panel.pinned {
                panel.pinned = true;
                return;
            }
            self.glance_panel = None;
            self.sampler.watch(false);
            self.kick_now();
            return;
        }
        self.open_glance(true);
    }

    fn open_glance(&mut self, pinned: bool) {
        self.close_panels();
        self.glance_card = None;
        self.sampler.watch(true);
        self.glance_panel = Some(GlancePanel {
            rect: Rect::new(Vec2::ZERO, Vec2::ZERO),
            art: None,
            tops: Vec::new(),
            counts: (0, 0),
            open: None,
            hover: None,
            undo: None,
            lane: worker::Lane::default(),
            pinned,
            ready: false,
            over_open: false,
        });
        self.refresh_glance();
    }

    pub(super) fn refresh_glance(&mut self) {
        let Some(groups) = self.sampler.groups() else {
            self.paint_glance();
            return;
        };
        let (groups, total) = groups;
        let held = self.glance_panel.as_ref().is_some_and(|panel| panel.rect.contains(self.pointer));
        if let Some(panel) = self.glance_panel.as_mut() {
            panel.ready = true;
            let fresh = glance::pick(&groups, total);
            panel.counts = (
                groups.iter().filter(|group| matches!(group.kind, drifter_core::activity::Kind::App | drifter_core::activity::Kind::Drifter)).count(),
                groups.iter().filter(|group| group.kind != drifter_core::activity::Kind::App && group.kind != drifter_core::activity::Kind::Drifter).count(),
            );
            panel.tops = match held && !panel.tops.is_empty() {
                true => panel
                    .tops
                    .iter()
                    .filter_map(|old| {
                        groups
                            .iter()
                            .find(|group| group.key == old.key)
                            .map(Top::of)
                    })
                    .collect(),
                false => fresh,
            };
            if panel.open.is_some_and(|at| at >= panel.tops.len()) {
                panel.open = None;
            }
        }
        self.paint_glance();
    }

    pub(super) fn paint_glance(&mut self) {
        let (Some(font), Some(bold)) = (self.card_font.clone(), self.font.clone()) else {
            return;
        };
        let Some(spot) = self.stats_spot() else {
            return;
        };
        let layout = self.glance_layout();
        let names: Vec<String> = self
            .glance_panel
            .as_ref()
            .map(|panel| panel.tops.iter().filter_map(|top| top.icon.clone()).collect())
            .unwrap_or_default();
        let mut icons = std::collections::HashMap::new();
        for name in names {
            if let Some(image) = self.icon_now(&name, glance::ICON) {
                icons.insert(name, image);
            }
        }
        let x = (spot.origin.x - 8.0).clamp(8.0, (self.screen.x - layout.card.width - 8.0).max(8.0));
        let y = self.tray_band().map_or(spot.max().y, |band| band.max().y) + 8.0;
        let Some(panel) = self.glance_panel.as_mut() else {
            return;
        };
        let height = layout.height(panel.tops.len(), panel.open);
        let rect = Rect::new(Vec2::new(x, y).round(), Vec2::new(layout.card.width, height.ceil()));
        panel.rect = rect;
        let tops = panel.tops.clone();
        let (open, hover, counts, ready, over_open) = (panel.open, panel.hover, panel.counts, panel.ready, panel.over_open);
        let undo = panel.undo.as_ref().map(|gone| gone.name.clone());
        let fields = self.glance_fields.clone();
        let history = self.cpu_history.clone();
        let accent = self.look.accent;
        let frosting = self.frosting();
        let solid = layout.card.back_alpha;
        self.paint_in_lane(
            "activity glance",
            |state| state.glance_panel.as_mut().map(|panel| &mut panel.lane),
            State::paint_glance,
            move || {
                let paint = glance::Paint {
                    font: &font,
                    bold: &bold,
                    fields: &fields,
                    history: &history,
                    tops: &tops,
                    open,
                    hover,
                    icons: &icons,
                    counts,
                    accent,
                    undo: undo.as_deref(),
                    ready,
                    over_open,
                };
                let mut image = glance::paint(&layout, &paint);
                worker::frost(&frosting, &mut image, rect.origin, solid);
                Some(image)
            },
            |state, art| {
                if let Some(panel) = state.glance_panel.as_mut() {
                    panel.art = Some(art);
                }
            },
        );
    }

    pub(super) fn hover_glance(&mut self) {
        let on_dot = self.stats_spot().is_some_and(|spot| spot.contains(self.pointer));
        let busy = self.lock.is_some() || self.popover.is_some() || self.control.is_some() || self.notes_panel.is_some();
        if self.glance_panel.is_none() {
            if on_dot && !busy {
                self.open_glance(false);
            }
            return;
        }
        if let (Some(panel), Some(spot)) = (self.glance_panel.as_ref(), self.stats_spot())
            && !panel.pinned
        {
            let left = spot.origin.x.min(panel.rect.origin.x);
            let right = spot.max().x.max(panel.rect.max().x);
            let near = Rect::new(Vec2::new(left, spot.origin.y), Vec2::new(right - left, panel.rect.max().y - spot.origin.y));
            if !near.contains(self.pointer) {
                self.glance_panel = None;
                self.sampler.watch(false);
                self.kick_now();
                return;
            }
        }
        let Some(font) = self.card_font.clone() else {
            return;
        };
        let layout = self.glance_layout();
        let pointer = self.pointer;
        let Some(panel) = self.glance_panel.as_mut() else {
            return;
        };
        let local = pointer - panel.rect.origin;
        let undo = panel.undo.as_ref().map(|gone| gone.name.clone());
        let hit = match panel.rect.contains(pointer) {
            true => layout.hit(panel.tops.len(), panel.open, &font, undo.as_deref(), local),
            false => None,
        };
        let hover = match hit {
            Some(GlanceHit::Row(at)) => Some(at),
            _ => None,
        };
        let over_open = hit == Some(GlanceHit::Open);
        if hover != panel.hover || over_open != panel.over_open {
            panel.hover = hover;
            panel.over_open = over_open;
            self.paint_glance();
        }
    }

    pub(super) fn press_glance(&mut self, code: u32) -> bool {
        let Some(rect) = self.glance_panel.as_ref().map(|panel| panel.rect) else {
            return false;
        };
        if !rect.contains(self.pointer) {
            let on_dot = self.stats_spot().is_some_and(|spot| spot.contains(self.pointer));
            self.glance_panel = None;
            self.sampler.watch(false);
            self.kick_now();
            return on_dot;
        }
        if code != BTN_LEFT {
            return true;
        }
        let Some(font) = self.card_font.clone() else {
            return true;
        };
        let layout = self.glance_layout();
        let Some(panel) = self.glance_panel.as_mut() else {
            return true;
        };
        let undo = panel.undo.as_ref().map(|gone| gone.name.clone());
        let hit = layout.hit(panel.tops.len(), panel.open, &font, undo.as_deref(), self.pointer - rect.origin);
        match hit {
            Some(GlanceHit::Row(at)) => {
                panel.open = match panel.open == Some(at) {
                    true => None,
                    false => Some(at),
                };
            }
            Some(GlanceHit::Pause(at)) => {
                if let Some(top) = panel.tops.get_mut(at).filter(|top| top.can_stop) {
                    let signal = if top.paused { Signal::Resume } else { Signal::Pause };
                    let pids: Vec<String> = top.pids.iter().map(u32::to_string).collect();
                    match drifter_core::activity::send_to(signal, &pids) {
                        Ok(()) => {
                            top.paused = signal == Signal::Pause;
                            top.detail = if top.paused { drifter_core::lang::t("Paused").to_string() } else { drifter_core::lang::t("Running").to_string() };
                        }
                        Err(error) => log!("drifter: cannot pause {}: {error}", top.name),
                    }
                }
            }
            Some(GlanceHit::Quit(at)) => {
                if let Some(top) = panel.tops.get(at).filter(|top| top.can_stop).cloned() {
                    let argv = std::fs::read(format!("/proc/{}/cmdline", top.root))
                        .map(|bytes| drifter_core::activity::parse_argv(&bytes))
                        .unwrap_or_default();
                    let cwd = std::fs::read_link(format!("/proc/{}/cwd", top.root))
                        .ok()
                        .map(|path| path.to_string_lossy().into_owned());
                    if top.paused {
                        let pids: Vec<String> = top.pids.iter().map(u32::to_string).collect();
                        let _ = drifter_core::activity::send_to(Signal::Resume, &pids);
                    }
                    match drifter_core::activity::send_to(Signal::Quit, &[top.root.to_string()]) {
                        Ok(()) => {
                            panel.undo = (!argv.is_empty()).then(|| Gone { name: top.name.clone(), argv, cwd });
                            panel.tops.remove(at);
                            panel.open = None;
                        }
                        Err(error) => log!("drifter: cannot quit {}: {error}", top.name),
                    }
                }
            }
            Some(GlanceHit::Details(at)) => {
                let key = panel.tops.get(at).map(|top| top.key.clone());
                self.glance_panel = None;
                self.sampler.watch(false);
                self.open_activity_with(key.as_deref());
                self.kick_now();
                return true;
            }
            Some(GlanceHit::Open) => {
                self.glance_panel = None;
                self.sampler.watch(false);
                self.open_activity_with(None);
                self.kick_now();
                return true;
            }
            Some(GlanceHit::Reopen) => {
                if let Some(gone) = panel.undo.take() {
                    crate::apps::launch_in(&gone.argv, gone.cwd.as_deref(), &self.socket);
                }
            }
            None => return true,
        }
        self.paint_glance();
        true
    }

    pub(super) fn open_activity_with(&mut self, key: Option<&str>) {
        if self.lock.is_some() {
            return;
        }
        let running = self
            .windows
            .iter()
            .position(|window| window.app == drifter_core::activity::ACTIVITY_APP && !window.going);
        let starting = self.activity_launched.is_some_and(|at| at.elapsed() < Duration::from_secs(4));
        if running.is_some() || starting {
            if let Some(key) = key {
                let path = drifter_core::Config::state_dir().join("activity-show");
                let _ = std::fs::create_dir_all(drifter_core::Config::state_dir());
                if let Err(error) = std::fs::write(&path, key) {
                    log!("drifter: cannot point Activity at the app: {error}");
                }
            }
            if let Some(place) = running {
                self.bring(place);
            }
            return;
        }
        self.activity_launched = Some(Instant::now());
        let program = std::env::current_exe()
            .map(|path| path.with_file_name(drifter_core::activity::ACTIVITY_APP))
            .unwrap_or_else(|_| std::path::PathBuf::from(drifter_core::activity::ACTIVITY_APP));
        let mut argv = vec![program.to_string_lossy().into_owned()];
        argv.extend(key.map(str::to_string));
        crate::apps::launch(&argv, &self.socket);
    }
}
