use super::*;
use termy_terminal_ui::TmuxClient;

fn reorder_active_window_id<'a>(
    previous_active_window_id: Option<&'a str>,
    moved_window_id: &'a str,
) -> &'a str {
    previous_active_window_id.unwrap_or(moved_window_id)
}

impl TerminalView {
    fn warn_stale_tmux_tab_index(action: &str, index: usize, tab_count: usize) {
        log::warn!(
            "Ignoring tmux {action} tab request for stale index {index}; current tab count is {tab_count}"
        );
    }

    pub(in crate::terminal_view) fn run_tmux_action<F>(&self, error_prefix: &str, action: F) -> bool
    where
        F: FnOnce(&TmuxClient) -> anyhow::Result<()>,
    {
        if !self.runtime_uses_tmux() {
            return false;
        }

        if let Err(error) = action(&self.tmux_runtime().client) {
            termy_toast::error(format!("{error_prefix}: {error}"));
            return false;
        }

        true
    }

    fn run_tmux_action_with_refresh<F>(
        &mut self,
        error_prefix: &str,
        refresh: TmuxPostActionRefresh,
        clear_selection: bool,
        cx: &mut Context<Self>,
        action: F,
    ) -> bool
    where
        F: FnOnce(&TmuxClient) -> anyhow::Result<()>,
    {
        if !self.run_tmux_action(error_prefix, action) {
            return false;
        }

        match refresh {
            TmuxPostActionRefresh::ImmediateSnapshot => {
                if !self.refresh_tmux_snapshot() {
                    return false;
                }
                if clear_selection {
                    self.clear_selection();
                }
                cx.notify();
                true
            }
            TmuxPostActionRefresh::EventDriven => {
                if clear_selection {
                    self.clear_selection();
                }
                let _ = self.event_wakeup_tx.try_send(());
                cx.notify();
                true
            }
        }
    }

    pub(in crate::terminal_view) fn tmux_send_input_to_active_pane(&self, input: &[u8]) -> bool {
        let Some(active_pane_id) = self.active_pane_id() else {
            return false;
        };
        let Some(tmux) = self.runtime.as_tmux() else {
            return false;
        };
        match tmux.client.send_input(active_pane_id, input) {
            Ok(()) => true,
            Err(error) => {
                termy_toast::error(format!("Input write failed: {error}"));
                false
            }
        }
    }

    pub(in crate::terminal_view) fn tmux_resize_pane_step(
        &mut self,
        pane_id: &str,
        axis: PaneResizeAxis,
        positive_direction: bool,
    ) -> bool {
        let resized = self.run_tmux_action("Failed to resize pane", |tmux_client| {
            match (axis, positive_direction) {
                (PaneResizeAxis::Horizontal, true) => tmux_client.resize_pane_right(pane_id, 1),
                (PaneResizeAxis::Horizontal, false) => tmux_client.resize_pane_left(pane_id, 1),
                (PaneResizeAxis::Vertical, true) => tmux_client.resize_pane_down(pane_id, 1),
                (PaneResizeAxis::Vertical, false) => tmux_client.resize_pane_up(pane_id, 1),
            }
        });
        if resized {
            let _ = self.event_wakeup_tx.try_send(());
        }
        resized
    }

    pub(in crate::terminal_view) fn tmux_reorder_tab(&mut self, from: usize, to: usize) -> bool {
        if from == to {
            return false;
        }
        let Some(moved_window_id) = self.tabs.get(from).map(|tab| tab.window_id.clone()) else {
            log::warn!(
                "Ignoring tmux reorder tab request for stale source index {from} -> {to}; current tab count is {}",
                self.tabs.len()
            );
            return false;
        };
        if self.tabs.get(to).is_none() {
            log::warn!(
                "Ignoring tmux reorder tab request for stale destination index {from} -> {to} (window_id={moved_window_id}); current tab count is {}",
                self.tabs.len()
            );
            return false;
        }
        let previous_active_window_id = self
            .tabs
            .get(self.active_tab)
            .map(|tab| tab.window_id.clone());
        let mut window_order = self
            .tabs
            .iter()
            .map(|tab| tab.window_id.clone())
            .collect::<Vec<_>>();
        let mut swapped_any = false;

        if from < to {
            for index in from..to {
                let source = window_order[index].clone();
                let target = window_order[index + 1].clone();
                if !self.run_tmux_action("Failed to reorder tabs", |tmux_client| {
                    tmux_client.swap_windows(source.as_str(), target.as_str())
                }) {
                    // Swap-window is incremental. If any earlier step succeeded, force
                    // a snapshot refresh so local tab order cannot drift from tmux.
                    if swapped_any {
                        let _ = self.refresh_tmux_snapshot();
                    }
                    return false;
                }
                window_order.swap(index, index + 1);
                swapped_any = true;
            }
        } else {
            for index in (to + 1..=from).rev() {
                let source = window_order[index].clone();
                let target = window_order[index - 1].clone();
                if !self.run_tmux_action("Failed to reorder tabs", |tmux_client| {
                    tmux_client.swap_windows(source.as_str(), target.as_str())
                }) {
                    if swapped_any {
                        let _ = self.refresh_tmux_snapshot();
                    }
                    return false;
                }
                window_order.swap(index, index - 1);
                swapped_any = true;
            }
        }

        if !self.refresh_tmux_snapshot() {
            return false;
        }

        // Preserve previously active tab identity when reordering an inactive tab.
        // Native runtime already behaves this way via index remapping.
        let active_target_window_id = reorder_active_window_id(
            previous_active_window_id.as_deref(),
            moved_window_id.as_str(),
        );
        if let Some(index) = self
            .tabs
            .iter()
            .position(|tab| tab.window_id == active_target_window_id)
        {
            self.active_tab = index;
        }

        true
    }

    pub(in crate::terminal_view) fn tmux_switch_active_tab_left(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.run_tmux_action("Failed to switch tab", |tmux_client| {
            tmux_client.previous_window()
        }) {
            return false;
        }
        let refreshed = self.refresh_tmux_snapshot();
        if refreshed {
            self.clear_selection();
            self.reset_tab_rename_state();
            self.reset_tab_drag_state();
            cx.notify();
        }
        refreshed
    }

    pub(in crate::terminal_view) fn tmux_switch_active_tab_right(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.run_tmux_action("Failed to switch tab", |tmux_client| {
            tmux_client.next_window()
        }) {
            return false;
        }
        let refreshed = self.refresh_tmux_snapshot();
        if refreshed {
            self.clear_selection();
            self.reset_tab_rename_state();
            self.reset_tab_drag_state();
            cx.notify();
        }
        refreshed
    }

    pub(in crate::terminal_view) fn tmux_add_tab(&mut self, cx: &mut Context<Self>) {
        let Some(active_window_id) = self
            .tabs
            .get(self.active_tab)
            .map(|tab| tab.window_id.clone())
        else {
            termy_toast::error("Failed to create tab: active tmux window is unavailable");
            return;
        };

        if !self.run_tmux_action("Failed to create tab", |tmux_client| {
            tmux_client.new_window_after(active_window_id.as_str())
        }) {
            return;
        }

        if self.refresh_tmux_snapshot() {
            self.reset_tab_interaction_state();
            cx.notify();
        }
    }

    pub(in crate::terminal_view) fn tmux_close_tab(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(window_id) = self.tabs.get(index).map(|tab| tab.window_id.clone()) else {
            Self::warn_stale_tmux_tab_index("close", index, self.tabs.len());
            return;
        };
        if !self.run_tmux_action("Failed to close tab", |tmux_client| {
            tmux_client.kill_window(window_id.as_str())
        }) {
            return;
        }

        if self.refresh_tmux_snapshot() {
            self.reset_tab_rename_state();
            self.reset_tab_drag_state();
            self.clear_selection();
            cx.notify();
        }
    }

    pub(in crate::terminal_view) fn tmux_switch_tab(
        &mut self,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(window_id) = self.tabs.get(index).map(|tab| tab.window_id.clone()) else {
            Self::warn_stale_tmux_tab_index("switch", index, self.tabs.len());
            return;
        };
        if !self.run_tmux_action("Failed to switch tab", |tmux_client| {
            tmux_client.select_window(window_id.as_str())
        }) {
            return;
        }

        if self.refresh_tmux_snapshot() {
            self.reset_tab_rename_state();
            self.reset_tab_drag_state();
            self.clear_selection();
            cx.notify();
        }
    }

    pub(in crate::terminal_view) fn tmux_commit_rename_tab(&mut self, index: usize) {
        let trimmed = self.rename_input.text().trim();
        if trimmed.is_empty() {
            return;
        }

        let renamed = Self::truncate_tab_title(trimmed);
        let Some(window_id) = self.tabs.get(index).map(|tab| tab.window_id.clone()) else {
            Self::warn_stale_tmux_tab_index("rename", index, self.tabs.len());
            return;
        };
        if self.run_tmux_action("Failed to rename tab", |tmux_client| {
            tmux_client.rename_window(window_id.as_str(), renamed.as_str())
        }) {
            let _ = self.refresh_tmux_snapshot();
        }
    }

    pub(in crate::terminal_view) fn tmux_focus_pane_target(
        &mut self,
        pane_id: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        self.run_tmux_action_with_refresh(
            "Failed to focus pane",
            TmuxPostActionRefresh::EventDriven,
            true,
            cx,
            |tmux_client| tmux_client.select_pane(pane_id),
        )
    }

    fn with_active_pane_action<F>(
        &mut self,
        error_prefix: &str,
        refresh: TmuxPostActionRefresh,
        clear_selection: bool,
        cx: &mut Context<Self>,
        action: F,
    ) -> bool
    where
        F: FnOnce(&TmuxClient, &str) -> anyhow::Result<()>,
    {
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };

        self.run_tmux_action_with_refresh(
            error_prefix,
            refresh,
            clear_selection,
            cx,
            |tmux_client| action(tmux_client, pane_id.as_str()),
        )
    }

    pub(in crate::terminal_view) fn tmux_split_active_pane_vertical(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.with_active_pane_action(
            "Failed to split pane",
            TmuxPostActionRefresh::ImmediateSnapshot,
            true,
            cx,
            |tmux_client, pane_id| tmux_client.split_vertical(pane_id),
        )
    }

    pub(in crate::terminal_view) fn tmux_split_active_pane_horizontal(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.with_active_pane_action(
            "Failed to split pane",
            TmuxPostActionRefresh::ImmediateSnapshot,
            true,
            cx,
            |tmux_client, pane_id| tmux_client.split_horizontal(pane_id),
        )
    }

    pub(in crate::terminal_view) fn tmux_close_active_pane(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.with_active_pane_action(
            "Failed to close pane",
            TmuxPostActionRefresh::ImmediateSnapshot,
            true,
            cx,
            |tmux_client, pane_id| tmux_client.close_pane(pane_id),
        )
    }

    pub(in crate::terminal_view) fn tmux_focus_pane_left(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.with_active_pane_action(
            "Failed to focus pane",
            TmuxPostActionRefresh::EventDriven,
            true,
            cx,
            |tmux_client, pane_id| tmux_client.focus_pane_left(pane_id),
        )
    }

    pub(in crate::terminal_view) fn tmux_focus_pane_right(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.with_active_pane_action(
            "Failed to focus pane",
            TmuxPostActionRefresh::EventDriven,
            true,
            cx,
            |tmux_client, pane_id| tmux_client.focus_pane_right(pane_id),
        )
    }

    pub(in crate::terminal_view) fn tmux_focus_pane_up(&mut self, cx: &mut Context<Self>) -> bool {
        self.with_active_pane_action(
            "Failed to focus pane",
            TmuxPostActionRefresh::EventDriven,
            true,
            cx,
            |tmux_client, pane_id| tmux_client.focus_pane_up(pane_id),
        )
    }

    pub(in crate::terminal_view) fn tmux_focus_pane_down(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.with_active_pane_action(
            "Failed to focus pane",
            TmuxPostActionRefresh::EventDriven,
            true,
            cx,
            |tmux_client, pane_id| tmux_client.focus_pane_down(pane_id),
        )
    }

    pub(in crate::terminal_view) fn tmux_resize_pane_left(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.with_active_pane_action(
            "Failed to resize pane",
            TmuxPostActionRefresh::EventDriven,
            false,
            cx,
            |tmux_client, pane_id| tmux_client.resize_pane_left(pane_id, 1),
        )
    }

    pub(in crate::terminal_view) fn tmux_resize_pane_right(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.with_active_pane_action(
            "Failed to resize pane",
            TmuxPostActionRefresh::EventDriven,
            false,
            cx,
            |tmux_client, pane_id| tmux_client.resize_pane_right(pane_id, 1),
        )
    }

    pub(in crate::terminal_view) fn tmux_resize_pane_up(&mut self, cx: &mut Context<Self>) -> bool {
        self.with_active_pane_action(
            "Failed to resize pane",
            TmuxPostActionRefresh::EventDriven,
            false,
            cx,
            |tmux_client, pane_id| tmux_client.resize_pane_up(pane_id, 1),
        )
    }

    pub(in crate::terminal_view) fn tmux_resize_pane_down(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.with_active_pane_action(
            "Failed to resize pane",
            TmuxPostActionRefresh::EventDriven,
            false,
            cx,
            |tmux_client, pane_id| tmux_client.resize_pane_down(pane_id, 1),
        )
    }

    pub(in crate::terminal_view) fn tmux_toggle_active_pane_zoom(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        self.with_active_pane_action(
            "Failed to toggle pane zoom",
            TmuxPostActionRefresh::ImmediateSnapshot,
            false,
            cx,
            |tmux_client, pane_id| tmux_client.toggle_pane_zoom(pane_id),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::reorder_active_window_id;

    #[test]
    fn reorder_active_window_id_preserves_previously_active_window() {
        assert_eq!(reorder_active_window_id(Some("@2"), "@3"), "@2");
        assert_eq!(reorder_active_window_id(None, "@3"), "@3");
    }
}
