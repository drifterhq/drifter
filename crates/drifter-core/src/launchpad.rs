use crate::config::Launch;
use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

pub const DOCK_COMMAND: &str = "drifter:apps";

pub fn button() -> Launch {
    Launch::Full {
        app: None,
        command: Some(DOCK_COMMAND.to_string()),
        icon: None,
    }
}

pub fn is_button(item: &Launch) -> bool {
    item.command() == Some(DOCK_COMMAND)
}

pub fn paint_icon(side: u32) -> Image {
    let size = f64::from(side.max(12));
    let mut canvas = Canvas::new(side, side);
    let tile = Rect::new(Vec2::splat(size * 0.06), Vec2::splat(size * 0.88));
    canvas.round_rect(tile, size * 0.22, [44, 48, 62], 1.0);
    canvas.round_rect(
        Rect::new(tile.origin, Vec2::new(tile.size.x, tile.size.y * 0.5)),
        size * 0.22,
        [70, 76, 96],
        0.35,
    );
    let colors = [
        [255, 95, 87], [254, 188, 46], [40, 200, 64],
        [10, 132, 255], [191, 90, 242], [255, 55, 95],
        [100, 210, 255], [255, 159, 10], [48, 209, 88],
    ];
    let step = tile.size.x / 3.6;
    let dot = step * 0.62;
    for (index, color) in colors.iter().enumerate() {
        let (col, row) = ((index % 3) as f64, (index / 3) as f64);
        let at = tile.origin + Vec2::new(step * 0.55 + step * col, step * 0.55 + step * row) + Vec2::splat(step * 0.15);
        canvas.round_rect(Rect::new(at, Vec2::splat(dot)), dot * 0.28, *color, 1.0);
    }
    canvas.image
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub icon: f64,
    pub cell: Vec2,
    pub field: Vec2,
    pub top: f64,
    pub bottom: f64,
    pub most_columns: usize,
    pub most_rows: usize,
    pub veil: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub field_back: [u8; 3],
}

impl Look {
    pub fn themed(p: &crate::theme::Palette) -> Look {
        Look {
            icon: 72.0,
            cell: Vec2::new(136.0, 124.0),
            field: Vec2::new(300.0, 34.0),
            top: 90.0,
            bottom: 80.0,
            most_columns: 7,
            most_rows: 5,
            veil: p.back,
            ink: p.ink,
            dim: p.dim,
            field_back: p.raised,
        }
    }

    pub fn grid(&self, screen: Vec2) -> (usize, usize) {
        let columns = ((screen.x - 80.0) / self.cell.x).floor().max(1.0) as usize;
        let rows = ((screen.y - self.top - self.bottom) / self.cell.y).floor().max(1.0) as usize;
        (columns.min(self.most_columns), rows.min(self.most_rows))
    }

    pub fn per_page(&self, screen: Vec2) -> usize {
        let (columns, rows) = self.grid(screen);
        columns * rows
    }

    pub fn pages(&self, screen: Vec2, count: usize) -> usize {
        count.div_ceil(self.per_page(screen)).max(1)
    }

    pub fn field_rect(&self, screen: Vec2) -> Rect {
        Rect::new(
            Vec2::new(((screen.x - self.field.x) / 2.0).round(), (self.top - self.field.y) / 2.0),
            self.field,
        )
    }

    pub fn cell_rect(&self, screen: Vec2, slot: usize) -> Rect {
        let (columns, rows) = self.grid(screen);
        let wide = self.cell.x * columns as f64;
        let tall = self.cell.y * rows as f64;
        let left = ((screen.x - wide) / 2.0).round();
        let top = self.top + ((screen.y - self.top - self.bottom - tall) / 2.0).max(0.0).round();
        Rect::new(
            Vec2::new(left + self.cell.x * (slot % columns) as f64, top + self.cell.y * (slot / columns) as f64),
            self.cell,
        )
    }

    pub fn icon_rect(&self, screen: Vec2, slot: usize) -> Rect {
        let cell = self.cell_rect(screen, slot);
        Rect::new(
            Vec2::new(cell.center().x - self.icon / 2.0, cell.origin.y + 10.0).round(),
            Vec2::splat(self.icon),
        )
    }

    pub fn slot_at(&self, screen: Vec2, pointer: Vec2, shown: usize) -> Option<usize> {
        (0..shown.min(self.per_page(screen))).find(|slot| {
            let icon = self.icon_rect(screen, *slot);
            Rect::new(icon.origin - Vec2::splat(8.0), icon.size + Vec2::new(16.0, 34.0)).contains(pointer)
        })
    }

    pub fn paint(
        &self,
        base: &Image,
        screen: Vec2,
        query: &str,
        apps: &[(String, Option<Image>)],
        page: usize,
        font: &Font,
    ) -> Image {
        let mut canvas = Canvas { image: base.clone() };
        let field = self.field_rect(screen);
        canvas.round_rect(field, field.size.y / 2.0, self.field_back, 0.85);
        let (text, ink) = match query.is_empty() {
            true => ("Search", self.dim),
            false => (query, self.ink),
        };
        let words = font.paint(text, ink);
        let x = match query.is_empty() {
            true => field.center().x - f64::from(words.width) / 2.0,
            false => field.origin.x + 16.0,
        };
        canvas.stamp(&words, Vec2::new(x, field.center().y - f64::from(words.height) / 2.0), 1.0);

        let per = self.per_page(screen);
        let start = page * per;
        for (slot, (name, icon)) in apps.iter().skip(start).take(per).enumerate() {
            let spot = self.icon_rect(screen, slot);
            match icon {
                Some(image) => canvas.stamp(image, spot.origin, 1.0),
                None => canvas.round_rect(spot, spot.size.x * 0.22, self.field_back, 1.0),
            }
            let label = fit(font, name, self.cell.x - 12.0);
            let text = font.paint(&label, self.ink);
            canvas.stamp(
                &text,
                Vec2::new(spot.center().x - f64::from(text.width) / 2.0, spot.max().y + 8.0).round(),
                1.0,
            );
        }

        let pages = self.pages(screen, apps.len());
        if pages > 1 {
            let gap = 16.0;
            let left = screen.x / 2.0 - gap * (pages as f64 - 1.0) / 2.0;
            for at in 0..pages {
                let center = Vec2::new(left + gap * at as f64, screen.y - self.bottom / 2.0);
                let alpha = if at == page { 1.0 } else { 0.35 };
                canvas.circle(center, 4.0, self.ink, alpha);
            }
        }
        canvas.image
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

    const SCREEN: Vec2 = Vec2::new(1200.0, 800.0);

    #[test]
    fn the_grid_fits_the_screen_and_pages_hold_the_rest() {
        let look = Look::themed(&crate::theme::DARK);
        let (columns, rows) = look.grid(SCREEN);
        assert_eq!(columns, 7);
        assert!(rows >= 4);
        let per = look.per_page(SCREEN);
        assert_eq!(look.pages(SCREEN, per), 1);
        assert_eq!(look.pages(SCREEN, per + 1), 2);
        assert_eq!(look.pages(SCREEN, 0), 1);
        let last = look.cell_rect(SCREEN, per - 1);
        assert!(last.max().x <= SCREEN.x && last.max().y <= SCREEN.y - look.bottom + 1.0);
        assert!(look.cell_rect(SCREEN, 0).origin.y >= look.top);
    }

    #[test]
    fn an_icon_or_its_label_is_found_by_the_pointer() {
        let look = Look::themed(&crate::theme::DARK);
        let icon = look.icon_rect(SCREEN, 3);
        assert_eq!(look.slot_at(SCREEN, icon.center(), 10), Some(3));
        assert_eq!(look.slot_at(SCREEN, icon.center() + Vec2::new(0.0, 50.0), 10), Some(3), "the name counts");
        assert_eq!(look.slot_at(SCREEN, icon.center(), 2), None, "an empty slot is nothing");
        assert_eq!(look.slot_at(SCREEN, Vec2::new(5.0, 5.0), 10), None);
    }

    #[test]
    fn the_dock_button_is_known_by_its_command() {
        assert!(is_button(&button()));
        assert!(!is_button(&Launch::App("foot".into())));
        let icon = paint_icon(48);
        assert_eq!((icon.width, icon.height), (48, 48));
    }
}
