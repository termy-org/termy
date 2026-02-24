use super::*;

impl TerminalView {
    fn clear_tab_drag_preview_state(&mut self) {
        self.tab_strip.drag_pointer_x = None;
        self.tab_strip.drag_viewport_width = 0.0;
        self.tab_strip.drag_autoscroll_animating = false;
    }

    fn ensure_tab_drag_autoscroll_animation(&mut self, cx: &mut Context<Self>) {
        if self.tab_strip.drag_autoscroll_animating {
            return;
        }
        self.tab_strip.drag_autoscroll_animating = true;

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                smol::Timer::after(Duration::from_millis(16)).await;
                let keep_animating = match cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        if !view.tab_strip.drag_autoscroll_animating
                            || view.tab_strip.drag.is_none()
                        {
                            view.tab_strip.drag_autoscroll_animating = false;
                            return false;
                        }

                        let Some(pointer_x) = view.tab_strip.drag_pointer_x else {
                            view.tab_strip.drag_autoscroll_animating = false;
                            return false;
                        };
                        let viewport_width = view.tab_strip.drag_viewport_width;
                        let scrolled =
                            view.auto_scroll_tab_strip_during_drag(pointer_x, viewport_width);
                        let marker_changed = view.update_tab_drag_marker(pointer_x, cx);
                        if scrolled && !marker_changed {
                            cx.notify();
                        }
                        if !scrolled {
                            view.tab_strip.drag_autoscroll_animating = false;
                            return false;
                        }
                        true
                    })
                }) {
                    Ok(keep_animating) => keep_animating,
                    _ => break,
                };

                if !keep_animating {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn begin_tab_drag(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.clear_tab_drag_preview_state();
            self.tab_strip.drag = Some(TabDragState {
                source_index: index,
                drop_slot: None,
            });
        }
    }

    pub(crate) fn finish_tab_drag(&mut self) -> bool {
        let marker_was_visible = self
            .tab_strip
            .drag
            .as_ref()
            .and_then(|drag| drag.drop_slot)
            .is_some();
        self.tab_strip.drag = None;
        self.clear_tab_drag_preview_state();
        marker_was_visible
    }

    fn tab_drop_slot_from_pointer_x_for_widths(
        tab_widths: impl IntoIterator<Item = f32>,
        pointer_x: f32,
        scroll_offset_x: f32,
    ) -> usize {
        let mut left = TAB_HORIZONTAL_PADDING + scroll_offset_x;
        let mut slot = 0;

        for width in tab_widths {
            let midpoint_x = left + (width * 0.5);
            if pointer_x < midpoint_x {
                return slot;
            }

            left += width + TAB_ITEM_GAP;
            slot += 1;
        }

        slot
    }

    fn tab_drop_slot_from_pointer_x(&self, pointer_x: f32) -> usize {
        let scroll_offset_x: f32 = self.tab_strip.scroll_handle.offset().x.into();
        Self::tab_drop_slot_from_pointer_x_for_widths(
            self.tabs.iter().map(|tab| tab.display_width),
            pointer_x,
            scroll_offset_x,
        )
    }

    fn normalized_drop_slot(source_index: usize, raw_slot: usize) -> Option<usize> {
        if raw_slot == source_index || raw_slot == source_index.saturating_add(1) {
            return None;
        }
        Some(raw_slot)
    }

    fn reorder_target_index_for_drop_slot(source_index: usize, drop_slot: usize) -> usize {
        if drop_slot > source_index {
            drop_slot - 1
        } else {
            drop_slot
        }
    }

    fn tab_drop_marker_side_for_slot(index: usize, drop_slot: usize) -> Option<TabDropMarkerSide> {
        if drop_slot == index {
            Some(TabDropMarkerSide::Left)
        } else if drop_slot == index.saturating_add(1) {
            Some(TabDropMarkerSide::Right)
        } else {
            None
        }
    }

    pub(crate) fn tab_drop_marker_side(&self, index: usize) -> Option<TabDropMarkerSide> {
        if index >= self.tabs.len() {
            return None;
        }

        let drop_slot = self.tab_strip.drag.and_then(|drag| drag.drop_slot)?;
        Self::tab_drop_marker_side_for_slot(index, drop_slot)
    }

    fn update_tab_drag_marker(&mut self, pointer_x: f32, cx: &mut Context<Self>) -> bool {
        let Some(source_index) = self.tab_strip.drag.map(|drag| drag.source_index) else {
            return false;
        };

        let raw_drop_slot = self.tab_drop_slot_from_pointer_x(pointer_x);
        let next_drop_slot = Self::normalized_drop_slot(source_index, raw_drop_slot);

        let Some(drag) = self.tab_strip.drag.as_mut() else {
            return false;
        };
        if drag.drop_slot == next_drop_slot {
            return false;
        }

        drag.drop_slot = next_drop_slot;
        cx.notify();
        true
    }

    fn auto_scroll_tab_strip_during_drag(&mut self, pointer_x: f32, viewport_width: f32) -> bool {
        if self.tab_strip.drag.is_none() || viewport_width <= f32::EPSILON {
            return false;
        }

        let edge = TAB_DRAG_AUTOSCROLL_EDGE_WIDTH
            .min(viewport_width * 0.5)
            .max(f32::EPSILON);
        let left_strength = ((edge - pointer_x) / edge).clamp(0.0, 1.0);
        let right_start = (viewport_width - edge).max(0.0);
        let right_strength = ((pointer_x - right_start) / edge).clamp(0.0, 1.0);
        let delta = (right_strength - left_strength) * TAB_DRAG_AUTOSCROLL_MAX_STEP;
        self.scroll_tab_strip_by(-delta)
    }

    pub(crate) fn update_tab_drag_preview(
        &mut self,
        pointer_x: f32,
        viewport_width: f32,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.tab_strip.drag.is_none() {
            return false;
        }
        self.tab_strip.drag_pointer_x = Some(pointer_x);
        self.tab_strip.drag_viewport_width = viewport_width.max(0.0);
        let widths_changed =
            self.sync_tab_display_widths_for_viewport_if_needed(self.tab_strip.drag_viewport_width);

        let scrolled = self.auto_scroll_tab_strip_during_drag(pointer_x, viewport_width);
        let marker_changed = self.update_tab_drag_marker(pointer_x, cx);
        if scrolled && !marker_changed {
            cx.notify();
        }
        if widths_changed && !scrolled && !marker_changed {
            cx.notify();
        }
        if scrolled {
            self.ensure_tab_drag_autoscroll_animation(cx);
        } else {
            self.tab_strip.drag_autoscroll_animating = false;
        }
        scrolled || marker_changed || widths_changed
    }

    pub(crate) fn commit_tab_drag(&mut self, cx: &mut Context<Self>) {
        let drag = self.tab_strip.drag.take();
        self.clear_tab_drag_preview_state();
        let Some(TabDragState {
            source_index,
            drop_slot,
        }) = drag
        else {
            return;
        };

        let Some(drop_slot) = drop_slot else {
            return;
        };

        let target_index = Self::reorder_target_index_for_drop_slot(source_index, drop_slot);
        if source_index == target_index {
            cx.notify();
            return;
        }

        if !self.reorder_tab(source_index, target_index, cx) {
            cx.notify();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn synthetic_title_width_px(title: &str) -> f32 {
        title.chars().count() as f32 * 7.0
    }

    #[test]
    fn normalized_drop_slot_filters_noop_boundaries() {
        assert_eq!(TerminalView::normalized_drop_slot(2, 2), None);
        assert_eq!(TerminalView::normalized_drop_slot(2, 3), None);
        assert_eq!(TerminalView::normalized_drop_slot(2, 1), Some(1));
        assert_eq!(TerminalView::normalized_drop_slot(2, 4), Some(4));
    }

    #[test]
    fn reorder_target_index_for_drop_slot_moves_right_correctly() {
        assert_eq!(TerminalView::reorder_target_index_for_drop_slot(1, 3), 2);
        assert_eq!(TerminalView::reorder_target_index_for_drop_slot(0, 3), 2);
    }

    #[test]
    fn reorder_target_index_for_drop_slot_moves_left_correctly() {
        assert_eq!(TerminalView::reorder_target_index_for_drop_slot(3, 1), 1);
        assert_eq!(TerminalView::reorder_target_index_for_drop_slot(2, 0), 0);
    }

    #[test]
    fn tab_drop_slot_from_pointer_x_respects_midpoints() {
        let widths = [100.0, 100.0, 100.0];
        assert_eq!(
            TerminalView::tab_drop_slot_from_pointer_x_for_widths(widths, 40.0, 0.0),
            0
        );
        assert_eq!(
            TerminalView::tab_drop_slot_from_pointer_x_for_widths(widths, 70.0, 0.0),
            1
        );
        assert_eq!(
            TerminalView::tab_drop_slot_from_pointer_x_for_widths(widths, 170.0, 0.0),
            2
        );
        assert_eq!(
            TerminalView::tab_drop_slot_from_pointer_x_for_widths(widths, 270.0, 0.0),
            3
        );
    }

    #[test]
    fn tab_drop_slot_from_pointer_x_respects_scroll_offset() {
        let widths = [100.0, 100.0];
        assert_eq!(
            TerminalView::tab_drop_slot_from_pointer_x_for_widths(widths, 40.0, 0.0),
            0
        );
        assert_eq!(
            TerminalView::tab_drop_slot_from_pointer_x_for_widths(widths, 40.0, -30.0),
            1
        );
    }

    #[test]
    fn tab_drop_marker_side_maps_slot_to_left_and_right_edges() {
        assert_eq!(
            TerminalView::tab_drop_marker_side_for_slot(2, 2),
            Some(TabDropMarkerSide::Left)
        );
        assert_eq!(
            TerminalView::tab_drop_marker_side_for_slot(2, 3),
            Some(TabDropMarkerSide::Right)
        );
        assert_eq!(TerminalView::tab_drop_marker_side_for_slot(2, 1), None);
    }

    #[test]
    fn tab_drop_slot_mapping_is_stable_with_adaptive_widths() {
        let effective_max = TerminalView::effective_tab_max_width_for_viewport(1500.0, 3);
        let widths = [
            TerminalView::tab_display_width_for_text_px_with_max(
                synthetic_title_width_px("~/Desktop/claudeCode/claude-code-provider-proxy/docs"),
                effective_max,
            ),
            TerminalView::tab_display_width_for_text_px_with_max(
                synthetic_title_width_px("~"),
                effective_max,
            ),
            TerminalView::tab_display_width_for_text_px_with_max(
                synthetic_title_width_px("~/projects/termy"),
                effective_max,
            ),
        ];

        let first_midpoint = TAB_HORIZONTAL_PADDING + (widths[0] * 0.5);
        assert_eq!(
            TerminalView::tab_drop_slot_from_pointer_x_for_widths(
                widths,
                first_midpoint - 1.0,
                0.0,
            ),
            0
        );
        assert_eq!(
            TerminalView::tab_drop_slot_from_pointer_x_for_widths(
                widths,
                first_midpoint + 1.0,
                0.0,
            ),
            1
        );
    }
}
