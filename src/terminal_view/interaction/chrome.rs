use super::*;
use crate::terminal_view::tab_strip::{
    clamp_expanded_vertical_tab_strip_width, collapsed_vertical_tab_strip_width,
};
use crate::terminal_view::tab_strip::state::TabStripOrientation;

impl TerminalView {
    pub(in super::super) fn tab_strip_orientation(&self) -> TabStripOrientation {
        if self.vertical_tabs {
            TabStripOrientation::Vertical
        } else {
            TabStripOrientation::Horizontal
        }
    }

    pub(in super::super) fn effective_vertical_tab_strip_width(&self) -> f32 {
        if !self.vertical_tabs {
            return 0.0;
        }

        if self.vertical_tabs_minimized {
            collapsed_vertical_tab_strip_width(Self::titlebar_left_padding_for_platform())
        } else {
            clamp_expanded_vertical_tab_strip_width(self.vertical_tabs_width)
        }
    }

    pub(in super::super) fn tab_strip_sidebar_width(&self) -> f32 {
        if self.vertical_tabs && self.should_render_tab_strip_chrome() {
            self.effective_vertical_tab_strip_width()
        } else {
            0.0
        }
    }

    pub(in super::super) fn vertical_tab_strip_header_height(&self) -> f32 {
        if self.vertical_tabs && self.should_render_tab_strip_chrome() {
            TABBAR_HEIGHT
        } else {
            0.0
        }
    }

    fn vertical_tab_strip_top_shelf_height(&self) -> f32 {
        if self.vertical_tabs && self.should_render_tab_strip_chrome() {
            VERTICAL_NEW_TAB_SHELF_HEIGHT
        } else {
            0.0
        }
    }

    fn vertical_tab_strip_bottom_control_slot_height(&self) -> f32 {
        if self.vertical_tabs && self.should_render_tab_strip_chrome() {
            VERTICAL_TITLEBAR_CONTROL_BUTTON_SIZE + (VERTICAL_TAB_STRIP_PADDING * 2.0)
        } else {
            0.0
        }
    }

    fn vertical_tabs_list_height_for(
        viewport_height: f32,
        chrome_height: f32,
        header_height: f32,
        top_shelf_height: f32,
        bottom_slot_height: f32,
    ) -> f32 {
        (viewport_height - chrome_height - header_height - top_shelf_height - bottom_slot_height)
            .max(0.0)
    }

    pub(in super::super) fn effective_vertical_tabs_list_height(&self) -> f32 {
        let header_height = self.vertical_tab_strip_header_height();
        let top_shelf_height = self.vertical_tab_strip_top_shelf_height();
        let bottom_slot_height = self.vertical_tab_strip_bottom_control_slot_height();
        let viewport_height = self.last_viewport_size_px.map_or(0.0, |(_, height)| height as f32);
        Self::vertical_tabs_list_height_for(
            viewport_height,
            self.chrome_height(),
            header_height,
            top_shelf_height,
            bottom_slot_height,
        )
    }

    pub(in super::super) fn terminal_content_position(
        &self,
        position: gpui::Point<Pixels>,
    ) -> (f32, f32) {
        let x: f32 = position.x.into();
        let y: f32 = position.y.into();
        (
            x - self.tab_strip_sidebar_width(),
            Self::window_y_to_terminal_content_y(y, self.chrome_height()),
        )
    }

    pub(in super::super) fn window_y_to_terminal_content_y(
        window_y: f32,
        chrome_height: f32,
    ) -> f32 {
        window_y - chrome_height
    }

    pub(in super::super) const fn titlebar_height() -> f32 {
        if TITLEBAR_HEIGHT > TABBAR_HEIGHT {
            TITLEBAR_HEIGHT
        } else {
            TABBAR_HEIGHT
        }
    }

    pub(in super::super) fn window_titlebar_height_for(
        vertical_tabs: bool,
        show_tab_strip_chrome: bool,
    ) -> f32 {
        if vertical_tabs && show_tab_strip_chrome {
            0.0
        } else {
            Self::titlebar_height()
        }
    }

    pub(in super::super) fn update_banner_height(&self) -> f32 {
        #[cfg(target_os = "macos")]
        if self.show_update_banner {
            return UPDATE_BANNER_HEIGHT;
        }
        0.0
    }

    pub(in super::super) fn chrome_height(&self) -> f32 {
        Self::window_titlebar_height_for(self.vertical_tabs, self.should_render_tab_strip_chrome())
            + self.update_banner_height()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_y_to_terminal_content_y_subtracts_non_zero_chrome() {
        assert_eq!(
            TerminalView::window_y_to_terminal_content_y(120.0, 34.0),
            86.0
        );
    }

    #[test]
    fn window_y_to_terminal_content_y_is_identity_when_chrome_is_zero() {
        assert_eq!(
            TerminalView::window_y_to_terminal_content_y(120.0, 0.0),
            120.0
        );
    }

    #[test]
    fn window_y_to_terminal_content_y_can_be_negative_when_cursor_is_above_chrome() {
        assert_eq!(TerminalView::window_y_to_terminal_content_y(20.0, 40.0), -20.0);
    }

    #[test]
    fn window_titlebar_height_keeps_horizontal_strip_height() {
        assert_eq!(TerminalView::window_titlebar_height_for(false, true), TerminalView::titlebar_height());
    }

    #[test]
    fn window_titlebar_height_drops_for_visible_vertical_sidebar() {
        assert_eq!(TerminalView::window_titlebar_height_for(true, true), 0.0);
    }

    #[test]
    fn window_titlebar_height_stays_when_vertical_sidebar_is_hidden() {
        assert_eq!(TerminalView::window_titlebar_height_for(true, false), TerminalView::titlebar_height());
    }

    #[test]
    fn collapsed_vertical_sidebar_width_covers_titlebar_left_inset() {
        assert!(
            collapsed_vertical_tab_strip_width(TerminalView::titlebar_left_padding_for_platform())
                >= TerminalView::titlebar_left_padding_for_platform()
        );
    }

    #[test]
    fn expanded_vertical_sidebar_width_clamps_to_reasonable_minimum() {
        assert_eq!(
            clamp_expanded_vertical_tab_strip_width(80.0),
            crate::terminal_view::tab_strip::min_expanded_vertical_tab_strip_width()
        );
    }

    #[test]
    fn expanded_vertical_sidebar_minimum_stays_above_collapsed_width() {
        assert!(
            crate::terminal_view::tab_strip::min_expanded_vertical_tab_strip_width()
                > collapsed_vertical_tab_strip_width(TerminalView::titlebar_left_padding_for_platform())
        );
    }

    #[test]
    fn vertical_bottom_shelf_height_matches_control_clearance() {
        assert_eq!(
            VERTICAL_TITLEBAR_CONTROL_BUTTON_SIZE + (VERTICAL_TAB_STRIP_PADDING * 2.0),
            38.0
        );
    }

    #[test]
    fn vertical_tabs_list_height_subtracts_header_top_shelf_and_bottom_shelf() {
        assert_eq!(
            TerminalView::vertical_tabs_list_height_for(600.0, 0.0, 34.0, 44.0, 38.0),
            484.0
        );
    }
}
