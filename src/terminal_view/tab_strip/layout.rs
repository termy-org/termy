use super::super::*;
use crate::terminal_view::tab_strip::state::TabStripOrientation;

pub(crate) const TAB_STRIP_RAIL_GUTTER_WIDTH: f32 = 2.0;
const TAB_STRIP_LAYOUT_EPSILON: f32 = 0.001;
#[cfg(target_os = "windows")]
const WINDOWS_CAPTION_BUTTONS_RESERVED_WIDTH: f32 = 140.0;

pub(crate) const fn min_expanded_vertical_tab_strip_width() -> f32 {
    VERTICAL_TAB_STRIP_MIN_WIDTH
}

pub(crate) fn clamp_expanded_vertical_tab_strip_width(width: f32) -> f32 {
    width.clamp(
        min_expanded_vertical_tab_strip_width(),
        VERTICAL_TAB_STRIP_MAX_WIDTH,
    )
}

pub(crate) fn collapsed_vertical_tab_strip_width(titlebar_left_padding: f32) -> f32 {
    VERTICAL_TAB_STRIP_COLLAPSED_WIDTH.max(titlebar_left_padding)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VerticalNewTabShelfLayout {
    pub(crate) shelf_height: f32,
    pub(crate) button_x: f32,
    pub(crate) button_y: f32,
    pub(crate) button_width: f32,
    pub(crate) button_height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VerticalBottomShelfLayout {
    pub(crate) shelf_height: f32,
    pub(crate) button_size: f32,
    pub(crate) icon_size: f32,
    pub(crate) button_x: f32,
    pub(crate) button_y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct VerticalTabRowLayout {
    pub(crate) index: usize,
    pub(crate) top: f32,
    pub(crate) height: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VerticalTabStripLayoutSnapshot {
    pub(crate) strip_width: f32,
    pub(crate) compact: bool,
    pub(crate) header_height: f32,
    pub(crate) top_shelf_layout: VerticalNewTabShelfLayout,
    pub(crate) bottom_shelf_layout: VerticalBottomShelfLayout,
    pub(crate) list_top: f32,
    pub(crate) list_height: f32,
    pub(crate) bottom_shelf_top: f32,
    pub(crate) divider_x: f32,
    pub(crate) resize_handle_left: f32,
    pub(crate) content_height: f32,
    pub(crate) rows: Vec<VerticalTabRowLayout>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VerticalTabStripLayoutInput {
    pub(crate) strip_width: f32,
    pub(crate) compact: bool,
    pub(crate) header_height: f32,
    pub(crate) list_height: f32,
    pub(crate) tab_heights: Vec<f32>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct TabStripGeometry {
    pub(crate) window_width: f32,
    pub(crate) left_inset_width: f32,
    pub(crate) right_inset_width: f32,
    pub(crate) row_start_x: f32,
    pub(crate) row_end_x: f32,
    pub(crate) row_width: f32,
    pub(crate) tabs_viewport_width: f32,
    pub(crate) gutter_start_x: f32,
    pub(crate) gutter_width: f32,
    pub(crate) action_rail_start_x: f32,
    pub(crate) action_rail_width: f32,
    pub(crate) button_start_x: f32,
    pub(crate) button_end_x: f32,
    pub(crate) button_start_y: f32,
    pub(crate) button_end_y: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct TabStripLayoutSnapshot {
    pub(crate) geometry: TabStripGeometry,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TabStripDragPreview {
    pub(crate) pointer_primary_axis: f32,
    pub(crate) viewport_extent: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TabStripLayoutInput {
    pub(crate) viewport_width: f32,
    pub(crate) left_inset_width: f32,
    pub(crate) content_width: Option<f32>,
}

impl TabStripGeometry {
    #[cfg(test)]
    pub(crate) fn left_inset_end_x(self) -> f32 {
        self.row_start_x
    }

    pub(crate) fn tabs_viewport_end_x(self) -> f32 {
        self.row_start_x + self.tabs_viewport_width
    }

    pub(crate) fn gutter_end_x(self) -> f32 {
        self.gutter_start_x + self.gutter_width
    }

    pub(crate) fn action_rail_end_x(self) -> f32 {
        self.action_rail_start_x + self.action_rail_width
    }

    #[cfg(test)]
    pub(crate) fn right_inset_start_x(self) -> f32 {
        self.row_end_x
    }

    pub(crate) fn contains_tabs_viewport_x(self, x: f32) -> bool {
        x >= self.row_start_x && x < self.tabs_viewport_end_x()
    }

    #[cfg(test)]
    pub(crate) fn contains_gutter_x(self, x: f32) -> bool {
        x >= self.gutter_start_x && x < self.gutter_end_x()
    }

    pub(crate) fn contains_action_rail_x(self, x: f32) -> bool {
        x >= self.action_rail_start_x && x < self.action_rail_end_x()
    }

    pub(crate) fn new_tab_button_contains(self, x: f32, y: f32) -> bool {
        x >= self.button_start_x
            && x < self.button_end_x
            && y >= self.button_start_y
            && y < self.button_end_y
    }
}

impl VerticalTabRowLayout {
    pub(crate) fn bottom(self) -> f32 {
        self.top + self.height
    }

    fn midpoint(self) -> f32 {
        self.top + (self.height * 0.5)
    }

    fn contains(self, y: f32) -> bool {
        y >= self.top && y < self.bottom()
    }
}

impl VerticalTabStripLayoutSnapshot {
    fn target_scroll_for_bounds(
        current_scroll: f32,
        viewport_extent: f32,
        item_start: f32,
        item_end: f32,
    ) -> f32 {
        let mut target_scroll = current_scroll;
        if item_end > current_scroll + viewport_extent {
            target_scroll = item_end - viewport_extent;
        } else if item_start < current_scroll {
            target_scroll = item_start;
        }
        target_scroll
    }

    fn point_hits_rect(x: f32, y: f32, left: f32, top: f32, width: f32, height: f32) -> bool {
        x >= left && x < left + width && y >= top && y < top + height
    }

    pub(crate) fn list_bottom(&self) -> f32 {
        self.list_top + self.list_height
    }

    pub(crate) fn list_pointer_y_from_window_y(&self, window_y: f32, chrome_height: f32) -> f32 {
        (window_y - chrome_height - self.list_top).clamp(0.0, self.list_height)
    }

    pub(crate) fn interactive_hit(&self, x: f32, y: f32, scroll_offset_y: f32) -> bool {
        if x < 0.0 || x >= self.strip_width || y < 0.0 {
            return false;
        }

        if !self.compact && x >= self.resize_handle_left {
            return true;
        }

        let top_shelf_top = self.header_height;
        if Self::point_hits_rect(
            x,
            y,
            self.top_shelf_layout.button_x,
            top_shelf_top + self.top_shelf_layout.button_y,
            self.top_shelf_layout.button_width,
            self.top_shelf_layout.button_height,
        ) {
            return true;
        }

        if y >= self.list_top && y < self.list_bottom() {
            return self
                .row_at_pointer(y - self.list_top, scroll_offset_y)
                .is_some();
        }

        Self::point_hits_rect(
            x,
            y,
            self.bottom_shelf_layout.button_x,
            self.bottom_shelf_top + self.bottom_shelf_layout.button_y,
            self.bottom_shelf_layout.button_size,
            self.bottom_shelf_layout.button_size,
        )
    }

    pub(crate) fn row_at_pointer(
        &self,
        list_pointer_y: f32,
        scroll_offset_y: f32,
    ) -> Option<usize> {
        let content_y = list_pointer_y - scroll_offset_y;
        self.rows
            .iter()
            .copied()
            .find(|row| row.contains(content_y))
            .map(|row| row.index)
    }

    pub(crate) fn drop_slot_for_pointer(&self, list_pointer_y: f32, scroll_offset_y: f32) -> usize {
        let content_y = list_pointer_y - scroll_offset_y;
        self.rows
            .iter()
            .copied()
            .find(|row| content_y < row.midpoint())
            .map_or(self.rows.len(), |row| row.index)
    }

    pub(crate) fn scroll_bounds(&self) -> (f32, f32) {
        (
            self.content_height,
            (self.content_height - self.list_height).max(0.0),
        )
    }

    pub(crate) fn scroll_target_for_active_row(
        &self,
        active_index: usize,
        current_scroll: f32,
    ) -> Option<f32> {
        let row = *self.rows.get(active_index)?;
        Some(Self::target_scroll_for_bounds(
            current_scroll,
            self.list_height,
            row.top,
            row.bottom(),
        ))
    }
}

impl TerminalView {
    #[cfg(target_os = "windows")]
    fn horizontal_action_rail_width_for_platform(_: f32) -> f32 {
        0.0
    }

    #[cfg(not(target_os = "windows"))]
    fn horizontal_action_rail_width_for_platform(max_row_width: f32) -> f32 {
        TABBAR_ACTION_RAIL_WIDTH.min(max_row_width)
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn titlebar_right_padding_for_platform() -> f32 {
        WINDOWS_CAPTION_BUTTONS_RESERVED_WIDTH
    }

    #[cfg(not(target_os = "windows"))]
    pub(crate) fn titlebar_right_padding_for_platform() -> f32 {
        TOP_STRIP_SIDE_PADDING
    }

    pub(crate) fn titlebar_left_padding_for_platform() -> f32 {
        if cfg!(target_os = "macos") {
            TOP_STRIP_MACOS_TRAFFIC_LIGHT_PADDING
        } else {
            TOP_STRIP_SIDE_PADDING
        }
    }

    pub(crate) fn tab_strip_layout_for_input(input: TabStripLayoutInput) -> TabStripLayoutSnapshot {
        let window_width = input.viewport_width.max(0.0);
        let left_inset_width = input.left_inset_width.max(0.0).min(window_width);
        let remaining_after_left = (window_width - left_inset_width).max(0.0);
        let reserved_right_inset_width =
            Self::titlebar_right_padding_for_platform().min(remaining_after_left);
        let max_row_width = (remaining_after_left - reserved_right_inset_width).max(0.0);
        let action_rail_width = Self::horizontal_action_rail_width_for_platform(max_row_width);
        let available_after_rail = (max_row_width - action_rail_width).max(0.0);
        let gutter_width = if action_rail_width > f32::EPSILON {
            TAB_STRIP_RAIL_GUTTER_WIDTH.min(available_after_rail)
        } else {
            0.0
        };
        let max_tabs_viewport_width = (max_row_width - action_rail_width - gutter_width).max(0.0);
        #[cfg(target_os = "windows")]
        let tabs_viewport_width = input
            .content_width
            .map(|width| width.max(0.0).min(max_tabs_viewport_width))
            .unwrap_or(max_tabs_viewport_width);
        #[cfg(not(target_os = "windows"))]
        let tabs_viewport_width = {
            let _ = input.content_width;
            max_tabs_viewport_width
        };
        let row_width = (tabs_viewport_width + gutter_width + action_rail_width)
            .min(remaining_after_left)
            .max(0.0);
        let right_inset_width = (remaining_after_left - row_width).max(0.0);
        let row_start_x = left_inset_width;
        let row_end_x = row_start_x + row_width;
        let gutter_start_x = row_start_x + tabs_viewport_width;
        let action_rail_start_x = gutter_start_x + gutter_width;
        let button_size = TABBAR_NEW_TAB_BUTTON_SIZE.min(action_rail_width);
        // Optical balance: center the button against the terminal edge lane (rail + trailing inset),
        // then clamp to keep the button fully inside the interactive action rail.
        let button_center_x =
            action_rail_start_x + (action_rail_width * 0.5) + (right_inset_width * 0.5);
        let max_button_start_x =
            (action_rail_start_x + action_rail_width - button_size).max(action_rail_start_x);
        let button_start_x =
            (button_center_x - (button_size * 0.5)).clamp(action_rail_start_x, max_button_start_x);
        let button_start_y =
            TOP_STRIP_CONTENT_OFFSET_Y + ((TABBAR_HEIGHT - button_size) * 0.5).max(0.0);
        let button_end_x = button_start_x + button_size;
        let button_end_y = button_start_y + button_size;

        let geometry = TabStripGeometry {
            window_width,
            left_inset_width,
            right_inset_width,
            row_start_x,
            row_end_x,
            row_width,
            tabs_viewport_width,
            gutter_start_x,
            gutter_width,
            action_rail_start_x,
            action_rail_width,
            button_start_x,
            button_end_x,
            button_start_y,
            button_end_y,
        };

        debug_assert!(
            (geometry.left_inset_width + geometry.row_width + geometry.right_inset_width
                - geometry.window_width)
                .abs()
                <= TAB_STRIP_LAYOUT_EPSILON
        );
        debug_assert!(
            geometry.tabs_viewport_end_x() <= geometry.gutter_start_x + TAB_STRIP_LAYOUT_EPSILON
        );
        debug_assert!(
            geometry.gutter_end_x() <= geometry.action_rail_start_x + TAB_STRIP_LAYOUT_EPSILON
        );
        debug_assert!(
            geometry.action_rail_end_x() <= geometry.row_end_x + TAB_STRIP_LAYOUT_EPSILON
        );
        debug_assert!(geometry.row_end_x <= geometry.window_width + TAB_STRIP_LAYOUT_EPSILON);
        debug_assert!(
            (geometry.row_end_x + geometry.right_inset_width - geometry.window_width).abs()
                <= TAB_STRIP_LAYOUT_EPSILON
        );
        debug_assert!(geometry.action_rail_start_x <= geometry.action_rail_end_x());
        debug_assert!(geometry.button_start_x >= geometry.action_rail_start_x);
        debug_assert!(geometry.button_end_x <= geometry.action_rail_end_x() + f32::EPSILON);

        TabStripLayoutSnapshot { geometry }
    }

    pub(crate) fn tab_strip_layout_for_viewport_width(
        viewport_width: f32,
    ) -> TabStripLayoutSnapshot {
        Self::tab_strip_layout_for_viewport_with_left_inset(
            viewport_width,
            Self::titlebar_left_padding_for_platform(),
        )
    }

    pub(crate) fn tab_strip_layout_for_viewport_with_left_inset(
        viewport_width: f32,
        left_inset_width: f32,
    ) -> TabStripLayoutSnapshot {
        Self::tab_strip_layout_for_input(TabStripLayoutInput {
            viewport_width,
            left_inset_width,
            content_width: None,
        })
    }

    pub(crate) fn tab_strip_layout_for_viewport_with_left_inset_and_content_width(
        viewport_width: f32,
        left_inset_width: f32,
        content_width: f32,
    ) -> TabStripLayoutSnapshot {
        Self::tab_strip_layout_for_input(TabStripLayoutInput {
            viewport_width,
            left_inset_width,
            content_width: Some(content_width),
        })
    }

    pub(crate) fn tab_strip_layout(&self, window: &Window) -> TabStripLayoutSnapshot {
        let viewport_width: f32 = window.viewport_size().width.into();
        Self::tab_strip_layout_for_viewport_width(viewport_width)
    }

    pub(crate) fn tab_strip_layout_snapshot(&self) -> Option<TabStripLayoutSnapshot> {
        self.tab_strip.horizontal_layout_snapshot
    }

    pub(crate) fn tab_strip_layout_snapshot_or_window(
        &self,
        window: &Window,
    ) -> TabStripLayoutSnapshot {
        self.tab_strip_layout_snapshot()
            .unwrap_or_else(|| self.tab_strip_layout(window))
    }

    pub(crate) fn set_tab_strip_layout_snapshot(&mut self, snapshot: TabStripLayoutSnapshot) {
        self.tab_strip.horizontal_layout_snapshot = Some(snapshot);
    }

    #[cfg(test)]
    pub(crate) fn tab_strip_geometry_for_viewport_width(viewport_width: f32) -> TabStripGeometry {
        Self::tab_strip_layout_for_viewport_width(viewport_width).geometry
    }

    #[cfg(test)]
    pub(crate) fn tab_strip_geometry_for_viewport_with_left_inset(
        viewport_width: f32,
        left_inset_width: f32,
    ) -> TabStripGeometry {
        Self::tab_strip_layout_for_viewport_with_left_inset(viewport_width, left_inset_width)
            .geometry
    }

    pub(crate) fn tab_strip_geometry(&self, window: &Window) -> TabStripGeometry {
        self.tab_strip_layout_snapshot_or_window(window).geometry
    }

    pub(crate) fn tab_strip_pointer_x_from_window_x_for_geometry(
        window_x: f32,
        geometry: TabStripGeometry,
    ) -> f32 {
        (window_x - geometry.row_start_x).clamp(0.0, geometry.tabs_viewport_width)
    }

    pub(crate) fn tab_strip_pointer_x_from_window_x(
        &self,
        window: &Window,
        window_x: Pixels,
    ) -> (f32, f32) {
        let geometry = self.tab_strip_geometry(window);
        let pointer_x =
            Self::tab_strip_pointer_x_from_window_x_for_geometry(window_x.into(), geometry);
        (pointer_x, geometry.tabs_viewport_width)
    }

    pub(crate) fn tab_strip_drag_preview(
        &self,
        orientation: TabStripOrientation,
        window: &Window,
        position: gpui::Point<Pixels>,
    ) -> TabStripDragPreview {
        match orientation {
            TabStripOrientation::Horizontal => {
                let (pointer_primary_axis, viewport_extent) =
                    self.tab_strip_pointer_x_from_window_x(window, position.x);
                TabStripDragPreview {
                    pointer_primary_axis,
                    viewport_extent,
                }
            }
            TabStripOrientation::Vertical => {
                let layout = self.vertical_tab_strip_layout_snapshot();
                TabStripDragPreview {
                    pointer_primary_axis: layout.list_pointer_y_from_window_y(
                        position.y.into(),
                        self.vertical_tab_strip_top_inset(),
                    ),
                    viewport_extent: layout.list_height,
                }
            }
        }
    }

    pub(super) fn vertical_new_tab_shelf_layout(
        divider_x: f32,
        compact: bool,
    ) -> VerticalNewTabShelfLayout {
        let shelf_height = if compact {
            VERTICAL_COMPACT_CONTROL_SHELF_HEIGHT
        } else {
            VERTICAL_NEW_TAB_SHELF_HEIGHT
        };
        let button_height = VERTICAL_NEW_TAB_SHELF_BUTTON_HEIGHT;
        let button_width = (divider_x - (VERTICAL_TAB_STRIP_PADDING * 2.0)).max(button_height);
        let button_x = VERTICAL_TAB_STRIP_PADDING;

        VerticalNewTabShelfLayout {
            shelf_height,
            button_x,
            button_y: ((shelf_height - button_height) * 0.5).max(0.0),
            button_width,
            button_height,
        }
    }

    pub(super) fn vertical_bottom_shelf_layout(strip_width: f32) -> VerticalBottomShelfLayout {
        let shelf_height = VERTICAL_COMPACT_CONTROL_SHELF_HEIGHT;
        let button_size = VERTICAL_TITLEBAR_CONTROL_BUTTON_SIZE;
        let divider_x = (strip_width - TAB_STROKE_THICKNESS).max(0.0);
        let button_x = (divider_x - VERTICAL_TAB_STRIP_PADDING - button_size).max(0.0);
        let button_y = ((shelf_height - button_size) * 0.5).max(0.0);

        VerticalBottomShelfLayout {
            shelf_height,
            button_size,
            icon_size: VERTICAL_TITLEBAR_CONTROL_ICON_SIZE,
            button_x,
            button_y,
        }
    }

    pub(crate) fn vertical_tab_strip_layout_for_input(
        input: VerticalTabStripLayoutInput,
    ) -> VerticalTabStripLayoutSnapshot {
        let strip_width = input.strip_width.max(0.0);
        let clamped_list_height = input.list_height.max(0.0);
        let top_shelf_layout =
            Self::vertical_new_tab_shelf_layout(strip_width - TAB_STROKE_THICKNESS, input.compact);
        let bottom_shelf_layout = Self::vertical_bottom_shelf_layout(strip_width);
        let list_top = input.header_height + top_shelf_layout.shelf_height;
        let bottom_shelf_top = list_top + clamped_list_height;
        let divider_x = (strip_width - TAB_STROKE_THICKNESS).max(0.0);
        let resize_handle_left = (strip_width - 4.0).max(0.0);
        let mut cursor_y = 0.0;
        let rows = input
            .tab_heights
            .into_iter()
            .enumerate()
            .map(|(index, height)| {
                let row = VerticalTabRowLayout {
                    index,
                    top: cursor_y,
                    height,
                };
                cursor_y = row.bottom() + TAB_ITEM_GAP;
                row
            })
            .collect::<Vec<_>>();
        let content_height = rows.last().map_or(0.0, |row| row.bottom());

        VerticalTabStripLayoutSnapshot {
            strip_width,
            compact: input.compact,
            header_height: input.header_height,
            top_shelf_layout,
            bottom_shelf_layout,
            list_top,
            list_height: clamped_list_height,
            bottom_shelf_top,
            divider_x,
            resize_handle_left,
            content_height,
            rows,
        }
    }

    fn compute_vertical_tab_strip_layout_snapshot(
        &self,
        now: Instant,
    ) -> VerticalTabStripLayoutSnapshot {
        let active_animation = self.new_tab_animation_progress(now);
        let tab_heights = (0..self.tabs.len())
            .map(|index| {
                let anim_progress = active_animation
                    .filter(|(anim_index, _)| *anim_index == index)
                    .map(|(_, progress)| progress)
                    .unwrap_or(1.0);
                TAB_ITEM_HEIGHT * anim_progress
            })
            .collect();
        Self::vertical_tab_strip_layout_for_input(VerticalTabStripLayoutInput {
            strip_width: self.effective_vertical_tab_strip_width(),
            compact: self.vertical_tabs_minimized,
            header_height: self.vertical_tab_strip_header_height(),
            list_height: self.effective_vertical_tabs_list_height(),
            tab_heights,
        })
    }

    pub(crate) fn vertical_tab_strip_layout_snapshot(&self) -> VerticalTabStripLayoutSnapshot {
        self.compute_vertical_tab_strip_layout_snapshot(Instant::now())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_float_eq(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.0001,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn viewport_and_rail_never_overlap() {
        let snapshot = TerminalView::tab_strip_layout_for_viewport_width(1280.0);
        let geometry = snapshot.geometry;
        assert!(geometry.tabs_viewport_end_x() <= geometry.gutter_start_x);
        assert!(geometry.gutter_end_x() <= geometry.action_rail_start_x);
        assert!(geometry.tabs_viewport_end_x() <= geometry.action_rail_start_x);
    }

    #[test]
    fn layout_contract_covers_full_window_width() {
        let snapshot = TerminalView::tab_strip_layout_for_viewport_width(1280.0);
        let geometry = snapshot.geometry;
        assert_float_eq(
            geometry.left_inset_width
                + geometry.tabs_viewport_width
                + geometry.gutter_width
                + geometry.action_rail_width
                + geometry.right_inset_width,
            geometry.window_width,
        );
    }

    #[test]
    fn lane_boundaries_are_continuous_without_overlap() {
        let snapshot = TerminalView::tab_strip_layout_for_viewport_width(1280.0);
        let geometry = snapshot.geometry;
        assert_float_eq(geometry.left_inset_end_x(), geometry.row_start_x);
        assert_float_eq(geometry.tabs_viewport_end_x(), geometry.gutter_start_x);
        assert_float_eq(geometry.gutter_end_x(), geometry.action_rail_start_x);
        assert_float_eq(geometry.action_rail_end_x(), geometry.row_end_x);
        assert_float_eq(geometry.right_inset_start_x(), geometry.row_end_x);
        assert_float_eq(
            geometry.row_end_x + geometry.right_inset_width,
            geometry.window_width,
        );
    }

    #[test]
    fn button_is_always_inside_rail() {
        let snapshot = TerminalView::tab_strip_layout_for_viewport_width(960.0);
        let geometry = snapshot.geometry;
        assert!(geometry.button_start_x >= geometry.action_rail_start_x);
        assert!(geometry.button_end_x <= geometry.action_rail_end_x());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn button_shrinks_to_narrow_action_rail() {
        let viewport_width =
            TerminalView::titlebar_left_padding_for_platform() + TOP_STRIP_SIDE_PADDING + 8.0;
        let snapshot = TerminalView::tab_strip_layout_for_viewport_width(viewport_width);
        let geometry = snapshot.geometry;
        assert_float_eq(geometry.action_rail_width, 8.0);
        assert_float_eq(
            geometry.button_end_x - geometry.button_start_x,
            geometry.action_rail_width,
        );
        assert!(geometry.button_start_x >= geometry.action_rail_start_x);
        assert!(geometry.button_end_x <= geometry.action_rail_end_x());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn action_rail_keeps_positive_width_in_standard_viewport() {
        let snapshot = TerminalView::tab_strip_layout_for_viewport_width(1280.0);
        assert!(snapshot.geometry.action_rail_width > 0.0);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn action_rail_is_hidden_on_windows() {
        let snapshot = TerminalView::tab_strip_layout_for_viewport_width(1280.0);
        assert_float_eq(snapshot.geometry.action_rail_width, 0.0);
        assert_float_eq(snapshot.geometry.gutter_width, 0.0);
    }

    #[test]
    fn button_stays_inside_window_bounds() {
        let snapshot = TerminalView::tab_strip_layout_for_viewport_width(1280.0);
        let geometry = snapshot.geometry;
        assert!(geometry.button_start_x >= geometry.action_rail_start_x);
        assert!(geometry.button_end_x <= geometry.action_rail_end_x());
        assert!(geometry.button_end_x <= geometry.window_width - geometry.right_inset_width);
    }

    #[test]
    fn gutter_divider_is_present_and_fixed() {
        let snapshot = TerminalView::tab_strip_layout_for_viewport_width(1280.0);
        let expected_width = if cfg!(target_os = "windows") {
            0.0
        } else {
            TAB_STRIP_RAIL_GUTTER_WIDTH
        };
        assert_float_eq(snapshot.geometry.gutter_width, expected_width);
    }

    #[test]
    fn baseline_segments_are_continuous_across_row_contract() {
        let snapshot = TerminalView::tab_strip_layout_for_viewport_width(1280.0);
        let geometry = snapshot.geometry;
        assert_float_eq(
            geometry.row_start_x + geometry.row_width,
            geometry.row_end_x,
        );
        assert_float_eq(geometry.action_rail_end_x(), geometry.row_end_x);
        assert_float_eq(
            geometry.tabs_viewport_width + geometry.gutter_width + geometry.action_rail_width,
            geometry.row_width,
        );
    }

    #[test]
    fn custom_left_inset_keeps_lane_contract_intact() {
        let snapshot = TerminalView::tab_strip_layout_for_viewport_with_left_inset(1280.0, 132.0);
        let geometry = snapshot.geometry;
        assert_float_eq(geometry.left_inset_width, 132.0);
        assert_float_eq(geometry.left_inset_end_x(), geometry.row_start_x);
        assert_float_eq(geometry.tabs_viewport_end_x(), geometry.gutter_start_x);
        assert_float_eq(geometry.gutter_end_x(), geometry.action_rail_start_x);
        assert_float_eq(geometry.action_rail_end_x(), geometry.row_end_x);
        assert_float_eq(
            geometry.row_end_x + geometry.right_inset_width,
            geometry.window_width,
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn fitting_windows_content_turns_extra_space_into_trailing_drag_region() {
        let left_inset_width = TerminalView::titlebar_left_padding_for_platform();
        let content_width = 192.0;
        let snapshot =
            TerminalView::tab_strip_layout_for_viewport_with_left_inset_and_content_width(
                1280.0,
                left_inset_width,
                content_width,
            );
        let geometry = snapshot.geometry;

        assert_float_eq(geometry.tabs_viewport_width, content_width);
        assert_float_eq(geometry.action_rail_width, 0.0);
        assert_float_eq(geometry.gutter_width, 0.0);
        assert!(
            geometry.right_inset_width > TerminalView::titlebar_right_padding_for_platform(),
            "expected leftover titlebar slack to move into the trailing drag region",
        );
        assert_float_eq(geometry.row_end_x, geometry.row_start_x + content_width);
    }

    #[test]
    fn vertical_layout_list_origin_includes_header_and_top_shelf() {
        let snapshot =
            TerminalView::vertical_tab_strip_layout_for_input(VerticalTabStripLayoutInput {
                strip_width: 220.0,
                compact: false,
                header_height: TABBAR_HEIGHT,
                list_height: 180.0,
                tab_heights: vec![TAB_ITEM_HEIGHT],
            });

        assert_float_eq(
            snapshot.list_top,
            TABBAR_HEIGHT + VERTICAL_NEW_TAB_SHELF_HEIGHT,
        );
        assert_float_eq(snapshot.bottom_shelf_top, snapshot.list_top + 180.0);
    }

    #[test]
    fn compact_vertical_layout_list_origin_uses_compact_top_shelf_height() {
        let snapshot =
            TerminalView::vertical_tab_strip_layout_for_input(VerticalTabStripLayoutInput {
                strip_width: collapsed_vertical_tab_strip_width(
                    TerminalView::titlebar_left_padding_for_platform(),
                ),
                compact: true,
                header_height: TABBAR_HEIGHT,
                list_height: 180.0,
                tab_heights: vec![TAB_ITEM_HEIGHT],
            });

        assert_float_eq(
            snapshot.list_top,
            TABBAR_HEIGHT + VERTICAL_COMPACT_CONTROL_SHELF_HEIGHT,
        );
        assert_float_eq(snapshot.bottom_shelf_top, snapshot.list_top + 180.0);
    }

    #[test]
    fn vertical_layout_clamps_negative_list_height_before_bottom_shelf_positioning() {
        let snapshot =
            TerminalView::vertical_tab_strip_layout_for_input(VerticalTabStripLayoutInput {
                strip_width: 220.0,
                compact: false,
                header_height: TABBAR_HEIGHT,
                list_height: -20.0,
                tab_heights: vec![TAB_ITEM_HEIGHT],
            });

        assert_float_eq(snapshot.list_height, 0.0);
        assert_float_eq(snapshot.bottom_shelf_top, snapshot.list_top);
    }

    #[test]
    fn expanded_and_compact_vertical_layouts_share_list_origin() {
        let expanded =
            TerminalView::vertical_tab_strip_layout_for_input(VerticalTabStripLayoutInput {
                strip_width: 220.0,
                compact: false,
                header_height: TABBAR_HEIGHT,
                list_height: 180.0,
                tab_heights: vec![TAB_ITEM_HEIGHT],
            });
        let compact =
            TerminalView::vertical_tab_strip_layout_for_input(VerticalTabStripLayoutInput {
                strip_width: collapsed_vertical_tab_strip_width(
                    TerminalView::titlebar_left_padding_for_platform(),
                ),
                compact: true,
                header_height: TABBAR_HEIGHT,
                list_height: 180.0,
                tab_heights: vec![TAB_ITEM_HEIGHT],
            });

        assert_float_eq(expanded.list_top, compact.list_top);
        assert_float_eq(expanded.bottom_shelf_top, compact.bottom_shelf_top);
    }

    #[test]
    fn vertical_layout_drop_slot_respects_animated_row_heights() {
        let snapshot =
            TerminalView::vertical_tab_strip_layout_for_input(VerticalTabStripLayoutInput {
                strip_width: 220.0,
                compact: false,
                header_height: TABBAR_HEIGHT,
                list_height: 180.0,
                tab_heights: vec![16.0, 32.0, 32.0],
            });

        assert_eq!(snapshot.drop_slot_for_pointer(7.0, 0.0), 0);
        assert_eq!(snapshot.drop_slot_for_pointer(9.0, 0.0), 1);
        assert_eq!(snapshot.drop_slot_for_pointer(40.0, 0.0), 2);
    }

    #[test]
    fn vertical_layout_row_hit_respects_scroll_offset() {
        let snapshot =
            TerminalView::vertical_tab_strip_layout_for_input(VerticalTabStripLayoutInput {
                strip_width: 220.0,
                compact: false,
                header_height: TABBAR_HEIGHT,
                list_height: 180.0,
                tab_heights: vec![32.0, 32.0],
            });

        assert_eq!(snapshot.row_at_pointer(10.0, 0.0), Some(0));
        assert_eq!(snapshot.row_at_pointer(10.0, -20.0), Some(0));
        assert_eq!(snapshot.row_at_pointer(25.0, -20.0), Some(1));
    }

    #[test]
    fn vertical_layout_scroll_target_uses_row_bounds() {
        let snapshot =
            TerminalView::vertical_tab_strip_layout_for_input(VerticalTabStripLayoutInput {
                strip_width: 220.0,
                compact: false,
                header_height: TABBAR_HEIGHT,
                list_height: 40.0,
                tab_heights: vec![16.0, 48.0],
            });

        assert_eq!(snapshot.scroll_target_for_active_row(1, 0.0), Some(24.0));
    }
}
