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
    color_setting_specs, root_setting_enum_choices, root_setting_specs, root_setting_value_kind,
};

mod colors;
mod components;
mod keybinds;
mod search;
mod sections;
mod state;
mod style;

const SIDEBAR_WIDTH: f32 = 220.0;
const NUMERIC_INPUT_WIDTH: f32 = 220.0;
const NUMERIC_INPUT_HEIGHT: f32 = 34.0;
const NUMERIC_STEP_BUTTON_SIZE: f32 = 24.0;
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EditableField {
    Theme,
    BackgroundOpacity,
    FontFamily,
    FontSize,
    PaddingX,
    PaddingY,
    Shell,
    Term,
    Colorterm,
    ScrollbackHistory,
    InactiveTabScrollback,
    ScrollMultiplier,
    CursorStyle,
    ScrollbarVisibility,
    ScrollbarStyle,
    TabFallbackTitle,
    TabTitlePriority,
    TabTitleMode,
    TabTitleExplicitPrefix,
    TabTitlePromptFormat,
    TabTitleCommandFormat,
    TabCloseVisibility,
    TabWidthMode,
    KeybindDirectives,
    WorkingDirectory,
    WorkingDirFallback,
    WindowWidth,
    WindowHeight,
    Color(termy_config_core::ColorSettingId),
}

#[derive(Clone, Debug)]
struct ActiveTextInput {
    field: EditableField,
    state: TextInputState,
    selecting: bool,
}

#[derive(Clone, Debug)]
struct DropdownOption {
    value: String,
    label: String,
    show_raw_value: bool,
}

impl DropdownOption {
    fn raw(value: String) -> Self {
        Self {
            label: value.clone(),
            value,
            show_raw_value: false,
        }
    }

    fn labeled(value: String, label: String, show_raw_value: bool) -> Self {
        Self {
            value,
            label,
            show_raw_value,
        }
    }

    fn display_text(&self) -> String {
        if self.show_raw_value {
            format!("{} ({})", self.label, self.value)
        } else if self.label == self.value {
            self.label.clone()
        } else {
            format!("{} ({})", self.label, self.value)
        }
    }
}

impl ActiveTextInput {
    fn new(field: EditableField, text: String) -> Self {
        Self {
            field,
            state: TextInputState::new(text),
            selecting: false,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SettingsSection {
    Appearance,
    Terminal,
    Tabs,
    Advanced,
    Colors,
    Keybindings,
}

#[derive(Clone, Copy, Debug)]
struct SettingMetadata {
    key: &'static str,
    section: SettingsSection,
    title: &'static str,
    description: &'static str,
    keywords: &'static [&'static str],
}

static SETTINGS_METADATA: LazyLock<Vec<SettingMetadata>> = LazyLock::new(|| {
    let mut entries = root_setting_specs()
        .iter()
        .map(|spec| SettingMetadata {
            key: spec.key,
            section: match spec.section {
                CoreSettingsSection::Appearance => SettingsSection::Appearance,
                CoreSettingsSection::Terminal => SettingsSection::Terminal,
                CoreSettingsSection::Tabs => SettingsSection::Tabs,
                CoreSettingsSection::Advanced => SettingsSection::Advanced,
                CoreSettingsSection::Colors => SettingsSection::Colors,
                CoreSettingsSection::Keybindings => SettingsSection::Keybindings,
            },
            title: spec.title,
            description: spec.description,
            keywords: spec.keywords,
        })
        .collect::<Vec<_>>();

    entries.extend(color_setting_specs().iter().map(|spec| SettingMetadata {
        key: spec.key,
        section: SettingsSection::Colors,
        title: spec.title,
        description: spec.description,
        keywords: spec.keywords,
    }));

    entries
});

#[derive(Clone, Debug)]
struct SearchableSetting {
    metadata: &'static SettingMetadata,
    title_lower: String,
    description_lower: String,
    section_lower: String,
    keywords_lower: String,
    haystack_lower: String,
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
    scroll_animation_token: u64,
    colors: TerminalColors,
    last_window_background_appearance: Option<WindowBackgroundAppearance>,
}

impl SettingsWindow {
    fn parse_tab_title_source_token(token: &str) -> Option<termy_config_core::TabTitleSource> {
        match token.trim().to_ascii_lowercase().as_str() {
            "manual" => Some(termy_config_core::TabTitleSource::Manual),
            "explicit" => Some(termy_config_core::TabTitleSource::Explicit),
            "shell" | "app" | "terminal" => Some(termy_config_core::TabTitleSource::Shell),
            "fallback" | "default" => Some(termy_config_core::TabTitleSource::Fallback),
            _ => None,
        }
    }

    fn custom_color_for_id(&self, id: termy_config_core::ColorSettingId) -> Option<termy_config_core::Rgb8> {
        let colors = &self.config.colors;
        match id {
            termy_config_core::ColorSettingId::Foreground => colors.foreground,
            termy_config_core::ColorSettingId::Background => colors.background,
            termy_config_core::ColorSettingId::Cursor => colors.cursor,
            termy_config_core::ColorSettingId::Black => colors.ansi[0],
            termy_config_core::ColorSettingId::Red => colors.ansi[1],
            termy_config_core::ColorSettingId::Green => colors.ansi[2],
            termy_config_core::ColorSettingId::Yellow => colors.ansi[3],
            termy_config_core::ColorSettingId::Blue => colors.ansi[4],
            termy_config_core::ColorSettingId::Magenta => colors.ansi[5],
            termy_config_core::ColorSettingId::Cyan => colors.ansi[6],
            termy_config_core::ColorSettingId::White => colors.ansi[7],
            termy_config_core::ColorSettingId::BrightBlack => colors.ansi[8],
            termy_config_core::ColorSettingId::BrightRed => colors.ansi[9],
            termy_config_core::ColorSettingId::BrightGreen => colors.ansi[10],
            termy_config_core::ColorSettingId::BrightYellow => colors.ansi[11],
            termy_config_core::ColorSettingId::BrightBlue => colors.ansi[12],
            termy_config_core::ColorSettingId::BrightMagenta => colors.ansi[13],
            termy_config_core::ColorSettingId::BrightCyan => colors.ansi[14],
            termy_config_core::ColorSettingId::BrightWhite => colors.ansi[15],
        }
    }

    fn set_custom_color_for_id(
        &mut self,
        id: termy_config_core::ColorSettingId,
        value: Option<termy_config_core::Rgb8>,
    ) {
        let colors = &mut self.config.colors;
        match id {
            termy_config_core::ColorSettingId::Foreground => colors.foreground = value,
            termy_config_core::ColorSettingId::Background => colors.background = value,
            termy_config_core::ColorSettingId::Cursor => colors.cursor = value,
            termy_config_core::ColorSettingId::Black => colors.ansi[0] = value,
            termy_config_core::ColorSettingId::Red => colors.ansi[1] = value,
            termy_config_core::ColorSettingId::Green => colors.ansi[2] = value,
            termy_config_core::ColorSettingId::Yellow => colors.ansi[3] = value,
            termy_config_core::ColorSettingId::Blue => colors.ansi[4] = value,
            termy_config_core::ColorSettingId::Magenta => colors.ansi[5] = value,
            termy_config_core::ColorSettingId::Cyan => colors.ansi[6] = value,
            termy_config_core::ColorSettingId::White => colors.ansi[7] = value,
            termy_config_core::ColorSettingId::BrightBlack => colors.ansi[8] = value,
            termy_config_core::ColorSettingId::BrightRed => colors.ansi[9] = value,
            termy_config_core::ColorSettingId::BrightGreen => colors.ansi[10] = value,
            termy_config_core::ColorSettingId::BrightYellow => colors.ansi[11] = value,
            termy_config_core::ColorSettingId::BrightBlue => colors.ansi[12] = value,
            termy_config_core::ColorSettingId::BrightMagenta => colors.ansi[13] = value,
            termy_config_core::ColorSettingId::BrightCyan => colors.ansi[14] = value,
            termy_config_core::ColorSettingId::BrightWhite => colors.ansi[15] = value,
        }
    }

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

    fn settings_section_label(section: SettingsSection) -> &'static str {
        match section {
            SettingsSection::Appearance => "Appearance",
            SettingsSection::Terminal => "Terminal",
            SettingsSection::Tabs => "Tabs",
            SettingsSection::Advanced => "Advanced",
            SettingsSection::Colors => "Colors",
            SettingsSection::Keybindings => "Keybindings",
        }
    }

    fn build_searchable_settings() -> Vec<SearchableSetting> {
        SETTINGS_METADATA
            .iter()
            .map(|metadata| {
                let title_lower = metadata.title.to_ascii_lowercase();
                let description_lower = metadata.description.to_ascii_lowercase();
                let section_lower =
                    Self::settings_section_label(metadata.section).to_ascii_lowercase();
                let keywords_lower = metadata.keywords.join(" ").to_ascii_lowercase();
                let haystack_lower = format!(
                    "{} {} {} {}",
                    title_lower, description_lower, section_lower, keywords_lower
                );

                SearchableSetting {
                    metadata,
                    title_lower,
                    description_lower,
                    section_lower,
                    keywords_lower,
                    haystack_lower,
                }
            })
            .collect()
    }

    fn build_searchable_setting_indices(
        searchable_settings: &[SearchableSetting],
    ) -> HashMap<&'static str, usize> {
        searchable_settings
            .iter()
            .enumerate()
            .map(|(index, setting)| (setting.metadata.key, index))
            .collect()
    }

    fn build_setting_scroll_anchors(
        content_scroll_handle: &ScrollHandle,
    ) -> HashMap<&'static str, ScrollAnchor> {
        SETTINGS_METADATA
            .iter()
            .map(|setting| {
                (
                    setting.key,
                    ScrollAnchor::for_handle(content_scroll_handle.clone()),
                )
            })
            .collect()
    }

    fn searchable_setting_by_key(&self, key: &'static str) -> Option<&SearchableSetting> {
        let index = self.searchable_setting_indices.get(key).copied()?;
        self.searchable_settings.get(index)
    }

    fn setting_metadata(key: &'static str) -> Option<&'static SettingMetadata> {
        SETTINGS_METADATA.iter().find(|setting| setting.key == key)
    }

    fn setting_search_score(
        setting: &SearchableSetting,
        query: &str,
        terms: &[&str],
    ) -> Option<i32> {
        if !terms
            .iter()
            .all(|term| setting.haystack_lower.contains(term))
        {
            return None;
        }

        let mut score = 0;
        if setting.title_lower == query {
            score += 150;
        }
        if setting.title_lower.starts_with(query) {
            score += 95;
        } else if setting.title_lower.contains(query) {
            score += 60;
        }
        if setting.description_lower.contains(query) {
            score += 24;
        }
        if setting.section_lower.contains(query) {
            score += 18;
        }
        if setting.keywords_lower.contains(query) {
            score += 30;
        }

        for term in terms {
            if setting.title_lower.starts_with(term) {
                score += 20;
            } else if setting.title_lower.contains(term) {
                score += 10;
            }
            if setting.keywords_lower.contains(term) {
                score += 8;
            }
        }

        Some(score.max(1))
    }

    fn sidebar_search_results(&self, limit: usize) -> Vec<&SearchableSetting> {
        let query = self.sidebar_search_state.text().trim().to_ascii_lowercase();
        if query.is_empty() {
            return Vec::new();
        }

        let terms: Vec<&str> = query.split_whitespace().collect();
        let mut matches: Vec<(i32, &SearchableSetting)> = self
            .searchable_settings
            .iter()
            .filter_map(|setting| {
                Self::setting_search_score(setting, &query, &terms).map(|score| (score, setting))
            })
            .collect();

        matches.sort_by(|(left_score, left_setting), (right_score, right_setting)| {
            right_score.cmp(left_score).then_with(|| {
                left_setting
                    .metadata
                    .title
                    .cmp(right_setting.metadata.title)
            })
        });

        matches
            .into_iter()
            .map(|(_, setting)| setting)
            .take(limit)
            .collect()
    }

    fn wrap_setting_with_scroll_anchor(
        &self,
        setting_key: &'static str,
        content: AnyElement,
    ) -> AnyElement {
        div()
            .id(SharedString::from(format!("setting-{setting_key}")))
            .anchor_scroll(self.setting_scroll_anchors.get(setting_key).cloned())
            .child(content)
            .into_any_element()
    }

    fn blur_sidebar_search(&mut self) {
        self.sidebar_search_active = false;
        self.sidebar_search_selecting = false;
        self.search_navigation_last_target = None;
        self.search_navigation_last_jump_at = None;
    }

    fn start_smooth_scroll_animation(
        &mut self,
        start_offset: gpui::Point<gpui::Pixels>,
        target_offset: gpui::Point<gpui::Pixels>,
        cx: &mut Context<Self>,
    ) {
        let start_x: f32 = start_offset.x.into();
        let start_y: f32 = start_offset.y.into();
        let target_x: f32 = target_offset.x.into();
        let target_y: f32 = target_offset.y.into();
        if (start_x - target_x).abs() < 0.5 && (start_y - target_y).abs() < 0.5 {
            self.content_scroll_handle.set_offset(target_offset);
            cx.notify();
            return;
        }

        self.scroll_animation_token = self.scroll_animation_token.wrapping_add(1);
        let token = self.scroll_animation_token;
        let scroll_handle = self.content_scroll_handle.clone();
        let duration = Duration::from_millis(SETTINGS_SCROLL_ANIMATION_DURATION_MS);

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let started_at = Instant::now();

            loop {
                smol::Timer::after(Duration::from_millis(SETTINGS_SCROLL_ANIMATION_TICK_MS)).await;

                let continue_animating = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        if view.scroll_animation_token != token {
                            return false;
                        }

                        let t = (started_at.elapsed().as_secs_f32() / duration.as_secs_f32())
                            .clamp(0.0, 1.0);
                        let eased = t * t * (3.0 - 2.0 * t);
                        let x = start_x + (target_x - start_x) * eased;
                        let y = start_y + (target_y - start_y) * eased;
                        scroll_handle.set_offset(point(px(x), px(y)));
                        cx.notify();
                        t < 1.0
                    })
                    .unwrap_or(false)
                });

                if !continue_animating {
                    break;
                }
            }
        })
        .detach();
    }

    fn jump_to_setting(
        &mut self,
        setting_key: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(setting) = self.searchable_setting_by_key(setting_key) else {
            return;
        };

        self.active_section = setting.metadata.section;
        self.active_input = None;
        self.sidebar_search_active = true;
        self.sidebar_search_selecting = false;
        if !self.focus_handle.is_focused(window) {
            self.focus_handle.focus(window, cx);
        }

        if let Some(anchor) = self.setting_scroll_anchors.get(setting_key).cloned() {
            let this = cx.entity().downgrade();
            let scroll_handle = self.content_scroll_handle.clone();
            let start_offset = scroll_handle.offset();

            window.on_next_frame(move |window, cx| {
                anchor.scroll_to(window, cx);
                let target_offset = scroll_handle.offset();
                scroll_handle.set_offset(start_offset);
                let _ = this.update(cx, |view, cx| {
                    view.start_smooth_scroll_animation(start_offset, target_offset, cx);
                });
            });
        }

        cx.notify();
    }

    fn jump_to_first_search_result(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(first_key) = self
            .sidebar_search_results(1)
            .into_iter()
            .next()
            .map(|setting| setting.metadata.key)
        {
            self.jump_to_setting(first_key, window, cx);
        }
    }

    fn refresh_search_navigation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.sidebar_search_active
            && self.active_input.is_none()
            && !self.sidebar_search_state.text().trim().is_empty()
        {
            let first_key = self
                .sidebar_search_results(1)
                .into_iter()
                .next()
                .map(|setting| setting.metadata.key);
            let Some(first_key) = first_key else {
                self.search_navigation_last_target = None;
                self.search_navigation_last_jump_at = None;
                cx.notify();
                return;
            };

            let now = Instant::now();
            let within_throttle = self.search_navigation_last_jump_at.is_some_and(|last| {
                now.duration_since(last) < Duration::from_millis(SETTINGS_SEARCH_NAV_THROTTLE_MS)
            });
            if self.search_navigation_last_target == Some(first_key) {
                cx.notify();
                return;
            }
            if within_throttle {
                cx.notify();
                return;
            }

            self.search_navigation_last_target = Some(first_key);
            self.search_navigation_last_jump_at = Some(now);
            self.jump_to_setting(first_key, window, cx);
        } else {
            self.search_navigation_last_target = None;
            self.search_navigation_last_jump_at = None;
            cx.notify();
        }
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

    // Color helpers derived from terminal theme
    fn background_opacity_factor(&self) -> f32 {
        self.config.background_opacity.clamp(0.0, 1.0)
    }

    fn scaled_background_alpha(&self, base_alpha: f32) -> f32 {
        (base_alpha * self.background_opacity_factor()).clamp(0.0, 1.0)
    }

    fn adaptive_panel_alpha(&self, base_alpha: f32) -> f32 {
        let floor = base_alpha * SETTINGS_OVERLAY_PANEL_ALPHA_FLOOR_RATIO;
        self.scaled_background_alpha(base_alpha)
            .max(floor)
            .clamp(0.0, 1.0)
    }

    fn sync_window_background_appearance(&mut self, window: &mut Window) {
        let appearance = crate::terminal_view::initial_window_background_appearance(&self.config);
        if self.last_window_background_appearance != Some(appearance) {
            window.set_background_appearance(appearance);
            self.last_window_background_appearance = Some(appearance);
        }
    }

    fn bg_primary(&self) -> Rgba {
        let mut c = self.colors.background;
        c.a = self.scaled_background_alpha(c.a);
        c
    }

    fn bg_secondary(&self) -> Rgba {
        let mut c = self.colors.background;
        c.a = self.adaptive_panel_alpha(0.7);
        c
    }

    fn bg_card(&self) -> Rgba {
        let mut c = self.colors.background;
        c.a = self.adaptive_panel_alpha(0.5);
        c
    }

    fn bg_input(&self) -> Rgba {
        let mut c = self.colors.background;
        c.a = self.adaptive_panel_alpha(0.3);
        c
    }

    fn bg_hover(&self) -> Rgba {
        let mut c = self.colors.foreground;
        c.a = 0.1;
        c
    }

    fn bg_active(&self) -> Rgba {
        let mut c = self.colors.foreground;
        c.a = 0.15;
        c
    }

    fn text_primary(&self) -> Rgba {
        self.colors.foreground
    }

    fn text_secondary(&self) -> Rgba {
        let mut c = self.colors.foreground;
        c.a = 0.7;
        c
    }

    fn text_muted(&self) -> Rgba {
        let mut c = self.colors.foreground;
        c.a = 0.5;
        c
    }

    fn border_color(&self) -> Rgba {
        let mut c = self.colors.foreground;
        c.a = 0.15;
        c
    }

    fn accent(&self) -> Rgba {
        self.colors.cursor
    }

    fn accent_with_alpha(&self, alpha: f32) -> Rgba {
        let mut c = self.colors.cursor;
        c.a = alpha;
        c
    }

    fn settings_scrollbar_style(&self) -> ScrollbarPaintStyle {
        let mut track = self.colors.foreground;
        track.a = self.adaptive_panel_alpha(SETTINGS_SCROLLBAR_TRACK_ALPHA);

        let mut thumb = self.colors.foreground;
        thumb.a = self.adaptive_panel_alpha(SETTINGS_SCROLLBAR_THUMB_ALPHA);

        let mut active_thumb = self.colors.foreground;
        active_thumb.a = self.adaptive_panel_alpha(SETTINGS_SCROLLBAR_THUMB_ACTIVE_ALPHA);

        ScrollbarPaintStyle {
            width: SETTINGS_SCROLLBAR_WIDTH,
            track_radius: 0.0,
            thumb_radius: 0.0,
            thumb_inset: 0.0,
            marker_inset: 0.0,
            marker_radius: 0.0,
            track_color: track,
            thumb_color: thumb,
            active_thumb_color: active_thumb,
            marker_color: None,
            current_marker_color: None,
        }
    }

    fn settings_scrollbar_metrics(&self, window: &Window) -> Option<ui_scrollbar::ScrollbarMetrics> {
        let viewport_height: f32 = window.viewport_size().height.into();
        let max_offset: f32 = self.content_scroll_handle.max_offset().height.into();
        let offset_y: f32 = self.content_scroll_handle.offset().y.into();
        let offset = (-offset_y).max(0.0);
        let range = ScrollbarRange {
            offset,
            max_offset,
            viewport_extent: viewport_height,
            track_extent: viewport_height,
        };

        ui_scrollbar::compute_metrics(range, SETTINGS_SCROLLBAR_MIN_THUMB_HEIGHT)
    }

    fn srgb_to_linear(channel: f32) -> f32 {
        if channel <= 0.04045 {
            channel / 12.92
        } else {
            ((channel + 0.055) / 1.055).powf(2.4)
        }
    }

    fn composite_over(fg: Rgba, bg: Rgba) -> Rgba {
        let fg_alpha = fg.a.clamp(0.0, 1.0);
        Rgba {
            r: (fg_alpha * fg.r + (1.0 - fg_alpha) * bg.r).clamp(0.0, 1.0),
            g: (fg_alpha * fg.g + (1.0 - fg_alpha) * bg.g).clamp(0.0, 1.0),
            b: (fg_alpha * fg.b + (1.0 - fg_alpha) * bg.b).clamp(0.0, 1.0),
            a: 1.0,
        }
    }

    fn relative_luminance(color: Rgba, backdrop: Rgba) -> f32 {
        let composited = Self::composite_over(color, backdrop);
        let r = Self::srgb_to_linear(composited.r);
        let g = Self::srgb_to_linear(composited.g);
        let b = Self::srgb_to_linear(composited.b);
        0.2126 * r + 0.7152 * g + 0.0722 * b
    }

    fn contrast_ratio(a: Rgba, b: Rgba, backdrop: Rgba) -> f32 {
        let l1 = Self::relative_luminance(a, backdrop);
        let l2 = Self::relative_luminance(b, backdrop);
        let (lighter, darker) = if l1 >= l2 { (l1, l2) } else { (l2, l1) };
        (lighter + 0.05) / (darker + 0.05)
    }

    fn contrasting_text_for_fill(&self, fill: Rgba, backdrop: Rgba) -> Rgba {
        let mut primary = self.text_primary();
        primary.a = 1.0;
        let mut dark = self.bg_primary();
        dark.a = 1.0;
        let mut backdrop = backdrop;
        backdrop.a = 1.0;
        let composited_fill = Self::composite_over(fill, backdrop);

        if Self::contrast_ratio(primary, composited_fill, backdrop)
            >= Self::contrast_ratio(dark, composited_fill, backdrop)
        {
            primary
        } else {
            dark
        }
    }

    fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(SIDEBAR_WIDTH))
            .h_full()
            .bg(self.bg_secondary())
            .border_r_1()
            .border_color(self.border_color())
            .flex()
            .flex_col()
            .child(
                div().px_5().pt_10().pb_2().child(
                    div()
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(self.text_muted())
                        .child("SETTINGS"),
                ),
            )
            .child(self.render_sidebar_search(cx))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .px_3()
                    .child(self.render_sidebar_item("Appearance", SettingsSection::Appearance, cx))
                    .child(self.render_sidebar_item("Terminal", SettingsSection::Terminal, cx))
                    .child(self.render_sidebar_item("Tabs", SettingsSection::Tabs, cx))
                    .child(self.render_sidebar_item("Advanced", SettingsSection::Advanced, cx))
                    .child(self.render_sidebar_item("Colors", SettingsSection::Colors, cx))
                    .child(self.render_sidebar_item(
                        "Keybindings",
                        SettingsSection::Keybindings,
                        cx,
                    )),
            )
    }

    fn render_sidebar_search(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let query_text = self.sidebar_search_state.text().to_string();
        let has_query = !query_text.trim().is_empty();
        let is_active = self.sidebar_search_active;
        let text_secondary = self.text_secondary();
        let text_muted = self.text_muted();
        let bg_input = self.bg_input();
        let border_color = self.border_color();
        let accent = self.accent();

        let search_content = if is_active {
            let font = Font {
                family: self.config.font_family.clone().into(),
                ..Font::default()
            };
            TextInputElement::new(
                cx.entity(),
                self.focus_handle.clone(),
                font,
                px(13.0),
                text_secondary.into(),
                self.accent_with_alpha(0.3).into(),
                TextInputAlignment::Left,
            )
            .into_any_element()
        } else if has_query {
            div()
                .text_sm()
                .text_color(text_secondary)
                .child(query_text.clone())
                .into_any_element()
        } else {
            div()
                .text_sm()
                .text_color(text_muted)
                .child("Search settings...")
                .into_any_element()
        };

        let search_container = div().id("settings-sidebar-search").px_3().pb_3().child(
            div()
                .id("settings-sidebar-search-input")
                .h(px(36.0))
                .px_3()
                .rounded(px(0.0))
                .bg(bg_input)
                .border_1()
                .border_color(if is_active {
                    accent.into()
                } else {
                    border_color
                })
                .overflow_hidden()
                .cursor_text()
                .flex()
                .items_center()
                .child(
                    div()
                        .w_full()
                        .h(px(20.0))
                        .overflow_hidden()
                        .child(search_content),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        view.active_input = None;
                        view.sidebar_search_active = true;

                        let index = view
                            .sidebar_search_state
                            .character_index_for_point(event.position);
                        if event.modifiers.shift {
                            view.sidebar_search_state.select_to_utf16(index);
                        } else {
                            view.sidebar_search_state.set_cursor_utf16(index);
                        }
                        view.sidebar_search_selecting = true;
                        view.refresh_search_navigation(window, cx);
                        view.focus_handle.focus(window, cx);
                        cx.notify();
                    }),
                )
                .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _window, cx| {
                    if !view.sidebar_search_selecting || !event.dragging() {
                        return;
                    }
                    let index = view
                        .sidebar_search_state
                        .character_index_for_point(event.position);
                    view.sidebar_search_state.select_to_utf16(index);
                    cx.notify();
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|view, _event: &MouseUpEvent, _window, cx| {
                        if view.sidebar_search_selecting {
                            view.sidebar_search_selecting = false;
                            cx.notify();
                        }
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|view, _event: &MouseUpEvent, _window, cx| {
                        if view.sidebar_search_selecting {
                            view.sidebar_search_selecting = false;
                            cx.notify();
                        }
                    }),
                ),
        );

        search_container
    }

    fn render_sidebar_item(
        &self,
        label: &'static str,
        section: SettingsSection,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_active = self.active_section == section;
        let active_bg = self.bg_active();
        let hover_bg = self.bg_hover();
        let text_primary = self.text_primary();
        let text_secondary = self.text_secondary();
        let accent = self.accent();

        div()
            .id(SharedString::from(label))
            .px_3()
            .py(px(10.0))
            .rounded(px(0.0))
            .cursor_pointer()
            .flex()
            .items_center()
            .gap_3()
            .bg(if is_active {
                active_bg
            } else {
                Rgba {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                }
            })
            .hover(|s| s.bg(hover_bg))
            .child(
                div()
                    .text_sm()
                    .font_weight(if is_active {
                        gpui::FontWeight::MEDIUM
                    } else {
                        gpui::FontWeight::NORMAL
                    })
                    .text_color(if is_active {
                        text_primary
                    } else {
                        text_secondary
                    })
                    .child(label),
            )
            .when(is_active, |s| {
                s.child(
                    div()
                        .ml_auto()
                        .w(px(3.0))
                        .h(px(16.0))
                        .rounded(px(0.0))
                        .bg(accent),
                )
            })
            .on_click(cx.listener(move |view, _, _, cx| {
                view.active_section = section;
                view.active_input = None;
                view.blur_sidebar_search();
                cx.notify();
            }))
    }

    fn root_setting_for_editable_field(field: EditableField) -> Option<RootSettingId> {
        match field {
            EditableField::Theme => Some(RootSettingId::Theme),
            EditableField::BackgroundOpacity => Some(RootSettingId::BackgroundOpacity),
            EditableField::FontFamily => Some(RootSettingId::FontFamily),
            EditableField::FontSize => Some(RootSettingId::FontSize),
            EditableField::PaddingX => Some(RootSettingId::PaddingX),
            EditableField::PaddingY => Some(RootSettingId::PaddingY),
            EditableField::Shell => Some(RootSettingId::Shell),
            EditableField::Term => Some(RootSettingId::Term),
            EditableField::Colorterm => Some(RootSettingId::Colorterm),
            EditableField::ScrollbackHistory => Some(RootSettingId::ScrollbackHistory),
            EditableField::InactiveTabScrollback => Some(RootSettingId::InactiveTabScrollback),
            EditableField::ScrollMultiplier => Some(RootSettingId::MouseScrollMultiplier),
            EditableField::CursorStyle => Some(RootSettingId::CursorStyle),
            EditableField::ScrollbarVisibility => Some(RootSettingId::ScrollbarVisibility),
            EditableField::ScrollbarStyle => Some(RootSettingId::ScrollbarStyle),
            EditableField::TabFallbackTitle => Some(RootSettingId::TabTitleFallback),
            EditableField::TabTitlePriority => Some(RootSettingId::TabTitlePriority),
            EditableField::TabTitleMode => Some(RootSettingId::TabTitleMode),
            EditableField::TabTitleExplicitPrefix => Some(RootSettingId::TabTitleExplicitPrefix),
            EditableField::TabTitlePromptFormat => Some(RootSettingId::TabTitlePromptFormat),
            EditableField::TabTitleCommandFormat => Some(RootSettingId::TabTitleCommandFormat),
            EditableField::TabCloseVisibility => Some(RootSettingId::TabCloseVisibility),
            EditableField::TabWidthMode => Some(RootSettingId::TabWidthMode),
            EditableField::WorkingDirectory => Some(RootSettingId::WorkingDir),
            EditableField::WorkingDirFallback => Some(RootSettingId::WorkingDirFallback),
            EditableField::WindowWidth => Some(RootSettingId::WindowWidth),
            EditableField::WindowHeight => Some(RootSettingId::WindowHeight),
            EditableField::KeybindDirectives | EditableField::Color(_) => None,
        }
    }

    fn enum_root_setting_for_field(field: EditableField) -> Option<RootSettingId> {
        let setting = Self::root_setting_for_editable_field(field)?;
        (root_setting_value_kind(setting) == RootSettingValueKind::Enum).then_some(setting)
    }

    fn field_uses_dropdown(field: EditableField) -> bool {
        matches!(field, EditableField::Theme | EditableField::FontFamily)
            || Self::enum_root_setting_for_field(field).is_some()
    }

    fn dropdown_option_for_enum_choice(value: &str, label: &str) -> DropdownOption {
        DropdownOption::labeled(value.to_string(), label.to_string(), true)
    }

    fn normalize_dropdown_query_token(value: &str) -> String {
        value
            .trim()
            .to_ascii_lowercase()
            .chars()
            .filter(|ch| !matches!(ch, '_' | '-' | ' ' | '+'))
            .collect()
    }

    fn filtered_enum_suggestions(&self, field: EditableField, query: &str) -> Vec<DropdownOption> {
        let Some(setting) = Self::enum_root_setting_for_field(field) else {
            return Vec::new();
        };
        let Some(choices) = root_setting_enum_choices(setting) else {
            return Vec::new();
        };

        let mut options = choices
            .iter()
            .map(|choice| Self::dropdown_option_for_enum_choice(choice.value, choice.label))
            .collect::<Vec<_>>();

        let trimmed_query = query.trim();
        let normalized_query = trimmed_query.to_ascii_lowercase();
        let normalized_compact = Self::normalize_dropdown_query_token(trimmed_query);
        if normalized_query.is_empty() {
            let current_value = self.editable_field_value(field);
            if let Some(index) = options
                .iter()
                .position(|option| option.value.eq_ignore_ascii_case(&current_value))
            {
                let selected = options.remove(index);
                options.insert(0, selected);
            } else if !current_value.trim().is_empty() {
                options.insert(0, DropdownOption::raw(current_value));
            }
            return options;
        }

        let mut matched = options
            .into_iter()
            .filter(|option| {
                let value_lower = option.value.to_ascii_lowercase();
                let label_lower = option.label.to_ascii_lowercase();
                let value_compact = Self::normalize_dropdown_query_token(&option.value);
                let label_compact = Self::normalize_dropdown_query_token(&option.label);
                value_lower.contains(&normalized_query)
                    || label_lower.contains(&normalized_query)
                    || (!normalized_compact.is_empty()
                        && (value_compact.contains(&normalized_compact)
                            || label_compact.contains(&normalized_compact)))
            })
            .collect::<Vec<_>>();

        if !trimmed_query.is_empty()
            && !matched
                .iter()
                .any(|option| {
                    option.value.eq_ignore_ascii_case(trimmed_query)
                        || Self::normalize_dropdown_query_token(&option.value)
                            == normalized_compact
                })
        {
            matched.insert(0, DropdownOption::raw(trimmed_query.to_string()));
        }

        matched
    }

    fn dropdown_options_for_field(&self, field: EditableField, query: &str) -> Vec<DropdownOption> {
        if field == EditableField::Theme {
            return self
                .filtered_theme_suggestions(query)
                .into_iter()
                .map(DropdownOption::raw)
                .collect();
        }
        if field == EditableField::FontFamily {
            return self
                .filtered_font_suggestions(query)
                .into_iter()
                .map(DropdownOption::raw)
                .collect();
        }

        self.filtered_enum_suggestions(field, query)
    }

    fn dropdown_display_value(&self, field: EditableField, raw_value: &str) -> String {
        let Some(setting) = Self::enum_root_setting_for_field(field) else {
            return raw_value.to_string();
        };
        let Some(choices) = root_setting_enum_choices(setting) else {
            return raw_value.to_string();
        };
        let Some(choice) = choices
            .iter()
            .find(|choice| choice.value.eq_ignore_ascii_case(raw_value))
        else {
            return raw_value.to_string();
        };
        Self::dropdown_option_for_enum_choice(choice.value, choice.label).display_text()
    }

    fn apply_dropdown_selection(
        &mut self,
        field: EditableField,
        selected_value: &str,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = self.apply_editable_field(field, selected_value) {
            termy_toast::error(error);
            return;
        }
        self.active_input = None;
        cx.notify();
    }

    fn commit_dropdown_selection(
        &mut self,
        field: EditableField,
        query: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        if !Self::field_uses_dropdown(field) {
            return false;
        }

        let Some(first_option) = self
            .dropdown_options_for_field(field, query)
            .into_iter()
            .next()
        else {
            self.cancel_active_input(cx);
            return true;
        };

        self.apply_dropdown_selection(field, &first_option.value, cx);
        true
    }

    fn editable_field_value(&self, field: EditableField) -> String {
        match field {
            EditableField::Theme => self.config.theme.clone(),
            EditableField::BackgroundOpacity => format!(
                "{}",
                (self.config.background_opacity * 100.0).round() as i32
            ),
            EditableField::FontFamily => self.config.font_family.clone(),
            EditableField::FontSize => format!("{}", self.config.font_size.round() as i32),
            EditableField::PaddingX => format!("{}", self.config.padding_x.round() as i32),
            EditableField::PaddingY => format!("{}", self.config.padding_y.round() as i32),
            EditableField::Shell => self.config.shell.clone().unwrap_or_default(),
            EditableField::Term => self.config.term.clone(),
            EditableField::Colorterm => self.config.colorterm.clone().unwrap_or_default(),
            EditableField::ScrollbackHistory => self.config.scrollback_history.to_string(),
            EditableField::InactiveTabScrollback => self
                .config
                .inactive_tab_scrollback
                .map(|value| value.to_string())
                .unwrap_or_default(),
            EditableField::ScrollMultiplier => format!("{}", self.config.mouse_scroll_multiplier),
            EditableField::CursorStyle => match self.config.cursor_style {
                termy_config_core::CursorStyle::Line => "line",
                termy_config_core::CursorStyle::Block => "block",
            }
            .to_string(),
            EditableField::ScrollbarVisibility => {
                match self.config.terminal_scrollbar_visibility {
                    termy_config_core::TerminalScrollbarVisibility::Off => "off",
                    termy_config_core::TerminalScrollbarVisibility::Always => "always",
                    termy_config_core::TerminalScrollbarVisibility::OnScroll => "on_scroll",
                }
                .to_string()
            }
            EditableField::ScrollbarStyle => {
                match self.config.terminal_scrollbar_style {
                    termy_config_core::TerminalScrollbarStyle::Neutral => "neutral",
                    termy_config_core::TerminalScrollbarStyle::MutedTheme => "muted_theme",
                    termy_config_core::TerminalScrollbarStyle::Theme => "theme",
                }
                .to_string()
            }
            EditableField::TabFallbackTitle => self.config.tab_title.fallback.clone(),
            EditableField::TabTitlePriority => self
                .config
                .tab_title
                .priority
                .iter()
                .map(|source| match source {
                    termy_config_core::TabTitleSource::Manual => "manual",
                    termy_config_core::TabTitleSource::Explicit => "explicit",
                    termy_config_core::TabTitleSource::Shell => "shell",
                    termy_config_core::TabTitleSource::Fallback => "fallback",
                })
                .collect::<Vec<_>>()
                .join(", "),
            EditableField::TabTitleMode => match self.config.tab_title.mode {
                termy_config_core::TabTitleMode::Smart => "smart",
                termy_config_core::TabTitleMode::Shell => "shell",
                termy_config_core::TabTitleMode::Explicit => "explicit",
                termy_config_core::TabTitleMode::Static => "static",
            }
            .to_string(),
            EditableField::TabTitleExplicitPrefix => self.config.tab_title.explicit_prefix.clone(),
            EditableField::TabTitlePromptFormat => self.config.tab_title.prompt_format.clone(),
            EditableField::TabTitleCommandFormat => self.config.tab_title.command_format.clone(),
            EditableField::TabCloseVisibility => match self.config.tab_close_visibility {
                termy_config_core::TabCloseVisibility::ActiveHover => "active_hover",
                termy_config_core::TabCloseVisibility::Hover => "hover",
                termy_config_core::TabCloseVisibility::Always => "always",
            }
            .to_string(),
            EditableField::TabWidthMode => match self.config.tab_width_mode {
                termy_config_core::TabWidthMode::Stable => "stable",
                termy_config_core::TabWidthMode::ActiveGrow => "active_grow",
                termy_config_core::TabWidthMode::ActiveGrowSticky => "active_grow_sticky",
            }
            .to_string(),
            EditableField::KeybindDirectives => self
                .config
                .keybind_lines
                .iter()
                .map(|line| line.value.as_str())
                .collect::<Vec<_>>()
                .join("; "),
            EditableField::WorkingDirectory => self.config.working_dir.clone().unwrap_or_default(),
            EditableField::WorkingDirFallback => match self.config.working_dir_fallback {
                termy_config_core::WorkingDirFallback::Home => "home",
                termy_config_core::WorkingDirFallback::Process => "process",
            }
            .to_string(),
            EditableField::WindowWidth => format!("{}", self.config.window_width.round() as i32),
            EditableField::WindowHeight => format!("{}", self.config.window_height.round() as i32),
            EditableField::Color(id) => self
                .custom_color_for_id(id)
                .map(|rgb| format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b))
                .unwrap_or_default(),
        }
    }

    fn apply_editable_field(&mut self, field: EditableField, raw: &str) -> Result<(), String> {
        let value = raw.trim();
        match field {
            EditableField::Theme => {
                if value.is_empty() {
                    return Err("Theme cannot be empty".to_string());
                }
                let message = crate::config::set_theme_in_config(value)?;
                let canonical_theme = message
                    .strip_prefix("Theme set to ")
                    .unwrap_or(value)
                    .to_string();
                self.config.theme = canonical_theme;
                Ok(())
            }
            EditableField::BackgroundOpacity => {
                let parsed = value
                    .trim_end_matches('%')
                    .parse::<f32>()
                    .map_err(|_| "Background opacity must be a number from 0 to 100".to_string())?;
                let opacity = (parsed / 100.0).clamp(0.0, 1.0);
                self.config.background_opacity = opacity;
                config::set_root_setting(termy_config_core::RootSettingId::BackgroundOpacity, &format!("{:.3}", opacity))
            }
            EditableField::FontFamily => {
                if value.is_empty() {
                    return Err("Font family cannot be empty".to_string());
                }
                self.config.font_family = value.to_string();
                config::set_root_setting(termy_config_core::RootSettingId::FontFamily, value)
            }
            EditableField::FontSize => {
                let parsed = value
                    .parse::<f32>()
                    .map_err(|_| "Font size must be a positive number".to_string())?;
                if parsed <= 0.0 {
                    return Err("Font size must be greater than 0".to_string());
                }
                self.config.font_size = parsed;
                config::set_root_setting(termy_config_core::RootSettingId::FontSize, &format!("{}", parsed))
            }
            EditableField::PaddingX => {
                let parsed = value
                    .parse::<f32>()
                    .map_err(|_| "Horizontal padding must be a number".to_string())?;
                if parsed < 0.0 {
                    return Err("Horizontal padding cannot be negative".to_string());
                }
                self.config.padding_x = parsed;
                config::set_root_setting(termy_config_core::RootSettingId::PaddingX, &format!("{}", parsed))
            }
            EditableField::PaddingY => {
                let parsed = value
                    .parse::<f32>()
                    .map_err(|_| "Vertical padding must be a number".to_string())?;
                if parsed < 0.0 {
                    return Err("Vertical padding cannot be negative".to_string());
                }
                self.config.padding_y = parsed;
                config::set_root_setting(termy_config_core::RootSettingId::PaddingY, &format!("{}", parsed))
            }
            EditableField::Shell => {
                if value.is_empty() {
                    self.config.shell = None;
                    config::set_root_setting(termy_config_core::RootSettingId::Shell, "none")
                } else {
                    self.config.shell = Some(value.to_string());
                    config::set_root_setting(termy_config_core::RootSettingId::Shell, value)
                }
            }
            EditableField::Term => {
                if value.is_empty() {
                    return Err("TERM cannot be empty".to_string());
                }
                self.config.term = value.to_string();
                config::set_root_setting(termy_config_core::RootSettingId::Term, value)
            }
            EditableField::Colorterm => {
                if value.is_empty() {
                    self.config.colorterm = None;
                    config::set_root_setting(termy_config_core::RootSettingId::Colorterm, "none")
                } else {
                    self.config.colorterm = Some(value.to_string());
                    config::set_root_setting(termy_config_core::RootSettingId::Colorterm, value)
                }
            }
            EditableField::ScrollbackHistory => {
                let parsed = value
                    .parse::<usize>()
                    .map_err(|_| "Scrollback history must be a positive integer".to_string())?;
                let parsed = parsed.min(100_000);
                self.config.scrollback_history = parsed;
                config::set_root_setting(termy_config_core::RootSettingId::ScrollbackHistory, &parsed.to_string())
            }
            EditableField::InactiveTabScrollback => {
                let parsed = value
                    .parse::<usize>()
                    .map_err(|_| "Inactive tab scrollback must be a positive integer".to_string())?;
                let parsed = parsed.min(100_000);
                self.config.inactive_tab_scrollback = Some(parsed);
                config::set_root_setting(termy_config_core::RootSettingId::InactiveTabScrollback, &parsed.to_string())
            }
            EditableField::ScrollMultiplier => {
                let parsed = value
                    .parse::<f32>()
                    .map_err(|_| "Scroll multiplier must be a number".to_string())?;
                if !parsed.is_finite() {
                    return Err("Scroll multiplier must be finite".to_string());
                }
                let parsed = parsed.clamp(0.1, 1000.0);
                self.config.mouse_scroll_multiplier = parsed;
                config::set_root_setting(termy_config_core::RootSettingId::MouseScrollMultiplier, &parsed.to_string())
            }
            EditableField::CursorStyle => {
                let parsed = match value.to_ascii_lowercase().as_str() {
                    "line" | "bar" | "beam" | "ibeam" => termy_config_core::CursorStyle::Line,
                    "block" | "box" => termy_config_core::CursorStyle::Block,
                    _ => return Err("Cursor style must be line or block".to_string()),
                };
                self.config.cursor_style = parsed;
                let canonical = match parsed {
                    termy_config_core::CursorStyle::Line => "line",
                    termy_config_core::CursorStyle::Block => "block",
                };
                config::set_root_setting(termy_config_core::RootSettingId::CursorStyle, canonical)
            }
            EditableField::ScrollbarVisibility => {
                let parsed = match value.to_ascii_lowercase().as_str() {
                    "off" => termy_config_core::TerminalScrollbarVisibility::Off,
                    "always" => termy_config_core::TerminalScrollbarVisibility::Always,
                    "on_scroll" | "onscroll" => termy_config_core::TerminalScrollbarVisibility::OnScroll,
                    _ => {
                        return Err("Scrollbar visibility must be off, always, or on_scroll".to_string())
                    }
                };
                self.config.terminal_scrollbar_visibility = parsed;
                let canonical = match parsed {
                    termy_config_core::TerminalScrollbarVisibility::Off => "off",
                    termy_config_core::TerminalScrollbarVisibility::Always => "always",
                    termy_config_core::TerminalScrollbarVisibility::OnScroll => "on_scroll",
                };
                config::set_root_setting(
                    termy_config_core::RootSettingId::ScrollbarVisibility,
                    canonical,
                )
            }
            EditableField::ScrollbarStyle => {
                let parsed = match value.to_ascii_lowercase().as_str() {
                    "neutral" => termy_config_core::TerminalScrollbarStyle::Neutral,
                    "muted_theme" | "mutedtheme" => {
                        termy_config_core::TerminalScrollbarStyle::MutedTheme
                    }
                    "theme" => termy_config_core::TerminalScrollbarStyle::Theme,
                    _ => return Err("Scrollbar style must be neutral, muted_theme, or theme".to_string()),
                };
                self.config.terminal_scrollbar_style = parsed;
                let canonical = match parsed {
                    termy_config_core::TerminalScrollbarStyle::Neutral => "neutral",
                    termy_config_core::TerminalScrollbarStyle::MutedTheme => "muted_theme",
                    termy_config_core::TerminalScrollbarStyle::Theme => "theme",
                };
                config::set_root_setting(termy_config_core::RootSettingId::ScrollbarStyle, canonical)
            }
            EditableField::TabFallbackTitle => {
                if value.is_empty() {
                    return Err("Fallback title cannot be empty".to_string());
                }
                self.config.tab_title.fallback = value.to_string();
                config::set_root_setting(termy_config_core::RootSettingId::TabTitleFallback, value)
            }
            EditableField::TabTitlePriority => {
                if value.is_empty() {
                    return Err("Title priority cannot be empty".to_string());
                }
                self.config.tab_title.priority = value
                    .split(',')
                    .filter_map(Self::parse_tab_title_source_token)
                    .fold(Vec::new(), |mut acc, source| {
                        if !acc.contains(&source) {
                            acc.push(source);
                        }
                        acc
                    });
                if self.config.tab_title.priority.is_empty() {
                    return Err("Title priority must contain valid sources".to_string());
                }
                config::set_root_setting(termy_config_core::RootSettingId::TabTitlePriority, value)
            }
            EditableField::TabTitleMode => {
                let parsed = match value.to_ascii_lowercase().as_str() {
                    "smart" => termy_config_core::TabTitleMode::Smart,
                    "shell" => termy_config_core::TabTitleMode::Shell,
                    "explicit" => termy_config_core::TabTitleMode::Explicit,
                    "static" => termy_config_core::TabTitleMode::Static,
                    _ => {
                        return Err(
                            "Tab title mode must be smart, shell, explicit, or static"
                                .to_string(),
                        )
                    }
                };
                self.config.tab_title.mode = parsed;
                let canonical = match parsed {
                    termy_config_core::TabTitleMode::Smart => "smart",
                    termy_config_core::TabTitleMode::Shell => "shell",
                    termy_config_core::TabTitleMode::Explicit => "explicit",
                    termy_config_core::TabTitleMode::Static => "static",
                };
                config::set_root_setting(termy_config_core::RootSettingId::TabTitleMode, canonical)
            }
            EditableField::TabTitleExplicitPrefix => {
                if value.is_empty() {
                    return Err("Explicit prefix cannot be empty".to_string());
                }
                self.config.tab_title.explicit_prefix = value.to_string();
                config::set_root_setting(termy_config_core::RootSettingId::TabTitleExplicitPrefix, value)
            }
            EditableField::TabTitlePromptFormat => {
                if value.is_empty() {
                    return Err("Prompt format cannot be empty".to_string());
                }
                self.config.tab_title.prompt_format = value.to_string();
                config::set_root_setting(termy_config_core::RootSettingId::TabTitlePromptFormat, value)
            }
            EditableField::TabTitleCommandFormat => {
                if value.is_empty() {
                    return Err("Command format cannot be empty".to_string());
                }
                self.config.tab_title.command_format = value.to_string();
                config::set_root_setting(termy_config_core::RootSettingId::TabTitleCommandFormat, value)
            }
            EditableField::TabCloseVisibility => {
                let parsed = match value.to_ascii_lowercase().as_str() {
                    "active_hover" | "activehover" | "active+hover" => {
                        termy_config_core::TabCloseVisibility::ActiveHover
                    }
                    "hover" => termy_config_core::TabCloseVisibility::Hover,
                    "always" => termy_config_core::TabCloseVisibility::Always,
                    _ => return Err("Tab close visibility must be active_hover, hover, or always".to_string()),
                };
                self.config.tab_close_visibility = parsed;
                let canonical = match parsed {
                    termy_config_core::TabCloseVisibility::ActiveHover => "active_hover",
                    termy_config_core::TabCloseVisibility::Hover => "hover",
                    termy_config_core::TabCloseVisibility::Always => "always",
                };
                config::set_root_setting(
                    termy_config_core::RootSettingId::TabCloseVisibility,
                    canonical,
                )
            }
            EditableField::TabWidthMode => {
                let parsed = match value.to_ascii_lowercase().as_str() {
                    "stable" => termy_config_core::TabWidthMode::Stable,
                    "active_grow" | "activegrow" | "active-grow" => {
                        termy_config_core::TabWidthMode::ActiveGrow
                    }
                    "active_grow_sticky" | "activegrowsticky" | "active-grow-sticky" => {
                        termy_config_core::TabWidthMode::ActiveGrowSticky
                    }
                    _ => {
                        return Err(
                            "Tab width mode must be stable, active_grow, or active_grow_sticky"
                                .to_string(),
                        )
                    }
                };
                self.config.tab_width_mode = parsed;
                let canonical = match parsed {
                    termy_config_core::TabWidthMode::Stable => "stable",
                    termy_config_core::TabWidthMode::ActiveGrow => "active_grow",
                    termy_config_core::TabWidthMode::ActiveGrowSticky => "active_grow_sticky",
                };
                config::set_root_setting(termy_config_core::RootSettingId::TabWidthMode, canonical)
            }
            EditableField::KeybindDirectives => {
                let lines = value
                    .split(['\n', ';'])
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(ToString::to_string)
                    .collect::<Vec<_>>();
                let (_directives, warnings) =
                    termy_command_core::parse_keybind_directives_from_iter(
                        lines.iter().enumerate().map(|(index, line)| {
                            termy_command_core::KeybindLineRef {
                                line_number: index + 1,
                                value: line.as_str(),
                            }
                        }),
                    );
                if let Some(first_warning) = warnings.first() {
                    return Err(format!(
                        "Invalid keybind directive on line {}: {}",
                        first_warning.line_number, first_warning.message
                    ));
                }
                config::set_keybind_lines(&lines)?;
                self.config.keybind_lines = lines
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| termy_config_core::KeybindConfigLine {
                        line_number: index + 1,
                        value,
                    })
                    .collect();
                Ok(())
            }
            EditableField::WorkingDirectory => {
                if value.is_empty() {
                    self.config.working_dir = None;
                    config::set_root_setting(termy_config_core::RootSettingId::WorkingDir, "none")
                } else {
                    self.config.working_dir = Some(value.to_string());
                    config::set_root_setting(termy_config_core::RootSettingId::WorkingDir, value)
                }
            }
            EditableField::WorkingDirFallback => {
                let parsed = match value.to_ascii_lowercase().as_str() {
                    "home" | "user" => termy_config_core::WorkingDirFallback::Home,
                    "process" | "cwd" => termy_config_core::WorkingDirFallback::Process,
                    _ => return Err("Working dir fallback must be home or process".to_string()),
                };
                self.config.working_dir_fallback = parsed;
                let canonical = match parsed {
                    termy_config_core::WorkingDirFallback::Home => "home",
                    termy_config_core::WorkingDirFallback::Process => "process",
                };
                config::set_root_setting(termy_config_core::RootSettingId::WorkingDirFallback, canonical)
            }
            EditableField::WindowWidth => {
                let parsed = value
                    .parse::<f32>()
                    .map_err(|_| "Default width must be a positive number".to_string())?;
                if parsed <= 0.0 {
                    return Err("Default width must be greater than 0".to_string());
                }
                self.config.window_width = parsed;
                config::set_root_setting(termy_config_core::RootSettingId::WindowWidth, &parsed.to_string())
            }
            EditableField::WindowHeight => {
                let parsed = value
                    .parse::<f32>()
                    .map_err(|_| "Default height must be a positive number".to_string())?;
                if parsed <= 0.0 {
                    return Err("Default height must be greater than 0".to_string());
                }
                self.config.window_height = parsed;
                config::set_root_setting(termy_config_core::RootSettingId::WindowHeight, &parsed.to_string())
            }
            EditableField::Color(id) => {
                if value.is_empty() {
                    config::set_color_setting(id, None)?;
                    self.set_custom_color_for_id(id, None);
                } else {
                    let Some(parsed) = termy_config_core::Rgb8::from_hex(value) else {
                        return Err("Color must be #RRGGBB".to_string());
                    };
                    let canonical = format!("#{:02x}{:02x}{:02x}", parsed.r, parsed.g, parsed.b);
                    config::set_color_setting(id, Some(&canonical))?;
                    self.set_custom_color_for_id(id, Some(parsed));
                }
                self.colors = TerminalColors::from_theme(&self.config.theme, &self.config.colors);
                Ok(())
            }
        }
    }

    fn begin_editing_field(
        &mut self,
        field: EditableField,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.blur_sidebar_search();
        self.active_input = Some(ActiveTextInput::new(
            field,
            self.editable_field_value(field),
        ));
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn is_numeric_field(field: EditableField) -> bool {
        matches!(
            field,
            EditableField::BackgroundOpacity
                | EditableField::FontSize
                | EditableField::PaddingX
                | EditableField::PaddingY
                | EditableField::ScrollbackHistory
                | EditableField::InactiveTabScrollback
                | EditableField::ScrollMultiplier
                | EditableField::WindowWidth
                | EditableField::WindowHeight
        )
    }

    fn uses_text_input_for_field(field: EditableField) -> bool {
        !Self::is_numeric_field(field)
    }

    fn step_numeric_field(&mut self, field: EditableField, delta: i32, cx: &mut Context<Self>) {
        let result = match field {
            EditableField::BackgroundOpacity => {
                let next = (self.config.background_opacity + (delta as f32 * 0.05)).clamp(0.0, 1.0);
                self.config.background_opacity = next;
                config::set_root_setting(termy_config_core::RootSettingId::BackgroundOpacity, &format!("{:.3}", next))
            }
            EditableField::FontSize => {
                let next = (self.config.font_size + delta as f32).max(1.0);
                self.config.font_size = next;
                config::set_root_setting(termy_config_core::RootSettingId::FontSize, &next.to_string())
            }
            EditableField::PaddingX => {
                let next = (self.config.padding_x + delta as f32).max(0.0);
                self.config.padding_x = next;
                config::set_root_setting(termy_config_core::RootSettingId::PaddingX, &next.to_string())
            }
            EditableField::PaddingY => {
                let next = (self.config.padding_y + delta as f32).max(0.0);
                self.config.padding_y = next;
                config::set_root_setting(termy_config_core::RootSettingId::PaddingY, &next.to_string())
            }
            EditableField::ScrollbackHistory => {
                let next = (self.config.scrollback_history as i64 + (delta as i64 * 100))
                    .clamp(0, 100_000) as usize;
                self.config.scrollback_history = next;
                config::set_root_setting(termy_config_core::RootSettingId::ScrollbackHistory, &next.to_string())
            }
            EditableField::InactiveTabScrollback => {
                let current = self.config.inactive_tab_scrollback.unwrap_or(0);
                let next = (current as i64 + (delta as i64 * 100)).clamp(0, 100_000) as usize;
                self.config.inactive_tab_scrollback = Some(next);
                config::set_root_setting(termy_config_core::RootSettingId::InactiveTabScrollback, &next.to_string())
            }
            EditableField::ScrollMultiplier => {
                let next =
                    (self.config.mouse_scroll_multiplier + (delta as f32 * 0.1)).clamp(0.1, 1000.0);
                self.config.mouse_scroll_multiplier = next;
                config::set_root_setting(termy_config_core::RootSettingId::MouseScrollMultiplier, &next.to_string())
            }
            EditableField::WindowWidth => {
                let next = (self.config.window_width + (delta as f32 * 20.0)).max(1.0);
                self.config.window_width = next;
                config::set_root_setting(termy_config_core::RootSettingId::WindowWidth, &next.to_string())
            }
            EditableField::WindowHeight => {
                let next = (self.config.window_height + (delta as f32 * 20.0)).max(1.0);
                self.config.window_height = next;
                config::set_root_setting(termy_config_core::RootSettingId::WindowHeight, &next.to_string())
            }
            _ => Ok(()),
        };

        if let Err(error) = result {
            termy_toast::error(error);
        }
        self.active_input = None;
        cx.notify();
    }

    fn ordered_theme_ids_for_settings(&self) -> Vec<String> {
        let mut theme_ids: Vec<String> = termy_themes::available_theme_ids()
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();
        theme_ids.push("shell-decide".to_string());

        if !theme_ids.iter().any(|theme| theme == &self.config.theme) {
            theme_ids.push(self.config.theme.clone());
        }

        theme_ids.sort_unstable();
        theme_ids.dedup();
        theme_ids
    }

    fn ordered_font_families_for_settings(&self) -> Vec<String> {
        let mut fonts = self.available_font_families.clone();
        if !fonts
            .iter()
            .any(|font| font.eq_ignore_ascii_case(&self.config.font_family))
        {
            fonts.push(self.config.font_family.clone());
        }
        fonts.sort_unstable_by_key(|font| font.to_ascii_lowercase());
        fonts.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
        fonts
    }

    fn filtered_theme_suggestions(&self, query: &str) -> Vec<String> {
        let normalized = query.trim().to_ascii_lowercase();
        let themes = self.ordered_theme_ids_for_settings();

        if normalized.is_empty() {
            return themes.into_iter().take(16).collect();
        }

        let mut matched = Vec::new();
        let mut rest = Vec::new();
        for theme in themes {
            let lower = theme.to_ascii_lowercase();
            if lower.contains(&normalized) || lower.replace('-', " ").contains(&normalized) {
                matched.push(theme);
            } else {
                rest.push(theme);
            }
        }
        matched.extend(rest);
        matched.into_iter().take(16).collect()
    }

    fn filtered_font_suggestions(&self, query: &str) -> Vec<String> {
        let normalized = query.trim().to_ascii_lowercase();
        let fonts = self.ordered_font_families_for_settings();
        let selected_font = self.config.font_family.trim().to_ascii_lowercase();

        // When the dropdown first opens, the input text equals the selected font.
        // Treat that like an empty query so users can browse the full installed list.
        if normalized.is_empty() || normalized == selected_font {
            return fonts;
        }

        fonts
            .into_iter()
            .filter(|font| font.to_ascii_lowercase().contains(&normalized))
            .collect()
    }

    fn commit_active_input(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.active_input.take() else {
            return;
        };

        if let Err(error) = self.apply_editable_field(input.field, input.state.text()) {
            termy_toast::error(error);
            self.active_input = Some(input);
        }
        cx.notify();
    }

    fn cancel_active_input(&mut self, cx: &mut Context<Self>) {
        self.active_input = None;
        cx.notify();
    }

    fn render_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w_full()
            .child(match self.active_section {
                SettingsSection::Appearance => {
                    self.render_appearance_section(cx).into_any_element()
                }
                SettingsSection::Terminal => self.render_terminal_section(cx).into_any_element(),
                SettingsSection::Tabs => self.render_tabs_section(cx).into_any_element(),
                SettingsSection::Advanced => self.render_advanced_section(cx).into_any_element(),
                SettingsSection::Colors => self.render_colors_section(cx).into_any_element(),
                SettingsSection::Keybindings => self.render_keybindings_section(cx).into_any_element(),
            })
            .into_any_element()
    }

    fn render_section_header(
        &self,
        title: &'static str,
        subtitle: &'static str,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_1()
            .mb_6()
            .child(
                div()
                    .text_xl()
                    .font_weight(gpui::FontWeight::BOLD)
                    .text_color(self.text_primary())
                    .child(title),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(self.text_muted())
                    .child(subtitle),
            )
    }

    fn render_group_header(&self, title: &'static str) -> impl IntoElement {
        div()
            .text_xs()
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(self.text_muted())
            .mt_4()
            .mb_2()
            .child(title)
    }

    fn render_setting_row(
        &self,
        search_key: &'static str,
        id: &'static str,
        title: &'static str,
        description: &'static str,
        checked: bool,
        cx: &mut Context<Self>,
        on_toggle: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let row = div()
            .flex()
            .items_center()
            .justify_between()
            .py_3()
            .px_4()
            .rounded(px(0.0))
            .bg(self.bg_card())
            .border_1()
            .border_color(self.border_color())
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(self.text_primary())
                            .truncate()
                            .child(title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(self.text_muted())
                            .truncate()
                            .child(description),
                    ),
            )
            .child(self.render_switch(id, checked, cx, on_toggle));

        self.wrap_setting_with_scroll_anchor(search_key, row.into_any_element())
    }

    fn render_switch(
        &self,
        id: &'static str,
        checked: bool,
        cx: &mut Context<Self>,
        on_toggle: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        let accent = self.accent();
        // Off state: use a more visible muted foreground color
        let mut bg_off = self.colors.foreground;
        bg_off.a = 0.25;
        let track_color = if checked { accent } else { bg_off };
        let knob_color = self.contrasting_text_for_fill(track_color, self.bg_card());

        div()
            .id(SharedString::from(id))
            .w(px(44.0))
            .h(px(24.0))
            .rounded(px(0.0))
            .bg(track_color)
            .cursor_pointer()
            .relative()
            .child(
                div()
                    .absolute()
                    .top(px(2.0))
                    .left(if checked { px(22.0) } else { px(2.0) })
                    .w(px(20.0))
                    .h(px(20.0))
                    .rounded(px(0.0))
                    .bg(knob_color)
                    .shadow_sm(),
            )
            .on_click(cx.listener(move |view, _, _, cx| {
                on_toggle(view, cx);
                cx.notify();
            }))
    }

    fn render_editable_row(
        &mut self,
        search_key: &'static str,
        field: EditableField,
        title: &'static str,
        description: &'static str,
        display_value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_numeric = Self::is_numeric_field(field);
        let is_active = self
            .active_input
            .as_ref()
            .is_some_and(|input| input.field == field);
        let uses_dropdown = Self::field_uses_dropdown(field);
        let accent_inner_border = is_numeric || uses_dropdown;
        let dropdown_options = if uses_dropdown && is_active {
            let query = self
                .active_input
                .as_ref()
                .map(|input| input.state.text())
                .unwrap_or("");
            self.dropdown_options_for_field(field, query)
        } else {
            Vec::new()
        };

        // Cache colors for closures
        let text_secondary = self.text_secondary();
        let hover_bg = self.bg_hover();
        let input_bg = self.bg_input();
        let border_color = self.border_color();
        let accent = self.accent();
        let bg_card = self.bg_card();
        let text_primary = self.text_primary();
        let text_muted = self.text_muted();

        let mut dropdown = None;
        let dropdown_open = is_active && uses_dropdown && !dropdown_options.is_empty();
        if dropdown_open {
            let mut list = div().flex().flex_col().py_1();
            for (index, option) in dropdown_options.into_iter().enumerate() {
                let option_label = option.display_text();
                let option_value = option.value.clone();
                let should_preview_font = field == EditableField::FontFamily;
                list = list.child(
                    div()
                        .id(SharedString::from(format!(
                            "dropdown-option-{field:?}-{index}"
                        )))
                        .px_3()
                        .py_1()
                        .text_sm()
                        .text_color(text_secondary)
                        .cursor_pointer()
                        .when(should_preview_font, |s| s.font_family(option_value.clone()))
                        .hover(|this| this.bg(hover_bg))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |view, _event: &MouseDownEvent, _window, cx| {
                                cx.stop_propagation();
                                view.apply_dropdown_selection(field, &option_value, cx);
                            }),
                        )
                        .child(option_label),
                );
            }

            // Use a fully opaque background for the dropdown so it covers content below
            let dropdown_bg = self.bg_primary();
            dropdown = Some(
                deferred(
                    div()
                        .id(SharedString::from(format!("dropdown-suggestions-{field:?}")))
                        .occlude()
                        .absolute()
                        .top(px(34.0))
                        .left_0()
                        .right_0()
                        .max_h(if field == EditableField::Theme {
                            px(180.0)
                        } else {
                            px(240.0)
                        })
                        .overflow_scroll()
                        .overflow_x_hidden()
                        .rounded(px(0.0))
                        .bg(dropdown_bg)
                        .border_1()
                        .border_color(border_color)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|_view, _event: &MouseDownEvent, _window, cx| {
                                cx.stop_propagation();
                            }),
                        )
                        .on_scroll_wheel(cx.listener(
                            |_view, _event: &ScrollWheelEvent, _window, cx| {
                                cx.stop_propagation();
                            },
                        ))
                        .child(list),
                )
                .with_priority(10)
                .into_any_element(),
            );
        }

        let readonly_display_value = if !is_active && uses_dropdown {
            self.dropdown_display_value(field, &display_value)
        } else {
            display_value.clone()
        };

        let value_element = if is_numeric {
            div()
                .h_full()
                .flex()
                .items_center()
                .justify_between()
                .gap_1()
                .child(
                    div()
                        .id(SharedString::from(format!("dec-{field:?}")))
                        .w(px(NUMERIC_STEP_BUTTON_SIZE))
                        .h(px(NUMERIC_STEP_BUTTON_SIZE))
                        .rounded(px(0.0))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(bg_card)
                        .text_color(text_primary)
                        .text_sm()
                        .child("-")
                        .on_click(cx.listener(move |view, _, _, cx| {
                            cx.stop_propagation();
                            view.step_numeric_field(field, -1, cx);
                        })),
                )
                .child(
                    div()
                        .flex_1()
                        .text_sm()
                        .text_color(text_secondary)
                        .text_align(TextAlign::Center)
                        .child(display_value),
                )
                .child(
                    div()
                        .id(SharedString::from(format!("inc-{field:?}")))
                        .w(px(NUMERIC_STEP_BUTTON_SIZE))
                        .h(px(NUMERIC_STEP_BUTTON_SIZE))
                        .rounded(px(0.0))
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(bg_card)
                        .text_color(text_primary)
                        .text_sm()
                        .child("+")
                        .on_click(cx.listener(move |view, _, _, cx| {
                            cx.stop_propagation();
                            view.step_numeric_field(field, 1, cx);
                        })),
                )
                .into_any_element()
        } else if is_active {
            let font = Font {
                family: self.config.font_family.clone().into(),
                ..Font::default()
            };
            let selection_color = self.accent_with_alpha(0.3);
            TextInputElement::new(
                cx.entity(),
                self.focus_handle.clone(),
                font,
                px(13.0),
                text_secondary.into(),
                selection_color.into(),
                TextInputAlignment::Left,
            )
            .into_any_element()
        } else {
            div()
                .text_sm()
                .text_color(text_secondary)
                .child(readonly_display_value)
                .into_any_element()
        };

        let row = div()
            .id(SharedString::from(format!("editable-row-{field:?}")))
            .flex()
            .items_start()
            .gap_4()
            .py_3()
            .px_4()
            .rounded(px(0.0))
            .bg(bg_card)
            .border_1()
            .border_color(if dropdown_open {
                Rgba {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                }
            } else {
                border_color
            })
            .cursor_pointer()
            .when(!is_numeric, |s| {
                s.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        if !view
                            .active_input
                            .as_ref()
                            .is_some_and(|input| input.field == field)
                        {
                            view.begin_editing_field(field, window, cx);
                        }

                        if let Some(input) = view.active_input.as_mut() {
                            let index = input.state.character_index_for_point(event.position);
                            if event.modifiers.shift {
                                input.state.select_to_utf16(index);
                            } else {
                                input.state.set_cursor_utf16(index);
                            }
                            input.selecting = true;
                        }

                        view.focus_handle.focus(window, cx);
                        cx.notify();
                    }),
                )
                .on_mouse_move(
                    cx.listener(move |view, event: &MouseMoveEvent, _window, cx| {
                        let Some(input) = view.active_input.as_mut() else {
                            return;
                        };
                        if input.field != field || !input.selecting || !event.dragging() {
                            return;
                        }
                        let index = input.state.character_index_for_point(event.position);
                        input.state.select_to_utf16(index);
                        cx.notify();
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |view, _event: &MouseUpEvent, _window, cx| {
                        if let Some(input) = view.active_input.as_mut()
                            && input.field == field
                        {
                            input.selecting = false;
                            cx.notify();
                        }
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(move |view, _event: &MouseUpEvent, _window, cx| {
                        if let Some(input) = view.active_input.as_mut()
                            && input.field == field
                        {
                            input.selecting = false;
                            cx.notify();
                        }
                    }),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(text_primary)
                            .truncate()
                            .child(title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(text_muted)
                            .truncate()
                            .child(description),
                    ),
            )
            .child(
                div()
                    .when(is_numeric, |s| s.w(px(NUMERIC_INPUT_WIDTH)).flex_none())
                    .when(!is_numeric, |s| {
                        s.flex_1().min_w(px(220.0)).max_w(px(560.0))
                    })
                    .relative()
                    .h(if is_numeric {
                        px(NUMERIC_INPUT_HEIGHT)
                    } else {
                        px(28.0)
                    })
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .h_full()
                            .px_2()
                            .rounded(px(0.0))
                            .bg(input_bg)
                            .border_1()
                            .border_color(if is_active && accent_inner_border {
                                accent.into()
                            } else {
                                border_color
                            })
                            .overflow_hidden()
                            .child(value_element),
                    )
                    .when_some(dropdown, |s, dropdown| s.child(dropdown)),
            );

        self.wrap_setting_with_scroll_anchor(search_key, row.into_any_element())
    }

    fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.modifiers.secondary()
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.control
            && !event.keystroke.modifiers.function
            && event.keystroke.key.eq_ignore_ascii_case("w")
        {
            window.remove_window();
            return;
        }

        let cmd_only = event.keystroke.modifiers.secondary()
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.function;

        if self.active_input.is_none() && !self.sidebar_search_active {
            return;
        }

        if self.sidebar_search_active && self.active_input.is_none() {
            if cmd_only && event.keystroke.key.eq_ignore_ascii_case("a") {
                self.sidebar_search_state.select_all();
                cx.notify();
                return;
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
            return;
        }

        let active_field = self.active_input.as_ref().map(|input| input.field);
        let active_input_query = self
            .active_input
            .as_ref()
            .map(|input| input.state.text().to_string())
            .unwrap_or_default();
        let allow_text_editing = active_field.is_some_and(Self::uses_text_input_for_field);

        if cmd_only
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

    fn render_appearance_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let background_blur = self.config.background_blur;
        let background_opacity = self.config.background_opacity;
        let theme = self.config.theme.clone();
        let font_family = self.config.font_family.clone();
        let font_size = self.config.font_size;
        let padding_x = self.config.padding_x;
        let padding_y = self.config.padding_y;
        let theme_meta = Self::setting_metadata("theme").expect("missing metadata for theme");
        let blur_meta = Self::setting_metadata("background_blur")
            .expect("missing metadata for background_blur");
        let opacity_meta = Self::setting_metadata("background_opacity")
            .expect("missing metadata for background_opacity");
        let font_family_meta =
            Self::setting_metadata("font_family").expect("missing metadata for font_family");
        let font_size_meta =
            Self::setting_metadata("font_size").expect("missing metadata for font_size");
        let padding_x_meta =
            Self::setting_metadata("padding_x").expect("missing metadata for padding_x");
        let padding_y_meta =
            Self::setting_metadata("padding_y").expect("missing metadata for padding_y");

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.render_section_header("Appearance", "Customize the look and feel"))
            .child(self.render_group_header("THEME"))
            .child(self.render_editable_row(
                "theme",
                EditableField::Theme,
                theme_meta.title,
                theme_meta.description,
                theme,
                cx,
            ))
            .child(self.render_group_header("WINDOW"))
            .child(self.render_setting_row(
                "background_blur",
                "blur-toggle",
                blur_meta.title,
                blur_meta.description,
                background_blur,
                cx,
                |view, _cx| {
                    view.config.background_blur = !view.config.background_blur;
                    let _ = config::set_root_setting(
                        termy_config_core::RootSettingId::BackgroundBlur,
                        &view.config.background_blur.to_string(),
                    );
                },
            ))
            .child(self.render_editable_row(
                "background_opacity",
                EditableField::BackgroundOpacity,
                opacity_meta.title,
                opacity_meta.description,
                format!("{}%", (background_opacity * 100.0) as i32),
                cx,
            ))
            .child(self.render_group_header("FONT"))
            .child(self.render_editable_row(
                "font_family",
                EditableField::FontFamily,
                font_family_meta.title,
                font_family_meta.description,
                font_family,
                cx,
            ))
            .child(self.render_editable_row(
                "font_size",
                EditableField::FontSize,
                font_size_meta.title,
                font_size_meta.description,
                format!("{}px", font_size as i32),
                cx,
            ))
            .child(self.render_group_header("PADDING"))
            .child(self.render_editable_row(
                "padding_x",
                EditableField::PaddingX,
                padding_x_meta.title,
                padding_x_meta.description,
                format!("{}px", padding_x as i32),
                cx,
            ))
            .child(self.render_editable_row(
                "padding_y",
                EditableField::PaddingY,
                padding_y_meta.title,
                padding_y_meta.description,
                format!("{}px", padding_y as i32),
                cx,
            ))
    }

    fn render_terminal_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let cursor_blink = self.config.cursor_blink;
        let term = self.config.term.clone();
        let shell = self
            .config
            .shell
            .clone()
            .unwrap_or_else(|| "System default".to_string());
        let colorterm = self
            .config
            .colorterm
            .clone()
            .unwrap_or_else(|| "Disabled".to_string());
        let scrollback = self.config.scrollback_history;
        let inactive_scrollback = self.config.inactive_tab_scrollback.unwrap_or(0);
        let scroll_mult = self.config.mouse_scroll_multiplier;
        let command_palette_show_keybinds = self.config.command_palette_show_keybinds;
        let cursor_blink_meta =
            Self::setting_metadata("cursor_blink").expect("missing metadata for cursor_blink");
        let shell_meta = Self::setting_metadata("shell").expect("missing metadata for shell");
        let term_meta = Self::setting_metadata("term").expect("missing metadata for term");
        let colorterm_meta =
            Self::setting_metadata("colorterm").expect("missing metadata for colorterm");
        let scrollback_meta = Self::setting_metadata("scrollback_history")
            .expect("missing metadata for scrollback_history");
        let scroll_mult_meta = Self::setting_metadata("mouse_scroll_multiplier")
            .expect("missing metadata for mouse_scroll_multiplier");
        let inactive_scrollback_meta = Self::setting_metadata("inactive_tab_scrollback")
            .expect("missing metadata for inactive_tab_scrollback");
        let cursor_style_meta =
            Self::setting_metadata("cursor_style").expect("missing metadata for cursor_style");
        let scrollbar_visibility_meta = Self::setting_metadata("scrollbar_visibility")
            .expect("missing metadata for scrollbar_visibility");
        let scrollbar_style_meta = Self::setting_metadata("scrollbar_style")
            .expect("missing metadata for scrollbar_style");
        let palette_meta = Self::setting_metadata("command_palette_show_keybinds")
            .expect("missing metadata for command_palette_show_keybinds");

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.render_section_header("Terminal", "Configure terminal behavior"))
            .child(self.render_group_header("CURSOR"))
            .child(self.render_setting_row(
                "cursor_blink",
                "cursor_blink-toggle",
                cursor_blink_meta.title,
                cursor_blink_meta.description,
                cursor_blink,
                cx,
                |view, _cx| {
                    view.config.cursor_blink = !view.config.cursor_blink;
                    let _ = config::set_root_setting(termy_config_core::RootSettingId::CursorBlink, &view.config.cursor_blink.to_string());
                },
            ))
            .child(self.render_editable_row(
                "cursor_style",
                EditableField::CursorStyle,
                cursor_style_meta.title,
                cursor_style_meta.description,
                self.editable_field_value(EditableField::CursorStyle),
                cx,
            ))
            .child(self.render_group_header("SHELL"))
            .child(self.render_editable_row(
                "shell",
                EditableField::Shell,
                shell_meta.title,
                shell_meta.description,
                shell,
                cx,
            ))
            .child(self.render_editable_row(
                "term",
                EditableField::Term,
                term_meta.title,
                term_meta.description,
                term,
                cx,
            ))
            .child(self.render_editable_row(
                "colorterm",
                EditableField::Colorterm,
                colorterm_meta.title,
                colorterm_meta.description,
                colorterm,
                cx,
            ))
            .child(self.render_group_header("SCROLLING"))
            .child(self.render_editable_row(
                "scrollback_history",
                EditableField::ScrollbackHistory,
                scrollback_meta.title,
                scrollback_meta.description,
                format!("{} lines", scrollback),
                cx,
            ))
            .child(self.render_editable_row(
                "inactive_tab_scrollback",
                EditableField::InactiveTabScrollback,
                inactive_scrollback_meta.title,
                inactive_scrollback_meta.description,
                format!("{} lines", inactive_scrollback),
                cx,
            ))
            .child(self.render_editable_row(
                "mouse_scroll_multiplier",
                EditableField::ScrollMultiplier,
                scroll_mult_meta.title,
                scroll_mult_meta.description,
                format!("{}x", scroll_mult),
                cx,
            ))
            .child(self.render_editable_row(
                "scrollbar_visibility",
                EditableField::ScrollbarVisibility,
                scrollbar_visibility_meta.title,
                scrollbar_visibility_meta.description,
                self.editable_field_value(EditableField::ScrollbarVisibility),
                cx,
            ))
            .child(self.render_editable_row(
                "scrollbar_style",
                EditableField::ScrollbarStyle,
                scrollbar_style_meta.title,
                scrollbar_style_meta.description,
                self.editable_field_value(EditableField::ScrollbarStyle),
                cx,
            ))
            .child(self.render_group_header("UI"))
            .child(self.render_setting_row(
                "command_palette_show_keybinds",
                "command_palette_show_keybinds-toggle",
                palette_meta.title,
                palette_meta.description,
                command_palette_show_keybinds,
                cx,
                |view, _cx| {
                    view.config.command_palette_show_keybinds =
                        !view.config.command_palette_show_keybinds;
                    let _ = config::set_root_setting(
                        termy_config_core::RootSettingId::CommandPaletteShowKeybinds,
                        &view.config.command_palette_show_keybinds.to_string(),
                    );
                },
            ))
    }

    fn render_tabs_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let shell_integration = self.config.tab_title.shell_integration;
        let show_termy = self.config.show_termy_in_titlebar;
        let fallback = self.config.tab_title.fallback.clone();
        let title_priority = self.editable_field_value(EditableField::TabTitlePriority);
        let explicit_prefix = self.config.tab_title.explicit_prefix.clone();
        let prompt_format = self.config.tab_title.prompt_format.clone();
        let command_format = self.config.tab_title.command_format.clone();
        let close_visibility = self.editable_field_value(EditableField::TabCloseVisibility);
        let width_mode = self.editable_field_value(EditableField::TabWidthMode);
        let shell_integration_meta = Self::setting_metadata("tab_title_shell_integration")
            .expect("missing metadata for tab_title_shell_integration");
        let title_mode_meta =
            Self::setting_metadata("tab_title_mode").expect("missing metadata for tab_title_mode");
        let fallback_meta =
            Self::setting_metadata("tab_title_fallback").expect("missing metadata for tab_title_fallback");
        let title_priority_meta = Self::setting_metadata("tab_title_priority")
            .expect("missing metadata for tab_title_priority");
        let explicit_prefix_meta = Self::setting_metadata("tab_title_explicit_prefix")
            .expect("missing metadata for tab_title_explicit_prefix");
        let prompt_format_meta = Self::setting_metadata("tab_title_prompt_format")
            .expect("missing metadata for tab_title_prompt_format");
        let command_format_meta = Self::setting_metadata("tab_title_command_format")
            .expect("missing metadata for tab_title_command_format");
        let close_visibility_meta = Self::setting_metadata("tab_close_visibility")
            .expect("missing metadata for tab_close_visibility");
        let width_mode_meta = Self::setting_metadata("tab_width_mode")
            .expect("missing metadata for tab_width_mode");
        let show_termy_meta = Self::setting_metadata("show_termy_in_titlebar")
            .expect("missing metadata for show_termy_in_titlebar");

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.render_section_header("Tabs", "Configure tab behavior and titles"))
            .child(self.render_group_header("TAB TITLES"))
            .child(self.render_editable_row(
                "tab_title_mode",
                EditableField::TabTitleMode,
                title_mode_meta.title,
                title_mode_meta.description,
                self.editable_field_value(EditableField::TabTitleMode),
                cx,
            ))
            .child(self.render_setting_row(
                "tab_title_shell_integration",
                "tab_title_shell_integration-toggle",
                shell_integration_meta.title,
                shell_integration_meta.description,
                shell_integration,
                cx,
                |view, _cx| {
                    view.config.tab_title.shell_integration =
                        !view.config.tab_title.shell_integration;
                    let _ = config::set_root_setting(
                        termy_config_core::RootSettingId::TabTitleShellIntegration,
                        &view.config.tab_title.shell_integration.to_string(),
                    );
                },
            ))
            .child(self.render_editable_row(
                "tab_title_fallback",
                EditableField::TabFallbackTitle,
                fallback_meta.title,
                fallback_meta.description,
                fallback,
                cx,
            ))
            .child(self.render_editable_row(
                "tab_title_priority",
                EditableField::TabTitlePriority,
                title_priority_meta.title,
                title_priority_meta.description,
                title_priority,
                cx,
            ))
            .child(self.render_editable_row(
                "tab_title_explicit_prefix",
                EditableField::TabTitleExplicitPrefix,
                explicit_prefix_meta.title,
                explicit_prefix_meta.description,
                explicit_prefix,
                cx,
            ))
            .child(self.render_editable_row(
                "tab_title_prompt_format",
                EditableField::TabTitlePromptFormat,
                prompt_format_meta.title,
                prompt_format_meta.description,
                prompt_format,
                cx,
            ))
            .child(self.render_editable_row(
                "tab_title_command_format",
                EditableField::TabTitleCommandFormat,
                command_format_meta.title,
                command_format_meta.description,
                command_format,
                cx,
            ))
            .child(self.render_group_header("TAB STRIP"))
            .child(self.render_editable_row(
                "tab_close_visibility",
                EditableField::TabCloseVisibility,
                close_visibility_meta.title,
                close_visibility_meta.description,
                close_visibility,
                cx,
            ))
            .child(self.render_editable_row(
                "tab_width_mode",
                EditableField::TabWidthMode,
                width_mode_meta.title,
                width_mode_meta.description,
                width_mode,
                cx,
            ))
            .child(self.render_group_header("TITLE BAR"))
            .child(self.render_setting_row(
                "show_termy_in_titlebar",
                "show_termy_in_titlebar-toggle",
                show_termy_meta.title,
                show_termy_meta.description,
                show_termy,
                cx,
                |view, _cx| {
                    view.config.show_termy_in_titlebar = !view.config.show_termy_in_titlebar;
                    let _ = config::set_root_setting(
                        termy_config_core::RootSettingId::ShowTermyInTitlebar,
                        &view.config.show_termy_in_titlebar.to_string(),
                    );
                },
            ))
    }

    fn render_advanced_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let working_dir = self
            .config
            .working_dir
            .clone()
            .unwrap_or_else(|| "Not set".to_string());
        let working_dir_fallback = self.editable_field_value(EditableField::WorkingDirFallback);
        let warn_on_quit = self.config.warn_on_quit_with_running_process;
        let window_width = self.config.window_width;
        let window_height = self.config.window_height;
        let bg_card = self.bg_card();
        let border_color = self.border_color();
        let text_muted = self.text_muted();
        let text_secondary = self.text_secondary();
        let accent = self.accent();
        let accent_hover = self.accent_with_alpha(0.8);
        let button_text = self.contrasting_text_for_fill(accent, bg_card);
        let button_hover_text = self.contrasting_text_for_fill(accent_hover, bg_card);
        let working_dir_meta = Self::setting_metadata("working_dir")
            .expect("missing metadata for working_dir");
        let working_dir_fallback_meta = Self::setting_metadata("working_dir_fallback")
            .expect("missing metadata for working_dir_fallback");
        let warn_on_quit_meta = Self::setting_metadata("warn_on_quit_with_running_process")
            .expect("missing metadata for warn_on_quit_with_running_process");
        let window_width_meta =
            Self::setting_metadata("window_width").expect("missing metadata for window_width");
        let window_height_meta =
            Self::setting_metadata("window_height").expect("missing metadata for window_height");

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.render_section_header("Advanced", "Advanced configuration options"))
            .child(self.render_group_header("STARTUP"))
            .child(self.render_editable_row(
                "working_dir",
                EditableField::WorkingDirectory,
                working_dir_meta.title,
                working_dir_meta.description,
                working_dir,
                cx,
            ))
            .child(self.render_editable_row(
                "working_dir_fallback",
                EditableField::WorkingDirFallback,
                working_dir_fallback_meta.title,
                working_dir_fallback_meta.description,
                working_dir_fallback,
                cx,
            ))
            .child(self.render_group_header("SAFETY"))
            .child(self.render_setting_row(
                "warn_on_quit_with_running_process",
                "warn_on_quit-toggle",
                warn_on_quit_meta.title,
                warn_on_quit_meta.description,
                warn_on_quit,
                cx,
                |view, _cx| {
                    view.config.warn_on_quit_with_running_process =
                        !view.config.warn_on_quit_with_running_process;
                    let _ = config::set_root_setting(
                        termy_config_core::RootSettingId::WarnOnQuitWithRunningProcess,
                        &view.config.warn_on_quit_with_running_process.to_string(),
                    );
                },
            ))
            .child(self.render_group_header("WINDOW"))
            .child(self.render_editable_row(
                "window_width",
                EditableField::WindowWidth,
                window_width_meta.title,
                window_width_meta.description,
                format!("{}px", window_width as i32),
                cx,
            ))
            .child(self.render_editable_row(
                "window_height",
                EditableField::WindowHeight,
                window_height_meta.title,
                window_height_meta.description,
                format!("{}px", window_height as i32),
                cx,
            ))
            .child(self.render_group_header("CONFIG FILE"))
            .child(
                div()
                    .py_4()
                    .px_4()
                    .rounded(px(0.0))
                    .bg(bg_card)
                    .border_1()
                    .border_color(border_color)
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_sm()
                            .text_color(text_muted)
                            .child("To change these settings, edit the config file:"),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_family("monospace")
                            .text_color(text_secondary)
                            .child("~/.config/termy/config.txt"),
                    )
                    .child(
                        div()
                            .id("open-config-btn")
                            .mt_2()
                            .px_4()
                            .py_2()
                            .rounded(px(0.0))
                            .bg(accent)
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(button_text)
                            .cursor_pointer()
                            .hover(move |s| s.bg(accent_hover).text_color(button_hover_text))
                            .child("Open Config File")
                            .on_click(cx.listener(|_view, _, _, cx| {
                                if let Err(error) = crate::config::open_config_file() {
                                    log::error!(
                                        "Failed to open config file from settings: {}",
                                        error
                                    );
                                    termy_toast::error(error.to_string());
                                }
                                cx.notify();
                            })),
                    ),
            )
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
        let settings_scrollbar = self.settings_scrollbar_metrics(window).map(|metrics| {
            div()
                .w(px(SETTINGS_SCROLLBAR_WIDTH + 4.0))
                .h_full()
                .pl(px(2.0))
                .pr(px(2.0))
                .child(ui_scrollbar::render_vertical(
                    "settings-content-scrollbar",
                    metrics,
                    self.settings_scrollbar_style(),
                    false,
                    &[],
                    None,
                    0.0,
                ))
        });
        div()
            .id("settings-root")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            .on_any_mouse_down(cx.listener(|view, _event: &MouseDownEvent, _window, cx| {
                if view.active_input.is_some() || view.sidebar_search_active {
                    view.active_input = None;
                    view.blur_sidebar_search();
                    cx.notify();
                }
            }))
            .flex()
            .size_full()
            .bg(bg)
            .font_family(self.config.font_family.clone())
            .child(self.render_sidebar(cx))
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_start()
                    .child(
                        div()
                            .id("settings-content-scroll")
                            .flex_1()
                            .h_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.content_scroll_handle)
                            .overflow_x_hidden()
                            .p_6()
                            .child(self.render_content(cx)),
                    )
                    .when_some(settings_scrollbar, |s, scrollbar| s.child(scrollbar)),
            )
    }
}
