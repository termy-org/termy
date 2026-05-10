use crate::colors::TerminalColors;
use crate::config::{self, AppConfig};
use crate::theme_store::{self, ThemeStoreTheme};
use gpui::{
    Animation, AnimationExt, AnyElement, App, AppContext, AsyncApp, Bounds, Context, FocusHandle,
    FontWeight, InteractiveElement, IntoElement, KeyDownEvent, MouseButton, MouseDownEvent,
    ObjectFit, ParentElement, Render, Rgba, ScrollHandle, SharedString,
    StatefulInteractiveElement, Styled, StyledImage, WeakEntity, Window,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, div, ease_out_quint, img,
    pulsating_between, px, size,
};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use termy_config_core::RootSettingId;
use termy_themes::{Rgb8, ThemeColors, parse_theme_colors_json};

mod components;
mod sections;
mod state;
mod style;

use state::{CursorChoice, FontChoice, Step, TabsChoice};

const ONBOARDING_WINDOW_WIDTH: f32 = 760.0;
const ONBOARDING_WINDOW_HEIGHT: f32 = 600.0;
const RECOMMENDED_THEME_LIMIT: usize = 9;

pub(crate) fn config_file_exists() -> bool {
    termy_config_core::config_path()
        .map(|path| path.exists())
        .unwrap_or(false)
}

fn force_onboarding_env() -> bool {
    std::env::var("TERMY_FORCE_ONBOARDING")
        .ok()
        .filter(|value| !value.is_empty() && value != "0")
        .is_some()
}

pub(crate) fn should_show_onboarding(was_first_run: bool, config: &AppConfig) -> bool {
    if force_onboarding_env() {
        return true;
    }
    if was_first_run {
        return true;
    }
    !config.onboarding_complete
}

fn mark_complete_in_config() {
    if let Err(error) = config::set_root_setting(RootSettingId::OnboardingComplete, "true") {
        log::error!("Failed to mark onboarding complete: {}", error);
    }
}

fn fetch_theme_colors(file_url: &str) -> Result<ThemeColors, String> {
    let response = ureq::builder()
        .timeout(Duration::from_secs(8))
        .build()
        .get(file_url)
        .set("Accept", "application/json")
        .call()
        .map_err(|error| format!("fetch failed: {error}"))?;
    let body = response
        .into_string()
        .map_err(|error| format!("read failed: {error}"))?;
    parse_theme_colors_json(&body).map_err(|error| format!("parse failed: {error}"))
}

fn rgba_from_rgb8(color: Rgb8) -> Rgba {
    Rgba {
        r: color.r as f32 / 255.0,
        g: color.g as f32 / 255.0,
        b: color.b as f32 / 255.0,
        a: 1.0,
    }
}

pub struct OnboardingWindow {
    step: Step,
    focus_handle: FocusHandle,
    colors: TerminalColors,
    content_scroll_handle: ScrollHandle,

    themes: Vec<ThemeStoreTheme>,
    themes_loading: bool,
    themes_error: Option<String>,
    theme_previews: HashMap<String, ThemeColors>,
    selected_theme_slug: Option<String>,
    installing_theme: bool,
    installed_theme_slug: Option<String>,

    cursor_choice: CursorChoice,
    tabs_choice: TabsChoice,
    font_choice: FontChoice,
    background_opacity: f32,

    step_token: u64,
    finalizing: bool,
}

impl OnboardingWindow {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        let default_config = AppConfig::default();
        let colors = TerminalColors::from_theme(&default_config.theme, &default_config.colors);
        let mut view = Self {
            step: Step::Welcome,
            focus_handle: cx.focus_handle(),
            colors,
            content_scroll_handle: ScrollHandle::new(),
            themes: Vec::new(),
            themes_loading: false,
            themes_error: None,
            theme_previews: HashMap::new(),
            selected_theme_slug: None,
            installing_theme: false,
            installed_theme_slug: None,
            cursor_choice: CursorChoice::Blink,
            tabs_choice: TabsChoice::Horizontal,
            font_choice: FontChoice::Default,
            background_opacity: 1.0,
            step_token: 0,
            finalizing: false,
        };
        view.kick_off_theme_fetch(cx);
        view
    }
}

impl Render for OnboardingWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let bg = self.colors.background;
        let body = match self.step {
            Step::Welcome => self.render_welcome(cx),
            Step::Theme => self.render_theme(cx),
            Step::Settings => self.render_settings(cx),
            Step::Done => self.render_done(cx),
        };
        let animated_body = div().w_full().child(body).with_animation(
            SharedString::from(format!("onboarding-step-{}", self.step_token)),
            Animation::new(Duration::from_millis(360)).with_easing(ease_out_quint()),
            |this, delta| this.opacity(delta),
        );

        div()
            .id("onboarding-root")
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::handle_key_down))
            .size_full()
            .bg(bg)
            .text_color(self.text_primary())
            .flex()
            .flex_col()
            .child(self.render_progress())
            .child(
                div()
                    .id("onboarding-content")
                    .flex_1()
                    .min_h(px(0.0))
                    .w_full()
                    .overflow_y_scroll()
                    .track_scroll(&self.content_scroll_handle)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .justify_center()
                            .min_h_full()
                            .px_8()
                            .py_6()
                            .child(animated_body),
                    ),
            )
    }
}

pub(crate) fn open_onboarding_window(cx: &mut App) -> Result<(), String> {
    let initial_size = size(px(ONBOARDING_WINDOW_WIDTH), px(ONBOARDING_WINDOW_HEIGHT));
    let bounds = Bounds::centered(None, initial_size, cx);

    #[cfg(target_os = "macos")]
    let titlebar = Some(gpui::TitlebarOptions {
        title: Some("Welcome to Termy".into()),
        appears_transparent: true,
        traffic_light_position: Some(gpui::point(px(12.0), px(10.0))),
    });
    #[cfg(target_os = "windows")]
    let titlebar = Some(gpui::TitlebarOptions {
        title: Some("Welcome to Termy".into()),
        appears_transparent: false,
        traffic_light_position: None,
    });
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let titlebar = Some(gpui::TitlebarOptions {
        title: Some("Welcome to Termy".into()),
        appears_transparent: true,
        traffic_light_position: None,
    });

    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar,
            window_background: WindowBackgroundAppearance::Opaque,
            is_movable: true,
            is_resizable: false,
            window_min_size: Some(initial_size),
            ..Default::default()
        },
        |window, cx| cx.new(|cx| OnboardingWindow::new(window, cx)),
    )
    .map(|_| ())
    .map_err(|error| format!("Failed to open onboarding window: {}", error))
}

#[cfg(test)]
mod tests {
    use super::Step;

    #[test]
    fn step_indices_cover_total() {
        assert_eq!(Step::Welcome.index(), 0);
        assert_eq!(Step::Theme.index(), 1);
        assert_eq!(Step::Settings.index(), 2);
        assert_eq!(Step::Done.index(), Step::total() - 1);
    }
}
