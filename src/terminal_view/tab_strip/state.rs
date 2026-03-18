use std::{cell::RefCell, collections::HashMap};

use gpui::ScrollHandle;

use super::hints::TabSwitchHintState;
use super::layout::{TabStripLayoutSnapshot, VerticalTabStripLayoutSnapshot};

const TAB_TITLE_WIDTH_CACHE_MAX_ENTRIES: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabStripOrientation {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabDropMarkerSide {
    Leading,
    Trailing,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct TabStripOverflowState {
    pub(crate) left: bool,
    pub(crate) right: bool,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct TabTitleWidthCache {
    font_family: String,
    font_size_bits: u32,
    widths: HashMap<String, f32>,
}

impl TabTitleWidthCache {
    pub(crate) fn clear(&mut self) {
        self.widths.clear();
    }

    pub(crate) fn invalidate_title(&mut self, title: &str) {
        self.widths.remove(title);
    }

    pub(crate) fn get(&self, title: &str, font_family: &str, font_size_bits: u32) -> Option<f32> {
        if self.font_family != font_family || self.font_size_bits != font_size_bits {
            return None;
        }

        self.widths.get(title).copied()
    }

    pub(crate) fn insert(
        &mut self,
        title: &str,
        font_family: &str,
        font_size_bits: u32,
        width: f32,
    ) {
        if self.font_family != font_family || self.font_size_bits != font_size_bits {
            self.font_family.clear();
            self.font_family.push_str(font_family);
            self.font_size_bits = font_size_bits;
            self.widths.clear();
        }

        if self.widths.len() >= TAB_TITLE_WIDTH_CACHE_MAX_ENTRIES
            && !self.widths.contains_key(title)
        {
            self.widths.clear();
        }
        self.widths.insert(title.to_string(), width.max(0.0));
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.widths.len()
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TabDragState {
    pub(crate) source_index: usize,
    pub(crate) drop_slot: Option<usize>,
    pub(crate) orientation: TabStripOrientation,
}

#[derive(Debug, Default)]
pub(crate) struct VerticalTabStripLayoutCache {
    snapshot: RefCell<Option<VerticalTabStripLayoutSnapshot>>,
}

impl VerticalTabStripLayoutCache {
    pub(crate) fn clear(&self) {
        self.snapshot.borrow_mut().take();
    }

    pub(crate) fn get_or_insert_with(
        &self,
        compute: impl FnOnce() -> VerticalTabStripLayoutSnapshot,
    ) -> VerticalTabStripLayoutSnapshot {
        if let Some(snapshot) = self.snapshot.borrow().clone() {
            return snapshot;
        }

        let snapshot = compute();
        self.snapshot.replace(Some(snapshot.clone()));
        snapshot
    }
}

pub(crate) struct TabStripState {
    pub(crate) horizontal_scroll_handle: ScrollHandle,
    pub(crate) vertical_scroll_handle: ScrollHandle,
    pub(crate) switch_hints: TabSwitchHintState,
    pub(crate) hovered_tab: Option<usize>,
    pub(crate) hovered_tab_close: Option<usize>,
    pub(crate) drag: Option<TabDragState>,
    pub(crate) drag_pointer_primary_axis: Option<f32>,
    pub(crate) drag_viewport_extent: f32,
    pub(crate) drag_autoscroll_animating: bool,
    pub(crate) horizontal_layout_revision: u64,
    pub(crate) horizontal_layout_last_synced_revision: u64,
    pub(crate) horizontal_layout_last_synced_viewport_width: f32,
    pub(crate) horizontal_layout_snapshot: Option<TabStripLayoutSnapshot>,
    pub(crate) vertical_layout_cache: VerticalTabStripLayoutCache,
    pub(crate) title_width_cache: TabTitleWidthCache,
    pub(crate) titlebar_move_armed: bool,
}

impl TabStripState {
    pub(crate) fn new(show_tab_switch_modifier_hints: bool) -> Self {
        Self {
            horizontal_scroll_handle: ScrollHandle::new(),
            vertical_scroll_handle: ScrollHandle::new(),
            switch_hints: TabSwitchHintState::new(show_tab_switch_modifier_hints),
            hovered_tab: None,
            hovered_tab_close: None,
            drag: None,
            drag_pointer_primary_axis: None,
            drag_viewport_extent: 0.0,
            drag_autoscroll_animating: false,
            horizontal_layout_revision: 0,
            horizontal_layout_last_synced_revision: 0,
            horizontal_layout_last_synced_viewport_width: f32::NAN,
            horizontal_layout_snapshot: None,
            vertical_layout_cache: VerticalTabStripLayoutCache::default(),
            title_width_cache: TabTitleWidthCache::default(),
            titlebar_move_armed: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn title_width_cache_hits_for_matching_font_context() {
        let mut cache = TabTitleWidthCache::default();
        cache.insert("~/projects/termy", "JetBrains Mono", 12f32.to_bits(), 142.0);
        assert_eq!(
            cache.get("~/projects/termy", "JetBrains Mono", 12f32.to_bits()),
            Some(142.0)
        );
    }

    #[test]
    fn title_width_cache_misses_when_font_context_changes() {
        let mut cache = TabTitleWidthCache::default();
        cache.insert("title", "JetBrains Mono", 12f32.to_bits(), 64.0);
        assert_eq!(cache.get("title", "JetBrains Mono", 13f32.to_bits()), None);
        assert_eq!(cache.get("title", "Fira Code", 12f32.to_bits()), None);
    }

    #[test]
    fn title_width_cache_invalidate_title_removes_entry() {
        let mut cache = TabTitleWidthCache::default();
        cache.insert("title", "JetBrains Mono", 12f32.to_bits(), 64.0);
        cache.invalidate_title("title");
        assert_eq!(cache.get("title", "JetBrains Mono", 12f32.to_bits()), None);
    }

    #[test]
    fn title_width_cache_bounds_growth_by_clearing_at_capacity() {
        let mut cache = TabTitleWidthCache::default();
        for index in 0..TAB_TITLE_WIDTH_CACHE_MAX_ENTRIES {
            cache.insert(
                format!("tab-{index}").as_str(),
                "JetBrains Mono",
                12f32.to_bits(),
                index as f32,
            );
        }
        assert_eq!(cache.len(), TAB_TITLE_WIDTH_CACHE_MAX_ENTRIES);

        cache.insert("tab-overflow", "JetBrains Mono", 12f32.to_bits(), 1.0);
        assert_eq!(cache.len(), 1);
        assert_eq!(
            cache.get("tab-overflow", "JetBrains Mono", 12f32.to_bits()),
            Some(1.0)
        );
    }

    #[test]
    fn vertical_layout_cache_reuses_snapshot_until_cleared() {
        let cache = VerticalTabStripLayoutCache::default();
        let first = VerticalTabStripLayoutSnapshot {
            strip_width: 160.0,
            compact: false,
            header_height: 34.0,
            top_shelf_layout: super::super::layout::VerticalNewTabShelfLayout {
                shelf_height: 44.0,
                button_x: 8.0,
                button_y: 8.0,
                button_width: 120.0,
                button_height: 22.0,
            },
            bottom_shelf_layout: super::super::layout::VerticalBottomShelfLayout {
                shelf_height: 38.0,
                button_size: 22.0,
                icon_size: 14.0,
            },
            list_top: 78.0,
            list_height: 120.0,
            bottom_shelf_top: 198.0,
            divider_x: 159.0,
            resize_handle_left: 156.0,
            content_height: 32.0,
            rows: vec![super::super::layout::VerticalTabRowLayout {
                index: 0,
                top: 0.0,
                height: 32.0,
            }],
        };
        let second = VerticalTabStripLayoutSnapshot {
            rows: vec![super::super::layout::VerticalTabRowLayout {
                index: 0,
                top: 0.0,
                height: 12.0,
            }],
            content_height: 12.0,
            ..first.clone()
        };
        let mut computes = 0;

        assert_eq!(
            cache.get_or_insert_with(|| {
                computes += 1;
                first.clone()
            }),
            first
        );
        assert_eq!(
            cache.get_or_insert_with(|| {
                computes += 1;
                second.clone()
            }),
            first
        );
        assert_eq!(computes, 1);

        cache.clear();

        assert_eq!(
            cache.get_or_insert_with(|| {
                computes += 1;
                second.clone()
            }),
            second
        );
        assert_eq!(computes, 2);
    }
}
