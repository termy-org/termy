use super::super::*;
use super::chrome;
use super::layout::TabStripGeometry;
use super::state::{TabStripOrientation, TabStripOverflowState};

pub(super) struct TabStripRenderState {
    pub(super) geometry: TabStripGeometry,
    pub(super) content_width: f32,
    pub(super) overflow_state: TabStripOverflowState,
    pub(super) chrome_layout: chrome::TabChromeLayout,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct DividerCollisionState {
    pub(super) left: bool,
    pub(super) right: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_strip_chrome_visible_matches_auto_hide_policy() {
        assert!(!TerminalView::tab_strip_chrome_visible(true, 1));
        assert!(!TerminalView::tab_strip_chrome_visible(true, 0));
        assert!(TerminalView::tab_strip_chrome_visible(false, 1));
        assert!(TerminalView::tab_strip_chrome_visible(true, 2));
    }

    #[test]
    fn hidden_titlebar_branding_shows_when_auto_hide_hides_single_tab_chrome() {
        assert!(TerminalView::should_render_hidden_titlebar_branding(
            true, 1, true
        ));
    }

    #[test]
    fn hidden_titlebar_branding_shows_when_auto_hide_hides_empty_tab_chrome() {
        assert!(TerminalView::should_render_hidden_titlebar_branding(
            true, 0, true
        ));
    }

    #[test]
    fn hidden_titlebar_branding_hides_when_branding_is_disabled() {
        assert!(!TerminalView::should_render_hidden_titlebar_branding(
            true, 1, false
        ));
    }

    #[test]
    fn hidden_titlebar_branding_hides_when_tab_strip_chrome_is_visible() {
        assert!(!TerminalView::should_render_hidden_titlebar_branding(
            false, 1, true
        ));
    }
}

impl TerminalView {
    pub(super) fn edge_divider_collision_state(
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

    pub(super) fn compact_vertical_tab_label(index: usize, title: &str) -> String {
        if index < 9 {
            return (index + 1).to_string();
        }

        title.chars().next().unwrap_or('•').to_string()
    }

    pub(crate) fn tab_strip_chrome_visible(auto_hide_tabbar: bool, tab_count: usize) -> bool {
        !auto_hide_tabbar || tab_count > 1
    }

    pub(crate) fn should_render_hidden_titlebar_branding(
        auto_hide_tabbar: bool,
        tab_count: usize,
        show_termy_in_titlebar: bool,
    ) -> bool {
        !Self::tab_strip_chrome_visible(auto_hide_tabbar, tab_count) && show_termy_in_titlebar
    }

    pub(crate) fn should_render_tab_strip_chrome(&self) -> bool {
        Self::tab_strip_chrome_visible(self.auto_hide_tabbar, self.tabs.len())
    }

    pub(super) fn build_tab_strip_render_state(
        &mut self,
        window: &Window,
        left_inset_width: f32,
    ) -> TabStripRenderState {
        let viewport_width: f32 = window.viewport_size().width.into();
        let layout =
            Self::tab_strip_layout_for_viewport_with_left_inset(viewport_width, left_inset_width);
        self.set_tab_strip_layout_snapshot(layout);

        let geometry = layout.geometry;
        let tab_strip_viewport_width = geometry.tabs_viewport_width;
        let _ = self.sync_tab_display_widths_for_viewport_if_needed(tab_strip_viewport_width);
        self.scroll_active_tab_into_view(TabStripOrientation::Horizontal);
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

    pub(super) fn render_tab_stroke(stroke: chrome::StrokeRect, color: gpui::Rgba) -> AnyElement {
        div()
            .absolute()
            .left(px(stroke.x))
            .top(px(stroke.y))
            .w(px(stroke.w))
            .h(px(stroke.h))
            .bg(color)
            .into_any_element()
    }

    pub(super) fn render_baseline_segments(
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

    pub(super) fn render_stroke_segments(
        strokes: &[chrome::StrokeRect],
        tab_stroke_color: gpui::Rgba,
    ) -> Vec<AnyElement> {
        strokes
            .iter()
            .copied()
            .map(|segment| Self::render_tab_stroke(segment, tab_stroke_color))
            .collect()
    }
}
