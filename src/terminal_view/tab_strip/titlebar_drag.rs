use super::super::*;
use super::state::{TabStripOrientation, TabStripTitlebarState};

impl TerminalView {
    pub(crate) fn disarm_titlebar_window_move(&mut self) {
        self.tab_strip.titlebar.disarm();
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
        let outcome = self
            .tab_strip
            .titlebar
            .on_mouse_down(interactive_hit, event.click_count);
        if outcome.trigger_window_action {
            #[cfg(target_os = "macos")]
            window.titlebar_double_click();
            #[cfg(not(target_os = "macos"))]
            window.zoom_window();
            cx.stop_propagation();
            return;
        }

        if outcome.arm_move {
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

        self.tab_strip.titlebar.on_mouse_up();
        cx.stop_propagation();
    }

    pub(crate) fn maybe_start_titlebar_window_move(
        &mut self,
        dragging: bool,
        window: &mut Window,
    ) -> bool {
        if !self
            .tab_strip
            .titlebar
            .take_window_move_request(dragging, self.tab_strip.drag.is_some())
        {
            return false;
        }

        window.start_window_move();
        true
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
            let preview = self.tab_strip_drag_preview(orientation, window, event.position);
            if !self.update_tab_drag_preview(
                orientation,
                preview.pointer_primary_axis,
                preview.viewport_extent,
                cx,
            ) && changed
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

    fn vertical_layout(
        strip_width: f32,
        compact: bool,
    ) -> crate::terminal_view::tab_strip::layout::VerticalTabStripLayoutSnapshot {
        TerminalView::vertical_tab_strip_layout_for_input(VerticalTabStripLayoutInput {
            strip_width,
            compact,
            header_height: TABBAR_HEIGHT,
            list_height: 180.0,
            tab_heights: vec![TAB_ITEM_HEIGHT],
        })
    }

    fn armed_titlebar_state() -> TabStripTitlebarState {
        let mut state = TabStripTitlebarState::default();
        state.arm();
        state
    }

    #[test]
    fn titlebar_window_move_requires_armed_and_dragging() {
        assert!(!TabStripTitlebarState::default().should_start_window_move(true, false));
        assert!(!armed_titlebar_state().should_start_window_move(false, false));
        assert!(armed_titlebar_state().should_start_window_move(true, false));
    }

    #[test]
    fn titlebar_window_move_does_not_start_during_tab_drag() {
        assert!(!armed_titlebar_state().should_start_window_move(true, true));
    }

    #[test]
    fn vertical_noninteractive_chrome_hit_arms_window_move() {
        let layout = vertical_layout(220.0, false);
        let interactive = TerminalView::vertical_tab_strip_interactive_hit_test_for_layout(
            24.0,
            12.0,
            &layout,
            0.0,
        );
        let mut state = TabStripTitlebarState::default();

        assert!(!interactive);
        assert!(state.on_mouse_down(interactive, 1).arm_move);
    }

    #[test]
    fn vertical_interactive_hit_does_not_arm_window_move() {
        let layout = vertical_layout(220.0, false);
        let interactive = TerminalView::vertical_tab_strip_interactive_hit_test_for_layout(
            24.0,
            layout.list_top + 12.0,
            &layout,
            0.0,
        );
        let mut state = TabStripTitlebarState::default();

        assert!(interactive);
        assert!(!state.on_mouse_down(interactive, 1).arm_move);
    }

    #[test]
    fn vertical_noninteractive_double_click_uses_titlebar_double_click_branch() {
        let layout = vertical_layout(220.0, false);
        let interactive = TerminalView::vertical_tab_strip_interactive_hit_test_for_layout(
            24.0,
            12.0,
            &layout,
            0.0,
        );
        let mut state = TabStripTitlebarState::default();

        assert!(!interactive);
        let outcome = state.on_mouse_down(interactive, 2);
        assert!(!outcome.arm_move);
        assert!(outcome.trigger_window_action);
    }
}
