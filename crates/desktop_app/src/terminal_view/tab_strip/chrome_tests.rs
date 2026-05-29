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
