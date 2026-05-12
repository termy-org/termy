use super::*;

impl OnboardingWindow {
    pub(super) fn render_progress(&self) -> AnyElement {
        let accent = self.accent();
        let muted_active = {
            let mut color = accent;
            color.a = 0.55;
            color
        };
        let inactive = {
            let mut color = self.colors.foreground;
            color.a = 0.20;
            color
        };
        let current_index = self.step.index();
        let mut row = div().flex().items_center().justify_center().gap_2().py_4();
        for index in 0..Step::total() {
            let fill = if index == current_index {
                accent
            } else if index < current_index {
                muted_active
            } else {
                inactive
            };
            let dot = div().w(px(8.0)).h(px(8.0)).rounded_full().bg(fill);
            if index == current_index {
                row = row.child(
                    dot.with_animation(
                        SharedString::from(format!("onboarding-dot-pulse-{}", self.step_token)),
                        Animation::new(Duration::from_millis(1600))
                            .repeat()
                            .with_easing(pulsating_between(0.55, 1.0)),
                        |this, delta| this.opacity(delta),
                    ),
                );
            } else {
                row = row.child(dot);
            }
        }
        row.into_any_element()
    }

    pub(super) fn render_primary_button(
        &self,
        id: &'static str,
        label: SharedString,
        enabled: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let accent = self.accent();
        let bg = if enabled {
            accent
        } else {
            self.accent_with_alpha(0.32)
        };
        let text_color = if enabled {
            self.colors.background
        } else {
            self.text_muted()
        };

        let mut element = div()
            .id(SharedString::from(id))
            .flex()
            .items_center()
            .justify_center()
            .px_5()
            .py_2()
            .min_w(px(150.0))
            .rounded(px(8.0))
            .bg(bg)
            .text_color(text_color)
            .text_sm()
            .font_weight(FontWeight::SEMIBOLD)
            .child(label);

        if enabled {
            let hover_bg = self.accent_with_alpha(0.85);
            element = element
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        on_click(view, window, cx);
                    }),
                );
        }

        element.into_any_element()
    }

    pub(super) fn render_secondary_button(
        &self,
        id: &'static str,
        label: SharedString,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        let bg = self.bg_card();
        let hover_bg = self.bg_card_hover();
        let text_color = self.text_secondary();
        div()
            .id(SharedString::from(id))
            .flex()
            .items_center()
            .justify_center()
            .px_5()
            .py_2()
            .rounded(px(8.0))
            .bg(bg)
            .text_color(text_color)
            .text_sm()
            .font_weight(FontWeight::MEDIUM)
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .child(label)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _event: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    on_click(view, window, cx);
                }),
            )
            .into_any_element()
    }

    pub(super) fn render_step_heading(
        &self,
        title: &'static str,
        subtitle: &'static str,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .items_center()
            .text_center()
            .child(
                div()
                    .text_2xl()
                    .font_weight(FontWeight::BOLD)
                    .text_color(self.text_primary())
                    .child(title),
            )
            .child(
                div()
                    .text_sm()
                    .text_color(self.text_muted())
                    .child(subtitle),
            )
            .into_any_element()
    }

    pub(super) fn render_choice_chip<T: Copy + PartialEq + 'static>(
        &self,
        id: &'static str,
        label: &'static str,
        description: Option<&'static str>,
        is_selected: bool,
        cx: &mut Context<Self>,
        on_click: impl Fn(&mut Self, &mut Context<Self>) + 'static,
        _phantom: std::marker::PhantomData<T>,
    ) -> AnyElement {
        let card_bg = if is_selected {
            self.accent_with_alpha(0.12)
        } else {
            self.bg_card()
        };
        let border = if is_selected {
            self.accent_with_alpha(0.85)
        } else {
            self.border_color()
        };
        let hover_bg = self.bg_card_hover();
        let label_color = if is_selected {
            self.text_primary()
        } else {
            self.text_secondary()
        };
        let mut chip = div()
            .id(SharedString::from(id))
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap_1()
            .px_4()
            .py_3()
            .min_w(px(110.0))
            .rounded(px(8.0))
            .bg(card_bg)
            .border_1()
            .border_color(border)
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .text_color(label_color)
            .child(div().text_sm().font_weight(FontWeight::MEDIUM).child(label))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _event: &MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                    on_click(view, cx);
                }),
            );
        if let Some(description) = description {
            chip = chip.child(
                div()
                    .text_xs()
                    .text_color(self.text_muted())
                    .child(description),
            );
        }
        chip.into_any_element()
    }

    pub(super) fn render_question_row(
        &self,
        title: &'static str,
        options: AnyElement,
    ) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .w_full()
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(self.text_secondary())
                    .child(title),
            )
            .child(options)
            .into_any_element()
    }

    pub(super) fn render_import_source_card(
        &self,
        source: &DetectedSource,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let kind = source.kind;
        let importable = source.importable();
        let card_bg = if is_selected {
            self.accent_with_alpha(0.10)
        } else {
            self.bg_card()
        };
        let border = if is_selected {
            self.accent_with_alpha(0.85)
        } else {
            self.border_color()
        };
        let hover_bg = self.bg_card_hover();
        let title_color = if importable {
            self.text_primary()
        } else {
            self.text_muted()
        };
        let muted = self.text_muted();
        let accent = self.accent();

        let mut chips = div().flex().flex_wrap().gap_2().items_center();
        chips = chips.child(self.render_status_chip(
            "App installed",
            source.app_installed,
            accent,
            muted,
        ));
        chips = chips.child(self.render_status_chip(
            "Config found",
            source.config_path.is_some(),
            accent,
            muted,
        ));

        let hint_text: Option<SharedString> = if let Some(hint) = source.status_hint.clone() {
            Some(SharedString::from(hint))
        } else if !importable {
            Some(SharedString::from("No config file detected"))
        } else {
            source
                .config_path
                .as_ref()
                .map(|path| SharedString::from(path.display().to_string()))
        };

        let icon_element: Option<AnyElement> = source.icon_path.as_ref().map(|path| {
            img(Path::new(path.as_path()))
                .w(px(32.0))
                .h(px(32.0))
                .object_fit(ObjectFit::Contain)
                .into_any_element()
        });

        let mut title_row = div().flex().items_center().gap_3().min_w(px(0.0));
        if let Some(icon) = icon_element {
            title_row = title_row.child(icon);
        }
        title_row = title_row.child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .text_base()
                .font_weight(FontWeight::SEMIBOLD)
                .whitespace_nowrap()
                .text_ellipsis()
                .child(SharedString::from(kind.display_name())),
        );

        let mut card = div()
            .id(SharedString::from(format!(
                "onboarding-import-{}",
                kind.slug()
            )))
            .flex()
            .flex_col()
            .gap_2()
            .p_4()
            .w(px(300.0))
            .min_h(px(116.0))
            .rounded(px(10.0))
            .bg(card_bg)
            .border_1()
            .border_color(border)
            .text_color(title_color)
            .child(title_row)
            .child(chips);

        if let Some(text) = hint_text {
            card = card.child(div().text_xs().text_color(muted).child(text));
        }

        if importable {
            card = card
                .cursor_pointer()
                .hover(move |s| s.bg(hover_bg))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |view, _event: &MouseDownEvent, _window, cx| {
                        cx.stop_propagation();
                        view.select_import_source(kind, cx);
                    }),
                );
        } else {
            card = card.opacity(0.55);
        }

        card.into_any_element()
    }

    fn render_status_chip(
        &self,
        label: &'static str,
        active: bool,
        accent: Rgba,
        muted: Rgba,
    ) -> AnyElement {
        let (bg, color) = if active {
            let mut bg = accent;
            bg.a = 0.18;
            (bg, accent)
        } else {
            let mut bg = muted;
            bg.a = 0.10;
            (bg, muted)
        };
        let prefix = if active { "✓" } else { "·" };
        div()
            .flex()
            .items_center()
            .gap_1()
            .px_2()
            .py_1()
            .rounded(px(999.0))
            .bg(bg)
            .text_xs()
            .text_color(color)
            .child(SharedString::from(format!("{prefix} {label}")))
            .into_any_element()
    }

    pub(super) fn render_theme_card(
        &self,
        theme: &ThemeStoreTheme,
        is_selected: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let slug = theme.slug.clone();
        let preview_slug = theme.slug.trim().to_ascii_lowercase();
        let card_bg = if is_selected {
            self.accent_with_alpha(0.10)
        } else {
            self.bg_card()
        };
        let border = if is_selected {
            self.accent_with_alpha(0.85)
        } else {
            self.border_color()
        };
        let hover_bg = self.bg_card_hover();
        let text_primary = self.text_primary();
        let text_muted = self.text_muted();

        let preview = self.theme_previews.get(&preview_slug);
        let (swatch_bg, swatch_fg, swatch_accent) = if let Some(colors) = preview {
            (
                rgba_from_rgb8(colors.background),
                rgba_from_rgb8(colors.foreground),
                rgba_from_rgb8(colors.cursor),
            )
        } else {
            let mut placeholder = self.text_primary();
            placeholder.a = 0.18;
            (placeholder, placeholder, placeholder)
        };

        let swatch_border = {
            let mut color = self.text_primary();
            color.a = 0.22;
            color
        };

        let swatches = div()
            .flex()
            .items_center()
            .gap_1()
            .child(
                div()
                    .w(px(14.0))
                    .h(px(14.0))
                    .rounded(px(3.0))
                    .bg(swatch_bg)
                    .border_1()
                    .border_color(swatch_border),
            )
            .child(
                div()
                    .w(px(14.0))
                    .h(px(14.0))
                    .rounded(px(3.0))
                    .bg(swatch_fg)
                    .border_1()
                    .border_color(swatch_border),
            )
            .child(
                div()
                    .w(px(14.0))
                    .h(px(14.0))
                    .rounded(px(3.0))
                    .bg(swatch_accent)
                    .border_1()
                    .border_color(swatch_border),
            );

        let name = theme.name.clone();
        let description = theme.description.clone();

        div()
            .id(SharedString::from(format!(
                "onboarding-theme-{}",
                theme.slug
            )))
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .w(px(210.0))
            .h(px(96.0))
            .rounded(px(10.0))
            .bg(card_bg)
            .border_1()
            .border_color(border)
            .cursor_pointer()
            .hover(move |s| s.bg(hover_bg))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(text_primary)
                            .child(SharedString::from(name)),
                    )
                    .child(swatches),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(text_muted)
                    .line_height(px(15.0))
                    .child(SharedString::from(description)),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, _event: &MouseDownEvent, _window, cx| {
                    cx.stop_propagation();
                    view.selected_theme_slug = Some(slug.clone());
                    cx.notify();
                }),
            )
            .into_any_element()
    }
}
