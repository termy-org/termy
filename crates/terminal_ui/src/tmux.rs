use anyhow::{Context, Result, anyhow};
use flume::{Receiver, RecvTimeoutError, Sender, TrySendError, bounded};
use std::collections::{HashMap, VecDeque};
#[cfg(unix)]
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
#[cfg(unix)]
use std::{
    os::fd::{FromRawFd, IntoRawFd},
    process::Stdio,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxRuntimeConfig {
    pub persistence: bool,
    pub binary: String,
}

impl Default for TmuxRuntimeConfig {
    fn default() -> Self {
        Self {
            persistence: false,
            binary: "tmux".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxPaneState {
    pub id: String,
    pub window_id: String,
    pub session_id: String,
    pub is_active: bool,
    pub left: u16,
    pub top: u16,
    pub width: u16,
    pub height: u16,
    pub cursor_x: u16,
    pub cursor_y: u16,
    pub current_path: String,
    pub current_command: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxWindowState {
    pub id: String,
    pub index: i32,
    pub name: String,
    pub layout: String,
    pub is_active: bool,
    pub automatic_rename: bool,
    pub active_pane_id: Option<String>,
    pub panes: Vec<TmuxPaneState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxSnapshot {
    pub session_name: String,
    pub session_id: Option<String>,
    pub windows: Vec<TmuxWindowState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TmuxNotification {
    Output { pane_id: String, bytes: Vec<u8> },
    NeedsRefresh,
    Exit(Option<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TmuxControlErrorKind {
    Channel,
    Protocol,
    Parse,
    Runtime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxControlError {
    pub kind: TmuxControlErrorKind,
    pub message: String,
}

impl TmuxControlError {
    fn channel(message: impl Into<String>) -> Self {
        Self {
            kind: TmuxControlErrorKind::Channel,
            message: message.into(),
        }
    }

    fn protocol(message: impl Into<String>) -> Self {
        Self {
            kind: TmuxControlErrorKind::Protocol,
            message: message.into(),
        }
    }

    fn parse(message: impl Into<String>) -> Self {
        Self {
            kind: TmuxControlErrorKind::Parse,
            message: message.into(),
        }
    }

    fn runtime(message: impl Into<String>) -> Self {
        Self {
            kind: TmuxControlErrorKind::Runtime,
            message: message.into(),
        }
    }
}

impl std::fmt::Display for TmuxControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let kind = match self.kind {
            TmuxControlErrorKind::Channel => "channel",
            TmuxControlErrorKind::Protocol => "protocol",
            TmuxControlErrorKind::Parse => "parse",
            TmuxControlErrorKind::Runtime => "runtime",
        };
        write!(f, "tmux control {} error: {}", kind, self.message)
    }
}

impl std::error::Error for TmuxControlError {}

#[derive(Debug)]
struct ControlRequest {
    command: String,
    response_tx: Option<Sender<std::result::Result<(), TmuxControlError>>>,
}

#[derive(Debug)]
struct PendingCommand {
    command: String,
    response_tx: Option<Sender<std::result::Result<(), TmuxControlError>>>,
    completion_tx: Sender<()>,
}

#[derive(Debug)]
enum ActiveControlCommand {
    Tracked(PendingCommand),
    Untracked,
}

#[derive(Debug)]
struct ControlCommandBlock {
    command_tag: String,
    output: String,
}

#[derive(Debug)]
struct NotificationCoalescer {
    queued: VecDeque<TmuxNotification>,
    has_refresh_queued: bool,
    queued_output_bytes: usize,
    max_output_bytes: usize,
}

impl Default for NotificationCoalescer {
    fn default() -> Self {
        Self::with_output_byte_limit(NOTIFICATION_COALESCER_OUTPUT_BYTES_BOUND)
    }
}

impl NotificationCoalescer {
    fn with_output_byte_limit(max_output_bytes: usize) -> Self {
        Self {
            queued: VecDeque::new(),
            has_refresh_queued: false,
            queued_output_bytes: 0,
            max_output_bytes,
        }
    }

    fn push(&mut self, notification: TmuxNotification) -> std::result::Result<(), TmuxControlError> {
        match notification {
            TmuxNotification::NeedsRefresh => {
                if !self.has_refresh_queued {
                    self.has_refresh_queued = true;
                    self.queued.push_back(TmuxNotification::NeedsRefresh);
                }
            }
            TmuxNotification::Output { pane_id, bytes } => {
                if bytes.is_empty() {
                    return Ok(());
                }

                let queued_output_bytes = self.queued_output_bytes.checked_add(bytes.len()).ok_or_else(|| {
                    TmuxControlError::channel("tmux notification output backlog byte-count overflowed")
                })?;
                if queued_output_bytes > self.max_output_bytes {
                    return Err(TmuxControlError::channel(format!(
                        "tmux notification output backlog exceeded {} bytes",
                        self.max_output_bytes
                    )));
                }

                if let Some(TmuxNotification::Output {
                    pane_id: tail_pane_id,
                    bytes: tail_bytes,
                }) = self.queued.back_mut()
                {
                    if *tail_pane_id == pane_id {
                        tail_bytes.extend_from_slice(&bytes);
                        self.queued_output_bytes = queued_output_bytes;
                        return Ok(());
                    }
                }

                self.queued.push_back(TmuxNotification::Output { pane_id, bytes });
                self.queued_output_bytes = queued_output_bytes;
            }
            TmuxNotification::Exit(reason) => {
                // Exit is terminal for the UI client. Drop stale backlog so the
                // consumer sees one deterministic shutdown signal.
                self.queued.clear();
                self.has_refresh_queued = false;
                self.queued_output_bytes = 0;
                self.queued.push_back(TmuxNotification::Exit(reason));
            }
        }

        Ok(())
    }

    fn pop_next(&mut self) -> Option<TmuxNotification> {
        let notification = self.queued.pop_front()?;
        match &notification {
            TmuxNotification::NeedsRefresh => {
                self.has_refresh_queued = false;
            }
            TmuxNotification::Output { bytes, .. } => {
                self.queued_output_bytes = self.queued_output_bytes.saturating_sub(bytes.len());
            }
            TmuxNotification::Exit(_) => {}
        }
        Some(notification)
    }

    fn drain(&mut self) -> Vec<TmuxNotification> {
        let mut drained = Vec::with_capacity(self.queued.len());
        while let Some(notification) = self.pop_next() {
            drained.push(notification);
        }
        drained
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ControlStateEvent {
    None,
    Notification(TmuxNotification),
    CommandBegin,
    CommandComplete {
        is_error: bool,
        output: String,
    },
    Exit(Option<String>),
}

#[derive(Debug, Default)]
struct ControlStateMachine {
    current_block: Option<ControlCommandBlock>,
}

impl ControlStateMachine {
    fn on_line(&mut self, line: &[u8]) -> std::result::Result<ControlStateEvent, TmuxControlError> {
        let line = strip_control_line_wrappers(line);
        if line.is_empty() {
            return Ok(ControlStateEvent::None);
        }

        if line.starts_with(b"%begin") {
            if self.current_block.is_some() {
                return Err(TmuxControlError::protocol(
                    "received nested %begin while previous command block was open",
                ));
            }
            let command_tag = parse_control_block_tag(line, b"%begin")?;
            self.current_block = Some(ControlCommandBlock {
                command_tag,
                output: String::new(),
            });
            return Ok(ControlStateEvent::CommandBegin);
        }

        if line.starts_with(b"%end") || line.starts_with(b"%error") {
            let is_error = line.starts_with(b"%error");
            let command_tag = parse_control_block_tag(line, if is_error { b"%error" } else { b"%end" })?;
            let block = self.current_block.take().ok_or_else(|| {
                TmuxControlError::protocol("received command terminator without open %begin block")
            })?;
            if command_tag != block.command_tag {
                return Err(TmuxControlError::protocol(format!(
                    "received mismatched command terminator: expected '{}', got '{}'",
                    block.command_tag, command_tag
                )));
            }
            return Ok(ControlStateEvent::CommandComplete {
                is_error,
                output: block.output,
            });
        }

        // Notifications can interleave with both async and sync command blocks.
        // Always route pane output/layout events to the UI instead of treating
        // them as command stdout, otherwise redraw control bytes can be dropped.
        if let Some((pane_id, bytes)) = parse_output_notification(line) {
            return Ok(ControlStateEvent::Notification(TmuxNotification::Output {
                pane_id,
                bytes,
            }));
        }
        if is_refresh_notification(line) {
            return Ok(ControlStateEvent::Notification(TmuxNotification::NeedsRefresh));
        }
        if line.starts_with(b"%exit") {
            return Ok(ControlStateEvent::Exit(parse_exit_reason(line)));
        }

        if let Some(block) = self.current_block.as_mut() {
            let line_text = std::str::from_utf8(line).map_err(|_| {
                TmuxControlError::parse("received non-utf8 bytes in control command output")
            })?;
            if !block.output.is_empty() {
                block.output.push('\n');
            }
            block.output.push_str(line_text);
        }

        Ok(ControlStateEvent::None)
    }
}

fn strip_control_line_wrappers(mut line: &[u8]) -> &[u8] {
    // tmux control mode may wrap protocol lines in DCS passthrough sequences
    // (for example: ESC P1000p ... ESC \\). Strip wrappers so parser matching
    // stays stable across tmux/terminal combinations.
    while let Some(rest) = line.strip_prefix(b"\x1bP") {
        let Some(end_idx) = rest.iter().position(|byte| *byte == b'p') else {
            break;
        };
        line = &rest[end_idx + 1..];
    }

    while let Some(rest) = line.strip_suffix(b"\x1b\\") {
        line = rest;
    }

    line
}

pub struct TmuxClient {
    tmux_binary: String,
    session_name: String,
    teardown_on_drop: bool,
    request_tx: Sender<ControlRequest>,
    notifications_rx: Receiver<TmuxNotification>,
    fatal_exit_rx: Receiver<Option<String>>,
}

#[derive(Debug, Clone)]
struct SessionLaunchPlan {
    session_name: String,
    attach_existing: bool,
    teardown_on_drop: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SendInputMode {
    ChunkedHex,
    Bulk,
}

const PERSISTENT_SESSION_NAME: &str = "termy";
const TERMY_TMUX_SOCKET_NAME: &str = "termy";
const REQUEST_QUEUE_BOUND: usize = 1024;
const PENDING_QUEUE_BOUND: usize = 1;
const PENDING_MATCH_TIMEOUT_MS: u64 = 80;
const NOTIFICATION_QUEUE_BOUND: usize = 2048;
const FATAL_EXIT_QUEUE_BOUND: usize = 1;
const NOTIFICATION_COALESCER_OUTPUT_BYTES_BOUND: usize = 512 * 1024;
const SEND_INPUT_CHUNKED_HEX_BYTES: usize = 256;
const SEND_INPUT_BULK_THRESHOLD_BYTES: usize = 2048;
const SEND_INPUT_BULK_HEX_BYTES: usize = 2048;
const SNAPSHOT_FIELD_SEP: char = '\u{1f}';
const WINDOW_SNAPSHOT_FORMAT: &str = concat!(
    "#{window_id}",
    "\u{1f}",
    "#{window_index}",
    "\u{1f}",
    "#{q:window_name}",
    "\u{1f}",
    "#{q:window_layout}",
    "\u{1f}",
    "#{window_active}",
    "\u{1f}",
    "#{automatic-rename}",
);
const PANE_SNAPSHOT_FORMAT: &str = concat!(
    "#{pane_id}",
    "\u{1f}",
    "#{window_id}",
    "\u{1f}",
    "#{session_id}",
    "\u{1f}",
    "#{pane_active}",
    "\u{1f}",
    "#{pane_left}",
    "\u{1f}",
    "#{pane_top}",
    "\u{1f}",
    "#{pane_width}",
    "\u{1f}",
    "#{pane_height}",
    "\u{1f}",
    "#{cursor_x}",
    "\u{1f}",
    "#{cursor_y}",
    "\u{1f}",
    "#{q:pane_current_path}",
    "\u{1f}",
    "#{q:pane_current_command}",
);

#[cfg(unix)]
fn spawn_tmux_control_mode(
    config: &TmuxRuntimeConfig,
    session_name: &str,
    attach_existing: bool,
) -> Result<(std::process::Child, File, File)> {
    let pty = rustix_openpty::openpty(None, None)
        .map_err(|error| anyhow!("failed to allocate tmux control pty: {error}"))?;

    let controller = unsafe { File::from_raw_fd(pty.controller.into_raw_fd()) };
    let user = unsafe { File::from_raw_fd(pty.user.into_raw_fd()) };

    let child_stdin = user
        .try_clone()
        .context("failed to clone tmux pty slave for stdin")?;
    let child_stdout = user
        .try_clone()
        .context("failed to clone tmux pty slave for stdout")?;
    let child_stderr = user;

    let mut command = Command::new(config.binary.as_str());
    // Use a dedicated socket so Termy control-mode runtime is isolated from any
    // user/default tmux server state and can start reliably.
    command
        .arg("-L")
        .arg(TERMY_TMUX_SOCKET_NAME)
        .arg("-CC")
        .arg("new-session");
    if attach_existing {
        command.arg("-A");
    }
    let child = command
        .arg("-s")
        .arg(session_name)
        // tmux windows/panes are authoritative in tmux runtime mode; disable
        // direct shell OSC integration hooks to avoid prompt-width drift artifacts.
        .env("TERMY_SHELL_INTEGRATION", "0")
        .env_remove("TERMY_TAB_TITLE_PREFIX")
        // zsh can emit inverse PROMPT_EOL_MARK (%) when line-state and repaint diverge.
        // Disable it for tmux-managed shells to avoid persistent visual artifacts.
        .env("PROMPT_EOL_MARK", "")
        .stdin(Stdio::from(child_stdin))
        .stdout(Stdio::from(child_stdout))
        .stderr(Stdio::from(child_stderr))
        .spawn()
        .with_context(|| {
            format!(
                "failed to spawn tmux control mode using '{}'",
                config.binary
            )
        })?;

    let writer = controller
        .try_clone()
        .context("failed to clone tmux pty controller for writer")?;

    Ok((child, writer, controller))
}

fn try_enqueue_control_request(
    request_tx: &Sender<ControlRequest>,
    request: ControlRequest,
) -> std::result::Result<(), TmuxControlError> {
    match request_tx.try_send(request) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(_)) => Err(TmuxControlError::channel(format!(
            "tmux control request queue is full (capacity {REQUEST_QUEUE_BOUND})"
        ))),
        Err(TrySendError::Disconnected(_)) => {
            Err(TmuxControlError::channel("tmux control channel is closed"))
        }
    }
}

fn try_send_notification(
    notifications_tx: &Sender<TmuxNotification>,
    notification: TmuxNotification,
) -> std::result::Result<(), TmuxControlError> {
    match notifications_tx.try_send(notification) {
        Ok(()) => Ok(()),
        Err(TrySendError::Full(_)) => Err(TmuxControlError::channel(format!(
            "tmux notification queue is full (capacity {NOTIFICATION_QUEUE_BOUND})"
        ))),
        Err(TrySendError::Disconnected(_)) => {
            Err(TmuxControlError::channel("tmux notification channel is closed"))
        }
    }
}

fn signal_event_wakeup(event_wakeup_tx: Option<&Sender<()>>) {
    let Some(event_wakeup_tx) = event_wakeup_tx else {
        return;
    };

    match event_wakeup_tx.try_send(()) {
        Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
    }
}

fn flush_notification_coalescer(
    coalescer: &mut NotificationCoalescer,
    notifications_tx: &Sender<TmuxNotification>,
    event_wakeup_tx: Option<&Sender<()>>,
) -> std::result::Result<(), TmuxControlError> {
    let mut sent_notifications = false;
    while let Some(notification) = coalescer.pop_next() {
        try_send_notification(notifications_tx, notification)?;
        sent_notifications = true;
    }
    if sent_notifications {
        signal_event_wakeup(event_wakeup_tx);
    }
    Ok(())
}

fn signal_fatal_exit(fatal_exit_tx: &Sender<Option<String>>, reason: Option<String>) {
    match fatal_exit_tx.try_send(reason) {
        Ok(()) | Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {}
    }
}

fn claim_pending_for_command_begin(
    pending_rx: &Receiver<PendingCommand>,
) -> std::result::Result<Option<PendingCommand>, TmuxControlError> {
    if let Ok(pending) = pending_rx.try_recv() {
        return Ok(Some(pending));
    }

    match pending_rx.recv_timeout(Duration::from_millis(PENDING_MATCH_TIMEOUT_MS)) {
        Ok(pending) => Ok(Some(pending)),
        Err(RecvTimeoutError::Timeout) => Ok(None),
        Err(RecvTimeoutError::Disconnected) => Err(TmuxControlError::channel(
            "tmux control pending-command channel closed",
        )),
    }
}

fn complete_pending_command(
    pending: PendingCommand,
    response: std::result::Result<(), TmuxControlError>,
) {
    if let Some(response_tx) = pending.response_tx {
        let _ = response_tx.send(response);
    }
    let _ = pending.completion_tx.send(());
}

fn map_command_completion_response(
    command: &str,
    is_error: bool,
    output: String,
) -> std::result::Result<(), TmuxControlError> {
    if !is_error {
        return Ok(());
    }

    let trimmed = output.trim();
    Err(TmuxControlError::runtime(if trimmed.is_empty() {
        format!("command '{command}' failed")
    } else {
        trimmed.to_string()
    }))
}

impl TmuxClient {
    fn launch_plan(config: &TmuxRuntimeConfig) -> SessionLaunchPlan {
        if config.persistence {
            SessionLaunchPlan {
                session_name: PERSISTENT_SESSION_NAME.to_string(),
                attach_existing: true,
                teardown_on_drop: false,
            }
        } else {
            SessionLaunchPlan {
                session_name: managed_session_name(),
                attach_existing: false,
                teardown_on_drop: true,
            }
        }
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
            return Err(anyhow!("tmux control mode is only supported on unix targets"));
        }

        let launch_plan = Self::launch_plan(&config);
        #[cfg(unix)]
        let (mut child, child_stdin, child_stdout) = spawn_tmux_control_mode(
            &config,
            launch_plan.session_name.as_str(),
            launch_plan.attach_existing,
        )?;

        let (request_tx, request_rx) = bounded::<ControlRequest>(REQUEST_QUEUE_BOUND);
        let (pending_tx, pending_rx) = bounded::<PendingCommand>(PENDING_QUEUE_BOUND);
        let (notifications_tx, notifications_rx) =
            bounded::<TmuxNotification>(NOTIFICATION_QUEUE_BOUND);
        let (fatal_exit_tx, fatal_exit_rx) = bounded::<Option<String>>(FATAL_EXIT_QUEUE_BOUND);

        std::thread::spawn(move || {
            let _ = child.wait();
        });

        std::thread::spawn(move || {
            let mut stdin = child_stdin;
            while let Ok(request) = request_rx.recv() {
                let command = request.command;
                let response_tx = request.response_tx;
                let (completion_tx, completion_rx) = flume::bounded(1);
                let response_tx_for_write_error = response_tx.clone();
                let completion_tx_for_write_error = completion_tx.clone();

                match pending_tx.try_send(PendingCommand {
                    command: command.clone(),
                    response_tx,
                    completion_tx,
                }) {
                    Ok(()) => {}
                    Err(TrySendError::Full(pending)) => {
                        complete_pending_command(
                            pending,
                            Err(TmuxControlError::channel("tmux control pending queue is full")),
                        );
                        break;
                    }
                    Err(TrySendError::Disconnected(pending)) => {
                        complete_pending_command(
                            pending,
                            Err(TmuxControlError::channel("tmux control reader is unavailable")),
                        );
                        break;
                    }
                }

                if stdin.write_all(command.as_bytes()).is_err() {
                    if let Some(response_tx) = response_tx_for_write_error {
                        let _ = response_tx.send(Err(TmuxControlError::channel(
                            "failed to write command to tmux control stdin",
                        )));
                    }
                    let _ = completion_tx_for_write_error.send(());
                    break;
                }
                if stdin.write_all(b"\n").is_err() {
                    if let Some(response_tx) = response_tx_for_write_error {
                        let _ = response_tx.send(Err(TmuxControlError::channel(
                            "failed to write command terminator to tmux control stdin",
                        )));
                    }
                    let _ = completion_tx_for_write_error.send(());
                    break;
                }
                if stdin.flush().is_err() {
                    if let Some(response_tx) = response_tx_for_write_error {
                        let _ = response_tx.send(Err(TmuxControlError::channel(
                            "failed to flush tmux control stdin",
                        )));
                    }
                    let _ = completion_tx_for_write_error.send(());
                    break;
                }
                if completion_rx.recv().is_err() {
                    break;
                }
            }
        });

        std::thread::spawn(move || {
            let mut reader = BufReader::new(child_stdout);
            let mut line = Vec::<u8>::new();
            let mut current_command = None::<ActiveControlCommand>;
            let mut control_state = ControlStateMachine::default();
            let mut notifications = NotificationCoalescer::default();

            let fail_pending = |pending: PendingCommand, error: TmuxControlError| {
                complete_pending_command(pending, Err(error));
            };
            let fail_all_pending =
                |current_command: &mut Option<ActiveControlCommand>,
                 pending_rx: &Receiver<PendingCommand>,
                 error: TmuxControlError| {
                    if let Some(active_command) = current_command.take()
                        && let ActiveControlCommand::Tracked(pending) = active_command
                    {
                        fail_pending(pending, error.clone());
                    }
                    while let Ok(pending) = pending_rx.try_recv() {
                        fail_pending(pending, error.clone());
                    }
                };
            let fail_with_exit =
                |current_command: &mut Option<ActiveControlCommand>,
                 pending_rx: &Receiver<PendingCommand>,
                 notifications_tx: &Sender<TmuxNotification>,
                 fatal_exit_tx: &Sender<Option<String>>,
                 notifications: &mut NotificationCoalescer,
                 event_wakeup_tx: Option<&Sender<()>>,
                 error: TmuxControlError| {
                    let exit_reason = Some(error.message.clone());
                    fail_all_pending(current_command, pending_rx, error);
                    signal_fatal_exit(fatal_exit_tx, exit_reason.clone());
                    let _ = notifications.push(TmuxNotification::Exit(exit_reason));
                    let _ = flush_notification_coalescer(
                        notifications,
                        notifications_tx,
                        event_wakeup_tx,
                    );
                };

            loop {
                line.clear();
                let read = reader.read_until(b'\n', &mut line);
                let Ok(read) = read else {
                    let exit_reason = Some("tmux control mode read failure".to_string());
                    fail_all_pending(
                        &mut current_command,
                        &pending_rx,
                        TmuxControlError::channel("tmux control mode read failure"),
                    );
                    signal_fatal_exit(&fatal_exit_tx, exit_reason.clone());
                    let _ = notifications.push(TmuxNotification::Exit(exit_reason));
                    let _ = flush_notification_coalescer(
                        &mut notifications,
                        &notifications_tx,
                        event_wakeup_tx.as_ref(),
                    );
                    break;
                };

                if read == 0 {
                    fail_all_pending(
                        &mut current_command,
                        &pending_rx,
                        TmuxControlError::channel("tmux control mode closed"),
                    );
                    signal_fatal_exit(&fatal_exit_tx, None);
                    let _ = notifications.push(TmuxNotification::Exit(None));
                    let _ = flush_notification_coalescer(
                        &mut notifications,
                        &notifications_tx,
                        event_wakeup_tx.as_ref(),
                    );
                    break;
                }

                while matches!(line.last(), Some(b'\n' | b'\r')) {
                    line.pop();
                }
                if line.is_empty() {
                    continue;
                }

                let event = match control_state.on_line(&line) {
                    Ok(event) => event,
                    Err(error) => {
                        fail_with_exit(
                            &mut current_command,
                            &pending_rx,
                            &notifications_tx,
                            &fatal_exit_tx,
                            &mut notifications,
                            event_wakeup_tx.as_ref(),
                            error,
                        );
                        break;
                    }
                };

                match event {
                    ControlStateEvent::None => {}
                    ControlStateEvent::Notification(notification) => {
                        if let Err(error) = notifications.push(notification) {
                            fail_with_exit(
                                &mut current_command,
                                &pending_rx,
                                &notifications_tx,
                                &fatal_exit_tx,
                                &mut notifications,
                                event_wakeup_tx.as_ref(),
                                error,
                            );
                            break;
                        }
                    }
                    ControlStateEvent::CommandBegin => {
                        // This explicit handoff removes race-based pending matching:
                        // the reader binds each %begin block to exactly one writer-issued request.
                        if current_command.is_some() {
                            fail_with_exit(
                                &mut current_command,
                                &pending_rx,
                                &notifications_tx,
                                &fatal_exit_tx,
                                &mut notifications,
                                event_wakeup_tx.as_ref(),
                                TmuxControlError::protocol(
                                    "received %begin while another command was still pending",
                                ),
                            );
                            break;
                        }

                        match claim_pending_for_command_begin(&pending_rx) {
                            Ok(Some(pending)) => {
                                current_command = Some(ActiveControlCommand::Tracked(pending));
                            }
                            Ok(None) => {
                                // tmux may emit unsolicited startup/control blocks before the
                                // first app-issued command is tracked; consume these blocks
                                // without binding a pending request.
                                current_command = Some(ActiveControlCommand::Untracked);
                            }
                            Err(error) => {
                                fail_with_exit(
                                    &mut current_command,
                                    &pending_rx,
                                    &notifications_tx,
                                    &fatal_exit_tx,
                                    &mut notifications,
                                    event_wakeup_tx.as_ref(),
                                    error,
                                );
                                break;
                            }
                        }
                    }
                    ControlStateEvent::CommandComplete { is_error, output } => {
                        let Some(active_command) = current_command.take() else {
                            fail_with_exit(
                                &mut current_command,
                                &pending_rx,
                                &notifications_tx,
                                &fatal_exit_tx,
                                &mut notifications,
                                event_wakeup_tx.as_ref(),
                                TmuxControlError::protocol(
                                    "received command completion without a pending request",
                                ),
                            );
                            break;
                        };

                        let ActiveControlCommand::Tracked(pending) = active_command else {
                            continue;
                        };

                        let response =
                            map_command_completion_response(&pending.command, is_error, output);
                        complete_pending_command(pending, response);
                    }
                    ControlStateEvent::Exit(reason) => {
                        fail_all_pending(
                            &mut current_command,
                            &pending_rx,
                            TmuxControlError::channel(
                                reason
                                    .clone()
                                    .unwrap_or_else(|| "tmux control mode exited".to_string()),
                            ),
                        );
                        signal_fatal_exit(&fatal_exit_tx, reason.clone());
                        let _ = notifications.push(TmuxNotification::Exit(reason));
                        let _ = flush_notification_coalescer(
                            &mut notifications,
                            &notifications_tx,
                            event_wakeup_tx.as_ref(),
                        );
                        break;
                    }
                }

                if let Err(error) = flush_notification_coalescer(
                    &mut notifications,
                    &notifications_tx,
                    event_wakeup_tx.as_ref(),
                )
                {
                    fail_with_exit(
                        &mut current_command,
                        &pending_rx,
                        &notifications_tx,
                        &fatal_exit_tx,
                        &mut notifications,
                        event_wakeup_tx.as_ref(),
                        error,
                    );
                    break;
                }
            }
        });

        let client = Self {
            tmux_binary: config.binary,
            session_name: launch_plan.session_name,
            teardown_on_drop: launch_plan.teardown_on_drop,
            request_tx,
            notifications_rx,
            fatal_exit_rx,
        };
        client.enforce_native_session_ui()?;
        client.set_client_size(cols, rows)?;
        Ok(client)
    }

    pub fn set_client_size(&self, cols: u16, rows: u16) -> Result<()> {
        let size = format!("{}x{}", cols, rows);
        self.run_control_status_args(&["refresh-client", "-C", size.as_str()])
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

    pub fn new_window(&self) -> Result<()> {
        self.run_control_status_args(&[
            "new-window",
            "-t",
            self.session_name.as_str(),
        ])
    }

    pub fn kill_window(&self, window_id: &str) -> Result<()> {
        self.run_control_status_args(&["kill-window", "-t", window_id])
    }

    pub fn rename_window(&self, window_id: &str, name: &str) -> Result<()> {
        self.run_control_status_args(&["rename-window", "-t", window_id, name])
    }

    pub fn previous_window(&self) -> Result<()> {
        self.run_control_status_args(&[
            "previous-window",
            "-t",
            self.session_name.as_str(),
        ])
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

    pub fn capture_pane(&self, pane_id: &str) -> Result<Vec<u8>> {
        let out = self.run_control_capture_args(&[
            "capture-pane",
            "-p",
            "-e",
            "-C",
            "-J",
            "-S",
            "-",
            "-E",
            "-",
            "-t",
            pane_id,
        ])?;
        Ok(sanitize_tmux_payload(unescape_tmux_payload(
            out.trim_end().as_bytes(),
        )))
    }

    pub fn capture_pane_viewport(&self, pane_id: &str, rows: u16) -> Result<Vec<u8>> {
        let start = format!("-{}", rows.max(1));
        let out = self.run_control_capture_args(&[
            "capture-pane",
            "-p",
            "-e",
            "-C",
            "-J",
            "-S",
            start.as_str(),
            "-E",
            "-",
            "-t",
            pane_id,
        ])?;
        Ok(sanitize_tmux_payload(unescape_tmux_payload(
            out.trim_end().as_bytes(),
        )))
    }

    pub fn verify_tmux_version(binary: &str, minimum_major: u8, minimum_minor: u8) -> Result<()> {
        let output = Command::new(binary)
            .arg("-V")
            .output()
            .with_context(|| format!("failed to execute '{}' -V", binary))?;
        if !output.status.success() {
            return Err(anyhow!("'{} -V' failed", binary));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let version = stdout
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| anyhow!("unable to parse tmux version output: '{}'", stdout.trim()))?;

        let (major, minor) = parse_version_prefix(version)
            .ok_or_else(|| anyhow!("unsupported tmux version format: '{}'", version))?;
        if (major, minor) < (minimum_major, minimum_minor) {
            return Err(anyhow!(
                "tmux {}.{}+ required, found {}",
                minimum_major,
                minimum_minor,
                version
            ));
        }

        Ok(())
    }

    fn enqueue_control_request(&self, request: ControlRequest) -> Result<()> {
        try_enqueue_control_request(&self.request_tx, request).map_err(anyhow::Error::new)
    }

    fn send_control_command_async(&self, command: &str) -> Result<()> {
        self.enqueue_control_request(ControlRequest {
            command: command.to_string(),
            response_tx: None,
        })
    }

    fn send_control_command_wait(&self, command: &str) -> Result<()> {
        const CONTROL_COMMAND_TIMEOUT: Duration = Duration::from_secs(3);

        let (response_tx, response_rx) = flume::bounded(1);
        self.enqueue_control_request(ControlRequest {
            command: command.to_string(),
            response_tx: Some(response_tx),
        })?;

        let response = response_rx.recv_timeout(CONTROL_COMMAND_TIMEOUT).map_err(|_| {
            anyhow!(TmuxControlError::channel(format!(
                "timed out waiting for command completion: '{}'",
                command
            )))
        })?;
        response.map_err(anyhow::Error::new)
    }

    fn run_control_capture_args(&self, args: &[&str]) -> Result<String> {
        let output = self
            .run_tmux_command(args)
            .with_context(|| format!("tmux capture command failed: {}", tmux_command_line(args)))?;
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn run_control_status_args(&self, args: &[&str]) -> Result<()> {
        self.run_tmux_command(args)
            .with_context(|| format!("tmux status command failed: {}", tmux_command_line(args)))?;
        Ok(())
    }

    fn run_tmux_command(&self, args: &[&str]) -> Result<std::process::Output> {
        let output = Command::new(self.tmux_binary.as_str())
            .arg("-L")
            .arg(TERMY_TMUX_SOCKET_NAME)
            .args(args)
            .output()
            .with_context(|| {
                format!(
                    "failed to execute tmux command via '{}': {}",
                    self.tmux_binary,
                    tmux_command_line(args)
                )
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = stderr.trim();
            if stderr.is_empty() {
                return Err(anyhow!(
                    "tmux command exited with status {}",
                    output
                        .status
                        .code()
                        .map_or_else(|| "signal".to_string(), |code| code.to_string())
                ));
            }
            return Err(anyhow!("{stderr}"));
        }

        Ok(output)
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
        self.run_control_status_args(&[
            "set-environment",
            "-t",
            session,
            "PROMPT_EOL_MARK",
            "",
        ])
        .context("failed to disable zsh prompt eol mark env in tmux session")?;

        self.run_control_status_args(&["set-option", "-q", "-t", session, "status", "off"])
            .context("failed to disable tmux status line for managed session")?;
        self.run_control_status_args(&[
            "set-window-option",
            "-q",
            "-t",
            all_windows_target.as_str(),
            "pane-border-status",
            "off",
        ])
        .context("failed to disable tmux pane border status for managed session")?;
        self.run_control_status_args(&[
            "set-window-option",
            "-q",
            "-t",
            all_windows_target.as_str(),
            "pane-border-format",
            "",
        ])
        .context("failed to clear tmux pane border format for managed session")?;
        self.run_control_status_args(&["refresh-client"])
            .context("failed to refresh tmux client after managed-session ui configuration")?;

        Ok(())
    }
}

impl Drop for TmuxClient {
    fn drop(&mut self) {
        if !self.teardown_on_drop {
            return;
        }

        let command = tmux_command_line(&["kill-session", "-t", self.session_name.as_str()]);
        if let Err(error) = self.send_control_command_wait(command.as_str()) {
            eprintln!(
                "Termy shutdown warning: failed to kill managed tmux session '{}': {}",
                self.session_name, error
            );
        }
    }
}

fn managed_session_name() -> String {
    let now_ns = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("termy-{}-{}", std::process::id(), now_ns)
}

fn tmux_command_line(args: &[&str]) -> String {
    args.iter()
        .map(|arg| quote_tmux_arg(arg))
        .collect::<Vec<_>>()
        .join(" ")
}

fn choose_send_input_mode(bytes_len: usize) -> (SendInputMode, usize) {
    if bytes_len >= SEND_INPUT_BULK_THRESHOLD_BYTES {
        return (
            SendInputMode::Bulk,
            bytes_len.div_ceil(SEND_INPUT_BULK_HEX_BYTES),
        );
    }

    (
        SendInputMode::ChunkedHex,
        bytes_len.div_ceil(SEND_INPUT_CHUNKED_HEX_BYTES),
    )
}

fn send_keys_hex_command(pane_id: &str, chunk: &[u8]) -> String {
    use std::fmt::Write as _;

    let mut command = String::with_capacity(18 + pane_id.len() + (chunk.len() * 3));
    command.push_str("send-keys -t ");
    command.push_str(pane_id);
    command.push_str(" -H");
    for byte in chunk {
        write!(&mut command, " {byte:02x}").expect("writing hex bytes into String cannot fail");
    }
    command
}

fn quote_tmux_arg(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    if value
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || b"-_./:@%+#,=".contains(&byte))
    {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', r"'\''"))
}

fn decode_snapshot_field(value: &str) -> Result<String> {
    let bytes = value.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        let current = bytes[index];
        if current != b'\\' {
            output.push(current);
            index += 1;
            continue;
        }

        index += 1;
        let escape = bytes
            .get(index)
            .ok_or_else(|| anyhow!("invalid trailing escape in snapshot field '{}'", value))?;
        match escape {
            b'\\' => {
                output.push(b'\\');
                index += 1;
            }
            b'n' => {
                output.push(b'\n');
                index += 1;
            }
            b'r' => {
                output.push(b'\r');
                index += 1;
            }
            b't' => {
                output.push(b'\t');
                index += 1;
            }
            b'x' => {
                let hex = bytes
                    .get(index + 1..index + 3)
                    .ok_or_else(|| anyhow!("invalid hex escape in snapshot field '{}'", value))?;
                let hi = (hex[0] as char)
                    .to_digit(16)
                    .ok_or_else(|| anyhow!("invalid hex escape in snapshot field '{}'", value))?;
                let lo = (hex[1] as char)
                    .to_digit(16)
                    .ok_or_else(|| anyhow!("invalid hex escape in snapshot field '{}'", value))?;
                output.push(((hi << 4) | lo) as u8);
                index += 3;
            }
            b'0'..=b'7' => {
                let octal = bytes
                    .get(index..index + 3)
                    .ok_or_else(|| anyhow!("invalid octal escape in snapshot field '{}'", value))?;
                if !octal.iter().all(|digit| (b'0'..=b'7').contains(digit)) {
                    return Err(anyhow!("invalid octal escape in snapshot field '{}'", value));
                }
                let decoded =
                    ((octal[0] - b'0') << 6) | ((octal[1] - b'0') << 3) | (octal[2] - b'0');
                output.push(decoded);
                index += 3;
            }
            _ => {
                // tmux `#{q:...}` uses shell-style escaping for punctuation and whitespace
                // (for example: "\[", "\(", "\ ", "\*", "\?"), so decode these as literals.
                output.push(*escape);
                index += 1;
            }
        }
    }

    String::from_utf8(output)
        .with_context(|| format!("snapshot field is not valid utf-8: '{}'", value))
}

fn parse_snapshot_fields<const N: usize>(line: &str, kind: &str) -> Result<[String; N]> {
    // Snapshot rows must have a fixed schema. Rejecting mismatched field counts
    // prevents silent record drift when delimiters appear unescaped in data.
    let fields = line
        .split(SNAPSHOT_FIELD_SEP)
        .map(decode_snapshot_field)
        .collect::<Result<Vec<_>>>()?;
    let field_count = fields.len();

    fields.try_into().map_err(|_| {
        anyhow!(
            "invalid tmux {kind} line: expected {N} fields, got {field_count}: '{}'",
            line
        )
    })
}

fn parse_snapshot_bool(value: &str, field: &str, kind: &str, line: &str) -> Result<bool> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(anyhow!("invalid {field} in tmux {kind} line: '{}'", line)),
    }
}

fn parse_snapshot_u16(value: &str, field: &str, kind: &str, line: &str) -> Result<u16> {
    value
        .parse::<u16>()
        .with_context(|| format!("invalid {field} in tmux {kind} line: '{}'", line))
}

fn parse_snapshot_i32(value: &str, field: &str, kind: &str, line: &str) -> Result<i32> {
    value
        .parse::<i32>()
        .with_context(|| format!("invalid {field} in tmux {kind} line: '{}'", line))
}

fn parse_snapshot(session_name: &str, windows: &str, panes: &str) -> Result<TmuxSnapshot> {
    let mut panes_by_window: HashMap<String, Vec<TmuxPaneState>> = HashMap::new();
    let mut session_id = None::<String>;

    for line in panes.lines().filter(|line| !line.trim().is_empty()) {
        let [
            pane_id,
            window_id,
            pane_session_id,
            pane_active,
            pane_left,
            pane_top,
            pane_width,
            pane_height,
            cursor_x,
            cursor_y,
            current_path,
            current_command,
        ] = parse_snapshot_fields::<12>(line, "pane")?;
        let is_active = parse_snapshot_bool(&pane_active, "pane_active", "pane", line)?;
        let left = parse_snapshot_u16(&pane_left, "pane_left", "pane", line)?;
        let top = parse_snapshot_u16(&pane_top, "pane_top", "pane", line)?;
        let width = parse_snapshot_u16(&pane_width, "pane_width", "pane", line)?;
        let height = parse_snapshot_u16(&pane_height, "pane_height", "pane", line)?;
        let cursor_x = parse_snapshot_u16(&cursor_x, "cursor_x", "pane", line)?;
        let cursor_y = parse_snapshot_u16(&cursor_y, "cursor_y", "pane", line)?;

        if session_id.is_none() {
            session_id = Some(pane_session_id.clone());
        }

        panes_by_window
            .entry(window_id.clone())
            .or_default()
            .push(TmuxPaneState {
                id: pane_id,
                window_id,
                session_id: pane_session_id,
                is_active,
                left,
                top,
                width,
                height,
                cursor_x,
                cursor_y,
                current_path,
                current_command,
            });
    }

    let mut parsed_windows = Vec::new();
    for line in windows.lines().filter(|line| !line.trim().is_empty()) {
        let [window_id, window_index, name, layout, window_active, automatic_rename] =
            parse_snapshot_fields::<6>(line, "window")?;
        let index = parse_snapshot_i32(&window_index, "window_index", "window", line)?;
        let is_active = parse_snapshot_bool(&window_active, "window_active", "window", line)?;
        let automatic_rename =
            parse_snapshot_bool(&automatic_rename, "automatic-rename", "window", line)?;

        let mut window_panes = panes_by_window.remove(&window_id).unwrap_or_default();
        window_panes.sort_by_key(|pane| (pane.top, pane.left));
        let active_pane_id = window_panes
            .iter()
            .find(|pane| pane.is_active)
            .map(|pane| pane.id.clone());

        parsed_windows.push(TmuxWindowState {
            id: window_id,
            index,
            name,
            layout,
            is_active,
            automatic_rename,
            active_pane_id,
            panes: window_panes,
        });
    }

    parsed_windows.sort_by_key(|window| window.index);

    Ok(TmuxSnapshot {
        session_name: session_name.to_string(),
        session_id,
        windows: parsed_windows,
    })
}

fn parse_control_block_tag(line: &[u8], marker: &[u8]) -> std::result::Result<String, TmuxControlError> {
    let Some(rest) = line.strip_prefix(marker) else {
        return Err(TmuxControlError::protocol(
            "failed to parse tmux control command marker",
        ));
    };
    let rest = rest.strip_prefix(b" ").unwrap_or(rest);
    let tag = std::str::from_utf8(rest)
        .map_err(|_| TmuxControlError::parse("received non-utf8 command marker metadata"))?;
    let tag = tag.trim();
    if tag.is_empty() {
        return Err(TmuxControlError::protocol(format!(
            "received '{}' without command metadata",
            String::from_utf8_lossy(marker)
        )));
    }
    Ok(tag.to_string())
}

fn parse_exit_reason(line: &[u8]) -> Option<String> {
    std::str::from_utf8(line)
        .ok()
        .and_then(|value| value.strip_prefix("%exit"))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn parse_output_notification(line: &[u8]) -> Option<(String, Vec<u8>)> {
    if let Some(rest) = line.strip_prefix(b"%output ") {
        let split = rest.iter().position(|byte| *byte == b' ')?;
        let pane_id = String::from_utf8(rest[..split].to_vec()).ok()?;
        let payload = &rest[split + 1..];
        return Some((pane_id, sanitize_tmux_payload(unescape_tmux_payload(payload))));
    }

    if let Some(rest) = line.strip_prefix(b"%extended-output ") {
        let colon_idx = rest.iter().position(|byte| *byte == b':')?;
        let header = &rest[..colon_idx];
        let mut header_parts = header.split(|byte| byte.is_ascii_whitespace());
        let pane_id = String::from_utf8(header_parts.next()?.to_vec()).ok()?;
        let mut payload = &rest[colon_idx + 1..];
        if let Some(b' ') = payload.first() {
            payload = &payload[1..];
        }
        return Some((pane_id, sanitize_tmux_payload(unescape_tmux_payload(payload))));
    }

    None
}

fn is_refresh_notification(line: &[u8]) -> bool {
    [
        b"%layout-change".as_slice(),
        b"%window-add".as_slice(),
        b"%window-close".as_slice(),
        b"%window-renamed".as_slice(),
        b"%window-pane-changed".as_slice(),
        b"%session-window-changed".as_slice(),
        b"%session-changed".as_slice(),
        b"%sessions-changed".as_slice(),
        b"%unlinked-window-add".as_slice(),
        b"%unlinked-window-close".as_slice(),
        b"%unlinked-window-renamed".as_slice(),
    ]
    .iter()
    .any(|prefix| line.starts_with(prefix))
}

fn unescape_tmux_payload(payload: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(payload.len());
    let mut index = 0;

    while index < payload.len() {
        if payload[index] == b'\\' && index + 3 < payload.len() {
            let oct = &payload[index + 1..index + 4];
            if oct.iter().all(|digit| (b'0'..=b'7').contains(digit)) {
                let value = ((oct[0] - b'0') << 6) | ((oct[1] - b'0') << 3) | (oct[2] - b'0');
                output.push(value);
                index += 4;
                continue;
            }
        }

        output.push(payload[index]);
        index += 1;
    }

    output
}

fn normalize_capture_payload(input: Vec<u8>) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len() + (input.len() / 4));
    for byte in input {
        if byte == b'\n' {
            if !matches!(output.last(), Some(b'\r')) {
                output.push(b'\r');
            }
            output.push(b'\n');
        } else {
            output.push(byte);
        }
    }
    output
}

fn sanitize_tmux_payload(input: Vec<u8>) -> Vec<u8> {
    strip_legacy_title_sequences(normalize_capture_payload(input))
}

fn strip_legacy_title_sequences(input: Vec<u8>) -> Vec<u8> {
    let mut output = Vec::with_capacity(input.len());
    let mut index = 0;

    while index < input.len() {
        if input[index] == 0x1b && index + 1 < input.len() && input[index + 1] == b'k' {
            index += 2;
            while index < input.len() {
                if input[index] == 0x07 {
                    index += 1;
                    break;
                }
                if input[index] == 0x1b && index + 1 < input.len() && input[index + 1] == b'\\' {
                    index += 2;
                    break;
                }
                index += 1;
            }
            continue;
        }

        output.push(input[index]);
        index += 1;
    }

    output
}

fn parse_version_prefix(version: &str) -> Option<(u8, u8)> {
    let mut digits = String::new();
    for ch in version.chars() {
        if ch.is_ascii_digit() || ch == '.' {
            digits.push(ch);
        } else {
            break;
        }
    }

    let mut parts = digits.split('.');
    let major = parts.next()?.parse::<u8>().ok()?;
    let minor = parts.next().unwrap_or("0").parse::<u8>().ok()?;
    Some((major, minor))
}

#[cfg(test)]
mod tests {
    use super::{
        ActiveControlCommand, ControlRequest, ControlStateEvent, ControlStateMachine,
        NotificationCoalescer, PERSISTENT_SESSION_NAME, PendingCommand, SNAPSHOT_FIELD_SEP,
        SendInputMode, TmuxClient, TmuxControlErrorKind, TmuxNotification, TmuxRuntimeConfig,
        choose_send_input_mode, claim_pending_for_command_begin, complete_pending_command,
        flush_notification_coalescer, managed_session_name, map_command_completion_response,
        parse_output_notification, parse_snapshot, parse_version_prefix, quote_tmux_arg,
        signal_fatal_exit, strip_legacy_title_sequences, try_enqueue_control_request,
        unescape_tmux_payload,
    };
    use std::time::Duration;

    fn bytes_contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|window| window == needle)
    }

    fn expect_command_complete(event: ControlStateEvent) -> (bool, String) {
        match event {
            ControlStateEvent::CommandComplete { is_error, output } => (is_error, output),
            other => panic!("expected command completion event, got {other:?}"),
        }
    }

    #[test]
    fn notification_coalescer_collapses_redundant_refresh_events() {
        let mut c = NotificationCoalescer::default();
        c.push(TmuxNotification::NeedsRefresh).expect("refresh");
        c.push(TmuxNotification::NeedsRefresh).expect("refresh");

        let drained = c.drain();
        let refresh_count = drained
            .iter()
            .filter(|notification| matches!(notification, TmuxNotification::NeedsRefresh))
            .count();
        assert_eq!(refresh_count, 1);
    }

    #[test]
    fn notification_coalescer_merges_adjacent_output_bursts_per_pane() {
        let mut c = NotificationCoalescer::default();
        c.push(TmuxNotification::Output {
            pane_id: "%1".to_string(),
            bytes: b"hello".to_vec(),
        })
        .expect("output");
        c.push(TmuxNotification::Output {
            pane_id: "%1".to_string(),
            bytes: b" world".to_vec(),
        })
        .expect("output");
        c.push(TmuxNotification::Output {
            pane_id: "%2".to_string(),
            bytes: b"!".to_vec(),
        })
        .expect("output");

        let drained = c.drain();
        assert_eq!(
            drained,
            vec![
                TmuxNotification::Output {
                    pane_id: "%1".to_string(),
                    bytes: b"hello world".to_vec(),
                },
                TmuxNotification::Output {
                    pane_id: "%2".to_string(),
                    bytes: b"!".to_vec(),
                }
            ]
        );
    }

    #[test]
    fn notification_coalescer_reports_backpressure_when_output_backlog_exceeds_limit() {
        let mut c = NotificationCoalescer::with_output_byte_limit(4);
        c.push(TmuxNotification::Output {
            pane_id: "%1".to_string(),
            bytes: b"abcd".to_vec(),
        })
        .expect("output");

        let err = c
            .push(TmuxNotification::Output {
                pane_id: "%1".to_string(),
                bytes: b"e".to_vec(),
            })
            .expect_err("backlog should fail");
        assert_eq!(err.kind, TmuxControlErrorKind::Channel);
        assert!(err.message.contains("backlog exceeded"));
    }

    #[test]
    fn notification_flush_signals_wakeup_when_notifications_are_enqueued() {
        let mut c = NotificationCoalescer::default();
        c.push(TmuxNotification::NeedsRefresh).expect("refresh");

        let (notifications_tx, notifications_rx) = flume::bounded(4);
        let (event_wakeup_tx, event_wakeup_rx) = flume::bounded(1);

        flush_notification_coalescer(&mut c, &notifications_tx, Some(&event_wakeup_tx))
            .expect("flush should succeed");

        assert!(matches!(
            notifications_rx.try_recv(),
            Ok(TmuxNotification::NeedsRefresh)
        ));
        assert!(event_wakeup_rx.try_recv().is_ok());
    }

    #[test]
    fn notification_flush_skips_wakeup_when_no_notifications_are_enqueued() {
        let mut c = NotificationCoalescer::default();
        let (notifications_tx, _notifications_rx) = flume::bounded(4);
        let (event_wakeup_tx, event_wakeup_rx) = flume::bounded(1);

        flush_notification_coalescer(&mut c, &notifications_tx, Some(&event_wakeup_tx))
            .expect("flush should succeed");

        assert!(event_wakeup_rx.try_recv().is_err());
    }

    #[test]
    fn request_enqueue_reports_backpressure_when_queue_is_full() {
        let (request_tx, _request_rx) = flume::bounded::<ControlRequest>(1);
        request_tx
            .try_send(ControlRequest {
                command: "first".to_string(),
                response_tx: None,
            })
            .expect("seed queue");

        let err = try_enqueue_control_request(
            &request_tx,
            ControlRequest {
                command: "second".to_string(),
                response_tx: None,
            },
        )
        .expect_err("full queue should fail");
        assert_eq!(err.kind, TmuxControlErrorKind::Channel);
        assert!(err.message.contains("request queue is full"));
    }

    #[test]
    fn unsolicited_command_block_does_not_consume_next_tracked_pending_request() {
        let mut sm = ControlStateMachine::default();
        let (pending_tx, pending_rx) = flume::bounded::<PendingCommand>(1);

        // Simulate unsolicited startup block before any app-issued command.
        assert_eq!(
            sm.on_line(b"%begin 1 1 0").expect("startup begin"),
            ControlStateEvent::CommandBegin
        );
        let mut active_command = match claim_pending_for_command_begin(&pending_rx).expect("claim") {
            Some(pending) => Some(ActiveControlCommand::Tracked(pending)),
            None => Some(ActiveControlCommand::Untracked),
        };
        assert!(matches!(active_command, Some(ActiveControlCommand::Untracked)));
        assert_eq!(
            sm.on_line(b"startup noise").expect("startup payload"),
            ControlStateEvent::None
        );
        expect_command_complete(sm.on_line(b"%end 1 1 0").expect("startup end"));
        match active_command.take().expect("startup block should be active") {
            ActiveControlCommand::Tracked(_) => {
                panic!("unsolicited block must stay untracked")
            }
            ActiveControlCommand::Untracked => {}
        }

        // Next block should claim the real pending request and deliver completion.
        let (response_tx, response_rx) = flume::bounded(1);
        let (completion_tx, completion_rx) = flume::bounded(1);
        pending_tx
            .send(PendingCommand {
                command: "list-windows".to_string(),
                response_tx: Some(response_tx),
                completion_tx,
            })
            .expect("queue pending request");

        assert_eq!(
            sm.on_line(b"%begin 2 2 0").expect("command begin"),
            ControlStateEvent::CommandBegin
        );
        active_command = match claim_pending_for_command_begin(&pending_rx).expect("claim") {
            Some(pending) => Some(ActiveControlCommand::Tracked(pending)),
            None => Some(ActiveControlCommand::Untracked),
        };
        assert!(matches!(
            active_command,
            Some(ActiveControlCommand::Tracked(_))
        ));
        assert_eq!(sm.on_line(b"ok").expect("command output"), ControlStateEvent::None);
        let (is_error, output) = expect_command_complete(sm.on_line(b"%end 2 2 0").expect("end"));
        let pending = match active_command.take().expect("tracked command") {
            ActiveControlCommand::Tracked(pending) => pending,
            ActiveControlCommand::Untracked => panic!("tracked request became untracked"),
        };
        let response = map_command_completion_response(&pending.command, is_error, output);
        complete_pending_command(pending, response);

        assert!(completion_rx.recv_timeout(Duration::from_millis(50)).is_ok());
        let result = response_rx
            .recv_timeout(Duration::from_millis(50))
            .expect("response sent");
        result.expect("tracked command should succeed");
    }

    #[test]
    fn command_completion_response_maps_error_output_to_runtime_error() {
        let error = map_command_completion_response("kill-pane", true, "pane not found\n".to_string())
            .expect_err("error completion should fail");
        assert_eq!(error.kind, TmuxControlErrorKind::Runtime);
        assert_eq!(error.message, "pane not found");

        let fallback_error = map_command_completion_response("kill-pane", true, " \n".to_string())
            .expect_err("empty error completion should fail");
        assert_eq!(fallback_error.kind, TmuxControlErrorKind::Runtime);
        assert_eq!(fallback_error.message, "command 'kill-pane' failed");
    }

    #[test]
    fn poll_notifications_prioritizes_dedicated_fatal_exit_signal() {
        let (request_tx, _request_rx) = flume::bounded::<ControlRequest>(1);
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

        let client = TmuxClient {
            tmux_binary: "tmux".to_string(),
            session_name: "test-session".to_string(),
            teardown_on_drop: false,
            request_tx,
            notifications_rx,
            fatal_exit_rx,
        };
        let notifications = client.poll_notifications();
        assert_eq!(
            notifications,
            vec![TmuxNotification::Exit(Some(
                "control-mode failure".to_string()
            ))]
        );
    }

    #[test]
    fn send_input_uses_chunked_hex_path_for_small_payloads() {
        let (mode, chunks) = choose_send_input_mode(1024);
        assert_eq!(mode, SendInputMode::ChunkedHex);
        assert_eq!(chunks, 4);
    }

    #[test]
    fn send_input_uses_bulk_path_for_large_payloads() {
        let (mode, chunks) = choose_send_input_mode(8192);
        assert_eq!(mode, SendInputMode::Bulk);
        assert_eq!(chunks, 4);
        assert!(chunks < 64);
    }

    #[test]
    fn send_input_switches_to_bulk_at_threshold() {
        let (small_mode, small_chunks) = choose_send_input_mode(2047);
        assert_eq!(small_mode, SendInputMode::ChunkedHex);
        assert_eq!(small_chunks, 8);

        let (bulk_mode, bulk_chunks) = choose_send_input_mode(2048);
        assert_eq!(bulk_mode, SendInputMode::Bulk);
        assert_eq!(bulk_chunks, 1);
    }

    #[test]
    fn output_unescape_decodes_octal_sequences() {
        let decoded = unescape_tmux_payload(b"hello\\040world\\015\\012");
        assert_eq!(decoded, b"hello world\r\n");
    }

    #[test]
    fn parse_output_handles_standard_output_line() {
        let (pane, bytes) = parse_output_notification(b"%output %3 hi\\012").expect("output");
        assert_eq!(pane, "%3");
        assert_eq!(bytes, b"hi\r\n");
    }

    #[test]
    fn parse_output_preserves_crlf_without_double_insert() {
        let (pane, bytes) =
            parse_output_notification(b"%output %5 hi\\015\\012there\\012").expect("output");
        assert_eq!(pane, "%5");
        assert_eq!(bytes, b"hi\r\nthere\r\n");
    }

    #[test]
    fn parse_output_handles_prompt_repaint_fragments() {
        let fragments: [&[u8]; 6] = [
            b"%output %77 c",
            b"%output %77 \\010cd\\040Desk",
            b"%output %77 \\010\\010\\010\\010\\010\\010\\010\\033[32mc\\033[32md\\033[39m\\033[5C",
            b"%output %77 \\015\\015\\012",
            b"%output %77 \\033kcd\\033\\134",
            b"%output %77 cd:\\040no\\040such\\040file\\040or\\040directory:\\040Desk\\015\\012",
        ];

        let mut merged = Vec::new();
        for fragment in fragments {
            let (pane, bytes) = parse_output_notification(fragment).expect("output");
            assert_eq!(pane, "%77");
            merged.extend_from_slice(&bytes);
        }

        assert!(bytes_contains(
            &merged,
            b"cd: no such file or directory: Desk\r\n"
        ));
        assert!(bytes_contains(&merged, b"\r\r\n"));
        assert!(!bytes_contains(&merged, b"cdcd:"));
        assert!(!bytes_contains(&merged, b"czsh:"));
    }

    #[test]
    fn parse_output_preserves_erase_heavy_suffix_spacing() {
        let (_, bytes) = parse_output_notification(
            b"%output %7 error\\015\\012\\040\\040\\040\\040\\040\\040\\040\\040\\015\\015",
        )
        .expect("output");
        assert_eq!(bytes, b"error\r\n        \r\r");
    }

    #[test]
    fn parse_output_strips_legacy_title_sequence() {
        let (_, bytes) = parse_output_notification(b"%output %9 \\033kcd\\033\\134")
            .expect("output");
        assert!(bytes.is_empty());
    }

    #[test]
    fn strip_legacy_title_sequence_preserves_surrounding_text() {
        let sanitized =
            strip_legacy_title_sequences(b"left\x1bkmy-title\x1b\\right".to_vec());
        assert_eq!(sanitized, b"leftright");
    }

    #[test]
    fn control_state_machine_keeps_notifications_out_of_command_output() {
        let mut sm = ControlStateMachine::default();
        assert_eq!(
            sm.on_line(b"%begin 10 3 0").expect("begin"),
            ControlStateEvent::CommandBegin
        );

        assert_eq!(
            sm.on_line(b"%output %1 hi\\012").expect("notification"),
            ControlStateEvent::Notification(TmuxNotification::Output {
                pane_id: "%1".to_string(),
                bytes: b"hi\r\n".to_vec(),
            })
        );

        assert_eq!(sm.on_line(b"ok").expect("output line"), ControlStateEvent::None);
        let (is_error, output) = expect_command_complete(sm.on_line(b"%end 10 3 0").expect("end"));
        assert!(!is_error);
        assert_eq!(output.trim(), "ok");
    }

    #[test]
    fn control_state_machine_reports_error_block_output() {
        let mut sm = ControlStateMachine::default();
        assert_eq!(
            sm.on_line(b"%begin 11 8 0").expect("begin"),
            ControlStateEvent::CommandBegin
        );
        assert_eq!(
            sm.on_line(b"pane not found").expect("output line"),
            ControlStateEvent::None
        );

        let (is_error, output) =
            expect_command_complete(sm.on_line(b"%error 11 8 0").expect("error"));
        assert!(is_error);
        assert_eq!(output.trim(), "pane not found");
    }

    #[test]
    fn control_state_machine_rejects_mismatched_end_marker() {
        let mut sm = ControlStateMachine::default();
        assert_eq!(
            sm.on_line(b"%begin 44 9 0").expect("begin"),
            ControlStateEvent::CommandBegin
        );
        let err = sm.on_line(b"%end 45 9 0").expect_err("mismatch should fail");
        assert_eq!(err.kind, TmuxControlErrorKind::Protocol);
        assert!(err.message.contains("mismatched command terminator"));
    }

    #[test]
    fn control_state_machine_routes_refresh_notifications_while_collecting_output() {
        let mut sm = ControlStateMachine::default();
        assert_eq!(
            sm.on_line(b"%begin 72 1 0").expect("begin"),
            ControlStateEvent::CommandBegin
        );
        assert_eq!(
            sm.on_line(b"%layout-change @1 tiled").expect("refresh"),
            ControlStateEvent::Notification(TmuxNotification::NeedsRefresh)
        );
        assert_eq!(
            sm.on_line(b"captured").expect("output line"),
            ControlStateEvent::None
        );

        let (is_error, output) =
            expect_command_complete(sm.on_line(b"%end 72 1 0").expect("end"));
        assert!(!is_error);
        assert_eq!(output, "captured");
    }

    #[test]
    fn control_state_machine_accepts_dcs_wrapped_control_markers() {
        let mut sm = ControlStateMachine::default();
        assert_eq!(
            sm.on_line(b"\x1bP1000p%begin 9 4 0").expect("wrapped begin"),
            ControlStateEvent::CommandBegin
        );
        assert_eq!(sm.on_line(b"ok").expect("output line"), ControlStateEvent::None);
        let (is_error, output) =
            expect_command_complete(sm.on_line(b"%end 9 4 0\x1b\\").expect("wrapped end"));
        assert!(!is_error);
        assert_eq!(output, "ok");
    }

    #[test]
    fn control_state_machine_ignores_standalone_dcs_terminator_line() {
        let mut sm = ControlStateMachine::default();
        assert_eq!(sm.on_line(b"\x1b\\").expect("dcs terminator"), ControlStateEvent::None);
    }

    #[test]
    fn parse_snapshot_builds_windows_and_panes() {
        let sep = SNAPSHOT_FIELD_SEP;
        let windows = format!(
            "@1{sep}0{sep}one{sep}layout-a{sep}1{sep}1\n@2{sep}1{sep}two{sep}layout-b{sep}0{sep}0\n",
        );
        let panes = format!(
            "%1{sep}@1{sep}$1{sep}1{sep}0{sep}0{sep}80{sep}24{sep}13{sep}22{sep}/tmp{sep}zsh\n\
             %2{sep}@2{sep}$1{sep}1{sep}0{sep}0{sep}60{sep}24{sep}7{sep}2{sep}/work{sep}sleep\n\
             %3{sep}@2{sep}$1{sep}0{sep}61{sep}0{sep}19{sep}24{sep}3{sep}8{sep}/work{sep}zsh\n",
        );
        let snapshot = parse_snapshot("termy", windows.as_str(), panes.as_str()).expect("snapshot");
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[0].id, "@1");
        assert_eq!(snapshot.windows[0].panes.len(), 1);
        assert_eq!(snapshot.windows[1].panes.len(), 2);
        assert!(snapshot.windows[0].automatic_rename);
        assert!(!snapshot.windows[1].automatic_rename);
        assert_eq!(snapshot.windows[0].panes[0].cursor_x, 13);
        assert_eq!(snapshot.windows[0].panes[0].cursor_y, 22);
        assert_eq!(snapshot.windows[0].panes[0].current_path, "/tmp");
        assert_eq!(snapshot.windows[1].panes[0].current_command, "sleep");
    }

    #[test]
    fn parse_snapshot_accepts_escaped_field_delimiters_in_window_name_and_command() {
        let sep = SNAPSHOT_FIELD_SEP;
        let windows =
            format!("@1{sep}0{sep}name\\x09with-tab\\x1fwindow{sep}layout\\x1fgrid{sep}1{sep}1\n");
        let panes = format!(
            "%1{sep}@1{sep}$1{sep}1{sep}0{sep}0{sep}80{sep}24{sep}0{sep}0{sep}/tmp\\x1fdir\\x09tab{sep}cmd\\x0awith-nl\\x1fpart\n",
        );
        let snapshot = parse_snapshot("termy", windows.as_str(), panes.as_str()).expect("snapshot");
        assert_eq!(snapshot.windows[0].name, "name\twith-tab\x1fwindow");
        assert_eq!(snapshot.windows[0].layout, "layout\x1fgrid");
        assert_eq!(snapshot.windows[0].panes[0].current_path, "/tmp\x1fdir\ttab");
        assert_eq!(
            snapshot.windows[0].panes[0].current_command,
            "cmd\nwith-nl\x1fpart"
        );
    }

    #[test]
    fn parse_snapshot_accepts_tmux_q_octal_escapes_for_tabs_newlines_and_delimiters() {
        let sep = SNAPSHOT_FIELD_SEP;
        let windows = format!(
            "@1{sep}0{sep}name\\011with-tab\\037window{sep}layout\\037grid{sep}1{sep}1\n"
        );
        let panes = format!(
            "%1{sep}@1{sep}$1{sep}1{sep}0{sep}0{sep}80{sep}24{sep}0{sep}0{sep}/tmp\\011dir\\037tab{sep}cmd\\012with-nl\\037part\n",
        );
        let snapshot = parse_snapshot("termy", windows.as_str(), panes.as_str()).expect("snapshot");
        assert_eq!(snapshot.windows[0].name, "name\twith-tab\x1fwindow");
        assert_eq!(snapshot.windows[0].layout, "layout\x1fgrid");
        assert_eq!(snapshot.windows[0].panes[0].current_path, "/tmp\tdir\x1ftab");
        assert_eq!(
            snapshot.windows[0].panes[0].current_command,
            "cmd\nwith-nl\x1fpart"
        );
    }

    #[test]
    fn parse_snapshot_accepts_tmux_q_shell_escaped_window_layout() {
        let sep = SNAPSHOT_FIELD_SEP;
        let windows = format!(
            "@1{sep}0{sep}one{sep}aeea,149x39,0,0{{74x39,0,0\\[74x19,0,0,0,74x19,0,20,2],74x39,75,0,1}}{sep}1{sep}1\n",
        );
        let panes =
            format!("%1{sep}@1{sep}$1{sep}1{sep}0{sep}0{sep}149{sep}39{sep}0{sep}0{sep}/tmp{sep}zsh\n");
        let snapshot = parse_snapshot("termy", windows.as_str(), panes.as_str()).expect("snapshot");
        assert_eq!(
            snapshot.windows[0].layout,
            "aeea,149x39,0,0{74x39,0,0[74x19,0,0,0,74x19,0,20,2],74x39,75,0,1}"
        );
    }

    #[test]
    fn parse_snapshot_accepts_tmux_q_shell_escaped_punctuation() {
        let sep = SNAPSHOT_FIELD_SEP;
        let windows = format!(
            "@1{sep}0{sep}name\\[a\\]\\(b\\)\\ c\\*d\\?e{sep}layout-a{sep}1{sep}1\n",
        );
        let panes = format!(
            "%1{sep}@1{sep}$1{sep}1{sep}0{sep}0{sep}80{sep}24{sep}0{sep}0{sep}/tmp\\ path\\[x\\]\\(y\\){sep}cmd\\ \\\"quoted\\\"\\ and\\ symbols\\*\\?\n",
        );
        let snapshot = parse_snapshot("termy", windows.as_str(), panes.as_str()).expect("snapshot");
        assert_eq!(snapshot.windows[0].name, "name[a](b) c*d?e");
        assert_eq!(snapshot.windows[0].panes[0].current_path, "/tmp path[x](y)");
        assert_eq!(
            snapshot.windows[0].panes[0].current_command,
            "cmd \"quoted\" and symbols*?"
        );
    }

    #[test]
    fn parse_snapshot_rejects_unescaped_field_separator_in_window_record() {
        let sep = SNAPSHOT_FIELD_SEP;
        let windows = format!("@1{sep}0{sep}broken{sep}name{sep}layout{sep}1{sep}1\n");
        let panes =
            format!("%1{sep}@1{sep}$1{sep}1{sep}0{sep}0{sep}80{sep}24{sep}0{sep}0{sep}/tmp{sep}zsh\n");
        let error = parse_snapshot("termy", windows.as_str(), panes.as_str()).unwrap_err();
        assert!(error.to_string().contains("expected 6 fields, got 7"));
    }

    #[test]
    fn parse_snapshot_rejects_invalid_hex_escape_in_fields() {
        let sep = SNAPSHOT_FIELD_SEP;
        let windows = format!("@1{sep}0{sep}name\\x0g{sep}layout{sep}1{sep}1\n");
        let panes =
            format!("%1{sep}@1{sep}$1{sep}1{sep}0{sep}0{sep}80{sep}24{sep}0{sep}0{sep}/tmp{sep}zsh\n");
        let error = parse_snapshot("termy", windows.as_str(), panes.as_str()).unwrap_err();
        assert!(error.to_string().contains("invalid hex escape"));
    }

    #[test]
    fn tmux_version_parser_accepts_patch_suffixes() {
        assert_eq!(parse_version_prefix("3.6a"), Some((3, 6)));
        assert_eq!(parse_version_prefix("3.3"), Some((3, 3)));
        assert_eq!(parse_version_prefix("2"), Some((2, 0)));
    }

    #[test]
    fn quote_tmux_arg_single_quotes_embedded_quotes() {
        assert_eq!(
            quote_tmux_arg("pane name with spaces and 'quote'"),
            "'pane name with spaces and '\\''quote'\\'''"
        );
    }

    #[test]
    fn persistent_launch_plan_reuses_fixed_session_without_teardown() {
        let plan = TmuxClient::launch_plan(&TmuxRuntimeConfig {
            persistence: true,
            binary: "tmux".to_string(),
        });
        assert_eq!(plan.session_name, PERSISTENT_SESSION_NAME);
        assert!(plan.attach_existing);
        assert!(!plan.teardown_on_drop);
    }

    #[test]
    fn isolated_launch_plan_uses_fresh_session_and_teardown() {
        let plan = TmuxClient::launch_plan(&TmuxRuntimeConfig {
            persistence: false,
            binary: "tmux".to_string(),
        });
        assert!(plan.session_name.starts_with("termy-"));
        assert!(!plan.attach_existing);
        assert!(plan.teardown_on_drop);
    }

    #[test]
    fn managed_session_name_prefix_is_stable() {
        let name = managed_session_name();
        assert!(name.starts_with("termy-"));
    }
}
