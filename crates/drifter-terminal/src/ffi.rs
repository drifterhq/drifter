#![allow(non_camel_case_types, non_upper_case_globals, dead_code)]

use core::ffi::c_void;

pub type GhosttyTerminal = *mut c_void;
pub type GhosttyRenderState = *mut c_void;
pub type GhosttyRenderStateRowIterator = *mut c_void;
pub type GhosttyRenderStateRowCells = *mut c_void;

pub const GHOSTTY_SUCCESS: i32 = 0;

// GhosttyRenderStateData
pub const DATA_COLS: i32 = 1;
pub const DATA_ROWS: i32 = 2;
pub const DATA_DIRTY: i32 = 3;
pub const DATA_ROW_ITERATOR: i32 = 4;
pub const DATA_CURSOR: i32 = 18;
pub const DATA_COLORS: i32 = 19;

// GhosttyRenderStateRowData
pub const ROW_DATA_CELLS: i32 = 3;
pub const ROW_DATA_VIEWPORT_Y: i32 = 6;

// GhosttyRenderStateRowCellsData
pub const CELL_STYLE: i32 = 2;
pub const CELL_BG_COLOR: i32 = 5;
pub const CELL_FG_COLOR: i32 = 6;
pub const CELL_SELECTED: i32 = 7;
pub const CELL_GRAPHEMES_UTF8: i32 = 9;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyColorRgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

#[repr(C)]
pub struct GhosttyBuffer {
    pub ptr: *mut u8,
    pub cap: usize,
    pub len: usize,
}

#[repr(C, align(8))]
#[derive(Clone, Copy)]
pub union GhosttyStyleColorValue {
    pub palette: u16,
    pub rgb: GhosttyColorRgb,
    pub pad: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct GhosttyStyleColor {
    pub tag: i32,
    pub value: GhosttyStyleColorValue,
}

#[repr(C)]
pub struct GhosttyStyle {
    pub size: usize,
    pub fg_color: GhosttyStyleColor,
    pub bg_color: GhosttyStyleColor,
    pub underline_color: GhosttyStyleColor,
    pub bold: bool,
    pub italic: bool,
    pub faint: bool,
    pub blink: bool,
    pub inverse: bool,
    pub invisible: bool,
    pub strikethrough: bool,
    pub overline: bool,
    pub underline: i32,
}

#[repr(C)]
pub struct GhosttyRenderStateColors {
    pub size: usize,
    pub background: GhosttyColorRgb,
    pub foreground: GhosttyColorRgb,
    pub cursor: GhosttyColorRgb,
    pub cursor_has_value: bool,
    pub palette: [GhosttyColorRgb; 256],
}

#[repr(C)]
pub struct GhosttyRenderStateCursor {
    pub size: usize,
    pub viewport_has_value: bool,
    pub viewport_x: u16,
    pub viewport_y: u16,
    pub wide_tail: bool,
    pub visible: bool,
    pub blinking: bool,
    pub password_input: bool,
    pub visual_style: i32,
}

unsafe extern "C" {
    pub fn ghostty_terminal_new(allocator: *const c_void, out: *mut GhosttyTerminal, cols: u16, rows: u16) -> i32;
    pub fn ghostty_terminal_free(terminal: GhosttyTerminal);
    pub fn ghostty_terminal_resize(terminal: GhosttyTerminal, cols: u16, rows: u16, cell_width_px: u32, cell_height_px: u32) -> i32;
    pub fn ghostty_terminal_vt_write(terminal: GhosttyTerminal, data: *const u8, len: usize);

    pub fn ghostty_render_state_new(allocator: *const c_void, out: *mut GhosttyRenderState) -> i32;
    pub fn ghostty_render_state_free(state: GhosttyRenderState);
    pub fn ghostty_render_state_update(state: GhosttyRenderState, terminal: GhosttyTerminal) -> i32;
    pub fn ghostty_render_state_get(state: GhosttyRenderState, data: i32, out: *mut c_void) -> i32;

    pub fn ghostty_render_state_row_iterator_new(allocator: *const c_void, out: *mut GhosttyRenderStateRowIterator) -> i32;
    pub fn ghostty_render_state_row_iterator_free(iterator: GhosttyRenderStateRowIterator);
    pub fn ghostty_render_state_row_iterator_next(iterator: GhosttyRenderStateRowIterator) -> bool;
    pub fn ghostty_render_state_row_iterator_next_dirty(iterator: GhosttyRenderStateRowIterator, out_y: *mut u16) -> bool;
    pub fn ghostty_render_state_row_get(iterator: GhosttyRenderStateRowIterator, data: i32, out: *mut c_void) -> i32;

    pub fn ghostty_render_state_row_cells_new(allocator: *const c_void, out: *mut GhosttyRenderStateRowCells) -> i32;
    pub fn ghostty_render_state_row_cells_free(cells: GhosttyRenderStateRowCells);
    pub fn ghostty_render_state_row_cells_next(cells: GhosttyRenderStateRowCells) -> bool;
    pub fn ghostty_render_state_row_cells_select(cells: GhosttyRenderStateRowCells, x: u16) -> i32;
    pub fn ghostty_render_state_row_cells_get(cells: GhosttyRenderStateRowCells, data: i32, out: *mut c_void) -> i32;
}
