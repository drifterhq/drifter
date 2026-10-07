use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

pub const WARM: [u8; 3] = [255, 159, 82];
pub const HOT: [u8; 3] = [255, 84, 84];
pub const COOL: [u8; 3] = [110, 190, 255];

pub fn heat(value: f64) -> [u8; 3] {
    let mix = |from: [u8; 3], to: [u8; 3], t: f64| -> [u8; 3] {
        std::array::from_fn(|c| (f64::from(from[c]) + (f64::from(to[c]) - f64::from(from[c])) * t.clamp(0.0, 1.0)).round() as u8)
    };
    match value < 0.88 {
        true => mix(COOL, WARM, ((value - 0.3) / 0.58).max(0.0)),
        false => mix(WARM, HOT, (value - 0.88) / 0.1),
    }
}
pub const HISTORY: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Text,
    Pills,
    Rings,
    Calm,
    Ticks,
}

impl Style {
    pub const ALL: [Style; 5] = [Style::Calm, Style::Pills, Style::Rings, Style::Ticks, Style::Text];

    pub fn parse(text: &str) -> Result<Style, String> {
        match text.trim().to_ascii_lowercase().as_str() {
            "text" => Ok(Style::Text),
            "pills" => Ok(Style::Pills),
            "rings" => Ok(Style::Rings),
            "calm" => Ok(Style::Calm),
            "ticks" => Ok(Style::Ticks),
            other => Err(format!("tray style {other:?} is not calm, pills, rings, ticks or text")),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Style::Text => "text",
            Style::Pills => "pills",
            Style::Rings => "rings",
            Style::Calm => "calm",
            Style::Ticks => "ticks",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Style::Text => "Text",
            Style::Pills => "Pills",
            Style::Rings => "Rings",
            Style::Calm => "Calm",
            Style::Ticks => "Ticks",
        }
    }
}

pub fn tone(ink: [u8; 3], value: f64) -> [u8; 3] {
    let mix = |from: [u8; 3], to: [u8; 3], t: f64| -> [u8; 3] {
        std::array::from_fn(|c| (f64::from(from[c]) + (f64::from(to[c]) - f64::from(from[c])) * t.clamp(0.0, 1.0)).round() as u8)
    };
    match value < 0.88 {
        true => mix(ink, WARM, (value - 0.7) / 0.18),
        false => mix(WARM, HOT, (value - 0.88) / 0.1),
    }
}

pub fn is_load(label: &str) -> bool {
    !matches!(label, "BAT" | "CHG")
}

fn share(value: u8) -> f64 {
    f64::from(value.min(100)) / 100.0
}

pub fn paint(style: Style, values: &[u8], ink: [u8; 3]) -> Image {
    match style {
        Style::Pills => pills(values, ink),
        Style::Rings => rings(values, ink),
        Style::Calm => calm(values, ink),
        Style::Ticks | Style::Text => ticks(values, ink),
    }
}

fn pills(values: &[u8], ink: [u8; 3]) -> Image {
    let count = values.len().max(1);
    let (w, h, step) = (5.0, 14.0, 7.5);
    let mut canvas = Canvas::new((step * count as f64 + 1.0).ceil() as u32, 16);
    for (at, value) in values.iter().enumerate() {
        let v = share(*value).max(0.05);
        let body = Rect::new(Vec2::new(1.0 + step * at as f64, 1.0), Vec2::new(w, h));
        canvas.ring(body, 2.5, 1.0, ink, 0.45);
        let inner = Rect::new(body.origin + Vec2::splat(1.5), body.size - Vec2::splat(3.0));
        let tall = inner.size.y * v;
        let fill = Rect::new(Vec2::new(inner.origin.x, inner.max().y - tall), Vec2::new(inner.size.x, tall));
        canvas.round_rect(fill, 1.0, tone(ink, v), 0.9);
    }
    canvas.image
}

fn rings(values: &[u8], ink: [u8; 3]) -> Image {
    let mut canvas = Canvas::new(16, 16);
    let center = Vec2::splat(8.0);
    for (value, radius) in values.iter().zip([7.0, 4.8, 2.6]) {
        let v = share(*value).max(0.02);
        arc(&mut canvas, center, radius, 1.25, 1.0, ink, 0.16);
        arc(&mut canvas, center, radius, 1.25, v, tone(ink, v), 0.9);
    }
    canvas.image
}

fn arc(canvas: &mut Canvas, center: Vec2, radius: f64, width: f64, part: f64, color: [u8; 3], alpha: f64) {
    let steps = (64.0 * part).ceil().max(1.0) as usize;
    let start = -std::f64::consts::FRAC_PI_2;
    let sweep = std::f64::consts::TAU * part.clamp(0.0, 1.0);
    let point = |t: f64| {
        let a = start + sweep * t;
        center + Vec2::new(a.cos(), a.sin()) * radius
    };
    let mut layer = Canvas::new(canvas.size().x as u32, canvas.size().y as u32);
    for i in 0..steps {
        let (a, b) = (i as f64 / steps as f64, (i + 1) as f64 / steps as f64);
        layer.line(point(a), point(b), width, color, 1.0);
    }
    canvas.stamp(&layer.image, Vec2::ZERO, alpha);
}

fn calm(values: &[u8], ink: [u8; 3]) -> Image {
    let load = values.iter().map(|v| share(*v)).fold(0.0, f64::max);
    let busy = ((load - 0.5) / 0.4).clamp(0.0, 1.0);
    let mut canvas = Canvas::new(14, 14);
    canvas.ring(Rect::new(Vec2::splat(1.4), Vec2::splat(11.2)), 5.6, 1.2, ink, 0.32 + busy * 0.45);
    if busy > 0.01 {
        canvas.circle(Vec2::splat(7.0), 5.0 * busy, tone(ink, load), 0.25 + busy * 0.55);
    }
    canvas.image
}

fn ticks(values: &[u8], ink: [u8; 3]) -> Image {
    let count = values.len().max(1);
    let mut canvas = Canvas::new((5.0 * count as f64 + 1.0).ceil() as u32, 14);
    for (at, value) in values.iter().enumerate() {
        let v = share(*value).max(0.06);
        let x = 2.5 + 5.0 * at as f64;
        canvas.line(Vec2::new(x, 12.5), Vec2::new(x, 1.5), 2.2, ink, 0.2);
        canvas.line(Vec2::new(x, 12.5), Vec2::new(x, 12.5 - 11.0 * v), 2.2, tone(ink, v), 0.92);
    }
    canvas.image
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Card {
    pub width: f64,
    pub back: [u8; 3],
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub back_alpha: f64,
}

impl Card {
    pub fn themed(p: &crate::theme::Palette) -> Card {
        Card {
            width: 292.0,
            back: p.back,
            ink: p.ink,
            dim: p.dim,
            back_alpha: 0.55,
        }
    }

    pub fn height(&self, rows: usize, spark: bool) -> f64 {
        14.0 + 24.0 + 22.0 * rows as f64 + if spark { 46.0 } else { 0.0 } + 10.0
    }
}

pub fn paint_card(card: &Card, font: &Font, bold: &Font, fields: &[(String, u8)], history: &[u8]) -> Image {
    let spark = history.len() > 1;
    let size = Vec2::new(card.width, card.height(fields.len(), spark));
    let mut canvas = Canvas::new(size.x as u32, size.y as u32);
    canvas.round_rect(Rect::new(Vec2::ZERO, size), 14.0, card.back, card.back_alpha);
    paint_system(&mut canvas, card, font, bold, fields, history);
    canvas.image
}

pub fn paint_system(canvas: &mut Canvas, card: &Card, font: &Font, bold: &Font, fields: &[(String, u8)], history: &[u8]) {
    let spark = history.len() > 1;
    let size = Vec2::new(card.width, card.height(fields.len(), spark));
    let busy = fields.iter().any(|(label, value)| label == "CPU" && *value > 70);
    let head = bold.paint_bold("System", card.ink);
    canvas.stamp(&head, Vec2::new(14.0, 14.0), 1.0);
    let note = font.paint(if busy { "Working hard" } else { "All calm" }, card.dim);
    canvas.stamp(&note, Vec2::new(size.x - 14.0 - crate::paint::logical(&note).x, 15.0).round(), 1.0);
    let pad = 14.0;
    let gap = 12.0;
    let number_room = font.width_of("100%").ceil();
    let widest = (size.x - pad * 2.0) * 0.38;
    let names: Vec<String> = fields.iter().map(|(label, _)| fit(font, &pretty(label), widest)).collect();
    let name_room = names.iter().map(|name| font.width_of(name)).fold(0.0, f64::max).ceil();
    let track_x = pad + name_room + gap;
    let track_w = (size.x - pad - number_room - gap - track_x).max(24.0);
    let mut y = 42.0;
    for ((_, value), name) in fields.iter().zip(&names) {
        let mid = y + 11.0;
        let label = font.paint(name, card.ink);
        canvas.stamp(&label, Vec2::new(pad, mid - crate::paint::logical(&label).y / 2.0).round(), 1.0);
        let number = font.paint_even_digits(&format!("{value}%"), card.dim);
        let nsize = crate::paint::logical(&number);
        canvas.stamp(&number, Vec2::new(size.x - pad - nsize.x, mid - nsize.y / 2.0).round(), 1.0);
        let track = Rect::new(Vec2::new(track_x, mid - 2.0), Vec2::new(track_w, 4.0));
        canvas.round_rect(track, 2.0, card.ink, 0.14);
        let v = share(*value);
        if v > 0.0 {
            canvas.round_rect(Rect::new(track.origin, Vec2::new((track.size.x * v).max(4.0), 4.0)), 2.0, tone(card.ink, v), 0.75);
        }
        y += 22.0;
    }
    if spark {
        let area = Rect::new(Vec2::new(14.0, y + 10.0), Vec2::new(size.x - 28.0, 32.0));
        let label = font.paint("CPU · 30 s", card.dim);
        canvas.stamp(&label, area.origin, 0.6);
        let point = |at: usize, v: u8| {
            Vec2::new(
                area.origin.x + area.size.x * at as f64 / (history.len() - 1) as f64,
                area.max().y - 2.0 - share(v) * (area.size.y - 6.0),
            )
        };
        let floor = area.max().y;
        let last = (history.len() - 1) as f64;
        let columns = area.size.x.floor() as usize;
        for column in 0..=columns {
            let x = area.origin.x + column as f64;
            let along = (column as f64 / area.size.x.max(1.0)) * last;
            let (low, high) = (along.floor() as usize, (along.ceil() as usize).min(history.len() - 1));
            let mix = along - low as f64;
            let value = f64::from(history[low]) * (1.0 - mix) + f64::from(history[high]) * mix;
            let top = area.max().y - 2.0 - value / 100.0 * (area.size.y - 6.0);
            canvas.round_rect(
                Rect::new(Vec2::new(x, top + 1.0), Vec2::new(1.0, (floor - top - 1.0).max(0.0))),
                0.0,
                heat(value / 100.0),
                0.12,
            );
        }
        for at in 1..history.len() {
            let color = heat(share(history[at - 1].max(history[at])));
            canvas.line(point(at - 1, history[at - 1]), point(at, history[at]), 1.4, color, 0.95);
        }
    }
}

pub fn rows_height(rows: usize, meter: bool) -> f64 {
    14.0 + 24.0 + if meter { 20.0 } else { 0.0 } + 22.0 * rows as f64 + 8.0
}

pub fn paint_rows(
    card: &Card,
    font: &Font,
    bold: &Font,
    head: &str,
    note: &str,
    meter: Option<(f64, [u8; 3])>,
    rows: &[(String, String)],
) -> Image {
    let size = Vec2::new(card.width, rows_height(rows.len(), meter.is_some()));
    let mut canvas = Canvas::new(size.x as u32, size.y as u32);
    canvas.round_rect(Rect::new(Vec2::ZERO, size), 14.0, card.back, card.back_alpha);
    let pad = 14.0;
    let title = bold.paint_bold(head, card.ink);
    canvas.stamp(&title, Vec2::new(pad, 14.0), 1.0);
    let room = size.x - pad * 3.0 - crate::paint::logical(&title).x;
    let quiet = font.paint(&fit(font, note, room.max(0.0)), card.dim);
    canvas.stamp(&quiet, Vec2::new(size.x - pad - crate::paint::logical(&quiet).x, 15.0).round(), 1.0);
    let mut y = 42.0;
    if let Some((share, color)) = meter {
        let track = Rect::new(Vec2::new(pad, y + 4.0), Vec2::new(size.x - pad * 2.0, 6.0));
        canvas.round_rect(track, 3.0, card.ink, 0.14);
        let filled = track.size.x * share.clamp(0.0, 1.0);
        if filled > 0.0 {
            canvas.round_rect(Rect::new(track.origin, Vec2::new(filled.max(6.0), track.size.y)), 3.0, color, 0.85);
        }
        y += 20.0;
    }
    let half = (size.x - pad * 2.0) / 2.0;
    for (label, value) in rows {
        let mid = y + 11.0;
        let name = font.paint(&fit(font, label, half - 6.0), card.dim);
        canvas.stamp(&name, Vec2::new(pad, mid - crate::paint::logical(&name).y / 2.0).round(), 1.0);
        let shown = font.paint_even_digits(&fit(font, value, half - 6.0), card.ink);
        let shown_size = crate::paint::logical(&shown);
        canvas.stamp(&shown, Vec2::new(size.x - pad - shown_size.x, mid - shown_size.y / 2.0).round(), 1.0);
        y += 22.0;
    }
    canvas.image
}

pub fn fit(font: &Font, text: &str, room: f64) -> String {
    if font.width_of(text) <= room {
        return text.to_string();
    }
    let mut kept: Vec<char> = text.chars().collect();
    while !kept.is_empty() {
        kept.pop();
        let tried: String = kept.iter().collect::<String>() + "…";
        if font.width_of(&tried) <= room {
            return tried;
        }
    }
    "…".to_string()
}

fn pretty(label: &str) -> String {
    match label {
        "RAM" => "Memory".to_string(),
        "CPU" => "CPU".to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lit(image: &Image) -> usize {
        image.rgba.chunks(4).filter(|px| px[3] > 40).count()
    }

    #[test]
    fn every_style_has_a_name_that_reads_back() {
        for style in Style::ALL {
            assert_eq!(Style::parse(style.name()), Ok(style));
        }
        assert!(Style::parse("sparkles").is_err());
    }

    #[test]
    fn a_fuller_glyph_lights_more_pixels() {
        let ink = [244, 245, 250];
        for style in [Style::Pills, Style::Rings, Style::Ticks] {
            let low = paint(style, &[10, 10, 10], ink);
            let high = paint(style, &[90, 90, 90], ink);
            assert!(lit(&high) > lit(&low), "{style:?}");
        }
    }

    #[test]
    fn the_calm_mark_stays_faint_until_load_is_high() {
        let ink = [244, 245, 250];
        let rest = paint(Style::Calm, &[12, 40, 55], ink);
        let busy = paint(Style::Calm, &[95, 40, 55], ink);
        let most = |image: &Image| image.rgba.chunks(4).map(|px| px[3]).max().unwrap_or(0);
        assert!(most(&rest) < 120, "a resting mark is see-through");
        assert!(lit(&busy) > lit(&rest));
    }

    #[test]
    fn load_warms_only_near_the_top() {
        let ink = [244, 245, 250];
        assert_eq!(tone(ink, 0.5), ink);
        assert_eq!(tone(ink, 0.88), WARM);
        assert_eq!(tone(ink, 1.0), HOT);
        assert!(!is_load("CHG") && !is_load("BAT") && is_load("CPU"));
    }

    #[test]
    fn the_sparkline_runs_cool_to_warm_to_hot() {
        assert_eq!(heat(0.1), COOL);
        assert_eq!(heat(0.88), WARM);
        assert_eq!(heat(1.0), HOT);
    }

    #[test]
    fn a_row_card_grows_with_its_rows_and_its_meter() {
        let font = Font::find(None, 11.5).unwrap();
        let card = Card::themed(&crate::theme::DARK);
        let rows = vec![("Time left".to_string(), "3:12".to_string())];
        let one = paint_rows(&card, &font, &font, "Battery 80%", "On battery", None, &rows);
        let metered = paint_rows(&card, &font, &font, "Battery 80%", "On battery", Some((0.8, card.ink)), &rows);
        assert!(crate::paint::logical(&metered).y > crate::paint::logical(&one).y);
        assert_eq!(crate::paint::logical(&one).y, rows_height(1, false));
    }

    #[test]
    fn a_long_drive_name_is_shortened_to_fit() {
        let font = Font::find(None, 11.5).unwrap();
        let short = fit(&font, "CPU", 80.0);
        assert_eq!(short, "CPU");
        let long = fit(&font, "NVME0N1P2-SAMSUNG-EVO", 80.0);
        assert!(long.ends_with('…'));
        assert!(font.width_of(&long) <= 80.0);
    }

    #[test]
    fn the_card_grows_with_its_rows_and_matches_at_density_two() {
        let font = Font::find(None, 12.5).unwrap();
        let card = Card::themed(&crate::theme::DARK);
        let fields = vec![("CPU".to_string(), 42), ("RAM".to_string(), 61), ("SDA3".to_string(), 55)];
        let history: Vec<u8> = (0..HISTORY as u8).collect();
        let image = paint_card(&card, &font, &font, &fields, &history);
        assert_eq!(crate::paint::logical(&image).y, card.height(3, true));
        crate::paint::same_at_two("glance card", || paint_card(&card, &font, &font, &fields, &history));
    }
}
