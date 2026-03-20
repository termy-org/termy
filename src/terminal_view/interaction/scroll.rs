use super::super::scrollbar as terminal_scrollbar;
use super::*;
use crate::ui::scrollbar as ui_scrollbar;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WheelScrollPaneDecision {
    UseActivePane,
    FocusHoveredPane,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WheelScrollRetargetResult {
    Unchanged,
    Switched,
    Abort,
    NotRetargeted,
}

impl TerminalView {
    fn terminal_scrollbar_interaction_layout(
        &self,
        window: &Window,
    ) -> Option<(
        TerminalScrollbarSurfaceGeometry,
        terminal_scrollbar::TerminalScrollbarLayout,
    )> {
        let pane_layout = self.active_terminal_pane_layout(window)?;
        let surface = pane_layout.scrollbar_surface;
        let layout = self.terminal_scrollbar_layout_for_track(surface.height)?;
        Some((surface, layout))
    }

    fn should_attempt_wheel_scroll_retarget(touch_phase: TouchPhase, delta_lines: i32) -> bool {
        matches!(touch_phase, TouchPhase::Moved) && delta_lines != 0
    }

    fn wheel_scroll_retarget_result(
        touch_phase: TouchPhase,
        delta_lines: i32,
        attempted_result: WheelScrollRetargetResult,
    ) -> WheelScrollRetargetResult {
        if Self::should_attempt_wheel_scroll_retarget(touch_phase, delta_lines) {
            attempted_result
        } else {
            WheelScrollRetargetResult::NotRetargeted
        }
    }

    fn wheel_scroll_retarget_result_for_decision(
        decision: WheelScrollPaneDecision,
        hovered_pane_id: Option<&str>,
        focus_succeeded: bool,
        active_pane_id: Option<&str>,
    ) -> WheelScrollRetargetResult {
        match decision {
            WheelScrollPaneDecision::UseActivePane => WheelScrollRetargetResult::Unchanged,
            WheelScrollPaneDecision::FocusHoveredPane => {
                let Some(pane_id) = hovered_pane_id else {
                    return WheelScrollRetargetResult::Unchanged;
                };

                if !focus_succeeded {
                    return WheelScrollRetargetResult::Abort;
                }
                if active_pane_id == Some(pane_id) {
                    WheelScrollRetargetResult::Switched
                } else {
                    WheelScrollRetargetResult::Abort
                }
            }
        }
    }

    fn consume_suppressed_scroll_event(
        &mut self,
        touch_phase: TouchPhase,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(until) = self.input_scroll_suppress_until else {
            return false;
        };

        match touch_phase {
            TouchPhase::Started => {
                self.input_scroll_suppress_until = None;
                false
            }
            TouchPhase::Ended => {
                self.input_scroll_suppress_until = None;
                cx.stop_propagation();
                true
            }
            TouchPhase::Moved => {
                let now = Instant::now();
                // Block residual momentum until we see a clear gesture boundary.
                // Fallback timeout keeps non-touch wheel devices from being blocked.
                let fallback_release = until + Duration::from_millis(INPUT_SCROLL_SUPPRESS_MS * 3);
                if now < fallback_release {
                    cx.stop_propagation();
                    true
                } else {
                    self.input_scroll_suppress_until = None;
                    false
                }
            }
        }
    }

    pub(in super::super) fn terminal_scroll_lines_from_pixels(
        accumulated_pixels: &mut f32,
        delta_pixels: f32,
        line_height: f32,
        viewport_height: f32,
    ) -> i32 {
        if line_height <= f32::EPSILON {
            return 0;
        }

        let old_offset = (*accumulated_pixels / line_height) as i32;
        *accumulated_pixels += delta_pixels;
        let new_offset = (*accumulated_pixels / line_height) as i32;

        if viewport_height > 0.0 {
            *accumulated_pixels %= viewport_height;
        }

        new_offset - old_offset
    }

    pub(in super::super) fn terminal_scroll_delta_to_lines(
        &mut self,
        event: &ScrollWheelEvent,
    ) -> i32 {
        match event.touch_phase {
            TouchPhase::Started => {
                self.terminal_scroll_accumulator_y = 0.0;
                0
            }
            TouchPhase::Ended => 0,
            TouchPhase::Moved => {
                let Some(terminal) = self.active_terminal() else {
                    return 0;
                };
                let size = terminal.size();
                if size.rows == 0 {
                    return 0;
                }

                let line_height: f32 = size.cell_height.into();
                let viewport_height = line_height * f32::from(size.rows);
                let raw_delta_pixels: f32 = event.delta.pixel_delta(size.cell_height).y.into();
                let delta_pixels = raw_delta_pixels * self.mouse_scroll_multiplier;

                Self::terminal_scroll_lines_from_pixels(
                    &mut self.terminal_scroll_accumulator_y,
                    delta_pixels,
                    line_height,
                    viewport_height,
                )
            }
        }
    }

    fn wheel_scroll_pane_decision(
        runtime_uses_tmux: bool,
        hovered_pane_id: Option<&str>,
        active_pane_id: Option<&str>,
    ) -> WheelScrollPaneDecision {
        if !runtime_uses_tmux || hovered_pane_id.is_none() || hovered_pane_id == active_pane_id {
            WheelScrollPaneDecision::UseActivePane
        } else {
            WheelScrollPaneDecision::FocusHoveredPane
        }
    }

    fn retarget_scroll_wheel_pane(
        &mut self,
        position: gpui::Point<Pixels>,
        cx: &mut Context<Self>,
    ) -> WheelScrollRetargetResult {
        let hovered_pane_id = self
            .position_to_pane_cell(position, false)
            .map(|(pane_id, _)| pane_id);
        let decision = Self::wheel_scroll_pane_decision(
            self.runtime_uses_tmux(),
            hovered_pane_id.as_deref(),
            self.active_pane_id(),
        );

        let focus_succeeded = matches!(decision, WheelScrollPaneDecision::FocusHoveredPane)
            && hovered_pane_id
                .as_deref()
                .is_some_and(|pane_id| self.focus_pane_target(pane_id, cx));

        Self::wheel_scroll_retarget_result_for_decision(
            decision,
            hovered_pane_id.as_deref(),
            focus_succeeded,
            self.active_pane_id(),
        )
    }

    pub(in super::super) fn terminal_scrollbar_hit_test(
        &self,
        position: gpui::Point<Pixels>,
        window: &Window,
    ) -> Option<TerminalScrollbarHit> {
        let terminal = self.active_terminal()?;
        let (display_offset, _) = terminal.scroll_state();
        let force_visible = display_offset > 0
            && self.terminal_scrollbar_mode() != ui_scrollbar::ScrollbarVisibilityMode::AlwaysOff;
        let alpha = self.terminal_scrollbar_alpha(Instant::now());
        if !force_visible
            && alpha <= f32::EPSILON
            && !self.terminal_scrollbar_visibility_controller.is_dragging()
        {
            return None;
        }

        let (surface, layout) = self.terminal_scrollbar_interaction_layout(window)?;
        let gutter = surface.gutter_frame()?;
        let (x, y) = self.terminal_content_position(position);
        if x < gutter.left || x > gutter.left + gutter.width {
            return None;
        }

        let local_y = surface.local_y(y)?;
        let metrics = layout.metrics;
        let thumb_hit =
            local_y >= metrics.thumb_top && local_y <= metrics.thumb_top + metrics.thumb_height;

        Some(TerminalScrollbarHit {
            local_y,
            thumb_hit,
            thumb_top: metrics.thumb_top,
        })
    }

    fn apply_terminal_scroll_offset(
        &mut self,
        target_offset: f32,
        layout: terminal_scrollbar::TerminalScrollbarLayout,
    ) -> bool {
        let Some(terminal) = self.active_terminal() else {
            return false;
        };
        let (display_offset, _) = terminal.scroll_state();
        let line_height = layout.range.viewport_extent / layout.viewport_rows as f32;
        if line_height <= f32::EPSILON {
            return false;
        }

        let target_display_offset = (ui_scrollbar::invert_offset_axis(
            target_offset,
            layout.range.max_offset,
        ) / line_height)
            .round()
            .clamp(0.0, layout.history_size as f32) as i32;
        let delta = target_display_offset - display_offset as i32;
        if delta == 0 {
            return false;
        }

        terminal.scroll_display(delta)
    }

    pub(in super::super) fn handle_terminal_scrollbar_mouse_down(
        &mut self,
        hit: TerminalScrollbarHit,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some((surface, layout)) = self.terminal_scrollbar_interaction_layout(window) else {
            self.stop_terminal_scrollbar_track_hold();
            return;
        };
        let range = layout.range;
        let metrics = layout.metrics;

        if hit.thumb_hit {
            self.stop_terminal_scrollbar_track_hold();
            let thumb_grab_offset = (hit.local_y - hit.thumb_top).clamp(0.0, metrics.thumb_height);
            self.start_terminal_scrollbar_drag(thumb_grab_offset, cx);
            cx.notify();
            return;
        }

        let changed = self.apply_terminal_scroll_offset(
            ui_scrollbar::offset_from_track_click(hit.local_y, range, metrics),
            layout,
        );
        if changed {
            self.terminal_scroll_accumulator_y = 0.0;
            self.sync_content_scroll_baseline();
        }
        self.start_terminal_scrollbar_track_hold(
            TerminalScrollbarTrackHoldState {
                local_y: hit.local_y,
                track_height: surface.height,
            },
            cx,
        );
        self.mark_terminal_scrollbar_activity(cx);
        cx.notify();
    }

    pub(in super::super) fn handle_terminal_scrollbar_track_hold_tick(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(state) = self.terminal_scrollbar_track_hold else {
            return false;
        };
        if self.terminal_scrollbar_drag.is_some() {
            return false;
        }

        let Some(layout) = self.terminal_scrollbar_layout_for_track(state.track_height) else {
            self.terminal_scrollbar_track_hold = None;
            return false;
        };
        let range = layout.range;
        let metrics = layout.metrics;
        let thumb_contains_point = state.local_y >= metrics.thumb_top
            && state.local_y <= metrics.thumb_top + metrics.thumb_height;
        if thumb_contains_point {
            self.terminal_scrollbar_track_hold = None;
            return false;
        }

        let changed = self.apply_terminal_scroll_offset(
            ui_scrollbar::offset_from_track_click(state.local_y, range, metrics),
            layout,
        );
        if !changed {
            self.terminal_scrollbar_track_hold = None;
            return false;
        }
        self.terminal_scroll_accumulator_y = 0.0;
        self.sync_content_scroll_baseline();
        cx.notify();
        self.mark_terminal_scrollbar_activity(cx);
        self.terminal_scrollbar_track_hold.is_some()
    }

    pub(in super::super) fn handle_terminal_scrollbar_drag(
        &mut self,
        position: gpui::Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.terminal_scrollbar_drag else {
            return;
        };
        let Some((surface, layout)) = self.terminal_scrollbar_interaction_layout(window) else {
            if self.finish_terminal_scrollbar_drag(cx) {
                cx.notify();
            }
            return;
        };
        let range = layout.range;
        let metrics = layout.metrics;

        let (_, y) = self.terminal_content_position(position);
        let local_y = (y - surface.origin_y).clamp(0.0, surface.height);
        let thumb_top = (local_y - drag.thumb_grab_offset).clamp(0.0, metrics.travel);
        let changed = self.apply_terminal_scroll_offset(
            ui_scrollbar::offset_from_thumb_top(thumb_top, range, metrics),
            layout,
        );
        if changed {
            self.terminal_scroll_accumulator_y = 0.0;
            self.sync_content_scroll_baseline();
            cx.notify();
        }
    }

    pub(in super::super) fn scroll_to_bottom(&mut self, cx: &mut Context<Self>) {
        if self
            .active_terminal()
            .is_some_and(|terminal| terminal.scroll_to_bottom())
        {
            self.content_scroll_baseline = 0;
            self.mark_terminal_scrollbar_activity(cx);
            cx.notify();
        }
    }

    pub(in super::super) fn handle_terminal_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let _ = self.close_terminal_context_menu(cx);

        if self.consume_suppressed_scroll_event(event.touch_phase, cx) {
            return;
        }

        if self.try_forward_scroll_wheel(event, cx) {
            return;
        }

        cx.stop_propagation();
        let delta_lines = self.terminal_scroll_delta_to_lines(event);
        let attempted_retarget =
            if Self::should_attempt_wheel_scroll_retarget(event.touch_phase, delta_lines) {
                self.retarget_scroll_wheel_pane(event.position, cx)
            } else {
                WheelScrollRetargetResult::Unchanged
            };
        let retarget_result =
            Self::wheel_scroll_retarget_result(event.touch_phase, delta_lines, attempted_retarget);
        match retarget_result {
            WheelScrollRetargetResult::Unchanged => {}
            WheelScrollRetargetResult::Switched => {
                // Avoid carrying fractional wheel residue across pane boundaries.
                self.terminal_scroll_accumulator_y = 0.0;
            }
            WheelScrollRetargetResult::Abort => {
                self.terminal_scroll_accumulator_y = 0.0;
                return;
            }
            WheelScrollRetargetResult::NotRetargeted => {}
        }

        if matches!(event.touch_phase, TouchPhase::Moved) {
            self.mark_terminal_scrollbar_activity(cx);
        }

        if delta_lines == 0 {
            return;
        }

        if self
            .active_terminal()
            .is_some_and(|terminal| terminal.scroll_display(delta_lines))
        {
            self.sync_content_scroll_baseline();
            cx.notify();
        } else {
            self.terminal_scroll_accumulator_y = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_scroll_lines_track_single_line_steps() {
        let mut accumulated = 0.0;
        assert_eq!(
            TerminalView::terminal_scroll_lines_from_pixels(&mut accumulated, 24.0, 24.0, 480.0),
            1
        );
    }

    #[test]
    fn terminal_scroll_lines_accumulate_fractional_pixels() {
        let mut accumulated = 0.0;
        assert_eq!(
            TerminalView::terminal_scroll_lines_from_pixels(&mut accumulated, 8.0, 24.0, 480.0),
            0
        );
        assert_eq!(
            TerminalView::terminal_scroll_lines_from_pixels(&mut accumulated, 8.0, 24.0, 480.0),
            0
        );
        assert_eq!(
            TerminalView::terminal_scroll_lines_from_pixels(&mut accumulated, 8.0, 24.0, 480.0),
            1
        );
    }

    #[test]
    fn terminal_scroll_lines_preserve_sign() {
        let mut accumulated = 0.0;
        assert_eq!(
            TerminalView::terminal_scroll_lines_from_pixels(&mut accumulated, -30.0, 24.0, 480.0),
            -1
        );
    }

    #[test]
    fn terminal_scroll_lines_wrap_accumulator_by_viewport_height() {
        let mut accumulated = 24.0 * 19.0;
        assert_eq!(
            TerminalView::terminal_scroll_lines_from_pixels(&mut accumulated, 24.0, 24.0, 480.0),
            1
        );
        assert!(accumulated.abs() < f32::EPSILON);
    }

    #[test]
    fn terminal_scroll_lines_ignore_zero_line_height() {
        let mut accumulated = 12.0;
        assert_eq!(
            TerminalView::terminal_scroll_lines_from_pixels(&mut accumulated, 24.0, 0.0, 480.0),
            0
        );
        assert_eq!(accumulated, 12.0);
    }

    #[test]
    fn wheel_scroll_pane_decision_uses_active_for_non_tmux() {
        assert_eq!(
            TerminalView::wheel_scroll_pane_decision(false, Some("%2"), Some("%1")),
            WheelScrollPaneDecision::UseActivePane
        );
    }

    #[test]
    fn wheel_scroll_pane_decision_uses_active_when_hovered_matches_active() {
        assert_eq!(
            TerminalView::wheel_scroll_pane_decision(true, Some("%7"), Some("%7")),
            WheelScrollPaneDecision::UseActivePane
        );
    }

    #[test]
    fn wheel_scroll_pane_decision_focuses_hovered_tmux_pane_when_different() {
        assert_eq!(
            TerminalView::wheel_scroll_pane_decision(true, Some("%8"), Some("%3")),
            WheelScrollPaneDecision::FocusHoveredPane
        );
    }

    #[test]
    fn wheel_scroll_retargets_to_switched() {
        let decision = TerminalView::wheel_scroll_pane_decision(true, Some("%8"), Some("%3"));
        let attempted = TerminalView::wheel_scroll_retarget_result_for_decision(
            decision,
            Some("%8"),
            true,
            Some("%8"),
        );
        let retarget = TerminalView::wheel_scroll_retarget_result(TouchPhase::Moved, 1, attempted);
        assert_eq!(retarget, WheelScrollRetargetResult::Switched);
    }

    #[test]
    fn wheel_scroll_retargets_to_abort() {
        let decision = TerminalView::wheel_scroll_pane_decision(true, Some("%8"), Some("%3"));
        let attempted = TerminalView::wheel_scroll_retarget_result_for_decision(
            decision,
            Some("%8"),
            false,
            Some("%3"),
        );
        let retarget = TerminalView::wheel_scroll_retarget_result(TouchPhase::Moved, 1, attempted);
        assert_eq!(retarget, WheelScrollRetargetResult::Abort);
    }

    #[test]
    fn wheel_scroll_retargets_to_not_retargeted() {
        let decision = TerminalView::wheel_scroll_pane_decision(true, Some("%8"), Some("%3"));
        let attempted = TerminalView::wheel_scroll_retarget_result_for_decision(
            decision,
            Some("%8"),
            true,
            Some("%8"),
        );
        let retarget = TerminalView::wheel_scroll_retarget_result(TouchPhase::Ended, 0, attempted);
        assert_eq!(retarget, WheelScrollRetargetResult::NotRetargeted);
    }

    #[test]
    fn terminal_scrollbar_surface_local_y_uses_content_space_origin() {
        let surface =
            TerminalScrollbarSurfaceGeometry::new(600.0, 100.0, 12.0, 300.0).expect("surface");
        let local_y = surface.local_y(120.0);
        assert_eq!(local_y, Some(20.0));
    }

    #[test]
    fn terminal_scrollbar_surface_local_y_rejects_points_outside_surface() {
        let surface =
            TerminalScrollbarSurfaceGeometry::new(600.0, 100.0, 12.0, 300.0).expect("surface");
        let local_y = surface.local_y(80.0);
        assert_eq!(local_y, None);
    }

    #[test]
    fn terminal_scrollbar_gutter_frame_anchors_to_surface_right_edge() {
        let surface =
            TerminalScrollbarSurfaceGeometry::new(600.0, 400.0, 407.0, 409.0).expect("surface");

        let frame = surface.gutter_frame().expect("frame");
        assert_eq!(frame.left, 995.0);
        assert_eq!(frame.width, 12.0);
    }
}
