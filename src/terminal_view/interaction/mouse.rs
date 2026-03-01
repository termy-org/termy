use super::*;

impl TerminalView {
    fn pane_resize_hit_test(&self, position: gpui::Point<Pixels>) -> Option<PaneResizeDragState> {
        if self.runtime_kind() != RuntimeKind::Tmux {
            return None;
        }
        const DIVIDER_HIT_MARGIN_PX: f32 = 4.0;

        let tab = self.tabs.get(self.active_tab)?;
        let (padding_x, padding_y) = self.effective_terminal_padding();
        let x: f32 = position.x.into();
        let y: f32 = position.y.into();
        let mut best: Option<(f32, PaneResizeAxis, String)> = None;

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

            let left = padding_x + (f32::from(pane.left) * cell_width);
            let top = padding_y + (f32::from(pane.top) * cell_height);
            let right = left + (f32::from(size.cols) * cell_width);
            let bottom = top + (f32::from(size.rows) * cell_height);
            let inside = x >= left && x <= right && y >= top && y <= bottom;
            if !inside {
                continue;
            }

            let near_left = (x - left).abs() <= DIVIDER_HIT_MARGIN_PX && pane.left > 0;
            let near_right = (x - right).abs() <= DIVIDER_HIT_MARGIN_PX
                && (u32::from(pane.left) + u32::from(pane.width))
                    < u32::from(self.tmux_client_cols());
            let near_top = (y - top).abs() <= DIVIDER_HIT_MARGIN_PX && pane.top > 0;
            let near_bottom = (y - bottom).abs() <= DIVIDER_HIT_MARGIN_PX
                && (u32::from(pane.top) + u32::from(pane.height))
                    < u32::from(self.tmux_client_rows());

            if near_left || near_right {
                let distance = (x - if near_left { left } else { right }).abs();
                let candidate = (distance, PaneResizeAxis::Horizontal, pane.id.clone());
                if best
                    .as_ref()
                    .map(|current| candidate.0 < current.0)
                    .unwrap_or(true)
                {
                    best = Some(candidate);
                }
            }
            if near_top || near_bottom {
                let distance = (y - if near_top { top } else { bottom }).abs();
                let candidate = (distance, PaneResizeAxis::Vertical, pane.id.clone());
                if best
                    .as_ref()
                    .map(|current| candidate.0 < current.0)
                    .unwrap_or(true)
                {
                    best = Some(candidate);
                }
            }
        }

        best.map(|(_, axis, pane_id)| PaneResizeDragState {
            pane_id,
            axis,
            start_x: x,
            start_y: y,
            applied_steps: 0,
        })
    }

    fn apply_pane_resize_drag(&mut self, position: gpui::Point<Pixels>) -> bool {
        if self.runtime_kind() != RuntimeKind::Tmux {
            return false;
        }
        let Some(drag_state) = self.pane_resize_drag.as_ref() else {
            return false;
        };
        let pane_id = drag_state.pane_id.clone();
        let axis = drag_state.axis;
        let start_x = drag_state.start_x;
        let start_y = drag_state.start_y;
        let already_applied_steps = drag_state.applied_steps;

        let Some(terminal) = self.pane_terminal_by_id(pane_id.as_str()) else {
            return false;
        };
        let terminal_size = terminal.size();
        let axis_cell_pixels = match axis {
            PaneResizeAxis::Horizontal => {
                let width: f32 = terminal_size.cell_width.into();
                width
            }
            PaneResizeAxis::Vertical => {
                let height: f32 = terminal_size.cell_height.into();
                height
            }
        };
        if axis_cell_pixels <= f32::EPSILON {
            return false;
        }

        let x: f32 = position.x.into();
        let y: f32 = position.y.into();
        let delta_pixels = match axis {
            PaneResizeAxis::Horizontal => x - start_x,
            PaneResizeAxis::Vertical => y - start_y,
        };
        let desired_steps = (delta_pixels / axis_cell_pixels).trunc() as i32;
        let step_delta = desired_steps - already_applied_steps;
        if step_delta == 0 {
            return false;
        }
        let before_geometry = self
            .pane_ref_by_id(pane_id.as_str())
            .map(|pane| (pane.left, pane.top, pane.width, pane.height));

        let mut completed_steps = 0i32;
        let mut failed = false;
        for _ in 0..step_delta.unsigned_abs() {
            if self.tmux_resize_pane_step(pane_id.as_str(), axis, step_delta.is_positive()) {
                completed_steps += 1;
            } else {
                failed = true;
                break;
            }
        }

        if completed_steps == 0 {
            return false;
        }

        let applied_delta = if step_delta.is_positive() {
            completed_steps
        } else {
            -completed_steps
        };
        if let Some(drag) = self.pane_resize_drag.as_mut() {
            drag.applied_steps += applied_delta;
        }
        let refreshed = self.refresh_tmux_snapshot();
        if refreshed
            && !failed
            && before_geometry
                == self
                    .pane_ref_by_id(pane_id.as_str())
                    .map(|pane| (pane.left, pane.top, pane.width, pane.height))
        {
            termy_toast::info("Pane cannot resize further");
        }
        if failed {
            return refreshed;
        }
        refreshed
    }

    pub(in super::super) fn handle_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Focus the terminal on click
        self.focus_handle.focus(window, cx);
        self.reset_cursor_blink_phase();
        let mut changed = false;
        if event.button == MouseButton::Left && self.tab_strip.drag.is_some() {
            self.commit_tab_drag(cx);
        } else if self.reset_tab_drag_state() {
            changed = true;
        }
        if self.clear_tab_hover_state() {
            changed = true;
        }
        if changed {
            cx.notify();
        }

        if event.button != MouseButton::Left {
            return;
        }

        if let Some(drag) = self.pane_resize_hit_test(event.position) {
            self.pane_resize_drag = Some(drag);
            cx.stop_propagation();
            return;
        }

        if let Some(hit) = self.terminal_scrollbar_hit_test(event.position, window) {
            self.handle_terminal_scrollbar_mouse_down(hit, window, cx);
            cx.stop_propagation();
            return;
        }

        if let Some((pane_id, _)) = self.position_to_pane_cell(event.position, false)
            && !self.is_active_pane_id(pane_id.as_str())
        {
            let focused = self.focus_pane_target(pane_id.as_str(), cx);
            if focused && self.clear_hovered_link() {
                cx.notify();
            }
        }

        if Self::is_link_modifier(event.modifiers) {
            if let Some(cell) = self.position_to_cell(event.position, false) {
                if let Some(link) = self.link_at_cell(cell) {
                    if !Self::open_link(&link.target) {
                        termy_toast::error("Failed to open link");
                    }
                    if self.clear_hovered_link() {
                        cx.notify();
                    }
                    return;
                }
            }
        }

        let Some(cell) = self.position_to_cell(event.position, false) else {
            self.clear_selection();
            self.clear_hovered_link();
            cx.notify();
            return;
        };

        if event.click_count >= 3 {
            if self.select_line_at_row(cell.row) {
                self.clear_hovered_link();
                cx.notify();
                return;
            }
        }

        if event.click_count == 2 {
            if self.select_token_at_cell(cell) {
                self.clear_hovered_link();
                cx.notify();
                return;
            }
        }

        self.selection_anchor = Some(cell);
        self.selection_head = Some(cell);
        self.selection_dragging = true;
        self.selection_moved = false;
        self.clear_hovered_link();
        cx.notify();
    }

    pub(in super::super) fn handle_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tab_strip.drag.is_some() && !event.dragging() {
            self.commit_tab_drag(cx);
        }

        if self.clear_tab_hover_state() {
            cx.notify();
        }

        if self.terminal_scrollbar_drag.is_some() {
            if event.dragging() {
                self.handle_terminal_scrollbar_drag(event.position, window, cx);
            } else if self.finish_terminal_scrollbar_drag(cx) {
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }
        if self.pane_resize_drag.is_some() {
            if event.dragging() {
                if self.apply_pane_resize_drag(event.position) {
                    cx.notify();
                }
            } else if self.pane_resize_drag.take().is_some() {
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }

        if !self.selection_dragging || !event.dragging() {
            if Self::is_link_modifier(event.modifiers) {
                let hover_cell = self.position_to_cell(event.position, false);
                if let (Some(cell), Some(current)) = (hover_cell, self.hovered_link.as_ref()) {
                    if current.row == cell.row
                        && (current.start_col..=current.end_col).contains(&cell.col)
                    {
                        return;
                    }
                }

                let next = hover_cell.and_then(|cell| self.link_at_cell(cell));
                if self.hovered_link != next {
                    self.hovered_link = next;
                    cx.notify();
                }
            } else if self.clear_hovered_link() {
                cx.notify();
            }
            return;
        }

        let Some(next_cell) = self.position_to_cell(event.position, true) else {
            return;
        };

        if self.selection_head != Some(next_cell) {
            self.selection_head = Some(next_cell);
            if self.selection_anchor != self.selection_head {
                self.selection_moved = true;
            }
            self.clear_hovered_link();
            cx.notify();
        }
    }

    pub(in super::super) fn handle_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button == MouseButton::Left && self.finish_terminal_scrollbar_drag(cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if event.button == MouseButton::Left && self.pane_resize_drag.take().is_some() {
            cx.stop_propagation();
            cx.notify();
            return;
        }

        if event.button != MouseButton::Left || !self.selection_dragging {
            return;
        }

        if let Some(next_cell) = self.position_to_cell(event.position, true) {
            self.selection_head = Some(next_cell);
            if self.selection_anchor != self.selection_head {
                self.selection_moved = true;
            }
        }

        self.selection_dragging = false;
        if !self.selection_moved {
            self.clear_selection();
        }
        self.clear_hovered_link();
        cx.notify();
    }
}
