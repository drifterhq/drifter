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

pub fn with_back(items: Vec<Item>) -> Vec<Item> {
    let mut out = vec![Item::line(BACK, crate::lang::t("Back")), Item::rule(BACK_RULE)];
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_and_desktop_menus_share_one_style() {
        let palette = crate::theme::DARK;
        let panel = Look::themed(&palette);
        let desk = crate::cascade::Look::themed(&palette, ACCENT);
        assert_eq!(panel.item, desk.item, "the same row height");
        assert_eq!(panel.radius, desk.radius, "the same corners");
        assert_eq!(panel.pad, desk.pad);
        assert_eq!(panel.rule_ink, desk.rule_ink, "the same rule colour");
        assert_eq!(f64::from(panel.back_alpha) / 255.0, 245.0 / 255.0);
    }

    #[test]
    fn a_menu_grows_from_where_it_was_opened() {
        let rect = Rect::new(Vec2::new(100.0, 100.0), Vec2::new(200.0, 300.0));
        let click = Vec2::new(90.0, 95.0);
        let (first, shown) = grown(rect, click, 0.0, false, false);
        assert_eq!(shown, 0.0);
        assert_eq!(first.origin, rect.origin, "the click is clamped to the menu corner");
        assert!((first.size.x - rect.size.x * 0.35).abs() < 1e-9, "it starts small");
        assert_eq!(grown(rect, click, GROW_WAIT + GROW, false, false), (rect, 1.0));
        assert_eq!(grown(rect, click, 0.0, true, false), (rect, 1.0), "no animation shows it whole");
        let (still, fading) = grown(rect, click, GROW_WAIT + GROW / 4.0, false, true);
        assert_eq!(still, rect, "reduce motion keeps the size");
        assert!(fading > 0.0 && fading < 1.0, "but still fades");
        let middle = Vec2::new(200.0, 250.0);
        let (mid, _) = grown(rect, middle, GROW_WAIT + GROW / 2.0, false, false);
        assert!(mid.contains(middle) && mid.size.x < rect.size.x, "it grows out of the point");
    }

    #[test]
    fn a_submenu_starts_with_back_and_a_rule() {
        let items = with_back(vec![Item::line(7, "Wi-Fi")]);
        assert_eq!(items[0].id, BACK);
        assert!(items[1].separator);
        assert_eq!(items[2].label, "Wi-Fi");
    }

    #[test]
    fn an_item_with_children_makes_room_for_its_arrow() {
        let look = Look::default();
        let plain = vec![Item::line(1, "")];
        let mut parent = Item::line(1, "");
        parent.children = true;
        let wide = look.size(&[parent.clone()], None).x;
        assert!(wide >= look.size(&plain, None).x);
        let image = look.paint(&[parent], Vec2::new(200.0, look.height(&plain)), None, None);
        assert_eq!(image.width, 200);
    }

    const SCREEN: Vec2 = Vec2::new(1920.0, 1080.0);

    #[test]
    fn a_menu_at_density_two_matches_density_one() {
        let look = Look::default();
        let font = crate::text::Font::find(None, 13.0).ok();
        let mut list = items();
        list.push(Item::more(90, "More"));
        list.push(Item::rule(91));
        let size = look.size(&list, font.as_ref());
        crate::paint::same_at_two("menu", || look.paint(&list, size, Some(1), font.as_ref()));
    }

    fn items() -> Vec<Item> {
        vec![
            Item::line(1, "_Show"),
            Item::line(2, "&Hide"),
            Item::rule(3),
            Item::line(4, "Quit"),
        ]
    }

    fn font() -> Option<Font> {
        Font::find(None, 13.0).ok()
    }

    #[test]
    fn a_mnemonic_marker_never_reaches_the_screen() {
        assert_eq!(clean("_Show"), "Show");
        assert_eq!(clean("&Hide"), "Hide");
        assert_eq!(clean("Save _As"), "Save As");
        assert_eq!(clean("100_% zoom"), "100% zoom");
    }

    #[test]
    fn the_menu_is_as_tall_as_its_items() {
        let look = Look::default();
        let items = items();
        let wanted = look.item * 3.0 + look.rule + look.pad * 2.0;
        assert_eq!(look.height(&items), wanted);
    }

    #[test]
    fn a_menu_is_never_narrower_than_the_least_width() {
        let look = Look::default();
        let size = look.size(&[Item::line(1, "ok")], font().as_ref());
        assert!(size.x >= look.least_width);
    }

    #[test]
    fn a_long_label_makes_the_menu_wider() {
        let Some(font) = font() else {
            return;
        };
        let look = Look::default();
        let short = look.size(&[Item::line(1, "Quit")], Some(&font));
        let long = look.size(
            &[Item::line(1, "Quit this program and every window it owns")],
            Some(&font),
        );
        assert!(long.x > short.x);
    }

    #[test]
    fn the_items_stack_with_no_overlap_and_no_gap() {
        let look = Look::default();
        let items = items();
        let size = look.size(&items, None);
        for index in 1..items.len() {
            let above = look.item_rect(&items, size, index - 1);
            let here = look.item_rect(&items, size, index);
            assert_eq!(above.max().y, here.origin.y);
        }
        let last = look.item_rect(&items, size, items.len() - 1);
        assert_eq!(last.max().y + look.pad, size.y);
    }

    #[test]
    fn a_click_finds_the_item_under_it() {
        let look = Look::default();
        let items = items();
        let size = look.size(&items, None);
        let second = look.item_rect(&items, size, 1);
        assert_eq!(look.item_at(&items, size, second.center()), Some(1));
    }

    #[test]
    fn a_separator_can_never_be_clicked() {
        let look = Look::default();
        let items = items();
        let size = look.size(&items, None);
        let rule = look.item_rect(&items, size, 2);
        assert_eq!(look.item_at(&items, size, rule.center()), None);
    }

    #[test]
    fn a_click_outside_the_menu_hits_nothing() {
        let look = Look::default();
        let items = items();
        let size = look.size(&items, None);
        assert_eq!(look.item_at(&items, size, Vec2::new(-4.0, 10.0)), None);
        assert_eq!(
            look.item_at(&items, size, Vec2::new(4.0, size.y + 10.0)),
            None
        );
    }

    #[test]
    fn the_menu_opens_at_the_icon() {
        let look = Look::default();
        let size = Vec2::new(180.0, 120.0);
        let rect = look.rect(Vec2::new(40.0, 36.0), size, SCREEN);
        assert_eq!(rect.origin, Vec2::new(40.0, 36.0));
    }

    #[test]
    fn a_menu_near_the_right_edge_slides_back_on_screen() {
        let look = Look::default();
        let size = Vec2::new(200.0, 120.0);
        let rect = look.rect(Vec2::new(1900.0, 36.0), size, SCREEN);
        assert_eq!(rect.max().x, SCREEN.x - 6.0);
        assert!(rect.origin.x < 1900.0);
    }

    #[test]
    fn a_menu_taller_than_the_screen_still_starts_on_screen() {
        let look = Look::default();
        let size = Vec2::new(200.0, 2000.0);
        let rect = look.rect(Vec2::new(10.0, 900.0), size, SCREEN);
        assert_eq!(rect.origin.y, 0.0);
    }

    #[test]
    fn painting_gives_an_image_of_the_asked_for_size() {
        let look = Look::default();
        let items = items();
        let size = look.size(&items, font().as_ref());
        let image = look.paint(&items, size, None, font().as_ref());
        assert_eq!(image.width, size.x as u32);
        assert_eq!(image.height, size.y as u32);
    }

    #[test]
    fn the_corners_are_rounded_and_the_middle_is_solid() {
        let look = Look::default();
        let items = items();
        let size = look.size(&items, None);
        let image = look.paint(&items, size, None, None);
        assert_eq!(image.rgba[3], 0);
        let middle = (((image.height / 2) as usize) * (image.width as usize)
            + (image.width / 2) as usize)
            * 4;
        assert_eq!(image.rgba[middle + 3], look.back_alpha);
    }

    #[test]
    fn the_hovered_item_is_a_solid_accent_bar_inset_from_the_edges() {
        let look = Look::default();
        let items = items();
        let size = look.size(&items, None);
        let lit = look.paint(&items, size, Some(0), None);
        let row = look.item_rect(&items, size, 0).center();
        let px = |x: f64| {
            let at = ((row.y as usize) * (lit.width as usize) + x as usize) * 4;
            [lit.rgba[at], lit.rgba[at + 1], lit.rgba[at + 2], lit.rgba[at + 3]]
        };
        assert_eq!(px(size.x / 2.0), [ACCENT[0], ACCENT[1], ACCENT[2], 255]);
        assert_ne!(px(1.0)[2], ACCENT[2], "the bar stops short of the menu edge");
    }

    #[test]
    fn the_label_is_drawn_inside_its_row() {
        let Some(font) = font() else {
            return;
        };
        let look = Look::default();
        let items = vec![Item::line(1, "Quit")];
        let size = look.size(&items, Some(&font));
        let image = look.paint(&items, size, None, Some(&font));
        let rect = look.item_rect(&items, size, 0);

        let mut ink = 0;
        for y in (rect.origin.y as u32)..(rect.max().y as u32) {
            for x in 0..image.width {
                let at = ((y as usize) * (image.width as usize) + x as usize) * 4;
                if image.rgba[at] > 100 {
                    ink += 1;
                }
            }
        }
        assert!(ink > 0, "the label never reached the image");
    }
}
