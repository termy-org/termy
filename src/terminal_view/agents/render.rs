use super::*;

impl TerminalView {
    pub(in super::super) fn agent_thread_relative_age(updated_at_ms: u64) -> String {
        let now = now_unix_ms();
        let elapsed_seconds = now
            .saturating_sub(updated_at_ms)
            .checked_div(1000)
            .unwrap_or_default();

        match elapsed_seconds {
            0..=59 => "now".to_string(),
            60..=3599 => format!("{}m", elapsed_seconds / 60),
            3600..=86_399 => format!("{}h", elapsed_seconds / 3600),
            86_400..=604_799 => format!("{}d", elapsed_seconds / 86_400),
            604_800..=2_592_000 => format!("{}w", elapsed_seconds / 604_800),
            _ => format!("{}mo", elapsed_seconds / 2_592_000),
        }
    }

    pub(in super::super) fn compact_agent_thread_detail(
        status: &AgentThreadStatusPresentation,
        is_active: bool,
    ) -> Option<String> {
        match status.tone {
            AgentThreadStatusTone::Error | AgentThreadStatusTone::Warning => {
                status.detail.clone().or_else(|| Some(status.label.clone()))
            }
            AgentThreadStatusTone::Active if is_active => match status.label.as_str() {
                "thinking" => Some("Thinking".to_string()),
                "tool" => Some("Using tools".to_string()),
                "approval" => Some("Approval required".to_string()),
                "starting" => Some("Starting".to_string()),
                _ => None,
            },
            AgentThreadStatusTone::Muted => None,
            AgentThreadStatusTone::Active => None,
        }
    }

    pub(in super::super) fn render_agent_project_glyph(
        stroke: gpui::Rgba,
        bg: gpui::Rgba,
    ) -> AnyElement {
        div()
            .relative()
            .flex_none()
            .w(px(12.0))
            .h(px(10.0))
            .child(
                div()
                    .absolute()
                    .left(px(1.0))
                    .top(px(1.0))
                    .w(px(4.0))
                    .h(px(2.0))
                    .bg(bg)
                    .border_1()
                    .border_color(stroke),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(2.0))
                    .w(px(11.0))
                    .h(px(7.0))
                    .bg(bg)
                    .border_1()
                    .border_color(stroke),
            )
            .into_any_element()
    }

    pub(in super::super) fn render_agent_sidebar_avatar(
        agent: command_palette::AiAgentPreset,
        dark_surface: bool,
        border: gpui::Rgba,
        bg: gpui::Rgba,
        text: gpui::Rgba,
    ) -> AnyElement {
        let fallback_label = agent.fallback_label();
        div()
            .flex_none()
            .size(px(14.0))
            .p(px(1.0))
            .bg(bg)
            .border_1()
            .border_color(border)
            .child(
                img(Path::new(agent.image_asset_path(dark_surface)))
                    .size_full()
                    .object_fit(ObjectFit::Contain)
                    .with_fallback(move || {
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(7.0))
                            .text_color(text)
                            .child(fallback_label)
                            .into_any_element()
                    }),
            )
            .into_any_element()
    }

    pub(in super::super) fn render_agent_status_badge(
        label: &str,
        tone: AgentThreadStatusTone,
        border: gpui::Rgba,
        bg: gpui::Rgba,
        text: gpui::Rgba,
        muted: gpui::Rgba,
        warning: gpui::Rgba,
        error: gpui::Rgba,
    ) -> AnyElement {
        let badge_text = match tone {
            AgentThreadStatusTone::Active => text,
            AgentThreadStatusTone::Warning => warning,
            AgentThreadStatusTone::Error => error,
            AgentThreadStatusTone::Muted => muted,
        };

        div()
            .flex_none()
            .h(px(18.0))
            .px(px(5.0))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_size(px(10.0))
            .text_color(badge_text)
            .child(label.to_ascii_lowercase())
            .into_any_element()
    }

    pub(in super::super) fn render_agent_sidebar_chip(
        label: impl Into<SharedString>,
        border: gpui::Rgba,
        bg: gpui::Rgba,
        text: gpui::Rgba,
    ) -> AnyElement {
        let label: SharedString = label.into();
        div()
            .flex_none()
            .h(px(18.0))
            .px(px(5.0))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_size(px(10.0))
            .text_color(text)
            .child(label)
            .into_any_element()
    }

    pub(in super::super) fn render_agent_activity_dot(color: gpui::Rgba) -> AnyElement {
        div()
            .flex_none()
            .size(px(5.0))
            .rounded(px(2.5))
            .bg(color)
            .into_any_element()
    }

    pub(in super::super) fn render_agent_git_panel_entry(
        &mut self,
        entry: AgentGitPanelEntry,
        is_selected: bool,
        show_border: bool,
        preview_loading: bool,
        preview_error: Option<&str>,
        preview_diff_lines: &[String],
        preview_history: &[AgentGitHistoryEntry],
        theme: &GitPanelTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = *theme;
        let repo_path = entry.repo_path.clone();
        let row_repo_path = repo_path.clone();
        let toggle_entry = entry.clone();
        let status_color = entry.status_color(theme);

        // Checkbox: filled = fully staged, half = partially staged, empty = unstaged/untracked
        let is_fully_staged = entry.is_staged() && !entry.is_unstaged() && !entry.is_untracked();
        let is_partially_staged = entry.is_staged() && entry.is_unstaged();
        let checkbox_label = if is_fully_staged {
            "☑"
        } else if is_partially_staged {
            "☐·"
        } else {
            "☐"
        };

        // File name (last component) and directory path
        let file_name = entry
            .repo_path
            .rsplit('/')
            .next()
            .unwrap_or(entry.repo_path.as_str())
            .to_string();
        let dir_path = entry
            .repo_path
            .rsplit_once('/')
            .map(|(dir, _)| format!("{}/", dir));

        // Expanded preview when selected
        let preview = is_selected.then(|| {
            let open_repo_path = repo_path.clone();
            let diff_repo_path = repo_path.clone();
            let discard_entry = entry.clone();

            let preview_body = if preview_loading {
                div()
                    .text_size(px(11.5))
                    .text_color(t.muted)
                    .child("Loading…")
                    .into_any_element()
            } else if let Some(error) = preview_error {
                div()
                    .text_size(px(11.5))
                    .text_color(t.muted)
                    .child(error.to_string())
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    // Action bar
                    .child(
                        div()
                            .flex()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |view, _event, _window, cx| {
                                            match view
                                                .open_agent_git_file(open_repo_path.as_str())
                                            {
                                                Ok(()) => {
                                                    termy_toast::success("Opened file");
                                                    view.notify_overlay(cx);
                                                }
                                                Err(error) => {
                                                    termy_toast::error(error);
                                                    view.notify_overlay(cx);
                                                }
                                            }
                                            cx.stop_propagation();
                                        }),
                                    )
                                    .child(Self::render_agent_sidebar_chip(
                                        "open", t.border, t.input_bg, t.text,
                                    )),
                            )
                            .child(
                                div()
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |view, _event, _window, cx| {
                                            match view.open_agent_git_full_diff(
                                                diff_repo_path.as_str(),
                                                cx,
                                            ) {
                                                Ok(()) => {
                                                    termy_toast::success("Opened diff tab");
                                                    view.notify_overlay(cx);
                                                }
                                                Err(error) => {
                                                    termy_toast::error(error);
                                                    view.notify_overlay(cx);
                                                }
                                            }
                                            cx.stop_propagation();
                                        }),
                                    )
                                    .child(Self::render_agent_sidebar_chip(
                                        "diff", t.border, t.input_bg, t.text,
                                    )),
                            )
                            .children(
                                (entry.is_untracked()
                                    || entry.is_unstaged()
                                    || entry.is_deleted())
                                    .then(|| {
                                        let discard_entry = discard_entry.clone();
                                        div()
                                            .cursor_pointer()
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |view, _event, _window, cx| {
                                                    view.discard_agent_git_entry(
                                                        discard_entry.clone(),
                                                        cx,
                                                    );
                                                    cx.stop_propagation();
                                                }),
                                            )
                                            .child(Self::render_agent_sidebar_chip(
                                                "discard", t.border, t.input_bg, t.danger,
                                            ))
                                            .into_any_element()
                                    }),
                            ),
                    )
                    // Diff lines
                    .children((!preview_diff_lines.is_empty()).then(|| {
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .children(preview_diff_lines.iter().take(60).map(|line| {
                                let tone = if line.starts_with('+') && !line.starts_with("+++") {
                                    t.success
                                } else if line.starts_with('-') && !line.starts_with("---") {
                                    t.danger
                                } else if line.starts_with("@@") {
                                    t.warning
                                } else {
                                    t.muted
                                };
                                div()
                                    .truncate()
                                    .text_size(px(12.5))
                                    .text_color(tone)
                                    .child(line.clone())
                                    .into_any_element()
                            }))
                            .into_any_element()
                    }))
                    // File history
                    .children((!preview_history.is_empty()).then(|| {
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(div().text_size(px(11.0)).text_color(t.muted).child("History"))
                            .children(preview_history.iter().take(6).map(|entry| {
                                div()
                                    .truncate()
                                    .text_size(px(11.5))
                                    .text_color(t.text)
                                    .child(entry.summary.clone())
                                    .into_any_element()
                            }))
                            .into_any_element()
                    }))
                    .into_any_element()
            };

            div()
                .mx(px(8.0))
                .mb(px(4.0))
                .px(px(8.0))
                .py(px(8.0))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .border_1()
                .border_color(t.border)
                .bg(t.selected_bg)
                .child(preview_body)
                .into_any_element()
        });

        div()
            .w_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .w_full()
                    .h(px(28.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .bg(if is_selected { t.selected_bg } else { t.panel_bg })
                    .cursor_pointer()
                    .when(show_border, |this| this.border_b_1().border_color(t.border))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _event, _window, cx| {
                            view.select_agent_git_panel_entry(row_repo_path.as_str(), cx);
                            cx.stop_propagation();
                        }),
                    )
                    // Checkbox
                    .child(
                        div()
                            .flex_none()
                            .w(px(16.0))
                            .text_size(px(12.0))
                            .text_color(if is_fully_staged { t.success } else { t.muted })
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |view, _event, _window, cx| {
                                    view.toggle_stage_agent_git_entry(&toggle_entry, cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .child(checkbox_label),
                    )
                    // Badge
                    .child(Self::render_agent_sidebar_chip(
                        entry.badge_label(),
                        t.border,
                        t.input_bg,
                        status_color,
                    ))
                    // File name + dir
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(12.5))
                                    .text_color(t.text)
                                    .child(file_name),
                            )
                            .children(dir_path.map(|dir| {
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_size(px(11.0))
                                    .text_color(t.muted)
                                    .child(dir)
                            })),
                    ),
            )
            .children(preview)
            .into_any_element()
    }

    pub(in super::super) fn render_agent_git_panel_section(
        &mut self,
        title: &str,
        entries: Vec<AgentGitPanelEntry>,
        selected_repo_path: Option<&str>,
        preview_loading: bool,
        preview_error: Option<&str>,
        preview_diff_lines: &[String],
        preview_history: &[AgentGitHistoryEntry],
        theme: &GitPanelTheme,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if entries.is_empty() {
            return None;
        }

        let t = *theme;
        let count = entries.len();
        Some(
            div()
                .px(px(8.0))
                .pb(px(4.0))
                .flex()
                .flex_col()
                .gap(px(2.0))
                .child(
                    div()
                        .px(px(2.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(t.muted)
                                .child(title.to_string()),
                        )
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(t.muted)
                                .child(count.to_string()),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .border_1()
                        .border_color(t.border)
                        .bg(t.panel_bg)
                        .children(entries.into_iter().enumerate().map(|(index, entry)| {
                            self.render_agent_git_panel_entry(
                                entry.clone(),
                                selected_repo_path == Some(entry.repo_path.as_str()),
                                index + 1 < count,
                                preview_loading,
                                preview_error,
                                preview_diff_lines,
                                preview_history,
                                &t,
                                cx,
                            )
                        })),
                )
                .into_any_element(),
        )
    }

    pub(in super::super) fn render_agent_git_panel_footer(
        &mut self,
        input_mode: Option<AgentGitPanelInputMode>,
        input_text: &str,
        theme: &GitPanelTheme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = *theme;
        let active_mode = input_mode.unwrap_or(AgentGitPanelInputMode::Commit);
        let has_value = !input_text.trim().is_empty();
        let show_cancel = input_mode.is_some() || has_value;
        let just_committed = self.agent_git_panel.just_committed;
        let last_commit = self.agent_git_panel.last_commit.clone();

        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(8.0))
            .py(px(8.0))
            .border_t_1()
            .border_color(t.border)
            .bg(t.panel_bg)
            // Uncommit bar (shown after a commit, like Zed)
            .children((just_committed && last_commit.is_some()).then(|| {
                let commit_msg = last_commit.unwrap_or_default();
                div()
                    .w_full()
                    .px(px(6.0))
                    .py(px(4.0))
                    .mb(px(2.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(6.0))
                    .border_1()
                    .border_color(t.border)
                    .bg(t.input_bg)
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(t.muted)
                            .child(commit_msg),
                    )
                    .child(
                        div()
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|view, _event, _window, cx| {
                                    view.uncommit_agent_git_panel(cx);
                                    cx.stop_propagation();
                                }),
                            )
                            .child(Self::render_agent_sidebar_chip(
                                "uncommit", t.border, t.panel_bg, t.warning,
                            )),
                    )
                    .into_any_element()
            }))
            // Mode label
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(t.muted)
                    .child(active_mode.title()),
            )
            // Input area
            .child({
                let line_count = if active_mode == AgentGitPanelInputMode::Commit {
                    input_text.lines().count().max(1)
                } else {
                    1
                };
                let input_height = px(8.0 + line_count as f32 * 20.0_f32);
                div()
                    .relative()
                    .min_h(px(32.0))
                    .h(input_height)
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .border_1()
                    .border_color(t.border)
                    .bg(t.input_bg)
                    .cursor_text()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _event, _window, cx| {
                            if view.agent_git_panel_input_mode != Some(active_mode) {
                                view.begin_agent_git_panel_input(active_mode, String::new(), cx);
                            }
                            cx.stop_propagation();
                        }),
                    )
                    .children((!has_value).then(|| {
                        div()
                            .truncate()
                            .text_size(px(12.5))
                            .text_color(t.muted)
                            .child(active_mode.placeholder())
                            .into_any_element()
                    }))
                    .children((input_mode == Some(active_mode)).then(|| {
                        self.render_inline_input_layer(
                            Font {
                                family: self.font_family.clone(),
                                weight: FontWeight::NORMAL,
                                ..Default::default()
                            },
                            px(11.0),
                            t.text.into(),
                            t.selected_bg.into(),
                            InlineInputAlignment::Left,
                            cx,
                        )
                        .into_any_element()
                    }))
                    .children((input_mode != Some(active_mode) && has_value).then(|| {
                        if active_mode == AgentGitPanelInputMode::Commit {
                            div()
                                .whitespace_normal()
                                .text_size(px(12.5))
                                .text_color(t.text)
                                .child(input_text.to_string())
                                .into_any_element()
                        } else {
                            div()
                                .truncate()
                                .text_size(px(12.5))
                                .text_color(t.text)
                                .child(input_text.to_string())
                                .into_any_element()
                        }
                    }))
            })
            // Action buttons
            .child(
                div()
                    .flex()
                    .justify_between()
                    .gap(px(6.0))
                    .child(
                        div().flex().gap(px(4.0)).children(
                            [
                                (AgentGitPanelInputMode::Commit, "commit", t.text),
                                (AgentGitPanelInputMode::SaveStash, "stash", t.muted),
                            ]
                            .into_iter()
                            .map(|(mode, label, color)| {
                                let is_active = active_mode == mode;
                                div()
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |view, _event, _window, cx| {
                                            let initial =
                                                if view.agent_git_panel_input_mode == Some(mode) {
                                                    view.agent_git_panel_input.text().to_string()
                                                } else {
                                                    String::new()
                                                };
                                            view.begin_agent_git_panel_input(mode, initial, cx);
                                            cx.stop_propagation();
                                        }),
                                    )
                                    .child(Self::render_agent_sidebar_chip(
                                        label,
                                        t.border,
                                        if is_active { t.selected_bg } else { t.input_bg },
                                        if is_active { t.text } else { color },
                                    ))
                                    .into_any_element()
                            }),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|view, _event, _window, cx| {
                                            if view.agent_git_panel_input_mode.is_some() {
                                                view.commit_agent_git_panel_input(cx);
                                            } else {
                                                view.begin_agent_git_panel_input(
                                                    AgentGitPanelInputMode::Commit,
                                                    String::new(),
                                                    cx,
                                                );
                                            }
                                            cx.stop_propagation();
                                        }),
                                    )
                                    .child(Self::render_agent_sidebar_chip(
                                        active_mode.action_label(),
                                        t.border,
                                        t.input_bg,
                                        t.text,
                                    )),
                            )
                            .children(show_cancel.then(|| {
                                div()
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(|view, _event, _window, cx| {
                                            view.cancel_agent_git_panel_input(cx);
                                            cx.stop_propagation();
                                        }),
                                    )
                                    .child(Self::render_agent_sidebar_chip(
                                        "cancel", t.border, t.input_bg, t.muted,
                                    ))
                                    .into_any_element()
                            })),
                    ),
            )
            .into_any_element()
    }

    pub(in super::super) fn render_agent_git_panel(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.agent_git_panel.open {
            return None;
        }

        let t = self.git_panel_theme();

        let entries = &self.agent_git_panel.entries;
        let tracked_entries = entries
            .iter()
            .filter(|e| !e.is_untracked())
            .cloned()
            .collect::<Vec<_>>();
        let untracked_entries = entries
            .iter()
            .filter(|e| e.is_untracked())
            .cloned()
            .collect::<Vec<_>>();
        let repo_root = self.agent_git_panel.repo_root.clone();
        let repo_name = repo_root
            .as_deref()
            .and_then(|p| Path::new(p).file_name())
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Repository".to_string());
        let current_branch = self.agent_git_panel.current_branch.clone();
        let current_branch_for_branches = current_branch.clone();
        let ahead = self.agent_git_panel.ahead;
        let behind = self.agent_git_panel.behind;
        let loading = self.agent_git_panel.loading;
        let error = self.agent_git_panel.error.clone();
        let selected_repo_path = self.agent_git_panel.selected_repo_path.clone();
        let preview_loading = self.agent_git_panel.preview_loading;
        let preview_error = self.agent_git_panel.preview_error.clone();
        let preview_diff_lines = self.agent_git_panel.preview_diff_lines.clone();
        let preview_history = self.agent_git_panel.preview_history.clone();
        let branches = self.agent_git_panel.branches.clone();
        let stashes = self.agent_git_panel.stashes.clone();
        let input_mode = self.agent_git_panel_input_mode;
        let input_text = self.agent_git_panel_input.text().to_string();
        let branch_dropdown_open = self.agent_git_panel_branch_dropdown_open;

        let total_changes = self.agent_git_panel.entries.len();
        let all_staged = total_changes > 0
            && self
                .agent_git_panel
                .entries
                .iter()
                .all(AgentGitPanelEntry::is_staged);

        let body = if loading {
            div()
                .px(px(10.0))
                .py(px(12.0))
                .text_size(px(12.5))
                .text_color(t.muted)
                .child("Loading…")
                .into_any_element()
        } else if let Some(error) = error {
            div()
                .px(px(10.0))
                .py(px(12.0))
                .text_size(px(12.5))
                .text_color(t.muted)
                .child(error)
                .into_any_element()
        } else {
            let mut sections = Vec::new();
            if tracked_entries.is_empty() && untracked_entries.is_empty() {
                sections.push(
                    div()
                        .px(px(10.0))
                        .py(px(10.0))
                        .text_size(px(12.5))
                        .text_color(t.muted)
                        .child("Working tree clean")
                        .into_any_element(),
                );
            }
            if let Some(section) = self.render_agent_git_panel_section(
                "Changes",
                tracked_entries,
                selected_repo_path.as_deref(),
                preview_loading,
                preview_error.as_deref(),
                &preview_diff_lines,
                &preview_history,
                &t,
                cx,
            ) {
                sections.push(section);
            }
            if let Some(section) = self.render_agent_git_panel_section(
                "Untracked",
                untracked_entries,
                selected_repo_path.as_deref(),
                preview_loading,
                preview_error.as_deref(),
                &preview_diff_lines,
                &preview_history,
                &t,
                cx,
            ) {
                sections.push(section);
            }
            // Stashes
            if !stashes.is_empty() {
                sections.push(
                    div()
                        .px(px(8.0))
                        .pb(px(4.0))
                        .flex()
                        .flex_col()
                        .gap(px(2.0))
                        .child(
                            div()
                                .px(px(2.0))
                                .text_size(px(11.0))
                                .text_color(t.muted)
                                .child("Stashes"),
                        )
                        .children(stashes.into_iter().take(5).map(|stash| {
                            let apply_name = stash.name.clone();
                            let pop_name = stash.name.clone();
                            div()
                                .px(px(2.0))
                                .h(px(24.0))
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .truncate()
                                        .text_size(px(11.5))
                                        .text_color(t.text)
                                        .child(format!("{} {}", stash.name, stash.summary)),
                                )
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |view, _event, _window, cx| {
                                                view.run_agent_git_mutation(
                                                    vec![
                                                        "stash".to_string(),
                                                        "apply".to_string(),
                                                        apply_name.clone(),
                                                    ],
                                                    "Applied stash",
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child(Self::render_agent_sidebar_chip(
                                            "apply", t.border, t.input_bg, t.text,
                                        )),
                                )
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |view, _event, _window, cx| {
                                                view.run_agent_git_mutation(
                                                    vec![
                                                        "stash".to_string(),
                                                        "pop".to_string(),
                                                        pop_name.clone(),
                                                    ],
                                                    "Popped stash",
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child(Self::render_agent_sidebar_chip(
                                            "pop", t.border, t.input_bg, t.warning,
                                        )),
                                )
                                .into_any_element()
                        }))
                        .into_any_element(),
                );
            }

            div()
                .w_full()
                .flex()
                .flex_col()
                .children(sections)
                .into_any_element()
        };

        let panel_width = self.agent_git_panel_width;

        Some(
            div()
                .id("agent-git-panel")
                .relative()
                .w(px(panel_width))
                .h_full()
                .flex_none()
                .flex()
                .flex_col()
                .bg(t.panel_bg)
                .border_l_1()
                .border_color(t.border)
                // Resize handle
                .child(
                    div()
                        .id("agent-git-panel-resize-handle")
                        .absolute()
                        .left(px(-4.0))
                        .top_0()
                        .bottom_0()
                        .w(px(8.0))
                        .cursor_col_resize()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _event: &MouseDownEvent, _window, cx| {
                                view.agent_git_panel_resize_drag =
                                    Some(AgentGitPanelResizeDragState);
                                cx.stop_propagation();
                            }),
                        ),
                )
                // Header: repo name + action buttons (Zed-style)
                .child(
                    div()
                        .px(px(8.0))
                        .h(px(AGENT_SIDEBAR_HEADER_HEIGHT))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap(px(6.0))
                        .border_b_1()
                        .border_color(t.border)
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .truncate()
                                .text_size(px(12.0))
                                .text_color(t.text)
                                .child(repo_name),
                        )
                        .child(
                            div()
                                .flex()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|view, _event, _window, cx| {
                                                view.fetch_agent_git_panel(cx);
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child(Self::render_agent_sidebar_chip(
                                            "fetch", t.border, t.input_bg, t.muted,
                                        )),
                                )
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|view, _event, _window, cx| {
                                                view.pull_agent_git_panel(cx);
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child(Self::render_agent_sidebar_chip(
                                            "pull", t.border, t.input_bg, t.info,
                                        )),
                                )
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|view, _event, _window, cx| {
                                                view.run_agent_git_mutation(
                                                    vec!["push".to_string()],
                                                    "Pushed to remote",
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child(Self::render_agent_sidebar_chip(
                                            "push", t.border, t.input_bg, t.info,
                                        )),
                                )
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|view, _event, _window, cx| {
                                                view.close_agent_git_panel();
                                                cx.notify();
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child(Self::render_agent_sidebar_chip(
                                            "×", t.border, t.input_bg, t.muted,
                                        )),
                                ),
                        ),
                )
                // Branch bar + ahead/behind + stage all
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(6.0))
                        .flex_none()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .border_b_1()
                        .border_color(t.border)
                        // Branch row
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .children(current_branch.map(|branch| {
                                    div()
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |view, _event, _window, cx| {
                                                view.agent_git_panel_branch_dropdown_open =
                                                    !view.agent_git_panel_branch_dropdown_open;
                                                cx.notify();
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child(Self::render_agent_sidebar_chip(
                                            branch,
                                            t.border,
                                            if branch_dropdown_open {
                                                t.selected_bg
                                            } else {
                                                t.input_bg
                                            },
                                            if branch_dropdown_open { t.text } else { t.muted },
                                        ))
                                        .into_any_element()
                                }))
                                // Ahead/behind indicators
                                .children((ahead > 0).then(|| {
                                    Self::render_agent_sidebar_chip(
                                        format!("↑{ahead}"),
                                        t.border,
                                        t.input_bg,
                                        t.success,
                                    )
                                }))
                                .children((behind > 0).then(|| {
                                    Self::render_agent_sidebar_chip(
                                        format!("↓{behind}"),
                                        t.border,
                                        t.input_bg,
                                        t.warning,
                                    )
                                }))
                                .child(div().flex_1())
                                // Stage all / unstage all
                                .children((total_changes > 0).then(|| {
                                    div()
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |view, _event, _window, cx| {
                                                if all_staged {
                                                    view.run_agent_git_mutation(
                                                        vec![
                                                            "restore".to_string(),
                                                            "--staged".to_string(),
                                                            ".".to_string(),
                                                        ],
                                                        "Unstaged all",
                                                        cx,
                                                    );
                                                } else {
                                                    view.run_agent_git_mutation(
                                                        vec![
                                                            "add".to_string(),
                                                            "-A".to_string(),
                                                        ],
                                                        "Staged all",
                                                        cx,
                                                    );
                                                }
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child(Self::render_agent_sidebar_chip(
                                            if all_staged {
                                                "unstage all"
                                            } else {
                                                "stage all"
                                            },
                                            t.border,
                                            t.input_bg,
                                            if all_staged { t.warning } else { t.success },
                                        ))
                                        .into_any_element()
                                })),
                        )
                        // Branch dropdown
                        .children(branch_dropdown_open.then(|| {
                            div()
                                .flex()
                                .flex_col()
                                .gap(px(4.0))
                                .pt(px(2.0))
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |view, _event, _window, cx| {
                                                view.agent_git_panel_branch_dropdown_open = false;
                                                view.begin_agent_git_panel_input(
                                                    AgentGitPanelInputMode::CreateBranch,
                                                    String::new(),
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child(Self::render_agent_sidebar_chip(
                                            "new branch",
                                            t.border,
                                            t.input_bg,
                                            t.info,
                                        )),
                                )
                                .children(branches.into_iter().map(|branch_name| {
                                    let is_current = current_branch_for_branches.as_deref()
                                        == Some(branch_name.as_str());
                                    if is_current {
                                        Self::render_agent_sidebar_chip(
                                            branch_name,
                                            t.border,
                                            t.selected_bg,
                                            t.text,
                                        )
                                        .into_any_element()
                                    } else {
                                        let checkout_branch = branch_name.clone();
                                        div()
                                            .cursor_pointer()
                                            .on_mouse_down(
                                                MouseButton::Left,
                                                cx.listener(move |view, _event, _window, cx| {
                                                    view.agent_git_panel_branch_dropdown_open =
                                                        false;
                                                    view.run_agent_git_mutation(
                                                        vec![
                                                            "checkout".to_string(),
                                                            checkout_branch.clone(),
                                                        ],
                                                        "Switched branch",
                                                        cx,
                                                    );
                                                    cx.stop_propagation();
                                                }),
                                            )
                                            .child(Self::render_agent_sidebar_chip(
                                                branch_name,
                                                t.border,
                                                t.input_bg,
                                                t.muted,
                                            ))
                                            .into_any_element()
                                    }
                                }))
                                .into_any_element()
                        })),
                )
                // Scrollable file list
                .child(
                    div()
                        .id("agent-git-panel-scroll")
                        .flex_1()
                        .overflow_y_scroll()
                        .child(div().w_full().py(px(6.0)).flex().flex_col().child(body)),
                )
                // Footer: commit editor
                .child(self.render_agent_git_panel_footer(
                    input_mode,
                    input_text.as_str(),
                    &t,
                    cx,
                ))
                .into_any_element(),
        )
    }

    pub(in super::super) fn render_agent_sidebar(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.should_render_agent_sidebar() {
            return None;
        }

        self.update_agent_session_ids();

        let overlay_style = self.overlay_style();
        let panel_bg = overlay_style.chrome_panel_background_with_floor(0.96, 0.88);
        let input_bg = overlay_style.chrome_panel_background_with_floor(0.74, 0.72);
        let transparent = overlay_style.transparent_background();
        let text = overlay_style.panel_foreground(0.94);
        let muted = overlay_style.panel_foreground(0.62);
        let border = resolve_chrome_stroke_color(
            panel_bg,
            self.colors.foreground,
            self.chrome_contrast_profile().stroke_mix,
        );
        let selected_bg = overlay_style.panel_cursor(0.10);
        let button_hover_bg = overlay_style.chrome_panel_cursor(0.14);
        let mut tooltip_bg = overlay_style.chrome_panel_background_with_floor(0.99, 0.94);
        tooltip_bg.a = 1.0;
        let tooltip_border = resolve_chrome_stroke_color(
            tooltip_bg,
            self.colors.foreground,
            self.chrome_contrast_profile().stroke_mix,
        );
        let tooltip_text = overlay_style.panel_foreground(0.98);
        let tooltip_muted = overlay_style.panel_foreground(0.74);
        let dark_surface = command_palette::AiAgentPreset::prefers_light_asset_variant(panel_bg);
        let active_thread_id = self
            .tabs
            .get(self.active_tab)
            .and_then(|tab| tab.agent_thread_id.as_deref())
            .map(str::to_string);
        let search_query = self.agent_sidebar_search_input.text().trim().to_string();
        let show_filtered_history = !search_query.is_empty();
        let filtered_projects = self.filtered_agent_projects_for_sidebar();
        let project_groups = filtered_projects
            .into_iter()
            .enumerate()
            .map(|(index, (project, project_threads))| {
                let project_id = project.id.clone();
                let project_context_menu_id = project.id.clone();
                let is_project_active =
                    self.active_agent_project_id.as_deref() == Some(project_id.as_str());
                let is_project_pinned = project.pinned;
                let is_renaming_project =
                    self.renaming_agent_project_id.as_deref() == Some(project_id.as_str());
                let allow_collapse_toggle = !show_filtered_history;
                let is_collapsed = allow_collapse_toggle
                    && self
                        .collapsed_agent_project_ids
                        .contains(project_id.as_str());

                let project_row = div()
                    .id(SharedString::from(format!("agent-project-{}", project.id)))
                    .w_full()
                    .h(px(AGENT_SIDEBAR_PROJECT_ROW_HEIGHT))
                    .px(px(10.0))
                    .mt(px(if index == 0 { 4.0 } else { 8.0 }))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _event, _window, cx| {
                            let was_active = view.active_agent_project_id.as_deref()
                                == Some(project_id.as_str());
                            if view.renaming_agent_project_id.as_deref()
                                != Some(project_id.as_str())
                            {
                                view.cancel_rename_agent_project(cx);
                            }
                            if view.renaming_agent_thread_id.is_some() {
                                view.cancel_rename_agent_thread(cx);
                            }
                            view.active_agent_project_id = Some(project_id.clone());
                            if allow_collapse_toggle && was_active {
                                view.toggle_agent_project_collapsed(project_id.as_str(), cx);
                            } else {
                                view.collapsed_agent_project_ids.remove(project_id.as_str());
                                view.sync_persisted_agent_workspace();
                                cx.notify();
                            }
                        }),
                    )
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |view, _event, _window, cx| {
                            view.schedule_agent_project_context_menu(
                                project_context_menu_id.clone(),
                                cx,
                            );
                            cx.stop_propagation();
                        }),
                    )
                    .child(
                        div()
                            .w(px(8.0))
                            .flex_none()
                            .text_size(px(8.0))
                            .text_color(muted)
                            .child(if is_collapsed { ">" } else { "v" }),
                    )
                    .child(Self::render_agent_project_glyph(
                        if is_project_active { text } else { muted },
                        panel_bg,
                    ))
                    .child(div().flex_1().min_w(px(0.0)).relative().h(px(16.0)).child(
                        if is_renaming_project {
                            self.render_inline_input_layer(
                                Font {
                                    family: self.font_family.clone(),
                                    weight: FontWeight::NORMAL,
                                    ..Default::default()
                                },
                                px(11.5),
                                text.into(),
                                selected_bg.into(),
                                InlineInputAlignment::Left,
                                cx,
                            )
                        } else {
                            div()
                                .truncate()
                                .text_size(px(11.5))
                                .text_color(if is_project_active { text } else { muted })
                                .child(project.name.clone())
                                .into_any_element()
                        },
                    ))
                    .children(is_project_pinned.then(|| {
                        Self::render_agent_sidebar_chip(
                            "pin",
                            border,
                            input_bg,
                            if is_project_active { text } else { muted },
                        )
                    }))
                    .into_any_element();

                let thread_rows = (show_filtered_history
                    || !is_collapsed)
                    .then_some(project_threads)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|thread| {
                        let thread_id = thread.id.clone();
                        let thread_context_menu_id = thread.id.clone();
                        let is_renaming_thread =
                            self.renaming_agent_thread_id.as_deref() == Some(thread_id.as_str());
                        let status = self.agent_thread_status_presentation(thread);
                        let is_active = active_thread_id.as_deref() == Some(thread_id.as_str());
                        let is_thread_pinned = thread.pinned;
                        let shows_activity = self.agent_thread_shows_activity(thread, is_active);
                        let title = self.agent_thread_display_title(thread);
                        let age = Self::agent_thread_relative_age(thread.updated_at_ms);
                        let detail = (!is_renaming_thread)
                            .then(|| {
                                Self::compact_agent_thread_detail(&status, is_active)
                                    .or_else(|| status.detail.clone())
                            })
                            .flatten();
                        let linked_tab_id = thread.linked_tab_id;

                        div()
                            .id(SharedString::from(format!("agent-thread-{}", thread.id)))
                            .w_full()
                            .px(px(10.0))
                            .py(px(if detail.is_some() || is_renaming_thread {
                                3.0
                            } else {
                                4.0
                            }))
                            .rounded(px(0.0))
                            .bg(if is_active || is_renaming_thread {
                                selected_bg
                            } else {
                                transparent
                            })
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |view, _event, _window, cx| {
                                    if view.renaming_agent_project_id.is_some() {
                                        view.cancel_rename_agent_project(cx);
                                    }
                                    view.agent_sidebar_search_active = false;
                                    if let Some(tab_index) = linked_tab_id
                                        .and_then(|tab_id| view.tab_index_by_id(tab_id))
                                    {
                                        view.switch_tab(tab_index, cx);
                                    } else if let Err(error) =
                                        view.resume_saved_agent_thread(thread_id.as_str(), cx)
                                    {
                                        termy_toast::error(error);
                                        view.notify_overlay(cx);
                                    }
                                    if view.renaming_agent_thread_id.as_deref()
                                        != Some(thread_id.as_str())
                                    {
                                        view.cancel_rename_agent_thread(cx);
                                    }
                                    cx.stop_propagation();
                                }),
                            )
                            .on_mouse_down(
                                MouseButton::Right,
                                cx.listener(move |view, _event, _window, cx| {
                                    view.schedule_agent_thread_context_menu(
                                        thread_context_menu_id.clone(),
                                        cx,
                                    );
                                    cx.stop_propagation();
                                }),
                            )
                            .child(
                                div()
                                    .w_full()
                                    .flex()
                                    .justify_between()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .flex()
                                            .gap(px(6.0))
                                            .child(Self::render_agent_sidebar_avatar(
                                                thread.agent,
                                                dark_surface,
                                                border,
                                                input_bg,
                                                muted,
                                            ))
                                            .child(
                                                div()
                                                    .flex_1()
                                                    .min_w(px(0.0))
                                                    .flex()
                                                    .flex_col()
                                                    .gap(px(1.0))
                                                    .child(div().relative().h(px(15.0)).child(
                                                        if is_renaming_thread {
                                                            self.render_inline_input_layer(
                                                                Font {
                                                                    family: self
                                                                        .font_family
                                                                        .clone(),
                                                                    weight: FontWeight::NORMAL,
                                                                    ..Default::default()
                                                                },
                                                                px(12.0),
                                                                text.into(),
                                                                selected_bg.into(),
                                                                InlineInputAlignment::Left,
                                                                cx,
                                                            )
                                                        } else {
                                                            div()
                                                                .truncate()
                                                                .text_size(px(12.0))
                                                                .text_color(text)
                                                                .child(title)
                                                                .into_any_element()
                                                        },
                                                    ))
                                                    .child(
                                                        div()
                                                            .flex()
                                                            .items_center()
                                                            .gap(px(4.0))
                                                            .children(shows_activity.then(|| {
                                                                Self::render_agent_activity_dot(
                                                                    self.colors.ansi[11],
                                                                )
                                                            }))
                                                            .child(Self::render_agent_status_badge(
                                                                status.label.as_str(),
                                                                status.tone,
                                                                border,
                                                                input_bg,
                                                                text,
                                                                muted,
                                                                self.colors.ansi[11],
                                                                self.colors.ansi[9],
                                                            ))
                                                            .children(is_thread_pinned.then(|| {
                                                                Self::render_agent_sidebar_chip(
                                                                    "pin", border, input_bg, muted,
                                                                )
                                                            }))
                                                            .children(detail.map(|detail| {
                                                                div()
                                                                    .flex_1()
                                                                    .min_w(px(0.0))
                                                                    .truncate()
                                                                    .text_size(px(11.0))
                                                                    .text_color(muted)
                                                                    .child(detail)
                                                            })),
                                                    ),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex_none()
                                            .text_size(px(11.0))
                                            .text_color(muted)
                                            .child(age),
                                    ),
                            )
                            .into_any_element()
                    })
                    .collect::<Vec<_>>();

                div()
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(project_row)
                    .children(thread_rows)
                    .into_any_element()
            })
            .collect::<Vec<_>>();

        let empty_state = project_groups.is_empty().then(|| {
            let message = if show_filtered_history {
                format!("No history matches \"{}\".", search_query)
            } else {
                "No threads yet. Start an agent to create a project.".to_string()
            };
            div()
                .px(px(10.0))
                .py(px(8.0))
                .text_size(px(12.5))
                .text_color(muted)
                .child(message)
                .into_any_element()
        });

        Some(
            div()
                .id("agent-sidebar")
                .relative()
                .w(px(self.agent_sidebar_width))
                .h_full()
                .flex_none()
                .flex()
                .flex_col()
                .bg(panel_bg)
                .border_r_1()
                .border_color(border)
                .child(
                    div()
                        .id("agent-sidebar-resize-handle")
                        .absolute()
                        .right(px(-4.0))
                        .top_0()
                        .bottom_0()
                        .w(px(8.0))
                        .cursor_col_resize()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|view, _event: &MouseDownEvent, _window, cx| {
                                view.agent_sidebar_resize_drag =
                                    Some(AgentSidebarResizeDragState);
                                cx.stop_propagation();
                            }),
                        ),
                )
                .child(
                    div()
                        .h(px(AGENT_SIDEBAR_HEADER_HEIGHT))
                        .px(px(10.0))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(12.0))
                                .text_color(muted)
                                .child("Threads"),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .id("agent-sidebar-new-thread")
                                        .w(px(20.0))
                                        .h(px(18.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(move |style| style.bg(button_hover_bg))
                                        .tooltip(move |_window, cx| {
                                            cx.new(|_| {
                                                AgentSidebarTooltip::new(
                                                    "New thread",
                                                    "Open the agent picker and start a new thread.",
                                                    tooltip_bg,
                                                    tooltip_border,
                                                    tooltip_text,
                                                    tooltip_muted,
                                                )
                                            })
                                            .into()
                                        })
                                        .text_size(px(16.0))
                                        .text_color(muted)
                                        .child("+")
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|view, _event, _window, cx| {
                                                view.open_command_palette_in_mode(
                                                    command_palette::CommandPaletteMode::AgentProjects,
                                                    cx,
                                                );
                                                cx.stop_propagation();
                                            }),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("agent-sidebar-search-toggle")
                                        .w(px(20.0))
                                        .h(px(18.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(move |style| style.bg(button_hover_bg))
                                        .tooltip(move |_window, cx| {
                                            cx.new(|_| {
                                                AgentSidebarTooltip::new(
                                                    "Search",
                                                    "Toggle the search bar.",
                                                    tooltip_bg,
                                                    tooltip_border,
                                                    tooltip_text,
                                                    tooltip_muted,
                                                )
                                            })
                                            .into()
                                        })
                                        .text_size(px(16.0))
                                        .text_color(muted)
                                        .child("/")
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|view, _event, _window, cx| {
                                                if view.agent_sidebar_search_active {
                                                    view.agent_sidebar_search_active = false;
                                                    view.agent_sidebar_search_input.clear();
                                                    cx.notify();
                                                } else {
                                                    view.begin_agent_sidebar_search(cx);
                                                }
                                                cx.stop_propagation();
                                            }),
                                        ),
                                )
                                .child(
                                    div()
                                        .id("agent-sidebar-hide")
                                        .w(px(20.0))
                                        .h(px(18.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .cursor_pointer()
                                        .hover(move |style| style.bg(button_hover_bg))
                                        .tooltip(move |_window, cx| {
                                            cx.new(|_| {
                                                AgentSidebarTooltip::new(
                                                    "Hide Threads",
                                                    "Close the Threads sidebar.",
                                                    tooltip_bg,
                                                    tooltip_border,
                                                    tooltip_text,
                                                    tooltip_muted,
                                                )
                                            })
                                            .into()
                                        })
                                        .text_size(px(16.0))
                                        .text_color(muted)
                                        .child("☰")
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|_view, _event, _window, cx| {
                                                cx.stop_propagation();
                                                cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                                                    let confirmed = smol::unblock(|| {
                                                        termy_native_sdk::confirm(
                                                            "Hide Threads",
                                                            "Are you sure you want to hide the Threads sidebar?",
                                                        )
                                                    })
                                                    .await;
                                                    if !confirmed {
                                                        return;
                                                    }
                                                    let _ = cx.update(|cx| {
                                                        this.update(cx, |view, cx| {
                                                            view.agent_sidebar_open = false;
                                                            view.agent_sidebar_search_active = false;
                                                            view.cancel_rename_agent_project(cx);
                                                            view.cancel_rename_agent_thread(cx);
                                                            view.hovered_agent_thread_id = None;
                                                            view.close_agent_git_panel();
                                                            view.sync_persisted_agent_workspace();
                                                            cx.notify();
                                                        })
                                                    });
                                                })
                                                .detach();
                                            }),
                                        ),
                                ),
                        ),
                )
                .children(self.agent_sidebar_search_active.then(|| {
                    div()
                        .h(px(AGENT_SIDEBAR_SEARCH_HEIGHT))
                        .px(px(10.0))
                        .pb(px(4.0))
                        .flex_none()
                        .child(
                            div()
                                .id("agent-sidebar-search")
                                .relative()
                                .w_full()
                                .h_full()
                                .px(px(8.0))
                                .flex()
                                .items_center()
                                .border_1()
                                .border_color(border)
                                .bg(selected_bg)
                                .cursor(gpui::CursorStyle::IBeam)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|view, _event, _window, cx| {
                                        view.begin_agent_sidebar_search(cx);
                                        cx.stop_propagation();
                                    }),
                                )
                                .child(
                                    div()
                                        .relative()
                                        .flex_1()
                                        .h_full()
                                        .flex()
                                        .items_center()
                                        .child(self.render_inline_input_layer(
                                            Font {
                                                family: self.font_family.clone(),
                                                weight: FontWeight::NORMAL,
                                                ..Default::default()
                                            },
                                            px(11.0),
                                            text.into(),
                                            selected_bg.into(),
                                            InlineInputAlignment::Left,
                                            cx,
                                        )),
                                ),
                        )
                        .into_any_element()
                }))
                .child(
                    div()
                        .id("agent-sidebar-scroll")
                        .flex_1()
                        .overflow_y_scroll()
                        .child(
                            div()
                                .w_full()
                                .pb(px(8.0))
                                .flex()
                                .flex_col()
                                .children(project_groups)
                                .children(empty_state),
                        ),
                )
                .into_any_element(),
        )
    }
}
