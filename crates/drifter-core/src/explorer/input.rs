use std::path::Path;

use super::look::{Fonts, Hit, Look, Rail, SheetHit, Spot};
use super::model::{Act, Drag, Line, Menu, Model, Pane, Sheet};
use super::prefs::{self, Click, Mode};
use crate::geom::Vec2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Named {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Enter,
    Escape,
    Back,
    Delete,
    Tab,
    F2,
    F5,
    F9,
    Menu,
    Letter(char),
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub named: Named,
    pub text: Option<String>,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
}

pub struct Ui<'a> {
    pub look: &'a Look,
    pub fonts: &'a Fonts<'a>,
    pub size: Vec2,
}

fn field_of(model: &mut Model, spot: Spot) -> Option<&mut super::field::Field> {
    match spot {
        Spot::Path => model.path.as_mut(),
        Spot::Search => model.search.as_mut().map(|search| &mut search.field),
        Spot::Rename => model.renaming.as_mut().map(|(_, field)| field),
    }
}

fn press_field(model: &mut Model, ui: &Ui, spot: Spot, at: Vec2, clicks: u32, shift: bool) {
    let Some((area, font)) = ui.look.field_area(ui.size, model, ui.fonts, spot) else {
        return;
    };
    let Some(field) = field_of(model, spot) else {
        return;
    };
    let caret = ui.look.field_caret(font, field, area, at.x);
    match clicks {
        0 | 1 => {
            field.caret_at(caret, shift);
            model.dragging = Some(Drag::Text(spot));
        }
        2 => field.pick_word(caret),
        _ => field.pick_all(),
    }
}

pub fn press(model: &mut Model, ui: &Ui, lines: &[Line], at: Vec2, button: Button, clicks: u32, mods: Mods) {
    model.tip = None;
    model.ring = false;
    if model.menu.is_some() {
        return press_menu(model, ui, lines, at);
    }
    if model.sheet.is_some() {
        return press_sheet(model, ui, at, clicks);
    }
    let hit = ui.look.hit(ui.size, model, lines, ui.fonts, at);
    if model.renaming.is_some() {
        let on_it = match (&hit, &model.renaming) {
            (Some(Hit::Entry(index)), Some((path, _))) => model.entries.get(*index).is_some_and(|each| &each.path == path),
            (Some(Hit::Column(column, Some(row))), Some((path, _))) => model.columns.get(*column).and_then(|c| c.entries.get(*row)).is_some_and(|each| &each.path == path),
            _ => false,
        };
        if !on_it {
            model.finish_rename();
        } else {
            if button == Button::Left
                && ui.look.field_area(ui.size, model, ui.fonts, Spot::Rename).is_some_and(|(area, _)| area.contains(at))
            {
                press_field(model, ui, Spot::Rename, at, clicks, mods.shift);
            }
            return;
        }
    }
    if model.path.is_some() && hit != Some(Hit::Path) {
        model.path = None;
    }
    if let Some(search) = model.search.as_mut()
        && hit != Some(Hit::Search)
    {
        search.focused = false;
    }
    let Some(hit) = hit else {
        return;
    };
    let right = button == Button::Right;
    match hit {
        Hit::Line(index) | Hit::Caret(index) | Hit::Eject(index) => {
            model.pane = Pane::Side;
            model.side_at = Some(index);
        }
        Hit::Entry(_) | Hit::Strip(_) | Hit::Column(..) | Hit::Space | Hit::Picture => model.pane = Pane::Files,
        _ => {}
    }
    if button == Button::Middle {
        return press_middle(model, ui, lines, hit);
    }
    match hit {
        Hit::Bar(index) => open_bar(model, ui, index),
        Hit::Back => model.act(Act::Back),
        Hit::Forward => model.act(Act::Forward),
        Hit::Up => model.act(Act::Up),
        Hit::Path => match model.path.is_some() {
            true => press_field(model, ui, Spot::Path, at, clicks, mods.shift),
            false => model.act(Act::EditPath),
        },
        Hit::Crumb(index) => {
            let area = ui.look.path_rect(ui.look.frame(ui.size, model).bar);
            if let Some(crumb) = ui.look.crumbs(area, model, ui.fonts).get(index).cloned() {
                match right {
                    true => model.act(Act::EditPath),
                    false => {
                        let pick = model.dir.ancestors().find(|at| at.parent() == Some(crumb.path.as_path())).map(Path::to_path_buf);
                        model.go(crumb.path, pick);
                    }
                }
            }
        }
        Hit::CrumbArrow(index) => {
            let area = ui.look.path_rect(ui.look.frame(ui.size, model).bar);
            if let Some(crumb) = ui.look.crumbs(area, model, ui.fonts).get(index).cloned() {
                let items = model.folder_menu(&crumb.path);
                model.menu = Some(menu_at(items, Vec2::new(crumb.arrow.origin.x - 8.0, area.max().y + 4.0)));
            }
        }
        Hit::Search => {
            model.act(Act::Search);
            press_field(model, ui, Spot::Search, at, clicks, mods.shift);
        }
        Hit::SearchClear => model.act(Act::EndSearch),
        Hit::Tab(index) => match right {
            true => model.menu = Some(menu_at(model.tab_menu(index), at)),
            false => model.act(Act::ShowTab(index)),
        },
        Hit::TabClose(index) => model.act(Act::CloseTab(index)),
        Hit::TabNew => model.act(Act::NewTab),
        Hit::Tabs => match (right, clicks) {
            (true, _) => model.menu = Some(menu_at(model.tab_menu(model.tab), at)),
            (false, 2..) => model.act(Act::NewTab),
            _ => {}
        },
        Hit::Show(mode) => model.act(Act::Show(mode)),
        Hit::Gear => model.act(Act::Prefs),
        Hit::Dismiss => model.act(Act::Dismiss),
        Hit::Stop(id) => model.act(Act::Stop(id)),
        Hit::Banner | Hit::Jobs => {}
        Hit::Split => model.dragging = Some(Drag::Split { from: at.x, width: model.prefs.tree_width }),
        Hit::Rail(rail) => {
            let Some((area, content, scroll)) = ui.look.rail_place(ui.size, model, lines, rail).filter(|_| !right) else {
                return;
            };
            let thumb = super::look::rail_thumb(area, content, scroll);
            let most = content - area.size.y;
            match (at.y < thumb.origin.y, at.y >= thumb.max().y) {
                (true, _) => set_rail(model, rail, scroll - (area.size.y - 40.0).max(20.0), most),
                (_, true) => set_rail(model, rail, scroll + (area.size.y - 40.0).max(20.0), most),
                _ => model.dragging = Some(Drag::Rail { rail, grab: at.y - thumb.origin.y }),
            }
        }
        Hit::Eject(index) => {
            if let Some(Line::Drive { drive: Some(drive), .. }) = lines.get(index)
                && let Some(device) = drive.device.clone()
            {
                model.act(Act::Eject(device));
            }
        }
        Hit::Line(index) if matches!(lines.get(index), Some(Line::Drive { drive: Some(_), .. })) => {
            let Some(Line::Drive { drive: Some(drive), .. }) = lines.get(index).cloned() else {
                return;
            };
            match (right, &drive.mount, &drive.device) {
                (true, _, _) => {
                    let items = model.drive_menu(&drive);
                    model.menu = Some(menu_at(items, at));
                }
                (false, Some(point), _) => model.go(point.clone(), None),
                (false, None, Some(device)) => model.act(Act::Mount(device.clone())),
                (false, None, None) => {}
            }
        }
        Hit::Line(index) => {
            if let Some(path) = lines.get(index).and_then(Line::path) {
                let path = path.to_path_buf();
                match right {
                    true => {
                        let mark = matches!(lines.get(index), Some(Line::Place { mark: true, .. }));
                        let items = model.line_menu(&path, mark);
                        model.menu = Some(menu_at(items, at));
                    }
                    false => model.go(path, None),
                }
            }
        }
        Hit::Caret(index) => {
            if let Some(path) = lines.get(index).and_then(Line::path) {
                let path = path.to_path_buf();
                model.fold(&path);
            }
        }
        Hit::Head(sort) => model.act(Act::SortBy(sort)),
        Hit::Edge(sort) if clicks >= 2 => {
            model.prefs.widths.set(sort, None);
            model.saved = false;
        }
        Hit::Edge(sort) => {
            let body = ui.look.frame(ui.size, model).body;
            let (_, extra) = ui.look.list_widths(body, model);
            let rest = extra.iter().skip_while(|(each, _)| *each != sort).skip(1).map(|(_, width)| width).sum();
            model.dragging = Some(Drag::Edge { sort, rest });
        }
        Hit::Entry(index) if !right && clicks == 1 && ui.look.row_blank(ui.size, model, ui.fonts, index, at) => {
            let keep = match mods.ctrl || mods.shift {
                true => model.picked.clone(),
                false => Default::default(),
            };
            pick_entry(model, index, right, clicks, mods, at);
            model.dragging = Some(Drag::Lasso { from: Vec2::new(at.x, at.y + model.scroll), at, keep });
        }
        Hit::Entry(index) | Hit::Strip(index) => pick_entry(model, index, right, clicks, mods, at),
        Hit::Space => {
            if !mods.ctrl && !mods.shift {
                model.pick_none();
            }
            if right {
                let items = model.space_menu();
                model.menu = Some(menu_at(items, at));
            } else if matches!(model.view.mode, Mode::List | Mode::Icons) && clicks == 1 {
                let keep = match mods.ctrl || mods.shift {
                    true => model.picked.clone(),
                    false => Default::default(),
                };
                model.dragging = Some(Drag::Lasso { from: Vec2::new(at.x, at.y + model.scroll), at, keep });
            }
        }
        Hit::Column(column, row) => press_column(model, column, row, right, clicks, mods, at),
        Hit::Zoom(share) => {
            zoom_share(model, share);
            model.dragging = Some(Drag::Zoom);
        }
        Hit::Picture => {
            if clicks >= 2 {
                model.act(Act::Open);
            }
        }
    }
}

fn set_rail(model: &mut Model, rail: Rail, to: f64, most: f64) {
    let to = to.round().clamp(0.0, most.max(0.0).ceil());
    match rail {
        Rail::Body => (model.scroll, model.scroll_rest) = (to, 0.0),
        Rail::Tree => (model.tree_scroll, model.tree_rest) = (to, 0.0),
        Rail::Column(index) => {
            if let Some(column) = model.columns.get_mut(index) {
                column.scroll = to;
            }
        }
    }
}

fn rail_scroll(model: &Model, rail: Rail) -> f64 {
    match rail {
        Rail::Body => model.scroll,
        Rail::Tree => model.tree_scroll,
        Rail::Column(index) => model.columns.get(index).map_or(0.0, |column| column.scroll),
    }
}

fn press_middle(model: &mut Model, ui: &Ui, lines: &[Line], hit: Hit) {
    let folder = match hit {
        Hit::Tab(index) | Hit::TabClose(index) => return model.close_tab(index),
        Hit::Entry(index) | Hit::Strip(index) => model.entries.get(index).filter(|one| one.folder).map(|one| one.path.clone()),
        Hit::Column(column, Some(row)) => model.columns.get(column).and_then(|c| c.entries.get(row)).filter(|one| one.folder).map(|one| one.path.clone()),
        Hit::Line(index) => lines.get(index).and_then(Line::path).map(Path::to_path_buf),
        Hit::Crumb(index) => {
            let area = ui.look.path_rect(ui.look.frame(ui.size, model).bar);
            ui.look.crumbs(area, model, ui.fonts).get(index).map(|crumb| crumb.path.clone())
        }
        _ => None,
    };
    if let Some(folder) = folder {
        model.new_tab(folder, false);
    }
}

fn zoom_share(model: &mut Model, share: f64) {
    let span = f64::from(prefs::MOST_ZOOM - prefs::LEAST_ZOOM);
    let zoom = f64::from(prefs::LEAST_ZOOM) + span * share;
    model.zoom_to((zoom / 4.0).round() as u32 * 4);
}

fn menu_at(items: Vec<super::model::Item>, at: Vec2) -> Menu {
    Menu { items, at, hover: None, open: None, kid_hover: None, bar: None }
}

fn open_bar(model: &mut Model, ui: &Ui, index: usize) {
    let titles = ui.look.bar_titles(ui.fonts);
    let Some((area, _)) = titles.get(index) else {
        return;
    };
    let items = model.bar_menu(index);
    model.menu = Some(Menu { items, at: Vec2::new(area.origin.x, area.max().y + 3.0), hover: None, open: None, kid_hover: None, bar: Some(index) });
}

fn pick_entry(model: &mut Model, index: usize, right: bool, clicks: u32, mods: Mods, at: Vec2) {
    let Some(path) = model.entries.get(index).map(|each| each.path.clone()) else {
        return;
    };
    if right {
        if !model.picked.contains(&path) {
            model.pick_only(path);
        }
        let items = model.item_menu();
        model.menu = Some(menu_at(items, at));
        return;
    }
    match (mods.ctrl, mods.shift) {
        (true, false) => model.pick_toggle(path.clone()),
        (_, true) => model.pick_range(index, mods.ctrl),
        _ => {
            if clicks >= 2 && model.picked.contains(&path) {
                return model.act(Act::Open);
            }
            let among = model.picked.contains(&path) && model.picked.len() > 1;
            if !model.picked.contains(&path) {
                model.pick_only(path.clone());
            }
            if model.prefs.click == Click::Single && clicks == 1 {
                return model.act(Act::Open);
            }
            if clicks == 1 {
                let pending = among.then(|| path.clone());
                model.dragging = Some(Drag::Files { paths: model.chosen_paths(), from: at, at, live: false, pending });
            }
            return;
        }
    }
    if model.picked.contains(&path) && clicks == 1 {
        model.dragging = Some(Drag::Files { paths: model.chosen_paths(), from: at, at, live: false, pending: None });
    }
}

fn press_column(model: &mut Model, column: usize, row: Option<usize>, right: bool, clicks: u32, mods: Mods, at: Vec2) {
    let Some(dir) = model.columns.get(column).map(|c| c.dir.clone()) else {
        return;
    };
    let path = row.and_then(|row| model.columns.get(column)?.entries.get(row)).map(|each| each.path.clone());
    if dir != model.dir {
        let back = model.dir.clone();
        model.go(dir.clone(), path.clone());
        if model.dir == dir && model.back.last() == Some(&back) && !model.dir.starts_with(&back) {
            model.back.pop();
        }
    }
    let Some(path) = path else {
        if !mods.ctrl && !mods.shift {
            model.pick_none();
        }
        if right {
            let items = model.space_menu();
            model.menu = Some(menu_at(items, at));
        }
        return;
    };
    let Some(index) = model.index_of(&path) else {
        return;
    };
    pick_entry(model, index, right, clicks, mods, at);
}

fn press_menu(model: &mut Model, ui: &Ui, lines: &[Line], at: Vec2) {
    let Some(menu) = model.menu.clone() else {
        return;
    };
    match ui.look.menu_hit(ui.size, &menu, ui.fonts, at) {
        Some((true, index)) => {
            let item = menu.open.and_then(|open| menu.items.get(open)).and_then(|parent| parent.kids.get(index)).cloned();
            if let Some(item) = item.filter(|item| item.on)
                && let Some(act) = item.act
            {
                model.act(act);
            }
        }
        Some((false, index)) => {
            let Some(item) = menu.items.get(index).cloned() else {
                return;
            };
            if !item.on {
                return;
            }
            match item.act {
                Some(act) => model.act(act),
                None => {
                    if let Some(open) = model.menu.as_mut() {
                        open.open = Some(index);
                    }
                }
            }
        }
        None => {
            let hit = ui.look.hit(ui.size, model, lines, ui.fonts, at);
            match (hit, menu.bar) {
                (Some(Hit::Bar(index)), Some(open)) if index == open => model.menu = None,
                (Some(Hit::Bar(index)), _) => open_bar(model, ui, index),
                _ => model.menu = None,
            }
        }
    }
}

fn press_sheet(model: &mut Model, ui: &Ui, at: Vec2, clicks: u32) {
    let Some(hit) = ui.look.sheet_hit(ui.size, model, ui.fonts, at) else {
        return;
    };
    match hit {
        SheetHit::Name => model.act(Act::NameInInfo),
        SheetHit::Outside => {
            model.act(Act::NamedInInfo);
            if !matches!(model.sheet, Some(Sheet::Confirm { .. } | Sheet::Clash { .. })) {
                model.sheet = None;
                model.save();
            }
        }
        SheetHit::Done | SheetHit::No => {
            model.act(Act::NamedInInfo);
            if matches!(&model.sheet, Some(Sheet::Info(info)) if info.naming.is_some()) {
                return;
            }
            model.sheet = None;
            model.save();
        }
        SheetHit::Inside => model.act(Act::NamedInInfo),
        SheetHit::Yes => match model.sheet.clone() {
            Some(Sheet::Confirm { act, .. }) => {
                model.act(act);
                model.sheet = None;
            }
            Some(Sheet::Apps(_)) => model.act(Act::ChooseApp),
            _ => {}
        },
        SheetHit::App(found) => {
            model.act(Act::PickApp(found));
            if clicks >= 2 {
                model.act(Act::ChooseApp);
            }
        }
        SheetHit::Always => model.act(Act::AlwaysApp),
        SheetHit::Lock => model.act(Act::Lock),
        SheetHit::Runs => model.act(Act::Runs),
        SheetHit::ChangeApp => model.act(Act::ChangeApp),
        SheetHit::Clash(clash) => model.act(Act::Clash(clash)),
        SheetHit::Tab(tab) => model.sheet = Some(Sheet::Prefs(tab)),
        SheetHit::Pref(pref) => model.set(pref),
        SheetHit::Slider(share) => {
            zoom_share(model, share);
            model.dragging = Some(Drag::Slider);
        }
    }
}

pub fn release(model: &mut Model, ui: &Ui, lines: &[Line], at: Vec2, copy: bool) {
    match model.dragging.take() {
        Some(Drag::Files { paths, live: true, .. }) => {
            if let Some((target, _)) = ui.look.drop_at(ui.size, model, lines, ui.fonts, at, &paths) {
                model.drop_on(target, paths, copy);
            }
        }
        Some(Drag::Files { pending: Some(path), .. }) => model.pick_only(path),
        Some(Drag::Files { .. }) | None => {}
        Some(_) => model.save(),
    }
}

pub fn motion(model: &mut Model, ui: &Ui, lines: &[Line], at: Vec2) -> bool {
    match model.dragging.clone() {
        Some(Drag::Files { paths, from, live, pending, .. }) => {
            let live = live || (at - from).length() > 6.0;
            model.dragging = Some(Drag::Files { paths, from, at, live, pending });
            return live;
        }
        Some(Drag::Split { from, width }) => {
            let wide = (f64::from(width) + at.x - from).round().clamp(f64::from(prefs::LEAST_TREE), f64::from(prefs::MOST_TREE)) as u32;
            let changed = wide != model.prefs.tree_width;
            model.prefs.tree_width = wide;
            model.saved = false;
            return changed;
        }
        Some(Drag::Text(spot)) => {
            let Some((area, font)) = ui.look.field_area(ui.size, model, ui.fonts, spot) else {
                return false;
            };
            let Some(field) = field_of(model, spot) else {
                return false;
            };
            let before = (field.caret, field.anchor);
            let caret = ui.look.field_caret(font, field, area, at.x);
            field.caret_at(caret, true);
            return before != (field.caret, field.anchor);
        }
        Some(Drag::Lasso { from, keep, .. }) => {
            let frame = ui.look.frame(ui.size, model);
            let top = frame.body.origin.y + if model.view.mode == Mode::List { super::look::HEAD_H } else { 0.0 };
            let most = ui.look.most_scroll(&frame, model).ceil();
            let push = match (at.y < top, at.y > frame.body.max().y) {
                (true, _) => at.y - top,
                (_, true) => at.y - frame.body.max().y,
                _ => 0.0,
            };
            if push != 0.0 {
                (model.scroll, model.scroll_rest) = ((model.scroll + push.clamp(-40.0, 40.0)).round().clamp(0.0, most), 0.0);
            }
            let at = Vec2::new(at.x.clamp(frame.body.origin.x, frame.body.max().x), at.y);
            model.dragging = Some(Drag::Lasso { from, at, keep: keep.clone() });
            let Some(area) = ui.look.lasso_box(model) else {
                return true;
            };
            let mut picked = keep;
            let mut last = None;
            for index in ui.look.lasso_hits(&frame, model, area) {
                let path = model.entries[index].path.clone();
                picked.insert(path.clone());
                last = Some(path);
            }
            if picked != model.picked {
                model.picked = picked;
                if let Some(last) = last {
                    model.focus = Some(last.clone());
                    model.anchor = Some(last);
                }
                model.refresh_peek();
            }
            return true;
        }
        Some(Drag::Edge { sort, rest }) => {
            let body = ui.look.frame(ui.size, model).body;
            let (name, extra) = ui.look.list_widths(body, model);
            let now = extra.iter().find(|(each, _)| *each == sort).map_or(0.0, |(_, width)| *width);
            let room = (name - 200.0).max(0.0) + now;
            let wanted = (body.max().x - 8.0 - rest - at.x).min(room).round();
            let before = model.prefs.widths;
            model.prefs.widths.set(sort, Some(wanted.max(0.0) as u32));
            let changed = before != model.prefs.widths;
            model.saved = model.saved && !changed;
            return changed;
        }
        Some(Drag::Rail { rail, grab }) => {
            let Some((area, content, scroll)) = ui.look.rail_place(ui.size, model, lines, rail) else {
                return false;
            };
            let thumb = super::look::rail_thumb(area, content, scroll);
            let room = area.size.y - thumb.size.y;
            if room <= 0.0 {
                return false;
            }
            let share = ((at.y - grab - area.origin.y) / room).clamp(0.0, 1.0);
            let before = rail_scroll(model, rail);
            set_rail(model, rail, share * (content - area.size.y), content - area.size.y);
            return before != rail_scroll(model, rail);
        }
        Some(Drag::Zoom) => {
            let before = model.prefs.zoom;
            if let Some(share) = ui.look.zoom_share(ui.size, model, ui.fonts, at) {
                zoom_share(model, share);
            }
            return before != model.prefs.zoom;
        }
        Some(Drag::Slider) => {
            let before = model.prefs.zoom;
            if let Some(share) = ui.look.slider_share(ui.size, model, at) {
                zoom_share(model, share);
            }
            return before != model.prefs.zoom;
        }
        Some(Drag::Foreign { .. }) => return false,
        None => {}
    }
    if let Some(menu) = model.menu.clone() {
        let found = ui.look.menu_hit(ui.size, &menu, ui.fonts, at);
        let mut next = menu.clone();
        match found {
            Some((true, index)) => next.kid_hover = Some(index),
            Some((false, index)) => {
                next.hover = Some(index);
                next.kid_hover = None;
                let has_kids = menu.items.get(index).is_some_and(|item| !item.kids.is_empty() && item.on);
                next.open = has_kids.then_some(index);
            }
            None => {
                next.kid_hover = None;
                if !ui.look.menu_contains(ui.size, &menu, ui.fonts, at) {
                    next.hover = menu.open;
                }
                if menu.bar.is_some()
                    && let Some(Hit::Bar(index)) = ui.look.hit(ui.size, model, lines, ui.fonts, at)
                    && Some(index) != menu.bar
                {
                    open_bar(model, ui, index);
                    return true;
                }
            }
        }
        let changed = next != menu;
        model.menu = Some(next);
        return changed;
    }
    if model.sheet.is_some() {
        return false;
    }
    let hover = ui.look.hit(ui.size, model, lines, ui.fonts, at).filter(|hit| !matches!(hit, Hit::Space | Hit::Picture | Hit::Path | Hit::Split | Hit::Zoom(_) | Hit::Banner | Hit::Jobs | Hit::Search | Hit::Tabs));
    let changed = hover != model.hover;
    if changed {
        model.tip = None;
    }
    model.hover = hover;
    changed
}

pub fn leave(model: &mut Model) -> bool {
    let tip = model.tip.take().is_some();
    model.hover.take().is_some() || tip
}

fn snap(wanted: f64, most: f64) -> (f64, f64) {
    let to = wanted.round().clamp(0.0, most);
    let rest = match to == wanted.round() {
        true => wanted - to,
        false => 0.0,
    };
    (to, rest)
}

pub fn scroll(model: &mut Model, ui: &Ui, lines: &[Line], at: Vec2, dx: f64, dy: f64, ctrl: bool) -> bool {
    model.tip = None;
    if let Some(Sheet::Apps(chooser)) = model.sheet.as_mut() {
        let most = chooser.apps.len().saturating_sub(ui.look.apps_shown(ui.size)) as f64;
        let before = chooser.scroll;
        chooser.scroll = (chooser.scroll + (dy / super::look::APP_ROW).round().clamp(-3.0, 3.0)).clamp(0.0, most);
        return chooser.scroll != before;
    }
    if model.menu.is_some() || model.sheet.is_some() {
        return false;
    }
    let frame = ui.look.frame(ui.size, model);
    if let Some(side) = frame.side
        && side.contains(at)
    {
        let most = (ui.look.tree_height(lines) - side.size.y).max(0.0).ceil();
        let before = model.tree_scroll;
        let (to, rest) = snap(model.tree_scroll + dy + model.tree_rest, most);
        (model.tree_scroll, model.tree_rest) = (to, rest);
        return before != model.tree_scroll;
    }
    if !frame.body.contains(at) {
        return false;
    }
    if ctrl {
        let before = model.prefs.zoom;
        match dy < 0.0 {
            true => model.act(Act::ZoomIn),
            false => model.act(Act::ZoomOut),
        }
        return before != model.prefs.zoom;
    }
    match model.view.mode {
        Mode::List | Mode::Icons => {
            let most = ui.look.most_scroll(&frame, model).ceil();
            let before = model.scroll;
            let (to, rest) = snap(model.scroll + dy + model.scroll_rest, most);
            (model.scroll, model.scroll_rest) = (to, rest);
            before != model.scroll
        }
        Mode::Columns => {
            let body = frame.body;
            let found = (0..model.columns.len()).find(|index| ui.look.column_rect(body, model, *index).contains(at));
            let Some(index) = found else {
                return false;
            };
            let column = &mut model.columns[index];
            let most = (12.0 + column.entries.len() as f64 * super::look::ROW_H - body.size.y).max(0.0);
            let before = column.scroll;
            column.scroll = (column.scroll + dy).clamp(0.0, most);
            before != column.scroll
        }
        Mode::Gallery => {
            let way = if dx.abs() > dy.abs() { dx } else { dy };
            if way.abs() < 1.0 {
                return false;
            }
            model.step(if way > 0.0 { 1 } else { -1 }, false);
            true
        }
    }
}

fn side_key(model: &mut Model, ui: &Ui, key: &Key) -> bool {
    let lines = model.lines();
    let usable: Vec<usize> = (0..lines.len()).filter(|at| !matches!(lines[*at], Line::Head(_))).collect();
    let Some(&first) = usable.first() else {
        return false;
    };
    let at = model.side_at.filter(|at| usable.contains(at)).unwrap_or(first);
    let place = usable.iter().position(|each| *each == at).unwrap_or(0);
    let to = match key.named {
        Named::Up => usable[place.saturating_sub(1)],
        Named::Down => usable[(place + 1).min(usable.len() - 1)],
        Named::Home => first,
        Named::End => usable[usable.len() - 1],
        Named::PageUp => usable[place.saturating_sub(10)],
        Named::PageDown => usable[(place + 10).min(usable.len() - 1)],
        Named::Right => match &lines[at] {
            Line::Folder { path, open: false, .. } => {
                let path = path.clone();
                model.fold(&path);
                at
            }
            Line::Folder { depth, open: true, .. } => match lines.get(at + 1) {
                Some(Line::Folder { depth: next, .. }) if next > depth => at + 1,
                _ => at,
            },
            _ => at,
        },
        Named::Left => match &lines[at] {
            Line::Folder { path, open: true, .. } => {
                let path = path.clone();
                model.fold(&path);
                at
            }
            Line::Folder { depth, .. } if *depth > 0 => (0..at).rev().find(|each| matches!(&lines[*each], Line::Folder { depth: up, .. } if up + 1 == *depth)).unwrap_or(at),
            _ => at,
        },
        Named::Enter | Named::Letter(' ') => {
            model.ring = true;
            match lines[at].clone() {
                Line::Drive { path: None, drive: Some(drive), .. } => {
                    if let Some(device) = drive.device {
                        model.act(Act::Mount(device));
                    }
                }
                line => {
                    if let Some(path) = line.path() {
                        model.go(path.to_path_buf(), None);
                    }
                }
            }
            return true;
        }
        Named::Menu => {
            let Some(side) = ui.look.frame(ui.size, model).side else {
                return true;
            };
            let rects = ui.look.line_rects(side, &lines, model.tree_scroll);
            let spot = rects.get(at).map_or(Vec2::new(40.0, 120.0), |area| Vec2::new(area.origin.x + 20.0, area.max().y));
            let items = match lines[at].clone() {
                Line::Drive { drive: Some(drive), .. } => model.drive_menu(&drive),
                line => match line.path() {
                    Some(path) => model.line_menu(path, matches!(line, Line::Place { mark: true, .. })),
                    None => return true,
                },
            };
            model.menu = Some(menu_at(items, spot));
            return true;
        }
        Named::Escape if model.alerts.is_empty() && model.search.is_none() => {
            model.pane = Pane::Files;
            return true;
        }
        _ => return false,
    };
    model.ring = true;
    model.side_at = Some(to);
    show_side(model, ui);
    true
}

fn show_side(model: &mut Model, ui: &Ui) {
    let Some(side) = ui.look.frame(ui.size, model).side else {
        return;
    };
    let lines = model.lines();
    let Some(area) = model.side_at.and_then(|at| ui.look.line_rects(side, &lines, model.tree_scroll).get(at).copied()) else {
        return;
    };
    if area.origin.y < side.origin.y + 6.0 {
        model.tree_scroll -= side.origin.y + 6.0 - area.origin.y;
    } else if area.max().y > side.max().y - 6.0 {
        model.tree_scroll += area.max().y - side.max().y + 6.0;
    }
    let most = (ui.look.tree_height(&lines) - side.size.y).max(0.0).ceil();
    model.tree_scroll = model.tree_scroll.round().clamp(0.0, most);
    model.tree_rest = 0.0;
}

pub fn show_focus(model: &mut Model, ui: &Ui) {
    let frame = ui.look.frame(ui.size, model);
    let body = frame.body;
    let Some(at) = model.focus_index() else {
        return;
    };
    match model.view.mode {
        Mode::List | Mode::Icons => {
            let Some(area) = ui.look.entry_rect(&frame, model, at) else {
                return;
            };
            let top = body.origin.y + if model.view.mode == Mode::List { super::look::HEAD_H } else { 0.0 };
            if area.origin.y < top {
                model.scroll -= top - area.origin.y;
            } else if area.max().y > body.max().y {
                model.scroll += area.max().y - body.max().y;
            }
            let most = ui.look.most_scroll(&frame, model);
            model.scroll = model.scroll.clamp(0.0, most);
        }
        Mode::Columns => {
            let dir = model.dir.clone();
            if let Some(column) = model.columns.iter_mut().find(|column| column.dir == dir) {
                let top = 6.0 + at as f64 * super::look::ROW_H;
                if top < column.scroll {
                    column.scroll = top - 6.0;
                } else if top + super::look::ROW_H > column.scroll + body.size.y {
                    column.scroll = top + super::look::ROW_H - body.size.y + 6.0;
                }
                column.scroll = column.scroll.max(0.0);
            }
        }
        Mode::Gallery => {}
    }
}

fn edit(field: &mut super::field::Field, key: &Key, pasted: Option<&str>) -> Option<Option<String>> {
    match key.named {
        Named::Left => field.left(key.ctrl, key.shift),
        Named::Right => field.right(key.ctrl, key.shift),
        Named::Home => field.home(key.shift),
        Named::End => field.end(key.shift),
        Named::Back => field.back(key.ctrl),
        Named::Delete => field.forward_delete(key.ctrl),
        Named::Letter('a') if key.ctrl => field.pick_all(),
        Named::Letter('c') if key.ctrl => return Some(field.picked_text().map(str::to_string)),
        Named::Letter('x') if key.ctrl => return Some(field.cut()),
        Named::Letter('v') if key.ctrl => field.put(pasted?.lines().next().unwrap_or("")),
        _ => match key.text.as_deref() {
            Some(text) if !key.ctrl && !key.alt && !text.chars().any(char::is_control) => field.put(text),
            _ => return None,
        },
    }
    Some(None)
}

pub fn key(model: &mut Model, ui: &Ui, key: &Key) {
    model.tip = None;
    let pasted = model.clip_text.clone();
    if model.sheet.is_some() {
        return sheet_key(model, ui, key);
    }
    if model.menu.is_some() {
        return menu_key(model, ui, key);
    }
    if let Some((_, field)) = model.renaming.as_mut() {
        match key.named {
            Named::Enter | Named::Tab => model.finish_rename(),
            Named::Escape => model.renaming = None,
            _ => {
                if let Some(Some(text)) = edit(field, key, pasted.as_deref()) {
                    model.hold(super::clip::Held::Text(text));
                }
            }
        }
        return;
    }
    if let Some(search) = model.search.as_mut().filter(|search| search.focused) {
        match key.named {
            Named::Escape => model.act(Act::EndSearch),
            Named::Enter | Named::Down | Named::Tab => {
                search.focused = false;
                if let Some(first) = model.entries.first().map(|each| each.path.clone()) {
                    model.pick_only(first);
                }
            }
            _ => {
                let before = search.field.text.clone();
                if let Some(Some(text)) = edit(&mut search.field, key, pasted.as_deref()) {
                    model.hold(super::clip::Held::Text(text));
                }
                if model.search.as_ref().is_some_and(|search| search.field.text != before) {
                    model.search_changed();
                }
            }
        }
        return;
    }
    if let Some(field) = model.path.as_mut() {
        match key.named {
            Named::Enter => model.finish_path(),
            Named::Escape => model.path = None,
            Named::Tab => {
                if let Some(done) = super::model::complete(&field.text, &model.dir, &model.home, model.prefs.hidden) {
                    *field = super::field::Field::new(&done);
                }
            }
            _ => {
                if let Some(Some(text)) = edit(field, key, pasted.as_deref()) {
                    model.hold(super::clip::Held::Text(text));
                }
            }
        }
        return;
    }
    let side_on = model.prefs.tree || model.prefs.places || model.prefs.drives;
    if key.named == Named::Tab && !key.ctrl && !key.alt {
        model.ring = true;
        model.pane = match (model.pane, side_on) {
            (Pane::Files, true) => Pane::Side,
            _ => Pane::Files,
        };
        if model.pane == Pane::Side {
            let lines = model.lines();
            let here = lines.iter().position(|line| matches!(line, Line::Folder { path, .. } if path == &model.dir)).or_else(|| lines.iter().position(|line| line.path() == Some(model.dir.as_path())));
            let start = model.side_at.filter(|at| lines.get(*at).is_some_and(|line| !matches!(line, Line::Head(_))));
            model.side_at = start.or(here).or_else(|| lines.iter().position(|line| !matches!(line, Line::Head(_))));
            show_side(model, ui);
        }
        return;
    }
    if model.pane == Pane::Side && side_on && !key.ctrl && !key.alt && side_key(model, ui, key) {
        return;
    }
    if key.ctrl {
        let act = match key.named {
            Named::Letter('n') if key.shift => Some(Act::NewFolder),
            Named::Letter('f') => Some(Act::Search),
            Named::Letter('n') => Some(Act::NewWindow),
            Named::Letter('t') => Some(Act::NewTab),
            Named::Letter('w') => Some(Act::CloseTab(model.tab)),
            Named::Letter('q') => Some(Act::Close),
            Named::Tab => Some(Act::NextTab(if key.shift { -1 } else { 1 })),
            Named::PageDown => Some(Act::NextTab(1)),
            Named::PageUp => Some(Act::NextTab(-1)),
            Named::Letter('l') => Some(Act::EditPath),
            Named::Letter('c') if key.shift => Some(Act::CopyPath),
            Named::Letter('c') => Some(Act::Copy),
            Named::Letter('x') => Some(Act::Cut),
            Named::Letter('v') => Some(Act::Paste),
            Named::Letter('a') => Some(Act::PickAll),
            Named::Letter('z') => Some(Act::Undo),
            Named::Letter('d') => Some(Act::Duplicate),
            Named::Letter('h') => Some(Act::Hidden),
            Named::Letter('i') => Some(Act::Info),
            Named::Letter('r') => Some(Act::Reload),
            Named::Letter(',') => Some(Act::Prefs),
            Named::Letter('/') | Named::Letter('?') => Some(Act::Keys),
            Named::Letter('1') => Some(Act::Show(Mode::List)),
            Named::Letter('2') => Some(Act::Show(Mode::Icons)),
            Named::Letter('3') => Some(Act::Show(Mode::Columns)),
            Named::Letter('4') => Some(Act::Show(Mode::Gallery)),
            Named::Letter('+') | Named::Letter('=') => Some(Act::ZoomIn),
            Named::Letter('-') => Some(Act::ZoomOut),
            _ => None,
        };
        if let Some(act) = act {
            model.act(act);
            show_focus(model, ui);
        }
        return;
    }
    if key.alt {
        let act = match key.named {
            Named::Left => Some(Act::Back),
            Named::Right => Some(Act::Forward),
            Named::Up => Some(Act::Up),
            Named::Down => Some(Act::Open),
            Named::Letter(digit @ '1'..='9') if model.tabs.len() > 1 => Some(match digit {
                '9' => Act::ShowTab(model.tabs.len() - 1),
                _ => Act::ShowTab(digit as usize - '1' as usize),
            }),
            _ => None,
        };
        if let Some(act) = act {
            model.act(act);
            show_focus(model, ui);
        }
        return;
    }
    let mode = model.view.mode;
    let frame = ui.look.frame(ui.size, model);
    let (cols, _, _) = ui.look.grid(frame.body, model);
    let page = match mode {
        Mode::Icons => cols * ((frame.body.size.y / ui.look.grid(frame.body, model).1.y).floor() as usize).max(1),
        _ => ((frame.body.size.y / super::look::ROW_H).floor() as usize).max(1),
    } as isize;
    let across = if mode == Mode::Icons { cols as isize } else { 1 };
    if matches!(key.named, Named::Up | Named::Down | Named::Left | Named::Right | Named::Home | Named::End | Named::PageUp | Named::PageDown) {
        model.ring = true;
        model.pane = Pane::Files;
    }
    match key.named {
        Named::Up => model.step(-across, key.shift),
        Named::Down => model.step(across, key.shift),
        Named::Left if mode == Mode::Columns => model.act(Act::Up),
        Named::Right if mode == Mode::Columns => {
            if let Some(folder) = model.single().filter(|one| one.folder).map(|one| one.path.clone()) {
                model.go(folder, None);
                model.step(1, false);
            }
        }
        Named::Left if mode != Mode::List => model.step(-1, key.shift),
        Named::Right if mode != Mode::List => model.step(1, key.shift),
        Named::Home => model.step(-(model.entries.len() as isize), key.shift),
        Named::End => model.step(model.entries.len() as isize, key.shift),
        Named::PageUp => model.step(-page, key.shift),
        Named::PageDown => model.step(page, key.shift),
        Named::Enter => model.act(Act::Open),
        Named::Escape if !model.alerts.is_empty() => model.act(Act::Dismiss),
        Named::Escape if model.search.is_some() => model.act(Act::EndSearch),
        Named::Escape => {
            model.pick_none();
            model.clip = model.clip.take().filter(|(_, cut)| !cut);
        }
        Named::Back => model.act(Act::Up),
        Named::Delete if key.shift => model.act(Act::DeleteForever),
        Named::Delete => model.act(Act::Trash),
        Named::F2 => model.act(Act::Rename),
        Named::F5 => model.act(Act::Reload),
        Named::F9 => model.act(Act::Tree),
        Named::Menu => {
            let center = frame.body.center();
            let items = match model.picked.is_empty() {
                true => model.space_menu(),
                false => model.item_menu(),
            };
            model.menu = Some(menu_at(items, center));
        }
        _ => {
            if let Some(text) = key.text.as_deref().filter(|text| !text.trim().is_empty() && !text.chars().any(char::is_control)) {
                model.type_to_find(text);
            }
        }
    }
    show_focus(model, ui);
}

fn sheet_key(model: &mut Model, ui: &Ui, key: &Key) {
    if matches!(&model.sheet, Some(Sheet::Info(info)) if info.naming.is_some()) {
        let pasted = model.clip_text.clone();
        let naming = match model.sheet.as_mut() {
            Some(Sheet::Info(info)) => &mut info.naming,
            _ => return,
        };
        match key.named {
            Named::Enter => model.act(Act::NamedInInfo),
            Named::Escape => *naming = None,
            _ => {
                let copied = naming.as_mut().and_then(|field| edit(field, key, pasted.as_deref()));
                if let Some(Some(text)) = copied {
                    model.hold(super::clip::Held::Text(text));
                }
            }
        }
        return;
    }
    if let Some(Sheet::Apps(chooser)) = model.sheet.as_mut() {
        let last = chooser.apps.len().saturating_sub(1);
        let room = ui.look.apps_shown(ui.size);
        let to = match key.named {
            Named::Down => Some(chooser.at.map_or(0, |at| (at + 1).min(last))),
            Named::Up => Some(chooser.at.map_or(0, |at| at.saturating_sub(1))),
            Named::PageDown => Some(chooser.at.map_or(0, |at| (at + room).min(last))),
            Named::PageUp => Some(chooser.at.map_or(0, |at| at.saturating_sub(room))),
            Named::Home => Some(0),
            Named::End => Some(last),
            _ => None,
        };
        if let Some(to) = to.filter(|_| !chooser.apps.is_empty()) {
            chooser.at = Some(to);
            let first = chooser.scroll as usize;
            if to < first {
                chooser.scroll = to as f64;
            } else if to >= first + room {
                chooser.scroll = (to + 1 - room) as f64;
            }
            return;
        }
        match key.named {
            Named::Enter => model.act(Act::ChooseApp),
            Named::Letter(' ') => model.act(Act::AlwaysApp),
            Named::Escape => model.sheet = None,
            _ => {}
        }
        return;
    }
    match (&model.sheet, key.named) {
        (Some(Sheet::Confirm { act, .. }), Named::Enter) => {
            let act = act.clone();
            model.act(act);
            model.sheet = None;
        }
        (Some(Sheet::Clash { .. }), Named::Enter) => model.act(Act::Clash(super::ops::Clash::KeepBoth)),
        (Some(Sheet::Prefs(tab)), Named::Down | Named::Tab) => model.sheet = Some(Sheet::Prefs((tab + 1) % 4)),
        (Some(Sheet::Prefs(tab)), Named::Up) => model.sheet = Some(Sheet::Prefs((tab + 3) % 4)),
        (_, Named::Escape | Named::Enter) => {
            model.sheet = None;
            model.save();
        }
        (_, Named::Letter(',')) if key.ctrl => {
            model.sheet = None;
            model.save();
        }
        _ => {}
    }
}

fn menu_key(model: &mut Model, ui: &Ui, key: &Key) {
    let Some(menu) = model.menu.as_mut() else {
        return;
    };
    let in_kids = menu.open.is_some() && menu.kid_hover.is_some();
    let items: Vec<super::model::Item> = match in_kids {
        true => menu.open.and_then(|open| menu.items.get(open)).map(|item| item.kids.clone()).unwrap_or_default(),
        false => menu.items.clone(),
    };
    let now = if in_kids { menu.kid_hover } else { menu.hover };
    let step = |from: Option<usize>, way: isize| -> Option<usize> {
        let count = items.len() as isize;
        if count == 0 {
            return None;
        }
        let mut at = from.map_or(if way > 0 { -1 } else { count }, |at| at as isize);
        for _ in 0..count {
            at = (at + way).rem_euclid(count);
            if items[at as usize].on && !items[at as usize].rule {
                return Some(at as usize);
            }
        }
        None
    };
    match key.named {
        Named::Escape => {
            match in_kids {
                true => menu.kid_hover = None,
                false => model.menu = None,
            }
        }
        Named::Down | Named::Up => {
            let way = if key.named == Named::Down { 1 } else { -1 };
            let next = step(now, way);
            match in_kids {
                true => menu.kid_hover = next,
                false => {
                    menu.hover = next;
                    menu.open = None;
                }
            }
        }
        Named::Right => {
            if let Some(at) = menu.hover.filter(|at| !menu.items[*at].kids.is_empty()) {
                menu.open = Some(at);
                menu.kid_hover = Some(0);
            } else if let Some(bar) = menu.bar {
                open_bar(model, ui, (bar + 1) % 5);
            }
        }
        Named::Left => {
            if in_kids {
                menu.kid_hover = None;
                menu.open = None;
            } else if let Some(bar) = menu.bar {
                open_bar(model, ui, (bar + 4) % 5);
            }
        }
        Named::Enter => {
            let chosen = now.and_then(|at| items.get(at)).cloned();
            match chosen {
                Some(item) if item.on => match item.act {
                    Some(act) => model.act(act),
                    None if !in_kids => {
                        menu.open = now;
                        menu.kid_hover = Some(0);
                    }
                    None => {}
                },
                _ => {}
            }
        }
        _ => {}
    }
}
