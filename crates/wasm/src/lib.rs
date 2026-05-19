use serde::Serialize;
use termy_config_core::{
    AppConfig, ConfigDiagnostic, ConfigDiagnosticKind, CursorStyle, SHELL_DECIDE_THEME_ID,
    SystemAppearance, resolve_active_theme,
};
use termy_themes::{ThemeColors, normalize_theme_id};
use vte::{Params, Parser, Perform};
use wasm_bindgen::prelude::*;

const DEFAULT_CELL_WIDTH: f32 = 9.0;
const DEFAULT_CELL_HEIGHT: f32 = 18.0;

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
    palette: Palette,
    title: Option<String>,
    display_offset: usize,
    history_size: usize,
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
            palette,
            title: None,
            display_offset: 0,
            history_size: 0,
            events: Vec::new(),
            responses: Vec::new(),
        };
        screen.cells = vec![screen.blank_cell(0, 0); screen.cell_count()];
        screen.reindex_cells();
        screen
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
            render_text: false,
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

        if self.cursor_col >= usize::from(self.cols) {
            self.carriage_return();
            self.line_feed();
        }

        let index = self.cursor_row * usize::from(self.cols) + self.cursor_col;
        if let Some(cell) = self.cells.get_mut(index) {
            *cell = Cell {
                col: self.cursor_col,
                row: self.cursor_row,
                char: c,
                fg: self.current_fg,
                bg: self.current_bg,
                uses_terminal_default_bg: self.current_bg == self.palette.background,
                bold: self.current_bold,
                render_text: !c.is_control(),
            };
        }
        self.cursor_col += 1;
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
            self.cells.drain(0..cols);
            for col in 0..cols {
                self.cells.push(self.blank_cell(col, rows - 1));
            }
            self.history_size += 1;
        }
        self.cursor_row = rows.saturating_sub(1);
        self.reindex_cells();
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
                }
                1 => self.current_bold = true,
                22 => self.current_bold = false,
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
        Frame {
            cols: self.cols,
            rows: self.rows,
            cells: self.cells.clone(),
            cursor: Some(Cursor {
                col: self.cursor_col,
                row: self.cursor_row,
                style: "block",
            }),
            display_offset: self.display_offset,
            history_size: self.history_size,
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
            _ => {}
        }
    }

    fn csi_dispatch(&mut self, params: &Params, intermediates: &[u8], _ignore: bool, action: char) {
        let values = params_to_vec(params);
        let first = values.first().copied().unwrap_or(0);
        let amount = usize::from(if first == 0 { 1 } else { first });

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
    render_text: bool,
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
}
