use super::super::*;
use crate::terminal_view::tab_strip::state::TabStripOrientation;

impl TerminalView {
    pub(crate) fn tab_strip_scroll_delta_from_pixels(delta_x: f32, delta_y: f32) -> f32 {
        if delta_x.abs() <= f32::EPSILON && delta_y.abs() <= f32::EPSILON {
            return 0.0;
        }

        let dominant_delta = if delta_x.abs() >= delta_y.abs() {
            delta_x
        } else {
            delta_y
        };

        // ScrollHandle offset-space is [-max, 0], while input deltas are content-space.
        // Invert once here so all callers can pass the resulting offset delta directly.
        -dominant_delta
    }

    pub(crate) fn handle_tab_strip_action_rail_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let pixel_delta = event
            .delta
            .pixel_delta(px(TAB_STRIP_WHEEL_DELTA_LINE_REFERENCE_PX));
        let delta_x: f32 = pixel_delta.x.into();
        let delta_y: f32 = pixel_delta.y.into();
        let scroll_delta = Self::tab_strip_scroll_delta_from_pixels(delta_x, delta_y);
        if self.scroll_tab_strip_by(scroll_delta) {
            cx.notify();
        }
        cx.stop_propagation();
    }

    pub(crate) fn on_tab_mouse_down(
        &mut self,
        orientation: TabStripOrientation,
        tab_index: usize,
        click_count: usize,
        cx: &mut Context<Self>,
    ) {
        self.switch_tab(tab_index, cx);
        self.begin_tab_drag(tab_index, orientation);
        if Self::should_begin_tab_rename(
            orientation,
            click_count,
            self.vertical_tabs_minimized,
        ) {
            self.begin_rename_tab(tab_index, cx);
        }
    }

    fn should_begin_tab_rename(
        orientation: TabStripOrientation,
        click_count: usize,
        vertical_tabs_minimized: bool,
    ) -> bool {
        click_count == 2
            && !(orientation == TabStripOrientation::Vertical && vertical_tabs_minimized)
    }

    pub(crate) fn tab_strip_drag_preview_from_window_position(
        &self,
        orientation: TabStripOrientation,
        window: &Window,
        position: gpui::Point<Pixels>,
    ) -> (f32, f32) {
        match orientation {
            TabStripOrientation::Horizontal => {
                self.tab_strip_pointer_x_from_window_x(window, position.x)
            }
            TabStripOrientation::Vertical => {
                let layout = self.vertical_tab_strip_layout_snapshot();
                let pointer_y = layout
                    .list_pointer_y_from_window_y(position.y.into(), self.chrome_height());
                (pointer_y, layout.list_height)
            }
        }
    }

    pub(crate) fn on_tab_close_mouse_move(&mut self, tab_index: usize, cx: &mut Context<Self>) {
        let mut hover_changed = false;
        if self.tab_strip.hovered_tab != Some(tab_index) {
            self.tab_strip.hovered_tab = Some(tab_index);
            hover_changed = true;
        }
        if self.tab_strip.hovered_tab_close != Some(tab_index) {
            self.tab_strip.hovered_tab_close = Some(tab_index);
            hover_changed = true;
        }
        if hover_changed {
            cx.notify();
        }
    }

    pub(crate) fn on_tab_mouse_move(
        &mut self,
        orientation: TabStripOrientation,
        tab_index: usize,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let mut hovered_changed = if self.tab_strip.hovered_tab != Some(tab_index) {
            self.tab_strip.hovered_tab = Some(tab_index);
            true
        } else {
            false
        };
        if self.tab_strip.hovered_tab_close.take().is_some() {
            hovered_changed = true;
        }

        let drag_changed = if event.dragging() {
            let (pointer_primary_axis, viewport_extent) =
                self.tab_strip_drag_preview_from_window_position(
                    orientation,
                    window,
                    event.position,
                );
            self.update_tab_drag_preview(
                orientation,
                pointer_primary_axis,
                viewport_extent,
                cx,
            )
        } else {
            false
        };
        if hovered_changed && !drag_changed {
            cx.notify();
        }
    }

    pub(crate) fn on_tabs_content_mouse_move(
        &mut self,
        orientation: TabStripOrientation,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let hovered_changed = self.clear_tab_hover_state();
        let drag_changed = if event.dragging() {
            let (pointer_primary_axis, viewport_extent) =
                self.tab_strip_drag_preview_from_window_position(
                    orientation,
                    window,
                    event.position,
                );
            self.update_tab_drag_preview(
                orientation,
                pointer_primary_axis,
                viewport_extent,
                cx,
            )
        } else {
            if self.tab_strip.drag.is_some() {
                self.commit_tab_drag(cx);
                return;
            }
            false
        };
        if hovered_changed && !drag_changed {
            cx.notify();
        }
    }

    pub(crate) fn on_action_rail_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let hovered_changed = self.clear_tab_hover_state();
        if !event.dragging() {
            if hovered_changed {
                cx.notify();
            }
            return;
        }

        let (pointer_primary_axis, viewport_extent) = self
            .tab_strip_drag_preview_from_window_position(
                TabStripOrientation::Horizontal,
                window,
                event.position,
            );
        if !self.update_tab_drag_preview(
            TabStripOrientation::Horizontal,
            pointer_primary_axis,
            viewport_extent,
            cx,
        ) && hovered_changed
        {
            cx.notify();
        }
    }

    fn arm_titlebar_window_move(&mut self) {
        self.tab_strip.titlebar_move_armed = true;
    }

    pub(crate) fn disarm_titlebar_window_move(&mut self) {
        self.tab_strip.titlebar_move_armed = false;
    }

    pub(crate) fn titlebar_move_armed_after_mouse_down(
        interactive_hit: bool,
        click_count: usize,
    ) -> bool {
        !interactive_hit && click_count != 2
    }

    pub(crate) fn titlebar_move_armed_after_mouse_up() -> bool {
        false
    }

    pub(crate) fn should_window_drag_surface_double_click(
        interactive_hit: bool,
        click_count: usize,
    ) -> bool {
        !interactive_hit && click_count == 2
    }

    fn tab_strip_interactive_hit_test(
        &self,
        orientation: TabStripOrientation,
        x: f32,
        y: f32,
        window: &Window,
    ) -> bool {
        match orientation {
            TabStripOrientation::Horizontal => {
                self.unified_titlebar_tab_interactive_hit_test(x, y, window)
            }
            TabStripOrientation::Vertical => self.vertical_tab_strip_interactive_hit_test(x, y),
        }
    }

    fn handle_window_drag_surface_mouse_down(
        &mut self,
        orientation: TabStripOrientation,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }

        let x: f32 = event.position.x.into();
        let y: f32 = event.position.y.into();
        let interactive_hit = self.tab_strip_interactive_hit_test(orientation, x, y, window);
        let next_move_armed =
            Self::titlebar_move_armed_after_mouse_down(interactive_hit, event.click_count);
        if !next_move_armed {
            self.disarm_titlebar_window_move();
        }
        if Self::should_window_drag_surface_double_click(interactive_hit, event.click_count) {
            #[cfg(target_os = "macos")]
            window.titlebar_double_click();
            #[cfg(not(target_os = "macos"))]
            window.zoom_window();
            cx.stop_propagation();
            return;
        }

        if next_move_armed {
            self.arm_titlebar_window_move();
            cx.stop_propagation();
        }
    }

    pub(crate) fn handle_unified_titlebar_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_window_drag_surface_mouse_down(
            TabStripOrientation::Horizontal,
            event,
            window,
            cx,
        );
    }

    pub(crate) fn handle_vertical_tab_strip_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_window_drag_surface_mouse_down(
            TabStripOrientation::Vertical,
            event,
            window,
            cx,
        );
    }

    pub(crate) fn handle_unified_titlebar_mouse_up(
        &mut self,
        event: &MouseUpEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Left {
            return;
        }

        self.tab_strip.titlebar_move_armed = Self::titlebar_move_armed_after_mouse_up();
        cx.stop_propagation();
    }

    pub(crate) fn maybe_start_titlebar_window_move(
        &mut self,
        dragging: bool,
        window: &mut Window,
    ) -> bool {
        if !Self::should_start_titlebar_window_move(
            self.tab_strip.titlebar_move_armed,
            dragging,
            self.tab_strip.drag.is_some(),
        ) {
            return false;
        }

        self.disarm_titlebar_window_move();
        window.start_window_move();
        true
    }

    pub(crate) fn should_start_titlebar_window_move(
        titlebar_move_armed: bool,
        dragging: bool,
        tab_drag_active: bool,
    ) -> bool {
        titlebar_move_armed && dragging && !tab_drag_active
    }

    fn handle_window_drag_surface_mouse_move(
        &mut self,
        orientation: TabStripOrientation,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.maybe_start_titlebar_window_move(event.dragging(), window) {
            cx.stop_propagation();
            return;
        }

        let mut changed = false;
        if self.clear_tab_hover_state() {
            changed = true;
        }
        if event.dragging() {
            let (pointer_primary_axis, viewport_extent) =
                self.tab_strip_drag_preview_from_window_position(orientation, window, event.position);
            if !self.update_tab_drag_preview(orientation, pointer_primary_axis, viewport_extent, cx)
                && changed
            {
                cx.notify();
            }
            return;
        }
        if self.tab_strip.drag.is_some() {
            self.commit_tab_drag(cx);
        }
        if changed {
            cx.notify();
        }
    }

    pub(crate) fn handle_titlebar_tab_strip_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_window_drag_surface_mouse_move(
            TabStripOrientation::Horizontal,
            event,
            window,
            cx,
        );
    }

    pub(crate) fn handle_vertical_tab_strip_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.handle_window_drag_surface_mouse_move(
            TabStripOrientation::Vertical,
            event,
            window,
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_view::tab_strip::layout::VerticalTabStripLayoutInput;

    fn vertical_layout(strip_width: f32, compact: bool) -> crate::terminal_view::tab_strip::layout::VerticalTabStripLayoutSnapshot {
        TerminalView::vertical_tab_strip_layout_for_input(VerticalTabStripLayoutInput {
            strip_width,
            compact,
            header_height: TABBAR_HEIGHT,
            list_height: 180.0,
            tab_heights: vec![TAB_ITEM_HEIGHT],
        })
    }

    #[test]
    fn titlebar_window_move_requires_armed_and_dragging() {
        assert!(!TerminalView::should_start_titlebar_window_move(
            false, true, false
        ));
        assert!(!TerminalView::should_start_titlebar_window_move(
            true, false, false
        ));
        assert!(TerminalView::should_start_titlebar_window_move(
            true, true, false
        ));
    }

    #[test]
    fn titlebar_window_move_does_not_start_during_tab_drag() {
        assert!(!TerminalView::should_start_titlebar_window_move(
            true, true, true
        ));
    }

    #[test]
    fn titlebar_move_arm_state_transitions_on_mouse_down() {
        assert!(!TerminalView::titlebar_move_armed_after_mouse_down(true, 1));
        assert!(!TerminalView::titlebar_move_armed_after_mouse_down(
            false, 2
        ));
        assert!(TerminalView::titlebar_move_armed_after_mouse_down(false, 1));
    }

    #[test]
    fn titlebar_move_arm_state_transitions_on_mouse_up() {
        assert!(!TerminalView::titlebar_move_armed_after_mouse_up());
    }

    #[test]
    fn window_drag_surface_double_click_requires_noninteractive_hit() {
        assert!(TerminalView::should_window_drag_surface_double_click(false, 2));
        assert!(!TerminalView::should_window_drag_surface_double_click(true, 2));
        assert!(!TerminalView::should_window_drag_surface_double_click(false, 1));
    }

    #[test]
    fn compact_vertical_double_click_does_not_begin_rename() {
        assert!(!TerminalView::should_begin_tab_rename(
            TabStripOrientation::Vertical,
            2,
            true,
        ));
        assert!(TerminalView::should_begin_tab_rename(
            TabStripOrientation::Vertical,
            2,
            false,
        ));
        assert!(TerminalView::should_begin_tab_rename(
            TabStripOrientation::Horizontal,
            2,
            true,
        ));
    }

    #[test]
    fn vertical_noninteractive_chrome_hit_arms_window_move() {
        let strip_width = 220.0;
        let compact = false;
        let layout = vertical_layout(strip_width, compact);
        let interactive = TerminalView::vertical_tab_strip_interactive_hit_test_for_layout(
            24.0,
            12.0,
            &layout,
            0.0,
        );

        assert!(!interactive);
        assert!(TerminalView::titlebar_move_armed_after_mouse_down(interactive, 1));
    }

    #[test]
    fn vertical_interactive_hit_does_not_arm_window_move() {
        let strip_width = 220.0;
        let compact = false;
        let layout = vertical_layout(strip_width, compact);
        let interactive = TerminalView::vertical_tab_strip_interactive_hit_test_for_layout(
            24.0,
            layout.list_top + 12.0,
            &layout,
            0.0,
        );

        assert!(interactive);
        assert!(!TerminalView::titlebar_move_armed_after_mouse_down(interactive, 1));
    }

    #[test]
    fn vertical_noninteractive_double_click_uses_titlebar_double_click_branch() {
        let strip_width = 220.0;
        let compact = false;
        let layout = vertical_layout(strip_width, compact);
        let interactive = TerminalView::vertical_tab_strip_interactive_hit_test_for_layout(
            24.0,
            12.0,
            &layout,
            0.0,
        );

        assert!(!interactive);
        assert!(!TerminalView::titlebar_move_armed_after_mouse_down(interactive, 2));
        assert!(TerminalView::should_window_drag_surface_double_click(
            interactive, 2
        ));
    }

    #[test]
    fn tab_strip_scroll_delta_prefers_horizontal_axis() {
        assert_eq!(
            TerminalView::tab_strip_scroll_delta_from_pixels(48.0, 12.0),
            -48.0
        );
    }

    #[test]
    fn tab_strip_scroll_delta_prefers_vertical_axis_when_dominant() {
        assert_eq!(
            TerminalView::tab_strip_scroll_delta_from_pixels(12.0, 48.0),
            -48.0
        );
    }

    #[test]
    fn tab_strip_scroll_delta_preserves_small_non_zero_trackpad_deltas() {
        assert_eq!(
            TerminalView::tab_strip_scroll_delta_from_pixels(0.2, -0.4),
            0.4
        );
    }

    #[test]
    fn tab_strip_scroll_delta_returns_zero_only_for_zero_input() {
        assert_eq!(
            TerminalView::tab_strip_scroll_delta_from_pixels(0.0, 0.0),
            0.0
        );
    }

    #[test]
    fn tab_strip_scroll_delta_falls_back_to_vertical_axis_for_zero_horizontal() {
        assert_eq!(
            TerminalView::tab_strip_scroll_delta_from_pixels(0.0, -30.0),
            30.0
        );
    }

    #[test]
    fn tab_strip_scroll_delta_falls_back_to_horizontal_axis_for_zero_vertical() {
        assert_eq!(
            TerminalView::tab_strip_scroll_delta_from_pixels(12.0, 0.0),
            -12.0
        );
    }
}
