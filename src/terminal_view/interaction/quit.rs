use super::*;
use gpui::PromptLevel;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CloseRequestTarget {
    Application,
    WindowClose,
    TabClose { tab_id: TabId },
}

impl TerminalView {
    fn should_prompt_for_close_target(
        target: CloseRequestTarget,
        warn_on_quit: bool,
        warn_on_quit_with_running_process: bool,
        busy_tab_count: usize,
    ) -> bool {
        if busy_tab_count > 0 {
            return warn_on_quit_with_running_process;
        }

        matches!(
            target,
            CloseRequestTarget::Application | CloseRequestTarget::WindowClose
        ) && warn_on_quit
    }

    fn should_force_quit_when_prompt_in_flight(target: CloseRequestTarget) -> bool {
        matches!(target, CloseRequestTarget::Application)
    }

    pub(in super::super) fn execute_quit_command_action(
        &mut self,
        action: CommandAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        match action {
            CommandAction::Quit => {
                self.request_application_quit(window, cx);
                true
            }
            CommandAction::RestartApp => {
                match self.restart_application_with_persist() {
                    Ok(()) => {
                        self.allow_quit_without_prompt = true;
                        cx.quit();
                    }
                    Err(error) => {
                        termy_toast::error(format!("Restart failed: {}", error));
                        self.notify_overlay(cx);
                    }
                }
                true
            }
            _ => false,
        }
    }

    pub(in super::super) fn restart_application_with_persist(&self) -> Result<(), String> {
        self.sync_persisted_native_workspace();
        self.restart_application()
    }

    fn restart_application(&self) -> Result<(), String> {
        let exe = std::env::current_exe().map_err(|e| format!("current_exe failed: {}", e))?;

        #[cfg(target_os = "macos")]
        {
            let app_bundle = exe
                .ancestors()
                .find(|path| {
                    path.extension()
                        .and_then(|ext| ext.to_str())
                        .map(|ext| ext.eq_ignore_ascii_case("app"))
                        .unwrap_or(false)
                })
                .map(PathBuf::from);

            if let Some(app_bundle) = app_bundle {
                let status = Command::new("open")
                    .arg("-n")
                    .arg(&app_bundle)
                    .status()
                    .map_err(|e| format!("failed to launch app bundle: {}", e))?;
                if status.success() {
                    return Ok(());
                }
                return Err(format!("open returned non-success status: {}", status));
            }
        }

        Command::new(&exe)
            .spawn()
            .map_err(|e| format!("failed to spawn executable: {}", e))?;
        Ok(())
    }

    fn tab_is_busy(tab: &TerminalTab) -> bool {
        tab.running_process
            || tab
                .panes
                .iter()
                .any(|pane| pane.terminal.alternate_screen_mode())
    }

    fn tab_title_for_warning(
        &self,
        index: usize,
        tab: &TerminalTab,
        fallback_title: &str,
    ) -> String {
        let title = tab.title.trim();
        if title.is_empty() {
            format!("{fallback_title} {}", index + 1)
        } else {
            title.to_string()
        }
    }

    fn busy_tab_titles_for_close_target(&self, target: CloseRequestTarget) -> Vec<String> {
        let fallback_title = self.fallback_title();
        match target {
            CloseRequestTarget::Application | CloseRequestTarget::WindowClose => self
                .tabs
                .iter()
                .enumerate()
                .filter(|(_, tab)| Self::tab_is_busy(tab))
                .map(|(index, tab)| self.tab_title_for_warning(index, tab, fallback_title))
                .collect(),
            CloseRequestTarget::TabClose { tab_id } => self
                .tabs
                .iter()
                .enumerate()
                .find(|(_, tab)| tab.id == tab_id && Self::tab_is_busy(tab))
                .map(|(index, tab)| vec![self.tab_title_for_warning(index, tab, fallback_title)])
                .unwrap_or_default(),
        }
    }

    fn close_warning_title(target: CloseRequestTarget) -> &'static str {
        match target {
            CloseRequestTarget::Application | CloseRequestTarget::WindowClose => "Quit Termy?",
            CloseRequestTarget::TabClose { .. } => "Close Tab?",
        }
    }

    fn close_warning_buttons(target: CloseRequestTarget) -> &'static [&'static str] {
        match target {
            CloseRequestTarget::Application | CloseRequestTarget::WindowClose => {
                &["Quit", "Cancel"]
            }
            CloseRequestTarget::TabClose { .. } => &["Close Tab", "Cancel"],
        }
    }

    fn close_warning_final_prompt(target: CloseRequestTarget) -> &'static str {
        match target {
            CloseRequestTarget::Application | CloseRequestTarget::WindowClose => "Quit anyway?",
            CloseRequestTarget::TabClose { .. } => "Close it anyway?",
        }
    }

    fn close_warning_detail(target: CloseRequestTarget, busy_titles: &[String]) -> Option<String> {
        if busy_titles.is_empty() {
            return None;
        }

        if matches!(target, CloseRequestTarget::TabClose { .. }) {
            let mut detail =
                "This tab is running a command or fullscreen terminal app:\n".to_string();

            if let Some(title) = busy_titles.first() {
                detail.push_str("- ");
                detail.push_str(title);
                detail.push('\n');
            }

            detail.push_str("\nClose this tab anyway?");
            return Some(detail);
        }

        let count = busy_titles.len();
        let mut detail = format!(
            "{} tab{} {} running a command or fullscreen terminal app:\n",
            count,
            if count == 1 { "" } else { "s" },
            if count == 1 { "has" } else { "have" },
        );

        for title in busy_titles {
            detail.push_str("- ");
            detail.push_str(title);
            detail.push('\n');
        }

        detail.push('\n');
        detail.push_str(Self::close_warning_final_prompt(target));
        Some(detail)
    }

    fn close_tab_by_id(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        if let Some(index) = self.tabs.iter().position(|tab| tab.id == tab_id) {
            self.close_tab(index, cx);
        }
    }

    fn follow_through_close_request(
        &mut self,
        target: CloseRequestTarget,
        cx: &mut Context<Self>,
    ) -> bool {
        match target {
            CloseRequestTarget::Application => {
                self.sync_persisted_native_workspace();
                self.allow_quit_without_prompt = true;
                cx.quit();
                false
            }
            CloseRequestTarget::WindowClose => {
                self.sync_persisted_native_workspace();
                true
            }
            CloseRequestTarget::TabClose { tab_id } => {
                self.close_tab_by_id(tab_id, cx);
                false
            }
        }
    }

    fn request_close(
        &mut self,
        target: CloseRequestTarget,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.quit_prompt_in_flight {
            if Self::should_force_quit_when_prompt_in_flight(target) {
                // If the quit confirm prompt is unresponsive, allow a second
                // Quit shortcut to force-close the app.
                self.allow_quit_without_prompt = true;
                cx.quit();
            }
            return false;
        }

        let busy_titles = self.busy_tab_titles_for_close_target(target);
        if !Self::should_prompt_for_close_target(
            target,
            self.warn_on_quit,
            self.warn_on_quit_with_running_process,
            busy_titles.len(),
        ) {
            return self.follow_through_close_request(target, cx);
        }

        self.quit_prompt_in_flight = true;
        let detail = Self::close_warning_detail(target, &busy_titles);
        let prompt = window.prompt(
            PromptLevel::Warning,
            Self::close_warning_title(target),
            detail.as_deref(),
            Self::close_warning_buttons(target),
            cx,
        );
        let window_handle = window.window_handle();

        cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let confirmed = matches!(prompt.await, Ok(0));
            cx.update(|cx| {
                let mut follow_through = false;
                if this
                    .update(cx, |view, _| {
                        view.quit_prompt_in_flight = false;
                        if confirmed {
                            if !matches!(target, CloseRequestTarget::TabClose { .. }) {
                                view.allow_quit_without_prompt = true;
                            }
                            follow_through = true;
                        }
                    })
                    .is_err()
                {
                    return;
                }

                if !follow_through {
                    return;
                }

                match target {
                    CloseRequestTarget::Application => cx.quit(),
                    CloseRequestTarget::WindowClose => {
                        let _ = window_handle.update(cx, |_, window, _| window.remove_window());
                    }
                    CloseRequestTarget::TabClose { tab_id } => {
                        let _ = this.update(cx, |view, cx| view.close_tab_by_id(tab_id, cx));
                    }
                }
            });
        })
        .detach();

        false
    }

    pub(crate) fn handle_window_should_close_request(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.allow_quit_without_prompt {
            self.sync_persisted_native_workspace();
            self.allow_quit_without_prompt = false;
            return true;
        }

        self.request_close(CloseRequestTarget::WindowClose, window, cx)
    }

    pub(in super::super) fn request_application_quit(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request_close(CloseRequestTarget::Application, window, cx);
    }

    pub(in super::super) fn request_active_tab_close(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.len() <= 1 || self.active_tab >= self.tabs.len() {
            return;
        }
        let tab_id = self.tabs[self.active_tab].id;
        let _ = self.request_close(CloseRequestTarget::TabClose { tab_id }, window, cx);
    }

    pub(in super::super) fn request_tab_close_by_index(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.len() <= 1 || index >= self.tabs.len() {
            return;
        }
        let tab_id = self.tabs[index].id;
        let _ = self.request_close(CloseRequestTarget::TabClose { tab_id }, window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{CloseRequestTarget, TerminalView};

    #[test]
    fn prompt_in_flight_force_quit_policy_only_allows_application_target() {
        assert!(TerminalView::should_force_quit_when_prompt_in_flight(
            CloseRequestTarget::Application
        ));
        assert!(!TerminalView::should_force_quit_when_prompt_in_flight(
            CloseRequestTarget::WindowClose
        ));
        assert!(!TerminalView::should_force_quit_when_prompt_in_flight(
            CloseRequestTarget::TabClose { tab_id: 1 }
        ));
    }

    #[test]
    fn always_warn_on_quit_only_prompts_for_app_or_window_close_when_not_busy() {
        assert!(TerminalView::should_prompt_for_close_target(
            CloseRequestTarget::Application,
            true,
            false,
            0,
        ));
        assert!(TerminalView::should_prompt_for_close_target(
            CloseRequestTarget::WindowClose,
            true,
            false,
            0,
        ));
        assert!(!TerminalView::should_prompt_for_close_target(
            CloseRequestTarget::TabClose { tab_id: 1 },
            true,
            false,
            0,
        ));
    }

    #[test]
    fn running_process_warning_only_prompts_when_busy() {
        assert!(TerminalView::should_prompt_for_close_target(
            CloseRequestTarget::Application,
            false,
            true,
            1,
        ));
        assert!(!TerminalView::should_prompt_for_close_target(
            CloseRequestTarget::Application,
            false,
            true,
            0,
        ));
    }

    #[test]
    fn close_warning_detail_is_absent_for_always_warn_without_busy_tabs() {
        assert_eq!(
            TerminalView::close_warning_detail(CloseRequestTarget::Application, &[]),
            None
        );
    }
}
