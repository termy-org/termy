use super::super::*;
use super::TmuxResizeWakeup;
use termy_terminal_ui::{
    TmuxClient, TmuxLaunchTarget, TmuxNotification, TmuxPaneState, TmuxRuntimeConfig,
    TmuxSnapshot, TmuxWindowState,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TmuxSnapshotRefreshMode {
    None,
    Debounced,
    Immediate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TmuxPaneHydrationMode {
    ViewportOnly,
    FullHistory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TmuxCutoverCleanupDecision {
    Proceed,
    AbortOldCleanupFailure,
    AbortOldAndNewCleanupFailure,
}

fn tmux_cutover_cleanup_decision(
    old_cleanup_succeeded: bool,
    new_cleanup_succeeded: bool,
) -> TmuxCutoverCleanupDecision {
    if old_cleanup_succeeded {
        TmuxCutoverCleanupDecision::Proceed
    } else if new_cleanup_succeeded {
        TmuxCutoverCleanupDecision::AbortOldCleanupFailure
    } else {
        TmuxCutoverCleanupDecision::AbortOldAndNewCleanupFailure
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TmuxDetachTransitionDecision {
    AbortNativeRuntimeStart,
    AbortTmuxShutdown,
    CommitNativeTransition,
}

fn tmux_detach_transition_decision(
    native_runtime_started: bool,
    shutdown_succeeded: bool,
) -> TmuxDetachTransitionDecision {
    if !native_runtime_started {
        TmuxDetachTransitionDecision::AbortNativeRuntimeStart
    } else if !shutdown_succeeded {
        TmuxDetachTransitionDecision::AbortTmuxShutdown
    } else {
        TmuxDetachTransitionDecision::CommitNativeTransition
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TmuxPostActionRefresh {
    ImmediateSnapshot,
    EventDriven,
}

fn tmux_hydration_warning_message(failures: &[String]) -> Option<String> {
    if failures.is_empty() {
        return None;
    }

    let preview = failures
        .iter()
        .take(3)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    let suffix = if failures.len() > 3 { ", ..." } else { "" };
    Some(format!(
        "tmux pane restore degraded for {} pane(s): {preview}{suffix}",
        failures.len()
    ))
}

impl TerminalPane {
    fn from_tmux_state(state: &TmuxPaneState, terminal: Terminal, degraded: bool) -> Self {
        Self {
            id: state.id.clone(),
            left: state.left,
            top: state.top,
            width: state.width,
            height: state.height,
            degraded,
            terminal,
        }
    }
}

impl TerminalTab {
    fn from_tmux_window(id: TabId, window: &TmuxWindowState, panes: Vec<TerminalPane>) -> Self {
        let title = DEFAULT_TAB_TITLE.to_string();
        let title_text_width = 0.0;
        let sticky_title_width = TerminalView::tab_display_width_for_text_px_without_close_with_max(
            title_text_width,
            TAB_MAX_WIDTH,
        );
        let display_width =
            TerminalView::tab_display_width_for_text_px_with_max(title_text_width, TAB_MAX_WIDTH);

        Self {
            id,
            window_id: window.id.clone(),
            window_index: window.index,
            active_pane_id: window
                .active_pane_id
                .clone()
                .or_else(|| panes.first().map(|pane| pane.id.clone()))
                .unwrap_or_default(),
            panes,
            manual_title: None,
            explicit_title: None,
            shell_title: None,
            pending_command_title: None,
            pending_command_token: 0,
            title,
            title_text_width,
            sticky_title_width,
            display_width,
            running_process: false,
        }
    }
}

impl TerminalView {
    fn refresh_runtime_capability_surfaces(&mut self, cx: &mut Context<Self>) {
        let loaded = config::load_runtime_config(
            &mut self.last_config_error_message,
            "Failed to reload config after tmux runtime transition",
        );
        let keybind_config = if loaded.loaded_from_disk {
            loaded.config
        } else {
            AppConfig::default()
        };
        keybindings::install_keybindings(cx, &keybind_config, self.runtime_uses_tmux());
        cx.set_menus(crate::menus::app_menus(
            self.install_cli_available(),
            self.runtime_uses_tmux(),
        ));
        self.refresh_command_palette_items_for_current_mode(cx);
    }

    fn create_native_runtime_tab_for_size(&self, size: TerminalSize) -> anyhow::Result<TerminalTab> {
        let terminal = Terminal::new_native(
            size,
            self.configured_working_dir.as_deref(),
            Some(self.event_wakeup_tx.clone()),
            Some(&self.tab_shell_integration),
            Some(&self.terminal_runtime),
        )?;
        let predicted_prompt_cwd = Self::predicted_prompt_cwd(
            self.configured_working_dir.as_deref(),
            self.terminal_runtime.working_dir_fallback,
        );
        let predicted_title =
            Self::predicted_prompt_seed_title(&self.tab_title, predicted_prompt_cwd.as_deref());
        let tab_id = self.next_tab_id;
        Ok(Self::create_native_tab(
            tab_id,
            terminal,
            size.cols,
            size.rows,
            predicted_title,
        ))
    }

    pub(in super::super) fn attach_tmux_runtime(
        &mut self,
        launch: TmuxLaunchTarget,
        cx: &mut Context<Self>,
    ) -> bool {
        let (binary, show_active_pane_border) = if self.runtime_uses_tmux() {
            (
                self.tmux_runtime().config.binary.clone(),
                self.tmux_runtime().config.show_active_pane_border,
            )
        } else {
            let loaded = config::load_runtime_config(
                &mut self.last_config_error_message,
                "Failed to read config for tmux attach",
            );
            (
                loaded.config.tmux_binary.trim().to_string(),
                loaded.config.tmux_show_active_pane_border,
            )
        };
        let runtime_config = TmuxRuntimeConfig {
            binary,
            launch,
            show_active_pane_border,
        };
        if let Err(error) = TmuxClient::verify_tmux_version(runtime_config.binary.as_str(), 3, 3) {
            termy_toast::error(format!("tmux preflight failed: {error}"));
            return false;
        }

        let size = self.active_terminal().size();
        let tmux_client = match TmuxClient::new(
            runtime_config.clone(),
            size.cols.max(1),
            size.rows.max(1),
            Some(self.event_wakeup_tx.clone()),
        ) {
            Ok(client) => client,
            Err(error) => {
                termy_toast::error(format!("failed to start tmux control runtime: {error}"));
                return false;
            }
        };
        let snapshot = match tmux_client.refresh_snapshot() {
            Ok(snapshot) => snapshot,
            Err(error) => {
                if let Err(cleanup_error) = tmux_client.shutdown_default() {
                    termy_toast::error(format!(
                        "failed to fetch tmux snapshot: {error}; cleanup failed: {cleanup_error}"
                    ));
                    return false;
                }
                termy_toast::error(format!("failed to fetch tmux snapshot: {error}"));
                return false;
            }
        };

        if self.runtime_uses_tmux()
            && let Err(error) = self
                .tmux_runtime()
                .client
                .shutdown_default()
        {
            // Switching sessions must be a hard cutover. If the previous client
            // cannot be detached, abort and explicitly cleanup the freshly spawned
            // client to avoid accumulating orphaned control clients.
            let new_cleanup_result = tmux_client.shutdown_default();
            match tmux_cutover_cleanup_decision(false, new_cleanup_result.is_ok()) {
                TmuxCutoverCleanupDecision::AbortOldAndNewCleanupFailure => {
                    let cleanup_error = new_cleanup_result
                        .expect_err("new cleanup error must be present when decision is combined failure");
                    termy_toast::error(format!(
                        "failed to cleanup previous tmux client before attach: {error}; \
                         failed to cleanup new tmux client: {cleanup_error}"
                    ));
                    return false;
                }
                TmuxCutoverCleanupDecision::AbortOldCleanupFailure => {
                    termy_toast::error(format!(
                        "failed to cleanup previous tmux client before attach: {error}"
                    ));
                    return false;
                }
                TmuxCutoverCleanupDecision::Proceed => {}
            }
        }

        self.runtime = RuntimeState::Tmux(TmuxRuntime::new(
            runtime_config,
            tmux_client,
            size.cols.max(1),
            size.rows.max(1),
        ));
        self.apply_tmux_snapshot_rehydrate(snapshot);
        self.reset_tab_interaction_state();
        self.clear_selection();
        self.refresh_runtime_capability_surfaces(cx);
        cx.notify();
        true
    }

    fn commit_tmux_runtime_to_native(
        &mut self,
        native_tab: TerminalTab,
        cx: &mut Context<Self>,
    ) {
        self.runtime = RuntimeState::Native;
        self.tabs = vec![native_tab];
        self.active_tab = 0;
        self.next_tab_id = self.tabs[0].id.saturating_add(1);
        self.refresh_tab_title(0);
        self.mark_tab_strip_layout_dirty();
        self.reset_tab_interaction_state();
        self.clear_selection();
        self.scroll_active_tab_into_view();
        self.refresh_runtime_capability_surfaces(cx);
        cx.notify();
    }

    fn transition_tmux_runtime_to_native(
        &mut self,
        size: TerminalSize,
        cx: &mut Context<Self>,
    ) -> bool {
        let native_tab = match self.create_native_runtime_tab_for_size(size) {
            Ok(tab) => tab,
            Err(error) => {
                termy_toast::error(format!("Failed to start native runtime: {error}"));
                return false;
            }
        };

        self.commit_tmux_runtime_to_native(native_tab, cx);
        true
    }

    pub(in super::super) fn detach_tmux_runtime_to_native(&mut self, cx: &mut Context<Self>) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }

        let size = self.active_terminal().size();
        let native_tab = match self.create_native_runtime_tab_for_size(size) {
            Ok(tab) => tab,
            Err(error) => {
                termy_toast::error(format!("Failed to start native runtime: {error}"));
                return false;
            }
        };

        let shutdown_result = self.tmux_runtime().client.shutdown_default();
        match tmux_detach_transition_decision(true, shutdown_result.is_ok()) {
            TmuxDetachTransitionDecision::CommitNativeTransition => {
                self.commit_tmux_runtime_to_native(native_tab, cx);
                termy_toast::success("Detached tmux session");
                true
            }
            TmuxDetachTransitionDecision::AbortTmuxShutdown => {
                let error = shutdown_result.expect_err(
                    "shutdown error must be present when decision aborts tmux shutdown",
                );
                termy_toast::error(format!("Failed to detach tmux session: {error}"));
                false
            }
            TmuxDetachTransitionDecision::AbortNativeRuntimeStart => {
                unreachable!("native runtime is already initialized at this stage")
            }
        }
    }

    pub(in super::super) fn recover_from_tmux_runtime_exit(
        &mut self,
        reason: Option<String>,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.runtime_uses_tmux() {
            return false;
        }

        let size = self.active_terminal().size();
        if let Some(reason) = reason {
            termy_toast::error(reason);
        }
        self.transition_tmux_runtime_to_native(size, cx)
    }

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
        match TmuxClient::new(
            next_config.clone(),
            cols,
            rows,
            Some(self.event_wakeup_tx.clone()),
        ) {
            Ok(next_client) => {
                if let Err(error) = self
                    .tmux_runtime()
                    .client
                    .shutdown_default()
                {
                    let new_cleanup_result = next_client.shutdown_default();
                    match tmux_cutover_cleanup_decision(false, new_cleanup_result.is_ok()) {
                        TmuxCutoverCleanupDecision::AbortOldAndNewCleanupFailure => {
                            let cleanup_error = new_cleanup_result.expect_err(
                                "new cleanup error must be present when decision is combined failure",
                            );
                            termy_toast::error(format!(
                                "tmux reconnect failed while cleaning previous client: {error}; \
                                 failed to cleanup new client: {cleanup_error}"
                            ));
                            return;
                        }
                        TmuxCutoverCleanupDecision::AbortOldCleanupFailure => {
                            termy_toast::error(format!(
                                "tmux reconnect failed while cleaning previous client: {error}"
                            ));
                            return;
                        }
                        TmuxCutoverCleanupDecision::Proceed => {}
                    }
                }

                let runtime = self.tmux_runtime_mut();
                runtime.config = next_config;
                runtime.client = next_client;
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

    pub(in super::super) fn tmux_send_input_to_active_pane(&self, input: &[u8]) -> bool {
        let Some(active_pane_id) = self.active_pane_id() else {
            return false;
        };
        match self.tmux_runtime().client.send_input(active_pane_id, input) {
            Ok(()) => true,
            Err(error) => {
                termy_toast::error(format!("Input write failed: {error}"));
                false
            }
        }
    }

    pub(in super::super) fn tmux_resize_pane_step(
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

    pub(in super::super) fn tmux_reorder_tab(&mut self, from: usize, to: usize) -> bool {
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
                if !self.run_tmux_action("Failed to reorder tabs", |tmux_client| {
                    tmux_client.swap_windows(source.as_str(), target.as_str())
                }) {
                    return false;
                }
                window_order.swap(index, index + 1);
            }
        } else {
            for index in (to + 1..=from).rev() {
                let source = window_order[index].clone();
                let target = window_order[index - 1].clone();
                if !self.run_tmux_action("Failed to reorder tabs", |tmux_client| {
                    tmux_client.swap_windows(source.as_str(), target.as_str())
                }) {
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

        true
    }

    pub(in super::super) fn tmux_switch_active_tab_left(
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
            self.scroll_active_tab_into_view();
            cx.notify();
        }
        refreshed
    }

    pub(in super::super) fn tmux_switch_active_tab_right(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.run_tmux_action("Failed to switch tab", |tmux_client| tmux_client.next_window()) {
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

    pub(in super::super) fn tmux_add_tab(&mut self, cx: &mut Context<Self>) {
        if !self.run_tmux_action("Failed to create tab", |tmux_client| tmux_client.new_window()) {
            return;
        }

        if self.refresh_tmux_snapshot() {
            self.reset_tab_interaction_state();
            self.scroll_active_tab_into_view();
            cx.notify();
        }
    }

    pub(in super::super) fn tmux_close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        let window_id = self.tabs[index].window_id.clone();
        if !self.run_tmux_action("Failed to close tab", |tmux_client| {
            tmux_client.kill_window(window_id.as_str())
        }) {
            return;
        }

        if self.refresh_tmux_snapshot() {
            self.reset_tab_drag_state();
            self.clear_selection();
            self.scroll_active_tab_into_view();
            cx.notify();
        }
    }

    pub(in super::super) fn tmux_switch_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        let window_id = self.tabs[index].window_id.clone();
        if !self.run_tmux_action("Failed to switch tab", |tmux_client| {
            tmux_client.select_window(window_id.as_str())
        }) {
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

    pub(in super::super) fn tmux_commit_rename_tab(&mut self, index: usize) {
        let trimmed = self.rename_input.text().trim();
        if trimmed.is_empty() {
            return;
        }

        let renamed = Self::truncate_tab_title(trimmed);
        let window_id = self.tabs[index].window_id.clone();
        if self.run_tmux_action("Failed to rename tab", |tmux_client| {
            tmux_client.rename_window(window_id.as_str(), renamed.as_str())
        }) {
            let _ = self.refresh_tmux_snapshot();
        }
    }

    pub(in super::super) fn tmux_focus_pane_target(
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

    pub(in super::super) fn tmux_split_active_pane_vertical(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        self.run_tmux_action_with_refresh(
            "Failed to split pane",
            TmuxPostActionRefresh::ImmediateSnapshot,
            true,
            cx,
            |tmux_client| tmux_client.split_vertical(pane_id.as_str()),
        )
    }

    pub(in super::super) fn tmux_split_active_pane_horizontal(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        self.run_tmux_action_with_refresh(
            "Failed to split pane",
            TmuxPostActionRefresh::ImmediateSnapshot,
            true,
            cx,
            |tmux_client| tmux_client.split_horizontal(pane_id.as_str()),
        )
    }

    pub(in super::super) fn tmux_close_active_pane(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        self.run_tmux_action_with_refresh(
            "Failed to close pane",
            TmuxPostActionRefresh::ImmediateSnapshot,
            true,
            cx,
            |tmux_client| tmux_client.close_pane(pane_id.as_str()),
        )
    }

    pub(in super::super) fn tmux_focus_pane_left(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        self.run_tmux_action_with_refresh(
            "Failed to focus pane",
            TmuxPostActionRefresh::EventDriven,
            true,
            cx,
            |tmux_client| tmux_client.focus_pane_left(pane_id.as_str()),
        )
    }

    pub(in super::super) fn tmux_focus_pane_right(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        self.run_tmux_action_with_refresh(
            "Failed to focus pane",
            TmuxPostActionRefresh::EventDriven,
            true,
            cx,
            |tmux_client| tmux_client.focus_pane_right(pane_id.as_str()),
        )
    }

    pub(in super::super) fn tmux_focus_pane_up(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        self.run_tmux_action_with_refresh(
            "Failed to focus pane",
            TmuxPostActionRefresh::EventDriven,
            true,
            cx,
            |tmux_client| tmux_client.focus_pane_up(pane_id.as_str()),
        )
    }

    pub(in super::super) fn tmux_focus_pane_down(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        self.run_tmux_action_with_refresh(
            "Failed to focus pane",
            TmuxPostActionRefresh::EventDriven,
            true,
            cx,
            |tmux_client| tmux_client.focus_pane_down(pane_id.as_str()),
        )
    }

    pub(in super::super) fn tmux_resize_pane_left(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        self.run_tmux_action_with_refresh(
            "Failed to resize pane",
            TmuxPostActionRefresh::EventDriven,
            false,
            cx,
            |tmux_client| tmux_client.resize_pane_left(pane_id.as_str(), 1),
        )
    }

    pub(in super::super) fn tmux_resize_pane_right(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        self.run_tmux_action_with_refresh(
            "Failed to resize pane",
            TmuxPostActionRefresh::EventDriven,
            false,
            cx,
            |tmux_client| tmux_client.resize_pane_right(pane_id.as_str(), 1),
        )
    }

    pub(in super::super) fn tmux_resize_pane_up(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        self.run_tmux_action_with_refresh(
            "Failed to resize pane",
            TmuxPostActionRefresh::EventDriven,
            false,
            cx,
            |tmux_client| tmux_client.resize_pane_up(pane_id.as_str(), 1),
        )
    }

    pub(in super::super) fn tmux_resize_pane_down(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        self.run_tmux_action_with_refresh(
            "Failed to resize pane",
            TmuxPostActionRefresh::EventDriven,
            false,
            cx,
            |tmux_client| tmux_client.resize_pane_down(pane_id.as_str(), 1),
        )
    }

    pub(in super::super) fn tmux_toggle_active_pane_zoom(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(pane_id) = self.active_pane_id().map(ToOwned::to_owned) else {
            return false;
        };
        self.run_tmux_action_with_refresh(
            "Failed to toggle pane zoom",
            TmuxPostActionRefresh::ImmediateSnapshot,
            false,
            cx,
            |tmux_client| tmux_client.toggle_pane_zoom(pane_id.as_str()),
        )
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

    fn apply_tmux_snapshot_rehydrate(&mut self, snapshot: TmuxSnapshot) {
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

    pub(in super::super) fn apply_tmux_snapshot(&mut self, snapshot: TmuxSnapshot) {
        self.apply_tmux_snapshot_inner(snapshot, true);
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

    fn tmux_snapshot_refresh_mode(
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
                TmuxNotification::Warning(message) => {
                    termy_toast::warning(message);
                    should_redraw = true;
                }
                TmuxNotification::Exit(reason) => {
                    let reason = Some(
                        reason.unwrap_or_else(|| "tmux control mode exited".to_string()),
                    );
                    return self.recover_from_tmux_runtime_exit(reason, cx);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmux_cutover_cleanup_decision_distinguishes_old_only_vs_combined_failure() {
        assert_eq!(
            tmux_cutover_cleanup_decision(true, true),
            TmuxCutoverCleanupDecision::Proceed
        );
        assert_eq!(
            tmux_cutover_cleanup_decision(false, true),
            TmuxCutoverCleanupDecision::AbortOldCleanupFailure
        );
        assert_eq!(
            tmux_cutover_cleanup_decision(false, false),
            TmuxCutoverCleanupDecision::AbortOldAndNewCleanupFailure
        );
    }

    #[test]
    fn tmux_detach_transition_decision_requires_native_and_shutdown_success() {
        assert_eq!(
            tmux_detach_transition_decision(false, false),
            TmuxDetachTransitionDecision::AbortNativeRuntimeStart
        );
        assert_eq!(
            tmux_detach_transition_decision(true, false),
            TmuxDetachTransitionDecision::AbortTmuxShutdown
        );
        assert_eq!(
            tmux_detach_transition_decision(true, true),
            TmuxDetachTransitionDecision::CommitNativeTransition
        );
    }

    #[test]
    fn tmux_snapshot_refresh_mode_is_debounced_when_deadline_has_elapsed() {
        let now = Instant::now();
        let mode = TerminalView::tmux_snapshot_refresh_mode(
            false,
            Some(now - Duration::from_millis(1)),
            now,
        );
        assert_eq!(mode, TmuxSnapshotRefreshMode::Debounced);
    }

    #[test]
    fn tmux_snapshot_refresh_mode_is_none_when_deadline_has_not_elapsed() {
        let now = Instant::now();
        let mode = TerminalView::tmux_snapshot_refresh_mode(
            false,
            Some(now + Duration::from_millis(5)),
            now,
        );
        assert_eq!(mode, TmuxSnapshotRefreshMode::None);
    }

    #[test]
    fn tmux_snapshot_refresh_mode_prioritizes_immediate_refresh_over_debounce() {
        let now = Instant::now();
        let mode = TerminalView::tmux_snapshot_refresh_mode(
            true,
            Some(now - Duration::from_millis(1)),
            now,
        );
        assert_eq!(mode, TmuxSnapshotRefreshMode::Immediate);
    }

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

    #[test]
    fn tmux_hydration_warning_message_is_none_for_empty_failures() {
        assert!(tmux_hydration_warning_message(&[]).is_none());
    }

    #[test]
    fn tmux_hydration_warning_message_truncates_preview_after_three_entries() {
        let failures = vec![
            "%1 (capture timeout)".to_string(),
            "%2 (capture timeout)".to_string(),
            "%3 (capture timeout)".to_string(),
            "%4 (capture timeout)".to_string(),
        ];
        let message = tmux_hydration_warning_message(&failures).expect("warning expected");
        assert!(message.contains("4 pane(s)"));
        assert!(message.contains("%1 (capture timeout), %2 (capture timeout), %3 (capture timeout), ..."));
    }
}
