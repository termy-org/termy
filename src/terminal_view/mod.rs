use crate::colors::TerminalColors;
use crate::commands::{self, CommandAction};
use crate::config::{
    self, AppConfig, CursorStyle as AppCursorStyle, PaneFocusEffect, TabCloseVisibility,
    TabTitleConfig, TabTitleSource, TabWidthMode, TaskConfig, TerminalScrollbarStyle,
    TerminalScrollbarVisibility,
};
use crate::keybindings;
use crate::ui::scrollbar::{ScrollbarVisibilityController, ScrollbarVisibilityMode};
use alacritty_terminal::term::cell::Flags;
use flume::{Sender, bounded};
use gpui::AppContext;
use gpui::{
    AnyElement, App, AsyncApp, ClipboardItem, Context, Element, Entity, ExternalPaths, FocusHandle,
    Focusable, Font, FontWeight, InteractiveElement, IntoElement, KeyDownEvent,
    ModifiersChangedEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, Pixels, Render, ScrollWheelEvent, SharedString, Size,
    StatefulInteractiveElement, Styled, TouchPhase, WeakEntity, Window, WindowBackgroundAppearance,
    div, point, px,
};
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    env,
    ops::Range,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex, atomic::AtomicU64},
    time::{Duration, Instant},
};
use sysinfo::{ProcessesToUpdate, System, get_current_pid};
#[cfg(target_os = "macos")]
use termy_auto_update::{AutoUpdater, UpdateState};
use termy_search::SearchState;
use termy_terminal_ui::{
    CellRenderInfo, PaneTerminal, TabTitleShellIntegration, Terminal as NativeTerminal,
    TerminalCursorState, TerminalCursorStyle, TerminalDamageSnapshot, TerminalDirtySpan,
    TerminalEvent, TerminalGrid, TerminalGridPaintCacheHandle, TerminalGridPaintDamage,
    TerminalGridRows, TerminalMouseMode, TerminalOptions, TerminalRuntimeConfig, TerminalSize,
    TmuxLaunchTarget, WorkingDirFallback as RuntimeWorkingDirFallback, find_link_in_line,
    keystroke_to_input,
};
#[cfg(debug_assertions)]
use termy_terminal_ui::{
    TerminalUiRenderMetricsSnapshot, terminal_ui_render_metrics_reset,
    terminal_ui_render_metrics_snapshot,
};
use termy_toast::ToastManager;

mod ai_input;
mod command_palette;
mod inline_input;
mod interaction;
mod overlay_view;
mod persistence;
mod render;
mod runtime;
mod scrollbar;
mod search;
mod tab_strip;
mod tabs;
mod titles;
#[cfg(target_os = "macos")]
mod update_toasts;

use command_palette::{CommandPaletteMode, CommandPaletteState, TmuxSessionIntent};
use inline_input::{InlineInputAlignment, InlineInputElement, InlineInputState};
use overlay_view::TerminalOverlayView;
use runtime::{RuntimeKind, RuntimeState, TmuxRuntime};
pub(crate) use tab_strip::constants::*;
use tab_strip::state::TabStripState;

const MIN_FONT_SIZE: f32 = 8.0;
const MAX_FONT_SIZE: f32 = 40.0;
const ZOOM_STEP: f32 = 1.0;
#[cfg(target_os = "windows")]
const TITLEBAR_HEIGHT: f32 = 32.0;
#[cfg(not(target_os = "windows"))]
const TITLEBAR_HEIGHT: f32 = 34.0;
const AGENT_SIDEBAR_MIN_WIDTH: f32 = 180.0;
const AGENT_SIDEBAR_MAX_WIDTH: f32 = 1000.0;
const MAX_TAB_TITLE_CHARS: usize = 96;
const DEFAULT_TAB_TITLE: &str = "Terminal";
const COMMAND_TITLE_DELAY_MS: u64 = 250;
#[cfg(not(test))]
const CONFIG_WATCH_INTERVAL_MS: u64 = 750;
const CURSOR_BLINK_INTERVAL_MS: u64 = 530;
const TMUX_TITLE_REFRESH_DEBOUNCE_MS: u64 = 120;
const CHILD_WORKING_DIR_CACHE_TTL_MS: u64 = 1500;
const SELECTION_BG_ALPHA: f32 = 0.35;
const DIM_TEXT_FACTOR: f32 = 0.66;
#[cfg(target_os = "macos")]
const UPDATE_BANNER_HEIGHT: f32 = 44.0;
const COMMAND_PALETTE_WIDTH: f32 = 640.0;
const COMMAND_PALETTE_MAX_ITEMS: usize = 8;
const COMMAND_PALETTE_ROW_HEIGHT: f32 = 30.0;
const COMMAND_PALETTE_SCROLLBAR_WIDTH: f32 = 8.0;
const COMMAND_PALETTE_SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 18.0;
const TERMINAL_SCROLLBAR_GUTTER_WIDTH: f32 = 12.0;
const TERMINAL_SCROLLBAR_TRACK_WIDTH: f32 = 12.0;
const TERMINAL_SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 40.0;
const TERMINAL_SCROLLBAR_HOLD_MS: u64 = 900;
const TERMINAL_SCROLLBAR_FADE_MS: u64 = 140;
const TERMINAL_SCROLLBAR_TRACK_HOLD_REPEAT_MS: u64 = 65;
const TERMINAL_SCROLLBAR_HOLD_DURATION: Duration =
    Duration::from_millis(TERMINAL_SCROLLBAR_HOLD_MS);
const TERMINAL_SCROLLBAR_FADE_DURATION: Duration =
    Duration::from_millis(TERMINAL_SCROLLBAR_FADE_MS);
const TERMINAL_SCROLLBAR_GUTTER_ALPHA: f32 = 0.14;
const TERMINAL_SCROLLBAR_TRACK_ALPHA: f32 = 0.28;
const TERMINAL_SCROLLBAR_THUMB_ALPHA: f32 = 0.56;
const TERMINAL_SCROLLBAR_THUMB_ACTIVE_ALPHA: f32 = 0.78;
const TERMINAL_SCROLLBAR_MATCH_MARKER_ALPHA: f32 = 0.55;
const TERMINAL_SCROLLBAR_CURRENT_MARKER_ALPHA: f32 = 0.92;
const TERMINAL_SCROLLBAR_MARKER_HEIGHT: f32 = 2.0;
const TERMINAL_SCROLLBAR_TRACK_RADIUS: f32 = 0.0;
const TERMINAL_SCROLLBAR_THUMB_RADIUS: f32 = 0.0;
const TERMINAL_SCROLLBAR_THUMB_INSET: f32 = 1.0;
const TERMINAL_SCROLLBAR_MUTED_THEME_BLEND: f32 = 0.38;
const SEARCH_BAR_WIDTH: f32 = 320.0;
const SEARCH_BAR_HEIGHT: f32 = 36.0;
const SEARCH_DEBOUNCE_MS: u64 = 50;
const TMUX_RESIZE_ERROR_TOAST_DEBOUNCE_MS: u64 = 2000;
const DEBUG_OVERLAY_SAMPLE_INTERVAL: Duration = Duration::from_millis(500);
#[cfg(target_os = "windows")]
const TMUX_UNSUPPORTED_WINDOWS_TOAST: &str =
    "tmux integration is unsupported on Windows; using native runtime instead.";
const INPUT_SCROLL_SUPPRESS_MS: u64 = 160;
const TOAST_COPY_FEEDBACK_MS: u64 = 1200;
const WINDOW_RESIZE_INDICATOR_MS: u64 = 850;
const ALT_SCREEN_POLL_FRAME_MS: u64 = 33;
const CHILD_WORKING_DIR_CACHE_TTL: Duration = Duration::from_millis(CHILD_WORKING_DIR_CACHE_TTL_MS);
const OVERLAY_PANEL_ALPHA_FLOOR_RATIO: f32 = 0.72;
const OVERLAY_PANEL_BORDER_ALPHA: f32 = 0.24;
const OVERLAY_PRIMARY_TEXT_ALPHA: f32 = 0.95;
const OVERLAY_MUTED_TEXT_ALPHA: f32 = 0.62;
const COMMAND_PALETTE_PANEL_SOLID_ALPHA: f32 = 0.90;
const COMMAND_PALETTE_INPUT_SOLID_ALPHA: f32 = 0.76;
const COMMAND_PALETTE_ROW_SELECTED_BG_ALPHA: f32 = 0.20;
const COMMAND_PALETTE_SHORTCUT_BG_ALPHA: f32 = 0.10;
const COMMAND_PALETTE_SHORTCUT_TEXT_ALPHA: f32 = 0.80;
const COMMAND_PALETTE_PANEL_BG_ALPHA: f32 = 0.98;
const COMMAND_PALETTE_INPUT_BG_ALPHA: f32 = 0.64;
const COMMAND_PALETTE_INPUT_SELECTION_ALPHA: f32 = 0.28;
const COMMAND_PALETTE_SCROLLBAR_TRACK_ALPHA: f32 = 0.10;
const COMMAND_PALETTE_SCROLLBAR_THUMB_ALPHA: f32 = 0.42;
const SEARCH_BAR_BG_ALPHA: f32 = 0.96;
const SEARCH_INPUT_BG_ALPHA: f32 = 0.60;
const SEARCH_COUNTER_TEXT_ALPHA: f32 = 0.60;
const SEARCH_BUTTON_TEXT_ALPHA: f32 = 0.70;
const SEARCH_BUTTON_HOVER_BG_ALPHA: f32 = 0.20;
const SEARCH_INPUT_SELECTION_ALPHA: f32 = 0.30;
const TAB_SWITCH_HINT_ANIMATION_FRAME_MS: u64 = 16;
const MAX_PANE_FOCUS_STRENGTH: f32 = 2.0;
const NATIVE_PANE_MIN_COLS: u16 = 24;
const NATIVE_PANE_MIN_ROWS: u16 = 8;
#[cfg(debug_assertions)]
const RENDER_METRICS_LOG_INTERVAL: Duration = Duration::from_secs(1);

type TabId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CellPos {
    col: usize,
    row: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SelectionPos {
    col: usize,
    line: i32,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct TerminalViewportGeometry {
    origin_x: f32,
    origin_y: f32,
    width: f32,
    height: f32,
}

#[derive(Clone, Copy, Debug)]
struct TerminalScrollbarDragState {
    thumb_grab_offset: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneResizeAxis {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneResizeEdge {
    Left,
    Right,
    Top,
    Bottom,
}

#[derive(Clone, Debug)]
struct PaneResizeDragState {
    pane_id: String,
    axis: PaneResizeAxis,
    edge: PaneResizeEdge,
    start_x: f32,
    start_y: f32,
    applied_steps: i32,
}

#[derive(Clone, Copy, Debug)]
struct AgentSidebarResizeDragState;

#[derive(Clone, Copy, Debug)]
struct VerticalTabStripResizeDragState;

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingCursorMoveClick {
    pane_id: String,
    selection_start: SelectionPos,
    start_cell: CellPos,
    target: CellPos,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingCursorMovePreview {
    pane_id: String,
    target: CellPos,
    style: TerminalCursorStyle,
}

#[derive(Clone, Copy, Debug)]
struct TerminalScrollbarHit {
    local_y: f32,
    thumb_hit: bool,
    thumb_top: f32,
}

#[derive(Clone, Debug, PartialEq)]
struct TerminalContextMenuState {
    anchor_position: gpui::Point<Pixels>,
    buffer_position: Option<SelectionPos>,
    can_copy: bool,
    can_paste: bool,
    can_ask_ai: bool,
    can_search_google: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalScrollbarMarkerCacheKey {
    results_revision: u64,
    history_size: usize,
    viewport_rows: usize,
    marker_top_limit_bucket: i32,
}

#[derive(Clone, Debug, Default)]
struct TerminalScrollbarMarkerCache {
    key: Option<TerminalScrollbarMarkerCacheKey>,
    marker_tops: Vec<f32>,
}

impl TerminalScrollbarMarkerCache {
    fn clear(&mut self) {
        self.key = None;
        self.marker_tops.clear();
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MouseReportTargetCell {
    pane_id: String,
    col: usize,
    row: usize,
}

#[derive(Clone, Debug, Default)]
struct MouseReportingState {
    left_button: Option<MouseReportTargetCell>,
    middle_button: Option<MouseReportTargetCell>,
    right_button: Option<MouseReportTargetCell>,
    hover_target: Option<MouseReportTargetCell>,
    scroll_accumulator_x: f32,
    scroll_accumulator_y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct PaneFocusPreset {
    inactive_fg_blend: f32,
    inactive_bg_blend: f32,
    inactive_desaturate: f32,
    active_border_alpha: f32,
}

#[allow(clippy::large_enum_variant)]
enum Terminal {
    Tmux(PaneTerminal),
    Native(Mutex<NativeTerminal>),
}

impl Terminal {
    fn new_tmux(size: TerminalSize, options: TerminalOptions) -> Self {
        Self::Tmux(PaneTerminal::new(size, options))
    }

    fn new_native(
        size: TerminalSize,
        configured_working_dir: Option<&str>,
        event_wakeup_tx: Option<Sender<()>>,
        tab_title_shell_integration: Option<&TabTitleShellIntegration>,
        runtime_config: Option<&TerminalRuntimeConfig>,
    ) -> anyhow::Result<Self> {
        Ok(Self::Native(Mutex::new(NativeTerminal::new(
            size,
            configured_working_dir,
            event_wakeup_tx,
            tab_title_shell_integration,
            runtime_config,
        )?)))
    }

    fn feed_output(&self, bytes: &[u8]) {
        if let Self::Tmux(terminal) = self {
            terminal.feed_output(bytes);
        }
    }

    fn hydrate_output(&self, bytes: &[u8]) {
        match self {
            Self::Tmux(terminal) => terminal.feed_output(bytes),
            Self::Native(terminal) => {
                if let Ok(terminal) = terminal.lock() {
                    terminal.hydrate_output(bytes);
                }
            }
        }
    }

    fn write_input(&self, input: &[u8]) {
        if let Self::Native(terminal) = self
            && let Ok(terminal) = terminal.lock()
        {
            terminal.write(input);
        }
    }

    fn process_events(&self) -> Vec<TerminalEvent> {
        match self {
            Self::Tmux(_) => Vec::new(),
            Self::Native(terminal) => terminal
                .lock()
                .map(|terminal| terminal.process_events())
                .unwrap_or_default(),
        }
    }

    fn resize(&self, new_size: TerminalSize) {
        match self {
            Self::Tmux(terminal) => terminal.resize(new_size),
            Self::Native(terminal) => {
                if let Ok(mut terminal) = terminal.lock() {
                    terminal.resize(new_size);
                }
            }
        }
    }

    /// Re-send the current PTY size to deliver SIGWINCH without changing dimensions.
    fn nudge_resize(&self) {
        if let Self::Native(terminal) = self
            && let Ok(terminal) = terminal.lock()
        {
            terminal.nudge_resize();
        }
    }

    fn size(&self) -> TerminalSize {
        match self {
            Self::Tmux(terminal) => terminal.size(),
            Self::Native(terminal) => terminal
                .lock()
                .map(|terminal| terminal.size())
                .unwrap_or_default(),
        }
    }

    fn child_pid(&self) -> Option<u32> {
        match self {
            Self::Tmux(_) => None,
            Self::Native(terminal) => terminal
                .lock()
                .ok()
                .and_then(|terminal| terminal.child_pid()),
        }
    }

    fn scroll_display(&self, delta_lines: i32) -> bool {
        match self {
            Self::Tmux(terminal) => terminal.scroll_display(delta_lines),
            Self::Native(terminal) => terminal
                .lock()
                .map(|terminal| terminal.scroll_display(delta_lines))
                .unwrap_or(false),
        }
    }

    fn scroll_to_bottom(&self) -> bool {
        match self {
            Self::Tmux(terminal) => terminal.scroll_to_bottom(),
            Self::Native(terminal) => terminal
                .lock()
                .map(|terminal| terminal.scroll_to_bottom())
                .unwrap_or(false),
        }
    }

    fn scroll_state(&self) -> (usize, usize) {
        match self {
            Self::Tmux(terminal) => terminal.scroll_state(),
            Self::Native(terminal) => terminal
                .lock()
                .map(|terminal| terminal.scroll_state())
                .unwrap_or((0, 0)),
        }
    }

    fn cursor_state(&self) -> Option<TerminalCursorState> {
        match self {
            Self::Tmux(terminal) => terminal.cursor_state(),
            Self::Native(terminal) => terminal
                .lock()
                .map(|terminal| terminal.cursor_state())
                .unwrap_or(None),
        }
    }

    fn cursor_position(&self) -> (usize, usize) {
        match self {
            Self::Tmux(terminal) => terminal.cursor_position(),
            Self::Native(terminal) => terminal
                .lock()
                .map(|terminal| terminal.cursor_position())
                .unwrap_or((0, 0)),
        }
    }

    fn set_term_options(&self, options: TerminalOptions) {
        match self {
            Self::Tmux(terminal) => terminal.set_term_options(options),
            Self::Native(terminal) => {
                if let Ok(terminal) = terminal.lock() {
                    terminal.set_term_options(options);
                }
            }
        }
    }

    fn bracketed_paste_mode(&self) -> bool {
        match self {
            Self::Tmux(terminal) => terminal.bracketed_paste_mode(),
            Self::Native(terminal) => terminal
                .lock()
                .map(|terminal| terminal.bracketed_paste_mode())
                .unwrap_or(false),
        }
    }

    fn alternate_screen_mode(&self) -> bool {
        match self {
            Self::Tmux(terminal) => terminal.alternate_screen_mode(),
            Self::Native(terminal) => terminal
                .lock()
                .map(|terminal| terminal.alternate_screen_mode())
                .unwrap_or(false),
        }
    }

    fn mouse_mode(&self) -> TerminalMouseMode {
        match self {
            Self::Tmux(terminal) => terminal.mouse_mode(),
            Self::Native(terminal) => terminal
                .lock()
                .map(|terminal| terminal.mouse_mode())
                .unwrap_or_default(),
        }
    }

    fn with_grid<R>(
        &self,
        f: impl FnOnce(&alacritty_terminal::grid::Grid<alacritty_terminal::term::cell::Cell>) -> R,
    ) -> Option<R> {
        match self {
            Self::Tmux(terminal) => Some(terminal.with_term(|term| f(term.grid()))),
            Self::Native(terminal) => {
                if let Ok(terminal) = terminal.lock() {
                    Some(terminal.with_term(|term| f(term.grid())))
                } else {
                    None
                }
            }
        }
    }

    fn take_damage_snapshot(&self) -> TerminalDamageSnapshot {
        match self {
            Self::Tmux(terminal) => terminal.take_damage_snapshot(),
            Self::Native(terminal) => terminal
                .lock()
                .map(|terminal| terminal.take_damage_snapshot())
                .unwrap_or(TerminalDamageSnapshot::Full),
        }
    }

    fn for_each_renderable_cell(
        &self,
        mut visitor: impl FnMut(usize, i32, usize, &alacritty_terminal::term::cell::Cell),
    ) -> Option<usize> {
        macro_rules! visit_term_cells {
            ($term:expr) => {{
                let content = $term.renderable_content();
                let display_offset = content.display_offset;
                for cell in content.display_iter {
                    visitor(
                        display_offset,
                        cell.point.line.0,
                        cell.point.column.0,
                        cell.cell,
                    );
                }
                display_offset
            }};
        }

        match self {
            Self::Tmux(terminal) => Some(terminal.with_term(|term| visit_term_cells!(term))),
            Self::Native(terminal) => {
                if let Ok(terminal) = terminal.lock() {
                    Some(terminal.with_term(|term| visit_term_cells!(term)))
                } else {
                    None
                }
            }
        }
    }
}

struct TerminalPane {
    id: String,
    left: u16,
    top: u16,
    width: u16,
    height: u16,
    degraded: bool,
    terminal: Terminal,
    render_cache: RefCell<TerminalPaneRenderCache>,
    /// Tracks the previous alternate-screen state so that transitions can be
    /// detected during `sync_terminal_size` and a SIGWINCH nudge sent.
    last_alternate_screen: Cell<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TerminalPaneCellColorTransformKey {
    fg_blend_bits: u32,
    bg_blend_bits: u32,
    desaturate_bits: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalPaneRenderCacheKey {
    is_active_pane: bool,
    alternate_screen_mode: bool,
    selection_range: Option<(SelectionPos, SelectionPos)>,
    search_results_revision: Option<u64>,
    search_position: Option<(usize, usize)>,
    effective_background_opacity_bits: u32,
    color_transform: TerminalPaneCellColorTransformKey,
}

#[derive(Clone, Default)]
struct TerminalPaneRenderCache {
    cells: TerminalGridRows,
    cols: usize,
    rows: usize,
    display_offset: usize,
    key: Option<TerminalPaneRenderCacheKey>,
    paint_cache: TerminalGridPaintCacheHandle,
}

impl TerminalPaneRenderCache {
    fn clear(&mut self) {
        self.cells = std::sync::Arc::new(Vec::new());
        self.cols = 0;
        self.rows = 0;
        self.display_offset = 0;
        self.key = None;
        self.paint_cache.clear();
    }
}

#[cfg(debug_assertions)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TerminalRenderMetricsCounters {
    render_count: u64,
    cache_full_count: u64,
    cache_partial_count: u64,
    cache_reuse_count: u64,
    dirty_span_count: u64,
    patched_cell_count: u64,
}

#[cfg(debug_assertions)]
impl TerminalRenderMetricsCounters {
    fn saturating_sub(self, previous: Self) -> Self {
        Self {
            render_count: self.render_count.saturating_sub(previous.render_count),
            cache_full_count: self
                .cache_full_count
                .saturating_sub(previous.cache_full_count),
            cache_partial_count: self
                .cache_partial_count
                .saturating_sub(previous.cache_partial_count),
            cache_reuse_count: self
                .cache_reuse_count
                .saturating_sub(previous.cache_reuse_count),
            dirty_span_count: self
                .dirty_span_count
                .saturating_sub(previous.dirty_span_count),
            patched_cell_count: self
                .patched_cell_count
                .saturating_sub(previous.patched_cell_count),
        }
    }
}

#[cfg(debug_assertions)]
#[derive(Clone, Debug)]
struct TerminalRenderMetricsState {
    enabled: bool,
    counters: TerminalRenderMetricsCounters,
    last_emit_counters: TerminalRenderMetricsCounters,
    last_emit_terminal_ui: TerminalUiRenderMetricsSnapshot,
    last_emit_at: Option<Instant>,
    log_interval: Duration,
}

#[cfg(debug_assertions)]
impl TerminalRenderMetricsState {
    fn parse_env_flag(value: &str) -> bool {
        matches!(value.trim(), "1")
            || value.eq_ignore_ascii_case("true")
            || value.eq_ignore_ascii_case("yes")
            || value.eq_ignore_ascii_case("on")
    }

    fn enabled_from_env() -> bool {
        env::var("TERMY_RENDER_METRICS")
            .ok()
            .is_some_and(|value| Self::parse_env_flag(value.as_str()))
    }

    fn from_env() -> Self {
        let enabled = Self::enabled_from_env();
        if enabled {
            terminal_ui_render_metrics_reset();
        }
        Self {
            enabled,
            counters: TerminalRenderMetricsCounters::default(),
            last_emit_counters: TerminalRenderMetricsCounters::default(),
            last_emit_terminal_ui: terminal_ui_render_metrics_snapshot(),
            last_emit_at: None,
            log_interval: RENDER_METRICS_LOG_INTERVAL,
        }
    }
}

#[derive(Debug)]
struct DebugOverlayStats {
    system: System,
    pid: Option<sysinfo::Pid>,
    sample_started_at: Instant,
    frames_in_sample: u32,
    fps: f32,
    cpu_percent: f32,
    memory_bytes: u64,
}

impl DebugOverlayStats {
    fn new() -> Self {
        let mut stats = Self {
            system: System::new(),
            pid: get_current_pid().ok(),
            sample_started_at: Instant::now(),
            frames_in_sample: 0,
            fps: 0.0,
            cpu_percent: 0.0,
            memory_bytes: 0,
        };
        stats.refresh_process_metrics();
        stats
    }

    fn reset(&mut self) {
        self.sample_started_at = Instant::now();
        self.frames_in_sample = 0;
        self.fps = 0.0;
        self.refresh_process_metrics();
    }

    fn record_frame(&mut self, now: Instant) {
        self.frames_in_sample = self.frames_in_sample.saturating_add(1);
        let elapsed = now.saturating_duration_since(self.sample_started_at);
        if elapsed < DEBUG_OVERLAY_SAMPLE_INTERVAL {
            return;
        }

        let elapsed_secs = elapsed.as_secs_f32();
        if elapsed_secs > f32::EPSILON {
            self.fps = self.frames_in_sample as f32 / elapsed_secs;
        }
        self.sample_started_at = now;
        self.frames_in_sample = 0;
        self.refresh_process_metrics();
    }

    fn refresh_process_metrics(&mut self) {
        let Some(pid) = self.pid else {
            self.cpu_percent = 0.0;
            self.memory_bytes = 0;
            return;
        };

        let _ = self
            .system
            .refresh_processes(ProcessesToUpdate::Some(&[pid]), true);
        if let Some(process) = self.system.process(pid) {
            self.cpu_percent = process.cpu_usage();
            self.memory_bytes = process.memory();
        }
    }
}

struct TerminalTab {
    id: TabId,
    window_id: String,
    window_index: i32,
    panes: Vec<TerminalPane>,
    active_pane_id: String,
    manual_title: Option<String>,
    explicit_title: Option<String>,
    shell_title: Option<String>,
    current_command: Option<String>,
    pending_command_title: Option<String>,
    pending_command_token: u64,
    last_prompt_cwd: Option<String>,
    title: String,
    title_text_width: f32,
    sticky_title_width: f32,
    display_width: f32,
    running_process: bool,
}

struct NativePaneZoomSnapshot {
    other_panes: Vec<TerminalPane>,
    active_pane_geometry: (u16, u16, u16, u16),
    active_pane_id: String,
    active_original_index: usize,
}
impl TerminalTab {
    fn active_pane_index(&self) -> Option<usize> {
        self.panes
            .iter()
            .position(|pane| pane.id == self.active_pane_id)
            .or_else(|| (!self.panes.is_empty()).then_some(0))
    }

    fn active_terminal(&self) -> Option<&Terminal> {
        self.active_pane_index()
            .and_then(|index| self.panes.get(index))
            .map(|pane| &pane.terminal)
    }

    fn active_pane_id(&self) -> Option<&str> {
        self.active_pane_index()
            .and_then(|index| self.panes.get(index))
            .map(|pane| pane.id.as_str())
    }
}

enum ExplicitTitlePayload {
    Prompt { title: String, cwd: String },
    Command { title: String, command: String },
    Title(String),
}

#[derive(Clone, Debug)]
struct ChildWorkingDirCacheEntry {
    value: Option<String>,
    resolved_at: Instant,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HoveredLink {
    row: usize,
    start_col: usize,
    end_col: usize,
    target: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
enum BackgroundPlatform {
    MacOs,
    Windows,
    Linux,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BackgroundSupportContext {
    platform: BackgroundPlatform,
    linux_wayland_session: bool,
}

impl BackgroundSupportContext {
    fn current() -> Self {
        #[cfg(target_os = "macos")]
        let platform = BackgroundPlatform::MacOs;
        #[cfg(target_os = "windows")]
        let platform = BackgroundPlatform::Windows;
        #[cfg(target_os = "linux")]
        let platform = BackgroundPlatform::Linux;
        #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
        let platform = BackgroundPlatform::Other;

        #[cfg(target_os = "linux")]
        let linux_wayland_session = std::env::var("XDG_SESSION_TYPE")
            .ok()
            .is_some_and(|session_type| session_type.eq_ignore_ascii_case("wayland"))
            || std::env::var_os("WAYLAND_DISPLAY").is_some();
        #[cfg(not(target_os = "linux"))]
        let linux_wayland_session = false;

        Self {
            platform,
            linux_wayland_session,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlurFallbackReason {
    None,
    KnownUnsupported,
    UnknownSupport,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct ResolvedBackgroundAppearance {
    appearance: WindowBackgroundAppearance,
    blur_fallback: BlurFallbackReason,
}

fn resolve_background_appearance(
    background_opacity: f32,
    background_blur: bool,
    context: BackgroundSupportContext,
) -> ResolvedBackgroundAppearance {
    let opacity = background_opacity.clamp(0.0, 1.0);
    if opacity >= 1.0 {
        return ResolvedBackgroundAppearance {
            appearance: WindowBackgroundAppearance::Opaque,
            blur_fallback: BlurFallbackReason::None,
        };
    }

    if !background_blur {
        return ResolvedBackgroundAppearance {
            appearance: WindowBackgroundAppearance::Transparent,
            blur_fallback: BlurFallbackReason::None,
        };
    }

    match context.platform {
        BackgroundPlatform::MacOs | BackgroundPlatform::Windows => ResolvedBackgroundAppearance {
            appearance: WindowBackgroundAppearance::Blurred,
            blur_fallback: BlurFallbackReason::None,
        },
        BackgroundPlatform::Linux => {
            if context.linux_wayland_session {
                ResolvedBackgroundAppearance {
                    appearance: WindowBackgroundAppearance::Blurred,
                    blur_fallback: BlurFallbackReason::UnknownSupport,
                }
            } else {
                ResolvedBackgroundAppearance {
                    appearance: WindowBackgroundAppearance::Transparent,
                    blur_fallback: BlurFallbackReason::KnownUnsupported,
                }
            }
        }
        BackgroundPlatform::Other => ResolvedBackgroundAppearance {
            appearance: WindowBackgroundAppearance::Blurred,
            blur_fallback: BlurFallbackReason::UnknownSupport,
        },
    }
}

fn background_opacity_factor(background_opacity: f32) -> f32 {
    background_opacity.clamp(0.0, 1.0)
}

fn scaled_background_alpha_for_opacity(base_alpha: f32, background_opacity: f32) -> f32 {
    (base_alpha * background_opacity_factor(background_opacity)).clamp(0.0, 1.0)
}

fn scaled_chrome_alpha_for_opacity(base_alpha: f32, background_opacity: f32) -> f32 {
    scaled_background_alpha_for_opacity(base_alpha, background_opacity)
}

fn adaptive_overlay_panel_alpha_for_opacity(base_alpha: f32, background_opacity: f32) -> f32 {
    let floor = base_alpha * OVERLAY_PANEL_ALPHA_FLOOR_RATIO;
    scaled_background_alpha_for_opacity(base_alpha, background_opacity)
        .max(floor)
        .clamp(0.0, 1.0)
}

fn adaptive_overlay_panel_alpha_with_floor_for_opacity(
    base_alpha: f32,
    background_opacity: f32,
    translucent_floor_alpha: f32,
) -> f32 {
    let alpha = adaptive_overlay_panel_alpha_for_opacity(base_alpha, background_opacity);
    if background_opacity_factor(background_opacity) < 1.0 {
        alpha.max(translucent_floor_alpha).clamp(0.0, 1.0)
    } else {
        alpha
    }
}

fn blend_rgba(base: gpui::Rgba, tint: gpui::Rgba, tint_factor: f32) -> gpui::Rgba {
    let tint_factor = tint_factor.clamp(0.0, 1.0);
    let base_factor = 1.0 - tint_factor;
    gpui::Rgba {
        r: (base.r * base_factor) + (tint.r * tint_factor),
        g: (base.g * base_factor) + (tint.g * tint_factor),
        b: (base.b * base_factor) + (tint.b * tint_factor),
        a: (base.a * base_factor) + (tint.a * tint_factor),
    }
}

fn resolve_chrome_stroke_color(
    chrome_background: gpui::Rgba,
    foreground: gpui::Rgba,
    foreground_mix: f32,
) -> gpui::Rgba {
    let mix = foreground_mix.clamp(0.0, 1.0);
    let inv_mix = 1.0 - mix;

    gpui::Rgba {
        r: (chrome_background.r * inv_mix) + (foreground.r * mix),
        g: (chrome_background.g * inv_mix) + (foreground.g * mix),
        b: (chrome_background.b * inv_mix) + (foreground.b * mix),
        a: 1.0,
    }
}

fn pane_divider_color(chrome_background: gpui::Rgba, foreground: gpui::Rgba) -> gpui::Rgba {
    resolve_chrome_stroke_color(chrome_background, foreground, TAB_STROKE_FOREGROUND_MIX)
}

fn pane_focus_strength_factor(pane_focus_strength: f32) -> f32 {
    pane_focus_strength.clamp(0.0, MAX_PANE_FOCUS_STRENGTH)
}

fn pane_focus_preset(effect: PaneFocusEffect) -> Option<PaneFocusPreset> {
    match effect {
        PaneFocusEffect::Off => None,
        PaneFocusEffect::SoftSpotlight => Some(PaneFocusPreset {
            inactive_fg_blend: 0.36,
            inactive_bg_blend: 0.12,
            inactive_desaturate: 0.0,
            active_border_alpha: 0.38,
        }),
        PaneFocusEffect::Cinematic => Some(PaneFocusPreset {
            inactive_fg_blend: 0.52,
            inactive_bg_blend: 0.18,
            inactive_desaturate: 0.34,
            active_border_alpha: 0.46,
        }),
        PaneFocusEffect::Minimal => Some(PaneFocusPreset {
            inactive_fg_blend: 0.22,
            inactive_bg_blend: 0.08,
            inactive_desaturate: 0.0,
            active_border_alpha: 0.28,
        }),
    }
}

#[derive(Clone, Copy)]
struct OverlayStyleBuilder<'a> {
    colors: &'a TerminalColors,
    background_opacity: f32,
}

impl<'a> OverlayStyleBuilder<'a> {
    fn new(colors: &'a TerminalColors, background_opacity: f32) -> Self {
        Self {
            colors,
            background_opacity,
        }
    }

    fn panel_background(self, base_alpha: f32) -> gpui::Rgba {
        let alpha = adaptive_overlay_panel_alpha_for_opacity(base_alpha, self.background_opacity);
        self.with_alpha(self.colors.background, alpha)
    }

    fn panel_background_with_floor(
        self,
        base_alpha: f32,
        translucent_floor_alpha: f32,
    ) -> gpui::Rgba {
        let alpha = adaptive_overlay_panel_alpha_with_floor_for_opacity(
            base_alpha,
            self.background_opacity,
            translucent_floor_alpha,
        );
        self.with_alpha(self.colors.background, alpha)
    }

    fn panel_cursor(self, base_alpha: f32) -> gpui::Rgba {
        let alpha = adaptive_overlay_panel_alpha_for_opacity(base_alpha, self.background_opacity);
        self.with_alpha(self.colors.cursor, alpha)
    }

    fn panel_foreground(self, base_alpha: f32) -> gpui::Rgba {
        let alpha = adaptive_overlay_panel_alpha_for_opacity(base_alpha, self.background_opacity);
        self.with_alpha(self.colors.foreground, alpha)
    }

    fn transparent_background(self) -> gpui::Rgba {
        self.with_alpha(self.colors.background, 0.0)
    }

    fn with_alpha(self, mut color: gpui::Rgba, alpha: f32) -> gpui::Rgba {
        color.a = alpha.clamp(0.0, 1.0);
        color
    }
}

pub(crate) fn initial_window_background_appearance(
    config: &AppConfig,
) -> WindowBackgroundAppearance {
    resolve_background_appearance(
        config.background_opacity,
        config.background_blur,
        BackgroundSupportContext::current(),
    )
    .appearance
}

/// The main terminal view component
pub struct TerminalView {
    tabs: Vec<TerminalTab>,
    native_pane_zoom_snapshots: HashMap<TabId, NativePaneZoomSnapshot>,
    next_tab_id: TabId,
    active_tab: usize,
    renaming_tab: Option<usize>,
    rename_input: InlineInputState,
    event_wakeup_tx: Sender<()>,
    focus_handle: FocusHandle,
    theme_id: String,
    colors: TerminalColors,
    inactive_tab_scrollback: Option<usize>,
    tasks: Vec<TaskConfig>,
    warn_on_quit: bool,
    warn_on_quit_with_running_process: bool,
    tab_title: TabTitleConfig,
    tab_close_visibility: TabCloseVisibility,
    tab_width_mode: TabWidthMode,
    vertical_tabs: bool,
    vertical_tabs_width: f32,
    vertical_tabs_minimized: bool,
    show_termy_in_titlebar: bool,
    tab_shell_integration: TabTitleShellIntegration,
    configured_working_dir: Option<String>,
    child_working_dir_cache: HashMap<u32, ChildWorkingDirCacheEntry>,
    child_working_dir_lookup_pending: HashSet<u32>,
    terminal_runtime: TerminalRuntimeConfig,
    runtime: RuntimeState,
    tmux_enabled_config: bool,
    native_tab_persistence: bool,
    native_layout_autosave: bool,
    native_buffer_persistence: bool,
    agent_sidebar_enabled: bool,
    agent_sidebar_width: f32,
    agent_sidebar_open: bool,
    agent_sidebar_input_active: bool,
    agent_sidebar_input: InlineInputState,
    current_named_layout: Option<String>,
    native_persist_revision: Arc<AtomicU64>,
    tmux_show_active_pane_border: bool,
    config_path: Option<PathBuf>,
    config_fingerprint: Option<u64>,
    last_config_error_message: Option<String>,
    cached_tmux_binary: Option<String>,
    font_family: SharedString,
    base_font_size: f32,
    font_size: Pixels,
    cursor_style: AppCursorStyle,
    cursor_blink: bool,
    cursor_blink_visible: bool,
    background_opacity: f32,
    preview_background_opacity: Option<config::BackgroundOpacityPreview>,
    background_blur: bool,
    background_support_context: BackgroundSupportContext,
    last_window_background_appearance: Option<WindowBackgroundAppearance>,
    warned_blur_unsupported_once: bool,
    padding_x: f32,
    padding_y: f32,
    mouse_scroll_multiplier: f32,
    pane_focus_effect: PaneFocusEffect,
    pane_focus_strength: f32,
    line_height: f32,
    copy_on_select: bool,
    copy_on_select_toast: bool,
    selection_anchor: Option<SelectionPos>,
    selection_head: Option<SelectionPos>,
    selection_dragging: bool,
    selection_moved: bool,
    pending_cursor_move_click: Option<PendingCursorMoveClick>,
    pending_cursor_move_preview: Option<PendingCursorMovePreview>,
    terminal_context_menu: Option<TerminalContextMenuState>,
    hovered_link: Option<HoveredLink>,
    hovered_toast: Option<u64>,
    copied_toast_feedback: Option<(u64, Instant)>,
    toast_animation_scheduled: bool,
    toast_manager: ToastManager,
    overlay_view: Option<Entity<TerminalOverlayView>>,
    command_palette: CommandPaletteState,
    last_viewport_size_px: Option<(i32, i32)>,
    resize_indicator_dims: Option<(u16, u16)>,
    resize_indicator_visible_until: Option<Instant>,
    resize_indicator_animation_scheduled: bool,
    alt_screen_refresh_scheduled: bool,
    show_debug_overlay: bool,
    debug_overlay_stats: DebugOverlayStats,
    install_cli_available: bool,
    tab_strip: TabStripState,
    inline_input_selecting: bool,
    mouse_reporting: MouseReportingState,
    terminal_scroll_accumulator_y: f32,
    input_scroll_suppress_until: Option<Instant>,
    last_tmux_resize_error_at: Option<Instant>,
    terminal_scrollbar_visibility: TerminalScrollbarVisibility,
    terminal_scrollbar_style: TerminalScrollbarStyle,
    terminal_scrollbar_visibility_controller: ScrollbarVisibilityController,
    terminal_scrollbar_animation_active: bool,
    terminal_scrollbar_drag: Option<TerminalScrollbarDragState>,
    terminal_scrollbar_track_hold_local_y: Option<f32>,
    terminal_scrollbar_track_hold_active: bool,
    pane_resize_drag: Option<PaneResizeDragState>,
    agent_sidebar_resize_drag: Option<AgentSidebarResizeDragState>,
    vertical_tab_strip_resize_drag: Option<VerticalTabStripResizeDragState>,
    terminal_scrollbar_marker_cache: TerminalScrollbarMarkerCache,
    /// Cached cell dimensions
    cell_size: Option<Size<Pixels>>,
    // Search state
    search_open: bool,
    search_input: InlineInputState,
    search_state: SearchState,
    search_debounce_token: u64,
    // AI input state
    ai_input_open: bool,
    ai_input: InlineInputState,
    // IME composing state for terminal mode
    ime_marked_text: Option<String>,
    ime_selected_range: Option<Range<usize>>,
    // Pending clipboard write from OSC 52
    pending_clipboard: Option<String>,
    #[cfg(debug_assertions)]
    render_metrics: TerminalRenderMetricsState,
    quit_prompt_in_flight: bool,
    allow_quit_without_prompt: bool,
    #[cfg(target_os = "macos")]
    auto_updater: Option<Entity<AutoUpdater>>,
    #[cfg(target_os = "macos")]
    show_update_banner: bool,
    #[cfg(target_os = "macos")]
    last_notified_update_state: Option<UpdateState>,
    #[cfg(target_os = "macos")]
    update_check_toast_id: Option<u64>,
}

impl TerminalView {
    fn install_cli_availability_from_probe(is_cli_installed: bool) -> bool {
        !is_cli_installed
    }

    fn install_cli_available_from_system() -> bool {
        Self::install_cli_availability_from_probe(termy_cli_install_core::is_cli_installed())
    }

    fn refreshed_install_cli_availability(
        current_available: bool,
        is_cli_installed: bool,
    ) -> (bool, bool) {
        let next_available = Self::install_cli_availability_from_probe(is_cli_installed);
        (next_available, next_available != current_available)
    }

    pub(super) fn install_cli_available(&self) -> bool {
        self.install_cli_available
    }

    pub(super) fn refresh_install_cli_availability(&mut self) -> bool {
        let (next_available, changed) = Self::refreshed_install_cli_availability(
            self.install_cli_available,
            termy_cli_install_core::is_cli_installed(),
        );
        self.install_cli_available = next_available;
        changed
    }

    fn runtime_config_from_app_config(config: &AppConfig) -> TerminalRuntimeConfig {
        let working_dir_fallback = match config.working_dir_fallback {
            config::WorkingDirFallback::Home => RuntimeWorkingDirFallback::Home,
            config::WorkingDirFallback::Process => RuntimeWorkingDirFallback::Process,
        };

        TerminalRuntimeConfig {
            shell: config.shell.clone(),
            term: config.term.clone(),
            colorterm: config.colorterm.clone(),
            working_dir_fallback,
            scrollback_history: config.scrollback_history,
            default_cursor_style: match config.cursor_style {
                AppCursorStyle::Line => TerminalCursorStyle::Line,
                AppCursorStyle::Block => TerminalCursorStyle::Block,
            },
        }
    }

    #[cfg(test)]
    fn uses_event_driven_tmux_wakeup() -> bool {
        true
    }

    fn user_home_dir() -> Option<PathBuf> {
        dirs::home_dir()
    }

    fn resolve_configured_working_directory(configured: Option<&str>) -> Option<PathBuf> {
        let configured = configured?.trim();
        if configured.is_empty() {
            return None;
        }

        let path = if configured == "~" {
            Self::user_home_dir()?
        } else if let Some(relative) = configured
            .strip_prefix("~/")
            .or_else(|| configured.strip_prefix("~\\"))
        {
            Self::user_home_dir()?.join(relative)
        } else {
            PathBuf::from(configured)
        };

        path.is_dir().then_some(path)
    }

    fn default_working_directory_with_fallback(
        fallback: RuntimeWorkingDirFallback,
    ) -> Option<PathBuf> {
        if fallback == RuntimeWorkingDirFallback::Home
            && let Some(home) = Self::user_home_dir()
            && home.is_dir()
        {
            return Some(home);
        }

        env::current_dir().ok()
    }

    fn display_working_directory_for_prompt(path: &Path) -> String {
        if let Some(home) = Self::user_home_dir() {
            if path == home.as_path() {
                return "~".to_string();
            }

            if let Ok(relative) = path.strip_prefix(&home) {
                let relative = relative.to_string_lossy();
                return format!("~{}{}", std::path::MAIN_SEPARATOR, relative);
            }
        }

        path.to_string_lossy().into_owned()
    }

    fn predicted_prompt_cwd(
        configured_working_dir: Option<&str>,
        fallback: RuntimeWorkingDirFallback,
    ) -> Option<String> {
        let path = Self::resolve_configured_working_directory(configured_working_dir)
            .or_else(|| Self::default_working_directory_with_fallback(fallback))?;
        Some(Self::display_working_directory_for_prompt(&path))
    }

    fn working_dir_title_candidate(value: &str) -> Option<&str> {
        let value = value.trim();
        if value.is_empty() {
            return None;
        }

        if let Some(prompt) = value
            .rsplit_once("prompt:")
            .map(|(_, prompt)| prompt.trim())
        {
            if !prompt.is_empty() {
                return Some(prompt);
            }
        }

        if let Some(cwd) = value.strip_prefix("cwd:").map(str::trim) {
            if !cwd.is_empty() {
                return Some(cwd);
            }
        }

        Some(value)
    }

    fn looks_like_working_dir_path(value: &str) -> bool {
        value.starts_with(std::path::MAIN_SEPARATOR)
            || value == "~"
            || value.starts_with("~/")
            || value.starts_with("~\\")
            || value.chars().nth(1).is_some_and(|ch| ch == ':')
                && value
                    .chars()
                    .next()
                    .is_some_and(|first| first.is_ascii_alphabetic())
                && value
                    .chars()
                    .nth(2)
                    .is_some_and(|sep| sep == '/' || sep == '\\')
    }

    fn working_dir_for_child_pid_blocking(pid: u32) -> Option<String> {
        #[cfg(any(target_os = "linux", target_os = "android"))]
        {
            let path = std::fs::read_link(format!("/proc/{pid}/cwd")).ok()?;
            return path.is_dir().then(|| path.to_string_lossy().into_owned());
        }

        #[cfg(target_os = "macos")]
        {
            let output = Command::new("lsof")
                .args(["-a", "-d", "cwd", "-p", &pid.to_string(), "-Fn"])
                .stdin(Stdio::null())
                .stderr(Stdio::null())
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if let Some(path) = line.strip_prefix('n') {
                    let path = path.trim();
                    if !path.is_empty() {
                        return Some(path.to_string());
                    }
                }
            }
            None
        }

        #[cfg(not(any(target_os = "linux", target_os = "android", target_os = "macos")))]
        {
            let _ = pid;
            None
        }
    }

    fn complete_child_working_dir_lookup(&mut self, pid: u32, value: Option<String>) {
        self.child_working_dir_lookup_pending.remove(&pid);
        self.child_working_dir_cache.insert(
            pid,
            ChildWorkingDirCacheEntry {
                value,
                resolved_at: Instant::now(),
            },
        );
    }

    fn schedule_child_working_dir_lookup(&mut self, pid: u32, cx: &mut Context<Self>) {
        if !self.child_working_dir_lookup_pending.insert(pid) {
            return;
        }

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let value = smol::unblock(move || Self::working_dir_for_child_pid_blocking(pid)).await;
            let _ = cx.update(|cx| {
                this.update(cx, |view, _cx| {
                    view.complete_child_working_dir_lookup(pid, value);
                })
            });
        })
        .detach();
    }

    fn cached_or_queued_working_dir_for_child_pid(
        &mut self,
        pid: u32,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        if let Some((cached_value, resolved_at)) = self
            .child_working_dir_cache
            .get(&pid)
            .map(|entry| (entry.value.clone(), entry.resolved_at))
        {
            let is_fresh = Instant::now().saturating_duration_since(resolved_at)
                <= CHILD_WORKING_DIR_CACHE_TTL;
            if !is_fresh {
                self.schedule_child_working_dir_lookup(pid, cx);
            }
            return cached_value;
        }

        self.schedule_child_working_dir_lookup(pid, cx);
        None
    }

    fn preferred_working_dir_for_new_native_session(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let active_tab = self.active_tab;
        let prompt_cwd = self
            .tabs
            .get(active_tab)
            .and_then(|tab| tab.last_prompt_cwd.clone());
        let process_cwd = self
            .tabs
            .get(active_tab)
            .and_then(TerminalTab::active_terminal)
            .and_then(Terminal::child_pid)
            .and_then(|pid| self.cached_or_queued_working_dir_for_child_pid(pid, cx));
        let title_cwd = self
            .tabs
            .get(active_tab)
            .and_then(|tab| {
                [
                    tab.explicit_title.as_deref(),
                    tab.shell_title.as_deref(),
                    Some(tab.title.as_str()),
                ]
                .into_iter()
                .flatten()
                .find_map(Self::working_dir_title_candidate)
                .map(str::to_string)
            })
            .filter(|candidate| Self::looks_like_working_dir_path(candidate.as_str()));

        prompt_cwd
            .or(process_cwd)
            .or(title_cwd)
            .or_else(|| self.configured_working_dir.clone())
    }

    fn runtime_kind(&self) -> RuntimeKind {
        self.runtime.kind()
    }

    fn runtime_uses_tmux(&self) -> bool {
        self.runtime_kind().uses_tmux()
    }

    fn tmux_runtime(&self) -> &TmuxRuntime {
        self.runtime
            .as_tmux()
            .expect("tmux runtime must exist while tmux backend is active")
    }

    fn tmux_runtime_mut(&mut self) -> &mut TmuxRuntime {
        self.runtime
            .as_tmux_mut()
            .expect("tmux runtime must exist while tmux backend is active")
    }

    fn create_native_tab(
        tab_id: TabId,
        terminal: Terminal,
        cols: u16,
        rows: u16,
        predicted_prompt_title: Option<String>,
    ) -> TerminalTab {
        let title = predicted_prompt_title
            .as_deref()
            .unwrap_or(DEFAULT_TAB_TITLE)
            .to_string();
        let title_text_width = 0.0;
        let sticky_title_width = Self::tab_display_width_for_text_px_without_close_with_max(
            title_text_width,
            TAB_MAX_WIDTH,
        );
        let display_width =
            Self::tab_display_width_for_text_px_with_max(title_text_width, TAB_MAX_WIDTH);
        let pane_id = format!("%native-{tab_id}");
        let pane = TerminalPane {
            id: pane_id.clone(),
            left: 0,
            top: 0,
            width: cols.max(1),
            height: rows.max(1),
            degraded: false,
            terminal,
            render_cache: RefCell::new(TerminalPaneRenderCache::default()),
            last_alternate_screen: Cell::new(false),
        };
        TerminalTab {
            id: tab_id,
            window_id: format!("@native-{tab_id}"),
            window_index: 0,
            panes: vec![pane],
            active_pane_id: pane_id,
            manual_title: None,
            explicit_title: predicted_prompt_title,
            shell_title: None,
            current_command: None,
            pending_command_title: None,
            pending_command_token: 0,
            last_prompt_cwd: None,
            title,
            title_text_width,
            sticky_title_width,
            display_width,
            running_process: false,
        }
    }

    fn pane_terminal_by_id(&self, pane_id: &str) -> Option<&Terminal> {
        self.tabs
            .iter()
            .flat_map(|tab| tab.panes.iter())
            .find(|pane| pane.id == pane_id)
            .map(|pane| &pane.terminal)
    }

    fn is_active_pane_id(&self, pane_id: &str) -> bool {
        self.tabs
            .get(self.active_tab)
            .and_then(|tab| tab.active_pane_id())
            == Some(pane_id)
    }

    fn active_pane_id(&self) -> Option<&str> {
        self.tabs
            .get(self.active_tab)
            .and_then(|tab| tab.active_pane_id())
    }

    fn active_tab_ref(&self) -> Option<&TerminalTab> {
        self.tabs.get(self.active_tab)
    }

    fn active_pane_ref(&self) -> Option<&TerminalPane> {
        let tab = self.active_tab_ref()?;
        let index = tab.active_pane_index()?;
        tab.panes.get(index)
    }

    fn background_opacity_factor(&self) -> f32 {
        background_opacity_factor(self.effective_background_opacity())
    }

    fn scaled_background_alpha(&self, base_alpha: f32) -> f32 {
        scaled_background_alpha_for_opacity(base_alpha, self.effective_background_opacity())
    }

    fn scaled_chrome_alpha(&self, base_alpha: f32) -> f32 {
        scaled_chrome_alpha_for_opacity(base_alpha, self.effective_background_opacity())
    }

    fn effective_background_opacity(&self) -> f32 {
        config::effective_background_opacity(
            self.background_opacity,
            self.preview_background_opacity,
        )
    }

    fn tab_switch_hints_blocked(&self) -> bool {
        self.is_command_palette_open() || self.search_open || self.ai_input_open
    }

    pub(crate) fn tab_switch_hint_progress(&self, now: Instant) -> f32 {
        self.tab_strip
            .switch_hints
            .progress(now, self.tab_switch_hints_blocked())
    }

    fn schedule_tab_switch_hint_animation(&mut self, cx: &mut Context<Self>) {
        if !self
            .tab_strip
            .switch_hints
            .begin_animation_frame(Instant::now(), self.tab_switch_hints_blocked())
        {
            return;
        }

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            smol::Timer::after(Duration::from_millis(TAB_SWITCH_HINT_ANIMATION_FRAME_MS)).await;
            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    view.tab_strip.switch_hints.finish_animation_frame();
                    cx.notify();
                })
            });
        })
        .detach();
    }

    fn pane_focus_config(&self) -> Option<(PaneFocusPreset, f32)> {
        let preset = pane_focus_preset(self.pane_focus_effect)?;
        let strength = pane_focus_strength_factor(self.pane_focus_strength);
        (strength > f32::EPSILON).then_some((preset, strength))
    }

    fn effective_terminal_padding(&self) -> (f32, f32) {
        if Self::uses_outer_terminal_padding(
            self.tabs
                .get(self.active_tab)
                .map_or(0, |tab| tab.panes.len()),
        ) {
            (self.padding_x, self.padding_y)
        } else {
            // Multi-pane layouts use per-pane content padding (native) or pane-managed
            // geometry (tmux), so disable global outer padding in that mode.
            (0.0, 0.0)
        }
    }

    fn native_split_content_padding(&self) -> (f32, f32) {
        if Self::uses_native_split_content_padding(
            self.runtime_uses_tmux(),
            self.tabs
                .get(self.active_tab)
                .map_or(0, |tab| tab.panes.len()),
        ) {
            (self.padding_x, self.padding_y)
        } else {
            (0.0, 0.0)
        }
    }

    fn uses_outer_terminal_padding(pane_count: usize) -> bool {
        pane_count <= 1
    }

    fn uses_native_split_content_padding(runtime_uses_tmux: bool, pane_count: usize) -> bool {
        !runtime_uses_tmux && pane_count > 1
    }

    fn overlay_style(&self) -> OverlayStyleBuilder<'_> {
        OverlayStyleBuilder::new(&self.colors, self.effective_background_opacity())
    }

    fn ensure_overlay_view(&mut self, cx: &mut Context<Self>) -> Entity<TerminalOverlayView> {
        if let Some(overlay_view) = self.overlay_view.clone() {
            return overlay_view;
        }

        let parent = cx.entity().downgrade();
        let overlay_view = cx.new(|_| TerminalOverlayView::new(parent));
        self.overlay_view = Some(overlay_view.clone());
        overlay_view
    }

    fn notify_overlay(&mut self, cx: &mut Context<Self>) {
        let overlay_view = self.ensure_overlay_view(cx);
        overlay_view.update(cx, |_overlay_view, cx| {
            cx.notify();
        });
    }

    fn record_debug_overlay_frame(&mut self) {
        if !self.show_debug_overlay {
            return;
        }
        self.debug_overlay_stats.record_frame(Instant::now());
    }

    fn debug_overlay_memory_label(&self) -> String {
        let mib = self.debug_overlay_stats.memory_bytes as f64 / (1024.0 * 1024.0);
        format!("{mib:.1} MB")
    }

    fn track_window_resize_indicator(&mut self, viewport: Size<Pixels>, now: Instant) {
        let viewport_width: f32 = viewport.width.into();
        let viewport_height: f32 = viewport.height.into();
        let viewport_key = (
            viewport_width.round() as i32,
            viewport_height.round() as i32,
        );
        if self.last_viewport_size_px == Some(viewport_key) {
            return;
        }
        self.last_viewport_size_px = Some(viewport_key);

        if let Some(size) = self.active_terminal().map(|terminal| terminal.size()) {
            self.resize_indicator_dims = Some((size.cols, size.rows));
            self.resize_indicator_visible_until =
                Some(now + Duration::from_millis(WINDOW_RESIZE_INDICATOR_MS));
        }
    }

    #[cfg(target_os = "macos")]
    pub(super) fn overlay_banner_visible_for_state(state: Option<&UpdateState>) -> bool {
        matches!(
            state,
            Some(
                UpdateState::Available { .. }
                    | UpdateState::Downloading { .. }
                    | UpdateState::Downloaded { .. }
                    | UpdateState::Installing { .. }
                    | UpdateState::Installed { .. }
                    | UpdateState::Error(_)
            )
        )
    }

    fn scrollbar_color(
        &self,
        overlay_style: OverlayStyleBuilder<'_>,
        base_alpha: f32,
    ) -> gpui::Rgba {
        match self.terminal_scrollbar_style {
            TerminalScrollbarStyle::Neutral => overlay_style.panel_foreground(base_alpha),
            TerminalScrollbarStyle::MutedTheme => {
                let background = overlay_style.panel_background(base_alpha);
                let accent = overlay_style.panel_cursor(base_alpha);
                blend_rgba(background, accent, TERMINAL_SCROLLBAR_MUTED_THEME_BLEND)
            }
            TerminalScrollbarStyle::Theme => overlay_style.panel_cursor(base_alpha),
        }
    }

    pub(super) fn terminal_scrollbar_mode(&self) -> ScrollbarVisibilityMode {
        match self.terminal_scrollbar_visibility {
            TerminalScrollbarVisibility::Off => ScrollbarVisibilityMode::AlwaysOff,
            TerminalScrollbarVisibility::Always => ScrollbarVisibilityMode::AlwaysOn,
            TerminalScrollbarVisibility::OnScroll => ScrollbarVisibilityMode::OnScroll,
        }
    }

    pub(super) fn terminal_scrollbar_alpha(&self, now: Instant) -> f32 {
        self.terminal_scrollbar_visibility_controller.alpha(
            self.terminal_scrollbar_mode(),
            now,
            TERMINAL_SCROLLBAR_HOLD_DURATION,
            TERMINAL_SCROLLBAR_FADE_DURATION,
        )
    }

    fn terminal_scrollbar_layout_for_track(
        &self,
        track_height: f32,
    ) -> Option<scrollbar::TerminalScrollbarLayout> {
        let terminal = self.active_terminal()?;
        let size = terminal.size();
        let viewport_rows = size.rows as usize;
        if viewport_rows == 0 {
            return None;
        }

        let line_height: f32 = size.cell_height.into();
        let (display_offset, history_size) = terminal.scroll_state();
        scrollbar::compute_layout(
            display_offset,
            history_size,
            viewport_rows,
            line_height,
            track_height,
            TERMINAL_SCROLLBAR_MIN_THUMB_HEIGHT,
        )
    }

    pub(super) fn terminal_viewport_geometry(&self) -> Option<TerminalViewportGeometry> {
        let pane = self.active_pane_ref()?;
        let size = pane.terminal.size();
        if size.cols == 0 || size.rows == 0 {
            return None;
        }

        let (padding_x, padding_y) = self.effective_terminal_padding();
        let (content_padding_x, content_padding_y) = self.native_split_content_padding();
        let cell_width: f32 = size.cell_width.into();
        let cell_height: f32 = size.cell_height.into();
        if cell_width <= f32::EPSILON || cell_height <= f32::EPSILON {
            return None;
        }

        Some(TerminalViewportGeometry {
            origin_x: padding_x + (f32::from(pane.left) * cell_width) + content_padding_x,
            origin_y: padding_y + (f32::from(pane.top) * cell_height) + content_padding_y,
            width: cell_width * f32::from(size.cols),
            height: cell_height * f32::from(size.rows),
        })
    }

    pub(super) fn terminal_surface_geometry(
        &self,
        _window: &Window,
    ) -> Option<TerminalViewportGeometry> {
        self.terminal_viewport_geometry()
    }

    pub(super) fn agent_sidebar_visible(&self) -> bool {
        self.agent_sidebar_enabled && self.agent_sidebar_open
    }

    pub(super) fn clear_terminal_scrollbar_marker_cache(&mut self) {
        self.terminal_scrollbar_marker_cache.clear();
    }

    pub(super) fn clear_pane_render_caches(&self) {
        for tab in &self.tabs {
            for pane in &tab.panes {
                pane.render_cache.borrow_mut().clear();
            }
        }
    }

    pub(super) fn mark_terminal_scrollbar_activity(&mut self, cx: &mut Context<Self>) {
        if self.terminal_scrollbar_mode() != ScrollbarVisibilityMode::OnScroll {
            return;
        }

        self.terminal_scrollbar_visibility_controller
            .mark_activity(Instant::now());
        self.start_terminal_scrollbar_animation(cx);
    }

    pub(super) fn start_terminal_scrollbar_drag(
        &mut self,
        thumb_grab_offset: f32,
        cx: &mut Context<Self>,
    ) {
        self.terminal_scrollbar_track_hold_local_y = None;
        self.terminal_scrollbar_drag = Some(TerminalScrollbarDragState { thumb_grab_offset });
        self.terminal_scrollbar_visibility_controller
            .start_drag(Instant::now());
        self.start_terminal_scrollbar_animation(cx);
    }

    pub(super) fn finish_terminal_scrollbar_drag(&mut self, cx: &mut Context<Self>) -> bool {
        if self.terminal_scrollbar_drag.take().is_none() {
            return false;
        }

        self.terminal_scrollbar_visibility_controller
            .end_drag(Instant::now());
        self.start_terminal_scrollbar_animation(cx);
        true
    }

    pub(super) fn start_terminal_scrollbar_track_hold(
        &mut self,
        local_y: f32,
        cx: &mut Context<Self>,
    ) {
        self.terminal_scrollbar_track_hold_local_y = Some(local_y);
        self.mark_terminal_scrollbar_activity(cx);
        if self.terminal_scrollbar_track_hold_active {
            return;
        }

        self.terminal_scrollbar_track_hold_active = true;
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                smol::Timer::after(Duration::from_millis(
                    TERMINAL_SCROLLBAR_TRACK_HOLD_REPEAT_MS,
                ))
                .await;
                let mut keep_running = false;
                let result = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        keep_running = view.handle_terminal_scrollbar_track_hold_tick(cx);
                        if !keep_running {
                            view.terminal_scrollbar_track_hold_active = false;
                        }
                    })
                });

                if result.is_err() || !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn update_terminal_scrollbar_track_hold(&mut self, local_y: f32) {
        self.terminal_scrollbar_track_hold_local_y = Some(local_y);
    }

    pub(super) fn stop_terminal_scrollbar_track_hold(&mut self) -> bool {
        self.terminal_scrollbar_track_hold_local_y.take().is_some()
    }

    fn terminal_scrollbar_needs_animation(&self, now: Instant) -> bool {
        self.terminal_scrollbar_visibility_controller
            .needs_animation(
                self.terminal_scrollbar_mode(),
                now,
                TERMINAL_SCROLLBAR_HOLD_DURATION,
                TERMINAL_SCROLLBAR_FADE_DURATION,
            )
    }

    fn start_terminal_scrollbar_animation(&mut self, cx: &mut Context<Self>) {
        if self.terminal_scrollbar_animation_active
            || self.terminal_scrollbar_mode() != ScrollbarVisibilityMode::OnScroll
            || !self.terminal_scrollbar_needs_animation(Instant::now())
        {
            return;
        }

        self.terminal_scrollbar_animation_active = true;
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                smol::Timer::after(Duration::from_millis(16)).await;

                let mut keep_running = false;
                let result = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        keep_running = view.terminal_scrollbar_needs_animation(Instant::now());
                        if !keep_running {
                            view.terminal_scrollbar_animation_active = false;
                        }
                        cx.notify();
                    })
                });

                if result.is_err() || !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    fn sync_window_background_appearance(&mut self, window: &mut Window) {
        let resolved = resolve_background_appearance(
            self.effective_background_opacity(),
            self.background_blur,
            self.background_support_context,
        );

        if self.last_window_background_appearance != Some(resolved.appearance) {
            window.set_background_appearance(resolved.appearance);
            self.last_window_background_appearance = Some(resolved.appearance);
        }

        if self.background_blur
            && resolved.blur_fallback == BlurFallbackReason::KnownUnsupported
            && !self.warned_blur_unsupported_once
        {
            self.warned_blur_unsupported_once = true;
            termy_toast::warning(
                "Background blur is unsupported in this session; using transparency",
            );
        }
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>, config: AppConfig) -> Self {
        let focus_handle = cx.focus_handle();
        let blur_focus_handle = focus_handle.clone();
        let (event_wakeup_tx, event_wakeup_rx) = bounded(1);
        let config_change_rx = config::subscribe_config_changes();
        let background_opacity_preview_rx = config::subscribe_background_opacity_preview();
        #[cfg(test)]
        let _ = &config_change_rx;
        #[cfg(test)]
        let _ = &background_opacity_preview_rx;

        // Focus the terminal immediately
        focus_handle.focus(window, cx);

        // Process terminal events only when runtimes signal activity.
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            while event_wakeup_rx.recv_async().await.is_ok() {
                while event_wakeup_rx.try_recv().is_ok() {}
                let result = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        if view.process_terminal_events(cx) {
                            cx.notify();
                        }
                    })
                });
                if result.is_err() {
                    break;
                }
            }
        })
        .detach();

        #[cfg(not(test))]
        {
            // Reload immediately when config is updated in-process (e.g. settings/theme actions).
            cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                loop {
                    let wait_rx = config_change_rx.clone();
                    if smol::unblock(move || wait_rx.recv()).await.is_err() {
                        break;
                    }
                    while config_change_rx.try_recv().is_ok() {}
                    let result = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            view.reload_config(cx);
                            cx.notify();
                        })
                    });
                    if result.is_err() {
                        break;
                    }
                }
            })
            .detach();
        }

        #[cfg(not(test))]
        {
            // Poll config file timestamp and hot-reload UI settings on change.
            cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                loop {
                    smol::Timer::after(Duration::from_millis(CONFIG_WATCH_INTERVAL_MS)).await;
                    let result = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            let config_changed = view.reload_config_if_changed(cx);
                            let availability_changed = view.refresh_install_cli_availability();
                            if availability_changed {
                                view.refresh_command_palette_items_for_current_mode(cx);
                                cx.set_menus(crate::menus::app_menus(
                                    view.install_cli_available(),
                                    view.runtime_uses_tmux(),
                                ));
                            }
                            if config_changed || availability_changed {
                                cx.notify();
                            }
                        })
                    });
                    if result.is_err() {
                        break;
                    }
                }
            })
            .detach();
        }

        // Toggle cursor visibility for blink in both terminal and inline inputs.
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                smol::Timer::after(Duration::from_millis(CURSOR_BLINK_INTERVAL_MS)).await;
                let result = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        if view.tick_cursor_blink() {
                            cx.notify();
                        }
                    })
                });
                if result.is_err() {
                    break;
                }
            }
        })
        .detach();

        let mut last_config_error_message = None;
        let config_path = match config::ensure_config_file() {
            Ok(path) => Some(path),
            Err(error) => {
                config::report_config_error_once(
                    &mut last_config_error_message,
                    "Failed to resolve config path for terminal view",
                    &error,
                );
                None
            }
        };
        let config_fingerprint = config_path.as_deref().and_then(config::config_fingerprint);
        let theme_id = config.theme.clone();
        let colors = TerminalColors::from_theme(&config.theme, &config.colors);
        let base_font_size = config.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        let padding_x = config.padding_x.max(0.0);
        let padding_y = config.padding_y.max(0.0);
        let background_support_context = BackgroundSupportContext::current();
        let configured_working_dir = config.working_dir.clone();
        let tab_title = config.tab_title.clone();
        let tab_shell_integration = TabTitleShellIntegration {
            enabled: tab_title.shell_integration,
            explicit_prefix: tab_title.explicit_prefix.clone(),
        };
        let terminal_runtime = Self::runtime_config_from_app_config(&config);
        let predicted_prompt_cwd = Self::predicted_prompt_cwd(
            configured_working_dir.as_deref(),
            terminal_runtime.working_dir_fallback,
        );
        let startup_predicted_title =
            Self::predicted_prompt_seed_title(&tab_title, predicted_prompt_cwd.as_deref());
        let initial_cols = TerminalSize::default().cols;
        let initial_rows = TerminalSize::default().rows;
        let (runtime, initial_snapshot, native_terminal) = Self::runtime_startup_from_app_config(
            &config,
            &event_wakeup_tx,
            configured_working_dir.as_deref(),
            &tab_shell_integration,
            &terminal_runtime,
            initial_cols,
            initial_rows,
        );
        let resolved_runtime_kind = runtime.kind();

        let mut view = Self {
            tabs: Vec::new(),
            native_pane_zoom_snapshots: HashMap::new(),
            next_tab_id: 1,
            active_tab: 0,
            renaming_tab: None,
            rename_input: InlineInputState::new(String::new()),
            event_wakeup_tx,
            focus_handle,
            theme_id,
            colors,
            inactive_tab_scrollback: config.inactive_tab_scrollback,
            tasks: config.tasks.clone(),
            warn_on_quit: config.warn_on_quit,
            warn_on_quit_with_running_process: config.warn_on_quit_with_running_process,
            tab_title,
            tab_close_visibility: config.tab_close_visibility,
            tab_width_mode: config.tab_width_mode,
            vertical_tabs: config.vertical_tabs,
            vertical_tabs_width: config
                .vertical_tabs_width
                .clamp(VERTICAL_TAB_STRIP_MIN_WIDTH, VERTICAL_TAB_STRIP_MAX_WIDTH),
            vertical_tabs_minimized: config.vertical_tabs_minimized,
            show_termy_in_titlebar: config.show_termy_in_titlebar,
            tab_shell_integration,
            configured_working_dir,
            child_working_dir_cache: HashMap::new(),
            child_working_dir_lookup_pending: HashSet::new(),
            terminal_runtime,
            runtime,
            tmux_enabled_config: config.tmux_enabled,
            native_tab_persistence: config.native_tab_persistence,
            native_layout_autosave: config.native_layout_autosave,
            native_buffer_persistence: config.native_buffer_persistence,
            agent_sidebar_enabled: config.agent_sidebar_enabled,
            agent_sidebar_width: config
                .agent_sidebar_width
                .clamp(AGENT_SIDEBAR_MIN_WIDTH, AGENT_SIDEBAR_MAX_WIDTH),
            agent_sidebar_open: false,
            agent_sidebar_input_active: false,
            agent_sidebar_input: InlineInputState::new(String::new()),
            current_named_layout: None,
            native_persist_revision: Arc::new(AtomicU64::new(0)),
            tmux_show_active_pane_border: config.tmux_show_active_pane_border,
            config_path,
            config_fingerprint,
            last_config_error_message,
            cached_tmux_binary: {
                let binary = config.tmux_binary.trim().to_string();
                (!binary.is_empty()).then_some(binary)
            },
            font_family: config.font_family.into(),
            base_font_size,
            font_size: px(base_font_size),
            cursor_style: config.cursor_style,
            cursor_blink: config.cursor_blink,
            cursor_blink_visible: true,
            background_opacity: config.background_opacity,
            preview_background_opacity: config::current_background_opacity_preview(),
            background_blur: config.background_blur,
            background_support_context,
            last_window_background_appearance: None,
            warned_blur_unsupported_once: false,
            padding_x,
            padding_y,
            mouse_scroll_multiplier: config.mouse_scroll_multiplier,
            pane_focus_effect: config.pane_focus_effect,
            pane_focus_strength: config.pane_focus_strength,
            line_height: 1.4,
            copy_on_select: config.copy_on_select,
            copy_on_select_toast: config.copy_on_select_toast,
            selection_anchor: None,
            selection_head: None,
            selection_dragging: false,
            selection_moved: false,
            pending_cursor_move_click: None,
            pending_cursor_move_preview: None,
            terminal_context_menu: None,
            hovered_link: None,
            hovered_toast: None,
            copied_toast_feedback: None,
            toast_animation_scheduled: false,
            toast_manager: ToastManager::new(),
            overlay_view: None,
            command_palette: CommandPaletteState::new(config.command_palette_show_keybinds),
            last_viewport_size_px: None,
            resize_indicator_dims: None,
            resize_indicator_visible_until: None,
            resize_indicator_animation_scheduled: false,
            alt_screen_refresh_scheduled: false,
            show_debug_overlay: config.show_debug_overlay,
            debug_overlay_stats: DebugOverlayStats::new(),
            install_cli_available: Self::install_cli_available_from_system(),
            tab_strip: TabStripState::new(config.tab_switch_modifier_hints),
            inline_input_selecting: false,
            mouse_reporting: MouseReportingState::default(),
            terminal_scroll_accumulator_y: 0.0,
            input_scroll_suppress_until: None,
            last_tmux_resize_error_at: None,
            terminal_scrollbar_visibility: config.terminal_scrollbar_visibility,
            terminal_scrollbar_style: config.terminal_scrollbar_style,
            terminal_scrollbar_visibility_controller: ScrollbarVisibilityController::default(),
            terminal_scrollbar_animation_active: false,
            terminal_scrollbar_drag: None,
            terminal_scrollbar_track_hold_local_y: None,
            terminal_scrollbar_track_hold_active: false,
            pane_resize_drag: None,
            agent_sidebar_resize_drag: None,
            vertical_tab_strip_resize_drag: None,
            terminal_scrollbar_marker_cache: TerminalScrollbarMarkerCache::default(),
            cell_size: None,
            search_open: false,
            search_input: InlineInputState::new(String::new()),
            search_state: SearchState::new(),
            search_debounce_token: 0,
            ai_input_open: false,
            ai_input: InlineInputState::new(String::new()),
            ime_marked_text: None,
            ime_selected_range: None,
            pending_clipboard: None,
            #[cfg(debug_assertions)]
            render_metrics: TerminalRenderMetricsState::from_env(),
            quit_prompt_in_flight: false,
            allow_quit_without_prompt: false,
            #[cfg(target_os = "macos")]
            auto_updater: None,
            #[cfg(target_os = "macos")]
            show_update_banner: false,
            #[cfg(target_os = "macos")]
            last_notified_update_state: None,
            #[cfg(target_os = "macos")]
            update_check_toast_id: None,
        };
        #[cfg(target_os = "windows")]
        if config.tmux_enabled {
            // Surface explicit feedback when a synced/shared config requests tmux on Windows.
            termy_toast::warning(TMUX_UNSUPPORTED_WINDOWS_TOAST);
        }
        let restored_native_workspace = if resolved_runtime_kind == RuntimeKind::Native {
            match view.restore_persisted_native_workspace(cx) {
                Ok(restored) => restored,
                Err(error) => {
                    log::error!("Failed to restore native tab workspace: {}", error);
                    termy_toast::error("Failed to restore saved native tabs");
                    false
                }
            }
        } else {
            false
        };

        match initial_snapshot {
            Some(initial_snapshot) => view.apply_tmux_snapshot(initial_snapshot),
            None => {
                if !restored_native_workspace && let Some(native_terminal) = native_terminal {
                    let tab_id = view.allocate_tab_id();
                    view.tabs = vec![Self::create_native_tab(
                        tab_id,
                        native_terminal,
                        initial_cols,
                        initial_rows,
                        startup_predicted_title.clone(),
                    )];
                    view.active_tab = 0;
                    view.refresh_tab_title(0);
                    view.mark_tab_strip_layout_dirty();
                }
            }
        }
        cx.observe_window_activation(window, |view, window, cx| {
            if !window.is_window_active() && view.release_all_forwarded_mouse_presses() {
                cx.notify();
            }
        })
        .detach();
        cx.on_blur(&blur_focus_handle, window, |view, _window, cx| {
            let released_mouse_presses = view.release_all_forwarded_mouse_presses();
            let cleared_tab_switch_hint_state = view.tab_strip.switch_hints.reset_hold_state();
            let dismissed_context_menu = view.close_terminal_context_menu(cx);
            if released_mouse_presses || cleared_tab_switch_hint_state || dismissed_context_menu {
                cx.notify();
            }
        })
        .detach();

        #[cfg(not(test))]
        {
            cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                loop {
                    let wait_rx = background_opacity_preview_rx.clone();
                    let Ok(mut opacity) = smol::unblock(move || wait_rx.recv()).await else {
                        break;
                    };
                    while let Ok(next_opacity) = background_opacity_preview_rx.try_recv() {
                        opacity = next_opacity;
                    }
                    let result = cx.update(|cx| {
                        this.update(cx, |view, cx| {
                            if view.preview_background_opacity != opacity {
                                view.preview_background_opacity = opacity;
                                cx.notify();
                            }
                        })
                    });
                    if result.is_err() {
                        break;
                    }
                }
            })
            .detach();
        }

        #[cfg(target_os = "macos")]
        if config.auto_update {
            let updater = cx.new(|_| AutoUpdater::new(crate::APP_VERSION));
            cx.observe(&updater, |view, updater, cx| {
                let state = updater.read(cx).state.clone();
                view.sync_update_toasts(Some(&state));
                let was_banner_visible = view.show_update_banner;
                view.show_update_banner = Self::overlay_banner_visible_for_state(Some(&state));
                view.notify_overlay(cx);
                if view.show_update_banner != was_banner_visible {
                    cx.notify();
                }
            })
            .detach();
            let weak = updater.downgrade();
            cx.spawn(async move |_this: WeakEntity<Self>, cx: &mut AsyncApp| {
                smol::Timer::after(Duration::from_millis(5000)).await;
                cx.update(|cx| AutoUpdater::check(weak, cx));
            })
            .detach();
            view.auto_updater = Some(updater);
        }

        view
    }

    fn apply_runtime_config(&mut self, config: AppConfig, cx: &mut Context<Self>) -> bool {
        keybindings::install_keybindings(cx, &config, self.runtime_uses_tmux());
        self.cached_tmux_binary = {
            let binary = config.tmux_binary.trim().to_string();
            (!binary.is_empty()).then_some(binary)
        };
        let previous_theme_id = self.theme_id.clone();
        let previous_font_family = self.font_family.clone();
        let previous_font_size = self.font_size;
        self.theme_id = config.theme.clone();
        self.colors = TerminalColors::from_theme(&config.theme, &config.colors);
        self.inactive_tab_scrollback = config.inactive_tab_scrollback;
        self.tasks = config.tasks.clone();
        self.warn_on_quit = config.warn_on_quit;
        self.warn_on_quit_with_running_process = config.warn_on_quit_with_running_process;
        self.tab_title = config.tab_title.clone();
        let tab_close_visibility_changed = self.tab_close_visibility != config.tab_close_visibility;
        let tab_width_mode_changed = self.tab_width_mode != config.tab_width_mode;
        let vertical_tabs_changed = self.vertical_tabs != config.vertical_tabs;
        let vertical_tabs_width = config
            .vertical_tabs_width
            .clamp(VERTICAL_TAB_STRIP_MIN_WIDTH, VERTICAL_TAB_STRIP_MAX_WIDTH);
        let vertical_tabs_width_changed =
            (self.vertical_tabs_width - vertical_tabs_width).abs() > f32::EPSILON;
        let vertical_tabs_minimized_changed =
            self.vertical_tabs_minimized != config.vertical_tabs_minimized;
        let tab_switch_modifier_hints_changed = self
            .tab_strip
            .switch_hints
            .sync_enabled(config.tab_switch_modifier_hints);
        let show_termy_in_titlebar_changed =
            self.show_termy_in_titlebar != config.show_termy_in_titlebar;
        let show_debug_overlay_changed = self.show_debug_overlay != config.show_debug_overlay;
        if self.theme_id != previous_theme_id {
            crate::plugins::emit_plugin_event(termy_plugin_core::HostEvent::ThemeChanged {
                theme_id: self.theme_id.clone(),
            });
        }
        self.tab_close_visibility = config.tab_close_visibility;
        self.tab_width_mode = config.tab_width_mode;
        self.vertical_tabs = config.vertical_tabs;
        self.vertical_tabs_width = vertical_tabs_width;
        self.vertical_tabs_minimized = config.vertical_tabs_minimized;
        self.show_termy_in_titlebar = config.show_termy_in_titlebar;
        self.show_debug_overlay = config.show_debug_overlay;
        self.tab_shell_integration = TabTitleShellIntegration {
            enabled: self.tab_title.shell_integration,
            explicit_prefix: self.tab_title.explicit_prefix.clone(),
        };
        #[cfg(target_os = "windows")]
        if !self.tmux_enabled_config && config.tmux_enabled {
            // Keep this visible on config reload so users understand why runtime did not switch.
            termy_toast::warning(TMUX_UNSUPPORTED_WINDOWS_TOAST);
        }
        #[cfg(not(target_os = "windows"))]
        let next_runtime_kind = Self::runtime_kind_from_app_config(&config);
        #[cfg(not(target_os = "windows"))]
        let tmux_enabled_changed = config.tmux_enabled != self.tmux_enabled_config;
        #[cfg(not(target_os = "windows"))]
        if next_runtime_kind != self.runtime_kind() && tmux_enabled_changed {
            termy_toast::info(
                "tmux startup default saved. Use Tmux Sessions to switch runtime now.",
            );
        }
        self.tmux_enabled_config = config.tmux_enabled;
        let native_tab_persistence_changed =
            self.native_tab_persistence != config.native_tab_persistence;
        let native_layout_autosave_changed =
            self.native_layout_autosave != config.native_layout_autosave;
        let native_buffer_persistence_changed =
            self.native_buffer_persistence != config.native_buffer_persistence;
        let agent_sidebar_enabled_changed =
            self.agent_sidebar_enabled != config.agent_sidebar_enabled;
        let clamped_agent_sidebar_width = config
            .agent_sidebar_width
            .clamp(AGENT_SIDEBAR_MIN_WIDTH, AGENT_SIDEBAR_MAX_WIDTH);
        let agent_sidebar_width_changed =
            (self.agent_sidebar_width - clamped_agent_sidebar_width).abs() > f32::EPSILON;
        self.native_tab_persistence = config.native_tab_persistence;
        self.native_layout_autosave = config.native_layout_autosave;
        self.native_buffer_persistence = config.native_buffer_persistence;
        self.agent_sidebar_enabled = config.agent_sidebar_enabled;
        self.agent_sidebar_width = clamped_agent_sidebar_width;
        if !self.agent_sidebar_enabled {
            self.agent_sidebar_open = false;
        }
        self.tmux_show_active_pane_border = config.tmux_show_active_pane_border;
        self.configured_working_dir = config.working_dir.clone();
        self.terminal_runtime = Self::runtime_config_from_app_config(&config);
        if native_tab_persistence_changed
            || native_layout_autosave_changed
            || native_buffer_persistence_changed
        {
            if native_tab_persistence_changed && !self.native_tab_persistence {
                if let Err(error) = self.clear_persisted_native_workspace() {
                    log::error!("Failed to clear saved native tab workspace: {}", error);
                }
            }
            if native_buffer_persistence_changed && !self.native_buffer_persistence {
                if let Err(error) = self.rewrite_persisted_native_workspace_without_buffers() {
                    log::error!(
                        "Failed to rewrite saved native tab workspace without buffers: {}",
                        error
                    );
                }
            }
            if self.native_tab_persistence
                || self.native_layout_autosave
                || self.native_buffer_persistence
            {
                self.sync_persisted_native_workspace();
            }
        }
        if agent_sidebar_enabled_changed || agent_sidebar_width_changed {
            self.clear_pane_render_caches();
            self.clear_terminal_scrollbar_marker_cache();
            self.cell_size = None;
        }
        if vertical_tabs_changed || vertical_tabs_width_changed || vertical_tabs_minimized_changed {
            self.clear_pane_render_caches();
            self.clear_terminal_scrollbar_marker_cache();
            self.cell_size = None;
        }
        let reconnect_managed_tmux = self.runtime_uses_tmux()
            && matches!(
                self.tmux_runtime().config.launch,
                TmuxLaunchTarget::Managed { .. }
            );
        if reconnect_managed_tmux {
            self.reconnect_tmux_runtime(Self::tmux_runtime_from_app_config(&config));
        } else if self.runtime_uses_tmux() {
            // Session-attached runtime keeps its explicit launch target across config reloads.
            // Only update the binary path used for external tmux command invocations.
            self.tmux_runtime_mut().config.binary = config.tmux_binary.trim().to_string();
        }
        self.font_family = config.font_family.into();
        self.base_font_size = config.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        self.font_size = px(self.base_font_size);
        self.cursor_style = config.cursor_style;
        self.cursor_blink = config.cursor_blink;
        self.cursor_blink_visible = true;
        self.cell_size = None;
        if self.font_family != previous_font_family || self.font_size != previous_font_size {
            self.clear_tab_title_width_cache();
            self.mark_tab_strip_layout_dirty();
        }
        self.background_opacity = config.background_opacity;
        self.preview_background_opacity = config::synced_background_opacity_preview(
            self.background_opacity,
            self.preview_background_opacity,
        );
        self.background_blur = config.background_blur;
        self.padding_x = config.padding_x.max(0.0);
        self.padding_y = config.padding_y.max(0.0);
        self.copy_on_select = config.copy_on_select;
        self.copy_on_select_toast = config.copy_on_select_toast;
        self.mouse_scroll_multiplier = config.mouse_scroll_multiplier;
        self.pane_focus_effect = config.pane_focus_effect;
        self.pane_focus_strength = config.pane_focus_strength;
        if self.terminal_scrollbar_visibility != config.terminal_scrollbar_visibility {
            self.terminal_scrollbar_visibility = config.terminal_scrollbar_visibility;
            self.terminal_scrollbar_visibility_controller.reset();
            self.terminal_scrollbar_drag = None;
            self.terminal_scrollbar_track_hold_local_y = None;
            self.terminal_scrollbar_track_hold_active = false;
            self.terminal_scrollbar_animation_active = false;
            self.clear_terminal_scrollbar_marker_cache();
        }
        self.terminal_scrollbar_style = config.terminal_scrollbar_style;
        self.set_command_palette_show_keybinds(config.command_palette_show_keybinds);
        if show_debug_overlay_changed {
            self.debug_overlay_stats.reset();
            self.notify_overlay(cx);
            cx.notify();
        }
        self.clear_pane_render_caches();
        let inactive_history = self
            .inactive_tab_scrollback
            .unwrap_or(self.terminal_runtime.scrollback_history);
        let active_options = self.terminal_runtime.term_options();
        let inactive_options = (inactive_history != active_options.scrollback_history)
            .then(|| active_options.with_scrollback_history(inactive_history));
        for (tab_index, tab) in self.tabs.iter().enumerate() {
            let options = if tab_index == self.active_tab {
                active_options
            } else {
                inactive_options.unwrap_or(active_options)
            };
            for pane in &tab.panes {
                pane.terminal.set_term_options(options);
            }
        }

        for index in 0..self.tabs.len() {
            self.refresh_tab_title(index);
        }
        if tab_close_visibility_changed
            || tab_width_mode_changed
            || vertical_tabs_changed
            || vertical_tabs_width_changed
            || vertical_tabs_minimized_changed
            || show_termy_in_titlebar_changed
        {
            self.mark_tab_strip_layout_dirty();
        }
        if tab_switch_modifier_hints_changed {
            cx.notify();
        }

        if self.is_command_palette_open() {
            self.refresh_command_palette_matches(true, cx);
        }

        true
    }

    fn emit_active_tab_changed_plugin_event(&self) {
        let Some(tab) = self.tabs.get(self.active_tab) else {
            return;
        };

        crate::plugins::emit_plugin_event(termy_plugin_core::HostEvent::ActiveTabChanged {
            tab_index: self.active_tab,
            tab_title: tab.title.clone(),
        });
    }

    #[cfg(not(test))]
    fn reload_config_if_changed(&mut self, cx: &mut Context<Self>) -> bool {
        let path = match self.config_path.clone() {
            Some(path) => path,
            None => {
                let loaded = config::load_runtime_config(
                    &mut self.last_config_error_message,
                    "Failed to reload config for terminal view",
                );
                self.config_path = loaded.path;
                self.config_fingerprint = loaded.fingerprint;
                if loaded.loaded_from_disk {
                    let changed = self.apply_runtime_config(loaded.config, cx);
                    if changed {
                        termy_toast::info("Configuration reloaded");
                    }
                    return changed;
                }
                return false;
            }
        };

        let Some(fingerprint) = config::config_fingerprint(&path) else {
            return false;
        };

        if self.config_fingerprint == Some(fingerprint) {
            return false;
        }

        let loaded = config::load_runtime_config(
            &mut self.last_config_error_message,
            "Failed to reload config for terminal view",
        );
        self.config_path = loaded.path;
        self.config_fingerprint = loaded.fingerprint;
        if loaded.loaded_from_disk {
            let changed = self.apply_runtime_config(loaded.config, cx);
            if changed {
                termy_toast::info("Configuration reloaded");
            }
            changed
        } else {
            false
        }
    }

    pub(super) fn reload_config(&mut self, cx: &mut Context<Self>) {
        let loaded = config::load_runtime_config(
            &mut self.last_config_error_message,
            "Failed to reload config for terminal view",
        );
        self.config_path = loaded.path;
        self.config_fingerprint = loaded.fingerprint;
        if loaded.loaded_from_disk {
            self.apply_runtime_config(loaded.config, cx);
        }
    }

    pub(super) fn persist_theme_selection(
        &mut self,
        theme_id: &str,
        cx: &mut Context<Self>,
    ) -> Result<bool, String> {
        if theme_id == self.theme_id {
            return Ok(false);
        }

        config::set_theme_in_config(theme_id)?;
        self.reload_config(cx);
        Ok(true)
    }

    fn tick_cursor_blink(&mut self) -> bool {
        if !self.cursor_blink {
            if self.cursor_blink_visible {
                return false;
            }
            self.cursor_blink_visible = true;
            return true;
        }

        self.cursor_blink_visible = !self.cursor_blink_visible;
        true
    }

    pub(super) fn reset_cursor_blink_phase(&mut self) {
        self.cursor_blink_visible = true;
    }

    pub(super) fn cursor_visible_for_focus(&self, focused: bool) -> bool {
        !self.cursor_blink || !focused || self.cursor_blink_visible
    }

    pub(super) fn terminal_cursor_style(&self) -> TerminalCursorStyle {
        match self.cursor_style {
            AppCursorStyle::Line => TerminalCursorStyle::Line,
            AppCursorStyle::Block => TerminalCursorStyle::Block,
        }
    }

    fn process_terminal_events(&mut self, cx: &mut Context<Self>) -> bool {
        if self.runtime_uses_tmux() {
            self.process_tmux_terminal_events(cx)
        } else {
            self.process_native_terminal_events(cx)
        }
    }

    fn process_native_terminal_events(&mut self, cx: &mut Context<Self>) -> bool {
        let mut should_redraw = false;
        let mut should_quit = false;
        let active_tab = self.active_tab;

        for index in 0..self.tabs.len() {
            let active_pane_id = self.tabs[index].active_pane_id.clone();

            for pane_index in 0..self.tabs[index].panes.len() {
                let pane_id = self.tabs[index].panes[pane_index].id.clone();
                let pane_is_active = pane_id == active_pane_id;
                let events = self.tabs[index].panes[pane_index].terminal.process_events();

                for event in events {
                    match event {
                        TerminalEvent::Wakeup | TerminalEvent::Bell => {
                            if index == active_tab {
                                should_redraw = true;
                            }
                        }
                        TerminalEvent::Exit => {
                            if Self::native_exit_should_quit_app(
                                self.tabs.len(),
                                self.tabs[index].panes.len(),
                            ) {
                                should_quit = true;
                            }
                            if index == active_tab {
                                should_redraw = true;
                            }
                        }
                        TerminalEvent::Title(title) => {
                            if pane_is_active && self.apply_terminal_title(index, &title, cx) {
                                should_redraw = true;
                            }
                        }
                        TerminalEvent::ResetTitle => {
                            if pane_is_active && self.clear_terminal_titles(index) {
                                should_redraw = true;
                            }
                        }
                        TerminalEvent::ClipboardStore(text) => {
                            if index == active_tab && pane_is_active {
                                self.pending_clipboard = Some(text);
                                should_redraw = true;
                            }
                        }
                    }
                }
            }
        }

        if should_quit {
            // Shell `exit` in the last native pane should close the app immediately.
            self.sync_persisted_native_workspace();
            self.allow_quit_without_prompt = true;
            cx.quit();
        }

        should_redraw
    }

    fn native_exit_should_quit_app(tab_count: usize, pane_count: usize) -> bool {
        tab_count == 1 && pane_count == 1
    }

    fn clear_selection(&mut self) -> bool {
        let anchor_changed = self.selection_anchor.take().is_some();
        let head_changed = self.selection_head.take().is_some();
        let dragging_changed = std::mem::replace(&mut self.selection_dragging, false);
        let moved_changed = std::mem::replace(&mut self.selection_moved, false);
        anchor_changed || head_changed || dragging_changed || moved_changed
    }

    fn clear_hovered_link(&mut self) -> bool {
        if self.hovered_link.is_some() {
            self.hovered_link = None;
            true
        } else {
            false
        }
    }

    fn active_terminal(&self) -> Option<&Terminal> {
        self.tabs
            .get(self.active_tab)
            .and_then(TerminalTab::active_terminal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(debug_assertions)]
    #[test]
    fn render_metrics_env_parser_accepts_truthy_values() {
        assert!(TerminalRenderMetricsState::parse_env_flag("1"));
        assert!(TerminalRenderMetricsState::parse_env_flag("true"));
        assert!(TerminalRenderMetricsState::parse_env_flag("TRUE"));
        assert!(TerminalRenderMetricsState::parse_env_flag("yes"));
        assert!(TerminalRenderMetricsState::parse_env_flag("on"));
    }

    #[test]
    fn native_exit_quits_only_for_single_tab_single_pane() {
        assert!(TerminalView::native_exit_should_quit_app(1, 1));
        assert!(!TerminalView::native_exit_should_quit_app(1, 2));
        assert!(!TerminalView::native_exit_should_quit_app(2, 1));
        assert!(!TerminalView::native_exit_should_quit_app(0, 0));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn render_metrics_env_parser_rejects_empty_and_zero_values() {
        assert!(!TerminalRenderMetricsState::parse_env_flag(""));
        assert!(!TerminalRenderMetricsState::parse_env_flag("0"));
        assert!(!TerminalRenderMetricsState::parse_env_flag("false"));
    }

    #[cfg(debug_assertions)]
    #[test]
    fn terminal_pane_render_cache_clear_resets_paint_cache_state() {
        let mut cache = TerminalPaneRenderCache {
            cells: std::sync::Arc::new(vec![std::sync::Arc::new(vec![])]),
            cols: 120,
            rows: 40,
            display_offset: 4,
            key: Some(TerminalPaneRenderCacheKey {
                is_active_pane: true,
                alternate_screen_mode: false,
                selection_range: Some((
                    SelectionPos { line: 1, col: 1 },
                    SelectionPos { line: 1, col: 2 },
                )),
                search_results_revision: Some(7),
                search_position: Some((1, 1)),
                effective_background_opacity_bits: 0.92f32.to_bits(),
                color_transform: TerminalPaneCellColorTransformKey {
                    fg_blend_bits: 0.1f32.to_bits(),
                    bg_blend_bits: 0.2f32.to_bits(),
                    desaturate_bits: 0.3f32.to_bits(),
                },
            }),
            paint_cache: TerminalGridPaintCacheHandle::default(),
        };
        cache.paint_cache.debug_seed_rows_for_tests(3);
        assert_eq!(cache.paint_cache.debug_row_cache_len_for_tests(), 3);

        cache.clear();

        assert!(cache.cells.is_empty());
        assert_eq!(cache.cols, 0);
        assert_eq!(cache.rows, 0);
        assert_eq!(cache.display_offset, 0);
        assert!(cache.key.is_none());
        assert_eq!(cache.paint_cache.debug_row_cache_len_for_tests(), 0);
    }

    #[test]
    fn resolve_background_appearance_is_opaque_when_opacity_is_full() {
        let resolved = resolve_background_appearance(
            1.0,
            true,
            BackgroundSupportContext {
                platform: BackgroundPlatform::MacOs,
                linux_wayland_session: false,
            },
        );
        assert_eq!(resolved.appearance, WindowBackgroundAppearance::Opaque);
        assert_eq!(resolved.blur_fallback, BlurFallbackReason::None);
    }

    #[test]
    fn resolve_background_appearance_is_transparent_without_blur() {
        let resolved = resolve_background_appearance(
            0.85,
            false,
            BackgroundSupportContext {
                platform: BackgroundPlatform::Windows,
                linux_wayland_session: false,
            },
        );
        assert_eq!(resolved.appearance, WindowBackgroundAppearance::Transparent);
        assert_eq!(resolved.blur_fallback, BlurFallbackReason::None);
    }

    #[test]
    fn resolve_background_appearance_blur_is_known_unsupported_on_linux_non_wayland() {
        let resolved = resolve_background_appearance(
            0.9,
            true,
            BackgroundSupportContext {
                platform: BackgroundPlatform::Linux,
                linux_wayland_session: false,
            },
        );
        assert_eq!(resolved.appearance, WindowBackgroundAppearance::Transparent);
        assert_eq!(resolved.blur_fallback, BlurFallbackReason::KnownUnsupported);
    }

    #[test]
    fn resolve_background_appearance_blur_is_unknown_on_linux_wayland() {
        let resolved = resolve_background_appearance(
            0.9,
            true,
            BackgroundSupportContext {
                platform: BackgroundPlatform::Linux,
                linux_wayland_session: true,
            },
        );
        assert_eq!(resolved.appearance, WindowBackgroundAppearance::Blurred);
        assert_eq!(resolved.blur_fallback, BlurFallbackReason::UnknownSupport);
    }

    #[test]
    fn resolve_background_appearance_blur_is_enabled_on_macos() {
        let resolved = resolve_background_appearance(
            0.9,
            true,
            BackgroundSupportContext {
                platform: BackgroundPlatform::MacOs,
                linux_wayland_session: false,
            },
        );
        assert_eq!(resolved.appearance, WindowBackgroundAppearance::Blurred);
        assert_eq!(resolved.blur_fallback, BlurFallbackReason::None);
    }

    #[test]
    fn chrome_alpha_scales_without_floor() {
        let base = 0.92;
        let alpha = scaled_chrome_alpha_for_opacity(base, 0.1);
        assert_eq!(alpha, base * 0.1);
    }

    #[test]
    fn overlay_panel_floor_applies_only_when_background_is_translucent() {
        let base = 0.64;
        let floor = 0.76;
        let translucent = adaptive_overlay_panel_alpha_with_floor_for_opacity(base, 0.2, floor);
        let opaque = adaptive_overlay_panel_alpha_with_floor_for_opacity(base, 1.0, floor);
        assert!(translucent >= floor);
        assert!(opaque < floor);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn overlay_banner_visibility_tracks_updater_state_policy() {
        assert!(!TerminalView::overlay_banner_visible_for_state(None));
        assert!(!TerminalView::overlay_banner_visible_for_state(Some(
            &UpdateState::Idle
        )));
        assert!(!TerminalView::overlay_banner_visible_for_state(Some(
            &UpdateState::Checking
        )));
        assert!(!TerminalView::overlay_banner_visible_for_state(Some(
            &UpdateState::UpToDate
        )));
        assert!(TerminalView::overlay_banner_visible_for_state(Some(
            &UpdateState::Available {
                version: "1.2.3".to_string(),
                url: "https://example.com/installer".to_string(),
                extension: "dmg".to_string(),
            }
        )));
        assert!(TerminalView::overlay_banner_visible_for_state(Some(
            &UpdateState::Downloading {
                version: "1.2.3".to_string(),
                downloaded: 5,
                total: 10,
            }
        )));
        assert!(TerminalView::overlay_banner_visible_for_state(Some(
            &UpdateState::Downloaded {
                version: "1.2.3".to_string(),
                installer_path: std::path::PathBuf::from("/tmp/termy-installer.dmg"),
            }
        )));
        assert!(TerminalView::overlay_banner_visible_for_state(Some(
            &UpdateState::Installing {
                version: "1.2.3".to_string(),
            }
        )));
        assert!(TerminalView::overlay_banner_visible_for_state(Some(
            &UpdateState::Installed {
                version: "1.2.3".to_string(),
            }
        )));
        assert!(TerminalView::overlay_banner_visible_for_state(Some(
            &UpdateState::Error("boom".to_string())
        )));
    }

    #[test]
    fn pane_divider_color_matches_shared_chrome_stroke_resolution() {
        let chrome_surface_bg = gpui::Rgba {
            r: 0.04,
            g: 0.08,
            b: 0.13,
            a: 0.94,
        };
        let foreground = gpui::Rgba {
            r: 0.82,
            g: 0.88,
            b: 0.93,
            a: 1.0,
        };

        assert_eq!(
            pane_divider_color(chrome_surface_bg, foreground),
            resolve_chrome_stroke_color(chrome_surface_bg, foreground, TAB_STROKE_FOREGROUND_MIX)
        );
    }

    #[test]
    fn pane_focus_preset_is_disabled_for_off() {
        assert!(pane_focus_preset(PaneFocusEffect::Off).is_none());
    }

    #[test]
    fn pane_focus_preset_strength_scales_monotonically() {
        let preset = pane_focus_preset(PaneFocusEffect::SoftSpotlight)
            .expect("soft spotlight preset should exist");
        let low_strength = pane_focus_strength_factor(0.2);
        let high_strength = pane_focus_strength_factor(0.8);

        assert!(
            (preset.inactive_fg_blend * high_strength) > (preset.inactive_fg_blend * low_strength)
        );
        assert!(
            (preset.inactive_bg_blend * high_strength) > (preset.inactive_bg_blend * low_strength)
        );
        assert!(
            (preset.active_border_alpha * high_strength)
                > (preset.active_border_alpha * low_strength)
        );
    }

    #[test]
    fn pane_focus_strength_factor_clamps_to_extended_upper_bound() {
        assert_eq!(pane_focus_strength_factor(2.5), MAX_PANE_FOCUS_STRENGTH);
    }

    #[test]
    fn install_cli_availability_is_inverse_of_installed_probe() {
        assert!(TerminalView::install_cli_availability_from_probe(false));
        assert!(!TerminalView::install_cli_availability_from_probe(true));
    }

    #[test]
    fn refresh_install_cli_availability_reports_state_changes() {
        let (next_available, changed) =
            TerminalView::refreshed_install_cli_availability(true, true);
        assert!(!next_available);
        assert!(changed);

        let (next_available, changed) =
            TerminalView::refreshed_install_cli_availability(false, true);
        assert!(!next_available);
        assert!(!changed);
    }

    #[test]
    fn runtime_kind_follows_tmux_enabled_flag() {
        let config = AppConfig {
            tmux_enabled: false,
            ..Default::default()
        };
        assert_eq!(
            TerminalView::runtime_kind_from_app_config(&config),
            RuntimeKind::Native
        );

        let config = AppConfig {
            tmux_enabled: true,
            ..Default::default()
        };
        #[cfg(not(target_os = "windows"))]
        assert_eq!(
            TerminalView::runtime_kind_from_app_config(&config),
            RuntimeKind::Tmux
        );
        #[cfg(target_os = "windows")]
        assert_eq!(
            TerminalView::runtime_kind_from_app_config(&config),
            RuntimeKind::Native
        );
    }

    #[test]
    fn tmux_runtime_uses_event_driven_wakeup_strategy() {
        assert!(TerminalView::uses_event_driven_tmux_wakeup());
    }

    #[test]
    fn create_native_tab_starts_with_one_full_size_pane() {
        let terminal = Terminal::new_tmux(
            TerminalSize::default(),
            TerminalOptions {
                scrollback_history: 2000,
                ..TerminalOptions::default()
            },
        );
        let tab = TerminalView::create_native_tab(7, terminal, 120, 42, None);

        assert_eq!(tab.panes.len(), 1);
        assert_eq!(tab.window_id, "@native-7");
        assert_eq!(tab.window_index, 0);
        assert_eq!(tab.active_pane_id, "%native-7");

        let pane = &tab.panes[0];
        assert_eq!(pane.id, "%native-7");
        assert_eq!(pane.left, 0);
        assert_eq!(pane.top, 0);
        assert_eq!(pane.width, 120);
        assert_eq!(pane.height, 42);
    }

    #[test]
    fn terminal_effective_background_opacity_prefers_preview() {
        assert_eq!(
            config::effective_background_opacity(
                0.9,
                Some(config::BackgroundOpacityPreview {
                    owner_id: 1,
                    opacity: 0.35,
                }),
            ),
            0.35
        );
        assert_eq!(config::effective_background_opacity(0.9, None), 0.9);
    }

    #[test]
    fn terminal_preview_clears_when_saved_matches() {
        assert_eq!(
            config::synced_background_opacity_preview(
                0.35,
                Some(config::BackgroundOpacityPreview {
                    owner_id: 1,
                    opacity: 0.35,
                }),
            ),
            None
        );
        assert_eq!(
            config::synced_background_opacity_preview(
                0.35,
                Some(config::BackgroundOpacityPreview {
                    owner_id: 1,
                    opacity: 0.5,
                }),
            ),
            Some(config::BackgroundOpacityPreview {
                owner_id: 1,
                opacity: 0.5,
            })
        );
    }

    #[test]
    fn resolve_background_appearance_uses_preview_opacity_during_drag() {
        let effective_opacity = config::effective_background_opacity(
            1.0,
            Some(config::BackgroundOpacityPreview {
                owner_id: 1,
                opacity: 0.4,
            }),
        );
        let resolved = resolve_background_appearance(
            effective_opacity,
            false,
            BackgroundSupportContext {
                platform: BackgroundPlatform::MacOs,
                linux_wayland_session: false,
            },
        );

        assert_eq!(resolved.appearance, WindowBackgroundAppearance::Transparent);
    }

    #[test]
    fn single_pane_layout_keeps_outer_terminal_padding() {
        assert!(TerminalView::uses_outer_terminal_padding(0));
        assert!(TerminalView::uses_outer_terminal_padding(1));
        assert!(!TerminalView::uses_outer_terminal_padding(2));
    }

    #[test]
    fn native_split_content_padding_is_only_used_for_native_multi_pane_tabs() {
        assert!(!TerminalView::uses_native_split_content_padding(false, 0));
        assert!(!TerminalView::uses_native_split_content_padding(false, 1));
        assert!(TerminalView::uses_native_split_content_padding(false, 2));
        assert!(!TerminalView::uses_native_split_content_padding(true, 2));
    }
}
