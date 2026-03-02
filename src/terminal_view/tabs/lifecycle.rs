use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClosePaneOrTabTarget {
    ClosePane,
    CloseTab,
}

impl TerminalView {
    pub(in super::super) fn execute_tab_command_action(
        &mut self,
        action: CommandAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match action {
            CommandAction::RenameTab => {
                self.begin_rename_tab(self.active_tab, cx);
                termy_toast::info("Rename mode enabled");
                true
            }
            CommandAction::NewTab => {
                self.add_tab(cx);
                true
            }
            CommandAction::CloseTab => {
                self.request_active_tab_close(window, cx);
                true
            }
            CommandAction::ClosePaneOrTab => self.close_active_pane_or_tab(window, cx),
            CommandAction::MoveTabLeft => {
                self.move_active_tab_left(cx);
                true
            }
            CommandAction::MoveTabRight => {
                self.move_active_tab_right(cx);
                true
            }
            CommandAction::SwitchTabLeft => {
                self.switch_active_tab_left(cx);
                true
            }
            CommandAction::SwitchTabRight => {
                self.switch_active_tab_right(cx);
                true
            }
            CommandAction::SplitPaneVertical => self.split_active_pane_vertical(cx),
            CommandAction::SplitPaneHorizontal => self.split_active_pane_horizontal(cx),
            CommandAction::ClosePane => self.close_active_pane(cx),
            CommandAction::FocusPaneLeft => self.focus_pane_left(cx),
            CommandAction::FocusPaneRight => self.focus_pane_right(cx),
            CommandAction::FocusPaneUp => self.focus_pane_up(cx),
            CommandAction::FocusPaneDown => self.focus_pane_down(cx),
            CommandAction::FocusPaneNext => self.focus_pane_next(cx),
            CommandAction::FocusPanePrevious => self.focus_pane_previous(cx),
            CommandAction::ResizePaneLeft => self.resize_pane_left(cx),
            CommandAction::ResizePaneRight => self.resize_pane_right(cx),
            CommandAction::ResizePaneUp => self.resize_pane_up(cx),
            CommandAction::ResizePaneDown => self.resize_pane_down(cx),
            CommandAction::TogglePaneZoom => self.toggle_pane_zoom(cx),
            _ => false,
        }
    }

    fn close_pane_or_tab_target(runtime_kind: RuntimeKind, pane_count: usize) -> ClosePaneOrTabTarget {
        if runtime_kind.uses_tmux() && pane_count > 1 {
            ClosePaneOrTabTarget::ClosePane
        } else {
            ClosePaneOrTabTarget::CloseTab
        }
    }

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

    fn adjacent_pane_index(active_pane: usize, pane_count: usize, step: i32) -> Option<usize> {
        if pane_count <= 1 || active_pane >= pane_count {
            return None;
        }

        if step > 0 {
            Some((active_pane + 1) % pane_count)
        } else if step < 0 {
            Some((active_pane + pane_count - 1) % pane_count)
        } else {
            None
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

        match self.runtime_kind() {
            RuntimeKind::Tmux => {
                if !self.tmux_reorder_tab(from, to) {
                    return false;
                }
            }
            RuntimeKind::Native => {
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
            }
        }
        self.reset_tab_drag_state();
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

        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_switch_active_tab_left(cx),
            RuntimeKind::Native => {
                self.switch_tab(target_index, cx);
                true
            }
        }
    }

    pub(crate) fn switch_active_tab_right(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(target_index) = Self::adjacent_tab_index(self.active_tab, self.tabs.len(), true)
        else {
            return false;
        };

        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_switch_active_tab_right(cx),
            RuntimeKind::Native => {
                self.switch_tab(target_index, cx);
                true
            }
        }
    }

    pub(crate) fn add_tab(&mut self, cx: &mut Context<Self>) {
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_add_tab(cx),
            RuntimeKind::Native => {
                // Tab creation should stay robust if active pane state is transiently missing.
                let size = self
                    .active_terminal()
                    .map(|terminal| terminal.size())
                    .unwrap_or_else(TerminalSize::default);
                let terminal = match Terminal::new_native(
                    size,
                    self.configured_working_dir.as_deref(),
                    Some(self.event_wakeup_tx.clone()),
                    Some(&self.tab_shell_integration),
                    Some(&self.terminal_runtime),
                ) {
                    Ok(terminal) => terminal,
                    Err(error) => {
                        termy_toast::error(format!("Failed to create tab: {error}"));
                        return;
                    }
                };

                let predicted_prompt_cwd = Self::predicted_prompt_cwd(
                    self.configured_working_dir.as_deref(),
                    self.terminal_runtime.working_dir_fallback,
                );
                let predicted_title =
                    Self::predicted_prompt_seed_title(&self.tab_title, predicted_prompt_cwd.as_deref());

                let tab_id = self.allocate_tab_id();
                self.tabs.push(Self::create_native_tab(
                    tab_id,
                    terminal,
                    size.cols,
                    size.rows,
                    predicted_title,
                ));
                self.active_tab = self.tabs.len() - 1;
                self.refresh_tab_title(self.active_tab);
                self.mark_tab_strip_layout_dirty();
                self.reset_tab_interaction_state();
                self.scroll_active_tab_into_view();
                cx.notify();
            }
        }
    }

    pub(crate) fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }

        match self.runtime_kind() {
            RuntimeKind::Tmux => {
                self.tmux_close_tab(index, cx);
                return;
            }
            RuntimeKind::Native => {}
        };

        if self.tabs.len() <= 1 {
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

    pub(crate) fn begin_rename_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }

        if self.is_command_palette_open() {
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

        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_switch_tab(index, cx),
            RuntimeKind::Native => {
                let old_active = self.active_tab;
                self.active_tab = index;
                if self.tab_width_mode != TabWidthMode::Stable {
                    self.mark_tab_strip_layout_dirty();
                }

                if let Some(inactive_scrollback) = self.inactive_tab_scrollback {
                    for pane in &self.tabs[old_active].panes {
                        pane.terminal.set_scrollback_history(inactive_scrollback);
                    }
                    for pane in &self.tabs[index].panes {
                        pane.terminal
                            .set_scrollback_history(self.terminal_runtime.scrollback_history);
                    }
                }

                self.reset_tab_rename_state();
                self.reset_tab_drag_state();
                self.clear_selection();
                self.sync_tab_strip_for_active_tab();
                cx.notify();
            }
        }
    }

    pub(crate) fn commit_rename_tab(&mut self, cx: &mut Context<Self>) {
        let Some(index) = self.renaming_tab else {
            return;
        };

        match self.runtime_kind() {
            RuntimeKind::Tmux => {
                self.tmux_commit_rename_tab(index);
            }
            RuntimeKind::Native => {
                let trimmed = self.rename_input.text().trim();
                self.tabs[index].manual_title = (!trimmed.is_empty())
                    .then(|| Self::truncate_tab_title(trimmed))
                    .filter(|title| !title.is_empty());
                self.refresh_tab_title(index);
            }
        }

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

    pub(crate) fn focus_pane_target(&mut self, pane_id: &str, cx: &mut Context<Self>) -> bool {
        self.tmux_focus_pane_target(pane_id, cx)
    }

    pub(crate) fn split_active_pane_vertical(&mut self, cx: &mut Context<Self>) -> bool {
        self.tmux_split_active_pane_vertical(cx)
    }

    pub(crate) fn split_active_pane_horizontal(&mut self, cx: &mut Context<Self>) -> bool {
        self.tmux_split_active_pane_horizontal(cx)
    }

    pub(crate) fn close_active_pane(&mut self, cx: &mut Context<Self>) -> bool {
        self.tmux_close_active_pane(cx)
    }

    pub(crate) fn close_active_pane_or_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let pane_count = self.tabs.get(self.active_tab).map_or(0, |tab| tab.panes.len());
        match Self::close_pane_or_tab_target(self.runtime_kind(), pane_count) {
            ClosePaneOrTabTarget::ClosePane => self.close_active_pane(cx),
            ClosePaneOrTabTarget::CloseTab => {
                // tmux rejects killing the last pane in a window, so we intentionally
                // promote that case to the existing tab-close flow.
                self.request_active_tab_close(window, cx);
                true
            }
        }
    }

    pub(crate) fn focus_pane_left(&mut self, cx: &mut Context<Self>) -> bool {
        self.tmux_focus_pane_left(cx)
    }

    pub(crate) fn focus_pane_right(&mut self, cx: &mut Context<Self>) -> bool {
        self.tmux_focus_pane_right(cx)
    }

    pub(crate) fn focus_pane_up(&mut self, cx: &mut Context<Self>) -> bool {
        self.tmux_focus_pane_up(cx)
    }

    pub(crate) fn focus_pane_down(&mut self, cx: &mut Context<Self>) -> bool {
        self.tmux_focus_pane_down(cx)
    }

    fn focus_pane_cycle(&mut self, step: i32, cx: &mut Context<Self>) -> bool {
        if !self.runtime_kind().uses_tmux() {
            return false;
        }

        let Some(tab) = self.tabs.get(self.active_tab) else {
            return false;
        };
        let Some(active_pane_index) = tab.active_pane_index() else {
            return false;
        };
        let Some(target_pane_index) =
            Self::adjacent_pane_index(active_pane_index, tab.panes.len(), step)
        else {
            return false;
        };

        let target_pane_id = tab.panes[target_pane_index].id.clone();
        self.focus_pane_target(target_pane_id.as_str(), cx)
    }

    pub(crate) fn focus_pane_next(&mut self, cx: &mut Context<Self>) -> bool {
        self.focus_pane_cycle(1, cx)
    }

    pub(crate) fn focus_pane_previous(&mut self, cx: &mut Context<Self>) -> bool {
        self.focus_pane_cycle(-1, cx)
    }

    pub(crate) fn resize_pane_left(&mut self, cx: &mut Context<Self>) -> bool {
        self.tmux_resize_pane_left(cx)
    }

    pub(crate) fn resize_pane_right(&mut self, cx: &mut Context<Self>) -> bool {
        self.tmux_resize_pane_right(cx)
    }

    pub(crate) fn resize_pane_up(&mut self, cx: &mut Context<Self>) -> bool {
        self.tmux_resize_pane_up(cx)
    }

    pub(crate) fn resize_pane_down(&mut self, cx: &mut Context<Self>) -> bool {
        self.tmux_resize_pane_down(cx)
    }

    pub(crate) fn toggle_pane_zoom(&mut self, cx: &mut Context<Self>) -> bool {
        self.tmux_toggle_active_pane_zoom(cx)
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
    fn adjacent_pane_index_wraps_for_next_and_previous() {
        assert_eq!(TerminalView::adjacent_pane_index(2, 4, 1), Some(3));
        assert_eq!(TerminalView::adjacent_pane_index(3, 4, 1), Some(0));
        assert_eq!(TerminalView::adjacent_pane_index(0, 4, -1), Some(3));
        assert_eq!(TerminalView::adjacent_pane_index(2, 4, -1), Some(1));
    }

    #[test]
    fn adjacent_pane_index_is_none_for_invalid_or_no_movement() {
        assert_eq!(TerminalView::adjacent_pane_index(0, 0, 1), None);
        assert_eq!(TerminalView::adjacent_pane_index(0, 1, 1), None);
        assert_eq!(TerminalView::adjacent_pane_index(2, 2, 1), None);
        assert_eq!(TerminalView::adjacent_pane_index(0, 2, 0), None);
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

    #[test]
    fn close_pane_or_tab_target_prefers_pane_for_tmux_multi_pane_tabs() {
        assert_eq!(
            TerminalView::close_pane_or_tab_target(RuntimeKind::Tmux, 2),
            ClosePaneOrTabTarget::ClosePane
        );
    }

    #[test]
    fn close_pane_or_tab_target_falls_back_to_tab_when_last_or_non_tmux() {
        assert_eq!(
            TerminalView::close_pane_or_tab_target(RuntimeKind::Tmux, 1),
            ClosePaneOrTabTarget::CloseTab
        );
        assert_eq!(
            TerminalView::close_pane_or_tab_target(RuntimeKind::Tmux, 0),
            ClosePaneOrTabTarget::CloseTab
        );
        assert_eq!(
            TerminalView::close_pane_or_tab_target(RuntimeKind::Native, 3),
            ClosePaneOrTabTarget::CloseTab
        );
    }
}
