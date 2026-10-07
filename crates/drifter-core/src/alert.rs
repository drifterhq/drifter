use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Cancel,
    Confirm,
    Tick,
}

pub const REOPEN: &str = "Reopen windows when logging back in";

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub width: f64,
    pub height: f64,
    pub radius: f64,
    pub back: [u8; 3],
    pub back_alpha: f64,
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub button: [u8; 3],
    pub accent: [u8; 3],
    pub tick: bool,
}

impl Look {
    pub fn themed(p: &crate::theme::Palette, accent: [u8; 3]) -> Look {
        Look {
            width: 300.0,
            height: 168.0,
            radius: 16.0,
            back: p.back,
            back_alpha: 0.97,
            ink: p.ink,
            dim: p.dim,
            button: p.raised,
            accent,
            tick: false,
        }
    }

    pub fn with_tick(self) -> Look {
        Look {
            width: 320.0,
            height: self.height + 32.0,
            tick: true,
            ..self
        }
    }

    pub fn tick_row(&self) -> Option<Rect> {
        self.tick
            .then(|| Rect::new(Vec2::new(16.0, 92.0), Vec2::new(self.width - 32.0, 26.0)))
    }

    pub fn rect(&self, screen: Vec2) -> Rect {
        let size = Vec2::new(self.width, self.height);
        Rect::new(
            ((screen - size) / 2.0 - Vec2::new(0.0, screen.y * 0.08)).round(),
            size,
        )
    }

    pub fn buttons(&self) -> [Rect; 2] {
        let wide = (self.width - 16.0 * 2.0 - 10.0) / 2.0;
        let y = self.height - 16.0 - 30.0;
        [
            Rect::new(Vec2::new(16.0, y), Vec2::new(wide, 30.0)),
            Rect::new(Vec2::new(16.0 + wide + 10.0, y), Vec2::new(wide, 30.0)),
        ]
    }

    pub fn hit(&self, inside: Vec2) -> Option<Press> {
        let [cancel, confirm] = self.buttons();
        if cancel.contains(inside) {
            return Some(Press::Cancel);
        }
        if self.tick_row().is_some_and(|row| row.contains(inside)) {
            return Some(Press::Tick);
        }
        confirm.contains(inside).then_some(Press::Confirm)
    }

    pub fn paint(
        &self,
        title: &str,
        message: &str,
        verb: &str,
        ticked: bool,
        font: &Font,
        bold: &Font,
    ) -> Image {
        let size = Vec2::new(self.width, self.height);
        let mut canvas = Canvas::new(size.x as u32, size.y as u32);
        canvas.round_rect(
            Rect::new(Vec2::ZERO, size),
            self.radius,
            self.back,
            self.back_alpha,
        );
        let head = bold.paint_bold(title, self.ink);
        canvas.stamp(
            &head,
            Vec2::new((size.x - crate::paint::logical(&head).x) / 2.0, 26.0).round(),
            1.0,
        );
        let body = font.paint(message, self.dim);
        canvas.stamp(
            &body,
            Vec2::new((size.x - crate::paint::logical(&body).x) / 2.0, 62.0).round(),
            1.0,
        );
        if let Some(row) = self.tick_row() {
            self.paint_tick(&mut canvas, row, ticked, font);
        }
        let [cancel, confirm] = self.buttons();
        canvas.round_rect(cancel, 8.0, self.button, 1.0);
        canvas.round_rect(confirm, 8.0, self.accent, 1.0);
        for (rect, word, ink) in [
            (cancel, "Cancel", self.ink),
            (confirm, verb, [255, 255, 255]),
        ] {
            let text = font.paint(word, ink);
            canvas.stamp(&text, (rect.center() - crate::paint::logical(&text) / 2.0).round(), 1.0);
        }
        canvas.image
    }

    fn paint_tick(&self, canvas: &mut Canvas, row: Rect, ticked: bool, font: &Font) {
        let label = font.paint(REOPEN, self.ink);
        let side = 16.0;
        let wide = side + 8.0 + crate::paint::logical(&label).x;
        let left = (row.origin.x + (row.size.x - wide) / 2.0).round();
        let mid = row.center().y;
        let mark = Rect::new(Vec2::new(left, (mid - side / 2.0).round()), Vec2::splat(side));
        match ticked {
            true => {
                canvas.round_rect(mark, 4.0, self.accent, 1.0);
                let o = mark.origin;
                canvas.line(o + Vec2::new(4.0, 8.5), o + Vec2::new(7.0, 11.5), 2.0, [255, 255, 255], 1.0);
                canvas.line(o + Vec2::new(7.0, 11.5), o + Vec2::new(12.0, 5.0), 2.0, [255, 255, 255], 1.0);
            }
            false => canvas.ring(mark, 4.0, 1.5, self.dim, 1.0),
        }
        canvas.stamp(
            &label,
            Vec2::new(left + side + 8.0, mid - crate::paint::logical(&label).y / 2.0).round(),
            1.0,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_alert_at_density_two_matches_density_one() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        let font = crate::text::Font::find(None, 13.0).unwrap();
        crate::paint::same_at_two("alert", || look.paint("Log out?", "All apps will close.", "Log Out", false, &font, &font));
    }

    #[test]
    fn the_two_buttons_are_found_and_the_rest_is_nothing() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        let [cancel, confirm] = look.buttons();
        assert_eq!(look.hit(cancel.center()), Some(Press::Cancel));
        assert_eq!(look.hit(confirm.center()), Some(Press::Confirm));
        assert_eq!(look.hit(Vec2::new(10.0, 10.0)), None);
        assert!(confirm.max().x <= look.width && confirm.max().y <= look.height);
        assert_eq!(look.tick_row(), None);
    }

    #[test]
    fn the_reopen_row_sits_between_the_words_and_the_buttons() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]).with_tick();
        let row = look.tick_row().unwrap();
        let [cancel, confirm] = look.buttons();
        assert_eq!(look.hit(row.center()), Some(Press::Tick));
        assert!(row.max().y < cancel.origin.y);
        assert_eq!(look.hit(confirm.center()), Some(Press::Confirm));
        let font = crate::text::Font::find(None, 13.0).unwrap();
        for ticked in [false, true] {
            crate::paint::same_at_two("alert tick", || {
                look.paint("Restart now?", "Your apps will close.", "Restart", ticked, &font, &font)
            });
        }
    }
}
