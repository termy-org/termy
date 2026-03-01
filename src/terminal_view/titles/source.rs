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
}
