use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalSelectionCharClass {
    Whitespace,
    Word,
    Other,
}

fn is_hidden_or_spacer(flags: Flags) -> bool {
    flags.intersects(Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER | Flags::HIDDEN)
}

fn fill_grid_rows_for_selection(
    terminal: &Terminal,
    min_row: usize,
    max_row: usize,
    cols: usize,
    grid: &mut [Vec<char>],
) {
    let _ = terminal.for_each_renderable_cell(|display_offset, term_line, col, cell| {
        let Some(row) = TerminalView::viewport_row_from_term_line(term_line, display_offset) else {
            return;
        };
        if row < min_row || row > max_row || col >= cols {
            return;
        }
        if is_hidden_or_spacer(cell.flags) {
            return;
        }

        let c = cell.c;
        if c != '\0' {
            let grid_row = row - min_row;
            grid[grid_row][col] = if c.is_control() { ' ' } else { c };
        }
    });
}

fn row_text_from_terminal(terminal: &Terminal, row: usize, cols: usize) -> Vec<char> {
    let mut line = vec![' '; cols];
    let _ = terminal.for_each_renderable_cell(|display_offset, term_line, col, cell| {
        let Some(cell_row) = TerminalView::viewport_row_from_term_line(term_line, display_offset)
        else {
            return;
        };
        if cell_row != row || col >= cols {
            return;
        }

        if is_hidden_or_spacer(cell.flags) {
            return;
        }

        let c = cell.c;
        if c != '\0' {
            line[col] = if c.is_control() { ' ' } else { c };
        }
    });
    line
}

fn pane_cell_for_position(
    pane: &TerminalPane,
    x: f32,
    y: f32,
    padding_x: f32,
    padding_y: f32,
    clamp: bool,
    allow_clamp_outside: bool,
) -> Option<CellPos> {
    let size = pane.terminal.size();
    if size.cols == 0 || size.rows == 0 {
        return None;
    }
    let cell_width: f32 = size.cell_width.into();
    let cell_height: f32 = size.cell_height.into();
    if cell_width <= f32::EPSILON || cell_height <= f32::EPSILON {
        return None;
    }

    let origin_x = padding_x + (f32::from(pane.left) * cell_width);
    let origin_y = padding_y + (f32::from(pane.top) * cell_height);
    let width = f32::from(size.cols) * cell_width;
    let height = f32::from(size.rows) * cell_height;
    if width <= f32::EPSILON || height <= f32::EPSILON {
        return None;
    }

    let mut local_x = x - origin_x;
    let mut local_y = y - origin_y;
    let is_inside = local_x >= 0.0 && local_x < width && local_y >= 0.0 && local_y < height;
    if !is_inside {
        if !clamp || !allow_clamp_outside {
            return None;
        }
        local_x = local_x.clamp(0.0, width - f32::EPSILON);
        local_y = local_y.clamp(0.0, height - f32::EPSILON);
    }

    let max_col = i32::from(size.cols) - 1;
    let max_row = i32::from(size.rows) - 1;
    if max_col < 0 || max_row < 0 {
        return None;
    }

    let mut col = (local_x / cell_width).floor() as i32;
    let mut row = (local_y / cell_height).floor() as i32;
    if clamp {
        col = col.clamp(0, max_col);
        row = row.clamp(0, max_row);
    } else if col < 0 || col > max_col || row < 0 || row > max_row {
        return None;
    }

    Some(CellPos {
        col: col as usize,
        row: row as usize,
    })
}

fn resolve_pane_cell_for_position(
    panes: &[TerminalPane],
    active_pane_id: Option<&str>,
    x: f32,
    y: f32,
    padding_x: f32,
    padding_y: f32,
    clamp: bool,
) -> Option<(String, CellPos)> {
    let pointer_inside_any_pane = panes.iter().any(|pane| {
        pane_cell_for_position(pane, x, y, padding_x, padding_y, false, false).is_some()
    });

    // When clamping points that are outside all panes, prefer the active pane first
    // so we never return a clamped hit for an inactive pane before the active pane.
    if clamp
        && !pointer_inside_any_pane
        && let Some(active_pane_id) = active_pane_id
        && let Some(active_pane) = panes.iter().find(|pane| pane.id.as_str() == active_pane_id)
        && let Some(cell) =
            pane_cell_for_position(active_pane, x, y, padding_x, padding_y, true, true)
    {
        return Some((active_pane.id.clone(), cell));
    }

    for pane in panes {
        if let Some(cell) = pane_cell_for_position(pane, x, y, padding_x, padding_y, clamp, false) {
            return Some((pane.id.clone(), cell));
        }
    }

    None
}

impl TerminalView {
    pub(in super::super) fn position_to_pane_cell(
        &self,
        position: gpui::Point<Pixels>,
        clamp: bool,
    ) -> Option<(String, CellPos)> {
        let tab = self.tabs.get(self.active_tab)?;
        let (padding_x, padding_y) = self.effective_terminal_padding();
        let (x, y) = self.terminal_content_position(position);
        resolve_pane_cell_for_position(
            &tab.panes,
            tab.active_pane_id(),
            x,
            y,
            padding_x,
            padding_y,
            clamp,
        )
    }

    pub(in super::super) fn has_selection(&self) -> bool {
        matches!((self.selection_anchor, self.selection_head), (Some(anchor), Some(head)) if self.selection_moved || anchor != head)
    }

    pub(in super::super) fn selection_range(&self) -> Option<(CellPos, CellPos)> {
        if !self.has_selection() {
            return None;
        }

        let (anchor, head) = (self.selection_anchor?, self.selection_head?);
        if (head.row, head.col) < (anchor.row, anchor.col) {
            Some((head, anchor))
        } else {
            Some((anchor, head))
        }
    }

    pub(in super::super) fn cell_is_selected(&self, col: usize, row: usize) -> bool {
        let Some((start, end)) = self.selection_range() else {
            return false;
        };

        let here = (row, col);
        here >= (start.row, start.col) && here <= (end.row, end.col)
    }

    pub(in super::super) fn viewport_row_from_term_line(
        term_line: i32,
        display_offset: usize,
    ) -> Option<usize> {
        usize::try_from(term_line + display_offset as i32).ok()
    }

    pub(in super::super) fn position_to_cell(
        &self,
        position: gpui::Point<Pixels>,
        clamp: bool,
    ) -> Option<CellPos> {
        let (pane_id, cell) = self.position_to_pane_cell(position, clamp)?;
        self.is_active_pane_id(&pane_id).then_some(cell)
    }

    pub(in super::super) fn selected_text(&self) -> Option<String> {
        let (start, end) = self.selection_range()?;
        let terminal = self.active_terminal()?;
        let size = terminal.size();
        let cols = size.cols as usize;
        let rows = size.rows as usize;
        if cols == 0 || rows == 0 {
            return None;
        }

        let clamped_start = CellPos {
            col: start.col.min(cols.saturating_sub(1)),
            row: start.row.min(rows.saturating_sub(1)),
        };
        let clamped_end = CellPos {
            col: end.col.min(cols.saturating_sub(1)),
            row: end.row.min(rows.saturating_sub(1)),
        };
        let (selection_start, selection_end) =
            if (clamped_end.row, clamped_end.col) < (clamped_start.row, clamped_start.col) {
                (clamped_end, clamped_start)
            } else {
                (clamped_start, clamped_end)
            };

        let min_row = selection_start.row;
        let max_row = selection_end.row;
        let grid_rows = max_row - min_row + 1;
        let mut grid = vec![vec![' '; cols]; grid_rows];
        fill_grid_rows_for_selection(terminal, min_row, max_row, cols, &mut grid);

        let mut lines = Vec::new();
        for row in min_row..=max_row {
            let col_start = if row == selection_start.row {
                selection_start.col
            } else {
                0
            };
            let col_end = if row == selection_end.row {
                selection_end.col
            } else {
                cols.saturating_sub(1)
            };
            if col_start > col_end {
                continue;
            }

            let grid_row = row - min_row;
            let mut line: String = grid[grid_row][col_start..=col_end].iter().collect();
            while line.ends_with(' ') {
                line.pop();
            }
            lines.push(line);
        }

        if lines.is_empty() {
            None
        } else {
            Some(lines.join("\n"))
        }
    }

    pub(in super::super) fn row_text(&self, row: usize) -> Option<Vec<char>> {
        let terminal = self.active_terminal()?;
        let size = terminal.size();
        let cols = size.cols as usize;
        let rows = size.rows as usize;
        if cols == 0 || row >= rows {
            return None;
        }

        let line = row_text_from_terminal(terminal, row, cols);

        Some(line)
    }

    fn terminal_selection_char_class(c: char) -> TerminalSelectionCharClass {
        if c.is_whitespace() {
            TerminalSelectionCharClass::Whitespace
        } else if c.is_alphanumeric() || c == '_' {
            TerminalSelectionCharClass::Word
        } else {
            TerminalSelectionCharClass::Other
        }
    }

    pub(in super::super) fn select_token_at_cell(&mut self, cell: CellPos) -> bool {
        let Some(line) = self.row_text(cell.row) else {
            return false;
        };
        if cell.col >= line.len() {
            return false;
        }

        let class = Self::terminal_selection_char_class(line[cell.col]);
        if class == TerminalSelectionCharClass::Whitespace {
            let Some(last_non_whitespace) = line.iter().rposition(|c| !c.is_whitespace()) else {
                return false;
            };
            if cell.col > last_non_whitespace {
                return false;
            }
        }

        let mut start_col = cell.col;
        while start_col > 0 && Self::terminal_selection_char_class(line[start_col - 1]) == class {
            start_col -= 1;
        }

        let mut end_col = cell.col;
        while end_col + 1 < line.len()
            && Self::terminal_selection_char_class(line[end_col + 1]) == class
        {
            end_col += 1;
        }

        self.selection_anchor = Some(CellPos {
            col: start_col,
            row: cell.row,
        });
        self.selection_head = Some(CellPos {
            col: end_col,
            row: cell.row,
        });
        self.selection_dragging = false;
        self.selection_moved = true;
        true
    }

    pub(in super::super) fn select_line_at_row(&mut self, row: usize) -> bool {
        let Some(terminal) = self.active_terminal() else {
            return false;
        };
        let size = terminal.size();
        let cols = size.cols as usize;
        let rows = size.rows as usize;
        if cols == 0 || row >= rows {
            return false;
        }

        self.selection_anchor = Some(CellPos { col: 0, row });
        self.selection_head = Some(CellPos {
            col: cols.saturating_sub(1),
            row,
        });
        self.selection_dragging = false;
        self.selection_moved = true;
        true
    }

    pub(in super::super) fn link_at_cell(&self, cell: CellPos) -> Option<HoveredLink> {
        let line = self.row_text(cell.row)?;
        let detected = find_link_in_line(&line, cell.col)?;

        Some(HoveredLink {
            row: cell.row,
            start_col: detected.start_col,
            end_col: detected.end_col,
            target: detected.target,
        })
    }

    pub(in super::super) fn open_link(url: &str) -> bool {
        #[cfg(target_os = "macos")]
        {
            return Command::new("open")
                .arg(url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map(|_| true)
                .unwrap_or(false);
        }
        #[cfg(target_os = "linux")]
        {
            return Command::new("xdg-open")
                .arg(url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map(|_| true)
                .unwrap_or(false);
        }
        #[cfg(target_os = "windows")]
        {
            return Command::new("cmd")
                .args(["/C", "start", "", url])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map(|_| true)
                .unwrap_or(false);
        }
    }

    pub(in super::super) fn is_link_modifier(modifiers: gpui::Modifiers) -> bool {
        modifiers.secondary() && !modifiers.alt && !modifiers.function
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{thread, time::Duration};
    use termy_terminal_ui::TerminalSize;

    #[test]
    fn viewport_row_maps_scrollback_lines_into_viewport() {
        assert_eq!(TerminalView::viewport_row_from_term_line(-3, 3), Some(0));
        assert_eq!(TerminalView::viewport_row_from_term_line(4, 3), Some(7));
    }

    #[test]
    fn terminal_read_adapter_extracts_rows_for_both_runtime_variants() {
        let size = TerminalSize {
            cols: 16,
            rows: 3,
            ..TerminalSize::default()
        };

        let tmux = Terminal::new_tmux(size, 128);
        tmux.feed_output(b"row-adapter\r\n");
        let tmux_row = row_text_from_terminal(&tmux, 0, usize::from(size.cols));
        assert_eq!(tmux_row.len(), usize::from(size.cols));
        assert!(tmux_row.iter().any(|c| !c.is_whitespace()));

        let native = Terminal::new_native(size, None, None, None, None)
            .expect("native terminal should initialize for row adapter test");
        native.write_input(b"printf native-row-adapter\r");
        let expected_native_token = "native-row";
        let mut native_row = row_text_from_terminal(&native, 0, usize::from(size.cols));
        for _ in 0..40 {
            let rendered_native_row: String = native_row.iter().collect();
            if rendered_native_row.contains(expected_native_token) {
                break;
            }
            thread::sleep(Duration::from_millis(25));
            let _ = native.process_events();
            native_row = row_text_from_terminal(&native, 0, usize::from(size.cols));
        }
        assert_eq!(native_row.len(), usize::from(size.cols));
        let rendered_native_row: String = native_row.iter().collect();
        assert!(rendered_native_row.contains(expected_native_token));
    }

    #[test]
    fn pane_row_mapping_uses_chrome_adjusted_pointer_y() {
        let chrome_height = 34.0;
        let padding_y = 6.0;
        let pane_top = 1u16;
        let cell_height = 20.0;
        let expected_row = 2i32;

        let window_y = chrome_height
            + padding_y
            + ((f32::from(pane_top) + expected_row as f32) * cell_height)
            + 0.1;
        let content_y = TerminalView::window_y_to_terminal_content_y(window_y, chrome_height);
        let origin_y = padding_y + (f32::from(pane_top) * cell_height);
        let row = ((content_y - origin_y) / cell_height).floor() as i32;

        assert_eq!(row, expected_row);
    }

    #[test]
    fn clamped_pane_lookup_returns_active_pane_when_pointer_inside_active_pane() {
        let rows = 6u16;
        let left_cols = 8u16;
        let right_cols = 8u16;
        let left_terminal = Terminal::new_tmux(
            TerminalSize {
                cols: left_cols,
                rows,
                ..TerminalSize::default()
            },
            128,
        );
        let right_terminal = Terminal::new_tmux(
            TerminalSize {
                cols: right_cols,
                rows,
                ..TerminalSize::default()
            },
            128,
        );

        let panes = vec![
            TerminalPane {
                id: "%left".to_string(),
                left: 0,
                top: 0,
                width: left_cols,
                height: rows,
                degraded: false,
                terminal: left_terminal,
            },
            TerminalPane {
                id: "%right".to_string(),
                left: left_cols,
                top: 0,
                width: right_cols,
                height: rows,
                degraded: false,
                terminal: right_terminal,
            },
        ];

        let cell_width: f32 = panes[0].terminal.size().cell_width.into();
        let cell_height: f32 = panes[0].terminal.size().cell_height.into();
        let pointer_x = (2.0 * cell_width) + 0.5;
        let pointer_y = (3.0 * cell_height) + 0.5;

        let resolved = resolve_pane_cell_for_position(
            &panes,
            Some("%left"),
            pointer_x,
            pointer_y,
            0.0,
            0.0,
            true,
        );
        assert_eq!(
            resolved,
            Some(("%left".to_string(), CellPos { col: 2, row: 3 }))
        );
    }

    #[test]
    fn clamped_pane_lookup_falls_back_to_active_pane_when_pointer_outside_all_panes() {
        let rows = 6u16;
        let left_cols = 8u16;
        let right_cols = 8u16;
        let left_terminal = Terminal::new_tmux(
            TerminalSize {
                cols: left_cols,
                rows,
                ..TerminalSize::default()
            },
            128,
        );
        let right_terminal = Terminal::new_tmux(
            TerminalSize {
                cols: right_cols,
                rows,
                ..TerminalSize::default()
            },
            128,
        );

        let panes = vec![
            TerminalPane {
                id: "%left".to_string(),
                left: 0,
                top: 0,
                width: left_cols,
                height: rows,
                degraded: false,
                terminal: left_terminal,
            },
            TerminalPane {
                id: "%right".to_string(),
                left: left_cols,
                top: 0,
                width: right_cols,
                height: rows,
                degraded: false,
                terminal: right_terminal,
            },
        ];

        let active_pane = &panes[0];
        let size = active_pane.terminal.size();
        let cell_width: f32 = size.cell_width.into();
        let cell_height: f32 = size.cell_height.into();
        let padding_x = 0.0;
        let padding_y = 0.0;
        let active_origin_x = padding_x + (f32::from(active_pane.left) * cell_width);
        let active_origin_y = padding_y + (f32::from(active_pane.top) * cell_height);
        let active_width = f32::from(active_pane.width) * cell_width;
        let active_height = f32::from(active_pane.height) * cell_height;

        // Outside both panes (to the far right and below) while clamp=true should
        // still return a clamped position in the active pane.
        let pointer_x = active_origin_x + active_width + (f32::from(right_cols) * cell_width);
        let pointer_y = active_origin_y + active_height + (2.0 * cell_height);

        let clamped_x = (pointer_x - active_origin_x).clamp(0.0, active_width - f32::EPSILON);
        let clamped_y = (pointer_y - active_origin_y).clamp(0.0, active_height - f32::EPSILON);
        let expected_col =
            ((clamped_x / cell_width).floor() as i32).clamp(0, i32::from(size.cols) - 1) as usize;
        let expected_row =
            ((clamped_y / cell_height).floor() as i32).clamp(0, i32::from(size.rows) - 1) as usize;

        let resolved = resolve_pane_cell_for_position(
            &panes,
            Some("%left"),
            pointer_x,
            pointer_y,
            padding_x,
            padding_y,
            true,
        );
        assert_eq!(
            resolved,
            Some((
                "%left".to_string(),
                CellPos {
                    col: expected_col,
                    row: expected_row,
                },
            ))
        );
    }
}
