use super::*;

impl TerminalView {
    fn adjacent_tab_index(
        active_tab: usize,
        tab_count: usize,
        to_right: bool,
    ) -> Option<usize> {
        if tab_count <= 1 || active_tab >= tab_count {
            return None;
        }

        if to_right {
            (active_tab + 1 < tab_count).then_some(active_tab + 1)
        } else {
            active_tab.checked_sub(1)
        }
    }

    fn remap_index_after_move(index: usize, from: usize, to: usize) -> usize {
        if index == from {
            return to;
        }

        if from < to {
            if (from + 1..=to).contains(&index) {
                return index - 1;
            }
            index
        } else if (to..from).contains(&index) {
            index + 1
        } else {
            index
        }
    }

    pub(crate) fn reorder_tab(&mut self, from: usize, to: usize, cx: &mut Context<Self>) -> bool {
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return false;
        }

        let moved_tab = self.tabs.remove(from);
        self.tabs.insert(to, moved_tab);

        self.active_tab = Self::remap_index_after_move(self.active_tab, from, to);
        self.renaming_tab = self
            .renaming_tab
            .map(|index| Self::remap_index_after_move(index, from, to));
        self.tab_strip.hovered_tab = self
            .tab_strip
            .hovered_tab
            .map(|index| Self::remap_index_after_move(index, from, to));
        self.tab_strip.hovered_tab_close = self
            .tab_strip
            .hovered_tab_close
            .map(|index| Self::remap_index_after_move(index, from, to));

        self.scroll_active_tab_into_view();
        cx.notify();
        true
    }

    pub(crate) fn move_active_tab_left(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(target_index) = Self::adjacent_tab_index(self.active_tab, self.tabs.len(), false)
        else {
            return false;
        };

        self.reorder_tab(self.active_tab, target_index, cx)
    }

    pub(crate) fn move_active_tab_right(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(target_index) = Self::adjacent_tab_index(self.active_tab, self.tabs.len(), true)
        else {
            return false;
        };

        self.reorder_tab(self.active_tab, target_index, cx)
    }

    pub(crate) fn switch_active_tab_left(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(target_index) = Self::adjacent_tab_index(self.active_tab, self.tabs.len(), false)
        else {
            return false;
        };

        self.switch_tab(target_index, cx);
        true
    }

    pub(crate) fn switch_active_tab_right(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(target_index) = Self::adjacent_tab_index(self.active_tab, self.tabs.len(), true)
        else {
            return false;
        };

        self.switch_tab(target_index, cx);
        true
    }

    pub(crate) fn add_tab(&mut self, cx: &mut Context<Self>) {
        let terminal = Terminal::new(
            TerminalSize::default(),
            self.configured_working_dir.as_deref(),
            Some(self.event_wakeup_tx.clone()),
            Some(&self.tab_shell_integration),
            Some(&self.terminal_runtime),
        )
        .expect("Failed to create terminal tab");

        let predicted_prompt_cwd = Self::predicted_prompt_cwd(
            self.configured_working_dir.as_deref(),
            self.terminal_runtime.working_dir_fallback,
        );
        let predicted_title =
            Self::predicted_prompt_seed_title(&self.tab_title, predicted_prompt_cwd.as_deref());

        let tab_id = self.allocate_tab_id();
        self.tabs
            .push(TerminalTab::new(tab_id, terminal, predicted_title));
        self.active_tab = self.tabs.len() - 1;
        self.refresh_tab_title(self.active_tab);
        self.mark_tab_strip_layout_dirty();
        self.reset_tab_interaction_state();
        self.scroll_active_tab_into_view();
        cx.notify();
    }

    pub(crate) fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            return;
        }

        self.tabs.remove(index);
        self.mark_tab_strip_layout_dirty();

        if self.active_tab > index {
            self.active_tab -= 1;
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }

        match self.renaming_tab {
            Some(editing) if editing == index => {
                self.reset_tab_rename_state();
            }
            Some(editing) if editing > index => {
                self.renaming_tab = Some(editing - 1);
            }
            _ => {}
        }

        self.tab_strip.hovered_tab = match self.tab_strip.hovered_tab {
            Some(hovered) if hovered == index => None,
            Some(hovered) if hovered > index => Some(hovered - 1),
            value => value,
        };
        self.tab_strip.hovered_tab_close = match self.tab_strip.hovered_tab_close {
            Some(hovered) if hovered == index => None,
            Some(hovered) if hovered > index => Some(hovered - 1),
            value => value,
        };
        self.reset_tab_drag_state();

        self.clear_selection();
        self.scroll_active_tab_into_view();
        cx.notify();
    }

    pub(crate) fn close_active_tab(&mut self, cx: &mut Context<Self>) {
        self.close_tab(self.active_tab, cx);
    }

    pub(crate) fn begin_rename_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }

        if self.command_palette_open {
            self.close_command_palette(cx);
        }
        if self.search_open {
            self.close_search(cx);
        }

        if self.active_tab != index {
            self.switch_tab(index, cx);
        }

        self.reset_tab_drag_state();
        self.renaming_tab = Some(index);
        self.rename_input.set_text(self.tabs[index].title.clone());
        self.reset_cursor_blink_phase();
        self.inline_input_selecting = false;
        cx.notify();
    }

    pub(crate) fn switch_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() || index == self.active_tab {
            return;
        }

        let old_active = self.active_tab;
        self.active_tab = index;
        if self.tab_width_mode != TabWidthMode::Stable {
            self.mark_tab_strip_layout_dirty();
            self.sync_tab_display_widths_for_viewport_if_needed(
                self.tab_strip.layout_last_synced_viewport_width,
            );
        }

        // Apply inactive_tab_scrollback optimization if configured
        if let Some(inactive_scrollback) = self.inactive_tab_scrollback {
            // Shrink the previously active tab's scrollback to save memory
            self.tabs[old_active]
                .terminal
                .set_scrollback_history(inactive_scrollback);

            // Restore full scrollback for the newly active tab
            self.tabs[index]
                .terminal
                .set_scrollback_history(self.terminal_runtime.scrollback_history);
        }

        self.reset_tab_rename_state();
        self.reset_tab_drag_state();
        self.clear_selection();
        self.scroll_active_tab_into_view();
        cx.notify();
    }

    pub(crate) fn commit_rename_tab(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.renaming_tab else {
            return;
        };

        let trimmed = self.rename_input.text().trim();
        self.tabs[index].manual_title = (!trimmed.is_empty())
            .then(|| Self::truncate_tab_title(trimmed))
            .filter(|title| !title.is_empty());
        self.refresh_tab_title(index);

        self.reset_tab_rename_state();
        self.reset_tab_drag_state();
        cx.notify();
    }

    pub(crate) fn cancel_rename_tab(&mut self, cx: &mut Context<Self>) {
        if self.renaming_tab.is_none() {
            return;
        }

        self.reset_tab_rename_state();
        self.reset_tab_drag_state();
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adjacent_tab_index_moves_middle_tab_left_and_right() {
        assert_eq!(TerminalView::adjacent_tab_index(2, 5, false), Some(1));
        assert_eq!(TerminalView::adjacent_tab_index(2, 5, true), Some(3));
    }

    #[test]
    fn adjacent_tab_index_is_none_for_edges() {
        assert_eq!(TerminalView::adjacent_tab_index(0, 5, false), None);
        assert_eq!(TerminalView::adjacent_tab_index(4, 5, true), None);
    }

    #[test]
    fn adjacent_tab_index_is_none_for_invalid_or_singleton_state() {
        assert_eq!(TerminalView::adjacent_tab_index(0, 0, false), None);
        assert_eq!(TerminalView::adjacent_tab_index(0, 1, true), None);
        assert_eq!(TerminalView::adjacent_tab_index(5, 3, true), None);
    }

    #[test]
    fn remap_index_after_move_handles_move_to_right() {
        assert_eq!(TerminalView::remap_index_after_move(1, 1, 3), 3);
        assert_eq!(TerminalView::remap_index_after_move(2, 1, 3), 1);
        assert_eq!(TerminalView::remap_index_after_move(3, 1, 3), 2);
        assert_eq!(TerminalView::remap_index_after_move(0, 1, 3), 0);
    }

    #[test]
    fn remap_index_after_move_handles_move_to_left() {
        assert_eq!(TerminalView::remap_index_after_move(3, 3, 1), 1);
        assert_eq!(TerminalView::remap_index_after_move(1, 3, 1), 2);
        assert_eq!(TerminalView::remap_index_after_move(2, 3, 1), 3);
        assert_eq!(TerminalView::remap_index_after_move(4, 3, 1), 4);
    }

    #[test]
    fn remap_index_after_move_keeps_moved_tab_active() {
        assert_eq!(TerminalView::remap_index_after_move(2, 2, 1), 1);
        assert_eq!(TerminalView::remap_index_after_move(2, 2, 3), 3);
    }
}
