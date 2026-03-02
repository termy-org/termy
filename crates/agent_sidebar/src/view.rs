use gpui::*;

pub fn render_sidebar(
    width: f32,
    header: AnyElement,
    sidebar_bg: gpui::Rgba,
    sidebar_border: gpui::Rgba,
    content: AnyElement,
    composer: AnyElement,
) -> AnyElement {
    div()
        .id("chat-sidebar")
        .w(px(width))
        .h_full()
        .bg(sidebar_bg)
        .border_l_1()
        .border_color(sidebar_border)
        .flex()
        .flex_col()
        // Stop mouse events from propagating to the terminal view
        .on_mouse_down(MouseButton::Left, |_, _, cx| {
            cx.stop_propagation();
        })
        .child(
            // Header
            div()
                .w_full()
                .px(px(12.0))
                .py(px(10.0))
                .border_b_1()
                .border_color(sidebar_border)
                .child(header),
        )
        .child(
            // Content (messages) - flex_1 + min_h(0) allows this to shrink and let child scroll
            div()
                .id("sidebar-content-wrapper")
                .h_full()
                .flex_1()
                .min_h(px(0.0))
                .child(content),
        )
        .child(
            // Composer
            div().w_full().p(px(12.0)).child(composer),
        )
        .into_any()
}
