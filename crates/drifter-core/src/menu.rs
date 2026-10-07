use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::text::Font;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: i32,
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    pub children: bool,
}

pub const BACK: i32 = -1;
pub const BACK_RULE: i32 = -2;

pub fn with_back(items: Vec<Item>) -> Vec<Item> {
    let mut out = vec![Item::line(BACK, "Back"), Item::rule(BACK_RULE)];
    out.extend(items);
    out
}

const ARROW_ROOM: f64 = 14.0;

impl Item {
    pub fn line(id: i32, label: &str) -> Item {
        Item {
            id,
            label: clean(label),
            enabled: true,
            separator: false,
            children: false,
        }
    }

    pub fn more(id: i32, label: &str) -> Item {
        Item {
            children: true,
            ..Item::line(id, label)
        }
    }

    pub fn rule(id: i32) -> Item {
        Item {
            id,
            label: String::new(),
            enabled: false,
            separator: true,
            children: false,
        }
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
}

pub const ACCENT: [u8; 3] = [10, 122, 255];

impl Look {
    pub fn themed(p: &crate::theme::Palette) -> Look {
        Look {
            pad: 5.0,
            item: 24.0,
            rule: 9.0,
            radius: 8.0,
            least_width: 160.0,
            text: 13.0,
            ink: p.ink,
            dim: p.dim,
            back: p.back,
            back_alpha: 246,
            hover: p.hover,
            accent: ACCENT,
            on_accent: [255, 255, 255],
        }
    }
}

impl Default for Look {
    fn default() -> Self {
        Look::themed(&crate::theme::DARK)
    }
}

impl Look {
    pub fn height(&self, items: &[Item]) -> f64 {
        let inside: f64 = items
            .iter()
            .map(|item| if item.separator { self.rule } else { self.item })
            .sum();
        inside + self.pad * 2.0
    }

    pub fn size(&self, items: &[Item], font: Option<&Font>) -> Vec2 {
        let widest = items
            .iter()
            .filter(|item| !item.separator)
            .map(|item| font.map(|font| font.width_of(&item.label)).unwrap_or(0.0))
            .fold(0.0f64, f64::max);
        let arrow = match items.iter().any(|item| item.children) {
            true => ARROW_ROOM,
            false => 0.0,
        };
        let width = (widest + self.pad * 4.0 + arrow).max(self.least_width);
        Vec2::new(width.ceil(), self.height(items).ceil())
    }

    pub fn item_rect(&self, items: &[Item], size: Vec2, index: usize) -> Rect {
        let mut y = self.pad;
        for item in items.iter().take(index) {
            y += if item.separator { self.rule } else { self.item };
        }
        let tall = items
            .get(index)
            .map(|item| if item.separator { self.rule } else { self.item })
            .unwrap_or(0.0);
        Rect::new(Vec2::new(0.0, y), Vec2::new(size.x, tall))
    }

    pub fn item_at(&self, items: &[Item], size: Vec2, inside: Vec2) -> Option<usize> {
        if inside.x < 0.0 || inside.x >= size.x {
            return None;
        }
        (0..items.len()).find(|index| {
            let rect = self.item_rect(items, size, *index);
            !items[*index].separator && inside.y >= rect.origin.y && inside.y < rect.max().y
        })
    }

    pub fn rect(&self, at: Vec2, size: Vec2, screen: Vec2) -> Rect {
        let x = at.x.min(screen.x - size.x).max(0.0);
        let y = at.y.min(screen.y - size.y).max(0.0);
        Rect::new(Vec2::new(x, y).round(), size)
    }

    pub fn paint(
        &self,
        items: &[Item],
        size: Vec2,
        hover: Option<usize>,
        font: Option<&Font>,
    ) -> Image {
        let dense = crate::paint::density();
        let width = crate::paint::dense(size.x.round().max(1.0));
        let height = crate::paint::dense(size.y.round().max(1.0));
        let mut image = Image {
            width,
            height,
            rgba: vec![0u8; (width as usize) * (height as usize) * 4],
        };

        let alpha = f64::from(self.back_alpha) / 255.0;
        let big = size * dense;
        for y in 0..height {
            let py = f64::from(y) + 0.5;
            for x in 0..width {
                let px = f64::from(x) + 0.5;
                let coverage = (corner(px, py, big, self.radius * dense) + 0.5).clamp(0.0, 1.0);
                if coverage <= 0.0 {
                    continue;
                }
                let here = alpha * coverage;
                let index = ((y as usize) * (width as usize) + x as usize) * 4;
                for (channel, value) in self.back.iter().enumerate() {
                    image.rgba[index + channel] =
                        (f64::from(*value) * here).round().clamp(0.0, 255.0) as u8;
                }
                image.rgba[index + 3] = (here * 255.0).round().clamp(0.0, 255.0) as u8;
            }
        }

        for (index, item) in items.iter().enumerate() {
            let rect = self.item_rect(items, size, index);
            if item.separator {
                let line = rect.origin.y + rect.size.y / 2.0;
                paint_rule(&mut image, self.pad, size.x - self.pad, line, self.dim);
                continue;
            }
            let lit = hover == Some(index) && item.enabled;
            if lit {
                let bar = Rect::new(
                    Vec2::new(rect.origin.x + self.pad, rect.origin.y + 1.0),
                    Vec2::new(rect.size.x - self.pad * 2.0, rect.size.y - 2.0),
                );
                fill_round(&mut image, bar, 5.0, self.accent);
            }
            let Some(font) = font else {
                continue;
            };
            let ink = match (lit, item.enabled) {
                (true, _) => self.on_accent,
                (false, true) => self.ink,
                (false, false) => self.dim,
            };
            if item.children {
                let tip = Vec2::new(size.x - self.pad * 2.0, rect.center().y);
                paint_arrow(&mut image, tip, ink);
            }
            let text = font.paint(&item.label, ink);
            let at = Vec2::new(
                rect.origin.x + self.pad * 2.0,
                rect.origin.y + (rect.size.y - crate::paint::logical(&text).y) / 2.0,
            );
            stamp(&mut image, &text, at);
        }

        image
    }
}

fn paint_arrow(image: &mut Image, tip: Vec2, ink: [u8; 3]) {
    let dense = crate::paint::density();
    let half = (4.0 * dense).round() as i64;
    let (tip_x, mid_y) = ((tip.x * dense).round() as i64, (tip.y * dense).round() as i64);
    for dy in -half..=half {
        let reach = half - dy.abs();
        for dx in 0..=reach {
            let (x, y) = (tip_x - half + dx, mid_y + dy);
            if x < 0 || y < 0 || x >= i64::from(image.width) || y >= i64::from(image.height) {
                continue;
            }
            let at = ((y as usize) * (image.width as usize) + x as usize) * 4;
            image.rgba[at..at + 3].copy_from_slice(&ink);
            image.rgba[at + 3] = 255;
        }
    }
}

pub(crate) fn corner(px: f64, py: f64, size: Vec2, radius: f64) -> f64 {
    let centers = [
        Vec2::new(radius, radius),
        Vec2::new(size.x - radius, radius),
        Vec2::new(size.x - radius, size.y - radius),
        Vec2::new(radius, size.y - radius),
    ];
    let past = [
        px < centers[0].x && py < centers[0].y,
        px > centers[1].x && py < centers[1].y,
        px > centers[2].x && py > centers[2].y,
        px < centers[3].x && py > centers[3].y,
    ];
    for (center, past) in centers.iter().zip(past) {
        if past {
            let dx = px - center.x;
            let dy = py - center.y;
            return radius - (dx * dx + dy * dy).sqrt();
        }
    }
    px.min(size.x - px).min(py).min(size.y - py)
}

pub(crate) fn fill_round(image: &mut Image, rect: Rect, radius: f64, color: [u8; 3]) {
    let dense = crate::paint::density();
    let (rect, radius) = (Rect::new(rect.origin * dense, rect.size * dense), radius * dense);
    let from_y = rect.origin.y.max(0.0) as u32;
    let to_y = (rect.max().y.min(f64::from(image.height))).ceil() as u32;
    let from_x = rect.origin.x.max(0.0) as u32;
    let to_x = (rect.max().x.min(f64::from(image.width))).ceil() as u32;
    for y in from_y..to_y {
        for x in from_x..to_x {
            let local = Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5) - rect.origin;
            let cover = (corner(local.x, local.y, rect.size, radius) + 0.5).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let index = ((y as usize) * (image.width as usize) + x as usize) * 4;
            for (channel, value) in color.iter().enumerate() {
                let was = f64::from(image.rgba[index + channel]);
                image.rgba[index + channel] = (f64::from(*value) * cover + was * (1.0 - cover)).round() as u8;
            }
            let was = f64::from(image.rgba[index + 3]);
            image.rgba[index + 3] = (255.0 * cover + was * (1.0 - cover)).round() as u8;
        }
    }
}

pub(crate) fn paint_rule(image: &mut Image, from: f64, to: f64, y: f64, color: [u8; 3]) {
    let dense = crate::paint::density();
    let (from, to) = (from * dense, to * dense);
    let first = (y.round() * dense).max(0.0) as u32;
    for row in first..first + dense as u32 {
        if row >= image.height {
            return;
        }
        rule_row(image, from, to, row, color);
    }
}

fn rule_row(image: &mut Image, from: f64, to: f64, row: u32, color: [u8; 3]) {
    for x in (from.max(0.0) as u32)..(to.min(f64::from(image.width)) as u32) {
        let index = ((row as usize) * (image.width as usize) + x as usize) * 4;
        if image.rgba[index + 3] == 0 {
            continue;
        }
        let a = f64::from(image.rgba[index + 3]) / 255.0 * 0.5;
        for (channel, value) in color.iter().enumerate() {
            image.rgba[index + channel] = (f64::from(*value) * a).round() as u8;
        }
    }
}

pub(crate) fn stamp(image: &mut Image, text: &Image, at: Vec2) {
    let at = at * crate::paint::density();
    for y in 0..text.height {
        let target_y = at.y.round() as i64 + i64::from(y);
        if target_y < 0 || target_y >= i64::from(image.height) {
            continue;
        }
        for x in 0..text.width {
            let target_x = at.x.round() as i64 + i64::from(x);
            if target_x < 0 || target_x >= i64::from(image.width) {
                continue;
            }
            let from = ((y as usize) * (text.width as usize) + x as usize) * 4;
            let ink = f64::from(text.rgba[from + 3]) / 255.0;
            if ink <= 0.0 {
                continue;
            }
            let to = ((target_y as usize) * (image.width as usize) + target_x as usize) * 4;
            for channel in 0..3 {
                let under = f64::from(image.rgba[to + channel]) * (1.0 - ink);
                let over = f64::from(text.rgba[from + channel]);
                image.rgba[to + channel] = (under + over).round().clamp(0.0, 255.0) as u8;
            }
            let under = f64::from(image.rgba[to + 3]) * (1.0 - ink);
            image.rgba[to + 3] = (under + f64::from(text.rgba[from + 3]))
                .round()
                .clamp(0.0, 255.0) as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(rect.max().x, SCREEN.x);
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
