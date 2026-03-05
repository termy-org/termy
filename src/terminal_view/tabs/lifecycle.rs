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
                    .unwrap_or_default();
                let preferred_working_dir = self.preferred_working_dir_for_new_native_session(cx);
                let terminal = match Terminal::new_native(
                    size,
                    preferred_working_dir.as_deref(),
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
                self.scroll_active_tab_into_view();
                cx.notify();
            }
        }
    }

    pub(crate) fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.tabs.len() {
            return;
        }
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
        cx: &mut Context<Self>,
    ) -> Result<Terminal, String> {
        let preferred_working_dir = self.preferred_working_dir_for_new_native_session(cx);
        Terminal::new_native(
            TerminalSize {
                cols: cols.max(1),
                rows: rows.max(1),
                ..TerminalSize::default()
            },
            preferred_working_dir.as_deref(),
            Some(self.event_wakeup_tx.clone()),
            Some(&self.tab_shell_integration),
            Some(&self.terminal_runtime),
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
        self.clear_selection();
        self.clear_hovered_link();
        cx.notify();
        true
    }

    fn native_split_active_pane(&mut self, axis: NativeSplitAxis, cx: &mut Context<Self>) -> bool {
        let Some((active_pane_id, left, top, width, height)) =
            self.tabs.get(self.active_tab).and_then(|tab| {
                let index = tab.active_pane_index()?;
                let pane = tab.panes.get(index)?;
                Some((
                    pane.id.clone(),
                    pane.left,
                    pane.top,
                    pane.width,
                    pane.height,
                ))
            })
        else {
            return false;
        };

        let (current_size, split_size) = match axis {
            NativeSplitAxis::Vertical => {
                if width <= 1 {
                    return false;
                }
                let current_width = (width / 2).max(1);
                let split_width = width.saturating_sub(current_width).max(1);
                (
                    (left, top, current_width, height),
                    (left.saturating_add(current_width), top, split_width, height),
                )
            }
            NativeSplitAxis::Horizontal => {
                if height <= 1 {
                    return false;
                }
                let current_height = (height / 2).max(1);
                let split_height = height.saturating_sub(current_height).max(1);
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

        let terminal = match self.native_make_terminal(split_size.2, split_size.3, cx) {
            Ok(terminal) => terminal,
            Err(error) => {
                termy_toast::error(error);
                return false;
            }
        };
        terminal.set_scrollback_history(self.terminal_runtime.scrollback_history);

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
        }

        let split_pane = TerminalPane {
            id: pane_id.clone(),
            left: split_size.0,
            top: split_size.1,
            width: split_size.2,
            height: split_size.3,
            degraded: false,
            terminal,
            render_cache: RefCell::new(TerminalPaneRenderCache::default()),
        };

        tab.panes.insert(active_index + 1, split_pane);
        tab.active_pane_id = pane_id;
        self.clear_selection();
        self.clear_hovered_link();
        cx.notify();
        true
    }

    fn native_overlap_cells(a_start: u16, a_end: u16, b_start: u16, b_end: u16) -> u16 {
        let start = a_start.max(b_start);
        let end = a_end.min(b_end);
        end.saturating_sub(start)
    }

    fn native_close_expand_neighbors(panes: &mut [TerminalPane], removed: &TerminalPane) {
        if panes.is_empty() {
            return;
        }

        let removed_left = removed.left;
        let removed_top = removed.top;
        let removed_right = removed.left.saturating_add(removed.width);
        let removed_bottom = removed.top.saturating_add(removed.height);
        let removed_width = removed.width;
        let removed_height = removed.height;

        let mut left_candidates = Vec::<(usize, u16)>::new();
        let mut right_candidates = Vec::<(usize, u16)>::new();
        let mut top_candidates = Vec::<(usize, u16)>::new();
        let mut bottom_candidates = Vec::<(usize, u16)>::new();

        for (index, pane) in panes.iter().enumerate() {
            let pane_left = pane.left;
            let pane_top = pane.top;
            let pane_right = pane.left.saturating_add(pane.width);
            let pane_bottom = pane.top.saturating_add(pane.height);

            if pane_right == removed_left {
                let overlap =
                    Self::native_overlap_cells(pane_top, pane_bottom, removed_top, removed_bottom);
                if overlap > 0 {
                    left_candidates.push((index, overlap));
                }
            }

            if pane_left == removed_right {
                let overlap =
                    Self::native_overlap_cells(pane_top, pane_bottom, removed_top, removed_bottom);
                if overlap > 0 {
                    right_candidates.push((index, overlap));
                }
            }

            if pane_bottom == removed_top {
                let overlap =
                    Self::native_overlap_cells(pane_left, pane_right, removed_left, removed_right);
                if overlap > 0 {
                    top_candidates.push((index, overlap));
                }
            }

            if pane_top == removed_bottom {
                let overlap =
                    Self::native_overlap_cells(pane_left, pane_right, removed_left, removed_right);
                if overlap > 0 {
                    bottom_candidates.push((index, overlap));
                }
            }
        }

        let sum_overlap = |candidates: &[(usize, u16)]| -> u16 {
            candidates.iter().map(|(_, overlap)| *overlap).sum()
        };
        let vertical_cover_target = removed_bottom.saturating_sub(removed_top);
        let horizontal_cover_target = removed_right.saturating_sub(removed_left);

        let mut candidates = Vec::<(&str, u16)>::new();
        let left_cover = sum_overlap(&left_candidates);
        let right_cover = sum_overlap(&right_candidates);
        let top_cover = sum_overlap(&top_candidates);
        let bottom_cover = sum_overlap(&bottom_candidates);

        if left_cover > 0 {
            candidates.push(("left", left_cover));
        }
        if right_cover > 0 {
            candidates.push(("right", right_cover));
        }
        if top_cover > 0 {
            candidates.push(("top", top_cover));
        }
        if bottom_cover > 0 {
            candidates.push(("bottom", bottom_cover));
        }

        candidates.sort_by_key(|(_, coverage)| Reverse(*coverage));

        let selected = candidates.into_iter().find_map(|(direction, coverage)| {
            let target = match direction {
                "left" | "right" => vertical_cover_target,
                _ => horizontal_cover_target,
            };
            (coverage >= target).then_some(direction)
        });
        let selected = selected.or_else(|| {
            ["left", "right", "top", "bottom"]
                .into_iter()
                .find(|direction| match *direction {
                    "left" => !left_candidates.is_empty(),
                    "right" => !right_candidates.is_empty(),
                    "top" => !top_candidates.is_empty(),
                    _ => !bottom_candidates.is_empty(),
                })
        });

        match selected {
            Some("left") => {
                for (index, _) in left_candidates {
                    panes[index].width = panes[index].width.saturating_add(removed_width);
                }
            }
            Some("right") => {
                for (index, _) in right_candidates {
                    panes[index].left = panes[index].left.saturating_sub(removed_width);
                    panes[index].width = panes[index].width.saturating_add(removed_width);
                }
            }
            Some("top") => {
                for (index, _) in top_candidates {
                    panes[index].height = panes[index].height.saturating_add(removed_height);
                }
            }
            Some("bottom") => {
                for (index, _) in bottom_candidates {
                    panes[index].top = panes[index].top.saturating_sub(removed_height);
                    panes[index].height = panes[index].height.saturating_add(removed_height);
                }
            }
            _ => {}
        }
    }

    fn native_close_active_pane(&mut self, cx: &mut Context<Self>) -> bool {
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

        self.clear_selection();
        self.clear_hovered_link();
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
}
