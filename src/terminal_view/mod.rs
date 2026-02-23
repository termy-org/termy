use crate::colors::TerminalColors;
use crate::commands::{self, CommandAction};
use crate::config::{
    self, AppConfig, CursorStyle as AppCursorStyle, TabTitleConfig, TabTitleSource,
    TerminalScrollbarStyle, TerminalScrollbarVisibility,
};
use crate::keybindings;
use crate::ui::scrollbar::{ScrollbarVisibilityController, ScrollbarVisibilityMode};
use alacritty_terminal::term::cell::Flags;
use flume::{Sender, bounded};
use gpui::{
    AnyElement, App, AsyncApp, ClipboardItem, Context, Element, ExternalPaths, FocusHandle,
    Focusable, Font, FontWeight, InteractiveElement, IntoElement, KeyDownEvent, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Render, ScrollWheelEvent,
    SharedString, Size, StatefulInteractiveElement, Styled, TouchPhase, UniformListScrollHandle,
    WeakEntity, Window, WindowBackgroundAppearance, WindowControlArea, div, px,
};
use std::{
    fs,
    hash::{DefaultHasher, Hash, Hasher},
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};
use termy_search::SearchState;
use termy_terminal_ui::{
    CellRenderInfo, TabTitleShellIntegration, Terminal, TerminalCursorStyle, TerminalEvent,
    TerminalGrid, TerminalRuntimeConfig, TerminalSize,
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
mod tabs;
mod titles;
#[cfg(target_os = "macos")]
mod update_toasts;

use inline_input::{InlineInputAlignment, InlineInputState};

const MIN_FONT_SIZE: f32 = 8.0;
const MAX_FONT_SIZE: f32 = 40.0;
const ZOOM_STEP: f32 = 1.0;
const TITLEBAR_HEIGHT: f32 = 34.0;
const TABBAR_HEIGHT: f32 = 40.0;
const TITLEBAR_PLUS_SIZE: f32 = 22.0;
const WINDOWS_TITLEBAR_BUTTON_WIDTH: f32 = 46.0;
const WINDOWS_TITLEBAR_CONTROLS_WIDTH: f32 = WINDOWS_TITLEBAR_BUTTON_WIDTH * 3.0;
const TITLEBAR_SIDE_PADDING: f32 = 12.0;
const TAB_HORIZONTAL_PADDING: f32 = 12.0;
const TAB_PILL_HEIGHT: f32 = 32.0;
const TAB_PILL_NORMAL_PADDING: f32 = 10.0;
const TAB_PILL_COMPACT_PADDING: f32 = 6.0;
const TAB_PILL_COMPACT_THRESHOLD: f32 = 120.0;
const TAB_PILL_GAP: f32 = 8.0;
const TAB_CLOSE_HITBOX: f32 = 22.0;
const TAB_INACTIVE_CLOSE_MIN_WIDTH: f32 = 120.0;
const MAX_TAB_TITLE_CHARS: usize = 96;
const DEFAULT_TAB_TITLE: &str = "Terminal";
const COMMAND_TITLE_DELAY_MS: u64 = 250;
const CONFIG_WATCH_INTERVAL_MS: u64 = 750;
const CURSOR_BLINK_INTERVAL_MS: u64 = 530;
const SELECTION_BG_ALPHA: f32 = 0.35;
const DIM_TEXT_FACTOR: f32 = 0.66;
#[cfg(target_os = "macos")]
const UPDATE_BANNER_HEIGHT: f32 = 32.0;
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
const TOAST_COPY_FEEDBACK_MS: u64 = 1200;
const OVERLAY_PANEL_ALPHA_FLOOR_RATIO: f32 = 0.72;
const OVERLAY_DIM_MIN_SCALE: f32 = 0.25;
const OVERLAY_PANEL_BORDER_ALPHA: f32 = 0.24;
const OVERLAY_PRIMARY_TEXT_ALPHA: f32 = 0.95;
const OVERLAY_MUTED_TEXT_ALPHA: f32 = 0.62;
const COMMAND_PALETTE_PANEL_SOLID_ALPHA: f32 = 0.90;
const COMMAND_PALETTE_INPUT_SOLID_ALPHA: f32 = 0.76;
const COMMAND_PALETTE_ROW_SELECTED_BG_ALPHA: f32 = 0.20;
const COMMAND_PALETTE_ROW_SELECTED_BORDER_ALPHA: f32 = 0.35;
const COMMAND_PALETTE_SHORTCUT_BG_ALPHA: f32 = 0.10;
const COMMAND_PALETTE_SHORTCUT_BORDER_ALPHA: f32 = 0.22;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CellPos {
    col: usize,
    row: usize,
}

#[derive(Clone, Copy, Debug)]
struct TabBarLayout {
    tab_pill_width: f32,
    tab_padding_x: f32,
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

struct TerminalTab {
    terminal: Terminal,
    manual_title: Option<String>,
    explicit_title: Option<String>,
    shell_title: Option<String>,
    pending_command_title: Option<String>,
    pending_command_token: u64,
    title: String,
}

impl TerminalTab {
    fn new(terminal: Terminal) -> Self {
        Self {
            terminal,
            manual_title: None,
            explicit_title: None,
            shell_title: None,
            pending_command_title: None,
            pending_command_token: 0,
            title: DEFAULT_TAB_TITLE.to_string(),
        }
    }
}

enum ExplicitTitlePayload {
    Prompt(String),
    Command(String),
    Title(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HoveredLink {
    row: usize,
    start_col: usize,
    end_col: usize,
    target: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CommandPaletteMode {
    Commands,
    Themes,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CommandPaletteItemKind {
    Command(CommandAction),
    Theme(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct CommandPaletteItem {
    title: String,
    keywords: String,
    kind: CommandPaletteItemKind,
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
    active_tab: usize,
    renaming_tab: Option<usize>,
    rename_input: InlineInputState,
    event_wakeup_tx: Sender<()>,
    focus_handle: FocusHandle,
    theme_id: String,
    colors: TerminalColors,
    use_tabs: bool,
    max_tabs: usize,
    inactive_tab_scrollback: Option<usize>,
    tab_title: TabTitleConfig,
    tab_shell_integration: TabTitleShellIntegration,
    configured_working_dir: Option<String>,
    terminal_runtime: TerminalRuntimeConfig,
    config_path: Option<PathBuf>,
    config_fingerprint: Option<u64>,
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
    command_palette_open: bool,
    command_palette_mode: CommandPaletteMode,
    command_palette_input: InlineInputState,
    command_palette_filtered_items: Vec<CommandPaletteItem>,
    command_palette_selected: usize,
    command_palette_scroll_handle: UniformListScrollHandle,
    command_palette_scroll_target_y: Option<f32>,
    command_palette_scroll_max_y: f32,
    command_palette_scroll_animating: bool,
    command_palette_scroll_last_tick: Option<Instant>,
    command_palette_show_keybinds: bool,
    inline_input_selecting: bool,
    terminal_scroll_accumulator_y: f32,
    terminal_scrollbar_visibility: TerminalScrollbarVisibility,
    terminal_scrollbar_style: TerminalScrollbarStyle,
    terminal_scrollbar_visibility_controller: ScrollbarVisibilityController,
    terminal_scrollbar_animation_active: bool,
    terminal_scrollbar_drag: Option<TerminalScrollbarDragState>,
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
        }
    }

    fn config_fingerprint(path: &PathBuf) -> Option<u64> {
        let contents = fs::read(path).ok()?;
        let mut hasher = DefaultHasher::new();
        contents.hash(&mut hasher);
        Some(hasher.finish())
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
        let size = self.active_terminal().size();
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
            origin_x: padding_x,
            origin_y: self.chrome_height() + padding_y,
            width: cell_width * f32::from(size.cols),
            height: cell_height * f32::from(size.rows),
        })
    }

    pub(super) fn terminal_surface_geometry(
        &self,
        window: &Window,
    ) -> Option<TerminalViewportGeometry> {
        let viewport = window.viewport_size();
        let width: f32 = viewport.width.into();
        let viewport_height: f32 = viewport.height.into();
        let height = (viewport_height - self.chrome_height()).max(0.0);
        if width <= f32::EPSILON || height <= f32::EPSILON {
            return None;
        }

        Some(TerminalViewportGeometry {
            origin_x: 0.0,
            origin_y: self.chrome_height(),
            width,
            height,
        })
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
        let (event_wakeup_tx, event_wakeup_rx) = bounded(1);

        // Focus the terminal immediately
        focus_handle.focus(window, cx);

        // Process terminal events only when terminals signal activity.
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

        // Poll config file timestamp and hot-reload UI settings on change.
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                smol::Timer::after(Duration::from_millis(CONFIG_WATCH_INTERVAL_MS)).await;
                let result = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        if view.reload_config_if_changed(cx) {
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

        let config_path = config::ensure_config_file();
        let config_fingerprint = config_path.as_ref().and_then(Self::config_fingerprint);
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
        let terminal = Terminal::new(
            TerminalSize::default(),
            configured_working_dir.as_deref(),
            Some(event_wakeup_tx.clone()),
            Some(&tab_shell_integration),
            Some(&terminal_runtime),
        )
        .expect("Failed to create terminal");

        let mut view = Self {
            tabs: vec![TerminalTab::new(terminal)],
            active_tab: 0,
            renaming_tab: None,
            rename_input: InlineInputState::new(String::new()),
            event_wakeup_tx,
            focus_handle,
            theme_id,
            colors,
            use_tabs: config.use_tabs,
            max_tabs: config.max_tabs,
            inactive_tab_scrollback: config.inactive_tab_scrollback,
            tab_title,
            tab_shell_integration,
            configured_working_dir,
            terminal_runtime,
            config_path,
            config_fingerprint,
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
            command_palette_open: false,
            command_palette_mode: CommandPaletteMode::Commands,
            command_palette_input: InlineInputState::new(String::new()),
            command_palette_filtered_items: Vec::new(),
            command_palette_selected: 0,
            command_palette_scroll_handle: UniformListScrollHandle::new(),
            command_palette_scroll_target_y: None,
            command_palette_scroll_max_y: 0.0,
            command_palette_scroll_animating: false,
            command_palette_scroll_last_tick: None,
            command_palette_show_keybinds: config.command_palette_show_keybinds,
            inline_input_selecting: false,
            terminal_scroll_accumulator_y: 0.0,
            terminal_scrollbar_visibility: config.terminal_scrollbar_visibility,
            terminal_scrollbar_style: config.terminal_scrollbar_style,
            terminal_scrollbar_visibility_controller: ScrollbarVisibilityController::default(),
            terminal_scrollbar_animation_active: false,
            terminal_scrollbar_drag: None,
            terminal_scrollbar_marker_cache: TerminalScrollbarMarkerCache::default(),
            cell_size: None,
            search_open: false,
            search_input: InlineInputState::new(String::new()),
            search_state: SearchState::new(),
            search_debounce_token: 0,
            pending_clipboard: None,
            #[cfg(target_os = "macos")]
            auto_updater: None,
            #[cfg(target_os = "macos")]
            show_update_banner: false,
            #[cfg(target_os = "macos")]
            last_notified_update_state: None,
            #[cfg(target_os = "macos")]
            update_check_toast_id: None,
        };
        view.refresh_tab_title(0);

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
        keybindings::install_keybindings(cx, &config);
        self.theme_id = config.theme.clone();
        self.colors = TerminalColors::from_theme(&config.theme, &config.colors);
        self.use_tabs = config.use_tabs;
        self.tab_title = config.tab_title.clone();
        self.tab_shell_integration = TabTitleShellIntegration {
            enabled: self.tab_title.shell_integration,
            explicit_prefix: self.tab_title.explicit_prefix.clone(),
        };
        self.configured_working_dir = config.working_dir.clone();
        self.terminal_runtime = Self::runtime_config_from_app_config(&config);
        self.font_family = config.font_family.into();
        self.base_font_size = config.font_size.clamp(MIN_FONT_SIZE, MAX_FONT_SIZE);
        self.font_size = px(self.base_font_size);
        self.cursor_style = config.cursor_style;
        self.cursor_blink = config.cursor_blink;
        self.cursor_blink_visible = true;
        self.cell_size = None;
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
        self.command_palette_show_keybinds = config.command_palette_show_keybinds;

        for index in 0..self.tabs.len() {
            self.refresh_tab_title(index);
        }

        if self.command_palette_open {
            self.refresh_command_palette_matches(true, cx);
        }

        true
    }

    fn reload_config_if_changed(&mut self, cx: &mut Context<Self>) -> bool {
        let path = match self.config_path.clone() {
            Some(path) => path,
            None => {
                self.config_path = config::ensure_config_file();
                match self.config_path.clone() {
                    Some(path) => path,
                    None => return false,
                }
            }
        };

        let Some(fingerprint) = Self::config_fingerprint(&path) else {
            return false;
        };

        if self.config_fingerprint == Some(fingerprint) {
            return false;
        }

        self.config_fingerprint = Some(fingerprint);
        let config = AppConfig::load_or_create();
        let changed = self.apply_runtime_config(config, cx);
        if changed {
            termy_toast::info("Configuration reloaded");
        }
        changed
    }

    pub(super) fn reload_config(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = &self.config_path {
            self.config_fingerprint = Self::config_fingerprint(path);
        }
        let config = AppConfig::load_or_create();
        self.apply_runtime_config(config, cx);
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
        let mut should_redraw = false;
        let active_tab = self.active_tab;

        for index in 0..self.tabs.len() {
            let events = self.tabs[index].terminal.process_events();
            for event in events {
                match event {
                    TerminalEvent::Wakeup | TerminalEvent::Bell | TerminalEvent::Exit => {
                        if index == active_tab {
                            should_redraw = true;
                        }
                    }
                    TerminalEvent::Title(title) => {
                        if self.apply_terminal_title(index, &title, cx)
                            && (index == active_tab || self.show_tab_bar())
                        {
                            should_redraw = true;
                        }
                    }
                    TerminalEvent::ResetTitle => {
                        if self.clear_terminal_titles(index)
                            && (index == active_tab || self.show_tab_bar())
                        {
                            should_redraw = true;
                        }
                    }
                    TerminalEvent::ClipboardStore(text) => {
                        self.pending_clipboard = Some(text);
                        should_redraw = true;
                    }
                }
            }
        }

        should_redraw
    }

    fn clear_selection(&mut self) {
        self.selection_anchor = None;
        self.selection_head = None;
        self.selection_dragging = false;
        self.selection_moved = false;
    }

    fn clear_hovered_link(&mut self) -> bool {
        if self.hovered_link.is_some() {
            self.hovered_link = None;
            true
        } else {
            false
        }
    }

    fn show_tab_bar(&self) -> bool {
        self.use_tabs && self.tabs.len() > 1
    }

    fn active_terminal(&self) -> &Terminal {
        &self.tabs[self.active_tab].terminal
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
}
