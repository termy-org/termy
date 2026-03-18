use super::super::*;
use super::render_palette::TabStripPalette;
use super::state::{TabDropMarkerSide, TabStripOrientation};

pub(super) struct TabItemRenderInput {
    pub(super) orientation: TabStripOrientation,
    pub(super) index: usize,
    pub(super) tab_primary_extent: f32,
    pub(super) tab_cross_extent: f32,
    pub(super) tab_strokes: TabItemStrokeRects,
    pub(super) label: String,
    pub(super) switch_hint_label: Option<String>,
    pub(super) is_active: bool,
    pub(super) is_hovered: bool,
    pub(super) is_renaming: bool,
    pub(super) show_tab_close: bool,
    pub(super) close_slot_width: f32,
    pub(super) text_padding_x: f32,
    pub(super) label_centered: bool,
    pub(super) trailing_divider_cover: Option<gpui::Rgba>,
    pub(super) drop_marker_side: Option<TabDropMarkerSide>,
    pub(super) open_anim_progress: Option<f32>,
}

#[derive(Clone, Copy)]
pub(super) struct TabItemStrokeRects {
    pub(super) top: Option<super::chrome::StrokeRect>,
    pub(super) bottom: Option<super::chrome::StrokeRect>,
    pub(super) left: Option<super::chrome::StrokeRect>,
    pub(super) right: Option<super::chrome::StrokeRect>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn close_chip_fits_within_close_slot() {
        assert!(TAB_CLOSE_CHIP_WIDTH < TAB_CLOSE_SLOT_WIDTH);
        assert!(TAB_CLOSE_CHIP_HEIGHT < TAB_CLOSE_HITBOX);
    }
}

impl TerminalView {
    fn render_tab_accessory(
        &self,
        input: &TabItemRenderInput,
        palette: &TabStripPalette,
        close_text_color: gpui::Rgba,
        hover_tab_index: usize,
        close_tab_index: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(label) = input.switch_hint_label.as_ref() {
            let mut accessory = div()
                .flex_none()
                .w(px(input.close_slot_width))
                .h(px(TAB_CLOSE_HITBOX))
                .flex()
                .items_center()
                .justify_center()
                .bg(palette.switch_hint_bg)
                .text_color(palette.switch_hint_text)
                .text_size(px(TAB_SWITCH_HINT_TEXT_SIZE))
                .font_weight(FontWeight::MEDIUM);

            if input.orientation == TabStripOrientation::Horizontal {
                accessory = accessory.border_l_1().border_color(palette.switch_hint_border);
            } else {
                accessory = accessory.border_1().border_color(palette.switch_hint_border);
            }

            return accessory.child(label.clone()).into_any_element();
        }

        div()
            .flex_none()
            .w(px(input.close_slot_width))
            .h(px(TAB_CLOSE_HITBOX))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(TAB_CLOSE_CHIP_WIDTH.min(input.close_slot_width)))
                    .h(px(TAB_CLOSE_CHIP_HEIGHT.min(TAB_CLOSE_HITBOX)))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(TAB_CLOSE_CHIP_RADIUS))
                    .bg(palette.close_button_bg)
                    .border_1()
                    .border_color(palette.close_button_border)
                    .text_color(close_text_color)
                    .text_size(px(12.0))
                    .font_weight(FontWeight::MEDIUM)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            let is_active = close_tab_index == this.active_tab;
                            if Self::tab_shows_close(
                                this.tab_close_visibility,
                                is_active,
                                this.tab_strip.hovered_tab,
                                this.tab_strip.hovered_tab_close,
                                close_tab_index,
                            ) {
                                this.request_tab_close_by_index(close_tab_index, window, cx);
                                cx.stop_propagation();
                            }
                        }),
                    )
                    .on_mouse_move(
                        cx.listener(move |this, _event: &MouseMoveEvent, _window, cx| {
                            this.on_tab_close_mouse_move(hover_tab_index, cx);
                            cx.stop_propagation();
                        }),
                    )
                    .hover(move |style| {
                        style
                            .bg(palette.close_button_hover_bg)
                            .border_color(palette.close_button_hover_border)
                            .text_color(palette.close_button_hover_text)
                    })
                    .cursor_pointer()
                    .child(div().mt(px(-1.0)).child("×")),
            )
            .into_any_element()
    }

    pub(super) fn render_tab_item(
        &mut self,
        input: TabItemRenderInput,
        font_family: &SharedString,
        colors: &TerminalColors,
        palette: &TabStripPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let orientation = input.orientation;
        let switch_tab_index = input.index;
        let hover_tab_index = input.index;
        let close_tab_index = input.index;

        let anim = input.open_anim_progress.unwrap_or(1.0);

        let mut rename_text_color = if input.is_active {
            palette.active_tab_text
        } else {
            palette.inactive_tab_text
        };
        rename_text_color.a *= anim;
        let mut rename_selection_color = colors.cursor;
        rename_selection_color.a = if input.is_active { 0.34 } else { 0.24 };
        rename_selection_color.a *= anim;

        let mut tab_bg = if input.is_active {
            palette.active_tab_bg
        } else if input.is_hovered {
            palette.hovered_tab_bg
        } else {
            palette.inactive_tab_bg
        };
        tab_bg.a *= anim;

        let mut close_text_color = if input.is_active {
            palette.active_tab_text
        } else {
            palette.inactive_tab_text
        };
        close_text_color.a *= anim;
        if !input.show_tab_close {
            close_text_color.a = 0.0;
        }

        let accessory_slot = self.render_tab_accessory(
            &input,
            palette,
            close_text_color,
            hover_tab_index,
            close_tab_index,
            cx,
        );

        let justify_label_center = input.label_centered;
        let trailing_divider_cover = input.trailing_divider_cover;
        let mut tab_shell = div()
            .flex_none()
            .relative()
            .overflow_hidden()
            .bg(tab_bg)
            .w(px(input.tab_primary_extent))
            .h(px(input.tab_cross_extent))
            .px(px(input.text_padding_x))
            .flex()
            .items_center()
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                    this.on_tab_mouse_down(orientation, switch_tab_index, event.click_count, cx);
                    cx.stop_propagation();
                }),
            )
            .on_mouse_move(
                cx.listener(move |this, event: &MouseMoveEvent, window, cx| {
                    this.on_tab_mouse_move(orientation, hover_tab_index, event, window, cx);
                    cx.stop_propagation();
                }),
            );

        for stroke in [
            input.tab_strokes.top,
            input.tab_strokes.bottom,
            input.tab_strokes.left,
            input.tab_strokes.right,
        ]
        .into_iter()
        .flatten()
        {
            tab_shell = tab_shell.child(Self::render_tab_stroke(stroke, palette.tab_stroke_color));
        }

        let drop_marker = input.drop_marker_side.map(|side| match orientation {
            TabStripOrientation::Horizontal => {
                let marker_x = match side {
                    TabDropMarkerSide::Leading => 0.0,
                    TabDropMarkerSide::Trailing => input.tab_primary_extent - TAB_DROP_MARKER_WIDTH,
                }
                .max(0.0);
                let marker_height =
                    (input.tab_cross_extent - (TAB_DROP_MARKER_INSET_Y * 2.0)).max(0.0);

                div()
                    .absolute()
                    .left(px(marker_x))
                    .top(px(TAB_DROP_MARKER_INSET_Y))
                    .w(px(TAB_DROP_MARKER_WIDTH))
                    .h(px(marker_height))
                    .bg(palette.tab_drop_marker_color)
            }
            TabStripOrientation::Vertical => {
                let marker_y = match side {
                    TabDropMarkerSide::Leading => 0.0,
                    TabDropMarkerSide::Trailing => input.tab_cross_extent - TAB_DROP_MARKER_WIDTH,
                }
                .max(0.0);

                div()
                    .absolute()
                    .left(px(TAB_DROP_MARKER_INSET_Y))
                    .top(px(marker_y))
                    .w(px((input.tab_primary_extent - (TAB_DROP_MARKER_INSET_Y * 2.0)).max(0.0)))
                    .h(px(TAB_DROP_MARKER_WIDTH))
                    .bg(palette.tab_drop_marker_color)
            }
        });

        tab_shell
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .h_full()
                    .relative()
                    .child(if input.is_renaming {
                        self.render_inline_input_layer(
                            Font {
                                family: font_family.clone(),
                                weight: FontWeight::NORMAL,
                                ..Default::default()
                            },
                            px(12.0),
                            rename_text_color.into(),
                            rename_selection_color.into(),
                            InlineInputAlignment::Left,
                            cx,
                        )
                    } else {
                        let mut title_text = div()
                            .size_full()
                            .flex()
                            .items_center()
                            .overflow_x_hidden()
                            .whitespace_nowrap()
                            .font_family(font_family.clone())
                            .text_color(rename_text_color)
                            .text_size(px(12.0))
                            .text_ellipsis();
                        if justify_label_center {
                            title_text = title_text.justify_center();
                        }
                        title_text.child(input.label).into_any_element()
                    }),
            )
            .children((input.close_slot_width > 0.0).then_some(accessory_slot))
            .children(trailing_divider_cover.map(|cover_color| {
                div()
                    .absolute()
                    .right_0()
                    .top_0()
                    .bottom_0()
                    .w(px(TAB_STROKE_THICKNESS))
                    .bg(cover_color)
            }))
            .children(drop_marker)
            .into_any_element()
    }
}
