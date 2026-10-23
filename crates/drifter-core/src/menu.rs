use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::text::Font;

pub const ROW: f64 = 28.0;
pub const RADIUS: f64 = crate::rows::RADIUS;
pub const HOVER_RADIUS: f64 = crate::rows::LIT_RADIUS;

pub const GROW_WAIT: f64 = 0.02;
pub const GROW: f64 = 0.2;
pub const DROP: f64 = 6.0;
pub const DROP_SCALE: f64 = 0.96;

pub fn grown(rect: Rect, anchor: Vec2, gone: f64, instant: bool, reduce: bool) -> (Rect, f64) {
    if instant {
        return (rect, 1.0);
    }
    let t = ((gone - GROW_WAIT) / GROW).clamp(0.0, 1.0);
    let alpha = crate::motion::smooth((t * 1.4).min(1.0));
    if reduce {
        return (rect, alpha);
    }
    let anchor = anchor.clamp(rect.origin, rect.max());
    let spring = crate::drawer::ease(t);
    let scale = DROP_SCALE + (1.0 - DROP_SCALE) * spring;
    let lift = DROP * (1.0 - spring);
    let grown = Rect::new(anchor + (rect.origin - anchor) * scale, rect.size * scale);
    (Rect::new(grown.origin - Vec2::new(0.0, lift), grown.size), alpha)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    pub children: bool,
    pub kind: crate::rows::Kind,
    pub lead: crate::rows::Lead,
    pub trail: crate::rows::Trail,
    pub note: String,
    pub value: String,
    pub still: bool,
}

pub const BACK: i32 = -1;
pub const BACK_RULE: i32 = -2;

pub fn with_back(items: Vec<Item>, title: &str) -> Vec<Item> {
    let title = match title.trim().is_empty() {
        true => crate::lang::t("Back"),
        false => title,
    };
    let mut out = vec![
        Item {
            label: title.to_string(),
            kind: crate::rows::Kind::Header,
            enabled: true,
            lead: crate::rows::Lead::Glyph(crate::rows::Glyph::Back),
            ..Item::line(BACK, "")
        },
        Item::rule(BACK_RULE),
    ];
    out.extend(items);
    out
}

impl Item {
    pub fn line(id: i32, label: &str) -> Item {
        Item {
            id,
            label: clean(label),
            enabled: true,
            separator: false,
            children: false,
            kind: crate::rows::Kind::Action,
            lead: crate::rows::Lead::None,
            trail: crate::rows::Trail::None,
            note: String::new(),
            value: String::new(),
            still: false,
        }
    }

    pub fn more(id: i32, label: &str) -> Item {
        Item {
            children: true,
            trail: crate::rows::Trail::Chevron,
            ..Item::line(id, label)
        }
    }

    pub fn rule(id: i32) -> Item {
        Item {
            enabled: false,
            separator: true,
            kind: crate::rows::Kind::Rule,
            ..Item::line(id, "")
        }
    }

    pub fn header(id: i32, title: &str, on: Option<bool>) -> Item {
        Item {
            kind: crate::rows::Kind::Header,
            trail: on.map_or(crate::rows::Trail::None, crate::rows::Trail::Switch),
            enabled: on.is_some(),
            ..Item::line(id, title)
        }
    }

    pub fn section(text: &str) -> Item {
        Item {
            kind: crate::rows::Kind::Section,
            enabled: false,
            ..Item::line(-30, text)
        }
    }

    pub fn rich(id: i32, name: &str, note: &str, lead: crate::rows::Lead, trail: crate::rows::Trail) -> Item {
        Item {
            label: name.to_string(),
            kind: crate::rows::Kind::Rich,
            note: note.to_string(),
            lead,
            trail,
            ..Item::line(id, "")
        }
    }

    pub fn info(key: &str, value: &str) -> Item {
        Item {
            kind: crate::rows::Kind::Info,
            enabled: false,
            value: value.to_string(),
            ..Item::line(-31, key)
        }
    }

    pub fn message(text: &str) -> Item {
        Item {
            kind: crate::rows::Kind::Message,
            enabled: false,
            ..Item::line(-32, text)
        }
    }

    pub fn glyph(self, glyph: crate::rows::Glyph) -> Item {
        Item {
            lead: crate::rows::Lead::Glyph(glyph),
            ..self
        }
    }

    pub fn icon(self, icon: Option<std::sync::Arc<Image>>) -> Item {
        match icon {
            Some(icon) => Item {
                lead: crate::rows::Lead::Icon(icon),
                ..self
            },
            None => self,
        }
    }

    pub fn trail(self, trail: crate::rows::Trail) -> Item {
        Item { trail, ..self }
    }

    pub fn off(self) -> Item {
        Item { enabled: false, ..self }
    }

    pub fn still(self) -> Item {
        Item { still: true, ..self }
    }

    pub fn height(&self) -> f64 {
        self.kind.height()
    }

    pub fn picks(&self) -> bool {
        self.enabled && !self.still && matches!(self.kind, crate::rows::Kind::Action | crate::rows::Kind::Rich | crate::rows::Kind::Header)
    }
}

pub fn clean(label: &str) -> String {
    let mut out = String::with_capacity(label.len());
    let mut skipping = false;
    for letter in label.chars() {
        if (letter == '_' || letter == '&') && !skipping {
            skipping = true;
            continue;
        }
        skipping = false;
        out.push(letter);
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub pad: f64,
    pub item: f64,
    pub rule: f64,
    pub radius: f64,
    pub least_width: f64,
    pub text: f64,
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub back: [u8; 3],
    pub back_alpha: u8,
    pub hover: [u8; 3],
    pub accent: [u8; 3],
    pub on_accent: [u8; 3],
    pub rule_ink: [u8; 3],
    pub off: [u8; 3],
}

pub const ACCENT: [u8; 3] = [10, 122, 255];

impl Look {
    pub fn themed(p: &crate::theme::Palette) -> Look {
        Look {
            pad: crate::rows::PAD,
            item: ROW,
            rule: crate::rows::Kind::Rule.height(),
            radius: RADIUS,
            least_width: 200.0,
            text: 13.0,
            ink: p.ink,
            dim: p.dim,
            back: p.back,
            back_alpha: 245,
            hover: p.hover,
            accent: ACCENT,
            on_accent: [255, 255, 255],
            rule_ink: p.rule,
            off: p.off,
        }
    }
}

impl Default for Look {
    fn default() -> Self {
        Look::themed(&crate::theme::DARK)
    }
}

impl Look {
    fn row_height(&self, item: &Item) -> f64 {
        match item.kind {
            crate::rows::Kind::Action => self.item,
            crate::rows::Kind::Rule => self.rule,
            kind => kind.height(),
        }
    }

    pub fn height(&self, items: &[Item]) -> f64 {
        items.iter().map(|item| self.row_height(item)).sum::<f64>() + self.pad * 2.0
    }

    fn lead_room(items: &[Item]) -> bool {
        items
            .iter()
            .any(|item| item.kind == crate::rows::Kind::Action && !matches!(item.lead, crate::rows::Lead::None))
    }

    fn row<'a>(item: &'a Item, room: bool) -> crate::rows::Row<'a> {
        crate::rows::Row {
            kind: item.kind,
            label: &item.label,
            note: &item.note,
            value: &item.value,
            lead: &item.lead,
            trail: &item.trail,
            enabled: item.enabled || !matches!(item.kind, crate::rows::Kind::Action | crate::rows::Kind::Rich),
            lead_room: room,
        }
    }

    pub fn size(&self, items: &[Item], font: Option<&Font>, small: Option<&Font>) -> Vec2 {
        let room = Self::lead_room(items);
        let widest = match font {
            Some(body) => {
                let fonts = crate::rows::Fonts { body, small: small.unwrap_or(body) };
                items
                    .iter()
                    .map(|item| crate::rows::width_of(&Self::row(item, room), &fonts))
                    .fold(0.0f64, f64::max)
            }
            None => 0.0,
        };
        let width = (widest + self.pad * 2.0).max(self.least_width);
        Vec2::new(width.ceil(), self.height(items).ceil())
    }

    pub fn item_rect(&self, items: &[Item], size: Vec2, index: usize) -> Rect {
        let y = self.pad + items.iter().take(index).map(|item| self.row_height(item)).sum::<f64>();
        let tall = items.get(index).map(|item| self.row_height(item)).unwrap_or(0.0);
        Rect::new(Vec2::new(0.0, y), Vec2::new(size.x, tall))
    }

    pub fn item_at(&self, items: &[Item], size: Vec2, inside: Vec2) -> Option<usize> {
        if inside.x < 0.0 || inside.x >= size.x {
            return None;
        }
        (0..items.len()).find(|index| {
            let rect = self.item_rect(items, size, *index);
            items[*index].picks() && inside.y >= rect.origin.y && inside.y < rect.max().y
        })
    }

    pub fn lit_rect(&self, items: &[Item], size: Vec2, index: usize) -> Rect {
        crate::rows::lit_rect(self.item_rect(items, size, index))
    }

    pub fn rect(&self, at: Vec2, size: Vec2, screen: Vec2) -> Rect {
        const EDGE: f64 = 6.0;
        let x = at.x.min(screen.x - size.x - EDGE).max(EDGE.min(at.x));
        let y = at.y.min(screen.y - size.y).max(0.0);
        Rect::new(Vec2::new(x, y).round(), size)
    }

    pub fn style(&self) -> crate::rows::Style {
        crate::rows::Style {
            back: self.back,
            back_alpha: f64::from(self.back_alpha) / 255.0,
            ink: self.ink,
            dim: self.dim,
            off: self.off,
            accent: self.accent,
            rule: self.ink,
            shade: self.hover,
        }
    }

    pub fn paint(&self, items: &[Item], size: Vec2, hover: Option<usize>, font: Option<&Font>, small: Option<&Font>) -> Image {
        let mut canvas = crate::paint::Canvas::new(size.x.round().max(1.0) as u32, size.y.round().max(1.0) as u32);
        let style = self.style();
        crate::rows::back(&mut canvas, size, self.radius, &style);
        let Some(body) = font else {
            return canvas.image;
        };
        let fonts = crate::rows::Fonts { body, small: small.unwrap_or(body) };
        let room = Self::lead_room(items);
        for (index, item) in items.iter().enumerate() {
            let rect = self.item_rect(items, size, index);
            if hover == Some(index) && item.picks() {
                crate::rows::highlight(&mut canvas, rect, &style, 0.07);
            }
            crate::rows::paint_row(&mut canvas, rect, &Self::row(item, room), &style, &fonts);
        }
        canvas.image
    }
}
