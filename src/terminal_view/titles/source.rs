use super::super::*;

impl TerminalView {
    pub(crate) fn resolve_template(
        template: &str,
        cwd: Option<&str>,
        command: Option<&str>,
    ) -> String {
        const CWD_TOKEN: &str = "{cwd}";
        const COMMAND_TOKEN: &str = "{command}";

        let cwd = cwd.unwrap_or("");
        let command = command.unwrap_or("");
        let mut remaining = template;
        let mut resolved = String::with_capacity(template.len());

        while let Some(open_brace_idx) = remaining.find('{') {
            resolved.push_str(&remaining[..open_brace_idx]);
            remaining = &remaining[open_brace_idx..];
            if let Some(tail) = remaining.strip_prefix(CWD_TOKEN) {
                resolved.push_str(cwd);
                remaining = tail;
                continue;
            }
            if let Some(tail) = remaining.strip_prefix(COMMAND_TOKEN) {
                resolved.push_str(command);
                remaining = tail;
                continue;
            }

            resolved.push('{');
            remaining = &remaining['{'.len_utf8()..];
        }

        resolved.push_str(remaining);
        resolved
    }

    pub(crate) fn is_shell_command(command: &str) -> bool {
        let command = command.trim();
        if command.is_empty() {
            return false;
        }

        matches!(
            command.to_ascii_lowercase().as_str(),
            "bash"
                | "zsh"
                | "fish"
                | "sh"
                | "dash"
                | "ksh"
                | "tcsh"
                | "csh"
                | "nu"
                | "pwsh"
                | "powershell"
                | "cmd"
        )
    }

    pub(crate) fn derive_tmux_shell_title(
        tab_title: &TabTitleConfig,
        pane: &TmuxPaneState,
    ) -> Option<String> {
        let cwd = pane.current_path.trim();
        let command = pane.current_command.trim();
        let resolved = if Self::is_shell_command(command) {
            Self::resolve_template(
                &tab_title.prompt_format,
                (!cwd.is_empty()).then_some(cwd),
                None,
            )
        } else {
            Self::resolve_template(
                &tab_title.command_format,
                None,
                (!command.is_empty()).then_some(command),
            )
        };

        let resolved = resolved.trim();
        if resolved.is_empty() {
            return None;
        }

        Some(Self::truncate_tab_title(resolved))
    }

    pub(crate) fn native_title_fields_from_osc(
        tab_title: &TabTitleConfig,
        raw_title: &str,
    ) -> (Option<String>, Option<String>, bool) {
        let trimmed = raw_title.trim();
        if trimmed.is_empty() {
            return (None, None, false);
        }

        let prefix = tab_title.explicit_prefix.trim();
        let payload = if prefix.is_empty() {
            trimmed
        } else {
            trimmed.strip_prefix(prefix).unwrap_or(trimmed)
        };

        if let Some(cwd_payload) = payload.strip_prefix("prompt:") {
            let cwd = cwd_payload.trim();
            let resolved =
                Self::resolve_template(&tab_title.prompt_format, (!cwd.is_empty()).then_some(cwd), None);
            let resolved = resolved.trim();
            let shell_title = (!resolved.is_empty()).then(|| Self::truncate_tab_title(resolved));
            return (shell_title, None, false);
        }

        if let Some(command_payload) = payload.strip_prefix("command:") {
            let command = command_payload.trim();
            let resolved = Self::resolve_template(
                &tab_title.command_format,
                None,
                (!command.is_empty()).then_some(command),
            );
            let resolved = resolved.trim();
            let shell_title = (!resolved.is_empty()).then(|| Self::truncate_tab_title(resolved));
            let running_process = !command.is_empty() && !Self::is_shell_command(command);
            return (shell_title, None, running_process);
        }

        let explicit = payload.trim();
        let explicit_title = (!explicit.is_empty()).then(|| Self::truncate_tab_title(explicit));
        (None, explicit_title, false)
    }

    pub(crate) fn native_command_payload_from_osc<'a>(
        tab_title: &TabTitleConfig,
        raw_title: &'a str,
    ) -> Option<&'a str> {
        let trimmed = raw_title.trim();
        if trimmed.is_empty() {
            return None;
        }

        let prefix = tab_title.explicit_prefix.trim();
        let payload = if prefix.is_empty() {
            trimmed
        } else {
            trimmed.strip_prefix(prefix).unwrap_or(trimmed)
        };

        payload.strip_prefix("command:").map(str::trim)
    }

    pub(crate) fn apply_native_osc_title(
        &mut self,
        index: usize,
        raw_title: &str,
        now: Instant,
    ) -> bool {
        if index >= self.tabs.len() {
            return false;
        }

        let (shell_title, explicit_title, running_process) =
            Self::native_title_fields_from_osc(&self.tab_title, raw_title);
        let command_payload = Self::native_command_payload_from_osc(&self.tab_title, raw_title);
        let mut title_sources_changed = false;

        {
            let tab = &mut self.tabs[index];
            tab.running_process = running_process;

            if command_payload.is_some() && running_process {
                tab.pending_shell_title = shell_title;
                tab.pending_shell_title_deadline = tab
                    .pending_shell_title
                    .as_ref()
                    .map(|_| now + Duration::from_millis(NATIVE_COMMAND_TITLE_DELAY_MS));
                title_sources_changed = tab.explicit_title.take().is_some();
            } else {
                tab.pending_shell_title = None;
                tab.pending_shell_title_deadline = None;

                if tab.shell_title != shell_title {
                    tab.shell_title = shell_title;
                    title_sources_changed = true;
                }
                if tab.explicit_title != explicit_title {
                    tab.explicit_title = explicit_title;
                    title_sources_changed = true;
                }
            }
        }

        title_sources_changed && self.refresh_tab_title(index)
    }

    pub(crate) fn clear_native_osc_title(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }

        let title_sources_changed;
        {
            let tab = &mut self.tabs[index];
            tab.pending_shell_title = None;
            tab.pending_shell_title_deadline = None;
            tab.running_process = false;
            let had_shell = tab.shell_title.take().is_some();
            let had_explicit = tab.explicit_title.take().is_some();
            title_sources_changed = had_shell || had_explicit;
        }

        title_sources_changed && self.refresh_tab_title(index)
    }

    pub(crate) fn apply_due_native_command_title(&mut self, index: usize, now: Instant) -> bool {
        if index >= self.tabs.len() {
            return false;
        }

        let mut title_sources_changed = false;
        {
            let tab = &mut self.tabs[index];
            let Some(deadline) = tab.pending_shell_title_deadline else {
                return false;
            };
            if now < deadline {
                return false;
            }

            tab.pending_shell_title_deadline = None;
            let next_shell_title = tab.pending_shell_title.take();
            if tab.shell_title != next_shell_title {
                tab.shell_title = next_shell_title;
                title_sources_changed = true;
            }
        }

        title_sources_changed && self.refresh_tab_title(index)
    }

    pub(crate) fn fallback_title(&self) -> &str {
        let fallback = self.tab_title.fallback.trim();
        if fallback.is_empty() {
            DEFAULT_TAB_TITLE
        } else {
            fallback
        }
    }

    pub(crate) fn resolved_tab_title(&self, index: usize) -> String {
        let tab = &self.tabs[index];

        for source in &self.tab_title.priority {
            let candidate = match source {
                TabTitleSource::Manual => tab.manual_title.as_deref(),
                TabTitleSource::Explicit => tab.explicit_title.as_deref(),
                TabTitleSource::Shell => tab.shell_title.as_deref(),
                TabTitleSource::Fallback => Some(self.fallback_title()),
            };

            if let Some(candidate) = candidate.map(str::trim).filter(|value| !value.is_empty()) {
                return Self::truncate_tab_title(candidate);
            }
        }

        Self::truncate_tab_title(self.fallback_title())
    }

    pub(crate) fn refresh_tab_title(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }

        let next = self.resolved_tab_title(index);
        if self.tabs[index].title == next {
            return false;
        }

        let previous = std::mem::replace(&mut self.tabs[index].title, next);
        let current_title = self.tabs[index].title.clone();
        self.invalidate_tab_title_width_cache_for_title(previous.as_str());
        self.invalidate_tab_title_width_cache_for_title(current_title.as_str());

        // Keep title-width behavior uniform across manual, shell, explicit, and fallback sources.
        self.tabs[index].sticky_title_width = 0.0;
        self.tabs[index].title_text_width = 0.0;
        self.mark_tab_strip_layout_dirty();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pane_with(path: &str, command: &str) -> TmuxPaneState {
        TmuxPaneState {
            id: "%1".to_string(),
            window_id: "@1".to_string(),
            session_id: "$1".to_string(),
            is_active: true,
            left: 0,
            top: 0,
            width: 80,
            height: 24,
            cursor_x: 0,
            cursor_y: 0,
            current_path: path.to_string(),
            current_command: command.to_string(),
        }
    }

    #[test]
    fn resolve_template_replaces_known_tokens_in_single_pass() {
        let resolved = TerminalView::resolve_template(
            "cwd={cwd} command={command}",
            Some("{command}"),
            Some("{cwd}"),
        );
        assert_eq!(resolved, "cwd={command} command={cwd}");
    }

    #[test]
    fn resolve_template_leaves_unknown_tokens_unchanged() {
        let resolved =
            TerminalView::resolve_template("start {unknown} end", Some("cwd"), Some("cmd"));
        assert_eq!(resolved, "start {unknown} end");
    }

    #[test]
    fn is_shell_command_matches_fixed_shell_set_case_insensitively() {
        assert!(TerminalView::is_shell_command("zsh"));
        assert!(TerminalView::is_shell_command("PwSh"));
        assert!(TerminalView::is_shell_command("cmd"));
        assert!(!TerminalView::is_shell_command("sleep"));
        assert!(!TerminalView::is_shell_command(""));
    }

    #[test]
    fn derive_tmux_shell_title_uses_prompt_format_for_shell_commands() {
        let mut tab_title = TabTitleConfig::default();
        tab_title.prompt_format = "cwd:{cwd}".to_string();
        let pane = pane_with("/tmp/work", "zsh");

        let title = TerminalView::derive_tmux_shell_title(&tab_title, &pane);
        assert_eq!(title.as_deref(), Some("cwd:/tmp/work"));
    }

    #[test]
    fn derive_tmux_shell_title_uses_command_format_for_non_shell_commands() {
        let mut tab_title = TabTitleConfig::default();
        tab_title.command_format = "run:{command}".to_string();
        let pane = pane_with("/tmp/work", "sleep");

        let title = TerminalView::derive_tmux_shell_title(&tab_title, &pane);
        assert_eq!(title.as_deref(), Some("run:sleep"));
    }

    #[test]
    fn derive_tmux_shell_title_returns_none_when_resolved_title_is_empty() {
        let mut tab_title = TabTitleConfig::default();
        tab_title.command_format = " ".to_string();
        let pane = pane_with("/tmp/work", "sleep");

        let title = TerminalView::derive_tmux_shell_title(&tab_title, &pane);
        assert!(title.is_none());
    }

    #[test]
    fn native_title_fields_parse_prompt_payload_with_prefix() {
        let mut tab_title = TabTitleConfig::default();
        tab_title.explicit_prefix = "termy:tab:".to_string();
        tab_title.prompt_format = "cwd:{cwd}".to_string();

        let (shell_title, explicit_title, running_process) =
            TerminalView::native_title_fields_from_osc(&tab_title, "termy:tab:prompt:~/projects");
        assert_eq!(shell_title.as_deref(), Some("cwd:~/projects"));
        assert!(explicit_title.is_none());
        assert!(!running_process);
    }

    #[test]
    fn native_title_fields_parse_command_payload_with_prefix() {
        let mut tab_title = TabTitleConfig::default();
        tab_title.explicit_prefix = "termy:tab:".to_string();
        tab_title.command_format = "run:{command}".to_string();

        let (shell_title, explicit_title, running_process) =
            TerminalView::native_title_fields_from_osc(&tab_title, "termy:tab:command:rg");
        assert_eq!(shell_title.as_deref(), Some("run:rg"));
        assert!(explicit_title.is_none());
        assert!(running_process);
    }

    #[test]
    fn native_title_fields_command_payload_marks_shell_as_not_running() {
        let mut tab_title = TabTitleConfig::default();
        tab_title.explicit_prefix = "termy:tab:".to_string();

        let (_, _, running_process) =
            TerminalView::native_title_fields_from_osc(&tab_title, "termy:tab:command:zsh");
        assert!(!running_process);
    }

    #[test]
    fn native_command_payload_detects_prefixed_command_payload() {
        let mut tab_title = TabTitleConfig::default();
        tab_title.explicit_prefix = "termy:tab:".to_string();

        let payload =
            TerminalView::native_command_payload_from_osc(&tab_title, "termy:tab:command:rg");
        assert_eq!(payload, Some("rg"));
    }

    #[test]
    fn native_command_payload_ignores_prompt_and_explicit_payloads() {
        let mut tab_title = TabTitleConfig::default();
        tab_title.explicit_prefix = "termy:tab:".to_string();

        assert_eq!(
            TerminalView::native_command_payload_from_osc(&tab_title, "termy:tab:prompt:~/work"),
            None
        );
        assert_eq!(
            TerminalView::native_command_payload_from_osc(&tab_title, "termy:tab:Deploy"),
            None
        );
    }

    #[test]
    fn native_title_fields_treat_raw_payload_as_explicit_title() {
        let mut tab_title = TabTitleConfig::default();
        tab_title.explicit_prefix = "termy:tab:".to_string();

        let (shell_title, explicit_title, running_process) =
            TerminalView::native_title_fields_from_osc(&tab_title, "termy:tab:Deploying");
        assert!(shell_title.is_none());
        assert_eq!(explicit_title.as_deref(), Some("Deploying"));
        assert!(!running_process);
    }
}
