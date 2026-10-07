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
    Default(crate::desktop::Role),
    Layout,
    Variant,
    Picture,
    Pointer,
    Font,
    WindowArt,
    DockArt,
    IconTheme,
    CursorTheme,
    Output,
    Input,
    Zone,
    Shortcut(usize),
    Import,
    Export,
    ShotFolder,
    Wifi(usize),
    NetInfo(usize),
    MyName,
    MyPassword,
    MyFace,
    AddUser,
    NewFace,
    RemoveUser(usize),
    MakeAdmin(usize),
    PlaceName(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub code: String,
    pub name: String,
}

impl Choice {
    pub fn new(code: &str, name: &str) -> Choice {
        Choice {
            code: code.to_string(),
            name: name.to_string(),
        }
    }
}

pub fn matching<'a>(items: &'a [Choice], text: &str) -> Vec<&'a Choice> {
    let wanted = text.trim().to_lowercase();
    items
        .iter()
        .filter(|item| {
            wanted.is_empty()
                || item.name.to_lowercase().contains(&wanted)
                || item.code.to_lowercase() == wanted
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Folders,
    Pictures,
    Fonts,
    Configs,
}

impl For {
    fn kind(self) -> Kind {
        match self {
            For::Folder | For::WindowArt | For::DockArt | For::Export | For::ShotFolder => {
                Kind::Folders
            }
            For::Font => Kind::Fonts,
            For::Import => Kind::Configs,
            _ => Kind::Pictures,
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub label: &'static str,
    pub text: String,
    pub secret: bool,
    pub follows: bool,
}

impl Field {
    pub fn plain(label: &'static str, text: &str) -> Field {
        Field {
            label,
            text: text.to_string(),
            secret: false,
            follows: false,
        }
    }

    pub fn secret(label: &'static str) -> Field {
        Field {
            secret: true,
            ..Field::plain(label, "")
        }
    }

    pub fn following(label: &'static str) -> Field {
        Field {
            follows: true,
            ..Field::plain(label, "")
        }
    }
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
    Choices {
        purpose: For,
        items: Vec<Choice>,
        scroll: f64,
        text: String,
    },
    Capture {
        purpose: For,
        title: String,
        now: String,
        note: Option<String>,
    },
    Secret {
        purpose: For,
        title: String,
        text: String,
    },
    Info {
        purpose: For,
        title: String,
        lines: Vec<(String, String)>,
        scroll: f64,
    },
    Form {
        purpose: For,
        title: String,
        fields: Vec<Field>,
        focus: usize,
        switch: Option<(&'static str, bool)>,
        face: Option<Option<PathBuf>>,
        note: Option<String>,
        busy: bool,
        go: &'static str,
    },
    Faces {
        purpose: For,
        stock: Vec<PathBuf>,
        chosen: Option<PathBuf>,
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
    Choice(String),
    Focus(usize),
    Toggle,
    Face,
    Browse,
    Submit { texts: Vec<String>, on: bool },
}

const ROW: f64 = 36.0;
const LINE: f64 = 26.0;
const FIELD: f64 = 58.0;
const SWITCH: f64 = 38.0;
const AVATAR: f64 = 72.0;
const NOTE: f64 = 34.0;
pub const FACE_CELL: f64 = 72.0;
const FACE_GAP: f64 = 14.0;

pub fn face_key(path: &Path, px: u32) -> String {
    format!("face:{px}:{}", path.display())
}

pub fn face_of(key: &str) -> Option<(u32, PathBuf)> {
    let rest = key.strip_prefix("face:")?;
    let (px, path) = rest.split_once(':')?;
    Some((px.parse().ok()?, PathBuf::from(path)))
}

pub fn is_font(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| matches!(ext.to_lowercase().as_str(), "ttf" | "otf"))
}

pub fn entries(folder: &Path, kind: Kind) -> Vec<Entry> {
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
            let wanted = folder
                || match kind {
                    Kind::Folders => false,
                    Kind::Pictures => crate::wallpaper::is_picture(&path),
                    Kind::Fonts => is_font(&path),
                    Kind::Configs => path.extension().is_some_and(|ext| ext == "toml"),
                };
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
        let entries = entries(&folder, purpose.kind());
        Sheet::Files {
            purpose,
            folder,
            entries,
            scroll: 0.0,
        }
    }

    pub fn choices(purpose: For, items: Vec<Choice>) -> Sheet {
        Sheet::Choices {
            purpose,
            items,
            scroll: 0.0,
            text: String::new(),
        }
    }

    pub fn capture(at: usize, title: &str, now: &str) -> Sheet {
        Sheet::Capture {
            purpose: For::Shortcut(at),
            title: title.to_string(),
            now: now.to_string(),
            note: None,
        }
    }

    pub fn secret(purpose: For, title: &str) -> Sheet {
        Sheet::Secret {
            purpose,
            title: title.to_string(),
            text: String::new(),
        }
    }

    pub fn info(purpose: For, title: &str, lines: Vec<(String, String)>) -> Sheet {
        Sheet::Info {
            purpose,
            title: title.to_string(),
            lines,
            scroll: 0.0,
        }
    }

    pub fn form(purpose: For, title: &str, fields: Vec<Field>, go: &'static str) -> Sheet {
        Sheet::Form {
            purpose,
            title: title.to_string(),
            fields,
            focus: 0,
            switch: None,
            face: None,
            note: None,
            busy: false,
            go,
        }
    }

    pub fn with_switch(mut self, label: &'static str, on: bool) -> Sheet {
        if let Sheet::Form { switch, .. } = &mut self {
            *switch = Some((label, on));
        }
        self
    }

    pub fn with_face(mut self, picture: Option<PathBuf>) -> Sheet {
        if let Sheet::Form { face, .. } = &mut self {
            *face = Some(picture);
        }
        self
    }

    pub fn with_note(mut self, words: &str) -> Sheet {
        if let Sheet::Form { note, .. } = &mut self {
            *note = Some(words.to_string());
        }
        self
    }

    pub fn faces(purpose: For, stock: Vec<PathBuf>, chosen: Option<PathBuf>) -> Sheet {
        Sheet::Faces {
            purpose,
            stock,
            chosen,
            scroll: 0.0,
        }
    }

    pub fn work(&mut self, words: Option<&str>, working: bool) {
        if let Sheet::Form { note, busy, .. } = self {
            *note = words.map(str::to_string);
            *busy = working;
        }
    }

    pub fn put_face(&mut self, picture: PathBuf) {
        if let Sheet::Form { face: Some(face), .. } = self {
            *face = Some(picture);
        }
    }

    pub fn face(&self) -> Option<PathBuf> {
        match self {
            Sheet::Form { face: Some(face), .. } => face.clone(),
            _ => None,
        }
    }

    pub fn focus(&mut self, at: usize) {
        if let Sheet::Form { fields, focus, .. } = self
            && at < fields.len()
        {
            *focus = at;
        }
    }

    pub fn next_field(&mut self) {
        if let Sheet::Form { fields, focus, .. } = self
            && !fields.is_empty()
        {
            *focus = (*focus + 1) % fields.len();
        }
    }

    pub fn toggle(&mut self) {
        if let Sheet::Form {
            switch: Some((_, on)),
            ..
        } = self
        {
            *on = !*on;
        }
    }

    fn submit(&self) -> Option<Pick> {
        match self {
            Sheet::Form {
                fields,
                switch,
                busy: false,
                ..
            } => Some(Pick::Submit {
                texts: fields.iter().map(|field| field.text.clone()).collect(),
                on: switch.is_some_and(|(_, on)| on),
            }),
            _ => None,
        }
    }

    fn follow(&mut self) {
        if let Sheet::Form { fields, .. } = self
            && let Some(source) = fields.first().map(|field| field.text.clone())
        {
            for field in fields.iter_mut().skip(1).filter(|field| field.follows) {
                field.text = crate::users::login_from(&source);
            }
        }
    }

    pub fn purpose(&self) -> For {
        match self {
            Sheet::Picker { purpose, .. }
            | Sheet::Files { purpose, .. }
            | Sheet::Choices { purpose, .. }
            | Sheet::Capture { purpose, .. }
            | Sheet::Secret { purpose, .. }
            | Sheet::Info { purpose, .. }
            | Sheet::Form { purpose, .. }
            | Sheet::Faces { purpose, .. } => *purpose,
        }
    }

    pub fn tell(&mut self, words: &str) {
        if let Sheet::Capture { note, .. } = self {
            *note = Some(words.to_string());
        }
    }

    pub fn typing(&self) -> bool {
        matches!(
            self,
            Sheet::Picker {
                tab: Tab::Command,
                ..
            } | Sheet::Choices { .. }
                | Sheet::Secret { .. }
                | Sheet::Form { .. }
        )
    }

    fn text_mut(&mut self) -> Option<&mut String> {
        match self {
            Sheet::Picker {
                tab: Tab::Command,
                text,
                ..
            } => Some(text),
            Sheet::Choices { text, scroll, .. } => {
                *scroll = 0.0;
                Some(text)
            }
            Sheet::Secret { text, .. } => Some(text),
            Sheet::Form {
                fields,
                focus,
                busy: false,
                ..
            } => fields.get_mut(*focus).map(|field| {
                field.follows = false;
                &mut field.text
            }),
            _ => None,
        }
    }

    pub fn type_text(&mut self, typed: &str) {
        if let Some(text) = self.text_mut() {
            text.extend(typed.chars().filter(|c| !c.is_control()));
        }
        self.follow();
    }

    pub fn backspace(&mut self) {
        if let Some(text) = self.text_mut() {
            text.pop();
        }
        self.follow();
    }

    pub fn enter(&self) -> Option<Pick> {
        match self {
            Sheet::Picker {
                tab: Tab::Command,
                text,
                ..
            } if !text.trim().is_empty() => Some(Pick::Command(text.trim().to_string())),
            Sheet::Choices { items, text, .. } => matching(items, text)
                .first()
                .map(|item| Pick::Choice(item.code.clone())),
            Sheet::Secret { text, .. } if !text.is_empty() => Some(Pick::Command(text.clone())),
            Sheet::Form { fields, focus, .. } if focus + 1 < fields.len() => {
                Some(Pick::Focus(focus + 1))
            }
            Sheet::Form { .. } => self.submit(),
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
            Sheet::Choices { items, text, .. } => matching(items, text).len(),
            Sheet::Capture { .. } | Sheet::Secret { .. } | Sheet::Form { .. } => 0,
            Sheet::Info { lines, .. } => lines.len(),
            Sheet::Faces { stock, .. } => stock.len(),
        }
    }

    fn content_height(&self, list: Rect) -> f64 {
        match self {
            Sheet::Faces { stock, .. } => {
                let (_, down) = face_grid(list.size.x, stock.len());
                down as f64 * (FACE_CELL + FACE_GAP) + FACE_GAP
            }
            _ => self.count() as f64 * self.row_height(),
        }
    }

    fn row_height(&self) -> f64 {
        match self {
            Sheet::Info { .. } => LINE,
            _ => ROW,
        }
    }

    fn scroll(&self) -> f64 {
        match self {
            Sheet::Picker { scroll, .. }
            | Sheet::Files { scroll, .. }
            | Sheet::Choices { scroll, .. }
            | Sheet::Info { scroll, .. }
            | Sheet::Faces { scroll, .. } => *scroll,
            Sheet::Capture { .. } | Sheet::Secret { .. } | Sheet::Form { .. } => 0.0,
        }
    }

    pub fn scroll_by(&mut self, size: Vec2, by: f64) {
        let list = frame(size, self).list;
        let most = (self.content_height(list) - list.size.y).max(0.0);
        match self {
            Sheet::Picker { scroll, .. }
            | Sheet::Files { scroll, .. }
            | Sheet::Choices { scroll, .. }
            | Sheet::Info { scroll, .. }
            | Sheet::Faces { scroll, .. } => {
                *scroll = (*scroll + by).clamp(0.0, most);
            }
            Sheet::Capture { .. } | Sheet::Secret { .. } | Sheet::Form { .. } => {}
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
                purpose: For::Folder | For::WindowArt | For::DockArt,
                folder,
                ..
            } => Some(("Choose this folder", Pick::Folder(folder.clone()))),
            Sheet::Files {
                purpose: For::Export,
                folder,
                ..
            } => Some(("Save here", Pick::Folder(folder.clone()))),
            Sheet::Files {
                purpose: For::DockIcon(_),
                ..
            } => Some(("Use the app's icon", Pick::Default)),
            Sheet::Capture { .. } => Some(("No shortcut", Pick::Default)),
            Sheet::Secret { text, .. } => Some(("Join", Pick::Command(text.clone()))),
            Sheet::Form { go, .. } => self.submit().map(|pick| (*go, pick)),
            Sheet::Faces { .. } => Some(("Choose a file…", Pick::Browse)),
            Sheet::Files { .. } | Sheet::Choices { .. } | Sheet::Info { .. } => None,
        }
    }

    fn cancel_label(&self) -> &'static str {
        match self {
            Sheet::Info { .. } => "Close",
            _ => "Cancel",
        }
    }

    fn title(&self) -> &str {
        if let Sheet::Capture { title, .. }
        | Sheet::Secret { title, .. }
        | Sheet::Info { title, .. }
        | Sheet::Form { title, .. } = self
        {
            return title;
        }
        match self.purpose() {
            For::Login => "Start at login",
            For::Dock => "Add to the dock",
            For::Folder => "Choose a wallpaper folder",
            For::ShotFolder => "Choose where screenshots go",
            For::DockIcon(_) => "Choose an icon",
            For::Default(crate::desktop::Role::Files) => "Choose the file manager",
            For::Default(crate::desktop::Role::Terminal) => "Choose the terminal",
            For::Default(crate::desktop::Role::Browser) => "Choose the web browser",
            For::Default(crate::desktop::Role::Editor) => "Choose the text editor",
            For::Default(crate::desktop::Role::Mail) => "Choose the mail app",
            For::Default(crate::desktop::Role::Images) => "Choose the photo viewer",
            For::Default(crate::desktop::Role::Video) => "Choose the video player",
            For::Default(crate::desktop::Role::Music) => "Choose the music player",
            For::Layout => "Choose a keyboard layout",
            For::Variant => "Choose a variant",
            For::Picture => "Choose a wallpaper",
            For::Pointer => "Choose a pointer picture",
            For::Font => "Choose a font",
            For::WindowArt => "Choose the window art folder",
            For::DockArt => "Choose the dock art folder",
            For::IconTheme => "Choose an icon theme",
            For::CursorTheme => "Choose a cursor theme",
            For::Output => "Choose a speaker or headphones",
            For::Input => "Choose a microphone",
            For::Zone => "Choose a time zone",
            For::Shortcut(_) => "Press the new shortcut",
            For::Import => "Choose a settings file",
            For::Export => "Choose where to save the settings",
            For::Wifi(_) => "Join a network",
            For::NetInfo(_) => "Network details",
            For::MyName => "Change your full name",
            For::MyPassword => "Change your password",
            For::MyFace | For::NewFace => "Choose a picture",
            For::AddUser => "Add a user",
            For::RemoveUser(_) => "Remove a user",
            For::MakeAdmin(_) => "Change a user",
            For::PlaceName(_) => "Rename this place",
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
        if let Sheet::Form { fields, .. } = self {
            let form = form_frame(frame.card, self);
            if let Some(at) = form.fields.iter().position(|rect| rect.contains(pointer)) {
                return (at < fields.len()).then_some(Pick::Focus(at));
            }
            if form.switch.is_some_and(|rect| rect.contains(pointer)) {
                return Some(Pick::Toggle);
            }
            if form.face.is_some_and(|rect| rect.contains(pointer)) {
                return Some(Pick::Face);
            }
            return None;
        }
        if let Sheet::Faces { stock, scroll, .. } = self {
            if !frame.list.contains(pointer) {
                return None;
            }
            let inside = pointer - frame.list.origin + Vec2::new(0.0, *scroll);
            return face_cells(frame.list.size.x, stock.len())
                .iter()
                .position(|cell| cell.contains(inside))
                .and_then(|at| stock.get(at))
                .map(|path| Pick::File(path.clone()));
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
            Sheet::Choices { items, text, .. } => matching(items, text)
                .get(index)
                .map(|item| Pick::Choice(item.code.clone())),
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
                canvas.stamp(&text, cell.center() - crate::paint::logical(&text) / 2.0, 1.0);
            }
        }

        if let (Some(up), Sheet::Files { folder, .. }) = (frame.up, self) {
            button(canvas, look, fonts, up, "Up", false);
            let room = up.origin.x - frame.list.origin.x - 10.0;
            let shown = fit_left(fonts.small, &folder.to_string_lossy(), room);
            let path = fonts.small.paint(&shown, look.dim);
            let at = Vec2::new(
                frame.list.origin.x,
                up.center().y - crate::paint::logical(&path).y / 2.0,
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
                    field.center().y - crate::paint::logical(&painted).y / 2.0,
                );
                canvas.stamp(&painted, at, 1.0);
                let caret_x = match text.is_empty() {
                    true => at.x,
                    false => at.x + crate::paint::logical(&painted).x + 1.0,
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
            Sheet::Choices { text, .. } => {
                search(canvas, look, fonts, frame.field, text);
                self.paint_list(canvas, look, fonts, icons, frame.list);
            }
            Sheet::Secret { text, .. } => {
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
                let dots = "•".repeat(text.chars().count());
                let (shown, ink) = match text.is_empty() {
                    true => ("Password", look.dim),
                    false => (dots.as_str(), look.ink),
                };
                let room = field.size.x - 24.0;
                let painted = fonts.body.paint(&fit_left(fonts.body, shown, room), ink);
                let at = Vec2::new(
                    field.origin.x + 12.0,
                    field.center().y - crate::paint::logical(&painted).y / 2.0,
                );
                canvas.stamp(&painted, at, 1.0);
                let caret_x = match text.is_empty() {
                    true => at.x,
                    false => at.x + crate::paint::logical(&painted).x + 1.0,
                };
                let caret = Rect::new(
                    Vec2::new(caret_x, field.center().y - 9.0),
                    Vec2::new(1.5, 18.0),
                );
                canvas.round_rect(caret, 0.0, look.accent, 1.0);
                let note = fonts
                    .small
                    .paint("Type the network password, then press Enter.", look.dim);
                canvas.stamp(
                    &note,
                    field.origin + Vec2::new(4.0, field.size.y + 10.0),
                    1.0,
                );
            }
            Sheet::Capture { now, note, .. } => {
                let list = frame.list;
                canvas.round_rect(list, 8.0, look.card, 1.0);
                let shown = match now.is_empty() {
                    true => "None".to_string(),
                    false => now.clone(),
                };
                let keys = fonts.title.paint(&shown, look.ink);
                canvas.stamp(&keys, list.center() - crate::paint::logical(&keys) / 2.0, 1.0);
                let lines = [
                    "Hold Ctrl, Alt, Shift or Super and press a key.",
                    "Esc keeps the old shortcut.",
                ];
                for (at, line) in lines.iter().enumerate() {
                    let text = fonts.small.paint(line, look.dim);
                    let y = list.origin.y + 18.0 + at as f64 * 18.0;
                    canvas.stamp(
                        &text,
                        Vec2::new(list.center().x - crate::paint::logical(&text).x / 2.0, y),
                        1.0,
                    );
                }
                if let Some(note) = note {
                    let text = fonts.small.paint(note, look.accent);
                    let y = list.max().y - 30.0;
                    canvas.stamp(
                        &text,
                        Vec2::new(list.center().x - crate::paint::logical(&text).x / 2.0, y),
                        1.0,
                    );
                }
            }
            Sheet::Info { lines, scroll, .. } => paint_info(canvas, look, fonts, frame.list, lines, *scroll),
            Sheet::Form { .. } => self.paint_form(canvas, look, fonts, icons, frame.card),
            Sheet::Faces {
                stock,
                chosen,
                scroll,
                ..
            } => paint_faces(
                canvas,
                look,
                fonts,
                icons,
                frame.list,
                stock,
                chosen.as_deref(),
                *scroll,
            ),
            _ => self.paint_list(canvas, look, fonts, icons, frame.list),
        }

        button(canvas, look, fonts, frame.cancel, self.cancel_label(), false);
        if let Some((label, pick)) = self.primary() {
            let off = matches!(&pick, Pick::Command(text) if text.is_empty());
            button(canvas, look, fonts, frame.primary, label, !off);
        } else if let Sheet::Form { go, .. } = self {
            button(canvas, look, fonts, frame.primary, go, false);
        }
    }

    fn paint_form(
        &self,
        canvas: &mut Canvas,
        look: &Look,
        fonts: &Fonts,
        icons: &HashMap<String, Image>,
        card: Rect,
    ) {
        let Sheet::Form {
            fields,
            focus,
            switch,
            face,
            note,
            busy,
            ..
        } = self
        else {
            return;
        };
        let form = form_frame(card, self);
        if let (Some(spot), Some(picture)) = (form.face, face) {
            paint_face(canvas, look, icons, spot, picture.as_deref());
            let hint = fonts
                .small
                .paint("Click the picture to choose another.", look.dim);
            let at = Vec2::new(
                spot.max().x + 16.0,
                spot.center().y - crate::paint::logical(&hint).y / 2.0,
            );
            canvas.stamp(&hint, at, 1.0);
        }
        for (at, (field, rect)) in fields.iter().zip(&form.fields).enumerate() {
            let label = fonts.small.paint(field.label, look.dim);
            canvas.stamp(&label, rect.origin - Vec2::new(-2.0, 20.0), 1.0);
            let mine = at == *focus && !busy;
            canvas.round_rect(*rect, 7.0, if mine { look.accent } else { look.rule }, 1.0);
            let inner = Rect::new(rect.origin + Vec2::splat(1.5), rect.size - Vec2::splat(3.0));
            canvas.round_rect(inner, 6.0, look.back, 1.0);
            let dots = "•".repeat(field.text.chars().count());
            let shown = match field.secret {
                true => dots.as_str(),
                false => field.text.as_str(),
            };
            let room = rect.size.x - 24.0;
            let painted = fonts.body.paint(&fit_left(fonts.body, shown, room), look.ink);
            let left = Vec2::new(
                rect.origin.x + 12.0,
                rect.center().y - crate::paint::logical(&painted).y / 2.0,
            );
            if !shown.is_empty() {
                canvas.stamp(&painted, left, 1.0);
            }
            if mine {
                let caret_x = match shown.is_empty() {
                    true => left.x,
                    false => left.x + crate::paint::logical(&painted).x + 1.0,
                };
                let caret = Rect::new(
                    Vec2::new(caret_x, rect.center().y - 8.0),
                    Vec2::new(1.5, 16.0),
                );
                canvas.round_rect(caret, 0.0, look.accent, 1.0);
            }
        }
        if let (Some(row), Some((label, on))) = (form.switch, switch) {
            let text = fonts.body.paint(
                &fit_left(fonts.body, label, row.size.x - 70.0),
                look.ink,
            );
            canvas.stamp(
                &text,
                Vec2::new(row.origin.x + 2.0, row.center().y - crate::paint::logical(&text).y / 2.0),
                1.0,
            );
            let pill = Rect::new(
                Vec2::new(row.max().x - look.switch.x, row.center().y - look.switch.y / 2.0),
                look.switch,
            );
            canvas.round_rect(pill, pill.size.y, if *on { look.accent } else { look.off }, 1.0);
            let knob = pill.size.y / 2.0 - 2.0;
            let cx = match on {
                true => pill.max().x - 2.0 - knob,
                false => pill.origin.x + 2.0 + knob,
            };
            canvas.circle(Vec2::new(cx, pill.center().y), knob, [255, 255, 255], 1.0);
        }
        if let Some(note) = note {
            let room = form.note.size.x;
            let text = fonts.small.paint(&fit_left(fonts.small, note, room), look.accent);
            canvas.stamp(&text, form.note.origin, 1.0);
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
            Sheet::Choices { items, text, .. } => matching(items, text)
                .into_iter()
                .map(|item| (item.name.clone(), None, false))
                .collect(),
            Sheet::Capture { .. }
            | Sheet::Secret { .. }
            | Sheet::Info { .. }
            | Sheet::Form { .. }
            | Sheet::Faces { .. } => Vec::new(),
        };
        if rows.is_empty() {
            let empty = fonts.small.paint("Nothing here", look.dim);
            inside.stamp(&empty, list.size / 2.0 - crate::paint::logical(&empty) / 2.0, 1.0);
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
                Some(image) => inside.stamp(image, spot.center() - crate::paint::logical(&image) / 2.0, 1.0),
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
            let at = Vec2::new(46.0, top + (ROW - crate::paint::logical(&text).y) / 2.0);
            inside.stamp(&text, at, 1.0);
        }
        canvas.stamp(&inside.image, list.origin, 1.0);
    }
}

pub fn paint_face(
    canvas: &mut Canvas,
    look: &Look,
    icons: &HashMap<String, Image>,
    spot: Rect,
    picture: Option<&Path>,
) {
    let found = picture.and_then(|path| icons.get(&face_key(path, spot.size.x as u32)));
    match found {
        Some(image) => canvas.stamp(image, spot.center() - crate::paint::logical(&image) / 2.0, 1.0),
        None => {
            let r = spot.size.x / 2.0;
            canvas.circle(spot.center(), r, look.off, 1.0);
            canvas.circle(spot.center() - Vec2::new(0.0, r * 0.22), r * 0.3, look.dim, 1.0);
            let body = Rect::new(
                spot.center() + Vec2::new(-r * 0.5, r * 0.18),
                Vec2::new(r, r * 0.5),
            );
            canvas.round_rect(body, r * 0.25, look.dim, 1.0);
        }
    }
}

fn face_grid(width: f64, count: usize) -> (usize, usize) {
    let across = (((width - FACE_GAP) / (FACE_CELL + FACE_GAP)).floor() as usize).max(1);
    (across, count.div_ceil(across))
}

fn face_cells(width: f64, count: usize) -> Vec<Rect> {
    let (across, _) = face_grid(width, count);
    let used = across as f64 * (FACE_CELL + FACE_GAP) - FACE_GAP;
    let left = ((width - used) / 2.0).floor();
    (0..count)
        .map(|at| {
            let (col, line) = (at % across, at / across);
            Rect::new(
                Vec2::new(
                    left + col as f64 * (FACE_CELL + FACE_GAP),
                    FACE_GAP + line as f64 * (FACE_CELL + FACE_GAP),
                ),
                Vec2::splat(FACE_CELL),
            )
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn paint_faces(
    canvas: &mut Canvas,
    look: &Look,
    fonts: &Fonts,
    icons: &HashMap<String, Image>,
    list: Rect,
    stock: &[PathBuf],
    chosen: Option<&Path>,
    scroll: f64,
) {
    canvas.round_rect(list, 8.0, look.card, 1.0);
    let mut inside = Canvas::new(list.size.x as u32, list.size.y as u32);
    if stock.is_empty() {
        let empty = fonts
            .small
            .paint("No pictures here yet. Choose a file instead.", look.dim);
        inside.stamp(&empty, list.size / 2.0 - crate::paint::logical(&empty) / 2.0, 1.0);
    }
    for (path, cell) in stock.iter().zip(face_cells(list.size.x, stock.len())) {
        let cell = Rect::new(cell.origin - Vec2::new(0.0, scroll), cell.size);
        if cell.max().y < -8.0 || cell.origin.y > list.size.y + 8.0 {
            continue;
        }
        if chosen == Some(path.as_path()) {
            inside.circle(cell.center(), FACE_CELL / 2.0 + 4.0, look.accent, 1.0);
            inside.circle(cell.center(), FACE_CELL / 2.0 + 1.5, look.card, 1.0);
        }
        paint_face(&mut inside, look, icons, cell, Some(path));
    }
    canvas.stamp(&inside.image, list.origin, 1.0);
}

pub fn letter(canvas: &mut Canvas, look: &Look, fonts: &Fonts, spot: Rect, name: &str) {
    canvas.round_rect(spot, spot.size.x * 0.25, look.off, 1.0);
    let first: String = name.chars().take(1).collect::<String>().to_uppercase();
    let text = fonts.small.paint(&first, look.ink);
    canvas.stamp(&text, spot.center() - crate::paint::logical(&text) / 2.0, 1.0);
}

fn search(canvas: &mut Canvas, look: &Look, fonts: &Fonts, field: Rect, text: &str) {
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
        true => ("Type to search", look.dim),
        false => (text, look.ink),
    };
    let room = field.size.x - 24.0;
    let painted = fonts.body.paint(&fit_left(fonts.body, shown, room), ink);
    let at = Vec2::new(
        field.origin.x + 12.0,
        field.center().y - crate::paint::logical(&painted).y / 2.0,
    );
    canvas.stamp(&painted, at, 1.0);
    let caret_x = match text.is_empty() {
        true => at.x,
        false => at.x + crate::paint::logical(&painted).x + 1.0,
    };
    let caret = Rect::new(
        Vec2::new(caret_x, field.center().y - 8.0),
        Vec2::new(1.5, 16.0),
    );
    canvas.round_rect(caret, 0.0, look.accent, 1.0);
}

fn paint_info(
    canvas: &mut Canvas,
    look: &Look,
    fonts: &Fonts,
    list: Rect,
    lines: &[(String, String)],
    scroll: f64,
) {
    canvas.round_rect(list, 8.0, look.card, 1.0);
    let mut inside = Canvas::new(list.size.x as u32, list.size.y as u32);
    let key_w = 150.0;
    for (index, (key, value)) in lines.iter().enumerate() {
        let top = index as f64 * LINE - scroll;
        if top + LINE < 0.0 || top > list.size.y {
            continue;
        }
        if value.is_empty() {
            let text = fonts.small.paint(key, look.accent);
            let at = Vec2::new(12.0, top + LINE - crate::paint::logical(&text).y - 4.0);
            inside.stamp(&text, at, 1.0);
            continue;
        }
        let label = fonts.small.paint(&fit_left(fonts.small, key, key_w - 20.0), look.dim);
        inside.stamp(&label, Vec2::new(12.0, top + (LINE - crate::paint::logical(&label).y) / 2.0), 1.0);
        let room = list.size.x - key_w - 12.0;
        let text = fonts.small.paint(&fit_left(fonts.small, value, room), look.ink);
        inside.stamp(&text, Vec2::new(key_w, top + (LINE - crate::paint::logical(&text).y) / 2.0), 1.0);
    }
    canvas.stamp(&inside.image, list.origin, 1.0);
}

fn button(canvas: &mut Canvas, look: &Look, fonts: &Fonts, rect: Rect, label: &str, main: bool) {
    let color = if main { look.accent } else { look.off };
    canvas.round_rect(rect, 7.0, color, 1.0);
    let text = fonts.body.paint(label, look.ink);
    canvas.stamp(&text, rect.center() - crate::paint::logical(&text) / 2.0, 1.0);
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

struct FormFrame {
    face: Option<Rect>,
    fields: Vec<Rect>,
    switch: Option<Rect>,
    note: Rect,
}

fn form_tall(sheet: &Sheet) -> f64 {
    let Sheet::Form {
        fields,
        switch,
        face,
        ..
    } = sheet
    else {
        return 0.0;
    };
    let face = if face.is_some() { AVATAR + 14.0 } else { 0.0 };
    let switch = if switch.is_some() { SWITCH } else { 0.0 };
    56.0 + face + fields.len() as f64 * FIELD + switch + NOTE + 50.0
}

fn form_frame(card: Rect, sheet: &Sheet) -> FormFrame {
    let x = card.origin.x + 20.0;
    let width = card.size.x - 40.0;
    let mut y = card.origin.y + 56.0;
    let (count, has_switch, has_face) = match sheet {
        Sheet::Form {
            fields,
            switch,
            face,
            ..
        } => (fields.len(), switch.is_some(), face.is_some()),
        _ => (0, false, false),
    };
    let face = has_face.then(|| {
        let spot = Rect::new(Vec2::new(x, y), Vec2::splat(AVATAR));
        y += AVATAR + 14.0;
        spot
    });
    let fields = (0..count)
        .map(|_| {
            let rect = Rect::new(Vec2::new(x, y + 20.0), Vec2::new(width, 32.0));
            y += FIELD;
            rect
        })
        .collect();
    let switch = has_switch.then(|| {
        let row = Rect::new(Vec2::new(x, y), Vec2::new(width, 30.0));
        y += SWITCH;
        row
    });
    let note = Rect::new(Vec2::new(x, y + 4.0), Vec2::new(width, 20.0));
    FormFrame {
        face,
        fields,
        switch,
        note,
    }
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
    let tall = match sheet {
        Sheet::Secret { .. } => 240.0,
        Sheet::Form { .. } => form_tall(sheet).min((size.y - 20.0).max(240.0)),
        _ => (size.y - 60.0).clamp(300.0, 500.0),
    };
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
    let top = match sheet {
        Sheet::Info { .. } | Sheet::Faces { .. } => card.origin.y + 54.0,
        _ => card.origin.y + 90.0,
    };
    let list = Rect::new(
        Vec2::new(inner_x, top),
        Vec2::new(inner_w, bar - 12.0 - top),
    );
    let field = match sheet {
        Sheet::Choices { .. } => Rect::new(
            Vec2::new(inner_x, card.origin.y + 48.0),
            Vec2::new(inner_w, 30.0),
        ),
        _ => Rect::new(Vec2::new(inner_x, top + 10.0), Vec2::new(inner_w, 36.0)),
    };
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

    #[test]
    fn a_password_sheet_keeps_what_is_typed_and_joins_on_enter() {
        let mut sheet = Sheet::secret(For::Wifi(2), "Join DrifterNet");
        assert!(sheet.typing());
        assert_eq!(sheet.enter(), None, "an empty password does nothing");
        sheet.type_text("drift pass\n");
        sheet.backspace();
        sheet.type_text("1");
        assert_eq!(sheet.enter(), Some(Pick::Command("drift pas1".to_string())));
        assert_eq!(sheet.purpose(), For::Wifi(2));
        assert_eq!(sheet.title(), "Join DrifterNet");
    }

    fn add_user() -> Sheet {
        Sheet::form(
            For::AddUser,
            "Add a user",
            vec![
                Field::plain("Full name", ""),
                Field::following("Account name"),
                Field::secret("Password"),
                Field::secret("Repeat password"),
            ],
            "Add User",
        )
        .with_switch("Allow this user to administer the computer", false)
        .with_face(Some(PathBuf::from("/faces/cat.png")))
    }

    #[test]
    fn the_account_name_follows_the_full_name_until_it_is_typed_in() {
        let mut sheet = add_user();
        assert!(sheet.typing());
        sheet.type_text("Ann Lee");
        let texts = |sheet: &Sheet| match sheet {
            Sheet::Form { fields, .. } => fields.iter().map(|f| f.text.clone()).collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        assert_eq!(texts(&sheet)[1], "annlee");
        sheet.backspace();
        assert_eq!(texts(&sheet)[1], "annle");
        sheet.focus(1);
        sheet.backspace();
        sheet.type_text("x");
        sheet.focus(0);
        sheet.type_text("e");
        assert_eq!(texts(&sheet)[0], "Ann Lee");
        assert_eq!(texts(&sheet)[1], "annlx", "a typed account name stays put");
    }

    #[test]
    fn enter_walks_the_fields_then_submits_with_the_switch() {
        let mut sheet = add_user();
        sheet.type_text("Ann");
        assert_eq!(sheet.enter(), Some(Pick::Focus(1)));
        sheet.focus(3);
        sheet.type_text("pw");
        sheet.toggle();
        assert_eq!(
            sheet.enter(),
            Some(Pick::Submit {
                texts: vec!["Ann".into(), "ann".into(), "".into(), "pw".into()],
                on: true,
            })
        );
        sheet.next_field();
        sheet.type_text("q");
        match &sheet {
            Sheet::Form { fields, focus, .. } => {
                assert_eq!(*focus, 0, "tab wraps to the first field");
                assert_eq!(fields[0].text, "Annq");
            }
            _ => panic!("not a form"),
        }
    }

    #[test]
    fn a_busy_form_takes_no_typing_and_cannot_submit_twice() {
        let mut sheet = add_user();
        sheet.type_text("Ann");
        sheet.work(Some("Adding Ann…"), true);
        sheet.type_text("x");
        assert_eq!(sheet.enter(), Some(Pick::Focus(1)));
        sheet.focus(3);
        assert_eq!(sheet.enter(), None);
        assert_eq!(sheet.hit(SIZE, frame(SIZE, &sheet).primary.center()), None);
        sheet.work(Some("That name is taken."), false);
        assert!(matches!(sheet.enter(), Some(Pick::Submit { .. })));
        match &sheet {
            Sheet::Form { fields, note, .. } => {
                assert_eq!(fields[0].text, "Ann");
                assert_eq!(note.as_deref(), Some("That name is taken."));
            }
            _ => panic!("not a form"),
        }
    }

    #[test]
    fn clicks_on_a_form_find_the_field_the_switch_and_the_picture() {
        let mut sheet = add_user();
        let card = frame(SIZE, &sheet).card;
        let form = form_frame(card, &sheet);
        assert_eq!(sheet.hit(SIZE, form.fields[2].center()), Some(Pick::Focus(2)));
        assert_eq!(sheet.hit(SIZE, form.switch.unwrap().center()), Some(Pick::Toggle));
        assert_eq!(sheet.hit(SIZE, form.face.unwrap().center()), Some(Pick::Face));
        assert!(form.note.max().y <= frame(SIZE, &sheet).cancel.origin.y, "the note sits above the buttons");
        sheet.put_face(PathBuf::from("/faces/ball.png"));
        assert_eq!(sheet.face(), Some(PathBuf::from("/faces/ball.png")));
    }

    #[test]
    fn a_picture_grid_picks_the_clicked_picture_and_offers_a_file() {
        let stock: Vec<PathBuf> = (0..30).map(|at| PathBuf::from(format!("/faces/{at}.png"))).collect();
        let mut sheet = Sheet::faces(For::MyFace, stock.clone(), None);
        let frame_now = frame(SIZE, &sheet);
        let cells = face_cells(frame_now.list.size.x, stock.len());
        let second = cells[1].center() + frame_now.list.origin;
        assert_eq!(sheet.hit(SIZE, second), Some(Pick::File(stock[1].clone())));
        assert_eq!(sheet.hit(SIZE, frame_now.primary.center()), Some(Pick::Browse));
        sheet.scroll_by(SIZE, 10_000.0);
        let Sheet::Faces { scroll, .. } = &sheet else {
            panic!("not faces");
        };
        assert!(*scroll > 0.0, "thirty pictures scroll");
        let last = cells[29];
        let shown = last.center() + frame_now.list.origin - Vec2::new(0.0, *scroll);
        assert!(frame_now.list.contains(shown), "the last picture can be reached");
        assert_eq!(sheet.hit(SIZE, shown), Some(Pick::File(stock[29].clone())));
    }

    #[test]
    fn a_form_with_no_fields_still_submits_from_its_button() {
        let sheet = Sheet::form(For::RemoveUser(0), "Remove Ann?", Vec::new(), "Remove")
            .with_switch("Delete the home folder too", false)
            .with_note("If you keep it, /home/ann stays on this computer.");
        let size = Vec2::new(866.0, 640.0);
        let button = frame(size, &sheet).primary.center();
        assert_eq!(
            sheet.hit(size, button),
            Some(Pick::Submit {
                texts: Vec::new(),
                on: false,
            })
        );
        assert_eq!(sheet.enter(), Some(Pick::Submit { texts: Vec::new(), on: false }));
    }

    #[test]
    fn face_keys_carry_their_size_and_path() {
        let key = face_key(Path::new("/var/lib/AccountsService/icons/dh"), 72);
        assert_eq!(key, "face:72:/var/lib/AccountsService/icons/dh");
        assert_eq!(face_of(&key), Some((72, PathBuf::from("/var/lib/AccountsService/icons/dh"))));
        assert_eq!(face_of("app:foo"), None);
    }

    fn apps() -> Vec<App> {
        (0..30)
            .map(|n| App {
                id: format!("app{n}"),
                name: format!("App {n}"),
                icon: None,
                exec: Some(format!("app{n}")),
                words: String::new(),
                categories: Vec::new(),
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
        std::fs::write(base.join("c.ttf"), b"x").unwrap();
        assert_eq!(names(entries(&base, Kind::Pictures)), ["zeta", "a.png"]);
        assert_eq!(names(entries(&base, Kind::Folders)), ["zeta"]);
        assert_eq!(names(entries(&base, Kind::Fonts)), ["zeta", "c.ttf"]);
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

    fn layouts() -> Vec<Choice> {
        crate::keyboard::parse("! layout\n  us  English (US)\n  de  German\n  gb  English (UK)\n")
    }

    #[test]
    fn typing_filters_the_layouts_and_enter_takes_the_first() {
        let mut sheet = Sheet::choices(For::Layout, layouts());
        assert!(sheet.typing());
        let list = frame(SIZE, &sheet).list;
        let first = Vec2::new(list.center().x, list.origin.y + ROW * 0.5);
        assert_eq!(sheet.hit(SIZE, first), Some(Pick::Choice("gb".into())));
        sheet.type_text("germ");
        assert_eq!(sheet.hit(SIZE, first), Some(Pick::Choice("de".into())));
        assert_eq!(sheet.enter(), Some(Pick::Choice("de".into())));
        sheet.type_text("x");
        assert_eq!(sheet.enter(), None);
        sheet.backspace();
        assert_eq!(sheet.purpose(), For::Layout);
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
