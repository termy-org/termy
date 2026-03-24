use super::search::SettingMetadata;
use super::*;
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

    fn render_root_bool_setting_row(
        &self,
        setting_key: &'static str,
        toggle_id: &'static str,
        setting: RootSettingId,
        checked: bool,
        success_message: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let metadata = Self::setting_metadata_or_fallback(setting_key);
        self.render_setting_row(
            setting_key,
            toggle_id,
            metadata.title,
            metadata.description,
            checked,
            cx,
            move |view, cx| {
                let next = !checked;
                match config::set_root_setting(setting, &next.to_string()) {
                    Ok(()) => {
                        let _ = view.reload_config_if_changed(cx);
                        termy_toast::success(success_message);
                        cx.notify();
                    }
                    Err(error) => termy_toast::error(error),
                }
            },
        )
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
                SettingsSection::ThemeStore => {
                    self.render_theme_store_section(cx).into_any_element()
                }
                SettingsSection::Advanced => self.render_advanced_section(cx).into_any_element(),
                SettingsSection::Colors => self.render_colors_section(cx).into_any_element(),
                SettingsSection::Keybindings => {
                    self.render_keybindings_section(cx).into_any_element()
                }
            })
            .into_any_element()
    }

    pub(super) fn render_appearance_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let background_blur = self.config.background_blur;
        let background_opacity_cells = self.config.background_opacity_cells;
        let theme = self.config.theme.clone();
        let chrome_contrast = self.config.chrome_contrast;
        let font_family = self.config.font_family.clone();
        let font_size = self.config.font_size;
        let line_height = self.config.line_height;
        let padding_x = self.config.padding_x;
        let padding_y = self.config.padding_y;
        let theme_meta = Self::setting_metadata_or_fallback("theme");
        let opacity_meta = Self::setting_metadata_or_fallback("background_opacity");
        let font_family_meta = Self::setting_metadata_or_fallback("font_family");
        let font_size_meta = Self::setting_metadata_or_fallback("font_size");
        let line_height_meta = Self::setting_metadata_or_fallback("line_height");
        let padding_x_meta = Self::setting_metadata_or_fallback("padding_x");
        let padding_y_meta = Self::setting_metadata_or_fallback("padding_y");

        let theme_rows = vec![self.render_editable_row(
            "theme",
            EditableField::Theme,
            theme_meta.title,
            theme_meta.description,
            theme,
            cx,
        )];
        let theme_group = self.render_settings_group("THEME", theme_rows);

        let chrome_rows = vec![self.render_root_bool_setting_row(
            "chrome_contrast",
            "chrome-contrast-toggle",
            RootSettingId::ChromeContrast,
            chrome_contrast,
            "Saved",
            cx,
        )];
        let chrome_group = self.render_settings_group("CHROME", chrome_rows);

        let window_rows = vec![
            self.render_root_bool_setting_row(
                "background_blur",
                "blur-toggle",
                RootSettingId::BackgroundBlur,
                background_blur,
                "Saved",
                cx,
            ),
            self.render_background_opacity_row(
                "background_opacity",
                opacity_meta.title,
                opacity_meta.description,
                cx,
            ),
            self.render_root_bool_setting_row(
                "background_opacity_cells",
                "background-opacity-cells-toggle",
                RootSettingId::BackgroundOpacityCells,
                background_opacity_cells,
                "Saved",
                cx,
            ),
        ];
        let window_group = self.render_settings_group("WINDOW", window_rows);

        let font_rows = vec![
            self.render_editable_row(
                "font_family",
                EditableField::FontFamily,
                font_family_meta.title,
                font_family_meta.description,
                font_family,
                cx,
            ),
            self.render_editable_row(
                "font_size",
                EditableField::FontSize,
                font_size_meta.title,
                font_size_meta.description,
                format!("{}px", font_size as i32),
                cx,
            ),
            self.render_editable_row(
                "line_height",
                EditableField::LineHeight,
                line_height_meta.title,
                line_height_meta.description,
                format!("{line_height:.2}"),
                cx,
            ),
        ];
        let font_group = self.render_settings_group("FONT", font_rows);

        let padding_rows = vec![
            self.render_editable_row(
                "padding_x",
                EditableField::PaddingX,
                padding_x_meta.title,
                padding_x_meta.description,
                format!("{}px", padding_x as i32),
                cx,
            ),
            self.render_editable_row(
                "padding_y",
                EditableField::PaddingY,
                padding_y_meta.title,
                padding_y_meta.description,
                format!("{}px", padding_y as i32),
                cx,
            ),
        ];
        let padding_group = self.render_settings_group("PADDING", padding_rows);

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
            .child(theme_group)
            .child(chrome_group)
            .child(window_group)
            .child(font_group)
            .child(padding_group)
    }

    pub(super) fn render_settings_group(
        &self,
        title: &'static str,
        rows: Vec<AnyElement>,
    ) -> AnyElement {
        // Keep group composition consistent across settings tabs (header + rows in normal flow)
        // to avoid spacing drift and width forcing regressions.
        div()
            .child(self.render_group_header(title))
            .children(rows)
            .into_any_element()
    }

    pub(super) fn render_terminal_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut section = div()
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
            .child(self.render_terminal_shell_group(cx));

        #[cfg(not(target_os = "windows"))]
        {
            section = section.child(self.render_terminal_tmux_group(cx));
        }

        section
            .child(self.render_terminal_scrolling_group(cx))
            .child(self.render_terminal_clipboard_group(cx))
            .child(self.render_terminal_ui_group(cx))
    }

    pub(super) fn render_terminal_cursor_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let cursor_style_meta = Self::setting_metadata_or_fallback("cursor_style");
        let cursor_blink = self.config.cursor_blink;

        let rows = vec![
            self.render_root_bool_setting_row(
                "cursor_blink",
                "cursor_blink-toggle",
                RootSettingId::CursorBlink,
                cursor_blink,
                "Saved",
                cx,
            ),
            self.render_editable_row(
                "cursor_style",
                EditableField::CursorStyle,
                cursor_style_meta.title,
                cursor_style_meta.description,
                self.editable_field_value(EditableField::CursorStyle),
                cx,
            ),
        ];
        self.render_settings_group("CURSOR", rows)
    }

    pub(super) fn render_terminal_shell_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let shell_meta = Self::setting_metadata_or_fallback("shell");
        let term_meta = Self::setting_metadata_or_fallback("term");
        let colorterm_meta = Self::setting_metadata_or_fallback("colorterm");
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

        let rows = vec![
            self.render_editable_row(
                "shell",
                EditableField::Shell,
                shell_meta.title,
                shell_meta.description,
                shell,
                cx,
            ),
            self.render_editable_row(
                "term",
                EditableField::Term,
                term_meta.title,
                term_meta.description,
                term,
                cx,
            ),
            self.render_editable_row(
                "colorterm",
                EditableField::Colorterm,
                colorterm_meta.title,
                colorterm_meta.description,
                colorterm,
                cx,
            ),
        ];
        self.render_settings_group("SHELL", rows)
    }

    #[cfg(not(target_os = "windows"))]
    pub(super) fn render_terminal_tmux_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let binary_meta = Self::setting_metadata_or_fallback("tmux_binary");
        let tmux_enabled = self.config.tmux_enabled;
        let tmux_persistence = self.config.tmux_persistence;
        let tmux_show_active_pane_border = self.config.tmux_show_active_pane_border;
        let binary = self.config.tmux_binary.clone();

        let mut rows = vec![self.render_root_bool_setting_row(
            "tmux_enabled",
            "tmux_enabled-toggle",
            RootSettingId::TmuxEnabled,
            tmux_enabled,
            "Saved. Use Tmux Sessions to switch runtime now.",
            cx,
        )];

        if tmux_enabled {
            rows.push(self.render_root_bool_setting_row(
                "tmux_persistence",
                "tmux_persistence-toggle",
                RootSettingId::TmuxPersistence,
                tmux_persistence,
                "Saved",
                cx,
            ));
            rows.push(self.render_root_bool_setting_row(
                "tmux_show_active_pane_border",
                "tmux_show_active_pane_border-toggle",
                RootSettingId::TmuxShowActivePaneBorder,
                tmux_show_active_pane_border,
                "Saved",
                cx,
            ));
            rows.push(self.render_editable_row(
                "tmux_binary",
                EditableField::TmuxBinary,
                binary_meta.title,
                binary_meta.description,
                binary,
                cx,
            ));
        }

        self.render_settings_group("TMUX", rows)
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

        let rows = vec![
            self.render_editable_row(
                "scrollback_history",
                EditableField::ScrollbackHistory,
                scrollback_meta.title,
                scrollback_meta.description,
                format!("{} lines", scrollback),
                cx,
            ),
            self.render_editable_row(
                "inactive_tab_scrollback",
                EditableField::InactiveTabScrollback,
                inactive_scrollback_meta.title,
                inactive_scrollback_meta.description,
                format!("{} lines", inactive_scrollback),
                cx,
            ),
            self.render_editable_row(
                "mouse_scroll_multiplier",
                EditableField::ScrollMultiplier,
                scroll_mult_meta.title,
                scroll_mult_meta.description,
                format!("{}x", scroll_mult),
                cx,
            ),
            self.render_editable_row(
                "scrollbar_visibility",
                EditableField::ScrollbarVisibility,
                scrollbar_visibility_meta.title,
                scrollbar_visibility_meta.description,
                self.editable_field_value(EditableField::ScrollbarVisibility),
                cx,
            ),
            self.render_editable_row(
                "scrollbar_style",
                EditableField::ScrollbarStyle,
                scrollbar_style_meta.title,
                scrollbar_style_meta.description,
                self.editable_field_value(EditableField::ScrollbarStyle),
                cx,
            ),
        ];
        self.render_settings_group("SCROLLING", rows)
    }

    pub(super) fn render_terminal_clipboard_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let copy_on_select = self.config.copy_on_select;
        let copy_on_select_toast = self.config.copy_on_select_toast;

        let rows = vec![
            self.render_root_bool_setting_row(
                "copy_on_select",
                "copy_on_select-toggle",
                RootSettingId::CopyOnSelect,
                copy_on_select,
                "Saved",
                cx,
            ),
            self.render_root_bool_setting_row(
                "copy_on_select_toast",
                "copy_on_select_toast-toggle",
                RootSettingId::CopyOnSelectToast,
                copy_on_select_toast,
                "Saved",
                cx,
            ),
        ];
        self.render_settings_group("CLIPBOARD", rows)
    }

    pub(super) fn render_terminal_ui_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let pane_focus_effect_meta = Self::setting_metadata_or_fallback("pane_focus_effect");
        let pane_focus_strength_meta = Self::setting_metadata_or_fallback("pane_focus_strength");
        let pane_focus_strength_percent = self.pane_focus_strength_display_percent();
        let command_palette_show_keybinds = self.config.command_palette_show_keybinds;

        let rows = vec![
            self.render_editable_row(
                "pane_focus_effect",
                EditableField::PaneFocusEffect,
                pane_focus_effect_meta.title,
                pane_focus_effect_meta.description,
                self.editable_field_value(EditableField::PaneFocusEffect),
                cx,
            ),
            self.render_editable_row(
                "pane_focus_strength",
                EditableField::PaneFocusStrength,
                pane_focus_strength_meta.title,
                pane_focus_strength_meta.description,
                format!("{}%", pane_focus_strength_percent),
                cx,
            ),
            self.render_root_bool_setting_row(
                "command_palette_show_keybinds",
                "command_palette_show_keybinds-toggle",
                RootSettingId::CommandPaletteShowKeybinds,
                command_palette_show_keybinds,
                "Saved",
                cx,
            ),
        ];
        self.render_settings_group("UI", rows)
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

    pub(super) fn render_theme_store_section(
        &mut self,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.ensure_theme_store_themes_loaded(cx);

        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(self.render_section_header(
                "Theme Store",
                "Browse and install community themes",
                SettingsSection::ThemeStore,
                cx,
            ))
            .child(self.render_tabs_theme_store_group(cx))
    }

    pub(super) fn render_tabs_title_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let shell_integration = self.config.tab_title.shell_integration;
        let fallback = self.config.tab_title.fallback.clone();
        let title_priority = self.editable_field_value(EditableField::TabTitlePriority);
        let explicit_prefix = self.config.tab_title.explicit_prefix.clone();
        let prompt_format = self.config.tab_title.prompt_format.clone();
        let command_format = self.config.tab_title.command_format.clone();
        let title_mode_meta = Self::setting_metadata_or_fallback("tab_title_mode");
        let fallback_meta = Self::setting_metadata_or_fallback("tab_title_fallback");
        let title_priority_meta = Self::setting_metadata_or_fallback("tab_title_priority");
        let explicit_prefix_meta = Self::setting_metadata_or_fallback("tab_title_explicit_prefix");
        let prompt_format_meta = Self::setting_metadata_or_fallback("tab_title_prompt_format");
        let command_format_meta = Self::setting_metadata_or_fallback("tab_title_command_format");

        let rows = vec![
            self.render_editable_row(
                "tab_title_mode",
                EditableField::TabTitleMode,
                title_mode_meta.title,
                title_mode_meta.description,
                self.editable_field_value(EditableField::TabTitleMode),
                cx,
            ),
            self.render_root_bool_setting_row(
                "tab_title_shell_integration",
                "tab_title_shell_integration-toggle",
                RootSettingId::TabTitleShellIntegration,
                shell_integration,
                "Saved",
                cx,
            ),
            self.render_editable_row(
                "tab_title_fallback",
                EditableField::TabFallbackTitle,
                fallback_meta.title,
                fallback_meta.description,
                fallback,
                cx,
            ),
            self.render_editable_row(
                "tab_title_priority",
                EditableField::TabTitlePriority,
                title_priority_meta.title,
                title_priority_meta.description,
                title_priority,
                cx,
            ),
            self.render_editable_row(
                "tab_title_explicit_prefix",
                EditableField::TabTitleExplicitPrefix,
                explicit_prefix_meta.title,
                explicit_prefix_meta.description,
                explicit_prefix,
                cx,
            ),
            self.render_editable_row(
                "tab_title_prompt_format",
                EditableField::TabTitlePromptFormat,
                prompt_format_meta.title,
                prompt_format_meta.description,
                prompt_format,
                cx,
            ),
            self.render_editable_row(
                "tab_title_command_format",
                EditableField::TabTitleCommandFormat,
                command_format_meta.title,
                command_format_meta.description,
                command_format,
                cx,
            ),
        ];

        self.render_settings_group("TAB TITLES", rows)
    }

    pub(super) fn render_tabs_strip_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let close_visibility = self.editable_field_value(EditableField::TabCloseVisibility);
        let width_mode = self.editable_field_value(EditableField::TabWidthMode);
        let vertical_tabs_width = self.editable_field_value(EditableField::VerticalTabsWidth);
        let show_switch_hints = self.config.tab_switch_modifier_hints;
        let vertical_tabs = self.config.vertical_tabs;
        let vertical_tabs_minimized = self.config.vertical_tabs_minimized;
        let agent_sidebar_enabled = self.config.agent_sidebar_enabled;
        let auto_hide_tabbar = self.config.auto_hide_tabbar;
        let close_visibility_meta = Self::setting_metadata_or_fallback("tab_close_visibility");
        let width_mode_meta = Self::setting_metadata_or_fallback("tab_width_mode");
        let vertical_width_meta = Self::setting_metadata_or_fallback("vertical_tabs_width");
        let rows = vec![
            self.render_editable_row(
                "tab_close_visibility",
                EditableField::TabCloseVisibility,
                close_visibility_meta.title,
                close_visibility_meta.description,
                close_visibility,
                cx,
            ),
            self.render_editable_row(
                "tab_width_mode",
                EditableField::TabWidthMode,
                width_mode_meta.title,
                width_mode_meta.description,
                width_mode,
                cx,
            ),
            self.render_editable_row(
                "vertical_tabs_width",
                EditableField::VerticalTabsWidth,
                vertical_width_meta.title,
                vertical_width_meta.description,
                format!("{}px", vertical_tabs_width),
                cx,
            ),
            self.render_root_bool_setting_row(
                "tab_switch_modifier_hints",
                "tab_switch_modifier_hints-toggle",
                RootSettingId::TabSwitchModifierHints,
                show_switch_hints,
                "Saved",
                cx,
            ),
            self.render_root_bool_setting_row(
                "vertical_tabs",
                "vertical_tabs-toggle",
                RootSettingId::VerticalTabs,
                vertical_tabs,
                "Saved",
                cx,
            ),
            self.render_root_bool_setting_row(
                "vertical_tabs_minimized",
                "vertical_tabs_minimized-toggle",
                RootSettingId::VerticalTabsMinimized,
                vertical_tabs_minimized,
                "Saved",
                cx,
            ),
            self.render_root_bool_setting_row(
                "agent_sidebar_enabled",
                "agent_sidebar_enabled-toggle",
                RootSettingId::AgentSidebarEnabled,
                agent_sidebar_enabled,
                "Saved",
                cx,
            ),
            self.render_root_bool_setting_row(
                "auto_hide_tabbar",
                "auto_hide_tabbar-toggle",
                RootSettingId::AutoHideTabbar,
                auto_hide_tabbar,
                "Saved",
                cx,
            ),
        ];

        self.render_settings_group("TAB STRIP", rows)
    }

    pub(super) fn render_tabs_titlebar_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let show_termy = self.config.show_termy_in_titlebar;
        let rows = vec![self.render_root_bool_setting_row(
            "show_termy_in_titlebar",
            "show_termy_in_titlebar-toggle",
            RootSettingId::ShowTermyInTitlebar,
            show_termy,
            "Saved",
            cx,
        )];

        self.render_settings_group("TITLE BAR", rows)
    }

    pub(super) fn render_tabs_theme_store_group(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let bg_card = self.bg_card();
        let border_color = self.border_color();
        let bg_input = self.bg_input();
        let hover_bg = self.bg_hover();
        let text_primary = self.text_primary();
        let text_secondary = self.text_secondary();
        let text_muted = self.text_muted();
        let accent = self.accent();
        let accent_hover = self.accent_with_alpha(0.8);
        let button_text = self.contrasting_text_for_fill(accent, bg_card);
        let button_hover_text = self.contrasting_text_for_fill(accent_hover, bg_card);
        let button_border = self.accent_with_alpha(0.45);
        let install_hover_bg = self.accent_with_alpha(0.18);
        let store_url = "https://termy.run/themes";
        let query_text = self.theme_store_search_state.text().to_string();
        let has_query = !query_text.trim().is_empty();
        let is_search_active = self.theme_store_search_active;
        let normalized_query = query_text.trim().to_ascii_lowercase();

        let mut rows: Vec<AnyElement> = Vec::new();
        let search_content = if is_search_active {
            let font = Font {
                family: self.config.font_family.clone().into(),
                ..Font::default()
            };
            TextInputElement::new(
                cx.entity(),
                self.focus_handle.clone(),
                font,
                px(SETTINGS_INPUT_TEXT_SIZE),
                text_secondary.into(),
                self.accent_with_alpha(0.3).into(),
                TextInputAlignment::Left,
            )
            .into_any_element()
        } else if has_query {
            div()
                .text_size(px(SETTINGS_INPUT_TEXT_SIZE))
                .text_color(text_secondary)
                .child(query_text.clone())
                .into_any_element()
        } else {
            div()
                .text_size(px(SETTINGS_INPUT_TEXT_SIZE))
                .text_color(text_muted)
                .child("Search themes...")
                .into_any_element()
        };

        let search_input = div()
            .id("theme-store-search-input")
            .h(px(36.0))
            .px_3()
            .rounded(px(0.0))
            .bg(bg_input)
            .border_1()
            .border_color(if is_search_active {
                accent
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
                    view.sidebar_search_active = false;
                    view.theme_store_search_active = true;
                    let index = view
                        .theme_store_search_state
                        .character_index_for_point(event.position);
                    if event.modifiers.shift {
                        view.theme_store_search_state.select_to_utf16(index);
                    } else {
                        view.theme_store_search_state.set_cursor_utf16(index);
                    }
                    view.theme_store_search_selecting = true;
                    view.focus_handle.focus(window, cx);
                    cx.notify();
                }),
            )
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _window, cx| {
                if !view.theme_store_search_selecting || !event.dragging() {
                    return;
                }
                let index = view
                    .theme_store_search_state
                    .character_index_for_point(event.position);
                view.theme_store_search_state.select_to_utf16(index);
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _event: &MouseUpEvent, _window, cx| {
                    if view.theme_store_search_selecting {
                        view.theme_store_search_selecting = false;
                        cx.notify();
                    }
                }),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|view, _event: &MouseUpEvent, _window, cx| {
                    if view.theme_store_search_selecting {
                        view.theme_store_search_selecting = false;
                        cx.notify();
                    }
                }),
            );

        rows.push(
            div()
                .py_3()
                .px_4()
                .bg(bg_card)
                .border_1()
                .border_color(border_color)
                .flex()
                .flex_col()
                .gap_2()
                .child(search_input)
                .child(
                    div()
                        .id("theme-store-open-link")
                        .mt_1()
                        .w(px(108.0))
                        .h(px(32.0))
                        .px_2()
                        .border_1()
                        .border_color(button_border)
                        .bg(bg_input)
                        .text_sm()
                        .text_color(text_secondary)
                        .cursor_pointer()
                        .flex()
                        .items_center()
                        .justify_center()
                        .hover(move |s| s.bg(hover_bg).text_color(text_primary))
                        .child("Open Store")
                        .on_click(cx.listener(move |_view, _, _, cx| {
                            if let Err(error) = SettingsWindow::open_url(store_url) {
                                termy_toast::error(error);
                            }
                            cx.notify();
                        })),
                )
                .into_any_element(),
        );

        if self.theme_store_loading {
            rows.push(
                div()
                    .py_4()
                    .px_4()
                    .bg(bg_card)
                    .border_1()
                    .border_color(border_color)
                    .text_sm()
                    .text_color(text_muted)
                    .child("Loading themes from store...")
                    .into_any_element(),
            );
        } else if let Some(error) = self.theme_store_error.clone() {
            rows.push(
                div()
                    .py_4()
                    .px_4()
                    .bg(bg_card)
                    .border_1()
                    .border_color(border_color)
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_sm().text_color(text_secondary).child(error))
                    .child(
                        div()
                            .id("theme-store-retry-btn")
                            .mt_1()
                            .w(px(90.0))
                            .px_3()
                            .py_1()
                            .border_1()
                            .border_color(button_border)
                            .bg(accent)
                            .text_sm()
                            .text_color(button_text)
                            .cursor_pointer()
                            .hover(move |s| s.bg(accent_hover).text_color(button_hover_text))
                            .child("Retry")
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.theme_store_loaded = false;
                                view.refresh_theme_store_themes(cx);
                            })),
                    )
                    .into_any_element(),
            );
        } else {
            if self.theme_store_from_cache {
                rows.push(
                    div()
                        .py_2()
                        .px_4()
                        .bg(bg_card)
                        .border_1()
                        .border_color(border_color)
                        .text_sm()
                        .text_color(text_muted)
                        .child("Showing cached themes (API unavailable)")
                        .into_any_element(),
                );
            }

            if self.theme_store_themes.is_empty() {
                rows.push(
                    div()
                        .py_4()
                        .px_4()
                        .bg(bg_card)
                        .border_1()
                        .border_color(border_color)
                        .text_sm()
                        .text_color(text_muted)
                        .child("No themes available in store.")
                        .into_any_element(),
                );
            } else {
                let filtered_themes: Vec<ThemeStoreTheme> = self
                    .theme_store_themes
                    .iter()
                    .filter(|theme| {
                        if normalized_query.is_empty() {
                            return true;
                        }
                        let haystack = format!(
                            "{} {} {}",
                            theme.name.to_ascii_lowercase(),
                            theme.slug.to_ascii_lowercase(),
                            theme.description.to_ascii_lowercase()
                        );
                        haystack.contains(&normalized_query)
                    })
                    .take(30)
                    .cloned()
                    .collect();

                if filtered_themes.is_empty() {
                    rows.push(
                        div()
                            .py_4()
                            .px_4()
                            .bg(bg_card)
                            .border_1()
                            .border_color(border_color)
                            .text_sm()
                            .text_color(text_muted)
                            .child("No themes match your search.")
                            .into_any_element(),
                    );
                }

                let mut theme_cards: Vec<AnyElement> = Vec::new();

                for theme in filtered_themes {
                    let install_theme = theme.clone();
                    let version_label = theme
                        .latest_version
                        .clone()
                        .unwrap_or_else(|| "n/a".to_string());
                    let slug_key = theme.slug.to_ascii_lowercase();
                    let installed_version = self.theme_store_installed_versions.get(&slug_key);
                    let is_installed = self
                        .theme_store_installed_versions
                        .get(&slug_key)
                        .is_some_and(|version| {
                            theme
                                .latest_version
                                .as_deref()
                                .is_none_or(|latest| version.eq_ignore_ascii_case(latest))
                        });
                    let description = if theme.description.trim().is_empty() {
                        "No description provided.".to_string()
                    } else {
                        theme.description.clone()
                    };
                    let installed_label = installed_version
                        .and_then(|version| (!version.trim().is_empty()).then_some(version))
                        .map(|_| "Installed".to_string())
                        .unwrap_or_else(|| "Installed".to_string());
                    let install_button = if is_installed {
                        let uninstall_slug = theme.slug.clone();
                        div()
                            .id(SharedString::from(format!(
                                "theme-store-actions-{}",
                                theme.slug
                            )))
                            .mt_auto()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .w(px(108.0))
                                    .h(px(32.0))
                                    .px_2()
                                    .border_1()
                                    .border_color(border_color)
                                    .bg(bg_card)
                                    .text_sm()
                                    .text_color(text_muted)
                                    .whitespace_nowrap()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(installed_label),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "theme-store-uninstall-{}",
                                        theme.slug
                                    )))
                                    .w(px(108.0))
                                    .h(px(32.0))
                                    .px_2()
                                    .border_1()
                                    .border_color(button_border)
                                    .bg(bg_input)
                                    .text_sm()
                                    .text_color(text_secondary)
                                    .cursor_pointer()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .hover(move |s| s.bg(install_hover_bg).text_color(text_primary))
                                    .child("Uninstall")
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        view.uninstall_theme_store_theme(&uninstall_slug, cx);
                                        cx.notify();
                                    })),
                            )
                            .into_any_element()
                    } else {
                        div()
                            .id(SharedString::from(format!(
                                "theme-store-install-{}",
                                theme.slug
                            )))
                            .mt_auto()
                            .w(px(108.0))
                            .h(px(32.0))
                            .px_2()
                            .border_1()
                            .border_color(button_border)
                            .bg(bg_input)
                            .text_sm()
                            .text_color(text_secondary)
                            .whitespace_nowrap()
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(move |s| s.bg(install_hover_bg).text_color(text_primary))
                            .child("Install")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                view.confirm_install_theme_store_theme(install_theme.clone(), cx);
                            }))
                            .into_any_element()
                    };

                    theme_cards.push(
                        div()
                            .py_3()
                            .px_4()
                            .w(px(250.0))
                            .min_w(px(250.0))
                            .max_w(px(250.0))
                            .min_h(px(186.0))
                            .bg(bg_card)
                            .border_1()
                            .border_color(border_color)
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        div().text_sm().text_color(text_primary).child(theme.name),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(text_muted)
                                            .child(format!("v{version_label}")),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(text_muted)
                                    .min_h(px(42.0))
                                    .child(description),
                            )
                            .child(install_button)
                            .into_any_element(),
                    );
                }

                if !theme_cards.is_empty() {
                    rows.push(
                        div()
                            .mt_3()
                            .pt_3()
                            .border_t_1()
                            .border_color(border_color)
                            .flex()
                            .flex_wrap()
                            .gap_2()
                            .children(theme_cards)
                            .into_any_element(),
                    );
                }
            } // end else (themes loaded, not error)
        }

        self.render_settings_group("THEME STORE", rows)
    }

    pub(super) fn render_advanced_section(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let working_dir = self
            .config
            .working_dir
            .clone()
            .unwrap_or_else(|| "Not set".to_string());
        let working_dir_fallback = self.editable_field_value(EditableField::WorkingDirFallback);
        let always_warn_on_quit = self.config.warn_on_quit;
        let warn_on_quit_with_running_process = self.config.warn_on_quit_with_running_process;
        let native_tab_persistence = self.config.native_tab_persistence;
        let native_layout_autosave = self.config.native_layout_autosave;
        let native_buffer_persistence = self.config.native_buffer_persistence;
        let show_debug_overlay = self.config.show_debug_overlay;
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
        let window_width_meta = Self::setting_metadata_or_fallback("window_width");
        let window_height_meta = Self::setting_metadata_or_fallback("window_height");
        let config_path_display = self
            .config_path
            .as_ref()
            .map(|path| path.display().to_string())
            .filter(|path| !path.trim().is_empty())
            .unwrap_or_else(|| "config path not set".to_string());

        let startup_rows = vec![
            self.render_editable_row(
                "working_dir",
                EditableField::WorkingDirectory,
                working_dir_meta.title,
                working_dir_meta.description,
                working_dir,
                cx,
            ),
            self.render_editable_row(
                "working_dir_fallback",
                EditableField::WorkingDirFallback,
                working_dir_fallback_meta.title,
                working_dir_fallback_meta.description,
                working_dir_fallback,
                cx,
            ),
            self.render_root_bool_setting_row(
                "native_tab_persistence",
                "native-tab-persistence-toggle",
                RootSettingId::NativeTabPersistence,
                native_tab_persistence,
                "Saved",
                cx,
            ),
            self.render_root_bool_setting_row(
                "native_layout_autosave",
                "native-layout-autosave-toggle",
                RootSettingId::NativeLayoutAutosave,
                native_layout_autosave,
                "Saved",
                cx,
            ),
            self.render_root_bool_setting_row(
                "native_buffer_persistence",
                "native-buffer-persistence-toggle",
                RootSettingId::NativeBufferPersistence,
                native_buffer_persistence,
                "Saved",
                cx,
            ),
        ];
        let startup_group = self.render_settings_group("STARTUP", startup_rows);

        let safety_rows = vec![
            self.render_root_bool_setting_row(
                "warn_on_quit",
                "warn_on_quit-toggle",
                RootSettingId::WarnOnQuit,
                always_warn_on_quit,
                "Saved",
                cx,
            ),
            self.render_root_bool_setting_row(
                "warn_on_quit_with_running_process",
                "warn_on_quit_with_running_process-toggle",
                RootSettingId::WarnOnQuitWithRunningProcess,
                warn_on_quit_with_running_process,
                "Saved",
                cx,
            ),
        ];
        let safety_group = self.render_settings_group("SAFETY", safety_rows);

        let window_rows = vec![
            self.render_editable_row(
                "window_width",
                EditableField::WindowWidth,
                window_width_meta.title,
                window_width_meta.description,
                format!("{}px", window_width as i32),
                cx,
            ),
            self.render_editable_row(
                "window_height",
                EditableField::WindowHeight,
                window_height_meta.title,
                window_height_meta.description,
                format!("{}px", window_height as i32),
                cx,
            ),
        ];
        let window_group = self.render_settings_group("WINDOW", window_rows);

        let ui_rows = vec![self.render_root_bool_setting_row(
            "show_debug_overlay",
            "show_debug_overlay-toggle",
            RootSettingId::ShowDebugOverlay,
            show_debug_overlay,
            "Saved",
            cx,
        )];
        let ui_group = self.render_settings_group("UI", ui_rows);

        let auto_update = self.config.auto_update;
        let updates_rows = vec![self.render_root_bool_setting_row(
            "auto_update",
            "auto_update-toggle",
            RootSettingId::AutoUpdate,
            auto_update,
            "Saved",
            cx,
        )];
        let updates_group = self.render_settings_group("UPDATES", updates_rows);

        let config_file_card = div()
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
                            log::error!("Failed to open config file from settings: {}", error);
                            termy_toast::error(error.to_string());
                        }
                        cx.notify();
                    })),
            )
            .into_any_element();

        let config_group = self.render_settings_group("CONFIG FILE", vec![config_file_card]);

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
            .child(startup_group)
            .child(safety_group)
            .child(window_group)
            .child(ui_group)
            .child(updates_group)
            .child(config_group)
    }
}
