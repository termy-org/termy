use super::*;

impl TerminalView {
    pub(in super::super) fn agent_thread_has_live_session(&self, thread: &AgentThread) -> bool {
        thread
            .linked_tab_id
            .and_then(|tab_id| self.tab_index_by_id(tab_id))
            .is_some()
    }

    pub(in super::super) fn agent_thread_shows_activity(
        &self,
        thread: &AgentThread,
        is_active: bool,
    ) -> bool {
        !is_active
            && matches!(
                self.agent_thread_runtime_status(thread),
                AgentThreadRuntimeStatus::Busy
            )
    }

    pub(in super::super) fn agent_thread_runtime_status(
        &self,
        thread: &AgentThread,
    ) -> AgentThreadRuntimeStatus {
        let Some(tab_id) = thread.linked_tab_id else {
            return AgentThreadRuntimeStatus::Saved;
        };
        let Some(tab_index) = self.tab_index_by_id(tab_id) else {
            return AgentThreadRuntimeStatus::Saved;
        };
        let Some(tab) = self.tabs.get(tab_index) else {
            return AgentThreadRuntimeStatus::Saved;
        };

        if tab.running_process
            || tab
                .current_command
                .as_deref()
                .is_some_and(|command| !command.trim().is_empty())
        {
            AgentThreadRuntimeStatus::Busy
        } else {
            AgentThreadRuntimeStatus::Ready
        }
    }

    pub(in super::super) fn extract_agent_status_line(
        grid: &alacritty_terminal::grid::Grid<alacritty_terminal::term::cell::Cell>,
        line_idx: i32,
    ) -> Option<String> {
        use alacritty_terminal::index::{Column, Line};

        let line = Line(line_idx);
        let cols = grid.columns();
        let total_lines = grid.total_lines();
        if line_idx < -(total_lines as i32 - grid.screen_lines() as i32)
            || line_idx >= grid.screen_lines() as i32
        {
            return None;
        }

        let mut text = String::with_capacity(cols);
        for col in 0..cols {
            let cell = &grid[line][Column(col)];
            let c = cell.c;
            if c == '\0' || cell.flags.contains(Flags::WIDE_CHAR_SPACER) || c.is_control() {
                text.push(' ');
            } else {
                text.push(c);
            }
        }

        Some(text.trim_end().to_string())
    }

    pub(in super::super) fn normalize_agent_status_line(line: &str) -> Option<String> {
        let normalized = line.split_whitespace().collect::<Vec<_>>().join(" ");
        let normalized = normalized.trim();
        if normalized.is_empty() || !normalized.chars().any(|ch| ch.is_alphanumeric()) {
            return None;
        }
        Some(normalized.to_string())
    }

    pub(in super::super) fn collect_visible_agent_status_lines(terminal: &Terminal) -> Vec<String> {
        let mut lines = Vec::new();
        let rows = i32::from(terminal.size().rows.max(1));
        let start_line = (rows - AGENT_STATUS_VISIBLE_LINE_COUNT).max(0);
        let _ = terminal.with_grid(|grid| {
            for line_idx in start_line..rows {
                if let Some(line) = Self::extract_agent_status_line(grid, line_idx)
                    .and_then(|line| Self::normalize_agent_status_line(&line))
                {
                    lines.push(line);
                }
            }
        });
        lines
    }

    pub(in super::super) fn detect_agent_session_id(
        agent: command_palette::AiAgentPreset,
        terminal: &Terminal,
    ) -> Option<String> {
        if !matches!(
            agent,
            command_palette::AiAgentPreset::Claude | command_palette::AiAgentPreset::Codex
        ) {
            return None;
        }

        let (_, history_size) = terminal.scroll_state();
        let rows = i32::from(terminal.size().rows.max(1));
        let scan_start = -(history_size as i32);

        let mut session_id: Option<String> = None;

        let _ = terminal.with_grid(|grid| {
            for line_idx in scan_start..rows {
                let Some(line) = Self::extract_agent_status_line(grid, line_idx) else {
                    continue;
                };
                let trimmed = line.trim();

                // Match patterns like:
                //   session: <id>
                //   session id: <id>
                //   Session ID: <id>
                //   conversation: <id>
                //   resume: <id>
                //   --resume <id>
                let lower = trimmed.to_ascii_lowercase();
                for prefix in &[
                    "session id: ",
                    "session: ",
                    "conversation id: ",
                    "conversation: ",
                    "session_id: ",
                    "resume id: ",
                ] {
                    if let Some(rest) = lower.find(prefix).map(|pos| {
                        let start = pos + prefix.len();
                        trimmed[start..].trim()
                    }) {
                        let id = rest
                            .split(|c: char| c.is_whitespace() || c == ',' || c == ')')
                            .next()
                            .unwrap_or(rest)
                            .trim();
                        if !id.is_empty() && id.len() >= 4 {
                            session_id = Some(id.to_string());
                        }
                    }
                }

                // Also match UUID-like patterns on lines containing "session"
                if lower.contains("session") || lower.contains("conversation") {
                    for word in trimmed.split_whitespace() {
                        let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-' && c != '_');
                        if cleaned.len() >= 8
                            && cleaned.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_')
                            && cleaned.chars().any(|c| c.is_alphanumeric())
                            && !cleaned.chars().all(|c| c.is_alphabetic())
                        {
                            session_id = Some(cleaned.to_string());
                        }
                    }
                }
            }
        });

        session_id
    }

    pub(in super::super) fn find_last_status_line(
        lines: &[String],
        needles: &[&str],
    ) -> Option<String> {
        lines.iter().rev().find_map(|line| {
            let normalized = line.to_ascii_lowercase();
            needles
                .iter()
                .any(|needle| normalized.contains(needle))
                .then(|| line.clone())
        })
    }

    pub(in super::super) fn detect_generic_agent_status(
        lines: &[String],
    ) -> Option<AgentThreadStatusPresentation> {
        if let Some(line) = Self::find_last_status_line(
            lines,
            &["error", "failed", "unable to", "panic", "denied", "refused"],
        ) {
            return Some(AgentThreadStatusPresentation {
                label: "error".to_string(),
                detail: Some(line),
                tone: AgentThreadStatusTone::Error,
            });
        }

        if let Some(line) = Self::find_last_status_line(
            lines,
            &[
                "approval",
                "approve",
                "permission",
                "permissions",
                "allow",
                "confirm",
            ],
        ) {
            return Some(AgentThreadStatusPresentation {
                label: "approval".to_string(),
                detail: Some(line),
                tone: AgentThreadStatusTone::Warning,
            });
        }

        if let Some(line) = Self::find_last_status_line(
            lines,
            &["thinking", "analyzing", "reasoning", "planning", "working"],
        ) {
            return Some(AgentThreadStatusPresentation {
                label: "thinking".to_string(),
                detail: Some(line),
                tone: AgentThreadStatusTone::Active,
            });
        }

        if let Some(line) = Self::find_last_status_line(
            lines,
            &[
                "running",
                "executing",
                "editing",
                "searching",
                "reading",
                "writing",
                "patching",
                "tool",
            ],
        ) {
            return Some(AgentThreadStatusPresentation {
                label: "tool".to_string(),
                detail: Some(line),
                tone: AgentThreadStatusTone::Active,
            });
        }

        None
    }

    pub(in super::super) fn detect_provider_status(
        agent: command_palette::AiAgentPreset,
        lines: &[String],
    ) -> Option<AgentThreadStatusPresentation> {
        match agent {
            command_palette::AiAgentPreset::Pi => {
                if let Some(line) =
                    Self::find_last_status_line(lines, &["no-model", "no models available"])
                {
                    return Some(AgentThreadStatusPresentation {
                        label: "setup".to_string(),
                        detail: Some(line),
                        tone: AgentThreadStatusTone::Warning,
                    });
                }
            }
            command_palette::AiAgentPreset::Claude => {
                if let Some(line) = Self::find_last_status_line(
                    lines,
                    &[
                        "unable to connect to anthropic services",
                        "api.anthropic.com",
                    ],
                ) {
                    return Some(AgentThreadStatusPresentation {
                        label: "error".to_string(),
                        detail: Some(line),
                        tone: AgentThreadStatusTone::Error,
                    });
                }
                if let Some(line) = Self::find_last_status_line(lines, &["welcome to claude code"])
                {
                    return Some(AgentThreadStatusPresentation {
                        label: "starting".to_string(),
                        detail: Some(line),
                        tone: AgentThreadStatusTone::Active,
                    });
                }
            }
            command_palette::AiAgentPreset::OpenCode => {
                if let Some(line) = Self::find_last_status_line(lines, &["models.dev"]) {
                    return Some(AgentThreadStatusPresentation {
                        label: "error".to_string(),
                        detail: Some(line),
                        tone: AgentThreadStatusTone::Error,
                    });
                }
                if let Some(generic) = Self::detect_generic_agent_status(lines) {
                    return Some(generic);
                }
                if let Some(line) = Self::find_last_status_line(lines, &["ask anything"]) {
                    let detail = Self::find_last_status_line(lines, &["opencode zen", "build "])
                        .or(Some(line));
                    return Some(AgentThreadStatusPresentation {
                        label: "ready".to_string(),
                        detail,
                        tone: AgentThreadStatusTone::Active,
                    });
                }
                if let Some(line) = Self::find_last_status_line(lines, &["mcp:", "servers"]) {
                    return Some(AgentThreadStatusPresentation {
                        label: "ready".to_string(),
                        detail: Some(line),
                        tone: AgentThreadStatusTone::Active,
                    });
                }
            }
            command_palette::AiAgentPreset::Codex => {
                if let Some(line) = Self::find_last_status_line(
                    lines,
                    &["otel exporter", "panicked", "could not create"],
                ) {
                    return Some(AgentThreadStatusPresentation {
                        label: "error".to_string(),
                        detail: Some(line),
                        tone: AgentThreadStatusTone::Error,
                    });
                }
            }
            command_palette::AiAgentPreset::Cursor => {}
            command_palette::AiAgentPreset::Copilot => {}
            command_palette::AiAgentPreset::Kiro => {}
        }

        if let Some(generic) = Self::detect_generic_agent_status(lines) {
            return Some(generic);
        }

        match agent {
            command_palette::AiAgentPreset::Pi => {
                Self::find_last_status_line(lines, &["%/", "(auto)", "no-model"]).map(|line| {
                    AgentThreadStatusPresentation {
                        label: "ready".to_string(),
                        detail: Some(line),
                        tone: AgentThreadStatusTone::Active,
                    }
                })
            }
            command_palette::AiAgentPreset::Claude => None,
            command_palette::AiAgentPreset::Cursor => None,
            command_palette::AiAgentPreset::Copilot => None,
            command_palette::AiAgentPreset::OpenCode => None,
            command_palette::AiAgentPreset::Codex => None,
            command_palette::AiAgentPreset::Kiro => None,
        }
    }

    pub(in super::super) fn live_agent_thread_fallback_detail(
        &self,
        thread: &AgentThread,
        tab: &TerminalTab,
    ) -> String {
        if let Some(command) = tab.current_command.as_deref()
            && !command.trim().is_empty()
        {
            return command.to_string();
        }

        format!(
            "{} attached",
            Self::display_working_directory_for_prompt(Path::new(&thread.working_dir))
        )
    }

    pub(in super::super) fn saved_agent_thread_detail(
        &self,
        thread: &AgentThread,
    ) -> Option<String> {
        match (
            thread.last_status_label.as_deref(),
            thread.last_status_detail.as_deref(),
        ) {
            (Some(label), Some(detail)) => Some(format!("Last {}: {}", label, detail)),
            (Some(label), None) => Some(format!("Last {}", label)),
            (None, Some(detail)) => Some(detail.to_string()),
            (None, None) => None,
        }
    }

    pub(in super::super) fn agent_thread_status_presentation(
        &self,
        thread: &AgentThread,
    ) -> AgentThreadStatusPresentation {
        if let Some(tab) = thread
            .linked_tab_id
            .and_then(|tab_id| self.tab_index_by_id(tab_id))
            .and_then(|index| self.tabs.get(index))
        {
            if let Some(terminal) = tab.active_terminal() {
                let lines = Self::collect_visible_agent_status_lines(terminal);
                if let Some(mut provider_status) =
                    Self::detect_provider_status(thread.agent, &lines)
                {
                    if provider_status.detail.is_none() {
                        provider_status.detail =
                            Some(self.live_agent_thread_fallback_detail(thread, tab));
                    }
                    return provider_status;
                }
            }

            let runtime_status = self.agent_thread_runtime_status(thread);
            return AgentThreadStatusPresentation {
                label: runtime_status.label().to_string(),
                detail: Some(self.live_agent_thread_fallback_detail(thread, tab)),
                tone: AgentThreadStatusTone::Active,
            };
        }

        AgentThreadStatusPresentation {
            label: AgentThreadRuntimeStatus::Saved.label().to_string(),
            detail: self
                .saved_agent_thread_detail(thread)
                .or_else(|| thread.last_seen_command.clone())
                .or_else(|| {
                    Some(Self::display_working_directory_for_prompt(Path::new(
                        &thread.working_dir,
                    )))
                }),
            tone: AgentThreadStatusTone::Muted,
        }
    }

    pub(in super::super) fn update_agent_session_ids(&mut self) {
        let updates: Vec<(usize, String)> = self
            .agent_threads
            .iter()
            .enumerate()
            .filter(|(_, thread)| thread.last_session_id.is_none() && thread.linked_tab_id.is_some())
            .filter_map(|(i, thread)| {
                let tab = thread
                    .linked_tab_id
                    .and_then(|tab_id| self.tab_index_by_id(tab_id))
                    .and_then(|index| self.tabs.get(index))?;
                let terminal = tab.active_terminal()?;
                let session_id = Self::detect_agent_session_id(thread.agent, terminal)?;
                Some((i, session_id))
            })
            .collect();

        for (index, session_id) in updates {
            self.agent_threads[index].last_session_id = Some(session_id);
        }
    }

    pub(in super::super) fn agent_thread_display_title(&self, thread: &AgentThread) -> String {
        thread
            .custom_title
            .clone()
            .or_else(|| {
                thread
                    .linked_tab_id
                    .and_then(|tab_id| self.tab_index_by_id(tab_id))
                    .and_then(|index| self.tabs.get(index))
                    .map(|tab| tab.title.clone())
            })
            .or_else(|| thread.last_seen_title.clone())
            .unwrap_or_else(|| thread.title.clone())
    }

    pub(in super::super) fn project_thread_count(&self, project_id: &str) -> usize {
        self.agent_threads
            .iter()
            .filter(|thread| thread.project_id == project_id)
            .count()
    }

    pub(in super::super) fn sorted_agent_projects(&self) -> Vec<&AgentProject> {
        let mut projects = self.agent_projects.iter().collect::<Vec<_>>();
        projects.sort_by_key(|project| (!project.pinned, std::cmp::Reverse(project.updated_at_ms)));
        projects
    }

    pub(in super::super) fn sorted_agent_threads_for_project(
        &self,
        project_id: &str,
    ) -> Vec<&AgentThread> {
        let mut threads = self
            .agent_threads
            .iter()
            .filter(|thread| thread.project_id == project_id)
            .collect::<Vec<_>>();
        threads.sort_by_key(|thread| (!thread.pinned, std::cmp::Reverse(thread.updated_at_ms)));
        threads
    }

    pub(in super::super) fn agent_sidebar_search_terms(&self) -> Vec<String> {
        self.agent_sidebar_search_input
            .text()
            .split_whitespace()
            .map(|term| term.trim().to_ascii_lowercase())
            .filter(|term| !term.is_empty())
            .collect()
    }

    pub(in super::super) fn agent_sidebar_query_matches_text(text: &str, terms: &[String]) -> bool {
        if terms.is_empty() {
            return true;
        }

        let normalized = text.to_ascii_lowercase();
        terms.iter().all(|term| normalized.contains(term))
    }

    pub(in super::super) fn agent_sidebar_project_matches_terms(
        project: &AgentProject,
        terms: &[String],
    ) -> bool {
        Self::agent_sidebar_query_matches_text(
            format!("{} {}", project.name, project.root_path).as_str(),
            terms,
        )
    }

    pub(in super::super) fn agent_sidebar_thread_matches_terms(
        &self,
        thread: &AgentThread,
        terms: &[String],
    ) -> bool {
        if terms.is_empty() {
            return true;
        }

        let mut haystack = vec![
            self.agent_thread_display_title(thread),
            thread.title.clone(),
            thread.agent.title().to_string(),
            thread.agent.keywords().to_string(),
            thread.launch_command.clone(),
            thread.working_dir.clone(),
        ];
        if let Some(custom_title) = thread.custom_title.as_deref() {
            haystack.push(custom_title.to_string());
        }
        if let Some(title) = thread.last_seen_title.as_deref() {
            haystack.push(title.to_string());
        }
        if let Some(command) = thread.last_seen_command.as_deref() {
            haystack.push(command.to_string());
        }
        if let Some(label) = thread.last_status_label.as_deref() {
            haystack.push(label.to_string());
        }
        if let Some(detail) = thread.last_status_detail.as_deref() {
            haystack.push(detail.to_string());
        }

        Self::agent_sidebar_query_matches_text(haystack.join("\n").as_str(), terms)
    }

    pub(in super::super) fn agent_sidebar_thread_matches_filter(
        &self,
        _project: &AgentProject,
        _thread: &AgentThread,
    ) -> bool {
        true
    }

    pub(in super::super) fn filtered_agent_projects_for_sidebar(
        &self,
    ) -> Vec<(&AgentProject, Vec<&AgentThread>)> {
        let terms = self.agent_sidebar_search_terms();

        self.sorted_agent_projects()
            .into_iter()
            .filter_map(|project| {
                let project_matches = Self::agent_sidebar_project_matches_terms(project, &terms);
                let mut threads = self.sorted_agent_threads_for_project(project.id.as_str());

                threads.retain(|thread| self.agent_sidebar_thread_matches_filter(project, thread));

                if !terms.is_empty() && !project_matches {
                    threads
                        .retain(|thread| self.agent_sidebar_thread_matches_terms(thread, &terms));
                }

                if !terms.is_empty() && !project_matches && threads.is_empty() {
                    return None;
                }

                Some((project, threads))
            })
            .collect()
    }

    pub(in super::super) fn open_first_matching_agent_thread(&mut self, cx: &mut Context<Self>) {
        let Some(thread_id) = self
            .filtered_agent_projects_for_sidebar()
            .into_iter()
            .flat_map(|(_, threads)| threads.into_iter())
            .map(|thread| thread.id.clone())
            .next()
        else {
            return;
        };

        let linked_tab_id = self
            .agent_threads
            .iter()
            .find(|thread| thread.id == thread_id)
            .and_then(|thread| thread.linked_tab_id);

        if let Some(tab_index) = linked_tab_id.and_then(|tab_id| self.tab_index_by_id(tab_id)) {
            self.switch_tab(tab_index, cx);
        } else if let Err(error) = self.resume_saved_agent_thread(thread_id.as_str(), cx) {
            termy_toast::error(error);
            self.notify_overlay(cx);
            return;
        }

        self.agent_sidebar_search_active = false;
        cx.notify();
    }
}
