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

pub(super) fn resolve_tab_stroke_color(
    tabbar_background: gpui::Rgba,
    foreground: gpui::Rgba,
    foreground_mix: f32,
) -> gpui::Rgba {
    let mix = foreground_mix.clamp(0.0, 1.0);
    let inv_mix = 1.0 - mix;

    gpui::Rgba {
        r: (tabbar_background.r * inv_mix) + (foreground.r * mix),
        g: (tabbar_background.g * inv_mix) + (foreground.g * mix),
        b: (tabbar_background.b * inv_mix) + (foreground.b * mix),
        // Opaque stroke eliminates tab-background-dependent blending drift.
        a: 1.0,
    }
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
}
