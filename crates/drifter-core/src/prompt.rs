use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ask {
    pub message: String,
    pub user: String,
    pub typed: usize,
    pub error: Option<String>,
    pub busy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Cancel,
    Go,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub width: f64,
    pub pad: f64,
    pub field: f64,
    pub button: Vec2,
    pub radius: f64,
    pub back: [u8; 3],
    pub well: [u8; 3],
    pub rule: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub accent: [u8; 3],
    pub off: [u8; 3],
    pub bad: [u8; 3],
}

impl Default for Look {
    fn default() -> Self {
        Look {
            width: 440.0,
            pad: 24.0,
            field: 38.0,
            button: Vec2::new(130.0, 32.0),
            radius: 14.0,
            back: [37, 40, 50],
            well: [30, 32, 40],
            rule: [58, 62, 78],
            ink: [236, 238, 244],
            dim: [146, 152, 166],
            accent: [64, 128, 238],
            off: [72, 77, 94],
            bad: [255, 120, 110],
        }
    }
}

pub const MOST_DOTS: usize = 24;

impl Look {
    fn lines(&self, ask: &Ask, font: &Font) -> Vec<String> {
        crate::notify::wrap(font, &ask.message, self.width - self.pad * 2.0, 4)
    }

    pub fn size(&self, ask: &Ask, font: &Font) -> Vec2 {
        let line = font.line_height().ceil();
        let text = line * (3.0 + self.lines(ask, font).len() as f64);
        Vec2::new(
            self.width,
            (self.pad * 2.0 + text + 18.0 + self.field + line + 24.0 + self.button.y).ceil(),
        )
    }

    pub fn rect(&self, screen: Vec2, ask: &Ask, font: &Font) -> Rect {
        let size = self.size(ask, font);
        Rect::new(((screen - size) / 2.0).round(), size)
    }

    fn field_rect(&self, size: Vec2, line: f64) -> Rect {
        let top = size.y - self.pad - self.button.y - line - 24.0 - self.field;
        Rect::new(
            Vec2::new(self.pad, top),
            Vec2::new(size.x - self.pad * 2.0, self.field),
        )
    }

    fn go_rect(&self, size: Vec2) -> Rect {
        Rect::new(
            Vec2::new(
                size.x - self.pad - self.button.x,
                size.y - self.pad - self.button.y,
            ),
            self.button,
        )
    }

    fn cancel_rect(&self, size: Vec2) -> Rect {
        let go = self.go_rect(size);
        Rect::new(
            Vec2::new(go.origin.x - 10.0 - self.button.x, go.origin.y),
            self.button,
        )
    }

    pub fn hit(&self, size: Vec2, inside: Vec2) -> Option<Press> {
        if self.go_rect(size).contains(inside) {
            return Some(Press::Go);
        }
        self.cancel_rect(size)
            .contains(inside)
            .then_some(Press::Cancel)
    }

    pub fn paint(&self, ask: &Ask, font: &Font) -> Image {
        let size = self.size(ask, font);
        let mut canvas = Canvas::new(size.x as u32, size.y as u32);
        canvas.round_rect(Rect::new(Vec2::ZERO, size), self.radius, self.back, 1.0);
        let line = font.line_height().ceil();
        let mut y = self.pad;
        let title = font.paint("Authentication required", self.ink);
        canvas.stamp(&title, Vec2::new(self.pad, y), 1.0);
        y += line * 1.5;
        for text in self.lines(ask, font) {
            let painted = font.paint(&text, self.dim);
            canvas.stamp(&painted, Vec2::new(self.pad, y), 1.0);
            y += line;
        }
        y += line * 0.5;
        let who = font.paint(&format!("Password for {}", ask.user), self.ink);
        canvas.stamp(&who, Vec2::new(self.pad, y), 1.0);

        let field = self.field_rect(size, line);
        canvas.round_rect(field, 8.0, self.rule, 1.0);
        canvas.round_rect(
            Rect::new(
                field.origin + Vec2::splat(1.0),
                field.size - Vec2::splat(2.0),
            ),
            7.0,
            self.well,
            1.0,
        );
        let dots = ask.typed.min(MOST_DOTS);
        for slot in 0..dots {
            let center = Vec2::new(field.origin.x + 16.0 + slot as f64 * 14.0, field.center().y);
            canvas.circle(center, 4.0, self.ink, 1.0);
        }
        let note = match (ask.busy, &ask.error) {
            (true, _) => Some(("Checking…".to_string(), self.dim)),
            (false, Some(error)) => Some((error.clone(), self.bad)),
            _ => None,
        };
        if let Some((text, ink)) = note {
            let painted = font.paint(&text, ink);
            canvas.stamp(&painted, Vec2::new(self.pad, field.max().y + 8.0), 1.0);
        }
        for (rect, label, color) in [
            (self.cancel_rect(size), "Cancel", self.off),
            (self.go_rect(size), "Authenticate", self.accent),
        ] {
            canvas.round_rect(rect, 8.0, color, if ask.busy { 0.5 } else { 1.0 });
            let painted = font.paint(label, self.ink);
            canvas.stamp(&painted, rect.center() - painted.size() / 2.0, 1.0);
        }
        canvas.image
    }
}

pub fn read_helper_line(line: &str) -> Helper {
    let line = line.trim_end();
    if line == "SUCCESS" {
        return Helper::Success;
    }
    if line == "FAILURE" {
        return Helper::Failure;
    }
    for (prefix, kind) in [
        (
            "PAM_PROMPT_ECHO_OFF",
            Helper::Secret as fn(String) -> Helper,
        ),
        ("PAM_PROMPT_ECHO_ON", Helper::Plain),
        ("PAM_ERROR_MSG", Helper::Error),
        ("PAM_TEXT_INFO", Helper::Info),
    ] {
        if let Some(rest) = line.strip_prefix(prefix) {
            return kind(rest.trim().to_string());
        }
    }
    Helper::Info(line.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Helper {
    Secret(String),
    Plain(String),
    Error(String),
    Info(String),
    Success,
    Failure,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask() -> Ask {
        Ask {
            message: "Authentication is needed to run `/usr/bin/true' as the super user".into(),
            user: "dh".into(),
            typed: 5,
            error: None,
            busy: false,
        }
    }

    fn font() -> Font {
        Font::find(None, 13.0).expect("a font")
    }

    #[test]
    fn the_helper_speaks_in_prefixed_lines() {
        assert_eq!(
            read_helper_line("PAM_PROMPT_ECHO_OFF Password: \n"),
            Helper::Secret("Password:".into())
        );
        assert_eq!(read_helper_line("SUCCESS\n"), Helper::Success);
        assert_eq!(read_helper_line("FAILURE"), Helper::Failure);
        assert_eq!(
            read_helper_line("PAM_ERROR_MSG Authentication failure"),
            Helper::Error("Authentication failure".into())
        );
        assert_eq!(
            read_helper_line("something else"),
            Helper::Info("something else".into())
        );
    }

    #[test]
    fn the_buttons_are_found_and_the_rest_is_not() {
        let look = Look::default();
        let size = look.size(&ask(), &font());
        let go = look.go_rect(size).center();
        let cancel = look.cancel_rect(size).center();
        assert_eq!(look.hit(size, go), Some(Press::Go));
        assert_eq!(look.hit(size, cancel), Some(Press::Cancel));
        assert_eq!(look.hit(size, Vec2::new(5.0, 5.0)), None);
    }

    #[test]
    fn the_card_paints_at_its_size_in_the_middle_of_the_screen() {
        let look = Look::default();
        let font = font();
        let image = look.paint(&ask(), &font);
        let rect = look.rect(Vec2::new(1920.0, 1080.0), &ask(), &font);
        assert_eq!(image.size(), rect.size);
        assert!((rect.center().x - 960.0).abs() <= 1.0);
        let line = font.line_height().ceil();
        let field = look.field_rect(rect.size, line);
        assert!(field.max().y + line + 8.0 < look.go_rect(rect.size).origin.y);
        assert!(look.cancel_rect(rect.size).origin.x > look.pad);
    }
}
