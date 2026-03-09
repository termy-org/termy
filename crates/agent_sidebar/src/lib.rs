use gpui::{
    AnyElement, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Styled, div, px,
};

pub const DEFAULT_WIDTH: f32 = 320.0;

pub fn render_sidebar(
    width: f32,
    background: Hsla,
    border: Hsla,
    text_primary: Hsla,
    text_muted: Hsla,
    input_content: AnyElement,
) -> AnyElement {
    let panel_bg = background;

    let mut top_bar_bg = background;
    top_bar_bg.a = (top_bar_bg.a * 0.92).clamp(0.0, 1.0);

    let mut card_bg = background;
    card_bg.a = (card_bg.a * 0.82).clamp(0.0, 1.0);

    let mut soft_fill = border;
    soft_fill.a = 0.18;

    let mut soft_fill_hover = border;
    soft_fill_hover.a = 0.28;

    let mut subtle_border = border;
    subtle_border.a = 0.72;

    let mut faint_border = border;
    faint_border.a = 0.38;

    let mut accent_text = text_primary;
    accent_text.a = 0.98;

    let mut subdued_text = text_muted;
    subdued_text.a = 0.92;

    let mut ghost_text = text_muted;
    ghost_text.a = 0.74;

    div()
        .id("agent-sidebar")
        .w(px(width))
        .h_full()
        .flex_none()
        .bg(panel_bg)
        .border_l_1()
        .border_color(subtle_border)
        .child(
            div()
                .w_full()
                .h_full()
                .flex()
                .flex_col()
                .child(render_header(
                    top_bar_bg,
                    subtle_border,
                    soft_fill,
                    soft_fill_hover,
                    accent_text,
                    subdued_text,
                ))
                // Spacer pushes composer to bottom
                .child(div().flex_1())
                .child(render_composer(
                    top_bar_bg,
                    subtle_border,
                    card_bg,
                    faint_border,
                    soft_fill,
                    soft_fill_hover,
                    accent_text,
                    subdued_text,
                    ghost_text,
                    input_content,
                )),
        )
        .into_any_element()
}

fn render_header(
    bg: Hsla,
    border: Hsla,
    button_bg: Hsla,
    button_hover: Hsla,
    text_primary: Hsla,
    text_muted: Hsla,
) -> AnyElement {
    div()
        .w_full()
        .border_b_1()
        .border_color(border)
        .bg(bg)
        .px_3()
        .py_2()
        .child(
            div()
                .w_full()
                .flex()
                .items_center()
                .justify_between()
                .gap_3()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_3()
                        .child(
                            div()
                                .size(px(22.0))
                                .rounded(px(6.0))
                                .border_1()
                                .border_color(border)
                                .bg(button_bg)
                                .child(
                                    div()
                                        .w_full()
                                        .h_full()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .text_xs()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(text_primary)
                                        .child("A"),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(1.0))
                                .child(
                                    div()
                                        .text_sm()
                                        .font_weight(FontWeight::BOLD)
                                        .text_color(text_primary)
                                        .child("Agent"),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(text_muted)
                                        .child("Experimental workspace copilot"),
                                ),
                        ),
                )
                .child(div().flex().items_center().gap_2().children(
                    ["+", "-", "..."].into_iter().map(|label| {
                        div()
                            .size(px(22.0))
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(border)
                            .bg(button_bg)
                            .hover(move |style| style.bg(button_hover))
                            .child(
                                div()
                                    .w_full()
                                    .h_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(text_muted)
                                    .child(label),
                            )
                    }),
                )),
        )
        .into_any_element()
}

fn render_composer(
    bg: Hsla,
    border: Hsla,
    input_bg: Hsla,
    input_border: Hsla,
    soft_fill: Hsla,
    soft_fill_hover: Hsla,
    text_primary: Hsla,
    text_muted: Hsla,
    _ghost_text: Hsla,
    input_content: AnyElement,
) -> AnyElement {
    div()
        .w_full()
        .border_t_1()
        .border_color(border)
        .bg(bg)
        .px_3()
        .py_3()
        .child(
            div()
                .flex()
                .flex_col()
                .gap_3()
                .child(
                    div()
                        .w_full()
                        .rounded(px(10.0))
                        .border_1()
                        .border_color(input_border)
                        .bg(input_bg)
                        .px_3()
                        .py_3()
                        .min_h(px(24.0))
                        .child(input_content),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_3()
                        .child(div().flex().items_center().gap_2().children(
                            ["+", "◎"].into_iter().map(|label| {
                                div()
                                    .size(px(22.0))
                                    .rounded(px(6.0))
                                    .border_1()
                                    .border_color(input_border)
                                    .bg(soft_fill)
                                    .hover(move |style| style.bg(soft_fill_hover))
                                    .child(
                                        div()
                                            .w_full()
                                            .h_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .text_xs()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(text_muted)
                                            .child(label),
                                    )
                            }),
                        ))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_3()
                                .child(
                                    div().text_xs().text_color(text_muted).child("Read Only"),
                                )
                                .child(div().text_xs().text_color(text_muted).child("gpt-5.4"))
                                .child(div().text_xs().text_color(text_muted).child("Medium"))
                                .child(
                                    div()
                                        .size(px(26.0))
                                        .rounded(px(7.0))
                                        .border_1()
                                        .border_color(input_border)
                                        .bg(soft_fill)
                                        .hover(move |style| style.bg(soft_fill_hover))
                                        .child(
                                            div()
                                                .w_full()
                                                .h_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .text_sm()
                                                .font_weight(FontWeight::SEMIBOLD)
                                                .text_color(text_primary)
                                                .child(">"),
                                        ),
                                ),
                        ),
                ),
        )
        .into_any_element()
}
