use super::*;

impl TerminalView {
    pub(super) fn allocate_tab_id(&mut self) -> TabId {
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.checked_add(1).expect("tab id overflow");
        id
    }

    pub(crate) fn tab_index_for_id_in_order(
        tab_ids: impl IntoIterator<Item = TabId>,
        tab_id: TabId,
    ) -> Option<usize> {
        tab_ids
            .into_iter()
            .enumerate()
            .find_map(|(index, candidate)| (candidate == tab_id).then_some(index))
    }

    pub(crate) fn index_for_tab_id(&self, tab_id: TabId) -> Option<usize> {
        Self::tab_index_for_id_in_order(self.tabs.iter().map(|tab| tab.id), tab_id)
    }

    pub(crate) fn clear_tab_title_width_cache(&mut self) {
        self.tab_strip.title_width_cache.clear();
    }

    pub(crate) fn invalidate_tab_title_width_cache_for_title(&mut self, title: &str) {
        self.tab_strip.title_width_cache.invalidate_title(title);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_index_for_id_follows_reordered_tab_sequence() {
        let tab_ids: [TabId; 4] = [11, 13, 17, 19];
        assert_eq!(
            TerminalView::tab_index_for_id_in_order(tab_ids, 17),
            Some(2)
        );

        // Simulate drag reorder 17 -> slot 1
        let reordered: [TabId; 4] = [11, 17, 13, 19];
        assert_eq!(
            TerminalView::tab_index_for_id_in_order(reordered, 17),
            Some(1)
        );
    }

    #[test]
    fn tab_index_for_id_returns_none_after_tab_close() {
        let tab_ids: [TabId; 4] = [11, 13, 17, 19];
        let after_close: [TabId; 3] = [11, 13, 19];
        assert_eq!(
            TerminalView::tab_index_for_id_in_order(tab_ids, 17),
            Some(2)
        );
        assert_eq!(
            TerminalView::tab_index_for_id_in_order(after_close, 17),
            None
        );
    }
}
