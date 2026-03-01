use super::*;
use termy_terminal_ui::{TmuxClient, TmuxLaunchTarget, TmuxPaneState, TmuxSnapshot};

impl TerminalView {
    fn terminal_size_for_pane_state(
        pane: &TmuxPaneState,
        cell_size: Option<Size<Pixels>>,
    ) -> TerminalSize {
        let default_size = TerminalSize::default();
        let (cell_width, cell_height) = if let Some(cell_size) = cell_size {
            (cell_size.width, cell_size.height)
        } else {
            (default_size.cell_width, default_size.cell_height)
        };

        TerminalSize {
            cols: pane.width.max(1),
            rows: pane.height.max(1),
            cell_width,
            cell_height,
        }
    }

    fn hydrate_pane_terminal(
        tmux_client: &TmuxClient,
        pane: &TmuxPaneState,
        scrollback_history: usize,
        cell_size: Option<Size<Pixels>>,
        hydration_mode: TmuxPaneHydrationMode,
    ) -> (Terminal, Option<String>) {
        let terminal = Terminal::new_tmux(
            Self::terminal_size_for_pane_state(pane, cell_size),
            scrollback_history,
        );

        // Managed persistent sessions can restore tmux history; all other modes
        // keep viewport-only hydration to avoid unexpected startup cost changes.
        let capture = match hydration_mode {
            TmuxPaneHydrationMode::ViewportOnly => tmux_client.capture_pane_viewport(&pane.id),
            TmuxPaneHydrationMode::FullHistory => tmux_client.capture_pane(&pane.id),
        };

        match capture {
            Ok(capture) => {
                terminal.feed_output(&capture);
                if matches!(hydration_mode, TmuxPaneHydrationMode::ViewportOnly) {
                    let cursor_row = pane.cursor_y.min(pane.height.saturating_sub(1)).saturating_add(1);
                    let cursor_col = pane.cursor_x.min(pane.width.saturating_sub(1)).saturating_add(1);
                    let cursor_escape = format!("\u{1b}[{};{}H", cursor_row, cursor_col);
                    terminal.feed_output(cursor_escape.as_bytes());
                }
                (terminal, None)
            }
            Err(error) => {
                // Snapshot-driven pane rehydrate must stay non-fatal: create an empty
                // terminal buffer, mark the pane degraded, and surface one warning later.
                (terminal, Some(error.to_string()))
            }
        }
    }

    fn tmux_pane_hydration_mode_for_launch(
        tmux_persist_scrollback: bool,
        launch: &TmuxLaunchTarget,
    ) -> TmuxPaneHydrationMode {
        if tmux_persist_scrollback
            && matches!(launch, TmuxLaunchTarget::Managed { persistence: true })
        {
            return TmuxPaneHydrationMode::FullHistory;
        }

        TmuxPaneHydrationMode::ViewportOnly
    }

    fn tmux_pane_hydration_mode(&self) -> TmuxPaneHydrationMode {
        Self::tmux_pane_hydration_mode_for_launch(
            self.tmux_persist_scrollback,
            &self.tmux_runtime().config.launch,
        )
    }

    pub(super) fn apply_tmux_snapshot_rehydrate(&mut self, snapshot: TmuxSnapshot) {
        self.apply_tmux_snapshot_inner(snapshot, false);
    }

    fn apply_tmux_snapshot_inner(&mut self, snapshot: TmuxSnapshot, reuse_existing_terminals: bool) {
        let previous_active_window_id = self.tabs.get(self.active_tab).map(|tab| tab.window_id.clone());
        let previous_ids = self
            .tabs
            .iter()
            .map(|tab| (tab.window_id.clone(), tab.id))
            .collect::<std::collections::HashMap<_, _>>();
        let hydration_mode = self.tmux_pane_hydration_mode();

        let mut existing_terminals = std::collections::HashMap::<String, Terminal>::new();
        let old_tabs = std::mem::take(&mut self.tabs);
        if reuse_existing_terminals {
            for mut tab in old_tabs {
                for pane in tab.panes.drain(..) {
                    existing_terminals.insert(pane.id.clone(), pane.terminal);
                }
            }
        }

        let mut new_tabs = Vec::new();
        let mut hydration_failures = Vec::<String>::new();
        for window in &snapshot.windows {
            let mut panes = Vec::new();
            for pane_state in &window.panes {
                let (terminal, degraded, hydration_error) =
                    if let Some(existing) = existing_terminals.remove(&pane_state.id) {
                        (existing, false, None)
                } else {
                        let (terminal, hydration_error) = Self::hydrate_pane_terminal(
                            &self.tmux_runtime().client,
                            pane_state,
                            self.terminal_runtime.scrollback_history,
                            self.cell_size,
                            hydration_mode,
                        );
                        (terminal, hydration_error.is_some(), hydration_error)
                    };

                if let Some(hydration_error) = hydration_error {
                    hydration_failures.push(format!("{} ({hydration_error})", pane_state.id));
                }

                let next_size = Self::terminal_size_for_pane_state(pane_state, self.cell_size);
                let current_size = terminal.size();
                if current_size.cols != next_size.cols
                    || current_size.rows != next_size.rows
                    || current_size.cell_width != next_size.cell_width
                    || current_size.cell_height != next_size.cell_height
                {
                    terminal.resize(next_size);
                }
                panes.push(TerminalPane::from_tmux_state(pane_state, terminal, degraded));
            }

            let tab_id = previous_ids
                .get(&window.id)
                .copied()
                .unwrap_or_else(|| self.allocate_tab_id());
            let active_pane_state = window
                .active_pane_id
                .as_deref()
                .and_then(|pane_id| window.panes.iter().find(|pane| pane.id == pane_id))
                .or_else(|| window.panes.first());
            let manual_title = (!window.automatic_rename)
                .then_some(window.name.trim())
                .and_then(|name| (!name.is_empty()).then(|| Self::truncate_tab_title(name)));
            let shell_title =
                active_pane_state.and_then(|pane| Self::derive_tmux_shell_title(&self.tab_title, pane));
            let running_process =
                active_pane_state.is_some_and(|pane| !Self::is_shell_command(pane.current_command.as_str()));

            let mut tab = TerminalTab::from_tmux_window(tab_id, window, panes);
            tab.manual_title = manual_title;
            tab.shell_title = shell_title;
            tab.running_process = running_process;
            new_tabs.push(tab);
        }

        new_tabs.sort_by_key(|tab| tab.window_index);
        self.tabs = new_tabs;

        let mut next_id = 1;
        for tab in &self.tabs {
            next_id = next_id.max(tab.id.saturating_add(1));
        }
        self.next_tab_id = next_id;

        let active_index_by_window = snapshot
            .windows
            .iter()
            .find(|window| window.is_active)
            .and_then(|window| self.tabs.iter().position(|tab| tab.window_id == window.id));
        let previous_index = previous_active_window_id
            .as_deref()
            .and_then(|window_id| self.tabs.iter().position(|tab| tab.window_id == window_id));
        self.active_tab = active_index_by_window
            .or(previous_index)
            .unwrap_or(0)
            .min(self.tabs.len().saturating_sub(1));

        if self.tabs.is_empty() {
            self.active_tab = 0;
        }
        if self.renaming_tab.is_some_and(|index| index >= self.tabs.len()) {
            self.renaming_tab = None;
        }
        for index in 0..self.tabs.len() {
            self.refresh_tab_title(index);
        }
        let inactive_history = self
            .inactive_tab_scrollback
            .unwrap_or(self.terminal_runtime.scrollback_history);
        for (tab_index, tab) in self.tabs.iter().enumerate() {
            let history = if tab_index == self.active_tab {
                self.terminal_runtime.scrollback_history
            } else {
                inactive_history
            };
            for pane in &tab.panes {
                pane.terminal.set_scrollback_history(history);
            }
        }
        self.mark_tab_strip_layout_dirty();
        self.scroll_active_tab_into_view();

        if let Some(message) = tmux_hydration_warning_message(&hydration_failures) {
            termy_toast::warning(message);
        }
    }

    pub(in crate::terminal_view) fn apply_tmux_snapshot(&mut self, snapshot: TmuxSnapshot) {
        self.apply_tmux_snapshot_inner(snapshot, true);
    }

    pub(in crate::terminal_view) fn refresh_tmux_snapshot(&mut self) -> bool {
        match self.tmux_runtime().client.refresh_snapshot() {
            Ok(snapshot) => {
                self.apply_tmux_snapshot(snapshot);
                true
            }
            Err(error) => {
                termy_toast::error(format!("tmux sync failed: {error}"));
                false
            }
        }
    }

    pub(super) fn snapshot_matches_client_size(snapshot: &TmuxSnapshot, cols: u16, rows: u16) -> bool {
        let expected_cols = u32::from(cols.max(1));
        let expected_rows = u32::from(rows.max(1));
        snapshot
            .windows
            .iter()
            .filter(|window| !window.panes.is_empty())
            .all(|window| {
                let max_right = window
                    .panes
                    .iter()
                    .map(|pane| u32::from(pane.left).saturating_add(u32::from(pane.width)))
                    .max()
                    .unwrap_or(0);
                let max_bottom = window
                    .panes
                    .iter()
                    .map(|pane| u32::from(pane.top).saturating_add(u32::from(pane.height)))
                    .max()
                    .unwrap_or(0);
                let min_left = window
                    .panes
                    .iter()
                    .map(|pane| u32::from(pane.left))
                    .min()
                    .unwrap_or(0);
                let min_top = window
                    .panes
                    .iter()
                    .map(|pane| u32::from(pane.top))
                    .min()
                    .unwrap_or(0);
                max_right == expected_cols
                    && max_bottom == expected_rows
                    && min_left == 0
                    && min_top == 0
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmux_pane_hydration_mode_uses_full_history_for_persistent_managed_runtime_with_flag() {
        let mode = TerminalView::tmux_pane_hydration_mode_for_launch(
            true,
            &TmuxLaunchTarget::Managed { persistence: true },
        );
        assert_eq!(mode, TmuxPaneHydrationMode::FullHistory);
    }

    #[test]
    fn tmux_pane_hydration_mode_uses_viewport_when_flag_is_disabled() {
        let mode = TerminalView::tmux_pane_hydration_mode_for_launch(
            false,
            &TmuxLaunchTarget::Managed { persistence: true },
        );
        assert_eq!(mode, TmuxPaneHydrationMode::ViewportOnly);
    }

    #[test]
    fn tmux_pane_hydration_mode_uses_viewport_for_non_persistent_managed_runtime() {
        let mode = TerminalView::tmux_pane_hydration_mode_for_launch(
            true,
            &TmuxLaunchTarget::Managed { persistence: false },
        );
        assert_eq!(mode, TmuxPaneHydrationMode::ViewportOnly);
    }

    #[test]
    fn tmux_pane_hydration_mode_uses_viewport_for_explicit_session_launch() {
        let mode = TerminalView::tmux_pane_hydration_mode_for_launch(
            true,
            &TmuxLaunchTarget::Session {
                name: "work".to_string(),
                socket: termy_terminal_ui::TmuxSocketTarget::Named("work".to_string()),
            },
        );
        assert_eq!(mode, TmuxPaneHydrationMode::ViewportOnly);
    }
}
