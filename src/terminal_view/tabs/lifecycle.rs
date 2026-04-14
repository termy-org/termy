use super::*;
use std::cmp::Reverse;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClosePaneOrTabTarget {
    ClosePane,
    CloseTab,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeSplitAxis {
    Vertical,
    Horizontal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeFocusDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeCloseDirection {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct NativeCloseCandidate {
    direction: NativeCloseDirection,
    pane_indices: Vec<usize>,
    coverage: u16,
    required_coverage: u16,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NativePaneRect {
    left: u16,
    top: u16,
    width: u16,
    height: u16,
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
            CommandAction::SwitchToTab1 => self.switch_to_tab_position(1, cx),
            CommandAction::SwitchToTab2 => self.switch_to_tab_position(2, cx),
            CommandAction::SwitchToTab3 => self.switch_to_tab_position(3, cx),
            CommandAction::SwitchToTab4 => self.switch_to_tab_position(4, cx),
            CommandAction::SwitchToTab5 => self.switch_to_tab_position(5, cx),
            CommandAction::SwitchToTab6 => self.switch_to_tab_position(6, cx),
            CommandAction::SwitchToTab7 => self.switch_to_tab_position(7, cx),
            CommandAction::SwitchToTab8 => self.switch_to_tab_position(8, cx),
            CommandAction::SwitchToTab9 => self.switch_to_tab_position(9, cx),
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

    fn switch_to_tab_position(&mut self, position: usize, cx: &mut Context<Self>) -> bool {
        let Some(target_index) = position.checked_sub(1) else {
            return false;
        };
        if target_index >= self.tabs.len() {
            return false;
        }
        self.switch_tab(target_index, cx);
        true
    }

    fn close_pane_or_tab_target(
        _runtime_kind: RuntimeKind,
        pane_count: usize,
    ) -> ClosePaneOrTabTarget {
        if pane_count > 1 {
            ClosePaneOrTabTarget::ClosePane
        } else {
            ClosePaneOrTabTarget::CloseTab
        }
    }

    fn adjacent_tab_index(active_tab: usize, tab_count: usize, to_right: bool) -> Option<usize> {
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
                self.schedule_persist_native_workspace();
            }
        }
        self.reset_tab_drag_state();
        self.scroll_active_tab_into_view(self.tab_strip_orientation());
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
        self.add_tab_with_working_dir(None, cx);
    }

    pub(crate) fn add_tab_with_working_dir(
        &mut self,
        working_dir: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_add_tab(working_dir, cx),
            RuntimeKind::Native => {
                // Tab creation should stay robust if active pane state is transiently missing.
                let size = self
                    .active_terminal()
                    .map(|terminal| terminal.size())
                    .unwrap_or_default();
                let preferred_working_dir =
                    self.preferred_working_dir_for_new_session(working_dir, cx);
                let terminal = match Terminal::new_native(
                    size,
                    preferred_working_dir.as_deref(),
                    Some(self.event_wakeup_tx.clone()),
                    Some(&self.tab_shell_integration),
                    Some(&self.terminal_runtime),
                    None,
                ) {
                    Ok(terminal) => terminal,
                    Err(error) => {
                        termy_toast::error(format!("Failed to create tab: {error}"));
                        return;
                    }
                };

                let predicted_prompt_cwd = Self::predicted_prompt_cwd(
                    preferred_working_dir.as_deref(),
                    self.terminal_runtime.working_dir_fallback,
                );
                let predicted_title = Self::predicted_prompt_seed_title(
                    &self.tab_title,
                    predicted_prompt_cwd.as_deref(),
                );

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
                self.scroll_active_tab_into_view(self.tab_strip_orientation());
                self.schedule_persist_native_workspace();
                self.start_new_tab_animation(tab_id, cx);
                cx.notify();
            }
        }
    }

    pub(crate) fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() || self.tabs[index].pinned {
            return;
        }
        let removed_tab_id = self.tabs[index].id;
        let removed_pane_ids = self.tabs[index]
            .panes
            .iter()
            .map(|pane| pane.id.clone())
            .collect::<Vec<_>>();
        let _ = self.release_forwarded_mouse_presses_for_panes(&removed_pane_ids);

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

        self.capture_agent_session_id_for_tab(index);
        let removed_agent_snapshot = self.agent_thread_archive_snapshot_for_tab(index);
        self.tabs.remove(index);
        self.native_pane_zoom_snapshots.remove(&removed_tab_id);
        self.mark_tab_strip_layout_dirty();
        if let Some((thread_id, title, current_command, status_label, status_detail)) =
            removed_agent_snapshot
        {
            self.archive_agent_thread_snapshot(
                thread_id.as_deref(),
                title.as_str(),
                current_command.as_deref(),
                status_label.as_deref(),
                status_detail.as_deref(),
            );
        }

        if self.active_tab > index {
            self.active_tab -= 1;
        } else if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }

        self.sync_agent_workspace_to_active_tab();

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
        self.scroll_active_tab_into_view(self.tab_strip_orientation());
        self.schedule_persist_native_workspace();
        cx.notify();
    }

    pub(crate) fn tab_index_by_id(&self, tab_id: TabId) -> Option<usize> {
        self.tabs.iter().position(|tab| tab.id == tab_id)
    }

    pub(crate) fn set_tab_pinned(
        &mut self,
        index: usize,
        pinned: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab) = self.tabs.get_mut(index) else {
            return false;
        };
        if tab.pinned == pinned {
            return false;
        }

        tab.pinned = pinned;
        self.mark_tab_strip_layout_dirty();
        if self.runtime_kind() == RuntimeKind::Native {
            self.schedule_persist_native_workspace();
        }
        cx.notify();
        true
    }

    pub(crate) fn set_tab_pinned_by_id(
        &mut self,
        tab_id: TabId,
        pinned: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(index) = self.tab_index_by_id(tab_id) else {
            return false;
        };
        self.set_tab_pinned(index, pinned, cx)
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
                    let active_options = self.terminal_runtime.term_options();
                    let inactive_options =
                        active_options.with_scrollback_history(inactive_scrollback);
                    for pane in &self.tabs[old_active].panes {
                        pane.terminal.set_term_options(inactive_options);
                    }
                    for pane in &self.tabs[index].panes {
                        pane.terminal.set_term_options(active_options);
                    }
                }

                self.reset_tab_rename_state();
                self.reset_tab_drag_state();
                self.clear_selection();
                self.sync_agent_workspace_to_active_tab();
                self.sync_tab_strip_for_active_tab();
                self.schedule_persist_native_workspace();
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
                self.schedule_persist_native_workspace();
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
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_focus_pane_target(pane_id, cx),
            RuntimeKind::Native => self.native_focus_pane_target(pane_id, cx),
        }
    }

    pub(crate) fn split_active_pane_vertical(&mut self, cx: &mut Context<Self>) -> bool {
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_split_active_pane_vertical(cx),
            RuntimeKind::Native => self.native_split_active_pane(NativeSplitAxis::Vertical, cx),
        }
    }

    pub(crate) fn split_active_pane_horizontal(&mut self, cx: &mut Context<Self>) -> bool {
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_split_active_pane_horizontal(cx),
            RuntimeKind::Native => self.native_split_active_pane(NativeSplitAxis::Horizontal, cx),
        }
    }

    pub(crate) fn close_active_pane(&mut self, cx: &mut Context<Self>) -> bool {
        if let Some(active_pane_id) = self.active_pane_id().map(str::to_string) {
            let _ = self
                .release_forwarded_mouse_presses_for_panes(std::slice::from_ref(&active_pane_id));
        }
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_close_active_pane(cx),
            RuntimeKind::Native => self.native_close_active_pane(cx),
        }
    }

    pub(crate) fn close_active_pane_or_tab(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let pane_count = self
            .tabs
            .get(self.active_tab)
            .map_or(0, |tab| tab.panes.len());
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
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_focus_pane_left(cx),
            RuntimeKind::Native => self.native_focus_pane_direction(NativeFocusDirection::Left, cx),
        }
    }

    pub(crate) fn focus_pane_right(&mut self, cx: &mut Context<Self>) -> bool {
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_focus_pane_right(cx),
            RuntimeKind::Native => {
                self.native_focus_pane_direction(NativeFocusDirection::Right, cx)
            }
        }
    }

    pub(crate) fn focus_pane_up(&mut self, cx: &mut Context<Self>) -> bool {
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_focus_pane_up(cx),
            RuntimeKind::Native => self.native_focus_pane_direction(NativeFocusDirection::Up, cx),
        }
    }

    pub(crate) fn focus_pane_down(&mut self, cx: &mut Context<Self>) -> bool {
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_focus_pane_down(cx),
            RuntimeKind::Native => self.native_focus_pane_direction(NativeFocusDirection::Down, cx),
        }
    }

    fn focus_pane_cycle(&mut self, step: i32, cx: &mut Context<Self>) -> bool {
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
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_resize_pane_left(cx),
            RuntimeKind::Native => self.native_resize_active_pane(
                PaneResizeAxis::Horizontal,
                PaneResizeEdge::Left,
                -1,
                cx,
            ),
        }
    }

    pub(crate) fn resize_pane_right(&mut self, cx: &mut Context<Self>) -> bool {
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_resize_pane_right(cx),
            RuntimeKind::Native => self.native_resize_active_pane(
                PaneResizeAxis::Horizontal,
                PaneResizeEdge::Right,
                1,
                cx,
            ),
        }
    }

    pub(crate) fn resize_pane_up(&mut self, cx: &mut Context<Self>) -> bool {
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_resize_pane_up(cx),
            RuntimeKind::Native => self.native_resize_active_pane(
                PaneResizeAxis::Vertical,
                PaneResizeEdge::Top,
                -1,
                cx,
            ),
        }
    }

    pub(crate) fn resize_pane_down(&mut self, cx: &mut Context<Self>) -> bool {
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_resize_pane_down(cx),
            RuntimeKind::Native => self.native_resize_active_pane(
                PaneResizeAxis::Vertical,
                PaneResizeEdge::Bottom,
                1,
                cx,
            ),
        }
    }

    pub(crate) fn toggle_pane_zoom(&mut self, cx: &mut Context<Self>) -> bool {
        match self.runtime_kind() {
            RuntimeKind::Tmux => self.tmux_toggle_active_pane_zoom(cx),
            RuntimeKind::Native => self.native_toggle_active_pane_zoom(cx),
        }
    }

    fn clear_native_zoom_snapshot_for_active_tab(&mut self) {
        if let Some(tab) = self.tabs.get(self.active_tab) {
            self.native_pane_zoom_snapshots.remove(&tab.id);
        }
    }

    fn native_resize_active_pane(
        &mut self,
        axis: PaneResizeAxis,
        edge: PaneResizeEdge,
        divider_delta: i16,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(active_pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        self.clear_native_zoom_snapshot_for_active_tab();
        if self.native_resize_pane_step(active_pane_id.as_str(), axis, edge, divider_delta)
            != PaneResizeResult::Applied
        {
            return false;
        }
        self.clear_selection();
        self.clear_hovered_link();
        self.schedule_persist_native_workspace();
        cx.notify();
        true
    }

    fn native_toggle_active_pane_zoom(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(tab_id) = self.tabs.get(self.active_tab).map(|tab| tab.id) else {
            return false;
        };

        if let Some(snapshot) = self.native_pane_zoom_snapshots.remove(&tab_id) {
            let Some(tab) = self.tabs.get_mut(self.active_tab) else {
                return false;
            };
            let Some(mut active_pane) = tab.panes.pop() else {
                return false;
            };
            active_pane.left = snapshot.active_pane_geometry.0;
            active_pane.top = snapshot.active_pane_geometry.1;
            active_pane.width = snapshot.active_pane_geometry.2;
            active_pane.height = snapshot.active_pane_geometry.3;

            let mut panes = snapshot.other_panes;
            let insert_index = snapshot.active_original_index.min(panes.len());
            panes.insert(insert_index, active_pane);
            tab.panes = panes;
            tab.active_pane_id = snapshot.active_pane_id;
            tab.assert_active_pane_invariant();

            self.clear_selection();
            self.clear_hovered_link();
            self.schedule_persist_native_workspace();
            cx.notify();
            return true;
        }

        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return false;
        };
        if tab.panes.len() <= 1 {
            return false;
        }
        let active_pane_id = tab.active_pane_id.clone();
        let Some(active_index) = tab.active_pane_index() else {
            return false;
        };

        let max_cols = tab
            .panes
            .iter()
            .map(|pane| pane.left.saturating_add(pane.width))
            .max()
            .unwrap_or(1)
            .max(1);
        let max_rows = tab
            .panes
            .iter()
            .map(|pane| pane.top.saturating_add(pane.height))
            .max()
            .unwrap_or(1)
            .max(1);

        let mut panes = std::mem::take(&mut tab.panes);
        let mut active_pane = panes.remove(active_index);
        let active_geometry = (
            active_pane.left,
            active_pane.top,
            active_pane.width,
            active_pane.height,
        );
        active_pane.left = 0;
        active_pane.top = 0;
        active_pane.width = max_cols;
        active_pane.height = max_rows;
        tab.panes = vec![active_pane];
        tab.active_pane_id = active_pane_id.clone();
        tab.assert_active_pane_invariant();

        self.native_pane_zoom_snapshots.insert(
            tab_id,
            NativePaneZoomSnapshot {
                other_panes: panes,
                active_pane_geometry: active_geometry,
                active_pane_id,
                active_original_index: active_index,
            },
        );

        self.clear_selection();
        self.clear_hovered_link();
        self.schedule_persist_native_workspace();
        cx.notify();
        true
    }

    fn native_allocate_pane_id(&self) -> String {
        let mut next = 1u64;
        loop {
            let candidate = format!("%native-pane-{next}");
            if self.pane_terminal_by_id(candidate.as_str()).is_none() {
                return candidate;
            }
            next = next.saturating_add(1);
        }
    }

    fn native_make_terminal(
        &mut self,
        cols: u16,
        rows: u16,
        cell_size: Size<Pixels>,
        cx: &mut Context<Self>,
    ) -> Result<Terminal, String> {
        let preferred_working_dir = self.preferred_working_dir_for_new_session(None, cx);
        Terminal::new_native(
            TerminalSize {
                cols: cols.max(1),
                rows: rows.max(1),
                cell_width: cell_size.width,
                cell_height: cell_size.height,
            },
            preferred_working_dir.as_deref(),
            Some(self.event_wakeup_tx.clone()),
            Some(&self.tab_shell_integration),
            Some(&self.terminal_runtime),
            None,
        )
        .map_err(|error| format!("Failed to split pane: {error}"))
    }

    fn native_focus_pane_target(&mut self, pane_id: &str, cx: &mut Context<Self>) -> bool {
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return false;
        };
        if tab.active_pane_id == pane_id {
            return false;
        }
        if !tab.panes.iter().any(|pane| pane.id == pane_id) {
            return false;
        }

        tab.active_pane_id = pane_id.to_string();
        tab.assert_active_pane_invariant();
        self.clear_selection();
        self.clear_hovered_link();
        self.schedule_persist_native_workspace();
        cx.notify();
        true
    }

    fn native_split_active_pane(&mut self, axis: NativeSplitAxis, cx: &mut Context<Self>) -> bool {
        self.clear_native_zoom_snapshot_for_active_tab();
        let Some((active_pane_id, left, top, width, height, pane_zoom_steps)) =
            self.tabs.get(self.active_tab).and_then(|tab| {
                let index = tab.active_pane_index()?;
                let pane = tab.panes.get(index)?;
                Some((
                    pane.id.clone(),
                    pane.left,
                    pane.top,
                    pane.width,
                    pane.height,
                    pane.pane_zoom_steps,
                ))
            })
        else {
            return false;
        };

        let (current_size, split_size) = match axis {
            NativeSplitAxis::Vertical => {
                let min_width = Self::native_pane_min_extent_for_axis(PaneResizeAxis::Horizontal);
                if width < min_width.saturating_mul(2) {
                    termy_toast::info(format!(
                        "Pane needs at least {} columns to split vertically",
                        min_width.saturating_mul(2)
                    ));
                    self.notify_overlay(cx);
                    return false;
                }
                let current_width = (width / 2).max(min_width);
                let split_width = width.saturating_sub(current_width).max(min_width);
                (
                    (left, top, current_width, height),
                    (left.saturating_add(current_width), top, split_width, height),
                )
            }
            NativeSplitAxis::Horizontal => {
                let min_height = Self::native_pane_min_extent_for_axis(PaneResizeAxis::Vertical);
                if height < min_height.saturating_mul(2) {
                    termy_toast::info(format!(
                        "Pane needs at least {} rows to split horizontally",
                        min_height.saturating_mul(2)
                    ));
                    self.notify_overlay(cx);
                    return false;
                }
                let current_height = (height / 2).max(min_height);
                let split_height = height.saturating_sub(current_height).max(min_height);
                (
                    (left, top, width, current_height),
                    (
                        left,
                        top.saturating_add(current_height),
                        width,
                        split_height,
                    ),
                )
            }
        };

        let cell_size = self.layout_cell_size();
        let terminal = match self.native_make_terminal(split_size.2, split_size.3, cell_size, cx) {
            Ok(terminal) => terminal,
            Err(error) => {
                termy_toast::error(error);
                return false;
            }
        };
        let pane_id = self.native_allocate_pane_id();
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return false;
        };
        let Some(active_index) = tab.panes.iter().position(|pane| pane.id == active_pane_id) else {
            return false;
        };

        if let Some(active_pane) = tab.panes.get_mut(active_index) {
            active_pane.left = current_size.0;
            active_pane.top = current_size.1;
            active_pane.width = current_size.2;
            active_pane.height = current_size.3;
            // Resize terminal immediately to avoid visual "crump" on first render
            active_pane.terminal.resize(TerminalSize {
                cols: current_size.2,
                rows: current_size.3,
                cell_width: cell_size.width,
                cell_height: cell_size.height,
            });
        }

        let cached_element_ids = PaneCachedElementIds::new(&pane_id);
        let split_pane = TerminalPane {
            id: pane_id.clone(),
            left: split_size.0,
            top: split_size.1,
            width: split_size.2,
            height: split_size.3,
            pane_zoom_steps,
            degraded: false,
            terminal,
            render_cache: RefCell::new(TerminalPaneRenderCache::default()),
            last_alternate_screen: Cell::new(false),
            cached_element_ids,
        };

        tab.panes.insert(active_index + 1, split_pane);
        tab.active_pane_id = pane_id;
        tab.assert_active_pane_invariant();
        self.clear_selection();
        self.clear_hovered_link();
        self.schedule_persist_native_workspace();
        cx.notify();
        true
    }

    fn native_overlap_cells(a_start: u16, a_end: u16, b_start: u16, b_end: u16) -> u16 {
        let start = a_start.max(b_start);
        let end = a_end.min(b_end);
        end.saturating_sub(start)
    }

    fn native_pane_rect_from_pane(pane: &TerminalPane) -> NativePaneRect {
        NativePaneRect {
            left: pane.left,
            top: pane.top,
            width: pane.width,
            height: pane.height,
        }
    }

    fn native_pane_rects_overlap(a: NativePaneRect, b: NativePaneRect) -> bool {
        let a_right = a.left.saturating_add(a.width);
        let a_bottom = a.top.saturating_add(a.height);
        let b_right = b.left.saturating_add(b.width);
        let b_bottom = b.top.saturating_add(b.height);
        a.left < b_right && b.left < a_right && a.top < b_bottom && b.top < a_bottom
    }

    fn native_close_direction_coverage(
        mut intervals: Vec<(u16, u16)>,
        target_start: u16,
        target_end: u16,
    ) -> u16 {
        if intervals.is_empty() || target_start >= target_end {
            return 0;
        }

        intervals.sort_unstable_by_key(|&(start, end)| (start, end));
        let mut coverage = 0u16;
        let mut current = intervals[0];

        for interval in intervals.into_iter().skip(1) {
            if interval.0 <= current.1 {
                current.1 = current.1.max(interval.1);
                continue;
            }

            coverage = coverage.saturating_add(current.1.saturating_sub(current.0));
            current = interval;
        }

        coverage
            .saturating_add(current.1.saturating_sub(current.0))
            .min(target_end.saturating_sub(target_start))
    }

    fn native_close_expand_pane_rects(
        pane_rects: &mut [NativePaneRect],
        pane_indices: &[usize],
        removed: &TerminalPane,
        direction: NativeCloseDirection,
    ) {
        let removed_width = removed.width;
        let removed_height = removed.height;

        match direction {
            NativeCloseDirection::Left => {
                for &index in pane_indices {
                    pane_rects[index].width = pane_rects[index].width.saturating_add(removed_width);
                }
            }
            NativeCloseDirection::Right => {
                for &index in pane_indices {
                    pane_rects[index].left = pane_rects[index].left.saturating_sub(removed_width);
                    pane_rects[index].width = pane_rects[index].width.saturating_add(removed_width);
                }
            }
            NativeCloseDirection::Top => {
                for &index in pane_indices {
                    pane_rects[index].height =
                        pane_rects[index].height.saturating_add(removed_height);
                }
            }
            NativeCloseDirection::Bottom => {
                for &index in pane_indices {
                    pane_rects[index].top = pane_rects[index].top.saturating_sub(removed_height);
                    pane_rects[index].height =
                        pane_rects[index].height.saturating_add(removed_height);
                }
            }
        }
    }

    fn native_close_direction_preserves_layout(
        panes: &[TerminalPane],
        removed: &TerminalPane,
        candidate: &NativeCloseCandidate,
    ) -> bool {
        if candidate.pane_indices.is_empty() {
            return false;
        }

        let mut pane_rects = panes
            .iter()
            .map(Self::native_pane_rect_from_pane)
            .collect::<Vec<_>>();
        Self::native_close_expand_pane_rects(
            &mut pane_rects,
            &candidate.pane_indices,
            removed,
            candidate.direction,
        );

        for left_index in 0..pane_rects.len() {
            for right_index in left_index + 1..pane_rects.len() {
                if Self::native_pane_rects_overlap(pane_rects[left_index], pane_rects[right_index])
                {
                    return false;
                }
            }
        }

        true
    }

    fn native_close_apply_candidate(
        panes: &mut [TerminalPane],
        removed: &TerminalPane,
        candidate: &NativeCloseCandidate,
    ) {
        let mut pane_rects = panes
            .iter()
            .map(Self::native_pane_rect_from_pane)
            .collect::<Vec<_>>();
        Self::native_close_expand_pane_rects(
            &mut pane_rects,
            &candidate.pane_indices,
            removed,
            candidate.direction,
        );

        for (pane, rect) in panes.iter_mut().zip(pane_rects) {
            pane.left = rect.left;
            pane.top = rect.top;
            pane.width = rect.width;
            pane.height = rect.height;
        }
    }

    fn native_close_expand_neighbors(panes: &mut [TerminalPane], removed: &TerminalPane) {
        if panes.is_empty() {
            return;
        }

        let removed_left = removed.left;
        let removed_top = removed.top;
        let removed_right = removed.left.saturating_add(removed.width);
        let removed_bottom = removed.top.saturating_add(removed.height);

        let mut left_candidates = Vec::<usize>::new();
        let mut right_candidates = Vec::<usize>::new();
        let mut top_candidates = Vec::<usize>::new();
        let mut bottom_candidates = Vec::<usize>::new();
        let mut left_intervals = Vec::<(u16, u16)>::new();
        let mut right_intervals = Vec::<(u16, u16)>::new();
        let mut top_intervals = Vec::<(u16, u16)>::new();
        let mut bottom_intervals = Vec::<(u16, u16)>::new();

        for (index, pane) in panes.iter().enumerate() {
            let pane_left = pane.left;
            let pane_top = pane.top;
            let pane_right = pane.left.saturating_add(pane.width);
            let pane_bottom = pane.top.saturating_add(pane.height);

            if pane_right == removed_left {
                let overlap =
                    Self::native_overlap_cells(pane_top, pane_bottom, removed_top, removed_bottom);
                if overlap > 0 {
                    left_candidates.push(index);
                    left_intervals
                        .push((pane_top.max(removed_top), pane_bottom.min(removed_bottom)));
                }
            }

            if pane_left == removed_right {
                let overlap =
                    Self::native_overlap_cells(pane_top, pane_bottom, removed_top, removed_bottom);
                if overlap > 0 {
                    right_candidates.push(index);
                    right_intervals
                        .push((pane_top.max(removed_top), pane_bottom.min(removed_bottom)));
                }
            }

            if pane_bottom == removed_top {
                let overlap =
                    Self::native_overlap_cells(pane_left, pane_right, removed_left, removed_right);
                if overlap > 0 {
                    top_candidates.push(index);
                    top_intervals
                        .push((pane_left.max(removed_left), pane_right.min(removed_right)));
                }
            }

            if pane_top == removed_bottom {
                let overlap =
                    Self::native_overlap_cells(pane_left, pane_right, removed_left, removed_right);
                if overlap > 0 {
                    bottom_candidates.push(index);
                    bottom_intervals
                        .push((pane_left.max(removed_left), pane_right.min(removed_right)));
                }
            }
        }

        let vertical_cover_target = removed_bottom.saturating_sub(removed_top);
        let horizontal_cover_target = removed_right.saturating_sub(removed_left);

        let mut candidates = vec![
            NativeCloseCandidate {
                direction: NativeCloseDirection::Left,
                pane_indices: left_candidates,
                coverage: Self::native_close_direction_coverage(
                    left_intervals,
                    removed_top,
                    removed_bottom,
                ),
                required_coverage: vertical_cover_target,
            },
            NativeCloseCandidate {
                direction: NativeCloseDirection::Right,
                pane_indices: right_candidates,
                coverage: Self::native_close_direction_coverage(
                    right_intervals,
                    removed_top,
                    removed_bottom,
                ),
                required_coverage: vertical_cover_target,
            },
            NativeCloseCandidate {
                direction: NativeCloseDirection::Top,
                pane_indices: top_candidates,
                coverage: Self::native_close_direction_coverage(
                    top_intervals,
                    removed_left,
                    removed_right,
                ),
                required_coverage: horizontal_cover_target,
            },
            NativeCloseCandidate {
                direction: NativeCloseDirection::Bottom,
                pane_indices: bottom_candidates,
                coverage: Self::native_close_direction_coverage(
                    bottom_intervals,
                    removed_left,
                    removed_right,
                ),
                required_coverage: horizontal_cover_target,
            },
        ];

        candidates.sort_by_key(|candidate| Reverse(candidate.coverage));

        if let Some(candidate) = candidates.iter().find(|candidate| {
            candidate.coverage >= candidate.required_coverage
                && Self::native_close_direction_preserves_layout(panes, removed, candidate)
        }) {
            Self::native_close_apply_candidate(panes, removed, candidate);
            return;
        }

        if let Some(candidate) = candidates.iter().find(|candidate| {
            !candidate.pane_indices.is_empty()
                && Self::native_close_direction_preserves_layout(panes, removed, candidate)
        }) {
            Self::native_close_apply_candidate(panes, removed, candidate);
            return;
        }

        log::warn!(
            "native pane close could not find a non-overlapping expansion target for removed pane {}",
            removed.id
        );
    }

    fn native_close_active_pane(&mut self, cx: &mut Context<Self>) -> bool {
        self.clear_native_zoom_snapshot_for_active_tab();
        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return false;
        };
        if tab.panes.len() <= 1 {
            return false;
        }
        let Some(active_index) = tab.active_pane_index() else {
            return false;
        };

        let removed = tab.panes.remove(active_index);
        Self::native_close_expand_neighbors(&mut tab.panes, &removed);

        let next_index = active_index.min(tab.panes.len().saturating_sub(1));
        if let Some(next) = tab.panes.get(next_index) {
            tab.active_pane_id = next.id.clone();
        }
        if tab.panes.len() == 1
            && let Some(remaining_pane) = tab.panes.first_mut()
        {
            remaining_pane.pane_zoom_steps = 0;
        }
        tab.assert_active_pane_invariant();

        self.clear_selection();
        self.clear_hovered_link();
        self.clear_terminal_scrollbar_marker_cache();
        self.schedule_persist_native_workspace();
        cx.notify();
        true
    }

    fn native_focus_pane_direction(
        &mut self,
        direction: NativeFocusDirection,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return false;
        };
        let Some(active_index) = tab.active_pane_index() else {
            return false;
        };
        let Some(active) = tab.panes.get(active_index) else {
            return false;
        };

        let active_left = active.left;
        let active_top = active.top;
        let active_right = active.left.saturating_add(active.width);
        let active_bottom = active.top.saturating_add(active.height);

        let mut best: Option<(u16, Reverse<u16>, String)> = None;
        for pane in &tab.panes {
            if pane.id == active.id {
                continue;
            }

            let pane_left = pane.left;
            let pane_top = pane.top;
            let pane_right = pane.left.saturating_add(pane.width);
            let pane_bottom = pane.top.saturating_add(pane.height);

            let (distance, overlap) = match direction {
                NativeFocusDirection::Left => {
                    let overlap = Self::native_overlap_cells(
                        active_top,
                        active_bottom,
                        pane_top,
                        pane_bottom,
                    );
                    if overlap == 0 || pane_right > active_left {
                        continue;
                    }
                    (active_left.saturating_sub(pane_right), overlap)
                }
                NativeFocusDirection::Right => {
                    let overlap = Self::native_overlap_cells(
                        active_top,
                        active_bottom,
                        pane_top,
                        pane_bottom,
                    );
                    if overlap == 0 || pane_left < active_right {
                        continue;
                    }
                    (pane_left.saturating_sub(active_right), overlap)
                }
                NativeFocusDirection::Up => {
                    let overlap = Self::native_overlap_cells(
                        active_left,
                        active_right,
                        pane_left,
                        pane_right,
                    );
                    if overlap == 0 || pane_bottom > active_top {
                        continue;
                    }
                    (active_top.saturating_sub(pane_bottom), overlap)
                }
                NativeFocusDirection::Down => {
                    let overlap = Self::native_overlap_cells(
                        active_left,
                        active_right,
                        pane_left,
                        pane_right,
                    );
                    if overlap == 0 || pane_top < active_bottom {
                        continue;
                    }
                    (pane_top.saturating_sub(active_bottom), overlap)
                }
            };

            let candidate = (distance, Reverse(overlap), pane.id.clone());
            if best
                .as_ref()
                .is_none_or(|current| (candidate.0, candidate.1) < (current.0, current.1))
            {
                best = Some(candidate);
            }
        }

        let Some((_, _, pane_id)) = best else {
            return false;
        };
        self.native_focus_pane_target(pane_id.as_str(), cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_terminal() -> Terminal {
        Terminal::new_tmux(TerminalSize::default(), TerminalOptions::default())
    }

    fn test_pane(id: &str, left: u16, top: u16, width: u16, height: u16) -> TerminalPane {
        TerminalPane {
            id: id.to_string(),
            left,
            top,
            width,
            height,
            pane_zoom_steps: 0,
            degraded: false,
            terminal: test_terminal(),
            render_cache: RefCell::new(TerminalPaneRenderCache::default()),
            last_alternate_screen: Cell::new(false),
            cached_element_ids: PaneCachedElementIds::new(id),
        }
    }

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
    fn close_pane_or_tab_target_falls_back_to_tab_when_last_pane() {
        assert_eq!(
            TerminalView::close_pane_or_tab_target(RuntimeKind::Tmux, 1),
            ClosePaneOrTabTarget::CloseTab
        );
        assert_eq!(
            TerminalView::close_pane_or_tab_target(RuntimeKind::Tmux, 0),
            ClosePaneOrTabTarget::CloseTab
        );
    }

    #[test]
    fn close_pane_or_tab_target_prefers_pane_when_multiple_exist() {
        assert_eq!(
            TerminalView::close_pane_or_tab_target(RuntimeKind::Native, 3),
            ClosePaneOrTabTarget::ClosePane
        );
    }

    #[test]
    fn native_close_expand_neighbors_avoids_expanding_into_overlapping_layout() {
        let removed = test_pane("%native-1", 0, 0, 60, 20);
        let mut panes = vec![
            test_pane("%native-2", 60, 0, 60, 20),
            test_pane("%native-3", 0, 20, 120, 20),
        ];

        TerminalView::native_close_expand_neighbors(&mut panes, &removed);

        assert_eq!(panes[0].left, 0);
        assert_eq!(panes[0].top, 0);
        assert_eq!(panes[0].width, 120);
        assert_eq!(panes[0].height, 20);
        assert_eq!(panes[1].left, 0);
        assert_eq!(panes[1].top, 20);
        assert_eq!(panes[1].width, 120);
        assert_eq!(panes[1].height, 20);
        assert!(!TerminalView::native_pane_rects_overlap(
            TerminalView::native_pane_rect_from_pane(&panes[0]),
            TerminalView::native_pane_rect_from_pane(&panes[1]),
        ));
    }
}
