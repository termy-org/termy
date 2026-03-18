use super::constants::*;

#[path = "chrome_horizontal.rs"]
mod horizontal;
#[cfg(test)]
#[path = "chrome_tests.rs"]
mod tests;
#[path = "chrome_vertical.rs"]
mod vertical;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct StrokeRect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

impl StrokeRect {
    pub(super) const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TabStrokeRects {
    pub(super) top: StrokeRect,
    pub(super) left_boundary: Option<StrokeRect>,
    pub(super) right_boundary: Option<StrokeRect>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct VerticalTabStrokeRects {
    pub(super) left: StrokeRect,
    pub(super) top_boundary: Option<StrokeRect>,
    pub(super) bottom_boundary: Option<StrokeRect>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(super) struct VerticalTailChrome {
    pub(super) draw_left_edge: bool,
    pub(super) draw_content_divider: bool,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TabChromeInput {
    pub(super) active_index: Option<usize>,
    pub(super) tabbar_height: f32,
    pub(super) tab_item_height: f32,
    pub(super) horizontal_padding: f32,
    pub(super) tab_item_gap: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct TabChromeLayout {
    pub(super) tab_strokes: Vec<TabStrokeRects>,
    pub(super) top_strokes: Vec<StrokeRect>,
    pub(super) boundary_strokes: Vec<StrokeRect>,
    pub(super) baseline_strokes: Vec<StrokeRect>,
    pub(super) content_width: f32,
    pub(super) tab_top_y: f32,
    pub(super) baseline_y: f32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct VerticalTabChromeInput {
    pub(super) active_index: Option<usize>,
    pub(super) strip_width: f32,
    pub(super) control_rail_height: f32,
    pub(super) tab_item_gap: f32,
    pub(super) external_top_seam: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct VerticalTabChromeLayout {
    pub(super) tab_strokes: Vec<VerticalTabStrokeRects>,
    pub(super) control_seam: Option<StrokeRect>,
    pub(super) content_divider_strokes: Vec<StrokeRect>,
    pub(super) tail: VerticalTailChrome,
    pub(super) content_height: f32,
    pub(super) divider_x: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct TabSpan {
    pub(super) left: f32,
    pub(super) right_exclusive: f32,
}

impl TabSpan {
    pub(super) fn width(self) -> f32 {
        self.right_exclusive - self.left
    }

    pub(super) fn right_edge(self) -> f32 {
        self.right_exclusive - TAB_STROKE_THICKNESS
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum BoundaryOwnerSide {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct VerticalTabSpan {
    pub(super) top: f32,
    pub(super) bottom_exclusive: f32,
}

impl VerticalTabSpan {
    pub(super) fn height(self) -> f32 {
        self.bottom_exclusive - self.top
    }

    pub(super) fn bottom_edge(self) -> f32 {
        self.bottom_exclusive - TAB_STROKE_THICKNESS
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HorizontalBoundaryOwnerSide {
    Top,
    Bottom,
}

pub(super) const PIXEL_EPSILON: f32 = 0.001;

pub(super) fn snap_px(value: f32) -> f32 {
    value.round()
}

pub(super) fn approximately_equal_px(lhs: f32, rhs: f32) -> bool {
    (lhs - rhs).abs() <= PIXEL_EPSILON
}

pub(super) fn inclusive_height(start_y: f32, end_y: f32) -> f32 {
    (end_y - start_y + TAB_STROKE_THICKNESS).max(0.0)
}

pub(super) fn inclusive_width(start_x: f32, end_x: f32) -> f32 {
    (end_x - start_x + TAB_STROKE_THICKNESS).max(0.0)
}

pub(super) fn resolve_tab_stroke_color(
    tabbar_background: gpui::Rgba,
    foreground: gpui::Rgba,
    foreground_mix: f32,
) -> gpui::Rgba {
    super::super::resolve_chrome_stroke_color(tabbar_background, foreground, foreground_mix)
}

pub(super) fn compute_tab_chrome_layout(
    tab_widths: impl IntoIterator<Item = f32>,
    input: TabChromeInput,
) -> TabChromeLayout {
    horizontal::compute_tab_chrome_layout(tab_widths, input)
}

pub(super) fn compute_vertical_tab_chrome_layout(
    tab_heights: impl IntoIterator<Item = f32>,
    input: VerticalTabChromeInput,
) -> VerticalTabChromeLayout {
    vertical::compute_vertical_tab_chrome_layout(tab_heights, input)
}
