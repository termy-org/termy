use std::collections::VecDeque;

use serde::Serialize;
use termy_config_core::{
    AppConfig, ConfigDiagnostic, ConfigDiagnosticKind, CursorStyle, SHELL_DECIDE_THEME_ID,
    SystemAppearance, resolve_active_theme,
};
use termy_themes::{ThemeColors, normalize_theme_id};
use unicode_width::UnicodeWidthChar;
use vte::{Params, Parser, Perform};
use wasm_bindgen::prelude::*;

const DEFAULT_CELL_WIDTH: f32 = 9.0;
const DEFAULT_CELL_HEIGHT: f32 = 18.0;
const DEFAULT_SCROLLBACK: usize = 5000;

#[wasm_bindgen]
pub struct TermyTerminal {
    parser: Parser,
    screen: TerminalScreen,
}

#[wasm_bindgen]
impl TermyTerminal {
    #[wasm_bindgen(constructor)]
    pub fn new(cols: u16, rows: u16) -> Self {
        Self::with_cell_size(cols, rows, DEFAULT_CELL_WIDTH, DEFAULT_CELL_HEIGHT)
    }

    #[wasm_bindgen(js_name = withCellSize)]
    pub fn with_cell_size(cols: u16, rows: u16, cell_width: f32, cell_height: f32) -> Self {
        Self {
            parser: Parser::new(),
            screen: TerminalScreen::new(cols, rows, cell_width, cell_height),
        }
    }

    pub fn resize(&mut self, cols: u16, rows: u16, cell_width: f32, cell_height: f32) {
        self.screen.resize(cols, rows, cell_width, cell_height);
    }

    #[wasm_bindgen(js_name = setScrollback)]
    pub fn set_scrollback(&mut self, budget: usize) {
        self.screen.set_scrollback(budget);
    }

    #[wasm_bindgen(js_name = scrollLines)]
    pub fn scroll_lines(&mut self, amount: i32) {
        self.screen.scroll_lines(amount);
    }

    #[wasm_bindgen(js_name = scrollToBottom)]
    pub fn scroll_to_bottom(&mut self) {
        self.screen.scroll_to_bottom();
    }

    #[wasm_bindgen(js_name = displayOffset)]
    pub fn display_offset(&self) -> usize {
        self.screen.display_offset
    }

    #[wasm_bindgen(js_name = historySize)]
    pub fn history_size(&self) -> usize {
        self.screen.history.len()
    }

    #[wasm_bindgen(js_name = applicationCursorKeys)]
    pub fn application_cursor_keys(&self) -> bool {
        self.screen.application_cursor_keys
    }

    #[wasm_bindgen(js_name = bracketedPaste)]
    pub fn bracketed_paste(&self) -> bool {
        self.screen.bracketed_paste
    }

    /// Returns the URI of the OSC8 hyperlink associated with the cell at
    /// `(row, col)` in the current viewport, or `None` if that cell has no
    /// hyperlink attached.
    #[wasm_bindgen(js_name = hyperlinkAt)]
    pub fn hyperlink_at(&self, row: u16, col: u16) -> Option<String> {
        self.screen.hyperlink_at(row, col)
    }

    /// Encode an xterm-compatible mouse report for the currently active mouse
    /// mode / encoding. Returns `None` when the current mode does not report
    /// this kind of event (e.g. `Move` events when only `Normal` tracking is
    /// active), when the mouse mode is disabled, or when the coordinates are
    /// out of range for the selected legacy encoding.
    ///
    /// Parameter encoding:
    /// * `button`:
    ///     - 0 = left, 1 = middle, 2 = right
    ///     - 64 = wheel-up, 65 = wheel-down, 66 = wheel-left, 67 = wheel-right
    /// * `modifiers` (bitmask): 4 = shift, 8 = alt, 16 = control
    /// * `kind`: 0 = press, 1 = release, 2 = drag, 3 = move (motion only)
    #[wasm_bindgen(js_name = encodeMouseReport)]
    pub fn encode_mouse_report(
        &self,
        button: u8,
        modifiers: u8,
        col: u16,
        row: u16,
        kind: u8,
    ) -> Option<Vec<u8>> {
        self.screen
            .encode_mouse_report(button, modifiers, col, row, kind)
    }

    /// Current mouse tracking mode as a lowercased camelCase-friendly string.
    /// See `MouseMode::as_str`.
    #[wasm_bindgen(js_name = mouseMode)]
    pub fn mouse_mode(&self) -> String {
        self.screen.mouse_mode.as_str().to_string()
    }

    /// Current mouse encoding as a string. See `MouseEncoding::as_str`.
    #[wasm_bindgen(js_name = mouseEncoding)]
    pub fn mouse_encoding(&self) -> String {
        self.screen.mouse_encoding.as_str().to_string()
    }

    #[wasm_bindgen(js_name = setConfigContents)]
    pub fn set_config_contents(&mut self, contents: &str) -> Result<JsValue, JsValue> {
        let report = AppConfig::from_contents_with_report(contents);
        let render_config = render_config_from_report(report, SystemAppearance::Dark);
        self.screen
            .set_palette(Palette::from_render_config(&render_config));
        to_js(&render_config)
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Result<JsValue, JsValue> {
        self.parser.advance(&mut self.screen, bytes);
        self.drain()
    }

    pub fn drain(&mut self) -> Result<JsValue, JsValue> {
        to_js(&FeedResult {
            events: std::mem::take(&mut self.screen.events),
            responses: std::mem::take(&mut self.screen.responses),
        })
    }

    pub fn snapshot(&self) -> Result<JsValue, JsValue> {
        to_js(&self.screen.snapshot())
    }

    pub fn search(&self, query: &str) -> Result<JsValue, JsValue> {
        to_js(&self.screen.search(query))
    }
}

#[wasm_bindgen(js_name = defaultRenderConfig)]
pub fn default_render_config() -> Result<JsValue, JsValue> {
    let report = AppConfig::from_contents_with_report("");
    to_js(&render_config_from_report(report, SystemAppearance::Dark))
}

#[wasm_bindgen(js_name = renderConfigFromContents)]
pub fn render_config_from_contents(contents: &str) -> Result<JsValue, JsValue> {
    let report = AppConfig::from_contents_with_report(contents);
    to_js(&render_config_from_report(report, SystemAppearance::Dark))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MouseMode {
    None,
    X10,
    Normal,
    ButtonEvent,
    AnyEvent,
}

impl MouseMode {
    fn as_str(self) -> &'static str {
        match self {
            MouseMode::None => "none",
            MouseMode::X10 => "x10",
            MouseMode::Normal => "normal",
            MouseMode::ButtonEvent => "button-event",
            MouseMode::AnyEvent => "any-event",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MouseEncoding {
    Legacy,
    Sgr,
    Utf8,
    SgrPixel,
}

impl MouseEncoding {
    fn as_str(self) -> &'static str {
        match self {
            MouseEncoding::Legacy => "legacy",
            MouseEncoding::Sgr => "sgr",
            MouseEncoding::Utf8 => "utf8",
            MouseEncoding::SgrPixel => "sgr-pixel",
        }
    }
}

/// Kind of mouse event passed to `encode_mouse_report`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MouseEventKind {
    Press,
    Release,
    Drag,
    Move,
}

impl MouseEventKind {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(MouseEventKind::Press),
            1 => Some(MouseEventKind::Release),
            2 => Some(MouseEventKind::Drag),
            3 => Some(MouseEventKind::Move),
            _ => None,
        }
    }
}

#[derive(Clone)]
struct TerminalScreen {
    cols: u16,
    rows: u16,
    cell_width: f32,
    cell_height: f32,
    cells: Vec<Cell>,
    cursor_col: usize,
    cursor_row: usize,
    current_fg: TerminalColor,
    current_bg: TerminalColor,
    current_bold: bool,
    current_italic: bool,
    current_underline: bool,
    current_strikethrough: bool,
    current_dim: bool,
    current_reverse: bool,
    current_blink: bool,
    current_invisible: bool,
    palette: Palette,
    title: Option<String>,
    display_offset: usize,
    history: VecDeque<Vec<Cell>>,
    scrollback_budget: usize,
    application_cursor_keys: bool,
    mouse_mode: MouseMode,
    mouse_encoding: MouseEncoding,
    bracketed_paste: bool,
    current_hyperlink: Option<u32>,
    hyperlinks: Vec<String>,
    events: Vec<BrowserEvent>,
    responses: Vec<String>,
}

impl TerminalScreen {
    fn new(cols: u16, rows: u16, cell_width: f32, cell_height: f32) -> Self {
        let palette = Palette::default();
        let mut screen = Self {
            cols,
            rows,
            cell_width,
            cell_height,
            cells: Vec::new(),
            cursor_col: 0,
            cursor_row: 0,
            current_fg: palette.foreground,
            current_bg: palette.background,
            current_bold: false,
            current_italic: false,
            current_underline: false,
            current_strikethrough: false,
            current_dim: false,
            current_reverse: false,
            current_blink: false,
            current_invisible: false,
            palette,
            title: None,
            display_offset: 0,
            history: VecDeque::new(),
            scrollback_budget: DEFAULT_SCROLLBACK,
            application_cursor_keys: false,
            mouse_mode: MouseMode::None,
            mouse_encoding: MouseEncoding::Legacy,
            bracketed_paste: false,
            current_hyperlink: None,
            hyperlinks: Vec::new(),
            events: Vec::new(),
            responses: Vec::new(),
        };
        screen.cells = vec![screen.blank_cell(0, 0); screen.cell_count()];
        screen.reindex_cells();
        screen
    }

    fn set_scrollback(&mut self, budget: usize) {
        self.scrollback_budget = budget;
        while self.history.len() > budget {
            self.history.pop_front();
        }
        if self.display_offset > self.history.len() {
            self.display_offset = self.history.len();
        }
    }

    /// Find an existing hyperlink URI in the table or insert it and return
    /// the index. Used by the OSC8 handler.
    fn intern_hyperlink(&mut self, uri: &str) -> u32 {
        if let Some(existing) = self.hyperlinks.iter().position(|entry| entry == uri) {
            return existing as u32;
        }
        let id = self.hyperlinks.len() as u32;
        self.hyperlinks.push(uri.to_string());
        id
    }

    /// Look up the OSC8 hyperlink URI for the cell at viewport `(row, col)`.
    fn hyperlink_at(&self, row: u16, col: u16) -> Option<String> {
        let cols = usize::from(self.cols);
        let rows = usize::from(self.rows);
        let row = usize::from(row);
        let col = usize::from(col);
        if cols == 0 || rows == 0 || row >= rows || col >= cols {
            return None;
        }

        let history_len = self.history.len();
        let display_offset = self.display_offset.min(history_len);
        let history_overlay = display_offset.min(rows);

        let cell_hyperlink = if row < history_overlay {
            let history_start = history_len - display_offset;
            self.history
                .get(history_start + row)
                .and_then(|line| line.get(col))
                .and_then(|cell| cell.hyperlink_id)
        } else {
            let grid_row = row - history_overlay;
            let idx = grid_row * cols + col;
            self.cells.get(idx).and_then(|cell| cell.hyperlink_id)
        };

        cell_hyperlink.and_then(|id| self.hyperlinks.get(id as usize).cloned())
    }

    fn set_mouse_mode(&mut self, mode: MouseMode) {
        self.mouse_mode = mode;
        if mode == MouseMode::None {
            // Reset encoding when mouse reporting is disabled so legacy clients
            // get a fresh slate on the next mode-set.
            self.mouse_encoding = MouseEncoding::Legacy;
        }
    }

    fn encode_mouse_report(
        &self,
        button: u8,
        modifiers: u8,
        col: u16,
        row: u16,
        kind: u8,
    ) -> Option<Vec<u8>> {
        let kind = MouseEventKind::from_u8(kind)?;
        if !event_allowed(self.mouse_mode, kind) {
            return None;
        }
        encode_mouse_packet(
            self.mouse_mode,
            self.mouse_encoding,
            button,
            modifiers,
            col,
            row,
            kind,
        )
    }

    fn scroll_lines(&mut self, amount: i32) {
        if amount == 0 {
            return;
        }
        let history_len = self.history.len();
        if amount > 0 {
            let limit = history_len.saturating_sub(self.display_offset);
            let delta = (amount as usize).min(limit);
            self.display_offset = self.display_offset.saturating_add(delta);
        } else {
            let delta = (-amount) as usize;
            self.display_offset = self.display_offset.saturating_sub(delta);
        }
    }

    fn scroll_to_bottom(&mut self) {
        self.display_offset = 0;
    }

    fn set_palette(&mut self, palette: Palette) {
        self.palette = palette;
        self.current_fg = palette.foreground;
        self.current_bg = palette.background;
        for cell in &mut self.cells {
            if cell.uses_terminal_default_bg {
                cell.bg = palette.background;
            }
        }
    }

    fn resize(&mut self, cols: u16, rows: u16, cell_width: f32, cell_height: f32) {
        let old_cols = usize::from(self.cols);
        let old_rows = usize::from(self.rows);
        let new_cols = usize::from(cols);
        let new_rows = usize::from(rows);
        let mut next = vec![self.blank_cell(0, 0); new_cols.saturating_mul(new_rows)];

        for row in 0..old_rows.min(new_rows) {
            for col in 0..old_cols.min(new_cols) {
                let old_idx = row * old_cols + col;
                let new_idx = row * new_cols + col;
                next[new_idx] = self.cells[old_idx].clone();
            }
        }

        self.cols = cols;
        self.rows = rows;
        self.cell_width = cell_width;
        self.cell_height = cell_height;
        self.cells = next;
        self.cursor_col = self.cursor_col.min(new_cols.saturating_sub(1));
        self.cursor_row = self.cursor_row.min(new_rows.saturating_sub(1));
        self.reindex_cells();
        self.events.push(simple_event("resize"));
    }

    fn cell_count(&self) -> usize {
        usize::from(self.cols).saturating_mul(usize::from(self.rows))
    }

    fn blank_cell(&self, col: usize, row: usize) -> Cell {
        Cell {
            col,
            row,
            char: ' ',
            fg: self.palette.foreground,
            bg: self.palette.background,
            uses_terminal_default_bg: true,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            dim: false,
            reverse: false,
            blink: false,
            invisible: false,
            render_text: false,
            hyperlink_id: None,
            width: 1,
        }
    }

    fn reindex_cells(&mut self) {
        let cols = usize::from(self.cols);
        for (index, cell) in self.cells.iter_mut().enumerate() {
            cell.col = index % cols;
            cell.row = index / cols;
        }
    }

    fn put_char(&mut self, c: char) {
        if self.cols == 0 || self.rows == 0 {
            return;
        }

        let cols = usize::from(self.cols);
        // Unicode East Asian Width. Wide / Fullwidth glyphs report `Some(2)`,
        // narrow glyphs report `Some(1)`, and zero-width / combining marks
        // report `Some(0)` (we punt on those — see note below).
        let char_width = UnicodeWidthChar::width(c).unwrap_or(1).max(1) as u8;
        let is_wide = char_width == 2;

        if self.cursor_col >= cols {
            self.carriage_return();
            self.line_feed();
        }

        // Don't split a wide glyph across two rows. If only one column is
        // left, wrap first so the whole glyph lands on the next row.
        if is_wide && self.cursor_col + 1 >= cols {
            self.carriage_return();
            self.line_feed();
        }

        let index = self.cursor_row * cols + self.cursor_col;
        if let Some(cell) = self.cells.get_mut(index) {
            *cell = Cell {
                col: self.cursor_col,
                row: self.cursor_row,
                char: c,
                fg: self.current_fg,
                bg: self.current_bg,
                uses_terminal_default_bg: self.current_bg == self.palette.background,
                bold: self.current_bold,
                italic: self.current_italic,
                underline: self.current_underline,
                strikethrough: self.current_strikethrough,
                dim: self.current_dim,
                reverse: self.current_reverse,
                blink: self.current_blink,
                invisible: self.current_invisible,
                render_text: !c.is_control(),
                hyperlink_id: self.current_hyperlink,
                width: char_width,
            };
        }

        if is_wide {
            // Write a placeholder right-half cell that inherits the visual
            // attributes from the wide glyph. `render_text = false` and
            // `width = 0` tell the renderer to skip emitting a glyph for this
            // cell — it's already covered by the wide cell to its left.
            let placeholder_col = self.cursor_col + 1;
            let placeholder_index = self.cursor_row * cols + placeholder_col;
            if let Some(cell) = self.cells.get_mut(placeholder_index) {
                *cell = Cell {
                    col: placeholder_col,
                    row: self.cursor_row,
                    char: ' ',
                    fg: self.current_fg,
                    bg: self.current_bg,
                    uses_terminal_default_bg: self.current_bg == self.palette.background,
                    bold: self.current_bold,
                    italic: self.current_italic,
                    underline: self.current_underline,
                    strikethrough: self.current_strikethrough,
                    dim: self.current_dim,
                    reverse: self.current_reverse,
                    blink: self.current_blink,
                    invisible: self.current_invisible,
                    render_text: false,
                    hyperlink_id: self.current_hyperlink,
                    width: 0,
                };
            }
            self.cursor_col += 2;
        } else {
            self.cursor_col += 1;
        }
        self.events.push(simple_event("wakeup"));
    }

    fn line_feed(&mut self) {
        if self.cursor_row + 1 >= usize::from(self.rows) {
            self.scroll_up(1);
        } else {
            self.cursor_row += 1;
        }
    }

    fn carriage_return(&mut self) {
        self.cursor_col = 0;
    }

    fn backspace(&mut self) {
        self.cursor_col = self.cursor_col.saturating_sub(1);
    }

    fn tab(&mut self) {
        let next = ((self.cursor_col / 8) + 1) * 8;
        self.cursor_col = next.min(usize::from(self.cols).saturating_sub(1));
    }

    fn scroll_up(&mut self, count: usize) {
        let cols = usize::from(self.cols);
        let rows = usize::from(self.rows);
        if cols == 0 || rows == 0 {
            return;
        }

        for _ in 0..count.max(1) {
            let drained: Vec<Cell> = self.cells.drain(0..cols).collect();
            self.push_history_row(drained);
            for col in 0..cols {
                self.cells.push(self.blank_cell(col, rows - 1));
            }
        }
        self.cursor_row = rows.saturating_sub(1);
        self.display_offset = 0;
        self.reindex_cells();
    }

    fn push_history_row(&mut self, row: Vec<Cell>) {
        if self.scrollback_budget == 0 {
            return;
        }
        self.history.push_back(row);
        while self.history.len() > self.scrollback_budget {
            self.history.pop_front();
        }
    }

    fn clear_display(&mut self, mode: u16) {
        match mode {
            0 => {
                for row in self.cursor_row..usize::from(self.rows) {
                    let start_col = if row == self.cursor_row {
                        self.cursor_col
                    } else {
                        0
                    };
                    self.clear_cells(row, start_col, usize::from(self.cols));
                }
            }
            1 => {
                for row in 0..=self.cursor_row {
                    let end_col = if row == self.cursor_row {
                        self.cursor_col.saturating_add(1)
                    } else {
                        usize::from(self.cols)
                    };
                    self.clear_cells(row, 0, end_col);
                }
            }
            2 | 3 => {
                for row in 0..usize::from(self.rows) {
                    self.clear_cells(row, 0, usize::from(self.cols));
                }
            }
            _ => {}
        }
        self.events.push(simple_event("wakeup"));
    }

    fn clear_line(&mut self, mode: u16) {
        match mode {
            0 => self.clear_cells(self.cursor_row, self.cursor_col, usize::from(self.cols)),
            1 => self.clear_cells(self.cursor_row, 0, self.cursor_col.saturating_add(1)),
            2 => self.clear_cells(self.cursor_row, 0, usize::from(self.cols)),
            _ => {}
        }
        self.events.push(simple_event("wakeup"));
    }

    fn clear_cells(&mut self, row: usize, start_col: usize, end_col: usize) {
        let cols = usize::from(self.cols);
        for col in start_col..end_col.min(cols) {
            let idx = row.saturating_mul(cols).saturating_add(col);
            let blank = self.blank_cell(col, row);
            if let Some(cell) = self.cells.get_mut(idx) {
                *cell = blank;
            }
        }
    }

    fn move_cursor(&mut self, col: usize, row: usize) {
        self.cursor_col = col.min(usize::from(self.cols).saturating_sub(1));
        self.cursor_row = row.min(usize::from(self.rows).saturating_sub(1));
    }

    fn move_relative(&mut self, delta_col: isize, delta_row: isize) {
        let col = self.cursor_col.saturating_add_signed(delta_col);
        let row = self.cursor_row.saturating_add_signed(delta_row);
        self.move_cursor(col, row);
    }

    fn sgr(&mut self, params: &Params) {
        let values = params_to_vec(params);
        let values = if values.is_empty() { vec![0] } else { values };
        let mut index = 0;

        while index < values.len() {
            match values[index] {
                0 => {
                    self.current_fg = self.palette.foreground;
                    self.current_bg = self.palette.background;
                    self.current_bold = false;
                    self.current_italic = false;
                    self.current_underline = false;
                    self.current_strikethrough = false;
                    self.current_dim = false;
                    self.current_reverse = false;
                    self.current_blink = false;
                    self.current_invisible = false;
                }
                1 => self.current_bold = true,
                2 => self.current_dim = true,
                3 => self.current_italic = true,
                4 => self.current_underline = true,
                5 => self.current_blink = true,
                7 => self.current_reverse = true,
                8 => self.current_invisible = true,
                9 => self.current_strikethrough = true,
                // `22` cancels both bold and dim per the ECMA-48 spec.
                22 => {
                    self.current_bold = false;
                    self.current_dim = false;
                }
                23 => self.current_italic = false,
                24 => self.current_underline = false,
                25 => self.current_blink = false,
                27 => self.current_reverse = false,
                28 => self.current_invisible = false,
                29 => self.current_strikethrough = false,
                30..=37 => self.current_fg = self.palette.ansi[(values[index] - 30) as usize],
                39 => self.current_fg = self.palette.foreground,
                40..=47 => self.current_bg = self.palette.ansi[(values[index] - 40) as usize],
                49 => self.current_bg = self.palette.background,
                90..=97 => self.current_fg = self.palette.ansi[(values[index] - 82) as usize],
                100..=107 => self.current_bg = self.palette.ansi[(values[index] - 92) as usize],
                38 | 48 => {
                    if let Some((color, consumed)) =
                        extended_color(&values[index + 1..], self.palette)
                    {
                        if values[index] == 38 {
                            self.current_fg = color;
                        } else {
                            self.current_bg = color;
                        }
                        index += consumed;
                    }
                }
                _ => {}
            }
            index += 1;
        }
    }

    fn snapshot(&self) -> Frame {
        let cols = usize::from(self.cols);
        let rows = usize::from(self.rows);
        let history_len = self.history.len();
        let display_offset = self.display_offset.min(history_len);
        let history_overlay = display_offset.min(rows);
        let grid_visible = rows - history_overlay;

        let mut cells: Vec<Cell> = Vec::with_capacity(cols * rows);
        let history_start = history_len - display_offset;
        for i in 0..history_overlay {
            if let Some(history_row) = self.history.get(history_start + i) {
                for col in 0..cols {
                    let cell = history_row
                        .get(col)
                        .cloned()
                        .unwrap_or_else(|| self.blank_cell(col, i));
                    cells.push(Cell {
                        col,
                        row: i,
                        ..cell
                    });
                }
            } else {
                for col in 0..cols {
                    cells.push(self.blank_cell(col, i));
                }
            }
        }
        for row in 0..grid_visible {
            let viewport_row = history_overlay + row;
            for col in 0..cols {
                let idx = row * cols + col;
                if let Some(cell) = self.cells.get(idx) {
                    cells.push(Cell {
                        col,
                        row: viewport_row,
                        ..cell.clone()
                    });
                } else {
                    cells.push(self.blank_cell(col, viewport_row));
                }
            }
        }

        let cursor_viewport_row = history_overlay + self.cursor_row;
        let cursor = if cursor_viewport_row < rows {
            Some(Cursor {
                col: self.cursor_col,
                row: cursor_viewport_row,
                style: "block",
            })
        } else {
            None
        };

        Frame {
            cols: self.cols,
            rows: self.rows,
            cells,
            cursor,
            display_offset: self.display_offset,
            history_size: history_len,
            application_cursor_keys: self.application_cursor_keys,
            mouse_mode: self.mouse_mode.as_str(),
            mouse_encoding: self.mouse_encoding.as_str(),
            bracketed_paste: self.bracketed_paste,
            hyperlinks: self.hyperlinks.clone(),
        }
    }

    fn search(&self, query: &str) -> Vec<SearchMatch> {
        if query.is_empty() || self.cols == 0 {
            return Vec::new();
        }

        let query = query.to_ascii_lowercase();
        let query_len = query.chars().count();
        let cols = usize::from(self.cols);
        let mut matches = Vec::new();

        for row in 0..usize::from(self.rows) {
            let line = self.line_text(row);
            let searchable = line.to_ascii_lowercase();
            let mut offset = 0;

            while let Some(byte_index) = searchable[offset..].find(&query) {
                let start_byte = offset + byte_index;
                let start_col = searchable[..start_byte].chars().count();
                let end_col = start_col + query_len.saturating_sub(1);
                matches.push(SearchMatch {
                    row,
                    start_col,
                    end_col,
                    line: line.clone(),
                });
                offset = start_byte + query.len();
            }

            if cols == 0 {
                break;
            }
        }

        matches
    }

    fn line_text(&self, row: usize) -> String {
        let cols = usize::from(self.cols);
        let start = row.saturating_mul(cols);
        let end = start.saturating_add(cols);
        if end > self.cells.len() {
            return String::new();
        }

        self.cells[start..end]
            .iter()
            .map(|cell| if cell.render_text { cell.char } else { ' ' })
            .collect::<String>()
            .trim_end()
            .to_string()
    }
}

impl Perform for TerminalScreen {
    fn print(&mut self, c: char) {
        self.put_char(c);
    }

    fn execute(&mut self, byte: u8) {
        match byte {
            b'\n' | 0x0b | 0x0c => self.line_feed(),
            b'\r' => self.carriage_return(),
            0x08 => self.backspace(),
            b'\t' => self.tab(),
            0x07 => self.events.push(simple_event("bell")),
            _ => {}
        }
    }

    fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
        let Some(kind) = params
            .first()
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
        else {
            return;
        };
        let payload = params
            .get(1)
            .and_then(|bytes| std::str::from_utf8(bytes).ok())
            .unwrap_or_default()
            .to_string();

        match kind {
            "0" | "2" => {
                self.title = Some(payload.clone());
                self.events.push(payload_event("title", payload));
            }
            "7" => self
                .events
                .push(payload_event("working-directory", payload)),
            "9" if payload.starts_with("4;") => {
                self.events.push(payload_event("progress", payload));
            }
            "52" => self.events.push(payload_event("clipboard-store", payload)),
            "8" => {
                // OSC8 hyperlink: \x1b]8;<params>;<uri>\x1b\\
                // params[1] is the OSC params string (e.g. "id=foo"); we
                // currently ignore those and key off the URI.
                let uri = params
                    .get(2)
                    .and_then(|bytes| std::str::from_utf8(bytes).ok())
                    .unwrap_or_default();
                if uri.is_empty() {
                    self.current_hyperlink = None;
                } else {
                    let id = self.intern_hyperlink(uri);
                    self.current_hyperlink = Some(id);
                }
            }
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let values = params_to_vec(params);
        let first = values.first().copied().unwrap_or(0);
        let amount = usize::from(if first == 0 { 1 } else { first });

        if intermediates == b"?" && (action == 'h' || action == 'l') {
            let enable = action == 'h';
            for value in &values {
                match *value {
                    1 => self.application_cursor_keys = enable,
                    9 => self.set_mouse_mode(if enable {
                        MouseMode::X10
                    } else {
                        MouseMode::None
                    }),
                    1000 => self.set_mouse_mode(if enable {
                        MouseMode::Normal
                    } else {
                        MouseMode::None
                    }),
                    1002 => self.set_mouse_mode(if enable {
                        MouseMode::ButtonEvent
                    } else {
                        MouseMode::None
                    }),
                    1003 => self.set_mouse_mode(if enable {
                        MouseMode::AnyEvent
                    } else {
                        MouseMode::None
                    }),
                    1005 => {
                        // UTF-8 extended coordinates.
                        self.mouse_encoding = if enable {
                            MouseEncoding::Utf8
                        } else {
                            MouseEncoding::Legacy
                        };
                    }
                    1006 => {
                        // SGR encoding.
                        self.mouse_encoding = if enable {
                            MouseEncoding::Sgr
                        } else {
                            MouseEncoding::Legacy
                        };
                    }
                    1015 => {
                        // urxvt encoding — treat as SGR fallback.
                        self.mouse_encoding = if enable {
                            MouseEncoding::Sgr
                        } else {
                            MouseEncoding::Legacy
                        };
                    }
                    1016 => {
                        // SGR pixel-precision encoding. Treat same as SGR for now,
                        // but expose distinct enum so JS can recognize it.
                        self.mouse_encoding = if enable {
                            MouseEncoding::SgrPixel
                        } else {
                            MouseEncoding::Legacy
                        };
                    }
                    2004 => self.bracketed_paste = enable,
                    _ => {}
                }
            }
            return;
        }

        match action {
            'A' => self.move_relative(0, -(amount as isize)),
            'B' => self.move_relative(0, amount as isize),
            'C' => self.move_relative(amount as isize, 0),
            'D' => self.move_relative(-(amount as isize), 0),
            'G' => self.move_cursor(usize::from(first.saturating_sub(1)), self.cursor_row),
            'H' | 'f' => {
                let row = values.first().copied().unwrap_or(1).saturating_sub(1);
                let col = values.get(1).copied().unwrap_or(1).saturating_sub(1);
                self.move_cursor(usize::from(col), usize::from(row));
            }
            'J' => self.clear_display(first),
            'K' => self.clear_line(first),
            'm' => self.sgr(params),
            'n' if first == 6 => {
                self.responses.push(format!(
                    "\x1b[{};{}R",
                    self.cursor_row + 1,
                    self.cursor_col + 1
                ));
            }
            'c' if intermediates.is_empty() => self.responses.push("\x1b[?1;2c".to_string()),
            _ => {}
        }
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], _ignore: bool, byte: u8) {
        match (intermediates, byte) {
            ([], b'D') => self.line_feed(),
            ([], b'E') => {
                self.line_feed();
                self.carriage_return();
            }
            ([], b'M') => {
                self.cursor_row = self.cursor_row.saturating_sub(1);
            }
            ([], b'c') => {
                let cols = self.cols;
                let rows = self.rows;
                let cell_width = self.cell_width;
                let cell_height = self.cell_height;
                *self = Self::new(cols, rows, cell_width, cell_height);
            }
            _ => {}
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalColor {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

#[derive(Clone, Copy)]
struct Palette {
    ansi: [TerminalColor; 16],
    foreground: TerminalColor,
    background: TerminalColor,
    cursor: TerminalColor,
}

impl Palette {
    fn from_render_config(config: &RenderConfig) -> Self {
        Self {
            ansi: config.ansi,
            foreground: config.foreground,
            background: config.background,
            cursor: config.cursor,
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            ansi: [
                color(0x00, 0x00, 0x00),
                color(0xcd, 0x00, 0x00),
                color(0x00, 0xcd, 0x00),
                color(0xcd, 0xcd, 0x00),
                color(0x00, 0x00, 0xee),
                color(0xcd, 0x00, 0xcd),
                color(0x00, 0xcd, 0xcd),
                color(0xe5, 0xe5, 0xe5),
                color(0x7f, 0x7f, 0x7f),
                color(0xff, 0x00, 0x00),
                color(0x00, 0xff, 0x00),
                color(0xff, 0xff, 0x00),
                color(0x5c, 0x5c, 0xff),
                color(0xff, 0x00, 0xff),
                color(0x00, 0xff, 0xff),
                color(0xff, 0xff, 0xff),
            ],
            foreground: color(0xe5, 0xe5, 0xe5),
            background: color(0x1e, 0x1e, 0x1e),
            cursor: color(0xe5, 0xe5, 0xe5),
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Cell {
    col: usize,
    row: usize,
    char: char,
    fg: TerminalColor,
    bg: TerminalColor,
    uses_terminal_default_bg: bool,
    bold: bool,
    italic: bool,
    underline: bool,
    strikethrough: bool,
    dim: bool,
    /// Reverse video. Stored on the cell as a flag; the painter is responsible
    /// for swapping `fg` <-> `bg` at draw time so the cell still carries the
    /// "logical" colors set by the producer.
    reverse: bool,
    blink: bool,
    invisible: bool,
    render_text: bool,
    hyperlink_id: Option<u32>,
    /// Column width occupied by this cell.
    ///
    /// * `1` — normal narrow cell (default for blanks and ASCII).
    /// * `2` — wide cell (CJK ideograph, emoji, fullwidth punctuation). The
    ///   glyph is painted spanning both this column and the next one.
    /// * `0` — placeholder representing the right half of a wide glyph. The
    ///   renderer should skip emitting a glyph for this cell; the wide cell
    ///   to its left already covers it.
    width: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Cursor {
    col: usize,
    row: usize,
    style: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Frame {
    cols: u16,
    rows: u16,
    cells: Vec<Cell>,
    cursor: Option<Cursor>,
    display_offset: usize,
    history_size: usize,
    application_cursor_keys: bool,
    mouse_mode: &'static str,
    mouse_encoding: &'static str,
    bracketed_paste: bool,
    hyperlinks: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchMatch {
    row: usize,
    start_col: usize,
    end_col: usize,
    line: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FeedResult {
    events: Vec<BrowserEvent>,
    responses: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct BrowserEvent {
    kind: &'static str,
    payload: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RenderConfig {
    active_theme: String,
    font_family: String,
    font_size: f32,
    line_height: f32,
    padding_x: f32,
    padding_y: f32,
    background_opacity: f32,
    background_opacity_cells: bool,
    cursor_blink: bool,
    cursor_style: &'static str,
    foreground: TerminalColor,
    background: TerminalColor,
    cursor: TerminalColor,
    ansi: [TerminalColor; 16],
    diagnostics: Vec<SerializableConfigDiagnostic>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SerializableConfigDiagnostic {
    line_number: usize,
    kind: &'static str,
    message: String,
}

fn render_config_from_report(
    report: termy_config_core::ConfigParseReport,
    system_appearance: SystemAppearance,
) -> RenderConfig {
    let colors = resolve_theme_colors(&report.config, system_appearance);
    RenderConfig {
        active_theme: colors.active_theme,
        font_family: report.config.font_family,
        font_size: report.config.font_size,
        line_height: report.config.line_height,
        padding_x: report.config.padding_x,
        padding_y: report.config.padding_y,
        background_opacity: report.config.background_opacity,
        background_opacity_cells: report.config.background_opacity_cells,
        cursor_blink: report.config.cursor_blink,
        cursor_style: match report.config.cursor_style {
            CursorStyle::Line => "line",
            CursorStyle::Block => "block",
        },
        foreground: colors.foreground,
        background: colors.background,
        cursor: colors.cursor,
        ansi: colors.ansi,
        diagnostics: report
            .diagnostics
            .into_iter()
            .map(config_diagnostic)
            .collect(),
    }
}

struct ResolvedColors {
    active_theme: String,
    ansi: [TerminalColor; 16],
    foreground: TerminalColor,
    background: TerminalColor,
    cursor: TerminalColor,
}

fn resolve_theme_colors(config: &AppConfig, system_appearance: SystemAppearance) -> ResolvedColors {
    let active_theme = resolve_active_theme(config, system_appearance).to_string();
    let mut colors = if active_theme.eq_ignore_ascii_case(SHELL_DECIDE_THEME_ID) {
        terminal_default_theme_colors()
    } else {
        builtin_theme_colors(&active_theme).unwrap_or_else(termy_themes::termy)
    };
    apply_custom_colors(&mut colors, config);

    ResolvedColors {
        active_theme,
        ansi: colors.ansi.map(color_from_theme_rgb),
        foreground: color_from_theme_rgb(colors.foreground),
        background: color_from_theme_rgb(colors.background),
        cursor: color_from_theme_rgb(colors.cursor),
    }
}

fn builtin_theme_colors(theme_id: &str) -> Option<ThemeColors> {
    match normalize_theme_id(theme_id).as_str() {
        "termy" => Some(termy_themes::termy()),
        "tokyo-night" | "tokyonight" => Some(termy_themes::tokyo_night()),
        "catppuccin-mocha" | "catppuccin" | "catppuccinmocha" => {
            Some(termy_themes::catppuccin_mocha())
        }
        "dracula" => Some(termy_themes::dracula()),
        "gruvbox-dark" | "gruvbox" | "gruvboxdark" => Some(termy_themes::gruvbox_dark()),
        "nord" => Some(termy_themes::nord()),
        "solarized-dark" | "solarized" | "solarizeddark" => Some(termy_themes::solarized_dark()),
        "one-dark" | "one" | "onedark" => Some(termy_themes::one_dark()),
        "monokai" => Some(termy_themes::monokai()),
        "material-dark" | "material" | "materialdark" => Some(termy_themes::material_dark()),
        "palenight" => Some(termy_themes::palenight()),
        "tomorrow-night" | "tomorrow" | "tomorrownight" => Some(termy_themes::tomorrow_night()),
        "oceanic-next" | "oceanic" | "oceanicnext" => Some(termy_themes::oceanic_next()),
        _ => termy_themes::resolve_theme(theme_id),
    }
}

fn apply_custom_colors(colors: &mut ThemeColors, config: &AppConfig) {
    if let Some(color) = config.colors.foreground {
        colors.foreground = theme_rgb_from_config_rgb(color);
    }
    if let Some(color) = config.colors.background {
        colors.background = theme_rgb_from_config_rgb(color);
    }
    if let Some(color) = config.colors.cursor {
        colors.cursor = theme_rgb_from_config_rgb(color);
    }
    for (index, color) in config.colors.ansi.iter().enumerate() {
        if let Some(color) = color {
            colors.ansi[index] = theme_rgb_from_config_rgb(*color);
        }
    }
}

fn terminal_default_theme_colors() -> ThemeColors {
    let defaults = Palette::default();
    ThemeColors {
        ansi: defaults.ansi.map(theme_rgb_from_color),
        foreground: theme_rgb_from_color(defaults.foreground),
        background: theme_rgb_from_color(defaults.background),
        cursor: theme_rgb_from_color(defaults.cursor),
    }
}

fn params_to_vec(params: &Params) -> Vec<u16> {
    params.iter().map(|param| param[0]).collect()
}

fn extended_color(values: &[u16], palette: Palette) -> Option<(TerminalColor, usize)> {
    match values {
        [2, r, g, b, ..] => Some((color(*r as u8, *g as u8, *b as u8), 4)),
        [5, index, ..] => Some((indexed_color(*index, palette), 2)),
        _ => None,
    }
}

fn indexed_color(index: u16, palette: Palette) -> TerminalColor {
    match index {
        0..=15 => palette.ansi[index as usize],
        16..=231 => {
            let idx = index as u8 - 16;
            let r = (idx / 36) % 6;
            let g = (idx / 6) % 6;
            let b = idx % 6;
            let to_component = |value: u8| if value == 0 { 0 } else { 55 + (value * 40) };
            color(to_component(r), to_component(g), to_component(b))
        }
        232..=255 => {
            let gray = 8 + ((index as u8 - 232) * 10);
            color(gray, gray, gray)
        }
        _ => palette.foreground,
    }
}

fn config_diagnostic(diagnostic: ConfigDiagnostic) -> SerializableConfigDiagnostic {
    SerializableConfigDiagnostic {
        line_number: diagnostic.line_number,
        kind: match diagnostic.kind {
            ConfigDiagnosticKind::UnknownSection => "unknown-section",
            ConfigDiagnosticKind::UnknownRootKey => "unknown-root-key",
            ConfigDiagnosticKind::UnknownColorKey => "unknown-color-key",
            ConfigDiagnosticKind::InvalidSyntax => "invalid-syntax",
            ConfigDiagnosticKind::InvalidValue => "invalid-value",
            ConfigDiagnosticKind::DuplicateRootKey => "duplicate-root-key",
        },
        message: diagnostic.message,
    }
}

fn simple_event(kind: &'static str) -> BrowserEvent {
    BrowserEvent {
        kind,
        payload: None,
    }
}

fn payload_event(kind: &'static str, payload: String) -> BrowserEvent {
    BrowserEvent {
        kind,
        payload: Some(payload),
    }
}

fn color(r: u8, g: u8, b: u8) -> TerminalColor {
    TerminalColor { r, g, b, a: 255 }
}

fn color_from_theme_rgb(color: termy_themes::Rgb8) -> TerminalColor {
    self::color(color.r, color.g, color.b)
}

fn theme_rgb_from_color(color: TerminalColor) -> termy_themes::Rgb8 {
    termy_themes::Rgb8::new(color.r, color.g, color.b)
}

fn theme_rgb_from_config_rgb(color: termy_config_core::Rgb8) -> termy_themes::Rgb8 {
    termy_themes::Rgb8::new(color.r, color.g, color.b)
}

fn to_js<T: Serialize + ?Sized>(value: &T) -> Result<JsValue, JsValue> {
    serde_wasm_bindgen::to_value(value).map_err(|error| JsValue::from_str(&error.to_string()))
}

/// Whether the active mouse mode permits reporting this kind of event.
fn event_allowed(mode: MouseMode, kind: MouseEventKind) -> bool {
    match mode {
        MouseMode::None => false,
        // X10: only press events (no release, no motion).
        MouseMode::X10 => matches!(kind, MouseEventKind::Press),
        // Normal: press + release, no motion.
        MouseMode::Normal => matches!(kind, MouseEventKind::Press | MouseEventKind::Release),
        // ButtonEvent: press + release + drag (motion while a button is held).
        MouseMode::ButtonEvent => matches!(
            kind,
            MouseEventKind::Press | MouseEventKind::Release | MouseEventKind::Drag
        ),
        // AnyEvent: everything, including idle motion.
        MouseMode::AnyEvent => true,
    }
}

/// Build the wire bytes for the given mouse event using the active encoding.
///
/// `button` is the protocol button number (0/1/2 for L/M/R, 64+ for wheel/etc),
/// `modifiers` is the xterm modifier bitmask (4=shift, 8=alt, 16=ctrl).
fn encode_mouse_packet(
    _mode: MouseMode,
    encoding: MouseEncoding,
    button: u8,
    modifiers: u8,
    col: u16,
    row: u16,
    kind: MouseEventKind,
) -> Option<Vec<u8>> {
    // Drag/motion sets the "motion bit" (32) in the button byte.
    let motion_bit = match kind {
        MouseEventKind::Drag | MouseEventKind::Move => 32u16,
        _ => 0,
    };

    match encoding {
        MouseEncoding::Sgr | MouseEncoding::SgrPixel => {
            // SGR: button keeps its original number on release; suffix `m` for release.
            let encoded_button = u16::from(button)
                .saturating_add(motion_bit)
                .saturating_add(u16::from(modifiers));
            let suffix = if matches!(kind, MouseEventKind::Release) {
                'm'
            } else {
                'M'
            };
            // Coordinates are 1-based.
            let col_value = u32::from(col).saturating_add(1);
            let row_value = u32::from(row).saturating_add(1);
            Some(format!("\x1b[<{encoded_button};{col_value};{row_value}{suffix}").into_bytes())
        }
        MouseEncoding::Legacy | MouseEncoding::Utf8 => {
            // X10 / legacy / UTF-8: release events collapse to button code 3.
            let base_button = if matches!(kind, MouseEventKind::Release) {
                3u16
            } else {
                u16::from(button)
            };
            let encoded_button = base_button
                .saturating_add(motion_bit)
                .saturating_add(u16::from(modifiers));

            // Encode coordinates. X10/legacy is single byte (32 + 1 + value, max 223).
            // UTF-8 mode uses 2-byte UTF-8 sequences for values >= 95.
            let utf8 = matches!(encoding, MouseEncoding::Utf8);
            let max_point: u32 = if utf8 { 2015 } else { 223 };
            if u32::from(col) >= max_point || u32::from(row) >= max_point {
                return None;
            }

            let mut packet = Vec::with_capacity(6);
            packet.extend_from_slice(b"\x1b[M");
            let button_byte = (32u16 + encoded_button).min(255) as u8;
            packet.push(button_byte);
            push_coordinate(&mut packet, col, utf8);
            push_coordinate(&mut packet, row, utf8);
            Some(packet)
        }
    }
}

fn push_coordinate(packet: &mut Vec<u8>, value: u16, utf8: bool) {
    let pos = u32::from(value);
    let encoded = 32 + 1 + pos;
    if utf8 && encoded >= 0x80 {
        let first = 0xC0 + encoded / 64;
        let second = 0x80 + (encoded & 0x3F);
        packet.push(first as u8);
        packet.push(second as u8);
    } else {
        packet.push(encoded.min(255) as u8);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_output_into_snapshot() {
        let mut terminal = TermyTerminal::new(8, 2);
        terminal.parser.advance(&mut terminal.screen, b"hello");
        let frame = terminal.screen.snapshot();

        assert_eq!(terminal.screen.line_text(0), "hello");
        assert_eq!(frame.cells[0].char, 'h');
    }

    #[test]
    fn resolves_custom_render_config() {
        let report = AppConfig::from_contents_with_report(
            "theme = nord\nfont_size = 18\n[colors]\nbackground = #010203\n",
        );
        let config = render_config_from_report(report, SystemAppearance::Dark);

        assert_eq!(config.active_theme, "nord");
        assert_eq!(config.font_size, 18.0);
        assert_eq!(config.background.r, 1);
        assert_eq!(config.background.g, 2);
        assert_eq!(config.background.b, 3);
    }

    #[test]
    fn search_matches_visible_rows() {
        let mut terminal = TermyTerminal::new(16, 2);
        terminal
            .parser
            .advance(&mut terminal.screen, b"alpha beta\r\nbeta");
        let matches = terminal.screen.search("beta");

        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].start_col, 6);
    }

    #[test]
    fn sgr_colors_cells() {
        let mut terminal = TermyTerminal::new(8, 1);
        terminal.parser.advance(&mut terminal.screen, b"\x1b[31mR");
        let frame = terminal.screen.snapshot();

        assert_eq!(frame.cells[0].fg, terminal.screen.palette.ansi[1]);
    }

    #[test]
    fn sgr_italic_toggle() {
        let mut terminal = TermyTerminal::new(8, 1);
        // ESC[3m enables italic, ESC[23m disables it.
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[3mI\x1b[23mN");
        let frame = terminal.screen.snapshot();
        assert!(frame.cells[0].italic, "ESC[3m should enable italic");
        assert!(!frame.cells[1].italic, "ESC[23m should disable italic");
    }

    #[test]
    fn sgr_underline_toggle() {
        let mut terminal = TermyTerminal::new(8, 1);
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[4mU\x1b[24mN");
        let frame = terminal.screen.snapshot();
        assert!(frame.cells[0].underline, "ESC[4m should enable underline");
        assert!(
            !frame.cells[1].underline,
            "ESC[24m should disable underline"
        );
    }

    #[test]
    fn sgr_reset_clears_all_attributes() {
        let mut terminal = TermyTerminal::new(8, 1);
        // Enable everything and verify the next cell still picks up the
        // attributes, then ESC[0m and verify it all clears.
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[1;2;3;4;5;7;8;9mA\x1b[0mB");
        let frame = terminal.screen.snapshot();
        let a = &frame.cells[0];
        assert!(a.bold);
        assert!(a.dim);
        assert!(a.italic);
        assert!(a.underline);
        assert!(a.blink);
        assert!(a.reverse);
        assert!(a.invisible);
        assert!(a.strikethrough);

        let b = &frame.cells[1];
        assert!(!b.bold);
        assert!(!b.dim);
        assert!(!b.italic);
        assert!(!b.underline);
        assert!(!b.blink);
        assert!(!b.reverse);
        assert!(!b.invisible);
        assert!(!b.strikethrough);
    }

    #[test]
    fn sgr_stacks_bold_italic_underline() {
        let mut terminal = TermyTerminal::new(8, 1);
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[1;3;4mX");
        let frame = terminal.screen.snapshot();
        let cell = &frame.cells[0];
        assert!(cell.bold);
        assert!(cell.italic);
        assert!(cell.underline);
    }

    #[test]
    fn sgr_22_clears_bold_and_dim() {
        let mut terminal = TermyTerminal::new(8, 1);
        // Per ECMA-48, SGR 22 cancels both bold and dim.
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[1;2mA\x1b[22mB");
        let frame = terminal.screen.snapshot();
        assert!(frame.cells[0].bold);
        assert!(frame.cells[0].dim);
        assert!(!frame.cells[1].bold);
        assert!(!frame.cells[1].dim);
    }

    #[test]
    fn sgr_strikethrough_and_reverse_toggle() {
        let mut terminal = TermyTerminal::new(8, 1);
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[9;7mA\x1b[29;27mB");
        let frame = terminal.screen.snapshot();
        assert!(frame.cells[0].strikethrough);
        assert!(frame.cells[0].reverse);
        assert!(!frame.cells[1].strikethrough);
        assert!(!frame.cells[1].reverse);
    }

    #[test]
    fn mouse_mode_starts_disabled() {
        let terminal = TermyTerminal::new(80, 24);
        assert_eq!(terminal.screen.mouse_mode, MouseMode::None);
        assert_eq!(terminal.screen.mouse_encoding, MouseEncoding::Legacy);
        let frame = terminal.screen.snapshot();
        assert_eq!(frame.mouse_mode, "none");
        assert_eq!(frame.mouse_encoding, "legacy");
    }

    #[test]
    fn enables_normal_tracking_and_sgr_encoding() {
        let mut terminal = TermyTerminal::new(80, 24);
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?1000h\x1b[?1006h");
        assert_eq!(terminal.screen.mouse_mode, MouseMode::Normal);
        assert_eq!(terminal.screen.mouse_encoding, MouseEncoding::Sgr);

        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?1000l");
        assert_eq!(terminal.screen.mouse_mode, MouseMode::None);
        // Disabling mode resets the encoding to legacy.
        assert_eq!(terminal.screen.mouse_encoding, MouseEncoding::Legacy);
    }

    #[test]
    fn enables_button_event_and_any_event_modes() {
        let mut terminal = TermyTerminal::new(80, 24);
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?1002h");
        assert_eq!(terminal.screen.mouse_mode, MouseMode::ButtonEvent);
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?1003h");
        assert_eq!(terminal.screen.mouse_mode, MouseMode::AnyEvent);
    }

    #[test]
    fn x10_left_click_encodes_legacy_packet() {
        let mut terminal = TermyTerminal::new(80, 24);
        terminal.parser.advance(&mut terminal.screen, b"\x1b[?9h");
        let bytes = terminal
            .screen
            .encode_mouse_report(0, 0, 4, 2, 0)
            .expect("packet");
        // 0x1b [ M  <button=32>  <col=33+4=37>  <row=33+2=35>
        assert_eq!(bytes, vec![0x1b, b'[', b'M', 32, 37, 35]);
    }

    #[test]
    fn x10_mode_ignores_release_events() {
        let mut terminal = TermyTerminal::new(80, 24);
        terminal.parser.advance(&mut terminal.screen, b"\x1b[?9h");
        // Release events are not allowed in X10 mode.
        assert!(terminal.screen.encode_mouse_report(0, 0, 4, 2, 1).is_none());
        // Drag and motion are not allowed either.
        assert!(terminal.screen.encode_mouse_report(0, 0, 4, 2, 2).is_none());
        assert!(terminal.screen.encode_mouse_report(0, 0, 4, 2, 3).is_none());
    }

    #[test]
    fn sgr_press_and_release_packets() {
        let mut terminal = TermyTerminal::new(80, 24);
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?1000h\x1b[?1006h");

        let press = terminal
            .screen
            .encode_mouse_report(0, 0, 4, 2, 0)
            .expect("press packet");
        assert_eq!(press, b"\x1b[<0;5;3M");

        let release = terminal
            .screen
            .encode_mouse_report(2, 0, 1, 1, 1)
            .expect("release packet");
        // Release in SGR keeps original button + lowercase suffix.
        assert_eq!(release, b"\x1b[<2;2;2m");
    }

    #[test]
    fn button_event_drag_sets_motion_bit() {
        let mut terminal = TermyTerminal::new(80, 24);
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?1002h\x1b[?1006h");

        let bytes = terminal
            .screen
            .encode_mouse_report(0, 0, 4, 2, 2)
            .expect("drag packet");
        // SGR drag: button 0 + motion bit 32 = 32.
        assert_eq!(bytes, b"\x1b[<32;5;3M");
    }

    #[test]
    fn any_event_motion_is_allowed_but_button_event_rejects_motion() {
        let mut terminal = TermyTerminal::new(80, 24);
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?1002h\x1b[?1006h");
        // ButtonEvent mode should NOT emit bare motion (kind = Move).
        assert!(terminal.screen.encode_mouse_report(0, 0, 4, 2, 3).is_none());

        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?1003h");
        // AnyEvent mode allows bare motion.
        let bytes = terminal
            .screen
            .encode_mouse_report(0, 0, 4, 2, 3)
            .expect("move packet");
        assert_eq!(bytes, b"\x1b[<32;5;3M");
    }

    #[test]
    fn mode_disable_suppresses_events() {
        let mut terminal = TermyTerminal::new(80, 24);
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?1000h\x1b[?1006h\x1b[?1000l");
        assert!(terminal.screen.encode_mouse_report(0, 0, 4, 2, 0).is_none());
    }

    #[test]
    fn ignores_events_outside_active_mode() {
        let mut terminal = TermyTerminal::new(80, 24);
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?1000h");
        // Normal tracking does not report drag.
        assert!(terminal.screen.encode_mouse_report(0, 0, 4, 2, 2).is_none());
        // ...nor bare motion.
        assert!(terminal.screen.encode_mouse_report(0, 0, 4, 2, 3).is_none());
        // But press/release work.
        assert!(terminal.screen.encode_mouse_report(0, 0, 4, 2, 0).is_some());
        assert!(terminal.screen.encode_mouse_report(0, 0, 4, 2, 1).is_some());
    }

    #[test]
    fn snapshot_exposes_mouse_state() {
        let mut terminal = TermyTerminal::new(80, 24);
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?1002h\x1b[?1006h");
        let frame = terminal.screen.snapshot();
        assert_eq!(frame.mouse_mode, "button-event");
        assert_eq!(frame.mouse_encoding, "sgr");
    }

    #[test]
    fn bracketed_paste_defaults_to_disabled() {
        let terminal = TermyTerminal::new(80, 24);
        assert!(!terminal.screen.bracketed_paste);
        let frame = terminal.screen.snapshot();
        assert!(!frame.bracketed_paste);
    }

    #[test]
    fn bracketed_paste_enable_and_disable() {
        let mut terminal = TermyTerminal::new(80, 24);
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?2004h");
        assert!(terminal.screen.bracketed_paste);
        assert!(terminal.screen.snapshot().bracketed_paste);

        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?2004l");
        assert!(!terminal.screen.bracketed_paste);
    }

    #[test]
    fn bracketed_paste_persists_across_feeds() {
        let mut terminal = TermyTerminal::new(80, 24);
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?2004h");
        assert!(terminal.screen.bracketed_paste);
        terminal.parser.advance(&mut terminal.screen, b"some text");
        assert!(terminal.screen.bracketed_paste);
    }

    #[test]
    fn bracketed_paste_cleared_by_ris() {
        let mut terminal = TermyTerminal::new(80, 24);
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?2004h");
        assert!(terminal.screen.bracketed_paste);
        // RIS (\x1bc) — full reset.
        terminal.parser.advance(&mut terminal.screen, b"\x1bc");
        assert!(!terminal.screen.bracketed_paste);
    }

    #[test]
    fn osc8_attaches_hyperlink_to_printed_cells() {
        let mut terminal = TermyTerminal::new(40, 2);
        terminal.parser.advance(
            &mut terminal.screen,
            b"\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\",
        );
        let frame = terminal.screen.snapshot();
        assert_eq!(frame.hyperlinks, vec!["https://example.com".to_string()]);
        // 'l', 'i', 'n', 'k' should all have hyperlink_id Some(0).
        for col in 0..4 {
            assert_eq!(
                frame.cells[col].hyperlink_id,
                Some(0),
                "col {} expected hyperlink",
                col
            );
        }
        // Next cell after the closer should have no hyperlink.
        assert_eq!(frame.cells[4].hyperlink_id, None);
    }

    #[test]
    fn osc8_multiple_links_per_line() {
        let mut terminal = TermyTerminal::new(40, 1);
        terminal.parser.advance(
            &mut terminal.screen,
            b"\x1b]8;;https://a.com\x1b\\AA\x1b]8;;\x1b\\ \x1b]8;;https://b.com\x1b\\BB\x1b]8;;\x1b\\",
        );
        let frame = terminal.screen.snapshot();
        assert_eq!(
            frame.hyperlinks,
            vec!["https://a.com".to_string(), "https://b.com".to_string()]
        );
        assert_eq!(frame.cells[0].hyperlink_id, Some(0));
        assert_eq!(frame.cells[1].hyperlink_id, Some(0));
        assert_eq!(frame.cells[2].hyperlink_id, None); // space
        assert_eq!(frame.cells[3].hyperlink_id, Some(1));
        assert_eq!(frame.cells[4].hyperlink_id, Some(1));
    }

    #[test]
    fn osc8_same_uri_shares_id_across_runs() {
        let mut terminal = TermyTerminal::new(40, 1);
        terminal.parser.advance(
            &mut terminal.screen,
            b"\x1b]8;id=one;https://example.com\x1b\\A\x1b]8;;\x1b\\ \x1b]8;id=two;https://example.com\x1b\\B\x1b]8;;\x1b\\",
        );
        let frame = terminal.screen.snapshot();
        assert_eq!(frame.hyperlinks.len(), 1);
        assert_eq!(frame.cells[0].hyperlink_id, Some(0));
        assert_eq!(frame.cells[2].hyperlink_id, Some(0));
    }

    #[test]
    fn osc8_link_spans_multiple_cells_with_same_id() {
        let mut terminal = TermyTerminal::new(40, 1);
        terminal.parser.advance(
            &mut terminal.screen,
            b"\x1b]8;;https://example.com\x1b\\Hello World\x1b]8;;\x1b\\",
        );
        let frame = terminal.screen.snapshot();
        for col in 0..11 {
            assert_eq!(
                frame.cells[col].hyperlink_id,
                Some(0),
                "col {} expected hyperlink",
                col
            );
        }
    }

    #[test]
    fn hyperlink_at_returns_uri() {
        let mut terminal = TermyTerminal::new(40, 2);
        terminal.parser.advance(
            &mut terminal.screen,
            b"\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\",
        );
        assert_eq!(
            terminal.screen.hyperlink_at(0, 0).as_deref(),
            Some("https://example.com"),
        );
        assert_eq!(terminal.screen.hyperlink_at(0, 4), None);
    }

    #[test]
    fn ascii_cells_have_width_one() {
        let mut terminal = TermyTerminal::new(8, 1);
        terminal.parser.advance(&mut terminal.screen, b"abc");
        let frame = terminal.screen.snapshot();
        assert_eq!(frame.cells[0].width, 1);
        assert_eq!(frame.cells[1].width, 1);
        assert_eq!(frame.cells[2].width, 1);
        // Blanks are also width 1.
        assert_eq!(frame.cells[3].width, 1);
    }

    #[test]
    fn cjk_ideograph_occupies_two_cells() {
        let mut terminal = TermyTerminal::new(8, 1);
        // U+3042 HIRAGANA LETTER A — East Asian Width: Wide.
        let mut bytes = String::new();
        bytes.push('あ');
        terminal
            .parser
            .advance(&mut terminal.screen, bytes.as_bytes());
        let frame = terminal.screen.snapshot();
        assert_eq!(frame.cells[0].char, 'あ');
        assert_eq!(frame.cells[0].width, 2);
        // Right-half placeholder.
        assert_eq!(frame.cells[1].width, 0);
        assert!(!frame.cells[1].render_text);
        // Cursor advanced two columns.
        assert_eq!(frame.cursor.as_ref().unwrap().col, 2);
    }

    #[test]
    fn emoji_occupies_two_cells() {
        let mut terminal = TermyTerminal::new(8, 1);
        let mut bytes = String::new();
        // U+1F980 CRAB — width 2.
        bytes.push('🦀');
        terminal
            .parser
            .advance(&mut terminal.screen, bytes.as_bytes());
        let frame = terminal.screen.snapshot();
        assert_eq!(frame.cells[0].char, '🦀');
        assert_eq!(frame.cells[0].width, 2);
        assert_eq!(frame.cells[1].width, 0);
        assert_eq!(frame.cursor.as_ref().unwrap().col, 2);
    }

    #[test]
    fn wide_char_at_last_column_wraps_to_next_row() {
        let mut terminal = TermyTerminal::new(4, 2);
        // Fill first three columns with ASCII, then emit a wide char. With
        // only one column left, the wide glyph must wrap to row 1 column 0.
        let mut bytes = String::from("abc");
        bytes.push('あ');
        terminal
            .parser
            .advance(&mut terminal.screen, bytes.as_bytes());
        let frame = terminal.screen.snapshot();
        // 'a', 'b', 'c' on row 0, the wide char skipped col 3 entirely.
        assert_eq!(frame.cells[0].char, 'a');
        assert_eq!(frame.cells[2].char, 'c');
        // The wide char landed on row 1 col 0.
        assert_eq!(frame.cells[4].char, 'あ');
        assert_eq!(frame.cells[4].width, 2);
        assert_eq!(frame.cells[5].width, 0);
    }

    #[test]
    fn combining_chars_do_not_get_their_own_cell() {
        // VTE-level: VTE does NOT coalesce combining marks for us; it emits a
        // separate `print(char)` call for each codepoint. unicode-width
        // reports `width = 0` for combining marks; we fall back to `width = 1`
        // for safety, so they consume a cell. This documents the current
        // behaviour rather than implementing full grapheme-cluster coalescing
        // (a future enhancement once a grapheme segmenter is on hand).
        let mut terminal = TermyTerminal::new(8, 1);
        // 'e' + COMBINING ACUTE ACCENT (U+0301).
        let bytes = "e\u{0301}";
        terminal
            .parser
            .advance(&mut terminal.screen, bytes.as_bytes());
        let frame = terminal.screen.snapshot();
        // Documented: combining mark currently lands in its own cell.
        assert_eq!(frame.cells[0].char, 'e');
        assert_eq!(frame.cells[1].char, '\u{0301}');
        // The combining mark falls through to width=1 (we max with 1).
        assert_eq!(frame.cells[1].width, 1);
    }

    #[test]
    fn urxvt_and_sgr_pixel_encodings() {
        let mut terminal = TermyTerminal::new(80, 24);
        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?1000h\x1b[?1015h");
        // urxvt encoding (1015) falls back to SGR.
        assert_eq!(terminal.screen.mouse_encoding, MouseEncoding::Sgr);

        terminal
            .parser
            .advance(&mut terminal.screen, b"\x1b[?1016h");
        assert_eq!(terminal.screen.mouse_encoding, MouseEncoding::SgrPixel);
    }
}
