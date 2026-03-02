use gpui::{AnyElement, Element, FontWeight, InteractiveElement, ParentElement, Styled, div, px};

pub fn render_sidebar(
    width: f32,
    sidebar_bg: gpui::Rgba,
    sidebar_border: gpui::Rgba,
    sidebar_title: gpui::Rgba,
    sidebar_text: gpui::Rgba,
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
        .px(px(12.0))
        .py(px(12.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .text_size(px(12.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(sidebar_title)
                .child("Agent Chat"),
        )
        .child(
            div()
                .text_size(px(11.0))
                .text_color(sidebar_text)
                .child("Session-aware AI agent with tool execution."),
        )
        .child(content)
        .child(
            div()
                .w_full()
                .h(px(136.0))
                .px(px(8.0))
                .py(px(6.0))
                .relative()
                .bg(sidebar_bg)
                .border_1()
                .border_color(sidebar_border)
                .child(composer),
        )
        .into_any()
}
