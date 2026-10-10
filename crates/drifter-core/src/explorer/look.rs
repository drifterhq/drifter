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
pub const JOB_H: f64 = 50.0;
pub const SPLIT_W: f64 = 6.0;
pub const ROW_H: f64 = 26.0;
pub const HEAD_H: f64 = 28.0;
pub const LINE_H: f64 = 26.0;
pub const COLUMN_W: f64 = 250.0;
pub const PREVIEW_W: f64 = 320.0;
pub const STRIP_H: f64 = 112.0;
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct Crumb {
    pub area: Rect,
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
        let banner = (!model.alerts.is_empty()).then(|| rect(0.0, MENU_H + BAR_H, size.x, BANNER_H));
        let below = MENU_H + BAR_H + banner.map_or(0.0, |banner| banner.size.y);
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
        Frame { menu, bar, banner, jobs, side, split, body, status, clip: None }
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

    pub fn path_rect(&self, bar: Rect) -> Rect {
        let nav = self.nav_rects(bar);
        let left = nav[2].max().x + 12.0;
        let right = self.view_rects(bar)[0].1.origin.x - 12.0;
        rect(left, nav[0].origin.y, right - left, BUTTON)
    }

    fn zoom_rect(&self, status: Rect, model: &Model, fonts: &Fonts) -> Option<Rect> {
        if model.prefs.mode != Mode::Icons {
            return None;
        }
        let free = model.free_words().map_or(0.0, |words| fonts.small.width_of(&words) + 24.0);
        Some(rect(status.size.x - free - ZOOM_W - 30.0, status.origin.y + 6.0, ZOOM_W, STATUS_H - 12.0))
    }

    pub fn list_widths(&self, body: Rect, model: &Model) -> (f64, Vec<(Sort, f64)>) {
        let mut extra = Vec::new();
        if model.dir == super::ops::trash_files() {
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
        let mut name = body.size.x - 16.0 - extra.iter().map(|(_, w)| w).sum::<f64>();
        while name < 200.0 && !extra.is_empty() {
            let (_, w) = extra.pop().unwrap_or((Sort::Kind, 0.0));
            name += w;
        }
        (name, extra)
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
        match model.prefs.mode {
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

    pub fn content_height(&self, frame: &Frame, model: &Model) -> f64 {
        let count = model.entries.len();
        match model.prefs.mode {
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
            let path = self.path_rect(frame.bar);
            if path.contains(at) {
                if model.path.is_none()
                    && let Some(found) = self.crumbs(path, model, fonts).iter().position(|crumb| crumb.area.contains(at) && !crumb.last)
                {
                    return Some(Hit::Crumb(found));
                }
                return Some(Hit::Path);
            }
            return None;
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
        match model.prefs.mode {
            Mode::List => {
                if at.y < body.origin.y + HEAD_H {
                    let (name, extra) = self.list_widths(body, model);
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
        if let Some(banner) = frame.banner.filter(|banner| touches(clip, *banner)) {
            self.paint_banner(&mut canvas, banner, model, fonts);
        }
        if let Some(jobs) = frame.jobs.filter(|jobs| touches(clip, *jobs)) {
            self.paint_jobs(&mut canvas, jobs, model, fonts);
        }
        if let Some(status) = frame.status.filter(|status| touches(clip, *status)) {
            self.paint_status(&mut canvas, status, model, fonts);
        }
        if let Some(super::model::Drag::Files { paths, at, live: true, .. }) = &model.dragging {
            self.paint_drag(&mut canvas, size, job, paths, *at);
        }
        if let Some(sheet) = &model.sheet {
            self.paint_sheet(&mut canvas, size, sheet, job);
        }
        if let Some(menu) = &model.menu {
            self.paint_menu(&mut canvas, size, menu, fonts);
        }
        canvas.image
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
                Line::Head(_) => None,
            };
        }
        match self.hit(size, model, lines, fonts, at)? {
            Hit::Entry(index) | Hit::Strip(index) => {
                let one = model.entries.get(index)?;
                into(&one.path, self.entry_rect(&frame, model, index)?, one.folder)
            }
            Hit::Crumb(index) => {
                let crumb = self.crumbs(self.path_rect(frame.bar), model, fonts).get(index)?.clone();
                into(&crumb.path, crumb.area, true)
            }
            Hit::Column(column, Some(row)) => {
                let one = model.columns.get(column)?.entries.get(row)?;
                let area = self.column_rect(frame.body, model, column);
                let y = area.origin.y + 6.0 + row as f64 * ROW_H - model.columns[column].scroll;
                into(&one.path, rect(area.origin.x + 6.0, y, COLUMN_W - 12.0, ROW_H), one.folder)
            }
            _ => None,
        }
    }

    fn paint_drag(&self, canvas: &mut Canvas, size: Vec2, job: &Paint, paths: &[PathBuf], at: Vec2) {
        let fonts = job.fonts;
        if let Some((drop, area)) = self.drop_at(size, job.model, job.lines, fonts, at, paths) {
            match drop {
                Drop::Mark => canvas.round_rect(area, 1.5, self.accent, 1.0),
                Drop::Into(_) | Drop::Trash => {
                    canvas.round_rect(area, 7.0, self.accent, 0.18);
                    canvas.ring(area, 7.0, 2.0, self.accent, 1.0);
                }
            }
        }
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
                canvas.round_rect(area, 6.0, self.accent, 1.0);
            } else if hot {
                canvas.round_rect(area, 6.0, self.hover, 0.7);
            }
            let ink = if open == Some(index) { self.on_accent } else { self.ink };
            self.text_center(canvas, fonts.body, name, ink, area.center(), area.size.x);
        }
    }

    fn button(&self, canvas: &mut Canvas, area: Rect, hot: bool, on: bool) {
        match (on, hot) {
            (true, _) => canvas.round_rect(area, 7.0, self.accent, 1.0),
            (false, true) => canvas.round_rect(area, 7.0, self.hover, 0.8),
            _ => {}
        }
    }

    fn paint_bar(&self, canvas: &mut Canvas, frame: &Frame, job: &Paint) {
        let model = job.model;
        let fonts = job.fonts;
        let bar = frame.bar;
        canvas.round_rect(bar, 0.0, self.bar, 1.0);
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
        canvas.round_rect(path, 8.0, self.well, 1.0);
        match editing {
            Some(field) => {
                canvas.ring(path, 8.0, 2.0, self.accent, 1.0);
                self.paint_field(canvas, fonts.body, field, rect(path.origin.x + 10.0, path.origin.y, path.size.x - 20.0, path.size.y));
            }
            None => {
                canvas.ring(path, 8.0, 1.0, self.rule, 1.0);
                self.paint_crumbs(canvas, path, model, fonts);
            }
        }
        for (mode, area) in self.view_rects(bar) {
            let on = model.prefs.mode == mode;
            let hot = model.hover == Some(Hit::Show(mode));
            self.button(canvas, rect(area.origin.x + 1.0, area.origin.y, area.size.x - 2.0, area.size.y), hot, on);
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
            out.push(Crumb { area: rect(x - 4.0, top, 22.0, tall), path: parts[first - 1].1.clone(), words: "…".to_string(), last: false });
            x += 14.0 + arrow - 4.0;
        }
        let count = parts.len();
        for (at, (words, place)) in parts.into_iter().enumerate().skip(first) {
            let wide = widths[at].min((room - x).max(0.0));
            out.push(Crumb { area: rect(x - 4.0, top, wide + 8.0, tall), path: place, words, last: at + 1 == count });
            x += wide + if at + 1 == count { 0.0 } else { arrow };
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
            if !crumb.last && crumb.words != "/" {
                self.chevron(canvas, Vec2::new(crumb.area.max().x + 5.0, middle), self.dim);
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

    fn paint_field(&self, canvas: &mut Canvas, font: &Font, field: &Field, area: Rect) {
        let middle = area.center().y;
        let before = font.width_of(&field.text[..field.caret]);
        let total = font.width_of(&field.text);
        let shift = (before - area.size.x + 4.0).max(0.0).min((total - area.size.x + 4.0).max(0.0));
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
                let size = logical(image);
                canvas.stamp(image, (area.center() - size / 2.0).round(), 1.0);
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
            canvas.shadow(rect(at.x, at.y, fit.size.x, fit.size.y), 3.0, 6.0, self.drop * 0.6);
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
                canvas.round_rect(area, 7.0, self.accent, 1.0);
            } else if hot {
                canvas.round_rect(area, 7.0, self.hover, 0.7);
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
                    let name = match path == &model.tree_base() {
                        true => match model.prefs.tree_root {
                            prefs::Root::Home => crate::lang::t("Home").to_string(),
                            prefs::Root::Computer => crate::lang::t("Computer").to_string(),
                        },
                        false => super::entry::name_of(path),
                    };
                    let icon_at = rect(area.origin.x + 22.0 + indent, middle - 8.0, 16.0, 16.0);
                    self.icon_or_glyph(&mut canvas, job.art, folder_icon(path, model), Family::Folder, icon_at);
                    self.text(&mut canvas, fonts.body, &name, ink, Vec2::new(icon_at.max().x + 7.0, middle), area.max().x - icon_at.max().x - 12.0);
                }
            }
        }
        self.scroll_mark(&mut canvas, local, self.tree_height(job.lines), model.tree_scroll);
        canvas.image
    }

    fn scroll_mark(&self, canvas: &mut Canvas, area: Rect, content: f64, scroll: f64) {
        if content <= area.size.y + 1.0 {
            return;
        }
        let share = area.size.y / content;
        let tall = (area.size.y * share).max(28.0);
        let top = area.origin.y + (area.size.y - tall) * (scroll / (content - area.size.y)).clamp(0.0, 1.0);
        canvas.round_rect(rect(area.max().x - 7.0, top + 2.0, 4.0, tall - 4.0), 2.0, self.dim, 0.45);
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
            match model.prefs.mode {
                Mode::List => self.paint_list(&mut piece, &shifted, job),
                Mode::Icons => self.paint_icons(&mut piece, &shifted, job),
                Mode::Columns => self.paint_columns(&mut piece, &shifted, job),
                Mode::Gallery => self.paint_gallery(&mut piece, &shifted, job),
            }
            if model.entries.is_empty() && model.prefs.mode != Mode::Columns {
                let words = match (model.loading.is_some(), super::ops::in_trash(&model.dir)) {
                    (true, _) if model.showing_loading() => Some(crate::lang::t("Loading…")),
                    (true, _) => None,
                    (false, true) => Some(crate::lang::t("The Trash is empty")),
                    (false, false) => Some(crate::lang::t("This folder is empty")),
                };
                if let Some(words) = words {
                    self.text_center(&mut piece, job.fonts.body, words, self.dim, local.center(), local.size.x - 40.0);
                }
            }
        }
        canvas.stamp(&piece.image, body.origin, 1.0);
    }

    fn row_back(&self, canvas: &mut Canvas, area: Rect, picked: bool, hot: bool, focused: bool, stripe: bool) {
        if picked {
            canvas.round_rect(area, 6.0, self.accent, 1.0);
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
            self.row_back(canvas, area, picked, hot, model.focus.as_ref() == Some(&entry.path) && model.picked.len() > 1, at % 2 == 1);
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
                    Sort::Origin => entry.trashed.as_ref().and_then(|(from, _)| from.parent()).map(|folder| super::entry::short_path(folder, &model.home)).unwrap_or_default(),
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
                let on = model.prefs.sort == sort;
                let hot = model.hover == Some(Hit::Head(sort));
                if hot {
                    canvas.round_rect(rect(x + 2.0, 3.0, w - 4.0, top - 7.0), 5.0, self.hover, 0.6);
                }
                let ink = if on { self.ink } else { self.dim };
                let label = fonts.small.paint_weight(sort.name(), ink, if on { 620.0 } else { 500.0 });
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
                    let corners = match model.prefs.down {
                        false => [c + Vec2::new(-3.5, 2.0), c + Vec2::new(3.5, 2.0), c + Vec2::new(0.0, -2.5)],
                        true => [c + Vec2::new(-3.5, -2.0), c + Vec2::new(3.5, -2.0), c + Vec2::new(0.0, 2.5)],
                    };
                    canvas.triangle(corners, self.dim, 1.0);
                }
                if sort != Sort::Name {
                    canvas.round_rect(rect(x, 7.0, 1.0, top - 14.0), 0.0, self.rule, 1.0);
                }
                x += w;
            }
        }
        self.scroll_mark(canvas, rect(0.0, top, body.size.x, body.size.y - top), self.content_height(frame, model) - top, model.scroll);
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
                canvas.round_rect(pill, 6.0, self.accent, 1.0);
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
        self.scroll_mark(canvas, body, self.content_height(frame, model), model.scroll);
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
            self.scroll_mark(canvas, area, content, column.scroll);
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
        match &model.peek {
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
            match (&model.peek, one.family().shows_picture() && model.prefs.thumbnails) {
                (Some(lines), _) => {
                    let page = Rect::from_center(picture.center(), Vec2::new(picture.size.x.min(picture.size.y * 0.8), picture.size.y));
                    self.paint_peek(canvas, page, lines, fonts);
                }
                (None, true) => {
                    let side = picture.size.x.min(picture.size.y).clamp(1.0, 1600.0);
                    let wide = rect(picture.origin.x, picture.origin.y, picture.size.x, picture.size.y);
                    match job.art.thumb(&one.path, side) {
                        Some(image) => {
                            let fitted = shrink_fit(logical(image), wide.size);
                            let at = wide.origin + fitted.origin;
                            let shown = image.scaled(crate::paint::dense(fitted.size.x), crate::paint::dense(fitted.size.y));
                            canvas.shadow(rect(at.x, at.y, fitted.size.x, fitted.size.y), 4.0, 14.0, self.drop);
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
                canvas.round_rect(tile, 10.0, self.accent, 1.0);
            } else if model.hover == Some(Hit::Strip(at)) {
                canvas.round_rect(tile, 10.0, self.hover, 0.6);
            }
            let side = tile.size.x - 16.0;
            self.entry_art(canvas, job.art, model, entry, Rect::from_center(tile.center(), Vec2::splat(side)));
        }
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
            false => match model.prefs.mode {
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
        let mut image = image;
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
            Hit::Crumb(index) => self.crumbs(self.path_rect(frame.bar), model, fonts).get(*index)?.area,
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
        let right_room = free.as_ref().map_or(0.0, |words| fonts.small.width_of(words) + 24.0) + if model.prefs.mode == Mode::Icons { ZOOM_W + 50.0 } else { 0.0 };
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
        let keys = items.iter().map(|item| fonts.small.width_of(item.keys)).fold(0.0, f64::max);
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
                canvas.round_rect(row, 6.0, self.accent, 1.0);
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
            let keys_w = if item.keys.is_empty() { 0.0 } else { fonts.small.width_of(item.keys) + 16.0 };
            self.text(canvas, fonts.body, &item.label, ink, Vec2::new(row.origin.x + 26.0, middle), row.size.x - 36.0 - keys_w);
            if !item.keys.is_empty() {
                let keys_ink = if lit { self.on_accent } else { self.dim };
                self.text_right(canvas, fonts.small, item.keys, keys_ink, row.max().x - 10.0, middle);
            }
            if !item.kids.is_empty() || (item.act.is_none() && !item.rule) {
                self.chevron(canvas, Vec2::new(row.max().x - 12.0, middle), ink);
            }
        }
    }

    pub fn sheet_rect(&self, size: Vec2, sheet: &Sheet) -> Rect {
        let want = match sheet {
            Sheet::Prefs(_) => Vec2::new(720.0, 520.0),
            Sheet::Info(info) => Vec2::new(460.0, 330.0 + 34.0 * (4 + usize::from(info.mode.is_some()) + usize::from(info.target.is_some())) as f64),
            Sheet::Confirm { .. } => Vec2::new(420.0, 190.0),
            Sheet::Clash { names, .. } => Vec2::new(540.0, if names.len() == 1 { 190.0 } else { 236.0 }),
            Sheet::Keys => Vec2::new(520.0, 520.0),
            Sheet::About => Vec2::new(380.0, 260.0),
        };
        let fitted = Vec2::new(want.x.min(size.x - 24.0), want.y.min(size.y - 24.0));
        Rect::from_center(size / 2.0, fitted).round_out()
    }

    fn sheet_buttons(&self, area: Rect, sheet: &Sheet) -> Vec<(SheetHit, Rect)> {
        let y = area.max().y - 20.0 - 32.0;
        match sheet {
            Sheet::Confirm { .. } => vec![
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
                canvas.round_rect(place, 8.0, self.accent, 1.0);
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
        let name = fonts.title.paint_weight(&fit(fonts.title, &one.name, area.size.x - 48.0), self.ink, 620.0);
        canvas.stamp(&name, Vec2::new(area.center().x - logical(&name).x / 2.0, art.max().y + 14.0).round(), 1.0);
        let size = match (one.folder, info.inside) {
            (true, Some(count)) => super::entry::items(count),
            (true, None) => "--".to_string(),
            (false, _) => format!("{} ({} {})", super::entry::size_words(one.size), one.size, crate::lang::t("bytes")),
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
        let mut y = art.max().y + 58.0;
        let card = rect(area.origin.x + 20.0, y - 8.0, area.size.x - 40.0, rows.len() as f64 * 34.0 + 6.0);
        canvas.round_rect(card, 10.0, self.well, 1.0);
        for (at, (label, value)) in rows.iter().enumerate() {
            if at > 0 {
                canvas.round_rect(rect(card.origin.x + 12.0, y - 5.0, card.size.x - 24.0, 1.0), 0.0, self.rule, 0.8);
            }
            self.text(canvas, fonts.small, label, self.dim, Vec2::new(card.origin.x + 14.0, y + 12.0), 90.0);
            self.text(canvas, fonts.small, value, self.ink, Vec2::new(card.origin.x + 108.0, y + 12.0), card.size.x - 122.0);
            y += 34.0;
        }
    }

    fn paint_keys(&self, canvas: &mut Canvas, area: Rect, fonts: &Fonts) {
        let head = fonts.title.paint_weight(crate::lang::t("Keyboard Shortcuts"), self.ink, 620.0);
        canvas.stamp(&head, Vec2::new(area.origin.x + 24.0, area.origin.y + 22.0).round(), 1.0);
        let rows: [(&str, &str); 16] = [
            ("Enter", crate::lang::t("Open")),
            ("Alt+Up / Backspace", crate::lang::t("Enclosing Folder")),
            ("Alt+Left / Alt+Right", crate::lang::t("Back and forward")),
            ("Ctrl+L", crate::lang::t("Type a path")),
            ("F2", crate::lang::t("Rename")),
            ("Delete", crate::lang::t("Move to Trash")),
            ("Shift+Delete", crate::lang::t("Delete Forever")),
            ("Ctrl+C / X / V", crate::lang::t("Copy, cut and paste")),
            ("Ctrl+D", crate::lang::t("Duplicate")),
            ("Ctrl+Z", crate::lang::t("Undo")),
            ("Shift+Ctrl+N", crate::lang::t("New Folder")),
            ("Ctrl+1 to 4", crate::lang::t("Change the view")),
            ("Ctrl++ / Ctrl+-", crate::lang::t("Zoom")),
            ("Ctrl+H", crate::lang::t("Show Hidden Files")),
            ("Ctrl+I", crate::lang::t("Get Info")),
            ("Ctrl+,", crate::lang::t("Settings")),
        ];
        let mut y = area.origin.y + 70.0;
        for (keys, words) in rows {
            let chip = fonts.small.paint_weight(keys, self.ink, 560.0);
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
                model.prefs.mode = mode;
                let pairs = [(None, Some(Hit::Entry(3))), (Some(Hit::Entry(3)), Some(Hit::Entry(4))), (Some(Hit::Entry(4)), Some(Hit::Line(2))), (Some(Hit::Line(2)), Some(Hit::Head(Sort::Name))), (Some(Hit::Head(Sort::Name)), Some(Hit::Gear)), (Some(Hit::Gear), Some(Hit::Crumb(0))), (Some(Hit::Crumb(0)), None)];
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
        let mut blits = 0;
        for density in [1, 2] {
            crate::paint::set_density(density);
            for (mode, side) in [(Mode::List, false), (Mode::Icons, false), (Mode::List, true)] {
                model.prefs.mode = mode;
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
}
