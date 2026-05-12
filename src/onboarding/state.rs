use super::*;
use crate::onboarding::import::{
    self, DetectedSource, ImportSourceKind, ImportedConfig, detect_sources,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Step {
    Welcome,
    Import,
    Theme,
    Settings,
    Done,
}

impl Step {
    pub(super) fn index(self) -> usize {
        match self {
            Self::Welcome => 0,
            Self::Import => 1,
            Self::Theme => 2,
            Self::Settings => 3,
            Self::Done => 4,
        }
    }

    pub(super) fn total() -> usize {
        5
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum CursorChoice {
    Blink,
    Static,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum TabsChoice {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(super) enum FontChoice {
    Compact,
    Default,
    Comfortable,
    Large,
}

impl FontChoice {
    pub(super) fn size(self) -> f32 {
        match self {
            Self::Compact => 12.0,
            Self::Default => 14.0,
            Self::Comfortable => 16.0,
            Self::Large => 18.0,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Compact => "Compact",
            Self::Default => "Default",
            Self::Comfortable => "Comfortable",
            Self::Large => "Large",
        }
    }

    pub(super) fn description(self) -> &'static str {
        match self {
            Self::Compact => "12 px",
            Self::Default => "14 px",
            Self::Comfortable => "16 px",
            Self::Large => "18 px",
        }
    }
}

impl OnboardingWindow {
    pub(super) fn kick_off_theme_fetch(&mut self, cx: &mut Context<Self>) {
        if self.themes_loading || !self.themes.is_empty() {
            return;
        }
        self.themes_loading = true;
        self.themes_error = None;
        let registry_url = theme_store::theme_store_registry_url();

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = smol::unblock(move || {
                theme_store::fetch_theme_store_themes_blocking(&registry_url)
            })
            .await;

            let preview_targets = match &result {
                Ok((themes, _)) => themes
                    .iter()
                    .take(RECOMMENDED_THEME_LIMIT)
                    .filter_map(|theme| {
                        theme
                            .file_url
                            .clone()
                            .map(|url| (theme.slug.trim().to_ascii_lowercase(), url))
                    })
                    .collect::<Vec<_>>(),
                Err(_) => Vec::new(),
            };

            cx.update(|cx| {
                let _ = this.update(cx, |view, cx| {
                    view.themes_loading = false;
                    match result {
                        Ok((themes, _from_cache)) => {
                            view.themes =
                                themes.into_iter().take(RECOMMENDED_THEME_LIMIT).collect();
                            view.themes_error = None;
                        }
                        Err(error) => {
                            log::error!("Onboarding registry fetch failed: {error}");
                            view.themes.clear();
                            view.themes_error = Some(error);
                        }
                    }
                    cx.notify();
                });
            });

            for (slug, url) in preview_targets {
                let preview_slug = slug.clone();
                let preview_url = url.clone();
                let preview_this = this.clone();
                cx.spawn(async move |cx: &mut AsyncApp| {
                    let result = smol::unblock({
                        let preview_url = preview_url.clone();
                        move || fetch_theme_colors(&preview_url)
                    })
                    .await;
                    match result {
                        Ok(colors) => {
                            cx.update(|cx| {
                                let _ = preview_this.update(cx, |view, cx| {
                                    view.theme_previews.insert(preview_slug.clone(), colors);
                                    cx.notify();
                                });
                            });
                        }
                        Err(error) => {
                            log::warn!(
                                "Onboarding preview fetch failed for {preview_slug} ({preview_url}): {error}"
                            );
                        }
                    }
                })
                .detach();
            }
        })
        .detach();
    }

    pub(super) fn refresh_themes(&mut self, cx: &mut Context<Self>) {
        self.themes.clear();
        self.theme_previews.clear();
        self.themes_loading = false;
        self.kick_off_theme_fetch(cx);
        cx.notify();
    }

    pub(super) fn go_to(&mut self, step: Step, cx: &mut Context<Self>) {
        if self.step != step {
            self.step_token = self.step_token.wrapping_add(1);
        }
        self.step = step;
        cx.notify();
    }

    pub(super) fn next_step(&mut self, cx: &mut Context<Self>) {
        let next = match self.step {
            Step::Welcome => Step::Import,
            Step::Import => Step::Theme,
            Step::Theme => Step::Settings,
            Step::Settings => Step::Done,
            Step::Done => Step::Done,
        };
        self.go_to(next, cx);
    }

    pub(super) fn ensure_import_sources(&mut self, cx: &mut Context<Self>) {
        if self.import_detected {
            return;
        }
        self.import_detected = true;
        let sources = detect_sources();
        self.import_sources = sources;
        cx.notify();
    }

    pub(super) fn select_import_source(&mut self, kind: ImportSourceKind, cx: &mut Context<Self>) {
        let importable = self
            .import_sources
            .iter()
            .find(|source| source.kind == kind)
            .is_some_and(DetectedSource::importable);
        if !importable {
            return;
        }
        self.selected_source = Some(kind);
        cx.notify();
    }

    pub(super) fn run_selected_import(&mut self, cx: &mut Context<Self>) {
        let Some(kind) = self.selected_source else {
            self.next_step(cx);
            return;
        };
        let Some(source) = self
            .import_sources
            .iter()
            .find(|source| source.kind == kind)
            .cloned()
        else {
            return;
        };
        if !source.importable() || self.importing {
            return;
        }
        self.importing = true;
        self.import_error = None;
        cx.notify();

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let result = smol::unblock(move || import::run_import(&source)).await;
            cx.update(|cx| {
                let _ = this.update(cx, |view, cx| {
                    view.importing = false;
                    match result {
                        Ok(imported) => view.apply_imported_config(imported, cx),
                        Err(error) => {
                            log::error!("Onboarding import failed: {error}");
                            view.import_error = Some(error.clone());
                            termy_toast::error(error);
                        }
                    }
                    cx.notify();
                });
            });
        })
        .detach();
    }

    fn apply_imported_config(&mut self, imported: ImportedConfig, cx: &mut Context<Self>) {
        let mut applied_settings = 0usize;
        let mut errors: Vec<String> = Vec::new();
        let mut applied_font_size: Option<f32> = None;
        let mut applied_cursor_blink: Option<bool> = None;
        let mut applied_opacity: Option<f32> = None;

        for (id, value) in &imported.settings {
            match crate::config::set_root_setting(*id, value) {
                Ok(()) => {
                    applied_settings += 1;
                    match id {
                        RootSettingId::FontSize => {
                            if let Ok(size) = value.parse::<f32>() {
                                applied_font_size = Some(size);
                            }
                        }
                        RootSettingId::CursorBlink => {
                            applied_cursor_blink = Some(value == "true");
                        }
                        RootSettingId::BackgroundOpacity => {
                            if let Ok(opacity) = value.parse::<f32>() {
                                applied_opacity = Some(opacity);
                            }
                        }
                        _ => {}
                    }
                }
                Err(error) => errors.push(error),
            }
        }

        if let Some(size) = applied_font_size {
            self.font_choice = match size {
                s if s <= 12.5 => FontChoice::Compact,
                s if s <= 14.5 => FontChoice::Default,
                s if s <= 16.5 => FontChoice::Comfortable,
                _ => FontChoice::Large,
            };
        }
        if let Some(blink) = applied_cursor_blink {
            self.cursor_choice = if blink {
                CursorChoice::Blink
            } else {
                CursorChoice::Static
            };
        }
        if let Some(opacity) = applied_opacity {
            self.background_opacity = opacity;
        }

        let mut installed_theme_name: Option<String> = None;
        if let Some(colors) = imported.theme.as_ref() {
            let slug = format!("imported-{}", imported.source.slug());
            let display = format!("Imported from {}", imported.source.display_name());
            match crate::theme_store::install_local_theme_blocking(&slug, &display, colors) {
                Ok(installed) => {
                    self.installed_theme_slug = Some(installed.slug);
                    installed_theme_name = Some(display);
                }
                Err(error) => errors.push(error),
            }
        }

        for error in &errors {
            log::error!("Import application error: {error}");
        }
        for warning in &imported.warnings {
            log::warn!(
                "Import warning ({}): {warning}",
                imported.source.display_name()
            );
        }

        let mut summary = format!("Imported from {}", imported.source.display_name());
        if applied_settings > 0 || installed_theme_name.is_some() {
            summary.push_str(&format!(
                " — {} setting{} applied",
                applied_settings,
                if applied_settings == 1 { "" } else { "s" }
            ));
            if installed_theme_name.is_some() {
                summary.push_str(" + theme");
            }
        }
        termy_toast::success(summary);

        self.go_to(Step::Done, cx);
    }

    pub(super) fn install_selected_theme(&mut self, cx: &mut Context<Self>) {
        let Some(slug) = self.selected_theme_slug.clone() else {
            self.next_step(cx);
            return;
        };
        if self.installed_theme_slug.as_ref() == Some(&slug) {
            self.next_step(cx);
            return;
        }
        let Some(theme) = self
            .themes
            .iter()
            .find(|theme| theme.slug.eq_ignore_ascii_case(&slug))
            .cloned()
        else {
            self.next_step(cx);
            return;
        };
        if theme.file_url.is_none() {
            termy_toast::error(format!(
                "Theme '{}' has no downloadable file URL",
                theme.name
            ));
            return;
        }

        self.installing_theme = true;
        cx.notify();

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let install_slug = theme.slug.trim().to_ascii_lowercase();
            let theme_name = theme.name.clone();
            let result =
                smol::unblock(move || theme_store::install_theme_from_store_blocking(theme)).await;

            cx.update(|cx| {
                let _ = this.update(cx, |view, cx| {
                    view.installing_theme = false;
                    match result {
                        Ok(installed) => {
                            view.installed_theme_slug = Some(installed.slug);
                            termy_toast::success(format!("Installed {theme_name}"));
                            view.next_step(cx);
                        }
                        Err(error) => {
                            log::error!("Onboarding theme install failed: {error}");
                            termy_toast::error(error);
                            let _ = install_slug;
                        }
                    }
                    cx.notify();
                });
            });
        })
        .detach();
    }

    pub(super) fn finalize(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.finalizing {
            return;
        }
        self.finalizing = true;

        let mut errors: Vec<String> = Vec::new();

        if let Some(slug) = self.installed_theme_slug.clone()
            && let Err(error) = config::set_theme_in_config(&slug)
        {
            errors.push(error);
        }

        let font_size = self.font_choice.size();
        if (font_size - AppConfig::default().font_size).abs() > f32::EPSILON
            && let Err(error) =
                config::set_root_setting(RootSettingId::FontSize, &format!("{font_size}"))
        {
            errors.push(error);
        }

        if self.tabs_choice == TabsChoice::Vertical
            && let Err(error) = config::set_root_setting(RootSettingId::VerticalTabs, "true")
        {
            errors.push(error);
        }

        if self.cursor_choice == CursorChoice::Static
            && let Err(error) = config::set_root_setting(RootSettingId::CursorBlink, "false")
        {
            errors.push(error);
        }

        if (self.background_opacity - AppConfig::default().background_opacity).abs() > f32::EPSILON
            && let Err(error) = config::set_root_setting(
                RootSettingId::BackgroundOpacity,
                &format!("{:.3}", self.background_opacity),
            )
        {
            errors.push(error);
        }

        mark_complete_in_config();

        for error in &errors {
            log::error!("Onboarding finalize error: {error}");
            termy_toast::error(error.clone());
        }

        if let Err(error) = crate::open_main_window_with_runtime_config(cx) {
            log::error!("Failed to open main window after onboarding: {error}");
            termy_toast::error(error);
        }

        window.remove_window();
    }

    pub(super) fn skip_onboarding(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.finalizing {
            return;
        }
        self.finalizing = true;

        mark_complete_in_config();

        if let Err(error) = crate::open_main_window_with_runtime_config(cx) {
            log::error!("Failed to open main window after onboarding: {error}");
            termy_toast::error(error);
        }

        window.remove_window();
    }

    pub(super) fn handle_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.modifiers.secondary()
            && !event.keystroke.modifiers.alt
            && !event.keystroke.modifiers.function
        {
            if event.keystroke.key.eq_ignore_ascii_case("q") {
                cx.quit();
                return;
            }
            if event.keystroke.key.eq_ignore_ascii_case("w") {
                window.remove_window();
                return;
            }
        }

        if event.keystroke.key == "escape" {
            self.skip_onboarding(window, cx);
            return;
        }

        if event.keystroke.key == "enter" {
            match self.step {
                Step::Welcome => self.next_step(cx),
                Step::Import => {
                    if self.selected_source.is_some() {
                        self.run_selected_import(cx);
                    } else {
                        self.next_step(cx);
                    }
                }
                Step::Theme => {
                    if self.selected_theme_slug.is_some() {
                        self.install_selected_theme(cx);
                    } else {
                        self.next_step(cx);
                    }
                }
                Step::Settings => self.next_step(cx),
                Step::Done => self.finalize(window, cx),
            }
        }
    }
}
