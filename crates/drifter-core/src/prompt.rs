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
    pub join: bool,
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

impl Look {
    pub fn themed(p: &crate::theme::Palette) -> Look {
        Look {
            width: 360.0,
            pad: 24.0,
            field: 34.0,
            button: Vec2::new(130.0, 30.0),
            radius: 16.0,
            back: p.surface,
            well: p.well,
            rule: p.rule,
            ink: p.ink,
            dim: p.dim,
            accent: [64, 128, 238],
            off: p.off,
            bad: p.alert,
        }
    }
}

impl Default for Look {
    fn default() -> Self {
        Look::themed(&crate::theme::DARK)
    }
}

pub const MOST_DOTS: usize = 18;
const LOCK: f64 = 52.0;
const ASK: &str = "Enter your password to allow this.";

pub fn plain(message: &str) -> String {
    let message = message.trim();
    if let Some(command) = message
        .strip_prefix("Authentication is needed to run `")
        .and_then(|rest| rest.split('\'').next())
    {
        let words: Vec<&str> = command.split_whitespace().collect();
        let installs = words.iter().position(|word| *word == "install");
        let wanted: Vec<&str> = installs
            .map(|at| words[at + 1..].iter().copied().filter(|word| !word.starts_with('-')).collect())
            .unwrap_or_default();
        return match wanted.as_slice() {
            [] if installs.is_some() => crate::lang::t("Drifter wants to install software.").to_string(),
            [] => crate::lang::t("Drifter wants to make a change.").to_string(),
            names => crate::lang::tf("Drifter wants to install {}.", &[&(names.iter().map(|name| pretty(name)).collect::<Vec<_>>().join(", "))]),
        };
    }
    let rest = ["Authentication is required to ", "Authentication is needed to "]
        .iter()
        .find_map(|head| message.strip_prefix(head));
    match rest {
        Some(rest) => crate::lang::tf("Drifter wants to {}", &[&(rest.trim_end_matches('.'))]).trim().to_string() + ".",
        None => message.to_string(),
    }
}

fn pretty(package: &str) -> String {
    crate::desktop::OFFERS
        .iter()
        .find(|offer| offer.apt == package || offer.dnf == package)
        .map(|offer| offer.name.to_string())
        .unwrap_or_else(|| title(package))
}

fn title(word: &str) -> String {
    let mut chars = word.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

impl Look {
    fn lines(&self, ask: &Ask, font: &Font) -> Vec<String> {
        let message = match ask.join {
            true => ask.message.clone(),
            false => plain(&ask.message),
        };
        crate::notify::wrap(font, &message, self.width - self.pad * 2.0, 3)
    }

    fn wifi(&self, canvas: &mut Canvas, center: Vec2) {
        canvas.shadow(Rect::from_center(center, Vec2::splat(52.0)), 26.0, 5.0, 0.2);
        canvas.circle(center, 26.0, self.accent, 1.0);
        let mark = crate::net::glyph(crate::net::Glyph::Wifi, crate::paint::dense(30.0), [255, 255, 255]);
        canvas.stamp(&mark, center - Vec2::new(15.0, 16.0), 1.0);
    }

    fn top(&self, ask: &Ask, font: &Font) -> f64 {
        let line = font.line_height().ceil();
        self.pad + LOCK + 14.0 + line * self.lines(ask, font).len() as f64 + 4.0 + line + 16.0
    }

    pub fn size(&self, ask: &Ask, font: &Font) -> Vec2 {
        let line = font.line_height().ceil();
        let tall = self.top(ask, font) + self.field + 6.0 + line + 10.0 + self.button.y + self.pad;
        Vec2::new(self.width, tall.ceil())
    }

    pub fn rect(&self, screen: Vec2, ask: &Ask, font: &Font) -> Rect {
        let size = self.size(ask, font);
        Rect::new(((screen - size) / 2.0).round(), size)
    }

    fn field_rect(&self, size: Vec2, line: f64) -> Rect {
        let top = size.y - self.pad - self.button.y - 10.0 - line - 6.0 - self.field;
        Rect::new(Vec2::new(self.pad, top), Vec2::new(size.x - self.pad * 2.0, self.field))
    }

    fn half(&self, size: Vec2) -> f64 {
        (size.x - self.pad * 2.0 - 10.0) / 2.0
    }

    fn go_rect(&self, size: Vec2) -> Rect {
        let half = self.half(size);
        Rect::new(
            Vec2::new(size.x - self.pad - half, size.y - self.pad - self.button.y),
            Vec2::new(half, self.button.y),
        )
    }

    fn cancel_rect(&self, size: Vec2) -> Rect {
        let half = self.half(size);
        Rect::new(
            Vec2::new(self.pad, size.y - self.pad - self.button.y),
            Vec2::new(half, self.button.y),
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

    fn lock(&self, canvas: &mut Canvas, center: Vec2) {
        let body = Rect::from_center(center + Vec2::new(0.0, 8.0), Vec2::new(40.0, 30.0));
        let bow = Rect::new(Vec2::new(center.x - 12.0, center.y - 24.0), Vec2::new(24.0, 40.0));
        canvas.ring(bow, 12.0, 5.0, [168, 172, 182], 1.0);
        canvas.shadow(body, 8.0, 5.0, 0.22);
        canvas.shade(body, 8.0, [255, 196, 64], [236, 150, 20], 1.0);
        canvas.circle(body.center() - Vec2::new(0.0, 2.0), 4.0, [150, 92, 10], 1.0);
        canvas.round_rect(
            Rect::from_center(body.center() + Vec2::new(0.0, 4.0), Vec2::new(3.0, 8.0)),
            1.5,
            [150, 92, 10],
            1.0,
        );
    }

    pub fn paint(&self, ask: &Ask, font: &Font) -> Image {
        let size = self.size(ask, font);
        let mut canvas = Canvas::new(size.x as u32, size.y as u32);
        canvas.round_rect(Rect::new(Vec2::ZERO, size), self.radius, self.back, 1.0);
        let line = font.line_height().ceil();
        let middle = size.x / 2.0;
        match ask.join {
            true => self.wifi(&mut canvas, Vec2::new(middle, self.pad + LOCK / 2.0)),
            false => self.lock(&mut canvas, Vec2::new(middle, self.pad + LOCK / 2.0)),
        }
        let mut y = self.pad + LOCK + 14.0;
        for text in self.lines(ask, font) {
            let painted = font.paint_bold(&text, self.ink);
            canvas.stamp(&painted, Vec2::new(middle - crate::paint::logical(&painted).x / 2.0, y).round(), 1.0);
            y += line;
        }
        let painted = font.paint(if ask.join { crate::lang::t("It is saved for the next time.") } else { ASK }, self.dim);
        canvas.stamp(&painted, Vec2::new(middle - crate::paint::logical(&painted).x / 2.0, y + 4.0).round(), 1.0);

        let field = self.field_rect(size, line);
        canvas.round_rect(field, 8.0, self.rule, 1.0);
        canvas.round_rect(
            Rect::new(field.origin + Vec2::splat(1.0), field.size - Vec2::splat(2.0)),
            7.0,
            self.well,
            1.0,
        );
        let dots = ask.typed.min(MOST_DOTS);
        match dots {
            0 => {
                let hint = font.paint(crate::lang::t("Password"), self.dim);
                let at = Vec2::new(field.origin.x + 12.0, field.center().y - crate::paint::logical(&hint).y / 2.0);
                canvas.stamp(&hint, at.round(), 0.7);
            }
            _ => {
                for slot in 0..dots {
                    let center = Vec2::new(field.origin.x + 16.0 + slot as f64 * 13.0, field.center().y);
                    canvas.circle(center, 3.5, self.ink, 1.0);
                }
            }
        }
        let note = match (ask.busy, &ask.error) {
            (true, _) => Some((crate::lang::t("Checking…").to_string(), self.dim)),
            (false, Some(error)) => Some((error.clone(), self.bad)),
            _ => None,
        };
        if let Some((text, ink)) = note {
            let painted = font.paint(&text, ink);
            canvas.stamp(&painted, Vec2::new(middle - crate::paint::logical(&painted).x / 2.0, field.max().y + 6.0).round(), 1.0);
        }
        for (rect, label, color, ink) in [
            (self.cancel_rect(size), crate::lang::t("Cancel"), self.off, self.ink),
            (self.go_rect(size), if ask.join { crate::lang::t("Join") } else { crate::lang::t("Allow") }, self.accent, [255, 255, 255]),
        ] {
            canvas.round_rect(rect, 8.0, color, if ask.busy { 0.5 } else { 1.0 });
            let painted = font.paint(label, ink);
            canvas.stamp(&painted, (rect.center() - crate::paint::logical(&painted) / 2.0).round(), 1.0);
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
    fn a_prompt_at_density_two_matches_density_one() {
        let look = Look::default();
        let font = font();
        crate::paint::same_at_two("prompt", || look.paint(&ask(), &font));
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
        assert!(field.max().y + line + 6.0 <= look.go_rect(rect.size).origin.y);
        assert!(look.cancel_rect(rect.size).max().x < look.go_rect(rect.size).origin.x);
    }

    #[test]
    fn the_prompt_says_what_it_is_for_and_never_shows_the_command() {
        assert_eq!(
            plain("Authentication is needed to run `/usr/bin/env DEBIAN_FRONTEND=noninteractive apt-get install -y thunderbird' as the super user"),
            "Drifter wants to install Thunderbird."
        );
        assert_eq!(
            plain("Authentication is needed to run `/usr/bin/env DEBIAN_FRONTEND=noninteractive apt-get install -y mpv' as the super user"),
            "Drifter wants to install mpv."
        );
        assert_eq!(
            plain("Authentication is needed to run `/usr/bin/true' as the super user"),
            "Drifter wants to make a change."
        );
        assert_eq!(
            plain("Authentication is required to change the system time."),
            "Drifter wants to change the system time."
        );
    }
}
