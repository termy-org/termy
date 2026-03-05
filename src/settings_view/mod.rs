use crate::colors::TerminalColors;
use crate::config::{self, AiProvider as ConfigAiProvider, AppConfig};
use crate::plugins::{self, PluginInventoryEntry};
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
use std::io::Write;
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
const DEFAULT_THEME_STORE_API_URL: &str = "https://api.termy.run";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SettingsSection {
    Appearance,
    Terminal,
    Tabs,
    ThemeStore,
    Plugins,
    Advanced,
    Colors,
    Keybindings,
}

#[derive(Clone, Debug)]
struct ThemeStoreTheme {
    name: String,
    slug: String,
    description: String,
    latest_version: Option<String>,
    file_url: Option<String>,
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
        let mut available_font_families = window.text_system().all_font_names();
        available_font_families.sort_unstable_by_key(|font| font.to_ascii_lowercase());
        available_font_families.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        let colors = TerminalColors::from_theme(&config.theme, &config.colors);
        let searchable_settings = Self::build_searchable_settings();
        let searchable_setting_indices =
            Self::build_searchable_setting_indices(&searchable_settings);
        let content_scroll_handle = ScrollHandle::new();
        let setting_scroll_anchors = Self::build_setting_scroll_anchors(&content_scroll_handle);
        let (plugin_directory, plugin_inventory, plugin_inventory_error) =
            Self::load_plugin_inventory();
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
            theme_store_installed_versions: Self::load_installed_theme_versions(),
            plugin_directory,
            plugin_inventory,
            plugin_inventory_error,
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

    fn theme_store_api_base_url() -> String {
        std::env::var("THEME_STORE_API_URL").unwrap_or_else(|_| DEFAULT_THEME_STORE_API_URL.into())
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
                smol::unblock(move || Self::fetch_theme_store_themes_blocking(&api_base)).await;

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

    fn fetch_theme_store_themes_blocking(api_base: &str) -> Result<Vec<ThemeStoreTheme>, String> {
        let base = api_base.trim_end_matches('/');
        let url = format!("{base}/themes");
        let response = ureq::get(&url)
            .set("Accept", "application/json")
            .call()
            .map_err(|error| format!("Failed to fetch store themes: {error}"))?;

        let payload: serde_json::Value = response
            .into_json()
            .map_err(|error| format!("Invalid theme store response: {error}"))?;

        let themes = payload
            .as_array()
            .ok_or_else(|| "Theme store response must be a JSON array".to_string())?;

        let mut parsed = Vec::with_capacity(themes.len());
        for theme in themes {
            let Some(object) = theme.as_object() else {
                continue;
            };

            let Some(name) = object
                .get("name")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };
            let Some(slug) = object
                .get("slug")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                continue;
            };

            let description = object
                .get("description")
                .and_then(|value| value.as_str())
                .unwrap_or_default()
                .to_string();
            let latest_version = object
                .get("latestVersion")
                .and_then(|value| value.as_str())
                .map(ToString::to_string);
            let file_url = object
                .get("fileUrl")
                .and_then(|value| value.as_str())
                .map(ToString::to_string);

            parsed.push(ThemeStoreTheme {
                name: name.to_string(),
                slug: slug.to_string(),
                description,
                latest_version,
                file_url,
            });
        }

        parsed.sort_unstable_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
        });
        Ok(parsed)
    }

    fn installed_theme_state_path() -> Option<PathBuf> {
        let config_path = config::ensure_config_file().ok()?;
        let parent = config_path.parent()?;
        Some(parent.join("theme_store_installed.json"))
    }

    fn load_installed_theme_versions() -> HashMap<String, String> {
        let Some(path) = Self::installed_theme_state_path() else {
            return HashMap::new();
        };
        let Ok(contents) = std::fs::read_to_string(&path) else {
            return HashMap::new();
        };

        if let Ok(parsed_map) = serde_json::from_str::<HashMap<String, String>>(&contents) {
            return parsed_map
                .into_iter()
                .map(|(slug, version)| {
                    (slug.trim().to_ascii_lowercase(), version.trim().to_string())
                })
                .filter(|(slug, _)| !slug.is_empty())
                .collect();
        }

        // Backward compatibility with older JSON array format.
        if let Ok(parsed_list) = serde_json::from_str::<Vec<String>>(&contents) {
            return parsed_list
                .into_iter()
                .map(|slug| (slug.trim().to_ascii_lowercase(), String::new()))
                .filter(|(slug, _)| !slug.is_empty())
                .collect();
        }

        HashMap::new()
    }

    fn persist_installed_theme_versions(versions: &HashMap<String, String>) -> Result<(), String> {
        let Some(path) = Self::installed_theme_state_path() else {
            return Err("Config path unavailable".to_string());
        };
        let Some(parent) = path.parent() else {
            return Err("Invalid installed-theme metadata path".to_string());
        };
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create metadata directory: {error}"))?;

        let mut sorted_entries: Vec<(String, String)> = versions
            .iter()
            .map(|(slug, version)| (slug.clone(), version.clone()))
            .collect();
        sorted_entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        let normalized: HashMap<String, String> = sorted_entries.into_iter().collect();
        let contents = serde_json::to_string_pretty(&normalized)
            .map_err(|error| format!("Failed to serialize installed themes: {error}"))?;
        std::fs::write(&path, contents)
            .map_err(|error| format!("Failed to write installed themes metadata: {error}"))?;
        Ok(())
    }

    fn install_theme_store_theme(&mut self, theme: ThemeStoreTheme, cx: &mut Context<Self>) {
        let installed_slug = theme.slug.trim().to_ascii_lowercase();
        let installed_version = theme.latest_version.clone().unwrap_or_default();
        let loading_id = termy_toast::loading(format!("Installing {}...", theme.name));

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result =
                smol::unblock(move || Self::install_theme_from_store_blocking(theme)).await;

            termy_toast::dismiss_toast(loading_id);

            let _ = cx.update(|cx| {
                this.update(cx, |view, cx| {
                    match result {
                        Ok(message) => {
                            termy_toast::success(message);
                            // Keep a single installed store theme marker at a time.
                            view.theme_store_installed_versions.clear();
                            view.theme_store_installed_versions
                                .insert(installed_slug.clone(), installed_version.clone());
                            if let Err(error) = Self::persist_installed_theme_versions(
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
            "Install theme \"{}\" and import its colors into your config?",
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

        if self.theme_store_installed_versions.remove(&key).is_some() {
            if let Err(error) = config::clear_all_color_overrides() {
                log::error!("Failed to clear colors during uninstall: {}", error);
                termy_toast::error("Failed to remove installed theme colors");
                return;
            }
            if let Err(error) =
                Self::persist_installed_theme_versions(&self.theme_store_installed_versions)
            {
                log::error!("Failed to persist installed theme state: {}", error);
                termy_toast::error("Failed to persist uninstall state");
                return;
            }
            let _ = self.reload_config_if_changed(cx);
            termy_toast::success("Theme uninstalled and colors reverted");
        } else {
            termy_toast::info("Theme is not installed");
        }
    }

    fn install_theme_from_store_blocking(theme: ThemeStoreTheme) -> Result<String, String> {
        let file_url = theme
            .file_url
            .ok_or_else(|| format!("Theme '{}' has no downloadable file URL", theme.slug))?;

        let response = ureq::get(&file_url)
            .set("Accept", "application/json")
            .call()
            .map_err(|error| format!("Failed to download theme '{}': {error}", theme.slug))?;
        let contents = response
            .into_string()
            .map_err(|error| format!("Failed to read theme '{}': {error}", theme.slug))?;

        let mut file =
            tempfile::NamedTempFile::new().map_err(|error| format!("Temp file error: {error}"))?;
        file.write_all(contents.as_bytes())
            .map_err(|error| format!("Failed to write temp theme file: {error}"))?;

        config::import_colors_from_json(file.path())
            .map(|_| format!("Installed theme '{}'", theme.name))
            .map_err(|error| format!("Failed to install theme '{}': {error}", theme.name))
    }

    pub(super) fn open_url(url: &str) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            Command::new("open")
                .arg(url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map(|_| ())
                .map_err(|error| format!("Failed to open URL: {error}"))?;
            return Ok(());
        }
        #[cfg(target_os = "linux")]
        {
            Command::new("xdg-open")
                .arg(url)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map(|_| ())
                .map_err(|error| format!("Failed to open URL: {error}"))?;
            return Ok(());
        }
        #[cfg(target_os = "windows")]
        {
            Command::new("cmd")
                .args(["/C", "start", "", url])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map(|_| ())
                .map_err(|error| format!("Failed to open URL: {error}"))?;
            return Ok(());
        }
        #[allow(unreachable_code)]
        Err("Opening URLs is unsupported on this platform".to_string())
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
