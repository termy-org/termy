use gpui::{AnyElement, InteractiveElement, IntoElement, ParentElement, Rgba, Styled, div, px};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarRange {
    pub offset: f32,
    pub max_offset: f32,
    pub viewport_extent: f32,
    pub track_extent: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarMetrics {
    pub thumb_top: f32,
    pub thumb_height: f32,
    pub travel: f32,
    pub track_height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollbarVisibilityMode {
    AlwaysOff,
    AlwaysOn,
    OnScroll,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarPaintStyle {
    pub width: f32,
    pub track_radius: f32,
    pub thumb_radius: f32,
    pub thumb_inset: f32,
    pub marker_inset: f32,
    pub marker_radius: f32,
    pub track_color: Rgba,
    pub thumb_color: Rgba,
    pub active_thumb_color: Rgba,
    pub marker_color: Option<Rgba>,
    pub current_marker_color: Option<Rgba>,
}

impl ScrollbarPaintStyle {
    pub fn scale_alpha(self, alpha: f32) -> Self {
        let alpha = alpha.clamp(0.0, 1.0);
        Self {
            track_color: scale_color_alpha(self.track_color, alpha),
            thumb_color: scale_color_alpha(self.thumb_color, alpha),
            active_thumb_color: scale_color_alpha(self.active_thumb_color, alpha),
            marker_color: self
                .marker_color
                .map(|color| scale_color_alpha(color, alpha)),
            current_marker_color: self
                .current_marker_color
                .map(|color| scale_color_alpha(color, alpha)),
            ..self
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ScrollbarVisibilityController {
    last_activity: Option<Instant>,
    dragging: bool,
}

impl ScrollbarVisibilityController {
    pub fn mark_activity(&mut self, now: Instant) {
        self.last_activity = Some(now);
    }

    pub fn start_drag(&mut self, now: Instant) {
        self.dragging = true;
        self.mark_activity(now);
    }

    pub fn end_drag(&mut self, now: Instant) {
        self.dragging = false;
        self.mark_activity(now);
    }

    pub fn reset(&mut self) {
        self.last_activity = None;
        self.dragging = false;
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    pub fn alpha(
        &self,
        mode: ScrollbarVisibilityMode,
        now: Instant,
        hold_duration: Duration,
        fade_duration: Duration,
    ) -> f32 {
        match mode {
            ScrollbarVisibilityMode::AlwaysOff => 0.0,
            ScrollbarVisibilityMode::AlwaysOn => 1.0,
            ScrollbarVisibilityMode::OnScroll => {
                if self.dragging {
                    return 1.0;
                }

                let Some(last_activity) = self.last_activity else {
                    return 0.0;
                };

                let elapsed = now.saturating_duration_since(last_activity);
                if elapsed <= hold_duration {
                    return 1.0;
                }

                if fade_duration.is_zero() {
                    return 0.0;
                }

                let fade_elapsed = elapsed.saturating_sub(hold_duration).as_secs_f32();
                let fade_total = fade_duration.as_secs_f32();
                (1.0 - (fade_elapsed / fade_total)).clamp(0.0, 1.0)
            }
        }
    }

    pub fn needs_animation(
        &self,
        mode: ScrollbarVisibilityMode,
        now: Instant,
        hold_duration: Duration,
        fade_duration: Duration,
    ) -> bool {
        if mode != ScrollbarVisibilityMode::OnScroll {
            return false;
        }
        if self.dragging {
            return true;
        }
        let Some(last_activity) = self.last_activity else {
            return false;
        };

        now.saturating_duration_since(last_activity) < hold_duration + fade_duration
    }
}

pub fn compute_metrics(range: ScrollbarRange, min_thumb_height: f32) -> Option<ScrollbarMetrics> {
    let viewport_extent = range.viewport_extent;
    let track_extent = range.track_extent;
    if viewport_extent <= f32::EPSILON || track_extent <= f32::EPSILON {
        return None;
    }

    let max_offset = range.max_offset.max(0.0);
    if max_offset <= f32::EPSILON {
        return None;
    }

    let offset = range.offset.clamp(0.0, max_offset);
    let content_extent = viewport_extent + max_offset;
    let thumb_height = ((viewport_extent / content_extent) * track_extent)
        .clamp(min_thumb_height.max(1.0), track_extent);
    let travel = (track_extent - thumb_height).max(0.0);
    let thumb_top = if travel <= f32::EPSILON {
        0.0
    } else {
        (offset / max_offset) * travel
    };

    Some(ScrollbarMetrics {
        thumb_top,
        thumb_height,
        travel,
        track_height: track_extent,
    })
}

pub fn offset_from_track_click(
    click_y: f32,
    range: ScrollbarRange,
    metrics: ScrollbarMetrics,
) -> f32 {
    let top = (click_y - (metrics.thumb_height * 0.5)).clamp(0.0, metrics.travel);
    offset_from_thumb_top(top, range, metrics)
}

pub fn offset_from_thumb_top(
    thumb_top: f32,
    range: ScrollbarRange,
    metrics: ScrollbarMetrics,
) -> f32 {
    let max_offset = range.max_offset.max(0.0);
    if max_offset <= f32::EPSILON || metrics.travel <= f32::EPSILON {
        return 0.0;
    }

    (thumb_top.clamp(0.0, metrics.travel) / metrics.travel) * max_offset
}

pub fn invert_offset_axis(offset: f32, max_offset: f32) -> f32 {
    let max_offset = max_offset.max(0.0);
    (max_offset - offset.clamp(0.0, max_offset)).clamp(0.0, max_offset)
}

pub fn render_vertical(
    id: &'static str,
    metrics: ScrollbarMetrics,
    style: ScrollbarPaintStyle,
    thumb_active: bool,
    marker_tops: &[f32],
    current_marker_top: Option<f32>,
    marker_height: f32,
) -> AnyElement {
    let thumb_color = if thumb_active {
        style.active_thumb_color
    } else {
        style.thumb_color
    };
    let thumb_inset = style.thumb_inset.max(0.0);
    let marker_inset = style.marker_inset.max(0.0);
    let marker_radius = style.marker_radius.max(0.0);
    let mut marker_elements = Vec::new();
    if marker_height > 0.0 {
        let marker_count = if style.marker_color.is_some() {
            marker_tops.len()
        } else {
            0
        };
        let current_marker_count =
            usize::from(style.current_marker_color.is_some() && current_marker_top.is_some());
        marker_elements.reserve(marker_count.saturating_add(current_marker_count));
        let marker_top_max = (metrics.track_height - marker_height).max(0.0);

        if let Some(color) = style.marker_color {
            marker_elements.extend(marker_tops.iter().copied().map(|top| {
                div()
                    .absolute()
                    .left(px(marker_inset))
                    .right(px(marker_inset))
                    .top(px(top.clamp(0.0, marker_top_max)))
                    .h(px(marker_height))
                    .rounded(px(marker_radius))
                    .bg(color)
                    .into_any_element()
            }));
        }

        if let Some(color) = style.current_marker_color {
            if let Some(top) = current_marker_top {
                marker_elements.push(
                    div()
                        .absolute()
                        .left(px(marker_inset))
                        .right(px(marker_inset))
                        .top(px(top.clamp(0.0, marker_top_max)))
                        .h(px(marker_height))
                        .rounded(px(marker_radius))
                        .bg(color)
                        .into_any_element(),
                );
            }
        }
    }

    div()
        .id(id)
        .relative()
        .w(px(style.width))
        .h(px(metrics.track_height))
        .rounded(px(style.track_radius.max(0.0)))
        .bg(style.track_color)
        .child(
            div()
                .absolute()
                .left(px(thumb_inset))
                .right(px(thumb_inset))
                .top(px(metrics.thumb_top))
                .h(px(metrics.thumb_height))
                .rounded(px(style.thumb_radius.max(0.0)))
                .bg(thumb_color),
        )
        .children(marker_elements)
        .into_any_element()
}

fn scale_color_alpha(mut color: Rgba, factor: f32) -> Rgba {
    color.a = (color.a * factor).clamp(0.0, 1.0);
    color
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_none_without_overflow() {
        let range = ScrollbarRange {
            offset: 0.0,
            max_offset: 0.0,
            viewport_extent: 400.0,
            track_extent: 400.0,
        };
        assert!(compute_metrics(range, 16.0).is_none());
    }

    #[test]
    fn metrics_enforce_min_thumb_height() {
        let range = ScrollbarRange {
            offset: 100.0,
            max_offset: 1_000.0,
            viewport_extent: 200.0,
            track_extent: 200.0,
        };
        let metrics = compute_metrics(range, 32.0).expect("expected metrics");
        assert!(metrics.thumb_height >= 32.0);
        assert!(metrics.thumb_top >= 0.0);
        assert!(metrics.thumb_top <= metrics.travel);
    }

    #[test]
    fn click_and_drag_offsets_clamp_to_range() {
        let range = ScrollbarRange {
            offset: 0.0,
            max_offset: 300.0,
            viewport_extent: 240.0,
            track_extent: 240.0,
        };
        let metrics = compute_metrics(range, 18.0).expect("expected metrics");

        let from_click = offset_from_track_click(1_000.0, range, metrics);
        assert!(from_click <= range.max_offset);

        let from_drag = offset_from_thumb_top(-20.0, range, metrics);
        assert!(from_drag >= 0.0);
    }

    #[test]
    fn invert_offset_axis_flips_and_clamps() {
        assert_eq!(invert_offset_axis(0.0, 300.0), 300.0);
        assert_eq!(invert_offset_axis(300.0, 300.0), 0.0);
        assert_eq!(invert_offset_axis(-20.0, 300.0), 300.0);
        assert_eq!(invert_offset_axis(420.0, 300.0), 0.0);
    }

    #[test]
    fn metrics_respect_separate_track_extent() {
        let range = ScrollbarRange {
            offset: 120.0,
            max_offset: 600.0,
            viewport_extent: 200.0,
            track_extent: 260.0,
        };
        let metrics = compute_metrics(range, 18.0).expect("expected metrics");
        assert_eq!(metrics.track_height, 260.0);
        assert!(metrics.thumb_height <= 260.0);
        assert!(metrics.thumb_top <= metrics.travel);
    }

    #[test]
    fn on_scroll_visibility_fades_after_hold() {
        let start = Instant::now();
        let hold = Duration::from_millis(900);
        let fade = Duration::from_millis(140);

        let mut controller = ScrollbarVisibilityController::default();
        controller.mark_activity(start);

        assert_eq!(
            controller.alpha(ScrollbarVisibilityMode::OnScroll, start, hold, fade),
            1.0
        );

        let during_fade = start + hold + Duration::from_millis(70);
        let alpha = controller.alpha(ScrollbarVisibilityMode::OnScroll, during_fade, hold, fade);
        assert!(alpha > 0.0);
        assert!(alpha < 1.0);

        let done = start + hold + fade + Duration::from_millis(1);
        assert_eq!(
            controller.alpha(ScrollbarVisibilityMode::OnScroll, done, hold, fade),
            0.0
        );
        assert!(!controller.needs_animation(ScrollbarVisibilityMode::OnScroll, done, hold, fade));
    }
}
