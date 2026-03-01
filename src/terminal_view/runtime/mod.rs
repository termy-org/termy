use std::time::Instant;

use termy_terminal_ui::{TmuxClient, TmuxRuntimeConfig};

mod tmux;
mod tmux_sync;

pub(super) use tmux_sync::{TmuxResizeScheduler, TmuxResizeWakeup};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RuntimeKind {
    Native,
    Tmux,
}

impl RuntimeKind {
    pub(super) const fn uses_tmux(self) -> bool {
        matches!(self, Self::Tmux)
    }
}

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
        cols: u16,
        rows: u16,
    ) -> Self {
        Self {
            config,
            client,
            client_cols: cols,
            client_rows: rows,
            resize_scheduler: TmuxResizeScheduler::default(),
            resize_wakeup_scheduled: false,
            title_refresh_deadline: None,
            title_refresh_wakeup_scheduled: false,
        }
    }
}
