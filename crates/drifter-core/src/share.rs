use crate::blue::Kind;
use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

pub const ACROSS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    Idle,
    Waiting,
    Sending(u8),
    Sent,
    Declined,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub key: String,
    pub name: String,
    pub kind: Kind,
    pub mac: Option<String>,
    pub state: State,
}

impl Target {
    pub fn note(&self) -> &'static str {
        match self.state {
            State::Idle => "",
            State::Waiting => crate::lang::t("Waiting…"),
            State::Sending(_) => crate::lang::t("Sending…"),
            State::Sent => crate::lang::t("Sent"),
            State::Declined => crate::lang::t("Declined"),
            State::Failed => crate::lang::t("Failed"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    Target(usize),
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Look {
    pub width: f64,
    pub pad: f64,
    pub bubble: f64,
    pub cell: Vec2,
    pub button: Vec2,
    pub radius: f64,
    pub back: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub off: [u8; 3],
    pub accent: [u8; 3],
}

const HEAD: f64 = 58.0;
const EMPTY: f64 = 96.0;

impl Look {
    pub fn themed(p: &crate::theme::Palette, accent: [u8; 3]) -> Look {
        Look {
            width: 460.0,
            pad: 22.0,
            bubble: 62.0,
            cell: Vec2::new(104.0, 112.0),
            button: Vec2::new(120.0, 30.0),
            radius: 18.0,
            back: p.surface,
            ink: p.ink,
            dim: p.dim,
            off: p.off,
            accent,
        }
    }

    fn rows(count: usize) -> usize {
        count.div_ceil(ACROSS).max(1)
    }

    fn body(&self, count: usize) -> f64 {
        match count {
            0 => EMPTY,
            _ => self.cell.y * Look::rows(count) as f64,
        }
    }

    pub fn size(&self, count: usize) -> Vec2 {
        Vec2::new(self.width, (self.pad + HEAD + self.body(count) + 12.0 + self.button.y + self.pad).ceil())
    }

    pub fn rect(&self, screen: Vec2, count: usize) -> Rect {
        let size = self.size(count);
        Rect::new(((screen - size) / 2.0).round(), size)
    }

    pub fn cell_rect(&self, count: usize, index: usize) -> Rect {
        let (row, col) = (index / ACROSS, index % ACROSS);
        let in_row = (count - row * ACROSS).min(ACROSS).max(1);
        let left = (self.width - self.cell.x * in_row as f64) / 2.0;
        Rect::new(
            Vec2::new(left + self.cell.x * col as f64, self.pad + HEAD + self.cell.y * row as f64),
            self.cell,
        )
    }

    pub fn bubble_rect(&self, count: usize, index: usize) -> Rect {
        let cell = self.cell_rect(count, index);
        Rect::from_center(Vec2::new(cell.center().x, cell.origin.y + 6.0 + self.bubble / 2.0), Vec2::splat(self.bubble))
    }

    pub fn cancel_rect(&self, count: usize) -> Rect {
        let size = self.size(count);
        Rect::new(
            Vec2::new((size.x - self.button.x) / 2.0, size.y - self.pad - self.button.y),
            self.button,
        )
    }

    pub fn status_at(&self, count: usize) -> Vec2 {
        Vec2::new(self.width / 2.0, self.pad + HEAD + self.body(count) / 2.0)
    }

    pub fn hit(&self, count: usize, inside: Vec2) -> Option<Press> {
        if self.cancel_rect(count).contains(inside) {
            return Some(Press::Cancel);
        }
        (0..count).find(|at| self.cell_rect(count, *at).contains(inside)).map(Press::Target)
    }

    pub fn paint(&self, count: usize, summary: &str, empty: &str, done: bool, title: &Font, font: &Font) -> Image {
        let size = self.size(count);
        let mut canvas = Canvas::new(size.x as u32, size.y as u32);
        canvas.round_rect(Rect::new(Vec2::ZERO, size), self.radius, self.back, 1.0);
        let heading = title.paint_bold("AirDrop", self.ink);
        canvas.stamp(&heading, Vec2::new((size.x - crate::paint::logical(&heading).x) / 2.0, self.pad).round(), 1.0);
        let words = font.paint(&crate::control::fit(font, summary, size.x - self.pad * 2.0), self.dim);
        canvas.stamp(&words, Vec2::new((size.x - crate::paint::logical(&words).x) / 2.0, self.pad + 30.0).round(), 1.0);
        if count == 0 {
            for (at, line) in crate::notify::wrap(font, empty, size.x - self.pad * 4.0, 3).iter().enumerate() {
                let painted = font.paint(line, self.dim);
                let y = self.pad + HEAD + EMPTY / 2.0 - 10.0 + at as f64 * font.line_height().ceil();
                canvas.stamp(&painted, Vec2::new((size.x - crate::paint::logical(&painted).x) / 2.0, y).round(), 1.0);
            }
        }
        let cancel = self.cancel_rect(count);
        canvas.round_rect(cancel, 8.0, self.off, 1.0);
        let label = font.paint(if done { crate::lang::t("Done") } else { crate::lang::t("Cancel") }, self.ink);
        canvas.stamp(&label, (cancel.center() - crate::paint::logical(&label) / 2.0).round(), 1.0);
        canvas.image
    }

    pub fn paint_target(&self, target: &Target, font: &Font, small: &Font) -> Image {
        let mut canvas = Canvas::new(self.cell.x as u32, self.cell.y as u32);
        let middle = Vec2::new(self.cell.x / 2.0, 6.0 + self.bubble / 2.0);
        let lit = matches!(target.state, State::Sent);
        canvas.shade(
            Rect::from_center(middle, Vec2::splat(self.bubble)),
            self.bubble / 2.0,
            if lit { self.accent } else { lift(self.off, 22) },
            if lit { self.accent } else { self.off },
            1.0,
        );
        let glyph = crate::blue::glyph(target.kind, crate::paint::dense(30.0), if lit { [255, 255, 255] } else { self.ink });
        canvas.stamp(&glyph, middle - Vec2::splat(15.0), 1.0);
        let lines = crate::notify::wrap(font, &target.name, self.cell.x - 8.0, 2);
        let mut y = 6.0 + self.bubble + 6.0;
        for line in &lines {
            let painted = font.paint(line, self.ink);
            canvas.stamp(&painted, Vec2::new((self.cell.x - crate::paint::logical(&painted).x) / 2.0, y).round(), 1.0);
            y += font.line_height().ceil() - 1.0;
        }
        let note = target.note();
        if !note.is_empty() {
            let ink = match target.state {
                State::Declined | State::Failed => [255, 99, 88],
                _ => self.dim,
            };
            let painted = small.paint(note, ink);
            canvas.stamp(&painted, Vec2::new((self.cell.x - crate::paint::logical(&painted).x) / 2.0, y).round(), 1.0);
        }
        canvas.image
    }

    pub fn ring(&self, share: f64) -> Image {
        let side = self.bubble + 10.0;
        let mut canvas = Canvas::new(side as u32, side as u32);
        canvas.arc(Vec2::splat(side / 2.0), side / 2.0 - 3.0, 3.0, 1.0, self.off, 0.6);
        canvas.arc(Vec2::splat(side / 2.0), side / 2.0 - 3.0, 3.0, share.clamp(0.0, 1.0), self.accent, 1.0);
        canvas.image
    }

    pub fn check(&self) -> Image {
        let side = 24.0;
        let mut canvas = Canvas::new(side as u32, side as u32);
        canvas.circle(Vec2::splat(side / 2.0), side / 2.0, self.accent, 1.0);
        canvas.line(Vec2::new(6.5, 12.5), Vec2::new(10.5, 16.5), 2.4, [255, 255, 255], 1.0);
        canvas.line(Vec2::new(10.5, 16.5), Vec2::new(17.5, 8.0), 2.4, [255, 255, 255], 1.0);
        canvas.image
    }
}

fn lift(color: [u8; 3], by: u8) -> [u8; 3] {
    color.map(|channel| channel.saturating_add(by))
}

pub fn summary(files: &[std::path::PathBuf]) -> String {
    match files {
        [] => String::new(),
        [one] => one.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(),
        many => crate::lang::tf("{} files", &[&many.len()]),
    }
}

pub fn mac_from_link_local(address: &str) -> Option<String> {
    let inner = address.trim().trim_start_matches('[');
    let inner = inner.split(['%', ']']).next()?;
    let ip: std::net::Ipv6Addr = inner.parse().ok()?;
    let s = ip.segments();
    if s[0] & 0xffc0 != 0xfe80 || s[5] & 0x00ff != 0x00ff || s[6] >> 8 != 0xfe {
        return None;
    }
    let bytes = [
        ((s[4] >> 8) as u8) ^ 0x02,
        (s[4] & 0xff) as u8,
        (s[5] >> 8) as u8,
        (s[6] & 0xff) as u8,
        (s[7] >> 8) as u8,
        (s[7] & 0xff) as u8,
    ];
    Some(bytes.iter().map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(":"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(name: &str) -> Target {
        Target {
            key: name.into(),
            name: name.into(),
            kind: Kind::Phone,
            mac: None,
            state: State::Idle,
        }
    }

    #[test]
    fn the_sheet_grows_by_rows_and_its_bubbles_are_hit() {
        let look = Look::themed(&crate::theme::DARK, [10, 122, 255]);
        assert!(look.size(5).y > look.size(4).y, "a second row");
        assert_eq!(look.size(1).y, look.size(4).y);
        let cell = look.cell_rect(3, 1);
        assert_eq!(look.hit(3, cell.center()), Some(Press::Target(1)));
        assert_eq!(look.hit(3, look.cancel_rect(3).center()), Some(Press::Cancel));
        let lone = look.cell_rect(1, 0);
        assert!((lone.center().x - look.width / 2.0).abs() < 0.5, "one bubble sits in the middle");
        let font = Font::find(None, 13.0).unwrap();
        crate::paint::same_at_two("share sheet", || look.paint(2, "photo.jpg", "", false, &font, &font));
        crate::paint::same_at_two("share target", || look.paint_target(&target("Jane's iPhone"), &font, &font));
    }

    #[test]
    fn a_link_local_address_gives_back_its_mac() {
        assert_eq!(mac_from_link_local("[fe80::a8bb:ccff:fedd:ee01%5]:8770").as_deref(), Some("aa:bb:cc:dd:ee:01"));
        assert_eq!(mac_from_link_local("fe80::1"), None);
        assert_eq!(summary(&[std::path::PathBuf::from("/x/photo.jpg")]), "photo.jpg");
        assert_eq!(summary(&[std::path::PathBuf::from("a"), std::path::PathBuf::from("b")]), "2 files");
    }
}
