use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::entry::{Entry, Family, Sort};
use super::field::Field;
use super::model::{Item, Line, Menu, Model, Sheet};
use super::prefs::{self, Mode};
use crate::geom::{Rect, Vec2};
use crate::glance::fit;
use crate::image::Image;
use crate::paint::{Canvas, logical};
use crate::text::Font;

pub const MENU_H: f64 = 30.0;
pub const BAR_H: f64 = 50.0;
pub const STATUS_H: f64 = 28.0;
pub const BANNER_H: f64 = 40.0;
pub const TABS_H: f64 = 36.0;
pub const JOB_H: f64 = 50.0;
pub const SPLIT_W: f64 = 6.0;
pub const ROW_H: f64 = 26.0;
pub const HEAD_H: f64 = 28.0;
pub const LINE_H: f64 = 26.0;
pub const COLUMN_W: f64 = 250.0;
pub const PREVIEW_W: f64 = 320.0;
pub const STRIP_H: f64 = 136.0;
pub const STRIP_CELL: f64 = 96.0;
pub const ITEM_H: f64 = 28.0;
pub const RULE_H: f64 = 9.0;
pub const MENU_PAD: f64 = 5.0;
pub const SHEET_ROW: f64 = 48.0;
const BUTTON: f64 = 30.0;
const VIEW_W: f64 = 34.0;
const ZOOM_W: f64 = 120.0;

pub struct Fonts<'a> {
    pub body: &'a Font,
    pub small: &'a Font,
    pub title: &'a Font,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Hit {
    Bar(usize),
    Back,
    Forward,
    Up,
    Path,
    Show(Mode),
    Gear,
    Split,
    Line(usize),
    Caret(usize),
    Head(Sort),
    Entry(usize),
    Space,
    Column(usize, Option<usize>),
    Strip(usize),
    Zoom(f64),
    Picture,
    Dismiss,
    Banner,
    Stop(u64),
    Jobs,
    Eject(usize),
    Crumb(usize),
    CrumbArrow(usize),
    Search,
    SearchClear,
    Tab(usize),
    TabClose(usize),
    TabNew,
    Tabs,
    Rail(Rail),
    Edge(Sort),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    Path,
    Search,
    Rename,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rail {
    Body,
    Tree,
    Column(usize),
}

pub fn rail_track(area: Rect) -> Rect {
    rect(area.max().x - 12.0, area.origin.y, 12.0, area.size.y)
}

pub fn rail_thumb(area: Rect, content: f64, scroll: f64) -> Rect {
    let tall = (area.size.y * area.size.y / content).max(28.0).min(area.size.y);
    let top = area.origin.y + (area.size.y - tall) * (scroll / (content - area.size.y)).clamp(0.0, 1.0);
    rect(area.max().x - 12.0, top, 12.0, tall)
}

pub fn ringed(model: &Model, pane: super::model::Pane) -> bool {
    model.ring && model.pane == pane && (pane == super::model::Pane::Files || model.prefs.tree || model.prefs.places || model.prefs.drives)
}

fn inset(area: Rect, by: f64) -> Rect {
    rect(area.origin.x + by, area.origin.y + by, area.size.x - by * 2.0, area.size.y - by * 2.0)
}

pub fn rail_hot(model: &Model, rail: Rail) -> bool {
    model.hover == Some(Hit::Rail(rail)) || matches!(model.dragging, Some(super::model::Drag::Rail { rail: held, .. }) if held == rail)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Crumb {
    pub area: Rect,
    pub arrow: Rect,
    pub path: PathBuf,
    pub words: String,
    pub last: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SheetHit {
    Tab(usize),
    Pref(super::model::Pref),
    Slider(f64),
    Done,
    No,
    Yes,
    Inside,
    Outside,
    Clash(super::ops::Clash),
    App(usize),
    Always,
    Lock,
    Runs,
    ChangeApp,
    Name,
    NameRule(super::model::NameRule),
    NameEnd(bool),
    NameField(usize),
}

pub const APP_ROW: f64 = 34.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Empty {
    Folder,
    Trash,
    Search,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drop {
    Mark,
    Into(PathBuf),
    Trash,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Want {
    Icon(String, u32),
    Thumb(PathBuf, u32),
}

#[derive(Default)]
pub struct Art {
    pub icons: HashMap<(String, u32), Image>,
    pub thumbs: HashMap<(PathBuf, u32), Image>,
    pub missing: HashSet<Want>,
    pub wants: RefCell<Vec<Want>>,
}

impl Art {
    pub fn icon(&self, name: &str, side: f64) -> Option<&Image> {
        let px = crate::paint::dense(side);
        let key = (name.to_string(), px);
        if let Some(found) = self.icons.get(&key) {
            return Some(found);
        }
        let want = Want::Icon(key.0, px);
        if !self.missing.contains(&want) {
            self.wants.borrow_mut().push(want);
        }
        None
    }

    pub fn thumb(&self, path: &Path, side: f64) -> Option<&Image> {
        let px = crate::paint::dense(side);
        let key = (path.to_path_buf(), px);
        if let Some(found) = self.thumbs.get(&key) {
            return Some(found);
        }
        let want = Want::Thumb(key.0, px);
        if !self.missing.contains(&want) {
            self.wants.borrow_mut().push(want);
        }
        self.thumbs.iter().find(|((each, _), _)| each == path).map(|(_, image)| image)
    }

    pub fn forget(&mut self, path: &Path) {
        self.thumbs.retain(|(each, _), _| each != path);
        self.missing.retain(|want| !matches!(want, Want::Thumb(each, _) if each == path));
    }

    pub fn take_wants(&self) -> Vec<Want> {
        let mut wants = std::mem::take(&mut *self.wants.borrow_mut());
        let mut seen = HashSet::new();
        wants.retain(|want| seen.insert(want.clone()));
        wants
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub menu: Rect,
    pub bar: Rect,
    pub tabs: Option<Rect>,
    pub banner: Option<Rect>,
    pub jobs: Option<Rect>,
    pub side: Option<Rect>,
    pub split: Option<Rect>,
    pub body: Rect,
    pub status: Option<Rect>,
    pub clip: Option<Rect>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub back: [u8; 3],
    pub side: [u8; 3],
    pub bar: [u8; 3],
    pub card: [u8; 3],
    pub well: [u8; 3],
    pub raised: [u8; 3],
    pub hover: [u8; 3],
    pub rule: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub alert: [u8; 3],
    pub accent: [u8; 3],
    pub on_accent: [u8; 3],
    pub shade: [u8; 3],
    pub drop: f64,
    pub dark: bool,
}

pub struct Paint<'a> {
    pub model: &'a Model,
    pub lines: &'a [Line],
    pub fonts: &'a Fonts<'a>,
    pub art: &'a Art,
    pub clip: Option<Rect>,
}

pub fn touches(clip: Option<Rect>, area: Rect) -> bool {
    clip.is_none_or(|clip| clip.origin.x < area.max().x && area.origin.x < clip.max().x && clip.origin.y < area.max().y && area.origin.y < clip.max().y)
}

pub fn tree_name(model: &Model, path: &Path) -> String {
    match path == model.tree_base() {
        true => match model.prefs.tree_root {
            prefs::Root::Home => crate::lang::t("Home").to_string(),
            prefs::Root::Computer => crate::lang::t("Computer").to_string(),
        },
        false => super::entry::name_of(path),
    }
}

fn info_row_count(info: &super::model::Info) -> usize {
    4 + usize::from(info.mode.is_some()) + usize::from(info.target.is_some()) + usize::from(info.opens.is_some())
}

fn info_switch_count(info: &super::model::Info) -> usize {
    usize::from(info.locked().is_some()) + usize::from(info.runs().is_some())
}

pub fn key_words(keys: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        let shown = match word.as_str() {
            "Ctrl" | "Shift" | "Space" | "Enter" | "Backspace" => crate::lang::t(word),
            "Delete" => crate::lang::t("Del"),
            "Up" => "↑",
            "Down" => "↓",
            "Left" => "←",
            "Right" => "→",
            _ => word.as_str(),
        };
        out.push_str(shown);
        word.clear();
    };
    for c in keys.chars() {
        match c.is_alphanumeric() && !(c.is_ascii_digit() && word.is_empty()) {
            true => word.push(c),
            false => {
                flush(&mut word, &mut out);
                out.push(c);
            }
        }
    }
    flush(&mut word, &mut out);
    out
}

pub fn ease(t: f64) -> f64 {
    1.0 - (1.0 - t.clamp(0.0, 1.0)).powi(3)
}

fn layered(canvas: &mut Canvas, size: Vec2, pop: f64, rise: f64, paint: impl FnOnce(&mut Canvas)) {
    if pop >= 1.0 {
        return paint(canvas);
    }
    let mut layer = Canvas::new(size.x.ceil() as u32, size.y.ceil() as u32);
    paint(&mut layer);
    canvas.stamp(&layer.image, Vec2::new(0.0, (rise * (1.0 - pop)).round()), pop.max(0.0));
}

fn lift(color: [u8; 3], amount: f64) -> [u8; 3] {
    match amount >= 0.0 {
        true => mix(color, [255, 255, 255], amount),
        false => mix(color, [0, 0, 0], -amount),
    }
}

pub fn join(a: Rect, b: Rect) -> Rect {
    let low = a.origin.min(b.origin);
    let high = a.max().max(b.max());
    Rect::new(low, high - low)
}

fn moved(clip: Option<Rect>, by: Vec2) -> Option<Rect> {
    clip.map(|clip| Rect::new(clip.origin - by, clip.size))
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(Vec2::new(x, y), Vec2::new(w.max(0.0), h.max(0.0)))
}

fn mix(a: [u8; 3], b: [u8; 3], t: f64) -> [u8; 3] {
    let at = |i: usize| (f64::from(a[i]) + (f64::from(b[i]) - f64::from(a[i])) * t).round().clamp(0.0, 255.0) as u8;
    [at(0), at(1), at(2)]
}

impl Look {
    pub fn themed(p: &crate::theme::Palette, accent: [u8; 3]) -> Look {
        let dark = p.ink[0] > 128;
        Look {
            back: p.card,
            side: p.surface,
            bar: p.bar,
            card: p.card,
            well: p.well,
            raised: p.raised,
            hover: p.hover,
            rule: p.rule,
            ink: p.ink,
            dim: p.dim,
            alert: p.alert,
            accent,
            on_accent: [255, 255, 255],
            shade: p.shade,
            drop: f64::from(p.drop) / 255.0,
            dark,
        }
    }

    pub fn least(&self) -> Vec2 {
        Vec2::new(560.0, 360.0)
    }

    pub fn frame(&self, size: Vec2, model: &Model) -> Frame {
        let menu = rect(0.0, 0.0, size.x, MENU_H);
        let bar = rect(0.0, MENU_H, size.x, BAR_H);
        let tabs = (model.tabs.len() > 1).then(|| rect(0.0, MENU_H + BAR_H, size.x, TABS_H));
        let under = MENU_H + BAR_H + tabs.map_or(0.0, |tabs| tabs.size.y);
        let banner = (!model.alerts.is_empty()).then(|| rect(0.0, under, size.x, BANNER_H));
        let below = under + banner.map_or(0.0, |banner| banner.size.y);
        let shown = model.jobs.iter().filter(|job| job.shown()).count();
        let jobs = (shown > 0).then(|| rect(0.0, below, size.x, JOB_H * shown as f64));
        let top = below + jobs.map_or(0.0, |jobs| jobs.size.y);
        let status_h = if model.prefs.status { STATUS_H } else { 0.0 };
        let tall = (size.y - top - status_h).max(0.0);
        let side_on = model.prefs.tree || model.prefs.places || model.prefs.drives;
        let side_w = match side_on {
            true => f64::from(model.prefs.tree_width).min(size.x * 0.5),
            false => 0.0,
        };
        let side = side_on.then(|| rect(0.0, top, side_w, tall));
        let split = side_on.then(|| rect(side_w - SPLIT_W / 2.0, top, SPLIT_W, tall));
        let body = rect(side_w, top, size.x - side_w, tall);
        let status = model.prefs.status.then(|| rect(0.0, size.y - STATUS_H, size.x, STATUS_H));
        Frame { menu, bar, tabs, banner, jobs, side, split, body, status, clip: None }
    }

    pub fn bar_titles(&self, fonts: &Fonts) -> Vec<(Rect, &'static str)> {
        let mut x = 8.0;
        [crate::lang::t("File"), crate::lang::t("Edit"), crate::lang::t("View"), crate::lang::t("Go"), crate::lang::t("Help")]
            .into_iter()
            .map(|name| {
                let w = fonts.body.width_of(name) + 20.0;
                let out = (rect(x, 3.0, w, MENU_H - 6.0), name);
                x += w;
                out
            })
            .collect()
    }

    fn nav_rects(&self, bar: Rect) -> [Rect; 3] {
        let y = bar.origin.y + (BAR_H - BUTTON) / 2.0;
        [rect(10.0, y, BUTTON, BUTTON), rect(10.0 + BUTTON + 4.0, y, BUTTON, BUTTON), rect(10.0 + (BUTTON + 4.0) * 2.0, y, BUTTON, BUTTON)]
    }

    fn gear_rect(&self, bar: Rect) -> Rect {
        rect(bar.size.x - 10.0 - BUTTON, bar.origin.y + (BAR_H - BUTTON) / 2.0, BUTTON, BUTTON)
    }

    fn view_rects(&self, bar: Rect) -> Vec<(Mode, Rect)> {
        let gear = self.gear_rect(bar);
        let left = gear.origin.x - 10.0 - VIEW_W * 4.0;
        Mode::ALL.iter().enumerate().map(|(at, mode)| (*mode, rect(left + VIEW_W * at as f64, gear.origin.y, VIEW_W, BUTTON))).collect()
    }

    pub fn search_rect(&self, bar: Rect) -> Rect {
        let views = self.view_rects(bar)[0].1;
        let wide = (bar.size.x * 0.22).clamp(140.0, 240.0);
        rect(views.origin.x - 12.0 - wide, views.origin.y, wide, BUTTON)
    }

    fn search_clear_rect(&self, search: Rect) -> Rect {
        rect(search.max().x - 26.0, search.origin.y + 3.0, 22.0, search.size.y - 6.0)
    }

    pub fn path_rect(&self, bar: Rect) -> Rect {
        let nav = self.nav_rects(bar);
        let left = nav[2].max().x + 12.0;
        let right = self.search_rect(bar).origin.x - 10.0;
        rect(left, nav[0].origin.y, right - left, BUTTON)
    }

    fn zoom_rect(&self, status: Rect, model: &Model, fonts: &Fonts) -> Option<Rect> {
        if model.view.mode != Mode::Icons {
            return None;
        }
        let free = model.free_words().map_or(0.0, |words| fonts.small.width_of(&words) + 24.0);
        Some(rect(status.size.x - free - ZOOM_W - 30.0, status.origin.y + 6.0, ZOOM_W, STATUS_H - 12.0))
    }

    pub fn row_blank(&self, size: Vec2, model: &Model, fonts: &Fonts, index: usize, at: Vec2) -> bool {
        if model.view.mode != Mode::List || model.renaming.is_some() {
            return false;
        }
        let frame = self.frame(size, model);
        let (Some(row), Some(entry)) = (self.entry_rect(&frame, model, index), model.entries.get(index)) else {
            return false;
        };
        let (name_w, _) = self.list_widths(frame.body, model);
        let name_x = row.origin.x + 34.0;
        let words = fonts.body.width_of(&fit(fonts.body, &entry.name, name_w - 44.0));
        at.x > name_x + words + 12.0 && at.x < row.origin.x + name_w - 8.0
    }

    pub fn list_widths(&self, body: Rect, model: &Model) -> (f64, Vec<(Sort, f64)>) {
        let mut extra = Vec::new();
        if model.searching() {
            extra.push((Sort::Origin, 230.0));
        }
        if model.dir == super::ops::trash_files() && !model.searching() {
            extra.push((Sort::Origin, 230.0));
            extra.push((Sort::Deleted, 170.0));
        } else if model.prefs.show_modified {
            extra.push((Sort::Modified, 170.0));
        }
        if model.prefs.show_size {
            extra.push((Sort::Size, 90.0));
        }
        if model.prefs.show_kind {
            extra.push((Sort::Kind, 140.0));
        }
        for (sort, width) in &mut extra {
            if let Some(chosen) = model.prefs.widths.get(*sort) {
                *width = f64::from(chosen);
            }
        }
        let mut name = body.size.x - 16.0 - extra.iter().map(|(_, w)| w).sum::<f64>();
        while name < 200.0 && !extra.is_empty() {
            let (_, w) = extra.pop().unwrap_or((Sort::Kind, 0.0));
            name += w;
        }
        (name, extra)
    }

    pub fn head_edges(&self, body: Rect, model: &Model) -> Vec<(Sort, f64)> {
        let (name, extra) = self.list_widths(body, model);
        let mut x = body.origin.x + 8.0 + name;
        extra
            .into_iter()
            .map(|(sort, width)| {
                let edge = (sort, x);
                x += width;
                edge
            })
            .collect()
    }

    pub fn grid(&self, body: Rect, model: &Model) -> (usize, Vec2, f64) {
        let zoom = f64::from(model.prefs.zoom);
        let cell = Vec2::new((zoom + 40.0).max(100.0), zoom + 56.0);
        let cols = (((body.size.x - 16.0) / cell.x).floor() as usize).max(1);
        let left = (body.size.x - cols as f64 * cell.x) / 2.0;
        (cols, cell, left)
    }

    pub fn entry_rect(&self, frame: &Frame, model: &Model, at: usize) -> Option<Rect> {
        let body = frame.body;
        match model.view.mode {
            Mode::List => Some(rect(body.origin.x + 8.0, body.origin.y + HEAD_H + at as f64 * ROW_H - model.scroll, body.size.x - 16.0, ROW_H)),
            Mode::Icons => {
                let (cols, cell, left) = self.grid(body, model);
                let (row, col) = (at / cols, at % cols);
                Some(rect(body.origin.x + left + col as f64 * cell.x, body.origin.y + 12.0 + row as f64 * cell.y - model.scroll, cell.x, cell.y))
            }
            Mode::Gallery => {
                let strip = self.strip_rect(body);
                let shift = self.strip_shift(body, model);
                Some(rect(strip.origin.x + at as f64 * STRIP_CELL - shift, strip.origin.y + 8.0, STRIP_CELL, STRIP_H - 16.0))
            }
            Mode::Columns => None,
        }
    }

    pub fn lasso_box(&self, model: &Model) -> Option<Rect> {
        let Some(super::model::Drag::Lasso { from, at, .. }) = &model.dragging else {
            return None;
        };
        let start = Vec2::new(from.x, from.y - model.scroll);
        let low = start.min(*at);
        Some(Rect::new(low, start.max(*at) - low))
    }

    pub fn lasso_hits(&self, frame: &Frame, model: &Model, area: Rect) -> Vec<usize> {
        let body = frame.body;
        let count = model.entries.len();
        let (first, last) = match model.view.mode {
            Mode::List => {
                let top = body.origin.y + HEAD_H - model.scroll;
                (((area.origin.y - top) / ROW_H).floor(), ((area.max().y - top) / ROW_H).floor())
            }
            Mode::Icons => {
                let (cols, cell, _) = self.grid(body, model);
                let top = body.origin.y + 12.0 - model.scroll;
                let rows = (((area.origin.y - top) / cell.y).floor(), ((area.max().y - top) / cell.y).floor());
                (rows.0 * cols as f64, (rows.1 + 1.0) * cols as f64 - 1.0)
            }
            _ => return Vec::new(),
        };
        let first = first.max(0.0) as usize;
        let last = (last.max(-1.0) + 1.0).min(count as f64) as usize;
        (first..last.max(first))
            .filter(|index| {
                self.entry_rect(frame, model, *index).is_some_and(|cell| {
                    let cell = match model.view.mode {
                        Mode::Icons => rect(cell.origin.x + 10.0, cell.origin.y + 4.0, cell.size.x - 20.0, cell.size.y - 8.0),
                        _ => cell,
                    };
                    cell.origin.x < area.max().x && area.origin.x < cell.max().x && cell.origin.y < area.max().y && area.origin.y < cell.max().y
                })
            })
            .collect()
    }

    pub fn content_height(&self, frame: &Frame, model: &Model) -> f64 {
        let count = model.entries.len();
        match model.view.mode {
            Mode::List => HEAD_H + count as f64 * ROW_H + 8.0,
            Mode::Icons => {
                let (cols, cell, _) = self.grid(frame.body, model);
                24.0 + count.div_ceil(cols) as f64 * cell.y
            }
            _ => 0.0,
        }
    }

    pub fn most_scroll(&self, frame: &Frame, model: &Model) -> f64 {
        (self.content_height(frame, model) - frame.body.size.y).max(0.0)
    }

    pub fn tree_height(&self, lines: &[Line]) -> f64 {
        lines.iter().map(|line| if matches!(line, Line::Head(_)) { 30.0 } else { LINE_H }).sum::<f64>() + 12.0
    }

    pub fn line_rects(&self, side: Rect, lines: &[Line], scroll: f64) -> Vec<Rect> {
        let mut y = side.origin.y + 6.0 - scroll;
        lines
            .iter()
            .map(|line| {
                let tall = if matches!(line, Line::Head(_)) { 30.0 } else { LINE_H };
                let out = rect(side.origin.x + 8.0, y, side.size.x - 16.0, tall);
                y += tall;
                out
            })
            .collect()
    }

    fn strip_rect(&self, body: Rect) -> Rect {
        rect(body.origin.x, body.max().y - STRIP_H, body.size.x, STRIP_H)
    }

    fn strip_shift(&self, body: Rect, model: &Model) -> f64 {
        let total = model.entries.len() as f64 * STRIP_CELL;
        if total <= body.size.x {
            return -(body.size.x - total) / 2.0;
        }
        let focus = model.focus_index().unwrap_or(0) as f64 * STRIP_CELL + STRIP_CELL / 2.0;
        (focus - body.size.x / 2.0).clamp(0.0, total - body.size.x)
    }

    fn picture_rect(&self, body: Rect) -> Rect {
        rect(body.origin.x + 24.0, body.origin.y + 20.0, body.size.x - 48.0, body.size.y - STRIP_H - 20.0 - 64.0)
    }

    pub fn column_shift(&self, body: Rect, model: &Model) -> f64 {
        let preview = match model.single().is_some_and(|one| !one.folder) {
            true => PREVIEW_W,
            false => 0.0,
        };
        let total = model.columns.len() as f64 * COLUMN_W + preview;
        (total - body.size.x).max(0.0)
    }

    pub fn column_rect(&self, body: Rect, model: &Model, at: usize) -> Rect {
        let shift = self.column_shift(body, model);
        rect(body.origin.x + at as f64 * COLUMN_W - shift, body.origin.y, COLUMN_W, body.size.y)
    }

    pub fn hit(&self, size: Vec2, model: &Model, lines: &[Line], fonts: &Fonts, at: Vec2) -> Option<Hit> {
        let frame = self.frame(size, model);
        if frame.menu.contains(at) {
            return self.bar_titles(fonts).iter().position(|(area, _)| area.contains(at)).map(Hit::Bar);
        }
        if frame.bar.contains(at) {
            let nav = self.nav_rects(frame.bar);
            if nav[0].contains(at) {
                return Some(Hit::Back);
            }
            if nav[1].contains(at) {
                return Some(Hit::Forward);
            }
            if nav[2].contains(at) {
                return Some(Hit::Up);
            }
            if self.gear_rect(frame.bar).contains(at) {
                return Some(Hit::Gear);
            }
            if let Some((mode, _)) = self.view_rects(frame.bar).into_iter().find(|(_, area)| area.contains(at)) {
                return Some(Hit::Show(mode));
            }
            let search = self.search_rect(frame.bar);
            if search.contains(at) {
                let full = model.search.as_ref().is_some_and(|search| !search.field.text.is_empty());
                return Some(match full && self.search_clear_rect(search).contains(at) {
                    true => Hit::SearchClear,
                    false => Hit::Search,
                });
            }
            let path = self.path_rect(frame.bar);
            if path.contains(at) {
                if model.path.is_none() {
                    let crumbs = self.crumbs(path, model, fonts);
                    if let Some(found) = crumbs.iter().position(|crumb| crumb.arrow.contains(at)) {
                        return Some(Hit::CrumbArrow(found));
                    }
                    if let Some(found) = crumbs.iter().position(|crumb| crumb.area.contains(at) && !crumb.last) {
                        return Some(Hit::Crumb(found));
                    }
                }
                return Some(Hit::Path);
            }
            return None;
        }
        if let Some(tabs) = frame.tabs
            && tabs.contains(at)
        {
            let (rects, plus) = self.strip_tabs(tabs, model.tabs.len());
            if plus.contains(at) {
                return Some(Hit::TabNew);
            }
            let found = rects.iter().position(|area| area.contains(at));
            return Some(match found {
                Some(index) if self.tab_close_rect(rects[index]).contains(at) => Hit::TabClose(index),
                Some(index) => Hit::Tab(index),
                None => Hit::Tabs,
            });
        }
        if let Some(banner) = frame.banner
            && banner.contains(at)
        {
            return Some(match self.dismiss_rect(banner).contains(at) {
                true => Hit::Dismiss,
                false => Hit::Banner,
            });
        }
        if let Some(jobs) = frame.jobs
            && jobs.contains(at)
        {
            let found = self.job_rows(jobs, model).into_iter().filter(|(id, _)| model.jobs.iter().any(|job| job.id == *id && job.stoppable)).find(|(_, row)| self.stop_rect(*row).contains(at));
            return Some(found.map_or(Hit::Jobs, |(id, _)| Hit::Stop(id)));
        }
        if let Some(status) = frame.status
            && status.contains(at)
        {
            let zoom = self.zoom_rect(status, model, fonts)?;
            let grown = rect(zoom.origin.x - 8.0, status.origin.y, zoom.size.x + 16.0, status.size.y);
            return grown.contains(at).then(|| Hit::Zoom(((at.x - zoom.origin.x) / zoom.size.x).clamp(0.0, 1.0)));
        }
        if let Some(split) = frame.split
            && split.contains(at)
        {
            return Some(Hit::Split);
        }
        if let Some(side) = frame.side
            && side.contains(at)
        {
            if self.rail_place(size, model, lines, Rail::Tree).is_some_and(|(area, _, _)| rail_track(area).contains(at)) {
                return Some(Hit::Rail(Rail::Tree));
            }
            let rects = self.line_rects(side, lines, model.tree_scroll);
            let found = rects.iter().position(|area| area.contains(at))?;
            if let Line::Folder { depth, .. } = &lines[found] {
                let caret = rect(rects[found].origin.x + 4.0 + *depth as f64 * 14.0, rects[found].origin.y, 18.0, LINE_H);
                if caret.contains(at) {
                    return Some(Hit::Caret(found));
                }
            }
            if let Line::Drive { drive: Some(drive), .. } = &lines[found]
                && drive.ejectable()
                && self.eject_rect(rects[found]).contains(at)
            {
                return Some(Hit::Eject(found));
            }
            return match lines[found] {
                Line::Head(_) => None,
                _ => Some(Hit::Line(found)),
            };
        }
        let body = frame.body;
        if !body.contains(at) {
            return None;
        }
        let rails = std::iter::once(Rail::Body).chain((0..model.columns.len()).map(Rail::Column).filter(|_| model.view.mode == Mode::Columns));
        for rail in rails {
            if self.rail_place(size, model, lines, rail).is_some_and(|(area, _, _)| rail_track(area).contains(at)) {
                return Some(Hit::Rail(rail));
            }
        }
        match model.view.mode {
            Mode::List => {
                if at.y < body.origin.y + HEAD_H {
                    let (name, extra) = self.list_widths(body, model);
                    if let Some((sort, _)) = self.head_edges(body, model).into_iter().find(|(_, x)| (at.x - x).abs() <= 4.0) {
                        return Some(Hit::Edge(sort));
                    }
                    let mut x = body.origin.x + 8.0 + name;
                    if at.x < x {
                        return Some(Hit::Head(Sort::Name));
                    }
                    for (sort, w) in extra {
                        x += w;
                        if at.x < x {
                            return Some(Hit::Head(sort));
                        }
                    }
                    return None;
                }
                let index = ((at.y - body.origin.y - HEAD_H + model.scroll) / ROW_H).floor();
                match index >= 0.0 && (index as usize) < model.entries.len() {
                    true => Some(Hit::Entry(index as usize)),
                    false => Some(Hit::Space),
                }
            }
            Mode::Icons => {
                let (cols, cell, left) = self.grid(body, model);
                let local = Vec2::new(at.x - body.origin.x - left, at.y - body.origin.y - 12.0 + model.scroll);
                if local.x < 0.0 || local.y < 0.0 {
                    return Some(Hit::Space);
                }
                let (col, row) = ((local.x / cell.x).floor() as usize, (local.y / cell.y).floor() as usize);
                let index = row * cols + col;
                match col < cols && index < model.entries.len() {
                    true => Some(Hit::Entry(index)),
                    false => Some(Hit::Space),
                }
            }
            Mode::Columns => {
                for (index, column) in model.columns.iter().enumerate() {
                    let area = self.column_rect(body, model, index);
                    if area.contains(at) {
                        let row = ((at.y - area.origin.y - 6.0 + column.scroll) / ROW_H).floor();
                        let row = (row >= 0.0 && (row as usize) < column.entries.len()).then_some(row as usize);
                        return Some(Hit::Column(index, row));
                    }
                }
                Some(Hit::Space)
            }
            Mode::Gallery => {
                let strip = self.strip_rect(body);
                if strip.contains(at) {
                    let shift = self.strip_shift(body, model);
                    let index = ((at.x - strip.origin.x + shift) / STRIP_CELL).floor();
                    return match index >= 0.0 && (index as usize) < model.entries.len() {
                        true => Some(Hit::Strip(index as usize)),
                        false => Some(Hit::Space),
                    };
                }
                match self.picture_rect(body).contains(at) {
                    true => Some(Hit::Picture),
                    false => Some(Hit::Space),
                }
            }
        }
    }

    pub fn paint(&self, size: Vec2, job: &Paint) -> Image {
        let canvas = Canvas::new(size.x.round() as u32, size.y.round() as u32);
        self.paint_on(canvas, size, &Paint { clip: None, ..*job })
    }

    pub fn repaint(&self, before: Image, size: Vec2, job: &Paint) -> Image {
        let mut canvas = Canvas::on(before);
        canvas.clip_to(job.clip);
        self.paint_on(canvas, size, job)
    }

    fn paint_on(&self, mut canvas: Canvas, size: Vec2, job: &Paint) -> Image {
        let model = job.model;
        let fonts = job.fonts;
        let clip = job.clip;
        let frame = Frame { clip, ..self.frame(size, model) };
        canvas.round_rect(rect(0.0, 0.0, size.x, size.y), 0.0, self.back, 1.0);
        if touches(clip, frame.body) {
            self.paint_body(&mut canvas, &frame, job);
            if ringed(model, super::model::Pane::Files) && model.focus_index().is_none() {
                canvas.ring(inset(frame.body, 3.0), 8.0, 2.0, self.accent, 0.8);
            }
        }
        if let Some(lasso) = self.lasso_box(model) {
            let top = frame.body.origin.y + if model.view.mode == Mode::List { HEAD_H } else { 0.0 };
            let low = lasso.origin.max(Vec2::new(frame.body.origin.x, top));
            let high = lasso.max().min(frame.body.max());
            if high.x > low.x && high.y > low.y {
                let shown = Rect::new(low, high - low);
                canvas.round_rect(shown, 2.0, self.accent, 0.16);
                canvas.ring(shown, 2.0, 1.0, self.accent, 0.9);
            }
        }
        if let Some(side) = frame.side
            && touches(clip, side)
        {
            let piece = self.paint_side(side, job, moved(clip, side.origin));
            canvas.stamp(&piece, side.origin, 1.0);
            canvas.round_rect(rect(side.max().x - 1.0, side.origin.y, 1.0, side.size.y), 0.0, self.rule, 1.0);
        }
        if touches(clip, frame.menu) {
            self.paint_menu_bar(&mut canvas, &frame, job);
        }
        if touches(clip, frame.bar) {
            self.paint_bar(&mut canvas, &frame, job);
        }
        if let Some(tabs) = frame.tabs.filter(|tabs| touches(clip, *tabs)) {
            let piece = self.paint_tabs(tabs, job, moved(clip, tabs.origin));
            canvas.stamp(&piece, tabs.origin, 1.0);
        }
        if let Some(banner) = frame.banner.filter(|banner| touches(clip, *banner)) {
            self.paint_banner(&mut canvas, banner, model, fonts);
        }
        if let Some(jobs) = frame.jobs.filter(|jobs| touches(clip, *jobs)) {
            self.paint_jobs(&mut canvas, jobs, model, fonts);
        }
        if let Some(status) = frame.status.filter(|status| touches(clip, *status)) {
            self.paint_status(&mut canvas, status, model, fonts);
        }
        match &model.dragging {
            Some(super::model::Drag::Files { paths, at, live: true, .. }) => self.paint_drag(&mut canvas, size, job, paths, *at),
            Some(super::model::Drag::Foreign { at, paths }) => self.paint_spot(&mut canvas, size, job, paths, *at),
            _ => {}
        }
        if let Some(sheet) = &model.sheet {
            layered(&mut canvas, size, model.pop, 10.0, |layer| self.paint_sheet(layer, size, sheet, job));
        }
        if let Some(menu) = &model.menu {
            layered(&mut canvas, size, model.pop, -4.0, |layer| self.paint_menu(layer, size, menu, fonts));
        }
        if let Some((card, words, keys)) = self.tip(size, model, job.lines, fonts)
            && touches(clip, self.tip_area(card))
        {
            canvas.shadow(card, 6.0, 8.0, self.drop * 0.8);
            canvas.round_rect(card, 6.0, if self.dark { self.raised } else { [255, 255, 255] }, 1.0);
            canvas.ring(card, 6.0, 1.0, self.rule, 1.0);
            let middle = card.center().y;
            self.text(&mut canvas, fonts.small, &words, self.ink, Vec2::new(card.origin.x + 9.0, middle), card.size.x);
            if !keys.is_empty() {
                self.text_right(&mut canvas, fonts.small, &keys, self.dim, card.max().x - 9.0, middle);
            }
        }
        canvas.image
    }

    pub fn tip_words(&self, hit: &Hit, model: &Model) -> Option<(String, &'static str)> {
        let t = |words: &str| crate::lang::t(words).to_string();
        Some(match hit {
            Hit::Back => (t("Back"), "Alt+Left"),
            Hit::Forward => (t("Forward"), "Alt+Right"),
            Hit::Up => (t("Enclosing Folder"), "Alt+Up"),
            Hit::Show(mode) => (mode.name().to_string(), ["Ctrl+1", "Ctrl+2", "Ctrl+3", "Ctrl+4"][Mode::ALL.iter().position(|each| each == mode)?]),
            Hit::Gear => (t("Settings"), "Ctrl+,"),
            Hit::SearchClear => (t("End search"), "Esc"),
            Hit::CrumbArrow(_) => (t("Folders inside"), ""),
            Hit::TabNew => (t("New Tab"), "Ctrl+T"),
            Hit::TabClose(_) => (t("Close Tab"), "Ctrl+W"),
            Hit::Tab(index) => {
                let dir = model.tab_dirs().get(*index)?.clone();
                let shown = match dir.strip_prefix(&model.home) {
                    Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
                    Ok(rest) => format!("~/{}", rest.display()),
                    Err(_) => dir.display().to_string(),
                };
                (shown, "")
            }
            Hit::Dismiss => (t("Close"), "Esc"),
            Hit::Stop(_) => (t("Stop"), ""),
            Hit::Eject(_) => (t("Eject"), ""),
            _ => return None,
        })
    }

    pub fn tip(&self, size: Vec2, model: &Model, lines: &[Line], fonts: &Fonts) -> Option<(Rect, String, String)> {
        if model.menu.is_some() || model.sheet.is_some() || model.dragging.is_some() {
            return None;
        }
        let hit = model.tip.as_ref()?;
        let (words, keys) = self.tip_words(hit, model)?;
        let frame = self.frame(size, model);
        let anchor = match hit {
            Hit::Eject(index) => self.eject_rect(*self.line_rects(frame.side?, lines, model.tree_scroll).get(*index)?),
            Hit::TabClose(index) => self.tab_close_rect(*self.strip_tabs(frame.tabs?, model.tabs.len()).0.get(*index)?),
            _ => self.hover_rect(size, model, lines, fonts, hit)?,
        };
        let words = fit(fonts.small, &words, 460.0);
        let keys = key_words(keys);
        let keys_w = if keys.is_empty() { 0.0 } else { fonts.small.width_of(&keys) + 14.0 };
        let width = (fonts.small.width_of(&words) + keys_w + 18.0).ceil();
        let height = 24.0;
        let x = (anchor.center().x - width / 2.0).round().clamp(6.0, (size.x - width - 6.0).max(6.0));
        let below = anchor.max().y + 4.0;
        let y = match below + height > size.y - 6.0 {
            true => anchor.origin.y - 4.0 - height,
            false => below,
        };
        Some((rect(x, y.round(), width, height), words, keys))
    }

    pub fn tip_area(&self, card: Rect) -> Rect {
        Rect::new(card.origin - Vec2::splat(12.0), card.size + Vec2::splat(24.0))
    }

    pub fn drop_at(&self, size: Vec2, model: &Model, lines: &[Line], fonts: &Fonts, at: Vec2, paths: &[PathBuf]) -> Option<(Drop, Rect)> {
        let frame = self.frame(size, model);
        let folders = paths.iter().any(|path| model.known_folder(path));
        let into = |path: &Path, area: Rect, folder: bool| -> Option<(Drop, Rect)> {
            let bad = paths.iter().any(|each| each == path || path.starts_with(each)) || super::ops::in_trash(path);
            (!bad && folder).then(|| (Drop::Into(path.to_path_buf()), area))
        };
        if let Some(side) = frame.side
            && side.contains(at)
        {
            let rects = self.line_rects(side, lines, model.tree_scroll);
            let trash = super::ops::trash_files();
            let bar = lines
                .iter()
                .zip(&rects)
                .find(|(line, _)| matches!(line, Line::Place { path, .. } if path == &trash))
                .map(|(_, area)| rect(area.origin.x + 4.0, area.origin.y - 1.5, area.size.x - 8.0, 3.0));
            let found = rects.iter().position(|area| area.contains(at))?;
            let area = rects[found];
            return match &lines[found] {
                Line::Head(_) if folders && matches!(lines.get(found + 1), Some(Line::Place { .. })) => bar.map(|bar| (Drop::Mark, bar)),
                Line::Place { path, .. } if path == &trash => Some((Drop::Trash, area)),
                Line::Place { .. } if folders && ((at.y - area.origin.y) < area.size.y * 0.25 || (area.max().y - at.y) < area.size.y * 0.25) => bar.map(|bar| (Drop::Mark, bar)),
                Line::Place { path, .. } | Line::Folder { path, .. } | Line::Drive { path: Some(path), .. } => into(path, area, true),
                Line::Drive { path: None, .. } => None,
                Line::Head(_) | Line::Recent { .. } => None,
            };
        }
        let here = || {
            let moved = paths.iter().all(|path| path.parent() == Some(model.dir.as_path()));
            let fits = frame.body.contains(at) && !model.searching() && !super::ops::in_trash(&model.dir);
            (fits && (paths.is_empty() || !moved)).then(|| (Drop::Into(model.dir.clone()), frame.body))
        };
        let found = match self.hit(size, model, lines, fonts, at)? {
            Hit::Entry(index) | Hit::Strip(index) => {
                let one = model.entries.get(index)?;
                into(&one.path, self.entry_rect(&frame, model, index)?, one.folder)
            }
            Hit::Crumb(index) => {
                let crumb = self.crumbs(self.path_rect(frame.bar), model, fonts).get(index)?.clone();
                into(&crumb.path, crumb.area, true)
            }
            Hit::Tab(index) | Hit::TabClose(index) if index != model.tab => {
                let dir = model.tab_dirs().get(index)?.clone();
                into(&dir, *self.strip_tabs(frame.tabs?, model.tabs.len()).0.get(index)?, true)
            }
            Hit::Column(column, Some(row)) => {
                let one = model.columns.get(column)?.entries.get(row)?;
                let area = self.column_rect(frame.body, model, column);
                let y = area.origin.y + 6.0 + row as f64 * ROW_H - model.columns[column].scroll;
                into(&one.path, rect(area.origin.x + 6.0, y, COLUMN_W - 12.0, ROW_H), one.folder)
            }
            _ => None,
        };
        found.or_else(here)
    }

    fn paint_spot(&self, canvas: &mut Canvas, size: Vec2, job: &Paint, paths: &[PathBuf], at: Vec2) {
        if let Some((drop, area)) = self.drop_at(size, job.model, job.lines, job.fonts, at, paths) {
            match drop {
                Drop::Mark => canvas.round_rect(area, 1.5, self.accent, 1.0),
                Drop::Into(_) | Drop::Trash => {
                    canvas.round_rect(area, 7.0, self.accent, 0.18);
                    canvas.ring(area, 7.0, 2.0, self.accent, 1.0);
                }
            }
        }
    }

    fn paint_drag(&self, canvas: &mut Canvas, size: Vec2, job: &Paint, paths: &[PathBuf], at: Vec2) {
        self.paint_spot(canvas, size, job, paths, at);
        self.paint_card(canvas, job, paths, at);
    }

    pub fn drag_card(&self, job: &Paint, paths: &[PathBuf]) -> Image {
        let words = match paths {
            [one] => super::entry::name_of(one),
            _ => super::entry::items(paths.len()),
        };
        let wide = fit(job.fonts.body, &words, 260.0);
        let width = job.fonts.body.width_of(&wide) + 46.0 + 14.0 + 16.0;
        let mut canvas = Canvas::new(width.ceil() as u32, 56);
        self.paint_card(&mut canvas, job, paths, Vec2::ZERO);
        canvas.image
    }

    fn paint_card(&self, canvas: &mut Canvas, job: &Paint, paths: &[PathBuf], at: Vec2) {
        let fonts = job.fonts;
        let words = match paths {
            [one] => super::entry::name_of(one),
            _ => super::entry::items(paths.len()),
        };
        let shown = fit(fonts.body, &words, 260.0);
        let card = rect(at.x + 14.0, at.y + 10.0, fonts.body.width_of(&shown) + 46.0, 30.0);
        canvas.shadow(card, 8.0, 12.0, self.drop);
        canvas.round_rect(card, 8.0, if self.dark { self.raised } else { [255, 255, 255] }, 0.96);
        let family = paths.first().and_then(|path| Entry::at(path)).map_or(Family::Other, |one| one.family());
        let icon = rect(card.origin.x + 8.0, card.center().y - 9.0, 18.0, 18.0);
        self.icon_or_glyph(canvas, job.art, family.icon(), family, icon);
        self.text(canvas, fonts.body, &shown, self.ink, Vec2::new(card.origin.x + 34.0, card.center().y), card.size.x - 40.0);
        if paths.len() > 1 {
            let badge = Vec2::new(card.max().x - 2.0, card.origin.y + 2.0);
            canvas.circle(badge, 9.0, self.accent, 1.0);
            let count = fonts.small.paint_weight(&paths.len().min(99).to_string(), self.on_accent, 650.0);
            canvas.stamp(&count, (badge - logical(&count) / 2.0).round(), 1.0);
        }
    }

    fn text(&self, canvas: &mut Canvas, font: &Font, words: &str, ink: [u8; 3], at: Vec2, room: f64) {
        if room <= 4.0 || words.is_empty() {
            return;
        }
        let shown = fit(font, words, room);
        let piece = font.paint(&shown, ink);
        canvas.stamp(&piece, Vec2::new(at.x, at.y - logical(&piece).y / 2.0).round(), 1.0);
    }

    fn text_right(&self, canvas: &mut Canvas, font: &Font, words: &str, ink: [u8; 3], right: f64, middle: f64) {
        let piece = font.paint(words, ink);
        let size = logical(&piece);
        canvas.stamp(&piece, Vec2::new(right - size.x, middle - size.y / 2.0).round(), 1.0);
    }

    fn text_center(&self, canvas: &mut Canvas, font: &Font, words: &str, ink: [u8; 3], center: Vec2, room: f64) {
        let shown = fit(font, words, room);
        let piece = font.paint(&shown, ink);
        canvas.stamp(&piece, (center - logical(&piece) / 2.0).round(), 1.0);
    }

    fn paint_menu_bar(&self, canvas: &mut Canvas, frame: &Frame, job: &Paint) {
        let fonts = job.fonts;
        canvas.round_rect(frame.menu, 0.0, self.bar, 1.0);
        let open = job.model.menu.as_ref().and_then(|menu| menu.bar);
        for (index, (area, name)) in self.bar_titles(fonts).into_iter().enumerate() {
            let hot = job.model.hover == Some(Hit::Bar(index));
            if open == Some(index) {
                self.glossy(canvas, area, 6.0);
            } else if hot {
                canvas.round_rect(area, 6.0, self.hover, 0.7);
            }
            let ink = if open == Some(index) { self.on_accent } else { self.ink };
            self.text_center(canvas, fonts.body, name, ink, area.center(), area.size.x);
        }
    }

    fn button(&self, canvas: &mut Canvas, area: Rect, hot: bool, on: bool) {
        match on {
            true => self.pressed(canvas, area, 7.0),
            false => self.raised(canvas, area, 7.0, hot),
        }
    }

    fn glossy(&self, canvas: &mut Canvas, area: Rect, radius: f64) {
        canvas.shade(area, radius, lift(self.accent, 0.14), lift(self.accent, -0.10), 1.0);
        let y = area.origin.y + 1.0;
        canvas.line(Vec2::new(area.origin.x + radius, y), Vec2::new(area.max().x - radius, y), 1.0, [255, 255, 255], 0.22);
    }

    fn raised(&self, canvas: &mut Canvas, area: Rect, radius: f64, hot: bool) {
        canvas.shadow(rect(area.origin.x, area.origin.y + 1.0, area.size.x, area.size.y), radius, 3.0, if self.dark { 0.35 } else { 0.16 });
        let base = if hot { self.hover } else { self.raised };
        let (top, bottom) = match self.dark {
            true => (lift(base, 0.16), lift(base, -0.06)),
            false => (lift(base, 0.7), lift(base, -0.02)),
        };
        canvas.shade(area, radius, top, bottom, 1.0);
        canvas.ring(area, radius, 1.0, [0, 0, 0], if self.dark { 0.5 } else { 0.16 });
        let y = area.origin.y + 1.5;
        canvas.line(Vec2::new(area.origin.x + radius, y), Vec2::new(area.max().x - radius, y), 1.0, [255, 255, 255], if self.dark { 0.16 } else { 0.9 });
    }

    fn pressed(&self, canvas: &mut Canvas, area: Rect, radius: f64) {
        canvas.shade(area, radius, lift(self.accent, -0.24), lift(self.accent, 0.06), 1.0);
        canvas.ring(area, radius, 1.0, [0, 0, 0], 0.3);
        let y = area.origin.y + 1.5;
        canvas.line(Vec2::new(area.origin.x + radius, y), Vec2::new(area.max().x - radius, y), 1.0, [0, 0, 0], 0.25);
    }

    fn sunk(&self, canvas: &mut Canvas, area: Rect, radius: f64, ring: Option<f64>) {
        canvas.round_rect(area, radius, self.well, 1.0);
        let y = area.origin.y + 1.5;
        canvas.line(Vec2::new(area.origin.x + radius, y), Vec2::new(area.max().x - radius, y), 1.0, [0, 0, 0], if self.dark { 0.35 } else { 0.10 });
        let y = area.max().y + 0.5;
        canvas.line(Vec2::new(area.origin.x + radius, y), Vec2::new(area.max().x - radius, y), 1.0, [255, 255, 255], if self.dark { 0.06 } else { 0.8 });
        match ring {
            Some(width) => canvas.ring(area, radius, width, self.accent, 1.0),
            None => canvas.ring(area, radius, 1.0, self.rule, 1.0),
        }
    }

    fn paint_bar(&self, canvas: &mut Canvas, frame: &Frame, job: &Paint) {
        let model = job.model;
        let fonts = job.fonts;
        let bar = frame.bar;
        canvas.shade(bar, 0.0, lift(self.bar, if self.dark { 0.05 } else { 0.5 }), lift(self.bar, -0.03), 1.0);
        canvas.round_rect(rect(0.0, bar.max().y - 1.0, bar.size.x, 1.0), 0.0, self.rule, 1.0);
        let nav = self.nav_rects(bar);
        let able = [!model.back.is_empty(), !model.ahead.is_empty(), model.dir.parent().is_some()];
        let hits = [Hit::Back, Hit::Forward, Hit::Up];
        for (at, area) in nav.iter().enumerate() {
            let hot = able[at] && model.hover.as_ref() == Some(&hits[at]);
            self.button(canvas, *area, hot, false);
            let ink = if able[at] { self.ink } else { mix(self.bar, self.dim, 0.5) };
            let c = area.center();
            match at {
                0 => {
                    canvas.line(c + Vec2::new(3.0, -6.0), c + Vec2::new(-3.0, 0.0), 2.0, ink, 1.0);
                    canvas.line(c + Vec2::new(-3.0, 0.0), c + Vec2::new(3.0, 6.0), 2.0, ink, 1.0);
                }
                1 => {
                    canvas.line(c + Vec2::new(-3.0, -6.0), c + Vec2::new(3.0, 0.0), 2.0, ink, 1.0);
                    canvas.line(c + Vec2::new(3.0, 0.0), c + Vec2::new(-3.0, 6.0), 2.0, ink, 1.0);
                }
                _ => {
                    canvas.line(c + Vec2::new(-6.0, 3.0), c + Vec2::new(0.0, -3.0), 2.0, ink, 1.0);
                    canvas.line(c + Vec2::new(0.0, -3.0), c + Vec2::new(6.0, 3.0), 2.0, ink, 1.0);
                }
            }
        }
        let path = self.path_rect(bar);
        let editing = model.path.as_ref();
        self.sunk(canvas, path, 8.0, editing.map(|_| 2.0));
        match editing {
            Some(field) => {
                self.paint_field(canvas, fonts.body, field, rect(path.origin.x + 10.0, path.origin.y, path.size.x - 20.0, path.size.y));
            }
            None => {
                self.paint_crumbs(canvas, path, model, fonts);
            }
        }
        self.paint_search(canvas, self.search_rect(bar), model, fonts);
        let views = self.view_rects(bar);
        if let (Some((_, first)), Some((_, last))) = (views.first(), views.last()) {
            self.raised(canvas, rect(first.origin.x, first.origin.y, last.max().x - first.origin.x, first.size.y), 7.0, false);
        }
        for (at, (mode, area)) in views.iter().copied().enumerate() {
            let on = model.view.mode == mode;
            let hot = model.hover == Some(Hit::Show(mode));
            if at > 0 {
                canvas.line(Vec2::new(area.origin.x, area.origin.y + 6.0), Vec2::new(area.origin.x, area.max().y - 6.0), 1.0, self.rule, 1.0);
            }
            let face = rect(area.origin.x + 1.0, area.origin.y + 1.0, area.size.x - 2.0, area.size.y - 2.0);
            match (on, hot) {
                (true, _) => self.pressed(canvas, face, 6.0),
                (false, true) => canvas.round_rect(face, 6.0, self.hover, 0.6),
                _ => {}
            }
            let ink = if on { self.on_accent } else { self.ink };
            self.view_glyph(canvas, mode, area.center(), ink);
        }
        let gear = self.gear_rect(bar);
        self.button(canvas, gear, model.hover == Some(Hit::Gear), matches!(model.sheet, Some(Sheet::Prefs(_))));
        let ink = if matches!(model.sheet, Some(Sheet::Prefs(_))) { self.on_accent } else { self.ink };
        let c = gear.center();
        for (row, knob) in [(-5.0, 3.0), (0.0, -3.0), (5.0, 1.0)] {
            canvas.line(c + Vec2::new(-7.0, row), c + Vec2::new(7.0, row), 1.6, ink, 1.0);
            canvas.circle(c + Vec2::new(knob, row), 2.6, ink, 1.0);
        }
    }

    fn paint_search(&self, canvas: &mut Canvas, area: Rect, model: &Model, fonts: &Fonts) {
        let search = model.search.as_ref();
        let focused = search.is_some_and(|search| search.focused);
        self.sunk(canvas, area, 8.0, focused.then_some(2.0));
        let c = Vec2::new(area.origin.x + 16.0, area.center().y - 1.0);
        canvas.ring(Rect::from_center(c, Vec2::splat(10.0)), 5.0, 1.6, self.dim, 1.0);
        canvas.line(c + Vec2::new(3.5, 3.5), c + Vec2::new(7.0, 7.0), 1.8, self.dim, 1.0);
        let text = rect(area.origin.x + 30.0, area.origin.y, area.size.x - 58.0, area.size.y);
        match search {
            Some(search) if focused => self.paint_field(canvas, fonts.body, &search.field, text),
            Some(search) if !search.field.text.is_empty() => self.text(canvas, fonts.body, &search.field.text, self.ink, Vec2::new(text.origin.x, text.center().y), text.size.x),
            _ => self.text(canvas, fonts.body, crate::lang::t("Search"), self.dim, Vec2::new(text.origin.x, text.center().y), text.size.x),
        }
        if search.is_some_and(|search| !search.field.text.is_empty()) {
            let clear = self.search_clear_rect(area);
            if model.hover == Some(Hit::SearchClear) {
                canvas.round_rect(clear, 5.0, self.hover, 0.9);
            }
            let x = clear.center();
            canvas.line(x + Vec2::new(-4.0, -4.0), x + Vec2::new(4.0, 4.0), 1.6, self.dim, 1.0);
            canvas.line(x + Vec2::new(4.0, -4.0), x + Vec2::new(-4.0, 4.0), 1.6, self.dim, 1.0);
        }
    }

    pub fn crumbs(&self, path: Rect, model: &Model, fonts: &Fonts) -> Vec<Crumb> {
        let trash = super::ops::trash_files();
        let mut parts: Vec<(String, PathBuf)> = Vec::new();
        if super::ops::in_trash(&model.dir) {
            parts.push((crate::lang::t("Trash").to_string(), trash.clone()));
            if let Ok(rest) = model.dir.strip_prefix(&trash) {
                let mut at = trash.clone();
                for part in rest.components() {
                    at.push(part);
                    parts.push((part.as_os_str().to_string_lossy().into_owned(), at.clone()));
                }
            }
        } else {
            let (mut at, rest) = match model.dir.strip_prefix(&model.home) {
                Ok(rest) => {
                    parts.push(("~".to_string(), model.home.clone()));
                    (model.home.clone(), rest.to_path_buf())
                }
                Err(_) => {
                    parts.push(("/".to_string(), PathBuf::from("/")));
                    (PathBuf::from("/"), model.dir.strip_prefix("/").map(Path::to_path_buf).unwrap_or_default())
                }
            };
            for part in rest.components() {
                at.push(part);
                parts.push((part.as_os_str().to_string_lossy().into_owned(), at.clone()));
            }
        }
        let arrow = 18.0;
        let room = path.max().x - 10.0;
        let start = path.origin.x + 10.0;
        let widths: Vec<f64> = parts.iter().map(|(words, _)| fonts.body.width_of(words) + 2.0).collect();
        let mut first = 0;
        while first + 1 < parts.len() && widths[first..].iter().sum::<f64>() + arrow * (parts.len() - first) as f64 + if first > 0 { 32.0 } else { 0.0 } > room - start {
            first += 1;
        }
        let tall = path.size.y - 8.0;
        let top = path.origin.y + 4.0;
        let mut out = Vec::new();
        let mut x = start;
        if first > 0 {
            out.push(Crumb { area: rect(x - 4.0, top, 22.0, tall), arrow: rect(x + 18.0, top, 10.0, tall), path: parts[first - 1].1.clone(), words: "…".to_string(), last: false });
            x += 14.0 + arrow - 4.0;
        }
        let count = parts.len();
        for (at, (words, place)) in parts.into_iter().enumerate().skip(first) {
            let wide = widths[at].min((room - x - 14.0).max(0.0));
            out.push(Crumb { area: rect(x - 4.0, top, wide + 8.0, tall), arrow: rect(x + wide + 4.0, top, 10.0, tall), path: place, words, last: at + 1 == count });
            x += wide + arrow;
        }
        out
    }

    fn paint_crumbs(&self, canvas: &mut Canvas, path: Rect, model: &Model, fonts: &Fonts) {
        let crumbs = self.crumbs(path, model, fonts);
        let middle = path.center().y;
        for (index, crumb) in crumbs.iter().enumerate() {
            if model.hover == Some(Hit::Crumb(index)) && !crumb.last {
                canvas.round_rect(crumb.area, 5.0, self.hover, 0.9);
            }
            let ink = if crumb.last { self.ink } else { self.dim };
            let x = crumb.area.origin.x + 4.0;
            let room = crumb.area.size.x - 6.0;
            match crumb.last {
                true => {
                    let piece = fonts.body.paint_weight(&fit(fonts.body, &crumb.words, room), ink, 560.0);
                    canvas.stamp(&piece, Vec2::new(x, middle - logical(&piece).y / 2.0).round(), 1.0);
                }
                false => self.text(canvas, fonts.body, &crumb.words, ink, Vec2::new(x, middle), room + 2.0),
            }
            if model.hover == Some(Hit::CrumbArrow(index)) {
                canvas.round_rect(crumb.arrow, 4.0, self.hover, 0.9);
            }
            if !crumb.last || model.hover == Some(Hit::CrumbArrow(index)) {
                self.chevron(canvas, crumb.arrow.center(), self.dim);
            }
        }
    }

    fn chevron(&self, canvas: &mut Canvas, c: Vec2, ink: [u8; 3]) {
        canvas.line(c + Vec2::new(-2.0, -4.0), c + Vec2::new(2.0, 0.0), 1.4, ink, 1.0);
        canvas.line(c + Vec2::new(2.0, 0.0), c + Vec2::new(-2.0, 4.0), 1.4, ink, 1.0);
    }

    fn view_glyph(&self, canvas: &mut Canvas, mode: Mode, c: Vec2, ink: [u8; 3]) {
        match mode {
            Mode::List => {
                for row in [-5.0, 0.0, 5.0] {
                    canvas.circle(c + Vec2::new(-6.0, row), 1.3, ink, 1.0);
                    canvas.line(c + Vec2::new(-3.0, row), c + Vec2::new(7.0, row), 1.6, ink, 1.0);
                }
            }
            Mode::Icons => {
                for (x, y) in [(-4.5, -4.5), (4.5, -4.5), (-4.5, 4.5), (4.5, 4.5)] {
                    canvas.round_rect(Rect::from_center(c + Vec2::new(x, y), Vec2::splat(6.5)), 1.5, ink, 1.0);
                }
            }
            Mode::Columns => {
                let outer = Rect::from_center(c, Vec2::new(17.0, 13.0));
                canvas.ring(outer, 2.5, 1.5, ink, 1.0);
                for x in [-2.8, 2.8] {
                    canvas.line(c + Vec2::new(x, -6.0), c + Vec2::new(x, 6.0), 1.5, ink, 1.0);
                }
            }
            Mode::Gallery => {
                canvas.ring(Rect::from_center(c + Vec2::new(0.0, -2.5), Vec2::new(16.0, 9.0)), 2.0, 1.5, ink, 1.0);
                for x in [-6.0, 0.0, 6.0] {
                    canvas.round_rect(Rect::from_center(c + Vec2::new(x, 6.0), Vec2::new(4.0, 3.0)), 1.0, ink, 1.0);
                }
            }
        }
    }

    fn field_shift(font: &Font, field: &Field, area: Rect) -> f64 {
        let before = font.width_of(&field.text[..field.caret]);
        let total = font.width_of(&field.text);
        (before - area.size.x + 4.0).max(0.0).min((total - area.size.x + 4.0).max(0.0))
    }

    pub fn field_caret(&self, font: &Font, field: &Field, area: Rect, x: f64) -> usize {
        let x = x - area.origin.x + Self::field_shift(font, field, area);
        let mut best = (0, f64::MAX);
        for (at, _) in field.text.char_indices().chain(std::iter::once((field.text.len(), ' '))) {
            let gap = (font.width_of(&field.text[..at]) - x).abs();
            if gap < best.1 {
                best = (at, gap);
            }
        }
        best.0
    }

    pub fn field_area<'f>(&self, size: Vec2, model: &Model, fonts: &Fonts<'f>, spot: Spot) -> Option<(Rect, &'f Font)> {
        let frame = self.frame(size, model);
        match spot {
            Spot::Path => {
                let path = self.path_rect(frame.bar);
                Some((rect(path.origin.x + 10.0, path.origin.y, path.size.x - 20.0, path.size.y), fonts.body))
            }
            Spot::Search => {
                let area = self.search_rect(frame.bar);
                Some((rect(area.origin.x + 30.0, area.origin.y, area.size.x - 58.0, area.size.y), fonts.body))
            }
            Spot::Rename => {
                let (path, _) = model.renaming.as_ref()?;
                if model.view.mode == Mode::Columns {
                    let (index, column) = model.columns.iter().enumerate().find(|(_, column)| column.entries.iter().any(|each| &each.path == path))?;
                    let row = column.entries.iter().position(|each| &each.path == path)?;
                    let area = self.column_rect(frame.body, model, index);
                    let line = rect(area.origin.x + 6.0, area.origin.y + 6.0 + row as f64 * ROW_H - column.scroll, COLUMN_W - 12.0, ROW_H);
                    let box_area = rect(line.origin.x + 28.0, line.origin.y + 2.0, line.size.x - 34.0, ROW_H - 4.0);
                    return Some((rect(box_area.origin.x + 4.0, box_area.origin.y, box_area.size.x - 8.0, box_area.size.y), fonts.body));
                }
                let index = model.index_of(path)?;
                let area = self.entry_rect(&frame, model, index)?;
                match model.view.mode {
                    Mode::List => {
                        let (name_w, _) = self.list_widths(frame.body, model);
                        Some((rect(area.origin.x + 34.0, area.origin.y, name_w - 44.0, ROW_H), fonts.body))
                    }
                    Mode::Icons => {
                        let zoom = f64::from(model.prefs.zoom);
                        let (_, cell, _) = self.grid(frame.body, model);
                        let label_top = area.origin.y + 6.0 + zoom + 8.0;
                        let font = if zoom >= 96.0 { fonts.body } else { fonts.small };
                        Some((rect(area.origin.x + 8.0, label_top - 2.0, cell.x - 16.0, 24.0), font))
                    }
                    _ => None,
                }
            }
        }
    }

    fn paint_field(&self, canvas: &mut Canvas, font: &Font, field: &Field, area: Rect) {
        let middle = area.center().y;
        let before = font.width_of(&field.text[..field.caret]);
        let shift = Self::field_shift(font, field, area);
        let mut piece_canvas = Canvas::new(area.size.x.ceil().max(1.0) as u32, area.size.y.ceil().max(1.0) as u32);
        if let Some((from, to)) = field.picked() {
            let x0 = font.width_of(&field.text[..from]) - shift;
            let x1 = font.width_of(&field.text[..to]) - shift;
            piece_canvas.round_rect(rect(x0, area.size.y / 2.0 - 9.0, x1 - x0, 18.0), 3.0, self.accent, 0.35);
        }
        let words = font.paint(&field.text, self.ink);
        piece_canvas.stamp(&words, Vec2::new(-shift, area.size.y / 2.0 - logical(&words).y / 2.0).round(), 1.0);
        let caret_x = (before - shift).round() + 0.5;
        piece_canvas.round_rect(rect(caret_x, area.size.y / 2.0 - 9.0, 1.5, 18.0), 0.0, self.accent, 1.0);
        canvas.stamp(&piece_canvas.image, Vec2::new(area.origin.x, middle - area.size.y / 2.0).round(), 1.0);
    }

    fn icon_or_glyph(&self, canvas: &mut Canvas, art: &Art, name: &str, family: Family, area: Rect) {
        match art.icon(name, area.size.y) {
            Some(image) => {
                let at = (area.center() - logical(image) / 2.0).round();
                if !self.dark && pale(image) {
                    let edge = silhouette(image);
                    let step = 1.0 / crate::paint::density();
                    for shift in [Vec2::new(step, 0.0), Vec2::new(-step, 0.0), Vec2::new(0.0, step), Vec2::new(0.0, -step)] {
                        canvas.stamp(&edge, at + shift, 0.10);
                    }
                    canvas.stamp(&edge, at + Vec2::new(0.0, step), 0.12);
                }
                canvas.stamp(image, at, 1.0);
            }
            None => self.glyph(canvas, family, area),
        }
    }

    fn glyph(&self, canvas: &mut Canvas, family: Family, area: Rect) {
        let side = area.size.y;
        let tint = family.tint();
        match family {
            Family::Folder => {
                let body = rect(area.origin.x + side * 0.06, area.origin.y + side * 0.22, side * 0.88, side * 0.64);
                let tab = rect(body.origin.x, body.origin.y - side * 0.08, side * 0.38, side * 0.16);
                canvas.round_rect(tab, side * 0.05, mix(tint, [0, 0, 0], 0.15), 1.0);
                canvas.round_rect(body, side * 0.08, tint, 1.0);
                canvas.round_rect(rect(body.origin.x, body.origin.y + side * 0.1, body.size.x, body.size.y - side * 0.1), side * 0.08, mix(tint, [255, 255, 255], 0.18), 1.0);
            }
            _ => {
                let page = rect(area.origin.x + side * 0.18, area.origin.y + side * 0.06, side * 0.64, side * 0.88);
                canvas.round_rect(page, side * 0.07, if self.dark { [226, 230, 238] } else { [250, 251, 253] }, 1.0);
                canvas.ring(page, side * 0.07, (side * 0.025).max(1.0), mix(tint, [0, 0, 0], 0.1), 0.7);
                let band = rect(page.origin.x, page.max().y - side * 0.26, page.size.x, side * 0.16);
                canvas.round_rect(band, 0.0, tint, 1.0);
            }
        }
    }

    fn entry_art(&self, canvas: &mut Canvas, art: &Art, model: &Model, entry: &Entry, area: Rect) {
        let family = entry.family();
        if model.prefs.thumbnails
            && family.shows_picture()
            && area.size.y >= 32.0
            && let Some(image) = art.thumb(&entry.path, area.size.y)
        {
            let size = logical(image);
            let fit = shrink_fit(size, area.size);
            let at = area.origin + fit.origin;
            if solid(image) {
                canvas.shadow(rect(at.x, at.y, fit.size.x, fit.size.y), 3.0, 6.0, self.drop * 0.6);
            }
            let shown = match (fit.size.x - size.x).abs() < 1.0 {
                true => image.clone(),
                false => image.scaled(crate::paint::dense(fit.size.x), crate::paint::dense(fit.size.y)),
            };
            canvas.stamp(&shown, at.round(), 1.0);
            return;
        }
        let name = match family {
            Family::Folder => folder_icon(&entry.path, model),
            _ => family.icon(),
        };
        self.icon_or_glyph(canvas, art, name, family, area);
        if entry.link {
            let badge = rect(area.origin.x, area.max().y - area.size.y * 0.32, area.size.y * 0.32, area.size.y * 0.32);
            canvas.circle(badge.center(), badge.size.x / 2.0, self.card, 1.0);
            let c = badge.center();
            let r = badge.size.x * 0.25;
            canvas.line(c + Vec2::new(-r, r), c + Vec2::new(r, -r), 1.4, self.ink, 1.0);
            canvas.line(c + Vec2::new(0.0, -r), c + Vec2::new(r, -r), 1.4, self.ink, 1.0);
            canvas.line(c + Vec2::new(r, 0.0), c + Vec2::new(r, -r), 1.4, self.ink, 1.0);
        }
    }

    fn paint_side(&self, side: Rect, job: &Paint, clip: Option<Rect>) -> Image {
        let model = job.model;
        let fonts = job.fonts;
        let mut canvas = Canvas::new(side.size.x.round() as u32, side.size.y.round() as u32);
        canvas.clip_to(clip);
        let local = rect(0.0, 0.0, side.size.x, side.size.y);
        canvas.round_rect(local, 0.0, self.side, 1.0);
        let rects = self.line_rects(local, job.lines, model.tree_scroll);
        for (index, (line, area)) in job.lines.iter().zip(rects).enumerate() {
            if area.max().y < 0.0 || area.origin.y > local.size.y || !touches(clip, area) {
                continue;
            }
            let here = line.path().is_some_and(|path| path == model.dir);
            let hot = model.hover == Some(Hit::Line(index)) || model.hover == Some(Hit::Caret(index));
            let indent = match line {
                Line::Folder { depth, .. } => *depth as f64 * 14.0,
                _ => 0.0,
            };
            if here {
                self.glossy(&mut canvas, area, 7.0);
            } else if hot {
                canvas.round_rect(area, 7.0, self.hover, 0.7);
            }
            if model.side_at == Some(index) && ringed(model, super::model::Pane::Side) {
                canvas.ring(inset(area, 1.0), 7.0, 2.0, if here { self.on_accent } else { self.accent }, 1.0);
            }
            let ink = if here { self.on_accent } else { self.ink };
            let middle = area.center().y;
            match line {
                Line::Head(words) => {
                    let piece = fonts.small.paint_weight(&words.to_uppercase(), self.dim, 600.0);
                    canvas.stamp(&piece, Vec2::new(area.origin.x + 6.0, area.max().y - 7.0 - logical(&piece).y).round(), 1.0);
                }
                Line::Place { name, icon, path, .. } => {
                    let family = if path == &super::ops::trash_files() { Family::Other } else { Family::Folder };
                    self.icon_or_glyph(&mut canvas, job.art, icon, family, rect(area.origin.x + 8.0, middle - 9.0, 18.0, 18.0));
                    self.text(&mut canvas, fonts.body, name, ink, Vec2::new(area.origin.x + 34.0, middle), area.size.x - 40.0);
                }
                Line::Recent { path, name } => {
                    let family = Entry::at(path).map_or(Family::Other, |one| one.family());
                    self.icon_or_glyph(&mut canvas, job.art, family.icon(), family, rect(area.origin.x + 8.0, middle - 9.0, 18.0, 18.0));
                    self.text(&mut canvas, fonts.body, name, ink, Vec2::new(area.origin.x + 34.0, middle), area.size.x - 40.0);
                }
                Line::Drive { name, drive, .. } => {
                    let icon = match drive {
                        None => "drive-harddisk",
                        Some(drive) if drive.mount.as_deref() == Some(Path::new("/")) => "drive-harddisk",
                        Some(drive) if drive.device.is_none() => "folder-remote",
                        Some(drive) if drive.removable => "drive-removable-media",
                        Some(_) => "drive-harddisk",
                    };
                    let mounted = drive.as_ref().is_none_or(|drive| drive.mount.is_some());
                    let eject = drive.as_ref().filter(|drive| drive.ejectable()).map(|_| self.eject_rect(area));
                    let right = eject.map_or(area.max().x - 8.0, |eject| eject.origin.x - 4.0);
                    let share = drive.as_ref().and_then(|drive| drive.share_used());
                    let lift = if share.is_some() { 3.0 } else { 0.0 };
                    let faded = if mounted { ink } else { mix(ink, self.side, 0.4) };
                    self.icon_or_glyph(&mut canvas, job.art, icon, Family::Archive, rect(area.origin.x + 8.0, middle - 9.0, 18.0, 18.0));
                    self.text(&mut canvas, fonts.body, name, faded, Vec2::new(area.origin.x + 34.0, middle - lift), right - area.origin.x - 34.0);
                    if let Some(share) = share {
                        let track = rect(area.origin.x + 34.0, area.max().y - 6.0, (right - area.origin.x - 34.0).max(0.0), 3.0);
                        let (back, fill) = match here {
                            true => (mix(self.accent, self.on_accent, 0.35), self.on_accent),
                            false => (self.raised, if share > 0.9 { self.alert } else { self.accent }),
                        };
                        canvas.round_rect(track, 1.5, back, 1.0);
                        canvas.round_rect(rect(track.origin.x, track.origin.y, (track.size.x * share).max(3.0), track.size.y), 1.5, fill, 1.0);
                    }
                    if let Some(eject) = eject {
                        let hot = model.hover == Some(Hit::Eject(index));
                        if hot && !here {
                            canvas.round_rect(eject, 5.0, self.hover, 1.0);
                        }
                        let c = eject.center();
                        canvas.triangle([c + Vec2::new(-5.0, 1.5), c + Vec2::new(5.0, 1.5), c + Vec2::new(0.0, -4.5)], ink, 1.0);
                        canvas.round_rect(rect(c.x - 5.0, c.y + 3.0, 10.0, 2.0), 0.5, ink, 1.0);
                    }
                }
                Line::Folder { path, open, .. } => {
                    let c = Vec2::new(area.origin.x + 12.0 + indent, middle);
                    let caret_ink = if here { self.on_accent } else { self.dim };
                    let corners = match open {
                        true => [c + Vec2::new(-4.0, -2.0), c + Vec2::new(4.0, -2.0), c + Vec2::new(0.0, 3.0)],
                        false => [c + Vec2::new(-2.0, -4.0), c + Vec2::new(3.0, 0.0), c + Vec2::new(-2.0, 4.0)],
                    };
                    canvas.triangle(corners, caret_ink, 1.0);
                    let name = tree_name(model, path);
                    let icon_at = rect(area.origin.x + 22.0 + indent, middle - 8.0, 16.0, 16.0);
                    self.icon_or_glyph(&mut canvas, job.art, folder_icon(path, model), Family::Folder, icon_at);
                    self.text(&mut canvas, fonts.body, &name, ink, Vec2::new(icon_at.max().x + 7.0, middle), area.max().x - icon_at.max().x - 12.0);
                }
            }
        }
        self.scroll_mark(&mut canvas, local, self.tree_height(job.lines), model.tree_scroll, rail_hot(model, Rail::Tree));
        canvas.image
    }

    fn scroll_mark(&self, canvas: &mut Canvas, area: Rect, content: f64, scroll: f64, hot: bool) {
        if content <= area.size.y + 1.0 {
            return;
        }
        let thumb = rail_thumb(area, content, scroll);
        match hot {
            true => canvas.round_rect(rect(thumb.max().x - 10.0, thumb.origin.y + 2.0, 8.0, thumb.size.y - 4.0), 4.0, self.dim, 0.75),
            false => canvas.round_rect(rect(thumb.max().x - 7.0, thumb.origin.y + 2.0, 4.0, thumb.size.y - 4.0), 2.0, self.dim, 0.45),
        }
    }

    pub fn rail_place(&self, size: Vec2, model: &Model, lines: &[Line], rail: Rail) -> Option<(Rect, f64, f64)> {
        let frame = self.frame(size, model);
        let body = frame.body;
        let (area, content, scroll) = match rail {
            Rail::Tree => (frame.side?, self.tree_height(lines), model.tree_scroll),
            Rail::Body => match model.view.mode {
                Mode::List => (rect(body.origin.x, body.origin.y + HEAD_H, body.size.x, body.size.y - HEAD_H), self.content_height(&frame, model) - HEAD_H, model.scroll),
                Mode::Icons => (body, self.content_height(&frame, model), model.scroll),
                _ => return None,
            },
            Rail::Column(index) => (self.column_rect(body, model, index), 12.0 + model.columns.get(index)?.entries.len() as f64 * ROW_H, model.columns[index].scroll),
        };
        (content > area.size.y + 1.0).then_some((area, content, scroll))
    }

    fn paint_body(&self, canvas: &mut Canvas, frame: &Frame, job: &Paint) {
        let body = frame.body;
        let mut piece = Canvas::new(body.size.x.round().max(1.0) as u32, body.size.y.round().max(1.0) as u32);
        let clip = moved(frame.clip, body.origin);
        piece.clip_to(clip);
        let local = rect(0.0, 0.0, body.size.x, body.size.y);
        piece.round_rect(local, 0.0, self.back, 1.0);
        let shifted = Frame { body: local, clip, ..*frame };
        let model = job.model;
        if let Some(trouble) = &model.trouble {
            self.text_center(&mut piece, job.fonts.title, crate::lang::t("This folder cannot be opened"), self.ink, local.center() - Vec2::new(0.0, 14.0), local.size.x - 40.0);
            self.text_center(&mut piece, job.fonts.body, trouble, self.dim, local.center() + Vec2::new(0.0, 14.0), local.size.x - 40.0);
        } else {
            match model.view.mode {
                Mode::List => self.paint_list(&mut piece, &shifted, job),
                Mode::Icons => self.paint_icons(&mut piece, &shifted, job),
                Mode::Columns => self.paint_columns(&mut piece, &shifted, job),
                Mode::Gallery => self.paint_gallery(&mut piece, &shifted, job),
            }
            if model.entries.is_empty() && model.view.mode != Mode::Columns {
                let hunting = model.search.as_ref().filter(|_| model.searching()).map(|search| search.done);
                let trash = super::ops::in_trash(&model.dir);
                let quiet = match (model.loading.is_some(), trash) {
                    _ if hunting == Some(false) => Some((crate::lang::t("Searching…"), None, None)),
                    _ if hunting == Some(true) => Some((crate::lang::t("Nothing matches"), Some(crate::lang::t("Check the spelling or try fewer letters")), Some(Empty::Search))),
                    (true, _) if model.showing_loading() => Some((crate::lang::t("Loading…"), None, None)),
                    (true, _) => None,
                    (false, true) => Some((crate::lang::t("The Trash is empty"), Some(crate::lang::t("Things you move to the Trash wait here until you empty it")), Some(Empty::Trash))),
                    (false, false) => Some((crate::lang::t("This folder is empty"), Some(crate::lang::t("Drop files here, or press Shift+Ctrl+N for a new folder")), Some(Empty::Folder))),
                };
                if let Some((words, hint, art)) = quiet {
                    let middle = match model.view.mode {
                        Mode::Gallery => self.picture_rect(local).center(),
                        _ => local.center(),
                    };
                    let lift = if art.is_some() { 36.0 } else { 0.0 };
                    if let Some(art) = art {
                        self.paint_empty(&mut piece, job, art, &model.dir, Rect::from_center(middle - Vec2::new(0.0, 34.0), Vec2::splat(88.0)));
                    }
                    self.text_center(&mut piece, job.fonts.body, words, if art.is_some() { self.ink } else { self.dim }, middle + Vec2::new(0.0, lift), local.size.x - 40.0);
                    if let Some(hint) = hint {
                        self.text_center(&mut piece, job.fonts.small, hint, self.dim, middle + Vec2::new(0.0, lift + 22.0), local.size.x - 40.0);
                    }
                }
            }
        }
        canvas.stamp(&piece.image, body.origin, 1.0);
    }

    fn paint_empty(&self, canvas: &mut Canvas, job: &Paint, art: Empty, dir: &Path, area: Rect) {
        let side = area.size.y.ceil() as u32;
        let mut layer = Canvas::new(side, side);
        let local = rect(0.0, 0.0, area.size.x, area.size.y);
        match art {
            Empty::Folder => self.icon_or_glyph(&mut layer, job.art, folder_icon(dir, job.model), Family::Folder, local),
            Empty::Trash => self.icon_or_glyph(&mut layer, job.art, "user-trash", Family::Other, local),
            Empty::Search => {
                let c = local.center() - Vec2::new(8.0, 8.0);
                layer.ring(Rect::from_center(c, Vec2::splat(50.0)), 25.0, 6.0, self.dim, 1.0);
                layer.line(c + Vec2::new(19.0, 19.0), c + Vec2::new(36.0, 36.0), 8.0, self.dim, 1.0);
            }
        }
        canvas.stamp(&layer.image, area.origin.round(), 0.55);
    }

    fn row_back(&self, canvas: &mut Canvas, area: Rect, picked: bool, hot: bool, focused: bool, stripe: bool) {
        if picked {
            self.glossy(canvas, area, 6.0);
        } else if hot {
            canvas.round_rect(area, 6.0, self.hover, 0.6);
        } else if stripe {
            canvas.round_rect(area, 6.0, self.well, 0.55);
        }
        if focused && !picked {
            canvas.ring(area, 6.0, 1.2, self.accent, 0.8);
        }
    }

    fn paint_list(&self, canvas: &mut Canvas, frame: &Frame, job: &Paint) {
        let model = job.model;
        let fonts = job.fonts;
        let body = frame.body;
        let (name_w, extra) = self.list_widths(body, model);
        let top = HEAD_H;
        let first = ((model.scroll / ROW_H).floor().max(0.0)) as usize;
        let count = (body.size.y / ROW_H).ceil() as usize + 2;
        for at in first..(first + count).min(model.entries.len()) {
            let entry = &model.entries[at];
            let Some(area) = self.entry_rect(frame, model, at) else {
                continue;
            };
            if area.max().y < top || area.origin.y > body.size.y || !touches(frame.clip, area) {
                continue;
            }
            let picked = model.picked.contains(&entry.path);
            let hot = model.hover == Some(Hit::Entry(at));
            let focused = model.focus.as_ref() == Some(&entry.path);
            self.row_back(canvas, area, picked, hot, focused && model.picked.len() > 1, at % 2 == 1);
            if focused && ringed(model, super::model::Pane::Files) {
                canvas.ring(inset(area, 1.0), 6.0, 2.0, if picked { self.on_accent } else { self.accent }, 1.0);
            }
            let ink = if picked { self.on_accent } else { self.ink };
            let dim = if picked { mix(self.on_accent, self.accent, 0.25) } else { self.dim };
            let middle = area.center().y;
            self.entry_art(canvas, job.art, model, entry, rect(area.origin.x + 8.0, middle - 9.0, 18.0, 18.0));
            let name_x = area.origin.x + 34.0;
            let renaming = model.renaming.as_ref().filter(|(path, _)| path == &entry.path);
            match renaming {
                Some((_, field)) => {
                    let box_area = rect(name_x - 4.0, area.origin.y + 2.0, name_w - 36.0, ROW_H - 4.0);
                    canvas.round_rect(box_area, 4.0, self.card, 1.0);
                    canvas.ring(box_area, 4.0, 1.5, self.accent, 1.0);
                    self.paint_field(canvas, fonts.body, field, rect(name_x, area.origin.y, name_w - 44.0, ROW_H));
                }
                None => {
                    let faded = model.clip.as_ref().is_some_and(|(paths, cut)| *cut && paths.contains(&entry.path));
                    let ink = if faded { dim } else { ink };
                    self.text(canvas, fonts.body, &entry.name, ink, Vec2::new(name_x, middle), name_w - 44.0);
                }
            }
            let mut x = area.origin.x + name_w;
            for (sort, w) in &extra {
                let words = match sort {
                    Sort::Modified => model.date(entry.modified),
                    Sort::Size if entry.folder => "--".to_string(),
                    Sort::Size => super::entry::size_words(entry.size),
                    Sort::Kind => entry.kind_words(),
                    Sort::Origin => match &entry.trashed {
                        Some((from, _)) => from.parent().map(|folder| super::entry::short_path(folder, &model.home)).unwrap_or_default(),
                        None => entry.path.parent().map(|folder| super::entry::short_path(folder, &model.home)).unwrap_or_default(),
                    },
                    Sort::Deleted => entry.trashed.as_ref().map(|(_, when)| model.date(*when)).unwrap_or_default(),
                    Sort::Name => String::new(),
                };
                match sort {
                    Sort::Size => self.text_right(canvas, fonts.body, &fit(fonts.body, &words, w - 20.0), dim, x + w - 16.0, middle),
                    _ => self.text(canvas, fonts.body, &words, dim, Vec2::new(x + 8.0, middle), w - 16.0),
                }
                x += w;
            }
        }
        if touches(frame.clip, rect(0.0, 0.0, body.size.x, top)) {
            canvas.round_rect(rect(0.0, 0.0, body.size.x, top), 0.0, self.back, 1.0);
            canvas.round_rect(rect(8.0, top - 1.0, body.size.x - 16.0, 1.0), 0.0, self.rule, 1.0);
            let mut x = 8.0;
            let mut heads = vec![(Sort::Name, name_w)];
            heads.extend(extra.iter().copied());
            for (sort, w) in heads {
                let on = model.view.sort == sort;
                let hot = model.hover == Some(Hit::Head(sort));
                if hot {
                    canvas.round_rect(rect(x + 2.0, 3.0, w - 4.0, top - 7.0), 5.0, self.hover, 0.6);
                }
                let ink = if on { self.ink } else { self.dim };
                let name = match (sort, model.searching()) {
                    (Sort::Origin, true) => crate::lang::t("Folder"),
                    _ => sort.name(),
                };
                let label = fonts.small.paint_weight(name, ink, if on { 620.0 } else { 500.0 });
                let size = logical(&label);
                let lx = match sort {
                    Sort::Name => x + 34.0,
                    Sort::Size => x + w - 16.0 - size.x,
                    _ => x + 8.0,
                };
                canvas.stamp(&label, Vec2::new(lx, top / 2.0 - size.y / 2.0).round(), 1.0);
                if on {
                    let ax = if sort == Sort::Size { lx - 10.0 } else { lx + size.x + 8.0 };
                    let c = Vec2::new(ax, top / 2.0);
                    let corners = match model.view.down {
                        false => [c + Vec2::new(-3.5, 2.0), c + Vec2::new(3.5, 2.0), c + Vec2::new(0.0, -2.5)],
                        true => [c + Vec2::new(-3.5, -2.0), c + Vec2::new(3.5, -2.0), c + Vec2::new(0.0, 2.5)],
                    };
                    canvas.triangle(corners, self.dim, 1.0);
                }
                if sort != Sort::Name {
                    let held = matches!(model.dragging, Some(super::model::Drag::Edge { sort: edge, .. }) if edge == sort);
                    match held || model.hover == Some(Hit::Edge(sort)) {
                        true => canvas.round_rect(rect(x - 1.0, 4.0, 3.0, top - 8.0), 1.5, self.accent, 1.0),
                        false => canvas.round_rect(rect(x, 7.0, 1.0, top - 14.0), 0.0, self.rule, 1.0),
                    }
                }
                x += w;
            }
        }
        self.scroll_mark(canvas, rect(0.0, top, body.size.x, body.size.y - top), self.content_height(frame, model) - top, model.scroll, rail_hot(model, Rail::Body));
    }

    fn wrap(&self, font: &Font, words: &str, room: f64, lines: usize) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        let mut rest = words.to_string();
        while !rest.is_empty() && out.len() < lines {
            if out.len() + 1 == lines || font.width_of(&rest) <= room {
                out.push(fit(font, &rest, room));
                break;
            }
            let mut cut = 0;
            let mut last_break = None;
            for (at, letter) in rest.char_indices() {
                let end = at + letter.len_utf8();
                if font.width_of(&rest[..end]) > room {
                    break;
                }
                cut = end;
                if matches!(letter, ' ' | '-' | '_' | '.') {
                    last_break = Some(end);
                }
            }
            let cut = last_break.filter(|at| *at > rest.len() / 4).unwrap_or(cut.max(rest.chars().next().map_or(1, char::len_utf8)));
            out.push(rest[..cut].to_string());
            rest = rest[cut..].to_string();
        }
        out
    }

    fn paint_icons(&self, canvas: &mut Canvas, frame: &Frame, job: &Paint) {
        let model = job.model;
        let fonts = job.fonts;
        let body = frame.body;
        let zoom = f64::from(model.prefs.zoom);
        let (cols, cell, _) = self.grid(body, model);
        let first_row = ((model.scroll - 12.0) / cell.y).floor().max(0.0) as usize;
        let rows = (body.size.y / cell.y).ceil() as usize + 2;
        let start = first_row * cols;
        let end = ((first_row + rows) * cols).min(model.entries.len());
        let label_font = if zoom >= 96.0 { fonts.body } else { fonts.small };
        for at in start..end {
            let entry = &model.entries[at];
            let Some(area) = self.entry_rect(frame, model, at).filter(|area| touches(frame.clip, *area)) else {
                continue;
            };
            let picked = model.picked.contains(&entry.path);
            let hot = model.hover == Some(Hit::Entry(at));
            let art_area = rect(area.center().x - zoom / 2.0, area.origin.y + 6.0, zoom, zoom);
            let tile = rect(art_area.origin.x - 6.0, art_area.origin.y - 4.0, zoom + 12.0, zoom + 8.0);
            if picked {
                canvas.round_rect(tile, 10.0, self.hover, 0.9);
            } else if hot {
                canvas.round_rect(tile, 10.0, self.hover, 0.45);
            }
            if model.focus.as_ref() == Some(&entry.path) && ringed(model, super::model::Pane::Files) {
                canvas.ring(tile, 10.0, 2.0, self.accent, 1.0);
            }
            self.entry_art(canvas, job.art, model, entry, art_area);
            let label_top = art_area.max().y + 8.0;
            let room = cell.x - 10.0;
            if let Some((_, field)) = model.renaming.as_ref().filter(|(path, _)| path == &entry.path) {
                let box_area = rect(area.origin.x + 4.0, label_top - 2.0, cell.x - 8.0, 24.0);
                canvas.round_rect(box_area, 4.0, self.card, 1.0);
                canvas.ring(box_area, 4.0, 1.5, self.accent, 1.0);
                self.paint_field(canvas, label_font, field, rect(box_area.origin.x + 4.0, box_area.origin.y, box_area.size.x - 8.0, box_area.size.y));
                continue;
            }
            let lines = self.wrap(label_font, &entry.name, room, 2);
            let line_h = label_font.line_height() + 1.0;
            let widest = lines.iter().map(|line| label_font.width_of(line)).fold(0.0, f64::max);
            if picked {
                let pill = rect(area.center().x - widest / 2.0 - 6.0, label_top - 2.0, widest + 12.0, line_h * lines.len() as f64 + 4.0);
                self.glossy(canvas, pill, 6.0);
            }
            let faded = model.clip.as_ref().is_some_and(|(paths, cut)| *cut && paths.contains(&entry.path));
            let ink = match (picked, faded) {
                (true, _) => self.on_accent,
                (false, true) => self.dim,
                _ => self.ink,
            };
            for (row, line) in lines.iter().enumerate() {
                let piece = label_font.paint(line, ink);
                let size = logical(&piece);
                canvas.stamp(&piece, Vec2::new(area.center().x - size.x / 2.0, label_top + row as f64 * line_h).round(), 1.0);
            }
        }
        self.scroll_mark(canvas, body, self.content_height(frame, model), model.scroll, rail_hot(model, Rail::Body));
    }

    fn paint_columns(&self, canvas: &mut Canvas, frame: &Frame, job: &Paint) {
        let model = job.model;
        let fonts = job.fonts;
        let body = frame.body;
        let count = model.columns.len();
        for (index, column) in model.columns.iter().enumerate() {
            let area = self.column_rect(body, model, index);
            if area.max().x < 0.0 || area.origin.x > body.size.x {
                continue;
            }
            let chosen_next = model.columns.get(index + 1).map(|next| next.dir.clone());
            let current = column.dir == model.dir;
            for (row, entry) in column.entries.iter().enumerate() {
                let y = area.origin.y + 6.0 + row as f64 * ROW_H - column.scroll;
                if y + ROW_H < 0.0 || y > body.size.y {
                    continue;
                }
                let line = rect(area.origin.x + 6.0, y, COLUMN_W - 12.0, ROW_H);
                if !touches(frame.clip, line) {
                    continue;
                }
                let picked = match current {
                    true => model.picked.contains(&entry.path),
                    false => chosen_next.as_ref() == Some(&entry.path),
                };
                let hot = model.hover == Some(Hit::Column(index, Some(row)));
                match (picked, current || index + 1 == count) {
                    (true, true) => canvas.round_rect(line, 6.0, self.accent, 1.0),
                    (true, false) => canvas.round_rect(line, 6.0, self.hover, 1.0),
                    _ if hot => canvas.round_rect(line, 6.0, self.hover, 0.5),
                    _ => {}
                }
                if current && model.focus.as_ref() == Some(&entry.path) && ringed(model, super::model::Pane::Files) {
                    canvas.ring(inset(line, 1.0), 6.0, 2.0, if picked { self.on_accent } else { self.accent }, 1.0);
                }
                let strong = picked && (current || index + 1 == count);
                let ink = if strong { self.on_accent } else { self.ink };
                let middle = line.center().y;
                self.entry_art(canvas, job.art, model, entry, rect(line.origin.x + 6.0, middle - 9.0, 18.0, 18.0));
                if let Some((_, field)) = model.renaming.as_ref().filter(|(path, _)| path == &entry.path) {
                    let box_area = rect(line.origin.x + 28.0, line.origin.y + 2.0, line.size.x - 34.0, ROW_H - 4.0);
                    canvas.round_rect(box_area, 4.0, self.card, 1.0);
                    canvas.ring(box_area, 4.0, 1.5, self.accent, 1.0);
                    self.paint_field(canvas, fonts.body, field, rect(box_area.origin.x + 4.0, box_area.origin.y, box_area.size.x - 8.0, box_area.size.y));
                    continue;
                }
                let room = line.size.x - 34.0 - if entry.folder { 18.0 } else { 6.0 };
                self.text(canvas, fonts.body, &entry.name, ink, Vec2::new(line.origin.x + 30.0, middle), room);
                if entry.folder {
                    self.chevron(canvas, Vec2::new(line.max().x - 10.0, middle), if strong { self.on_accent } else { self.dim });
                }
            }
            if column.entries.is_empty() {
                self.text_center(canvas, fonts.small, crate::lang::t("Empty"), self.dim, Vec2::new(area.center().x, area.origin.y + 30.0), COLUMN_W - 20.0);
            }
            let content = 12.0 + column.entries.len() as f64 * ROW_H;
            self.scroll_mark(canvas, area, content, column.scroll, rail_hot(model, Rail::Column(index)));
            canvas.round_rect(rect(area.max().x - 1.0, 0.0, 1.0, body.size.y), 0.0, self.rule, 1.0);
        }
        if let Some(one) = model.single().filter(|one| !one.folder) {
            let left = self.column_rect(body, model, count).origin.x;
            let area = rect(left, 0.0, PREVIEW_W, body.size.y);
            self.paint_preview(canvas, area, one, job);
        }
    }

    fn paint_preview(&self, canvas: &mut Canvas, area: Rect, one: &Entry, job: &Paint) {
        let model = job.model;
        let fonts = job.fonts;
        let pad = 20.0;
        let picture = rect(area.origin.x + pad, area.origin.y + pad, area.size.x - pad * 2.0, (area.size.y * 0.5).min(area.size.x - pad * 2.0));
        match model.peek.as_ref().filter(|lines| lines.iter().any(|line| !line.trim().is_empty())) {
            Some(lines) => self.paint_peek(canvas, picture, lines, fonts),
            None => {
                let side = picture.size.x.min(picture.size.y);
                let square = Rect::from_center(picture.center(), Vec2::splat(side));
                let thumb = model.prefs.thumbnails && one.family().shows_picture();
                match thumb {
                    true => self.entry_art(canvas, job.art, model, one, square),
                    false => {
                        let small = Rect::from_center(picture.center(), Vec2::splat(side.min(128.0)));
                        self.entry_art(canvas, job.art, model, one, small);
                    }
                }
            }
        }
        let mut y = picture.max().y + 18.0;
        let title = fonts.title.paint_weight(&fit(fonts.title, &one.name, area.size.x - pad * 2.0), self.ink, 600.0);
        canvas.stamp(&title, Vec2::new(area.center().x - logical(&title).x / 2.0, y).round(), 1.0);
        y += logical(&title).y + 4.0;
        self.text_center(canvas, fonts.small, &format!("{} · {}", one.kind_words(), super::entry::size_words(one.size)), self.dim, Vec2::new(area.center().x, y + 8.0), area.size.x - pad * 2.0);
        y += 34.0;
        let rows = [
            (crate::lang::t("Modified"), model.date(one.modified)),
            (crate::lang::t("Where"), one.path.parent().map(|p| p.display().to_string()).unwrap_or_default()),
        ];
        for (label, value) in rows {
            canvas.round_rect(rect(area.origin.x + pad, y - 6.0, area.size.x - pad * 2.0, 1.0), 0.0, self.rule, 1.0);
            self.text(canvas, fonts.small, label, self.dim, Vec2::new(area.origin.x + pad, y + 10.0), 90.0);
            self.text(canvas, fonts.small, &value, self.ink, Vec2::new(area.origin.x + pad + 90.0, y + 10.0), area.size.x - pad * 2.0 - 90.0);
            y += 30.0;
        }
    }

    fn paint_peek(&self, canvas: &mut Canvas, area: Rect, lines: &[String], fonts: &Fonts) {
        canvas.shadow(area, 6.0, 10.0, self.drop * 0.5);
        canvas.round_rect(area, 6.0, if self.dark { [24, 26, 32] } else { [255, 255, 255] }, 1.0);
        canvas.ring(area, 6.0, 1.0, self.rule, 1.0);
        let line_h = fonts.small.line_height() + 2.0;
        let mut y = area.origin.y + 10.0;
        for line in lines {
            if y + line_h > area.max().y - 6.0 {
                break;
            }
            self.text(canvas, fonts.small, line, self.dim, Vec2::new(area.origin.x + 10.0, y + line_h / 2.0), area.size.x - 20.0);
            y += line_h;
        }
    }

    fn paint_gallery(&self, canvas: &mut Canvas, frame: &Frame, job: &Paint) {
        let model = job.model;
        let fonts = job.fonts;
        let body = frame.body;
        let picture = self.picture_rect(body);
        let focus = model.focus_index().and_then(|at| model.entries.get(at)).or_else(|| model.single());
        if let Some(one) = focus {
            match (model.peek.as_ref().filter(|lines| lines.iter().any(|line| !line.trim().is_empty())), one.family().shows_picture() && model.prefs.thumbnails) {
                (Some(lines), _) => {
                    let page = Rect::from_center(picture.center(), Vec2::new(picture.size.x.min(picture.size.y * 0.8), picture.size.y));
                    self.paint_peek(canvas, page, lines, fonts);
                }
                (None, true) => {
                    let side = picture.size.x.min(picture.size.y).clamp(1.0, 1600.0);
                    let wide = rect(picture.origin.x, picture.origin.y, picture.size.x, picture.size.y);
                    match job.art.thumb(&one.path, side) {
                        Some(image) => {
                            let fitted = grow_fit(logical(image), wide.size, MOST_GROW);
                            let at = wide.origin + fitted.origin;
                            let shown = image.scaled(crate::paint::dense(fitted.size.x), crate::paint::dense(fitted.size.y));
                            if solid(image) {
                                canvas.shadow(rect(at.x, at.y, fitted.size.x, fitted.size.y), 4.0, 14.0, self.drop);
                            }
                            canvas.stamp(&shown, at.round(), 1.0);
                        }
                        None => self.entry_art(canvas, job.art, model, one, Rect::from_center(picture.center(), Vec2::splat(side.min(160.0)))),
                    }
                }
                (None, false) => self.entry_art(canvas, job.art, model, one, Rect::from_center(picture.center(), Vec2::splat(picture.size.y.min(160.0)))),
            }
            let y = picture.max().y + 22.0;
            let name = fonts.title.paint_weight(&fit(fonts.title, &one.name, body.size.x - 60.0), self.ink, 600.0);
            canvas.stamp(&name, Vec2::new(body.size.x / 2.0 - logical(&name).x / 2.0, y - logical(&name).y / 2.0).round(), 1.0);
            let detail = match one.folder {
                true => one.kind_words(),
                false => format!("{} · {} · {}", one.kind_words(), super::entry::size_words(one.size), model.date(one.modified)),
            };
            self.text_center(canvas, fonts.small, &detail, self.dim, Vec2::new(body.size.x / 2.0, y + 22.0), body.size.x - 60.0);
        }
        let strip = self.strip_rect(body);
        canvas.round_rect(strip, 0.0, self.side, 1.0);
        canvas.round_rect(rect(0.0, strip.origin.y, body.size.x, 1.0), 0.0, self.rule, 1.0);
        let shift = self.strip_shift(body, model);
        let first = ((shift / STRIP_CELL).floor().max(0.0)) as usize;
        let shown = (body.size.x / STRIP_CELL).ceil() as usize + 2;
        for at in first..(first + shown).min(model.entries.len()) {
            let entry = &model.entries[at];
            let Some(cell) = self.entry_rect(frame, model, at) else {
                continue;
            };
            let picked = model.picked.contains(&entry.path);
            let tile = rect(cell.origin.x + 6.0, cell.origin.y, cell.size.x - 12.0, cell.size.y);
            if picked {
                self.glossy(canvas, tile, 10.0);
            } else if model.hover == Some(Hit::Strip(at)) {
                canvas.round_rect(tile, 10.0, self.hover, 0.6);
            }
            if model.focus.as_ref() == Some(&entry.path) && ringed(model, super::model::Pane::Files) {
                canvas.ring(inset(tile, 1.0), 10.0, 2.0, if picked { self.on_accent } else { self.accent }, 1.0);
            }
            let side = tile.size.x - 30.0;
            self.entry_art(canvas, job.art, model, entry, Rect::from_center(Vec2::new(tile.center().x, tile.origin.y + 8.0 + side / 2.0), Vec2::splat(side)));
            let ink = if picked { self.on_accent } else { self.ink };
            self.text_center(canvas, fonts.small, &entry.name, ink, Vec2::new(tile.center().x, tile.max().y - 13.0), tile.size.x - 10.0);
        }
    }

    pub fn strip_tabs(&self, area: Rect, count: usize) -> (Vec<Rect>, Rect) {
        let room = (area.size.x - 16.0 - 40.0).max(0.0);
        let width = (room / count.max(1) as f64).min(240.0).floor();
        let top = area.origin.y + 5.0;
        let tabs = (0..count).map(|at| rect(area.origin.x + 8.0 + at as f64 * width, top, width, area.max().y - top)).collect();
        let plus = rect(area.origin.x + 8.0 + count as f64 * width + 6.0, top + 1.0, 28.0, 26.0);
        (tabs, plus)
    }

    pub fn tab_close_rect(&self, tab: Rect) -> Rect {
        rect(tab.max().x - 28.0, tab.origin.y + (tab.size.y - 22.0) / 2.0, 22.0, 22.0)
    }

    fn paint_tabs(&self, area: Rect, job: &Paint, clip: Option<Rect>) -> Image {
        let model = job.model;
        let fonts = job.fonts;
        let mut canvas = Canvas::new(area.size.x.round() as u32, area.size.y.round() as u32);
        canvas.clip_to(clip);
        let local = rect(0.0, 0.0, area.size.x, area.size.y);
        canvas.round_rect(local, 0.0, self.bar, 1.0);
        canvas.round_rect(rect(0.0, local.max().y - 1.0, local.size.x, 1.0), 0.0, self.rule, 1.0);
        let dirs = model.tab_dirs();
        let (tabs, plus) = self.strip_tabs(local, dirs.len());
        let lit = |index: usize| index == model.tab || matches!(model.hover, Some(Hit::Tab(at) | Hit::TabClose(at)) if at == index);
        for (index, (dir, tab)) in dirs.iter().zip(&tabs).enumerate() {
            let current = index == model.tab;
            let face = rect(tab.origin.x + 1.0, tab.origin.y, tab.size.x - 2.0, tab.size.y + 10.0);
            match (current, lit(index)) {
                (true, _) => {
                    canvas.round_rect(Rect::new(face.origin - Vec2::new(1.0, 1.0), face.size + Vec2::new(2.0, 0.0)), 8.0, self.rule, 1.0);
                    canvas.round_rect(face, 7.0, self.back, 1.0);
                }
                (false, true) => canvas.round_rect(rect(face.origin.x, face.origin.y, face.size.x, tab.size.y - 4.0), 7.0, self.hover, 0.7),
                (false, false) => {
                    if index + 1 < tabs.len() && !lit(index + 1) {
                        canvas.round_rect(rect(tab.max().x - 0.5, tab.origin.y + 7.0, 1.0, tab.size.y - 18.0), 0.0, self.rule, 1.0);
                    }
                }
            }
            let close = self.tab_close_rect(*tab);
            let show_close = lit(index);
            let icon = rect(tab.origin.x + 12.0, tab.center().y - 8.0 - 1.0, 16.0, 16.0);
            self.icon_or_glyph(&mut canvas, job.art, folder_icon(dir, model), Family::Folder, icon);
            let left = icon.max().x + 8.0;
            let right = if show_close { close.origin.x - 4.0 } else { tab.max().x - 12.0 };
            let ink = if current { self.ink } else { self.dim };
            self.text(&mut canvas, fonts.body, &super::model::title_of(dir), ink, Vec2::new(left, icon.center().y), right - left);
            if show_close {
                let hot = model.hover == Some(Hit::TabClose(index));
                if hot {
                    canvas.round_rect(close, 5.0, self.hover, 1.0);
                }
                let c = close.center();
                let ink = if hot { self.ink } else { self.dim };
                canvas.line(c + Vec2::new(-4.0, -4.0), c + Vec2::new(4.0, 4.0), 1.6, ink, 1.0);
                canvas.line(c + Vec2::new(4.0, -4.0), c + Vec2::new(-4.0, 4.0), 1.6, ink, 1.0);
            }
        }
        if let Some(tab) = tabs.get(model.tab) {
            canvas.round_rect(rect(tab.origin.x + 1.0, local.max().y - 1.0, tab.size.x - 2.0, 1.0), 0.0, self.back, 1.0);
        }
        self.button(&mut canvas, plus, model.hover == Some(Hit::TabNew), false);
        let c = plus.center();
        canvas.line(c + Vec2::new(-5.0, 0.0), c + Vec2::new(5.0, 0.0), 1.8, self.dim, 1.0);
        canvas.line(c + Vec2::new(0.0, -5.0), c + Vec2::new(0.0, 5.0), 1.8, self.dim, 1.0);
        canvas.image
    }

    pub fn dismiss_rect(&self, banner: Rect) -> Rect {
        rect(banner.max().x - 10.0 - BUTTON, banner.origin.y + (banner.size.y - BUTTON) / 2.0, BUTTON, BUTTON)
    }

    pub fn scrolled(&self, before: Image, size: Vec2, job: &Paint, by: f64, side: bool) -> Result<Image, Image> {
        let model = job.model;
        let frame = self.frame(size, model);
        let area = match side {
            true => match frame.side {
                Some(side) => side,
                None => return Err(before),
            },
            false => match model.view.mode {
                Mode::List => rect(frame.body.origin.x, frame.body.origin.y + HEAD_H, frame.body.size.x, frame.body.size.y - HEAD_H),
                Mode::Icons => frame.body,
                _ => return Err(before),
            },
        };
        let density = crate::paint::density();
        let step = by * density;
        if by.fract() != 0.0 || step.abs() >= area.size.y * density - 1.0 || before.width != crate::paint::dense(size.x) || before.height != crate::paint::dense(size.y) {
            return Err(before);
        }
        let mut image = before;
        let width = image.width as usize;
        let x0 = (area.origin.x * density).round().max(0.0) as usize;
        let x1 = ((area.max().x * density).round() as usize).min(width);
        let y0 = (area.origin.y * density).round().max(0.0) as usize;
        let y1 = ((area.max().y * density).round() as usize).min(image.height as usize);
        let shift = step.round() as isize;
        let rows: Box<dyn Iterator<Item = usize>> = match shift > 0 {
            true => Box::new(y0..y1),
            false => Box::new((y0..y1).rev()),
        };
        for y in rows {
            let from = y as isize + shift;
            if from < y0 as isize || from >= y1 as isize {
                continue;
            }
            let (src, dst) = ((from as usize * width + x0) * 4, (y * width + x0) * 4);
            image.rgba.copy_within(src..src + (x1 - x0) * 4, dst);
        }
        let gap = by.abs();
        let open = match by > 0.0 {
            true => rect(area.origin.x, area.max().y - gap - 1.0, area.size.x, gap + 1.0),
            false => rect(area.origin.x, area.origin.y, area.size.x, gap + 1.0),
        };
        let mark = rect(area.max().x - 12.0, area.origin.y, 12.0, area.size.y);
        for clip in [open, mark] {
            image = self.repaint(image, size, &Paint { clip: Some(clip), ..*job });
        }
        Ok(image)
    }

    pub fn hover_rect(&self, size: Vec2, model: &Model, lines: &[Line], fonts: &Fonts, hit: &Hit) -> Option<Rect> {
        let frame = self.frame(size, model);
        let area = match hit {
            Hit::Bar(index) => self.bar_titles(fonts).get(*index)?.0,
            Hit::Back => self.nav_rects(frame.bar)[0],
            Hit::Forward => self.nav_rects(frame.bar)[1],
            Hit::Up => self.nav_rects(frame.bar)[2],
            Hit::Show(mode) => self.view_rects(frame.bar).into_iter().find(|(each, _)| each == mode)?.1,
            Hit::Gear => self.gear_rect(frame.bar),
            Hit::SearchClear => self.search_clear_rect(self.search_rect(frame.bar)),
            Hit::Crumb(index) => self.crumbs(self.path_rect(frame.bar), model, fonts).get(*index)?.area,
            Hit::CrumbArrow(index) => self.crumbs(self.path_rect(frame.bar), model, fonts).get(*index)?.arrow,
            Hit::Line(index) | Hit::Caret(index) | Hit::Eject(index) => *self.line_rects(frame.side?, lines, model.tree_scroll).get(*index)?,
            Hit::Head(_) => rect(frame.body.origin.x, frame.body.origin.y, frame.body.size.x, HEAD_H),
            Hit::Entry(index) | Hit::Strip(index) => self.entry_rect(&frame, model, *index)?,
            Hit::Column(index, Some(row)) => {
                let area = self.column_rect(frame.body, model, *index);
                let column = model.columns.get(*index)?;
                rect(area.origin.x + 6.0, area.origin.y + 6.0 + *row as f64 * ROW_H - column.scroll, COLUMN_W - 12.0, ROW_H)
            }
            Hit::Stop(id) => self.job_rows(frame.jobs?, model).into_iter().find(|(each, _)| each == id)?.1,
            Hit::Dismiss => self.dismiss_rect(frame.banner?),
            Hit::Tab(index) | Hit::TabClose(index) => *self.strip_tabs(frame.tabs?, model.tabs.len()).0.get(*index)?,
            Hit::TabNew => self.strip_tabs(frame.tabs?, model.tabs.len()).1,
            Hit::Rail(rail) => rail_track(self.rail_place(size, model, lines, *rail)?.0),
            Hit::Edge(_) => rect(frame.body.origin.x, frame.body.origin.y, frame.body.size.x, HEAD_H),
            _ => return None,
        };
        Some(Rect::new(area.origin - Vec2::splat(4.0), area.size + Vec2::splat(8.0)))
    }

    pub fn menu_area(&self, size: Vec2, menu: &Menu, fonts: &Fonts) -> Rect {
        let main = self.menu_rect(size, menu.at, &menu.items, fonts);
        let whole = match self.kid_rect(size, menu, fonts) {
            Some(kid) => join(main, kid),
            None => main,
        };
        Rect::new(whole.origin - Vec2::splat(24.0), whole.size + Vec2::splat(48.0))
    }

    pub fn eject_rect(&self, line: Rect) -> Rect {
        rect(line.max().x - 28.0, line.center().y - 10.0, 22.0, 20.0)
    }

    pub fn job_rows(&self, area: Rect, model: &Model) -> Vec<(u64, Rect)> {
        model.jobs.iter().filter(|job| job.shown()).enumerate().map(|(at, job)| (job.id, rect(area.origin.x, area.origin.y + at as f64 * JOB_H, area.size.x, JOB_H))).collect()
    }

    pub fn stop_rect(&self, row: Rect) -> Rect {
        rect(row.max().x - 10.0 - BUTTON, row.origin.y + (row.size.y - BUTTON) / 2.0, BUTTON, BUTTON)
    }

    fn paint_jobs(&self, canvas: &mut Canvas, area: Rect, model: &Model, fonts: &Fonts) {
        canvas.round_rect(area, 0.0, self.bar, 1.0);
        for (id, row) in self.job_rows(area, model) {
            let Some(job) = model.jobs.iter().find(|each| each.id == id) else {
                continue;
            };
            canvas.round_rect(rect(0.0, row.max().y - 1.0, row.size.x, 1.0), 0.0, self.rule, 1.0);
            let stop = self.stop_rect(row);
            if job.stoppable {
                let hot = model.hover == Some(Hit::Stop(id));
                self.button(canvas, stop, hot, false);
                let c = stop.center();
                canvas.line(c + Vec2::new(-5.0, -5.0), c + Vec2::new(5.0, 5.0), 2.0, self.ink, 1.0);
                canvas.line(c + Vec2::new(5.0, -5.0), c + Vec2::new(-5.0, 5.0), 2.0, self.ink, 1.0);
            }
            let left = row.origin.x + 16.0;
            let right = stop.origin.x - 14.0;
            let top = row.origin.y + 17.0;
            let detail = job.detail();
            let detail_w = match detail.is_empty() {
                true => 0.0,
                false => fonts.small.width_of(&detail) + 16.0,
            };
            self.text(canvas, fonts.body, &job.words, self.ink, Vec2::new(left, top), right - left - detail_w);
            if !detail.is_empty() {
                self.text_right(canvas, fonts.small, &detail, self.dim, right, top);
            }
            let track = rect(left, row.origin.y + 32.0, (right - left).max(0.0), 6.0);
            canvas.round_rect(track, 3.0, self.raised, 1.0);
            match job.progress.share() {
                Some(share) => {
                    let width = (track.size.x * share).max(6.0);
                    canvas.round_rect(rect(track.origin.x, track.origin.y, width, track.size.y), 3.0, self.accent, 1.0);
                }
                None => {
                    let sweep = (job.started.elapsed().as_secs_f64() * 0.6).fract();
                    let width = track.size.x * 0.25;
                    let x = track.origin.x + (track.size.x - width) * sweep;
                    canvas.round_rect(rect(x, track.origin.y, width, track.size.y), 3.0, self.accent, 0.7);
                }
            }
        }
    }

    fn paint_banner(&self, canvas: &mut Canvas, banner: Rect, model: &Model, fonts: &Fonts) {
        let Some(words) = model.alerts.last() else {
            return;
        };
        canvas.round_rect(banner, 0.0, mix(self.back, self.alert, if self.dark { 0.22 } else { 0.12 }), 1.0);
        canvas.round_rect(rect(0.0, banner.max().y - 1.0, banner.size.x, 1.0), 0.0, mix(self.rule, self.alert, 0.35), 1.0);
        let middle = banner.center().y;
        let sign = Vec2::new(banner.origin.x + 22.0, middle);
        canvas.circle(sign, 9.0, self.alert, 1.0);
        canvas.line(sign + Vec2::new(0.0, -4.5), sign + Vec2::new(0.0, 1.0), 2.0, self.on_accent, 1.0);
        canvas.circle(sign + Vec2::new(0.0, 4.5), 1.2, self.on_accent, 1.0);
        let close = self.dismiss_rect(banner);
        self.button(canvas, close, model.hover == Some(Hit::Dismiss), false);
        let c = close.center();
        canvas.line(c + Vec2::new(-5.0, -5.0), c + Vec2::new(5.0, 5.0), 2.0, self.ink, 1.0);
        canvas.line(c + Vec2::new(5.0, -5.0), c + Vec2::new(-5.0, 5.0), 2.0, self.ink, 1.0);
        let mut right = close.origin.x - 10.0;
        if model.alerts.len() > 1 {
            let more = crate::lang::tf("{} more", &[&(model.alerts.len() - 1)]);
            let width = fonts.small.width_of(&more);
            self.text_right(canvas, fonts.small, &more, self.dim, right, middle);
            right -= width + 16.0;
        }
        let left = sign.x + 18.0;
        self.text(canvas, fonts.body, words, self.ink, Vec2::new(left, middle), right - left);
    }

    fn paint_status(&self, canvas: &mut Canvas, status: Rect, model: &Model, fonts: &Fonts) {
        canvas.round_rect(status, 0.0, self.bar, 1.0);
        canvas.round_rect(rect(0.0, status.origin.y, status.size.x, 1.0), 0.0, self.rule, 1.0);
        let middle = status.center().y;
        let (words, ink) = match (&model.note, model.jobs.iter().rfind(|job| !job.shown())) {
            (_, Some(job)) => (format!("{}…", job.words), self.ink),
            (Some((words, _)), None) => (words.clone(), self.ink),
            (None, None) => (model.summary(), self.dim),
        };
        let free = model.free_words();
        let right_room = free.as_ref().map_or(0.0, |words| fonts.small.width_of(words) + 24.0) + if model.view.mode == Mode::Icons { ZOOM_W + 50.0 } else { 0.0 };
        self.text(canvas, fonts.small, &words, ink, Vec2::new(12.0, middle), status.size.x - right_room - 24.0);
        if let Some(free) = &free {
            self.text_right(canvas, fonts.small, free, self.dim, status.size.x - 12.0, middle);
        }
        if let Some(zoom) = self.zoom_rect(status, model, fonts) {
            let share = f64::from(model.prefs.zoom - prefs::LEAST_ZOOM) / f64::from(prefs::MOST_ZOOM - prefs::LEAST_ZOOM);
            let track = rect(zoom.origin.x, middle - 2.0, zoom.size.x, 4.0);
            canvas.round_rect(track, 2.0, self.raised, 1.0);
            canvas.round_rect(rect(track.origin.x, track.origin.y, track.size.x * share, 4.0), 2.0, self.accent, 1.0);
            let knob = Vec2::new(track.origin.x + track.size.x * share, middle);
            canvas.circle(knob + Vec2::new(0.0, 1.0), 7.0, [0, 0, 0], 0.18);
            canvas.circle(knob, 6.5, [255, 255, 255], 1.0);
            canvas.round_rect(Rect::from_center(Vec2::new(zoom.origin.x - 12.0, middle), Vec2::splat(6.0)), 1.5, self.dim, 1.0);
            canvas.round_rect(Rect::from_center(Vec2::new(zoom.max().x + 14.0, middle), Vec2::splat(11.0)), 2.5, self.dim, 1.0);
        }
    }

    pub fn menu_size(&self, items: &[Item], fonts: &Fonts) -> Vec2 {
        let widest = items.iter().filter(|item| !item.rule).map(|item| fonts.body.width_of(&item.label)).fold(0.0, f64::max);
        let keys = items.iter().map(|item| fonts.small.width_of(&key_words(item.keys))).fold(0.0, f64::max);
        let kids = items.iter().any(|item| !item.kids.is_empty() || item.act.is_none() && !item.rule);
        let height: f64 = items.iter().map(|item| if item.rule { RULE_H } else { ITEM_H }).sum::<f64>() + MENU_PAD * 2.0;
        let width = 30.0 + widest + if keys > 0.0 { keys + 32.0 } else { 0.0 } + if kids { 20.0 } else { 0.0 } + 16.0;
        Vec2::new(width.max(180.0).ceil(), height.ceil())
    }

    pub fn menu_rect(&self, size: Vec2, at: Vec2, items: &[Item], fonts: &Fonts) -> Rect {
        let want = self.menu_size(items, fonts);
        let x = at.x.min(size.x - want.x - 4.0).max(4.0);
        let y = match at.y + want.y > size.y - 4.0 {
            true => (at.y - want.y).max(4.0),
            false => at.y,
        };
        rect(x, y, want.x, want.y.min(size.y - 8.0))
    }

    pub fn kid_rect(&self, size: Vec2, menu: &Menu, fonts: &Fonts) -> Option<Rect> {
        let open = menu.open?;
        let parent = self.menu_rect(size, menu.at, &menu.items, fonts);
        let row = self.item_rects(parent, &menu.items).get(open).copied()?;
        let kids = &menu.items.get(open)?.kids;
        if kids.is_empty() {
            return None;
        }
        let want = self.menu_size(kids, fonts);
        let x = match parent.max().x + want.x - 4.0 > size.x {
            true => parent.origin.x - want.x + 4.0,
            false => parent.max().x - 4.0,
        };
        let y = (row.origin.y - MENU_PAD).min(size.y - want.y - 4.0).max(4.0);
        Some(rect(x.max(4.0), y, want.x, want.y))
    }

    pub fn item_rects(&self, area: Rect, items: &[Item]) -> Vec<Rect> {
        let mut y = area.origin.y + MENU_PAD;
        items
            .iter()
            .map(|item| {
                let tall = if item.rule { RULE_H } else { ITEM_H };
                let out = rect(area.origin.x + MENU_PAD, y, area.size.x - MENU_PAD * 2.0, tall);
                y += tall;
                out
            })
            .collect()
    }

    pub fn menu_hit(&self, size: Vec2, menu: &Menu, fonts: &Fonts, at: Vec2) -> Option<(bool, usize)> {
        if let Some(kid) = self.kid_rect(size, menu, fonts)
            && kid.contains(at)
        {
            let kids = &menu.items[menu.open?].kids;
            return self.item_rects(kid, kids).iter().position(|row| row.contains(at)).filter(|found| !kids[*found].rule).map(|found| (true, found));
        }
        let area = self.menu_rect(size, menu.at, &menu.items, fonts);
        if !area.contains(at) {
            return None;
        }
        self.item_rects(area, &menu.items).iter().position(|row| row.contains(at)).filter(|found| !menu.items[*found].rule).map(|found| (false, found))
    }

    pub fn menu_contains(&self, size: Vec2, menu: &Menu, fonts: &Fonts, at: Vec2) -> bool {
        self.menu_rect(size, menu.at, &menu.items, fonts).contains(at) || self.kid_rect(size, menu, fonts).is_some_and(|kid| kid.contains(at))
    }

    fn paint_menu(&self, canvas: &mut Canvas, size: Vec2, menu: &Menu, fonts: &Fonts) {
        let area = self.menu_rect(size, menu.at, &menu.items, fonts);
        self.paint_items(canvas, area, &menu.items, menu.hover, menu.open, fonts);
        if let (Some(kid), Some(open)) = (self.kid_rect(size, menu, fonts), menu.open) {
            self.paint_items(canvas, kid, &menu.items[open].kids, menu.kid_hover, None, fonts);
        }
    }

    fn paint_items(&self, canvas: &mut Canvas, area: Rect, items: &[Item], hover: Option<usize>, open: Option<usize>, fonts: &Fonts) {
        canvas.shadow(area, 10.0, 18.0, self.drop * 0.9);
        canvas.round_rect(area, 10.0, if self.dark { mix(self.card, [255, 255, 255], 0.04) } else { [252, 252, 253] }, 1.0);
        canvas.ring(area, 10.0, 1.0, self.rule, 0.9);
        for (index, (item, row)) in items.iter().zip(self.item_rects(area, items)).enumerate() {
            if item.rule {
                canvas.round_rect(rect(row.origin.x + 8.0, row.center().y, row.size.x - 16.0, 1.0), 0.0, self.rule, 1.0);
                continue;
            }
            let lit = item.on && (hover == Some(index) || open == Some(index));
            if lit {
                self.glossy(canvas, row, 6.0);
            }
            let ink = match (lit, item.on) {
                (true, _) => self.on_accent,
                (false, true) => self.ink,
                (false, false) => mix(self.card, self.dim, 0.6),
            };
            let middle = row.center().y;
            if item.check == Some(true) {
                let c = Vec2::new(row.origin.x + 13.0, middle);
                canvas.line(c + Vec2::new(-4.0, 0.0), c + Vec2::new(-1.0, 3.5), 1.8, ink, 1.0);
                canvas.line(c + Vec2::new(-1.0, 3.5), c + Vec2::new(5.0, -4.0), 1.8, ink, 1.0);
            }
            let keys = key_words(item.keys);
            let keys_w = if keys.is_empty() { 0.0 } else { fonts.small.width_of(&keys) + 16.0 };
            self.text(canvas, fonts.body, &item.label, ink, Vec2::new(row.origin.x + 26.0, middle), row.size.x - 36.0 - keys_w);
            if !keys.is_empty() {
                let keys_ink = if lit { self.on_accent } else { self.dim };
                self.text_right(canvas, fonts.small, &keys, keys_ink, row.max().x - 10.0, middle);
            }
            if !item.kids.is_empty() || (item.act.is_none() && !item.rule) {
                self.chevron(canvas, Vec2::new(row.max().x - 12.0, middle), ink);
            }
        }
    }

    pub fn sheet_rect(&self, size: Vec2, sheet: &Sheet) -> Rect {
        let want = match sheet {
            Sheet::Prefs(_) => Vec2::new(720.0, 520.0),
            Sheet::Info(info) => {
                let switches = info_switch_count(info);
                Vec2::new(460.0, 330.0 + 34.0 * (info_row_count(info) + switches) as f64 - if switches > 0 { 30.0 } else { 0.0 })
            }
            Sheet::Confirm { .. } => Vec2::new(420.0, 190.0),
            Sheet::Clash { names, .. } => Vec2::new(540.0, if names.len() == 1 { 190.0 } else { 236.0 }),
            Sheet::Keys => Vec2::new(520.0, 665.0),
            Sheet::Look => Vec2::new(size.x - 80.0, size.y - 80.0).min(Vec2::new(1000.0, 760.0)),
            Sheet::Names(_) => Vec2::new(560.0, 470.0),
            Sheet::Many(_) => Vec2::new(460.0, 330.0),
            Sheet::About => Vec2::new(380.0, 260.0),
            Sheet::Apps(_) => Vec2::new(480.0, 560.0),
        };
        let fitted = Vec2::new(want.x.min(size.x - 24.0), want.y.min(size.y - 24.0));
        Rect::from_center(size / 2.0, fitted).round_out()
    }

    fn sheet_buttons(&self, area: Rect, sheet: &Sheet) -> Vec<(SheetHit, Rect)> {
        let y = area.max().y - 20.0 - 32.0;
        match sheet {
            Sheet::Confirm { .. } | Sheet::Apps(_) | Sheet::Names(_) => vec![
                (SheetHit::No, rect(area.max().x - 20.0 - 120.0 * 2.0 - 10.0, y, 120.0, 32.0)),
                (SheetHit::Yes, rect(area.max().x - 20.0 - 120.0, y, 120.0, 32.0)),
            ],
            Sheet::Clash { .. } => {
                use super::ops::Clash;
                let order = [SheetHit::No, SheetHit::Clash(Clash::Skip), SheetHit::Clash(Clash::Replace), SheetHit::Clash(Clash::KeepBoth)];
                let (wide, gap) = (116.0, 8.0);
                let left = area.max().x - 20.0 - wide * 4.0 - gap * 3.0;
                order.into_iter().enumerate().map(|(at, hit)| (hit, rect(left + at as f64 * (wide + gap), y, wide, 32.0))).collect()
            }
            Sheet::Look => Vec::new(),
            _ => vec![(SheetHit::Done, rect(area.max().x - 20.0 - 100.0, y, 100.0, 32.0))],
        }
    }

    pub fn tabs(&self) -> [&'static str; 4] {
        [crate::lang::t("General"), crate::lang::t("Views"), crate::lang::t("Sidebar"), crate::lang::t("Advanced")]
    }

    fn tab_rects(&self, area: Rect) -> Vec<Rect> {
        (0..4).map(|at| rect(area.origin.x + 14.0, area.origin.y + 64.0 + at as f64 * 38.0, 160.0, 32.0)).collect()
    }

    fn pref_area(&self, area: Rect) -> Rect {
        rect(area.origin.x + 190.0, area.origin.y + 20.0, area.size.x - 210.0, area.size.y - 92.0)
    }

    pub fn pref_rows(&self, area: Rect, rows: &[Row]) -> Vec<Rect> {
        let inner = self.pref_area(area);
        let mut y = inner.origin.y + 44.0;
        let mut out = Vec::new();
        for row in rows {
            if let Row::Head(_) = row {
                y += 10.0;
                out.push(rect(inner.origin.x, y, inner.size.x, 26.0));
                y += 26.0;
                continue;
            }
            out.push(rect(inner.origin.x, y, inner.size.x, SHEET_ROW));
            y += SHEET_ROW;
        }
        out
    }

    fn switch_rect(&self, row: Rect) -> Rect {
        rect(row.max().x - 16.0 - 40.0, row.center().y - 12.0, 40.0, 24.0)
    }

    fn choice_rects(&self, row: Rect, words: &[String], fonts: &Fonts) -> Vec<Rect> {
        let widths: Vec<f64> = words.iter().map(|word| fonts.small.width_of(word) + 22.0).collect();
        let total: f64 = widths.iter().sum();
        let mut x = row.max().x - 16.0 - total;
        widths
            .iter()
            .map(|w| {
                let out = rect(x, row.center().y - 13.0, *w, 26.0);
                x += w;
                out
            })
            .collect()
    }

    fn slider_rect(&self, row: Rect) -> Rect {
        rect(row.max().x - 16.0 - 200.0, row.center().y - 10.0, 200.0, 20.0)
    }

    pub fn sheet_hit(&self, size: Vec2, model: &Model, fonts: &Fonts, at: Vec2) -> Option<SheetHit> {
        let sheet = model.sheet.as_ref()?;
        let area = self.sheet_rect(size, sheet);
        if !area.contains(at) {
            return Some(SheetHit::Outside);
        }
        if let Some((hit, _)) = self.sheet_buttons(area, sheet).into_iter().find(|(_, button)| button.contains(at)) {
            return Some(hit);
        }
        if let Sheet::Names(renamer) = sheet
            && let Some((hit, _)) = self.names_hits(area, renamer, fonts).into_iter().find(|(_, place)| place.contains(at))
        {
            return Some(hit);
        }
        if let Sheet::Info(info) = sheet {
            if self.info_name(area).contains(at) {
                return Some(SheetHit::Name);
            }
            if self.info_change(area, info).is_some_and(|button| button.contains(at)) {
                return Some(SheetHit::ChangeApp);
            }
            if let Some((hit, _)) = self.info_switches(area, info).into_iter().find(|(_, row)| row.contains(at)) {
                return Some(hit);
            }
        }
        if let Sheet::Apps(chooser) = sheet {
            if let Some((found, _)) = self.app_rows(area, chooser).into_iter().find(|(_, row)| row.contains(at)) {
                return Some(SheetHit::App(found));
            }
            if self.always_rect(area).contains(at) {
                return Some(SheetHit::Always);
            }
        }
        if let Sheet::Prefs(tab) = sheet {
            if let Some(found) = self.tab_rects(area).iter().position(|each| each.contains(at)) {
                return Some(SheetHit::Tab(found));
            }
            let rows = pref_rows(*tab, model);
            for (row, place) in rows.iter().zip(self.pref_rows(area, &rows)) {
                if !place.contains(at) {
                    continue;
                }
                match row {
                    Row::Switch { pref, .. } => return Some(SheetHit::Pref(*pref)),
                    Row::Choice { options, .. } => {
                        let words: Vec<String> = options.iter().map(|(word, _, _)| word.clone()).collect();
                        for ((_, pref, _), button) in options.iter().zip(self.choice_rects(place, &words, fonts)) {
                            if button.contains(at) {
                                return Some(SheetHit::Pref(*pref));
                            }
                        }
                    }
                    Row::Slider { .. } => {
                        let slider = self.slider_rect(place);
                        let grown = rect(slider.origin.x - 10.0, place.origin.y, slider.size.x + 20.0, place.size.y);
                        if grown.contains(at) {
                            return Some(SheetHit::Slider(((at.x - slider.origin.x) / slider.size.x).clamp(0.0, 1.0)));
                        }
                    }
                    Row::Head(_) => {}
                }
            }
        }
        Some(SheetHit::Inside)
    }

    pub fn slider_share(&self, size: Vec2, model: &Model, at: Vec2) -> Option<f64> {
        let sheet = model.sheet.as_ref()?;
        let Sheet::Prefs(tab) = sheet else {
            return None;
        };
        let area = self.sheet_rect(size, sheet);
        let rows = pref_rows(*tab, model);
        rows.iter().zip(self.pref_rows(area, &rows)).find_map(|(row, place)| match row {
            Row::Slider { .. } => {
                let slider = self.slider_rect(place);
                Some(((at.x - slider.origin.x) / slider.size.x).clamp(0.0, 1.0))
            }
            _ => None,
        })
    }

    pub fn zoom_share(&self, size: Vec2, model: &Model, fonts: &Fonts, at: Vec2) -> Option<f64> {
        let frame = self.frame(size, model);
        let zoom = self.zoom_rect(frame.status?, model, fonts)?;
        Some(((at.x - zoom.origin.x) / zoom.size.x).clamp(0.0, 1.0))
    }

    pub fn app_list(&self, area: Rect) -> Rect {
        rect(area.origin.x + 20.0, area.origin.y + 62.0, area.size.x - 40.0, area.size.y - 62.0 - 112.0)
    }

    pub fn apps_shown(&self, size: Vec2) -> usize {
        let area = self.sheet_rect(size, &Sheet::Apps(super::model::Chooser::default()));
        ((self.app_list(area).size.y - 8.0) / APP_ROW).floor().max(1.0) as usize
    }

    fn always_rect(&self, area: Rect) -> Rect {
        let list = self.app_list(area);
        rect(list.origin.x, list.max().y + 14.0, list.size.x, 24.0)
    }

    fn app_rows(&self, area: Rect, chooser: &super::model::Chooser) -> Vec<(usize, Rect)> {
        let list = self.app_list(area);
        let first = chooser.scroll.max(0.0) as usize;
        let room = ((list.size.y - 8.0) / APP_ROW).floor().max(1.0) as usize;
        (first..chooser.apps.len().min(first + room))
            .map(|at| (at, rect(list.origin.x + 4.0, list.origin.y + 4.0 + (at - first) as f64 * APP_ROW, list.size.x - 8.0, APP_ROW)))
            .collect()
    }

    fn paint_apps(&self, canvas: &mut Canvas, area: Rect, chooser: &super::model::Chooser, job: &Paint) {
        let fonts = job.fonts;
        let title = crate::lang::tf("Open “{}” with", &[&chooser.name]);
        let head = fonts.title.paint_weight(&fit(fonts.title, &title, area.size.x - 48.0), self.ink, 620.0);
        canvas.stamp(&head, Vec2::new(area.origin.x + 24.0, area.origin.y + 24.0).round(), 1.0);
        let list = self.app_list(area);
        canvas.round_rect(list, 8.0, self.back, 1.0);
        canvas.ring(list, 8.0, 1.0, self.rule, 1.0);
        for (at, row) in self.app_rows(area, chooser) {
            let app = &chooser.apps[at];
            let picked = chooser.at == Some(at);
            if picked {
                self.glossy(canvas, row, 6.0);
            }
            let icon = rect(row.origin.x + 8.0, row.center().y - 11.0, 22.0, 22.0);
            self.icon_or_glyph(canvas, job.art, app.icon.as_deref().unwrap_or("application-x-executable"), Family::Program, icon);
            let ink = if picked { self.on_accent } else { self.ink };
            self.text(canvas, fonts.body, &app.name, ink, Vec2::new(row.origin.x + 40.0, row.center().y), row.size.x - 48.0);
            if at + 1 == chooser.suggested && at + 1 < chooser.apps.len() {
                canvas.line(Vec2::new(row.origin.x + 8.0, row.max().y), Vec2::new(row.max().x - 8.0, row.max().y), 1.0, self.rule, 1.0);
            }
        }
        if chooser.apps.is_empty() {
            self.text_center(canvas, fonts.body, crate::lang::t("No apps found"), self.dim, list.center(), list.size.x - 20.0);
        }
        let always = self.always_rect(area);
        let tick = rect(always.origin.x + 2.0, always.center().y - 8.0, 16.0, 16.0);
        canvas.round_rect(tick, 4.0, if chooser.always { self.accent } else { self.raised }, 1.0);
        canvas.ring(tick, 4.0, 1.0, self.rule, 1.0);
        if chooser.always {
            let (a, b, c) = (Vec2::new(tick.origin.x + 4.0, tick.center().y), Vec2::new(tick.origin.x + 7.0, tick.max().y - 4.5), Vec2::new(tick.max().x - 3.5, tick.origin.y + 4.0));
            canvas.line(a, b, 2.0, self.on_accent, 1.0);
            canvas.line(b, c, 2.0, self.on_accent, 1.0);
        }
        let words = crate::lang::tf("Always use this app for {} files", &[&chooser.kind]);
        self.text(canvas, fonts.body, &words, self.ink, Vec2::new(tick.max().x + 10.0, always.center().y), always.size.x - 30.0);
    }

    fn names_rules(&self, area: Rect) -> [(super::model::NameRule, Rect); 3] {
        use super::model::NameRule;
        let width = (area.size.x - 48.0) / 3.0;
        let y = area.origin.y + 66.0;
        [NameRule::Replace, NameRule::Add, NameRule::Number].map(|rule| {
            let at = rule as usize as f64;
            (rule, rect(area.origin.x + 24.0 + at * width, y, width, 30.0))
        })
    }

    fn names_fields(&self, area: Rect, renamer: &super::model::Renamer) -> Vec<Rect> {
        use super::model::NameRule;
        let y = area.origin.y + 132.0;
        let full = area.size.x - 48.0 - 120.0;
        match renamer.rule {
            NameRule::Add => vec![rect(area.origin.x + 144.0, y, full, 30.0)],
            _ => vec![rect(area.origin.x + 144.0, y, full, 30.0), rect(area.origin.x + 144.0, y + 42.0, full, 30.0)],
        }
    }

    fn names_ends(&self, area: Rect, fonts: &Fonts) -> Vec<(bool, Rect)> {
        let y = area.origin.y + 132.0 + 42.0;
        let words = [crate::lang::t("Before the name"), crate::lang::t("After the name")];
        let mut x = area.origin.x + 144.0;
        words
            .iter()
            .enumerate()
            .map(|(at, word)| {
                let width = fonts.body.width_of(word) + 28.0;
                let place = rect(x, y, width, 30.0);
                x += width + 8.0;
                (at == 1, place)
            })
            .collect()
    }

    fn names_hits(&self, area: Rect, renamer: &super::model::Renamer, fonts: &Fonts) -> Vec<(SheetHit, Rect)> {
        let mut hits: Vec<(SheetHit, Rect)> = self.names_rules(area).into_iter().map(|(rule, place)| (SheetHit::NameRule(rule), place)).collect();
        hits.extend(self.names_fields(area, renamer).into_iter().enumerate().map(|(at, place)| (SheetHit::NameField(at), place)));
        if renamer.rule == super::model::NameRule::Add {
            hits.extend(self.names_ends(area, fonts).into_iter().map(|(end, place)| (SheetHit::NameEnd(end), place)));
        }
        hits
    }

    fn paint_names(&self, canvas: &mut Canvas, area: Rect, renamer: &super::model::Renamer, job: &Paint) {
        use super::model::NameRule;
        let fonts = job.fonts;
        let title = crate::lang::tf("Rename {} Items", &[&renamer.paths.len()]);
        let head = fonts.title.paint_weight(&title, self.ink, 620.0);
        canvas.stamp(&head, Vec2::new(area.origin.x + 24.0, area.origin.y + 24.0).round(), 1.0);
        let words = [crate::lang::t("Replace text"), crate::lang::t("Add text"), crate::lang::t("Name and number")];
        let rules = self.names_rules(area);
        let whole = rect(rules[0].1.origin.x, rules[0].1.origin.y, rules[2].1.max().x - rules[0].1.origin.x, 30.0);
        self.raised(canvas, whole, 7.0, false);
        for ((rule, place), word) in rules.iter().zip(words) {
            let on = renamer.rule == *rule;
            if on {
                self.pressed(canvas, inset(*place, 1.0), 6.0);
            }
            self.text_center(canvas, fonts.body, word, if on { self.on_accent } else { self.ink }, place.center(), place.size.x - 12.0);
        }
        let labels = match renamer.rule {
            NameRule::Replace => vec![crate::lang::t("Find"), crate::lang::t("Replace with")],
            NameRule::Add => vec![crate::lang::t("Text")],
            NameRule::Number => vec![crate::lang::t("Name"), crate::lang::t("Start at")],
        };
        let fields = [&renamer.first, &renamer.second];
        for (at, place) in self.names_fields(area, renamer).into_iter().enumerate() {
            self.text(canvas, fonts.body, labels[at], self.dim, Vec2::new(area.origin.x + 24.0, place.center().y), 116.0);
            let on = renamer.on == at;
            self.sunk(canvas, place, 7.0, on.then_some(2.0));
            let inner = rect(place.origin.x + 10.0, place.origin.y, place.size.x - 20.0, place.size.y);
            match on {
                true => self.paint_field(canvas, fonts.body, fields[at], inner),
                false => self.text(canvas, fonts.body, &fields[at].text, self.ink, Vec2::new(inner.origin.x, inner.center().y), inner.size.x),
            }
        }
        if renamer.rule == NameRule::Add {
            let words = [crate::lang::t("Before the name"), crate::lang::t("After the name")];
            for ((end, place), word) in self.names_ends(area, fonts).into_iter().zip(words) {
                let on = renamer.at_end == end;
                match on {
                    true => self.pressed(canvas, place, 7.0),
                    false => self.raised(canvas, place, 7.0, false),
                }
                self.text_center(canvas, fonts.body, word, if on { self.on_accent } else { self.ink }, place.center(), place.size.x - 8.0);
            }
        }
        let list = rect(area.origin.x + 24.0, area.origin.y + 226.0, area.size.x - 48.0, 150.0);
        canvas.round_rect(list, 8.0, self.well, 1.0);
        let names = renamer.new_names();
        let half = (list.size.x - 40.0) / 2.0;
        for (row, (path, name)) in names.iter().take(5).enumerate() {
            let y = list.origin.y + 18.0 + row as f64 * 26.0;
            let old = super::entry::name_of(path);
            self.text(canvas, fonts.small, &old, self.dim, Vec2::new(list.origin.x + 12.0, y), half - 12.0);
            self.text(canvas, fonts.small, "→", self.dim, Vec2::new(list.origin.x + 12.0 + half, y), 20.0);
            self.text(canvas, fonts.small, name, self.ink, Vec2::new(list.origin.x + 32.0 + half, y), half - 12.0);
        }
        if names.len() > 5 {
            let more = crate::lang::tf("and {} more", &[&(names.len() - 5)]);
            self.text(canvas, fonts.small, &more, self.dim, Vec2::new(list.origin.x + 12.0, list.max().y - 12.0), list.size.x - 24.0);
        }
        if let Some(problem) = renamer.problem() {
            self.text(canvas, fonts.small, &problem, self.dim, Vec2::new(area.origin.x + 24.0, area.max().y - 36.0), area.size.x - 300.0);
        }
    }

    fn paint_many(&self, canvas: &mut Canvas, area: Rect, group: &super::model::Group, job: &Paint) {
        let fonts = job.fonts;
        let art = Rect::from_center(Vec2::new(area.center().x, area.origin.y + 62.0), Vec2::splat(72.0));
        for (at, path) in group.paths.iter().take(3).enumerate().rev() {
            let shift = Vec2::new(at as f64 * 10.0 - 10.0, at as f64 * -6.0 + 6.0);
            if let Some(one) = Entry::at(path) {
                self.entry_art(canvas, job.art, job.model, &one, Rect::new(art.origin + shift, art.size));
            }
        }
        let title = crate::lang::tf("{} Items", &[&group.paths.len()]);
        let head = fonts.title.paint_weight(&title, self.ink, 620.0);
        canvas.stamp(&head, Vec2::new(area.center().x - logical(&head).x / 2.0, art.max().y + 14.0).round(), 1.0);
        let kinds = match (group.folders.len(), group.files) {
            (0, files) => super::entry::files_words(files),
            (folders, 0) => super::entry::folders_words(folders),
            (folders, files) => format!("{}, {}", super::entry::folders_words(folders), super::entry::files_words(files)),
        };
        let (bytes, count) = group.total();
        let size = match group.done() {
            true => crate::lang::tf("{} in {}", &[&super::entry::size_words(bytes), &super::entry::items(count as usize)]),
            false => crate::lang::tf("{}, counting…", &[&super::entry::size_words(bytes)]),
        };
        let place = group.place().map(|at| at.display().to_string()).unwrap_or_else(|| "/".to_string());
        let rows = [(crate::lang::t("Kind"), kinds), (crate::lang::t("Size"), size), (crate::lang::t("Where"), place)];
        let card = rect(area.origin.x + 20.0, art.max().y + 50.0, area.size.x - 40.0, rows.len() as f64 * 34.0 + 6.0);
        canvas.round_rect(card, 10.0, self.well, 1.0);
        let mut y = card.origin.y + 8.0;
        for (at, (label, value)) in rows.iter().enumerate() {
            if at > 0 {
                canvas.round_rect(rect(card.origin.x + 12.0, y - 5.0, card.size.x - 24.0, 1.0), 0.0, self.rule, 0.8);
            }
            self.text(canvas, fonts.small, label, self.dim, Vec2::new(card.origin.x + 14.0, y + 12.0), 90.0);
            self.text(canvas, fonts.small, value, self.ink, Vec2::new(card.origin.x + 108.0, y + 12.0), card.size.x - 122.0);
            y += 34.0;
        }
    }

    fn paint_look(&self, canvas: &mut Canvas, area: Rect, job: &Paint) {
        let model = job.model;
        let fonts = job.fonts;
        let Some(one) = model.single() else {
            return;
        };
        let head = fonts.title.paint_weight(&fit(fonts.title, &one.name, area.size.x - 48.0), self.ink, 620.0);
        canvas.stamp(&head, Vec2::new(area.center().x - logical(&head).x / 2.0, area.origin.y + 16.0).round(), 1.0);
        let detail = match one.folder {
            true => one.kind_words(),
            false => format!("{} · {} · {}", one.kind_words(), super::entry::size_words(one.size), model.date(one.modified)),
        };
        self.text_center(canvas, fonts.small, &detail, self.dim, Vec2::new(area.center().x, area.max().y - 22.0), area.size.x - 48.0);
        let room = rect(area.origin.x + 24.0, area.origin.y + 56.0, area.size.x - 48.0, area.size.y - 56.0 - 48.0);
        match (model.peek.as_ref().filter(|lines| lines.iter().any(|line| !line.trim().is_empty())), one.family().shows_picture() && model.prefs.thumbnails) {
            (Some(lines), _) => {
                let larger = Fonts { body: fonts.body, small: fonts.body, title: fonts.title };
                self.paint_peek(canvas, room, lines, &larger);
            }
            (None, true) => match job.art.thumb(&one.path, room.size.x.max(room.size.y).clamp(1.0, 1600.0)) {
                Some(image) => {
                    let fitted = grow_fit(logical(image), room.size, MOST_GROW);
                    let at = room.origin + fitted.origin;
                    if solid(image) {
                        canvas.shadow(rect(at.x, at.y, fitted.size.x, fitted.size.y), 4.0, 14.0, self.drop);
                    }
                    let shown = image.scaled(crate::paint::dense(fitted.size.x), crate::paint::dense(fitted.size.y));
                    canvas.stamp(&shown, at.round(), 1.0);
                }
                None => self.entry_art(canvas, job.art, model, one, Rect::from_center(room.center(), Vec2::splat(room.size.y.min(160.0)))),
            },
            (None, false) => self.entry_art(canvas, job.art, model, one, Rect::from_center(room.center(), Vec2::splat(room.size.y.min(160.0)))),
        }
    }

    fn paint_sheet(&self, canvas: &mut Canvas, size: Vec2, sheet: &Sheet, job: &Paint) {
        let fonts = job.fonts;
        canvas.round_rect(rect(0.0, 0.0, size.x, size.y), 0.0, [0, 0, 0], if self.dark { 0.42 } else { 0.24 });
        let area = self.sheet_rect(size, sheet);
        canvas.shadow(area, 14.0, 30.0, self.drop * 1.4);
        canvas.round_rect(area, 14.0, self.card, 1.0);
        canvas.ring(area, 14.0, 1.0, self.rule, 1.0);
        match sheet {
            Sheet::Prefs(tab) => self.paint_prefs(canvas, area, *tab, job),
            Sheet::Info(info) => self.paint_info(canvas, area, info, job),
            Sheet::Confirm { title, words, .. } => {
                let head = fonts.title.paint_weight(title, self.ink, 620.0);
                canvas.stamp(&head, Vec2::new(area.origin.x + 24.0, area.origin.y + 24.0).round(), 1.0);
                for (row, line) in self.wrap(fonts.body, words, area.size.x - 48.0, 3).iter().enumerate() {
                    self.text(canvas, fonts.body, line, self.dim, Vec2::new(area.origin.x + 24.0, area.origin.y + 70.0 + row as f64 * 20.0), area.size.x - 48.0);
                }
            }
            Sheet::Clash { into, names, .. } => {
                let place = crate::explorer::entry::name_of(into);
                let (title, list, words) = match names.as_slice() {
                    [one] => (
                        crate::lang::tf("“{}” is already in {}", &[one, &place]),
                        None,
                        crate::lang::t("Replace puts the old one in the Trash. Keep Both gives the new one another name.").to_string(),
                    ),
                    many => {
                        let shown = many.iter().take(3).map(|name| format!("“{name}”")).collect::<Vec<_>>().join(", ");
                        let list = match many.len() {
                            0..=3 => shown,
                            _ => crate::lang::tf("{} and {} more", &[&shown, &(many.len() - 3)]),
                        };
                        (
                            crate::lang::tf("{} items are already in {}", &[&many.len(), &place]),
                            Some(list),
                            crate::lang::t("Replace puts the old ones in the Trash. Keep Both gives the new ones other names. Your choice counts for all of them.").to_string(),
                        )
                    }
                };
                let head = fonts.title.paint_weight(&fit(fonts.title, &title, area.size.x - 48.0), self.ink, 620.0);
                canvas.stamp(&head, Vec2::new(area.origin.x + 24.0, area.origin.y + 24.0).round(), 1.0);
                let mut y = area.origin.y + 70.0;
                if let Some(list) = list {
                    self.text(canvas, fonts.body, &list, self.ink, Vec2::new(area.origin.x + 24.0, y), area.size.x - 48.0);
                    y += 28.0;
                }
                for (row, line) in self.wrap(fonts.body, &words, area.size.x - 48.0, 3).iter().enumerate() {
                    self.text(canvas, fonts.body, line, self.dim, Vec2::new(area.origin.x + 24.0, y + row as f64 * 20.0), area.size.x - 48.0);
                }
            }
            Sheet::Keys => self.paint_keys(canvas, area, fonts),
            Sheet::Look => self.paint_look(canvas, area, job),
            Sheet::Names(renamer) => self.paint_names(canvas, area, renamer, job),
            Sheet::Many(group) => self.paint_many(canvas, area, group, job),
            Sheet::Apps(chooser) => self.paint_apps(canvas, area, chooser, job),
            Sheet::About => {
                let c = Vec2::new(area.center().x, area.origin.y + 70.0);
                self.glyph(canvas, Family::Folder, Rect::from_center(c, Vec2::splat(72.0)));
                let name = fonts.title.paint_weight("Drifter Explorer", self.ink, 650.0);
                canvas.stamp(&name, Vec2::new(c.x - logical(&name).x / 2.0, c.y + 48.0).round(), 1.0);
                self.text_center(canvas, fonts.small, &crate::lang::tf("Version {}", &[&env!("CARGO_PKG_VERSION")]), self.dim, Vec2::new(c.x, c.y + 88.0), area.size.x - 40.0);
                self.text_center(canvas, fonts.small, crate::lang::t("Files and folders, made for Drifter."), self.dim, Vec2::new(c.x, c.y + 108.0), area.size.x - 40.0);
            }
        }
        for (hit, button) in self.sheet_buttons(area, sheet) {
            let (words, strong, danger) = match (&hit, sheet) {
                (SheetHit::Yes, Sheet::Confirm { yes, .. }) => (yes.clone(), true, yes == crate::lang::t("Delete") || yes == crate::lang::t("Empty Trash")),
                (SheetHit::Yes, Sheet::Apps(_)) => (crate::lang::t("Open").to_string(), true, false),
                (SheetHit::Yes, Sheet::Names(_)) => (crate::lang::t("Rename").to_string(), true, false),
                (SheetHit::No, _) => (crate::lang::t("Cancel").to_string(), false, false),
                (SheetHit::Clash(super::ops::Clash::Skip), _) => (crate::lang::t("Skip").to_string(), false, false),
                (SheetHit::Clash(super::ops::Clash::Replace), _) => (crate::lang::t("Replace").to_string(), false, false),
                (SheetHit::Clash(super::ops::Clash::KeepBoth), _) => (crate::lang::t("Keep Both").to_string(), true, false),
                _ => (crate::lang::t("Done").to_string(), true, false),
            };
            let fill = match (strong, danger) {
                (true, true) => self.alert,
                (true, false) => self.accent,
                _ => self.raised,
            };
            canvas.round_rect(button, 8.0, fill, 1.0);
            let ink = if strong { self.on_accent } else { self.ink };
            let label = fonts.body.paint_weight(&words, ink, 560.0);
            canvas.stamp(&label, (button.center() - logical(&label) / 2.0).round(), 1.0);
        }
    }

    fn paint_prefs(&self, canvas: &mut Canvas, area: Rect, tab: usize, job: &Paint) {
        let fonts = job.fonts;
        let model = job.model;
        let side = rect(area.origin.x, area.origin.y, 188.0, area.size.y);
        canvas.round_rect(side, 14.0, self.side, 1.0);
        canvas.round_rect(rect(side.max().x - 14.0, side.origin.y, 14.0, side.size.y), 0.0, self.side, 1.0);
        canvas.round_rect(rect(side.max().x, side.origin.y, 1.0, side.size.y), 0.0, self.rule, 1.0);
        let title = fonts.title.paint_weight(crate::lang::t("Settings"), self.ink, 650.0);
        canvas.stamp(&title, Vec2::new(area.origin.x + 24.0, area.origin.y + 22.0).round(), 1.0);
        let tints: [[u8; 3]; 4] = [[64, 128, 238], [52, 178, 120], [255, 149, 0], [140, 110, 230]];
        for (at, (place, name)) in self.tab_rects(area).into_iter().zip(self.tabs()).enumerate() {
            let on = at == tab;
            if on {
                self.glossy(canvas, place, 8.0);
            }
            let dot = rect(place.origin.x + 8.0, place.center().y - 10.0, 20.0, 20.0);
            canvas.round_rect(dot, 6.0, tints[at], 1.0);
            self.tab_glyph(canvas, at, dot.center());
            let ink = if on { self.on_accent } else { self.ink };
            self.text(canvas, fonts.body, name, ink, Vec2::new(place.origin.x + 38.0, place.center().y), place.size.x - 44.0);
        }
        let inner = self.pref_area(area);
        let head = fonts.title.paint_weight(self.tabs()[tab.min(3)], self.ink, 620.0);
        canvas.stamp(&head, Vec2::new(inner.origin.x, inner.origin.y + 4.0).round(), 1.0);
        let rows = pref_rows(tab, model);
        let places = self.pref_rows(area, &rows);
        let mut group_top: Option<f64> = None;
        let group_bottom = |at: usize| -> Option<f64> {
            match rows.get(at + 1) {
                Some(Row::Head(_)) | None => Some(places[at].max().y),
                _ => None,
            }
        };
        for (at, (row, place)) in rows.iter().zip(places.iter()).enumerate() {
            if let Row::Head(words) = row {
                let piece = fonts.small.paint_weight(&words.to_uppercase(), self.dim, 600.0);
                canvas.stamp(&piece, Vec2::new(place.origin.x + 4.0, place.max().y - 6.0 - logical(&piece).y).round(), 1.0);
                group_top = None;
                continue;
            }
            let top = *group_top.get_or_insert(place.origin.y);
            if let Some(bottom) = group_bottom(at) {
                canvas.round_rect(rect(place.origin.x, top, place.size.x, bottom - top), 10.0, self.well, 1.0);
            }
        }
        for (at, (row, place)) in rows.iter().zip(places.iter()).enumerate() {
            if matches!(row, Row::Head(_)) {
                continue;
            }
            if at > 0 && !matches!(rows[at - 1], Row::Head(_)) {
                canvas.round_rect(rect(place.origin.x + 14.0, place.origin.y, place.size.x - 28.0, 1.0), 0.0, self.rule, 0.8);
            }
            let (label, detail) = row.words();
            let middle = place.center().y;
            match detail {
                Some(detail) => {
                    self.text(canvas, fonts.body, label, self.ink, Vec2::new(place.origin.x + 16.0, middle - 8.0), place.size.x * 0.5);
                    self.text(canvas, fonts.small, detail, self.dim, Vec2::new(place.origin.x + 16.0, middle + 9.0), place.size.x * 0.55);
                }
                None => self.text(canvas, fonts.body, label, self.ink, Vec2::new(place.origin.x + 16.0, middle), place.size.x * 0.5),
            }
            match row {
                Row::Switch { on, .. } => {
                    let switch = self.switch_rect(*place);
                    canvas.round_rect(switch, 12.0, if *on { self.accent } else { self.raised }, 1.0);
                    let knob = Vec2::new(if *on { switch.max().x - 12.0 } else { switch.origin.x + 12.0 }, switch.center().y);
                    canvas.circle(knob + Vec2::new(0.0, 1.0), 10.0, [0, 0, 0], 0.15);
                    canvas.circle(knob, 9.5, [255, 255, 255], 1.0);
                }
                Row::Choice { options, .. } => {
                    let words: Vec<String> = options.iter().map(|(word, _, _)| word.clone()).collect();
                    let rects = self.choice_rects(*place, &words, fonts);
                    if let (Some(first), Some(last)) = (rects.first(), rects.last()) {
                        canvas.round_rect(rect(first.origin.x, first.origin.y, last.max().x - first.origin.x, first.size.y), 7.0, self.raised, 1.0);
                    }
                    for ((word, _, on), button) in options.iter().zip(rects) {
                        if *on {
                            let inner = rect(button.origin.x + 2.0, button.origin.y + 2.0, button.size.x - 4.0, button.size.y - 4.0);
                            canvas.shadow(inner, 6.0, 3.0, self.drop * 0.4);
                            canvas.round_rect(inner, 6.0, if self.dark { self.hover } else { [255, 255, 255] }, 1.0);
                        }
                        let piece = fonts.small.paint_weight(word, self.ink, if *on { 600.0 } else { 450.0 });
                        canvas.stamp(&piece, (button.center() - logical(&piece) / 2.0).round(), 1.0);
                    }
                }
                Row::Slider { share, words, .. } => {
                    let slider = self.slider_rect(*place);
                    let track = rect(slider.origin.x, slider.center().y - 2.0, slider.size.x, 4.0);
                    canvas.round_rect(track, 2.0, self.raised, 1.0);
                    canvas.round_rect(rect(track.origin.x, track.origin.y, track.size.x * share, 4.0), 2.0, self.accent, 1.0);
                    let knob = Vec2::new(track.origin.x + track.size.x * share, track.center().y);
                    canvas.circle(knob + Vec2::new(0.0, 1.0), 9.5, [0, 0, 0], 0.18);
                    canvas.circle(knob, 9.0, [255, 255, 255], 1.0);
                    self.text_right(canvas, fonts.small, words, self.dim, slider.origin.x - 14.0, middle);
                }
                Row::Head(_) => {}
            }
        }
    }

    fn tab_glyph(&self, canvas: &mut Canvas, tab: usize, c: Vec2) {
        let white = [255, 255, 255];
        match tab {
            0 => {
                canvas.ring(Rect::from_center(c, Vec2::splat(11.0)), 5.5, 1.6, white, 1.0);
                canvas.circle(c, 1.8, white, 1.0);
            }
            1 => self.view_glyph(canvas, Mode::Icons, c, white),
            2 => {
                canvas.ring(Rect::from_center(c, Vec2::new(12.0, 10.0)), 2.0, 1.4, white, 1.0);
                canvas.line(c + Vec2::new(-1.5, -5.0), c + Vec2::new(-1.5, 5.0), 1.4, white, 1.0);
            }
            _ => {
                for (row, knob) in [(-3.5, 2.0), (0.0, -2.0), (3.5, 1.0)] {
                    canvas.line(c + Vec2::new(-5.0, row), c + Vec2::new(5.0, row), 1.2, white, 1.0);
                    canvas.circle(c + Vec2::new(knob, row), 1.6, white, 1.0);
                }
            }
        }
    }

    fn paint_info(&self, canvas: &mut Canvas, area: Rect, info: &super::model::Info, job: &Paint) {
        let fonts = job.fonts;
        let model = job.model;
        let one = &info.entry;
        let art = Rect::from_center(Vec2::new(area.center().x, area.origin.y + 76.0), Vec2::splat(96.0));
        self.entry_art(canvas, job.art, model, one, art);
        match &info.naming {
            Some(field) => {
                let place = self.info_name(area);
                self.sunk(canvas, place, 7.0, Some(2.0));
                self.paint_field(canvas, fonts.body, field, rect(place.origin.x + 10.0, place.origin.y, place.size.x - 20.0, place.size.y));
            }
            None => {
                let name = fonts.title.paint_weight(&fit(fonts.title, &one.name, area.size.x - 48.0), self.ink, 620.0);
                canvas.stamp(&name, Vec2::new(area.center().x - logical(&name).x / 2.0, art.max().y + 14.0).round(), 1.0);
            }
        }
        let size = match (one.folder, info.total, info.inside) {
            (true, Some((bytes, count)), _) => crate::lang::tf("{} in {}", &[&super::entry::size_words(bytes), &super::entry::items(count as usize)]),
            (true, None, Some(count)) => crate::lang::tf("{}, counting…", &[&super::entry::items(count)]),
            (true, None, None) => "--".to_string(),
            (false, _, _) => format!("{} ({} {})", super::entry::size_words(one.size), one.size, crate::lang::t("bytes")),
        };
        let mut rows = vec![
            (crate::lang::t("Kind"), one.kind_words()),
            (crate::lang::t("Size"), size),
            (crate::lang::t("Where"), one.path.parent().map(|p| p.display().to_string()).unwrap_or_else(|| "/".to_string())),
            (crate::lang::t("Modified"), entry_date(one, model)),
        ];
        if let Some(mode) = info.mode {
            rows.push((crate::lang::t("Access"), super::model::mode_words(mode)));
        }
        if let Some(target) = &info.target {
            rows.push((crate::lang::t("Points to"), target.display().to_string()));
        }
        if let Some(app) = &info.opens {
            rows.push((crate::lang::t("Opens with"), app.clone()));
        }
        let card = self.info_card(area, info);
        let mut y = card.origin.y + 8.0;
        canvas.round_rect(card, 10.0, self.well, 1.0);
        for (at, (label, value)) in rows.iter().enumerate() {
            if at > 0 {
                canvas.round_rect(rect(card.origin.x + 12.0, y - 5.0, card.size.x - 24.0, 1.0), 0.0, self.rule, 0.8);
            }
            self.text(canvas, fonts.small, label, self.dim, Vec2::new(card.origin.x + 14.0, y + 12.0), 90.0);
            self.text(canvas, fonts.small, value, self.ink, Vec2::new(card.origin.x + 108.0, y + 12.0), card.size.x - 122.0 - if at + 1 == rows.len() && info.opens.is_some() { 90.0 } else { 0.0 });
            y += 34.0;
        }
        if let Some(button) = self.info_change(area, info) {
            canvas.round_rect(button, 6.0, self.raised, 1.0);
            self.text_center(canvas, fonts.small, crate::lang::t("Change…"), self.ink, button.center(), button.size.x - 8.0);
        }
        for (hit, row) in self.info_switches(area, info) {
            let (label, on) = match hit {
                SheetHit::Lock => (crate::lang::t("Locked"), info.locked().unwrap_or(false)),
                _ => (crate::lang::t("Can run as a program"), info.runs().unwrap_or(false)),
            };
            self.text(canvas, fonts.body, label, self.ink, Vec2::new(row.origin.x + 14.0, row.center().y), row.size.x - 80.0);
            let switch = rect(row.max().x - 14.0 - 44.0, row.center().y - 12.0, 44.0, 24.0);
            canvas.round_rect(switch, 12.0, if on { self.accent } else { self.raised }, 1.0);
            let knob = Vec2::new(if on { switch.max().x - 12.0 } else { switch.origin.x + 12.0 }, switch.center().y);
            canvas.circle(knob + Vec2::new(0.0, 1.0), 10.0, [0, 0, 0], 0.15);
            canvas.circle(knob, 9.5, [255, 255, 255], 1.0);
        }
    }

    pub fn info_name(&self, area: Rect) -> Rect {
        rect(area.origin.x + 40.0, area.origin.y + 134.0, area.size.x - 80.0, 30.0)
    }

    fn info_card(&self, area: Rect, info: &super::model::Info) -> Rect {
        let top = area.origin.y + 124.0 + 50.0;
        rect(area.origin.x + 20.0, top, area.size.x - 40.0, info_row_count(info) as f64 * 34.0 + 6.0)
    }

    fn info_change(&self, area: Rect, info: &super::model::Info) -> Option<Rect> {
        info.opens.as_ref()?;
        let card = self.info_card(area, info);
        let row = card.origin.y + 8.0 + (info_row_count(info) - 1) as f64 * 34.0;
        Some(rect(card.max().x - 14.0 - 84.0, row, 84.0, 24.0))
    }

    fn info_switches(&self, area: Rect, info: &super::model::Info) -> Vec<(SheetHit, Rect)> {
        let card = self.info_card(area, info);
        let hits = [(SheetHit::Lock, info.locked().is_some()), (SheetHit::Runs, info.runs().is_some())];
        hits.into_iter()
            .filter(|(_, shown)| *shown)
            .enumerate()
            .map(|(at, (hit, _))| (hit, rect(card.origin.x, card.max().y + 10.0 + at as f64 * 34.0, card.size.x, 34.0)))
            .collect()
    }

    fn paint_keys(&self, canvas: &mut Canvas, area: Rect, fonts: &Fonts) {
        let head = fonts.title.paint_weight(crate::lang::t("Keyboard Shortcuts"), self.ink, 620.0);
        canvas.stamp(&head, Vec2::new(area.origin.x + 24.0, area.origin.y + 22.0).round(), 1.0);
        let rows: [(&str, &str); 21] = [
            ("Enter", crate::lang::t("Open")),
            ("Alt+Up / Backspace", crate::lang::t("Enclosing Folder")),
            ("Alt+Left / Alt+Right", crate::lang::t("Back and forward")),
            ("Ctrl+L", crate::lang::t("Type a path")),
            ("Ctrl+F", crate::lang::t("Search")),
            ("Tab", crate::lang::t("Move between the sidebar and the files")),
            ("Ctrl+T / Ctrl+W", crate::lang::t("New tab and close tab")),
            ("Ctrl+Tab", crate::lang::t("Next tab")),
            ("F2", crate::lang::t("Rename")),
            ("Delete", crate::lang::t("Move to Trash")),
            ("Shift+Delete", crate::lang::t("Delete Forever")),
            ("Ctrl+C / X / V", crate::lang::t("Copy, cut and paste")),
            ("Ctrl+D", crate::lang::t("Duplicate")),
            ("Ctrl+Z / Shift+Ctrl+Z", crate::lang::t("Undo and redo")),
            ("Shift+Ctrl+N", crate::lang::t("New Folder")),
            ("Ctrl+1–4", crate::lang::t("Change the view")),
            ("Ctrl++ / Ctrl+-", crate::lang::t("Zoom")),
            ("Ctrl+H", crate::lang::t("Show Hidden Files")),
            ("Space", crate::lang::t("Quick Look")),
            ("Ctrl+I", crate::lang::t("Get Info")),
            ("Ctrl+,", crate::lang::t("Settings")),
        ];
        let mut y = area.origin.y + 70.0;
        for (keys, words) in rows {
            let keys = key_words(keys);
            let chip = fonts.small.paint_weight(&keys, self.ink, 560.0);
            let size = logical(&chip);
            let pill = rect(area.origin.x + 24.0, y - 10.0, size.x + 14.0, 20.0);
            canvas.round_rect(pill, 5.0, self.raised, 1.0);
            canvas.stamp(&chip, Vec2::new(pill.origin.x + 7.0, y - size.y / 2.0).round(), 1.0);
            self.text(canvas, fonts.body, words, self.ink, Vec2::new(area.origin.x + 210.0, y), area.size.x - 234.0);
            y += 25.0;
            if y > area.max().y - 70.0 {
                break;
            }
        }
    }
}

fn entry_date(one: &Entry, model: &Model) -> String {
    super::entry::date_words(one.modified, model.now, true)
}

const MOST_GROW: f64 = 4.0;

fn pale(image: &Image) -> bool {
    let (mut light, mut seen) = (0u64, 0u64);
    for pixel in image.rgba.as_chunks::<4>().0.iter().filter(|pixel| pixel[3] > 128) {
        seen += 1;
        light += u64::from(pixel[0].min(pixel[1]).min(pixel[2]) > 215);
    }
    seen > 0 && light * 10 >= seen * 6
}

fn silhouette(image: &Image) -> Image {
    let rgba = image.rgba.as_chunks::<4>().0.iter().flat_map(|pixel| [0, 0, 0, pixel[3]]).collect();
    Image { width: image.width, height: image.height, rgba }
}

fn solid(image: &Image) -> bool {
    image.rgba.as_chunks::<4>().0.iter().all(|pixel| pixel[3] >= 250)
}

fn grow_fit(content: Vec2, into: Vec2, most: f64) -> Rect {
    let fitted = Rect::fit_centered(content, into);
    match fitted.size.x > content.x * most {
        true => {
            let size = content * most;
            Rect::new((into - size) / 2.0, size)
        }
        false => fitted,
    }
}

fn shrink_fit(content: Vec2, into: Vec2) -> Rect {
    let fitted = Rect::fit_centered(content, into);
    match fitted.size.x > content.x {
        true => Rect::new((into - content) / 2.0, content),
        false => fitted,
    }
}

pub fn folder_icon(path: &Path, model: &Model) -> &'static str {
    if path == model.home {
        return "user-home";
    }
    if path == super::ops::trash_files() {
        return "user-trash";
    }
    model.places().into_iter().find(|(each, _, _)| each == path).map_or("folder", |(_, _, icon)| icon)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Head(&'static str),
    Switch { label: &'static str, detail: Option<&'static str>, pref: super::model::Pref, on: bool },
    Choice { label: &'static str, detail: Option<&'static str>, options: Vec<(String, super::model::Pref, bool)> },
    Slider { label: &'static str, detail: Option<&'static str>, share: f64, words: String },
}

impl Row {
    fn words(&self) -> (&'static str, Option<&'static str>) {
        match self {
            Row::Head(words) => (words, None),
            Row::Switch { label, detail, .. } | Row::Choice { label, detail, .. } | Row::Slider { label, detail, .. } => (label, *detail),
        }
    }
}

pub fn pref_rows(tab: usize, model: &Model) -> Vec<Row> {
    use super::model::Pref;
    use super::prefs::{Click, Dates, Root, Start};
    let p = &model.prefs;
    let switch = |label: &'static str, detail: Option<&'static str>, pref: Pref, on: bool| Row::Switch { label, detail, pref, on };
    match tab {
        0 => vec![
            Row::Head(crate::lang::t("Opening")),
            Row::Choice {
                label: crate::lang::t("New windows open"),
                detail: None,
                options: vec![(crate::lang::t("Home").to_string(), Pref::Start(Start::Home), p.start == Start::Home), (crate::lang::t("Last folder").to_string(), Pref::Start(Start::Last), p.start == Start::Last)],
            },
            Row::Choice {
                label: crate::lang::t("Open items with"),
                detail: Some(crate::lang::t("A single click opens at once")),
                options: vec![(crate::lang::t("Double click").to_string(), Pref::Click(Click::Double), p.click == Click::Double), (crate::lang::t("Single click").to_string(), Pref::Click(Click::Single), p.click == Click::Single)],
            },
            Row::Head(crate::lang::t("Files")),
            switch(crate::lang::t("Show hidden files"), Some(crate::lang::t("Names that start with a dot")), Pref::Hidden, p.hidden),
            switch(crate::lang::t("Ask before Move to Trash"), None, Pref::ConfirmTrash, p.confirm_trash),
        ],
        1 => vec![
            Row::Head(crate::lang::t("Default view")),
            Row::Choice {
                label: crate::lang::t("Show folders as"),
                detail: None,
                options: Mode::ALL.iter().map(|mode| (mode.name().to_string(), Pref::Mode(*mode), p.mode == *mode)).collect(),
            },
            Row::Slider {
                label: crate::lang::t("Icon size"),
                detail: None,
                share: f64::from(p.zoom - prefs::LEAST_ZOOM) / f64::from(prefs::MOST_ZOOM - prefs::LEAST_ZOOM),
                words: format!("{} px", p.zoom),
            },
            switch(crate::lang::t("Show previews"), Some(crate::lang::t("Pictures, videos and PDFs show what is inside")), Pref::Thumbnails, p.thumbnails),
            Row::Head(crate::lang::t("Order")),
            Row::Choice {
                label: crate::lang::t("Sort by"),
                detail: None,
                options: Sort::ALL.iter().map(|sort| (sort.name().to_string(), Pref::Sort(*sort), p.sort == *sort)).collect(),
            },
            switch(crate::lang::t("Folders first"), None, Pref::FoldersFirst, p.folders_first),
            switch(crate::lang::t("Reverse order"), None, Pref::Down, p.down),
        ],
        2 => vec![
            Row::Head(crate::lang::t("Sidebar")),
            switch(crate::lang::t("Places"), Some(crate::lang::t("Home, Documents, Downloads and more")), Pref::Places, p.places),
            switch(crate::lang::t("Folder tree"), None, Pref::Tree, p.tree),
            Row::Choice {
                label: crate::lang::t("Tree starts at"),
                detail: None,
                options: vec![(crate::lang::t("Home").to_string(), Pref::Root(Root::Home), p.tree_root == Root::Home), (crate::lang::t("Computer").to_string(), Pref::Root(Root::Computer), p.tree_root == Root::Computer)],
            },
            switch(crate::lang::t("Recent"), Some(crate::lang::t("The files you opened last")), Pref::Recent, p.recent),
            switch(crate::lang::t("Drives"), Some(crate::lang::t("This computer and plugged in disks")), Pref::Drives, p.drives),
            Row::Head(crate::lang::t("Window")),
            switch(crate::lang::t("Status bar"), Some(crate::lang::t("Item count and free space")), Pref::Status, p.status),
        ],
        _ => {
            let mut rows = vec![
                Row::Head(crate::lang::t("List columns")),
                switch(crate::lang::t("Modified"), None, Pref::ShowModified, p.show_modified),
                switch(crate::lang::t("Size"), None, Pref::ShowSize, p.show_size),
                switch(crate::lang::t("Kind"), None, Pref::ShowKind, p.show_kind),
                Row::Choice {
                    label: crate::lang::t("Dates"),
                    detail: None,
                    options: vec![(crate::lang::t("Friendly").to_string(), Pref::Dates(Dates::Friendly), p.dates == Dates::Friendly), (crate::lang::t("Exact").to_string(), Pref::Dates(Dates::Exact), p.dates == Dates::Exact)],
                },
            ];
            if !model.terminals.is_empty() {
                rows.push(Row::Head(crate::lang::t("Terminal")));
                let chosen = p.terminal.clone().or_else(|| model.terminals.first().map(|(program, _)| program.to_string()));
                rows.push(Row::Choice {
                    label: crate::lang::t("Open in Terminal uses"),
                    detail: None,
                    options: model.terminals.iter().take(3).enumerate().map(|(at, (program, name))| (name.to_string(), Pref::Terminal(at), chosen.as_deref() == Some(*program))).collect(),
                });
            }
            rows
        }
    }
}

#[cfg(test)]
mod repaint_tests {
    use super::*;

    fn same(a: &Image, b: &Image) -> bool {
        a.width == b.width && a.height == b.height && a.rgba == b.rgba
    }

    #[test]
    fn a_hover_repaint_matches_a_full_paint() {
        let dir = std::env::temp_dir().join(format!("drifter-repaint-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("work/inner")).unwrap();
        for n in 0..60 {
            std::fs::write(dir.join(format!("work/file {n}.txt")), "x").unwrap();
        }
        let mut model = Model::new(dir.join("explorer.toml"), Some(dir.join("work")));
        model.settle_all(std::time::Duration::from_secs(5));
        let (Ok(body), Ok(small), Ok(title)) = (crate::Font::find(None, 13.0), crate::Font::find(None, 11.5), crate::Font::find(None, 17.0)) else {
            return;
        };
        let fonts = Fonts { body: &body, small: &small, title: &title };
        let art = Art::default();
        let look = Look::themed(&crate::theme::DARK, [64, 128, 238]);
        let size = Vec2::new(900.0, 600.0);
        for density in [1, 2] {
            crate::paint::set_density(density);
            for mode in [Mode::List, Mode::Icons] {
                model.view.mode = mode;
                let pairs = [(None, Some(Hit::Entry(3))), (Some(Hit::Entry(3)), Some(Hit::Entry(4))), (Some(Hit::Entry(4)), Some(Hit::Line(2))), (Some(Hit::Line(2)), Some(Hit::Head(Sort::Name))), (Some(Hit::Head(Sort::Name)), Some(Hit::Gear)), (Some(Hit::Gear), Some(Hit::Crumb(0))), (Some(Hit::Crumb(0)), Some(Hit::CrumbArrow(1))), (Some(Hit::CrumbArrow(1)), None)];
                for (was, now) in pairs {
                    model.hover = was.clone();
                    let lines = model.lines();
                    let before = look.paint(size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None });
                    let mut area: Option<Rect> = None;
                    for hit in [&was, &now].into_iter().flatten() {
                        let found = look.hover_rect(size, &model, &lines, &fonts, hit).expect("a hover rect");
                        area = Some(area.map_or(found, |had| join(had, found)));
                    }
                    model.hover = now.clone();
                    let part = look.repaint(before, size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: area });
                    let whole = look.paint(size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None });
                    assert!(same(&part, &whole), "{mode:?} at {density}x: {was:?} -> {now:?}");
                }
            }
        }
        model.view.mode = Mode::List;
        model.new_tab(dir.join("work/inner"), false);
        model.new_tab(dir.join("work/inner"), false);
        for density in [1, 2] {
            crate::paint::set_density(density);
            let pairs = [(None, Some(Hit::Tab(1))), (Some(Hit::Tab(1)), Some(Hit::TabClose(1))), (Some(Hit::TabClose(1)), Some(Hit::Tab(2))), (Some(Hit::Tab(2)), Some(Hit::TabNew)), (Some(Hit::TabNew), Some(Hit::Tab(0))), (Some(Hit::Tab(0)), Some(Hit::Entry(2))), (Some(Hit::Entry(2)), None)];
            for (was, now) in pairs {
                model.hover = was.clone();
                let lines = model.lines();
                let before = look.paint(size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None });
                let mut area: Option<Rect> = None;
                for hit in [&was, &now].into_iter().flatten() {
                    let found = look.hover_rect(size, &model, &lines, &fonts, hit).expect("a hover rect");
                    area = Some(area.map_or(found, |had| join(had, found)));
                }
                model.hover = now.clone();
                let part = look.repaint(before, size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: area });
                let whole = look.paint(size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None });
                assert!(same(&part, &whole), "tabs at {density}x: {was:?} -> {now:?}");
            }
        }
        for density in [1, 2] {
            crate::paint::set_density(density);
            for hit in [Hit::Back, Hit::Gear, Hit::TabClose(1), Hit::Tab(2), Hit::Show(Mode::Icons)] {
                model.hover = Some(hit.clone());
                model.tip = None;
                let lines = model.lines();
                let plain = look.paint(size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None });
                model.tip = Some(hit.clone());
                let (card, _, _) = look.tip(size, &model, &lines, &fonts).expect("a tip");
                assert!(card.origin.x >= 6.0 && card.max().x <= size.x - 6.0, "{hit:?} tip leaves the window");
                let area = look.tip_area(card);
                let shown = look.repaint(plain.clone(), size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: Some(area) });
                let whole = look.paint(size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None });
                assert!(same(&shown, &whole), "{hit:?} tip at {density}x");
                assert!(!same(&shown, &plain), "{hit:?} tip shows");
                model.tip = None;
                let hidden = look.repaint(shown, size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: Some(area) });
                assert!(same(&hidden, &plain), "{hit:?} tip hides at {density}x");
            }
        }
        model.hover = None;
        model.act(super::super::model::Act::CloseOtherTabs(0));
        let mut blits = 0;
        for density in [1, 2] {
            crate::paint::set_density(density);
            for (mode, side) in [(Mode::List, false), (Mode::Icons, false), (Mode::List, true)] {
                model.view.mode = mode;
                model.hover = None;
                for (from, to) in [(0.0, 26.0), (26.0, 13.0), (13.0, 200.0), (200.0, 0.0)] {
                    let set = |model: &mut Model, at: f64| match side {
                        true => model.tree_scroll = at,
                        false => model.scroll = at,
                    };
                    set(&mut model, from);
                    let lines = model.lines();
                    let before = look.paint(size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None });
                    set(&mut model, to);
                    let whole = look.paint(size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None });
                    if let Ok(part) = look.scrolled(before, size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None }, to - from, side) {
                        let first = part.rgba.iter().zip(&whole.rgba).position(|(a, b)| a != b).map(|at| (at / 4 % part.width as usize, at / 4 / part.width as usize));
                        let count = part.rgba.chunks(4).zip(whole.rgba.chunks(4)).filter(|(a, b)| a != b).count();
                        assert!(same(&part, &whole), "{mode:?} side {side} at {density}x: {from} -> {to}: first {first:?}, {count} pixels");
                        blits += 1;
                    }
                }
                model.scroll = 0.0;
                model.tree_scroll = 0.0;
            }
        }
        assert!(blits >= 18, "only {blits} blits ran");
        crate::paint::set_density(1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_scrollbar_drags_and_pages() {
        use super::super::input::{self, Button, Mods, Ui};
        let dir = std::env::temp_dir().join(format!("drifter-rail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("work")).unwrap();
        for n in 0..80 {
            std::fs::write(dir.join(format!("work/file {n}.txt")), "x").unwrap();
        }
        let mut model = Model::new(dir.join("explorer.toml"), Some(dir.join("work")));
        model.settle_all(std::time::Duration::from_secs(5));
        let (Ok(body), Ok(small), Ok(title)) = (crate::Font::find(None, 13.0), crate::Font::find(None, 11.5), crate::Font::find(None, 17.0)) else {
            return;
        };
        let fonts = Fonts { body: &body, small: &small, title: &title };
        let look = Look::themed(&crate::theme::DARK, [64, 128, 238]);
        let size = Vec2::new(900.0, 600.0);
        crate::paint::set_density(1);
        let ui = Ui { look: &look, fonts: &fonts, size };
        let lines = model.lines();
        let (area, content, _) = look.rail_place(size, &model, &lines, Rail::Body).expect("a rail");
        let most = content - area.size.y;
        let grab = rail_thumb(area, content, model.scroll).center();
        assert_eq!(look.hit(size, &model, &lines, &fonts, grab), Some(Hit::Rail(Rail::Body)));
        assert_ne!(look.hit(size, &model, &lines, &fonts, grab - Vec2::new(20.0, 0.0)), Some(Hit::Rail(Rail::Body)));
        input::press(&mut model, &ui, &lines, grab, Button::Left, 1, Mods::default());
        assert!(rail_hot(&model, Rail::Body));
        assert!(input::motion(&mut model, &ui, &lines, Vec2::new(grab.x, area.max().y + 80.0)));
        assert_eq!(model.scroll, most.round().min(most.ceil()));
        input::release(&mut model, &ui, &lines, Vec2::new(grab.x, area.max().y + 80.0), false);
        assert!(model.dragging.is_none());
        let before = model.scroll;
        input::press(&mut model, &ui, &lines, Vec2::new(grab.x, area.origin.y + 3.0), Button::Left, 1, Mods::default());
        assert_eq!(model.scroll, before - (area.size.y - 40.0).round());
        assert!(model.dragging.is_none());
        input::press(&mut model, &ui, &lines, Vec2::new(grab.x, area.max().y - 3.0), Button::Left, 1, Mods::default());
        assert_eq!(model.scroll, before);
        let art = Art::default();
        for density in [1, 2] {
            crate::paint::set_density(density);
            for (was, now) in [(None, Some(Hit::Rail(Rail::Body))), (Some(Hit::Rail(Rail::Body)), Some(Hit::Entry(70))), (Some(Hit::Entry(70)), None)] {
                model.hover = was.clone();
                let before = look.paint(size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None });
                let mut area: Option<Rect> = None;
                for hit in [&was, &now].into_iter().flatten() {
                    let found = look.hover_rect(size, &model, &lines, &fonts, hit).expect("a hover rect");
                    area = Some(area.map_or(found, |had| join(had, found)));
                }
                model.hover = now.clone();
                let part = look.repaint(before, size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: area });
                let whole = look.paint(size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None });
                assert!(same(&part, &whole), "rail at {density}x: {was:?} -> {now:?}");
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_lasso_picks_what_it_touches() {
        use super::super::input::{self, Button, Mods, Ui};
        let dir = std::env::temp_dir().join(format!("drifter-lasso-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("work")).unwrap();
        for n in 10..22 {
            std::fs::write(dir.join(format!("work/file {n}.txt")), "x").unwrap();
        }
        let mut model = Model::new(dir.join("explorer.toml"), Some(dir.join("work")));
        model.settle_all(std::time::Duration::from_secs(5));
        let (Ok(body), Ok(small), Ok(title)) = (crate::Font::find(None, 13.0), crate::Font::find(None, 11.5), crate::Font::find(None, 17.0)) else {
            return;
        };
        let fonts = Fonts { body: &body, small: &small, title: &title };
        let look = Look::themed(&crate::theme::DARK, [64, 128, 238]);
        let size = Vec2::new(900.0, 600.0);
        crate::paint::set_density(1);
        let ui = Ui { look: &look, fonts: &fonts, size };
        let lines = model.lines();
        let frame = look.frame(size, &model);
        let top = frame.body.origin.y + HEAD_H;
        let start = Vec2::new(frame.body.origin.x + 300.0, top + 12.0 * ROW_H + 40.0);
        assert_eq!(look.hit(size, &model, &lines, &fonts, start), Some(Hit::Space));
        input::press(&mut model, &ui, &lines, start, Button::Left, 1, Mods::default());
        input::motion(&mut model, &ui, &lines, Vec2::new(start.x + 40.0, top + 3.0 * ROW_H + 5.0));
        let names: Vec<String> = model.chosen().iter().map(|one| one.name.clone()).collect();
        assert_eq!(names, (13..22).map(|n| format!("file {n}.txt")).collect::<Vec<_>>());
        let art = Art::default();
        let drawn = look.paint(size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None });
        input::release(&mut model, &ui, &lines, Vec2::new(start.x + 40.0, top + 3.0 * ROW_H + 5.0), false);
        assert!(model.dragging.is_none());
        let after = look.paint(size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None });
        assert!(!same(&drawn, &after), "the lasso box shows while dragging");
        let ctrl = Mods { ctrl: true, shift: false };
        input::press(&mut model, &ui, &lines, start, Button::Left, 1, ctrl);
        input::motion(&mut model, &ui, &lines, Vec2::new(start.x, top + 1.0));
        assert_eq!(model.picked.len(), 12);
        input::motion(&mut model, &ui, &lines, Vec2::new(start.x, top + 11.0 * ROW_H + 2.0));
        assert_eq!(model.picked.len(), 9);
        input::release(&mut model, &ui, &lines, start, false);
        input::press(&mut model, &ui, &lines, start, Button::Left, 1, Mods::default());
        input::release(&mut model, &ui, &lines, start, false);
        assert!(model.picked.is_empty());

        let blank = |row: usize| Vec2::new(frame.body.origin.x + 180.0, top + row as f64 * ROW_H + 10.0);
        assert!(look.row_blank(size, &model, &fonts, 2, blank(2)), "right of the name is blank");
        assert!(!look.row_blank(size, &model, &fonts, 2, Vec2::new(frame.body.origin.x + 60.0, blank(2).y)), "the name is not");
        input::press(&mut model, &ui, &lines, blank(2), Button::Left, 1, Mods::default());
        input::motion(&mut model, &ui, &lines, blank(5));
        input::release(&mut model, &ui, &lines, blank(5), false);
        let names: Vec<String> = model.chosen().iter().map(|one| one.name.clone()).collect();
        assert_eq!(names, (12..16).map(|n| format!("file {n}.txt")).collect::<Vec<_>>(), "a lasso can start on a row");
        input::press(&mut model, &ui, &lines, blank(7), Button::Left, 1, Mods::default());
        input::release(&mut model, &ui, &lines, blank(7), false);
        let names: Vec<String> = model.chosen().iter().map(|one| one.name.clone()).collect();
        assert_eq!(names, ["file 17.txt"], "a click there still picks the row");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn list_columns_resize_from_their_left_edge() {
        use super::super::input::{self, Button, Mods, Ui};
        let dir = std::env::temp_dir().join(format!("drifter-edges-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("work")).unwrap();
        std::fs::write(dir.join("work/a.txt"), "x").unwrap();
        let mut model = Model::new(dir.join("explorer.toml"), Some(dir.join("work")));
        model.settle_all(std::time::Duration::from_secs(5));
        let (Ok(body), Ok(small), Ok(title)) = (crate::Font::find(None, 13.0), crate::Font::find(None, 11.5), crate::Font::find(None, 17.0)) else {
            return;
        };
        let fonts = Fonts { body: &body, small: &small, title: &title };
        let look = Look::themed(&crate::theme::DARK, [64, 128, 238]);
        let size = Vec2::new(900.0, 600.0);
        crate::paint::set_density(1);
        let ui = Ui { look: &look, fonts: &fonts, size };
        let lines = model.lines();
        let frame = look.frame(size, &model);
        let edges = look.head_edges(frame.body, &model);
        let (sort, x) = edges[0];
        assert_eq!(sort, Sort::Modified);
        let y = frame.body.origin.y + HEAD_H / 2.0;
        assert_eq!(look.hit(size, &model, &lines, &fonts, Vec2::new(x + 2.0, y)), Some(Hit::Edge(Sort::Modified)));
        let size_x = edges[1].1;
        input::press(&mut model, &ui, &lines, Vec2::new(x, y), Button::Left, 1, Mods::default());
        assert!(input::motion(&mut model, &ui, &lines, Vec2::new(x - 50.0, y)));
        assert_eq!(model.prefs.widths.get(Sort::Modified), Some(220));
        let moved = look.head_edges(frame.body, &model);
        assert_eq!((moved[0].1, moved[1].1), (x - 50.0, size_x));
        input::motion(&mut model, &ui, &lines, Vec2::new(frame.body.origin.x, y));
        let (name, _) = look.list_widths(frame.body, &model);
        assert!(name >= 200.0, "the name column keeps room");
        input::release(&mut model, &ui, &lines, Vec2::new(x, y), false);
        let edge = look.head_edges(frame.body, &model)[0].1;
        input::press(&mut model, &ui, &lines, Vec2::new(edge, y), Button::Left, 2, Mods::default());
        assert_eq!(model.prefs.widths.get(Sort::Modified), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn dragging_in_a_field_picks_text() {
        use super::super::input::{self, Button, Mods, Ui};
        let dir = std::env::temp_dir().join(format!("drifter-fields-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("work")).unwrap();
        std::fs::write(dir.join("work/holiday photos.txt"), "x").unwrap();
        let mut model = Model::new(dir.join("explorer.toml"), Some(dir.join("work")));
        model.settle_all(std::time::Duration::from_secs(5));
        let (Ok(body), Ok(small), Ok(title)) = (crate::Font::find(None, 13.0), crate::Font::find(None, 11.5), crate::Font::find(None, 17.0)) else {
            return;
        };
        let fonts = Fonts { body: &body, small: &small, title: &title };
        let look = Look::themed(&crate::theme::DARK, [64, 128, 238]);
        let size = Vec2::new(900.0, 600.0);
        crate::paint::set_density(1);
        let ui = Ui { look: &look, fonts: &fonts, size };
        let lines = model.lines();
        let x_of = |font: &crate::Font, text: &str, at: usize, area: Rect| area.origin.x + font.width_of(&text[..at]);
        for mode in [Mode::List, Mode::Icons] {
            model.view.mode = mode;
            model.pick_only(dir.join("work/holiday photos.txt"));
            model.start_rename();
            let (area, font) = look.field_area(size, &model, &fonts, Spot::Rename).expect("a rename field");
            let text = model.renaming.as_ref().unwrap().1.text.clone();
            let y = area.center().y;
            input::press(&mut model, &ui, &lines, Vec2::new(x_of(font, &text, 0, area), y), Button::Left, 1, Mods::default());
            input::motion(&mut model, &ui, &lines, Vec2::new(x_of(font, &text, 7, area), y));
            input::release(&mut model, &ui, &lines, Vec2::new(x_of(font, &text, 7, area), y), false);
            assert_eq!(model.renaming.as_ref().unwrap().1.picked_text(), Some("holiday"), "{mode:?}");
            input::press(&mut model, &ui, &lines, Vec2::new(x_of(font, &text, 10, area), y), Button::Left, 2, Mods::default());
            assert_eq!(model.renaming.as_ref().unwrap().1.picked_text(), Some("photos"), "{mode:?}");
            assert!(model.renaming.is_some());
            model.renaming = None;
        }
        model.view.mode = Mode::List;
        model.act(super::super::model::Act::EditPath);
        let (area, font) = look.field_area(size, &model, &fonts, Spot::Path).unwrap();
        let text = model.path.as_ref().unwrap().text.clone();
        let end = Vec2::new(x_of(font, &text, text.len(), area), area.center().y);
        input::press(&mut model, &ui, &lines, end, Button::Left, 1, Mods::default());
        input::motion(&mut model, &ui, &lines, Vec2::new(x_of(font, &text, text.len() - 4, area), area.center().y));
        assert_eq!(model.path.as_ref().unwrap().picked_text(), Some("work"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn key_names_read_well() {
        assert_eq!(key_words("Alt+Up / Backspace"), format!("Alt+↑ / {}", crate::lang::t("Backspace")));
        assert_eq!(key_words("Shift+Ctrl+N"), format!("{}+{}+N", crate::lang::t("Shift"), crate::lang::t("Ctrl")));
        assert_eq!(key_words("Ctrl+1–4"), format!("{}+1–4", crate::lang::t("Ctrl")));
        assert_eq!(key_words("F2"), "F2");
        assert_eq!(key_words("Shift+Delete"), format!("{}+{}", crate::lang::t("Shift"), crate::lang::t("Del")));
        assert_eq!(key_words(""), "");
    }

    #[test]
    fn small_pictures_grow_in_the_gallery_but_not_without_end() {
        let room = Vec2::new(800.0, 600.0);
        let small = grow_fit(Vec2::new(100.0, 50.0), room, MOST_GROW);
        assert_eq!(small.size, Vec2::new(400.0, 200.0));
        assert_eq!(small.center(), room / 2.0);
        let mid = grow_fit(Vec2::new(300.0, 300.0), room, MOST_GROW);
        assert_eq!(mid.size, Vec2::new(600.0, 600.0));
        let big = grow_fit(Vec2::new(4000.0, 1000.0), room, MOST_GROW);
        assert_eq!(big.size, Vec2::new(800.0, 200.0));
        let paper = Image { width: 2, height: 1, rgba: vec![250, 250, 248, 255, 0, 0, 0, 0] };
        assert!(pale(&paper) && silhouette(&paper).rgba == [0, 0, 0, 255, 0, 0, 0, 0]);
        assert!(!pale(&Image { width: 1, height: 1, rgba: vec![60, 120, 230, 255] }), "a blue folder is not pale");
        let mut logo = Image { width: 2, height: 1, rgba: vec![9, 9, 9, 255, 0, 0, 0, 0] };
        assert!(!solid(&logo), "see-through pictures get no box shadow");
        logo.rgba[7] = 255;
        assert!(solid(&logo));
    }

    #[test]
    fn an_empty_file_shows_its_icon_not_a_blank_page() {
        let dir = std::env::temp_dir().join(format!("drifter-blank-peek-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("work")).unwrap();
        std::fs::write(dir.join("work/empty.txt"), "").unwrap();
        let mut model = Model::new(dir.join("explorer.toml"), Some(dir.join("work")));
        model.settle_all(std::time::Duration::from_secs(5));
        let (Ok(body), Ok(small), Ok(title)) = (crate::Font::find(None, 13.0), crate::Font::find(None, 11.5), crate::Font::find(None, 17.0)) else {
            return;
        };
        let fonts = Fonts { body: &body, small: &small, title: &title };
        let look = Look::themed(&crate::theme::DARK, [64, 128, 238]);
        let size = Vec2::new(900.0, 600.0);
        let art = Art::default();
        model.act(super::super::model::Act::Show(Mode::Columns));
        model.pick_only(dir.join("work/empty.txt"));
        let lines = model.lines();
        let shot = |model: &Model| look.paint(size, &Paint { model, lines: &lines, fonts: &fonts, art: &art, clip: None }).rgba;
        model.peek = None;
        let icon = shot(&model);
        model.peek = Some(vec![String::new()]);
        assert!(shot(&model) == icon, "a blank peek paints like no peek");
        model.peek = Some(vec!["words".to_string()]);
        assert!(shot(&model) != icon, "real text still shows as a page");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn menus_and_sheets_fade_in() {
        let dir = std::env::temp_dir().join(format!("drifter-pop-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("work")).unwrap();
        let mut model = Model::new(dir.join("explorer.toml"), Some(dir.join("work")));
        model.settle_all(std::time::Duration::from_secs(5));
        let (Ok(body), Ok(small), Ok(title)) = (crate::Font::find(None, 13.0), crate::Font::find(None, 11.5), crate::Font::find(None, 17.0)) else {
            return;
        };
        let fonts = Fonts { body: &body, small: &small, title: &title };
        let look = Look::themed(&crate::theme::DARK, [64, 128, 238]);
        let size = Vec2::new(900.0, 600.0);
        let art = Art::default();
        let lines = model.lines();
        let shot = |model: &Model| look.paint(size, &Paint { model, lines: &lines, fonts: &fonts, art: &art, clip: None }).rgba;
        let plain = shot(&model);
        let items = model.space_menu();
        model.menu = Some(super::super::model::Menu { items, at: Vec2::new(300.0, 200.0), hover: None, open: None, kid_hover: None, bar: None });
        let full = shot(&model);
        model.pop = 0.0;
        assert!(shot(&model) == plain, "a menu that just opened is not drawn yet");
        model.pop = 0.5;
        let half = shot(&model);
        assert!(half != plain && half != full, "halfway it is see-through");
        model.pop = 1.0;
        assert!(shot(&model) == full);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn files_from_elsewhere_drop_into_the_open_folder() {
        let dir = std::env::temp_dir().join(format!("drifter-drop-here-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("work/inner")).unwrap();
        std::fs::write(dir.join("work/a.txt"), "x").unwrap();
        let mut model = Model::new(dir.join("explorer.toml"), Some(dir.join("work")));
        model.settle_all(std::time::Duration::from_secs(5));
        let (Ok(body), Ok(small), Ok(title)) = (crate::Font::find(None, 13.0), crate::Font::find(None, 11.5), crate::Font::find(None, 17.0)) else {
            return;
        };
        let fonts = Fonts { body: &body, small: &small, title: &title };
        let look = Look::themed(&crate::theme::DARK, [64, 128, 238]);
        let size = Vec2::new(900.0, 600.0);
        let lines = model.lines();
        let empty = look.frame(size, &model).body.max() - Vec2::new(20.0, 20.0);
        let here = Some(Drop::Into(dir.join("work")));
        let drop = |paths: &[PathBuf]| look.drop_at(size, &model, &lines, &fonts, empty, paths).map(|(drop, _)| drop);
        assert_eq!(drop(&[]), here, "a drop from another app");
        assert_eq!(drop(&[dir.join("far.txt")]), here, "a drop from another folder");
        assert_eq!(drop(&[dir.join("work/a.txt")]), None, "a file already here stays put");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_keyboard_moves_between_the_sidebar_and_the_files() {
        use super::super::input::{self, Button, Key, Mods, Named, Ui};
        use super::super::model::{Pane, Pref};
        let dir = std::env::temp_dir().join(format!("drifter-panes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("work/inner")).unwrap();
        std::fs::write(dir.join("work/a.txt"), "x").unwrap();
        let mut model = Model::new(dir.join("explorer.toml"), Some(dir.join("work")));
        model.set(Pref::Root(super::super::prefs::Root::Computer));
        for _ in 0..8 {
            model.lines();
            model.settle_all(std::time::Duration::from_secs(5));
        }
        let (Ok(body), Ok(small), Ok(title)) = (crate::Font::find(None, 13.0), crate::Font::find(None, 11.5), crate::Font::find(None, 17.0)) else {
            return;
        };
        let fonts = Fonts { body: &body, small: &small, title: &title };
        let look = Look::themed(&crate::theme::DARK, [64, 128, 238]);
        let size = Vec2::new(900.0, 600.0);
        crate::paint::set_density(1);
        let ui = Ui { look: &look, fonts: &fonts, size };
        let art = Art::default();
        let press = |model: &mut Model, named: Named| input::key(model, &ui, &Key { named, text: None, ctrl: false, shift: false, alt: false });
        let lines = model.lines();
        let plain = look.paint(size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None });
        press(&mut model, Named::Tab);
        assert_eq!(model.pane, Pane::Side);
        assert!(model.ring);
        let lines = model.lines();
        let at = model.side_at.expect("a line");
        assert_eq!(lines[at].path(), Some(dir.join("work").as_path()));
        let ringed = look.paint(size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None });
        assert!(!same(&plain, &ringed), "the sidebar ring shows");
        press(&mut model, Named::Left);
        press(&mut model, Named::Left);
        let lines = model.lines();
        assert_eq!(lines[model.side_at.unwrap()].path(), Some(dir.as_path()));
        press(&mut model, Named::Enter);
        assert_eq!(model.dir, dir);
        press(&mut model, Named::Right);
        press(&mut model, Named::Right);
        let lines = model.lines();
        assert_eq!(lines[model.side_at.unwrap()].path(), Some(dir.join("work").as_path()));
        press(&mut model, Named::Tab);
        assert_eq!(model.pane, Pane::Files);
        press(&mut model, Named::Down);
        assert_eq!(model.focus, Some(dir.join("work")));
        let lines = model.lines();
        let files = look.paint(size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None });
        model.ring = false;
        let quiet = look.paint(size, &Paint { model: &model, lines: &lines, fonts: &fonts, art: &art, clip: None });
        assert!(!same(&files, &quiet), "the file ring shows");
        model.ring = true;
        let frame = look.frame(size, &model);
        input::press(&mut model, &ui, &lines, frame.body.center(), Button::Left, 1, Mods::default());
        assert!(!model.ring);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
