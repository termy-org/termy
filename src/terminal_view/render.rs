use super::scrollbar as terminal_scrollbar;
use super::*;
use crate::ui::scrollbar::{self as ui_scrollbar, ScrollbarPaintStyle};
use gpui::prelude::FluentBuilder;

fn cell_ranges_overlap(start_a: u32, end_a: u32, start_b: u32, end_b: u32) -> bool {
    start_a < end_b && start_b < end_a
}

fn blend_rgb_only(base: gpui::Rgba, target: gpui::Rgba, factor: f32) -> gpui::Rgba {
    let factor = factor.clamp(0.0, 1.0);
    let inv = 1.0 - factor;
    gpui::Rgba {
        r: (base.r * inv) + (target.r * factor),
        g: (base.g * inv) + (target.g * factor),
        b: (base.b * inv) + (target.b * factor),
        a: base.a,
    }
}

fn desaturate_rgb(color: gpui::Rgba, amount: f32) -> gpui::Rgba {
    let amount = amount.clamp(0.0, 1.0);
    if amount <= f32::EPSILON {
        return color;
    }
    let luma = (color.r * 0.2126) + (color.g * 0.7152) + (color.b * 0.0722);
    let inv = 1.0 - amount;
    gpui::Rgba {
        r: (color.r * inv) + (luma * amount),
        g: (color.g * inv) + (luma * amount),
        b: (color.b * inv) + (luma * amount),
        a: color.a,
    }
}

fn effective_pane_focus_active_border_alpha(
    active_border_alpha: f32,
    runtime_uses_tmux: bool,
    tmux_show_active_pane_border: bool,
) -> f32 {
    if runtime_uses_tmux && !tmux_show_active_pane_border {
        0.0
    } else {
        active_border_alpha
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct TerminalScrollbarOverlayFrame {
    left: f32,
    top: f32,
    width: f32,
    height: f32,
}

fn terminal_scrollbar_overlay_frame(surface: TerminalViewportGeometry) -> TerminalScrollbarOverlayFrame {
    let surface_width = surface.width.max(0.0);
    let effective_gutter = TERMINAL_SCROLLBAR_GUTTER_WIDTH.min(surface_width);
    let left = (surface.origin_x + surface_width - effective_gutter).max(surface.origin_x);
    TerminalScrollbarOverlayFrame {
        left,
        top: surface.origin_y,
        width: effective_gutter,
        height: surface.height,
    }
}

impl Focusable for TerminalView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TerminalView {
    fn pane_right_gap_cells(pane: &TerminalPane, panes: &[TerminalPane]) -> u32 {
        let pane_size = pane.terminal.size();
        let pane_right = u32::from(pane.left) + u32::from(pane_size.cols);
        let pane_top = u32::from(pane.top);
        let pane_bottom = pane_top + u32::from(pane_size.rows);

        panes
            .iter()
            .filter_map(|candidate| {
                if candidate.id == pane.id {
                    return None;
                }

                let candidate_left = u32::from(candidate.left);
                if candidate_left < pane_right {
                    return None;
                }

                let candidate_size = candidate.terminal.size();
                let candidate_top = u32::from(candidate.top);
                let candidate_bottom = candidate_top + u32::from(candidate_size.rows);
                if !cell_ranges_overlap(pane_top, pane_bottom, candidate_top, candidate_bottom) {
                    return None;
                }

                Some(candidate_left.saturating_sub(pane_right))
            })
            .min()
            .unwrap_or(0)
    }

    fn pane_bottom_gap_cells(pane: &TerminalPane, panes: &[TerminalPane]) -> u32 {
        let pane_left = u32::from(pane.left);
        let pane_size = pane.terminal.size();
        let pane_right = pane_left + u32::from(pane_size.cols);
        let pane_bottom = u32::from(pane.top) + u32::from(pane_size.rows);

        panes
            .iter()
            .filter_map(|candidate| {
                if candidate.id == pane.id {
                    return None;
                }

                let candidate_top = u32::from(candidate.top);
                if candidate_top < pane_bottom {
                    return None;
                }

                let candidate_size = candidate.terminal.size();
                let candidate_left = u32::from(candidate.left);
                let candidate_right = candidate_left + u32::from(candidate_size.cols);
                if !cell_ranges_overlap(pane_left, pane_right, candidate_left, candidate_right) {
                    return None;
                }

                Some(candidate_top.saturating_sub(pane_bottom))
            })
            .min()
            .unwrap_or(0)
    }

    fn refresh_terminal_scrollbar_marker_cache(
        &mut self,
        layout: terminal_scrollbar::TerminalScrollbarLayout,
        marker_height: f32,
    ) -> Option<f32> {
        if !self.search_open {
            self.clear_terminal_scrollbar_marker_cache();
            return None;
        }

        let marker_height = marker_height.max(0.0);
        let marker_top_limit =
            terminal_scrollbar::marker_top_limit(layout.metrics.track_height, marker_height);
        let cache_key = TerminalScrollbarMarkerCacheKey {
            results_revision: self.search_state.results_revision(),
            history_size: layout.history_size,
            viewport_rows: layout.viewport_rows,
            marker_top_limit_bucket: terminal_scrollbar::marker_top_limit_bucket(marker_top_limit),
        };
        let rebuild_markers = self.terminal_scrollbar_marker_cache.key.as_ref() != Some(&cache_key);

        let (is_empty, current_line, new_marker_tops) = {
            let results = self.search_state.results();
            if results.is_empty() {
                (true, None, None)
            } else {
                let current_line = results.current().map(|current| current.line);
                let new_marker_tops = rebuild_markers.then(|| {
                    terminal_scrollbar::deduped_marker_tops(
                        results
                            .matches()
                            .iter()
                            .map(|search_match| search_match.line),
                        layout.history_size,
                        layout.viewport_rows,
                        marker_height,
                        marker_top_limit,
                    )
                });
                (false, current_line, new_marker_tops)
            }
        };

        if is_empty {
            self.clear_terminal_scrollbar_marker_cache();
            return None;
        }

        if let Some(marker_tops) = new_marker_tops {
            self.terminal_scrollbar_marker_cache.marker_tops = marker_tops;
            self.terminal_scrollbar_marker_cache.key = Some(cache_key);
        }

        current_line.map(|line| {
            terminal_scrollbar::marker_top_for_line(
                line,
                layout.history_size,
                layout.viewport_rows,
                marker_top_limit,
            )
        })
    }

    fn render_terminal_scrollbar_overlay(
        &mut self,
        surface: TerminalViewportGeometry,
        layout: terminal_scrollbar::TerminalScrollbarLayout,
        force_visible: bool,
    ) -> Option<AnyElement> {
        let now = Instant::now();
        let force_visible = force_visible
            && self.terminal_scrollbar_mode() != ui_scrollbar::ScrollbarVisibilityMode::AlwaysOff;
        let alpha = if force_visible {
            1.0
        } else {
            self.terminal_scrollbar_alpha(now)
        };
        if alpha <= f32::EPSILON && !self.terminal_scrollbar_visibility_controller.is_dragging() {
            return None;
        }
        let overlay_style = self.overlay_style();
        let gutter_bg = overlay_style.panel_background(TERMINAL_SCROLLBAR_GUTTER_ALPHA);
        let style = ScrollbarPaintStyle {
            width: TERMINAL_SCROLLBAR_TRACK_WIDTH,
            track_radius: TERMINAL_SCROLLBAR_TRACK_RADIUS,
            thumb_radius: TERMINAL_SCROLLBAR_THUMB_RADIUS,
            thumb_inset: TERMINAL_SCROLLBAR_THUMB_INSET,
            marker_inset: TERMINAL_SCROLLBAR_THUMB_INSET,
            marker_radius: TERMINAL_SCROLLBAR_THUMB_RADIUS,
            track_color: self.scrollbar_color(overlay_style, TERMINAL_SCROLLBAR_TRACK_ALPHA),
            thumb_color: self.scrollbar_color(overlay_style, TERMINAL_SCROLLBAR_THUMB_ALPHA),
            active_thumb_color: self
                .scrollbar_color(overlay_style, TERMINAL_SCROLLBAR_THUMB_ACTIVE_ALPHA),
            marker_color: Some(
                self.scrollbar_color(overlay_style, TERMINAL_SCROLLBAR_MATCH_MARKER_ALPHA),
            ),
            current_marker_color: Some(
                overlay_style.panel_cursor(TERMINAL_SCROLLBAR_CURRENT_MARKER_ALPHA),
            ),
        }
        .scale_alpha(alpha);

        let current_marker_top =
            self.refresh_terminal_scrollbar_marker_cache(layout, TERMINAL_SCROLLBAR_MARKER_HEIGHT);
        let marker_tops = &self.terminal_scrollbar_marker_cache.marker_tops;
        let frame = terminal_scrollbar_overlay_frame(surface);

        Some(
            div()
                .id("terminal-scrollbar-overlay")
                .absolute()
                .left(px(frame.left))
                .top(px(frame.top))
                .w(px(frame.width))
                .h(px(frame.height))
                .bg(gutter_bg)
                .child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .right_0()
                        .w(px(TERMINAL_SCROLLBAR_TRACK_WIDTH))
                        .child(ui_scrollbar::render_vertical(
                            "terminal-scrollbar",
                            layout.metrics,
                            style,
                            self.terminal_scrollbar_visibility_controller.is_dragging(),
                            marker_tops,
                            current_marker_top,
                            TERMINAL_SCROLLBAR_MARKER_HEIGHT,
                        )),
                )
                .into_any_element(),
        )
    }

    #[cfg(target_os = "macos")]
    fn render_update_banner(
        &mut self,
        state: &UpdateState,
        colors: &TerminalColors,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let model = termy_auto_update_ui::UpdateBannerModel::from_state(state)?;
        let updater_weak = self.auto_updater.as_ref().map(|e| e.downgrade());

        let mut banner_bg = colors.background;
        banner_bg.a = 0.88;
        let mut border_color = colors.foreground;
        border_color.a = 0.16;
        let mut muted_text = colors.foreground;
        muted_text.a = 0.72;

        let tone = match model.tone {
            termy_auto_update_ui::UpdateBannerTone::Info => {
                let mut color = colors.cursor;
                color.a = 0.22;
                color
            }
            termy_auto_update_ui::UpdateBannerTone::Success => gpui::Rgba {
                r: 0.25,
                g: 0.66,
                b: 0.36,
                a: 0.24,
            },
            termy_auto_update_ui::UpdateBannerTone::Error => gpui::Rgba {
                r: 0.85,
                g: 0.31,
                b: 0.31,
                a: 0.24,
            },
        };

        let mut actions = div().flex().items_center().gap(px(6.0));
        for button in model.buttons {
            let action = button.action;
            let updater_weak = updater_weak.clone();
            let (button_bg, button_text, button_border) = match button.style {
                termy_auto_update_ui::UpdateButtonStyle::Primary => {
                    let mut bg = colors.cursor;
                    bg.a = 0.96;
                    (
                        bg,
                        colors.background,
                        gpui::Rgba {
                            r: 0.0,
                            g: 0.0,
                            b: 0.0,
                            a: 0.0,
                        },
                    )
                }
                termy_auto_update_ui::UpdateButtonStyle::Secondary => {
                    let mut bg = colors.foreground;
                    bg.a = 0.08;
                    let mut border = colors.foreground;
                    border.a = 0.2;
                    (bg, colors.foreground, border)
                }
            };

            actions = actions.child(
                div()
                    .px(px(9.0))
                    .py(px(3.0))
                    .rounded_md()
                    .bg(button_bg)
                    .border_1()
                    .border_color(button_border)
                    .text_size(px(11.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(button_text)
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event, _window, cx| match action {
                            termy_auto_update_ui::UpdateBannerAction::Install => {
                                if let Some(ref weak) = updater_weak
                                    && let Some(entity) = weak.upgrade()
                                {
                                    AutoUpdater::install(entity.downgrade(), cx);
                                    termy_toast::info("Downloading update...");
                                }
                            }
                            termy_auto_update_ui::UpdateBannerAction::CompleteInstall => {
                                if let Some(ref weak) = updater_weak
                                    && let Some(entity) = weak.upgrade()
                                {
                                    AutoUpdater::complete_install(entity.downgrade(), cx);
                                    termy_toast::info("Starting installation...");
                                }
                            }
                            termy_auto_update_ui::UpdateBannerAction::Restart => {
                                match this.restart_application() {
                                    Ok(()) => cx.quit(),
                                    Err(error) => {
                                        termy_toast::error(format!("Restart failed: {}", error));
                                    }
                                }
                            }
                            termy_auto_update_ui::UpdateBannerAction::Dismiss => {
                                if let Some(ref weak) = updater_weak
                                    && let Some(entity) = weak.upgrade()
                                {
                                    entity.update(cx, |updater, cx| updater.dismiss(cx));
                                }
                            }
                        }),
                    )
                    .child(button.label),
            );
        }

        let progress_element = model.progress_percent.map(|progress| {
            let mut progress_track = colors.foreground;
            progress_track.a = 0.14;
            let progress_width = 130.0;
            let fill_width = (f32::from(progress) / 100.0) * progress_width;

            div()
                .mt(px(2.0))
                .w(px(progress_width))
                .h(px(4.0))
                .rounded_full()
                .bg(progress_track)
                .child(
                    div()
                        .h_full()
                        .w(px(fill_width.max(0.0)))
                        .rounded_full()
                        .bg(colors.cursor),
                )
                .into_any()
        });

        Some(
            div()
                .id("update-banner")
                .w_full()
                .h(px(UPDATE_BANNER_HEIGHT))
                .flex_none()
                .bg(banner_bg)
                .border_b_1()
                .border_color(border_color)
                .child(
                    div()
                        .size_full()
                        .px(px(10.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .px(px(8.0))
                                        .py(px(3.0))
                                        .rounded_full()
                                        .bg(tone)
                                        .text_size(px(10.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(colors.foreground)
                                        .child(model.badge),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .child(
                                            div()
                                                .text_size(px(12.0))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(colors.foreground)
                                                .child(model.message),
                                        )
                                        .children(model.detail.map(|detail| {
                                            div()
                                                .text_size(px(10.0))
                                                .text_color(muted_text)
                                                .child(detail)
                                                .into_any()
                                        }))
                                        .children(progress_element),
                                ),
                        )
                        .child(actions),
                )
                .into_any(),
        )
    }
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Process pending OSC 52 clipboard writes
        if let Some(text) = self.pending_clipboard.take() {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }

        self.toast_manager.ingest_pending();
        self.toast_manager.tick_with_hovered(self.hovered_toast);
        if let Some((_, copied_at)) = self.copied_toast_feedback
            && copied_at.elapsed() >= Duration::from_millis(TOAST_COPY_FEEDBACK_MS)
        {
            self.copied_toast_feedback = None;
        }

        // Request re-render during toast animations for smooth fade in/out
        // Only schedule one timer at a time to avoid spawning 60 tasks/sec
        if self.toast_manager.is_animating() && !self.toast_animation_scheduled {
            self.toast_animation_scheduled = true;
            cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                smol::Timer::after(Duration::from_millis(16)).await;
                let _ = cx.update(|cx| {
                    this.update(cx, |view, cx| {
                        view.toast_animation_scheduled = false;
                        cx.notify();
                    })
                });
            })
            .detach();
        }

        // Compute update banner state
        #[cfg(target_os = "macos")]
        let banner_state = self.auto_updater.as_ref().map(|e| e.read(cx).state.clone());
        #[cfg(target_os = "macos")]
        {
            self.sync_update_toasts(banner_state.as_ref());
            self.show_update_banner = matches!(
                &banner_state,
                Some(
                    UpdateState::Available { .. }
                        | UpdateState::Downloading { .. }
                        | UpdateState::Downloaded { .. }
                        | UpdateState::Installing { .. }
                        | UpdateState::Installed { .. }
                        | UpdateState::Error(_)
                )
            );
        }

        let cell_size = self.calculate_cell_size(window, cx);
        let colors = self.colors.clone();
        let font_family = self.font_family.clone();
        let font_size = self.font_size;
        self.sync_window_background_appearance(window);
        let effective_background_opacity = self.background_opacity_factor();
        let (effective_padding_x, effective_padding_y) = self.effective_terminal_padding();
        let mut terminal_surface_bg = colors.background;
        terminal_surface_bg.a = self.scaled_background_alpha(terminal_surface_bg.a);

        self.sync_terminal_size(window, cell_size);

        let now = Instant::now();
        let active_pane_id = self.active_pane_id().map(ToOwned::to_owned);
        let active_tab_focus_snapshot = self
            .tabs
            .get(self.active_tab)
            .map(|tab| (tab.id, tab.panes.len()));
        self.update_pane_focus_target(
            active_tab_focus_snapshot.map(|(id, _)| id),
            active_tab_focus_snapshot
                .map(|(_, pane_count)| pane_count)
                .unwrap_or(0),
            active_pane_id.as_deref(),
            now,
        );
        let pane_focus_transition =
            self.pane_focus_transition_snapshot(active_tab_focus_snapshot.map(|(id, _)| id), now);
        let pane_focus_config = self.pane_focus_config();
        let terminal_cursor_active =
            !self.is_command_palette_open() && self.renaming_tab.is_none() && !self.search_open;
        let cursor_visible = terminal_cursor_active
            && self.cursor_visible_for_focus(self.focus_handle.is_focused(window));

        // Pre-compute search match info for active pane.
        let search_active = self.search_open;
        let mut terminal_display_offset = 0usize;
        let divider_rgba = pane_divider_color(terminal_surface_bg, colors.foreground);
        let divider_color: gpui::Hsla = divider_rgba.into();
        let mut pane_layers = Vec::<AnyElement>::new();
        let mut pane_dividers = Vec::<AnyElement>::new();
        let mut pane_focus_accents = Vec::<AnyElement>::new();
        let mut pane_focus_needs_animation = false;

        if let Some(active_tab) = self.tabs.get(self.active_tab) {
            let multi_pane = active_tab.panes.len() > 1;
            let pane_focus_enabled = multi_pane && pane_focus_config.is_some();
            pane_focus_needs_animation = pane_focus_enabled && pane_focus_transition.is_some();
            let max_right_cells = active_tab
                .panes
                .iter()
                .map(|pane| {
                    let pane_size = pane.terminal.size();
                    u32::from(pane.left).saturating_add(u32::from(pane_size.cols))
                })
                .max()
                .unwrap_or(0);
            let max_bottom_cells = active_tab
                .panes
                .iter()
                .map(|pane| {
                    let pane_size = pane.terminal.size();
                    u32::from(pane.top).saturating_add(u32::from(pane_size.rows))
                })
                .max()
                .unwrap_or(0);

            for pane in &active_tab.panes {
                let terminal = &pane.terminal;
                let terminal_size = terminal.size();
                let cols = terminal_size.cols as usize;
                let rows = terminal_size.rows as usize;
                if cols == 0 || rows == 0 {
                    continue;
                }
                let is_active_pane = active_pane_id.as_deref() == Some(pane.id.as_str());
                let (pane_inactive_focus, pane_active_focus) = if pane_focus_enabled {
                    if let Some((from_pane_id, to_pane_id, progress)) = pane_focus_transition.as_ref()
                    {
                        if pane.id == *from_pane_id {
                            (*progress, 1.0 - *progress)
                        } else if pane.id == *to_pane_id {
                            (1.0 - *progress, *progress)
                        } else {
                            (1.0, 0.0)
                        }
                    } else if is_active_pane {
                        (0.0, 1.0)
                    } else {
                        (1.0, 0.0)
                    }
                } else {
                    (0.0, 0.0)
                };
                let (
                    pane_inactive_fg_blend,
                    pane_inactive_bg_blend,
                    pane_inactive_desaturate,
                    raw_pane_active_border_alpha,
                ) = if let Some((preset, strength)) = pane_focus_config {
                    let inactive_scale = strength * pane_inactive_focus;
                    let active_scale = strength * pane_active_focus;
                    (
                        preset.inactive_fg_blend * inactive_scale,
                        preset.inactive_bg_blend * inactive_scale,
                        preset.inactive_desaturate * inactive_scale,
                        preset.active_border_alpha * active_scale,
                    )
                } else {
                    (0.0, 0.0, 0.0, 0.0)
                };
                // tmux mode already has pane boundary affordances; layering Termy's active-pane
                // outline on top creates a second full-frame box around the active pane.
                let pane_active_border_alpha = effective_pane_focus_active_border_alpha(
                    raw_pane_active_border_alpha,
                    self.runtime_uses_tmux(),
                    self.tmux_show_active_pane_border,
                );
                let pane_focus_target_bg = colors.background;
                let estimated_cells = cols.saturating_mul(rows);
                let mut cells_to_render: Vec<CellRenderInfo> = Vec::with_capacity(estimated_cells);
                let (cursor_col, cursor_row) = terminal.cursor_position();
                let mut pane_display_offset = 0usize;
                let pane_search_results = if search_active && is_active_pane {
                    Some(self.search_state.results())
                } else {
                    None
                };
                let _ = terminal.for_each_renderable_cell(
                    |display_offset, term_line, col, cell_content| {
                        pane_display_offset = display_offset;
                        let show_cursor = display_offset == 0 && cursor_visible && is_active_pane;
                        let Some(row) =
                            Self::viewport_row_from_term_line(term_line, display_offset)
                        else {
                            return;
                        };

                        let mut fg = colors.convert(cell_content.fg);
                        let mut bg = colors.convert(cell_content.bg);
                        if cell_content.flags.contains(Flags::INVERSE) {
                            std::mem::swap(&mut fg, &mut bg);
                        }
                        if cell_content.flags.contains(Flags::DIM) {
                            fg.r *= DIM_TEXT_FACTOR;
                            fg.g *= DIM_TEXT_FACTOR;
                            fg.b *= DIM_TEXT_FACTOR;
                        }
                        bg.a *= effective_background_opacity;
                        if pane_inactive_fg_blend > f32::EPSILON {
                            fg = blend_rgb_only(fg, pane_focus_target_bg, pane_inactive_fg_blend);
                        }
                        if pane_inactive_bg_blend > f32::EPSILON {
                            bg = blend_rgb_only(bg, terminal_surface_bg, pane_inactive_bg_blend);
                        }
                        if pane_inactive_desaturate > f32::EPSILON {
                            fg = desaturate_rgb(fg, pane_inactive_desaturate);
                            bg = desaturate_rgb(bg, pane_inactive_desaturate);
                        }

                        let c = cell_content.c;
                        let is_cursor = show_cursor && col == cursor_col && row == cursor_row;
                        let selected = is_active_pane && self.cell_is_selected(col, row);

                        let (search_current, search_match) =
                            if let Some(results) = &pane_search_results {
                                let is_current = results.is_current_match(term_line, col);
                                let is_any = results.is_any_match(term_line, col);
                                (is_current, is_any && !is_current)
                            } else {
                                (false, false)
                            };

                        cells_to_render.push(CellRenderInfo {
                            col,
                            row,
                            char: c,
                            fg: fg.into(),
                            bg: bg.into(),
                            bold: cell_content.flags.contains(Flags::BOLD),
                            render_text: !cell_content.flags.intersects(
                                Flags::WIDE_CHAR_SPACER
                                    | Flags::LEADING_WIDE_CHAR_SPACER
                                    | Flags::HIDDEN,
                            ),
                            is_cursor,
                            selected,
                            search_current,
                            search_match,
                        });
                    },
                );

                if is_active_pane {
                    terminal_display_offset = pane_display_offset;
                }

                let mut selection_bg = colors.cursor;
                selection_bg.a = SELECTION_BG_ALPHA;
                let selection_fg = colors.background;
                let hovered_link_range = if is_active_pane {
                    self.hovered_link
                        .as_ref()
                        .map(|link| (link.row, link.start_col, link.end_col))
                } else {
                    None
                };

                let default_cell_bg: gpui::Hsla = {
                    let mut bg = colors.background;
                    bg.a = self.scaled_background_alpha(bg.a);
                    bg.into()
                };

                let terminal_grid = TerminalGrid {
                    cells: cells_to_render,
                    cell_size,
                    cols,
                    rows,
                    clear_bg: gpui::Hsla::transparent_black(),
                    default_bg: default_cell_bg,
                    cursor_color: colors.cursor.into(),
                    selection_bg: selection_bg.into(),
                    selection_fg: selection_fg.into(),
                    // Search highlight colors tuned for strong contrast on dark terminal themes.
                    search_match_bg: gpui::Hsla {
                        h: 0.14,
                        s: 0.92,
                        l: 0.62,
                        a: 0.62,
                    },
                    search_current_bg: gpui::Hsla {
                        h: 0.09,
                        s: 0.98,
                        l: 0.56,
                        a: 0.86,
                    },
                    hovered_link_range,
                    font_family: font_family.clone(),
                    font_size,
                    cursor_style: self.terminal_cursor_style(),
                };

                let cell_width: f32 = cell_size.width.into();
                let cell_height: f32 = cell_size.height.into();
                let pane_left = effective_padding_x + (f32::from(pane.left) * cell_width);
                let pane_top = effective_padding_y + (f32::from(pane.top) * cell_height);
                let pane_width = f32::from(terminal_size.cols) * cell_width;
                let pane_height = f32::from(terminal_size.rows) * cell_height;
                if pane_width <= f32::EPSILON || pane_height <= f32::EPSILON {
                    continue;
                }

                // Keep divider-boundary comparisons in the same geometry space as the
                // pane pixels computed above to avoid drift from mixed sources.
                let pane_right_cells =
                    ((pane_left + pane_width - effective_padding_x) / cell_width).round().max(0.0) as u32;
                let pane_bottom_cells =
                    ((pane_top + pane_height - effective_padding_y) / cell_height).round().max(0.0)
                        as u32;

                if multi_pane && pane_right_cells < max_right_cells {
                    let gap_cells = Self::pane_right_gap_cells(pane, &active_tab.panes);
                    let gap_px = (gap_cells as f32) * cell_width;
                    let divider_left = pane_left + pane_width + (gap_px * 0.5) - 0.5;
                    pane_dividers.push(
                        div()
                            .absolute()
                            .left(px(divider_left))
                            .top(px(pane_top))
                            .w(px(1.0))
                            .h(px(pane_height))
                            .bg(divider_color)
                            .into_any_element(),
                    );
                }
                if multi_pane && pane_bottom_cells < max_bottom_cells {
                    let gap_cells = Self::pane_bottom_gap_cells(pane, &active_tab.panes);
                    let gap_px = (gap_cells as f32) * cell_height;
                    let divider_top = pane_top + pane_height + (gap_px * 0.5) - 0.5;
                    pane_dividers.push(
                        div()
                            .absolute()
                            .left(px(pane_left))
                            .top(px(divider_top))
                            .w(px(pane_width))
                            .h(px(1.0))
                            .bg(divider_color)
                            .into_any_element(),
                    );
                }

                pane_layers.push(
                    div()
                        .id(SharedString::from(format!("pane-{}", pane.id)))
                        .absolute()
                        .left(px(pane_left))
                        .top(px(pane_top))
                        .w(px(pane_width))
                        .h(px(pane_height))
                        .child(terminal_grid)
                        .into_any_element(),
                );

                if pane_active_border_alpha > f32::EPSILON {
                    let mut accent = blend_rgb_only(colors.cursor, colors.foreground, 0.18);
                    accent.a = self.scaled_chrome_alpha(pane_active_border_alpha);
                    let accent_hsla: gpui::Hsla = accent.into();
                    pane_focus_accents.push(
                        div()
                            .id(SharedString::from(format!("pane-focus-accent-{}", pane.id)))
                            .absolute()
                            .left(px(pane_left))
                            .top(px(pane_top))
                            .w(px(pane_width))
                            .h(px(pane_height))
                            .border_1()
                            .border_color(accent_hsla)
                            .into_any_element(),
                    );
                }

                if pane.degraded {
                    // Hydration degraded panes still function, but this marker makes
                    // the warning state persistent until the next successful snapshot.
                    let degraded_accent = gpui::Hsla {
                        h: 0.09,
                        s: 0.92,
                        l: 0.58,
                        a: self.scaled_chrome_alpha(0.68),
                    };
                    pane_focus_accents.push(
                        div()
                            .id(SharedString::from(format!("pane-degraded-accent-{}", pane.id)))
                            .absolute()
                            .left(px(pane_left))
                            .top(px(pane_top))
                            .w(px(pane_width))
                            .h(px(pane_height))
                            .border_1()
                            .border_color(degraded_accent)
                            .into_any_element(),
                    );
                }
            }
        }

        if pane_focus_needs_animation {
            self.schedule_pane_focus_animation(cx);
        }

        let focus_handle = self.focus_handle.clone();
        let titlebar_height = Self::titlebar_height();
        let titlebar_bg = terminal_surface_bg;
        let tabbar_bg = terminal_surface_bg;
        let tabs_row = self.render_tab_strip(window, &colors, &font_family, tabbar_bg, cx);

        // Build update banner element (macOS only)
        #[cfg(target_os = "macos")]
        let banner_element: Option<AnyElement> = banner_state
            .as_ref()
            .and_then(|state| self.render_update_banner(state, &colors, cx));
        #[cfg(not(target_os = "macos"))]
        let banner_element: Option<AnyElement> = None;
        let terminal_surface_bg_hsla: gpui::Hsla = terminal_surface_bg.into();
        if self.terminal_scrollbar_mode() == ui_scrollbar::ScrollbarVisibilityMode::OnScroll
            && !self.terminal_scrollbar_animation_active
            && self.terminal_scrollbar_needs_animation(Instant::now())
        {
            self.start_terminal_scrollbar_animation(cx);
        }
        let terminal_surface = self.terminal_surface_geometry(window);
        let terminal_scrollbar_layout = terminal_surface.and_then(|surface| {
            self.terminal_scrollbar_layout_for_track(surface.height)
                .map(|layout| (surface, layout))
        });
        if terminal_scrollbar_layout.is_none() {
            self.clear_terminal_scrollbar_marker_cache();
        }
        let terminal_scrollbar_overlay = terminal_scrollbar_layout.and_then(|(surface, layout)| {
            self.render_terminal_scrollbar_overlay(surface, layout, terminal_display_offset > 0)
        });
        let terminal_grid_layer = div()
            .relative()
            .w_full()
            .h_full()
            .children(pane_layers)
            .children(pane_dividers)
            .children(pane_focus_accents)
            .into_any_element();
        let command_palette_overlay = if self.is_command_palette_open() {
            Some(self.render_command_palette_modal(cx))
        } else {
            None
        };
        let search_overlay = if self.search_open {
            Some(self.render_search_bar(cx))
        } else {
            None
        };
        let key_context = if self.has_active_inline_input() {
            "Terminal InlineInput"
        } else {
            "Terminal"
        };
        let titlebar_element: Option<AnyElement> = (titlebar_height > 0.0).then(|| {
            let titlebar_container = div()
                .id("titlebar")
                .w_full()
                .h(px(titlebar_height))
                .flex_none()
                .relative()
                .flex()
                .items_center()
                .on_mouse_move(cx.listener(Self::handle_titlebar_tab_strip_mouse_move))
                .bg(titlebar_bg);

            titlebar_container
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(Self::handle_unified_titlebar_mouse_down),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(Self::handle_unified_titlebar_mouse_up),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(Self::handle_unified_titlebar_mouse_up),
                )
                .child(
                    div()
                        .w_full()
                        .h_full()
                        .flex()
                        .items_end()
                        .mt(px(TOP_STRIP_CONTENT_OFFSET_Y))
                        .child(tabs_row),
                )
                .into_any()
        });
        let toast_overlay = if self.toast_manager.active().is_empty() {
            None
        } else {
            let mut container = div().flex().flex_col().gap(px(6.0));
            for toast in self.toast_manager.active().iter() {
                let toast_id = toast.id;
                let toast_message = toast.message.clone();
                let is_hovered = self.hovered_toast == Some(toast_id);
                let is_copied = self
                    .copied_toast_feedback
                    .is_some_and(|(id, _)| id == toast_id);

                // Animation values
                let opacity = toast.opacity();
                let slide_offset = toast.slide_offset();

                // Clean, minimal icons and subtle accent colors
                let (icon, accent, _is_loading) = match toast.kind {
                    termy_toast::ToastKind::Info => (
                        "\u{2139}", // ℹ info symbol
                        gpui::Rgba {
                            r: 0.53,
                            g: 0.70,
                            b: 0.92,
                            a: opacity,
                        },
                        false,
                    ),
                    termy_toast::ToastKind::Success => (
                        "\u{2713}", // ✓ checkmark
                        gpui::Rgba {
                            r: 0.42,
                            g: 0.78,
                            b: 0.55,
                            a: opacity,
                        },
                        false,
                    ),
                    termy_toast::ToastKind::Warning => (
                        "\u{26A0}", // ⚠ warning
                        gpui::Rgba {
                            r: 0.94,
                            g: 0.76,
                            b: 0.38,
                            a: opacity,
                        },
                        false,
                    ),
                    termy_toast::ToastKind::Error => (
                        "\u{2715}", // ✕ x mark
                        gpui::Rgba {
                            r: 0.92,
                            g: 0.45,
                            b: 0.45,
                            a: opacity,
                        },
                        false,
                    ),
                    termy_toast::ToastKind::Loading => {
                        // Animated spinner using braille characters
                        const SPINNER_FRAMES: &[&str] =
                            &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
                        let elapsed_ms = toast.created_at.elapsed().as_millis() as usize;
                        let frame_index = (elapsed_ms / 80) % SPINNER_FRAMES.len();
                        (
                            SPINNER_FRAMES[frame_index],
                            gpui::Rgba {
                                r: 0.53,
                                g: 0.70,
                                b: 0.92,
                                a: opacity,
                            },
                            true,
                        )
                    }
                };

                // Subtle, glassy background with animation
                let mut bg = colors.background;
                bg.a = 0.88 * opacity;
                let mut border = colors.foreground;
                border.a = 0.08 * opacity;
                let mut text = colors.foreground;
                text.a = 0.92 * opacity;

                container = container.child(
                    div()
                        .id(("toast", toast_id))
                        .max_w(px(480.0))
                        .mt(px(slide_offset))
                        .rounded_lg()
                        .bg(bg)
                        .border_1()
                        .border_color(border)
                        .shadow_md()
                        .child(
                            div()
                                .px(px(14.0))
                                .py(px(12.0))
                                .flex()
                                .items_start()
                                .gap(px(10.0))
                                // Icon
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .text_size(px(14.0))
                                        .text_color(accent)
                                        .mt(px(1.0))
                                        .child(icon),
                                )
                                // Message - max width accounts for icon (24px) + copy btn (68px) + gaps (20px) + padding (28px)
                                .child(
                                    div()
                                        .max_w(px(340.0))
                                        .text_size(px(13.0))
                                        .text_color(text)
                                        .child(toast_message.clone()),
                                )
                                .child(
                                    div()
                                        .flex_shrink_0()
                                        .w(px(68.0))
                                        .h(px(24.0))
                                        .flex()
                                        .items_center()
                                        .justify_end()
                                        .children(is_copied.then(|| {
                                            let mut copied_bg = accent;
                                            copied_bg.a = 0.22;
                                            div()
                                                .rounded(px(6.0))
                                                .px(px(8.0))
                                                .py(px(4.0))
                                                .text_size(px(11.0))
                                                .text_color(accent)
                                                .bg(copied_bg)
                                                .child("Copied")
                                        }))
                                        .children((!is_copied && is_hovered).then(|| {
                                            let toast_message_for_copy = toast_message.clone();
                                            div()
                                                .rounded(px(6.0))
                                                .px(px(8.0))
                                                .py(px(4.0))
                                                .text_size(px(11.0))
                                                .text_color(text)
                                                .bg(border)
                                                .hover(|style| style.bg(accent))
                                                .cursor_pointer()
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(
                                                        move |this, _event, _window, cx| {
                                                            cx.write_to_clipboard(
                                                                ClipboardItem::new_string(
                                                                    toast_message_for_copy.clone(),
                                                                ),
                                                            );
                                                            this.copied_toast_feedback =
                                                                Some((toast_id, Instant::now()));
                                                            cx.notify();
                                                            cx.spawn(
                                                                async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
                                                                    smol::Timer::after(Duration::from_millis(
                                                                        TOAST_COPY_FEEDBACK_MS,
                                                                    ))
                                                                    .await;
                                                                    let _ = cx.update(|cx| {
                                                                        this.update(cx, |view, cx| {
                                                                            if view
                                                                                .copied_toast_feedback
                                                                                .is_some_and(
                                                                                    |(id, _)| {
                                                                                        id == toast_id
                                                                                    },
                                                                                )
                                                                            {
                                                                                view.copied_toast_feedback = None;
                                                                                cx.notify();
                                                                            }
                                                                        })
                                                                    });
                                                                },
                                                            )
                                                            .detach();
                                                            cx.stop_propagation();
                                                        },
                                                    ),
                                                )
                                                .child("Copy")
                                        })),
                                )
                                .on_mouse_move(cx.listener(move |this, _event, _window, cx| {
                                    if this.hovered_toast != Some(toast_id) {
                                        this.hovered_toast = Some(toast_id);
                                        cx.notify();
                                    }
                                    cx.stop_propagation();
                                })),
                        ),
                );
            }

            Some(
                div()
                    .id("toast-overlay")
                    .size_full()
                    .absolute()
                    .top_0()
                    .left_0()
                    .child(
                        div()
                            .size_full()
                            .flex()
                            .flex_col()
                            .items_end()
                            .justify_end()
                            .pr(px(20.0))
                            .pb(px(20.0))
                            .on_mouse_move(cx.listener(|this, _event, _window, cx| {
                                if this.hovered_toast.is_some() {
                                    this.hovered_toast = None;
                                    cx.notify();
                                }
                            }))
                            .child(container),
                    )
                    .into_any(),
            )
        };
        let mut root_bg = colors.background;
        root_bg.a = self.scaled_background_alpha(root_bg.a);

        div()
            .id("termy-root")
            .flex()
            .flex_col()
            .size_full()
            .bg(root_bg)
            .font_family(font_family.clone())
            .capture_any_mouse_up(cx.listener(|this, event: &MouseUpEvent, _window, cx| {
                if event.button == MouseButton::Left {
                    this.disarm_titlebar_window_move();
                    this.commit_tab_drag(cx);
                }
            }))
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                    this.disarm_titlebar_window_move();
                    this.commit_tab_drag(cx);
                }),
            )
            .children(titlebar_element)
            .children(banner_element)
            .child(
                div()
                    .id("terminal")
                    .track_focus(&focus_handle)
                    .key_context(key_context)
                    .on_action(cx.listener(Self::handle_toggle_command_palette_action))
                    .on_action(cx.listener(Self::handle_import_colors_action))
                    .on_action(cx.listener(Self::handle_switch_theme_action))
                    .on_action(cx.listener(Self::handle_app_info_action))
                    .on_action(cx.listener(Self::handle_native_sdk_example_action))
                    .on_action(cx.listener(Self::handle_restart_app_action))
                    .on_action(cx.listener(Self::handle_rename_tab_action))
                    .on_action(cx.listener(Self::handle_check_for_updates_action))
                    .on_action(cx.listener(Self::handle_new_tab_action))
                    .on_action(cx.listener(Self::handle_close_tab_action))
                    .on_action(cx.listener(Self::handle_close_pane_or_tab_action))
                    .on_action(cx.listener(Self::handle_move_tab_left_action))
                    .on_action(cx.listener(Self::handle_move_tab_right_action))
                    .on_action(cx.listener(Self::handle_switch_tab_left_action))
                    .on_action(cx.listener(Self::handle_switch_tab_right_action))
                    .on_action(cx.listener(Self::handle_manage_tmux_sessions_action))
                    // GPUI grays out unavailable menu actions, so we only register
                    // File menu pane handlers when the tmux runtime is active.
                    .when(self.runtime_uses_tmux(), |s| {
                        s.on_action(cx.listener(Self::handle_split_pane_vertical_action))
                            .on_action(cx.listener(Self::handle_split_pane_horizontal_action))
                            .on_action(cx.listener(Self::handle_close_pane_action))
                            .on_action(cx.listener(Self::handle_focus_pane_next_action))
                    })
                    .on_action(cx.listener(Self::handle_focus_pane_left_action))
                    .on_action(cx.listener(Self::handle_focus_pane_right_action))
                    .on_action(cx.listener(Self::handle_focus_pane_up_action))
                    .on_action(cx.listener(Self::handle_focus_pane_down_action))
                    .on_action(cx.listener(Self::handle_focus_pane_previous_action))
                    .on_action(cx.listener(Self::handle_resize_pane_left_action))
                    .on_action(cx.listener(Self::handle_resize_pane_right_action))
                    .on_action(cx.listener(Self::handle_resize_pane_up_action))
                    .on_action(cx.listener(Self::handle_resize_pane_down_action))
                    .on_action(cx.listener(Self::handle_toggle_pane_zoom_action))
                    .on_action(cx.listener(Self::handle_minimize_window_action))
                    .on_action(cx.listener(Self::handle_copy_action))
                    .on_action(cx.listener(Self::handle_paste_action))
                    .on_action(cx.listener(Self::handle_zoom_in_action))
                    .on_action(cx.listener(Self::handle_zoom_out_action))
                    .on_action(cx.listener(Self::handle_zoom_reset_action))
                    .on_action(cx.listener(Self::handle_quit_action))
                    .on_action(cx.listener(Self::handle_open_search_action))
                    .on_action(cx.listener(Self::handle_close_search_action))
                    .on_action(cx.listener(Self::handle_search_next_action))
                    .on_action(cx.listener(Self::handle_search_previous_action))
                    .on_action(cx.listener(Self::handle_toggle_search_case_sensitive_action))
                    .on_action(cx.listener(Self::handle_toggle_search_regex_action))
                    .when(self.install_cli_available(), |s| {
                        s.on_action(cx.listener(Self::handle_install_cli_action))
                    })
                    .on_action(cx.listener(Self::handle_inline_backspace_action))
                    .on_action(cx.listener(Self::handle_inline_delete_action))
                    .on_action(cx.listener(Self::handle_inline_move_left_action))
                    .on_action(cx.listener(Self::handle_inline_move_right_action))
                    .on_action(cx.listener(Self::handle_inline_select_left_action))
                    .on_action(cx.listener(Self::handle_inline_select_right_action))
                    .on_action(cx.listener(Self::handle_inline_select_all_action))
                    .on_action(cx.listener(Self::handle_inline_move_to_start_action))
                    .on_action(cx.listener(Self::handle_inline_move_to_end_action))
                    .on_action(cx.listener(Self::handle_inline_delete_word_backward_action))
                    .on_action(cx.listener(Self::handle_inline_delete_word_forward_action))
                    .on_action(cx.listener(Self::handle_inline_delete_to_start_action))
                    .on_action(cx.listener(Self::handle_inline_delete_to_end_action))
                    .on_key_down(cx.listener(Self::handle_key_down))
                    .on_scroll_wheel(cx.listener(Self::handle_terminal_scroll_wheel))
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::handle_mouse_down))
                    .on_mouse_move(cx.listener(Self::handle_mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::handle_mouse_up))
                    .on_drop(cx.listener(Self::handle_file_drop))
                    .relative()
                    .flex_1()
                    .w_full()
                    .overflow_hidden()
                    .bg(terminal_surface_bg_hsla)
                    .font_family(font_family.clone())
                    .text_size(font_size)
                    .child(terminal_grid_layer)
                    .children(terminal_scrollbar_overlay)
                    .children(command_palette_overlay)
                    .children(search_overlay),
            )
            .children(toast_overlay)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_scrollbar_overlay_frame_anchors_to_active_pane_geometry() {
        let surface = TerminalViewportGeometry {
            origin_x: 32.0,
            origin_y: 48.0,
            width: 640.0,
            height: 420.0,
        };

        let frame = terminal_scrollbar_overlay_frame(surface);
        assert_eq!(
            frame.left,
            surface.origin_x + surface.width - TERMINAL_SCROLLBAR_GUTTER_WIDTH
        );
        assert_eq!(frame.top, surface.origin_y);
        assert_eq!(frame.width, TERMINAL_SCROLLBAR_GUTTER_WIDTH);
        assert_eq!(frame.height, surface.height);
    }

    #[test]
    fn terminal_scrollbar_overlay_frame_clamps_when_surface_is_narrower_than_gutter() {
        let surface = TerminalViewportGeometry {
            origin_x: 10.0,
            origin_y: 20.0,
            width: 6.0,
            height: 100.0,
        };

        let frame = terminal_scrollbar_overlay_frame(surface);
        assert_eq!(frame.left, surface.origin_x);
        assert_eq!(frame.top, surface.origin_y);
        assert_eq!(frame.width, surface.width);
        assert_eq!(frame.height, surface.height);
    }

    #[test]
    fn pane_focus_active_border_alpha_is_zero_in_tmux_runtime() {
        let alpha = effective_pane_focus_active_border_alpha(0.38, true, false);
        assert_eq!(alpha, 0.0);
    }

    #[test]
    fn pane_focus_active_border_alpha_is_unchanged_in_native_runtime() {
        let alpha = effective_pane_focus_active_border_alpha(0.38, false, false);
        assert_eq!(alpha, 0.38);
    }

    #[test]
    fn pane_focus_active_border_alpha_is_unchanged_when_tmux_border_is_enabled() {
        let alpha = effective_pane_focus_active_border_alpha(0.38, true, true);
        assert_eq!(alpha, 0.38);
    }
}
