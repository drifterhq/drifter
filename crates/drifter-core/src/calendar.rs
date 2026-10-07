use chrono::{Datelike, NaiveDate};

use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::notify::Recent;
use crate::paint::Canvas;
use crate::text::Font;

pub const NOTES: usize = 6;

pub fn month_grid(day: NaiveDate) -> Vec<Option<u32>> {
    let first = day.with_day(1).unwrap_or(day);
    let lead = first.weekday().num_days_from_sunday() as usize;
    let days = days_in(first);
    let mut cells: Vec<Option<u32>> = vec![None; lead];
    cells.extend((1..=days).map(Some));
    while cells.len() % 7 != 0 {
        cells.push(None);
    }
    cells
}

fn days_in(first: NaiveDate) -> u32 {
    let next = match first.month() {
        12 => NaiveDate::from_ymd_opt(first.year() + 1, 1, 1),
        month => NaiveDate::from_ymd_opt(first.year(), month + 1, 1),
    };
    next.map(|next| (next - first).num_days() as u32).unwrap_or(30)
}

pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

pub fn rows_now() -> usize {
    month_grid(today()).len() / 7
}

pub fn month_name(day: NaiveDate) -> String {
    const NAMES: [&str; 12] = [
        "January", "February", "March", "April", "May", "June", "July", "August", "September",
        "October", "November", "December",
    ];
    format!("{} {}", NAMES[day.month0() as usize], day.year())
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub width: f64,
    pub pad: f64,
    pub cell: f64,
    pub card_gap: f64,
    pub note: f64,
    pub radius: f64,
    pub back: [u8; 3],
    pub back_alpha: f64,
    pub card: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub accent: [u8; 3],
}

impl Look {
    pub fn themed(p: &crate::theme::Palette, accent: [u8; 3]) -> Look {
        Look {
            width: 340.0,
            pad: 12.0,
            cell: 40.0,
            card_gap: 10.0,
            note: 64.0,
            radius: 14.0,
            back: p.back,
            back_alpha: 0.96,
            card: p.card,
            ink: p.ink,
            dim: p.dim,
            accent,
        }
    }

    fn calendar_height(&self, rows: usize) -> f64 {
        44.0 + 22.0 + self.cell * 0.8 * rows as f64 + 10.0
    }

    fn list_top(&self, rows: usize) -> f64 {
        self.pad + self.calendar_height(rows) + self.card_gap
    }

    pub fn size(&self, rows: usize, notes: usize) -> Vec2 {
        let list = 30.0 + match notes {
            0 => 40.0,
            n => (self.note + 8.0) * n.min(NOTES) as f64,
        };
        Vec2::new(self.width, self.list_top(rows) + list + self.pad)
    }

    pub fn rect(&self, under: Rect, screen: Vec2, rows: usize, notes: usize) -> Rect {
        let size = self.size(rows, notes);
        let x = (under.max().x - size.x).min(screen.x - size.x - 6.0).max(6.0);
        Rect::new(Vec2::new(x, under.max().y + 8.0).round(), size)
    }

    pub fn clear_rect(&self, rows: usize) -> Rect {
        Rect::new(
            Vec2::new(self.width - self.pad - 56.0, self.list_top(rows)),
            Vec2::new(56.0, 22.0),
        )
    }

    pub fn paint(
        &self,
        today: NaiveDate,
        notes: &[Recent],
        now: u64,
        font: &Font,
        title: &Font,
    ) -> Image {
        let cells = month_grid(today);
        let rows = cells.len() / 7;
        let size = self.size(rows, notes.len());
        let mut canvas = Canvas::new(size.x as u32, size.y as u32);
        canvas.round_rect(Rect::new(Vec2::ZERO, size), self.radius, self.back, self.back_alpha);

        let card = Rect::new(
            Vec2::splat(self.pad),
            Vec2::new(self.width - self.pad * 2.0, self.calendar_height(rows)),
        );
        canvas.round_rect(card, 12.0, self.card, 0.85);
        let head = title.paint(&month_name(today), self.ink);
        canvas.stamp(&head, card.origin + Vec2::new(14.0, 12.0), 1.0);
        let step = (card.size.x - 20.0) / 7.0;
        let left = card.origin.x + 10.0;
        for (at, letter) in ["S", "M", "T", "W", "T", "F", "S"].iter().enumerate() {
            let text = font.paint(letter, self.dim);
            let x = left + step * at as f64 + (step - crate::paint::logical(&text).x) / 2.0;
            canvas.stamp(&text, Vec2::new(x, card.origin.y + 44.0), 1.0);
        }
        let row_tall = self.cell * 0.8;
        for (index, cell) in cells.iter().enumerate() {
            let Some(day) = cell else {
                continue;
            };
            let center = Vec2::new(
                left + step * (index % 7) as f64 + step / 2.0,
                card.origin.y + 66.0 + row_tall * (index / 7) as f64 + row_tall / 2.0,
            );
            let mine = *day == today.day();
            if mine {
                canvas.circle(center, row_tall * 0.45, self.accent, 1.0);
            }
            let text = font.paint(&day.to_string(), if mine { [255, 255, 255] } else { self.ink });
            canvas.stamp(&text, center - crate::paint::logical(&text) / 2.0, 1.0);
        }

        let top = self.list_top(rows);
        let heading = font.paint_bold("Notifications", self.ink);
        canvas.stamp(&heading, Vec2::new(self.pad + 4.0, top + 3.0), 1.0);
        if notes.is_empty() {
            let none = font.paint("No Notifications", self.dim);
            canvas.stamp(&none, Vec2::new((self.width - crate::paint::logical(&none).x) / 2.0, top + 40.0), 1.0);
            return canvas.image;
        }
        let clear = self.clear_rect(rows);
        canvas.round_rect(clear, 11.0, self.card, 0.9);
        let word = font.paint("Clear", self.ink);
        canvas.stamp(&word, clear.center() - crate::paint::logical(&word) / 2.0, 1.0);
        for (index, note) in notes.iter().take(NOTES).enumerate() {
            let rect = Rect::new(
                Vec2::new(self.pad, top + 30.0 + (self.note + 8.0) * index as f64),
                Vec2::new(self.width - self.pad * 2.0, self.note),
            );
            canvas.round_rect(rect, 12.0, self.card, 0.85);
            let room = rect.size.x - 24.0;
            let app = font.paint(&note.app, self.dim);
            let when = font.paint(&crate::notify::ago(note.when, now), self.dim);
            canvas.stamp(&app, rect.origin + Vec2::new(12.0, 8.0), 1.0);
            canvas.stamp(&when, Vec2::new(rect.max().x - 12.0 - crate::paint::logical(&when).x, rect.origin.y + 8.0), 1.0);
            let summary = font.paint_bold(&fit(font, &note.summary, room), self.ink);
            canvas.stamp(&summary, rect.origin + Vec2::new(12.0, 26.0), 1.0);
            let body = font.paint(&fit(font, &note.body, room), self.dim);
            canvas.stamp(&body, rect.origin + Vec2::new(12.0, 44.0), 1.0);
        }
        canvas.image
    }
}

fn fit(font: &Font, text: &str, room: f64) -> String {
    let flat: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if font.width_of(&flat) <= room {
        return flat;
    }
    let mut out = String::new();
    for c in flat.chars() {
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
    fn october_2026_starts_on_a_thursday_and_has_31_days() {
        let day = NaiveDate::from_ymd_opt(2026, 10, 5).unwrap();
        let cells = month_grid(day);
        assert_eq!(cells.iter().position(|cell| *cell == Some(1)), Some(4));
        assert_eq!(cells.iter().flatten().count(), 31);
        assert_eq!(cells.len() % 7, 0);
        assert_eq!(month_name(day), "October 2026");
        let feb = month_grid(NaiveDate::from_ymd_opt(2028, 2, 10).unwrap());
        assert_eq!(feb.iter().flatten().count(), 29, "a leap year");
        let dec = month_grid(NaiveDate::from_ymd_opt(2026, 12, 1).unwrap());
        assert_eq!(dec.iter().flatten().count(), 31);
    }

    #[test]
    fn the_calendar_at_density_two_matches_density_one() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        let font = crate::text::Font::find(None, 13.0).unwrap();
        let title = crate::text::Font::find(None, 15.0).unwrap();
        let today = NaiveDate::from_ymd_opt(2026, 10, 6).unwrap();
        let notes = [Recent {
            when: 1_000,
            app: "Mail".to_string(),
            summary: "New message".to_string(),
            body: "Lunch at noon?".to_string(),
        }];
        crate::paint::same_at_two("calendar", || look.paint(today, &notes, 1_600, &font, &title));
    }

    #[test]
    fn the_panel_grows_with_the_notes_and_stops_at_six() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        let none = look.size(5, 0).y;
        let two = look.size(5, 2).y;
        let many = look.size(5, 40).y;
        assert!(two > none);
        assert_eq!(many, look.size(5, NOTES).y);
        assert!(look.clear_rect(5).max().x <= look.width);
    }
}
