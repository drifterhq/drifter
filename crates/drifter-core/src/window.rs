use crate::config::{ButtonName, Side, WindowConfig};
use crate::geom::{Rect, Vec2};
use crate::image::Image;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chrome {
    pub title: f64,
    pub edge: f64,
    pub radius: f64,
    pub button: f64,
    pub gap: f64,
    pub margin: f64,
    pub grip: f64,
    pub frame: [u8; 3],
    pub frame_alpha: u8,
    pub idle: [u8; 3],
    pub idle_alpha: u8,
    pub side: Side,
    pub order: [Button; 3],
    pub show_title: bool,
}

impl Chrome {
    pub fn themed(p: &crate::theme::Palette) -> Chrome {
        Chrome {
            title: 30.0,
            edge: 4.0,
            radius: 10.0,
            button: 12.0,
            gap: 8.0,
            margin: 11.0,
            grip: 7.0,
            frame: p.back,
            frame_alpha: 242,
            idle: p.idle,
            idle_alpha: 230,
            side: Side::Left,
            order: [Button::Close, Button::Minimize, Button::Maximize],
            show_title: true,
        }
    }
}

impl Default for Chrome {
    fn default() -> Self {
        Chrome::themed(&crate::theme::DARK)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece {
    TitleLeft,
    TitleMid,
    TitleRight,
    EdgeLeft,
    EdgeRight,
    EdgeBottom,
    CornerBottomLeft,
    CornerBottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Close,
    Minimize,
    Maximize,
    Restore,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Rest,
    Hover,
    Press,
}

impl Piece {
    pub const ALL: [Piece; 8] = [
        Piece::TitleLeft,
        Piece::TitleMid,
        Piece::TitleRight,
        Piece::EdgeLeft,
        Piece::EdgeRight,
        Piece::EdgeBottom,
        Piece::CornerBottomLeft,
        Piece::CornerBottomRight,
    ];

    pub fn file_name(self, active: bool) -> String {
        let name = match self {
            Piece::TitleLeft => "title-left",
            Piece::TitleMid => "title-mid",
            Piece::TitleRight => "title-right",
            Piece::EdgeLeft => "edge-left",
            Piece::EdgeRight => "edge-right",
            Piece::EdgeBottom => "edge-bottom",
            Piece::CornerBottomLeft => "corner-bottom-left",
            Piece::CornerBottomRight => "corner-bottom-right",
        };
        match active {
            true => format!("{name}.png"),
            false => format!("{name}-idle.png"),
        }
    }

    pub fn size(self, chrome: &Chrome) -> Vec2 {
        let tile = 8.0;
        match self {
            Piece::TitleLeft | Piece::TitleRight => Vec2::new(chrome.radius, chrome.title),
            Piece::TitleMid => Vec2::new(tile, chrome.title),
            Piece::EdgeLeft | Piece::EdgeRight => Vec2::new(chrome.edge, tile),
            Piece::EdgeBottom => Vec2::new(tile, chrome.edge),
            Piece::CornerBottomLeft | Piece::CornerBottomRight => {
                Vec2::new(chrome.radius, chrome.radius)
            }
        }
    }

    fn corners(self, radius: f64) -> [f64; 4] {
        match self {
            Piece::TitleLeft => [radius, 0.0, 0.0, 0.0],
            Piece::TitleRight => [0.0, radius, 0.0, 0.0],
            Piece::CornerBottomLeft => [0.0, 0.0, 0.0, radius],
            Piece::CornerBottomRight => [0.0, 0.0, radius, 0.0],
            _ => [0.0; 4],
        }
    }
}

impl Button {
    pub const ALL: [Button; 4] = [
        Button::Close,
        Button::Minimize,
        Button::Maximize,
        Button::Restore,
    ];

    pub fn file_name(self, look: Look) -> String {
        let name = match self {
            Button::Close => "close",
            Button::Minimize => "minimize",
            Button::Maximize => "maximize",
            Button::Restore => "restore",
        };
        match look {
            Look::Rest => format!("button-{name}.png"),
            Look::Hover => format!("button-{name}-hover.png"),
            Look::Press => format!("button-{name}-press.png"),
        }
    }

    fn color(self) -> [u8; 3] {
        match self {
            Button::Close => [255, 95, 87],
            Button::Minimize => [254, 188, 46],
            Button::Maximize | Button::Restore => [40, 200, 64],
        }
    }
}

impl Look {
    pub const ALL: [Look; 3] = [Look::Rest, Look::Hover, Look::Press];

    fn shade(self) -> f64 {
        match self {
            Look::Rest => 1.0,
            Look::Hover => 1.0,
            Look::Press => 0.78,
        }
    }
}

impl Chrome {
    pub fn follow(&mut self, config: &WindowConfig) {
        self.side = config.buttons;
        self.show_title = config.title;
        for (slot, name) in config.order.iter().take(3).enumerate() {
            self.order[slot] = match name {
                ButtonName::Close => Button::Close,
                ButtonName::Minimize => Button::Minimize,
                ButtonName::Maximize => Button::Maximize,
            };
        }
    }

    pub fn button_rect(&self, slot: usize, width: f64) -> Rect {
        let step = self.button + self.gap;
        let start = match self.side {
            Side::Left => self.margin,
            Side::Right => width - self.buttons_width(3) + self.margin,
        };
        let x = start + step * slot as f64;
        let y = (self.title - self.button) / 2.0;
        Rect::new(Vec2::new(x, y), Vec2::splat(self.button))
    }

    pub fn buttons_width(&self, count: usize) -> f64 {
        if count == 0 {
            return self.margin;
        }
        self.margin + (self.button + self.gap) * count as f64 - self.gap + self.margin
    }
}

fn rounded(size: Vec2, corners: [f64; 4], color: [u8; 3], alpha: u8, shade: f64) -> Image {
    let width = size.x.round().max(1.0) as u32;
    let height = size.y.round().max(1.0) as u32;
    let mut rgba = vec![0u8; (width as usize) * (height as usize) * 4];
    let base = f64::from(alpha) / 255.0;

    for y in 0..height {
        let py = f64::from(y) + 0.5;
        for x in 0..width {
            let px = f64::from(x) + 0.5;
            let coverage = (inside(px, py, size, corners) + 0.5).clamp(0.0, 1.0);
            if coverage <= 0.0 {
                continue;
            }

            let lit = shade;
            let pixel_alpha = base * coverage;
            let index = ((y as usize) * (width as usize) + x as usize) * 4;
            for (channel, value) in color.iter().enumerate() {
                rgba[index + channel] = (f64::from(*value) * lit * pixel_alpha)
                    .round()
                    .clamp(0.0, 255.0) as u8;
            }
            rgba[index + 3] = (pixel_alpha * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }

    Image {
        width,
        height,
        rgba,
    }
}

fn inside(px: f64, py: f64, size: Vec2, corners: [f64; 4]) -> f64 {
    let [top_left, top_right, bottom_right, bottom_left] = corners;
    let corner = if top_left > 0.0 && px < top_left && py < top_left {
        Some((top_left, Vec2::new(top_left, top_left)))
    } else if top_right > 0.0 && px > size.x - top_right && py < top_right {
        Some((top_right, Vec2::new(size.x - top_right, top_right)))
    } else if bottom_right > 0.0 && px > size.x - bottom_right && py > size.y - bottom_right {
        Some((
            bottom_right,
            Vec2::new(size.x - bottom_right, size.y - bottom_right),
        ))
    } else if bottom_left > 0.0 && px < bottom_left && py > size.y - bottom_left {
        Some((bottom_left, Vec2::new(bottom_left, size.y - bottom_left)))
    } else {
        None
    };

    if let Some((radius, center)) = corner {
        let dx = px - center.x;
        let dy = py - center.y;
        return radius - (dx * dx + dy * dy).sqrt();
    }

    px.min(size.x - px).min(py).min(size.y - py)
}

pub fn paint(piece: Piece, chrome: &Chrome, active: bool) -> Image {
    let (color, alpha) = match active {
        true => (chrome.frame, chrome.frame_alpha),
        false => (chrome.idle, chrome.idle_alpha),
    };
    rounded(
        piece.size(chrome),
        piece.corners(chrome.radius),
        color,
        alpha,
        1.0,
    )
}

pub fn painted(name: &str, chrome: &Chrome) -> Option<Image> {
    for piece in Piece::ALL {
        for active in [true, false] {
            if piece.file_name(active) == name {
                return Some(paint(piece, chrome, active));
            }
        }
    }
    for button in Button::ALL {
        for look in Look::ALL {
            if button.file_name(look) == name {
                return Some(paint_button(button, look, chrome));
            }
        }
    }
    None
}

pub fn paint_button(button: Button, look: Look, chrome: &Chrome) -> Image {
    let size = Vec2::splat(chrome.button);
    let mut image = rounded(
        size,
        [chrome.button / 2.0; 4],
        button.color(),
        255,
        look.shade(),
    );

    if look != Look::Rest {
        glyph(&mut image, button);
    }
    image
}

fn glyph(image: &mut Image, button: Button) {
    let width = f64::from(image.width);
    let height = f64::from(image.height);
    let ink = [28u8, 24, 20];
    let thickness = (width / 10.0).max(1.0);
    let reach = width * 0.28;
    let middle = Vec2::new(width / 2.0, height / 2.0);

    for y in 0..image.height {
        let py = f64::from(y) + 0.5;
        for x in 0..image.width {
            let px = f64::from(x) + 0.5;
            let dx = px - middle.x;
            let dy = py - middle.y;
            let on = match button {
                Button::Close => {
                    (dx - dy).abs() <= thickness && dx.abs() <= reach && dy.abs() <= reach
                        || (dx + dy).abs() <= thickness && dx.abs() <= reach && dy.abs() <= reach
                }
                Button::Minimize => dy.abs() <= thickness / 2.0 && dx.abs() <= reach,
                Button::Maximize => {
                    (dx.abs() <= reach && dy.abs() <= reach)
                        && (dx.abs() >= reach - thickness || dy.abs() >= reach - thickness)
                }
                Button::Restore => {
                    (dx + 1.0).abs() <= reach
                        && (dy - 1.0).abs() <= reach
                        && ((dx + 1.0).abs() >= reach - thickness
                            || (dy - 1.0).abs() >= reach - thickness)
                }
            };
            if !on {
                continue;
            }
            let index = ((y as usize) * (image.width as usize) + x as usize) * 4;
            if image.rgba[index + 3] == 0 {
                continue;
            }
            let a = f64::from(image.rgba[index + 3]) / 255.0;
            for (channel, value) in ink.iter().enumerate() {
                image.rgba[index + channel] = (f64::from(*value) * a).round() as u8;
            }
        }
    }
}

pub fn cascade(wanted: Vec2, taken: &[Vec2], step: f64, room: Rect) -> Vec2 {
    let mut place = wanted;
    for _ in 0..12 {
        let clash = taken
            .iter()
            .any(|other| (*other - place).length() < step / 2.0);
        if !clash {
            break;
        }
        place = place + Vec2::splat(step);
        if place.x > room.max().x - step || place.y > room.max().y - step {
            place = room.origin + Vec2::splat(step);
        }
    }
    place.round()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grip {
    Button(usize),
    Title,
    Left,
    Right,
    Top,
    TopLeft,
    TopRight,
    Bottom,
    BottomLeft,
    BottomRight,
}

impl Grip {
    pub fn moves_left(self) -> bool {
        matches!(self, Grip::Left | Grip::TopLeft | Grip::BottomLeft)
    }

    pub fn moves_up(self) -> bool {
        matches!(self, Grip::Top | Grip::TopLeft | Grip::TopRight)
    }

    pub fn changes_width(self) -> bool {
        !matches!(
            self,
            Grip::Title | Grip::Top | Grip::Bottom | Grip::Button(_)
        )
    }

    pub fn changes_height(self) -> bool {
        matches!(
            self,
            Grip::Top
                | Grip::TopLeft
                | Grip::TopRight
                | Grip::Bottom
                | Grip::BottomLeft
                | Grip::BottomRight
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Layout {
    pub frame: Rect,
    pub content: Rect,
    pub title_left: Rect,
    pub title_mid: Rect,
    pub title_right: Rect,
    pub edge_left: Rect,
    pub edge_right: Rect,
    pub edge_bottom: Rect,
    pub corner_left: Rect,
    pub corner_right: Rect,
}

impl Chrome {
    pub fn layout(&self, content: Rect) -> Layout {
        let frame = Rect::new(
            Vec2::new(content.origin.x - self.edge, content.origin.y - self.title),
            Vec2::new(
                content.size.x + self.edge * 2.0,
                content.size.y + self.title + self.edge,
            ),
        );
        let right = frame.max().x;
        let bottom = frame.max().y;

        Layout {
            frame,
            content,
            title_left: Rect::new(frame.origin, Vec2::new(self.radius, self.title)),
            title_mid: Rect::new(
                Vec2::new(frame.origin.x + self.radius, frame.origin.y),
                Vec2::new((frame.size.x - self.radius * 2.0).max(0.0), self.title),
            ),
            title_right: Rect::new(
                Vec2::new(right - self.radius, frame.origin.y),
                Vec2::new(self.radius, self.title),
            ),
            edge_left: Rect::new(
                Vec2::new(frame.origin.x, content.origin.y),
                Vec2::new(self.edge, content.size.y),
            ),
            edge_right: Rect::new(
                Vec2::new(content.max().x, content.origin.y),
                Vec2::new(self.edge, content.size.y),
            ),
            edge_bottom: Rect::new(
                Vec2::new(frame.origin.x + self.radius, bottom - self.edge),
                Vec2::new((frame.size.x - self.radius * 2.0).max(0.0), self.edge),
            ),
            corner_left: Rect::new(
                Vec2::new(frame.origin.x, bottom - self.radius),
                Vec2::splat(self.radius),
            ),
            corner_right: Rect::new(
                Vec2::new(right - self.radius, bottom - self.radius),
                Vec2::splat(self.radius),
            ),
        }
    }

    pub fn title_bar(&self, content: Rect) -> Rect {
        let frame = self.layout(content).frame;
        Rect::new(frame.origin, Vec2::new(frame.size.x, self.title))
    }

    pub fn grip_at(&self, content: Rect, pointer: Vec2, buttons: usize) -> Option<Grip> {
        if let Some(slot) = self.button_at(content, pointer, buttons) {
            return Some(Grip::Button(slot));
        }

        let frame = self.layout(content).frame;
        let reach = self.grip;
        let outside = Rect::new(
            frame.origin - Vec2::splat(reach),
            frame.size + Vec2::splat(reach * 2.0),
        );
        if !outside.contains(pointer) {
            return None;
        }

        let near_left = pointer.x < frame.origin.x + reach;
        let near_right = pointer.x > frame.max().x - reach;
        let near_top = pointer.y < frame.origin.y + reach;
        let near_bottom = pointer.y > frame.max().y - reach;

        if near_top && near_left {
            return Some(Grip::TopLeft);
        }
        if near_top && near_right {
            return Some(Grip::TopRight);
        }
        if near_top {
            return Some(Grip::Top);
        }

        if near_bottom && near_left {
            return Some(Grip::BottomLeft);
        }
        if near_bottom && near_right {
            return Some(Grip::BottomRight);
        }
        if near_bottom {
            return Some(Grip::Bottom);
        }
        if near_left {
            return Some(Grip::Left);
        }
        if near_right {
            return Some(Grip::Right);
        }
        if self.title_bar(content).contains(pointer) {
            return Some(Grip::Title);
        }
        None
    }

    pub fn resize(&self, start: Rect, grip: Grip, moved: Vec2, least: Vec2) -> Rect {
        let mut rect = start;
        if grip.changes_width() {
            if grip.moves_left() {
                let width = (start.size.x - moved.x).max(least.x);
                rect.origin.x = start.max().x - width;
                rect.size.x = width;
            } else {
                rect.size.x = (start.size.x + moved.x).max(least.x);
            }
        }
        if grip.changes_height() {
            if grip.moves_up() {
                let height = (start.size.y - moved.y).max(least.y);
                rect.origin.y = start.max().y - height;
                rect.size.y = height;
            } else {
                rect.size.y = (start.size.y + moved.y).max(least.y);
            }
        }
        rect
    }

    pub fn button_at(&self, content: Rect, pointer: Vec2, count: usize) -> Option<usize> {
        let bar = self.title_bar(content);
        (0..count).find(|slot| {
            let rect = self.button_rect(*slot, bar.size.x);
            Rect::new(bar.origin + rect.origin, rect.size).contains(pointer)
        })
    }
}

pub const SHADOW_BLUR: f64 = 44.0;

pub fn shadow_patch(radius: f64, blur: f64) -> Image {
    let corner = (blur + radius).ceil();
    let side = (corner * 2.0 + 1.0) as u32;
    let sigma = blur / 2.6;
    let floor = (-(blur * blur) / (2.0 * sigma * sigma)).exp();
    let mut rgba = vec![0u8; (side * side * 4) as usize];
    let inner_lo = blur;
    let inner_hi = f64::from(side) - blur;
    for y in 0..side {
        for x in 0..side {
            let p = Vec2::new(f64::from(x) + 0.5, f64::from(y) + 0.5);
            let cx = p.x.clamp(inner_lo + radius, inner_hi - radius);
            let cy = p.y.clamp(inner_lo + radius, inner_hi - radius);
            let away = ((p - Vec2::new(cx, cy)).length() - radius).max(0.0);
            let alpha = (((-(away * away) / (2.0 * sigma * sigma)).exp() - floor) / (1.0 - floor)).max(0.0);
            rgba[((y * side + x) * 4 + 3) as usize] = (alpha * 255.0).round() as u8;
        }
    }
    Image {
        width: side,
        height: side,
        rgba,
    }
}

pub fn shadow_slices(frame: Rect, corner: f64, zoom: f64, drop: f64) -> Vec<(Rect, Rect)> {
    let c = corner * zoom;
    let outer = Rect::new(
        frame.origin - Vec2::splat(c - (corner - SHADOW_BLUR).max(0.0) * zoom) + Vec2::new(0.0, drop * zoom),
        frame.size + Vec2::splat(2.0 * (c - (corner - SHADOW_BLUR).max(0.0) * zoom)),
    );
    let middle = Vec2::new((outer.size.x - 2.0 * c).max(0.0), (outer.size.y - 2.0 * c).max(0.0));
    let cols = [(0.0, corner, outer.origin.x, c), (corner, 1.0, outer.origin.x + c, middle.x), (corner + 1.0, corner, outer.origin.x + c + middle.x, c)];
    let rows = [(0.0, corner, outer.origin.y, c), (corner, 1.0, outer.origin.y + c, middle.y), (corner + 1.0, corner, outer.origin.y + c + middle.y, c)];
    let mut out = Vec::new();
    for (row, (sy, sh, dy, dh)) in rows.iter().enumerate() {
        for (col, (sx, sw, dx, dw)) in cols.iter().enumerate() {
            if row == 1 && col == 1 {
                continue;
            }
            if *dw <= 0.0 || *dh <= 0.0 {
                continue;
            }
            out.push((
                Rect::new(Vec2::new(*sx, *sy), Vec2::new(*sw, *sh)),
                Rect::new(Vec2::new(*dx, *dy), Vec2::new(*dw, *dh)),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {

    #[test]
    fn every_skin_file_has_a_painted_stand_in() {
        let chrome = Chrome::default();
        for piece in Piece::ALL {
            for active in [true, false] {
                assert!(painted(&piece.file_name(active), &chrome).is_some());
            }
        }
        for button in Button::ALL {
            for look in Look::ALL {
                assert!(painted(&button.file_name(look), &chrome).is_some());
            }
        }
        assert!(painted("nothing.png", &chrome).is_none());
    }

    use super::*;

    #[test]
    fn the_window_shadow_fades_out_and_has_no_gap() {
        let patch = shadow_patch(10.0, SHADOW_BLUR);
        let side = patch.width;
        let alpha = |x: u32, y: u32| patch.rgba[((y * side + x) * 4 + 3) as usize];
        let middle = side / 2;
        assert_eq!(alpha(middle, middle), 255, "solid under the window, so a drop leaves no gap");
        assert!(alpha(middle, SHADOW_BLUR as u32 - 1) > 200, "dark at the window edge");
        assert!(alpha(middle, 0) < 20, "faded out at the far edge");
        let frame = Rect::new(Vec2::new(100.0, 100.0), Vec2::new(400.0, 300.0));
        let corner = f64::from(side - 1) / 2.0;
        let slices = shadow_slices(frame, corner, 1.0, 0.0);
        assert_eq!(slices.len(), 8, "eight pieces, no middle");
        let left = slices.iter().map(|(_, d)| d.origin.x).fold(f64::MAX, f64::min);
        assert_eq!(left, frame.origin.x - SHADOW_BLUR);
        let half = shadow_slices(frame, corner, 0.5, 0.0);
        let narrow = half.iter().map(|(_, d)| d.origin.x).fold(f64::MAX, f64::min);
        assert_eq!(narrow, frame.origin.x - SHADOW_BLUR * 0.5, "the shadow shrinks with the zoom");
    }

    fn chrome() -> Chrome {
        Chrome::default()
    }

    #[test]
    fn the_top_edge_and_its_corners_are_grips() {
        let chrome = chrome();
        let content = Rect::new(Vec2::new(200.0, 200.0), Vec2::new(400.0, 300.0));
        let frame = chrome.layout(content).frame;
        let above = Vec2::new(frame.center().x, frame.origin.y - 3.0);
        assert_eq!(chrome.grip_at(content, above, 3), Some(Grip::Top));
        let left = frame.origin - Vec2::splat(3.0);
        assert_eq!(chrome.grip_at(content, left, 3), Some(Grip::TopLeft));
        let right = Vec2::new(frame.max().x + 3.0, frame.origin.y - 3.0);
        assert_eq!(chrome.grip_at(content, right, 3), Some(Grip::TopRight));
    }

    #[test]
    fn dragging_the_top_edge_keeps_the_bottom_still() {
        let chrome = chrome();
        let start = Rect::new(Vec2::new(200.0, 200.0), Vec2::new(400.0, 300.0));
        let least = Vec2::new(160.0, 80.0);
        let taller = chrome.resize(start, Grip::Top, Vec2::new(5.0, -50.0), least);
        assert_eq!(taller.size, Vec2::new(400.0, 350.0));
        assert_eq!(taller.max().y, start.max().y);
        let corner = chrome.resize(start, Grip::TopLeft, Vec2::new(-20.0, 1000.0), least);
        assert_eq!(corner.size, Vec2::new(420.0, 80.0));
        assert_eq!(corner.max(), start.max());
    }

    fn pixel(image: &Image, x: u32, y: u32) -> [u8; 4] {
        let index = ((y as usize) * (image.width as usize) + x as usize) * 4;
        [
            image.rgba[index],
            image.rgba[index + 1],
            image.rgba[index + 2],
            image.rgba[index + 3],
        ]
    }

    #[test]
    fn every_piece_has_its_own_file_name() {
        let mut names: Vec<String> = Vec::new();
        for piece in Piece::ALL {
            for active in [true, false] {
                names.push(piece.file_name(active));
            }
        }
        for button in Button::ALL {
            for look in Look::ALL {
                names.push(button.file_name(look));
            }
        }
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), names.len(), "two pieces share a file name");
    }

    #[test]
    fn the_title_bar_pieces_are_all_the_same_height() {
        let chrome = chrome();
        for piece in [Piece::TitleLeft, Piece::TitleMid, Piece::TitleRight] {
            assert_eq!(piece.size(&chrome).y, chrome.title);
        }
    }

    #[test]
    fn the_title_corners_are_cut_away_and_the_middle_is_not() {
        let chrome = chrome();
        let left = paint(Piece::TitleLeft, &chrome, true);
        assert_eq!(pixel(&left, 0, 0)[3], 0, "the top left corner is square");
        assert_eq!(
            pixel(&left, left.width - 1, left.height - 1)[3],
            chrome.frame_alpha
        );

        let mid = paint(Piece::TitleMid, &chrome, true);
        assert_eq!(pixel(&mid, 0, 0)[3], chrome.frame_alpha);
    }

    #[test]
    fn the_right_cap_is_the_mirror_of_the_left_one() {
        let chrome = chrome();
        let left = paint(Piece::TitleLeft, &chrome, true);
        let right = paint(Piece::TitleRight, &chrome, true);
        assert_eq!(left.width, right.width);
        for y in 0..left.height {
            for x in 0..left.width {
                assert_eq!(
                    pixel(&left, x, y),
                    pixel(&right, right.width - 1 - x, y),
                    "the caps are not mirror images at {x} {y}"
                );
            }
        }
        assert_eq!(pixel(&left, 0, 0)[3], 0);
        assert_eq!(pixel(&right, right.width - 1, 0)[3], 0);
    }

    #[test]
    fn the_bottom_corners_round_downward() {
        let chrome = chrome();
        let left = paint(Piece::CornerBottomLeft, &chrome, true);
        assert_eq!(pixel(&left, 0, left.height - 1)[3], 0);
        assert_eq!(pixel(&left, left.width - 1, 0)[3], chrome.frame_alpha);
    }

    #[test]
    fn the_idle_frame_is_darker_than_the_active_one() {
        let chrome = chrome();
        let active = paint(Piece::TitleMid, &chrome, true);
        let idle = paint(Piece::TitleMid, &chrome, false);
        assert!(pixel(&idle, 4, 4)[0] < pixel(&active, 4, 4)[0]);
    }

    #[test]
    fn a_tiling_strip_is_the_same_all_the_way_down() {
        let chrome = chrome();
        let edge = paint(Piece::EdgeLeft, &chrome, true);
        let first = pixel(&edge, 0, 0);
        let last = pixel(&edge, 0, edge.height - 1);
        assert_eq!(first, last, "the left edge cannot tile");
    }

    #[test]
    fn the_buttons_are_round_and_the_right_size() {
        let chrome = chrome();
        let close = paint_button(Button::Close, Look::Rest, &chrome);
        assert_eq!(close.width, chrome.button as u32);
        assert_eq!(pixel(&close, 0, 0)[3], 0, "the button is a square");
        let middle = close.width / 2;
        assert_eq!(pixel(&close, middle, middle)[3], 255);
    }

    #[test]
    fn each_button_has_its_own_color() {
        let chrome = chrome();
        let middle = (chrome.button as u32) / 2;
        let close = pixel(
            &paint_button(Button::Close, Look::Rest, &chrome),
            middle,
            middle,
        );
        let minimize = pixel(
            &paint_button(Button::Minimize, Look::Rest, &chrome),
            middle,
            middle,
        );
        assert_ne!(close, minimize);
    }

    #[test]
    fn the_glyph_only_shows_on_hover_and_press() {
        let chrome = chrome();
        let middle = (chrome.button as u32) / 2;
        let rest = pixel(
            &paint_button(Button::Close, Look::Rest, &chrome),
            middle,
            middle,
        );
        let hover = pixel(
            &paint_button(Button::Close, Look::Hover, &chrome),
            middle,
            middle,
        );
        assert_ne!(rest, hover, "hover draws no glyph");
        assert!(hover[0] < rest[0]);
    }

    #[test]
    fn pressing_darkens_the_button() {
        let chrome = chrome();
        let edge = 2;
        let hover = pixel(
            &paint_button(Button::Minimize, Look::Hover, &chrome),
            edge,
            6,
        );
        let press = pixel(
            &paint_button(Button::Minimize, Look::Press, &chrome),
            edge,
            6,
        );
        assert!(press[0] <= hover[0]);
    }

    #[test]
    fn the_buttons_sit_in_a_row_and_are_centered_in_the_title_bar() {
        let chrome = chrome();
        let first = chrome.button_rect(0, 400.0);
        let second = chrome.button_rect(1, 400.0);
        assert_eq!(first.origin.x, chrome.margin);
        assert_eq!(second.origin.x - first.max().x, chrome.gap);
        assert_eq!(first.center().y, chrome.title / 2.0);
    }

    #[test]
    fn buttons_on_the_right_end_one_margin_from_the_right_edge() {
        let mut chrome = chrome();
        chrome.side = Side::Right;
        let last = chrome.button_rect(2, 400.0);
        assert_eq!(last.max().x, 400.0 - chrome.margin);
        let content = Rect::new(Vec2::new(100.0, 100.0), Vec2::new(400.0, 300.0));
        let bar = chrome.title_bar(content);
        let rect = chrome.button_rect(2, bar.size.x);
        assert_eq!(
            chrome.button_at(content, bar.origin + rect.center(), 3),
            Some(2)
        );
        let left = bar.origin + Vec2::new(chrome.margin + 4.0, chrome.title / 2.0);
        assert_eq!(chrome.button_at(content, left, 3), None);
    }

    #[test]
    fn the_config_sets_side_order_and_title() {
        let mut chrome = chrome();
        let config = WindowConfig {
            buttons: Side::Right,
            order: vec![
                ButtonName::Minimize,
                ButtonName::Maximize,
                ButtonName::Close,
            ],
            title: false,
        };
        chrome.follow(&config);
        assert_eq!(chrome.side, Side::Right);
        assert_eq!(
            chrome.order,
            [Button::Minimize, Button::Maximize, Button::Close]
        );
        assert!(!chrome.show_title);
    }

    #[test]
    fn three_buttons_take_a_known_width() {
        let chrome = chrome();
        let wanted = chrome.margin * 2.0 + chrome.button * 3.0 + chrome.gap * 2.0;
        assert_eq!(chrome.buttons_width(3), wanted);
    }

    #[test]
    fn the_frame_wraps_the_content_on_every_side() {
        let chrome = chrome();
        let content = Rect::new(Vec2::new(400.0, 300.0), Vec2::new(800.0, 600.0));
        let layout = chrome.layout(content);
        assert_eq!(layout.frame.origin.x, content.origin.x - chrome.edge);
        assert_eq!(layout.frame.origin.y, content.origin.y - chrome.title);
        assert_eq!(layout.frame.max().x, content.max().x + chrome.edge);
        assert_eq!(layout.frame.max().y, content.max().y + chrome.edge);
    }

    #[test]
    fn the_title_pieces_cover_the_whole_bar_with_no_gap() {
        let chrome = chrome();
        let content = Rect::new(Vec2::new(0.0, 100.0), Vec2::new(500.0, 400.0));
        let layout = chrome.layout(content);
        assert_eq!(layout.title_left.max().x, layout.title_mid.origin.x);
        assert_eq!(layout.title_mid.max().x, layout.title_right.origin.x);
        assert_eq!(layout.title_right.max().x, layout.frame.max().x);
        for piece in [layout.title_left, layout.title_mid, layout.title_right] {
            assert_eq!(piece.size.y, chrome.title);
            assert_eq!(piece.origin.y, layout.frame.origin.y);
        }
    }

    #[test]
    fn the_side_edges_hug_the_content() {
        let chrome = chrome();
        let content = Rect::new(Vec2::new(200.0, 200.0), Vec2::new(300.0, 300.0));
        let layout = chrome.layout(content);
        assert_eq!(layout.edge_left.max().x, content.origin.x);
        assert_eq!(layout.edge_right.origin.x, content.max().x);
        assert_eq!(layout.edge_left.size.y, content.size.y);
    }

    #[test]
    fn the_bottom_corners_sit_at_the_bottom_of_the_frame() {
        let chrome = chrome();
        let content = Rect::new(Vec2::new(10.0, 60.0), Vec2::new(200.0, 150.0));
        let layout = chrome.layout(content);
        assert_eq!(layout.corner_left.origin.x, layout.frame.origin.x);
        assert_eq!(layout.corner_left.max().y, layout.frame.max().y);
        assert_eq!(layout.corner_right.max().x, layout.frame.max().x);
        assert_eq!(layout.corner_right.max().y, layout.frame.max().y);
    }

    #[test]
    fn a_tiny_window_does_not_give_the_title_a_negative_width() {
        let chrome = chrome();
        let content = Rect::new(Vec2::ZERO, Vec2::new(4.0, 4.0));
        let layout = chrome.layout(content);
        assert!(layout.title_mid.size.x >= 0.0);
        assert!(layout.edge_bottom.size.x >= 0.0);
    }

    #[test]
    fn a_click_on_the_title_bar_finds_the_buttons() {
        let chrome = chrome();
        let content = Rect::new(Vec2::new(100.0, 100.0), Vec2::new(400.0, 300.0));
        let bar = chrome.title_bar(content);
        let first = chrome.button_rect(0, bar.size.x);
        let middle = bar.origin + first.origin + first.size / 2.0;
        assert_eq!(chrome.button_at(content, middle, 3), Some(0));
        assert_eq!(chrome.button_at(content, bar.center(), 3), None);
    }

    #[test]
    fn the_first_window_lands_where_it_asked() {
        let room = Rect::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        let wanted = Vec2::new(600.0, 300.0);
        assert_eq!(cascade(wanted, &[], 32.0, room), wanted);
    }

    #[test]
    fn a_second_window_steps_off_the_first() {
        let room = Rect::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        let wanted = Vec2::new(600.0, 300.0);
        let next = cascade(wanted, &[wanted], 32.0, room);
        assert_ne!(next, wanted);
        assert_eq!(next, wanted + Vec2::splat(32.0));
    }

    #[test]
    fn a_third_window_steps_off_both() {
        let room = Rect::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        let wanted = Vec2::new(600.0, 300.0);
        let taken = vec![wanted, wanted + Vec2::splat(32.0)];
        assert_eq!(
            cascade(wanted, &taken, 32.0, room),
            wanted + Vec2::splat(64.0)
        );
    }

    #[test]
    fn a_window_far_from_the_others_never_moves() {
        let room = Rect::new(Vec2::ZERO, Vec2::new(1920.0, 1080.0));
        let wanted = Vec2::new(100.0, 100.0);
        let taken = vec![Vec2::new(900.0, 700.0)];
        assert_eq!(cascade(wanted, &taken, 32.0, room), wanted);
    }

    #[test]
    fn the_cascade_wraps_rather_than_walking_off_the_view() {
        let room = Rect::new(Vec2::ZERO, Vec2::new(400.0, 400.0));
        let wanted = Vec2::new(360.0, 360.0);
        let taken = vec![wanted];
        let next = cascade(wanted, &taken, 32.0, room);
        assert!(next.x < room.max().x && next.y < room.max().y);
    }

    #[test]
    fn the_cascade_gives_up_rather_than_looping_forever() {
        let room = Rect::new(Vec2::ZERO, Vec2::new(200.0, 200.0));
        let taken: Vec<Vec2> = (0..60)
            .map(|step| Vec2::splat(32.0 * step as f64))
            .collect();
        let place = cascade(Vec2::ZERO, &taken, 32.0, room);
        assert!(place.x.is_finite() && place.y.is_finite());
    }

    #[test]
    fn the_title_bar_is_a_move_grip() {
        let chrome = chrome();
        let content = Rect::new(Vec2::new(300.0, 300.0), Vec2::new(400.0, 300.0));
        let bar = chrome.title_bar(content);
        let middle = Vec2::new(bar.center().x, bar.center().y);
        assert_eq!(chrome.grip_at(content, middle, 3), Some(Grip::Title));
    }

    #[test]
    fn the_buttons_win_over_the_title_bar() {
        let chrome = chrome();
        let content = Rect::new(Vec2::new(300.0, 300.0), Vec2::new(400.0, 300.0));
        let bar = chrome.title_bar(content);
        let first = chrome.button_rect(0, bar.size.x);
        let on_button = bar.origin + first.origin + first.size / 2.0;
        assert_eq!(chrome.grip_at(content, on_button, 3), Some(Grip::Button(0)));
    }

    #[test]
    fn the_corners_win_over_the_sides() {
        let chrome = chrome();
        let content = Rect::new(Vec2::new(300.0, 300.0), Vec2::new(400.0, 300.0));
        let frame = chrome.layout(content).frame;
        let corner = Vec2::new(frame.origin.x + 1.0, frame.max().y - 1.0);
        assert_eq!(chrome.grip_at(content, corner, 3), Some(Grip::BottomLeft));
    }

    #[test]
    fn each_side_has_its_own_grip() {
        let chrome = chrome();
        let content = Rect::new(Vec2::new(300.0, 300.0), Vec2::new(400.0, 300.0));
        let frame = chrome.layout(content).frame;
        let middle_y = frame.center().y;
        assert_eq!(
            chrome.grip_at(content, Vec2::new(frame.origin.x + 1.0, middle_y), 3),
            Some(Grip::Left)
        );
        assert_eq!(
            chrome.grip_at(content, Vec2::new(frame.max().x - 1.0, middle_y), 3),
            Some(Grip::Right)
        );
        assert_eq!(
            chrome.grip_at(content, Vec2::new(frame.center().x, frame.max().y - 1.0), 3),
            Some(Grip::Bottom)
        );
    }

    #[test]
    fn the_grip_reaches_outside_the_frame() {
        let chrome = chrome();
        let content = Rect::new(Vec2::new(300.0, 300.0), Vec2::new(400.0, 300.0));
        let frame = chrome.layout(content).frame;
        let just_outside = Vec2::new(frame.origin.x - 2.0, frame.center().y);
        assert_eq!(chrome.grip_at(content, just_outside, 3), Some(Grip::Left));
    }

    #[test]
    fn the_middle_of_a_window_is_not_a_grip() {
        let chrome = chrome();
        let content = Rect::new(Vec2::new(300.0, 300.0), Vec2::new(400.0, 300.0));
        assert_eq!(chrome.grip_at(content, content.center(), 3), None);
    }

    #[test]
    fn a_far_away_click_is_not_a_grip() {
        let chrome = chrome();
        let content = Rect::new(Vec2::new(300.0, 300.0), Vec2::new(400.0, 300.0));
        assert_eq!(chrome.grip_at(content, Vec2::new(10.0, 10.0), 3), None);
    }

    #[test]
    fn dragging_the_right_edge_only_changes_the_width() {
        let chrome = chrome();
        let start = Rect::new(Vec2::new(100.0, 100.0), Vec2::new(400.0, 300.0));
        let after = chrome.resize(start, Grip::Right, Vec2::new(50.0, 80.0), Vec2::splat(40.0));
        assert_eq!(after.origin, start.origin);
        assert_eq!(after.size, Vec2::new(450.0, 300.0));
    }

    #[test]
    fn dragging_the_left_edge_keeps_the_right_edge_still() {
        let chrome = chrome();
        let start = Rect::new(Vec2::new(100.0, 100.0), Vec2::new(400.0, 300.0));
        let after = chrome.resize(start, Grip::Left, Vec2::new(60.0, 0.0), Vec2::splat(40.0));
        assert_eq!(after.max().x, start.max().x);
        assert_eq!(after.size.x, 340.0);
    }

    #[test]
    fn a_window_can_never_be_dragged_smaller_than_the_least() {
        let chrome = chrome();
        let start = Rect::new(Vec2::new(100.0, 100.0), Vec2::new(400.0, 300.0));
        let least = Vec2::new(120.0, 80.0);
        let after = chrome.resize(start, Grip::BottomLeft, Vec2::new(9000.0, -9000.0), least);
        assert_eq!(after.size.x, least.x);
        assert_eq!(after.size.y, least.y);
        assert_eq!(after.max().x, start.max().x);
    }

    #[test]
    fn the_bottom_corner_changes_both_sides() {
        let chrome = chrome();
        let start = Rect::new(Vec2::new(100.0, 100.0), Vec2::new(400.0, 300.0));
        let after = chrome.resize(
            start,
            Grip::BottomRight,
            Vec2::new(20.0, 30.0),
            Vec2::splat(40.0),
        );
        assert_eq!(after.size, Vec2::new(420.0, 330.0));
    }

    #[test]
    fn every_pixel_is_premultiplied() {
        let chrome = chrome();
        let mut all = vec![paint(Piece::TitleLeft, &chrome, true)];
        all.push(paint_button(Button::Close, Look::Hover, &chrome));
        for image in all {
            for pixel in image.rgba.as_chunks::<4>().0 {
                let a = pixel[3];
                assert!(pixel[0] <= a && pixel[1] <= a && pixel[2] <= a);
            }
        }
    }
}
