use crate::colors::TerminalColors;
use crate::config::{self, AppConfig};
use crate::text_input::TextInputState;
use crate::theme_store::{self, ThemeStoreAuthSession, ThemeStoreAuthUser, ThemeStoreTheme};
use crate::gpui::{
    AnyElement, App, AsyncApp, Bounds, Context, FocusHandle, InteractiveElement,
    IntoElement, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ObjectFit, ParentElement, Pixels, Render, Rgba, ScrollAnchor, ScrollHandle,
    SharedString, StatefulInteractiveElement, Styled, StyledImage, WeakEntity, Window,
    WindowBackgroundAppearance, div, img, point, prelude::FluentBuilder, px,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{
    LazyLock,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};
use termy_command_core::CommandId;
use termy_config_core::{
    RootSettingId, RootSettingValueKind, SettingsSection as CoreSettingsSection,
    color_setting_from_key, color_setting_specs, format_line_height, root_setting_default_value,
    root_setting_enum_choices, root_setting_from_key, root_setting_specs, root_setting_value_kind,
};

mod colors;
mod components;
mod keybinds;
mod search;
mod sections;
mod state;
mod state_apply;
mod style;
#[cfg(test)]
mod test_utils;

use self::search::SearchableSetting;
use self::state::{ActiveTextInput, EditableField};

const SIDEBAR_WIDTH: f32 = 220.0;
const SETTINGS_CONTROL_WIDTH: f32 = 360.0;
const SETTINGS_CONTROL_HEIGHT: f32 = 36.0;
const NUMERIC_STEP_BUTTON_SIZE: f32 = 26.0;
const SETTINGS_INPUT_TEXT_SIZE: f32 = 13.0;
const SETTINGS_CONFIG_WATCH_INTERVAL_MS: u64 = 750;
const SETTINGS_SEARCH_NAV_THROTTLE_MS: u64 = 70;
const SETTINGS_SCROLL_ANIMATION_DURATION_MS: u64 = 170;
const SETTINGS_SCROLL_ANIMATION_TICK_MS: u64 = 16;
const SETTINGS_OVERLAY_PANEL_ALPHA_FLOOR_RATIO: f32 = 0.72;
const SETTINGS_SWITCH_KNOB_SIZE: f32 = 20.0;
const SETTINGS_SEARCH_PREVIEW_LIMIT: usize = 6;
const SETTINGS_SLIDER_VALUE_WIDTH: f32 = 60.0;
const SETTINGS_OPACITY_STEP_RATIO: f32 = 0.05;
const SETTINGS_CONTROL_INNER_PADDING: f32 = 8.0;
const SETTINGS_OPACITY_CONTROL_GAP: f32 = 6.0;
static NEXT_BACKGROUND_OPACITY_PREVIEW_OWNER_ID: AtomicU64 = AtomicU64::new(1);
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SettingsSection {
    Appearance,
    Terminal,
    Tabs,
    ThemeStore,
    Advanced,
    Colors,
    Keybindings,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BackgroundOpacityDragState {
    start_local_x: f32,
    start_ratio: f32,
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
    theme_store_search_state: TextInputState,
    theme_store_search_active: bool,
    theme_store_search_selecting: bool,
    search_navigation_last_target: Option<&'static str>,
    search_navigation_last_jump_at: Option<Instant>,
    capturing_action: Option<CommandId>,
    background_opacity_preview_owner_id: u64,
    preview_background_opacity: Option<config::BackgroundOpacityPreview>,
    background_opacity_drag_state: Option<BackgroundOpacityDragState>,
    background_opacity_slider_bounds: Option<Bounds<Pixels>>,
    scroll_animation_token: u64,
    colors: TerminalColors,
    last_window_background_appearance: Option<WindowBackgroundAppearance>,
    theme_store_themes: Vec<ThemeStoreTheme>,
    theme_store_loaded: bool,
    theme_store_loading: bool,
    theme_store_error: Option<String>,
    theme_store_from_cache: bool,
    theme_store_auth_session: Option<ThemeStoreAuthSession>,
    theme_store_auth_loading: bool,
    theme_store_auth_error: Option<String>,
    theme_store_installed_versions: HashMap<String, String>,
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
        let background_opacity_preview_rx = config::subscribe_background_opacity_preview();
        #[cfg(test)]
        let _ = &config_change_rx;
        #[cfg(test)]
        let _ = &background_opacity_preview_rx;
        let mut available_font_families = window.text_system().all_font_names();
        available_font_families.sort_unstable_by_key(|font| font.to_ascii_lowercase());
        available_font_families.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        let colors = TerminalColors::from_theme(&config.theme, &config.colors);
        let searchable_settings = Self::build_searchable_settings();
        let searchable_setting_indices =
            Self::build_searchable_setting_indices(&searchable_settings);
        let content_scroll_handle = ScrollHandle::new();
        let setting_scroll_anchors = Self::build_setting_scroll_anchors(&content_scroll_handle);
        let theme_store_auth_session = theme_store::load_auth_session();
        let mut view = Self {
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
            theme_store_search_state: TextInputState::new(String::new()),
            theme_store_search_active: false,
            theme_store_search_selecting: false,
            search_navigation_last_target: None,
            search_navigation_last_jump_at: None,
            capturing_action: None,
            background_opacity_preview_owner_id: NEXT_BACKGROUND_OPACITY_PREVIEW_OWNER_ID
                .fetch_add(1, Ordering::Relaxed),
            preview_background_opacity: config::current_background_opacity_preview(),
            background_opacity_drag_state: None,
            background_opacity_slider_bounds: None,
            scroll_animation_token: 0,
            colors,
            last_window_background_appearance: None,
            theme_store_themes: Vec::new(),
            theme_store_loaded: false,
            theme_store_loading: false,
            theme_store_error: None,
            theme_store_from_cache: false,
            theme_store_auth_session,
            theme_store_auth_loading: false,
            theme_store_auth_error: None,
            theme_store_installed_versions: theme_store::load_installed_theme_versions(),
        };
        view.focus_handle.focus(window, cx);
        if view.theme_store_auth_session.is_some() {
            view.refresh_theme_store_auth_user(cx);
        }

        #[cfg(not(test))]
        {
            cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                loop {
                    let wait_rx = config_change_rx.clone();
                    if smol::unblock(move || wait_rx.recv()).await.is_err() {
                        break;
                    }
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
        }

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
                            if view.sync_background_opacity_preview(opacity) {
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

    fn theme_store_api_base_url() -> String {
        theme_store::theme_store_api_base_url()
    }

    fn ensure_theme_store_themes_loaded(&mut self, cx: &mut Context<Self>) {
        if self.theme_store_loaded || self.theme_store_loading {
            return;
        }
        self.refresh_theme_store_themes(cx);
    }

    fn refresh_theme_store_themes(&mut self, cx: &mut Context<Self>) {
        if self.theme_store_loading {
            return;
        }

        self.theme_store_loading = true;
        self.theme_store_error = None;
        self.theme_store_from_cache = false;
        let api_base = Self::theme_store_api_base_url();

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result =
                smol::unblock(move || theme_store::fetch_theme_store_themes_blocking(&api_base))
                    .await;

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    view.theme_store_loading = false;
                    match result {
                        Ok((themes, from_cache)) => {
                            view.theme_store_themes = themes;
                            view.theme_store_loaded = true;
                            view.theme_store_error = None;
                            view.theme_store_from_cache = from_cache;
                        }
                        Err(error) => {
                            view.theme_store_themes.clear();
                            view.theme_store_loaded = true;
                            view.theme_store_error = Some(error);
                            view.theme_store_from_cache = false;
                        }
                    }
                    cx.notify();
                })
            });
        })
        .detach();
    }

    fn install_theme_store_theme(&mut self, theme: ThemeStoreTheme, cx: &mut Context<Self>) {
        let installed_slug = theme.slug.trim().to_ascii_lowercase();
        let installed_version = theme.latest_version.clone().unwrap_or_default();
        let loading_id = termy_toast::loading(format!("Installing {}...", theme.name));

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result =
                smol::unblock(move || theme_store::install_theme_from_store_blocking(theme)).await;

            termy_toast::dismiss_toast(loading_id);

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    match result {
                        Ok(installed_theme) => {
                            termy_toast::success(installed_theme.message);
                            view.theme_store_installed_versions
                                .insert(installed_slug.clone(), installed_version.clone());
                            if let Err(error) = theme_store::persist_installed_theme_versions(
                                &view.theme_store_installed_versions,
                            ) {
                                log::error!("Failed to persist installed theme state: {}", error);
                            }
                            let _ = view.reload_config_if_changed(cx);
                        }
                        Err(error) => termy_toast::error(error),
                    }
                    cx.notify();
                })
            });
        })
        .detach();
    }

    fn confirm_install_theme_store_theme(
        &mut self,
        theme: ThemeStoreTheme,
        cx: &mut Context<Self>,
    ) {
        let title = "Install Theme";
        let message = format!(
            "Install theme \"{}\" into your local theme library?",
            theme.name
        );

        cx.spawn(async move |this, cx: &mut AsyncApp| {
            let confirmed = termy_native_sdk::confirm(title, &message);
            if !confirmed {
                return;
            }

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    view.install_theme_store_theme(theme.clone(), cx);
                })
            });
        })
        .detach();
    }

    fn uninstall_theme_store_theme(&mut self, slug: &str, cx: &mut Context<Self>) {
        let key = slug.trim().to_ascii_lowercase();
        if key.is_empty() {
            return;
        }

        match theme_store::uninstall_installed_theme(&key) {
            Ok(true) => {
                self.theme_store_installed_versions.remove(&key);
                if self.config.theme.eq_ignore_ascii_case(&key) {
                    if let Err(error) = config::set_theme_in_config(config::SHELL_DECIDE_THEME_ID) {
                        log::error!("Failed to reset theme during uninstall: {}", error);
                        termy_toast::error("Failed to reset selected theme");
                        return;
                    }
                    self.config.theme = config::SHELL_DECIDE_THEME_ID.to_string();
                }
                let _ = self.reload_config_if_changed(cx);
                termy_toast::success("Theme uninstalled");
            }
            Ok(false) => {
                termy_toast::info("Theme is not installed");
                return;
            }
            Err(error) => {
                log::error!("Failed to uninstall theme: {}", error);
                termy_toast::error(error);
                return;
            }
        }
    }

    pub(crate) fn apply_theme_store_install(
        &mut self,
        slug: &str,
        version: &str,
        cx: &mut Context<Self>,
    ) {
        self.theme_store_installed_versions
            .insert(slug.trim().to_ascii_lowercase(), version.to_string());
        cx.notify();
    }

    fn refresh_theme_store_auth_user(&mut self, cx: &mut Context<Self>) {
        if self.theme_store_auth_loading {
            return;
        }
        let Some(session) = self.theme_store_auth_session.clone() else {
            return;
        };

        self.theme_store_auth_loading = true;
        self.theme_store_auth_error = None;
        let api_base = Self::theme_store_api_base_url();

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let session_token = session.session_token.clone();
            let result = smol::unblock(move || {
                theme_store::fetch_auth_user_blocking(&api_base, &session_token).map(|user| {
                    ThemeStoreAuthSession {
                        session_token,
                        user,
                    }
                })
            })
            .await;

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    view.theme_store_auth_loading = false;
                    match result {
                        Ok(session) => {
                            if let Err(error) = theme_store::persist_auth_session(&session) {
                                log::error!("Failed to persist auth session: {}", error);
                                view.theme_store_auth_error = Some(error);
                            } else {
                                view.theme_store_auth_error = None;
                            }
                            view.theme_store_auth_session = Some(session);
                        }
                        Err(error) => {
                            log::error!("Failed to refresh theme store auth session: {}", error);
                            view.theme_store_auth_error = Some(error);
                        }
                    }
                    cx.notify();
                })
            });
        })
        .detach();
    }

    fn logout_theme_store_user(&mut self, cx: &mut Context<Self>) {
        if self.theme_store_auth_loading {
            return;
        }
        let Some(session) = self.theme_store_auth_session.clone() else {
            return;
        };

        self.theme_store_auth_loading = true;
        self.theme_store_auth_error = None;
        let api_base = Self::theme_store_api_base_url();

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let session_token = session.session_token.clone();
            let result = smol::unblock(move || {
                theme_store::logout_auth_session_blocking(&api_base, &session_token)?;
                theme_store::clear_auth_session()
            })
            .await;

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    view.theme_store_auth_loading = false;
                    match result {
                        Ok(()) => {
                            view.theme_store_auth_session = None;
                            view.theme_store_auth_error = None;
                            termy_toast::success("Logged out from theme store");
                        }
                        Err(error) => {
                            log::error!("Failed to logout from theme store: {}", error);
                            view.theme_store_auth_error = Some(error.clone());
                            termy_toast::error(error);
                        }
                    }
                    cx.notify();
                })
            });
        })
        .detach();
    }

    fn theme_store_auth_display_name(user: &ThemeStoreAuthUser) -> String {
        user.name
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| format!("@{}", user.github_login))
    }

    fn theme_store_auth_avatar_fallback_label(user: &ThemeStoreAuthUser) -> String {
        user.github_login
            .chars()
            .next()
            .map(|ch| ch.to_ascii_uppercase().to_string())
            .unwrap_or_else(|| "?".to_string())
    }

    pub(super) fn open_url(url: &str) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            if Command::new("open")
                .arg(url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .is_ok()
            {
                return Ok(());
            }
        }
        #[cfg(target_os = "linux")]
        {
            if Command::new("xdg-open")
                .arg(url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .is_ok()
            {
                return Ok(());
            }
        }
        #[cfg(target_os = "windows")]
        {
            if Command::new("cmd")
                .args(["/C", "start", "", url])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .is_ok()
            {
                return Ok(());
            }
        }

        if webbrowser::open(url).is_ok() {
            return Ok(());
        }

        Err(format!("Failed to open URL: {url}"))
    }

    fn apply_runtime_config(&mut self, config: AppConfig) -> bool {
        self.colors = TerminalColors::from_theme(&config.theme, &config.colors);
        self.config = config;
        let previous_preview = self.preview_background_opacity;
        let synced_preview = config::synced_background_opacity_preview(
            self.config.background_opacity,
            previous_preview,
        );
        if synced_preview != previous_preview {
            self.preview_background_opacity = synced_preview;
            if previous_preview
                .is_some_and(|preview| preview.owner_id == self.background_opacity_preview_owner_id)
                && self.background_opacity_drag_state.is_none()
            {
                config::publish_background_opacity_preview(None);
            }
        }
        true
    }

    fn effective_background_opacity(&self) -> f32 {
        config::effective_background_opacity(
            self.config.background_opacity,
            self.preview_background_opacity,
        )
    }

    fn sync_background_opacity_preview(
        &mut self,
        preview: Option<config::BackgroundOpacityPreview>,
    ) -> bool {
        let preview = preview.map(|preview| config::BackgroundOpacityPreview {
            opacity: preview.opacity.clamp(0.0, 1.0),
            ..preview
        });
        if self.preview_background_opacity == preview {
            return false;
        }
        self.preview_background_opacity = preview;
        true
    }

    fn clear_background_opacity_preview(&mut self) -> bool {
        if !self.sync_background_opacity_preview(None) {
            return false;
        }
        config::publish_background_opacity_preview(None);
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

    fn cmd_only(modifiers: crate::gpui::Modifiers) -> bool {
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

        if Self::cmd_only(event.keystroke.modifiers)
            && event.keystroke.key.eq_ignore_ascii_case("a")
        {
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

    fn handle_theme_store_search_key_down(
        &mut self,
        event: &KeyDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.theme_store_search_active || self.active_input.is_some() {
            return false;
        }

        if Self::cmd_only(event.keystroke.modifiers)
            && event.keystroke.key.eq_ignore_ascii_case("a")
        {
            self.theme_store_search_state.select_all();
            cx.notify();
            return true;
        }

        match event.keystroke.key.as_str() {
            "escape" => {
                self.theme_store_search_active = false;
                self.theme_store_search_selecting = false;
                cx.notify();
            }
            "backspace" => {
                self.theme_store_search_state.delete_backward();
                cx.notify();
            }
            "delete" => {
                self.theme_store_search_state.delete_forward();
                cx.notify();
            }
            "left" => {
                self.theme_store_search_state.move_left();
                cx.notify();
            }
            "right" => {
                self.theme_store_search_state.move_right();
                cx.notify();
            }
            "home" => {
                self.theme_store_search_state.move_to_start();
                cx.notify();
            }
            "end" => {
                self.theme_store_search_state.move_to_end();
                cx.notify();
            }
            _ => {}
        }

        true
    }

    fn handle_active_input_key_down(&mut self, event: &KeyDownEvent, cx: &mut Context<Self>) {
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

        // Handle Cmd+V (paste)
        if Self::cmd_only(event.keystroke.modifiers)
            && event.keystroke.key.eq_ignore_ascii_case("v")
            && allow_text_editing
        {
            if let Some(clipboard_text) = cx.read_from_clipboard().and_then(|item| item.text()) {
                // Filter out newlines for single-line input
                let filtered_text: String = clipboard_text
                    .chars()
                    .filter(|c| *c != '\n' && *c != '\r')
                    .collect();
                if !filtered_text.is_empty()
                    && let Some(input) = self.active_input.as_mut()
                {
                    input.state.replace_text_in_range(None, &filtered_text);
                    cx.notify();
                }
            }
            return;
        }

        match event.keystroke.key.as_str() {
            "enter" => {
                if let Some(field) = active_field
                    && field == EditableField::Theme
                {
                    self.commit_active_input(cx);
                    return;
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
        if self.capturing_action.is_some() {
            self.handle_keybind_capture(event, cx);
            return;
        }

        self.handle_global_shortcuts(event, window, cx);
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_window_background_appearance(window);
        let bg = self.bg_primary();
        let entity = cx.entity().clone();

        div()
            .id("settings-root")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            .flex()
            .size_full()
            .bg(bg)
            .font_family(self.config.font_family.clone())
            .child(
                gpui_component::setting::Settings::new("settings")
                    .sidebar_width(px(SIDEBAR_WIDTH))
                    .pages(self.build_pages(&entity, cx)),
            )
    }
}

impl Drop for SettingsWindow {
    fn drop(&mut self) {
        if config::synced_background_opacity_preview(
            self.config.background_opacity,
            self.preview_background_opacity,
        )
        .is_some_and(|preview| preview.owner_id == self.background_opacity_preview_owner_id)
        {
            config::publish_background_opacity_preview(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_utils::open_settings_window_handle;
    use super::*;
    use crate::gpui::TestAppContext;

    #[test]
    fn settings_effective_background_opacity_prefers_preview() {
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
    fn settings_preview_clears_when_saved_matches_preview() {
        assert_eq!(
            config::synced_background_opacity_preview(
                0.4,
                Some(config::BackgroundOpacityPreview {
                    owner_id: 1,
                    opacity: 0.4,
                }),
            ),
            None
        );
    }

    #[test]
    fn settings_preview_keeps_unrelated_value() {
        assert_eq!(
            config::synced_background_opacity_preview(
                0.4,
                Some(config::BackgroundOpacityPreview {
                    owner_id: 1,
                    opacity: 0.6,
                }),
            ),
            Some(config::BackgroundOpacityPreview {
                owner_id: 1,
                opacity: 0.6,
            })
        );
    }

    #[crate::gpui::test]
    fn apply_runtime_config_preserves_out_of_range_vertical_tab_width(cx: &mut TestAppContext) {
        let settings = open_settings_window_handle(cx);
        settings
            .update(cx, |view, _window, _cx| {
                let next = AppConfig {
                    vertical_tabs_width: 12.0,
                    ..Default::default()
                };
                assert!(view.apply_runtime_config(next));
                assert_eq!(view.config.vertical_tabs_width, 12.0);
            })
            .expect("settings window update should succeed");
    }
}
