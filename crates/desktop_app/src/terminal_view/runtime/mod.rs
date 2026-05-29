use std::time::Instant;

use flume::Sender;
use termy_terminal_ui::{TmuxClient, TmuxLaunchTarget, TmuxRuntimeConfig, TmuxSnapshot};

use crate::startup::StartupBlocker;

use super::*;

mod tmux;
mod tmux_sync;

pub(super) use tmux_sync::{TmuxResizeScheduler, TmuxResizeWakeup};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RuntimeKind {
    Native,
    Tmux,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TmuxStartupSnapshotCleanupDecision {
    ContinueToFatalExit,
    EmitCleanupWarningAndExit,
}

fn tmux_startup_snapshot_cleanup_decision(
    cleanup_succeeded: bool,
) -> TmuxStartupSnapshotCleanupDecision {
    if cleanup_succeeded {
        TmuxStartupSnapshotCleanupDecision::ContinueToFatalExit
    } else {
        TmuxStartupSnapshotCleanupDecision::EmitCleanupWarningAndExit
    }
}

impl RuntimeKind {
    pub(super) const fn uses_tmux(self) -> bool {
        matches!(self, Self::Tmux)
    }

    #[cfg(target_os = "windows")]
    pub(super) fn from_app_config(_config: &AppConfig) -> Self {
        // Hard cutover: tmux runtime is unsupported on Windows, regardless of config value.
        Self::Native
    }

    #[cfg(not(target_os = "windows"))]
    pub(super) fn from_app_config(config: &AppConfig) -> Self {
        if config.tmux_enabled {
            Self::Tmux
        } else {
            Self::Native
        }
    }
}

#[allow(clippy::large_enum_variant)]
pub(super) enum RuntimeState {
    Native,
    Tmux(TmuxRuntime),
}

impl RuntimeState {
    pub(super) const fn kind(&self) -> RuntimeKind {
        match self {
            Self::Native => RuntimeKind::Native,
            Self::Tmux(_) => RuntimeKind::Tmux,
        }
    }

    pub(super) fn as_tmux(&self) -> Option<&TmuxRuntime> {
        match self {
            Self::Native => None,
            Self::Tmux(runtime) => Some(runtime),
        }
    }

    pub(super) fn as_tmux_mut(&mut self) -> Option<&mut TmuxRuntime> {
        match self {
            Self::Native => None,
            Self::Tmux(runtime) => Some(runtime),
        }
    }
}

pub(super) struct TmuxRuntime {
    pub(super) config: TmuxRuntimeConfig,
    pub(super) client: TmuxClient,
    pub(super) preferred_cwd: Option<String>,
    pub(super) client_cols: u16,
    pub(super) client_rows: u16,
    pub(super) resize_scheduler: TmuxResizeScheduler,
    pub(super) resize_wakeup_scheduled: bool,
    pub(super) title_refresh_deadline: Option<Instant>,
    pub(super) title_refresh_wakeup_scheduled: bool,
}

impl TmuxRuntime {
    pub(super) fn new(
        config: TmuxRuntimeConfig,
        client: TmuxClient,
        preferred_cwd: Option<String>,
        cols: u16,
        rows: u16,
    ) -> Self {
        Self {
            config,
            client,
            preferred_cwd,
            client_cols: cols,
            client_rows: rows,
            resize_scheduler: TmuxResizeScheduler::default(),
            resize_wakeup_scheduled: false,
            title_refresh_deadline: None,
            title_refresh_wakeup_scheduled: false,
        }
    }
}

impl TerminalView {
    #[cfg(any(not(target_os = "windows"), test))]
    pub(super) fn runtime_kind_from_app_config(config: &AppConfig) -> RuntimeKind {
        RuntimeKind::from_app_config(config)
    }

    pub(super) fn tmux_runtime_from_app_config(config: &AppConfig) -> TmuxRuntimeConfig {
        TmuxRuntimeConfig {
            binary: config.tmux_binary.trim().to_string(),
            launch: TmuxLaunchTarget::Managed {
                persistence: config.tmux_persistence,
            },
            show_active_pane_border: config.tmux_show_active_pane_border,
        }
    }

    pub(super) fn runtime_startup_from_app_config(
        config: &AppConfig,
        event_wakeup_tx: &Sender<()>,
        configured_working_dir: Option<&str>,
        tab_shell_integration: &TabTitleShellIntegration,
        terminal_runtime: &TerminalRuntimeConfig,
        startup_command: Option<&str>,
        initial_cols: u16,
        initial_rows: u16,
    ) -> (RuntimeState, Option<TmuxSnapshot>, Option<Terminal>) {
        match RuntimeKind::from_app_config(config) {
            RuntimeKind::Tmux => {
                let tmux_runtime = Self::tmux_runtime_from_app_config(config);
                let initial_working_dir = termy_terminal_ui::resolve_launch_working_directory(
                    configured_working_dir,
                    terminal_runtime.working_dir_fallback,
                )
                .map(|path| path.to_string_lossy().into_owned());
                let tmux_client = match TmuxClient::new(
                    tmux_runtime.clone(),
                    initial_cols,
                    initial_rows,
                    initial_working_dir.as_deref(),
                    Some(event_wakeup_tx.clone()),
                ) {
                    Ok(client) => client,
                    Err(error) => {
                        StartupBlocker::TmuxClientLaunch(format!("{error:#}")).present_and_exit()
                    }
                };
                let initial_snapshot = match tmux_client.refresh_snapshot() {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        // `present_and_exit` terminates the process without running
                        // destructors. Explicit shutdown avoids leaking a control
                        // client when startup fails after tmux launch succeeds.
                        let cleanup_result = tmux_client.shutdown_default();
                        if matches!(
                            tmux_startup_snapshot_cleanup_decision(cleanup_result.is_ok()),
                            TmuxStartupSnapshotCleanupDecision::EmitCleanupWarningAndExit
                        ) {
                            let cleanup_error = cleanup_result.expect_err(
                                "cleanup error must be present when startup decision emits warning",
                            );
                            eprintln!(
                                "Termy startup warning: failed to cleanup tmux client after \
                                 snapshot startup failure: {cleanup_error}"
                            );
                        }
                        StartupBlocker::TmuxInitialSnapshot(format!("{error:#}")).present_and_exit()
                    }
                };
                (
                    RuntimeState::Tmux(TmuxRuntime::new(
                        tmux_runtime,
                        tmux_client,
                        initial_working_dir,
                        initial_cols,
                        initial_rows,
                    )),
                    Some(initial_snapshot),
                    None,
                )
            }
            RuntimeKind::Native => {
                let native_terminal = match Terminal::new_native(
                    TerminalSize {
                        cols: initial_cols,
                        rows: initial_rows,
                        ..TerminalSize::default()
                    },
                    configured_working_dir,
                    Some(event_wakeup_tx.clone()),
                    Some(tab_shell_integration),
                    Some(terminal_runtime),
                    startup_command,
                ) {
                    Ok(terminal) => terminal,
                    Err(error) => {
                        eprintln!("Termy startup blocked: failed to start native runtime: {error}");
                        std::process::exit(1);
                    }
                };
                (RuntimeState::Native, None, Some(native_terminal))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_snapshot_cleanup_decision_only_warns_when_cleanup_fails() {
        assert_eq!(
            tmux_startup_snapshot_cleanup_decision(true),
            TmuxStartupSnapshotCleanupDecision::ContinueToFatalExit
        );
        assert_eq!(
            tmux_startup_snapshot_cleanup_decision(false),
            TmuxStartupSnapshotCleanupDecision::EmitCleanupWarningAndExit
        );
    }
}
