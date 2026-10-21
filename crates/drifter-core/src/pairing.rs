use crate::blue::{Ask, Question};
use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

pub use crate::prompt::Press;

pub const MOST_TYPED: usize = 16;
const DISC: f64 = 76.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub width: f64,
    pub pad: f64,
    pub digit: Vec2,
    pub step: f64,
    pub button: Vec2,
    pub radius: f64,
    pub back: [u8; 3],
    pub well: [u8; 3],
    pub rule: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub accent: [u8; 3],
    pub off: [u8; 3],
}

impl Look {
    pub fn themed(p: &crate::theme::Palette, accent: [u8; 3]) -> Look {
        Look {
            width: 380.0,
            pad: 24.0,
            digit: Vec2::new(38.0, 50.0),
            step: 44.0,
            button: Vec2::new(150.0, 32.0),
            radius: 18.0,
            back: p.surface,
            well: p.well,
            rule: p.rule,
            ink: p.ink,
            dim: p.dim,
            accent,
            off: p.off,
        }
    }

    fn lines(&self, ask: &Ask, font: &Font) -> Vec<String> {
        crate::notify::wrap(font, &ask.words(), self.width - self.pad * 2.0, 3)
    }

    fn slots(&self, ask: &Ask) -> usize {
        match &ask.question {
            Question::Confirm(_) => 6,
            Question::Show(code, _) => code.chars().count().clamp(1, MOST_TYPED),
            Question::Type => 6,
            Question::TypePin | Question::Allow => 0,
        }
    }

    fn field(&self, ask: &Ask) -> f64 {
        match (self.slots(ask), ask.question == Question::TypePin) {
            (0, false) => 0.0,
            (0, true) => 40.0 + 18.0,
            _ => self.digit.y + 18.0,
        }
    }

    fn top(&self, ask: &Ask, title: &Font, font: &Font) -> f64 {
        let line = font.line_height().ceil();
        self.pad + DISC + 16.0 + title.line_height().ceil() + 6.0 + line * self.lines(ask, font).len() as f64 + 18.0
    }

    pub fn size(&self, ask: &Ask, title: &Font, font: &Font) -> Vec2 {
        let tall = self.top(ask, title, font) + self.field(ask) + self.button.y + self.pad;
        Vec2::new(self.width, tall.ceil())
    }

    pub fn rect(&self, screen: Vec2, ask: &Ask, title: &Font, font: &Font) -> Rect {
        let size = self.size(ask, title, font);
        Rect::new(((screen - size) / 2.0).round(), size)
    }

    pub fn disc_rect(&self) -> Rect {
        Rect::from_center(Vec2::new(self.width / 2.0, self.pad + DISC / 2.0), Vec2::splat(DISC))
    }

    pub fn digit_rects(&self, ask: &Ask, title: &Font, font: &Font) -> Vec<Rect> {
        let count = self.slots(ask);
        if count == 0 {
            return Vec::new();
        }
        let step = self.step.min((self.width - self.pad * 2.0) / count as f64);
        let size = Vec2::new(step - 6.0, self.digit.y);
        let left = (self.width - step * count as f64) / 2.0 + 3.0;
        let top = self.top(ask, title, font);
        (0..count)
            .map(|at| Rect::new(Vec2::new(left + step * at as f64, top), size))
            .collect()
    }

    fn pin_rect(&self, ask: &Ask, title: &Font, font: &Font) -> Rect {
        Rect::new(
            Vec2::new(self.pad, self.top(ask, title, font)),
            Vec2::new(self.width - self.pad * 2.0, 40.0),
        )
    }

    fn half(&self) -> f64 {
        (self.width - self.pad * 2.0 - 10.0) / 2.0
    }

    pub fn go_rect(&self, size: Vec2, ask: &Ask) -> Option<Rect> {
        if matches!(ask.question, Question::Show(_, _)) {
            return None;
        }
        let half = self.half();
        Some(Rect::new(
            Vec2::new(size.x - self.pad - half, size.y - self.pad - self.button.y),
            Vec2::new(half, self.button.y),
        ))
    }

    pub fn cancel_rect(&self, size: Vec2, ask: &Ask) -> Rect {
        let half = self.half();
        let x = match self.go_rect(size, ask) {
            Some(_) => self.pad,
            None => (size.x - half) / 2.0,
        };
        Rect::new(Vec2::new(x, size.y - self.pad - self.button.y), Vec2::new(half, self.button.y))
    }

    pub fn hit(&self, size: Vec2, ask: &Ask, inside: Vec2) -> Option<Press> {
        if self.go_rect(size, ask).is_some_and(|rect| rect.contains(inside)) {
            return Some(Press::Go);
        }
        self.cancel_rect(size, ask).contains(inside).then_some(Press::Cancel)
    }

    pub fn go_label(ask: &Ask) -> &'static str {
        match ask.question {
            Question::Confirm(_) => crate::lang::t("They match"),
            Question::Allow => crate::lang::t("Allow"),
            _ => crate::lang::t("Set Up"),
        }
    }

    pub fn paint(&self, ask: &Ask, typed: usize, title: &Font, font: &Font) -> Image {
        let size = self.size(ask, title, font);
        let mut canvas = Canvas::new(size.x as u32, size.y as u32);
        canvas.round_rect(Rect::new(Vec2::ZERO, size), self.radius, self.back, 1.0);
        let middle = size.x / 2.0;
        let disc = self.disc_rect();
        canvas.shadow(disc, DISC / 2.0, 8.0, 0.25);
        canvas.shade(disc, DISC / 2.0, lift(self.accent, 28), self.accent, 1.0);
        let glyph = crate::blue::glyph(ask.kind, crate::paint::dense(42.0), [255, 255, 255]);
        canvas.stamp(&glyph, disc.center() - Vec2::splat(21.0), 1.0);
        let mut y = self.pad + DISC + 16.0;
        let heading = title.paint_bold(&ask.title(), self.ink);
        canvas.stamp(&heading, Vec2::new(middle - crate::paint::logical(&heading).x / 2.0, y).round(), 1.0);
        y += title.line_height().ceil() + 6.0;
        for text in self.lines(ask, font) {
            let painted = font.paint(&text, self.dim);
            canvas.stamp(&painted, Vec2::new(middle - crate::paint::logical(&painted).x / 2.0, y).round(), 1.0);
            y += font.line_height().ceil();
        }
        for rect in self.digit_rects(ask, title, font) {
            canvas.round_rect(rect, 9.0, self.rule, 1.0);
            canvas.round_rect(Rect::new(rect.origin + Vec2::splat(1.0), rect.size - Vec2::splat(2.0)), 8.0, self.well, 1.0);
        }
        if ask.question == Question::TypePin {
            let field = self.pin_rect(ask, title, font);
            canvas.round_rect(field, 8.0, self.rule, 1.0);
            canvas.round_rect(Rect::new(field.origin + Vec2::splat(1.0), field.size - Vec2::splat(2.0)), 7.0, self.well, 1.0);
            if typed == 0 {
                let hint = font.paint(crate::lang::t("PIN"), self.dim);
                canvas.stamp(&hint, Vec2::new(field.origin.x + 12.0, field.center().y - crate::paint::logical(&hint).y / 2.0).round(), 0.7);
            }
        }
        let go = self.go_rect(size, ask);
        let cancel = self.cancel_rect(size, ask);
        canvas.round_rect(cancel, 8.0, self.off, 1.0);
        let painted = font.paint(crate::lang::t("Cancel"), self.ink);
        canvas.stamp(&painted, (cancel.center() - crate::paint::logical(&painted) / 2.0).round(), 1.0);
        if let Some(go) = go {
            canvas.round_rect(go, 8.0, self.accent, 1.0);
            let painted = font.paint_weight(Look::go_label(ask), [255, 255, 255], 600.0);
            canvas.stamp(&painted, (go.center() - crate::paint::logical(&painted) / 2.0).round(), 1.0);
        }
        canvas.image
    }

    pub fn pin_dots(&self, ask: &Ask, typed: usize, title: &Font, font: &Font) -> Vec<Vec2> {
        if ask.question != Question::TypePin {
            return Vec::new();
        }
        let field = self.pin_rect(ask, title, font);
        (0..typed.min(MOST_TYPED))
            .map(|slot| Vec2::new(field.origin.x + 18.0 + slot as f64 * 15.0, field.center().y))
            .collect()
    }

    pub fn digit(&self, glyph: char, lit: bool, big: &Font) -> Image {
        let painted = big.paint_weight(&glyph.to_string(), if lit { self.accent } else { self.ink }, 600.0);
        let mut canvas = Canvas::new(self.digit.x as u32, self.digit.y as u32);
        let at = (self.digit - crate::paint::logical(&painted)) / 2.0;
        canvas.stamp(&painted, at.round(), 1.0);
        canvas.image
    }

    pub fn dot(&self) -> Image {
        let mut canvas = Canvas::new(10, 10);
        canvas.circle(Vec2::splat(5.0), 4.0, self.ink, 1.0);
        canvas.image
    }
}

fn lift(color: [u8; 3], by: u8) -> [u8; 3] {
    color.map(|channel| channel.saturating_add(by))
}

pub fn shown(ask: &Ask, typed: &str) -> Vec<(char, bool)> {
    match &ask.question {
        Question::Confirm(key) => format!("{key:06}").chars().map(|c| (c, false)).collect(),
        Question::Show(code, entered) => code
            .chars()
            .take(MOST_TYPED)
            .enumerate()
            .map(|(at, c)| (c, at < usize::from(*entered)))
            .collect(),
        Question::Type => typed.chars().take(6).map(|c| (c, false)).collect(),
        Question::TypePin | Question::Allow => Vec::new(),
    }
}

pub fn takes(ask: &Ask, typed: &str, c: char) -> bool {
    match ask.question {
        Question::Type => c.is_ascii_digit() && typed.chars().count() < 6,
        Question::TypePin => c.is_ascii_alphanumeric() && typed.chars().count() < MOST_TYPED,
        _ => false,
    }
}

pub fn ready(ask: &Ask, typed: &str) -> bool {
    match ask.question {
        Question::Type => typed.chars().count() == 6,
        Question::TypePin => !typed.is_empty(),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::blue::Kind;

    fn ask(question: Question) -> Ask {
        Ask {
            id: 1,
            address: "AA:BB:CC:DD:EE:FF".into(),
            name: "Buds".into(),
            kind: Kind::Earbuds,
            question,
        }
    }

    #[test]
    fn a_confirm_shows_six_digits_and_two_buttons() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        let font = Font::find(None, 13.0).unwrap();
        let title = Font::find(None, 17.0).unwrap();
        let confirm = ask(Question::Confirm(4321));
        let size = look.size(&confirm, &title, &font);
        let digits = look.digit_rects(&confirm, &title, &font);
        assert_eq!(digits.len(), 6);
        assert!(digits.iter().all(|rect| rect.max().x <= size.x && rect.max().y < size.y - look.button.y));
        assert_eq!(shown(&confirm, "").iter().map(|(c, _)| *c).collect::<String>(), "004321");
        let go = look.go_rect(size, &confirm).unwrap();
        assert_eq!(look.hit(size, &confirm, go.center()), Some(Press::Go));
        assert_eq!(look.hit(size, &confirm, look.cancel_rect(size, &confirm).center()), Some(Press::Cancel));
        crate::paint::same_at_two("pairing", || look.paint(&confirm, 0, &title, &font));
    }

    #[test]
    fn a_shown_code_lights_what_was_typed_and_only_cancels() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        let font = Font::find(None, 13.0).unwrap();
        let show = ask(Question::Show("123456".into(), 2));
        let lit: Vec<bool> = shown(&show, "").iter().map(|(_, lit)| *lit).collect();
        assert_eq!(lit, [true, true, false, false, false, false]);
        let size = look.size(&show, &font, &font);
        assert_eq!(look.go_rect(size, &show), None);
        assert_eq!(look.hit(size, &show, look.cancel_rect(size, &show).center()), Some(Press::Cancel));
    }

    #[test]
    fn typing_takes_only_what_the_question_wants() {
        let typed = ask(Question::Type);
        assert!(takes(&typed, "12", '3'));
        assert!(!takes(&typed, "12", 'a'));
        assert!(!takes(&typed, "123456", '7'));
        assert!(!ready(&typed, "123"));
        assert!(ready(&typed, "123456"));
        let pin = ask(Question::TypePin);
        assert!(takes(&pin, "", '0'));
        assert!(!ready(&pin, ""));
        assert!(!takes(&ask(Question::Allow), "", '1'));
    }
}
