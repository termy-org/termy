use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalSelectionCharClass {
    Whitespace,
    Word,
    Other,
}


impl TerminalView {
    pub(in super::super) fn position_to_pane_cell(
        &self,
        position: gpui::Point<Pixels>,
        clamp: bool,
    ) -> Option<(String, CellPos)> {
        let tab = self.tabs.get(self.active_tab)?;
        let (padding_x, padding_y) = self.effective_terminal_padding();
        let x: f32 = position.x.into();
        let y: f32 = position.y.into();
        let active_pane_id = tab.active_pane_id();

        for pane in &tab.panes {
            let size = pane.terminal.size();
            if size.cols == 0 || size.rows == 0 {
                continue;
            }
            let cell_width: f32 = size.cell_width.into();
            let cell_height: f32 = size.cell_height.into();
            if cell_width <= f32::EPSILON || cell_height <= f32::EPSILON {
                continue;
            }

            let origin_x = padding_x + (f32::from(pane.left) * cell_width);
            let origin_y = padding_y + (f32::from(pane.top) * cell_height);
            let width = f32::from(size.cols) * cell_width;
            let height = f32::from(size.rows) * cell_height;
            if width <= f32::EPSILON || height <= f32::EPSILON {
                continue;
            }

            let mut local_x = x - origin_x;
            let mut local_y = y - origin_y;
            let is_inside =
                local_x >= 0.0 && local_x < width && local_y >= 0.0 && local_y < height;
            if !is_inside {
                if !clamp || active_pane_id != Some(pane.id.as_str()) {
                    continue;
                }
                local_x = local_x.clamp(0.0, width - f32::EPSILON);
                local_y = local_y.clamp(0.0, height - f32::EPSILON);
            }

            let max_col = i32::from(size.cols) - 1;
            let max_row = i32::from(size.rows) - 1;
            if max_col < 0 || max_row < 0 {
                continue;
            }

            let mut col = (local_x / cell_width).floor() as i32;
            let mut row = (local_y / cell_height).floor() as i32;
            if clamp {
                col = col.clamp(0, max_col);
                row = row.clamp(0, max_row);
            } else if col < 0 || col > max_col || row < 0 || row > max_row {
                continue;
            }

            return Some((
                pane.id.clone(),
                CellPos {
                    col: col as usize,
                    row: row as usize,
                },
            ));
        }

        None
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
        let size = self.active_terminal().size();
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
        let (selection_start, selection_end) = if (clamped_end.row, clamped_end.col)
            < (clamped_start.row, clamped_start.col)
        {
            (clamped_end, clamped_start)
        } else {
            (clamped_start, clamped_end)
        };

        let min_row = selection_start.row;
        let max_row = selection_end.row;
        let grid_rows = max_row - min_row + 1;
        let mut grid = vec![vec![' '; cols]; grid_rows];
        match self.active_terminal() {
            Terminal::Tmux(terminal) => terminal.with_term(|term| {
                let content = term.renderable_content();
                for cell in content.display_iter {
                    let Some(row) =
                        Self::viewport_row_from_term_line(cell.point.line.0, content.display_offset)
                    else {
                        continue;
                    };
                    let col = cell.point.column.0;
                    if row < min_row || row > max_row || col >= cols {
                        continue;
                    }

                    let c = cell.cell.c;
                    if c != '\0' {
                        let grid_row = row - min_row;
                        grid[grid_row][col] = if c.is_control() { ' ' } else { c };
                    }
                }
            }),
            Terminal::Native(terminal) => {
                if let Ok(terminal) = terminal.lock() {
                    terminal.with_term(|term| {
                        let content = term.renderable_content();
                        for cell in content.display_iter {
                            let Some(row) = Self::viewport_row_from_term_line(
                                cell.point.line.0,
                                content.display_offset,
                            ) else {
                                continue;
                            };
                            let col = cell.point.column.0;
                            if row < min_row || row > max_row || col >= cols {
                                continue;
                            }

                            let c = cell.cell.c;
                            if c != '\0' {
                                let grid_row = row - min_row;
                                grid[grid_row][col] = if c.is_control() { ' ' } else { c };
                            }
                        }
                    });
                }
            }
        }

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
        let size = self.active_terminal().size();
        let cols = size.cols as usize;
        let rows = size.rows as usize;
        if cols == 0 || row >= rows {
            return None;
        }

        let mut line = vec![' '; cols];
        match self.active_terminal() {
            Terminal::Tmux(terminal) => terminal.with_term(|term| {
                let content = term.renderable_content();
                for cell in content.display_iter {
                    let Some(cell_row) =
                        Self::viewport_row_from_term_line(cell.point.line.0, content.display_offset)
                    else {
                        continue;
                    };
                    if cell_row != row {
                        continue;
                    }

                    let col = cell.point.column.0;
                    if col >= cols {
                        continue;
                    }

                    if cell.cell.flags.intersects(
                        Flags::WIDE_CHAR_SPACER | Flags::LEADING_WIDE_CHAR_SPACER | Flags::HIDDEN,
                    ) {
                        continue;
                    }

                    let c = cell.cell.c;
                    if c != '\0' {
                        line[col] = if c.is_control() { ' ' } else { c };
                    }
                }
            }),
            Terminal::Native(terminal) => {
                if let Ok(terminal) = terminal.lock() {
                    terminal.with_term(|term| {
                        let content = term.renderable_content();
                        for cell in content.display_iter {
                            let Some(cell_row) = Self::viewport_row_from_term_line(
                                cell.point.line.0,
                                content.display_offset,
                            ) else {
                                continue;
                            };
                            if cell_row != row {
                                continue;
                            }

                            let col = cell.point.column.0;
                            if col >= cols {
                                continue;
                            }

                            if cell.cell.flags.intersects(
                                Flags::WIDE_CHAR_SPACER
                                    | Flags::LEADING_WIDE_CHAR_SPACER
                                    | Flags::HIDDEN,
                            ) {
                                continue;
                            }

                            let c = cell.cell.c;
                            if c != '\0' {
                                line[col] = if c.is_control() { ' ' } else { c };
                            }
                        }
                    });
                }
            }
        }

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
        let size = self.active_terminal().size();
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

    #[test]
    fn viewport_row_maps_scrollback_lines_into_viewport() {
        assert_eq!(TerminalView::viewport_row_from_term_line(-3, 3), Some(0));
        assert_eq!(TerminalView::viewport_row_from_term_line(4, 3), Some(7));
    }

}
