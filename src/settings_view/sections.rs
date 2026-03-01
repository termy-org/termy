use super::*;
use super::search::SettingMetadata;
use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

const FALLBACK_SETTING_METADATA: SettingMetadata = SettingMetadata {
    key: "__missing_setting_metadata__",
    section: SettingsSection::Advanced,
    title: "Unknown setting",
    description: "Metadata unavailable for this setting.",
    keywords: &[],
};

static LOGGED_MISSING_METADATA_KEYS: LazyLock<Mutex<HashSet<&'static str>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

impl SettingsWindow {
    fn setting_metadata_or_fallback(key: &'static str) -> &'static SettingMetadata {
        if let Some(metadata) = Self::setting_metadata(key) {
            return metadata;
        }

        let mut logged = LOGGED_MISSING_METADATA_KEYS
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if logged.insert(key) {
            log::error!("Missing settings metadata for key '{}'", key);
        }

        &FALLBACK_SETTING_METADATA
    }

    pub(super) fn render_content(&mut self, cx: &mut Context<Self>) -> AnyElement {
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

    pub(super) fn render_appearance_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let background_blur = self.config.background_blur;
        let theme = self.config.theme.clone();
        let font_family = self.config.font_family.clone();
        let font_size = self.config.font_size;
        let padding_x = self.config.padding_x;
        let padding_y = self.config.padding_y;
        let theme_meta = Self::setting_metadata_or_fallback("theme");
        let blur_meta = Self::setting_metadata_or_fallback("background_blur");
        let opacity_meta = Self::setting_metadata_or_fallback("background_opacity");
        let font_family_meta = Self::setting_metadata_or_fallback("font_family");
        let font_size_meta = Self::setting_metadata_or_fallback("font_size");
        let padding_x_meta = Self::setting_metadata_or_fallback("padding_x");
        let padding_y_meta = Self::setting_metadata_or_fallback("padding_y");

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.render_section_header(
                "Appearance",
                "Customize the look and feel",
                SettingsSection::Appearance,
                cx,
            ))
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
                    let next = !view.config.background_blur;
                    match config::set_root_setting(
                        RootSettingId::BackgroundBlur,
                        &next.to_string(),
                    ) {
                        Ok(()) => {
                            view.config.background_blur = next;
                            termy_toast::success("Saved");
                        }
                        Err(error) => termy_toast::error(error),
                    }
                },
            ))
            .child(self.render_background_opacity_row(
                "background_opacity",
                opacity_meta.title,
                opacity_meta.description,
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

    pub(super) fn render_terminal_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.render_section_header(
                "Terminal",
                "Configure terminal behavior",
                SettingsSection::Terminal,
                cx,
            ))
            .child(self.render_terminal_cursor_group(cx))
            .child(self.render_terminal_shell_group(cx))
            .child(self.render_terminal_tmux_group(cx))
            .child(self.render_terminal_scrolling_group(cx))
            .child(self.render_terminal_ui_group(cx))
    }

    pub(super) fn render_terminal_cursor_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let cursor_blink_meta =
            Self::setting_metadata_or_fallback("cursor_blink");
        let cursor_style_meta =
            Self::setting_metadata_or_fallback("cursor_style");
        let cursor_blink = self.config.cursor_blink;

        div()
            .child(self.render_group_header("CURSOR"))
            .child(self.render_setting_row(
                "cursor_blink",
                "cursor_blink-toggle",
                cursor_blink_meta.title,
                cursor_blink_meta.description,
                cursor_blink,
                cx,
                |view, _cx| {
                    let next = !view.config.cursor_blink;
                    match config::set_root_setting(RootSettingId::CursorBlink, &next.to_string()) {
                        Ok(()) => {
                            view.config.cursor_blink = next;
                            termy_toast::success("Saved");
                        }
                        Err(error) => termy_toast::error(error),
                    }
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
            .into_any_element()
    }

    pub(super) fn render_terminal_shell_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let shell_meta = Self::setting_metadata_or_fallback("shell");
        let term_meta = Self::setting_metadata_or_fallback("term");
        let colorterm_meta =
            Self::setting_metadata_or_fallback("colorterm");
        let shell = self
            .config
            .shell
            .clone()
            .unwrap_or_else(|| "System default".to_string());
        let term = self.config.term.clone();
        let colorterm = self
            .config
            .colorterm
            .clone()
            .unwrap_or_else(|| "Disabled".to_string());

        div()
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
            .into_any_element()
    }

    pub(super) fn render_terminal_tmux_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let enabled_meta = Self::setting_metadata_or_fallback("tmux_enabled");
        let persistence_meta = Self::setting_metadata_or_fallback("tmux_persistence");
        let show_active_border_meta =
            Self::setting_metadata_or_fallback("tmux_show_active_pane_border");
        let binary_meta = Self::setting_metadata_or_fallback("tmux_binary");
        let tmux_enabled = self.config.tmux_enabled;
        let tmux_persistence = self.config.tmux_persistence;
        let tmux_show_active_pane_border = self.config.tmux_show_active_pane_border;
        let binary = self.config.tmux_binary.clone();

        let mut group = div()
            .child(self.render_group_header("TMUX"))
            .child(self.render_setting_row(
                "tmux_enabled",
                "tmux_enabled-toggle",
                enabled_meta.title,
                enabled_meta.description,
                tmux_enabled,
                cx,
                |view, _cx| {
                    let next = !view.config.tmux_enabled;
                    match config::set_root_setting(RootSettingId::TmuxEnabled, &next.to_string()) {
                        Ok(()) => {
                            view.config.tmux_enabled = next;
                            termy_toast::success(
                                "Saved. Use Attach/Detach tmux Session commands to switch runtime now.",
                            );
                        }
                        Err(error) => termy_toast::error(error),
                    }
                },
            ));

        if !tmux_enabled {
            return group.into_any_element();
        }

        group = group
            .child(self.render_setting_row(
                "tmux_persistence",
                "tmux_persistence-toggle",
                persistence_meta.title,
                persistence_meta.description,
                tmux_persistence,
                cx,
                |view, _cx| {
                    let next = !view.config.tmux_persistence;
                    match config::set_root_setting(RootSettingId::TmuxPersistence, &next.to_string())
                    {
                        Ok(()) => {
                            view.config.tmux_persistence = next;
                            termy_toast::success("Saved");
                        }
                        Err(error) => termy_toast::error(error),
                    }
                },
            ))
            .child(self.render_setting_row(
                "tmux_show_active_pane_border",
                "tmux_show_active_pane_border-toggle",
                show_active_border_meta.title,
                show_active_border_meta.description,
                tmux_show_active_pane_border,
                cx,
                |view, _cx| {
                    let next = !view.config.tmux_show_active_pane_border;
                    match config::set_root_setting(
                        RootSettingId::TmuxShowActivePaneBorder,
                        &next.to_string(),
                    ) {
                        Ok(()) => {
                            view.config.tmux_show_active_pane_border = next;
                            termy_toast::success("Saved");
                        }
                        Err(error) => termy_toast::error(error),
                    }
                },
            ))
            .child(self.render_editable_row(
                "tmux_binary",
                EditableField::TmuxBinary,
                binary_meta.title,
                binary_meta.description,
                binary,
                cx,
            ));

        group.into_any_element()
    }

    pub(super) fn render_terminal_scrolling_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let scrollback_meta = Self::setting_metadata_or_fallback("scrollback_history");
        let scroll_mult_meta = Self::setting_metadata_or_fallback("mouse_scroll_multiplier");
        let inactive_scrollback_meta =
            Self::setting_metadata_or_fallback("inactive_tab_scrollback");
        let scrollbar_visibility_meta = Self::setting_metadata_or_fallback("scrollbar_visibility");
        let scrollbar_style_meta = Self::setting_metadata_or_fallback("scrollbar_style");
        let scrollback = self.config.scrollback_history;
        let inactive_scrollback = self.config.inactive_tab_scrollback.unwrap_or(0);
        let scroll_mult = self.config.mouse_scroll_multiplier;

        div()
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
            .into_any_element()
    }

    pub(super) fn render_terminal_ui_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let pane_focus_effect_meta = Self::setting_metadata_or_fallback("pane_focus_effect");
        let pane_focus_strength_meta = Self::setting_metadata_or_fallback("pane_focus_strength");
        let palette_meta = Self::setting_metadata_or_fallback("command_palette_show_keybinds");
        let pane_focus_strength_percent = (self.config.pane_focus_strength * 100.0).round() as i32;
        let command_palette_show_keybinds = self.config.command_palette_show_keybinds;

        div()
            .child(self.render_group_header("UI"))
            .child(self.render_editable_row(
                "pane_focus_effect",
                EditableField::PaneFocusEffect,
                pane_focus_effect_meta.title,
                pane_focus_effect_meta.description,
                self.editable_field_value(EditableField::PaneFocusEffect),
                cx,
            ))
            .child(self.render_editable_row(
                "pane_focus_strength",
                EditableField::PaneFocusStrength,
                pane_focus_strength_meta.title,
                pane_focus_strength_meta.description,
                format!("{}%", pane_focus_strength_percent),
                cx,
            ))
            .child(self.render_setting_row(
                "command_palette_show_keybinds",
                "command_palette_show_keybinds-toggle",
                palette_meta.title,
                palette_meta.description,
                command_palette_show_keybinds,
                cx,
                |view, _cx| {
                    let next = !view.config.command_palette_show_keybinds;
                    match config::set_root_setting(
                        RootSettingId::CommandPaletteShowKeybinds,
                        &next.to_string(),
                    ) {
                        Ok(()) => {
                            view.config.command_palette_show_keybinds = next;
                            termy_toast::success("Saved");
                        }
                        Err(error) => termy_toast::error(error),
                    }
                },
            ))
            .into_any_element()
    }

    pub(super) fn render_tabs_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.render_section_header(
                "Tabs",
                "Configure tab behavior and titles",
                SettingsSection::Tabs,
                cx,
            ))
            .child(self.render_tabs_title_group(cx))
            .child(self.render_tabs_strip_group(cx))
            .child(self.render_tabs_titlebar_group(cx))
    }

    pub(super) fn render_tabs_title_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let shell_integration = self.config.tab_title.shell_integration;
        let fallback = self.config.tab_title.fallback.clone();
        let title_priority = self.editable_field_value(EditableField::TabTitlePriority);
        let explicit_prefix = self.config.tab_title.explicit_prefix.clone();
        let prompt_format = self.config.tab_title.prompt_format.clone();
        let command_format = self.config.tab_title.command_format.clone();
        let shell_integration_meta =
            Self::setting_metadata_or_fallback("tab_title_shell_integration");
        let title_mode_meta = Self::setting_metadata_or_fallback("tab_title_mode");
        let fallback_meta = Self::setting_metadata_or_fallback("tab_title_fallback");
        let title_priority_meta = Self::setting_metadata_or_fallback("tab_title_priority");
        let explicit_prefix_meta =
            Self::setting_metadata_or_fallback("tab_title_explicit_prefix");
        let prompt_format_meta = Self::setting_metadata_or_fallback("tab_title_prompt_format");
        let command_format_meta =
            Self::setting_metadata_or_fallback("tab_title_command_format");

        div()
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
                    let next = !view.config.tab_title.shell_integration;
                    match config::set_root_setting(
                        RootSettingId::TabTitleShellIntegration,
                        &next.to_string(),
                    ) {
                        Ok(()) => {
                            view.config.tab_title.shell_integration = next;
                            termy_toast::success("Saved");
                        }
                        Err(error) => termy_toast::error(error),
                    }
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
            .into_any_element()
    }

    pub(super) fn render_tabs_strip_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let close_visibility = self.editable_field_value(EditableField::TabCloseVisibility);
        let width_mode = self.editable_field_value(EditableField::TabWidthMode);
        let close_visibility_meta = Self::setting_metadata_or_fallback("tab_close_visibility");
        let width_mode_meta = Self::setting_metadata_or_fallback("tab_width_mode");

        div()
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
            .into_any_element()
    }

    pub(super) fn render_tabs_titlebar_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let show_termy = self.config.show_termy_in_titlebar;
        let show_termy_meta = Self::setting_metadata_or_fallback("show_termy_in_titlebar");

        div()
            .child(self.render_group_header("TITLE BAR"))
            .child(self.render_setting_row(
                "show_termy_in_titlebar",
                "show_termy_in_titlebar-toggle",
                show_termy_meta.title,
                show_termy_meta.description,
                show_termy,
                cx,
                |view, _cx| {
                    let next = !view.config.show_termy_in_titlebar;
                    match config::set_root_setting(
                        RootSettingId::ShowTermyInTitlebar,
                        &next.to_string(),
                    ) {
                        Ok(()) => {
                            view.config.show_termy_in_titlebar = next;
                            termy_toast::success("Saved");
                        }
                        Err(error) => termy_toast::error(error),
                    }
                },
            ))
            .into_any_element()
    }

    pub(super) fn render_advanced_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
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
        let working_dir_meta = Self::setting_metadata_or_fallback("working_dir");
        let working_dir_fallback_meta = Self::setting_metadata_or_fallback("working_dir_fallback");
        let warn_on_quit_meta =
            Self::setting_metadata_or_fallback("warn_on_quit_with_running_process");
        let window_width_meta = Self::setting_metadata_or_fallback("window_width");
        let window_height_meta = Self::setting_metadata_or_fallback("window_height");
        let config_path_display = self
            .config_path
            .as_ref()
            .map(|path| path.display().to_string())
            .filter(|path| !path.trim().is_empty())
            .unwrap_or_else(|| "config path not set".to_string());

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.render_section_header(
                "Advanced",
                "Advanced configuration options",
                SettingsSection::Advanced,
                cx,
            ))
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
                    let next = !view.config.warn_on_quit_with_running_process;
                    match config::set_root_setting(
                        RootSettingId::WarnOnQuitWithRunningProcess,
                        &next.to_string(),
                    ) {
                        Ok(()) => {
                            view.config.warn_on_quit_with_running_process = next;
                            termy_toast::success("Saved");
                        }
                        Err(error) => termy_toast::error(error),
                    }
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
                            .text_color(text_secondary)
                            .child(config_path_display),
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
