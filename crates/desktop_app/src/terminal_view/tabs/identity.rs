use super::*;

impl TerminalView {
    pub(crate) fn allocate_tab_id(&mut self) -> TabId {
        let id = self.next_tab_id;
        self.next_tab_id = self.next_tab_id.checked_add(1).expect("tab id overflow");
        id
    }

    pub(crate) fn clear_tab_title_width_cache(&mut self) {
        self.tab_strip.title_width_cache.clear();
    }

    pub(crate) fn invalidate_tab_title_width_cache_for_title(&mut self, title: &str) {
        self.tab_strip.title_width_cache.invalidate_title(title);
    }
}
