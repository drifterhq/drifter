use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

#[derive(Clone)]
pub enum Kind {
    Action(u32),
    Submenu(Vec<Entry>),
    Rule,
    Disabled,
}

#[derive(Clone)]
pub struct Entry {
    pub label: String,
    pub icon: Option<Image>,
    pub kind: Kind,
}

impl Entry {
    pub fn action(id: u32, label: &str) -> Entry {
        Entry {
            label: label.to_string(),
            icon: None,
            kind: Kind::Action(id),
        }
    }

    pub fn submenu(label: &str, children: Vec<Entry>) -> Entry {
        Entry {
            label: label.to_string(),
            icon: None,
            kind: match children.is_empty() {
                true => Kind::Disabled,
                false => Kind::Submenu(children),
            },
        }
    }

    pub fn rule() -> Entry {
        Entry {
            label: String::new(),
            icon: None,
            kind: Kind::Rule,
        }
    }

    pub fn with_icon(mut self, icon: Option<Image>) -> Entry {
        self.icon = icon;
        self
    }

    fn pickable(&self) -> bool {
        matches!(self.kind, Kind::Action(_) | Kind::Submenu(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Escape,
    Letter(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Nothing,
    Changed,
    Activate(u32),
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub item: f64,
    pub rule: f64,
    pub pad: f64,
    pub radius: f64,
    pub icon: f64,
    pub least: f64,
    pub back: [u8; 3],
    pub back_alpha: f64,
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub rule_ink: [u8; 3],
    pub accent: [u8; 3],
    pub off: [u8; 3],
}

impl Look {
    pub fn themed(p: &crate::theme::Palette, accent: [u8; 3]) -> Look {
        Look {
            item: crate::menu::ROW,
            rule: crate::rows::Kind::Rule.height(),
            pad: crate::rows::PAD,
            radius: crate::menu::RADIUS,
            icon: crate::rows::GLYPH,
            least: 210.0,
            back: p.back,
            back_alpha: 0.96,
            ink: p.ink,
            dim: p.dim,
            rule_ink: p.rule,
            accent,
            off: p.off,
        }
    }

    fn row(&self, entry: &Entry) -> f64 {
        match entry.kind {
            Kind::Rule => self.rule,
            _ => self.item,
        }
    }

    fn content(&self, items: &[Entry]) -> f64 {
        items.iter().map(|entry| self.row(entry)).sum::<f64>() + self.pad * 2.0
    }

    fn style(&self) -> crate::rows::Style {
        crate::rows::Style {
            back: self.back,
            back_alpha: self.back_alpha,
            ink: self.ink,
            dim: self.dim,
            off: self.off,
            accent: self.accent,
            rule: self.ink,
            shade: self.rule_ink,
        }
    }

    fn rows(items: &[Entry]) -> Vec<(crate::rows::Kind, crate::rows::Lead, crate::rows::Trail, bool)> {
        items
            .iter()
            .map(|entry| {
                let lead = entry
                    .icon
                    .as_ref()
                    .map_or(crate::rows::Lead::None, |icon| crate::rows::Lead::Icon(std::sync::Arc::new(icon.clone())));
                match &entry.kind {
                    Kind::Rule => (crate::rows::Kind::Rule, crate::rows::Lead::None, crate::rows::Trail::None, false),
                    Kind::Submenu(_) => (crate::rows::Kind::Action, lead, crate::rows::Trail::Chevron, true),
                    Kind::Action(_) => (crate::rows::Kind::Action, lead, crate::rows::Trail::None, true),
                    Kind::Disabled => (crate::rows::Kind::Action, lead, crate::rows::Trail::None, false),
                }
            })
            .collect()
    }

    pub fn size(&self, items: &[Entry], font: &Font, screen: Vec2) -> Vec2 {
        let room = items.iter().any(|entry| entry.icon.is_some());
        let fonts = crate::rows::Fonts { body: font, small: font };
        let rows = Self::rows(items);
        let widest = items
            .iter()
            .zip(&rows)
            .map(|(entry, (kind, lead, trail, enabled))| {
                crate::rows::width_of(
                    &crate::rows::Row {
                        kind: *kind,
                        label: &entry.label,
                        note: "",
                        value: "",
                        lead,
                        trail,
                        enabled: *enabled,
                        lead_room: room,
                    },
                    &fonts,
                )
            })
            .fold(0.0f64, f64::max);
        let width = (widest + self.pad * 2.0).max(self.least).ceil();
        let height = self.content(items).min(screen.y - 24.0).ceil();
        Vec2::new(width, height)
    }

    pub fn item_top(&self, items: &[Entry], index: usize) -> f64 {
        self.pad
            + items
                .iter()
                .take(index)
                .map(|entry| self.row(entry))
                .sum::<f64>()
    }

    pub fn most_scroll(&self, items: &[Entry], size: Vec2) -> f64 {
        (self.content(items) - size.y).max(0.0)
    }

    pub fn index_at(
        &self,
        items: &[Entry],
        size: Vec2,
        scroll: f64,
        inside: Vec2,
    ) -> Option<usize> {
        if inside.x < 0.0 || inside.x > size.x || inside.y < 0.0 || inside.y > size.y {
            return None;
        }
        let y = inside.y + scroll;
        (0..items.len()).find(|at| {
            let top = self.item_top(items, *at);
            y >= top && y < top + self.row(&items[*at])
        })
    }

    pub fn paint(
        &self,
        items: &[Entry],
        size: Vec2,
        hover: Option<usize>,
        scroll: f64,
        font: &Font,
    ) -> Image {
        let mut canvas = Canvas::new(size.x as u32, size.y as u32);
        let style = self.style();
        crate::rows::back(&mut canvas, size, self.radius, &style);
        let room = items.iter().any(|entry| entry.icon.is_some());
        let fonts = crate::rows::Fonts { body: font, small: font };
        let rows = Self::rows(items);
        for (index, (entry, (kind, lead, trail, enabled))) in items.iter().zip(&rows).enumerate() {
            let top = self.item_top(items, index) - scroll;
            let tall = self.row(entry);
            if top + tall < 0.0 || top > size.y {
                continue;
            }
            let rect = Rect::new(Vec2::new(0.0, top), Vec2::new(size.x, tall));
            if hover == Some(index) && entry.pickable() {
                crate::rows::highlight(&mut canvas, rect, &style, 0.07);
            }
            crate::rows::paint_row(
                &mut canvas,
                rect,
                &crate::rows::Row {
                    kind: *kind,
                    label: &entry.label,
                    note: "",
                    value: "",
                    lead,
                    trail,
                    enabled: *enabled,
                    lead_room: room,
                },
                &style,
                &fonts,
            );
        }
        let most = self.most_scroll(items, size);
        if most > 0.0 {
            let dot = |canvas: &mut Canvas, y: f64| {
                canvas.circle(Vec2::new(size.x / 2.0, y), 2.5, self.dim, 0.9);
            };
            if scroll > 0.5 {
                dot(&mut canvas, 4.0);
            }
            if scroll < most - 0.5 {
                dot(&mut canvas, size.y - 4.0);
            }
        }
        canvas.image
    }
}

#[derive(Clone)]
pub struct Cascade {
    pub root: Vec<Entry>,
    pub path: Vec<usize>,
    pub hover: Vec<Option<usize>>,
    pub scroll: Vec<f64>,
    pub origin: Vec2,
}

impl Cascade {
    pub fn new(root: Vec<Entry>, origin: Vec2) -> Cascade {
        Cascade {
            root,
            path: Vec::new(),
            hover: vec![None],
            scroll: vec![0.0],
            origin,
        }
    }

    pub fn levels(&self) -> usize {
        self.path.len() + 1
    }

    pub fn items(&self, level: usize) -> &[Entry] {
        let mut items: &[Entry] = &self.root;
        for at in self.path.iter().take(level) {
            match items.get(*at).map(|entry| &entry.kind) {
                Some(Kind::Submenu(children)) => items = children,
                _ => return &[],
            }
        }
        items
    }

    pub fn panels(&self, look: &Look, font: &Font, screen: Vec2) -> Vec<Rect> {
        let mut out: Vec<Rect> = Vec::new();
        for level in 0..self.levels() {
            let items = self.items(level);
            let size = look.size(items, font, screen);
            let rect = match out.last() {
                None => {
                    let x = self.origin.x.min(screen.x - size.x - 8.0).max(8.0);
                    let y = self.origin.y.min(screen.y - size.y - 8.0).max(8.0);
                    Rect::new(Vec2::new(x, y), size)
                }
                Some(parent) => {
                    let parent_items = self.items(level - 1);
                    let at = self.path[level - 1];
                    let top = parent.origin.y + look.item_top(parent_items, at)
                        - self.scroll[level - 1]
                        - look.pad;
                    let right = parent.max().x - 4.0;
                    let x = match right + size.x <= screen.x - 4.0 {
                        true => right,
                        false => (parent.origin.x - size.x + 4.0).max(4.0),
                    };
                    let y = top.min(screen.y - size.y - 8.0).max(8.0);
                    Rect::new(Vec2::new(x, y), size)
                }
            };
            out.push(Rect::new(rect.origin.round(), rect.size));
        }
        out
    }

    pub fn hit(
        &self,
        look: &Look,
        font: &Font,
        screen: Vec2,
        pointer: Vec2,
    ) -> Option<(usize, Option<usize>)> {
        let panels = self.panels(look, font, screen);
        for level in (0..panels.len()).rev() {
            let rect = panels[level];
            if rect.contains(pointer) {
                let items = self.items(level);
                let index =
                    look.index_at(items, rect.size, self.scroll[level], pointer - rect.origin);
                return Some((
                    level,
                    index.filter(|at| {
                        items[*at].pickable() || matches!(items[*at].kind, Kind::Disabled)
                    }),
                ));
            }
        }
        None
    }

    pub fn lit_target(&self, look: &Look, font: &Font, screen: Vec2, pointer: Vec2) -> Option<Rect> {
        let (level, index) = self.hit(look, font, screen, pointer)?;
        let index = index?;
        let items = self.items(level);
        let entry = items.get(index).filter(|entry| entry.pickable())?;
        let panel = *self.panels(look, font, screen).get(level)?;
        let top = look.item_top(items, index) - self.scroll[level];
        let row = Rect::new(panel.origin + Vec2::new(0.0, top), Vec2::new(panel.size.x, look.row(entry)));
        Some(crate::rows::lit_rect(row))
    }

    pub fn point(&mut self, level: usize, index: Option<usize>) -> bool {
        if level >= self.hover.len() || self.hover[level] == index {
            return false;
        }
        self.hover[level] = index;
        true
    }

    pub fn open(&mut self, level: usize, index: usize) -> bool {
        let opens = matches!(
            self.items(level).get(index).map(|entry| &entry.kind),
            Some(Kind::Submenu(_))
        );
        let already = self.path.get(level) == Some(&index) && self.path.len() == level + 1;
        self.path.truncate(level);
        self.hover.truncate(level + 1);
        self.scroll.truncate(level + 1);
        self.hover[level] = Some(index);
        if opens {
            self.path.push(index);
            self.hover.push(None);
            self.scroll.push(0.0);
        }
        !already
    }

    pub fn close_below(&mut self, level: usize) -> bool {
        if self.path.len() <= level {
            return false;
        }
        self.path.truncate(level);
        self.hover.truncate(level + 1);
        self.scroll.truncate(level + 1);
        true
    }

    pub fn scroll_by(
        &mut self,
        look: &Look,
        font: &Font,
        screen: Vec2,
        level: usize,
        by: f64,
    ) -> bool {
        let items = self.items(level).to_vec();
        let size = look.size(&items, font, screen);
        let most = look.most_scroll(&items, size);
        let before = self.scroll[level];
        self.scroll[level] = (before + by).clamp(0.0, most);
        self.close_below(level);
        self.scroll[level] != before
    }

    fn step(&self, level: usize, from: Option<usize>, down: bool) -> Option<usize> {
        let items = self.items(level);
        let count = items.len();
        if count == 0 {
            return None;
        }
        let mut at = match (from, down) {
            (None, true) => count - 1,
            (None, false) => 0,
            (Some(at), _) => at,
        };
        for _ in 0..count {
            at = match down {
                true => (at + 1) % count,
                false => (at + count - 1) % count,
            };
            if items[at].pickable() {
                return Some(at);
            }
        }
        None
    }

    pub fn key(&mut self, key: Key) -> Outcome {
        let level = self.levels() - 1;
        let current = self.hover[level];
        match key {
            Key::Down | Key::Up => {
                let next = self.step(level, current, key == Key::Down);
                match self.point(level, next) {
                    true => Outcome::Changed,
                    false => Outcome::Nothing,
                }
            }
            Key::Right | Key::Enter => {
                let Some(at) = current else {
                    return match key {
                        Key::Enter => Outcome::Nothing,
                        _ => {
                            let first = self.step(level, None, true);
                            self.point(level, first);
                            Outcome::Changed
                        }
                    };
                };
                match self.items(level)[at].kind {
                    Kind::Submenu(_) => {
                        self.open(level, at);
                        let first = self.step(level + 1, None, true);
                        self.point(level + 1, first);
                        Outcome::Changed
                    }
                    Kind::Action(id) if key == Key::Enter => Outcome::Activate(id),
                    _ => Outcome::Nothing,
                }
            }
            Key::Left | Key::Escape => match level {
                0 if key == Key::Escape => Outcome::Close,
                0 => Outcome::Nothing,
                _ => {
                    self.close_below(level - 1);
                    Outcome::Changed
                }
            },
            Key::Letter(letter) => {
                let wanted = letter.to_lowercase().next().unwrap_or(letter);
                let items = self.items(level);
                let start = current.map(|at| at + 1).unwrap_or(0);
                let found = (0..items.len())
                    .map(|step| (start + step) % items.len().max(1))
                    .find(|at| {
                        items[*at].pickable() && items[*at].label.to_lowercase().starts_with(wanted)
                    });
                match found.map(|at| self.point(level, Some(at))) {
                    Some(true) => Outcome::Changed,
                    _ => Outcome::Nothing,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn font() -> Font {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts/WorkSans.ttf");
        Font::load(&path, 13.0).unwrap()
    }

    #[test]
    fn a_cascade_at_density_two_matches_density_one() {
        let font = font();
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        let items = menu();
        let size = Vec2::new(220.0, 200.0);
        crate::paint::same_at_two("cascade", || look.paint(&items, size, Some(1), 0.0, &font));
    }

    #[test]
    fn the_glide_lands_on_the_row_under_the_pointer_in_the_deepest_panel() {
        let font = font();
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        let screen = Vec2::new(1600.0, 1000.0);
        let mut menu = Cascade::new(menu(), Vec2::new(100.0, 100.0));
        let panels = menu.panels(&look, &font, screen);
        let first = panels[0].origin + Vec2::new(30.0, look.item_top(menu.items(0), 0) + 4.0);
        let lit = menu.lit_target(&look, &font, screen, first).expect("the first row lights");
        assert!(lit.contains(first));
        menu.open(0, 0);
        let panels = menu.panels(&look, &font, screen);
        let inner = panels[1].origin + Vec2::new(30.0, look.item_top(menu.items(1), 1) + 4.0);
        let lit = menu.lit_target(&look, &font, screen, inner).expect("the submenu row lights");
        assert!(lit.contains(inner) && panels[1].contains(lit.center()));
        assert_eq!(menu.lit_target(&look, &font, screen, Vec2::new(5.0, 5.0)), None);
    }

    fn menu() -> Vec<Entry> {
        vec![
            Entry::submenu(
                "Applications",
                vec![
                    Entry::submenu(
                        "Internet",
                        vec![Entry::action(10, "Firefox"), Entry::action(11, "Mail")],
                    ),
                    Entry::submenu("Office", vec![Entry::action(20, "Writer")]),
                ],
            ),
            Entry::rule(),
            Entry::action(1, "Settings…"),
            Entry::action(2, "Lock Screen"),
            Entry::submenu("Empty", Vec::new()),
        ]
    }

    const SCREEN: Vec2 = Vec2::new(1200.0, 800.0);

    #[test]
    fn submenus_open_beside_their_item_and_flip_at_the_edge() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        let font = font();
        let mut menu = Cascade::new(menu(), Vec2::new(100.0, 100.0));
        menu.open(0, 0);
        menu.open(1, 0);
        let panels = menu.panels(&look, &font, SCREEN);
        assert_eq!(panels.len(), 3);
        assert!(
            panels[1].origin.x > panels[0].origin.x + 100.0,
            "to the right"
        );
        assert_eq!(menu.items(2)[0].label, "Firefox");
        let mut edge = Cascade::new(menu.root.clone(), Vec2::new(1150.0, 100.0));
        edge.open(0, 0);
        let panels = edge.panels(&look, &font, SCREEN);
        assert!(
            panels[0].max().x <= SCREEN.x,
            "the first panel stays on screen"
        );
        assert!(
            panels[1].max().x <= panels[0].origin.x + 5.0,
            "the submenu flips left"
        );
    }

    #[test]
    fn the_pointer_finds_items_in_the_deepest_panel_first() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        let font = font();
        let mut menu = Cascade::new(menu(), Vec2::new(100.0, 100.0));
        menu.open(0, 0);
        let panels = menu.panels(&look, &font, SCREEN);
        let office = panels[1].origin + Vec2::new(30.0, look.item_top(menu.items(1), 1) + 5.0);
        assert_eq!(menu.hit(&look, &font, SCREEN, office), Some((1, Some(1))));
        let rule = panels[0].origin + Vec2::new(30.0, look.item_top(menu.items(0), 1) + 2.0);
        assert_eq!(
            menu.hit(&look, &font, SCREEN, rule),
            Some((0, None)),
            "a rule is not an item"
        );
        assert_eq!(menu.hit(&look, &font, SCREEN, Vec2::new(5.0, 5.0)), None);
    }

    #[test]
    fn the_keyboard_walks_in_and_out_and_picks() {
        let mut menu = Cascade::new(menu(), Vec2::ZERO);
        assert_eq!(menu.key(Key::Down), Outcome::Changed);
        assert_eq!(menu.hover[0], Some(0));
        assert_eq!(menu.key(Key::Right), Outcome::Changed);
        assert_eq!(menu.levels(), 2);
        assert_eq!(menu.hover[1], Some(0));
        menu.key(Key::Enter);
        assert_eq!(menu.levels(), 3);
        assert_eq!(menu.key(Key::Enter), Outcome::Activate(10));
        assert_eq!(menu.key(Key::Left), Outcome::Changed);
        assert_eq!(menu.levels(), 2);
        menu.key(Key::Escape);
        assert_eq!(menu.key(Key::Escape), Outcome::Close);
        let mut menu = Cascade::new(menu.root.clone(), Vec2::ZERO);
        menu.key(Key::Down);
        menu.key(Key::Up);
        assert_eq!(
            menu.hover[0],
            Some(3),
            "up wraps and skips the disabled item"
        );
        assert_eq!(menu.key(Key::Letter('s')), Outcome::Changed);
        assert_eq!(menu.hover[0], Some(2));
    }

    #[test]
    fn a_long_list_scrolls_inside_its_panel() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        let font = font();
        let many: Vec<Entry> = (0..80)
            .map(|n| Entry::action(n, &format!("App {n}")))
            .collect();
        let mut menu = Cascade::new(vec![Entry::submenu("Apps", many)], Vec2::new(10.0, 10.0));
        menu.open(0, 0);
        let panels = menu.panels(&look, &font, SCREEN);
        assert!(panels[1].size.y <= SCREEN.y - 24.0);
        assert!(menu.scroll_by(&look, &font, SCREEN, 1, 300.0));
        assert!(menu.scroll_by(&look, &font, SCREEN, 1, 1.0e6));
        assert!(
            !menu.scroll_by(&look, &font, SCREEN, 1, 10.0),
            "it stops at the end"
        );
        let size = look.size(menu.items(1), &font, SCREEN);
        let last = look.index_at(
            menu.items(1),
            size,
            menu.scroll[1],
            Vec2::new(20.0, size.y - 8.0),
        );
        assert_eq!(last, Some(79));
    }
}
