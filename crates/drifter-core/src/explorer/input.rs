use super::look::{Fonts, Hit, Look, SheetHit};
use super::model::{Act, Drag, Line, Menu, Model, Sheet};
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

fn caret_from_x(font: &crate::text::Font, text: &str, x: f64) -> usize {
    let mut best = (0, f64::MAX);
    for (at, _) in text.char_indices().chain(std::iter::once((text.len(), ' '))) {
        let gap = (font.width_of(&text[..at]) - x).abs();
        if gap < best.1 {
            best = (at, gap);
        }
    }
    best.0
}

pub fn press(model: &mut Model, ui: &Ui, lines: &[Line], at: Vec2, button: Button, clicks: u32, mods: Mods) {
    if model.menu.is_some() {
        return press_menu(model, ui, lines, at);
    }
    if model.sheet.is_some() {
        return press_sheet(model, ui, at);
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
            return;
        }
    }
    if model.path.is_some() && hit != Some(Hit::Path) {
        model.path = None;
    }
    let Some(hit) = hit else {
        return;
    };
    let right = button == Button::Right;
    match hit {
        Hit::Bar(index) => open_bar(model, ui, index),
        Hit::Back => model.act(Act::Back),
        Hit::Forward => model.act(Act::Forward),
        Hit::Up => model.act(Act::Up),
        Hit::Path => match model.path.is_some() {
            true => {
                let area = ui.look.path_rect(ui.look.frame(ui.size, model).bar);
                let Some(field) = model.path.as_mut() else {
                    return;
                };
                let caret = caret_from_x(ui.fonts.body, &field.text, at.x - area.origin.x - 10.0);
                match clicks {
                    2.. => field.pick_all(),
                    _ => field.caret_at(caret, mods.shift),
                }
            }
            false => model.act(Act::EditPath),
        },
        Hit::Show(mode) => model.act(Act::Show(mode)),
        Hit::Gear => model.act(Act::Prefs),
        Hit::Dismiss => model.act(Act::Dismiss),
        Hit::Stop(id) => model.act(Act::Stop(id)),
        Hit::Banner | Hit::Jobs => {}
        Hit::Split => model.dragging = Some(Drag::Split { from: at.x, width: model.prefs.tree_width }),
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
        Hit::Entry(index) | Hit::Strip(index) => pick_entry(model, index, right, clicks, mods, at),
        Hit::Space => {
            if !mods.ctrl && !mods.shift {
                model.pick_none();
            }
            if right {
                let items = model.space_menu();
                model.menu = Some(menu_at(items, at));
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

fn press_sheet(model: &mut Model, ui: &Ui, at: Vec2) {
    let Some(hit) = ui.look.sheet_hit(ui.size, model, ui.fonts, at) else {
        return;
    };
    match hit {
        SheetHit::Outside => {
            if !matches!(model.sheet, Some(Sheet::Confirm { .. } | Sheet::Clash { .. })) {
                model.sheet = None;
                model.save();
            }
        }
        SheetHit::Done | SheetHit::No => {
            model.sheet = None;
            model.save();
        }
        SheetHit::Yes => {
            if let Some(Sheet::Confirm { act, .. }) = model.sheet.clone() {
                model.act(act);
                model.sheet = None;
            }
        }
        SheetHit::Clash(clash) => model.act(Act::Clash(clash)),
        SheetHit::Tab(tab) => model.sheet = Some(Sheet::Prefs(tab)),
        SheetHit::Pref(pref) => model.set(pref),
        SheetHit::Slider(share) => {
            zoom_share(model, share);
            model.dragging = Some(Drag::Slider);
        }
        SheetHit::Inside => {}
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
    let hover = ui.look.hit(ui.size, model, lines, ui.fonts, at).filter(|hit| !matches!(hit, Hit::Space | Hit::Picture | Hit::Path | Hit::Split | Hit::Zoom(_) | Hit::Banner | Hit::Jobs));
    let changed = hover != model.hover;
    model.hover = hover;
    changed
}

pub fn leave(model: &mut Model) -> bool {
    model.hover.take().is_some()
}

pub fn scroll(model: &mut Model, ui: &Ui, lines: &[Line], at: Vec2, dx: f64, dy: f64, ctrl: bool) -> bool {
    if model.menu.is_some() || model.sheet.is_some() {
        return false;
    }
    let frame = ui.look.frame(ui.size, model);
    if let Some(side) = frame.side
        && side.contains(at)
    {
        let most = (ui.look.tree_height(lines) - side.size.y).max(0.0);
        let before = model.tree_scroll;
        model.tree_scroll = (model.tree_scroll + dy).clamp(0.0, most);
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
    match model.prefs.mode {
        Mode::List | Mode::Icons => {
            let most = ui.look.most_scroll(&frame, model);
            let before = model.scroll;
            model.scroll = (model.scroll + dy).clamp(0.0, most);
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

pub fn show_focus(model: &mut Model, ui: &Ui) {
    let frame = ui.look.frame(ui.size, model);
    let body = frame.body;
    let Some(at) = model.focus_index() else {
        return;
    };
    match model.prefs.mode {
        Mode::List | Mode::Icons => {
            let Some(area) = ui.look.entry_rect(&frame, model, at) else {
                return;
            };
            let top = body.origin.y + if model.prefs.mode == Mode::List { super::look::HEAD_H } else { 0.0 };
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

fn edit(field: &mut super::field::Field, key: &Key) -> Option<Option<String>> {
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
        _ => match key.text.as_deref() {
            Some(text) if !key.ctrl && !key.alt && !text.chars().any(char::is_control) => field.put(text),
            _ => return None,
        },
    }
    Some(None)
}

pub fn key(model: &mut Model, ui: &Ui, key: &Key) {
    if model.sheet.is_some() {
        return sheet_key(model, key);
    }
    if model.menu.is_some() {
        return menu_key(model, ui, key);
    }
    if let Some((_, field)) = model.renaming.as_mut() {
        match key.named {
            Named::Enter | Named::Tab => model.finish_rename(),
            Named::Escape => model.renaming = None,
            _ => {
                if let Some(Some(text)) = edit(field, key) {
                    model.effects.push(super::model::Effect::Clip(text));
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
                if let Some(Some(text)) = edit(field, key) {
                    model.effects.push(super::model::Effect::Clip(text));
                }
            }
        }
        return;
    }
    if key.ctrl {
        let act = match key.named {
            Named::Letter('n') if key.shift => Some(Act::NewFolder),
            Named::Letter('n') => Some(Act::NewWindow),
            Named::Letter('w') | Named::Letter('q') => Some(Act::Close),
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
            _ => None,
        };
        if let Some(act) = act {
            model.act(act);
            show_focus(model, ui);
        }
        return;
    }
    let mode = model.prefs.mode;
    let frame = ui.look.frame(ui.size, model);
    let (cols, _, _) = ui.look.grid(frame.body, model);
    let page = match mode {
        Mode::Icons => cols * ((frame.body.size.y / ui.look.grid(frame.body, model).1.y).floor() as usize).max(1),
        _ => ((frame.body.size.y / super::look::ROW_H).floor() as usize).max(1),
    } as isize;
    let across = if mode == Mode::Icons { cols as isize } else { 1 };
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

fn sheet_key(model: &mut Model, key: &Key) {
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
