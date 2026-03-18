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
