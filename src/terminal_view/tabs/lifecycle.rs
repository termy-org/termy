use super::*;

impl TerminalView {
    pub(in super::super) fn execute_tab_command_action(
        &mut self,
        action: CommandAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }

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
            CommandAction::ResizePaneLeft => self.resize_pane_left(cx),
            CommandAction::ResizePaneRight => self.resize_pane_right(cx),
            CommandAction::ResizePaneUp => self.resize_pane_up(cx),
            CommandAction::ResizePaneDown => self.resize_pane_down(cx),
            CommandAction::TogglePaneZoom => self.toggle_pane_zoom(cx),
            _ => false,
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

    pub(crate) fn reorder_tab(&mut self, from: usize, to: usize, cx: &mut Context<Self>) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }
        if from >= self.tabs.len() || to >= self.tabs.len() || from == to {
            return false;
        }

        let moved_window_id = self.tabs[from].window_id.clone();
        let mut window_order = self
            .tabs
            .iter()
            .map(|tab| tab.window_id.clone())
            .collect::<Vec<_>>();

        if from < to {
            for index in from..to {
                let source = window_order[index].clone();
                let target = window_order[index + 1].clone();
                if let Err(error) = self.tmux_client_required().swap_windows(source.as_str(), target.as_str()) {
                    termy_toast::error(format!("Failed to reorder tabs: {error}"));
                    return false;
                }
                window_order.swap(index, index + 1);
            }
        } else {
            for index in (to + 1..=from).rev() {
                let source = window_order[index].clone();
                let target = window_order[index - 1].clone();
                if let Err(error) = self.tmux_client_required().swap_windows(source.as_str(), target.as_str()) {
                    termy_toast::error(format!("Failed to reorder tabs: {error}"));
                    return false;
                }
                window_order.swap(index, index - 1);
            }
        }

        if !self.refresh_tmux_snapshot() {
            return false;
        }
        if let Some(index) = self
            .tabs
            .iter()
            .position(|tab| tab.window_id == moved_window_id)
        {
            self.active_tab = index;
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
        if !self.runtime_uses_tmux() {
            return false;
        }
        if self.tabs.len() <= 1 {
            return false;
        }

        if let Err(error) = self.tmux_client_required().previous_window() {
            termy_toast::error(format!("Failed to switch tab: {error}"));
            return false;
        }
        let refreshed = self.refresh_tmux_snapshot();
        if refreshed {
            self.clear_selection();
            self.scroll_active_tab_into_view();
            cx.notify();
        }
        refreshed
    }

    pub(crate) fn switch_active_tab_right(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }
        if self.tabs.len() <= 1 {
            return false;
        }

        if let Err(error) = self.tmux_client_required().next_window() {
            termy_toast::error(format!("Failed to switch tab: {error}"));
            return false;
        }
        let refreshed = self.refresh_tmux_snapshot();
        if refreshed {
            self.clear_selection();
            self.scroll_active_tab_into_view();
            cx.notify();
        }
        refreshed
    }

    pub(crate) fn add_tab(&mut self, cx: &mut Context<Self>) {
        if !self.runtime_uses_tmux() {
            return;
        }
        if let Err(error) = self.tmux_client_required().new_window() {
            termy_toast::error(format!("Failed to create tab: {error}"));
            return;
        }

        if self.refresh_tmux_snapshot() {
            self.reset_tab_interaction_state();
            self.scroll_active_tab_into_view();
            cx.notify();
        }
    }

    pub(crate) fn close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if !self.runtime_uses_tmux() {
            return;
        }
        if index >= self.tabs.len() {
            return;
        }

        let window_id = self.tabs[index].window_id.clone();
        if let Err(error) = self.tmux_client_required().kill_window(window_id.as_str()) {
            termy_toast::error(format!("Failed to close tab: {error}"));
            return;
        }

        if self.refresh_tmux_snapshot() {
            self.reset_tab_drag_state();
            self.clear_selection();
            self.scroll_active_tab_into_view();
            cx.notify();
        }
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
        if !self.runtime_uses_tmux() {
            return;
        }
        if index >= self.tabs.len() || index == self.active_tab {
            return;
        }

        let window_id = self.tabs[index].window_id.clone();
        if let Err(error) = self.tmux_client_required().select_window(window_id.as_str()) {
            termy_toast::error(format!("Failed to switch tab: {error}"));
            return;
        }

        if self.refresh_tmux_snapshot() {
            self.reset_tab_rename_state();
            self.reset_tab_drag_state();
            self.clear_selection();
            self.scroll_active_tab_into_view();
            cx.notify();
        }
    }

    pub(crate) fn commit_rename_tab(&mut self, cx: &mut Context<Self>) {
        if !self.runtime_uses_tmux() {
            self.reset_tab_rename_state();
            self.reset_tab_drag_state();
            cx.notify();
            return;
        }
        let Some(index) = self.renaming_tab else {
            return;
        };

        let trimmed = self.rename_input.text().trim();
        if !trimmed.is_empty() {
            let renamed = Self::truncate_tab_title(trimmed);
            let window_id = self.tabs[index].window_id.clone();
            if let Err(error) = self
                .tmux_client_required()
                .rename_window(window_id.as_str(), renamed.as_str())
            {
                termy_toast::error(format!("Failed to rename tab: {error}"));
            } else {
                let _ = self.refresh_tmux_snapshot();
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

    fn pane_geometry_signature(&self, pane_id: &str) -> Option<(u16, u16, u16, u16)> {
        let pane = self.pane_ref_by_id(pane_id)?;
        Some((pane.left, pane.top, pane.width, pane.height))
    }

    fn apply_focus_snapshot(
        &mut self,
        previous_pane_id: &str,
        no_change_message: Option<&str>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.refresh_tmux_snapshot() {
            return false;
        }

        let focused_pane_changed = self.active_pane_id() != Some(previous_pane_id);
        if !focused_pane_changed {
            if let Some(message) = no_change_message {
                termy_toast::info(message);
            }
            cx.notify();
            return false;
        }

        self.clear_selection();
        cx.notify();
        true
    }

    fn apply_resize_snapshot(
        &mut self,
        pane_id: &str,
        before: (u16, u16, u16, u16),
        blocked_message: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.refresh_tmux_snapshot() {
            return false;
        }

        let changed = self.pane_geometry_signature(pane_id).is_some_and(|after| after != before);
        if !changed {
            termy_toast::info(blocked_message);
        }
        cx.notify();
        changed
    }

    pub(crate) fn focus_pane_target(&mut self, pane_id: &str, cx: &mut Context<Self>) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }
        let previous_pane_id = self.active_pane_id().map(ToOwned::to_owned);
        if let Err(error) = self.tmux_client_required().select_pane(pane_id) {
            termy_toast::error(format!("Failed to focus pane: {error}"));
            return false;
        }

        if let Some(previous_pane_id) = previous_pane_id {
            self.apply_focus_snapshot(previous_pane_id.as_str(), None, cx)
        } else if self.refresh_tmux_snapshot() {
            cx.notify();
            true
        } else {
            false
        }
    }

    pub(crate) fn split_active_pane_vertical(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        if let Err(error) = self.tmux_client_required().split_vertical(pane_id.as_str()) {
            termy_toast::error(format!("Failed to split pane: {error}"));
            return false;
        }
        let refreshed = self.refresh_tmux_snapshot();
        if refreshed {
            self.clear_selection();
            cx.notify();
        }
        refreshed
    }

    pub(crate) fn split_active_pane_horizontal(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        if let Err(error) = self.tmux_client_required().split_horizontal(pane_id.as_str()) {
            termy_toast::error(format!("Failed to split pane: {error}"));
            return false;
        }
        let refreshed = self.refresh_tmux_snapshot();
        if refreshed {
            self.clear_selection();
            cx.notify();
        }
        refreshed
    }

    pub(crate) fn close_active_pane(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        if let Err(error) = self.tmux_client_required().close_pane(pane_id.as_str()) {
            termy_toast::error(format!("Failed to close pane: {error}"));
            return false;
        }
        let refreshed = self.refresh_tmux_snapshot();
        if refreshed {
            self.clear_selection();
            cx.notify();
        }
        refreshed
    }

    pub(crate) fn focus_pane_left(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        if let Err(error) = self.tmux_client_required().focus_pane_left(pane_id.as_str()) {
            termy_toast::error(format!("Failed to focus pane: {error}"));
            return false;
        }
        self.apply_focus_snapshot(pane_id.as_str(), Some("No pane to the left"), cx)
    }

    pub(crate) fn focus_pane_right(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        if let Err(error) = self.tmux_client_required().focus_pane_right(pane_id.as_str()) {
            termy_toast::error(format!("Failed to focus pane: {error}"));
            return false;
        }
        self.apply_focus_snapshot(pane_id.as_str(), Some("No pane to the right"), cx)
    }

    pub(crate) fn focus_pane_up(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        if let Err(error) = self.tmux_client_required().focus_pane_up(pane_id.as_str()) {
            termy_toast::error(format!("Failed to focus pane: {error}"));
            return false;
        }
        self.apply_focus_snapshot(pane_id.as_str(), Some("No pane above"), cx)
    }

    pub(crate) fn focus_pane_down(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        if let Err(error) = self.tmux_client_required().focus_pane_down(pane_id.as_str()) {
            termy_toast::error(format!("Failed to focus pane: {error}"));
            return false;
        }
        self.apply_focus_snapshot(pane_id.as_str(), Some("No pane below"), cx)
    }

    pub(crate) fn resize_pane_left(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        let Some(before) = self.pane_geometry_signature(pane_id.as_str()) else {
            return false;
        };
        if let Err(error) = self.tmux_client_required().resize_pane_left(pane_id.as_str(), 1) {
            termy_toast::error(format!("Failed to resize pane: {error}"));
            return false;
        }
        self.apply_resize_snapshot(
            pane_id.as_str(),
            before,
            "Pane cannot resize further to the left",
            cx,
        )
    }

    pub(crate) fn resize_pane_right(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        let Some(before) = self.pane_geometry_signature(pane_id.as_str()) else {
            return false;
        };
        if let Err(error) = self.tmux_client_required().resize_pane_right(pane_id.as_str(), 1) {
            termy_toast::error(format!("Failed to resize pane: {error}"));
            return false;
        }
        self.apply_resize_snapshot(
            pane_id.as_str(),
            before,
            "Pane cannot resize further to the right",
            cx,
        )
    }

    pub(crate) fn resize_pane_up(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        let Some(before) = self.pane_geometry_signature(pane_id.as_str()) else {
            return false;
        };
        if let Err(error) = self.tmux_client_required().resize_pane_up(pane_id.as_str(), 1) {
            termy_toast::error(format!("Failed to resize pane: {error}"));
            return false;
        }
        self.apply_resize_snapshot(
            pane_id.as_str(),
            before,
            "Pane cannot resize further upward",
            cx,
        )
    }

    pub(crate) fn resize_pane_down(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        let Some(before) = self.pane_geometry_signature(pane_id.as_str()) else {
            return false;
        };
        if let Err(error) = self.tmux_client_required().resize_pane_down(pane_id.as_str(), 1) {
            termy_toast::error(format!("Failed to resize pane: {error}"));
            return false;
        }
        self.apply_resize_snapshot(
            pane_id.as_str(),
            before,
            "Pane cannot resize further downward",
            cx,
        )
    }

    pub(crate) fn toggle_pane_zoom(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        if let Err(error) = self.tmux_client_required().toggle_pane_zoom(pane_id.as_str()) {
            termy_toast::error(format!("Failed to toggle pane zoom: {error}"));
            return false;
        }
        let refreshed = self.refresh_tmux_snapshot();
        if refreshed {
            cx.notify();
        }
        refreshed
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
}
