use super::*;

impl SettingsWindow {
    fn masked_secret_value(text: &str) -> String {
        if text.is_empty() || text.eq_ignore_ascii_case("not configured") {
            "Not configured".to_string()
        } else {
            "••••••••".to_string()
        }
    }

    pub(super) fn wrap_setting_with_scroll_anchor(
        &self,
        setting_key: &'static str,
        content: AnyElement,
    ) -> AnyElement {
        // Anchor wrappers must participate in width layout; otherwise `w_full()` row
        // children can resolve against content size and render far beyond the viewport.
        div()
            .id(SharedString::from(format!("setting-{setting_key}")))
            .w_full()
            .min_w(px(0.0))
            .anchor_scroll(self.setting_scroll_anchors.get(setting_key).cloned())
            .child(content)
            .into_any_element()
    }

    pub(super) fn render_section_header(
        &self,
        title: &'static str,
        subtitle: &'static str,
        section: SettingsSection,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bg_input = self.bg_input();
        let hover_bg = self.bg_hover();
        let border_color = self.border_color();
        let text_primary = self.text_primary();
        let text_muted = self.text_muted();
        let text_secondary = self.text_secondary();
        let can_reset = self.section_has_non_default_values(section);
        let mut disabled_bg = bg_input;
        disabled_bg.a *= 0.6;
        let mut disabled_border = border_color;
        disabled_border.a *= 0.6;
        let mut disabled_text = text_muted;
        disabled_text.a *= 0.6;

        div()
            .flex()
            .items_end()
            .justify_between()
            .gap_4()
            .mb_6()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .text_xl()
                            .font_weight(gpui::FontWeight::BOLD)
                            .text_color(text_primary)
                            .child(title),
                    )
                    .child(div().text_sm().text_color(text_muted).child(subtitle)),
            )
            .child(
                div()
                    .id(SharedString::from(format!("reset-section-{section:?}")))
                    .px_3()
                    .py_2()
                    .rounded(px(0.0))
                    .bg(if can_reset { bg_input } else { disabled_bg })
                    .border_1()
                    .border_color(if can_reset {
                        border_color
                    } else {
                        disabled_border
                    })
                    .text_sm()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(if can_reset {
                        text_secondary
                    } else {
                        disabled_text
                    })
                    .when(can_reset, |s| {
                        s.cursor_pointer()
                            .hover(move |s| s.bg(hover_bg).text_color(text_primary))
                    })
                    .child("Reset section")
                    .when(can_reset, |s| {
                        s.on_click(cx.listener(move |view, _, _, cx| {
                            view.confirm_reset_section_to_defaults(section, cx);
                        }))
                    }),
            )
    }

    pub(super) fn render_group_header(&self, title: &'static str) -> impl IntoElement {
        div()
            .text_xs()
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(self.text_muted())
            .mt_4()
            .mb_2()
            .child(title)
    }

    pub(super) fn render_reset_setting_button(
        &self,
        setting_key: &'static str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let bg_input = self.bg_input();
        let hover_bg = self.bg_hover();
        let border_color = self.border_color();
        let text_primary = self.text_primary();
        let text_muted = self.text_muted();
        let can_reset = !self.is_setting_at_default(setting_key);
        let mut disabled_bg = bg_input;
        disabled_bg.a *= 0.6;
        let mut disabled_border = border_color;
        disabled_border.a *= 0.6;
        let mut disabled_text = text_muted;
        disabled_text.a *= 0.6;

        div()
            .id(SharedString::from(format!("reset-setting-{setting_key}")))
            .px_2()
            .py_1()
            .rounded(px(0.0))
            .bg(if can_reset { bg_input } else { disabled_bg })
            .border_1()
            .border_color(if can_reset {
                border_color
            } else {
                disabled_border
            })
            .text_xs()
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(if can_reset { text_muted } else { disabled_text })
            .when(can_reset, |s| {
                s.cursor_pointer()
                    .hover(move |s| s.bg(hover_bg).text_color(text_primary))
            })
            .child("Reset")
            .when(can_reset, |s| {
                s.on_click(cx.listener(move |view, _, _, cx| {
                    view.confirm_reset_setting_to_default(setting_key, cx);
                }))
            })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_setting_row(
        &self,
        search_key: &'static str,
        id: &'static str,
        title: &'static str,
        description: &'static str,
        checked: bool,
        cx: &mut Context<Self>,
        on_toggle: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let is_search_match = self.setting_matches_sidebar_query(search_key);
        let row_border = if is_search_match {
            self.accent_with_alpha(0.55)
        } else {
            self.border_color()
        };
        let row_bg = if is_search_match {
            self.accent_with_alpha(0.08)
        } else {
            self.bg_card()
        };
        let toggle_label_color = if checked {
            self.accent()
        } else {
            self.text_muted()
        };

        // Keep toggle rows shrink-safe in narrow content areas.
        // `justify_between` with a growing left column can inflate intrinsic width
        // and push the control cluster beyond the visible viewport.
        let row = div()
            .w_full()
            .flex()
            .items_center()
            .gap_4()
            .py_3()
            .px_4()
            .rounded(px(0.0))
            .bg(row_bg)
            .border_1()
            .border_color(row_border)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(self.text_primary())
                            .child(title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(self.text_muted())
                            .line_height(px(17.0))
                            .child(description),
                    ),
            )
            .child(
                div()
                    .ml_auto()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .w(px(24.0))
                            .text_xs()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(toggle_label_color)
                            .text_align(TextAlign::Center)
                            .child(if checked { "On" } else { "Off" }),
                    )
                    .child(self.render_switch(id, checked, cx, on_toggle))
                    .child(self.render_reset_setting_button(search_key, cx)),
            );

        self.wrap_setting_with_scroll_anchor(search_key, row.into_any_element())
    }

    pub(super) fn render_switch(
        &self,
        id: impl Into<SharedString>,
        checked: bool,
        cx: &mut Context<Self>,
        on_toggle: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> impl IntoElement {
        let accent = self.accent_with_alpha(0.95);
        let mut bg_off = self.colors.foreground;
        bg_off.a = 0.34;
        let track_color = if checked { accent } else { bg_off };
        let knob_color = self.contrasting_text_for_fill(track_color, self.bg_card());
        let knob_left = if checked {
            SETTINGS_SWITCH_WIDTH - SETTINGS_SWITCH_KNOB_SIZE - 2.0
        } else {
            2.0
        };

        div()
            .id(id.into())
            .w(px(SETTINGS_SWITCH_WIDTH))
            .h(px(SETTINGS_SWITCH_HEIGHT))
            .rounded(px(0.0))
            .bg(track_color)
            .border_1()
            .border_color(self.border_color())
            .cursor_pointer()
            .relative()
            .child(
                div()
                    .absolute()
                    .top(px(2.0))
                    .left(px(knob_left))
                    .w(px(SETTINGS_SWITCH_KNOB_SIZE))
                    .h(px(SETTINGS_SWITCH_KNOB_SIZE))
                    .rounded(px(0.0))
                    .bg(knob_color)
                    .shadow_sm(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _event: &MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                    on_toggle(view, cx);
                    cx.notify();
                }),
            )
    }

    pub(super) fn active_dropdown_options(
        &self,
        field: EditableField,
        is_active: bool,
        uses_dropdown: bool,
    ) -> Vec<DropdownOption> {
        if !uses_dropdown || !is_active {
            return Vec::new();
        }
        let query = self
            .active_input
            .as_ref()
            .map(|input| input.state.text())
            .unwrap_or("");
        if Self::field_uses_click_only_dropdown(field) {
            self.dropdown_options_for_field(field, "")
        } else {
            self.dropdown_options_for_field(field, query)
        }
    }

    pub(super) fn editable_dropdown_overlay(
        &self,
        field: EditableField,
        options: Vec<DropdownOption>,
        text_secondary: Rgba,
        hover_bg: Rgba,
        border_color: Rgba,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if options.is_empty() {
            return None;
        }

        let mut list = div().flex().flex_col().py_1();
        for (index, option) in options.into_iter().enumerate() {
            let option_label = option.display_text();
            let option_value = option.value.clone();
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

        let mut dropdown_bg = self.colors.background;
        dropdown_bg.a = 1.0;
        Some(
            deferred(
                div()
                    .id(SharedString::from(format!(
                        "dropdown-suggestions-{field:?}"
                    )))
                    .occlude()
                    .absolute()
                    .top(px(SETTINGS_CONTROL_HEIGHT + 2.0))
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
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn editable_row_value_element(
        &self,
        field: EditableField,
        is_numeric: bool,
        is_active: bool,
        uses_text_input: bool,
        uses_dropdown: bool,
        display_value: String,
        text_secondary: Rgba,
        bg_card: Rgba,
        text_primary: Rgba,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_secret_field = Self::is_secret_field(field);
        let readonly_display_value = if !is_active && uses_dropdown {
            self.dropdown_display_value(field, &display_value)
        } else if !is_active && is_secret_field {
            Self::masked_secret_value(&display_value)
        } else {
            display_value.clone()
        };

        if is_numeric {
            return div()
                .h_full()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
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
                .into_any_element();
        }

        if is_active && uses_text_input {
            if is_secret_field {
                let mut hidden_text = text_secondary;
                hidden_text.a = 0.0;
                let mut hidden_selection = self.accent_with_alpha(0.3);
                hidden_selection.a = 0.0;
                let masked_active_value = self
                    .active_input
                    .as_ref()
                    .filter(|input| input.field == field)
                    .map(|input| Self::masked_secret_value(input.state.text()))
                    .unwrap_or_else(|| Self::masked_secret_value(&display_value));

                let font = Font {
                    family: self.config.font_family.clone().into(),
                    ..Font::default()
                };

                return div()
                    .relative()
                    .size_full()
                    .child(TextInputElement::new(
                        cx.entity(),
                        self.focus_handle.clone(),
                        font,
                        px(SETTINGS_INPUT_TEXT_SIZE),
                        hidden_text.into(),
                        hidden_selection.into(),
                        TextInputAlignment::Left,
                    ))
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .h_full()
                            .flex()
                            .items_center()
                            .text_size(px(SETTINGS_INPUT_TEXT_SIZE))
                            .text_color(text_secondary)
                            .child(masked_active_value),
                    )
                    .into_any_element();
            }
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

        let mut readonly = div()
            .h_full()
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .flex_1()
                    .text_size(px(SETTINGS_INPUT_TEXT_SIZE))
                    .text_color(text_secondary)
                    .child(readonly_display_value),
            );
        if field == EditableField::Theme {
            readonly = readonly.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(div().w(px(10.0)).h(px(10.0)).bg(self.colors.background))
                    .child(div().w(px(10.0)).h(px(10.0)).bg(self.colors.foreground))
                    .child(div().w(px(10.0)).h(px(10.0)).bg(self.colors.cursor)),
            );
        } else if field == EditableField::FontFamily {
            readonly = readonly.child(
                div()
                    .text_xs()
                    .text_color(self.text_muted())
                    .font_family(self.config.font_family.clone())
                    .child("Ag"),
            );
        }
        if uses_dropdown {
            readonly = readonly.child(
                div()
                    .text_xs()
                    .text_color(self.text_muted())
                    .child(if is_active { "▲" } else { "▼" }),
            );
        }
        readonly.into_any_element()
    }

    pub(super) fn handle_editable_row_mouse_down(
        &mut self,
        field: EditableField,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        if !self
            .active_input
            .as_ref()
            .is_some_and(|input| input.field == field)
        {
            self.begin_editing_field(field, window, cx);
        }
        if let Some(input) = self.active_input.as_mut() {
            if Self::uses_text_input_for_field(field) {
                let index = input.state.character_index_for_point(event.position);
                if event.modifiers.shift {
                    input.state.select_to_utf16(index);
                } else {
                    input.state.set_cursor_utf16(index);
                }
                input.selecting = true;
            } else {
                input.selecting = false;
            }
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(super) fn handle_editable_row_mouse_move(
        &mut self,
        field: EditableField,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(input) = self.active_input.as_mut() else {
            return;
        };
        if input.field != field || !input.selecting || !event.dragging() {
            return;
        }
        let index = input.state.character_index_for_point(event.position);
        input.state.select_to_utf16(index);
        cx.notify();
    }

    pub(super) fn handle_editable_row_mouse_up(
        &mut self,
        field: EditableField,
        cx: &mut Context<Self>,
    ) {
        if let Some(input) = self.active_input.as_mut()
            && input.field == field
        {
            input.selecting = false;
            cx.notify();
        }
    }

    pub(super) fn render_editable_row(
        &mut self,
        search_key: &'static str,
        field: EditableField,
        title: &'static str,
        description: &'static str,
        display_value: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if field == EditableField::BackgroundOpacity {
            return self.render_background_opacity_row(search_key, title, description, cx);
        }
        let is_numeric = Self::is_numeric_field(field);
        let uses_text_input = Self::uses_text_input_for_field(field);
        let is_active = self
            .active_input
            .as_ref()
            .is_some_and(|input| input.field == field);
        let uses_dropdown = Self::field_uses_dropdown(field);
        let accent_inner_border = is_numeric || uses_dropdown;
        let text_secondary = self.text_secondary();
        let hover_bg = self.bg_hover();
        let input_bg = self.bg_input();
        let border_color = self.border_color();
        let accent = self.accent();
        let bg_card = self.bg_card();
        let text_primary = self.text_primary();
        let text_muted = self.text_muted();
        let row_border = if self.setting_matches_sidebar_query(search_key) {
            self.accent_with_alpha(0.55)
        } else {
            self.border_color()
        };
        let row_bg = if self.setting_matches_sidebar_query(search_key) {
            self.accent_with_alpha(0.08)
        } else {
            self.bg_card()
        };
        let dropdown_options = self.active_dropdown_options(field, is_active, uses_dropdown);
        let dropdown_open = is_active && uses_dropdown && !dropdown_options.is_empty();
        let dropdown = self.editable_dropdown_overlay(
            field,
            dropdown_options,
            text_secondary,
            hover_bg,
            border_color,
            cx,
        );
        let value_element = self.editable_row_value_element(
            field,
            is_numeric,
            is_active,
            uses_text_input,
            uses_dropdown,
            display_value,
            text_secondary,
            bg_card,
            text_primary,
            cx,
        );

        let row = div()
            .id(SharedString::from(format!("editable-row-{field:?}")))
            .w_full()
            .flex()
            .items_center()
            .gap_4()
            .py_3()
            .px_4()
            .rounded(px(0.0))
            .bg(row_bg)
            .border_1()
            .border_color(if dropdown_open {
                Rgba::default()
            } else {
                row_border
            })
            .cursor_pointer()
            .when(!is_numeric, |s| {
                s.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                        view.handle_editable_row_mouse_down(field, event, window, cx);
                    }),
                )
                .on_mouse_move(
                    cx.listener(move |view, event: &MouseMoveEvent, _window, cx| {
                        view.handle_editable_row_mouse_move(field, event, cx);
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(move |view, _event: &MouseUpEvent, _window, cx| {
                        view.handle_editable_row_mouse_up(field, cx);
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(move |view, _event: &MouseUpEvent, _window, cx| {
                        view.handle_editable_row_mouse_up(field, cx);
                    }),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(text_primary)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(text_muted)
                            .line_height(px(17.0))
                            .child(description),
                    ),
            )
            .child(
                div()
                    .ml_auto()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(SETTINGS_CONTROL_WIDTH))
                            .relative()
                            .h(px(SETTINGS_CONTROL_HEIGHT))
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
                                        accent
                                    } else {
                                        border_color
                                    })
                                    .overflow_hidden()
                                    .child(value_element),
                            )
                            .when_some(dropdown, |s, dropdown| s.child(dropdown)),
                    )
                    .child(self.render_reset_setting_button(search_key, cx)),
            );
        self.wrap_setting_with_scroll_anchor(search_key, row.into_any_element())
    }

    pub(super) fn background_opacity_slider_width() -> f32 {
        let fixed = SETTINGS_SLIDER_VALUE_WIDTH + (NUMERIC_STEP_BUTTON_SIZE * 2.0);
        let gaps = SETTINGS_OPACITY_CONTROL_GAP * 3.0;
        (SETTINGS_CONTROL_WIDTH - (SETTINGS_CONTROL_INNER_PADDING * 2.0) - fixed - gaps).max(80.0)
    }

    pub(super) fn quantize_background_opacity_ratio(ratio: f32) -> f32 {
        let step = SETTINGS_OPACITY_STEP_RATIO;
        ((ratio.clamp(0.0, 1.0) / step).round() * step).clamp(0.0, 1.0)
    }

    pub(super) fn set_background_opacity_preview(&mut self, ratio: f32) -> bool {
        let ratio = Self::quantize_background_opacity_ratio(ratio);
        if (self.config.background_opacity - ratio).abs() < f32::EPSILON {
            return false;
        }
        self.config.background_opacity = ratio;
        true
    }

    pub(super) fn commit_background_opacity(&mut self) -> Result<(), String> {
        config::set_root_setting(
            RootSettingId::BackgroundOpacity,
            &format!("{:.3}", self.config.background_opacity),
        )
    }

    pub(super) fn begin_background_opacity_drag(&mut self, pointer_x: f32) {
        self.background_opacity_drag_anchor =
            Some((pointer_x, self.config.background_opacity.clamp(0.0, 1.0)));
    }

    pub(super) fn update_background_opacity_drag(
        &mut self,
        pointer_x: f32,
        slider_width: f32,
    ) -> bool {
        let Some((drag_start_x, drag_start_ratio)) = self.background_opacity_drag_anchor else {
            return false;
        };
        let delta_ratio = (pointer_x - drag_start_x) / slider_width.max(1.0);
        self.set_background_opacity_preview(drag_start_ratio + delta_ratio)
    }

    pub(super) fn set_background_opacity_from_slider_position(
        &mut self,
        pointer_x: f32,
        slider_width: f32,
    ) -> Result<bool, String> {
        let ratio = (pointer_x / slider_width.max(1.0)).clamp(0.0, 1.0);
        if !self.set_background_opacity_preview(ratio) {
            return Ok(false);
        }
        self.commit_background_opacity()?;
        Ok(true)
    }

    pub(super) fn finish_background_opacity_drag(&mut self) -> Result<bool, String> {
        let Some((_, start_ratio)) = self.background_opacity_drag_anchor.take() else {
            return Ok(false);
        };
        if (self.config.background_opacity - start_ratio).abs() < f32::EPSILON {
            return Ok(false);
        }
        self.commit_background_opacity()?;
        Ok(true)
    }

    pub(super) fn step_background_opacity(&mut self, delta: i32) -> Result<bool, String> {
        let next = self.config.background_opacity + (delta as f32 * SETTINGS_OPACITY_STEP_RATIO);
        if !self.set_background_opacity_preview(next) {
            return Ok(false);
        }
        self.commit_background_opacity()?;
        Ok(true)
    }

    pub(super) fn background_opacity_step_button(
        &self,
        id: &'static str,
        label: &'static str,
        delta: i32,
        border_color: Rgba,
        text_primary: Rgba,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .w(px(NUMERIC_STEP_BUTTON_SIZE))
            .h(px(NUMERIC_STEP_BUTTON_SIZE))
            .rounded(px(0.0))
            .cursor_pointer()
            .flex()
            .items_center()
            .justify_center()
            .bg(self.bg_input())
            .border_1()
            .border_color(border_color)
            .text_color(text_primary)
            .font_weight(gpui::FontWeight::BOLD)
            .text_sm()
            .child(label)
            .on_click(cx.listener(move |view, _, _, cx| {
                match view.step_background_opacity(delta) {
                    Ok(true) => termy_toast::success("Saved"),
                    Ok(false) => {}
                    Err(error) => termy_toast::error(error),
                }
                cx.notify();
            }))
            .into_any_element()
    }

    pub(super) fn background_opacity_slider(
        &self,
        slider_width: f32,
        slider_fill_width: f32,
        slider_thumb_left: f32,
        slider_track: Rgba,
        slider_fill: Rgba,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id("background-opacity-slider")
            .relative()
            .w(px(slider_width))
            .h(px(SETTINGS_SWITCH_KNOB_SIZE))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                    let x: f32 = event.position.x.into();
                    match view.set_background_opacity_from_slider_position(x, slider_width) {
                        Ok(_) => {}
                        Err(error) => termy_toast::error(error),
                    }
                    view.begin_background_opacity_drag(x);
                    cx.notify();
                }),
            )
            .on_mouse_move(
                cx.listener(move |view, event: &MouseMoveEvent, _window, cx| {
                    if !event.dragging() {
                        return;
                    }
                    cx.stop_propagation();
                    let x: f32 = event.position.x.into();
                    view.update_background_opacity_drag(x, slider_width);
                    cx.notify();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|view, _event: &MouseUpEvent, _window, cx| {
                    cx.stop_propagation();
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
                    cx.stop_propagation();
                    match view.finish_background_opacity_drag() {
                        Ok(true) => termy_toast::success("Saved"),
                        Ok(false) => {}
                        Err(error) => termy_toast::error(error),
                    }
                    cx.notify();
                }),
            )
            .child(
                div()
                    .absolute()
                    .top(px(7.0))
                    .left_0()
                    .w(px(slider_width))
                    .h(px(4.0))
                    .bg(slider_track),
            )
            .child(
                div()
                    .absolute()
                    .top(px(7.0))
                    .left_0()
                    .w(px(slider_fill_width))
                    .h(px(4.0))
                    .bg(slider_fill),
            )
            .child(
                div()
                    .absolute()
                    .top(px(2.0))
                    .left(px(slider_thumb_left))
                    .w(px(14.0))
                    .h(px(14.0))
                    .bg(self.accent()),
            )
            .into_any_element()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn background_opacity_controls(
        &self,
        slider_width: f32,
        slider_fill_width: f32,
        slider_thumb_left: f32,
        slider_track: Rgba,
        slider_fill: Rgba,
        percentage: String,
        border_color: Rgba,
        text_primary: Rgba,
        text_secondary: Rgba,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .h_full()
            .flex()
            .items_center()
            .gap(px(SETTINGS_OPACITY_CONTROL_GAP))
            .child(self.background_opacity_slider(
                slider_width,
                slider_fill_width,
                slider_thumb_left,
                slider_track,
                slider_fill,
                cx,
            ))
            .child(self.background_opacity_step_button(
                "background-opacity-dec",
                "-",
                -1,
                border_color,
                text_primary,
                cx,
            ))
            .child(
                div()
                    .w(px(SETTINGS_SLIDER_VALUE_WIDTH))
                    .text_sm()
                    .text_color(text_secondary)
                    .text_align(TextAlign::Center)
                    .child(percentage),
            )
            .child(self.background_opacity_step_button(
                "background-opacity-inc",
                "+",
                1,
                border_color,
                text_primary,
                cx,
            ))
            .into_any_element()
    }

    pub(super) fn render_background_opacity_row(
        &mut self,
        search_key: &'static str,
        title: &'static str,
        description: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_search_match = self.setting_matches_sidebar_query(search_key);
        let row_border = if is_search_match {
            self.accent_with_alpha(0.55)
        } else {
            self.border_color()
        };
        let row_bg = if is_search_match {
            self.accent_with_alpha(0.08)
        } else {
            self.bg_card()
        };
        let border_color = self.border_color();
        let input_bg = self.bg_input();
        let text_primary = self.text_primary();
        let text_secondary = self.text_secondary();
        let text_muted = self.text_muted();
        let slider_track = self.bg_hover();
        let slider_fill = self.accent_with_alpha(0.9);
        let slider_width = Self::background_opacity_slider_width();
        let slider_ratio = self.config.background_opacity.clamp(0.0, 1.0);
        let slider_fill_width = slider_ratio * slider_width;
        let slider_thumb_left = (slider_fill_width - 7.0).clamp(0.0, slider_width - 14.0);
        let percentage = format!("{}%", (slider_ratio * 100.0).round() as i32);
        let controls = self.background_opacity_controls(
            slider_width,
            slider_fill_width,
            slider_thumb_left,
            slider_track,
            slider_fill,
            percentage,
            border_color,
            text_primary,
            text_secondary,
            cx,
        );

        let row = div()
            .id("editable-row-background-opacity")
            .w_full()
            .flex()
            .items_center()
            .gap_4()
            .py_3()
            .px_4()
            .rounded(px(0.0))
            .bg(row_bg)
            .border_1()
            .border_color(row_border)
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .text_sm()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(text_primary)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(text_muted)
                            .line_height(px(17.0))
                            .child(description),
                    ),
            )
            .child(
                div()
                    .ml_auto()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(SETTINGS_CONTROL_WIDTH))
                            .h(px(SETTINGS_CONTROL_HEIGHT))
                            .px(px(SETTINGS_CONTROL_INNER_PADDING))
                            .rounded(px(0.0))
                            .bg(input_bg)
                            .border_1()
                            .border_color(border_color)
                            .child(controls),
                    )
                    .child(self.render_reset_setting_button(search_key, cx)),
            );

        self.wrap_setting_with_scroll_anchor(search_key, row.into_any_element())
    }
}
