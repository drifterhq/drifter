use std::path::Path;

use crate::geom::{Rect, Vec2};
use crate::image::Image;
use crate::paint::Canvas;
use crate::text::Font;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Take {
    Still,
    Video,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    Screen,
    Window,
    Portion,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tool {
    pub take: Take,
    pub pick: Pick,
}

impl Tool {
    pub const BAR: [Tool; 5] = [
        Tool::new(Take::Still, Pick::Screen),
        Tool::new(Take::Still, Pick::Window),
        Tool::new(Take::Still, Pick::Portion),
        Tool::new(Take::Video, Pick::Screen),
        Tool::new(Take::Video, Pick::Portion),
    ];

    pub const fn new(take: Take, pick: Pick) -> Tool {
        Tool { take, pick }
    }

    pub fn label(self) -> &'static str {
        match (self.take, self.pick) {
            (Take::Still, Pick::Screen) => "Capture Entire Screen",
            (Take::Still, Pick::Window) => "Capture Selected Window",
            (Take::Still, Pick::Portion) => "Capture Selected Portion",
            (Take::Video, Pick::Screen) => "Record Entire Screen",
            (Take::Video, Pick::Window) => "Record Selected Window",
            (Take::Video, Pick::Portion) => "Record Selected Portion",
        }
    }

    pub fn spaced(self) -> Tool {
        let pick = match self.pick {
            Pick::Screen => Pick::Screen,
            Pick::Window => Pick::Portion,
            Pick::Portion => Pick::Window,
        };
        Tool { pick, ..self }
    }

    pub fn go_word(self) -> &'static str {
        match self.take {
            Take::Still => "Capture",
            Take::Video => "Record",
        }
    }

    fn lit(self, slot: Tool) -> bool {
        let video_window = Tool::new(Take::Video, Pick::Window);
        self == slot || (self == video_window && slot == Tool::new(Take::Video, Pick::Portion))
    }
}

pub fn drag_rect(from: Vec2, to: Vec2, screen: Vec2) -> Rect {
    let low = from.min(to).clamp(Vec2::ZERO, screen).round();
    let high = from.max(to).clamp(Vec2::ZERO, screen).round();
    Rect::new(low, high - low)
}

pub fn big_enough(rect: Rect) -> bool {
    rect.size.x >= 4.0 && rect.size.y >= 4.0
}

pub fn even(rect: Rect) -> Rect {
    let size = Vec2::new(
        (rect.size.x / 2.0).floor() * 2.0,
        (rect.size.y / 2.0).floor() * 2.0,
    );
    Rect::new(rect.origin.round(), size)
}

pub fn crop(image: &Image, rect: Rect) -> Image {
    let left = rect.origin.x.round().clamp(0.0, f64::from(image.width)) as u32;
    let top = rect.origin.y.round().clamp(0.0, f64::from(image.height)) as u32;
    let right = rect.max().x.round().clamp(f64::from(left), f64::from(image.width)) as u32;
    let bottom = rect.max().y.round().clamp(f64::from(top), f64::from(image.height)) as u32;
    let (width, height) = (right - left, bottom - top);
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in top..bottom {
        let from = ((y * image.width + left) * 4) as usize;
        rgba.extend_from_slice(&image.rgba[from..from + (width * 4) as usize]);
    }
    Image {
        width,
        height,
        rgba,
    }
}

pub fn file_name(take: Take, stamp: &str, extension: &str) -> String {
    match take {
        Take::Still => format!("Screenshot {stamp}.{extension}"),
        Take::Video => format!("Screen Recording {stamp}.mp4"),
    }
}

pub fn recorder_args(output: &str, area: Option<Rect>, audio: Option<&str>, path: &Path) -> Vec<String> {
    let mut args = vec!["-y".to_string()];
    match area {
        Some(rect) => {
            let rect = even(rect);
            args.push("-g".to_string());
            args.push(format!(
                "{},{} {}x{}",
                rect.origin.x, rect.origin.y, rect.size.x, rect.size.y
            ));
        }
        None => {
            args.push("-o".to_string());
            args.push(output.to_string());
        }
    }
    if let Some(device) = audio {
        args.push(format!("--audio={device}"));
    }
    args.push("-f".to_string());
    args.push(path.display().to_string());
    args
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    pub name: String,
    pub label: String,
}

pub fn sources(listing: &str) -> Vec<Source> {
    let mut found: Vec<(String, String)> = Vec::new();
    for line in listing.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix("Name:") {
            found.push((name.trim().to_string(), String::new()));
        } else if let Some(label) = line.strip_prefix("Description:")
            && let Some(last) = found.last_mut()
            && last.1.is_empty()
        {
            last.1 = label.trim().to_string();
        }
    }
    let named = |(name, label): (String, String)| {
        let label = match label.is_empty() {
            true => name.clone(),
            false => label,
        };
        match name.ends_with(".monitor") {
            true => Source {
                label: format!("Sound from {}", label.trim_start_matches("Monitor of ")),
                name,
            },
            false => Source { name, label },
        }
    };
    let (mut heard, played): (Vec<Source>, Vec<Source>) = found
        .into_iter()
        .map(named)
        .partition(|source| !source.name.ends_with(".monitor"));
    heard.extend(played);
    heard
}

pub fn clock(seconds: u64) -> String {
    let (hours, minutes, seconds) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    match hours {
        0 => format!("{minutes}:{seconds:02}"),
        _ => format!("{hours}:{minutes:02}:{seconds:02}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    Close,
    Tool(Tool),
    Options,
    Go,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    pub pad: f64,
    pub button: f64,
    pub gap: f64,
    pub split: f64,
    pub options: f64,
    pub go: f64,
    pub radius: f64,
    pub lift: f64,
    pub back: [u8; 3],
    pub back_alpha: f64,
    pub ink: [u8; 3],
    pub dim: [u8; 3],
    pub pick: [u8; 3],
    pub accent: [u8; 3],
}

impl Bar {
    pub fn themed(p: &crate::theme::Palette, accent: [u8; 3]) -> Bar {
        Bar {
            pad: 8.0,
            button: 44.0,
            gap: 4.0,
            split: 17.0,
            options: 92.0,
            go: 92.0,
            radius: 14.0,
            lift: 120.0,
            back: p.back,
            back_alpha: 0.96,
            ink: p.ink,
            dim: p.dim,
            pick: p.raised,
            accent,
        }
    }

    pub fn slots(&self) -> Vec<(Rect, Slot)> {
        let mut slots = Vec::new();
        let mut x = self.pad;
        let put = |slots: &mut Vec<(Rect, Slot)>, x: &mut f64, wide: f64, slot: Slot| {
            slots.push((
                Rect::new(Vec2::new(*x, self.pad), Vec2::new(wide, self.button)),
                slot,
            ));
            *x += wide + self.gap;
        };
        put(&mut slots, &mut x, self.button, Slot::Close);
        x += self.split - self.gap;
        for (at, tool) in Tool::BAR.into_iter().enumerate() {
            if at == 3 {
                x += self.split - self.gap;
            }
            put(&mut slots, &mut x, self.button, Slot::Tool(tool));
        }
        x += self.split - self.gap;
        put(&mut slots, &mut x, self.options, Slot::Options);
        put(&mut slots, &mut x, self.go, Slot::Go);
        slots
    }

    pub fn size(&self) -> Vec2 {
        let right = self
            .slots()
            .last()
            .map(|(rect, _)| rect.max().x)
            .unwrap_or(0.0);
        Vec2::new(right + self.pad, self.button + self.pad * 2.0)
    }

    pub fn rect(&self, screen: Vec2) -> Rect {
        let size = self.size();
        Rect::new(
            Vec2::new((screen.x - size.x) / 2.0, screen.y - size.y - self.lift).round(),
            size,
        )
    }

    pub fn hit(&self, inside: Vec2) -> Option<Slot> {
        self.slots()
            .into_iter()
            .find(|(rect, _)| rect.contains(inside))
            .map(|(_, slot)| slot)
    }

    pub fn paint(&self, chosen: Tool, hover: Option<Slot>, font: &Font) -> Image {
        let size = self.size();
        let mut canvas = Canvas::new(size.x as u32, size.y as u32);
        canvas.round_rect(Rect::new(Vec2::ZERO, size), self.radius, self.back, self.back_alpha);
        let mut splits = Vec::new();
        let mut last_kind = 0;
        for (rect, slot) in self.slots() {
            let kind = match slot {
                Slot::Close => 0,
                Slot::Tool(Tool { take: Take::Still, .. }) => 1,
                Slot::Tool(_) => 2,
                Slot::Options | Slot::Go => 3,
            };
            if kind != last_kind {
                splits.push(rect.origin.x - self.split / 2.0 + self.gap / 2.0);
                last_kind = kind;
            }
            let lit = matches!(slot, Slot::Tool(tool) if chosen.lit(tool));
            if lit {
                canvas.round_rect(rect, 9.0, self.pick, 1.0);
            } else if hover == Some(slot) {
                canvas.round_rect(rect, 9.0, self.pick, 0.5);
            }
            let center = rect.center();
            match slot {
                Slot::Close => close_mark(&mut canvas, center, self.dim),
                Slot::Tool(tool) => tool_mark(&mut canvas, tool, center, self.ink),
                Slot::Options => {
                    let word = font.paint("Options", self.ink);
                    let at = Vec2::new(rect.origin.x + 12.0, center.y - f64::from(word.height) / 2.0);
                    canvas.stamp(&word, at.round(), 1.0);
                    let tip = Vec2::new(rect.max().x - 16.0, center.y + 2.0);
                    canvas.triangle(
                        [tip + Vec2::new(-4.0, -4.0), tip + Vec2::new(4.0, -4.0), tip + Vec2::new(0.0, 1.0)],
                        self.ink,
                        1.0,
                    );
                }
                Slot::Go => {
                    let pill = Rect::new(rect.origin + Vec2::new(0.0, 6.0), rect.size - Vec2::new(0.0, 12.0));
                    canvas.round_rect(pill, 8.0, self.accent, 1.0);
                    let word = font.paint(chosen.go_word(), [255, 255, 255]);
                    let at = pill.center() - word.size() / 2.0;
                    canvas.stamp(&word, at.round(), 1.0);
                }
            }
        }
        for x in splits.into_iter().skip(1) {
            let line = Rect::new(Vec2::new(x.round(), self.pad + 8.0), Vec2::new(1.0, self.button - 16.0));
            canvas.round_rect(line, 0.0, self.dim, 0.5);
        }
        canvas.image
    }
}

fn outline(canvas: &mut Canvas, rect: Rect, thick: f64, ink: [u8; 3]) {
    let (o, s) = (rect.origin, rect.size);
    for side in [
        Rect::new(o, Vec2::new(s.x, thick)),
        Rect::new(Vec2::new(o.x, o.y + s.y - thick), Vec2::new(s.x, thick)),
        Rect::new(o, Vec2::new(thick, s.y)),
        Rect::new(Vec2::new(o.x + s.x - thick, o.y), Vec2::new(thick, s.y)),
    ] {
        canvas.round_rect(side, 0.0, ink, 1.0);
    }
}

fn dashed(canvas: &mut Canvas, rect: Rect, ink: [u8; 3]) {
    let (o, m) = (rect.origin, rect.max());
    let mut x = o.x;
    while x < m.x {
        let wide = (m.x - x).min(3.0);
        canvas.round_rect(Rect::new(Vec2::new(x, o.y), Vec2::new(wide, 1.5)), 0.0, ink, 1.0);
        canvas.round_rect(Rect::new(Vec2::new(x, m.y - 1.5), Vec2::new(wide, 1.5)), 0.0, ink, 1.0);
        x += 5.0;
    }
    let mut y = o.y;
    while y < m.y {
        let tall = (m.y - y).min(3.0);
        canvas.round_rect(Rect::new(Vec2::new(o.x, y), Vec2::new(1.5, tall)), 0.0, ink, 1.0);
        canvas.round_rect(Rect::new(Vec2::new(m.x - 1.5, y), Vec2::new(1.5, tall)), 0.0, ink, 1.0);
        y += 5.0;
    }
}

fn tool_mark(canvas: &mut Canvas, tool: Tool, center: Vec2, ink: [u8; 3]) {
    let frame = Rect::from_center(center, Vec2::new(24.0, 17.0)).round_out();
    match tool.pick {
        Pick::Screen => outline(canvas, frame, 1.5, ink),
        Pick::Window => {
            let inner = Rect::new(frame.origin + Vec2::new(3.0, 3.0), frame.size - Vec2::new(6.0, 6.0));
            outline(canvas, frame, 1.5, ink);
            canvas.round_rect(inner, 1.5, ink, 0.85);
        }
        Pick::Portion => dashed(canvas, frame, ink),
    }
    if tool.take == Take::Video {
        let dot = frame.max() - Vec2::new(1.0, 1.0);
        canvas.circle(dot, 6.5, [0, 0, 0], 0.35);
        canvas.circle(dot, 5.0, ink, 1.0);
        canvas.circle(dot, 2.2, [0, 0, 0], 0.6);
    }
}

fn close_mark(canvas: &mut Canvas, center: Vec2, ink: [u8; 3]) {
    canvas.circle(center, 11.0, ink, 0.35);
    let arm = 4.5;
    for (a, b) in [(Vec2::new(-arm, -arm), Vec2::new(arm, arm)), (Vec2::new(-arm, arm), Vec2::new(arm, -arm))] {
        let along = b - a;
        let steps = 24;
        for step in 0..=steps {
            let t = f64::from(step) / f64::from(steps);
            canvas.circle(center + a + along * t, 0.9, [255, 255, 255], 1.0);
        }
    }
}

pub fn paint_cross(side: u32) -> Image {
    let size = f64::from(side.max(9) | 1);
    let mut canvas = Canvas::new(size as u32, size as u32);
    let mid = (size / 2.0).floor();
    for (rect, ink) in [
        (Rect::new(Vec2::new(mid - 1.0, 0.0), Vec2::new(3.0, size)), [0, 0, 0]),
        (Rect::new(Vec2::new(0.0, mid - 1.0), Vec2::new(size, 3.0)), [0, 0, 0]),
    ] {
        canvas.round_rect(rect, 0.0, ink, 0.55);
    }
    for rect in [
        Rect::new(Vec2::new(mid, 0.0), Vec2::new(1.0, size)),
        Rect::new(Vec2::new(0.0, mid), Vec2::new(size, 1.0)),
    ] {
        canvas.round_rect(rect, 0.0, [255, 255, 255], 1.0);
    }
    canvas.image
}

pub fn paint_tag(text: &str, font: &Font) -> Image {
    let word = font.paint_even_digits(text, [255, 255, 255]);
    let size = word.size() + Vec2::new(12.0, 6.0);
    let mut canvas = Canvas::new(size.x.ceil() as u32, size.y.ceil() as u32);
    canvas.round_rect(Rect::new(Vec2::ZERO, size), 5.0, [0, 0, 0], 0.6);
    canvas.stamp(&word, Vec2::new(6.0, 3.0), 1.0);
    canvas.image
}

pub fn size_tag(rect: Rect) -> String {
    format!("{} × {}", rect.size.x.round(), rect.size.y.round())
}

pub fn paint_stop(seconds: u64, font: &Font) -> Image {
    let time = font.paint_even_digits(&clock(seconds), [255, 255, 255]);
    let height = f64::from(time.height) + 14.0;
    let width = 18.0 + 10.0 + f64::from(time.width) + 14.0 + 16.0 + 12.0;
    let mut canvas = Canvas::new(width.ceil() as u32, height.ceil() as u32);
    canvas.round_rect(Rect::new(Vec2::ZERO, Vec2::new(width, height)), height / 2.0, [24, 24, 28], 0.92);
    let mid = height / 2.0;
    canvas.circle(Vec2::new(18.0, mid), 5.0, [255, 69, 58], 1.0);
    canvas.stamp(&time, Vec2::new(30.0, mid - f64::from(time.height) / 2.0).round(), 1.0);
    let stop = Rect::from_center(Vec2::new(width - 20.0, mid), Vec2::splat(11.0));
    canvas.round_rect(stop, 2.0, [255, 255, 255], 1.0);
    canvas.image
}

pub fn thumb_size(shot: Vec2, widest: f64) -> Vec2 {
    let scale = (widest / shot.x.max(1.0)).min(widest / shot.y.max(1.0)).min(1.0);
    (shot * scale).round().max(Vec2::splat(1.0))
}

pub fn paint_thumb(shot: &Image, widest: f64) -> Image {
    let inner = thumb_size(shot.size(), widest);
    let small = shot.scaled(inner.x as u32, inner.y as u32);
    let (rim, blur) = (3.0, 10.0);
    let card = Rect::new(Vec2::splat(blur), inner + Vec2::splat(rim * 2.0));
    let size = card.size + Vec2::splat(blur * 2.0);
    let mut canvas = Canvas::new(size.x as u32, size.y as u32);
    canvas.shadow(card, 6.0, blur, 0.45);
    canvas.round_rect(card, 6.0, [255, 255, 255], 1.0);
    canvas.stamp(&small, card.origin + Vec2::splat(rim), 1.0);
    canvas.image
}

pub fn thumb_rect(art: Vec2, screen: Vec2) -> Rect {
    Rect::new(screen - art - Vec2::new(14.0, 14.0), art)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drag_makes_the_same_box_from_any_corner() {
        let screen = Vec2::new(1920.0, 1080.0);
        let one = drag_rect(Vec2::new(10.0, 20.0), Vec2::new(110.0, 220.0), screen);
        let two = drag_rect(Vec2::new(110.0, 220.0), Vec2::new(10.0, 20.0), screen);
        assert_eq!(one, two);
        assert_eq!(one, Rect::new(Vec2::new(10.0, 20.0), Vec2::new(100.0, 200.0)));
        let past = drag_rect(Vec2::new(1900.0, 1000.0), Vec2::new(2500.0, 1500.0), screen);
        assert_eq!(past.max(), screen);
        assert!(!big_enough(drag_rect(Vec2::ZERO, Vec2::new(2.0, 50.0), screen)));
    }

    #[test]
    fn space_swaps_window_and_portion_only() {
        let portion = Tool::new(Take::Still, Pick::Portion);
        assert_eq!(portion.spaced().pick, Pick::Window);
        assert_eq!(portion.spaced().spaced(), portion);
        let video = Tool::new(Take::Video, Pick::Portion);
        assert_eq!(video.spaced(), Tool::new(Take::Video, Pick::Window));
        let screen = Tool::new(Take::Video, Pick::Screen);
        assert_eq!(screen.spaced(), screen);
    }

    #[test]
    fn a_crop_keeps_the_right_pixels() {
        let mut rgba = Vec::new();
        for y in 0..4u8 {
            for x in 0..4u8 {
                rgba.extend_from_slice(&[x, y, 0, 255]);
            }
        }
        let image = Image {
            width: 4,
            height: 4,
            rgba,
        };
        let part = crop(&image, Rect::new(Vec2::new(1.0, 2.0), Vec2::new(2.0, 2.0)));
        assert_eq!((part.width, part.height), (2, 2));
        assert_eq!(&part.rgba[0..4], &[1, 2, 0, 255]);
        assert_eq!(&part.rgba[12..16], &[2, 3, 0, 255]);
        let past = crop(&image, Rect::new(Vec2::new(3.0, 3.0), Vec2::new(9.0, 9.0)));
        assert_eq!((past.width, past.height), (1, 1));
    }

    #[test]
    fn the_recorder_gets_an_even_box_or_the_screen() {
        let path = Path::new("/home/dh/Movies/a.mp4");
        let boxed = recorder_args(
            "DP-1",
            Some(Rect::new(Vec2::new(10.0, 20.0), Vec2::new(301.0, 199.0))),
            Some("alsa_input.mic"),
            path,
        );
        assert_eq!(
            boxed,
            ["-y", "-g", "10,20 300x198", "--audio=alsa_input.mic", "-f", "/home/dh/Movies/a.mp4"]
        );
        let whole = recorder_args("DP-1", None, None, path);
        assert_eq!(whole, ["-y", "-o", "DP-1", "-f", "/home/dh/Movies/a.mp4"]);
    }

    #[test]
    fn sound_sources_list_microphones_first() {
        let listing = "Source #114\n\tState: SUSPENDED\n\tName: alsa_output.pci.analog-stereo.monitor\n\tDescription: Monitor of Built-in Audio Analog Stereo\n\nSource #115\n\tName: alsa_input.pci.analog-stereo\n\tDescription: Built-in Audio Analog Stereo\n";
        let found = sources(listing);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].name, "alsa_input.pci.analog-stereo");
        assert_eq!(found[0].label, "Built-in Audio Analog Stereo");
        assert_eq!(found[1].label, "Sound from Built-in Audio Analog Stereo");
    }

    #[test]
    fn names_and_clocks_read_well() {
        assert_eq!(file_name(Take::Still, "2026-10-06 at 00.40.00", "png"), "Screenshot 2026-10-06 at 00.40.00.png");
        assert_eq!(file_name(Take::Video, "2026-10-06 at 00.40.00", "png"), "Screen Recording 2026-10-06 at 00.40.00.mp4");
        assert_eq!(clock(7), "0:07");
        assert_eq!(clock(3725), "1:02:05");
        assert_eq!(size_tag(Rect::new(Vec2::ZERO, Vec2::new(640.0, 480.0))), "640 × 480");
    }

    #[test]
    fn the_bar_finds_each_button() {
        let bar = Bar::themed(&crate::theme::DARK, [10, 132, 255]);
        let slots = bar.slots();
        assert_eq!(slots.len(), 8);
        for (rect, slot) in &slots {
            assert_eq!(bar.hit(rect.center()), Some(*slot));
        }
        assert_eq!(bar.hit(Vec2::new(1.0, 1.0)), None);
        let rect = bar.rect(Vec2::new(1920.0, 1080.0));
        assert!((rect.center().x - 960.0).abs() <= 1.0);
    }

    #[test]
    fn a_thumbnail_fits_and_keeps_its_shape() {
        assert_eq!(thumb_size(Vec2::new(1920.0, 1080.0), 200.0), Vec2::new(200.0, 113.0));
        assert_eq!(thumb_size(Vec2::new(100.0, 400.0), 200.0), Vec2::new(50.0, 200.0));
        assert_eq!(thumb_size(Vec2::new(50.0, 40.0), 200.0), Vec2::new(50.0, 40.0));
    }
}
