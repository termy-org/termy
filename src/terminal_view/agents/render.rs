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
            .h(px(22.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_size(px(12.0))
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
            .h(px(22.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(border)
            .bg(bg)
            .text_size(px(12.0))
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

    pub(in super::super) fn render_agent_sidebar_new_session_icon(
        stroke: gpui::Rgba,
        bg: gpui::Rgba,
    ) -> AnyElement {
        div()
            .relative()
            .flex_none()
            .w(px(17.0))
            .h(px(14.0))
            .child(
                div()
                    .absolute()
                    .left(px(1.0))
                    .top(px(2.0))
                    .w(px(5.0))
                    .h(px(3.0))
                    .bg(bg)
                    .border_1()
                    .border_color(stroke),
            )
            .child(
                div()
                    .absolute()
                    .left_0()
                    .top(px(4.0))
                    .w(px(11.0))
                    .h(px(8.0))
                    .bg(bg)
                    .border_1()
                    .border_color(stroke),
            )
            .child(
                div()
                    .absolute()
                    .right(px(1.0))
                    .top(px(3.0))
                    .w(px(1.5))
                    .h(px(7.0))
                    .bg(stroke),
            )
            .child(
                div()
                    .absolute()
                    .right_0()
                    .top(px(6.0))
                    .w(px(5.0))
                    .h(px(1.5))
                    .bg(stroke),
            )
            .into_any_element()
    }

    pub(in super::super) fn render_agent_sidebar_hide_icon(stroke: gpui::Rgba) -> AnyElement {
        div()
            .relative()
            .flex_none()
            .w(px(15.0))
            .h(px(12.0))
            .child(
                div()
                    .absolute()
                    .right_0()
                    .top(px(1.0))
                    .w(px(12.0))
                    .h(px(1.5))
                    .bg(stroke),
            )
            .child(
                div()
                    .absolute()
                    .right_0()
                    .top(px(5.0))
                    .w(px(9.0))
                    .h(px(1.5))
                    .bg(stroke),
            )
            .child(
                div()
                    .absolute()
                    .right_0()
                    .top(px(9.0))
                    .w(px(6.0))
                    .h(px(1.5))
                    .bg(stroke),
            )
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
        border: gpui::Rgba,
        panel_bg: gpui::Rgba,
        input_bg: gpui::Rgba,
        selected_bg: gpui::Rgba,
        text: gpui::Rgba,
        muted: gpui::Rgba,
        success: gpui::Rgba,
        warning: gpui::Rgba,
        danger: gpui::Rgba,
        info: gpui::Rgba,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let repo_path = entry.repo_path.clone();
        let row_repo_path = repo_path.clone();
        let status_color = if entry.is_untracked() || entry.status.contains('A') {
            success
        } else if entry.status.contains('D') || entry.status.contains('U') {
            danger
        } else if entry.status.contains('R') {
            info
        } else {
            warning
        };

        let preview = is_selected.then(|| {
            let open_repo_path = repo_path.clone();
            let diff_repo_path = repo_path.clone();
            let stage_repo_path = repo_path.clone();
            let unstage_repo_path = repo_path.clone();
            let discard_entry = entry.clone();
            let preview_body = if preview_loading {
                div()
                    .text_size(px(11.5))
                    .text_color(muted)
                    .child("Loading diff preview...")
                    .into_any_element()
            } else if let Some(error) = preview_error {
                div()
                    .text_size(px(11.5))
                    .text_color(muted)
                    .child(error.to_string())
                    .into_any_element()
            } else {
                div()
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |view, _event, _window, cx| {
                                            match view.open_agent_git_file(open_repo_path.as_str())
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
                                        "open", border, input_bg, text,
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
                                        "diff", border, input_bg, text,
                                    )),
                            )
                            .children((entry.is_untracked() || entry.is_unstaged()).then(|| {
                                div()
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |view, _event, _window, cx| {
                                            view.run_agent_git_mutation(
                                                vec![
                                                    "add".to_string(),
                                                    "--".to_string(),
                                                    stage_repo_path.clone(),
                                                ],
                                                "Staged file",
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }),
                                    )
                                    .child(Self::render_agent_sidebar_chip(
                                        "stage", border, input_bg, success,
                                    ))
                                    .into_any_element()
                            }))
                            .children(entry.is_staged().then(|| {
                                div()
                                    .cursor_pointer()
                                    .on_mouse_down(
                                        MouseButton::Left,
                                        cx.listener(move |view, _event, _window, cx| {
                                            view.run_agent_git_mutation(
                                                vec![
                                                    "restore".to_string(),
                                                    "--staged".to_string(),
                                                    "--".to_string(),
                                                    unstage_repo_path.clone(),
                                                ],
                                                "Unstaged file",
                                                cx,
                                            );
                                            cx.stop_propagation();
                                        }),
                                    )
                                    .child(Self::render_agent_sidebar_chip(
                                        "unstage", border, input_bg, warning,
                                    ))
                                    .into_any_element()
                            }))
                            .children(
                                (entry.is_untracked() || entry.is_unstaged() || entry.is_deleted())
                                    .then(|| {
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
                                                "discard", border, input_bg, danger,
                                            ))
                                            .into_any_element()
                                    }),
                            ),
                    )
                    .children((!preview_diff_lines.is_empty()).then(|| {
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(1.0))
                            .children(preview_diff_lines.iter().take(60).map(|line| {
                                let tone = if line.starts_with('+') && !line.starts_with("+++") {
                                    success
                                } else if line.starts_with('-') && !line.starts_with("---") {
                                    danger
                                } else if line.starts_with("@@") {
                                    warning
                                } else {
                                    muted
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
                    .children((preview_history.is_empty()).then(|| {
                        div()
                            .text_size(px(11.5))
                            .text_color(muted)
                            .child("No file history yet.")
                            .into_any_element()
                    }))
                    .children((!preview_history.is_empty()).then(|| {
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(div().text_size(px(11.5)).text_color(muted).child("History"))
                            .children(preview_history.iter().take(6).map(|entry| {
                                div()
                                    .truncate()
                                    .text_size(px(11.5))
                                    .text_color(text)
                                    .child(entry.summary.clone())
                                    .into_any_element()
                            }))
                            .into_any_element()
                    }))
                    .into_any_element()
            };

            div()
                .mx(px(8.0))
                .mb(px(8.0))
                .px(px(8.0))
                .py(px(10.0))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .border_1()
                .border_color(border)
                .bg(selected_bg)
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
                    .px(px(8.0))
                    .py(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .bg(if is_selected { selected_bg } else { panel_bg })
                    .cursor_pointer()
                    .when(show_border, |this| this.border_b_1().border_color(border))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |view, _event, _window, cx| {
                            view.select_agent_git_panel_entry(row_repo_path.as_str(), cx);
                            cx.stop_propagation();
                        }),
                    )
                    .child(Self::render_agent_sidebar_chip(
                        entry.badge_label(),
                        border,
                        input_bg,
                        status_color,
                    ))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .truncate()
                                    .text_size(px(12.5))
                                    .text_color(text)
                                    .child(entry.path.clone()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(12.5))
                                    .text_color(muted)
                                    .child(entry.status.clone()),
                            ),
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
        border: gpui::Rgba,
        panel_bg: gpui::Rgba,
        input_bg: gpui::Rgba,
        selected_bg: gpui::Rgba,
        text: gpui::Rgba,
        muted: gpui::Rgba,
        success: gpui::Rgba,
        warning: gpui::Rgba,
        danger: gpui::Rgba,
        info: gpui::Rgba,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if entries.is_empty() {
            return None;
        }

        let count = entries.len();
        Some(
            div()
                .px(px(8.0))
                .pb(px(8.0))
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .px(px(2.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(muted)
                                .child(title.to_string()),
                        )
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(muted)
                                .child(count.to_string()),
                        ),
                )
                .child(
                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .border_1()
                        .border_color(border)
                        .bg(panel_bg)
                        .children(entries.into_iter().enumerate().map(|(index, entry)| {
                            self.render_agent_git_panel_entry(
                                entry.clone(),
                                selected_repo_path == Some(entry.repo_path.as_str()),
                                index + 1 < count,
                                preview_loading,
                                preview_error,
                                preview_diff_lines,
                                preview_history,
                                border,
                                panel_bg,
                                input_bg,
                                selected_bg,
                                text,
                                muted,
                                success,
                                warning,
                                danger,
                                info,
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
        border: gpui::Rgba,
        panel_bg: gpui::Rgba,
        input_bg: gpui::Rgba,
        selected_bg: gpui::Rgba,
        text: gpui::Rgba,
        muted: gpui::Rgba,
        info: gpui::Rgba,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active_mode = input_mode.unwrap_or(AgentGitPanelInputMode::Commit);
        let has_value = !input_text.trim().is_empty();
        let show_cancel = input_mode.is_some() || has_value;
        let action_color = match active_mode {
            AgentGitPanelInputMode::Commit => text,
            AgentGitPanelInputMode::CreateBranch => info,
            AgentGitPanelInputMode::SaveStash => muted,
        };

        div()
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(6.0))
            .px(px(8.0))
            .py(px(10.0))
            .border_t_1()
            .border_color(border)
            .bg(panel_bg)
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(muted)
                    .child(active_mode.title()),
            )
            .child({
                let line_count = if active_mode == AgentGitPanelInputMode::Commit {
                    input_text.lines().count().max(1)
                } else {
                    1
                };
                let input_height = px(8.0 + line_count as f32 * 20.0_f32);
                div()
                    .relative()
                    .min_h(px(36.0))
                    .h(input_height)
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .border_1()
                    .border_color(border)
                    .bg(input_bg)
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
                            .text_color(muted)
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
                            text.into(),
                            selected_bg.into(),
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
                                .text_color(text)
                                .child(input_text.to_string())
                                .into_any_element()
                        } else {
                            div()
                                .truncate()
                                .text_size(px(12.5))
                                .text_color(text)
                                .child(input_text.to_string())
                                .into_any_element()
                        }
                    }))
            })
            .child(
                div()
                    .flex()
                    .justify_between()
                    .gap(px(6.0))
                    .child(
                        div().flex().flex_wrap().gap(px(4.0)).children(
                            [
                                (AgentGitPanelInputMode::Commit, "commit", text),
                                (AgentGitPanelInputMode::SaveStash, "stash", muted),
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
                                        border,
                                        if is_active { selected_bg } else { input_bg },
                                        if is_active { text } else { color },
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
                                        border,
                                        input_bg,
                                        action_color,
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
                                        "cancel", border, input_bg, muted,
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

        let overlay_style = self.overlay_style();
        let panel_bg = overlay_style.chrome_panel_background_with_floor(0.96, 0.88);
        let input_bg = overlay_style.chrome_panel_background_with_floor(0.74, 0.72);
        let selected_bg = overlay_style.panel_cursor(0.10);
        let text = overlay_style.panel_foreground(0.94);
        let muted = overlay_style.panel_foreground(0.62);
        let border = resolve_chrome_stroke_color(
            panel_bg,
            self.colors.foreground,
            self.chrome_contrast_profile().stroke_mix,
        );
        let success = self.colors.ansi[10];
        let warning = self.colors.ansi[11];
        let danger = self.colors.ansi[9];
        let info = self.colors.ansi[12];

        let filtered_entries = self.agent_git_entries_for_filter();
        let tracked_entries = filtered_entries
            .iter()
            .filter(|entry| !entry.is_untracked())
            .cloned()
            .collect::<Vec<_>>();
        let untracked_entries = filtered_entries
            .iter()
            .filter(|entry| entry.is_untracked())
            .cloned()
            .collect::<Vec<_>>();

        let label = self
            .agent_git_panel
            .label
            .clone()
            .unwrap_or_else(|| "Git Changes".to_string());
        let repo_root = self.agent_git_panel.repo_root.clone();
        let repo_name = repo_root
            .as_deref()
            .and_then(|path| Path::new(path).file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| label.clone());
        let current_branch = self.agent_git_panel.current_branch.clone();
        let current_branch_for_branches = current_branch.clone();
        let ahead = self.agent_git_panel.ahead;
        let behind = self.agent_git_panel.behind;
        let dirty_count = self.agent_git_panel.dirty_count;
        let loading = self.agent_git_panel.loading;
        let error = self.agent_git_panel.error.clone();
        let selected_repo_path = self.agent_git_panel.selected_repo_path.clone();
        let preview_loading = self.agent_git_panel.preview_loading;
        let preview_error = self.agent_git_panel.preview_error.clone();
        let preview_diff_lines = self.agent_git_panel.preview_diff_lines.clone();
        let preview_history = self.agent_git_panel.preview_history.clone();
        let branches = self.agent_git_panel.branches.clone();
        let branches_for_dropdown = branches.clone();
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
        let change_summary = match total_changes {
            0 => "No Changes".to_string(),
            1 => "1 Change".to_string(),
            count => format!("{count} Changes"),
        };

        let body = if loading {
            div()
                .px(px(10.0))
                .py(px(12.0))
                .text_size(px(12.5))
                .text_color(muted)
                .child("Loading git changes...")
                .into_any_element()
        } else if let Some(error) = error {
            div()
                .px(px(10.0))
                .py(px(12.0))
                .text_size(px(12.5))
                .text_color(muted)
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
                        .text_color(muted)
                        .child("No files match the current filter.")
                        .into_any_element(),
                );
            }
            if let Some(section) = self.render_agent_git_panel_section(
                "Tracked",
                tracked_entries,
                selected_repo_path.as_deref(),
                preview_loading,
                preview_error.as_deref(),
                &preview_diff_lines,
                &preview_history,
                border,
                panel_bg,
                input_bg,
                selected_bg,
                text,
                muted,
                success,
                warning,
                danger,
                info,
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
                border,
                panel_bg,
                input_bg,
                selected_bg,
                text,
                muted,
                success,
                warning,
                danger,
                info,
                cx,
            ) {
                sections.push(section);
            }
            if !stashes.is_empty() {
                sections.push(
                    div()
                        .px(px(8.0))
                        .pb(px(8.0))
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .child(
                            div()
                                .px(px(2.0))
                                .text_size(px(11.5))
                                .text_color(muted)
                                .child("Stashes"),
                        )
                        .children(stashes.into_iter().take(5).map(|stash| {
                            let apply_name = stash.name.clone();
                            let pop_name = stash.name.clone();
                            div()
                                .px(px(2.0))
                                .flex()
                                .items_center()
                                .gap(px(4.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .truncate()
                                        .text_size(px(11.5))
                                        .text_color(text)
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
                                            "apply", border, input_bg, text,
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
                                            "pop", border, input_bg, warning,
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
                .bg(panel_bg)
                .border_l_1()
                .border_color(border)
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
                        .border_color(border)
                        .child(
                            div()
                                .flex_1()
                                .min_w(px(0.0))
                                .truncate()
                                .text_size(px(12.5))
                                .text_color(muted)
                                .child(change_summary),
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
                                            cx.listener(move |view, _event, _window, cx| {
                                                if view.agent_git_panel.entries.is_empty() {
                                                    cx.stop_propagation();
                                                    return;
                                                }
                                                if all_staged {
                                                    view.run_agent_git_mutation(
                                                        vec![
                                                            "restore".to_string(),
                                                            "--staged".to_string(),
                                                            ".".to_string(),
                                                        ],
                                                        "Unstaged all changes",
                                                        cx,
                                                    );
                                                } else {
                                                    view.run_agent_git_mutation(
                                                        vec!["add".to_string(), "-A".to_string()],
                                                        "Staged all changes",
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
                                            border,
                                            input_bg,
                                            if all_staged { warning } else { success },
                                        )),
                                )
                                .child(
                                    div()
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|view, _event, _window, cx| {
                                                view.refresh_agent_git_panel(cx);
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child(Self::render_agent_sidebar_chip(
                                            "refresh", border, input_bg, text,
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
                                            "push", border, input_bg, info,
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
                                            "hide", border, input_bg, muted,
                                        )),
                                ),
                        ),
                )
                .child(
                    div()
                        .px(px(8.0))
                        .py(px(6.0))
                        .flex_none()
                        .flex()
                        .flex_col()
                        .gap(px(4.0))
                        .border_b_1()
                        .border_color(border)
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .truncate()
                                        .text_size(px(11.5))
                                        .text_color(text)
                                        .child(repo_name),
                                )
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
                                            border,
                                            if branch_dropdown_open {
                                                selected_bg
                                            } else {
                                                input_bg
                                            },
                                            if branch_dropdown_open { text } else { muted },
                                        ))
                                        .into_any_element()
                                })),
                        )
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
                                            border,
                                            input_bg,
                                            info,
                                        )),
                                )
                                .children(branches_for_dropdown.into_iter().map(|branch_name| {
                                    let is_current = current_branch_for_branches.as_deref()
                                        == Some(branch_name.as_str());
                                    if is_current {
                                        Self::render_agent_sidebar_chip(
                                            branch_name,
                                            border,
                                            selected_bg,
                                            text,
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
                                                border,
                                                input_bg,
                                                muted,
                                            ))
                                            .into_any_element()
                                    }
                                }))
                                .into_any_element()
                        }))
                        .child(
                            div()
                                .flex()
                                .flex_wrap()
                                .gap(px(4.0))
                                .children((ahead > 0).then(|| {
                                    Self::render_agent_sidebar_chip(
                                        format!("+{ahead}"),
                                        border,
                                        input_bg,
                                        success,
                                    )
                                    .into_any_element()
                                }))
                                .children((behind > 0).then(|| {
                                    Self::render_agent_sidebar_chip(
                                        format!("-{behind}"),
                                        border,
                                        input_bg,
                                        warning,
                                    )
                                    .into_any_element()
                                }))
                                .children((dirty_count > 0).then(|| {
                                    Self::render_agent_sidebar_chip(
                                        format!("{dirty_count} dirty"),
                                        border,
                                        input_bg,
                                        danger,
                                    )
                                    .into_any_element()
                                }))
                                .children(AgentGitPanelFilter::ALL.into_iter().map(|filter| {
                                    let is_selected = self.agent_git_panel.filter == filter;
                                    div()
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |view, _event, _window, cx| {
                                                view.set_agent_git_panel_filter(filter, cx);
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child(Self::render_agent_sidebar_chip(
                                            filter.label(),
                                            border,
                                            if is_selected { selected_bg } else { input_bg },
                                            if is_selected { text } else { muted },
                                        ))
                                        .into_any_element()
                                })),
                        ),
                )
                .child(
                    div()
                        .id("agent-git-panel-scroll")
                        .flex_1()
                        .overflow_y_scroll()
                        .child(div().w_full().py(px(8.0)).flex().flex_col().child(body)),
                )
                .child(self.render_agent_git_panel_footer(
                    input_mode,
                    input_text.as_str(),
                    border,
                    panel_bg,
                    input_bg,
                    selected_bg,
                    text,
                    muted,
                    info,
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
        let has_non_default_filter = self.agent_sidebar_filter != AgentSidebarFilter::All;
        let filtered_projects = self.filtered_agent_projects_for_sidebar();
        let filtered_thread_count = filtered_projects
            .iter()
            .map(|(_, threads)| threads.len())
            .sum::<usize>();
        let history_thread_count = self.agent_threads.len();
        let history_summary = if show_filtered_history || has_non_default_filter {
            format!(
                "{} match{}",
                filtered_thread_count,
                if filtered_thread_count == 1 { "" } else { "es" }
            )
        } else {
            format!(
                "{} thread{}",
                history_thread_count,
                if history_thread_count == 1 { "" } else { "s" }
            )
        };
        let all_projects_collapsed = self.are_all_agent_projects_collapsed();
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
                    || has_non_default_filter
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
            } else if has_non_default_filter {
                format!(
                    "No threads match the {} filter.",
                    self.agent_sidebar_filter.label().to_lowercase()
                )
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
                                        .child(Self::render_agent_sidebar_new_session_icon(
                                            muted, panel_bg,
                                        ))
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
                                        .child(Self::render_agent_sidebar_hide_icon(muted))
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|view, _event, _window, cx| {
                                                view.agent_sidebar_open = false;
                                                view.agent_sidebar_search_active = false;
                                                view.cancel_rename_agent_project(cx);
                                                view.cancel_rename_agent_thread(cx);
                                                view.hovered_agent_thread_id = None;
                                                view.close_agent_git_panel();
                                                view.sync_persisted_agent_workspace();
                                                cx.notify();
                                                cx.stop_propagation();
                                            }),
                                        ),
                                ),
                        ),
                )
                .child(
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
                                .bg(if self.agent_sidebar_search_active {
                                    selected_bg
                                } else {
                                    input_bg
                                })
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
                                        .children(
                                            (!self.agent_sidebar_search_active
                                                && self
                                                    .agent_sidebar_search_input
                                                    .text()
                                                    .trim()
                                                    .is_empty())
                                            .then(|| {
                                                div()
                                                    .truncate()
                                                    .text_size(px(12.5))
                                                    .text_color(muted)
                                                    .child("Search history")
                                                    .into_any_element()
                                            }),
                                        )
                                        .children(
                                            (!self.agent_sidebar_search_active
                                                && !self
                                                    .agent_sidebar_search_input
                                                    .text()
                                                    .trim()
                                                    .is_empty())
                                            .then(|| {
                                                div()
                                                    .truncate()
                                                    .text_size(px(12.5))
                                                    .text_color(text)
                                                    .child(
                                                        self.agent_sidebar_search_input
                                                            .text()
                                                            .to_string(),
                                                    )
                                                    .into_any_element()
                                            }),
                                        )
                                        .children(self.agent_sidebar_search_active.then(|| {
                                            self.render_inline_input_layer(
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
                                            )
                                        })),
                                ),
                        ),
                )
                .child(
                    div()
                        .px(px(10.0))
                        .pb(px(2.0))
                        .flex_none()
                        .flex()
                        .flex_wrap()
                        .gap(px(4.0))
                        .children(AgentSidebarFilter::ALL.into_iter().map(|filter| {
                            let is_selected = self.agent_sidebar_filter == filter;
                            div()
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |view, _event, _window, cx| {
                                        view.set_agent_sidebar_filter(filter, cx);
                                        cx.stop_propagation();
                                    }),
                                )
                                .child(Self::render_agent_sidebar_chip(
                                    filter.label(),
                                    border,
                                    if is_selected { selected_bg } else { input_bg },
                                    if is_selected { text } else { muted },
                                ))
                                .into_any_element()
                        })),
                )
                .child(
                    div()
                        .px(px(10.0))
                        .pt(px(2.0))
                        .pb(px(2.0))
                        .flex_none()
                        .flex()
                        .justify_between()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .text_color(muted)
                                .child(if show_filtered_history {
                                    "Search Results"
                                } else {
                                    "History"
                                }),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .children((!show_filtered_history
                                    && !has_non_default_filter
                                    && !self.agent_projects.is_empty())
                                    .then(|| {
                                    div()
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |view, _event, _window, cx| {
                                                view.set_all_agent_projects_collapsed(!all_projects_collapsed, cx);
                                                cx.stop_propagation();
                                            }),
                                        )
                                        .child(Self::render_agent_sidebar_chip(
                                            if all_projects_collapsed { "expand" } else { "collapse" },
                                            border,
                                            input_bg,
                                            muted,
                                        ))
                                        .into_any_element()
                                }))
                                .child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(muted)
                                        .child(history_summary),
                                ),
                        ),
                )
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
