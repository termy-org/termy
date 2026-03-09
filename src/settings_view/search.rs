use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct SettingMetadata {
    pub(super) key: &'static str,
    pub(super) section: SettingsSection,
    pub(super) title: &'static str,
    pub(super) description: &'static str,
    pub(super) keywords: &'static [&'static str],
}

#[cfg(test)]
mod tests {
    use super::{SearchableSetting, SettingMetadata, SettingsSection, SettingsWindow};
    use std::time::{Duration, Instant};

    static META_ALPHA: SettingMetadata = SettingMetadata {
        key: "alpha",
        section: SettingsSection::Terminal,
        title: "Alpha Cursor",
        description: "Primary cursor behavior",
        keywords: &["cursor", "terminal"],
    };

    static META_BETA: SettingMetadata = SettingMetadata {
        key: "beta",
        section: SettingsSection::Tabs,
        title: "Tab Width",
        description: "Tab strip width behavior",
        keywords: &["tabs", "width"],
    };

    fn searchable(metadata: &'static SettingMetadata) -> SearchableSetting {
        let title_lower = metadata.title.to_ascii_lowercase();
        let description_lower = metadata.description.to_ascii_lowercase();
        let section_lower =
            SettingsWindow::settings_section_label(metadata.section).to_ascii_lowercase();
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
    }

    #[test]
    fn setting_search_score_prioritizes_exact_title_matches() {
        let alpha = searchable(&META_ALPHA);
        let beta = searchable(&META_BETA);
        let query = "alpha cursor";
        let terms = query.split_whitespace().collect::<Vec<_>>();

        let alpha_score = SettingsWindow::setting_search_score(&alpha, query, &terms).unwrap();
        let beta_score = SettingsWindow::setting_search_score(&beta, query, &terms);

        assert!(alpha_score > 0);
        assert!(beta_score.is_none());
    }

    #[test]
    fn setting_search_score_requires_all_terms_to_match() {
        let alpha = searchable(&META_ALPHA);
        let query = "cursor tabs";
        let terms = query.split_whitespace().collect::<Vec<_>>();
        let score = SettingsWindow::setting_search_score(&alpha, query, &terms);
        assert!(score.is_none());
    }

    #[test]
    fn should_skip_search_jump_when_target_repeats() {
        let now = Instant::now();
        assert!(SettingsWindow::should_skip_search_jump(
            Some("alpha"),
            Some(now - Duration::from_millis(500)),
            "alpha",
            now
        ));
    }

    #[test]
    fn should_skip_search_jump_when_within_throttle_window() {
        let now = Instant::now();
        assert!(SettingsWindow::should_skip_search_jump(
            Some("alpha"),
            Some(now - Duration::from_millis(20)),
            "beta",
            now
        ));
    }

    #[test]
    fn should_not_skip_search_jump_for_new_target_after_throttle_window() {
        let now = Instant::now();
        assert!(!SettingsWindow::should_skip_search_jump(
            Some("alpha"),
            Some(now - Duration::from_millis(500)),
            "beta",
            now
        ));
    }
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

    entries.push(SettingMetadata {
        key: "experimental",
        section: SettingsSection::Experimental,
        title: "Experimental",
        description: "Track workspace crates and features that are still considered experimental.",
        keywords: &["experimental", "preview", "beta", "unstable", "crate"],
    });
    entries.push(SettingMetadata {
        key: "theme_store",
        section: SettingsSection::ThemeStore,
        title: "Theme Store",
        description: "Browse and install community themes from the online store.",
        keywords: &["theme", "store", "install", "colors"],
    });
    entries.push(SettingMetadata {
        key: "plugins",
        section: SettingsSection::Plugins,
        title: "Plugins",
        description: "Inspect installed plugins, permissions, startup state, and failures.",
        keywords: &["plugins", "extensions", "permissions", "autostart"],
    });

    entries
});

#[derive(Clone, Debug)]
pub(super) struct SearchableSetting {
    pub(super) metadata: &'static SettingMetadata,
    pub(super) title_lower: String,
    pub(super) description_lower: String,
    pub(super) section_lower: String,
    pub(super) keywords_lower: String,
    pub(super) haystack_lower: String,
}

impl SettingsWindow {
    fn is_plugins_section_enabled(&self) -> bool {
        self.config.show_plugins_tab
    }

    fn is_experimental_section_enabled() -> bool {
        crate::experimental::has_entries()
    }

    fn is_section_visible(
        section: SettingsSection,
        plugins_enabled: bool,
        experimental_enabled: bool,
    ) -> bool {
        match section {
            SettingsSection::Plugins => plugins_enabled,
            SettingsSection::Experimental => experimental_enabled,
            _ => true,
        }
    }

    pub(super) fn settings_section_label(section: SettingsSection) -> &'static str {
        match section {
            SettingsSection::Appearance => "Appearance",
            SettingsSection::Terminal => "Terminal",
            SettingsSection::Tabs => "Tabs",
            SettingsSection::Experimental => "Experimental",
            SettingsSection::ThemeStore => "Theme Store",
            SettingsSection::Plugins => "Plugins",
            SettingsSection::Advanced => "Advanced",
            SettingsSection::Colors => "Colors",
            SettingsSection::Keybindings => "Keybindings",
        }
    }

    pub(super) fn settings_sections_in_order(&self) -> Vec<SettingsSection> {
        let plugins_enabled = self.is_plugins_section_enabled();
        let experimental_enabled = Self::is_experimental_section_enabled();
        [
            SettingsSection::Appearance,
            SettingsSection::Terminal,
            SettingsSection::Tabs,
            SettingsSection::Experimental,
            SettingsSection::ThemeStore,
            SettingsSection::Plugins,
            SettingsSection::Advanced,
            SettingsSection::Colors,
            SettingsSection::Keybindings,
        ]
        .into_iter()
        .filter(|section| Self::is_section_visible(*section, plugins_enabled, experimental_enabled))
        .collect()
    }

    pub(super) fn set_active_section(
        &mut self,
        section: SettingsSection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let section = if Self::is_section_visible(
            section,
            self.is_plugins_section_enabled(),
            Self::is_experimental_section_enabled(),
        ) {
            section
        } else {
            SettingsSection::Appearance
        };
        self.active_section = section;
        self.active_input = None;
        self.capturing_action = None;
        self.blur_sidebar_search();
        self.theme_store_search_active = section == SettingsSection::ThemeStore;
        self.theme_store_search_selecting = false;
        self.scroll_animation_token = self.scroll_animation_token.wrapping_add(1);
        self.content_scroll_handle
            .set_offset(point(px(0.0), px(0.0)));
        self.request_scrollbar_refresh_frames(3, window, cx);
    }

    pub(super) fn cycle_active_section(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let sections = self.settings_sections_in_order();
        let current_index = sections
            .iter()
            .position(|section| *section == self.active_section)
            .unwrap_or(0);
        let next_index = if reverse {
            if current_index == 0 {
                sections.len() - 1
            } else {
                current_index - 1
            }
        } else {
            (current_index + 1) % sections.len()
        };
        self.set_active_section(sections[next_index], window, cx);
    }

    pub(super) fn build_searchable_settings(
        include_plugins: bool,
        include_experimental: bool,
    ) -> Vec<SearchableSetting> {
        SETTINGS_METADATA
            .iter()
            .filter(|metadata| {
                Self::is_section_visible(metadata.section, include_plugins, include_experimental)
            })
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

    pub(super) fn build_searchable_setting_indices(
        searchable_settings: &[SearchableSetting],
    ) -> HashMap<&'static str, usize> {
        searchable_settings
            .iter()
            .enumerate()
            .map(|(index, setting)| (setting.metadata.key, index))
            .collect()
    }

    pub(super) fn build_setting_scroll_anchors(
        content_scroll_handle: &ScrollHandle,
        include_plugins: bool,
        include_experimental: bool,
    ) -> HashMap<&'static str, ScrollAnchor> {
        SETTINGS_METADATA
            .iter()
            .filter(|setting| {
                Self::is_section_visible(setting.section, include_plugins, include_experimental)
            })
            .map(|setting| {
                (
                    setting.key,
                    ScrollAnchor::for_handle(content_scroll_handle.clone()),
                )
            })
            .collect()
    }

    pub(super) fn searchable_setting_by_key(
        &self,
        key: &'static str,
    ) -> Option<&SearchableSetting> {
        let index = self.searchable_setting_indices.get(key).copied()?;
        self.searchable_settings.get(index)
    }

    pub(super) fn setting_metadata(key: &'static str) -> Option<&'static SettingMetadata> {
        SETTINGS_METADATA.iter().find(|setting| setting.key == key)
    }

    pub(super) fn setting_search_score(
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

    pub(super) fn sidebar_search_results(&self, limit: usize) -> Vec<&SearchableSetting> {
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

    pub(super) fn setting_matches_sidebar_query(&self, setting_key: &'static str) -> bool {
        let query = self.sidebar_search_state.text().trim().to_ascii_lowercase();
        if query.is_empty() {
            return false;
        }
        let terms: Vec<&str> = query.split_whitespace().collect();
        self.searchable_setting_by_key(setting_key)
            .is_some_and(|setting| Self::setting_search_score(setting, &query, &terms).is_some())
    }

    pub(super) fn blur_sidebar_search(&mut self) {
        self.sidebar_search_active = false;
        self.sidebar_search_selecting = false;
        self.search_navigation_last_target = None;
        self.search_navigation_last_jump_at = None;
    }

    pub(super) fn start_smooth_scroll_animation(
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

    pub(super) fn jump_to_setting(
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
        self.capturing_action = None;
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

    pub(super) fn jump_to_first_search_result(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(first_key) = self
            .sidebar_search_results(1)
            .into_iter()
            .next()
            .map(|setting| setting.metadata.key)
        {
            self.jump_to_setting(first_key, window, cx);
        }
    }

    pub(super) fn should_skip_search_jump(
        last_target: Option<&'static str>,
        last_jump_at: Option<Instant>,
        next_target: &'static str,
        now: Instant,
    ) -> bool {
        if last_target == Some(next_target) {
            return true;
        }
        last_jump_at.is_some_and(|last| {
            now.duration_since(last) < Duration::from_millis(SETTINGS_SEARCH_NAV_THROTTLE_MS)
        })
    }

    pub(super) fn refresh_search_navigation(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
            if Self::should_skip_search_jump(
                self.search_navigation_last_target,
                self.search_navigation_last_jump_at,
                first_key,
                now,
            ) {
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

    pub(super) fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex_none()
            .w(px(SIDEBAR_WIDTH))
            .min_w(px(SIDEBAR_WIDTH))
            .max_w(px(SIDEBAR_WIDTH))
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
                    .when(Self::is_experimental_section_enabled(), |this| {
                        this.child(self.render_sidebar_item(
                            "Experimental",
                            SettingsSection::Experimental,
                            cx,
                        ))
                    })
                    .child(self.render_sidebar_item("Theme Store", SettingsSection::ThemeStore, cx))
                    .when(self.config.show_plugins_tab, |this| {
                        this.child(self.render_sidebar_item(
                            "Plugins",
                            SettingsSection::Plugins,
                            cx,
                        ))
                    })
                    .child(self.render_sidebar_item("Advanced", SettingsSection::Advanced, cx))
                    .child(self.render_sidebar_item("Colors", SettingsSection::Colors, cx))
                    .child(self.render_sidebar_item(
                        "Keybindings",
                        SettingsSection::Keybindings,
                        cx,
                    )),
            )
            .child(div().flex_1())
            .child(self.render_sidebar_footer(cx))
    }

    pub(super) fn render_sidebar_footer(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let text_muted = self.text_muted();
        let text_primary = self.text_primary();
        let border_color = self.border_color();
        let button_border = self.accent_with_alpha(0.35);
        let login_bg = self.bg_input();
        let hover_bg = self.bg_hover();
        let auth_status: AnyElement = match self.theme_store_auth_session.clone() {
            Some(session) => {
                let user = session.user;
                let display_name = Self::theme_store_auth_display_name(&user);
                let avatar_fallback = Self::theme_store_auth_avatar_fallback_label(&user);
                let avatar: AnyElement = match user.avatar_url.clone() {
                    Some(avatar_url) => div()
                        .w(px(30.0))
                        .h(px(30.0))
                        .rounded_full()
                        .overflow_hidden()
                        .bg(self.bg_input())
                        .child(
                            img(avatar_url)
                                .w_full()
                                .h_full()
                                .object_fit(ObjectFit::Cover),
                        )
                        .into_any_element(),
                    None => div()
                        .w(px(30.0))
                        .h(px(30.0))
                        .rounded_full()
                        .bg(self.accent_with_alpha(0.18))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .font_weight(gpui::FontWeight::BOLD)
                        .text_color(text_primary)
                        .child(avatar_fallback)
                        .into_any_element(),
                };

                div()
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(avatar)
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(text_primary)
                                            .child(display_name),
                                    )
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(text_muted)
                                            .child(format!("@{}", user.github_login)),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("theme-store-logout-button")
                            .px_3()
                            .py(px(8.0))
                            .rounded(px(0.0))
                            .border_1()
                            .border_color(button_border)
                            .bg(login_bg)
                            .cursor_pointer()
                            .hover(|s| s.bg(hover_bg))
                            .text_xs()
                            .text_color(text_primary)
                            .child(if self.theme_store_auth_loading {
                                "Signing out..."
                            } else {
                                "Logout"
                            })
                            .on_click(cx.listener(|view, _, _, cx| {
                                view.logout_theme_store_user(cx);
                            })),
                    )
                    .into_any_element()
            }
            None => div().into_any_element(),
        };

        let mut footer = div()
            .border_t_1()
            .border_color(border_color)
            .px_4()
            .py_3()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(auth_status)
            .child(
                div()
                    .text_xs()
                    .text_color(text_muted)
                    .child(format!("Termy v{}", crate::APP_VERSION)),
            );

        if let Some(error) = self.theme_store_auth_error.clone() {
            footer = footer.child(div().text_xs().text_color(self.accent()).child(error));
        }

        footer
    }

    pub(super) fn search_input_content(
        &self,
        query_text: &str,
        has_query: bool,
        is_active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text_secondary = self.text_secondary();
        let text_muted = self.text_muted();
        if is_active {
            let font = Font {
                family: self.config.font_family.clone().into(),
                ..Font::default()
            };
            return TextInputElement::new(
                cx.entity(),
                self.focus_handle.clone(),
                font,
                px(SETTINGS_INPUT_TEXT_SIZE),
                text_secondary.into(),
                self.accent_with_alpha(0.3).into(),
                TextInputAlignment::Left,
            )
            .into_any_element();
        }

        if has_query {
            div()
                .text_size(px(SETTINGS_INPUT_TEXT_SIZE))
                .text_color(text_secondary)
                .child(query_text.to_string())
                .into_any_element()
        } else {
            div()
                .text_size(px(SETTINGS_INPUT_TEXT_SIZE))
                .text_color(text_muted)
                .child("Search settings...")
                .into_any_element()
        }
    }

    pub(super) fn search_input_container(
        &self,
        search_content: AnyElement,
        is_active: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bg_input = self.bg_input();
        let border_color = self.border_color();
        let accent = self.accent();
        div()
            .id("settings-sidebar-search-input")
            .h(px(36.0))
            .px_3()
            .rounded(px(0.0))
            .bg(bg_input)
            .border_1()
            .border_color(if is_active { accent } else { border_color })
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
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn search_results_preview(
        &self,
        all_results: Vec<&SearchableSetting>,
        text_muted: Rgba,
        text_secondary: Rgba,
        bg_input: Rgba,
        border_color: Rgba,
        hover_bg: Rgba,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let total_results = all_results.len();
        let summary = if total_results == 1 {
            "1 match".to_string()
        } else {
            format!("{total_results} matches")
        };

        let mut container =
            div().child(div().px_1().text_xs().text_color(text_muted).child(summary));
        if total_results == 0 {
            return container.into_any_element();
        }

        let mut preview = div().flex().flex_col().gap(px(2.0));
        for setting in all_results.into_iter().take(SETTINGS_SEARCH_PREVIEW_LIMIT) {
            let key = setting.metadata.key;
            let section_label = Self::settings_section_label(setting.metadata.section);
            preview = preview.child(
                div()
                    .id(SharedString::from(format!("search-preview-{key}")))
                    .px_2()
                    .py_1()
                    .rounded(px(0.0))
                    .bg(bg_input)
                    .border_1()
                    .border_color(border_color)
                    .cursor_pointer()
                    .hover(move |s| s.bg(hover_bg))
                    .child(
                        div()
                            .text_xs()
                            .text_color(text_secondary)
                            .child(setting.metadata.title),
                    )
                    .child(div().text_xs().text_color(text_muted).child(section_label))
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.jump_to_setting(key, window, cx);
                    })),
            );
        }
        container = container.child(preview);
        container.into_any_element()
    }

    pub(super) fn render_sidebar_search(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let query_text = self.sidebar_search_state.text().to_string();
        let has_query = !query_text.trim().is_empty();
        let is_active = self.sidebar_search_active;
        let all_results = if has_query {
            self.sidebar_search_results(self.searchable_settings.len())
        } else {
            Vec::new()
        };
        let text_secondary = self.text_secondary();
        let text_muted = self.text_muted();
        let bg_input = self.bg_input();
        let border_color = self.border_color();
        let hover_bg = self.bg_hover();
        let search_content = self.search_input_content(&query_text, has_query, is_active, cx);
        let input = self.search_input_container(search_content, is_active, cx);

        let mut search_container = div()
            .id("settings-sidebar-search")
            .px_3()
            .pb_3()
            .child(input);
        if has_query {
            search_container = search_container.child(self.search_results_preview(
                all_results,
                text_muted,
                text_secondary,
                bg_input,
                border_color,
                hover_bg,
                cx,
            ));
        }
        search_container
    }

    pub(super) fn render_sidebar_item(
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
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _event: &MouseDownEvent, window, cx| {
                    view.set_active_section(section, window, cx);
                    cx.notify();
                }),
            )
    }
}
