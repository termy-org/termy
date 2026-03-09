use crate::colors::TerminalColors;
use crate::config::{self, AiProvider as ConfigAiProvider, AppConfig};
use crate::plugins::{self, PluginInventoryEntry};
use crate::text_input::{TextInputAlignment, TextInputElement, TextInputProvider, TextInputState};
use crate::theme_store::{self, ThemeStoreAuthSession, ThemeStoreAuthUser, ThemeStoreTheme};
use crate::ui::scrollbar::{self as ui_scrollbar, ScrollbarPaintStyle, ScrollbarRange};
use gpui::{
    AnyElement, AsyncApp, Context, FocusHandle, Font, InteractiveElement, IntoElement,
    KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    ParentElement, Render, Rgba, ScrollAnchor, ScrollHandle, ScrollWheelEvent, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, TextAlign, WeakEntity, Window,
    WindowBackgroundAppearance, deferred, div, img, point, prelude::FluentBuilder, px,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::LazyLock;
use std::time::{Duration, Instant};
use termy_command_core::CommandId;
use termy_config_core::{
    RootSettingId, RootSettingValueKind, SettingsSection as CoreSettingsSection,
    color_setting_from_key, color_setting_specs, root_setting_default_value,
    root_setting_enum_choices, root_setting_from_key, root_setting_specs, root_setting_value_kind,
};

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
pub(crate) enum SettingsSection {
    Appearance,
    Terminal,
    Tabs,
    Experimental,
    ThemeStore,
    Plugins,
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
    theme_store_search_state: TextInputState,
    theme_store_search_active: bool,
    theme_store_search_selecting: bool,
    search_navigation_last_target: Option<&'static str>,
    search_navigation_last_jump_at: Option<Instant>,
    capturing_action: Option<CommandId>,
    background_opacity_drag_anchor: Option<(f32, f32)>,
    scroll_animation_token: u64,
    colors: TerminalColors,
    last_window_background_appearance: Option<WindowBackgroundAppearance>,
    openai_model_options: Vec<String>,
    openai_models_loading: bool,
    openai_models_loaded_for_api_key: Option<(ConfigAiProvider, String)>,
    theme_store_themes: Vec<ThemeStoreTheme>,
    theme_store_loaded: bool,
    theme_store_loading: bool,
    theme_store_error: Option<String>,
    theme_store_auth_session: Option<ThemeStoreAuthSession>,
    theme_store_auth_loading: bool,
    theme_store_auth_error: Option<String>,
    theme_store_installed_versions: HashMap<String, String>,
    plugin_directory: Option<PathBuf>,
    plugin_inventory: Vec<PluginInventoryEntry>,
    plugin_inventory_error: Option<String>,
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
        #[cfg(test)]
        let _ = &config_change_rx;
        let mut available_font_families = window.text_system().all_font_names();
        available_font_families.sort_unstable_by_key(|font| font.to_ascii_lowercase());
        available_font_families.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        let colors = TerminalColors::from_theme(&config.theme, &config.colors);
        let searchable_settings = Self::build_searchable_settings(
            config.show_plugins_tab,
            crate::experimental::has_entries(),
        );
        let searchable_setting_indices =
            Self::build_searchable_setting_indices(&searchable_settings);
        let content_scroll_handle = ScrollHandle::new();
        let setting_scroll_anchors = Self::build_setting_scroll_anchors(
            &content_scroll_handle,
            config.show_plugins_tab,
            crate::experimental::has_entries(),
        );
        let theme_store_auth_session = theme_store::load_auth_session();
        let (plugin_directory, plugin_inventory, plugin_inventory_error) =
            Self::load_plugin_inventory();
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
            background_opacity_drag_anchor: None,
            scroll_animation_token: 0,
            colors,
            last_window_background_appearance: None,
            openai_model_options: Vec::new(),
            openai_models_loading: false,
            openai_models_loaded_for_api_key: None,
            theme_store_themes: Vec::new(),
            theme_store_loaded: false,
            theme_store_loading: false,
            theme_store_error: None,
            theme_store_auth_session,
            theme_store_auth_loading: false,
            theme_store_auth_error: None,
            theme_store_installed_versions: theme_store::load_installed_theme_versions(),
            plugin_directory,
            plugin_inventory,
            plugin_inventory_error,
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

    fn load_plugin_inventory() -> (Option<PathBuf>, Vec<PluginInventoryEntry>, Option<String>) {
        match plugins::plugin_inventory() {
            Ok(inventory) => (Some(inventory.root_dir), inventory.entries, None),
            Err(error) => (None, Vec::new(), Some(error)),
        }
    }

    fn refresh_plugin_inventory(&mut self) {
        let (directory, entries, error) = Self::load_plugin_inventory();
        self.plugin_directory = directory;
        self.plugin_inventory = entries;
        self.plugin_inventory_error = error;
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
        let api_base = Self::theme_store_api_base_url();

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result =
                smol::unblock(move || theme_store::fetch_theme_store_themes_blocking(&api_base))
                    .await;

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    view.theme_store_loading = false;
                    match result {
                        Ok(themes) => {
                            view.theme_store_themes = themes;
                            view.theme_store_loaded = true;
                            view.theme_store_error = None;
                        }
                        Err(error) => {
                            view.theme_store_themes.clear();
                            view.theme_store_loaded = true;
                            view.theme_store_error = Some(error);
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

    pub(crate) fn apply_theme_store_auth_session(
        &mut self,
        session: ThemeStoreAuthSession,
        cx: &mut Context<Self>,
    ) {
        self.theme_store_auth_session = Some(session);
        self.theme_store_auth_loading = false;
        self.theme_store_auth_error = None;
        cx.notify();
    }

    pub(crate) fn clear_theme_store_auth_session(&mut self, cx: &mut Context<Self>) {
        self.theme_store_auth_session = None;
        self.theme_store_auth_loading = false;
        self.theme_store_auth_error = None;
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

    pub(super) fn open_path(path: &std::path::Path) -> Result<(), String> {
        let path_str = path.to_string_lossy().to_string();
        #[cfg(target_os = "macos")]
        {
            Command::new("open")
                .arg(&path_str)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|error| format!("Failed to open path: {error}"))?;
            return Ok(());
        }
        #[cfg(target_os = "linux")]
        {
            Command::new("xdg-open")
                .arg(&path_str)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|error| format!("Failed to open path: {error}"))?;
            return Ok(());
        }
        #[cfg(target_os = "windows")]
        {
            Command::new("cmd")
                .args(["/C", "start", "", &path_str])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|error| format!("Failed to open path: {error}"))?;
            return Ok(());
        }
        #[allow(unreachable_code)]
        Err("Opening paths is not supported on this platform".to_string())
    }

    fn apply_runtime_config(&mut self, config: AppConfig) -> bool {
        let previous_provider = self.config.ai_provider;
        let next_provider = config.ai_provider;
        let previous_show_plugins_tab = self.config.show_plugins_tab;
        let next_show_plugins_tab = config.show_plugins_tab;
        let previous_api_key = match previous_provider {
            ConfigAiProvider::OpenAi => self.config.openai_api_key.clone(),
            ConfigAiProvider::Gemini => self.config.gemini_api_key.clone(),
        }
        .filter(|value| !value.trim().is_empty());
        let next_api_key = match next_provider {
            ConfigAiProvider::OpenAi => config.openai_api_key.clone(),
            ConfigAiProvider::Gemini => config.gemini_api_key.clone(),
        }
        .filter(|value| !value.trim().is_empty());
        if previous_provider != next_provider || previous_api_key != next_api_key {
            self.openai_model_options.clear();
            self.openai_models_loaded_for_api_key = None;
            self.openai_models_loading = false;
        }
        self.colors = TerminalColors::from_theme(&config.theme, &config.colors);
        self.config = config;
        if previous_show_plugins_tab != next_show_plugins_tab {
            self.searchable_settings = Self::build_searchable_settings(
                next_show_plugins_tab,
                crate::experimental::has_entries(),
            );
            self.searchable_setting_indices =
                Self::build_searchable_setting_indices(&self.searchable_settings);
            self.setting_scroll_anchors = Self::build_setting_scroll_anchors(
                &self.content_scroll_handle,
                next_show_plugins_tab,
                crate::experimental::has_entries(),
            );
            if self.active_section == SettingsSection::Plugins && !next_show_plugins_tab {
                self.active_section = SettingsSection::Appearance;
            }
        }
        true
    }

    fn current_ai_provider(&self) -> ConfigAiProvider {
        self.config.ai_provider
    }

    fn current_ai_api_key(&self) -> Option<String> {
        match self.config.ai_provider {
            ConfigAiProvider::OpenAi => self.config.openai_api_key.clone(),
            ConfigAiProvider::Gemini => self.config.gemini_api_key.clone(),
        }
        .filter(|value| !value.trim().is_empty())
    }

    fn refresh_openai_model_options(&mut self, force: bool, cx: &mut Context<Self>) {
        let provider = self.current_ai_provider();
        let Some(api_key) = self.current_ai_api_key() else {
            self.openai_model_options.clear();
            self.openai_models_loaded_for_api_key = None;
            self.openai_models_loading = false;
            return;
        };

        if self.openai_models_loading {
            return;
        }

        let already_loaded_for_key = self.openai_models_loaded_for_api_key.as_ref().is_some_and(
            |(loaded_provider, loaded_key)| *loaded_provider == provider && loaded_key == &api_key,
        );
        if !force && already_loaded_for_key {
            return;
        }

        self.openai_models_loading = true;
        let provider_name = match provider {
            ConfigAiProvider::OpenAi => "OpenAI",
            ConfigAiProvider::Gemini => "Gemini",
        };
        let loading_toast_id = termy_toast::loading(format!("Fetching {provider_name} models..."));
        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let request_provider = provider;
            let request_api_key = api_key.clone();
            let result = smol::unblock(move || match provider {
                ConfigAiProvider::OpenAi => {
                    let client = termy_openai::OpenAiClient::new(api_key);
                    client
                        .fetch_chat_models()
                        .map(|models| models.into_iter().map(|model| model.id).collect::<Vec<_>>())
                        .map_err(|error| error.to_string())
                }
                ConfigAiProvider::Gemini => {
                    let client = termy_gemini::GeminiClient::new(api_key);
                    client
                        .fetch_chat_models()
                        .map(|models| models.into_iter().map(|model| model.id).collect::<Vec<_>>())
                        .map_err(|error| error.to_string())
                }
            })
            .await;

            termy_toast::dismiss_toast(loading_toast_id);

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    view.openai_models_loading = false;

                    let active_provider = view.current_ai_provider();
                    let active_api_key = view.current_ai_api_key();
                    if active_provider != request_provider
                        || active_api_key.as_deref() != Some(request_api_key.as_str())
                    {
                        return;
                    }

                    match result {
                        Ok(mut models) => {
                            models.sort_unstable();
                            models.dedup();
                            view.openai_model_options = models;
                            view.openai_models_loaded_for_api_key =
                                Some((request_provider, request_api_key));
                        }
                        Err(error) => {
                            view.openai_model_options.clear();
                            view.openai_models_loaded_for_api_key = None;
                            let provider_name = match request_provider {
                                ConfigAiProvider::OpenAi => "OpenAI",
                                ConfigAiProvider::Gemini => "Gemini",
                            };
                            termy_toast::error(format!(
                                "Failed to fetch {provider_name} models: {error}"
                            ));
                        }
                    }

                    cx.notify();
                })
            });
        })
        .detach();
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
            KeyInputMode::ThemeStoreSearch => {
                let _ = self.handle_theme_store_search_key_down(event, cx);
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
            if self.sidebar_search_active {
                Some(&self.sidebar_search_state)
            } else if self.theme_store_search_active {
                Some(&self.theme_store_search_state)
            } else {
                None
            }
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
        } else if self.theme_store_search_active {
            Some(&mut self.theme_store_search_state)
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
                    || view.theme_store_search_active
                    || view.capturing_action.is_some()
                {
                    view.active_input = None;
                    view.capturing_action = None;
                    view.blur_sidebar_search();
                    view.theme_store_search_active = false;
                    view.theme_store_search_selecting = false;
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
