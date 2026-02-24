use super::super::*;

impl TerminalView {
    pub(crate) fn fallback_title(&self) -> &str {
        let fallback = self.tab_title.fallback.trim();
        if fallback.is_empty() {
            DEFAULT_TAB_TITLE
        } else {
            fallback
        }
    }

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

    pub(crate) fn should_seed_predicted_prompt_title(tab_title: &TabTitleConfig) -> bool {
        tab_title
            .priority
            .iter()
            .any(|source| *source == TabTitleSource::Explicit)
    }

    pub(crate) fn predicted_prompt_seed_title(
        tab_title: &TabTitleConfig,
        cwd: Option<&str>,
    ) -> Option<String> {
        if !Self::should_seed_predicted_prompt_title(tab_title) {
            return None;
        }

        let resolved = Self::resolve_template(&tab_title.prompt_format, cwd, None);
        let resolved = resolved.trim();
        if resolved.is_empty() {
            return None;
        }

        Some(Self::truncate_tab_title(resolved))
    }

    pub(super) fn prompt_cwd_payload_from_explicit_title(
        title: &str,
        explicit_prefix: &str,
    ) -> Option<String> {
        let prefix = explicit_prefix.trim();
        if prefix.is_empty() {
            return None;
        }

        let payload = title.trim().strip_prefix(prefix)?.trim();
        let prompt = payload.strip_prefix("prompt:")?.trim();
        if prompt.is_empty() {
            return None;
        }

        Some(prompt.to_string())
    }

    fn parse_explicit_title(&self, title: &str) -> Option<ExplicitTitlePayload> {
        let prefix = self.tab_title.explicit_prefix.trim();
        if prefix.is_empty() {
            return None;
        }

        let payload = title.strip_prefix(prefix)?.trim();
        if payload.is_empty() {
            return None;
        }

        if let Some(prompt) = payload.strip_prefix("prompt:") {
            let prompt = prompt.trim();
            if prompt.is_empty() {
                return None;
            }
            return Some(ExplicitTitlePayload::Prompt(Self::resolve_template(
                &self.tab_title.prompt_format,
                Some(prompt),
                None,
            )));
        }

        if let Some(command) = payload.strip_prefix("command:") {
            let command = command.trim();
            if command.is_empty() {
                return None;
            }
            return Some(ExplicitTitlePayload::Command(Self::resolve_template(
                &self.tab_title.command_format,
                None,
                Some(command),
            )));
        }

        let explicit = payload.strip_prefix("title:").unwrap_or(payload).trim();
        if explicit.is_empty() {
            return None;
        }

        Some(ExplicitTitlePayload::Title(explicit.to_string()))
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

    pub(crate) fn apply_terminal_title(
        &mut self,
        index: usize,
        title: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        let title = title.trim();
        if title.is_empty() || index >= self.tabs.len() {
            return false;
        }

        if let Some(prompt_cwd) = Self::prompt_cwd_payload_from_explicit_title(
            title,
            &self.tab_title.explicit_prefix,
        ) {
            self.tabs[index].working_dir = Some(prompt_cwd);
        }

        if let Some(explicit_payload) = self.parse_explicit_title(title) {
            return match explicit_payload {
                ExplicitTitlePayload::Prompt(prompt_title) => {
                    self.tabs[index].running_process = false;
                    self.cancel_pending_command_title(index);
                    self.set_explicit_title(index, prompt_title)
                }
                ExplicitTitlePayload::Title(prompt_title) => {
                    self.cancel_pending_command_title(index);
                    self.set_explicit_title(index, prompt_title)
                }
                ExplicitTitlePayload::Command(command_title) => {
                    self.tabs[index].running_process = true;
                    let tab_id = self.tabs[index].id;
                    self.schedule_delayed_command_title(
                        tab_id,
                        command_title,
                        COMMAND_TITLE_DELAY_MS,
                        cx,
                    );
                    false
                }
            };
        }

        let shell_title = Self::truncate_tab_title(title);
        if self.tabs[index].shell_title.as_deref() == Some(shell_title.as_str()) {
            return false;
        }

        self.tabs[index].shell_title = Some(shell_title);
        self.refresh_tab_title(index)
    }

    pub(crate) fn clear_terminal_titles(&mut self, index: usize) -> bool {
        if index >= self.tabs.len() {
            return false;
        }

        self.cancel_pending_command_title(index);
        let tab = &mut self.tabs[index];
        tab.running_process = false;
        let had_shell = tab.shell_title.take().is_some();
        let had_explicit = tab.explicit_title.take().is_some();
        if !had_shell && !had_explicit {
            return false;
        }

        self.refresh_tab_title(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{TabTitleConfig, TabTitleSource};

    #[test]
    fn predicted_prompt_seed_title_uses_cwd_template_when_explicit_is_enabled() {
        let config = TabTitleConfig::default();
        let title = TerminalView::predicted_prompt_seed_title(&config, Some("~/projects/termy"));
        assert_eq!(title.as_deref(), Some("~/projects/termy"));
    }

    #[test]
    fn predicted_prompt_seed_title_skips_static_only_priority() {
        let mut config = TabTitleConfig::default();
        config.priority = vec![TabTitleSource::Manual, TabTitleSource::Fallback];

        let title = TerminalView::predicted_prompt_seed_title(&config, Some("~/projects/termy"));
        assert!(title.is_none());
    }

    #[test]
    fn predicted_prompt_seed_title_ignores_empty_resolved_output() {
        let mut config = TabTitleConfig::default();
        config.prompt_format = "{cwd}".to_string();

        let title = TerminalView::predicted_prompt_seed_title(&config, None);
        assert!(title.is_none());
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
    fn resolve_template_leaves_unknown_brace_tokens_unchanged() {
        let resolved =
            TerminalView::resolve_template("start {unknown} end", Some("cwd"), Some("cmd"));
        assert_eq!(resolved, "start {unknown} end");
    }

    #[test]
    fn prompt_cwd_payload_from_explicit_title_extracts_prompt_payload() {
        let cwd = TerminalView::prompt_cwd_payload_from_explicit_title(
            "termy:tab:prompt:~/projects/termy",
            "termy:tab:",
        );
        assert_eq!(cwd.as_deref(), Some("~/projects/termy"));
    }

    #[test]
    fn prompt_cwd_payload_from_explicit_title_ignores_non_prompt_payloads() {
        let command = TerminalView::prompt_cwd_payload_from_explicit_title(
            "termy:tab:command:cargo test",
            "termy:tab:",
        );
        let plain_title = TerminalView::prompt_cwd_payload_from_explicit_title(
            "termy:tab:title:Deploy",
            "termy:tab:",
        );
        assert!(command.is_none());
        assert!(plain_title.is_none());
    }
}
