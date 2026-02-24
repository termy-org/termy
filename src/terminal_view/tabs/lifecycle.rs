use super::*;

impl TerminalView {
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

    pub(crate) fn add_tab(&mut self, cx: &mut Context<Self>) {
        let inherited_working_dir = self.tabs.get(self.active_tab).and_then(|tab| {
            tab.terminal
                .foreground_working_directory()
                .map(|cwd| cwd.to_string_lossy().into_owned())
                .or_else(|| tab.working_dir.clone())
        });
        let startup_working_dir = inherited_working_dir
            .as_deref()
            .or(self.configured_working_dir.as_deref());

        let terminal = Terminal::new(
            TerminalSize::default(),
            startup_working_dir,
            Some(self.event_wakeup_tx.clone()),
            Some(&self.tab_shell_integration),
            Some(&self.terminal_runtime),
        )
        .expect("Failed to create terminal tab");

        let predicted_prompt_cwd = Self::predicted_prompt_cwd(
            startup_working_dir,
            self.terminal_runtime.working_dir_fallback,
        );
        let predicted_title =
            Self::predicted_prompt_seed_title(&self.tab_title, predicted_prompt_cwd.as_deref());

        let tab_id = self.allocate_tab_id();
        self.tabs.push(TerminalTab::new(
            tab_id,
            terminal,
            predicted_title,
            startup_working_dir.map(|cwd| cwd.to_string()),
        ));
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
}
