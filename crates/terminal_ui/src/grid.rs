use crate::render_metrics::{increment_grid_paint_count, increment_shape_line_calls};
use gpui::{
    App, Bounds, Element, Font, FontFeatures, FontWeight, Hsla, IntoElement, Pixels, SharedString,
    Size, TextAlign, TextRun, UnderlineStyle, Window, point, px, quad,
};
use std::sync::Arc;

/// Info needed to render a single cell.
#[derive(Clone)]
pub struct CellRenderInfo {
    pub col: usize,
    pub row: usize,
    pub char: char,
    pub fg: Hsla,
    pub bg: Hsla,
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

pub struct TerminalGrid {
    pub cells: TerminalGridRows,
    pub cell_size: Size<Pixels>,
    pub cols: usize,
    pub rows: usize,
    /// Clear color used to reset the grid surface every frame.
    pub clear_bg: Hsla,
    pub default_bg: Hsla,
    pub cursor_color: Hsla,
    pub selection_bg: Hsla,
    pub selection_fg: Hsla,
    pub search_match_bg: Hsla,
    pub search_current_bg: Hsla,
    pub hovered_link_range: Option<(usize, usize, usize)>,
    pub cursor_cell: Option<(usize, usize)>,
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

/// Check if two HSLA colors are approximately equal.
/// This is used to avoid painting cell backgrounds that match the terminal's default background,
/// which can cause visual artifacts due to slight color differences between ANSI colors.
fn colors_approximately_equal(a: &Hsla, b: &Hsla) -> bool {
    // Keep this tolerance tight: broad matching can skip legitimate near-default
    // app backgrounds and create visible seams/strips at edges.
    const EPSILON: f32 = 0.001;
    (a.h - b.h).abs() < EPSILON
        && (a.s - b.s).abs() < EPSILON
        && (a.l - b.l).abs() < EPSILON
        && (a.a - b.a).abs() < EPSILON
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
        let origin = bounds.origin;
        let grid_bounds = Bounds {
            origin,
            size: bounds.size,
        };

        // Always clear the full terminal surface first to avoid ghosting artifacts
        // when scrolled content reveals previously untouched cells.
        window.paint_quad(quad(
            grid_bounds,
            px(0.0),
            self.clear_bg,
            gpui::Edges::default(),
            Hsla::transparent_black(),
            gpui::BorderStyle::default(),
        ));

        // Paint background colors first.
        for row_cells in self.cells.iter() {
            for cell in row_cells.iter() {
                let x = origin.x + self.cell_size.width * cell.col as f32;
                let y = origin.y + self.cell_size.height * cell.row as f32;

                let cell_bounds = Bounds {
                    origin: point(x, y),
                    size: self.cell_size,
                };

                if cell.selected {
                    window.paint_quad(quad(
                        cell_bounds,
                        px(0.0),
                        self.selection_bg,
                        gpui::Edges::default(),
                        Hsla::transparent_black(),
                        gpui::BorderStyle::default(),
                    ));
                } else if cell.search_current {
                    window.paint_quad(quad(
                        cell_bounds,
                        px(0.0),
                        self.search_current_bg,
                        gpui::Edges::default(),
                        Hsla::transparent_black(),
                        gpui::BorderStyle::default(),
                    ));
                } else if cell.search_match {
                    window.paint_quad(quad(
                        cell_bounds,
                        px(0.0),
                        self.search_match_bg,
                        gpui::Edges::default(),
                        Hsla::transparent_black(),
                        gpui::BorderStyle::default(),
                    ));
                } else if cell.bg.a > 0.01
                    && !colors_approximately_equal(&cell.bg, &self.default_bg)
                {
                    window.paint_quad(quad(
                        cell_bounds,
                        px(0.0),
                        cell.bg,
                        gpui::Edges::default(),
                        Hsla::transparent_black(),
                        gpui::BorderStyle::default(),
                    ));
                }
            }
        }

        if let Some((cursor_col, cursor_row)) = self.cursor_cell {
            let x = origin.x + self.cell_size.width * cursor_col as f32;
            let y = origin.y + self.cell_size.height * cursor_row as f32;
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

        // Pre-create font structs to avoid cloning font_family for every batch.
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

        // Pre-compute cursor foreground color (black on cursor block)
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

        let draw_ops = self.collect_draw_ops(cursor_fg, highlight_fg);
        for op in draw_ops {
            match op {
                TextDrawOp::Batch(batch) => {
                    let x = origin.x + self.cell_size.width * batch.start_col as f32;
                    let y = origin.y + self.cell_size.height * batch.row as f32;
                    let font = if batch.bold { &font_bold } else { &font_normal };
                    let run = TextRun {
                        len: batch.text.len(),
                        font: font.clone(),
                        color: batch.fg,
                        background_color: None,
                        underline: batch.underline,
                        strikethrough: None,
                    };

                    increment_shape_line_calls();
                    let line = window.text_system().shape_line(
                        batch.text.into(),
                        self.font_size,
                        &[run],
                        // `force_width` is per-glyph advance, so terminal text must use one cell width.
                        Some(self.cell_size.width),
                    );
                    let _ = line.paint(
                        point(x, y),
                        self.cell_size.height,
                        TextAlign::Left,
                        None,
                        window,
                        cx,
                    );
                }
                TextDrawOp::Block(block) => {
                    let x = origin.x + self.cell_size.width * block.col as f32;
                    let y = origin.y + self.cell_size.height * block.row as f32;
                    let cell_bounds = Bounds {
                        origin: point(x, y),
                        size: self.cell_size,
                    };
                    paint_block_element_quad(window, cell_bounds, block.geometry, block.fg);
                }
            }
        }
    }
}

impl TerminalGrid {
    fn cell_count(&self) -> usize {
        self.cells.iter().map(|row| row.len()).sum()
    }

    fn iter_cells(&self) -> impl Iterator<Item = &CellRenderInfo> {
        self.cells.iter().flat_map(|row| row.iter())
    }

    fn cell_is_drawable_text(cell: &CellRenderInfo) -> bool {
        cell.render_text && cell.char != ' ' && cell.char != '\0' && !cell.char.is_control()
    }

    fn cell_fg_color(&self, cell: &CellRenderInfo, cursor_fg: Hsla, highlight_fg: Hsla) -> Hsla {
        if self.cursor_cell == Some((cell.col, cell.row))
            && self.cursor_style == TerminalCursorStyle::Block
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
        self.hovered_link_range.and_then(|(link_row, start_col, end_col)| {
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

    fn collect_draw_ops(&self, cursor_fg: Hsla, highlight_fg: Hsla) -> Vec<TextDrawOp> {
        let mut ops = Vec::new();
        ops.reserve(self.cell_count());
        let mut current: Option<TextBatch> = None;

        for cell in self.iter_cells() {
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
            current = Some(TextBatch::new(cell.col, cell.row, cell.char, key, underline));
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
            bold: false,
            render_text: true,
            selected: false,
            search_current: false,
            search_match: false,
        }
    }

    fn test_grid(cells: Vec<CellRenderInfo>, hovered: Option<(usize, usize, usize)>) -> TerminalGrid {
        TerminalGrid {
            cells: Arc::new(vec![Arc::new(cells)]),
            cell_size: Size {
                width: px(10.0),
                height: px(20.0),
            },
            cols: 120,
            rows: 40,
            clear_bg: Hsla::transparent_black(),
            default_bg: Hsla::transparent_black(),
            cursor_color: test_color(0.1, 0.1, 0.1),
            selection_bg: test_color(0.2, 0.2, 0.2),
            selection_fg: test_color(0.3, 0.3, 0.3),
            search_match_bg: test_color(0.4, 0.4, 0.4),
            search_current_bg: test_color(0.5, 0.5, 0.5),
            hovered_link_range: hovered,
            cursor_cell: None,
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
            vec![test_cell(0, 0, 'a'), test_cell(1, 0, 'b'), test_cell(2, 0, 'c')],
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
            vec![test_cell(0, 0, 'a'), test_cell(1, 0, '\u{2588}'), test_cell(2, 0, 'b')],
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
        let grid = test_grid(vec![test_cell(0, 0, '你'), spacer, test_cell(2, 0, 'x')], None);
        let batches = collect_batches(&grid);
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].text, "你");
        assert_eq!(batches[1].text, "x");
    }

    #[test]
    fn draw_ops_interleave_text_and_block_in_cell_order() {
        let grid = test_grid(
            vec![test_cell(0, 0, 'a'), test_cell(1, 0, '\u{2588}'), test_cell(2, 0, 'b')],
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
        assert!(matches!(&ops[0], TextDrawOp::Batch(batch) if batch.text == "ab" && batch.row == 0));
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
}
