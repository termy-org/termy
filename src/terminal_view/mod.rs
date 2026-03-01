use crate::colors::TerminalColors;
use crate::commands::{self, CommandAction};
use crate::config::{
    self, AppConfig, CursorStyle as AppCursorStyle, TabCloseVisibility, TabTitleConfig,
    TabTitleSource, TabWidthMode, TerminalScrollbarStyle, TerminalScrollbarVisibility,
};
use crate::keybindings;
use crate::ui::scrollbar::{ScrollbarVisibilityController, ScrollbarVisibilityMode};
use alacritty_terminal::term::cell::Flags;
use gpui::{
    AnyElement, App, AsyncApp, ClipboardItem, Context, Element, ExternalPaths, FocusHandle,
    Focusable, Font, FontWeight, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Render, ScrollWheelEvent,
    SharedString, Size, StatefulInteractiveElement, Styled, TouchPhase, WeakEntity, Window,
    WindowBackgroundAppearance, div, point, px,
};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::Mutex,
    time::{Duration, Instant},
};
use termy_search::SearchState;
use termy_terminal_ui::{
    CellRenderInfo, PaneTerminal, TabTitleShellIntegration, Terminal as NativeTerminal,
    TerminalCursorStyle, TerminalEvent, TerminalGrid, TerminalRuntimeConfig, TerminalSize, TmuxClient,
    TmuxNotification, TmuxPaneState, TmuxRuntimeConfig, TmuxSnapshot, TmuxWindowState,
    WorkingDirFallback as RuntimeWorkingDirFallback, find_link_in_line, keystroke_to_input,
};
use termy_toast::ToastManager;

#[cfg(target_os = "macos")]
use gpui::{AppContext, Entity};
#[cfg(target_os = "macos")]
use termy_auto_update::{AutoUpdater, UpdateState};

mod command_palette;
mod inline_input;
mod interaction;
mod render;
mod scrollbar;
mod search;
mod tab_strip;
mod tabs;
mod titles;
#[cfg(target_os = "macos")]
mod update_toasts;

use command_palette::{CommandPaletteMode, CommandPaletteState};
use inline_input::{InlineInputAlignment, InlineInputState};
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
const CONFIG_WATCH_INTERVAL_MS: u64 = 750;
const CURSOR_BLINK_INTERVAL_MS: u64 = 530;
const TMUX_POLL_INTERVAL_MS: u64 = 16;
const TMUX_TITLE_REFRESH_DEBOUNCE_MS: u64 = 120;
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
const INPUT_SCROLL_SUPPRESS_MS: u64 = 160;
const TOAST_COPY_FEEDBACK_MS: u64 = 1200;
const OVERLAY_PANEL_ALPHA_FLOOR_RATIO: f32 = 0.72;
const OVERLAY_DIM_MIN_SCALE: f32 = 0.25;
const OVERLAY_PANEL_BORDER_ALPHA: f32 = 0.24;
const OVERLAY_PRIMARY_TEXT_ALPHA: f32 = 0.95;
const OVERLAY_MUTED_TEXT_ALPHA: f32 = 0.62;
const COMMAND_PALETTE_PANEL_SOLID_ALPHA: f32 = 0.90;
const COMMAND_PALETTE_INPUT_SOLID_ALPHA: f32 = 0.76;
const COMMAND_PALETTE_ROW_SELECTED_BG_ALPHA: f32 = 0.20;
const COMMAND_PALETTE_SHORTCUT_BG_ALPHA: f32 = 0.10;
const COMMAND_PALETTE_SHORTCUT_TEXT_ALPHA: f32 = 0.80;
const COMMAND_PALETTE_DIM_ALPHA: f32 = 0.78;
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

type TabId = u64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CellPos {
    col: usize,
    row: usize,
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
enum TmuxSnapshotRefreshMode {
    None,
    Debounced,
    Immediate,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TerminalRuntimeMode {
    Native,
    Tmux,
}

#[derive(Clone, Debug)]
struct PaneResizeDragState {
    pane_id: String,
    axis: PaneResizeAxis,
    start_x: f32,
    start_y: f32,
    applied_steps: i32,
}

#[derive(Clone, Copy, Debug)]
struct TerminalScrollbarHit {
    local_y: f32,
    thumb_hit: bool,
    thumb_top: f32,
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

enum Terminal {
    Tmux(PaneTerminal),
    Native(Mutex<NativeTerminal>),
}

impl Terminal {
    fn new_tmux(size: TerminalSize, scrollback_history: usize) -> Self {
        Self::Tmux(PaneTerminal::new(size, scrollback_history))
    }

    fn new_native(
        size: TerminalSize,
        configured_working_dir: Option<&str>,
        tab_title_shell_integration: Option<&TabTitleShellIntegration>,
        runtime_config: Option<&TerminalRuntimeConfig>,
    ) -> anyhow::Result<Self> {
        Ok(Self::Native(Mutex::new(NativeTerminal::new(
            size,
            configured_working_dir,
            None,
            tab_title_shell_integration,
            runtime_config,
        )?)))
    }

    fn feed_output(&self, bytes: &[u8]) {
        if let Self::Tmux(terminal) = self {
            terminal.feed_output(bytes);
        }
    }

    fn write_input(&self, input: &[u8]) {
        if let Self::Native(terminal) = self {
            if let Ok(terminal) = terminal.lock() {
                terminal.write(input);
            }
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

    fn size(&self) -> TerminalSize {
        match self {
            Self::Tmux(terminal) => terminal.size(),
            Self::Native(terminal) => terminal
                .lock()
                .map(|terminal| terminal.size())
                .unwrap_or_default(),
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

    fn cursor_position(&self) -> (usize, usize) {
        match self {
            Self::Tmux(terminal) => terminal.cursor_position(),
            Self::Native(terminal) => terminal
                .lock()
                .map(|terminal| terminal.cursor_position())
                .unwrap_or((0, 0)),
        }
    }

    fn set_scrollback_history(&self, history_size: usize) {
        match self {
            Self::Tmux(terminal) => terminal.set_scrollback_history(history_size),
            Self::Native(terminal) => {
                if let Ok(terminal) = terminal.lock() {
                    terminal.set_scrollback_history(history_size);
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
}

struct TerminalPane {
    id: String,
    left: u16,
    top: u16,
    width: u16,
    height: u16,
    terminal: Terminal,
}

impl TerminalPane {
    fn from_tmux_state(state: &TmuxPaneState, terminal: Terminal) -> Self {
        Self {
            id: state.id.clone(),
            left: state.left,
            top: state.top,
            width: state.width,
            height: state.height,
            terminal,
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
    title: String,
    title_text_width: f32,
    sticky_title_width: f32,
    display_width: f32,
    running_process: bool,
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
            title,
            title_text_width,
            sticky_title_width,
            display_width,
            running_process: false,
        }
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

fn adaptive_overlay_dim_alpha_for_opacity(base_alpha: f32, background_opacity: f32) -> f32 {
    let opacity = background_opacity_factor(background_opacity);
    let scale = OVERLAY_DIM_MIN_SCALE + (1.0 - OVERLAY_DIM_MIN_SCALE) * opacity;
    (base_alpha * scale).clamp(0.0, base_alpha)
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

    fn dim_background(self, base_alpha: f32) -> gpui::Rgba {
        let alpha = adaptive_overlay_dim_alpha_for_opacity(base_alpha, self.background_opacity);
        self.with_alpha(self.colors.background, alpha)
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
    next_tab_id: TabId,
    active_tab: usize,
    renaming_tab: Option<usize>,
    rename_input: InlineInputState,
    focus_handle: FocusHandle,
    theme_id: String,
    colors: TerminalColors,
    inactive_tab_scrollback: Option<usize>,
    warn_on_quit_with_running_process: bool,
    tab_title: TabTitleConfig,
    tab_close_visibility: TabCloseVisibility,
    tab_width_mode: TabWidthMode,
    show_termy_in_titlebar: bool,
    tab_shell_integration: TabTitleShellIntegration,
    configured_working_dir: Option<String>,
    terminal_runtime: TerminalRuntimeConfig,
    runtime_mode: TerminalRuntimeMode,
    tmux_runtime: Option<TmuxRuntimeConfig>,
    tmux_client: Option<TmuxClient>,
    tmux_client_cols: u16,
    tmux_client_rows: u16,
    tmux_title_refresh_deadline: Option<Instant>,
    tmux_enabled_config: bool,
    config_path: Option<PathBuf>,
    config_fingerprint: Option<u64>,
    last_config_error_message: Option<String>,
    font_family: SharedString,
    base_font_size: f32,
    font_size: Pixels,
    cursor_style: AppCursorStyle,
    cursor_blink: bool,
    cursor_blink_visible: bool,
    background_opacity: f32,
    background_blur: bool,
    background_support_context: BackgroundSupportContext,
    last_window_background_appearance: Option<WindowBackgroundAppearance>,
    warned_blur_unsupported_once: bool,
    padding_x: f32,
    padding_y: f32,
    mouse_scroll_multiplier: f32,
    line_height: f32,
    selection_anchor: Option<CellPos>,
    selection_head: Option<CellPos>,
    selection_dragging: bool,
    selection_moved: bool,
    hovered_link: Option<HoveredLink>,
    hovered_toast: Option<u64>,
    copied_toast_feedback: Option<(u64, Instant)>,
    toast_animation_scheduled: bool,
    toast_manager: ToastManager,
    command_palette: CommandPaletteState,
    install_cli_available: bool,
    tab_strip: TabStripState,
    inline_input_selecting: bool,
    terminal_scroll_accumulator_y: f32,
    input_scroll_suppress_until: Option<Instant>,
    terminal_scrollbar_visibility: TerminalScrollbarVisibility,
    terminal_scrollbar_style: TerminalScrollbarStyle,
    terminal_scrollbar_visibility_controller: ScrollbarVisibilityController,
    terminal_scrollbar_animation_active: bool,
    terminal_scrollbar_drag: Option<TerminalScrollbarDragState>,
    pane_resize_drag: Option<PaneResizeDragState>,
    terminal_scrollbar_marker_cache: TerminalScrollbarMarkerCache,
    /// Cached cell dimensions
    cell_size: Option<Size<Pixels>>,
    // Search state
    search_open: bool,
    search_input: InlineInputState,
    search_state: SearchState,
    search_debounce_token: u64,
    // Pending clipboard write from OSC 52
    pending_clipboard: Option<String>,
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

        let mut runtime = TerminalRuntimeConfig::default();
        runtime.working_dir_fallback = working_dir_fallback;
        runtime.scrollback_history = config.scrollback_history;
        runtime
    }

    fn runtime_mode_from_app_config(config: &AppConfig) -> TerminalRuntimeMode {
        if config.tmux_enabled {
            TerminalRuntimeMode::Tmux
        } else {
            TerminalRuntimeMode::Native
        }
    }

    fn tmux_runtime_from_app_config(config: &AppConfig) -> TmuxRuntimeConfig {
        TmuxRuntimeConfig {
            persistence: config.tmux_persistence,
            binary: config.tmux_binary.trim().to_string(),
        }
    }

    fn runtime_uses_tmux(&self) -> bool {
        self.runtime_mode == TerminalRuntimeMode::Tmux
    }

    fn tmux_client(&self) -> Option<&TmuxClient> {
        self.tmux_client.as_ref()
    }

    fn tmux_client_required(&self) -> &TmuxClient {
        self.tmux_client
            .as_ref()
            .expect("tmux client must exist while tmux runtime is active")
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
    ) -> Terminal {
        let terminal = Terminal::new_tmux(
            Self::terminal_size_for_pane_state(pane, cell_size),
            scrollback_history,
        );

        if let Ok(capture) = tmux_client.capture_pane_viewport(&pane.id, pane.height.max(1)) {
            terminal.feed_output(&capture);
            let cursor_row = pane.cursor_y.min(pane.height.saturating_sub(1)).saturating_add(1);
            let cursor_col = pane.cursor_x.min(pane.width.saturating_sub(1)).saturating_add(1);
            let cursor_escape = format!("\u{1b}[{};{}H", cursor_row, cursor_col);
            terminal.feed_output(cursor_escape.as_bytes());
        }

        terminal
    }

    fn create_native_tab(terminal: Terminal, cols: u16, rows: u16) -> TerminalTab {
        let title = DEFAULT_TAB_TITLE.to_string();
        let title_text_width = 0.0;
        let sticky_title_width =
            Self::tab_display_width_for_text_px_without_close_with_max(title_text_width, TAB_MAX_WIDTH);
        let display_width =
            Self::tab_display_width_for_text_px_with_max(title_text_width, TAB_MAX_WIDTH);
        let pane_id = "%native-1".to_string();
        let pane = TerminalPane {
            id: pane_id.clone(),
            left: 0,
            top: 0,
            width: cols.max(1),
            height: rows.max(1),
            terminal,
        };
        TerminalTab {
            id: 1,
            window_id: "@native-1".to_string(),
            window_index: 0,
            panes: vec![pane],
            active_pane_id: pane_id,
            manual_title: None,
            explicit_title: None,
            shell_title: None,
            title,
            title_text_width,
            sticky_title_width,
            display_width,
            running_process: false,
        }
    }

    fn apply_tmux_snapshot(&mut self, snapshot: TmuxSnapshot) {
        let previous_active_window_id = self.tabs.get(self.active_tab).map(|tab| tab.window_id.clone());
        let previous_ids = self
            .tabs
            .iter()
            .map(|tab| (tab.window_id.clone(), tab.id))
            .collect::<std::collections::HashMap<_, _>>();

        let mut existing_terminals = std::collections::HashMap::<String, Terminal>::new();
        for mut tab in std::mem::take(&mut self.tabs) {
            for pane in tab.panes.drain(..) {
                existing_terminals.insert(pane.id.clone(), pane.terminal);
            }
        }

        let mut new_tabs = Vec::new();
        for window in &snapshot.windows {
            let mut panes = Vec::new();
            for pane_state in &window.panes {
                let terminal = if let Some(existing) = existing_terminals.remove(&pane_state.id) {
                    existing
                } else {
                    Self::hydrate_pane_terminal(
                        self.tmux_client_required(),
                        pane_state,
                        self.terminal_runtime.scrollback_history,
                        self.cell_size,
                    )
                };
                let next_size = Self::terminal_size_for_pane_state(pane_state, self.cell_size);
                let current_size = terminal.size();
                if current_size.cols != next_size.cols
                    || current_size.rows != next_size.rows
                    || current_size.cell_width != next_size.cell_width
                    || current_size.cell_height != next_size.cell_height
                {
                    terminal.resize(next_size);
                }
                panes.push(TerminalPane::from_tmux_state(pane_state, terminal));
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
            let shell_title = active_pane_state
                .and_then(|pane| Self::derive_tmux_shell_title(&self.tab_title, pane));
            let running_process = active_pane_state
                .is_some_and(|pane| !Self::is_shell_command(pane.current_command.as_str()));

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
    }

    fn refresh_tmux_snapshot(&mut self) -> bool {
        if self.tmux_client.is_none() {
            return false;
        }
        match self.tmux_client_required().refresh_snapshot() {
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

    fn refresh_tmux_snapshot_for_client_size(&mut self, cols: u16, rows: u16) -> bool {
        const MAX_ATTEMPTS: usize = 6;
        const RETRY_DELAY_MS: u64 = 12;

        if self.tmux_client.is_none() {
            return false;
        }
        let mut applied = false;
        for attempt in 0..MAX_ATTEMPTS {
            match self.tmux_client_required().refresh_snapshot() {
                Ok(snapshot) => {
                    let converged = Self::snapshot_matches_client_size(&snapshot, cols, rows);
                    self.apply_tmux_snapshot(snapshot);
                    applied = true;
                    if converged {
                        return true;
                    }
                    if attempt + 1 < MAX_ATTEMPTS {
                        std::thread::sleep(Duration::from_millis(RETRY_DELAY_MS));
                    }
                }
                Err(error) => {
                    termy_toast::error(format!("tmux sync failed: {error}"));
                    return applied;
                }
            }
        }

        applied
    }
    fn pane_terminal_by_id(&self, pane_id: &str) -> Option<&Terminal> {
        self.tabs
            .iter()
            .flat_map(|tab| tab.panes.iter())
            .find(|pane| pane.id == pane_id)
            .map(|pane| &pane.terminal)
    }

    fn pane_ref_by_id(&self, pane_id: &str) -> Option<&TerminalPane> {
        self.tabs
            .iter()
            .flat_map(|tab| tab.panes.iter())
            .find(|pane| pane.id == pane_id)
    }

    fn is_active_pane_id(&self, pane_id: &str) -> bool {
        self.tabs
            .get(self.active_tab)
            .and_then(|tab| tab.active_pane_id())
            == Some(pane_id)
    }

    fn active_pane_id(&self) -> Option<&str> {
        self.tabs.get(self.active_tab).and_then(|tab| tab.active_pane_id())
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
        background_opacity_factor(self.background_opacity)
    }

    fn scaled_background_alpha(&self, base_alpha: f32) -> f32 {
        scaled_background_alpha_for_opacity(base_alpha, self.background_opacity)
    }

    fn scaled_chrome_alpha(&self, base_alpha: f32) -> f32 {
        scaled_chrome_alpha_for_opacity(base_alpha, self.background_opacity)
    }

    fn effective_terminal_padding(&self) -> (f32, f32) {
        if self.active_terminal().alternate_screen_mode() {
            (0.0, 0.0)
        } else {
            (self.padding_x, self.padding_y)
        }
    }

    fn overlay_style(&self) -> OverlayStyleBuilder<'_> {
        OverlayStyleBuilder::new(&self.colors, self.background_opacity)
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
        let size = self.active_terminal().size();
        let viewport_rows = size.rows as usize;
        if viewport_rows == 0 {
            return None;
        }

        let line_height: f32 = size.cell_height.into();
        let (display_offset, history_size) = self.active_terminal().scroll_state();
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
        let cell_width: f32 = size.cell_width.into();
        let cell_height: f32 = size.cell_height.into();
        if cell_width <= f32::EPSILON || cell_height <= f32::EPSILON {
            return None;
        }

        Some(TerminalViewportGeometry {
            origin_x: padding_x + (f32::from(pane.left) * cell_width),
            origin_y: padding_y + (f32::from(pane.top) * cell_height),
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

    pub(super) fn clear_terminal_scrollbar_marker_cache(&mut self) {
        self.terminal_scrollbar_marker_cache.clear();
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
            self.background_opacity,
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
        let config_change_rx = config::subscribe_config_changes();

        // Focus the terminal immediately
        focus_handle.focus(window, cx);

        // Poll tmux control-mode notifications.
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                smol::Timer::after(Duration::from_millis(TMUX_POLL_INTERVAL_MS)).await;
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

        // Reload immediately when config is updated in-process (e.g. settings/theme actions).
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            while config_change_rx.recv_async().await.is_ok() {
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
        let runtime_mode = Self::runtime_mode_from_app_config(&config);
        let initial_cols = TerminalSize::default().cols;
        let initial_rows = TerminalSize::default().rows;
        let (tmux_runtime, tmux_client, initial_snapshot, native_terminal) = match runtime_mode {
            TerminalRuntimeMode::Tmux => {
                let tmux_runtime = Self::tmux_runtime_from_app_config(&config);
                let tmux_client = match TmuxClient::new(tmux_runtime.clone(), initial_cols, initial_rows) {
                    Ok(client) => client,
                    Err(error) => {
                        eprintln!("Termy startup blocked: failed to start tmux control runtime: {error}");
                        std::process::exit(1);
                    }
                };
                let initial_snapshot = match tmux_client.refresh_snapshot() {
                    Ok(snapshot) => snapshot,
                    Err(error) => {
                        eprintln!("Termy startup blocked: failed to fetch initial tmux snapshot: {error}");
                        std::process::exit(1);
                    }
                };
                (Some(tmux_runtime), Some(tmux_client), Some(initial_snapshot), None)
            }
            TerminalRuntimeMode::Native => {
                let native_terminal = match Terminal::new_native(
                    TerminalSize {
                        cols: initial_cols,
                        rows: initial_rows,
                        ..TerminalSize::default()
                    },
                    configured_working_dir.as_deref(),
                    Some(&tab_shell_integration),
                    Some(&terminal_runtime),
                ) {
                    Ok(terminal) => terminal,
                    Err(error) => {
                        eprintln!("Termy startup blocked: failed to start native runtime: {error}");
                        std::process::exit(1);
                    }
                };
                (None, None, None, Some(native_terminal))
            }
        };

        let mut view = Self {
            tabs: Vec::new(),
            next_tab_id: 1,
            active_tab: 0,
            renaming_tab: None,
            rename_input: InlineInputState::new(String::new()),
            focus_handle,
            theme_id,
            colors,
            inactive_tab_scrollback: config.inactive_tab_scrollback,
            warn_on_quit_with_running_process: config.warn_on_quit_with_running_process,
            tab_title,
            tab_close_visibility: config.tab_close_visibility,
            tab_width_mode: config.tab_width_mode,
            show_termy_in_titlebar: config.show_termy_in_titlebar,
            tab_shell_integration,
            configured_working_dir,
            terminal_runtime,
            runtime_mode,
            tmux_runtime,
            tmux_client,
            tmux_client_cols: initial_cols,
            tmux_client_rows: initial_rows,
            tmux_title_refresh_deadline: None,
            tmux_enabled_config: config.tmux_enabled,
            config_path,
            config_fingerprint,
            last_config_error_message,
            font_family: config.font_family.into(),
            base_font_size,
            font_size: px(base_font_size),
            cursor_style: config.cursor_style,
            cursor_blink: config.cursor_blink,
            cursor_blink_visible: true,
            background_opacity: config.background_opacity,
            background_blur: config.background_blur,
            background_support_context,
            last_window_background_appearance: None,
            warned_blur_unsupported_once: false,
            padding_x,
            padding_y,
            mouse_scroll_multiplier: config.mouse_scroll_multiplier,
            line_height: 1.4,
            selection_anchor: None,
            selection_head: None,
            selection_dragging: false,
            selection_moved: false,
            hovered_link: None,
            hovered_toast: None,
            copied_toast_feedback: None,
            toast_animation_scheduled: false,
            toast_manager: ToastManager::new(),
            command_palette: CommandPaletteState::new(config.command_palette_show_keybinds),
            install_cli_available: Self::install_cli_available_from_system(),
            tab_strip: TabStripState::new(),
            inline_input_selecting: false,
            terminal_scroll_accumulator_y: 0.0,
            input_scroll_suppress_until: None,
            terminal_scrollbar_visibility: config.terminal_scrollbar_visibility,
            terminal_scrollbar_style: config.terminal_scrollbar_style,
            terminal_scrollbar_visibility_controller: ScrollbarVisibilityController::default(),
            terminal_scrollbar_animation_active: false,
            terminal_scrollbar_drag: None,
            pane_resize_drag: None,
            terminal_scrollbar_marker_cache: TerminalScrollbarMarkerCache::default(),
            cell_size: None,
            search_open: false,
            search_input: InlineInputState::new(String::new()),
            search_state: SearchState::new(),
            search_debounce_token: 0,
            pending_clipboard: None,
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
        match initial_snapshot {
            Some(initial_snapshot) => view.apply_tmux_snapshot(initial_snapshot),
            None => {
                if let Some(native_terminal) = native_terminal {
                    view.tabs = vec![Self::create_native_tab(
                        native_terminal,
                        initial_cols,
                        initial_rows,
                    )];
                    view.next_tab_id = 2;
                    view.active_tab = 0;
                    view.refresh_tab_title(0);
                    view.mark_tab_strip_layout_dirty();
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            let updater = cx.new(|_| AutoUpdater::new(crate::APP_VERSION));
            cx.observe(&updater, |_, _, cx| cx.notify()).detach();
            let weak = updater.downgrade();
            cx.spawn(async move |_this: WeakEntity<Self>, cx: &mut AsyncApp| {
                smol::Timer::after(Duration::from_millis(5000)).await;
                let _ = cx.update(|cx| AutoUpdater::check(weak, cx));
            })
            .detach();
            view.auto_updater = Some(updater);
        }

        view
    }

    fn apply_runtime_config(&mut self, config: AppConfig, cx: &mut Context<Self>) -> bool {
        keybindings::install_keybindings(cx, &config, self.runtime_uses_tmux());
        let previous_font_family = self.font_family.clone();
        let previous_font_size = self.font_size;
        self.theme_id = config.theme.clone();
        self.colors = TerminalColors::from_theme(&config.theme, &config.colors);
        self.inactive_tab_scrollback = config.inactive_tab_scrollback;
        self.warn_on_quit_with_running_process = config.warn_on_quit_with_running_process;
        self.tab_title = config.tab_title.clone();
        let tab_close_visibility_changed = self.tab_close_visibility != config.tab_close_visibility;
        let tab_width_mode_changed = self.tab_width_mode != config.tab_width_mode;
        let show_termy_in_titlebar_changed =
            self.show_termy_in_titlebar != config.show_termy_in_titlebar;
        self.tab_close_visibility = config.tab_close_visibility;
        self.tab_width_mode = config.tab_width_mode;
        self.show_termy_in_titlebar = config.show_termy_in_titlebar;
        self.tab_shell_integration = TabTitleShellIntegration {
            enabled: self.tab_title.shell_integration,
            explicit_prefix: self.tab_title.explicit_prefix.clone(),
        };
        let next_runtime_mode = Self::runtime_mode_from_app_config(&config);
        let tmux_enabled_changed = config.tmux_enabled != self.tmux_enabled_config;
        if next_runtime_mode != self.runtime_mode && tmux_enabled_changed {
            termy_toast::info("Runtime change saved. Restart Termy to apply tmux mode updates.");
        }
        self.tmux_enabled_config = config.tmux_enabled;
        self.configured_working_dir = config.working_dir.clone();
        self.terminal_runtime = Self::runtime_config_from_app_config(&config);
        if self.runtime_uses_tmux() {
            let next_tmux_runtime = Some(Self::tmux_runtime_from_app_config(&config));
            if next_tmux_runtime != self.tmux_runtime {
                let runtime = next_tmux_runtime.clone().expect("tmux runtime missing");
                match TmuxClient::new(
                    runtime,
                    self.tmux_client_cols.max(1),
                    self.tmux_client_rows.max(1),
                ) {
                    Ok(client) => {
                        self.tmux_runtime = next_tmux_runtime;
                        self.tmux_client = Some(client);
                        self.tmux_title_refresh_deadline = None;
                        let _ = self.refresh_tmux_snapshot();
                    }
                    Err(error) => {
                        termy_toast::error(format!("tmux reconnect failed: {error}"));
                    }
                }
            }
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
        self.background_blur = config.background_blur;
        self.padding_x = config.padding_x.max(0.0);
        self.padding_y = config.padding_y.max(0.0);
        self.mouse_scroll_multiplier = config.mouse_scroll_multiplier;
        if self.terminal_scrollbar_visibility != config.terminal_scrollbar_visibility {
            self.terminal_scrollbar_visibility = config.terminal_scrollbar_visibility;
            self.terminal_scrollbar_visibility_controller.reset();
            self.terminal_scrollbar_drag = None;
            self.terminal_scrollbar_animation_active = false;
            self.clear_terminal_scrollbar_marker_cache();
        }
        self.terminal_scrollbar_style = config.terminal_scrollbar_style;
        self.set_command_palette_show_keybinds(config.command_palette_show_keybinds);
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

        for index in 0..self.tabs.len() {
            self.refresh_tab_title(index);
        }
        if tab_close_visibility_changed || tab_width_mode_changed || show_termy_in_titlebar_changed
        {
            self.mark_tab_strip_layout_dirty();
        }

        if self.is_command_palette_open() {
            self.refresh_command_palette_matches(true, cx);
        }

        true
    }

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

    pub(super) fn schedule_tmux_title_refresh(&mut self) {
        self.tmux_title_refresh_deadline = Some(
            Instant::now() + Duration::from_millis(TMUX_TITLE_REFRESH_DEBOUNCE_MS),
        );
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

    fn process_terminal_events(&mut self, cx: &mut Context<Self>) -> bool {
        if self.runtime_uses_tmux() {
            let Some(tmux_client) = self.tmux_client.as_ref() else {
                return false;
            };
            let mut should_redraw = false;
            let mut needs_refresh = false;

            for notification in tmux_client.poll_notifications() {
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
                    TmuxNotification::Exit(reason) => {
                        let reason =
                            reason.unwrap_or_else(|| "tmux control mode exited".to_string());
                        termy_toast::error(reason);
                        cx.quit();
                    }
                }
            }

            let now = Instant::now();
            match Self::tmux_snapshot_refresh_mode(needs_refresh, self.tmux_title_refresh_deadline, now)
            {
                TmuxSnapshotRefreshMode::Immediate | TmuxSnapshotRefreshMode::Debounced => {
                    self.tmux_title_refresh_deadline = None;
                    if self.refresh_tmux_snapshot() {
                        should_redraw = true;
                    }
                }
                TmuxSnapshotRefreshMode::None => {}
            }

            should_redraw
        } else {
            let mut should_redraw = false;
            let events = self.active_terminal().process_events();
            for event in events {
                match event {
                    TerminalEvent::Wakeup => should_redraw = true,
                    TerminalEvent::Title(title) => {
                        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                            tab.shell_title = Some(Self::truncate_tab_title(title.as_str()));
                            tab.running_process = false;
                            self.refresh_tab_title(self.active_tab);
                            should_redraw = true;
                        }
                    }
                    TerminalEvent::ResetTitle => {
                        if let Some(tab) = self.tabs.get_mut(self.active_tab) {
                            tab.shell_title = None;
                            self.refresh_tab_title(self.active_tab);
                            should_redraw = true;
                        }
                    }
                    TerminalEvent::Bell => {}
                    TerminalEvent::Exit => {
                        cx.quit();
                    }
                    TerminalEvent::ClipboardStore(text) => {
                        self.pending_clipboard = Some(text);
                        should_redraw = true;
                    }
                }
            }
            should_redraw
        }
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

    fn active_terminal(&self) -> &Terminal {
        self.tabs
            .get(self.active_tab)
            .and_then(TerminalTab::active_terminal)
            .expect("active pane terminal missing")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn overlay_dim_gets_stronger_as_opacity_decreases() {
        let base = 0.78;
        let high_opacity = adaptive_overlay_dim_alpha_for_opacity(base, 1.0);
        let low_opacity = adaptive_overlay_dim_alpha_for_opacity(base, 0.2);
        assert!(low_opacity < high_opacity);
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
    fn runtime_mode_follows_tmux_enabled_flag() {
        let mut config = AppConfig::default();
        config.tmux_enabled = false;
        assert_eq!(
            TerminalView::runtime_mode_from_app_config(&config),
            TerminalRuntimeMode::Native
        );

        config.tmux_enabled = true;
        assert_eq!(
            TerminalView::runtime_mode_from_app_config(&config),
            TerminalRuntimeMode::Tmux
        );
    }

    #[test]
    fn create_native_tab_starts_with_one_full_size_pane() {
        let terminal = Terminal::new_tmux(TerminalSize::default(), 2000);
        let tab = TerminalView::create_native_tab(terminal, 120, 42);

        assert_eq!(tab.panes.len(), 1);
        assert_eq!(tab.window_id, "@native-1");
        assert_eq!(tab.window_index, 0);
        assert_eq!(tab.active_pane_id, "%native-1");

        let pane = &tab.panes[0];
        assert_eq!(pane.id, "%native-1");
        assert_eq!(pane.left, 0);
        assert_eq!(pane.top, 0);
        assert_eq!(pane.width, 120);
        assert_eq!(pane.height, 42);
    }
}
