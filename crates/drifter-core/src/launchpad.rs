use crate::config::Launch;
use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

pub const DOCK_COMMAND: &str = "drifter:apps";

pub fn button() -> Launch {
    Launch::command_line(DOCK_COMMAND)
}

pub fn is_button(item: &Launch) -> bool {
    item.command() == Some(DOCK_COMMAND)
}

fn squircle(p: Vec2, half: f64) -> f64 {
    let n = 5.0;
    let x = (p.x.abs() / half).powf(n);
    let y = (p.y.abs() / half).powf(n);
    (1.0 - (x + y).powf(1.0 / n)) * half
}

fn mix(a: [u8; 3], b: [u8; 3], t: f64) -> [f64; 3] {
    [0, 1, 2].map(|c| f64::from(a[c]) + (f64::from(b[c]) - f64::from(a[c])) * t.clamp(0.0, 1.0))
}

fn blend(image: &mut Image, x: u32, y: u32, color: [f64; 3], cover: f64) {
    if cover <= 0.0 {
        return;
    }
    let at = ((y * image.width + x) * 4) as usize;
    for c in 0..3 {
        let was = f64::from(image.rgba[at + c]);
        image.rgba[at + c] = (color[c] * cover + was * (1.0 - cover)).round().clamp(0.0, 255.0) as u8;
    }
    let was = f64::from(image.rgba[at + 3]);
    image.rgba[at + 3] = (255.0 * cover + was * (1.0 - cover)).round().clamp(0.0, 255.0) as u8;
}

pub fn paint_icon(side: u32) -> Image {
    let side = side.max(16);
    let size = f64::from(side);
    let dense = crate::paint::density();
    let side = crate::paint::dense(size);
    let mut image = Image {
        width: side,
        height: side,
        rgba: vec![0; (side * side * 4) as usize],
    };
    let half = size * 0.46;
    let center = Vec2::splat(size / 2.0);
    let tiles: [([u8; 3], [u8; 3]); 9] = [
        ([255, 128, 102], [242, 64, 74]),
        ([255, 214, 92], [255, 159, 10]),
        ([110, 230, 120], [40, 190, 80]),
        ([110, 190, 255], [10, 120, 255]),
        ([214, 150, 255], [160, 80, 235]),
        ([255, 120, 170], [235, 50, 110]),
        ([120, 230, 230], [30, 175, 200]),
        ([255, 180, 110], [245, 120, 40]),
        ([150, 160, 255], [90, 100, 235]),
    ];
    let pitch = size * 0.235;
    let tile = size * 0.19;
    let first = center - Vec2::splat(pitch);
    for y in 0..side {
        for x in 0..side {
            let p = Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5) / dense;
            let edge = squircle(p - center, half);
            let body = (edge * dense + 0.5).clamp(0.0, 1.0);
            let shade = (p.y - (center.y - half)) / (half * 2.0);
            blend(&mut image, x, y, mix([250, 251, 253], [214, 219, 230], shade), body);
            let rim = (1.0 - (edge - 0.6).abs()).clamp(0.0, 1.0) * body;
            blend(&mut image, x, y, [255.0, 255.0, 255.0], rim * 0.6);
            for (index, (top, bottom)) in tiles.iter().enumerate() {
                let at = first + Vec2::new(pitch * (index % 3) as f64, pitch * (index / 3) as f64);
                let local = p - at;
                let inside = squircle(local, tile / 2.0);
                let cover = (inside * dense + 0.5).clamp(0.0, 1.0);
                if cover > 0.0 {
                    let t = (local.y + tile / 2.0) / tile;
                    blend(&mut image, x, y, mix(*top, *bottom, t), cover);
                }
            }
        }
    }
    image
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub icon: f64,
    pub cell: Vec2,
    pub field: f64,
    pub pad: f64,
    pub dots: f64,
    pub radius: f64,
    pub back: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub field_back: [u8; 3],
    pub rule: [u8; 3],
}

impl Look {
    pub fn themed(p: &crate::theme::Palette) -> Look {
        Look {
            icon: 60.0,
            cell: Vec2::new(108.0, 104.0),
            field: 56.0,
            pad: 20.0,
            dots: 30.0,
            radius: 22.0,
            back: p.back,
            ink: p.ink,
            dim: p.dim,
            field_back: p.raised,
            rule: p.rule,
        }
    }

    pub fn rect(&self, screen: Vec2) -> Rect {
        let wide = (screen.x - 120.0).clamp(360.0, self.cell.x * 6.0 + self.pad * 2.0);
        let tall = (screen.y - 180.0).clamp(300.0, self.field + self.cell.y * 4.0 + self.pad * 2.0 + self.dots);
        Rect::new(((screen - Vec2::new(wide, tall)) / 2.0).round(), Vec2::new(wide, tall))
    }

    pub fn grid(&self, size: Vec2) -> (usize, usize) {
        let columns = ((size.x - self.pad * 2.0) / self.cell.x).floor().max(1.0) as usize;
        let rows = ((size.y - self.field - self.pad * 2.0 - self.dots) / self.cell.y).floor().max(1.0) as usize;
        (columns, rows)
    }

    pub fn per_page(&self, size: Vec2) -> usize {
        let (columns, rows) = self.grid(size);
        columns * rows
    }

    pub fn pages(&self, size: Vec2, count: usize) -> usize {
        count.div_ceil(self.per_page(size)).max(1)
    }

    pub fn grid_band(&self, size: Vec2) -> Rect {
        let top = self.field + 1.0;
        let bottom = size.y - self.dots - 8.0;
        Rect::new(Vec2::new(0.0, top), Vec2::new(size.x, (bottom - top).max(0.0)))
    }

    pub fn field_rect(&self, size: Vec2) -> Rect {
        Rect::new(Vec2::ZERO, Vec2::new(size.x, self.field))
    }

    pub fn cell_rect(&self, size: Vec2, slot: usize) -> Rect {
        let (columns, _) = self.grid(size);
        let wide = self.cell.x * columns as f64;
        let left = ((size.x - wide) / 2.0).round();
        let top = self.field + self.pad;
        Rect::new(
            Vec2::new(left + self.cell.x * (slot % columns) as f64, top + self.cell.y * (slot / columns) as f64),
            self.cell,
        )
    }

    pub fn icon_rect(&self, size: Vec2, slot: usize) -> Rect {
        let cell = self.cell_rect(size, slot);
        Rect::new(
            Vec2::new(cell.center().x - self.icon / 2.0, cell.origin.y + 8.0).round(),
            Vec2::splat(self.icon),
        )
    }

    pub fn slot_at(&self, size: Vec2, inside: Vec2, shown: usize) -> Option<usize> {
        (0..shown.min(self.per_page(size))).find(|slot| self.cell_rect(size, *slot).contains(inside))
    }

    pub fn paint(
        &self,
        base: &Image,
        query: &str,
        apps: &[(String, Option<Image>)],
        page: usize,
        font: &Font,
        big: &Font,
    ) -> Image {
        let size = crate::paint::logical(&base);
        let mut canvas = Canvas::on(base.clone());
        let field = self.field_rect(size);
        let glass = Vec2::new(self.pad + 10.0, field.center().y - 2.0);
        canvas.circle(glass, 8.0, self.dim, 1.0);
        canvas.circle(glass, 6.0, self.back, 1.0);
        for step in 0..8 {
            let t = f64::from(step) * 0.8;
            canvas.circle(glass + Vec2::new(6.5 + t, 6.5 + t), 1.6, self.dim, 1.0);
        }
        let (text, ink) = match query.is_empty() {
            true => (crate::lang::t("Search apps"), self.dim),
            false => (query, self.ink),
        };
        let words = big.paint(text, ink);
        canvas.stamp(&words, Vec2::new(self.pad + 32.0, field.center().y - crate::paint::logical(&words).y / 2.0), 1.0);
        canvas.round_rect(
            Rect::new(Vec2::new(0.0, field.max().y), Vec2::new(size.x, 1.0)),
            0.0,
            self.rule,
            1.0,
        );

        let per = self.per_page(size);
        for (slot, (name, icon)) in apps.iter().skip(page * per).take(per).enumerate() {
            let spot = self.icon_rect(size, slot);
            match icon {
                Some(image) => canvas.stamp(image, spot.origin, 1.0),
                None => canvas.round_rect(spot, spot.size.x * 0.22, self.field_back, 1.0),
            }
            let label = fit(font, name, self.cell.x - 10.0);
            let text = font.paint(&label, self.ink);
            canvas.stamp(
                &text,
                Vec2::new(spot.center().x - crate::paint::logical(&text).x / 2.0, spot.max().y + 7.0).round(),
                1.0,
            );
        }
        if apps.is_empty() {
            let none = font.paint(crate::lang::t("No apps match"), self.dim);
            canvas.stamp(&none, Vec2::new((size.x - crate::paint::logical(&none).x) / 2.0, size.y / 2.0), 1.0);
        }

        let pages = self.pages(size, apps.len());
        if pages > 1 {
            let gap = 14.0;
            let left = size.x / 2.0 - gap * (pages as f64 - 1.0) / 2.0;
            for at in 0..pages {
                let center = Vec2::new(left + gap * at as f64, size.y - self.dots / 2.0 - 4.0);
                canvas.circle(center, 3.5, self.ink, if at == page { 0.9 } else { 0.25 });
            }
        }
        canvas.image
    }
}

pub fn page_after_swipe(page: usize, pages: usize, swipe: f64, threshold: f64) -> (usize, f64) {
    if swipe <= -threshold {
        return ((page + 1).min(pages.saturating_sub(1)), 0.0);
    }
    if swipe >= threshold {
        return (page.saturating_sub(1), 0.0);
    }
    (page, swipe)
}

pub fn slide_crops(width: f64, moved: f64, ahead: bool) -> [(f64, f64, f64); 2] {
    let moved = moved.clamp(0.0, width);
    match ahead {
        true => [(moved, 0.0, width - moved), (0.0, width - moved, moved)],
        false => [(0.0, moved, width - moved), (width - moved, 0.0, moved)],
    }
}

fn fit(font: &Font, text: &str, room: f64) -> String {
    if font.width_of(text) <= room {
        return text.to_string();
    }
    let mut out = String::new();
    for c in text.chars() {
        if font.width_of(&format!("{out}{c}…")) > room {
            break;
        }
        out.push(c);
    }
    format!("{}…", out.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_swipe_turns_one_page_past_the_threshold_and_stops_at_the_ends() {
        assert_eq!(page_after_swipe(0, 3, -50.0, 90.0), (0, -50.0), "short of the threshold");
        assert_eq!(page_after_swipe(0, 3, -95.0, 90.0), (1, 0.0), "left goes ahead");
        assert_eq!(page_after_swipe(1, 3, 120.0, 90.0), (0, 0.0), "right goes back");
        assert_eq!(page_after_swipe(2, 3, -400.0, 90.0), (2, 0.0), "the last page stays");
        assert_eq!(page_after_swipe(0, 3, 400.0, 90.0), (0, 0.0), "the first page stays");
    }

    #[test]
    fn slide_crops_cover_the_width() {
        for ahead in [true, false] {
            for moved in [0.0, 37.5, 300.0, 600.0] {
                let [old, new] = slide_crops(600.0, moved, ahead);
                assert_eq!(old.2 + new.2, 600.0);
                if old.2 > 0.0 && new.2 > 0.0 {
                    let (left, right) = if old.1 < new.1 { (old, new) } else { (new, old) };
                    assert_eq!(left.1, 0.0);
                    assert_eq!(left.1 + left.2, right.1, "they meet with no gap");
                }
            }
        }
        assert_eq!(slide_crops(600.0, 600.0, true)[1], (0.0, 0.0, 600.0), "the new page lands whole");
    }

    const SCREEN: Vec2 = Vec2::new(1200.0, 800.0);

    #[test]
    fn apps_at_density_two_match_density_one() {
        let look = Look::themed(&crate::theme::DARK);
        let font = crate::text::Font::find(None, 13.0).unwrap();
        let big = crate::text::Font::find(None, 20.0).unwrap();
        let apps = vec![("Terminal".to_string(), None), ("Files".to_string(), None)];
        crate::paint::same_at_two("apps icon", || paint_icon(48));
        crate::paint::same_at_two("apps", || {
            let size = look.rect(SCREEN).size;
            let base = crate::paint::Canvas::new(size.x as u32, size.y as u32).image;
            look.paint(&base, "te", &apps, 0, &font, &big)
        });
    }

    #[test]
    fn the_panel_is_a_centered_modal_not_the_whole_screen() {
        let look = Look::themed(&crate::theme::DARK);
        let rect = look.rect(SCREEN);
        assert!(rect.size.x < SCREEN.x && rect.size.y < SCREEN.y);
        assert!((rect.center().x - SCREEN.x / 2.0).abs() <= 1.0);
        let (columns, rows) = look.grid(rect.size);
        assert_eq!(columns, 6);
        assert!(rows >= 3);
        let per = look.per_page(rect.size);
        assert_eq!(look.pages(rect.size, per + 1), 2);
        let last = look.cell_rect(rect.size, per - 1);
        assert!(last.max().y <= rect.size.y - look.dots + 1.0);
    }

    #[test]
    fn an_app_is_found_by_the_pointer_inside_the_panel() {
        let look = Look::themed(&crate::theme::DARK);
        let size = look.rect(SCREEN).size;
        let icon = look.icon_rect(size, 3);
        assert_eq!(look.slot_at(size, icon.center(), 10), Some(3));
        assert_eq!(look.slot_at(size, icon.center(), 2), None);
        assert_eq!(look.slot_at(size, Vec2::new(5.0, 5.0), 10), None, "the search field is not an app");
    }

    #[test]
    fn the_dock_button_is_known_by_its_command_and_has_a_rounded_icon() {
        assert!(is_button(&button()));
        assert!(!is_button(&Launch::App("foot".into())));
        let icon = paint_icon(64);
        let alpha = |x: u32, y: u32| icon.rgba[((y * 64 + x) * 4 + 3) as usize];
        assert_eq!(alpha(0, 0), 0, "the corner is cut round");
        assert_eq!(alpha(32, 32), 255);
    }
}
