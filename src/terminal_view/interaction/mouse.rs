use super::*;

const SELECTION_DRAG_AUTOSCROLL_MAX_LINES: i32 = 3;

impl TerminalView {
    fn pane_resize_hit_test(&self, position: gpui::Point<Pixels>) -> Option<PaneResizeDragState> {
        if self.runtime_kind() != RuntimeKind::Tmux {
            return None;
        }
        const DIVIDER_HIT_MARGIN_PX: f32 = 4.0;

        let tab = self.tabs.get(self.active_tab)?;
        let (padding_x, padding_y) = self.effective_terminal_padding();
        let (x, y) = self.terminal_content_position(position);
        let mut best: Option<(f32, PaneResizeAxis, PaneResizeEdge, String)> = None;

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
                let edge = if near_left {
                    PaneResizeEdge::Left
                } else {
                    PaneResizeEdge::Right
                };
                let candidate = (distance, PaneResizeAxis::Horizontal, edge, pane.id.clone());
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
                let edge = if near_top {
                    PaneResizeEdge::Top
                } else {
                    PaneResizeEdge::Bottom
                };
                let candidate = (distance, PaneResizeAxis::Vertical, edge, pane.id.clone());
                if best
                    .as_ref()
                    .map(|current| candidate.0 < current.0)
                    .unwrap_or(true)
                {
                    best = Some(candidate);
                }
            }
        }

        best.map(|(_, axis, edge, pane_id)| PaneResizeDragState {
            pane_id,
            axis,
            edge,
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
        let edge = drag_state.edge;
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

        let (current_x, current_y) = self.terminal_content_position(position);
        let delta_pixels = match axis {
            PaneResizeAxis::Horizontal => current_x - start_x,
            PaneResizeAxis::Vertical => current_y - start_y,
        };
        let desired_steps = (delta_pixels / axis_cell_pixels).trunc() as i32;
        let step_delta = desired_steps - already_applied_steps;
        if step_delta == 0 {
            return false;
        }
        // Left/top drags invert the tmux resize direction relative to cursor delta,
        // because dragging toward the pane interior shrinks that edge.
        let positive_direction = match edge {
            PaneResizeEdge::Left | PaneResizeEdge::Top => step_delta.is_negative(),
            PaneResizeEdge::Right | PaneResizeEdge::Bottom => step_delta.is_positive(),
        };
        let mut completed_steps = 0i32;
        for _ in 0..step_delta.unsigned_abs() {
            if self.tmux_resize_pane_step(pane_id.as_str(), axis, positive_direction) {
                completed_steps += 1;
            } else {
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
        true
    }

    pub(in super::super) fn selection_drag_autoscroll_lines_from_bounds(
        pointer_y: f32,
        top: f32,
        bottom: f32,
        line_height: f32,
    ) -> i32 {
        if line_height <= f32::EPSILON || top >= bottom {
            return 0;
        }

        let lines = if pointer_y < top {
            ((top - pointer_y).powf(1.1) / line_height).ceil() as i32
        } else if pointer_y > bottom {
            -((pointer_y - bottom).powf(1.1) / line_height).ceil() as i32
        } else {
            0
        };

        lines.clamp(
            -SELECTION_DRAG_AUTOSCROLL_MAX_LINES,
            SELECTION_DRAG_AUTOSCROLL_MAX_LINES,
        )
    }

    fn selection_drag_autoscroll_lines(&self, position: gpui::Point<Pixels>) -> i32 {
        let Some(geometry) = self.terminal_viewport_geometry() else {
            return 0;
        };
        let Some(terminal) = self.active_terminal() else {
            return 0;
        };
        let line_height: f32 = terminal.size().cell_height.into();
        let (_, pointer_y) = self.terminal_content_position(position);
        let top = geometry.origin_y;
        let bottom = geometry.origin_y + geometry.height.max(0.0);
        Self::selection_drag_autoscroll_lines_from_bounds(pointer_y, top, bottom, line_height)
    }

    fn update_selection_head_from_position(
        &mut self,
        position: gpui::Point<Pixels>,
        clamp: bool,
    ) -> bool {
        let Some(next) = self.position_to_selection_pos(position, clamp) else {
            return false;
        };

        if self.selection_head != Some(next) {
            self.selection_head = Some(next);
            if self.selection_anchor != self.selection_head {
                self.selection_moved = true;
            }
            true
        } else {
            false
        }
    }

    fn handle_selection_drag_motion(
        &mut self,
        position: gpui::Point<Pixels>,
        allow_autoscroll: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.selection_dragging {
            return false;
        }

        let mut changed = false;
        if allow_autoscroll {
            let delta_lines = self.selection_drag_autoscroll_lines(position);
            if delta_lines != 0
                && self
                    .active_terminal()
                    .is_some_and(|terminal| terminal.scroll_display(delta_lines))
            {
                self.mark_terminal_scrollbar_activity(cx);
                changed = true;
            }
        }

        if self.update_selection_head_from_position(position, true) {
            changed = true;
        }

        if changed {
            self.clear_hovered_link();
            cx.notify();
        }
        changed
    }

    fn finish_selection_drag_at_position(
        &mut self,
        position: Option<gpui::Point<Pixels>>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.selection_dragging {
            return false;
        }

        if let Some(position) = position {
            self.update_selection_head_from_position(position, true);
        }

        self.selection_dragging = false;
        if !self.selection_moved {
            self.clear_selection();
        }
        self.clear_hovered_link();
        cx.notify();
        true
    }

    fn should_finish_selection_drag(button: MouseButton, selection_dragging: bool) -> bool {
        button == MouseButton::Left && selection_dragging
    }

    pub(in super::super) fn handle_global_mouse_move_event(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        if self.try_forward_mouse_move(event, cx) {
            return;
        }

        if event.pressed_button != Some(MouseButton::Left) || !self.selection_dragging {
            return;
        }

        self.handle_selection_drag_motion(event.position, true, cx);
    }

    pub(in super::super) fn handle_global_mouse_up_event(
        &mut self,
        event: &MouseUpEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.try_forward_mouse_up(event, cx) {
            return true;
        }

        if !Self::should_finish_selection_drag(event.button, self.selection_dragging) {
            return false;
        }

        self.finish_selection_drag_at_position(Some(event.position), cx)
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

        if self.try_forward_mouse_down(event, cx) {
            return;
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

        if Self::is_link_modifier(event.modifiers)
            && let Some(cell) = self.position_to_cell(event.position, false)
            && let Some(link) = self.link_at_cell(cell)
        {
            if !Self::open_link(&link.target) {
                termy_toast::error("Failed to open link");
            }
            if self.clear_hovered_link() {
                cx.notify();
            }
            return;
        }

        let Some(cell) = self.position_to_cell(event.position, false) else {
            self.clear_selection();
            self.clear_hovered_link();
            cx.notify();
            return;
        };

        if event.click_count >= 3 && self.select_line_at_row(cell.row) {
            self.clear_hovered_link();
            cx.notify();
            return;
        }

        if event.click_count == 2 && self.select_token_at_cell(cell) {
            self.clear_hovered_link();
            cx.notify();
            return;
        }

        let Some(selection_pos) = self.selection_pos_for_cell(cell) else {
            self.clear_selection();
            self.clear_hovered_link();
            cx.notify();
            return;
        };

        self.selection_anchor = Some(selection_pos);
        self.selection_head = Some(selection_pos);
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

        if self.try_forward_mouse_move(event, cx) {
            return;
        }

        if !self.selection_dragging || !event.dragging() {
            if Self::is_link_modifier(event.modifiers) {
                let hover_cell = self.position_to_cell(event.position, false);
                if let (Some(cell), Some(current)) = (hover_cell, self.hovered_link.as_ref())
                    && current.row == cell.row
                    && (current.start_col..=current.end_col).contains(&cell.col)
                {
                    return;
                }

                let next = hover_cell.and_then(|cell| self.link_at_cell(cell));
                if self.hovered_link != next {
                    self.hovered_link = next;
                    cx.notify();
                }
            } else if self.clear_hovered_link() {
                cx.notify();
            }
        }

        // Selection drag updates are handled by the global mouse-move listener so drag
        // behavior remains continuous when the pointer exits the terminal bounds.
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

        if !Self::should_finish_selection_drag(event.button, self.selection_dragging) {
            return;
        }

        self.finish_selection_drag_at_position(Some(event.position), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_drag_autoscroll_lines_scrolls_up_when_pointer_above_top() {
        let lines =
            TerminalView::selection_drag_autoscroll_lines_from_bounds(90.0, 100.0, 300.0, 20.0);
        assert!(lines > 0);
    }

    #[test]
    fn selection_drag_autoscroll_lines_scrolls_down_when_pointer_below_bottom() {
        let lines =
            TerminalView::selection_drag_autoscroll_lines_from_bounds(330.0, 100.0, 300.0, 20.0);
        assert!(lines < 0);
    }

    #[test]
    fn selection_drag_autoscroll_lines_is_zero_inside_bounds() {
        let lines =
            TerminalView::selection_drag_autoscroll_lines_from_bounds(200.0, 100.0, 300.0, 20.0);
        assert_eq!(lines, 0);
    }

    #[test]
    fn selection_drag_autoscroll_lines_clamps_to_max_speed() {
        let up = TerminalView::selection_drag_autoscroll_lines_from_bounds(
            -10_000.0, 100.0, 300.0, 20.0,
        );
        let down =
            TerminalView::selection_drag_autoscroll_lines_from_bounds(10_000.0, 100.0, 300.0, 20.0);

        assert_eq!(up, SELECTION_DRAG_AUTOSCROLL_MAX_LINES);
        assert_eq!(down, -SELECTION_DRAG_AUTOSCROLL_MAX_LINES);
    }

    #[test]
    fn finish_selection_drag_only_for_left_button_with_active_drag() {
        assert!(TerminalView::should_finish_selection_drag(
            MouseButton::Left,
            true
        ));
        assert!(!TerminalView::should_finish_selection_drag(
            MouseButton::Left,
            false
        ));
        assert!(!TerminalView::should_finish_selection_drag(
            MouseButton::Right,
            true
        ));
    }
}
