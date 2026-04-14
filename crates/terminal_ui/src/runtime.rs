use crate::grid::TerminalCursorStyle;
use crate::keyboard::TerminalKeyboardMode;
#[cfg(unix)]
use crate::locale::{Utf8LocaleOverridePlan, preferred_utf8_locale, utf8_locale_override_plan};
use crate::mouse_protocol::TerminalMouseMode;
use crate::osc_intercept::{OscEvent, OscInterceptor};
#[cfg(not(target_os = "windows"))]
use crate::path_env::normalized_path_env;
use crate::protocol::{TerminalQueryColors, TerminalReplyHost, reply_bytes_for_event};
use crate::render_metrics::increment_runtime_wakeup_count;
use crate::shell_integration::ProgressState;
use alacritty_terminal::{
    event::{Event as AlacEvent, EventListener, OnResize, WindowSize},
    grid::{Dimensions, Scroll},
    sync::FairMutex,
    term::{Config as TermConfig, LineDamageBounds, Term, TermDamage, TermMode},
    thread,
    tty::{self, EventedPty, EventedReadWrite, Options as PtyOptions, Shell},
    vte::ansi::{self, CursorShape, CursorStyle as AlacrittyCursorStyle},
};
use flume::{Receiver, Sender, unbounded};
use gpui::{Pixels, px};
use polling::{Event as PollingEvent, Events, PollMode, Poller};
#[cfg(not(target_os = "windows"))]
use std::path::Path;
use std::{
    borrow::Cow,
    collections::HashMap,
    env,
    io::{self, ErrorKind, Read, Write},
    num::NonZeroUsize,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver as StdReceiver, Sender as StdSender, TryRecvError},
    },
    time::Instant,
};

#[derive(Debug, Clone)]
pub struct TabTitleShellIntegration {
    pub enabled: bool,
    pub explicit_prefix: String,
}

const DEFAULT_TERM: &str = "xterm-256color";
const DEFAULT_COLORTERM: &str = "truecolor";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkingDirFallback {
    Home,
    Process,
}

#[allow(clippy::derivable_impls)]
impl Default for WorkingDirFallback {
    fn default() -> Self {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        {
            Self::Home
        }

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            Self::Process
        }
    }
}

const DEFAULT_SCROLLBACK_HISTORY: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalCursorState {
    pub col: usize,
    pub row: usize,
    pub style: TerminalCursorStyle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalOptions {
    pub scrollback_history: usize,
    pub default_cursor_style: TerminalCursorStyle,
}

impl Default for TerminalOptions {
    fn default() -> Self {
        Self {
            scrollback_history: DEFAULT_SCROLLBACK_HISTORY,
            default_cursor_style: TerminalCursorStyle::Block,
        }
    }
}

#[derive(Debug, Clone)]
pub struct TerminalRuntimeConfig {
    pub shell: Option<String>,
    pub term: String,
    pub colorterm: Option<String>,
    pub query_colors: TerminalQueryColors,
    pub working_dir_fallback: WorkingDirFallback,
    pub scrollback_history: usize,
    pub default_cursor_style: TerminalCursorStyle,
}

impl Default for TerminalRuntimeConfig {
    fn default() -> Self {
        Self {
            shell: None,
            term: DEFAULT_TERM.to_string(),
            colorterm: Some(DEFAULT_COLORTERM.to_string()),
            query_colors: TerminalQueryColors::default(),
            working_dir_fallback: WorkingDirFallback::default(),
            scrollback_history: DEFAULT_SCROLLBACK_HISTORY,
            default_cursor_style: TerminalCursorStyle::Block,
        }
    }
}

impl TerminalOptions {
    pub(crate) fn term_config(&self) -> TermConfig {
        let shape = match self.default_cursor_style {
            TerminalCursorStyle::Line => CursorShape::Beam,
            TerminalCursorStyle::Block => CursorShape::Block,
        };
        TermConfig {
            scrolling_history: self.scrollback_history,
            default_cursor_style: AlacrittyCursorStyle {
                shape,
                blinking: false,
            },
            kitty_keyboard: true,
            ..TermConfig::default()
        }
    }

    pub fn with_scrollback_history(self, scrollback_history: usize) -> Self {
        Self {
            scrollback_history,
            ..self
        }
    }
}

impl TerminalRuntimeConfig {
    pub fn term_options(&self) -> TerminalOptions {
        TerminalOptions {
            scrollback_history: self.scrollback_history,
            default_cursor_style: self.default_cursor_style,
        }
    }
}

fn terminal_cursor_style_from_shape(shape: CursorShape) -> Option<TerminalCursorStyle> {
    match shape {
        CursorShape::Hidden => None,
        // Collapse shapes we do not render distinctly yet onto the existing
        // two-style renderer rather than reintroducing a fake app-level cursor.
        CursorShape::Block | CursorShape::HollowBlock => Some(TerminalCursorStyle::Block),
        CursorShape::Underline | CursorShape::Beam => Some(TerminalCursorStyle::Line),
    }
}

pub(crate) fn cursor_state_from_term<T: EventListener>(
    term: &Term<T>,
) -> Option<TerminalCursorState> {
    let cursor = term.renderable_content().cursor;
    let style = terminal_cursor_style_from_shape(cursor.shape)?;
    let row = usize::try_from(cursor.point.line.0).ok()?;
    Some(TerminalCursorState {
        col: cursor.point.column.0,
        row,
        style,
    })
}

pub(crate) fn cursor_position_from_term<T: EventListener>(term: &Term<T>) -> (usize, usize) {
    let cursor = term.renderable_content().cursor;
    let row = usize::try_from(cursor.point.line.0).ok().unwrap_or(0);
    (cursor.point.column.0, row)
}

pub(crate) fn termmode_to_terminal_mouse_mode(mode: TermMode) -> TerminalMouseMode {
    TerminalMouseMode {
        enabled: mode.intersects(TermMode::MOUSE_MODE) && !mode.contains(TermMode::VI),
        report_click: mode.contains(TermMode::MOUSE_REPORT_CLICK),
        report_drag: mode.contains(TermMode::MOUSE_DRAG),
        report_motion: mode.contains(TermMode::MOUSE_MOTION),
        sgr_encoding: mode.contains(TermMode::SGR_MOUSE),
        utf8_encoding: mode.contains(TermMode::UTF8_MOUSE),
    }
}

/// On Windows, `CreateProcessW` splits `lpCommandLine` on spaces to find the
/// executable name when `lpApplicationName` is `NULL`.  A shell path that contains
/// spaces (e.g. `C:\Program Files\PowerShell\7\pwsh.exe`) must therefore be
/// wrapped in double-quotes so the entire path is treated as a single token.
///
/// This function is a no-op on non-Windows platforms.
#[cfg(target_os = "windows")]
fn quote_shell_program_if_needed(shell_path: &str) -> String {
    // Already fully quoted (starts and ends with a double-quote): leave unchanged.
    if shell_path.starts_with('"') && shell_path.ends_with('"') && shell_path.len() >= 2 {
        return shell_path.to_string();
    }
    // No quoting required when the path contains no spaces.
    if !shell_path.contains(' ') {
        return shell_path.to_string();
    }
    // Escape any embedded double-quotes inside the path, then wrap in outer quotes.
    // (Windows file names cannot legally contain '"', but we handle it defensively.)
    let escaped = shell_path.replace('"', "\\\"");
    format!("\"{}\"", escaped)
}

fn login_shell_args(shell_path: &str) -> Vec<String> {
    #[cfg(target_os = "windows")]
    {
        let _ = shell_path;
        Vec::new()
    }

    // On macOS, terminals conventionally launch login shells so that the user's
    // PATH and environment (set up in ~/.bash_profile, ~/.zprofile, etc.) are
    // available.  Pass both -i (interactive) and -l (login).
    #[cfg(target_os = "macos")]
    match Path::new(shell_path)
        .file_name()
        .and_then(|name| name.to_str())
    {
        Some("bash" | "zsh" | "fish") => vec!["-i".to_string(), "-l".to_string()],
        _ => Vec::new(),
    }

    // On Linux (and other non-macOS Unix), the user is already in a login
    // session, so sourcing all login scripts on every terminal open adds
    // unnecessary startup latency.  Launch an interactive non-login shell
    // instead, which is the convention used by alacritty and other Linux
    // terminal emulators.
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    match Path::new(shell_path)
        .file_name()
        .and_then(|name| name.to_str())
    {
        Some("bash" | "zsh" | "fish") => vec!["-i".to_string()],
        _ => Vec::new(),
    }
}

fn resolve_shell_path(configured_shell: Option<&str>) -> String {
    if let Some(shell) = configured_shell
        .map(str::trim)
        .filter(|shell| !shell.is_empty())
    {
        return shell.to_string();
    }

    if let Ok(shell) = env::var("SHELL")
        && !shell.trim().is_empty()
    {
        return shell;
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(comspec) = env::var("COMSPEC")
            && !comspec.trim().is_empty()
        {
            return comspec;
        }
        "C:\\Windows\\System32\\cmd.exe".to_string()
    }

    #[cfg(target_os = "macos")]
    {
        "/bin/zsh".to_string()
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        "/bin/bash".to_string()
    }
}

fn startup_shell(runtime_config: &TerminalRuntimeConfig, startup_command: Option<&str>) -> Shell {
    if let Some(command) = startup_command
        .map(str::trim)
        .filter(|command| !command.is_empty())
    {
        #[cfg(unix)]
        {
            return Shell::new(
                "/bin/sh".to_string(),
                vec!["-c".to_string(), command.to_string()],
            );
        }

        #[cfg(target_os = "windows")]
        {
            return Shell::new(
                "cmd.exe".to_string(),
                vec!["/C".to_string(), command.to_string()],
            );
        }
    }

    let shell_path = resolve_shell_path(runtime_config.shell.as_deref());

    #[cfg(target_os = "windows")]
    let shell_program = quote_shell_program_if_needed(&shell_path);
    #[cfg(not(target_os = "windows"))]
    let shell_program = shell_path.clone();

    Shell::new(shell_program, login_shell_args(&shell_path))
}

fn user_home_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        if let Ok(user_profile) = env::var("USERPROFILE")
            && !user_profile.trim().is_empty()
        {
            return Some(PathBuf::from(user_profile));
        }

        if let (Ok(home_drive), Ok(home_path)) = (env::var("HOMEDRIVE"), env::var("HOMEPATH"))
            && !home_drive.trim().is_empty()
            && !home_path.trim().is_empty()
        {
            return Some(PathBuf::from(format!("{home_drive}{home_path}")));
        }
    }

    if let Ok(home) = env::var("HOME")
        && !home.trim().is_empty()
    {
        return Some(PathBuf::from(home));
    }

    None
}

fn pty_env_overrides(
    shell_integration: Option<&TabTitleShellIntegration>,
    runtime_config: &TerminalRuntimeConfig,
) -> HashMap<String, String> {
    let mut env_overrides = HashMap::new();

    #[cfg(not(target_os = "windows"))]
    {
        if let Some(path) = normalized_path_env(env::var_os("PATH").as_deref()) {
            env_overrides.insert("PATH".to_string(), path);
        }
    }

    let term = runtime_config.term.trim();
    let term = if term.is_empty() { DEFAULT_TERM } else { term };
    env_overrides.insert("TERM".to_string(), term.to_string());

    if let Some(colorterm) = runtime_config
        .colorterm
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        env_overrides.insert("COLORTERM".to_string(), colorterm.to_string());
    }

    env_overrides.insert("TERM_PROGRAM".to_string(), "termy".to_string());

    // Locale overrides are intentionally Unix-only. POSIX shells use libc locale
    // (`LC_*`/`LANG`) for wcwidth/prompt width, while native Windows shells
    // (`cmd.exe`/PowerShell) do not use this locale contract.
    #[cfg(unix)]
    {
        apply_utf8_locale_overrides(&mut env_overrides);
    }

    let shell_integration_enabled = shell_integration.map(|cfg| cfg.enabled).unwrap_or(false);
    env_overrides.insert(
        "TERMY_SHELL_INTEGRATION".to_string(),
        if shell_integration_enabled { "1" } else { "0" }.to_string(),
    );

    if shell_integration_enabled {
        let prefix = shell_integration
            .and_then(|cfg| {
                let trimmed = cfg.explicit_prefix.trim();
                (!trimmed.is_empty()).then_some(trimmed)
            })
            .unwrap_or("termy:tab:");
        env_overrides.insert("TERMY_TAB_TITLE_PREFIX".to_string(), prefix.to_string());
    }

    env_overrides
}

#[cfg(unix)]
fn apply_utf8_locale_overrides(env_overrides: &mut HashMap<String, String>) {
    let lc_all = env::var("LC_ALL").ok();
    let lc_ctype = env::var("LC_CTYPE").ok();
    let lang = env::var("LANG").ok();
    let target_utf8_locale =
        preferred_utf8_locale(lc_all.as_deref(), lc_ctype.as_deref(), lang.as_deref());

    // zsh prompt width calculations rely on libc wcwidth + locale. If the shell
    // starts in C/POSIX/non-UTF-8 locale, multibyte prompt glyphs (e.g. U+276F)
    // can be counted by byte-length, drifting completion rendering.
    match utf8_locale_override_plan(lc_all.as_deref(), lc_ctype.as_deref(), lang.as_deref()) {
        Utf8LocaleOverridePlan::None => {}
        Utf8LocaleOverridePlan::LcCtypeOnly => {
            env_overrides.insert("LC_CTYPE".to_string(), target_utf8_locale);
        }
        Utf8LocaleOverridePlan::LcAllAndLcCtype => {
            env_overrides.insert("LC_ALL".to_string(), target_utf8_locale.clone());
            env_overrides.insert("LC_CTYPE".to_string(), target_utf8_locale);
        }
    }
}

pub fn resolve_working_directory_path(configured: Option<&str>) -> Option<std::path::PathBuf> {
    let configured = configured?.trim();
    if configured.is_empty() {
        return None;
    }

    let path = if configured == "~" {
        user_home_dir()?
    } else if let Some(relative) = configured
        .strip_prefix("~/")
        .or_else(|| configured.strip_prefix("~\\"))
    {
        user_home_dir()?.join(relative)
    } else {
        PathBuf::from(configured)
    };

    if path.is_dir() { Some(path) } else { None }
}

pub fn resolve_launch_working_directory(
    configured: Option<&str>,
    fallback: WorkingDirFallback,
) -> Option<PathBuf> {
    resolve_working_directory_path(configured)
        .or_else(|| default_working_directory_with_fallback(fallback))
}

pub fn normalize_working_directory_candidate(candidate: Option<&str>) -> Option<String> {
    let candidate = candidate?.trim();
    if candidate.is_empty() {
        return None;
    }

    Some(
        resolve_working_directory_path(Some(candidate))
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_else(|| candidate.to_string()),
    )
}

fn default_working_directory_with_fallback(fallback: WorkingDirFallback) -> Option<PathBuf> {
    if fallback == WorkingDirFallback::Home
        && let Some(home) = user_home_dir()
        && home.is_dir()
    {
        return Some(home);
    }

    env::current_dir().ok()
}

#[cfg(unix)]
fn pty_child_pid(pty: &tty::Pty) -> Option<u32> {
    Some(pty.child().id())
}

#[cfg(target_os = "windows")]
fn pty_child_pid(pty: &tty::Pty) -> Option<u32> {
    pty.child_watcher().pid().map(|pid| pid.get())
}

#[cfg(not(any(unix, target_os = "windows")))]
fn pty_child_pid(_pty: &tty::Pty) -> Option<u32> {
    None
}

/// Events sent from the terminal to the view
#[derive(Debug, Clone)]
pub enum TerminalEvent {
    /// Terminal content has changed, needs redraw
    Wakeup,
    /// Terminal title changed
    #[allow(dead_code)]
    Title(String),
    /// Terminal title reset
    ResetTitle,
    /// Bell character received
    Bell,
    /// Terminal exited
    Exit,
    /// OSC 52 clipboard store request
    ClipboardStore(String),

    // Shell integration events (OSC 133)
    /// OSC 133;A - Shell prompt start
    ShellPromptStart,
    /// OSC 133;B - Command input start
    ShellCommandStart,
    /// OSC 133;C - Command executing
    ShellCommandExecuting,
    /// OSC 133;D - Command finished with optional exit code
    ShellCommandFinished(Option<i32>),

    // Notification events
    /// OSC 777 - Desktop notification with title and body
    Notification { title: String, body: String },
    /// OSC 9 - Simple notification message
    Notify(String),

    // Progress indicator (OSC 9;4)
    /// Progress state change from OSC 9;4
    Progress(ProgressState),

    // Working directory (OSC 7)
    /// Working directory changed
    WorkingDirectory(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalDirtySpan {
    pub row: usize,
    pub left_col: usize,
    pub right_col: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalDamageSnapshot {
    Full,
    Partial(Vec<TerminalDirtySpan>),
}

fn normalized_dirty_span(
    damage: LineDamageBounds,
    rows: usize,
    cols: usize,
    display_offset: usize,
) -> Option<TerminalDirtySpan> {
    // Alacritty line damage is tracked in terminal-space line coordinates and can straddle
    // wide characters. Expand by one column on both sides so partial updates never split
    // a multi-cell glyph and leave stale spacer artifacts.
    if rows == 0 || cols == 0 || display_offset != 0 {
        return None;
    }
    if damage.line >= rows {
        return None;
    }
    let left_col = damage.left.saturating_sub(1).min(cols.saturating_sub(1));
    let right_col = damage.right.saturating_add(1).min(cols.saturating_sub(1));
    if left_col > right_col {
        return None;
    }
    Some(TerminalDirtySpan {
        row: damage.line,
        left_col,
        right_col,
    })
}

pub(crate) fn take_term_damage_snapshot<T: EventListener>(
    term: &mut Term<T>,
) -> TerminalDamageSnapshot {
    let rows = term.grid().screen_lines();
    let cols = term.grid().columns();
    let display_offset = term.grid().display_offset();
    let snapshot = match term.damage() {
        TermDamage::Full => TerminalDamageSnapshot::Full,
        TermDamage::Partial(damage_iter) => {
            let mut damage_iter = damage_iter.peekable();
            if display_offset != 0 {
                // While viewing history, partial damage coordinates are difficult to map
                // correctly across viewport-relative lines. Only force a full rebuild when
                // alacritty actually reports damaged lines, otherwise keep this as a no-op.
                if damage_iter.peek().is_some() {
                    TerminalDamageSnapshot::Full
                } else {
                    TerminalDamageSnapshot::Partial(Vec::new())
                }
            } else {
                let mut spans = Vec::new();
                for damage in damage_iter {
                    if let Some(span) = normalized_dirty_span(damage, rows, cols, display_offset) {
                        spans.push(span);
                    }
                }
                TerminalDamageSnapshot::Partial(spans)
            }
        }
    };
    term.reset_damage();
    snapshot
}

/// Event listener that forwards alacritty events to our channel
#[derive(Clone)]
pub struct JsonEventListener {
    events_tx: Sender<RuntimeEvent>,
    wake_tx: Option<Sender<()>>,
    replay_suppressed: Arc<AtomicBool>,
}

impl JsonEventListener {
    fn new(events_tx: Sender<RuntimeEvent>, wake_tx: Option<Sender<()>>) -> Self {
        Self {
            events_tx,
            wake_tx,
            replay_suppressed: Arc::new(AtomicBool::new(false)),
        }
    }

    fn set_replay_suppressed(&self, suppressed: bool) {
        self.replay_suppressed.store(suppressed, Ordering::Release);
    }

    fn send_terminal_event(&self, event: TerminalEvent) {
        let _ = self.events_tx.send(RuntimeEvent::Terminal(event));
        if let Some(wake_tx) = &self.wake_tx {
            let _ = wake_tx.try_send(());
        }
    }
}

impl EventListener for JsonEventListener {
    fn send_event(&self, event: AlacEvent) {
        if self.replay_suppressed.load(Ordering::Acquire) {
            return;
        }
        if matches!(event, AlacEvent::Wakeup) {
            increment_runtime_wakeup_count();
        }
        let _ = self.events_tx.send(RuntimeEvent::Alacritty(event));
        if let Some(wake_tx) = &self.wake_tx {
            // This channel only nudges the UI thread to drain terminal events promptly.
            let _ = wake_tx.try_send(());
        }
    }
}

const NATIVE_EVENT_LOOP_READ_BUFFER_SIZE: usize = 0x10_0000;
const NATIVE_EVENT_LOOP_MAX_LOCKED_READ: usize = u16::MAX as usize;
#[cfg(not(target_os = "windows"))]
const NATIVE_EVENT_LOOP_READ_WRITE_TOKEN: usize = 0;
#[cfg(not(target_os = "windows"))]
const NATIVE_EVENT_LOOP_CHILD_EVENT_TOKEN: usize = 1;
#[cfg(target_os = "windows")]
const NATIVE_EVENT_LOOP_READ_WRITE_TOKEN: usize = 2;
#[cfg(target_os = "windows")]
const NATIVE_EVENT_LOOP_CHILD_EVENT_TOKEN: usize = 1;

#[derive(Debug, Clone)]
enum RuntimeEvent {
    Alacritty(AlacEvent),
    Terminal(TerminalEvent),
}

#[derive(Debug)]
enum EventLoopMsg {
    Input(Cow<'static, [u8]>),
    Shutdown,
    Resize(WindowSize),
}

#[derive(Clone)]
struct EventLoopSender {
    sender: StdSender<EventLoopMsg>,
    poller: Arc<Poller>,
}

impl EventLoopSender {
    fn send(&self, msg: EventLoopMsg) -> io::Result<()> {
        self.sender
            .send(msg)
            .map_err(|error| io::Error::new(ErrorKind::BrokenPipe, error.to_string()))?;
        self.poller.notify()
    }
}

struct Writing {
    source: Cow<'static, [u8]>,
    written: usize,
}

impl Writing {
    fn new(source: Cow<'static, [u8]>) -> Self {
        Self { source, written: 0 }
    }

    fn advance(&mut self, count: usize) {
        self.written += count;
    }

    fn remaining_bytes(&self) -> &[u8] {
        &self.source[self.written..]
    }

    fn finished(&self) -> bool {
        self.written >= self.source.len()
    }
}

struct PeekableReceiver<T> {
    rx: StdReceiver<T>,
    peeked: Option<T>,
}

impl<T> PeekableReceiver<T> {
    fn new(rx: StdReceiver<T>) -> Self {
        Self { rx, peeked: None }
    }

    fn peek(&mut self) -> Option<&T> {
        if self.peeked.is_none() {
            self.peeked = self.rx.try_recv().ok();
        }

        self.peeked.as_ref()
    }

    fn recv(&mut self) -> Option<T> {
        if self.peeked.is_some() {
            self.peeked.take()
        } else {
            match self.rx.try_recv() {
                Err(TryRecvError::Disconnected) => panic!("event loop channel closed"),
                res => res.ok(),
            }
        }
    }
}

#[derive(Default)]
struct NativeEventLoopState {
    write_list: std::collections::VecDeque<Cow<'static, [u8]>>,
    writing: Option<Writing>,
    parser: ansi::Processor,
    osc_interceptor: OscInterceptor,
}

impl NativeEventLoopState {
    fn ensure_next(&mut self) {
        if self.writing.is_none() {
            self.goto_next();
        }
    }

    fn goto_next(&mut self) {
        self.writing = self.write_list.pop_front().map(Writing::new);
    }

    fn take_current(&mut self) -> Option<Writing> {
        self.writing.take()
    }

    fn needs_write(&self) -> bool {
        self.writing.is_some() || !self.write_list.is_empty()
    }

    fn set_current(&mut self, current: Option<Writing>) {
        self.writing = current;
    }
}

struct NativeEventLoop {
    poll: Arc<Poller>,
    pty: tty::Pty,
    rx: PeekableReceiver<EventLoopMsg>,
    tx: StdSender<EventLoopMsg>,
    terminal: Arc<FairMutex<Term<JsonEventListener>>>,
    event_proxy: JsonEventListener,
    drain_on_exit: bool,
}

impl NativeEventLoop {
    fn new(
        terminal: Arc<FairMutex<Term<JsonEventListener>>>,
        event_proxy: JsonEventListener,
        pty: tty::Pty,
        drain_on_exit: bool,
    ) -> io::Result<Self> {
        let (tx, rx) = mpsc::channel();
        Ok(Self {
            poll: Poller::new()?.into(),
            pty,
            rx: PeekableReceiver::new(rx),
            tx,
            terminal,
            event_proxy,
            drain_on_exit,
        })
    }

    fn channel(&self) -> EventLoopSender {
        EventLoopSender {
            sender: self.tx.clone(),
            poller: self.poll.clone(),
        }
    }

    fn drain_recv_channel(&mut self, state: &mut NativeEventLoopState) -> bool {
        while let Some(msg) = self.rx.recv() {
            match msg {
                EventLoopMsg::Input(input) => state.write_list.push_back(input),
                EventLoopMsg::Resize(window_size) => self.pty.on_resize(window_size),
                EventLoopMsg::Shutdown => return false,
            }
        }

        true
    }

    fn handle_osc_events(&self, osc_events: Vec<OscEvent>) {
        for osc_event in osc_events {
            self.event_proxy
                .send_terminal_event(terminal_event_from_osc(osc_event));
        }
    }

    fn pty_read(&mut self, state: &mut NativeEventLoopState, buf: &mut [u8]) -> io::Result<()> {
        let mut parsed = 0usize;
        let mut unprocessed = 0usize;

        let _terminal_lease = Some(self.terminal.lease());
        let mut terminal = None;

        loop {
            match self.pty.reader().read(&mut buf[unprocessed..]) {
                Ok(0) if unprocessed == 0 => break,
                Ok(got) => unprocessed += got,
                Err(err) => match err.kind() {
                    ErrorKind::Interrupted | ErrorKind::WouldBlock => {
                        if unprocessed == 0 {
                            break;
                        }
                    }
                    _ => return Err(err),
                },
            }

            let terminal = match &mut terminal {
                Some(terminal) => terminal,
                None => terminal.insert(match self.terminal.try_lock_unfair() {
                    None if unprocessed >= NATIVE_EVENT_LOOP_READ_BUFFER_SIZE => {
                        self.terminal.lock_unfair()
                    }
                    None => continue,
                    Some(terminal) => terminal,
                }),
            };

            let (filtered, osc_events) = state.osc_interceptor.process(&buf[..unprocessed]);
            self.handle_osc_events(osc_events);

            if !filtered.is_empty() {
                parsed += filtered.len();
                state.parser.advance(&mut **terminal, &filtered);
            }

            unprocessed = 0;
            if parsed >= NATIVE_EVENT_LOOP_MAX_LOCKED_READ {
                break;
            }
        }

        if state.parser.sync_bytes_count() < parsed && parsed > 0 {
            self.event_proxy.send_event(AlacEvent::Wakeup);
        }

        Ok(())
    }

    fn pty_write(&mut self, state: &mut NativeEventLoopState) -> io::Result<()> {
        state.ensure_next();

        'write_many: while let Some(mut current) = state.take_current() {
            'write_one: loop {
                match self.pty.writer().write(current.remaining_bytes()) {
                    Ok(0) => {
                        state.set_current(Some(current));
                        break 'write_many;
                    }
                    Ok(count) => {
                        current.advance(count);
                        if current.finished() {
                            state.goto_next();
                            break 'write_one;
                        }
                    }
                    Err(err) => {
                        state.set_current(Some(current));
                        match err.kind() {
                            ErrorKind::Interrupted | ErrorKind::WouldBlock => break 'write_many,
                            _ => return Err(err),
                        }
                    }
                }
            }
        }

        Ok(())
    }

    fn spawn(mut self) {
        let _ = thread::spawn_named("PTY reader", move || {
            let mut state = NativeEventLoopState::default();
            let mut buf = [0u8; NATIVE_EVENT_LOOP_READ_BUFFER_SIZE];
            let poll_mode = PollMode::Level;
            let mut interest = PollingEvent::readable(NATIVE_EVENT_LOOP_READ_WRITE_TOKEN);

            if unsafe { self.pty.register(&self.poll, interest, poll_mode) }.is_err() {
                return;
            }

            let mut events = Events::with_capacity(NonZeroUsize::new(1024).expect("non-zero"));

            'event_loop: loop {
                let timeout = self
                    .rx
                    .peek()
                    .is_none()
                    .then(|| state.parser.sync_timeout().sync_timeout())
                    .flatten()
                    .map(|deadline| deadline.saturating_duration_since(Instant::now()));

                events.clear();
                if self.poll.wait(&mut events, timeout).is_err() {
                    break 'event_loop;
                }

                if events.is_empty() && self.rx.peek().is_none() {
                    state.parser.stop_sync(&mut *self.terminal.lock());
                    self.event_proxy.send_event(AlacEvent::Wakeup);
                    continue;
                }

                if !self.drain_recv_channel(&mut state) {
                    break 'event_loop;
                }

                for event in events.iter() {
                    match event.key {
                        NATIVE_EVENT_LOOP_CHILD_EVENT_TOKEN => {
                            if let Some(tty::ChildEvent::Exited(_)) = self.pty.next_child_event() {
                                if self.drain_on_exit {
                                    let _ = self.pty_read(&mut state, &mut buf);
                                }
                                self.terminal.lock().exit();
                                self.event_proxy.send_event(AlacEvent::Wakeup);
                                break 'event_loop;
                            }
                        }
                        NATIVE_EVENT_LOOP_READ_WRITE_TOKEN => {
                            if event.is_interrupt() {
                                continue;
                            }

                            if event.readable {
                                if self.pty_read(&mut state, &mut buf).is_err() {
                                    break 'event_loop;
                                }
                            }

                            if event.writable && self.pty_write(&mut state).is_err() {
                                break 'event_loop;
                            }
                        }
                        _ => {}
                    }
                }

                let needs_write = state.needs_write();
                if needs_write != interest.writable {
                    interest.writable = needs_write;
                    if self
                        .pty
                        .reregister(&self.poll, interest, poll_mode)
                        .is_err()
                    {
                        break 'event_loop;
                    }
                }
            }

            let _ = self.pty.deregister(&self.poll);
        });
    }
}

/// Terminal dimensions in cells and pixels
#[derive(Debug, Clone, Copy)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
    pub cell_width: Pixels,
    pub cell_height: Pixels,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self {
            cols: 80,
            rows: 24,
            cell_width: px(9.0),
            cell_height: px(18.0),
        }
    }
}

impl From<TerminalSize> for WindowSize {
    fn from(size: TerminalSize) -> Self {
        // Extract the f32 value from Pixels
        let cell_width_f32: f32 = size.cell_width.into();
        let cell_height_f32: f32 = size.cell_height.into();
        WindowSize {
            num_cols: size.cols,
            num_lines: size.rows,
            cell_width: cell_width_f32 as u16,
            cell_height: cell_height_f32 as u16,
        }
    }
}

impl Dimensions for TerminalSize {
    fn total_lines(&self) -> usize {
        self.rows as usize
    }

    fn screen_lines(&self) -> usize {
        self.rows as usize
    }

    fn columns(&self) -> usize {
        self.cols as usize
    }

    fn last_column(&self) -> alacritty_terminal::index::Column {
        alacritty_terminal::index::Column(self.cols.saturating_sub(1) as usize)
    }

    fn bottommost_line(&self) -> alacritty_terminal::index::Line {
        alacritty_terminal::index::Line(i32::from(self.rows.saturating_sub(1)))
    }

    fn topmost_line(&self) -> alacritty_terminal::index::Line {
        alacritty_terminal::index::Line(0)
    }
}

/// The terminal state wrapper
pub struct Terminal {
    /// The alacritty terminal emulator
    term: Arc<FairMutex<Term<JsonEventListener>>>,
    /// Listener clone used to suppress side effects during replay hydration.
    listener: JsonEventListener,
    /// Parser used for buffer rehydration without writing to the PTY.
    parser: FairMutex<ansi::Processor>,
    /// Channel to send input to the PTY
    pty_tx: EventLoopSender,
    /// Channel to receive events from the native PTY loop
    events_rx: Receiver<RuntimeEvent>,
    /// Current terminal size
    size: TerminalSize,
    /// Colors returned to child processes that probe terminal palette state.
    query_colors: TerminalQueryColors,
    /// Shell process id backing this PTY.
    child_pid: Option<u32>,
}

impl Terminal {
    /// Create a new terminal with the given size
    pub fn new(
        size: TerminalSize,
        configured_working_dir: Option<&str>,
        event_wakeup_tx: Option<Sender<()>>,
        tab_title_shell_integration: Option<&TabTitleShellIntegration>,
        runtime_config: Option<&TerminalRuntimeConfig>,
        startup_command: Option<&str>,
    ) -> anyhow::Result<Self> {
        // Create event channels
        let (events_tx, events_rx) = unbounded();
        let runtime_config = runtime_config.cloned().unwrap_or_default();
        let shell = startup_shell(&runtime_config, startup_command);

        // Get working directory
        let working_directory = resolve_launch_working_directory(
            configured_working_dir,
            runtime_config.working_dir_fallback,
        );

        // Configure PTY
        let pty_options = PtyOptions {
            shell: Some(shell),
            working_directory,
            env: pty_env_overrides(tab_title_shell_integration, &runtime_config),
            drain_on_exit: true,
            #[cfg(target_os = "windows")]
            escape_args: true,
        };

        // Create terminal config with configurable scrollback history
        let term_config = runtime_config.term_options().term_config();

        // Create the terminal emulator
        let listener = JsonEventListener::new(events_tx.clone(), event_wakeup_tx);
        let term = Term::new(term_config, &size, listener.clone());
        let term = Arc::new(FairMutex::new(term));

        // Create PTY
        let window_id = 0;
        let pty = tty::new(&pty_options, size.into(), window_id)?;
        let child_pid = pty_child_pid(&pty);

        // Create and spawn the event loop
        let event_loop = NativeEventLoop::new(
            term.clone(),
            listener.clone(),
            pty,
            pty_options.drain_on_exit,
        )?;
        let pty_tx = event_loop.channel();
        event_loop.spawn();

        Ok(Self {
            term,
            listener: listener.clone(),
            parser: FairMutex::new(ansi::Processor::new()),
            pty_tx,
            events_rx,
            size,
            query_colors: runtime_config.query_colors,
            child_pid,
        })
    }

    pub fn child_pid(&self) -> Option<u32> {
        self.child_pid
    }

    /// Write bytes to the PTY (user input)
    pub fn write(&self, input: &[u8]) {
        let _ = self.pty_tx.send(EventLoopMsg::Input(input.to_vec().into()));
    }

    /// Rehydrate saved terminal output into the in-memory grid without sending input to the PTY.
    pub fn hydrate_output(&self, bytes: &[u8]) {
        if bytes.is_empty() {
            return;
        }

        self.listener.set_replay_suppressed(true);
        let mut parser = self.parser.lock();
        let mut term = self.term.lock();
        parser.advance(&mut *term, bytes);
        self.listener.set_replay_suppressed(false);
    }

    /// Write a string to the PTY
    #[allow(dead_code)]
    pub fn write_str(&self, input: &str) {
        self.write(input.as_bytes());
    }

    /// Resize the terminal
    pub fn resize(&mut self, new_size: TerminalSize) {
        self.size = new_size;
        let _ = self.pty_tx.send(EventLoopMsg::Resize(new_size.into()));
        self.term.lock().resize(new_size);
    }

    /// Re-send the current size to the PTY without touching the term grid.
    /// This delivers SIGWINCH to the child process, nudging TUI applications
    /// (e.g. lazygit) to refresh their display after an alternate-screen
    /// transition even though the actual dimensions have not changed.
    pub fn nudge_resize(&self) {
        let _ = self.pty_tx.send(EventLoopMsg::Resize(self.size.into()));
    }

    /// Get the current terminal size
    pub fn size(&self) -> TerminalSize {
        self.size
    }

    /// Drain pending Alacritty events, writing reply bytes back to the PTY when required.
    /// Returns the collected events and whether more events remain (batch limit hit).
    pub fn drain_events(&self, host: &mut impl TerminalReplyHost) -> (Vec<TerminalEvent>, bool) {
        drain_runtime_events(
            &self.events_rx,
            self.size,
            &self.term,
            self.query_colors,
            host,
            |response| self.write(response),
        )
    }

    pub fn set_query_colors(&mut self, query_colors: TerminalQueryColors) {
        self.query_colors = query_colors;
    }

    /// Access the terminal for reading cell content
    pub fn with_term<R>(&self, f: impl FnOnce(&Term<JsonEventListener>) -> R) -> R {
        let term = self.term.lock();
        f(&term)
    }

    /// Access the terminal for in-place mutation.
    fn with_term_mut<R>(&self, f: impl FnOnce(&mut Term<JsonEventListener>) -> R) -> R {
        let mut term = self.term.lock();
        f(&mut term)
    }

    /// Consume and normalize terminal damage spans for incremental rendering.
    pub fn take_damage_snapshot(&self) -> TerminalDamageSnapshot {
        self.with_term_mut(take_term_damage_snapshot)
    }

    /// Scroll the displayed viewport through scrollback history.
    /// Positive deltas move up into history, negative deltas move down toward live output.
    pub fn scroll_display(&self, delta_lines: i32) -> bool {
        if delta_lines == 0 {
            return false;
        }

        let mut term = self.term.lock();
        let old_offset = term.grid().display_offset();
        term.scroll_display(Scroll::Delta(delta_lines));
        term.grid().display_offset() != old_offset
    }

    /// Scroll the displayed viewport to the bottom (live output) atomically.
    /// Returns true if the scroll position changed.
    pub fn scroll_to_bottom(&self) -> bool {
        let mut term = self.term.lock();
        let old_offset = term.grid().display_offset();
        if old_offset == 0 {
            return false;
        }
        term.scroll_display(Scroll::Bottom);
        true
    }

    /// Return `(display_offset, history_size)` for viewport scrollbar rendering.
    pub fn scroll_state(&self) -> (usize, usize) {
        let term = self.term.lock();
        let grid = term.grid();
        (grid.display_offset(), grid.history_size())
    }

    /// Get the cursor state the terminal currently intends to render.
    pub fn cursor_state(&self) -> Option<TerminalCursorState> {
        let term = self.term.lock();
        cursor_state_from_term(&term)
    }

    /// Returns the cursor position regardless of visibility (for IME positioning).
    pub fn cursor_position(&self) -> (usize, usize) {
        let term = self.term.lock();
        cursor_position_from_term(&term)
    }

    /// Check if there are pending events
    #[allow(dead_code)]
    pub fn has_pending_events(&self) -> bool {
        !self.events_rx.is_empty()
    }

    /// Sync live term options derived from the current runtime configuration.
    pub fn set_term_options(&self, options: TerminalOptions) {
        self.with_term_mut(|term| term.set_options(options.term_config()));
    }

    /// Check if bracketed paste mode is enabled
    pub fn bracketed_paste_mode(&self) -> bool {
        let term = self.term.lock();
        term.mode().contains(TermMode::BRACKETED_PASTE)
    }

    /// Return current xterm mouse-reporting mode bits.
    pub fn mouse_mode(&self) -> TerminalMouseMode {
        let term = self.term.lock();
        termmode_to_terminal_mouse_mode(*term.mode())
    }

    pub fn keyboard_mode(&self) -> TerminalKeyboardMode {
        let term = self.term.lock();
        TerminalKeyboardMode::from_term_mode(*term.mode())
    }

    /// Check if the terminal is currently in alternate screen mode
    pub fn alternate_screen_mode(&self) -> bool {
        let term = self.term.lock();
        term.mode().contains(TermMode::ALT_SCREEN)
    }
}

/// Maximum number of alacritty events to drain in a single frame. Prevents
/// massive output (e.g. `cat huge_file`) from blocking the render thread.
const EVENT_DRAIN_BATCH_LIMIT: usize = 2048;

/// Drain pending events, returning the collected terminal events and whether
/// the batch limit was hit (indicating more events remain).
fn drain_runtime_events<T: EventListener>(
    events_rx: &Receiver<RuntimeEvent>,
    size: TerminalSize,
    term: &FairMutex<Term<T>>,
    query_colors: TerminalQueryColors,
    host: &mut impl TerminalReplyHost,
    mut write_reply: impl FnMut(&[u8]),
) -> (Vec<TerminalEvent>, bool) {
    let fallback_live_colors = alacritty_terminal::term::color::Colors::default();
    let mut events = Vec::new();
    let mut drained = 0usize;

    while let Ok(runtime_event) = events_rx.try_recv() {
        match runtime_event {
            RuntimeEvent::Alacritty(event) => {
                let response = match &event {
                    AlacEvent::ColorRequest(_, _) => {
                        let term = term.lock();
                        reply_bytes_for_event(&event, size, term.colors(), query_colors, host)
                    }
                    _ => reply_bytes_for_event(
                        &event,
                        size,
                        &fallback_live_colors,
                        query_colors,
                        host,
                    ),
                };

                if let Some(response) = response {
                    write_reply(&response);
                }

                if let Some(event) = terminal_event_from_alacritty(event) {
                    events.push(event);
                }
            }
            RuntimeEvent::Terminal(event) => events.push(event),
        }

        drained += 1;
        if drained >= EVENT_DRAIN_BATCH_LIMIT {
            return (events, true);
        }
    }

    (events, false)
}

fn terminal_event_from_alacritty(event: AlacEvent) -> Option<TerminalEvent> {
    match event {
        AlacEvent::Wakeup => Some(TerminalEvent::Wakeup),
        AlacEvent::Title(title) => Some(TerminalEvent::Title(title)),
        AlacEvent::ResetTitle => Some(TerminalEvent::ResetTitle),
        AlacEvent::Bell => Some(TerminalEvent::Bell),
        AlacEvent::Exit => Some(TerminalEvent::Exit),
        AlacEvent::ClipboardStore(_, text) => Some(TerminalEvent::ClipboardStore(text)),
        _ => None,
    }
}

fn terminal_event_from_osc(event: OscEvent) -> TerminalEvent {
    match event {
        OscEvent::WorkingDirectory(path) => TerminalEvent::WorkingDirectory(path),
        OscEvent::Notify(message) => TerminalEvent::Notify(message),
        OscEvent::Progress(state) => TerminalEvent::Progress(state),
        OscEvent::ShellPromptStart => TerminalEvent::ShellPromptStart,
        OscEvent::ShellCommandStart => TerminalEvent::ShellCommandStart,
        OscEvent::ShellCommandExecuting => TerminalEvent::ShellCommandExecuting,
        OscEvent::ShellCommandFinished(code) => TerminalEvent::ShellCommandFinished(code),
        OscEvent::Notification { title, body } => TerminalEvent::Notification { title, body },
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        // Ensure the PTY event loop exits so PTY drop can terminate/reap the child process.
        let _ = self.pty_tx.send(EventLoopMsg::Shutdown);
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "windows")]
    use super::quote_shell_program_if_needed;
    use super::{
        DEFAULT_TERM, RuntimeEvent, TerminalCursorState, TerminalDamageSnapshot, TerminalEvent,
        TerminalRuntimeConfig, TerminalSize, WorkingDirFallback, cursor_position_from_term,
        cursor_state_from_term, drain_runtime_events, normalize_working_directory_candidate,
        pty_env_overrides, resolve_launch_working_directory, resolve_shell_path,
        take_term_damage_snapshot, terminal_event_from_osc, termmode_to_terminal_mouse_mode,
        user_home_dir,
    };
    use crate::grid::TerminalCursorStyle;
    use crate::keyboard::{TerminalKeyEventKind, TerminalKeyboardMode, keystroke_to_input};
    use crate::protocol::{TerminalClipboardTarget, TerminalQueryColors, TerminalReplyHost};
    use alacritty_terminal::{
        event::VoidListener,
        grid::{Dimensions, Scroll},
        sync::FairMutex,
        term::{ClipboardType, Config as TermConfig, LineDamageBounds, Term, TermMode},
        vte::ansi::{self, CursorShape, NamedColor},
    };
    use flume::unbounded;
    use gpui::{Keystroke, Modifiers, px};
    use std::sync::Arc;

    fn test_terminal_size() -> TerminalSize {
        TerminalSize {
            cols: 32,
            rows: 4,
            cell_width: px(9.0),
            cell_height: px(18.0),
        }
    }

    fn cursor_after_bytes(input: &[u8]) -> (usize, i32) {
        let size = test_terminal_size();
        let mut term: Term<VoidListener> = Term::new(TermConfig::default(), &size, VoidListener);
        let mut parser: ansi::Processor = ansi::Processor::new();
        parser.advance(&mut term, input);
        let point = term.grid().cursor.point;
        (point.column.0, point.line.0)
    }

    fn term_after_bytes(input: &[u8]) -> Term<VoidListener> {
        let size = test_terminal_size();
        let mut term: Term<VoidListener> = Term::new(TermConfig::default(), &size, VoidListener);
        let mut parser: ansi::Processor = ansi::Processor::new();
        parser.advance(&mut term, input);
        term
    }

    fn cursor_state_after_bytes(
        input: &[u8],
        runtime_config: TerminalRuntimeConfig,
    ) -> Option<TerminalCursorState> {
        let size = test_terminal_size();
        let mut term: Term<VoidListener> = Term::new(
            runtime_config.term_options().term_config(),
            &size,
            VoidListener,
        );
        let mut parser: ansi::Processor = ansi::Processor::new();
        parser.advance(&mut term, input);
        cursor_state_from_term(&term)
    }

    fn cursor_position_after_bytes(input: &[u8]) -> (usize, usize) {
        let size = test_terminal_size();
        let mut term: Term<VoidListener> = Term::new(TermConfig::default(), &size, VoidListener);
        let mut parser: ansi::Processor = ansi::Processor::new();
        parser.advance(&mut term, input);
        cursor_position_from_term(&term)
    }

    fn mouse_mode_after_bytes(input: &[u8]) -> crate::mouse_protocol::TerminalMouseMode {
        let size = test_terminal_size();
        let mut term: Term<VoidListener> = Term::new(TermConfig::default(), &size, VoidListener);
        let mut parser: ansi::Processor = ansi::Processor::new();
        parser.advance(&mut term, input);
        termmode_to_terminal_mouse_mode(*term.mode())
    }

    fn keystroke(key: &str, modifiers: Modifiers) -> Keystroke {
        Keystroke {
            modifiers,
            key: key.to_string(),
            key_char: None,
        }
    }

    fn press_mode() -> TerminalKeyboardMode {
        TerminalKeyboardMode::default()
    }

    fn keyboard_mode(flags: TermMode) -> TerminalKeyboardMode {
        TerminalKeyboardMode::from_term_mode(flags)
    }

    #[derive(Default)]
    struct RecordingReplyHost {
        clipboard_text: Option<String>,
        requested_targets: Vec<TerminalClipboardTarget>,
    }

    impl TerminalReplyHost for RecordingReplyHost {
        fn load_clipboard(&mut self, target: TerminalClipboardTarget) -> Option<String> {
            self.requested_targets.push(target);
            self.clipboard_text.clone()
        }
    }

    #[test]
    fn terminal_size_dimensions_saturate_bottommost_line_for_zero_rows() {
        let size = TerminalSize {
            cols: 0,
            rows: 0,
            cell_width: px(9.0),
            cell_height: px(18.0),
        };

        assert_eq!(size.last_column().0, 0);
        assert_eq!(size.bottommost_line().0, 0);
    }

    #[test]
    fn normalize_working_directory_candidate_preserves_relative_paths() {
        assert_eq!(
            normalize_working_directory_candidate(Some(" crates/cli ")).as_deref(),
            Some("crates/cli")
        );
    }

    #[test]
    fn resolve_launch_working_directory_falls_back_when_configured_path_is_invalid() {
        let fallback = std::env::current_dir().expect("current dir");
        let resolved = resolve_launch_working_directory(
            Some("/definitely/not/a/real/termy/path"),
            WorkingDirFallback::Process,
        )
        .expect("fallback path");
        assert_eq!(resolved, fallback);
    }

    #[test]
    fn normalize_working_directory_candidate_expands_home_directory() {
        let expected = user_home_dir()
            .expect("home dir")
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            normalize_working_directory_candidate(Some("~")).as_deref(),
            Some(expected.as_str())
        );
    }

    #[test]
    fn drain_runtime_events_replays_replies_and_collects_runtime_events() {
        let (events_tx, events_rx) = unbounded();
        events_tx
            .send(RuntimeEvent::Alacritty(
                alacritty_terminal::event::Event::PtyWrite("\x1b[?6c".to_string()),
            ))
            .unwrap();
        events_tx
            .send(RuntimeEvent::Alacritty(
                alacritty_terminal::event::Event::TextAreaSizeRequest(Arc::new(|window_size| {
                    format!("size:{}x{}", window_size.num_cols, window_size.num_lines)
                })),
            ))
            .unwrap();
        events_tx
            .send(RuntimeEvent::Alacritty(
                alacritty_terminal::event::Event::ClipboardLoad(
                    ClipboardType::Selection,
                    Arc::new(|text| format!("clip:{text}")),
                ),
            ))
            .unwrap();
        events_tx
            .send(RuntimeEvent::Alacritty(
                alacritty_terminal::event::Event::ColorRequest(
                    NamedColor::Foreground as usize,
                    Arc::new(|color| format!("fg:{:02x}{:02x}{:02x}", color.r, color.g, color.b)),
                ),
            ))
            .unwrap();
        events_tx
            .send(RuntimeEvent::Alacritty(
                alacritty_terminal::event::Event::Wakeup,
            ))
            .unwrap();
        events_tx
            .send(RuntimeEvent::Alacritty(
                alacritty_terminal::event::Event::Title("shell title".to_string()),
            ))
            .unwrap();
        events_tx
            .send(RuntimeEvent::Alacritty(
                alacritty_terminal::event::Event::ClipboardStore(
                    ClipboardType::Clipboard,
                    "stored text".to_string(),
                ),
            ))
            .unwrap();
        events_tx
            .send(RuntimeEvent::Alacritty(
                alacritty_terminal::event::Event::Exit,
            ))
            .unwrap();
        drop(events_tx);

        let term = FairMutex::new(term_after_bytes(b"\x1b]10;#123456\x07"));
        let mut reply_host = RecordingReplyHost {
            clipboard_text: Some("payload".to_string()),
            requested_targets: Vec::new(),
        };
        let mut replies = Vec::new();

        let (events, _has_more) = drain_runtime_events(
            &events_rx,
            test_terminal_size(),
            &term,
            TerminalQueryColors::default(),
            &mut reply_host,
            |response| replies.push(String::from_utf8(response.to_vec()).unwrap()),
        );

        assert_eq!(
            replies,
            vec![
                "\x1b[?6c".to_string(),
                "size:32x4".to_string(),
                "clip:payload".to_string(),
                "fg:123456".to_string(),
            ]
        );
        assert_eq!(
            reply_host.requested_targets,
            vec![TerminalClipboardTarget::Selection]
        );
        assert!(matches!(
            events.as_slice(),
            [
                TerminalEvent::Wakeup,
                TerminalEvent::Title(title),
                TerminalEvent::ClipboardStore(text),
                TerminalEvent::Exit,
            ] if title == "shell title" && text == "stored text"
        ));
    }

    #[test]
    fn drain_runtime_events_includes_custom_osc_events() {
        let (events_tx, events_rx) = unbounded();
        events_tx
            .send(RuntimeEvent::Terminal(terminal_event_from_osc(
                crate::osc_intercept::OscEvent::Notification {
                    title: "Build Complete".to_string(),
                    body: "Project built successfully.".to_string(),
                },
            )))
            .unwrap();
        events_tx
            .send(RuntimeEvent::Terminal(terminal_event_from_osc(
                crate::osc_intercept::OscEvent::Notify("Heads up".to_string()),
            )))
            .unwrap();
        drop(events_tx);

        let term = FairMutex::new(term_after_bytes(b""));
        let mut reply_host = RecordingReplyHost::default();

        let (events, has_more) = drain_runtime_events(
            &events_rx,
            test_terminal_size(),
            &term,
            TerminalQueryColors::default(),
            &mut reply_host,
            |_| {},
        );

        assert!(!has_more);
        assert_eq!(events.len(), 2);
        assert!(matches!(
            &events[0],
            TerminalEvent::Notification { title, body }
                if title == "Build Complete" && body == "Project built successfully."
        ));
        assert!(matches!(&events[1], TerminalEvent::Notify(message) if message == "Heads up"));
    }

    #[test]
    fn mouse_mode_detects_click_reporting() {
        let mode = mouse_mode_after_bytes(b"\x1b[?1000h");
        assert!(mode.enabled);
        assert!(mode.report_click);
        assert!(!mode.report_drag);
        assert!(!mode.report_motion);
    }

    #[test]
    fn mouse_mode_detects_drag_reporting() {
        let mode = mouse_mode_after_bytes(b"\x1b[?1002h");
        assert!(mode.enabled);
        assert!(mode.report_drag);
        assert!(!mode.report_motion);
    }

    #[test]
    fn mouse_mode_detects_motion_reporting() {
        let mode = mouse_mode_after_bytes(b"\x1b[?1003h");
        assert!(mode.enabled);
        assert!(mode.report_motion);
    }

    #[test]
    fn mouse_mode_detects_sgr_encoding() {
        let mode = mouse_mode_after_bytes(b"\x1b[?1006h");
        assert!(mode.sgr_encoding);
    }

    #[test]
    fn mouse_mode_detects_utf8_reporting() {
        let mode = mouse_mode_after_bytes(b"\x1b[?1005h");
        assert!(mode.utf8_encoding);
    }

    #[test]
    fn take_term_damage_snapshot_is_full_for_new_term() {
        let size = TerminalSize {
            cols: 12,
            rows: 4,
            cell_width: px(9.0),
            cell_height: px(18.0),
        };
        let mut term: Term<VoidListener> = Term::new(TermConfig::default(), &size, VoidListener);
        assert!(matches!(
            take_term_damage_snapshot(&mut term),
            TerminalDamageSnapshot::Full
        ));
    }

    #[test]
    fn take_term_damage_snapshot_resets_damage_after_read() {
        let size = TerminalSize {
            cols: 12,
            rows: 4,
            cell_width: px(9.0),
            cell_height: px(18.0),
        };
        let mut term: Term<VoidListener> = Term::new(TermConfig::default(), &size, VoidListener);
        let _ = take_term_damage_snapshot(&mut term);
        let second = take_term_damage_snapshot(&mut term);
        let third = take_term_damage_snapshot(&mut term);
        assert!(matches!(second, TerminalDamageSnapshot::Partial(_)));
        assert_eq!(second, third);
    }

    #[test]
    fn take_term_damage_snapshot_returns_partial_spans_for_output() {
        let size = TerminalSize {
            cols: 12,
            rows: 4,
            cell_width: px(9.0),
            cell_height: px(18.0),
        };
        let mut term: Term<VoidListener> = Term::new(TermConfig::default(), &size, VoidListener);
        let _ = take_term_damage_snapshot(&mut term);
        let mut parser: ansi::Processor = ansi::Processor::new();
        parser.advance(&mut term, b"abc");
        assert!(matches!(
            take_term_damage_snapshot(&mut term),
            TerminalDamageSnapshot::Partial(spans) if !spans.is_empty()
        ));
    }

    #[test]
    fn take_term_damage_snapshot_while_scrolled_returns_empty_partial_without_damage() {
        let size = TerminalSize {
            cols: 12,
            rows: 4,
            cell_width: px(9.0),
            cell_height: px(18.0),
        };
        let mut term: Term<VoidListener> = Term::new(TermConfig::default(), &size, VoidListener);
        let _ = take_term_damage_snapshot(&mut term);

        let mut parser: ansi::Processor = ansi::Processor::new();
        parser.advance(&mut term, b"1\n2\n3\n4\n5\n6\n");
        let _ = take_term_damage_snapshot(&mut term);

        term.scroll_display(Scroll::Delta(1));
        assert!(term.grid().display_offset() > 0);

        assert!(matches!(
            take_term_damage_snapshot(&mut term),
            TerminalDamageSnapshot::Full
        ));
        assert_eq!(
            take_term_damage_snapshot(&mut term),
            TerminalDamageSnapshot::Partial(Vec::new())
        );
    }

    #[test]
    fn take_term_damage_snapshot_while_scrolled_returns_full_for_visible_damage() {
        let size = TerminalSize {
            cols: 12,
            rows: 4,
            cell_width: px(9.0),
            cell_height: px(18.0),
        };
        let mut term: Term<VoidListener> = Term::new(TermConfig::default(), &size, VoidListener);
        let _ = take_term_damage_snapshot(&mut term);

        let mut parser: ansi::Processor = ansi::Processor::new();
        parser.advance(&mut term, b"1\n2\n3\n4\n5\n6\n");
        let _ = take_term_damage_snapshot(&mut term);

        term.scroll_display(Scroll::Delta(1));
        let _ = take_term_damage_snapshot(&mut term);
        let _ = take_term_damage_snapshot(&mut term);

        ansi::Handler::goto(&mut term, 0, 0);
        assert!(matches!(
            take_term_damage_snapshot(&mut term),
            TerminalDamageSnapshot::Full
        ));
    }

    #[test]
    fn normalized_dirty_span_expands_and_clamps_column_bounds() {
        let span = super::normalized_dirty_span(LineDamageBounds::new(1, 1, 2), 4, 4, 0)
            .expect("dirty span should normalize");
        assert_eq!(span.row, 1);
        assert_eq!(span.left_col, 0);
        assert_eq!(span.right_col, 3);

        let span = super::normalized_dirty_span(LineDamageBounds::new(0, 0, 0), 4, 4, 0)
            .expect("left edge should clamp");
        assert_eq!(span.left_col, 0);
        assert_eq!(span.right_col, 1);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_secondary_shortcuts_map_to_line_editing_sequences() {
        let secondary = Modifiers {
            platform: true,
            ..Default::default()
        };

        assert_eq!(
            keystroke_to_input(
                &keystroke("left", secondary),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x01".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("home", secondary),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x01".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("right", secondary),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x05".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("end", secondary),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x05".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("backspace", secondary),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x15".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("delete", secondary),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x0b".to_vec())
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_alt_shortcuts_map_to_word_editing_sequences() {
        let alt = Modifiers {
            alt: true,
            ..Default::default()
        };

        assert_eq!(
            keystroke_to_input(
                &keystroke("left", alt),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x1bb".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("right", alt),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x1bf".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("backspace", alt),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x1b\x7f".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("delete", alt),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x1bd".to_vec())
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn non_macos_secondary_shortcuts_map_to_native_word_sequences() {
        let secondary = Modifiers {
            control: true,
            ..Default::default()
        };

        assert_eq!(
            keystroke_to_input(
                &keystroke("left", secondary),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x1bb".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("right", secondary),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x1bf".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("backspace", secondary),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x17".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("delete", secondary),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x1bd".to_vec())
        );
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn non_macos_secondary_shortcuts_do_not_remap_in_alternate_screen() {
        let secondary = Modifiers {
            control: true,
            ..Default::default()
        };

        assert_eq!(
            keystroke_to_input(
                &keystroke("left", secondary),
                TerminalKeyEventKind::Press,
                press_mode(),
                false,
            ),
            Some(b"\x1b[D".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("right", secondary),
                TerminalKeyEventKind::Press,
                press_mode(),
                false,
            ),
            Some(b"\x1b[C".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("backspace", secondary),
                TerminalKeyEventKind::Press,
                press_mode(),
                false,
            ),
            Some(vec![0x7f])
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("delete", secondary),
                TerminalKeyEventKind::Press,
                press_mode(),
                false,
            ),
            Some(b"\x1b[3~".to_vec())
        );
    }

    #[test]
    fn plain_special_key_sequences_remain_unchanged() {
        let none = Modifiers::default();

        assert_eq!(
            keystroke_to_input(
                &keystroke("backspace", none),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(vec![0x7f])
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("delete", none),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x1b[3~".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("left", none),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x1b[D".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("right", none),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x1b[C".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("home", none),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x1b[H".to_vec())
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("end", none),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(b"\x1b[F".to_vec())
        );
    }

    #[test]
    fn control_letter_mappings_remain_unchanged() {
        let control = Modifiers {
            control: true,
            ..Default::default()
        };

        assert_eq!(
            keystroke_to_input(
                &keystroke("a", control),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(vec![0x01])
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("c", control),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(vec![0x03])
        );
        assert_eq!(
            keystroke_to_input(
                &keystroke("z", control),
                TerminalKeyEventKind::Press,
                press_mode(),
                true,
            ),
            Some(vec![0x1a])
        );
    }

    #[test]
    fn term_options_enable_kitty_keyboard_negotiation() {
        assert!(
            TerminalRuntimeConfig::default()
                .term_options()
                .term_config()
                .kitty_keyboard
        );
    }

    #[test]
    fn keyboard_mode_detects_report_all_and_event_types() {
        let mode = keyboard_mode(TermMode::REPORT_ALL_KEYS_AS_ESC | TermMode::REPORT_EVENT_TYPES);
        assert!(mode.report_all_keys_as_esc());
        assert!(mode.report_event_types());
        assert!(mode.enhanced_reporting_active());
    }

    #[test]
    fn keyboard_mode_augment_only_flags_do_not_activate_enhanced_reporting() {
        let mode =
            keyboard_mode(TermMode::REPORT_ALTERNATE_KEYS | TermMode::REPORT_ASSOCIATED_TEXT);
        assert!(mode.report_alternate_keys());
        assert!(mode.report_associated_text());
        assert!(!mode.enhanced_reporting_active());
    }

    #[test]
    fn env_overrides_set_term_by_default() {
        let env = pty_env_overrides(None, &TerminalRuntimeConfig::default());
        assert_eq!(env.get("TERM").map(String::as_str), Some(DEFAULT_TERM));
    }

    #[test]
    fn env_overrides_allow_disabling_colorterm() {
        let config = TerminalRuntimeConfig {
            colorterm: None,
            ..TerminalRuntimeConfig::default()
        };
        let env = pty_env_overrides(None, &config);
        assert!(!env.contains_key("COLORTERM"));
    }

    #[test]
    fn explicit_shell_path_wins() {
        assert_eq!(resolve_shell_path(Some("/bin/custom")), "/bin/custom");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn shell_program_with_spaces_is_quoted() {
        let path = r"C:\Program Files\PowerShell\7\pwsh.exe";
        let quoted = quote_shell_program_if_needed(path);
        assert_eq!(quoted, r#""C:\Program Files\PowerShell\7\pwsh.exe""#);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn shell_program_without_spaces_is_unchanged() {
        let path = r"C:\Windows\System32\cmd.exe";
        assert_eq!(quote_shell_program_if_needed(path), path);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn already_quoted_shell_program_is_not_double_quoted() {
        let path = r#""C:\Program Files\PowerShell\7\pwsh.exe""#;
        assert_eq!(quote_shell_program_if_needed(path), path);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn shell_program_with_embedded_quotes_is_escaped() {
        // Defensively handle a path that (illegally on Windows) contains a
        // double-quote character alongside spaces.
        let path = "C:\\weird \\path\"\\pwsh.exe";
        let quoted = quote_shell_program_if_needed(path);
        assert_eq!(quoted, r#""C:\weird \path\"\pwsh.exe""#);
    }

    #[test]
    fn core_cursor_advance_matches_for_ascii_and_starship_glyph() {
        let ascii = cursor_after_bytes(b"> ");
        let starship = cursor_after_bytes("❯ ".as_bytes());
        assert_eq!(ascii, starship);
    }

    #[test]
    fn core_cursor_advance_ignores_ansi_sequences_for_ascii_and_starship_glyph() {
        let ascii = cursor_after_bytes(b"\x1b[1;32m>\x1b[0m ");
        let starship = cursor_after_bytes("\x1b[1;32m❯\x1b[0m ".as_bytes());
        assert_eq!(ascii, starship);
    }

    #[test]
    fn core_cursor_advance_matches_after_osc_title_with_bel_terminator() {
        let ascii = cursor_after_bytes(b"\x1b]2;termy:tab:prompt:/tmp\x07> ");
        let starship = cursor_after_bytes("\x1b]2;termy:tab:prompt:/tmp\x07❯ ".as_bytes());
        assert_eq!(ascii, starship);
    }

    #[test]
    fn core_cursor_advance_matches_after_osc_title_with_st_terminator() {
        let ascii = cursor_after_bytes(b"\x1b]2;termy:tab:prompt:/tmp\x1b\\> ");
        let starship = cursor_after_bytes("\x1b]2;termy:tab:prompt:/tmp\x1b\\❯ ".as_bytes());
        assert_eq!(ascii, starship);
    }

    #[test]
    fn cursor_state_hides_and_restores_with_terminal_visibility_sequences() {
        let hidden = cursor_state_after_bytes(b"prompt\x1b[?25l", TerminalRuntimeConfig::default());
        assert_eq!(hidden, None);

        let restored = cursor_state_after_bytes(
            b"prompt\x1b[?25l\x1b[?25h",
            TerminalRuntimeConfig::default(),
        );
        assert_eq!(
            restored,
            Some(TerminalCursorState {
                col: 6,
                row: 0,
                style: TerminalCursorStyle::Block,
            })
        );
    }

    #[test]
    fn cursor_position_remains_available_when_terminal_hides_cursor() {
        assert_eq!(cursor_position_after_bytes(b"prompt\x1b[?25l"), (6, 0));
    }

    #[test]
    fn cursor_state_maps_terminal_requested_shapes_to_supported_renderer_styles() {
        let block = cursor_state_after_bytes(
            b"\x1b[2 q",
            TerminalRuntimeConfig {
                default_cursor_style: TerminalCursorStyle::Line,
                ..TerminalRuntimeConfig::default()
            },
        );
        assert_eq!(
            block,
            Some(TerminalCursorState {
                col: 0,
                row: 0,
                style: TerminalCursorStyle::Block,
            })
        );

        let underline = cursor_state_after_bytes(b"\x1b[4 q", TerminalRuntimeConfig::default());
        assert_eq!(
            underline,
            Some(TerminalCursorState {
                col: 0,
                row: 0,
                style: TerminalCursorStyle::Line,
            })
        );

        let beam = cursor_state_after_bytes(b"\x1b[6 q", TerminalRuntimeConfig::default());
        assert_eq!(
            beam,
            Some(TerminalCursorState {
                col: 0,
                row: 0,
                style: TerminalCursorStyle::Line,
            })
        );
    }

    #[test]
    fn applying_runtime_options_preserves_default_cursor_style_when_scrollback_changes() {
        let size = test_terminal_size();
        let initial = TerminalRuntimeConfig {
            scrollback_history: 256,
            default_cursor_style: TerminalCursorStyle::Line,
            ..TerminalRuntimeConfig::default()
        };
        let mut term: Term<VoidListener> =
            Term::new(initial.term_options().term_config(), &size, VoidListener);

        let updated = TerminalRuntimeConfig {
            scrollback_history: 8,
            ..initial.clone()
        };
        term.set_options(updated.term_options().term_config());
        let mut parser: ansi::Processor = ansi::Processor::new();
        let output = (0..80)
            .map(|index| format!("line-{index}\r\n"))
            .collect::<String>();
        parser.advance(&mut term, output.as_bytes());

        assert_eq!(term.grid().history_size(), 8);
        assert_eq!(term.cursor_style().shape, CursorShape::Beam);
    }

    #[test]
    fn applying_runtime_options_preserves_scrollback_when_cursor_style_changes() {
        let size = test_terminal_size();
        let initial = TerminalRuntimeConfig {
            scrollback_history: 8,
            ..TerminalRuntimeConfig::default()
        };
        let mut term: Term<VoidListener> =
            Term::new(initial.term_options().term_config(), &size, VoidListener);

        let updated = TerminalRuntimeConfig {
            default_cursor_style: TerminalCursorStyle::Line,
            ..initial.clone()
        };
        term.set_options(updated.term_options().term_config());
        let mut parser: ansi::Processor = ansi::Processor::new();
        let output = (0..80)
            .map(|index| format!("line-{index}\r\n"))
            .collect::<String>();
        parser.advance(&mut term, output.as_bytes());

        assert_eq!(term.grid().history_size(), 8);
        assert_eq!(term.cursor_style().shape, CursorShape::Beam);
    }

    #[cfg(unix)]
    #[test]
    fn locale_override_plan_forces_lc_ctype_when_no_utf8_and_no_lc_all() {
        assert_eq!(
            super::utf8_locale_override_plan(None, Some("C"), Some("")),
            super::Utf8LocaleOverridePlan::LcCtypeOnly
        );
    }

    #[cfg(unix)]
    #[test]
    fn locale_override_plan_forces_lc_all_when_lc_all_is_non_utf8() {
        assert_eq!(
            super::utf8_locale_override_plan(Some("C"), Some("C"), Some("")),
            super::Utf8LocaleOverridePlan::LcAllAndLcCtype
        );
    }

    #[cfg(unix)]
    #[test]
    fn locale_override_plan_skips_when_utf8_present() {
        assert_eq!(
            super::utf8_locale_override_plan(Some("en_US.UTF-8"), Some("C"), Some("")),
            super::Utf8LocaleOverridePlan::None
        );
    }

    #[cfg(unix)]
    #[test]
    fn locale_override_plan_prefers_lc_all_over_lang() {
        assert_eq!(
            super::utf8_locale_override_plan(
                Some("fr_FR.ISO8859-1"),
                Some("C"),
                Some("en_US.UTF-8")
            ),
            super::Utf8LocaleOverridePlan::LcAllAndLcCtype
        );
    }

    #[cfg(unix)]
    #[test]
    fn locale_override_plan_does_not_skip_for_utf8_substring_false_positive() {
        assert_eq!(
            super::utf8_locale_override_plan(Some("en_US.fakeutf8"), Some("C"), Some("")),
            super::Utf8LocaleOverridePlan::LcAllAndLcCtype
        );
    }

    #[cfg(unix)]
    #[test]
    fn locale_override_plan_skips_for_utf8_with_modifier() {
        assert_eq!(
            super::utf8_locale_override_plan(Some("en_US.UTF-8@variant"), Some("C"), Some("")),
            super::Utf8LocaleOverridePlan::None
        );
    }

    #[cfg(unix)]
    #[test]
    fn preferred_utf8_locale_preserves_lang_region_from_lc_all() {
        assert_eq!(
            super::preferred_utf8_locale(
                Some("fr_FR.ISO8859-1"),
                Some("C"),
                Some("en_US.ISO8859-1")
            ),
            "fr_FR.UTF-8"
        );
    }

    #[cfg(unix)]
    #[test]
    fn preferred_utf8_locale_preserves_locale_modifier() {
        assert_eq!(
            super::preferred_utf8_locale(None, Some("sr_RS@latin"), Some("")),
            "sr_RS.UTF-8@latin"
        );
    }

    #[cfg(unix)]
    #[test]
    fn preferred_utf8_locale_falls_back_for_c_or_posix() {
        assert_eq!(
            super::preferred_utf8_locale(Some("C"), Some("POSIX"), Some("")),
            crate::locale::DEFAULT_UTF8_LOCALE
        );
    }
}
