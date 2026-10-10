use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};

use super::entry::{self, Entry, Family, Sort};
use super::field::Field;
use super::ops::{self, Done, Job, Undo};
use super::open::{self, Handler};
use super::prefs::{self, Click, Dates, Mode, Prefs, Root, Start};

#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    Open,
    OpenWith(String),
    NewWindow,
    Window(PathBuf),
    NewFolder,
    NewFile,
    Rename,
    Duplicate,
    Trash,
    DeleteForever,
    Restore,
    EmptyTrash,
    Info,
    Close,
    Undo,
    Cut,
    Copy,
    Paste,
    PasteInto(PathBuf),
    PickAll,
    CopyPath,
    Prefs,
    Show(Mode),
    ZoomIn,
    ZoomOut,
    Hidden,
    Tree,
    Status,
    Reload,
    SortBy(Sort),
    Back,
    Forward,
    Up,
    Go(PathBuf),
    Mark(PathBuf),
    Unmark(PathBuf),
    EditPath,
    Terminal(Option<PathBuf>),
    Keys,
    About,
    Set(Pref),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pref {
    Start(Start),
    Click(Click),
    ConfirmTrash,
    Hidden,
    Mode(Mode),
    Sort(Sort),
    Down,
    FoldersFirst,
    Thumbnails,
    ShowSize,
    ShowKind,
    ShowModified,
    Dates(Dates),
    Tree,
    Root(Root),
    Places,
    Drives,
    Status,
    Terminal(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    pub label: String,
    pub keys: &'static str,
    pub act: Option<Act>,
    pub on: bool,
    pub check: Option<bool>,
    pub rule: bool,
    pub kids: Vec<Item>,
}

impl Item {
    pub fn new(label: &str, keys: &'static str, act: Act) -> Item {
        Item { label: label.to_string(), keys, act: Some(act), on: true, check: None, rule: false, kids: Vec::new() }
    }

    pub fn rule() -> Item {
        Item { label: String::new(), keys: "", act: None, on: false, check: None, rule: true, kids: Vec::new() }
    }

    pub fn more(label: &str, kids: Vec<Item>) -> Item {
        Item { label: label.to_string(), keys: "", act: None, on: !kids.is_empty(), check: None, rule: false, kids }
    }

    pub fn when(mut self, on: bool) -> Item {
        self.on = on;
        self
    }

    pub fn checked(mut self, on: bool) -> Item {
        self.check = Some(on);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Menu {
    pub items: Vec<Item>,
    pub at: crate::Vec2,
    pub hover: Option<usize>,
    pub open: Option<usize>,
    pub kid_hover: Option<usize>,
    pub bar: Option<usize>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Info {
    pub entry: Entry,
    pub inside: Option<usize>,
    pub mode: Option<u32>,
    pub target: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Sheet {
    Prefs(usize),
    Info(Info),
    Confirm { title: String, words: String, yes: String, act: Act },
    Keys,
    About,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Head(&'static str),
    Place { path: PathBuf, name: String, icon: &'static str, mark: bool },
    Folder { path: PathBuf, depth: usize, open: bool },
    Drive { path: PathBuf, name: String },
}

impl Line {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Line::Head(_) => None,
            Line::Place { path, .. } | Line::Folder { path, .. } | Line::Drive { path, .. } => Some(path),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    pub scroll: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Job(Job),
    Launch(Vec<String>),
    Clip(String),
    NewWindow(PathBuf),
    Close,
}

pub struct Model {
    pub prefs: Prefs,
    pub prefs_path: PathBuf,
    pub home: PathBuf,
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    pub trouble: Option<String>,
    pub picked: HashSet<PathBuf>,
    pub anchor: Option<PathBuf>,
    pub focus: Option<PathBuf>,
    pub back: Vec<PathBuf>,
    pub ahead: Vec<PathBuf>,
    pub scroll: f64,
    pub tree_scroll: f64,
    pub open: HashSet<PathBuf>,
    pub kids: HashMap<PathBuf, Vec<PathBuf>>,
    pub columns: Vec<Column>,
    pub path: Option<Field>,
    pub renaming: Option<(PathBuf, Field)>,
    pub menu: Option<Menu>,
    pub sheet: Option<Sheet>,
    pub clip: Option<(Vec<PathBuf>, bool)>,
    pub undo: Vec<Undo>,
    pub busy: Vec<String>,
    pub note: Option<(String, bool, Instant)>,
    pub peek: Option<Vec<String>>,
    pub typed: (String, Option<Instant>),
    pub now: SystemTime,
    pub free: Option<(u64, u64)>,
    pub stamp: Option<SystemTime>,
    pub handlers: HashMap<&'static str, Vec<Handler>>,
    pub terminals: Vec<(&'static str, &'static str)>,
    pub drives: Vec<(PathBuf, String)>,
    pub effects: Vec<Effect>,
    pub dragging: Option<Drag>,
    pub hover: Option<super::look::Hit>,
    pub saved: bool,
    pub marks: Vec<super::marks::Mark>,
    pub marks_path: PathBuf,
    pub marks_stamp: Option<SystemTime>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Drag {
    Split { from: f64, width: u32 },
    Zoom,
    Slider,
    Files { paths: Vec<PathBuf>, from: crate::Vec2, at: crate::Vec2, live: bool, pending: Option<PathBuf> },
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

pub fn expand(typed: &str, dir: &Path, home: &Path) -> PathBuf {
    let typed = typed.trim();
    let typed = typed.strip_prefix("file://").unwrap_or(typed);
    let path = match typed {
        "" => dir.to_path_buf(),
        "~" => home.to_path_buf(),
        _ if typed.starts_with("~/") => home.join(&typed[2..]),
        _ if typed.starts_with('/') => PathBuf::from(typed),
        _ => dir.join(typed),
    };
    let mut clean = PathBuf::new();
    for part in path.components() {
        match part {
            std::path::Component::ParentDir => {
                clean.pop();
            }
            std::path::Component::CurDir => {}
            other => clean.push(other),
        }
    }
    if clean.as_os_str().is_empty() {
        clean.push("/");
    }
    clean
}

pub fn complete(typed: &str, dir: &Path, home: &Path, hidden: bool) -> Option<String> {
    let (head, tail) = match typed.rfind('/') {
        Some(at) => (&typed[..=at], &typed[at + 1..]),
        None => ("", typed),
    };
    let folder = expand(if head.is_empty() { "." } else { head }, dir, home);
    let names: Vec<String> = entry::folders_in(&folder, hidden || tail.starts_with('.'))
        .iter()
        .map(|path| entry::name_of(path))
        .filter(|name| name.starts_with(tail))
        .collect();
    let first = names.first()?;
    let mut common = first.clone();
    for name in &names[1..] {
        let shared = common.chars().zip(name.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8()).sum();
        common.truncate(shared);
    }
    let finished = names.len() == 1;
    Some(format!("{head}{common}{}", if finished { "/" } else { "" }))
}

fn decode_mount(field: &str) -> String {
    field.replace("\\040", " ").replace("\\011", "\t").replace("\\134", "\\")
}

pub fn drives_in(mounts: &str) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let (Some(_), Some(point)) = (parts.next(), parts.next()) else {
            continue;
        };
        let point = decode_mount(point);
        let removable = ["/media/", "/run/media/", "/mnt/"].iter().any(|root| point.starts_with(root));
        if removable && !out.iter().any(|(path, _): &(PathBuf, String)| path.as_os_str() == point.as_str()) {
            let path = PathBuf::from(&point);
            out.push((path.clone(), entry::name_of(&path)));
        }
    }
    out
}

impl Model {
    pub fn new(prefs_path: PathBuf, asked: Option<PathBuf>) -> Model {
        let prefs = Prefs::load_from(&prefs_path);
        let home = home();
        let start = asked
            .filter(|path| path.is_dir())
            .or_else(|| match prefs.start {
                Start::Last => prefs.last.clone().filter(|path| path.is_dir()),
                Start::Home => None,
            })
            .unwrap_or_else(|| home.clone());
        let mut model = Model {
            prefs,
            prefs_path,
            home: home.clone(),
            dir: start.clone(),
            entries: Vec::new(),
            trouble: None,
            picked: HashSet::new(),
            anchor: None,
            focus: None,
            back: Vec::new(),
            ahead: Vec::new(),
            scroll: 0.0,
            tree_scroll: 0.0,
            open: HashSet::new(),
            kids: HashMap::new(),
            columns: Vec::new(),
            path: None,
            renaming: None,
            menu: None,
            sheet: None,
            clip: None,
            undo: Vec::new(),
            busy: Vec::new(),
            note: None,
            peek: None,
            typed: (String::new(), None),
            now: SystemTime::now(),
            free: None,
            stamp: None,
            handlers: HashMap::new(),
            terminals: prefs::terminals(),
            drives: Vec::new(),
            effects: Vec::new(),
            dragging: None,
            hover: None,
            saved: true,
            marks: Vec::new(),
            marks_path: super::marks::file(),
            marks_stamp: None,
        };
        model.read_marks();
        model.drives = drives_in(&std::fs::read_to_string("/proc/mounts").unwrap_or_default());
        model.open.insert(model.tree_base());
        model.load(start, None);
        model
    }

    pub fn tree_base(&self) -> PathBuf {
        match self.prefs.tree_root {
            Root::Home => self.home.clone(),
            Root::Computer => PathBuf::from("/"),
        }
    }

    pub fn column_base(&self) -> PathBuf {
        match self.dir.starts_with(&self.home) {
            true => self.home.clone(),
            false => PathBuf::from("/"),
        }
    }

    pub fn say(&mut self, words: impl Into<String>, alert: bool) {
        self.note = Some((words.into(), alert, Instant::now()));
    }

    fn read_dir(&self, dir: &Path) -> std::io::Result<Vec<Entry>> {
        let mut found = entry::read(dir, self.prefs.hidden)?;
        entry::sort(&mut found, self.prefs.sort, self.prefs.down, self.prefs.folders_first);
        Ok(found)
    }

    fn load(&mut self, dir: PathBuf, pick: Option<PathBuf>) -> bool {
        match self.read_dir(&dir) {
            Ok(found) => {
                self.entries = found;
                self.trouble = None;
            }
            Err(error) => {
                if self.entries.is_empty() || self.dir == dir {
                    self.entries.clear();
                    self.trouble = Some(error.to_string());
                    self.dir = dir.clone();
                } else {
                    self.say(crate::lang::tf("Cannot open {}: {}", &[&entry::name_of(&dir), &error]), true);
                    return false;
                }
            }
        }
        self.dir = dir;
        self.picked.clear();
        self.anchor = None;
        self.focus = None;
        self.scroll = 0.0;
        self.renaming = None;
        self.typed = (String::new(), None);
        if let Some(pick) = pick.filter(|pick| self.entries.iter().any(|each| &each.path == pick)) {
            self.pick_only(pick);
        }
        self.stamp = modified(&self.dir);
        self.free = entry::free_space(&self.dir);
        self.reveal();
        self.rebuild_columns();
        self.refresh_peek();
        if self.prefs.last.as_ref() != Some(&self.dir) {
            self.prefs.last = Some(self.dir.clone());
            self.saved = false;
        }
        true
    }

    pub fn go(&mut self, dir: PathBuf, pick: Option<PathBuf>) {
        if dir == self.dir {
            if let Some(pick) = pick {
                self.pick_only(pick);
            }
            return;
        }
        let was = self.dir.clone();
        if self.load(dir, pick) {
            self.back.push(was);
            self.ahead.clear();
        }
    }

    pub fn reload(&mut self) {
        let keep: HashSet<PathBuf> = self.picked.clone();
        let (focus, anchor, scroll) = (self.focus.clone(), self.anchor.clone(), self.scroll);
        match self.read_dir(&self.dir.clone()) {
            Ok(found) => {
                self.entries = found;
                self.trouble = None;
            }
            Err(error) => {
                self.entries.clear();
                self.trouble = Some(error.to_string());
            }
        }
        let here: HashSet<&PathBuf> = self.entries.iter().map(|each| &each.path).collect();
        self.picked = keep.into_iter().filter(|path| here.contains(path)).collect();
        self.focus = focus.filter(|path| here.contains(path));
        self.anchor = anchor.filter(|path| here.contains(path));
        self.scroll = scroll;
        if let Some((path, _)) = &self.renaming
            && !here.contains(path)
        {
            self.renaming = None;
        }
        self.stamp = modified(&self.dir);
        self.free = entry::free_space(&self.dir);
        self.kids.clear();
        self.rebuild_columns();
        self.refresh_peek();
    }

    pub fn tick(&mut self) -> bool {
        self.now = SystemTime::now();
        let mut changed = false;
        if self.note.as_ref().is_some_and(|(_, _, at)| at.elapsed().as_secs() >= 6) {
            self.note = None;
            changed = true;
        }
        if self.typed.1.is_some_and(|at| at.elapsed().as_millis() > 900) {
            self.typed = (String::new(), None);
        }
        if modified(&self.marks_path) != self.marks_stamp {
            self.read_marks();
            changed = true;
        }
        let stamp = modified(&self.dir);
        if stamp != self.stamp {
            self.reload();
            changed = true;
        }
        changed
    }

    pub fn save(&mut self) {
        if self.saved {
            return;
        }
        match self.prefs.save_to(&self.prefs_path) {
            Ok(()) => self.saved = true,
            Err(error) => self.say(crate::lang::tf("Settings were not saved: {}", &[&error]), true),
        }
    }

    pub fn reveal(&mut self) {
        let base = self.tree_base();
        if !self.dir.starts_with(&base) {
            return;
        }
        let mut walk = self.dir.parent().map(Path::to_path_buf);
        while let Some(at) = walk {
            if !at.starts_with(&base) {
                break;
            }
            self.open.insert(at.clone());
            walk = at.parent().map(Path::to_path_buf);
        }
    }

    pub fn read_marks(&mut self) {
        self.marks = super::marks::load_from(&self.marks_path);
        self.marks_stamp = modified(&self.marks_path);
    }

    pub fn marked(&self, path: &Path) -> bool {
        self.marks.iter().any(|mark| mark.path == path)
    }

    pub fn mark(&mut self, paths: &[PathBuf]) {
        let fresh: Vec<PathBuf> = paths.iter().filter(|path| path.is_dir() && !self.marked(path)).cloned().collect();
        if fresh.is_empty() {
            return;
        }
        for path in &fresh {
            self.marks.push(super::marks::Mark { name: entry::name_of(path), path: path.clone() });
        }
        self.write_marks();
        self.say(crate::lang::tf("Added {} to the sidebar", &[&match fresh.as_slice() {
            [one] => entry::name_of(one),
            _ => entry::items(fresh.len()),
        }]), false);
    }

    pub fn unmark(&mut self, path: &Path) {
        let before = self.marks.len();
        self.marks.retain(|mark| mark.path != path);
        if self.marks.len() != before {
            self.write_marks();
        }
    }

    fn write_marks(&mut self) {
        match super::marks::save_to(&self.marks_path, &self.marks) {
            Ok(()) => self.marks_stamp = modified(&self.marks_path),
            Err(error) => self.say(crate::lang::tf("The sidebar was not saved: {}", &[&error]), true),
        }
    }

    pub fn places(&self) -> Vec<(PathBuf, String, &'static str)> {
        let home = &self.home;
        let mut out = vec![(home.clone(), crate::lang::t("Home").to_string(), "user-home")];
        let named: [(&str, &str, &str, &str); 6] = [
            ("DESKTOP", "Desktop", crate::lang::t("Desktop"), "user-desktop"),
            ("DOCUMENTS", "Documents", crate::lang::t("Documents"), "folder-documents"),
            ("DOWNLOAD", "Downloads", crate::lang::t("Downloads"), "folder-download"),
            ("MUSIC", "Music", crate::lang::t("Music"), "folder-music"),
            ("PICTURES", "Pictures", crate::lang::t("Pictures"), "folder-pictures"),
            ("VIDEOS", "Videos", crate::lang::t("Videos"), "folder-videos"),
        ];
        for (key, fallback, name, icon) in named {
            let path = crate::folders::user_dir(home, key, fallback);
            if path.is_dir() && !out.iter().any(|(each, _, _)| each == &path) {
                out.push((path, name.to_string(), icon));
            }
        }
        for mark in &self.marks {
            if !out.iter().any(|(each, _, _)| each == &mark.path) {
                out.push((mark.path.clone(), mark.name.clone(), "folder"));
            }
        }
        out.push((ops::trash_files(), crate::lang::t("Trash").to_string(), "user-trash"));
        out
    }

    pub fn lines(&mut self) -> Vec<Line> {
        let mut out = Vec::new();
        if self.prefs.places {
            out.push(Line::Head(crate::lang::t("Places")));
            for (path, name, icon) in self.places() {
                let mark = self.marked(&path);
                out.push(Line::Place { path, name, icon, mark });
            }
        }
        if self.prefs.tree {
            out.push(Line::Head(crate::lang::t("Folders")));
            let base = self.tree_base();
            self.walk(&base, 0, &mut out);
        }
        if self.prefs.drives {
            out.push(Line::Head(crate::lang::t("Drives")));
            out.push(Line::Drive { path: PathBuf::from("/"), name: crate::lang::t("Computer").to_string() });
            for (path, name) in &self.drives {
                out.push(Line::Drive { path: path.clone(), name: name.clone() });
            }
        }
        out
    }

    fn walk(&mut self, at: &Path, depth: usize, out: &mut Vec<Line>) {
        let open = self.open.contains(at);
        out.push(Line::Folder { path: at.to_path_buf(), depth, open });
        if !open || depth > 24 {
            return;
        }
        let hidden = self.prefs.hidden;
        let kids = self.kids.entry(at.to_path_buf()).or_insert_with(|| entry::folders_in(at, hidden)).clone();
        for kid in kids {
            self.walk(&kid, depth + 1, out);
        }
    }

    pub fn fold(&mut self, path: &Path) {
        if !self.open.remove(path) {
            self.open.insert(path.to_path_buf());
        }
    }

    pub fn rebuild_columns(&mut self) {
        let base = self.column_base();
        let mut chain: Vec<PathBuf> = self.dir.ancestors().take_while(|at| at.starts_with(&base)).map(Path::to_path_buf).collect();
        chain.reverse();
        if chain.is_empty() {
            chain.push(self.dir.clone());
        }
        let old: HashMap<PathBuf, f64> = self.columns.iter().map(|column| (column.dir.clone(), column.scroll)).collect();
        let mut columns = Vec::new();
        for dir in chain {
            let entries = match dir == self.dir {
                true => self.entries.clone(),
                false => self.read_dir(&dir).unwrap_or_default(),
            };
            let scroll = old.get(&dir).copied().unwrap_or(0.0);
            columns.push(Column { dir, entries, scroll });
        }
        if let Some(folder) = self.single().filter(|one| one.folder).map(|one| one.path.clone()) {
            let entries = self.read_dir(&folder).unwrap_or_default();
            let scroll = old.get(&folder).copied().unwrap_or(0.0);
            columns.push(Column { dir: folder, entries, scroll });
        }
        self.columns = columns;
    }

    pub fn single(&self) -> Option<&Entry> {
        match self.picked.len() {
            1 => self.entries.iter().find(|each| self.picked.contains(&each.path)),
            _ => None,
        }
    }

    pub fn chosen(&self) -> Vec<&Entry> {
        self.entries.iter().filter(|each| self.picked.contains(&each.path)).collect()
    }

    pub fn chosen_paths(&self) -> Vec<PathBuf> {
        self.chosen().iter().map(|each| each.path.clone()).collect()
    }

    pub fn index_of(&self, path: &Path) -> Option<usize> {
        self.entries.iter().position(|each| each.path == path)
    }

    pub fn focus_index(&self) -> Option<usize> {
        self.focus.as_deref().and_then(|path| self.index_of(path))
    }

    pub fn pick_only(&mut self, path: PathBuf) {
        self.picked.clear();
        self.picked.insert(path.clone());
        self.anchor = Some(path.clone());
        self.focus = Some(path);
        self.after_pick();
    }

    pub fn pick_toggle(&mut self, path: PathBuf) {
        if !self.picked.remove(&path) {
            self.picked.insert(path.clone());
        }
        self.anchor = Some(path.clone());
        self.focus = Some(path);
        self.after_pick();
    }

    pub fn pick_range(&mut self, to: usize, add: bool) {
        let from = self.anchor.as_deref().and_then(|path| self.index_of(path)).unwrap_or(to);
        if !add {
            self.picked.clear();
        }
        let (low, high) = (from.min(to), from.max(to));
        for each in &self.entries[low..=high.min(self.entries.len().saturating_sub(1))] {
            self.picked.insert(each.path.clone());
        }
        self.focus = self.entries.get(to).map(|each| each.path.clone());
        self.after_pick();
    }

    pub fn pick_none(&mut self) {
        if self.picked.is_empty() {
            return;
        }
        self.picked.clear();
        self.focus = None;
        self.after_pick();
    }

    fn after_pick(&mut self) {
        self.renaming = None;
        if self.prefs.mode == Mode::Columns {
            self.rebuild_columns();
        }
        self.refresh_peek();
    }

    pub fn refresh_peek(&mut self) {
        self.peek = self.single().filter(|one| !one.folder && one.size < 4 << 20).and_then(|one| match one.family() {
            Family::Text | Family::Other => peek(&one.path),
            _ => None,
        });
    }

    pub fn step(&mut self, by: isize, keep: bool) {
        if self.entries.is_empty() {
            return;
        }
        let last = self.entries.len() as isize - 1;
        let to = match self.focus_index() {
            Some(at) => (at as isize + by).clamp(0, last) as usize,
            None if by < 0 => last as usize,
            None => 0,
        };
        match keep {
            true => self.pick_range(to, false),
            false => self.pick_only(self.entries[to].path.clone()),
        }
    }

    pub fn type_to_find(&mut self, letters: &str) {
        let now = Instant::now();
        self.typed.0.push_str(&letters.to_lowercase());
        self.typed.1 = Some(now);
        let wanted = self.typed.0.clone();
        let start = self.focus_index().unwrap_or(0);
        let found = (0..self.entries.len())
            .map(|step| (start + step) % self.entries.len())
            .find(|at| self.entries[*at].name.to_lowercase().starts_with(&wanted));
        if let Some(at) = found {
            self.pick_only(self.entries[at].path.clone());
        }
    }

    pub fn handlers_for(&mut self, mime: &'static str) -> Vec<Handler> {
        self.handlers.entry(mime).or_insert_with(|| open::handlers(mime)).clone()
    }

    pub fn open_paths(&mut self, paths: Vec<PathBuf>) {
        let entries: Vec<Entry> = paths.iter().filter_map(|path| self.entries.iter().find(|each| &each.path == path).cloned().or_else(|| Entry::at(path))).collect();
        if let [one] = entries.as_slice()
            && one.folder
        {
            return self.go(one.path.clone(), None);
        }
        if ops::in_trash(&self.dir) && !entries.is_empty() {
            return self.say(crate::lang::t("Put items back before you open them"), false);
        }
        for each in entries.iter().filter(|each| each.folder).skip(1) {
            self.effects.push(Effect::NewWindow(each.path.clone()));
        }
        if let Some(folder) = entries.iter().find(|each| each.folder) {
            let folder = folder.path.clone();
            self.go(folder, None);
        }
        let mut by_mime: Vec<(&'static str, Vec<PathBuf>)> = Vec::new();
        for each in entries.iter().filter(|each| !each.folder) {
            match by_mime.iter_mut().find(|(mime, _)| *mime == each.mime) {
                Some((_, list)) => list.push(each.path.clone()),
                None => by_mime.push((each.mime, vec![each.path.clone()])),
            }
        }
        for (mime, files) in by_mime {
            let handler = self.handlers_for(mime).into_iter().next();
            let refs: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
            match handler.and_then(|handler| open::argv(&handler, &refs)) {
                Some(argv) => self.effects.push(Effect::Launch(argv)),
                None => {
                    for file in &files {
                        self.effects.push(Effect::Launch(open::fallback(file)));
                    }
                }
            }
        }
    }

    pub fn open_with(&mut self, id: &str) {
        let files: Vec<PathBuf> = self.chosen().iter().filter(|each| !each.folder).map(|each| each.path.clone()).collect();
        let Some(mime) = self.single().map(|one| one.mime).or_else(|| self.chosen().first().map(|one| one.mime)) else {
            return;
        };
        let handler = self.handlers_for(mime).into_iter().find(|each| each.id == id);
        let refs: Vec<&Path> = files.iter().map(PathBuf::as_path).collect();
        if let Some(argv) = handler.and_then(|handler| open::argv(&handler, &refs)) {
            self.effects.push(Effect::Launch(argv));
        }
    }

    pub fn start_job(&mut self, job: Job) {
        self.busy.push(job.doing());
        self.effects.push(Effect::Job(job));
    }

    pub fn finished(&mut self, done: Done) {
        if let Some(at) = self.busy.iter().position(|each| each == &done.words) {
            self.busy.remove(at);
        }
        if let Some(undo) = done.undo {
            self.undo.push(undo);
            if self.undo.len() > 50 {
                self.undo.remove(0);
            }
        }
        self.reload();
        let here: Vec<PathBuf> = done.made.iter().filter(|made| made.parent() == Some(self.dir.as_path())).cloned().collect();
        if !here.is_empty() {
            self.picked = here.iter().cloned().collect();
            self.focus = here.last().cloned();
            self.anchor = here.first().cloned();
            self.after_pick();
        }
        match done.failed.as_slice() {
            [] => {}
            [one] => self.say(one.clone(), true),
            [first, rest @ ..] => self.say(crate::lang::tf("{} and {} more failed", &[first, &rest.len()]), true),
        }
    }

    pub fn start_rename(&mut self) {
        let Some(one) = self.single().cloned().or_else(|| self.focus.as_deref().and_then(|path| self.entries.iter().find(|each| each.path == path)).cloned()) else {
            return;
        };
        if ops::in_trash(&self.dir) {
            return;
        }
        let field = match one.folder {
            true => Field::all(&one.name),
            false => Field::stem(&one.name),
        };
        self.renaming = Some((one.path, field));
    }

    pub fn finish_rename(&mut self) {
        let Some((path, field)) = self.renaming.take() else {
            return;
        };
        match ops::rename(&path, &field.text) {
            Ok(made) if made == path => {}
            Ok(made) => {
                self.undo.push(Undo::Renamed(path, made.clone()));
                self.reload();
                self.pick_only(made);
            }
            Err(words) => self.say(words, true),
        }
    }

    pub fn finish_path(&mut self) {
        let Some(field) = self.path.take() else {
            return;
        };
        let wanted = expand(&field.text, &self.dir, &self.home);
        match wanted.is_dir() {
            true => self.go(wanted, None),
            false if wanted.exists() => {
                let parent = wanted.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("/"));
                self.go(parent, Some(wanted.clone()));
                self.open_paths(vec![wanted]);
            }
            false => self.say(crate::lang::tf("{} does not exist", &[&wanted.display()]), true),
        }
    }

    pub fn confirm(&mut self, title: &str, words: String, yes: &str, act: Act) {
        self.sheet = Some(Sheet::Confirm { title: title.to_string(), words, yes: yes.to_string(), act });
    }

    pub fn set(&mut self, pref: Pref) {
        let p = &mut self.prefs;
        let mut reread = false;
        match pref {
            Pref::Start(start) => p.start = start,
            Pref::Click(click) => p.click = click,
            Pref::ConfirmTrash => p.confirm_trash = !p.confirm_trash,
            Pref::Hidden => {
                p.hidden = !p.hidden;
                reread = true;
            }
            Pref::Mode(mode) => p.mode = mode,
            Pref::Sort(sort) => {
                p.sort = sort;
                reread = true;
            }
            Pref::Down => {
                p.down = !p.down;
                reread = true;
            }
            Pref::FoldersFirst => {
                p.folders_first = !p.folders_first;
                reread = true;
            }
            Pref::Thumbnails => p.thumbnails = !p.thumbnails,
            Pref::ShowSize => p.show_size = !p.show_size,
            Pref::ShowKind => p.show_kind = !p.show_kind,
            Pref::ShowModified => p.show_modified = !p.show_modified,
            Pref::Dates(dates) => p.dates = dates,
            Pref::Tree => p.tree = !p.tree,
            Pref::Root(root) => {
                p.tree_root = root;
                self.open.insert(self.tree_base());
                self.reveal();
            }
            Pref::Places => p.places = !p.places,
            Pref::Drives => p.drives = !p.drives,
            Pref::Status => p.status = !p.status,
            Pref::Terminal(at) => p.terminal = self.terminals.get(at).map(|(program, _)| program.to_string()),
        }
        if reread {
            self.reload();
        }
        if pref == Pref::Mode(Mode::Columns) {
            self.rebuild_columns();
        }
        self.saved = false;
    }

    pub fn act(&mut self, act: Act) {
        self.menu = None;
        let in_trash = ops::in_trash(&self.dir);
        match act {
            Act::Open => {
                let paths = self.chosen_paths();
                self.open_paths(paths);
            }
            Act::OpenWith(id) => self.open_with(&id),
            Act::NewWindow => self.effects.push(Effect::NewWindow(self.dir.clone())),
            Act::Window(path) => self.effects.push(Effect::NewWindow(path)),
            Act::NewFolder | Act::NewFile if in_trash => {}
            Act::NewFolder => match ops::new_folder(&self.dir) {
                Ok(made) => {
                    self.undo.push(Undo::Made(vec![made.clone()]));
                    self.reload();
                    self.pick_only(made);
                    self.start_rename();
                }
                Err(error) => self.say(error.to_string(), true),
            },
            Act::NewFile => match ops::new_file(&self.dir) {
                Ok(made) => {
                    self.undo.push(Undo::Made(vec![made.clone()]));
                    self.reload();
                    self.pick_only(made);
                    self.start_rename();
                }
                Err(error) => self.say(error.to_string(), true),
            },
            Act::Rename => self.start_rename(),
            Act::Duplicate => {
                let from = self.chosen_paths();
                if !from.is_empty() && !in_trash {
                    self.start_job(Job::Copy { from, into: self.dir.clone() });
                }
            }
            Act::Trash if in_trash => self.act(Act::DeleteForever),
            Act::Trash => {
                let paths = self.chosen_paths();
                if paths.is_empty() {
                    return;
                }
                match self.prefs.confirm_trash && self.sheet.is_none() {
                    true => {
                        let words = gone_words(&paths);
                        self.confirm(crate::lang::t("Move to Trash?"), words, crate::lang::t("Move to Trash"), Act::Trash);
                    }
                    false => {
                        self.sheet = None;
                        self.start_job(Job::Trash(paths));
                    }
                }
            }
            Act::DeleteForever => {
                let paths = self.chosen_paths();
                if paths.is_empty() {
                    return;
                }
                match self.sheet.is_none() {
                    true => {
                        let words = format!("{} {}", gone_words(&paths), crate::lang::t("This cannot be undone."));
                        self.confirm(crate::lang::t("Delete forever?"), words, crate::lang::t("Delete"), Act::DeleteForever);
                    }
                    false => {
                        self.sheet = None;
                        self.start_job(Job::Delete(paths));
                    }
                }
            }
            Act::Restore => {
                let paths = self.chosen_paths();
                if in_trash && !paths.is_empty() {
                    self.start_job(Job::Restore(paths));
                }
            }
            Act::EmptyTrash => match self.sheet.is_none() {
                true => self.confirm(
                    crate::lang::t("Empty the Trash?"),
                    crate::lang::t("Everything in the Trash is deleted forever.").to_string(),
                    crate::lang::t("Empty Trash"),
                    Act::EmptyTrash,
                ),
                false => {
                    self.sheet = None;
                    self.start_job(Job::EmptyTrash);
                }
            },
            Act::Info => {
                let one = self.single().cloned().or_else(|| Entry::at(&self.dir));
                if let Some(one) = one {
                    self.sheet = Some(Sheet::Info(info_of(one)));
                }
            }
            Act::Close => self.effects.push(Effect::Close),
            Act::Undo => match self.undo.pop() {
                Some(undo) => self.start_job(Job::Undo(undo)),
                None => self.say(crate::lang::t("Nothing to undo"), false),
            },
            Act::Cut | Act::Copy => {
                let paths = self.chosen_paths();
                if paths.is_empty() {
                    return;
                }
                let cut = act == Act::Cut && !in_trash;
                let words = match (cut, paths.len()) {
                    (true, count) => crate::lang::tf("Cut {}", &[&entry::items(count)]),
                    (false, count) => crate::lang::tf("Copied {}", &[&entry::items(count)]),
                };
                let text = paths.iter().map(|path| path.to_string_lossy()).collect::<Vec<_>>().join("\n");
                self.effects.push(Effect::Clip(text));
                self.clip = Some((paths, cut));
                self.say(words, false);
            }
            Act::Paste => {
                let Some((paths, cut)) = self.clip.clone() else {
                    return;
                };
                if !in_trash {
                    let into = self.dir.clone();
                    self.paste(paths, cut, into);
                }
            }
            Act::PasteInto(into) => {
                if let Some((paths, cut)) = self.clip.clone()
                    && !paths.contains(&into)
                {
                    self.paste(paths, cut, into);
                }
            }
            Act::PickAll => {
                self.picked = self.entries.iter().map(|each| each.path.clone()).collect();
                self.after_pick();
            }
            Act::CopyPath => {
                let paths = match self.picked.is_empty() {
                    true => vec![self.dir.clone()],
                    false => self.chosen_paths(),
                };
                let text = paths.iter().map(|path| path.to_string_lossy()).collect::<Vec<_>>().join("\n");
                self.effects.push(Effect::Clip(text));
                self.say(crate::lang::t("Path copied"), false);
            }
            Act::Prefs => self.sheet = Some(Sheet::Prefs(0)),
            Act::Show(mode) => {
                self.prefs.mode = mode;
                self.saved = false;
                self.scroll = 0.0;
                if mode == Mode::Columns {
                    self.rebuild_columns();
                }
            }
            Act::ZoomIn => self.zoom_to(self.prefs.zoom + zoom_step(self.prefs.zoom)),
            Act::ZoomOut => self.zoom_to(self.prefs.zoom.saturating_sub(zoom_step(self.prefs.zoom - 1))),
            Act::Hidden => self.set(Pref::Hidden),
            Act::Tree => {
                let on = !(self.prefs.tree || self.prefs.places);
                self.prefs.tree = on;
                self.prefs.places = on;
                self.prefs.drives = on;
                self.saved = false;
            }
            Act::Status => self.set(Pref::Status),
            Act::Reload => {
                self.drives = drives_in(&std::fs::read_to_string("/proc/mounts").unwrap_or_default());
                self.handlers.clear();
                self.reload();
            }
            Act::SortBy(sort) => {
                match self.prefs.sort == sort {
                    true => self.prefs.down = !self.prefs.down,
                    false => {
                        self.prefs.sort = sort;
                        self.prefs.down = false;
                    }
                }
                self.saved = false;
                self.reload();
            }
            Act::Back => {
                if let Some(to) = self.back.pop() {
                    let was = self.dir.clone();
                    match self.load(to.clone(), Some(was.clone())) {
                        true => self.ahead.push(was),
                        false => self.back.push(to),
                    }
                }
            }
            Act::Forward => {
                if let Some(to) = self.ahead.pop() {
                    let was = self.dir.clone();
                    match self.load(to.clone(), None) {
                        true => self.back.push(was),
                        false => self.ahead.push(to),
                    }
                }
            }
            Act::Up => {
                if let Some(parent) = self.dir.parent().map(Path::to_path_buf) {
                    let was = self.dir.clone();
                    self.go(parent, Some(was));
                }
            }
            Act::Go(path) => self.go(path, None),
            Act::Mark(path) => self.mark(&[path]),
            Act::Unmark(path) => self.unmark(&path),
            Act::EditPath => self.path = Some(Field::all(&self.dir.to_string_lossy())),
            Act::Terminal(at) => {
                let dir = at.unwrap_or_else(|| self.dir.clone());
                let program = self.prefs.terminal.clone().or_else(|| self.terminals.first().map(|(program, _)| program.to_string()));
                match program {
                    Some(program) => self.effects.push(Effect::Launch(prefs::terminal_argv(&program, &dir))),
                    None => self.say(crate::lang::t("No terminal app is installed"), true),
                }
            }
            Act::Keys => self.sheet = Some(Sheet::Keys),
            Act::About => self.sheet = Some(Sheet::About),
            Act::Set(pref) => self.set(pref),
        }
    }

    fn paste(&mut self, paths: Vec<PathBuf>, cut: bool, into: PathBuf) {
        match cut {
            true => {
                self.clip = None;
                self.start_job(Job::Move { from: paths, into });
            }
            false => self.start_job(Job::Copy { from: paths, into }),
        }
    }

    pub fn zoom_to(&mut self, zoom: u32) {
        let zoom = zoom.clamp(prefs::LEAST_ZOOM, prefs::MOST_ZOOM);
        if zoom != self.prefs.zoom {
            self.prefs.zoom = zoom;
            self.saved = false;
        }
    }

    pub fn bar_menu(&mut self, at: usize) -> Vec<Item> {
        let any = !self.picked.is_empty();
        let in_trash = ops::in_trash(&self.dir);
        let p = self.prefs.clone();
        match at {
            0 => vec![
                Item::new(crate::lang::t("New Window"), "Ctrl+N", Act::NewWindow),
                Item::new(crate::lang::t("New Folder"), "Shift+Ctrl+N", Act::NewFolder).when(!in_trash),
                Item::new(crate::lang::t("New Text File"), "", Act::NewFile).when(!in_trash),
                Item::rule(),
                Item::new(crate::lang::t("Open"), "Enter", Act::Open).when(any),
                Item::more(crate::lang::t("Open With"), self.open_with_items()),
                Item::new(crate::lang::t("Open in Terminal"), "", Act::Terminal(None)),
                Item::rule(),
                Item::new(crate::lang::t("Rename"), "F2", Act::Rename).when(self.picked.len() == 1 && !in_trash),
                Item::new(crate::lang::t("Duplicate"), "Ctrl+D", Act::Duplicate).when(any && !in_trash),
                Item::new(crate::lang::t("Move to Trash"), "Delete", Act::Trash).when(any && !in_trash),
                Item::new(crate::lang::t("Put Back"), "", Act::Restore).when(any && in_trash),
                Item::new(crate::lang::t("Empty Trash"), "", Act::EmptyTrash),
                Item::rule(),
                Item::new(crate::lang::t("Get Info"), "Ctrl+I", Act::Info),
                Item::rule(),
                Item::new(crate::lang::t("Close Window"), "Ctrl+W", Act::Close),
            ],
            1 => vec![
                Item::new(crate::lang::t("Undo"), "Ctrl+Z", Act::Undo).when(!self.undo.is_empty()),
                Item::rule(),
                Item::new(crate::lang::t("Cut"), "Ctrl+X", Act::Cut).when(any && !in_trash),
                Item::new(crate::lang::t("Copy"), "Ctrl+C", Act::Copy).when(any),
                Item::new(crate::lang::t("Paste"), "Ctrl+V", Act::Paste).when(self.clip.is_some() && !in_trash),
                Item::rule(),
                Item::new(crate::lang::t("Select All"), "Ctrl+A", Act::PickAll),
                Item::new(crate::lang::t("Copy Path"), "Shift+Ctrl+C", Act::CopyPath),
                Item::rule(),
                Item::new(crate::lang::t("Settings"), "Ctrl+,", Act::Prefs),
            ],
            2 => {
                let mut items: Vec<Item> = Mode::ALL
                    .iter()
                    .zip(["Ctrl+1", "Ctrl+2", "Ctrl+3", "Ctrl+4"])
                    .map(|(mode, keys)| Item::new(mode.name(), keys, Act::Show(*mode)).checked(p.mode == *mode))
                    .collect();
                items.push(Item::rule());
                items.push(Item::more(
                    crate::lang::t("Sort By"),
                    Sort::ALL.iter().map(|sort| Item::new(sort.name(), "", Act::SortBy(*sort)).checked(p.sort == *sort)).collect(),
                ));
                items.push(Item::new(crate::lang::t("Zoom In"), "Ctrl++", Act::ZoomIn).when(p.zoom < prefs::MOST_ZOOM));
                items.push(Item::new(crate::lang::t("Zoom Out"), "Ctrl+-", Act::ZoomOut).when(p.zoom > prefs::LEAST_ZOOM));
                items.push(Item::rule());
                items.push(Item::new(crate::lang::t("Show Hidden Files"), "Ctrl+H", Act::Hidden).checked(p.hidden));
                items.push(Item::new(crate::lang::t("Show Sidebar"), "F9", Act::Tree).checked(p.tree || p.places));
                items.push(Item::new(crate::lang::t("Show Status Bar"), "", Act::Status).checked(p.status));
                items.push(Item::rule());
                items.push(Item::new(crate::lang::t("Reload"), "F5", Act::Reload));
                items
            }
            3 => {
                let mut items = vec![
                    Item::new(crate::lang::t("Back"), "Alt+Left", Act::Back).when(!self.back.is_empty()),
                    Item::new(crate::lang::t("Forward"), "Alt+Right", Act::Forward).when(!self.ahead.is_empty()),
                    Item::new(crate::lang::t("Enclosing Folder"), "Alt+Up", Act::Up).when(self.dir.parent().is_some()),
                    Item::rule(),
                ];
                for (path, name, _) in self.places() {
                    items.push(Item::new(&name, "", Act::Go(path)));
                }
                items.push(Item::new(crate::lang::t("Computer"), "", Act::Go(PathBuf::from("/"))));
                items.push(Item::rule());
                items.push(Item::new(crate::lang::t("Go to Folder"), "Ctrl+L", Act::EditPath));
                items
            }
            _ => vec![Item::new(crate::lang::t("Keyboard Shortcuts"), "Ctrl+?", Act::Keys), Item::new(crate::lang::t("About Explorer"), "", Act::About)],
        }
    }

    pub fn open_with_items(&mut self) -> Vec<Item> {
        let Some(one) = self.chosen().into_iter().find(|each| !each.folder).cloned() else {
            return Vec::new();
        };
        self.handlers_for(one.mime).into_iter().map(|handler| Item::new(&handler.name, "", Act::OpenWith(handler.id))).collect()
    }

    pub fn item_menu(&mut self) -> Vec<Item> {
        let in_trash = ops::in_trash(&self.dir);
        let one = self.picked.len() == 1;
        let folder = self.single().filter(|one| one.folder).map(|one| one.path.clone());
        let mut items = Vec::new();
        if in_trash {
            items.push(Item::new(crate::lang::t("Put Back"), "", Act::Restore));
            items.push(Item::new(crate::lang::t("Delete Forever"), "Shift+Delete", Act::DeleteForever));
            items.push(Item::rule());
            items.push(Item::new(crate::lang::t("Get Info"), "Ctrl+I", Act::Info).when(one));
            return items;
        }
        items.push(Item::new(crate::lang::t("Open"), "Enter", Act::Open));
        match &folder {
            Some(path) => {
                items.push(Item::new(crate::lang::t("Open in New Window"), "", Act::Window(path.clone())));
                items.push(Item::new(crate::lang::t("Open in Terminal"), "", Act::Terminal(Some(path.clone()))));
                if !self.marked(path) {
                    items.push(Item::new(crate::lang::t("Add to Sidebar"), "", Act::Mark(path.clone())));
                }
            }
            None => items.push(Item::more(crate::lang::t("Open With"), self.open_with_items())),
        }
        items.push(Item::rule());
        items.push(Item::new(crate::lang::t("Cut"), "Ctrl+X", Act::Cut));
        items.push(Item::new(crate::lang::t("Copy"), "Ctrl+C", Act::Copy));
        if let Some(path) = folder.filter(|_| self.clip.is_some()) {
            items.push(Item::new(crate::lang::t("Paste Into Folder"), "", Act::PasteInto(path)));
        }
        items.push(Item::new(crate::lang::t("Copy Path"), "Shift+Ctrl+C", Act::CopyPath));
        items.push(Item::rule());
        items.push(Item::new(crate::lang::t("Rename"), "F2", Act::Rename).when(one));
        items.push(Item::new(crate::lang::t("Duplicate"), "Ctrl+D", Act::Duplicate));
        items.push(Item::new(crate::lang::t("Move to Trash"), "Delete", Act::Trash));
        items.push(Item::rule());
        items.push(Item::new(crate::lang::t("Get Info"), "Ctrl+I", Act::Info).when(one));
        items
    }

    pub fn line_menu(&mut self, path: &Path, mark: bool) -> Vec<Item> {
        let mut items = vec![
            Item::new(crate::lang::t("Open"), "", Act::Go(path.to_path_buf())),
            Item::new(crate::lang::t("Open in New Window"), "", Act::Window(path.to_path_buf())),
        ];
        if path != ops::trash_files() {
            items.push(Item::new(crate::lang::t("Open in Terminal"), "", Act::Terminal(Some(path.to_path_buf()))));
        }
        match (mark, path == ops::trash_files()) {
            (true, _) => {
                items.push(Item::rule());
                items.push(Item::new(crate::lang::t("Remove from Sidebar"), "", Act::Unmark(path.to_path_buf())));
            }
            (false, true) => {
                items.push(Item::rule());
                items.push(Item::new(crate::lang::t("Empty Trash"), "", Act::EmptyTrash));
            }
            (false, false) if !self.marked(path) && path.is_dir() && !self.places().iter().any(|(each, _, _)| each == path) => {
                items.push(Item::rule());
                items.push(Item::new(crate::lang::t("Add to Sidebar"), "", Act::Mark(path.to_path_buf())));
            }
            _ => {}
        }
        items
    }

    pub fn drop_on(&mut self, target: super::look::Drop, paths: Vec<PathBuf>, copy: bool) {
        match target {
            super::look::Drop::Mark => self.mark(&paths),
            super::look::Drop::Trash => self.start_job(Job::Trash(paths)),
            super::look::Drop::Into(into) => {
                let paths: Vec<PathBuf> = paths.into_iter().filter(|path| path != &into && !into.starts_with(path)).collect();
                if paths.is_empty() || ops::in_trash(&into) {
                    return;
                }
                let job = match copy {
                    true => Job::Copy { from: paths, into },
                    false => Job::Move { from: paths, into },
                };
                self.start_job(job);
            }
        }
    }

    pub fn space_menu(&mut self) -> Vec<Item> {
        let in_trash = ops::in_trash(&self.dir);
        let p = self.prefs.clone();
        if in_trash {
            return vec![Item::new(crate::lang::t("Empty Trash"), "", Act::EmptyTrash), Item::rule(), Item::new(crate::lang::t("Select All"), "Ctrl+A", Act::PickAll)];
        }
        vec![
            Item::new(crate::lang::t("New Folder"), "Shift+Ctrl+N", Act::NewFolder),
            Item::new(crate::lang::t("New Text File"), "", Act::NewFile),
            Item::new(crate::lang::t("Paste"), "Ctrl+V", Act::Paste).when(self.clip.is_some()),
            Item::rule(),
            Item::new(crate::lang::t("Select All"), "Ctrl+A", Act::PickAll),
            Item::more(crate::lang::t("View"), Mode::ALL.iter().map(|mode| Item::new(mode.name(), "", Act::Show(*mode)).checked(p.mode == *mode)).collect()),
            Item::more(crate::lang::t("Sort By"), Sort::ALL.iter().map(|sort| Item::new(sort.name(), "", Act::SortBy(*sort)).checked(p.sort == *sort)).collect()),
            Item::new(crate::lang::t("Show Hidden Files"), "Ctrl+H", Act::Hidden).checked(p.hidden),
            Item::rule(),
            Item::new(crate::lang::t("Open in Terminal"), "", Act::Terminal(None)),
            Item::new(crate::lang::t("Copy Path"), "Shift+Ctrl+C", Act::CopyPath),
            Item::new(crate::lang::t("Get Info"), "Ctrl+I", Act::Info),
        ]
    }

    pub fn summary(&self) -> String {
        let all = entry::items(self.entries.len());
        match self.picked.len() {
            0 => all,
            picked => {
                let bytes: u64 = self.chosen().iter().map(|each| each.size).sum();
                let folders = self.chosen().iter().all(|each| each.folder);
                match folders {
                    true => crate::lang::tf("{} of {} selected", &[&picked, &all]),
                    false => crate::lang::tf("{} of {} selected, {}", &[&picked, &all, &entry::size_words(bytes)]),
                }
            }
        }
    }

    pub fn free_words(&self) -> Option<String> {
        let (free, _) = self.free?;
        Some(crate::lang::tf("{} free", &[&crate::disks::size_words(free)]))
    }

    pub fn date(&self, when: Option<SystemTime>) -> String {
        entry::date_words(when, self.now, self.prefs.dates == Dates::Exact)
    }
}

fn zoom_step(zoom: u32) -> u32 {
    match zoom {
        0..48 => 8,
        48..96 => 16,
        96..160 => 32,
        _ => 48,
    }
}

fn gone_words(paths: &[PathBuf]) -> String {
    match paths {
        [one] => crate::lang::tf("{} will go.", &[&format!("\"{}\"", entry::name_of(one))]),
        _ => crate::lang::tf("{} will go.", &[&entry::items(paths.len())]),
    }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

pub fn info_of(entry: Entry) -> Info {
    use std::os::unix::fs::PermissionsExt;
    let inside = entry.folder.then(|| std::fs::read_dir(&entry.path).map(|items| items.count()).ok()).flatten();
    let mode = std::fs::metadata(&entry.path).ok().map(|meta| meta.permissions().mode());
    let target = entry.link.then(|| std::fs::read_link(&entry.path).ok()).flatten();
    Info { entry, inside, mode, target }
}

pub fn mode_words(mode: u32) -> String {
    let who = [crate::lang::t("You"), crate::lang::t("Group"), crate::lang::t("Others")];
    let mut parts = Vec::new();
    for (at, name) in who.iter().enumerate() {
        let bits = (mode >> (6 - at * 3)) & 7;
        let can = match (bits & 4 != 0, bits & 2 != 0) {
            (true, true) => crate::lang::t("read and write"),
            (true, false) => crate::lang::t("read only"),
            (false, true) => crate::lang::t("write only"),
            (false, false) => crate::lang::t("no access"),
        };
        parts.push(format!("{name}: {can}"));
    }
    parts.join(", ")
}

fn peek(path: &Path) -> Option<Vec<String>> {
    use std::io::Read;
    let mut bytes = vec![0u8; 16 * 1024];
    let read = std::fs::File::open(path).ok()?.read(&mut bytes).ok()?;
    bytes.truncate(read);
    if bytes.contains(&0) {
        return None;
    }
    let text = match std::str::from_utf8(&bytes) {
        Ok(text) => text.to_string(),
        Err(error) if error.valid_up_to() > bytes.len().saturating_sub(4) => String::from_utf8_lossy(&bytes[..error.valid_up_to()]).into_owned(),
        Err(_) => return None,
    };
    Some(text.lines().take(80).map(|line| line.replace('\t', "    ")).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Place(PathBuf);

    impl Drop for Place {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn place(tag: &str) -> (Place, Model) {
        let dir = std::env::temp_dir().join(format!("drifter-model-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("work/Photos/2026")).unwrap();
        std::fs::write(dir.join("work/b.txt"), "bee\nline two").unwrap();
        std::fs::write(dir.join("work/a.png"), [0u8; 10]).unwrap();
        std::fs::write(dir.join("work/.hidden"), "x").unwrap();
        let model = Model::new(dir.join("explorer.toml"), Some(dir.join("work")));
        (Place(dir), model)
    }

    #[test]
    fn paths_typed_by_hand_resolve() {
        let home = Path::new("/home/me");
        let dir = Path::new("/home/me/Music");
        assert_eq!(expand("~/Pictures", dir, home), PathBuf::from("/home/me/Pictures"));
        assert_eq!(expand("../Videos/", dir, home), PathBuf::from("/home/me/Videos"));
        assert_eq!(expand("file:///etc", dir, home), PathBuf::from("/etc"));
        assert_eq!(expand("/..", dir, home), PathBuf::from("/"));
    }

    #[test]
    fn a_folder_opens_with_folders_first() {
        let (place, model) = place("open");
        assert_eq!(model.dir, place.0.join("work"));
        let names: Vec<&str> = model.entries.iter().map(|each| each.name.as_str()).collect();
        assert_eq!(names, ["Photos", "a.png", "b.txt"]);
    }

    #[test]
    fn back_forward_and_up_walk_the_history() {
        let (place, mut model) = place("walk");
        let work = place.0.join("work");
        model.go(work.join("Photos"), None);
        model.go(work.join("Photos/2026"), None);
        model.act(Act::Back);
        assert_eq!(model.dir, work.join("Photos"));
        assert!(model.picked.contains(&work.join("Photos/2026")));
        model.act(Act::Forward);
        assert_eq!(model.dir, work.join("Photos/2026"));
        model.act(Act::Up);
        model.act(Act::Up);
        assert_eq!(model.dir, work);
        assert_eq!(model.focus, Some(work.join("Photos")));
    }

    #[test]
    fn shift_picks_a_range_and_steps_move() {
        let (_place, mut model) = place("pick");
        model.step(1, false);
        assert_eq!(model.focus_index(), Some(0));
        model.step(2, true);
        assert_eq!(model.picked.len(), 3);
        model.step(-1, false);
        assert_eq!(model.picked.len(), 1);
        assert_eq!(model.focus_index(), Some(1));
        model.type_to_find("b");
        assert_eq!(model.single().map(|one| one.name.as_str()), Some("b.txt"));
        assert_eq!(model.peek.as_ref().map(|lines| lines.len()), Some(2));
    }

    #[test]
    fn hidden_files_show_on_request() {
        let (_place, mut model) = place("hidden");
        assert_eq!(model.entries.len(), 3);
        model.act(Act::Hidden);
        assert_eq!(model.entries.len(), 4);
        assert!(!model.saved);
        model.save();
        assert!(Prefs::load_from(&model.prefs_path).hidden);
    }

    #[test]
    fn a_new_folder_starts_a_rename_and_the_rename_lands() {
        let (place, mut model) = place("rename");
        model.act(Act::NewFolder);
        let (path, field) = model.renaming.clone().unwrap();
        assert_eq!(field.picked_text(), Some("untitled folder"));
        model.renaming.as_mut().unwrap().1.put("Music");
        model.finish_rename();
        assert!(!path.exists());
        assert!(place.0.join("work/Music").is_dir());
        assert_eq!(model.single().map(|one| one.name.as_str()), Some("Music"));
    }

    #[test]
    fn columns_follow_the_path_and_the_picked_folder() {
        let (place, mut model) = place("columns");
        model.prefs.mode = Mode::Columns;
        model.home = place.0.clone();
        model.go(place.0.join("work/Photos"), None);
        let dirs: Vec<PathBuf> = model.columns.iter().map(|column| column.dir.clone()).collect();
        assert_eq!(dirs, [place.0.clone(), place.0.join("work"), place.0.join("work/Photos")]);
        model.pick_only(place.0.join("work/Photos/2026"));
        assert_eq!(model.columns.last().unwrap().dir, place.0.join("work/Photos/2026"));
    }

    #[test]
    fn the_tree_opens_down_to_the_folder() {
        let (place, mut model) = place("tree");
        model.home = place.0.clone();
        model.open.insert(place.0.clone());
        model.go(place.0.join("work/Photos/2026"), None);
        let lines = model.lines();
        let folders: Vec<(String, usize)> = lines
            .iter()
            .filter_map(|line| match line {
                Line::Folder { path, depth, .. } => Some((entry::name_of(path), *depth)),
                _ => None,
            })
            .collect();
        assert!(folders.contains(&("2026".to_string(), 3)), "{folders:?}");
    }

    #[test]
    fn copy_and_paste_ask_for_a_job() {
        let (place, mut model) = place("paste");
        model.pick_only(place.0.join("work/b.txt"));
        model.act(Act::Copy);
        model.go(place.0.join("work/Photos"), None);
        model.effects.clear();
        model.act(Act::Paste);
        assert_eq!(model.effects, [Effect::Job(Job::Copy { from: vec![place.0.join("work/b.txt")], into: place.0.join("work/Photos") })]);
        let job = match model.effects.pop() {
            Some(Effect::Job(job)) => job,
            _ => unreachable!(),
        };
        let done = ops::run_with(&place.0.join("Trash"), job);
        model.finished(done);
        assert!(model.busy.is_empty());
        assert_eq!(model.single().map(|one| one.name.as_str()), Some("b.txt"));
        assert_eq!(model.undo.len(), 1);
    }

    #[test]
    fn folders_pin_to_the_sidebar_and_drops_move_files() {
        use super::super::look::Drop;
        let (place, mut model) = place("drop");
        model.marks_path = place.0.join("bookmarks");
        model.marks.clear();
        let photos = place.0.join("work/Photos");
        model.drop_on(Drop::Mark, vec![photos.clone(), place.0.join("work/b.txt")], false);
        assert!(model.marked(&photos));
        assert_eq!(super::super::marks::load_from(&model.marks_path).len(), 1);
        assert!(model.lines().iter().any(|line| matches!(line, Line::Place { path, mark: true, .. } if path == &photos)));
        model.effects.clear();
        model.drop_on(Drop::Into(photos.clone()), vec![place.0.join("work/b.txt")], false);
        assert_eq!(model.effects, [Effect::Job(Job::Move { from: vec![place.0.join("work/b.txt")], into: photos.clone() })]);
        model.effects.clear();
        model.drop_on(Drop::Into(photos.join("2026")), vec![photos.clone()], false);
        assert!(model.effects.is_empty());
        model.act(Act::Unmark(photos.clone()));
        assert!(!model.marked(&photos));
    }

    #[test]
    fn drives_come_from_the_mount_table() {
        let mounts = "/dev/sda1 / ext4 rw 0 0\n/dev/sdb1 /run/media/me/My\\040Stick vfat rw 0 0\n/dev/sdc1 /mnt/backup ext4 rw 0 0\n";
        let found = drives_in(mounts);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].1, "My Stick");
    }

    #[test]
    fn tab_finishes_a_folder_name() {
        let (place, model) = place("complete");
        let work = place.0.join("work");
        assert_eq!(complete("Ph", &work, &model.home, false), Some("Photos/".to_string()));
        assert_eq!(complete("Photos/2", &work, &model.home, false), Some("Photos/2026/".to_string()));
        assert_eq!(complete("zz", &work, &model.home, false), None);
    }

    #[test]
    fn permissions_read_as_words() {
        assert_eq!(mode_words(0o640), "You: read and write, Group: read only, Others: no access");
    }
}
