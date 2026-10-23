use super::*;

const HIDE: i32 = 0;
const HIDE_OTHERS: i32 = 1;
const SHOW_ALL: i32 = 2;
const QUIT: i32 = 3;
const ABOUT: i32 = 4;
const SETTINGS: i32 = 5;

impl State {
    pub(super) fn front_app(&self) -> Option<(String, String)> {
        let id = self.focus?;
        let window = self
            .windows
            .iter()
            .find(|window| window.id == id && !window.hidden && !window.going)?;
        if window.app.is_empty() {
            return None;
        }
        let name = match window.app.as_str() {
            SETTINGS_APP => drifter_core::lang::t("Settings").to_string(),
            drifter_core::activity::ACTIVITY_APP => drifter_core::lang::t("Activity").to_string(),
            app => self
                .app_names
                .borrow_mut()
                .entry(app.to_string())
                .or_insert_with(|| {
                    crate::apps::find(app)
                        .and_then(|entry| entry.name)
                        .unwrap_or_else(|| app.to_string())
                })
                .clone(),
        };
        Some((window.app.clone(), name))
    }

    pub(super) fn wind_the_app_label(&mut self) {
        if !self.drawers[Edge::Top.index()].visible() {
            return;
        }
        let name = self
            .front_app()
            .map(|(_, name)| name)
            .unwrap_or_else(|| drifter_core::lang::t("Drifter").to_string());
        if self
            .app_label
            .as_ref()
            .is_some_and(|(shown, _, _)| *shown == name)
        {
            return;
        }
        let Some(font) = self.font.as_deref() else {
            return;
        };
        let image = font.paint_bold(&name, self.tray.ink);
        let size = drifter_core::paint::logical(&image);
        match upload(&mut self.renderer, Some(&image), "app name") {
            Ok(Some(buffer)) => self.app_label = Some((name, buffer, size)),
            Ok(None) => {}
            Err(error) => log!("drifter: {error}"),
        }
    }

    pub(super) fn app_label_rect(&self) -> Option<Rect> {
        let band = self.tray_band()?;
        let (_, _, size) = self.app_label.as_ref()?;
        Some(self.tray.app_rect(band, *size))
    }

    pub(super) fn tray_lead(&self) -> f64 {
        self.tray
            .lead(self.app_label.as_ref().map(|(_, _, size)| *size))
    }

    pub(super) fn press_app_label(&mut self) -> bool {
        let Some(rect) = self.app_label_rect().filter(|rect| {
            Rect::new(rect.origin - Vec2::splat(6.0), rect.size + Vec2::splat(12.0)).contains(self.pointer)
        }) else {
            return false;
        };
        let at = Vec2::new(rect.origin.x - 6.0, rect.max().y + 8.0);
        let items = match self.front_app() {
            Some((app, name)) => {
                use drifter_core::rows::Glyph;
                let icon = self.icon_now(&app, 32).map(std::sync::Arc::new);
                let items = vec![
                    Item::line(HIDE, &drifter_core::lang::tf("Hide {}", &[&(name)])).glyph(Glyph::Hide),
                    Item::line(HIDE_OTHERS, drifter_core::lang::t("Hide Others")).glyph(Glyph::HideOthers),
                    Item::line(SHOW_ALL, drifter_core::lang::t("Show All")).glyph(Glyph::ShowAll),
                    Item::rule(9),
                    Item::line(QUIT, &drifter_core::lang::tf("Quit {}", &[&(name)])).glyph(Glyph::Quit).icon(icon),
                ];
                self.open_popover(items, at, Target::App(app));
                return true;
            }
            None => vec![
                Item::line(ABOUT, drifter_core::lang::t("About Drifter")).glyph(drifter_core::rows::Glyph::About),
                Item::line(SETTINGS, drifter_core::lang::t("Settings")).glyph(drifter_core::rows::Glyph::Settings),
                Item::rule(9),
                Item::line(SHOW_ALL, drifter_core::lang::t("Show All")).glyph(drifter_core::rows::Glyph::ShowAll),
            ],
        };
        self.open_popover(items, at, Target::App(String::new()));
        true
    }

    pub(super) fn app_menu_pick(&mut self, app: &str, id: i32) {
        match id {
            ABOUT => self.open_settings_at("about"),
            SETTINGS => self.open_settings(),
            SHOW_ALL => {
                for window in self.windows.iter_mut() {
                    window.hidden = false;
                }
                self.wake_windows();
            }
            HIDE | HIDE_OTHERS => {
                let hide_mine = id == HIDE;
                let targets: Vec<usize> = self
                    .windows
                    .iter()
                    .enumerate()
                    .filter(|(_, window)| window.placed && !window.hidden && !window.going)
                    .filter(|(_, window)| (window.app == app) == hide_mine)
                    .map(|(at, _)| at)
                    .collect();
                for at in targets {
                    let seat = self.dock_slot(self.minimized().len());
                    self.windows[at].motion = Some(Motion::new(Phase::Minimizing, seat));
                    self.begin_stow(at);
                }
                if hide_mine {
                    let mine: Vec<u64> = self
                        .windows
                        .iter()
                        .filter(|window| window.app == app)
                        .map(|window| window.id)
                        .collect();
                    if let Some(first) = mine.first() {
                        self.focus_next(Some(*first));
                    }
                }
            }
            QUIT => {
                for window in self.windows.iter().filter(|window| window.app == app) {
                    window.toplevel.send_close();
                }
            }
            _ => {}
        }
    }
}
