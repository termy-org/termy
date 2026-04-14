use crate::chrome_style::ChromeContrastProfile;
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
    Focusable, Font, FontWeight, InteractiveElement, IntoElement, KeyDownEvent, KeyUpEvent,
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
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use sysinfo::{ProcessesToUpdate, System, get_current_pid};
#[cfg(target_os = "macos")]
use termy_auto_update::{AutoUpdater, UpdateState};
use termy_config_core::{MAX_LINE_HEIGHT, MIN_LINE_HEIGHT};
use termy_search::SearchState;
#[cfg(debug_assertions)]
use termy_terminal_ui::terminal_ui_render_metrics_reset;
use termy_terminal_ui::{
    CellRenderInfo, CommandLifecycle, PaneTerminal, ProgressState, TabTitleShellIntegration,
    Terminal as NativeTerminal, TerminalClipboardTarget, TerminalCursorState, TerminalCursorStyle,
    TerminalDamageSnapshot, TerminalDirtySpan, TerminalEvent, TerminalGrid,
    TerminalGridPaintCacheHandle, TerminalGridPaintDamage, TerminalGridRows, TerminalKeyEventKind,
    TerminalKeyboardMode, TerminalMouseMode, TerminalOptions, TerminalQueryColors,
    TerminalReplyHost, TerminalRuntimeConfig, TerminalSize, TmuxLaunchTarget,
    WorkingDirFallback as RuntimeWorkingDirFallback, find_link_in_line, keystroke_to_input,
    normalize_working_directory_candidate, resolve_launch_working_directory,
};
use termy_terminal_ui::{TerminalUiRenderMetricsSnapshot, terminal_ui_render_metrics_snapshot};
use termy_toast::ToastManager;

#[cfg(not(target_os = "windows"))]
mod agents;
#[cfg(target_os = "windows")]
#[path = "agents_windows.rs"]
mod agents;
mod benchmark;
mod command_palette;
mod inline_input;
mod interaction;
#[cfg(target_os = "macos")]
mod macos_file_drop;
mod overlay_view;
mod persistence;
mod render;
mod runtime;
mod scrollbar;
mod search;
pub(crate) mod tab_strip;
mod tabs;
mod titles;
#[cfg(target_os = "macos")]
mod update_toasts;

use self::benchmark::{BENCHMARK_SAMPLE_INTERVAL, BenchmarkConfig, BenchmarkSession};
use command_palette::{CommandPaletteMode, CommandPaletteState, TmuxSessionIntent};
use inline_input::{InlineInputAlignment, InlineInputState};
#[cfg(target_os = "macos")]
pub(crate) use macos_file_drop::{NativeDropResult, install_native_file_drop};
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
const SEARCH_BAR_WIDTH: f32 = 280.0;
const SEARCH_BAR_HEIGHT: f32 = 40.0;
const SEARCH_DEBOUNCE_MS: u64 = 50;
const TMUX_RESIZE_ERROR_TOAST_DEBOUNCE_MS: u64 = 2000;
const DEBUG_OVERLAY_SAMPLE_INTERVAL: Duration = Duration::from_millis(500);
#[cfg(target_os = "windows")]
const TMUX_UNSUPPORTED_WINDOWS_TOAST: &str =
    "tmux integration is unsupported on Windows; using native runtime instead.";
const INPUT_SCROLL_SUPPRESS_MS: u64 = 160;
const TOAST_COPY_FEEDBACK_MS: u64 = 1200;
const WINDOW_RESIZE_INDICATOR_MS: u64 = 850;
const RESIZE_THROTTLE_MS: u64 = 32;
const CHILD_WORKING_DIR_CACHE_TTL: Duration = Duration::from_millis(CHILD_WORKING_DIR_CACHE_TTL_MS);
const BENCHMARK_EXIT_GRACE_DURATION: Duration = Duration::from_millis(250);
const OVERLAY_PANEL_ALPHA_FLOOR_RATIO: f32 = 0.72;
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
const SEARCH_BAR_BG_ALPHA: f32 = 0.92;
const SEARCH_INPUT_BG_ALPHA: f32 = 0.15;
const SEARCH_COUNTER_TEXT_ALPHA: f32 = 0.60;
const SEARCH_BUTTON_TEXT_ALPHA: f32 = 0.70;
const SEARCH_BUTTON_HOVER_BG_ALPHA: f32 = 0.20;
const SEARCH_INPUT_SELECTION_ALPHA: f32 = 0.30;
const TAB_SWITCH_HINT_ANIMATION_FRAME_MS: u64 = 16;
const NEW_TAB_ANIMATION_DURATION: Duration = Duration::from_millis(180);
const NEW_TAB_ANIMATION_FRAME_MS: u64 = 16;
const MAX_PANE_FOCUS_STRENGTH: f32 = 2.0;
const NATIVE_PANE_MIN_COLS: u16 = 24;
const NATIVE_PANE_MIN_ROWS: u16 = 8;
#[cfg(debug_assertions)]
const RENDER_METRICS_LOG_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq)]
struct TerminalOverlayGeometry {
    panel_radius: f32,
    input_radius: f32,
    control_radius: f32,
}

// Floating terminal chrome stays square to match the app's shared overlay language.
const TERMINAL_OVERLAY_GEOMETRY: TerminalOverlayGeometry = TerminalOverlayGeometry {
    panel_radius: 0.0,
    input_radius: 0.0,
    control_radius: 0.0,
};

// Search bar uses rounded corners for a native macOS feel.
const SEARCH_OVERLAY_GEOMETRY: TerminalOverlayGeometry = TerminalOverlayGeometry {
    panel_radius: 10.0,
    input_radius: 6.0,
    control_radius: 6.0,
};

// Toast notifications use rounded corners for a softer, modern look.
const TOAST_GEOMETRY: TerminalOverlayGeometry = TerminalOverlayGeometry {
    panel_radius: 10.0,
    input_radius: 6.0,
    control_radius: 6.0,
};

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
    height: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TerminalContentRect {
    origin_x: f32,
    origin_y: f32,
    width: f32,
    height: f32,
}

impl TerminalContentRect {
    fn new(origin_x: f32, origin_y: f32, width: f32, height: f32) -> Option<Self> {
        if width <= f32::EPSILON || height <= f32::EPSILON {
            return None;
        }

        Some(Self {
            origin_x,
            origin_y,
            width,
            height,
        })
    }

    fn right(self) -> f32 {
        self.origin_x + self.width
    }

    fn bottom(self) -> f32 {
        self.origin_y + self.height
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TerminalScrollbarSurfaceGeometry {
    origin_x: f32,
    origin_y: f32,
    width: f32,
    height: f32,
}

impl TerminalScrollbarSurfaceGeometry {
    fn new(origin_x: f32, origin_y: f32, width: f32, height: f32) -> Option<Self> {
        if width <= f32::EPSILON || height <= f32::EPSILON {
            return None;
        }

        Some(Self {
            origin_x,
            origin_y,
            width,
            height,
        })
    }

    fn gutter_frame(self) -> Option<TerminalScrollbarGutterFrame> {
        let gutter_width = TERMINAL_SCROLLBAR_GUTTER_WIDTH.min(self.width.max(0.0));
        if gutter_width <= f32::EPSILON {
            return None;
        }

        Some(TerminalScrollbarGutterFrame {
            left: (self.origin_x + self.width.max(0.0) - gutter_width).max(self.origin_x),
            top: self.origin_y,
            width: gutter_width,
            height: self.height,
        })
    }

    fn local_y(self, content_y: f32) -> Option<f32> {
        if content_y < self.origin_y || content_y > self.origin_y + self.height {
            return None;
        }

        Some(content_y - self.origin_y)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TerminalScrollbarGutterFrame {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct TerminalPaneNeighborGaps {
    right_cells: Option<u32>,
    bottom_cells: Option<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TerminalPaneLayout {
    frame: TerminalContentRect,
    content_frame: TerminalContentRect,
    scrollbar_surface: TerminalScrollbarSurfaceGeometry,
    cell_width: f32,
    cell_height: f32,
    extends_right_edge: bool,
    extends_bottom_edge: bool,
    gaps: TerminalPaneNeighborGaps,
}

fn cell_ranges_overlap(start_a: u32, end_a: u32, start_b: u32, end_b: u32) -> bool {
    start_a < end_b && start_b < end_a
}

#[derive(Clone, Copy, Debug)]
struct TerminalScrollbarDragState {
    thumb_grab_offset: f32,
}

#[derive(Clone, Copy, Debug)]
struct TerminalScrollbarTrackHoldState {
    local_y: f32,
    track_height: f32,
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct HoveredPaneDivider {
    pane_id: String,
    axis: PaneResizeAxis,
    edge: PaneResizeEdge,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaneResizeResult {
    Applied,
    BlockedByMinimum,
    NoChange,
}

#[derive(Clone, Copy, Debug)]
struct VerticalTabStripResizeDragState;

#[derive(Clone, Copy, Debug)]
struct AgentSidebarResizeDragState;

#[derive(Clone, Copy, Debug)]
struct AgentGitPanelResizeDragState;

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
}

#[derive(Clone, Debug, PartialEq)]
struct TabContextMenuState {
    anchor_position: gpui::Point<Pixels>,
    tab_id: TabId,
    pinned: bool,
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

#[derive(Clone, Debug, PartialEq, Eq)]
enum PendingKeyRelease {
    Consumed,
    Terminal { pane_id: String },
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

struct GpuiClipboardReplyHost {
    clipboard_text: Option<String>,
}

impl GpuiClipboardReplyHost {
    fn from_cx(cx: &mut Context<TerminalView>) -> Self {
        Self {
            clipboard_text: cx.read_from_clipboard().and_then(|item| item.text()),
        }
    }
}

impl TerminalReplyHost for GpuiClipboardReplyHost {
    fn load_clipboard(&mut self, _target: TerminalClipboardTarget) -> Option<String> {
        // GPUI exposes a single host clipboard source here, so both OSC 52
        // targets resolve through the same adapter.
        self.clipboard_text.clone()
    }
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
        startup_command: Option<&str>,
    ) -> anyhow::Result<Self> {
        Ok(Self::Native(Mutex::new(NativeTerminal::new(
            size,
            configured_working_dir,
            event_wakeup_tx,
            tab_title_shell_integration,
            runtime_config,
            startup_command,
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

    /// Drain pending events. Returns collected events and whether more remain.
    fn drain_events(&self, host: &mut impl TerminalReplyHost) -> (Vec<TerminalEvent>, bool) {
        match self {
            Self::Tmux(_) => (Vec::new(), false),
            Self::Native(terminal) => terminal
                .lock()
                .map(|terminal| terminal.drain_events(host))
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

    fn set_query_colors(&self, query_colors: TerminalQueryColors) {
        if let Self::Native(terminal) = self
            && let Ok(mut terminal) = terminal.lock()
        {
            terminal.set_query_colors(query_colors);
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

    fn keyboard_mode(&self) -> TerminalKeyboardMode {
        match self {
            Self::Tmux(terminal) => terminal.keyboard_mode(),
            Self::Native(terminal) => terminal
                .lock()
                .map(|terminal| terminal.keyboard_mode())
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
    pane_zoom_steps: i16,
    degraded: bool,
    terminal: Terminal,
    render_cache: RefCell<TerminalPaneRenderCache>,
    /// Tracks the previous alternate-screen state so that transitions can be
    /// detected during `sync_terminal_size` and a SIGWINCH nudge sent.
    last_alternate_screen: Cell<bool>,
    /// Pre-computed element IDs to avoid per-frame `format!()` allocations.
    cached_element_ids: PaneCachedElementIds,
}

/// Pre-computed GPUI element IDs for a terminal pane, avoiding `format!()`
/// string allocations on every render frame.
struct PaneCachedElementIds {
    pane: SharedString,
    resize_handle_right: SharedString,
    resize_handle_bottom: SharedString,
    focus_accent: SharedString,
    degraded_accent: SharedString,
}

impl PaneCachedElementIds {
    fn new(id: &str) -> Self {
        Self {
            pane: SharedString::from(format!("pane-{}", id)),
            resize_handle_right: SharedString::from(format!("pane-resize-handle-right-{}", id)),
            resize_handle_bottom: SharedString::from(format!("pane-resize-handle-bottom-{}", id)),
            focus_accent: SharedString::from(format!("pane-focus-accent-{}", id)),
            degraded_accent: SharedString::from(format!("pane-degraded-accent-{}", id)),
        }
    }
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
    background_opacity_cells: bool,
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

impl TerminalPane {
    fn new_native(
        id: String,
        left: u16,
        top: u16,
        width: u16,
        height: u16,
        terminal: Terminal,
    ) -> Self {
        let cached_element_ids = PaneCachedElementIds::new(&id);
        Self {
            id,
            left,
            top,
            width,
            height,
            pane_zoom_steps: 0,
            degraded: false,
            terminal,
            render_cache: RefCell::new(TerminalPaneRenderCache::default()),
            last_alternate_screen: Cell::new(false),
            cached_element_ids,
        }
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
    last_frame_at: Option<Instant>,
    frames_in_sample: u32,
    frame_interval_samples_micros: Vec<u32>,
    fps: f32,
    frame_p50_ms: f32,
    frame_p95_ms: f32,
    frame_p99_ms: f32,
    cpu_percent: f32,
    memory_bytes: u64,
    view_wake_signals: u64,
    terminal_event_drain_passes: u64,
    terminal_redraws: u64,
    alt_screen_fallback_redraws: u64,
    span_damage_ms: f32,
    span_rebuild_ms: f32,
    span_shaping_ms: f32,
    span_paint_ms: f32,
    span_snapshot_base: TerminalUiRenderMetricsSnapshot,
    #[cfg(debug_assertions)]
    runtime_wakeup_base: u64,
    #[cfg(debug_assertions)]
    runtime_wakeups: u64,
}

impl DebugOverlayStats {
    fn new() -> Self {
        #[cfg(debug_assertions)]
        let runtime_wakeup_base = terminal_ui_render_metrics_snapshot().runtime_wakeup_count;
        let mut stats = Self {
            system: System::new(),
            pid: get_current_pid().ok(),
            sample_started_at: Instant::now(),
            last_frame_at: None,
            frames_in_sample: 0,
            frame_interval_samples_micros: Vec::with_capacity(128),
            fps: 0.0,
            frame_p50_ms: 0.0,
            frame_p95_ms: 0.0,
            frame_p99_ms: 0.0,
            cpu_percent: 0.0,
            memory_bytes: 0,
            view_wake_signals: 0,
            terminal_event_drain_passes: 0,
            terminal_redraws: 0,
            alt_screen_fallback_redraws: 0,
            span_damage_ms: 0.0,
            span_rebuild_ms: 0.0,
            span_shaping_ms: 0.0,
            span_paint_ms: 0.0,
            span_snapshot_base: terminal_ui_render_metrics_snapshot(),
            #[cfg(debug_assertions)]
            runtime_wakeup_base,
            #[cfg(debug_assertions)]
            runtime_wakeups: 0,
        };
        stats.refresh_process_metrics();
        stats.refresh_runtime_wakeups();
        stats
    }

    fn reset(&mut self) {
        self.sample_started_at = Instant::now();
        self.last_frame_at = None;
        self.frames_in_sample = 0;
        self.frame_interval_samples_micros.clear();
        self.fps = 0.0;
        self.frame_p50_ms = 0.0;
        self.frame_p95_ms = 0.0;
        self.frame_p99_ms = 0.0;
        self.view_wake_signals = 0;
        self.terminal_event_drain_passes = 0;
        self.terminal_redraws = 0;
        self.alt_screen_fallback_redraws = 0;
        self.span_damage_ms = 0.0;
        self.span_rebuild_ms = 0.0;
        self.span_shaping_ms = 0.0;
        self.span_paint_ms = 0.0;
        self.span_snapshot_base = terminal_ui_render_metrics_snapshot();
        #[cfg(debug_assertions)]
        {
            self.runtime_wakeup_base = terminal_ui_render_metrics_snapshot().runtime_wakeup_count;
            self.runtime_wakeups = 0;
        }
        self.refresh_process_metrics();
    }

    fn record_frame(&mut self, now: Instant) {
        if let Some(previous_frame_at) = self.last_frame_at.replace(now) {
            let frame_interval = now.saturating_duration_since(previous_frame_at);
            let micros = frame_interval.as_micros().min(u128::from(u32::MAX)) as u32;
            self.frame_interval_samples_micros.push(micros);
        }
        self.frames_in_sample = self.frames_in_sample.saturating_add(1);
        let elapsed = now.saturating_duration_since(self.sample_started_at);
        if elapsed < DEBUG_OVERLAY_SAMPLE_INTERVAL {
            return;
        }

        let elapsed_secs = elapsed.as_secs_f32();
        if elapsed_secs > f32::EPSILON {
            self.fps = self.frames_in_sample as f32 / elapsed_secs;
        }
        self.refresh_frame_percentiles();
        self.refresh_span_timings();
        self.refresh_runtime_wakeups();
        self.sample_started_at = now;
        self.frames_in_sample = 0;
        self.frame_interval_samples_micros.clear();
        self.refresh_process_metrics();
    }

    fn record_view_wake_signal(&mut self) {
        self.view_wake_signals = self.view_wake_signals.saturating_add(1);
    }

    fn record_terminal_event_drain_pass(&mut self) {
        self.terminal_event_drain_passes = self.terminal_event_drain_passes.saturating_add(1);
    }

    fn record_terminal_redraw(&mut self) {
        self.terminal_redraws = self.terminal_redraws.saturating_add(1);
    }

    #[allow(dead_code)]
    fn record_alt_screen_fallback_redraw(&mut self) {
        self.alt_screen_fallback_redraws = self.alt_screen_fallback_redraws.saturating_add(1);
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

    fn refresh_frame_percentiles(&mut self) {
        if self.frame_interval_samples_micros.is_empty() {
            self.frame_p50_ms = 0.0;
            self.frame_p95_ms = 0.0;
            self.frame_p99_ms = 0.0;
            return;
        }

        let mut sorted_samples = self.frame_interval_samples_micros.clone();
        sorted_samples.sort_unstable();
        self.frame_p50_ms = percentile_millis(&sorted_samples, 50, 100);
        self.frame_p95_ms = percentile_millis(&sorted_samples, 95, 100);
        self.frame_p99_ms = percentile_millis(&sorted_samples, 99, 100);
    }

    fn refresh_span_timings(&mut self) {
        let current = terminal_ui_render_metrics_snapshot();
        let delta = current.saturating_sub(self.span_snapshot_base);
        self.span_snapshot_base = current;
        let frames = self.frames_in_sample.max(1) as f32;
        self.span_damage_ms = delta.span_damage_compute_us as f32 / 1000.0 / frames;
        self.span_rebuild_ms = delta.span_row_ops_rebuild_us as f32 / 1000.0 / frames;
        self.span_shaping_ms = delta.span_text_shaping_us as f32 / 1000.0 / frames;
        self.span_paint_ms = delta.span_grid_paint_us as f32 / 1000.0 / frames;
    }

    #[cfg(debug_assertions)]
    fn refresh_runtime_wakeups(&mut self) {
        let snapshot = terminal_ui_render_metrics_snapshot();
        self.runtime_wakeups = snapshot
            .runtime_wakeup_count
            .saturating_sub(self.runtime_wakeup_base);
    }

    #[cfg(not(debug_assertions))]
    fn refresh_runtime_wakeups(&mut self) {}
}

fn percentile_millis(samples_micros: &[u32], numerator: usize, denominator: usize) -> f32 {
    let Some(last_index) = samples_micros.len().checked_sub(1) else {
        return 0.0;
    };
    let rank = (samples_micros.len().saturating_mul(numerator) + denominator.saturating_sub(1))
        / denominator;
    let index = rank.saturating_sub(1).min(last_index);
    samples_micros[index] as f32 / 1000.0
}

struct TerminalTab {
    id: TabId,
    window_id: String,
    window_index: i32,
    panes: Vec<TerminalPane>,
    active_pane_id: String,
    agent_thread_id: Option<String>,
    pinned: bool,
    manual_title: Option<String>,
    explicit_title: Option<String>,
    /// When `true`, `explicit_title` is a speculative seed derived from the
    /// initial working directory at tab creation—not a title confirmed by shell
    /// integration.  While this flag is set, `title_source_candidate` prefers a
    /// live `shell_title` over the prediction.  Cleared to `false` by any real
    /// explicit-title event (`set_explicit_title`, `activate_pending_command_title`)
    /// or by `clear_terminal_titles`.
    explicit_title_is_prediction: bool,
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
    agent_command_has_started: bool,
    progress_state: ProgressState,
    command_lifecycle: CommandLifecycle,
}

struct NativePaneZoomSnapshot {
    other_panes: Vec<TerminalPane>,
    active_pane_geometry: (u16, u16, u16, u16),
    active_pane_id: String,
    active_original_index: usize,
}
impl TerminalTab {
    fn has_active_pane(&self) -> bool {
        self.panes.iter().any(|pane| pane.id == self.active_pane_id)
    }

    fn assert_active_pane_invariant(&self) {
        assert!(
            self.panes.is_empty() || self.has_active_pane(),
            "tab {} is missing active pane {}",
            self.window_id,
            self.active_pane_id
        );
    }

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
    contrast_profile: ChromeContrastProfile,
}

impl<'a> OverlayStyleBuilder<'a> {
    fn new(
        colors: &'a TerminalColors,
        background_opacity: f32,
        contrast_profile: ChromeContrastProfile,
    ) -> Self {
        Self {
            colors,
            background_opacity,
            contrast_profile,
        }
    }

    fn panel_background(self, base_alpha: f32) -> gpui::Rgba {
        let alpha = adaptive_overlay_panel_alpha_for_opacity(base_alpha, self.background_opacity);
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

    fn chrome_panel_background(self, base_alpha: f32) -> gpui::Rgba {
        let alpha = adaptive_overlay_panel_alpha_for_opacity(
            self.contrast_profile.panel_surface_alpha(base_alpha),
            self.background_opacity,
        );
        self.with_alpha(self.colors.background, alpha)
    }

    fn chrome_panel_background_with_floor(
        self,
        base_alpha: f32,
        translucent_floor_alpha: f32,
    ) -> gpui::Rgba {
        let alpha = adaptive_overlay_panel_alpha_with_floor_for_opacity(
            self.contrast_profile.panel_surface_alpha(base_alpha),
            self.background_opacity,
            self.contrast_profile
                .panel_surface_alpha(translucent_floor_alpha),
        );
        self.with_alpha(self.colors.background, alpha)
    }

    fn chrome_panel_cursor(self, base_alpha: f32) -> gpui::Rgba {
        let alpha = adaptive_overlay_panel_alpha_for_opacity(
            self.contrast_profile.panel_accent_alpha(base_alpha),
            self.background_opacity,
        );
        self.with_alpha(self.colors.cursor, alpha)
    }

    fn chrome_panel_neutral(self, base_alpha: f32) -> gpui::Rgba {
        let alpha = adaptive_overlay_panel_alpha_for_opacity(
            self.contrast_profile.panel_neutral_alpha(base_alpha),
            self.background_opacity,
        );
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum TabBarVisibility {
    #[default]
    FollowConfig,
    ForceVisible,
    ForceHidden,
}

/// The main terminal view component
pub struct TerminalView {
    tabs: Vec<TerminalTab>,
    native_pane_zoom_snapshots: HashMap<TabId, NativePaneZoomSnapshot>,
    next_tab_id: TabId,
    active_tab: usize,
    renaming_tab: Option<usize>,
    rename_input: InlineInputState,
    renaming_agent_project_id: Option<String>,
    agent_project_rename_input: InlineInputState,
    renaming_agent_thread_id: Option<String>,
    agent_thread_rename_input: InlineInputState,
    agent_sidebar_search_active: bool,
    agent_sidebar_search_input: InlineInputState,
    agent_git_panel_input_mode: Option<agents::AgentGitPanelInputMode>,
    agent_git_panel_input: InlineInputState,
    agent_git_panel_branch_dropdown_open: bool,
    agent_git_panel_poll_task: Option<gpui::Task<()>>,
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
    ai_features_enabled: bool,
    agent_sidebar_enabled: bool,
    agent_sidebar_width: f32,
    agent_sidebar_open: bool,
    agent_git_panel: agents::AgentGitPanelState,
    agent_git_panel_width: f32,
    agent_git_panel_resize_drag: Option<AgentGitPanelResizeDragState>,
    last_viewport_width: f32,
    active_agent_project_id: Option<String>,
    collapsed_agent_project_ids: HashSet<String>,
    agent_projects: Vec<agents::AgentProject>,
    agent_threads: Vec<agents::AgentThread>,
    hovered_agent_thread_id: Option<String>,
    auto_hide_tabbar: bool,
    tab_bar_visibility: TabBarVisibility,
    new_tab_animation_tab_id: Option<TabId>,
    new_tab_animation_start: Option<Instant>,
    new_tab_animation_scheduled: bool,
    show_termy_in_titlebar: bool,
    tab_shell_integration: TabTitleShellIntegration,
    notifications_enabled: bool,
    notification_min_duration: f32,
    notify_only_unfocused: bool,
    shell_integration_enabled: bool,
    progress_indicator_enabled: bool,
    configured_working_dir: Option<String>,
    child_working_dir_cache: HashMap<u32, ChildWorkingDirCacheEntry>,
    child_working_dir_lookup_pending: HashSet<u32>,
    terminal_runtime: TerminalRuntimeConfig,
    runtime: RuntimeState,
    tmux_enabled_config: bool,
    native_tab_persistence: bool,
    native_layout_autosave: bool,
    native_buffer_persistence: bool,
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
    chrome_contrast: bool,
    background_opacity_cells: bool,
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
    last_terminal_modifiers: gpui::Modifiers,
    pending_key_releases: HashMap<String, PendingKeyRelease>,
    deferred_ime_key_releases: HashSet<String>,
    selection_anchor: Option<SelectionPos>,
    selection_head: Option<SelectionPos>,
    selection_dragging: bool,
    selection_moved: bool,
    /// Tracks the active terminal's display_offset as observed from the UI thread.
    /// Updated after every user-initiated scroll and after each content-scroll adjustment,
    /// so that process_terminal_events can detect only content-driven offset changes.
    content_scroll_baseline: usize,
    pending_cursor_move_click: Option<PendingCursorMoveClick>,
    pending_cursor_move_preview: Option<PendingCursorMovePreview>,
    terminal_context_menu: Option<TerminalContextMenuState>,
    tab_context_menu: Option<TabContextMenuState>,
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
    resize_throttle_task: Option<gpui::Task<()>>,
    last_resize_applied_at: Option<Instant>,
    benchmark_session: Option<BenchmarkSession>,
    benchmark_exit_scheduled: bool,
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
    terminal_scrollbar_track_hold: Option<TerminalScrollbarTrackHoldState>,
    terminal_scrollbar_track_hold_active: bool,
    pane_resize_drag: Option<PaneResizeDragState>,
    hovered_pane_divider: Option<HoveredPaneDivider>,
    pane_resize_blocked: bool,
    vertical_tab_strip_resize_drag: Option<VerticalTabStripResizeDragState>,
    agent_sidebar_resize_drag: Option<AgentSidebarResizeDragState>,
    terminal_scrollbar_marker_cache: TerminalScrollbarMarkerCache,
    /// Cached cell dimensions keyed by font-size bits.
    cell_size_cache: HashMap<u32, Size<Pixels>>,
    // Search state
    search_open: bool,
    search_input: InlineInputState,
    search_state: SearchState,
    search_debounce_token: u64,
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
    #[cfg(target_os = "macos")]
    native_file_drop_enabled: bool,
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

    pub(super) const fn update_banner_height() -> f32 {
        #[cfg(target_os = "macos")]
        {
            UPDATE_BANNER_HEIGHT
        }

        #[cfg(not(target_os = "macos"))]
        {
            0.0
        }
    }

    pub(super) fn update_banner_visible(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.show_update_banner
        }

        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn set_native_file_drop_enabled(&mut self, enabled: bool) {
        self.native_file_drop_enabled = enabled;
    }

    pub(super) fn refresh_install_cli_availability(&mut self) -> bool {
        let (next_available, changed) = Self::refreshed_install_cli_availability(
            self.install_cli_available,
            termy_cli_install_core::is_cli_installed(),
        );
        self.install_cli_available = next_available;
        changed
    }

    fn runtime_config_from_app_config(
        config: &AppConfig,
        colors: &TerminalColors,
    ) -> TerminalRuntimeConfig {
        let working_dir_fallback = match config.working_dir_fallback {
            config::WorkingDirFallback::Home => RuntimeWorkingDirFallback::Home,
            config::WorkingDirFallback::Process => RuntimeWorkingDirFallback::Process,
        };

        TerminalRuntimeConfig {
            shell: config.shell.clone(),
            term: config.term.clone(),
            colorterm: config.colorterm.clone(),
            query_colors: Self::terminal_query_colors(colors),
            working_dir_fallback,
            scrollback_history: config.scrollback_history,
            default_cursor_style: match config.cursor_style {
                AppCursorStyle::Line => TerminalCursorStyle::Line,
                AppCursorStyle::Block => TerminalCursorStyle::Block,
            },
        }
    }

    fn terminal_query_colors(colors: &TerminalColors) -> TerminalQueryColors {
        TerminalQueryColors {
            ansi: colors.ansi.map(Self::ansi_rgb_from_rgba),
            foreground: Self::ansi_rgb_from_rgba(colors.foreground),
            background: Self::ansi_rgb_from_rgba(colors.background),
            cursor: None,
        }
    }

    fn ansi_rgb_from_rgba(color: gpui::Rgba) -> alacritty_terminal::vte::ansi::Rgb {
        let to_u8 = |component: f32| (component.clamp(0.0, 1.0) * 255.0).round() as u8;
        alacritty_terminal::vte::ansi::Rgb {
            r: to_u8(color.r),
            g: to_u8(color.g),
            b: to_u8(color.b),
        }
    }

    #[cfg(test)]
    fn uses_event_driven_tmux_wakeup() -> bool {
        true
    }

    fn user_home_dir() -> Option<PathBuf> {
        dirs::home_dir()
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
        let path = resolve_launch_working_directory(configured_working_dir, fallback)?;
        Some(path.to_string_lossy().into_owned())
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

    fn resolve_preferred_working_directory(
        explicit_working_dir: Option<&str>,
        prompt_cwd: Option<&str>,
        process_cwd: Option<&str>,
        title_cwd: Option<&str>,
        configured_working_dir: Option<&str>,
        fallback: RuntimeWorkingDirFallback,
    ) -> Option<String> {
        // Keep tmux and native session creation on the same cwd precedence chain so
        // new tabs/panes do not drift based on which runtime happens to be active.
        let explicit_working_dir = normalize_working_directory_candidate(explicit_working_dir);
        let prompt_cwd = normalize_working_directory_candidate(prompt_cwd);
        let process_cwd = normalize_working_directory_candidate(process_cwd);
        let title_cwd = title_cwd
            .map(str::trim)
            .filter(|value| Self::looks_like_working_dir_path(value))
            .and_then(|value| normalize_working_directory_candidate(Some(value)));

        explicit_working_dir
            .or(prompt_cwd)
            .or(process_cwd)
            .or(title_cwd)
            .or_else(|| {
                resolve_launch_working_directory(configured_working_dir, fallback)
                    .map(|path| path.to_string_lossy().into_owned())
            })
    }

    fn preferred_working_dir_for_new_session(
        &mut self,
        explicit_working_dir: Option<&str>,
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
            })
            .map(|candidate| candidate.to_string());

        Self::resolve_preferred_working_directory(
            explicit_working_dir,
            prompt_cwd.as_deref(),
            process_cwd.as_deref(),
            title_cwd.as_deref(),
            self.configured_working_dir.as_deref(),
            self.terminal_runtime.working_dir_fallback,
        )
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
        let explicit_title_is_prediction = predicted_prompt_title.is_some();
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
            ..TerminalPane::new_native(pane_id.clone(), 0, 0, cols.max(1), rows.max(1), terminal)
        };
        TerminalTab {
            id: tab_id,
            window_id: format!("@native-{tab_id}"),
            window_index: 0,
            panes: vec![pane],
            active_pane_id: pane_id,
            agent_thread_id: None,
            pinned: false,
            manual_title: None,
            explicit_title: predicted_prompt_title,
            explicit_title_is_prediction,
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
            agent_command_has_started: false,
            progress_state: ProgressState::default(),
            command_lifecycle: CommandLifecycle::default(),
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

    fn chrome_contrast_profile(&self) -> ChromeContrastProfile {
        ChromeContrastProfile::from_enabled(self.chrome_contrast)
    }

    fn scaled_chrome_surface_alpha(&self, base_alpha: f32) -> f32 {
        scaled_chrome_alpha_for_opacity(
            self.chrome_contrast_profile().surface_alpha(base_alpha),
            self.effective_background_opacity(),
        )
    }

    fn scaled_chrome_neutral_border_alpha(&self, base_alpha: f32) -> f32 {
        scaled_chrome_alpha_for_opacity(
            self.chrome_contrast_profile()
                .neutral_border_alpha(base_alpha),
            self.effective_background_opacity(),
        )
    }

    fn scaled_chrome_accent_alpha(&self, base_alpha: f32) -> f32 {
        scaled_chrome_alpha_for_opacity(
            self.chrome_contrast_profile().accent_alpha(base_alpha),
            self.effective_background_opacity(),
        )
    }

    fn effective_background_opacity(&self) -> f32 {
        config::effective_background_opacity(
            self.background_opacity,
            self.preview_background_opacity,
        )
    }

    fn tab_switch_hints_blocked(&self) -> bool {
        self.is_command_palette_open() || self.search_open
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

    pub(crate) fn start_new_tab_animation(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        self.new_tab_animation_tab_id = Some(tab_id);
        self.new_tab_animation_start = Some(Instant::now());
        self.new_tab_animation_scheduled = false;
        self.schedule_new_tab_animation(cx);
    }

    fn schedule_new_tab_animation(&mut self, cx: &mut Context<Self>) {
        if self.new_tab_animation_scheduled {
            return;
        }
        self.new_tab_animation_scheduled = true;
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            smol::Timer::after(Duration::from_millis(NEW_TAB_ANIMATION_FRAME_MS)).await;
            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    view.new_tab_animation_scheduled = false;
                    let still_animating = view
                        .new_tab_animation_start
                        .map(|start| {
                            Instant::now().saturating_duration_since(start)
                                < NEW_TAB_ANIMATION_DURATION
                        })
                        .unwrap_or(false);
                    view.mark_tab_strip_layout_dirty();
                    if still_animating {
                        view.schedule_new_tab_animation(cx);
                    } else {
                        view.new_tab_animation_tab_id = None;
                        view.new_tab_animation_start = None;
                    }
                    cx.notify();
                })
            });
        })
        .detach();
    }

    pub(crate) fn new_tab_animation_progress(&self, now: Instant) -> Option<(usize, f32)> {
        let tab_id = self.new_tab_animation_tab_id?;
        let start = self.new_tab_animation_start?;
        let elapsed = now.saturating_duration_since(start).as_secs_f32();
        let total = NEW_TAB_ANIMATION_DURATION.as_secs_f32();
        if elapsed >= total {
            return None;
        }
        let raw = (elapsed / total).clamp(0.0, 1.0);
        let progress = 1.0 - (1.0 - raw).powi(3); // ease_out_cubic
        let index = self.tabs.iter().position(|t| t.id == tab_id)?;
        Some((index, progress))
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
        OverlayStyleBuilder::new(
            &self.colors,
            self.effective_background_opacity(),
            self.chrome_contrast_profile(),
        )
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

    fn record_benchmark_view_wakeup(&mut self) {
        if let Some(benchmark_session) = self.benchmark_session.as_mut() {
            benchmark_session.record_view_wakeup();
        }
    }

    fn record_benchmark_terminal_event_drain_pass(&mut self) {
        if let Some(benchmark_session) = self.benchmark_session.as_mut() {
            benchmark_session.record_terminal_event_drain_pass();
        }
    }

    fn record_benchmark_terminal_redraw(&mut self) {
        if let Some(benchmark_session) = self.benchmark_session.as_mut() {
            benchmark_session.record_terminal_redraw();
        }
    }

    fn record_benchmark_frame(&mut self, now: Instant) {
        if let Some(benchmark_session) = self.benchmark_session.as_mut() {
            benchmark_session.record_frame(now);
        }
    }

    fn sample_benchmark_session(&mut self) {
        if let Some(benchmark_session) = self.benchmark_session.as_mut() {
            benchmark_session.sample_if_due(Instant::now());
        }
    }

    fn finish_benchmark_session(&mut self) {
        if let Some(benchmark_session) = self.benchmark_session.as_mut()
            && let Err(error) = benchmark_session.finish()
        {
            log::error!("Failed to write benchmark metrics: {error}");
        }
    }

    fn benchmark_exit_on_complete(&self) -> bool {
        self.benchmark_session
            .as_ref()
            .is_some_and(BenchmarkSession::exit_on_complete)
    }

    fn schedule_benchmark_exit(&mut self, cx: &mut Context<Self>) {
        if self.benchmark_exit_scheduled {
            return;
        }

        self.benchmark_exit_scheduled = true;
        self.allow_quit_without_prompt = true;
        cx.notify();
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            smol::Timer::after(BENCHMARK_EXIT_GRACE_DURATION).await;
            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    if view
                        .benchmark_session
                        .as_ref()
                        .is_none_or(BenchmarkSession::is_finished)
                    {
                        return;
                    }
                    view.finish_benchmark_session();
                    cx.quit();
                })
            });
        })
        .detach();
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

    /// Schedules a follow-up task to apply pending resize after the throttle window.
    /// This ensures the final resize is applied even if no new frames are rendered.
    fn schedule_resize_throttle_follow_up(&mut self, cx: &mut Context<Self>) {
        // Only schedule if not already scheduled
        if self.resize_throttle_task.is_some() {
            return;
        }

        self.resize_throttle_task = Some(cx.spawn(async move |this, cx| {
            smol::Timer::after(Duration::from_millis(RESIZE_THROTTLE_MS + 1)).await;
            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    view.resize_throttle_task = None;
                    // Clear the timestamp to allow immediate resize on next frame
                    view.last_resize_applied_at = None;
                    // Trigger a redraw to apply any pending resize
                    cx.notify();
                })
            });
        }));
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

    fn terminal_content_bounds(&self, window: &Window) -> Option<TerminalContentRect> {
        let viewport = window.viewport_size();
        let viewport_width: f32 = viewport.width.into();
        let viewport_height: f32 = viewport.height.into();
        TerminalContentRect::new(
            0.0,
            0.0,
            viewport_width - self.terminal_left_sidebar_width() - self.terminal_right_panel_width(),
            viewport_height - self.terminal_content_top_inset(),
        )
    }

    fn pane_neighbor_gaps(pane: &TerminalPane, panes: &[TerminalPane]) -> TerminalPaneNeighborGaps {
        let pane_left = u32::from(pane.left);
        let pane_top = u32::from(pane.top);
        let pane_right = pane_left + u32::from(pane.width);
        let pane_bottom = pane_top + u32::from(pane.height);
        let mut gaps = TerminalPaneNeighborGaps::default();

        for candidate in panes {
            if candidate.id == pane.id {
                continue;
            }

            let candidate_left = u32::from(candidate.left);
            let candidate_top = u32::from(candidate.top);
            let candidate_right = candidate_left + u32::from(candidate.width);
            let candidate_bottom = candidate_top + u32::from(candidate.height);

            if candidate_left >= pane_right
                && cell_ranges_overlap(pane_top, pane_bottom, candidate_top, candidate_bottom)
            {
                let gap = candidate_left.saturating_sub(pane_right);
                gaps.right_cells = Some(gaps.right_cells.map_or(gap, |current| current.min(gap)));
            }

            if candidate_top >= pane_bottom
                && cell_ranges_overlap(pane_left, pane_right, candidate_left, candidate_right)
            {
                let gap = candidate_top.saturating_sub(pane_bottom);
                gaps.bottom_cells = Some(gaps.bottom_cells.map_or(gap, |current| current.min(gap)));
            }
        }

        gaps
    }

    fn terminal_pane_layout(
        &self,
        tab: &TerminalTab,
        pane: &TerminalPane,
        content_bounds: TerminalContentRect,
    ) -> Option<TerminalPaneLayout> {
        let layout_cell_size = self.layout_cell_size();
        let layout_cell_width: f32 = layout_cell_size.width.into();
        let layout_cell_height: f32 = layout_cell_size.height.into();
        if layout_cell_width <= f32::EPSILON || layout_cell_height <= f32::EPSILON {
            return None;
        }

        let terminal_size = pane.terminal.size();
        if terminal_size.cols == 0 || terminal_size.rows == 0 {
            return None;
        }

        let cell_width: f32 = terminal_size.cell_width.into();
        let cell_height: f32 = terminal_size.cell_height.into();
        if cell_width <= f32::EPSILON || cell_height <= f32::EPSILON {
            return None;
        }

        let (outer_padding_x, outer_padding_y) = self.effective_terminal_padding();
        let (content_padding_x, content_padding_y) = self.native_split_content_padding();
        let frame = TerminalContentRect::new(
            outer_padding_x + (f32::from(pane.left) * layout_cell_width),
            outer_padding_y + (f32::from(pane.top) * layout_cell_height),
            f32::from(pane.width) * layout_cell_width,
            f32::from(pane.height) * layout_cell_height,
        )?;
        let content_frame = TerminalContentRect::new(
            frame.origin_x + content_padding_x,
            frame.origin_y + content_padding_y,
            f32::from(terminal_size.cols) * cell_width,
            f32::from(terminal_size.rows) * cell_height,
        )?;
        let gaps = Self::pane_neighbor_gaps(pane, &tab.panes);
        let pane_right = u32::from(pane.left).saturating_add(u32::from(pane.width));
        let pane_bottom = u32::from(pane.top).saturating_add(u32::from(pane.height));
        let max_right = tab
            .panes
            .iter()
            .map(|candidate| u32::from(candidate.left).saturating_add(u32::from(candidate.width)))
            .max()
            .unwrap_or(pane_right);
        let max_bottom = tab
            .panes
            .iter()
            .map(|candidate| u32::from(candidate.top).saturating_add(u32::from(candidate.height)))
            .max()
            .unwrap_or(pane_bottom);
        let multi_pane = tab.panes.len() > 1;
        let extends_right_edge = !multi_pane || pane_right == max_right;
        let extends_bottom_edge = !multi_pane || pane_bottom == max_bottom;
        let scrollbar_surface = TerminalScrollbarSurfaceGeometry::new(
            if multi_pane {
                frame.origin_x
            } else {
                content_bounds.origin_x
            },
            if multi_pane {
                frame.origin_y
            } else {
                content_bounds.origin_y
            },
            if multi_pane && !extends_right_edge {
                frame.width
            } else if multi_pane {
                (content_bounds.right() - frame.origin_x).max(0.0)
            } else {
                content_bounds.width
            },
            if multi_pane && !extends_bottom_edge {
                frame.height
            } else if multi_pane {
                (content_bounds.bottom() - frame.origin_y).max(0.0)
            } else {
                content_bounds.height
            },
        )?;

        Some(TerminalPaneLayout {
            frame,
            content_frame,
            scrollbar_surface,
            cell_width: layout_cell_width,
            cell_height: layout_cell_height,
            extends_right_edge,
            extends_bottom_edge,
            gaps,
        })
    }

    fn active_terminal_pane_layout(&self, window: &Window) -> Option<TerminalPaneLayout> {
        let content_bounds = self.terminal_content_bounds(window)?;
        let tab = self.active_tab_ref()?;
        let pane_index = tab.active_pane_index()?;
        let pane = tab.panes.get(pane_index)?;
        self.terminal_pane_layout(tab, pane, content_bounds)
    }

    pub(super) fn terminal_viewport_geometry(&self) -> Option<TerminalViewportGeometry> {
        let tab = self.active_tab_ref()?;
        let pane_index = tab.active_pane_index()?;
        let pane = tab.panes.get(pane_index)?;
        let layout_cell_size = self.layout_cell_size();
        let layout_cell_width: f32 = layout_cell_size.width.into();
        let layout_cell_height: f32 = layout_cell_size.height.into();
        let size = pane.terminal.size();
        if layout_cell_width <= f32::EPSILON
            || layout_cell_height <= f32::EPSILON
            || size.cols == 0
            || size.rows == 0
        {
            return None;
        }
        let (padding_x, padding_y) = self.effective_terminal_padding();
        let (content_padding_x, content_padding_y) = self.native_split_content_padding();
        let pane_cell_height: f32 = size.cell_height.into();
        Some(TerminalViewportGeometry {
            origin_x: padding_x + (f32::from(pane.left) * layout_cell_width) + content_padding_x,
            origin_y: padding_y + (f32::from(pane.top) * layout_cell_height) + content_padding_y,
            height: pane_cell_height * f32::from(size.rows),
        })
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

    pub(super) fn set_tab_bar_visibility(&mut self, visibility: TabBarVisibility) -> bool {
        if self.tab_bar_visibility == visibility {
            return false;
        }

        self.tab_bar_visibility = visibility;
        self.clear_pane_render_caches();
        self.clear_terminal_scrollbar_marker_cache();
        self.cell_size_cache.clear();
        self.mark_tab_strip_layout_dirty();
        true
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
        self.terminal_scrollbar_track_hold = None;
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

    fn start_terminal_scrollbar_track_hold(
        &mut self,
        state: TerminalScrollbarTrackHoldState,
        cx: &mut Context<Self>,
    ) {
        self.terminal_scrollbar_track_hold = Some(state);
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
        if let Some(state) = self.terminal_scrollbar_track_hold.as_mut() {
            state.local_y = local_y;
        }
    }

    pub(super) fn stop_terminal_scrollbar_track_hold(&mut self) -> bool {
        self.terminal_scrollbar_track_hold.take().is_some()
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
        let terminal_frame_drain_scheduled = Arc::new(AtomicBool::new(false));
        let window_handle = window.window_handle();

        // Focus the terminal immediately
        focus_handle.focus(window, cx);

        // Process terminal events on the next frame so bursty PTY wakeups do not monopolize
        // the UI executor and starve actual paints.
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            while event_wakeup_rx.recv_async().await.is_ok() {
                while event_wakeup_rx.try_recv().is_ok() {}
                let mut should_schedule = false;
                let result = cx.update(|cx| {
                    this.update(cx, |view, _cx| {
                        view.record_benchmark_view_wakeup();
                        view.debug_overlay_stats.record_view_wake_signal();
                        if !terminal_frame_drain_scheduled.swap(true, Ordering::AcqRel) {
                            should_schedule = true;
                        }
                    })
                });
                if result.is_err() {
                    break;
                }
                if !should_schedule {
                    continue;
                }

                let this = this.clone();
                let terminal_frame_drain_scheduled = terminal_frame_drain_scheduled.clone();
                if cx
                    .update_window(window_handle, move |_, window, _| {
                        window.on_next_frame(move |_window, cx| {
                            terminal_frame_drain_scheduled.store(false, Ordering::Release);
                            let _ = this.update(cx, |view, cx| {
                                if view.process_terminal_events(cx) {
                                    cx.notify();
                                }
                            });
                        });
                    })
                    .is_err()
                {
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
        // Skip the notification when the blink has no visible effect (blink disabled
        // or command palette is covering the terminal cursor).
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                smol::Timer::after(Duration::from_millis(CURSOR_BLINK_INTERVAL_MS)).await;
                let result = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        if view.tick_cursor_blink()
                            && !view.is_command_palette_open()
                            && view.renaming_tab.is_none()
                        {
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
        let benchmark_config = match BenchmarkConfig::from_env() {
            Ok(config) => config,
            Err(error) => {
                eprintln!("Termy startup blocked: {error}");
                std::process::exit(1);
            }
        };
        if benchmark_config.is_some() && RuntimeKind::from_app_config(&config) == RuntimeKind::Tmux
        {
            eprintln!("Termy startup blocked: benchmark mode requires native runtime");
            std::process::exit(1);
        }
        let tab_title = config.tab_title.clone();
        let tab_shell_integration = TabTitleShellIntegration {
            enabled: tab_title.shell_integration,
            explicit_prefix: tab_title.explicit_prefix.clone(),
        };
        let terminal_runtime = Self::runtime_config_from_app_config(&config, &colors);
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
            benchmark_config
                .as_ref()
                .map(|config| config.command.as_str()),
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
            renaming_agent_project_id: None,
            agent_project_rename_input: InlineInputState::new(String::new()),
            renaming_agent_thread_id: None,
            agent_thread_rename_input: InlineInputState::new(String::new()),
            agent_sidebar_search_active: false,
            agent_sidebar_search_input: InlineInputState::new(String::new()),
            agent_git_panel_input_mode: None,
            agent_git_panel_input: InlineInputState::new(String::new()),
            agent_git_panel_branch_dropdown_open: false,
            agent_git_panel_poll_task: None,
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
            vertical_tabs_width: tab_strip::clamp_expanded_vertical_tab_strip_width(
                config.vertical_tabs_width,
            ),
            vertical_tabs_minimized: config.vertical_tabs_minimized,
            ai_features_enabled: config.ai_features_enabled,
            agent_sidebar_enabled: if cfg!(target_os = "windows") {
                false
            } else {
                config.ai_features_enabled && config.agent_sidebar_enabled
            },
            agent_sidebar_width: agents::clamp_agent_sidebar_width(config.agent_sidebar_width),
            agent_sidebar_open: false,
            agent_git_panel: agents::AgentGitPanelState::default(),
            agent_git_panel_width: agents::AGENT_GIT_PANEL_DEFAULT_WIDTH,
            agent_git_panel_resize_drag: None,
            last_viewport_width: 1280.0,
            active_agent_project_id: None,
            collapsed_agent_project_ids: HashSet::new(),
            agent_projects: Vec::new(),
            agent_threads: Vec::new(),
            hovered_agent_thread_id: None,
            auto_hide_tabbar: config.auto_hide_tabbar,
            tab_bar_visibility: TabBarVisibility::FollowConfig,
            new_tab_animation_tab_id: None,
            new_tab_animation_start: None,
            new_tab_animation_scheduled: false,
            show_termy_in_titlebar: config.show_termy_in_titlebar,
            tab_shell_integration,
            notifications_enabled: config.notifications_enabled,
            notification_min_duration: config.notification_min_duration,
            notify_only_unfocused: config.notify_only_unfocused,
            shell_integration_enabled: config.shell_integration_enabled,
            progress_indicator_enabled: config.progress_indicator_enabled,
            configured_working_dir,
            child_working_dir_cache: HashMap::new(),
            child_working_dir_lookup_pending: HashSet::new(),
            terminal_runtime,
            runtime,
            tmux_enabled_config: config.tmux_enabled,
            native_tab_persistence: config.native_tab_persistence,
            native_layout_autosave: config.native_layout_autosave,
            native_buffer_persistence: config.native_buffer_persistence,
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
            chrome_contrast: config.chrome_contrast,
            background_opacity_cells: config.background_opacity_cells,
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
            line_height: config.line_height.clamp(MIN_LINE_HEIGHT, MAX_LINE_HEIGHT),
            copy_on_select: config.copy_on_select,
            copy_on_select_toast: config.copy_on_select_toast,
            last_terminal_modifiers: gpui::Modifiers::default(),
            pending_key_releases: HashMap::default(),
            deferred_ime_key_releases: HashSet::default(),
            selection_anchor: None,
            selection_head: None,
            selection_dragging: false,
            selection_moved: false,
            content_scroll_baseline: 0,
            pending_cursor_move_click: None,
            pending_cursor_move_preview: None,
            terminal_context_menu: None,
            tab_context_menu: None,
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
            resize_throttle_task: None,
            last_resize_applied_at: None,
            benchmark_session: benchmark_config.map(BenchmarkSession::new),
            benchmark_exit_scheduled: false,
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
            terminal_scrollbar_track_hold: None,
            terminal_scrollbar_track_hold_active: false,
            pane_resize_drag: None,
            hovered_pane_divider: None,
            pane_resize_blocked: false,
            vertical_tab_strip_resize_drag: None,
            agent_sidebar_resize_drag: None,
            terminal_scrollbar_marker_cache: TerminalScrollbarMarkerCache::default(),
            cell_size_cache: HashMap::new(),
            search_open: false,
            search_input: InlineInputState::new(String::new()),
            search_state: SearchState::new(),
            search_debounce_token: 0,
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
            #[cfg(target_os = "macos")]
            native_file_drop_enabled: false,
        };
        #[cfg(target_os = "windows")]
        if config.tmux_enabled {
            // Surface explicit feedback when a synced/shared config requests tmux on Windows.
            termy_toast::warning(TMUX_UNSUPPORTED_WINDOWS_TOAST);
        }
        command_palette::prewarm_user_path_resolution();
        if view.ai_features_enabled {
            view.restore_persisted_agent_workspace();
        } else {
            view.reset_agent_workspace_runtime_state();
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

        if view.benchmark_session.is_some() {
            cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                loop {
                    smol::Timer::after(BENCHMARK_SAMPLE_INTERVAL).await;
                    let result = cx.update(|cx| {
                        this.update(cx, |view, _cx| {
                            view.sample_benchmark_session();
                        })
                    });
                    if result.is_err() {
                        break;
                    }
                }
            })
            .detach();
        }
        cx.observe_window_activation(window, |view, window, cx| {
            if !window.is_window_active() && view.release_all_forwarded_mouse_presses() {
                cx.notify();
            }
        })
        .detach();
        cx.on_blur(&blur_focus_handle, window, |view, _window, cx| {
            let released_mouse_presses = view.release_all_forwarded_mouse_presses();
            let released_keyboard_modifiers = view.release_forwarded_modifiers(cx);
            let cleared_tab_switch_hint_state = view.tab_strip.switch_hints.reset_hold_state();
            let dismissed_context_menu = view.close_terminal_context_menu(cx);
            if released_mouse_presses
                || released_keyboard_modifiers
                || cleared_tab_switch_hint_state
                || dismissed_context_menu
            {
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
        let ai_features_enabled_changed = self.ai_features_enabled != config.ai_features_enabled;
        let vertical_tabs_width =
            tab_strip::clamp_expanded_vertical_tab_strip_width(config.vertical_tabs_width);
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
        let auto_hide_tabbar_changed = self.auto_hide_tabbar != config.auto_hide_tabbar;
        self.tab_close_visibility = config.tab_close_visibility;
        self.tab_width_mode = config.tab_width_mode;
        self.vertical_tabs = config.vertical_tabs;
        self.vertical_tabs_width = vertical_tabs_width;
        self.vertical_tabs_minimized = config.vertical_tabs_minimized;
        self.ai_features_enabled = config.ai_features_enabled;
        self.agent_sidebar_enabled = if cfg!(target_os = "windows") {
            false
        } else {
            config.ai_features_enabled && config.agent_sidebar_enabled
        };
        self.agent_sidebar_width = agents::clamp_agent_sidebar_width(config.agent_sidebar_width);
        if !self.ai_features_enabled {
            self.reset_agent_workspace_runtime_state();
        } else if ai_features_enabled_changed {
            self.restore_persisted_agent_workspace();
        } else if !self.agent_sidebar_enabled {
            self.agent_sidebar_open = false;
            self.agent_git_panel = agents::AgentGitPanelState::default();
            self.agent_git_panel_input_mode = None;
            self.agent_git_panel_input.clear();
            self.agent_git_panel_branch_dropdown_open = false;
            self.renaming_agent_project_id = None;
            self.renaming_agent_thread_id = None;
        } else if self.agent_projects.is_empty() && self.agent_threads.is_empty() {
            self.agent_sidebar_open = true;
        }
        self.auto_hide_tabbar = config.auto_hide_tabbar;
        self.show_termy_in_titlebar = config.show_termy_in_titlebar;
        self.show_debug_overlay = config.show_debug_overlay;
        self.tab_shell_integration = TabTitleShellIntegration {
            enabled: self.tab_title.shell_integration,
            explicit_prefix: self.tab_title.explicit_prefix.clone(),
        };
        self.notifications_enabled = config.notifications_enabled;
        self.notification_min_duration = config.notification_min_duration;
        self.notify_only_unfocused = config.notify_only_unfocused;
        self.shell_integration_enabled = config.shell_integration_enabled;
        self.progress_indicator_enabled = config.progress_indicator_enabled;
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
        self.native_tab_persistence = config.native_tab_persistence;
        self.native_layout_autosave = config.native_layout_autosave;
        self.native_buffer_persistence = config.native_buffer_persistence;
        self.tmux_show_active_pane_border = config.tmux_show_active_pane_border;
        self.configured_working_dir = config.working_dir.clone();
        self.terminal_runtime = Self::runtime_config_from_app_config(&config, &self.colors);
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
        if vertical_tabs_changed || vertical_tabs_width_changed || vertical_tabs_minimized_changed {
            self.clear_pane_render_caches();
            self.clear_terminal_scrollbar_marker_cache();
            self.cell_size_cache.clear();
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
        self.line_height = config.line_height.clamp(MIN_LINE_HEIGHT, MAX_LINE_HEIGHT);
        self.cursor_style = config.cursor_style;
        self.cursor_blink = config.cursor_blink;
        self.cursor_blink_visible = true;
        self.cell_size_cache.clear();
        if self.font_family != previous_font_family || self.font_size != previous_font_size {
            self.clear_tab_title_width_cache();
            self.mark_tab_strip_layout_dirty();
        }
        self.background_opacity = config.background_opacity;
        self.chrome_contrast = config.chrome_contrast;
        self.background_opacity_cells = config.background_opacity_cells;
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
            self.terminal_scrollbar_track_hold = None;
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
                pane.terminal
                    .set_query_colors(self.terminal_runtime.query_colors);
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
            || auto_hide_tabbar_changed
        {
            self.mark_tab_strip_layout_dirty();
        }
        if tab_switch_modifier_hints_changed || auto_hide_tabbar_changed {
            cx.notify();
        }

        if self.is_command_palette_open() {
            self.reset_agent_command_palette_mode_if_disabled();
            self.refresh_command_palette_matches(true, cx);
        }

        true
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
        self.debug_overlay_stats.record_terminal_event_drain_pass();

        let should_redraw = if self.runtime_uses_tmux() {
            self.process_tmux_terminal_events(cx)
        } else {
            self.process_native_terminal_events(cx)
        };

        if should_redraw {
            self.debug_overlay_stats.record_terminal_redraw();

            // Detect content-driven display_offset changes: Alacritty auto-increments the
            // offset to keep the viewport stable when new lines arrive while the user is
            // scrolled into history. The background PTY thread already updated the offset
            // before we got here, so we compare against content_scroll_baseline (a value
            // we maintain separately from user-initiated scrolls) to find the delta.
            let current_offset = self
                .active_terminal()
                .map(|t| t.scroll_state().0)
                .unwrap_or(0);
            if current_offset != self.content_scroll_baseline {
                self.adjust_selection_for_display_offset_change(
                    self.content_scroll_baseline,
                    current_offset,
                );
                self.content_scroll_baseline = current_offset;
            }
        }

        should_redraw
    }

    fn process_native_terminal_events(&mut self, cx: &mut Context<Self>) -> bool {
        let mut should_redraw = false;
        let mut should_quit = false;
        let mut agent_tabs_to_close: Vec<TabId> = Vec::new();
        let active_tab = self.active_tab;
        let mut reply_host = GpuiClipboardReplyHost::from_cx(cx);
        self.record_benchmark_terminal_event_drain_pass();

        for index in 0..self.tabs.len() {
            let active_pane_id = self.tabs[index].active_pane_id.clone();

            for pane_index in 0..self.tabs[index].panes.len() {
                let pane_id = self.tabs[index].panes[pane_index].id.clone();
                let pane_is_active = pane_id == active_pane_id;
                let (events, has_more) = self.tabs[index].panes[pane_index]
                    .terminal
                    .drain_events(&mut reply_host);
                if has_more {
                    should_redraw = true;
                }

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
                            let was_running = self.tabs[index].agent_command_has_started
                                && self.tabs[index].running_process;
                            if pane_is_active && self.apply_terminal_title(index, &title, cx) {
                                should_redraw = true;
                            }
                            if was_running
                                && !self.tabs[index].running_process
                                && self.tabs[index].agent_thread_id.is_some()
                            {
                                agent_tabs_to_close.push(self.tabs[index].id);
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
                        // Shell integration events (OSC 133)
                        TerminalEvent::ShellPromptStart => {
                            if self.shell_integration_enabled {
                                // Notify for long-running commands when prompt returns
                                if let Some(duration) = self.tabs[index].command_lifecycle.elapsed()
                                {
                                    if duration.as_secs_f32() >= self.notification_min_duration
                                        && self.notifications_enabled
                                        && (!self.notify_only_unfocused
                                            || !termy_native_sdk::is_app_active())
                                    {
                                        termy_native_sdk::show_notification(
                                            "Command Complete",
                                            "Long-running command finished",
                                        );
                                    }
                                }
                                self.tabs[index].command_lifecycle.prompt_start();
                            }
                        }
                        TerminalEvent::ShellCommandStart => {
                            if self.shell_integration_enabled {
                                self.tabs[index].command_lifecycle.command_start();
                            }
                        }
                        TerminalEvent::ShellCommandExecuting => {
                            if self.shell_integration_enabled {
                                self.tabs[index].command_lifecycle.command_executing();
                            }
                        }
                        TerminalEvent::ShellCommandFinished(code) => {
                            if self.shell_integration_enabled {
                                self.tabs[index].command_lifecycle.command_finished(code);
                            }
                        }
                        // Notification events (OSC 9, OSC 777)
                        TerminalEvent::Notification { title, body } => {
                            if self.notifications_enabled {
                                let should_notify = !self.notify_only_unfocused
                                    || !termy_native_sdk::is_app_active();
                                if should_notify {
                                    termy_native_sdk::show_notification(&title, &body);
                                }
                            }
                        }
                        TerminalEvent::Notify(msg) => {
                            if self.notifications_enabled {
                                let should_notify = !self.notify_only_unfocused
                                    || !termy_native_sdk::is_app_active();
                                if should_notify {
                                    termy_native_sdk::show_notification("Terminal", &msg);
                                }
                            }
                        }
                        // Progress indicator (OSC 9;4)
                        TerminalEvent::Progress(state) => {
                            if self.progress_indicator_enabled {
                                self.tabs[index].progress_state = state;
                                should_redraw = true;
                            }
                        }
                        // Working directory (OSC 7)
                        TerminalEvent::WorkingDirectory(path) => {
                            self.tabs[index].last_prompt_cwd = Some(path);
                        }
                    }
                }
            }
        }

        for tab_id in agent_tabs_to_close {
            if let Some(tab_index) = self.tab_index_by_id(tab_id) {
                self.close_tab(tab_index, cx);
                should_redraw = true;
            }
        }

        if should_quit {
            // Shell `exit` in the last native pane should close the app immediately.
            self.sync_persisted_native_workspace();
            if self.benchmark_exit_on_complete() {
                self.schedule_benchmark_exit(cx);
                should_redraw = true;
            } else {
                self.finish_benchmark_session();
                self.allow_quit_without_prompt = true;
                cx.quit();
            }
        }

        if should_redraw {
            self.record_benchmark_terminal_redraw();
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

    #[test]
    fn terminal_overlay_geometry_defaults_to_square_edges() {
        assert_eq!(TERMINAL_OVERLAY_GEOMETRY.panel_radius, 0.0);
        assert_eq!(TERMINAL_OVERLAY_GEOMETRY.input_radius, 0.0);
        assert_eq!(TERMINAL_OVERLAY_GEOMETRY.control_radius, 0.0);
    }

    #[test]
    fn toast_geometry_uses_rounded_corners() {
        assert_eq!(TOAST_GEOMETRY.panel_radius, 10.0);
        assert_eq!(TOAST_GEOMETRY.input_radius, 6.0);
        assert_eq!(TOAST_GEOMETRY.control_radius, 6.0);
    }

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

    #[test]
    fn preferred_working_directory_prefers_active_sources_before_configured_and_fallback() {
        let cwd = TerminalView::resolve_preferred_working_directory(
            None,
            Some("/prompt"),
            Some("/process"),
            Some("/title"),
            Some("/configured"),
            RuntimeWorkingDirFallback::Process,
        );
        assert_eq!(cwd.as_deref(), Some("/prompt"));

        let cwd = TerminalView::resolve_preferred_working_directory(
            None,
            None,
            Some("/process"),
            Some("/title"),
            Some("/configured"),
            RuntimeWorkingDirFallback::Process,
        );
        assert_eq!(cwd.as_deref(), Some("/process"));
    }

    #[test]
    fn preferred_working_directory_uses_explicit_value_first() {
        let cwd = TerminalView::resolve_preferred_working_directory(
            Some(" /explicit "),
            Some("/prompt"),
            Some("/process"),
            Some("/title"),
            Some("/configured"),
            RuntimeWorkingDirFallback::Process,
        );
        assert_eq!(cwd.as_deref(), Some("/explicit"));
    }

    #[test]
    fn preferred_working_directory_expands_tilde_candidates() {
        let expected = TerminalView::user_home_dir()
            .expect("home dir")
            .to_string_lossy()
            .into_owned();
        let cwd = TerminalView::resolve_preferred_working_directory(
            Some("~"),
            None,
            None,
            None,
            None,
            RuntimeWorkingDirFallback::Process,
        );
        assert_eq!(cwd.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn preferred_working_directory_uses_configured_before_fallback() {
        let configured = std::env::current_dir().expect("current dir");
        let cwd = TerminalView::resolve_preferred_working_directory(
            None,
            None,
            None,
            None,
            Some(configured.to_string_lossy().as_ref()),
            RuntimeWorkingDirFallback::Home,
        );
        assert_eq!(cwd.as_deref(), Some(configured.to_string_lossy().as_ref()));
    }

    #[test]
    fn invalid_configured_working_directory_falls_back_instead_of_passing_through() {
        let fallback = std::env::current_dir()
            .expect("current dir")
            .to_string_lossy()
            .into_owned();
        let cwd = TerminalView::resolve_preferred_working_directory(
            None,
            None,
            None,
            None,
            Some("/definitely/not/a/real/termy/path"),
            RuntimeWorkingDirFallback::Process,
        );
        assert_eq!(cwd.as_deref(), Some(fallback.as_str()));
    }

    #[test]
    fn attach_resolution_uses_active_working_directory_before_default_launch_dir() {
        let cwd = TerminalView::resolve_preferred_working_directory(
            None,
            Some("/active/project"),
            None,
            None,
            Some("/configured"),
            RuntimeWorkingDirFallback::Process,
        );
        assert_eq!(cwd.as_deref(), Some("/active/project"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn unresolved_working_directory_fallback_uses_home_on_macos() {
        let expected = TerminalView::user_home_dir()
            .expect("home dir")
            .to_string_lossy()
            .into_owned();
        let cwd = TerminalView::resolve_preferred_working_directory(
            None,
            None,
            None,
            None,
            None,
            RuntimeWorkingDirFallback::Home,
        );
        assert_eq!(cwd.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn percentile_millis_uses_full_length_rank() {
        let samples: Vec<u32> = (1..=100).collect();

        assert_eq!(percentile_millis(&samples, 50, 100), 0.050);
        assert_eq!(percentile_millis(&samples, 95, 100), 0.095);
        assert_eq!(percentile_millis(&samples, 99, 100), 0.099);
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
                background_opacity_cells: false,
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

    #[test]
    fn terminal_content_rect_reports_right_and_bottom_edges() {
        let rect = TerminalContentRect::new(32.0, 48.0, 640.0, 420.0).expect("rect");

        assert_eq!(rect.right(), 672.0);
        assert_eq!(rect.bottom(), 468.0);
    }

    #[test]
    fn terminal_scrollbar_surface_geometry_requires_positive_size() {
        assert!(TerminalScrollbarSurfaceGeometry::new(0.0, 0.0, 0.0, 10.0).is_none());
        assert!(TerminalScrollbarSurfaceGeometry::new(0.0, 0.0, 10.0, 0.0).is_none());
    }

    #[test]
    fn terminal_query_colors_omits_cursor_without_explicit_override() {
        let colors = TerminalColors::default();
        let query_colors = TerminalView::terminal_query_colors(&colors);

        assert_eq!(query_colors.cursor, None);
    }
}
