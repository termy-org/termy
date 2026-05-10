use super::*;

impl OnboardingWindow {
    pub(super) fn render_welcome(&self, cx: &mut Context<Self>) -> AnyElement {
        let halo_outer = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .w(px(180.0))
            .h(px(180.0))
            .rounded_full()
            .bg(self.accent_with_alpha(0.18))
            .with_animation(
                "onboarding-halo-outer",
                Animation::new(Duration::from_millis(3600))
                    .repeat()
                    .with_easing(pulsating_between(0.0, 0.55)),
                |this, delta| this.opacity(delta),
            );
        let halo_inner = div()
            .absolute()
            .top(px(28.0))
            .left(px(28.0))
            .w(px(124.0))
            .h(px(124.0))
            .rounded_full()
            .bg(self.accent_with_alpha(0.32))
            .with_animation(
                "onboarding-halo-inner",
                Animation::new(Duration::from_millis(2600))
                    .repeat()
                    .with_easing(pulsating_between(0.25, 0.95)),
                |this, delta| this.opacity(delta),
            );
        let logo_image = div()
            .absolute()
            .top(px(34.0))
            .left(px(34.0))
            .w(px(112.0))
            .h(px(112.0))
            .child(
                img(Path::new(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/assets/icon.svg"
                )))
                .size_full()
                .object_fit(ObjectFit::Contain),
            );

        let logo_stack = div()
            .relative()
            .w(px(180.0))
            .h(px(180.0))
            .child(halo_outer)
            .child(halo_inner)
            .child(logo_image);

        let title = div()
            .text_color(self.text_primary())
            .font_weight(FontWeight::BOLD)
            .text_3xl()
            .child("Welcome to Termy");
        let subtitle = div()
            .text_color(self.text_muted())
            .text_base()
            .max_w(px(440.0))
            .text_center()
            .child("A fast, native terminal. Let's set it up in under a minute.");

        let primary = self.render_primary_button(
            "onboarding-welcome-next",
            "Get started".into(),
            true,
            cx,
            |view, _window, cx| view.next_step(cx),
        );
        let skip = self.render_secondary_button(
            "onboarding-welcome-skip",
            "Skip setup".into(),
            cx,
            |view, window, cx| view.skip_onboarding(window, cx),
        );

        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_6()
            .child(logo_stack)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .child(title)
                    .child(subtitle),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .mt_4()
                    .child(primary)
                    .child(skip),
            )
            .into_any_element()
    }

    pub(super) fn render_theme(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let heading = self.render_step_heading(
            "Choose a theme",
            "Pick a starting look — you can change or add more later.",
        );

        let body: AnyElement = if self.themes_loading {
            div()
                .text_sm()
                .text_color(self.text_muted())
                .py_8()
                .child("Loading themes…")
                .into_any_element()
        } else if let Some(error) = self.themes_error.clone() {
            let retry = self.render_secondary_button(
                "onboarding-theme-retry",
                "Retry".into(),
                cx,
                |view, _window, cx| view.refresh_themes(cx),
            );
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_3()
                .py_8()
                .child(
                    div()
                        .text_sm()
                        .text_color(self.text_muted())
                        .max_w(px(420.0))
                        .text_center()
                        .child(SharedString::from(error)),
                )
                .child(retry)
                .into_any_element()
        } else if self.themes.is_empty() {
            div()
                .text_sm()
                .text_color(self.text_muted())
                .py_8()
                .child("No themes available right now.")
                .into_any_element()
        } else {
            let mut grid = div()
                .flex()
                .flex_wrap()
                .justify_center()
                .gap_3()
                .max_w(px(680.0));
            let themes = self.themes.clone();
            let selected = self.selected_theme_slug.clone();
            for theme in themes.iter() {
                let is_selected = selected
                    .as_ref()
                    .map(|slug| slug.eq_ignore_ascii_case(&theme.slug))
                    .unwrap_or(false);
                grid = grid.child(self.render_theme_card(theme, is_selected, cx));
            }
            grid.into_any_element()
        };

        let has_selection = self.selected_theme_slug.is_some();
        let primary_label = if self.installing_theme {
            "Installing…"
        } else if has_selection {
            "Install & continue"
        } else {
            "Continue"
        };
        let primary_enabled = !self.installing_theme;
        let primary = self.render_primary_button(
            "onboarding-theme-next",
            primary_label.into(),
            primary_enabled,
            cx,
            |view, _window, cx| {
                if view.selected_theme_slug.is_some() {
                    view.install_selected_theme(cx);
                } else {
                    view.next_step(cx);
                }
            },
        );
        let skip = self.render_secondary_button(
            "onboarding-theme-skip",
            "Skip".into(),
            cx,
            |view, _window, cx| {
                view.selected_theme_slug = None;
                view.next_step(cx);
            },
        );

        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_6()
            .w_full()
            .child(heading)
            .child(body)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(skip)
                    .child(primary),
            )
            .into_any_element()
    }

    pub(super) fn render_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let heading = self.render_step_heading(
            "A few preferences",
            "Pick what feels right. Everything here is editable later in Settings.",
        );

        let font_options = {
            let mut row = div().flex().flex_wrap().gap_2();
            for choice in [
                FontChoice::Compact,
                FontChoice::Default,
                FontChoice::Comfortable,
                FontChoice::Large,
            ] {
                let is_selected = self.font_choice == choice;
                let chip = self.render_choice_chip::<FontChoice>(
                    match choice {
                        FontChoice::Compact => "onboarding-font-compact",
                        FontChoice::Default => "onboarding-font-default",
                        FontChoice::Comfortable => "onboarding-font-comfortable",
                        FontChoice::Large => "onboarding-font-large",
                    },
                    choice.label(),
                    Some(choice.description()),
                    is_selected,
                    cx,
                    move |view, cx| {
                        view.font_choice = choice;
                        cx.notify();
                    },
                    std::marker::PhantomData,
                );
                row = row.child(chip);
            }
            row.into_any_element()
        };

        let tabs_options = {
            let mut row = div().flex().gap_2();
            for choice in [TabsChoice::Horizontal, TabsChoice::Vertical] {
                let is_selected = self.tabs_choice == choice;
                let chip = self.render_choice_chip::<TabsChoice>(
                    match choice {
                        TabsChoice::Horizontal => "onboarding-tabs-horizontal",
                        TabsChoice::Vertical => "onboarding-tabs-vertical",
                    },
                    match choice {
                        TabsChoice::Horizontal => "Horizontal",
                        TabsChoice::Vertical => "Sidebar",
                    },
                    Some(match choice {
                        TabsChoice::Horizontal => "Tabs across the top",
                        TabsChoice::Vertical => "Tabs in a sidebar",
                    }),
                    is_selected,
                    cx,
                    move |view, cx| {
                        view.tabs_choice = choice;
                        cx.notify();
                    },
                    std::marker::PhantomData,
                );
                row = row.child(chip);
            }
            row.into_any_element()
        };

        let cursor_options = {
            let mut row = div().flex().gap_2();
            for choice in [CursorChoice::Blink, CursorChoice::Static] {
                let is_selected = self.cursor_choice == choice;
                let chip = self.render_choice_chip::<CursorChoice>(
                    match choice {
                        CursorChoice::Blink => "onboarding-cursor-blink",
                        CursorChoice::Static => "onboarding-cursor-static",
                    },
                    match choice {
                        CursorChoice::Blink => "Blinking",
                        CursorChoice::Static => "Steady",
                    },
                    None,
                    is_selected,
                    cx,
                    move |view, cx| {
                        view.cursor_choice = choice;
                        cx.notify();
                    },
                    std::marker::PhantomData,
                );
                row = row.child(chip);
            }
            row.into_any_element()
        };

        let opacity_options = {
            let mut row = div().flex().gap_2();
            let presets: [(f32, &'static str, &'static str); 3] = [
                (1.0, "Solid", "100%"),
                (0.92, "Subtle", "92%"),
                (0.78, "Translucent", "78%"),
            ];
            for (value, label, description) in presets {
                let is_selected = (self.background_opacity - value).abs() < 0.001;
                let chip = self.render_choice_chip::<f32>(
                    match label {
                        "Solid" => "onboarding-opacity-solid",
                        "Subtle" => "onboarding-opacity-subtle",
                        _ => "onboarding-opacity-translucent",
                    },
                    label,
                    Some(description),
                    is_selected,
                    cx,
                    move |view, cx| {
                        view.background_opacity = value;
                        cx.notify();
                    },
                    std::marker::PhantomData,
                );
                row = row.child(chip);
            }
            row.into_any_element()
        };

        let questions = div()
            .flex()
            .flex_col()
            .gap_5()
            .w(px(560.0))
            .child(self.render_question_row("Font size", font_options))
            .child(self.render_question_row("Tab style", tabs_options))
            .child(self.render_question_row("Cursor", cursor_options))
            .child(self.render_question_row("Window opacity", opacity_options));

        let primary = self.render_primary_button(
            "onboarding-settings-next",
            "Continue".into(),
            true,
            cx,
            |view, _window, cx| view.next_step(cx),
        );
        let skip = self.render_secondary_button(
            "onboarding-settings-skip",
            "Skip".into(),
            cx,
            |view, window, cx| view.skip_onboarding(window, cx),
        );

        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_6()
            .child(heading)
            .child(questions)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(skip)
                    .child(primary),
            )
            .into_any_element()
    }

    pub(super) fn render_done(&self, cx: &mut Context<Self>) -> AnyElement {
        let halo = div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .w(px(120.0))
            .h(px(120.0))
            .rounded_full()
            .bg(self.accent_with_alpha(0.28))
            .with_animation(
                "onboarding-done-halo",
                Animation::new(Duration::from_millis(2400))
                    .repeat()
                    .with_easing(pulsating_between(0.0, 0.6)),
                |this, delta| this.opacity(delta),
            );
        let check_inner = div()
            .absolute()
            .top(px(24.0))
            .left(px(24.0))
            .flex()
            .items_center()
            .justify_center()
            .w(px(72.0))
            .h(px(72.0))
            .rounded_full()
            .bg(self.accent_with_alpha(0.20))
            .border_1()
            .border_color(self.accent_with_alpha(0.55))
            .child(
                div()
                    .text_color(self.accent())
                    .font_weight(FontWeight::BOLD)
                    .text_2xl()
                    .child("✓"),
            );
        let check = div()
            .relative()
            .w(px(120.0))
            .h(px(120.0))
            .child(halo)
            .child(check_inner);

        let heading = self.render_step_heading(
            "You're all set",
            "Your preferences are saved. You can tweak everything later in Settings.",
        );

        let primary = self.render_primary_button(
            "onboarding-done-finish",
            "Open Termy".into(),
            !self.finalizing,
            cx,
            |view, window, cx| view.finalize(window, cx),
        );

        div()
            .flex()
            .flex_col()
            .items_center()
            .gap_6()
            .child(check)
            .child(heading)
            .child(primary)
            .into_any_element()
    }
}
