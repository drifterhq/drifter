use core::ffi::c_void;
use std::ptr;

use crate::ffi;

#[derive(Clone)]
pub struct Cell {
    pub text: String,
    pub fg: Option<[u8; 3]>,
    pub bg: Option<[u8; 3]>,
    pub bold: bool,
    pub italic: bool,
    pub inverse: bool,
    pub underline: bool,
}

pub struct Snapshot {
    pub cols: u16,
    pub rows: u16,
    pub grid: Vec<Vec<Cell>>,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
    pub cursor: Option<(u16, u16)>,
    pub cursor_block: bool,
    pub top: u64,
}

pub struct Vt {
    term: ffi::GhosttyTerminal,
    state: ffi::GhosttyRenderState,
    iter: ffi::GhosttyRenderStateRowIterator,
    cells: ffi::GhosttyRenderStateRowCells,
    pub cols: u16,
    pub rows: u16,
}

fn rgb(c: ffi::GhosttyColorRgb) -> [u8; 3] {
    [c.r, c.g, c.b]
}

unsafe extern "C" fn reply(_: ffi::GhosttyTerminal, user: *mut c_void, data: *const u8, len: usize) {
    if data.is_null() || len == 0 {
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data, len) };
    crate::pty::write_all(user as usize as std::os::fd::RawFd, bytes);
}

impl Vt {
    pub fn new(cols: u16, rows: u16, fd: std::os::fd::RawFd) -> Result<Vt, String> {
        unsafe {
            let mut term: ffi::GhosttyTerminal = ptr::null_mut();
            if ffi::ghostty_terminal_new(ptr::null(), &mut term, cols.max(1), rows.max(1)) != ffi::GHOSTTY_SUCCESS {
                return Err("ghostty_terminal_new failed".into());
            }
            ffi::ghostty_terminal_set(term, ffi::OPT_USERDATA, fd as usize as *const c_void);
            let answer: ffi::GhosttyTerminalWritePtyFn = reply;
            ffi::ghostty_terminal_set(term, ffi::OPT_WRITE_PTY, answer as *const c_void);
            let mut state: ffi::GhosttyRenderState = ptr::null_mut();
            let mut iter: ffi::GhosttyRenderStateRowIterator = ptr::null_mut();
            let mut cells: ffi::GhosttyRenderStateRowCells = ptr::null_mut();
            ffi::ghostty_render_state_new(ptr::null(), &mut state);
            ffi::ghostty_render_state_row_iterator_new(ptr::null(), &mut iter);
            ffi::ghostty_render_state_row_cells_new(ptr::null(), &mut cells);
            Ok(Vt { term, state, iter, cells, cols, rows })
        }
    }

    pub fn paint_with(&mut self, theme: &drifter_core::termtheme::Theme) {
        let color = |c: [u8; 3]| ffi::GhosttyColorRgb { r: c[0], g: c[1], b: c[2] };
        let fg = color(theme.foreground);
        let bg = color(theme.background);
        let palette = drifter_core::termtheme::full_palette(theme).map(color);
        unsafe {
            ffi::ghostty_terminal_set(self.term, ffi::OPT_COLOR_FOREGROUND, &fg as *const _ as *const c_void);
            ffi::ghostty_terminal_set(self.term, ffi::OPT_COLOR_BACKGROUND, &bg as *const _ as *const c_void);
            match theme.cursor.map(color) {
                Some(cursor) => ffi::ghostty_terminal_set(self.term, ffi::OPT_COLOR_CURSOR, &cursor as *const _ as *const c_void),
                None => ffi::ghostty_terminal_set(self.term, ffi::OPT_COLOR_CURSOR, ptr::null()),
            };
            ffi::ghostty_terminal_set(self.term, ffi::OPT_COLOR_PALETTE, palette.as_ptr() as *const c_void);
            let mut fresh: ffi::GhosttyRenderState = ptr::null_mut();
            if ffi::ghostty_render_state_new(ptr::null(), &mut fresh) == ffi::GHOSTTY_SUCCESS {
                if !self.state.is_null() {
                    ffi::ghostty_render_state_free(self.state);
                }
                self.state = fresh;
            }
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) {
        unsafe { ffi::ghostty_terminal_vt_write(self.term, bytes.as_ptr(), bytes.len()) };
    }

    pub fn title(&self) -> String {
        let mut text = ffi::GhosttyString { ptr: ptr::null(), len: 0 };
        unsafe {
            if ffi::ghostty_terminal_get(self.term, ffi::TERM_TITLE, &mut text as *mut _ as *mut c_void) != ffi::GHOSTTY_SUCCESS
                || text.ptr.is_null()
            {
                return String::new();
            }
            String::from_utf8_lossy(std::slice::from_raw_parts(text.ptr, text.len)).trim().to_string()
        }
    }

    pub fn scrollbar(&self) -> ffi::GhosttyTerminalScrollbar {
        let mut bar = ffi::GhosttyTerminalScrollbar::default();
        unsafe { ffi::ghostty_terminal_get(self.term, ffi::TERM_SCROLLBAR, &mut bar as *mut _ as *mut c_void) };
        bar
    }

    pub fn scrolled_back(&self) -> bool {
        let bar = self.scrollbar();
        bar.offset + bar.len < bar.total
    }

    pub fn scroll(&mut self, lines: isize) {
        let behavior = ffi::GhosttyTerminalScrollViewport {
            tag: ffi::SCROLL_DELTA,
            value: ffi::GhosttyTerminalScrollViewportValue { delta: lines },
        };
        unsafe { ffi::ghostty_terminal_scroll_viewport(self.term, behavior) };
    }

    pub fn scroll_bottom(&mut self) {
        let behavior = ffi::GhosttyTerminalScrollViewport {
            tag: ffi::SCROLL_BOTTOM,
            value: ffi::GhosttyTerminalScrollViewportValue { pad: [0; 2] },
        };
        unsafe { ffi::ghostty_terminal_scroll_viewport(self.term, behavior) };
    }

    pub fn alternate(&self) -> bool {
        let mut screen: i32 = 0;
        unsafe { ffi::ghostty_terminal_get(self.term, ffi::TERM_ACTIVE_SCREEN, &mut screen as *mut _ as *mut c_void) };
        screen == 1
    }

    pub fn mouse_tracking(&self) -> bool {
        let mut on = false;
        unsafe { ffi::ghostty_terminal_get(self.term, ffi::TERM_MOUSE_TRACKING, &mut on as *mut _ as *mut c_void) };
        on
    }

    pub fn mode(&self, number: u16) -> bool {
        let mut config = ffi::GhosttyTerminalModeConfig { mode: number & 0x7fff, value: false };
        unsafe { ffi::ghostty_terminal_get(self.term, ffi::TERM_MODE, &mut config as *mut _ as *mut c_void) };
        config.value
    }

    pub fn resize(&mut self, cols: u16, rows: u16, cell_w: u32, cell_h: u32) {
        self.cols = cols.max(1);
        self.rows = rows.max(1);
        unsafe { ffi::ghostty_terminal_resize(self.term, self.cols, self.rows, cell_w, cell_h) };
    }

    pub fn snapshot(&mut self) -> Snapshot {
        unsafe {
            ffi::ghostty_render_state_update(self.state, self.term);

            let mut colors: ffi::GhosttyRenderStateColors = std::mem::zeroed();
            colors.size = std::mem::size_of::<ffi::GhosttyRenderStateColors>();
            ffi::ghostty_render_state_get(self.state, ffi::DATA_COLORS, &mut colors as *mut _ as *mut c_void);
            let fg = rgb(colors.foreground);
            let bg = rgb(colors.background);

            let blank = Cell { text: String::new(), fg: None, bg: None, bold: false, italic: false, inverse: false, underline: false };
            let mut grid: Vec<Vec<Cell>> = vec![vec![blank.clone(); self.cols as usize]; self.rows as usize];

            ffi::ghostty_render_state_get(self.state, ffi::DATA_ROW_ITERATOR, &mut self.iter as *mut _ as *mut c_void);
            while ffi::ghostty_render_state_row_iterator_next(self.iter) {
                let mut vy: i32 = -1;
                ffi::ghostty_render_state_row_get(self.iter, ffi::ROW_DATA_VIEWPORT_Y, &mut vy as *mut _ as *mut c_void);
                if vy < 0 || vy as usize >= grid.len() {
                    continue;
                }
                if ffi::ghostty_render_state_row_get(self.iter, ffi::ROW_DATA_CELLS, &mut self.cells as *mut _ as *mut c_void) != ffi::GHOSTTY_SUCCESS {
                    continue;
                }
                let row = &mut grid[vy as usize];
                let mut x = 0usize;
                while ffi::ghostty_render_state_row_cells_next(self.cells) && x < row.len() {
                    row[x] = self.read_cell();
                    x += 1;
                }
            }

            let mut cur: ffi::GhosttyRenderStateCursor = std::mem::zeroed();
            cur.size = std::mem::size_of::<ffi::GhosttyRenderStateCursor>();
            ffi::ghostty_render_state_get(self.state, ffi::DATA_CURSOR, &mut cur as *mut _ as *mut c_void);
            let cursor = if cur.viewport_has_value && cur.visible {
                Some((cur.viewport_x, cur.viewport_y))
            } else {
                None
            };
            let cursor_block = cur.visual_style == 0 || cur.visual_style == 1;

            let top = self.scrollbar().offset;
            Snapshot { cols: self.cols, rows: self.rows, grid, fg, bg, cursor, cursor_block, top }
        }
    }

    unsafe fn read_cell(&self) -> Cell { unsafe {
        let mut buf = [0u8; 32];
        let mut gb = ffi::GhosttyBuffer { ptr: buf.as_mut_ptr(), cap: buf.len(), len: 0 };
        ffi::ghostty_render_state_row_cells_get(self.cells, ffi::CELL_GRAPHEMES_UTF8, &mut gb as *mut _ as *mut c_void);
        let text = if gb.len > 0 && gb.len <= buf.len() {
            String::from_utf8_lossy(&buf[..gb.len]).into_owned()
        } else {
            String::new()
        };

        let mut fgc = ffi::GhosttyColorRgb { r: 0, g: 0, b: 0 };
        let fg = if ffi::ghostty_render_state_row_cells_get(self.cells, ffi::CELL_FG_COLOR, &mut fgc as *mut _ as *mut c_void) == ffi::GHOSTTY_SUCCESS {
            Some(rgb(fgc))
        } else {
            None
        };

        let mut bgc = ffi::GhosttyColorRgb { r: 0, g: 0, b: 0 };
        let bg = if ffi::ghostty_render_state_row_cells_get(self.cells, ffi::CELL_BG_COLOR, &mut bgc as *mut _ as *mut c_void) == ffi::GHOSTTY_SUCCESS {
            Some(rgb(bgc))
        } else {
            None
        };

        let mut style: ffi::GhosttyStyle = std::mem::zeroed();
        style.size = std::mem::size_of::<ffi::GhosttyStyle>();
        let (bold, italic, inverse, underline) = if ffi::ghostty_render_state_row_cells_get(self.cells, ffi::CELL_STYLE, &mut style as *mut _ as *mut c_void) == ffi::GHOSTTY_SUCCESS {
            (style.bold, style.italic, style.inverse, style.underline != 0)
        } else {
            (false, false, false, false)
        };

        Cell { text, fg, bg, bold, italic, inverse, underline }
    }}
}

impl Drop for Vt {
    fn drop(&mut self) {
        unsafe {
            if !self.cells.is_null() {
                ffi::ghostty_render_state_row_cells_free(self.cells);
            }
            if !self.iter.is_null() {
                ffi::ghostty_render_state_row_iterator_free(self.iter);
            }
            if !self.state.is_null() {
                ffi::ghostty_render_state_free(self.state);
            }
            if !self.term.is_null() {
                ffi::ghostty_terminal_free(self.term);
            }
        }
    }
}
