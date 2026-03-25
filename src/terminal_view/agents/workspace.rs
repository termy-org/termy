use super::*;

impl TerminalView {
    pub(in super::super) fn persisted_agent_workspace_db_path() -> Result<PathBuf, String> {
        let config_path = crate::config::ensure_config_file().map_err(|error| error.to_string())?;
        let parent = config_path
            .parent()
            .ok_or_else(|| format!("Invalid config path '{}'", config_path.display()))?;
        Ok(parent.join(AGENT_WORKSPACE_DB_FILE))
    }

    pub(in super::super) fn legacy_agent_workspace_json_path() -> Result<PathBuf, String> {
        let db_path = Self::persisted_agent_workspace_db_path()?;
        let parent = db_path
            .parent()
            .ok_or_else(|| format!("Invalid agent workspace path '{}'", db_path.display()))?;
        Ok(parent.join(LEGACY_AGENT_WORKSPACE_STATE_FILE))
    }

    pub(in super::super) fn load_persisted_agent_workspace_state()
    -> Result<PersistedAgentWorkspaceState, String> {
        let db_path = Self::persisted_agent_workspace_db_path()?;
        let legacy_path = Self::legacy_agent_workspace_json_path()?;
        let db = AgentWorkspaceDb::open(&db_path)?;
        load_or_migrate_agent_workspace_state(&db, &legacy_path)
    }

    pub(in super::super) fn store_persisted_agent_workspace_state(&self) -> Result<(), String> {
        let path = Self::persisted_agent_workspace_db_path()?;
        let state = PersistedAgentWorkspaceState {
            version: AGENT_WORKSPACE_SCHEMA_VERSION,
            sidebar_open: self.agent_sidebar_open,
            active_project_id: self.active_agent_project_id.clone(),
            collapsed_project_ids: {
                let mut ids = self
                    .collapsed_agent_project_ids
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>();
                ids.sort();
                ids
            },
            projects: self.agent_projects.clone(),
            threads: self
                .agent_threads
                .iter()
                .cloned()
                .map(|mut thread| {
                    thread.linked_tab_id = None;
                    thread
                })
                .collect(),
        };
        let db = AgentWorkspaceDb::open(&path)?;
        store_agent_workspace_state_to_db(&db, &state)
    }

    pub(in super::super) fn restore_persisted_agent_workspace(&mut self) {
        match Self::load_persisted_agent_workspace_state() {
            Ok(state) => {
                self.agent_projects = state.projects;
                self.agent_threads = state.threads;
                self.collapsed_agent_project_ids = state
                    .collapsed_project_ids
                    .into_iter()
                    .filter(|project_id| {
                        self.agent_projects
                            .iter()
                            .any(|project| project.id == *project_id)
                    })
                    .collect();
                self.active_agent_project_id = state.active_project_id.filter(|project_id| {
                    self.agent_projects
                        .iter()
                        .any(|project| &project.id == project_id)
                });
                self.agent_sidebar_open =
                    if self.agent_projects.is_empty() && self.agent_threads.is_empty() {
                        self.agent_sidebar_enabled
                    } else {
                        state.sidebar_open
                    };
                if self.active_agent_project_id.is_none() {
                    self.active_agent_project_id = self
                        .sorted_agent_projects()
                        .first()
                        .map(|project| project.id.clone());
                }
            }
            Err(error) => {
                log::error!("Failed to restore agent workspace: {}", error);
                self.agent_sidebar_open = self.agent_sidebar_enabled;
            }
        }
    }

    pub(in super::super) fn sync_persisted_agent_workspace(&self) {
        if let Err(error) = self.store_persisted_agent_workspace_state() {
            log::error!("Failed to persist agent workspace: {}", error);
        }
    }

    pub(in super::super) fn toggle_agent_sidebar(&mut self, cx: &mut Context<Self>) {
        if !self.agent_sidebar_enabled {
            termy_toast::info(
                "Enable agent_sidebar_enabled in config.txt to use the agent workspace",
            );
            self.notify_overlay(cx);
            return;
        }

        self.agent_sidebar_open = !self.agent_sidebar_open;
        if !self.agent_sidebar_open {
            self.agent_sidebar_search_active = false;
            self.cancel_rename_agent_project(cx);
            self.cancel_rename_agent_thread(cx);
            self.hovered_agent_thread_id = None;
            self.close_agent_git_panel();
        }
        self.sync_persisted_agent_workspace();
        cx.notify();
    }

    pub(in super::super) fn normalized_agent_working_dir(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        self.preferred_working_dir_for_new_agent_session(cx)
            .or_else(|| {
                resolve_launch_working_directory(
                    self.configured_working_dir.as_deref(),
                    self.terminal_runtime.working_dir_fallback,
                )
                .map(|path| path.to_string_lossy().into_owned())
            })
            .or_else(|| Self::user_home_dir().map(|path| path.to_string_lossy().into_owned()))
    }

    pub(in super::super) fn preferred_working_dir_for_new_agent_session(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        let active_tab = self.active_tab;
        let prompt_cwd = self
            .tabs
            .get(active_tab)
            .and_then(|tab| tab.last_prompt_cwd.clone());
        let process_cwd = self
            .tabs
            .get(active_tab)
            .and_then(TerminalTab::active_terminal)
            .and_then(Terminal::child_pid)
            .and_then(|pid| self.cached_or_queued_working_dir_for_child_pid(pid, cx));
        let title_cwd = self
            .tabs
            .get(active_tab)
            .and_then(|tab| {
                [
                    tab.explicit_title.as_deref(),
                    tab.shell_title.as_deref(),
                    Some(tab.title.as_str()),
                ]
                .into_iter()
                .flatten()
                .find_map(Self::working_dir_title_candidate)
            })
            .map(|candidate| candidate.to_string());

        Self::resolve_preferred_working_directory(
            None,
            prompt_cwd.as_deref(),
            process_cwd.as_deref(),
            title_cwd.as_deref(),
            self.configured_working_dir.as_deref(),
            self.terminal_runtime.working_dir_fallback,
        )
    }

    pub(in super::super) fn agent_project_name_for_path(path: &str) -> String {
        let path_obj = Path::new(path);
        path_obj
            .file_name()
            .and_then(|segment| segment.to_str())
            .map(str::trim)
            .filter(|segment| !segment.is_empty())
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| Self::display_working_directory_for_prompt(path_obj))
    }

    pub(in super::super) fn ensure_agent_project(&mut self, root_path: &str) -> String {
        let normalized = normalize_working_directory_candidate(Some(root_path))
            .unwrap_or_else(|| root_path.trim().to_string());
        let now = now_unix_ms();

        if let Some(project) = self
            .agent_projects
            .iter_mut()
            .find(|project| project.root_path == normalized)
        {
            project.updated_at_ms = now;
            return project.id.clone();
        }

        let project_id = next_agent_entity_id("project");
        self.agent_projects.push(AgentProject {
            id: project_id.clone(),
            name: Self::agent_project_name_for_path(&normalized),
            root_path: normalized,
            pinned: false,
            created_at_ms: now,
            updated_at_ms: now,
        });
        project_id
    }

    pub(in super::super) fn touch_agent_project(&mut self, project_id: &str) -> Option<String> {
        let project = self
            .agent_projects
            .iter_mut()
            .find(|project| project.id == project_id)?;
        project.updated_at_ms = now_unix_ms();
        Some(project.root_path.clone())
    }

    pub(in super::super) fn set_agent_project_pinned(
        &mut self,
        project_id: &str,
        pinned: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(project) = self
            .agent_projects
            .iter_mut()
            .find(|project| project.id == project_id)
        else {
            return false;
        };

        if project.pinned == pinned {
            return false;
        }

        project.pinned = pinned;
        self.sync_persisted_agent_workspace();
        cx.notify();
        true
    }

    pub(in super::super) fn set_agent_thread_pinned(
        &mut self,
        thread_id: &str,
        pinned: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(thread) = self
            .agent_threads
            .iter_mut()
            .find(|thread| thread.id == thread_id)
        else {
            return false;
        };

        if thread.pinned == pinned {
            return false;
        }

        thread.pinned = pinned;
        thread.updated_at_ms = now_unix_ms();
        self.sync_persisted_agent_workspace();
        cx.notify();
        true
    }

    pub(in super::super) fn set_agent_sidebar_filter(
        &mut self,
        filter: AgentSidebarFilter,
        cx: &mut Context<Self>,
    ) {
        if self.agent_sidebar_filter == filter {
            return;
        }

        self.agent_sidebar_filter = filter;
        cx.notify();
    }

    pub(in super::super) fn are_all_agent_projects_collapsed(&self) -> bool {
        !self.agent_projects.is_empty()
            && self.collapsed_agent_project_ids.len() >= self.agent_projects.len()
    }

    pub(in super::super) fn set_all_agent_projects_collapsed(
        &mut self,
        collapsed: bool,
        cx: &mut Context<Self>,
    ) {
        if collapsed {
            self.collapsed_agent_project_ids = self
                .agent_projects
                .iter()
                .map(|project| project.id.clone())
                .collect();
            self.cancel_rename_agent_project(cx);
            self.cancel_rename_agent_thread(cx);
        } else {
            self.collapsed_agent_project_ids.clear();
        }

        self.sync_persisted_agent_workspace();
        cx.notify();
    }

    pub(in super::super) fn begin_rename_agent_project(
        &mut self,
        project_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(initial_name) = self
            .agent_projects
            .iter()
            .find(|project| project.id == project_id)
            .map(|project| project.name.clone())
        else {
            return;
        };

        if self.is_command_palette_open() {
            self.close_command_palette(cx);
        }
        if self.search_open {
            self.close_search(cx);
        }
        if self.renaming_tab.is_some() {
            self.cancel_rename_tab(cx);
        }
        if self.renaming_agent_project_id.is_some() {
            self.cancel_rename_agent_project(cx);
        }
        if self.renaming_agent_thread_id.is_some() {
            self.cancel_rename_agent_thread(cx);
        }
        self.agent_sidebar_search_active = false;
        self.active_agent_project_id = Some(project_id.to_string());
        self.collapsed_agent_project_ids.remove(project_id);
        self.renaming_agent_project_id = Some(project_id.to_string());
        self.agent_project_rename_input.set_text(initial_name);
        self.reset_cursor_blink_phase();
        self.inline_input_selecting = false;
        cx.notify();
    }

    pub(in super::super) fn commit_rename_agent_project(&mut self, cx: &mut Context<Self>) {
        let Some(project_id) = self.renaming_agent_project_id.clone() else {
            return;
        };
        let Some(project) = self
            .agent_projects
            .iter_mut()
            .find(|project| project.id == project_id)
        else {
            self.cancel_rename_agent_project(cx);
            return;
        };

        let trimmed = self.agent_project_rename_input.text().trim();
        project.name = if trimmed.is_empty() {
            Self::agent_project_name_for_path(&project.root_path)
        } else {
            Self::truncate_tab_title(trimmed)
        };
        project.updated_at_ms = now_unix_ms();
        self.sync_persisted_agent_workspace();
        self.cancel_rename_agent_project(cx);
    }

    pub(in super::super) fn cancel_rename_agent_project(&mut self, cx: &mut Context<Self>) {
        if self.renaming_agent_project_id.take().is_some()
            || !self.agent_project_rename_input.text().is_empty()
        {
            self.agent_project_rename_input.clear();
            self.inline_input_selecting = false;
            cx.notify();
        }
    }

    pub(in super::super) fn create_agent_thread_for_active_tab(
        &mut self,
        agent: command_palette::AiAgentPreset,
        project_id: String,
        working_dir: &str,
    ) -> Option<String> {
        let now = now_unix_ms();
        let thread_id = next_agent_entity_id("thread");
        let thread_title = format!(
            "{} {}",
            agent.title(),
            Self::agent_project_name_for_path(working_dir)
        );
        let tab_id = self.tabs.get(self.active_tab)?.id;

        self.tabs.get_mut(self.active_tab)?.agent_thread_id = Some(thread_id.clone());
        self.agent_threads.push(AgentThread {
            id: thread_id.clone(),
            project_id: project_id.clone(),
            agent,
            title: thread_title.clone(),
            custom_title: None,
            pinned: false,
            launch_command: agent.launch_command().to_string(),
            working_dir: working_dir.to_string(),
            last_seen_title: Some(thread_title),
            last_seen_command: Some(agent.launch_command().to_string()),
            last_status_label: None,
            last_status_detail: None,
            created_at_ms: now,
            updated_at_ms: now,
            linked_tab_id: Some(tab_id),
        });
        self.active_agent_project_id = Some(project_id);
        if self.agent_sidebar_enabled {
            self.agent_sidebar_open = true;
        }
        Some(thread_id)
    }

    pub(in super::super) fn resume_saved_agent_thread(
        &mut self,
        thread_id: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(thread_index) = self
            .agent_threads
            .iter()
            .position(|thread| thread.id == thread_id)
        else {
            return Err("Agent thread no longer exists".to_string());
        };

        if let Some(tab_index) = self.agent_threads[thread_index]
            .linked_tab_id
            .and_then(|tab_id| self.tab_index_by_id(tab_id))
        {
            self.switch_tab(tab_index, cx);
            return Ok(());
        }

        let command = self.agent_threads[thread_index].launch_command.clone();
        let working_dir = self.agent_threads[thread_index].working_dir.clone();
        let project_id = self.agent_threads[thread_index].project_id.clone();
        let previous_tab_count = self.tabs.len();
        self.add_tab_with_working_dir(Some(working_dir.as_str()), cx);

        if self.tabs.len() == previous_tab_count {
            return Err("Failed to create a tab for the saved thread".to_string());
        }

        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return Err("Failed to access the new agent tab".to_string());
        };
        let Some(terminal) = tab.active_terminal() else {
            return Err("Failed to access the new agent terminal".to_string());
        };

        let mut command_input = command.clone();
        if !command_input.ends_with('\n') {
            command_input.push('\n');
        }
        terminal.write_input(command_input.as_bytes());

        let now = now_unix_ms();
        let tab_id = tab.id;
        tab.agent_thread_id = Some(thread_id.to_string());
        let thread = &mut self.agent_threads[thread_index];
        thread.linked_tab_id = Some(tab_id);
        thread.updated_at_ms = now;
        thread.last_seen_command = Some(command);
        self.active_agent_project_id = Some(project_id);
        self.sync_persisted_agent_workspace();
        cx.notify();
        Ok(())
    }

    pub(in super::super) fn launch_ai_agent_from_palette(
        &mut self,
        agent: command_palette::AiAgentPreset,
        project_id: Option<&str>,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (project_id, working_dir) = match project_id {
            Some(project_id) => {
                let working_dir = self
                    .touch_agent_project(project_id)
                    .ok_or_else(|| "The selected project no longer exists".to_string())?;
                (project_id.to_string(), working_dir)
            }
            None => {
                let working_dir = self.normalized_agent_working_dir(cx).ok_or_else(|| {
                    "Could not resolve a working directory for the agent".to_string()
                })?;
                let project_id = self.ensure_agent_project(&working_dir);
                (project_id, working_dir)
            }
        };
        let previous_tab_count = self.tabs.len();
        self.add_tab_with_working_dir(Some(working_dir.as_str()), cx);

        if self.tabs.len() == previous_tab_count {
            return Err("Failed to create a tab for the agent".to_string());
        }

        let Some(tab) = self.tabs.get_mut(self.active_tab) else {
            return Err("Failed to access the new agent tab".to_string());
        };
        let Some(terminal) = tab.active_terminal() else {
            return Err("Failed to access the new agent terminal".to_string());
        };

        let mut command_input = agent.launch_command().to_string();
        if !command_input.ends_with('\n') {
            command_input.push('\n');
        }
        terminal.write_input(command_input.as_bytes());

        self.create_agent_thread_for_active_tab(agent, project_id, &working_dir)
            .ok_or_else(|| "Failed to link the new agent thread".to_string())?;
        self.sync_persisted_agent_workspace();
        cx.notify();
        Ok(())
    }

    pub(in super::super) fn detach_agent_thread_from_live_tab(&mut self, thread_id: &str) {
        let Some(thread_index) = self
            .agent_threads
            .iter()
            .position(|thread| thread.id == thread_id)
        else {
            return;
        };
        let Some(tab_id) = self.agent_threads[thread_index].linked_tab_id else {
            return;
        };

        let mut clear_thread_link = false;
        match self.tab_index_by_id(tab_id) {
            Some(tab_index) => {
                if let Some(tab) = self.tabs.get_mut(tab_index)
                    && tab.agent_thread_id.as_deref() == Some(thread_id)
                {
                    tab.agent_thread_id = None;
                    clear_thread_link = true;
                }
            }
            None => {
                clear_thread_link = true;
            }
        }

        if clear_thread_link && let Some(thread) = self.agent_threads.get_mut(thread_index) {
            thread.linked_tab_id = None;
        }
    }

    pub(in super::super) fn delete_agent_thread(&mut self, thread_id: &str) -> Result<(), String> {
        let Some(thread_index) = self
            .agent_threads
            .iter()
            .position(|thread| thread.id == thread_id)
        else {
            return Err("Agent thread no longer exists".to_string());
        };

        let project_id = self.agent_threads[thread_index].project_id.clone();
        self.detach_agent_thread_from_live_tab(thread_id);
        self.agent_threads.remove(thread_index);
        if let Some(project) = self
            .agent_projects
            .iter_mut()
            .find(|project| project.id == project_id)
        {
            project.updated_at_ms = now_unix_ms();
        }

        self.sync_persisted_agent_workspace();
        Ok(())
    }

    pub(in super::super) fn delete_agent_project(
        &mut self,
        project_id: &str,
    ) -> Result<usize, String> {
        let Some(project_index) = self
            .agent_projects
            .iter()
            .position(|project| project.id == project_id)
        else {
            return Err("Agent project no longer exists".to_string());
        };

        let thread_ids = self
            .agent_threads
            .iter()
            .filter(|thread| thread.project_id == project_id)
            .map(|thread| thread.id.clone())
            .collect::<Vec<_>>();
        for thread_id in &thread_ids {
            self.detach_agent_thread_from_live_tab(thread_id);
        }

        let removed_threads = thread_ids.len();
        self.agent_threads
            .retain(|thread| thread.project_id != project_id);
        self.agent_projects.remove(project_index);
        self.collapsed_agent_project_ids.remove(project_id);

        if self.active_agent_project_id.as_deref() == Some(project_id) {
            self.active_agent_project_id = self
                .sorted_agent_projects()
                .first()
                .map(|project| project.id.clone());
        }

        self.sync_persisted_agent_workspace();
        Ok(removed_threads)
    }

    pub(in super::super) fn agent_thread_archive_snapshot_for_tab(
        &self,
        index: usize,
    ) -> Option<(
        Option<String>,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> {
        let tab = self.tabs.get(index)?;
        let (status_label, status_detail) = tab
            .agent_thread_id
            .as_deref()
            .and_then(|thread_id| {
                self.agent_threads
                    .iter()
                    .find(|thread| thread.id == thread_id)
                    .map(|thread| self.agent_thread_status_presentation(thread))
            })
            .map(|status| (Some(status.label), status.detail))
            .unwrap_or_default();

        Some((
            tab.agent_thread_id.clone(),
            tab.title.clone(),
            tab.current_command.clone(),
            status_label,
            status_detail,
        ))
    }

    pub(in super::super) fn archive_agent_thread_snapshot(
        &mut self,
        thread_id: Option<&str>,
        title: &str,
        current_command: Option<&str>,
        status_label: Option<&str>,
        status_detail: Option<&str>,
    ) {
        let Some(thread_id) = thread_id else {
            return;
        };
        let Some(thread) = self
            .agent_threads
            .iter_mut()
            .find(|thread| thread.id == thread_id)
        else {
            return;
        };

        thread.linked_tab_id = None;
        thread.last_seen_title = Some(title.to_string());
        thread.last_seen_command = current_command.map(ToOwned::to_owned);
        thread.last_status_label = status_label.map(ToOwned::to_owned);
        thread.last_status_detail = status_detail.map(ToOwned::to_owned);
        thread.updated_at_ms = now_unix_ms();
        if let Some(project) = self
            .agent_projects
            .iter_mut()
            .find(|project| project.id == thread.project_id)
        {
            project.updated_at_ms = thread.updated_at_ms;
        }
        self.sync_persisted_agent_workspace();
    }

    pub(in super::super) fn sync_agent_workspace_to_active_tab(&mut self) {
        let Some(project_id) = self
            .tabs
            .get(self.active_tab)
            .and_then(|tab| tab.agent_thread_id.as_deref())
            .and_then(|thread_id| {
                self.agent_threads
                    .iter()
                    .find(|thread| thread.id == thread_id)
                    .map(|thread| thread.project_id.clone())
            })
        else {
            return;
        };

        self.active_agent_project_id = Some(project_id);
    }

    pub(in super::super) fn begin_agent_sidebar_search(&mut self, cx: &mut Context<Self>) {
        if self.is_command_palette_open() {
            self.close_command_palette(cx);
        }
        if self.search_open {
            self.close_search(cx);
        }
        if self.renaming_tab.is_some() {
            self.cancel_rename_tab(cx);
        }
        if self.renaming_agent_thread_id.is_some() {
            self.cancel_rename_agent_thread(cx);
        }

        self.agent_sidebar_search_active = true;
        self.reset_cursor_blink_phase();
        self.inline_input_selecting = false;
        cx.notify();
    }

    pub(in super::super) fn dismiss_agent_sidebar_search(&mut self, cx: &mut Context<Self>) {
        let had_query = !self.agent_sidebar_search_input.text().is_empty();
        let was_active = self.agent_sidebar_search_active;
        if !had_query && !was_active {
            return;
        }

        if had_query {
            self.agent_sidebar_search_input.clear();
        } else {
            self.agent_sidebar_search_active = false;
        }
        self.inline_input_selecting = false;
        cx.notify();
    }
}
