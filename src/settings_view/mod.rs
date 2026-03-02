use crate::colors::TerminalColors;
use crate::config::{self, AppConfig};
use crate::text_input::{TextInputAlignment, TextInputElement, TextInputProvider, TextInputState};
use crate::ui::scrollbar::{self as ui_scrollbar, ScrollbarPaintStyle, ScrollbarRange};
use gpui::{
    AnyElement, AsyncApp, Context, FocusHandle, Font, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Render,
    Rgba, ScrollAnchor, ScrollHandle, ScrollWheelEvent, SharedString, StatefulInteractiveElement,
    Styled, TextAlign, WeakEntity, Window, WindowBackgroundAppearance, deferred, div, point,
    prelude::FluentBuilder, px,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use termy_config_core::{
    RootSettingId, RootSettingValueKind, SettingsSection as CoreSettingsSection,
    color_setting_from_key, color_setting_specs, root_setting_default_value,
    root_setting_enum_choices, root_setting_from_key, root_setting_specs, root_setting_value_kind,
};
use termy_command_core::CommandId;

mod colors;
mod components;
mod input_mode;
mod keybinds;
mod search;
mod sections;
mod state;
mod state_apply;
mod style;

use self::search::SearchableSetting;
use self::state::{ActiveTextInput, DropdownOption, EditableField};
use input_mode::KeyInputMode;

const SIDEBAR_WIDTH: f32 = 220.0;
const SETTINGS_CONTROL_WIDTH: f32 = 360.0;
const SETTINGS_CONTROL_HEIGHT: f32 = 36.0;
const NUMERIC_STEP_BUTTON_SIZE: f32 = 26.0;
const SETTINGS_INPUT_TEXT_SIZE: f32 = 13.0;
const SETTINGS_CONFIG_WATCH_INTERVAL_MS: u64 = 750;
const SETTINGS_SEARCH_NAV_THROTTLE_MS: u64 = 70;
const SETTINGS_SCROLL_ANIMATION_DURATION_MS: u64 = 170;
const SETTINGS_SCROLL_ANIMATION_TICK_MS: u64 = 16;
const SETTINGS_SCROLLBAR_WIDTH: f32 = 8.0;
const SETTINGS_SCROLLBAR_MIN_THUMB_HEIGHT: f32 = 18.0;
const SETTINGS_SCROLLBAR_TRACK_ALPHA: f32 = 0.10;
const SETTINGS_SCROLLBAR_THUMB_ALPHA: f32 = 0.42;
const SETTINGS_SCROLLBAR_THUMB_ACTIVE_ALPHA: f32 = 0.58;
const SETTINGS_OVERLAY_PANEL_ALPHA_FLOOR_RATIO: f32 = 0.72;
const SETTINGS_SWITCH_WIDTH: f32 = 48.0;
const SETTINGS_SWITCH_HEIGHT: f32 = 26.0;
const SETTINGS_SWITCH_KNOB_SIZE: f32 = 20.0;
const SETTINGS_SEARCH_PREVIEW_LIMIT: usize = 6;
const SETTINGS_SLIDER_VALUE_WIDTH: f32 = 60.0;
const SETTINGS_OPACITY_STEP_RATIO: f32 = 0.05;
const SETTINGS_CONTROL_INNER_PADDING: f32 = 8.0;
const SETTINGS_OPACITY_CONTROL_GAP: f32 = 6.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SettingsSection {
    Appearance,
    Terminal,
    Tabs,
    Advanced,
    Colors,
    Keybindings,
}

pub struct SettingsWindow {
    active_section: SettingsSection,
    config: AppConfig,
    config_path: Option<PathBuf>,
    config_fingerprint: Option<u64>,
    last_config_error_message: Option<String>,
    available_font_families: Vec<String>,
    focus_handle: FocusHandle,
    active_input: Option<ActiveTextInput>,
    content_scroll_handle: ScrollHandle,
    setting_scroll_anchors: HashMap<&'static str, ScrollAnchor>,
    searchable_settings: Vec<SearchableSetting>,
    searchable_setting_indices: HashMap<&'static str, usize>,
    sidebar_search_state: TextInputState,
    sidebar_search_active: bool,
    sidebar_search_selecting: bool,
    search_navigation_last_target: Option<&'static str>,
    search_navigation_last_jump_at: Option<Instant>,
    capturing_action: Option<CommandId>,
    background_opacity_drag_anchor: Option<(f32, f32)>,
    scroll_animation_token: u64,
    colors: TerminalColors,
    last_window_background_appearance: Option<WindowBackgroundAppearance>,
}

impl SettingsWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let mut last_config_error_message = None;
        let loaded = config::load_runtime_config(
            &mut last_config_error_message,
            "Failed to load config for settings view",
        );
        let config = loaded.config;
        let config_path = loaded.path;
        let config_fingerprint = loaded.fingerprint;
        let config_change_rx = config::subscribe_config_changes();
        let mut available_font_families = window.text_system().all_font_names();
        available_font_families.sort_unstable_by_key(|font| font.to_ascii_lowercase());
        available_font_families.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        let colors = TerminalColors::from_theme(&config.theme, &config.colors);
        let searchable_settings = Self::build_searchable_settings();
        let searchable_setting_indices =
            Self::build_searchable_setting_indices(&searchable_settings);
        let content_scroll_handle = ScrollHandle::new();
        let setting_scroll_anchors = Self::build_setting_scroll_anchors(&content_scroll_handle);
        let view = Self {
            active_section: SettingsSection::Appearance,
            config,
            config_path,
            config_fingerprint,
            last_config_error_message,
            available_font_families,
            focus_handle: cx.focus_handle(),
            active_input: None,
            content_scroll_handle,
            setting_scroll_anchors,
            searchable_settings,
            searchable_setting_indices,
            sidebar_search_state: TextInputState::new(String::new()),
            sidebar_search_active: true,
            sidebar_search_selecting: false,
            search_navigation_last_target: None,
            search_navigation_last_jump_at: None,
            capturing_action: None,
            background_opacity_drag_anchor: None,
            scroll_animation_token: 0,
            colors,
            last_window_background_appearance: None,
        };
        view.focus_handle.focus(window, cx);

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            while config_change_rx.recv_async().await.is_ok() {
                while config_change_rx.try_recv().is_ok() {}
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

        // Fallback polling in case filesystem notifications are coalesced/missed.
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            loop {
                smol::Timer::after(Duration::from_millis(SETTINGS_CONFIG_WATCH_INTERVAL_MS)).await;
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

        view
    }

    fn apply_runtime_config(&mut self, config: AppConfig) -> bool {
        self.colors = TerminalColors::from_theme(&config.theme, &config.colors);
        self.config = config;
        true
    }

    fn reload_config_if_changed(&mut self, _cx: &mut Context<Self>) -> bool {
        let path = match self.config_path.clone() {
            Some(path) => path,
            None => {
                let loaded = config::load_runtime_config(
                    &mut self.last_config_error_message,
                    "Failed to reload config for settings view",
                );
                self.config_path = loaded.path;
                self.config_fingerprint = loaded.fingerprint;
                return if loaded.loaded_from_disk {
                    self.apply_runtime_config(loaded.config)
                } else {
                    false
                };
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
            "Failed to reload config for settings view",
        );
        self.config_path = loaded.path;
        self.config_fingerprint = loaded.fingerprint;
        if loaded.loaded_from_disk {
            self.apply_runtime_config(loaded.config)
        } else {
            false
        }
    }

    fn key_requires_secondary_only(event: &KeyDownEvent, key: &str) -> bool {
        (event.keystroke.modifiers.secondary() || event.keystroke.modifiers.control)
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.function
            && event.keystroke.key.eq_ignore_ascii_case(key)
    }

    fn cmd_only(modifiers: gpui::Modifiers) -> bool {
        modifiers.secondary() && !modifiers.alt && !modifiers.function
    }

    fn handle_global_shortcuts(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if Self::key_requires_secondary_only(event, "q") {
            cx.quit();
            return true;
        }

        if Self::key_requires_secondary_only(event, "w") {
            window.remove_window();
            return true;
        }

        false
    }

    fn handle_section_cycle_shortcut(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.active_input.is_none()
            && event.keystroke.key.eq_ignore_ascii_case("tab")
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.function
            && !event.keystroke.modifiers.platform
        {
            self.cycle_active_section(event.keystroke.modifiers.shift, window, cx);
            cx.notify();
            return true;
        }

        false
    }

    fn handle_sidebar_search_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.sidebar_search_active || self.active_input.is_some() {
            return false;
        }

        if Self::cmd_only(event.keystroke.modifiers) && event.keystroke.key.eq_ignore_ascii_case("a") {
            self.sidebar_search_state.select_all();
            cx.notify();
            return true;
        }

        match event.keystroke.key.as_str() {
            "enter" => self.jump_to_first_search_result(window, cx),
            "escape" => {
                self.blur_sidebar_search();
                cx.notify();
            }
            "backspace" => {
                self.sidebar_search_state.delete_backward();
                self.refresh_search_navigation(window, cx);
            }
            "delete" => {
                self.sidebar_search_state.delete_forward();
                self.refresh_search_navigation(window, cx);
            }
            "left" => {
                self.sidebar_search_state.move_left();
                cx.notify();
            }
            "right" => {
                self.sidebar_search_state.move_right();
                cx.notify();
            }
            "home" => {
                self.sidebar_search_state.move_to_start();
                cx.notify();
            }
            "end" => {
                self.sidebar_search_state.move_to_end();
                cx.notify();
            }
            _ => {}
        }

        true
    }

    fn handle_active_input_key_down(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) {
        let active_field = self.active_input.as_ref().map(|input| input.field);
        let active_input_query = self
            .active_input
            .as_ref()
            .map(|input| input.state.text().to_string())
            .unwrap_or_default();
        let allow_text_editing = active_field.is_some_and(Self::uses_text_input_for_field);

        if Self::cmd_only(event.keystroke.modifiers)
            && event.keystroke.key.eq_ignore_ascii_case("a")
            && let Some(input) = self.active_input.as_mut()
        {
            input.state.select_all();
            cx.notify();
            return;
        }

        match event.keystroke.key.as_str() {
            "enter" => {
                if let Some(field) = active_field {
                    if field == EditableField::Theme {
                        self.commit_active_input(cx);
                        return;
                    }
                }
                if let Some(field) = active_field
                    && self.commit_dropdown_selection(field, &active_input_query, cx)
                {
                    return;
                }
                self.commit_active_input(cx);
            }
            "escape" => self.cancel_active_input(cx),
            "tab" => {
                if let Some(field) = active_field
                    && self.commit_dropdown_selection(field, &active_input_query, cx)
                {
                    return;
                }
            }
            "backspace" => {
                if allow_text_editing && let Some(input) = self.active_input.as_mut() {
                    input.state.delete_backward();
                }
                cx.notify();
            }
            "delete" => {
                if allow_text_editing && let Some(input) = self.active_input.as_mut() {
                    input.state.delete_forward();
                }
                cx.notify();
            }
            "left" => {
                if allow_text_editing && let Some(input) = self.active_input.as_mut() {
                    input.state.move_left();
                }
                cx.notify();
            }
            "right" => {
                if allow_text_editing && let Some(input) = self.active_input.as_mut() {
                    input.state.move_right();
                }
                cx.notify();
            }
            "home" => {
                if allow_text_editing && let Some(input) = self.active_input.as_mut() {
                    input.state.move_to_start();
                }
                cx.notify();
            }
            "end" => {
                if allow_text_editing && let Some(input) = self.active_input.as_mut() {
                    input.state.move_to_end();
                }
                cx.notify();
            }
            _ => {}
        }
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.key_input_mode() == KeyInputMode::CaptureAction {
            self.handle_keybind_capture(event, cx);
            return;
        }

        if self.handle_global_shortcuts(event, window, cx)
            || self.handle_section_cycle_shortcut(event, window, cx)
        {
            return;
        }

        match self.key_input_mode() {
            KeyInputMode::CaptureAction => self.handle_keybind_capture(event, cx),
            KeyInputMode::SidebarSearch => {
                let _ = self.handle_sidebar_search_key_down(event, window, cx);
            }
            KeyInputMode::ActiveInput => self.handle_active_input_key_down(event, cx),
            KeyInputMode::Idle => {}
        }
    }
}

impl TextInputProvider for SettingsWindow {
    fn text_input_state(&self) -> Option<&TextInputState> {
        let settings_input = self
            .active_input
            .as_ref()
            .and_then(|input| Self::uses_text_input_for_field(input.field).then_some(&input.state));

        settings_input.or_else(|| {
            self.sidebar_search_active
                .then_some(&self.sidebar_search_state)
        })
    }

    fn text_input_state_mut(&mut self) -> Option<&mut TextInputState> {
        let settings_input = self.active_input.as_mut().and_then(|input| {
            Self::uses_text_input_for_field(input.field).then_some(&mut input.state)
        });

        if settings_input.is_some() {
            settings_input
        } else if self.sidebar_search_active {
            Some(&mut self.sidebar_search_state)
        } else {
            None
        }
    }
}

impl gpui::EntityInputHandler for SettingsWindow {
    fn text_for_range(
        &mut self,
        range: std::ops::Range<usize>,
        adjusted_range: &mut Option<std::ops::Range<usize>>,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<String> {
        let state = TextInputProvider::text_input_state(self)?;
        Some(state.text_for_range(range, adjusted_range))
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::UTF16Selection> {
        let state = TextInputProvider::text_input_state(self)?;
        Some(state.selected_text_range())
    }

    fn marked_text_range(
        &self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<std::ops::Range<usize>> {
        let state = TextInputProvider::text_input_state(self)?;
        state.marked_text_range_utf16()
    }

    fn unmark_text(&mut self, _window: &mut gpui::Window, _cx: &mut gpui::Context<Self>) {
        if let Some(state) = TextInputProvider::text_input_state_mut(self) {
            state.unmark_text();
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<std::ops::Range<usize>>,
        text: &str,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut changed = false;
        if let Some(state) = TextInputProvider::text_input_state_mut(self) {
            state.replace_text_in_range(range, text);
            changed = true;
        }

        if changed {
            self.refresh_search_navigation(window, cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<std::ops::Range<usize>>,
        new_text: &str,
        new_selected_range: Option<std::ops::Range<usize>>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut changed = false;
        if let Some(state) = TextInputProvider::text_input_state_mut(self) {
            state.replace_and_mark_text_in_range(range, new_text, new_selected_range);
            changed = true;
        }

        if changed {
            self.refresh_search_navigation(window, cx);
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: std::ops::Range<usize>,
        element_bounds: gpui::Bounds<gpui::Pixels>,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::Bounds<gpui::Pixels>> {
        let state = TextInputProvider::text_input_state(self)?;
        Some(state.bounds_for_range(range_utf16, element_bounds))
    }

    fn character_index_for_point(
        &mut self,
        point: gpui::Point<gpui::Pixels>,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> Option<usize> {
        let state = TextInputProvider::text_input_state(self)?;
        Some(state.character_index_for_point(point))
    }

    fn accepts_text_input(
        &self,
        _window: &mut gpui::Window,
        _cx: &mut gpui::Context<Self>,
    ) -> bool {
        TextInputProvider::text_input_state(self).is_some()
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_window_background_appearance(window);
        let bg = self.bg_primary();
        let settings_scrollbar_metrics = self.settings_scrollbar_metrics(window);
        let settings_scrollbar_lane = {
            div()
                .flex_none()
                .w(px(SETTINGS_SCROLLBAR_WIDTH + 4.0))
                .min_w(px(SETTINGS_SCROLLBAR_WIDTH + 4.0))
                .max_w(px(SETTINGS_SCROLLBAR_WIDTH + 4.0))
                .h_full()
                .pl(px(2.0))
                .pr(px(2.0))
                .when_some(settings_scrollbar_metrics, |s, metrics| {
                    s.child(ui_scrollbar::render_vertical(
                        "settings-content-scrollbar",
                        metrics,
                        self.settings_scrollbar_style(),
                        false,
                        &[],
                        None,
                        0.0,
                    ))
                })
        };
        div()
            .id("settings-root")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            .on_any_mouse_down(cx.listener(|view, _event: &MouseDownEvent, _window, cx| {
                if view.active_input.is_some()
                    || view.sidebar_search_active
                    || view.capturing_action.is_some()
                {
                    view.active_input = None;
                    view.capturing_action = None;
                    view.blur_sidebar_search();
                    cx.notify();
                }
            }))
            .when(self.background_opacity_drag_anchor.is_some(), |s| {
                s.on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _window, cx| {
                    if !event.dragging() {
                        return;
                    }
                    let x: f32 = event.position.x.into();
                    view.update_background_opacity_drag(x, Self::background_opacity_slider_width());
                    cx.notify();
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|view, _event: &MouseUpEvent, _window, cx| {
                        match view.finish_background_opacity_drag() {
                            Ok(true) => termy_toast::success("Saved"),
                            Ok(false) => {}
                            Err(error) => termy_toast::error(error),
                        }
                        cx.notify();
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|view, _event: &MouseUpEvent, _window, cx| {
                        match view.finish_background_opacity_drag() {
                            Ok(true) => termy_toast::success("Saved"),
                            Ok(false) => {}
                            Err(error) => termy_toast::error(error),
                        }
                        cx.notify();
                    }),
                )
            })
            .flex()
            .size_full()
            .bg(bg)
            .font_family(self.config.font_family.clone())
            .child(self.render_sidebar(cx))
            .child(
                // Keep the shared content pane shrink-safe so wide rows cannot
                // push controls or the scrollbar lane off-canvas.
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .h_full()
                    .flex()
                    .items_start()
                    .child(
                        div()
                            .id("settings-content-scroll")
                            .flex_1()
                            .min_w(px(0.0))
                            .h_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.content_scroll_handle)
                            .overflow_x_hidden()
                            .p_6()
                            .child(self.render_content(cx)),
                    )
                    .child(settings_scrollbar_lane),
            )
    }
}
