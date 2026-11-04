use drifter_core::paint::Canvas;
use drifter_core::{Image, Rect, Vec2};

use crate::config::{Config, Cursor};
use crate::font::Face;
use crate::select::{Spot, holds};
use crate::vt::Snapshot;

pub struct Grid {
    pub face: Face,
    pub pad: f64,
    fg: [u8; 3],
    bg: [u8; 3],
    cursor: [u8; 3],
    select: [u8; 3],
    opacity: f64,
    cursor_shape: Cursor,
}

impl Grid {
    pub fn new(cfg: &Config) -> Result<Grid, String> {
        let face = Face::new(cfg.font_px, cfg.font.as_deref())?;
        Ok(Grid {
            face,
            pad: cfg.pad,
            fg: cfg.fg,
            bg: cfg.bg,
            cursor: cfg.cursor,
            select: cfg.theme.selection,
            opacity: cfg.opacity,
            cursor_shape: cfg.cursor_shape,
        })
    }

    pub fn cell(&self) -> (u32, u32) {
        (self.face.cell_w, self.face.cell_h)
    }

    pub fn spot(&self, area: Rect, snap_top: u64, cols: u16, rows: u16, at: Vec2) -> Spot {
        let cw = f64::from(self.face.cell_w);
        let ch = f64::from(self.face.cell_h);
        let inside = at - area.origin - Vec2::new(self.pad, self.pad);
        let col = (inside.x / cw).floor().clamp(0.0, f64::from(cols.max(1) - 1)) as u16;
        let row = (inside.y / ch).floor().clamp(0.0, f64::from(rows.max(1) - 1)) as u64;
        Spot { row: snap_top + row, col }
    }

    pub fn text_size(&self, cols: u32, rows: u32) -> (f64, f64) {
        (
            f64::from(cols) * f64::from(self.face.cell_w) + self.pad * 2.0,
            f64::from(rows) * f64::from(self.face.cell_h) + self.pad * 2.0,
        )
    }

    pub fn fit(&self, area_w: f64, area_h: f64) -> (u16, u16) {
        let inner_w = (area_w - self.pad * 2.0).max(f64::from(self.face.cell_w));
        let inner_h = (area_h - self.pad * 2.0).max(f64::from(self.face.cell_h));
        let cols = (inner_w / f64::from(self.face.cell_w)).floor().max(1.0) as u16;
        let rows = (inner_h / f64::from(self.face.cell_h)).floor().max(1.0) as u16;
        (cols, rows)
    }

    pub fn base_image(&self, size: (u32, u32), fill: [u8; 3]) -> Image {
        let alpha = (self.opacity * 255.0).round() as u8;
        let mut rgba = Vec::with_capacity((size.0 * size.1 * 4) as usize);
        for _ in 0..(size.0 * size.1) {
            rgba.extend_from_slice(&[fill[0], fill[1], fill[2], alpha]);
        }
        Image { width: size.0, height: size.1, rgba }
    }

    pub fn bg(&self) -> [u8; 3] {
        self.bg
    }

    pub fn opacity(&self) -> f64 {
        self.opacity
    }

    pub fn paint_cells(&mut self, canvas: &mut Canvas, snap: &Snapshot, area: Rect, picked: Option<(Spot, Spot)>) {
        let cw = f64::from(self.face.cell_w);
        let ch = f64::from(self.face.cell_h);
        let origin = area.origin + Vec2::new(self.pad, self.pad);
        for (y, row) in snap.grid.iter().enumerate() {
            let cy = origin.y + y as f64 * ch;
            for (x, cell) in row.iter().enumerate() {
                let cx = origin.x + x as f64 * cw;
                let orig_fg = cell.fg.unwrap_or(self.fg);
                let orig_bg = cell.bg.unwrap_or(self.bg);
                let (fg, bg_fill) = if cell.inverse {
                    (orig_bg, Some(orig_fg))
                } else {
                    (orig_fg, cell.bg)
                };
                let chosen = picked.is_some_and(|bounds| holds(bounds, snap.top + y as u64, x as u16));
                let bg_fill = if chosen { Some(self.select) } else { bg_fill };
                if let Some(bgc) = bg_fill
                    && bgc != self.bg
                {
                    canvas.round_rect(Rect::new(Vec2::new(cx, cy), Vec2::new(cw, ch)), 0.0, bgc, 1.0);
                }
                let text = cell.text.trim_end_matches(' ');
                if !text.is_empty() {
                    let glyph = self.face.glyph(text, fg, cell.bold, cell.italic);
                    canvas.stamp(glyph, Vec2::new(cx, cy), 1.0);
                }
                if cell.underline {
                    let uy = cy + ch - 2.0;
                    canvas.line(Vec2::new(cx, uy), Vec2::new(cx + cw, uy), 1.0, fg, 1.0);
                }
            }
        }
        if let Some((cx, cy)) = snap.cursor {
            let rect = Rect::new(
                Vec2::new(origin.x + f64::from(cx) * cw, origin.y + f64::from(cy) * ch),
                Vec2::new(cw, ch),
            );
            match self.cursor_shape {
                Cursor::Block => canvas.round_rect(rect, 1.0, self.cursor, 0.55),
                Cursor::Bar => canvas.round_rect(Rect::new(rect.origin, Vec2::new(2.0, ch)), 0.0, self.cursor, 0.9),
                Cursor::Underline => canvas.round_rect(
                    Rect::new(Vec2::new(rect.origin.x, rect.origin.y + ch - 2.0), Vec2::new(cw, 2.0)),
                    0.0,
                    self.cursor,
                    0.9,
                ),
            }
        }
    }
}
