use super::*;

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
