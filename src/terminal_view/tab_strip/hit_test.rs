use super::super::*;
use super::layout::TabStripGeometry;

impl TerminalView {
    pub(crate) fn unified_titlebar_tab_shell_hit_test(
        pointer_x: f32,
        pointer_y: f32,
        tab_widths: impl IntoIterator<Item = f32>,
        scroll_offset_x: f32,
    ) -> bool {
        let tab_top = TOP_STRIP_CONTENT_OFFSET_Y + (TABBAR_HEIGHT - TAB_ITEM_HEIGHT);
        let tab_bottom = TOP_STRIP_CONTENT_OFFSET_Y + TABBAR_HEIGHT;
        if pointer_y < tab_top || pointer_y > tab_bottom {
            return false;
        }

        let mut left = TAB_HORIZONTAL_PADDING + scroll_offset_x;
        for width in tab_widths {
            let right = left + width;
            if pointer_x >= left && pointer_x <= right {
                return true;
            }
            left = right + TAB_ITEM_GAP;
        }

        false
    }

    pub(crate) fn unified_titlebar_tab_interactive_hit_test(
        &self,
        x: f32,
        y: f32,
        window: &Window,
    ) -> bool {
        let geometry = self.tab_strip_geometry(window);
        let scroll_offset_x: f32 = self.tab_strip.scroll_handle.offset().x.into();
        Self::unified_titlebar_tab_interactive_hit_test_for_geometry(
            x,
            y,
            geometry,
            self.tabs.iter().map(|tab| tab.display_width),
            scroll_offset_x,
        )
    }

    pub(crate) fn unified_titlebar_tab_interactive_hit_test_for_geometry(
        x: f32,
        y: f32,
        geometry: TabStripGeometry,
        tab_widths: impl IntoIterator<Item = f32>,
        scroll_offset_x: f32,
    ) -> bool {
        if geometry.contains_tabs_viewport_x(x) {
            let pointer_x = (x - geometry.row_start_x).clamp(0.0, geometry.tabs_viewport_width);
            if Self::unified_titlebar_tab_shell_hit_test(pointer_x, y, tab_widths, scroll_offset_x)
            {
                return true;
            }
        }

        if !geometry.contains_action_rail_x(x) {
            return false;
        }

        geometry.new_tab_button_contains(x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tab_hit_test_y() -> f32 {
        TOP_STRIP_CONTENT_OFFSET_Y + TABBAR_HEIGHT - 1.0
    }

    #[test]
    fn shell_hit_test_detects_tabs_and_respects_y_bounds() {
        let widths = [100.0, 120.0];
        let scroll_offset_x = 0.0;
        let tab_y = TOP_STRIP_CONTENT_OFFSET_Y + TABBAR_HEIGHT - 1.0;

        assert!(TerminalView::unified_titlebar_tab_shell_hit_test(
            TAB_HORIZONTAL_PADDING + 20.0,
            tab_y,
            widths,
            scroll_offset_x
        ));
        assert!(!TerminalView::unified_titlebar_tab_shell_hit_test(
            TAB_HORIZONTAL_PADDING + 240.0,
            tab_y,
            widths,
            scroll_offset_x
        ));
        assert!(!TerminalView::unified_titlebar_tab_shell_hit_test(
            TAB_HORIZONTAL_PADDING + 20.0,
            TOP_STRIP_CONTENT_OFFSET_Y,
            widths,
            scroll_offset_x
        ));
    }

    #[test]
    fn interactive_hit_test_detects_tab_shell() {
        let geometry = TerminalView::tab_strip_geometry_for_viewport_width(1280.0);
        let x = geometry.row_start_x + TAB_HORIZONTAL_PADDING + 12.0;
        assert!(
            TerminalView::unified_titlebar_tab_interactive_hit_test_for_geometry(
                x,
                tab_hit_test_y(),
                geometry,
                [120.0, 120.0],
                0.0,
            )
        );
    }

    #[test]
    fn interactive_hit_test_detects_new_tab_button() {
        let geometry = TerminalView::tab_strip_geometry_for_viewport_width(1280.0);
        let center_x = (geometry.button_start_x + geometry.button_end_x) * 0.5;
        let center_y = (geometry.button_start_y + geometry.button_end_y) * 0.5;
        assert!(
            TerminalView::unified_titlebar_tab_interactive_hit_test_for_geometry(
                center_x,
                center_y,
                geometry,
                [120.0, 120.0],
                0.0,
            )
        );
    }

    #[test]
    fn interactive_hit_test_excludes_action_rail_empty_space() {
        let geometry = TerminalView::tab_strip_geometry_for_viewport_width(1280.0);
        let x = geometry.action_rail_start_x + 1.0;
        let y = (geometry.button_start_y + geometry.button_end_y) * 0.5;
        assert!(!geometry.new_tab_button_contains(x, y));
        assert!(geometry.contains_action_rail_x(x));
        assert!(
            !TerminalView::unified_titlebar_tab_interactive_hit_test_for_geometry(
                x,
                y,
                geometry,
                [120.0, 120.0],
                0.0,
            )
        );
    }

    #[test]
    fn interactive_hit_test_excludes_gutter() {
        let geometry = TerminalView::tab_strip_geometry_for_viewport_width(1280.0);
        let x = geometry.gutter_start_x + (geometry.gutter_width * 0.5);
        assert!(geometry.contains_gutter_x(x));
        assert!(
            !TerminalView::unified_titlebar_tab_interactive_hit_test_for_geometry(
                x,
                tab_hit_test_y(),
                geometry,
                [120.0, 120.0],
                0.0,
            )
        );
    }

    #[test]
    fn interactive_hit_test_excludes_expanded_left_inset_branding_space() {
        let base_left_inset = TerminalView::titlebar_left_padding_for_platform();
        let geometry = TerminalView::tab_strip_geometry_for_viewport_with_left_inset(
            1280.0,
            base_left_inset + 64.0,
        );
        let x = geometry.left_inset_width - 1.0;
        assert!(
            !TerminalView::unified_titlebar_tab_interactive_hit_test_for_geometry(
                x,
                tab_hit_test_y(),
                geometry,
                [120.0, 120.0],
                0.0,
            )
        );
    }

    #[test]
    fn interactive_hit_test_respects_half_open_region_boundaries() {
        let geometry = TerminalView::tab_strip_geometry_for_viewport_width(1280.0);
        let tabs_boundary = geometry.tabs_viewport_end_x();
        assert!(!geometry.contains_tabs_viewport_x(tabs_boundary));

        let action_start = geometry.gutter_end_x();
        assert!(geometry.contains_action_rail_x(action_start));
        assert!(
            !TerminalView::unified_titlebar_tab_interactive_hit_test_for_geometry(
                action_start,
                tab_hit_test_y(),
                geometry,
                [120.0, 120.0],
                0.0,
            )
        );

        let action_end = geometry.action_rail_end_x();
        assert!(!geometry.contains_action_rail_x(action_end));
        assert!(
            !TerminalView::unified_titlebar_tab_interactive_hit_test_for_geometry(
                action_end,
                tab_hit_test_y(),
                geometry,
                [120.0, 120.0],
                0.0,
            )
        );
    }
}
