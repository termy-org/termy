use super::super::*;
use super::hints::TabSwitchHintState;
use super::render_controls::TabStripControlAction;
use super::render_palette::TabStripPalette;
use super::render_shared::TabStripRenderState;
use super::render_tab_item::{TabItemRenderInput, TabItemStrokeRects};
use super::state::{TabStripOrientation, TabStripOverflowState};

#[cfg(test)]
use super::chrome;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gutter_divider_shows_without_overflow() {
        assert!(TerminalView::should_render_gutter_divider(
            TabStripOverflowState {
                left: false,
                right: false,
            },
            false,
        ));
    }

    #[test]
    fn gutter_divider_shows_when_only_right_overflow_exists() {
        assert!(TerminalView::should_render_gutter_divider(
            TabStripOverflowState {
                left: false,
                right: true,
            },
            false,
        ));
    }

    #[test]
    fn gutter_divider_shows_when_overflow_exists_on_both_sides() {
        assert!(TerminalView::should_render_gutter_divider(
            TabStripOverflowState {
                left: true,
                right: true,
            },
            false,
        ));
    }

    #[test]
    fn gutter_divider_hides_at_true_max_right_scroll() {
        assert!(!TerminalView::should_render_gutter_divider(
            TabStripOverflowState {
                left: true,
                right: false,
            },
            false,
        ));
    }

    #[test]
    fn left_inset_divider_hides_without_left_overflow() {
        assert!(!TerminalView::should_render_left_inset_divider(
            TabStripOverflowState {
                left: false,
                right: true,
            },
            false,
        ));
        assert!(!TerminalView::should_render_left_inset_divider(
            TabStripOverflowState {
                left: false,
                right: false,
            },
            false,
        ));
    }

    #[test]
    fn left_inset_divider_shows_when_left_overflow_exists() {
        assert!(TerminalView::should_render_left_inset_divider(
            TabStripOverflowState {
                left: true,
                right: true,
            },
            false,
        ));
        assert!(TerminalView::should_render_left_inset_divider(
            TabStripOverflowState {
                left: true,
                right: false,
            },
            false,
        ));
    }

    #[test]
    fn gutter_divider_hides_when_tab_boundary_already_occupies_right_edge() {
        assert!(!TerminalView::should_render_gutter_divider(
            TabStripOverflowState {
                left: false,
                right: true,
            },
            true,
        ));
    }

    #[test]
    fn left_inset_divider_hides_when_tab_boundary_already_occupies_left_edge() {
        assert!(!TerminalView::should_render_left_inset_divider(
            TabStripOverflowState {
                left: true,
                right: true,
            },
            true,
        ));
    }

    #[test]
    fn edge_divider_collision_detects_fractional_overlap_on_both_edges() {
        let layout = chrome::compute_tab_chrome_layout(
            [100.0],
            chrome::TabChromeInput {
                active_index: Some(0),
                tabbar_height: TABBAR_HEIGHT,
                tab_item_height: TAB_ITEM_HEIGHT,
                horizontal_padding: TAB_HORIZONTAL_PADDING,
                tab_item_gap: TAB_ITEM_GAP,
            },
        );

        let collisions = TerminalView::edge_divider_collision_state(&layout, -0.49, 100.0);
        assert!(collisions.left);
        assert!(collisions.right);
    }

    #[test]
    fn edge_divider_collision_ignores_fractional_non_overlap_on_both_edges() {
        let layout = chrome::compute_tab_chrome_layout(
            [100.0],
            chrome::TabChromeInput {
                active_index: Some(0),
                tabbar_height: TABBAR_HEIGHT,
                tab_item_height: TAB_ITEM_HEIGHT,
                horizontal_padding: TAB_HORIZONTAL_PADDING,
                tab_item_gap: TAB_ITEM_GAP,
            },
        );

        let collisions = TerminalView::edge_divider_collision_state(&layout, -1.01, 100.0);
        assert!(!collisions.left);
        assert!(!collisions.right);
    }
}

impl TerminalView {
    fn render_inset_lane(
        id: &'static str,
        width: f32,
        tab_baseline_y: f32,
        tab_stroke_color: gpui::Rgba,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .relative()
            .flex_none()
            .w(px(width))
            .h_full()
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.on_action_rail_mouse_move(event, window, cx);
            }))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(tab_baseline_y))
                    .h(px(TAB_STROKE_THICKNESS))
                    .bg(tab_stroke_color),
            )
            .into_any_element()
    }

    pub(super) fn render_termy_branding(
        font_family: &SharedString,
        termy_branding_slot_start_x: f32,
        termy_branding_slot_width: f32,
        termy_branding_text_color: gpui::Rgba,
    ) -> Option<AnyElement> {
        (termy_branding_slot_width > f32::EPSILON).then(|| {
            div()
                .id("tabbar-termy-branding")
                .absolute()
                .left(px(termy_branding_slot_start_x.max(0.0)))
                .top_0()
                .bottom_0()
                .w(px(termy_branding_slot_width.max(0.0)))
                .overflow_hidden()
                .flex()
                .items_center()
                .justify_center()
                .child(
                    div()
                        .font_family(font_family.clone())
                        .text_size(px(TOP_STRIP_TERMY_BRANDING_FONT_SIZE))
                        .text_color(termy_branding_text_color)
                        .child(TOP_STRIP_TERMY_BRANDING_TEXT),
                )
                .into_any_element()
        })
    }

    fn render_left_inset_lane(
        width: f32,
        tab_baseline_y: f32,
        tab_stroke_color: gpui::Rgba,
        font_family: &SharedString,
        termy_branding_slot_start_x: f32,
        termy_branding_slot_width: f32,
        termy_branding_text_color: gpui::Rgba,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("tabbar-left-inset")
            .relative()
            .flex_none()
            .w(px(width))
            .h_full()
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.on_action_rail_mouse_move(event, window, cx);
            }))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(tab_baseline_y))
                    .h(px(TAB_STROKE_THICKNESS))
                    .bg(tab_stroke_color),
            )
            .children(Self::render_termy_branding(
                font_family,
                termy_branding_slot_start_x,
                termy_branding_slot_width,
                termy_branding_text_color,
            ))
            .into_any_element()
    }

    fn render_gutter_lane(
        gutter_width: f32,
        tab_baseline_y: f32,
        tab_stroke_color: gpui::Rgba,
        show_divider: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("tabbar-action-gutter")
            .relative()
            .flex_none()
            .w(px(gutter_width))
            .h_full()
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.on_action_rail_mouse_move(event, window, cx);
            }))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(tab_baseline_y))
                    .h(px(TAB_STROKE_THICKNESS))
                    .bg(tab_stroke_color),
            )
            .children(show_divider.then(|| {
                div()
                    .absolute()
                    .left(px(-TAB_STROKE_THICKNESS))
                    .top(px(TAB_STROKE_THICKNESS))
                    .bottom_0()
                    .w(px(TAB_STROKE_THICKNESS))
                    .bg(tab_stroke_color)
            }))
            .into_any_element()
    }

    fn should_render_gutter_divider(
        overflow: TabStripOverflowState,
        boundary_at_viewport_right: bool,
    ) -> bool {
        (overflow.right || !overflow.left) && !boundary_at_viewport_right
    }

    fn should_render_left_inset_divider(
        overflow: TabStripOverflowState,
        boundary_at_viewport_left: bool,
    ) -> bool {
        overflow.left && !boundary_at_viewport_left
    }

    #[allow(clippy::too_many_arguments)]
    fn build_tabs_scroll_content(
        &mut self,
        window: &Window,
        state: &TabStripRenderState,
        palette: &TabStripPalette,
        font_family: &SharedString,
        font_family_key: &str,
        colors: &TerminalColors,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let now = Instant::now();
        let new_tab_anim = self.new_tab_animation_progress(now);
        let mut tabs_scroll_content = div()
            .id("tabs-scroll-content")
            .flex_none()
            .w(px(state.content_width))
            .min_w(px(state.content_width))
            .h(px(TABBAR_HEIGHT))
            .flex()
            .relative()
            .items_end()
            .gap(px(TAB_ITEM_GAP))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.on_tabs_content_mouse_move(
                    TabStripOrientation::Horizontal,
                    event,
                    window,
                    cx,
                );
            }));

        tabs_scroll_content = tabs_scroll_content.child(
            div()
                .id("tabs-left-padding-spacer")
                .flex_none()
                .w(px(TAB_HORIZONTAL_PADDING))
                .h(px(TABBAR_HEIGHT)),
        );

        for index in 0..self.tabs.len() {
            let (display_width, tab_title) = {
                let tab = &self.tabs[index];
                (tab.display_width, tab.title.clone())
            };
            let anim_progress = new_tab_anim
                .filter(|(anim_index, _)| *anim_index == index)
                .map(|(_, p)| p);
            let tab_width = if let Some(p) = anim_progress {
                display_width * p
            } else {
                display_width
            };
            let is_active = index == self.active_tab;
            let is_hovered = self.tab_strip.hovered_tab == Some(index);
            let show_close_button = Self::tab_shows_close(
                self.tab_close_visibility,
                is_active,
                self.tab_strip.hovered_tab,
                self.tab_strip.hovered_tab_close,
                index,
            );
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
            let show_tab_close = show_close_button && switch_hint_label.is_none();
            let close_slot_width = if show_tab_close || switch_hint_label.is_some() {
                TAB_CLOSE_SLOT_WIDTH
            } else {
                0.0
            };
            let available_text_px = Self::tab_title_text_area_width(tab_width, close_slot_width);
            let label = Self::format_tab_label_for_render_measured(
                &tab_title,
                available_text_px,
                |candidate| {
                    self.measure_tab_title_width(window, font_family, font_family_key, candidate)
                },
            );

            let tab_item = self.render_tab_item(
                TabItemRenderInput {
                    orientation: TabStripOrientation::Horizontal,
                    index,
                    tab_primary_extent: tab_width,
                    tab_cross_extent: TAB_ITEM_HEIGHT,
                    tab_strokes: TabItemStrokeRects {
                        top: Some(state.chrome_layout.tab_strokes[index].top),
                        bottom: None,
                        left: state.chrome_layout.tab_strokes[index].left_boundary,
                        right: state.chrome_layout.tab_strokes[index].right_boundary,
                    },
                    label,
                    switch_hint_label,
                    is_active,
                    is_hovered,
                    is_renaming,
                    show_tab_close,
                    close_slot_width,
                    text_padding_x: TAB_TEXT_PADDING_X,
                    label_centered: false,
                    trailing_divider_cover: None,
                    drop_marker_side: self.tab_drop_marker_side(index),
                    open_anim_progress: anim_progress,
                },
                font_family,
                colors,
                palette,
                cx,
            );

            tabs_scroll_content = tabs_scroll_content.child(tab_item);
        }

        for element in
            Self::render_baseline_segments(&state.chrome_layout, palette.tab_stroke_color)
        {
            tabs_scroll_content = tabs_scroll_content.child(element);
        }

        tabs_scroll_content.into_any_element()
    }

    fn render_action_rail(
        &mut self,
        state: &TabStripRenderState,
        palette: &TabStripPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let tabbar_new_tab_left =
            (state.geometry.button_start_x - state.geometry.action_rail_start_x).max(0.0);
        let tabbar_new_tab_top =
            (state.geometry.button_start_y - TOP_STRIP_CONTENT_OFFSET_Y).max(0.0);
        let tabbar_new_tab_size =
            (state.geometry.button_end_x - state.geometry.button_start_x).max(0.0);

        div()
            .id("tabbar-action-rail")
            .relative()
            .flex_none()
            .w(px(state.geometry.action_rail_width))
            .h_full()
            .on_scroll_wheel(cx.listener(Self::handle_tab_strip_action_rail_scroll_wheel))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.on_action_rail_mouse_move(event, window, cx);
            }))
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .top(px(state.chrome_layout.baseline_y))
                    .h(px(TAB_STROKE_THICKNESS))
                    .bg(palette.tab_stroke_color),
            )
            .child(
                div()
                    .absolute()
                    .left(px(tabbar_new_tab_left))
                    .top(px(tabbar_new_tab_top))
                    .child(self.render_tab_strip_control_button(
                        "tabbar-new-tab",
                        "+",
                        TabStripControlAction::NewTab,
                        palette.tabbar_new_tab_bg,
                        palette.tabbar_new_tab_hover_bg,
                        palette.tabbar_new_tab_border,
                        palette.tabbar_new_tab_hover_border,
                        palette.tabbar_new_tab_text,
                        palette.tabbar_new_tab_hover_text,
                        tabbar_new_tab_size,
                        TABBAR_NEW_TAB_ICON_SIZE,
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(crate) fn render_tab_strip(
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

        let base_left_inset_width = Self::titlebar_left_padding_for_platform();
        let termy_branding_reserved_width =
            self.termy_branding_reserved_width(window, font_family, font_family_key.as_str());
        let termy_branding_tab_gap = if termy_branding_reserved_width > f32::EPSILON {
            TOP_STRIP_TERMY_BRANDING_TAB_GAP
        } else {
            0.0
        };
        let state = self.build_tab_strip_render_state(
            window,
            base_left_inset_width + termy_branding_reserved_width + termy_branding_tab_gap,
        );
        let palette = self.resolve_tab_strip_palette(colors, tabbar_bg);
        let termy_branding_slot_start_x =
            base_left_inset_width.min(state.geometry.left_inset_width);
        let termy_branding_slot_width = (state.geometry.left_inset_width
            - termy_branding_slot_start_x)
            .max(0.0)
            .min(termy_branding_reserved_width.max(0.0));
        let mut termy_branding_text_color = palette.inactive_tab_text;
        termy_branding_text_color.a = termy_branding_text_color.a.max(0.82);
        let tabs_scroll_content = self.build_tabs_scroll_content(
            window,
            &state,
            &palette,
            font_family,
            font_family_key.as_str(),
            colors,
            cx,
        );
        let scroll_offset_x: f32 = self.tab_strip.horizontal_scroll_handle.offset().x.into();
        let divider_collisions = Self::edge_divider_collision_state(
            &state.chrome_layout,
            scroll_offset_x,
            state.geometry.tabs_viewport_width,
        );

        let show_gutter_divider =
            Self::should_render_gutter_divider(state.overflow_state, divider_collisions.right);
        let show_left_inset_divider =
            Self::should_render_left_inset_divider(state.overflow_state, divider_collisions.left);

        div()
            .w_full()
            .h(px(TABBAR_HEIGHT))
            .flex()
            .children((state.geometry.left_inset_width > 0.0).then(|| {
                Self::render_left_inset_lane(
                    state.geometry.left_inset_width,
                    state.chrome_layout.baseline_y,
                    palette.tab_stroke_color,
                    font_family,
                    termy_branding_slot_start_x,
                    termy_branding_slot_width,
                    termy_branding_text_color,
                    cx,
                )
            }))
            .child(
                div()
                    .id("tabs-scroll-viewport-lane")
                    .flex_none()
                    .w(px(state.geometry.tabs_viewport_width))
                    .min_w(px(0.0))
                    .h_full()
                    .relative()
                    .child(
                        div()
                            .id("tabs-scroll-viewport")
                            .absolute()
                            .left_0()
                            .top_0()
                            .right_0()
                            .bottom_0()
                            .overflow_x_scroll()
                            .track_scroll(&self.tab_strip.horizontal_scroll_handle)
                            .child(tabs_scroll_content),
                    )
                    .children(show_left_inset_divider.then(|| {
                        div()
                            .absolute()
                            .left_0()
                            .top(px(TAB_STROKE_THICKNESS))
                            .bottom_0()
                            .w(px(TAB_STROKE_THICKNESS))
                            .bg(palette.tab_stroke_color)
                    })),
            )
            .children((state.geometry.gutter_width > 0.0).then(|| {
                Self::render_gutter_lane(
                    state.geometry.gutter_width,
                    state.chrome_layout.baseline_y,
                    palette.tab_stroke_color,
                    show_gutter_divider,
                    cx,
                )
            }))
            .children(
                (state.geometry.action_rail_width > 0.0)
                    .then(|| self.render_action_rail(&state, &palette, cx)),
            )
            .children((state.geometry.right_inset_width > 0.0).then(|| {
                Self::render_inset_lane(
                    "tabbar-right-inset",
                    state.geometry.right_inset_width,
                    state.chrome_layout.baseline_y,
                    palette.tab_stroke_color,
                    cx,
                )
            }))
            .into_any_element()
    }
}
