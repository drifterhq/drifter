use drifter_core::paint::Canvas;
use drifter_core::{Font, Image, Rect, Vec2};

use crate::vt::Snapshot;

const MONO_FONTS: [&str; 7] = [
    "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu-sans-mono-fonts/DejaVuSansMono.ttf",
    "/usr/share/fonts/dejavu/DejaVuSansMono.ttf",
    "/usr/share/fonts/liberation/LiberationMono-Regular.ttf",
    "/usr/share/fonts/liberation-mono/LiberationMono-Regular.ttf",
    "/usr/share/fonts/TTF/DejaVuSansMono.ttf",
    "/usr/share/fonts/truetype/liberation/LiberationMono-Regular.ttf",
];

pub struct Grid {
    pub font: Font,
    pub cell_w: u32,
    pub cell_h: u32,
    pub pad: f64,
}

impl Grid {
    pub fn new(px: f64, pad: f64) -> Result<Grid, String> {
        let path = MONO_FONTS.iter().map(std::path::PathBuf::from).find(|p| p.is_file());
        let font = match path {
            Some(path) => Font::load(&path, px).or_else(|_| Font::find(None, px))?,
            None => Font::find(None, px)?,
        };
        let cell_w = font.width_of("M").round().max(1.0) as u32;
        let cell_h = font.line_height().round().max(1.0) as u32;
        Ok(Grid { font, cell_w, cell_h, pad })
    }

    /// How many cols and rows fit in a window of this logical pixel size.
    pub fn fit(&self, width: u32, height: u32) -> (u16, u16) {
        let inner_w = (f64::from(width) - self.pad * 2.0).max(f64::from(self.cell_w));
        let inner_h = (f64::from(height) - self.pad * 2.0).max(f64::from(self.cell_h));
        let cols = (inner_w / f64::from(self.cell_w)).floor().max(1.0) as u16;
        let rows = (inner_h / f64::from(self.cell_h)).floor().max(1.0) as u16;
        (cols, rows)
    }

    pub fn paint(&self, snap: &Snapshot, width: u32, height: u32) -> Image {
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        let [br, bg, bb] = snap.bg;
        for _ in 0..(width * height) {
            rgba.extend_from_slice(&[br, bg, bb, 255]);
        }
        let mut canvas = Canvas::on(Image { width, height, rgba });

        let cw = f64::from(self.cell_w);
        let ch = f64::from(self.cell_h);
        for (y, row) in snap.grid.iter().enumerate() {
            let cy = self.pad + y as f64 * ch;
            for (x, cell) in row.iter().enumerate() {
                let cx = self.pad + x as f64 * cw;
                let mut fg = cell.fg;
                let mut cell_bg = cell.bg;
                if cell.inverse {
                    let shown_bg = cell_bg.unwrap_or(snap.bg);
                    cell_bg = Some(fg);
                    fg = shown_bg;
                }
                if let Some(bgc) = cell_bg
                    && bgc != snap.bg
                {
                    canvas.round_rect(Rect::new(Vec2::new(cx, cy), Vec2::new(cw, ch)), 0.0, bgc, 1.0);
                }
                let text = cell.text.trim_end_matches(' ');
                if !text.is_empty() {
                    let glyph = if cell.bold {
                        self.font.paint_bold(text, fg)
                    } else {
                        self.font.paint(text, fg)
                    };
                    canvas.stamp(&glyph, Vec2::new(cx, cy), 1.0);
                }
                if cell.underline {
                    let uy = cy + ch - 2.0;
                    canvas.line(Vec2::new(cx, uy), Vec2::new(cx + cw, uy), 1.0, fg, 1.0);
                }
            }
        }

        if let Some((cx, cy)) = snap.cursor {
            let rect = Rect::new(
                Vec2::new(self.pad + f64::from(cx) * cw, self.pad + f64::from(cy) * ch),
                Vec2::new(cw, ch),
            );
            canvas.round_rect(rect, 1.0, snap.fg, 0.55);
        }

        canvas.image
    }
}
