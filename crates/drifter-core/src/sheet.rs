use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::desktop::App;
use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::settings::{Fonts, Look};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum For {
    Login,
    Dock,
    Folder,
    DockIcon(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Apps,
    Command,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub folder: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Sheet {
    Picker {
        purpose: For,
        tab: Tab,
        apps: Vec<App>,
        scroll: f64,
        text: String,
    },
    Files {
        purpose: For,
        folder: PathBuf,
        entries: Vec<Entry>,
        scroll: f64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pick {
    Cancel,
    Tab(Tab),
    App(usize),
    Command(String),
    Into(PathBuf),
    Folder(PathBuf),
    File(PathBuf),
    Default,
}

const ROW: f64 = 36.0;

pub fn entries(folder: &Path, folders_only: bool) -> Vec<Entry> {
    let Ok(read) = std::fs::read_dir(folder) else {
        return Vec::new();
    };
    let mut found: Vec<Entry> = read
        .flatten()
        .filter_map(|item| {
            let name = item.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                return None;
            }
            let path = item.path();
            let folder = path.is_dir();
            let wanted = folder || (!folders_only && crate::wallpaper::is_picture(&path));
            wanted.then_some(Entry { name, path, folder })
        })
        .collect();
    found.sort_by_key(|entry| (!entry.folder, entry.name.to_lowercase()));
    found
}

impl Sheet {
    pub fn picker(purpose: For, apps: Vec<App>) -> Sheet {
        Sheet::Picker {
            purpose,
            tab: Tab::Apps,
            apps,
            scroll: 0.0,
            text: String::new(),
        }
    }

    pub fn files(purpose: For, folder: PathBuf) -> Sheet {
        let entries = entries(&folder, purpose == For::Folder);
        Sheet::Files {
            purpose,
            folder,
            entries,
            scroll: 0.0,
        }
    }

    pub fn purpose(&self) -> For {
        match self {
            Sheet::Picker { purpose, .. } | Sheet::Files { purpose, .. } => *purpose,
        }
    }

    pub fn typing(&self) -> bool {
        matches!(
            self,
            Sheet::Picker {
                tab: Tab::Command,
                ..
            }
        )
    }

    pub fn type_text(&mut self, typed: &str) {
        if let Sheet::Picker {
            tab: Tab::Command,
            text,
            ..
        } = self
        {
            text.extend(typed.chars().filter(|c| !c.is_control()));
        }
    }

    pub fn backspace(&mut self) {
        if let Sheet::Picker {
            tab: Tab::Command,
            text,
            ..
        } = self
        {
            text.pop();
        }
    }

    pub fn enter(&self) -> Option<Pick> {
        match self {
            Sheet::Picker {
                tab: Tab::Command,
                text,
                ..
            } if !text.trim().is_empty() => Some(Pick::Command(text.trim().to_string())),
            _ => None,
        }
    }

    fn count(&self) -> usize {
        match self {
            Sheet::Picker {
                tab: Tab::Apps,
                apps,
                ..
            } => apps.len(),
            Sheet::Picker { .. } => 0,
            Sheet::Files { entries, .. } => entries.len(),
        }
    }

    fn scroll(&self) -> f64 {
        match self {
            Sheet::Picker { scroll, .. } | Sheet::Files { scroll, .. } => *scroll,
        }
    }

    pub fn scroll_by(&mut self, size: Vec2, by: f64) {
        let list = frame(size, self).list;
        let most = (self.count() as f64 * ROW - list.size.y).max(0.0);
        match self {
            Sheet::Picker { scroll, .. } | Sheet::Files { scroll, .. } => {
                *scroll = (*scroll + by).clamp(0.0, most);
            }
        }
    }

    fn primary(&self) -> Option<(&'static str, Pick)> {
        match self {
            Sheet::Picker {
                tab: Tab::Command,
                text,
                ..
            } => Some(("Add", Pick::Command(text.trim().to_string()))),
            Sheet::Picker { .. } => None,
            Sheet::Files {
                purpose: For::Folder,
                folder,
                ..
            } => Some(("Choose this folder", Pick::Folder(folder.clone()))),
            Sheet::Files {
                purpose: For::DockIcon(_),
                ..
            } => Some(("Use the app's icon", Pick::Default)),
            Sheet::Files { .. } => None,
        }
    }

    fn title(&self) -> &'static str {
        match self.purpose() {
            For::Login => "Start at login",
            For::Dock => "Add to the dock",
            For::Folder => "Choose a wallpaper folder",
            For::DockIcon(_) => "Choose an icon",
        }
    }

    pub fn hit(&self, size: Vec2, pointer: Vec2) -> Option<Pick> {
        let frame = frame(size, self);
        if frame.cancel.contains(pointer) {
            return Some(Pick::Cancel);
        }
        if let Some((_, pick)) = self.primary()
            && frame.primary.contains(pointer)
        {
            return match pick {
                Pick::Command(text) if text.is_empty() => None,
                pick => Some(pick),
            };
        }
        if let Some(tabs) = frame.tabs {
            let half = Rect::new(tabs.origin, Vec2::new(tabs.size.x / 2.0, tabs.size.y));
            if half.contains(pointer) {
                return Some(Pick::Tab(Tab::Apps));
            }
            if tabs.contains(pointer) {
                return Some(Pick::Tab(Tab::Command));
            }
        }
        if let (Some(up), Sheet::Files { folder, .. }) = (frame.up, self)
            && up.contains(pointer)
        {
            return folder
                .parent()
                .map(|parent| Pick::Into(parent.to_path_buf()));
        }
        if !frame.list.contains(pointer) {
            return None;
        }
        let index = ((pointer.y - frame.list.origin.y + self.scroll()) / ROW).floor() as usize;
        match self {
            Sheet::Picker {
                tab: Tab::Apps,
                apps,
                ..
            } if index < apps.len() => Some(Pick::App(index)),
            Sheet::Files { entries, .. } => entries.get(index).map(|entry| match entry.folder {
                true => Pick::Into(entry.path.clone()),
                false => Pick::File(entry.path.clone()),
            }),
            _ => None,
        }
    }

    pub fn paint(
        &self,
        canvas: &mut Canvas,
        look: &Look,
        fonts: &Fonts,
        icons: &HashMap<String, Image>,
    ) {
        let size = canvas.size();
        let frame = frame(size, self);
        canvas.round_rect(Rect::new(Vec2::ZERO, size), 0.0, [0, 0, 0], 0.45);
        canvas.shadow(frame.card, 12.0, 18.0, 0.5);
        canvas.round_rect(frame.card, 12.0, look.side, 1.0);

        let title = fonts.body.paint(self.title(), look.ink);
        canvas.stamp(&title, frame.card.origin + Vec2::new(20.0, 18.0), 1.0);

        if let (Some(tabs), Sheet::Picker { tab, .. }) = (frame.tabs, self) {
            canvas.round_rect(tabs, 7.0, look.off, 1.0);
            for (slot, (name, which)) in [("Applications", Tab::Apps), ("Command", Tab::Command)]
                .iter()
                .enumerate()
            {
                let cell = Rect::new(
                    Vec2::new(
                        tabs.origin.x + tabs.size.x / 2.0 * slot as f64,
                        tabs.origin.y,
                    ),
                    Vec2::new(tabs.size.x / 2.0, tabs.size.y),
                );
                let picked = which == tab;
                if picked {
                    let inner =
                        Rect::new(cell.origin + Vec2::splat(2.0), cell.size - Vec2::splat(4.0));
                    canvas.round_rect(inner, 5.0, look.pick, 1.0);
                }
                let text = fonts
                    .body
                    .paint(name, if picked { look.ink } else { look.dim });
                canvas.stamp(&text, cell.center() - text.size() / 2.0, 1.0);
            }
        }

        if let (Some(up), Sheet::Files { folder, .. }) = (frame.up, self) {
            button(canvas, look, fonts, up, "Up", false);
            let room = up.origin.x - frame.list.origin.x - 10.0;
            let shown = fit_left(fonts.small, &folder.to_string_lossy(), room);
            let path = fonts.small.paint(&shown, look.dim);
            let at = Vec2::new(
                frame.list.origin.x,
                up.center().y - f64::from(path.height) / 2.0,
            );
            canvas.stamp(&path, at, 1.0);
        }

        match self {
            Sheet::Picker {
                tab: Tab::Command,
                text,
                ..
            } => {
                let field = frame.field;
                canvas.round_rect(field, 7.0, look.rule, 1.0);
                canvas.round_rect(
                    Rect::new(
                        field.origin + Vec2::splat(1.0),
                        field.size - Vec2::splat(2.0),
                    ),
                    6.0,
                    look.back,
                    1.0,
                );
                let (shown, ink) = match text.is_empty() {
                    true => ("A command line, like foot -e htop", look.dim),
                    false => (text.as_str(), look.ink),
                };
                let room = field.size.x - 24.0;
                let painted = fonts.body.paint(&fit_left(fonts.body, shown, room), ink);
                let at = Vec2::new(
                    field.origin.x + 12.0,
                    field.center().y - f64::from(painted.height) / 2.0,
                );
                canvas.stamp(&painted, at, 1.0);
                let caret_x = match text.is_empty() {
                    true => at.x,
                    false => at.x + f64::from(painted.width) + 1.0,
                };
                let caret = Rect::new(
                    Vec2::new(caret_x, field.center().y - 9.0),
                    Vec2::new(1.5, 18.0),
                );
                canvas.round_rect(caret, 0.0, look.accent, 1.0);
                let note = fonts.small.paint(
                    "It runs as written, with no shell. Press Enter to add it.",
                    look.dim,
                );
                canvas.stamp(
                    &note,
                    field.origin + Vec2::new(4.0, field.size.y + 10.0),
                    1.0,
                );
            }
            _ => self.paint_list(canvas, look, fonts, icons, frame.list),
        }

        button(canvas, look, fonts, frame.cancel, "Cancel", false);
        if let Some((label, pick)) = self.primary() {
            let off = matches!(&pick, Pick::Command(text) if text.is_empty());
            button(canvas, look, fonts, frame.primary, label, !off);
        }
    }

    fn paint_list(
        &self,
        canvas: &mut Canvas,
        look: &Look,
        fonts: &Fonts,
        icons: &HashMap<String, Image>,
        list: Rect,
    ) {
        canvas.round_rect(list, 8.0, look.card, 1.0);
        let mut inside = Canvas::new(list.size.x as u32, list.size.y as u32);
        let scroll = self.scroll();
        let rows: Vec<(String, Option<String>, bool)> = match self {
            Sheet::Picker { apps, .. } => apps
                .iter()
                .map(|app| (app.name.clone(), Some(format!("app:{}", app.id)), false))
                .collect(),
            Sheet::Files { entries, .. } => entries
                .iter()
                .map(|entry| (entry.name.clone(), None, entry.folder))
                .collect(),
        };
        if rows.is_empty() {
            let empty = fonts.small.paint("Nothing here", look.dim);
            inside.stamp(&empty, list.size / 2.0 - empty.size() / 2.0, 1.0);
        }
        for (index, (name, icon, folder)) in rows.iter().enumerate() {
            let top = index as f64 * ROW - scroll;
            if top + ROW < 0.0 || top > list.size.y {
                continue;
            }
            if index > 0 {
                let line = Rect::new(Vec2::new(46.0, top), Vec2::new(list.size.x - 46.0, 1.0));
                inside.round_rect(line, 0.0, look.rule, 1.0);
            }
            let spot = Rect::new(Vec2::new(12.0, top + (ROW - 22.0) / 2.0), Vec2::splat(22.0));
            match icon.as_ref().and_then(|key| icons.get(key)) {
                Some(image) => inside.stamp(image, spot.center() - image.size() / 2.0, 1.0),
                None if *folder => {
                    inside.round_rect(
                        Rect::new(spot.origin + Vec2::new(0.0, 4.0), Vec2::new(22.0, 16.0)),
                        3.0,
                        [88, 140, 220],
                        1.0,
                    );
                    inside.round_rect(
                        Rect::new(spot.origin + Vec2::new(0.0, 2.0), Vec2::new(10.0, 5.0)),
                        2.0,
                        [88, 140, 220],
                        1.0,
                    );
                }
                None => letter(&mut inside, look, fonts, spot, name),
            }
            let text = fonts.body.paint(name, look.ink);
            let at = Vec2::new(46.0, top + (ROW - f64::from(text.height)) / 2.0);
            inside.stamp(&text, at, 1.0);
        }
        canvas.stamp(&inside.image, list.origin, 1.0);
    }
}

pub fn letter(canvas: &mut Canvas, look: &Look, fonts: &Fonts, spot: Rect, name: &str) {
    canvas.round_rect(spot, spot.size.x * 0.25, look.off, 1.0);
    let first: String = name.chars().take(1).collect::<String>().to_uppercase();
    let text = fonts.small.paint(&first, look.ink);
    canvas.stamp(&text, spot.center() - text.size() / 2.0, 1.0);
}

fn button(canvas: &mut Canvas, look: &Look, fonts: &Fonts, rect: Rect, label: &str, main: bool) {
    let color = if main { look.accent } else { look.off };
    canvas.round_rect(rect, 7.0, color, 1.0);
    let text = fonts.body.paint(label, look.ink);
    canvas.stamp(&text, rect.center() - text.size() / 2.0, 1.0);
}

fn fit_left(font: &crate::text::Font, text: &str, width: f64) -> String {
    if font.width_of(text) <= width {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    for start in 1..chars.len() {
        let tail: String = chars[start..].iter().collect();
        let shown = format!("…{tail}");
        if font.width_of(&shown) <= width {
            return shown;
        }
    }
    "…".to_string()
}

struct Frame {
    card: Rect,
    tabs: Option<Rect>,
    up: Option<Rect>,
    list: Rect,
    field: Rect,
    cancel: Rect,
    primary: Rect,
}

fn frame(size: Vec2, sheet: &Sheet) -> Frame {
    let wide = (size.x - 60.0).clamp(300.0, 500.0);
    let tall = (size.y - 60.0).clamp(300.0, 500.0);
    let card = Rect::new(
        ((size - Vec2::new(wide, tall)) / 2.0).round(),
        Vec2::new(wide, tall),
    );
    let inner_x = card.origin.x + 20.0;
    let inner_w = card.size.x - 40.0;
    let bar = card.max().y - 50.0;
    let tabs = matches!(sheet, Sheet::Picker { .. }).then(|| {
        Rect::new(
            Vec2::new(inner_x, card.origin.y + 50.0),
            Vec2::new(inner_w, 28.0),
        )
    });
    let up = matches!(sheet, Sheet::Files { .. }).then(|| {
        Rect::new(
            Vec2::new(card.max().x - 20.0 - 56.0, card.origin.y + 48.0),
            Vec2::new(56.0, 28.0),
        )
    });
    let top = card.origin.y + 90.0;
    let list = Rect::new(
        Vec2::new(inner_x, top),
        Vec2::new(inner_w, bar - 12.0 - top),
    );
    let field = Rect::new(Vec2::new(inner_x, top + 10.0), Vec2::new(inner_w, 36.0));
    let cancel = Rect::new(Vec2::new(inner_x, bar), Vec2::new(90.0, 30.0));
    let primary_w = 170.0;
    let primary = Rect::new(
        Vec2::new(card.max().x - 20.0 - primary_w, bar),
        Vec2::new(primary_w, 30.0),
    );
    Frame {
        card,
        tabs,
        up,
        list,
        field,
        cancel,
        primary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Vec2 = Vec2::new(860.0, 620.0);

    fn apps() -> Vec<App> {
        (0..30)
            .map(|n| App {
                id: format!("app{n}"),
                name: format!("App {n}"),
                icon: None,
                exec: Some(format!("app{n}")),
            })
            .collect()
    }

    #[test]
    fn a_click_on_a_row_picks_that_app() {
        let sheet = Sheet::picker(For::Login, apps());
        let list = frame(SIZE, &sheet).list;
        let second = Vec2::new(list.center().x, list.origin.y + ROW * 1.5);
        assert_eq!(sheet.hit(SIZE, second), Some(Pick::App(1)));
    }

    #[test]
    fn scrolling_moves_the_rows_and_stops_at_the_end() {
        let mut sheet = Sheet::picker(For::Login, apps());
        sheet.scroll_by(SIZE, ROW * 3.0);
        let list = frame(SIZE, &sheet).list;
        let first = Vec2::new(list.center().x, list.origin.y + ROW * 0.5);
        assert_eq!(sheet.hit(SIZE, first), Some(Pick::App(3)));
        sheet.scroll_by(SIZE, 1e9);
        let bottom = Vec2::new(list.center().x, list.max().y - 1.0);
        assert_eq!(sheet.hit(SIZE, bottom), Some(Pick::App(29)));
    }

    #[test]
    fn the_command_tab_takes_typing_and_enter_adds_it() {
        let mut sheet = Sheet::picker(For::Dock, apps());
        sheet.type_text("ignored");
        let tabs = frame(SIZE, &sheet).tabs.unwrap();
        let command = Vec2::new(tabs.max().x - 10.0, tabs.center().y);
        assert_eq!(sheet.hit(SIZE, command), Some(Pick::Tab(Tab::Command)));
        if let Sheet::Picker { tab, .. } = &mut sheet {
            *tab = Tab::Command;
        }
        assert_eq!(sheet.enter(), None);
        sheet.type_text("foot -e htop\n");
        sheet.backspace();
        sheet.type_text("p");
        assert_eq!(sheet.enter(), Some(Pick::Command("foot -e htop".into())));
    }

    #[test]
    fn an_empty_command_cannot_be_added() {
        let mut sheet = Sheet::picker(For::Dock, apps());
        if let Sheet::Picker { tab, .. } = &mut sheet {
            *tab = Tab::Command;
        }
        let add = frame(SIZE, &sheet).primary.center();
        assert_eq!(sheet.hit(SIZE, add), None);
    }

    #[test]
    fn folders_come_first_and_hidden_files_stay_hidden() {
        let base = std::env::temp_dir().join(format!("drifter-sheet-{}", std::process::id()));
        std::fs::create_dir_all(base.join("zeta")).unwrap();
        std::fs::create_dir_all(base.join(".secret")).unwrap();
        std::fs::write(base.join("a.png"), b"x").unwrap();
        std::fs::write(base.join("b.txt"), b"x").unwrap();
        let names =
            |list: Vec<Entry>| -> Vec<String> { list.into_iter().map(|e| e.name).collect() };
        assert_eq!(names(entries(&base, false)), ["zeta", "a.png"]);
        assert_eq!(names(entries(&base, true)), ["zeta"]);
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn the_folder_browser_goes_in_and_up() {
        let base = std::env::temp_dir().join(format!("drifter-walk-{}", std::process::id()));
        std::fs::create_dir_all(base.join("inner")).unwrap();
        let sheet = Sheet::files(For::Folder, base.clone());
        let frame_now = frame(SIZE, &sheet);
        let first = Vec2::new(
            frame_now.list.center().x,
            frame_now.list.origin.y + ROW * 0.5,
        );
        assert_eq!(sheet.hit(SIZE, first), Some(Pick::Into(base.join("inner"))));
        let up = frame_now.up.unwrap().center();
        assert_eq!(
            sheet.hit(SIZE, up),
            Some(Pick::Into(base.parent().unwrap().to_path_buf()))
        );
        let choose = frame_now.primary.center();
        assert_eq!(sheet.hit(SIZE, choose), Some(Pick::Folder(base.clone())));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn a_long_path_keeps_its_end() {
        let font = crate::text::Font::find(None, 12.0).expect("a font");
        let shown = fit_left(
            &font,
            "/home/someone/a/very/long/path/to/the/wallpapers",
            120.0,
        );
        assert!(shown.starts_with('…') && shown.ends_with("wallpapers"));
    }
}
