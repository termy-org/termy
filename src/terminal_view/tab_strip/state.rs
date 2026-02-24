use std::collections::HashMap;

use gpui::ScrollHandle;

use super::layout::TabStripLayoutSnapshot;

const TAB_TITLE_WIDTH_CACHE_MAX_ENTRIES: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TabDropMarkerSide {
    Left,
    Right,
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
}

pub(crate) struct TabStripState {
    pub(crate) scroll_handle: ScrollHandle,
    pub(crate) hovered_tab: Option<usize>,
    pub(crate) hovered_tab_close: Option<usize>,
    pub(crate) drag: Option<TabDragState>,
    pub(crate) drag_pointer_x: Option<f32>,
    pub(crate) drag_viewport_width: f32,
    pub(crate) drag_autoscroll_animating: bool,
    pub(crate) layout_revision: u64,
    pub(crate) layout_last_synced_revision: u64,
    pub(crate) layout_last_synced_viewport_width: f32,
    pub(crate) layout_snapshot: Option<TabStripLayoutSnapshot>,
    pub(crate) title_width_cache: TabTitleWidthCache,
    pub(crate) titlebar_move_armed: bool,
}

impl TabStripState {
    pub(crate) fn new() -> Self {
        Self {
            scroll_handle: ScrollHandle::new(),
            hovered_tab: None,
            hovered_tab_close: None,
            drag: None,
            drag_pointer_x: None,
            drag_viewport_width: 0.0,
            drag_autoscroll_animating: false,
            layout_revision: 0,
            layout_last_synced_revision: 0,
            layout_last_synced_viewport_width: f32::NAN,
            layout_snapshot: None,
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
}
