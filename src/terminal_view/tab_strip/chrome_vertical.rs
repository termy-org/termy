use super::*;

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
            first_tab.left.y = top_seam_offset_y;
            first_tab.left.h = (first_tab.left.h - top_seam_offset_y).max(0.0);
        }

        if input.active_index == Some(0)
            && let Some(first_span) = spans.first().copied()
        {
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
    } else if control_seam.is_none() && let Some(first_span) = spans.first().copied() {
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
