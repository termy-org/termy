use super::super::*;
use super::chrome;
use super::hints::TabSwitchHintState;
use super::layout::{VerticalBottomShelfLayout, VerticalNewTabShelfLayout};
use super::render_controls::TabStripControlAction;
use super::render_palette::TabStripPalette;
use super::render_tab_item::{TabItemRenderInput, TabItemStrokeRects};
use super::state::TabStripOrientation;

#[derive(Clone, Copy, Debug, PartialEq)]
struct VerticalTitlebarChromeLayout {
    block_width: f32,
    bottom_seam: chrome::StrokeRect,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_view::tab_strip::collapsed_vertical_tab_strip_width;

    #[test]
    fn compact_vertical_tab_label_prefers_shortcuts_then_initial() {
        assert_eq!(
            TerminalView::compact_vertical_tab_label(0, "~/projects/termy"),
            "1"
        );
        assert_eq!(
            TerminalView::compact_vertical_tab_label(8, "~/projects/termy"),
            "9"
        );
        assert_eq!(
            TerminalView::compact_vertical_tab_label(9, "~/projects/termy"),
            "~"
        );
        assert_eq!(TerminalView::compact_vertical_tab_label(10, ""), "•");
    }

    #[test]
    fn vertical_titlebar_sidebar_block_layout_uses_sidebar_width() {
        let layout = TerminalView::vertical_titlebar_sidebar_block_layout(240.0, TABBAR_HEIGHT)
            .expect("positive size should produce layout");
        assert_eq!(layout.block_width, 240.0);
        assert_eq!(layout.bottom_seam.w, 240.0);
        let divider = TerminalView::vertical_titlebar_right_divider_stroke(
            layout.block_width,
            TABBAR_HEIGHT,
            true,
            64.0,
        )
        .expect("visible sidebar should produce a titlebar divider");
        assert_eq!(divider.x, 239.0);
    }

    #[test]
    fn vertical_titlebar_sidebar_block_layout_hides_without_positive_extent() {
        assert_eq!(
            TerminalView::vertical_titlebar_sidebar_block_layout(0.0, TABBAR_HEIGHT),
            None
        );
        assert_eq!(
            TerminalView::vertical_titlebar_sidebar_block_layout(64.0, 0.0),
            None
        );
    }

    #[test]
    fn vertical_new_tab_shelf_layout_uses_wide_button_when_expanded() {
        let layout = TerminalView::vertical_new_tab_shelf_layout(219.0, false);
        assert_eq!(layout.shelf_height, VERTICAL_NEW_TAB_SHELF_HEIGHT);
        assert_eq!(layout.button_x, VERTICAL_TAB_STRIP_PADDING);
        assert_eq!(layout.button_y, 8.0);
        assert_eq!(layout.button_width, 203.0);
        assert_eq!(layout.button_height, VERTICAL_NEW_TAB_SHELF_BUTTON_HEIGHT);
    }

    #[test]
    fn vertical_new_tab_shelf_layout_centers_compact_button_when_collapsed() {
        let strip_width =
            collapsed_vertical_tab_strip_width(TerminalView::titlebar_left_padding_for_platform());
        let divider_x = strip_width - TAB_STROKE_THICKNESS;
        let layout = TerminalView::vertical_new_tab_shelf_layout(divider_x, true);
        assert_eq!(layout.shelf_height, VERTICAL_COMPACT_CONTROL_SHELF_HEIGHT);
        assert_eq!(layout.button_height, VERTICAL_NEW_TAB_SHELF_BUTTON_HEIGHT);
        assert_eq!(
            layout.button_y,
            (layout.shelf_height - layout.button_height) * 0.5
        );
        assert_eq!(
            layout.button_width,
            divider_x - (VERTICAL_TAB_STRIP_PADDING * 2.0)
        );
        assert_eq!(layout.button_x, VERTICAL_TAB_STRIP_PADDING);
        assert!(layout.button_x + layout.button_width <= divider_x);
    }

    #[test]
    fn compact_vertical_new_tab_button_is_wider_but_not_taller_than_expanded() {
        let expanded = TerminalView::vertical_new_tab_shelf_layout(219.0, false);
        let compact = TerminalView::vertical_new_tab_shelf_layout(
            collapsed_vertical_tab_strip_width(TerminalView::titlebar_left_padding_for_platform())
                - TAB_STROKE_THICKNESS,
            true,
        );

        assert_eq!(expanded.button_height, VERTICAL_NEW_TAB_SHELF_BUTTON_HEIGHT);
        assert_eq!(compact.button_height, VERTICAL_NEW_TAB_SHELF_BUTTON_HEIGHT);
        assert!(compact.button_width > compact.button_height);
    }

    #[test]
    fn expanded_and_compact_vertical_new_tab_shelves_share_height() {
        let expanded = TerminalView::vertical_new_tab_shelf_layout(219.0, false);
        let compact = TerminalView::vertical_new_tab_shelf_layout(
            collapsed_vertical_tab_strip_width(TerminalView::titlebar_left_padding_for_platform())
                - TAB_STROKE_THICKNESS,
            true,
        );

        assert_eq!(expanded.shelf_height, compact.shelf_height);
        assert_eq!(expanded.button_y, compact.button_y);
    }

    #[test]
    fn vertical_bottom_shelf_height_reserves_button_clearance() {
        assert_eq!(
            TerminalView::vertical_bottom_shelf_height(),
            TABBAR_NEW_TAB_BUTTON_SIZE + (VERTICAL_TAB_STRIP_PADDING * 2.0)
        );
    }

    #[test]
    fn vertical_bottom_shelf_button_origin_is_right_aligned() {
        let strip_width = 220.0;
        let layout = TerminalView::vertical_bottom_shelf_layout(strip_width);
        let divider_x = strip_width - TAB_STROKE_THICKNESS;
        assert_eq!(
            layout.button_x + layout.button_size + VERTICAL_TAB_STRIP_PADDING,
            divider_x
        );
        assert_eq!(
            layout.button_y,
            (layout.shelf_height - layout.button_size) * 0.5
        );
    }

    #[test]
    fn compact_vertical_bottom_shelf_button_stays_inside_collapsed_strip() {
        let strip_width =
            collapsed_vertical_tab_strip_width(TerminalView::titlebar_left_padding_for_platform());
        let layout = TerminalView::vertical_bottom_shelf_layout(strip_width);
        assert!(layout.button_x >= 0.0);
        assert!(layout.button_x + layout.button_size <= strip_width - TAB_STROKE_THICKNESS);
    }

    #[test]
    fn titlebar_branding_width_hides_for_visible_compact_sidebar() {
        assert_eq!(TerminalView::titlebar_branding_width(true, true, 64.0), 0.0);
        assert_eq!(
            TerminalView::titlebar_branding_width(true, false, 64.0),
            64.0
        );
        assert_eq!(
            TerminalView::titlebar_branding_width(false, true, 64.0),
            64.0
        );
    }

    #[test]
    fn vertical_titlebar_right_divider_uses_full_height_without_visible_branding() {
        let divider =
            TerminalView::vertical_titlebar_right_divider_stroke(80.0, TABBAR_HEIGHT, true, 0.0)
                .expect("visible sidebar should always render a titlebar divider");
        assert_eq!(divider.y, 0.0);
        assert_eq!(divider.h, TABBAR_HEIGHT - TAB_STROKE_THICKNESS);
    }

    #[test]
    fn vertical_titlebar_right_divider_uses_handoff_height_with_visible_branding() {
        let divider =
            TerminalView::vertical_titlebar_right_divider_stroke(160.0, TABBAR_HEIGHT, true, 64.0)
                .expect("branding handoff divider should render");
        assert_eq!(
            divider.y,
            (TABBAR_HEIGHT - TAB_ITEM_HEIGHT + TAB_STROKE_THICKNESS).max(0.0)
        );
        assert_eq!(
            divider.h,
            (TABBAR_HEIGHT - divider.y - TAB_STROKE_THICKNESS).max(0.0)
        );
    }

    #[test]
    fn vertical_titlebar_right_divider_hides_when_sidebar_is_hidden() {
        assert_eq!(
            TerminalView::vertical_titlebar_right_divider_stroke(160.0, TABBAR_HEIGHT, false, 64.0),
            None
        );
    }
}

impl TerminalView {
    fn vertical_titlebar_sidebar_block_layout(
        block_width: f32,
        titlebar_height: f32,
    ) -> Option<VerticalTitlebarChromeLayout> {
        if block_width <= f32::EPSILON || titlebar_height <= f32::EPSILON {
            return None;
        }

        Some(VerticalTitlebarChromeLayout {
            block_width,
            bottom_seam: chrome::StrokeRect {
                x: 0.0,
                y: (titlebar_height - TAB_STROKE_THICKNESS).max(0.0),
                w: block_width.max(0.0),
                h: TAB_STROKE_THICKNESS,
            },
        })
    }

    fn titlebar_branding_width(
        show_sidebar_chrome: bool,
        compact_sidebar: bool,
        reserved_width: f32,
    ) -> f32 {
        if show_sidebar_chrome && compact_sidebar {
            0.0
        } else {
            reserved_width.max(0.0)
        }
    }

    fn titlebar_branding_shows_handoff_divider(
        show_sidebar_chrome: bool,
        branding_width: f32,
    ) -> bool {
        show_sidebar_chrome && branding_width > f32::EPSILON
    }

    fn vertical_titlebar_right_divider_stroke(
        block_width: f32,
        titlebar_height: f32,
        show_sidebar_chrome: bool,
        branding_width: f32,
    ) -> Option<chrome::StrokeRect> {
        if !show_sidebar_chrome || block_width <= f32::EPSILON || titlebar_height <= f32::EPSILON {
            return None;
        }

        let (y, h) =
            if Self::titlebar_branding_shows_handoff_divider(show_sidebar_chrome, branding_width) {
                let divider_top =
                    (titlebar_height - TAB_ITEM_HEIGHT + TAB_STROKE_THICKNESS).max(0.0);
                let divider_height =
                    (titlebar_height - divider_top - TAB_STROKE_THICKNESS).max(0.0);
                (divider_top, divider_height)
            } else {
                (0.0, (titlebar_height - TAB_STROKE_THICKNESS).max(0.0))
            };

        Some(chrome::StrokeRect {
            x: (block_width - TAB_STROKE_THICKNESS).max(0.0),
            y,
            w: TAB_STROKE_THICKNESS,
            h,
        })
    }

    #[cfg(test)]
    fn vertical_bottom_shelf_height() -> f32 {
        TABBAR_NEW_TAB_BUTTON_SIZE + (VERTICAL_TAB_STRIP_PADDING * 2.0)
    }

    pub(crate) fn render_titlebar_branding(
        &mut self,
        window: &Window,
        colors: &TerminalColors,
        font_family: &SharedString,
        tabbar_bg: gpui::Rgba,
        show_sidebar_chrome: bool,
        _cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let font_family_key = font_family.to_string();
        let reserved_width =
            self.termy_branding_reserved_width(window, font_family, font_family_key.as_str());
        let branding_width = Self::titlebar_branding_width(
            show_sidebar_chrome,
            self.vertical_tabs_minimized,
            reserved_width,
        );
        if !show_sidebar_chrome && branding_width <= f32::EPSILON {
            return None;
        }

        let gap_width = TOP_STRIP_TERMY_BRANDING_TAB_GAP;
        let leading_inset_width = Self::titlebar_left_padding_for_platform();
        let lane_width = leading_inset_width + branding_width + gap_width;
        let palette = self.resolve_tab_strip_palette(colors, tabbar_bg);
        let mut branding_text_color = palette.inactive_tab_text;
        branding_text_color.a = branding_text_color.a.max(0.82);
        let visible_block_width = self.effective_vertical_tab_strip_width();

        if let Some(layout) = show_sidebar_chrome
            .then(|| {
                Self::vertical_titlebar_sidebar_block_layout(visible_block_width, TABBAR_HEIGHT)
            })
            .flatten()
        {
            let right_divider = Self::vertical_titlebar_right_divider_stroke(
                layout.block_width,
                TABBAR_HEIGHT,
                show_sidebar_chrome,
                branding_width,
            );

            return Some(
                div()
                    .id("vertical-titlebar-chrome-block")
                    .relative()
                    .flex_none()
                    .w(px(layout.block_width))
                    .h(px(TABBAR_HEIGHT))
                    .children(Self::render_termy_branding(
                        font_family,
                        leading_inset_width,
                        branding_width,
                        branding_text_color,
                    ))
                    .children(
                        right_divider.map(|stroke| {
                            Self::render_tab_stroke(stroke, palette.tab_stroke_color)
                        }),
                    )
                    .child(Self::render_tab_stroke(
                        layout.bottom_seam,
                        palette.tab_stroke_color,
                    ))
                    .into_any_element(),
            );
        }

        Some(
            div()
                .id("vertical-titlebar-branding-slot")
                .relative()
                .flex_none()
                .w(px(lane_width))
                .h(px(TABBAR_HEIGHT))
                .children(Self::render_termy_branding(
                    font_family,
                    leading_inset_width,
                    branding_width,
                    branding_text_color,
                ))
                .into_any_element(),
        )
    }

    fn render_vertical_tail(
        layout: &chrome::VerticalTabChromeLayout,
        tab_stroke_color: gpui::Rgba,
    ) -> AnyElement {
        let mut tail = div()
            .id("vertical-tabs-lane-tail")
            .relative()
            .flex_1()
            .min_h(px(0.0));

        if layout.tail.draw_left_edge {
            tail = tail.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(TAB_STROKE_THICKNESS))
                    .bg(tab_stroke_color),
            );
        }

        if layout.tail.draw_content_divider {
            tail = tail.child(
                div()
                    .absolute()
                    .left(px(layout.divider_x))
                    .top_0()
                    .bottom_0()
                    .w(px(TAB_STROKE_THICKNESS))
                    .bg(tab_stroke_color),
            );
        }

        tail.into_any_element()
    }

    fn render_vertical_new_tab_shelf_button(
        &self,
        width: f32,
        height: f32,
        palette: &TabStripPalette,
        show_label: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let corner_radius = TABBAR_NEW_TAB_BUTTON_RADIUS.min(height * 0.5);

        div()
            .id("vertical-top-shelf-new-tab-button")
            .w(px(width))
            .h(px(height))
            .rounded(px(corner_radius))
            .bg(palette.tabbar_new_tab_bg)
            .border_1()
            .border_color(palette.tabbar_new_tab_border)
            .text_color(palette.tabbar_new_tab_text)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                    this.perform_tab_strip_control_action(TabStripControlAction::NewTab, cx);
                    cx.stop_propagation();
                }),
            )
            .hover(move |style| {
                style
                    .bg(palette.tabbar_new_tab_hover_bg)
                    .border_color(palette.tabbar_new_tab_hover_border)
                    .text_color(palette.tabbar_new_tab_hover_text)
            })
            .child(
                div()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(if show_label {
                        VERTICAL_NEW_TAB_SHELF_LABEL_GAP
                    } else {
                        0.0
                    }))
                    .child(
                        div()
                            .text_size(px(VERTICAL_TITLEBAR_CONTROL_ICON_SIZE.min(height)))
                            .font_weight(FontWeight::MEDIUM)
                            .mt(px(TABBAR_NEW_TAB_ICON_BASELINE_NUDGE_Y))
                            .child("+"),
                    )
                    .children(show_label.then(|| {
                        div()
                            .text_size(px(12.0))
                            .font_weight(FontWeight::MEDIUM)
                            .child("New Tab")
                    })),
            )
            .into_any_element()
    }

    fn render_vertical_new_tab_shelf(
        &mut self,
        layout: VerticalNewTabShelfLayout,
        divider_x: f32,
        tab_stroke_color: gpui::Rgba,
        palette: &TabStripPalette,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let button = self.render_vertical_new_tab_shelf_button(
            layout.button_width,
            layout.button_height,
            palette,
            !compact,
            cx,
        );

        div()
            .id("vertical-tabs-top-shelf")
            .flex_none()
            .relative()
            .w_full()
            .h(px(layout.shelf_height))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(TAB_STROKE_THICKNESS))
                    .bg(tab_stroke_color),
            )
            .child(
                div()
                    .absolute()
                    .left(px(divider_x))
                    .top_0()
                    .bottom_0()
                    .w(px(TAB_STROKE_THICKNESS))
                    .bg(tab_stroke_color),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .bottom_0()
                    .h(px(TAB_STROKE_THICKNESS))
                    .bg(tab_stroke_color),
            )
            .child(
                div()
                    .absolute()
                    .left(px(layout.button_x))
                    .top(px(layout.button_y))
                    .child(button),
            )
            .into_any_element()
    }

    fn render_vertical_bottom_shelf(
        &mut self,
        layout: VerticalBottomShelfLayout,
        divider_x: f32,
        tab_stroke_color: gpui::Rgba,
        palette: &TabStripPalette,
        compact: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("vertical-tabs-bottom-shelf")
            .flex_none()
            .relative()
            .w_full()
            .h(px(layout.shelf_height))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top_0()
                    .h(px(TAB_STROKE_THICKNESS))
                    .bg(tab_stroke_color),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .bottom_0()
                    .w(px(TAB_STROKE_THICKNESS))
                    .bg(tab_stroke_color),
            )
            .child(
                div()
                    .absolute()
                    .left(px(divider_x))
                    .top_0()
                    .bottom_0()
                    .w(px(TAB_STROKE_THICKNESS))
                    .bg(tab_stroke_color),
            )
            .child(
                div()
                    .absolute()
                    .left(px(layout.button_x))
                    .top(px(layout.button_y))
                    .child(self.render_tab_strip_control_button(
                        "vertical-bottom-shelf-toggle",
                        if compact { "›" } else { "‹" },
                        TabStripControlAction::ToggleVerticalSidebar,
                        palette.tabbar_new_tab_bg,
                        palette.tabbar_new_tab_hover_bg,
                        palette.tabbar_new_tab_border,
                        palette.tabbar_new_tab_hover_border,
                        palette.tabbar_new_tab_text,
                        palette.tabbar_new_tab_hover_text,
                        layout.button_size,
                        layout.icon_size,
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(crate) fn render_vertical_tab_strip(
        &mut self,
        window: &Window,
        colors: &TerminalColors,
        font_family: &SharedString,
        tabbar_bg: gpui::Rgba,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let font_family_key = font_family.to_string();
        let measured_title_widths =
            self.measure_tab_title_widths(window, font_family, font_family_key.as_str());
        self.sync_tab_title_text_widths(&measured_title_widths);

        let palette = self.resolve_tab_strip_palette(colors, tabbar_bg);
        let now = Instant::now();
        let vertical_layout = self.vertical_tab_strip_layout_snapshot();
        let compact = vertical_layout.compact;
        let strip_width = vertical_layout.strip_width;
        let active_tab_index = (self.active_tab < self.tabs.len()).then_some(self.active_tab);
        let chrome_layout = chrome::compute_vertical_tab_chrome_layout(
            vertical_layout.rows.iter().map(|row| row.height),
            chrome::VerticalTabChromeInput {
                active_index: active_tab_index,
                strip_width,
                control_rail_height: vertical_layout.header_height,
                tab_item_gap: TAB_ITEM_GAP,
                external_top_seam: true,
            },
        );
        debug_assert_eq!(chrome_layout.tab_strokes.len(), self.tabs.len());
        let titlebar_block =
            self.render_titlebar_branding(window, colors, font_family, tabbar_bg, true, cx);

        let mut list = div()
            .id("vertical-tabs-list")
            .relative()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(TAB_ITEM_GAP));

        for index in 0..self.tabs.len() {
            let tab_title = self.tabs[index].title.clone();
            let tab_height = vertical_layout.rows[index].height;
            let anim_progress =
                (tab_height < TAB_ITEM_HEIGHT).then_some(tab_height / TAB_ITEM_HEIGHT);
            let is_active = index == self.active_tab;
            let is_hovered = self.tab_strip.hovered_tab == Some(index);
            let is_renaming = self.renaming_tab == Some(index);
            let show_switch_hint = self.tab_strip.switch_hints.should_render(
                index,
                is_renaming,
                self.tab_switch_hints_blocked(),
                now,
            );
            let switch_hint_label = show_switch_hint
                .then(|| TabSwitchHintState::label_for_index(index))
                .flatten();
            let show_close_button = Self::tab_shows_close(
                self.tab_close_visibility,
                is_active,
                self.tab_strip.hovered_tab,
                self.tab_strip.hovered_tab_close,
                index,
            );
            let show_tab_close = !compact && show_close_button && switch_hint_label.is_none();
            let close_slot_width = if !compact && (show_tab_close || switch_hint_label.is_some()) {
                TAB_CLOSE_SLOT_WIDTH
            } else {
                0.0
            };
            let label = if compact {
                Self::compact_vertical_tab_label(index, &tab_title)
            } else {
                let available_text_px =
                    Self::tab_title_text_area_width(strip_width, close_slot_width);
                Self::format_tab_label_for_render_measured(
                    &tab_title,
                    available_text_px,
                    |candidate| {
                        self.measure_tab_title_width(
                            window,
                            font_family,
                            font_family_key.as_str(),
                            candidate,
                        )
                    },
                )
            };
            list = list.child(self.render_tab_item(
                TabItemRenderInput {
                    orientation: TabStripOrientation::Vertical,
                    index,
                    tab_primary_extent: strip_width,
                    tab_cross_extent: tab_height,
                    tab_strokes: TabItemStrokeRects {
                        top: chrome_layout.tab_strokes[index].top_boundary,
                        bottom: chrome_layout.tab_strokes[index].bottom_boundary,
                        left: Some(chrome_layout.tab_strokes[index].left),
                        right: None,
                    },
                    label,
                    switch_hint_label,
                    is_active,
                    is_hovered,
                    is_renaming,
                    show_tab_close,
                    close_slot_width,
                    text_padding_x: if compact { 0.0 } else { TAB_TEXT_PADDING_X },
                    label_centered: compact,
                    trailing_divider_cover: None,
                    drop_marker_side: self.tab_drop_marker_side(index),
                    open_anim_progress: anim_progress,
                },
                font_family,
                colors,
                &palette,
                cx,
            ));
        }

        // Paint the shared divider after the tab rows so the chrome owns the
        // visible seam instead of letting row backgrounds define that edge.
        for element in Self::render_stroke_segments(
            &chrome_layout.content_divider_strokes,
            palette.tab_stroke_color,
        ) {
            list = list.child(element);
        }

        div()
            .id("vertical-tab-strip")
            .relative()
            .flex_none()
            .w(px(strip_width))
            .h_full()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(Self::handle_vertical_tab_strip_mouse_down),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(Self::handle_unified_titlebar_mouse_up),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(Self::handle_unified_titlebar_mouse_up),
            )
            .on_mouse_move(cx.listener(Self::handle_vertical_tab_strip_mouse_move))
            .children((!compact).then(|| {
                div()
                    .id("vertical-tabs-resize-handle")
                    .absolute()
                    .right(px(-4.0))
                    .top_0()
                    .bottom_0()
                    .w(px(8.0))
                    .cursor_col_resize()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|view, _event: &MouseDownEvent, _window, cx| {
                            view.vertical_tab_strip_resize_drag =
                                Some(VerticalTabStripResizeDragState);
                            cx.stop_propagation();
                        }),
                    )
            }))
            .child(
                div()
                    .relative()
                    .w_full()
                    .h_full()
                    .flex()
                    .flex_col()
                    .children(titlebar_block)
                    .child(self.render_vertical_new_tab_shelf(
                        vertical_layout.top_shelf_layout,
                        chrome_layout.divider_x,
                        palette.tab_stroke_color,
                        &palette,
                        compact,
                        cx,
                    ))
                    .child(
                        div()
                            .id("vertical-tabs-scroll-viewport")
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .track_scroll(&self.tab_strip.vertical_scroll_handle)
                            .child(
                                div()
                                    .id("vertical-tabs-scroll-content")
                                    .w_full()
                                    .h_full()
                                    .flex()
                                    .flex_col()
                                    .child(list)
                                    .child(Self::render_vertical_tail(
                                        &chrome_layout,
                                        palette.tab_stroke_color,
                                    )),
                            ),
                    )
                    .child(self.render_vertical_bottom_shelf(
                        vertical_layout.bottom_shelf_layout,
                        chrome_layout.divider_x,
                        palette.tab_stroke_color,
                        &palette,
                        compact,
                        cx,
                    )),
            )
            .into_any_element()
    }
}
