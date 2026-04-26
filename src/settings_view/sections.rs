use super::*;
use gpui_component::setting::{
    NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage,
};
use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};
use termy_config_core::RootSettingId;

static LOGGED_MISSING_METADATA_KEYS: LazyLock<Mutex<HashSet<&'static str>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));

fn setting_metadata(key: &'static str) -> Option<&'static search::SettingMetadata> {
    search::SETTINGS_METADATA.iter().find(|s| s.key == key)
}

fn setting_metadata_or_fallback(key: &'static str) -> &'static search::SettingMetadata {
    if let Some(m) = setting_metadata(key) {
        return m;
    }
    let mut logged = LOGGED_MISSING_METADATA_KEYS
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    if logged.insert(key) {
        log::error!("Missing settings metadata for key '{}'", key);
    }
    &search::FALLBACK_SETTING_METADATA
}

fn set_and_reload(
    entity: &crate::gpui::Entity<SettingsWindow>,
    setting: RootSettingId,
    value: &str,
    cx: &mut App,
) {
    match config::set_root_setting(setting, value) {
        Ok(()) => {
            let _ = entity.update(cx, |view, cx| {
                let _ = view.reload_config_if_changed(cx);
                termy_toast::success("Saved");
                cx.notify();
            });
        }
        Err(error) => termy_toast::error(error),
    }
}

fn switch_item(
    entity: &crate::gpui::Entity<SettingsWindow>,
    key: &'static str,
    setting: RootSettingId,
    get: impl Fn(&AppConfig) -> bool + 'static,
) -> SettingItem {
    let m = setting_metadata_or_fallback(key);
    let entity = entity.clone();
    let entity2 = entity.clone();
    SettingItem::new(
        m.title,
        SettingField::switch(
            move |cx| get(&entity.read(cx).config),
            move |value, cx| set_and_reload(&entity2, setting, &value.to_string(), cx),
        ),
    )
    .description(m.description)
}

fn dropdown_item(
    entity: &crate::gpui::Entity<SettingsWindow>,
    key: &'static str,
    options: Vec<(crate::gpui::SharedString, crate::gpui::SharedString)>,
    get: impl Fn(&AppConfig) -> crate::gpui::SharedString + 'static,
    setting: RootSettingId,
) -> SettingItem {
    let m = setting_metadata_or_fallback(key);
    let entity = entity.clone();
    let entity2 = entity.clone();
    SettingItem::new(
        m.title,
        SettingField::dropdown(
            options,
            move |cx| get(&entity.read(cx).config),
            move |value: crate::gpui::SharedString, cx| {
                set_and_reload(&entity2, setting, &value, cx);
            },
        ),
    )
    .description(m.description)
}

fn number_item(
    entity: &crate::gpui::Entity<SettingsWindow>,
    key: &'static str,
    num_options: NumberFieldOptions,
    get: impl Fn(&AppConfig) -> f64 + 'static,
    setting: RootSettingId,
) -> SettingItem {
    let m = setting_metadata_or_fallback(key);
    let entity = entity.clone();
    let entity2 = entity.clone();
    SettingItem::new(
        m.title,
        SettingField::number_input(
            num_options,
            move |cx| get(&entity.read(cx).config),
            move |value: f64, cx| set_and_reload(&entity2, setting, &value.to_string(), cx),
        ),
    )
    .description(m.description)
}

fn input_item(
    entity: &crate::gpui::Entity<SettingsWindow>,
    key: &'static str,
    get: impl Fn(&AppConfig) -> crate::gpui::SharedString + 'static,
    apply: impl Fn(&str, &mut SettingsWindow) -> Result<(), String> + 'static,
) -> SettingItem {
    let m = setting_metadata_or_fallback(key);
    let entity = entity.clone();
    let entity2 = entity.clone();
    SettingItem::new(
        m.title,
        SettingField::input(
            move |cx| get(&entity.read(cx).config),
            move |value: crate::gpui::SharedString, cx| {
                let _ = entity2.update(cx, |view, cx| {
                    match apply(&value, view) {
                        Ok(()) => termy_toast::success("Saved"),
                        Err(error) => termy_toast::error(error),
                    }
                    cx.notify();
                });
            },
        ),
    )
    .description(m.description)
}

impl SettingsWindow {
    pub(super) fn build_pages(
        &self,
        entity: &crate::gpui::Entity<Self>,
        cx: &mut Context<Self>,
    ) -> Vec<SettingPage> {
        vec![
            self.build_appearance_page(entity, cx),
            self.build_terminal_page(entity, cx),
            self.build_tabs_page(entity, cx),
            self.build_theme_store_page(entity, cx),
            self.build_advanced_page(entity, cx),
            self.build_colors_page(entity, cx),
            self.build_keybindings_page(entity, cx),
        ]
    }

    fn build_appearance_page(
        &self,
        entity: &crate::gpui::Entity<Self>,
        _cx: &mut Context<Self>,
    ) -> SettingPage {
        let theme_options: Vec<(crate::gpui::SharedString, crate::gpui::SharedString)> = self
            .ordered_theme_ids_for_settings()
            .into_iter()
            .map(|id| (id.clone().into(), id.into()))
            .collect();

        let font_options: Vec<(crate::gpui::SharedString, crate::gpui::SharedString)> = self
            .ordered_font_families_for_settings()
            .into_iter()
            .map(|f| (f.clone().into(), f.into()))
            .collect();

        let entity = entity.clone();
        let entity2 = entity.clone();
        let entity3 = entity.clone();

        let theme_group = SettingGroup::new()
            .title("THEME")
            .item({
                let entity = entity.clone();
                let entity2 = entity.clone();
                SettingItem::new(
                    setting_metadata_or_fallback("theme").title,
                    SettingField::dropdown(
                        theme_options,
                        move |cx| entity.read(cx).config.theme.clone().into(),
                        move |value: crate::gpui::SharedString, cx| {
                            let value = value.to_string();
                            if value.is_empty() {
                                termy_toast::error("Theme cannot be empty");
                                return;
                            }
                            match crate::config::set_theme_in_config(&value) {
                                Ok(_) => {
                                    let _ = entity2.update(cx, |view, cx| {
                                        let canonical = crate::config::load_runtime_config(
                                            &mut None,
                                            "Failed to reload config",
                                        )
                                        .config
                                        .theme;
                                        view.config.theme = canonical;
                                        view.colors = crate::colors::TerminalColors::from_theme(
                                            &view.config.theme,
                                            &view.config.colors,
                                        );
                                        termy_toast::success("Saved");
                                        cx.notify();
                                    });
                                }
                                Err(error) => termy_toast::error(error),
                            }
                        },
                    ),
                )
                .description(setting_metadata_or_fallback("theme").description)
            });

        let chrome_group = SettingGroup::new()
            .title("CHROME")
            .item(switch_item(
                &entity,
                "chrome_contrast",
                RootSettingId::ChromeContrast,
                |c| c.chrome_contrast,
            ));

        let window_group = SettingGroup::new()
            .title("WINDOW")
            .item(switch_item(
                &entity,
                "background_blur",
                RootSettingId::BackgroundBlur,
                |c| c.background_blur,
            ))
            .item({
                let entity = entity.clone();
                let entity2 = entity.clone();
                SettingItem::new(
                    setting_metadata_or_fallback("background_opacity").title,
                    SettingField::number_input(
                        NumberFieldOptions {
                            min: 0.0,
                            max: 100.0,
                            step: 5.0,
                        },
                        move |cx| (entity.read(cx).config.background_opacity * 100.0) as f64,
                        move |value: f64, cx| {
                            let opacity = (value / 100.0).clamp(0.0, 1.0);
                            let _ = entity2.update(cx, |view, cx| {
                                view.clear_background_opacity_preview();
                                match view.persist_background_opacity(opacity as f32) {
                                    Ok(()) => termy_toast::success("Saved"),
                                    Err(error) => termy_toast::error(error),
                                }
                                cx.notify();
                            });
                        },
                    ),
                )
                .description(setting_metadata_or_fallback("background_opacity").description)
            })
            .item(switch_item(
                &entity,
                "background_opacity_cells",
                RootSettingId::BackgroundOpacityCells,
                |c| c.background_opacity_cells,
            ));

        let font_group = SettingGroup::new()
            .title("FONT")
            .item({
                let entity = entity2.clone();
                let entity_set = entity.clone();
                SettingItem::new(
                    setting_metadata_or_fallback("font_family").title,
                    SettingField::dropdown(
                        font_options,
                        move |cx| entity.read(cx).config.font_family.clone().into(),
                        move |value: crate::gpui::SharedString, cx| {
                            let value = value.to_string();
                            if value.is_empty() {
                                termy_toast::error("Font family cannot be empty");
                                return;
                            }
                            set_and_reload(
                                &entity_set,
                                RootSettingId::FontFamily,
                                &value,
                                cx,
                            );
                        },
                    ),
                )
                .description(setting_metadata_or_fallback("font_family").description)
            })
            .item(number_item(
                &entity2,
                "font_size",
                NumberFieldOptions {
                    min: 1.0,
                    max: 4096.0,
                    step: 1.0,
                },
                |c| c.font_size as f64,
                RootSettingId::FontSize,
            ))
            .item(number_item(
                &entity2,
                "line_height",
                NumberFieldOptions {
                    min: termy_config_core::MIN_LINE_HEIGHT as f64,
                    max: termy_config_core::MAX_LINE_HEIGHT as f64,
                    step: 0.05,
                },
                |c| c.line_height as f64,
                RootSettingId::LineHeight,
            ));

        let padding_group = SettingGroup::new()
            .title("PADDING")
            .item(number_item(
                &entity3,
                "padding_x",
                NumberFieldOptions {
                    min: 0.0,
                    max: 4096.0,
                    step: 1.0,
                },
                |c| c.padding_x as f64,
                RootSettingId::PaddingX,
            ))
            .item(number_item(
                &entity3,
                "padding_y",
                NumberFieldOptions {
                    min: 0.0,
                    max: 4096.0,
                    step: 1.0,
                },
                |c| c.padding_y as f64,
                RootSettingId::PaddingY,
            ));

        SettingPage::new("Appearance")
            .description("Customize the look and feel")
            .default_open(true)
            .group(theme_group)
            .group(chrome_group)
            .group(window_group)
            .group(font_group)
            .group(padding_group)
    }

    fn build_terminal_page(
        &self,
        entity: &crate::gpui::Entity<Self>,
        _cx: &mut Context<Self>,
    ) -> SettingPage {
        let e = entity.clone();
        let e2 = entity.clone();
        let e3 = entity.clone();

        let cursor_group = SettingGroup::new()
            .title("CURSOR")
            .item(switch_item(
                entity,
                "cursor_blink",
                RootSettingId::CursorBlink,
                |c| c.cursor_blink,
            ))
            .item(dropdown_item(
                entity,
                "cursor_style",
                vec![
                    ("line".into(), "Line".into()),
                    ("block".into(), "Block".into()),
                ],
                |c| match c.cursor_style {
                    termy_config_core::CursorStyle::Line => "line".into(),
                    termy_config_core::CursorStyle::Block => "block".into(),
                },
                RootSettingId::CursorStyle,
            ));

        let shell_group = SettingGroup::new()
            .title("SHELL")
            .item({
                let entity = e.clone();
                input_item(
                    &entity,
                    "shell",
                    move |c| c.shell.clone().unwrap_or_default().into(),
                    move |value, view| {
                        if value.is_empty() {
                            view.config.shell = None;
                            config::set_root_setting(RootSettingId::Shell, "none")
                        } else {
                            view.config.shell = Some(value.to_string());
                            config::set_root_setting(RootSettingId::Shell, value)
                        }
                    },
                )
            })
            .item({
                let entity = e.clone();
                input_item(
                    &entity,
                    "term",
                    move |c| c.term.clone().into(),
                    move |value, view| {
                        if value.is_empty() {
                            return Err("TERM cannot be empty".to_string());
                        }
                        view.config.term = value.to_string();
                        config::set_root_setting(RootSettingId::Term, value)
                    },
                )
            })
            .item({
                let entity = e.clone();
                input_item(
                    &entity,
                    "colorterm",
                    move |c| c.colorterm.clone().unwrap_or_default().into(),
                    move |value, view| {
                        if value.is_empty() {
                            view.config.colorterm = None;
                            config::set_root_setting(RootSettingId::Colorterm, "none")
                        } else {
                            view.config.colorterm = Some(value.to_string());
                            config::set_root_setting(RootSettingId::Colorterm, value)
                        }
                    },
                )
            });

        let mut page = SettingPage::new("Terminal")
            .description("Configure terminal behavior")
            .group(cursor_group)
            .group(shell_group);

        #[cfg(not(target_os = "windows"))]
        {
            let mut tmux_rows = vec![
                switch_item(
                    &e2,
                    "tmux_enabled",
                    RootSettingId::TmuxEnabled,
                    |c| c.tmux_enabled,
                ),
            ];
            let tmux_on = self.config.tmux_enabled;
            if tmux_on {
                tmux_rows.push(switch_item(
                    &e2,
                    "tmux_persistence",
                    RootSettingId::TmuxPersistence,
                    |c| c.tmux_persistence,
                ));
                tmux_rows.push(switch_item(
                    &e2,
                    "tmux_show_active_pane_border",
                    RootSettingId::TmuxShowActivePaneBorder,
                    |c| c.tmux_show_active_pane_border,
                ));
                tmux_rows.push({
                    let entity = e2.clone();
                    input_item(
                        &entity,
                        "tmux_binary",
                        move |c| c.tmux_binary.clone().into(),
                        move |value, view| {
                            if value.is_empty() {
                                return Err("tmux binary cannot be empty".to_string());
                            }
                            view.config.tmux_binary = value.to_string();
                            config::set_root_setting(RootSettingId::TmuxBinary, value)
                        },
                    )
                });
            }
            let mut tmux_group = SettingGroup::new().title("TMUX");
            for item in tmux_rows {
                tmux_group = tmux_group.item(item);
            }
            page = page.group(tmux_group);
        }

        let scrolling_group = SettingGroup::new()
            .title("SCROLLING")
            .item(number_item(
                &e3,
                "scrollback_history",
                NumberFieldOptions {
                    min: 0.0,
                    max: 100000.0,
                    step: 100.0,
                },
                |c| c.scrollback_history as f64,
                RootSettingId::ScrollbackHistory,
            ))
            .item(number_item(
                &e3,
                "inactive_tab_scrollback",
                NumberFieldOptions {
                    min: 0.0,
                    max: 100000.0,
                    step: 100.0,
                },
                |c| c.inactive_tab_scrollback.unwrap_or(0) as f64,
                RootSettingId::InactiveTabScrollback,
            ))
            .item(number_item(
                &e3,
                "mouse_scroll_multiplier",
                NumberFieldOptions {
                    min: 0.1,
                    max: 1000.0,
                    step: 0.1,
                },
                |c| c.mouse_scroll_multiplier as f64,
                RootSettingId::MouseScrollMultiplier,
            ))
            .item(dropdown_item(
                &e3,
                "scrollbar_visibility",
                vec![
                    ("off".into(), "Off".into()),
                    ("always".into(), "Always".into()),
                    ("on_scroll".into(), "On Scroll".into()),
                ],
                |c| match c.terminal_scrollbar_visibility {
                    termy_config_core::TerminalScrollbarVisibility::Off => "off".into(),
                    termy_config_core::TerminalScrollbarVisibility::Always => "always".into(),
                    termy_config_core::TerminalScrollbarVisibility::OnScroll => "on_scroll".into(),
                },
                RootSettingId::ScrollbarVisibility,
            ))
            .item(dropdown_item(
                &e3,
                "scrollbar_style",
                vec![
                    ("neutral".into(), "Neutral".into()),
                    ("muted_theme".into(), "Muted Theme".into()),
                    ("theme".into(), "Theme".into()),
                ],
                |c| match c.terminal_scrollbar_style {
                    termy_config_core::TerminalScrollbarStyle::Neutral => "neutral".into(),
                    termy_config_core::TerminalScrollbarStyle::MutedTheme => "muted_theme".into(),
                    termy_config_core::TerminalScrollbarStyle::Theme => "theme".into(),
                },
                RootSettingId::ScrollbarStyle,
            ));

        let clipboard_group = SettingGroup::new()
            .title("CLIPBOARD")
            .item(switch_item(
                entity,
                "copy_on_select",
                RootSettingId::CopyOnSelect,
                |c| c.copy_on_select,
            ))
            .item(switch_item(
                entity,
                "copy_on_select_toast",
                RootSettingId::CopyOnSelectToast,
                |c| c.copy_on_select_toast,
            ));

        let ui_group = SettingGroup::new()
            .title("UI")
            .item(dropdown_item(
                entity,
                "pane_focus_effect",
                vec![
                    ("off".into(), "Off".into()),
                    ("soft_spotlight".into(), "Soft Spotlight".into()),
                    ("cinematic".into(), "Cinematic".into()),
                    ("minimal".into(), "Minimal".into()),
                ],
                |c| match c.pane_focus_effect {
                    termy_config_core::PaneFocusEffect::Off => "off".into(),
                    termy_config_core::PaneFocusEffect::SoftSpotlight => "soft_spotlight".into(),
                    termy_config_core::PaneFocusEffect::Cinematic => "cinematic".into(),
                    termy_config_core::PaneFocusEffect::Minimal => "minimal".into(),
                },
                RootSettingId::PaneFocusEffect,
            ))
            .item(number_item(
                entity,
                "pane_focus_strength",
                NumberFieldOptions {
                    min: 0.0,
                    max: 100.0,
                    step: 5.0,
                },
                |c| {
                    let max = 2.0f32;
                    (c.pane_focus_strength.clamp(0.0, max) / max * 100.0) as f64
                },
                RootSettingId::PaneFocusStrength,
            ))
            .item(switch_item(
                entity,
                "command_palette_show_keybinds",
                RootSettingId::CommandPaletteShowKeybinds,
                |c| c.command_palette_show_keybinds,
            ));

        page.group(scrolling_group)
            .group(clipboard_group)
            .group(ui_group)
    }

    fn build_tabs_page(
        &self,
        entity: &crate::gpui::Entity<Self>,
        _cx: &mut Context<Self>,
    ) -> SettingPage {
        let e = entity.clone();
        let e2 = entity.clone();
        let e3 = entity.clone();

        let title_group = SettingGroup::new()
            .title("TAB TITLES")
            .item(dropdown_item(
                entity,
                "tab_title_mode",
                vec![
                    ("smart".into(), "Smart".into()),
                    ("shell".into(), "Shell".into()),
                    ("explicit".into(), "Explicit".into()),
                    ("static".into(), "Static".into()),
                ],
                |c| match c.tab_title.mode {
                    termy_config_core::TabTitleMode::Smart => "smart".into(),
                    termy_config_core::TabTitleMode::Shell => "shell".into(),
                    termy_config_core::TabTitleMode::Explicit => "explicit".into(),
                    termy_config_core::TabTitleMode::Static => "static".into(),
                },
                RootSettingId::TabTitleMode,
            ))
            .item(switch_item(
                entity,
                "tab_title_shell_integration",
                RootSettingId::TabTitleShellIntegration,
                |c| c.tab_title.shell_integration,
            ))
            .item({
                let entity = e.clone();
                input_item(
                    &entity,
                    "tab_title_fallback",
                    move |c| c.tab_title.fallback.clone().into(),
                    move |value, view| {
                        if value.is_empty() {
                            return Err("Fallback title cannot be empty".to_string());
                        }
                        view.config.tab_title.fallback = value.to_string();
                        config::set_root_setting(RootSettingId::TabTitleFallback, value)
                    },
                )
            })
            .item({
                let entity = e.clone();
                input_item(
                    &entity,
                    "tab_title_priority",
                    move |c| {
                        c.tab_title
                            .priority
                            .iter()
                            .map(|s| match s {
                                termy_config_core::TabTitleSource::Manual => "manual",
                                termy_config_core::TabTitleSource::Explicit => "explicit",
                                termy_config_core::TabTitleSource::Shell => "shell",
                                termy_config_core::TabTitleSource::Fallback => "fallback",
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                            .into()
                    },
                    move |value, view| {
                        if value.is_empty() {
                            return Err("Title priority cannot be empty".to_string());
                        }
                        view.config.tab_title.priority = value
                            .split(',')
                            .filter_map(Self::parse_tab_title_source_token)
                            .fold(Vec::new(), |mut acc, source| {
                                if !acc.contains(&source) {
                                    acc.push(source);
                                }
                                acc
                            });
                        if view.config.tab_title.priority.is_empty() {
                            return Err("Title priority must contain valid sources".to_string());
                        }
                        config::set_root_setting(RootSettingId::TabTitlePriority, value)
                    },
                )
            })
            .item({
                let entity = e.clone();
                input_item(
                    &entity,
                    "tab_title_explicit_prefix",
                    move |c| c.tab_title.explicit_prefix.clone().into(),
                    move |value, view| {
                        if value.is_empty() {
                            return Err("Explicit prefix cannot be empty".to_string());
                        }
                        view.config.tab_title.explicit_prefix = value.to_string();
                        config::set_root_setting(RootSettingId::TabTitleExplicitPrefix, value)
                    },
                )
            })
            .item({
                let entity = e.clone();
                input_item(
                    &entity,
                    "tab_title_prompt_format",
                    move |c| c.tab_title.prompt_format.clone().into(),
                    move |value, view| {
                        if value.is_empty() {
                            return Err("Prompt format cannot be empty".to_string());
                        }
                        view.config.tab_title.prompt_format = value.to_string();
                        config::set_root_setting(RootSettingId::TabTitlePromptFormat, value)
                    },
                )
            })
            .item({
                let entity = e.clone();
                input_item(
                    &entity,
                    "tab_title_command_format",
                    move |c| c.tab_title.command_format.clone().into(),
                    move |value, view| {
                        if value.is_empty() {
                            return Err("Command format cannot be empty".to_string());
                        }
                        view.config.tab_title.command_format = value.to_string();
                        config::set_root_setting(RootSettingId::TabTitleCommandFormat, value)
                    },
                )
            });

        let mut strip_group = SettingGroup::new()
            .title("TAB STRIP")
            .item(dropdown_item(
                &e2,
                "tab_close_visibility",
                vec![
                    ("active_hover".into(), "Active Hover".into()),
                    ("hover".into(), "Hover".into()),
                    ("always".into(), "Always".into()),
                ],
                |c| match c.tab_close_visibility {
                    termy_config_core::TabCloseVisibility::ActiveHover => "active_hover".into(),
                    termy_config_core::TabCloseVisibility::Hover => "hover".into(),
                    termy_config_core::TabCloseVisibility::Always => "always".into(),
                },
                RootSettingId::TabCloseVisibility,
            ))
            .item(dropdown_item(
                &e2,
                "tab_width_mode",
                vec![
                    ("stable".into(), "Stable".into()),
                    ("active_grow".into(), "Active Grow".into()),
                    ("active_grow_sticky".into(), "Active Grow Sticky".into()),
                ],
                |c| match c.tab_width_mode {
                    termy_config_core::TabWidthMode::Stable => "stable".into(),
                    termy_config_core::TabWidthMode::ActiveGrow => "active_grow".into(),
                    termy_config_core::TabWidthMode::ActiveGrowSticky => "active_grow_sticky".into(),
                },
                RootSettingId::TabWidthMode,
            ))
            .item(number_item(
                &e2,
                "vertical_tabs_width",
                NumberFieldOptions {
                    min: crate::terminal_view::tab_strip::min_expanded_vertical_tab_strip_width() as f64,
                    max: 480.0,
                    step: 10.0,
                },
                |c| c.vertical_tabs_width as f64,
                RootSettingId::VerticalTabsWidth,
            ))
            .item(switch_item(
                &e2,
                "tab_switch_modifier_hints",
                RootSettingId::TabSwitchModifierHints,
                |c| c.tab_switch_modifier_hints,
            ))
            .item(switch_item(
                &e2,
                "vertical_tabs",
                RootSettingId::VerticalTabs,
                |c| c.vertical_tabs,
            ))
            .item(switch_item(
                &e2,
                "vertical_tabs_minimized",
                RootSettingId::VerticalTabsMinimized,
                |c| c.vertical_tabs_minimized,
            ));

        #[cfg(not(target_os = "windows"))]
        {
            strip_group = strip_group
                .item(switch_item(
                    &e2,
                    "ai_features_enabled",
                    RootSettingId::AiFeaturesEnabled,
                    |c| c.ai_features_enabled,
                ));
            if self.config.ai_features_enabled {
                strip_group = strip_group.item(switch_item(
                    &e2,
                    "agent_sidebar_enabled",
                    RootSettingId::AgentSidebarEnabled,
                    |c| c.agent_sidebar_enabled,
                ));
            }
        }

        strip_group = strip_group.item(switch_item(
            &e2,
            "auto_hide_tabbar",
            RootSettingId::AutoHideTabbar,
            |c| c.auto_hide_tabbar,
        ));

        let titlebar_group = SettingGroup::new()
            .title("TITLE BAR")
            .item(switch_item(
                &e3,
                "show_termy_in_titlebar",
                RootSettingId::ShowTermyInTitlebar,
                |c| c.show_termy_in_titlebar,
            ));

        SettingPage::new("Tabs")
            .description("Configure tab behavior and titles")
            .group(title_group)
            .group(strip_group)
            .group(titlebar_group)
    }

    fn build_theme_store_page(
        &self,
        entity: &crate::gpui::Entity<Self>,
        _cx: &mut Context<Self>,
    ) -> SettingPage {
        let entity = entity.clone();
        let entity2 = entity.clone();
        let store_url = "https://termy.run/themes";

        let store_render_item = SettingItem::render({
            let entity = entity.clone();
            move |_options: &gpui_component::setting::RenderOptions, _window: &mut Window, cx: &mut App| {
                let bg_card = entity.read(cx).bg_card();
                let border_color = entity.read(cx).border_color();
                let _text_muted = entity.read(cx).text_muted();
                let text_secondary = entity.read(cx).text_secondary();
                let _accent = entity.read(cx).accent();
                let hover_bg = entity.read(cx).bg_hover();
                let store_url = store_url.to_string();

                div()
                    .py_3()
                    .px_4()
                    .bg(bg_card)
                    .border_1()
                    .border_color(border_color)
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div().text_sm().text_color(text_secondary).child(
                            "Browse and install community themes",
                        ),
                    )
                    .child(
                        div()
                            .id("theme-store-open-link")
                            .mt_1()
                            .w(px(108.0))
                            .h(px(32.0))
                            .px_2()
                            .border_1()
                            .border_color(border_color)
                            .bg(entity.read(cx).bg_input())
                            .text_sm()
                            .text_color(text_secondary)
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(move |s| s.bg(hover_bg).text_color(text_secondary))
                            .child("Open Store")
                            .on_click(move |_view, _window, _cx| {
                                if let Err(error) = SettingsWindow::open_url(&store_url) {
                                    termy_toast::error(error);
                                }
                            }),
                    )
                    .into_any_element()
            }
        });

        let _ = entity2;
        SettingPage::new("Theme Store")
            .description("Browse and install community themes")
            .resettable(false)
            .group(SettingGroup::new().title("ONLINE").item(store_render_item))
    }

    fn build_advanced_page(
        &self,
        entity: &crate::gpui::Entity<Self>,
        cx: &mut Context<Self>,
    ) -> SettingPage {
        let e = entity.clone();
        let e2 = entity.clone();
        let e3 = entity.clone();

        let startup_group = SettingGroup::new()
            .title("STARTUP")
            .item({
                let entity = e.clone();
                input_item(
                    &entity,
                    "working_dir",
                    move |c| c.working_dir.clone().unwrap_or_default().into(),
                    move |value, view| {
                        if value.is_empty() {
                            view.config.working_dir = None;
                            config::set_root_setting(RootSettingId::WorkingDir, "none")
                        } else {
                            view.config.working_dir = Some(value.to_string());
                            config::set_root_setting(RootSettingId::WorkingDir, value)
                        }
                    },
                )
            })
            .item(dropdown_item(
                &e,
                "working_dir_fallback",
                vec![
                    ("home".into(), "Home".into()),
                    ("process".into(), "Process".into()),
                ],
                |c| match c.working_dir_fallback {
                    termy_config_core::WorkingDirFallback::Home => "home".into(),
                    termy_config_core::WorkingDirFallback::Process => "process".into(),
                },
                RootSettingId::WorkingDirFallback,
            ))
            .item(switch_item(
                &e,
                "native_tab_persistence",
                RootSettingId::NativeTabPersistence,
                |c| c.native_tab_persistence,
            ))
            .item(switch_item(
                &e,
                "native_layout_autosave",
                RootSettingId::NativeLayoutAutosave,
                |c| c.native_layout_autosave,
            ))
            .item(switch_item(
                &e,
                "native_buffer_persistence",
                RootSettingId::NativeBufferPersistence,
                |c| c.native_buffer_persistence,
            ));

        let safety_group = SettingGroup::new()
            .title("SAFETY")
            .item(switch_item(
                entity,
                "warn_on_quit",
                RootSettingId::WarnOnQuit,
                |c| c.warn_on_quit,
            ))
            .item(switch_item(
                entity,
                "warn_on_quit_with_running_process",
                RootSettingId::WarnOnQuitWithRunningProcess,
                |c| c.warn_on_quit_with_running_process,
            ));

        let window_group = SettingGroup::new()
            .title("WINDOW")
            .item(number_item(
                &e2,
                "window_width",
                NumberFieldOptions {
                    min: 100.0,
                    max: 10000.0,
                    step: 10.0,
                },
                |c| c.window_width as f64,
                RootSettingId::WindowWidth,
            ))
            .item(number_item(
                &e2,
                "window_height",
                NumberFieldOptions {
                    min: 100.0,
                    max: 10000.0,
                    step: 10.0,
                },
                |c| c.window_height as f64,
                RootSettingId::WindowHeight,
            ));

        let ui_group = SettingGroup::new()
            .title("UI")
            .item(switch_item(
                &e3,
                "show_debug_overlay",
                RootSettingId::ShowDebugOverlay,
                |c| c.show_debug_overlay,
            ));

        let updates_group = SettingGroup::new()
            .title("UPDATES")
            .item(switch_item(
                &e3,
                "auto_update",
                RootSettingId::AutoUpdate,
                |c| c.auto_update,
            ));

        let entity_for_config = e3.clone();
        let config_group = SettingGroup::new()
            .title("CONFIG FILE")
            .item(SettingItem::render(
                move |_options, _window: &mut Window, cx: &mut App| {
                    let entity = entity_for_config.clone();
                    let view = entity.read(cx);
                    let bg_card = view.bg_card();
                    let border_color = view.border_color();
                    let text_muted = view.text_muted();
                    let text_secondary = view.text_secondary();
                    let accent = view.accent();
                    let accent_hover = view.accent_with_alpha(0.8);
                    let _bg_card_for_hover = bg_card;
                    let button_text = view.contrasting_text_for_fill(accent, bg_card);
                    let button_hover_text = view.contrasting_text_for_fill(accent_hover, bg_card);
                    let config_path_display = view
                        .config_path
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .filter(|p| !p.trim().is_empty())
                        .unwrap_or_else(|| "config path not set".to_string());
                    let _ = view;

                    let entity = entity_for_config.clone();
                    div()
                        .py_4()
                        .px_4()
                        .bg(bg_card)
                        .border_1()
                        .border_color(border_color)
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(
                            div().text_sm().text_color(text_muted)
                                .child("To change these settings, edit the config file:"),
                        )
                        .child(
                            div().text_xs().text_color(text_secondary)
                                .child(config_path_display),
                        )
                        .child(
                            div()
                                .id("open-config-btn")
                                .mt_2()
                                .px_4()
                                .py_2()
                                .bg(accent)
                                .text_sm()
                                .font_weight(crate::gpui::FontWeight::MEDIUM)
                                .text_color(button_text)
                                .cursor_pointer()
                                .hover(move |s| s.bg(accent_hover).text_color(button_hover_text))
                                .child("Open Config File")
                                .on_click(move |_event, _window, cx| {
                                    let _ = entity.update(cx, |_view, _cx| {
                                        if let Err(error) = crate::config::open_config_file() {
                                            log::error!(
                                                "Failed to open config file from settings: {}",
                                                error
                                            );
                                            termy_toast::error(error.to_string());
                                        }
                                    });
                                }),
                        )
                        .into_any_element()
                },
            ));

        let _ = cx;
        SettingPage::new("Advanced")
            .description("Advanced configuration options")
            .group(startup_group)
            .group(safety_group)
            .group(window_group)
            .group(ui_group)
            .group(updates_group)
            .group(config_group)
    }

    fn build_colors_page(
        &self,
        entity: &crate::gpui::Entity<Self>,
        _cx: &mut Context<Self>,
    ) -> SettingPage {
        let entity = entity.clone();
        let items: Vec<SettingItem> = termy_config_core::color_setting_specs()
            .iter()
            .map(|spec| {
                let _spec_key = spec.key;
                let spec_id = spec.id;
                let spec_title = spec.title;
                let spec_desc = spec.description;
                let entity = entity.clone();
                let entity_set = entity.clone();
                SettingItem::new(
                    spec_title,
                    SettingField::input(
                        move |cx| {
                            let view = entity.read(cx);
                            view.custom_color_for_id(spec_id)
                                .map(|rgb| format!("#{:02x}{:02x}{:02x}", rgb.r, rgb.g, rgb.b).into())
                                .unwrap_or_else(|| "Theme default".into())
                        },
                        move |value: crate::gpui::SharedString, cx| {
                            let _ = entity_set.update(cx, |view, cx| {
                                match view.apply_color_field(spec_id, &value) {
                                    Ok(()) => termy_toast::success("Saved"),
                                    Err(error) => termy_toast::error(error),
                                }
                                cx.notify();
                            });
                        },
                    ),
                )
                .description(spec_desc)
            })
            .collect();

        let mut group = SettingGroup::new().title("OVERRIDES");
        for item in items {
            group = group.item(item);
        }

        SettingPage::new("Colors")
            .description("Override individual terminal colors")
            .group(group)
    }

    fn build_keybindings_page(
        &self,
        entity: &crate::gpui::Entity<Self>,
        _cx: &mut Context<Self>,
    ) -> SettingPage {
        let entity = entity.clone();
        let actions = Self::bindable_actions();

        let keybind_render = SettingItem::render({
            let entity = entity.clone();
            move |_options, _window: &mut Window, cx: &mut App| {
                let view = entity.read(cx);
                let action_bindings =
                    Self::effective_action_bindings_from_lines(&view.config.keybind_lines);
                let capturing = view.capturing_action;
                let bg_card = view.bg_card();
                let border_color = view.border_color();
                let input_bg = view.bg_input();
                let hover_bg = view.bg_hover();
                let accent = view.accent();
                let text_primary = view.text_primary();
                let text_secondary = view.text_secondary();
                let text_muted = view.text_muted();
                let _ = view;

                let mut container = div().flex().flex_col().gap_2();
                for &action in &actions {
                    let config_name = action.config_name().to_string();
                    let action_title = Self::action_title_from_config_name(action.config_name());
                    let is_capturing_this = capturing == Some(action);
                    let binding_display = if is_capturing_this {
                        "Press shortcut...".to_string()
                    } else {
                        action_bindings
                            .get(&action)
                            .map(|trigger| Self::display_trigger_for_os(trigger))
                            .unwrap_or_else(|| "Unbound".to_string())
                    };

                    let entity_for_bind = entity.clone();
                    let entity_for_clear = entity.clone();
                    let bind_border = if is_capturing_this {
                        accent
                    } else {
                        border_color
                    };
                    let bind_text = if is_capturing_this {
                        text_primary
                    } else {
                        text_secondary
                    };
                    let row_border = if is_capturing_this {
                        accent
                    } else {
                        border_color
                    };

                    container = container.child(
                        div()
                            .id(SharedString::from(format!(
                                "keybind-row-{}",
                                config_name
                            )))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_4()
                            .py_3()
                            .px_4()
                            .rounded(px(0.0))
                            .bg(bg_card)
                            .border_1()
                            .border_color(row_border)
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(2.0))
                                    .child(
                                        div()
                                            .text_sm()
                                            .font_weight(crate::gpui::FontWeight::MEDIUM)
                                            .text_color(text_primary)
                                            .child(action_title),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(text_muted)
                                            .child(config_name),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "keybind-bind-{}",
                                                action.config_name()
                                            )))
                                            .w(px(SETTINGS_CONTROL_WIDTH))
                                            .px_3()
                                            .py_1()
                                            .rounded(px(0.0))
                                            .bg(input_bg)
                                            .border_1()
                                            .border_color(bind_border)
                                            .text_sm()
                                            .text_color(bind_text)
                                            .cursor_pointer()
                                            .hover(move |s| {
                                                s.bg(hover_bg).text_color(text_primary)
                                            })
                                            .on_click(move |_event, window, cx| {
                                                let _ = entity_for_bind.update(cx, |view, inner_cx| {
                                                    if view.capturing_action == Some(action) {
                                                        view.capturing_action = None;
                                                        inner_cx.notify();
                                                        return;
                                                    }
                                                    view.capturing_action = Some(action);
                                                    view.focus_handle.focus(window, inner_cx);
                                                    inner_cx.notify();
                                                });
                                            })
                                            .child(binding_display),
                                    )
                                    .child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "keybind-clear-{}",
                                                action.config_name()
                                            )))
                                            .px_3()
                                            .py_1()
                                            .rounded(px(0.0))
                                            .bg(input_bg)
                                            .text_sm()
                                            .font_weight(crate::gpui::FontWeight::MEDIUM)
                                            .text_color(text_secondary)
                                            .cursor_pointer()
                                            .hover(move |s| {
                                                s.bg(hover_bg).text_color(text_primary)
                                            })
                                            .child("Clear")
                                            .on_click(move |_event, _window, cx| {
                                                let _ = entity_for_clear.update(cx, |view, inner_cx| {
                                                    view.clear_action_binding(action, inner_cx);
                                                });
                                            }),
                                    ),
                            )
                            .into_any_element(),
                    );
                }

                container.into_any_element()
            }
        });

        SettingPage::new("Keybindings")
            .description("Click a shortcut box, then press a key combo")
            .resettable(false)
            .group(SettingGroup::new().title("SHORTCUTS").item(keybind_render))
    }
}
