use anyhow::{Context, Result, anyhow};
use flume::{Receiver, RecvTimeoutError, Sender, bounded};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

#[cfg(test)]
use super::command::split_control_completion_token;
use super::command::{
    SEND_INPUT_BULK_HEX_BYTES, SEND_INPUT_CHUNKED_HEX_BYTES, SendInputMode, choose_send_input_mode,
    next_control_completion_token, send_keys_hex_command, tmux_command_line,
};
use super::control::{
    ControlRequest, FATAL_EXIT_QUEUE_BOUND, NOTIFICATION_QUEUE_BOUND, NotificationCoalescer,
    PENDING_QUEUE_BOUND, REQUEST_QUEUE_BOUND, spawn_control_threads, try_enqueue_control_request,
};
use super::launch::{
    SessionLaunchPlan, managed_session_window_option_override_commands, spawn_tmux_control_mode,
};
use super::payload::{capture_full_pane_args, sanitize_tmux_payload, unescape_tmux_payload};
use super::session::{self, run_tmux_command_with_socket};
use super::shutdown::{
    is_tmux_missing_client_error, is_tmux_no_server_error, normalize_shutdown_teardown_result,
    run_shutdown_actions,
};
use super::snapshot::{PANE_SNAPSHOT_FORMAT, WINDOW_SNAPSHOT_FORMAT, parse_snapshot};
use super::types::{
    TmuxControlError, TmuxLaunchTarget, TmuxNotification, TmuxRuntimeConfig, TmuxSessionSummary,
    TmuxShutdownMode, TmuxSnapshot, TmuxSocketTarget,
};

pub struct TmuxClient {
    tmux_binary: String,
    session_name: String,
    socket_target: TmuxSocketTarget,
    show_active_pane_border: bool,
    control_client_pid: u32,
    shutdown_mode_on_drop: TmuxShutdownMode,
    shutdown_in_progress: AtomicBool,
    shutdown_completed: AtomicBool,
    request_tx: Sender<ControlRequest>,
    notifications_rx: Receiver<TmuxNotification>,
    fatal_exit_rx: Receiver<Option<String>>,
}

impl TmuxClient {
    fn launch_plan(config: &TmuxRuntimeConfig) -> SessionLaunchPlan {
        super::launch::launch_plan(config)
    }

    pub fn new(
        config: TmuxRuntimeConfig,
        cols: u16,
        rows: u16,
        event_wakeup_tx: Option<Sender<()>>,
    ) -> Result<Self> {
        #[cfg(not(unix))]
        {
            let _ = (cols, rows, event_wakeup_tx);
            return Err(anyhow!(
                "tmux control mode is only supported on unix targets"
            ));
        }

        let launch_plan = Self::launch_plan(&config);
        let enforce_managed_session_ui = matches!(&config.launch, TmuxLaunchTarget::Managed { .. });
        if launch_plan.session_name.trim().is_empty() {
            return Err(anyhow!("tmux session name cannot be empty"));
        }
        #[cfg(unix)]
        let (child, child_stdin, child_stdout) = spawn_tmux_control_mode(
            &config,
            &launch_plan.socket_target,
            launch_plan.session_name.as_str(),
            launch_plan.attach_existing,
        )?;

        let (request_tx, request_rx) = bounded::<ControlRequest>(REQUEST_QUEUE_BOUND);
        let (pending_tx, pending_rx) = bounded(PENDING_QUEUE_BOUND);
        let (notifications_tx, notifications_rx) =
            bounded::<TmuxNotification>(NOTIFICATION_QUEUE_BOUND);
        let (fatal_exit_tx, fatal_exit_rx) = bounded::<Option<String>>(FATAL_EXIT_QUEUE_BOUND);
        #[cfg(unix)]
        let control_client_pid = child.id();
        #[cfg(not(unix))]
        let control_client_pid = 0;

        #[cfg(unix)]
        spawn_control_threads(
            child,
            child_stdin,
            child_stdout,
            request_rx,
            pending_tx,
            pending_rx,
            notifications_tx,
            fatal_exit_tx,
            event_wakeup_tx,
        );

        let client = Self {
            tmux_binary: config.binary,
            session_name: launch_plan.session_name,
            socket_target: launch_plan.socket_target,
            show_active_pane_border: config.show_active_pane_border,
            control_client_pid,
            shutdown_mode_on_drop: launch_plan.shutdown_mode_on_drop,
            shutdown_in_progress: AtomicBool::new(false),
            shutdown_completed: AtomicBool::new(false),
            request_tx,
            notifications_rx,
            fatal_exit_rx,
        };
        if enforce_managed_session_ui {
            client.enforce_native_session_ui()?;
        }
        client.set_client_size(cols, rows)?;
        Ok(client)
    }

    pub fn set_client_size(&self, cols: u16, rows: u16) -> Result<()> {
        let size = format!("{}x{}", cols, rows);
        let command = tmux_command_line(&["refresh-client", "-C", size.as_str()]);
        // `refresh-client -C` operates on the *current control client*.
        // Running it as an out-of-band tmux process can fail with no client
        // context during attach/re-attach; issuing it through the active control
        // channel binds it to the correct client deterministically.
        self.send_control_command_wait(command.as_str())
            .with_context(|| format!("tmux status command failed: {command}"))
            .map(|_| ())
    }

    pub fn session_name(&self) -> &str {
        self.session_name.as_str()
    }

    pub fn poll_notifications(&self) -> Vec<TmuxNotification> {
        let mut coalescer = NotificationCoalescer::with_output_byte_limit(usize::MAX);
        let mut coalescer_error = None::<String>;
        for notification in self.notifications_rx.try_iter() {
            // Draining already-bounded channel contents cannot grow unbounded;
            // this second-stage coalescing keeps refresh/output bursts from
            // triggering redraw storms in the UI event loop.
            if let Err(error) = coalescer.push(notification) {
                coalescer_error = Some(error.message);
                break;
            }
        }

        if let Some(exit_reason) = self.fatal_exit_rx.try_iter().last() {
            return vec![TmuxNotification::Exit(exit_reason)];
        }
        if let Some(error) = coalescer_error {
            return vec![TmuxNotification::Exit(Some(error))];
        }

        coalescer.drain()
    }

    pub fn refresh_snapshot(&self) -> Result<TmuxSnapshot> {
        let windows_output = self.run_control_capture_args(&[
            "list-windows",
            "-t",
            self.session_name.as_str(),
            "-F",
            // Use an explicit non-printable field separator and escaped string fields
            // so tabs/newlines inside names cannot corrupt record framing.
            WINDOW_SNAPSHOT_FORMAT,
        ])?;

        let panes_output = self.run_control_capture_args(&[
            "list-panes",
            "-s",
            "-t",
            self.session_name.as_str(),
            "-F",
            PANE_SNAPSHOT_FORMAT,
        ])?;

        parse_snapshot(&self.session_name, &windows_output, &panes_output)
    }

    pub fn new_window_after(&self, target_window_id: &str) -> Result<()> {
        // Use explicit insert-after targeting so Termy tab creation is deterministic:
        // new tabs always appear immediately to the right of the active tab.
        self.run_control_status_args(&["new-window", "-a", "-t", target_window_id])
    }

    pub fn kill_window(&self, window_id: &str) -> Result<()> {
        self.run_control_status_args(&["kill-window", "-t", window_id])
    }

    pub fn rename_window(&self, window_id: &str, name: &str) -> Result<()> {
        self.run_control_status_args(&["rename-window", "-t", window_id, name])
    }

    pub fn previous_window(&self) -> Result<()> {
        self.run_control_status_args(&["previous-window", "-t", self.session_name.as_str()])
    }

    pub fn next_window(&self) -> Result<()> {
        self.run_control_status_args(&["next-window", "-t", self.session_name.as_str()])
    }

    pub fn select_window(&self, window_id: &str) -> Result<()> {
        self.run_control_status_args(&["select-window", "-t", window_id])
    }

    pub fn swap_windows(&self, src: &str, dst: &str) -> Result<()> {
        self.run_control_status_args(&["swap-window", "-s", src, "-t", dst])
    }

    pub fn split_vertical(&self, pane_id: &str) -> Result<()> {
        self.run_control_status_args(&["split-window", "-h", "-t", pane_id])
    }

    pub fn split_horizontal(&self, pane_id: &str) -> Result<()> {
        self.run_control_status_args(&["split-window", "-t", pane_id])
    }

    pub fn close_pane(&self, pane_id: &str) -> Result<()> {
        self.run_control_status_args(&["kill-pane", "-t", pane_id])
    }

    pub fn focus_pane_left(&self, pane_id: &str) -> Result<()> {
        self.run_control_status_args(&["select-pane", "-L", "-t", pane_id])
    }

    pub fn focus_pane_right(&self, pane_id: &str) -> Result<()> {
        self.run_control_status_args(&["select-pane", "-R", "-t", pane_id])
    }

    pub fn focus_pane_up(&self, pane_id: &str) -> Result<()> {
        self.run_control_status_args(&["select-pane", "-U", "-t", pane_id])
    }

    pub fn focus_pane_down(&self, pane_id: &str) -> Result<()> {
        self.run_control_status_args(&["select-pane", "-D", "-t", pane_id])
    }

    pub fn select_pane(&self, pane_id: &str) -> Result<()> {
        self.run_control_status_args(&["select-pane", "-t", pane_id])
    }

    pub fn resize_pane_left(&self, pane_id: &str, cells: u16) -> Result<()> {
        let cells = cells.to_string();
        self.run_control_status_args(&["resize-pane", "-L", "-t", pane_id, cells.as_str()])
    }

    pub fn resize_pane_right(&self, pane_id: &str, cells: u16) -> Result<()> {
        let cells = cells.to_string();
        self.run_control_status_args(&["resize-pane", "-R", "-t", pane_id, cells.as_str()])
    }

    pub fn resize_pane_up(&self, pane_id: &str, cells: u16) -> Result<()> {
        let cells = cells.to_string();
        self.run_control_status_args(&["resize-pane", "-U", "-t", pane_id, cells.as_str()])
    }

    pub fn resize_pane_down(&self, pane_id: &str, cells: u16) -> Result<()> {
        let cells = cells.to_string();
        self.run_control_status_args(&["resize-pane", "-D", "-t", pane_id, cells.as_str()])
    }

    pub fn toggle_pane_zoom(&self, pane_id: &str) -> Result<()> {
        self.run_control_status_args(&["resize-pane", "-Z", "-t", pane_id])
    }

    pub fn detach_client(&self) -> Result<()> {
        let Some(client_name) = self.resolve_control_client_name_by_pid()? else {
            return Ok(());
        };

        match self.run_control_status_args(&["detach-client", "-t", client_name.as_str()]) {
            Ok(()) => Ok(()),
            Err(error) => {
                if is_tmux_missing_client_error(&error) || is_tmux_no_server_error(&error) {
                    // The targeted control client already disappeared between list and detach.
                    return Ok(());
                }
                Err(error)
            }
        }
    }

    pub fn shutdown(&self, mode: TmuxShutdownMode) -> Result<()> {
        self.run_shutdown_attempt(|| self.shutdown_impl(mode))
    }

    pub fn shutdown_default(&self) -> Result<()> {
        self.shutdown(self.shutdown_mode_on_drop)
    }

    fn run_shutdown_attempt<F>(&self, shutdown_action: F) -> Result<()>
    where
        F: FnOnce() -> Result<()>,
    {
        if self.shutdown_completed.load(Ordering::Acquire) {
            return Ok(());
        }

        if self.shutdown_in_progress.swap(true, Ordering::AcqRel) {
            return Ok(());
        }

        // Another thread may have completed shutdown between our optimistic
        // completion check and acquiring the in-progress flag.
        if self.shutdown_completed.load(Ordering::Acquire) {
            self.shutdown_in_progress.store(false, Ordering::Release);
            return Ok(());
        }

        let result = shutdown_action();
        if result.is_ok() {
            self.shutdown_completed.store(true, Ordering::Release);
        }
        // Failures must unlock retries so drop/reconnect can attempt cleanup again.
        self.shutdown_in_progress.store(false, Ordering::Release);
        result
    }

    fn shutdown_impl(&self, mode: TmuxShutdownMode) -> Result<()> {
        run_shutdown_actions(
            mode,
            self.session_name.as_str(),
            || {
                self.detach_client().with_context(|| {
                    format!(
                        "failed to detach tmux control client for session '{}'",
                        self.session_name
                    )
                })
            },
            || {
                // Isolated managed sessions are ephemeral. Teardown is always attempted in
                // this mode, even if detach failed, so stale sessions cannot accumulate.
                let teardown_result = Self::kill_session(
                    self.tmux_binary.as_str(),
                    self.socket_target.clone(),
                    self.session_name.as_str(),
                );
                normalize_shutdown_teardown_result(self.session_name.as_str(), teardown_result)
            },
        )
    }

    fn resolve_control_client_name_by_pid(&self) -> Result<Option<String>> {
        let output =
            match self.run_tmux_command(&["list-clients", "-F", "#{client_pid}\t#{client_name}"]) {
                Ok(output) => output,
                Err(error) => {
                    if is_tmux_no_server_error(&error) {
                        return Ok(None);
                    }
                    return Err(error).context("failed to resolve tmux control client identity");
                }
            };

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
            let Some((pid_raw, client_name_raw)) = line.split_once('\t') else {
                return Err(anyhow!(
                    "invalid tmux list-clients row while resolving control client pid {}: '{}'",
                    self.control_client_pid,
                    line
                ));
            };
            let pid = pid_raw.trim().parse::<u32>().with_context(|| {
                format!(
                    "invalid tmux client pid '{}' while resolving control client pid {}",
                    pid_raw.trim(),
                    self.control_client_pid
                )
            })?;

            if pid != self.control_client_pid {
                continue;
            }

            let client_name = client_name_raw.trim();
            if client_name.is_empty() {
                return Err(anyhow!(
                    "tmux client pid {} has empty client_name",
                    self.control_client_pid
                ));
            }
            return Ok(Some(client_name.to_string()));
        }

        Ok(None)
    }

    pub fn send_input(&self, pane_id: &str, bytes: &[u8]) -> Result<()> {
        if bytes.is_empty() {
            return Ok(());
        }

        let (mode, _) = choose_send_input_mode(bytes.len());
        match mode {
            SendInputMode::ChunkedHex => {
                for chunk in bytes.chunks(SEND_INPUT_CHUNKED_HEX_BYTES) {
                    let command = send_keys_hex_command(pane_id, chunk);
                    self.send_control_command_async(command.as_str())?;
                }
            }
            SendInputMode::Bulk => {
                // Large pastes must honor per-command completion so bounded control queues
                // cannot be flooded by thousands of async send-keys requests.
                for chunk in bytes.chunks(SEND_INPUT_BULK_HEX_BYTES) {
                    let command = send_keys_hex_command(pane_id, chunk);
                    let _ = self.send_control_command_wait(command.as_str())?;
                }
            }
        }

        Ok(())
    }

    pub fn capture_pane(&self, pane_id: &str, max_rows: usize) -> Result<Vec<u8>> {
        // Hydration capture must stay bounded to avoid expensive full-history
        // scans that can time out during reattach on large tmux histories.
        let start_row = format!("-{}", max_rows.max(1));
        let args = capture_full_pane_args(pane_id, start_row.as_str());
        let out = self.run_control_capture_args(&args)?;
        let payload = trim_trailing_line_terminators(out.as_bytes());
        Ok(sanitize_tmux_payload(unescape_tmux_payload(payload)))
    }

    pub fn verify_tmux_version(binary: &str, minimum_major: u8, minimum_minor: u8) -> Result<()> {
        session::verify_tmux_version(binary, minimum_major, minimum_minor)
    }

    pub fn list_sessions(
        binary: &str,
        socket_target: TmuxSocketTarget,
    ) -> Result<Vec<TmuxSessionSummary>> {
        session::list_sessions(binary, socket_target)
    }

    pub fn rename_session(
        binary: &str,
        socket_target: TmuxSocketTarget,
        current_session_name: &str,
        next_session_name: &str,
    ) -> Result<()> {
        session::rename_session(
            binary,
            socket_target,
            current_session_name,
            next_session_name,
        )
    }

    pub fn kill_session(
        binary: &str,
        socket_target: TmuxSocketTarget,
        session_name: &str,
    ) -> Result<()> {
        session::kill_session(binary, socket_target, session_name)
    }

    fn enqueue_control_request(&self, request: ControlRequest) -> Result<()> {
        try_enqueue_control_request(&self.request_tx, request).map_err(anyhow::Error::new)
    }

    fn send_control_command_async(&self, command: &str) -> Result<()> {
        self.enqueue_control_request(ControlRequest {
            command: command.to_string(),
            completion_token: next_control_completion_token(),
            response_tx: None,
        })
    }

    fn send_control_command_wait_with_timeout(
        &self,
        command: &str,
        timeout: Duration,
    ) -> Result<super::control::ControlCommandResult> {
        let (response_tx, response_rx) = flume::bounded(1);
        self.enqueue_control_request(ControlRequest {
            command: command.to_string(),
            completion_token: next_control_completion_token(),
            response_tx: Some(response_tx),
        })?;

        let response = match response_rx.recv_timeout(timeout) {
            Ok(response) => response,
            Err(RecvTimeoutError::Timeout) => {
                return Err(anyhow!(TmuxControlError::channel(format!(
                    "timed out waiting for command completion after {:?}: '{}'",
                    timeout, command
                ))));
            }
            Err(RecvTimeoutError::Disconnected) => {
                return Err(anyhow!(TmuxControlError::channel(format!(
                    "tmux control worker channel disconnected before command completion: '{}'",
                    command
                ))));
            }
        };
        response.map_err(anyhow::Error::new)
    }

    fn send_control_command_wait(
        &self,
        command: &str,
    ) -> Result<super::control::ControlCommandResult> {
        const CONTROL_COMMAND_TIMEOUT: Duration = Duration::from_secs(3);
        self.send_control_command_wait_with_timeout(command, CONTROL_COMMAND_TIMEOUT)
    }

    fn run_control_capture_args(&self, args: &[&str]) -> Result<String> {
        const CONTROL_CAPTURE_TIMEOUT: Duration = Duration::from_secs(10);
        let command = tmux_command_line(args);
        let response = self
            .send_control_command_wait_with_timeout(command.as_str(), CONTROL_CAPTURE_TIMEOUT)
            .with_context(|| format!("tmux capture command failed: {command}"))?;
        Ok(response.output)
    }

    fn run_control_status_args(&self, args: &[&str]) -> Result<()> {
        let command = tmux_command_line(args);
        self.send_control_command_wait(command.as_str())
            .with_context(|| format!("tmux status command failed: {command}"))
            .map(|_| ())
    }

    fn run_tmux_command(&self, args: &[&str]) -> Result<std::process::Output> {
        run_tmux_command_with_socket(self.tmux_binary.as_str(), &self.socket_target, args)
            .with_context(|| {
                format!(
                    "failed to execute tmux command via '{}': {}",
                    self.tmux_binary,
                    tmux_command_line(args)
                )
            })
    }

    fn enforce_native_session_ui(&self) -> Result<()> {
        let session = self.session_name.as_str();
        let all_windows_target = format!("{session}:*");

        self.run_control_status_args(&[
            "set-environment",
            "-t",
            session,
            "TERMY_SHELL_INTEGRATION",
            "0",
        ])
        .context("failed to disable termy shell integration env in tmux session")?;
        self.run_control_status_args(&[
            "set-environment",
            "-u",
            "-t",
            session,
            "TERMY_TAB_TITLE_PREFIX",
        ])
        .context("failed to clear termy shell title prefix env in tmux session")?;
        self.run_control_status_args(&["set-environment", "-t", session, "PROMPT_EOL_MARK", ""])
            .context("failed to disable zsh prompt eol mark env in tmux session")?;

        self.run_control_status_args(&["set-option", "-q", "-t", session, "status", "off"])
            .context("failed to disable tmux status line for managed session")?;
        // Managed persistence must survive detach->reattach even when the user's tmux
        // config enables `destroy-unattached`, which would otherwise tear down the
        // session as soon as Termy's control client detaches.
        self.run_control_status_args(&[
            "set-option",
            "-q",
            "-t",
            session,
            "destroy-unattached",
            "off",
        ])
        .context("failed to disable destroy-unattached for managed session")?;
        for command in managed_session_window_option_override_commands(
            all_windows_target.as_str(),
            self.show_active_pane_border,
        ) {
            self.run_control_status_args(&command).with_context(|| {
                let option_key = command.get(4).copied().unwrap_or("<missing-option-key>");
                let option_value = command.get(5).copied().unwrap_or("<missing-option-value>");
                format!(
                    "failed to apply tmux managed-session window option override '{}={}' (command: {})",
                    option_key,
                    option_value,
                    tmux_command_line(&command),
                )
            })?;
        }
        self.run_control_status_args(&["refresh-client"])
            .context("failed to refresh tmux client after managed-session ui configuration")?;

        Ok(())
    }
}

fn trim_trailing_line_terminators(mut bytes: &[u8]) -> &[u8] {
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

impl Drop for TmuxClient {
    fn drop(&mut self) {
        if let Err(error) = self.shutdown_default() {
            let action = match self.shutdown_mode_on_drop {
                TmuxShutdownMode::DetachOnly => "detach tmux control client",
                TmuxShutdownMode::DetachAndTeardownSession => {
                    "detach tmux control client and teardown managed session"
                }
            };
            eprintln!(
                "Termy shutdown warning: failed to {} '{}': {}",
                action, self.session_name, error
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmux::control::coalescer::signal_fatal_exit;
    use anyhow::anyhow;
    use std::cell::Cell;

    fn test_tmux_client(shutdown_mode_on_drop: TmuxShutdownMode) -> TmuxClient {
        let (request_tx, _request_rx) = flume::bounded::<ControlRequest>(1);
        let (_notifications_tx, notifications_rx) = flume::bounded::<TmuxNotification>(1);
        let (_fatal_exit_tx, fatal_exit_rx) = flume::bounded::<Option<String>>(1);
        TmuxClient {
            tmux_binary: "tmux".to_string(),
            session_name: "test-session".to_string(),
            socket_target: TmuxSocketTarget::DedicatedTermy,
            show_active_pane_border: false,
            control_client_pid: 0,
            shutdown_mode_on_drop,
            shutdown_in_progress: AtomicBool::new(false),
            shutdown_completed: AtomicBool::new(false),
            request_tx,
            notifications_rx,
            fatal_exit_rx,
        }
    }

    #[test]
    fn poll_notifications_prioritizes_dedicated_fatal_exit_signal() {
        let (notifications_tx, notifications_rx) = flume::bounded::<TmuxNotification>(4);
        let (fatal_exit_tx, fatal_exit_rx) = flume::bounded::<Option<String>>(1);
        notifications_tx
            .send(TmuxNotification::Output {
                pane_id: "%1".to_string(),
                bytes: b"stale".to_vec(),
            })
            .expect("queue stale output");
        notifications_tx
            .send(TmuxNotification::NeedsRefresh)
            .expect("queue stale refresh");
        signal_fatal_exit(&fatal_exit_tx, Some("control-mode failure".to_string()));

        let mut client = test_tmux_client(TmuxShutdownMode::DetachOnly);
        client.notifications_rx = notifications_rx;
        client.fatal_exit_rx = fatal_exit_rx;
        client.shutdown_completed.store(true, Ordering::Release);
        let notifications = client.poll_notifications();
        assert_eq!(
            notifications,
            vec![TmuxNotification::Exit(Some(
                "control-mode failure".to_string()
            ))]
        );
    }

    #[test]
    fn shutdown_latch_resets_after_failed_attempt() {
        let client = test_tmux_client(TmuxShutdownMode::DetachOnly);
        let attempts = Cell::new(0usize);

        let first = client.run_shutdown_attempt(|| {
            attempts.set(attempts.get() + 1);
            Err(anyhow!("forced shutdown failure"))
        });
        assert!(first.is_err());
        assert_eq!(attempts.get(), 1);
        assert!(!client.shutdown_in_progress.load(Ordering::Acquire));
        assert!(!client.shutdown_completed.load(Ordering::Acquire));

        let second = client.run_shutdown_attempt(|| {
            attempts.set(attempts.get() + 1);
            Ok(())
        });
        assert!(second.is_ok());
        assert_eq!(attempts.get(), 2);
        assert!(client.shutdown_completed.load(Ordering::Acquire));
    }

    #[test]
    fn successful_shutdown_keeps_latch_completed() {
        let client = test_tmux_client(TmuxShutdownMode::DetachOnly);
        let attempts = Cell::new(0usize);

        let first = client.run_shutdown_attempt(|| {
            attempts.set(attempts.get() + 1);
            Ok(())
        });
        assert!(first.is_ok());
        assert_eq!(attempts.get(), 1);
        assert!(client.shutdown_completed.load(Ordering::Acquire));

        let second = client.run_shutdown_attempt(|| {
            attempts.set(attempts.get() + 1);
            Err(anyhow!("must not execute after successful shutdown"))
        });
        assert!(second.is_ok());
        assert_eq!(attempts.get(), 1);
    }

    #[test]
    fn shutdown_retry_after_forced_detach_failure_can_still_teardown() {
        let client = test_tmux_client(TmuxShutdownMode::DetachAndTeardownSession);
        let teardown_attempts = Cell::new(0usize);

        let first = client.run_shutdown_attempt(|| {
            run_shutdown_actions(
                TmuxShutdownMode::DetachAndTeardownSession,
                "test-session",
                || Err(anyhow!("forced detach failure")),
                || {
                    teardown_attempts.set(teardown_attempts.get() + 1);
                    Err(anyhow!("forced teardown failure"))
                },
            )
        });
        assert!(first.is_err());
        assert_eq!(teardown_attempts.get(), 1);
        assert!(!client.shutdown_completed.load(Ordering::Acquire));

        let second = client.run_shutdown_attempt(|| {
            run_shutdown_actions(
                TmuxShutdownMode::DetachAndTeardownSession,
                "test-session",
                || Ok(()),
                || {
                    teardown_attempts.set(teardown_attempts.get() + 1);
                    Ok(())
                },
            )
        });
        assert!(second.is_ok());
        assert_eq!(teardown_attempts.get(), 2);
        assert!(client.shutdown_completed.load(Ordering::Acquire));
    }

    #[test]
    fn control_channel_ordering_completes_only_after_token_suffix() {
        let token = "__termy_cmd_done_77";
        let partial = "row-1\nrow-2";
        let full = format!("{partial}\n{token}");
        assert_eq!(
            split_control_completion_token(full.as_str(), token),
            Some(partial.to_string())
        );
        assert_eq!(split_control_completion_token(partial, token), None);
    }

    #[test]
    fn backpressure_single_oversized_burst_forces_refresh_warning_without_exit() {
        let mut coalescer = NotificationCoalescer::with_output_byte_limit(8);
        coalescer
            .push(TmuxNotification::Output {
                pane_id: "%9".to_string(),
                bytes: b"0123456789abcdef".to_vec(),
            })
            .expect("coalescer should survive oversized burst");

        let drained = coalescer.drain();
        assert!(
            drained
                .iter()
                .any(|n| matches!(n, TmuxNotification::NeedsRefresh))
        );
        assert!(
            drained
                .iter()
                .any(|n| matches!(n, TmuxNotification::Warning(_)))
        );
        assert!(
            !drained
                .iter()
                .any(|n| matches!(n, TmuxNotification::Exit(_)))
        );
    }

    #[test]
    fn trim_trailing_line_terminators_preserves_trailing_spaces_and_tabs() {
        assert_eq!(
            trim_trailing_line_terminators(b"abc \t\r\n"),
            b"abc \t".as_slice()
        );
        assert_eq!(
            trim_trailing_line_terminators(b"abc \t"),
            b"abc \t".as_slice()
        );
    }
}
