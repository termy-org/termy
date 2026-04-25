use super::super::*;
use super::state::TabStripOrientation;

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
        self.disarm_titlebar_window_move();
        self.switch_tab(tab_index, cx);
        self.begin_tab_drag(tab_index, orientation);
        if Self::should_begin_tab_rename(orientation, click_count, self.vertical_tabs_minimized) {
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
            self.disarm_titlebar_window_move();
            let preview = self.tab_strip_drag_preview(orientation, window, event.position);
            self.update_tab_drag_preview(
                orientation,
                preview.pointer_primary_axis,
                preview.viewport_extent,
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
            let preview = self.tab_strip_drag_preview(orientation, window, event.position);
            self.update_tab_drag_preview(
                orientation,
                preview.pointer_primary_axis,
                preview.viewport_extent,
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

        let preview =
            self.tab_strip_drag_preview(TabStripOrientation::Horizontal, window, event.position);
        if !self.update_tab_drag_preview(
            TabStripOrientation::Horizontal,
            preview.pointer_primary_axis,
            preview.viewport_extent,
            cx,
        ) && hovered_changed
        {
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
