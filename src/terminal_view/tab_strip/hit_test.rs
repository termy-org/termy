use super::super::*;
use super::layout::{TabStripGeometry, VerticalTabStripLayoutSnapshot};
use super::state::TabStripOrientation;

impl TerminalView {
    pub(crate) fn unified_titlebar_top_chrome_interactive_hit_test(
        &self,
        x: f32,
        y: f32,
        window: &Window,
    ) -> bool {
        let geometry = self.tab_strip_geometry(window);
        let scroll_offset_x: f32 = self.tab_strip.horizontal_scroll_handle.offset().x.into();
        Self::unified_titlebar_top_chrome_interactive_hit_test_for_geometry(
            x,
            y,
            self.should_render_tab_strip_chrome(),
            geometry,
            self.tabs.iter().map(|tab| tab.display_width),
            scroll_offset_x,
        )
    }

    pub(crate) fn unified_titlebar_tab_shell_hit_test(
        pointer_x: f32,
        pointer_y: f32,
        tab_widths: impl IntoIterator<Item = f32>,
        scroll_offset_x: f32,
    ) -> bool {
        let tab_top = TOP_STRIP_CONTENT_OFFSET_Y + (TABBAR_HEIGHT - TAB_ITEM_HEIGHT);
        let tab_bottom = TOP_STRIP_CONTENT_OFFSET_Y + TABBAR_HEIGHT;
        if pointer_y < tab_top || pointer_y >= tab_bottom {
            return false;
        }

        let mut left = TAB_HORIZONTAL_PADDING + scroll_offset_x;
        for width in tab_widths {
            let right = left + width;
            if pointer_x >= left && pointer_x < right {
                return true;
            }
            left = right + TAB_ITEM_GAP;
        }

        false
    }

    pub(crate) fn unified_titlebar_top_chrome_interactive_hit_test_for_geometry(
        x: f32,
        y: f32,
        show_tab_strip_chrome: bool,
        geometry: TabStripGeometry,
        tab_widths: impl IntoIterator<Item = f32>,
        scroll_offset_x: f32,
    ) -> bool {
        // Hidden-titlebar states still render top padding/branding, but no tab-strip controls.
        // Treat that entire surface as noninteractive so it can arm window dragging instead of
        // reusing invisible tab geometry from the collapsed strip layout.
        if !show_tab_strip_chrome {
            return false;
        }

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

    pub(crate) fn top_chrome_interactive_hit_test(
        &self,
        orientation: TabStripOrientation,
        x: f32,
        y: f32,
        window: &Window,
    ) -> bool {
        match orientation {
            TabStripOrientation::Horizontal => {
                self.unified_titlebar_top_chrome_interactive_hit_test(x, y, window)
            }
            TabStripOrientation::Vertical => {
                self.vertical_tabs
                    && self.should_render_tab_strip_chrome()
                    && self.vertical_tab_strip_interactive_hit_test(x, y)
            }
        }
    }

    pub(crate) fn vertical_tab_strip_interactive_hit_test(&self, x: f32, y: f32) -> bool {
        let local_y = y - self.vertical_tab_strip_top_inset();
        if local_y < 0.0 {
            return false;
        }

        let layout = self.vertical_tab_strip_layout_snapshot();
        let scroll_offset_y: f32 = self.tab_strip.vertical_scroll_handle.offset().y.into();
        Self::vertical_tab_strip_interactive_hit_test_for_layout(
            x,
            local_y,
            &layout,
            scroll_offset_y,
        )
    }

    pub(super) fn vertical_tab_strip_interactive_hit_test_for_layout(
        x: f32,
        y: f32,
        layout: &VerticalTabStripLayoutSnapshot,
        scroll_offset_y: f32,
    ) -> bool {
        layout.interactive_hit(x, y, scroll_offset_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal_view::tab_strip::layout::VerticalTabStripLayoutInput;

    fn tab_hit_test_y() -> f32 {
        TOP_STRIP_CONTENT_OFFSET_Y + TABBAR_HEIGHT - 1.0
    }

    fn vertical_hit_test_layout(strip_width: f32, compact: bool) -> VerticalTabStripLayoutSnapshot {
        TerminalView::vertical_tab_strip_layout_for_input(VerticalTabStripLayoutInput {
            strip_width,
            compact,
            header_height: TABBAR_HEIGHT,
            list_height: 180.0,
            tab_heights: vec![TAB_ITEM_HEIGHT, TAB_ITEM_HEIGHT],
        })
    }

    #[test]
    fn shell_hit_test_detects_tabs_and_respects_y_bounds() {
        let widths = [100.0, 120.0];
        let scroll_offset_x = 0.0;
        let tab_top = TOP_STRIP_CONTENT_OFFSET_Y + (TABBAR_HEIGHT - TAB_ITEM_HEIGHT);
        let tab_bottom = TOP_STRIP_CONTENT_OFFSET_Y + TABBAR_HEIGHT;
        let tab_y = tab_bottom - 1.0;
        let first_tab_left = TAB_HORIZONTAL_PADDING;
        let first_tab_right = first_tab_left + widths[0];
        let second_tab_left = first_tab_right + TAB_ITEM_GAP;

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
        assert!(TerminalView::unified_titlebar_tab_shell_hit_test(
            TAB_HORIZONTAL_PADDING + 20.0,
            tab_top,
            widths,
            scroll_offset_x
        ));
        assert!(!TerminalView::unified_titlebar_tab_shell_hit_test(
            TAB_HORIZONTAL_PADDING + 20.0,
            tab_bottom,
            widths,
            scroll_offset_x
        ));
        assert!(!TerminalView::unified_titlebar_tab_shell_hit_test(
            first_tab_right,
            tab_y,
            [widths[0]],
            scroll_offset_x
        ));
        assert!(TerminalView::unified_titlebar_tab_shell_hit_test(
            second_tab_left,
            tab_y,
            widths,
            scroll_offset_x
        ));
    }

    #[test]
    fn interactive_hit_test_detects_tab_shell() {
        let geometry = TerminalView::tab_strip_geometry_for_viewport_width(1280.0);
        let x = geometry.row_start_x + TAB_HORIZONTAL_PADDING + 12.0;
        assert!(
            TerminalView::unified_titlebar_top_chrome_interactive_hit_test_for_geometry(
                x,
                tab_hit_test_y(),
                true,
                geometry,
                [120.0, 120.0],
                0.0,
            )
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn interactive_hit_test_detects_new_tab_button() {
        let geometry = TerminalView::tab_strip_geometry_for_viewport_width(1280.0);
        let center_x = (geometry.button_start_x + geometry.button_end_x) * 0.5;
        let center_y = (geometry.button_start_y + geometry.button_end_y) * 0.5;
        assert!(
            TerminalView::unified_titlebar_top_chrome_interactive_hit_test_for_geometry(
                center_x,
                center_y,
                true,
                geometry,
                [120.0, 120.0],
                0.0,
            )
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn interactive_hit_test_excludes_action_rail_empty_space() {
        let geometry = TerminalView::tab_strip_geometry_for_viewport_width(1280.0);
        let x = geometry.action_rail_start_x + 1.0;
        let y = (geometry.button_start_y + geometry.button_end_y) * 0.5;
        assert!(!geometry.new_tab_button_contains(x, y));
        assert!(geometry.contains_action_rail_x(x));
        assert!(
            !TerminalView::unified_titlebar_top_chrome_interactive_hit_test_for_geometry(
                x,
                y,
                true,
                geometry,
                [120.0, 120.0],
                0.0,
            )
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn interactive_hit_test_excludes_gutter() {
        let geometry = TerminalView::tab_strip_geometry_for_viewport_width(1280.0);
        let x = geometry.gutter_start_x + (geometry.gutter_width * 0.5);
        assert!(geometry.contains_gutter_x(x));
        assert!(
            !TerminalView::unified_titlebar_top_chrome_interactive_hit_test_for_geometry(
                x,
                tab_hit_test_y(),
                true,
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
            !TerminalView::unified_titlebar_top_chrome_interactive_hit_test_for_geometry(
                x,
                tab_hit_test_y(),
                true,
                geometry,
                [120.0, 120.0],
                0.0,
            )
        );
    }

    #[test]
    fn interactive_hit_test_excludes_right_inset_space() {
        let geometry = TerminalView::tab_strip_geometry_for_viewport_width(1280.0);
        let x = geometry.action_rail_end_x() + (geometry.right_inset_width * 0.5);
        assert!(
            !TerminalView::unified_titlebar_top_chrome_interactive_hit_test_for_geometry(
                x,
                tab_hit_test_y(),
                true,
                geometry,
                [120.0, 120.0],
                0.0,
            )
        );
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn interactive_hit_test_respects_half_open_region_boundaries() {
        let geometry = TerminalView::tab_strip_geometry_for_viewport_width(1280.0);
        let tabs_boundary = geometry.tabs_viewport_end_x();
        assert!(!geometry.contains_tabs_viewport_x(tabs_boundary));

        let action_start = geometry.gutter_end_x();
        assert!(geometry.contains_action_rail_x(action_start));
        assert!(
            !TerminalView::unified_titlebar_top_chrome_interactive_hit_test_for_geometry(
                action_start,
                tab_hit_test_y(),
                true,
                geometry,
                [120.0, 120.0],
                0.0,
            )
        );

        let action_end = geometry.action_rail_end_x();
        assert!(!geometry.contains_action_rail_x(action_end));
        assert!(
            !TerminalView::unified_titlebar_top_chrome_interactive_hit_test_for_geometry(
                action_end,
                tab_hit_test_y(),
                true,
                geometry,
                [120.0, 120.0],
                0.0,
            )
        );
    }

    #[test]
    fn hidden_horizontal_titlebar_ignores_invisible_tab_shells() {
        let geometry = TerminalView::tab_strip_geometry_for_viewport_width(1280.0);
        let x = geometry.row_start_x + TAB_HORIZONTAL_PADDING + 12.0;

        assert!(
            !TerminalView::unified_titlebar_top_chrome_interactive_hit_test_for_geometry(
                x,
                tab_hit_test_y(),
                false,
                geometry,
                [120.0],
                0.0,
            )
        );
    }

    #[test]
    fn hidden_vertical_titlebar_ignores_invisible_action_rail_controls() {
        let geometry = TerminalView::tab_strip_geometry_for_viewport_width(1280.0);
        let center_x = (geometry.button_start_x + geometry.button_end_x) * 0.5;
        let center_y = (geometry.button_start_y + geometry.button_end_y) * 0.5;

        assert!(
            !TerminalView::unified_titlebar_top_chrome_interactive_hit_test_for_geometry(
                center_x,
                center_y,
                false,
                geometry,
                [120.0],
                0.0,
            )
        );
    }

    #[test]
    fn vertical_interactive_hit_test_detects_tab_rows() {
        let strip_width = 220.0;
        let compact = false;
        let layout = vertical_hit_test_layout(strip_width, compact);

        assert!(
            TerminalView::vertical_tab_strip_interactive_hit_test_for_layout(
                24.0,
                layout.list_top + 12.0,
                &layout,
                0.0,
            )
        );
    }

    #[test]
    fn vertical_interactive_hit_test_detects_shelf_buttons_and_resize_handle() {
        let strip_width = 220.0;
        let compact = false;
        let layout = vertical_hit_test_layout(strip_width, compact);
        let top_button_x =
            layout.top_shelf_layout.button_x + (layout.top_shelf_layout.button_width * 0.5);
        let top_button_y = layout.header_height
            + layout.top_shelf_layout.button_y
            + (layout.top_shelf_layout.button_height * 0.5);
        assert!(
            TerminalView::vertical_tab_strip_interactive_hit_test_for_layout(
                top_button_x,
                top_button_y,
                &layout,
                0.0,
            )
        );
        assert!(
            TerminalView::vertical_tab_strip_interactive_hit_test_for_layout(
                layout.bottom_shelf_layout.button_x
                    + (layout.bottom_shelf_layout.button_size * 0.5),
                layout.bottom_shelf_top
                    + layout.bottom_shelf_layout.button_y
                    + (layout.bottom_shelf_layout.button_size * 0.5),
                &layout,
                0.0,
            )
        );
        assert!(
            TerminalView::vertical_tab_strip_interactive_hit_test_for_layout(
                strip_width - 1.0,
                24.0,
                &layout,
                0.0,
            )
        );
    }

    #[test]
    fn vertical_interactive_hit_test_excludes_noninteractive_chrome_backgrounds() {
        let strip_width = 220.0;
        let compact = false;
        let layout = vertical_hit_test_layout(strip_width, compact);
        let top_shelf_background_x = layout.top_shelf_layout.button_x * 0.5;
        let top_shelf_background_y = layout.header_height
            + layout.top_shelf_layout.button_y
            + (layout.top_shelf_layout.button_height * 0.5);

        assert!(
            !TerminalView::vertical_tab_strip_interactive_hit_test_for_layout(
                24.0, 12.0, &layout, 0.0,
            )
        );
        assert!(
            !TerminalView::vertical_tab_strip_interactive_hit_test_for_layout(
                top_shelf_background_x,
                top_shelf_background_y,
                &layout,
                0.0,
            )
        );
        assert!(
            !TerminalView::vertical_tab_strip_interactive_hit_test_for_layout(
                24.0,
                layout.list_top + TAB_ITEM_HEIGHT + 40.0,
                &layout,
                0.0,
            )
        );
    }
}
