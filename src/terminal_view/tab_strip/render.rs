use super::super::*;
use super::chrome;
use super::hints::TabSwitchHintState;
use super::layout::TabStripGeometry;
use super::state::{TabDropMarkerSide, TabStripOrientation, TabStripOverflowState};
use gpui::{Hsla, TextRun};

#[derive(Clone, Copy)]
struct TabStripPalette {
    tab_stroke_color: gpui::Rgba,
    inactive_tab_bg: gpui::Rgba,
    active_tab_bg: gpui::Rgba,
    hovered_tab_bg: gpui::Rgba,
    active_tab_text: gpui::Rgba,
    inactive_tab_text: gpui::Rgba,
    close_button_hover_bg: gpui::Rgba,
    close_button_hover_text: gpui::Rgba,
    switch_hint_bg: gpui::Rgba,
    switch_hint_border: gpui::Rgba,
    switch_hint_text: gpui::Rgba,
    tab_drop_marker_color: gpui::Rgba,
    tabbar_new_tab_bg: gpui::Rgba,
    tabbar_new_tab_hover_bg: gpui::Rgba,
    tabbar_new_tab_border: gpui::Rgba,
    tabbar_new_tab_hover_border: gpui::Rgba,
    tabbar_new_tab_text: gpui::Rgba,
    tabbar_new_tab_hover_text: gpui::Rgba,
}

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
    fn tab_strip_chrome_visible_matches_auto_hide_policy() {
        assert!(!TerminalView::tab_strip_chrome_visible(true, 1));
        assert!(!TerminalView::tab_strip_chrome_visible(true, 0));
        assert!(TerminalView::tab_strip_chrome_visible(false, 1));
        assert!(TerminalView::tab_strip_chrome_visible(true, 2));
    }
}

struct TabStripRenderState {
    geometry: TabStripGeometry,
    content_width: f32,
    overflow_state: TabStripOverflowState,
    chrome_layout: chrome::TabChromeLayout,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct DividerCollisionState {
    left: bool,
    right: bool,
}

struct TabItemRenderInput {
    orientation: TabStripOrientation,
    index: usize,
    tab_primary_extent: f32,
    tab_cross_extent: f32,
    tab_strokes: TabItemStrokeRects,
    label: String,
    switch_hint_label: Option<String>,
    is_active: bool,
    is_hovered: bool,
    is_renaming: bool,
    show_tab_close: bool,
    close_slot_width: f32,
    text_padding_x: f32,
    label_centered: bool,
    trailing_divider_cover: Option<gpui::Rgba>,
    drop_marker_side: Option<TabDropMarkerSide>,
    /// 0.0..=1.0 progress for new-tab open animation, None when not animating
    open_anim_progress: Option<f32>,
}

#[derive(Clone, Copy)]
struct TabItemStrokeRects {
    top: Option<chrome::StrokeRect>,
    bottom: Option<chrome::StrokeRect>,
    left: Option<chrome::StrokeRect>,
    right: Option<chrome::StrokeRect>,
}

#[derive(Clone, Copy)]
enum TabStripControlAction {
    NewTab,
    ToggleVerticalSidebar,
}

impl TerminalView {
    fn edge_divider_collision_state(
        layout: &chrome::TabChromeLayout,
        scroll_offset_x: f32,
        tabs_viewport_width: f32,
    ) -> DividerCollisionState {
        let left_divider_start_col = 0_i32;
        let left_divider_end_col = (TAB_STROKE_THICKNESS.ceil() as i32).max(1);
        let right_divider_x = (tabs_viewport_width - TAB_STROKE_THICKNESS).max(0.0);
        let right_divider_start_col = right_divider_x.floor() as i32;
        let right_divider_end_col = ((right_divider_x + TAB_STROKE_THICKNESS).ceil() as i32)
            .max(right_divider_start_col + 1);

        let mut collisions = DividerCollisionState::default();

        for stroke in &layout.boundary_strokes {
            let boundary_left = stroke.x + scroll_offset_x;
            let boundary_start_col = boundary_left.floor() as i32;
            let boundary_end_col =
                ((boundary_left + TAB_STROKE_THICKNESS).ceil() as i32).max(boundary_start_col + 1);

            if boundary_start_col < left_divider_end_col
                && boundary_end_col > left_divider_start_col
            {
                collisions.left = true;
            }
            if boundary_start_col < right_divider_end_col
                && boundary_end_col > right_divider_start_col
            {
                collisions.right = true;
            }

            if collisions.left && collisions.right {
                break;
            }
        }

        collisions
    }

    fn measure_text_width(
        &mut self,
        window: &Window,
        font_family: &SharedString,
        font_family_key: &str,
        text: &str,
        font_size_px: f32,
    ) -> f32 {
        if text.is_empty() {
            return 0.0;
        }

        if !font_size_px.is_finite() || font_size_px <= 0.0 {
            return 0.0;
        }
        let font_size_bits = font_size_px.to_bits();
        if let Some(width) =
            self.tab_strip
                .title_width_cache
                .get(text, font_family_key, font_size_bits)
        {
            return width;
        }

        let run = TextRun {
            len: text.len(),
            font: Font {
                family: font_family.clone(),
                weight: FontWeight::NORMAL,
                ..Default::default()
            },
            color: Hsla {
                h: 0.0,
                s: 0.0,
                l: 1.0,
                a: 1.0,
            },
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let shaped = window.text_system().shape_line(
            text.to_string().into(),
            px(font_size_px),
            &[run],
            None,
        );
        let width: f32 = shaped.x_for_index(text.len()).into();
        let width = width.max(0.0);
        self.tab_strip
            .title_width_cache
            .insert(text, font_family_key, font_size_bits, width);
        width
    }

    fn measure_tab_title_width(
        &mut self,
        window: &Window,
        font_family: &SharedString,
        font_family_key: &str,
        title: &str,
    ) -> f32 {
        self.measure_text_width(window, font_family, font_family_key, title, 12.0)
    }

    fn measure_tab_title_widths(
        &mut self,
        window: &Window,
        font_family: &SharedString,
        font_family_key: &str,
    ) -> Vec<f32> {
        let mut widths = Vec::with_capacity(self.tabs.len());
        for index in 0..self.tabs.len() {
            let title = self.tabs[index].title.clone();
            widths.push(self.measure_tab_title_width(window, font_family, font_family_key, &title));
        }
        widths
    }

    fn termy_branding_reserved_width(
        &mut self,
        window: &Window,
        font_family: &SharedString,
        font_family_key: &str,
    ) -> f32 {
        if !cfg!(target_os = "macos") || !self.show_termy_in_titlebar {
            return 0.0;
        }

        let text_width = self.measure_text_width(
            window,
            font_family,
            font_family_key,
            TOP_STRIP_TERMY_BRANDING_TEXT,
            TOP_STRIP_TERMY_BRANDING_FONT_SIZE,
        );
        if text_width <= f32::EPSILON {
            return 0.0;
        }

        text_width + (TOP_STRIP_TERMY_BRANDING_SIDE_PADDING * 2.0)
    }

    fn resolve_tab_strip_palette(
        &self,
        colors: &TerminalColors,
        tabbar_bg: gpui::Rgba,
    ) -> TabStripPalette {
        let tab_stroke_color = chrome::resolve_tab_stroke_color(
            tabbar_bg,
            colors.foreground,
            TAB_STROKE_FOREGROUND_MIX,
        );
        let mut inactive_tab_bg = colors.foreground;
        inactive_tab_bg.a = self.scaled_chrome_alpha(0.10);
        let mut active_tab_bg = tabbar_bg;
        active_tab_bg.a = 0.0;
        let mut hovered_tab_bg = colors.foreground;
        hovered_tab_bg.a = self.scaled_chrome_alpha(0.13);
        let mut active_tab_text = colors.foreground;
        active_tab_text.a = 0.95;
        let mut inactive_tab_text = colors.foreground;
        inactive_tab_text.a = 0.7;
        let mut close_button_hover_bg = colors.foreground;
        close_button_hover_bg.a = self.scaled_chrome_alpha(0.24);
        let mut close_button_hover_text = colors.foreground;
        close_button_hover_text.a = 0.98;
        let now = Instant::now();
        let hint_progress = self.tab_switch_hint_progress(now);
        let mut switch_hint_bg = colors.cursor;
        switch_hint_bg.a = self.scaled_chrome_alpha(0.18 * hint_progress);
        let mut switch_hint_border = colors.cursor;
        switch_hint_border.a = self.scaled_chrome_alpha(0.52 * hint_progress);
        let mut switch_hint_text = colors.foreground;
        switch_hint_text.a = (0.99 * hint_progress).clamp(0.0, 1.0);
        let mut tab_drop_marker_color = colors.cursor;
        tab_drop_marker_color.a = self.scaled_chrome_alpha(0.95);
        let mut tabbar_new_tab_bg = colors.foreground;
        tabbar_new_tab_bg.a = self.scaled_chrome_alpha(0.11);
        let mut tabbar_new_tab_hover_bg = colors.foreground;
        tabbar_new_tab_hover_bg.a = self.scaled_chrome_alpha(0.2);
        let mut tabbar_new_tab_border = colors.foreground;
        tabbar_new_tab_border.a = self.scaled_chrome_alpha(0.24);
        let mut tabbar_new_tab_hover_border = colors.cursor;
        tabbar_new_tab_hover_border.a = self.scaled_chrome_alpha(0.76);
        let mut tabbar_new_tab_text = colors.foreground;
        tabbar_new_tab_text.a = 0.9;
        let mut tabbar_new_tab_hover_text = colors.cursor;
        tabbar_new_tab_hover_text.a = 0.98;

        TabStripPalette {
            tab_stroke_color,
            inactive_tab_bg,
            active_tab_bg,
            hovered_tab_bg,
            active_tab_text,
            inactive_tab_text,
            close_button_hover_bg,
            close_button_hover_text,
            switch_hint_bg,
            switch_hint_border,
            switch_hint_text,
            tab_drop_marker_color,
            tabbar_new_tab_bg,
            tabbar_new_tab_hover_bg,
            tabbar_new_tab_border,
            tabbar_new_tab_hover_border,
            tabbar_new_tab_text,
            tabbar_new_tab_hover_text,
        }
    }

    fn compact_vertical_tab_label(index: usize, title: &str) -> String {
        if index < 9 {
            return (index + 1).to_string();
        }

        title.chars().next().unwrap_or('•').to_string()
    }

    pub(crate) fn tab_strip_chrome_visible(auto_hide_tabbar: bool, tab_count: usize) -> bool {
        !auto_hide_tabbar || tab_count > 1
    }

    pub(crate) fn should_render_tab_strip_chrome(&self) -> bool {
        Self::tab_strip_chrome_visible(self.auto_hide_tabbar, self.tabs.len())
    }

    fn build_tab_strip_render_state(
        &mut self,
        window: &Window,
        left_inset_width: f32,
    ) -> TabStripRenderState {
        let viewport_width: f32 = window.viewport_size().width.into();
        let layout =
            Self::tab_strip_layout_for_viewport_with_left_inset(viewport_width, left_inset_width);
        self.set_tab_strip_layout_snapshot(layout);

        let mut geometry = layout.geometry;
        geometry.tabs_viewport_width += geometry.action_rail_width;
        geometry.gutter_start_x += geometry.action_rail_width;
        geometry.action_rail_start_x += geometry.action_rail_width;
        geometry.action_rail_width = 0.0;
        let tab_strip_viewport_width = geometry.tabs_viewport_width;
        let widths_changed =
            self.sync_tab_display_widths_for_viewport_if_needed(tab_strip_viewport_width);
        if widths_changed {
            // Width updates can move the active tab offscreen (especially after
            // tmux snapshot/title sync). Snap once here to keep parity with
            // non-tmux active-tab visibility without overriding manual scrolling.
            self.scroll_active_tab_into_view(TabStripOrientation::Horizontal);
        }
        let content_width = self
            .tab_strip_fixed_content_width()
            .max(tab_strip_viewport_width);
        let overflow_state = self.tab_strip_overflow_state();
        let active_tab_index = (self.active_tab < self.tabs.len()).then_some(self.active_tab);
        let chrome_layout = chrome::compute_tab_chrome_layout(
            self.tabs.iter().map(|tab| tab.display_width),
            chrome::TabChromeInput {
                active_index: active_tab_index,
                tabbar_height: TABBAR_HEIGHT,
                tab_item_height: TAB_ITEM_HEIGHT,
                horizontal_padding: TAB_HORIZONTAL_PADDING,
                tab_item_gap: TAB_ITEM_GAP,
            },
        );
        debug_assert!(chrome_layout.tab_strokes.len() == self.tabs.len());

        TabStripRenderState {
            geometry,
            content_width,
            overflow_state,
            chrome_layout,
        }
    }

    fn render_tab_stroke(stroke: chrome::StrokeRect, color: gpui::Rgba) -> AnyElement {
        div()
            .absolute()
            .left(px(stroke.x))
            .top(px(stroke.y))
            .w(px(stroke.w))
            .h(px(stroke.h))
            .bg(color)
            .into_any_element()
    }

    fn render_inset_lane(
        id: &'static str,
        width: f32,
        tab_baseline_y: f32,
        tab_stroke_color: gpui::Rgba,
    ) -> AnyElement {
        div()
            .id(id)
            .relative()
            .flex_none()
            .w(px(width))
            .h_full()
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

    fn render_termy_branding(
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
    ) -> AnyElement {
        div()
            .id("tabbar-left-inset")
            .relative()
            .flex_none()
            .w(px(width))
            .h_full()
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

    pub(crate) fn render_vertical_titlebar_branding(
        &mut self,
        window: &Window,
        colors: &TerminalColors,
        font_family: &SharedString,
        tabbar_bg: gpui::Rgba,
    ) -> Option<AnyElement> {
        let font_family_key = font_family.to_string();
        let reserved_width =
            self.termy_branding_reserved_width(window, font_family, font_family_key.as_str());
        if reserved_width <= f32::EPSILON {
            return None;
        }

        let gap_width = TOP_STRIP_TERMY_BRANDING_TAB_GAP;
        let leading_inset_width = Self::titlebar_left_padding_for_platform();
        let lane_width = leading_inset_width + reserved_width + gap_width;
        let palette = self.resolve_tab_strip_palette(colors, tabbar_bg);
        let mut branding_text_color = palette.inactive_tab_text;
        branding_text_color.a = branding_text_color.a.max(0.82);

        // Vertical mode keeps the titlebar branding anchored in the same slot,
        // but the sidebar chrome owns the titlebar/content seam. Reusing the
        // horizontal inset lane here would double-draw a baseline.
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
                    reserved_width,
                    branding_text_color,
                ))
                .into_any_element(),
        )
    }

    fn render_gutter_lane(
        gutter_width: f32,
        tab_baseline_y: f32,
        tab_stroke_color: gpui::Rgba,
        show_divider: bool,
    ) -> AnyElement {
        div()
            .id("tabbar-action-gutter")
            .relative()
            .flex_none()
            .w(px(gutter_width))
            .h_full()
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

    fn render_baseline_segments(
        layout: &chrome::TabChromeLayout,
        tab_stroke_color: gpui::Rgba,
    ) -> Vec<AnyElement> {
        let mut elements = Vec::with_capacity(layout.baseline_strokes.len() + 1);
        for segment in &layout.baseline_strokes {
            elements.push(Self::render_tab_stroke(*segment, tab_stroke_color));
        }
        elements.push(
            div()
                .id("tabs-baseline-tail-filler")
                .flex_1()
                .min_w(px(0.0))
                .h(px(TABBAR_HEIGHT))
                .relative()
                .child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(px(layout.baseline_y))
                        .h(px(TAB_STROKE_THICKNESS))
                        .bg(tab_stroke_color),
                )
                .into_any_element(),
        );
        elements
    }

    fn render_stroke_segments(
        strokes: &[chrome::StrokeRect],
        tab_stroke_color: gpui::Rgba,
    ) -> Vec<AnyElement> {
        strokes
            .iter()
            .copied()
            .map(|segment| Self::render_tab_stroke(segment, tab_stroke_color))
            .collect()
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

    fn render_vertical_utility_dock(
        &self,
        compact: bool,
        dock_height: f32,
        strip_width: f32,
        divider_x: f32,
        collapse_button: AnyElement,
        new_tab_button: AnyElement,
        palette: &TabStripPalette,
    ) -> AnyElement {
        let dock_seam = chrome::StrokeRect {
            x: TAB_STROKE_THICKNESS,
            y: 0.0,
            w: divider_x,
            h: TAB_STROKE_THICKNESS,
        };

        let controls = div()
            .id("vertical-tabs-utility-controls")
            .flex()
            .items_center()
            .gap(px(if compact { 4.0 } else { 6.0 }))
            .child(collapse_button)
            .child(new_tab_button);

        let mut dock = div()
            .id("vertical-tabs-utility-dock")
            .relative()
            .flex_none()
            .w(px(strip_width))
            .h(px(dock_height))
            .px(px(VERTICAL_TAB_STRIP_PADDING))
            .flex()
            .items_center();

        dock = dock.justify_center();

        dock.child(Self::render_tab_stroke(dock_seam, palette.tab_stroke_color))
            .child(controls)
            .into_any_element()
    }

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
            .rounded(px(5.0))
            .text_color(close_text_color)
            .text_size(px(12.0))
            .child("×")
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
                    .text_color(palette.close_button_hover_text)
            })
            .cursor_pointer()
            .into_any_element()
    }

    fn render_tab_item(
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
                    TabDropMarkerSide::Trailing => {
                        input.tab_primary_extent - TAB_DROP_MARKER_WIDTH
                    }
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
                    TabDropMarkerSide::Trailing => {
                        input.tab_cross_extent - TAB_DROP_MARKER_WIDTH
                    }
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

    #[allow(clippy::too_many_arguments)]
    fn render_tab_strip_control_button(
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
                    match action {
                        TabStripControlAction::NewTab => {
                            this.disarm_titlebar_window_move();
                            this.add_tab(cx);
                        }
                        TabStripControlAction::ToggleVerticalSidebar => {
                            if let Err(error) =
                                this.set_vertical_tabs_minimized(!this.vertical_tabs_minimized)
                            {
                                termy_toast::error(error);
                            } else {
                                cx.notify();
                            }
                        }
                    }
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
            // baseline stroke intentionally omitted (new tab button removed)
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
        let new_tab_anim = self.new_tab_animation_progress(now);
        let compact = self.vertical_tabs_minimized;
        let strip_width = self.effective_vertical_tab_strip_width();
        let utility_dock_height = self.vertical_tab_strip_utility_dock_height();
        let active_tab_index = (self.active_tab < self.tabs.len()).then_some(self.active_tab);
        let tab_heights: Vec<f32> = (0..self.tabs.len())
            .map(|index| {
                let anim_progress = new_tab_anim
                    .filter(|(anim_index, _)| *anim_index == index)
                    .map(|(_, progress)| progress)
                    .unwrap_or(1.0);
                TAB_ITEM_HEIGHT * anim_progress
            })
            .collect();
        let chrome_layout = chrome::compute_vertical_tab_chrome_layout(
            tab_heights.iter().copied(),
            chrome::VerticalTabChromeInput {
                active_index: active_tab_index,
                strip_width,
                control_rail_height: self.vertical_tab_strip_header_height(),
                tab_item_gap: TAB_ITEM_GAP,
            },
        );
        debug_assert_eq!(chrome_layout.tab_strokes.len(), self.tabs.len());
        let control_button_size = if compact { 18.0 } else { TABBAR_NEW_TAB_BUTTON_SIZE };
        let control_icon_size = if compact { 11.0 } else { TABBAR_NEW_TAB_ICON_SIZE };
        let new_tab_button = self.render_tab_strip_control_button(
            "vertical-tabs-new-tab",
            "+",
            TabStripControlAction::NewTab,
            palette.tabbar_new_tab_bg,
            palette.tabbar_new_tab_hover_bg,
            palette.tabbar_new_tab_border,
            palette.tabbar_new_tab_hover_border,
            palette.tabbar_new_tab_text,
            palette.tabbar_new_tab_hover_text,
            control_button_size,
            control_icon_size,
            cx,
        );
        let collapse_icon = if compact { "›" } else { "‹" };
        let collapse_button = self.render_tab_strip_control_button(
            "vertical-tabs-collapse",
            collapse_icon,
            TabStripControlAction::ToggleVerticalSidebar,
            palette.tabbar_new_tab_bg,
            palette.tabbar_new_tab_hover_bg,
            palette.tabbar_new_tab_border,
            palette.tabbar_new_tab_hover_border,
            palette.tabbar_new_tab_text,
            palette.tabbar_new_tab_hover_text,
            control_button_size,
            if compact { 12.0 } else { 14.0 },
            cx,
        );
        let utility_dock = self.render_vertical_utility_dock(
            compact,
            utility_dock_height,
            strip_width,
            chrome_layout.divider_x,
            collapse_button,
            new_tab_button,
            &palette,
        );

        let mut list = div()
            .id("vertical-tabs-list")
            .relative()
            .w_full()
            .flex()
            .flex_col()
            .gap(px(TAB_ITEM_GAP))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                this.on_tabs_content_mouse_move(TabStripOrientation::Vertical, event, window, cx);
            }));

        for index in 0..self.tabs.len() {
            let tab_title = self.tabs[index].title.clone();
            let anim_progress = new_tab_anim
                .filter(|(anim_index, _)| *anim_index == index)
                .map(|(_, progress)| progress);
            let tab_height = tab_heights[index];
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
        for element in
            Self::render_stroke_segments(&chrome_layout.content_divider_strokes, palette.tab_stroke_color)
        {
            list = list.child(element);
        }

        div()
            .id("vertical-tab-strip")
            .relative()
            .flex_none()
            .w(px(strip_width))
            .h_full()
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
                    .w_full()
                    .h_full()
                    .flex()
                    .flex_col()
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
                    .child(utility_dock),
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
                )
            }))
            .into_any_element()
    }
}
