use super::Terminal;
use anyhow::Result;
use termy_terminal_ui::{TmuxClient, TmuxNotification};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RuntimeBackendMode {
    Native,
    Tmux,
}

impl RuntimeBackendMode {
    pub(super) const fn uses_tmux(self) -> bool {
        matches!(self, Self::Tmux)
    }
}

/// `TerminalView` runtime boundary: interaction code talks to this shape instead
/// of branching directly on tmux/native details.
#[derive(Clone, Copy)]
pub(super) struct RuntimeBackend<'a> {
    mode: RuntimeBackendMode,
    tmux_client: Option<&'a TmuxClient>,
}

impl<'a> RuntimeBackend<'a> {
    pub(super) const fn new(mode: RuntimeBackendMode, tmux_client: Option<&'a TmuxClient>) -> Self {
        Self { mode, tmux_client }
    }

    pub(super) fn tmux_client(&self) -> Option<&'a TmuxClient> {
        if self.mode.uses_tmux() {
            self.tmux_client
        } else {
            None
        }
    }

    pub(super) fn poll_tmux_notifications(&self) -> Vec<TmuxNotification> {
        self.tmux_client()
            .map_or_else(Vec::new, TmuxClient::poll_notifications)
    }

    pub(super) fn send_input(
        &self,
        active_terminal: &Terminal,
        active_pane_id: Option<&str>,
        input: &[u8],
    ) -> Result<bool> {
        if self.mode.uses_tmux() {
            let Some(pane_id) = active_pane_id else {
                return Ok(false);
            };
            let Some(tmux_client) = self.tmux_client() else {
                return Ok(false);
            };
            tmux_client.send_input(pane_id, input)?;
            return Ok(true);
        }

        active_terminal.write_input(input);
        Ok(true)
    }

    pub(super) fn set_client_size(&self, cols: u16, rows: u16) -> Result<bool> {
        let Some(tmux_client) = self.tmux_client() else {
            return Ok(false);
        };
        tmux_client.set_client_size(cols, rows)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use termy_terminal_ui::TerminalSize;

    #[test]
    fn backend_mode_reports_tmux_without_leaking_tmux_client_type() {
        assert!(RuntimeBackendMode::Tmux.uses_tmux());
        assert!(!RuntimeBackendMode::Native.uses_tmux());
    }

    #[test]
    fn backend_hides_tmux_client_when_mode_is_native() {
        let backend = RuntimeBackend::new(RuntimeBackendMode::Native, None);
        assert!(backend.tmux_client().is_none());
        assert!(backend.poll_tmux_notifications().is_empty());
    }

    #[test]
    fn native_backend_input_path_returns_written() {
        let terminal = Terminal::new_tmux(TerminalSize::default(), 32);
        let backend = RuntimeBackend::new(RuntimeBackendMode::Native, None);

        let wrote = backend
            .send_input(&terminal, None, b"echo test")
            .expect("native input dispatch should not error");
        assert!(wrote);
    }

    #[test]
    fn tmux_backend_requires_active_pane_for_input_dispatch() {
        let terminal = Terminal::new_tmux(TerminalSize::default(), 32);
        let backend = RuntimeBackend::new(RuntimeBackendMode::Tmux, None);

        let wrote = backend
            .send_input(&terminal, None, b"echo test")
            .expect("missing pane id should return false instead of error");
        assert!(!wrote);
    }

    #[test]
    fn set_client_size_is_noop_without_tmux_client() {
        let native = RuntimeBackend::new(RuntimeBackendMode::Native, None);
        let tmux_without_client = RuntimeBackend::new(RuntimeBackendMode::Tmux, None);

        assert!(
            !native
                .set_client_size(120, 40)
                .expect("native backend size updates should no-op")
        );
        assert!(
            !tmux_without_client
                .set_client_size(120, 40)
                .expect("missing tmux client should no-op")
        );
    }
}
