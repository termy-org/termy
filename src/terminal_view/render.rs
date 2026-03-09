use super::scrollbar as terminal_scrollbar;
use super::*;
use crate::ui::scrollbar::{self as ui_scrollbar, ScrollbarPaintStyle};
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};
use gpui::prelude::FluentBuilder;
use std::sync::Arc;

fn cell_ranges_overlap(start_a: u32, end_a: u32, start_b: u32, end_b: u32) -> bool {
    start_a < end_b && start_b < end_a
}

fn blend_rgb_only(base: gpui::Rgba, target: gpui::Rgba, factor: f32) -> gpui::Rgba {
    let factor = factor.clamp(0.0, 1.0);
    let inv = 1.0 - factor;
    gpui::Rgba {
        r: (base.r * inv) + (target.r * factor),
        g: (base.g * inv) + (target.g * factor),
        b: (base.b * inv) + (target.b * factor),
        a: base.a,
    }
}

fn desaturate_rgb(color: gpui::Rgba, amount: f32) -> gpui::Rgba {
    let amount = amount.clamp(0.0, 1.0);
    if amount <= f32::EPSILON {
        return color;
    }
    let luma = (color.r * 0.2126) + (color.g * 0.7152) + (color.b * 0.0722);
    let inv = 1.0 - amount;
    gpui::Rgba {
        r: (color.r * inv) + (luma * amount),
        g: (color.g * inv) + (luma * amount),
        b: (color.b * inv) + (luma * amount),
        a: color.a,
    }
}

const COMMAND_PALETTE_BACKDROP_STRENGTH: f32 = 1.0;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct CellColorTransform {
    fg_blend: f32,
    bg_blend: f32,
    desaturate: f32,
}

impl CellColorTransform {
    fn is_active(self) -> bool {
        self.fg_blend > f32::EPSILON
            || self.bg_blend > f32::EPSILON
            || self.desaturate > f32::EPSILON
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneCacheUpdateStrategy {
    Reuse,
    Partial,
    Full,
}

#[cfg(debug_assertions)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RenderPassCacheStrategyCounts {
    full: u64,
    partial: u64,
    reuse: u64,
    dirty_span_count: u64,
    patched_cell_count: u64,
}

#[cfg(debug_assertions)]
impl RenderPassCacheStrategyCounts {
    fn record(&mut self, strategy: PaneCacheUpdateStrategy) {
        match strategy {
            PaneCacheUpdateStrategy::Reuse => {
                self.reuse = self.reuse.saturating_add(1);
            }
            PaneCacheUpdateStrategy::Partial => {
                self.partial = self.partial.saturating_add(1);
            }
            PaneCacheUpdateStrategy::Full => {
                self.full = self.full.saturating_add(1);
            }
        }
    }

    fn record_partial_work(&mut self, dirty_span_count: usize, patched_cell_count: usize) {
        self.dirty_span_count = self
            .dirty_span_count
            .saturating_add(usize_to_u64_saturating(dirty_span_count));
        self.patched_cell_count = self
            .patched_cell_count
            .saturating_add(usize_to_u64_saturating(patched_cell_count));
    }
}

#[cfg(debug_assertions)]
fn increment_render_count_counter(counters: &mut TerminalRenderMetricsCounters) {
    counters.render_count = counters.render_count.saturating_add(1);
}

#[cfg(debug_assertions)]
fn usize_to_u64_saturating(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn pane_cache_update_strategy(
    cache_has_cells: bool,
    cache_size_matches: bool,
    cache_offset_matches: bool,
    cache_key_matches: bool,
    damage: &TerminalDamageSnapshot,
) -> PaneCacheUpdateStrategy {
    if !cache_has_cells || !cache_size_matches || !cache_offset_matches || !cache_key_matches {
        return PaneCacheUpdateStrategy::Full;
    }
    match damage {
        TerminalDamageSnapshot::Full => PaneCacheUpdateStrategy::Full,
        TerminalDamageSnapshot::Partial(spans) if spans.is_empty() => {
            PaneCacheUpdateStrategy::Reuse
        }
        TerminalDamageSnapshot::Partial(_) => PaneCacheUpdateStrategy::Partial,
    }
}

fn finalized_cache_update_strategy(
    planned: PaneCacheUpdateStrategy,
    did_full_rebuild: bool,
) -> PaneCacheUpdateStrategy {
    if planned == PaneCacheUpdateStrategy::Partial && did_full_rebuild {
        PaneCacheUpdateStrategy::Full
    } else {
        planned
    }
}

fn paint_damage_from_dirty_spans(
    spans: &[TerminalDirtySpan],
    row_count: usize,
) -> TerminalGridPaintDamage {
    let mut rows = Vec::with_capacity(spans.len());
    for span in spans {
        if span.row < row_count {
            rows.push(span.row);
        }
    }
    rows.sort_unstable();
    rows.dedup();
    if rows.is_empty() {
        TerminalGridPaintDamage::None
    } else {
        TerminalGridPaintDamage::Rows(rows.into())
    }
}

#[derive(Clone, Copy)]
struct PaneCellBuildContext<'a> {
    colors: &'a TerminalColors,
    effective_background_opacity: f32,
    cell_color_transform: CellColorTransform,
    pane_focus_target_bg: gpui::Rgba,
    terminal_surface_bg: gpui::Rgba,
    selection_range: Option<(SelectionPos, SelectionPos)>,
    pane_search_results: Option<&'a termy_search::SearchResults>,
}

fn selection_range_contains(
    selection_range: Option<(SelectionPos, SelectionPos)>,
    col: usize,
    line: i32,
) -> bool {
    let Some((start, end)) = selection_range else {
        return false;
    };
    let here = (line, col);
    here >= (start.line, start.col) && here <= (end.line, end.col)
}

fn term_line_from_viewport_row(row: usize, display_offset: usize) -> Option<i32> {
    let row = i64::try_from(row).ok()?;
    let display_offset = i64::try_from(display_offset).ok()?;
    i32::try_from(row - display_offset).ok()
}

fn filtered_cursor_state(
    cursor_state: Option<TerminalCursorState>,
    pane_display_offset: usize,
    is_active_pane: bool,
    cols: usize,
    rows: usize,
) -> Option<TerminalCursorState> {
    cursor_state
        .filter(|_| pane_display_offset == 0 && is_active_pane)
        .filter(|cursor| cursor.col < cols && cursor.row < rows)
}

fn cursor_state_for_pane(
    terminal: &Terminal,
    pane_display_offset: usize,
    is_active_pane: bool,
    cols: usize,
    rows: usize,
) -> Option<TerminalCursorState> {
    filtered_cursor_state(
        terminal.cursor_state(),
        pane_display_offset,
        is_active_pane,
        cols,
        rows,
    )
}

type PaneRenderRow = Arc<Vec<CellRenderInfo>>;
type PaneRenderCells = Arc<Vec<PaneRenderRow>>;

fn pane_render_cells_match_dimensions(cells: &PaneRenderCells, cols: usize, rows: usize) -> bool {
    cells.len() == rows && cells.iter().all(|row_cells| row_cells.len() == cols)
}

fn merge_pane_render_rows(
    existing: &PaneRenderCells,
    rows: usize,
    cols: usize,
    updates: Vec<(usize, usize, CellRenderInfo)>,
) -> PaneRenderCells {
    if updates.is_empty() {
        return existing.clone();
    }

    let mut touched_rows = vec![None; rows];
    for (row, col, cell) in updates {
        if row >= rows || col >= cols {
            continue;
        }

        let row_cells = touched_rows[row].get_or_insert_with(|| existing[row].as_ref().clone());
        row_cells[col] = cell;
    }

    if touched_rows.iter().all(Option::is_none) {
        return existing.clone();
    }

    let mut merged_rows = Vec::with_capacity(rows);
    for row in 0..rows {
        if let Some(next_row) = touched_rows[row].take() {
            merged_rows.push(Arc::new(next_row));
        } else {
            merged_rows.push(existing[row].clone());
        }
    }

    Arc::new(merged_rows)
}

fn command_palette_backdrop_transform() -> CellColorTransform {
    let preset = pane_focus_preset(PaneFocusEffect::SoftSpotlight)
        .expect("soft spotlight pane focus preset must exist");
    CellColorTransform {
        fg_blend: preset.inactive_fg_blend * COMMAND_PALETTE_BACKDROP_STRENGTH,
        bg_blend: preset.inactive_bg_blend * COMMAND_PALETTE_BACKDROP_STRENGTH,
        desaturate: preset.inactive_desaturate * COMMAND_PALETTE_BACKDROP_STRENGTH,
    }
}

fn apply_cell_color_transform(
    fg: gpui::Rgba,
    bg: gpui::Rgba,
    transform: CellColorTransform,
    fg_blend_target: gpui::Rgba,
    bg_blend_target: gpui::Rgba,
) -> (gpui::Rgba, gpui::Rgba) {
    if !transform.is_active() {
        return (fg, bg);
    }

    let mut next_fg = fg;
    let mut next_bg = bg;
    if transform.fg_blend > f32::EPSILON {
        next_fg = blend_rgb_only(next_fg, fg_blend_target, transform.fg_blend);
    }
    if transform.bg_blend > f32::EPSILON {
        next_bg = blend_rgb_only(next_bg, bg_blend_target, transform.bg_blend);
    }
    if transform.desaturate > f32::EPSILON {
        next_fg = desaturate_rgb(next_fg, transform.desaturate);
        next_bg = desaturate_rgb(next_bg, transform.desaturate);
    }
    (next_fg, next_bg)
}

fn effective_pane_focus_active_border_alpha(
    active_border_alpha: f32,
    runtime_uses_tmux: bool,
    tmux_show_active_pane_border: bool,
) -> f32 {
    if runtime_uses_tmux && !tmux_show_active_pane_border {
        return 0.0;
    }
    active_border_alpha
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TerminalScrollbarOverlayFrame {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

fn terminal_scrollbar_overlay_frame(
    surface: TerminalViewportGeometry,
) -> TerminalScrollbarOverlayFrame {
    let surface_width = surface.width.max(0.0);
    let effective_gutter = TERMINAL_SCROLLBAR_GUTTER_WIDTH.min(surface_width);
    let left = (surface.origin_x + surface_width - effective_gutter).max(surface.origin_x);
    TerminalScrollbarOverlayFrame {
        left,
        top: surface.origin_y,
        width: effective_gutter,
        height: surface.height,
    }
}

fn terminal_scrollbar_track_width(frame_width: f32) -> f32 {
    TERMINAL_SCROLLBAR_TRACK_WIDTH.min(frame_width.max(0.0))
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TerminalView {
    fn pane_right_gap_cells(pane: &TerminalPane, panes: &[TerminalPane]) -> Option<u32> {
        let pane_right = u32::from(pane.left) + u32::from(pane.width);
        let pane_top = u32::from(pane.top);
        let pane_bottom = pane_top + u32::from(pane.height);

        panes
            .iter()
            .filter_map(|candidate| {
                if candidate.id == pane.id {
                    return None;
                }

                let candidate_left = u32::from(candidate.left);
                if candidate_left < pane_right {
                    return None;
                }

                let candidate_top = u32::from(candidate.top);
                let candidate_bottom = candidate_top + u32::from(candidate.height);
                if !cell_ranges_overlap(pane_top, pane_bottom, candidate_top, candidate_bottom) {
                    return None;
                }

                Some(candidate_left.saturating_sub(pane_right))
            })
            .min()
    }

    fn pane_bottom_gap_cells(pane: &TerminalPane, panes: &[TerminalPane]) -> Option<u32> {
        let pane_left = u32::from(pane.left);
        let pane_right = pane_left + u32::from(pane.width);
        let pane_bottom = u32::from(pane.top) + u32::from(pane.height);

        panes
            .iter()
            .filter_map(|candidate| {
                if candidate.id == pane.id {
                    return None;
                }

                let candidate_top = u32::from(candidate.top);
                if candidate_top < pane_bottom {
                    return None;
                }

                let candidate_left = u32::from(candidate.left);
                let candidate_right = candidate_left + u32::from(candidate.width);
                if !cell_ranges_overlap(pane_left, pane_right, candidate_left, candidate_right) {
                    return None;
                }

                Some(candidate_top.saturating_sub(pane_bottom))
            })
            .min()
    }

    fn pane_render_cache_key(
        &self,
        is_active_pane: bool,
        search_active: bool,
        cell_color_transform: CellColorTransform,
        effective_background_opacity: f32,
    ) -> TerminalPaneRenderCacheKey {
        let (search_results_revision, search_position) = if search_active && is_active_pane {
            let results = self.search_state.results();
            (
                Some(self.search_state.results_revision()),
                results.position(),
            )
        } else {
            (None, None)
        };

        TerminalPaneRenderCacheKey {
            is_active_pane,
            selection_range: is_active_pane.then(|| self.selection_range()).flatten(),
            search_results_revision,
            search_position,
            effective_background_opacity_bits: effective_background_opacity.to_bits(),
            color_transform: TerminalPaneCellColorTransformKey {
                fg_blend_bits: cell_color_transform.fg_blend.to_bits(),
                bg_blend_bits: cell_color_transform.bg_blend.to_bits(),
                desaturate_bits: cell_color_transform.desaturate.to_bits(),
            },
        }
    }

    fn build_cell_render_info(
        &self,
        col: usize,
        row: usize,
        term_line: i32,
        cell_content: &alacritty_terminal::term::cell::Cell,
        context: PaneCellBuildContext<'_>,
    ) -> CellRenderInfo {
        let mut fg = context.colors.convert(cell_content.fg);
        let mut bg = context.colors.convert(cell_content.bg);
        if cell_content.flags.contains(Flags::INVERSE) {
            std::mem::swap(&mut fg, &mut bg);
        }
        if cell_content.flags.contains(Flags::DIM) {
            fg.r *= DIM_TEXT_FACTOR;
            fg.g *= DIM_TEXT_FACTOR;
            fg.b *= DIM_TEXT_FACTOR;
        }
        bg.a *= context.effective_background_opacity;
        (fg, bg) = apply_cell_color_transform(
            fg,
            bg,
            context.cell_color_transform,
            context.pane_focus_target_bg,
            context.terminal_surface_bg,
        );

        let (search_current, search_match) = if let Some(results) = context.pane_search_results {
            let is_current = results.is_current_match(term_line, col);
            let is_any = results.is_any_match(term_line, col);
            (is_current, is_any && !is_current)
        } else {
            (false, false)
        };

        CellRenderInfo {
            col,
            row,
            char: cell_content.c,
            fg: fg.into(),
            bg: bg.into(),
            bold: cell_content.flags.contains(Flags::BOLD),
            render_text: !cell_content.flags.intersects(
                Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER | Flags::HIDDEN,
            ),
            selected: selection_range_contains(context.selection_range, col, term_line),
            search_current,
            search_match,
        }
    }

    fn rebuild_pane_render_cache(
        &self,
        terminal: &Terminal,
        cols: usize,
        rows: usize,
        display_offset: usize,
        context: PaneCellBuildContext<'_>,
    ) -> PaneRenderCells {
        if rows == 0 {
            return Arc::new(Vec::new());
        }
        if cols == 0 {
            return Arc::new((0..rows).map(|_| Arc::new(Vec::new())).collect());
        }

        let mut row_cells: Vec<Vec<CellRenderInfo>> =
            (0..rows).map(|_| Vec::with_capacity(cols)).collect();
        let mut expected_row = 0usize;
        let mut expected_col = 0usize;
        let mut ordering_failed = false;

        let _ = terminal.for_each_renderable_cell(
            |cell_display_offset, term_line, col, cell_content| {
                if ordering_failed || cell_display_offset != display_offset {
                    return;
                }
                if col >= cols {
                    ordering_failed = true;
                    return;
                }
                let Some(row) = Self::viewport_row_from_term_line(term_line, cell_display_offset)
                else {
                    ordering_failed = true;
                    return;
                };
                if row >= rows || row != expected_row || col != expected_col {
                    ordering_failed = true;
                    return;
                }

                row_cells[row].push(self.build_cell_render_info(
                    col,
                    row,
                    term_line,
                    cell_content,
                    context,
                ));

                expected_col += 1;
                if expected_col == cols {
                    expected_col = 0;
                    expected_row += 1;
                }
            },
        );

        let fully_populated = expected_row == rows
            && expected_col == 0
            && row_cells.iter().all(|row| row.len() == cols);
        if !ordering_failed && fully_populated {
            return Arc::new(row_cells.into_iter().map(Arc::new).collect());
        }

        self.rebuild_pane_render_cache_fallback(terminal, cols, rows, display_offset, context)
    }

    fn rebuild_pane_render_cache_fallback(
        &self,
        terminal: &Terminal,
        cols: usize,
        rows: usize,
        display_offset: usize,
        context: PaneCellBuildContext<'_>,
    ) -> PaneRenderCells {
        let mut default_bg = context.colors.background;
        default_bg.a *= context.effective_background_opacity;
        let (default_fg, default_bg) = apply_cell_color_transform(
            context.colors.foreground,
            default_bg,
            context.cell_color_transform,
            context.pane_focus_target_bg,
            context.terminal_surface_bg,
        );
        let mut rows_cache = Vec::with_capacity(rows);
        for row in 0..rows {
            let mut row_cells = Vec::with_capacity(cols);
            for col in 0..cols {
                row_cells.push(CellRenderInfo {
                    col,
                    row,
                    char: ' ',
                    fg: default_fg.into(),
                    bg: default_bg.into(),
                    bold: false,
                    render_text: false,
                    selected: false,
                    search_current: false,
                    search_match: false,
                });
            }
            rows_cache.push(row_cells);
        }

        let _ = terminal.for_each_renderable_cell(
            |cell_display_offset, term_line, col, cell_content| {
                if cell_display_offset != display_offset || col >= cols {
                    return;
                }
                let Some(row) = Self::viewport_row_from_term_line(term_line, cell_display_offset)
                else {
                    return;
                };
                if row >= rows {
                    return;
                }

                rows_cache[row][col] =
                    self.build_cell_render_info(col, row, term_line, cell_content, context);
            },
        );

        Arc::new(rows_cache.into_iter().map(Arc::new).collect())
    }

    #[allow(clippy::too_many_arguments)]
    fn patch_pane_render_cache(
        &self,
        terminal: &Terminal,
        cols: usize,
        rows: usize,
        display_offset: usize,
        cells: &mut PaneRenderCells,
        spans: &[TerminalDirtySpan],
        context: PaneCellBuildContext<'_>,
    ) -> (usize, bool) {
        if !pane_render_cells_match_dimensions(cells, cols, rows) {
            *cells = self.rebuild_pane_render_cache(terminal, cols, rows, display_offset, context);
            return (0, true);
        }

        let mut updates = Vec::new();
        let _ = terminal.with_grid(|grid| {
            let Some(screen_lines) = i32::try_from(grid.screen_lines()).ok() else {
                return;
            };
            let Some(total_lines) = i32::try_from(grid.total_lines()).ok() else {
                return;
            };
            let min_line = -(total_lines - screen_lines);
            let max_line = screen_lines - 1;

            for span in spans {
                if span.row >= rows || cols == 0 {
                    continue;
                }

                let Some(term_line) = term_line_from_viewport_row(span.row, display_offset) else {
                    continue;
                };
                if term_line < min_line || term_line > max_line {
                    continue;
                }

                let row = span.row;
                let line_ref = &grid[Line(term_line)];
                let left_col = span.left_col.min(cols.saturating_sub(1));
                let right_col = span.right_col.min(cols.saturating_sub(1));
                if left_col > right_col {
                    continue;
                }

                for col in left_col..=right_col {
                    let cell_content = &line_ref[Column(col)];
                    updates.push((
                        row,
                        col,
                        self.build_cell_render_info(col, row, term_line, cell_content, context),
                    ));
                }
            }
        });

        if updates.is_empty() {
            return (0, false);
        }

        let patched_cell_count = updates.len();
        *cells = merge_pane_render_rows(cells, rows, cols, updates);
        (patched_cell_count, false)
    }

    #[allow(clippy::too_many_arguments)]
    fn update_pane_render_cache(
        &self,
        terminal: &Terminal,
        cols: usize,
        rows: usize,
        display_offset: usize,
        cache: &mut TerminalPaneRenderCache,
        cache_key: TerminalPaneRenderCacheKey,
        context: PaneCellBuildContext<'_>,
        #[cfg(debug_assertions)] render_pass_cache_counts: &mut RenderPassCacheStrategyCounts,
    ) -> (
        PaneRenderCells,
        PaneCacheUpdateStrategy,
        TerminalGridPaintDamage,
    ) {
        let damage = terminal.take_damage_snapshot();
        let mut strategy = pane_cache_update_strategy(
            !cache.cells.is_empty(),
            cache.cols == cols && cache.rows == rows,
            cache.display_offset == display_offset,
            cache.key.as_ref() == Some(&cache_key),
            &damage,
        );
        let mut paint_damage = match strategy {
            PaneCacheUpdateStrategy::Reuse => TerminalGridPaintDamage::None,
            PaneCacheUpdateStrategy::Full => TerminalGridPaintDamage::Full,
            PaneCacheUpdateStrategy::Partial => match &damage {
                TerminalDamageSnapshot::Partial(spans) => {
                    paint_damage_from_dirty_spans(spans, rows)
                }
                TerminalDamageSnapshot::Full => TerminalGridPaintDamage::Full,
            },
        };

        match strategy {
            PaneCacheUpdateStrategy::Reuse => {}
            PaneCacheUpdateStrategy::Full => {
                cache.cells =
                    self.rebuild_pane_render_cache(terminal, cols, rows, display_offset, context);
            }
            PaneCacheUpdateStrategy::Partial => {
                let TerminalDamageSnapshot::Partial(spans) = damage else {
                    cache.cells = self.rebuild_pane_render_cache(
                        terminal,
                        cols,
                        rows,
                        display_offset,
                        context,
                    );
                    cache.cols = cols;
                    cache.rows = rows;
                    cache.display_offset = display_offset;
                    cache.key = Some(cache_key);
                    return (
                        cache.cells.clone(),
                        PaneCacheUpdateStrategy::Full,
                        TerminalGridPaintDamage::Full,
                    );
                };
                #[cfg_attr(not(debug_assertions), allow(unused_variables))]
                let (patched_cell_count, did_full_rebuild) = self.patch_pane_render_cache(
                    terminal,
                    cols,
                    rows,
                    display_offset,
                    &mut cache.cells,
                    &spans,
                    context,
                );
                strategy = finalized_cache_update_strategy(strategy, did_full_rebuild);
                if strategy == PaneCacheUpdateStrategy::Full {
                    paint_damage = TerminalGridPaintDamage::Full;
                } else {
                    #[cfg(debug_assertions)]
                    if patched_cell_count > 0 {
                        render_pass_cache_counts
                            .record_partial_work(spans.len(), patched_cell_count);
                    }
                }
            }
        }

        cache.cols = cols;
        cache.rows = rows;
        cache.display_offset = display_offset;
        cache.key = Some(cache_key);
        (cache.cells.clone(), strategy, paint_damage)
    }

    #[allow(clippy::too_many_arguments)]
    fn build_terminal_grid_from_cache(
        &self,
        cells: PaneRenderCells,
        paint_cache: TerminalGridPaintCacheHandle,
        paint_damage: TerminalGridPaintDamage,
        cell_size: Size<Pixels>,
        cols: usize,
        rows: usize,
        colors: &TerminalColors,
        hovered_link_range: Option<(usize, usize, usize)>,
        font_family: SharedString,
        font_size: Pixels,
        cursor_style: TerminalCursorStyle,
        cursor_cell: Option<(usize, usize)>,
    ) -> TerminalGrid {
        let mut selection_bg = colors.cursor;
        selection_bg.a = SELECTION_BG_ALPHA;
        let selection_fg = colors.background;
        let default_cell_bg: gpui::Hsla = {
            let mut bg = colors.background;
            bg.a = self.scaled_background_alpha(bg.a);
            bg.into()
        };
        TerminalGrid {
            cells,
            paint_cache,
            paint_damage,
            cell_size,
            cols,
            rows,
            clear_bg: gpui::Hsla::transparent_black(),
            default_bg: default_cell_bg,
            cursor_color: colors.cursor.into(),
            selection_bg: selection_bg.into(),
            selection_fg: selection_fg.into(),
            search_match_bg: gpui::Hsla {
                h: 0.14,
                s: 0.92,
                l: 0.62,
                a: 0.62,
            },
            search_current_bg: gpui::Hsla {
                h: 0.09,
                s: 0.98,
                l: 0.56,
                a: 0.86,
            },
            hovered_link_range,
            cursor_cell,
            font_family,
            font_size,
            cursor_style,
        }
    }

    #[cfg(debug_assertions)]
    fn record_render_metrics_for_pass(&mut self, cache_counts: RenderPassCacheStrategyCounts) {
        if !self.render_metrics.enabled {
            return;
        }
        increment_render_count_counter(&mut self.render_metrics.counters);
        self.render_metrics.counters.cache_full_count = self
            .render_metrics
            .counters
            .cache_full_count
            .saturating_add(cache_counts.full);
        self.render_metrics.counters.cache_partial_count = self
            .render_metrics
            .counters
            .cache_partial_count
            .saturating_add(cache_counts.partial);
        self.render_metrics.counters.cache_reuse_count = self
            .render_metrics
            .counters
            .cache_reuse_count
            .saturating_add(cache_counts.reuse);
        self.render_metrics.counters.dirty_span_count = self
            .render_metrics
            .counters
            .dirty_span_count
            .saturating_add(cache_counts.dirty_span_count);
        self.render_metrics.counters.patched_cell_count = self
            .render_metrics
            .counters
            .patched_cell_count
            .saturating_add(cache_counts.patched_cell_count);
    }

    #[cfg(debug_assertions)]
    fn maybe_emit_render_metrics_log(&mut self, now: Instant) {
        if !self.render_metrics.enabled {
            return;
        }

        if let Some(last_emit) = self.render_metrics.last_emit_at
            && now.duration_since(last_emit) < self.render_metrics.log_interval
        {
            return;
        }

        let terminal_ui_snapshot = terminal_ui_render_metrics_snapshot();
        let counters_delta = self
            .render_metrics
            .counters
            .saturating_sub(self.render_metrics.last_emit_counters);
        let terminal_ui_delta =
            terminal_ui_snapshot.saturating_sub(self.render_metrics.last_emit_terminal_ui);
        let dt_ms = self
            .render_metrics
            .last_emit_at
            .map(|last_emit| now.duration_since(last_emit).as_millis())
            .unwrap_or(0);

        log::info!(
            "render_metrics dt_ms={} render={} grid_paint={} full={} partial={} reuse={} dirty_span={} patched_cell={} shape_line={} total_render={} total_grid_paint={} total_full={} total_partial={} total_reuse={} total_dirty_span={} total_patched_cell={} total_shape_line={}",
            dt_ms,
            counters_delta.render_count,
            terminal_ui_delta.grid_paint_count,
            counters_delta.cache_full_count,
            counters_delta.cache_partial_count,
            counters_delta.cache_reuse_count,
            counters_delta.dirty_span_count,
            counters_delta.patched_cell_count,
            terminal_ui_delta.shape_line_calls,
            self.render_metrics.counters.render_count,
            terminal_ui_snapshot.grid_paint_count,
            self.render_metrics.counters.cache_full_count,
            self.render_metrics.counters.cache_partial_count,
            self.render_metrics.counters.cache_reuse_count,
            self.render_metrics.counters.dirty_span_count,
            self.render_metrics.counters.patched_cell_count,
            terminal_ui_snapshot.shape_line_calls,
        );

        self.render_metrics.last_emit_counters = self.render_metrics.counters;
        self.render_metrics.last_emit_terminal_ui = terminal_ui_snapshot;
        self.render_metrics.last_emit_at = Some(now);
    }

    fn refresh_terminal_scrollbar_marker_cache(
        &mut self,
        layout: terminal_scrollbar::TerminalScrollbarLayout,
        marker_height: f32,
    ) -> Option<f32> {
        if !self.search_open {
            self.clear_terminal_scrollbar_marker_cache();
            return None;
        }

        let marker_height = marker_height.max(0.0);
        let marker_top_limit =
            terminal_scrollbar::marker_top_limit(layout.metrics.track_height, marker_height);
        let cache_key = TerminalScrollbarMarkerCacheKey {
            results_revision: self.search_state.results_revision(),
            history_size: layout.history_size,
            viewport_rows: layout.viewport_rows,
            marker_top_limit_bucket: terminal_scrollbar::marker_top_limit_bucket(marker_top_limit),
        };
        let rebuild_markers = self.terminal_scrollbar_marker_cache.key.as_ref() != Some(&cache_key);

        let (is_empty, current_line, new_marker_tops) = {
            let results = self.search_state.results();
            if results.is_empty() {
                (true, None, None)
            } else {
                let current_line = results.current().map(|current| current.line);
                let new_marker_tops = rebuild_markers.then(|| {
                    terminal_scrollbar::deduped_marker_tops(
                        results
                            .matches()
                            .iter()
                            .map(|search_match| search_match.line),
                        layout.history_size,
                        layout.viewport_rows,
                        marker_height,
                        marker_top_limit,
                    )
                });
                (false, current_line, new_marker_tops)
            }
        };

        if is_empty {
            self.clear_terminal_scrollbar_marker_cache();
            return None;
        }

        if let Some(marker_tops) = new_marker_tops {
            self.terminal_scrollbar_marker_cache.marker_tops = marker_tops;
            self.terminal_scrollbar_marker_cache.key = Some(cache_key);
        }

        current_line.map(|line| {
            terminal_scrollbar::marker_top_for_line(
                line,
                layout.history_size,
                layout.viewport_rows,
                marker_top_limit,
            )
        })
    }

    fn render_terminal_scrollbar_overlay(
        &mut self,
        surface: TerminalViewportGeometry,
        layout: terminal_scrollbar::TerminalScrollbarLayout,
        force_visible: bool,
    ) -> Option<AnyElement> {
        let now = Instant::now();
        let force_visible = force_visible
            && self.terminal_scrollbar_mode() != ui_scrollbar::ScrollbarVisibilityMode::AlwaysOff;
        let alpha = if force_visible {
            1.0
        } else {
            self.terminal_scrollbar_alpha(now)
        };
        if alpha <= f32::EPSILON && !self.terminal_scrollbar_visibility_controller.is_dragging() {
            return None;
        }
        let overlay_style = self.overlay_style();
        let gutter_bg = overlay_style.panel_background(TERMINAL_SCROLLBAR_GUTTER_ALPHA);
        let frame = terminal_scrollbar_overlay_frame(surface);
        let track_width = terminal_scrollbar_track_width(frame.width);
        let style = ScrollbarPaintStyle {
            width: track_width,
            track_radius: TERMINAL_SCROLLBAR_TRACK_RADIUS,
            thumb_radius: TERMINAL_SCROLLBAR_THUMB_RADIUS,
            thumb_inset: TERMINAL_SCROLLBAR_THUMB_INSET,
            marker_inset: TERMINAL_SCROLLBAR_THUMB_INSET,
            marker_radius: TERMINAL_SCROLLBAR_THUMB_RADIUS,
            track_color: self.scrollbar_color(overlay_style, TERMINAL_SCROLLBAR_TRACK_ALPHA),
            thumb_color: self.scrollbar_color(overlay_style, TERMINAL_SCROLLBAR_THUMB_ALPHA),
            active_thumb_color: self
                .scrollbar_color(overlay_style, TERMINAL_SCROLLBAR_THUMB_ACTIVE_ALPHA),
            marker_color: Some(
                self.scrollbar_color(overlay_style, TERMINAL_SCROLLBAR_MATCH_MARKER_ALPHA),
            ),
            current_marker_color: Some(
                overlay_style.panel_cursor(TERMINAL_SCROLLBAR_CURRENT_MARKER_ALPHA),
            ),
        }
        .scale_alpha(alpha);

        let current_marker_top =
            self.refresh_terminal_scrollbar_marker_cache(layout, TERMINAL_SCROLLBAR_MARKER_HEIGHT);
        let marker_tops = &self.terminal_scrollbar_marker_cache.marker_tops;

        Some(
            div()
                .id("terminal-scrollbar-overlay")
                .absolute()
                .left(px(frame.left))
                .top(px(frame.top))
                .w(px(frame.width))
                .h(px(frame.height))
                .bg(gutter_bg)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right_0()
                        .w(px(track_width))
                        .child(ui_scrollbar::render_vertical(
                            "terminal-scrollbar",
                            layout.metrics,
                            style,
                            self.terminal_scrollbar_visibility_controller.is_dragging(),
                            marker_tops,
                            current_marker_top,
                            TERMINAL_SCROLLBAR_MARKER_HEIGHT,
                        )),
                )
                .into_any_element(),
        )
    }

    #[cfg(target_os = "macos")]
    fn render_update_banner(
        &mut self,
        state: &UpdateState,
        colors: &TerminalColors,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let model = termy_auto_update_ui::UpdateBannerModel::from_state(state)?;
        let updater_weak = self.auto_updater.as_ref().map(|e| e.downgrade());

        let mut banner_bg = colors.background;
        banner_bg.a = 0.88;
        let mut border_color = colors.foreground;
        border_color.a = 0.16;
        let mut muted_text = colors.foreground;
        muted_text.a = 0.72;

        let tone = match model.tone {
            termy_auto_update_ui::UpdateBannerTone::Info => {
                let mut color = colors.cursor;
                color.a = 0.22;
                color
            }
            termy_auto_update_ui::UpdateBannerTone::Success => gpui::Rgba {
                r: 0.25,
                g: 0.66,
                b: 0.36,
                a: 0.24,
            },
            termy_auto_update_ui::UpdateBannerTone::Error => gpui::Rgba {
                r: 0.85,
                g: 0.31,
                b: 0.31,
                a: 0.24,
            },
        };

        let mut actions = div().flex().items_center().gap(px(6.0));
        for button in model.buttons {
            let action = button.action;
            let updater_weak = updater_weak.clone();
            let (button_bg, button_text, button_border) = match button.style {
                termy_auto_update_ui::UpdateButtonStyle::Primary => {
                    let mut bg = colors.cursor;
                    bg.a = 0.96;
                    (
                        bg,
                        colors.background,
                        gpui::Rgba {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        },
                    )
                }
                termy_auto_update_ui::UpdateButtonStyle::Secondary => {
                    let mut bg = colors.foreground;
                    bg.a = 0.08;
                    let mut border = colors.foreground;
                    border.a = 0.2;
                    (bg, colors.foreground, border)
                }
            };

            actions = actions.child(
                div()
                    .px(px(9.0))
                    .py(px(3.0))
                    .rounded_md()
                    .bg(button_bg)
                    .border_1()
                    .border_color(button_border)
                    .text_size(px(11.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(button_text)
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| match action {
                            termy_auto_update_ui::UpdateBannerAction::Install => {
                                if let Some(ref weak) = updater_weak
                                    && let Some(entity) = weak.upgrade()
                                {
                                    AutoUpdater::install(entity.downgrade(), cx);
                                    termy_toast::info("Downloading update...");
                                    this.notify_overlay(cx);
                                }
                            }
                            termy_auto_update_ui::UpdateBannerAction::CompleteInstall => {
                                if let Some(ref weak) = updater_weak
                                    && let Some(entity) = weak.upgrade()
                                {
                                    AutoUpdater::complete_install(entity.downgrade(), cx);
                                    termy_toast::info("Starting installation...");
                                    this.notify_overlay(cx);
                                }
                            }
                            termy_auto_update_ui::UpdateBannerAction::Restart => {
                                match this.restart_application_with_persist() {
                                    Ok(()) => {
                                        this.allow_quit_without_prompt = true;
                                        cx.quit();
                                    }
                                    Err(error) => {
                                        termy_toast::error(format!("Restart failed: {}", error));
                                        this.notify_overlay(cx);
                                    }
                                }
                            }
                            termy_auto_update_ui::UpdateBannerAction::Dismiss => {
                                if let Some(ref weak) = updater_weak
                                    && let Some(entity) = weak.upgrade()
                                {
                                    entity.update(cx, |updater, cx| updater.dismiss(cx));
                                }
                            }
                        }),
                    )
                    .child(button.label),
            );
        }

        let progress_element = model.progress_percent.map(|progress| {
            let mut progress_track = colors.foreground;
            progress_track.a = 0.14;
            let progress_width = 130.0;
            let fill_width = (f32::from(progress) / 100.0) * progress_width;

            div()
                .mt(px(2.0))
                .w(px(progress_width))
                .h(px(4.0))
                .rounded_full()
                .bg(progress_track)
                .child(
                    div()
                        .h_full()
                        .w(px(fill_width.max(0.0)))
                        .rounded_full()
                        .bg(colors.cursor),
                )
                .into_any()
        });

        Some(
            div()
                .id("update-banner")
                .w_full()
                .h(px(UPDATE_BANNER_HEIGHT))
                .flex_none()
                .bg(banner_bg)
                .border_b_1()
                .border_color(border_color)
                .child(
                    div()
                        .size_full()
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .px(px(8.0))
                                        .py(px(3.0))
                                        .rounded_full()
                                        .bg(tone)
                                        .text_size(px(10.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(colors.foreground)
                                        .child(model.badge),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(colors.foreground)
                                                .child(model.message),
                                        )
                                        .children(model.detail.map(|detail| {
                                            div()
                                                .text_size(px(10.0))
                                                .text_color(muted_text)
                                                .child(detail)
                                                .into_any()
                                        }))
                                        .children(progress_element),
                                ),
                        )
                        .child(actions),
                )
                .into_any(),
        )
    }

    fn render_toast_overlay(
        &mut self,
        colors: &TerminalColors,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.toast_manager.active().is_empty() {
            return None;
        }

        let mut container = div().flex().flex_col().gap(px(6.0));
        for toast in self.toast_manager.active().iter() {
            let toast_id = toast.id;
            let toast_message = toast.message.clone();
            let is_hovered = self.hovered_toast == Some(toast_id);
            let is_copied = self
                .copied_toast_feedback
                .is_some_and(|(id, _)| id == toast_id);

            // Animation values
            let opacity = toast.opacity();
            let slide_offset = toast.slide_offset();

            // Clean, minimal icons and subtle accent colors
            let (icon, accent, _is_loading) = match toast.kind {
                termy_toast::ToastKind::Info => (
                    "\u{2139}", // ℹ info symbol
                    gpui::Rgba {
                        r: 0.53,
                        g: 0.70,
                        b: 0.92,
                        a: opacity,
                    },
                    false,
                ),
                termy_toast::ToastKind::Success => (
                    "\u{2713}", // ✓ checkmark
                    gpui::Rgba {
                        r: 0.42,
                        g: 0.78,
                        b: 0.55,
                        a: opacity,
                    },
                    false,
                ),
                termy_toast::ToastKind::Warning => (
                    "\u{26A0}", // ⚠ warning
                    gpui::Rgba {
                        r: 0.94,
                        g: 0.76,
                        b: 0.38,
                        a: opacity,
                    },
                    false,
                ),
                termy_toast::ToastKind::Error => (
                    "\u{2715}", // ✕ x mark
                    gpui::Rgba {
                        r: 0.92,
                        g: 0.45,
                        b: 0.45,
                        a: opacity,
                    },
                    false,
                ),
                termy_toast::ToastKind::Loading => {
                    // Animated spinner using braille characters
                    const SPINNER_FRAMES: &[&str] =
                        &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                    let elapsed_ms = toast.created_at.elapsed().as_millis() as usize;
                    let frame_index = (elapsed_ms / 80) % SPINNER_FRAMES.len();
                    (
                        SPINNER_FRAMES[frame_index],
                        gpui::Rgba {
                            r: 0.53,
                            g: 0.70,
                            b: 0.92,
                            a: opacity,
                        },
                        true,
                    )
                }
            };

            // Subtle, glassy background with animation
            let mut bg = colors.background;
            bg.a = 0.88 * opacity;
            let mut border = colors.foreground;
            border.a = 0.08 * opacity;
            let mut text = colors.foreground;
            text.a = 0.92 * opacity;

            container = container.child(
                div()
                    .id(("toast", toast_id))
                    .max_w(px(480.0))
                    .mt(px(slide_offset))
                    .rounded_lg()
                    .bg(bg)
                    .border_1()
                    .border_color(border)
                    .shadow_md()
                    .child(
                        div()
                            .px(px(14.0))
                            .py(px(12.0))
                            .flex()
                            .items_start()
                            .gap(px(10.0))
                            // Icon
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(14.0))
                                    .text_color(accent)
                                    .mt(px(1.0))
                                    .child(icon),
                            )
                            // Message - max width accounts for icon (24px) + copy btn (68px) + gaps (20px) + padding (28px)
                            .child(
                                div()
                                    .max_w(px(340.0))
                                    .text_size(px(13.0))
                                    .text_color(text)
                                    .child(toast_message.clone()),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .w(px(68.0))
                                    .h(px(24.0))
                                    .flex()
                                    .items_center()
                                    .justify_end()
                                    .children(is_copied.then(|| {
                                        let mut copied_bg = accent;
                                        copied_bg.a = 0.22;
                                        div()
                                            .rounded(px(6.0))
                                            .px(px(8.0))
                                            .py(px(4.0))
                                            .text_size(px(11.0))
                                            .text_color(accent)
                                            .bg(copied_bg)
                                            .child("Copied")
                                    }))
                                    .children((!is_copied && is_hovered).then(|| {
                                        let toast_message_for_copy = toast_message.clone();
                                        div()
                                            .rounded(px(6.0))
                                            .px(px(8.0))
                                            .py(px(4.0))
                                            .text_size(px(11.0))
                                            .text_color(text)
                                            .bg(border)
                                            .hover(|style| style.bg(accent))
                                            .cursor_pointer()
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(
                                                    move |this, _event, _window, cx| {
                                                        cx.write_to_clipboard(
                                                            ClipboardItem::new_string(
                                                                toast_message_for_copy.clone(),
                                                            ),
                                                        );
                                                        this.copied_toast_feedback =
                                                            Some((toast_id, Instant::now()));
                                                        this.notify_overlay(cx);
                                                        cx.spawn(
                                                            async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                                                                smol::Timer::after(Duration::from_millis(
                                                                    TOAST_COPY_FEEDBACK_MS,
                                                                ))
                                                                .await;
                                                                let _ = cx.update(|cx| {
                                                                    this.update(cx, |view, cx| {
                                                                        if view
                                                                            .copied_toast_feedback
                                                                            .is_some_and(
                                                                                |(id, _)| {
                                                                                    id == toast_id
                                                                                },
                                                                            )
                                                                        {
                                                                            view.copied_toast_feedback = None;
                                                                            view.notify_overlay(cx);
                                                                        }
                                                                    })
                                                                });
                                                            },
                                                        )
                                                        .detach();
                                                        cx.stop_propagation();
                                                    },
                                                ),
                                            )
                                            .child("Copy")
                                    })),
                            )
                            .on_mouse_move(cx.listener(move |this, _event, _window, cx| {
                                if this.hovered_toast != Some(toast_id) {
                                    this.hovered_toast = Some(toast_id);
                                    this.notify_overlay(cx);
                                }
                                cx.stop_propagation();
                            })),
                    ),
            );
        }

        Some(
            div()
                .id("toast-overlay")
                .size_full()
                .absolute()
                .top_0()
                .left_0()
                .child(
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .items_end()
                        .justify_end()
                        .pr(px(20.0))
                        .pb(px(20.0))
                        .on_mouse_move(cx.listener(|this, _event, _window, cx| {
                            if this.hovered_toast.is_some() {
                                this.hovered_toast = None;
                                this.notify_overlay(cx);
                            }
                        }))
                        .child(container),
                )
                .into_any(),
        )
    }

    pub(super) fn render_overlay_layer(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let now = Instant::now();
        self.toast_manager.ingest_pending();
        self.toast_manager.tick_with_hovered(self.hovered_toast);
        if let Some((_, copied_at)) = self.copied_toast_feedback
            && copied_at.elapsed() >= Duration::from_millis(TOAST_COPY_FEEDBACK_MS)
        {
            self.copied_toast_feedback = None;
        }
        if let Some(until) = self.resize_indicator_visible_until
            && now >= until
        {
            self.resize_indicator_visible_until = None;
        }

        // Request re-render during toast animations for smooth fade in/out.
        if self.toast_manager.is_animating() && !self.toast_animation_scheduled {
            self.toast_animation_scheduled = true;
            cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                smol::Timer::after(Duration::from_millis(16)).await;
                let _ = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        view.toast_animation_scheduled = false;
                        view.notify_overlay(cx);
                    })
                });
            })
            .detach();
        }
        if self.resize_indicator_visible_until.is_some()
            && !self.resize_indicator_animation_scheduled
        {
            self.resize_indicator_animation_scheduled = true;
            cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                smol::Timer::after(Duration::from_millis(16)).await;
                let _ = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        view.resize_indicator_animation_scheduled = false;
                        view.notify_overlay(cx);
                    })
                });
            })
            .detach();
        }

        let colors = self.colors.clone();
        let command_palette_overlay = if self.is_command_palette_open() {
            Some(self.render_command_palette_modal(cx))
        } else {
            None
        };
        let search_overlay = if self.search_open {
            Some(self.render_search_bar(cx))
        } else {
            None
        };
        let ai_input_overlay = if self.is_ai_input_open() {
            Some(self.render_ai_input_modal(cx))
        } else {
            None
        };
        let chrome_height = self.chrome_height();
        let terminal_overlay = (command_palette_overlay.is_some()
            || search_overlay.is_some()
            || ai_input_overlay.is_some())
        .then(|| {
            div()
                .id("terminal-scoped-overlay")
                .absolute()
                .top(px(chrome_height))
                .left_0()
                .right_0()
                .bottom_0()
                .children(command_palette_overlay)
                .children(search_overlay)
                .children(ai_input_overlay)
                .into_any_element()
        });
        let toast_overlay = self.render_toast_overlay(&colors, cx);
        let resize_overlay = self
            .resize_indicator_visible_until
            .zip(self.resize_indicator_dims)
            .map(|(_, (cols, rows))| {
                let overlay_style = self.overlay_style();
                div()
                    .id("window-resize-indicator-overlay")
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .px(px(14.0))
                            .py(px(8.0))
                            .rounded(px(6.0))
                            .bg(overlay_style.panel_background(0.84))
                            .border_1()
                            .border_color(overlay_style.panel_foreground(0.24))
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(overlay_style.panel_foreground(0.95))
                            .child(format!("{} x {}", cols, rows)),
                    )
                    .into_any_element()
            });
        let debug_overlay = self.show_debug_overlay.then(|| {
            let overlay_style = self.overlay_style();
            let cpu_percent = self.debug_overlay_stats.cpu_percent;
            let render_fps = self.debug_overlay_stats.fps;
            let memory = self.debug_overlay_memory_label();
            #[cfg(target_os = "macos")]
            let display_hint = "up to 120Hz";
            #[cfg(not(target_os = "macos"))]
            let display_hint = "system";

            div()
                .id("debug-metrics-overlay")
                .absolute()
                .top(px(chrome_height + 10.0))
                .right(px(10.0))
                .px(px(10.0))
                .py(px(8.0))
                .rounded(px(6.0))
                .bg(overlay_style.panel_background(0.84))
                .border_1()
                .border_color(overlay_style.panel_foreground(0.24))
                .text_size(px(12.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(overlay_style.panel_foreground(0.95))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(format!("Display: {}", display_hint))
                .child(format!("Render FPS: {:.1}", render_fps))
                .child(format!("CPU: {:.1}%", cpu_percent))
                .child(format!("MEM: {}", memory))
                .into_any_element()
        });

        #[cfg(target_os = "macos")]
        let banner_overlay: Option<AnyElement> = if self.show_update_banner {
            let banner_state = self.auto_updater.as_ref().map(|e| e.read(cx).state.clone());
            banner_state
                .as_ref()
                .and_then(|state| self.render_update_banner(state, &colors, cx))
                .map(|banner| {
                    div()
                        .id("update-banner-overlay")
                        .absolute()
                        .top(px(Self::titlebar_height()))
                        .left_0()
                        .right_0()
                        .child(banner)
                        .into_any_element()
                })
        } else {
            None
        };
        #[cfg(not(target_os = "macos"))]
        let banner_overlay: Option<AnyElement> = None;

        div()
            .id("terminal-overlay-layer")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .children(banner_overlay)
            .children(terminal_overlay)
            .children(resize_overlay)
            .children(debug_overlay)
            .children(toast_overlay)
            .into_any_element()
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.record_debug_overlay_frame();

        // Process pending OSC 52 clipboard writes
        if let Some(text) = self.pending_clipboard.take() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }

        let cell_size = self.calculate_cell_size(window, cx);
        let colors = self.colors.clone();
        let font_family = self.font_family.clone();
        let font_size = self.font_size;
        self.sync_window_background_appearance(window);
        let effective_background_opacity = self.background_opacity_factor();
        let (effective_padding_x, effective_padding_y) = self.effective_terminal_padding();
        let (native_split_padding_x, native_split_padding_y) = self.native_split_content_padding();
        let mut terminal_surface_bg = colors.background;
        terminal_surface_bg.a = self.scaled_background_alpha(terminal_surface_bg.a);

        self.sync_terminal_size(window, cell_size);
        let now = Instant::now();
        self.track_window_resize_indicator(window.viewport_size(), now);

        let active_pane_id = self.active_pane_id().map(ToOwned::to_owned);
        let active_tab_focus_snapshot = self
            .tabs
            .get(self.active_tab)
            .map(|tab| (tab.id, tab.panes.len()));
        self.update_pane_focus_target(
            active_tab_focus_snapshot.map(|(id, _)| id),
            active_tab_focus_snapshot
                .map(|(_, pane_count)| pane_count)
                .unwrap_or(0),
            active_pane_id.as_deref(),
            now,
        );
        let pane_focus_transition =
            self.pane_focus_transition_snapshot(active_tab_focus_snapshot.map(|(id, _)| id), now);
        let pane_focus_config = self.pane_focus_config();
        let command_palette_open = self.is_command_palette_open();
        let palette_backdrop_transform =
            command_palette_open.then(command_palette_backdrop_transform);
        let terminal_cursor_active =
            !command_palette_open && self.renaming_tab.is_none() && !self.search_open;
        let cursor_visible = terminal_cursor_active
            && self.cursor_visible_for_focus(self.focus_handle.is_focused(window));

        // Pre-compute search match info for active pane.
        let search_active = self.search_open;
        let configured_cursor_style = self.terminal_cursor_style();
        let mut terminal_display_offset = 0usize;
        let divider_rgba = pane_divider_color(terminal_surface_bg, colors.foreground);
        let divider_color: gpui::Hsla = divider_rgba.into();
        let mut pane_layers = Vec::<AnyElement>::new();
        let mut pane_dividers = Vec::<AnyElement>::new();
        let mut pane_resize_handles = Vec::<AnyElement>::new();
        let mut pane_focus_accents = Vec::<AnyElement>::new();
        let mut pane_focus_needs_animation = false;
        #[cfg(debug_assertions)]
        let mut render_pass_cache_counts = RenderPassCacheStrategyCounts::default();

        if let Some(active_tab) = self.tabs.get(self.active_tab) {
            let multi_pane = active_tab.panes.len() > 1;
            let pane_focus_enabled =
                multi_pane && pane_focus_config.is_some() && !command_palette_open;
            pane_focus_needs_animation = pane_focus_enabled && pane_focus_transition.is_some();
            let max_right_cells = active_tab
                .panes
                .iter()
                .map(|pane| u32::from(pane.left).saturating_add(u32::from(pane.width)))
                .max()
                .unwrap_or(0);
            let max_bottom_cells = active_tab
                .panes
                .iter()
                .map(|pane| u32::from(pane.top).saturating_add(u32::from(pane.height)))
                .max()
                .unwrap_or(0);

            for pane in &active_tab.panes {
                let terminal = &pane.terminal;
                let terminal_size = terminal.size();
                let cols = terminal_size.cols as usize;
                let rows = terminal_size.rows as usize;
                if cols == 0 || rows == 0 {
                    continue;
                }
                let is_active_pane = active_pane_id.as_deref() == Some(pane.id.as_str());
                let (pane_inactive_focus, pane_active_focus) = if pane_focus_enabled {
                    if let Some((from_pane_id, to_pane_id, progress)) =
                        pane_focus_transition.as_ref()
                    {
                        if pane.id == *from_pane_id {
                            (*progress, 1.0 - *progress)
                        } else if pane.id == *to_pane_id {
                            (1.0 - *progress, *progress)
                        } else {
                            (1.0, 0.0)
                        }
                    } else if is_active_pane {
                        (0.0, 1.0)
                    } else {
                        (1.0, 0.0)
                    }
                } else {
                    (0.0, 0.0)
                };
                let (pane_focus_transform, raw_pane_active_border_alpha) =
                    if let Some((preset, strength)) = pane_focus_config {
                        let inactive_scale = strength * pane_inactive_focus;
                        let active_scale = strength * pane_active_focus;
                        (
                            CellColorTransform {
                                fg_blend: preset.inactive_fg_blend * inactive_scale,
                                bg_blend: preset.inactive_bg_blend * inactive_scale,
                                desaturate: preset.inactive_desaturate * inactive_scale,
                            },
                            preset.active_border_alpha * active_scale,
                        )
                    } else {
                        (CellColorTransform::default(), 0.0)
                    };
                // Palette backdrop uses the same inactive-pane transform path to keep one
                // consistent dimming model and avoid a separate full-screen color overlay.
                let cell_color_transform =
                    palette_backdrop_transform.unwrap_or(pane_focus_transform);
                // tmux mode already has pane boundary affordances; layering Termy's active-pane
                // outline on top creates a second full-frame box around the active pane.
                let pane_active_border_alpha = effective_pane_focus_active_border_alpha(
                    raw_pane_active_border_alpha,
                    self.runtime_uses_tmux(),
                    self.tmux_show_active_pane_border,
                );
                let pane_focus_target_bg = colors.background;
                let pane_cache_key = self.pane_render_cache_key(
                    is_active_pane,
                    search_active,
                    cell_color_transform,
                    effective_background_opacity,
                );
                let (pane_display_offset, _) = terminal.scroll_state();
                let pane_search_results = if search_active && is_active_pane {
                    Some(self.search_state.results())
                } else {
                    None
                };
                #[cfg_attr(not(debug_assertions), allow(unused_variables))]
                let (pane_cells, cache_strategy, paint_damage, paint_cache) = {
                    let mut pane_render_cache = pane.render_cache.borrow_mut();
                    let paint_cache = pane_render_cache.paint_cache.clone();
                    let (pane_cells, cache_strategy, paint_damage) = self.update_pane_render_cache(
                        terminal,
                        cols,
                        rows,
                        pane_display_offset,
                        &mut pane_render_cache,
                        pane_cache_key.clone(),
                        PaneCellBuildContext {
                            colors: &colors,
                            effective_background_opacity,
                            cell_color_transform,
                            pane_focus_target_bg,
                            terminal_surface_bg,
                            selection_range: pane_cache_key.selection_range,
                            pane_search_results,
                        },
                        #[cfg(debug_assertions)]
                        &mut render_pass_cache_counts,
                    );
                    (pane_cells, cache_strategy, paint_damage, paint_cache)
                };
                #[cfg(debug_assertions)]
                render_pass_cache_counts.record(cache_strategy);

                if is_active_pane {
                    terminal_display_offset = pane_display_offset;
                }

                let hovered_link_range = if is_active_pane {
                    self.hovered_link
                        .as_ref()
                        .map(|link| (link.row, link.start_col, link.end_col))
                } else {
                    None
                };
                // Keep cursor state out of cached cells so blink/overlay redraws don't force
                // full cell-buffer rebuilds.
                let pane_cursor_state = cursor_state_for_pane(
                    terminal,
                    pane_display_offset,
                    is_active_pane,
                    cols,
                    rows,
                );
                let (cursor_cell, pane_cursor_style) = match pane_cursor_state {
                    Some(cursor) => (
                        cursor_visible.then_some((cursor.col, cursor.row)),
                        cursor.style,
                    ),
                    None => (None, configured_cursor_style),
                };

                let terminal_grid = self.build_terminal_grid_from_cache(
                    pane_cells,
                    paint_cache,
                    paint_damage,
                    cell_size,
                    cols,
                    rows,
                    &colors,
                    hovered_link_range,
                    font_family.clone(),
                    font_size,
                    pane_cursor_style,
                    cursor_cell,
                );

                let cell_width: f32 = cell_size.width.into();
                let cell_height: f32 = cell_size.height.into();
                let pane_frame_left = effective_padding_x + (f32::from(pane.left) * cell_width);
                let pane_frame_top = effective_padding_y + (f32::from(pane.top) * cell_height);
                let pane_frame_width = f32::from(pane.width) * cell_width;
                let pane_frame_height = f32::from(pane.height) * cell_height;
                let pane_left = pane_frame_left + native_split_padding_x;
                let pane_top = pane_frame_top + native_split_padding_y;
                let pane_width = f32::from(terminal_size.cols) * cell_width;
                let pane_height = f32::from(terminal_size.rows) * cell_height;
                if pane_width <= f32::EPSILON || pane_height <= f32::EPSILON {
                    continue;
                }

                let pane_right_cells = u32::from(pane.left).saturating_add(u32::from(pane.width));
                let pane_bottom_cells = u32::from(pane.top).saturating_add(u32::from(pane.height));

                if multi_pane
                    && pane_right_cells < max_right_cells
                    && let Some(gap_cells) = Self::pane_right_gap_cells(pane, &active_tab.panes)
                {
                    let gap_px = (gap_cells as f32) * cell_width;
                    let divider_left = pane_frame_left + pane_frame_width + (gap_px * 0.5) - 0.5;
                    let handle_width = gap_px.max(8.0);
                    let handle_left = divider_left - ((handle_width - 1.0) * 0.5);
                    let pane_id = pane.id.clone();
                    pane_dividers.push(
                        div()
                            .absolute()
                            .left(px(divider_left))
                            .top(px(pane_frame_top))
                            .w(px(1.0))
                            .h(px(pane_frame_height))
                            .bg(divider_color)
                            .into_any_element(),
                    );
                    pane_resize_handles.push(
                        div()
                            .id(SharedString::from(format!(
                                "pane-resize-handle-right-{}",
                                pane.id
                            )))
                            .absolute()
                            .left(px(handle_left))
                            .top(px(pane_frame_top))
                            .w(px(handle_width))
                            .h(px(pane_frame_height))
                            .cursor_col_resize()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |view, event: &MouseDownEvent, _window, cx| {
                                    view.begin_pane_resize_drag(
                                        pane_id.as_str(),
                                        PaneResizeAxis::Horizontal,
                                        PaneResizeEdge::Right,
                                        event.position,
                                    );
                                    cx.stop_propagation();
                                }),
                            )
                            .into_any_element(),
                    );
                }
                if multi_pane
                    && pane_bottom_cells < max_bottom_cells
                    && let Some(gap_cells) = Self::pane_bottom_gap_cells(pane, &active_tab.panes)
                {
                    let gap_px = (gap_cells as f32) * cell_height;
                    let divider_top = pane_frame_top + pane_frame_height + (gap_px * 0.5) - 0.5;
                    let handle_height = gap_px.max(8.0);
                    let handle_top = divider_top - ((handle_height - 1.0) * 0.5);
                    let pane_id = pane.id.clone();
                    pane_dividers.push(
                        div()
                            .absolute()
                            .left(px(pane_frame_left))
                            .top(px(divider_top))
                            .w(px(pane_frame_width))
                            .h(px(1.0))
                            .bg(divider_color)
                            .into_any_element(),
                    );
                    pane_resize_handles.push(
                        div()
                            .id(SharedString::from(format!(
                                "pane-resize-handle-bottom-{}",
                                pane.id
                            )))
                            .absolute()
                            .left(px(pane_frame_left))
                            .top(px(handle_top))
                            .w(px(pane_frame_width))
                            .h(px(handle_height))
                            .cursor_row_resize()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |view, event: &MouseDownEvent, _window, cx| {
                                    view.begin_pane_resize_drag(
                                        pane_id.as_str(),
                                        PaneResizeAxis::Vertical,
                                        PaneResizeEdge::Bottom,
                                        event.position,
                                    );
                                    cx.stop_propagation();
                                }),
                            )
                            .into_any_element(),
                    );
                }

                pane_layers.push(
                    div()
                        .id(SharedString::from(format!("pane-{}", pane.id)))
                        .absolute()
                        .left(px(pane_left))
                        .top(px(pane_top))
                        .w(px(pane_width))
                        .h(px(pane_height))
                        .child(terminal_grid)
                        .into_any_element(),
                );

                if multi_pane && pane_active_border_alpha > f32::EPSILON {
                    let mut accent = blend_rgb_only(colors.cursor, colors.foreground, 0.18);
                    accent.a = self.scaled_chrome_alpha(pane_active_border_alpha);
                    let accent_hsla: gpui::Hsla = accent.into();
                    pane_focus_accents.push(
                        div()
                            .id(SharedString::from(format!("pane-focus-accent-{}", pane.id)))
                            .absolute()
                            .left(px(pane_frame_left))
                            .top(px(pane_frame_top))
                            .w(px(pane_frame_width))
                            .h(px(pane_frame_height))
                            .border_1()
                            .border_color(accent_hsla)
                            .into_any_element(),
                    );
                }

                if pane.degraded {
                    // Hydration degraded panes still function, but this marker makes
                    // the warning state persistent until the next successful snapshot.
                    let degraded_accent = gpui::Hsla {
                        h: 0.09,
                        s: 0.92,
                        l: 0.58,
                        a: self.scaled_chrome_alpha(0.68),
                    };
                    pane_focus_accents.push(
                        div()
                            .id(SharedString::from(format!(
                                "pane-degraded-accent-{}",
                                pane.id
                            )))
                            .absolute()
                            .left(px(pane_frame_left))
                            .top(px(pane_frame_top))
                            .w(px(pane_frame_width))
                            .h(px(pane_frame_height))
                            .border_1()
                            .border_color(degraded_accent)
                            .into_any_element(),
                    );
                }
            }
        }

        if pane_focus_needs_animation {
            self.schedule_pane_focus_animation(cx);
        }
        if self
            .tab_strip
            .switch_hints
            .animation_active(now, self.tab_switch_hints_blocked())
        {
            self.schedule_tab_switch_hint_animation(cx);
        }
        #[cfg(debug_assertions)]
        self.record_render_metrics_for_pass(render_pass_cache_counts);

        let focus_handle = self.focus_handle.clone();
        let titlebar_height = Self::titlebar_height();
        let titlebar_bg = terminal_surface_bg;
        let tabbar_bg = terminal_surface_bg;
        let tabs_row = self.render_tab_strip(window, &colors, &font_family, tabbar_bg, cx);

        #[cfg(target_os = "macos")]
        let banner_spacer: Option<AnyElement> = self.show_update_banner.then(|| {
            div()
                .id("update-banner-spacer")
                .w_full()
                .h(px(UPDATE_BANNER_HEIGHT))
                .flex_none()
                .into_any_element()
        });
        #[cfg(not(target_os = "macos"))]
        let banner_spacer: Option<AnyElement> = None;
        let terminal_surface_bg_hsla: gpui::Hsla = terminal_surface_bg.into();
        if self.terminal_scrollbar_mode() == ui_scrollbar::ScrollbarVisibilityMode::OnScroll
            && !self.terminal_scrollbar_animation_active
            && self.terminal_scrollbar_needs_animation(Instant::now())
        {
            self.start_terminal_scrollbar_animation(cx);
        }
        let terminal_surface = self.terminal_surface_geometry(window);
        let terminal_scrollbar_layout = terminal_surface.and_then(|surface| {
            self.terminal_scrollbar_layout_for_track(surface.height)
                .map(|layout| (surface, layout))
        });
        if terminal_scrollbar_layout.is_none() {
            self.clear_terminal_scrollbar_marker_cache();
        }
        let terminal_scrollbar_overlay = terminal_scrollbar_layout.and_then(|(surface, layout)| {
            self.render_terminal_scrollbar_overlay(surface, layout, terminal_display_offset > 0)
        });
        let terminal_grid_layer = div()
            .relative()
            .w_full()
            .h_full()
            .children(pane_layers)
            .children(pane_dividers)
            .children(pane_resize_handles)
            .children(pane_focus_accents)
            .into_any_element();
        let mut agent_sidebar_muted: gpui::Hsla = self.colors.foreground.into();
        agent_sidebar_muted.a = 0.72;
        let agent_sidebar = self.agent_sidebar_visible().then(|| {
            let text_color: gpui::Hsla = self.colors.foreground.into();
            let mut selection_color = text_color;
            selection_color.a = 0.3;
            let has_text = !self.agent_sidebar_input.text().is_empty();
            let input_content = div()
                .id("agent-sidebar-input")
                .w_full()
                .h(px(20.0))
                .overflow_hidden()
                .cursor_text()
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, _event: &MouseDownEvent, window, cx| {
                        view.agent_sidebar_input_active = true;
                        view.focus_handle.focus(window, cx);
                        cx.notify();
                    }),
                )
                .child(if self.agent_sidebar_input_active {
                    InlineInputElement::new(
                        cx.entity(),
                        self.focus_handle.clone(),
                        Font {
                            family: self.font_family.clone(),
                            ..Font::default()
                        },
                        px(14.0),
                        text_color,
                        selection_color,
                        InlineInputAlignment::Left,
                    )
                    .into_any_element()
                } else if has_text {
                    div()
                        .text_size(px(14.0))
                        .text_color(text_color)
                        .child(self.agent_sidebar_input.text().to_string())
                        .into_any_element()
                } else {
                    let mut ghost = agent_sidebar_muted;
                    ghost.a = 0.74;
                    div()
                        .text_size(px(14.0))
                        .text_color(ghost)
                        .child("Message Agent")
                        .into_any_element()
                })
                .into_any_element();
            div()
                .relative()
                .h_full()
                .child(
                    div()
                        .id("agent-sidebar-resize-handle")
                        .absolute()
                        .left(px(0.0))
                        .top(px(0.0))
                        .w(px(8.0))
                        .h_full()
                        .cursor_col_resize()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _event: &MouseDownEvent, _window, cx| {
                                view.agent_sidebar_resize_drag = Some(AgentSidebarResizeDragState);
                                cx.stop_propagation();
                            }),
                        ),
                )
                .child(termy_agent_sidebar::render_sidebar(
                    self.agent_sidebar_width,
                    terminal_surface_bg_hsla,
                    divider_color,
                    self.colors.foreground.into(),
                    agent_sidebar_muted,
                    input_content,
                ))
                .into_any_element()
        });
        let overlay_view = self.ensure_overlay_view(cx);
        let key_context = if self.has_active_inline_input() {
            "Terminal InlineInput"
        } else {
            "Terminal"
        };
        let titlebar_element: Option<AnyElement> = (titlebar_height > 0.0).then(|| {
            let titlebar_container = div()
                .id("titlebar")
                .w_full()
                .h(px(titlebar_height))
                .flex_none()
                .relative()
                .flex()
                .items_center()
                .on_mouse_move(cx.listener(Self::handle_titlebar_tab_strip_mouse_move))
                .bg(titlebar_bg);

            titlebar_container
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(Self::handle_unified_titlebar_mouse_down),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(Self::handle_unified_titlebar_mouse_up),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(Self::handle_unified_titlebar_mouse_up),
                )
                .child(
                    div()
                        .w_full()
                        .h_full()
                        .flex()
                        .items_end()
                        .mt(px(TOP_STRIP_CONTENT_OFFSET_Y))
                        .child(tabs_row),
                )
                .into_any()
        });
        let mut root_bg = colors.background;
        root_bg.a = self.scaled_background_alpha(root_bg.a);

        let root = div()
            .id("termy-root")
            .flex()
            .flex_col()
            .size_full()
            .bg(root_bg)
            .font_family(font_family.clone())
            .capture_any_mouse_up(cx.listener(|this, event: &MouseUpEvent, _window, cx| {
                if matches!(
                    event.button,
                    MouseButton::Left | MouseButton::Middle | MouseButton::Right
                ) {
                    this.handle_global_mouse_up_event(event, cx);
                }
                if event.button == MouseButton::Left {
                    this.disarm_titlebar_window_move();
                    this.commit_tab_drag(cx);
                }
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                this.handle_global_mouse_move_event(event, cx);
            }))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    this.disarm_titlebar_window_move();
                    this.commit_tab_drag(cx);
                }),
            )
            .children(titlebar_element)
            .children(banner_spacer)
            .child(
                div()
                    .id("terminal")
                    .track_focus(&focus_handle)
                    .key_context(key_context)
                    .on_action(cx.listener(Self::handle_toggle_command_palette_action))
                    .on_action(cx.listener(Self::handle_import_colors_action))
                    .on_action(cx.listener(Self::handle_switch_theme_action))
                    .on_action(cx.listener(Self::handle_app_info_action))
                    .on_action(cx.listener(Self::handle_restart_app_action))
                    .on_action(cx.listener(Self::handle_rename_tab_action))
                    .on_action(cx.listener(Self::handle_check_for_updates_action))
                    .on_action(cx.listener(Self::handle_new_tab_action))
                    .on_action(cx.listener(Self::handle_close_tab_action))
                    .on_action(cx.listener(Self::handle_close_pane_or_tab_action))
                    .on_action(cx.listener(Self::handle_move_tab_left_action))
                    .on_action(cx.listener(Self::handle_move_tab_right_action))
                    .on_action(cx.listener(Self::handle_switch_tab_left_action))
                    .on_action(cx.listener(Self::handle_switch_tab_right_action))
                    .on_action(cx.listener(Self::handle_switch_to_tab_1_action))
                    .on_action(cx.listener(Self::handle_switch_to_tab_2_action))
                    .on_action(cx.listener(Self::handle_switch_to_tab_3_action))
                    .on_action(cx.listener(Self::handle_switch_to_tab_4_action))
                    .on_action(cx.listener(Self::handle_switch_to_tab_5_action))
                    .on_action(cx.listener(Self::handle_switch_to_tab_6_action))
                    .on_action(cx.listener(Self::handle_switch_to_tab_7_action))
                    .on_action(cx.listener(Self::handle_switch_to_tab_8_action))
                    .on_action(cx.listener(Self::handle_switch_to_tab_9_action))
                    .on_action(cx.listener(Self::handle_manage_tmux_sessions_action))
                    .on_action(cx.listener(Self::handle_manage_saved_layouts_action))
                    .on_action(cx.listener(Self::handle_split_pane_vertical_action))
                    .on_action(cx.listener(Self::handle_split_pane_horizontal_action))
                    .on_action(cx.listener(Self::handle_close_pane_action))
                    .on_action(cx.listener(Self::handle_focus_pane_next_action))
                    .on_action(cx.listener(Self::handle_focus_pane_left_action))
                    .on_action(cx.listener(Self::handle_focus_pane_right_action))
                    .on_action(cx.listener(Self::handle_focus_pane_up_action))
                    .on_action(cx.listener(Self::handle_focus_pane_down_action))
                    .on_action(cx.listener(Self::handle_focus_pane_previous_action))
                    .on_action(cx.listener(Self::handle_resize_pane_left_action))
                    .on_action(cx.listener(Self::handle_resize_pane_right_action))
                    .on_action(cx.listener(Self::handle_resize_pane_up_action))
                    .on_action(cx.listener(Self::handle_resize_pane_down_action))
                    .on_action(cx.listener(Self::handle_toggle_pane_zoom_action))
                    .on_action(cx.listener(Self::handle_minimize_window_action))
                    .on_action(cx.listener(Self::handle_copy_action))
                    .on_action(cx.listener(Self::handle_paste_action))
                    .on_action(cx.listener(Self::handle_zoom_in_action))
                    .on_action(cx.listener(Self::handle_zoom_out_action))
                    .on_action(cx.listener(Self::handle_zoom_reset_action))
                    .on_action(cx.listener(Self::handle_quit_action))
                    .on_action(cx.listener(Self::handle_open_search_action))
                    .on_action(cx.listener(Self::handle_close_search_action))
                    .on_action(cx.listener(Self::handle_search_next_action))
                    .on_action(cx.listener(Self::handle_search_previous_action))
                    .on_action(cx.listener(Self::handle_toggle_search_case_sensitive_action))
                    .on_action(cx.listener(Self::handle_toggle_search_regex_action))
                    .when(self.install_cli_available(), |s| {
                        s.on_action(cx.listener(Self::handle_install_cli_action))
                    })
                    .on_action(cx.listener(Self::handle_toggle_ai_input_action))
                    .on_action(cx.listener(Self::handle_toggle_agent_sidebar_action))
                    .on_action(cx.listener(Self::handle_inline_backspace_action))
                    .on_action(cx.listener(Self::handle_inline_delete_action))
                    .on_action(cx.listener(Self::handle_inline_move_left_action))
                    .on_action(cx.listener(Self::handle_inline_move_right_action))
                    .on_action(cx.listener(Self::handle_inline_select_left_action))
                    .on_action(cx.listener(Self::handle_inline_select_right_action))
                    .on_action(cx.listener(Self::handle_inline_select_all_action))
                    .on_action(cx.listener(Self::handle_inline_move_to_start_action))
                    .on_action(cx.listener(Self::handle_inline_move_to_end_action))
                    .on_action(cx.listener(Self::handle_inline_delete_word_backward_action))
                    .on_action(cx.listener(Self::handle_inline_delete_word_forward_action))
                    .on_action(cx.listener(Self::handle_inline_delete_to_start_action))
                    .on_action(cx.listener(Self::handle_inline_delete_to_end_action))
                    .on_key_down(cx.listener(Self::handle_key_down))
                    .on_modifiers_changed(cx.listener(Self::handle_modifiers_changed))
                    .relative()
                    .flex_1()
                    .w_full()
                    .overflow_hidden()
                    .child(
                        div()
                            .id("terminal-content")
                            .flex()
                            .w_full()
                            .h_full()
                            .child(
                                div()
                                    .id("terminal-surface")
                                    .relative()
                                    .flex_1()
                                    .h_full()
                                    .overflow_hidden()
                                    .bg(terminal_surface_bg_hsla)
                                    .on_scroll_wheel(
                                        cx.listener(Self::handle_terminal_scroll_wheel),
                                    )
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(Self::handle_mouse_down),
                                    )
                                    .on_mouse_down(
                                        MouseButton::Middle,
                                        cx.listener(Self::handle_mouse_down),
                                    )
                                    .on_mouse_down(
                                        MouseButton::Right,
                                        cx.listener(Self::handle_mouse_down),
                                    )
                                    .on_mouse_move(cx.listener(Self::handle_mouse_move))
                                    .on_mouse_up(
                                        MouseButton::Left,
                                        cx.listener(Self::handle_mouse_up),
                                    )
                                    .on_mouse_up(
                                        MouseButton::Middle,
                                        cx.listener(Self::handle_mouse_up),
                                    )
                                    .on_mouse_up(
                                        MouseButton::Right,
                                        cx.listener(Self::handle_mouse_up),
                                    )
                                    .on_drop(cx.listener(Self::handle_file_drop))
                                    .when_some(
                                        self.pane_resize_drag.as_ref(),
                                        |s, drag| match drag.axis {
                                            PaneResizeAxis::Horizontal => s.cursor_col_resize(),
                                            PaneResizeAxis::Vertical => s.cursor_row_resize(),
                                        },
                                    )
                                    .font_family(font_family.clone())
                                    .text_size(font_size)
                                    .child(terminal_grid_layer)
                                    .children(terminal_scrollbar_overlay),
                            )
                            .children(agent_sidebar),
                    ),
            )
            .child(overlay_view);

        #[cfg(debug_assertions)]
        self.maybe_emit_render_metrics_log(Instant::now());

        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_render_cell(col: usize, row: usize, c: char) -> CellRenderInfo {
        CellRenderInfo {
            col,
            row,
            char: c,
            fg: gpui::Hsla::transparent_black(),
            bg: gpui::Hsla::transparent_black(),
            bold: false,
            render_text: true,
            selected: false,
            search_current: false,
            search_match: false,
        }
    }

    fn tmux_test_pane(id: &str, left: u16, top: u16, cols: u16, rows: u16) -> TerminalPane {
        let size = TerminalSize {
            cols,
            rows,
            ..TerminalSize::default()
        };
        TerminalPane {
            id: id.to_string(),
            left,
            top,
            width: cols,
            height: rows,
            degraded: false,
            terminal: Terminal::new_tmux(
                size,
                TerminalOptions {
                    scrollback_history: 128,
                    ..TerminalOptions::default()
                },
            ),
            render_cache: std::cell::RefCell::new(TerminalPaneRenderCache::default()),
        }
    }

    fn test_render_rows(rows: Vec<Vec<CellRenderInfo>>) -> PaneRenderCells {
        Arc::new(rows.into_iter().map(Arc::new).collect())
    }

    #[test]
    fn resolved_cursor_state_for_pane_keeps_terminal_hidden_cursor_hidden() {
        let resolved = filtered_cursor_state(None, 0, true, 10, 4);
        assert_eq!(resolved, None);
    }

    #[test]
    fn resolved_cursor_state_for_pane_filters_inactive_scrolled_and_out_of_bounds_cursors() {
        let cursor = TerminalCursorState {
            col: 3,
            row: 1,
            style: TerminalCursorStyle::Line,
        };
        assert_eq!(filtered_cursor_state(Some(cursor), 1, true, 10, 4), None);
        assert_eq!(filtered_cursor_state(Some(cursor), 0, false, 10, 4), None);
        assert_eq!(
            filtered_cursor_state(
                Some(TerminalCursorState {
                    col: 12,
                    row: 1,
                    style: TerminalCursorStyle::Block,
                }),
                0,
                true,
                10,
                4,
            ),
            None
        );
        assert_eq!(
            filtered_cursor_state(
                Some(TerminalCursorState {
                    col: 3,
                    row: 4,
                    style: TerminalCursorStyle::Block,
                }),
                0,
                true,
                10,
                4,
            ),
            None
        );
        assert_eq!(
            filtered_cursor_state(Some(cursor), 0, true, 10, 4),
            Some(cursor)
        );
    }

    #[test]
    fn merge_pane_render_rows_reuses_existing_arc_when_updates_empty() {
        let existing = test_render_rows(vec![vec![
            test_render_cell(0, 0, 'a'),
            test_render_cell(1, 0, 'b'),
            test_render_cell(2, 0, 'c'),
        ]]);

        let merged = merge_pane_render_rows(&existing, 1, 3, Vec::new());
        assert!(Arc::ptr_eq(&existing, &merged));
    }

    #[test]
    fn merge_pane_render_rows_updates_only_touched_row_cells() {
        let existing = test_render_rows(vec![
            vec![test_render_cell(0, 0, 'a'), test_render_cell(1, 0, 'b')],
            vec![test_render_cell(0, 1, 'c'), test_render_cell(1, 1, 'd')],
            vec![test_render_cell(0, 2, 'e'), test_render_cell(1, 2, 'f')],
        ]);
        let updates = vec![(1, 1, test_render_cell(1, 1, 'x'))];

        let merged = merge_pane_render_rows(&existing, 3, 2, updates);

        assert!(Arc::ptr_eq(&existing[0], &merged[0]));
        assert!(!Arc::ptr_eq(&existing[1], &merged[1]));
        assert!(Arc::ptr_eq(&existing[2], &merged[2]));
        assert_eq!(merged[0][0].char, 'a');
        assert_eq!(merged[1][1].char, 'x');
        assert_eq!(merged[2][1].char, 'f');
    }

    #[test]
    fn merge_pane_render_rows_uses_last_write_for_duplicate_cell() {
        let existing = test_render_rows(vec![
            vec![test_render_cell(0, 0, 'a'), test_render_cell(1, 0, 'b')],
            vec![test_render_cell(0, 1, 'c'), test_render_cell(1, 1, 'd')],
        ]);
        let updates = vec![
            (1, 1, test_render_cell(1, 1, 'x')),
            (1, 1, test_render_cell(1, 1, 'y')),
            (0, 0, test_render_cell(0, 0, 'z')),
        ];

        let merged = merge_pane_render_rows(&existing, 2, 2, updates);

        assert_eq!(merged[0][0].char, 'z');
        assert_eq!(merged[1][1].char, 'y');
        assert_eq!(merged[1][0].char, 'c');
    }

    #[test]
    fn terminal_scrollbar_overlay_frame_anchors_to_active_pane_geometry() {
        let surface = TerminalViewportGeometry {
            origin_x: 32.0,
            origin_y: 48.0,
            width: 640.0,
            height: 420.0,
        };

        let frame = terminal_scrollbar_overlay_frame(surface);
        assert_eq!(
            frame.left,
            surface.origin_x + surface.width - TERMINAL_SCROLLBAR_GUTTER_WIDTH
        );
        assert_eq!(frame.top, surface.origin_y);
        assert_eq!(frame.width, TERMINAL_SCROLLBAR_GUTTER_WIDTH);
        assert_eq!(frame.height, surface.height);
    }

    #[test]
    fn terminal_scrollbar_overlay_frame_clamps_when_surface_is_narrower_than_gutter() {
        let surface = TerminalViewportGeometry {
            origin_x: 10.0,
            origin_y: 20.0,
            width: 6.0,
            height: 100.0,
        };

        let frame = terminal_scrollbar_overlay_frame(surface);
        assert_eq!(frame.left, surface.origin_x);
        assert_eq!(frame.top, surface.origin_y);
        assert_eq!(frame.width, surface.width);
        assert_eq!(frame.height, surface.height);
    }

    #[test]
    fn apply_cell_color_transform_is_noop_for_zero_factors() {
        let fg = gpui::Rgba {
            r: 0.72,
            g: 0.64,
            b: 0.35,
            a: 0.91,
        };
        let bg = gpui::Rgba {
            r: 0.12,
            g: 0.17,
            b: 0.26,
            a: 0.66,
        };
        let fg_target = gpui::Rgba {
            r: 0.01,
            g: 0.02,
            b: 0.03,
            a: 1.0,
        };
        let bg_target = gpui::Rgba {
            r: 0.98,
            g: 0.97,
            b: 0.96,
            a: 1.0,
        };

        let (next_fg, next_bg) =
            apply_cell_color_transform(fg, bg, CellColorTransform::default(), fg_target, bg_target);

        assert_eq!(next_fg, fg);
        assert_eq!(next_bg, bg);
    }

    #[test]
    fn command_palette_backdrop_transform_uses_soft_spotlight_coefficients() {
        let preset = pane_focus_preset(PaneFocusEffect::SoftSpotlight)
            .expect("soft spotlight preset should exist");
        let transform = command_palette_backdrop_transform();
        let expected_fg = preset.inactive_fg_blend * COMMAND_PALETTE_BACKDROP_STRENGTH;
        let expected_bg = preset.inactive_bg_blend * COMMAND_PALETTE_BACKDROP_STRENGTH;
        let expected_desaturate = preset.inactive_desaturate * COMMAND_PALETTE_BACKDROP_STRENGTH;

        assert!((transform.fg_blend - expected_fg).abs() <= f32::EPSILON);
        assert!((transform.bg_blend - expected_bg).abs() <= f32::EPSILON);
        assert!((transform.desaturate - expected_desaturate).abs() <= f32::EPSILON);
    }

    #[test]
    fn terminal_scrollbar_track_width_clamps_to_overlay_frame() {
        assert_eq!(
            terminal_scrollbar_track_width(TERMINAL_SCROLLBAR_TRACK_WIDTH + 2.0),
            TERMINAL_SCROLLBAR_TRACK_WIDTH
        );
        assert_eq!(terminal_scrollbar_track_width(6.0), 6.0);
        assert_eq!(terminal_scrollbar_track_width(-2.0), 0.0);
    }

    #[test]
    fn pane_right_gap_cells_returns_zero_for_adjacent_overlapping_pane() {
        let base = tmux_test_pane("%1", 0, 0, 10, 6);
        let adjacent = tmux_test_pane("%2", 10, 2, 5, 2);
        let panes = vec![base, adjacent];
        assert_eq!(
            TerminalView::pane_right_gap_cells(&panes[0], &panes),
            Some(0)
        );
    }

    #[test]
    fn pane_right_gap_cells_returns_none_without_vertical_overlap() {
        let base = tmux_test_pane("%1", 0, 0, 10, 6);
        let separated = tmux_test_pane("%2", 10, 6, 5, 3);
        let panes = vec![base, separated];
        assert_eq!(TerminalView::pane_right_gap_cells(&panes[0], &panes), None);
    }

    #[test]
    fn pane_right_gap_cells_prefers_smallest_matching_candidate_gap() {
        let base = tmux_test_pane("%1", 0, 0, 10, 6);
        let far = tmux_test_pane("%2", 15, 0, 3, 6);
        let near = tmux_test_pane("%3", 12, 1, 3, 2);
        let non_overlap = tmux_test_pane("%4", 11, 7, 3, 2);
        let panes = vec![base, far, near, non_overlap];
        assert_eq!(
            TerminalView::pane_right_gap_cells(&panes[0], &panes),
            Some(2)
        );
    }

    #[test]
    fn pane_bottom_gap_cells_returns_zero_for_adjacent_overlapping_pane() {
        let base = tmux_test_pane("%1", 0, 0, 10, 6);
        let adjacent = tmux_test_pane("%2", 2, 6, 3, 3);
        let panes = vec![base, adjacent];
        assert_eq!(
            TerminalView::pane_bottom_gap_cells(&panes[0], &panes),
            Some(0)
        );
    }

    #[test]
    fn pane_bottom_gap_cells_returns_none_without_horizontal_overlap() {
        let base = tmux_test_pane("%1", 0, 0, 10, 6);
        let separated = tmux_test_pane("%2", 10, 6, 4, 3);
        let panes = vec![base, separated];
        assert_eq!(TerminalView::pane_bottom_gap_cells(&panes[0], &panes), None);
    }

    #[test]
    fn pane_bottom_gap_cells_prefers_smallest_matching_candidate_gap() {
        let base = tmux_test_pane("%1", 0, 0, 10, 6);
        let far = tmux_test_pane("%2", 0, 10, 10, 2);
        let near = tmux_test_pane("%3", 3, 8, 2, 2);
        let non_overlap = tmux_test_pane("%4", 11, 9, 2, 2);
        let panes = vec![base, far, near, non_overlap];
        assert_eq!(
            TerminalView::pane_bottom_gap_cells(&panes[0], &panes),
            Some(2)
        );
    }

    #[test]
    fn pane_focus_active_border_alpha_is_zero_in_tmux_runtime() {
        let alpha = effective_pane_focus_active_border_alpha(0.38, true, false);
        assert_eq!(alpha, 0.0);
    }

    #[test]
    fn pane_focus_active_border_alpha_is_unchanged_in_native_runtime() {
        let alpha = effective_pane_focus_active_border_alpha(0.38, false, false);
        assert_eq!(alpha, 0.38);
    }

    #[test]
    fn pane_focus_active_border_alpha_is_unchanged_when_tmux_border_is_enabled() {
        let alpha = effective_pane_focus_active_border_alpha(0.38, true, true);
        assert_eq!(alpha, 0.38);
    }

    #[test]
    fn pane_cache_strategy_reuses_cells_when_damage_is_empty_and_key_matches() {
        let strategy = pane_cache_update_strategy(
            true,
            true,
            true,
            true,
            &TerminalDamageSnapshot::Partial(Vec::new()),
        );
        assert_eq!(strategy, PaneCacheUpdateStrategy::Reuse);
    }

    #[test]
    fn pane_cache_strategy_forces_full_rebuild_when_cache_key_changes() {
        let strategy = pane_cache_update_strategy(
            true,
            true,
            true,
            false,
            &TerminalDamageSnapshot::Partial(vec![TerminalDirtySpan {
                row: 0,
                left_col: 0,
                right_col: 1,
            }]),
        );
        assert_eq!(strategy, PaneCacheUpdateStrategy::Full);
    }

    #[test]
    fn pane_cache_strategy_uses_partial_patch_for_non_empty_partial_damage() {
        let strategy = pane_cache_update_strategy(
            true,
            true,
            true,
            true,
            &TerminalDamageSnapshot::Partial(vec![TerminalDirtySpan {
                row: 1,
                left_col: 2,
                right_col: 4,
            }]),
        );
        assert_eq!(strategy, PaneCacheUpdateStrategy::Partial);
    }

    #[test]
    fn pane_cache_strategy_forces_full_rebuild_when_display_offset_changes() {
        let strategy = pane_cache_update_strategy(
            true,
            true,
            false,
            true,
            &TerminalDamageSnapshot::Partial(vec![TerminalDirtySpan {
                row: 1,
                left_col: 0,
                right_col: 0,
            }]),
        );
        assert_eq!(strategy, PaneCacheUpdateStrategy::Full);
    }

    #[test]
    fn pane_cache_strategy_forces_full_rebuild_when_cache_is_empty() {
        let strategy = pane_cache_update_strategy(
            false,
            true,
            true,
            true,
            &TerminalDamageSnapshot::Partial(vec![TerminalDirtySpan {
                row: 0,
                left_col: 0,
                right_col: 0,
            }]),
        );
        assert_eq!(strategy, PaneCacheUpdateStrategy::Full);
    }

    #[test]
    fn finalized_cache_update_strategy_upgrades_partial_when_fallback_rebuilds() {
        let strategy = finalized_cache_update_strategy(PaneCacheUpdateStrategy::Partial, true);
        assert_eq!(strategy, PaneCacheUpdateStrategy::Full);
    }

    #[test]
    fn finalized_cache_update_strategy_keeps_planned_strategy_without_fallback() {
        assert_eq!(
            finalized_cache_update_strategy(PaneCacheUpdateStrategy::Reuse, false),
            PaneCacheUpdateStrategy::Reuse
        );
        assert_eq!(
            finalized_cache_update_strategy(PaneCacheUpdateStrategy::Partial, false),
            PaneCacheUpdateStrategy::Partial
        );
        assert_eq!(
            finalized_cache_update_strategy(PaneCacheUpdateStrategy::Full, true),
            PaneCacheUpdateStrategy::Full
        );
    }

    #[test]
    fn paint_damage_from_dirty_spans_sorts_and_dedupes_rows() {
        let damage = paint_damage_from_dirty_spans(
            &[
                TerminalDirtySpan {
                    row: 3,
                    left_col: 0,
                    right_col: 1,
                },
                TerminalDirtySpan {
                    row: 1,
                    left_col: 2,
                    right_col: 4,
                },
                TerminalDirtySpan {
                    row: 3,
                    left_col: 5,
                    right_col: 6,
                },
            ],
            4,
        );
        assert_eq!(
            damage,
            TerminalGridPaintDamage::Rows(vec![1usize, 3usize].into())
        );
    }

    #[test]
    fn paint_damage_from_dirty_spans_ignores_out_of_bounds_rows() {
        let damage = paint_damage_from_dirty_spans(
            &[TerminalDirtySpan {
                row: 7,
                left_col: 0,
                right_col: 1,
            }],
            2,
        );
        assert_eq!(damage, TerminalGridPaintDamage::None);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn record_cache_strategy_increments_reuse() {
        let mut counts = RenderPassCacheStrategyCounts::default();
        counts.record(PaneCacheUpdateStrategy::Reuse);
        assert_eq!(counts.reuse, 1);
        assert_eq!(counts.partial, 0);
        assert_eq!(counts.full, 0);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn record_cache_strategy_increments_partial() {
        let mut counts = RenderPassCacheStrategyCounts::default();
        counts.record(PaneCacheUpdateStrategy::Partial);
        assert_eq!(counts.reuse, 0);
        assert_eq!(counts.partial, 1);
        assert_eq!(counts.full, 0);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn record_cache_strategy_increments_full() {
        let mut counts = RenderPassCacheStrategyCounts::default();
        counts.record(PaneCacheUpdateStrategy::Full);
        assert_eq!(counts.reuse, 0);
        assert_eq!(counts.partial, 0);
        assert_eq!(counts.full, 1);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn record_partial_work_tracks_dirty_spans_and_patched_cells() {
        let mut counts = RenderPassCacheStrategyCounts::default();
        counts.record_partial_work(3, 12);
        assert_eq!(counts.dirty_span_count, 3);
        assert_eq!(counts.patched_cell_count, 12);
    }

    #[cfg(debug_assertions)]
    #[test]
    fn render_count_increments_once_per_render_call() {
        let mut counters = TerminalRenderMetricsCounters::default();
        increment_render_count_counter(&mut counters);
        assert_eq!(counters.render_count, 1);
    }
}
