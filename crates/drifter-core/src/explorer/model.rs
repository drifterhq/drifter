use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use super::clip::Held;
use super::entry::{self, Entry, Family, Sort};
use super::field::Field;
use super::ops::{self, Clash, Done, Job, Progress, Undo};
use super::open::{self, Handler};
use super::prefs::{self, Click, Dates, Mode, Prefs, Root, Start, View};
use super::read::{Ask, Got, Lens, Listing, Reader};

const BUDGET: Duration = match cfg!(test) {
    true => Duration::from_secs(5),
    false => Duration::from_millis(50),
};
const SHOW_LOADING: Duration = Duration::from_millis(250);
const SHOW_JOB: Duration = Duration::from_millis(400);

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
    Dismiss,
    Stop(u64),
    StopAndClose,
    Clash(Clash),
    Compress,
    Extract,
    TrashThese(Vec<PathBuf>),
    DeleteThese(Vec<PathBuf>),
    Search,
    EndSearch,
    Mount(String),
    Unmount(String),
    Eject(String),
    NewTab,
    Tab(PathBuf),
    ShowTab(usize),
    NextTab(isize),
    CloseTab(usize),
    CloseOtherTabs(usize),
    OtherApp,
    PickApp(usize),
    AlwaysApp,
    ChooseApp,
    CloseSheet,
    Lock,
    Runs,
    ChangeApp,
    NameInInfo,
    NamedInInfo,
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
    pub total: Option<(u64, u64)>,
    pub opens: Option<String>,
    pub writers: Option<u32>,
    pub naming: Option<Field>,
}

impl Info {
    pub fn locked(&self) -> Option<bool> {
        self.mode.map(|mode| mode & 0o222 == 0)
    }

    pub fn runs(&self) -> Option<bool> {
        self.mode.filter(|_| !self.entry.folder).map(|mode| mode & 0o100 != 0)
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Chooser {
    pub mime: &'static str,
    pub kind: String,
    pub name: String,
    pub files: Vec<PathBuf>,
    pub apps: Vec<Handler>,
    pub suggested: usize,
    pub at: Option<usize>,
    pub always: bool,
    pub scroll: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Sheet {
    Prefs(usize),
    Info(Info),
    Confirm { title: String, words: String, yes: String, act: Act },
    Clash { from: Vec<PathBuf>, into: PathBuf, cut: bool, names: Vec<String> },
    Keys,
    About,
    Apps(Chooser),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Line {
    Head(&'static str),
    Place { path: PathBuf, name: String, icon: &'static str, mark: bool },
    Folder { path: PathBuf, depth: usize, open: bool },
    Drive { path: Option<PathBuf>, name: String, drive: Option<super::drives::Drive> },
}

impl Line {
    pub fn path(&self) -> Option<&Path> {
        match self {
            Line::Head(_) => None,
            Line::Place { path, .. } | Line::Folder { path, .. } => Some(path),
            Line::Drive { path, .. } => path.as_deref(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Column {
    pub dir: PathBuf,
    pub entries: Vec<Entry>,
    pub scroll: f64,
}

#[derive(Debug, Clone)]
pub struct Search {
    pub field: Field,
    pub focused: bool,
    pub seq: u64,
    pub found: Vec<Entry>,
    pub done: bool,
    pub capped: bool,
    pub mode_before: Option<Mode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pane {
    #[default]
    Files,
    Side,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tab {
    pub dir: PathBuf,
    pub back: Vec<PathBuf>,
    pub ahead: Vec<PathBuf>,
    pub picked: HashSet<PathBuf>,
    pub focus: Option<PathBuf>,
    pub anchor: Option<PathBuf>,
    pub scroll: f64,
    pub view: View,
}

impl Tab {
    fn at(dir: PathBuf, view: View) -> Tab {
        Tab { dir, back: Vec::new(), ahead: Vec::new(), picked: HashSet::new(), focus: None, anchor: None, scroll: 0.0, view }
    }
}

pub fn title_of(dir: &Path) -> String {
    match ops::in_trash(dir) {
        true => crate::lang::t("Trash").to_string(),
        false => entry::name_of(dir),
    }
}

#[derive(Debug, Clone)]
pub struct Running {
    pub id: u64,
    pub stoppable: bool,
    pub words: String,
    pub progress: std::sync::Arc<Progress>,
    pub started: Instant,
}

impl Running {
    pub fn shown(&self) -> bool {
        self.started.elapsed() >= SHOW_JOB
    }

    pub fn detail(&self) -> String {
        use std::sync::atomic::Ordering::Relaxed;
        let p = &self.progress;
        if p.stopped() {
            return crate::lang::t("Stopping…").to_string();
        }
        if p.counting.load(Relaxed) {
            return crate::lang::t("Getting ready…").to_string();
        }
        let (bytes, total) = (p.bytes.load(Relaxed), p.total.load(Relaxed));
        if total == 0 {
            let (items, count) = (p.items.load(Relaxed), p.count.load(Relaxed));
            return match count {
                0 => String::new(),
                _ => crate::lang::tf("{} of {}", &[&items.min(count), &entry::items(count as usize)]),
            };
        }
        let sizes = crate::lang::tf("{} of {}", &[&entry::size_words(bytes.min(total)), &entry::size_words(total)]);
        let spent = self.started.elapsed().as_secs_f64();
        match bytes > 0 && spent >= 3.0 {
            true => {
                let left = spent * (total.saturating_sub(bytes)) as f64 / bytes as f64;
                format!("{sizes}, {}", time_left(left))
            }
            false => sizes,
        }
    }
}

pub fn time_left(seconds: f64) -> String {
    let seconds = seconds.ceil().max(1.0) as u64;
    match seconds {
        0..60 => crate::lang::tf("about {} s left", &[&seconds]),
        60..3600 => crate::lang::tf("about {} min left", &[&seconds.div_ceil(60)]),
        _ => crate::lang::tf("about {} h left", &[&(seconds / 3600)]),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Job(u64, Job),
    Launch(Vec<String>),
    Hold(super::clip::Held),
    NewWindow(PathBuf),
    Measure(PathBuf),
    Close,
}

pub struct Model {
    pub prefs: Prefs,
    pub prefs_path: PathBuf,
    pub view: View,
    pub views: Vec<(PathBuf, View)>,
    pub views_path: PathBuf,
    views_touched: Vec<PathBuf>,
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
    pub scroll_rest: f64,
    pub tree_rest: f64,
    pub open: HashSet<PathBuf>,
    pub kids: HashMap<PathBuf, Vec<PathBuf>>,
    pub columns: Vec<Column>,
    pub path: Option<Field>,
    pub renaming: Option<(PathBuf, Field)>,
    pub menu: Option<Menu>,
    pub sheet: Option<Sheet>,
    pub clip: Option<(Vec<PathBuf>, bool)>,
    pub clip_text: Option<String>,
    pub mime_list: PathBuf,
    pub pop: f64,
    pub undo: Vec<Undo>,
    pub jobs: Vec<Running>,
    pub job_ids: u64,
    pub closing: bool,
    pub note: Option<(String, Instant)>,
    pub alerts: Vec<String>,
    pub peek: Option<Vec<String>>,
    pub typed: (String, Option<Instant>),
    pub now: SystemTime,
    pub free: Option<(u64, u64)>,
    pub stamp: Option<SystemTime>,
    pub handlers: HashMap<&'static str, Vec<Handler>>,
    pub terminals: Vec<(&'static str, &'static str)>,
    pub drives: Vec<super::drives::Drive>,
    pub mounts_seen: String,
    pub drive_watch: Vec<PathBuf>,
    pub effects: Vec<Effect>,
    pub dragging: Option<Drag>,
    pub hover: Option<super::look::Hit>,
    pub tip: Option<super::look::Hit>,
    pub pane: Pane,
    pub ring: bool,
    pub side_at: Option<usize>,
    pub saved: bool,
    pub marks: Vec<super::marks::Mark>,
    pub marks_path: PathBuf,
    pub marks_stamp: Option<SystemTime>,
    pub place_list: Vec<(PathBuf, String, &'static str)>,
    pub search: Option<Search>,
    pub tabs: Vec<Tab>,
    pub tab: usize,
    pub reader: Reader,
    pub listed: HashMap<PathBuf, Listing>,
    pub loading: Option<Instant>,
    pub wanted: Option<PathBuf>,
    pub budget: Duration,
    pub peek_for: Option<PathBuf>,
    fresh_dir: bool,
    fresh_columns: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Drag {
    Split { from: f64, width: u32 },
    Rail { rail: super::look::Rail, grab: f64 },
    Edge { sort: Sort, rest: f64 },
    Lasso { from: crate::Vec2, at: crate::Vec2, keep: HashSet<PathBuf> },
    Text(super::look::Spot),
    Zoom,
    Slider,
    Files { paths: Vec<PathBuf>, from: crate::Vec2, at: crate::Vec2, live: bool, pending: Option<PathBuf> },
    Foreign { at: crate::Vec2, paths: Vec<PathBuf> },
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

impl Model {
    pub fn new(prefs_path: PathBuf, asked: Option<PathBuf>) -> Model {
        let prefs = Prefs::load_from(&prefs_path);
        let home = home();
        let last = match prefs.start {
            Start::Last => prefs.last.clone(),
            Start::Home => None,
        };
        let from_last = asked.is_none() && last.is_some();
        let start = asked.or(last).unwrap_or_else(|| home.clone());
        let views_path = prefs::views_path(&prefs_path);
        let mut model = Model {
            view: prefs.view(),
            views: prefs::load_views(&views_path),
            views_path,
            views_touched: Vec::new(),
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
            scroll_rest: 0.0,
            tree_rest: 0.0,
            open: HashSet::new(),
            kids: HashMap::new(),
            columns: Vec::new(),
            path: None,
            renaming: None,
            menu: None,
            sheet: None,
            clip: None,
            clip_text: None,
            mime_list: crate::desktop::user_mime_list(),
            pop: 1.0,
            undo: Vec::new(),
            jobs: Vec::new(),
            job_ids: 0,
            closing: false,
            note: None,
            alerts: Vec::new(),
            peek: None,
            typed: (String::new(), None),
            now: SystemTime::now(),
            free: None,
            stamp: None,
            handlers: HashMap::new(),
            terminals: prefs::terminals(),
            drives: Vec::new(),
            mounts_seen: String::new(),
            drive_watch: super::drives::watch_dirs(),
            effects: Vec::new(),
            dragging: None,
            hover: None,
            tip: None,
            pane: Pane::Files,
            ring: false,
            side_at: None,
            saved: true,
            marks: Vec::new(),
            marks_path: super::marks::file(),
            marks_stamp: None,
            place_list: Vec::new(),
            search: None,
            tabs: Vec::new(),
            tab: 0,
            reader: Reader::default(),
            listed: HashMap::new(),
            loading: None,
            wanted: None,
            budget: BUDGET,
            peek_for: None,
            fresh_dir: false,
            fresh_columns: false,
        };
        model.read_marks();
        model.mounts_seen = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
        model.reader.ask(Ask::Drives);
        model.open.insert(model.tree_base());
        model.refresh_places();
        model.load(start, None);
        if from_last && model.trouble.is_some() {
            let home = model.home.clone();
            model.load(home, None);
        }
        model.tabs = vec![model.tab_now()];
        if from_last && model.trouble.is_none() {
            model.reopen_tabs();
        }
        model
    }

    fn tab_now(&self) -> Tab {
        let view = match &self.search {
            Some(Search { mode_before: Some(mode), .. }) => View { mode: *mode, ..self.view },
            _ => self.view,
        };
        Tab { dir: self.dir.clone(), back: self.back.clone(), ahead: self.ahead.clone(), picked: self.picked.clone(), focus: self.focus.clone(), anchor: self.anchor.clone(), scroll: self.scroll, view }
    }

    pub fn view_for(&self, dir: &Path) -> View {
        self.views.iter().rfind(|(path, _)| path == dir).map_or_else(|| self.prefs.view(), |(_, view)| *view)
    }

    fn keep_view(&mut self) {
        let dir = self.dir.clone();
        self.views.retain(|(path, _)| path != &dir);
        if self.view != self.prefs.view() {
            self.views.push((dir, self.view));
            if self.views.len() > prefs::MOST_VIEWS {
                self.views.remove(0);
            }
        }
        if !self.views_touched.contains(&self.dir) {
            self.views_touched.push(self.dir.clone());
        }
    }

    pub fn tab_dirs(&self) -> Vec<PathBuf> {
        self.tabs.iter().enumerate().map(|(at, tab)| if at == self.tab { self.dir.clone() } else { tab.dir.clone() }).collect()
    }

    pub fn new_tab(&mut self, dir: PathBuf, show: bool) {
        self.tabs[self.tab] = self.tab_now();
        let at = self.tab + 1;
        let view = self.view_for(&dir);
        self.tabs.insert(at, Tab::at(dir, view));
        if show {
            self.show_tab(at);
        }
        self.remember_tabs();
    }

    fn remember_tabs(&mut self) {
        if self.tabs.is_empty() {
            return;
        }
        let dirs = self.tab_dirs();
        if self.prefs.tabs != dirs || self.prefs.tab != self.tab {
            self.prefs.tabs = dirs;
            self.prefs.tab = self.tab;
            self.saved = false;
        }
    }

    fn reopen_tabs(&mut self) {
        let saved: Vec<PathBuf> = self.prefs.tabs.iter().filter(|dir| dir.is_dir()).cloned().collect();
        if saved.len() < 2 {
            return;
        }
        let here = match saved.get(self.prefs.tab) {
            Some(dir) if *dir == self.dir => self.prefs.tab,
            _ => match saved.iter().position(|dir| *dir == self.dir) {
                Some(at) => at,
                None => return,
            },
        };
        self.tabs = saved.iter().map(|dir| Tab::at(dir.clone(), self.view_for(dir))).collect();
        self.tab = here;
        self.tabs[here] = self.tab_now();
        self.remember_tabs();
    }

    pub fn show_tab(&mut self, to: usize) {
        if to >= self.tabs.len() || to == self.tab {
            return;
        }
        self.tabs[self.tab] = self.tab_now();
        self.tab = to;
        let tab = self.tabs[to].clone();
        self.path = None;
        self.leave_search();
        self.view = tab.view;
        let dir = tab.dir.ancestors().find(|at| at.is_dir()).map_or_else(|| self.home.clone(), Path::to_path_buf);
        if !self.load(dir, None) {
            let home = self.home.clone();
            self.load(home, None);
        }
        self.back = tab.back;
        self.ahead = tab.ahead;
        if self.dir == tab.dir {
            let known: HashSet<&PathBuf> = self.entries.iter().map(|each| &each.path).collect();
            let fresh = !known.is_empty() || self.loading.is_none();
            self.picked = tab.picked.into_iter().filter(|path| !fresh || known.contains(path)).collect();
            self.focus = tab.focus.filter(|path| !fresh || known.contains(path));
            self.anchor = tab.anchor.filter(|path| !fresh || known.contains(path));
            self.scroll = tab.scroll;
            self.after_pick();
        }
        self.tabs[self.tab] = self.tab_now();
        self.remember_tabs();
    }

    pub fn close_tab(&mut self, at: usize) {
        if at >= self.tabs.len() {
            return;
        }
        if self.tabs.len() == 1 {
            return self.act(Act::Close);
        }
        if at == self.tab {
            let to = if at + 1 < self.tabs.len() { at + 1 } else { at - 1 };
            self.show_tab(to);
        }
        self.tabs.remove(at);
        if self.tab > at {
            self.tab -= 1;
        }
        self.remember_tabs();
    }

    pub fn tab_menu(&self, at: usize) -> Vec<Item> {
        vec![
            Item::new(crate::lang::t("New Tab"), "Ctrl+T", Act::NewTab),
            Item::rule(),
            Item::new(crate::lang::t("Close Tab"), "Ctrl+W", Act::CloseTab(at)),
            Item::new(crate::lang::t("Close Other Tabs"), "", Act::CloseOtherTabs(at)).when(self.tabs.len() > 1),
        ]
    }

    pub fn drive_of(&self, device: &str) -> Option<super::drives::Drive> {
        self.drives.iter().find(|drive| drive.device.as_deref() == Some(device)).cloned()
    }

    fn leave_drive(&mut self, device: &str) -> Option<super::drives::Drive> {
        let drive = self.drive_of(device)?;
        if let Some(point) = &drive.mount
            && self.dir.starts_with(point)
        {
            let home = self.home.clone();
            self.go(home, None);
        }
        Some(drive)
    }

    pub fn drive_menu(&self, drive: &super::drives::Drive) -> Vec<Item> {
        let mut items = Vec::new();
        let Some(device) = drive.device.clone() else {
            if let Some(point) = &drive.mount {
                items.push(Item::new(crate::lang::t("Open"), "", Act::Go(point.clone())));
                items.push(Item::new(crate::lang::t("Open in New Window"), "", Act::Window(point.clone())));
            }
            return items;
        };
        match &drive.mount {
            Some(point) => {
                items.push(Item::new(crate::lang::t("Open"), "", Act::Go(point.clone())));
                items.push(Item::new(crate::lang::t("Open in New Window"), "", Act::Window(point.clone())));
                items.push(Item::rule());
                items.push(Item::new(crate::lang::t("Unmount"), "", Act::Unmount(device.clone())));
            }
            None => items.push(Item::new(crate::lang::t("Mount"), "", Act::Mount(device.clone()))),
        }
        if drive.removable {
            items.push(Item::new(crate::lang::t("Eject"), "", Act::Eject(device)));
        }
        items
    }

    pub fn folders_of(&mut self, dir: &Path) -> Vec<PathBuf> {
        let hidden = self.prefs.hidden;
        let fresh = Ask::Folders(dir.to_path_buf(), hidden);
        if !self.kids.contains_key(dir) || !self.open.contains(dir) {
            self.reader.ask(fresh.clone());
            let until = Instant::now() + self.budget;
            self.settle(until, |model| !model.reader.asked(&fresh));
        }
        self.kids.get(dir).cloned().unwrap_or_default()
    }

    pub fn folder_menu(&mut self, dir: &Path) -> Vec<Item> {
        const MOST: usize = 40;
        let kids = self.folders_of(dir);
        let here = self.dir.clone();
        let mut items: Vec<Item> = kids.iter().take(MOST).map(|kid| Item::new(&entry::name_of(kid), "", Act::Go(kid.clone())).checked(here.starts_with(kid))).collect();
        match kids.len() {
            0 => items.push(Item::new(crate::lang::t("No folders inside"), "", Act::Go(dir.to_path_buf())).when(false)),
            more if more > MOST => items.push(Item::new(&crate::lang::tf("{} more", &[&(more - MOST)]), "", Act::Go(dir.to_path_buf()))),
            _ => {}
        }
        items
    }

    pub fn searching(&self) -> bool {
        self.search.as_ref().is_some_and(|search| !search.field.text.trim().is_empty())
    }

    pub fn search_changed(&mut self) {
        let Some(search) = self.search.as_mut() else {
            return;
        };
        let words = search.field.text.trim().to_string();
        search.seq += 1;
        search.found.clear();
        search.capped = false;
        self.picked.clear();
        self.focus = None;
        self.anchor = None;
        self.scroll = 0.0;
        match words.is_empty() {
            true => {
                search.done = true;
                self.reader.stop_search();
                self.apply();
            }
            false => {
                search.done = false;
                let seq = search.seq;
                self.entries.clear();
                self.reader.search(self.dir.clone(), &words, self.prefs.hidden, seq);
            }
        }
    }

    fn leave_search(&mut self) {
        if let Some(search) = self.search.take() {
            self.reader.stop_search();
            if let Some(mode) = search.mode_before {
                self.view.mode = mode;
                self.rebuild_columns();
            }
        }
    }

    pub fn known_folder(&self, path: &Path) -> bool {
        let listed = self.entries.iter().chain(self.columns.iter().flat_map(|column| column.entries.iter())).find(|each| each.path == path);
        match listed {
            Some(each) => each.folder,
            None => self.kids.contains_key(path) || self.kids.values().any(|kids| kids.iter().any(|kid| kid == path)) || self.place_list.iter().any(|(each, _, _)| each == path),
        }
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

    pub fn hold(&mut self, held: Held) {
        self.heard_clip(Some(held.clone()));
        self.effects.push(Effect::Hold(held));
    }

    pub fn heard_clip(&mut self, held: Option<Held>) {
        self.clip_text = held.as_ref().map(Held::text);
        self.clip = match held {
            Some(Held::Files(paths, cut)) => Some((paths, cut)),
            _ => None,
        };
    }

    pub fn say(&mut self, words: impl Into<String>, alert: bool) {
        let words = words.into();
        if !alert {
            self.note = Some((words, Instant::now()));
            return;
        }
        self.alerts.retain(|each| each != &words);
        self.alerts.push(words);
        if self.alerts.len() > 20 {
            self.alerts.remove(0);
        }
    }

    pub fn lens(&self) -> Lens {
        Lens { hidden: self.prefs.hidden, sort: self.view.sort, down: self.view.down, folders_first: self.prefs.folders_first }
    }

    pub fn listing(&self, dir: &Path) -> Option<&Listing> {
        self.listed.get(dir).filter(|listing| listing.lens == self.lens())
    }

    fn ask_list(&mut self, dir: &Path) -> u64 {
        self.reader.ask(Ask::List(dir.to_path_buf(), self.lens()))
    }

    fn listed_since(&self, dir: &Path, seq: u64) -> bool {
        self.listing(dir).is_some_and(|listing| listing.seq >= seq)
    }

    fn settle(&mut self, until: Instant, done: impl Fn(&Model) -> bool) {
        while !done(self) {
            match self.reader.next(Some(until)) {
                Some(got) => {
                    self.absorb(got);
                }
                None => break,
            }
        }
    }

    pub fn settle_all(&mut self, most: Duration) {
        let until = Instant::now() + most;
        while self.reader.busy() {
            match self.reader.next(Some(until)) {
                Some(got) => {
                    self.absorb(got);
                }
                None => break,
            }
        }
        self.follow();
    }

    fn absorb(&mut self, got: Got) -> bool {
        match got {
            Got::List(dir, listing) => {
                if listing.lens != self.lens() {
                    return false;
                }
                if dir == self.dir {
                    self.fresh_dir = true;
                } else if self.columns.iter().any(|column| column.dir == dir) {
                    self.fresh_columns = true;
                }
                self.listed.insert(dir, listing);
                true
            }
            Got::Folders(dir, hidden, kids) => {
                if hidden != self.prefs.hidden {
                    return false;
                }
                self.kids.insert(dir, kids);
                true
            }
            Got::Stamp(dir, stamp) => {
                if dir != self.dir || stamp == self.stamp || self.loading.is_some() {
                    return false;
                }
                self.ask_list(&dir);
                false
            }
            Got::Drives(drives) => {
                let changed = drives != self.drives;
                self.drives = drives;
                changed
            }
            Got::Found { seq, entries, done, capped } => {
                let Some(search) = self.search.as_mut().filter(|search| search.seq == seq) else {
                    return false;
                };
                search.found.extend(entries);
                search.done = done;
                search.capped = capped;
                let mut found = search.found.clone();
                entry::sort(&mut found, self.view.sort, self.view.down, self.prefs.folders_first);
                self.entries = found;
                let here: HashSet<&PathBuf> = self.entries.iter().map(|each| &each.path).collect();
                self.picked.retain(|path| here.contains(path));
                true
            }
            Got::Changed(dirs) => {
                if dirs.iter().any(|dir| self.drive_watch.contains(dir)) {
                    self.reader.ask(Ask::Drives);
                }
                for dir in dirs {
                    if self.loading.is_none() && dir == self.dir {
                        self.ask_list(&dir);
                    } else if self.columns.iter().any(|column| column.dir == dir) {
                        self.ask_list(&dir);
                    }
                    if self.kids.contains_key(&dir) {
                        self.reader.ask(Ask::Folders(dir, self.prefs.hidden));
                    }
                }
                false
            }
            Got::Peek(path, lines) => {
                if self.peek_for.as_ref() != Some(&path) {
                    return false;
                }
                self.peek = lines;
                true
            }
        }
    }

    fn follow(&mut self) {
        if std::mem::take(&mut self.fresh_dir) {
            self.apply();
        } else if std::mem::take(&mut self.fresh_columns) {
            self.rebuild_columns();
        }
    }

    pub fn arrived(&mut self) -> bool {
        let mut changed = self.fresh_dir || self.fresh_columns;
        while let Some(got) = self.reader.next(None) {
            changed |= self.absorb(got);
        }
        self.follow();
        changed
    }

    fn apply(&mut self) {
        self.fresh_dir = false;
        let Some(listing) = self.listing(&self.dir).cloned() else {
            return;
        };
        if self.searching() {
            self.stamp = listing.stamp;
            self.free = listing.free;
            return;
        }
        let still = self.reader.asked(&Ask::List(self.dir.clone(), self.lens()));
        self.loading = match still {
            true => Some(self.loading.unwrap_or_else(Instant::now)),
            false => None,
        };
        match listing.entries {
            Ok(found) => {
                self.entries = found;
                self.trouble = None;
            }
            Err(error) => {
                self.entries.clear();
                self.trouble = Some(error);
            }
        }
        let here: HashSet<&PathBuf> = self.entries.iter().map(|each| &each.path).collect();
        self.picked.retain(|path| here.contains(path));
        self.focus = self.focus.take().filter(|path| here.contains(path));
        self.anchor = self.anchor.take().filter(|path| here.contains(path));
        if let Some((path, _)) = &self.renaming
            && !here.contains(path)
        {
            self.renaming = None;
        }
        let wanted = self.wanted.clone().filter(|path| here.contains(path));
        self.peek_for = None;
        self.stamp = listing.stamp;
        self.free = listing.free;
        match wanted {
            Some(pick) => {
                self.wanted = None;
                self.pick_only(pick);
                if self.view.mode != Mode::Columns {
                    self.rebuild_columns();
                }
            }
            None => {
                if !still {
                    self.wanted = None;
                }
                self.rebuild_columns();
                self.refresh_peek();
            }
        }
    }

    fn load(&mut self, dir: PathBuf, pick: Option<PathBuf>) -> bool {
        self.leave_search();
        let before = self.view;
        if self.view.mode != Mode::Columns {
            self.view = self.view_for(&dir);
        }
        let seq = self.ask_list(&dir);
        let until = Instant::now() + self.budget;
        self.settle(until, |model| model.listed_since(&dir, seq));
        if let Some(Listing { entries: Err(error), .. }) = self.listing(&dir).filter(|listing| listing.seq >= seq)
            && !self.entries.is_empty()
            && self.dir != dir
        {
            let words = crate::lang::tf("Cannot open {}: {}", &[&entry::name_of(&dir), error]);
            self.say(words, true);
            self.view = before;
            return false;
        }
        self.dir = dir;
        self.picked.clear();
        self.anchor = None;
        self.focus = None;
        self.scroll = 0.0;
        self.renaming = None;
        self.typed = (String::new(), None);
        self.wanted = pick;
        self.peek = None;
        self.peek_for = None;
        self.reveal();
        self.fill_tree(until);
        match self.listing(&self.dir).is_some() {
            true => self.apply(),
            false => {
                self.entries.clear();
                self.trouble = None;
                self.loading = Some(Instant::now());
                self.stamp = None;
                self.free = None;
                self.rebuild_columns();
            }
        }
        if self.prefs.last.as_ref() != Some(&self.dir) {
            self.prefs.last = Some(self.dir.clone());
            self.saved = false;
        }
        self.remember_tabs();
        self.sync_watches();
        true
    }

    fn fill_tree(&mut self, until: Instant) {
        let hidden = self.prefs.hidden;
        let base = self.tree_base();
        let wanted: Vec<PathBuf> = self.dir.ancestors().take_while(|at| at.starts_with(&base)).filter(|at| self.open.contains(*at) && !self.kids.contains_key(*at)).map(Path::to_path_buf).collect();
        for dir in &wanted {
            self.reader.ask(Ask::Folders(dir.clone(), hidden));
        }
        self.settle(until, |model| wanted.iter().all(|dir| model.kids.contains_key(dir)));
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
        let dir = self.dir.clone();
        let seq = self.ask_list(&dir);
        let hidden = self.prefs.hidden;
        let open = self.open.clone();
        self.kids.retain(|path, _| open.contains(path));
        for path in self.kids.keys().cloned().collect::<Vec<_>>() {
            self.reader.ask(Ask::Folders(path, hidden));
        }
        for column in self.columns.iter().map(|column| column.dir.clone()).filter(|each| each != &dir).collect::<Vec<_>>() {
            self.ask_list(&column);
        }
        let until = Instant::now() + self.budget;
        self.settle(until, |model| model.listed_since(&dir, seq));
        match self.listed_since(&dir, seq) {
            true => self.apply(),
            false => self.loading = Some(self.loading.unwrap_or_else(Instant::now)),
        }
        self.follow();
    }

    pub fn sync_watches(&mut self) {
        let mut dirs = vec![self.dir.clone()];
        dirs.extend(self.columns.iter().map(|column| column.dir.clone()));
        dirs.extend(self.kids.keys().filter(|dir| self.open.contains(*dir)).cloned());
        dirs.extend(self.drive_watch.iter().cloned());
        self.reader.watch(dirs);
    }

    pub fn showing_loading(&self) -> bool {
        self.loading.is_some_and(|at| at.elapsed() >= SHOW_LOADING)
    }

    pub fn tick(&mut self) -> bool {
        self.now = SystemTime::now();
        let mut changed = false;
        if self.note.as_ref().is_some_and(|(_, at)| at.elapsed().as_secs() >= 6) {
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
        if self.loading.is_none() {
            self.reader.ask(Ask::Stamp(self.dir.clone()));
        } else if self.showing_loading() {
            changed = true;
        }
        if self.jobs.iter().any(Running::shown) {
            changed = true;
        }
        let mounts = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
        if mounts != self.mounts_seen {
            self.mounts_seen = mounts;
            self.reader.ask(Ask::Drives);
        }
        let changed = changed | self.arrived();
        self.sync_watches();
        changed
    }

    pub fn unsaved(&self) -> bool {
        !self.saved || !self.views_touched.is_empty()
    }

    pub fn save(&mut self) {
        if !self.views_touched.is_empty() {
            let mut disk = prefs::load_views(&self.views_path);
            for dir in &self.views_touched {
                disk.retain(|(path, _)| path != dir);
                if let Some(mine) = self.views.iter().rfind(|(path, _)| path == dir) {
                    disk.push(mine.clone());
                }
            }
            if disk.len() > prefs::MOST_VIEWS {
                disk.drain(..disk.len() - prefs::MOST_VIEWS);
            }
            match prefs::save_views(&self.views_path, &disk) {
                Ok(()) => {
                    self.views_touched.clear();
                    self.views = disk;
                }
                Err(error) => self.say(crate::lang::tf("Settings were not saved: {}", &[&error]), true),
            }
        }
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
        self.refresh_places();
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
        self.refresh_places();
        match super::marks::save_to(&self.marks_path, &self.marks) {
            Ok(()) => self.marks_stamp = modified(&self.marks_path),
            Err(error) => self.say(crate::lang::tf("The sidebar was not saved: {}", &[&error]), true),
        }
    }

    pub fn places(&self) -> Vec<(PathBuf, String, &'static str)> {
        self.place_list.clone()
    }

    pub fn refresh_places(&mut self) {
        self.place_list = self.find_places();
    }

    fn find_places(&self) -> Vec<(PathBuf, String, &'static str)> {
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
            if self.drives.first().is_none_or(|first| first.mount.as_deref() != Some(Path::new("/"))) {
                out.push(Line::Drive { path: Some(PathBuf::from("/")), name: crate::lang::t("Computer").to_string(), drive: None });
            }
            for drive in &self.drives {
                out.push(Line::Drive { path: drive.mount.clone(), name: drive.name.clone(), drive: Some(drive.clone()) });
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
        let Some(kids) = self.kids.get(at).cloned() else {
            self.reader.ask(Ask::Folders(at.to_path_buf(), self.prefs.hidden));
            return;
        };
        for kid in kids {
            self.walk(&kid, depth + 1, out);
        }
    }

    pub fn fold(&mut self, path: &Path) {
        if self.open.remove(path) {
            return;
        }
        self.open.insert(path.to_path_buf());
        if !self.kids.contains_key(path) {
            let ask = Ask::Folders(path.to_path_buf(), self.prefs.hidden);
            self.reader.ask(ask);
            let until = Instant::now() + self.budget;
            self.settle(until, |model| model.kids.contains_key(path));
        }
    }

    pub fn rebuild_columns(&mut self) {
        self.fresh_columns = false;
        if self.view.mode != Mode::Columns {
            self.columns.clear();
            let dir = self.dir.clone();
            self.listed.retain(|each, _| *each == dir);
            return;
        }
        let base = self.column_base();
        let mut chain: Vec<PathBuf> = self.dir.ancestors().take_while(|at| at.starts_with(&base)).map(Path::to_path_buf).collect();
        chain.reverse();
        if chain.is_empty() {
            chain.push(self.dir.clone());
        }
        if let Some(folder) = self.single().filter(|one| one.folder).map(|one| one.path.clone()) {
            chain.push(folder);
        }
        let missing: Vec<PathBuf> = chain.iter().filter(|dir| **dir != self.dir && self.listing(dir).is_none()).cloned().collect();
        if !missing.is_empty() {
            for dir in &missing {
                self.ask_list(dir);
            }
            let until = Instant::now() + self.budget;
            self.settle(until, |model| missing.iter().all(|dir| model.listing(dir).is_some()));
        }
        let old: HashMap<PathBuf, f64> = self.columns.iter().map(|column| (column.dir.clone(), column.scroll)).collect();
        let mut columns = Vec::new();
        for dir in chain {
            let entries = match dir == self.dir {
                true => self.entries.clone(),
                false => self.listing(&dir).and_then(|listing| listing.entries.clone().ok()).unwrap_or_default(),
            };
            let scroll = old.get(&dir).copied().unwrap_or(0.0);
            columns.push(Column { dir, entries, scroll });
        }
        self.columns = columns;
        let keep: HashSet<&PathBuf> = self.columns.iter().map(|column| &column.dir).chain(std::iter::once(&self.dir)).collect();
        self.listed.retain(|dir, _| keep.contains(dir));
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
        if self.view.mode == Mode::Columns {
            self.rebuild_columns();
        }
        self.refresh_peek();
    }

    pub fn refresh_peek(&mut self) {
        let want = self.single().filter(|one| !one.folder && one.size < 4 << 20 && matches!(one.family(), Family::Text | Family::Other)).map(|one| one.path.clone());
        if want == self.peek_for {
            return;
        }
        self.peek = None;
        self.peek_for = want.clone();
        let Some(path) = want else {
            return;
        };
        self.reader.ask(Ask::Peek(path.clone()));
        let until = Instant::now() + self.budget;
        self.settle(until, |model| !model.reader.asked(&Ask::Peek(path.clone())));
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
            match handler.and_then(|handler| self.app_argv(&handler, &refs)) {
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
        if let Some(argv) = handler.and_then(|handler| self.app_argv(&handler, &refs)) {
            self.effects.push(Effect::Launch(argv));
        }
    }

    fn app_argv(&self, handler: &Handler, files: &[&Path]) -> Option<Vec<String>> {
        let argv = open::argv(handler, files)?;
        if !handler.terminal {
            return Some(argv);
        }
        let program = self.prefs.terminal.clone().or_else(|| self.terminals.first().map(|(program, _)| program.to_string()))?;
        Some(prefs::terminal_run(&program, argv))
    }

    pub fn start_job(&mut self, job: Job) {
        self.job_ids += 1;
        let id = self.job_ids;
        let stoppable = !matches!(job, Job::Undo(_));
        self.jobs.push(Running { id, stoppable, words: job.doing(), progress: Default::default(), started: Instant::now() });
        self.effects.push(Effect::Job(id, job));
    }

    pub fn progress_of(&self, id: u64) -> std::sync::Arc<Progress> {
        self.jobs.iter().find(|each| each.id == id).map(|each| each.progress.clone()).unwrap_or_default()
    }

    pub fn finished(&mut self, done: Done) {
        self.jobs.retain(|each| each.id != done.id);
        self.reader.ask(Ask::Drives);
        if let Some(point) = done.go.clone() {
            self.go(point, None);
        }
        if self.closing && self.jobs.is_empty() {
            self.effects.push(Effect::Close);
        }
        if done.stopped {
            self.say(crate::lang::tf("Stopped: {}", &[&done.words]), false);
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
        let own = self.views.iter().any(|(path, _)| path == &self.dir) || self.search.as_ref().is_some_and(|search| search.mode_before.is_some());
        if matches!(pref, Pref::Mode(_) | Pref::Sort(_) | Pref::Down) && !own {
            reread |= self.view.sort != self.prefs.sort || self.view.down != self.prefs.down;
            self.view = self.prefs.view();
        }
        if reread {
            self.reload();
        }
        if self.view.mode == Mode::Columns {
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
            Act::OtherApp => self.choose_app(open::all_apps()),
            Act::PickApp(at) => {
                if let Some(Sheet::Apps(chooser)) = self.sheet.as_mut()
                    && at < chooser.apps.len()
                {
                    chooser.at = Some(at);
                }
            }
            Act::AlwaysApp => {
                if let Some(Sheet::Apps(chooser)) = self.sheet.as_mut() {
                    chooser.always = !chooser.always;
                }
            }
            Act::ChooseApp => self.chose_app(),
            Act::NewWindow => self.effects.push(Effect::NewWindow(self.dir.clone())),
            Act::Window(path) => self.effects.push(Effect::NewWindow(path)),
            Act::NewTab => self.new_tab(self.dir.clone(), true),
            Act::Tab(path) => self.new_tab(path, true),
            Act::ShowTab(at) => self.show_tab(at),
            Act::NextTab(by) => {
                let count = self.tabs.len() as isize;
                self.show_tab((self.tab as isize + by).rem_euclid(count) as usize);
            }
            Act::CloseTab(at) => self.close_tab(at),
            Act::CloseOtherTabs(at) => {
                if at < self.tabs.len() {
                    self.show_tab(at);
                    self.tabs = vec![self.tab_now()];
                    self.tab = 0;
                    self.remember_tabs();
                }
            }
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
                    self.start_job(Job::Copy { from, into: self.dir.clone(), clash: Clash::KeepBoth });
                }
            }
            Act::Trash if in_trash => self.act(Act::DeleteForever),
            Act::Trash => {
                let paths = self.chosen_paths();
                if paths.is_empty() {
                    return;
                }
                match self.prefs.confirm_trash {
                    true => {
                        let words = gone_words(&paths);
                        self.confirm(crate::lang::t("Move to Trash?"), words, crate::lang::t("Move to Trash"), Act::TrashThese(paths));
                    }
                    false => self.start_job(Job::Trash(paths)),
                }
            }
            Act::Mount(device) => {
                if let Some(drive) = self.drive_of(&device) {
                    match &drive.mount {
                        Some(point) => self.go(point.clone(), None),
                        None => self.start_job(Job::Mount { device, name: drive.name }),
                    }
                }
            }
            Act::Unmount(device) => {
                if let Some(drive) = self.leave_drive(&device) {
                    self.start_job(Job::Unmount { device, name: drive.name });
                }
            }
            Act::Eject(device) => {
                if let Some(drive) = self.leave_drive(&device) {
                    let mounted = drive.mount.is_some();
                    self.start_job(Job::Eject { device, disk: drive.disk, name: drive.name, mounted });
                }
            }
            Act::Search => {
                match self.search.as_mut() {
                    Some(search) => {
                        search.focused = true;
                        search.field.pick_all();
                    }
                    None => {
                        let mode_before = (self.view.mode == Mode::Columns).then_some(Mode::Columns);
                        if mode_before.is_some() {
                            self.view.mode = Mode::List;
                        }
                        self.search = Some(Search { field: Field::new(""), focused: true, seq: 0, found: Vec::new(), done: true, capped: false, mode_before });
                    }
                }
                self.path = None;
                self.renaming = None;
            }
            Act::EndSearch => {
                self.leave_search();
                match self.listing(&self.dir).is_some() {
                    true => self.apply(),
                    false => self.reload(),
                }
            }
            Act::TrashThese(paths) => {
                self.sheet = None;
                self.start_job(Job::Trash(paths));
            }
            Act::DeleteForever => {
                let paths = self.chosen_paths();
                if paths.is_empty() {
                    return;
                }
                let words = format!("{} {}", gone_words(&paths), crate::lang::t("This cannot be undone."));
                self.confirm(crate::lang::t("Delete forever?"), words, crate::lang::t("Delete"), Act::DeleteThese(paths));
            }
            Act::DeleteThese(paths) => {
                self.sheet = None;
                self.start_job(Job::Delete(paths));
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
                    let mut info = info_of(one);
                    match info.entry.folder {
                        true => self.effects.push(Effect::Measure(info.entry.path.clone())),
                        false => info.opens = self.handlers_for(info.entry.mime).first().map(|app| app.name.clone()),
                    }
                    self.sheet = Some(Sheet::Info(info));
                }
            }
            Act::Close => match self.jobs.as_slice() {
                [] => self.effects.push(Effect::Close),
                [one, rest @ ..] => {
                    let words = match rest.len() {
                        0 => crate::lang::tf("{}. Closing the window stops it.", &[&one.words]),
                        more => crate::lang::tf("{} and {} more. Closing the window stops them.", &[&one.words, &more]),
                    };
                    self.confirm(crate::lang::t("Stop the work and close?"), words, crate::lang::t("Stop and Close"), Act::StopAndClose);
                }
            },
            Act::Compress if !in_trash => {
                let paths = self.chosen_paths();
                if !paths.is_empty() {
                    self.start_job(Job::Compress(paths));
                }
            }
            Act::Extract if !in_trash => {
                let paths: Vec<PathBuf> = self.chosen_paths().into_iter().filter(|path| super::pack::can_extract(path)).collect();
                if !paths.is_empty() {
                    self.start_job(Job::Extract(paths));
                }
            }
            Act::Compress | Act::Extract => {}
            Act::Clash(clash) => {
                if let Some(Sheet::Clash { from, into, cut, .. }) = self.sheet.take() {
                    self.send_with(from, into, cut, clash);
                }
            }
            Act::StopAndClose => {
                for job in &self.jobs {
                    job.progress.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
                self.closing = true;
                if self.jobs.is_empty() {
                    self.effects.push(Effect::Close);
                }
            }
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
                self.hold(Held::Files(paths, cut));
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
                self.hold(Held::Text(text));
                self.say(crate::lang::t("Path copied"), false);
            }
            Act::Prefs => self.sheet = Some(Sheet::Prefs(0)),
            Act::Show(mode) => {
                if let Some(search) = self.search.as_mut() {
                    search.mode_before = None;
                }
                self.view.mode = mode;
                self.scroll = 0.0;
                self.keep_view();
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
            Act::Dismiss => {
                self.alerts.pop();
            }
            Act::NameInInfo => {
                if let Some(Sheet::Info(info)) = self.sheet.as_mut()
                    && info.naming.is_none()
                    && !in_trash
                {
                    info.naming = Some(Field::stem(&info.entry.name));
                }
            }
            Act::NamedInInfo => self.named_in_info(),
            Act::Lock => self.change_mode(true),
            Act::Runs => self.change_mode(false),
            Act::ChangeApp => {
                if let Some(Sheet::Info(info)) = self.sheet.clone()
                    && !info.entry.folder
                {
                    self.pick_only(info.entry.path.clone());
                    self.choose_app(open::all_apps());
                    if let Some(Sheet::Apps(chooser)) = self.sheet.as_mut() {
                        chooser.always = true;
                    }
                }
            }
            Act::CloseSheet => {
                self.sheet = None;
                self.save();
            }
            Act::Stop(id) => {
                if let Some(job) = self.jobs.iter().find(|each| each.id == id) {
                    job.progress.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            Act::Reload => {
                self.refresh_places();
                self.reader.ask(Ask::Drives);
                self.handlers.clear();
                self.reload();
            }
            Act::SortBy(sort) => {
                match self.view.sort == sort {
                    true => self.view.down = !self.view.down,
                    false => {
                        self.view.sort = sort;
                        self.view.down = false;
                    }
                }
                self.keep_view();
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
        if cut {
            self.clip = None;
        }
        self.send(paths, into, cut);
    }

    fn send(&mut self, from: Vec<PathBuf>, into: PathBuf, cut: bool) {
        let names: Vec<String> = ops::clashes(&from, &into).iter().map(|path| entry::name_of(path)).collect();
        match names.is_empty() {
            true => self.send_with(from, into, cut, Clash::KeepBoth),
            false => self.sheet = Some(Sheet::Clash { from, into, cut, names }),
        }
    }

    fn send_with(&mut self, from: Vec<PathBuf>, into: PathBuf, cut: bool, clash: Clash) {
        match cut {
            true => self.start_job(Job::Move { from, into, clash }),
            false => self.start_job(Job::Copy { from, into, clash }),
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
                Item::new(crate::lang::t("New Tab"), "Ctrl+T", Act::NewTab),
                Item::new(crate::lang::t("New Folder"), "Shift+Ctrl+N", Act::NewFolder).when(!in_trash),
                Item::new(crate::lang::t("New Text File"), "", Act::NewFile).when(!in_trash),
                Item::rule(),
                Item::new(crate::lang::t("Open"), "Enter", Act::Open).when(any),
                Item::more(crate::lang::t("Open With"), self.open_with_items()),
                Item::new(crate::lang::t("Open in Terminal"), "", Act::Terminal(None)),
                Item::rule(),
                Item::new(crate::lang::t("Rename"), "F2", Act::Rename).when(self.picked.len() == 1 && !in_trash),
                Item::new(crate::lang::t("Duplicate"), "Ctrl+D", Act::Duplicate).when(any && !in_trash),
                Item::new(crate::lang::t("Compress"), "", Act::Compress).when(any && !in_trash),
                Item::new(crate::lang::t("Extract Here"), "", Act::Extract).when(any && !in_trash && self.chosen().iter().all(|each| !each.folder && super::pack::kind_of(&each.name).is_some())),
                Item::new(crate::lang::t("Move to Trash"), "Delete", Act::Trash).when(any && !in_trash),
                Item::new(crate::lang::t("Put Back"), "", Act::Restore).when(any && in_trash),
                Item::new(crate::lang::t("Empty Trash"), "", Act::EmptyTrash),
                Item::rule(),
                Item::new(crate::lang::t("Get Info"), "Ctrl+I", Act::Info),
                Item::rule(),
                Item::new(crate::lang::t("Close Tab"), "Ctrl+W", Act::CloseTab(self.tab)).when(self.tabs.len() > 1),
                Item::new(crate::lang::t("Close Window"), if self.tabs.len() > 1 { "Ctrl+Q" } else { "Ctrl+W" }, Act::Close),
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
                    .map(|(mode, keys)| Item::new(mode.name(), keys, Act::Show(*mode)).checked(self.view.mode == *mode))
                    .collect();
                items.push(Item::rule());
                items.push(Item::more(
                    crate::lang::t("Sort By"),
                    Sort::ALL.iter().map(|sort| Item::new(sort.name(), "", Act::SortBy(*sort)).checked(self.view.sort == *sort)).collect(),
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
        let mut items: Vec<Item> = self.handlers_for(one.mime).into_iter().map(|handler| Item::new(&handler.name, "", Act::OpenWith(handler.id))).collect();
        if !items.is_empty() {
            items.push(Item::rule());
        }
        items.push(Item::new(crate::lang::t("Other App…"), "", Act::OtherApp));
        items
    }

    fn named_in_info(&mut self) {
        let Some(Sheet::Info(info)) = self.sheet.as_mut() else {
            return;
        };
        let Some(field) = info.naming.take() else {
            return;
        };
        let path = info.entry.path.clone();
        match ops::rename(&path, &field.text) {
            Ok(made) if made == path => {}
            Ok(made) => {
                if let Some(entry) = Entry::at(&made) {
                    info.entry = entry;
                }
                self.undo.push(Undo::Renamed(path, made.clone()));
                self.reload();
                self.pick_only(made);
            }
            Err(words) => {
                info.naming = Some(field);
                self.say(words, true);
            }
        }
    }

    pub fn measured(&mut self, path: &Path, total: (u64, u64)) {
        if let Some(Sheet::Info(info)) = self.sheet.as_mut()
            && info.entry.path == path
        {
            info.total = Some(total);
        }
    }

    fn change_mode(&mut self, lock: bool) {
        use std::os::unix::fs::PermissionsExt;
        let Some(Sheet::Info(info)) = self.sheet.as_mut() else {
            return;
        };
        let Some(mode) = info.mode else {
            return;
        };
        let next = match (lock, mode & 0o222 != 0, mode & 0o111 != 0) {
            (true, true, _) => {
                info.writers = Some(mode & 0o222);
                mode & !0o222
            }
            (true, false, _) => mode | info.writers.take().unwrap_or(0o200),
            (false, _, true) => mode & !0o111,
            (false, _, false) => mode | ((mode & 0o444) >> 2),
        };
        match std::fs::set_permissions(&info.entry.path, std::fs::Permissions::from_mode(next & 0o7777)) {
            Ok(()) => info.mode = Some(next),
            Err(error) => {
                let words = crate::lang::tf("Cannot change “{}”: {}", &[&info.entry.name, &error]);
                self.say(words, true);
            }
        }
    }

    pub fn choose_app(&mut self, all: Vec<Handler>) {
        let files: Vec<Entry> = self.chosen().into_iter().filter(|each| !each.folder).cloned().collect();
        let Some(one) = files.first().cloned() else {
            return;
        };
        let mut apps = self.handlers_for(one.mime);
        let suggested = apps.len();
        let mut rest: Vec<Handler> = all.into_iter().filter(|app| !apps.iter().any(|each| each.id == app.id)).collect();
        rest.sort_by(|a, b| entry::natural(&a.name, &b.name));
        apps.extend(rest);
        let name = match files.as_slice() {
            [only] => only.name.clone(),
            many => entry::items(many.len()),
        };
        let files = files.into_iter().map(|each| each.path).collect();
        let at = (!apps.is_empty()).then_some(0);
        self.sheet = Some(Sheet::Apps(Chooser { mime: one.mime, kind: one.kind_words(), name, files, apps, suggested, at, always: false, scroll: 0.0 }));
    }

    fn chose_app(&mut self) {
        let Some(Sheet::Apps(chooser)) = self.sheet.take() else {
            return;
        };
        let Some(app) = chooser.at.and_then(|at| chooser.apps.get(at)).cloned() else {
            return;
        };
        if chooser.always {
            let text = std::fs::read_to_string(&self.mime_list).unwrap_or_default();
            let written = self.mime_list.parent().map_or(Ok(()), std::fs::create_dir_all).and_then(|_| std::fs::write(&self.mime_list, crate::desktop::set_defaults(&text, &[chooser.mime], &app.id)));
            match written {
                Ok(()) => {
                    self.handlers.remove(chooser.mime);
                    self.say(crate::lang::tf("{} files now open with {}", &[&chooser.kind, &app.name]), false);
                }
                Err(error) => self.say(crate::lang::tf("Cannot change the default app: {}", &[&error]), true),
            }
        }
        let refs: Vec<&Path> = chooser.files.iter().map(PathBuf::as_path).collect();
        if let Some(argv) = self.app_argv(&app, &refs) {
            self.effects.push(Effect::Launch(argv));
        }
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
                items.push(Item::new(crate::lang::t("Open in New Tab"), "", Act::Tab(path.clone())));
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
        items.push(Item::new(crate::lang::t("Compress"), "", Act::Compress));
        let archives = self.chosen().iter().all(|each| !each.folder && super::pack::kind_of(&each.name).is_some());
        if archives {
            items.push(Item::new(crate::lang::t("Extract Here"), "", Act::Extract));
        }
        items.push(Item::new(crate::lang::t("Move to Trash"), "Delete", Act::Trash));
        items.push(Item::rule());
        items.push(Item::new(crate::lang::t("Get Info"), "Ctrl+I", Act::Info).when(one));
        items
    }

    pub fn line_menu(&mut self, path: &Path, mark: bool) -> Vec<Item> {
        let mut items = vec![
            Item::new(crate::lang::t("Open"), "", Act::Go(path.to_path_buf())),
            Item::new(crate::lang::t("Open in New Tab"), "", Act::Tab(path.to_path_buf())),
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
                self.send(paths, into, !copy);
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
            Item::more(crate::lang::t("View"), Mode::ALL.iter().map(|mode| Item::new(mode.name(), "", Act::Show(*mode)).checked(self.view.mode == *mode)).collect()),
            Item::more(crate::lang::t("Sort By"), Sort::ALL.iter().map(|sort| Item::new(sort.name(), "", Act::SortBy(*sort)).checked(self.view.sort == *sort)).collect()),
            Item::new(crate::lang::t("Show Hidden Files"), "Ctrl+H", Act::Hidden).checked(p.hidden),
            Item::rule(),
            Item::new(crate::lang::t("Open in Terminal"), "", Act::Terminal(None)),
            Item::new(crate::lang::t("Copy Path"), "Shift+Ctrl+C", Act::CopyPath),
            Item::new(crate::lang::t("Get Info"), "Ctrl+I", Act::Info),
        ]
    }

    pub fn summary(&self) -> String {
        if let Some(search) = self.search.as_ref().filter(|_| self.searching()) {
            let count = self.entries.len();
            return match (search.done, search.capped) {
                (false, _) => crate::lang::tf("Searching… {} found", &[&count]),
                (true, true) => crate::lang::tf("The first {} found", &[&count]),
                (true, false) => crate::lang::tf("{} found", &[&count]),
            };
        }
        if self.entries.is_empty() && self.loading.is_some() {
            return crate::lang::t("Loading…").to_string();
        }
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
    Info { entry, inside, mode, target, total: None, opens: None, writers: None, naming: None }
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
    fn a_slow_folder_shows_loading_and_fills_in_later() {
        let (place, mut model) = place("slow");
        let work = place.0.join("work");
        model.budget = Duration::ZERO;
        model.go(work.join("Photos"), None);
        assert_eq!(model.dir, work.join("Photos"));
        assert!(model.entries.is_empty() && model.loading.is_some());
        assert_eq!(model.summary(), crate::lang::t("Loading…"));
        model.settle_all(Duration::from_secs(5));
        assert!(model.loading.is_none());
        assert_eq!(model.entries.iter().map(|each| each.name.as_str()).collect::<Vec<_>>(), ["2026"]);
        model.act(Act::Up);
        model.settle_all(Duration::from_secs(5));
        assert_eq!(model.dir, work);
        assert!(model.picked.contains(&work.join("Photos")));
        assert_eq!(model.back, [work.clone(), work.join("Photos")]);
    }

    #[test]
    fn a_folder_that_cannot_open_says_so() {
        let (place, mut model) = place("gone");
        let work = place.0.join("work");
        model.go(work.join("nope"), None);
        assert_eq!(model.dir, work);
        assert!(model.alerts.last().is_some_and(|words| words.contains("nope")));
        model.budget = Duration::ZERO;
        model.go(work.join("nope"), None);
        model.settle_all(Duration::from_secs(5));
        assert_eq!(model.dir, work.join("nope"));
        assert!(model.trouble.is_some() && model.loading.is_none());
    }

    #[test]
    fn a_change_on_disk_comes_in_on_the_next_tick() {
        let (place, mut model) = place("tick");
        let work = place.0.join("work");
        std::thread::sleep(Duration::from_millis(20));
        std::fs::write(work.join("new.txt"), "x").unwrap();
        model.tick();
        model.settle_all(Duration::from_secs(5));
        model.settle_all(Duration::from_secs(5));
        assert!(model.index_of(&work.join("new.txt")).is_some());
    }

    #[test]
    fn text_peeks_in_from_a_worker() {
        let (place, mut model) = place("peek");
        let work = place.0.join("work");
        model.pick_only(work.join("b.txt"));
        assert_eq!(model.peek, Some(vec!["bee".to_string(), "line two".to_string()]));
        model.budget = Duration::ZERO;
        model.pick_only(work.join("a.png"));
        assert_eq!(model.peek, None);
        model.pick_only(work.join("b.txt"));
        model.settle_all(Duration::from_secs(5));
        assert_eq!(model.peek.as_ref().map(Vec::len), Some(2));
    }

    #[test]
    fn errors_stay_until_they_are_closed() {
        let (_place, mut model) = place("alerts");
        model.say("Path copied", false);
        model.say("first went wrong", true);
        model.say("second went wrong", true);
        model.say("first went wrong", true);
        assert_eq!(model.alerts, ["second went wrong", "first went wrong"]);
        model.note = model.note.take().map(|(words, at)| (words, at - Duration::from_secs(7)));
        model.tick();
        assert!(model.note.is_none());
        assert_eq!(model.alerts.len(), 2);
        model.act(Act::Dismiss);
        assert_eq!(model.alerts, ["second went wrong"]);
        model.act(Act::Dismiss);
        assert!(model.alerts.is_empty());
    }

    #[test]
    fn a_job_can_be_stopped_and_says_so() {
        let (place, mut model) = place("stopjob");
        model.start_job(Job::Delete(vec![place.0.join("work/b.txt")]));
        let id = model.jobs[0].id;
        model.act(Act::Stop(id));
        let progress = model.progress_of(id);
        assert!(progress.stopped());
        let mut done = ops::run_with(&place.0.join("Trash"), Job::Delete(vec![place.0.join("work/b.txt")]), &progress);
        done.id = id;
        model.finished(done);
        assert!(model.jobs.is_empty());
        assert!(place.0.join("work/b.txt").exists());
        assert!(model.note.as_ref().is_some_and(|(words, _)| words.starts_with(crate::lang::t("Stopped"))));
        model.start_job(Job::EmptyTrash);
        model.act(Act::Close);
        assert!(matches!(model.sheet, Some(Sheet::Confirm { act: Act::StopAndClose, .. })));
        model.effects.clear();
        model.act(Act::StopAndClose);
        assert!(model.jobs[0].progress.stopped() && model.effects.is_empty());
        let id = model.jobs[0].id;
        model.finished(Done { id, stopped: true, undo: None, made: Vec::new(), failed: Vec::new(), words: String::new(), go: None });
        assert_eq!(model.effects, [Effect::Close]);
        assert_eq!(time_left(42.2), "about 43 s left");
        assert_eq!(time_left(61.0), "about 2 min left");
        assert_eq!(time_left(7300.0), "about 2 h left");
    }

    #[test]
    fn a_paste_over_a_name_asks_first() {
        let (place, mut model) = place("clashsheet");
        let work = place.0.join("work");
        std::fs::write(work.join("Photos/b.txt"), "old").unwrap();
        model.pick_only(work.join("b.txt"));
        model.act(Act::Cut);
        model.go(work.join("Photos"), None);
        model.effects.clear();
        model.act(Act::Paste);
        assert!(model.effects.is_empty());
        assert!(matches!(&model.sheet, Some(Sheet::Clash { names, cut: true, .. }) if names == &["b.txt"]));
        model.act(Act::Clash(Clash::Skip));
        assert!(model.sheet.is_none());
        assert_eq!(model.effects, [Effect::Job(1, Job::Move { from: vec![work.join("b.txt")], into: work.join("Photos"), clash: Clash::Skip })]);
        model.effects.clear();
        model.drop_on(super::super::look::Drop::Into(work.join("Photos")), vec![work.join("a.png")], true);
        assert_eq!(model.effects, [Effect::Job(2, Job::Copy { from: vec![work.join("a.png")], into: work.join("Photos"), clash: Clash::KeepBoth })]);
    }

    #[test]
    fn compress_and_extract_run_as_jobs() {
        let (place, mut model) = place("pack");
        let work = place.0.join("work");
        model.pick_only(work.join("b.txt"));
        model.act(Act::Extract);
        assert!(model.effects.is_empty());
        assert!(!model.item_menu().iter().any(|item| item.act == Some(Act::Extract)));
        model.act(Act::Compress);
        let Some(Effect::Job(id, job)) = model.effects.pop() else {
            panic!("no job");
        };
        assert_eq!(job, Job::Compress(vec![work.join("b.txt")]));
        if !crate::explorer::prefs::on_path("bsdtar") && !crate::explorer::prefs::on_path("zip") {
            return;
        }
        let mut done = ops::run_with(&place.0.join("Trash"), job, &model.progress_of(id));
        done.id = id;
        model.finished(done);
        assert!(model.picked.contains(&work.join("b.txt.zip")));
        assert!(model.item_menu().iter().any(|item| item.act == Some(Act::Extract)));
    }

    #[test]
    fn a_change_event_refreshes_the_tree_and_the_list() {
        let (place, mut model) = place("changed");
        let work = place.0.join("work");
        model.fold(&work.join("Photos"));
        model.lines();
        model.settle_all(Duration::from_secs(5));
        assert_eq!(model.kids.get(&work.join("Photos")), Some(&vec![work.join("Photos/2026")]));
        std::fs::create_dir(work.join("Photos/2027")).unwrap();
        std::fs::write(work.join("c.txt"), "c").unwrap();
        model.absorb(Got::Changed(vec![work.join("Photos"), work.clone()]));
        model.settle_all(Duration::from_secs(5));
        assert_eq!(model.kids.get(&work.join("Photos")).map(Vec::len), Some(2));
        assert!(model.index_of(&work.join("c.txt")).is_some());
    }

    #[test]
    fn a_confirm_acts_on_what_it_named() {
        let (place, mut model) = place("confirm");
        let work = place.0.join("work");
        model.pick_only(work.join("b.txt"));
        model.act(Act::DeleteForever);
        model.pick_only(work.join("a.png"));
        let Some(Sheet::Confirm { act, .. }) = model.sheet.clone() else {
            panic!("no sheet");
        };
        model.act(act);
        assert_eq!(model.effects, [Effect::Job(1, Job::Delete(vec![work.join("b.txt")]))]);
    }

    #[test]
    fn path_crumbs_lead_to_each_folder_on_the_way() {
        let (place, mut model) = place("crumbs");
        let work = place.0.join("work");
        model.go(work.join("Photos/2026"), None);
        let Ok(body) = crate::Font::find(None, 13.0) else {
            return;
        };
        let fonts = super::super::look::Fonts { body: &body, small: &body, title: &body };
        let look = super::super::look::Look::themed(&crate::theme::DARK, [64, 128, 238]);
        let bar = crate::Rect::new(crate::Vec2::new(100.0, 40.0), crate::Vec2::new(3000.0, 30.0));
        let crumbs = look.crumbs(bar, &model, &fonts);
        let last = crumbs.last().unwrap();
        assert!(last.last && last.words == "2026" && last.path == work.join("Photos/2026"));
        assert_eq!(crumbs[crumbs.len() - 2].path, work.join("Photos"));
        assert!(crumbs.windows(2).all(|pair| pair[0].area.max().x <= pair[1].area.origin.x + 1.0));
        let menu = model.folder_menu(&work);
        assert_eq!(menu.iter().map(|item| item.label.as_str()).collect::<Vec<_>>(), ["Photos"]);
        assert_eq!(menu[0].act, Some(Act::Go(work.join("Photos"))));
        let empty = model.folder_menu(&work.join("Photos/2026"));
        assert!(empty.len() == 1 && !empty[0].on);
        let narrow = crate::Rect::new(crate::Vec2::new(100.0, 40.0), crate::Vec2::new(160.0, 30.0));
        let tight = look.crumbs(narrow, &model, &fonts);
        assert_eq!(tight.first().map(|crumb| crumb.words.as_str()), Some("…"));
        assert!(tight.iter().all(|crumb| crumb.area.max().x <= narrow.max().x + 1.0));
    }

    #[test]
    fn search_finds_names_below_the_folder_and_ends_cleanly() {
        let (place, mut model) = place("search");
        let work = place.0.join("work");
        std::fs::write(work.join("Photos/2026/beach.png"), "x").unwrap();
        std::fs::write(work.join("Photos/Beach notes.txt"), "x").unwrap();
        model.view.mode = Mode::Columns;
        model.act(Act::Search);
        assert_eq!(model.view.mode, Mode::List);
        let search = model.search.as_mut().unwrap();
        search.field.put("beach");
        model.search_changed();
        let until = Instant::now() + Duration::from_secs(5);
        while !model.search.as_ref().unwrap().done && Instant::now() < until {
            model.settle(Instant::now() + Duration::from_millis(50), |_| false);
        }
        let mut names: Vec<&str> = model.entries.iter().map(|each| each.name.as_str()).collect();
        names.sort();
        assert_eq!(names, ["Beach notes.txt", "beach.png"]);
        assert_eq!(model.summary(), crate::lang::tf("{} found", &[&2]));
        model.reload();
        assert_eq!(model.entries.len(), 2);
        model.act(Act::EndSearch);
        assert!(model.search.is_none());
        assert_eq!(model.view.mode, Mode::Columns);
        assert!(model.index_of(&work.join("b.txt")).is_some());
        model.act(Act::Search);
        model.search.as_mut().unwrap().field.put("zzz");
        model.search_changed();
        model.go(work.join("Photos"), None);
        assert!(model.search.is_none());
    }

    #[test]
    fn tabs_keep_their_own_folder_history_and_picks() {
        let (place, mut model) = place("tabs");
        let work = place.0.join("work");
        let photos = work.join("Photos");
        model.pick_only(work.join("b.txt"));
        model.act(Act::NewTab);
        assert_eq!((model.tabs.len(), model.tab, model.dir.clone()), (2, 1, work.clone()));
        assert!(model.picked.is_empty());
        model.go(photos.clone(), None);
        model.act(Act::ShowTab(0));
        assert_eq!(model.dir, work);
        assert!(model.picked.contains(&work.join("b.txt")));
        assert!(model.back.is_empty());
        model.act(Act::NextTab(1));
        assert_eq!(model.dir, photos);
        assert_eq!(model.back, vec![work.clone()]);
        model.new_tab(photos.join("2026"), false);
        assert_eq!(model.tab_dirs(), vec![work.clone(), photos.clone(), photos.join("2026")]);
        assert_eq!(model.dir, photos);
        model.act(Act::CloseTab(1));
        assert_eq!((model.tabs.len(), model.tab, model.dir.clone()), (2, 1, photos.join("2026")));
        std::fs::create_dir(work.join("Gone")).unwrap();
        model.new_tab(work.join("Gone"), false);
        std::fs::remove_dir(work.join("Gone")).unwrap();
        model.act(Act::ShowTab(2));
        assert_eq!(model.dir, work);
        model.act(Act::CloseOtherTabs(0));
        assert_eq!((model.tabs.len(), model.tab, model.dir.clone()), (1, 0, work.clone()));
        assert!(model.effects.is_empty());
        model.act(Act::CloseTab(0));
        assert_eq!(model.effects, vec![Effect::Close]);
    }

    #[test]
    fn each_folder_keeps_its_own_view() {
        let (place, mut model) = place("views");
        let work = place.0.join("work");
        let photos = work.join("Photos");
        assert_eq!(model.view, model.prefs.view());
        model.go(photos.clone(), None);
        model.act(Act::Show(Mode::Icons));
        model.act(Act::SortBy(Sort::Size));
        model.act(Act::Up);
        assert_eq!(model.view.mode, Mode::List);
        assert_eq!(model.view.sort, Sort::Name);
        model.go(photos.clone(), None);
        assert_eq!((model.view.mode, model.view.sort), (Mode::Icons, Sort::Size));
        model.act(Act::Show(Mode::Columns));
        model.go(photos.join("2026"), None);
        assert_eq!(model.view.mode, Mode::Columns);
        model.act(Act::Back);
        model.act(Act::Show(Mode::Icons));
        model.act(Act::Up);
        model.set(Pref::Mode(Mode::Gallery));
        assert_eq!(model.view.mode, Mode::Gallery);
        model.save();
        assert!(!model.unsaved());
        let other = Model::new(place.0.join("explorer.toml"), Some(photos.clone()));
        assert_eq!((other.view.mode, other.view.sort), (Mode::Icons, Sort::Size));
        let mut both = Model::new(place.0.join("explorer.toml"), Some(work.clone()));
        both.act(Act::Show(Mode::List));
        both.save();
        let saved = prefs::load_views(&model.views_path);
        assert!(saved.iter().any(|(path, view)| path == &photos && view.mode == Mode::Icons));
        assert!(saved.iter().any(|(path, view)| path == &work && view.mode == Mode::List));
        model.go(photos.clone(), None);
        model.act(Act::NewTab);
        model.go(work.clone(), None);
        assert_eq!(model.view.mode, Mode::Gallery);
        model.act(Act::ShowTab(0));
        assert_eq!(model.view.mode, Mode::Icons);
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
        model.view.mode = Mode::Columns;
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
        assert_eq!(model.effects, [Effect::Job(1, Job::Copy { from: vec![place.0.join("work/b.txt")], into: place.0.join("work/Photos"), clash: Clash::KeepBoth })]);
        let job = match model.effects.pop() {
            Some(Effect::Job(_, job)) => job,
            _ => unreachable!(),
        };
        let mut done = ops::run_with(&place.0.join("Trash"), job, &Progress::default());
        done.id = 1;
        model.finished(done);
        assert!(model.jobs.is_empty());
        assert_eq!(model.single().map(|one| one.name.as_str()), Some("b.txt"));
        assert_eq!(model.undo.len(), 1);
    }

    #[test]
    fn get_info_can_rename() {
        let (place, mut model) = place("info-name");
        let b = place.0.join("work/b.txt");
        model.pick_only(b.clone());
        model.act(Act::Info);
        model.act(Act::NameInInfo);
        let Some(Sheet::Info(info)) = model.sheet.as_mut() else {
            panic!("Info opens");
        };
        let field = info.naming.as_mut().expect("the name is a field");
        assert_eq!(field.picked_text(), Some("b"));
        field.put("bee");
        model.act(Act::NamedInInfo);
        let bee = place.0.join("work/bee.txt");
        assert!(bee.exists() && !b.exists());
        let Some(Sheet::Info(info)) = &model.sheet else {
            panic!("Info stays open");
        };
        assert_eq!((info.entry.name.as_str(), info.naming.is_none()), ("bee.txt", true));
        assert_eq!(model.undo.last(), Some(&Undo::Renamed(b.clone(), bee.clone())));
        model.act(Act::NameInInfo);
        if let Some(Sheet::Info(info)) = model.sheet.as_mut() {
            let field = info.naming.as_mut().unwrap();
            field.pick_all();
            field.put("a.png");
        }
        model.act(Act::NamedInInfo);
        let Some(Sheet::Info(info)) = &model.sheet else {
            panic!("Info stays open");
        };
        assert!(info.naming.is_some() && bee.exists(), "a clash keeps the field open");
    }

    #[test]
    fn tabs_come_back_next_time() {
        let (place, mut model) = place("tabs-again");
        model.set(Pref::Start(Start::Last));
        let photos = place.0.join("work/Photos");
        model.new_tab(photos.clone(), true);
        model.new_tab(place.0.join("work/Photos/2026"), false);
        model.close_tab(2);
        model.save();
        let again = Model::new(place.0.join("explorer.toml"), None);
        assert_eq!(again.tab_dirs(), [place.0.join("work"), photos.clone()]);
        assert_eq!((again.tab, again.dir.clone()), (1, photos.clone()));
        let asked = Model::new(place.0.join("explorer.toml"), Some(place.0.join("work")));
        assert_eq!(asked.tab_dirs(), [place.0.join("work")], "a folder asked for opens alone");
        std::fs::remove_dir_all(&photos).unwrap();
        let gone = Model::new(place.0.join("explorer.toml"), None);
        assert_eq!(gone.tabs.len(), 1, "tabs whose folder is gone are left out");
    }

    #[test]
    fn get_info_counts_folders_and_changes_access() {
        use std::os::unix::fs::PermissionsExt;
        let (place, mut model) = place("info");
        let photos = place.0.join("work/Photos");
        model.pick_only(photos.clone());
        model.effects.clear();
        model.act(Act::Info);
        assert_eq!(model.effects, [Effect::Measure(photos.clone())]);
        assert_eq!(ops::folder_size(&photos), (0, 1));
        assert_eq!(ops::folder_size(&place.0.join("work")), (10 + 12 + 1, 5));
        model.measured(&place.0.join("elsewhere"), (9, 9));
        model.measured(&photos, (0, 1));
        let Some(Sheet::Info(info)) = &model.sheet else {
            panic!("Info opens");
        };
        assert_eq!((info.total, info.runs()), (Some((0, 1)), None));

        let b = place.0.join("work/b.txt");
        std::fs::set_permissions(&b, std::fs::Permissions::from_mode(0o644)).unwrap();
        model.pick_only(b.clone());
        let mime = model.single().unwrap().mime;
        model.handlers.insert(mime, vec![Handler { id: "edit".into(), name: "Edit".into(), exec: "edit %F".into(), icon: None, terminal: false }]);
        model.act(Act::Info);
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        let Some(Sheet::Info(info)) = &model.sheet else {
            panic!("Info opens");
        };
        assert_eq!((info.opens.as_deref(), info.locked(), info.runs()), (Some("Edit"), Some(false), Some(false)));
        model.act(Act::Lock);
        assert_eq!(mode(&b), 0o444);
        model.act(Act::Runs);
        assert_eq!(mode(&b), 0o555);
        model.act(Act::Lock);
        model.act(Act::Runs);
        assert_eq!(mode(&b), 0o644);
        std::fs::set_permissions(&b, std::fs::Permissions::from_mode(0o664)).unwrap();
        model.act(Act::Info);
        model.act(Act::Lock);
        model.act(Act::Lock);
        assert_eq!(mode(&b), 0o664, "unlocking gives back what locking took");
        model.act(Act::ChangeApp);
        let Some(Sheet::Apps(chooser)) = &model.sheet else {
            panic!("Change opens the chooser");
        };
        assert!(chooser.always && chooser.apps[0].id == "edit");
    }

    #[test]
    fn another_app_can_open_a_file_and_become_the_default() {
        let (place, mut model) = place("apps");
        model.mime_list = place.0.join("config/mimeapps.list");
        let b = place.0.join("work/b.txt");
        model.pick_only(b.clone());
        let mime = model.single().unwrap().mime;
        let app = |id: &str, name: &str| Handler { id: id.to_string(), name: name.to_string(), exec: format!("{id} %F"), icon: None, terminal: false };
        model.handlers.insert(mime, vec![app("edit", "Edit")]);
        let menu = model.open_with_items();
        assert_eq!(menu.last().map(|item| item.act.clone()), Some(Some(Act::OtherApp)));
        model.choose_app(vec![app("zed", "Zed"), app("edit", "Edit"), app("art", "Art")]);
        let Some(Sheet::Apps(chooser)) = &model.sheet else {
            panic!("the chooser opens");
        };
        assert_eq!(chooser.apps.iter().map(|each| each.name.as_str()).collect::<Vec<_>>(), ["Edit", "Art", "Zed"]);
        assert_eq!((chooser.at, chooser.suggested, chooser.name.as_str()), (Some(0), 1, "b.txt"));
        model.act(Act::PickApp(2));
        model.act(Act::AlwaysApp);
        model.effects.clear();
        model.act(Act::ChooseApp);
        assert_eq!(model.sheet, None);
        assert_eq!(model.effects, [Effect::Launch(vec!["zed".to_string(), b.to_string_lossy().into_owned()])]);
        let list = std::fs::read_to_string(&model.mime_list).unwrap();
        assert!(list.contains(&format!("{mime}=zed.desktop;")), "{list}");
        assert!(!model.handlers.contains_key(mime), "the old list is forgotten");

        model.prefs.terminal = Some("foot".to_string());
        let vim = Handler { terminal: true, ..app("vim", "Vim") };
        assert_eq!(model.app_argv(&vim, &[b.as_path()]), Some(vec!["foot".to_string(), "vim".to_string(), b.to_string_lossy().into_owned()]));
        model.prefs.terminal = Some("alacritty".to_string());
        assert_eq!(model.app_argv(&vim, &[b.as_path()]).unwrap()[..3], ["alacritty", "-e", "vim"]);
    }

    #[test]
    fn the_clipboard_is_shared_with_other_apps() {
        use super::super::look::{Fonts, Look};
        use super::super::input::{self, Key, Named, Ui};
        let (place, mut model) = place("clip");
        let b = place.0.join("work/b.txt");
        model.pick_only(b.clone());
        model.act(Act::Cut);
        assert_eq!(model.effects, [Effect::Hold(Held::Files(vec![b.clone()], true))]);
        assert_eq!(model.clip, Some((vec![b.clone()], true)));
        assert_eq!(model.clip_text.as_deref(), b.to_str());

        let far = place.0.join("far.txt");
        std::fs::write(&far, "far").unwrap();
        model.heard_clip(Some(Held::Files(vec![far.clone()], false)));
        model.effects.clear();
        model.act(Act::Paste);
        assert_eq!(model.effects, [Effect::Job(1, Job::Copy { from: vec![far.clone()], into: place.0.join("work"), clash: Clash::KeepBoth })]);

        model.heard_clip(Some(Held::Text("holiday\nsecond line".to_string())));
        assert_eq!(model.clip, None);
        model.effects.clear();
        model.act(Act::Paste);
        assert!(model.effects.is_empty(), "text is not pasted as files");

        let (Ok(body), Ok(small), Ok(title)) = (crate::Font::find(None, 13.0), crate::Font::find(None, 11.5), crate::Font::find(None, 17.0)) else {
            return;
        };
        let fonts = Fonts { body: &body, small: &small, title: &title };
        let look = Look::themed(&crate::theme::DARK, [64, 128, 238]);
        let ui = Ui { look: &look, fonts: &fonts, size: crate::Vec2::new(900.0, 600.0) };
        model.pick_only(b.clone());
        model.act(Act::Rename);
        input::key(&mut model, &ui, &Key { named: Named::Letter('v'), text: Some("v".to_string()), ctrl: true, shift: false, alt: false });
        assert_eq!(model.renaming.as_ref().map(|(_, field)| field.text.as_str()), Some("holiday.txt"));
        model.heard_clip(None);
        assert_eq!((model.clip.clone(), model.clip_text.clone()), (None, None));
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
        assert_eq!(model.effects, [Effect::Job(1, Job::Move { from: vec![place.0.join("work/b.txt")], into: photos.clone(), clash: Clash::KeepBoth })]);
        model.effects.clear();
        model.drop_on(Drop::Into(photos.join("2026")), vec![photos.clone()], false);
        assert!(model.effects.is_empty());
        model.act(Act::Unmark(photos.clone()));
        assert!(!model.marked(&photos));
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
