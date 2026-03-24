use crate::render_metrics::{
    add_span_grid_paint_us, add_span_row_ops_rebuild_us, add_span_text_shaping_us,
    increment_grid_paint_count, increment_shape_line_calls, increment_shaped_line_cache_hit,
    increment_shaped_line_cache_miss,
};
use gpui::{
    App, Bounds, Element, Font, FontFeatures, FontWeight, Hsla, IntoElement, Pixels, ShapedLine,
    SharedString, Size, TextAlign, TextRun, UnderlineStyle, Window, point, px, quad,
};
use std::{cell::RefCell, collections::HashMap, rc::Rc, sync::Arc, time::Instant};

/// Info needed to render a single cell.
#[derive(Clone)]
pub struct CellRenderInfo {
    pub col: usize,
    pub row: usize,
    pub char: char,
    pub fg: Hsla,
    pub bg: Hsla,
    pub uses_terminal_default_bg: bool,
    pub bold: bool,
    pub render_text: bool,
    pub selected: bool,
    /// Part of the current (focused) search match
    pub search_current: bool,
    /// Part of any search match (but not current)
    pub search_match: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalCursorStyle {
    Line,
    Block,
}

/// Custom element for rendering the terminal grid.
pub type TerminalGridRow = Arc<Vec<CellRenderInfo>>;
pub type TerminalGridRows = Arc<Vec<TerminalGridRow>>;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TerminalGridPaintDamage {
    #[default]
    None,
    Full,
    Rows(Arc<[usize]>),
    /// Row damage with column bounds `(row, left_col_inclusive, right_col_inclusive)`.
    /// Emitted when alacritty reports partial damage with column-level granularity.
    RowRanges(Arc<[(usize, usize, usize)]>),
}

#[derive(Clone, Default)]
pub struct TerminalGridPaintCacheHandle(Rc<RefCell<TerminalGridPaintCache>>);

impl TerminalGridPaintCacheHandle {
    pub fn clear(&self) {
        self.0.borrow_mut().clear();
    }

    #[cfg(any(test, debug_assertions))]
    #[doc(hidden)]
    pub fn debug_seed_rows_for_tests(&self, row_count: usize) {
        self.0.borrow_mut().row_ops = vec![CachedRowPaintOps::default(); row_count];
    }

    #[cfg(any(test, debug_assertions))]
    #[doc(hidden)]
    pub fn debug_row_cache_len_for_tests(&self) -> usize {
        self.0.borrow().row_ops.len()
    }
}

pub struct TerminalGrid {
    pub cells: TerminalGridRows,
    pub paint_cache: TerminalGridPaintCacheHandle,
    pub paint_damage: TerminalGridPaintDamage,
    pub cell_size: Size<Pixels>,
    pub cols: usize,
    pub rows: usize,
    /// Clear color used to reset the grid surface every frame.
    pub clear_bg: Hsla,
    pub terminal_surface_bg: Hsla,
    pub cursor_color: Hsla,
    pub selection_bg: Hsla,
    pub selection_fg: Hsla,
    pub search_match_bg: Hsla,
    pub search_current_bg: Hsla,
    pub hovered_link_range: Option<(usize, usize, usize)>,
    pub cursor_cell: Option<(usize, usize)>,
    pub cursor_visible: bool,
    pub font_family: SharedString,
    pub font_size: Pixels,
    pub cursor_style: TerminalCursorStyle,
}

impl IntoElement for TerminalGrid {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

// NOTE: We intentionally render Unicode block elements (U+2580..U+259F) as
// pixel-snapped quads instead of shaped font glyphs.
//
// Why:
// - Glyph rasterization anti-aliases the hard edges of chars like '▀'.
// - In transparent/layered terminal surfaces (GPUI terminals, e.g. Zed/opencode),
//   those semi-transparent edge pixels can show up as faint seams/lines.
// - Drawing exact geometry with snapped bounds gives deterministic, hard edges
//   and eliminates the artifact.
const BLOCK_ELEMENTS_START: u32 = 0x2580;
const BLOCK_ELEMENTS_END: u32 = 0x259F;
const QUAD_UPPER_LEFT: u8 = 0b0001;
const QUAD_UPPER_RIGHT: u8 = 0b0010;
const QUAD_LOWER_LEFT: u8 = 0b0100;
const QUAD_LOWER_RIGHT: u8 = 0b1000;

#[derive(Clone, Copy, Debug, PartialEq)]
struct BlockRectSpec {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
    alpha: f32,
}

impl BlockRectSpec {
    const fn new(left: f32, top: f32, right: f32, bottom: f32, alpha: f32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
            alpha,
        }
    }
}

const EMPTY_BLOCK_RECT: BlockRectSpec = BlockRectSpec::new(0.0, 0.0, 0.0, 0.0, 0.0);

#[derive(Clone, Copy, Debug, PartialEq)]
struct BlockElementGeometry {
    rects: [BlockRectSpec; 4],
    rect_count: usize,
}

impl BlockElementGeometry {
    const fn one(rect: BlockRectSpec) -> Self {
        Self {
            rects: [rect, EMPTY_BLOCK_RECT, EMPTY_BLOCK_RECT, EMPTY_BLOCK_RECT],
            rect_count: 1,
        }
    }

    fn rects(&self) -> &[BlockRectSpec] {
        &self.rects[..self.rect_count]
    }
}

#[derive(Clone)]
struct TextBatch {
    start_col: usize,
    row: usize,
    text: String,
    bold: bool,
    fg: Hsla,
    underline: Option<UnderlineStyle>,
    cell_len: usize,
}

#[derive(Clone, Copy)]
struct BlockDraw {
    #[cfg_attr(not(test), allow(dead_code))]
    row: usize,
    col: usize,
    geometry: BlockElementGeometry,
    fg: Hsla,
}

#[derive(Clone)]
enum TextDrawOp {
    Batch(TextBatch),
    Block(BlockDraw),
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BackgroundSpan {
    start_col: usize,
    end_col_exclusive: usize,
    color: Hsla,
}

#[derive(Clone, Default)]
struct CachedRowPaintOps {
    background_spans: Vec<BackgroundSpan>,
    draw_ops: Vec<TextDrawOp>,
    shaped_lines: Vec<Option<ShapedLine>>,
}

#[derive(Clone, Debug, PartialEq)]
struct GridPaintStyleKey {
    cols: usize,
    rows: usize,
    cell_width_bits: u32,
    cell_height_bits: u32,
    clear_bg: [u32; 4],
    terminal_surface_bg: [u32; 4],
    selection_bg: [u32; 4],
    selection_fg: [u32; 4],
    search_match_bg: [u32; 4],
    search_current_bg: [u32; 4],
    cursor_style: TerminalCursorStyle,
    font_family: SharedString,
    font_size_bits: u32,
}

#[derive(Default)]
struct TerminalGridPaintCache {
    row_ops: Vec<CachedRowPaintOps>,
    style_key: Option<GridPaintStyleKey>,
    last_cursor_cell: Option<(usize, usize)>,
    last_cursor_visible: bool,
    last_hovered_link_range: Option<(usize, usize, usize)>,
    /// Per-pass scratch: `Some((left, right))` if only that column range is dirty for the row.
    /// `None` means full-row damage (cursor/hover transitions, or no damage info available).
    /// Cleared and repopulated at the start of every paint pass.
    dirty_col_ranges: Vec<Option<(usize, usize)>>,
    /// Per-style cache: maps hsla_bits(cell.bg) → resolved background fill color.
    /// Avoids redundant float comparisons when many cells share the same default background.
    /// Cleared whenever the style key changes.
    color_cache: HashMap<[u32; 4], Option<Hsla>>,
}

impl TerminalGridPaintCache {
    fn clear(&mut self) {
        self.row_ops.clear();
        self.style_key = None;
        self.last_cursor_cell = None;
        self.last_cursor_visible = false;
        self.last_hovered_link_range = None;
        self.dirty_col_ranges.clear();
        self.color_cache.clear();
    }

    fn ensure_row_capacity(&mut self, row_count: usize) {
        if self.row_ops.len() != row_count {
            self.row_ops = vec![CachedRowPaintOps::default(); row_count];
        }
        // dirty_col_ranges is per-pass scratch — resize and reset every frame
        if self.dirty_col_ranges.len() != row_count {
            self.dirty_col_ranges = vec![None; row_count];
        } else {
            self.dirty_col_ranges.fill(None);
        }
    }
}

#[derive(Clone, Copy)]
struct TextBatchKey {
    bold: bool,
    fg: Hsla,
}

impl TextBatch {
    fn new(
        start_col: usize,
        row: usize,
        c: char,
        key: TextBatchKey,
        underline: Option<UnderlineStyle>,
    ) -> Self {
        let mut text = String::with_capacity(16);
        text.push(c);
        Self {
            start_col,
            row,
            text,
            bold: key.bold,
            fg: key.fg,
            underline,
            cell_len: 1,
        }
    }

    fn can_append(
        &self,
        col: usize,
        row: usize,
        key: TextBatchKey,
        underline: &Option<UnderlineStyle>,
    ) -> bool {
        self.row == row
            && self.start_col + self.cell_len == col
            && self.bold == key.bold
            && self.fg == key.fg
            && self.underline == *underline
    }

    fn append_char(&mut self, c: char) {
        self.text.push(c);
        self.cell_len += 1;
    }
}

fn full_cell_rect(alpha: f32) -> BlockRectSpec {
    BlockRectSpec::new(0.0, 0.0, 1.0, 1.0, alpha)
}

fn vertical_fill_from_bottom(fraction: f32) -> BlockElementGeometry {
    BlockElementGeometry::one(BlockRectSpec::new(0.0, 1.0 - fraction, 1.0, 1.0, 1.0))
}

fn horizontal_fill_from_left(fraction: f32) -> BlockElementGeometry {
    BlockElementGeometry::one(BlockRectSpec::new(0.0, 0.0, fraction, 1.0, 1.0))
}

fn quadrants(mask: u8) -> BlockElementGeometry {
    let mut rects = [EMPTY_BLOCK_RECT; 4];
    let mut count = 0;

    if mask & QUAD_UPPER_LEFT != 0 {
        rects[count] = BlockRectSpec::new(0.0, 0.0, 0.5, 0.5, 1.0);
        count += 1;
    }
    if mask & QUAD_UPPER_RIGHT != 0 {
        rects[count] = BlockRectSpec::new(0.5, 0.0, 1.0, 0.5, 1.0);
        count += 1;
    }
    if mask & QUAD_LOWER_LEFT != 0 {
        rects[count] = BlockRectSpec::new(0.0, 0.5, 0.5, 1.0, 1.0);
        count += 1;
    }
    if mask & QUAD_LOWER_RIGHT != 0 {
        rects[count] = BlockRectSpec::new(0.5, 0.5, 1.0, 1.0, 1.0);
        count += 1;
    }

    BlockElementGeometry {
        rects,
        rect_count: count,
    }
}

fn block_element_geometry(c: char) -> Option<BlockElementGeometry> {
    let codepoint = c as u32;
    if !(BLOCK_ELEMENTS_START..=BLOCK_ELEMENTS_END).contains(&codepoint) {
        return None;
    }

    Some(match c {
        '\u{2580}' => BlockElementGeometry::one(BlockRectSpec::new(0.0, 0.0, 1.0, 0.5, 1.0)),
        '\u{2581}' => vertical_fill_from_bottom(1.0 / 8.0),
        '\u{2582}' => vertical_fill_from_bottom(2.0 / 8.0),
        '\u{2583}' => vertical_fill_from_bottom(3.0 / 8.0),
        '\u{2584}' => vertical_fill_from_bottom(4.0 / 8.0),
        '\u{2585}' => vertical_fill_from_bottom(5.0 / 8.0),
        '\u{2586}' => vertical_fill_from_bottom(6.0 / 8.0),
        '\u{2587}' => vertical_fill_from_bottom(7.0 / 8.0),
        '\u{2588}' => BlockElementGeometry::one(full_cell_rect(1.0)),
        '\u{2589}' => horizontal_fill_from_left(7.0 / 8.0),
        '\u{258A}' => horizontal_fill_from_left(6.0 / 8.0),
        '\u{258B}' => horizontal_fill_from_left(5.0 / 8.0),
        '\u{258C}' => horizontal_fill_from_left(4.0 / 8.0),
        '\u{258D}' => horizontal_fill_from_left(3.0 / 8.0),
        '\u{258E}' => horizontal_fill_from_left(2.0 / 8.0),
        '\u{258F}' => horizontal_fill_from_left(1.0 / 8.0),
        '\u{2590}' => BlockElementGeometry::one(BlockRectSpec::new(0.5, 0.0, 1.0, 1.0, 1.0)),
        '\u{2591}' => BlockElementGeometry::one(full_cell_rect(0.25)),
        '\u{2592}' => BlockElementGeometry::one(full_cell_rect(0.50)),
        '\u{2593}' => BlockElementGeometry::one(full_cell_rect(0.75)),
        '\u{2594}' => BlockElementGeometry::one(BlockRectSpec::new(0.0, 0.0, 1.0, 1.0 / 8.0, 1.0)),
        '\u{2595}' => BlockElementGeometry::one(BlockRectSpec::new(7.0 / 8.0, 0.0, 1.0, 1.0, 1.0)),
        '\u{2596}' => quadrants(QUAD_LOWER_LEFT),
        '\u{2597}' => quadrants(QUAD_LOWER_RIGHT),
        '\u{2598}' => quadrants(QUAD_UPPER_LEFT),
        '\u{2599}' => quadrants(QUAD_UPPER_LEFT | QUAD_LOWER_LEFT | QUAD_LOWER_RIGHT),
        '\u{259A}' => quadrants(QUAD_UPPER_LEFT | QUAD_LOWER_RIGHT),
        '\u{259B}' => quadrants(QUAD_UPPER_LEFT | QUAD_UPPER_RIGHT | QUAD_LOWER_LEFT),
        '\u{259C}' => quadrants(QUAD_UPPER_LEFT | QUAD_UPPER_RIGHT | QUAD_LOWER_RIGHT),
        '\u{259D}' => quadrants(QUAD_UPPER_RIGHT),
        '\u{259E}' => quadrants(QUAD_UPPER_RIGHT | QUAD_LOWER_LEFT),
        '\u{259F}' => quadrants(QUAD_UPPER_RIGHT | QUAD_LOWER_LEFT | QUAD_LOWER_RIGHT),
        _ => return None,
    })
}

fn snapped_block_rect_bounds(
    cell_bounds: Bounds<Pixels>,
    rect: BlockRectSpec,
) -> Option<Bounds<Pixels>> {
    let origin_x: f32 = cell_bounds.origin.x.into();
    let origin_y: f32 = cell_bounds.origin.y.into();
    let cell_width: f32 = cell_bounds.size.width.into();
    let cell_height: f32 = cell_bounds.size.height.into();

    let left = (origin_x + cell_width * rect.left).round();
    let right = (origin_x + cell_width * rect.right).round();
    let top = (origin_y + cell_height * rect.top).round();
    let bottom = (origin_y + cell_height * rect.bottom).round();

    let width = right - left;
    let height = bottom - top;
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    Some(Bounds {
        origin: point(px(left), px(top)),
        size: Size {
            width: px(width),
            height: px(height),
        },
    })
}

fn snapped_quad_bounds(bounds: Bounds<Pixels>) -> Option<Bounds<Pixels>> {
    let origin_x: f32 = bounds.origin.x.into();
    let origin_y: f32 = bounds.origin.y.into();
    let width: f32 = bounds.size.width.into();
    let height: f32 = bounds.size.height.into();

    let left = origin_x.round();
    let right = (origin_x + width).round();
    let top = origin_y.round();
    let bottom = (origin_y + height).round();

    let snapped_width = right - left;
    let snapped_height = bottom - top;
    if snapped_width <= 0.0 || snapped_height <= 0.0 {
        return None;
    }

    Some(Bounds {
        origin: point(px(left), px(top)),
        size: Size {
            width: px(snapped_width),
            height: px(snapped_height),
        },
    })
}

fn should_paint_clear_bg(color: Hsla) -> bool {
    color.a > f32::EPSILON
}

fn paint_block_element_quad(
    window: &mut Window,
    cell_bounds: Bounds<Pixels>,
    geometry: BlockElementGeometry,
    color: Hsla,
) {
    for rect in geometry.rects() {
        if let Some(bounds) = snapped_block_rect_bounds(cell_bounds, *rect) {
            let mut fill = color;
            fill.a *= rect.alpha;
            window.paint_quad(quad(
                bounds,
                px(0.0),
                fill,
                gpui::Edges::default(),
                Hsla::transparent_black(),
                gpui::BorderStyle::default(),
            ));
        }
    }
}

fn hsla_bits(color: Hsla) -> [u32; 4] {
    [
        color.h.to_bits(),
        color.s.to_bits(),
        color.l.to_bits(),
        color.a.to_bits(),
    ]
}

fn push_row_if_in_bounds(rows: &mut Vec<usize>, maybe_row: Option<usize>, row_count: usize) {
    if let Some(row) = maybe_row
        && row < row_count
    {
        rows.push(row);
    }
}

fn sorted_dedup_rows(mut rows: Vec<usize>) -> Arc<[usize]> {
    rows.sort_unstable();
    rows.dedup();
    rows.into()
}

fn text_batches_match_without_row(lhs: &TextBatch, rhs: &TextBatch) -> bool {
    lhs.start_col == rhs.start_col
        && lhs.text == rhs.text
        && lhs.bold == rhs.bold
        && lhs.fg == rhs.fg
        && lhs.underline == rhs.underline
        && lhs.cell_len == rhs.cell_len
}

fn block_draws_match_without_row(lhs: &BlockDraw, rhs: &BlockDraw) -> bool {
    lhs.col == rhs.col && lhs.geometry == rhs.geometry && lhs.fg == rhs.fg
}

fn text_draw_ops_match_without_row(lhs: &TextDrawOp, rhs: &TextDrawOp) -> bool {
    match (lhs, rhs) {
        (TextDrawOp::Batch(lhs), TextDrawOp::Batch(rhs)) => {
            text_batches_match_without_row(lhs, rhs)
        }
        (TextDrawOp::Block(lhs), TextDrawOp::Block(rhs)) => block_draws_match_without_row(lhs, rhs),
        _ => false,
    }
}

fn draw_op_col_range(op: &TextDrawOp) -> (usize, usize) {
    match op {
        TextDrawOp::Batch(b) => (b.start_col, b.start_col + b.cell_len.saturating_sub(1)),
        TextDrawOp::Block(b) => (b.col, b.col),
    }
}

fn col_ranges_overlap(a: (usize, usize), b: (usize, usize)) -> bool {
    a.0 <= b.1 && b.0 <= a.1
}

fn cached_row_draw_ops_match_without_row(lhs: &CachedRowPaintOps, rhs: &CachedRowPaintOps) -> bool {
    lhs.background_spans == rhs.background_spans
        && lhs.draw_ops.len() == rhs.draw_ops.len()
        && lhs
            .draw_ops
            .iter()
            .zip(rhs.draw_ops.iter())
            .all(|(lhs, rhs)| text_draw_ops_match_without_row(lhs, rhs))
}

fn find_matching_previous_row_ops_index(
    row: usize,
    row_ops: &CachedRowPaintOps,
    previous_row_ops: &[CachedRowPaintOps],
) -> Option<usize> {
    for preferred in [Some(row), row.checked_add(1), row.checked_sub(1)] {
        let Some(index) = preferred else {
            continue;
        };
        let Some(previous) = previous_row_ops.get(index) else {
            continue;
        };
        if cached_row_draw_ops_match_without_row(row_ops, previous) {
            return Some(index);
        }
    }

    previous_row_ops.iter().enumerate().find_map(|(index, previous)| {
        matches!(index, i if i != row && Some(i) != row.checked_add(1) && Some(i) != row.checked_sub(1))
            .then_some(previous)
            .filter(|previous| cached_row_draw_ops_match_without_row(row_ops, previous))
            .map(|_| index)
    })
}

impl Element for TerminalGrid {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<gpui::ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (gpui::LayoutId, Self::RequestLayoutState) {
        let width = self.cell_size.width * self.cols as f32;
        let height = self.cell_size.height * self.rows as f32;

        let layout_id = window.request_layout(
            gpui::Style {
                size: gpui::Size {
                    width: gpui::Length::Definite(gpui::DefiniteLength::Absolute(
                        gpui::AbsoluteLength::Pixels(width),
                    )),
                    height: gpui::Length::Definite(gpui::DefiniteLength::Absolute(
                        gpui::AbsoluteLength::Pixels(height),
                    )),
                },
                ..Default::default()
            },
            [],
            cx,
        );

        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&gpui::GlobalElementId>,
        _inspector_id: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        increment_grid_paint_count();
        let t_paint = Instant::now();
        self.paint_with_row_cache(bounds, window, cx);
        add_span_grid_paint_us(t_paint.elapsed().as_micros() as u64);
    }
}

impl TerminalGrid {
    fn paint_style_key(&self) -> GridPaintStyleKey {
        GridPaintStyleKey {
            cols: self.cols,
            rows: self.rows,
            cell_width_bits: Into::<f32>::into(self.cell_size.width).to_bits(),
            cell_height_bits: Into::<f32>::into(self.cell_size.height).to_bits(),
            clear_bg: hsla_bits(self.clear_bg),
            terminal_surface_bg: hsla_bits(self.terminal_surface_bg),
            selection_bg: hsla_bits(self.selection_bg),
            selection_fg: hsla_bits(self.selection_fg),
            search_match_bg: hsla_bits(self.search_match_bg),
            search_current_bg: hsla_bits(self.search_current_bg),
            cursor_style: self.cursor_style,
            font_family: self.font_family.clone(),
            font_size_bits: Into::<f32>::into(self.font_size).to_bits(),
        }
    }

    fn row_background_fill(&self, cell: &CellRenderInfo) -> Option<Hsla> {
        if cell.selected {
            Some(self.selection_bg)
        } else if cell.search_current {
            Some(self.search_current_bg)
        } else if cell.search_match {
            Some(self.search_match_bg)
        } else if cell.bg.a <= 0.01 {
            None
        } else if cell.uses_terminal_default_bg {
            (cell.bg != self.terminal_surface_bg).then_some(cell.bg)
        } else {
            Some(cell.bg)
        }
    }

    fn build_row_background_spans(
        &self,
        row_cells: &[CellRenderInfo],
        color_cache: &mut HashMap<[u32; 4], Option<Hsla>>,
    ) -> Vec<BackgroundSpan> {
        if row_cells.is_empty() {
            return Vec::new();
        }

        let mut spans = Vec::new();
        let mut current: Option<BackgroundSpan> = None;

        for cell in row_cells {
            // For cells with default background that aren't highlighted, cache the fill
            // resolution to avoid repeated float comparisons against terminal_surface_bg.
            let fill = if !cell.selected
                && !cell.search_current
                && !cell.search_match
                && cell.bg.a > 0.01
                && cell.uses_terminal_default_bg
            {
                let key = hsla_bits(cell.bg);
                *color_cache
                    .entry(key)
                    .or_insert_with(|| (cell.bg != self.terminal_surface_bg).then_some(cell.bg))
            } else {
                self.row_background_fill(cell)
            };
            match (current.as_mut(), fill) {
                (Some(span), Some(color))
                    if span.color == color && span.end_col_exclusive == cell.col =>
                {
                    span.end_col_exclusive = cell.col.saturating_add(1);
                }
                (Some(span), Some(color)) => {
                    spans.push(*span);
                    current = Some(BackgroundSpan {
                        start_col: cell.col,
                        end_col_exclusive: cell.col.saturating_add(1),
                        color,
                    });
                }
                (Some(span), None) => {
                    spans.push(*span);
                    current = None;
                }
                (None, Some(color)) => {
                    current = Some(BackgroundSpan {
                        start_col: cell.col,
                        end_col_exclusive: cell.col.saturating_add(1),
                        color,
                    });
                }
                (None, None) => {}
            }
        }

        if let Some(span) = current {
            spans.push(span);
        }

        spans
    }

    fn collect_row_draw_ops(
        &self,
        row_cells: &[CellRenderInfo],
        cursor_fg: Hsla,
        highlight_fg: Hsla,
    ) -> Vec<TextDrawOp> {
        let mut ops = Vec::with_capacity(row_cells.len());
        let mut current: Option<TextBatch> = None;

        for cell in row_cells {
            if !Self::cell_is_drawable_text(cell) {
                Self::push_pending_text_batch(&mut current, &mut ops);
                continue;
            }

            let fg = self.cell_fg_color(cell, cursor_fg, highlight_fg);
            if let Some(geometry) = block_element_geometry(cell.char) {
                Self::push_pending_text_batch(&mut current, &mut ops);
                ops.push(TextDrawOp::Block(BlockDraw {
                    row: cell.row,
                    col: cell.col,
                    geometry,
                    fg,
                }));
                continue;
            }

            let underline = self.cell_underline(cell.row, cell.col, fg);
            let key = TextBatchKey {
                bold: cell.bold,
                fg,
            };

            let should_append = current
                .as_ref()
                .is_some_and(|batch| batch.can_append(cell.col, cell.row, key, &underline));
            if should_append {
                if let Some(batch) = current.as_mut() {
                    batch.append_char(cell.char);
                }
                continue;
            }

            Self::push_pending_text_batch(&mut current, &mut ops);
            current = Some(TextBatch::new(
                cell.col, cell.row, cell.char, key, underline,
            ));
        }

        Self::push_pending_text_batch(&mut current, &mut ops);
        ops
    }

    fn rebuild_cached_row_ops(
        &self,
        row_cells: &[CellRenderInfo],
        cursor_fg: Hsla,
        highlight_fg: Hsla,
        color_cache: &mut HashMap<[u32; 4], Option<Hsla>>,
    ) -> CachedRowPaintOps {
        let draw_ops = self.collect_row_draw_ops(row_cells, cursor_fg, highlight_fg);
        CachedRowPaintOps {
            background_spans: self.build_row_background_spans(row_cells, color_cache),
            shaped_lines: vec![None; draw_ops.len()],
            draw_ops,
        }
    }

    fn clear_bounds(&self, bounds: Bounds<Pixels>, window: &mut Window) {
        if !should_paint_clear_bg(self.clear_bg) {
            return;
        }
        window.paint_quad(quad(
            bounds,
            px(0.0),
            self.clear_bg,
            gpui::Edges::default(),
            Hsla::transparent_black(),
            gpui::BorderStyle::default(),
        ));
    }

    #[allow(clippy::too_many_arguments)]
    fn paint_cached_row_ops(
        &self,
        row: usize,
        row_ops: &mut CachedRowPaintOps,
        origin: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
        font_normal: &Font,
        font_bold: &Font,
    ) {
        for span in &row_ops.background_spans {
            if span.start_col >= span.end_col_exclusive {
                continue;
            }
            let x = origin.x + self.cell_size.width * span.start_col as f32;
            let width_cells = span.end_col_exclusive.saturating_sub(span.start_col);
            if width_cells == 0 {
                continue;
            }
            let cell_bounds = Bounds {
                origin: point(x, origin.y),
                size: Size {
                    width: self.cell_size.width * width_cells as f32,
                    height: self.cell_size.height,
                },
            };
            if let Some(bounds) = snapped_quad_bounds(cell_bounds) {
                window.paint_quad(quad(
                    bounds,
                    px(0.0),
                    span.color,
                    gpui::Edges::default(),
                    Hsla::transparent_black(),
                    gpui::BorderStyle::default(),
                ));
            }
        }

        // Keep block cursors beneath glyphs, but paint line cursors on top so text/block ops
        // cannot overdraw the line.
        if self.cursor_style == TerminalCursorStyle::Block {
            self.paint_cursor_for_row(row, origin, window);
        }

        for (index, op) in row_ops.draw_ops.iter().enumerate() {
            match op {
                TextDrawOp::Batch(batch) => {
                    let x = origin.x + self.cell_size.width * batch.start_col as f32;
                    let line = if row_ops.shaped_lines.get(index).is_some_and(Option::is_some) {
                        increment_shaped_line_cache_hit();
                        row_ops.shaped_lines[index]
                            .as_ref()
                            .expect("cached shaped line must exist")
                    } else {
                        increment_shaped_line_cache_miss();
                        increment_shape_line_calls();
                        let font = if batch.bold { font_bold } else { font_normal };
                        let run = TextRun {
                            len: batch.text.len(),
                            font: font.clone(),
                            color: batch.fg,
                            background_color: None,
                            underline: batch.underline,
                            strikethrough: None,
                        };
                        let t_shape = Instant::now();
                        row_ops.shaped_lines[index] = Some(window.text_system().shape_line(
                            batch.text.clone().into(),
                            self.font_size,
                            &[run],
                            Some(self.cell_size.width),
                        ));
                        add_span_text_shaping_us(t_shape.elapsed().as_micros() as u64);
                        row_ops.shaped_lines[index]
                            .as_ref()
                            .expect("cached shaped line must be created")
                    };
                    let _ = line.paint(
                        point(x, origin.y),
                        self.cell_size.height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                }
                TextDrawOp::Block(block) => {
                    let x = origin.x + self.cell_size.width * block.col as f32;
                    let cell_bounds = Bounds {
                        origin: point(x, origin.y),
                        size: self.cell_size,
                    };
                    paint_block_element_quad(window, cell_bounds, block.geometry, block.fg);
                }
            }
        }

        if self.cursor_style == TerminalCursorStyle::Line {
            self.paint_cursor_for_row(row, origin, window);
        }
    }

    fn dirty_rows_for_pass(
        &self,
        cache: &mut TerminalGridPaintCache,
    ) -> (bool, bool, Arc<[usize]>) {
        let style_key = self.paint_style_key();
        let style_changed = cache.style_key.as_ref() != Some(&style_key);
        if style_changed {
            cache.color_cache.clear();
        }
        cache.style_key = Some(style_key);

        let mut full_repaint =
            style_changed || matches!(self.paint_damage, TerminalGridPaintDamage::Full);
        let mut rows = Vec::new();
        if let TerminalGridPaintDamage::Rows(damaged_rows) = &self.paint_damage {
            rows.extend(damaged_rows.iter().copied().filter(|row| *row < self.rows));
        }
        if let TerminalGridPaintDamage::RowRanges(spans) = &self.paint_damage {
            for &(row, left, right) in spans.iter() {
                if row < self.rows {
                    rows.push(row);
                    // Merge multiple spans on the same row into one union range
                    cache.dirty_col_ranges[row] = Some(match cache.dirty_col_ranges[row] {
                        None => (left, right),
                        Some((prev_l, prev_r)) => (prev_l.min(left), prev_r.max(right)),
                    });
                }
            }
        }

        if cache.last_cursor_cell != self.cursor_cell {
            push_row_if_in_bounds(
                &mut rows,
                cache.last_cursor_cell.map(|(_, row)| row),
                self.rows,
            );
            push_row_if_in_bounds(&mut rows, self.cursor_cell.map(|(_, row)| row), self.rows);
        }

        // Blink visibility changed → only need to rebuild for Block cursor, since the
        // cursor cell's text fg color is baked into draw ops. Line cursor is a plain
        // quad painted after row ops and needs no row rebuild on blink.
        if cache.last_cursor_visible != self.cursor_visible
            && self.cursor_style == TerminalCursorStyle::Block
        {
            push_row_if_in_bounds(&mut rows, self.cursor_cell.map(|(_, row)| row), self.rows);
        }

        if cache.last_hovered_link_range != self.hovered_link_range {
            push_row_if_in_bounds(
                &mut rows,
                cache.last_hovered_link_range.map(|(row, _, _)| row),
                self.rows,
            );
            push_row_if_in_bounds(
                &mut rows,
                self.hovered_link_range.map(|(row, _, _)| row),
                self.rows,
            );
        }

        if self.rows == 0 || self.cols == 0 {
            rows.clear();
            full_repaint = false;
        }

        cache.last_cursor_cell = self.cursor_cell;
        cache.last_cursor_visible = self.cursor_visible;
        cache.last_hovered_link_range = self.hovered_link_range;

        (full_repaint, style_changed, sorted_dedup_rows(rows))
    }

    fn paint_cursor_for_row(&self, row: usize, origin: gpui::Point<Pixels>, window: &mut Window) {
        let Some((cursor_col, cursor_row)) = self.cursor_cell else {
            return;
        };
        if !self.cursor_visible {
            return;
        }
        if cursor_row != row {
            return;
        }
        let x = origin.x + self.cell_size.width * cursor_col as f32;
        let y = origin.y;
        let cell_bounds = Bounds {
            origin: point(x, y),
            size: self.cell_size,
        };
        let cursor_bounds = match self.cursor_style {
            TerminalCursorStyle::Block => cell_bounds,
            TerminalCursorStyle::Line => {
                let cell_width: f32 = self.cell_size.width.into();
                let cursor_width = px(cell_width.clamp(1.0, 2.0));
                Bounds::new(
                    cell_bounds.origin,
                    Size {
                        width: cursor_width,
                        height: cell_bounds.size.height,
                    },
                )
            }
        };

        window.paint_quad(quad(
            cursor_bounds,
            px(0.0),
            self.cursor_color,
            gpui::Edges::default(),
            Hsla::transparent_black(),
            gpui::BorderStyle::default(),
        ));
    }

    fn rebuild_cached_rows_for_pass(
        &self,
        cache: &mut TerminalGridPaintCache,
        full_repaint: bool,
        style_changed: bool,
        dirty_rows: &[usize],
        cursor_fg: Hsla,
        highlight_fg: Hsla,
    ) {
        let previous_row_ops = (!style_changed).then(|| cache.row_ops.clone());
        // Build ops first using color_cache, then write to row_ops (field-split borrow).
        let mut rebuild_row = |row: usize| {
            if row >= self.rows {
                return;
            }
            // Read col range hint (Copy) before any mutable borrows.
            let dirty_col_range = cache.dirty_col_ranges.get(row).copied().flatten();

            // Build the next ops, using color_cache (separate field from row_ops).
            let mut next_row_ops = if let Some(row_cells) = self.cells.get(row) {
                self.rebuild_cached_row_ops(
                    row_cells.as_slice(),
                    cursor_fg,
                    highlight_fg,
                    &mut cache.color_cache,
                )
            } else {
                // Row is no longer present — clear stale ops.
                CachedRowPaintOps::default()
            };

            // color_cache borrow ends here; now we can mutably borrow row_ops.
            let Some(row_slot) = cache.row_ops.get_mut(row) else {
                return;
            };

            // 1. Try whole-row ShapedLine reuse: if the entire row's ops match a previous
            //    row, reuse all its ShapedLine objects (existing logic).
            let mut whole_row_reused = false;
            if let Some(previous_row_ops) = previous_row_ops.as_ref()
                && let Some(previous_index) =
                    find_matching_previous_row_ops_index(row, &next_row_ops, previous_row_ops)
            {
                let previous = &previous_row_ops[previous_index];
                if previous.shaped_lines.len() == next_row_ops.shaped_lines.len() {
                    next_row_ops.shaped_lines = previous.shaped_lines.clone();
                    whole_row_reused = true;
                }
            }

            // 2. Per-op ShapedLine reuse: if we know the dirty column range (from RowRanges
            //    damage), reuse ShapedLines for text batches that don't overlap the dirty
            //    region. This avoids re-shaping unchanged text runs when only a few columns
            //    changed (e.g. a single character typed at the cursor).
            if !whole_row_reused {
                if let Some(dirty_range) = dirty_col_range {
                    if let Some(prev_row) = previous_row_ops.as_ref().and_then(|ops| ops.get(row)) {
                        for (i, op) in next_row_ops.draw_ops.iter().enumerate() {
                            let op_range = draw_op_col_range(op);
                            if !col_ranges_overlap(op_range, dirty_range) {
                                if let Some(prev_op) = prev_row.draw_ops.get(i) {
                                    if text_draw_ops_match_without_row(op, prev_op) {
                                        next_row_ops.shaped_lines[i] =
                                            prev_row.shaped_lines[i].clone();
                                    }
                                }
                            }
                        }
                    }
                }
            }

            *row_slot = next_row_ops;
        };

        let t0 = Instant::now();
        if full_repaint {
            for row in 0..self.rows {
                rebuild_row(row);
            }
        } else {
            for row in dirty_rows.iter().copied() {
                rebuild_row(row);
            }
        }
        add_span_row_ops_rebuild_us(t0.elapsed().as_micros() as u64);
    }

    fn paint_with_row_cache(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let origin = bounds.origin;
        let terminal_font_features = FontFeatures::disable_ligatures();
        let font_normal = Font {
            family: self.font_family.clone(),
            features: terminal_font_features.clone(),
            weight: FontWeight::NORMAL,
            ..Default::default()
        };
        let font_bold = Font {
            family: self.font_family.clone(),
            features: terminal_font_features,
            weight: FontWeight::BOLD,
            ..Default::default()
        };
        let cursor_fg = Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.0,
            a: 1.0,
        };
        let highlight_fg = Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.08,
            a: 1.0,
        };

        let mut cache = self.paint_cache.0.borrow_mut();
        cache.ensure_row_capacity(self.rows);
        let (full_repaint, style_changed, dirty_rows) = self.dirty_rows_for_pass(&mut cache);
        self.rebuild_cached_rows_for_pass(
            &mut cache,
            full_repaint,
            style_changed,
            dirty_rows.as_ref(),
            cursor_fg,
            highlight_fg,
        );

        // GPUI paint passes do not preserve previous pixels across frames. Always clear and draw
        // all rows; damage only controls which cached row ops are recomputed.
        self.clear_bounds(
            Bounds {
                origin,
                size: bounds.size,
            },
            window,
        );
        for row in 0..self.rows {
            let row_origin = point(origin.x, origin.y + self.cell_size.height * row as f32);
            self.paint_cached_row_ops(
                row,
                &mut cache.row_ops[row],
                row_origin,
                window,
                cx,
                &font_normal,
                &font_bold,
            );
        }

        drop(cache);
    }

    #[cfg(test)]
    fn cell_count(&self) -> usize {
        self.cells.iter().map(|row| row.len()).sum()
    }

    fn cell_is_drawable_text(cell: &CellRenderInfo) -> bool {
        cell.render_text && cell.char != ' ' && cell.char != '\0' && !cell.char.is_control()
    }

    fn cell_fg_color(&self, cell: &CellRenderInfo, cursor_fg: Hsla, highlight_fg: Hsla) -> Hsla {
        if self.cursor_cell == Some((cell.col, cell.row))
            && self.cursor_style == TerminalCursorStyle::Block
            && self.cursor_visible
        {
            cursor_fg
        } else if cell.selected {
            self.selection_fg
        } else if cell.search_current || cell.search_match {
            highlight_fg
        } else {
            cell.fg
        }
    }

    fn cell_underline(&self, row: usize, col: usize, color: Hsla) -> Option<UnderlineStyle> {
        self.hovered_link_range
            .and_then(|(link_row, start_col, end_col)| {
                if row == link_row && col >= start_col && col <= end_col {
                    Some(UnderlineStyle {
                        thickness: px(1.0),
                        color: Some(color),
                        wavy: false,
                    })
                } else {
                    None
                }
            })
    }

    fn push_pending_text_batch(current: &mut Option<TextBatch>, ops: &mut Vec<TextDrawOp>) {
        if let Some(batch) = current.take() {
            ops.push(TextDrawOp::Batch(batch));
        }
    }

    #[cfg(test)]
    fn collect_draw_ops(&self, cursor_fg: Hsla, highlight_fg: Hsla) -> Vec<TextDrawOp> {
        let mut ops = Vec::with_capacity(self.cell_count());
        let mut current: Option<TextBatch> = None;

        for cell in self.cells.iter().flat_map(|row| row.iter()) {
            if !Self::cell_is_drawable_text(cell) {
                Self::push_pending_text_batch(&mut current, &mut ops);
                continue;
            }

            let fg = self.cell_fg_color(cell, cursor_fg, highlight_fg);
            if let Some(geometry) = block_element_geometry(cell.char) {
                Self::push_pending_text_batch(&mut current, &mut ops);
                ops.push(TextDrawOp::Block(BlockDraw {
                    row: cell.row,
                    col: cell.col,
                    geometry,
                    fg,
                }));
                continue;
            }

            let underline = self.cell_underline(cell.row, cell.col, fg);
            let key = TextBatchKey {
                bold: cell.bold,
                fg,
            };

            let should_append = current
                .as_ref()
                .is_some_and(|batch| batch.can_append(cell.col, cell.row, key, &underline));

            if should_append {
                if let Some(batch) = current.as_mut() {
                    batch.append_char(cell.char);
                }
                continue;
            }

            Self::push_pending_text_batch(&mut current, &mut ops);
            current = Some(TextBatch::new(
                cell.col, cell.row, cell.char, key, underline,
            ));
        }

        Self::push_pending_text_batch(&mut current, &mut ops);

        ops
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Bounds, Size, point, px};

    fn test_color(h: f32, s: f32, l: f32) -> Hsla {
        Hsla { h, s, l, a: 1.0 }
    }

    fn test_cell(col: usize, row: usize, c: char) -> CellRenderInfo {
        CellRenderInfo {
            col,
            row,
            char: c,
            fg: test_color(0.4, 0.5, 0.6),
            bg: test_color(0.0, 0.0, 0.0),
            uses_terminal_default_bg: false,
            bold: false,
            render_text: true,
            selected: false,
            search_current: false,
            search_match: false,
        }
    }

    fn test_grid(
        cells: Vec<CellRenderInfo>,
        hovered: Option<(usize, usize, usize)>,
    ) -> TerminalGrid {
        test_grid_rows(vec![cells], hovered)
    }

    fn test_grid_rows(
        rows: Vec<Vec<CellRenderInfo>>,
        hovered: Option<(usize, usize, usize)>,
    ) -> TerminalGrid {
        let row_count = rows.len();
        let col_count = rows.iter().map(Vec::len).max().unwrap_or(0);
        TerminalGrid {
            cells: Arc::new(rows.into_iter().map(Arc::new).collect()),
            paint_cache: TerminalGridPaintCacheHandle::default(),
            paint_damage: TerminalGridPaintDamage::Full,
            cell_size: Size {
                width: px(10.0),
                height: px(20.0),
            },
            cols: col_count,
            rows: row_count,
            clear_bg: Hsla::transparent_black(),
            terminal_surface_bg: test_color(0.0, 0.0, 0.0),
            cursor_color: test_color(0.1, 0.1, 0.1),
            selection_bg: test_color(0.2, 0.2, 0.2),
            selection_fg: test_color(0.3, 0.3, 0.3),
            search_match_bg: test_color(0.4, 0.4, 0.4),
            search_current_bg: test_color(0.5, 0.5, 0.5),
            hovered_link_range: hovered,
            cursor_cell: None,
            cursor_visible: false,
            font_family: SharedString::from("JetBrains Mono"),
            font_size: px(14.0),
            cursor_style: TerminalCursorStyle::Block,
        }
    }

    fn collect_draw_ops(grid: &TerminalGrid) -> Vec<TextDrawOp> {
        grid.collect_draw_ops(
            Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.0,
                a: 1.0,
            },
            Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.08,
                a: 1.0,
            },
        )
    }

    fn collect_batches(grid: &TerminalGrid) -> Vec<TextBatch> {
        collect_draw_ops(grid)
            .into_iter()
            .filter_map(|op| match op {
                TextDrawOp::Batch(batch) => Some(batch),
                TextDrawOp::Block(_) => None,
            })
            .collect()
    }

    #[test]
    fn block_element_geometry_is_complete_for_unicode_range() {
        for codepoint in BLOCK_ELEMENTS_START..=BLOCK_ELEMENTS_END {
            let glyph = char::from_u32(codepoint).expect("valid block-element codepoint");
            assert!(
                block_element_geometry(glyph).is_some(),
                "missing geometry for U+{codepoint:04X}"
            );
        }
    }

    #[test]
    fn upper_half_block_geometry_covers_top_half() {
        let geometry = block_element_geometry('\u{2580}').expect("expected block geometry");
        assert_eq!(geometry.rect_count, 1);
        let rect = geometry.rects()[0];
        assert_eq!(rect.left, 0.0);
        assert_eq!(rect.top, 0.0);
        assert_eq!(rect.right, 1.0);
        assert_eq!(rect.bottom, 0.5);
        assert_eq!(rect.alpha, 1.0);
    }

    #[test]
    fn upper_half_block_bounds_are_pixel_snapped() {
        let geometry = block_element_geometry('\u{2580}').expect("expected block geometry");
        let rect = geometry.rects()[0];
        let cell_bounds = Bounds {
            origin: point(px(12.3), px(40.7)),
            size: Size {
                width: px(17.8),
                height: px(15.2),
            },
        };

        let snapped = snapped_block_rect_bounds(cell_bounds, rect).expect("expected bounds");

        let x: f32 = snapped.origin.x.into();
        let y: f32 = snapped.origin.y.into();
        let width: f32 = snapped.size.width.into();
        let height: f32 = snapped.size.height.into();
        assert_eq!(x.fract(), 0.0);
        assert_eq!(y.fract(), 0.0);
        assert_eq!(width.fract(), 0.0);
        assert_eq!(height.fract(), 0.0);
    }

    #[test]
    fn quad_bounds_are_pixel_snapped() {
        let bounds = Bounds {
            origin: point(px(3.4), px(7.6)),
            size: Size {
                width: px(9.2),
                height: px(10.3),
            },
        };

        let snapped = snapped_quad_bounds(bounds).expect("expected bounds");
        let x: f32 = snapped.origin.x.into();
        let y: f32 = snapped.origin.y.into();
        let width: f32 = snapped.size.width.into();
        let height: f32 = snapped.size.height.into();
        assert_eq!(x.fract(), 0.0);
        assert_eq!(y.fract(), 0.0);
        assert_eq!(width.fract(), 0.0);
        assert_eq!(height.fract(), 0.0);
    }

    #[test]
    fn transparent_clear_background_skips_clear_quad() {
        assert!(!should_paint_clear_bg(Hsla::transparent_black()));
        assert!(should_paint_clear_bg(test_color(0.1, 0.2, 0.3)));
    }

    #[test]
    fn fast_path_excludes_non_block_glyphs() {
        assert!(block_element_geometry('\u{2579}').is_none());
        assert!(block_element_geometry('A').is_none());
    }

    #[test]
    fn batches_merge_adjacent_cells_with_same_style() {
        let grid = test_grid(vec![test_cell(0, 0, 'a'), test_cell(1, 0, 'b')], None);
        let batches = collect_batches(&grid);
        assert_eq!(batches.len(), 1);
        assert_eq!(batches[0].row, 0);
        assert_eq!(batches[0].start_col, 0);
        assert_eq!(batches[0].text, "ab");
    }

    #[test]
    fn batches_split_on_row_change() {
        let grid = test_grid(vec![test_cell(0, 0, 'a'), test_cell(0, 1, 'b')], None);
        let batches = collect_batches(&grid);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].text, "a");
        assert_eq!(batches[1].text, "b");
        assert_eq!(batches[0].row, 0);
        assert_eq!(batches[1].row, 1);
    }

    #[test]
    fn batches_split_on_bold_or_color_change() {
        let first = test_cell(0, 0, 'a');
        let mut second = test_cell(1, 0, 'b');
        let mut third = test_cell(2, 0, 'c');
        second.bold = true;
        third.fg = test_color(0.8, 0.4, 0.3);
        let grid = test_grid(vec![first, second, third], None);
        let batches = collect_batches(&grid);
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].text, "a");
        assert_eq!(batches[1].text, "b");
        assert_eq!(batches[2].text, "c");
    }

    #[test]
    fn batches_split_on_hover_underline_boundary() {
        let grid = test_grid(
            vec![
                test_cell(0, 0, 'a'),
                test_cell(1, 0, 'b'),
                test_cell(2, 0, 'c'),
            ],
            Some((0, 1, 2)),
        );
        let batches = collect_batches(&grid);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].text, "a");
        assert!(batches[0].underline.is_none());
        assert_eq!(batches[1].text, "bc");
        assert!(batches[1].underline.is_some());
    }

    #[test]
    fn batches_split_on_non_render_text_cells_and_controls() {
        let mut spacer = test_cell(1, 0, 'x');
        spacer.render_text = false;
        let mut control = test_cell(2, 0, '\u{001B}');
        control.render_text = true;
        let grid = test_grid(
            vec![
                test_cell(0, 0, 'a'),
                spacer,
                control,
                test_cell(3, 0, ' '),
                test_cell(4, 0, '\0'),
                test_cell(5, 0, 'b'),
            ],
            None,
        );
        let batches = collect_batches(&grid);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].text, "a");
        assert_eq!(batches[1].text, "b");
    }

    #[test]
    fn batches_do_not_include_block_element_glyphs() {
        let grid = test_grid(
            vec![
                test_cell(0, 0, 'a'),
                test_cell(1, 0, '\u{2588}'),
                test_cell(2, 0, 'b'),
            ],
            None,
        );
        let batches = collect_batches(&grid);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].text, "a");
        assert_eq!(batches[1].text, "b");
    }

    #[test]
    fn batches_break_around_wide_char_spacer_boundaries() {
        let mut spacer = test_cell(1, 0, ' ');
        spacer.render_text = false;
        let grid = test_grid(
            vec![test_cell(0, 0, '你'), spacer, test_cell(2, 0, 'x')],
            None,
        );
        let batches = collect_batches(&grid);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].text, "你");
        assert_eq!(batches[1].text, "x");
    }

    #[test]
    fn draw_ops_interleave_text_and_block_in_cell_order() {
        let grid = test_grid(
            vec![
                test_cell(0, 0, 'a'),
                test_cell(1, 0, '\u{2588}'),
                test_cell(2, 0, 'b'),
            ],
            None,
        );
        let ops = collect_draw_ops(&grid);
        assert_eq!(ops.len(), 3);
        assert!(matches!(&ops[0], TextDrawOp::Batch(batch) if batch.text == "a"));
        assert!(matches!(&ops[1], TextDrawOp::Block(_)));
        assert!(matches!(&ops[2], TextDrawOp::Batch(batch) if batch.text == "b"));
    }

    #[test]
    fn draw_ops_flush_batch_before_block() {
        let grid = test_grid(
            vec![
                test_cell(0, 0, 'a'),
                test_cell(1, 0, 'b'),
                test_cell(2, 0, '\u{2588}'),
                test_cell(3, 0, 'c'),
                test_cell(4, 0, 'd'),
            ],
            None,
        );
        let ops = collect_draw_ops(&grid);
        assert_eq!(ops.len(), 3);
        assert!(matches!(&ops[0], TextDrawOp::Batch(batch) if batch.text == "ab"));
        assert!(matches!(&ops[1], TextDrawOp::Block(_)));
        assert!(matches!(&ops[2], TextDrawOp::Batch(batch) if batch.text == "cd"));
    }

    #[test]
    fn draw_ops_skip_non_drawable_and_preserve_subsequent_order() {
        let mut spacer = test_cell(1, 0, 'x');
        spacer.render_text = false;
        let mut control = test_cell(3, 0, '\u{001B}');
        control.render_text = true;
        let grid = test_grid(
            vec![
                test_cell(0, 0, 'a'),
                spacer,
                test_cell(2, 0, '\u{2588}'),
                control,
                test_cell(4, 0, 'b'),
            ],
            None,
        );
        let ops = collect_draw_ops(&grid);
        assert_eq!(ops.len(), 3);
        assert!(matches!(&ops[0], TextDrawOp::Batch(batch) if batch.text == "a"));
        assert!(matches!(&ops[1], TextDrawOp::Block(_)));
        assert!(matches!(&ops[2], TextDrawOp::Batch(batch) if batch.text == "b"));
    }

    #[test]
    fn draw_ops_preserve_row_boundaries_with_blocks() {
        let grid = test_grid(
            vec![
                test_cell(0, 0, 'a'),
                test_cell(1, 0, 'b'),
                test_cell(0, 1, 'c'),
                test_cell(1, 1, '\u{2588}'),
                test_cell(2, 1, 'd'),
            ],
            None,
        );
        let ops = collect_draw_ops(&grid);
        assert_eq!(ops.len(), 4);
        assert!(
            matches!(&ops[0], TextDrawOp::Batch(batch) if batch.text == "ab" && batch.row == 0)
        );
        assert!(matches!(&ops[1], TextDrawOp::Batch(batch) if batch.text == "c" && batch.row == 1));
        assert!(matches!(&ops[2], TextDrawOp::Block(block) if block.row == 1 && block.col == 1));
        assert!(matches!(&ops[3], TextDrawOp::Batch(batch) if batch.text == "d" && batch.row == 1));
    }

    #[test]
    fn block_draw_uses_same_fg_precedence_as_text() {
        let mut selected_text = test_cell(0, 0, 'x');
        selected_text.selected = true;
        let mut selected_block = test_cell(1, 0, '\u{2588}');
        selected_block.selected = true;
        let grid = test_grid(vec![selected_text, selected_block], None);
        let ops = collect_draw_ops(&grid);
        assert_eq!(ops.len(), 2);
        let text_fg = match &ops[0] {
            TextDrawOp::Batch(batch) => batch.fg,
            TextDrawOp::Block(_) => panic!("expected text batch"),
        };
        let block_fg = match &ops[1] {
            TextDrawOp::Block(block) => block.fg,
            TextDrawOp::Batch(_) => panic!("expected block draw"),
        };
        assert_eq!(text_fg, grid.selection_fg);
        assert_eq!(block_fg, grid.selection_fg);

        let mut cursor_block = test_cell(0, 0, '\u{2588}');
        cursor_block.selected = true;
        cursor_block.search_current = true;
        let mut grid = test_grid(vec![cursor_block], None);
        grid.cursor_cell = Some((0, 0));
        grid.cursor_visible = true;
        let ops = collect_draw_ops(&grid);
        let block_fg = match &ops[0] {
            TextDrawOp::Block(block) => block.fg,
            TextDrawOp::Batch(_) => panic!("expected block draw"),
        };
        assert_eq!(
            block_fg,
            Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.0,
                a: 1.0
            }
        );
    }

    #[test]
    fn dirty_rows_for_pass_includes_cursor_transition_rows() {
        let mut grid = test_grid(vec![test_cell(0, 0, 'a')], None);
        grid.rows = 5;
        grid.paint_damage = TerminalGridPaintDamage::Rows(vec![2usize].into());
        grid.cursor_cell = Some((0, 1));

        let mut cache = TerminalGridPaintCache {
            style_key: Some(grid.paint_style_key()),
            last_cursor_cell: Some((0, 4)),
            ..Default::default()
        };
        let (full, style_changed, dirty_rows) = grid.dirty_rows_for_pass(&mut cache);
        assert!(!full);
        assert!(!style_changed);
        assert_eq!(&*dirty_rows, &[1usize, 2usize, 4usize]);
    }

    #[test]
    fn blink_only_does_not_dirty_rows_for_line_cursor() {
        // Line cursor: toggling cursor_visible should NOT mark the cursor row dirty,
        // since the cursor quad is painted as an overlay and row draw ops are unchanged.
        let mut grid = test_grid(vec![test_cell(0, 0, 'a')], None);
        grid.rows = 3;
        grid.paint_damage = TerminalGridPaintDamage::None;
        grid.cursor_cell = Some((0, 1));
        grid.cursor_visible = false; // blink off
        grid.cursor_style = TerminalCursorStyle::Line;

        let mut cache = TerminalGridPaintCache {
            style_key: Some(grid.paint_style_key()),
            last_cursor_cell: Some((0, 1)), // same position
            last_cursor_visible: true,      // was visible
            ..Default::default()
        };
        let (full, style_changed, dirty_rows) = grid.dirty_rows_for_pass(&mut cache);
        assert!(!full);
        assert!(!style_changed);
        assert!(
            dirty_rows.is_empty(),
            "Line cursor blink should not dirty any rows"
        );
    }

    #[test]
    fn blink_only_dirties_cursor_row_for_block_cursor() {
        // Block cursor: toggling cursor_visible MUST mark the cursor row dirty,
        // since the text fg color at the cursor cell is baked into draw ops.
        let mut grid = test_grid(vec![test_cell(0, 0, 'a')], None);
        grid.rows = 3;
        grid.paint_damage = TerminalGridPaintDamage::None;
        grid.cursor_cell = Some((0, 1));
        grid.cursor_visible = false; // blink off
        grid.cursor_style = TerminalCursorStyle::Block;

        let mut cache = TerminalGridPaintCache {
            style_key: Some(grid.paint_style_key()),
            last_cursor_cell: Some((0, 1)), // same position
            last_cursor_visible: true,      // was visible
            ..Default::default()
        };
        let (full, style_changed, dirty_rows) = grid.dirty_rows_for_pass(&mut cache);
        assert!(!full);
        assert!(!style_changed);
        assert_eq!(
            &*dirty_rows,
            &[1usize],
            "Block cursor blink must dirty the cursor row"
        );
    }

    #[test]
    fn dirty_rows_for_pass_includes_hover_transition_rows() {
        let mut grid = test_grid(vec![test_cell(0, 0, 'a')], Some((3, 1, 2)));
        grid.rows = 5;
        grid.paint_damage = TerminalGridPaintDamage::None;
        let mut cache = TerminalGridPaintCache {
            style_key: Some(grid.paint_style_key()),
            last_hovered_link_range: Some((1, 0, 0)),
            ..Default::default()
        };
        let (full, style_changed, dirty_rows) = grid.dirty_rows_for_pass(&mut cache);
        assert!(!full);
        assert!(!style_changed);
        assert_eq!(&*dirty_rows, &[1usize, 3usize]);
    }

    #[test]
    fn dirty_rows_for_pass_forces_full_repaint_when_style_changes() {
        let grid = test_grid(vec![test_cell(0, 0, 'a')], None);
        let mut cache = TerminalGridPaintCache::default();
        let (full, style_changed, dirty_rows) = grid.dirty_rows_for_pass(&mut cache);
        assert!(full);
        assert!(style_changed);
        assert!(dirty_rows.is_empty());
    }

    #[test]
    fn row_background_spans_merge_contiguous_cells_with_same_fill() {
        let mut first = test_cell(0, 0, 'a');
        let mut second = test_cell(1, 0, 'b');
        let mut third = test_cell(2, 0, 'c');
        let mut fourth = test_cell(3, 0, 'd');
        let mut fifth = test_cell(4, 0, 'e');
        let shared_bg = test_color(0.6, 0.3, 0.2);
        first.bg = shared_bg;
        second.bg = shared_bg;
        third.search_match = true;
        fourth.search_match = true;
        fifth.bg = Hsla::transparent_black();

        let grid = test_grid(vec![first, second, third, fourth, fifth], None);
        let spans = grid.build_row_background_spans(grid.cells[0].as_slice(), &mut HashMap::new());
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].start_col, 0);
        assert_eq!(spans[0].end_col_exclusive, 2);
        assert_eq!(spans[0].color, shared_bg);
        assert_eq!(spans[1].start_col, 2);
        assert_eq!(spans[1].end_col_exclusive, 4);
        assert_eq!(spans[1].color, grid.search_match_bg);
    }

    #[test]
    fn row_background_spans_skip_default_background_that_matches_surface() {
        let mut default_bg_cell = test_cell(0, 0, 'a');
        let mut ansi_bg_cell = test_cell(1, 0, 'b');
        default_bg_cell.uses_terminal_default_bg = true;
        default_bg_cell.bg = test_color(0.2, 0.2, 0.2);
        ansi_bg_cell.bg = test_color(0.2, 0.2, 0.2);

        let mut grid = test_grid(vec![default_bg_cell, ansi_bg_cell], None);
        grid.terminal_surface_bg = test_color(0.2, 0.2, 0.2);
        let spans = grid.build_row_background_spans(grid.cells[0].as_slice(), &mut HashMap::new());

        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start_col, 1);
        assert_eq!(spans[0].end_col_exclusive, 2);
        assert_eq!(spans[0].color, test_color(0.2, 0.2, 0.2));
    }

    #[test]
    fn row_background_spans_include_transformed_default_background_cells() {
        let mut default_bg_cell = test_cell(0, 0, 'a');
        default_bg_cell.uses_terminal_default_bg = true;
        default_bg_cell.bg = test_color(0.2, 0.2, 0.2);

        let mut grid = test_grid(vec![default_bg_cell], None);
        grid.terminal_surface_bg = test_color(0.1, 0.1, 0.1);
        let spans = grid.build_row_background_spans(grid.cells[0].as_slice(), &mut HashMap::new());

        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start_col, 0);
        assert_eq!(spans[0].end_col_exclusive, 1);
        assert_eq!(spans[0].color, test_color(0.2, 0.2, 0.2));
    }

    #[test]
    fn upper_half_block_cells_keep_non_default_background_spans() {
        let mut half_block = test_cell(0, 0, '\u{2580}');
        half_block.bg = test_color(0.8, 0.4, 0.2);

        let grid = test_grid(vec![half_block], None);
        let spans = grid.build_row_background_spans(grid.cells[0].as_slice(), &mut HashMap::new());

        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].start_col, 0);
        assert_eq!(spans[0].end_col_exclusive, 1);
        assert_eq!(spans[0].color, test_color(0.8, 0.4, 0.2));
    }

    #[test]
    fn matching_previous_row_ops_ignores_row_index_for_shifted_content() {
        let old_grid = test_grid_rows(
            vec![vec![test_cell(0, 0, 'a')], vec![test_cell(0, 1, 'b')]],
            None,
        );
        let new_grid = test_grid_rows(
            vec![vec![test_cell(0, 0, 'b')], vec![test_cell(0, 1, 'c')]],
            None,
        );
        let cursor_fg = Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.0,
            a: 1.0,
        };
        let highlight_fg = Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.08,
            a: 1.0,
        };

        let previous_row_ops = vec![
            old_grid.rebuild_cached_row_ops(
                old_grid.cells[0].as_slice(),
                cursor_fg,
                highlight_fg,
                &mut HashMap::new(),
            ),
            old_grid.rebuild_cached_row_ops(
                old_grid.cells[1].as_slice(),
                cursor_fg,
                highlight_fg,
                &mut HashMap::new(),
            ),
        ];
        let next_row_ops = new_grid.rebuild_cached_row_ops(
            new_grid.cells[0].as_slice(),
            cursor_fg,
            highlight_fg,
            &mut HashMap::new(),
        );

        assert_eq!(
            find_matching_previous_row_ops_index(0, &next_row_ops, &previous_row_ops),
            Some(1)
        );
    }

    #[test]
    fn matching_previous_row_ops_rejects_hover_style_mismatches() {
        let previous_grid = test_grid(vec![test_cell(0, 0, 'a')], Some((0, 0, 0)));
        let next_grid = test_grid(vec![test_cell(0, 0, 'a')], None);
        let cursor_fg = Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.0,
            a: 1.0,
        };
        let highlight_fg = Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.08,
            a: 1.0,
        };

        let previous_row_ops = vec![previous_grid.rebuild_cached_row_ops(
            previous_grid.cells[0].as_slice(),
            cursor_fg,
            highlight_fg,
            &mut HashMap::new(),
        )];
        let next_row_ops = next_grid.rebuild_cached_row_ops(
            next_grid.cells[0].as_slice(),
            cursor_fg,
            highlight_fg,
            &mut HashMap::new(),
        );

        assert_eq!(
            find_matching_previous_row_ops_index(0, &next_row_ops, &previous_row_ops),
            None
        );
    }

    #[test]
    fn rebuild_cached_row_ops_initializes_shaped_line_slots_per_draw_op() {
        let grid = test_grid(
            vec![
                test_cell(0, 0, 'a'),
                test_cell(1, 0, '\u{2588}'),
                test_cell(2, 0, 'b'),
            ],
            None,
        );
        let row_ops = grid.rebuild_cached_row_ops(
            grid.cells[0].as_slice(),
            Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.0,
                a: 1.0,
            },
            Hsla {
                h: 0.0,
                s: 0.0,
                l: 0.08,
                a: 1.0,
            },
            &mut HashMap::new(),
        );

        assert_eq!(row_ops.draw_ops.len(), 3);
        assert_eq!(row_ops.shaped_lines.len(), 3);
        assert!(row_ops.shaped_lines.iter().all(Option::is_none));
    }

    #[test]
    fn rebuild_cached_rows_for_pass_clears_rows_missing_from_cells() {
        let mut grid = test_grid(vec![test_cell(0, 0, 'a')], None);
        grid.rows = 2;
        grid.cells = Arc::new(vec![Arc::new(vec![test_cell(0, 0, 'a')])]);

        let cursor_fg = Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.0,
            a: 1.0,
        };
        let highlight_fg = Hsla {
            h: 0.0,
            s: 0.0,
            l: 0.08,
            a: 1.0,
        };

        let stale_row_cells = vec![test_cell(0, 1, 'z')];
        let mut cache = TerminalGridPaintCache {
            row_ops: vec![
                CachedRowPaintOps::default(),
                grid.rebuild_cached_row_ops(
                    stale_row_cells.as_slice(),
                    cursor_fg,
                    highlight_fg,
                    &mut HashMap::new(),
                ),
            ],
            ..Default::default()
        };
        assert!(!cache.row_ops[1].draw_ops.is_empty());
        assert_eq!(
            cache.row_ops[1].shaped_lines.len(),
            cache.row_ops[1].draw_ops.len()
        );

        grid.rebuild_cached_rows_for_pass(
            &mut cache,
            false,
            false,
            &[1usize],
            cursor_fg,
            highlight_fg,
        );
        assert!(cache.row_ops[1].draw_ops.is_empty());
        assert!(cache.row_ops[1].background_spans.is_empty());
        assert!(cache.row_ops[1].shaped_lines.is_empty());
    }

    #[test]
    fn paint_cache_handle_clear_resets_seeded_rows() {
        let handle = TerminalGridPaintCacheHandle::default();
        handle.debug_seed_rows_for_tests(3);
        assert_eq!(handle.debug_row_cache_len_for_tests(), 3);
        handle.clear();
        assert_eq!(handle.debug_row_cache_len_for_tests(), 0);
    }

    #[test]
    fn dirty_rows_for_pass_row_ranges_extracts_rows_and_col_ranges() {
        let mut grid = test_grid(vec![test_cell(0, 0, 'a')], None);
        grid.rows = 5;
        grid.paint_damage = TerminalGridPaintDamage::RowRanges(vec![(1, 10, 20), (3, 5, 8)].into());

        let mut cache = TerminalGridPaintCache {
            style_key: Some(grid.paint_style_key()),
            ..Default::default()
        };
        cache.ensure_row_capacity(5);
        let (full, style_changed, dirty_rows) = grid.dirty_rows_for_pass(&mut cache);

        assert!(!full);
        assert!(!style_changed);
        assert_eq!(&*dirty_rows, &[1usize, 3usize]);
        assert_eq!(cache.dirty_col_ranges[1], Some((10, 20)));
        assert_eq!(cache.dirty_col_ranges[3], Some((5, 8)));
        assert_eq!(cache.dirty_col_ranges[0], None);
        assert_eq!(cache.dirty_col_ranges[2], None);
    }

    #[test]
    fn dirty_rows_for_pass_row_ranges_merges_spans_on_same_row() {
        let mut grid = test_grid(vec![test_cell(0, 0, 'a')], None);
        grid.rows = 3;
        // Two spans on row 1: cols 5-10 and cols 15-20 → should merge to 5-20
        grid.paint_damage =
            TerminalGridPaintDamage::RowRanges(vec![(1, 5, 10), (1, 15, 20)].into());

        let mut cache = TerminalGridPaintCache {
            style_key: Some(grid.paint_style_key()),
            ..Default::default()
        };
        cache.ensure_row_capacity(3);
        let (_, _, dirty_rows) = grid.dirty_rows_for_pass(&mut cache);

        // Row 1 appears once despite two spans
        assert_eq!(&*dirty_rows, &[1usize]);
        // Col ranges should be unioned: min(5,15)=5, max(10,20)=20
        assert_eq!(cache.dirty_col_ranges[1], Some((5, 20)));
    }

    #[test]
    fn draw_op_col_range_returns_correct_range_for_batch() {
        let batch = TextDrawOp::Batch(TextBatch::new(
            5, // start_col
            0, // row
            'a',
            TextBatchKey {
                bold: false,
                fg: Hsla::transparent_black(),
            },
            None,
        ));
        // Single char batch: range is (5, 5)
        assert_eq!(draw_op_col_range(&batch), (5, 5));
    }

    #[test]
    fn draw_op_col_range_returns_correct_range_for_block() {
        let block = TextDrawOp::Block(BlockDraw {
            row: 0,
            col: 7,
            geometry: block_element_geometry('\u{2580}').unwrap(),
            fg: Hsla::transparent_black(),
        });
        assert_eq!(draw_op_col_range(&block), (7, 7));
    }

    #[test]
    fn col_ranges_overlap_detects_overlapping_ranges() {
        assert!(col_ranges_overlap((0, 5), (3, 8)));
        assert!(col_ranges_overlap((3, 8), (0, 5)));
        assert!(col_ranges_overlap((5, 5), (5, 5)));
        assert!(col_ranges_overlap((0, 10), (5, 5)));
    }

    #[test]
    fn col_ranges_overlap_detects_non_overlapping_ranges() {
        assert!(!col_ranges_overlap((0, 4), (5, 10)));
        assert!(!col_ranges_overlap((5, 10), (0, 4)));
        assert!(!col_ranges_overlap((0, 0), (1, 1)));
    }

    #[test]
    fn dirty_rows_for_pass_row_ranges_resets_each_pass() {
        // Verify that dirty_col_ranges is cleared between passes (via ensure_row_capacity)
        let mut grid = test_grid(vec![test_cell(0, 0, 'a')], None);
        grid.rows = 3;
        grid.paint_damage = TerminalGridPaintDamage::RowRanges(vec![(1, 5, 10)].into());

        let mut cache = TerminalGridPaintCache {
            style_key: Some(grid.paint_style_key()),
            ..Default::default()
        };
        cache.ensure_row_capacity(3);
        grid.dirty_rows_for_pass(&mut cache);
        assert_eq!(cache.dirty_col_ranges[1], Some((5, 10)));

        // Second pass with different damage — must not carry over previous col range
        grid.paint_damage = TerminalGridPaintDamage::None;
        cache.ensure_row_capacity(3);
        let (_, _, dirty_rows) = grid.dirty_rows_for_pass(&mut cache);
        assert!(dirty_rows.is_empty());
        assert_eq!(
            cache.dirty_col_ranges[1], None,
            "col ranges must reset each pass"
        );
    }
}
