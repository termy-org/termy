use super::constants::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct StrokeRect {
    pub(super) x: f32,
    pub(super) y: f32,
    pub(super) w: f32,
    pub(super) h: f32,
}

impl StrokeRect {
    const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
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
struct TabSpan {
    left: f32,
    right_exclusive: f32,
}

impl TabSpan {
    fn width(self) -> f32 {
        self.right_exclusive - self.left
    }

    fn right_edge(self) -> f32 {
        self.right_exclusive - TAB_STROKE_THICKNESS
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoundaryOwnerSide {
    Left,
    Right,
}

const PIXEL_EPSILON: f32 = 0.001;

fn snap_px(value: f32) -> f32 {
    value.round()
}

fn approximately_equal_px(lhs: f32, rhs: f32) -> bool {
    (lhs - rhs).abs() <= PIXEL_EPSILON
}

fn inclusive_height(start_y: f32, end_y: f32) -> f32 {
    (end_y - start_y + TAB_STROKE_THICKNESS).max(0.0)
}

fn inclusive_width(start_x: f32, end_x: f32) -> f32 {
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
    let tab_top_y = snap_px(input.tabbar_height - input.tab_item_height);
    let baseline_y = snap_px(input.tabbar_height - TAB_STROKE_THICKNESS);
    let boundary_start_y = tab_top_y + TAB_STROKE_THICKNESS;
    let full_boundary_height = inclusive_height(boundary_start_y, baseline_y);
    let short_boundary_height =
        inclusive_height(boundary_start_y, baseline_y - TAB_STROKE_THICKNESS);
    let horizontal_padding = snap_px(input.horizontal_padding.max(0.0));
    let tab_item_gap = snap_px(input.tab_item_gap.max(0.0));

    let iter = tab_widths.into_iter();
    let (lower_bound, _) = iter.size_hint();
    let mut spans = Vec::with_capacity(lower_bound);
    let mut top_strokes = Vec::with_capacity(lower_bound);
    let mut tab_strokes = Vec::with_capacity(lower_bound);

    let mut cursor_x = horizontal_padding;
    for raw_width in iter {
        debug_assert!(raw_width.is_finite(), "tab width must be finite");
        let width = snap_px(raw_width.max(TAB_STROKE_THICKNESS));
        let span = TabSpan {
            left: cursor_x,
            right_exclusive: cursor_x + width,
        };
        spans.push(span);
        top_strokes.push(StrokeRect::new(
            span.left,
            tab_top_y,
            span.width(),
            TAB_STROKE_THICKNESS,
        ));
        tab_strokes.push(TabStrokeRects {
            top: StrokeRect::new(0.0, 0.0, span.width(), TAB_STROKE_THICKNESS),
            left_boundary: None,
            right_boundary: None,
        });
        cursor_x = span.right_exclusive + tab_item_gap;
    }

    let content_width = spans
        .last()
        .map_or(horizontal_padding, |span| snap_px(span.right_exclusive));

    let active_span = input
        .active_index
        .and_then(|active_index| spans.get(active_index).copied());
    if input.active_index.is_some() {
        debug_assert!(
            active_span.is_some(),
            "active tab index is out of bounds for tab chrome layout"
        );
    }

    let active_left_boundary_x = active_span.map(|span| span.left);
    let active_right_boundary_x = active_span.map(TabSpan::right_edge);
    let boundary_height_for_x = |x: f32| {
        let touches_active = active_left_boundary_x
            .is_some_and(|active_left| approximately_equal_px(active_left, x))
            || active_right_boundary_x
                .is_some_and(|active_right| approximately_equal_px(active_right, x));
        if touches_active {
            full_boundary_height
        } else {
            short_boundary_height
        }
    };

    let mut boundary_strokes = Vec::with_capacity(spans.len() + 1);
    let assign_boundary = |tab_strokes: &mut [TabStrokeRects],
                           spans: &[TabSpan],
                           tab_index: usize,
                           side: BoundaryOwnerSide,
                           global_rect: StrokeRect| {
        let tab_span = spans[tab_index];
        let local_x = match side {
            BoundaryOwnerSide::Left => 0.0,
            BoundaryOwnerSide::Right => tab_span.width() - TAB_STROKE_THICKNESS,
        };
        let local_rect = StrokeRect::new(
            local_x,
            TAB_STROKE_THICKNESS,
            TAB_STROKE_THICKNESS,
            global_rect.h,
        );
        match side {
            BoundaryOwnerSide::Left => tab_strokes[tab_index].left_boundary = Some(local_rect),
            BoundaryOwnerSide::Right => tab_strokes[tab_index].right_boundary = Some(local_rect),
        }
    };

    if let Some(first_span) = spans.first().copied() {
        let left_boundary_x = first_span.left;
        let left_boundary_rect = StrokeRect::new(
            left_boundary_x,
            boundary_start_y,
            TAB_STROKE_THICKNESS,
            boundary_height_for_x(left_boundary_x),
        );
        assign_boundary(
            &mut tab_strokes,
            &spans,
            0,
            BoundaryOwnerSide::Left,
            left_boundary_rect,
        );
        boundary_strokes.push(left_boundary_rect);
    }

    for divider_index in 0..spans.len().saturating_sub(1) {
        let owner = if input.active_index == Some(divider_index + 1) {
            (
                divider_index + 1,
                BoundaryOwnerSide::Left,
                spans[divider_index + 1].left,
            )
        } else {
            (
                divider_index,
                BoundaryOwnerSide::Right,
                spans[divider_index].right_edge(),
            )
        };

        let divider_rect = StrokeRect::new(
            owner.2,
            boundary_start_y,
            TAB_STROKE_THICKNESS,
            boundary_height_for_x(owner.2),
        );
        assign_boundary(&mut tab_strokes, &spans, owner.0, owner.1, divider_rect);
        boundary_strokes.push(divider_rect);
    }

    if let Some((last_index, last_span)) = spans
        .len()
        .checked_sub(1)
        .and_then(|index| spans.get(index).copied().map(|span| (index, span)))
    {
        let right_boundary_x = last_span.right_edge();
        let right_boundary_rect = StrokeRect::new(
            right_boundary_x,
            boundary_start_y,
            TAB_STROKE_THICKNESS,
            boundary_height_for_x(right_boundary_x),
        );
        assign_boundary(
            &mut tab_strokes,
            &spans,
            last_index,
            BoundaryOwnerSide::Right,
            right_boundary_rect,
        );
        boundary_strokes.push(right_boundary_rect);
    }

    let mut baseline_strokes = Vec::with_capacity(2);
    match active_span {
        Some(active_span) => {
            let left_width = active_span.left.clamp(0.0, content_width);
            if left_width > 0.0 {
                baseline_strokes.push(StrokeRect::new(
                    0.0,
                    baseline_y,
                    left_width,
                    TAB_STROKE_THICKNESS,
                ));
            }

            let right_start_x =
                (active_span.right_edge() + TAB_STROKE_THICKNESS).clamp(0.0, content_width);
            let right_width = (content_width - right_start_x).max(0.0);
            if right_width > 0.0 {
                baseline_strokes.push(StrokeRect::new(
                    right_start_x,
                    baseline_y,
                    right_width,
                    TAB_STROKE_THICKNESS,
                ));
            }
        }
        None => {
            if content_width > 0.0 {
                baseline_strokes.push(StrokeRect::new(
                    0.0,
                    baseline_y,
                    content_width,
                    TAB_STROKE_THICKNESS,
                ));
            }
        }
    }

    debug_assert_eq!(tab_strokes.len(), spans.len());
    debug_assert_eq!(top_strokes.len(), spans.len());

    TabChromeLayout {
        tab_strokes,
        top_strokes,
        boundary_strokes,
        baseline_strokes,
        content_width,
        tab_top_y,
        baseline_y,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct VerticalTabSpan {
    top: f32,
    bottom_exclusive: f32,
}

impl VerticalTabSpan {
    fn height(self) -> f32 {
        self.bottom_exclusive - self.top
    }

    fn bottom_edge(self) -> f32 {
        self.bottom_exclusive - TAB_STROKE_THICKNESS
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HorizontalBoundaryOwnerSide {
    Top,
    Bottom,
}

pub(super) fn compute_vertical_tab_chrome_layout(
    tab_heights: impl IntoIterator<Item = f32>,
    input: VerticalTabChromeInput,
) -> VerticalTabChromeLayout {
    let strip_width = snap_px(input.strip_width.max(TAB_STROKE_THICKNESS));
    let divider_x = snap_px((strip_width - TAB_STROKE_THICKNESS).max(0.0));
    let boundary_start_x = TAB_STROKE_THICKNESS;
    let full_boundary_width = inclusive_width(boundary_start_x, divider_x);
    let short_boundary_width = inclusive_width(boundary_start_x, divider_x - TAB_STROKE_THICKNESS);
    let tab_item_gap = snap_px(input.tab_item_gap.max(0.0));
    let control_seam = (input.control_rail_height > 0.0).then(|| {
        StrokeRect::new(
            0.0,
            snap_px(input.control_rail_height - TAB_STROKE_THICKNESS),
            strip_width,
            TAB_STROKE_THICKNESS,
        )
    });

    let iter = tab_heights.into_iter();
    let (lower_bound, _) = iter.size_hint();
    let mut spans = Vec::with_capacity(lower_bound);
    let mut tab_strokes = Vec::with_capacity(lower_bound);

    let mut cursor_y = 0.0;
    for raw_height in iter {
        debug_assert!(raw_height.is_finite(), "tab height must be finite");
        let height = snap_px(raw_height.max(TAB_STROKE_THICKNESS));
        let span = VerticalTabSpan {
            top: cursor_y,
            bottom_exclusive: cursor_y + height,
        };
        spans.push(span);
        tab_strokes.push(VerticalTabStrokeRects {
            left: StrokeRect::new(0.0, 0.0, TAB_STROKE_THICKNESS, span.height()),
            top_boundary: None,
            bottom_boundary: None,
        });
        cursor_y = span.bottom_exclusive + tab_item_gap;
    }

    let content_height = spans
        .last()
        .map_or(0.0, |span| snap_px(span.bottom_exclusive));
    let top_seam_offset_y = if input.external_top_seam {
        TAB_STROKE_THICKNESS
    } else {
        0.0
    };

    let active_span = input
        .active_index
        .and_then(|active_index| spans.get(active_index).copied());
    if input.active_index.is_some() {
        debug_assert!(
            active_span.is_some(),
            "active tab index is out of bounds for vertical chrome layout"
        );
    }

    let active_top_boundary_y = active_span.map(|span| span.top);
    let active_bottom_boundary_y = active_span.map(VerticalTabSpan::bottom_edge);
    let boundary_width_for_y = |y: f32| {
        let touches_active = active_top_boundary_y
            .is_some_and(|active_top| approximately_equal_px(active_top, y))
            || active_bottom_boundary_y
                .is_some_and(|active_bottom| approximately_equal_px(active_bottom, y));
        if touches_active {
            full_boundary_width
        } else {
            short_boundary_width
        }
    };

    let assign_boundary = |tab_strokes: &mut [VerticalTabStrokeRects],
                           spans: &[VerticalTabSpan],
                           tab_index: usize,
                           side: HorizontalBoundaryOwnerSide,
                           global_rect: StrokeRect| {
        let tab_span = spans[tab_index];
        let local_y = match side {
            HorizontalBoundaryOwnerSide::Top => 0.0,
            HorizontalBoundaryOwnerSide::Bottom => tab_span.height() - TAB_STROKE_THICKNESS,
        };
        let local_rect = StrokeRect::new(
            TAB_STROKE_THICKNESS,
            local_y,
            global_rect.w,
            TAB_STROKE_THICKNESS,
        );
        match side {
            HorizontalBoundaryOwnerSide::Top => tab_strokes[tab_index].top_boundary = Some(local_rect),
            HorizontalBoundaryOwnerSide::Bottom => {
                tab_strokes[tab_index].bottom_boundary = Some(local_rect)
            }
        }
    };

    if input.external_top_seam {
        if let Some(first_tab) = tab_strokes.first_mut() {
            // The titlebar block owns the titlebar/sidebar seam, so the list
            // starts one pixel lower to avoid double-drawing that corner.
            first_tab.left.y = top_seam_offset_y;
            first_tab.left.h = (first_tab.left.h - top_seam_offset_y).max(0.0);
        }

        if input.active_index == Some(0)
            && let Some(first_span) = spans.first().copied()
        {
            // Keep the active first tab visually closed at the top. Relying on
            // the external seam alone leaves the selected row looking open.
            let top_boundary_y = first_span.top;
            let top_boundary_rect = StrokeRect::new(
                boundary_start_x,
                top_boundary_y,
                boundary_width_for_y(top_boundary_y),
                TAB_STROKE_THICKNESS,
            );
            assign_boundary(
                &mut tab_strokes,
                &spans,
                0,
                HorizontalBoundaryOwnerSide::Top,
                top_boundary_rect,
            );
        }
    } else if control_seam.is_none() {
        if let Some(first_span) = spans.first().copied() {
            let top_boundary_y = first_span.top;
            let top_boundary_rect = StrokeRect::new(
                boundary_start_x,
                top_boundary_y,
                boundary_width_for_y(top_boundary_y),
                TAB_STROKE_THICKNESS,
            );
            assign_boundary(
                &mut tab_strokes,
                &spans,
                0,
                HorizontalBoundaryOwnerSide::Top,
                top_boundary_rect,
            );
        }
    }

    for divider_index in 0..spans.len().saturating_sub(1) {
        let owner = if input.active_index == Some(divider_index + 1) {
            (
                divider_index + 1,
                HorizontalBoundaryOwnerSide::Top,
                spans[divider_index + 1].top,
            )
        } else {
            (
                divider_index,
                HorizontalBoundaryOwnerSide::Bottom,
                spans[divider_index].bottom_edge(),
            )
        };

        let divider_rect = StrokeRect::new(
            boundary_start_x,
            owner.2,
            boundary_width_for_y(owner.2),
            TAB_STROKE_THICKNESS,
        );
        assign_boundary(&mut tab_strokes, &spans, owner.0, owner.1, divider_rect);
    }

    if let Some((last_index, last_span)) = spans
        .len()
        .checked_sub(1)
        .and_then(|index| spans.get(index).copied().map(|span| (index, span)))
    {
        let bottom_boundary_y = last_span.bottom_edge();
        let bottom_boundary_rect = StrokeRect::new(
            boundary_start_x,
            bottom_boundary_y,
            boundary_width_for_y(bottom_boundary_y),
            TAB_STROKE_THICKNESS,
        );
        assign_boundary(
            &mut tab_strokes,
            &spans,
            last_index,
            HorizontalBoundaryOwnerSide::Bottom,
            bottom_boundary_rect,
        );
    }

    let mut content_divider_strokes = Vec::with_capacity(2);
    match active_span {
        Some(active_span) => {
            let top_height = (active_span.top - top_seam_offset_y).clamp(0.0, content_height);
            if top_height > 0.0 {
                content_divider_strokes.push(StrokeRect::new(
                    divider_x,
                    top_seam_offset_y,
                    TAB_STROKE_THICKNESS,
                    top_height,
                ));
            }

            let bottom_start_y =
                (active_span.bottom_edge() + TAB_STROKE_THICKNESS).clamp(0.0, content_height);
            let bottom_height = (content_height - bottom_start_y).max(0.0);
            if bottom_height > 0.0 {
                content_divider_strokes.push(StrokeRect::new(
                    divider_x,
                    bottom_start_y,
                    TAB_STROKE_THICKNESS,
                    bottom_height,
                ));
            }
        }
        None => {
            if content_height > top_seam_offset_y {
                content_divider_strokes.push(StrokeRect::new(
                    divider_x,
                    top_seam_offset_y,
                    TAB_STROKE_THICKNESS,
                    content_height - top_seam_offset_y,
                ));
            }
        }
    }

    let tail = VerticalTailChrome {
        draw_left_edge: strip_width > 0.0,
        draw_content_divider: strip_width > 0.0,
    };

    VerticalTabChromeLayout {
        tab_strokes,
        control_seam,
        content_divider_strokes,
        tail,
        content_height,
        divider_x,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};

    fn layout_for(widths: &[f32], active_index: Option<usize>) -> TabChromeLayout {
        compute_tab_chrome_layout(
            widths.iter().copied(),
            TabChromeInput {
                active_index,
                tabbar_height: TABBAR_HEIGHT,
                tab_item_height: TAB_ITEM_HEIGHT,
                horizontal_padding: TAB_HORIZONTAL_PADDING,
                tab_item_gap: TAB_ITEM_GAP,
            },
        )
    }

    fn all_global_strokes(layout: &TabChromeLayout) -> impl Iterator<Item = StrokeRect> + '_ {
        layout
            .top_strokes
            .iter()
            .copied()
            .chain(layout.boundary_strokes.iter().copied())
            .chain(layout.baseline_strokes.iter().copied())
    }

    fn vertical_layout_for(
        heights: &[f32],
        active_index: Option<usize>,
        strip_width: f32,
    ) -> VerticalTabChromeLayout {
        vertical_layout_for_with_input(heights, active_index, strip_width, TABBAR_HEIGHT, false)
    }

    fn vertical_layout_for_with_input(
        heights: &[f32],
        active_index: Option<usize>,
        strip_width: f32,
        control_rail_height: f32,
        external_top_seam: bool,
    ) -> VerticalTabChromeLayout {
        compute_vertical_tab_chrome_layout(
            heights.iter().copied(),
            VerticalTabChromeInput {
                active_index,
                strip_width,
                control_rail_height,
                tab_item_gap: TAB_ITEM_GAP,
                external_top_seam,
            },
        )
    }

    fn vertical_coverage_map(
        layout: &VerticalTabChromeLayout,
        heights: &[f32],
        tail_height: f32,
    ) -> HashMap<(i32, i32), usize> {
        let mut coverage = HashMap::new();

        let mut push_rect = |rect: StrokeRect| {
            let x_start = rect.x as i32;
            let y_start = rect.y as i32;
            let x_end = (rect.x + rect.w) as i32;
            let y_end = (rect.y + rect.h) as i32;
            for x in x_start..x_end {
                for y in y_start..y_end {
                    *coverage.entry((x, y)).or_insert(0) += 1;
                }
            }
        };

        if let Some(control_seam) = layout.control_seam {
            push_rect(control_seam);
        }

        let list_origin_y = layout.control_seam.map_or(0.0, |stroke| stroke.y + stroke.h);

        for stroke in &layout.content_divider_strokes {
            push_rect(StrokeRect::new(
                stroke.x,
                list_origin_y + stroke.y,
                stroke.w,
                stroke.h,
            ));
        }

        let mut cursor_y = 0.0;
        for (index, height) in heights.iter().copied().enumerate() {
            let strokes = layout.tab_strokes[index];
            push_rect(StrokeRect::new(
                strokes.left.x,
                list_origin_y + cursor_y + strokes.left.y,
                strokes.left.w,
                strokes.left.h,
            ));
            if let Some(top) = strokes.top_boundary {
                push_rect(StrokeRect::new(
                    top.x,
                    list_origin_y + cursor_y + top.y,
                    top.w,
                    top.h,
                ));
            }
            if let Some(bottom) = strokes.bottom_boundary {
                push_rect(StrokeRect::new(
                    bottom.x,
                    list_origin_y + cursor_y + bottom.y,
                    bottom.w,
                    bottom.h,
                ));
            }
            cursor_y += height + TAB_ITEM_GAP;
        }

        if layout.tail.draw_left_edge && tail_height > 0.0 {
            push_rect(StrokeRect::new(
                0.0,
                list_origin_y + layout.content_height,
                TAB_STROKE_THICKNESS,
                tail_height,
            ));
        }
        if layout.tail.draw_content_divider && tail_height > 0.0 {
            push_rect(StrokeRect::new(
                layout.divider_x,
                list_origin_y + layout.content_height,
                TAB_STROKE_THICKNESS,
                tail_height,
            ));
        }

        coverage
    }

    fn coverage_map(layout: &TabChromeLayout) -> HashMap<(i32, i32), usize> {
        let mut coverage = HashMap::new();
        for rect in all_global_strokes(layout) {
            let x_start = rect.x as i32;
            let y_start = rect.y as i32;
            let x_end = (rect.x + rect.w) as i32;
            let y_end = (rect.y + rect.h) as i32;
            for x in x_start..x_end {
                for y in y_start..y_end {
                    *coverage.entry((x, y)).or_insert(0) += 1;
                }
            }
        }
        coverage
    }

    fn boundary_at_x(layout: &TabChromeLayout, x: i32) -> StrokeRect {
        layout
            .boundary_strokes
            .iter()
            .copied()
            .find(|stroke| stroke.x as i32 == x)
            .expect("expected boundary stroke at x")
    }

    fn tab_left_x(layout: &TabChromeLayout, tab_index: usize) -> i32 {
        layout.top_strokes[tab_index].x as i32
    }

    fn tab_right_boundary_x(layout: &TabChromeLayout, tab_index: usize) -> i32 {
        (layout.top_strokes[tab_index].x + layout.top_strokes[tab_index].w - TAB_STROKE_THICKNESS)
            as i32
    }

    fn baseline_pixels(layout: &TabChromeLayout) -> HashSet<i32> {
        layout
            .baseline_strokes
            .iter()
            .flat_map(|stroke| (stroke.x as i32)..((stroke.x + stroke.w) as i32))
            .collect()
    }

    #[test]
    fn active_middle_has_no_pixel_overlap() {
        let layout = layout_for(&[100.0, 100.0, 100.0], Some(1));
        let coverage = coverage_map(&layout);
        assert!(coverage.values().all(|count| *count == 1));
    }

    #[test]
    fn active_first_has_correct_left_boundary_and_gap() {
        let layout = layout_for(&[100.0, 100.0], Some(0));
        let full_height = layout.baseline_y - (layout.tab_top_y + TAB_STROKE_THICKNESS) + 1.0;
        let short_height = full_height - TAB_STROKE_THICKNESS;
        let first_left = tab_left_x(&layout, 0);
        let first_right = tab_right_boundary_x(&layout, 0);
        let second_right = tab_right_boundary_x(&layout, 1);

        assert_eq!(boundary_at_x(&layout, first_left).h, full_height);
        assert_eq!(boundary_at_x(&layout, first_right).h, full_height);
        assert_eq!(boundary_at_x(&layout, second_right).h, short_height);

        let baseline_pixels = baseline_pixels(&layout);
        assert!(!baseline_pixels.contains(&first_left));
        assert!(!baseline_pixels.contains(&first_right));
        assert!(baseline_pixels.contains(&(first_right + 1)));
    }

    #[test]
    fn active_last_has_correct_right_boundary_and_no_trailing_gap() {
        let layout = layout_for(&[100.0, 100.0], Some(1));
        let full_height = layout.baseline_y - (layout.tab_top_y + TAB_STROKE_THICKNESS) + 1.0;
        let short_height = full_height - TAB_STROKE_THICKNESS;
        let first_left = tab_left_x(&layout, 0);
        let second_left = tab_left_x(&layout, 1);
        let second_right = tab_right_boundary_x(&layout, 1);

        assert_eq!(boundary_at_x(&layout, first_left).h, short_height);
        assert_eq!(boundary_at_x(&layout, second_left).h, full_height);
        assert_eq!(boundary_at_x(&layout, second_right).h, full_height);

        let baseline_pixels = baseline_pixels(&layout);
        assert!(baseline_pixels.contains(&(second_left - 1)));
        assert!(!baseline_pixels.contains(&second_left));
        assert!(!baseline_pixels.contains(&second_right));
        assert!(!baseline_pixels.contains(&(second_right + 1)));
    }

    #[test]
    fn single_tab_active_with_zero_padding_has_no_baseline_strokes() {
        let layout = layout_for(&[100.0], Some(0));
        let first_left = tab_left_x(&layout, 0);
        let first_right = tab_right_boundary_x(&layout, 0);

        assert_eq!(layout.boundary_strokes.len(), 2);
        assert_eq!(
            boundary_at_x(&layout, first_left).h,
            boundary_at_x(&layout, first_right).h
        );
        assert_eq!(layout.baseline_strokes.len(), 0);
    }

    #[test]
    fn baseline_is_continuous_outside_active_span() {
        let layout = layout_for(&[100.0, 100.0, 100.0], Some(1));
        let active_left = tab_left_x(&layout, 1);
        let active_right = tab_right_boundary_x(&layout, 1);
        let baseline_pixels = baseline_pixels(&layout);

        let content_width = layout.content_width as i32;
        for x in 0..content_width {
            if x < active_left || x > active_right {
                assert!(
                    baseline_pixels.contains(&x),
                    "missing baseline pixel at x={x}"
                );
            } else {
                assert!(
                    !baseline_pixels.contains(&x),
                    "active tab gap unexpectedly filled at x={x}"
                );
            }
        }
    }

    #[test]
    fn resolved_stroke_color_is_uniform_contract() {
        let tabbar_bg = gpui::Rgba {
            r: 0.12,
            g: 0.18,
            b: 0.24,
            a: 0.64,
        };
        let foreground = gpui::Rgba {
            r: 0.91,
            g: 0.87,
            b: 0.83,
            a: 0.42,
        };
        let resolved = resolve_tab_stroke_color(tabbar_bg, foreground, 0.12);

        assert!((resolved.r - ((0.12 * 0.88) + (0.91 * 0.12))).abs() < 0.0001);
        assert!((resolved.g - ((0.18 * 0.88) + (0.87 * 0.12))).abs() < 0.0001);
        assert!((resolved.b - ((0.24 * 0.88) + (0.83 * 0.12))).abs() < 0.0001);
        assert_eq!(resolved.a, 1.0);
    }

    #[test]
    fn baseline_rectangles_are_bounded_by_content_width() {
        let layout = layout_for(&[96.0, 112.0, 132.0], Some(1));
        for stroke in &layout.baseline_strokes {
            assert!(stroke.x >= 0.0);
            assert!(stroke.w >= 0.0);
            assert!(stroke.x + stroke.w <= layout.content_width);
        }
    }

    #[test]
    fn vertical_active_middle_suppresses_content_divider_only_across_active_span() {
        let layout = vertical_layout_for(&[32.0, 32.0, 32.0], Some(1), 180.0);
        let divider_pixels: HashSet<i32> = layout
            .content_divider_strokes
            .iter()
            .flat_map(|stroke| (stroke.y as i32)..((stroke.y + stroke.h) as i32))
            .collect();

        for y in 0..layout.content_height as i32 {
            if (32..64).contains(&y) {
                assert!(
                    !divider_pixels.contains(&y),
                    "active span unexpectedly contains divider pixel at y={y}"
                );
            } else {
                assert!(
                    divider_pixels.contains(&y),
                    "missing divider pixel outside active span at y={y}"
                );
            }
        }
    }

    #[test]
    fn vertical_active_first_suppresses_divider_for_first_tab_only() {
        let layout = vertical_layout_for(&[32.0, 32.0], Some(0), 180.0);
        let divider_pixels: HashSet<i32> = layout
            .content_divider_strokes
            .iter()
            .flat_map(|stroke| (stroke.y as i32)..((stroke.y + stroke.h) as i32))
            .collect();

        for y in 0..layout.content_height as i32 {
            if y < 32 {
                assert!(
                    !divider_pixels.contains(&y),
                    "active first tab unexpectedly contains divider pixel at y={y}"
                );
            } else {
                assert!(
                    divider_pixels.contains(&y),
                    "missing divider pixel below active first tab at y={y}"
                );
            }
        }
    }

    #[test]
    fn vertical_active_last_suppresses_divider_for_last_tab_only() {
        let layout = vertical_layout_for(&[32.0, 32.0], Some(1), 180.0);
        let divider_pixels: HashSet<i32> = layout
            .content_divider_strokes
            .iter()
            .flat_map(|stroke| (stroke.y as i32)..((stroke.y + stroke.h) as i32))
            .collect();

        for y in 0..layout.content_height as i32 {
            if y < 32 {
                assert!(
                    divider_pixels.contains(&y),
                    "missing divider pixel above active last tab at y={y}"
                );
            } else {
                assert!(
                    !divider_pixels.contains(&y),
                    "active last tab unexpectedly contains divider pixel at y={y}"
                );
            }
        }
    }

    #[test]
    fn vertical_boundaries_shorten_when_not_adjacent_to_active_tab() {
        let layout = vertical_layout_for(&[32.0, 32.0, 32.0], Some(1), 180.0);
        let full_width = layout.divider_x;
        let short_width = full_width - TAB_STROKE_THICKNESS;

        assert!(layout.control_seam.is_some());
        assert!(
            layout.tab_strokes[0].top_boundary.is_none(),
            "control seam should own the list top boundary"
        );
        assert!(
            layout.tab_strokes[0].bottom_boundary.is_none(),
            "active-adjacent separator should be owned by the active tab's top boundary"
        );
        assert_eq!(
            layout.tab_strokes[1]
                .top_boundary
                .expect("active tab should own incoming boundary")
                .w,
            full_width
        );
        assert_eq!(
            layout.tab_strokes[1]
                .bottom_boundary
                .expect("active tab should own outgoing boundary")
                .w,
            full_width
        );
        assert_eq!(
            layout.tab_strokes[2]
                .bottom_boundary
                .expect("last tab should own bottom boundary")
                .w,
            short_width
        );
    }

    #[test]
    fn vertical_divider_rectangles_are_bounded_by_content_height() {
        let layout = vertical_layout_for(&[32.0, 48.0, 32.0], Some(1), 180.0);
        for stroke in &layout.content_divider_strokes {
            assert!(stroke.y >= 0.0);
            assert!(stroke.h >= 0.0);
            assert!(stroke.y + stroke.h <= layout.content_height);
        }
    }

    #[test]
    fn vertical_chrome_has_no_pixel_overlap_through_content_and_tail() {
        let heights = [32.0, 32.0, 32.0];
        let layout = vertical_layout_for(&heights, Some(1), 180.0);
        let coverage = vertical_coverage_map(&layout, &heights, 48.0);

        assert!(coverage.values().all(|count| *count == 1));
    }

    #[test]
    fn vertical_no_active_keeps_continuous_divider_and_single_top_owner() {
        let layout = vertical_layout_for(&[32.0, 32.0], None, 180.0);
        let divider_pixels: HashSet<i32> = layout
            .content_divider_strokes
            .iter()
            .flat_map(|stroke| (stroke.y as i32)..((stroke.y + stroke.h) as i32))
            .collect();

        for y in 0..layout.content_height as i32 {
            assert!(divider_pixels.contains(&y), "missing divider pixel at y={y}");
        }
        assert!(layout.control_seam.is_some());
        assert!(layout.tab_strokes[0].top_boundary.is_none());
    }

    #[test]
    fn vertical_tail_continues_sidebar_edges_below_last_tab() {
        let layout = vertical_layout_for(&[32.0], Some(0), 180.0);

        assert!(layout.tail.draw_left_edge);
        assert!(layout.tail.draw_content_divider);
        assert_eq!(
            layout.tab_strokes[0]
                .bottom_boundary
                .expect("single tab should own bottom boundary")
                .y,
            31.0
        );
    }

    #[test]
    fn vertical_external_top_seam_suppresses_first_row_top_boundary() {
        let layout = vertical_layout_for_with_input(&[32.0, 32.0], Some(1), 180.0, 0.0, true);

        assert!(layout.control_seam.is_none());
        assert!(layout.tab_strokes[0].top_boundary.is_none());
        assert_eq!(layout.tab_strokes[0].left.y, 1.0);
        assert_eq!(layout.tab_strokes[0].left.h, 31.0);
    }

    #[test]
    fn vertical_external_top_seam_starts_content_divider_below_titlebar_seam() {
        let layout = vertical_layout_for_with_input(&[32.0, 32.0], Some(1), 180.0, 0.0, true);

        assert_eq!(
            layout
                .content_divider_strokes
                .first()
                .expect("inactive segment above active tab should keep divider")
                .y,
            1.0
        );
    }

    #[test]
    fn vertical_external_top_seam_has_no_pixel_overlap() {
        let heights = [32.0, 32.0, 32.0];
        let layout = vertical_layout_for_with_input(&heights, Some(1), 180.0, 0.0, true);
        let coverage = vertical_coverage_map(&layout, &heights, 48.0);

        assert!(coverage.values().all(|count| *count == 1));
    }

    #[test]
    fn vertical_external_top_seam_keeps_top_boundary_for_active_first_tab() {
        let layout = vertical_layout_for_with_input(&[32.0, 32.0], Some(0), 180.0, 0.0, true);

        assert_eq!(
            layout.tab_strokes[0]
                .top_boundary
                .expect("active first tab should keep a top boundary")
                .y,
            0.0
        );
        assert_eq!(layout.tab_strokes[0].left.y, 1.0);
    }
}
