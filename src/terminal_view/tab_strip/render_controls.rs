use super::super::*;

#[derive(Clone, Copy)]
pub(super) enum TabStripControlAction {
    NewTab,
    ToggleVerticalSidebar,
}

impl TerminalView {
    pub(super) fn perform_tab_strip_control_action(
        &mut self,
        action: TabStripControlAction,
        cx: &mut Context<Self>,
    ) {
        match action {
            TabStripControlAction::NewTab => {
                self.disarm_titlebar_window_move();
                self.add_tab(cx);
            }
            TabStripControlAction::ToggleVerticalSidebar => {
                self.disarm_titlebar_window_move();
                if let Err(error) = self.set_vertical_tabs_minimized(!self.vertical_tabs_minimized) {
                    termy_toast::error(error);
                } else {
                    cx.notify();
                }
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_tab_strip_control_button(
        &self,
        id: &'static str,
        icon: &'static str,
        action: TabStripControlAction,
        bg: gpui::Rgba,
        hover_bg: gpui::Rgba,
        border: gpui::Rgba,
        hover_border: gpui::Rgba,
        text: gpui::Rgba,
        hover_text: gpui::Rgba,
        button_size: f32,
        icon_size: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if button_size <= 0.0 {
            return div().id(id).w(px(0.0)).h(px(0.0)).into_any_element();
        }

        let corner_radius = TABBAR_NEW_TAB_BUTTON_RADIUS.min(button_size * 0.5);
        let icon_size = icon_size.min(button_size);

        div()
            .id(id)
            .w(px(button_size))
            .h(px(button_size))
            .rounded(px(corner_radius))
            .bg(bg)
            .border_1()
            .border_color(border)
            .text_color(text)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                    this.perform_tab_strip_control_action(action, cx);
                    cx.stop_propagation();
                }),
            )
            .hover(move |style| {
                style
                    .bg(hover_bg)
                    .border_color(hover_border)
                    .text_color(hover_text)
            })
            .child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(icon_size))
                    .font_weight(FontWeight::MEDIUM)
                    .mt(px(TABBAR_NEW_TAB_ICON_BASELINE_NUDGE_Y))
                    .child(icon),
            )
            .into_any_element()
    }
}
