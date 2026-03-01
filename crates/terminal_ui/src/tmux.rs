use anyhow::{Context, Result, anyhow};
use flume::{Receiver, Sender, unbounded};
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmuxWindowState {
    pub id: String,
    pub index: i32,
    pub name: String,
    pub layout: String,
    pub is_active: bool,
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
struct ControlCommandResult {
    output: String,
}

#[derive(Debug)]
struct ControlRequest {
    command: String,
    response_tx: Option<Sender<std::result::Result<ControlCommandResult, TmuxControlError>>>,
}

#[derive(Debug)]
struct PendingCommand {
    command: String,
    response_tx: Option<Sender<std::result::Result<ControlCommandResult, TmuxControlError>>>,
}

#[derive(Debug)]
struct ControlCommandBlock {
    pending: PendingCommand,
    output: String,
}

pub struct TmuxClient {
    session_name: String,
    teardown_on_drop: bool,
    request_tx: Sender<ControlRequest>,
    notifications_rx: Receiver<TmuxNotification>,
}

#[derive(Debug, Clone)]
struct SessionLaunchPlan {
    session_name: String,
    attach_existing: bool,
    teardown_on_drop: bool,
}

const PERSISTENT_SESSION_NAME: &str = "termy";

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
    command.arg("-CC").arg("new-session");
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

    pub fn new(config: TmuxRuntimeConfig, cols: u16, rows: u16) -> Result<Self> {
        #[cfg(not(unix))]
        {
            let _ = (cols, rows);
            return Err(anyhow!("tmux control mode is only supported on unix targets"));
        }

        let launch_plan = Self::launch_plan(&config);
        #[cfg(unix)]
        let (mut child, child_stdin, child_stdout) = spawn_tmux_control_mode(
            &config,
            launch_plan.session_name.as_str(),
            launch_plan.attach_existing,
        )?;

        let (request_tx, request_rx) = unbounded::<ControlRequest>();
        let (pending_tx, pending_rx) = unbounded::<PendingCommand>();
        let (notifications_tx, notifications_rx) = unbounded::<TmuxNotification>();

        std::thread::spawn(move || {
            let _ = child.wait();
        });

        std::thread::spawn(move || {
            let mut stdin = child_stdin;
            while let Ok(request) = request_rx.recv() {
                let command = request.command;
                if stdin.write_all(command.as_bytes()).is_err() {
                    if let Some(response_tx) = request.response_tx {
                        let _ = response_tx.send(Err(TmuxControlError::channel(
                            "failed to write command to tmux control stdin",
                        )));
                    }
                    break;
                }
                if stdin.write_all(b"\n").is_err() {
                    if let Some(response_tx) = request.response_tx {
                        let _ = response_tx.send(Err(TmuxControlError::channel(
                            "failed to write command terminator to tmux control stdin",
                        )));
                    }
                    break;
                }
                if stdin.flush().is_err() {
                    if let Some(response_tx) = request.response_tx {
                        let _ = response_tx.send(Err(TmuxControlError::channel(
                            "failed to flush tmux control stdin",
                        )));
                    }
                    break;
                }

                if pending_tx
                    .send(PendingCommand {
                        command,
                        response_tx: request.response_tx,
                    })
                    .is_err()
                {
                    break;
                }
            }
        });

        std::thread::spawn(move || {
            const PENDING_MATCH_TIMEOUT: Duration = Duration::from_millis(80);

            let mut reader = BufReader::new(child_stdout);
            let mut line = Vec::<u8>::new();
            let mut pending_queue = VecDeque::<PendingCommand>::new();
            let mut current_block = None::<ControlCommandBlock>;

            let fail_pending = |pending: PendingCommand, error: TmuxControlError| {
                if let Some(response_tx) = pending.response_tx {
                    let _ = response_tx.send(Err(error));
                }
            };

            let fail_all_pending =
                |current_block: &mut Option<ControlCommandBlock>,
                 pending_queue: &mut VecDeque<PendingCommand>,
                 pending_rx: &Receiver<PendingCommand>,
                 error: TmuxControlError| {
                    if let Some(block) = current_block.take() {
                        fail_pending(block.pending, error.clone());
                    }
                    while let Some(pending) = pending_queue.pop_front() {
                        fail_pending(pending, error.clone());
                    }
                    while let Ok(pending) = pending_rx.try_recv() {
                        fail_pending(pending, error.clone());
                    }
                };

            loop {
                line.clear();
                let read = reader.read_until(b'\n', &mut line);
                let Ok(read) = read else {
                    fail_all_pending(
                        &mut current_block,
                        &mut pending_queue,
                        &pending_rx,
                        TmuxControlError::channel("tmux control mode read failure"),
                    );
                    let _ = notifications_tx.send(TmuxNotification::Exit(Some(
                        "tmux control mode read failure".to_string(),
                    )));
                    break;
                };

                if read == 0 {
                    fail_all_pending(
                        &mut current_block,
                        &mut pending_queue,
                        &pending_rx,
                        TmuxControlError::channel("tmux control mode closed"),
                    );
                    let _ = notifications_tx.send(TmuxNotification::Exit(None));
                    break;
                }

                while matches!(line.last(), Some(b'\n' | b'\r')) {
                    line.pop();
                }
                if line.is_empty() {
                    continue;
                }

                while let Ok(pending) = pending_rx.try_recv() {
                    pending_queue.push_back(pending);
                }

                if line.starts_with(b"%begin") {
                    if current_block.is_some() {
                        if let Some(block) = current_block.take() {
                            fail_pending(
                                block.pending,
                                TmuxControlError::protocol(
                                    "received nested %begin while previous command block was open",
                                ),
                            );
                        }
                    }
                    let pending = if let Some(pending) = pending_queue.pop_front() {
                        Some(pending)
                    } else {
                        pending_rx.recv_timeout(PENDING_MATCH_TIMEOUT).ok()
                    };

                    if let Some(pending) = pending {
                        current_block = Some(ControlCommandBlock {
                            pending,
                            output: String::new(),
                        });
                    }
                    continue;
                }

                if line.starts_with(b"%end") || line.starts_with(b"%error") {
                    let is_error = line.starts_with(b"%error");
                    if let Some(block) = current_block.take()
                        && let Some(response_tx) = block.pending.response_tx
                    {
                        let trimmed = block.output.trim();
                        let response = if is_error {
                            Err(TmuxControlError::runtime(if trimmed.is_empty() {
                                format!("command '{}' failed", block.pending.command)
                            } else {
                                trimmed.to_string()
                            }))
                        } else {
                            Ok(ControlCommandResult {
                                output: block.output,
                            })
                        };
                        let _ = response_tx.send(response);
                    }
                    continue;
                }

                // Notifications can interleave with both async and sync command blocks.
                // Always route pane output/layout events to the UI instead of treating
                // them as command stdout, otherwise redraw control bytes can be dropped.
                if let Some((pane_id, bytes)) = parse_output_notification(&line) {
                    let _ = notifications_tx.send(TmuxNotification::Output { pane_id, bytes });
                    continue;
                }
                if is_refresh_notification(&line) {
                    let _ = notifications_tx.send(TmuxNotification::NeedsRefresh);
                    continue;
                }

                if let Some(block) = current_block.as_mut() {
                    let line_text = match std::str::from_utf8(&line) {
                        Ok(line_text) => line_text,
                        Err(_) => {
                            if let Some(block) = current_block.take() {
                                fail_pending(
                                    block.pending,
                                    TmuxControlError::parse(
                                        "received non-utf8 bytes in control command output",
                                    ),
                                );
                            }
                            continue;
                        }
                    };
                    if !block.output.is_empty() {
                        block.output.push('\n');
                    }
                    block.output.push_str(line_text);
                    continue;
                }

                if line.starts_with(b"%exit") {
                    let reason = std::str::from_utf8(&line)
                        .ok()
                        .and_then(|value| value.strip_prefix("%exit"))
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned);
                    let _ = notifications_tx.send(TmuxNotification::Exit(reason));
                    break;
                }
            }
        });

        let client = Self {
            session_name: launch_plan.session_name,
            teardown_on_drop: launch_plan.teardown_on_drop,
            request_tx,
            notifications_rx,
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
        self.notifications_rx.try_iter().collect()
    }

    pub fn refresh_snapshot(&self) -> Result<TmuxSnapshot> {
        let windows_output = self.run_control_capture_args(&[
            "list-windows",
            "-t",
            self.session_name.as_str(),
            "-F",
            "#{window_id}\t#{window_index}\t#{window_name}\t#{window_layout}\t#{window_active}",
        ])?;

        let panes_output = self.run_control_capture_args(&[
            "list-panes",
            "-s",
            "-t",
            self.session_name.as_str(),
            "-F",
            "#{pane_id}\t#{window_id}\t#{session_id}\t#{pane_active}\t#{pane_left}\t#{pane_top}\t#{pane_width}\t#{pane_height}\t#{cursor_x}\t#{cursor_y}",
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

        for chunk in bytes.chunks(256) {
            let mut command = format!("send-keys -t {} -H", pane_id);
            for byte in chunk {
                command.push(' ');
                command.push_str(&format!("{:02x}", byte));
            }
            self.send_control_command_async(command.as_str())?;
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

    fn send_control_command_async(&self, command: &str) -> Result<()> {
        self.request_tx
            .send(ControlRequest {
                command: command.to_string(),
                response_tx: None,
            })
            .map_err(|_| anyhow!("tmux control channel is closed"))
    }

    fn send_control_command_wait(&self, command: &str) -> Result<ControlCommandResult> {
        const CONTROL_COMMAND_TIMEOUT: Duration = Duration::from_secs(3);

        let (response_tx, response_rx) = flume::bounded(1);
        self.request_tx
            .send(ControlRequest {
                command: command.to_string(),
                response_tx: Some(response_tx),
            })
            .map_err(|_| anyhow!("tmux control channel is closed"))?;

        let response = response_rx.recv_timeout(CONTROL_COMMAND_TIMEOUT).map_err(|_| {
            anyhow!(TmuxControlError::channel(format!(
                "timed out waiting for command completion: '{}'",
                command
            )))
        })?;
        response.map_err(anyhow::Error::new)
    }

    fn run_control_capture_args(&self, args: &[&str]) -> Result<String> {
        let command = tmux_command_line(args);
        let result = self.send_control_command_wait(command.as_str())?;
        Ok(result.output)
    }

    fn run_control_status_args(&self, args: &[&str]) -> Result<()> {
        let command = tmux_command_line(args);
        self.send_control_command_wait(command.as_str())?;
        Ok(())
    }

    fn enforce_native_session_ui(&self) -> Result<()> {
        let session = self.session_name.as_str();
        let all_windows_target = format!("{session}:*");

        self.run_control_status_args(&[
            "set-environment",
            "-g",
            "-t",
            session,
            "TERMY_SHELL_INTEGRATION",
            "0",
        ])
        .context("failed to disable termy shell integration env in tmux session")?;
        self.run_control_status_args(&[
            "set-environment",
            "-g",
            "-u",
            "-t",
            session,
            "TERMY_TAB_TITLE_PREFIX",
        ])
        .context("failed to clear termy shell title prefix env in tmux session")?;
        self.run_control_status_args(&[
            "set-environment",
            "-g",
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

fn parse_snapshot(session_name: &str, windows: &str, panes: &str) -> Result<TmuxSnapshot> {
    let mut panes_by_window: HashMap<String, Vec<TmuxPaneState>> = HashMap::new();
    let mut session_id = None::<String>;

    for line in panes.lines().filter(|line| !line.trim().is_empty()) {
        let mut parts = line.split('\t');
        let pane_id = parts
            .next()
            .ok_or_else(|| anyhow!("invalid tmux pane line: '{}'", line))?
            .to_string();
        let window_id = parts
            .next()
            .ok_or_else(|| anyhow!("invalid tmux pane line: '{}'", line))?
            .to_string();
        let pane_session_id = parts
            .next()
            .ok_or_else(|| anyhow!("invalid tmux pane line: '{}'", line))?
            .to_string();
        let is_active = parts
            .next()
            .ok_or_else(|| anyhow!("invalid tmux pane line: '{}'", line))?
            == "1";
        let left = parts
            .next()
            .ok_or_else(|| anyhow!("invalid tmux pane line: '{}'", line))?
            .parse::<u16>()
            .with_context(|| format!("invalid pane_left in '{}'", line))?;
        let top = parts
            .next()
            .ok_or_else(|| anyhow!("invalid tmux pane line: '{}'", line))?
            .parse::<u16>()
            .with_context(|| format!("invalid pane_top in '{}'", line))?;
        let width = parts
            .next()
            .ok_or_else(|| anyhow!("invalid tmux pane line: '{}'", line))?
            .parse::<u16>()
            .with_context(|| format!("invalid pane_width in '{}'", line))?;
        let height = parts
            .next()
            .ok_or_else(|| anyhow!("invalid tmux pane line: '{}'", line))?
            .parse::<u16>()
            .with_context(|| format!("invalid pane_height in '{}'", line))?;
        let cursor_x = parts
            .next()
            .ok_or_else(|| anyhow!("invalid tmux pane line: '{}'", line))?
            .parse::<u16>()
            .with_context(|| format!("invalid cursor_x in '{}'", line))?;
        let cursor_y = parts
            .next()
            .ok_or_else(|| anyhow!("invalid tmux pane line: '{}'", line))?
            .parse::<u16>()
            .with_context(|| format!("invalid cursor_y in '{}'", line))?;

        if parts.next().is_some() {
            return Err(anyhow!("invalid tmux pane line: '{}'", line));
        }

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
            });
    }

    let mut parsed_windows = Vec::new();
    for line in windows.lines().filter(|line| !line.trim().is_empty()) {
        let mut parts = line.split('\t');
        let window_id = parts
            .next()
            .ok_or_else(|| anyhow!("invalid tmux window line: '{}'", line))?
            .to_string();
        let index = parts
            .next()
            .ok_or_else(|| anyhow!("invalid tmux window line: '{}'", line))?
            .parse::<i32>()
            .with_context(|| format!("invalid window_index in '{}'", line))?;
        let name = parts
            .next()
            .ok_or_else(|| anyhow!("invalid tmux window line: '{}'", line))?
            .to_string();
        let layout = parts
            .next()
            .ok_or_else(|| anyhow!("invalid tmux window line: '{}'", line))?
            .to_string();
        let is_active = parts
            .next()
            .ok_or_else(|| anyhow!("invalid tmux window line: '{}'", line))?
            == "1";
        if parts.next().is_some() {
            return Err(anyhow!("invalid tmux window line: '{}'", line));
        }

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
        PERSISTENT_SESSION_NAME, TmuxClient, TmuxRuntimeConfig, managed_session_name,
        parse_output_notification, parse_snapshot, parse_version_prefix, quote_tmux_arg,
        strip_legacy_title_sequences, unescape_tmux_payload,
    };

    fn bytes_contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|window| window == needle)
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
    fn parse_snapshot_builds_windows_and_panes() {
        let windows = "@1\t0\tone\tlayout-a\t1\n@2\t1\ttwo\tlayout-b\t0\n";
        let panes = "%1\t@1\t$1\t1\t0\t0\t80\t24\t13\t22\n%2\t@2\t$1\t1\t0\t0\t60\t24\t7\t2\n%3\t@2\t$1\t0\t61\t0\t19\t24\t3\t8\n";
        let snapshot = parse_snapshot("termy", windows, panes).expect("snapshot");
        assert_eq!(snapshot.windows.len(), 2);
        assert_eq!(snapshot.windows[0].id, "@1");
        assert_eq!(snapshot.windows[0].panes.len(), 1);
        assert_eq!(snapshot.windows[1].panes.len(), 2);
        assert_eq!(snapshot.windows[0].panes[0].cursor_x, 13);
        assert_eq!(snapshot.windows[0].panes[0].cursor_y, 22);
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
