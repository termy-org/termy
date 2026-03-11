use super::*;

const SELECTION_DRAG_AUTOSCROLL_MAX_LINES: i32 = 3;
const CURSOR_MOVE_PREVIEW_MS: u64 = 75;

impl TerminalView {
    fn is_plain_click_cursor_move_gesture(modifiers: gpui::Modifiers, click_count: usize) -> bool {
        click_count == 1
            && !modifiers.control
            && !modifiers.alt
            && !modifiers.shift
            && !modifiers.platform
            && !modifiers.function
    }

    fn click_cursor_move_allowed_for_state(
        runtime_kind: RuntimeKind,
        running_process: bool,
        has_current_command: bool,
        alternate_screen_mode: bool,
    ) -> bool {
        if running_process || alternate_screen_mode {
            return false;
        }

        match runtime_kind {
            RuntimeKind::Tmux => true,
            RuntimeKind::Native => !has_current_command,
        }
    }

    fn cursor_move_input_for_click_target(cursor: CellPos, target: CellPos) -> Option<Vec<u8>> {
        if cursor.row != target.row || cursor.col == target.col {
            return None;
        }

        let (sequence, repeats) = if target.col > cursor.col {
            (b"\x1b[C".as_slice(), target.col - cursor.col)
        } else {
            (b"\x1b[D".as_slice(), cursor.col - target.col)
        };

        let mut input = Vec::with_capacity(sequence.len() * repeats);
        for _ in 0..repeats {
            input.extend_from_slice(sequence);
        }
        Some(input)
    }

    fn pending_cursor_move_click_for_mouse_down(
        &self,
        event: &MouseDownEvent,
    ) -> Option<PendingCursorMoveClick> {
        if event.button != MouseButton::Left
            || !Self::is_plain_click_cursor_move_gesture(event.modifiers, event.click_count)
        {
            return None;
        }

        let (pane_id, target) = self.position_to_pane_cell(event.position, false)?;
        let selection_start = self.selection_pos_for_pane_cell(pane_id.as_str(), target)?;
        let terminal = self.pane_terminal_by_id(pane_id.as_str())?;
        let tab = self.tabs.get(self.active_tab)?;
        let runtime_kind = self.runtime_kind();
        if !Self::click_cursor_move_allowed_for_state(
            runtime_kind,
            tab.running_process,
            tab.current_command.is_some(),
            terminal.alternate_screen_mode(),
        ) {
            return None;
        }

        Some(PendingCursorMoveClick {
            pane_id,
            selection_start,
            start_cell: target,
            target,
        })
    }

    fn begin_selection_drag_from_pending_cursor_move(
        &mut self,
        position: gpui::Point<Pixels>,
    ) -> bool {
        let Some(pending) = self.pending_cursor_move_click.as_ref() else {
            return false;
        };
        let Some((pane_id, cell)) = self.position_to_pane_cell(position, true) else {
            return false;
        };
        if !Self::pending_cursor_move_starts_selection(pending, pane_id.as_str(), cell) {
            return false;
        }

        let Some(selection_head) = self.selection_pos_for_pane_cell(pane_id.as_str(), cell) else {
            return false;
        };
        let selection_start = pending.selection_start;
        self.pending_cursor_move_click = None;
        self.selection_anchor = Some(selection_start);
        self.selection_head = Some(selection_head);
        self.selection_dragging = true;
        self.selection_moved = selection_start != selection_head;
        true
    }

    fn pending_cursor_move_starts_selection(
        pending: &PendingCursorMoveClick,
        pane_id: &str,
        cell: CellPos,
    ) -> bool {
        pending.pane_id == pane_id && cell != pending.start_cell
    }

    fn maybe_move_cursor_to_click_target(&mut self, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_cursor_move_click.take() else {
            return;
        };
        if !self.is_active_pane_id(pending.pane_id.as_str()) {
            return;
        }

        let Some(terminal) = self.pane_terminal_by_id(pending.pane_id.as_str()) else {
            return;
        };
        let (cursor_col, cursor_row) = terminal.cursor_position();
        let Some(input) = Self::cursor_move_input_for_click_target(
            CellPos {
                col: cursor_col,
                row: cursor_row,
            },
            pending.target,
        ) else {
            return;
        };

        let cursor_style = terminal
            .cursor_state()
            .map(|cursor_state| cursor_state.style)
            .unwrap_or(TerminalCursorStyle::Block);
        self.start_cursor_move_preview(
            PendingCursorMovePreview {
                pane_id: pending.pane_id,
                target: pending.target,
                style: cursor_style,
            },
            cx,
        );
        self.write_terminal_input(&input, cx);
    }

    fn start_cursor_move_preview(
        &mut self,
        preview: PendingCursorMovePreview,
        cx: &mut Context<Self>,
    ) {
        self.pending_cursor_move_preview = Some(preview.clone());
        cx.notify();

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            smol::Timer::after(Duration::from_millis(CURSOR_MOVE_PREVIEW_MS)).await;
            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    if view.pending_cursor_move_preview.as_ref() == Some(&preview) {
                        view.pending_cursor_move_preview = None;
                        cx.notify();
                    }
                })
            });
        })
        .detach();
    }

    fn finish_pending_cursor_move_click(
        &mut self,
        button: MouseButton,
        cx: &mut Context<Self>,
    ) -> bool {
        if button != MouseButton::Left || self.pending_cursor_move_click.is_none() {
            return false;
        }

        let selection_changed = self.clear_selection();
        let hovered_link_changed = self.clear_hovered_link();
        self.maybe_move_cursor_to_click_target(cx);
        if selection_changed || hovered_link_changed {
            cx.notify();
        }
        true
    }

    fn apply_vertical_tab_strip_resize_drag(&mut self, position: gpui::Point<Pixels>) -> bool {
        if self.vertical_tab_strip_resize_drag.is_none() || self.vertical_tabs_minimized {
            return false;
        }

        let pointer_x: f32 = position.x.into();
        let next_width =
            pointer_x.clamp(VERTICAL_TAB_STRIP_MIN_WIDTH, VERTICAL_TAB_STRIP_MAX_WIDTH);
        if (self.vertical_tabs_width - next_width).abs() < f32::EPSILON {
            return false;
        }

        self.vertical_tabs_width = next_width;
        self.clear_pane_render_caches();
        self.clear_terminal_scrollbar_marker_cache();
        self.cell_size = None;
        true
    }

    fn persist_vertical_tab_strip_width(&self) -> Result<(), String> {
        config::set_root_setting(
            termy_config_core::RootSettingId::VerticalTabsWidth,
            &self.vertical_tabs_width.to_string(),
        )
    }

    pub(in super::super) fn set_vertical_tabs_minimized(
        &mut self,
        minimized: bool,
    ) -> Result<(), String> {
        if self.vertical_tabs_minimized == minimized {
            return Ok(());
        }

        self.vertical_tabs_minimized = minimized;
        self.clear_pane_render_caches();
        self.clear_terminal_scrollbar_marker_cache();
        self.cell_size = None;
        self.mark_tab_strip_layout_dirty();
        config::set_root_setting(
            termy_config_core::RootSettingId::VerticalTabsMinimized,
            &minimized.to_string(),
        )
    }

    fn apply_agent_sidebar_resize_drag(&mut self, position: gpui::Point<Pixels>) -> bool {
        if self.agent_sidebar_resize_drag.is_none() {
            return false;
        }

        let viewport_width = self.last_viewport_size_px.map(|(width, _)| width as f32);
        let Some(viewport_width) = viewport_width else {
            return false;
        };

        let pointer_x: f32 = position.x.into();
        let next_width =
            (viewport_width - pointer_x).clamp(AGENT_SIDEBAR_MIN_WIDTH, AGENT_SIDEBAR_MAX_WIDTH);
        if (self.agent_sidebar_width - next_width).abs() < f32::EPSILON {
            return false;
        }

        self.agent_sidebar_width = next_width;
        self.clear_pane_render_caches();
        self.clear_terminal_scrollbar_marker_cache();
        self.cell_size = None;
        true
    }

    fn persist_agent_sidebar_width(&self) -> Result<(), String> {
        config::set_root_setting(
            termy_config_core::RootSettingId::AgentSidebarWidth,
            &self.agent_sidebar_width.to_string(),
        )
    }

    fn native_resize_overlap_cells(a_start: u16, a_end: u16, b_start: u16, b_end: u16) -> u16 {
        let start = a_start.max(b_start);
        let end = a_end.min(b_end);
        end.saturating_sub(start)
    }

    pub(in super::super) fn begin_pane_resize_drag(
        &mut self,
        pane_id: &str,
        axis: PaneResizeAxis,
        edge: PaneResizeEdge,
        position: gpui::Point<Pixels>,
    ) -> bool {
        let (x, y) = self.terminal_content_position(position);
        self.pane_resize_drag = Some(PaneResizeDragState {
            pane_id: pane_id.to_string(),
            axis,
            edge,
            start_x: x,
            start_y: y,
            applied_steps: 0,
        });
        true
    }

    fn pane_resize_hit_test(&self, position: gpui::Point<Pixels>) -> Option<PaneResizeDragState> {
        const DIVIDER_HIT_MARGIN_PX: f32 = 4.0;

        let tab = self.tabs.get(self.active_tab)?;
        let max_right_cells = tab
            .panes
            .iter()
            .map(|pane| u32::from(pane.left).saturating_add(u32::from(pane.width)))
            .max()
            .unwrap_or(0);
        let max_bottom_cells = tab
            .panes
            .iter()
            .map(|pane| u32::from(pane.top).saturating_add(u32::from(pane.height)))
            .max()
            .unwrap_or(0);
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
                && (u32::from(pane.left) + u32::from(pane.width)) < max_right_cells;
            let near_top = (y - top).abs() <= DIVIDER_HIT_MARGIN_PX && pane.top > 0;
            let near_bottom = (y - bottom).abs() <= DIVIDER_HIT_MARGIN_PX
                && (u32::from(pane.top) + u32::from(pane.height)) < max_bottom_cells;

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

    pub(in crate::terminal_view) fn native_resize_pane_step(
        &mut self,
        pane_id: &str,
        axis: PaneResizeAxis,
        edge: PaneResizeEdge,
        divider_delta: i16,
    ) -> bool {
        fn overlaps_any(span_start: u16, span_end: u16, spans: &[(u16, u16)]) -> bool {
            spans.iter().any(|(start, end)| {
                TerminalView::native_resize_overlap_cells(span_start, span_end, *start, *end) > 0
            })
        }

        if divider_delta == 0 {
            return false;
        }

        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return false;
        };
        let Some(target_index) = tab.panes.iter().position(|pane| pane.id == pane_id) else {
            return false;
        };
        let Some(target) = tab.panes.get(target_index) else {
            return false;
        };

        match axis {
            PaneResizeAxis::Horizontal => {
                let boundary = match edge {
                    PaneResizeEdge::Left => target.left,
                    PaneResizeEdge::Right => target.left.saturating_add(target.width),
                    PaneResizeEdge::Top | PaneResizeEdge::Bottom => return false,
                };
                let mut spans = vec![(target.top, target.top.saturating_add(target.height))];
                let mut left_indices = Vec::<usize>::new();
                let mut right_indices = Vec::<usize>::new();

                loop {
                    let mut changed = false;
                    for (index, pane) in tab.panes.iter().enumerate() {
                        let pane_top = pane.top;
                        let pane_bottom = pane.top.saturating_add(pane.height);
                        if !overlaps_any(pane_top, pane_bottom, &spans) {
                            continue;
                        }

                        let pane_left = pane.left;
                        let pane_right = pane.left.saturating_add(pane.width);
                        if pane_right == boundary && !left_indices.contains(&index) {
                            left_indices.push(index);
                            spans.push((pane_top, pane_bottom));
                            changed = true;
                        }
                        if pane_left == boundary && !right_indices.contains(&index) {
                            right_indices.push(index);
                            spans.push((pane_top, pane_bottom));
                            changed = true;
                        }
                    }
                    if !changed {
                        break;
                    }
                }

                if left_indices.is_empty() || right_indices.is_empty() {
                    return false;
                }
                let min_width = Self::native_min_extent_allowed(
                    tab.panes
                        .iter()
                        .map(|pane| pane.left.saturating_add(pane.width))
                        .max()
                        .unwrap_or(0),
                    tab.panes.len(),
                    Self::native_pane_min_extent_for_axis(PaneResizeAxis::Horizontal),
                );

                if divider_delta > 0 {
                    if right_indices.iter().any(|index| {
                        tab.panes[*index].width
                            < min_width.saturating_add(divider_delta.unsigned_abs())
                    }) {
                        return false;
                    }
                    for index in left_indices {
                        tab.panes[index].width =
                            tab.panes[index].width.saturating_add(divider_delta as u16);
                    }
                    for index in right_indices {
                        tab.panes[index].left =
                            tab.panes[index].left.saturating_add(divider_delta as u16);
                        tab.panes[index].width =
                            tab.panes[index].width.saturating_sub(divider_delta as u16);
                    }
                } else {
                    let shrink = divider_delta.unsigned_abs();
                    if left_indices
                        .iter()
                        .any(|index| tab.panes[*index].width < min_width.saturating_add(shrink))
                    {
                        return false;
                    }
                    for index in left_indices {
                        tab.panes[index].width = tab.panes[index].width.saturating_sub(shrink);
                    }
                    for index in right_indices {
                        tab.panes[index].left = tab.panes[index].left.saturating_sub(shrink);
                        tab.panes[index].width = tab.panes[index].width.saturating_add(shrink);
                    }
                }
            }
            PaneResizeAxis::Vertical => {
                let boundary = match edge {
                    PaneResizeEdge::Top => target.top,
                    PaneResizeEdge::Bottom => target.top.saturating_add(target.height),
                    PaneResizeEdge::Left | PaneResizeEdge::Right => return false,
                };
                let mut spans = vec![(target.left, target.left.saturating_add(target.width))];
                let mut top_indices = Vec::<usize>::new();
                let mut bottom_indices = Vec::<usize>::new();

                loop {
                    let mut changed = false;
                    for (index, pane) in tab.panes.iter().enumerate() {
                        let pane_left = pane.left;
                        let pane_right = pane.left.saturating_add(pane.width);
                        if !overlaps_any(pane_left, pane_right, &spans) {
                            continue;
                        }

                        let pane_top = pane.top;
                        let pane_bottom = pane.top.saturating_add(pane.height);
                        if pane_bottom == boundary && !top_indices.contains(&index) {
                            top_indices.push(index);
                            spans.push((pane_left, pane_right));
                            changed = true;
                        }
                        if pane_top == boundary && !bottom_indices.contains(&index) {
                            bottom_indices.push(index);
                            spans.push((pane_left, pane_right));
                            changed = true;
                        }
                    }
                    if !changed {
                        break;
                    }
                }

                if top_indices.is_empty() || bottom_indices.is_empty() {
                    return false;
                }
                let min_height = Self::native_min_extent_allowed(
                    tab.panes
                        .iter()
                        .map(|pane| pane.top.saturating_add(pane.height))
                        .max()
                        .unwrap_or(0),
                    tab.panes.len(),
                    Self::native_pane_min_extent_for_axis(PaneResizeAxis::Vertical),
                );

                if divider_delta > 0 {
                    if bottom_indices.iter().any(|index| {
                        tab.panes[*index].height
                            < min_height.saturating_add(divider_delta.unsigned_abs())
                    }) {
                        return false;
                    }
                    for index in top_indices {
                        tab.panes[index].height =
                            tab.panes[index].height.saturating_add(divider_delta as u16);
                    }
                    for index in bottom_indices {
                        tab.panes[index].top =
                            tab.panes[index].top.saturating_add(divider_delta as u16);
                        tab.panes[index].height =
                            tab.panes[index].height.saturating_sub(divider_delta as u16);
                    }
                } else {
                    let shrink = divider_delta.unsigned_abs();
                    if top_indices
                        .iter()
                        .any(|index| tab.panes[*index].height < min_height.saturating_add(shrink))
                    {
                        return false;
                    }
                    for index in top_indices {
                        tab.panes[index].height = tab.panes[index].height.saturating_sub(shrink);
                    }
                    for index in bottom_indices {
                        tab.panes[index].top = tab.panes[index].top.saturating_sub(shrink);
                        tab.panes[index].height = tab.panes[index].height.saturating_add(shrink);
                    }
                }
            }
        }

        true
    }

    fn apply_pane_resize_drag(&mut self, position: gpui::Point<Pixels>) -> bool {
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
        let mut completed_steps = 0i32;
        for _ in 0..step_delta.unsigned_abs() {
            let resized = match self.runtime_kind() {
                RuntimeKind::Tmux => {
                    // Left/top drags invert the tmux resize direction relative to cursor delta,
                    // because dragging toward the pane interior shrinks that edge.
                    let positive_direction = match edge {
                        PaneResizeEdge::Left | PaneResizeEdge::Top => step_delta.is_negative(),
                        PaneResizeEdge::Right | PaneResizeEdge::Bottom => step_delta.is_positive(),
                    };
                    self.tmux_resize_pane_step(pane_id.as_str(), axis, positive_direction)
                }
                RuntimeKind::Native => self.native_resize_pane_step(
                    pane_id.as_str(),
                    axis,
                    edge,
                    if step_delta.is_positive() { 1 } else { -1 },
                ),
            };
            if resized {
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
            self.maybe_move_cursor_to_click_target(cx);
        } else {
            self.pending_cursor_move_click = None;
            self.copy_selection_to_clipboard_if_enabled(cx);
        }
        self.clear_hovered_link();
        cx.notify();
        true
    }

    fn copy_selection_to_clipboard_if_enabled(&mut self, cx: &mut Context<Self>) {
        if self.copy_on_select {
            if let Some(text) = self.selected_text() {
                cx.write_to_clipboard(ClipboardItem::new_string(text));
                if self.copy_on_select_toast {
                    termy_toast::enqueue_toast(
                        termy_toast::ToastKind::Success,
                        "Copied",
                        Some(std::time::Duration::from_millis(1500)),
                    );
                }
            }
        }
    }

    fn should_finish_selection_drag(button: MouseButton, selection_dragging: bool) -> bool {
        button == MouseButton::Left && selection_dragging
    }

    fn is_terminal_context_menu_passthrough(modifiers: gpui::Modifiers) -> bool {
        modifiers.shift
    }

    pub(in super::super) fn handle_global_mouse_move_event(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        if self.vertical_tab_strip_resize_drag.is_some() && event.dragging() {
            if self.apply_vertical_tab_strip_resize_drag(event.position) {
                cx.notify();
            }
            return;
        }

        if self.agent_sidebar_resize_drag.is_some() && event.dragging() {
            if self.apply_agent_sidebar_resize_drag(event.position) {
                cx.notify();
            }
            return;
        }

        if self.try_forward_mouse_move(event, cx) {
            return;
        }

        if event.pressed_button == Some(MouseButton::Left)
            && self.begin_selection_drag_from_pending_cursor_move(event.position)
        {
            self.clear_hovered_link();
            cx.notify();
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
        if event.button == MouseButton::Right
            && Self::is_terminal_context_menu_passthrough(event.modifiers)
        {
            return self.force_forward_right_mouse_up(event, cx);
        }

        if event.button == MouseButton::Left && self.agent_sidebar_resize_drag.take().is_some() {
            if let Err(error) = self.persist_agent_sidebar_width() {
                termy_toast::error(error);
            }
            cx.notify();
            return true;
        }

        if event.button == MouseButton::Left && self.vertical_tab_strip_resize_drag.take().is_some()
        {
            if let Err(error) = self.persist_vertical_tab_strip_width() {
                termy_toast::error(error);
            }
            cx.notify();
            return true;
        }

        if self.try_forward_mouse_up(event, cx) {
            return true;
        }

        if self.finish_pending_cursor_move_click(event.button, cx) {
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
        self.pending_cursor_move_click = None;
        self.pending_cursor_move_preview = None;

        // Focus the terminal on click
        self.focus_handle.focus(window, cx);
        self.reset_cursor_blink_phase();
        if event.button != MouseButton::Right {
            let _ = self.close_terminal_context_menu(cx);
        }
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

        if event.button == MouseButton::Right {
            if Self::is_terminal_context_menu_passthrough(event.modifiers) {
                let _ = self.close_terminal_context_menu(cx);
                let _ = self.force_forward_right_mouse_down(event, cx);
                return;
            }

            if let Some((pane_id, _)) = self.position_to_pane_cell(event.position, false)
                && !self.is_active_pane_id(pane_id.as_str())
            {
                let _ = self.focus_pane_target(pane_id.as_str(), cx);
            }
            self.open_terminal_context_menu(event.position, cx);
            cx.stop_propagation();
            return;
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
            self.copy_selection_to_clipboard_if_enabled(cx);
            self.clear_hovered_link();
            cx.notify();
            return;
        }

        if event.click_count == 2 && self.select_token_at_cell(cell) {
            self.copy_selection_to_clipboard_if_enabled(cx);
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

        self.pending_cursor_move_click = self.pending_cursor_move_click_for_mouse_down(event);
        if self.pending_cursor_move_click.is_some() {
            if self.clear_hovered_link() {
                cx.notify();
            }
            return;
        }

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
        if self.agent_sidebar_resize_drag.is_some() {
            if event.dragging() {
                if self.apply_agent_sidebar_resize_drag(event.position) {
                    cx.notify();
                }
            } else if self.agent_sidebar_resize_drag.take().is_some() {
                if let Err(error) = self.persist_agent_sidebar_width() {
                    termy_toast::error(error);
                }
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }
        if self.vertical_tab_strip_resize_drag.is_some() {
            if event.dragging() {
                if self.apply_vertical_tab_strip_resize_drag(event.position) {
                    cx.notify();
                }
            } else if self.vertical_tab_strip_resize_drag.take().is_some() {
                if let Err(error) = self.persist_vertical_tab_strip_width() {
                    termy_toast::error(error);
                }
                cx.notify();
            }
            cx.stop_propagation();
            return;
        }
        if event.dragging()
            && self.terminal_scrollbar_track_hold_local_y.is_some()
            && let Some(hit) = self.terminal_scrollbar_hit_test(event.position, window)
        {
            self.update_terminal_scrollbar_track_hold(hit.local_y);
            cx.stop_propagation();
            return;
        }
        if self.pane_resize_drag.is_some() {
            if event.dragging() {
                if self.apply_pane_resize_drag(event.position) {
                    cx.notify();
                }
            } else if self.pane_resize_drag.take().is_some() {
                if self.runtime_kind() == RuntimeKind::Native {
                    self.schedule_persist_native_workspace();
                }
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
        if event.button == MouseButton::Right
            && Self::is_terminal_context_menu_passthrough(event.modifiers)
        {
            let _ = self.force_forward_right_mouse_up(event, cx);
            return;
        }

        if event.button == MouseButton::Left && self.stop_terminal_scrollbar_track_hold() {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if event.button == MouseButton::Left && self.finish_terminal_scrollbar_drag(cx) {
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if event.button == MouseButton::Left && self.agent_sidebar_resize_drag.take().is_some() {
            if let Err(error) = self.persist_agent_sidebar_width() {
                termy_toast::error(error);
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if event.button == MouseButton::Left && self.vertical_tab_strip_resize_drag.take().is_some()
        {
            if let Err(error) = self.persist_vertical_tab_strip_width() {
                termy_toast::error(error);
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }
        if event.button == MouseButton::Left && self.pane_resize_drag.take().is_some() {
            if self.runtime_kind() == RuntimeKind::Native {
                self.schedule_persist_native_workspace();
            }
            cx.stop_propagation();
            cx.notify();
            return;
        }

        if self.finish_pending_cursor_move_click(event.button, cx) {
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

    #[test]
    fn terminal_context_menu_passthrough_requires_shift_modifier() {
        let shifted = gpui::Modifiers {
            shift: true,
            ..gpui::Modifiers::default()
        };
        let plain = gpui::Modifiers::default();

        assert!(TerminalView::is_terminal_context_menu_passthrough(shifted));
        assert!(!TerminalView::is_terminal_context_menu_passthrough(plain));
    }

    #[test]
    fn cursor_move_input_for_click_target_moves_horizontally_only() {
        assert_eq!(
            TerminalView::cursor_move_input_for_click_target(
                CellPos { col: 2, row: 4 },
                CellPos { col: 5, row: 4 },
            ),
            Some(b"\x1b[C\x1b[C\x1b[C".to_vec())
        );
        assert_eq!(
            TerminalView::cursor_move_input_for_click_target(
                CellPos { col: 5, row: 4 },
                CellPos { col: 2, row: 4 },
            ),
            Some(b"\x1b[D\x1b[D\x1b[D".to_vec())
        );
        assert_eq!(
            TerminalView::cursor_move_input_for_click_target(
                CellPos { col: 5, row: 4 },
                CellPos { col: 2, row: 3 },
            ),
            None
        );
    }

    #[test]
    fn pending_cursor_move_only_turns_into_selection_after_cell_change() {
        let pending = PendingCursorMoveClick {
            pane_id: "%pane".to_string(),
            selection_start: SelectionPos { col: 2, line: 4 },
            start_cell: CellPos { col: 2, row: 4 },
            target: CellPos { col: 8, row: 4 },
        };

        assert!(!TerminalView::pending_cursor_move_starts_selection(
            &pending,
            "%pane",
            CellPos { col: 2, row: 4 },
        ));
        assert!(TerminalView::pending_cursor_move_starts_selection(
            &pending,
            "%pane",
            CellPos { col: 3, row: 4 },
        ));
        assert!(!TerminalView::pending_cursor_move_starts_selection(
            &pending,
            "%other",
            CellPos { col: 3, row: 4 },
        ));
    }

    #[test]
    fn cursor_move_preview_timer_matches_expected_budget() {
        assert_eq!(CURSOR_MOVE_PREVIEW_MS, 75);
    }

    #[test]
    fn click_cursor_move_allowed_state_works_without_prompt_markers() {
        assert!(TerminalView::click_cursor_move_allowed_for_state(
            RuntimeKind::Native,
            false,
            false,
            false,
        ));
        assert!(!TerminalView::click_cursor_move_allowed_for_state(
            RuntimeKind::Native,
            false,
            true,
            false,
        ));
        assert!(!TerminalView::click_cursor_move_allowed_for_state(
            RuntimeKind::Native,
            true,
            false,
            false,
        ));
        assert!(TerminalView::click_cursor_move_allowed_for_state(
            RuntimeKind::Tmux,
            false,
            true,
            false,
        ));
    }
}
