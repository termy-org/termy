use super::super::*;
use super::TmuxResizeWakeup;

impl TerminalView {
    pub(in super::super) fn tmux_client_cols(&self) -> u16 {
        self.tmux_runtime().client_cols
    }

    pub(in super::super) fn tmux_client_rows(&self) -> u16 {
        self.tmux_runtime().client_rows
    }

    pub(in super::super) fn sync_tmux_client_size(
        &mut self,
        cols: u16,
        rows: u16,
    ) -> anyhow::Result<()> {
        {
            let runtime = self.tmux_runtime_mut();
            runtime.client.set_client_size(cols, rows)?;
            runtime.client_cols = cols;
            runtime.client_rows = rows;
        }
        // Snapshot convergence is scheduled asynchronously so UI resize never blocks.
        self.request_tmux_resize_convergence(cols, rows);
        Ok(())
    }

    pub(in super::super) fn reconnect_tmux_runtime(&mut self, next_config: TmuxRuntimeConfig) {
        if !self.runtime_uses_tmux() {
            return;
        }

        if self.tmux_runtime().config == next_config {
            return;
        }

        let cols = self.tmux_runtime().client_cols.max(1);
        let rows = self.tmux_runtime().client_rows.max(1);
        match TmuxClient::new(next_config.clone(), cols, rows, Some(self.event_wakeup_tx.clone())) {
            Ok(client) => {
                let runtime = self.tmux_runtime_mut();
                runtime.config = next_config;
                runtime.client = client;
                runtime.resize_scheduler.clear();
                runtime.resize_wakeup_scheduled = false;
                runtime.title_refresh_deadline = None;
                runtime.title_refresh_wakeup_scheduled = false;
                let _ = self.refresh_tmux_snapshot();
            }
            Err(error) => {
                termy_toast::error(format!("tmux reconnect failed: {error}"));
            }
        }
    }

    pub(in super::super) fn run_tmux_action<F>(&self, error_prefix: &str, action: F) -> bool
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
    ) -> Terminal {
        let terminal = Terminal::new_tmux(
            Self::terminal_size_for_pane_state(pane, cell_size),
            scrollback_history,
        );

        if let Ok(capture) = tmux_client.capture_pane_viewport(&pane.id, pane.height.max(1)) {
            terminal.feed_output(&capture);
            let cursor_row = pane.cursor_y.min(pane.height.saturating_sub(1)).saturating_add(1);
            let cursor_col = pane.cursor_x.min(pane.width.saturating_sub(1)).saturating_add(1);
            let cursor_escape = format!("\u{1b}[{};{}H", cursor_row, cursor_col);
            terminal.feed_output(cursor_escape.as_bytes());
        }

        terminal
    }

    pub(in super::super) fn apply_tmux_snapshot(&mut self, snapshot: TmuxSnapshot) {
        let previous_active_window_id = self.tabs.get(self.active_tab).map(|tab| tab.window_id.clone());
        let previous_ids = self
            .tabs
            .iter()
            .map(|tab| (tab.window_id.clone(), tab.id))
            .collect::<std::collections::HashMap<_, _>>();

        let mut existing_terminals = std::collections::HashMap::<String, Terminal>::new();
        for mut tab in std::mem::take(&mut self.tabs) {
            for pane in tab.panes.drain(..) {
                existing_terminals.insert(pane.id.clone(), pane.terminal);
            }
        }

        let mut new_tabs = Vec::new();
        for window in &snapshot.windows {
            let mut panes = Vec::new();
            for pane_state in &window.panes {
                let terminal = if let Some(existing) = existing_terminals.remove(&pane_state.id) {
                    existing
                } else {
                    Self::hydrate_pane_terminal(
                        &self.tmux_runtime().client,
                        pane_state,
                        self.terminal_runtime.scrollback_history,
                        self.cell_size,
                    )
                };
                let next_size = Self::terminal_size_for_pane_state(pane_state, self.cell_size);
                let current_size = terminal.size();
                if current_size.cols != next_size.cols
                    || current_size.rows != next_size.rows
                    || current_size.cell_width != next_size.cell_width
                    || current_size.cell_height != next_size.cell_height
                {
                    terminal.resize(next_size);
                }
                panes.push(TerminalPane::from_tmux_state(pane_state, terminal));
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
    }

    pub(in super::super) fn refresh_tmux_snapshot(&mut self) -> bool {
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

    fn snapshot_matches_client_size(snapshot: &TmuxSnapshot, cols: u16, rows: u16) -> bool {
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

    pub(in super::super) fn request_tmux_resize_convergence(&mut self, cols: u16, rows: u16) {
        self.tmux_runtime_mut().resize_scheduler.request_resize(cols, rows);
        let _ = self.event_wakeup_tx.try_send(());
    }

    pub(in super::super) fn clear_tmux_resize_convergence(&mut self) {
        let runtime = self.tmux_runtime_mut();
        runtime.resize_scheduler.clear();
        runtime.resize_wakeup_scheduled = false;
    }

    fn ensure_tmux_resize_convergence_wakeup(&mut self, cx: &mut Context<Self>) {
        if !self.runtime_uses_tmux() || !self.tmux_runtime().resize_scheduler.has_work() {
            return;
        }

        match self.tmux_runtime().resize_scheduler.next_wakeup(Instant::now()) {
            TmuxResizeWakeup::None => {}
            TmuxResizeWakeup::Immediate => {
                let _ = self.event_wakeup_tx.try_send(());
            }
            TmuxResizeWakeup::Delayed(delay) => {
                if self.tmux_runtime().resize_wakeup_scheduled {
                    return;
                }

                self.tmux_runtime_mut().resize_wakeup_scheduled = true;
                let wakeup_tx = self.event_wakeup_tx.clone();
                cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                    smol::Timer::after(delay).await;
                    let _ = cx.update(|cx| {
                        this.update(cx, |view, _cx| {
                            view.tmux_runtime_mut().resize_wakeup_scheduled = false;
                            if view.runtime_uses_tmux() && view.tmux_runtime().resize_scheduler.has_work() {
                                let _ = wakeup_tx.try_send(());
                            }
                        })
                    });
                })
                .detach();
            }
        }
    }

    pub(in super::super) fn drive_tmux_resize_convergence(&mut self, cx: &mut Context<Self>) -> bool {
        let mut should_redraw = false;
        if let Some(attempt) = self
            .tmux_runtime_mut()
            .resize_scheduler
            .claim_attempt(Instant::now())
        {
            match self.tmux_runtime().client.refresh_snapshot() {
                Ok(snapshot) => {
                    let converged = Self::snapshot_matches_client_size(&snapshot, attempt.cols, attempt.rows);
                    self.apply_tmux_snapshot(snapshot);
                    should_redraw = true;
                    self.tmux_runtime_mut()
                        .resize_scheduler
                        .complete_attempt(Instant::now(), converged);
                }
                Err(error) => {
                    termy_toast::error(format!("tmux sync failed: {error}"));
                    self.clear_tmux_resize_convergence();
                }
            }
        }

        self.ensure_tmux_resize_convergence_wakeup(cx);
        should_redraw
    }

    pub(in super::super) fn schedule_tmux_title_refresh(&mut self) {
        self.tmux_runtime_mut().title_refresh_deadline =
            Some(Instant::now() + Duration::from_millis(TMUX_TITLE_REFRESH_DEBOUNCE_MS));
        let _ = self.event_wakeup_tx.try_send(());
    }

    fn ensure_tmux_title_refresh_wakeup(&mut self, cx: &mut Context<Self>) {
        if !self.runtime_uses_tmux()
            || self.tmux_runtime().title_refresh_deadline.is_none()
            || self.tmux_runtime().title_refresh_wakeup_scheduled
        {
            return;
        }

        self.tmux_runtime_mut().title_refresh_wakeup_scheduled = true;
        let wakeup_tx = self.event_wakeup_tx.clone();
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            smol::Timer::after(Duration::from_millis(TMUX_TITLE_REFRESH_DEBOUNCE_MS)).await;
            let _ = cx.update(|cx| {
                this.update(cx, |view, _cx| {
                    view.tmux_runtime_mut().title_refresh_wakeup_scheduled = false;
                    if view.runtime_uses_tmux() && view.tmux_runtime().title_refresh_deadline.is_some() {
                        let _ = wakeup_tx.try_send(());
                    }
                })
            });
        })
        .detach();
    }

    pub(in super::super) fn tmux_snapshot_refresh_mode(
        needs_refresh: bool,
        title_refresh_deadline: Option<Instant>,
        now: Instant,
    ) -> TmuxSnapshotRefreshMode {
        if needs_refresh {
            return TmuxSnapshotRefreshMode::Immediate;
        }

        if title_refresh_deadline.is_some_and(|deadline| now >= deadline) {
            return TmuxSnapshotRefreshMode::Debounced;
        }

        TmuxSnapshotRefreshMode::None
    }

    pub(in super::super) fn process_tmux_terminal_events(&mut self, cx: &mut Context<Self>) -> bool {
        let mut should_redraw = false;
        let mut needs_refresh = false;

        for notification in self.tmux_runtime().client.poll_notifications() {
            match notification {
                TmuxNotification::Output { pane_id, bytes } => {
                    if let Some(terminal) = self.pane_terminal_by_id(&pane_id) {
                        terminal.feed_output(&bytes);
                        if self.is_active_pane_id(&pane_id) {
                            should_redraw = true;
                            self.schedule_tmux_title_refresh();
                        }
                    }
                }
                TmuxNotification::NeedsRefresh => {
                    needs_refresh = true;
                }
                TmuxNotification::Exit(reason) => {
                    let reason = reason.unwrap_or_else(|| "tmux control mode exited".to_string());
                    termy_toast::error(reason);
                    cx.quit();
                }
            }
        }

        self.ensure_tmux_title_refresh_wakeup(cx);
        let now = Instant::now();
        match Self::tmux_snapshot_refresh_mode(needs_refresh, self.tmux_runtime().title_refresh_deadline, now)
        {
            TmuxSnapshotRefreshMode::Immediate | TmuxSnapshotRefreshMode::Debounced => {
                {
                    let runtime = self.tmux_runtime_mut();
                    runtime.title_refresh_deadline = None;
                    runtime.title_refresh_wakeup_scheduled = false;
                }
                if self.refresh_tmux_snapshot() {
                    should_redraw = true;
                }
            }
            TmuxSnapshotRefreshMode::None => {}
        }

        if self.drive_tmux_resize_convergence(cx) {
            should_redraw = true;
        }

        should_redraw
    }
}
